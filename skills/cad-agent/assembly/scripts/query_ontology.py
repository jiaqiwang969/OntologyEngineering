#!/usr/bin/env python3
"""Query the bundled assembly-engineering ontology without project dependencies."""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any


SKILL_ROOT = Path(__file__).resolve().parents[1]
ONTOLOGY_ROOT = SKILL_ROOT / "references" / "ontology"
FILES = {
    "criteria": ONTOLOGY_ROOT / "criteria.v1.json",
    "failures": ONTOLOGY_ROOT / "failures.v1.json",
    "pipeline": ONTOLOGY_ROOT / "pipeline.v1.json",
    "tools": ONTOLOGY_ROOT / "tool-capabilities.v1.json",
    "open_items": ONTOLOGY_ROOT / "open-items.v1.json",
    "manifest": ONTOLOGY_ROOT / "snapshot-manifest.v1.json",
    "algorithm_policy": ONTOLOGY_ROOT / "algorithm-physics-routing-policy.v1.json",
}

AUTHORITY = {
    "scope": "historical reusable method memory",
    "not_authority_for": "target-project facts, target thresholds, physical release, Semantica publication or approval",
    "portability_rule": "Revalidate applicability and numeric thresholds on the target assembly.",
}


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def load_all() -> dict[str, Any]:
    return {key: load_json(path) for key, path in FILES.items()}


def emit(value: Any, as_json: bool) -> None:
    if as_json:
        print(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True))
        return
    if isinstance(value, str):
        print(value)
        return
    print(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=False))


def overview(data: dict[str, Any]) -> dict[str, Any]:
    criteria = data["criteria"]["criteria"]
    failures = data["failures"]["failures"]
    stages = data["pipeline"]["stages"]
    tools = data["tools"]["tools"]
    open_items = data["open_items"]["items"]
    conditional_branches = data["pipeline"].get("conditional_branches", [])
    return {
        "skill": "assembly-ontology",
        "snapshot": data["manifest"]["snapshot_id"],
        "criteria": {
            "count": len(criteria),
            "by_status": dict(
                sorted(
                    Counter(item["validation"]["status"] for item in criteria).items()
                )
            ),
            "by_domain": dict(
                sorted(Counter(item["domain"] for item in criteria).items())
            ),
        },
        "failures": {
            "count": len(failures),
            "by_domain": dict(
                sorted(Counter(item["domain"] for item in failures).items())
            ),
        },
        "pipeline": [{"id": item["id"], "name": item["name"]} for item in stages],
        "conditional_routes": [
            {"id": item["id"], "name": item["name"]} for item in conditional_branches
        ],
        "tool_capabilities": len(tools),
        "open_items": {
            "count": len(open_items),
            "by_severity": dict(
                sorted(Counter(item["severity"] for item in open_items).items())
            ),
        },
        "algorithm_physics_policy": data["algorithm_policy"]["policy_id"],
        "authority_boundary": "Bundled snapshot is reusable method memory, not current project fact or physical release authority.",
    }


def bounded(value: Any, record_type: str) -> dict[str, Any]:
    key = "records" if isinstance(value, list) else "record"
    return {key: value, "record_type": record_type, "_authority": AUTHORITY}


def stage_record(item: dict[str, Any]) -> dict[str, Any]:
    result = dict(item)
    historical_status = result.pop("orbita_status", None)
    if historical_status is not None:
        result["historical_case_status"] = {
            "case": "Orbita",
            "status": historical_status,
            "target_project_status": False,
        }
    return result


