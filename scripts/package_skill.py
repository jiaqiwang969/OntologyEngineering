#!/usr/bin/env python3
"""Check and package a relocatable ontology-engineering skill, without caches.

This is a filesystem/transport check, not a semantic verifier or publication
approval. Semantic payloads are checked against their immutable transport locks.
"""

from __future__ import annotations

import argparse
from html import unescape
import hashlib
import json
import lzma
import os
from pathlib import Path
import re
import stat
import tarfile
import tempfile
import sys
from urllib.parse import unquote, urlsplit
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "scripts"))
from check_public_privacy import content_findings, path_findings
from semantic_bundle_transport import load_bundle, safe_relative, validate_data_archive, _regular_inside
from ontology_engineering.local_paths import private_path

DIRECTORIES = {"agents", "demos", "distribution", "docs", "examples", "ontology_engineering", "references", "runtime", "scripts", "skills", "tests", ".github"}
ROOT_FILES = {"SKILL.md", "README.md", "README.en.md", "VERSION", "LICENSE", ".gitignore", "CANDIDATE-STATUS.json"}
# Downloaded books and catalogs may live beside the installed skill. They are
# outside the immutable core payload, even when explicitly passed to the writer.
LOCAL_SOURCE_ROOT = "sources"
LOCAL_ONLY_ROOTS = frozenset({LOCAL_SOURCE_ROOT, "var"})
SKIP_DIRS = {".git", ".venv", "__pycache__", ".pytest_cache", ".ruff_cache", "node_modules", "local-builds"}
SKIP_SUFFIXES = {".pyc", ".aux", ".fdb_latexmk", ".fls", ".log", ".out", ".toc", ".xdv"}


CHUNK_BYTES = 1024 * 1024


def build_directory(root: Path | None = None) -> Path:
    """Keep temporary packaging work in the owning skill's private tree."""
    base = Path(root if root is not None else ROOT).resolve()
    directory = private_path(base / "var/builds", "build_directory", root=base)
    directory.mkdir(parents=True, exist_ok=True)
    return directory


def new_output_path(path: Path, root: Path | None = None) -> Path:
    base = Path(root if root is not None else ROOT).resolve()
    output = private_path(path, "output", root=base)
    if output.exists():
        raise ValueError("output_must_be_new")
    return output


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


SOURCE_PACK_PREFIX = "runtime/misumi/source-pack"


def source_pack_inventory(directory: Path, *, verify_hashes: bool = True) -> tuple[list[dict], dict]:
    """Select only manifest-declared compressed resources, not a directory glob."""
    from ontology_engineering.misumi.source_pack import load_pack, local_path
    directory = directory.resolve()
    manifest = load_pack(directory)
    # The integrated delivery includes both original pages and the assembled
    # books. An unfinished download-folder manifest cannot masquerade as it.
    components = [(manifest["metadata"], "metadata_archive")]
    components += [(entry, "page_segment") for entry in manifest["segments"]]
    components.append((manifest["full_pdfs_archive"], "full_pdfs_archive"))
    manifest_file = local_path(directory, "source-pack.json")
    entries = [{"path": "source-pack.json", "sha256": file_sha256(manifest_file),
                "bytes": manifest_file.stat().st_size, "role": "source_pack_manifest"}]
    seen = {"source-pack.json"}
    for value, role in components:
        name = safe_relative(value["path"])
        if (name in seen or not name.endswith(".tar.xz")
                or not re.fullmatch(r"[0-9a-f]{64}", value["sha256"])
                or type(value["bytes"]) is not int or value["bytes"] < 0):
            raise ValueError("invalid compressed source asset")
        seen.add(name)
        path = local_path(directory, name)
        if not path.is_file() or path.stat().st_size != value["bytes"]:
            raise ValueError("compressed source file missing or size mismatch: " + name)
        if verify_hashes and file_sha256(path) != value["sha256"]:
            raise ValueError("compressed source file hash mismatch: " + name)
        entries.append({"path": name, "sha256": value["sha256"], "bytes": value["bytes"], "role": role})
    return entries, manifest


