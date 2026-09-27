#!/usr/bin/env python3
"""Import and verify the source-bound catalog bundle without external programs."""
from __future__ import annotations

import argparse
import hashlib
import json
import lzma
import os
from pathlib import Path, PurePosixPath
import shutil
import sqlite3
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from ontology_engineering.misumi.catalog import CATALOG_CODES, connect, metadata
from ontology_engineering.misumi.source_pack import load_pack, local_path, check_layout
from ontology_engineering.misumi.paths import default_data_root, runtime_config
from ontology_engineering.local_paths import skill_path

SCHEMA = "ontology-engineering.misumi-data/v1"


def register(archive, index, target):
    """Bind existing local source bytes relative to the final registration root."""
    if (target / "source.json").exists():
        raise ValueError("source_registration_already_exists")
    _, identity = required_files(archive, index)
    location = {"schema": "ontology-engineering.misumi-source-location/v1",
                "archive_root": os.path.relpath(archive.resolve(), target.resolve()),
                "index_path": os.path.relpath(index.resolve(), target.resolve()),
                "index_sha256": file_hash(index), "source_catalogs": identity,
                "verification": "source_mapping_checked; source_bytes_checked_at_use_or_verify"}
    target.mkdir(parents=True, exist_ok=True)
    with (target / "source.json").open("x") as stream:
        json.dump(location, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    (target / "source.json").chmod(0o600)
    return {"status": "registered", "copied_source_files": 0, "source_catalogs": identity}


def _copy_bound(source, destination, identity):
    if (not source.is_file() or source.is_symlink() or source.stat().st_size != identity["bytes"]
            or file_hash(source) != identity["sha256"]):
        raise ValueError("downloaded_file_identity_mismatch:" + identity["path"])
    destination.parent.mkdir(parents=True, exist_ok=True)
    try:
        destination.hardlink_to(source)
    except OSError:
        shutil.copyfile(source, destination)
    if file_hash(destination) != identity["sha256"]:
        raise ValueError("installed_file_changed:" + identity["path"])


def _extract_bound(archive, root, expected, *, label):
    """One streaming extractor for declared regular files, never extractall."""
    seen = set()
    with tarfile.open(archive, "r:xz") as tar:
        for member in tar:
            name = member.name
            path = local_path(root, name)
            if (name not in expected or name in seen or not member.isfile() or member.issparse()
                    or member.size != expected[name]["bytes"]):
                raise ValueError(label + "_member_mismatch")
            seen.add(name)
            path.parent.mkdir(parents=True, exist_ok=True)
            hashed, count = hashlib.sha256(), 0
            with tar.extractfile(member) as source, path.open("xb") as out:
                for chunk in iter(lambda: source.read(2 ** 20), b""):
                    count += len(chunk)
                    if count > expected[name]["bytes"]:
                        raise ValueError(label + "_member_size_mismatch")
                    hashed.update(chunk)
                    out.write(chunk)
            if count != expected[name]["bytes"] or hashed.hexdigest() != expected[name]["sha256"]:
                raise ValueError(label + "_member_hash_mismatch")
    if seen != set(expected):
        raise ValueError(label + "_inventory_mismatch")


def _full_archive(manifest):
    entry = manifest.get("full_pdfs_archive")
    if entry is None:
        return None, {}
    files = entry.get("files", [])
    expected = {e["path"]: e for e in files}
    assemblies = {e.get("download_path"): e for e in manifest["assembled_pdfs"]}
    if not files or len(expected) != len(files) or set(expected) != set(assemblies):
        raise ValueError("full_pdf_archive_inventory_mismatch")
    for name, item in expected.items():
        if (not isinstance(name, str) or not name.startswith("full/")
                or item["sha256"] != assemblies[name]["sha256"] or item["bytes"] != assemblies[name]["bytes"]):
            raise ValueError("full_pdf_archive_assembly_mismatch")
    return entry, expected


def _verify_bound_file(path, entry, label):
    if (not path.is_file() or path.stat().st_size != entry["bytes"] or file_hash(path) != entry["sha256"]):
        raise ValueError(label + "_hash_mismatch")


def install_pack(downloads, target):
    """Install local downloaded shards, leaving all source pages compressed."""
    if target.exists():
        raise ValueError("data_root_already_exists; use a fresh destination")
    manifest = load_pack(downloads)
    meta = manifest["metadata"]
    archive = local_path(downloads, meta["path"])
    if archive.stat().st_size != meta["bytes"] or file_hash(archive) != meta["sha256"]:
        raise ValueError("metadata_archive_hash_mismatch")
    expected = {}
    for entry in meta["files"]:
        name = entry["path"]
        local_path(downloads, name)
        if (name in expected or not (name == "catalog.sqlite3" or name.startswith("archive/") and name.endswith(".json"))
                or type(entry["bytes"]) is not int or not 0 <= entry["bytes"] <= 128 * 1024 * 1024):
            raise ValueError("metadata_inventory_invalid")
        expected[name] = entry
    if expected.get("catalog.sqlite3") != manifest["index"] or sum(e["bytes"] for e in expected.values()) > 256 * 1024 * 1024:
        raise ValueError("metadata_index_identity_mismatch")
    target.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".pack-install-", dir=target.parent))
    try:
        _extract_bound(archive, staging, expected, label="metadata")
        for segment in manifest["segments"]:
            _copy_bound(local_path(downloads, segment["path"]), local_path(staging, segment["path"]), segment)
        full_archive, _ = _full_archive(manifest)
        if full_archive:
            _copy_bound(local_path(downloads, full_archive["path"]), local_path(staging, full_archive["path"]), full_archive)
        # Full books are optional local downloads; do not fetch the network or
        # extract every page merely to make a local citation appear available.
        installed_full = 0
        for entry in manifest.get("assembled_pdfs", []):
            if not full_archive and entry.get("download_path"):
                source = local_path(downloads, entry["download_path"])
                if source.is_file():
                    _copy_bound(source, local_path(staging, "cache/archive/" + entry["path"]), entry)
                    installed_full += 1
        (staging / "cache/archive").mkdir(parents=True, exist_ok=True)
        shutil.copyfile(downloads / "source-pack.json", staging / "source-pack.json")
        if load_pack(staging) != manifest or check_layout(staging, manifest, verify_segments=True):
            raise ValueError("installed_source_pack_invalid")
        closure = verify_pack_closure(staging, manifest)
        staging.rename(target)
        return {"status": "installed", "segments": len(manifest["segments"]), "pages_available": closure["pages"],
                "pages_expanded": 0, "full_pdfs_installed": installed_full,
                "full_pdfs_archive_preserved": bool(full_archive),
                "storage": "compressed_shards_plus_index; selected_pages_restored_at_use"}
    finally:
        if staging.exists():
            shutil.rmtree(staging)


