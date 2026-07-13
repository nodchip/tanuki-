from __future__ import annotations

import argparse
import csv
import http.client
import socket
import re
import subprocess
import time
import urllib.request
from pathlib import Path
from urllib.parse import urljoin
from urllib.error import HTTPError, URLError

from select_floodgate_policy_data import load_rating_snapshots, select_games, write_outputs


X_BASE_URL = "https://wdoor.c.u-tokyo.ac.jp/shogi/x/"
ARCHIVE_BASE_URL = "https://wdoor.c.u-tokyo.ac.jp/shogi/archive/"
RATING_INDEX_URL = urljoin(X_BASE_URL, "rating/")
HREF_PATTERN = re.compile(r'href="([^"]+)"', re.IGNORECASE)
RATING_LINK_PATTERN = re.compile(r"players-floodgate-(\d{8})\.yaml$", re.IGNORECASE)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Download Floodgate kifu for selected years, fetch long-term ratings, and emit 3300+ training data."
    )
    parser.add_argument("--work-dir", required=True, help="Working directory for downloaded kifu, ratings, and outputs.")
    parser.add_argument(
        "--years",
        default="2021,2022,2023,2024,2025",
        help="Comma-separated years to mirror from /shogi/x/. Default: 2021,2022,2023,2024,2025",
    )
    parser.add_argument("--min-rating", type=float, default=3300.0, help="Minimum long-term rating. Default: 3300.")
    parser.add_argument("--min-plies", type=int, default=0, help="Skip games shorter than this many plies.")
    parser.add_argument("--limit", type=int, default=0, help="Stop after selecting this many games. Default: 0.")
    parser.add_argument("--output", help="Output file for 'position startpos moves ...' lines.")
    parser.add_argument("--manifest", help="Optional CSV manifest for selected games.")
    parser.add_argument("--summary", help="Optional CSV summary by year.")
    parser.add_argument("--timeout", type=int, default=20, help="Per-request timeout seconds. Default: 20.")
    parser.add_argument("--sleep", type=float, default=0.0, help="Sleep seconds between downloads. Default: 0.")
    parser.add_argument("--skip-kifu-download", action="store_true", help="Reuse already-downloaded CSA files.")
    parser.add_argument("--skip-rating-download", action="store_true", help="Reuse already-downloaded rating files.")
    return parser.parse_args()


def format_progress_label(kind: str, current: int, total: int, context: str) -> str:
    return f"[{kind}] {current}/{total} {context}"


def archive_url_for_year(year: int) -> str:
    return urljoin(ARCHIVE_BASE_URL, f"wdoor{year}.7z")


def extracted_csa_root_for_year(work_dir: Path, year: int) -> Path:
    return work_dir / "archive" / str(year)


def is_retryable_download_error(error: Exception) -> bool:
    if isinstance(error, (TimeoutError, socket.timeout, ConnectionResetError, http.client.RemoteDisconnected, URLError)):
        return True
    if isinstance(error, HTTPError):
        return error.code >= 500
    return False


def extract_links(html_text: str) -> list[str]:
    return [match.group(1) for match in HREF_PATTERN.finditer(html_text)]


def classify_index_links(links: list[str]) -> tuple[list[str], list[str]]:
    directories: list[str] = []
    csa_files: list[str] = []
    for href in links:
        if href in {"../", "..", ".", ""}:
            continue
        if href.startswith("/") or "://" in href:
            continue
        if href.endswith(".csa"):
            csa_files.append(href)
            continue
        if "." in href or href.startswith("prating"):
            continue
        candidate = href.rstrip("/")
        if not candidate.isdigit():
            continue
        directories.append(candidate)
    return sorted(set(directories)), sorted(set(csa_files))


def fetch_text(url: str, timeout: int) -> str:
    with urllib.request.urlopen(url, timeout=timeout) as response:
        return response.read().decode("utf-8", errors="replace")


def download_file(url: str, target: Path, timeout: int, sleep_seconds: float, retries: int = 3) -> bool:
    if target.exists() and target.stat().st_size > 0:
        return False
    target.parent.mkdir(parents=True, exist_ok=True)
    last_error: Exception | None = None
    for attempt in range(1, retries + 1):
        try:
            with urllib.request.urlopen(url, timeout=timeout) as response:
                target.write_bytes(response.read())
            return True
        except Exception as error:
            last_error = error
            if not is_retryable_download_error(error) or attempt >= retries:
                break
            print(
                f"[retry] attempt={attempt}/{retries} url={url} error={type(error).__name__}: {error}",
                flush=True,
            )
            time.sleep(max(1.0, sleep_seconds))
    assert last_error is not None
    raise last_error


def ensure_archive_extracted(work_dir: Path, year: int, timeout: int, sleep_seconds: float) -> tuple[Path, bool, bool]:
    archive_dir = work_dir / "archives"
    archive_dir.mkdir(parents=True, exist_ok=True)
    archive_path = archive_dir / f"wdoor{year}.7z"
    archive_downloaded = download_file(archive_url_for_year(year), archive_path, timeout, sleep_seconds)
    extract_root = work_dir / "archive"
    csa_root = extracted_csa_root_for_year(work_dir, year)
    if csa_root.exists() and any(csa_root.rglob("*.csa")):
        return csa_root, archive_downloaded, False
    extract_root.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["7z", "x", "-y", str(archive_path), f"-o{extract_root}"],
        check=True,
    )
    return csa_root, archive_downloaded, True


