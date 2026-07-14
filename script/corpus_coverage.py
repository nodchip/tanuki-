from __future__ import annotations

from typing import Any

try:
    from script.book_validation import registered_legal_move_count
except ImportError:
    from book_validation import registered_legal_move_count


def _metric(covered: int, total: int) -> dict[str, int | float]:
    return {"covered": covered, "total": total, "rate": covered / total if total else 0.0}


def _ply_band(ply_band: int) -> str:
    lower = ply_band * 20
    return f"{lower + 1}-{lower + 20}"


def _pair(row: Any) -> dict[str, int | float]:
    return _metric(int(row[1] or 0), int(row[2] or 0))


def _group_metrics(connection: Any, expression: str) -> dict[str, dict[str, int | float]]:
    base = f"""
        SELECT {expression} AS group_key, gp.position_id, cp.covered
        FROM game_position gp
        JOIN logical_game lg ON lg.id = gp.game_id
        JOIN raw_source rs ON rs.id = lg.primary_source_id
        JOIN temp.covered_position cp ON cp.position_id = gp.position_id
    """
    unique_rows = connection.execute(
        f"""WITH grouped AS (
                SELECT group_key, position_id, MAX(covered) AS covered
                FROM ({base}) GROUP BY group_key, position_id
              )
              SELECT group_key, SUM(covered), COUNT(*)
              FROM grouped GROUP BY group_key ORDER BY group_key"""
    )
    unique = {str(row[0]): _pair(row) for row in unique_rows}
    unique_rows.close()
    occurrence_rows = connection.execute(
        f"""SELECT group_key, SUM(covered), COUNT(*)
             FROM ({base}) GROUP BY group_key ORDER BY group_key"""
    )
    occurrences = {str(row[0]): _pair(row) for row in occurrence_rows}
    occurrence_rows.close()
    return {
        key: {"unique": value, "occurrences": occurrences[key]}
        for key, value in unique.items()
    }


def generate_coverage_report(
    store: Any,
    book: Any,
    *,
    snapshot_id: str,
    book_hash: str,
) -> dict[str, Any]:
    """Generate coverage with bounded Python memory using a disk-backed temp table."""
    connection = store.connection
    connection.execute("PRAGMA temp_store = FILE")
    connection.execute("DROP TABLE IF EXISTS temp.covered_position")
    connection.execute(
        """CREATE TEMP TABLE covered_position(
               position_id INTEGER PRIMARY KEY,
               covered INTEGER NOT NULL
           ) WITHOUT ROWID"""
    )
    batch: list[tuple[int, int]] = []
    positions = connection.execute(
        """SELECT DISTINCT p.id, p.position_key
           FROM game_position gp JOIN position p ON p.id = gp.position_id
           ORDER BY p.id"""
    )
    for row in positions:
        key = str(row["position_key"])
        position = book.positions.get(book.position_key(key))
        covered = int(
            position is not None
            and registered_legal_move_count(position.sfen, position.entries) > 0
        )
        batch.append((int(row["id"]), covered))
        if len(batch) >= 10_000:
            connection.executemany(
                "INSERT INTO temp.covered_position(position_id,covered) VALUES(?,?)", batch
            )
            batch.clear()
    positions.close()
    if batch:
        connection.executemany(
            "INSERT INTO temp.covered_position(position_id,covered) VALUES(?,?)", batch
        )

    unique_row = connection.execute(
        "SELECT SUM(covered), COUNT(*) FROM temp.covered_position"
    ).fetchone()
    occurrence_row = connection.execute(
        """SELECT SUM(cp.covered), COUNT(*) FROM game_position gp
           JOIN temp.covered_position cp ON cp.position_id = gp.position_id"""
    ).fetchone()
    report: dict[str, Any] = {
        "snapshot_id": snapshot_id,
        "book_hash": book_hash,
        "corpus_revision": store.corpus_revision(),
        "overall": {
            "unique": _metric(int(unique_row[0] or 0), int(unique_row[1])),
            "occurrences": _metric(int(occurrence_row[0] or 0), int(occurrence_row[1])),
        },
        "by_site": _group_metrics(connection, "rs.site"),
        "by_event": _group_metrics(connection, "rs.event"),
        "by_bucket": _group_metrics(connection, "CAST(floor((rs.year - 2024) / 3.0) AS INTEGER)"),
        "by_ply_band": _group_metrics(connection, "CAST(gp.ply / 20 AS INTEGER)"),
        "by_side": _group_metrics(connection, "CASE WHEN gp.ply % 2 = 0 THEN 'black' ELSE 'white' END"),
    }
    report["by_ply_band"] = {
        _ply_band(int(key)): value for key, value in report["by_ply_band"].items()
    }
    connection.execute("DROP TABLE temp.covered_position").close()
    connection.commit()
    return report
