from __future__ import annotations

import argparse
import pathlib
import subprocess
from typing import Optional, Sequence

try:
    from script.book_extension_config import load_extension_config
except ImportError:
    from book_extension_config import load_extension_config


def build_extension_argv(
    *,
    runtime_exe: pathlib.Path,
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
    random_seed: Optional[int] = None,
    usi_search_timeout_sec: Optional[float] = None,
) -> list[str]:
    config = load_extension_config(config_path)
    state = config.runtime.state_dir
    state.mkdir(parents=True, exist_ok=True)
    argv = [
        str(runtime_exe), "--config", str(config_path),
        "--input", str(input_book), "--output", str(output_book),
        "--engine", str(engine),
        "--nodes", str(nodes), "--multipv", str(multipv),
        "--stop-request-path", str(state / "stop.request"),
        "--lock-path", str(state / "book-extension.lock"),
        "--black-target", str(black_target),
        "--white-target", str(white_target),
    ]
    if heartbeat_path is not None:
        argv.extend(("--heartbeat-path", str(heartbeat_path)))
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
    if random_seed is not None:
        argv.extend(("--random-seed", str(random_seed)))
    if usi_search_timeout_sec is not None:
        argv.extend(("--usi-search-timeout-sec", str(usi_search_timeout_sec)))
    if config.corpus.enabled:
        if corpus_db is None:
            raise ValueError("enabled corpus requires --corpus-db")
        argv.extend(("--corpus-db", str(corpus_db)))
    return argv


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime-exe", type=pathlib.Path, required=True)
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
    parser.add_argument("--random-seed", type=int)
    parser.add_argument("--usi-search-timeout-sec", type=float)
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = build_parser().parse_args(argv)
    extension_argv = build_extension_argv(
        runtime_exe=args.runtime_exe,
        config_path=args.config, input_book=args.input, output_book=args.output,
        engine=args.engine, nodes=args.nodes, multipv=args.multipv,
        black_target=args.black_target, white_target=args.white_target,
        corpus_db=args.corpus_db,
        heartbeat_path=args.heartbeat_path, stop_request_path=args.stop_request_path,
        lock_path=args.lock_path, max_runtime_sec=args.max_runtime_sec,
        ignore_ply=args.ignore_ply, random_seed=args.random_seed,
        usi_search_timeout_sec=args.usi_search_timeout_sec,
    )
    return subprocess.run(extension_argv, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
