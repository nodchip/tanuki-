from __future__ import annotations

import json
import os
import pathlib
import time
import threading
from typing import Any, BinaryIO, Mapping, Optional, Sequence


class LockUnavailableError(RuntimeError):
    """Raised when another extension process owns the book lock."""


class WindowsFileLock:
    """A cooperative one-byte file lock that is released when the handle closes."""

    _LOCK_OFFSET = 2_147_483_647

    def __init__(self, path: pathlib.Path) -> None:
        self.path = pathlib.Path(path)
        self._stream: Optional[BinaryIO] = None

    def acquire(self, metadata: Optional[Mapping[str, Any]] = None) -> None:
        if self._stream is not None:
            raise RuntimeError("lock is already acquired")
        self.path.parent.mkdir(parents=True, exist_ok=True)
        stream = self.path.open("a+b")
        try:
            # Lock beyond EOF so diagnostics at the start remain readable.
            stream.seek(self._LOCK_OFFSET)
            self._lock_nonblocking(stream)
        except OSError as error:
            stream.close()
            raise LockUnavailableError(f"book lock is already held: {self.path}") from error
        self._stream = stream
        self.write_metadata({} if metadata is None else metadata)

    @staticmethod
    def _lock_nonblocking(stream: BinaryIO) -> None:
        if os.name == "nt":
            import msvcrt

            msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
            return
        import fcntl  # pragma: no cover - Windows is the production platform.

        fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

    @staticmethod
    def _unlock(stream: BinaryIO) -> None:
        if os.name == "nt":
            import msvcrt

            stream.seek(WindowsFileLock._LOCK_OFFSET)
            msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
            return
        import fcntl  # pragma: no cover - Windows is the production platform.

        fcntl.flock(stream.fileno(), fcntl.LOCK_UN)

    def write_metadata(self, metadata: Mapping[str, Any]) -> None:
        if self._stream is None:
            raise RuntimeError("lock is not acquired")
        document = dict(metadata)
        document.setdefault("pid", os.getpid())
        document.setdefault("acquired_at", time.time())
        payload = (json.dumps(document, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")
        self._stream.seek(0)
        self._stream.truncate()
        self._stream.write(payload)
        self._stream.flush()
        os.fsync(self._stream.fileno())
        self._stream.seek(0)

    def close(self) -> None:
        if self._stream is None:
            return
        stream = self._stream
        self._stream = None
        try:
            self._unlock(stream)
        finally:
            stream.close()

    def __enter__(self) -> "WindowsFileLock":
        self.acquire()
        return self

    def __exit__(self, exc_type: object, exc_value: object, traceback: object) -> None:
        self.close()


def drain_workers(
    engines: Sequence[Any],
    threads: Sequence[threading.Thread],
    stop_event: threading.Event,
    *,
    timeout_sec: float,
) -> bool:
    """Request USI stop and wait up to timeout_sec for every worker."""
    stop_event.set()
    for engine in engines:
        try:
            engine.stop_search()
        except (OSError, RuntimeError):
            pass
    deadline = time.monotonic() + max(0.0, timeout_sec)
    for thread in threads:
        remaining = max(0.0, deadline - time.monotonic())
        thread.join(timeout=remaining)
    return all(not thread.is_alive() for thread in threads)


class HeartbeatMonitor:
    """Detect Jenkins wrapper loss or an explicit graceful-stop request."""

    def __init__(
        self,
        path: pathlib.Path,
        *,
        timeout_sec: float,
        started_at: Optional[float] = None,
        stop_request_path: Optional[pathlib.Path] = None,
    ) -> None:
        if timeout_sec <= 0:
            raise ValueError("timeout_sec must be positive")
        self.path = pathlib.Path(path)
        self.timeout_sec = timeout_sec
        self.started_at = time.time() if started_at is None else started_at
        self.stop_request_path = None if stop_request_path is None else pathlib.Path(stop_request_path)

    def expired(self, *, now: Optional[float] = None) -> bool:
        current_time = time.time() if now is None else now
        try:
            last_seen = self.path.stat().st_mtime
        except FileNotFoundError:
            last_seen = self.started_at
        return current_time - last_seen > self.timeout_sec

    def stop_reason(self, *, now: Optional[float] = None) -> Optional[str]:
        if self.stop_request_path is not None and self.stop_request_path.exists():
            return "manual-stop-request"
        if self.expired(now=now):
            return "jenkins-heartbeat-expired"
        return None
