from __future__ import annotations

import argparse
import csv
import datetime as dt
import html
import pathlib
import re
import sys
from dataclasses import dataclass
from typing import Any

import yaml


LONG_TERM_PATTERN = re.compile(r"^players-floodgate-(?!14-).+\.(?:ya?ml|html?)$", re.IGNORECASE)
DATE_TOKEN_PATTERN = re.compile(r"(\d{14}|\d{8})")
CSA_MOVE_PATTERN = re.compile(r"^([+-])(\d{2})(\d{2})([A-Z]{2})$")
HTML_ROW_PATTERN = re.compile(
    r'<tr\b[^>]*>.*?<td\b[^>]*class="name"[^>]*>.*?(?:<a\b[^>]*>)?(?P<name>[^<]+)'
    r'(?:</a>)?.*?</td>.*?<td\b[^>]*class="rate"[^>]*>.*?(?:<span\b[^>]*>)?\s*(?P<rate>-?\d+(?:\.\d+)?)',
    re.IGNORECASE | re.DOTALL,
)

RANK_TO_USI = {
    1: "a",
    2: "b",
    3: "c",
    4: "d",
    5: "e",
    6: "f",
    7: "g",
    8: "h",
    9: "i",
}

PROMOTION_MAP = {
    "FU": "TO",
    "KY": "NY",
    "KE": "NK",
    "GI": "NG",
    "KA": "UM",
    "HI": "RY",
}
UNPROMOTE_MAP = {promoted: raw for raw, promoted in PROMOTION_MAP.items()}
DROP_USI = {
    "FU": "P",
    "KY": "L",
    "KE": "N",
    "GI": "S",
    "KI": "G",
    "KA": "B",
    "HI": "R",
}


@dataclass(frozen=True)
class RatingSnapshot:
    snapshot_time: dt.datetime
    ratings: dict[str, float]
    path: pathlib.Path


@dataclass(frozen=True)
class CsaGame:
    path: pathlib.Path
    game_time: dt.datetime | None
    black_name: str
    white_name: str
    result: str | None
    usi_moves: list[str]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Select Floodgate CSA games using long-term ratings and emit policylearn-ready position lines."
    )
    parser.add_argument("--csa-dir", required=True, help="Root directory containing extracted Floodgate CSA files.")
    parser.add_argument(
        "--rating-dir",
        required=True,
        help="Directory containing players-floodgate-* long-term rating snapshots (.html/.yaml).",
    )
    parser.add_argument("--output", required=True, help="Output text file with 'position startpos moves ...' lines.")
    parser.add_argument(
        "--manifest",
        help="Optional CSV manifest for selected games with rating metadata.",
    )
    parser.add_argument(
        "--min-rating",
        type=float,
        default=3300.0,
        help="Minimum long-term rating required for both players. Default: 3300.",
    )
    parser.add_argument(
        "--min-plies",
        type=int,
        default=0,
        help="Skip games shorter than this many plies. Default: 0.",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=0,
        help="Stop after writing this many selected games. Default: 0 (no limit).",
    )
    parser.add_argument(
        "--allow-nonstandard-result",
        action="store_true",
        help="Keep games even when the terminal CSA result token is missing or uncommon.",
    )
    return parser.parse_args()


def parse_snapshot_time(path: pathlib.Path) -> dt.datetime:
    match = DATE_TOKEN_PATTERN.search(path.stem)
    if not match:
        raise ValueError(f"Could not parse snapshot date from {path}")
    token = match.group(1)
    if len(token) == 14:
        return dt.datetime.strptime(token, "%Y%m%d%H%M%S")
    return dt.datetime.strptime(token, "%Y%m%d")


def normalize_player_name(value: str) -> str:
    return value.strip()


def extract_ratings_from_yaml(node: Any) -> dict[str, float]:
    ratings: dict[str, float] = {}

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            lower_key_map = {str(key).lower(): key for key in value.keys()}
            name_key = next((lower_key_map[key] for key in ("name", "player") if key in lower_key_map), None)
            rating_key = next((lower_key_map[key] for key in ("rate", "rating") if key in lower_key_map), None)

            if name_key is not None and rating_key is not None:
                try:
                    ratings[normalize_player_name(str(value[name_key]))] = float(value[rating_key])
                except (TypeError, ValueError):
                    pass

            for key, child in value.items():
                if isinstance(child, dict):
                    child_keys = {str(k).lower() for k in child.keys()}
                    if {"rate", "rating"} & child_keys:
                        rating_field = "rate" if "rate" in child_keys else "rating"
                        try:
                            ratings[normalize_player_name(str(key))] = float(child[rating_field])
                        except (KeyError, TypeError, ValueError):
                            pass
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(node)
    return ratings


def extract_ratings_from_html(text: str) -> dict[str, float]:
    ratings: dict[str, float] = {}
    for match in HTML_ROW_PATTERN.finditer(text):
        name = normalize_player_name(html.unescape(match.group("name")))
        if not name:
            continue
        try:
            ratings[name] = float(match.group("rate"))
        except ValueError:
            continue
    return ratings


