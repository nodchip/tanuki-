from __future__ import annotations

import argparse
import dataclasses
import math
import os
import pathlib
import shutil
import subprocess
import sys
import threading
import time
from typing import Callable, Dict, Iterable, List, Optional, Sequence, Set, TextIO, Tuple

try:
    import cshogi
except ImportError:  # pragma: no cover - 実行環境に cshogi がない場合の CLI 用エラーに委ねる。
    cshogi = None


DEFAULT_HEADER = "#YANEURAOU-DB2016 1.00"
MATE_SCORE = 100000


@dataclasses.dataclass
class BookEntry:
    """定跡 DB の 1 指し手エントリを表す。"""

    move: str
    response: str
    eval_cp: int
    depth: int
    visits: int
    order_index: int


@dataclasses.dataclass
class BookPosition:
    """1 つの SFEN 局面と、その局面に登録された指し手一覧を表す。"""

    sfen: str
    entries: List[BookEntry]
    order_index: int

    def find_entry(self, move: str) -> Optional[BookEntry]:
        """USI 指し手に対応する登録済みエントリを返す。"""
        for entry in self.entries:
            if entry.move == move:
                return entry
        return None


@dataclasses.dataclass(frozen=True)
class SearchResult:
    """USI MultiPV から抽出した 1 候補の探索結果を表す。"""

    move: str
    response: str
    eval_cp: int
    depth: int


@dataclasses.dataclass(frozen=True)
class PathStep:
    """MCTS traversal で選択した 1 辺を表す。"""

    sfen: str
    entry: BookEntry


@dataclasses.dataclass(frozen=True)
class LeafPath:
    """root から leaf までの選択経路を表す。"""

    steps: List[PathStep]
    leaf_sfen: str


@dataclasses.dataclass
class RunStats:
    """実行中の停止条件判定に使う統計値を保持する。"""

    start_time: float
    added_positions: int = 0
    searches: int = 0
    total_nodes: int = 0


@dataclasses.dataclass(frozen=True)
class StopLimits:
    """Ctrl+C 以外の自動停止条件を保持する。"""

    max_added_positions: Optional[int] = None
    max_searches: Optional[int] = None
    max_total_nodes: Optional[int] = None
    max_runtime_sec: Optional[float] = None

    def should_stop(self, stats: RunStats, *, now: Optional[float] = None) -> bool:
        """いずれかの停止条件に達していれば True を返す。"""
        return self.stop_reason(stats, now=now) is not None

    def stop_reason(self, stats: RunStats, *, now: Optional[float] = None) -> Optional[str]:
        """到達済みの停止理由を返す。未到達なら None を返す。"""
        current_time = time.monotonic() if now is None else now
        if self.max_added_positions is not None and stats.added_positions >= self.max_added_positions:
            return "max-added-positions"
        if self.max_searches is not None and stats.searches >= self.max_searches:
            return "max-searches"
        if self.max_total_nodes is not None and stats.total_nodes >= self.max_total_nodes:
            return "max-total-nodes"
        if self.max_runtime_sec is not None and current_time - stats.start_time >= self.max_runtime_sec:
            return "max-runtime-sec"
        return None

    def can_start_search(self, stats: RunStats, *, nodes: int) -> bool:
        """次の探索を開始しても思考回数・総ノード数の上限を超えないかを返す。"""
        if self.max_searches is not None and stats.searches >= self.max_searches:
            return False
        if self.max_total_nodes is not None and stats.total_nodes + nodes > self.max_total_nodes:
            return False
        return True


