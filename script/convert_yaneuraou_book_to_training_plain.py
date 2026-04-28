from __future__ import annotations

import argparse
import dataclasses
import pathlib
import re
import struct
import sys
import time
from typing import BinaryIO, Iterable, Optional, TextIO


DEFAULT_BUFFER_SIZE = 8 * 1024 * 1024
DEFAULT_PROGRESS_INTERVAL_SEC = 5.0
SFEN_PREFIX = "sfen "
BLACK = 0
WHITE = 1
NO_PIECE = 0
KING = 8
PIECE_PROMOTE = 8
PIECE_WHITE = 16
MOVE_DROP = 1 << 14
MOVE_PROMOTE = 1 << 15
HUFFMAN_TABLE = {
    0: (0x00, 1),
    1: (0x01, 2),
    2: (0x03, 4),
    3: (0x0B, 4),
    4: (0x07, 4),
    5: (0x1F, 6),
    6: (0x3F, 6),
    7: (0x0F, 5),
}
PIECE_TYPES = {
    "P": 1,
    "L": 2,
    "N": 3,
    "S": 4,
    "B": 5,
    "R": 6,
    "G": 7,
    "K": 8,
}


@dataclasses.dataclass
class ConvertStats:
    positions: int = 0
    moves: int = 0
    records: int = 0
    skipped_positions: int = 0


@dataclasses.dataclass
class BookMove:
    move: str
    next_move: str
    value: int


@dataclasses.dataclass
class TrainingSample:
    sfen: str
    next_move: str
    value: int


def parse_ply(sfen: str) -> int:
    match = re.search(r"\s(\d+)\s*$", sfen)
    if match is None:
        return 1
    return int(match.group(1))


def parse_book_move(line: str) -> Optional[BookMove]:
    tokens = line.split()
    if len(tokens) < 3:
        return None

    move = tokens[0]
    if move in ("none", "resign"):
        return None

    try:
        value = int(tokens[2])
    except ValueError:
        return None

    next_move = tokens[1]
    return BookMove(move=move, next_move=next_move, value=value)


def piece_color(piece: int) -> int:
    return color_of(piece)


def piece_type_of(piece: int) -> int:
    return piece & 15


def unpromoted_piece_type(piece: int) -> int:
    return raw_type_of(piece)


def format_board(board: list[int]) -> str:
    piece_chars = {
        1: "P",
        2: "L",
        3: "N",
        4: "S",
        5: "B",
        6: "R",
        7: "G",
        8: "K",
        9: "+P",
        10: "+L",
        11: "+N",
        12: "+S",
        13: "+B",
        14: "+R",
    }
    ranks = []
    for rank_number in range(1, 10):
        rank_text = []
        empty = 0
        for file_number in range(9, 0, -1):
            piece = board[square(file_number, rank_number)]
            if piece == NO_PIECE:
                empty += 1
                continue
            if empty:
                rank_text.append(str(empty))
                empty = 0
            text = piece_chars[piece_type_of(piece)]
            if piece_color(piece) == WHITE:
                text = text.lower()
            rank_text.append(text)
        if empty:
            rank_text.append(str(empty))
        ranks.append("".join(rank_text))
    return "/".join(ranks)


def format_hands(hand_counts: dict[tuple[int, int], int]) -> str:
    parts = []
    for color, chars in ((BLACK, "RBGSLNP"), (WHITE, "rbgslnp")):
        for char in chars:
            piece_type = PIECE_TYPES[char.upper()]
            count = hand_counts.get((color, piece_type), 0)
            if count <= 0:
                continue
            if count > 1:
                parts.append(str(count))
            parts.append(char)
    return "".join(parts) if parts else "-"


def format_sfen(
    board: list[int],
    hand_counts: dict[tuple[int, int], int],
    side_to_move: int,
    ply: int,
) -> str:
    turn = "b" if side_to_move == BLACK else "w"
    return f"{format_board(board)} {turn} {format_hands(hand_counts)} {ply}"


