from __future__ import annotations

import argparse
import json
import os
import sqlite3
import uuid
import pathlib
import sys
from typing import Optional, Sequence

try:
    from script.book_corpus import CorpusStore
    from script.book_extension_config import load_extension_config
    from script.book_extension_bundle import claim_bundle, create_bundle, extract_and_verify_bundle
    from script.corpus_collection import iter_csa_records
    from script.corpus_coverage import generate_coverage_report
    from script.corpus_ingest import ingest_csa_text
    from script.corpus_priority import PriorityFacts, POLICY_VERSION, encode_priority, priority_tuple
    from script.corpus_rankings import (
        FloodgateRatingResult, TournamentResult, import_floodgate_rating,
        import_tournament_ranking, recompute_candidate_priorities,
    )
    from script.extend_book_mcts import OpeningBook, sha256_file
except ImportError:
    from book_corpus import CorpusStore
    from book_extension_config import load_extension_config
    from book_extension_bundle import claim_bundle, create_bundle, extract_and_verify_bundle
    from corpus_collection import iter_csa_records
    from corpus_coverage import generate_coverage_report
    from corpus_ingest import ingest_csa_text
    from corpus_priority import PriorityFacts, POLICY_VERSION, encode_priority, priority_tuple
    from corpus_rankings import (
        FloodgateRatingResult, TournamentResult, import_floodgate_rating,
        import_tournament_ranking, recompute_candidate_priorities,
    )
    from extend_book_mcts import OpeningBook, sha256_file



def _ingest(args: argparse.Namespace) -> int:
    """Build a complete replacement DB, then atomically publish one corpus revision."""
    accepted = 0
    excluded = 0
    priority = encode_priority(priority_tuple(PriorityFacts(year=args.year)))
    args.db.parent.mkdir(parents=True, exist_ok=True)
    temporary_db = args.db.with_name(f"{args.db.name}.{uuid.uuid4().hex}.ingest.tmp")
    if args.db.exists():
        source = sqlite3.connect(args.db)
        target = sqlite3.connect(temporary_db)
        try:
            source.backup(target)
        finally:
            target.close()
            source.close()
    try:
        with CorpusStore(temporary_db) as store:
            for input_path in args.input:
                for relative_path, text in iter_csa_records(input_path):
                    source_path = f"{input_path.name}/{relative_path}"
                    result = ingest_csa_text(
                        store, text, site=args.site, event=args.event, year=args.year,
                        relative_path=source_path, priority_key=priority,
                        retrieved_at=args.retrieved_at,
                    )
                    if result.accepted:
                        accepted += 1
                    else:
                        excluded += 1
                        if excluded <= args.max_error_lines:
                            print(result.error, file=sys.stderr)
            if accepted:
                revision = store.bump_corpus_revision()
            else:
                revision = store.corpus_revision()
            store.connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        os.replace(temporary_db, args.db)
    except Exception:
        temporary_db.unlink(missing_ok=True)
        raise
    print(json.dumps({"accepted": accepted, "excluded": excluded, "corpus_revision": revision,
                      "priority_policy_version": POLICY_VERSION}, ensure_ascii=False))
    return 0

def _coverage(args: argparse.Namespace) -> int:
    book = OpeningBook.load(args.book, ignore_ply=args.ignore_ply)
    with CorpusStore(args.db) as store:
        report = generate_coverage_report(
            store, book, snapshot_id=args.snapshot_id, book_hash=sha256_file(args.book)
        )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return 0


def _bundle_create(args: argparse.Namespace) -> int:
    manifest = create_bundle(
        args.output, book=args.book, corpus_db=args.db, config=args.config,
        vulnerability_books=args.vulnerability_book,
    )
    print(json.dumps(manifest, ensure_ascii=False, sort_keys=True))
    return 0


def _ranking_import(args: argparse.Namespace) -> int:
    document = json.loads(args.json.read_text(encoding="utf-8"))
    results = [TournamentResult(**item) for item in document["results"]]
    with CorpusStore(args.db) as store:
        snapshot_id = import_tournament_ranking(
            store, site=document["site"], event=document["event"],
            source_url=document["source_url"], source_sha256=document["source_sha256"],
            retrieved_at=float(document["retrieved_at"]),
            provisional=bool(document.get("provisional", False)), results=results,
        )
        recompute_candidate_priorities(store)
    print(snapshot_id)
    return 0


