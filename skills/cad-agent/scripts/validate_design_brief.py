#!/usr/bin/env python3
"""Validate a formal/high-risk agentic CAD design brief before P3 work or release."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Any

ALLOWED_STATUSES = {"given", "derived", "assumed", "recommended", "unknown"}
RELEASE_STATES = {"draft", "modelled", "checked", "verified-for-prototype", "released"}
RISK_LEVELS = {"low", "moderate", "high"}


def load_yaml(path: Path) -> tuple[dict[str, Any] | None, str | None]:
    try:
        import yaml  # type: ignore
    except ImportError:
        return None, "PyYAML is not installed; using conservative text checks only."
    try:
        data = yaml.safe_load(path.read_text(encoding="utf-8"))
    except Exception as exc:  # pragma: no cover - parser message is useful to user
        raise ValueError(f"YAML parse failed: {exc}") from exc
    if not isinstance(data, dict):
        raise ValueError("The YAML root must be a mapping.")
    return data, None


def walk(node: Any, path: str = ""):
    if isinstance(node, dict):
        yield path, node
        for key, value in node.items():
            child = f"{path}.{key}" if path else str(key)
            yield from walk(value, child)
    elif isinstance(node, list):
        for index, value in enumerate(node):
            yield from walk(value, f"{path}[{index}]")


def validate_structured(data: dict[str, Any], release: bool) -> list[str]:
    errors: list[str] = []
    warnings: list[str] = []

    job = data.get("job", {})
    if not isinstance(job, dict):
        errors.append("job must be a mapping")
    else:
        for key in ("id", "name", "owner", "risk_level", "revision"):
            if not job.get(key):
                errors.append(f"job.{key} is required")
        state = job.get("status")
        if state not in RELEASE_STATES:
            errors.append(f"job.status must be one of {sorted(RELEASE_STATES)}")
        risk_level = job.get("risk_level")
        if risk_level not in RISK_LEVELS:
            errors.append(f"job.risk_level must be one of {sorted(RISK_LEVELS)}")

    units = data.get("units", {})
    if not isinstance(units, dict) or not units:
        errors.append("units must define the project's explicit unit system")

    for location, mapping in walk(data):
        if (
            not isinstance(mapping, dict)
            or "status" not in mapping
            or location == "job"
        ):
            continue
        status = mapping.get("status")
        if status not in ALLOWED_STATUSES:
            errors.append(f"{location}.status has invalid value {status!r}")
            continue
        value = mapping.get("value")
        source = mapping.get("source")
        if status in {"given", "derived", "assumed", "recommended"} and not source:
            errors.append(f"{location}.source is required for status {status!r}")
        if status in {"given", "derived", "assumed"} and value in (None, ""):
            errors.append(f"{location}.value is required for status {status!r}")
        if status == "unknown":
            warnings.append(f"{location} remains unknown")
        if status == "recommended":
            warnings.append(f"{location} is recommended and awaits acceptance")
        if status == "assumed":
            warnings.append(f"{location} is assumed and requires closure evidence")

    parameters = data.get("parameters", [])
    if not isinstance(parameters, list):
        errors.append("parameters must be a list")
    else:
        names: set[str] = set()
        for index, parameter in enumerate(parameters):
            if not isinstance(parameter, dict):
                errors.append(f"parameters[{index}] must be a mapping")
                continue
            name = parameter.get("name")
            if not name:
                errors.append(f"parameters[{index}].name is required")
            elif name in names:
                errors.append(f"duplicate parameter name: {name}")
            else:
                names.add(str(name))
            if name == "example_parameter":
                warnings.append("replace or delete parameters[0] example_parameter")

    if release:
        blocking_tokens = {"unknown", "recommended", "assumed"}
        for location, mapping in walk(data):
            if isinstance(mapping, dict) and mapping.get("status") in blocking_tokens:
                errors.append(
                    f"release blocked: {location}.status is {mapping.get('status')!r}"
                )
        approvals = data.get("approvals", {})
        if not isinstance(approvals, dict) or not approvals.get(
            "manufacturing_release_approved_by"
        ):
            errors.append("release blocked: manufacturing release approval is missing")

    return [*(f"ERROR: {item}" for item in errors), *(f"WARN: {item}" for item in warnings)]


def validate_text(text: str, release: bool) -> list[str]:
    messages: list[str] = []
    required_keys = (
        "job:",
        "intent:",
        "units:",
        "coordinate_system:",
        "interfaces:",
        "parameters:",
        "verification:",
        "approvals:",
    )
    for key in required_keys:
        if key not in text:
            messages.append(f"ERROR: missing required section {key}")
    unknown_count = len(re.findall(r'^\s*status:\s*["\']?unknown["\']?\s*$', text, re.M))
    recommended_count = len(
        re.findall(r'^\s*status:\s*["\']?recommended["\']?\s*$', text, re.M)
    )
    assumed_count = len(re.findall(r'^\s*status:\s*["\']?assumed["\']?\s*$', text, re.M))
    if unknown_count:
        messages.append(f"WARN: {unknown_count} field(s) remain unknown")
    if recommended_count:
        messages.append(f"WARN: {recommended_count} recommendation(s) await acceptance")
    if assumed_count:
        messages.append(f"WARN: {assumed_count} assumption(s) require closure evidence")
    if "example_parameter" in text:
        messages.append("WARN: replace or delete the example parameter")
    if release and (unknown_count or recommended_count or assumed_count):
        messages.append("ERROR: release blocked by unresolved parameter/input status")
    return messages


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate a CAD design-brief YAML file.")
    parser.add_argument("brief", help="Path to design-brief.yaml")
    parser.add_argument(
        "--release",
        action="store_true",
        help="Apply strict manufacturing-release gates",
    )
    args = parser.parse_args()

    path = Path(args.brief).expanduser().resolve()
    if not path.is_file():
        print(f"ERROR: brief not found: {path}", file=sys.stderr)
        return 2

    text = path.read_text(encoding="utf-8")
    try:
        data, fallback_notice = load_yaml(path)
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    if data is None:
        print(f"NOTICE: {fallback_notice}")
        messages = validate_text(text, args.release)
    else:
        messages = validate_structured(data, args.release)

    if messages:
        print("\n".join(messages))
    else:
        print("PASS: design brief passed all selected checks")

    return 1 if any(message.startswith("ERROR:") for message in messages) else 0


if __name__ == "__main__":
    raise SystemExit(main())
