"""Private-source handoff integrity and preservation of engineering unknowns."""
import copy
import json
from pathlib import Path
import shutil

import pytest

from ontology_engineering import supplier_knowledge as sk


def context():
    return sk.validate_context({"schema": sk.CONTEXT_SCHEMA, "task_id": "test-supplier",
        "object": {"id": "guide", "name": "linear guide", "revision": "A"},
        "goal": "Determine whether a changed motion requires a different guide.",
        "changes": ["The required motion now includes rotation."],
        "record_refs": ["/private/project/ledger.json"],
        "statements": [{"id": "h1", "text": "The original guide might be unsuitable.",
                        "status": "hypothesis", "source_refs": ["/private/project/observation.json"]}]})


def report(tmp_path, ctx):
    source = tmp_path / "source.pdf"
    source.write_bytes(b"fixture source bytes; PDF parsing is the producer's responsibility")
    text = "For type A, rotation is not suitable. Type B is a different construction."
    return {"schema": "misumi.search-result/v1", "status": "completed", "query": "Can it rotate?",
            "context_sha256": sk.fingerprint(ctx), "parameter_verification": "not_run",
            "results": [{"id": "book:page:1", "source_pdf": str(source), "source_url": "https://example.test/page.pdf",
                "catalog": "fixture", "catalog_title": "Fixture", "printed_page": "1", "pdf_page": 1,
                "source_sha256": sk.fingerprint(source.read_bytes()), "text_sha256": sk.fingerprint(text.encode()),
                "native_text": text, "judgment": {"relation": "direct"},
                "knowledge_units": [{"id": "span-1", "page_id": "book:page:1", "source_text_sha256": sk.fingerprint(text.encode()),
                    "source_span": {"start": 0, "end": len(text), "text": text, "sha256": sk.fingerprint(text.encode())},
                    "context_sha256": sk.fingerprint(ctx), "status": "source_passage_candidate", "applicability": "not_assessed"}]}]}


def test_unknown_is_not_false_and_revision_binds_history():
    c = sk.validate_context({"schema": sk.CONTEXT_SCHEMA, "task_id": "unknown"})
    assert c["object"]["id"] == ""
    assert c["statements"] == []
    before = context()
    after = copy.deepcopy(before)
    after["object"]["revision"] = "B"
    assert sk.fingerprint(before) != sk.fingerprint(after)
    assert before["statements"][0]["status"] == "hypothesis"


def test_local_references_do_not_enter_model_state():
    c = context()
    state = sk.model_context(c)
    assert "/private/" not in json.dumps(state)
    assert state["context_sha256"] == sk.fingerprint(c)
    assert state["changes"] == c["changes"]
    assert state["statements"][0]["status"] == "hypothesis"


@pytest.mark.parametrize("status", ["true", "verified_by_model", "passed", None])
def test_input_cannot_smuggle_a_model_verdict_as_statement_status(status):
    c = context()
    c["statements"][0]["status"] = status
    with pytest.raises(ValueError, match="invalid_statement"):
        sk.validate_context(c)


def test_unreviewed_text_stays_candidate_even_when_retrieval_completed(tmp_path):
    c = context()
    packet = sk.make_packet(report(tmp_path, c), c)
    assert packet["status"] == "completed"
    assert packet["adoption"] == "pending_agent_review"
    assert packet["semantic_review"] == packet["ontology_promotion"] == "not_run"
    assert packet["evidence_candidates"][0]["applicability"] == "not_assessed"


def test_optional_relative_source_locations_survive_handoff(tmp_path):
    c = context()
    r = report(tmp_path, c)
    r["results"][0].update(source_relative_path="source.pdf", assembled_relative_path="output/catalog.pdf",
                           assembled_pdf=str(tmp_path / "output/catalog.pdf"))
    source = sk.make_packet(r, c)["evidence_candidates"][0]["source"]
    assert source["source_relative_path"] == "source.pdf"
    assert source["assembled_relative_path"] == "output/catalog.pdf"
    assert source["assembled_pdf"] == str(tmp_path / "output/catalog.pdf")


