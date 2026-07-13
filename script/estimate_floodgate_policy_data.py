from __future__ import annotations

import argparse
import csv
import re
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

from select_floodgate_policy_data import load_rating_snapshots, select_games, write_outputs


ARCHIVE_URL = "http://wdoor.c.u-tokyo.ac.jp/shogi/x/wdoor2026.7z"
RATING_BASE_URL = "http://wdoor.c.u-tokyo.ac.jp/shogi/logs/LATEST/rating/"
DATE_PATTERN = re.compile(r"(\d{8})\d{6}\.csa$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Download Floodgate ratings, extract archive, and estimate 3300+ policy training data volume."
    )
    parser.add_argument("--archive", required=True, help="Path to local wdoor2026.7z archive.")
    parser.add_argument("--work-dir", required=True, help="Working directory for extracted CSA and ratings.")
    parser.add_argument("--min-rating", type=float, default=3300.0, help="Minimum long-term rating. Default: 3300.")
    parser.add_argument("--min-plies", type=int, default=0, help="Skip games shorter than this many plies.")
    parser.add_argument("--limit", type=int, default=0, help="Stop after selecting this many games. Default: 0.")
    parser.add_argument("--output", help="Optional output file for selected 'position startpos moves ...' lines.")
    parser.add_argument("--manifest", help="Optional CSV manifest for selected games.")
    parser.add_argument("--skip-extract", action="store_true", help="Skip 7z extraction if archive is already extracted.")
    parser.add_argument("--skip-rating-download", action="store_true", help="Skip downloading missing rating YAML files.")
    parser.add_argument("--retries", type=int, default=3, help="Retries for each rating YAML download.")
    parser.add_argument("--timeout", type=int, default=20, help="Per-request timeout seconds for rating YAML download.")
    return parser.parse_args()


def extract_archive(archive_path: Path, work_dir: Path) -> Path:
    extract_dir = work_dir / "archive"
    extract_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["7z", "x", "-y", str(archive_path), f"-o{extract_dir}"],
        check=True,
    )
    return extract_dir / "2026"


def collect_game_dates(csa_root: Path) -> list[str]:
    dates = set()
    for csa_path in csa_root.rglob("*.csa"):
        match = DATE_PATTERN.search(csa_path.name)
        if match:
            dates.add(match.group(1))
    return sorted(dates)


def download_required_ratings(dates: list[str], ratings_dir: Path, retries: int, timeout: int) -> tuple[int, list[str]]:
    ratings_dir.mkdir(parents=True, exist_ok=True)
    downloaded = 0
    failed: list[str] = []

    for index, date_text in enumerate(dates, 1):
        name = f"players-floodgate-{date_text}.yaml"
        target = ratings_dir / name
        if target.exists() and target.stat().st_size > 0:
            downloaded += 1
            continue

        url = RATING_BASE_URL + name
        success = False
        for attempt in range(retries):
            try:
                with urllib.request.urlopen(url, timeout=timeout) as response:
                    target.write_bytes(response.read())
                downloaded += 1
                success = True
                break
            except Exception:
                time.sleep(1 + attempt)

        if not success:
            failed.append(name)

        if index % 10 == 0:
            print(f"[ratings] {index}/{len(dates)} processed, ready={downloaded}, failed={len(failed)}", flush=True)

    return downloaded, failed


def summarize_manifest(manifest_path: Path) -> dict[str, float]:
    games = 0
    plies = 0
    with manifest_path.open("r", encoding="utf-8", newline="") as fh:
        for row in csv.DictReader(fh):
            games += 1
            plies += int(row["plies"])
    return {
        "games": games,
        "plies": plies,
        "avg_plies": (plies / games) if games else 0.0,
    }


def main() -> int:
    args = parse_args()
    archive_path = Path(args.archive)
    work_dir = Path(args.work_dir)
    ratings_dir = work_dir / "ratings"
    manifest_path = Path(args.manifest) if args.manifest else work_dir / f"selected_{int(args.min_rating)}plus_manifest.csv"
    output_path = Path(args.output) if args.output else work_dir / f"selected_{int(args.min_rating)}plus.txt"

    if args.skip_extract:
        csa_root = work_dir / "archive" / "2026"
    else:
        csa_root = extract_archive(archive_path, work_dir)

    if not csa_root.exists():
        print(f"CSA root not found: {csa_root}", file=sys.stderr)
        return 1

    all_csa_games = sum(1 for _ in csa_root.rglob("*.csa"))
    dates = collect_game_dates(csa_root)
    print(f"[archive] total_csa_games={all_csa_games} unique_dates={len(dates)} date_range={dates[0]}..{dates[-1]}", flush=True)

    failed: list[str] = []
    if not args.skip_rating_download:
        downloaded, failed = download_required_ratings(dates, ratings_dir, args.retries, args.timeout)
        print(f"[ratings] available={downloaded} failed={len(failed)}", flush=True)
    else:
        print(f"[ratings] skipped download, existing_files={sum(1 for _ in ratings_dir.glob('players-floodgate-*.yaml'))}", flush=True)

    snapshots = load_rating_snapshots(ratings_dir)
    if not snapshots:
        print("No usable long-term rating snapshots were found.", file=sys.stderr)
        return 1

    selected = select_games(
        csa_dir=csa_root,
        snapshots=snapshots,
        min_rating=args.min_rating,
        min_plies=args.min_plies,
        allow_nonstandard_result=False,
        limit=args.limit,
    )
    write_outputs(selected, output_path, manifest_path)
    summary = summarize_manifest(manifest_path)

    print(f"[selected] games={summary['games']} plies={summary['plies']} avg_plies={summary['avg_plies']:.2f}", flush=True)
    print(f"[selected] output={output_path}", flush=True)
    print(f"[selected] manifest={manifest_path}", flush=True)
    if failed:
        print("[ratings] failed files:", file=sys.stderr)
        for name in failed:
            print(name, file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
