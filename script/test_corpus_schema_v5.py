from __future__ import annotations

import pathlib
import tempfile
import unittest

from script.book_corpus import CorpusStore
from script.corpus_ingest import ingest_csa_text
from script.corpus_priority import encode_priority


CSA = """V2.2
N+Alpha
N-Beta
PI
+
+7776FU
-3334FU
%TORYO
"""


class CorpusSchemaV5Test(unittest.TestCase):
    def test_schema_normalizes_positions_and_has_no_per_position_history(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                self.assertEqual(store.schema_version(), 5)
                tables = {
                    row["name"]
                    for row in store.connection.execute(
                        "SELECT name FROM sqlite_schema WHERE type = 'table'"
                    )
                }
                self.assertIn("position", tables)
                self.assertNotIn("candidate_source", tables)
                game_position_columns = {
                    row["name"] for row in store.connection.execute("PRAGMA table_info(game_position)")
                }
                candidate_columns = {
                    row["name"] for row in store.connection.execute("PRAGMA table_info(candidate)")
                }
                self.assertNotIn("history_json", game_position_columns)
                self.assertNotIn("sfen", game_position_columns)
                self.assertNotIn("position_key", game_position_columns)
                self.assertNotIn("history_json", candidate_columns)
                self.assertNotIn("source", candidate_columns)
                self.assertIn("representative_game_id", candidate_columns)
                self.assertIn("representative_ply", candidate_columns)

    def test_candidate_rejects_non_fixed_width_priority_blob(self) -> None:
        import sqlite3

        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                store.connection.execute("INSERT INTO position(position_key) VALUES('p')")
                position_id = store.connection.execute(
                    "SELECT id FROM position WHERE position_key='p'"
                ).fetchone()[0]
                with self.assertRaises(sqlite3.IntegrityError):
                    store.connection.execute(
                        """INSERT INTO candidate(position_id,move,priority_key,created_at,updated_at)
                           VALUES(?, '7g7f', ?, 0, 0)""",
                        (position_id, b"short"),
                    )
    def test_ingested_candidate_reconstructs_history_from_logical_game(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                ingest_csa_text(
                    store,
                    CSA,
                    site="wcsc",
                    event="wcsc36",
                    year=2026,
                    relative_path="a.csa",
                    priority_key=encode_priority((1.0,) * 11),
                )
                second = store.connection.execute(
                    """SELECT p.position_key AS sfen, gp.move
                       FROM game_position gp JOIN position p ON p.id = gp.position_id
                       ORDER BY gp.ply DESC LIMIT 1"""
                ).fetchone()
                task = store.reserve_for_position(
                    "book-a", str(second["sfen"]), excluded_moves=set(), width=1, lease_sec=10
                )
                self.assertIsNotNone(task)
                assert task is not None
                self.assertEqual(task.move, "3c3d")
                self.assertEqual(task.history, ("7g7f",))
                self.assertEqual(task.source, "wcsc:wcsc36:a.csa")

    def test_priority_is_fixed_width_binary_and_preserves_tuple_order(self) -> None:
        low = encode_priority((0.0,) * 11)
        high = encode_priority((0.0,) * 10 + (1.0,))
        earlier_component = encode_priority((1.0,) + (-999999.0,) * 10)
        self.assertIsInstance(low, bytes)
        self.assertEqual(len(low), 88)
        self.assertLess(low, high)
        self.assertGreater(earlier_component, high)


    def test_priority_blob_matches_tuple_order_at_boundaries(self) -> None:
        values = [
            (-999999.999999,) + (0.0,) * 10,
            (0.0,) * 11,
            (0.0, 0.0, -0.5) + (0.0,) * 8,
            (0.0, 0.0, 0.123456) + (0.0,) * 7 + (-1.0,),
            (0.0, 0.0, 0.123456) + (0.0,) * 7 + (2147483647.123456,),
            (100.0,) + (0.0,) * 10,
        ]
        tuple_order = sorted(range(len(values)), key=lambda index: values[index])
        blob_order = sorted(range(len(values)), key=lambda index: encode_priority(values[index]))
        self.assertEqual(blob_order, tuple_order)
        self.assertEqual(encode_priority(values[1]), encode_priority(tuple(values[1])))

if __name__ == "__main__":
    unittest.main()