def prepare_year_from_archive(work_dir: Path, year: int, timeout: int, sleep_seconds: float) -> tuple[Path, bool, bool, int]:
    print(format_progress_label("archive", 1, 1, str(year)), flush=True)
    csa_root, archive_downloaded, extracted = ensure_archive_extracted(work_dir, year, timeout, sleep_seconds)
    csa_count = sum(1 for _ in csa_root.rglob("*.csa"))
    print(
        f"[archive-ready] year={year} downloaded_archive={archive_downloaded} extracted={extracted} csa={csa_count}",
        flush=True,
    )
    return csa_root, archive_downloaded, extracted, csa_count


def list_rating_links(index_html: str, years: set[int]) -> list[str]:
    selected: list[str] = []
    seen: set[str] = set()
    for href in extract_links(index_html):
        match = RATING_LINK_PATTERN.fullmatch(href)
        if not match:
            continue
        year = int(match.group(1)[:4])
        if year not in years:
            continue
        if href in seen:
            continue
        seen.add(href)
        selected.append(href)
    return sorted(selected)


def download_ratings(rating_dir: Path, years: set[int], timeout: int, sleep_seconds: float) -> tuple[int, int]:
    index_html = fetch_text(RATING_INDEX_URL, timeout)
    links = list_rating_links(index_html, years)
    downloaded = 0
    for index, href in enumerate(links, 1):
        print(format_progress_label("ratings", index, len(links), href), flush=True)
        url = urljoin(RATING_INDEX_URL, href)
        target = rating_dir / href
        try:
            if download_file(url, target, timeout, sleep_seconds):
                downloaded += 1
                if sleep_seconds > 0:
                    time.sleep(sleep_seconds)
        except Exception as error:
            print(f"[rating-skip] file={href} error={type(error).__name__}: {error}", flush=True)
    return len(links), downloaded


def summarize_manifest(manifest_path: Path) -> tuple[int, int]:
    games = 0
    plies = 0
    with manifest_path.open("r", encoding="utf-8", newline="") as fh:
        for row in csv.DictReader(fh):
            games += 1
            plies += int(row["plies"])
    return games, plies


def write_year_summary(manifest_path: Path, summary_path: Path) -> None:
    counts: dict[str, dict[str, int]] = {}
    with manifest_path.open("r", encoding="utf-8", newline="") as fh:
        for row in csv.DictReader(fh):
            game_time = row["game_time"]
            year = game_time[:4] if game_time else "unknown"
            entry = counts.setdefault(year, {"games": 0, "plies": 0})
            entry["games"] += 1
            entry["plies"] += int(row["plies"])

    summary_path.parent.mkdir(parents=True, exist_ok=True)
    with summary_path.open("w", encoding="utf-8", newline="") as fh:
        writer = csv.writer(fh)
        writer.writerow(["year", "games", "plies", "avg_plies"])
        for year in sorted(counts):
            games = counts[year]["games"]
            plies = counts[year]["plies"]
            writer.writerow([year, games, plies, f"{(plies / games) if games else 0.0:.2f}"])


def main() -> int:
    args = parse_args()
    years = {int(year.strip()) for year in args.years.split(",") if year.strip()}
    work_dir = Path(args.work_dir)
    kifu_root = work_dir / "kifu"
    rating_dir = work_dir / "ratings"
    output_path = Path(args.output) if args.output else work_dir / f"floodgate_{int(args.min_rating)}plus_positions.txt"
    manifest_path = Path(args.manifest) if args.manifest else work_dir / f"floodgate_{int(args.min_rating)}plus_manifest.csv"
    summary_path = Path(args.summary) if args.summary else work_dir / f"floodgate_{int(args.min_rating)}plus_year_summary.csv"

    if not args.skip_kifu_download:
        for year in sorted(years):
            csa_root, archive_downloaded, extracted, csa_count = prepare_year_from_archive(work_dir, year, args.timeout, args.sleep)
            print(
                f"[kifu] year={year} archive_downloaded={archive_downloaded} extracted={extracted} csa={csa_count}",
                flush=True,
            )
    else:
        print("[kifu] skipped download", flush=True)

    if not args.skip_rating_download:
        available, downloaded = download_ratings(rating_dir, years, args.timeout, args.sleep)
        print(f"[ratings] available={available} newly_downloaded={downloaded}", flush=True)
    else:
        print("[ratings] skipped download", flush=True)

    snapshots = load_rating_snapshots(rating_dir)
    if not snapshots:
        raise SystemExit("No usable long-term ratings found.")

    csa_root = work_dir / "archive"
    selected = select_games(
        csa_dir=csa_root,
        snapshots=snapshots,
        min_rating=args.min_rating,
        min_plies=args.min_plies,
        allow_nonstandard_result=False,
        limit=args.limit,
    )
    write_outputs(selected, output_path, manifest_path)
    write_year_summary(manifest_path, summary_path)
    games, plies = summarize_manifest(manifest_path)
    avg_plies = (plies / games) if games else 0.0
    print(f"[selected] games={games} plies={plies} avg_plies={avg_plies:.2f}", flush=True)
    print(f"[selected] output={output_path}", flush=True)
    print(f"[selected] manifest={manifest_path}", flush=True)
    print(f"[selected] summary={summary_path}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
