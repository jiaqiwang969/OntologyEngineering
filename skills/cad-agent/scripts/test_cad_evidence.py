#!/usr/bin/env python3
"""Boundary tests for general CAD evidence and its manufacturing handoff."""

from __future__ import annotations

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cad_evidence import CadEvidenceError, compare_packets, project_abox, verify
from cad_process_handoff import HandoffError, compare_packets as compare_process, project_abox as process_abox, verify as verify_process


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class CadEvidenceTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.model = self.root / "assembly.step"
        self.model.write_text("ISO-10303-21;\nEND-ISO-10303-21;\n", encoding="utf-8")
        self.readback = self.root / "readback.json"
        self.readback.write_text(json.dumps({"model_sha": digest(self.model), "occurrences": [{"name": "plate-a"}],
                                             "joints": [{"name": "joint-a", "offset_mm": 0.2}],
                                             "relations": [{"kind": "mates_with"}]}), encoding="utf-8")
        self.drawing = self.root / "drawing.txt"
        self.drawing.write_text("Synthetic example: dimension D1 is nominal 0.2 mm", encoding="utf-8")
        self.project = {"project_id": "demo", "configuration_id": "assembly_a", "model_revision": "A", "product_state": "nominal"}
        self.packet = {
            "record_type": "ontology-engineering.cad-evidence/v1", "packet_id": "cad_demo", "project": self.project,
            "sources": [
                {"id": "model", "path": "assembly.step", "sha256": digest(self.model), "media_type": "model/step", "role": "native_model"},
                {"id": "readback", "path": "readback.json", "sha256": digest(self.readback), "media_type": "application/json",
                 "role": "native_readback", "embedded_source_hashes": {"/model_sha": "model"}},
                {"id": "drawing", "path": "drawing.txt", "sha256": digest(self.drawing), "media_type": "text/plain", "role": "drawing"},
            ],
            "objects": [
                {"id": "plate_occ", "kind": "part_occurrence", "statement": "plate occurrence", "identity_scope": "model_revision",
                 "basis": "native_readback", "source_id": "readback", "source_locator": "/occurrences/0", "expected": {"name": "plate-a"}},
                {"id": "joint", "kind": "joint", "statement": "joint candidate", "identity_scope": "model_revision",
                 "basis": "native_readback", "source_id": "readback", "source_locator": "/joints/0", "expected": {"offset_mm": 0.2}},
                {"id": "dim_d1", "kind": "drawing_dimension", "statement": "D1 nominal 0.2 mm", "identity_scope": "model_revision",
                 "basis": "drawing_review", "source_id": "drawing", "source_locator": "D1"},
            ],
            "relations": [
                {"id": "mate", "subject_id": "plate_occ", "predicate": "mates_with", "object_id": "joint",
                 "statement": "model mate", "status": "observed", "basis": "native_readback", "source_id": "readback",
                 "source_locator": "/relations/0", "expected": {"kind": "mates_with"}},
            ],
            "requirements": [
                {"id": "alignment", "statement": "alignment required", "status": "required",
                 "target_ids": ["joint"], "source_ids": ["drawing"]},
            ],
            "assertions": [
                {"id": "fit_candidate", "statement": "nominal model has a mate", "scope": "nominal CAD only",
                 "maturity": "bounded_observation", "target_ids": ["joint"], "depends_on_ids": ["mate", "alignment"],
                 "source_ids": ["readback"], "challenge_source_ids": []},
            ],
            "questions": [
                {"id": "q_tolerance", "text": "Will tolerance stack preserve fit?", "status": "open",
                 "target_ids": ["fit_candidate", "dim_d1"], "source_ids": ["drawing"],
                 "requested_evidence": ["tolerance analysis"]},
            ],
        }
        self.path = self.root / "cad.json"
        self.write_packet()

    def write_packet(self) -> None:
        self.path.write_text(json.dumps(self.packet), encoding="utf-8")

    def test_general_objects_project_without_physical_verdict(self) -> None:
        packet, audit = verify(self.path, self.root)
        triples = project_abox(packet, audit["packet_sha256"]).decode("utf-8")
        self.assertEqual(audit["source_integrity"], "verified")
        self.assertEqual(audit["semantic_execution"], "not_run")
        self.assertEqual(audit["engineering_verdict"], "not_assessed")
        self.assertIn("cad_object:joint", triples)
        self.assertIn("cad_relation:mate", triples)
        self.assertIn("cad_requirement:alignment", triples)

    def test_source_tamper_and_native_drift_block(self) -> None:
        self.model.write_text("changed", encoding="utf-8")
        with self.assertRaisesRegex(CadEvidenceError, "SHA-256 mismatch"):
            verify(self.path, self.root)
        self.model.write_text("ISO-10303-21;\nEND-ISO-10303-21;\n", encoding="utf-8")
        self.packet["objects"][1]["expected"]["offset_mm"] = 0.4
        self.write_packet()
        with self.assertRaisesRegex(CadEvidenceError, "value mismatch"):
            verify(self.path, self.root)

    def test_invalid_role_and_circular_support_block(self) -> None:
        self.packet["objects"][1]["source_id"] = "drawing"
        self.write_packet()
        with self.assertRaisesRegex(CadEvidenceError, "claims native_readback"):
            verify(self.path, self.root)
        self.packet["objects"][1]["source_id"] = "readback"
        self.packet["assertions"][0]["depends_on_ids"].append("fit_candidate")
        self.write_packet()
        with self.assertRaisesRegex(CadEvidenceError, "circular assertion"):
            verify(self.path, self.root)

    def test_revision_propagates_through_relation_and_claim(self) -> None:
        previous = deepcopy(self.packet)
        self.packet["objects"][1]["statement"] = "revised joint offset"
        impact = compare_packets(previous, self.packet)
        self.assertIn("joint", impact["changed_ids"])
        self.assertIn("fit_candidate", impact["affected_assertion_ids"])
        self.assertIn("q_tolerance", impact["affected_question_ids"])

    def test_process_link_requires_exact_project_and_real_cad_object(self) -> None:
        process = {
            "record_type": "ontology-engineering.cad-process-handoff/v1", "handoff_id": "handoff", "project": self.project,
            "sources": [
                {"id": "cad_packet", "path": "cad.json", "sha256": digest(self.path),
                 "media_type": "application/json", "role": "other"},
                {"id": "native", "path": "readback.json", "sha256": digest(self.readback),
                 "media_type": "application/json", "role": "native_readback"},
            ],
            "features": [{"id": "joint_feature", "part_id": "plate", "kind": "joint_interface",
                          "statement": "joint on plate", "geometry_state": "nominal", "basis": "native_geometry_readback",
                          "source_id": "native", "source_locator": "/joints/0", "attributes": {"offset_mm": 0.2}}],
            "functions": [], "routes": [], "coverage_assertions": [], "claims": [],
            "questions": [], "assumptions": [], "downstream_outputs": [],
            "cad_evidence": {"source_id": "cad_packet", "links": [
                {"handoff_entity_id": "joint_feature", "cad_object_ids": ["joint"]}
            ]},
        }
        path = self.root / "process.json"
        path.write_text(json.dumps(process), encoding="utf-8")
        verified, audit = verify_process(path, self.root)
        self.assertEqual(audit["cad_evidence_packet_sha256"], digest(self.path))
        self.assertIn("cad_object:joint", process_abox(verified).decode("utf-8"))
        modified = deepcopy(process)
        modified["sources"][0]["sha256"] = "0" * 64
        impact = compare_process(process, modified)
        self.assertIn("joint_feature", impact["affected_ids"])
        process["cad_evidence"]["links"][0]["cad_object_ids"] = ["missing"]
        path.write_text(json.dumps(process), encoding="utf-8")
        with self.assertRaisesRegex(HandoffError, "missing object"):
            verify_process(path, self.root)
        process["cad_evidence"]["links"][0]["cad_object_ids"] = ["joint"]
        process["project"]["model_revision"] = "B"
        path.write_text(json.dumps(process), encoding="utf-8")
        with self.assertRaisesRegex(HandoffError, "identity/state differs"):
            verify_process(path, self.root)


if __name__ == "__main__":
    unittest.main()
