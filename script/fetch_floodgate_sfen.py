#!/usr/bin/env python
"""Fetch Floodgate CSA games and save as SFEN (USI move list).

Example:
  python script/fetch_floodgate_sfen.py --start-date 2025-01-01 --end-date 2025-01-31 --output floodgate_3300.sfen
"""

from __future__ import annotations

import argparse
import datetime as _dt
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.parse
import urllib.request

RATING_URL_DEFAULT = "http://wdoor.c.u-tokyo.ac.jp/shogi/logs/LATEST/players-floodgate.html"
CSA_BASE_URL_DEFAULT = "https://p.mzr.jp/wdoor-latest/"
ARCHIVE_BASE_URL_DEFAULT = "http://wdoor.c.u-tokyo.ac.jp/shogi/x/"

PROMOTED_SET = {"TO", "NY", "NK", "NG", "UM", "RY"}
CSA_TO_PIECE = {
    "FU": "P",
    "KY": "L",
    "KE": "N",
    "GI": "S",
    "KI": "G",
    "KA": "B",
    "HI": "R",
    "OU": "K",
    "TO": "+P",
    "NY": "+L",
    "NK": "+N",
    "NG": "+S",
    "UM": "+B",
    "RY": "+R",
}

RANK_TO_LETTER = {
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

STARTPOS_SFEN = "lnsgkgsnl/1r5b1/p1ppppppp/9/9/9/P1PPPPPPP/1B5R1/LNSGKGSNL b - 1"


def fetch_text(url: str, user_agent: str, timeout: int = 30) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": user_agent})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        charset = resp.headers.get_content_charset() or "utf-8"
        return resp.read().decode(charset, errors="replace")


def parse_strong_players(html: str, min_rating: int, include_equal: bool) -> set[str]:
    strong = set()
    in_group0 = False
    pending_name = None

    for line in html.splitlines():
        if "<caption>Group: 0" in line:
            in_group0 = True
            continue
        if in_group0 and "</table>" in line:
            break
        if not in_group0:
            continue

        if "<a id=\"popup" in line:
            m = re.search(r">\s*([^<]+)\s*<", line)
            if m:
                pending_name = m.group(1).strip()
            continue

        if "<span id=\"popup" in line and pending_name:
            m = re.search(r">\s*([^<]+)\s*<", line)
            if not m:
                continue
            rate_str = m.group(1).strip()
            if not rate_str or not rate_str[0].isdigit():
                continue
            try:
                rate = int(rate_str)
            except ValueError:
                continue
            if include_equal:
                ok = rate >= min_rating
            else:
                ok = rate > min_rating
            if ok:
                strong.add(pending_name)
            pending_name = None

    return strong


def date_range(start: _dt.date, end: _dt.date):
    if end < start:
        raise ValueError("end-date must be >= start-date")
    cur = start
    while cur <= end:
        yield cur
        cur += _dt.timedelta(days=1)


def list_csa_urls(day: _dt.date, base_url: str, user_agent: str, timeout: int) -> list[str]:
    ymd_path = f"{day.year:04d}/{day.month:02d}/{day.day:02d}/"
    url = urllib.parse.urljoin(base_url.rstrip("/") + "/", ymd_path)
    try:
        html = fetch_text(url, user_agent, timeout=timeout)
    except Exception:
        return []

    urls = []
    for m in re.finditer(r"href=\"([^\"]+\.csa)\"", html, re.IGNORECASE):
        href = urllib.parse.unquote(m.group(1))
        urls.append(urllib.parse.urljoin(url, href))
    return urls


def list_yearly_archive_urls(year: int, base_url: str, user_agent: str, timeout: int) -> list[str]:
    url = base_url.rstrip("/") + "/"
    urls = []
    try:
        html = fetch_text(url, user_agent, timeout=timeout)
        for m in re.finditer(r"href=\"([^\"]+\.7z)\"", html, re.IGNORECASE):
            href = urllib.parse.unquote(m.group(1))
            name = href.rsplit("/", 1)[-1]
            if re.search(rf"{year}", name):
                urls.append(urllib.parse.urljoin(url, href))
    except Exception:
        urls = []

    if not urls:
        urls = [f"{url}wdoor{year}.7z"]
    return urls


def download_file(url: str, dst_path: str, user_agent: str, timeout: int) -> bool:
    os.makedirs(os.path.dirname(dst_path), exist_ok=True)
    req = urllib.request.Request(url, headers={"User-Agent": user_agent})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp, open(dst_path, "wb") as f:
            shutil.copyfileobj(resp, f)
        return True
    except Exception:
        return False


def try_extract_7z(archive_path: str, extract_dir: str) -> bool:
    try:
        import py7zr  # type: ignore

        with py7zr.SevenZipFile(archive_path, mode="r") as zf:
            zf.extractall(extract_dir)
        return True
    except Exception:
        pass

    seven_zip = shutil.which("7z") or shutil.which("7za")
    if not seven_zip:
        return False

    os.makedirs(extract_dir, exist_ok=True)
    cmd = [seven_zip, "x", "-y", f"-o{extract_dir}", archive_path]
    result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return result.returncode == 0


def parse_sfen_board(sfen: str):
    board_part = sfen.split()[0]
    ranks = board_part.split("/")
    if len(ranks) != 9:
        raise ValueError("invalid sfen board")

    board = {}
    current_rank = 9
    for rank_str in ranks:
        file_idx = 1
        i = 0
        while i < len(rank_str):
            ch = rank_str[i]
            if ch.isdigit():
                file_idx += int(ch)
                i += 1
                continue
            promoted = False
            if ch == "+":
                promoted = True
                i += 1
                ch = rank_str[i]
            piece = ch
            color = "b" if piece.isupper() else "w"
            base = piece.upper()
            piece_str = ("+" if promoted else "") + base
            board[(file_idx, current_rank)] = (color, piece_str)
            file_idx += 1
            i += 1
        current_rank -= 1
    return board


def to_usi_square(file: int, rank: int) -> str:
    return f"{file}{RANK_TO_LETTER[rank]}"


def csa_to_usi_moves(csa_text: str, require_toryo: bool):
    black = ""
    white = ""
    result = ""

    board = parse_sfen_board(STARTPOS_SFEN)
    hands = {"b": {p: 0 for p in "PLNSGBR"}, "w": {p: 0 for p in "PLNSGBR"}}
    moves = []

    for raw in csa_text.splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.startswith("N+"):
            black = line[2:].strip()
            continue
        if line.startswith("N-"):
            white = line[2:].strip()
            continue
        if line.startswith("%"):
            result = line[1:].strip().upper()
            break
        if not (line.startswith("+") or line.startswith("-")):
            continue

        m = re.match(r"^([+-])(\d{2})(\d{2})([A-Z]{2})", line)
        if not m:
            continue
        side = "b" if m.group(1) == "+" else "w"
        from_sq = m.group(2)
        to_sq = m.group(3)
        piece_code = m.group(4)

        if piece_code not in CSA_TO_PIECE:
            continue
        to_piece = CSA_TO_PIECE[piece_code]

        to_file = int(to_sq[0])
        to_rank = int(to_sq[1])
        to_usi = to_usi_square(to_file, to_rank)

        if from_sq == "00":
            drop_piece = to_piece.replace("+", "")
            if hands[side][drop_piece] > 0:
                hands[side][drop_piece] -= 1
            board[(to_file, to_rank)] = (side, drop_piece)
            usi = f"{drop_piece}*{to_usi}"
            moves.append(usi)
            continue

        from_file = int(from_sq[0])
        from_rank = int(from_sq[1])
        from_usi = to_usi_square(from_file, from_rank)

        moving = board.get((from_file, from_rank))
        if moving is None:
            return None

        _, moving_piece = moving
        promotion = False
        if moving_piece.startswith("+"):
            promotion = False
        else:
            if piece_code in PROMOTED_SET:
                promotion = True

        captured = board.get((to_file, to_rank))
        if captured:
            cap_color, cap_piece = captured
            cap_base = cap_piece.replace("+", "")
            hands[side][cap_base] += 1

        board.pop((from_file, from_rank), None)
        board[(to_file, to_rank)] = (side, to_piece)

        usi = f"{from_usi}{to_usi}{'+' if promotion else ''}"
        moves.append(usi)

    if require_toryo and result != "TORYO":
        return None

    return {
        "black": black,
        "white": white,
        "moves": moves,
    }


def parse_names_from_filename(url: str):
    name = urllib.parse.unquote(url.rsplit("/", 1)[-1])
    return parse_names_from_csa_filename(name)


def parse_names_from_csa_filename(name: str):
    m = re.search(r"\+([^+]+)\+([^+]+)\+\d{14}\.csa$", name)
    if not m:
        return "", ""
    return m.group(1), m.group(2)


def parse_date_from_csa_filename(name: str):
    m = re.search(r"\+(\d{14})\.csa$", name)
    if not m:
        return None
    stamp = m.group(1)
    try:
        return _dt.datetime.strptime(stamp, "%Y%m%d%H%M%S").date()
    except ValueError:
        return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--start-date", required=True, help="YYYY-MM-DD")
    parser.add_argument("--end-date", required=True, help="YYYY-MM-DD")
    parser.add_argument("--min-rating", type=int, default=3300)
    parser.add_argument("--include-equal", action="store_true", help=">= min rating")
    parser.add_argument("--rating-url", default=RATING_URL_DEFAULT)
    parser.add_argument("--use-rating-filter", action="store_true", help="filter by rating list")
    parser.add_argument("--csa-base-url", default=CSA_BASE_URL_DEFAULT)
    parser.add_argument("--archive-base-url", default=ARCHIVE_BASE_URL_DEFAULT)
    parser.add_argument("--output", default="floodgate_3300.sfen")
    parser.add_argument("--require-toryo", action="store_true")
    parser.add_argument("--max-games", type=int, default=0, help="0 = unlimited")
    parser.add_argument("--sleep", type=float, default=0.0)
    parser.add_argument("--progress-every", type=int, default=200)
    parser.add_argument("--use-archive", action="store_true", help="prefer yearly .7z archives")
    parser.add_argument("--archive-dir", default="floodgate_archives")
    parser.add_argument("--force-extract", action="store_true")
    parser.add_argument("--user-agent", default="hakubishin-fetch/1.0")
    parser.add_argument("--timeout", type=int, default=30)
    args = parser.parse_args()

    start = _dt.date.fromisoformat(args.start_date)
    end = _dt.date.fromisoformat(args.end_date)

    if args.use_rating_filter:
        rating_html = fetch_text(args.rating_url, args.user_agent, timeout=args.timeout)
        strong_players = parse_strong_players(rating_html, args.min_rating, args.include_equal)
        if not strong_players:
            print("no strong players found", file=sys.stderr)
            return 2
    else:
        strong_players = None

    out_lines = []
    total = 0
    kept = 0

    total_days = (end - start).days + 1
    day_index = 0

    if args.use_archive:
        years = range(start.year, end.year + 1)
        year_index = 0
        total_years = end.year - start.year + 1
        for year in years:
            year_index += 1
            if args.max_games and kept >= args.max_games:
                break
            archive_urls = list_yearly_archive_urls(year, args.archive_base_url, args.user_agent, args.timeout)
            print(
                f"[{year_index}/{total_years}] {year} archives={len(archive_urls)} total={total} kept={kept}",
                file=sys.stderr,
            )
            for url in archive_urls:
                if args.max_games and kept >= args.max_games:
                    break
                if args.sleep > 0:
                    time.sleep(args.sleep)

                name = urllib.parse.unquote(url.rsplit("/", 1)[-1])
                year_dir = os.path.join(args.archive_dir, str(year))
                archive_path = os.path.join(year_dir, name)
                extract_dir = os.path.join(year_dir, name.replace(".7z", ""))

                if not os.path.exists(archive_path):
                    ok = download_file(url, archive_path, args.user_agent, timeout=args.timeout)
                    if not ok:
                        continue

                if args.force_extract or not os.path.isdir(extract_dir):
                    ok = try_extract_7z(archive_path, extract_dir)
                    if not ok:
                        continue

                for root, _, files in os.walk(extract_dir):
                    for fn in files:
                        if not fn.lower().endswith(".csa"):
                            continue
                        if args.max_games and kept >= args.max_games:
                            break
                        file_date = parse_date_from_csa_filename(fn)
                        if file_date is None or file_date < start or file_date > end:
                            continue
                        file_path = os.path.join(root, fn)
                        try:
                            with open(file_path, "r", encoding="utf-8", errors="replace") as f:
                                csa_text = f.read()
                        except Exception:
                            continue

                        total += 1
                        if args.progress_every > 0 and total % args.progress_every == 0:
                            print(
                                f"progress total={total} kept={kept} last={fn}",
                                file=sys.stderr,
                            )

                        info = csa_to_usi_moves(csa_text, args.require_toryo)
                        if not info:
                            continue

                        black = info["black"]
                        white = info["white"]
                        if not black or not white:
                            black, white = parse_names_from_csa_filename(fn)

                        if not black or not white:
                            continue

                        if strong_players is not None:
                            if black not in strong_players or white not in strong_players:
                                continue

                        moves = info["moves"]
                        if not moves:
                            continue

                        out_lines.append("startpos moves " + " ".join(moves))
                        kept += 1

        if args.max_games and kept >= args.max_games:
            pass
        if kept > 0:
            with open(args.output, "w", encoding="ascii", newline="\n") as f:
                for line in out_lines:
                    f.write(line + "\n")
            print(f"total={total} kept={kept} output={args.output}")
            return 0

    for day in date_range(start, end):
        day_index += 1
        urls = list_csa_urls(day, args.csa_base_url, args.user_agent, args.timeout)
        print(
            f"[{day_index}/{total_days}] {day.isoformat()} urls={len(urls)} total={total} kept={kept}",
            file=sys.stderr,
        )
        for url in urls:
            if args.sleep > 0:
                time.sleep(args.sleep)
            total += 1
            if args.progress_every > 0 and total % args.progress_every == 0:
                print(
                    f"progress total={total} kept={kept} last={url.rsplit('/', 1)[-1]}",
                    file=sys.stderr,
                )
            if args.max_games and kept >= args.max_games:
                break

            try:
                csa_text = fetch_text(url, args.user_agent, timeout=args.timeout)
            except Exception:
                continue

            info = csa_to_usi_moves(csa_text, args.require_toryo)
            if not info:
                continue

            black = info["black"]
            white = info["white"]
            if not black or not white:
                black, white = parse_names_from_filename(url)

            if not black or not white:
                continue

            if strong_players is not None:
                if black not in strong_players or white not in strong_players:
                    continue

            moves = info["moves"]
            if not moves:
                continue

            out_lines.append("startpos moves " + " ".join(moves))
            kept += 1

        if args.max_games and kept >= args.max_games:
            break

    with open(args.output, "w", encoding="ascii", newline="\n") as f:
        for line in out_lines:
            f.write(line + "\n")

    print(f"total={total} kept={kept} output={args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
