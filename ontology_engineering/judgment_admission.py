"""Explicit, source-bound project assertion adoption through native review.

The caller supplies its controlled project binding, factual review decision and,
for adoption, a separate exact authorization. These JSON records express the
caller's authority; they are not signatures or an identity provider. No model
response or imported semantic PASS is an admission input. Native semantics
remain in Semantica. Accepted records do not authorize physical product release.
"""
from __future__ import annotations

from pathlib import Path

from ontology_engineering.judgment_contracts import contracts, digest, source_selection
from ontology_engineering.judgment_review import _binding, _generated_record, execute_record, now, write_new
from ontology_engineering.jev_transport import strict_json
from ontology_engineering.method_evidence import _inside, _keys, _text, project_record

REQUEST_SCHEMA = "ontology-engineering.assertion-admission/v1"
DECISION_SCHEMA = "ontology-engineering.assertion-review-decision/v1"
AUTHORIZATION_SCHEMA = "ontology-engineering.assertion-adoption-authorization/v1"
ADOPTION_SCOPE = "adopt-project-assertion"


def subject_identity(request, binding_sha256):
    """The reviewed subject includes obligations; the decision cannot review itself."""
    return {"project_id": request["project_id"], "assertion": request["assertion"],
            "source": request["source"], "required_methods": request["required_methods"],
            "records": request["records"], "binding_sha256": binding_sha256}


def _reference(root, ref):
    _keys(ref, {"path", "sha256"}, "admission reference")
    path = _inside(root, ref["path"])
    raw = path.read_bytes()
    if digest(raw) != ref["sha256"]:
        raise ValueError("admission_reference_hash_mismatch")
    return path, strict_json(raw)


def _authorization(path, actor, binding, binding_sha, request_sha, decision_sha):
    raw = Path(path).read_bytes()
    value = strict_json(raw)
    _keys(value, {"schema", "authorization_id", "action", "actor_id", "authority_id",
                  "binding_sha256", "request_sha256", "review_decision_sha256", "reason", "issued_at"},
          "assertion adoption authorization")
    authority = binding["authority"]["decision"]
    if (value["schema"] != AUTHORIZATION_SCHEMA or value["action"] != ADOPTION_SCOPE or
        value["actor_id"] != actor or actor != authority["authority_id"] or
        value["authority_id"] != authority["authority_id"] or ADOPTION_SCOPE not in authority["scope"]):
        raise ValueError("assertion_adoption_authority_not_permitted")
    if (value["binding_sha256"], value["request_sha256"], value["review_decision_sha256"]) != (binding_sha, request_sha, decision_sha):
        raise ValueError("assertion_adoption_authorization_subject_mismatch")
    for key in ("authorization_id", "reason", "issued_at"):
        _text(value[key], key)
    return value, digest(raw)


