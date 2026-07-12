from __future__ import annotations

import pathlib
import tempfile
import unittest

from script.book_corpus import CorpusStore
from script.corpus_ingest import AliasResolver, ingest_csa_text


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

    def test_alias_resolution_is_exact_or_explicit_never_fuzzy(self) -> None:
        resolver = AliasResolver({"Alpha v2": "Alpha"})
        self.assertEqual(resolver.resolve(" Alpha ", {"Alpha"}), ("Alpha", "exact"))
        self.assertEqual(resolver.resolve("Alpha v2", {"Alpha"}), ("Alpha", "alias"))
        self.assertEqual(resolver.resolve("Alph", {"Alpha"}), (None, "unresolved"))


if __name__ == "__main__":
    unittest.main()