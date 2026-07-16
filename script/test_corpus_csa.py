from __future__ import annotations

import pathlib
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from corpus_csa import CsaGameError, normalize_csa_game


VALID_CSA = """V2.2
N+black
N-white
PI
+
+7776FU
-3334FU
%TORYO
"""


class CorpusCsaTest(unittest.TestCase):
    def test_normalizes_each_position_that_has_a_recorded_next_move(self) -> None:
        game = normalize_csa_game(VALID_CSA, source_path="raw/game.csa")

        self.assertEqual(game.players, ("black", "white"))
        self.assertEqual(game.moves, ("7g7f", "3c3d"))
        self.assertEqual(len(game.positions), 2)
        self.assertEqual(game.positions[0].history, ())
        self.assertEqual(game.positions[0].move, "7g7f")
        self.assertEqual(game.positions[1].history, ("7g7f",))
        self.assertEqual(game.positions[1].move, "3c3d")

    def test_does_not_emit_terminal_post_move_position(self) -> None:
        game = normalize_csa_game(VALID_CSA, source_path="raw/game.csa")
        self.assertEqual(len(game.positions), len(game.moves))

    def test_accepts_inline_comment_after_first_move(self) -> None:
        with_inline_comment = VALID_CSA.replace("+7776FU", "+7776FU'comment")
        game = normalize_csa_game(
            with_inline_comment, source_path="raw/inline-comment.csa"
        )
        self.assertEqual(game.moves[0], "7g7f")

    def test_rejects_non_startpos_games(self) -> None:
        handicap = VALID_CSA.replace("PI\n", "PI82HI\n")
        with self.assertRaisesRegex(CsaGameError, "startpos"):
            normalize_csa_game(handicap, source_path="raw/handicap.csa")

    def test_reports_broken_game_with_source_path(self) -> None:
        broken = VALID_CSA.replace("-3334FU", "-9998FU")
        with self.assertRaises(CsaGameError) as caught:
            normalize_csa_game(broken, source_path="raw/broken.csa")
        self.assertIn("raw/broken.csa", str(caught.exception))

    def test_rejects_time_before_first_move_without_calling_native_parser(self) -> None:
        broken = """V2.2
PI
+
'+2726FU
T1
"""
        with mock.patch(
            "corpus_csa.CSA.Parser",
            side_effect=AssertionError("native parser must not be called"),
        ):
            with self.assertRaisesRegex(
                CsaGameError, "time line before first recorded move"
            ) as caught:
                normalize_csa_game(broken, source_path="raw/time-before-move.csa")
        self.assertEqual(caught.exception.line_number, 5)

    def test_rejects_turn_before_initial_position_without_calling_parser(self) -> None:
        broken = """'header
+
+7776FU,T0
"""
        with mock.patch(
            "corpus_csa.CSA.Parser",
            side_effect=AssertionError("native parser must not be called"),
        ):
            with self.assertRaisesRegex(
                CsaGameError, "turn line before initial position"
            ) as caught:
                normalize_csa_game(broken, source_path="raw/missing-position.csa")
        self.assertEqual(caught.exception.line_number, 2)


if __name__ == "__main__":
    unittest.main()