@pytest.mark.parametrize("relative", ["../source.pdf", "/source.pdf", "x\\source.pdf", "x//source.pdf", "other.pdf", None])
def test_relative_locator_cannot_escape_or_relabel_verified_source(tmp_path, relative):
    c = context()
    r = report(tmp_path, c)
    r["results"][0]["source_relative_path"] = relative
    with pytest.raises(ValueError, match="supplier_handoff_relative_source_mismatch"):
        sk.make_packet(r, c)


def test_source_and_assembly_relative_locations_must_share_archive(tmp_path):
    c = context()
    r = report(tmp_path, c)
    r["results"][0].update(source_relative_path="source.pdf", assembled_relative_path="catalog.pdf",
                           assembled_pdf=str(tmp_path / "different-archive/catalog.pdf"))
    with pytest.raises(ValueError, match="supplier_handoff_relative_source_mismatch"):
        sk.make_packet(r, c)


@pytest.mark.parametrize("corruption", ["source", "span", "source_text", "context", "promoted"])
def test_changed_source_context_span_and_unreviewed_promotion_are_rejected(tmp_path, corruption):
    c = context()
    r = report(tmp_path, c)
    unit = r["results"][0]["knowledge_units"][0]
    if corruption == "source":
        Path(r["results"][0]["source_pdf"]).write_bytes(b"changed")
    elif corruption == "span":
        unit["source_span"]["text"] = "Rotation is suitable for every construction."
    elif corruption == "source_text":
        unit["source_text_sha256"] = "0" * 64
    elif corruption == "context":
        c["object"]["revision"] = "new"
    else:
        unit["applicability"] = "passed"
    with pytest.raises(ValueError, match="supplier_handoff"):
        sk.make_packet(r, c)


def test_method_and_topics_are_portable_and_multi_role():
    policy, sha = sk.load_policy()
    assert len(sha) == 64
    assert {t["id"] for t in policy["topics"]} >= {"mechanism", "selection", "interfaces", "failure"}
    assert "source-locked Semantica" in policy["method"]
    assert "missing observations remain unknown" in policy["resolved_method"].lower() or "未知" in policy["resolved_method"]
    for path in policy["method_sources"]:
        assert not Path(path).is_absolute()


def test_optional_tradeoffs_preserve_hard_constraints_unknowns_and_private_refs():
    original = context()
    c = copy.deepcopy(original)
    c.update(decision_criteria=[{"id": "precision", "dimension": "repeatability", "kind": "hard_constraint",
                               "statement": "The project requirement is 0.02 mm.", "source_refs": ["/private/spec.json"]},
                              {"id": "budget", "dimension": "total cost", "kind": "unknown",
                               "statement": "Budget has not been agreed."}],
             tradeoffs=["A is more precise, B is cheaper; comparative measurements are incomplete."],
             iteration={"id": "iteration-2", "focus": "Resolve the current comparison", "stop_condition": ""})
    result = sk.validate_context(c)
    assert result["decision_criteria"][0]["kind"] == "hard_constraint"
    assert result["decision_criteria"][1]["kind"] == "unknown"
    assert "/private/" not in json.dumps(sk.model_context(result))
    assert sk.fingerprint(result) != sk.fingerprint(original)
    assert sk.validate_context(original) == original
    assert "decision_criteria" not in original


def test_criteria_do_not_accept_model_scores_as_an_approval():
    c = context()
    c["decision_criteria"] = [{"id": "x", "dimension": "fit", "kind": "approved_by_jev", "statement": "yes"}]
    with pytest.raises(ValueError, match="invalid_criterion"):
        sk.validate_context(c)


def portable_method(tmp_path, monkeypatch):
    directory = tmp_path / "references"
    directory.mkdir()
    for name in ("supplier-knowledge-policy.json", "context-routing-instructions.json"):
        (directory / name).write_bytes((sk.ROOT / "references" / name).read_bytes())
    lock = json.loads((sk.ROOT / "runtime/semantic-bundles.json").read_text())
    bundle = lock["bundles"]["engineering-judgment-intake"]["path"]
    for name in ("runtime/semantic-bundles.json", "runtime/semantica-source-lock.json", bundle):
        destination = tmp_path / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes((sk.ROOT / name).read_bytes())
    monkeypatch.setattr(sk, "ROOT", tmp_path)
    return directory


