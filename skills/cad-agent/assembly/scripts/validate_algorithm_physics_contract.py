#!/usr/bin/env python3
"""Validate an assembly algorithm-to-physics contract without running a solver."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any


SCHEMA = "cad-agent.assembly-algorithm-physics-contract/v1"
POLICY_SCHEMA = "cad-agent.assembly-algorithm-physics-routing-policy/v1"
SKILL_ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = (
    SKILL_ROOT / "references" / "ontology" / "algorithm-physics-routing-policy.v1.json"
)
HEX64 = re.compile(r"^[0-9a-f]{64}$")
PAB_IDS = [f"PAB-{index:02d}" for index in range(1, 13)]
S9_IDS = [f"S9-FC-{index:02d}" for index in range(1, 11)]
CP_IDS = [f"CP-{index:02d}" for index in range(1, 13)]
PAPER_ALGORITHM_IDS = [
    "AAMERI_STATIC_SEMANTICS",
    "AGRAWALA_ACTION_GRAPH",
    "LI_DAG_SHORT_ESCAPE",
    "ATA_REVERSE_DISASSEMBLY",
    "ASAP_STABILITY",
    "SBDP_DBG_INSPIRED",
    "IPC_LIBUIPC",
    "FACTORY_THREADED_CONTACT",
]
ROUTE_MATRIX_IDS = [
    "ROUTE-S7-GEOMETRY",
    "ROUTE-S9-DIRECTION",
    "ROUTE-P-CONTACT",
    "ROUTE-THREADED-CONTACT",
    "ROUTE-S11-PHYSICAL",
]
BRANCH_IDS = {
    "S7_GEOMETRY",
    "S9_DIRECTION_PROBE",
    "P_CONTACT_PHYSICS",
    "THREADED_CONTACT",
    "S11_PHYSICAL",
}
CAPABILITY_STATUSES = {
    "PINNED_EXECUTABLE",
    "LOCAL_ADAPTER",
    "CASE_PROVEN_NOT_GENERALIZED",
    "LITERATURE_ONLY",
    "NOT_EXECUTED",
}
APPLICABILITY_STATUSES = {"APPLICABLE", "NOT_APPLICABLE", "HOLD"}
BASIS_AUTHORITIES = {
    "FUSION",
    "PLANNER",
    "EXACT_GEOMETRY_CHECKER",
    "PHYSICS_SOLVER",
    "THREADED_SOLVER",
    "BLENDER",
    "PHYSICAL_FIXTURE",
}
POSITIVE_DIRECTION_RESULTS = {
    "PROMISING_DIRECTION",
    "GUIDED_CONTACT_DIRECTION",
    "RESISTED_DIRECTION",
}
ALL_DIRECTION_RESULTS = POSITIVE_DIRECTION_RESULTS | {
    "AMBIGUOUS_DIRECTION",
    "INCONCLUSIVE",
    "NOT_CLAIMED",
    "NOT_REQUIRED",
    "HOLD",
}
CONTACT_LADDER = [
    "GEOMETRY_VERIFIED",
    "DRIVER_VALIDATED",
    "ASSEMBLY_VALIDATED",
    "SOLVER_QUALIFIED",
    "CONTACT_TRANSFER_VALIDATED",
    "PASSAGE_VALIDATED",
]
ROUTE_KEYS = {
    "s7_geometry",
    "s9_direction_probe",
    "p_contact_physics",
    "threaded_contact",
    "s11_physical",
}
STAGED_RUN_KEYS = {
    "geometry_placement",
    "static_initialization",
    "two_step_smoke",
    "ten_step_smoke",
    "loaded_contact_canary",
    "guide_passage_canary",
    "matched_control",
    "full_continuous_solve",
}
CAPACITY_KEYS = {
    "inertia",
    "torque_speed_or_current_curve",
    "losses",
    "controller_and_saturation",
    "stall_slip_backdrive",
}
PHYSICAL_CHECK_KEYS = {
    "fixture_calibration",
    "reclamp_repeatability",
    "machine_observable_completion",
    "load_transfer_and_clamp_release",
    "fixture_and_tool_withdrawal",
    "minimal_high_risk_trial",
}
NONPOSITIVE_RESULTS = {"NOT_CLAIMED", "NOT_REQUIRED", "INCONCLUSIVE", "HOLD"}


@dataclass
class ValidationState:
    errors: list[str] = field(default_factory=list)
    holds: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    evidence_registry: set[str] = field(default_factory=set)
    referenced_evidence: set[str] = field(default_factory=set)

    def evidence_links(
        self, value: Any, label: str, *, required: bool = False
    ) -> list[str]:
        if not isinstance(value, list) or any(
            not isinstance(item, str) or not item for item in value
        ):
            self.errors.append(f"{label}: expected an array of non-empty evidence IDs")
            return []
        if len(set(value)) != len(value):
            self.errors.append(f"{label}: duplicate evidence IDs are not allowed")
        missing = sorted(set(value) - self.evidence_registry)
        if missing:
            self.errors.append(f"{label}: unregistered evidence IDs {missing}")
        self.referenced_evidence.update(value)
        if required and not value:
            self.holds.append(f"{label}: evidence is required")
        return value


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def placeholder(value: Any) -> bool:
    if value is None:
        return True
    if not isinstance(value, str):
        return False
    normalized = value.strip().upper()
    return (
        not normalized
        or "REPLACE" in normalized
        or normalized in {"UNKNOWN", "UNSELECTED"}
    )


def valid_timestamp(value: Any) -> bool:
    if not isinstance(value, str) or placeholder(value):
        return False
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return False
    return parsed.tzinfo is not None


def require_known(value: Any, label: str, state: ValidationState) -> None:
    if placeholder(value):
        state.holds.append(f"{label}: missing or placeholder value")


def require_sha(value: Any, label: str, state: ValidationState) -> None:
    if placeholder(value):
        state.holds.append(f"{label}: missing or placeholder SHA-256")
    elif not isinstance(value, str) or not HEX64.fullmatch(value):
        state.errors.append(f"{label}: expected a lower-case 64-digit SHA-256")


def object_value(
    parent: dict[str, Any], key: str, state: ValidationState, label: str = ""
) -> dict[str, Any]:
    value = parent.get(key)
    if not isinstance(value, dict):
        state.errors.append(f"{label or key}: expected an object")
        return {}
    return value


def list_value(
    parent: dict[str, Any], key: str, state: ValidationState, label: str = ""
) -> list[Any]:
    value = parent.get(key)
    if not isinstance(value, list):
        state.errors.append(f"{label or key}: expected an array")
        return []
    return value


def exact_keys(
    value: dict[str, Any], expected: set[str], label: str, state: ValidationState
) -> None:
    missing = sorted(expected - set(value))
    extra = sorted(set(value) - expected)
    if missing:
        state.errors.append(f"{label}: missing keys {missing}")
    if extra:
        state.errors.append(f"{label}: unsupported keys {extra}")


def enum_value(
    value: Any, allowed: set[str], label: str, state: ValidationState
) -> bool:
    if not isinstance(value, str) or value not in allowed:
        state.errors.append(f"{label}: unsupported value {value!r}")
        return False
    return True


def string_list(value: Any, label: str, state: ValidationState) -> list[str]:
    if not isinstance(value, list) or any(
        not isinstance(item, str) or not item for item in value
    ):
        state.errors.append(f"{label}: expected an array of non-empty strings")
        return []
    if len(set(value)) != len(value):
        state.errors.append(f"{label}: duplicate values are not allowed")
    return value


def load_policy(path: Path, state: ValidationState) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as handle:
            policy = json.load(handle)
    except OSError as exc:
        state.errors.append(f"policy: cannot read {path}: {exc}")
        return {}
    except json.JSONDecodeError as exc:
        state.errors.append(
            f"policy: invalid JSON at line {exc.lineno}, column {exc.colno}: {exc.msg}"
        )
        return {}
    if not isinstance(policy, dict):
        state.errors.append("policy: root must be an object")
        return {}
    if policy.get("$schema") != POLICY_SCHEMA:
        state.errors.append(
            f"policy.$schema: {policy.get('$schema')!r}, expected {POLICY_SCHEMA!r}"
        )
    if placeholder(policy.get("policy_id")) or placeholder(policy.get("version")):
        state.errors.append("policy: policy_id and version must be stable values")
    expected_sections = {
        "authority_model",
        "claim_ledger",
        "status_vocabularies",
        "planning_admission_invariants",
        "s9_fair_comparison_facets",
        "contact_physics_invariants",
        "paper_algorithm_routes",
        "route_matrix",
        "routing_rules",
        "overclaim_guards",
    }
    for section in sorted(expected_sections):
        if section not in policy:
            state.errors.append(f"policy: missing section {section}")
    validate_policy_ids(policy, "planning_admission_invariants", PAB_IDS, state)
    validate_policy_ids(policy, "s9_fair_comparison_facets", S9_IDS, state)
    validate_policy_ids(policy, "contact_physics_invariants", CP_IDS, state)
    validate_paper_routes(policy, state)
    validate_route_matrix(policy, state)
    vocabularies = policy.get("status_vocabularies")
    if not isinstance(vocabularies, dict):
        state.errors.append("policy.status_vocabularies: expected an object")
    else:
        portability = vocabularies.get("parameter_portability")
        expected_portability = {
            "STRUCTURAL_INVARIANT",
            "TARGET_SPECIFIC",
            "CASE_LOCKED_EXAMPLE",
        }
        if (
            not isinstance(portability, list)
            or set(portability) != expected_portability
        ):
            state.errors.append(
                "policy.status_vocabularies.parameter_portability: expected the exact three-value vocabulary"
            )
    guards = policy.get("overclaim_guards")
    if not isinstance(guards, dict):
        state.errors.append("policy.overclaim_guards: expected an object")
    else:
        required_false = {
            "zero_contact_alone_can_be_promising",
            "motion_servo_strength_is_motor_capacity",
            "geometric_reclosure_is_mechanical_relock",
            "simulation_is_physical_release",
            "case_locked_parameter_may_be_active",
        }
        for key in sorted(required_false):
            if guards.get(key) is not False:
                state.errors.append(f"policy.overclaim_guards.{key}: must be false")
        if guards.get("required_route_without_execution") != "INCONCLUSIVE":
            state.errors.append(
                "policy.overclaim_guards.required_route_without_execution: must be INCONCLUSIVE"
            )
        if guards.get("blender_claim_ceiling") != "DELIVERY_VERIFIED":
            state.errors.append(
                "policy.overclaim_guards.blender_claim_ceiling: must be DELIVERY_VERIFIED"
            )
    return policy


def validate_policy_ids(
    policy: dict[str, Any], key: str, expected: list[str], state: ValidationState
) -> None:
    records = policy.get(key)
    if not isinstance(records, list) or any(
        not isinstance(item, dict) for item in records
    ):
        state.errors.append(f"policy.{key}: expected an array of objects")
        return
    ids = [item.get("id") for item in records]
    if ids != expected:
        state.errors.append(f"policy.{key}: ids {ids}, expected {expected}")
    for index, record in enumerate(records):
        if placeholder(record.get("name")):
            state.errors.append(f"policy.{key}[{index}].name: required")


def validate_paper_routes(policy: dict[str, Any], state: ValidationState) -> None:
    records = policy.get("paper_algorithm_routes")
    if not isinstance(records, list) or any(
        not isinstance(item, dict) for item in records
    ):
        state.errors.append(
            "policy.paper_algorithm_routes: expected an array of objects"
        )
        return
    ids = [item.get("id") for item in records]
    if ids != PAPER_ALGORITHM_IDS:
        state.errors.append(
            f"policy.paper_algorithm_routes: ids {ids}, expected {PAPER_ALGORITHM_IDS}"
        )
    required_fields = {
        "id",
        "name",
        "applicability",
        "required_inputs",
        "produces",
        "does_not_prove",
        "capability_status",
        "claim_ceiling",
        "branch_refs",
    }
    for index, record in enumerate(records):
        label = f"policy.paper_algorithm_routes[{index}]"
        exact_keys(record, required_fields, label, state)
        require_known(record.get("name"), f"{label}.name", state)
        for key in ("applicability", "required_inputs", "produces", "does_not_prove"):
            values = string_list(record.get(key), f"{label}.{key}", state)
            if not values:
                state.errors.append(f"{label}.{key}: must be non-empty")
        enum_value(
            record.get("capability_status"),
            CAPABILITY_STATUSES,
            f"{label}.capability_status",
            state,
        )
        require_known(record.get("claim_ceiling"), f"{label}.claim_ceiling", state)
        branches = string_list(record.get("branch_refs"), f"{label}.branch_refs", state)
        unknown = sorted(set(branches) - BRANCH_IDS)
        if unknown:
            state.errors.append(f"{label}.branch_refs: unknown branches {unknown}")


def validate_route_matrix(policy: dict[str, Any], state: ValidationState) -> None:
    records = policy.get("route_matrix")
    if not isinstance(records, list) or any(
        not isinstance(item, dict) for item in records
    ):
        state.errors.append("policy.route_matrix: expected an array of objects")
        return
    ids = [item.get("id") for item in records]
    if ids != ROUTE_MATRIX_IDS:
        state.errors.append(
            f"policy.route_matrix: ids {ids}, expected {ROUTE_MATRIX_IDS}"
        )
    required_fields = {
        "id",
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
    }
    algorithm_ids = set(PAPER_ALGORITHM_IDS)
    observed_branches: set[str] = set()
    for index, record in enumerate(records):
        label = f"policy.route_matrix[{index}]"
        exact_keys(record, required_fields, label, state)
        branch = record.get("branch_id")
        if branch not in BRANCH_IDS:
            state.errors.append(f"{label}.branch_id: unsupported value {branch!r}")
        elif branch in observed_branches:
            state.errors.append(f"{label}.branch_id: duplicate {branch!r}")
        else:
            observed_branches.add(branch)
        for key in (
            "operation_classes",
            "required_upstream",
            "required_claims",
            "tool_refs",
            "algorithm_refs",
        ):
            values = string_list(record.get(key), f"{label}.{key}", state)
            if not values:
                state.errors.append(f"{label}.{key}: must be non-empty")
        unknown_algorithms = sorted(
            set(record.get("algorithm_refs", [])) - algorithm_ids
            if isinstance(record.get("algorithm_refs"), list)
            else set()
        )
        if unknown_algorithms:
            state.errors.append(f"{label}.algorithm_refs: unknown {unknown_algorithms}")
        require_known(record.get("trigger"), f"{label}.trigger", state)
        require_known(record.get("claim_ceiling"), f"{label}.claim_ceiling", state)
        if record.get("on_unknown") != "HOLD":
            state.errors.append(f"{label}.on_unknown: must be HOLD")
        if not isinstance(record.get("return_stage"), str) or not re.fullmatch(
            r"S(?:[0-9]|1[01])", record.get("return_stage", "")
        ):
            state.errors.append(f"{label}.return_stage: expected S0-S11")
    if observed_branches != BRANCH_IDS:
        state.errors.append(
            f"policy.route_matrix: branches {sorted(observed_branches)}, expected {sorted(BRANCH_IDS)}"
        )


def validate_evidence_registry(
    document: dict[str, Any], state: ValidationState
) -> None:
    records = list_value(document, "evidence_registry", state)
    for index, raw in enumerate(records):
        label = f"evidence_registry[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        evidence_id = raw.get("evidence_id")
        if not isinstance(evidence_id, str) or not evidence_id:
            state.errors.append(f"{label}.evidence_id: required")
            continue
        if evidence_id in state.evidence_registry:
            state.errors.append(f"{label}.evidence_id: duplicate {evidence_id!r}")
        state.evidence_registry.add(evidence_id)
        require_known(raw.get("kind"), f"{label}.kind", state)
        require_known(raw.get("path"), f"{label}.path", state)
        require_sha(raw.get("sha256"), f"{label}.sha256", state)
        if not valid_timestamp(raw.get("captured_at")):
            state.holds.append(
                f"{label}.captured_at: timezone-aware timestamp required"
            )


def validate_check_records(
    value: Any,
    expected_ids: list[str],
    allowed_statuses: set[str],
    label: str,
    state: ValidationState,
) -> dict[str, str]:
    if not isinstance(value, list):
        state.errors.append(f"{label}: expected an array")
        return {}
    if any(not isinstance(item, dict) for item in value):
        state.errors.append(f"{label}: every check must be an object")
    records = [item for item in value if isinstance(item, dict)]
    ids = [item.get("id") for item in records]
    if ids != expected_ids:
        state.errors.append(f"{label}: ids {ids}, expected {expected_ids}")
    statuses: dict[str, str] = {}
    for index, record in enumerate(records):
        item_label = f"{label}[{index}]"
        check_id = record.get("id")
        status = record.get("status")
        if isinstance(check_id, str) and enum_value(
            status, allowed_statuses, f"{item_label}.status", state
        ):
            statuses[check_id] = status
        if status != "PASS" and placeholder(record.get("rationale")):
            state.holds.append(f"{item_label}.rationale: required for non-PASS check")
        state.evidence_links(
            record.get("evidence_ids"),
            f"{item_label}.evidence_ids",
            required=status == "PASS",
        )
    return statuses


def validate_authority(document: dict[str, Any], state: ValidationState) -> None:
    authority = object_value(document, "authority", state)
    exact_keys(
        authority,
        {"cad_pose", "physics", "presentation", "physical"},
        "authority",
        state,
    )
    cad = object_value(authority, "cad_pose", state, "authority.cad_pose")
    if cad.get("system") != "FUSION" or cad.get("authoritative") is not True:
        state.errors.append(
            "authority.cad_pose: Fusion must be the authoritative CAD pose source"
        )
    require_known(
        cad.get("source_document_id"), "authority.cad_pose.source_document_id", state
    )
    require_sha(
        cad.get("source_document_sha256"),
        "authority.cad_pose.source_document_sha256",
        state,
    )
    state.evidence_links(
        cad.get("evidence_ids"), "authority.cad_pose.evidence_ids", required=True
    )

    physics = object_value(authority, "physics", state, "authority.physics")
    if not isinstance(physics.get("authoritative"), bool):
        state.errors.append("authority.physics.authoritative: expected a boolean")
    if physics.get("claim_ceiling") != "MODELED_PHYSICS_ONLY":
        state.errors.append(
            "authority.physics.claim_ceiling: must be MODELED_PHYSICS_ONLY"
        )
    state.evidence_links(physics.get("evidence_ids"), "authority.physics.evidence_ids")

    presentation = object_value(
        authority, "presentation", state, "authority.presentation"
    )
    if (
        presentation.get("system") != "BLENDER"
        or presentation.get("read_only") is not True
        or presentation.get("claim_ceiling") != "DELIVERY_VERIFIED"
    ):
        state.errors.append(
            "authority.presentation: Blender must be read-only with DELIVERY_VERIFIED ceiling"
        )
    state.evidence_links(
        presentation.get("evidence_ids"), "authority.presentation.evidence_ids"
    )

    physical = object_value(authority, "physical", state, "authority.physical")
    if not isinstance(physical.get("authoritative"), bool):
        state.errors.append("authority.physical.authoritative: expected a boolean")
    if physical.get("claim_ceiling") != "PHYSICAL_RELEASE_VALIDATED":
        state.errors.append(
            "authority.physical.claim_ceiling: must be PHYSICAL_RELEASE_VALIDATED"
        )
    state.evidence_links(
        physical.get("evidence_ids"), "authority.physical.evidence_ids"
    )


def validate_coordinate_and_operation(
    document: dict[str, Any], targets: set[str], state: ValidationState
) -> tuple[dict[str, Any], dict[str, Any]]:
    coordinate = object_value(document, "coordinate_contract", state)
    if coordinate.get("length_unit") != "m" or coordinate.get("angle_unit") != "rad":
        state.errors.append(
            "coordinate_contract: solver contract must use SI metres and radians"
        )
    if coordinate.get("transform_application_count") != 1:
        state.errors.append(
            "coordinate_contract.transform_application_count: must be exactly 1"
        )
    require_known(
        coordinate.get("transform_convention"),
        "coordinate_contract.transform_convention",
        state,
    )
    require_known(
        coordinate.get("q0_state_id"), "coordinate_contract.q0_state_id", state
    )
    for key in ("q0_state_sha256", "geometry_set_sha256", "world_pose_set_sha256"):
        require_sha(coordinate.get(key), f"coordinate_contract.{key}", state)
    state.evidence_links(
        coordinate.get("evidence_ids"),
        "coordinate_contract.evidence_ids",
        required=bool(targets),
    )

    operation = object_value(document, "operation_profile", state)
    require_known(
        operation.get("operation_class"), "operation_profile.operation_class", state
    )
    mover = string_list(
        operation.get("mover_occurrence_ids"),
        "operation_profile.mover_occurrence_ids",
        state,
    )
    fixed = string_list(
        operation.get("fixed_occurrence_ids"),
        "operation_profile.fixed_occurrence_ids",
        state,
    )
    deformable = string_list(
        operation.get("deformable_occurrence_ids"),
        "operation_profile.deformable_occurrence_ids",
        state,
    )
    if targets and (not mover or not fixed):
        state.holds.append(
            "operation_profile: target claims require non-empty mover and fixed partitions"
        )
    overlap = sorted(set(mover) & set(fixed))
    if overlap:
        state.errors.append(f"operation_profile: mover/fixed overlap {overlap}")
    if not isinstance(operation.get("rigid_subassembly_declared"), bool):
        state.errors.append(
            "operation_profile.rigid_subassembly_declared: expected a boolean"
        )
    if set(deformable) & (set(mover) | set(fixed)):
        state.errors.append(
            "operation_profile: deformables must be separately partitioned"
        )
    for key in (
        "thread_engagement",
        "force_or_capacity_claim",
        "physical_release_claim",
    ):
        enum_value(
            operation.get(key),
            {"YES", "NO", "UNKNOWN"},
            f"operation_profile.{key}",
            state,
        )
    state.evidence_links(
        operation.get("evidence_ids"), "operation_profile.evidence_ids"
    )
    return coordinate, operation


def validate_parameters(
    document: dict[str, Any], policy: dict[str, Any], state: ValidationState
) -> list[dict[str, Any]]:
    vocabularies = policy.get("status_vocabularies", {})
    provenance_values = set(vocabularies.get("parameter_provenance", []))
    portability_values = set(vocabularies.get("parameter_portability", []))
    records = list_value(document, "parameter_records", state)
    seen: set[str] = set()
    active: list[dict[str, Any]] = []
    for index, raw in enumerate(records):
        label = f"parameter_records[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        parameter_id = raw.get("parameter_id")
        if not isinstance(parameter_id, str) or not parameter_id:
            state.errors.append(f"{label}.parameter_id: required")
        elif parameter_id in seen:
            state.errors.append(f"{label}.parameter_id: duplicate {parameter_id!r}")
        else:
            seen.add(parameter_id)
        require_known(raw.get("name"), f"{label}.name", state)
        enum_value(
            raw.get("provenance"), provenance_values, f"{label}.provenance", state
        )
        portability = raw.get("portability")
        enum_value(portability, portability_values, f"{label}.portability", state)
        is_active = raw.get("active_profile")
        if not isinstance(is_active, bool):
            state.errors.append(f"{label}.active_profile: expected a boolean")
            is_active = False
        evidence = state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=is_active
        )
        if portability == "CASE_LOCKED_EXAMPLE" and is_active:
            state.errors.append(
                f"{label}: CASE_LOCKED_EXAMPLE parameter cannot enter the active profile"
            )
        if is_active:
            active.append(raw)
            if placeholder(raw.get("value")) or placeholder(raw.get("unit")):
                state.holds.append(f"{label}: active parameter requires value and unit")
            if raw.get("provenance") == "UNKNOWN":
                state.holds.append(f"{label}: active parameter provenance is UNKNOWN")
            if not evidence:
                state.holds.append(f"{label}: active parameter lacks evidence")
    return active


def validate_planning(
    document: dict[str, Any],
    coordinate: dict[str, Any],
    operation: dict[str, Any],
    targets: set[str],
    check_statuses: set[str],
    state: ValidationState,
) -> dict[str, str]:
    planning = object_value(document, "planning", state)
    method = object_value(planning, "method", state, "planning.method")
    require_known(method.get("method_id"), "planning.method.method_id", state)
    require_known(method.get("method_class"), "planning.method.method_class", state)
    enum_value(
        method.get("capability_status"),
        CAPABILITY_STATUSES,
        "planning.method.capability_status",
        state,
    )
    enum_value(
        method.get("applicability_status"),
        APPLICABILITY_STATUSES,
        "planning.method.applicability_status",
        state,
    )
    require_known(
        method.get("implementation_identity"),
        "planning.method.implementation_identity",
        state,
    )
    state.evidence_links(method.get("evidence_ids"), "planning.method.evidence_ids")
    pab = validate_check_records(
        planning.get("pab_checks"),
        PAB_IDS,
        check_statuses,
        "planning.pab_checks",
        state,
    )
    ledger = object_value(planning, "state_ledger", state, "planning.state_ledger")
    state_ids = string_list(
        ledger.get("ordered_absolute_state_ids"),
        "planning.state_ledger.ordered_absolute_state_ids",
        state,
    )
    state_count = ledger.get("state_count")
    if (
        isinstance(state_count, bool)
        or not isinstance(state_count, int)
        or state_count < 0
    ):
        state.errors.append(
            "planning.state_ledger.state_count: expected non-negative integer"
        )
    elif state_count != len(state_ids):
        state.errors.append(
            "planning.state_ledger.state_count: must equal ordered_absolute_state_ids length"
        )
    require_sha(
        ledger.get("canonical_state_digest"),
        "planning.state_ledger.canonical_state_digest",
        state,
    )
    ledger_mover = string_list(
        ledger.get("mover_occurrence_ids"),
        "planning.state_ledger.mover_occurrence_ids",
        state,
    )
    ledger_fixed = string_list(
        ledger.get("fixed_occurrence_ids"),
        "planning.state_ledger.fixed_occurrence_ids",
        state,
    )
    if set(ledger_mover) != set(operation.get("mover_occurrence_ids", [])):
        state.errors.append(
            "planning.state_ledger: mover partition differs from operation profile"
        )
    if set(ledger_fixed) != set(operation.get("fixed_occurrence_ids", [])):
        state.errors.append(
            "planning.state_ledger: fixed partition differs from operation profile"
        )
    enum_value(
        ledger.get("saved_scene_readback_status"),
        check_statuses,
        "planning.state_ledger.saved_scene_readback_status",
        state,
    )
    state.evidence_links(
        ledger.get("evidence_ids"), "planning.state_ledger.evidence_ids"
    )

    reversal = object_value(planning, "reversal", state, "planning.reversal")
    if not isinstance(reversal.get("used"), bool):
        state.errors.append("planning.reversal.used: expected a boolean")
    if (
        reversal.get("used") is True
        and reversal.get("mode") != "REVERSE_ORDERED_ABSOLUTE_STATES"
    ):
        state.errors.append(
            "planning.reversal.mode: reversal must reverse ordered absolute states; transform negation is forbidden"
        )
    if isinstance(reversal.get("mode"), str) and "NEGAT" in reversal["mode"].upper():
        state.errors.append(
            "planning.reversal.mode: absolute-state negation is forbidden"
        )
    enum_value(
        reversal.get("connector_path_status"),
        check_statuses,
        "planning.reversal.connector_path_status",
        state,
    )
    state.evidence_links(reversal.get("evidence_ids"), "planning.reversal.evidence_ids")

    if targets & {"ORDERED_PLANNING", "GEOMETRIC_PATH"}:
        if any(pab.get(check_id) != "PASS" for check_id in PAB_IDS):
            state.holds.append(
                "planning: all PAB-01..PAB-12 checks must PASS for admitted trajectory claims"
            )
        if len(state_ids) < 2:
            state.holds.append(
                "planning.state_ledger: admitted trajectory requires at least two absolute states"
            )
        if ledger.get("initial_state_id") != (state_ids[0] if state_ids else None):
            state.errors.append(
                "planning.state_ledger: initial_state_id must equal the first ordered state"
            )
        if ledger.get("initial_state_id") != coordinate.get("q0_state_id"):
            state.errors.append(
                "planning.state_ledger: initial_state_id must equal authoritative coordinate_contract.q0_state_id"
            )
        if ledger.get("terminal_state_id") != (state_ids[-1] if state_ids else None):
            state.errors.append(
                "planning.state_ledger: terminal_state_id must equal the last ordered state"
            )
        require_known(
            ledger.get("terminal_goal_class"),
            "planning.state_ledger.terminal_goal_class",
            state,
        )
        require_known(
            ledger.get("edge_check_class"),
            "planning.state_ledger.edge_check_class",
            state,
        )
        if ledger.get("saved_scene_readback_status") != "PASS":
            state.holds.append("planning.state_ledger: saved-scene readback must PASS")
        if method.get("applicability_status") != "APPLICABLE":
            state.holds.append(
                "planning.method: target planning/path claim requires APPLICABLE method"
            )
        if method.get("capability_status") in {"LITERATURE_ONLY", "NOT_EXECUTED"}:
            state.holds.append(
                "planning.method: literature-only or unexecuted method cannot admit a target path"
            )
    return pab


def validate_route_plan(
    document: dict[str, Any], policy: dict[str, Any], state: ValidationState
) -> dict[str, str]:
    routes = object_value(document, "route_plan", state)
    exact_keys(routes, ROUTE_KEYS, "route_plan", state)
    route_values = set(policy.get("status_vocabularies", {}).get("route_status", []))
    statuses: dict[str, str] = {}
    for key in sorted(ROUTE_KEYS):
        route = routes.get(key)
        if not isinstance(route, dict):
            state.errors.append(f"route_plan.{key}: expected an object")
            continue
        status = route.get("status")
        if enum_value(status, route_values, f"route_plan.{key}.status", state):
            statuses[key] = status
        if status != "REQUIRED" and placeholder(route.get("rationale")):
            state.holds.append(
                f"route_plan.{key}.rationale: non-required route needs rationale"
            )
    return statuses


def validate_directional_probe(
    document: dict[str, Any],
    routes: dict[str, str],
    policy: dict[str, Any],
    check_statuses: set[str],
    state: ValidationState,
) -> tuple[dict[str, Any], dict[str, str]]:
    probe = object_value(document, "directional_probe", state)
    execution_values = set(
        policy.get("status_vocabularies", {}).get("execution_status", [])
    )
    execution = probe.get("execution_status")
    enum_value(execution, execution_values, "directional_probe.execution_status", state)
    checks = validate_check_records(
        probe.get("fairness_checks"),
        S9_IDS,
        check_statuses,
        "directional_probe.fairness_checks",
        state,
    )
    candidates = list_value(probe, "candidates", state, "directional_probe.candidates")
    candidate_ids: set[str] = set()
    reached_feature = False
    any_contact = False
    start_digests: set[str] = set()
    environment_digests: set[str] = set()
    profile_digests: set[str] = set()
    for index, raw in enumerate(candidates):
        label = f"directional_probe.candidates[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        candidate_id = raw.get("candidate_id")
        if not isinstance(candidate_id, str) or not candidate_id:
            state.errors.append(f"{label}.candidate_id: required")
        elif candidate_id in candidate_ids:
            state.errors.append(f"{label}.candidate_id: duplicate {candidate_id!r}")
        else:
            candidate_ids.add(candidate_id)
        direction = raw.get("direction_unit_vector")
        if (
            not isinstance(direction, list)
            or len(direction) != 3
            or any(
                isinstance(item, bool)
                or not isinstance(item, (int, float))
                or not math.isfinite(item)
                for item in direction
            )
            or abs(sum(float(item) ** 2 for item in direction) - 1.0) > 1e-6
        ):
            state.errors.append(
                f"{label}.direction_unit_vector: expected a finite unit vector"
            )
        for key in (
            "start_state_sha256",
            "environment_sha256",
            "comparison_profile_sha256",
        ):
            require_sha(raw.get(key), f"{label}.{key}", state)
        if isinstance(raw.get("start_state_sha256"), str):
            start_digests.add(raw["start_state_sha256"])
        if isinstance(raw.get("environment_sha256"), str):
            environment_digests.add(raw["environment_sha256"])
        if isinstance(raw.get("comparison_profile_sha256"), str):
            profile_digests.add(raw["comparison_profile_sha256"])
        if not isinstance(raw.get("first_discriminating_feature_reached"), bool):
            state.errors.append(
                f"{label}.first_discriminating_feature_reached: expected boolean"
            )
        reached_feature = (
            reached_feature or raw.get("first_discriminating_feature_reached") is True
        )
        if not isinstance(raw.get("contact_detected"), bool):
            state.errors.append(f"{label}.contact_detected: expected boolean")
        any_contact = any_contact or raw.get("contact_detected") is True
        if raw.get("classification") not in ALL_DIRECTION_RESULTS:
            state.errors.append(
                f"{label}.classification: unsupported value {raw.get('classification')!r}"
            )
        state.evidence_links(
            raw.get("evidence_ids"),
            f"{label}.evidence_ids",
            required=execution == "EXECUTED",
        )
    if not isinstance(probe.get("zero_contact_only_basis"), bool):
        state.errors.append(
            "directional_probe.zero_contact_only_basis: expected a boolean"
        )
    result = probe.get("result")
    if result not in ALL_DIRECTION_RESULTS:
        state.errors.append(f"directional_probe.result: unsupported value {result!r}")
    state.evidence_links(probe.get("evidence_ids"), "directional_probe.evidence_ids")

    if routes.get("s9_direction_probe") == "REQUIRED":
        if execution != "EXECUTED":
            if result != "INCONCLUSIVE":
                state.errors.append(
                    "directional_probe: required route without execution must return INCONCLUSIVE"
                )
            state.holds.append(
                "directional_probe: required S9 route has no numerical execution"
            )
        else:
            if len(candidates) < 2:
                state.errors.append(
                    "directional_probe: fair S9 comparison requires at least two candidates"
                )
            if (
                len(start_digests) != 1
                or len(environment_digests) != 1
                or len(profile_digests) != 1
            ):
                state.errors.append(
                    "directional_probe: every candidate must share one start state, environment, and comparison profile"
                )
            if result in POSITIVE_DIRECTION_RESULTS and any(
                checks.get(check_id) != "PASS" for check_id in S9_IDS
            ):
                state.errors.append(
                    "directional_probe: positive direction result requires all S9-FC-01..10 checks to PASS"
                )
            if result in POSITIVE_DIRECTION_RESULTS and not reached_feature:
                state.errors.append(
                    "directional_probe: no discriminating feature reached; result must be ambiguous"
                )
            if result in POSITIVE_DIRECTION_RESULTS:
                require_known(
                    probe.get("first_discriminating_feature_id"),
                    "directional_probe.first_discriminating_feature_id",
                    state,
                )
            if not reached_feature and result not in {
                "AMBIGUOUS_DIRECTION",
                "INCONCLUSIVE",
            }:
                state.errors.append(
                    "directional_probe: no-feature campaign must return AMBIGUOUS_DIRECTION or INCONCLUSIVE"
                )
    if result == "PROMISING_DIRECTION" and (
        probe.get("zero_contact_only_basis") is True or not any_contact
    ):
        state.errors.append(
            "directional_probe: zero-contact evidence alone cannot produce PROMISING_DIRECTION"
        )
    return probe, checks


def validate_contact_physics(
    document: dict[str, Any],
    routes: dict[str, str],
    targets: set[str],
    active_parameters: list[dict[str, Any]],
    policy: dict[str, Any],
    check_statuses: set[str],
    state: ValidationState,
) -> tuple[dict[str, Any], dict[str, str]]:
    contact = object_value(document, "contact_physics", state)
    execution_values = set(
        policy.get("status_vocabularies", {}).get("execution_status", [])
    )
    execution = contact.get("execution_status")
    enum_value(execution, execution_values, "contact_physics.execution_status", state)
    drive_values = set(policy.get("status_vocabularies", {}).get("drive_mode", []))
    enum_value(
        contact.get("drive_mode"), drive_values, "contact_physics.drive_mode", state
    )
    checks = validate_check_records(
        contact.get("checks"), CP_IDS, check_statuses, "contact_physics.checks", state
    )
    staged = object_value(contact, "staged_runs", state, "contact_physics.staged_runs")
    exact_keys(staged, STAGED_RUN_KEYS, "contact_physics.staged_runs", state)
    run_values = {"PASS", "FAIL", "NOT_RUN", "NOT_REQUIRED"}
    for key in sorted(STAGED_RUN_KEYS):
        enum_value(
            staged.get(key), run_values, f"contact_physics.staged_runs.{key}", state
        )
    enum_value(
        contact.get("action_reaction_status"),
        check_statuses,
        "contact_physics.action_reaction_status",
        state,
    )
    enum_value(
        contact.get("convergence_status"),
        check_statuses,
        "contact_physics.convergence_status",
        state,
    )
    capacity = object_value(
        contact, "capacity_model", state, "contact_physics.capacity_model"
    )
    exact_keys(
        {key: value for key, value in capacity.items() if key != "evidence_ids"},
        CAPACITY_KEYS,
        "contact_physics.capacity_model",
        state,
    )
    for key in sorted(CAPACITY_KEYS):
        enum_value(
            capacity.get(key),
            check_statuses,
            f"contact_physics.capacity_model.{key}",
            state,
        )
    state.evidence_links(
        capacity.get("evidence_ids"), "contact_physics.capacity_model.evidence_ids"
    )
    result_ladder = contact.get("result_ladder")
    if result_ladder not in set(CONTACT_LADDER) | NONPOSITIVE_RESULTS:
        state.errors.append(
            f"contact_physics.result_ladder: unsupported value {result_ladder!r}"
        )
    state.evidence_links(contact.get("evidence_ids"), "contact_physics.evidence_ids")

    dependent = targets & {"CONTACT_TRANSFER", "PASSAGE", "MOTOR_CAPACITY"}
    if result_ladder in CONTACT_LADDER and any(
        checks.get(check_id) != "PASS" for check_id in CP_IDS
    ):
        state.errors.append(
            "contact_physics: any positive ladder result requires all CP-01..12 checks to PASS"
        )
    if routes.get("p_contact_physics") == "REQUIRED":
        if execution != "EXECUTED":
            if result_ladder != "INCONCLUSIVE":
                state.errors.append(
                    "contact_physics: required route without execution must return INCONCLUSIVE"
                )
            state.holds.append(
                "contact_physics: required P-level route has no numerical execution"
            )
        elif dependent:
            if any(checks.get(check_id) != "PASS" for check_id in CP_IDS):
                state.errors.append(
                    "contact_physics: positive contact-dependent claim requires all CP-01..12 checks to PASS"
                )
            if any(staged.get(key) != "PASS" for key in STAGED_RUN_KEYS):
                state.errors.append(
                    "contact_physics: positive contact-dependent claim requires every staged run to PASS"
                )
            require_known(
                contact.get("solver_name"), "contact_physics.solver_name", state
            )
            require_known(
                contact.get("solver_version"), "contact_physics.solver_version", state
            )
            require_known(
                contact.get("force_recovery_convention"),
                "contact_physics.force_recovery_convention",
                state,
            )
            if contact.get("action_reaction_status") != "PASS":
                state.holds.append("contact_physics: action-reaction must PASS")
            if contact.get("convergence_status") != "PASS":
                state.holds.append("contact_physics: convergence must PASS")
            if not active_parameters:
                state.holds.append(
                    "contact_physics: active parameter profile is required"
                )
    if "MOTOR_CAPACITY" in targets:
        if contact.get("drive_mode") != "TORQUE_LIMITED_MOTOR_VALIDATION":
            state.errors.append(
                "contact_physics: motion-driven servo strength is not motor capacity; use TORQUE_LIMITED_MOTOR_VALIDATION"
            )
        if any(capacity.get(key) != "PASS" for key in CAPACITY_KEYS):
            state.errors.append(
                "contact_physics.capacity_model: motor-capacity claim requires inertia, curve, losses, controller/saturation, and stall/slip/backdrive PASS"
            )
        state.evidence_links(
            capacity.get("evidence_ids"),
            "contact_physics.capacity_model.evidence_ids",
            required=True,
        )
    return contact, checks


def validate_thread_endpoint_physical_outputs(
    document: dict[str, Any],
    routes: dict[str, str],
    targets: set[str],
    policy: dict[str, Any],
    state: ValidationState,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any]]:
    execution_values = set(
        policy.get("status_vocabularies", {}).get("execution_status", [])
    )
    check_values = set(policy.get("status_vocabularies", {}).get("check_status", []))
    thread = object_value(document, "threaded_contact", state)
    enum_value(
        thread.get("execution_status"),
        execution_values,
        "threaded_contact.execution_status",
        state,
    )
    if not isinstance(thread.get("dedicated_model"), bool):
        state.errors.append("threaded_contact.dedicated_model: expected a boolean")
    if thread.get("result") not in {"THREADED_CONTACT_VALIDATED"} | NONPOSITIVE_RESULTS:
        state.errors.append(
            f"threaded_contact.result: unsupported value {thread.get('result')!r}"
        )
    thread_evidence = state.evidence_links(
        thread.get("evidence_ids"), "threaded_contact.evidence_ids"
    )
    if (
        routes.get("threaded_contact") == "REQUIRED"
        and thread.get("execution_status") != "EXECUTED"
    ):
        if thread.get("result") != "INCONCLUSIVE":
            state.errors.append(
                "threaded_contact: required route without execution must return INCONCLUSIVE"
            )
        state.holds.append(
            "threaded_contact: required dedicated route has no execution"
        )
    if "THREADED_CONTACT" in targets:
        if (
            thread.get("execution_status") != "EXECUTED"
            or thread.get("dedicated_model") is not True
            or thread.get("result") != "THREADED_CONTACT_VALIDATED"
        ):
            state.holds.append(
                "threaded_contact: validated claim requires executed dedicated model"
            )
        require_known(
            thread.get("model_identity"), "threaded_contact.model_identity", state
        )
        if not thread_evidence:
            state.holds.append("threaded_contact: target claim requires evidence")

    endpoint = object_value(document, "endpoint_validation", state)
    geometric = object_value(
        endpoint,
        "geometric_reclosure",
        state,
        "endpoint_validation.geometric_reclosure",
    )
    mechanical = object_value(
        endpoint, "mechanical_relock", state, "endpoint_validation.mechanical_relock"
    )
    enum_value(
        geometric.get("status"),
        check_values,
        "endpoint_validation.geometric_reclosure.status",
        state,
    )
    enum_value(
        mechanical.get("status"),
        check_values,
        "endpoint_validation.mechanical_relock.status",
        state,
    )
    geometric_evidence = state.evidence_links(
        geometric.get("evidence_ids"),
        "endpoint_validation.geometric_reclosure.evidence_ids",
    )
    mechanical_evidence = state.evidence_links(
        mechanical.get("evidence_ids"),
        "endpoint_validation.mechanical_relock.evidence_ids",
    )
    if "ENDPOINT_RECLOSURE" in targets:
        if geometric.get("status") != "PASS":
            state.holds.append(
                "endpoint_validation: geometric reclosure target requires PASS"
            )
        require_known(
            geometric.get("feature_relation_method"),
            "endpoint_validation.geometric_reclosure.feature_relation_method",
            state,
        )
        if not geometric_evidence:
            state.holds.append(
                "endpoint_validation: geometric reclosure requires evidence"
            )
    if "MECHANICAL_RELOCK" in targets:
        if (
            mechanical.get("status") != "PASS"
            or mechanical.get("retention_or_pullout_test") != "PASS"
        ):
            state.errors.append(
                "endpoint_validation: geometric reclosure is not mechanical relock; retention or pull-out test must PASS"
            )
        if not mechanical_evidence:
            state.holds.append(
                "endpoint_validation: mechanical relock requires independent evidence"
            )
        elif set(mechanical_evidence).issubset(set(geometric_evidence)):
            state.errors.append(
                "endpoint_validation: mechanical relock needs evidence beyond geometric reclosure"
            )

    physical = object_value(document, "physical_falsification", state)
    enum_value(
        physical.get("execution_status"),
        execution_values,
        "physical_falsification.execution_status",
        state,
    )
    for key in sorted(PHYSICAL_CHECK_KEYS):
        enum_value(
            physical.get(key), check_values, f"physical_falsification.{key}", state
        )
    if (
        physical.get("result")
        not in {"PHYSICAL_RELEASE_VALIDATED"} | NONPOSITIVE_RESULTS
    ):
        state.errors.append(
            f"physical_falsification.result: unsupported value {physical.get('result')!r}"
        )
    physical_evidence = state.evidence_links(
        physical.get("evidence_ids"), "physical_falsification.evidence_ids"
    )
    if (
        routes.get("s11_physical") == "REQUIRED"
        and physical.get("execution_status") != "EXECUTED"
    ):
        if physical.get("result") != "INCONCLUSIVE":
            state.errors.append(
                "physical_falsification: required route without execution must return INCONCLUSIVE"
            )
        state.holds.append(
            "physical_falsification: required S11 route has no execution"
        )
    if "PHYSICAL_RELEASE" in targets:
        if physical.get("execution_status") != "EXECUTED":
            state.holds.append(
                "physical_falsification: physical release requires executed fixture trial"
            )
        if any(physical.get(key) != "PASS" for key in PHYSICAL_CHECK_KEYS):
            state.errors.append(
                "physical_falsification: all fixture and minimal-trial checks must PASS"
            )
        if physical.get("result") != "PHYSICAL_RELEASE_VALIDATED":
            state.holds.append(
                "physical_falsification: result must be PHYSICAL_RELEASE_VALIDATED"
            )
        if not physical_evidence:
            state.holds.append(
                "physical_falsification: physical release requires evidence"
            )

    outputs = object_value(document, "outputs", state)
    media = object_value(outputs, "blender_media", state, "outputs.blender_media")
    enum_value(
        media.get("execution_status"),
        execution_values,
        "outputs.blender_media.execution_status",
        state,
    )
    enum_value(
        media.get("readback_status"),
        check_values,
        "outputs.blender_media.readback_status",
        state,
    )
    if media.get("result") not in {"DELIVERY_VERIFIED"} | NONPOSITIVE_RESULTS:
        state.errors.append(
            "outputs.blender_media.result: Blender ceiling is DELIVERY_VERIFIED"
        )
    media_evidence = state.evidence_links(
        media.get("evidence_ids"), "outputs.blender_media.evidence_ids"
    )
    if "MEDIA" in targets and (
        media.get("execution_status") != "EXECUTED"
        or media.get("readback_status") != "PASS"
        or media.get("result") != "DELIVERY_VERIFIED"
        or not media_evidence
    ):
        state.holds.append(
            "outputs.blender_media: media target requires executed/read-back DELIVERY_VERIFIED evidence"
        )
    return thread, endpoint, physical, media


def validate_claim_ledger(
    document: dict[str, Any],
    policy: dict[str, Any],
    targets: set[str],
    routes: dict[str, str],
    probe: dict[str, Any],
    contact: dict[str, Any],
    thread: dict[str, Any],
    endpoint: dict[str, Any],
    physical: dict[str, Any],
    media: dict[str, Any],
    state: ValidationState,
) -> dict[str, dict[str, Any]]:
    claim_policy = policy.get("claim_ledger", {})
    claim_map = claim_policy.get("claims", {}) if isinstance(claim_policy, dict) else {}
    expected_claims = list(claim_map)
    common = set(claim_policy.get("common_nonpositive_results", []))
    records = list_value(document, "claim_ledger", state)
    observed = [item.get("claim") for item in records if isinstance(item, dict)]
    if observed != expected_claims:
        state.errors.append(
            f"claim_ledger: claim order {observed}, expected {expected_claims}"
        )
    by_claim: dict[str, dict[str, Any]] = {}
    for index, raw in enumerate(records):
        label = f"claim_ledger[{index}]"
        if not isinstance(raw, dict):
            state.errors.append(f"{label}: expected an object")
            continue
        claim = raw.get("claim")
        if claim not in claim_map:
            state.errors.append(f"{label}.claim: unsupported value {claim!r}")
            continue
        if claim in by_claim:
            state.errors.append(f"{label}.claim: duplicate {claim!r}")
            continue
        by_claim[claim] = raw
        result = raw.get("result")
        allowed = common | set(claim_map[claim])
        if result not in allowed:
            state.errors.append(
                f"{label}.result: {result!r} exceeds the {claim} claim vocabulary"
            )
        bases = string_list(
            raw.get("basis_authorities"), f"{label}.basis_authorities", state
        )
        unknown_bases = sorted(set(bases) - BASIS_AUTHORITIES)
        if unknown_bases:
            state.errors.append(
                f"{label}.basis_authorities: unsupported {unknown_bases}"
            )
        if "BLENDER" in bases and claim != "MEDIA":
            state.errors.append(
                f"{label}: Blender is presentation-only and can support only DELIVERY_VERIFIED media"
            )
        positive = result not in common
        evidence = state.evidence_links(
            raw.get("evidence_ids"), f"{label}.evidence_ids", required=positive
        )
        if positive and claim not in targets:
            state.errors.append(
                f"{label}: positive result requires {claim} in target_claims"
            )
        if claim in targets and result in {"NOT_CLAIMED", "NOT_REQUIRED"}:
            state.errors.append(f"{label}: selected target cannot be {result}")
        if positive and not evidence:
            state.holds.append(f"{label}: positive result lacks evidence")
        if placeholder(raw.get("rationale")):
            state.holds.append(f"{label}.rationale: required")

    def result(claim: str) -> Any:
        return by_claim.get(claim, {}).get("result")

    if result("DIRECTION_RANKING") in POSITIVE_DIRECTION_RESULTS:
        if (
            routes.get("s9_direction_probe") != "REQUIRED"
            or probe.get("execution_status") != "EXECUTED"
        ):
            state.errors.append(
                "claim_ledger.DIRECTION_RANKING: positive result requires executed S9 route"
            )
        if result("DIRECTION_RANKING") != probe.get("result"):
            state.errors.append(
                "claim_ledger.DIRECTION_RANKING: result must match directional_probe.result"
            )
    if (
        routes.get("s9_direction_probe") == "REQUIRED"
        and probe.get("execution_status") != "EXECUTED"
    ):
        if result("DIRECTION_RANKING") != "INCONCLUSIVE":
            state.errors.append(
                "claim_ledger.DIRECTION_RANKING: no execution must be INCONCLUSIVE"
            )

    contact_results = {
        "CONTACT_TRANSFER": "CONTACT_TRANSFER_VALIDATED",
        "PASSAGE": "PASSAGE_VALIDATED",
        "MOTOR_CAPACITY": "MOTOR_CAPACITY_VALIDATED",
    }
    for claim, positive_result in contact_results.items():
        if result(claim) == positive_result and (
            routes.get("p_contact_physics") != "REQUIRED"
            or contact.get("execution_status") != "EXECUTED"
        ):
            state.errors.append(
                f"claim_ledger.{claim}: positive result requires executed P-level route"
            )
        if (
            claim in targets
            and routes.get("p_contact_physics") == "REQUIRED"
            and contact.get("execution_status") != "EXECUTED"
            and result(claim) != "INCONCLUSIVE"
        ):
            state.errors.append(
                f"claim_ledger.{claim}: no execution must be INCONCLUSIVE"
            )
    ladder_result = contact.get("result_ladder")
    if result("CONTACT_TRANSFER") == "CONTACT_TRANSFER_VALIDATED" and (
        ladder_result not in {"CONTACT_TRANSFER_VALIDATED", "PASSAGE_VALIDATED"}
    ):
        state.errors.append(
            "claim_ledger.CONTACT_TRANSFER: contact ladder has not reached validation"
        )
    if (
        result("PASSAGE") == "PASSAGE_VALIDATED"
        and ladder_result != "PASSAGE_VALIDATED"
    ):
        state.errors.append(
            "claim_ledger.PASSAGE: contact ladder has not reached PASSAGE_VALIDATED"
        )

    if (
        result("THREADED_CONTACT") == "THREADED_CONTACT_VALIDATED"
        and thread.get("result") != "THREADED_CONTACT_VALIDATED"
    ):
        state.errors.append(
            "claim_ledger.THREADED_CONTACT: must match dedicated threaded result"
        )
    geometric = (
        endpoint.get("geometric_reclosure", {}) if isinstance(endpoint, dict) else {}
    )
    mechanical = (
        endpoint.get("mechanical_relock", {}) if isinstance(endpoint, dict) else {}
    )
    if (
        result("ENDPOINT_RECLOSURE") == "GEOMETRIC_RECLOSURE_CANDIDATE"
        and geometric.get("status") != "PASS"
    ):
        state.errors.append(
            "claim_ledger.ENDPOINT_RECLOSURE: geometric feature relation has not passed"
        )
    if result("MECHANICAL_RELOCK") == "MECHANICAL_RELOCK_VALIDATED" and (
        mechanical.get("status") != "PASS"
        or mechanical.get("retention_or_pullout_test") != "PASS"
    ):
        state.errors.append(
            "claim_ledger.MECHANICAL_RELOCK: geometric reclosure cannot substitute for retention evidence"
        )
    if (
        result("MEDIA") == "DELIVERY_VERIFIED"
        and media.get("result") != "DELIVERY_VERIFIED"
    ):
        state.errors.append("claim_ledger.MEDIA: must match Blender delivery result")
    if result("MEDIA") == "DELIVERY_VERIFIED" and "BLENDER" not in set(
        by_claim.get("MEDIA", {}).get("basis_authorities", [])
    ):
        state.errors.append(
            "claim_ledger.MEDIA: DELIVERY_VERIFIED requires BLENDER as presentation authority"
        )
    if result("PHYSICAL_RELEASE") == "PHYSICAL_RELEASE_VALIDATED":
        bases = set(by_claim.get("PHYSICAL_RELEASE", {}).get("basis_authorities", []))
        if bases != {"PHYSICAL_FIXTURE"}:
            state.errors.append(
                "claim_ledger.PHYSICAL_RELEASE: only PHYSICAL_FIXTURE may support physical release"
            )
        if physical.get("result") != "PHYSICAL_RELEASE_VALIDATED":
            state.errors.append(
                "claim_ledger.PHYSICAL_RELEASE: simulation cannot substitute for physical trial"
            )
    return by_claim


def validate_document(
    document: Any,
    contract_path: Path,
    policy_path: Path = POLICY_PATH,
) -> tuple[dict[str, Any], int]:
    state = ValidationState()
    policy = load_policy(policy_path, state)
    if not isinstance(document, dict):
        state.errors.append("contract: root must be an object")
        document = {}
    if document.get("$schema") != SCHEMA:
        state.errors.append(
            f"$schema: {document.get('$schema')!r}, expected {SCHEMA!r}"
        )
    require_known(document.get("contract_id"), "contract_id", state)
    require_known(document.get("operation_id"), "operation_id", state)
    if not valid_timestamp(document.get("captured_at")):
        state.holds.append("captured_at: timezone-aware timestamp required")

    pin = object_value(document, "policy_pin", state)
    if policy:
        if pin.get("policy_id") != policy.get("policy_id"):
            state.errors.append("policy_pin.policy_id: does not match bundled policy")
        if pin.get("version") != policy.get("version"):
            state.errors.append("policy_pin.version: does not match bundled policy")
        expected_digest = sha256(policy_path)
        if pin.get("sha256") != expected_digest:
            state.errors.append(
                f"policy_pin.sha256: {pin.get('sha256')!r}, expected {expected_digest!r}"
            )

    claim_map = policy.get("claim_ledger", {}).get("claims", {}) if policy else {}
    target_values = string_list(document.get("target_claims"), "target_claims", state)
    targets = set(target_values)
    unknown_targets = sorted(targets - set(claim_map))
    if unknown_targets:
        state.errors.append(f"target_claims: unsupported claims {unknown_targets}")

    validate_evidence_registry(document, state)
    validate_authority(document, state)
    coordinate, operation = validate_coordinate_and_operation(document, targets, state)
    active_parameters = validate_parameters(document, policy, state)
    check_statuses = set(policy.get("status_vocabularies", {}).get("check_status", []))
    validate_planning(document, coordinate, operation, targets, check_statuses, state)
    routes = validate_route_plan(document, policy, state)

    if (
        operation.get("thread_engagement") == "YES"
        and routes.get("threaded_contact") != "REQUIRED"
    ):
        state.errors.append(
            "route_plan.threaded_contact: thread engagement requires dedicated route"
        )
    if (
        operation.get("thread_engagement") == "UNKNOWN"
        and routes.get("threaded_contact") != "HOLD"
    ):
        state.errors.append(
            "route_plan.threaded_contact: unknown thread applicability must remain HOLD"
        )
    if "THREADED_CONTACT" in targets and routes.get("threaded_contact") != "REQUIRED":
        state.errors.append(
            "route_plan.threaded_contact: target threaded claim requires dedicated route"
        )
    if (
        operation.get("force_or_capacity_claim") == "YES"
        and routes.get("p_contact_physics") != "REQUIRED"
    ):
        state.errors.append(
            "route_plan.p_contact_physics: force or capacity claim requires P-level route"
        )
    if (
        operation.get("force_or_capacity_claim") == "UNKNOWN"
        and routes.get("p_contact_physics") != "HOLD"
    ):
        state.errors.append(
            "route_plan.p_contact_physics: unknown force/capacity applicability must remain HOLD"
        )
    if (
        targets & {"CONTACT_TRANSFER", "PASSAGE", "MOTOR_CAPACITY"}
        and routes.get("p_contact_physics") != "REQUIRED"
    ):
        state.errors.append(
            "route_plan.p_contact_physics: contact-dependent target requires P-level route"
        )
    if (
        operation.get("physical_release_claim") == "YES"
        and routes.get("s11_physical") != "REQUIRED"
    ):
        state.errors.append("route_plan.s11_physical: physical release requires S11")
    if (
        operation.get("physical_release_claim") == "UNKNOWN"
        and routes.get("s11_physical") != "HOLD"
    ):
        state.errors.append(
            "route_plan.s11_physical: unknown physical-release applicability must remain HOLD"
        )
    if "PHYSICAL_RELEASE" in targets and routes.get("s11_physical") != "REQUIRED":
        state.errors.append(
            "route_plan.s11_physical: target physical release requires S11"
        )
    if targets - {"MEDIA"} and routes.get("s7_geometry") != "REQUIRED":
        state.holds.append(
            "route_plan.s7_geometry: engineering target claims require the S7 geometry gate"
        )

    probe, _ = validate_directional_probe(
        document, routes, policy, check_statuses, state
    )
    contact, _ = validate_contact_physics(
        document,
        routes,
        targets,
        active_parameters,
        policy,
        check_statuses,
        state,
    )
    thread, endpoint, physical, media = validate_thread_endpoint_physical_outputs(
        document, routes, targets, policy, state
    )
    ledger = validate_claim_ledger(
        document,
        policy,
        targets,
        routes,
        probe,
        contact,
        thread,
        endpoint,
        physical,
        media,
        state,
    )

    authority = document.get("authority", {})
    if targets & {"CONTACT_TRANSFER", "PASSAGE", "MOTOR_CAPACITY"} and isinstance(
        authority, dict
    ):
        physics = authority.get("physics", {})
        if not isinstance(physics, dict) or physics.get("authoritative") is not True:
            state.holds.append(
                "authority.physics: contact-dependent target requires authoritative modeled-physics record"
            )
    if "PHYSICAL_RELEASE" in targets and isinstance(authority, dict):
        physical_authority = authority.get("physical", {})
        if (
            not isinstance(physical_authority, dict)
            or physical_authority.get("authoritative") is not True
        ):
            state.holds.append(
                "authority.physical: physical release requires authoritative fixture trial"
            )

    known_unknowns = list_value(document, "known_unknowns", state)
    if any(not isinstance(item, str) or not item for item in known_unknowns):
        state.errors.append("known_unknowns: every item must be a non-empty string")
    if known_unknowns:
        state.holds.append(
            f"known_unknowns: {len(known_unknowns)} unresolved declarations"
        )
    declared = document.get("declared_verdict")
    enum_value(
        declared, {"PASS", "HOLD", "FAIL", "INCONCLUSIVE"}, "declared_verdict", state
    )
    for claim in targets:
        result = ledger.get(claim, {}).get("result")
        if result in {"HOLD", "INCONCLUSIVE"}:
            state.holds.append(f"claim_ledger.{claim}: target remains {result}")
    unused_evidence = sorted(state.evidence_registry - state.referenced_evidence)
    if unused_evidence:
        state.warnings.append(
            f"evidence_registry: unreferenced records {unused_evidence}"
        )
    if declared == "PASS" and state.holds:
        state.errors.append("declared_verdict: PASS overclaims unresolved holds")

    if state.errors or declared == "FAIL":
        status = "FAIL"
        exit_code = 1
    elif state.holds or declared in {"HOLD", "INCONCLUSIVE"}:
        status = "HOLD"
        exit_code = 2
    else:
        status = "PASS"
        exit_code = 0
    result = {
        "status": status,
        "declared_verdict": declared,
        "contract": str(contract_path),
        "policy": {
            "path": str(policy_path),
            "policy_id": policy.get("policy_id") if policy else None,
            "version": policy.get("version") if policy else None,
            "sha256": sha256(policy_path) if policy_path.is_file() else None,
        },
        "target_claims": target_values,
        "errors": sorted(set(state.errors)),
        "holds": sorted(set(state.holds)),
        "warnings": sorted(set(state.warnings)),
        "summary": {
            "registered_evidence_count": len(state.evidence_registry),
            "active_parameter_count": len(active_parameters),
            "route_statuses": routes,
            "claim_results": {
                claim: ledger.get(claim, {}).get("result") for claim in claim_map
            },
        },
        "claim_boundary": (
            "This validator checks policy pinning, authority separation, PAB/S9/CP completeness, "
            "parameter portability, route-to-execution consistency, evidence links, and overclaim "
            "guards. It does not execute CAD, collision, libuipc, Factory, Blender, a fixture trial, "
            "or Semantica, and it does not establish that cited evidence is truthful."
        ),
    }
    return result, exit_code


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("contract", type=Path, help="filled algorithm-physics contract")
    parser.add_argument(
        "--policy",
        type=Path,
        default=POLICY_PATH,
        help="pinned routing policy (defaults to the bundled v1 policy)",
    )
    args = parser.parse_args()
    try:
        with args.contract.open("r", encoding="utf-8") as handle:
            document = json.load(handle)
    except OSError as exc:
        print(
            json.dumps(
                {"status": "FAIL", "errors": [f"cannot read contract: {exc}"]}, indent=2
            )
        )
        return 1
    except json.JSONDecodeError as exc:
        print(
            json.dumps(
                {
                    "status": "FAIL",
                    "errors": [
                        f"invalid JSON at line {exc.lineno}, column {exc.colno}: {exc.msg}"
                    ],
                },
                indent=2,
            )
        )
        return 1
    result, exit_code = validate_document(document, args.contract, args.policy)
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
