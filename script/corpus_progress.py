from __future__ import annotations

import contextlib
import threading
import time
from collections.abc import Iterator
from typing import TextIO


class CorpusProgressReporter:
    def __init__(
        self,
        stream: TextIO,
        *,
        clock=time.monotonic,
        heartbeat_interval: float = 30.0,
        download_interval: float = 10.0,
        download_bytes_interval: int = 64 * 1024**2,
        ingest_interval: float = 30.0,
        ingest_games_interval: int = 1000,
    ) -> None:
        self.stream = stream
        self.clock = clock
        self.heartbeat_interval = heartbeat_interval
        self.download_interval = download_interval
        self.download_bytes_interval = download_bytes_interval
        self.ingest_interval = ingest_interval
        self.ingest_games_interval = ingest_games_interval
        self._write_lock = threading.Lock()
        self._download_last_time = self.clock()
        self._download_last_bytes = 0
        self._ingest_last_time = self.clock()
        self._ingest_last_games = 0

    @staticmethod
    def _format_elapsed(seconds: float) -> str:
        total = max(0, int(seconds))
        hours, remainder = divmod(total, 3600)
        minutes, seconds = divmod(remainder, 60)
        return f"{hours:02d}:{minutes:02d}:{seconds:02d}"

    @staticmethod
    def _format_value(value: object) -> str:
        return "".join("_" if character.isspace() else character for character in str(value))

    def _emit(self, phase: str, event: str, **fields: object) -> None:
        parts = ["[corpus]", f"phase={phase}", f"event={event}"]
        parts.extend(
            f"{name}={self._format_value(value)}" for name, value in fields.items()
        )
        with self._write_lock:
            print(" ".join(parts), file=self.stream, flush=True)

    @contextlib.contextmanager
    def phase(self, name: str, **fields: object) -> Iterator[None]:
        started = self.clock()
        stopped = threading.Event()
        self._emit(name, "start", **fields)

        def heartbeat() -> None:
            while not stopped.wait(self.heartbeat_interval):
                self._emit(
                    name,
                    "heartbeat",
                    elapsed=self._format_elapsed(self.clock() - started),
                )

        thread = threading.Thread(
            target=heartbeat,
            name=f"corpus-progress-{name}",
            daemon=True,
        )
        thread.start()
        try:
            yield
        except BaseException:
            stopped.set()
            thread.join()
            self._emit(
                name,
                "failed",
                elapsed=self._format_elapsed(self.clock() - started),
            )
            raise
        else:
            stopped.set()
            thread.join()
            self._emit(
                name,
                "done",
                elapsed=self._format_elapsed(self.clock() - started),
            )

    def download_progress(self, kind: str, **fields: object) -> None:
        now = self.clock()
        if kind == "source_start":
            self._download_last_time = now
            self._download_last_bytes = 0
            self._emit("download", kind, **fields)
            return
        if kind != "download_chunk":
            self._emit("download", kind, **fields)
            return

        current_bytes = int(fields["file_bytes"])
        if (
            now - self._download_last_time < self.download_interval
            and current_bytes - self._download_last_bytes < self.download_bytes_interval
        ):
            return
        self._download_last_time = now
        self._download_last_bytes = current_bytes
        self._emit("download", "progress", **fields)

    def ingest_progress(self, kind: str, **fields: object) -> None:
        now = self.clock()
        if kind == "source_start":
            self._ingest_last_time = now
            self._ingest_last_games = int(fields.get("games", 0))
            self._emit("ingest", kind, **fields)
            return
        if kind != "game":
            self._emit("ingest", kind, **fields)
            return

        games = int(fields["games"])
        if (
            now - self._ingest_last_time < self.ingest_interval
            and games - self._ingest_last_games < self.ingest_games_interval
        ):
            return
        self._ingest_last_time = now
        self._ingest_last_games = games
        self._emit("ingest", "progress", **fields)
