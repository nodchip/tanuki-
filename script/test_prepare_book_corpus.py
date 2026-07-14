from __future__ import annotations

import hashlib
import json
import pathlib
import tempfile
import unittest
from unittest import mock

from script.book_corpus import CorpusStore
from script.book_extension_runtime import WindowsFileLock
from script.prepare_book_corpus import CorpusBuildError, build_corpus, main


CSA_TEMPLATE = """V2.2
N+{black}
N-{white}
PI
+
+7776FU
-3334FU
%TORYO
"""


class PrepareBookCorpusTest(unittest.TestCase):
    def create_fixture(self, root: pathlib.Path, name: str = "pilot") -> pathlib.Path:
        source = root / "source"
        source.mkdir()
        specifications = [
            ("floodgate", "fg-2026", 2026, "Alpha", "Beta"),
            ("wcsc", "wcsc36", 2026, "Gamma", "Delta"),
            ("denryu", "denryu6", 2025, "Epsilon", "Zeta"),
        ]
        sources = []
        ingests = []
        for site, event, year, black, white in specifications:
            path = source / f"{site}.csa"
            path.write_text(CSA_TEMPLATE.format(black=black, white=white), encoding="utf-8")
            payload = path.read_bytes()
            relative = f"raw/{site}.csa"
            sources.append(
                {
                    "site": site,
                    "event": event,
                    "year": year,
                    "retrieved_at": 1783785600,
                    "url": path.resolve().as_uri(),
                    "relative_path": relative,
                    "size": len(payload),
                    "sha256": hashlib.sha256(payload).hexdigest(),
                }
            )
            ingests.append(
                {
                    "site": site,
                    "event": event,
                    "year": year,
                    "retrieved_at": 1783785600,
                    "inputs": [relative],
                }
            )
        (root / "manifest.json").write_text(
            json.dumps({"sources": sources}), encoding="utf-8"
        )
        ranking = {
            "site": "wcsc",
            "event": "wcsc36",
            "source_url": "fixture://ranking",
            "source_sha256": "11" * 32,
            "retrieved_at": 1783785600,
            "provisional": False,
            "results": [
                {
                    "official_name": "Gamma",
                    "stage": "final",
                    "stage_tier": 3,
                    "rank": 1,
                    "participants": 2,
                }
            ],
        }
        (root / "ranking.json").write_text(json.dumps(ranking), encoding="utf-8")
        rating = {
            "year": 2026,
            "snapshot_time": 1783785600,
            "source_url": "fixture://rating",
            "source_sha256": "22" * 32,
            "anchor_era": "2024-2026",
            "rated_player_count": 2,
            "component_size": 2,
            "anchor_connected_rate": 1.0,
            "results": [
                {
                    "player_name": "Alpha",
                    "rating": 4000.0,
                    "effective_games": 50.0,
                    "connected_to_anchor": True,
                    "anchor_margin": 0.0,
                    "snapshot_percentile": 1.0,
                }
            ],
        }
        (root / "rating.json").write_text(json.dumps(rating), encoding="utf-8")
        (root / "extension.toml").write_text(
            """[workers]
engine_count=1
threads_per_engine=1
vulnerability_black=0
vulnerability_white=0
general=1
[corpus]
enabled=true
max_concurrent_searches=1
general_pool_node_share=0.25
rating_medium_games=15
rating_high_games=50
rating_min_component_size=1
[runtime]
state_dir="state"
save_interval_sec=3600
backup_count=3
heartbeat_timeout_sec=10
usi_stop_timeout_sec=5
""",
            encoding="utf-8",
        )
        (root / "book.db").write_text("#YANEURAOU-DB2016 1.00\n", encoding="utf-8")
        profile = {
            "schema_version": 1,
            "name": name,
            "manifest": "manifest.json",
            "ingests": ingests,
            "ranking_files": ["ranking.json"],
            "rating_file": "rating.json",
            "rating_config": "extension.toml",
            "input_book": "book.db",
            "coverage_snapshot_id": f"{name}-initial",
        }
        profile_path = root / f"{name}.json"
        profile_path.write_text(json.dumps(profile), encoding="utf-8")
        return profile_path

    def test_coverage_matches_book_positions_while_ignoring_sfen_ply(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            (root / "book.db").write_text(
                "#YANEURAOU-DB2016 1.00\n"
                "sfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 0\n"
                "7g7f none 10 1 0\n",
                encoding="utf-8",
            )
            state = root / "state"

            build_corpus(profile, state, now=1783785700)

            coverage = json.loads(
                (state / "coverage-initial.json").read_text(encoding="utf-8")
            )
            self.assertGreater(coverage["overall"]["unique"]["covered"], 0)
    def test_uses_state_input_book_and_allows_optional_rating(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            document = json.loads(profile.read_text(encoding="utf-8"))
            document["rating_file"] = None
            document["rating_config"] = None
            document["input_book"] = None
            profile.write_text(json.dumps(document), encoding="utf-8")
            state = root / "state"
            state.mkdir()
            (root / "book.db").replace(state / "input-book.db")

            summary = build_corpus(profile, state, now=1783785700)

            self.assertEqual(summary.status, "success")
            with CorpusStore(state / "corpus.sqlite") as store:
                self.assertEqual(
                    store.connection.execute(
                        "SELECT COUNT(*) FROM rating_snapshot"
                    ).fetchone()[0],
                    0,
                )
    def test_one_call_builds_three_sites_metadata_and_coverage(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"

            summary = build_corpus(profile, state, now=1783785700)

            self.assertEqual(summary.status, "success")
            for name in (
                "corpus.sqlite",
                "snapshot.json",
                "coverage-initial.json",
                "corpus-build-summary.json",
            ):
                self.assertTrue((state / name).is_file(), name)
            with CorpusStore(state / "corpus.sqlite") as store:
                self.assertEqual(store.table_count("logical_game"), 3)
                self.assertEqual(store.connection.execute("SELECT COUNT(*) FROM ranking_snapshot").fetchone()[0], 1)
                self.assertEqual(store.connection.execute("SELECT COUNT(*) FROM rating_snapshot").fetchone()[0], 1)
                self.assertEqual(store.table_count("candidate"), 2)
            coverage = json.loads((state / "coverage-initial.json").read_text(encoding="utf-8"))
            self.assertEqual(coverage["snapshot_id"], "pilot-initial")
            self.assertEqual(coverage["overall"]["occurrences"]["total"], 6)
            summary_document = json.loads(
                (state / "corpus-build-summary.json").read_text(encoding="utf-8")
            )
            self.assertEqual(summary_document["coverage"]["occurrences"]["total"], 6)
            self.assertEqual(
                set(summary_document["phase_seconds"]),
                {"download", "ingest", "metadata", "coverage", "validation", "publish"},
            )
            self.assertGreaterEqual(summary_document["finished_at"], summary_document["started_at"])

    def test_metadata_failure_preserves_active_database(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            (root / "ranking.json").write_text("not-json", encoding="utf-8")
            state = root / "state"
            state.mkdir()
            active = state / "corpus.sqlite"
            active.write_bytes(b"active-database")

            with self.assertRaises(CorpusBuildError) as raised:
                build_corpus(profile, state, now=1783785700)

            self.assertEqual(active.read_bytes(), b"active-database")
            self.assertEqual(raised.exception.phase, "ranking")
            self.assertEqual(raised.exception.exit_code, 4)

    def test_success_keeps_one_previous_database_generation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"
            state.mkdir()
            (state / "corpus.sqlite").write_bytes(b"previous-database")

            build_corpus(profile, state, now=1783785700)

            self.assertEqual(
                (state / "corpus.sqlite.previous").read_bytes(), b"previous-database"
            )
            with CorpusStore(state / "corpus.sqlite") as store:
                self.assertEqual(store.schema_version(), 5)

    def test_active_book_extension_lock_rejects_build_without_changes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"
            state.mkdir()
            active = state / "corpus.sqlite"
            active.write_bytes(b"active-database")
            lock = WindowsFileLock(state / "book-extension.lock")
            lock.acquire({"owner": "test"})
            try:
                with self.assertRaises(CorpusBuildError) as raised:
                    build_corpus(profile, state, now=1783785700)

                self.assertEqual(raised.exception.exit_code, 6)
                self.assertEqual(active.read_bytes(), b"active-database")
            finally:
                lock.close()

    def test_database_validation_failure_preserves_active_database(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"
            state.mkdir()
            active = state / "corpus.sqlite"
            active.write_bytes(b"active-database")

            with mock.patch(
                "script.prepare_book_corpus._validate_database",
                side_effect=ValueError("integrity failure"),
            ):
                with self.assertRaises(CorpusBuildError) as raised:
                    build_corpus(profile, state, now=1783785700)

            self.assertEqual(raised.exception.phase, "database-validation")
            self.assertEqual(raised.exception.exit_code, 5)
            self.assertEqual(active.read_bytes(), b"active-database")

    def test_concurrent_corpus_build_lock_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"
            state.mkdir()
            lock = WindowsFileLock(state / "corpus-build.lock")
            lock.acquire({"owner": "test"})
            try:
                with self.assertRaises(CorpusBuildError) as raised:
                    build_corpus(profile, state, now=1783785700)
                self.assertEqual(raised.exception.exit_code, 6)
            finally:
                lock.close()

    def test_verified_download_is_reused_when_source_disappears(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"
            build_corpus(profile, state, now=1783785700)
            for path in (root / "source").iterdir():
                path.unlink()

            second = build_corpus(profile, state, now=1783785800)

            self.assertEqual(second.status, "success")

    def test_download_hash_mismatch_is_exit_code_three_and_preserves_active(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"
            target = state / "downloads" / "pilot" / "raw" / "floodgate.csa"
            target.parent.mkdir(parents=True)
            target.write_bytes(b"wrong")
            active = state / "corpus.sqlite"
            active.write_bytes(b"active")

            with self.assertRaises(CorpusBuildError) as raised:
                build_corpus(profile, state, now=1783785700)

            self.assertEqual(raised.exception.exit_code, 3)
            self.assertEqual(active.read_bytes(), b"active")

    def test_broken_game_is_excluded_and_reported_without_failing_build(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            source = root / "source" / "denryu.csa"
            source.write_text(
                source.read_text(encoding="utf-8").replace("-3334FU", "-9998FU"),
                encoding="utf-8",
            )
            manifest_path = root / "manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            entry = next(item for item in manifest["sources"] if item["site"] == "denryu")
            payload = source.read_bytes()
            entry["size"] = len(payload)
            entry["sha256"] = hashlib.sha256(payload).hexdigest()
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            summary = build_corpus(profile, root / "state", now=1783785700)

            self.assertEqual(summary.excluded, 1)
            document = json.loads(
                (root / "state" / "corpus-build-summary.json").read_text(encoding="utf-8")
            )
            self.assertEqual(document["database_checks"]["ingest_errors"], 1)
            self.assertEqual(document["database_checks"]["foreign_key_check"], "ok")

    def test_failure_run_writes_bounded_summary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            (root / "ranking.json").write_text("not-json", encoding="utf-8")
            state = root / "state"

            with self.assertRaises(CorpusBuildError):
                build_corpus(profile, state, now=1783785700)

            summaries = list((state / "build").glob("*/corpus-build-summary.json"))
            self.assertEqual(len(summaries), 1)
            summary = json.loads(summaries[0].read_text(encoding="utf-8"))
            self.assertEqual(summary["status"], "failed")
            self.assertEqual(summary["phase"], "ranking")
            self.assertEqual(summary["exit_code"], 4)
            self.assertLess(len(json.dumps(summary)), 10000)

    def test_coverage_failure_is_exit_code_five_and_preserves_active(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            state = root / "state"
            state.mkdir()
            active = state / "corpus.sqlite"
            active.write_bytes(b"active")

            with mock.patch(
                "script.prepare_book_corpus.generate_coverage_report",
                side_effect=ValueError("coverage failed"),
            ):
                with self.assertRaises(CorpusBuildError) as raised:
                    build_corpus(profile, state, now=1783785700)

            self.assertEqual(raised.exception.phase, "coverage")
            self.assertEqual(raised.exception.exit_code, 5)
            self.assertEqual(active.read_bytes(), b"active")

    def test_corrupt_archive_is_ingest_failure_and_preserves_active(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = self.create_fixture(root)
            manifest_path = root / "manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            entry = manifest["sources"][0]
            source = root / "source" / "floodgate.zip"
            source.write_bytes(b"not-a-zip")
            entry["url"] = source.resolve().as_uri()
            entry["relative_path"] = "raw/floodgate.zip"
            entry["size"] = source.stat().st_size
            entry["sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            profile_document = json.loads(profile.read_text(encoding="utf-8"))
            profile_document["ingests"][0]["inputs"] = ["raw/floodgate.zip"]
            profile.write_text(json.dumps(profile_document), encoding="utf-8")
            state = root / "state"
            state.mkdir()
            active = state / "corpus.sqlite"
            active.write_bytes(b"active")

            with self.assertRaises(CorpusBuildError) as raised:
                build_corpus(profile, state, now=1783785700)

            self.assertEqual(raised.exception.phase, "ingest")
            self.assertEqual(raised.exception.exit_code, 4)
            self.assertEqual(active.read_bytes(), b"active")

    def test_cli_returns_phase_exit_code_without_traceback(self) -> None:
        with mock.patch(
            "script.prepare_book_corpus.build_corpus",
            side_effect=CorpusBuildError(3, "download", "hash mismatch"),
        ):
            self.assertEqual(
                main(["--profile", "pilot", "--state-dir", "state"]),
                3,
            )

    def test_storage_preflight_rejects_insufficient_free_space(self) -> None:
        from script import prepare_book_corpus

        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            with mock.patch.object(
                prepare_book_corpus.shutil,
                "disk_usage",
                return_value=mock.Mock(free=99),
            ):
                with self.assertRaisesRegex(CorpusBuildError, "required=100"):
                    prepare_book_corpus._check_free_space(root, 100)

    def test_database_generation_publish_uses_hardlink_without_full_copy(self) -> None:
        from script import prepare_book_corpus

        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            new_database = root / "new.sqlite"
            active = root / "corpus.sqlite"
            previous = root / "corpus.sqlite.previous"
            new_database.write_bytes(b"new")
            active.write_bytes(b"active")
            previous.write_bytes(b"older")
            with mock.patch.object(
                prepare_book_corpus.shutil,
                "copy2",
                side_effect=AssertionError("full database copy must not be used on hardlink-capable storage"),
            ):
                prepare_book_corpus._publish_database_generation(
                    new_database, active, previous
                )
            self.assertEqual(active.read_bytes(), b"new")
            self.assertEqual(previous.read_bytes(), b"active")
            self.assertFalse(new_database.exists())

    def test_database_generation_publish_propagates_replace_failure(self) -> None:
        from script import prepare_book_corpus

        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            new_database = root / "new.sqlite"
            active = root / "corpus.sqlite"
            previous = root / "corpus.sqlite.previous"
            new_database.write_bytes(b"new")
            active.write_bytes(b"active")
            original_replace = prepare_book_corpus.os.replace

            def fail_new_publish(source: object, destination: object) -> None:
                if pathlib.Path(source) == new_database and pathlib.Path(destination) == active:
                    raise OSError("injected publish failure")
                original_replace(source, destination)

            with mock.patch.object(
                prepare_book_corpus.os, "replace", side_effect=fail_new_publish
            ):
                with self.assertRaisesRegex(OSError, "injected publish failure"):
                    prepare_book_corpus._publish_database_generation(
                        new_database, active, previous
                    )
            self.assertEqual(active.read_bytes(), b"active")
            self.assertEqual(new_database.read_bytes(), b"new")
            self.assertFalse(previous.exists())

if __name__ == "__main__":
    unittest.main()
