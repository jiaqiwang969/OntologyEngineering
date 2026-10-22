#!/usr/bin/env python3
"""Audit LCEDA/JLC component model evidence folders."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_sha256sums(path: Path) -> list[dict]:
    rows = []
    if not path.is_file():
        return rows
    for line_no, raw in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        line = raw.strip()
        if not line:
            continue
        parts = line.split(None, 1)
        if len(parts) != 2:
            rows.append({"line": line_no, "status": "malformed", "raw": raw})
            continue
        expected, rel = parts
        rel = rel.strip()
        target = (path.parent / rel).resolve()
        if not target.is_file():
            rows.append({"line": line_no, "status": "missing", "path": rel, "expected": expected})
            continue
        actual = sha256(target)
        rows.append(
            {
                "line": line_no,
                "status": "ok" if actual == expected else "mismatch",
                "path": rel,
                "expected": expected,
                "actual": actual,
            }
        )
    return rows


def obj_summary(path: Path) -> dict:
    vertices = []
    faces = 0
    with path.open("r", encoding="utf-8", errors="replace") as f:
        for raw in f:
            if raw.startswith("v "):
                parts = raw.split()
                if len(parts) >= 4:
                    try:
                        vertices.append((float(parts[1]), float(parts[2]), float(parts[3])))
                    except ValueError:
                        pass
            elif raw.startswith("f "):
                faces += 1
    result = {"path": str(path), "bytes": path.stat().st_size, "vertices": len(vertices), "faces": faces}
    if vertices:
        xs, ys, zs = zip(*vertices)
        result["bbox_min"] = [min(xs), min(ys), min(zs)]
        result["bbox_max"] = [max(xs), max(ys), max(zs)]
        result["bbox_size"] = [max(xs) - min(xs), max(ys) - min(ys), max(zs) - min(zs)]
    return result


def audit_folder(folder: Path) -> dict:
    folder = folder.resolve()
    if not folder.is_dir():
        return {
            "folder": str(folder),
            "status": "error",
            "errors": ["folder_missing"],
            "warnings": [],
            "counts": {
                "obj": 0,
                "step_or_stp": 0,
                "derived_step": 0,
                "step_report_json": 0,
                "sha256_rows": 0,
            },
            "obj_summaries": [],
            "sha256": [],
        }
    files = [p for p in folder.iterdir() if p.is_file()]
    obj_files = sorted(p for p in files if p.suffix.lower() == ".obj")
    step_files = sorted(p for p in files if p.suffix.lower() in {".step", ".stp"})
    derived_steps = sorted(p for p in step_files if ".derived-from-obj" in p.name)
    report_files = sorted(p for p in files if p.name.endswith(".step-report.json"))
    sha_file = folder / "SHA256SUMS"

    errors = []
    warnings = []
    if not obj_files and not step_files:
        errors.append("no_obj_step_or_stp_model")
    if not sha_file.is_file():
        errors.append("missing_SHA256SUMS")
    if obj_files and not step_files:
        warnings.append("obj_present_without_step_or_derived_step_candidate")
    if derived_steps and not report_files:
        warnings.append("derived_step_present_without_step_report_json")

    sha_rows = parse_sha256sums(sha_file)
    for row in sha_rows:
        if row["status"] in {"missing", "mismatch", "malformed"}:
            errors.append(f"sha256_{row['status']}:{row.get('path', row.get('line'))}")

    objs = [obj_summary(p) for p in obj_files]
    for obj in objs:
        if obj["vertices"] == 0 or obj["faces"] == 0:
            errors.append(f"obj_has_no_geometry:{Path(obj['path']).name}")

    return {
        "folder": str(folder),
        "status": "ok" if not errors else "error",
        "errors": errors,
        "warnings": warnings,
        "counts": {
            "obj": len(obj_files),
            "step_or_stp": len(step_files),
            "derived_step": len(derived_steps),
            "step_report_json": len(report_files),
            "sha256_rows": len(sha_rows),
        },
        "obj_summaries": objs,
        "sha256": sha_rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit LCEDA/JLC model evidence folders.")
    parser.add_argument("folders", nargs="+", type=Path)
    parser.add_argument("--pretty", action="store_true", help="Pretty-print JSON output.")
    args = parser.parse_args()

    reports = [audit_folder(folder) for folder in args.folders]
    payload = {
        "status": "ok" if all(r["status"] == "ok" for r in reports) else "error",
        "folders": reports,
    }
    print(json.dumps(payload, ensure_ascii=False, indent=2 if args.pretty else None))
    return 0 if payload["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
