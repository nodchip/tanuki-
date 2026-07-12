from __future__ import annotations

import dataclasses
from typing import Any, Iterable, Sequence

import cshogi


@dataclasses.dataclass(frozen=True)
class ValidationIssue:
    sfen: str
    move: str | None
    kind: str
    detail: str


@dataclasses.dataclass(frozen=True)
class ValidationReport:
    positions: int
    entries: int
    issues: tuple[ValidationIssue, ...]

    @property
    def valid(self) -> bool:
        return not self.issues


class BookValidationError(ValueError):
    def __init__(self, report: ValidationReport) -> None:
        self.report = report
        first = report.issues[0] if report.issues else None
        detail = "unknown invariant violation" if first is None else f"{first.kind}: {first.detail}"
        super().__init__(f"book validation failed: {detail}")


def _board_from_sfen(sfen: str) -> cshogi.Board:
    return cshogi.Board(sfen)


def _legal_move(board: cshogi.Board, move_usi: str) -> int | None:
    try:
        move = board.move_from_usi(move_usi)
    except (TypeError, ValueError, RuntimeError):
        return None
    if not move or not board.is_legal(move):
        return None
    return move


def _entry_issue(sfen: str, entry: Any) -> ValidationIssue | None:
    try:
        board = _board_from_sfen(sfen)
    except (TypeError, ValueError, RuntimeError):
        return ValidationIssue(sfen, None, "invalid-sfen", "SFEN cannot be parsed")

    move = _legal_move(board, entry.move)
    if move is None:
        return ValidationIssue(sfen, entry.move, "illegal-move", "move is not legal in position")
    if str(entry.response).lower() == "none":
        return None

    board.push(move)
    response = _legal_move(board, entry.response)
    if response is None:
        return ValidationIssue(
            sfen,
            entry.move,
            "illegal-response",
            f"response {entry.response} is not legal after move",
        )
    return None


def legal_distinct_entries(sfen: str, entries: Sequence[Any]) -> list[Any]:
    """Return the first valid entry for each distinct legal move."""
    try:
        _board_from_sfen(sfen)
    except (TypeError, ValueError, RuntimeError):
        return []

    result: list[Any] = []
    seen: set[str] = set()
    for entry in entries:
        if entry.move in seen:
            continue
        issue = _entry_issue(sfen, entry)
        if issue is not None:
            continue
        seen.add(entry.move)
        result.append(entry)
    return result


def registered_legal_move_count(sfen: str, entries: Sequence[Any]) -> int:
    return len(legal_distinct_entries(sfen, entries))


def validate_book(book: Any, *, mode: str = "strict") -> ValidationReport:
    if mode not in {"strict", "report"}:
        raise ValueError("mode must be strict or report")

    issues: list[ValidationIssue] = []
    entry_count = 0
    for position in book.positions.values():
        sfen = position.sfen
        entries = position.entries
        entry_count += len(entries)
        try:
            _board_from_sfen(sfen)
        except (TypeError, ValueError, RuntimeError):
            issues.append(ValidationIssue(sfen, None, "invalid-sfen", "SFEN cannot be parsed"))
            continue

        seen: set[str] = set()
        for entry in entries:
            if entry.move in seen:
                issues.append(
                    ValidationIssue(sfen, entry.move, "duplicate-move", "move occurs more than once")
                )
                continue
            issue = _entry_issue(sfen, entry)
            if issue is not None:
                issues.append(issue)
                continue
            seen.add(entry.move)

    report = ValidationReport(len(book.positions), entry_count, tuple(issues))
    if mode == "strict" and not report.valid:
        raise BookValidationError(report)
    return report