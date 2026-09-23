#!/usr/bin/env python3
"""Adversarial checks for the CAD/process evidence boundary."""

from __future__ import annotations

import hashlib
import json
from copy import deepcopy
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cad_process_handoff import HandoffError, compare_packets, project_abox, verify
from rdf_lines import Namespace, RdfTerms


CP = Namespace("urn:ontology-engineering:cad-process:v1:")


class HandoffTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.model = self.root / "base.step"
        self.model.write_text("ISO-10303-21;\nEND-ISO-10303-21;\n", encoding="utf-8")
        model_sha = hashlib.sha256(self.model.read_bytes()).hexdigest()
        self.readback = self.root / "readback.json"
        self.readback.write_text(json.dumps({"source_hashes": {"base": model_sha}, "holes": [{"x_mm": 1.25}]}), encoding="utf-8")
        readback_sha = hashlib.sha256(self.readback.read_bytes()).hexdigest()
        self.packet = {
            "record_type": "ontology-engineering.cad-process-handoff/v1",
            "handoff_id": "demo_1",
            "project": {"project_id": "demo", "configuration_id": "rev_a", "model_revision": "A", "product_state": "nominal"},
            "sources": [
                {"id": "model", "path": "base.step", "sha256": model_sha, "media_type": "model/step", "role": "native_model"},
                {"id": "readback", "path": "readback.json", "sha256": readback_sha, "media_type": "application/json", "role": "native_readback", "embedded_source_hashes": {"/source_hashes/base": "model"}},
            ],
            "features": [
                {"id": "hole_1", "part_id": "base", "kind": "through_hole", "statement": "nominal hole", "geometry_state": "nominal", "basis": "native_geometry_readback", "source_id": "readback", "source_locator": "/holes/0", "attributes": {"x_mm": 1.25}}
            ],
            "functions": [
                {"id": "seal_1", "name": "seal", "status": "required", "basis": "drawing plus geometry", "source_ids": ["readback"], "feature_ids": ["hole_1"]}
            ],
            "routes": [
                {"id": "laser", "name": "laser candidate", "status": "candidate", "source_ids": ["readback"], "operations": []}
            ],
            "coverage_assertions": [],
            "claims": [
                {"id": "deliver", "statement": "can deliver?", "maturity": "batch_delivery", "source_ids": ["readback"], "depends_on_function_ids": ["seal_1"], "depends_on_route_ids": ["laser"]}
            ],
            "questions": [
                {"id": "q1", "direction": "cad_to_process", "text": "what seals hole?", "status": "open", "target_ids": ["hole_1", "seal_1"], "source_ids": ["readback"], "requested_evidence": ["joint map"]},
                {"id": "q2", "direction": "process_to_cad", "text": "can tool reach?", "status": "open", "target_ids": ["hole_1", "laser"], "source_ids": ["readback"], "requested_evidence": ["access check"]},
            ],
            "assumptions": [],
            "downstream_outputs": [
                {"id": "quote", "kind": "cost_estimate", "statement": "conditional estimate", "state": "conditional", "source_ids": ["readback"], "depends_on_claim_ids": ["deliver"]}
            ],
        }
        self.packet_path = self.root / "packet.json"
        self.write_packet()

    def write_packet(self) -> None:
        self.packet_path.write_text(json.dumps(self.packet), encoding="utf-8")

    def test_verified_projection_keeps_bidirectional_questions_and_no_verdict(self) -> None:
        packet, audit = verify(self.packet_path, self.root)
        lines = project_abox(packet).decode("utf-8").splitlines()
        self.assertEqual(audit["semantic_execution"], "not_run")
        self.assertEqual(audit["engineering_verdict"], "not_assessed")
        self.assertEqual(sum(f" {RdfTerms.type.n3()} {CP.EngineeringQuestion.n3()} ." in line for line in lines), 2)
        self.assertEqual(
            {json.loads(line.split(f" {CP.direction.n3()} ", 1)[1].removesuffix(" ."))
             for line in lines if f" {CP.direction.n3()} " in line},
            {"cad_to_process", "process_to_cad"},
        )

    def test_source_tampering_fails_before_projection(self) -> None:
        self.model.write_text("changed model", encoding="utf-8")
        with self.assertRaisesRegex(HandoffError, "SHA-256 mismatch"):
            verify(self.packet_path, self.root)

    def test_readback_attribute_drift_fails(self) -> None:
        self.packet["features"][0]["attributes"]["x_mm"] = 9.99
        self.write_packet()
        with self.assertRaisesRegex(HandoffError, "attribute mismatch"):
            verify(self.packet_path, self.root)

    def test_wrong_identity_and_path_escape_fail(self) -> None:
        self.packet["functions"][0]["feature_ids"] = ["missing_hole"]
        self.write_packet()
        with self.assertRaisesRegex(HandoffError, "absent/wrong-kind"):
            verify(self.packet_path, self.root)
        self.packet["functions"][0]["feature_ids"] = ["hole_1"]
        self.packet["sources"][0]["path"] = "../base.step"
        self.write_packet()
        with self.assertRaisesRegex(HandoffError, "normalized and relative"):
            verify(self.packet_path, self.root)

    def test_geometry_change_propagates_to_process_claim_and_both_questions(self) -> None:
        previous = deepcopy(self.packet)
        current = deepcopy(self.packet)
        current["features"][0]["attributes"]["x_mm"] = 1.35
        impact = compare_packets(previous, current)
        self.assertIn("hole_1", impact["changed_ids"])
        self.assertEqual(impact["affected_claim_ids"], ["deliver"])
        self.assertEqual(impact["affected_question_ids"], ["q1", "q2"])
        self.assertEqual(impact["affected_output_ids"], ["quote"])
        self.assertEqual(impact["engineering_verdict"], "re_review_required_not_assessed")

    def test_verified_coverage_requires_scoped_physical_test(self) -> None:
        self.packet["routes"][0]["operations"] = [
            {
                "id": "join_hole_1", "process_type": "laser_welding",
                "target_feature_ids": ["hole_1"], "intended_function_ids": ["seal_1"],
                "mechanism_statement": "candidate continuous boundary",
                "source_ids": ["readback"], "validation_source_ids": [],
            }
        ]
        self.packet["coverage_assertions"] = [
            {
                "id": "coverage_1", "claim_id": "deliver", "function_id": "seal_1",
                "operation_id": "join_hole_1", "status": "verified",
                "mechanism_statement": "claim of sealing", "source_ids": ["readback"],
                "validation_source_ids": ["readback"], "scope_checked_source_ids": ["readback"],
            }
        ]
        self.write_packet()
        with self.assertRaisesRegex(HandoffError, "scoped physical test evidence"):
            verify(self.packet_path, self.root)

        self.packet["coverage_assertions"][0]["status"] = "proposed"
        self.write_packet()
        packet, _ = verify(self.packet_path, self.root)
        lines = project_abox(packet).decode("utf-8").splitlines()
        self.assertEqual(sum(f" {RdfTerms.type.n3()} {CP.FunctionCoverageAssertion.n3()} ." in line for line in lines), 1)


if __name__ == "__main__":
    unittest.main()
