from __future__ import annotations

import dataclasses
import hashlib
import json
import time
import unicodedata
from typing import Mapping, Optional, Sequence, Set, Tuple

try:
    from script.book_corpus import (
        CorpusStore,
        canonical_position_key,
        compact_priority,
    )
    from script.corpus_csa import CsaGameError, normalize_csa_game
except ImportError:
    from book_corpus import (
        CorpusStore,
        canonical_position_key,
        compact_priority,
    )
    from corpus_csa import CsaGameError, normalize_csa_game


DEFAULT_INGEST_BATCH_SIZE = 500


@dataclasses.dataclass(frozen=True)
class IngestResult:
    accepted: bool
    source_id: int
    game_id: Optional[int]
    error: Optional[str] = None


@dataclasses.dataclass(frozen=True)
class BatchIngestResult:
    accepted: int
    excluded: int


class AliasResolver:
    def __init__(self, aliases: Mapping[str, str]) -> None:
        self.aliases = {self.normalize(key): self.normalize(value) for key, value in aliases.items()}

    @staticmethod
    def normalize(name: str) -> str:
        return unicodedata.normalize("NFKC", name).strip()

    def resolve(self, name: str, official_names: Set[str]) -> Tuple[Optional[str], str]:
        normalized = self.normalize(name)
        normalized_official = {self.normalize(value): value for value in official_names}
        if normalized in normalized_official:
            return normalized_official[normalized], "exact"
        alias = self.aliases.get(normalized)
        if alias is not None and alias in normalized_official:
            return normalized_official[alias], "alias"
        return None, "unresolved"


