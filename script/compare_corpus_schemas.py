from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sqlite3
from collections.abc import Iterable
from typing import Any


def _canonical_sfen(value: str) -> str:
    tokens = value.split()
    return " ".join(tokens[:3]) if len(tokens) >= 4 else value


def _old_priority_blob(value: str) -> bytes:
    encoded = bytearray()
    for component in value.split("|"):
        whole, fraction = component.split(".", 1)
        scaled = int(whole) * 1_000_000 + int(fraction[:6].ljust(6, "0"))
        encoded.extend(scaled.to_bytes(8, "big", signed=False))
    return bytes(encoded)


def _stream_digest(rows: Iterable[tuple[Any, ...]]) -> dict[str, int | str]:
    digest = hashlib.sha256()
    count = 0
    for row in rows:
        payload = json.dumps(row, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
        count += 1
    return {"count": count, "sha256": digest.hexdigest()}


def _version(connection: sqlite3.Connection) -> int:
    row = connection.execute(
        "SELECT value FROM meta WHERE key='schema_version'"
    ).fetchone()
    return int(row[0])


def _deduplicate_old_candidates(rows: Iterable[Any]) -> Iterable[tuple[Any, ...]]:
    current_key: tuple[str, str] | None = None
    best: tuple[Any, ...] | None = None
    for row in rows:
        candidate = (
            _canonical_sfen(str(row[0])), str(row[1]),
            _old_priority_blob(str(row[2])).hex(), int(row[3]),
        )
        key = (candidate[0], candidate[1])
        if current_key is not None and key != current_key:
            assert best is not None
            yield best
            best = None
        current_key = key
        if best is None or (candidate[2], candidate[3]) > (best[2], best[3]):
            best = candidate
    if best is not None:
        yield best

def _has_table(connection: sqlite3.Connection, name: str) -> bool:
    return connection.execute(
        "SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?", (name,)
    ).fetchone() is not None


def _deduplicate_sorted_tuples(rows: Iterable[Any]) -> Iterable[tuple[Any, ...]]:
    previous: tuple[Any, ...] | None = None
    for row in rows:
        value = tuple(row)
        if value != previous:
            yield value
            previous = value

def _streams(connection: sqlite3.Connection, version: int) -> dict[str, Iterable[tuple[Any, ...]]]:
    games = (
        tuple(row)
        for row in connection.execute(
            """SELECT game_hash,initial_sfen,black_name,white_name,moves_json
               FROM logical_game ORDER BY game_hash"""
        )
    )
    sources = (
        tuple(row)
        for row in connection.execute(
            """SELECT site,event,year,relative_path,sha256,retrieved_at
               FROM raw_source ORDER BY site,relative_path,sha256"""
        )
    )
    source_games = (
        tuple(row)
        for row in connection.execute(
            """SELECT rs.site,rs.relative_path,rs.sha256,lg.game_hash
               FROM source_game sg JOIN raw_source rs ON rs.id=sg.source_id
               JOIN logical_game lg ON lg.id=sg.game_id
               ORDER BY rs.site,rs.relative_path,rs.sha256,lg.game_hash"""
        )
    )
    if version == 4:
        positions = (
            (str(row[0]), int(row[1]), _canonical_sfen(str(row[2])), str(row[3]),
             str(row[4]), str(row[5]), int(row[6]))
            for row in connection.execute(
                """SELECT lg.game_hash,gp.ply,gp.position_key,gp.move,
                          gp.site,gp.event,gp.year
                   FROM game_position gp JOIN logical_game lg ON lg.id=gp.game_id
                   ORDER BY lg.game_hash,gp.ply"""
            )
        )
        candidates = _deduplicate_old_candidates(
            connection.execute(
                """SELECT position_key,move,priority_key,active FROM candidate
                   ORDER BY canonical_sfen(position_key),move,priority_key DESC,id"""
            )
        )
    elif version == 5:
        positions = (
            tuple(row)
            for row in connection.execute(
                """SELECT lg.game_hash,gp.ply,p.position_key,gp.move,
                          rs.site,rs.event,rs.year
                   FROM game_position gp JOIN logical_game lg ON lg.id=gp.game_id
                   JOIN position p ON p.id=gp.position_id
                   JOIN raw_source rs ON rs.id=lg.primary_source_id
                   ORDER BY lg.game_hash,gp.ply"""
            )
        )
        candidates = (
            (str(row[0]), str(row[1]), bytes(row[2]).hex(), int(row[3]))
            for row in connection.execute(
                """SELECT p.position_key,c.move,c.priority_key,c.active
                   FROM candidate c JOIN position p ON p.id=c.position_id
                   ORDER BY p.position_key,c.move"""
            )
        )
    else:
        raise ValueError(f"unsupported corpus schema version: {version}")
    if version == 4 and _has_table(connection, "candidate_source"):
        candidate_sources: Iterable[tuple[Any, ...]] = _deduplicate_sorted_tuples(
            (
                (_canonical_sfen(str(row[0])), str(row[1]), str(row[2]))
                for row in connection.execute(
                    """SELECT c.position_key,c.move,cs.source
                       FROM candidate_source cs JOIN candidate c ON c.id=cs.candidate_id
                       ORDER BY canonical_sfen(c.position_key),c.move,cs.source"""
                )
            )
        )
    elif version == 5:
        candidate_sources = _deduplicate_sorted_tuples(
            connection.execute(
                """SELECT p.position_key,gp.move,
                          rs.site || ':' || rs.event || ':' || rs.relative_path AS source
                   FROM game_position gp JOIN position p ON p.id=gp.position_id
                   JOIN source_game sg ON sg.game_id=gp.game_id
                   JOIN raw_source rs ON rs.id=sg.source_id
                   ORDER BY p.position_key,gp.move,source"""
            )
        )
    else:
        candidate_sources = iter(())
    errors = (
        tuple(row)
        for row in connection.execute(
            """SELECT rs.site,rs.relative_path,ie.error,ie.error_line,ie.previous_sfen,
                      ie.move,ie.reason
               FROM ingest_error ie JOIN raw_source rs ON rs.id=ie.source_id
               ORDER BY rs.site,rs.relative_path,ie.id"""
        )
    )
    return {
        "games": games,
        "sources": sources,
        "source_games": source_games,
        "positions": positions,
        "candidates": candidates,
        "candidate_sources": candidate_sources,
        "ingest_errors": errors,
    }


def _open_readonly(path: pathlib.Path) -> sqlite3.Connection:
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    connection.create_function("canonical_sfen", 1, _canonical_sfen, deterministic=True)
    return connection


def compare_corpus_schemas(old_path: pathlib.Path, new_path: pathlib.Path) -> dict[str, Any]:
    old = _open_readonly(pathlib.Path(old_path))
    new = _open_readonly(pathlib.Path(new_path))
    try:
        old_version = _version(old)
        new_version = _version(new)
        old_streams = _streams(old, old_version)
        new_streams = _streams(new, new_version)
        streams: dict[str, Any] = {}
        mismatches: list[str] = []
        for name in old_streams:
            old_digest = _stream_digest(old_streams[name])
            new_digest = _stream_digest(new_streams[name])
            streams[name] = {"old": old_digest, "new": new_digest, **new_digest}
            if old_digest != new_digest:
                mismatches.append(name)
        return {
            "old_schema_version": old_version,
            "new_schema_version": new_version,
            "streams": streams,
            "mismatches": mismatches,
        }
    finally:
        old.close()
        new.close()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Stream-compare corpus schema semantics")
    parser.add_argument("--old", type=pathlib.Path, required=True)
    parser.add_argument("--new", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args(argv)
    report = compare_corpus_schemas(args.old, args.new)
    output = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(output, encoding="utf-8")
    else:
        print(output, end="")
    return 1 if report["mismatches"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