class ProgressReporter:
    """進捗、保存、停止理由を stderr などへ一定間隔で出力する。"""

    def __init__(self, stream: TextIO, *, interval_sec: float) -> None:
        self.stream = stream
        self.interval_sec = max(0.0, interval_sec)
        self._last_progress_time: Optional[float] = None

    def maybe_progress(self, stats: RunStats, *, now: Optional[float] = None) -> None:
        """interval_sec 経過時に集計進捗を出力する。"""
        current_time = time.monotonic() if now is None else now
        if self._last_progress_time is not None:
            if self.interval_sec <= 0.0 or current_time - self._last_progress_time < self.interval_sec:
                return
        self._last_progress_time = current_time
        self._emit("[progress]", stats, now=current_time)

    def search_start(
        self,
        *,
        worker_id: int,
        leaf_sfen: str,
        depth: int,
        nodes: int,
        stats: RunStats,
    ) -> None:
        """1 回の leaf 探索開始を出力する。"""
        self.stream.write(
            f"[search-start] worker={worker_id} depth={depth} searches={stats.searches} "
            f"total_nodes={stats.total_nodes} nodes={nodes} "
            f"sfen={leaf_sfen}\n"
        )
        self.stream.flush()

    def search_finish(
        self,
        *,
        worker_id: int,
        entries: int,
        leaf_sfen: str,
        depth: int,
        stats: RunStats,
    ) -> None:
        """1 回の leaf 探索完了を出力する。"""
        self.stream.write(
            f"[search-finish] worker={worker_id} depth={depth} entries={entries} searches={stats.searches} "
            f"added_positions={stats.added_positions} total_nodes={stats.total_nodes} "
            f"sfen={leaf_sfen}\n"
        )
        self.stream.flush()

    def save(self, path: pathlib.Path, stats: RunStats, *, now: Optional[float] = None) -> None:
        """保存完了を出力する。"""
        current_time = time.monotonic() if now is None else now
        self._emit("[save]", stats, now=current_time, extra=f"output={path}")

    def stop(self, reason: str, stats: RunStats, *, now: Optional[float] = None) -> None:
        """停止理由を出力する。"""
        current_time = time.monotonic() if now is None else now
        self._emit("[stop]", stats, now=current_time, extra=f"reason={reason}")

    def _emit(
        self,
        prefix: str,
        stats: RunStats,
        *,
        now: float,
        extra: str = "",
    ) -> None:
        elapsed_sec = max(0.0, now - stats.start_time)
        nps = stats.total_nodes / elapsed_sec if elapsed_sec > 0 else 0.0
        parts = [
            prefix,
            f"elapsed={format_elapsed(elapsed_sec)}",
        ]
        if extra:
            parts.append(extra)
        parts.extend(
            [
                f"searches={stats.searches}",
                f"added_positions={stats.added_positions}",
                f"total_nodes={stats.total_nodes}",
            ]
        )
        if prefix == "[progress]":
            parts.append(f"nps_est={nps:.1f}")
        self.stream.write(" ".join(parts) + "\n")
        self.stream.flush()


def format_elapsed(elapsed_sec: float) -> str:
    """秒数を HH:MM:SS 形式に整形する。"""
    total = int(elapsed_sec)
    hours = total // 3600
    minutes = (total % 3600) // 60
    seconds = total % 60
    return f"{hours:02d}:{minutes:02d}:{seconds:02d}"


class OpeningBook:
    """やねうら王形式の定跡 DB をメモリ上で保持する。"""

    def __init__(self, *, ignore_ply: bool = False) -> None:
        self.header = DEFAULT_HEADER
        self.ignore_ply = ignore_ply
        self.positions: Dict[str, BookPosition] = {}
        self._next_position_order = 0
        self._next_entry_order = 0

    @classmethod
    def from_text(cls, text: str, *, ignore_ply: bool = False) -> "OpeningBook":
        """文字列から定跡 DB を読み込む。"""
        book = cls(ignore_ply=ignore_ply)
        current: Optional[BookPosition] = None
        for raw_line in text.splitlines():
            line = raw_line.strip()
            if not line:
                continue
            if line.startswith("#"):
                if line.startswith("#YANEURAOU-DB"):
                    book.header = line
                continue
            if line.startswith("//"):
                continue
            if line.startswith("sfen "):
                sfen = line[5:].strip()
                current = book.ensure_position(sfen)
                continue
            if current is None:
                raise ValueError(f"sfen 行より前に指し手行があります: {line}")
            current.entries.append(book._parse_entry_line(line))
        return book

    @classmethod
    def load(cls, path: pathlib.Path, *, ignore_ply: bool = False) -> "OpeningBook":
        """ファイルから定跡 DB を読み込む。"""
        data = pathlib.Path(path).read_text(encoding="utf-8-sig")
        return cls.from_text(data, ignore_ply=ignore_ply)

    def ensure_position(self, sfen: str) -> BookPosition:
        """局面がなければ作成し、既存または新規の局面を返す。"""
        key = self.position_key(sfen)
        position = self.positions.get(key)
        if position is not None:
            return position
        position = BookPosition(
            sfen=self.output_sfen(sfen),
            entries=[],
            order_index=self._next_position_order,
        )
        self._next_position_order += 1
        self.positions[key] = position
        return position

    def position_key(self, sfen: str) -> str:
        """局面検索に使うキーを返す。"""
        if not self.ignore_ply:
            return sfen
        return strip_sfen_ply(sfen)

    def output_sfen(self, sfen: str) -> str:
        """出力 DB に書く SFEN を返す。"""
        if not self.ignore_ply:
            return sfen
        return replace_sfen_ply(sfen, "0")

    def to_text(self) -> str:
        """定跡 DB を評価値降順の安定ソートで文字列化する。"""
        lines = [self.header]
        positions = sorted(self.positions.values(), key=lambda position: position.order_index)
        for position in positions:
            lines.append(f"sfen {position.sfen}")
            entries = sorted(position.entries, key=lambda entry: (-entry.eval_cp, entry.order_index))
            for entry in entries:
                lines.append(
                    f"{entry.move} {entry.response} {entry.eval_cp} {entry.depth} {entry.visits}"
                )
        return "\n".join(lines) + "\n"

    def write_atomic(self, path: pathlib.Path, backup_count: int) -> None:
        """一時ファイル経由で DB を保存し、世代バックアップを保持する。"""
        path = pathlib.Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp_path = path.with_name(path.name + ".tmp")
        tmp_path.write_text(self.to_text(), encoding="utf-8", newline="\n")
        rotate_backups(path, backup_count)
        os.replace(tmp_path, path)

    def _parse_entry_line(self, line: str) -> BookEntry:
        """指し手行を解析し、省略された depth / visits を 0 で補う。"""
        tokens = line.split()
        if len(tokens) < 3:
            raise ValueError(f"指し手行の列数が足りません: {line}")
        move = tokens[0]
        response = tokens[1]
        eval_cp = int(tokens[2])
        depth = int(tokens[3]) if len(tokens) >= 4 else 0
        visits = int(tokens[4]) if len(tokens) >= 5 else 0
        entry = BookEntry(move, response, eval_cp, depth, visits, self._next_entry_order)
        self._next_entry_order += 1
        return entry

    def append_entry(self, position: BookPosition, result: SearchResult) -> BookEntry:
        """探索結果から新規エントリを追加する。"""
        entry = BookEntry(
            result.move,
            result.response,
            result.eval_cp,
            result.depth,
            0,
            self._next_entry_order,
        )
        self._next_entry_order += 1
        position.entries.append(entry)
        return entry


