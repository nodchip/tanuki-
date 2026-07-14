from __future__ import annotations

import pathlib
import tempfile
import tarfile
import unittest
import zipfile
from unittest import mock

from script.corpus_collection import iter_csa_records


class CorpusCollectionTest(unittest.TestCase):
    def test_undecodable_csa_is_yielded_for_game_level_error_recording(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            path = pathlib.Path(temporary_directory) / "broken.csa"
            path.write_bytes(b"V2.2\n\x81\x00\n")
            records = list(iter_csa_records(path))
            self.assertEqual(records[0][0], "broken.csa")
            self.assertIn("\ufffd", records[0][1])
    def test_reads_tar_xz(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            source = root / "game.csa"
            source.write_text("V2.2\n", encoding="utf-8")
            archive = root / "annual.tar.xz"
            with tarfile.open(archive, "w:xz") as output:
                output.add(source, "month/game.csa")
            self.assertEqual(
                list(iter_csa_records(archive)),
                [("annual.tar.xz!/month/game.csa", "V2.2\n")],
            )
    def test_reads_tar_xz_in_single_stream_and_preserves_sorted_order(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            archive = root / "annual.tar.xz"
            with tarfile.open(archive, "w:xz") as output:
                for name in ("z/game.csa", "a/game.csa"):
                    source = root / f"{name[0]}.csa"
                    source.write_text(f"V2.2\nN+{name[0]}\n", encoding="utf-8")
                    output.add(source, name)

            modes = []
            real_open = tarfile.open

            def tracked_open(*args, **kwargs):
                modes.append(args[1] if len(args) > 1 else kwargs.get("mode"))
                return real_open(*args, **kwargs)

            with mock.patch("script.corpus_collection.tarfile.open", side_effect=tracked_open):
                records = list(iter_csa_records(archive))

            self.assertEqual(modes, ["r|xz"])
            self.assertEqual(
                records,
                [
                    ("annual.tar.xz!/a/game.csa", "V2.2\nN+a\n"),
                    ("annual.tar.xz!/z/game.csa", "V2.2\nN+z\n"),
                ],
            )
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
    def test_converts_kif_members_in_zip_to_csa(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            archive = pathlib.Path(temporary_directory) / "event.zip"
            kif = (
                "先手：Alpha\n後手：Beta\n"
                "手数----指手---------消費時間--\n"
                "1 ７六歩(77)\n2 ３四歩(33)\n3 投了\n"
            )
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr("stage/game.kif", kif.encode("utf-8"))

            records = list(iter_csa_records(archive))

            self.assertEqual(records[0][0], "event.zip!/stage/game.kif")
            self.assertIn("N+Alpha", records[0][1])
            self.assertIn("+7776FU", records[0][1])
            self.assertIn("-3334FU", records[0][1])
            self.assertTrue(records[0][1].endswith("%CHUDAN\n"))

    def test_filters_zip_members_before_parsing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            archive = pathlib.Path(temporary_directory) / "event.zip"
            kif = (
                "先手：Alpha\n後手：Beta\n"
                "手数----指手---------消費時間--\n"
                "1 ７六歩(77)\n2 ３四歩(33)\n3 投了\n"
            )
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr("stage/game_tsec7p1-1.kif", b"not a KIF")
                output.writestr("stage/game_tsec7p2-1.kif", kif.encode("utf-8"))

            try:
                records = list(iter_csa_records(archive, r"_tsec7p2-.*\.kif$"))
            except TypeError as error:
                self.fail(f"member filtering should be supported: {error}")

            self.assertEqual(
                [source for source, _ in records],
                ["event.zip!/stage/game_tsec7p2-1.kif"],
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
