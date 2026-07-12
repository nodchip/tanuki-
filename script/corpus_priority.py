from __future__ import annotations

import dataclasses
import threading
from typing import Iterable, Mapping


POLICY_VERSION = "corpus-priority-v1"


def recency_bucket(year: int) -> int:
    """Return fixed calendar buckets 2024-2026, 2027-2029, and so on."""
    if year < 1:
        raise ValueError("year must be positive")
    return (year - 2024) // 3


@dataclasses.dataclass(frozen=True)
class PriorityFacts:
    year: int
    stage_tier: int = 0
    mover_percentile: float = 0.0
    opponent_stage_tier: int = 0
    opponent_percentile: float = 0.0
    rating_reliability: int = 0
    anchor_margin: float = 0.0
    rating_percentile: float = 0.0
    recent_occurrences: int = 0
    occurrences: int = 0
    exact_time: float = 0.0


def encode_priority(values: tuple[float, ...]) -> str:
    """Encode a bounded numeric tuple so SQLite TEXT order preserves tuple order."""
    return "|".join(f"{value + 1_000_000:020.6f}" for value in values)

def priority_tuple(facts: PriorityFacts) -> tuple[float, ...]:
    """Build the explainable lexicographic key used within a site queue."""
    return (
        float(recency_bucket(facts.year)),
        float(facts.stage_tier),
        float(facts.mover_percentile),
        float(facts.opponent_stage_tier),
        float(facts.opponent_percentile),
        float(facts.rating_reliability),
        float(facts.anchor_margin if facts.rating_reliability >= 2 else 0.0),
        float(facts.rating_percentile if facts.rating_reliability >= 2 else 0.0),
        float(facts.recent_occurrences),
        float(facts.occurrences),
        float(facts.exact_time),
    )


@dataclasses.dataclass(frozen=True)
class SiteBudget:
    weights: Mapping[str, int] = dataclasses.field(
        default_factory=lambda: {"wcsc": 40, "denryu": 40, "floodgate": 20}
    )

    def allocate(self, nodes: int, nonempty_sites: Iterable[str]) -> dict[str, int]:
        sites = sorted(set(nonempty_sites))
        if nodes < 0:
            raise ValueError("nodes must be non-negative")
        unknown = set(sites) - set(self.weights)
        if unknown:
            raise ValueError(f"unknown sites: {sorted(unknown)}")
        total_weight = sum(self.weights[site] for site in sites)
        if not sites or total_weight == 0:
            return {}
        raw = {site: nodes * self.weights[site] / total_weight for site in sites}
        allocated = {site: int(raw[site]) for site in sites}
        remainder = nodes - sum(allocated.values())
        order = sorted(sites, key=lambda site: (raw[site] - allocated[site], self.weights[site], site), reverse=True)
        for site in order[:remainder]:
            allocated[site] += 1
        return allocated


class SiteNodeBudget:
    """Thread-safe weighted fair ordering using actual corpus search nodes."""

    def __init__(self, weights: Mapping[str, int] | None = None) -> None:
        self.weights = dict(weights or {"wcsc": 40, "denryu": 40, "floodgate": 20})
        if set(self.weights) != {"wcsc", "denryu", "floodgate"} or min(self.weights.values()) < 0:
            raise ValueError("weights must define three non-negative sites")
        self._order = ("wcsc", "denryu", "floodgate")
        self._nodes = {site: 0 for site in self._order}
        self._lock = threading.Lock()

    def order(self) -> tuple[str, ...]:
        with self._lock:
            return tuple(sorted(
                self._order,
                key=lambda site: (
                    self._nodes[site] / self.weights[site] if self.weights[site] else float("inf"),
                    self._order.index(site),
                ),
            ))

    def record(self, site: str, nodes: int) -> None:
        if site not in self.weights or nodes < 0:
            raise ValueError("invalid site or node count")
        with self._lock:
            self._nodes[site] += nodes

    def snapshot(self) -> dict[str, int]:
        with self._lock:
            return dict(self._nodes)

@dataclasses.dataclass
class ProgressiveWidth:
    width: int = 1
    window_size: int = 100
    rollouts_since_addition: int = 0
    _pending_seen: bool = False

    def observe(self, *, added: bool, pending_or_running: bool) -> bool:
        """Record one corpus root-to-leaf rollout and report an N increment."""
        if added:
            self.rollouts_since_addition = 0
            self._pending_seen = False
            return False
        self.rollouts_since_addition += 1
        self._pending_seen = self._pending_seen or pending_or_running
        if self.rollouts_since_addition < self.window_size:
            return False
        if pending_or_running or self._pending_seen:
            self._pending_seen = pending_or_running
            return False
        self.width += 1
        self.rollouts_since_addition = 0
        return True