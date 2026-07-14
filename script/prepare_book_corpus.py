from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import os
import pathlib
import shutil
import sys
import time
import uuid
from typing import Optional, Sequence

try:
    from script.book_corpus import CorpusStore
    from script.book_extension_config import load_extension_config
    from script.book_extension_runtime import WindowsFileLock
    from script.collect_book_corpus import collect_manifest
    from script.corpus_build_profile import CorpusBuildProfile, load_build_profile
    from script.corpus_collection import iter_csa_records
    from script.corpus_coverage import generate_coverage_report
    from script.corpus_ingest import ingest_csa_text
    from script.corpus_priority import PriorityFacts, encode_priority, priority_tuple
    from script.corpus_rankings import (
        FloodgateRatingResult,
        TournamentResult,
        import_floodgate_rating,
        import_tournament_ranking,
        recompute_candidate_priorities,
    )
    from script.extend_book_mcts import OpeningBook, sha256_file
except ImportError:
    from book_corpus import CorpusStore
    from book_extension_config import load_extension_config
    from book_extension_runtime import WindowsFileLock
    from collect_book_corpus import collect_manifest
    from corpus_build_profile import CorpusBuildProfile, load_build_profile
    from corpus_collection import iter_csa_records
    from corpus_coverage import generate_coverage_report
    from corpus_ingest import ingest_csa_text
    from corpus_priority import PriorityFacts, encode_priority, priority_tuple
    from corpus_rankings import (
        FloodgateRatingResult,
        TournamentResult,
        import_floodgate_rating,
        import_tournament_ranking,
        recompute_candidate_priorities,
    )
    from extend_book_mcts import OpeningBook, sha256_file


class CorpusBuildError(RuntimeError):
    def __init__(self, exit_code: int, phase: str, detail: str) -> None:
        self.exit_code = exit_code
        self.phase = phase
        self.detail = detail
        super().__init__(f"{phase}: {detail}")

@dataclasses.dataclass(frozen=True)
class CorpusBuildSummary:
    status: str
    profile: str
    run_id: str
    accepted: int
    excluded: int
    database_size: int


