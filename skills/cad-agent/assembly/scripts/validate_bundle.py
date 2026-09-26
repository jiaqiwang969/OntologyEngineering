#!/usr/bin/env python3
"""Validate the installed assembly-ontology snapshot without project dependencies."""

from __future__ import annotations

import hashlib
import json
import re
from collections import Counter
from pathlib import Path
from typing import Any


SKILL_ROOT = Path(__file__).resolve().parents[1]
ONTOLOGY_ROOT = SKILL_ROOT / "references" / "ontology"
DATA_FILES = {
    "criteria": "criteria.v1.json",
    "failures": "failures.v1.json",
    "pipeline": "pipeline.v1.json",
    "tools": "tool-capabilities.v1.json",
    "open_items": "open-items.v1.json",
    "algorithm_policy": "algorithm-physics-routing-policy.v1.json",
}
MANIFEST_FILE = "snapshot-manifest.v1.json"
EXPECTED_SCHEMAS = {
    "criteria": "assembly.engineering-ontology.criteria/v1",
    "failures": "assembly.engineering-ontology.failure-catalog/v1",
    "pipeline": "assembly.engineering-ontology.pipeline/v1",
    "tools": "assembly.engineering-ontology.tool-matrix/v1",
    "open_items": "assembly.engineering-ontology.open-items/v1",
    "algorithm_policy": "cad-agent.assembly-algorithm-physics-routing-policy/v1",
}

