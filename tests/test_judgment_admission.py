"""Real native reviews of controlled synthetic assertion adoption boundaries."""
from copy import deepcopy
import json
from pathlib import Path

import pytest

from ontology_engineering import judgment_admission as admission
from ontology_engineering import judgment_evolution as evolution
from ontology_engineering import method_bootstrap as bootstrap
from ontology_engineering.judgment_contracts import digest, encoded
from ontology_engineering.judgment_review import _generated_record


def save(path, value):
    path.write_bytes(encoded(value) + b"\n")
    return path


@pytest.fixture(scope="module")
def native_context(tmp_path_factory):
    directory = tmp_path_factory.mktemp("native-assertion-admission") / "recipient"
    bootstrap.prepare(directory, project="synthetic-admission-project", domain="engineering-evidence",
        actor="synthetic-controller", fact_authority="synthetic-factual-reviewer", evidence_root="evidence:synthetic-admission")
    plan = bootstrap.read(directory / "plan.json")
    auth = {"schema": "ontology-engineering.method-bootstrap-authorization/v1", "action": "initialize-frozen-methods",
        "actor_id": plan["actor"], "plan_sha256": bootstrap.digest(bootstrap.encoded(plan)),
        "reason": "Isolated native synthetic test; no human signature or production approval.", "issued_at": bootstrap.now()}
    bootstrap.apply(directory, save(directory / "initialize.json", auth))
    auth.update(action="adopt-project-binding", binding_sha256=digest((directory / "proposed-project-binding.json").read_bytes()))
    bootstrap.adopt(directory, save(directory / "adopt-binding.json", auth))
    binding = bootstrap.read(directory / "project-binding.json")
    # Explicitly scoped synthetic test policy, separate from the method-library
    # authorization; the library's commit/promote authority cannot adopt facts.
    binding["binding_id"] += "-synthetic-assertion-policy"
    binding["authority"]["decision"]["scope"] = [admission.ADOPTION_SCOPE, evolution.REVIEW_SCOPE]
    path = save(directory / "assertion-policy-binding.json", binding)
    return path, directory / "registry"


def fixture(root, context, *, engineering=False, decision_changes=None, missing_record=False, scope_mismatch=False):
    binding, workspace = context
    text = "合成记录：样件尚未进行试验。"
    source_path = save(root / "source.json", {"note": text})
    source = {"id": "synthetic-note", "path": source_path.name, "sha256": digest(source_path.read_bytes()),
        "media_type": "application/json", "selection": {"pointer": "/note"}, "lineage_group": "synthetic-event-1",
        "context_status": "complete_for_question"}
    assertion = {"id": "statement-a", "statement": text, "subject_revision": "synthetic-r1", "scope": "synthetic-record-review",
        "kind": "engineering_claim" if engineering else "reported_statement"}
    request = {"schema": admission.REQUEST_SCHEMA, "admission_id": "admission-a", "project_id": "synthetic-admission-project",
        "assertion": assertion, "source": source, "review_decision": None,
        "required_methods": ["pattern-scope"] if engineering else [], "records": []}
    if engineering and not missing_record:
        claim = {k: assertion[k] for k in ("id", "statement", "subject_revision", "scope")}
        record = _generated_record(root, "scope", claim, "pattern-scope",
            {"claim_scope": assertion["scope"], "evidence_scope": "different-scope" if scope_mismatch else assertion["scope"],
             "required_conditions": ["synthetic-only"], "established_conditions": ["synthetic-only"],
             "inventory_complete": True, "representation": "applicability_reviewed"},
            {"scope": "Synthetic declared applicability; no actual specimen or completed experiment."})
        request["records"] = [{"path": record.name, "sha256": digest(record.read_bytes())}]
    decision = {"schema": admission.DECISION_SCHEMA, "review_id": "synthetic-review-a",
        "subject_sha256": digest(admission.subject_identity(request, digest(binding.read_bytes()))),
        "assertion_sha256": digest(assertion), "source_sha256": digest(source), "scope": assertion["scope"],
        "subject_revision": assertion["subject_revision"], "authority_id": "synthetic-factual-reviewer",
        "authority_scope": ["controlled-project-records"], "origin": "explicit_control_plane_decision", "decision": "accept",
        "source_relation": "supports", "applicability_review": "controlled_engineering_review" if engineering else "source_event_only",
        "reason": "Synthetic fixture review of exactly the declared record, not independent reference labels or physical approval.",
        "issued_at": bootstrap.now()}
    decision.update(decision_changes or {})
    decision_path = save(root / "factual-decision.json", decision)
    request["review_decision"] = {"path": decision_path.name, "sha256": digest(decision_path.read_bytes())}
    path = save(root / "request.json", request)
    auth = {"schema": admission.AUTHORIZATION_SCHEMA, "authorization_id": "synthetic-authorization-a",
        "action": admission.ADOPTION_SCOPE, "actor_id": "synthetic-controller", "authority_id": "synthetic-controller",
        "binding_sha256": digest(binding.read_bytes()), "request_sha256": digest(path.read_bytes()),
        "review_decision_sha256": digest(decision_path.read_bytes()), "reason": "Explicit synthetic local test action only.",
        "issued_at": bootstrap.now()}
    return path, save(root / "adoption-authorization.json", auth)


