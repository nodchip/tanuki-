from __future__ import annotations

import pathlib
import tempfile
import unittest

from script.book_corpus import CorpusStore
from script.corpus_coverage import generate_coverage_report
from script.corpus_ingest import ingest_csa_text
from script.extend_book_mcts import BookEntry, OpeningBook


CSA = """V2.2
N+Alpha
N-Beta
PI
+
+7776FU
-3334FU
%TORYO
"""


class CorpusCoverageTest(unittest.TestCase):
    def test_reports_unique_and_occurrence_coverage_with_snapshot_identity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                ingest_csa_text(
                    store, CSA, site="wcsc", event="wcsc36", year=2026,
                    relative_path="a.csa", priority_key="9",
                )
                first_sfen = store.connection.execute(
                    "SELECT sfen FROM game_position ORDER BY ply LIMIT 1"
                ).fetchone()["sfen"]
                book = OpeningBook()
                position = book.ensure_position(first_sfen)
                position.entries.append(BookEntry("7g7f", "3c3d", 10, 1, 0, 0))

                report = generate_coverage_report(
                    store, book, snapshot_id="pilot-001", book_hash="abc123"
                )

        self.assertEqual(report["snapshot_id"], "pilot-001")
        self.assertEqual(report["book_hash"], "abc123")
        self.assertEqual(report["corpus_revision"], 0)
        self.assertEqual(report["overall"]["unique"], {"covered": 1, "total": 2, "rate": 0.5})
        self.assertEqual(report["overall"]["occurrences"], {"covered": 1, "total": 2, "rate": 0.5})
        self.assertIn("wcsc", report["by_site"])


if __name__ == "__main__":
    unittest.main()