def rotate_backups(path: pathlib.Path, backup_count: int) -> None:
    """保存前に既存出力ファイルのバックアップを最大数だけローテートする。"""
    if backup_count <= 0 or not path.exists():
        return
    for index in range(backup_count - 1, 0, -1):
        src = backup_path(path, index)
        dst = backup_path(path, index + 1)
        if src.exists():
            if dst.exists():
                dst.unlink()
            src.replace(dst)
    first = backup_path(path, 1)
    if first.exists():
        first.unlink()
    shutil.copy2(path, first)


def backup_path(path: pathlib.Path, index: int) -> pathlib.Path:
    """世代バックアップのファイル名を返す。"""
    return path.with_name(f"{path.name}.{index:03d}.bak")


def strip_sfen_ply(sfen: str) -> str:
    """SFEN 文字列から手数だけを取り除く。"""
    tokens = sfen.split()
    if len(tokens) >= 4 and tokens[-1].lstrip("-").isdigit():
        return " ".join(tokens[:-1])
    return sfen


def replace_sfen_ply(sfen: str, ply: str) -> str:
    """SFEN 文字列の手数を指定値に置き換える。"""
    tokens = sfen.split()
    if len(tokens) >= 4 and tokens[-1].lstrip("-").isdigit():
        tokens[-1] = ply
        return " ".join(tokens)
    return f"{sfen} {ply}"


def score_to_winrate(eval_cp: int, eval_scale: float) -> float:
    """評価値をシグモイド関数で勝率へ変換する。"""
    if eval_scale <= 0:
        raise ValueError("eval_scale は正の値である必要があります")
    x = max(-60.0, min(60.0, eval_cp / eval_scale))
    return 1.0 / (1.0 + math.exp(-x))


def calculate_ucb(
    *,
    eval_cp: int,
    child_visits: int,
    parent_visits: int,
    c_puct: float,
    eval_scale: float,
) -> float:
    """評価値由来の勝率と探索項から UCB 値を計算する。"""
    winrate = score_to_winrate(eval_cp, eval_scale)
    exploration = c_puct * math.sqrt(math.log(parent_visits + 1) / (child_visits + 1))
    return winrate + exploration


def merge_search_results(
    position: BookPosition,
    results: Sequence[SearchResult],
    *,
    selected_move: Optional[str] = None,
) -> None:
    """leaf 探索結果を既存上書き規則に従って局面へ反映する。"""
    max_order = max((entry.order_index for entry in position.entries), default=-1)
    next_order = max_order + 1
    for result in results:
        entry = position.find_entry(result.move)
        if entry is None:
            position.entries.append(
                BookEntry(
                    result.move,
                    result.response,
                    result.eval_cp,
                    result.depth,
                    0,
                    next_order,
                )
            )
            next_order += 1
            continue
        if is_no_response(entry.response) and not is_no_response(result.response):
            entry.response = result.response
    if selected_move is not None:
        selected = position.find_entry(selected_move)
        if selected is not None:
            selected.visits += 1


