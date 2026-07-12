from __future__ import annotations

import pathlib
import tempfile
import unittest

from script.book_corpus import CorpusStore
from script.corpus_ingest import ingest_csa_text
from script.corpus_rankings import TournamentResult, import_tournament_ranking, recompute_candidate_priorities


CSA = """V2.2
N+{black}
N-Beta
PI
+
+{first}
-3334FU
%TORYO
"""


class CorpusRankingsTest(unittest.TestCase):
    def test_tournament_stage_and_rank_reorder_candidates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                ingest_csa_text(store, CSA.format(black="Weak", first="2726FU"),
                    site="wcsc", event="wcsc36", year=2026, relative_path="weak.csa", priority_key="0")
                ingest_csa_text(store, CSA.format(black="Strong", first="7776FU"),
                    site="wcsc", event="wcsc36", year=2026, relative_path="strong.csa", priority_key="0")
                import_tournament_ranking(
                    store, site="wcsc", event="wcsc36", source_url="https://example/rank",
                    source_sha256="abc", retrieved_at=1.0, provisional=False,
                    results=[
                        TournamentResult("Strong", "final", 3, 1, 8),
                        TournamentResult("Weak", "first", 1, 1, 30),
                        TournamentResult("Beta", "final", 3, 8, 8),
                    ],
                )
                recompute_candidate_priorities(store)
                moves = [row["move"] for row in store.connection.execute(
                    "SELECT move FROM candidate ORDER BY priority_key DESC"
                )]
        self.assertEqual(moves[0], "7g7f")


if __name__ == "__main__":
    unittest.main()