from __future__ import annotations

import dataclasses
import enum
import json
import pathlib
import sqlite3
import time
from typing import Optional, Sequence


SCHEMA_VERSION = 5
_SCHEMA_PATH = pathlib.Path(__file__).with_name("corpus_schema.sql")


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
    priority_key: bytes
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


def canonical_position_key(sfen: str) -> str:
    tokens = sfen.split()
    return " ".join(tokens[:3]) if len(tokens) >= 4 else sfen


def position_sfen(position_key: str) -> str:
    return f"{position_key} 0" if len(position_key.split()) == 3 else position_key


def compact_priority(value: bytes | str) -> bytes:
    if isinstance(value, bytes):
        return value
    numeric = float(value)
    fixed = f"{numeric + 1_000_000:.6f}"
    whole, fraction = fixed.split(".", 1)
    component = (int(whole) * 1_000_000 + int(fraction)).to_bytes(8, "big")
    return component * 11


class CorpusStore:
    """Portable SQLite control plane using normalized corpus schema v5."""

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
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta'"
        ).fetchone()
        if has_meta is not None:
            row = self.connection.execute(
                "SELECT value FROM meta WHERE key='schema_version'"
            ).fetchone()
            if row is not None:
                version = int(row["value"])
                if version > SCHEMA_VERSION:
                    raise ValueError(f"database uses newer schema version {version}")
                if version < SCHEMA_VERSION:
                    raise ValueError(
                        f"database schema version {version} must be rebuilt as version "
                        f"{SCHEMA_VERSION} with prepare_book_corpus.py"
                    )
        with self.connection:
            self.connection.executescript(_SCHEMA_PATH.read_text(encoding="utf-8"))
            for key, value in (
                ("schema_version", str(SCHEMA_VERSION)),
                ("corpus_revision", "0"),
                ("progressive_width", "1"),
                ("zero_addition_rollouts", "0"),
            ):
                self.connection.execute(
                    "INSERT OR IGNORE INTO meta(key,value) VALUES(?,?)", (key, value)
                )

    def add_metric(self, name: str, value: int) -> None:
        with self.connection:
            self.connection.execute(
                """INSERT INTO metric_counter(name,value) VALUES(?,?)
                   ON CONFLICT(name) DO UPDATE SET value=metric_counter.value+excluded.value""",
                (name, value),
            )

    def metric(self, name: str) -> int:
        row = self.connection.execute(
            "SELECT value FROM metric_counter WHERE name=?", (name,)
        ).fetchone()
        return 0 if row is None else int(row["value"])

    def table_count(self, table: str) -> int:
        allowed = {
            "raw_source", "position", "logical_game", "source_game", "game_position",
            "ingest_error", "candidate", "candidate_adhoc_history", "candidate_adhoc_source",
            "search_task", "checkpoint",
        }
        if table not in allowed:
            raise ValueError(f"unsupported table: {table}")
        return int(self.connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0])

    def _meta_int(self, key: str) -> int:
        row = self.connection.execute("SELECT value FROM meta WHERE key=?", (key,)).fetchone()
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
                "UPDATE meta SET value=? WHERE key='zero_addition_rollouts'", (str(zero_count),)
            )
            pending = int(self.connection.execute(
                "SELECT COUNT(*) FROM search_task WHERE status IN (?,?)",
                (SearchTaskStatus.PENDING.value, SearchTaskStatus.RUNNING.value),
            ).fetchone()[0])
            if zero_count < saturation_window or pending != 0:
                return False
            old_width = self.progressive_width()
            self.connection.execute(
                "UPDATE meta SET value=? WHERE key='progressive_width'",
                (str(old_width + 1),),
            )
            self.connection.execute(
                """INSERT INTO progressive_width_history(old_width,new_width,changed_at,reason)
                   VALUES(?,?,?,?)""",
                (old_width, old_width + 1, time.time(), f"{saturation_window} zero-addition rollouts"),
            )
            self.connection.execute(
                "UPDATE meta SET value='0' WHERE key='zero_addition_rollouts'"
            )
            return True

    def bump_corpus_revision(self) -> int:
        with self.connection:
            revision = self.corpus_revision() + 1
            self.connection.execute(
                "UPDATE meta SET value=? WHERE key='corpus_revision'", (str(revision),)
            )
            self.connection.execute(
                "UPDATE meta SET value='0' WHERE key='zero_addition_rollouts'"
            )
        return revision

    def _ensure_position(self, position_key: str) -> int:
        key = canonical_position_key(position_key)
        self.connection.execute(
            "INSERT OR IGNORE INTO position(position_key) VALUES(?)", (key,)
        )
        row = self.connection.execute(
            "SELECT id FROM position WHERE position_key=?", (key,)
        ).fetchone()
        assert row is not None
        return int(row["id"])

    def upsert_candidate(
        self,
        position_key: str,
        move: str,
        history: Sequence[str],
        source: str,
        priority_key: bytes | str,
    ) -> int:
        """Insert a small ad-hoc candidate for tests/manual use; corpus ingest uses references."""
        now = time.time()
        key = compact_priority(priority_key)
        with self.connection:
            position_id = self._ensure_position(position_key)
            self.connection.execute(
                """INSERT INTO candidate(position_id,move,priority_key,active,created_at,updated_at)
                   VALUES(?,?,?,1,?,?)
                   ON CONFLICT(position_id,move) DO UPDATE SET
                     priority_key=MAX(candidate.priority_key,excluded.priority_key),
                     active=1,updated_at=excluded.updated_at""",
                (position_id, move, key, now, now),
            )
            row = self.connection.execute(
                "SELECT id,priority_key FROM candidate WHERE position_id=? AND move=?",
                (position_id, move),
            ).fetchone()
            assert row is not None
            candidate_id = int(row["id"])
            self.connection.execute(
                """INSERT INTO candidate_adhoc_history(candidate_id,position_sfen,history_json,source)
                   VALUES(?,?,?,?) ON CONFLICT(candidate_id) DO UPDATE SET
                   position_sfen=CASE WHEN ? >= (SELECT priority_key FROM candidate WHERE id=?)
                                      THEN excluded.position_sfen ELSE position_sfen END,
                   history_json=CASE WHEN ? >= (SELECT priority_key FROM candidate WHERE id=?)
                                     THEN excluded.history_json ELSE history_json END,
                   source=CASE WHEN ? >= (SELECT priority_key FROM candidate WHERE id=?)
                               THEN excluded.source ELSE source END""",
                (candidate_id, position_key, json.dumps(list(history), separators=(",", ":")), source,
                 key, candidate_id, key, candidate_id, key, candidate_id),
            )
            self.connection.execute(
                "INSERT OR IGNORE INTO candidate_adhoc_source(candidate_id,source) VALUES(?,?)",
                (candidate_id, source),
            )
        return candidate_id

    def upsert_ingested_candidate(
        self, *, position_id: int, move: str, game_id: int, ply: int,
        source_id: int, priority_key: bytes | str,
    ) -> int:
        now = time.time()
        key = compact_priority(priority_key)
        self.connection.execute(
            """INSERT INTO candidate(position_id,move,representative_game_id,representative_ply,
                                      source_id,priority_key,active,created_at,updated_at)
               VALUES(?,?,?,?,?,?,1,?,?)
               ON CONFLICT(position_id,move) DO UPDATE SET
                 representative_game_id=CASE WHEN excluded.priority_key>candidate.priority_key
                                             THEN excluded.representative_game_id ELSE candidate.representative_game_id END,
                 representative_ply=CASE WHEN excluded.priority_key>candidate.priority_key
                                         THEN excluded.representative_ply ELSE candidate.representative_ply END,
                 source_id=CASE WHEN excluded.priority_key>candidate.priority_key
                                THEN excluded.source_id ELSE candidate.source_id END,
                 priority_key=MAX(candidate.priority_key,excluded.priority_key),
                 active=1,updated_at=excluded.updated_at""",
            (position_id, move, game_id, ply, source_id, key, now, now),
        )
        row = self.connection.execute(
            "SELECT id FROM candidate WHERE position_id=? AND move=?", (position_id, move)
        ).fetchone()
        assert row is not None
        return int(row["id"])

    def candidate_source_count(self, position_key: str, move: str) -> int:
        key = canonical_position_key(position_key)
        row = self.connection.execute(
            """SELECT COUNT(DISTINCT source) FROM (
                 SELECT CAST(sg.source_id AS TEXT) AS source
                 FROM candidate c JOIN position p ON p.id=c.position_id
                 JOIN game_position gp ON gp.position_id=c.position_id AND gp.move=c.move
                 JOIN source_game sg ON sg.game_id=gp.game_id
                 WHERE p.position_key=? AND c.move=?
                 UNION
                 SELECT cas.source FROM candidate c JOIN position p ON p.id=c.position_id
                 JOIN candidate_adhoc_source cas ON cas.candidate_id=c.id
                 WHERE p.position_key=? AND c.move=?
               )""",
            (key, move, key, move),
        ).fetchone()
        return int(row[0])

    @staticmethod
    def _history_and_source(row: sqlite3.Row) -> tuple[tuple[str, ...], str]:
        if row["moves_json"] is not None:
            history = tuple(json.loads(row["moves_json"]))[: int(row["representative_ply"])]
            source = f"{row['site']}:{row['event']}:{row['relative_path']}"
            return history, source
        return tuple(json.loads(row["adhoc_history_json"] or "[]")), str(row["adhoc_source"] or "")

    def _candidate_query(self, *, by_position: bool) -> str:
        where = "c.active=1 AND p.position_key=?" if by_position else "c.active=1"
        return f"""SELECT c.id AS candidate_id,p.position_key,c.move,c.priority_key,
                    c.representative_ply,lg.moves_json,rs.site,rs.event,rs.relative_path,
                    ah.history_json AS adhoc_history_json,ah.source AS adhoc_source,
                    t.id AS task_id,t.status,t.attempts,t.lease_until
             FROM candidate c JOIN position p ON p.id=c.position_id
             LEFT JOIN logical_game lg ON lg.id=c.representative_game_id
             LEFT JOIN raw_source rs ON rs.id=c.source_id
             LEFT JOIN candidate_adhoc_history ah ON ah.candidate_id=c.id
             LEFT JOIN search_task t ON t.candidate_id=c.id AND t.book_snapshot_id=?
             WHERE {where} ORDER BY c.priority_key DESC,c.id LIMIT ?"""

    def reserve_for_position(
        self, book_snapshot_id: str, position_key: str, *, excluded_moves: set[str],
        width: int, lease_sec: float, site: Optional[str] = None,
        now: Optional[float] = None,
    ) -> Optional[CorpusCandidate]:
        if width < 1:
            raise ValueError("width must be positive")
        current_time = time.time() if now is None else now
        key = canonical_position_key(position_key)
        with self.connection:
            rows = self.connection.execute(
                self._candidate_query(by_position=True), (book_snapshot_id, key, width)
            ).fetchall()
            return self._reserve_from_rows(
                rows, book_snapshot_id, excluded_moves, lease_sec, site, current_time
            )

    def reserve_candidate(
        self, book_snapshot_id: str, *, now: Optional[float] = None, lease_sec: float,
    ) -> Optional[CorpusCandidate]:
        current_time = time.time() if now is None else now
        with self.connection:
            rows = self.connection.execute(
                self._candidate_query(by_position=False), (book_snapshot_id, 1)
            ).fetchall()
            return self._reserve_from_rows(
                rows, book_snapshot_id, set(), lease_sec, None, current_time
            )

    def _reserve_from_rows(
        self, rows: Sequence[sqlite3.Row], book_snapshot_id: str,
        excluded_moves: set[str], lease_sec: float, site: Optional[str], current_time: float,
    ) -> Optional[CorpusCandidate]:
        for item in rows:
            if (item["task_id"] is not None and item["move"] in excluded_moves
                    and item["status"] == SearchTaskStatus.PENDING.value):
                self.connection.execute(
                    "UPDATE search_task SET status=?,updated_at=? WHERE id=?",
                    (SearchTaskStatus.SUPERSEDED.value, current_time, item["task_id"]),
                )
        selected = None
        selected_source = ""
        for item in rows:
            _, source = self._history_and_source(item)
            eligible_task = (
                item["task_id"] is None
                or item["status"] == SearchTaskStatus.PENDING.value
                or (item["status"] == SearchTaskStatus.RUNNING.value
                    and item["lease_until"] is not None
                    and float(item["lease_until"]) <= current_time)
            )
            if (item["move"] not in excluded_moves and eligible_task
                    and (site is None or source.split(":", 1)[0] == site)):
                selected = item
                selected_source = source
                break
        if selected is None:
            return None
        attempts = int(selected["attempts"] or 0) + 1
        if selected["task_id"] is None:
            cursor = self.connection.execute(
                """INSERT INTO search_task(candidate_id,book_snapshot_id,status,attempts,
                                           lease_until,updated_at) VALUES(?,?,?,?,?,?)""",
                (selected["candidate_id"], book_snapshot_id, SearchTaskStatus.RUNNING.value,
                 attempts, current_time + lease_sec, current_time),
            )
            task_id = int(cursor.lastrowid)
        else:
            task_id = int(selected["task_id"])
            self.connection.execute(
                """UPDATE search_task SET status=?,attempts=?,lease_until=?,last_error=NULL,
                                          updated_at=? WHERE id=?""",
                (SearchTaskStatus.RUNNING.value, attempts, current_time + lease_sec,
                 current_time, task_id),
            )
        history, _ = self._history_and_source(selected)
        return CorpusCandidate(
            id=task_id, position_key=position_sfen(str(selected["position_key"])),
            move=str(selected["move"]), history=history, source=selected_source,
            priority_key=bytes(selected["priority_key"]), status=SearchTaskStatus.RUNNING,
            attempts=attempts, book_snapshot_id=book_snapshot_id,
        )

    def complete_search(self, task_id: int, *, eval_cp: int, response: str, depth: int,
                        nodes: int, engine_config_id: str) -> None:
        with self.connection:
            cursor = self.connection.execute(
                """UPDATE search_task SET status=?,lease_until=NULL,last_error=NULL,
                   eval_cp=?,response=?,depth=?,nodes=?,engine_config_id=?,
                   persisted_checkpoint_id=NULL,updated_at=? WHERE id=? AND status=?""",
                (SearchTaskStatus.EVALUATED.value, eval_cp, response, depth, nodes,
                 engine_config_id, time.time(), task_id, SearchTaskStatus.RUNNING.value),
            )
            if cursor.rowcount != 1:
                raise KeyError(f"running task not found: {task_id}")

    def fail_search(self, task_id: int, error: str, *, max_attempts: int) -> SearchTaskStatus:
        row = self.connection.execute(
            "SELECT attempts FROM search_task WHERE id=?", (task_id,)
        ).fetchone()
        if row is None:
            raise KeyError(task_id)
        status = (SearchTaskStatus.PERMANENT_FAILED
                  if int(row["attempts"]) >= max_attempts else SearchTaskStatus.PENDING)
        with self.connection:
            self.connection.execute(
                "UPDATE search_task SET status=?,lease_until=NULL,last_error=?,updated_at=? WHERE id=?",
                (status.value, error, time.time(), task_id),
            )
        return status

    def reset_interrupted_tasks(self) -> int:
        with self.connection:
            cursor = self.connection.execute(
                "UPDATE search_task SET status=?,lease_until=NULL,updated_at=? WHERE status=?",
                (SearchTaskStatus.PENDING.value, time.time(), SearchTaskStatus.RUNNING.value),
            )
        return int(cursor.rowcount)

    def has_checkpoint(self, book_hash: str) -> bool:
        return self.connection.execute(
            "SELECT 1 FROM checkpoint WHERE book_hash=? LIMIT 1", (book_hash,)
        ).fetchone() is not None

    def _query_results(self, condition: str, parameters: tuple[object, ...]) -> list[StoredSearchResult]:
        rows = self.connection.execute(
            f"""SELECT t.id AS task_id,COALESCE(ah.position_sfen,p.position_key || ' 0') AS position_key,c.move,t.eval_cp,t.response,t.depth,
                       t.nodes,t.engine_config_id,t.persisted_checkpoint_id
                FROM search_task t JOIN candidate c ON c.id=t.candidate_id
                JOIN position p ON p.id=c.position_id
                LEFT JOIN candidate_adhoc_history ah ON ah.candidate_id=c.id
                WHERE {condition} ORDER BY t.id""", parameters
        ).fetchall()
        return [StoredSearchResult(
            task_id=int(row["task_id"]), position_key=str(row["position_key"]),
            move=str(row["move"]), eval_cp=int(row["eval_cp"]), response=str(row["response"]),
            depth=int(row["depth"]), nodes=int(row["nodes"]),
            engine_config_id=str(row["engine_config_id"]),
            persisted_checkpoint_id=row["persisted_checkpoint_id"],
        ) for row in rows]

    def results_requiring_replay(self, book_hash: str) -> list[StoredSearchResult]:
        checkpoint_id = self.connection.execute(
            "SELECT MAX(id) FROM checkpoint WHERE book_hash=?", (book_hash,)
        ).fetchone()[0]
        if checkpoint_id is None:
            return self._query_results("t.status=?", (SearchTaskStatus.EVALUATED.value,))
        return self._query_results(
            "t.status=? AND (t.persisted_checkpoint_id IS NULL OR t.persisted_checkpoint_id>?)",
            (SearchTaskStatus.EVALUATED.value, int(checkpoint_id)),
        )

    def unpersisted_results(self) -> list[StoredSearchResult]:
        return self._query_results(
            "t.status=? AND t.persisted_checkpoint_id IS NULL",
            (SearchTaskStatus.EVALUATED.value,),
        )

    def record_checkpoint(self, book_hash: str) -> int:
        created_at = time.time()
        with self.connection:
            cursor = self.connection.execute(
                "INSERT INTO checkpoint(book_hash,created_at) VALUES(?,?)",
                (book_hash, created_at),
            )
            checkpoint_id = int(cursor.lastrowid)
            self.connection.execute(
                """UPDATE search_task SET persisted_checkpoint_id=?,updated_at=?
                   WHERE status=? AND persisted_checkpoint_id IS NULL""",
                (checkpoint_id, created_at, SearchTaskStatus.EVALUATED.value),
            )
        return checkpoint_id

    def checkpoint(self, checkpoint_id: int) -> Checkpoint:
        row = self.connection.execute(
            "SELECT id,book_hash,created_at FROM checkpoint WHERE id=?", (checkpoint_id,)
        ).fetchone()
        if row is None:
            raise KeyError(checkpoint_id)
        return Checkpoint(int(row["id"]), str(row["book_hash"]), float(row["created_at"]))