def select_leaf_path(
    book: OpeningBook,
    root_sfen: str,
    *,
    navigator: Callable[[str, str], str],
    multipv: int,
    c_puct: float,
    eval_scale: float,
    inflight: Set[str],
    max_ply: Optional[int] = None,
    book_side: Optional[str] = None,
    turn_provider: Optional[Callable[[str], str]] = None,
    root_best_eval: Optional[int] = None,
    eval_diff: Optional[int] = None,
) -> LeafPath:
    """登録済み定跡手だけを UCB でたどり、leaf 局面を返す。"""
    steps: List[PathStep] = []
    sfen = book.position_key(root_sfen)
    visited: Set[str] = set()
    while True:
        if max_ply is not None and len(steps) >= max_ply:
            return LeafPath(steps=steps, leaf_sfen=sfen)
        position = book.positions.get(sfen)
        if position is None or len(position.entries) < multipv or not position.entries:
            return LeafPath(steps=steps, leaf_sfen=sfen)
        if sfen in visited:
            return LeafPath(steps=steps, leaf_sfen=sfen)
        visited.add(sfen)

        parent_visits = sum(entry.visits for entry in position.entries)
        candidates: List[Tuple[float, BookEntry, str]] = []
        filtered_entries = filter_entries_for_peta_rule(
            position.entries,
            sfen=sfen,
            book_side=book_side,
            turn_provider=turn_provider,
            root_best_eval=root_best_eval,
            eval_diff=eval_diff,
        )
        for entry in filtered_entries:
            child_sfen = navigator(sfen, entry.move)
            if child_sfen in inflight:
                continue
            ucb = calculate_ucb(
                eval_cp=entry.eval_cp,
                child_visits=entry.visits,
                parent_visits=parent_visits,
                c_puct=c_puct,
                eval_scale=eval_scale,
            )
            candidates.append((ucb, entry, book.position_key(child_sfen)))
        if not candidates:
            return LeafPath(steps=steps, leaf_sfen=sfen)

        _, selected, next_sfen = max(candidates, key=lambda item: (item[0], -item[1].order_index))
        steps.append(PathStep(sfen=sfen, entry=selected))
        sfen = next_sfen


def reserve_leaf_path(
    book: OpeningBook,
    root_sfen: str,
    *,
    navigator: Callable[[str, str], str],
    multipv: int,
    c_puct: float,
    eval_scale: float,
    inflight: Set[str],
    max_ply: Optional[int] = None,
    book_side: Optional[str] = None,
    turn_provider: Optional[Callable[[str], str]] = None,
    root_best_eval: Optional[int] = None,
    eval_diff: Optional[int] = None,
) -> Optional[LeafPath]:
    """leaf を選択して inflight に予約する。予約済み leaf なら None を返す。"""
    path = select_available_leaf_path(
        book,
        book.position_key(root_sfen),
        navigator=navigator,
        multipv=multipv,
        c_puct=c_puct,
        eval_scale=eval_scale,
        inflight=inflight,
        max_ply=max_ply,
        book_side=book_side,
        turn_provider=turn_provider,
        root_best_eval=root_best_eval,
        eval_diff=eval_diff,
    )
    if path is None:
        return None
    leaf_key = path.leaf_sfen
    if leaf_key in inflight:
        return None
    inflight.add(leaf_key)
    return path


def select_available_leaf_path(
    book: OpeningBook,
    sfen: str,
    *,
    navigator: Callable[[str, str], str],
    multipv: int,
    c_puct: float,
    eval_scale: float,
    inflight: Set[str],
    max_ply: Optional[int],
    book_side: Optional[str],
    turn_provider: Optional[Callable[[str], str]],
    root_best_eval: Optional[int],
    eval_diff: Optional[int],
) -> Optional[LeafPath]:
    """予約済み leaf を避け、次善候補へバックトラックして leaf を選ぶ。"""
    return _select_available_leaf_path(
        book,
        sfen,
        navigator=navigator,
        multipv=multipv,
        c_puct=c_puct,
        eval_scale=eval_scale,
        inflight=inflight,
        max_ply=max_ply,
        book_side=book_side,
        turn_provider=turn_provider,
        root_best_eval=root_best_eval,
        eval_diff=eval_diff,
        depth=0,
        visited=set(),
    )


