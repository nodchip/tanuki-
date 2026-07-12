from __future__ import annotations

import pathlib
import tempfile
import unittest
import zipfile

from script.corpus_collection import iter_csa_records


class CorpusCollectionTest(unittest.TestCase):
    def test_undecodable_csa_is_yielded_for_game_level_error_recording(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            path = pathlib.Path(temporary_directory) / "broken.csa"
            path.write_bytes(b"V2.2\n\x81\x00\n")
            records = list(iter_csa_records(path))
            self.assertEqual(records[0][0], "broken.csa")
            self.assertIn("\ufffd", records[0][1])
    def test_reads_7z_with_python_fallback(self) -> None:
        import py7zr
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            source = root / "game.csa"
            source.write_text("V2.2\n", encoding="utf-8")
            archive = root / "annual.7z"
            with py7zr.SevenZipFile(archive, "w") as output:
                output.write(source, "month/game.csa")
            self.assertEqual(
                list(iter_csa_records(archive)),
                [("annual.7z!/month/game.csa", "V2.2\n")],
            )
    def test_reads_loose_and_zip_csa_with_stable_relative_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            loose = root / "loose.csa"
            loose.write_text("V2.2\n", encoding="utf-8")
            archive = root / "event.zip"
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr("stage/game.csa", "V2.2\n")
                output.writestr("ignore.txt", "x")

            self.assertEqual(list(iter_csa_records(loose)), [("loose.csa", "V2.2\n")])
            self.assertEqual(
                list(iter_csa_records(archive)),
                [("event.zip!/stage/game.csa", "V2.2\n")],
            )


if __name__ == "__main__":
    unittest.main()