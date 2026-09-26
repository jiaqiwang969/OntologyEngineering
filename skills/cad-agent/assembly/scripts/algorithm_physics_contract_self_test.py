#!/usr/bin/env python3
"""Run deterministic regressions for the algorithm-physics contract validator."""

from __future__ import annotations

import json
import sys
from copy import deepcopy
from pathlib import Path
from typing import Any, Callable

from validate_algorithm_physics_contract import (
    PAPER_ALGORITHM_IDS,
    POLICY_PATH,
    ROUTE_MATRIX_IDS,
    sha256,
    validate_document,
)


SCRIPT_ROOT = Path(__file__).resolve().parent
SKILL_ROOT = SCRIPT_ROOT.parent
TEMPLATE_PATH = SKILL_ROOT / "assets" / "algorithm-physics-contract.template.json"


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def evidence_record(evidence_id: str, digit: str) -> dict[str, str]:
    return {
        "evidence_id": evidence_id,
        "kind": "SYNTHETIC_TEST",
        "path": f"evidence/{evidence_id.lower()}.json",
        "sha256": digit * 64,
        "captured_at": "2026-08-27T12:00:00Z",
    }


def set_checks_pass(records: list[dict[str, Any]], evidence_id: str) -> None:
    for record in records:
        record["status"] = "PASS"
        record["rationale"] = "Synthetic positive control."
        record["evidence_ids"] = [evidence_id]


