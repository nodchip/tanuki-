from __future__ import annotations

import json
import pathlib
import tempfile
import unittest

from script.corpus_build_profile import ProfileError, load_build_profile


class CorpusBuildProfileTest(unittest.TestCase):
    def write_fixture(self, root: pathlib.Path, **overrides: object) -> pathlib.Path:
        manifest = {
            "sources": [
                {
                    "site": "floodgate",
                    "event": "fg-2026",
                    "year": 2026,
                    "retrieved_at": 1783785600,
                    "url": "file:///fixture.7z",
                    "relative_path": "raw/fixture.7z",
                    "size": 1,
                    "sha256": "00" * 32,
                }
            ]
        }
        (root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        for name in ("ranking.json", "rating.json", "extension.toml", "book.db"):
            (root / name).write_text("{}", encoding="utf-8")
        profile: dict[str, object] = {
            "schema_version": 1,
            "name": "pilot",
            "manifest": "manifest.json",
            "ingests": [
                {
                    "site": "floodgate",
                    "event": "fg-2026",
                    "year": 2026,
                    "retrieved_at": 1783785600,
                    "inputs": ["raw/fixture.7z"],
                }
            ],
            "ranking_files": ["ranking.json"],
            "rating_file": "rating.json",
            "rating_config": "extension.toml",
            "input_book": "book.db",
            "coverage_snapshot_id": "pilot-initial",
        }
        profile.update(overrides)
        path = root / "profile.json"
        path.write_text(json.dumps(profile), encoding="utf-8")
        return path

    def test_loads_and_resolves_paths_relative_to_profile(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            profile = load_build_profile(self.write_fixture(root))
            self.assertEqual(profile.name, "pilot")
            self.assertEqual(profile.manifest, root / "manifest.json")
            self.assertEqual(profile.ingests[0].inputs, (pathlib.Path("raw/fixture.7z"),))
            self.assertEqual(profile.ranking_files, (root / "ranking.json",))
            self.assertEqual(profile.input_book, root / "book.db")

    def test_rejects_unknown_profile_key(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            with self.assertRaisesRegex(ProfileError, "unknown profile keys"):
                load_build_profile(self.write_fixture(root, surprise=True))

    def test_rejects_duplicate_ingest_identity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            path = self.write_fixture(root)
            document = json.loads(path.read_text(encoding="utf-8"))
            document["ingests"].append(dict(document["ingests"][0]))
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ProfileError, "duplicate ingest"):
                load_build_profile(path)

    def test_rejects_ingest_missing_from_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            path = self.write_fixture(root)
            document = json.loads(path.read_text(encoding="utf-8"))
            document["ingests"][0]["event"] = "unknown"
            path.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(ProfileError, "not represented in manifest"):
                load_build_profile(path)

    def test_rejects_invalid_profile_name(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            with self.assertRaisesRegex(ProfileError, "name must be pilot or production"):
                load_build_profile(self.write_fixture(root, name="staging"))

    def test_allows_machine_local_input_book_and_optional_rating(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            path = self.write_fixture(root, input_book=None, rating_file=None, rating_config=None)

            profile = load_build_profile(path)

            self.assertIsNone(profile.input_book)
            self.assertIsNone(profile.rating_file)
            self.assertIsNone(profile.rating_config)


if __name__ == "__main__":
    unittest.main()