def run(root, context, request, authorization=None):
    return admission.review_admission(request, root, *context, "synthetic-controller", root / "result", authorization=authorization)


def test_clear_native_review_does_not_implicitly_adopt(tmp_path, native_context):
    request, _ = fixture(tmp_path, native_context)
    result = run(tmp_path, native_context, request)
    assert result["status"] == "eligible_for_separate_adoption" and not result["fact_admitted"]
    assert result["admission_review"]["execution_origin"] == "fresh_native_review"
    assert not (tmp_path / "result/adopted-assertion.json").exists()


@pytest.mark.parametrize("engineering", [False, True])
def test_separate_authorization_adopts_exact_scoped_assertion(tmp_path, native_context, engineering):
    request, auth = fixture(tmp_path, native_context, engineering=engineering)
    result = run(tmp_path, native_context, request, auth)
    assert result["fact_admitted"] and result["product_release"] == "not_performed"
    document = json.loads((tmp_path / "result/adopted-assertion.json").read_text())
    assert document["assertion"]["statement"] == document["source_selection_text"]
    assert digest((tmp_path / "result/adopted-abox.ttl").read_bytes()) == result["adopted_abox_sha256"]
    if engineering:
        assert result["coverage_review"]["status"] == "clear"
        assert result["obligation_reviews"][0]["execution_origin"] == "fresh_native_review"
    with pytest.raises(FileExistsError):
        run(tmp_path, native_context, request, auth)


def test_source_hash_does_not_validate_an_invented_quote(tmp_path, native_context):
    request, auth = fixture(tmp_path, native_context)
    value = json.loads(request.read_text()); value["assertion"]["statement"] = "合成记录：样件试验通过。"
    save(request, value)
    with pytest.raises(ValueError, match="not_the_exact_source"):
        run(tmp_path, native_context, request, auth)
    assert not (tmp_path / "result/adopted-assertion.json").exists()


@pytest.mark.parametrize("change,reason", [
    ({"assertion_sha256": "a" * 64}, "reviewed_assertion_differs"),
    ({"source_sha256": "a" * 64}, "reviewed_source_differs"),
    ({"scope": "different-scope"}, "admission_scope_or_revision_mismatch"),
    ({"subject_revision": "old-r0"}, "admission_scope_or_revision_mismatch"),
    ({"authority_id": "model-provider"}, "admission_authority_mismatch"),
    ({"origin": "model_candidate"}, "explicit_admission_decision_missing"),
    ({"decision": "pending"}, "admission_decision_pending"),
    ({"decision": "reject"}, "admission_explicitly_rejected"),
    ({"source_relation": "contradicts"}, "source_challenges_admission"),
    ({"source_relation": "not_established"}, "admission_support_not_established"),
])
def test_native_review_blocks_wrong_or_unestablished_basis(tmp_path, native_context, change, reason):
    request, auth = fixture(tmp_path, native_context, decision_changes=change)
    result = run(tmp_path, native_context, request, auth)
    assert result["status"] == "blocked" and not result["fact_admitted"]
    assert reason in json.dumps(result["admission_review"])
    assert not (tmp_path / "result/adopted-assertion.json").exists()


def test_missing_decision_remains_unknown(tmp_path, native_context):
    request, _ = fixture(tmp_path, native_context)
    value = json.loads(request.read_text()); value["review_decision"] = None; save(request, value)
    result = run(tmp_path, native_context, request)
    assert result["status"] == "blocked" and "missing_required_fact" in json.dumps(result["admission_review"])


@pytest.mark.parametrize("missing_record,scope_mismatch", [(True, False), (False, True)])
def test_adoption_cannot_bypass_declared_engineering_obligations(tmp_path, native_context, missing_record, scope_mismatch):
    request, auth = fixture(tmp_path, native_context, engineering=True, missing_record=missing_record, scope_mismatch=scope_mismatch)
    result = run(tmp_path, native_context, request, auth)
    assert result["admission_review"]["status"] == "clear"
    assert result["coverage_review"]["status"] == "blocked" and not result["fact_admitted"]


