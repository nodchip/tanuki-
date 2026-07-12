from __future__ import annotations

import json
import os
import pathlib
import tempfile
import threading
import time
import unittest

from script.book_extension_runtime import (
    HeartbeatMonitor,
    drain_workers,
    LockUnavailableError,
    WindowsFileLock,
)


class BookExtensionRuntimeTest(unittest.TestCase):
    def test_drain_workers_stops_engines_and_waits_for_threads(self) -> None:
        stop_event = threading.Event()
        search_stopped = threading.Event()

        class FakeEngine:
            def __init__(self) -> None:
                self.stop_calls = 0

            def stop_search(self) -> None:
                self.stop_calls += 1
                search_stopped.set()

        engine = FakeEngine()
        worker = threading.Thread(target=search_stopped.wait)
        worker.start()

        drained = drain_workers(
            [engine],  # type: ignore[list-item]
            [worker],
            stop_event,
            timeout_sec=1.0,
        )

        self.assertTrue(drained)
        self.assertTrue(stop_event.is_set())
        self.assertEqual(engine.stop_calls, 1)
        self.assertFalse(worker.is_alive())

    def test_file_lock_rejects_second_owner_and_records_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            lock_path = pathlib.Path(temporary_directory) / "working.db.lock"
            first = WindowsFileLock(lock_path)
            second = WindowsFileLock(lock_path)
            first.acquire({"run_id": "first", "build_number": "42"})
            try:
                metadata = json.loads(lock_path.read_text(encoding="utf-8").strip())
                self.assertEqual(metadata["run_id"], "first")
                self.assertEqual(metadata["build_number"], "42")
                self.assertEqual(metadata["pid"], os.getpid())
                with self.assertRaises(LockUnavailableError):
                    second.acquire({"run_id": "second"})
            finally:
                first.close()

            second.acquire({"run_id": "second"})
            second.close()

    def test_file_lock_context_manager_releases_lock(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            lock_path = pathlib.Path(temporary_directory) / "working.db.lock"
            with WindowsFileLock(lock_path) as lock:
                lock.write_metadata({"run_id": "context"})
            with WindowsFileLock(lock_path):
                pass

    def test_heartbeat_expires_when_file_is_stale(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            heartbeat_path = pathlib.Path(temporary_directory) / "heartbeat.json"
            heartbeat_path.write_text("{}", encoding="utf-8")
            os.utime(heartbeat_path, (100.0, 100.0))
            monitor = HeartbeatMonitor(heartbeat_path, timeout_sec=10.0)

            self.assertFalse(monitor.expired(now=109.9))
            self.assertTrue(monitor.expired(now=110.1))

    def test_missing_heartbeat_is_not_expired_during_startup_grace(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            monitor = HeartbeatMonitor(
                pathlib.Path(temporary_directory) / "missing.json",
                timeout_sec=10.0,
                started_at=100.0,
            )

            self.assertFalse(monitor.expired(now=109.9))
            self.assertTrue(monitor.expired(now=110.1))

    def test_manual_stop_file_requests_shutdown_immediately(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            heartbeat_path = root / "heartbeat.json"
            stop_path = root / "graceful-stop.request"
            heartbeat_path.write_text("{}", encoding="utf-8")
            os.utime(heartbeat_path, (100.0, 100.0))
            monitor = HeartbeatMonitor(
                heartbeat_path,
                timeout_sec=10.0,
                stop_request_path=stop_path,
            )

            self.assertIsNone(monitor.stop_reason(now=101.0))
            stop_path.write_text("", encoding="utf-8")
            self.assertEqual(monitor.stop_reason(now=101.0), "manual-stop-request")


if __name__ == "__main__":
    unittest.main()