def restore_books(target):
    """Explicitly restore the original full books; ordinary queries never do so."""
    manifest = load_pack(target)
    verify_pack_closure(target, manifest)
    entry, expected = _full_archive(manifest)
    if entry is None:
        raise ValueError("full_pdf_archive_not_declared")
    archive = local_path(target, entry["path"])
    _verify_bound_file(archive, entry, "full_pdf_archive")
    destinations = {e["download_path"]: local_path(target, "cache/archive/" + e["path"])
                    for e in manifest["assembled_pdfs"]}
    for name, path in destinations.items():
        if path.exists():
            _verify_bound_file(path, expected[name], "existing_full_pdf")
    if all(path.is_file() for path in destinations.values()):
        return {"status": "verified", "restored": 0, "full_pdfs": [str(p) for p in destinations.values()]}
    staging = Path(tempfile.mkdtemp(prefix=".restore-books-", dir=target))
    try:
        _extract_bound(archive, staging, expected, label="full_pdf")
        restored = 0
        for name, destination in destinations.items():
            destination.parent.mkdir(parents=True, exist_ok=True)
            try:
                os.link(local_path(staging, name), destination)
                restored += 1
            except FileExistsError:
                _verify_bound_file(destination, expected[name], "existing_full_pdf")
        return {"status": "restored", "restored": restored, "full_pdfs": [str(p) for p in destinations.values()]}
    finally:
        shutil.rmtree(staging)


def safe_path(root, relative):
    if not isinstance(relative, str) or not relative:
        raise ValueError("invalid_relative_data_path")
    value = PurePosixPath(relative)
    if ("\\" in relative
            or value.is_absolute() or ".." in value.parts or value.as_posix() != relative):
        raise ValueError("invalid_relative_data_path")
    path = root / relative
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("data_path_escape")
    if any(p.is_symlink() for p in [path, *path.parents] if p != root.parent):
        raise ValueError("data_symlink_not_portable")
    return path


