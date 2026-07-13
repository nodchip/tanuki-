from __future__ import annotations

import pathlib
import tempfile
import textwrap
import unittest

from unittest import mock

from script.run_book_extension import build_extension_argv, main


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
                runtime_exe=root / "book-extender.exe",
                config_path=config, input_book=root / "in.db", output_book=root / "out.db",
                engine=root / "engine.exe", nodes=3000, multipv=4,
                black_target=root / "target.db", white_target=root / "target.db",
                corpus_db=root / "corpus.sqlite",
                ignore_ply=True,
            )

        self.assertEqual(argv[0], str(root / "book-extender.exe"))
        self.assertIn("--config", argv)
        self.assertEqual(argv.count(str(root / "target.db")), 2)
        self.assertNotIn("--engine-count", argv)
        self.assertNotIn("--corpus-nodes", argv)
        self.assertIn("--ignore-ply", argv)
        self.assertNotIn("--heartbeat-path", argv)

    @mock.patch("script.run_book_extension.subprocess.run")
    def test_main_launches_rust_runtime_without_python_mcts(self, run: mock.Mock) -> None:
        run.return_value.returncode = 7
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            config = root / "config.toml"
            config.write_text(textwrap.dedent("""\
                [workers]
                engine_count = 1
                threads_per_engine = 1
                vulnerability_black = 0
                vulnerability_white = 0
                general = 1
                [corpus]
                enabled = false
                max_concurrent_searches = 0
                general_pool_node_share = 0.25
                [runtime]
                state_dir = "state"
                save_interval_sec = 3600
                backup_count = 3
                heartbeat_timeout_sec = 10
                usi_stop_timeout_sec = 60
            """), encoding="utf-8")
            result = main([
                "--runtime-exe", str(root / "book-extender.exe"),
                "--config", str(config), "--input", str(root / "in.db"),
                "--output", str(root / "out.db"), "--engine", str(root / "engine.exe"),
                "--nodes", "10", "--multipv", "1", "--black-target", str(root / "target.db"),
                "--white-target", str(root / "target.db"),
            ])
        self.assertEqual(result, 7)
        self.assertEqual(run.call_args.args[0][0], str(root / "book-extender.exe"))

class RustJenkinsWrapperTest(unittest.TestCase):
    def test_jenkins_wrapper_launches_rust_runtime_directly(self) -> None:
        wrapper = pathlib.Path(__file__).with_name("run_extend_book_mcts_jenkins.ps1")
        text = wrapper.read_text(encoding="utf-8")

        self.assertIn("[string]$RuntimeExe", text)
        self.assertIn("Start-Process -FilePath $RuntimeExe", text)
        self.assertLess(
            text.index("[System.IO.File]::WriteAllText($heartbeatPath"),
            text.index("Start-Process -FilePath $RuntimeExe"),
        )
        self.assertLess(
            text.index("Start-Process -FilePath $RuntimeExe"),
            text.index("$env:BUILD_ID = $previousBuildId"),
        )
        self.assertNotIn("$PythonExe", text)
        self.assertNotIn("$ExtensionScript", text)

if __name__ == "__main__":
    unittest.main()