def test_actual_shared_instructions_change_method_and_cache_identity(tmp_path, monkeypatch):
    directory = portable_method(tmp_path, monkeypatch)
    before, first = sk.load_policy()
    source = directory / "context-routing-instructions.json"
    modified = json.loads(source.read_text())
    modified["instructions"]["evidence_and_knowledge"].append("Distinct changed context must be re-examined.")
    source.write_text(json.dumps(modified))
    after, second = sk.load_policy()
    assert first != second
    assert before["resolved_method"] != after["resolved_method"]
    assert after["resolved_method"].endswith(after["pattern_question_preamble"])
    assert before["pattern_contract"] == after["pattern_contract"]
    assert all(a["status"] == "not_bundled" for a in after["method_bundle"]["source_availability"])


def test_present_book_source_must_match_reviewed_anchor(tmp_path, monkeypatch):
    directory = portable_method(tmp_path, monkeypatch)
    policy = json.loads((directory / "supplier-knowledge-policy.json").read_text())
    source = tmp_path / policy["book_lenses"][0]["anchors"][0]["path"]
    source.parent.mkdir(parents=True)
    source.write_text("unreviewed replacement book text")
    with pytest.raises(ValueError, match="book_anchor_changed"):
        sk.load_policy()


def attach_review(r, c):
    policy, sha = sk.load_policy()
    r.update(method_bundle=policy["method_bundle"], method_bundle_sha256=sha, knowledge_policy_sha256=sha)
    result = r["results"][0]
    parent = result["knowledge_units"][0]["source_span"]
    span = {**parent, "unit_id": "span-1", "parent_span_sha256": parent["sha256"]}
    r["review_plan"] = {"schema": "ontology-engineering.supplier-review-plan/v1", "status": "completed",
        "context_sha256": sk.fingerprint(c), "method_bundle_sha256": sha, "policy_sha256": sha,
        "adoption": "pending_agent_review", "execution": "not_run",
        "evidence_input": {"pages": [{"page_id": result["id"], "source_sha256": result["source_sha256"],
                                     "text_sha256": result["text_sha256"], "spans": [span]}]},
        "answers": {"review_change_dependencies": {"choice": "needed"}},
        "actions": [{"id": "review_change_dependencies", "choice": "needed", "status": "candidate",
                     "adoption": "pending_agent_review", "execution": "not_run"}],
        "selected_actions": ["review_change_dependencies"], "uncertain_actions": [],
        **sk.pattern_candidates(policy)}
    return r


def test_source_bound_review_is_handed_back_without_execution(tmp_path):
    c = context()
    r = attach_review(report(tmp_path, c), c)
    packet = sk.make_packet(r, c)
    assert packet["review_plan"] == r["review_plan"]
    assert packet["review_plan"]["execution"] == "not_run"


@pytest.mark.parametrize("corruption", [None, "identity", "meaning", "choice", "execution"])
def test_native_pattern_candidate_references_are_bound_without_semantic_execution(tmp_path, corruption):
    c = context()
    r = attach_review(report(tmp_path, c), c)
    policy, _ = sk.load_policy()
    answers = {qid: {"choice": "not_established"} for qid in policy["pattern_questions"]}
    projected = sk.pattern_candidates(policy, answers)
    r["review_plan"].update(projected)
    if corruption == "identity":
        r["review_plan"]["pattern_identity"] = {"pattern_sha256": "0" * 64}
    elif corruption == "meaning":
        r["review_plan"]["pattern_refs"][0]["method"] = "invented-independent-method"
    elif corruption == "choice":
        r["review_plan"]["pattern_refs"][0]["choice"] = "related"
    elif corruption == "execution":
        r["review_plan"]["pattern_refs"][0]["execution"] = "completed"
    if corruption:
        with pytest.raises(ValueError, match="supplier_handoff_pattern"):
            sk.make_packet(r, c)
    else:
        packet = sk.make_packet(r, c)
        assert packet["review_plan"]["pattern_refs"] == projected["pattern_refs"]
        assert packet["semantic_review"] == "not_run"
        assert packet["ontology_promotion"] == "not_run"