def compressed_metadata_findings(directory: Path, manifest: dict) -> list[dict]:
    """Inspect the bounded metadata archive, including live SQLite TEXT values.

    Original page/full-book PDFs remain controlled binary sources. We do not
    expand the entire corpus or call PDF data a completed privacy investigation.
    """
    from ontology_engineering.misumi.source_pack import local_path
    spec = manifest["metadata"]
    expected = {}
    for entry in spec["files"]:
        name = safe_relative(entry["path"])
        local_path(directory, name)
        if (name in expected or not (name == "catalog.sqlite3" or name.startswith("archive/") and name.endswith(".json"))
                or type(entry["bytes"]) is not int or not 0 <= entry["bytes"] <= 128 * 1024 * 1024
                or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])):
            raise ValueError("invalid source pack metadata inventory")
        expected[name] = entry
    if (expected.get("catalog.sqlite3") != manifest["index"]
            or sum(e["bytes"] for e in expected.values()) > 256 * 1024 * 1024):
        raise ValueError("source pack metadata index identity mismatch")
    findings, seen = [], set()
    with tempfile.TemporaryDirectory(prefix="oe-packed-metadata-audit-", dir=build_directory()) as temporary:
        extracted = Path(temporary).resolve()
        with tarfile.open(local_path(directory, spec["path"]), mode="r:xz") as archive:
            for member in archive:
                name = safe_relative(member.name)
                target = local_path(extracted, name)
                if (name not in expected or name in seen or not member.isfile() or member.issparse()
                        or member.size != expected[name]["bytes"]):
                    raise ValueError("source pack metadata member mismatch")
                seen.add(name)
                target.parent.mkdir(parents=True, exist_ok=True)
                digest = hashlib.sha256()
                count = 0
                stream = archive.extractfile(member)
                if stream is None:
                    raise ValueError("source pack metadata member unreadable")
                with stream, target.open("xb") as output:
                    for chunk in iter(lambda: stream.read(CHUNK_BYTES), b""):
                        count += len(chunk)
                        if count > expected[name]["bytes"]:
                            raise ValueError("source pack metadata size exceeded")
                        digest.update(chunk)
                        output.write(chunk)
                if count != expected[name]["bytes"] or digest.hexdigest() != expected[name]["sha256"]:
                    raise ValueError("source pack metadata byte mismatch")
                findings.extend({"path": SOURCE_PACK_PREFIX + "/" + spec["path"] + ":" + f.path,
                                 "reason": f.rule, "line": f.line}
                                for f in path_findings(target, extracted) + content_findings(target, extracted))
        if seen != set(expected):
            raise ValueError("source pack metadata inventory incomplete")
    return findings


def candidates(root: Path) -> list[Path]:
    result = [root / n for n in ROOT_FILES if (root / n).exists()]
    for folder in sorted(DIRECTORIES):
        if folder in LOCAL_ONLY_ROOTS:
            continue
        start = root / folder
        if start.is_symlink():
            result.append(start)
            continue
        if not start.exists():
            continue
        for current, directories, files in os.walk(start, followlinks=False):
            for name in directories[:]:
                path = Path(current) / name
                if path.is_symlink():
                    result.append(path)
                    directories.remove(name)
                elif name in SKIP_DIRS:
                    directories.remove(name)
            for name in files:
                path = Path(current) / name
                if name == ".DS_Store" or path.suffix in SKIP_SUFFIXES or name.endswith(".synctex.gz"):
                    continue
                # Generated aliases are not controlled book source or named PDF artifacts.
                if path.relative_to(root).as_posix() == "references/ontology-engineering-book/handbook/main.pdf":
                    continue
                result.append(path)
    return sorted(result, key=lambda x: x.relative_to(root).as_posix())


