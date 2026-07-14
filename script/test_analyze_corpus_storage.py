from __future__ import annotations

import pathlib
import tempfile
import unittest

from script.analyze_corpus_storage import analyze_database
from script.book_corpus import CorpusStore


class AnalyzeCorpusStorageTest(unittest.TestCase):
    def test_reports_dbstat_and_indexed_position_query(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            path = pathlib.Path(temporary_directory) / "corpus.sqlite"
            with CorpusStore(path) as store:
                store.upsert_candidate("p", "7g7f", (), "wcsc:event", "9")
            report = analyze_database(path, sample_size=1, repetitions=2)
            self.assertEqual(report["schema_version"], 5)
            self.assertGreater(report["file_size"], 0)
            self.assertTrue(any(
                "candidate_position_priority_idx" in line
                for line in report["query_plan"]
            ))
            self.assertEqual(report["lookup"]["samples"], 2)


if __name__ == "__main__":
    unittest.main()
