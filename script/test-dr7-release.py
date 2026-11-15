"""Smoke-test selected Windows builds on a CPU that supports them."""

import argparse
import queue
import re
import subprocess
import threading
import time
from pathlib import Path


def check_engine(release, target):
    executable = release / f"tanuki-dr7_prd-{target.lower()}.exe"
    process = subprocess.Popen(
        [str(executable)], cwd=release, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace", bufsize=1,
    )
    lines = queue.Queue()

    def reader():
        for line in process.stdout:
            lines.put(line.rstrip())
        lines.put(None)

    threading.Thread(target=reader, daemon=True).start()

    def send(command):
        process.stdin.write(command + "\n")
        process.stdin.flush()

    def wait_for(prefix, timeout=60):
        output = []
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError((target, prefix, output[-10:]))
            try:
                line = lines.get(timeout=remaining)
            except queue.Empty:
                raise TimeoutError((target, prefix, output[-10:])) from None
            if line is None:
                raise RuntimeError((target, "engine exited", process.poll(), output[-10:]))
            output.append(line)
            if line.startswith(prefix):
                return output

    try:
        send("usi")
        output = wait_for("usiok")
        assert any(re.fullmatch(r"id name tanuki-dr7_prd .+ 9\.80git 64\S+ TOURNAMENT", line) for line in output), output
        assert "id author yanurao, nodchip" in output, output
        assert "option name FV_SCALE type spin default 24 min 1 max 128" in output, output
        send("setoption name FV_SCALE value 16")
        send("getoption FV_SCALE")
        output = wait_for("Options[FV_SCALE]")
        assert re.search(r"=\s*16$", output[-1]), output
        send("setoption name FV_SCALE value 24")
        for command in (
            "setoption name Threads value 1", "setoption name USI_Hash value 64",
            "setoption name BookFile value no_book", "setoption name PvInterval value 0",
            "isready",
        ):
            send(command)
        output = wait_for("readyok")
        assert not any("Error!" in line or "Warning:" in line for line in output), output
        assert any("loading eval file" in line for line in output), output
        assert any("loading progress file" in line for line in output), output
        send("position startpos")
        send("go depth 10")
        output = wait_for("bestmove")
        info = [line for line in output if line.startswith("info depth 10 ")]
        assert info and output[-1].split()[1] not in ("none", "resign"), output
        final_info = re.sub(r" (?:nps|time|hashfull) \d+", "", info[-1])
        bestmove = output[-1]
        send("quit")
        process.wait(timeout=10)
        assert process.returncode == 0, process.returncode
        return final_info, bestmove
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("release_dir", type=Path)
    parser.add_argument("targets", nargs="+", help="Only targets supported by this CPU and OS")
    args = parser.parse_args()
    release = args.release_dir.resolve()
    reference = None
    for target in args.targets:
        result = check_engine(release, target)
        if reference is None:
            reference = result
        assert result == reference, (target, reference, result)
        print(f"PASS {target}: {result[1]}", flush=True)


if __name__ == "__main__":
    main()