@pytest.mark.parametrize("corruption", ["context", "bundle", "source", "span", "execution", "choice"])
def test_review_cannot_detach_sources_or_turn_proposal_into_execution(tmp_path, corruption):
    c = context()
    r = attach_review(report(tmp_path, c), c)
    plan = r["review_plan"]
    if corruption == "context":
        plan["context_sha256"] = "0" * 64
    elif corruption == "bundle":
        r["method_bundle"]["method_text_sha256"] = "0" * 64
    elif corruption == "source":
        plan["evidence_input"]["pages"][0]["source_sha256"] = "0" * 64
    elif corruption == "span":
        plan["evidence_input"]["pages"][0]["spans"][0]["text"] = "Unrelated statement."
    elif corruption == "execution":
        plan["actions"][0]["execution"] = "completed"
    else:
        plan["selected_actions"] = []
    with pytest.raises(ValueError, match="supplier_handoff_review"):
        sk.make_packet(r, c)


def project_context():
    c = context()
    c["project"] = {"id": "fixture-project", "revision": "project-A"}
    c["decision_id"] = "guide-motion-choice"
    return sk.validate_context(c)


def test_decision_identity_is_optional_preserves_old_snapshot_and_binds_new_one(tmp_path):
    old = context()
    old_sha = sk.fingerprint(old)
    assert "decision_id" not in sk.validate_context(old)
    assert sk.fingerprint(sk.validate_context(old)) == old_sha
    assert "decision_id" not in sk.make_packet(report(tmp_path, old), old)
    current = project_context()
    packet = sk.make_packet(report(tmp_path, current), current)
    assert packet["decision_id"] == current["decision_id"]
    assert sk.model_context(current)["decision_id"] == current["decision_id"]
    changed = {**current, "decision_id": "different-choice"}
    assert sk.fingerprint(changed) != packet["context_sha256"]


@pytest.mark.parametrize("decision_id", ["", " \t", None, 3, "x" * 201])
def test_present_decision_identity_must_be_nonempty_text(decision_id):
    with pytest.raises(ValueError, match="knowledge_context"):
        sk.validate_context({**context(), "decision_id": decision_id})


def test_decision_identity_does_not_bypass_snapshot_byte_budget():
    c = project_context()
    assert len(json.dumps(c, ensure_ascii=False).encode()) < 8000
    c.update(background="中" * 1800, goal="中" * 1500)
    with pytest.raises(ValueError, match="knowledge_context_too_large"):
        sk.validate_context(c)


def test_generic_search_result_preserves_candidate_checks(tmp_path):
    c = project_context()
    r = report(tmp_path, c)
    r["schema"] = "ontology-engineering.knowledge-search-result/v1"
    packet = sk.make_packet(r, c)
    assert packet["evidence_candidates"][0]["knowledge_units"] == r["results"][0]["knowledge_units"]
    r["results"][0]["knowledge_units"][0]["applicability"] = "passed"
    with pytest.raises(ValueError, match="supplier_handoff_unreviewed_assertion"):
        sk.make_packet(r, c)


def test_source_hashing_does_not_read_whole_pdf_bytes(tmp_path, monkeypatch):
    c = project_context()
    r = report(tmp_path, c)
    source = Path(r["results"][0]["source_pdf"])
    source.write_bytes(b"large source fixture\n" * 100000)
    r["results"][0]["source_sha256"] = sk.fingerprint(source.read_bytes())
    read_bytes = Path.read_bytes
    def bounded(path):
        if path == source:
            raise AssertionError("PDF must be hashed as a stream")
        return read_bytes(path)
    monkeypatch.setattr(Path, "read_bytes", bounded)
    packet = sk.make_packet(r, c)
    assert sk.verify_packet_binding(packet, c)["status"] == "ready_for_agent_review"