def _select_available_leaf_path(
    book: OpeningBook,
    sfen: str,
    *,
    navigator: Callable[[str, str], str],
    multipv: int,
    c_puct: float,
    eval_scale: float,
    inflight: Set[str],
    max_ply: Optional[int],
    book_side: Optional[str],
    turn_provider: Optional[Callable[[str], str]],
    root_best_eval: Optional[int],
    eval_diff: Optional[int],
    depth: int,
    visited: Set[str],
) -> Optional[LeafPath]:
    """select_available_leaf_path の再帰本体。"""
    if max_ply is not None and depth >= max_ply:
        return LeafPath(steps=[], leaf_sfen=sfen) if sfen not in inflight else None

    position = book.positions.get(sfen)
    if position is None or len(position.entries) < multipv or not position.entries:
        return LeafPath(steps=[], leaf_sfen=sfen) if sfen not in inflight else None
    if sfen in visited:
        return LeafPath(steps=[], leaf_sfen=sfen) if sfen not in inflight else None

    parent_visits = sum(entry.visits for entry in position.entries)
    filtered_entries = filter_entries_for_peta_rule(
        position.entries,
        sfen=sfen,
        book_side=book_side,
        turn_provider=turn_provider,
        root_best_eval=root_best_eval,
        eval_diff=eval_diff,
    )
    candidates: List[Tuple[float, BookEntry, str]] = []
    for entry in filtered_entries:
        child_sfen = book.position_key(navigator(sfen, entry.move))
        ucb = calculate_ucb(
            eval_cp=entry.eval_cp,
            child_visits=entry.visits,
            parent_visits=parent_visits,
            c_puct=c_puct,
            eval_scale=eval_scale,
        )
        candidates.append((ucb, entry, child_sfen))

    candidates.sort(key=lambda item: (item[0], -item[1].order_index), reverse=True)
    next_visited = set(visited)
    next_visited.add(sfen)
    for _, entry, child_sfen in candidates:
        child_path = _select_available_leaf_path(
            book,
            child_sfen,
            navigator=navigator,
            multipv=multipv,
            c_puct=c_puct,
            eval_scale=eval_scale,
            inflight=inflight,
            max_ply=max_ply,
            book_side=book_side,
            turn_provider=turn_provider,
            root_best_eval=root_best_eval,
            eval_diff=eval_diff,
            depth=depth + 1,
            visited=next_visited,
        )
        if child_path is not None:
            return LeafPath(
                steps=[PathStep(sfen=sfen, entry=entry)] + child_path.steps,
                leaf_sfen=child_path.leaf_sfen,
            )
    return None


def filter_entries_for_peta_rule(
    entries: Sequence[BookEntry],
    *,
    sfen: str,
    book_side: Optional[str],
    turn_provider: Optional[Callable[[str], str]],
    root_best_eval: Optional[int],
    eval_diff: Optional[int],
) -> List[BookEntry]:
    """PetaNext 由来の root 評価値基準で UCB 候補を絞る。"""
    if eval_diff is None:
        return list(entries)
    if book_side is None or turn_provider is None or root_best_eval is None:
        return list(entries)
    if not entries:
        return []

    turn = turn_provider(sfen)
    if turn == book_side:
        best_eval = max(entry.eval_cp for entry in entries)
        best_entries = [entry for entry in entries if entry.eval_cp == best_eval]
        return [min(best_entries, key=lambda entry: entry.order_index)]

    threshold = root_best_eval - eval_diff
    return [entry for entry in entries if entry.eval_cp >= threshold]


def propagate_minimax(book: OpeningBook, path: LeafPath, *, leaf_sfen: str) -> None:
    """leaf から root へ Min-Max 評価値を伝搬する。"""
    current_value = node_value(book, leaf_sfen)
    for step in reversed(path.steps):
        step.entry.eval_cp = -current_value
        current_value = node_value(book, step.sfen)


def node_value(book: OpeningBook, sfen: str) -> int:
    """局面の値を登録手の最大評価値として返す。"""
    position = book.positions.get(sfen)
    if position is None or not position.entries:
        return 0
    return max(entry.eval_cp for entry in position.entries)


def increment_path_visits(path: LeafPath) -> None:
    """選択経路上の指し手訪問回数を 1 増やす。"""
    for step in path.steps:
        step.entry.visits += 1


def parse_info_line(line: str) -> Optional[Tuple[int, SearchResult]]:
    """USI info 行から MultiPV の探索結果を抽出する。"""
    tokens = line.strip().split()
    if not tokens or tokens[0] != "info":
        return None
    if "lowerbound" in tokens or "upperbound" in tokens:
        return None
    depth = 0
    multipv = 1
    eval_cp: Optional[int] = None
    pv: List[str] = []
    index = 1
    while index < len(tokens):
        token = tokens[index]
        if token == "depth" and index + 1 < len(tokens):
            depth = int(tokens[index + 1])
            index += 2
        elif token == "multipv" and index + 1 < len(tokens):
            multipv = int(tokens[index + 1])
            index += 2
        elif token == "score" and index + 2 < len(tokens):
            eval_cp = parse_score(tokens[index + 1], tokens[index + 2])
            index += 3
        elif token == "pv":
            pv = tokens[index + 1 :]
            break
        else:
            index += 1
    if eval_cp is None or not pv:
        return None
    response = pv[1] if len(pv) >= 2 else "none"
    return multipv, SearchResult(pv[0], response, eval_cp, depth)


def is_no_response(response: str) -> bool:
    """定跡 DB の応手なし表記かどうかを返す。"""
    return response.lower() == "none"


