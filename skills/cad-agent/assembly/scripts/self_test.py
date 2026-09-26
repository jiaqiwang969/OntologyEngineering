#!/usr/bin/env python3
"""Run deterministic, dependency-free regression checks for assembly-ontology."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from copy import deepcopy
from pathlib import Path

from preflight_step import decide, existing_records, inspect_step
from validate_authoring_manifest import REQUIRED_CATEGORIES, sha256, validate_document


def part21(schema: str, body: str, description: str = "fixture") -> bytes:
    return (
        "ISO-10303-21;\n"
        "HEADER;\n"
        f"FILE_DESCRIPTION(('{description}'),'2;1');\n"
        "FILE_NAME('fixture.step','2026-08-26T00:00:00',('test'),('test'),'','','');\n"
        f"FILE_SCHEMA(('{schema}'));\n"
        "ENDSEC;\n"
        "DATA;\n"
        f"{body}\n"
        "ENDSEC;\n"
        "END-ISO-10303-21;\n"
    ).encode("ascii")


def check(condition: bool, message: str, errors: list[str]) -> None:
    if not condition:
        errors.append(message)


def complete_authoring_fixture(occurrence_count: int = 2) -> dict[str, object]:
    if occurrence_count < 2:
        raise ValueError(
            "fixture requires at least one structural occurrence and one nut"
        )
    evidence_id = "evidence:fixture/control"
    structural_count = occurrence_count - 1
    structural_occurrences = []
    placement_contracts = []
    reconciliation_rows = []
    for index in range(occurrence_count):
        is_nut = index == occurrence_count - 1
        stem = "nut" if is_nut else "struct"
        serial = 1 if is_nut else index + 1
        occurrence_id = f"req-occ-{stem}-{serial:03d}"
        source_id = f"src-occ-{stem}-{serial:03d}"
        fusion_id = f"fus-occ-{stem}-{serial:03d}"
        placement_id = f"place-{stem}-{serial:03d}"
        occurrence = {
            "occurrence_id": occurrence_id,
            "source_occurrence_id": source_id,
            "fusion_occurrence_id": fusion_id,
            "geometry_definition_id": "def-nut" if is_nut else "def-struct",
            "placement_contract_id": placement_id,
            "presence": "EXACT",
            "evidence_ids": [evidence_id],
        }
        if not is_nut:
            structural_occurrences.append(occurrence)
        placement_contracts.append(
            {
                "placement_contract_id": placement_id,
                "requirement_occurrence_id": occurrence_id,
                "fusion_occurrence_id": fusion_id,
                "lineage_source_occurrence_id": source_id,
                "host_fusion_occurrence_ids": ["fusion-root-anchor"]
                if index == 0
                else ["fus-occ-struct-001"],
                "world_transform": [
                    1,
                    0,
                    0,
                    index,
                    0,
                    1,
                    0,
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    0,
                    0,
                    1,
                ],
                "transform_convention": "ROW_MAJOR_PARENT_TO_WORLD",
                "placement_status": "PASS",
                "transform_status": "PASS",
                "lineage_status": "PASS",
                "joint_id": None,
                "evidence_ids": [evidence_id],
            }
        )
        reconciliation_rows.append(
            {
                "row_id": f"recon-{index + 1:03d}",
                "source_occurrence_id": source_id,
                "source_definition_id": "source-def-nut"
                if is_nut
                else "source-def-struct",
                "fusion_occurrence_id": fusion_id,
                "fusion_definition_id": "def-nut" if is_nut else "def-struct",
                "disposition": "MATCHED",
                "rationale": None,
                "evidence_ids": [evidence_id],
            }
        )
    nut_occurrence = {
        "occurrence_id": "req-occ-nut-001",
        "source_occurrence_id": "src-occ-nut-001",
        "fusion_occurrence_id": "fus-occ-nut-001",
        "geometry_definition_id": "def-nut",
        "placement_contract_id": "place-nut-001",
        "presence": "EXACT",
        "evidence_ids": [evidence_id],
    }
    categories = []
    for category in sorted(REQUIRED_CATEGORIES):
        expected = (
            structural_count
            if category == "STRUCTURAL"
            else 1
            if category == "NUT"
            else 0
        )
        categories.append(
            {
                "category": category,
                "expected_count": expected,
                "present_exact_count": expected,
                "present_proxy_count": 0,
                "missing_count": 0,
                "unknown_count": 0,
                "status": "PASS" if expected else "NOT_APPLICABLE",
                "evidence_ids": [evidence_id] if expected else [],
                "not_applicable_rationale": None
                if expected
                else "Controlled fixture has no requirement in this category.",
                "not_applicable_evidence_ids": [] if expected else [evidence_id],
            }
        )
    return {
        "$schema": "cad-agent.assembly-authoring-manifest/v2",
        "project_id": "assembly-fixture",
        "captured_at": "2026-08-27T00:00:00Z",
        "target_claims": [
            "SOURCE_CLOSURE",
            "TREE_REPRODUCTION",
            "CONTENT_COMPLETENESS",
            "NATIVE_GEOMETRY",
            "PERSISTED_FUSION_ASSEMBLY",
        ],
        "release_intent": "SYNTHETIC_TEST_ONLY",
        "evidence_registry": [
            {
                "evidence_id": evidence_id,
                "kind": "SYNTHETIC_TEST",
                "path": "evidence/synthetic-control.json",
                "sha256": "1" * 64,
                "media_type": "application/json",
                "captured_at": "2026-08-27T00:00:00Z",
            }
        ],
        "sources": [
            {
                "source_id": "root",
                "role": "ASSEMBLY_ROOT",
                "carrier_class": "STEP",
                "path": "fixture.step",
                "sha256": "0" * 64,
                "size_bytes": 1,
                "revision": "fixture-v1",
                "authority": "synthetic-test",
                "rights_status": "GRANTED_PRIVATE_USE",
                "rights_evidence_ids": [evidence_id],
            }
        ],
        "selected_root": {
            "source_id": "root",
            "configuration": "default",
            "selection_basis_evidence_ids": [evidence_id],
        },
        "source_closure": {
            "expected_external_file_count": 0,
            "resolved_external_file_count": 0,
            "unresolved_reference_count": 0,
            "definition_count": 2,
            "saved_occurrence_count": occurrence_count,
            "expanded_occurrence_count": occurrence_count,
            "independent_tree_crosscheck": "PASS",
            "file_records": [
                {
                    "file_id": "file-root",
                    "file_kind": "ROOT",
                    "resolution_status": "RESOLVED",
                    "source_id": "root",
                    "evidence_ids": [evidence_id],
                }
            ],
            "definition_records": [
                {
                    "source_definition_id": "source-def-struct",
                    "definition_kind": "PART",
                    "file_id": "file-root",
                    "evidence_ids": [evidence_id],
                },
                {
                    "source_definition_id": "source-def-nut",
                    "definition_kind": "PURCHASED_HARDWARE",
                    "file_id": "file-root",
                    "evidence_ids": [evidence_id],
                },
            ],
            "reference_edges": [],
            "evidence_ids": [evidence_id],
        },
        "geometry_policy": {
            "accepted_native_classes": [
                "NATIVE_PARAMETRIC",
                "IMPORTED_BREP",
                "SUPPLIER_EXACT_BREP",
            ],
            "triangle_mesh_delivery_allowed": False,
            "proxy_claim_limit": "DISPLAY_ONLY",
            "supplier_exact_required_for_manufacturing": True,
            "mesh_to_brep_selfcheck_required": True,
        },
        "geometry_definitions": [
            {
                "definition_id": "def-struct",
                "geometry_class": "IMPORTED_BREP",
                "provenance_kind": "CONTROLLED_BREP_IMPORT",
                "source_ids": ["root"],
                "source_definition_id": "source-def-struct",
                "faceted_or_tessellated": False,
                "brep_body_count": 1,
                "mesh_body_count": 0,
                "surface_only_or_empty_count": 0,
                "evidence_ids": [evidence_id],
            },
            {
                "definition_id": "def-nut",
                "geometry_class": "SUPPLIER_EXACT_BREP",
                "provenance_kind": "SUPPLIER_BREP",
                "source_ids": ["root"],
                "source_definition_id": "source-def-nut",
                "faceted_or_tessellated": False,
                "brep_body_count": 1,
                "mesh_body_count": 0,
                "surface_only_or_empty_count": 0,
                "evidence_ids": [evidence_id],
            },
        ],
        "completeness": {
            "procurement_package_is_not_installed_count": True,
            "expected_physical_occurrence_count": occurrence_count,
            "requirements": [
                {
                    "requirement_id": "req-struct",
                    "category": "STRUCTURAL",
                    "item_identity": "fixture structural part",
                    "quantity_semantics": "EXACT",
                    "quantity": {
                        "lower_bound": structural_count,
                        "upper_bound": structural_count,
                        "selected_installed_count": structural_count,
                        "procurement_pack_size": None,
                    },
                    "status": "PASS",
                    "rationale": None,
                    "evidence_ids": [evidence_id],
                    "occurrences": structural_occurrences,
                },
                {
                    "requirement_id": "req-nut",
                    "category": "NUT",
                    "item_identity": "fixture M3 nut",
                    "quantity_semantics": "EXACT",
                    "quantity": {
                        "lower_bound": 1,
                        "upper_bound": 1,
                        "selected_installed_count": 1,
                        "procurement_pack_size": None,
                    },
                    "status": "PASS",
                    "rationale": None,
                    "evidence_ids": [evidence_id],
                    "occurrences": [nut_occurrence],
                },
            ],
            "categories": categories,
            "unresolved_items": [],
        },
        "placement_root": {
            "root_anchor_id": "fusion-root-anchor",
            "anchor_type": "FUSION_ROOT_COMPONENT",
            "status": "PASS",
            "evidence_ids": [evidence_id],
        },
        "placement_contracts": placement_contracts,
        "joint_records": [],
        "joint_validation": {
            "status": "NOT_APPLICABLE",
            "expected_joint_count": 0,
            "healthy_joint_count": 0,
            "unhealthy_joint_count": 0,
            "rationale": "Static fixture is constrained by verified rigid placements.",
            "evidence_ids": [evidence_id],
        },
        "fusion_target": {
            "document_name": "Assembly Fixture v1",
            "destination_project": "Test",
            "destination_folder": "Fixtures",
            "overwrite_allowed": False,
            "overwrite_authority_evidence_ids": [],
            "one_writer": True,
            "writer_receipt_evidence_ids": [evidence_id],
        },
        "native_readback": {
            "status": "PASS",
            "document_name": "Assembly Fixture v1",
            "document_creation_id": "creation-fixture-v1",
            "is_saved": True,
            "is_modified_after_save": False,
            "data_file_identity": "data-file-fixture-v1",
            "component_definition_count": 2,
            "occurrence_count": occurrence_count,
            "brep_body_count": 2,
            "mesh_body_count": 0,
            "surface_only_or_empty_count": 0,
            "unresolved_external_reference_count": 0,
            "duplicate_occurrence_count": 0,
            "orphan_occurrence_count": 0,
            "close_reopen_status": "PASS",
            "evidence_ids": [evidence_id],
            "second_readback": {
                "status": "PASS",
                "document_creation_id": "creation-fixture-v1",
                "data_file_identity": "data-file-fixture-v1",
                "occurrence_count": occurrence_count,
                "evidence_ids": [evidence_id],
            },
        },
        "occurrence_reconciliation": {
            "status": "PASS",
            "rows": reconciliation_rows,
            "evidence_ids": [evidence_id],
        },
        "s0_s11_handoff": {
            "status": "NOT_RUN",
            "project_input_id": "not-requested",
            "certified_motion_count": None,
            "uncertified_motion_count": None,
            "scene_readback_status": "NOT_RUN",
            "media_decode_status": "NOT_RUN",
            "media_sha256": "UNKNOWN",
            "evidence_ids": [],
        },
        "declared_verdict": "PASS",
        "known_unknowns": [],
        "claim_boundary": "Synthetic validator regression only.",
    }


def bind_authoring_fixture_files(
    fixture: dict[str, object], source_path: Path, evidence_path: Path
) -> dict[str, object]:
    fixture["sources"][0].update(
        {
            "path": source_path.name,
            "sha256": sha256(source_path),
            "size_bytes": source_path.stat().st_size,
        }
    )
    fixture["evidence_registry"][0].update(
        {
            "path": evidence_path.name,
            "sha256": sha256(evidence_path),
        }
    )
    return fixture


def main() -> int:
    errors: list[str] = []
    observations: dict[str, object] = {}
    with tempfile.TemporaryDirectory(prefix="assembly-ontology-self-test-") as raw_root:
        root = Path(raw_root)
        assembly = root / "minified.step"
        assembly.write_bytes(
            part21(
                "AUTOMOTIVE_DESIGN",
                "#1=PRODUCT('real product','fake ADVANCED_FACE (', '',());"
                "/* #90=ADVANCED_FACE('',(),$); #91=NEXT_ASSEMBLY_USAGE_OCCURRENCE('','','',#1,#2,$); */"
                "#2 = NEXT_ASSEMBLY_USAGE_OCCURRENCE\n('','','',#3,#4,$);"
                "#3=MANIFOLD_SOLID_BREP('',#5);",
            )
        )
        inspected = inspect_step(assembly)
        check(
            inspected["schema_family_observed"] == "AP214",
            "AUTOMOTIVE_DESIGN must map to AP214",
            errors,
        )
        check(
            inspected["assembly_structure_signal_count"] == 1,
            "real multiline NAUO must count exactly once",
            errors,
        )
        check(
            inspected["geometry_signal_count"] == 1,
            "comment/string pseudo-entities must not count",
            errors,
        )
        check(
            inspected["entity_signals"]["product"] == 1,
            "one real PRODUCT expected",
            errors,
        )
        observations["ap214_minified_multiline"] = {
            "assembly": inspected["assembly_structure_signal_count"],
            "geometry": inspected["geometry_signal_count"],
        }

        ap242 = root / "ap242.step"
        ap242.write_bytes(
            part21(
                "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF",
                "#1=OPEN_SHELL('',());",
            )
        )
        inspected_ap242 = inspect_step(ap242)
        check(
            inspected_ap242["schema_family_observed"] == "AP242",
            "AP242 long schema must map to AP242",
            errors,
        )
        check(
            inspected_ap242["geometry_signal_count"] == 1,
            "AP242 OPEN_SHELL must be a geometry signal",
            errors,
        )

        ap203 = root / "ap203.step"
        ap203.write_bytes(
            part21(
                "CONFIG_CONTROL_DESIGN", "#1=MANIFOLD_SOLID_BREP('',#2);", "STEP AP203"
            )
        )
        inspected_ap203 = inspect_step(ap203)
        check(
            inspected_ap203["schema_family_observed"] == "AP203",
            "CONFIG_CONTROL_DESIGN must map to AP203",
            errors,
        )
        check(
            inspected_ap203["assembly_structure_signal_count"] == 0,
            "single-part fixture must not look like an assembly",
            errors,
        )

        empty = root / "empty.step"
        empty.write_bytes(b"")
        inspected_empty = inspect_step(empty)
        check(
            not any(inspected_empty["container_markers"].values()),
            "empty file must have no container markers",
            errors,
        )
        empty_status, _ = decide(inspected_empty, [], [], [])
        check(
            empty_status == "HOLD_INVALID_STEP_CONTAINER",
            "empty file must return container HOLD",
            errors,
        )
        empty_run = subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("preflight_step.py")),
                str(empty),
                "--json",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        check(
            empty_run.returncode == 3,
            "empty-file CLI must exit 3 without traceback",
            errors,
        )
        try:
            empty_report = json.loads(empty_run.stdout)
        except json.JSONDecodeError:
            empty_report = {}
            errors.append("empty-file CLI did not emit JSON")
        check(
            empty_report.get("status") == "HOLD_INVALID_STEP_CONTAINER",
            "empty-file CLI status is wrong",
            errors,
        )

        missing_output = root / "missing-report.json"
        missing_run = subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("preflight_step.py")),
                str(root / "absent.step"),
                "--output",
                str(missing_output),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        check(missing_run.returncode == 2, "missing-input CLI must exit 2", errors)
        check(
            missing_output.is_file(),
            "missing-input --output report was not written",
            errors,
        )
        if missing_output.is_file():
            missing_report = json.loads(missing_output.read_text(encoding="utf-8"))
            check(
                missing_report.get("status") == "HOLD_INPUT_MISSING",
                "missing-input report status is wrong",
                errors,
            )

        renamed = root / "renamed.bin"
        renamed.write_bytes(assembly.read_bytes())
        renamed_run = subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("preflight_step.py")),
                str(renamed),
                "--json",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        renamed_report = json.loads(renamed_run.stdout)
        check(
            renamed_run.returncode == 0,
            "valid renamed Part 21 input should remain READY",
            errors,
        )
        check(
            "UNEXPECTED_STEP_SUFFIX" in renamed_report.get("warnings", []),
            "renamed valid input must carry a suffix warning",
            errors,
        )

        bom = root / "bom.json"
        sop = root / "sop.json"
        bom.write_text("{}\n", encoding="utf-8")
        sop.write_text("{}\n", encoding="utf-8")
        bom_records, _ = existing_records([str(bom)], "bom")
        sop_records, _ = existing_records([str(sop)], "sop")
        base_step = inspect_step(assembly)
        bom_status, bom_holds = decide(base_step, [], bom_records, [])
        sop_status, sop_holds = decide(base_step, sop_records, [], [])
        both_status, both_holds = decide(base_step, sop_records, bom_records, [])
        missing_status, missing_holds = decide(
            base_step, [], [], [str(root / "missing.json")]
        )
        check(
            bom_status == "READY_FOR_S0_IDENTITY_REVIEW__HOLD_PROCESS_TRUTH",
            "BOM-only lane is wrong",
            errors,
        )
        check(
            "PROCESS_TRUTH_NOT_SUPPLIED" in bom_holds,
            "BOM must not satisfy process truth",
            errors,
        )
        check(
            sop_status == "READY_FOR_S0_PROCESS_REVIEW__HOLD_IDENTITY_TRUTH",
            "SOP-only lane is wrong",
            errors,
        )
        check(
            "IDENTITY_RECORD_NOT_SUPPLIED" in sop_holds,
            "SOP must not satisfy BOM identity truth",
            errors,
        )
        check(
            both_status == "READY_FOR_S0_CONTROLLED_RECORD_VALIDATION",
            "BOM+SOP must stop at record validation",
            errors,
        )
        check(
            "CONTROLLED_RECORD_SCHEMA_NOT_VALIDATED" in both_holds,
            "record content must remain unvalidated",
            errors,
        )
        check(
            missing_status == "HOLD_PRE_S1_REVIEW_REQUIRED",
            "missing declared record must HOLD",
            errors,
        )
        check(
            "DECLARED_RECORD_FILE_MISSING" in missing_holds,
            "missing record hold is absent",
            errors,
        )
        observations["record_lanes"] = {
            "bom_only": bom_status,
            "sop_only": sop_status,
            "both": both_status,
            "missing": missing_status,
        }

        controlled_evidence = root / "synthetic-control.json"
        controlled_evidence.write_text('{"fixture": true}\n', encoding="utf-8")
        controlled_media = root / "synthetic-motion.mp4"
        controlled_media.write_bytes(
            b"synthetic structurally hash-bound media fixture\n"
        )
        authoring_fixture = bind_authoring_fixture_files(
            complete_authoring_fixture(), assembly, controlled_evidence
        )
        authoring_result, authoring_exit = validate_document(
            authoring_fixture, root / "authoring-fixture.json", check_files=True
        )
        check(authoring_exit == 0, "complete authoring fixture must exit 0", errors)
        check(
            authoring_result.get("status") == "PASS",
            "complete authoring fixture must PASS",
            errors,
        )

        checked_files = deepcopy(authoring_fixture)
        checked_result, checked_exit = validate_document(
            checked_files, root / "checked-files-manifest.json", check_files=True
        )
        check(
            checked_exit == 0,
            "hash-bound source/evidence fixture must PASS --check-files",
            errors,
        )
        check(
            checked_result.get("status") == "PASS",
            "checked-file fixture must PASS",
            errors,
        )

        tampered_evidence_hash = deepcopy(checked_files)
        tampered_evidence_hash["evidence_registry"][0]["sha256"] = "f" * 64
        tampered_result, tampered_exit = validate_document(
            tampered_evidence_hash,
            root / "tampered-evidence-hash.json",
            check_files=True,
        )
        check(
            tampered_exit == 1, "tampered evidence hash must fail --check-files", errors
        )
        check(
            any(
                "evidence_registry[0].sha256" in item
                for item in tampered_result["errors"]
            ),
            "tampered evidence hash must identify its registry record",
            errors,
        )

        unchecked_claims = deepcopy(authoring_fixture)
        unchecked_claims["declared_verdict"] = "HOLD"
        unchecked_result, unchecked_exit = validate_document(
            unchecked_claims, root / "unchecked-claims.json", check_files=False
        )
        check(
            unchecked_exit == 2, "unchecked engineering claims must return HOLD", errors
        )
        check(
            unchecked_result.get("status") == "HOLD",
            "unchecked claims cannot return PASS",
            errors,
        )
        check(
            any("require --check-files" in item for item in unchecked_result["holds"]),
            "unchecked claim must retain the explicit file-verification hold",
            errors,
        )

        quantity_semantics_results: dict[str, str] = {
            "EXACT": authoring_result.get("status")
        }
        for semantics, quantity in (
            (
                "MINIMUM",
                {
                    "lower_bound": 1,
                    "upper_bound": None,
                    "selected_installed_count": 1,
                    "procurement_pack_size": None,
                },
            ),
            (
                "RANGE",
                {
                    "lower_bound": 1,
                    "upper_bound": 2,
                    "selected_installed_count": 1,
                    "procurement_pack_size": None,
                },
            ),
            (
                "OPTIONAL",
                {
                    "lower_bound": 0,
                    "upper_bound": 1,
                    "selected_installed_count": 1,
                    "procurement_pack_size": None,
                },
            ),
        ):
            quantity_fixture = deepcopy(authoring_fixture)
            requirement = next(
                row
                for row in quantity_fixture["completeness"]["requirements"]
                if row["category"]
                == ("NUT" if semantics == "OPTIONAL" else "STRUCTURAL")
            )
            requirement["quantity_semantics"] = semantics
            requirement["quantity"] = quantity
            quantity_result, quantity_exit = validate_document(
                quantity_fixture,
                root / f"quantity-{semantics.lower()}.json",
                check_files=True,
            )
            check(
                quantity_exit == 0, f"resolved {semantics} quantity must PASS", errors
            )
            quantity_semantics_results[semantics] = quantity_result.get("status")

        procurement_pack = deepcopy(authoring_fixture)
        procurement_pack["completeness"]["requirements"].append(
            {
                "requirement_id": "req-procurement-pack",
                "category": "OTHER_REQUIRED",
                "item_identity": "fixture purchase package only",
                "quantity_semantics": "PROCUREMENT_PACK",
                "quantity": {
                    "lower_bound": None,
                    "upper_bound": None,
                    "selected_installed_count": None,
                    "procurement_pack_size": 10,
                },
                "status": "NOT_APPLICABLE",
                "rationale": "Purchasing package is not an installed occurrence count.",
                "evidence_ids": ["evidence:fixture/control"],
                "occurrences": [],
            }
        )
        procurement_result, procurement_exit = validate_document(
            procurement_pack, root / "quantity-procurement-pack.json", check_files=True
        )
        check(
            procurement_exit == 0,
            "PROCUREMENT_PACK must not inflate installed count",
            errors,
        )
        quantity_semantics_results["PROCUREMENT_PACK"] = procurement_result.get(
            "status"
        )

        unknown_upper = deepcopy(authoring_fixture)
        unknown_requirement = next(
            row
            for row in unknown_upper["completeness"]["requirements"]
            if row["category"] == "NUT"
        )
        unknown_requirement.update(
            {
                "quantity_semantics": "UNKNOWN_UPPER",
                "quantity": {
                    "lower_bound": 1,
                    "upper_bound": None,
                    "selected_installed_count": None,
                    "procurement_pack_size": None,
                },
                "status": "UNKNOWN",
                "rationale": "One nut is evidenced, but the source upper bound is not closed.",
            }
        )
        next(
            row
            for row in unknown_upper["completeness"]["categories"]
            if row["category"] == "NUT"
        )["status"] = "UNKNOWN"
        unknown_upper["completeness"]["expected_physical_occurrence_count"] = None
        unknown_upper["declared_verdict"] = "HOLD"
        unknown_result, unknown_exit = validate_document(
            unknown_upper, root / "quantity-unknown-upper.json", check_files=True
        )
        check(unknown_exit == 2, "UNKNOWN_UPPER must remain a structural HOLD", errors)
        check(
            not unknown_result["errors"],
            "honest UNKNOWN_UPPER must not be structurally invalid",
            errors,
        )
        quantity_semantics_results["UNKNOWN_UPPER"] = unknown_result.get("status")

        process_fixture = deepcopy(authoring_fixture)
        process_fixture["target_claims"].append("PROCESS_AND_MOTION")
        process_fixture["evidence_registry"].append(
            {
                "evidence_id": "evidence:fixture/motion-media",
                "kind": "PROCESS_RECORD",
                "path": controlled_media.name,
                "sha256": sha256(controlled_media),
                "media_type": "video/mp4",
                "captured_at": "2026-08-27T00:00:00Z",
            }
        )
        process_fixture["s0_s11_handoff"].update(
            {
                "status": "PASS",
                "project_input_id": "fixture-project-input-v1",
                "certified_motion_count": 1,
                "uncertified_motion_count": 0,
                "scene_readback_status": "PASS",
                "media_decode_status": "PASS",
                "media_sha256": sha256(controlled_media),
                "evidence_ids": ["evidence:fixture/motion-media"],
            }
        )
        process_result, process_exit = validate_document(
            process_fixture, root / "process-and-motion.json", check_files=True
        )
        check(
            process_exit == 0,
            "hash-bound positive PROCESS_AND_MOTION fixture must PASS",
            errors,
        )

        zero_motion = deepcopy(process_fixture)
        zero_motion["s0_s11_handoff"]["certified_motion_count"] = 0
        zero_motion_result, zero_motion_exit = validate_document(
            zero_motion, root / "zero-certified-motion.json", check_files=True
        )
        check(
            zero_motion_exit == 1,
            "zero certified motions cannot PASS PROCESS_AND_MOTION",
            errors,
        )

        unbound_media = deepcopy(process_fixture)
        unbound_media["s0_s11_handoff"]["media_sha256"] = "0" * 64
        unbound_media_result, unbound_media_exit = validate_document(
            unbound_media, root / "unbound-media-hash.json", check_files=True
        )
        check(
            unbound_media_exit == 1,
            "unbound media hash cannot PASS PROCESS_AND_MOTION",
            errors,
        )
        check(
            any(
                "cited hash-checked video" in item
                for item in unbound_media_result["errors"]
            ),
            "unbound media hash must retain a registry-binding error",
            errors,
        )

        mesh_overclaim = deepcopy(authoring_fixture)
        mesh_overclaim["native_readback"]["mesh_body_count"] = 1
        mesh_result, mesh_exit = validate_document(
            mesh_overclaim, root / "mesh-overclaim.json", check_files=True
        )
        check(
            mesh_exit == 1,
            "mesh delivery with declared PASS must fail overclaim checking",
            errors,
        )
        check(
            mesh_result.get("status") == "FAIL",
            "mesh delivery overclaim must FAIL",
            errors,
        )
        check(
            any("mesh bodies" in item for item in mesh_result.get("holds", [])),
            "mesh delivery overclaim must retain the native-geometry hold",
            errors,
        )

        unregistered_evidence = deepcopy(authoring_fixture)
        unregistered_evidence["selected_root"]["selection_basis_evidence_ids"] = [
            "evidence:fixture/not-registered"
        ]
        unregistered_result, unregistered_exit = validate_document(
            unregistered_evidence, root / "unregistered-evidence.json", check_files=True
        )
        check(
            unregistered_exit == 1, "unregistered evidence reference must fail", errors
        )
        check(
            any(
                "unregistered evidence IDs" in item
                for item in unregistered_result.get("errors", [])
            ),
            "unregistered evidence failure must identify the missing registry entry",
            errors,
        )

        malformed_enum = deepcopy(authoring_fixture)
        malformed_enum["geometry_definitions"][0]["geometry_class"] = []
        malformed_result, malformed_exit = validate_document(
            malformed_enum, root / "malformed-enum.json", check_files=True
        )
        check(
            malformed_exit == 1,
            "malformed enum value must fail without a traceback",
            errors,
        )

        singular_transform = deepcopy(authoring_fixture)
        singular_transform["placement_contracts"][0]["world_transform"] = [0] * 16
        singular_result, singular_exit = validate_document(
            singular_transform, root / "singular-transform.json", check_files=True
        )
        check(
            singular_exit == 1,
            "singular all-zero placement transform must fail",
            errors,
        )

        hostless_placements = deepcopy(authoring_fixture)
        for contract in hostless_placements["placement_contracts"]:
            contract["host_fusion_occurrence_ids"] = []
        hostless_result, hostless_exit = validate_document(
            hostless_placements, root / "hostless-placements.json", check_files=True
        )
        check(hostless_exit == 1, "unanchored placement forest must fail", errors)
        check(
            any("root_anchor_id" in item for item in hostless_result["errors"]),
            "hostless placements must retain an explicit root-anchor error",
            errors,
        )

        sibling_top_level = deepcopy(authoring_fixture)
        sibling_top_level["placement_contracts"][1]["host_fusion_occurrence_ids"] = [
            "fusion-root-anchor"
        ]
        sibling_result, sibling_exit = validate_document(
            sibling_top_level, root / "sibling-top-level.json", check_files=True
        )
        check(
            sibling_exit == 0,
            "multiple evidence-backed sibling top-level placements must PASS",
            errors,
        )

        empty_requirements = deepcopy(authoring_fixture)
        empty_requirements["completeness"]["requirements"] = []
        empty_requirements_result, empty_requirements_exit = validate_document(
            empty_requirements, root / "empty-requirements.json", check_files=True
        )
        check(
            empty_requirements_exit == 1,
            "empty requirement ledger with stale placements must fail without a traceback",
            errors,
        )
        check(
            any("right-handed rigid 4x4" in item for item in singular_result["errors"]),
            "singular transform must retain a rigid-transform error",
            errors,
        )

        unbound_source_definition = deepcopy(authoring_fixture)
        unbound_source_definition["geometry_definitions"][0]["source_definition_id"] = (
            "source-def-not-enumerated"
        )
        unbound_definition_result, unbound_definition_exit = validate_document(
            unbound_source_definition,
            root / "unbound-source-definition.json",
            check_files=True,
        )
        check(
            unbound_definition_exit == 1,
            "source-bound geometry must resolve its source definition ledger identity",
            errors,
        )
        check(
            any(
                "requires a ledger definition" in item
                for item in unbound_definition_result["errors"]
            ),
            "unbound source definition must retain a provenance error",
            errors,
        )
        check(
            any(
                "geometry_class: unsupported value" in item
                for item in malformed_result["errors"]
            ),
            "malformed enum must produce a structured validation error",
            errors,
        )

        concealed_nut = deepcopy(authoring_fixture)
        requirements = concealed_nut["completeness"]["requirements"]
        structural_requirement = next(
            row for row in requirements if row["category"] == "STRUCTURAL"
        )
        nut_requirement = next(row for row in requirements if row["category"] == "NUT")
        structural_requirement["occurrences"].extend(nut_requirement["occurrences"])
        structural_requirement["quantity"].update(
            {
                "lower_bound": 2,
                "upper_bound": 2,
                "selected_installed_count": 2,
            }
        )
        concealed_nut["completeness"]["requirements"] = [structural_requirement]
        structural_row = next(
            row
            for row in concealed_nut["completeness"]["categories"]
            if row["category"] == "STRUCTURAL"
        )
        structural_row.update({"expected_count": 2, "present_exact_count": 2})
        nut_row = next(
            row
            for row in concealed_nut["completeness"]["categories"]
            if row["category"] == "NUT"
        )
        nut_row.update(
            {
                "expected_count": 0,
                "present_exact_count": 0,
                "status": "NOT_APPLICABLE",
                "evidence_ids": [],
                "not_applicable_rationale": None,
                "not_applicable_evidence_ids": [],
            }
        )
        concealed_nut_result, concealed_nut_exit = validate_document(
            concealed_nut, root / "concealed-nut.json", check_files=True
        )
        check(
            concealed_nut_exit == 1,
            "concealed nut by unsupported N/A must fail",
            errors,
        )
        check(
            any(
                "not_applicable" in item
                for item in concealed_nut_result.get("holds", [])
            ),
            "concealed nut must retain an evidence/rationale N/A hold",
            errors,
        )

        disguised_triangulated_brep = deepcopy(authoring_fixture)
        disguised_triangulated_brep["geometry_policy"][
            "accepted_native_classes"
        ].append("TRIANGULATED_BREP")
        disguised_triangulated_brep["geometry_definitions"][1]["geometry_class"] = (
            "TRIANGULATED_BREP"
        )
        disguised_result, disguised_exit = validate_document(
            disguised_triangulated_brep,
            root / "disguised-triangulated-brep.json",
            check_files=True,
        )
        check(
            disguised_exit == 1,
            "triangulated B-Rep cannot enter the native enum",
            errors,
        )
        check(
            any(
                "strict native enum" in item
                for item in disguised_result.get("errors", [])
            ),
            "triangulated B-Rep attack must fail the accepted-class whitelist",
            errors,
        )

        faceted_laundering = deepcopy(authoring_fixture)
        faceted_definition = faceted_laundering["geometry_definitions"][1]
        faceted_definition.update(
            {
                "geometry_class": "IMPORTED_BREP",
                "provenance_kind": "TESSELLATED_PROXY",
                "faceted_or_tessellated": True,
            }
        )
        faceted_result, faceted_exit = validate_document(
            faceted_laundering, root / "faceted-brep-laundering.json", check_files=True
        )
        check(
            faceted_exit == 1,
            "faceted geometry cannot be laundered as imported B-Rep",
            errors,
        )

        manufacturing_supplier_downgrade = deepcopy(authoring_fixture)
        manufacturing_supplier_downgrade["release_intent"] = "MANUFACTURING_HANDOFF"
        manufacturing_supplier_downgrade["geometry_definitions"][1].update(
            {
                "geometry_class": "IMPORTED_BREP",
                "provenance_kind": "CONTROLLED_BREP_IMPORT",
            }
        )
        manufacturing_result, manufacturing_exit = validate_document(
            manufacturing_supplier_downgrade,
            root / "manufacturing-supplier-downgrade.json",
            check_files=True,
        )
        check(
            manufacturing_exit == 1,
            "manufacturing handoff cannot downgrade purchased hardware geometry",
            errors,
        )
        check(
            any(
                "requires supplier-exact geometry" in item
                for item in manufacturing_result["holds"]
            ),
            "manufacturing supplier downgrade must retain an explicit hold",
            errors,
        )

        swapped_identity_pairs = deepcopy(authoring_fixture)
        first_row, second_row = swapped_identity_pairs["occurrence_reconciliation"][
            "rows"
        ]
        first_row["source_occurrence_id"], second_row["source_occurrence_id"] = (
            second_row["source_occurrence_id"],
            first_row["source_occurrence_id"],
        )
        swapped_result, swapped_exit = validate_document(
            swapped_identity_pairs,
            root / "swapped-identity-pairs.json",
            check_files=True,
        )
        check(
            swapped_exit == 1,
            "equal-count but swapped occurrence identities must fail",
            errors,
        )
        check(
            any(
                "exact source/Fusion pair" in item for item in swapped_result["errors"]
            ),
            "swapped identities must fail pair-level reconciliation",
            errors,
        )

        swapped_fusion_definition = deepcopy(authoring_fixture)
        structural_requirement = next(
            row
            for row in swapped_fusion_definition["completeness"]["requirements"]
            if row["category"] == "STRUCTURAL"
        )
        structural_requirement["occurrences"][0]["geometry_definition_id"] = "def-nut"
        swapped_fusion_definition_result, swapped_fusion_definition_exit = (
            validate_document(
                swapped_fusion_definition,
                root / "swapped-fusion-definition.json",
                check_files=True,
            )
        )
        check(
            swapped_fusion_definition_exit == 1,
            "equal-count Fusion occurrence/definition swaps must fail",
            errors,
        )
        check(
            any(
                "Fusion occurrence/definition pair" in item
                for item in swapped_fusion_definition_result["errors"]
            ),
            "Fusion definition swaps must retain a pair-level reconciliation error",
            errors,
        )

        swapped_source_definitions = deepcopy(authoring_fixture)
        first_row, second_row = swapped_source_definitions["occurrence_reconciliation"][
            "rows"
        ]
        first_row["source_definition_id"], second_row["source_definition_id"] = (
            second_row["source_definition_id"],
            first_row["source_definition_id"],
        )
        swapped_source_definition_result, swapped_source_definition_exit = (
            validate_document(
                swapped_source_definitions,
                root / "swapped-source-definitions.json",
                check_files=True,
            )
        )
        check(
            swapped_source_definition_exit == 1,
            "equal-count source occurrence/definition swaps must fail",
            errors,
        )
        check(
            any(
                "reconciled source definition" in item
                for item in swapped_source_definition_result["errors"]
            ),
            "source definition swaps must retain a cross-ledger binding error",
            errors,
        )
        check(
            any(
                "faceted/mesh-derived" in item
                for item in faceted_result.get("errors", [])
            ),
            "faceted B-Rep laundering must retain a provenance error",
            errors,
        )

        mismatch_10_to_9 = bind_authoring_fixture_files(
            complete_authoring_fixture(10), assembly, controlled_evidence
        )
        mismatch_10_to_9["native_readback"]["occurrence_count"] = 9
        mismatch_10_to_9["native_readback"]["second_readback"]["occurrence_count"] = 9
        mismatch_10_to_9_result, mismatch_10_to_9_exit = validate_document(
            mismatch_10_to_9, root / "mismatch-10-to-9.json", check_files=True
        )
        check(
            mismatch_10_to_9_exit == 1,
            "10 source rows to 9 Fusion count must fail",
            errors,
        )
        check(
            any(
                "Fusion ledger 10 != Fusion count 9" in item
                for item in mismatch_10_to_9_result["errors"]
            ),
            "10-to-9 mismatch must be derived from occurrence rows",
            errors,
        )

        mismatch_10_to_0 = bind_authoring_fixture_files(
            complete_authoring_fixture(10), assembly, controlled_evidence
        )
        mismatch_10_to_0["native_readback"]["occurrence_count"] = 0
        mismatch_10_to_0["native_readback"]["second_readback"]["occurrence_count"] = 0
        mismatch_10_to_0_result, mismatch_10_to_0_exit = validate_document(
            mismatch_10_to_0, root / "mismatch-10-to-0.json", check_files=True
        )
        check(
            mismatch_10_to_0_exit == 1,
            "10 source rows to zero Fusion count must fail",
            errors,
        )
        check(
            any(
                "positive count required" in item
                for item in mismatch_10_to_0_result["holds"]
            ),
            "zero Fusion occurrence count must retain a positive-count hold",
            errors,
        )

        authorized_10_to_9 = bind_authoring_fixture_files(
            complete_authoring_fixture(10), assembly, controlled_evidence
        )
        authorized_10_to_9["native_readback"]["occurrence_count"] = 9
        authorized_10_to_9["native_readback"]["second_readback"]["occurrence_count"] = 9
        authorized_10_to_9["completeness"]["expected_physical_occurrence_count"] = 9
        authorized_requirements = authorized_10_to_9["completeness"]["requirements"]
        authorized_nut_requirement = next(
            row for row in authorized_requirements if row["category"] == "NUT"
        )
        authorized_nut_requirement.update(
            {
                "quantity_semantics": "OPTIONAL",
                "quantity": {
                    "lower_bound": 0,
                    "upper_bound": 1,
                    "selected_installed_count": 0,
                    "procurement_pack_size": None,
                },
                "status": "NOT_APPLICABLE",
                "rationale": "Controlled target configuration excludes this source nut occurrence.",
                "occurrences": [],
            }
        )
        authorized_nut_row = next(
            row
            for row in authorized_10_to_9["completeness"]["categories"]
            if row["category"] == "NUT"
        )
        authorized_nut_row.update(
            {
                "expected_count": 0,
                "present_exact_count": 0,
                "status": "NOT_APPLICABLE",
                "evidence_ids": [],
                "not_applicable_rationale": "Controlled target configuration excludes the nut.",
                "not_applicable_evidence_ids": ["evidence:fixture/control"],
            }
        )
        authorized_10_to_9["placement_contracts"] = [
            row
            for row in authorized_10_to_9["placement_contracts"]
            if row["fusion_occurrence_id"] != "fus-occ-nut-001"
        ]
        authorized_difference = authorized_10_to_9["occurrence_reconciliation"]["rows"][
            -1
        ]
        authorized_difference.update(
            {
                "fusion_occurrence_id": None,
                "fusion_definition_id": None,
                "disposition": "SOURCE_ONLY_AUTHORIZED_EXCLUSION",
                "rationale": "Evidence-backed target configuration exclusion.",
            }
        )
        authorized_result, authorized_exit = validate_document(
            authorized_10_to_9, root / "authorized-10-to-9.json", check_files=True
        )
        check(
            authorized_exit == 0,
            "evidence-backed 10-to-9 disposition must PASS",
            errors,
        )
        check(
            authorized_result.get("status") == "PASS",
            "authorized mismatch must PASS",
            errors,
        )

        fake_210_closure = deepcopy(authoring_fixture)
        fake_210_closure["source_closure"].update(
            {
                "expected_external_file_count": 210,
                "resolved_external_file_count": 210,
                "unresolved_reference_count": 0,
            }
        )
        fake_210_result, fake_210_exit = validate_document(
            fake_210_closure, root / "fake-210-closure.json", check_files=True
        )
        check(
            fake_210_exit == 1,
            "self-reported 210-file closure without ledger must fail",
            errors,
        )
        check(
            any(
                "declared 210 != ledger 0" in item for item in fake_210_result["errors"]
            ),
            "fake 210-file closure must fail ledger-derived totals",
            errors,
        )

        denied_public = deepcopy(authoring_fixture)
        denied_public["release_intent"] = "PUBLIC_REDISTRIBUTION"
        denied_public["sources"][0]["rights_status"] = "DENIED"
        denied_public_result, denied_public_exit = validate_document(
            denied_public, root / "denied-public-release.json", check_files=True
        )
        check(
            denied_public_exit == 1,
            "denied rights must block public redistribution PASS",
            errors,
        )
        check(
            any(
                "does not authorize public redistribution" in item
                for item in denied_public_result["holds"]
            ),
            "rights denial must retain the release-intent hold",
            errors,
        )

        unreconciled_tree = deepcopy(authoring_fixture)
        unreconciled_tree["occurrence_reconciliation"]["status"] = "NOT_RUN"
        unreconciled_tree["occurrence_reconciliation"]["evidence_ids"] = []
        unreconciled_result, unreconciled_exit = validate_document(
            unreconciled_tree,
            root / "unreconciled-tree-overclaim.json",
            check_files=True,
        )
        check(
            unreconciled_exit == 1,
            "unreconciled source/Fusion counts must fail declared PASS",
            errors,
        )
        check(
            any(
                "reconciliation" in item
                for item in unreconciled_result.get("holds", [])
            ),
            "unreconciled source/Fusion counts must retain a reconciliation hold",
            errors,
        )
        observations["authoring_manifest"] = {
            "complete": authoring_result.get("status"),
            "checked_source_and_evidence_hashes": checked_result.get("status"),
            "tampered_evidence_hash": tampered_result.get("status"),
            "unchecked_engineering_claims": unchecked_result.get("status"),
            "quantity_semantics": quantity_semantics_results,
            "process_and_motion": process_result.get("status"),
            "zero_certified_motion": zero_motion_result.get("status"),
            "unbound_media_hash": unbound_media_result.get("status"),
            "mesh_overclaim": mesh_result.get("status"),
            "unregistered_evidence": unregistered_result.get("status"),
            "malformed_enum_no_traceback": malformed_result.get("status"),
            "singular_transform": singular_result.get("status"),
            "hostless_placements": hostless_result.get("status"),
            "sibling_top_level_placements": sibling_result.get("status"),
            "empty_requirements_no_traceback": empty_requirements_result.get("status"),
            "unbound_source_definition": unbound_definition_result.get("status"),
            "concealed_nut": concealed_nut_result.get("status"),
            "disguised_triangulated_brep": disguised_result.get("status"),
            "faceted_brep_laundering": faceted_result.get("status"),
            "manufacturing_supplier_downgrade": manufacturing_result.get("status"),
            "swapped_identity_pairs": swapped_result.get("status"),
            "swapped_fusion_definition": swapped_fusion_definition_result.get("status"),
            "swapped_source_definitions": swapped_source_definition_result.get(
                "status"
            ),
            "mismatch_10_to_9": mismatch_10_to_9_result.get("status"),
            "mismatch_10_to_0": mismatch_10_to_0_result.get("status"),
            "authorized_10_to_9": authorized_result.get("status"),
            "fake_210_closure": fake_210_result.get("status"),
            "denied_public_release": denied_public_result.get("status"),
            "unreconciled_tree_overclaim": unreconciled_result.get("status"),
        }

    script_root = Path(__file__).resolve().parent
    algorithm_physics_run = subprocess.run(
        [sys.executable, str(script_root / "algorithm_physics_contract_self_test.py")],
        capture_output=True,
        text=True,
        check=False,
    )
    check(
        algorithm_physics_run.returncode == 0,
        "algorithm/physics contract regressions must pass: "
        + (algorithm_physics_run.stderr or algorithm_physics_run.stdout),
        errors,
    )
    try:
        algorithm_physics_result = json.loads(algorithm_physics_run.stdout)
    except json.JSONDecodeError:
        algorithm_physics_result = {
            "status": "INVALID_OUTPUT",
            "stdout": algorithm_physics_run.stdout,
        }
    observations["algorithm_physics_contract"] = algorithm_physics_result

    result = {
        "status": "PASS" if not errors else "FAIL",
        "errors": errors,
        "observations": observations,
        "claim_boundary": "Synthetic lexical, evidence-lane and authoring-manifest regressions only; no B-Rep, Fusion document or target assembly was validated.",
    }
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if not errors else 1


if __name__ == "__main__":
    raise SystemExit(main())
