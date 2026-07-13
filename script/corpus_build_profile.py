from __future__ import annotations

import dataclasses
import json
import pathlib
from typing import Any


class ProfileError(ValueError):
    pass


@dataclasses.dataclass(frozen=True)
class CorpusIngestSpec:
    site: str
    event: str
    year: int
    retrieved_at: float
    inputs: tuple[pathlib.Path, ...]


@dataclasses.dataclass(frozen=True)
class CorpusBuildProfile:
    schema_version: int
    name: str
    source_path: pathlib.Path
    manifest: pathlib.Path
    ingests: tuple[CorpusIngestSpec, ...]
    ranking_files: tuple[pathlib.Path, ...]
    rating_file: pathlib.Path | None
    rating_config: pathlib.Path | None
    input_book: pathlib.Path | None
    coverage_snapshot_id: str


_PROFILE_KEYS = {
    "schema_version",
    "name",
    "manifest",
    "ingests",
    "ranking_files",
    "rating_file",
    "rating_config",
    "input_book",
    "coverage_snapshot_id",
}
_INGEST_KEYS = {"site", "event", "year", "retrieved_at", "inputs"}
_SITES = {"floodgate", "wcsc", "denryu"}


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProfileError(f"{label} must be an object")
    return value


def _required(document: dict[str, Any], keys: set[str], label: str) -> None:
    missing = keys - document.keys()
    if missing:
        raise ProfileError(f"{label} is missing keys: {sorted(missing)}")
    unknown = document.keys() - keys
    if unknown:
        raise ProfileError(f"unknown {label} keys: {sorted(unknown)}")


def _resolved(base: pathlib.Path, value: Any, label: str) -> pathlib.Path:
    if not isinstance(value, str) or not value:
        raise ProfileError(f"{label} must be a non-empty path")
    path = pathlib.Path(value)
    return path if path.is_absolute() else base / path


def _relative_input(value: Any) -> pathlib.Path:
    if not isinstance(value, str) or not value:
        raise ProfileError("ingest input must be a non-empty relative path")
    path = pathlib.PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts:
        raise ProfileError("ingest input must stay within the download directory")
    return pathlib.Path(*path.parts)


def load_build_profile(path: pathlib.Path) -> CorpusBuildProfile:
    source_path = pathlib.Path(path).resolve()
    try:
        document = _object(json.loads(source_path.read_text(encoding="utf-8")), "profile")
    except (OSError, json.JSONDecodeError) as error:
        raise ProfileError(f"cannot read profile: {error}") from error
    _required(document, _PROFILE_KEYS, "profile")
    if document["schema_version"] != 1:
        raise ProfileError("profile schema_version must be 1")
    if document["name"] not in {"pilot", "production"}:
        raise ProfileError("profile name must be pilot or production")
    base = source_path.parent
    manifest = _resolved(base, document["manifest"], "manifest")
    try:
        manifest_document = _object(
            json.loads(manifest.read_text(encoding="utf-8")), "manifest"
        )
    except (OSError, json.JSONDecodeError) as error:
        raise ProfileError(f"cannot read manifest: {error}") from error
    sources = manifest_document.get("sources")
    if not isinstance(sources, list):
        raise ProfileError("manifest requires a sources list")
    manifest_identities = {
        (source.get("site"), source.get("event"), source.get("year"), source.get("retrieved_at"))
        for source in sources
        if isinstance(source, dict)
    }

    raw_ingests = document["ingests"]
    if not isinstance(raw_ingests, list) or not raw_ingests:
        raise ProfileError("profile ingests must be a non-empty list")
    ingests: list[CorpusIngestSpec] = []
    identities: set[tuple[str, str, int]] = set()
    for index, raw in enumerate(raw_ingests):
        ingest = _object(raw, f"ingest[{index}]")
        _required(ingest, _INGEST_KEYS, f"ingest[{index}]")
        site = ingest["site"]
        event = ingest["event"]
        year = ingest["year"]
        retrieved_at = ingest["retrieved_at"]
        inputs = ingest["inputs"]
        if site not in _SITES or not isinstance(event, str) or not event:
            raise ProfileError(f"invalid ingest identity at index {index}")
        if not isinstance(year, int) or not isinstance(retrieved_at, (int, float)):
            raise ProfileError(f"invalid ingest time at index {index}")
        identity = (site, event, year)
        if identity in identities:
            raise ProfileError(f"duplicate ingest: {identity}")
        identities.add(identity)
        if (site, event, year, retrieved_at) not in manifest_identities:
            raise ProfileError(f"ingest {identity} is not represented in manifest")
        if not isinstance(inputs, list) or not inputs:
            raise ProfileError(f"ingest {identity} requires inputs")
        ingests.append(
            CorpusIngestSpec(
                site=site,
                event=event,
                year=year,
                retrieved_at=float(retrieved_at),
                inputs=tuple(_relative_input(item) for item in inputs),
            )
        )

    ranking = document["ranking_files"]
    if not isinstance(ranking, list):
        raise ProfileError("ranking_files must be a list")
    coverage_snapshot_id = document["coverage_snapshot_id"]
    if not isinstance(coverage_snapshot_id, str) or not coverage_snapshot_id:
        raise ProfileError("coverage_snapshot_id must be a non-empty string")
    rating_file = (
        None
        if document["rating_file"] is None
        else _resolved(base, document["rating_file"], "rating_file")
    )
    rating_config = (
        None
        if document["rating_config"] is None
        else _resolved(base, document["rating_config"], "rating_config")
    )
    if (rating_file is None) != (rating_config is None):
        raise ProfileError("rating_file and rating_config must both be paths or null")
    input_book = (
        None
        if document["input_book"] is None
        else _resolved(base, document["input_book"], "input_book")
    )
    return CorpusBuildProfile(
        schema_version=1,
        name=document["name"],
        source_path=source_path,
        manifest=manifest,
        ingests=tuple(ingests),
        ranking_files=tuple(_resolved(base, item, "ranking file") for item in ranking),
        rating_file=rating_file,
        rating_config=rating_config,
        input_book=input_book,
        coverage_snapshot_id=coverage_snapshot_id,
    )
