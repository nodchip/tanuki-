from __future__ import annotations

import pathlib
import shutil
import subprocess
import tempfile
import zipfile
from collections.abc import Iterator


class CollectionError(ValueError):
    pass


def _decode_csa(data: bytes) -> str:
    for encoding in ("utf-8-sig", "cp932"):
        try:
            return data.decode(encoding).replace("\r\n", "\n").replace("\r", "\n")
        except UnicodeDecodeError:
            pass
    return data.decode("utf-8", errors="replace").replace("\r\n", "\n").replace("\r", "\n")


def iter_csa_records(path: pathlib.Path) -> Iterator[tuple[str, str]]:
    """Yield stable source-relative names and CSA text from loose, ZIP, directory, or 7z inputs."""
    path = pathlib.Path(path)
    if path.is_dir():
        for child in sorted(path.rglob("*.csa")):
            yield child.relative_to(path).as_posix(), _decode_csa(child.read_bytes())
        return
    suffix = path.suffix.lower()
    if suffix == ".csa":
        yield path.name, _decode_csa(path.read_bytes())
        return
    if suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            for name in sorted(archive.namelist()):
                member = pathlib.PurePosixPath(name)
                if member.suffix.lower() != ".csa" or member.is_absolute() or ".." in member.parts:
                    continue
                yield f"{path.name}!/{member.as_posix()}", _decode_csa(archive.read(name))
        return
    if suffix == ".7z":
        executable = shutil.which("7z") or shutil.which("7z.exe")
        with tempfile.TemporaryDirectory() as temporary_directory:
            if executable is not None:
                result = subprocess.run(
                    [executable, "x", str(path), f"-o{temporary_directory}", "-y"],
                    capture_output=True, text=True, encoding="utf-8", errors="replace",
                )
                if result.returncode != 0:
                    raise CollectionError(f"7z extraction failed: {result.stderr or result.stdout}")
            else:
                try:
                    import py7zr
                except ImportError as error:
                    raise CollectionError("7z executable or py7zr is required") from error
                with py7zr.SevenZipFile(path) as archive:
                    archive.extractall(temporary_directory)
            root = pathlib.Path(temporary_directory)
            for child in sorted(root.rglob("*.csa")):
                yield f"{path.name}!/{child.relative_to(root).as_posix()}", _decode_csa(child.read_bytes())
        return
    raise CollectionError(f"unsupported corpus input: {path}")