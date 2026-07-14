from __future__ import annotations

import pathlib
import sqlite3
import tempfile
import unittest

from script.book_corpus import CorpusStore
from script.compare_corpus_schemas import compare_corpus_schemas
from script.corpus_ingest import ingest_csa_text
from script.corpus_priority import PriorityFacts, encode_priority, priority_tuple


CSA = """V2.2
N+Alpha
N-Beta
PI
+
+7776FU
-3334FU
%TORYO
"""


class CompareCorpusSchemasTest(unittest.TestCase):
    def test_equivalent_v4_and_v5_databases_have_identical_stream_hashes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            new_path = root / "new.sqlite"
            values = priority_tuple(PriorityFacts(year=2026))
            with CorpusStore(new_path) as store:
                ingest_csa_text(
                    store, CSA, site="wcsc", event="wcsc36", year=2026,
                    relative_path="a.csa", priority_key=encode_priority(values), retrieved_at=1.0,
                )
                game = store.connection.execute("SELECT * FROM logical_game").fetchone()
                raw_source = store.connection.execute("SELECT * FROM raw_source").fetchone()
                positions = store.connection.execute(
                    """SELECT gp.ply,p.position_key,gp.move FROM game_position gp
                       JOIN position p ON p.id=gp.position_id ORDER BY gp.ply"""
                ).fetchall()
            old_path = root / "old.sqlite"
            old = sqlite3.connect(old_path)
            old.executescript("""
                CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
                CREATE TABLE raw_source(id INTEGER PRIMARY KEY,site TEXT,event TEXT,year INTEGER,
                    relative_path TEXT,sha256 TEXT,retrieved_at REAL);
                CREATE TABLE logical_game(id INTEGER PRIMARY KEY,game_hash TEXT,initial_sfen TEXT,
                    black_name TEXT,white_name TEXT,moves_json TEXT);
                CREATE TABLE source_game(source_id INTEGER,game_id INTEGER);
                CREATE TABLE game_position(game_id INTEGER,ply INTEGER,site TEXT,event TEXT,year INTEGER,
                    position_key TEXT,sfen TEXT,history_json TEXT,move TEXT);
                CREATE TABLE candidate(id INTEGER PRIMARY KEY,position_key TEXT,move TEXT,
                    history_json TEXT,source TEXT,priority_key TEXT,active INTEGER,created_at REAL,updated_at REAL);
                CREATE TABLE candidate_source(candidate_id INTEGER,source TEXT,history_json TEXT);
                CREATE TABLE ingest_error(id INTEGER PRIMARY KEY,source_id INTEGER,error TEXT,
                    error_line INTEGER,previous_sfen TEXT,move TEXT,reason TEXT,created_at REAL);
            """)
            old.execute("INSERT INTO meta VALUES('schema_version','4')")
            old.execute(
                "INSERT INTO raw_source VALUES(?,?,?,?,?,?,?)",
                (1, raw_source["site"], raw_source["event"], raw_source["year"],
                 raw_source["relative_path"], raw_source["sha256"], raw_source["retrieved_at"]),
            )
            old.execute(
                "INSERT INTO logical_game VALUES(?,?,?,?,?,?)",
                (1, game["game_hash"], game["initial_sfen"], game["black_name"],
                 game["white_name"], game["moves_json"]),
            )
            old.execute("INSERT INTO source_game VALUES(1,1)")
            old_priority = "|".join(f"{value + 1_000_000:020.6f}" for value in values)
            history = []
            for ply, position_key, move in positions:
                sfen = f"{position_key} {int(ply) + 1}"
                import json
                old.execute(
                    "INSERT INTO game_position VALUES(?,?,?,?,?,?,?,?,?)",
                    (1, ply, 'wcsc', 'wcsc36', 2026, sfen, sfen,
                     json.dumps(history, separators=(',', ':')), move),
                )
                old.execute(
                    "INSERT INTO candidate VALUES(?,?,?,?,?,?,?,?,?)",
                    (int(ply) + 1, sfen, move, json.dumps(history, separators=(',', ':')),
                     'wcsc:wcsc36:a.csa', old_priority, 1, 1.0, 1.0),
                )
                old.execute(
                    "INSERT INTO candidate_source VALUES(?,?,?)",
                    (int(ply) + 1, 'wcsc:wcsc36:a.csa', json.dumps(history, separators=(',', ':'))),
                )
                history.append(move)
            old.commit()
            old.close()

            report = compare_corpus_schemas(old_path, new_path)
            self.assertEqual(report["mismatches"], [])
            self.assertEqual(report["streams"]["positions"]["count"], 2)


if __name__ == "__main__":
    unittest.main()
