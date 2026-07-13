from __future__ import annotations

import json
import pathlib
import tempfile
import unittest

from script.book_corpus import CorpusStore
from script.book_extension_bundle import (
    BundleClaimError,
    claim_bundle,
    create_bundle,
    extract_and_verify_bundle,
)


class BookExtensionBundleTest(unittest.TestCase):
    def test_bundle_round_trip_verifies_hashes_schema_n_and_revision(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            book = root / "book.db"
            book.write_text("#YANEURAOU-DB2016 1.00\n", encoding="utf-8")
            config = root / "extension.toml"
            config.write_text("[runtime]\n", encoding="utf-8")
            database = root / "corpus.sqlite"
            with CorpusStore(database) as store:
                store.bump_corpus_revision()
            runtime_exe = root / "book-extender.exe"
            runtime_exe.write_bytes(b"rust-runtime")
            archive = root / "bundle.zip"

            manifest = create_bundle(
                archive, book=book, corpus_db=database, config=config,
                vulnerability_books=[],
                runtime_exe=runtime_exe,
            )
            extracted = root / "extracted"
            verified = extract_and_verify_bundle(archive, extracted)

            self.assertEqual(verified["bundle_id"], manifest["bundle_id"])
            self.assertEqual(verified["schema_version"], 4)
            self.assertEqual(verified["progressive_width"], 1)
            self.assertEqual(verified["corpus_revision"], 1)
            self.assertTrue((extracted / "book" / "book.db").exists())
            self.assertEqual((extracted / "runtime" / "book-extender.exe").read_bytes(), b"rust-runtime")

    def test_shared_claim_registry_rejects_second_machine(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            registry = pathlib.Path(temporary_directory)
            claim_bundle("bundle-1", registry, machine_id="machine-a")
            with self.assertRaises(BundleClaimError):
                claim_bundle("bundle-1", registry, machine_id="machine-b")


if __name__ == "__main__":
    unittest.main()