def file_hash(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(2 ** 20), b""):
            h.update(block)
    return h.hexdigest()


def required_files(archive, index, *, check_page_files=True):
    """Use source manifests and the DB as mutually checked identities, not globs."""
    db = connect(index)
    try:
        if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise ValueError("index_integrity_failure")
        meta = metadata(db)
        files = [(index, "catalog.sqlite3", None, "index")]
        identities = []
        for code in CATALOG_CODES:
            cat = db.execute("SELECT * FROM catalogs WHERE code=?", (code,)).fetchone()
            if cat is None:
                raise ValueError("missing_catalog")
            prefix = f"tmp/pdfs/{code}"
            meta_rel = f"{prefix}/metadata.json"
            manifest_rel = f"下载记录/{code}-manifest.json"
            assembly_rel = f"下载记录/{code}-assembly.json"
            ebook = json.loads(safe_path(archive, meta_rel).read_text())["ebook"]
            manifest = json.loads(safe_path(archive, manifest_rel).read_text())
            assembly = json.loads(safe_path(archive, assembly_rel).read_text())
            if (ebook["catalogCode"] != code or assembly["book"] != code
                    or len(manifest) != cat["page_count"]
                    or len(ebook["pages"]["info"]) != cat["page_count"]
                    or assembly["pages"] != cat["page_count"]
                    or assembly["file"] != cat["pdf_relative_path"]
                    or assembly["sha256"] != cat["pdf_sha256"]):
                raise ValueError("source_catalog_identity_mismatch")
            for rel, sha, role in [
                (meta_rel, cat["metadata_sha256"], "ebook_metadata"),
                (manifest_rel, cat["manifest_sha256"], "page_manifest"),
                (assembly_rel, None, "assembly_record"),
                (assembly["file"], cat["pdf_sha256"], "assembled_pdf"),
            ]:
                files.append((safe_path(archive, rel), "archive/" + rel, sha, role))
            # Keep referenced QC records when present; they remain historical observations.
            qc = assembly.get("qc_report")
            if qc and (not check_page_files or safe_path(archive, qc).is_file()):
                files.append((safe_path(archive, qc), "archive/" + qc, None, "assembly_qc"))
            rows = {r["pdf_page"]: r for r in db.execute("SELECT * FROM pages WHERE catalog=?", (code,))}
            seen = set()
            for entry in manifest:
                number = entry["page_number"]
                if number in seen or number not in rows:
                    raise ValueError("source_page_mapping_mismatch")
                seen.add(number)
                row = rows[number]
                relative = prefix + "/pages/" + entry["filename"]
                if (relative != row["source_relative_path"] or entry["sha256"] != row["source_sha256"]
                        or entry["bytes"] != row["source_bytes"] or entry["page_id"] != row["page_id"]):
                    raise ValueError("source_page_identity_mismatch")
                source = safe_path(archive, relative)
                if check_page_files and source.stat().st_size != entry["bytes"]:
                    raise ValueError("source_page_size_mismatch")
                files.append((source, "archive/" + relative, entry["sha256"], "source_page_pdf"))
            if len(seen) != len(rows):
                raise ValueError("source_page_count_mismatch")
            identities.append([code, cat["metadata_sha256"], cat["manifest_sha256"], cat["pdf_sha256"]])
        if identities != meta["source_identity"]:
            raise ValueError("index_source_identity_mismatch")
        if len({p for _, p, _, _ in files}) != len(files):
            raise ValueError("duplicate_data_path")
        return files, {"source_identity": identities, "counts": meta["counts"],
                       "index_fingerprint": meta["fingerprint"]}
    finally:
        db.close()


