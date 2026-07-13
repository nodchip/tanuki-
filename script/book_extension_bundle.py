from __future__ import annotations

import hashlib
import json
import os
import pathlib
import shutil
import sqlite3
import tempfile
import time
import uuid
import zipfile
from typing import Any, Sequence

try:
    from script.book_corpus import CorpusStore, SearchTaskStatus
except ImportError:
    from book_corpus import CorpusStore, SearchTaskStatus


class BundleError(ValueError):
    pass


class BundleClaimError(BundleError):
    pass


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def create_bundle(
    archive: pathlib.Path,
    *,
    book: pathlib.Path,
    corpus_db: pathlib.Path,
    config: pathlib.Path,
    vulnerability_books: Sequence[pathlib.Path],
    runtime_exe: pathlib.Path,
) -> dict[str, Any]:
    """Create a stopped, checkpointed, hash-manifested portable ZIP bundle."""
    archive = pathlib.Path(archive)
    archive.parent.mkdir(parents=True, exist_ok=True)
    with CorpusStore(corpus_db) as store:
        active = store.connection.execute(
            "SELECT COUNT(*) AS count FROM search_task WHERE status = ?",
            (SearchTaskStatus.RUNNING.value,),
        ).fetchone()
        if int(active["count"]) != 0:
            raise BundleError("cannot bundle while corpus searches are running")
        if store.unpersisted_results():
            raise BundleError("cannot bundle before evaluated results have a book checkpoint")
        schema_version = store.schema_version()
        progressive_width = store.progressive_width()
        corpus_revision = store.corpus_revision()
        store.connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")

    with tempfile.TemporaryDirectory(dir=archive.parent) as temporary_directory:
        root = pathlib.Path(temporary_directory)
        members: dict[str, pathlib.Path] = {}
        book_target = root / "book" / pathlib.Path(book).name
        book_target.parent.mkdir(parents=True)
        shutil.copy2(book, book_target)
        members[book_target.relative_to(root).as_posix()] = book_target

        database_target = root / "state" / "corpus.sqlite"
        database_target.parent.mkdir(parents=True)
        source_connection = sqlite3.connect(corpus_db)
        target_connection = sqlite3.connect(database_target)
        try:
            source_connection.backup(target_connection)
        finally:
            target_connection.close()
            source_connection.close()
        members[database_target.relative_to(root).as_posix()] = database_target

        config_target = root / "config" / pathlib.Path(config).name
        config_target.parent.mkdir(parents=True)
        shutil.copy2(config, config_target)
        members[config_target.relative_to(root).as_posix()] = config_target
        runtime_target = root / "runtime" / pathlib.Path(runtime_exe).name
        runtime_target.parent.mkdir(parents=True)
        shutil.copy2(runtime_exe, runtime_target)
        members[runtime_target.relative_to(root).as_posix()] = runtime_target

        for index, target_book in enumerate(vulnerability_books):
            target = root / "targets" / f"{index:02d}-{pathlib.Path(target_book).name}"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(target_book, target)
            members[target.relative_to(root).as_posix()] = target

        manifest: dict[str, Any] = {
            "bundle_id": str(uuid.uuid4()),
            "created_at": time.time(),
            "single_use": True,
            "schema_version": schema_version,
            "progressive_width": progressive_width,
            "corpus_revision": corpus_revision,
            "files": {name: _sha256(path) for name, path in sorted(members.items())},
        }
        manifest_path = root / "manifest.json"
        manifest_path.write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
            output.write(manifest_path, "manifest.json")
            for name, path in sorted(members.items()):
                output.write(path, name)
    return manifest


def extract_and_verify_bundle(archive: pathlib.Path, destination: pathlib.Path) -> dict[str, Any]:
    destination = pathlib.Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as source:
        for member in source.namelist():
            path = pathlib.PurePosixPath(member)
            if path.is_absolute() or ".." in path.parts:
                raise BundleError(f"unsafe bundle member: {member}")
        source.extractall(destination)
    manifest = json.loads((destination / "manifest.json").read_text(encoding="utf-8"))
    for name, expected_hash in manifest["files"].items():
        path = destination / pathlib.PurePosixPath(name)
        if not path.is_file() or _sha256(path) != expected_hash:
            raise BundleError(f"bundle hash mismatch: {name}")
    database = destination / "state" / "corpus.sqlite"
    with CorpusStore(database) as store:
        if store.schema_version() != manifest["schema_version"]:
            raise BundleError("schema version mismatch")
        if store.progressive_width() != manifest["progressive_width"]:
            raise BundleError("progressive width mismatch")
        if store.corpus_revision() != manifest["corpus_revision"]:
            raise BundleError("corpus revision mismatch")
    return manifest


def claim_bundle(bundle_id: str, registry_dir: pathlib.Path, *, machine_id: str) -> pathlib.Path:
    """Atomically claim a bundle in a registry shared by all extension machines."""
    registry = pathlib.Path(registry_dir)
    registry.mkdir(parents=True, exist_ok=True)
    claim = registry / f"{bundle_id}.claim.json"
    payload = json.dumps({"bundle_id": bundle_id, "machine_id": machine_id, "claimed_at": time.time()})
    try:
        with claim.open("x", encoding="utf-8") as stream:
            stream.write(payload + "\n")
    except FileExistsError as error:
        owner = claim.read_text(encoding="utf-8", errors="replace").strip()
        raise BundleClaimError(f"bundle already claimed: {owner}") from error
    return claim