def load_rating_snapshots(rating_dir: pathlib.Path) -> list[RatingSnapshot]:
    snapshots: list[RatingSnapshot] = []
    for path in sorted(rating_dir.rglob("*")):
        if path.suffix.lower() not in {".yaml", ".yml", ".html", ".htm"}:
            continue
        if not LONG_TERM_PATTERN.match(path.name):
            continue
        with path.open("r", encoding="utf-8", errors="replace") as fh:
            content = fh.read()
        if path.suffix.lower() in {".yaml", ".yml"}:
            payload = yaml.safe_load(content)
            ratings = extract_ratings_from_yaml(payload)
        else:
            ratings = extract_ratings_from_html(content)
        if not ratings:
            continue
        snapshots.append(RatingSnapshot(parse_snapshot_time(path), ratings, path))

    snapshots.sort(key=lambda snapshot: snapshot.snapshot_time)
    return snapshots


def find_rating(snapshot_list: list[RatingSnapshot], player_name: str, game_time: dt.datetime | None) -> tuple[float | None, pathlib.Path | None]:
    if not snapshot_list:
        return None, None

    matching_snapshot: RatingSnapshot | None = None
    if game_time is not None:
        for snapshot in snapshot_list:
            if snapshot.snapshot_time <= game_time:
                matching_snapshot = snapshot
            else:
                break

    if matching_snapshot is None:
        matching_snapshot = snapshot_list[-1]

    if player_name in matching_snapshot.ratings:
        return matching_snapshot.ratings[player_name], matching_snapshot.path

    for snapshot in reversed(snapshot_list):
        if game_time is not None and snapshot.snapshot_time > game_time:
            continue
        if player_name in snapshot.ratings:
            return snapshot.ratings[player_name], snapshot.path

    return None, None


def initial_board() -> tuple[dict[tuple[int, int], tuple[str, str]], dict[str, dict[str, int]]]:
    board: dict[tuple[int, int], tuple[str, str]] = {}
    hands = {
        "+": {piece: 0 for piece in DROP_USI},
        "-": {piece: 0 for piece in DROP_USI},
    }

    top = ["KY", "KE", "GI", "KI", "OU", "KI", "GI", "KE", "KY"]
    bottom = list(reversed(top))

    for file_index, piece in enumerate(top, start=1):
        board[(file_index, 1)] = ("-", piece)
    board[(2, 2)] = ("-", "HI")
    board[(8, 2)] = ("-", "KA")
    for file_index in range(1, 10):
        board[(file_index, 3)] = ("-", "FU")

    for file_index in range(1, 10):
        board[(file_index, 7)] = ("+", "FU")
    board[(2, 8)] = ("+", "KA")
    board[(8, 8)] = ("+", "HI")
    for file_index, piece in enumerate(bottom, start=1):
        board[(file_index, 9)] = ("+", piece)

    return board, hands


def square_to_usi(square: str) -> str:
    file_index = int(square[0])
    rank_index = int(square[1])
    return f"{file_index}{RANK_TO_USI[rank_index]}"


def parse_game_time(path: pathlib.Path) -> dt.datetime | None:
    candidates: list[str] = []
    for part in path.parts:
        candidates.extend(match.group(1) for match in DATE_TOKEN_PATTERN.finditer(part))

    if not candidates:
        return None

    token = max(candidates, key=len)
    try:
        if len(token) == 14:
            return dt.datetime.strptime(token, "%Y%m%d%H%M%S")
        return dt.datetime.strptime(token, "%Y%m%d")
    except ValueError:
        return None


def convert_csa_move_to_usi(
    board: dict[tuple[int, int], tuple[str, str]],
    hands: dict[str, dict[str, int]],
    move_text: str,
) -> str:
    match = CSA_MOVE_PATTERN.match(move_text)
    if not match:
        raise ValueError(f"Invalid CSA move: {move_text}")

    side, from_sq_text, to_sq_text, piece_after = match.groups()
    to_sq = (int(to_sq_text[0]), int(to_sq_text[1]))

    if from_sq_text == "00":
        piece_before = UNPROMOTE_MAP.get(piece_after, piece_after)
        if piece_before not in DROP_USI:
            raise ValueError(f"Unsupported drop piece in CSA move: {move_text}")
        hands[side][piece_before] -= 1
        board[to_sq] = (side, piece_before)
        return f"{DROP_USI[piece_before]}*{square_to_usi(to_sq_text)}"

    from_sq = (int(from_sq_text[0]), int(from_sq_text[1]))
    moving_side, moving_piece = board.pop(from_sq)
    if moving_side != side:
        raise ValueError(f"CSA side mismatch at {move_text}")

    captured = board.pop(to_sq, None)
    if captured is not None:
        captured_piece = UNPROMOTE_MAP.get(captured[1], captured[1])
        hands[side][captured_piece] += 1

    promoted = PROMOTION_MAP.get(moving_piece) == piece_after
    placed_piece = piece_after if promoted or moving_piece == piece_after else piece_after
    board[to_sq] = (side, placed_piece)

    usi = f"{square_to_usi(from_sq_text)}{square_to_usi(to_sq_text)}"
    if promoted:
        usi += "+"
    return usi