def apply_usi_move_to_sfen(sfen: str, move: str) -> str:
    board, hand_counts, side_to_move = parse_sfen_position(sfen)
    ply = parse_ply(sfen)
    to_square = parse_square(move[2:4])

    if move[1] == "*":
        piece_type = PIECE_TYPES[move[0]]
        key = (side_to_move, piece_type)
        if hand_counts.get(key, 0) <= 0:
            raise ValueError(f"Missing hand piece for move {move}: {sfen}")
        hand_counts[key] -= 1
        if hand_counts[key] == 0:
            del hand_counts[key]
        board[to_square] = make_piece(side_to_move, piece_type)
    else:
        from_square = parse_square(move[0:2])
        piece = board[from_square]
        if piece == NO_PIECE:
            raise ValueError(f"Missing board piece for move {move}: {sfen}")
        captured = board[to_square]
        if captured != NO_PIECE:
            captured_type = unpromoted_piece_type(captured)
            key = (side_to_move, captured_type)
            hand_counts[key] = hand_counts.get(key, 0) + 1
        if len(move) == 5 and move[4] == "+":
            piece += PIECE_PROMOTE
        board[from_square] = NO_PIECE
        board[to_square] = piece

    return format_sfen(board, hand_counts, WHITE if side_to_move == BLACK else BLACK, ply + 1)


def make_after_move_sample(sfen: str, move: str, next_move: str, value: int) -> TrainingSample:
    return TrainingSample(
        sfen=apply_usi_move_to_sfen(sfen, move),
        next_move=next_move,
        value=-value,
    )


