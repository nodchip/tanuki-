from __future__ import annotations

import dataclasses
import hashlib
import json
import time
import unicodedata
from typing import Mapping, Optional, Set, Tuple

try:
    from script.book_corpus import CorpusStore
    from script.corpus_csa import CsaGameError, normalize_csa_game
except ImportError:
    from book_corpus import CorpusStore
    from corpus_csa import CsaGameError, normalize_csa_game


@dataclasses.dataclass(frozen=True)
class IngestResult:
    accepted: bool
    source_id: int
    game_id: Optional[int]
    error: Optional[str] = None


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