def parse_csa_game(path: pathlib.Path) -> CsaGame:
    board, hands = initial_board()
    black_name = ""
    white_name = ""
    result: str | None = None
    usi_moves: list[str] = []

    with path.open("r", encoding="utf-8", errors="replace") as fh:
        for raw_line in fh:
            line = raw_line.strip()
            if not line or line.startswith("'"):
                continue

            if "," in line:
                line = line.split(",", 1)[0]

            if line.startswith("N+"):
                black_name = line[2:].strip()
                continue
            if line.startswith("N-"):
                white_name = line[2:].strip()
                continue
            if line in {"+", "-"}:
                continue
            if line == "PI":
                continue
            if line.startswith("P"):
                # Floodgate should be startpos; skip explicit board lines.
                continue
            if line.startswith("%"):
                result = line[1:]
                continue
            if line.startswith("+") or line.startswith("-"):
                usi_moves.append(convert_csa_move_to_usi(board, hands, line))

    if not black_name or not white_name:
        raise ValueError(f"Missing player names in {path}")

    return CsaGame(
        path=path,
        game_time=parse_game_time(path),
        black_name=black_name,
        white_name=white_name,
        result=result,
        usi_moves=usi_moves,
    )


def accepted_result(result: str | None, allow_nonstandard_result: bool) -> bool:
    if result is None:
        return allow_nonstandard_result
    return result in {
        "TORYO",
        "KACHI",
        "TSUMI",
        "SENNICHITE",
        "JISHOGI",
        "ILLEGAL_MOVE",
        "TIME_UP",
        "CHUDAN",
    }


def build_position_command(usi_moves: list[str]) -> str:
    if not usi_moves:
        return "position startpos"
    return "position startpos moves " + " ".join(usi_moves)


def select_games(
    csa_dir: pathlib.Path,
    snapshots: list[RatingSnapshot],
    min_rating: float,
    min_plies: int,
    allow_nonstandard_result: bool,
    limit: int,
) -> list[tuple[CsaGame, float, float, pathlib.Path | None, pathlib.Path | None]]:
    selected: list[tuple[CsaGame, float, float, pathlib.Path | None, pathlib.Path | None]] = []

    for path in sorted(csa_dir.rglob("*.csa")):
        try:
            game = parse_csa_game(path)
        except Exception:
            continue

        if len(game.usi_moves) < min_plies:
            continue
        if not accepted_result(game.result, allow_nonstandard_result):
            continue

        black_rating, black_snapshot = find_rating(snapshots, game.black_name, game.game_time)
        white_rating, white_snapshot = find_rating(snapshots, game.white_name, game.game_time)
        if black_rating is None or white_rating is None:
            continue
        if black_rating <= min_rating or white_rating <= min_rating:
            continue

        selected.append((game, black_rating, white_rating, black_snapshot, white_snapshot))
        if limit and len(selected) >= limit:
            break

    return selected


def write_outputs(
    selected: list[tuple[CsaGame, float, float, pathlib.Path | None, pathlib.Path | None]],
    output_path: pathlib.Path,
    manifest_path: pathlib.Path | None,
) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", encoding="utf-8", newline="\n") as fh:
        for game, _, _, _, _ in selected:
            fh.write(build_position_command(game.usi_moves))
            fh.write("\n")

    if manifest_path is None:
        return

    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    with manifest_path.open("w", encoding="utf-8", newline="") as fh:
        writer = csv.writer(fh)
        writer.writerow(
            [
                "csa_path",
                "game_time",
                "black_name",
                "black_rating",
                "black_snapshot",
                "white_name",
                "white_rating",
                "white_snapshot",
                "result",
                "plies",
            ]
        )
        for game, black_rating, white_rating, black_snapshot, white_snapshot in selected:
            writer.writerow(
                [
                    str(game.path),
                    game.game_time.isoformat() if game.game_time is not None else "",
                    game.black_name,
                    black_rating,
                    str(black_snapshot) if black_snapshot is not None else "",
                    game.white_name,
                    white_rating,
                    str(white_snapshot) if white_snapshot is not None else "",
                    game.result or "",
                    len(game.usi_moves),
                ]
            )


def main() -> int:
    args = parse_args()
    csa_dir = pathlib.Path(args.csa_dir)
    rating_dir = pathlib.Path(args.rating_dir)
    output_path = pathlib.Path(args.output)
    manifest_path = pathlib.Path(args.manifest) if args.manifest else None

    snapshots = load_rating_snapshots(rating_dir)
    if not snapshots:
        print("No long-term rating snapshots found.", file=sys.stderr)
        return 1

    selected = select_games(
        csa_dir=csa_dir,
        snapshots=snapshots,
        min_rating=args.min_rating,
        min_plies=args.min_plies,
        allow_nonstandard_result=args.allow_nonstandard_result,
        limit=args.limit,
    )
    write_outputs(selected, output_path, manifest_path)

    print(
        f"selected_games={len(selected)} output={output_path}"
        + (f" manifest={manifest_path}" if manifest_path else "")
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
