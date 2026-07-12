from __future__ import annotations

from collections import defaultdict
from typing import Any, Iterable

try:
    from script.book_validation import registered_legal_move_count
    from script.corpus_priority import recency_bucket
except ImportError:
    from book_validation import registered_legal_move_count
    from corpus_priority import recency_bucket


def _metric(covered: int, total: int) -> dict[str, int | float]:
    return {"covered": covered, "total": total, "rate": covered / total if total else 0.0}


def _ply_band(ply: int) -> str:
    lower = (ply // 20) * 20
    return f"{lower + 1}-{lower + 20}"


def generate_coverage_report(
    store: Any,
    book: Any,
    *,
    snapshot_id: str,
    book_hash: str,
) -> dict[str, Any]:
    """Generate reproducible position and occurrence coverage from accepted games only."""
    rows = store.connection.execute(
        """SELECT site, event, year, ply, position_key, sfen
           FROM game_position ORDER BY game_id, ply"""
    ).fetchall()
    covered_cache: dict[str, bool] = {}
    for row in rows:
        key = str(row["position_key"])
        position = book.positions.get(book.position_key(key))
        covered_cache[key] = bool(
            position is not None
            and registered_legal_move_count(position.sfen, position.entries) > 0
        )

    def summarize(items: Iterable[Any]) -> dict[str, dict[str, int | float]]:
        selected = list(items)
        unique_keys = {str(row["position_key"]) for row in selected}
        covered_unique = sum(1 for key in unique_keys if covered_cache.get(key, False))
        covered_occurrences = sum(
            1 for row in selected if covered_cache.get(str(row["position_key"]), False)
        )
        return {
            "unique": _metric(covered_unique, len(unique_keys)),
            "occurrences": _metric(covered_occurrences, len(selected)),
        }

    groups: dict[str, dict[str, list[Any]]] = {
        "by_site": defaultdict(list),
        "by_event": defaultdict(list),
        "by_bucket": defaultdict(list),
        "by_ply_band": defaultdict(list),
        "by_side": defaultdict(list),
    }
    for row in rows:
        groups["by_site"][str(row["site"])].append(row)
        groups["by_event"][str(row["event"])].append(row)
        groups["by_bucket"][str(recency_bucket(int(row["year"])))].append(row)
        groups["by_ply_band"][_ply_band(int(row["ply"]))].append(row)
        groups["by_side"]["black" if int(row["ply"]) % 2 == 0 else "white"].append(row)

    report: dict[str, Any] = {
        "snapshot_id": snapshot_id,
        "book_hash": book_hash,
        "corpus_revision": store.corpus_revision(),
        "overall": summarize(rows),
    }
    for group_name, values in groups.items():
        report[group_name] = {key: summarize(items) for key, items in sorted(values.items())}
    return report