def index_by_id(items: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {item["id"]: item for item in items}


def search(data: dict[str, Any], needle: str) -> dict[str, list[dict[str, str]]]:
    q = needle.casefold()
    result: dict[str, list[dict[str, str]]] = {
        "criteria": [],
        "failures": [],
        "stages": [],
        "tools": [],
        "open_items": [],
        "algorithm_policy": [],
    }
    mappings = [
        ("criteria", data["criteria"]["criteria"], ("id", "name", "statement")),
        ("failures", data["failures"]["failures"], ("id", "symptom", "root_cause")),
        ("stages", data["pipeline"]["stages"], ("id", "name", "question")),
        ("tools", data["tools"]["tools"], ("id", "name", "answers", "does_not_answer")),
        ("open_items", data["open_items"]["items"], ("id", "title", "detail")),
    ]
    for bucket, items, fields in mappings:
        for item in items:
            haystack = json.dumps(item, ensure_ascii=False).casefold()
            if q not in haystack:
                continue
            summary = {"id": str(item.get("id", ""))}
            for field in fields[1:]:
                if field in item:
                    value = item[field]
                    if isinstance(value, list):
                        value = "; ".join(str(part) for part in value)
                    summary[field] = str(value)
                    break
            result[bucket].append(summary)
    policy = data["algorithm_policy"]
    if q in json.dumps(policy, ensure_ascii=False).casefold():
        result["algorithm_policy"].append(
            {"id": policy["policy_id"], "purpose": policy["purpose"]}
        )
    return result


def conditional_branches(data: dict[str, Any]) -> list[dict[str, Any]]:
    return data["pipeline"].get("conditional_branches", [])


def require_policy_records(policy: dict[str, Any], key: str) -> list[dict[str, Any]]:
    value = policy.get(key)
    if not isinstance(value, list) or not value:
        raise ValueError(f"algorithm policy requires a non-empty {key} array")
    if any(
        not isinstance(item, dict) or not isinstance(item.get("id"), str)
        for item in value
    ):
        raise ValueError(f"algorithm policy {key} entries require string id fields")
    return value


def references_from(records: list[dict[str, Any]], key: str) -> list[str]:
    references: set[str] = set()
    for record in records:
        value = record.get(key, [])
        if isinstance(value, str):
            references.add(value)
        elif isinstance(value, list):
            references.update(str(part) for part in value)
    return sorted(references, key=str.casefold)


def resolve_references(
    references: list[str], records: list[dict[str, Any]]
) -> tuple[list[dict[str, Any]], list[str]]:
    by_id = index_by_id(records)
    return (
        [by_id[reference] for reference in references if reference in by_id],
        [reference for reference in references if reference not in by_id],
    )


def resolve_policy_branches(
    references: list[str], route_matrix: list[dict[str, Any]]
) -> tuple[list[dict[str, Any]], list[str]]:
    by_branch: dict[str, dict[str, Any]] = {}
    for route in route_matrix:
        branch_id = route.get("branch_id")
        if not isinstance(branch_id, str):
            raise ValueError(f"route {route['id']} requires a string branch_id")
        if branch_id in by_branch:
            raise ValueError(f"duplicate route_matrix branch_id: {branch_id}")
        by_branch[branch_id] = route
    return (
        [by_branch[reference] for reference in references if reference in by_branch],
        [reference for reference in references if reference not in by_branch],
    )


def paper_algorithms(data: dict[str, Any]) -> dict[str, Any]:
    policy = data["algorithm_policy"]
    routes = require_policy_records(policy, "paper_algorithm_routes")
    route_matrix = require_policy_records(policy, "route_matrix")
    branch_refs = references_from(routes, "branch_refs")
    branches, unresolved_branches = resolve_policy_branches(branch_refs, route_matrix)
    return {
        "policy_id": policy["policy_id"],
        "planning_admission_invariants": policy["planning_admission_invariants"],
        "paper_algorithm_routes": routes,
        "resolved_route_matrix_branches": branches,
        "unresolved_branch_refs": unresolved_branches,
        "claim_boundary": (
            "Every paper route produces a target-project candidate only; ordered absolute "
            "states, q0, mover/fixed identity, path checks, and downstream gates remain due."
        ),
    }


def physics_lanes(data: dict[str, Any]) -> dict[str, Any]:
    policy = data["algorithm_policy"]
    routes = require_policy_records(policy, "route_matrix")
    tool_refs = references_from(routes, "tool_refs")
    tools, unresolved_tools = resolve_references(tool_refs, data["tools"]["tools"])
    return {
        "policy_id": policy["policy_id"],
        "route_matrix": routes,
        "resolved_tools": tools,
        "unresolved_tool_refs": unresolved_tools,
        "s9_fair_comparison_facets": policy["s9_fair_comparison_facets"],
        "contact_physics_invariants": policy["contact_physics_invariants"],
        "routing_rules": policy["routing_rules"],
        "overclaim_guards": policy["overclaim_guards"],
        "claim_boundary": (
            "S9 ranks short-prefix directions only; full contact, threaded contact, and "
            "S11 physical falsification are distinct conditional routes."
        ),
    }


def route_for_operation(data: dict[str, Any], operation_class: str) -> dict[str, Any]:
    requested = operation_class.strip()
    if not requested:
        raise ValueError("--operation-class must not be empty")
    normalized = requested.casefold()
    policy = data["algorithm_policy"]
    routes = require_policy_records(policy, "route_matrix")

    def operation_classes(item: dict[str, Any]) -> list[str]:
        value = item.get("operation_classes", [])
        if isinstance(value, str):
            return [value]
        if isinstance(value, list):
            return [str(part) for part in value]
        raise ValueError(
            f"route {item['id']} operation_classes must be a string or array"
        )

    known_classes = sorted(
        {value for item in routes for value in operation_classes(item)},
        key=str.casefold,
    )

    def matches(item: dict[str, Any]) -> bool:
        return any(value.casefold() == normalized for value in operation_classes(item))

    matching_routes = [item for item in routes if matches(item)]
    tool_refs = references_from(matching_routes, "tool_refs")
    matching_tools, unresolved_tools = resolve_references(
        tool_refs, data["tools"]["tools"]
    )
    matched = bool(matching_routes)
    return {
        "requested_operation_class": requested,
        "route_status": (
            "CANDIDATE_ROUTE_REQUIRES_PROJECT_EVIDENCE"
            if matched
            else "HOLD_UNKNOWN_OPERATION_CLASS"
        ),
        "matching_routes": matching_routes,
        "resolved_tools": matching_tools,
        "unresolved_tool_refs": unresolved_tools,
        "known_operation_classes": known_classes,
        "routing_rules": policy["routing_rules"],
        "upstream_admission": (
            "Occurrence identity, BOM/interface/connection-tree scope, q0, world transforms, "
            "mechanism class, and rigid/flexible partition must be closed first."
        ),
        "authority_boundary": (
            "A route match selects checks; it is not an execution result, path certificate, "
            "force certificate, or physical release."
        ),
    }


def thresholds(data: dict[str, Any]) -> list[dict[str, Any]]:
    rows = []
    for item in data["criteria"]["criteria"]:
        if item.get("thresholds"):
            rows.append(
                {
                    "id": item["id"],
                    "domain": item["domain"],
                    "name": item["name"],
                    "thresholds": item["thresholds"],
                    "validation_status": item["validation"]["status"],
                    "portability_rule": "Recalibrate on the target assembly unless the criterion evidence explicitly proves scale independence.",
                }
            )
    return rows


def parse_args() -> argparse.Namespace:
    raw_args = sys.argv[1:]
    as_json = "--json" in raw_args
    raw_args = [argument for argument in raw_args if argument != "--json"]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--json",
        action="store_true",
        help="emit stable JSON; accepted before or after the subcommand",
    )
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("overview", help="summarize the ontology snapshot")

    criterion = sub.add_parser("criterion", help="show a criterion or list a domain")
    criterion.add_argument("id", nargs="?")
    criterion.add_argument("--domain")

    failure = sub.add_parser("failure", help="show one recorded failure")
    failure.add_argument("id")

    stage = sub.add_parser("stage", help="show one S0-S11 stage")
    stage.add_argument("id")

    tool = sub.add_parser("tool", help="show a tool capability boundary")
    tool.add_argument("id")

    sub.add_parser(
        "paper-algorithms",
        help="show paper-derived planning routes and their admission invariants",
    )
    sub.add_parser(
        "physics-lanes",
        help="show S9, full-contact, threaded-contact, and physical-trial boundaries",
    )
    route = sub.add_parser(
        "route",
        help="select candidate tools and branches for an exact operation class",
    )
    route.add_argument("--operation-class", required=True)

    sub.add_parser("thresholds", help="list all criteria containing thresholds")
    sub.add_parser("category-errors", help="list category errors in tool selection")

    open_items = sub.add_parser(
        "open-items", help="list inherited method limits and unresolved items"
    )
    open_items.add_argument("--severity")

    find = sub.add_parser("search", help="search all bundled ontology records")
    find.add_argument("text")
    args = parser.parse_args(raw_args)
    args.json = as_json
    return args


