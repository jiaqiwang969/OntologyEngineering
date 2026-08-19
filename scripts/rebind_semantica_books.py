#!/usr/bin/env python3
"""Rebind both maintained book sources to Semantica's 29 chapter packages.

The command is intentionally a release-engineering tool, not a runtime
fallback.  It updates source/TeX digests only in an explicit Semantica source
checkout and then leaves normal Semantica validation to fail closed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


VOL1_GUIDE_DIRS = {
    "ch01": "ch01-introduction",
    "ch02": "ch02-ontology-foundations",
    "ch03": "ch03-ontology-methodology",
    "ch04": "ch04-ontology-languages",
    "ch05": "ch05-reasoning",
    "ch06": "ch06-applications",
    "ch07": "ch07-knowledge-graph",
    "ch08": "ch08-ontology-llm",
    "ch09": "ch09-capstone-manufacturing",
}


def _read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"expected an object: {path}")
    return value


def _encoded(value: dict[str, Any]) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def _sha_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _sha_file(path: Path) -> str:
    return _sha_bytes(path.read_bytes())


def _relative_book_path(book_root: Path, anchor: str) -> Path:
    relative = Path(anchor)
    if relative.is_absolute():
        raise ValueError(f"absolute book anchor is forbidden: {anchor}")
    if relative.parts and relative.parts[0] == "ontology-engineering":
        relative = Path(*relative.parts[1:])
    root = book_root.resolve()
    result = (root / relative).resolve()
    try:
        result.relative_to(root)
    except ValueError as exc:
        raise ValueError(f"book anchor escapes repository: {anchor}") from exc
    return result


def _source_binding(
    *, volume: str, chapter: str, book_root: Path, manifest: dict[str, Any]
) -> tuple[str, str, str, str, str, str, set[str]]:
    if volume == "vol1":
        source_anchor = (
            "references/ontology-engineering-book/handbook/chapters/"
            f"{chapter}.tex"
        )
        guide_anchor = (
            "references/ontology-engineering-book/"
            f"{VOL1_GUIDE_DIRS[chapter]}/README.md"
        )
        tex_anchor = source_anchor
        current = manifest.get("chapter", {}).get("external_specification", {})
        old_anchor = current.get("source_anchor", "")
    elif volume == "vol2":
        current = manifest.get("book_source", {})
        source_anchor = str(current.get("logical_anchor", ""))
        source_parent = Path(source_anchor).parent
        guide_anchor = (source_parent / "README.md").as_posix()
        tex_anchor = (
            "ontology-engineering/references/product-trustworthiness-book/"
            f"handbook/fragments/readme-{chapter}.tex"
        )
        old_anchor = current.get("logical_anchor", "")
    else:
        raise ValueError(f"unsupported volume: {volume}")
    source_path = _relative_book_path(book_root, source_anchor)
    guide_path = _relative_book_path(book_root, guide_anchor)
    tex_path = _relative_book_path(book_root, tex_anchor)
    if not source_path.is_file():
        raise FileNotFoundError(f"missing authoritative book source: {source_path}")
    if not tex_path.is_file():
        raise FileNotFoundError(f"missing maintained TeX source: {tex_path}")
    if not guide_path.is_file():
        raise FileNotFoundError(f"missing maintained chapter guide: {guide_path}")
    return (
        source_anchor,
        _sha_file(source_path),
        guide_anchor,
        _sha_file(guide_path),
        tex_anchor,
        _sha_file(tex_path),
        {str(old_anchor), source_anchor} - {""},
    )


def _set_external_binding(
    target: dict[str, Any],
    *,
    source_anchor: str,
    source_sha256: str,
    guide_anchor: str,
    guide_sha256: str,
    tex_anchor: str,
    tex_sha256: str,
) -> None:
    target["source_anchor"] = source_anchor
    target["source_sha256"] = source_sha256
    target["guide_anchor"] = guide_anchor
    target["guide_sha256"] = guide_sha256
    target["tex_anchor"] = tex_anchor
    target["tex_sha256"] = tex_sha256


def _set_book_source_binding(
    target: dict[str, Any],
    *,
    source_anchor: str,
    source_sha256: str,
    guide_anchor: str,
    guide_sha256: str,
    tex_anchor: str,
    tex_sha256: str,
) -> None:
    target["logical_anchor"] = source_anchor
    target["sha256"] = source_sha256
    target["guide_anchor"] = guide_anchor
    target["guide_sha256"] = guide_sha256
    target["tex_anchor"] = tex_anchor
    target["tex_sha256"] = tex_sha256


def rebind(
    *, book_root: Path, semantica_root: Path, write: bool
) -> dict[str, Any]:
    package_root = semantica_root / "semantica" / "chapter_packages"
    registry_path = package_root / "registry.yaml"
    registry = _read_json(registry_path)
    records = registry.get("packages")
    if not isinstance(records, list) or len(records) != 29:
        raise ValueError("Semantica registry must contain exactly 29 chapter packages")

    changed: list[str] = []
    errors: list[str] = []
    contract_hashes: dict[tuple[str, str], str] = {}
    desired_files: dict[Path, bytes] = {}

    for record in records:
        volume = str(record["volume"])
        chapter = str(record["chapter"])
        manifest_path = package_root / str(record["manifest"])
        manifest = _read_json(manifest_path)
        original_manifest = manifest_path.read_bytes()
        try:
            (
                source_anchor,
                source_sha256,
                guide_anchor,
                guide_sha256,
                tex_anchor,
                tex_sha256,
                old_anchors,
            ) = _source_binding(
                volume=volume,
                chapter=chapter,
                book_root=book_root,
                manifest=manifest,
            )
        except (FileNotFoundError, ValueError) as exc:
            errors.append(f"{volume}.{chapter}: {exc}")
            continue

        assets = manifest.get("assets")
        if not isinstance(assets, list):
            errors.append(f"{volume}.{chapter}: manifest assets are missing")
            continue
        contract_asset = next(
            (
                item
                for item in assets
                if isinstance(item, dict) and item.get("role") == "chapter_contract"
            ),
            None,
        )
        if not isinstance(contract_asset, dict):
            errors.append(f"{volume}.{chapter}: chapter contract asset is missing")
            continue
        contract_path = manifest_path.parent / str(contract_asset["path"])
        contract = _read_json(contract_path)
        original_contract = contract_path.read_bytes()

        external = contract.setdefault("external_specification", {})
        if not isinstance(external, dict):
            errors.append(f"{volume}.{chapter}: invalid contract external specification")
            continue
        _set_external_binding(
            external,
            source_anchor=source_anchor,
            source_sha256=source_sha256,
            guide_anchor=guide_anchor,
            guide_sha256=guide_sha256,
            tex_anchor=tex_anchor,
            tex_sha256=tex_sha256,
        )
        if volume == "vol1":
            manifest_external = manifest["chapter"].setdefault(
                "external_specification", {}
            )
            _set_external_binding(
                manifest_external,
                source_anchor=source_anchor,
                source_sha256=source_sha256,
                guide_anchor=guide_anchor,
                guide_sha256=guide_sha256,
                tex_anchor=tex_anchor,
                tex_sha256=tex_sha256,
            )
        else:
            contract_book = contract.setdefault("book_source", {})
            _set_book_source_binding(
                contract_book,
                source_anchor=source_anchor,
                source_sha256=source_sha256,
                guide_anchor=guide_anchor,
                guide_sha256=guide_sha256,
                tex_anchor=tex_anchor,
                tex_sha256=tex_sha256,
            )
            contract["source_anchor"] = source_anchor
            manifest_book = manifest.setdefault("book_source", {})
            _set_book_source_binding(
                manifest_book,
                source_anchor=source_anchor,
                source_sha256=source_sha256,
                guide_anchor=guide_anchor,
                guide_sha256=guide_sha256,
                tex_anchor=tex_anchor,
                tex_sha256=tex_sha256,
            )

        for asset in assets:
            if not isinstance(asset, dict):
                continue
            if str(asset.get("source_anchor", "")) in old_anchors:
                asset["source_anchor"] = source_anchor
                asset["source_sha256"] = source_sha256

        contract_bytes = _encoded(contract)
        contract_sha256 = _sha_bytes(contract_bytes)
        contract_asset["sha256"] = contract_sha256
        contract_hashes[(str(record["package_id"]), str(contract_asset["asset_id"]))] = (
            contract_sha256
        )
        manifest_bytes = _encoded(manifest)
        desired_files[contract_path] = contract_bytes
        desired_files[manifest_path] = manifest_bytes
        if original_contract != contract_bytes:
            changed.append(str(contract_path.relative_to(semantica_root)))
        if original_manifest != manifest_bytes:
            changed.append(str(manifest_path.relative_to(semantica_root)))

    migration_path = package_root / "vol2" / "migration-map.json"
    migration = _read_json(migration_path)
    original_migration = migration_path.read_bytes()
    mappings = migration.get("mappings")
    if not isinstance(mappings, list):
        errors.append("vol2 migration map has no mappings list")
    else:
        for item in mappings:
            if not isinstance(item, dict):
                continue
            key = (str(item.get("new_package", "")), str(item.get("new_asset_id", "")))
            if key in contract_hashes:
                item["new_sha256"] = contract_hashes[key]
        migration_bytes = _encoded(migration)
        desired_files[migration_path] = migration_bytes
        if original_migration != migration_bytes:
            changed.append(str(migration_path.relative_to(semantica_root)))

    if write and not errors:
        for path, content in desired_files.items():
            path.write_bytes(content)

    return {
        "changed_count": len(changed),
        "changed_files": sorted(changed),
        "error_count": len(errors),
        "errors": errors,
        "mode": "write" if write else "check",
        "package_count": len(records),
        "passed": not errors and (write or not changed),
        "schema_version": "1.0",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--book-root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Root of the ontology-engineering checkout (defaults to this repository).",
    )
    parser.add_argument(
        "--semantica-root",
        type=Path,
        required=True,
        help="Explicit Semantica source checkout to audit or update.",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="Write reviewed hashes; without this flag the command is check-only.",
    )
    args = parser.parse_args()
    report = rebind(
        book_root=args.book_root.resolve(),
        semantica_root=args.semantica_root.resolve(),
        write=args.write,
    )
    print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
