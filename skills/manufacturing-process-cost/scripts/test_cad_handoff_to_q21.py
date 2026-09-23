#!/usr/bin/env python3
"""Check that CAD evidence maps to q21 without inventing process coverage."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cad_handoff_to_q21 import HandoffError, P, project_q21
from rdf_lines import RdfTerms


class Q21ProjectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.packet = {
            "handoff_id": "handoff_1",
            "project": {"project_id": "demo", "configuration_id": "rev_a", "model_revision": "A", "product_state": "nominal"},
            "sources": [{"id": "drawing", "sha256": "a" * 64, "role": "drawing"}],
            "functions": [{"id": "seal", "status": "required", "basis": "drawing + topology", "source_ids": ["drawing"]}],
            "routes": [{"id": "brazing"}, {"id": "laser"}],
            "coverage_assertions": [],
            "claims": [
                {"id": "deliver", "maturity": "batch_delivery", "baseline_route_id": "brazing", "target_route_id": "laser", "depends_on_route_ids": ["laser"], "depends_on_function_ids": ["seal"]}
            ],
        }

    def test_missing_completeness_and_coverage_stay_absent(self) -> None:
        rdf, focus = project_q21(self.packet, "deliver", "b" * 64)
        lines = rdf.decode("utf-8").splitlines()
        self.assertEqual(sum(f" {RdfTerms.type.n3()} {P.ProcessReplacementClaim.n3()} ." in line for line in lines), 1)
        self.assertEqual(sum(f" {P.requiredFunction.n3()} " in line for line in lines), 1)
        self.assertEqual(sum(f" {P.completenessEvidence.n3()} " in line for line in lines), 0)
        self.assertEqual(sum(f" {RdfTerms.type.n3()} {P.FunctionCoverage.n3()} ." in line for line in lines), 0)
        self.assertIn("demo.rev_a.A.claim.deliver", focus)

    def test_candidate_coverage_is_not_verified(self) -> None:
        self.packet["coverage_assertions"] = [
            {"id": "cov", "claim_id": "deliver", "function_id": "seal", "operation_id": "weld", "status": "proposed", "validation_source_ids": [], "scope_checked_source_ids": []}
        ]
        rdf, _ = project_q21(self.packet, "deliver", "b" * 64)
        lines = rdf.decode("utf-8").splitlines()
        coverage = next(line.split(" ", 1)[0] for line in lines if f" {RdfTerms.type.n3()} {P.FunctionCoverage.n3()} ." in line)
        self.assertIn(f'{coverage} {P.status.n3()} "proposed" .', lines)
        self.assertEqual(sum(f" {P.validatedBy.n3()} " in line for line in lines), 0)

    def test_requires_explicit_route_pair(self) -> None:
        del self.packet["claims"][0]["baseline_route_id"]
        with self.assertRaisesRegex(HandoffError, "explicit baseline"):
            project_q21(self.packet, "deliver", "b" * 64)


if __name__ == "__main__":
    unittest.main()
