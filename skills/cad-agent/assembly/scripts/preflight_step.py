#!/usr/bin/env python3
"""Read-only STEP intake preflight for a new assembly-ontology project.

This script checks container/schema/assembly signals and evidence availability. It
does not parse B-Rep geometry, prove transforms, infer process truth, or certify a
motion path.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import mmap
import re
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


ENTITY_NAMES = (
    "PRODUCT",
    "PRODUCT_DEFINITION",
    "NEXT_ASSEMBLY_USAGE_OCCURRENCE",
    "SPECIFIED_HIGHER_USAGE_OCCURRENCE",
    "MAPPED_ITEM",
    "ITEM_DEFINED_TRANSFORMATION",
    "CARTESIAN_TRANSFORMATION_OPERATOR",
    "AXIS2_PLACEMENT_3D",
    "MANIFOLD_SOLID_BREP",
    "BREP_WITH_VOIDS",
    "CLOSED_SHELL",
    "OPEN_SHELL",
    "ADVANCED_FACE",
)
P21_TOKEN = re.compile(
    rb"(?P<string>'(?:''|[^'])*')"
    rb"|(?P<comment>/\*.*?\*/)"
    rb"|(?P<entity>#\d+\s*=\s*(?:\(\s*)?"
    rb"(?P<name>[A-Z_][A-Z0-9_]*)\s*\(\s*"
    rb"(?P<first_string>'(?:''|[^'])*')?)",
    re.IGNORECASE | re.DOTALL,
)
SCHEMA_FAMILY_ALIASES = {
    "AP203": (b"AP203", b"CONFIG_CONTROL_DESIGN"),
    "AP214": (b"AP214", b"AUTOMOTIVE_DESIGN"),
    "AP242": (b"AP242", b"MANAGED_MODEL_BASED_3D_ENGINEERING"),
}
STEP_STRING = re.compile(br"'((?:''|[^'])*)'")
FILE_SCHEMA = re.compile(br"FILE_SCHEMA\s*\(\s*\((.*?)\)\s*\)", re.IGNORECASE | re.DOTALL)


def sha256_path(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def file_record(path: Path, media_type: str) -> dict[str, Any]:
    return {
        "path": str(path.resolve()),
        "sha256": sha256_path(path),
        "size_bytes": path.stat().st_size,
        "media_type": media_type,
    }


def decode_product_strings(strings: list[bytes]) -> dict[str, Any]:
    non_ascii = [item.replace(b"''", b"'") for item in strings if any(byte > 127 for byte in item)]
    results: Counter[str] = Counter()
    for item in non_ascii:
        if b"\\X2\\" in item or b"\\X4\\" in item:
            results["iso_10303_escape"] += 1
            continue
        for encoding in ("utf-8", "gb18030"):
            try:
                item.decode(encoding)
            except UnicodeDecodeError:
                results[f"{encoding}_fail"] += 1
            else:
                results[f"{encoding}_success"] += 1
    return {
        "strings_observed": len(strings),
        "raw_non_ascii_strings": len(non_ascii),
        "decode_probe": dict(sorted(results.items())),
        "interpretation": "A decode probe is diagnostic only; preserve raw bytes and declare the chosen source-name encoding before identity binding.",
    }


def inspect_step(path: Path) -> dict[str, Any]:
    digest = hashlib.sha256()
    header = bytearray()
    tail = bytearray()
    capture_limit = 2 * 1024 * 1024
    chunk_size = 4 * 1024 * 1024
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(chunk_size), b""):
            digest.update(chunk)
            if len(header) < capture_limit:
                header.extend(chunk[: capture_limit - len(header)])
            tail.extend(chunk)
            if len(tail) > capture_limit:
                del tail[: len(tail) - capture_limit]

    counts = Counter({name.casefold(): 0 for name in ENTITY_NAMES})
    product_names: list[bytes] = []
    entity_records = 0
    if path.stat().st_size:
        with path.open("rb") as handle:
            with mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ) as mapped:
                for match in P21_TOKEN.finditer(mapped):
                    raw_name = match.group("name")
                    if raw_name is None:
                        continue
                    entity_records += 1
                    name = raw_name.decode("ascii").casefold()
                    if name.startswith("cartesian_transformation_operator"):
                        name = "cartesian_transformation_operator"
                    if name in counts:
                        counts[name] += 1
                    first_string = match.group("first_string")
                    if name == "product" and first_string:
                        product_names.append(first_string[1:-1])

    schema_match = FILE_SCHEMA.search(bytes(header))
    schemas: list[str] = []
    if schema_match:
        schemas = [
            item.decode("ascii", errors="replace")
            for item in STEP_STRING.findall(schema_match.group(1))
        ]
    schema_upper = " ".join(schemas).upper()
    header_upper = bytes(header).upper()
    tail_upper = bytes(tail).upper()
    schema_evidence = schema_upper.encode("ascii", errors="ignore") + b" " + header_upper
    schema_family = next(
        (
            family
            for family, aliases in SCHEMA_FAMILY_ALIASES.items()
            if any(alias in schema_evidence for alias in aliases)
        ),
        None,
    )
    markers = {
        "iso_10303_21": b"ISO-10303-21;" in header_upper,
        "header": b"HEADER;" in header_upper,
        "data": b"DATA;" in header_upper or b"DATA;" in tail_upper,
        "end_iso_10303_21": b"END-ISO-10303-21;" in tail_upper,
    }
    assembly_signal_count = (
        counts["next_assembly_usage_occurrence"]
        + counts["specified_higher_usage_occurrence"]
        + counts["mapped_item"]
    )
    geometry_signal_count = (
        counts["manifold_solid_brep"]
        + counts["brep_with_voids"]
        + counts["closed_shell"]
        + counts["open_shell"]
        + counts["advanced_face"]
    )
    return {
        "path": str(path.resolve()),
        "sha256": digest.hexdigest(),
        "size_bytes": path.stat().st_size,
        "entity_record_lines": entity_records,
        "file_schema": schemas,
        "schema_family_observed": schema_family,
        "container_markers": markers,
        "entity_signals": dict(sorted(counts.items())),
        "assembly_structure_signal_count": assembly_signal_count,
        "geometry_signal_count": geometry_signal_count,
        "product_name_encoding_probe": decode_product_strings(product_names),
    }


def existing_records(paths: Iterable[str] | None, role: str) -> tuple[list[dict[str, Any]], list[str]]:
    records: list[dict[str, Any]] = []
    missing: list[str] = []
    for raw in paths or []:
        path = Path(raw).expanduser()
        if not path.is_file():
            missing.append(str(path))
            continue
        media_type = "application/octet-stream"
        if path.suffix.casefold() == ".json":
            media_type = "application/json"
        elif path.suffix.casefold() in {".xlsx", ".xlsm"}:
            media_type = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        elif path.suffix.casefold() in {".md", ".txt", ".csv"}:
            media_type = "text/plain"
        record = file_record(path, media_type)
        record["role"] = role
        records.append(record)
    return records, missing


def decide(
    step: dict[str, Any],
    sop_records: list[dict[str, Any]],
    bom_records: list[dict[str, Any]],
    missing: list[str],
) -> tuple[str, list[str]]:
    holds: list[str] = []
    if not all(step["container_markers"].values()):
        holds.append("STEP_CONTAINER_MARKERS_INCOMPLETE")
        return "HOLD_INVALID_STEP_CONTAINER", holds
    if not step["file_schema"]:
        holds.append("FILE_SCHEMA_NOT_OBSERVED")
    elif not step["schema_family_observed"]:
        holds.append("STEP_SCHEMA_ADAPTER_NOT_DECLARED")
    if step["geometry_signal_count"] == 0:
        holds.append("GEOMETRY_ENTITIES_NOT_OBSERVED")
    if step["assembly_structure_signal_count"] == 0:
        holds.append("ASSEMBLY_STRUCTURE_NOT_OBSERVED_BY_TEXT_PREFLIGHT")
    if missing:
        holds.append("DECLARED_RECORD_FILE_MISSING")
    if not bom_records:
        holds.append("IDENTITY_RECORD_NOT_SUPPLIED")
    if not sop_records:
        holds.append("PROCESS_TRUTH_NOT_SUPPLIED")
    if sop_records or bom_records:
        holds.append("CONTROLLED_RECORD_SCHEMA_NOT_VALIDATED")

    structural_holds = {
        "FILE_SCHEMA_NOT_OBSERVED",
        "STEP_SCHEMA_ADAPTER_NOT_DECLARED",
        "GEOMETRY_ENTITIES_NOT_OBSERVED",
        "ASSEMBLY_STRUCTURE_NOT_OBSERVED_BY_TEXT_PREFLIGHT",
        "DECLARED_RECORD_FILE_MISSING",
    }
    if structural_holds.intersection(holds):
        return "HOLD_PRE_S1_REVIEW_REQUIRED", holds
    if not bom_records and not sop_records:
        return "READY_FOR_S1_GEOMETRY__HOLD_IDENTITY_AND_PROCESS_TRUTH", holds
    if bom_records and not sop_records:
        return "READY_FOR_S0_IDENTITY_REVIEW__HOLD_PROCESS_TRUTH", holds
    if sop_records and not bom_records:
        return "READY_FOR_S0_PROCESS_REVIEW__HOLD_IDENTITY_TRUTH", holds
    return "READY_FOR_S0_CONTROLLED_RECORD_VALIDATION", holds


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("step", help="STEP/STP file to inspect")
    parser.add_argument("--sop", action="append", help="controlled SOP/process record; repeatable")
    parser.add_argument("--bom", action="append", help="controlled BOM/identity record; repeatable")
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    parser.add_argument("--output", help="also write the JSON report to this path")
    return parser.parse_args()


def write_report(report: dict[str, Any], output_path: str | None) -> str:
    rendered = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True)
    if output_path:
        output = Path(output_path).expanduser()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered + "\n", encoding="utf-8")
    return rendered


def main() -> int:
    args = parse_args()
    path = Path(args.step).expanduser()
    if not path.is_file():
        report = {
            "$schema": "assembly-ontology.step-preflight/v1",
            "status": "HOLD_INPUT_MISSING",
            "step": str(path),
            "holds": ["SOURCE_STEP_NOT_FOUND"],
        }
        print(write_report(report, args.output))
        return 2
    if path.suffix.casefold() not in {".step", ".stp", ".p21"}:
        suffix_warning = "UNEXPECTED_STEP_SUFFIX"
    else:
        suffix_warning = None

    try:
        step = inspect_step(path)
    except OSError as exc:
        report = {
            "$schema": "assembly-ontology.step-preflight/v1",
            "status": "HOLD_INPUT_UNREADABLE",
            "step": str(path.resolve()),
            "holds": ["SOURCE_STEP_READ_FAILED"],
            "error": str(exc),
        }
        print(write_report(report, args.output))
        return 2
    sop, missing_sop = existing_records(args.sop, "sop")
    bom, missing_bom = existing_records(args.bom, "bom")
    missing = missing_sop + missing_bom
    status, holds = decide(step, sop, bom, missing)
    warnings = [suffix_warning] if suffix_warning else []
    report = {
        "$schema": "assembly-ontology.step-preflight/v1",
        "captured_at": datetime.now(timezone.utc).isoformat(),
        "status": status,
        "step": step,
        "controlled_records": {"sop": sop, "bom": bom},
        "missing_declared_records": missing,
        "holds": sorted(set(holds)),
        "warnings": warnings,
        "next_stage": "S0_RECORD_VALIDATION" if sop or bom else "S1_GEOMETRY_PRECHECK",
        "claim_boundary": [
            "Text preflight does not parse or validate B-Rep geometry.",
            "Entity counts are intake signals, not occurrence identities.",
            "No transform, interface, path, collision, tool-access or physical-release claim is made.",
            "STEP alone does not establish actual assembly order, tool, torque, adhesive, acceptance or left/right semantics.",
            "BOM and SOP files are only hashed at preflight; their schema, authority and content are not yet validated.",
        ],
    }
    rendered = write_report(report, args.output)
    if args.json:
        print(rendered)
    else:
        print(f"status: {status}")
        print(f"step: {step['path']}")
        print(f"sha256: {step['sha256']}")
        print(f"schema: {', '.join(step['file_schema']) or 'NOT_OBSERVED'}")
        print(f"assembly signals: {step['assembly_structure_signal_count']}")
        print(f"geometry signals: {step['geometry_signal_count']}")
        print(f"controlled BOM records: {len(bom)}")
        print(f"controlled SOP records: {len(sop)}")
        if holds:
            print("holds: " + ", ".join(sorted(set(holds))))
        if warnings:
            print("warnings: " + ", ".join(warnings))
        print("scope: intake signals only; run an independent STEP/XCAF geometry chain before S1 claims")
    return 0 if not status.startswith("HOLD_") else 3


if __name__ == "__main__":
    raise SystemExit(main())
