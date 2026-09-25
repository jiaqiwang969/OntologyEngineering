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
import os
from pathlib import Path
import re
import stat
import sys
from urllib.parse import unquote, urlsplit
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "scripts"))
from check_public_privacy import content_findings, path_findings
from semantic_bundle_transport import load_bundle, safe_relative, validate_data_archive, _regular_inside

DIRECTORIES = {"agents", "demos", "distribution", "docs", "examples", "ontology_engineering", "references", "runtime", "scripts", "skills", "tests", ".github"}
ROOT_FILES = {"SKILL.md", "README.md", "README.en.md", "VERSION", "LICENSE", ".gitignore"}
SKIP_DIRS = {".git", ".venv", "__pycache__", ".pytest_cache", ".ruff_cache", "node_modules", "local-builds"}
SKIP_SUFFIXES = {".pyc", ".aux", ".fdb_latexmk", ".fls", ".log", ".out", ".toc", ".xdv"}


def candidates(root: Path) -> list[Path]:
    result = [root / n for n in ROOT_FILES if (root / n).exists()]
    for folder in sorted(DIRECTORIES):
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
            if file.is_symlink() or not file.is_file() or hashlib.sha256(file.read_bytes()).hexdigest() != expected[name]:
                raise ValueError("delivery byte mismatch: " + name)
    except (OSError, ValueError, KeyError) as error:
        return [{"path": "PORTABLE-MANIFEST.json", "reason": str(error)}]
    return []


def check(root: Path) -> tuple[dict, list[Path]]:
    root = root.resolve()
    files = candidates(root)
    names = {f.relative_to(root).as_posix() for f in files}
    issues = delivery_issues(root, files)
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
            try:
                override = file.relative_to(root / "distribution/shareable-overrides")
            except ValueError:
                pass
            else:
                location = root / override
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
        if hashlib.sha256(wheel.read_bytes()).hexdigest() != lock["artifact"]["sha256"]:
            raise ValueError("vendored runtime wheel hash mismatch")
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        issues.append({"path": "runtime", "reason": str(error)})
    report = {"passed": not issues, "files": len(files), "local_links_checked": links,
              "bundles": bundle_reports, "bootstrap": bootstrap_reports, "issues": issues,
              "scope": "File closure, direct-identifier scan and frozen package hashes; semantic execution and human publication authority are separate."}
    return report, files


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT, help="Skill root to inspect; defaults to this script's parent root.")
    parser.add_argument("--output", type=Path, help="New ZIP outside the source tree; omit for check only.")
    args = parser.parse_args()
    root = args.root.expanduser().resolve()
    report, files = check(root)
    if report["passed"] and args.output:
        output = args.output.expanduser().resolve()
        if output.exists() or output.is_relative_to(root):
            parser.error("output must be a new path outside the source skill")
        # Read exact bytes once: each manifest entry hashes the bytes placed in ZIP.
        payload = {f.relative_to(root).as_posix(): f.read_bytes() for f in files}
        manifest = {"format": "ontology-engineering.portable-skill/v1", "entry": "SKILL.md",
                    "scope": "Transport integrity only; release authorization is recorded separately.",
                    "files": [{"path": n, "sha256": hashlib.sha256(b).hexdigest()} for n, b in sorted(payload.items())]}
        output.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(output, "x", zipfile.ZIP_DEFLATED) as archive:
            modes = {f.relative_to(root).as_posix(): f.stat().st_mode & 0o777 for f in files}
            for name, data in sorted(payload.items()):
                member = zipfile.ZipInfo("ontology-engineering/" + name)
                member.compress_type = zipfile.ZIP_DEFLATED
                member.external_attr = (stat.S_IFREG | modes[name]) << 16
                archive.writestr(member, data)
            member = zipfile.ZipInfo("ontology-engineering/PORTABLE-MANIFEST.json")
            member.compress_type = zipfile.ZIP_DEFLATED
            member.external_attr = (stat.S_IFREG | 0o644) << 16
            archive.writestr(member, json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        report["archive"] = {"file": str(output), "sha256": hashlib.sha256(output.read_bytes()).hexdigest(), "bytes": output.stat().st_size}
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