def parse_score(score_type: str, value: str) -> int:
    """USI score を定跡 DB 用の評価値に変換する。"""
    if score_type == "cp":
        return int(value)
    if score_type == "mate":
        if value == "+":
            return MATE_SCORE
        if value == "-":
            return -MATE_SCORE
        mate = int(value)
        if mate > 0:
            return MATE_SCORE - mate
        return -MATE_SCORE - mate
    raise ValueError(f"未対応の score 種別です: {score_type}")


def sfen_after_move(sfen: str, move: str) -> str:
    """cshogi を使って 1 手後の SFEN を返す。"""
    if cshogi is None:
        raise RuntimeError("cshogi がインストールされていません")
    board = cshogi.Board()
    if sfen != "startpos":
        board.set_sfen(sfen)
    board.push_usi(move)
    return board.sfen()


def sfen_turn(sfen: str) -> str:
    """SFEN から手番を black / white で返す。"""
    if sfen == "startpos":
        return "black"
    tokens = sfen.split()
    if len(tokens) < 2:
        raise ValueError(f"手番を取得できない SFEN です: {sfen}")
    if tokens[1] == "b":
        return "black"
    if tokens[1] == "w":
        return "white"
    raise ValueError(f"未対応の SFEN 手番です: {tokens[1]}")


def root_best_eval(book: OpeningBook, root_sfen: str) -> Optional[int]:
    """root 局面の bestmove 評価値を返す。"""
    position = book.positions.get(book.position_key(root_sfen))
    if position is None or not position.entries:
        return None
    return max(entry.eval_cp for entry in position.entries)


class UsiEngine:
    """USI エンジン 1 プロセスを管理する。"""

    def __init__(
        self,
        engine_path: pathlib.Path,
        *,
        usi_hash: int,
        threads: int,
        multipv: int,
        extra_options: Sequence[Tuple[str, Optional[str]]],
        stderr: Optional[TextIO] = None,
    ) -> None:
        self.engine_path = pathlib.Path(engine_path)
        self.usi_hash = usi_hash
        self.threads = threads
        self.multipv = multipv
        self.extra_options = list(extra_options)
        self.stderr = stderr
        self.process: Optional[subprocess.Popen[str]] = None
        self.option_names: Set[str] = set()

    def start(self) -> None:
        """エンジンを起動し、初期化コマンドを 1 回だけ送る。"""
        self.process = subprocess.Popen(
            [str(self.engine_path)],
            cwd=str(self.engine_path.parent),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL if self.stderr is None else self.stderr,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
        )
        self.send("usi")
        self.wait_for("usiok")
        self.initialize_options()

    def initialize_options(self) -> None:
        """起動済みエンジンへ初期化 option と ready チェックを 1 回だけ送る。"""
        self.setoption(self.hash_option_name(), str(self.usi_hash))
        self.setoption("Threads", str(self.threads))
        self.setoption("MultiPV", str(self.multipv))
        for name, value in self.extra_options:
            self.setoption(name, value)
        self.send("isready")
        self.wait_for("readyok")
        self.send("usinewgame")

    def hash_option_name(self) -> str:
        """エンジンが対応するハッシュ option 名を返す。"""
        if "USI_Hash" in self.option_names:
            return "USI_Hash"
        return "Hash"

    def close(self) -> None:
        """エンジンへ quit を送り、プロセスを終了する。"""
        if self.process is None:
            return
        try:
            self.send("quit")
            self.process.wait(timeout=5)
        except Exception:
            self.process.kill()
        finally:
            self.process = None

    def search(self, sfen: str, nodes: int) -> List[SearchResult]:
        """指定局面を go nodes で探索し、最終 MultiPV 結果を返す。"""
        self.send(f"position sfen {sfen}")
        self.send(f"go nodes {nodes}")
        latest: Dict[int, SearchResult] = {}
        while True:
            line = self.readline()
            if line.startswith("bestmove "):
                break
            parsed = parse_info_line(line)
            if parsed is not None:
                multipv, result = parsed
                latest[multipv] = result
        return [latest[index] for index in sorted(latest)]

    def setoption(self, name: str, value: Optional[str]) -> None:
        """USI setoption コマンドを送る。"""
        if value is None:
            self.send(f"setoption name {name}")
        else:
            self.send(f"setoption name {name} value {value}")

    def send(self, command: str) -> None:
        """USI エンジンへ 1 行送る。"""
        if self.process is None or self.process.stdin is None:
            raise RuntimeError("エンジンが起動していません")
        self.process.stdin.write(command + "\n")
        self.process.stdin.flush()

    def readline(self) -> str:
        """USI エンジンから 1 行読む。"""
        if self.process is None or self.process.stdout is None:
            raise RuntimeError("エンジンが起動していません")
        line = self.process.stdout.readline()
        if line == "":
            raise RuntimeError("エンジンが終了しました")
        stripped = line.strip()
        self.remember_option_name(stripped)
        return stripped

    def wait_for(self, expected: str) -> None:
        """指定した USI 応答が来るまで読み飛ばす。"""
        while True:
            line = self.readline()
            if line == expected:
                return

    def remember_option_name(self, line: str) -> None:
        """usi 応答の option 行から option 名を記録する。"""
        if not line.startswith("option name "):
            return
        rest = line[len("option name ") :]
        name, sep, _ = rest.partition(" type ")
        if sep and name:
            self.option_names.add(name)


