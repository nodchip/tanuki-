from __future__ import annotations

import argparse
import pathlib
from typing import Optional, Sequence

try:
    from script.book_extension_config import load_extension_config
    from script.extend_book_mcts import main as extend_main
except ImportError:
    from book_extension_config import load_extension_config
    from extend_book_mcts import main as extend_main


def build_extension_argv(
    *,
    config_path: pathlib.Path,
    input_book: pathlib.Path,
    output_book: pathlib.Path,
    engine: pathlib.Path,
    nodes: int,
    multipv: int,
    black_target: pathlib.Path,
    white_target: pathlib.Path,
    corpus_db: Optional[pathlib.Path],
    heartbeat_path: Optional[pathlib.Path] = None,
    stop_request_path: Optional[pathlib.Path] = None,
    lock_path: Optional[pathlib.Path] = None,
    max_runtime_sec: Optional[float] = None,
    ignore_ply: bool = False,
) -> list[str]:
    config = load_extension_config(config_path)
    state = config.runtime.state_dir
    state.mkdir(parents=True, exist_ok=True)
    argv = [
        "--input", str(input_book), "--output", str(output_book),
        "--engine", str(engine), "--engine-count", str(config.workers.engine_count),
        "--threads", str(config.workers.threads_per_engine),
        "--nodes", str(nodes), "--multipv", str(multipv),
        "--save-interval-sec", str(config.runtime.save_interval_sec),
        "--backup-count", str(config.runtime.backup_count),
        "--heartbeat-timeout-sec", str(config.runtime.heartbeat_timeout_sec),
        "--usi-stop-timeout-sec", str(config.runtime.usi_stop_timeout_sec),
        "--heartbeat-path", str(state / "jenkins.heartbeat"),
        "--stop-request-path", str(state / "stop.request"),
        "--lock-path", str(state / "book-extension.lock"),
    ]
    if heartbeat_path is not None:
        index = argv.index("--heartbeat-path")
        argv[index + 1] = str(heartbeat_path)
    if stop_request_path is not None:
        index = argv.index("--stop-request-path")
        argv[index + 1] = str(stop_request_path)
    if lock_path is not None:
        index = argv.index("--lock-path")
        argv[index + 1] = str(lock_path)
    if max_runtime_sec is not None:
        argv.extend(("--max-runtime-sec", str(max_runtime_sec)))
    if ignore_ply:
        argv.append("--ignore-ply")
    for _ in range(config.workers.vulnerability_black):
        argv.extend(("--vulnerability-target", f"{black_target}:black"))
    for _ in range(config.workers.vulnerability_white):
        argv.extend(("--vulnerability-target", f"{white_target}:white"))
    if config.corpus.enabled:
        if corpus_db is None:
            raise ValueError("enabled corpus requires --corpus-db")
        share = config.corpus.general_pool_node_share
        corpus_nodes = max(1, round(nodes * share / (1.0 - share))) if share < 1.0 else nodes
        argv.extend((
            "--corpus-db", str(corpus_db),
            "--corpus-nodes", str(corpus_nodes),
            "--corpus-max-concurrent", str(config.corpus.max_concurrent_searches),
            "--book-snapshot-id", output_book.name,
            "--corpus-saturation-window", str(config.corpus.saturation_window),
            "--site-weight-wcsc", str(config.corpus.wcsc_weight),
            "--site-weight-denryu", str(config.corpus.denryu_weight),
            "--site-weight-floodgate", str(config.corpus.floodgate_weight),
        ))
    return argv


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=pathlib.Path, required=True)
    parser.add_argument("--input", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--engine", type=pathlib.Path, required=True)
    parser.add_argument("--nodes", type=int, required=True)
    parser.add_argument("--multipv", type=int, required=True)
    parser.add_argument("--black-target", type=pathlib.Path, required=True)
    parser.add_argument("--white-target", type=pathlib.Path, required=True)
    parser.add_argument("--corpus-db", type=pathlib.Path)
    parser.add_argument("--heartbeat-path", type=pathlib.Path)
    parser.add_argument("--stop-request-path", type=pathlib.Path)
    parser.add_argument("--lock-path", type=pathlib.Path)
    parser.add_argument("--max-runtime-sec", type=float)
    parser.add_argument("--ignore-ply", action="store_true")
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    extension_argv = build_extension_argv(
        config_path=args.config, input_book=args.input, output_book=args.output,
        engine=args.engine, nodes=args.nodes, multipv=args.multipv,
        black_target=args.black_target, white_target=args.white_target,
        corpus_db=args.corpus_db,
        heartbeat_path=args.heartbeat_path, stop_request_path=args.stop_request_path,
        lock_path=args.lock_path, max_runtime_sec=args.max_runtime_sec,
        ignore_ply=args.ignore_ply,
    )
    return extend_main(extension_argv)


if __name__ == "__main__":
    raise SystemExit(main())