def verify_pack_closure(target, manifest):
    """Reuse the archive/index mapping checks without materializing PDF pages."""
    required, identity = required_files(target / "archive", target / "catalog.sqlite3", check_page_files=False)
    meta = {e["path"]: e for e in manifest["metadata"]["files"]}
    if len(meta) != len(manifest["metadata"]["files"]):
        raise ValueError("duplicate_metadata_path")
    for name, entry in meta.items():
        path = local_path(target, name)
        if not path.is_file() or path.stat().st_size != entry["bytes"] or file_hash(path) != entry["sha256"]:
            raise ValueError("installed_metadata_hash_mismatch:" + name)
    pages = {"archive/" + e["path"]: e for s in manifest["segments"] for e in s["files"]}
    full = {"archive/" + e["path"]: e for e in manifest["assembled_pdfs"]}
    declared = {**meta, **pages, **full}
    if (len(declared) != len(meta) + len(pages) + len(full)
            or set(declared) != {relative for _, relative, _, _ in required}):
        raise ValueError("source_pack_metadata_closure_mismatch")
    for _, relative, expected_sha, _ in required:
        if expected_sha and declared[relative]["sha256"] != expected_sha:
            raise ValueError("source_pack_metadata_index_mismatch:" + relative)
    if "source_catalogs" in manifest and manifest["source_catalogs"] != identity:
        raise ValueError("source_pack_catalog_identity_mismatch")
    with connect(target / "catalog.sqlite3") as db:
        records = db.execute("SELECT source_relative_path,source_sha256,source_bytes FROM pages").fetchall()
        indexed = {r[0]: (r[1], r[2]) for r in records}
        if len(indexed) != len(records) or indexed != {e["path"]: (e["sha256"], e["bytes"]) for e in pages.values()}:
            raise ValueError("source_pack_index_page_mismatch")
        assemblies = {r[0]: (r[1], r[2]) for r in db.execute("SELECT code,pdf_relative_path,pdf_sha256 FROM catalogs")}
        if (len(assemblies) != len(manifest["assembled_pdfs"])
                or assemblies != {e["catalog"]: (e["path"], e["sha256"]) for e in manifest["assembled_pdfs"]}):
            raise ValueError("source_pack_assembled_identity_mismatch")
    return {"pages": len(indexed), "source_catalogs": identity}