def _logical_game_hash(players: tuple[str, str], moves: tuple[str, ...]) -> str:
    payload = json.dumps(
        {"players": list(players), "moves": list(moves)},
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _create_ingest_staging_tables(store: CorpusStore) -> None:
    store.connection.executescript(
        """
        CREATE TEMP TABLE IF NOT EXISTS stage_source(
            source_seq INTEGER PRIMARY KEY,
            relative_path TEXT NOT NULL,
            sha256 TEXT NOT NULL
        );
        CREATE TEMP TABLE IF NOT EXISTS stage_game(
            source_seq INTEGER PRIMARY KEY,
            game_hash TEXT NOT NULL,
            initial_sfen TEXT NOT NULL,
            black_name TEXT NOT NULL,
            white_name TEXT NOT NULL,
            moves_json TEXT NOT NULL
        );
        CREATE TEMP TABLE IF NOT EXISTS stage_position(
            source_seq INTEGER NOT NULL,
            ply INTEGER NOT NULL,
            position_key TEXT NOT NULL,
            move TEXT NOT NULL,
            PRIMARY KEY(source_seq, ply)
        ) WITHOUT ROWID;
        CREATE TEMP TABLE IF NOT EXISTS stage_error(
            source_seq INTEGER PRIMARY KEY,
            error TEXT NOT NULL,
            error_line INTEGER,
            previous_sfen TEXT,
            move TEXT,
            reason TEXT NOT NULL
        );
        """
    )


def ingest_csa_batch(
    store: CorpusStore,
    records: Sequence[tuple[str, str]],
    *,
    site: str,
    event: str,
    year: int,
    priority_key: bytes | str,
    retrieved_at: Optional[float] = None,
) -> BatchIngestResult:
    """Normalize and atomically ingest one bounded batch of CSA sources."""
    if not records:
        return BatchIngestResult(0, 0)

    sources: list[tuple[int, str, str]] = []
    games: list[tuple[int, str, str, str, str, str]] = []
    positions: list[tuple[int, int, str, str]] = []
    errors: list[tuple[int, str, Optional[int], Optional[str], Optional[str], str]] = []
    accepted = 0
    excluded = 0
    for source_seq, (relative_path, text) in enumerate(records, start=1):
        source_sha = hashlib.sha256(text.encode("utf-8")).hexdigest()
        sources.append((source_seq, relative_path, source_sha))
        try:
            game = normalize_csa_game(text, source_path=relative_path)
        except CsaGameError as error:
            excluded += 1
            errors.append(
                (
                    source_seq,
                    str(error),
                    error.line_number,
                    error.previous_sfen,
                    error.move,
                    error.reason,
                )
            )
            continue

        accepted += 1
        games.append(
            (
                source_seq,
                _logical_game_hash(game.players, game.moves),
                game.positions[0].sfen,
                game.players[0],
                game.players[1],
                json.dumps(list(game.moves), separators=(",", ":")),
            )
        )
        positions.extend(
            (
                source_seq,
                position.ply,
                canonical_position_key(position.sfen),
                position.move,
            )
            for position in game.positions
        )

    timestamp = time.time() if retrieved_at is None else retrieved_at
    now = time.time()
    key = compact_priority(priority_key)
    _create_ingest_staging_tables(store)
    connection = store.connection
    with connection:
        connection.execute("DELETE FROM stage_error")
        connection.execute("DELETE FROM stage_position")
        connection.execute("DELETE FROM stage_game")
        connection.execute("DELETE FROM stage_source")
        connection.executemany(
            "INSERT INTO stage_source(source_seq,relative_path,sha256) VALUES(?,?,?)",
            sources,
        )
        connection.executemany(
            """INSERT INTO stage_game(
                   source_seq,game_hash,initial_sfen,black_name,white_name,moves_json
               ) VALUES(?,?,?,?,?,?)""",
            games,
        )
        connection.executemany(
            """INSERT INTO stage_position(source_seq,ply,position_key,move)
               VALUES(?,?,?,?)""",
            positions,
        )
        connection.executemany(
            """INSERT INTO stage_error(
                   source_seq,error,error_line,previous_sfen,move,reason
               ) VALUES(?,?,?,?,?,?)""",
            errors,
        )
        connection.execute(
            """INSERT OR IGNORE INTO raw_source(
                   site,event,year,relative_path,sha256,retrieved_at
               )
               SELECT ?,?,?,relative_path,sha256,?
               FROM stage_source ORDER BY source_seq""",
            (site, event, year, timestamp),
        )
        connection.execute(
            """INSERT INTO ingest_error(
                   source_id,error,error_line,previous_sfen,move,reason,created_at
               )
               SELECT rs.id,se.error,se.error_line,se.previous_sfen,se.move,se.reason,?
               FROM stage_error se
               JOIN stage_source ss ON ss.source_seq=se.source_seq
               JOIN raw_source rs
                 ON rs.site=? AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
               ORDER BY se.source_seq""",
            (timestamp, site),
        )
        connection.execute(
            """INSERT OR IGNORE INTO logical_game(
                   game_hash,initial_sfen,black_name,white_name,moves_json,primary_source_id
               )
               SELECT sg.game_hash,sg.initial_sfen,sg.black_name,sg.white_name,
                      sg.moves_json,rs.id
               FROM stage_game sg
               JOIN stage_source ss ON ss.source_seq=sg.source_seq
               JOIN raw_source rs
                 ON rs.site=? AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
               ORDER BY sg.source_seq""",
            (site,),
        )
        connection.execute(
            """INSERT OR IGNORE INTO source_game(source_id,game_id)
               SELECT rs.id,lg.id
               FROM stage_game sg
               JOIN stage_source ss ON ss.source_seq=sg.source_seq
               JOIN raw_source rs
                 ON rs.site=? AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
               JOIN logical_game lg ON lg.game_hash=sg.game_hash
               ORDER BY sg.source_seq""",
            (site,),
        )
        connection.execute(
            """INSERT OR IGNORE INTO position(position_key)
               SELECT position_key FROM stage_position
               GROUP BY position_key
               ORDER BY MIN(source_seq),MIN(ply)"""
        )
        connection.execute(
            """INSERT OR IGNORE INTO game_position(game_id,ply,position_id,move)
               SELECT lg.id,sp.ply,p.id,sp.move
               FROM stage_position sp
               JOIN stage_game sg ON sg.source_seq=sp.source_seq
               JOIN logical_game lg ON lg.game_hash=sg.game_hash
               JOIN position p ON p.position_key=sp.position_key
               ORDER BY sp.source_seq,sp.ply"""
        )
        connection.execute(
            """WITH ranked AS (
                   SELECT gp.position_id,gp.move,lg.id AS game_id,gp.ply,
                          rs.id AS source_id,sg.source_seq,
                          ROW_NUMBER() OVER (
                              PARTITION BY gp.position_id,gp.move
                              ORDER BY sg.source_seq,gp.ply
                          ) AS candidate_rank
                   FROM stage_game sg
                   JOIN stage_source ss ON ss.source_seq=sg.source_seq
                   JOIN raw_source rs
                     ON rs.site=? AND rs.relative_path=ss.relative_path AND rs.sha256=ss.sha256
                   JOIN logical_game lg ON lg.game_hash=sg.game_hash
                   JOIN game_position gp ON gp.game_id=lg.id
               )
               INSERT INTO candidate(
                   position_id,move,representative_game_id,representative_ply,
                   source_id,priority_key,active,created_at,updated_at
               )
               SELECT position_id,move,game_id,ply,source_id,?,1,?,?
               FROM ranked WHERE candidate_rank=1
               ORDER BY source_seq,ply
               ON CONFLICT(position_id,move) DO UPDATE SET
                 representative_game_id=CASE WHEN excluded.priority_key>candidate.priority_key
                                             THEN excluded.representative_game_id ELSE candidate.representative_game_id END,
                 representative_ply=CASE WHEN excluded.priority_key>candidate.priority_key
                                         THEN excluded.representative_ply ELSE candidate.representative_ply END,
                 source_id=CASE WHEN excluded.priority_key>candidate.priority_key
                                THEN excluded.source_id ELSE candidate.source_id END,
                 priority_key=MAX(candidate.priority_key,excluded.priority_key),
                 active=1,updated_at=excluded.updated_at""",
            (site, key, now, now),
        )
    return BatchIngestResult(accepted, excluded)


def ingest_csa_text(
    store: CorpusStore,
    text: str,
    *,
    site: str,
    event: str,
    year: int,
    relative_path: str,
    priority_key: str,
    retrieved_at: Optional[float] = None,
) -> IngestResult:
    """Transactionally ingest one raw CSA source and its normalized logical game."""
    source_sha = hashlib.sha256(text.encode("utf-8")).hexdigest()
    timestamp = time.time() if retrieved_at is None else retrieved_at
    with store.connection:
        store.connection.execute(
            """
            INSERT OR IGNORE INTO raw_source(
                site, event, year, relative_path, sha256, retrieved_at
            ) VALUES(?, ?, ?, ?, ?, ?)
            """,
            (site, event, year, relative_path, source_sha, timestamp),
        )
        source_row = store.connection.execute(
            """SELECT id FROM raw_source
               WHERE site = ? AND relative_path = ? AND sha256 = ?""",
            (site, relative_path, source_sha),
        ).fetchone()
        assert source_row is not None
        source_id = int(source_row["id"])

    try:
        game = normalize_csa_game(text, source_path=relative_path)
    except CsaGameError as error:
        with store.connection:
            store.connection.execute(
                """INSERT INTO ingest_error(
                       source_id, error, error_line, previous_sfen, move, reason, created_at
                   ) VALUES(?, ?, ?, ?, ?, ?, ?)""",
                (source_id, str(error), error.line_number, error.previous_sfen,
                 error.move, error.reason, timestamp),
            )
        return IngestResult(False, source_id, None, str(error))

    game_hash = _logical_game_hash(game.players, game.moves)
    with store.connection:
        cursor = store.connection.execute(
            """
            INSERT OR IGNORE INTO logical_game(
                game_hash, initial_sfen, black_name, white_name, moves_json, primary_source_id
            ) VALUES(?, ?, ?, ?, ?, ?)
            """,
            (
                game_hash,
                game.positions[0].sfen,
                game.players[0],
                game.players[1],
                json.dumps(list(game.moves), separators=(",", ":")),
                source_id,
            ),
        )
        game_row = store.connection.execute(
            "SELECT id FROM logical_game WHERE game_hash = ?", (game_hash,)
        ).fetchone()
        assert game_row is not None
        game_id = int(game_row["id"])
        is_new_game = cursor.rowcount == 1
        store.connection.execute(
            "INSERT OR IGNORE INTO source_game(source_id, game_id) VALUES(?, ?)",
            (source_id, game_id),
        )
        if is_new_game:
            for position in game.positions:
                position_id = store._ensure_position(position.sfen)
                store.connection.execute(
                    """INSERT INTO game_position(game_id, ply, position_id, move)
                       VALUES(?, ?, ?, ?)""",
                    (game_id, position.ply, position_id, position.move),
                )
        positions = store.connection.execute(
            "SELECT ply, position_id, move FROM game_position WHERE game_id = ? ORDER BY ply",
            (game_id,),
        ).fetchall()
        for position in positions:
            store.upsert_ingested_candidate(
                position_id=int(position["position_id"]),
                move=str(position["move"]),
                game_id=game_id,
                ply=int(position["ply"]),
                source_id=source_id,
                priority_key=priority_key,
            )
    return IngestResult(True, source_id, game_id)