from __future__ import annotations

import pathlib
import tempfile
import textwrap
import unittest

from script.book_extension_config import (
    ConfigError,
    CorpusConfig,
    ExtensionConfig,
    RuntimeConfig,
    WorkerConfig,
    load_extension_config,
)


class BookExtensionConfigTest(unittest.TestCase):
    def test_production_defaults_preserve_8_8_8_roles(self) -> None:
        config = ExtensionConfig.production_defaults(pathlib.Path(r"C:\book-state"))

        self.assertEqual(config.workers.engine_count, 24)
        self.assertEqual(config.workers.threads_per_engine, 1)
        self.assertEqual(
            config.workers.roles(),
            ("vulnerability_black",) * 8
            + ("vulnerability_white",) * 8
            + ("general",) * 8,
        )
        self.assertEqual(config.corpus.max_concurrent_searches, 2)
        self.assertEqual(config.corpus.general_pool_node_share, 0.25)
        self.assertEqual(config.runtime.save_interval_sec, 3600.0)

    def test_pilot_defaults_use_eight_single_threaded_engines(self) -> None:
        config = ExtensionConfig.pilot_defaults(pathlib.Path(r"C:\book-state"))

        self.assertEqual(config.workers.engine_count, 8)
        self.assertEqual(config.workers.threads_per_engine, 1)
        self.assertEqual(config.workers.vulnerability_black, 2)
        self.assertEqual(config.workers.vulnerability_white, 2)
        self.assertEqual(config.workers.general, 4)
        self.assertEqual(config.corpus.max_concurrent_searches, 1)

    def test_worker_counts_must_sum_to_engine_count(self) -> None:
        workers = WorkerConfig(
            engine_count=8,
            threads_per_engine=1,
            vulnerability_black=2,
            vulnerability_white=2,
            general=3,
        )

        with self.assertRaisesRegex(ConfigError, "engine_count"):
            workers.validate()

    def test_corpus_concurrency_cannot_exceed_general_workers(self) -> None:
        config = ExtensionConfig(
            workers=WorkerConfig(3, 1, 1, 1, 1),
            corpus=CorpusConfig(True, 2, 0.25),
            runtime=RuntimeConfig(pathlib.Path(r"C:\state"), 3600.0, 3, 10.0, 60.0),
        )

        with self.assertRaisesRegex(ConfigError, "max_concurrent_searches"):
            config.validate()

    def test_node_share_must_be_between_zero_and_one(self) -> None:
        corpus = CorpusConfig(True, 1, 1.5)

        with self.assertRaisesRegex(ConfigError, "general_pool_node_share"):
            corpus.validate(general_workers=1)

    def test_load_toml_resolves_relative_state_dir_from_config_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            config_path = root / "config" / "extension.toml"
            config_path.parent.mkdir()
            config_path.write_text(
                textwrap.dedent(
                    """\
                    [workers]
                    engine_count = 3
                    threads_per_engine = 1
                    vulnerability_black = 1
                    vulnerability_white = 1
                    general = 1

                    [corpus]
                    enabled = true
                    max_concurrent_searches = 1
                    general_pool_node_share = 0.25

                    [runtime]
                    state_dir = "../state"
                    save_interval_sec = 3600
                    backup_count = 3
                    heartbeat_timeout_sec = 10
                    usi_stop_timeout_sec = 60
                    """
                ),
                encoding="utf-8",
            )

            config = load_extension_config(config_path)

        self.assertEqual(config.runtime.state_dir, (root / "state").resolve())
        self.assertEqual(config.workers.roles(), (
            "vulnerability_black",
            "vulnerability_white",
            "general",
        ))


if __name__ == "__main__":
    unittest.main()