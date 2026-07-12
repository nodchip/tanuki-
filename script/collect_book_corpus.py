from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import tempfile
import urllib.request
from typing import Optional, Sequence


class ManifestError(ValueError):
    pass


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def collect_manifest(manifest_path: pathlib.Path, destination: pathlib.Path) -> list[dict[str, object]]:
    document = json.loads(manifest_path.read_text(encoding="utf-8"))
    entries = document.get("sources")
    if not isinstance(entries, list):
        raise ManifestError("manifest requires a sources list")
    destination.mkdir(parents=True, exist_ok=True)
    collected: list[dict[str, object]] = []
    for entry in entries:
        if not isinstance(entry, dict):
            raise ManifestError("each source must be an object")
        required = {"site", "event", "year", "retrieved_at", "url", "relative_path", "size", "sha256"}
        missing = required - set(entry)
        if missing:
            raise ManifestError(f"source is missing {sorted(missing)}")
        if entry["site"] not in {"floodgate", "wcsc", "denryu"}:
            raise ManifestError(f"unknown site: {entry['site']}")
        relative = pathlib.PurePosixPath(str(entry["relative_path"]))
        if relative.is_absolute() or ".." in relative.parts:
            raise ManifestError("relative_path must stay within destination")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists():
            actual_size = target.stat().st_size
            actual_hash = sha256_file(target)
            if actual_size == int(entry["size"]) and actual_hash.lower() == str(entry["sha256"]).lower():
                collected.append({**entry, "size": actual_size, "sha256": actual_hash,
                                  "local_path": relative.as_posix()})
                continue
            raise ManifestError(f"existing file does not match frozen manifest: {relative}")
        with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as temporary:
            temporary_path = pathlib.Path(temporary.name)
            with urllib.request.urlopen(str(entry["url"])) as response:
                while block := response.read(1024 * 1024):
                    temporary.write(block)
        actual_size = temporary_path.stat().st_size
        if actual_size != int(entry["size"]):
            temporary_path.unlink(missing_ok=True)
            raise ManifestError(f"size mismatch: {relative}")
        actual_hash = sha256_file(temporary_path)
        expected_hash = entry.get("sha256")
        if expected_hash and actual_hash.lower() != str(expected_hash).lower():
            temporary_path.unlink(missing_ok=True)
            raise ManifestError(f"SHA-256 mismatch: {relative}")
        temporary_path.replace(target)
        collected.append({**entry, "size": actual_size, "sha256": actual_hash, "local_path": relative.as_posix()})
    snapshot = {
        "manifest_version": 1,
        "source_manifest": manifest_path.name,
        "sources": collected,
    }
    (destination / "snapshot.json").write_text(
        json.dumps(snapshot, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return collected


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Download only an explicitly frozen corpus manifest")
    parser.add_argument("--manifest", type=pathlib.Path, required=True)
    parser.add_argument("--destination", type=pathlib.Path, required=True)
    args = parser.parse_args(argv)
    collected = collect_manifest(args.manifest, args.destination)
    print(json.dumps({"collected": len(collected)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())