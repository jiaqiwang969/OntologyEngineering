"""Fail-closed validator for acceptance-contract.v1.json packets.

The validator is intentionally generic: a future assembly run normalizes its
evidence into assembly-ontology.acceptance-packet/v1, then this script evaluates
the immutable packet and re-hashes every declared artifact.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


MISSING = object()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def get_path(document: dict[str, Any], dotted: str) -> Any:
    value: Any = document
    for key in dotted.split("."):
        if not isinstance(value, dict) or key not in value:
            return MISSING
        value = value[key]
    return value


def artifact_check(
    packet: dict[str, Any], root: Path, required_roles: list[str]
) -> tuple[str, dict[str, Any]]:
    rows = packet.get("artifacts")
    if not isinstance(rows, list):
        return "UNKNOWN", {"reason": "artifacts is missing or is not a list"}
    roles = [str(row.get("role", "")) for row in rows if isinstance(row, dict)]
    missing_roles = sorted(set(required_roles) - set(roles))
    failures: list[dict[str, Any]] = []
    unknowns: list[dict[str, Any]] = []
    for row in rows:
        if not isinstance(row, dict):
            failures.append({"reason": "artifact row is not an object"})
            continue
        raw_path = row.get("path")
        expected = row.get("sha256")
        if not isinstance(raw_path, str) or not isinstance(expected, str):
            unknowns.append({"role": row.get("role"), "reason": "path or sha256 missing"})
            continue
        target = Path(raw_path)
        if not target.is_absolute():
            target = root / target
        if not target.is_file():
            failures.append({"role": row.get("role"), "path": str(target), "reason": "file missing"})
            continue
        actual = sha256(target)
        if actual != expected:
            failures.append({
                "role": row.get("role"),
                "path": str(target),
                "expected": expected,
                "actual": actual,
            })
    if missing_roles:
        failures.append({"reason": "required artifact roles missing", "roles": missing_roles})
    if failures:
        return "FAIL", {"failures": failures, "unknowns": unknowns}
    if unknowns:
        return "UNKNOWN", {"unknowns": unknowns}
    return "PASS", {"checked": len(rows), "required_roles": required_roles}


def evaluate(
    check: dict[str, Any], packet: dict[str, Any], root: Path
) -> tuple[str, dict[str, Any]]:
    operator = check["operator"]
    path = check.get("path")
    actual = get_path(packet, path) if isinstance(path, str) else MISSING
    if operator == "artifact_hashes_valid":
        return artifact_check(packet, root, list(check.get("required_roles", [])))
    if actual is MISSING:
        return "UNKNOWN", {"reason": "field missing", "path": path}
    if operator == "eq":
        expected = check.get("expected")
        passed = actual == expected
    elif operator == "gt":
        expected = check.get("expected")
        passed = isinstance(actual, (int, float)) and actual > expected
    elif operator == "same_as":
        expected_path = check["expected_path"]
        expected = get_path(packet, expected_path)
        if expected is MISSING:
            return "UNKNOWN", {"reason": "comparison field missing", "path": expected_path}
        passed = actual == expected
    elif operator == "empty":
        expected = []
        passed = isinstance(actual, (list, dict, str)) and len(actual) == 0
    elif operator == "sum_fields_eq":
        values = [get_path(packet, item) for item in check.get("sum_paths", [])]
        if any(item is MISSING for item in values):
            return "UNKNOWN", {"reason": "sum field missing", "paths": check.get("sum_paths", [])}
        if not all(isinstance(item, (int, float)) for item in values):
            return "FAIL", {"reason": "sum fields are not numeric", "values": values}
        expected = sum(values)
        passed = actual == expected
    else:
        return "UNKNOWN", {"reason": "unsupported operator", "operator": operator}
    return (
        "PASS" if passed else "FAIL",
        {"path": path, "operator": operator, "actual": actual, "expected": expected},
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("packet", type=Path)
    parser.add_argument(
        "--contract",
        type=Path,
        default=Path(__file__).with_name("acceptance-contract.v1.json"),
    )
    parser.add_argument("--profile", choices=("engineering-film", "physical-release"), default="engineering-film")
    parser.add_argument("--root", type=Path, help="Root for relative artifact paths; defaults to packet directory")
    args = parser.parse_args()

    packet = json.loads(args.packet.read_text(encoding="utf-8"))
    contract = json.loads(args.contract.read_text(encoding="utf-8"))
    root = (args.root or args.packet.parent).resolve()
    schema = packet.get("$schema")
    schema_ok = schema == contract["packet_schema"]
    results: list[dict[str, Any]] = []
    failures = 0
    unknowns = 0
    for gate in contract["gates"]:
        if args.profile not in gate["applies_to"]:
            continue
        check_rows = []
        for check in gate["checks"]:
            decision, detail = evaluate(check, packet, root)
            failures += decision == "FAIL"
            unknowns += decision == "UNKNOWN"
            check_rows.append({"id": check["id"], "decision": decision, "detail": detail})
        gate_decision = "FAIL" if any(row["decision"] == "FAIL" for row in check_rows) else (
            "UNKNOWN" if any(row["decision"] == "UNKNOWN" for row in check_rows) else "PASS"
        )
        results.append({"id": gate["id"], "decision": gate_decision, "checks": check_rows})
    if not schema_ok:
        failures += 1
    decision = "REJECTED" if failures else ("HOLD_UNKNOWN" if unknowns else "ACCEPTED")
    output = {
        "$schema": "assembly-ontology.acceptance-validation-result/v1",
        "contract_id": contract["contract_id"],
        "profile": args.profile,
        "packet": str(args.packet.resolve()),
        "packet_schema": schema,
        "packet_schema_valid": schema_ok,
        "decision": decision,
        "failure_count": failures,
        "unknown_count": unknowns,
        "gates": results,
    }
    print(json.dumps(output, ensure_ascii=False, indent=2))
    return 0 if decision == "ACCEPTED" else 2


if __name__ == "__main__":
    raise SystemExit(main())
