import io
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from convert_yaneuraou_book_to_training_plain import (
    convert_book_to_training_plain,
    make_after_move_sample,
    move16_from_usi,
    packed_sfen_value_bytes,
)


class ConvertYaneuraouBookToTrainingPlainTest(unittest.TestCase):
    def test_outputs_after_position_for_each_book_move_with_inverted_score(self) -> None:
        source = io.StringIO(
            "#YANEURAOU-DB2016 1.00\n"
            "sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1\n"
            "7g7f 3c3d 12 1 1\n"
            "2g2f none 91 1 1\n"
        )
        output = io.StringIO()

        stats = convert_book_to_training_plain(source, output)

        self.assertEqual(stats.positions, 1)
        self.assertEqual(stats.moves, 2)
        self.assertEqual(stats.records, 2)
        self.assertEqual(
            output.getvalue(),
            "sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2\n"
            "move 3c3d\n"
            "score -12\n"
            "ply 2\n"
            "result 0\n"
            "e\n"
            "sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/7P1/PPPPPPP1P/1B5R1/LNSGKGSNL w - 2\n"
            "move none\n"
            "score -91\n"
            "ply 2\n"
            "result 0\n"
            "e\n",
        )

    def test_capture_adds_unpromoted_piece_to_movers_hand(self) -> None:
        sample = make_after_move_sample(
            "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1",
            move="8h2b+",
            next_move="none",
            value=40,
        )

        self.assertEqual(
            sample.sfen,
            "lnsgkgsnl/1r5+B1/ppppppppp/9/9/9/PPPPPPPPP/7R1/LNSGKGSNL w B 2",
        )
        self.assertEqual(sample.value, -40)
        self.assertEqual(sample.next_move, "none")

    def test_drop_consumes_hand_piece(self) -> None:
        source = io.StringIO(
            "#YANEURAOU-DB2016 1.00\n"
            "sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPP1PPPP/1B5R1/LNSGKGSNL b P 1\n"
            "P*5e none 7 1 1\n"
        )
        output = io.StringIO()

        stats = convert_book_to_training_plain(source, output)

        self.assertEqual(stats.records, 1)
        self.assertEqual(
            output.getvalue(),
            "sfen lnsgkgsnl/1r5b1/ppppppppp/9/4P4/9/PPPP1PPPP/1B5R1/LNSGKGSNL w - 2\n"
            "move none\n"
            "score -7\n"
            "ply 2\n"
            "result 0\n"
            "e\n",
        )

    def test_packed_record_uses_after_position_score_and_next_move(self) -> None:
        sample = make_after_move_sample(
            "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1",
            move="7g7f",
            next_move="3c3d",
            value=12,
        )

        record = packed_sfen_value_bytes(sample.sfen, sample)

        self.assertEqual(len(record), 40)
        self.assertEqual(int.from_bytes(record[32:34], "little", signed=True), -12)
        self.assertEqual(int.from_bytes(record[34:36], "little"), move16_from_usi("3c3d"))
        self.assertEqual(int.from_bytes(record[36:38], "little"), 2)
        self.assertEqual(record[38], 0)
        self.assertEqual(record[39], 0)

    def test_packed_record_uses_zero_move_when_next_move_is_none(self) -> None:
        sample = make_after_move_sample(
            "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1",
            move="2g2f",
            next_move="none",
            value=91,
        )

        record = packed_sfen_value_bytes(sample.sfen, sample)

        self.assertEqual(len(record), 40)
        self.assertEqual(int.from_bytes(record[32:34], "little", signed=True), -91)
        self.assertEqual(int.from_bytes(record[34:36], "little"), 0)
        self.assertEqual(int.from_bytes(record[36:38], "little"), 2)

    def test_skips_positions_without_usable_moves(self) -> None:
        source = io.StringIO(
            "#YANEURAOU-DB2016 1.00\n"
            "sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1\n"
            "none none -10 0 1\n"
        )
        output = io.StringIO()

        stats = convert_book_to_training_plain(source, output)

        self.assertEqual(stats.positions, 1)
        self.assertEqual(stats.moves, 0)
        self.assertEqual(stats.records, 0)
        self.assertEqual(output.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