def _rating_import(args: argparse.Namespace) -> int:
    document = json.loads(args.json.read_text(encoding="utf-8"))
    results = [FloodgateRatingResult(**item) for item in document["results"]]
    policy = load_extension_config(args.config).corpus if args.config is not None else None
    with CorpusStore(args.db) as store:
        snapshot_id = import_floodgate_rating(
            store, year=int(document["year"]), snapshot_time=float(document["snapshot_time"]),
            source_url=document["source_url"], source_sha256=document["source_sha256"],
            anchor_era=document["anchor_era"],
            rated_player_count=int(document["rated_player_count"]),
            component_size=int(document["component_size"]),
            anchor_connected_rate=float(document["anchor_connected_rate"]), results=results,
            medium_games=policy.rating_medium_games if policy else 15,
            high_games=policy.rating_high_games if policy else 50,
            min_component_size=policy.rating_min_component_size if policy else 10,
        )
        recompute_candidate_priorities(store)
    print(snapshot_id)
    return 0

def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Manage the portable book-extension corpus")
    commands = parser.add_subparsers(dest="command", required=True)

    ingest = commands.add_parser("ingest", help="ingest stopped snapshot inputs transactionally")
    ingest.add_argument("--db", type=pathlib.Path, required=True)
    ingest.add_argument("--site", choices=["floodgate", "wcsc", "denryu"], required=True)
    ingest.add_argument("--event", required=True)
    ingest.add_argument("--year", type=int, required=True)
    ingest.add_argument("--retrieved-at", type=float, required=True)
    ingest.add_argument("--input", type=pathlib.Path, action="append", required=True)
    ingest.add_argument("--max-error-lines", type=int, default=20)
    ingest.set_defaults(handler=_ingest)

    coverage = commands.add_parser("coverage")
    coverage.add_argument("--db", type=pathlib.Path, required=True)
    coverage.add_argument("--book", type=pathlib.Path, required=True)
    coverage.add_argument("--snapshot-id", required=True)
    coverage.add_argument("--output", type=pathlib.Path, required=True)
    coverage.add_argument("--ignore-ply", action="store_true")
    coverage.set_defaults(handler=_coverage)

    bundle = commands.add_parser("bundle-create")
    bundle.add_argument("--book", type=pathlib.Path, required=True)
    bundle.add_argument("--db", type=pathlib.Path, required=True)
    bundle.add_argument("--config", type=pathlib.Path, required=True)
    bundle.add_argument("--vulnerability-book", type=pathlib.Path, action="append", default=[])
    bundle.add_argument("--output", type=pathlib.Path, required=True)
    bundle.set_defaults(handler=_bundle_create)

    verify = commands.add_parser("bundle-verify")
    verify.add_argument("--bundle", type=pathlib.Path, required=True)
    verify.add_argument("--destination", type=pathlib.Path, required=True)
    verify.set_defaults(handler=lambda args: (print(json.dumps(extract_and_verify_bundle(args.bundle, args.destination))), 0)[1])

    claim = commands.add_parser("bundle-claim")
    claim.add_argument("--bundle-id", required=True)
    claim.add_argument("--registry", type=pathlib.Path, required=True)
    claim.add_argument("--machine-id", required=True)
    claim.set_defaults(handler=lambda args: (print(claim_bundle(args.bundle_id, args.registry, machine_id=args.machine_id)), 0)[1])
    ranking = commands.add_parser("ranking-import")
    ranking.add_argument("--db", type=pathlib.Path, required=True)
    ranking.add_argument("--json", type=pathlib.Path, required=True)
    ranking.set_defaults(handler=_ranking_import)

    rating = commands.add_parser("rating-import")
    rating.add_argument("--db", type=pathlib.Path, required=True)
    rating.add_argument("--json", type=pathlib.Path, required=True)
    rating.add_argument("--config", type=pathlib.Path)
    rating.set_defaults(handler=_rating_import)

    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    return int(args.handler(args))


if __name__ == "__main__":
    raise SystemExit(main())