def main() -> int:
    args = parse_args()
    try:
        data = load_all()
        if args.command == "overview":
            value: Any = overview(data)
        elif args.command == "criterion":
            items = data["criteria"]["criteria"]
            if args.id:
                value = bounded(index_by_id(items)[args.id.upper()], "criterion")
            elif args.domain:
                value = bounded(
                    [
                        item
                        for item in items
                        if item["domain"].casefold() == args.domain.casefold()
                    ],
                    "criterion",
                )
            else:
                raise ValueError("criterion requires an ID or --domain")
        elif args.command == "failure":
            value = bounded(
                index_by_id(data["failures"]["failures"])[args.id.upper()], "failure"
            )
        elif args.command == "stage":
            value = bounded(
                stage_record(index_by_id(data["pipeline"]["stages"])[args.id.upper()]),
                "pipeline_stage",
            )
        elif args.command == "tool":
            tools_by_id = {
                item["id"].casefold(): item for item in data["tools"]["tools"]
            }
            value = bounded(tools_by_id[args.id.casefold()], "tool_capability")
        elif args.command == "paper-algorithms":
            value = bounded(paper_algorithms(data), "paper_algorithm_routes")
        elif args.command == "physics-lanes":
            value = bounded(physics_lanes(data), "physics_lanes")
        elif args.command == "route":
            value = bounded(
                route_for_operation(data, args.operation_class),
                "algorithm_physics_route_selection",
            )
        elif args.command == "thresholds":
            value = bounded(thresholds(data), "criterion_threshold")
        elif args.command == "category-errors":
            value = bounded(data["tools"]["category_errors"], "tool_category_error")
        elif args.command == "open-items":
            items = data["open_items"]["items"]
            value = bounded(
                [
                    item
                    for item in items
                    if not args.severity
                    or item["severity"].casefold() == args.severity.casefold()
                ],
                "open_item",
            )
        elif args.command == "search":
            value = search(data, args.text)
            value["_authority"] = AUTHORITY
        else:  # pragma: no cover - argparse makes this unreachable
            raise ValueError(f"unsupported command: {args.command}")
    except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    emit(value, args.json)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
