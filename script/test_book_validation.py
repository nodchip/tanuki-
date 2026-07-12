from __future__ import annotations

import unittest

import cshogi

from script.book_validation import (
    BookValidationError,
    legal_distinct_entries,
    registered_legal_move_count,
    validate_book,
)
from script.extend_book_mcts import BookEntry, BookPosition, OpeningBook


class BookValidationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.sfen = cshogi.Board().sfen()

    def test_legal_distinct_entries_excludes_duplicate_illegal_move_and_illegal_response(self) -> None:
        entries = [
            BookEntry("7g7f", "3c3d", 10, 1, 0, 0),
            BookEntry("7g7f", "8c8d", 20, 2, 0, 1),
            BookEntry("7g7e", "none", 30, 3, 0, 2),
            BookEntry("2g2f", "7g7f", 40, 4, 0, 3),
            BookEntry("2g2f", "8c8d", 50, 5, 0, 4),
        ]

        valid = legal_distinct_entries(self.sfen, entries)

        self.assertEqual([entry.move for entry in valid], ["7g7f", "2g2f"])
        self.assertEqual(registered_legal_move_count(self.sfen, entries), 2)

    def test_validate_book_reports_all_invariant_violations_without_mutating_input(self) -> None:
        entries = [
            BookEntry("7g7f", "3c3d", 10, 1, 0, 0),
            BookEntry("7g7f", "8c8d", 20, 2, 0, 1),
            BookEntry("7g7e", "none", 30, 3, 0, 2),
            BookEntry("2g2f", "7g7f", 40, 4, 0, 3),
        ]
        book = OpeningBook()
        book.positions[self.sfen] = BookPosition(self.sfen, entries, 0)

        report = validate_book(book, mode="report")

        self.assertEqual(
            [issue.kind for issue in report.issues],
            ["duplicate-move", "illegal-move", "illegal-response"],
        )
        self.assertEqual(book.positions[self.sfen].entries, entries)

    def test_validate_book_strict_raises_with_report(self) -> None:
        book = OpeningBook()
        book.positions[self.sfen] = BookPosition(
            self.sfen,
            [BookEntry("7g7e", "none", 0, 0, 0, 0)],
            0,
        )

        with self.assertRaises(BookValidationError) as caught:
            validate_book(book, mode="strict")

        self.assertEqual(caught.exception.report.issues[0].kind, "illegal-move")

    def test_validate_book_reports_malformed_sfen(self) -> None:
        book = OpeningBook()
        book.positions["not-an-sfen"] = BookPosition(
            "not-an-sfen",
            [BookEntry("7g7f", "none", 0, 0, 0, 0)],
            0,
        )

        report = validate_book(book, mode="report")

        self.assertEqual([issue.kind for issue in report.issues], ["invalid-sfen"])


if __name__ == "__main__":
    unittest.main()