def document_links(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    # Samples in fenced code do not declare content dependencies.
    text = re.sub(r"(?ms)^\s*(```|~~~).*?^\s*\1\s*$", "", text)
    links = re.findall(r"!?\[[^\]\n]*\]\(([^)\n]+)\)", text)
    links += re.findall(r"(?:href|src)\s*=\s*[\"']([^\"']+)[\"']", text, re.I)
    links += re.findall(r"(?m)^\s*\[[^\]]+\]:\s*(\S+)", text)
    clean = []
    for item in links:
        item = unescape(item.strip())
        if item.startswith("<"):
            item = item[1:item.index(">")]
        else:
            item = re.split(r"\s+[\"']", item, maxsplit=1)[0]
        clean.append(item)
    return clean


def delivery_issues(root: Path, files: list[Path]) -> list[dict]:
    """When inspecting an unpacked delivery, check its own exact file snapshot."""
    path = root / "PORTABLE-MANIFEST.json"
    if not path.exists():
        return []
    try:
        if path.is_symlink():
            raise ValueError("delivery manifest is a symbolic link")
        manifest = json.loads(path.read_text())
        if manifest["format"] != "ontology-engineering.portable-skill/v1":
            raise ValueError("unknown delivery manifest format")
        expected = {safe_relative(x["path"]): x["sha256"] for x in manifest["files"]}
        actual = {f.relative_to(root).as_posix(): f for f in files}
        if len(expected) != len(manifest["files"]) or set(expected) != set(actual):
            raise ValueError("delivery file inventory differs from frozen manifest")
        for name, file in actual.items():
            if file.is_symlink() or not file.is_file() or file_sha256(file) != expected[name]:
                raise ValueError("delivery byte mismatch: " + name)
    except (OSError, ValueError, KeyError) as error:
        return [{"path": "PORTABLE-MANIFEST.json", "reason": str(error)}]
    return []


def candidate_issues(root: Path, files: list[Path]) -> list[dict]:
    """Check the private candidate ledger and the complete source data binding."""
    status_path = root / "CANDIDATE-STATUS.json"
    if not status_path.exists():
        return []
    try:
        status = json.loads(status_path.read_text())
        if (status.get("schema") != "ontology-engineering.candidate-status/v1"
                or status.get("status") != "candidate_not_published"
                or status.get("publication_approval") != "not_granted"
                or status.get("ledger") != "distribution/engineering-next-assets.json"):
            raise ValueError("invalid candidate status")
        ledger_path = _regular_inside(root, status["ledger"])
        if file_sha256(ledger_path) != status["ledger_sha256"]:
            raise ValueError("candidate ledger hash mismatch")
        ledger = json.loads(ledger_path.read_text())
        if (ledger.get("format") != "ontology-engineering.engineering-candidate-assets/v1"
                or ledger.get("status") != "candidate_not_published"
                or ledger.get("publication_approval") != "not_granted"):
            raise ValueError("candidate ledger status mismatch")
        expected = {safe_relative(e["path"]): e for e in ledger["files"]}
        names = {p.relative_to(root).as_posix() for p in files}
        if (len(expected) != len(ledger["files"])
                or names != set(expected) | {status["ledger"], "CANDIDATE-STATUS.json"}):
            raise ValueError("candidate inventory differs from frozen ledger")
        # A delivery check has already streamed the archive's complete inventory.
        # Bind its hashes to the candidate instead of hashing GB a second time.
        delivery = root / "PORTABLE-MANIFEST.json"
        delivered = {e["path"]: e["sha256"] for e in json.loads(delivery.read_text())["files"]} if delivery.exists() else None
        for name, entry in expected.items():
            path = _regular_inside(root, name)
            digest = delivered.get(name) if delivered is not None else file_sha256(path)
            if path.stat().st_size != entry["bytes"] or digest != entry["sha256"]:
                raise ValueError("candidate bytes differ from frozen ledger: " + name)
        prefix = "runtime/misumi/data/"
        data_present = any(n.startswith(prefix) or e.get("kind") == "supplier_data" for n, e in expected.items())
        included = ledger.get("source_data_included", data_present)
        if (type(included) is not bool or included != data_present
                or status.get("source_data_included", included) != included):
            raise ValueError("candidate source data scope differs from inventory")
        packed = {name: entry for name, entry in expected.items() if entry.get("kind") == "compressed_source_pack"}
        pack_identity = ledger.get("source_pack")
        if bool(packed) != bool(pack_identity) or status.get("source_pack") != pack_identity:
            raise ValueError("candidate compressed source scope mismatch")
        if packed:
            if included:
                raise ValueError("expanded and compressed source modes cannot be combined")
            entries, manifest = source_pack_inventory(root / SOURCE_PACK_PREFIX, verify_hashes=False)
            mapped = {SOURCE_PACK_PREFIX + "/" + e["path"]: e for e in entries}
            if set(mapped) != set(packed):
                raise ValueError("candidate compressed source inventory mismatch")
            for name, entry in mapped.items():
                if any(entry[k] != packed[name][k] for k in ("sha256", "bytes", "role")):
                    raise ValueError("candidate compressed source identity mismatch")
            identity = {"manifest_sha256": mapped[SOURCE_PACK_PREFIX + "/source-pack.json"]["sha256"],
                        "files": len(entries), "bytes": sum(e["bytes"] for e in entries),
                        "target": SOURCE_PACK_PREFIX}
            if identity != pack_identity:
                raise ValueError("candidate compressed source manifest binding mismatch")
            found = compressed_metadata_findings(root / SOURCE_PACK_PREFIX, manifest)
            if found:
                return found
        elif any(name.startswith(SOURCE_PACK_PREFIX + "/") for name in expected):
            raise ValueError("compressed source assets are not declared")
        if not included:
            if ledger.get("source_catalogs") is not None:
                raise ValueError("lightweight candidate cannot assert bundled source catalogs")
            return []
        data = json.loads(_regular_inside(root, prefix + "data-manifest.json").read_text())
        if data.get("schema") != "ontology-engineering.misumi-data/v1":
            raise ValueError("unknown candidate data manifest")
        data_files = {prefix + safe_relative(e["path"]): e for e in data["files"]}
        if len(data_files) != len(data["files"]) or set(data_files) != {n for n, e in expected.items() if e["kind"] == "supplier_data"}:
            raise ValueError("candidate data inventory differs from source manifest")
        for name, entry in data_files.items():
            if any(entry[k] != expected[name][k] for k in ("sha256", "bytes", "role", "media_type")):
                raise ValueError("candidate data identity mismatch: " + name)
        if data["source_catalogs"] != ledger["source_catalogs"]:
            raise ValueError("candidate source identity mismatch")
    except (OSError, ValueError, KeyError, TypeError, tarfile.TarError, lzma.LZMAError, EOFError) as error:
        return [{"path": "CANDIDATE-STATUS.json", "reason": str(error)}]
    return []


def check(root: Path) -> tuple[dict, list[Path]]:
    root = root.resolve()
    files = candidates(root)
    names = {f.relative_to(root).as_posix() for f in files}
    issues = delivery_issues(root, files)
    issues += candidate_issues(root, files)
    links = 0
    required = {"SKILL.md", "runtime/semantica-source-lock.json", "runtime/semantic-bundles.json",
                "skills/manufacturing-process-cost/SKILL.md", "scripts/run_manufacturing_cases.py"}
    for missing in sorted(required - names):
        issues.append({"path": missing, "reason": "required file missing"})
    for file in files:
        relative = file.relative_to(root).as_posix()
        found = path_findings(file, root)
        if not file.is_symlink() and file.is_file():
            found += content_findings(file, root)
        issues.extend({"path": x.path, "reason": x.rule, "line": x.line} for x in found)
        if found or file.suffix not in {".md", ".html"}:
            continue
        for target in document_links(file):
            parsed = urlsplit(target)
            if parsed.scheme or target.startswith("//") or not parsed.path:
                continue
            links += 1
            # Override documents are authored for their declared staged location.
            # Carrying the distribution sources makes a full delivery rebuildable;
            # the core builder still checks the exact staged inventory and bytes.
            location = file
            for template_root in ("distribution/shareable-overrides", "distribution/engineering-next-overrides"):
                try:
                    override = file.relative_to(root / template_root)
                except ValueError:
                    continue
                location = root / override
                break
            destination = (location.parent / unquote(parsed.path)).resolve()
            if not destination.is_relative_to(root):
                issues.append({"path": relative, "reason": "link escapes skill root", "target": target})
            elif not destination.exists():
                issues.append({"path": relative, "reason": "missing local link", "target": target})
            elif destination.is_file() and destination.relative_to(root).as_posix() not in names:
                issues.append({"path": relative, "reason": "link targets excluded file", "target": target})
    bundle_reports = []
    bootstrap_reports = []
    try:
        locks = json.loads((root / "runtime/semantic-bundles.json").read_text())
        for name in locks["bundles"]:
            _, payload, report = load_bundle(name, root)
            # Audit expanded payload as text too; ZIP itself is not a privacy exception.
            from check_public_privacy import CONTENT_RULES
            for member, value in payload.items():
                text = value.decode("utf-8")
                for rule in CONTENT_RULES:
                    if rule.pattern.search(text):
                        issues.append({"path": name + ":" + member, "reason": rule.name})
            bundle_reports.append({k: v for k, v in report.items() if k != "scenarios"} | {"scenario_count": len(report["scenarios"])})
        lock = json.loads((root / "runtime/semantica-source-lock.json").read_text())
        if "ontology_engineering/method_bootstrap.py" in names:
            bootstrap = json.loads(_regular_inside(root, "runtime/semantic-bootstrap.json").read_text())
            if bootstrap["schema"] != "ontology-engineering.method-bootstrap-lock/v1":
                raise ValueError("unknown bootstrap lock")
            for name, spec in bootstrap["capsules"].items():
                bundle = locks["bundles"][name]
                if any(spec[k] != bundle[k] for k in ("package_id", "package_version", "package_sha256", "runtime")):
                    raise ValueError("bootstrap and bundle configuration differ")
                payload = validate_data_archive(_regular_inside(root, spec["path"]).read_bytes(), spec)
                for member, value in payload.items():
                    for rule in CONTENT_RULES:
                        if rule.pattern.search(value.decode("utf-8")):
                            issues.append({"path": name + ":bootstrap:" + member, "reason": rule.name})
                bootstrap_reports.append({"name": name, "files": len(payload), "sha256": spec["sha256"],
                                          "scope": "Data transport only; native initialization verified separately."})
        wheel = root / "runtime/vendor" / lock["artifact"]["filename"]
        if file_sha256(wheel) != lock["artifact"]["sha256"]:
            raise ValueError("vendored runtime wheel hash mismatch")
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        issues.append({"path": "runtime", "reason": str(error)})
    report = {"passed": not issues, "files": len(files), "local_links_checked": links,
              "bundles": bundle_reports, "bootstrap": bootstrap_reports, "issues": issues,
              "scope": "File closure, direct-identifier scan and frozen package hashes; semantic execution and human publication authority are separate."}
    return report, files


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="Skill root to inspect; defaults to this script's parent root.")
    parser.add_argument("--output", type=Path, help="New ZIP inside this skill's var/ (normally var/builds/); omit for check only.")
    args = parser.parse_args(argv)
    root = args.root.expanduser().resolve()
    try:
        output = new_output_path(args.output) if args.output else None
    except ValueError as error:
        parser.error(str(error))
    report, files = check(root)
    if report["passed"] and output:
        report["archive"] = write_archive(root, files, output)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
