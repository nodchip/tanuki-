from __future__ import annotations

import io
import pathlib
import sys
import tempfile
import textwrap
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from extend_book_mcts import (
    BookEntry,
    BookPosition,
    LeafPath,
    OpeningBook,
    PathStep,
    ProgressReporter,
    RunStats,
    SearchResult,
    StopLimits,
    UsiEngine,
    calculate_ucb,
    merge_search_results,
    parse_info_line,
    propagate_minimax,
    score_to_winrate,
    select_leaf_path,
)


class ExtendBookMctsTest(unittest.TestCase):
    def test_load_book_initializes_omitted_depth_and_visits_to_zero(self) -> None:
        text = textwrap.dedent(
            """\
            #YANEURAOU-DB2016 1.00
            sfen root
            7g7f 3c3d 12
            2g2f none -5 4
            """
        )

        book = OpeningBook.from_text(text)

        entries = book.positions["root"].entries
        self.assertEqual(entries[0], BookEntry("7g7f", "3c3d", 12, 0, 0, 0))
        self.assertEqual(entries[1], BookEntry("2g2f", "none", -5, 4, 0, 1))

    def test_write_book_stably_sorts_entries_by_eval_descending(self) -> None:
        book = OpeningBook()
        book.positions["root"] = BookPosition(
            "root",
            [
                BookEntry("a", "none", 10, 1, 1, 0),
                BookEntry("b", "none", 30, 1, 1, 1),
                BookEntry("c", "none", 30, 1, 1, 2),
            ],
            0,
        )

        self.assertEqual(
            book.to_text(),
            textwrap.dedent(
                """\
                #YANEURAOU-DB2016 1.00
                sfen root
                b none 30 1 1
                c none 30 1 1
                a none 10 1 1
                """
            ),
        )

    def test_ucb_accepts_zero_initial_visits_without_special_priority(self) -> None:
        first = calculate_ucb(eval_cp=0, child_visits=0, parent_visits=0, c_puct=1.4, eval_scale=600.0)
        second = calculate_ucb(eval_cp=600, child_visits=0, parent_visits=0, c_puct=1.4, eval_scale=600.0)

        self.assertAlmostEqual(first, 0.5)
        self.assertGreater(second, first)
        self.assertAlmostEqual(score_to_winrate(0, 600.0), 0.5)

    def test_merge_search_results_preserves_existing_eval_and_depth_but_updates_response_and_visits(self) -> None:
        position = BookPosition(
            "root",
            [BookEntry("7g7f", "none", 12, 3, 5, 0)],
            0,
        )
        results = [
            SearchResult("7g7f", "3c3d", 99, 10),
            SearchResult("2g2f", "none", -1, 8),
        ]

        merge_search_results(position, results, selected_move="7g7f")

        self.assertEqual(position.entries[0], BookEntry("7g7f", "3c3d", 12, 3, 6, 0))
        self.assertEqual(position.entries[1], BookEntry("2g2f", "none", -1, 8, 0, 1))

    def test_merge_search_results_treats_uppercase_none_as_empty_response(self) -> None:
        position = BookPosition(
            "root",
            [BookEntry("7g7f", "None", 12, 3, 0, 0)],
            0,
        )

        merge_search_results(
            position,
            [SearchResult("7g7f", "3c3d", 99, 10)],
            selected_move=None,
        )

        self.assertEqual(position.entries[0].response, "3c3d")

    def test_select_leaf_path_uses_only_registered_book_moves(self) -> None:
        book = OpeningBook()
        book.positions["root"] = BookPosition(
            "root",
            [
                BookEntry("a", "none", 20, 1, 2, 0),
                BookEntry("b", "none", -10, 1, 2, 1),
            ],
            0,
        )
        navigator = {"root:a": "child", "root:b": "other"}

        path = select_leaf_path(
            book,
            "root",
            navigator=lambda sfen, move: navigator[f"{sfen}:{move}"],
            multipv=2,
            c_puct=1.4,
            eval_scale=600.0,
            inflight=set(),
        )

        self.assertEqual([(step.sfen, step.entry.move) for step in path.steps], [("root", "a")])
        self.assertEqual(path.leaf_sfen, "child")

    def test_select_leaf_path_uses_only_bestmove_on_book_side(self) -> None:
        book = OpeningBook()
        book.positions["root"] = BookPosition(
            "root",
            [
                BookEntry("bad-high-ucb", "none", 10, 1, 0, 0),
                BookEntry("best", "none", 100, 1, 100, 1),
            ],
            0,
        )
        navigator = {"root:bad-high-ucb": "bad", "root:best": "good"}

        path = select_leaf_path(
            book,
            "root",
            navigator=lambda sfen, move: navigator[f"{sfen}:{move}"],
            multipv=2,
            c_puct=1.4,
            eval_scale=600.0,
            inflight=set(),
            book_side="black",
            turn_provider=lambda sfen: "black",
            root_best_eval=100,
            eval_diff=50,
        )

        self.assertEqual(path.steps[0].entry.move, "best")
        self.assertEqual(path.leaf_sfen, "good")

    def test_select_leaf_path_keeps_non_book_side_moves_above_root_threshold(self) -> None:
        book = OpeningBook()
        book.positions["root"] = BookPosition(
            "root",
            [
                BookEntry("low", "none", 40, 1, 0, 0),
                BookEntry("kept", "none", 70, 1, 100, 1),
            ],
            0,
        )
        navigator = {"root:low": "low-child", "root:kept": "kept-child"}

        path = select_leaf_path(
            book,
            "root",
            navigator=lambda sfen, move: navigator[f"{sfen}:{move}"],
            multipv=2,
            c_puct=1.4,
            eval_scale=600.0,
            inflight=set(),
            book_side="black",
            turn_provider=lambda sfen: "white",
            root_best_eval=100,
            eval_diff=50,
        )

        self.assertEqual(path.steps[0].entry.move, "kept")
        self.assertEqual(path.leaf_sfen, "kept-child")

    def test_select_leaf_path_stops_at_max_ply(self) -> None:
        book = OpeningBook()
        book.positions["root"] = BookPosition(
            "root",
            [
                BookEntry("a", "none", 100, 1, 1, 0),
                BookEntry("b", "none", 90, 1, 1, 1),
            ],
            0,
        )
        navigator = {"root:a": "child", "root:b": "other"}

        path = select_leaf_path(
            book,
            "root",
            navigator=lambda sfen, move: navigator[f"{sfen}:{move}"],
            multipv=2,
            c_puct=1.4,
            eval_scale=600.0,
            inflight=set(),
            max_ply=0,
        )

        self.assertEqual(path.steps, [])
        self.assertEqual(path.leaf_sfen, "root")

    def test_propagate_minimax_updates_selected_path_entries(self) -> None:
        book = OpeningBook()
        book.positions["root"] = BookPosition(
            "root",
            [
                BookEntry("a", "none", 30, 1, 1, 0),
                BookEntry("d", "none", 0, 1, 1, 1),
            ],
            0,
        )
        book.positions["child"] = BookPosition(
            "child",
            [
                BookEntry("b", "none", 40, 1, 1, 0),
                BookEntry("c", "none", 20, 1, 1, 1),
            ],
            1,
        )
        path = LeafPath([PathStep("root", book.positions["root"].entries[0])], "child")

        propagate_minimax(book, path, leaf_sfen="child")

        self.assertEqual(book.positions["root"].entries[0].eval_cp, -40)

    def test_parse_info_line_extracts_multipv_result(self) -> None:
        parsed = parse_info_line("info depth 12 score cp -99 multipv 2 pv 2g2f 8c8d 2f2e")

        self.assertEqual(parsed, (2, SearchResult("2g2f", "8c8d", -99, 12)))

    def test_parse_info_line_ignores_bound_scores(self) -> None:
        self.assertIsNone(parse_info_line("info depth 9 score cp 30 lowerbound multipv 1 pv 7g7f"))

    def test_stop_limits_stop_on_added_positions_searches_nodes_and_runtime(self) -> None:
        limits = StopLimits(
            max_added_positions=2,
            max_searches=3,
            max_total_nodes=400,
            max_runtime_sec=10.0,
        )

        self.assertFalse(limits.should_stop(RunStats(start_time=100.0), now=101.0))
        self.assertTrue(limits.should_stop(RunStats(start_time=100.0, added_positions=2), now=101.0))
        self.assertTrue(limits.should_stop(RunStats(start_time=100.0, searches=3), now=101.0))
        self.assertTrue(limits.should_stop(RunStats(start_time=100.0, total_nodes=400), now=101.0))
        self.assertTrue(limits.should_stop(RunStats(start_time=100.0), now=111.0))

    def test_stop_limits_prevents_starting_search_past_search_or_node_limit(self) -> None:
        limits = StopLimits(max_searches=1, max_total_nodes=100)

        self.assertTrue(limits.can_start_search(RunStats(start_time=100.0), nodes=100))
        self.assertFalse(limits.can_start_search(RunStats(start_time=100.0, searches=1), nodes=1))
        self.assertFalse(limits.can_start_search(RunStats(start_time=100.0, total_nodes=100), nodes=1))
        self.assertFalse(limits.can_start_search(RunStats(start_time=100.0, total_nodes=50), nodes=51))

    def test_stop_limits_reports_stop_reason(self) -> None:
        limits = StopLimits(max_searches=2, max_total_nodes=100)

        self.assertEqual(limits.stop_reason(RunStats(start_time=100.0, searches=2), now=101.0), "max-searches")
        self.assertEqual(limits.stop_reason(RunStats(start_time=100.0, total_nodes=100), now=101.0), "max-total-nodes")
        self.assertIsNone(limits.stop_reason(RunStats(start_time=100.0), now=101.0))

    def test_progress_reporter_emits_progress_save_and_stop_lines(self) -> None:
        stream = io.StringIO()
        stats = RunStats(start_time=100.0, added_positions=3, searches=5, total_nodes=5000)
        reporter = ProgressReporter(stream, interval_sec=10.0)

        reporter.maybe_progress(stats, now=111.0)
        reporter.save(pathlib.Path("out.db"), stats, now=112.0)
        reporter.stop("max-searches", stats, now=113.0)

        text = stream.getvalue()
        self.assertIn("[progress] elapsed=00:00:11 searches=5 added_positions=3 total_nodes=5000 nps_est=454.5", text)
        self.assertIn("[save] elapsed=00:00:12 output=out.db searches=5 added_positions=3 total_nodes=5000", text)
        self.assertIn("[stop] elapsed=00:00:13 reason=max-searches searches=5 added_positions=3 total_nodes=5000", text)

    def test_usi_engine_initializes_hash_option_with_hash_when_usi_hash_is_unavailable(self) -> None:
        engine = UsiEngine(
            pathlib.Path("dummy.exe"),
            usi_hash=16,
            threads=1,
            multipv=2,
            extra_options=[],
        )
        commands = []
        engine.send = commands.append  # type: ignore[method-assign]
        engine.wait_for = lambda expected: None  # type: ignore[method-assign]

        engine.initialize_options()

        self.assertIn("setoption name Hash value 16", commands)
        self.assertNotIn("setoption name USI_Hash value 16", commands)

    def test_usi_engine_initializes_hash_option_with_usi_hash_when_available(self) -> None:
        engine = UsiEngine(
            pathlib.Path("dummy.exe"),
            usi_hash=16,
            threads=1,
            multipv=2,
            extra_options=[],
        )
        engine.option_names.add("USI_Hash")
        commands = []
        engine.send = commands.append  # type: ignore[method-assign]
        engine.wait_for = lambda expected: None  # type: ignore[method-assign]

        engine.initialize_options()

        self.assertIn("setoption name USI_Hash value 16", commands)
        self.assertNotIn("setoption name Hash value 16", commands)


if __name__ == "__main__":
    unittest.main()
