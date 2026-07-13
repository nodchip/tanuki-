from __future__ import annotations

import argparse
import json
import pathlib
import random
import subprocess
import tempfile
from typing import Any

import cshogi

try:
    from script.book_corpus import CorpusStore
    from script.book_validation import registered_legal_move_count
    from script.extend_book_mcts import (
        BookEntry,
        LeafPath,
        OpeningBook,
        PathStep,
        calculate_ucb,
        propagate_minimax,
        select_leaf_path,
    )
except ImportError:
    from book_corpus import CorpusStore
    from book_validation import registered_legal_move_count
    from extend_book_mcts import (
        BookEntry,
        LeafPath,
        OpeningBook,
        PathStep,
        calculate_ucb,
        propagate_minimax,
        select_leaf_path,
    )

STARTPOS = "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1"
AFTER_7G7F = "lnsgkgsnl/1r5b1/ppppppppp/9/9/2P6/PP1PPPPPP/1B5R1/LNSGKGSNL w - 2"


def navigate(sfen: str, move: str) -> str:
    board = cshogi.Board(sfen)
    board.push_usi(move)
    return board.sfen()


def python_observation() -> dict[str, Any]:
    fixture = (
        f"#YANEURAOU-DB2016 1.00\nsfen {STARTPOS}\n"
        "7g7f 3c3d 100 1 1\n2g2f 8c8d 10 1 1\n"
    )
    book = OpeningBook.from_text(fixture)
    ignore = OpeningBook.from_text(fixture, ignore_ply=True)
    path = select_leaf_path(
        book,
        STARTPOS,
        navigator=navigate,
        multipv=2,
        c_puct=1.4,
        eval_scale=600.0,
        inflight=set(),
    )
    minimax = OpeningBook.from_text(fixture)
    minimax.ensure_position(AFTER_7G7F).entries = [
        BookEntry("3c3d", "none", 40, 1, 1, 2),
        BookEntry("8c8d", "none", 20, 1, 1, 3),
    ]
    root_entry = minimax.positions[STARTPOS].find_entry("7g7f")
    assert root_entry is not None
    propagate_minimax(
        minimax,
        LeafPath([PathStep(STARTPOS, root_entry)], AFTER_7G7F),
        leaf_sfen=AFTER_7G7F,
    )
    entries = [
        BookEntry("7g7f", "3c3d", 0, 0, 0, 0),
        BookEntry("7g7f", "8c8d", 0, 0, 0, 1),
        BookEntry("7g7e", "none", 0, 0, 0, 2),
        BookEntry("2g2f", "8c8d", 0, 0, 0, 3),
    ]
    with tempfile.TemporaryDirectory() as temporary_directory:
        corpus = CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite")
        corpus.upsert_candidate("p", "7g7f", ["7g7f"], "wcsc:event", "100")
        corpus.upsert_candidate("p", "2g2f", ["2g2f"], "denryu:event", "200")
        candidate = corpus.reserve_for_position(
            "snapshot", "p", excluded_moves=set(), width=1, lease_sec=60.0, now=1000.0
        )
        assert candidate is not None
        corpus_values = {
            "corpus_move": candidate.move,
            "corpus_status": candidate.status.value,
            "corpus_attempts": candidate.attempts,
            "corpus_width": corpus.progressive_width(),
        }
        corpus.close()
    chooser = random.Random(42)
    return {
        "roundtrip": book.to_text(),
        "ignore_key": ignore.position_key(STARTPOS),
        "distinct_legal": registered_legal_move_count(STARTPOS, entries),
        "ucb": f"{calculate_ucb(eval_cp=100, child_visits=1, parent_visits=2, c_puct=1.4, eval_scale=600.0):.12f}",
        "leaf_moves": [step.entry.move for step in path.steps],
        "leaf_sfen": path.leaf_sfen,
        "minimax_root_eval": root_entry.eval_cp,
        "position": "position startpos moves 7g7f 3c3d",
        "random_choices": [chooser.choice(list(range(3))) for _ in range(10)],
        **corpus_values,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rust-probe", type=pathlib.Path, required=True)
    args = parser.parse_args()
    rust = json.loads(subprocess.check_output([args.rust_probe], text=True, encoding="utf-8"))
    python = python_observation()
    if rust != python:
        print(json.dumps({"python": python, "rust": rust}, ensure_ascii=False, indent=2))
        return 1
    print(json.dumps({"status": "match", "fields": sorted(python)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