def parse_setoption(option: str) -> Tuple[str, Optional[str]]:
    """CLI の --setoption 値を USI option 名と値に分解する。"""
    if "=" not in option:
        return option, None
    name, value = option.split("=", 1)
    return name, value


def worker_loop(
    *,
    worker_id: int,
    engine: UsiEngine,
    book: OpeningBook,
    root_sfen: str,
    nodes: int,
    multipv: int,
    c_puct: float,
    eval_scale: float,
    max_ply: int,
    book_side: Optional[str],
    root_eval: Optional[int],
    eval_diff: Optional[int],
    book_lock: threading.Lock,
    inflight: Set[str],
    stats: RunStats,
    stop_limits: StopLimits,
    stop_event: threading.Event,
    progress: ProgressReporter,
) -> None:
    """1 エンジン専有スレッドで leaf 選択と探索を繰り返す。"""
    idle_sleep_sec = 0.05
    while not stop_event.is_set():
        with book_lock:
            if stop_limits.should_stop(stats) or not stop_limits.can_start_search(stats, nodes=nodes):
                stop_event.set()
                return
            path = reserve_leaf_path(
                book,
                root_sfen,
                navigator=sfen_after_move,
                multipv=multipv,
                c_puct=c_puct,
                eval_scale=eval_scale,
                inflight=inflight,
                max_ply=max_ply,
                book_side=book_side,
                turn_provider=sfen_turn,
                root_best_eval=root_eval,
                eval_diff=eval_diff,
            )
            if path is None:
                leaf_key = None
                leaf_sfen = None
            else:
                leaf_key = path.leaf_sfen
                leaf_sfen = book.output_sfen(leaf_key)
                stats.searches += 1
                stats.total_nodes += nodes
                progress.search_start(
                    worker_id=worker_id,
                    leaf_sfen=leaf_sfen,
                    depth=len(path.steps),
                    nodes=nodes,
                    stats=stats,
                )
        if path is None:
            stop_event.wait(idle_sleep_sec)
            continue
        assert leaf_key is not None
        assert leaf_sfen is not None
        try:
            results = engine.search(leaf_sfen, nodes)
            with book_lock:
                added_position = leaf_key not in book.positions
                position = book.ensure_position(leaf_sfen)
                for result in results:
                    if position.find_entry(result.move) is None:
                        book.append_entry(position, result)
                    else:
                        merge_search_results(position, [result], selected_move=None)
                increment_path_visits(path)
                propagate_minimax(book, path, leaf_sfen=leaf_key)
                if added_position:
                    stats.added_positions += 1
                if stop_limits.should_stop(stats):
                    stop_event.set()
                progress.search_finish(
                    worker_id=worker_id,
                    entries=len(results),
                    leaf_sfen=leaf_sfen,
                    depth=len(path.steps),
                    stats=stats,
                )
                progress.maybe_progress(stats)
        finally:
            with book_lock:
                inflight.discard(leaf_key)


