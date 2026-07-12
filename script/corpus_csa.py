from __future__ import annotations

import dataclasses
import hashlib
from typing import Tuple

import cshogi
from cshogi import CSA


class CsaGameError(ValueError):
    """A CSA record could not be accepted as one legal startpos game."""

    def __init__(
        self, source_path: str, reason: str, *, line_number: int | None = None,
        previous_sfen: str | None = None, move: str | None = None,
    ) -> None:
        self.source_path = source_path
        self.reason = reason
        self.line_number = line_number
        self.previous_sfen = previous_sfen
        self.move = move
        details = [source_path]
        if line_number is not None:
            details.append(f"line {line_number}")
        if previous_sfen is not None:
            details.append(f"previous_sfen={previous_sfen}")
        if move is not None:
            details.append(f"move={move}")
        super().__init__(": ".join(details + [reason]))


def _diagnose_move_lines(text: str, source_path: str) -> CsaGameError | None:
    board = cshogi.Board()
    for line_number, raw_line in enumerate(text.splitlines(), 1):
        line = raw_line.strip()
        if len(line) != 7 or line[0] not in "+-" or not line[1:5].isdigit():
            continue
        previous_sfen = board.sfen()
        expected = "+" if board.turn == cshogi.BLACK else "-"
        try:
            move = board.move_from_csa(line[1:])
        except Exception:
            move = 0
        if line[0] != expected or not move or not board.is_legal(move):
            return CsaGameError(
                source_path, "illegal or unreplayable CSA move",
                line_number=line_number, previous_sfen=previous_sfen, move=line,
            )
        board.push(move)
    return None


@dataclasses.dataclass(frozen=True)
class CorpusPosition:
    sfen: str
    history: Tuple[str, ...]
    move: str
    ply: int


@dataclasses.dataclass(frozen=True)
class NormalizedGame:
    source_path: str
    sha256: str
    players: Tuple[str, str]
    moves: Tuple[str, ...]
    positions: Tuple[CorpusPosition, ...]
    endgame: str


def _position_key(sfen: str) -> str:
    tokens = sfen.split()
    return " ".join(tokens[:3])


def normalize_csa_game(text: str, *, source_path: str) -> NormalizedGame:
    """Parse one CSA game, require startpos, and replay every recorded move."""
    parser = CSA.Parser()
    try:
        parser.parse_csa_str(text)
    except Exception as error:
        diagnostic = _diagnose_move_lines(text, source_path)
        if diagnostic is not None:
            raise diagnostic from error
        raise CsaGameError(source_path, f"CSA parse error: {error}") from error

    initial = cshogi.Board()
    if not parser.sfen or _position_key(parser.sfen) != _position_key(initial.sfen()):
        raise CsaGameError(source_path, "game does not start from startpos")
    if not parser.moves:
        raise CsaGameError(source_path, "game has no recorded moves")

    board = cshogi.Board()
    history: list[str] = []
    moves: list[str] = []
    positions: list[CorpusPosition] = []
    try:
        for ply, encoded_move in enumerate(parser.moves):
            move = cshogi.move_to_usi(encoded_move)
            if not board.is_legal(encoded_move):
                raise ValueError(f"illegal move at ply {ply + 1}: {move}")
            positions.append(CorpusPosition(board.sfen(), tuple(history), move, ply))
            board.push(encoded_move)
            history.append(move)
            moves.append(move)
    except Exception as error:
        move_lines = [(number, line.strip()) for number, line in enumerate(text.splitlines(), 1) if line.strip().startswith(("+", "-"))]
        line_number, raw_move = move_lines[len(history)] if len(history) < len(move_lines) else (None, None)
        raise CsaGameError(source_path, f"replay error: {error}", line_number=line_number, previous_sfen=board.sfen(), move=raw_move) from error

    names = tuple(parser.names[:2])
    if len(names) != 2:
        names = (names + ("", ""))[:2]
    return NormalizedGame(
        source_path=source_path,
        sha256=hashlib.sha256(text.encode("utf-8")).hexdigest(),
        players=(str(names[0]), str(names[1])),
        moves=tuple(moves),
        positions=tuple(positions),
        endgame=str(parser.endgame),
    )