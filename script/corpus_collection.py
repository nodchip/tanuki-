from __future__ import annotations

import pathlib
import re
import shutil
import subprocess
import tempfile
import tarfile
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


def _kif_to_csa(data: bytes) -> str:
    try:
        import cshogi
        from cshogi import KIF
    except ImportError as error:
        raise CollectionError("cshogi is required for KIF corpus input") from error
    parsed = KIF.Parser.parse_str(_decode_csa(data))
    if isinstance(parsed, list):
        if not parsed:
            raise CollectionError("KIF contains no game")
        parsed = parsed[0]
    if parsed.sfen != cshogi.STARTING_SFEN:
        raise CollectionError("KIF game does not start from startpos")
    names = list(parsed.names[:2]) + ["", ""]
    lines = ["V2.2", f"N+{names[0] or ''}", f"N-{names[1] or ''}", "PI", "+"]
    for ply, move in enumerate(parsed.moves):
        lines.append(("+" if ply % 2 == 0 else "-") + cshogi.move_to_csa(move))
    lines.append(parsed.endgame or "%CHUDAN")
    return "\n".join(lines) + "\n"

def iter_csa_records(
    path: pathlib.Path,
    member_pattern: str | None = None,
) -> Iterator[tuple[str, str]]:
    """Yield stable source-relative names and CSA text from loose, ZIP, directory, or 7z inputs."""
    path = pathlib.Path(path)
    member_regex = re.compile(member_pattern) if member_pattern is not None else None

    def included(name: str) -> bool:
        return member_regex is None or member_regex.search(name) is not None

    if path.is_dir():
        for child in sorted(item for item in path.rglob("*") if item.suffix.lower() in {".csa", ".kif"}):
            relative_name = child.relative_to(path).as_posix()
            if not included(relative_name):
                continue
            text = _decode_csa(child.read_bytes()) if child.suffix.lower() == ".csa" else _kif_to_csa(child.read_bytes())
            yield relative_name, text
        return
    if path.name.lower().endswith(".tar.xz"):
        entries: list[tuple[str, int, int, int]] = []
        with tempfile.TemporaryFile(mode="w+b", dir=path.parent) as spool:
            with tarfile.open(path, "r|xz") as archive:
                for ordinal, member in enumerate(archive):
                    member_path = pathlib.PurePosixPath(member.name)
                    if (
                        not member.isfile()
                        or member_path.suffix.lower() != ".csa"
                        or member_path.is_absolute()
                        or ".." in member_path.parts
                        or not included(member_path.as_posix())
                    ):
                        continue
                    source = archive.extractfile(member)
                    if source is None:
                        continue
                    data = source.read()
                    offset = spool.tell()
                    spool.write(data)
                    entries.append((member_path.as_posix(), ordinal, offset, len(data)))
            for member_name, _, offset, size in sorted(entries):
                spool.seek(offset)
                yield f"{path.name}!/{member_name}", _decode_csa(spool.read(size))
        return
    suffix = path.suffix.lower()
    if suffix == ".csa":
        yield path.name, _decode_csa(path.read_bytes())
        return
    if suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            for name in sorted(archive.namelist()):
                member = pathlib.PurePosixPath(name)
                if member.is_absolute() or ".." in member.parts:
                    continue
                if not included(member.as_posix()):
                    continue
                if member.suffix.lower() == ".csa":
                    yield f"{path.name}!/{member.as_posix()}", _decode_csa(archive.read(name))
                elif member.suffix.lower() == ".kif":
                    yield f"{path.name}!/{member.as_posix()}", _kif_to_csa(archive.read(name))
        return
    if suffix in {".7z", ".lzh"}:
        executable = (
            shutil.which("7z")
            or shutil.which("7z.exe")
            or (r"C:\Program Files\7-Zip\7z.exe" if pathlib.Path(r"C:\Program Files\7-Zip\7z.exe").is_file() else None)
        )
        with tempfile.TemporaryDirectory() as temporary_directory:
            if executable is not None:
                result = subprocess.run(
                    [executable, "x", str(path), f"-o{temporary_directory}", "-y"],
                    capture_output=True, text=True, encoding="utf-8", errors="replace",
                )
                if result.returncode != 0:
                    raise CollectionError(f"7z extraction failed: {result.stderr or result.stdout}")
            elif suffix == ".7z":
                try:
                    import py7zr
                except ImportError as error:
                    raise CollectionError("7z executable or py7zr is required") from error
                with py7zr.SevenZipFile(path) as archive:
                    archive.extractall(temporary_directory)
            else:
                raise CollectionError("7z executable is required for LZH input")
            root = pathlib.Path(temporary_directory)
            for child in sorted(item for item in root.rglob("*") if item.suffix.lower() in {".csa", ".kif"}):
                relative_name = child.relative_to(root).as_posix()
                if not included(relative_name):
                    continue
                text = _decode_csa(child.read_bytes()) if child.suffix.lower() == ".csa" else _kif_to_csa(child.read_bytes())
                yield f"{path.name}!/{relative_name}", text
        return
    raise CollectionError(f"unsupported corpus input: {path}")