def _fail_build(
    run_dir: pathlib.Path,
    profile: CorpusBuildProfile,
    run_id: str,
    started_at: float,
    error: CorpusBuildError,
) -> None:
    document = {
        "status": "failed",
        "profile": profile.name,
        "run_id": run_id,
        "started_at": started_at,
        "failed_at": time.time(),
        "phase": error.phase,
        "exit_code": error.exit_code,
        "detail": error.detail[:2000],
    }
    (run_dir / "corpus-build-summary.json").write_text(
        json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    raise error

def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _import_rankings(store: CorpusStore, paths: tuple[pathlib.Path, ...]) -> int:
    imported = 0
    for path in paths:
        document = json.loads(path.read_text(encoding="utf-8"))
        results = [TournamentResult(**item) for item in document["results"]]
        import_tournament_ranking(
            store,
            site=document["site"],
            event=document["event"],
            source_url=document["source_url"],
            source_sha256=document["source_sha256"],
            retrieved_at=float(document["retrieved_at"]),
            provisional=bool(document.get("provisional", False)),
            results=results,
        )
        imported += 1
    return imported


def _import_rating(store: CorpusStore, profile: CorpusBuildProfile) -> int:
    if profile.rating_file is None:
        return 0
    assert profile.rating_config is not None
    document = json.loads(profile.rating_file.read_text(encoding="utf-8"))
    results = [FloodgateRatingResult(**item) for item in document["results"]]
    policy = load_extension_config(profile.rating_config).corpus
    import_floodgate_rating(
        store,
        year=int(document["year"]),
        snapshot_time=float(document["snapshot_time"]),
        source_url=document["source_url"],
        source_sha256=document["source_sha256"],
        anchor_era=document["anchor_era"],
        rated_player_count=int(document["rated_player_count"]),
        component_size=int(document["component_size"]),
        anchor_connected_rate=float(document["anchor_connected_rate"]),
        results=results,
        medium_games=policy.rating_medium_games,
        high_games=policy.rating_high_games,
        min_component_size=policy.rating_min_component_size,
    )
    return 1


def _ingest_sources(
    store: CorpusStore,
    profile: CorpusBuildProfile,
    download_dir: pathlib.Path,
) -> tuple[int, int]:
    accepted = 0
    excluded = 0
    for ingest in sorted(profile.ingests, key=lambda item: (item.site, item.event, item.year)):
        priority = encode_priority(priority_tuple(PriorityFacts(year=ingest.year)))
        for relative_input in ingest.inputs:
            input_path = download_dir / relative_input
            for relative_path, text in iter_csa_records(input_path):
                result = ingest_csa_text(
                    store,
                    text,
                    site=ingest.site,
                    event=ingest.event,
                    year=ingest.year,
                    relative_path=f"{relative_input.as_posix()}/{relative_path}",
                    priority_key=priority,
                    retrieved_at=ingest.retrieved_at,
                )
                if result.accepted:
                    accepted += 1
                else:
                    excluded += 1
    return accepted, excluded

def _validate_database(path: pathlib.Path) -> dict[str, int | str]:
    import sqlite3

    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        integrity = str(connection.execute("PRAGMA integrity_check").fetchone()[0])
        if integrity != "ok":
            raise ValueError(f"SQLite integrity_check failed: {integrity}")
        foreign_key_error = connection.execute("PRAGMA foreign_key_check").fetchone()
        if foreign_key_error is not None:
            raise ValueError(f"SQLite foreign_key_check failed: {tuple(foreign_key_error)}")
        version_row = connection.execute(
            "SELECT value FROM meta WHERE key = 'schema_version'"
        ).fetchone()
        if version_row is None or int(version_row[0]) != 5:
            raise ValueError("SQLite schema version must be 5")
        index = connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?",
            ("candidate_position_priority_idx",),
        ).fetchone()
        if index is None:
            raise ValueError("candidate_position_priority_idx is missing")
        return {
            "integrity_check": integrity,
            "foreign_key_check": "ok",
            "schema_version": 5,
            "raw_sources": int(connection.execute("SELECT COUNT(*) FROM raw_source").fetchone()[0]),
            "logical_games": int(connection.execute("SELECT COUNT(*) FROM logical_game").fetchone()[0]),
            "ingest_errors": int(connection.execute("SELECT COUNT(*) FROM ingest_error").fetchone()[0]),
            "candidates": int(connection.execute("SELECT COUNT(*) FROM candidate").fetchone()[0]),
            "candidate_sources": int(connection.execute("SELECT COUNT(*) FROM game_position gp JOIN source_game sg ON sg.game_id=gp.game_id").fetchone()[0]),
        }
    finally:
        connection.close()

