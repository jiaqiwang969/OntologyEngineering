#!/usr/bin/env python3
"""Freeze and build a private integrated engineering candidate, never a release.

The legacy public list supplies an explicit selection baseline, not approval for
new bytes. --refresh-ledger freezes current reviewed scope; building subsequently
rejects drift. The default is a lightweight core. Source data requires an explicit
opt-in for both freezing and building, and its source-bound per-file manifest. Nothing
installs software, calls a service, edits the old approval or copies query state.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from package_skill import (check, file_sha256, write_archive, source_pack_inventory,
                           SOURCE_PACK_PREFIX, LOCAL_ONLY_ROOTS, build_directory, new_output_path)
from ontology_engineering.local_paths import private_path
from check_public_privacy import path_findings

FORMAT = "ontology-engineering.engineering-candidate-assets/v1"
STATUS = "candidate_not_published"
LEDGER = "distribution/engineering-next-assets.json"
BASELINE = "distribution/shareable-core-assets.json"
OVERRIDES = "distribution/engineering-next-overrides"
DATA = "runtime/misumi/data"
DATA_MANIFEST = DATA + "/data-manifest.json"
EXTRA_FILES = {
    ".gitignore", "ontology_engineering/local_paths.py", "ontology_engineering/source_library.py",
    "scripts/source_library.py", "tests/test_source_library.py",
    "ontology_engineering/supplier_knowledge.py",
    "ontology_engineering/knowledge_context.py", "tests/test_knowledge_context.py",
    "ontology_engineering/local_pdf_sources.py", "ontology_engineering/source_citations.py",
    "ontology_engineering/semantic_bundle_transport.py", "ontology_engineering/semantic_engagement.py",
    "runtime/source-delivery.json", "tests/test_local_pdf_sources.py", "tests/test_unified_sources.py",
    "examples/knowledge-distillation/linear-motion.json", "examples/knowledge-distillation/README.md",
    "ontology_engineering/misumi/__init__.py", "ontology_engineering/misumi/catalog.py",
    "ontology_engineering/misumi/paths.py", "ontology_engineering/misumi/judger.py",
    "ontology_engineering/misumi/knowledge.py", "ontology_engineering/misumi/cli.py",
    "ontology_engineering/misumi/source_pack.py", "tests/test_misumi_source_pack.py",
    "scripts/jev_knowledge.py", "scripts/search_misumi_knowledge.py",
    "scripts/build_engineering_candidate.py", "scripts/package_skill.py", "scripts/check_public_privacy.py",
    "references/supplier-knowledge.md", "references/supplier-knowledge-policy.json",
    "references/supplier-knowledge-interpretation.md", "references/engineering-iteration.md",
    "references/practice-consolidation.md", "docs/releases/mechanical-design-next.md", "docs/releases/0.7.0.md",
    "docs/ENGINEERING-CANDIDATE.md", "docs/ENGINEERING-ITERATIONS.md", "runtime/misumi/setup.py",
    "runtime/misumi/requirements.txt", "runtime/misumi/README.md",
    "tests/test_supplier_knowledge.py", "tests/test_misumi_provider.py",
    "tests/test_jev_knowledge_portable.py", "tests/test_integrated_distribution.py",
    "tests/test_misumi_data_setup.py", "tests/fixtures/jev_setup_preflight.sh",
    "tests/misumi_reindex_fixture.py", "tests/test_jev_installation.py",
    "tests/test_knowledge_navigation.py",
}

# This orchestration implementation is shared by the source workspace and the
# next candidate. Historical release overrides retain their original identity,
# but must not silently substitute an older control plane in a new package.
CANONICAL_IMPLEMENTATIONS = frozenset({
    "ontology_engineering/semantic_engagement.py", "SKILL.md", "docs/PORTABLE-DISTRIBUTION.md",
    "scripts/package_skill.py", "scripts/build_engineering_candidate.py", "scripts/package_jev_trial.py",
})


# Explicitly retired from the next candidate after the 2026-09-27 cleanup.
# The legacy approved ledger stays immutable; missing assets are never silently skipped.
RETIRED_BASELINE_FILES = frozenset({
    'skills/cad-agent/references/fusion-execution.md',
    'skills/cad-agent/references/fusion-multi-instance.md',
    'skills/cad-agent/references/fusion-failure-recovery.md',
    'skills/cad-agent/references/fusion-release-compatibility.md',
    'skills/cad-agent/references/mcmaster-entry-guide.md',
    'skills/cad-agent/references/source-to-fusion-assembly-delivery.md',
    'skills/cad-agent/references/fusion-2704-intent-mode-routing.json',
    'skills/cad-agent/references/runbooks/fusion-runtime-safety-incidents.md',
    'skills/cad-agent/references/runbooks/fusion-import-tools.md',
    'skills/cad-agent/references/runbooks/fusion-mcp-ambiguity-triage.md',
    'skills/cad-agent/references/runbooks/fusion-mcp-caller-guide.md',
    'skills/cad-agent/references/runbooks/proposed-2026-09-06-reliability/C-transport-caller.md',
    'skills/cad-agent/references/runbooks/proposed-2026-09-06-reliability/B-budgets-sleep-recovery.md',
    'skills/cad-agent/references/runbooks/proposed-2026-09-06-reliability/D-tooling-peekaboo-doctor.md',
    'skills/cad-agent/references/runbooks/proposed-2026-09-06-reliability/A-window-census.md',
    'skills/cad-agent/references/runbooks/proposed-2026-09-06-reliability/D-tools-README.md',
    'skills/cad-agent/references/runbooks/backup-2026-09-07/fusion-execution.md',
    'skills/cad-agent/references/runbooks/backup-2026-09-07/fusion-runtime-safety-incidents.md',
    'skills/cad-agent/references/runbooks/backup-2026-09-07/fusion-mcp-ambiguity-triage.md',
    'skills/cad-agent/references/runbooks/backup-2026-09-07/fusion-mcp-caller-guide.md',
    'skills/cad-agent/references/images/mcmaster-fusion-cn-entry-20260925.png',
    'skills/cad-agent/references/images/mcmaster-fusion-cn-entry-20260925.provenance.json',
    'runtime/vendor/semantica-0.6.5+oe.3-py3-none-any.whl',
    'skills/cad-agent/references/legacy-20260925/setup.sh.txt',
    'skills/cad-agent/references/legacy-20260925/README.md',
    'skills/cad-agent/references/legacy-20260925/CONTRIBUTING.md',
    'skills/cad-agent/references/legacy-20260925/doctor.sh.txt',
    'skills/cad-agent/references/legacy-20260925/AGENTS.md',
    'skills/cad-agent/references/legacy-20260925/ARCHIVED_ENTRYPOINT.md',
    'skills/cad-agent/references/legacy-20260925/references/supplier-cad-acquisition.md',
    'skills/cad-agent/references/legacy-20260925/references/source-to-fusion-assembly-delivery.md',
    'skills/cad-agent/references/legacy-20260925/assembly/assets/assembly-authoring-manifest.template.json',
    'skills/cad-agent/data/semantic-assets/active-bundle.json',
    'skills/cad-agent/lessons-inbox/20260827-experience-route-standalone-deps.md',
})

def safe_relative(value: str) -> str:
    if not isinstance(value, str) or not value or "\\" in value:
        raise ValueError("invalid candidate relative path")
    path = Path(value)
    if (path.is_absolute() or path.as_posix() != value or any(p in {".", ".."} for p in path.parts)
            or any(ord(c) < 32 or ord(c) == 127 for c in value)):
        raise ValueError("invalid candidate relative path")
    return value


def regular(root: Path, relative: str) -> Path:
    path = root / safe_relative(relative)
    if not path.is_file() or path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("missing or unsafe candidate file: " + relative)
    for parent in path.parents:
        if parent == root:
            break
        if parent.is_symlink():
            raise ValueError("symbolic candidate parent: " + relative)
    if path_findings(path, root):
        raise ValueError("excluded candidate path: " + relative)
    return path


def data_entries(root: Path) -> tuple[list[dict], dict]:
    manifest = json.loads(regular(root, DATA_MANIFEST).read_text(encoding="utf-8"))
    if manifest.get("schema") != "ontology-engineering.misumi-data/v1":
        raise ValueError("unknown catalog data manifest")
    entries, seen = [], set()
    for source in manifest["files"]:
        name = safe_relative(source["path"])
        if name in seen or name == "data-manifest.json":
            raise ValueError("duplicate or self-listed data entry")
        seen.add(name)
        if not re.fullmatch(r"[0-9a-f]{64}", source["sha256"]):
            raise ValueError("invalid data hash")
        if type(source["bytes"]) is not int or source["bytes"] < 0:
            raise ValueError("invalid data byte count")
        relative = DATA + "/" + name
        path = regular(root, relative)
        if path.stat().st_size != source["bytes"]:
            raise ValueError("data size differs from manifest: " + name)
        if (path.suffix.lower(), source["media_type"]) not in {
                (".pdf", "application/pdf"), (".json", "application/json"),
                (".sqlite3", "application/vnd.sqlite3")}:
            raise ValueError("unsupported data media type: " + name)
        entries.append({"path": relative, "source": relative, "sha256": source["sha256"],
                        "bytes": source["bytes"], "kind": "supplier_data", "role": source["role"],
                        "media_type": source["media_type"], "provenance": "source-bound data manifest; private use candidate"})
    actual = {p.relative_to(root / DATA).as_posix() for p in (root / DATA).rglob("*") if p.is_file() or p.is_symlink()}
    if actual != seen | {"data-manifest.json"}:
        raise ValueError("unexpected or absent data file outside data manifest")
    return entries, manifest["source_catalogs"]


def refresh_ledger(root: Path = ROOT, *, include_source_data: bool = False, source_pack: Path | None = None) -> dict:
    root = root.resolve()
    if include_source_data and source_pack is not None:
        raise ValueError("expanded and compressed source modes cannot be combined")
    old_path = regular(root, BASELINE)
    baseline = json.loads(old_path.read_text())
    if baseline.get("format") != "ontology-engineering.shareable-core-assets/v1":
        raise ValueError("unknown baseline selection")
    selected = {}
    for old in baseline["files"]:
        name = safe_relative(old["path"])
        if name in selected:
            raise ValueError("duplicate baseline asset")
        if old["origin"] == "source":
            selected[name] = name
        elif old["origin"] == "override":
            source = "distribution/shareable-overrides/" + name
            selected[name] = source + ".in" if Path(name).name == "SKILL.md" else source
        else:
            raise ValueError("unknown baseline origin")
    for name in EXTRA_FILES:
        selected[name] = name
    # This deliberately enumerates only our small reviewed template tree, never
    # the complete CAD history, book sources or arbitrary untracked code.
    for path in sorted((root / OVERRIDES).rglob("*")):
        if not path.is_file() and not path.is_symlink():
            continue
        relative = path.relative_to(root / OVERRIDES).as_posix()
        target = relative[:-3] if relative.endswith("SKILL.md.in") else relative
        selected[safe_relative(target)] = path.relative_to(root).as_posix()
    for name in CANONICAL_IMPLEMENTATIONS.intersection(selected):
        selected[name] = name
    # The primary package never inherits raw data, even if a future baseline
    # selection contains it. An absent data directory is valid for the core.
    selected = {name: source for name, source in selected.items()
                if name not in RETIRED_BASELINE_FILES
                and name.split("/", 1)[0] not in LOCAL_ONLY_ROOTS
                and source.split("/", 1)[0] not in LOCAL_ONLY_ROOTS
                and not name.startswith((DATA + "/", SOURCE_PACK_PREFIX + "/"))
                and not source.startswith((DATA + "/", SOURCE_PACK_PREFIX + "/"))}
    if include_source_data:
        selected[DATA_MANIFEST] = DATA_MANIFEST
    files = []
    for target, source in sorted(selected.items()):
        path = regular(root, source)
        files.append({"path": target, "source": source, "sha256": file_sha256(path),
                      "bytes": path.stat().st_size, "kind": "code_or_method",
                      "provenance": "baseline selection; current candidate bytes" if target not in EXTRA_FILES else "integrated candidate addition"})
    data, identity = data_entries(root) if include_source_data else ([], None)
    files += data
    packed_identity = None
    if source_pack is not None:
        entries, _ = source_pack_inventory(source_pack)
        for entry in entries:
            files.append({**entry, "path": SOURCE_PACK_PREFIX + "/" + entry["path"],
                          "source": entry["path"], "source_origin": "source_pack",
                          "kind": "compressed_source_pack", "provenance": "lossless original-source archive; private candidate"})
        packed_identity = {"manifest_sha256": next(e["sha256"] for e in entries if e["path"] == "source-pack.json"),
                           "files": len(entries), "bytes": sum(e["bytes"] for e in entries), "target": SOURCE_PACK_PREFIX}
    if len({e["path"] for e in files}) != len(files):
        raise ValueError("candidate target collision")
    ledger = {"format": FORMAT, "status": STATUS, "base_version": (root / "VERSION").read_text().strip(),
              "publication_approval": "not_granted", "distribution_scope": "private integrated engineering development candidate",
              "baseline": {"path": BASELINE, "sha256": file_sha256(old_path),
                           "purpose": "Explicit asset selection only; prior approval does not apply to changed bytes or supplier data."},
              "excludes": baseline["excludes"] + ["installed environments", "query history and cache", "API keys", "book full text"],
              "source_data_included": include_source_data, "source_pack": packed_identity,
              "retired_baseline_assets": sorted(RETIRED_BASELINE_FILES),
              "knowledge_scope": "Shared engineering method and bounded distilled examples; not a fully distilled supplier catalog.",
              "source_catalogs": identity,
              "files": sorted(files, key=lambda e: e["path"])}
    (root / LEDGER).write_text(json.dumps(ledger, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return {"status": STATUS, "source_data_included": include_source_data, "source_pack": packed_identity, "asset_count": len(files), "data_files": len(data),
            "bytes": sum(e["bytes"] for e in files), "ledger_sha256": file_sha256(root / LEDGER)}


def stage(directory: Path, root: Path = ROOT, *, link_data: bool = True, include_source_data: bool = False,
          source_pack: Path | None = None) -> dict:
    root = root.resolve()
    if source_pack is not None:
        source_pack = source_pack.resolve()
    if include_source_data and source_pack is not None:
        raise ValueError("expanded and compressed source modes cannot be combined")
    if directory.exists() and any(directory.iterdir()):
        raise ValueError("candidate stage must be a new or empty directory")
    ledger_path = regular(root, LEDGER)
    ledger_text = ledger_path.read_text(encoding="utf-8")
    ledger = json.loads(ledger_text)
    if (ledger.get("format") != FORMAT or ledger.get("status") != STATUS
            or ledger.get("publication_approval") != "not_granted"):
        raise ValueError("candidate status cannot confer publication approval")
    data_present = any(e["path"].startswith(DATA + "/") or e.get("kind") == "supplier_data"
                       for e in ledger["files"])
    frozen_data = ledger.get("source_data_included", data_present)
    if type(frozen_data) is not bool or frozen_data != data_present:
        raise ValueError("candidate source data scope differs from inventory")
    if frozen_data != include_source_data:
        raise ValueError("source data requires matching explicit --include-source-data at freeze and build; refresh the ledger for the intended scope")
    packed_identity = ledger.get("source_pack")
    if bool(packed_identity) != (source_pack is not None):
        raise ValueError("compressed source pack requires matching --source-pack at freeze and build")
    if source_pack is not None:
        entries, _ = source_pack_inventory(source_pack, verify_hashes=False)
        current = {"manifest_sha256": next(e["sha256"] for e in entries if e["path"] == "source-pack.json"),
                   "files": len(entries), "bytes": sum(e["bytes"] for e in entries), "target": SOURCE_PACK_PREFIX}
        if current != packed_identity:
            raise ValueError("source pack changed since candidate freeze")
    seen, hashes = set(), {}
    directory.mkdir(parents=True, exist_ok=True)
    for entry in ledger["files"]:
        name = safe_relative(entry["path"])
        if name.split("/", 1)[0] in LOCAL_ONLY_ROOTS:
            raise ValueError("local sources or private state must not be included in the candidate core")
        if name in seen or name in {LEDGER, "CANDIDATE-STATUS.json", "PORTABLE-MANIFEST.json"}:
            raise ValueError("duplicate or reserved candidate path")
        seen.add(name)
        if entry.get("source_origin") == "source_pack":
            if source_pack is None or entry["kind"] != "compressed_source_pack" or name != SOURCE_PACK_PREFIX + "/" + entry["source"]:
                raise ValueError("invalid compressed candidate source mapping")
            source = regular(source_pack, entry["source"])
        elif entry.get("source_origin", "source") == "source":
            if safe_relative(entry["source"]).split("/", 1)[0] in LOCAL_ONLY_ROOTS:
                raise ValueError("local sources or private state must not be included in the candidate core")
            source = regular(root, entry["source"])
        else:
            raise ValueError("unknown candidate source origin")
        if source.stat().st_size != entry["bytes"] or file_sha256(source) != entry["sha256"]:
            raise ValueError("asset changed since candidate freeze: " + name)
        target = directory / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if entry["kind"] in {"supplier_data", "compressed_source_pack"} and link_data:
            try:
                os.link(source, target)
            except OSError:
                shutil.copy2(source, target)
        else:
            shutil.copy2(source, target)
        hashes[name] = entry["sha256"]
    (directory / LEDGER).parent.mkdir(parents=True, exist_ok=True)
    (directory / LEDGER).write_text(ledger_text, encoding="utf-8")
    hashes[LEDGER] = file_sha256(directory / LEDGER)
    status = {"schema": "ontology-engineering.candidate-status/v1", "status": STATUS,
              "base_version": ledger["base_version"], "publication_approval": "not_granted",
              "ledger": LEDGER, "ledger_sha256": hashes[LEDGER], "source_data_included": frozen_data, "source_pack": packed_identity,
              "scope": "Private integrated candidate. Supplier materials retain their rights. No physical acceptance or semantic publication is asserted."}
    (directory / "CANDIDATE-STATUS.json").write_text(json.dumps(status, ensure_ascii=False, indent=2) + "\n")
    hashes["CANDIDATE-STATUS.json"] = file_sha256(directory / "CANDIDATE-STATUS.json")
    return {"status": STATUS, "source_data_included": frozen_data, "source_pack": packed_identity, "asset_count": len(seen), "ledger_sha256": hashes[LEDGER], "hashes": hashes}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--refresh-ledger", action="store_true", help="Freeze selection and hashes; does not grant release approval")
    parser.add_argument("--output", type=Path, help="New private ZIP inside this skill's var/ (normally var/builds/)")
    parser.add_argument("--stage", type=Path, help="Retain a new exact stage inside this skill's var/")
    parser.add_argument("--include-source-data", action="store_true", help="Explicit compatibility build with the full source archive; required at both freeze and build")
    parser.add_argument("--source-pack", type=Path, help="Explicit local folder containing a complete compressed source-pack manifest and its declared archives")
    parser.add_argument("--copy-data", action="store_true", help="Copy data instead of space-saving hard links")
    parser.add_argument("--inspect-only", action="store_true", help="Stage and check without writing a ZIP")
    args = parser.parse_args(argv)
    root = args.root.expanduser().resolve()
    try:
        if root != ROOT.resolve():
            private_path(root, "candidate_root", root=ROOT)
        if args.include_source_data and args.source_pack:
            parser.error("--include-source-data and --source-pack are mutually exclusive")
        packed_root = args.source_pack.expanduser().resolve() if args.source_pack else None
        if args.refresh_ledger:
            if args.output or args.stage:
                parser.error("freeze and build are separate operations")
            print(json.dumps(refresh_ledger(root, include_source_data=args.include_source_data, source_pack=packed_root), ensure_ascii=False, indent=2))
            return 0
        output = new_output_path(args.output, ROOT) if args.output else None
        retained = private_path(args.stage, "stage", root=ROOT) if args.stage else None
        if retained and retained.exists():
            parser.error("stage must be a new path")
        if output and retained and output.is_relative_to(retained):
            parser.error("output must not be inside the stage")
        with tempfile.TemporaryDirectory(prefix="oe-engineering-candidate-", dir=build_directory(ROOT)) as temporary:
            directory = retained or Path(temporary) / "ontology-engineering"
            frozen = stage(directory, root, link_data=not args.copy_data, include_source_data=args.include_source_data, source_pack=packed_root)
            report, files = check(directory)
            report.update({k: v for k, v in frozen.items() if k != "hashes"})
            if report["passed"] and output and not args.inspect_only:
                report["archive"] = write_archive(directory, files, output, frozen["hashes"])
            print(json.dumps(report, ensure_ascii=False, indent=2))
            return 0 if report["passed"] else 1
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(json.dumps({"passed": False, "status": STATUS, "error": str(error)}, ensure_ascii=False))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
