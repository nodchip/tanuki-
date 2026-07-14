from __future__ import annotations

import pathlib
import tempfile
import threading
import unittest

from script.book_corpus import CorpusStore, SearchTaskStatus


class BookCorpusStoreTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.db_path = pathlib.Path(self.temporary_directory.name) / "corpus.sqlite"
    def test_migration_creates_versioned_portable_database(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            path = pathlib.Path(temporary_directory) / "corpus.sqlite"
            with CorpusStore(path) as store:
                self.assertEqual(store.schema_version(), 5)
                self.assertEqual(store.corpus_revision(), 0)
                self.assertEqual(store.progressive_width(), 1)

            self.assertTrue(path.exists())

    def test_migration_creates_candidate_position_priority_index(self) -> None:
        with CorpusStore(self.db_path) as store:
            columns = store.connection.execute(
                "PRAGMA index_xinfo(candidate_position_priority_idx)"
            ).fetchall()

        indexed_columns = [
            (str(row["name"]), int(row["desc"]))
            for row in columns
            if int(row["key"]) == 1
        ]
        self.assertEqual(
            indexed_columns,
            [
                ("position_id", 0),
                ("active", 0),
                ("priority_key", 1),
                ("id", 0),
            ],
        )

    def test_connection_can_be_used_by_worker_under_external_state_lock(self) -> None:
        with CorpusStore(self.db_path) as store:
            state_lock = threading.Lock()
            errors: list[BaseException] = []

            def worker() -> None:
                try:
                    with state_lock:
                        store.upsert_candidate("p", "7g7f", [], "wcsc", "9")
                except BaseException as error:
                    errors.append(error)

            thread = threading.Thread(target=worker)
            thread.start()
            thread.join()
            self.assertEqual(errors, [])
    def test_migration_rejects_v4_with_rebuild_instruction(self) -> None:
        import sqlite3
        connection = sqlite3.connect(self.db_path)
        connection.execute("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        connection.execute("INSERT INTO meta(key, value) VALUES('schema_version', '4')")
        connection.commit()
        connection.close()
        with self.assertRaisesRegex(ValueError, "must be rebuilt as version 5"):
            CorpusStore(self.db_path)
    def test_migration_rejects_newer_unknown_schema(self) -> None:
        import sqlite3
        connection = sqlite3.connect(self.db_path)
        connection.execute("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        connection.execute("INSERT INTO meta(key, value) VALUES('schema_version', '999')")
        connection.commit()
        connection.close()
        with self.assertRaisesRegex(ValueError, "newer schema"):
            CorpusStore(self.db_path)
    def test_candidate_reservation_is_unique_per_book_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                store.upsert_candidate(
                    position_key="position",
                    move="7g7f",
                    history=("7g7f",),
                    source="wcsc",
                    priority_key="009",
                )

                first = store.reserve_candidate(
                    book_snapshot_id="book-a",
                    now=100.0,
                    lease_sec=30.0,
                )
                second = store.reserve_candidate(
                    book_snapshot_id="book-a",
                    now=101.0,
                    lease_sec=30.0,
                )

                self.assertIsNotNone(first)
                assert first is not None
                self.assertEqual(first.position_key, "position")
                self.assertEqual(first.move, "7g7f")
                self.assertEqual(first.history, ("7g7f",))
                self.assertEqual(first.status, SearchTaskStatus.RUNNING)
                self.assertIsNone(second)

    def test_expired_lease_can_be_reserved_again(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                store.upsert_candidate("position", "7g7f", (), "wcsc", "009")
                first = store.reserve_candidate("book-a", now=100.0, lease_sec=10.0)
                second = store.reserve_candidate("book-a", now=111.0, lease_sec=10.0)

                self.assertIsNotNone(first)
                self.assertIsNotNone(second)
                assert second is not None
                self.assertEqual(second.attempts, 2)

    def test_failed_search_becomes_permanent_after_three_attempts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                store.upsert_candidate("position", "7g7f", (), "wcsc", "009")
                status = None
                for attempt in range(3):
                    task = store.reserve_candidate("book-a", now=100.0 + attempt, lease_sec=1.0)
                    self.assertIsNotNone(task)
                    assert task is not None
                    status = store.fail_search(task.id, "engine-disconnected", max_attempts=3)

                self.assertEqual(status, SearchTaskStatus.PERMANENT_FAILED)
                self.assertIsNone(
                    store.reserve_candidate("book-a", now=200.0, lease_sec=1.0)
                )

    def test_completed_search_remains_unpersisted_until_checkpoint(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                store.upsert_candidate("position", "7g7f", (), "wcsc", "009")
                task = store.reserve_candidate("book-a", now=100.0, lease_sec=10.0)
                assert task is not None

                store.complete_search(
                    task.id,
                    eval_cp=123,
                    response="3c3d",
                    depth=12,
                    nodes=1000,
                    engine_config_id="engine-v1",
                )

                results = store.unpersisted_results()
                self.assertEqual(len(results), 1)
                self.assertEqual(results[0].move, "7g7f")
                self.assertEqual(results[0].eval_cp, 123)
                self.assertIsNone(results[0].persisted_checkpoint_id)

                checkpoint_id = store.record_checkpoint("book-hash")
                self.assertEqual(store.unpersisted_results(), [])
                self.assertEqual(store.checkpoint(checkpoint_id).book_hash, "book-hash")

    def test_same_move_keeps_multiple_source_provenances(self) -> None:
        with CorpusStore(self.db_path) as store:
            store.upsert_candidate("p", "7g7f", [], "wcsc:event-a", "9")
            store.upsert_candidate("p", "7g7f", [], "denryu:event-b", "8")
            self.assertEqual(store.candidate_source_count("p", "7g7f"), 2)

    def test_reserve_for_position_only_uses_top_n_at_visited_position(self) -> None:
        with CorpusStore(self.db_path) as store:
            store.upsert_candidate("other", "9g9f", [], "wcsc", "99")
            store.upsert_candidate("visited", "7g7f", [], "wcsc", "30")
            store.upsert_candidate("visited", "2g2f", [], "denryu", "20")
            store.upsert_candidate("visited", "5g5f", [], "floodgate", "10")

            task = store.reserve_for_position(
                "book-a", "visited", excluded_moves={"7g7f"}, width=2, lease_sec=10
            )
            self.assertIsNotNone(task)
            assert task is not None
            self.assertEqual(task.move, "2g2f")

            self.assertIsNone(
                store.reserve_for_position(
                    "book-a", "visited", excluded_moves={"7g7f", "2g2f"}, width=2, lease_sec=10
                )
            )
    def test_progressive_width_persists_and_increments_after_100_empty_rollouts(self) -> None:
        with CorpusStore(self.db_path) as store:
            for _ in range(99):
                self.assertFalse(store.record_corpus_rollout(added=False, saturation_window=100))
            self.assertTrue(store.record_corpus_rollout(added=False, saturation_window=100))
            self.assertEqual(store.progressive_width(), 2)

            store.record_corpus_rollout(added=True, saturation_window=100)
            self.assertEqual(store.zero_addition_rollouts(), 0)

    def test_new_corpus_revision_keeps_n_and_resets_saturation_counter(self) -> None:
        with CorpusStore(self.db_path) as store:
            store.record_corpus_rollout(added=False, saturation_window=100)
            revision = store.bump_corpus_revision()
            self.assertEqual(revision, 1)
            self.assertEqual(store.progressive_width(), 1)
            self.assertEqual(store.zero_addition_rollouts(), 0)
    def test_results_after_unknown_book_hash_replays_even_checkpointed_results(self) -> None:
        with CorpusStore(self.db_path) as store:
            store.upsert_candidate("p", "7g7f", [], "wcsc", "9")
            task = store.reserve_candidate("book-a", lease_sec=10)
            assert task is not None
            store.complete_search(task.id, eval_cp=1, response="none", depth=1,
                                  nodes=1, engine_config_id="e")
            store.record_checkpoint("newer-book")
            results = store.results_requiring_replay("older-book")
            self.assertEqual([result.task_id for result in results], [task.id])
            self.assertEqual(store.results_requiring_replay("newer-book"), [])
    def test_registered_move_supersedes_pending_retry_so_it_cannot_block_n(self) -> None:
        with CorpusStore(self.db_path) as store:
            store.upsert_candidate("p", "7g7f", [], "wcsc", "9")
            task = store.reserve_for_position("book-a", "p", excluded_moves=set(), width=1, lease_sec=10)
            assert task is not None
            store.fail_search(task.id, "temporary", max_attempts=3)
            self.assertIsNone(store.reserve_for_position(
                "book-a", "p", excluded_moves={"7g7f"}, width=1, lease_sec=10
            ))
            status = store.connection.execute(
                "SELECT status FROM search_task WHERE id = ?", (task.id,)
            ).fetchone()["status"]
            self.assertEqual(status, SearchTaskStatus.SUPERSEDED.value)
    def test_reset_interrupted_tasks_returns_running_tasks_to_pending(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            with CorpusStore(pathlib.Path(temporary_directory) / "corpus.sqlite") as store:
                store.upsert_candidate("position", "7g7f", (), "wcsc", "009")
                task = store.reserve_candidate("book-a", now=100.0, lease_sec=100.0)
                assert task is not None

                self.assertEqual(store.reset_interrupted_tasks(), 1)
                rerun = store.reserve_candidate("book-a", now=101.0, lease_sec=10.0)

                self.assertIsNotNone(rerun)


if __name__ == "__main__":
    unittest.main()