def _publish(source: pathlib.Path, destination: pathlib.Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    os.replace(source, destination)


def _check_free_space(path: pathlib.Path, required_bytes: int) -> int:
    free_bytes = int(shutil.disk_usage(path).free)
    if free_bytes < required_bytes:
        raise CorpusBuildError(
            2,
            "storage",
            f"insufficient free space: required={required_bytes}, available={free_bytes}",
        )
    return free_bytes


def _storage_estimate(
    profile: CorpusBuildProfile, download_dir: pathlib.Path
) -> dict[str, int]:
    document = json.loads(profile.manifest.read_text(encoding="utf-8"))
    sources = document.get("sources", [])
    archive_bytes = sum(int(item.get("size", 0)) for item in sources)
    missing_download_bytes = sum(
        int(item.get("size", 0))
        for item in sources
        if not (download_dir / str(item["relative_path"])).exists()
    )
    estimated_database_bytes = max(512 * 1024**2, archive_bytes * 40)
    safety_bytes = 512 * 1024**2
    return {
        "archive_bytes": archive_bytes,
        "missing_download_bytes": missing_download_bytes,
        "estimated_database_bytes": estimated_database_bytes,
        "safety_bytes": safety_bytes,
        "required_free_bytes": estimated_database_bytes + missing_download_bytes + safety_bytes,
    }


def _publish_database_generation(
    new_database: pathlib.Path,
    active_database: pathlib.Path,
    previous_database: pathlib.Path,
) -> None:
    """Publish a DB while preserving one generation without copying on NTFS."""
    active_database.parent.mkdir(parents=True, exist_ok=True)
    if not active_database.exists():
        os.replace(new_database, active_database)
        return
    previous_temporary = previous_database.with_name(f"{previous_database.name}.tmp")
    if previous_temporary.exists():
        previous_temporary.unlink()
    try:
        os.link(active_database, previous_temporary)
    except OSError:
        shutil.copy2(active_database, previous_temporary)
    try:
        os.replace(new_database, active_database)
        try:
            os.replace(previous_temporary, previous_database)
        except Exception:
            os.replace(active_database, new_database)
            os.replace(previous_temporary, active_database)
            raise
    except Exception:
        if previous_temporary.exists():
            previous_temporary.unlink()
        raise


def _build_corpus_unlocked(
    profile_path: pathlib.Path,
    state_dir: pathlib.Path,
    *,
    input_book: Optional[pathlib.Path] = None,
    now: Optional[float] = None,
) -> CorpusBuildSummary:
    started_at = time.time() if now is None else now
    phase_seconds: dict[str, float] = {}
    profile = load_build_profile(profile_path)
    state_dir = pathlib.Path(state_dir).resolve()
    state_dir.mkdir(parents=True, exist_ok=True)
    run_id = f"{int(started_at)}-{uuid.uuid4().hex[:12]}"
    run_dir = state_dir / "build" / run_id
    run_dir.mkdir(parents=True)
    download_dir = state_dir / "downloads" / profile.name
    storage_estimate = _storage_estimate(profile, download_dir)
    try:
        free_bytes_before = _check_free_space(state_dir, storage_estimate["required_free_bytes"])
    except CorpusBuildError as error:
        _fail_build(run_dir, profile, run_id, started_at, error)
    phase_started = time.perf_counter()
    try:
        collected = collect_manifest(profile.manifest, download_dir)
    except Exception as error:
        _fail_build(
            run_dir,
            profile,
            run_id,
            started_at,
            CorpusBuildError(3, "download", str(error)),
        )
    phase_seconds["download"] = time.perf_counter() - phase_started
    database = run_dir / "corpus.sqlite"
    phase_started = time.perf_counter()
    with CorpusStore(database) as store:
        try:
            accepted, excluded = _ingest_sources(store, profile, download_dir)
        except Exception as error:
            _fail_build(
                run_dir,
                profile,
                run_id,
                started_at,
                CorpusBuildError(4, "ingest", str(error)),
            )
        phase_seconds["ingest"] = time.perf_counter() - phase_started
        phase_started = time.perf_counter()
        if accepted:
            store.bump_corpus_revision()
        try:
            ranking_count = _import_rankings(store, profile.ranking_files)
        except Exception as error:
            _fail_build(
                run_dir,
                profile,
                run_id,
                started_at,
                CorpusBuildError(4, "ranking", str(error)),
            )
        try:
            rating_count = _import_rating(store, profile)
        except Exception as error:
            _fail_build(
                run_dir,
                profile,
                run_id,
                started_at,
                CorpusBuildError(4, "rating", str(error)),
            )
        recompute_candidate_priorities(store)
        phase_seconds["metadata"] = time.perf_counter() - phase_started
        phase_started = time.perf_counter()
        selected_book = (
            pathlib.Path(input_book)
            if input_book is not None
            else (
                profile.input_book
                if profile.input_book is not None
                else state_dir / "input-book.db"
            )
        )
        try:
            book = OpeningBook.load(selected_book, ignore_ply=True)
            coverage = generate_coverage_report(
                store,
                book,
                snapshot_id=profile.coverage_snapshot_id,
                book_hash=sha256_file(selected_book),
            )
        except Exception as error:
            _fail_build(
                run_dir,
                profile,
                run_id,
                started_at,
                CorpusBuildError(5, "coverage", str(error)),
            )
        (run_dir / "coverage-initial.json").write_text(
            json.dumps(coverage, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        phase_seconds["coverage"] = time.perf_counter() - phase_started
        store.connection.execute("ANALYZE")
        store.connection.execute("PRAGMA optimize")
        store.connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")

    phase_started = time.perf_counter()
    try:
        database_checks = _validate_database(database)
    except Exception as error:
        _fail_build(
            run_dir,
            profile,
            run_id,
            started_at,
            CorpusBuildError(5, "database-validation", str(error)),
        )
    phase_seconds["validation"] = time.perf_counter() - phase_started
    shutil.copy2(download_dir / "snapshot.json", run_dir / "snapshot.json")
    summary_document = {
        "status": "success",
        "profile": profile.name,
        "profile_sha256": _sha256(profile.source_path),
        "manifest_sha256": _sha256(profile.manifest),
        "run_id": run_id,
        "started_at": started_at,
        "accepted": accepted,
        "excluded": excluded,
        "ranking_snapshots": ranking_count,
        "rating_snapshots": rating_count,
        "sources": collected,
        "database_size": database.stat().st_size,
        "database_checks": database_checks,
        "coverage": coverage["overall"],
        "phase_seconds": phase_seconds,
        "storage": {**storage_estimate, "free_bytes_before": free_bytes_before},
    }
    phase_started = time.perf_counter()
    active_database = state_dir / "corpus.sqlite"
    for name in ("snapshot.json", "coverage-initial.json"):
        _publish(run_dir / name, state_dir / name)
    _publish_database_generation(
        run_dir / "corpus.sqlite",
        active_database,
        state_dir / "corpus.sqlite.previous",
    )
    phase_seconds["publish"] = time.perf_counter() - phase_started
    summary_document["finished_at"] = time.time()
    summary_path = run_dir / "corpus-build-summary.json"
    summary_path.write_text(
        json.dumps(summary_document, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    _publish(summary_path, state_dir / "corpus-build-summary.json")
    return CorpusBuildSummary(
        status="success",
        profile=profile.name,
        run_id=run_id,
        accepted=accepted,
        excluded=excluded,
        database_size=int(summary_document["database_size"]),
    )


def build_corpus(
    profile_path: pathlib.Path,
    state_dir: pathlib.Path,
    *,
    input_book: Optional[pathlib.Path] = None,
    now: Optional[float] = None,
) -> CorpusBuildSummary:
    state_dir = pathlib.Path(state_dir).resolve()
    state_dir.mkdir(parents=True, exist_ok=True)
    runtime_probe = WindowsFileLock(state_dir / "book-extension.lock")
    try:
        runtime_probe.acquire({"owner": "corpus-build-probe"})
    except RuntimeError as error:
        raise CorpusBuildError(6, "lock", "book extension runtime is active") from error
    else:
        runtime_probe.close()
    build_lock = WindowsFileLock(state_dir / "corpus-build.lock")
    try:
        build_lock.acquire({"owner": "prepare-book-corpus"})
    except RuntimeError as error:
        raise CorpusBuildError(6, "lock", "another corpus build is active") from error
    try:
        return _build_corpus_unlocked(
            profile_path, state_dir, input_book=input_book, now=now
        )
    finally:
        build_lock.close()

def resolve_profile(value: str) -> pathlib.Path:
    if value in {"pilot", "production"}:
        return pathlib.Path(__file__).resolve().parents[1] / "config" / f"corpus-{value}.json"
    return pathlib.Path(value)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Build a fixed book corpus in one command")
    parser.add_argument("--profile", required=True)
    parser.add_argument("--state-dir", type=pathlib.Path, required=True)
    parser.add_argument("--input-book", type=pathlib.Path)
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        summary = build_corpus(
            resolve_profile(args.profile), args.state_dir, input_book=args.input_book
        )
    except CorpusBuildError as error:
        print(
            json.dumps(
                {
                    "status": "failed",
                    "phase": error.phase,
                    "exit_code": error.exit_code,
                    "detail": error.detail,
                },
                ensure_ascii=False,
                sort_keys=True,
            ),
            file=sys.stderr,
        )
        return error.exit_code
    print(json.dumps(dataclasses.asdict(summary), ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
