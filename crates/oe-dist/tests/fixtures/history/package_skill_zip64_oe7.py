# Frozen historical ZIP64 interoperability oracle, test-only.
# Original function bodies are byte-for-byte excerpts from the v1 oe.7 release.
# This file has no CLI, production imports, providers or default execution.
# Source identity and exact excerpt SHA/lines: adjacent .source.json.
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import tempfile
import zipfile


LOCAL_SOURCE_ROOT = "sources"


LOCAL_ONLY_ROOTS = frozenset({LOCAL_SOURCE_ROOT, "var"})


CHUNK_BYTES = 1024 * 1024


class BundleError(ValueError):
    pass


def safe_relative(value: str) -> str:
    if not isinstance(value, str) or not value or "\\" in value:
        raise BundleError("invalid relative path")
    path = PurePosixPath(value)
    if path.is_absolute() or any(x in {"", ".", ".."} for x in value.split("/")):
        raise BundleError("path must remain inside its package")
    if ":" in value or any(ord(c) < 32 for c in value):
        raise BundleError("invalid path characters")
    return path.as_posix()


def file_sha256(path: Path) -> str:
    """Hash bounded chunks, including multi-gigabyte catalog and ZIP files."""
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(CHUNK_BYTES), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_archive(root: Path, files: list[Path], output: Path,
                  expected_hashes: dict[str, str] | None = None) -> dict:
    """Stream a ZIP64 snapshot, publishing only after every byte matches its lock.

    The manifest hashes the bytes actually written. Explicit expected hashes
    bind a candidate stage; other callers freeze them before writing. A source
    changed during packing fails closed and the partial archive is removed.
    """
    root = root.resolve()
    output = output.resolve()
    if output.exists():
        raise ValueError("output must be a new path")
    if output.is_relative_to(root) and not output.is_relative_to(root / "var"):
        raise ValueError("output inside the source skill must be under var")
    ordered = sorted(files, key=lambda p: p.relative_to(root).as_posix())
    names = [safe_relative(p.relative_to(root).as_posix()) for p in ordered]
    if any(name.split("/", 1)[0] in LOCAL_ONLY_ROOTS for name in names):
        raise ValueError("local sources or private state must not be included in the core archive")
    if len(set(names)) != len(names):
        raise ValueError("duplicate archive member")
    expected = expected_hashes if expected_hashes is not None else {
        name: file_sha256(path) for name, path in zip(names, ordered)
    }
    if set(expected) != set(names):
        raise ValueError("expected archive inventory differs from stage")
    output.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(prefix=output.name + ".partial-", dir=output.parent)
    os.close(handle)
    pending = Path(temporary)
    entries = []
    try:
        with zipfile.ZipFile(pending, "w", zipfile.ZIP_DEFLATED, allowZip64=True) as archive:
            for name, path in zip(names, ordered):
                if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(root):
                    raise ValueError("archive source is not a regular in-root file: " + name)
                before = path.stat()
                member = zipfile.ZipInfo("ontology-engineering/" + name)
                # PDF/wheel/ZIP payloads already compress well; avoid spending
                # minutes recompressing several GB without improving integrity.
                member.compress_type = zipfile.ZIP_STORED if path.suffix.lower() in {".pdf", ".whl", ".zip", ".xz"} else zipfile.ZIP_DEFLATED
                member.external_attr = (stat.S_IFREG | (before.st_mode & 0o777)) << 16
                member.file_size = before.st_size
                digest = hashlib.sha256()
                count = 0
                with path.open("rb") as source, archive.open(member, "w", force_zip64=True) as target:
                    for chunk in iter(lambda: source.read(CHUNK_BYTES), b""):
                        target.write(chunk)
                        digest.update(chunk)
                        count += len(chunk)
                after = path.stat()
                if (digest.hexdigest() != expected[name] or count != before.st_size
                        or (before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_ino, after.st_size, after.st_mtime_ns)):
                    raise ValueError("archive source changed since frozen hash: " + name)
                entries.append({"path": name, "sha256": digest.hexdigest(), "bytes": count})
            manifest = {"format": "ontology-engineering.portable-skill/v1", "entry": "SKILL.md",
                        "scope": "Transport integrity only; release authorization is recorded separately.", "files": entries}
            member = zipfile.ZipInfo("ontology-engineering/PORTABLE-MANIFEST.json")
            member.compress_type = zipfile.ZIP_DEFLATED
            member.external_attr = (stat.S_IFREG | 0o644) << 16
            archive.writestr(member, json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        # Exclusive publication: never replace an existing delivery.
        os.link(pending, output)
    finally:
        pending.unlink(missing_ok=True)
    return {"file": str(output), "sha256": file_sha256(output), "bytes": output.stat().st_size,
            "format": "ZIP64", "payload_files": len(entries)}
