"""Place and verify declared source bytes beside the skill; reuse its indexers.

Only an explicit fetch uses gws. Queries never download data. This module owns
file placement, not engineering facts, ontology adoption or Drive permissions.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile

from ontology_engineering.local_paths import skill_path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "runtime/source-delivery.json"


def source_path(root, relative):
    p = PurePosixPath(relative)
    if (not isinstance(relative, str) or p.is_absolute() or p.as_posix() != relative
            or ".." in p.parts or "\\" in relative or not p.parts
            or any(ord(c) < 32 for c in relative)):
        raise ValueError("source_library_invalid_relative_path")
    root = Path(root).expanduser().absolute()
    path = root.joinpath(*p.parts)
    if any(x.is_symlink() for x in [root, path, *path.parents] if x == root or root in x.parents):
        raise ValueError("source_library_symlink")
    return path


def load_delivery(path=DEFAULT_MANIFEST):
    document = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(document, dict) or document.get("schema") != "ontology-engineering.source-delivery/v1":
        raise ValueError("source_library_manifest_schema")
    entries = document.get("files")
    if not isinstance(entries, list) or not entries:
        raise ValueError("source_library_empty_manifest")
    seen = set()
    for entry in entries:
        if (not isinstance(entry, dict) or not isinstance(entry.get("drive", {}), dict)
                or not isinstance(entry.get("registration", {}), dict)):
            raise ValueError("source_library_invalid_inventory")
        name = entry["relative_path"]
        source_path(Path(tempfile.gettempdir()) / "oe-path-validation", name)
        if (name in seen or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])
                or type(entry["bytes"]) is not int or entry["bytes"] < 0):
            raise ValueError("source_library_invalid_inventory")
        seen.add(name)
    return document


def select_entries(document, selection):
    if selection == "all":
        return document["files"]
    selected = [e for e in document["files"] if
                (selection in {"books", "misumi"} and e["relative_path"].startswith(selection + "/"))
                or e.get("registration", {}).get("id") == selection
                or e["relative_path"] == selection]
    if not selected:
        raise ValueError("source_library_unknown_selection")
    return selected


def file_status(path, entry):
    if not path.exists():
        return "missing"
    if not path.is_file() or path.is_symlink():
        return "not_regular_file"
    if path.stat().st_size != entry["bytes"]:
        return "size_mismatch"
    hashed = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(2 ** 20), b""):
            hashed.update(chunk)
    return "verified" if hashed.hexdigest() == entry["sha256"] else "sha256_mismatch"


def inspect_sources(entries, root, *, verify=False):
    files = []
    for entry in entries:
        path = source_path(root, entry["relative_path"])
        state = file_status(path, entry) if verify else ("present_unverified" if path.is_file() else "missing")
        files.append({"relative_path": entry["relative_path"], "bytes": entry["bytes"],
                      "status": state, "path": str(path),
                      "download_url": entry.get("drive", {}).get("view_url"),
                      "book_id": entry.get("registration", {}).get("id")})
    return {"status": "verified" if verify and all(f["status"] == "verified" for f in files)
            else "listed" if not verify else "not_ready", "sources_root": str(root),
            "files": files, "bytes": sum(e["bytes"] for e in entries), "network_requests": 0}


def place_sources(entries, root, *, incoming=None):
    """Import verified downloads or fetch explicit Drive IDs without overwriting.

    Browser downloads may be flat; expected basenames must be unique. Preserve
    the input folder. A hardlink avoids copying existing multi-GB archives.
    """
    names = [PurePosixPath(e["relative_path"]).name for e in entries]
    if len(names) != len(set(names)):
        raise ValueError("source_library_ambiguous_download_names")
    reports = []
    for entry in entries:
        destination = source_path(root, entry["relative_path"])
        if destination.exists():
            status = file_status(destination, entry)
            if status != "verified":
                raise ValueError("source_library_existing_file_" + status)
            reports.append({"relative_path": entry["relative_path"], "status": "already_verified"})
            continue
        source = None
        if incoming is not None:
            candidates = [source_path(incoming, entry["relative_path"]),
                          source_path(incoming, PurePosixPath(entry["relative_path"]).name)]
            source = next((p for p in candidates if p.is_file()), None)
            if source is None or file_status(source, entry) != "verified":
                raise ValueError("source_library_download_missing_or_mismatched:" + entry["relative_path"])
        destination.parent.mkdir(parents=True, exist_ok=True)
        fd, name = tempfile.mkstemp(prefix=".incoming-", dir=destination.parent)
        os.close(fd)
        temporary = Path(name)
        try:
            if source is not None:
                temporary.unlink()
                try:
                    temporary.hardlink_to(source)
                except OSError:
                    shutil.copyfile(source, temporary)
            else:
                file_id = entry.get("drive", {}).get("file_id", "")
                if not re.fullmatch(r"[A-Za-z0-9_-]{10,200}", file_id):
                    raise ValueError("source_library_drive_id_invalid")
                # gws owns authentication. Never export, inspect or log tokens.
                try:
                    process = subprocess.run(["gws", "drive", "files", "get", "--params",
                        json.dumps({"fileId": file_id, "alt": "media"}), "--output", str(temporary)],
                        capture_output=True, timeout=1800)
                except FileNotFoundError:
                    raise ValueError("source_library_gws_not_installed") from None
                except subprocess.TimeoutExpired:
                    raise ValueError("source_library_gws_download_timeout") from None
                if process.returncode:
                    raise ValueError("source_library_gws_download_failed_check_login_access_and_network")
            if file_status(temporary, entry) != "verified":
                raise ValueError("source_library_download_identity_mismatch")
            os.link(temporary, destination)
            reports.append({"relative_path": entry["relative_path"], "status": "verified"})
        finally:
            temporary.unlink(missing_ok=True)
    return {"status": "placed", "files": reports, "sources_root": str(root),
            "network_download": "not_run" if incoming is not None else "gws_explicit_fetch",
            "originals_modified": False}


def register_books(entries, root, *, replace=False):
    from .local_pdf_sources import MANIFEST_SCHEMA, register_sources
    books = [e for e in entries if e.get("registration")]
    if not books:
        raise ValueError("source_library_no_book_registration")
    if inspect_sources(books, root, verify=True)["status"] != "verified":
        raise ValueError("source_library_books_not_verified")
    manifest = {"schema": MANIFEST_SCHEMA, "sources": [{
        "id": e["registration"]["id"], "title": e["title"], "edition": e["registration"]["edition"],
        "path": str(source_path(root, e["relative_path"])), "sha256": e["sha256"],
        "chapters": e["registration"]["chapters"],
    } for e in books]}
    target = source_path(root, ".indexes/local-pdf")
    with tempfile.TemporaryDirectory(prefix="oe-book-registration-") as temporary:
        path = Path(temporary) / "manifest.json"
        path.write_text(json.dumps(manifest, ensure_ascii=False), encoding="utf-8")
        path.chmod(0o600)
        return register_sources(path, target, replace=replace, relative_paths=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description="下载原件归入 skill/sources；校验并接入同一知识入口。")
    parser.add_argument("action", choices=("list", "verify", "import", "fetch", "register"))
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--root", type=Path, default=ROOT / "sources", help="默认此 skill 的 sources/")
    parser.add_argument("--select", help="all、books、misumi、书 ID 或清单相对路径；fetch 必须明确选择")
    parser.add_argument("--from", dest="incoming", type=Path, help="浏览器下载解压后的目录；保留原目录")
    parser.add_argument("--replace-index", action="store_true", help="用本次完整选定书目重建现有索引；不是增量追加")
    args = parser.parse_args(argv)
    if args.action == "fetch" and not args.select:
        parser.error("fetch 需要 --select 明确下载范围；先用 list 查看文件与大小")
    if (args.action == "import") != bool(args.incoming):
        parser.error("import 需要 --from；其他操作不接受 --from")
    if args.replace_index and args.action != "register":
        parser.error("--replace-index 仅用于 register")
    try:
        root = skill_path(args.root, "source_root", root=ROOT)
        entries = select_entries(load_delivery(args.manifest), args.select or ("books" if args.action == "register" else "all"))
        if args.action in {"list", "verify"}:
            report = inspect_sources(entries, root, verify=args.action == "verify")
        elif args.action in {"import", "fetch"}:
            report = place_sources(entries, root, incoming=args.incoming)
        else:
            if any(not e.get("registration") for e in entries):
                raise ValueError("source_library_register_select_books_only")
            report = register_books(entries, root, replace=args.replace_index)
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 2 if report["status"] == "not_ready" else 0
    except (OSError, ValueError, KeyError, TypeError) as exc:
        code = str(exc).split(":", 1)[0] if isinstance(exc, ValueError) else "source_library_io_or_manifest_error"
        print(json.dumps({"status": "failed", "error": code}, ensure_ascii=False))
        return 2