def positive_contract() -> dict[str, Any]:
    document = load_json(TEMPLATE_PATH)
    evidence_ids = [
        "E-CAD",
        "E-PLAN",
        "E-S9",
        "E-PHYS",
        "E-THREAD",
        "E-GEO",
        "E-MECH",
        "E-PHYSICAL",
        "E-MEDIA",
        "E-PARAM",
    ]
    document["contract_id"] = "synthetic-algorithm-physics-contract-v1"
    document["operation_id"] = "synthetic-operation-001"
    document["captured_at"] = "2026-08-27T12:00:00Z"
    document["policy_pin"]["sha256"] = sha256(POLICY_PATH)
    document["target_claims"] = [
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
    ]
    document["evidence_registry"] = [
        evidence_record(evidence_id, str(index % 9 + 1))
        for index, evidence_id in enumerate(evidence_ids)
    ]
    document["authority"] = {
        "cad_pose": {
            "system": "FUSION",
            "authoritative": True,
            "source_document_id": "fusion-document-synthetic-001",
            "source_document_sha256": "a" * 64,
            "evidence_ids": ["E-CAD"],
        },
        "physics": {
            "system": "libuipc-synthetic",
            "authoritative": True,
            "claim_ceiling": "MODELED_PHYSICS_ONLY",
            "evidence_ids": ["E-PHYS"],
        },
        "presentation": {
            "system": "BLENDER",
            "read_only": True,
            "claim_ceiling": "DELIVERY_VERIFIED",
            "evidence_ids": ["E-MEDIA"],
        },
        "physical": {
            "system": "FIXTURE_TRIAL",
            "authoritative": True,
            "claim_ceiling": "PHYSICAL_RELEASE_VALIDATED",
            "evidence_ids": ["E-PHYSICAL"],
        },
    }
    document["coordinate_contract"] = {
        "length_unit": "m",
        "angle_unit": "rad",
        "transform_convention": "ROW_MAJOR_PARENT_TO_WORLD",
        "transform_application_count": 1,
        "q0_state_id": "q0",
        "q0_state_sha256": "b" * 64,
        "geometry_set_sha256": "c" * 64,
        "world_pose_set_sha256": "d" * 64,
        "evidence_ids": ["E-CAD"],
    }
    document["operation_profile"] = {
        "operation_class": "COMPOSITE_SYNTHETIC_VALIDATION",
        "mover_occurrence_ids": ["occ-mover-001"],
        "fixed_occurrence_ids": ["occ-fixed-001", "occ-fixed-002"],
        "rigid_subassembly_declared": False,
        "deformable_occurrence_ids": ["occ-flex-001"],
        "thread_engagement": "YES",
        "force_or_capacity_claim": "YES",
        "physical_release_claim": "YES",
        "evidence_ids": ["E-CAD"],
    }
    document["parameter_records"] = [
        {
            "parameter_id": "friction-target",
            "name": "Target friction coefficient",
            "value": 0.31,
            "unit": "1",
            "provenance": "GIVEN",
            "portability": "TARGET_SPECIFIC",
            "active_profile": True,
            "evidence_ids": ["E-PARAM"],
        },
        {
            "parameter_id": "legacy-case-only",
            "name": "Legacy case-only timestep",
            "value": 0.001,
            "unit": "s",
            "provenance": "GIVEN",
            "portability": "CASE_LOCKED_EXAMPLE",
            "active_profile": False,
            "evidence_ids": ["E-PARAM"],
        },
    ]
    document["planning"]["method"] = {
        "method_id": "ATA_REVERSE_DISASSEMBLY",
        "method_class": "PAPER_DERIVED_WITH_LOCAL_ADAPTER",
        "capability_status": "CASE_PROVEN_NOT_GENERALIZED",
        "implementation_identity": "synthetic-adapter-sha256:" + "e" * 64,
        "applicability_status": "APPLICABLE",
        "claim_ceiling": "ORDERED_STATE_CANDIDATE_ONLY",
        "evidence_ids": ["E-PLAN"],
    }
    set_checks_pass(document["planning"]["pab_checks"], "E-PLAN")
    document["planning"]["state_ledger"] = {
        "initial_state_id": "q0",
        "ordered_absolute_state_ids": ["q0", "q1", "q2"],
        "state_count": 3,
        "canonical_state_digest": "f" * 64,
        "mover_occurrence_ids": ["occ-mover-001"],
        "fixed_occurrence_ids": ["occ-fixed-001", "occ-fixed-002"],
        "terminal_state_id": "q2",
        "terminal_goal_class": "EXACT_DISASSEMBLED_GOAL",
        "edge_check_class": "CONTINUOUS_EXACT_OR_BOUND_PROXY",
        "saved_scene_readback_status": "PASS",
        "evidence_ids": ["E-PLAN"],
    }
    document["planning"]["reversal"] = {
        "used": True,
        "mode": "REVERSE_ORDERED_ABSOLUTE_STATES",
        "connector_path_status": "PASS",
        "evidence_ids": ["E-PLAN"],
    }
    for route in document["route_plan"].values():
        route["status"] = "REQUIRED"
        route["rationale"] = "Synthetic positive control requires this branch."

    set_checks_pass(document["directional_probe"]["fairness_checks"], "E-S9")
    document["directional_probe"].update(
        {
            "execution_status": "EXECUTED",
            "candidates": [
                {
                    "candidate_id": "D1",
                    "direction_unit_vector": [1.0, 0.0, 0.0],
                    "start_state_sha256": "1" * 64,
                    "environment_sha256": "2" * 64,
                    "comparison_profile_sha256": "3" * 64,
                    "first_discriminating_feature_reached": True,
                    "contact_detected": False,
                    "classification": "PROMISING_DIRECTION",
                    "evidence_ids": ["E-S9"],
                },
                {
                    "candidate_id": "D2",
                    "direction_unit_vector": [-1.0, 0.0, 0.0],
                    "start_state_sha256": "1" * 64,
                    "environment_sha256": "2" * 64,
                    "comparison_profile_sha256": "3" * 64,
                    "first_discriminating_feature_reached": True,
                    "contact_detected": True,
                    "classification": "RESISTED_DIRECTION",
                    "evidence_ids": ["E-S9"],
                },
            ],
            "first_discriminating_feature_id": "feature-guide-entry",
            "zero_contact_only_basis": False,
            "result": "PROMISING_DIRECTION",
            "stop_action": "RETURN_TO_S6_WITH_DIRECTION_CANDIDATE",
            "evidence_ids": ["E-S9"],
        }
    )
    set_checks_pass(document["contact_physics"]["checks"], "E-PHYS")
    document["contact_physics"].update(
        {
            "execution_status": "EXECUTED",
            "solver_name": "libuipc",
            "solver_version": "synthetic-pinned-v1",
            "drive_mode": "TORQUE_LIMITED_MOTOR_VALIDATION",
            "staged_runs": {
                key: "PASS" for key in document["contact_physics"]["staged_runs"]
            },
            "force_recovery_convention": "force=-contact_gradient/dt^2",
            "action_reaction_status": "PASS",
            "convergence_status": "PASS",
            "capacity_model": {
                "inertia": "PASS",
                "torque_speed_or_current_curve": "PASS",
                "losses": "PASS",
                "controller_and_saturation": "PASS",
                "stall_slip_backdrive": "PASS",
                "evidence_ids": ["E-PHYS"],
            },
            "result_ladder": "PASSAGE_VALIDATED",
            "evidence_ids": ["E-PHYS"],
        }
    )
    document["threaded_contact"] = {
        "execution_status": "EXECUTED",
        "dedicated_model": True,
        "model_identity": "synthetic-thread-model-v1",
        "result": "THREADED_CONTACT_VALIDATED",
        "evidence_ids": ["E-THREAD"],
    }
    document["endpoint_validation"] = {
        "geometric_reclosure": {
            "status": "PASS",
            "feature_relation_method": "SIGNED_FEATURE_DISTANCE_AND_CAPTURE_TOLERANCE",
            "evidence_ids": ["E-GEO"],
        },
        "mechanical_relock": {
            "status": "PASS",
            "retention_or_pullout_test": "PASS",
            "evidence_ids": ["E-MECH"],
        },
    }
    document["physical_falsification"] = {
        "execution_status": "EXECUTED",
        "fixture_calibration": "PASS",
        "reclamp_repeatability": "PASS",
        "machine_observable_completion": "PASS",
        "load_transfer_and_clamp_release": "PASS",
        "fixture_and_tool_withdrawal": "PASS",
        "minimal_high_risk_trial": "PASS",
        "result": "PHYSICAL_RELEASE_VALIDATED",
        "evidence_ids": ["E-PHYSICAL"],
    }
    document["outputs"]["blender_media"] = {
        "execution_status": "EXECUTED",
        "readback_status": "PASS",
        "result": "DELIVERY_VERIFIED",
        "evidence_ids": ["E-MEDIA"],
    }
    claim_values = {
        "ORDERED_PLANNING": (
            "ORDERED_STATES_VERIFIED",
            ["FUSION", "PLANNER"],
            "E-PLAN",
        ),
        "GEOMETRIC_PATH": (
            "GEOMETRY_PATH_VALIDATED",
            ["FUSION", "EXACT_GEOMETRY_CHECKER"],
            "E-GEO",
        ),
        "DIRECTION_RANKING": ("PROMISING_DIRECTION", ["PHYSICS_SOLVER"], "E-S9"),
        "CONTACT_TRANSFER": (
            "CONTACT_TRANSFER_VALIDATED",
            ["PHYSICS_SOLVER"],
            "E-PHYS",
        ),
        "PASSAGE": ("PASSAGE_VALIDATED", ["PHYSICS_SOLVER"], "E-PHYS"),
        "MOTOR_CAPACITY": ("MOTOR_CAPACITY_VALIDATED", ["PHYSICS_SOLVER"], "E-PHYS"),
        "ENDPOINT_RECLOSURE": (
            "GEOMETRIC_RECLOSURE_CANDIDATE",
            ["FUSION", "PHYSICS_SOLVER"],
            "E-GEO",
        ),
        "MECHANICAL_RELOCK": (
            "MECHANICAL_RELOCK_VALIDATED",
            ["PHYSICAL_FIXTURE"],
            "E-MECH",
        ),
        "THREADED_CONTACT": (
            "THREADED_CONTACT_VALIDATED",
            ["THREADED_SOLVER"],
            "E-THREAD",
        ),
        "MEDIA": ("DELIVERY_VERIFIED", ["BLENDER"], "E-MEDIA"),
        "PHYSICAL_RELEASE": (
            "PHYSICAL_RELEASE_VALIDATED",
            ["PHYSICAL_FIXTURE"],
            "E-PHYSICAL",
        ),
    }
    for row in document["claim_ledger"]:
        result, bases, evidence_id = claim_values[row["claim"]]
        row.update(
            {
                "result": result,
                "basis_authorities": bases,
                "evidence_ids": [evidence_id],
                "rationale": "Synthetic positive control with branch-specific evidence.",
            }
        )
    document["known_unknowns"] = []
    document["declared_verdict"] = "PASS"
    return document