CRITERION_ID = re.compile(
    r"^K-(ID|IF|KIN|DIR|PATH|TOOL|PHY|REAL|SCN|DEL|GOV)-[0-9]{2}$"
)
FAILURE_ID = re.compile(
    r"^F-(ID|IF|KIN|DIR|PATH|TOOL|PHY|REAL|SCN|DEL|GOV|ALG|GEO)-[0-9]{2}$"
)
CAPABILITY_STATUSES = {
    "PINNED_EXECUTABLE",
    "LOCAL_ADAPTER",
    "CASE_PROVEN_NOT_GENERALIZED",
    "LITERATURE_ONLY",
    "NOT_EXECUTED",
}
NEW_CRITERION_FAILURE_LINKS = {
    "K-PATH-16": "F-ALG-10",
    "K-PATH-17": "F-ALG-11",
    "K-KIN-05": "F-ALG-12",
    "K-SCN-09": "F-SCN-13",
    "K-PHY-06": "F-PHY-02",
    "K-PHY-07": "F-PHY-03",
    "K-PHY-08": "F-PHY-04",
    "K-PHY-09": "F-PHY-05",
    "K-PHY-10": "F-PHY-06",
    "K-PHY-11": "F-PHY-07",
    "K-PHY-12": "F-PHY-08",
    "K-REAL-01": "F-REAL-01",
    "K-REAL-02": "F-REAL-02",
    "K-REAL-03": "F-REAL-03",
    "K-REAL-04": "F-REAL-04",
    "K-REAL-05": "F-REAL-05",
}
PAB_IDS = {f"PAB-{index:02d}" for index in range(1, 13)}
S9_FACET_IDS = {f"S9-FC-{index:02d}" for index in range(1, 11)}
CONTACT_PHYSICS_IDS = {f"CP-{index:02d}" for index in range(1, 13)}
PAPER_ALGORITHM_IDS = {
    "AAMERI_STATIC_SEMANTICS",
    "AGRAWALA_ACTION_GRAPH",
    "LI_DAG_SHORT_ESCAPE",
    "ATA_REVERSE_DISASSEMBLY",
    "ASAP_STABILITY",
    "SBDP_DBG_INSPIRED",
    "IPC_LIBUIPC",
    "FACTORY_THREADED_CONTACT",
}
ROUTE_MATRIX_IDS = {
    "ROUTE-S7-GEOMETRY",
    "ROUTE-S9-DIRECTION",
    "ROUTE-P-CONTACT",
    "ROUTE-THREADED-CONTACT",
    "ROUTE-S11-PHYSICAL",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(filename: str, errors: list[str]) -> dict[str, Any] | None:
    path = ONTOLOGY_ROOT / filename
    try:
        with path.open("r", encoding="utf-8") as handle:
            value = json.load(handle)
    except OSError as exc:
        errors.append(f"{filename}: cannot read: {exc}")
        return None
    except json.JSONDecodeError as exc:
        errors.append(
            f"{filename}: invalid JSON at line {exc.lineno}, column {exc.colno}: {exc.msg}"
        )
        return None
    if not isinstance(value, dict):
        errors.append(f"{filename}: root must be an object")
        return None
    return value


def record_list(
    document: dict[str, Any], key: str, label: str, errors: list[str]
) -> list[dict[str, Any]]:
    value = document.get(key)
    if not isinstance(value, list):
        errors.append(f"{label}: .{key} must be an array")
        return []
    records: list[dict[str, Any]] = []
    for index, item in enumerate(value):
        if not isinstance(item, dict):
            errors.append(f"{label}[{index}]: record must be an object")
        else:
            records.append(item)
    return records


def unique_ids(items: list[dict[str, Any]], label: str, errors: list[str]) -> set[str]:
    raw_ids = [item.get("id") for item in items]
    missing = sum(not isinstance(item, str) or not item for item in raw_ids)
    if missing:
        errors.append(f"{label}: {missing} records lack a non-empty string id")
    counts = Counter(item for item in raw_ids if isinstance(item, str) and item)
    duplicates = sorted(item for item, count in counts.items() if count > 1)
    if duplicates:
        errors.append(f"{label}: duplicate ids {duplicates}")
    return set(counts)


def require_exact_record_ids(
    document: dict[str, Any],
    key: str,
    expected: set[str],
    label: str,
    errors: list[str],
) -> list[dict[str, Any]]:
    records = record_list(document, key, label, errors)
    observed = unique_ids(records, label, errors)
    if observed != expected:
        errors.append(f"{label}: ids {sorted(observed)}, expected {sorted(expected)}")
    return records


def require_fields(
    item: dict[str, Any], fields: tuple[str, ...], label: str, errors: list[str]
) -> None:
    for field in fields:
        if field not in item:
            errors.append(f"{label}: missing field {field}")


def require_local_reference(raw: Any, label: str, errors: list[str]) -> None:
    if not isinstance(raw, str) or not raw:
        errors.append(f"{label}: expected a non-empty local reference")
        return
    path_text = raw.split("#", 1)[0]
    path = (ONTOLOGY_ROOT / path_text).resolve()
    if not path.is_file():
        errors.append(f"{label}: local reference does not exist: {raw}")


def emit(status: str, errors: list[str], warnings: list[str], **extra: Any) -> int:
    result = {
        "status": status,
        **extra,
        "errors": errors,
        "warnings": warnings,
        "claim_boundary": "This validates bundled JSON structure, links and hashes; it does not validate any target STEP, historical evidence payload or physical process.",
    }
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if status == "PASS" else 1


def main() -> int:
    errors: list[str] = []
    warnings: list[str] = []
    documents = {
        key: read_json(filename, errors) for key, filename in DATA_FILES.items()
    }
    manifest = read_json(MANIFEST_FILE, errors)
    if errors or manifest is None or any(value is None for value in documents.values()):
        return emit("FAIL", errors, warnings)

    docs = {key: value for key, value in documents.items() if value is not None}
    for key, expected in EXPECTED_SCHEMAS.items():
        observed = docs[key].get("$schema")
        if observed != expected:
            errors.append(
                f"{DATA_FILES[key]}: $schema {observed!r}, expected {expected!r}"
            )

    criteria = record_list(docs["criteria"], "criteria", "criteria", errors)
    failures = record_list(docs["failures"], "failures", "failures", errors)
    stages = record_list(docs["pipeline"], "stages", "stages", errors)
    tools = record_list(docs["tools"], "tools", "tools", errors)
    open_items = record_list(docs["open_items"], "items", "open_items", errors)
    criterion_ids = unique_ids(criteria, "criteria", errors)
    failure_ids = unique_ids(failures, "failures", errors)
    stage_ids = unique_ids(stages, "stages", errors)
    tool_ids = unique_ids(tools, "tools", errors)
    open_item_ids = unique_ids(open_items, "open_items", errors)

    for criterion_id in sorted(criterion_ids):
        if not CRITERION_ID.fullmatch(criterion_id):
            errors.append(f"criteria: invalid id format {criterion_id!r}")
    for failure_id in sorted(failure_ids):
        if not FAILURE_ID.fullmatch(failure_id):
            errors.append(f"failures: invalid id format {failure_id!r}")

    policy = docs["algorithm_policy"]
    require_fields(
        policy,
        (
            "policy_id",
            "version",
            "authority_model",
            "status_vocabularies",
            "planning_admission_invariants",
            "s9_fair_comparison_facets",
            "contact_physics_invariants",
            "paper_algorithm_routes",
            "route_matrix",
            "routing_rules",
            "claim_ledger",
            "overclaim_guards",
        ),
        "algorithm_policy",
        errors,
    )
    planning_invariants = require_exact_record_ids(
        policy,
        "planning_admission_invariants",
        PAB_IDS,
        "algorithm_policy.planning_admission_invariants",
        errors,
    )
    s9_facets = require_exact_record_ids(
        policy,
        "s9_fair_comparison_facets",
        S9_FACET_IDS,
        "algorithm_policy.s9_fair_comparison_facets",
        errors,
    )
    contact_invariants = require_exact_record_ids(
        policy,
        "contact_physics_invariants",
        CONTACT_PHYSICS_IDS,
        "algorithm_policy.contact_physics_invariants",
        errors,
    )
    paper_algorithms = require_exact_record_ids(
        policy,
        "paper_algorithm_routes",
        PAPER_ALGORITHM_IDS,
        "algorithm_policy.paper_algorithm_routes",
        errors,
    )
    route_matrix = require_exact_record_ids(
        policy,
        "route_matrix",
        ROUTE_MATRIX_IDS,
        "algorithm_policy.route_matrix",
        errors,
    )

    vocabulary = docs["criteria"].get("validation_status_vocabulary")
    if not isinstance(vocabulary, dict) or not vocabulary:
        errors.append(
            "criteria: validation_status_vocabulary must be a non-empty object"
        )
        allowed_status: set[str] = set()
    else:
        allowed_status = set(vocabulary)

    for criterion in criteria:
        label = str(criterion.get("id", "criteria<?>"))
        require_fields(
            criterion,
            (
                "name",
                "domain",
                "statement",
                "thresholds",
                "provenance",
                "validation",
                "blocks",
                "implemented_by",
            ),
            label,
            errors,
        )
        validation = criterion.get("validation")
        if not isinstance(validation, dict):
            errors.append(f"{label}: validation must be an object")
            continue
        status = validation.get("status")
        if status not in allowed_status:
            errors.append(f"{label}: invalid validation status {status!r}")
        if not validation.get("evidence"):
            errors.append(f"{label}: validation.evidence is required")
        for failure_id in (
            criterion.get("blocks", [])
            if isinstance(criterion.get("blocks"), list)
            else []
        ):
            if failure_id not in failure_ids:
                errors.append(f"{label}: unknown blocked failure {failure_id}")

    failures_by_id = {
        item["id"]: item for item in failures if item.get("id") in failure_ids
    }
    for failure in failures:
        label = str(failure.get("id", "failure<?>"))
        require_fields(
            failure,
            ("symptom", "root_cause", "blocked_by", "impact", "evidence"),
            label,
            errors,
        )
        for criterion_id in (
            failure.get("blocked_by", [])
            if isinstance(failure.get("blocked_by"), list)
            else []
        ):
            if criterion_id not in criterion_ids:
                errors.append(f"{label}: unknown blocking criterion {criterion_id}")
    for criterion in criteria:
        criterion_id = criterion.get("id")
        for failure_id in (
            criterion.get("blocks", [])
            if isinstance(criterion.get("blocks"), list)
            else []
        ):
            failure = failures_by_id.get(failure_id)
            if failure is not None and criterion_id not in failure.get(
                "blocked_by", []
            ):
                errors.append(
                    f"{criterion_id} -> {failure_id} lacks reverse blocked_by reference"
                )

    criteria_by_id = {
        item["id"]: item for item in criteria if item.get("id") in criterion_ids
    }
    for criterion_id, failure_id in NEW_CRITERION_FAILURE_LINKS.items():
        criterion = criteria_by_id.get(criterion_id)
        failure = failures_by_id.get(failure_id)
        if criterion is None:
            errors.append(
                f"required algorithm/physics criterion missing: {criterion_id}"
            )
            continue
        if failure is None:
            errors.append(f"required algorithm/physics failure missing: {failure_id}")
            continue
        if criterion.get("blocks") != [failure_id]:
            errors.append(f"{criterion_id}.blocks must be exactly [{failure_id!r}]")
        if failure.get("blocked_by") != [criterion_id]:
            errors.append(f"{failure_id}.blocked_by must be exactly [{criterion_id!r}]")
        provenance = criterion.get("provenance", [])
        if not isinstance(provenance, list) or not provenance:
            errors.append(
                f"{criterion_id}.provenance must contain local method evidence"
            )
        else:
            for index, raw in enumerate(provenance):
                require_local_reference(
                    raw, f"{criterion_id}.provenance[{index}]", errors
                )
        implemented_by = criterion.get("implemented_by", [])
        if isinstance(implemented_by, list):
            for index, raw in enumerate(implemented_by):
                require_local_reference(
                    raw, f"{criterion_id}.implemented_by[{index}]", errors
                )
        require_local_reference(
            failure.get("evidence"), f"{failure_id}.evidence", errors
        )

    for tool in tools:
        label = str(tool.get("id", "tool<?>"))
        require_fields(
            tool,
            (
                "name",
                "tier",
                "capability_status",
                "claim_ceiling",
                "applicable_operation_classes",
                "answers",
                "does_not_answer",
                "pitfalls",
                "criteria",
            ),
            label,
            errors,
        )
        if tool.get("capability_status") not in CAPABILITY_STATUSES:
            errors.append(
                f"{label}: invalid capability_status {tool.get('capability_status')!r}"
            )
        if not isinstance(tool.get("claim_ceiling"), str) or not tool.get(
            "claim_ceiling"
        ):
            errors.append(f"{label}: claim_ceiling must be a non-empty string")
        operation_classes = tool.get("applicable_operation_classes")
        if not isinstance(operation_classes, list) or not operation_classes:
            errors.append(
                f"{label}: applicable_operation_classes must be a non-empty array"
            )
        if not tool.get("does_not_answer"):
            errors.append(f"{label}: does_not_answer must be non-empty")
        for criterion_id in (
            tool.get("criteria", []) if isinstance(tool.get("criteria"), list) else []
        ):
            if criterion_id not in criterion_ids:
                errors.append(f"{label}: unknown criterion {criterion_id}")
    fidelity_tiers = docs["tools"].get("fidelity_tiers", {})
    if not isinstance(fidelity_tiers, dict):
        errors.append("tools: fidelity_tiers must be an object")
    else:
        for tier, spec in fidelity_tiers.items():
            if not isinstance(spec, dict):
                errors.append(f"fidelity_tiers.{tier}: must be an object")
                continue
            for tool_id in spec.get("工具", []):
                if tool_id not in tool_ids:
                    errors.append(f"fidelity_tiers.{tier}: unknown tool {tool_id}")

    tools_by_id = {item["id"]: item for item in tools if item.get("id") in tool_ids}
    short_probe = tools_by_id.get("T-libuipc", {})
    if short_probe.get("claim_ceiling") != "DIRECTION_RANKING_ONLY":
        errors.append("T-libuipc: claim_ceiling must remain DIRECTION_RANKING_ONLY")
    fusion_pin = str(tools_by_id.get("T-fusion", {}).get("version_pin", ""))
    if "127.0.0.1" in fusion_pin or "27182" in fusion_pin:
        errors.append(
            "T-fusion: version_pin must use guarded dynamic discovery, not a fixed loopback endpoint"
        )
    required_tools = {
        "T-libuipc-contact-validation",
        "T-asap-adapter",
        "T-action-graph-adapter",
        "T-factory-thread-contact",
        "T-physical-trial",
    }
    missing_required_tools = sorted(required_tools - tool_ids)
    if missing_required_tools:
        errors.append(
            f"tools: missing algorithm/physics tools {missing_required_tools}"
        )

    for record, label in (
        *[
            (item, f"algorithm_policy.planning_admission_invariants.{item.get('id')}")
            for item in planning_invariants
        ],
        *[
            (item, f"algorithm_policy.s9_fair_comparison_facets.{item.get('id')}")
            for item in s9_facets
        ],
        *[
            (item, f"algorithm_policy.contact_physics_invariants.{item.get('id')}")
            for item in contact_invariants
        ],
    ):
        require_fields(record, ("name",), label, errors)

    route_branch_ids = {
        route.get("branch_id")
        for route in route_matrix
        if isinstance(route.get("branch_id"), str) and route.get("branch_id")
    }
    if len(route_branch_ids) != len(route_matrix):
        errors.append(
            "algorithm_policy.route_matrix: branch_id values must be unique non-empty strings"
        )
    for algorithm in paper_algorithms:
        label = f"algorithm_policy.paper_algorithm_routes.{algorithm.get('id')}"
        require_fields(
            algorithm,
            (
                "name",
                "applicability",
                "required_inputs",
                "produces",
                "does_not_prove",
                "capability_status",
                "claim_ceiling",
                "branch_refs",
            ),
            label,
            errors,
        )
        if algorithm.get("capability_status") not in CAPABILITY_STATUSES:
            errors.append(
                f"{label}: invalid capability_status {algorithm.get('capability_status')!r}"
            )
        for branch_id in (
            algorithm.get("branch_refs", [])
            if isinstance(algorithm.get("branch_refs"), list)
            else []
        ):
            if branch_id not in route_branch_ids:
                errors.append(f"{label}: unknown branch_ref {branch_id}")
    for route in route_matrix:
        label = f"algorithm_policy.route_matrix.{route.get('id')}"
        require_fields(
            route,
            (
                "branch_id",
                "trigger",
                "operation_classes",
                "required_upstream",
                "required_claims",
                "tool_refs",
                "algorithm_refs",
                "claim_ceiling",
                "on_unknown",
                "return_stage",
            ),
            label,
            errors,
        )
        for tool_id in (
            route.get("tool_refs", [])
            if isinstance(route.get("tool_refs"), list)
            else []
        ):
            if tool_id not in tool_ids:
                errors.append(f"{label}: unknown tool_ref {tool_id}")
        for algorithm_id in (
            route.get("algorithm_refs", [])
            if isinstance(route.get("algorithm_refs"), list)
            else []
        ):
            if algorithm_id not in PAPER_ALGORITHM_IDS:
                errors.append(f"{label}: unknown algorithm_ref {algorithm_id}")
        if route.get("on_unknown") != "HOLD":
            errors.append(f"{label}: on_unknown must remain HOLD")

    status_vocabularies = policy.get("status_vocabularies", {})
    portability = (
        set(status_vocabularies.get("parameter_portability", []))
        if isinstance(status_vocabularies, dict)
        else set()
    )
    if portability != {
        "STRUCTURAL_INVARIANT",
        "TARGET_SPECIFIC",
        "CASE_LOCKED_EXAMPLE",
    }:
        errors.append(
            "algorithm_policy: parameter_portability must contain the three governed values"
        )
    guards = policy.get("overclaim_guards", {})
    expected_guards = {
        "required_route_without_execution": "INCONCLUSIVE",
        "zero_contact_alone_can_be_promising": False,
        "motion_servo_strength_is_motor_capacity": False,
        "geometric_reclosure_is_mechanical_relock": False,
        "blender_claim_ceiling": "DELIVERY_VERIFIED",
        "simulation_is_physical_release": False,
        "case_locked_parameter_may_be_active": False,
    }
    for key, expected in expected_guards.items():
        if guards.get(key) != expected:
            errors.append(
                f"algorithm_policy.overclaim_guards.{key}: expected {expected!r}"
            )
    routing_rules = policy.get("routing_rules", {})
    if routing_rules.get("s9_claim_ceiling") != "DIRECTION_RANKING_ONLY":
        errors.append(
            "algorithm_policy.routing_rules.s9_claim_ceiling must be DIRECTION_RANKING_ONLY"
        )
    if routing_rules.get("physical_release_requires_fixture_trial") is not True:
        errors.append("algorithm_policy: physical release must require a fixture trial")
    required_claims = {
        "ORDERED_PLANNING",
        "GEOMETRIC_PATH",
        "DIRECTION_RANKING",
        "CONTACT_TRANSFER",
        "PASSAGE",
        "MOTOR_CAPACITY",
        "ENDPOINT_RECLOSURE",
        "MECHANICAL_RELOCK",
        "THREADED_CONTACT",
        "MEDIA",
        "PHYSICAL_RELEASE",
    }
    policy_claims = policy.get("claim_ledger", {}).get("claims", {})
    if not isinstance(policy_claims, dict) or set(policy_claims) != required_claims:
        errors.append(
            f"algorithm_policy.claim_ledger.claims must contain {sorted(required_claims)}"
        )
    authority_model = policy.get("authority_model", {})
    required_authorities = {"FUSION", "PHYSICS_SOLVER", "BLENDER", "PHYSICAL_FIXTURE"}
    if (
        not isinstance(authority_model, dict)
        or set(authority_model) != required_authorities
    ):
        errors.append(
            f"algorithm_policy.authority_model must contain {sorted(required_authorities)}"
        )
    elif not isinstance(authority_model["BLENDER"], dict):
        errors.append("algorithm_policy.authority_model.BLENDER must be an object")
    elif authority_model["BLENDER"].get("claim_ceiling") != "DELIVERY_VERIFIED":
        errors.append(
            "algorithm_policy.authority_model.BLENDER cannot exceed DELIVERY_VERIFIED"
        )

    sbdp_tool = tools_by_id.get("T-sbdp-adapter", {})
    if (
        sbdp_tool.get("implementation_identity")
        != "SBDP_DBG_INSPIRED_NOT_FULL_SBDP_IMPLEMENTATION"
    ):
        errors.append(
            "T-sbdp-adapter: implementation_identity must remain "
            "SBDP_DBG_INSPIRED_NOT_FULL_SBDP_IMPLEMENTATION"
        )

    expected_order = [f"S{index}" for index in range(12)]
    observed_order = [stage.get("id") for stage in stages]
    if observed_order != expected_order:
        errors.append(
            f"pipeline stage order is {observed_order}, expected {expected_order}"
        )
    if stage_ids != set(expected_order):
        errors.append(f"pipeline stage set is {sorted(stage_ids)}, expected S0-S11")
    referenced_criteria: set[str] = set()
    for stage in stages:
        label = str(stage.get("id", "stage<?>"))
        require_fields(
            stage,
            (
                "name",
                "question",
                "inputs",
                "outputs",
                "tools",
                "criteria",
                "hold_state",
                "orbita_status",
            ),
            label,
            errors,
        )
        for criterion_id in (
            stage.get("criteria", []) if isinstance(stage.get("criteria"), list) else []
        ):
            referenced_criteria.add(criterion_id)
            if criterion_id not in criterion_ids:
                errors.append(f"{label}: unknown criterion {criterion_id}")
        for tool_id in (
            stage.get("tools", []) if isinstance(stage.get("tools"), list) else []
        ):
            if tool_id not in tool_ids:
                errors.append(f"{label}: unknown tool {tool_id}")

    conditional_branches = record_list(
        docs["pipeline"], "conditional_branches", "conditional_branches", errors
    )
    branch_ids = unique_ids(conditional_branches, "conditional_branches", errors)
    expected_branch_ids = {
        "R-PLAN-PAPER",
        "R-P-CONTACT-VALIDATION",
        "R-P-THREADED-CONTACT",
    }
    if branch_ids != expected_branch_ids:
        errors.append(
            f"pipeline conditional branch set is {sorted(branch_ids)}, expected {sorted(expected_branch_ids)}"
        )
    for branch in conditional_branches:
        label = str(branch.get("id", "conditional_branch<?>"))
        require_fields(
            branch,
            (
                "name",
                "trigger",
                "entry_after",
                "returns_to",
                "tools",
                "criteria",
                "claim_ceiling",
                "completion_rule",
            ),
            label,
            errors,
        )
        for criterion_id in (
            branch.get("criteria", [])
            if isinstance(branch.get("criteria"), list)
            else []
        ):
            referenced_criteria.add(criterion_id)
            if criterion_id not in criterion_ids:
                errors.append(f"{label}: unknown criterion {criterion_id}")
        for tool_id in (
            branch.get("tools", []) if isinstance(branch.get("tools"), list) else []
        ):
            if tool_id not in tool_ids:
                errors.append(f"{label}: unknown tool {tool_id}")
        branch_ceiling = str(branch.get("claim_ceiling", ""))
        if (
            "PHYSICAL_RELEASE" in branch_ceiling
            and "NEVER_PHYSICAL_RELEASE" not in branch_ceiling
        ):
            errors.append(
                f"{label}: conditional simulation/planning branch cannot grant physical release"
            )

    stages_by_id = {item["id"]: item for item in stages if item.get("id") in stage_ids}
    s9 = stages_by_id.get("S9", {})
    if s9.get("claim_ceiling") != "DIRECTION_RANKING_ONLY":
        errors.append("S9: claim_ceiling must remain DIRECTION_RANKING_ONLY")
    if s9.get("not_triggered_state") != "NOT_APPLICABLE_WITH_RATIONALE":
        errors.append("S9: non-triggered state must require a NOT_APPLICABLE rationale")
    s11 = stages_by_id.get("S11", {})
    if "PHYSICAL_RELEASE" not in str(s11.get("claim_ceiling", "")):
        errors.append("S11: claim_ceiling must explicitly bound physical release")
    required_real_criteria = {f"K-REAL-{index:02d}" for index in range(1, 6)}
    if not required_real_criteria.issubset(set(s11.get("criteria", []))):
        errors.append("S11: K-REAL-01 through K-REAL-05 are required")
    for stage in stages:
        if stage.get("id") != "S11" and "PHYSICAL_RELEASE" in str(
            stage.get("claim_ceiling", "")
        ):
            errors.append(f"{stage.get('id')}: only S11 may grant physical release")
    cross_cutting = docs["pipeline"].get("cross_cutting", {})
    if isinstance(cross_cutting, dict):
        for value in cross_cutting.values():
            if not isinstance(value, list):
                continue
            for criterion_id in value:
                referenced_criteria.add(criterion_id)
                if criterion_id not in criterion_ids:
                    errors.append(
                        f"pipeline.cross_cutting: unknown criterion {criterion_id}"
                    )
    else:
        errors.append("pipeline: cross_cutting must be an object")
    unreferenced = sorted(criterion_ids - referenced_criteria)
    if unreferenced:
        warnings.append(
            f"criteria not referenced by a stage/cross-cutting list: {unreferenced}"
        )

    allowed_severity = {"BLOCKER", "HIGH", "MEDIUM", "LOW"}
    for item in open_items:
        label = str(item.get("id", "open_item<?>"))
        require_fields(
            item,
            ("title", "kind", "detail", "criteria", "next", "severity"),
            label,
            errors,
        )
        if item.get("severity") not in allowed_severity:
            errors.append(f"{label}: invalid severity {item.get('severity')!r}")
        for criterion_id in (
            item.get("criteria", []) if isinstance(item.get("criteria"), list) else []
        ):
            if criterion_id not in criterion_ids:
                errors.append(f"{label}: unknown criterion {criterion_id}")
    required_open_items = {"O-18", "O-19", "O-20"}
    if not required_open_items.issubset(open_item_ids):
        errors.append(
            f"open_items: missing algorithm/physics issues {sorted(required_open_items - open_item_ids)}"
        )

    manifest_files = manifest.get("files")
    if not isinstance(manifest_files, dict):
        errors.append("manifest: files must be an object")
        manifest_files = {}
    expected_filenames = set(DATA_FILES.values())
    if set(manifest_files) != expected_filenames:
        errors.append(
            f"manifest file set is {sorted(manifest_files)}, expected {sorted(expected_filenames)}"
        )
    for filename, record in manifest_files.items():
        if not isinstance(record, dict) or not isinstance(
            record.get("bundled_sha256"), str
        ):
            errors.append(f"manifest: {filename} lacks bundled_sha256")
            continue
        path = ONTOLOGY_ROOT / filename
        if not path.is_file():
            errors.append(f"manifest file missing: {filename}")
            continue
        observed = sha256(path)
        if observed != record["bundled_sha256"]:
            errors.append(
                f"manifest hash mismatch: {filename} {observed} != {record['bundled_sha256']}"
            )

    actual_counts = {
        "criteria": len(criteria),
        "failures": len(failures),
        "stages": len(stages),
        "conditional_branches": len(conditional_branches),
        "tools": len(tools),
        "open_items": len(open_items),
        "paper_algorithm_routes": len(paper_algorithms),
        "route_matrix": len(route_matrix),
    }
    if manifest.get("counts") != actual_counts:
        errors.append(
            f"manifest counts {manifest.get('counts')} != observed {actual_counts}"
        )
    status_counts = dict(
        sorted(
            Counter(
                str(item.get("validation", {}).get("status", "<MISSING>"))
                if isinstance(item.get("validation"), dict)
                else "<MISSING>"
                for item in criteria
            ).items()
        )
    )
    if manifest.get("criteria_by_validation_status") != status_counts:
        errors.append(
            "manifest criteria_by_validation_status "
            f"{manifest.get('criteria_by_validation_status')} != observed {status_counts}"
        )

    provenance_links = sum(
        len(item.get("provenance", [])) + len(item.get("implemented_by", []))
        for item in criteria
    )
    return emit(
        "PASS" if not errors else "FAIL",
        errors,
        warnings,
        snapshot_id=manifest.get("snapshot_id"),
        counts=actual_counts,
        criteria_by_status=status_counts,
        policy_id=policy.get("policy_id"),
        policy_contract={
            "planning_admission_invariants": len(planning_invariants),
            "s9_fair_comparison_facets": len(s9_facets),
            "contact_physics_invariants": len(contact_invariants),
        },
        historical_reference_links=provenance_links,
        link_contract=(
            "criterion.blocks must be reverse-listed by failure.blocked_by; failure.blocked_by may list additional "
            "preventive criteria without forcing every criterion to enumerate every failure"
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
