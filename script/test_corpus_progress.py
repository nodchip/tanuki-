from __future__ import annotations

import io
import time
import unittest

from script.corpus_progress import CorpusProgressReporter


class FakeClock:
    def __init__(self) -> None:
        self.value = 100.0

    def __call__(self) -> float:
        return self.value

    def advance(self, seconds: float) -> None:
        self.value += seconds


class CorpusProgressReporterTest(unittest.TestCase):
    def test_phase_emits_start_and_done_with_elapsed_time(self) -> None:
        output = io.StringIO()
        clock = FakeClock()
        reporter = CorpusProgressReporter(
            output, clock=clock, heartbeat_interval=3600.0
        )

        with reporter.phase("coverage", items=3):
            clock.advance(2.0)

        self.assertEqual(
            output.getvalue().splitlines(),
            [
                "[corpus] phase=coverage event=start items=3",
                "[corpus] phase=coverage event=done elapsed=00:00:02",
            ],
        )

    def test_phase_emits_heartbeat_while_blocked(self) -> None:
        output = io.StringIO()
        reporter = CorpusProgressReporter(output, heartbeat_interval=0.01)

        with reporter.phase("validation"):
            deadline = time.monotonic() + 1.0
            while "event=heartbeat" not in output.getvalue():
                if time.monotonic() >= deadline:
                    self.fail("heartbeat was not emitted")
                time.sleep(0.005)

        lines = output.getvalue().splitlines()
        self.assertEqual(lines[0], "[corpus] phase=validation event=start")
        self.assertIn("[corpus] phase=validation event=heartbeat elapsed=00:00:00", lines)
        self.assertEqual(lines[-1], "[corpus] phase=validation event=done elapsed=00:00:00")

    def test_phase_emits_failed_and_stops_heartbeat_after_exception(self) -> None:
        output = io.StringIO()
        reporter = CorpusProgressReporter(output, heartbeat_interval=0.01)

        with self.assertRaisesRegex(ValueError, "boom"):
            with reporter.phase("metadata"):
                raise ValueError("boom")

        lines_before_wait = output.getvalue().splitlines()
        time.sleep(0.03)
        self.assertEqual(output.getvalue().splitlines(), lines_before_wait)
        self.assertEqual(lines_before_wait[-1], "[corpus] phase=metadata event=failed elapsed=00:00:00")

    def test_download_chunks_are_throttled_by_time_or_bytes(self) -> None:
        output = io.StringIO()
        clock = FakeClock()
        reporter = CorpusProgressReporter(
            output,
            clock=clock,
            heartbeat_interval=3600.0,
            download_interval=10.0,
            download_bytes_interval=64 * 1024**2,
        )
        common = {
            "source_index": 1,
            "source_count": 2,
            "file": "large file.7z",
            "file_total": 200 * 1024**2,
            "aggregate_total": 400 * 1024**2,
        }

        reporter.download_progress("source_start", **common)
        reporter.download_progress(
            "download_chunk", **common, file_bytes=1024**2, aggregate_bytes=1024**2
        )
        self.assertEqual(len(output.getvalue().splitlines()), 1)

        reporter.download_progress(
            "download_chunk",
            **common,
            file_bytes=64 * 1024**2,
            aggregate_bytes=64 * 1024**2,
        )
        clock.advance(10.0)
        reporter.download_progress(
            "download_chunk",
            **common,
            file_bytes=65 * 1024**2,
            aggregate_bytes=65 * 1024**2,
        )

        lines = output.getvalue().splitlines()
        self.assertEqual(len(lines), 3)
        self.assertIn("event=progress", lines[1])
        self.assertIn("file=large_file.7z", lines[1])
        self.assertIn(f"file_bytes={64 * 1024**2}", lines[1])
        self.assertIn(f"file_bytes={65 * 1024**2}", lines[2])

    def test_ingest_is_throttled_by_games_or_time(self) -> None:
        output = io.StringIO()
        clock = FakeClock()
        reporter = CorpusProgressReporter(
            output,
            clock=clock,
            heartbeat_interval=3600.0,
            ingest_interval=30.0,
            ingest_games_interval=1000,
        )
        common = {"site": "floodgate", "event_name": "floodgate-2026"}

        reporter.ingest_progress("source_start", **common)
        reporter.ingest_progress(
            "game", **common, games=999, accepted=998, excluded=1
        )
        self.assertEqual(len(output.getvalue().splitlines()), 1)

        reporter.ingest_progress(
            "game", **common, games=1000, accepted=999, excluded=1
        )
        clock.advance(30.0)
        reporter.ingest_progress(
            "game", **common, games=1001, accepted=1000, excluded=1
        )

        lines = output.getvalue().splitlines()
        self.assertEqual(len(lines), 3)
        self.assertIn("event=progress", lines[1])
        self.assertIn("games=1000", lines[1])
        self.assertIn("games=1001", lines[2])


if __name__ == "__main__":
    unittest.main()
