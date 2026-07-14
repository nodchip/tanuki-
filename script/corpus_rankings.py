from __future__ import annotations

import dataclasses
import unicodedata
from collections import defaultdict
from typing import Iterable

try:
    from script.book_corpus import CorpusStore
    from script.corpus_priority import POLICY_VERSION, PriorityFacts, encode_priority, priority_tuple
except ImportError:
    from book_corpus import CorpusStore
    from corpus_priority import POLICY_VERSION, PriorityFacts, encode_priority, priority_tuple


@dataclasses.dataclass(frozen=True)
class FloodgateRatingResult:
    player_name: str
    rating: float
    effective_games: float
    connected_to_anchor: bool
    anchor_margin: float
    snapshot_percentile: float

    @property
    def reliability_bucket(self) -> int:
        if not self.connected_to_anchor:
            return 1
        return 3 if self.effective_games >= 50 else 2 if self.effective_games >= 15 else 1

@dataclasses.dataclass(frozen=True)
class TournamentResult:
    official_name: str
    stage: str
    stage_tier: int
    rank: int
    participants: int

    @property
    def percentile(self) -> float:
        return 1.0 - (self.rank - 1) / max(self.participants - 1, 1)


def _normalize(value: str) -> str:
    return unicodedata.normalize("NFKC", value).strip()


def import_tournament_ranking(
    store: CorpusStore,
    *, site: str, event: str, source_url: str, source_sha256: str,
    retrieved_at: float, provisional: bool, results: Iterable[TournamentResult],
) -> int:
    values = list(results)
    with store.connection:
        store.connection.execute(
            "UPDATE ranking_snapshot SET active = 0 WHERE site = ? AND event = ?",
            (site, event),
        )
        cursor = store.connection.execute(
            """INSERT INTO ranking_snapshot(site, event, source_url, retrieved_at,
               source_sha256, provisional, policy_version, active)
               VALUES(?, ?, ?, ?, ?, ?, ?, 1)""",
            (site, event, source_url, retrieved_at, source_sha256,
             int(provisional), POLICY_VERSION),
        )
        snapshot_id = int(cursor.lastrowid)
        for result in values:
            store.connection.execute(
                """INSERT OR IGNORE INTO participant(event, official_name, normalized_name)
                   VALUES(?, ?, ?)""",
                (event, result.official_name, _normalize(result.official_name)),
            )
            participant = store.connection.execute(
                "SELECT id FROM participant WHERE event = ? AND official_name = ?",
                (event, result.official_name),
            ).fetchone()
            store.connection.execute(
                """INSERT INTO ranking_result(snapshot_id, participant_id, stage,
                   stage_tier, rank, participants, rank_percentile)
                   VALUES(?, ?, ?, ?, ?, ?, ?)""",
                (snapshot_id, participant["id"], result.stage, result.stage_tier,
                 result.rank, result.participants, result.percentile),
            )
    return snapshot_id


def import_floodgate_rating(
    store: CorpusStore,
    *, year: int, snapshot_time: float, source_url: str, source_sha256: str,
    anchor_era: str, rated_player_count: int, component_size: int,
    anchor_connected_rate: float, results: Iterable[FloodgateRatingResult],
    medium_games: int = 15, high_games: int = 50, min_component_size: int = 10,
) -> int:
    with store.connection:
        store.connection.execute("UPDATE rating_snapshot SET active = 0 WHERE year = ?", (year,))
        cursor = store.connection.execute(
            """INSERT INTO rating_snapshot(year, snapshot_time, source_url, source_sha256,
               anchor_era, rated_player_count, component_size, anchor_connected_rate, active)
               VALUES(?, ?, ?, ?, ?, ?, ?, ?, 1)""",
            (year, snapshot_time, source_url, source_sha256, anchor_era,
             rated_player_count, component_size, anchor_connected_rate),
        )
        snapshot_id = int(cursor.lastrowid)
        for result in results:
            store.connection.execute(
                """INSERT INTO rating_result(snapshot_id, player_name, rating,
                   effective_games, connected_to_anchor, reliability_bucket,
                   anchor_margin, snapshot_percentile) VALUES(?, ?, ?, ?, ?, ?, ?, ?)""",
                (snapshot_id, _normalize(result.player_name), result.rating,
                 result.effective_games, int(result.connected_to_anchor),
                 1 if component_size < min_component_size else (
                     3 if result.connected_to_anchor and result.effective_games >= high_games
                     else 2 if result.connected_to_anchor and result.effective_games >= medium_games
                     else 1
                 ), result.anchor_margin,
                 result.snapshot_percentile),
            )
    return snapshot_id


def _rating_result(store: CorpusStore, year: int, player_name: str):
    return store.connection.execute(
        """SELECT rr.reliability_bucket, rr.anchor_margin, rr.snapshot_percentile
           FROM rating_snapshot rs JOIN rating_result rr ON rr.snapshot_id = rs.id
           WHERE rs.active = 1 AND rs.year = ? AND rr.player_name = ?
           ORDER BY rs.snapshot_time DESC LIMIT 1""",
        (year, _normalize(player_name)),
    ).fetchone()

