from __future__ import annotations

import dataclasses
import pathlib
import tomllib
from typing import Any, Mapping


class ConfigError(ValueError):
    """Raised when an extension configuration is internally inconsistent."""


@dataclasses.dataclass(frozen=True)
class WorkerConfig:
    engine_count: int
    threads_per_engine: int
    vulnerability_black: int
    vulnerability_white: int
    general: int

    def validate(self) -> None:
        values = {
            "engine_count": self.engine_count,
            "threads_per_engine": self.threads_per_engine,
            "vulnerability_black": self.vulnerability_black,
            "vulnerability_white": self.vulnerability_white,
            "general": self.general,
        }
        for name, value in values.items():
            minimum = 1 if name in {"engine_count", "threads_per_engine"} else 0
            if value < minimum:
                raise ConfigError(f"{name} must be at least {minimum}")
        assigned = self.vulnerability_black + self.vulnerability_white + self.general
        if assigned != self.engine_count:
            raise ConfigError(
                "engine_count must equal vulnerability_black + "
                f"vulnerability_white + general ({assigned})"
            )

    def roles(self) -> tuple[str, ...]:
        self.validate()
        return (
            ("vulnerability_black",) * self.vulnerability_black
            + ("vulnerability_white",) * self.vulnerability_white
            + ("general",) * self.general
        )


@dataclasses.dataclass(frozen=True)
class CorpusConfig:
    enabled: bool
    max_concurrent_searches: int
    general_pool_node_share: float
    saturation_window: int = 100
    wcsc_weight: int = 40
    denryu_weight: int = 40
    floodgate_weight: int = 20
    rating_medium_games: int = 15
    rating_high_games: int = 50
    rating_min_component_size: int = 10

    def validate(self, *, general_workers: int) -> None:
        if not 0.0 <= self.general_pool_node_share <= 1.0:
            raise ConfigError("general_pool_node_share must be between 0 and 1")
        if self.saturation_window < 1:
            raise ConfigError("saturation_window must be positive")
        if min(self.wcsc_weight, self.denryu_weight, self.floodgate_weight) < 0:
            raise ConfigError("site weights must be non-negative")
        if self.wcsc_weight + self.denryu_weight + self.floodgate_weight <= 0:
            raise ConfigError("at least one site weight must be positive")
        if not (0 < self.rating_medium_games < self.rating_high_games):
            raise ConfigError("rating game thresholds must be increasing")
        if self.rating_min_component_size < 1:
            raise ConfigError("rating_min_component_size must be positive")
        if self.max_concurrent_searches < 0:
            raise ConfigError("max_concurrent_searches must be non-negative")
        if self.enabled:
            if general_workers < 1:
                raise ConfigError("corpus requires at least one general worker")
            if not 1 <= self.max_concurrent_searches <= general_workers:
                raise ConfigError(
                    "max_concurrent_searches must be between 1 and the number "
                    "of general workers"
                )
        elif self.max_concurrent_searches != 0:
            raise ConfigError("disabled corpus must use max_concurrent_searches = 0")


@dataclasses.dataclass(frozen=True)
class RuntimeConfig:
    state_dir: pathlib.Path
    save_interval_sec: float
    backup_count: int
    heartbeat_timeout_sec: float
    usi_stop_timeout_sec: float

    def validate(self) -> None:
        if self.save_interval_sec <= 0:
            raise ConfigError("save_interval_sec must be positive")
        if self.backup_count < 0:
            raise ConfigError("backup_count must be non-negative")
        if self.heartbeat_timeout_sec <= 0:
            raise ConfigError("heartbeat_timeout_sec must be positive")
        if self.usi_stop_timeout_sec <= 0:
            raise ConfigError("usi_stop_timeout_sec must be positive")


@dataclasses.dataclass(frozen=True)
class ExtensionConfig:
    workers: WorkerConfig
    corpus: CorpusConfig
    runtime: RuntimeConfig

    def validate(self) -> None:
        self.workers.validate()
        self.corpus.validate(general_workers=self.workers.general)
        self.runtime.validate()

    @classmethod
    def production_defaults(cls, state_dir: pathlib.Path) -> "ExtensionConfig":
        config = cls(
            workers=WorkerConfig(24, 1, 8, 8, 8),
            corpus=CorpusConfig(True, 2, 0.25),
            runtime=RuntimeConfig(state_dir, 3600.0, 3, 10.0, 60.0),
        )
        config.validate()
        return config

    @classmethod
    def pilot_defaults(cls, state_dir: pathlib.Path) -> "ExtensionConfig":
        config = cls(
            workers=WorkerConfig(8, 1, 2, 2, 4),
            corpus=CorpusConfig(True, 1, 0.25),
            runtime=RuntimeConfig(state_dir, 3600.0, 3, 10.0, 60.0),
        )
        config.validate()
        return config


def _section(document: Mapping[str, Any], name: str) -> Mapping[str, Any]:
    value = document.get(name)
    if not isinstance(value, dict):
        raise ConfigError(f"missing TOML section [{name}]")
    return value


def load_extension_config(path: pathlib.Path) -> ExtensionConfig:
    """Load and validate a portable extension TOML configuration."""
    config_path = path.resolve()
    with config_path.open("rb") as stream:
        document = tomllib.load(stream)

    workers_data = _section(document, "workers")
    corpus_data = _section(document, "corpus")
    runtime_data = _section(document, "runtime")
    state_dir = pathlib.Path(str(runtime_data["state_dir"]))
    if not state_dir.is_absolute():
        state_dir = (config_path.parent / state_dir).resolve()

    config = ExtensionConfig(
        workers=WorkerConfig(
            engine_count=int(workers_data["engine_count"]),
            threads_per_engine=int(workers_data["threads_per_engine"]),
            vulnerability_black=int(workers_data["vulnerability_black"]),
            vulnerability_white=int(workers_data["vulnerability_white"]),
            general=int(workers_data["general"]),
        ),
        corpus=CorpusConfig(
            enabled=bool(corpus_data["enabled"]),
            max_concurrent_searches=int(corpus_data["max_concurrent_searches"]),
            general_pool_node_share=float(corpus_data["general_pool_node_share"]),
            saturation_window=int(corpus_data.get("saturation_window", 100)),
            wcsc_weight=int(corpus_data.get("wcsc_weight", 40)),
            denryu_weight=int(corpus_data.get("denryu_weight", 40)),
            floodgate_weight=int(corpus_data.get("floodgate_weight", 20)),
            rating_medium_games=int(corpus_data.get("rating_medium_games", 15)),
            rating_high_games=int(corpus_data.get("rating_high_games", 50)),
            rating_min_component_size=int(corpus_data.get("rating_min_component_size", 10)),
        ),
        runtime=RuntimeConfig(
            state_dir=state_dir,
            save_interval_sec=float(runtime_data["save_interval_sec"]),
            backup_count=int(runtime_data["backup_count"]),
            heartbeat_timeout_sec=float(runtime_data["heartbeat_timeout_sec"]),
            usi_stop_timeout_sec=float(runtime_data["usi_stop_timeout_sec"]),
        ),
    )
    config.validate()
    return config