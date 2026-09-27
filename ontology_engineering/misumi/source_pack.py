"""Read local lossless source shards and restore only a requested source page.

No downloading, PDF rewriting or semantic interpretation occurs here.
"""
from __future__ import annotations

import hashlib
import json
import lzma
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import tempfile

from .catalog import file_hash

SCHEMA = "ontology-engineering.misumi-source-pack/v1"
MAX_MANIFEST_BYTES = 16 * 1024 * 1024
MAX_SOURCE_BYTES = 256 * 1024 * 1024
MAX_SEGMENT_MEMBERS = 128


def local_path(root, relative):
    """Canonical relative names; no symlinks anywhere below the selected root."""
    if not isinstance(relative, str) or not relative or "\\" in relative:
        raise ValueError("source_pack_invalid_path")
    rel = PurePosixPath(relative)
    if (not rel.parts or rel.is_absolute() or ".." in rel.parts or rel.as_posix() != relative
            or ":" in relative or any(ord(c) < 32 for c in relative)):
        raise ValueError("source_pack_invalid_path")
    root = Path(root).resolve()
    path = root
    for part in rel.parts:
        path = path / part
        if path.is_symlink():
            raise ValueError("source_pack_symlink_not_allowed")
    if not path.resolve().is_relative_to(root):
        raise ValueError("source_pack_path_escape")
    return path


def _file(entry, root, *, source=False):
    if not isinstance(entry, dict):
        raise ValueError("source_pack_invalid_file")
    local_path(root, entry.get("path"))
    if (not isinstance(entry.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])
            or type(entry.get("bytes")) is not int or entry["bytes"] < 0
            or (source and entry["bytes"] > MAX_SOURCE_BYTES)):
        raise ValueError("source_pack_invalid_file_identity")


def load_pack(root):
    root = Path(root).resolve()
    path = local_path(root, "source-pack.json")
    if not path.is_file() or path.stat().st_size > MAX_MANIFEST_BYTES:
        raise ValueError("source_pack_manifest_missing_or_oversized")
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
        if manifest.get("schema") != SCHEMA:
            raise ValueError("source_pack_schema_mismatch")
        index = manifest["index"]
        _file(index, root)
        if index["path"] != "catalog.sqlite3":
            raise ValueError("source_pack_index_path_mismatch")
        segments = manifest["segments"]
        if not isinstance(segments, list) or not segments:
            raise ValueError("source_pack_empty_segments")
        seen_ids, seen_files, seen_segments = set(), set(), set()
        for segment in segments:
            _file(segment, root)
            if (not isinstance(segment.get("id"), str) or not segment["id"]
                    or segment["id"] in seen_ids or segment["path"] in seen_segments
                    or not segment["path"].startswith("segments/") or not segment["path"].endswith(".tar.xz")
                    or not isinstance(segment.get("files"), list)
                    or not 1 <= len(segment["files"]) <= MAX_SEGMENT_MEMBERS):
                raise ValueError("source_pack_invalid_segment")
            seen_ids.add(segment["id"])
            seen_segments.add(segment["path"])
            for entry in segment["files"]:
                _file(entry, root, source=True)
                if entry["path"] in seen_files:
                    raise ValueError("source_pack_duplicate_member")
                seen_files.add(entry["path"])
        assemblies = manifest.get("assembled_pdfs", [])
        if not isinstance(assemblies, list):
            raise ValueError("source_pack_invalid_assembled_inventory")
        seen = set()
        for entry in assemblies:
            _file(entry, root)
            if entry["path"] in seen or not isinstance(entry.get("catalog"), str):
                raise ValueError("source_pack_invalid_assembled_inventory")
            seen.add(entry["path"])
            url = entry.get("download_url")
            if url is not None and (not isinstance(url, str) or not url.startswith("https://")):
                raise ValueError("source_pack_invalid_download_url")
        return manifest
    except (KeyError, TypeError, AttributeError, json.JSONDecodeError) as exc:
        raise ValueError("source_pack_invalid_manifest") from exc


def pack_for_archive(archive):
    archive = Path(archive).absolute()
    if archive.name == "archive" and archive.parent.name == "cache":
        root = archive.parent.parent
        if (root / "source-pack.json").is_file():
            if local_path(root, "cache/archive") != archive:
                raise ValueError("source_pack_cache_path_mismatch")
            return root
    return None


