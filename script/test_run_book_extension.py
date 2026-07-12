from __future__ import annotations

import pathlib
import tempfile
import textwrap
import unittest

from script.run_book_extension import build_extension_argv


class RunBookExtensionTest(unittest.TestCase):
    def test_pilot_config_expands_roles_and_corpus_node_share(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            config = root / "pilot.toml"
            config.write_text(textwrap.dedent("""\
                [workers]
                engine_count = 8
                threads_per_engine = 1
                vulnerability_black = 2
                vulnerability_white = 2
                general = 4
                [corpus]
                enabled = true
                max_concurrent_searches = 1
                general_pool_node_share = 0.25
                [runtime]
                state_dir = "state"
                save_interval_sec = 3600
                backup_count = 3
                heartbeat_timeout_sec = 10
                usi_stop_timeout_sec = 60
            """), encoding="utf-8")
            argv = build_extension_argv(
                config_path=config, input_book=root / "in.db", output_book=root / "out.db",
                engine=root / "engine.exe", nodes=3000, multipv=4,
                black_target=root / "target.db", white_target=root / "target.db",
                corpus_db=root / "corpus.sqlite",
                ignore_ply=True,
            )

        self.assertEqual(argv.count(f"{root / 'target.db'}:black"), 2)
        self.assertEqual(argv.count(f"{root / 'target.db'}:white"), 2)
        self.assertIn("1000", argv)
        self.assertIn("8", argv)
        self.assertIn("1", argv)
        self.assertIn("--ignore-ply", argv)


if __name__ == "__main__":
    unittest.main()