def import_data(archive, index, target):
    if target.exists():
        raise ValueError("data_root_already_exists; use a fresh destination")
    files, identity = required_files(archive, index)
    target.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=".data-import-", dir=target.parent))
    try:
        entries = []
        for source, relative, expected, role in files:
            dest = safe_path(staging, relative)
            dest.parent.mkdir(parents=True, exist_ok=True)
            h = hashlib.sha256()
            with source.open("rb") as src, dest.open("xb") as dst:
                for chunk in iter(lambda: src.read(2 ** 20), b""):
                    h.update(chunk)
                    dst.write(chunk)
            sha = h.hexdigest()
            if expected and sha != expected:
                raise ValueError("source_hash_mismatch:" + relative)
            media = {".pdf": "application/pdf", ".json": "application/json",
                     ".sqlite3": "application/vnd.sqlite3"}.get(dest.suffix, "application/octet-stream")
            entries.append({"path": relative, "sha256": sha, "bytes": dest.stat().st_size,
                            "media_type": media, "role": role})
        manifest = {"schema": SCHEMA, "source_catalogs": identity, "files": entries,
                    "scope": "private development candidate; supplier source material retains its rights; no project records or credentials"}
        (staging / "data-manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        # Validate the copied index and records, not just identities observed
        # before copying. Streamed hashes already cover the large PDF bytes.
        verify_source_closure(staging, manifest)
        staging.rename(target)
        return {"status": "imported", "files": len(entries), "bytes": sum(e["bytes"] for e in entries),
                "source_catalogs": identity}
    finally:
        if staging.exists():
            shutil.rmtree(staging)


def verify_source_closure(target, manifest):
    seen = set()
    for entry in manifest["files"]:
        relative = entry["path"]
        if relative in seen:
            raise ValueError("duplicate_data_path")
        seen.add(relative)
    required, identity = required_files(target / "archive", target / "catalog.sqlite3")
    if seen != {relative for _, relative, _, _ in required} or identity != manifest["source_catalogs"]:
        raise ValueError("data_manifest_closure_mismatch")
    by_path = {entry["path"]: entry for entry in manifest["files"]}
    for _, relative, expected, _ in required:
        if expected and by_path[relative]["sha256"] != expected:
            raise ValueError("data_source_hash_mismatch:" + relative)
    return {"status": "verified", "files": len(seen), "source_catalogs": identity}


def verify(target):
    if (target / "source-pack.json").is_file():
        manifest = load_pack(target)
        problems = check_layout(target, manifest, verify_segments=True)
        if problems:
            raise ValueError("compressed_source_pack_verification_failed:" + problems[0]["kind"])
        verify_pack_closure(target, manifest)
        full_archive, _ = _full_archive(manifest)
        if full_archive:
            _verify_bound_file(local_path(target, full_archive["path"]), full_archive, "full_pdf_archive")
        return {"status": "verified", "segments": len(manifest["segments"]),
                "scope": "metadata_index_and_compressed_shard_closure; requested_page_bytes_reverified_at_use",
                "expanded_all_pages": False}
    if (target / "source.json").is_file():
        location = json.loads((target / "source.json").read_text())
        if location.get("schema") != "ontology-engineering.misumi-source-location/v1":
            raise ValueError("invalid_source_registration")
        archive, index = (target / location["archive_root"]).resolve(), (target / location["index_path"]).resolve()
        if file_hash(index) != location["index_sha256"]:
            raise ValueError("registered_index_changed")
        files, identity = required_files(archive, index)
        if identity != location["source_catalogs"]:
            raise ValueError("registered_source_identity_changed")
        for source, relative, expected, _ in files:
            if expected and file_hash(source) != expected:
                raise ValueError("source_hash_mismatch:" + relative)
        return {"status": "verified", "files": len(files), "source_catalogs": identity,
                "storage": "registered_source_archive; no copied PDFs in skill"}
    manifest = json.loads((target / "data-manifest.json").read_text())
    if manifest.get("schema") != SCHEMA:
        raise ValueError("data_manifest_schema_mismatch")
    for entry in manifest["files"]:
        relative = entry["path"]
        path = safe_path(target, relative)
        if path.stat().st_size != entry["bytes"] or file_hash(path) != entry["sha256"]:
            raise ValueError("data_hash_mismatch:" + relative)
    return verify_source_closure(target, manifest)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    subs = parser.add_subparsers(dest="command", required=True)
    imp = subs.add_parser("import", help="import the documented ebook archive + matching source-bound SQLite")
    imp.add_argument("--archive", type=Path, required=True)
    imp.add_argument("--index", type=Path, required=True)
    imp.add_argument("--data-root", type=Path, default=default_data_root())
    reg = subs.add_parser("register", help="register an existing archive/index without copying PDFs")
    reg.add_argument("--archive", type=Path, required=True)
    reg.add_argument("--index", type=Path, required=True)
    reg.add_argument("--data-root", type=Path, default=default_data_root())
    pack = subs.add_parser("install", help="install local downloaded source shards without expanding all PDFs")
    pack.add_argument("--from", dest="downloads", type=Path, default=ROOT / "sources/misumi")
    pack.add_argument("--data-root", type=Path, default=default_data_root())
    books = subs.add_parser("restore-books", help="explicitly restore and hash-check both full original PDFs")
    books.add_argument("--data-root", type=Path, default=default_data_root())
    check = subs.add_parser("verify", help="stream-hash all data and check source/index closure")
    check.add_argument("--data-root", type=Path, default=default_data_root())
    args = parser.parse_args(argv)
    try:
        args.data_root = skill_path(args.data_root, "misumi_data_root", root=ROOT)
        if (args.data_root / "source.json").is_file():
            config = runtime_config(args.data_root)
            for name in ("archive_root", "index_path"):
                skill_path(config[name], "misumi_" + name, root=ROOT)
        if args.command == "register":
            args.archive = skill_path(args.archive, "misumi_archive", root=ROOT)
            args.index = skill_path(args.index, "misumi_index", root=ROOT)
        if args.command in ("import", "register"):
            action = register if args.command == "register" else import_data
            result = action(args.archive.resolve(), args.index.resolve(), args.data_root.resolve())
        elif args.command == "install":
            result = install_pack(args.downloads.resolve(), args.data_root.resolve())
        elif args.command == "restore-books":
            result = restore_books(args.data_root.resolve())
        else:
            result = verify(args.data_root.resolve())
    except (OSError, ValueError, KeyError, TypeError, sqlite3.Error, tarfile.TarError, lzma.LZMAError) as exc:
        print(json.dumps({"status": "not_ready", "error": str(exc)}, ensure_ascii=False))
        return 2
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