def run_extend_loop(args: argparse.Namespace, progress_stream: TextIO = sys.stderr) -> None:
    """CLI 引数に従ってエンジンを起動し、Ctrl+C まで定跡拡張を続ける。"""
    book = OpeningBook.load(args.input, ignore_ply=args.ignore_ply)
    root_eval = root_best_eval(book, args.root_sfen)
    book_lock = threading.Lock()
    inflight: Set[str] = set()
    stop_event = threading.Event()
    stats = RunStats(start_time=time.monotonic())
    progress = ProgressReporter(progress_stream, interval_sec=args.progress_interval_sec)
    stop_limits = StopLimits(
        max_added_positions=args.max_added_positions,
        max_searches=args.max_searches,
        max_total_nodes=args.max_total_nodes,
        max_runtime_sec=args.max_runtime_sec,
    )
    options = [parse_setoption(option) for option in args.setoption]
    engines = [
        UsiEngine(
            args.engine,
            usi_hash=args.usi_hash,
            threads=args.threads,
            multipv=args.multipv,
            extra_options=options,
        )
        for _ in range(args.engine_count)
    ]
    threads: List[threading.Thread] = []
    try:
        for engine in engines:
            engine.start()

        for worker_id, engine in enumerate(engines):
            thread = threading.Thread(
                target=worker_loop,
                kwargs={
                    "worker_id": worker_id,
                    "engine": engine,
                    "book": book,
                    "root_sfen": args.root_sfen,
                    "nodes": args.nodes,
                    "multipv": args.multipv,
                    "c_puct": args.c_puct,
                    "eval_scale": args.eval_scale,
                    "max_ply": args.max_ply,
                    "book_side": args.book_side if args.eval_diff is not None else None,
                    "root_eval": root_eval,
                    "eval_diff": args.eval_diff,
                    "book_lock": book_lock,
                    "inflight": inflight,
                    "stats": stats,
                    "stop_limits": stop_limits,
                    "stop_event": stop_event,
                    "progress": progress,
                },
                daemon=True,
            )
            thread.start()
            threads.append(thread)

        next_save = time.monotonic() + args.save_interval_sec
        while not stop_event.is_set():
            time.sleep(0.2)
            with book_lock:
                reason = stop_limits.stop_reason(stats)
                if reason is not None:
                    stop_event.set()
                    break
                progress.maybe_progress(stats)
            if time.monotonic() >= next_save:
                with book_lock:
                    book.write_atomic(args.output, args.backup_count)
                    progress.save(args.output, stats)
                next_save = time.monotonic() + args.save_interval_sec
    except KeyboardInterrupt:
        with book_lock:
            progress.stop("keyboard-interrupt", stats)
    finally:
        stop_event.set()
        for thread in threads:
            thread.join(timeout=1)
        with book_lock:
            book.write_atomic(args.output, args.backup_count)
            final_reason = stop_limits.stop_reason(stats) or "finished"
            progress.save(args.output, stats)
            progress.stop(final_reason, stats)
        for engine in engines:
            engine.close()


def build_arg_parser() -> argparse.ArgumentParser:
    """CLI 引数パーサを構築する。"""
    parser = argparse.ArgumentParser(description="MCTS 風 traversal でやねうら王定跡 DB を拡張します。")
    parser.add_argument("--input", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--engine", type=pathlib.Path, required=True)
    parser.add_argument("--engine-count", type=int, required=True)
    parser.add_argument("--nodes", type=int, required=True)
    parser.add_argument("--multipv", type=int, required=True)
    parser.add_argument("--usi-hash", type=int, default=1024)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--setoption", action="append", default=[])
    parser.add_argument("--c-puct", type=float, default=1.4)
    parser.add_argument("--eval-scale", type=float, default=600.0)
    parser.add_argument("--eval-diff", type=int, default=None)
    parser.add_argument("--book-side", choices=["black", "white"], default="black")
    parser.add_argument("--max-ply", type=int, default=200)
    parser.add_argument("--save-interval-sec", type=float, default=300.0)
    parser.add_argument("--progress-interval-sec", type=float, default=10.0)
    parser.add_argument("--backup-count", type=int, default=3)
    parser.add_argument("--root-sfen", default=initial_sfen())
    parser.add_argument("--ignore-ply", action="store_true")
    parser.add_argument("--max-added-positions", type=int, default=None)
    parser.add_argument("--max-searches", type=int, default=None)
    parser.add_argument("--max-total-nodes", type=int, default=None)
    parser.add_argument("--max-runtime-sec", type=float, default=None)
    return parser


def initial_sfen() -> str:
    """平手初期局面の SFEN を返す。"""
    if cshogi is None:
        return "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1"
    return cshogi.Board().sfen()


def main(argv: Optional[Sequence[str]] = None) -> int:
    """コマンドラインエントリポイント。"""
    parser = build_arg_parser()
    args = parser.parse_args(argv)
    if args.engine_count <= 0:
        parser.error("--engine-count は 1 以上を指定してください")
    if args.nodes <= 0:
        parser.error("--nodes は 1 以上を指定してください")
    if args.multipv <= 0:
        parser.error("--multipv は 1 以上を指定してください")
    if args.eval_diff is not None and args.eval_diff < 0:
        parser.error("--eval-diff は 0 以上を指定してください")
    if args.max_ply < 0:
        parser.error("--max-ply は 0 以上を指定してください")
    if args.max_added_positions is not None and args.max_added_positions <= 0:
        parser.error("--max-added-positions は 1 以上を指定してください")
    if args.max_searches is not None and args.max_searches <= 0:
        parser.error("--max-searches は 1 以上を指定してください")
    if args.max_total_nodes is not None and args.max_total_nodes <= 0:
        parser.error("--max-total-nodes は 1 以上を指定してください")
    if args.max_runtime_sec is not None and args.max_runtime_sec <= 0:
        parser.error("--max-runtime-sec は正の値を指定してください")
    if args.progress_interval_sec < 0:
        parser.error("--progress-interval-sec は 0 以上を指定してください")
    run_extend_loop(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
