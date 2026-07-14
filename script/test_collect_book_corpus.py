from __future__ import annotations

import hashlib
import json
import pathlib
import tempfile
import unittest

from script.collect_book_corpus import collect_manifest


class CollectBookCorpusProgressTest(unittest.TestCase):
    def create_manifest(self, root: pathlib.Path, payload: bytes) -> pathlib.Path:
        source = root / "source.bin"
        source.write_bytes(payload)
        manifest = root / "manifest.json"
        manifest.write_text(
            json.dumps(
                {
                    "sources": [
                        {
                            "site": "floodgate",
                            "event": "floodgate-2026",
                            "year": 2026,
                            "retrieved_at": 1783785600,
                            "url": source.resolve().as_uri(),
                            "relative_path": "raw/source.bin",
                            "size": len(payload),
                            "sha256": hashlib.sha256(payload).hexdigest(),
                        }
                    ]
                }
            ),
            encoding="utf-8",
        )
        return manifest

    def test_download_reports_source_chunks_and_completion(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            payload = b"a" * (2 * 1024**2 + 17)
            manifest = self.create_manifest(root, payload)
            events: list[tuple[str, dict[str, object]]] = []

            collect_manifest(
                manifest,
                root / "downloads",
                progress=lambda kind, fields: events.append((kind, fields)),
            )

            self.assertEqual(
                [kind for kind, _ in events],
                ["source_start", "download_chunk", "download_chunk", "download_chunk", "source_done"],
            )
            first = events[0][1]
            self.assertEqual(first["source_index"], 1)
            self.assertEqual(first["source_count"], 1)
            self.assertEqual(first["file"], "raw/source.bin")
            self.assertEqual(first["file_total"], len(payload))
            self.assertEqual(first["aggregate_total"], len(payload))
            final_chunk = events[-2][1]
            self.assertEqual(final_chunk["file_bytes"], len(payload))
            self.assertEqual(final_chunk["aggregate_bytes"], len(payload))
            self.assertEqual(events[-1][1]["aggregate_bytes"], len(payload))

    def test_cached_source_reports_cached_without_chunk_events(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = pathlib.Path(temporary_directory)
            payload = b"cached payload"
            manifest = self.create_manifest(root, payload)
            destination = root / "downloads"
            collect_manifest(manifest, destination)
            events: list[tuple[str, dict[str, object]]] = []

            collect_manifest(
                manifest,
                destination,
                progress=lambda kind, fields: events.append((kind, fields)),
            )

            self.assertEqual(
                [kind for kind, _ in events], ["source_start", "source_cached"]
            )
            self.assertEqual(events[-1][1]["aggregate_bytes"], len(payload))
            self.assertEqual(events[-1][1]["file_bytes"], len(payload))


if __name__ == "__main__":
    unittest.main()
