#!/usr/bin/env python3
"""Build the reviewed, relocatable public engineering-ontology core.

The exact source/override file list and hashes are pinned in
``distribution/shareable-core-assets.json``. No directory glob is accepted.
The books, customer evidence, historical CAD cases and site-specific machine configurations remain
outside this distribution.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from package_skill import check  # noqa: E402

ASSETS = ROOT / "distribution/shareable-core-assets.json"
OVERRIDES = ROOT / "distribution/shareable-overrides"
PUBLIC_CORE_SCOPE = "public-core-v0.5.8-candidate"


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _safe_relative(value: str) -> Path:
    if not isinstance(value, str):
        raise ValueError(f"unsafe asset path: {value}")
    path = Path(value)
    if (not value or "\\" in value
            or path.is_absolute() or path.as_posix() != value
            or any(part in {".", ".."} for part in path.parts)
            or any(ord(character) < 32 or ord(character) == 127 for character in value)):
        raise ValueError(f"unsafe asset path: {value}")
    return path


def stage(directory: Path) -> dict:
    ledger = json.loads(ASSETS.read_text(encoding="utf-8"))
    if ledger.get("format") != "ontology-engineering.shareable-core-assets/v1":
        raise ValueError("unknown shareable asset ledger")
    if ledger.get("distribution_scope") != PUBLIC_CORE_SCOPE + "; owner-approved":
        raise ValueError("shareable distribution scope changed without review")
    seen: set[str] = set()
    for entry in ledger["files"]:
        name = entry["path"]
        relative = _safe_relative(name)
        if name in seen:
            raise ValueError(f"duplicate asset: {name}")
        seen.add(name)
        if (entry.get("privacy_review") != "screened_for_known_identifiers_and_direct_secrets"
                or entry.get("rights_scope") != PUBLIC_CORE_SCOPE
                or entry.get("public_approval") != "owner_approved"
                or entry.get("has_personal_data") is not False
                or not entry.get("license_or_authority")
                or not entry.get("author_or_generation")
                or not entry.get("input_rights")):
            raise ValueError(f"asset review state is not public: {name}")
        origin = entry["origin"]
        if origin not in {"source", "override"}:
            raise ValueError(f"unknown origin for {name}")
        origin_root = ROOT if origin == "source" else OVERRIDES
        source = origin_root / relative
        if (source.is_symlink() or not source.is_file()
                or not source.resolve().is_relative_to(origin_root.resolve())
                or any(parent.is_symlink() for parent in source.parents if parent != origin_root)):
            raise ValueError(f"asset is missing or symbolic: {origin}:{name}")
        data = source.read_bytes()
        if _sha256(data) != entry["sha256"]:
            raise ValueError(f"asset changed since review: {origin}:{name}")
        target = directory / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        target.chmod(source.stat().st_mode & 0o777)
    return {"asset_count": len(seen), "ledger_sha256": _sha256(ASSETS.read_bytes())}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New ZIP path outside the source tree")
    parser.add_argument("--inspect-only", action="store_true", help="Validate the stage without writing a ZIP")
    args = parser.parse_args()
    output = args.output.expanduser().resolve()
    if output.exists() or output.is_relative_to(ROOT):
        parser.error("output must be a new path outside the source tree")
    with tempfile.TemporaryDirectory(prefix="oe-shareable-core-") as temporary:
        stage_root = Path(temporary) / "ontology-engineering"
        stage_root.mkdir()
        ledger_report = stage(stage_root)
        report, files = check(stage_root)
        report.update(ledger_report)
        if not report["passed"]:
            print(json.dumps(report, ensure_ascii=False, indent=2))
            return 1
        if not args.inspect_only:
            # Reuse the same checker and manifest writer against the staged tree.
            import subprocess
            command = [sys.executable, str(ROOT / "scripts/package_skill.py"),
                       "--root", str(stage_root), "--output", str(output)]
            result = subprocess.run(command, capture_output=True, text=True)
            if result.returncode:
                raise RuntimeError(result.stdout + result.stderr)
            packaged = json.loads(result.stdout)
            report["archive"] = packaged["archive"]
        print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
