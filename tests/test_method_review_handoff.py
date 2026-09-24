"""Source transport between shared judgments and existing engineering methods."""
import json

import pytest

from ontology_engineering import judgment_review as review
from ontology_engineering import method_evidence
from scripts.semantic_bundle_transport import ROOT

EXAMPLES = ROOT / "examples/judgment_intake/method-review"


def test_new_method_records_preserve_roles_and_source_pointers():
    manifest = json.loads((EXAMPLES / "cases.json").read_bytes())
    for case in manifest["cases"]:
        rdf, audit = method_evidence.project_record(EXAMPLES / case["record"], EXAMPLES)
        assert audit["source_integrity"] == "verified"
        assert audit["semantic_execution"] == "not_run"
        assert audit["engineering_verdict"] == "not_assessed"
        assert audit["field_lineage"]
        assert b"model_candidate" not in rdf
    source = json.loads((EXAMPLES / "sources.json").read_bytes())
    assert len(source["forwarding_lineage"]["reports"]) == 24
    assert {r["root_event"] for r in source["forwarding_lineage"]["reports"]} == {"synthetic-real-check-a"}


def test_existing_method_cannot_be_executed_under_judgment_package_identity(tmp_path):
    binding = {
        "project": {"project_id": "synthetic-project"},
        "semantic_target": {"kind": "workspace", "package_id": "semantica.engineering.judgment-intake"},
        "baseline": {"version": "0.2.1", "digest": "0" * 64},
    }
    path = tmp_path / "wrong-binding.json"; path.write_text(json.dumps(binding))
    with pytest.raises(ValueError, match="review_bundle_binding_mismatch"):
        review.execute_record(EXAMPLES / "quote-kept-as-estimate.json", EXAMPLES, path,
            tmp_path / "unused-registry", "test-operator", tmp_path / "review", project_id="synthetic-project",
            bundle_name="engineering-evidence-methods")


def test_new_bundle_selection_does_not_widen_project_binding(tmp_path):
    binding = {"project": {"project_id": "different-project"}}
    path = tmp_path / "wrong-project.json"; path.write_text(json.dumps(binding))
    with pytest.raises(ValueError, match="review_project_mismatch"):
        review.execute_record(EXAMPLES / "forwarded-same-event.json", EXAMPLES, path,
            tmp_path / "unused-registry", "test-operator", tmp_path / "review", project_id="synthetic-project",
            bundle_name="engineering-evidence-methods")