def check_layout(root, manifest, *, verify_segments=False):
    """No expansion: inspect installed shards and bind the index to the pack."""
    missing = []
    index = local_path(root, manifest["index"]["path"])
    if not index.is_file():
        missing.append({"kind": "index_missing", "path": str(index)})
    elif (index.stat().st_size != manifest["index"]["bytes"]
          or file_hash(index) != manifest["index"]["sha256"]):
        missing.append({"kind": "source_pack_index_changed", "path": str(index)})
    for segment in manifest["segments"]:
        path = local_path(root, segment["path"])
        if not path.is_file():
            missing.append({"kind": "missing_source_segment", "path": str(path), "segment_id": segment["id"]})
        elif path.stat().st_size != segment["bytes"]:
            missing.append({"kind": "source_segment_size_mismatch", "path": str(path), "segment_id": segment["id"]})
        elif verify_segments and file_hash(path) != segment["sha256"]:
            missing.append({"kind": "source_segment_hash_mismatch", "path": str(path), "segment_id": segment["id"]})
    return missing


def restore_page(archive, relative, expected_sha256):
    """Atomically restore one exact original member; never extractall a shard."""
    root = pack_for_archive(archive)
    if root is None:
        return None
    manifest = load_pack(root)
    target = local_path(archive, relative)
    located = [(segment, entry) for segment in manifest["segments"] for entry in segment["files"]
               if entry["path"] == relative]
    if len(located) != 1 or located[0][1]["sha256"] != expected_sha256:
        raise ValueError("source_pack_page_identity_mismatch")
    segment, entry = located[0]
    shard = local_path(root, segment["path"])
    if not shard.is_file():
        raise ValueError("missing_source_segment:" + segment["id"])
    if shard.stat().st_size != segment["bytes"] or file_hash(shard) != segment["sha256"]:
        raise ValueError("source_segment_hash_mismatch:" + segment["id"])
    declared = {item["path"]: item for item in segment["files"]}
    target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    # Recheck components after creating them, before writing any source bytes.
    local_path(root, "cache/archive/" + relative)
    fd, temporary = tempfile.mkstemp(prefix=".restore-", dir=target.parent)
    try:
        with os.fdopen(fd, "wb") as output, tarfile.open(shard, mode="r:xz") as tar:
            seen, recovered = set(), False
            for member in tar:
                local_path(archive, member.name)
                if (member.name not in declared or member.name in seen or not member.isfile()
                        or member.issparse() or member.size != declared[member.name]["bytes"]):
                    raise ValueError("source_segment_member_mismatch")
                seen.add(member.name)
                if member.name != relative:
                    continue
                stream = tar.extractfile(member)
                if stream is None:
                    raise ValueError("source_segment_member_missing")
                hashed, count = hashlib.sha256(), 0
                with stream:
                    for block in iter(lambda: stream.read(2 ** 20), b""):
                        count += len(block)
                        if count > entry["bytes"]:
                            raise ValueError("source_segment_member_size_mismatch")
                        output.write(block)
                        hashed.update(block)
                if count != entry["bytes"] or hashed.hexdigest() != expected_sha256:
                    raise ValueError("source_restored_page_hash_mismatch")
                recovered = True
            if not recovered or seen != set(declared):
                raise ValueError("source_segment_inventory_mismatch")
        # An interrupted extraction never becomes a source citation. Existing
        # files from a simultaneous query are accepted only with the same hash.
        try:
            os.link(temporary, target)
        except FileExistsError:
            if not target.is_file() or file_hash(target) != expected_sha256:
                raise ValueError("source_cache_conflict") from None
    except (tarfile.TarError, EOFError, lzma.LZMAError) as exc:
        raise ValueError("source_segment_invalid_archive") from exc
    finally:
        os.unlink(temporary)
    return target


def assembly_details(archive, catalog):
    """A missing full PDF is a download locator, not a valid file citation."""
    root = pack_for_archive(archive)
    path = local_path(archive, catalog["pdf_relative_path"])
    url = None
    packed_original_present = False
    if root is not None:
        manifest = load_pack(root)
        matches = [item for item in manifest.get("assembled_pdfs", [])
                   if item["catalog"] == catalog["code"] and item["path"] == catalog["pdf_relative_path"]]
        if len(matches) != 1 or matches[0]["sha256"] != catalog["pdf_sha256"]:
            raise ValueError("source_pack_assembled_identity_mismatch")
        url = matches[0].get("download_url")
        packed = manifest.get("full_pdfs_archive")
        if packed:
            packed_original_present = local_path(root, packed["path"]).is_file()
    exists = path.is_file()
    # In the legacy archive this remains the prior observation. Compressed
    # installs verify a separately downloaded full original before citing it.
    verified = exists and (root is None or file_hash(path) == catalog["pdf_sha256"])
    return {"assembled_pdf": str(path), "assembled_relative_path": catalog["pdf_relative_path"],
            "assembled_pdf_exists": exists,
            "assembled_pdf_status": ("verified_present" if root is not None else "present_not_reverified")
                                    if verified else "hash_mismatch" if exists else
                                    "not_restored" if packed_original_present else "not_downloaded",
            "assembled_pdf_uri": path.as_uri() if verified else None,
            "assembled_pdf_download_url": url}