def test_packet_binding_requires_same_explicit_project_object_and_decision(tmp_path):
    c = project_context()
    packet = sk.make_packet(attach_review(report(tmp_path, c), c), c)
    before = copy.deepcopy(packet)
    verification = sk.verify_packet_binding(packet, c)
    assert verification["status"] == "ready_for_agent_review"
    assert verification["missing_identity"] == verification["changed_fields"] == []
    assert verification["adoption"] == "pending_agent_review"
    assert verification["semantic_review"] == verification["ontology_promotion"] == "not_run"
    assert packet == before


@pytest.mark.parametrize("query_status,has_candidates", [
    ("failed", False), ("partial", False), ("partial", True),
    ("no_match_in_candidates", False), ("completed", True)])
def test_binding_preserves_query_failures_and_partial_candidates_without_adopting(tmp_path, query_status, has_candidates):
    c = project_context()
    r = report(tmp_path, c)
    r["status"] = query_status
    r["errors"] = ([{"stage": "source_query", "code": "fixture_source_failure"}]
                   if query_status in {"failed", "partial"} else [])
    if has_candidates:
        attach_review(r, c)
    else:
        r["results"] = []
    packet = sk.make_packet(r, c)
    before = copy.deepcopy(packet)
    verification = sk.verify_packet_binding(packet, c)
    assert verification["status"] == "ready_for_agent_review"
    assert verification["source_query_status"] == query_status
    assert verification["source_query_errors"] == r["errors"]
    assert verification["evidence_availability"] == ("returned_candidates" if has_candidates else "no_returned_candidates")
    assert verification["evidence_candidate_count"] == len(packet["evidence_candidates"])
    assert verification["review_plan_status"] == ("completed" if has_candidates else "not_run")
    assert verification["adoption"] == "pending_agent_review"
    assert verification["semantic_review"] == "not_run"
    assert packet == before


@pytest.mark.parametrize("field", ["task_id", "decision_id", "goal", "project.id", "project.revision", "object.id", "object.revision"])
def test_packet_cannot_silently_rebind_to_another_current_context(tmp_path, field):
    c = project_context()
    packet = sk.make_packet(report(tmp_path, c), c)
    current = copy.deepcopy(c)
    parts = field.split(".")
    if len(parts) == 2:
        current[parts[0]][parts[1]] += "-changed"
    else:
        current[field] += "-changed"
    verification = sk.verify_packet_binding(packet, current)
    assert verification["status"] == "changed"
    assert parts[0] in verification["changed_fields"]
    assert verification["packet_context_sha256"] != verification["current_context_sha256"]
    assert verification["adoption"] == "pending_agent_review"


@pytest.mark.parametrize("field", ["project.id", "project.revision", "object.id", "object.revision", "decision_id"])
def test_missing_engineering_identity_leaves_query_valid_but_handoff_unbound(tmp_path, field):
    c = project_context()
    parts = field.split(".")
    if len(parts) == 2:
        c[parts[0]][parts[1]] = ""
    else:
        del c[field]
    packet = sk.make_packet(report(tmp_path, c), c)
    verification = sk.verify_packet_binding(packet, c)
    assert packet["status"] == "completed"
    assert verification["status"] == "unbound"
    assert field in verification["missing_identity"]
    assert verification["semantic_review"] == "not_run"


@pytest.mark.parametrize("field", ["task_id", "project", "object", "decision_id", "context_sha256", "context_snapshot"])
def test_packet_internal_identity_tampering_is_not_a_changed_context(tmp_path, field):
    c = project_context()
    packet = sk.make_packet(report(tmp_path, c), c)
    if field == "context_snapshot":
        packet[field]["goal"] = "tampered goal"
    elif field in {"project", "object"}:
        packet[field] = {**packet[field], "id": "different-project-or-object"}
    else:
        packet[field] = "changed"
    with pytest.raises(ValueError, match="supplier_handoff_packet_(identity|context)_mismatch"):
        sk.verify_packet_binding(packet, c)