class BitWriter:
    def __init__(self) -> None:
        self.data = bytearray(32)
        self.cursor = 0

    def write_one_bit(self, value: int) -> None:
        if value:
            self.data[self.cursor // 8] |= 1 << (self.cursor & 7)
        self.cursor += 1

    def write_bits(self, value: int, bit_count: int) -> None:
        for i in range(bit_count):
            self.write_one_bit(value & (1 << i))


def square(file_number: int, rank_number: int) -> int:
    return (file_number - 1) * 9 + (rank_number - 1)


def parse_square(text: str) -> int:
    return square(int(text[0]), ord(text[1]) - ord("a") + 1)


def make_piece(color: int, piece_type: int, promoted: bool = False) -> int:
    return (color << 4) + piece_type + (PIECE_PROMOTE if promoted else 0)


def raw_type_of(piece: int) -> int:
    return piece & 7


def color_of(piece: int) -> int:
    return (piece & PIECE_WHITE) >> 4


def parse_sfen_position(sfen: str) -> tuple[list[int], dict[tuple[int, int], int], int]:
    tokens = sfen.split()
    if len(tokens) < 4:
        raise ValueError(f"Invalid SFEN: {sfen}")

    board = [NO_PIECE] * 81
    ranks = tokens[0].split("/")
    if len(ranks) != 9:
        raise ValueError(f"Invalid SFEN board: {sfen}")

    for rank_index, rank_text in enumerate(ranks, start=1):
        file_number = 9
        promoted = False
        for char in rank_text:
            if char == "+":
                promoted = True
                continue
            if char.isdigit():
                file_number -= int(char)
                continue
            piece_type = PIECE_TYPES[char.upper()]
            color = BLACK if char.isupper() else WHITE
            board[square(file_number, rank_index)] = make_piece(color, piece_type, promoted)
            promoted = False
            file_number -= 1
        if file_number != 0:
            raise ValueError(f"Invalid SFEN rank: {rank_text}")

    hand_counts: dict[tuple[int, int], int] = {}
    hand_text = tokens[2]
    if hand_text != "-":
        count_text = ""
        for char in hand_text:
            if char.isdigit():
                count_text += char
                continue
            count = int(count_text) if count_text else 1
            piece_type = PIECE_TYPES[char.upper()]
            color = BLACK if char.isupper() else WHITE
            hand_counts[(color, piece_type)] = hand_counts.get((color, piece_type), 0) + count
            count_text = ""

    side_to_move = BLACK if tokens[1] == "b" else WHITE
    return board, hand_counts, side_to_move


def write_board_piece(writer: BitWriter, piece: int) -> None:
    piece_type = raw_type_of(piece)
    code, bits = HUFFMAN_TABLE[piece_type]
    writer.write_bits(code, bits)
    if piece == NO_PIECE:
        return
    if piece_type != 7:
        writer.write_one_bit(piece & PIECE_PROMOTE)
    writer.write_one_bit(color_of(piece))


def write_hand_piece(writer: BitWriter, color: int, piece_type: int) -> None:
    code, bits = HUFFMAN_TABLE[piece_type]
    writer.write_bits(code >> 1, bits - 1)
    if piece_type != 7:
        writer.write_one_bit(0)
    writer.write_one_bit(color)


def pack_sfen(sfen: str) -> bytes:
    board, hand_counts, side_to_move = parse_sfen_position(sfen)
    writer = BitWriter()
    writer.write_one_bit(side_to_move)

    for color in (BLACK, WHITE):
        king_piece = make_piece(color, KING)
        try:
            king_square = board.index(king_piece)
        except ValueError as exc:
            raise ValueError(f"King not found in SFEN: {sfen}") from exc
        writer.write_bits(king_square, 7)

    for piece in board:
        if (piece & 15) == KING:
            continue
        write_board_piece(writer, piece)

    for color in (BLACK, WHITE):
        for piece_type in range(1, KING):
            for _ in range(hand_counts.get((color, piece_type), 0)):
                write_hand_piece(writer, color, piece_type)

    if writer.cursor != 256:
        raise ValueError(f"Packed SFEN is {writer.cursor} bits, not 256 bits: {sfen}")
    return bytes(writer.data)


def move16_from_usi(move: str) -> int:
    to_square = parse_square(move[2:4])
    if move[1] == "*":
        return to_square + (PIECE_TYPES[move[0]] << 7) + MOVE_DROP
    from_square = parse_square(move[0:2])
    value = to_square + (from_square << 7)
    if len(move) == 5 and move[4] == "+":
        value += MOVE_PROMOTE
    return value


def packed_sfen_value_bytes(sfen: str, move: BookMove) -> bytes:
    next_move = getattr(move, "next_move", getattr(move, "move", "none"))
    score = max(-32768, min(32767, move.value))
    return struct.pack(
        "<32shHHbB",
        pack_sfen(sfen),
        score,
        0 if next_move in ("none", "resign") else move16_from_usi(next_move),
        parse_ply(sfen),
        0,
        0,
    )


def write_record(output: TextIO, sfen: str, move: BookMove) -> None:
    sample = make_after_move_sample(sfen, move.move, move.next_move, move.value)
    output.write(f"sfen {sample.sfen}\n")
    output.write(f"move {sample.next_move}\n")
    output.write(f"score {sample.value}\n")
    output.write(f"ply {parse_ply(sample.sfen)}\n")
    output.write("result 0\n")
    output.write("e\n")


def write_packed_record(output: BinaryIO, sfen: str, move: BookMove) -> None:
    sample = make_after_move_sample(sfen, move.move, move.next_move, move.value)
    output.write(packed_sfen_value_bytes(sample.sfen, sample))


def iter_meaningful_lines(input_file: TextIO) -> Iterable[str]:
    for raw_line in input_file:
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith("#") or line.startswith("//"):
            continue
        yield line


def convert_book_to_training_plain(input_file: TextIO, output_file: TextIO) -> ConvertStats:
    stats = ConvertStats()
    current_sfen: Optional[str] = None
    moves: list[BookMove] = []

    def flush_position() -> None:
        nonlocal moves, current_sfen
        if current_sfen is None:
            return
        stats.positions += 1
        if not moves:
            stats.skipped_positions += 1
        else:
            for move in moves:
                write_record(output_file, current_sfen, move)
                stats.records += 1

    for line in iter_meaningful_lines(input_file):
        if line.startswith(SFEN_PREFIX):
            flush_position()
            current_sfen = line[len(SFEN_PREFIX) :]
            moves = []
            continue

        if current_sfen is None:
            continue

        move = parse_book_move(line)
        if move is None:
            continue

        stats.moves += 1
        moves.append(move)

    flush_position()
    return stats


class ProgressWriter:
    def __init__(self, stream: TextIO, interval_sec: float) -> None:
        self.stream = stream
        self.interval_sec = interval_sec
        self.start_time = time.monotonic()
        self.last_time = self.start_time

    def maybe_report(self, input_path: pathlib.Path, input_file: TextIO, stats: ConvertStats) -> None:
        now = time.monotonic()
        if now - self.last_time < self.interval_sec:
            return
        total_bytes = input_path.stat().st_size
        try:
            processed_bytes = input_file.tell()
        except OSError:
            processed_bytes = input_file.buffer.tell()
        percent = 100.0 * processed_bytes / total_bytes if total_bytes else 100.0
        elapsed = now - self.start_time
        self.stream.write(
            f"progress positions={stats.positions} records={stats.records} "
            f"bytes={processed_bytes}/{total_bytes} percent={percent:.2f} elapsed_sec={elapsed:.1f}\n"
        )
        self.stream.flush()
        self.last_time = now


def convert_book_file_to_training_plain(
    input_path: pathlib.Path,
    output_path: pathlib.Path,
    *,
    progress_stream: Optional[TextIO] = None,
) -> ConvertStats:
    input_path = pathlib.Path(input_path)
    output_path = pathlib.Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    stats = ConvertStats()
    progress = ProgressWriter(progress_stream, DEFAULT_PROGRESS_INTERVAL_SEC) if progress_stream else None

    with input_path.open("r", encoding="utf-8", errors="replace", buffering=DEFAULT_BUFFER_SIZE, newline="") as input_file:
        with output_path.open("w", encoding="utf-8", buffering=DEFAULT_BUFFER_SIZE, newline="\n") as output_file:
            current_sfen: Optional[str] = None
            moves: list[BookMove] = []

            def flush_position() -> None:
                nonlocal moves, current_sfen
                if current_sfen is None:
                    return
                stats.positions += 1
                if not moves:
                    stats.skipped_positions += 1
                else:
                    for move in moves:
                        write_record(output_file, current_sfen, move)
                        stats.records += 1
                if progress is not None:
                    progress.maybe_report(input_path, input_file, stats)

            for line in iter_meaningful_lines(input_file):
                if line.startswith(SFEN_PREFIX):
                    flush_position()
                    current_sfen = line[len(SFEN_PREFIX) :]
                    moves = []
                    continue

                if current_sfen is None:
                    continue

                move = parse_book_move(line)
                if move is None:
                    continue

                stats.moves += 1
                moves.append(move)

            flush_position()

    return stats


def convert_book_file_to_packed_sfen(
    input_path: pathlib.Path,
    output_path: pathlib.Path,
    *,
    progress_stream: Optional[TextIO] = None,
) -> ConvertStats:
    input_path = pathlib.Path(input_path)
    output_path = pathlib.Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    stats = ConvertStats()
    progress = ProgressWriter(progress_stream, DEFAULT_PROGRESS_INTERVAL_SEC) if progress_stream else None

    with input_path.open("r", encoding="utf-8", errors="replace", buffering=DEFAULT_BUFFER_SIZE, newline="") as input_file:
        with output_path.open("wb", buffering=DEFAULT_BUFFER_SIZE) as output_file:
            current_sfen: Optional[str] = None
            moves: list[BookMove] = []

            def flush_position() -> None:
                nonlocal moves, current_sfen
                if current_sfen is None:
                    return
                stats.positions += 1
                if not moves:
                    stats.skipped_positions += 1
                else:
                    for move in moves:
                        write_packed_record(output_file, current_sfen, move)
                        stats.records += 1
                if progress is not None:
                    progress.maybe_report(input_path, input_file, stats)

            for line in iter_meaningful_lines(input_file):
                if line.startswith(SFEN_PREFIX):
                    flush_position()
                    current_sfen = line[len(SFEN_PREFIX) :]
                    moves = []
                    continue

                if current_sfen is None:
                    continue

                move = parse_book_move(line)
                if move is None:
                    continue

                stats.moves += 1
                moves.append(move)

            flush_position()

    return stats


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Convert a YaneuraOu opening-book database to plain PackedSfenValue training records."
    )
    parser.add_argument("--input", required=True, type=pathlib.Path, help="Input YaneuraOu book DB path")
    parser.add_argument("--output", required=True, type=pathlib.Path, help="Output training data path")
    parser.add_argument(
        "--format",
        choices=("packed", "plain"),
        default="packed",
        help="Output format. packed writes binary PackedSfenValue records.",
    )
    parser.add_argument("--progress", action="store_true", help="Write progress to stderr")
    return parser


def main(argv: Optional[list[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.format == "plain":
        stats = convert_book_file_to_training_plain(
            args.input,
            args.output,
            progress_stream=sys.stderr if args.progress else None,
        )
    else:
        stats = convert_book_file_to_packed_sfen(
            args.input,
            args.output,
            progress_stream=sys.stderr if args.progress else None,
        )
    print(
        f"positions={stats.positions} moves={stats.moves} records={stats.records} "
        f"skipped_positions={stats.skipped_positions}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