def review_admission(request_path, evidence_root, binding_path, workspace, actor, output, *, authorization=None):
    """Review freshly; optionally record one separately authorized adoption.

    An output directory is append-only for one invocation. Reusing it fails
    before any adoption is overwritten. Engineering obligations are declared by
    the controlled review subject, not inferred as complete from model labels.
    """
    raw = Path(request_path).read_bytes()
    request = strict_json(raw)
    _keys(request, {"schema", "admission_id", "project_id", "assertion", "source", "review_decision",
                    "required_methods", "records"}, "admission request")
    if request["schema"] != REQUEST_SCHEMA:
        raise ValueError("unsupported_assertion_admission_schema")
    _text(request["admission_id"], "admission_id")
    assertion = request["assertion"]
    _keys(assertion, {"id", "statement", "subject_revision", "scope", "kind"}, "assertion")
    for key, value in assertion.items():
        _text(value, "assertion." + key)
    if assertion["kind"] not in {"reported_statement", "engineering_claim"}:
        raise ValueError("unsupported_assertion_kind")
    claim = {k: assertion[k] for k in ("id", "statement", "subject_revision", "scope")}
    root = Path(evidence_root).resolve()
    text = source_selection(request["source"], root)
    if assertion["kind"] == "reported_statement" and assertion["statement"] != text:
        raise ValueError("reported_statement_is_not_the_exact_source_selection")
    binding_raw = Path(binding_path).read_bytes()
    binding_sha = digest(binding_raw)
    binding = _binding(binding_path, request["project_id"], contracts()["identity"])
    methods = request["required_methods"]
    excluded = {"assertion-admission-review", "pattern-evolution-review", "pattern-realization"}
    if (not isinstance(methods, list) or any(not isinstance(m, str) for m in methods) or
        len(methods) != len(set(methods)) or set(methods) & excluded or
        set(methods) - set(contracts()["profiles"]["profiles"])):
        raise ValueError("invalid_admission_obligations")
    if assertion["kind"] == "engineering_claim" and not methods:
        raise ValueError("engineering_admission_requires_declared_obligations")
    if not isinstance(request["records"], list):
        raise ValueError("invalid_admission_records")

    decision, decision_sha = None, None
    if request["review_decision"] is not None:
        _, decision = _reference(root, request["review_decision"])
        decision_sha = request["review_decision"]["sha256"]
        _keys(decision, {"schema", "review_id", "subject_sha256", "assertion_sha256", "source_sha256",
                        "scope", "subject_revision", "authority_id", "authority_scope", "origin", "decision",
                        "source_relation", "applicability_review", "reason", "issued_at"}, "factual review decision")
        if decision["schema"] != DECISION_SCHEMA or decision["subject_sha256"] != digest(subject_identity(request, binding_sha)):
            raise ValueError("assertion_review_subject_mismatch")
        fact = binding["authority"]["fact"]
        scope = decision["authority_scope"]
        if (not isinstance(scope, list) or not scope or any(not isinstance(s, str) for s in scope) or
            len(set(scope)) != len(scope) or set(scope) - set(fact["scope"])):
            raise ValueError("assertion_review_authority_scope_mismatch")
        for key in ("review_id", "reason", "issued_at"):
            _text(decision[key], "review." + key)
    approval = None
    if authorization is not None:
        if decision is None:
            raise ValueError("assertion_adoption_requires_factual_review")
        approval = _authorization(authorization, actor, binding, binding_sha, digest(raw), decision_sha)

    out = Path(output)
    out.mkdir(parents=True, exist_ok=False)
    write_new(out / "request.json", raw)
    write_new(out / "subject.json", subject_identity(request, binding_sha))
    if decision is not None:
        write_new(out / "decision.json", decision)
    checks, projections, seen = [], [], set()
    for index, ref in enumerate(request["records"]):
        path, record = _reference(root, ref)
        method = record["method"]
        if record["claim"] != claim or method not in methods or method in seen:
            raise ValueError("admission_method_record_subject_mismatch")
        seen.add(method)
        rdf, audit = project_record(path, root, bundle_name="engineering-judgment-intake")
        projections.append((path, digest(rdf), audit["record_sha256"]))
        checks.append(execute_record(path, root, binding_path, workspace, actor, out / f"obligation-{index}",
                                     project_id=request["project_id"]))
    coverage = None
    if methods:
        coverage_path = _generated_record(out, "coverage", claim, "pattern-realization",
            {"required_functions": methods, "realized_functions": sorted(seen),
             "verified_functions": [c["method"] for c in checks if c["status"] == "clear"],
             "challenged_functions": [], "inventory_complete": True, "review_origin": "controlled_native_execution"},
            {"subject_sha256": digest(subject_identity(request, binding_sha)), "actual_checks": checks,
             "meaning": "Execution coverage of the explicitly reviewed obligation inventory, not physical validation or discovery of every engineering obligation."})
        coverage = execute_record(coverage_path, out, binding_path, workspace, actor, out / "coverage-review",
                                  project_id=request["project_id"])
    d = decision or {}
    fields = {"assertion_kind": assertion["kind"], "assertion_sha256": digest(assertion),
        "reviewed_assertion_sha256": d.get("assertion_sha256", "0" * 64),
        # This digest binds the source descriptor, including selection and context.
        "source_sha256": digest(request["source"]), "reviewed_source_sha256": d.get("source_sha256", "0" * 64),
        "claim_scope": assertion["scope"], "review_scope": d.get("scope", "not_reviewed"),
        "assertion_revision": assertion["subject_revision"], "reviewed_assertion_revision": d.get("subject_revision", "not_reviewed"),
        "expected_authority": binding["authority"]["fact"]["authority_id"],
        "review_authority": d.get("authority_id", "not_reviewed"),
        "review_origin": d.get("origin", "missing"), "review_decision": d.get("decision", "pending"),
        "reviewed_source_relation": d.get("source_relation", "not_established"),
        "source_integrity_verified": True, "review_id": d.get("review_id", "not_reviewed"),
        "applicability_review": d.get("applicability_review", "not_reviewed")}
    # Missing decision is absence, not a fabricated contradictory decision.
    if decision is None:
        for key in ("reviewed_assertion_sha256", "reviewed_source_sha256", "review_scope",
                    "reviewed_assertion_revision", "review_authority", "review_id"):
            fields.pop(key)
    record_path = _generated_record(out, "admission", claim, "assertion-admission-review", fields,
        {"binding_sha256": binding_sha, "request_sha256": digest(raw), "review_decision_sha256": decision_sha,
         "source_document_sha256": request["source"]["sha256"], "source_selection": request["source"]["selection"],
         "meaning": "Exact source bytes and declared external review; no reviewer authentication or model-to-fact conversion."})
    admission = execute_record(record_path, out, binding_path, workspace, actor, out / "admission-review",
                               project_id=request["project_id"])
    eligible = admission["status"] == "clear" and (coverage is None or coverage["status"] == "clear")
    result = {"schema": "ontology-engineering.assertion-admission-result/v1", "admission_id": request["admission_id"],
        "request_sha256": digest(raw), "binding_sha256": binding_sha, "decision_sha256": decision_sha,
        "admission_review": admission, "coverage_review": coverage, "obligation_reviews": checks,
        "status": "eligible_for_separate_adoption" if eligible else "blocked", "fact_admitted": False,
        "product_release": "not_performed", "reviewer_authentication": "caller_control_plane_responsibility"}
    if approval is not None and eligible:
        if Path(binding_path).read_bytes() != binding_raw or Path(request_path).read_bytes() != raw:
            raise ValueError("admission_subject_changed_during_review")
        if source_selection(request["source"], root) != text:
            raise ValueError("admission_source_changed_during_review")
        _reference(root, request["review_decision"])
        for path, rdf_sha, record_sha in projections:
            current_rdf, current_audit = project_record(path, root, bundle_name="engineering-judgment-intake")
            if (digest(current_rdf), current_audit["record_sha256"]) != (rdf_sha, record_sha):
                raise ValueError("admission_obligation_changed_during_review")
        adopted = {"schema": "ontology-engineering.adopted-project-assertion/v1", "project_id": request["project_id"], "assertion": assertion,
            "source": request["source"], "source_selection_text": text, "decision": decision,
            "authorization": approval[0], "authorization_sha256": approval[1],
            "binding_sha256": binding_sha, "request_sha256": digest(raw), "recorded_at": now(),
            "admission_report_sha256": admission["report_sha256"],
            "epistemic_status": "adopted_source_statement" if assertion["kind"] == "reported_statement" else "adopted_scoped_engineering_claim",
            "scope_boundary": "Controlled project assertion under this exact snapshot; no certification, physical product release or TBox promotion."}
        adopted_path = write_new(out / "adopted-assertion.json", adopted)
        abox_record = _generated_record(out, "adopted", claim, "pattern-claim",
            {"assertion_kind": assertion["kind"], "source_relation": decision["source_relation"], "source_verified": True,
             "admission_basis": "verbatim_source_record" if assertion["kind"] == "reported_statement" else "controlled_engineering_review"},
            {"adopted_record_sha256": digest(adopted_path.read_bytes()), "scope_boundary": adopted["scope_boundary"]})
        rdf, audit = project_record(abox_record, out, bundle_name="engineering-judgment-intake")
        write_new(out / "adopted-abox.ttl", rdf)
        write_new(out / "adopted-abox-projection.json", audit)
        result.update(status="adopted_under_exact_project_snapshot", fact_admitted=True,
                      adopted_record_sha256=digest(adopted_path.read_bytes()), adopted_abox_sha256=digest(rdf))
    write_new(out / "summary.json", result)
    return result