@pytest.mark.parametrize("corruption", ["source", "text", "span", "unit_context", "candidate_adoption", "packet_adoption", "review_span"])
def test_packet_binding_rechecks_sources_and_never_accepts_promoted_candidates(tmp_path, corruption):
    c = project_context()
    packet = sk.make_packet(attach_review(report(tmp_path, c), c), c)
    candidate = packet["evidence_candidates"][0]
    if corruption == "source":
        Path(candidate["source"]["source_pdf"]).write_bytes(b"changed original")
    elif corruption == "text":
        candidate["page_native_text"] += " Changed source text."
    elif corruption == "span":
        candidate["knowledge_units"][0]["source_span"]["text"] = "changed span"
    elif corruption == "unit_context":
        candidate["knowledge_units"][0]["context_sha256"] = "0" * 64
    elif corruption == "candidate_adoption":
        candidate["adoption"] = "adopted"
    elif corruption == "packet_adoption":
        packet["adoption"] = "adopted"
    else:
        packet["review_plan"]["evidence_input"]["pages"][0]["spans"][0]["text"] = "changed reviewed span"
    with pytest.raises(ValueError, match="supplier_handoff"):
        sk.verify_packet_binding(packet, c)


def local_pdf_report(tmp_path, c):
    from test_local_pdf_sources import pdf_fixture
    r = report(tmp_path, c)
    r["schema"] = "ontology-engineering.knowledge-search-result/v1"
    result = r["results"][0]
    source = Path(result["source_pdf"])
    pdf_fixture(source, ["First page.", result["native_text"]])
    result.update(source_sha256=sk.fingerprint(source.read_bytes()), pdf_page=2, printed_page=None,
                  text_origin="poppler_native_text")
    result["source_locator"] = {"schema": "ontology-engineering.source-page/v1", "kind": "local_pdf",
        "document_id": "fixture-book", "document_sha256": result["source_sha256"],
        "physical_page": 2, "page_count": 2, "title": "Fixture book", "edition": "Synthetic test edition",
        "printed_page": None, "printed_page_status": "unverified"}
    return r


@pytest.mark.skipif(not shutil.which("pdfinfo"), reason="local Poppler not installed")
def test_local_pdf_locator_is_verified_and_preserved_through_project_binding(tmp_path):
    c = project_context()
    r = local_pdf_report(tmp_path, c)
    packet = sk.make_packet(r, c)
    source = packet["evidence_candidates"][0]["source"]
    assert source["source_locator"] == r["results"][0]["source_locator"]
    assert source["source_locator"] is not r["results"][0]["source_locator"]
    assert source["text_origin"] == "poppler_native_text"
    assert source["pdf_page"] == 2 and source["printed_page"] is None
    assert sk.verify_packet_binding(packet, c)["status"] == "ready_for_agent_review"
    source["source_locator"]["physical_page"] = 1
    with pytest.raises(ValueError, match="source_locator_page_mismatch"):
        sk.verify_packet_binding(packet, c)


@pytest.mark.skipif(not shutil.which("pdfinfo"), reason="local Poppler not installed")
@pytest.mark.parametrize("fault", ["document", "outside_page", "false_page_count", "printed_page", "result_page"])
def test_local_pdf_locator_cannot_relabel_original_bytes_or_physical_page(tmp_path, fault):
    c = project_context()
    r = local_pdf_report(tmp_path, c)
    result = r["results"][0]
    locator = result["source_locator"]
    if fault == "document":
        locator["document_sha256"] = "0" * 64
    elif fault == "outside_page":
        locator["physical_page"] = result["pdf_page"] = 3
    elif fault == "false_page_count":
        locator["page_count"] = 3
    elif fault == "printed_page":
        locator["printed_page"], locator["printed_page_status"] = "200", "verified"
    else:
        result["pdf_page"] = 1
    with pytest.raises(ValueError, match="source_locator"):
        sk.make_packet(r, c)
