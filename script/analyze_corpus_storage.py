from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil
import sqlite3
import subprocess
import time
from typing import Any


def _percentile(values: list[float], percentile: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, int((len(ordered) - 1) * percentile))
    return ordered[index]


def _canonical_sfen(value: str) -> str:
    tokens = value.split()
    return " ".join(tokens[:3]) if len(tokens) >= 4 else value


def sample_position_keys(path: pathlib.Path, sample_size: int) -> list[str]:
    """Choose deterministic canonical keys once for an old/new paired benchmark."""
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        version = int(connection.execute(
            "SELECT value FROM meta WHERE key='schema_version'"
        ).fetchone()[0])
        if version == 4:
            rows = connection.execute(
                "SELECT position_key FROM candidate ORDER BY id"
            )
            keys: list[str] = []
            seen: set[str] = set()
            for row in rows:
                key = _canonical_sfen(str(row[0]))
                if key not in seen:
                    seen.add(key)
                    keys.append(str(row[0]))
                    if len(keys) == sample_size:
                        break
            return keys
        if version == 5:
            return [str(row[0]) for row in connection.execute(
                """SELECT p.position_key FROM position p JOIN candidate c ON c.position_id=p.id
                   GROUP BY p.id ORDER BY p.id LIMIT ?""", (sample_size,)
            )]
        raise ValueError(f"unsupported schema version: {version}")
    finally:
        connection.close()


def analyze_database(
    path: pathlib.Path, *, sample_size: int = 100, repetitions: int = 10,
    position_keys: list[str] | None = None,
) -> dict[str, Any]:
    path = pathlib.Path(path)
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        version = int(connection.execute(
            "SELECT value FROM meta WHERE key='schema_version'"
        ).fetchone()[0])
        page_size = int(connection.execute("PRAGMA page_size").fetchone()[0])
        page_count = int(connection.execute("PRAGMA page_count").fetchone()[0])
        freelist = int(connection.execute("PRAGMA freelist_count").fetchone()[0])
        try:
            object_rows = connection.execute(
                """SELECT name,SUM(pgsize),COUNT(*),SUM(payload)
                   FROM dbstat GROUP BY name ORDER BY SUM(pgsize) DESC"""
            )
            objects = [
                {"name": str(row[0]), "bytes": int(row[1]),
                 "pages": int(row[2]), "payload": int(row[3] or 0)}
                for row in object_rows
            ]
        except sqlite3.OperationalError as error:
            if "no such table: dbstat" not in str(error):
                raise
            sqlite_exe = shutil.which("sqlite3")
            if sqlite_exe is None:
                raise RuntimeError("sqlite3 CLI with dbstat is required") from error
            sql = ("SELECT name,SUM(pgsize),COUNT(*),SUM(payload) FROM dbstat "
                   "GROUP BY name ORDER BY SUM(pgsize) DESC;")
            completed = subprocess.run(
                [sqlite_exe, "-readonly", str(path), sql], check=True,
                capture_output=True, text=True, encoding="utf-8",
            )
            objects = []
            for line in completed.stdout.splitlines():
                name, bytes_value, pages, payload = line.split("|", 3)
                objects.append({"name": name, "bytes": int(bytes_value),
                                "pages": int(pages), "payload": int(payload or 0)})
        keys = position_keys if position_keys is not None else sample_position_keys(path, sample_size)
        query_keys = keys if version == 4 else [_canonical_sfen(key) for key in keys]
        if version == 4:
            query = """SELECT id,move,priority_key FROM candidate
                       WHERE active=1 AND position_key=?
                       ORDER BY priority_key DESC,id LIMIT 8"""
            plan_parameters: tuple[object, ...] = (query_keys[0] if query_keys else "",)
        elif version == 5:
            query = """SELECT c.id,c.move,c.priority_key FROM candidate c
                       JOIN position p ON p.id=c.position_id
                       WHERE c.active=1 AND p.position_key=?
                       ORDER BY c.priority_key DESC,c.id LIMIT 8"""
            plan_parameters = (query_keys[0] if query_keys else "",)
        else:
            raise ValueError(f"unsupported schema version: {version}")
        plan = [str(row[3]) for row in connection.execute(
            f"EXPLAIN QUERY PLAN {query}", plan_parameters
        )]
        durations: list[float] = []
        for _ in range(repetitions):
            for key in query_keys:
                started = time.perf_counter_ns()
                connection.execute(query, (key,)).fetchall()
                durations.append((time.perf_counter_ns() - started) / 1_000.0)
        return {
            "path": str(path.resolve()),
            "schema_version": version,
            "file_size": path.stat().st_size,
            "page_size": page_size,
            "page_count": page_count,
            "freelist_count": freelist,
            "freelist_bytes": freelist * page_size,
            "objects": objects,
            "query_plan": plan,
            "lookup": {
                "keys": len(keys),
                "repetitions": repetitions,
                "samples": len(durations),
                "p50_us": _percentile(durations, 0.50),
                "p95_us": _percentile(durations, 0.95),
                "max_us": max(durations, default=0.0),
            },
        }
    finally:
        connection.close()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Measure corpus SQLite storage and lookup")
    parser.add_argument("--db", type=pathlib.Path, action="append", required=True)
    parser.add_argument("--sample-size", type=int, default=100)
    parser.add_argument("--repetitions", type=int, default=10)
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args(argv)
    benchmark_keys = sample_position_keys(args.db[0], args.sample_size)
    report = {
        "benchmark_key_sha256": hashlib.sha256(
            "\n".join(_canonical_sfen(key) for key in benchmark_keys).encode("utf-8")
        ).hexdigest(),
        "databases": [
            analyze_database(
                path, sample_size=args.sample_size, repetitions=args.repetitions,
                position_keys=benchmark_keys,
            )
            for path in args.db
        ],
    }
    output = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(output, encoding="utf-8")
    else:
        print(output, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
