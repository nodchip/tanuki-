from __future__ import annotations

import pathlib
import sqlite3
import tempfile
import unittest

from script.book_corpus import CorpusStore
from script.corpus_ingest import AliasResolver, ingest_csa_batch, ingest_csa_text


CSA = """V2.2
N+Alpha
N-Beta
PI
+
+7776FU
-3334FU
%TORYO
"""


class CorpusIngestTest(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.store = CorpusStore(pathlib.Path(temporary.name) / "corpus.sqlite")
        self.addCleanup(self.store.close)

    def test_exact_duplicate_sources_share_one_logical_game_and_keep_provenance(self) -> None:
        first = ingest_csa_text(
            self.store, CSA, site="wcsc", event="wcsc36", year=2026,
            relative_path="wcsc/a.csa", priority_key="9",
        )
        second = ingest_csa_text(
            self.store, CSA, site="wcsc", event="wcsc36", year=2026,
            relative_path="mirror/a.csa", priority_key="9",
        )

        self.assertTrue(first.accepted)
        self.assertTrue(second.accepted)
        self.assertEqual(self.store.table_count("raw_source"), 2)
        self.assertEqual(self.store.table_count("logical_game"), 1)
        self.assertEqual(self.store.table_count("game_position"), 2)
        self.assertEqual(self.store.table_count("candidate"), 2)

    def test_invalid_game_is_recorded_and_excluded(self) -> None:
        broken = CSA.replace("-3334FU", "-9998FU")
        result = ingest_csa_text(
            self.store, broken, site="denryu", event="denryu6", year=2025,
            relative_path="broken.csa", priority_key="8",
        )
        self.assertFalse(result.accepted)
        self.assertEqual(self.store.table_count("ingest_error"), 1)
        self.assertEqual(self.store.table_count("logical_game"), 0)

    def test_batch_ingest_matches_sequential_content_and_representative(self) -> None:
        broken = CSA.replace("-3334FU", "-9998FU")
        records = [
            ("wcsc/a.csa", CSA),
            ("mirror/a.csa", CSA),
            ("broken.csa", broken),
        ]
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(
                pathlib.Path(temporary_directory) / "sequential.sqlite"
            ) as sequential:
                for relative_path, text in records:
                    ingest_csa_text(
                        sequential,
                        text,
                        site="wcsc",
                        event="wcsc36",
                        year=2026,
                        relative_path=relative_path,
                        priority_key="9",
                        retrieved_at=123.0,
                    )

                result = ingest_csa_batch(
                    self.store,
                    records,
                    site="wcsc",
                    event="wcsc36",
                    year=2026,
                    priority_key="9",
                    retrieved_at=123.0,
                )

                self.assertEqual((result.accepted, result.excluded), (2, 1))
                for table in (
                    "raw_source",
                    "logical_game",
                    "position",
                    "game_position",
                    "candidate",
                    "ingest_error",
                ):
                    self.assertEqual(
                        self.store.table_count(table),
                        sequential.table_count(table),
                        table,
                    )
                representative_query = """
                    SELECT p.position_key,c.move,rs.relative_path,gp.ply
                    FROM candidate c
                    JOIN position p ON p.id=c.position_id
                    JOIN raw_source rs ON rs.id=c.source_id
                    JOIN game_position gp
                      ON gp.game_id=c.representative_game_id
                     AND gp.ply=c.representative_ply
                    ORDER BY p.position_key,c.move
                """
                self.assertEqual(
                    [
                        tuple(row)
                        for row in self.store.connection.execute(representative_query)
                    ],
                    [
                        tuple(row)
                        for row in sequential.connection.execute(representative_query)
                    ],
                )

    def test_batch_ingest_rolls_back_all_persistent_rows_on_sql_failure(self) -> None:
        self.store.connection.execute(
            """
            CREATE TEMP TRIGGER reject_logical_game
            BEFORE INSERT ON logical_game
            BEGIN
                SELECT RAISE(ABORT, 'forced batch failure');
            END
            """
        )

        with self.assertRaisesRegex(sqlite3.IntegrityError, "forced batch failure"):
            ingest_csa_batch(
                self.store,
                [("wcsc/a.csa", CSA)],
                site="wcsc",
                event="wcsc36",
                year=2026,
                priority_key="9",
                retrieved_at=123.0,
            )

        for table in (
            "raw_source",
            "logical_game",
            "position",
            "game_position",
            "candidate",
            "ingest_error",
        ):
            self.assertEqual(self.store.table_count(table), 0, table)

    def test_alias_resolution_is_exact_or_explicit_never_fuzzy(self) -> None:
        resolver = AliasResolver({"Alpha v2": "Alpha"})
        self.assertEqual(resolver.resolve(" Alpha ", {"Alpha"}), ("Alpha", "exact"))
        self.assertEqual(resolver.resolve("Alpha v2", {"Alpha"}), ("Alpha", "alias"))
        self.assertEqual(resolver.resolve("Alph", {"Alpha"}), (None, "unresolved"))


if __name__ == "__main__":
    unittest.main()