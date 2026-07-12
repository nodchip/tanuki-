from __future__ import annotations

import dataclasses
import enum
import json
import pathlib
import sqlite3
import time
from typing import Optional, Sequence


SCHEMA_VERSION = 4


class SearchTaskStatus(str, enum.Enum):
    PENDING = "pending"
    RUNNING = "running"
    EVALUATED = "evaluated"
    PERMANENT_FAILED = "permanent_failed"
    SUPERSEDED = "superseded"


@dataclasses.dataclass(frozen=True)
class CorpusCandidate:
    id: int
    position_key: str
    move: str
    history: tuple[str, ...]
    source: str
    priority_key: str
    status: SearchTaskStatus
    attempts: int
    book_snapshot_id: str


@dataclasses.dataclass(frozen=True)
class StoredSearchResult:
    task_id: int
    position_key: str
    move: str
    eval_cp: int
    response: str
    depth: int
    nodes: int
    engine_config_id: str
    persisted_checkpoint_id: Optional[int]


@dataclasses.dataclass(frozen=True)
class Checkpoint:
    id: int
    book_hash: str
    created_at: float


class CorpusStore:
    """Portable SQLite control plane for corpus candidates and search results."""

    def __init__(self, path: pathlib.Path) -> None:
        self.path = pathlib.Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.connection = sqlite3.connect(self.path, timeout=5.0, check_same_thread=False)
        self.connection.row_factory = sqlite3.Row
        self.connection.execute("PRAGMA foreign_keys = ON")
        self.connection.execute("PRAGMA journal_mode = WAL")
        try:
            self.migrate()
        except Exception:
            self.connection.close()
            raise

    def __enter__(self) -> "CorpusStore":
        return self

    def __exit__(self, exc_type: object, exc_value: object, traceback: object) -> None:
        self.close()

    def close(self) -> None:
        self.connection.close()

    def migrate(self) -> None:
        has_meta = self.connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'meta'"
        ).fetchone()
        if has_meta is not None:
            row = self.connection.execute(
                "SELECT value FROM meta WHERE key = 'schema_version'"
            ).fetchone()
            if row is not None and int(row["value"]) > SCHEMA_VERSION:
                raise ValueError(f"database uses newer schema version {row['value']}")
        with self.connection:
            self.connection.executescript(
                """
                CREATE TABLE IF NOT EXISTS meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS candidate (
                    id INTEGER PRIMARY KEY,
                    position_key TEXT NOT NULL,
                    move TEXT NOT NULL,
                    history_json TEXT NOT NULL,
                    source TEXT NOT NULL,
                    priority_key TEXT NOT NULL,
                    active INTEGER NOT NULL DEFAULT 1,
                    created_at REAL NOT NULL,
                    updated_at REAL NOT NULL,
                    UNIQUE(position_key, move)
                );

                CREATE TABLE IF NOT EXISTS candidate_source (
                    candidate_id INTEGER NOT NULL REFERENCES candidate(id),
                    source TEXT NOT NULL,
                    history_json TEXT NOT NULL,
                    PRIMARY KEY(candidate_id, source)
                );
                CREATE INDEX IF NOT EXISTS candidate_priority_idx
                    ON candidate(active, priority_key DESC, id);
                CREATE INDEX IF NOT EXISTS candidate_position_priority_idx
                    ON candidate(position_key, active, priority_key DESC, id);

                CREATE TABLE IF NOT EXISTS search_task (
                    id INTEGER PRIMARY KEY,
                    candidate_id INTEGER NOT NULL REFERENCES candidate(id),
                    book_snapshot_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    lease_until REAL,
                    last_error TEXT,
                    eval_cp INTEGER,
                    response TEXT,
                    depth INTEGER,
                    nodes INTEGER,
                    engine_config_id TEXT,
                    persisted_checkpoint_id INTEGER,
                    updated_at REAL NOT NULL,
                    UNIQUE(candidate_id, book_snapshot_id)
                );

                CREATE INDEX IF NOT EXISTS search_task_status_idx
                    ON search_task(book_snapshot_id, status, lease_until);

                CREATE TABLE IF NOT EXISTS raw_source (
                    id INTEGER PRIMARY KEY,
                    site TEXT NOT NULL,
                    event TEXT NOT NULL,
                    year INTEGER NOT NULL,
                    relative_path TEXT NOT NULL,
                    sha256 TEXT NOT NULL,
                    retrieved_at REAL NOT NULL,
                    UNIQUE(site, relative_path, sha256)
                );

                CREATE TABLE IF NOT EXISTS logical_game (
                    id INTEGER PRIMARY KEY,
                    game_hash TEXT NOT NULL UNIQUE,
                    initial_sfen TEXT NOT NULL,
                    black_name TEXT NOT NULL,
                    white_name TEXT NOT NULL,
                    moves_json TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS source_game (
                    source_id INTEGER NOT NULL REFERENCES raw_source(id),
                    game_id INTEGER NOT NULL REFERENCES logical_game(id),
                    PRIMARY KEY(source_id, game_id)
                );

                CREATE TABLE IF NOT EXISTS game_position (
                    game_id INTEGER NOT NULL REFERENCES logical_game(id),
                    ply INTEGER NOT NULL,
                    site TEXT NOT NULL,
                    event TEXT NOT NULL,
                    year INTEGER NOT NULL,
                    position_key TEXT NOT NULL,
                    sfen TEXT NOT NULL,
                    history_json TEXT NOT NULL,
                    move TEXT NOT NULL,
                    PRIMARY KEY(game_id, ply)
                );

                CREATE INDEX IF NOT EXISTS game_position_coverage_idx
                    ON game_position(site, year, position_key);

                CREATE TABLE IF NOT EXISTS ingest_error (
                    id INTEGER PRIMARY KEY,
                    source_id INTEGER NOT NULL REFERENCES raw_source(id),
                    error TEXT NOT NULL,
                    error_line INTEGER,
                    previous_sfen TEXT,
                    move TEXT,
                    reason TEXT NOT NULL DEFAULT '',
                    created_at REAL NOT NULL
                );
                CREATE TABLE IF NOT EXISTS ranking_snapshot (
                    id INTEGER PRIMARY KEY,
                    site TEXT NOT NULL,
                    event TEXT NOT NULL,
                    source_url TEXT NOT NULL,
                    retrieved_at REAL NOT NULL,
                    source_sha256 TEXT NOT NULL,
                    provisional INTEGER NOT NULL,
                    policy_version TEXT NOT NULL,
                    active INTEGER NOT NULL DEFAULT 1
                );

                CREATE TABLE IF NOT EXISTS participant (
                    id INTEGER PRIMARY KEY,
                    event TEXT NOT NULL,
                    official_name TEXT NOT NULL,
                    normalized_name TEXT NOT NULL,
                    UNIQUE(event, official_name)
                );

                CREATE TABLE IF NOT EXISTS participant_alias (
                    event TEXT NOT NULL,
                    alias TEXT NOT NULL,
                    participant_id INTEGER NOT NULL REFERENCES participant(id),
                    mapping_status TEXT NOT NULL,
                    PRIMARY KEY(event, alias)
                );

                CREATE TABLE IF NOT EXISTS ranking_result (
                    snapshot_id INTEGER NOT NULL REFERENCES ranking_snapshot(id),
                    participant_id INTEGER NOT NULL REFERENCES participant(id),
                    stage TEXT NOT NULL,
                    stage_tier INTEGER NOT NULL,
                    rank INTEGER NOT NULL,
                    participants INTEGER NOT NULL,
                    rank_percentile REAL NOT NULL,
                    PRIMARY KEY(snapshot_id, participant_id)
                );

                CREATE TABLE IF NOT EXISTS rating_snapshot (
                    id INTEGER PRIMARY KEY,
                    year INTEGER NOT NULL,
                    snapshot_time REAL NOT NULL,
                    source_url TEXT NOT NULL,
                    source_sha256 TEXT NOT NULL,
                    anchor_era TEXT NOT NULL,
                    rated_player_count INTEGER NOT NULL,
                    component_size INTEGER NOT NULL,
                    anchor_connected_rate REAL NOT NULL,
                    active INTEGER NOT NULL DEFAULT 1
                );

                CREATE TABLE IF NOT EXISTS rating_result (
                    snapshot_id INTEGER NOT NULL REFERENCES rating_snapshot(id),
                    player_name TEXT NOT NULL,
                    rating REAL NOT NULL,
                    effective_games REAL NOT NULL,
                    connected_to_anchor INTEGER NOT NULL,
                    reliability_bucket INTEGER NOT NULL,
                    anchor_margin REAL NOT NULL,
                    snapshot_percentile REAL NOT NULL,
                    PRIMARY KEY(snapshot_id, player_name)
                );
                CREATE TABLE IF NOT EXISTS progressive_width_history (
                    id INTEGER PRIMARY KEY,
                    old_width INTEGER NOT NULL,
                    new_width INTEGER NOT NULL,
                    changed_at REAL NOT NULL,
                    reason TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS metric_counter (
                    name TEXT PRIMARY KEY,
                    value INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS checkpoint (
                    id INTEGER PRIMARY KEY,
                    book_hash TEXT NOT NULL,
                    created_at REAL NOT NULL
                );
                """
            )
            self.connection.execute(
                "INSERT OR IGNORE INTO meta(key, value) VALUES('schema_version', ?)",
                (str(SCHEMA_VERSION),),
            )
            self.connection.execute(
                "INSERT OR IGNORE INTO meta(key, value) VALUES('corpus_revision', '0')"
            )
            self.connection.execute(
                "INSERT OR IGNORE INTO meta(key, value) VALUES('progressive_width', '1')"
            )
            self.connection.execute(
                "INSERT OR IGNORE INTO meta(key, value) VALUES('zero_addition_rollouts', '0')"
            )
            ingest_error_columns = {
                str(row["name"]) for row in self.connection.execute("PRAGMA table_info(ingest_error)")
            }
            for name, declaration in (
                ("error_line", "INTEGER"),
                ("previous_sfen", "TEXT"),
                ("move", "TEXT"),
                ("reason", "TEXT NOT NULL DEFAULT ''"),
            ):
                if name not in ingest_error_columns:
                    self.connection.execute(
                        f"ALTER TABLE ingest_error ADD COLUMN {name} {declaration}"
                    )
            self.connection.execute(
                "UPDATE meta SET value = ? WHERE key = 'schema_version'",
                (str(SCHEMA_VERSION),),
            )

    def add_metric(self, name: str, value: int) -> None:
        with self.connection:
            self.connection.execute(
                """INSERT INTO metric_counter(name, value) VALUES(?, ?)
                   ON CONFLICT(name) DO UPDATE SET value = metric_counter.value + excluded.value""",
                (name, value),
            )

    def metric(self, name: str) -> int:
        row = self.connection.execute(
            "SELECT value FROM metric_counter WHERE name = ?", (name,)
        ).fetchone()
        return 0 if row is None else int(row["value"])
    def table_count(self, table: str) -> int:
        allowed = {
            "raw_source", "logical_game", "source_game", "game_position",
            "ingest_error", "candidate", "candidate_source", "search_task", "checkpoint",
        }
        if table not in allowed:
            raise ValueError(f"unsupported table: {table}")
        row = self.connection.execute(f"SELECT COUNT(*) AS count FROM {table}").fetchone()
        return int(row["count"])
    def _meta_int(self, key: str) -> int:
        row = self.connection.execute("SELECT value FROM meta WHERE key = ?", (key,)).fetchone()
        if row is None:
            raise KeyError(key)
        return int(row["value"])

    def schema_version(self) -> int:
        return self._meta_int("schema_version")

    def corpus_revision(self) -> int:
        return self._meta_int("corpus_revision")

    def progressive_width(self) -> int:
        return self._meta_int("progressive_width")

    def zero_addition_rollouts(self) -> int:
        return self._meta_int("zero_addition_rollouts")

    def record_corpus_rollout(self, *, added: bool, saturation_window: int = 100) -> bool:
        if saturation_window < 1:
            raise ValueError("saturation_window must be positive")
        with self.connection:
            zero_count = 0 if added else self.zero_addition_rollouts() + 1
            self.connection.execute(
                "UPDATE meta SET value = ? WHERE key = 'zero_addition_rollouts'",
                (str(zero_count),),
            )
            pending = self.connection.execute(
                "SELECT COUNT(*) AS count FROM search_task WHERE status IN (?, ?)",
                (SearchTaskStatus.PENDING.value, SearchTaskStatus.RUNNING.value),
            ).fetchone()
            if zero_count < saturation_window or int(pending["count"]) != 0:
                return False
            width = self.progressive_width() + 1
            old_width = width - 1
            self.connection.execute(
                "UPDATE meta SET value = ? WHERE key = 'progressive_width'", (str(width),)
            )
            self.connection.execute(
                """INSERT INTO progressive_width_history(
                       old_width, new_width, changed_at, reason
                   ) VALUES(?, ?, ?, ?)""",
                (old_width, width, time.time(), f"{saturation_window} zero-addition rollouts"),
            )
            self.connection.execute(
                "UPDATE meta SET value = '0' WHERE key = 'zero_addition_rollouts'"
            )
            return True

    def bump_corpus_revision(self) -> int:
        """Commit a manually stopped corpus refresh without resetting progressive N."""
        with self.connection:
            revision = self.corpus_revision() + 1
            self.connection.execute(
                "UPDATE meta SET value = ? WHERE key = 'corpus_revision'", (str(revision),)
            )
            self.connection.execute(
                "UPDATE meta SET value = '0' WHERE key = 'zero_addition_rollouts'"
            )
        return revision
    def upsert_candidate(
        self,
        position_key: str,
        move: str,
        history: Sequence[str],
        source: str,
        priority_key: str,
    ) -> int:
        now = time.time()
        with self.connection:
            self.connection.execute(
                """
                INSERT INTO candidate(
                    position_key, move, history_json, source, priority_key,
                    active, created_at, updated_at
                ) VALUES(?, ?, ?, ?, ?, 1, ?, ?)
                ON CONFLICT(position_key, move) DO UPDATE SET
                    history_json = CASE
                        WHEN excluded.priority_key > candidate.priority_key
                        THEN excluded.history_json ELSE candidate.history_json END,
                    source = CASE
                        WHEN excluded.priority_key > candidate.priority_key
                        THEN excluded.source ELSE candidate.source END,
                    priority_key = MAX(candidate.priority_key, excluded.priority_key),
                    active = 1,
                    updated_at = excluded.updated_at
                """,
                (
                    position_key,
                    move,
                    json.dumps(list(history), separators=(",", ":")),
                    source,
                    priority_key,
                    now,
                    now,
                ),
            )
            row = self.connection.execute(
                "SELECT id FROM candidate WHERE position_key = ? AND move = ?",
                (position_key, move),
            ).fetchone()
        assert row is not None
        candidate_id = int(row["id"])
        with self.connection:
            self.connection.execute(
                """
                INSERT INTO candidate_source(candidate_id, source, history_json)
                VALUES(?, ?, ?)
                ON CONFLICT(candidate_id, source) DO UPDATE SET
                    history_json = excluded.history_json
                """,
                (candidate_id, source, json.dumps(list(history), separators=(",", ":"))),
            )
        return candidate_id

    def candidate_source_count(self, position_key: str, move: str) -> int:
        row = self.connection.execute(
            """
            SELECT COUNT(*) AS count FROM candidate_source cs
            JOIN candidate c ON c.id = cs.candidate_id
            WHERE c.position_key = ? AND c.move = ?
            """, (position_key, move),
        ).fetchone()
        return int(row["count"])

    def reserve_for_position(
        self, book_snapshot_id: str, position_key: str, *,
        excluded_moves: set[str], width: int, lease_sec: float,
        site: Optional[str] = None, now: Optional[float] = None,
    ) -> Optional[CorpusCandidate]:
        """Reserve an eligible candidate among the top N moves at one visited position."""
        if width < 1:
            raise ValueError("width must be positive")
        current_time = time.time() if now is None else now
        lease_until = current_time + lease_sec
        with self.connection:
            rows = self.connection.execute(
                """
                SELECT c.id AS candidate_id, c.position_key, c.move,
                       c.history_json, c.source, c.priority_key,
                       t.id AS task_id, t.status, t.attempts, t.lease_until
                FROM candidate c
                LEFT JOIN search_task t
                  ON t.candidate_id = c.id AND t.book_snapshot_id = ?
                WHERE c.active = 1 AND c.position_key = ?
                ORDER BY c.priority_key DESC, c.id LIMIT ?
                """, (book_snapshot_id, position_key, width),
            ).fetchall()
            pending_ids = [
                int(item["task_id"]) for item in rows
                if item["task_id"] is not None
                and item["move"] in excluded_moves
                and item["status"] == SearchTaskStatus.PENDING.value
            ]
            for task_id in pending_ids:
                self.connection.execute(
                    "UPDATE search_task SET status = ?, updated_at = ? WHERE id = ?",
                    (SearchTaskStatus.SUPERSEDED.value, current_time, task_id),
                )
            row = next((item for item in rows if item["move"] not in excluded_moves
                and (site is None or str(item["source"]).split(":", 1)[0] == site) and (
                item["task_id"] is None
                or item["status"] == SearchTaskStatus.PENDING.value
                or (item["status"] == SearchTaskStatus.RUNNING.value
                    and item["lease_until"] is not None
                    and float(item["lease_until"]) <= current_time)
            )), None)
            if row is None:
                return None
            attempts = int(row["attempts"] or 0) + 1
            if row["task_id"] is None:
                cursor = self.connection.execute(
                    """INSERT INTO search_task(candidate_id, book_snapshot_id, status,
                       attempts, lease_until, updated_at) VALUES(?, ?, ?, ?, ?, ?)""",
                    (row["candidate_id"], book_snapshot_id, SearchTaskStatus.RUNNING.value,
                     attempts, lease_until, current_time),
                )
                task_id = int(cursor.lastrowid)
            else:
                task_id = int(row["task_id"])
                self.connection.execute(
                    """UPDATE search_task SET status = ?, attempts = ?, lease_until = ?,
                       last_error = NULL, updated_at = ? WHERE id = ?""",
                    (SearchTaskStatus.RUNNING.value, attempts, lease_until, current_time, task_id),
                )
        return CorpusCandidate(
            id=task_id, position_key=str(row["position_key"]), move=str(row["move"]),
            history=tuple(json.loads(row["history_json"])), source=str(row["source"]),
            priority_key=str(row["priority_key"]), status=SearchTaskStatus.RUNNING,
            attempts=attempts, book_snapshot_id=book_snapshot_id,
        )
    def reserve_candidate(
        self,
        book_snapshot_id: str,
        *,
        now: Optional[float] = None,
        lease_sec: float,
    ) -> Optional[CorpusCandidate]:
        current_time = time.time() if now is None else now
        lease_until = current_time + lease_sec
        with self.connection:
            row = self.connection.execute(
                """
                SELECT
                    c.id AS candidate_id, c.position_key, c.move,
                    c.history_json, c.source, c.priority_key,
                    t.id AS task_id, t.status, t.attempts, t.lease_until
                FROM candidate c
                LEFT JOIN search_task t
                  ON t.candidate_id = c.id AND t.book_snapshot_id = ?
                WHERE c.active = 1
                  AND (
                    t.id IS NULL
                    OR t.status = ?
                    OR (t.status = ? AND t.lease_until <= ?)
                  )
                ORDER BY c.priority_key DESC, c.id
                LIMIT 1
                """,
                (
                    book_snapshot_id,
                    SearchTaskStatus.PENDING.value,
                    SearchTaskStatus.RUNNING.value,
                    current_time,
                ),
            ).fetchone()
            if row is None:
                return None
            attempts = int(row["attempts"] or 0) + 1
            if row["task_id"] is None:
                cursor = self.connection.execute(
                    """
                    INSERT INTO search_task(
                        candidate_id, book_snapshot_id, status, attempts,
                        lease_until, updated_at
                    ) VALUES(?, ?, ?, ?, ?, ?)
                    """,
                    (
                        row["candidate_id"],
                        book_snapshot_id,
                        SearchTaskStatus.RUNNING.value,
                        attempts,
                        lease_until,
                        current_time,
                    ),
                )
                task_id = int(cursor.lastrowid)
            else:
                task_id = int(row["task_id"])
                self.connection.execute(
                    """
                    UPDATE search_task
                    SET status = ?, attempts = ?, lease_until = ?,
                        last_error = NULL, updated_at = ?
                    WHERE id = ?
                    """,
                    (
                        SearchTaskStatus.RUNNING.value,
                        attempts,
                        lease_until,
                        current_time,
                        task_id,
                    ),
                )

        return CorpusCandidate(
            id=task_id,
            position_key=str(row["position_key"]),
            move=str(row["move"]),
            history=tuple(json.loads(row["history_json"])),
            source=str(row["source"]),
            priority_key=str(row["priority_key"]),
            status=SearchTaskStatus.RUNNING,
            attempts=attempts,
            book_snapshot_id=book_snapshot_id,
        )

    def complete_search(
        self,
        task_id: int,
        *,
        eval_cp: int,
        response: str,
        depth: int,
        nodes: int,
        engine_config_id: str,
    ) -> None:
        with self.connection:
            cursor = self.connection.execute(
                """
                UPDATE search_task
                SET status = ?, lease_until = NULL, last_error = NULL,
                    eval_cp = ?, response = ?, depth = ?, nodes = ?,
                    engine_config_id = ?, persisted_checkpoint_id = NULL,
                    updated_at = ?
                WHERE id = ? AND status = ?
                """,
                (
                    SearchTaskStatus.EVALUATED.value,
                    eval_cp,
                    response,
                    depth,
                    nodes,
                    engine_config_id,
                    time.time(),
                    task_id,
                    SearchTaskStatus.RUNNING.value,
                ),
            )
            if cursor.rowcount != 1:
                raise KeyError(f"running task not found: {task_id}")

    def fail_search(
        self,
        task_id: int,
        error: str,
        *,
        max_attempts: int,
    ) -> SearchTaskStatus:
        row = self.connection.execute(
            "SELECT attempts FROM search_task WHERE id = ?", (task_id,)
        ).fetchone()
        if row is None:
            raise KeyError(task_id)
        status = (
            SearchTaskStatus.PERMANENT_FAILED
            if int(row["attempts"]) >= max_attempts
            else SearchTaskStatus.PENDING
        )
        with self.connection:
            self.connection.execute(
                """
                UPDATE search_task
                SET status = ?, lease_until = NULL, last_error = ?, updated_at = ?
                WHERE id = ?
                """,
                (status.value, error, time.time(), task_id),
            )
        return status

    def reset_interrupted_tasks(self) -> int:
        with self.connection:
            cursor = self.connection.execute(
                """
                UPDATE search_task
                SET status = ?, lease_until = NULL, updated_at = ?
                WHERE status = ?
                """,
                (
                    SearchTaskStatus.PENDING.value,
                    time.time(),
                    SearchTaskStatus.RUNNING.value,
                ),
            )
        return int(cursor.rowcount)

    def has_checkpoint(self, book_hash: str) -> bool:
        return self.connection.execute(
            "SELECT 1 FROM checkpoint WHERE book_hash = ? LIMIT 1", (book_hash,)
        ).fetchone() is not None
    def results_requiring_replay(self, book_hash: str) -> list[StoredSearchResult]:
        checkpoint = self.connection.execute(
            "SELECT MAX(id) AS id FROM checkpoint WHERE book_hash = ?", (book_hash,)
        ).fetchone()
        checkpoint_id = checkpoint["id"]
        if checkpoint_id is None:
            condition = "t.status = ?"
            parameters: tuple[object, ...] = (SearchTaskStatus.EVALUATED.value,)
        else:
            condition = "t.status = ? AND (t.persisted_checkpoint_id IS NULL OR t.persisted_checkpoint_id > ?)"
            parameters = (SearchTaskStatus.EVALUATED.value, int(checkpoint_id))
        rows = self.connection.execute(
            f"""SELECT t.id AS task_id, c.position_key, c.move, t.eval_cp,
                t.response, t.depth, t.nodes, t.engine_config_id,
                t.persisted_checkpoint_id
                FROM search_task t JOIN candidate c ON c.id = t.candidate_id
                WHERE {condition} ORDER BY t.id""",
            parameters,
        ).fetchall()
        return [
            StoredSearchResult(
                task_id=int(row["task_id"]), position_key=str(row["position_key"]),
                move=str(row["move"]), eval_cp=int(row["eval_cp"]),
                response=str(row["response"]), depth=int(row["depth"]),
                nodes=int(row["nodes"]), engine_config_id=str(row["engine_config_id"]),
                persisted_checkpoint_id=row["persisted_checkpoint_id"],
            ) for row in rows
        ]
    def unpersisted_results(self) -> list[StoredSearchResult]:
        rows = self.connection.execute(
            """
            SELECT
                t.id AS task_id, c.position_key, c.move,
                t.eval_cp, t.response, t.depth, t.nodes,
                t.engine_config_id, t.persisted_checkpoint_id
            FROM search_task t
            JOIN candidate c ON c.id = t.candidate_id
            WHERE t.status = ? AND t.persisted_checkpoint_id IS NULL
            ORDER BY t.id
            """,
            (SearchTaskStatus.EVALUATED.value,),
        ).fetchall()
        return [
            StoredSearchResult(
                task_id=int(row["task_id"]),
                position_key=str(row["position_key"]),
                move=str(row["move"]),
                eval_cp=int(row["eval_cp"]),
                response=str(row["response"]),
                depth=int(row["depth"]),
                nodes=int(row["nodes"]),
                engine_config_id=str(row["engine_config_id"]),
                persisted_checkpoint_id=row["persisted_checkpoint_id"],
            )
            for row in rows
        ]

    def record_checkpoint(self, book_hash: str) -> int:
        created_at = time.time()
        with self.connection:
            cursor = self.connection.execute(
                "INSERT INTO checkpoint(book_hash, created_at) VALUES(?, ?)",
                (book_hash, created_at),
            )
            checkpoint_id = int(cursor.lastrowid)
            self.connection.execute(
                """
                UPDATE search_task
                SET persisted_checkpoint_id = ?, updated_at = ?
                WHERE status = ? AND persisted_checkpoint_id IS NULL
                """,
                (
                    checkpoint_id,
                    created_at,
                    SearchTaskStatus.EVALUATED.value,
                ),
            )
        return checkpoint_id

    def checkpoint(self, checkpoint_id: int) -> Checkpoint:
        row = self.connection.execute(
            "SELECT id, book_hash, created_at FROM checkpoint WHERE id = ?",
            (checkpoint_id,),
        ).fetchone()
        if row is None:
            raise KeyError(checkpoint_id)
        return Checkpoint(int(row["id"]), str(row["book_hash"]), float(row["created_at"]))