def mutate_check_status(
    document: dict[str, Any], path: tuple[str, str], check_id: str, status: str
) -> None:
    records = document[path[0]][path[1]]
    for record in records:
        if record["id"] == check_id:
            record["status"] = status
            record["rationale"] = "Synthetic negative control."
            return
    raise KeyError(check_id)


def main() -> int:
    failures: list[str] = []
    passed: list[str] = []
    base = positive_contract()

    def run_case(
        name: str,
        mutate: Callable[[dict[str, Any]], None] | None,
        expected_code: int,
        expected_fragment: str | None = None,
    ) -> None:
        document = deepcopy(base)
        if mutate is not None:
            mutate(document)
        result, code = validate_document(document, Path(f"<{name}>"))
        messages = result.get("errors", []) + result.get("holds", [])
        if code != expected_code:
            failures.append(
                f"{name}: exit {code}, expected {expected_code}; messages={messages}"
            )
            return
        if expected_fragment is not None and not any(
            expected_fragment in message for message in messages
        ):
            failures.append(
                f"{name}: missing diagnostic {expected_fragment!r}; messages={messages}"
            )
            return
        passed.append(name)

    run_case("positive_complete_contract", None, 0)

    def remove_pab(document: dict[str, Any]) -> None:
        document["planning"]["pab_checks"].pop()

    run_case("reject_missing_pab_invariant", remove_pab, 1, "planning.pab_checks: ids")

    def hold_pab(document: dict[str, Any]) -> None:
        mutate_check_status(document, ("planning", "pab_checks"), "PAB-12", "HOLD")

    run_case("hold_unclosed_pab_invariant", hold_pab, 1, "all PAB-01..PAB-12")

    def remove_s9_facet(document: dict[str, Any]) -> None:
        document["directional_probe"]["fairness_checks"].pop()

    run_case(
        "reject_missing_s9_facet",
        remove_s9_facet,
        1,
        "directional_probe.fairness_checks: ids",
    )

    def zero_contact_promising(document: dict[str, Any]) -> None:
        document["directional_probe"]["zero_contact_only_basis"] = True
        for candidate in document["directional_probe"]["candidates"]:
            candidate["contact_detected"] = False

    run_case(
        "reject_zero_contact_only_promising",
        zero_contact_promising,
        1,
        "zero-contact evidence alone",
    )

    def s9_no_execution_overclaim(document: dict[str, Any]) -> None:
        document["directional_probe"]["execution_status"] = "NOT_RUN"

    run_case(
        "reject_s9_no_execution_overclaim",
        s9_no_execution_overclaim,
        1,
        "required route without execution must return INCONCLUSIVE",
    )

    def contact_no_execution_correct(document: dict[str, Any]) -> None:
        document["contact_physics"]["execution_status"] = "NOT_RUN"
        document["contact_physics"]["result_ladder"] = "INCONCLUSIVE"
        for row in document["claim_ledger"]:
            if row["claim"] in {"CONTACT_TRANSFER", "PASSAGE", "MOTOR_CAPACITY"}:
                row["result"] = "INCONCLUSIVE"
                row["basis_authorities"] = []
                row["evidence_ids"] = []
        document["declared_verdict"] = "HOLD"

    run_case(
        "accept_no_execution_as_inconclusive_hold",
        contact_no_execution_correct,
        2,
        "required P-level route has no numerical execution",
    )

    def contact_no_execution_overclaim(document: dict[str, Any]) -> None:
        document["contact_physics"]["execution_status"] = "NOT_RUN"

    run_case(
        "reject_contact_no_execution_overclaim",
        contact_no_execution_overclaim,
        1,
        "required route without execution must return INCONCLUSIVE",
    )

    def remove_cp(document: dict[str, Any]) -> None:
        document["contact_physics"]["checks"].pop()

    run_case("reject_missing_cp_invariant", remove_cp, 1, "contact_physics.checks: ids")

    def activate_case_locked(document: dict[str, Any]) -> None:
        document["parameter_records"][1]["active_profile"] = True

    run_case(
        "reject_active_case_locked_parameter",
        activate_case_locked,
        1,
        "CASE_LOCKED_EXAMPLE parameter cannot enter the active profile",
    )

    def servo_motor_overclaim(document: dict[str, Any]) -> None:
        document["contact_physics"]["drive_mode"] = "MOTION_DRIVEN_CONTACT_VALIDATION"

    run_case(
        "reject_servo_as_motor_capacity",
        servo_motor_overclaim,
        1,
        "motion-driven servo strength is not motor capacity",
    )

    def geometry_as_relock(document: dict[str, Any]) -> None:
        document["endpoint_validation"]["mechanical_relock"]["status"] = "HOLD"
        document["endpoint_validation"]["mechanical_relock"][
            "retention_or_pullout_test"
        ] = "NOT_RUN"
        document["endpoint_validation"]["mechanical_relock"]["evidence_ids"] = ["E-GEO"]

    run_case(
        "reject_geometric_reclosure_as_mechanical_relock",
        geometry_as_relock,
        1,
        "geometric reclosure is not mechanical relock",
    )

    def blender_engineering_authority(document: dict[str, Any]) -> None:
        for row in document["claim_ledger"]:
            if row["claim"] == "GEOMETRIC_PATH":
                row["basis_authorities"].append("BLENDER")

    run_case(
        "reject_blender_engineering_authority",
        blender_engineering_authority,
        1,
        "Blender is presentation-only",
    )

    def blender_ceiling_overclaim(document: dict[str, Any]) -> None:
        document["outputs"]["blender_media"]["result"] = "PHYSICAL_RELEASE_VALIDATED"

    run_case(
        "reject_blender_beyond_delivery_verified",
        blender_ceiling_overclaim,
        1,
        "Blender ceiling is DELIVERY_VERIFIED",
    )

    def simulation_as_physical_release(document: dict[str, Any]) -> None:
        for row in document["claim_ledger"]:
            if row["claim"] == "PHYSICAL_RELEASE":
                row["basis_authorities"] = ["PHYSICS_SOLVER"]

    run_case(
        "reject_simulation_as_physical_release",
        simulation_as_physical_release,
        1,
        "only PHYSICAL_FIXTURE",
    )

    policy = load_json(POLICY_PATH)
    paper_ids = [
        record.get("id") for record in policy.get("paper_algorithm_routes", [])
    ]
    route_ids = [record.get("id") for record in policy.get("route_matrix", [])]
    if paper_ids != PAPER_ALGORITHM_IDS:
        failures.append(f"policy_paper_routes: {paper_ids} != {PAPER_ALGORITHM_IDS}")
    else:
        passed.append("policy_paper_routes")
    if route_ids != ROUTE_MATRIX_IDS:
        failures.append(f"policy_route_matrix: {route_ids} != {ROUTE_MATRIX_IDS}")
    else:
        passed.append("policy_route_matrix")

    result = {
        "status": "PASS" if not failures else "FAIL",
        "passed": passed,
        "failed": failures,
        "summary": {
            "passed_count": len(passed),
            "failed_count": len(failures),
            "policy_sha256": sha256(POLICY_PATH),
        },
    }
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.path.insert(0, str(SCRIPT_ROOT))
    raise SystemExit(main())