def add_participant_alias(store: CorpusStore, *, event: str, alias: str, official_name: str) -> None:
    participant = store.connection.execute(
        "SELECT id FROM participant WHERE event = ? AND official_name = ?",
        (event, official_name),
    ).fetchone()
    if participant is None:
        raise KeyError(official_name)
    with store.connection:
        store.connection.execute(
            """INSERT INTO participant_alias(event, alias, participant_id, mapping_status)
               VALUES(?, ?, ?, 'manual')
               ON CONFLICT(event, alias) DO UPDATE SET participant_id=excluded.participant_id,
               mapping_status=excluded.mapping_status""",
            (event, _normalize(alias), participant["id"]),
        )


def _tournament_result(store: CorpusStore, event: str, player_name: str):
    normalized = _normalize(player_name)
    return store.connection.execute(
        """
        SELECT rr.stage_tier, rr.rank_percentile
        FROM ranking_snapshot rs
        JOIN ranking_result rr ON rr.snapshot_id = rs.id
        JOIN participant p ON p.id = rr.participant_id
        LEFT JOIN participant_alias pa
          ON pa.participant_id = p.id AND pa.event = p.event
        WHERE rs.active = 1 AND rs.event = ?
          AND (p.normalized_name = ? OR pa.alias = ?)
        ORDER BY rs.provisional ASC, rs.retrieved_at DESC LIMIT 1
        """, (event, normalized, normalized),
    ).fetchone()


def recompute_candidate_priorities(store: CorpusStore) -> int:
    cursor = store.connection.execute(
        """
        SELECT c.id AS candidate_id, rs.site, rs.event, rs.year, gp.ply,
               gp.game_id, lg.black_name, lg.white_name, lg.primary_source_id
        FROM game_position gp
        JOIN candidate c ON c.position_id = gp.position_id AND c.move = gp.move
        JOIN logical_game lg ON lg.id = gp.game_id
        JOIN raw_source rs ON rs.id = lg.primary_source_id
        ORDER BY gp.game_id, gp.ply
        """
    )
    tournament_cache: dict[tuple[str, str], object] = {}
    rating_cache: dict[tuple[int, str], object] = {}
    updates: list[tuple[bytes, int, int, int, int, bytes]] = []
    for row in cursor:
        mover = str(row["black_name"] if int(row["ply"]) % 2 == 0 else row["white_name"])
        opponent = str(row["white_name"] if int(row["ply"]) % 2 == 0 else row["black_name"])
        event = str(row["event"])
        mover_key = (event, mover)
        opponent_key = (event, opponent)
        if mover_key not in tournament_cache:
            tournament_cache[mover_key] = _tournament_result(store, event, mover)
        if opponent_key not in tournament_cache:
            tournament_cache[opponent_key] = _tournament_result(store, event, opponent)
        mover_result = tournament_cache[mover_key]
        opponent_result = tournament_cache[opponent_key]
        rating_result = None
        if row["site"] == "floodgate":
            rating_key = (int(row["year"]), mover)
            if rating_key not in rating_cache:
                rating_cache[rating_key] = _rating_result(store, rating_key[0], mover)
            rating_result = rating_cache[rating_key]
        facts = PriorityFacts(
            year=int(row["year"]),
            stage_tier=int(mover_result["stage_tier"]) if mover_result else 0,
            mover_percentile=float(mover_result["rank_percentile"]) if mover_result else 0.0,
            opponent_stage_tier=int(opponent_result["stage_tier"]) if opponent_result else 0,
            opponent_percentile=float(opponent_result["rank_percentile"]) if opponent_result else 0.0,
            rating_reliability=int(rating_result["reliability_bucket"]) if rating_result else 0,
            anchor_margin=float(rating_result["anchor_margin"]) if rating_result else 0.0,
            rating_percentile=float(rating_result["snapshot_percentile"]) if rating_result else 0.0,
        )
        priority = encode_priority(priority_tuple(facts))
        updates.append((
            priority, int(row["game_id"]), int(row["ply"]),
            int(row["primary_source_id"]), int(row["candidate_id"]), priority,
        ))
        if len(updates) >= 10_000:
            _apply_priority_updates(store, updates)
            updates.clear()
    cursor.close()
    if updates:
        _apply_priority_updates(store, updates)
    return int(store.connection.execute("SELECT COUNT(*) FROM candidate").fetchone()[0])


def _apply_priority_updates(
    store: CorpusStore, updates: list[tuple[bytes, int, int, int, int, bytes]]
) -> None:
    with store.connection:
        store.connection.executemany(
            """UPDATE candidate SET priority_key=?, representative_game_id=?,
               representative_ply=?, source_id=?, updated_at=strftime('%s','now')
               WHERE id=? AND priority_key < ?""",
            updates,
        )