@pytest.mark.parametrize("field,value", [("request_sha256", "0" * 64), ("review_decision_sha256", "0" * 64), ("actor_id", "unbound-actor")])
def test_adoption_authorization_cannot_be_reused_for_a_different_subject(tmp_path, native_context, field, value):
    request, auth = fixture(tmp_path, native_context)
    document = json.loads(auth.read_text()); document[field] = value; save(auth, document)
    with pytest.raises(ValueError, match="assertion_adoption_"):
        run(tmp_path, native_context, request, auth)


def test_changing_obligations_invalidates_the_factual_review(tmp_path, native_context):
    request, _ = fixture(tmp_path, native_context, engineering=True)
    value = json.loads(request.read_text()); value["records"] = []; save(request, value)
    with pytest.raises(ValueError, match="review_subject_mismatch"):
        run(tmp_path, native_context, request)


def test_source_change_after_native_review_cannot_be_adopted(tmp_path, native_context, monkeypatch):
    request, auth = fixture(tmp_path, native_context)
    original = admission.execute_record
    def changed_after_review(*args, **kwargs):
        result = original(*args, **kwargs)
        save(tmp_path / "source.json", {"note": "source changed during review"})
        return result
    monkeypatch.setattr(admission, "execute_record", changed_after_review)
    with pytest.raises(ValueError, match="source_hash_mismatch"):
        run(tmp_path, native_context, request, auth)
    assert not (tmp_path / "result/adopted-assertion.json").exists()


def mapping_fixture(root, context, *, merge=False, relation=None, decision_state="reviewed"):
    binding, _ = context
    mapping = {"schema": evolution.SCHEMA, "mapping_id": "synthetic-mapping", "project_id": "synthetic-admission-project",
        "source_version": "0.2.2" if merge else "0.2.1", "target_version": "0.2.2",
        "kind": "merge" if merge else "split", "relation": relation or ("composes_review" if merge else "decomposes_review"),
        "sources": ["P-CLAIM-REPORT", "P-CLAIM-ENGINEERING"] if merge else ["P-CLAIM"],
        "targets": ["P-CLAIM-COMPOSITION"] if merge else ["P-CLAIM-REPORT", "P-CLAIM-ENGINEERING"], "review_decision": None}
    subject, _ = evolution.migration_subject(mapping, binding)
    decision = {"schema": "ontology-engineering.pattern-migration-decision/v1", "review_id": "synthetic-migration-decision",
        "subject_sha256": digest(subject), "actor_id": "synthetic-controller", "authority_id": "synthetic-controller",
        "action": evolution.REVIEW_SCOPE, "state": decision_state,
        "reason": "Explicit synthetic review of separate pattern identities; no equivalence or production application adoption.",
        "issued_at": bootstrap.now()}
    decision_path = save(root / "mapping-decision.json", decision)
    mapping["review_decision"] = {"path": decision_path.name, "sha256": digest(decision_path.read_bytes())}
    return save(root / "mapping.json", mapping)


@pytest.mark.parametrize("merge", [False, True])
def test_native_split_and_composition_preserve_exact_predecessor_meanings(tmp_path, native_context, merge):
    path = mapping_fixture(tmp_path, native_context, merge=merge)
    result = evolution.review_migration(path, tmp_path, *native_context, "synthetic-controller", tmp_path / "result")
    assert result["status"] == "clear" and result["old_identity_retained"] and result["old_definition_retained"]
    assert result["logical_equivalence"] == "not_established" and not result["project_applications_migrated"]
    subject = json.loads((tmp_path / "result/subject.json").read_text())
    assert "P-CLAIM" in subject["retained_definition_fingerprints"]
    assert set(subject["source_definitions"]).isdisjoint(subject["target_definitions"])


@pytest.mark.parametrize("relation,decision_state,reason", [
    ("equivalent", "reviewed", "logical_equivalence_not_established"),
    ("decomposes_review", "proposed", "pattern_mapping_not_reviewed"),
    ("composes_review", "reviewed", "mapping_relation_not_supported"),
])
def test_review_does_not_assert_equivalence_or_unreviewed_mapping(tmp_path, native_context, relation, decision_state, reason):
    path = mapping_fixture(tmp_path, native_context, relation=relation, decision_state=decision_state)
    result = evolution.review_migration(path, tmp_path, *native_context, "synthetic-controller", tmp_path / "result")
    assert result["status"] == "blocked" and reason in json.dumps(result["review"])


def test_modified_mapping_cannot_reuse_old_review(tmp_path, native_context):
    path = mapping_fixture(tmp_path, native_context)
    value = json.loads(path.read_text()); value["targets"] = ["P-CLAIM-COMPOSITION"]; save(path, value)
    with pytest.raises(ValueError, match="mapping_review_subject_mismatch"):
        evolution.review_migration(path, tmp_path, *native_context, "synthetic-controller", tmp_path / "result")
