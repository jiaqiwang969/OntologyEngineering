"""Source-bound review orchestration, with no imported semantic PASS interface.

Jev outputs stay candidates. Method meanings and findings execute only through
the promoted Semantica package. A review establishes scoped record consistency,
not authenticity of an engineer's assertion or physical product acceptance.
"""
from __future__ import annotations

from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

from ontology_engineering import semantic_engagement
from ontology_engineering.judgment_batch import journal
from ontology_engineering.judgment_contracts import contracts, digest, encoded, validate_lock
from ontology_engineering.jev_transport import strict_json, validate_response
from ontology_engineering.method_evidence import _inside, _keys, _text, pointer_value, project_record
from scripts.semantic_bundle_transport import load_bundle


def now():
    return datetime.now(timezone.utc).isoformat()


def write_new(path, value):
    path = Path(path)
    with path.open("xb") as stream:
        stream.write(value if isinstance(value, bytes) else encoded(value) + b"\n")
    return path


def read_candidates(prepared, journal_path):
    """Rebind every candidate to its stored request and parsed raw answer.

    This is a local provenance audit, not a cryptographic provider attestation.
    Historical configurations need their original environment; no silent replay
    under the current lock is accepted.
    """
    validate_lock(prepared["deployment"])
    manifest = prepared["input"]
    if not Path(journal_path).is_file():
        raise ValueError("candidate_journal_missing")
    records = []
    with journal(journal_path, manifest["project_id"], manifest["access_scope"]) as db:
        run = db.execute("SELECT * FROM runs WHERE id=?", (manifest["batch_id"],)).fetchone()
        if not run or (run["input_sha"], run["lock_sha"]) != (prepared["input_sha256"], prepared["deployment_sha256"]):
            raise ValueError("candidate_run_identity_mismatch")
        for item in prepared["items"]:
            rows = list(db.execute("SELECT c.*,a.request,a.response,a.request_sha,a.kind,a.run_id AS attempt_run,a.item_id AS attempt_item FROM candidates c JOIN attempts a ON a.id=c.attempt_id WHERE c.run_id=? AND c.item_id=?", (run["id"], item["id"])))
            known = {r["question_id"]: strict_json(r["answer"]) for r in rows}
            answers = []
            for row in rows:
                payload, response = strict_json(row["request"]), strict_json(row["response"])
                qid = row["question_id"]
                if (row["attempt_run"], row["attempt_item"]) != (run["id"], item["id"]):
                    raise ValueError("candidate_attempt_identity_mismatch")
                if qid not in item["question_ids"] or set(payload) != {"model", "state", "questions"}:
                    raise ValueError("candidate_request_identity_mismatch")
                group = list(payload["questions"])
                if not group or not any(set(group).issubset(stage) for stage in item["stages"]):
                    raise ValueError("candidate_stage_mismatch")
                if prepared["deployment"]["strategy"] == "single" and len(group) != 1:
                    raise ValueError("candidate_strategy_mismatch")
                expected_state = deepcopy(item["state"])
                dependencies = prepared["catalog"].get("dependencies", {})
                needs = {dep for q in group for dep in dependencies.get(q, [])}
                if needs:
                    if not needs.issubset(known):
                        raise ValueError("candidate_dependency_missing")
                    expected_state["prior_answers"] = {q: known[q] for q in sorted(needs)}
                expected_questions = {q: prepared["catalog"]["questions"][q] for q in group}
                if payload != {"model": prepared["deployment"]["model"], "state": expected_state, "questions": expected_questions}:
                    raise ValueError("candidate_request_content_mismatch")
                key = digest({"project": manifest["project_id"], "access_scope": manifest["access_scope"], "source": item["source"], "claim": item["claim"], "lock": prepared["deployment"], "transport": run["transport"], "payload": payload})
                valid, _ = validate_response(response, payload)
                answer = strict_json(row["answer"])
                cid = digest({"run": run["id"], "item": item["id"], "question": qid, "request": key, "answer": answer})
                if key != row["request_sha"] or valid.get(qid) != answer or cid != row["candidate_id"]:
                    raise ValueError("candidate_raw_response_mismatch")
                answers.append({"question_id": qid, "answer": answer, "candidate_id": cid,
                                "request_sha256": key, "response_sha256": digest(response), "attempt_id": row["attempt_id"],
                                "execution_kind": row["kind"], "fact_admitted": False})
            records.append({"item": deepcopy(item), "answers": answers})
    return records


def _binding(binding_path, project_id, bundle):
    binding = strict_json(Path(binding_path).read_bytes())
    if binding["project"]["project_id"] != project_id:
        raise ValueError("review_project_mismatch")
    if (binding["semantic_target"]["kind"] != "workspace" or
        binding["semantic_target"]["package_id"] != bundle["package_id"] or
        binding["baseline"] != {"version": bundle["package_version"], "digest": bundle["package_sha256"]}):
        raise ValueError("review_bundle_binding_mismatch")
    return binding


def execute_record(record_path, evidence_root, binding_path, workspace, actor, output, *, project_id):
    """Always execute freshly; saved result JSON cannot replace this call."""
    rdf, audit = project_record(record_path, evidence_root, bundle_name="engineering-judgment-intake")
    return execute_projection(rdf, audit, binding_path, workspace, actor, output, project_id=project_id)


def execute_projection(rdf, audit, binding_path, workspace, actor, output, *, project_id):
    """Bind a deterministic projection to the single source-locked review entry."""
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    binding = _binding(binding_path, project_id, audit["package"])
    rid = audit["record_sha256"]
    evidence_path = write_new(output / "projection.ttl", rdf)
    write_new(output / "projection.json", audit)
    captured = now()
    task = {"$schema": "ontology-engineering.semantic-task-envelope/v1", "task_id": "method-review-"+rid,
            "task_kind": "source-bound-method-review", "intent": "Review one declared evidence obligation",
            "project": project_id, "domain": binding["project"]["domain"],
            "requested_decision": "Scoped record consistency only; no physical acceptance", "actor_id": _text(actor, "actor"),
            "requested_actions": ["review"], "required_capabilities": ["semantica.ontology.decision-review/v1"],
            "evidence": [{"source_id": "projection-"+rid, "uri": binding["evidence"]["logical_root"].rstrip("/")+"/projection-"+rid,
                          "sha256": digest(rdf), "media_type": "text/turtle", "captured_at": captured}], "created_at": captured}
    task_path = write_new(output / "task.json", task)
    result = semantic_engagement.review(binding_path, workspace=workspace, task=task_path, evidence_file=evidence_path,
        source_id=task["evidence"][0]["source_id"], evidence_format="turtle", scope_id=audit["scope"],
        focus_iri=audit["focus"], focus_type_iri=audit["focus_type"], query_asset_id=audit["query_asset"], shape_asset_id=audit["shape_asset"])
    native = result["execution"]["review"]
    # Check actual returned identity before it can contribute to any coverage.
    expected = {"scope_id": audit["scope"], "focus_iri": audit["focus"], "focus_type_iri": audit["focus_type"]}
    if (any(native.get(k) != v for k, v in expected.items()) or native["evidence"]["sha256"] != audit["projection_sha256"] or
        native["package"]["registry_package_sha256"] != audit["package"]["package_sha256"] or
        native["package"]["query_asset_id"] != audit["query_asset"] or native["package"]["shape_asset_id"] != audit["shape_asset"] or
        native["package"]["origin"] != "promoted_registry"):
        raise ValueError("actual_review_identity_mismatch")
    write_new(output / "review.json", result)
    return {"method": audit["method"], "projection_sha256": audit["projection_sha256"], "record_sha256": rid,
            "report_sha256": native["report_sha256"], "status": native["status"], "findings": native["findings"],
            "violations": native["violations"], "execution_origin": "fresh_native_review",
            "relative_report": output.name+"/review.json", "fact_admitted": False}


def _generated_record(directory, name, claim, method, fields, provenance):
    snapshot = {"fields": fields, "provenance": provenance}
    path = write_new(directory / (name+"-snapshot.json"), snapshot)
    record = {"schema": "ontology-engineering.method-evidence/v1", "record_id": name, "claim": claim, "method": method,
              "sources": [{"id": name, "path": path.name, "sha256": digest(path.read_bytes()), "media_type": "application/json"}],
              "facts": {key: {"source_id": name, "pointer": "/fields/"+key} for key in fields}}
    return write_new(directory / (name+"-record.json"), record)


def review_candidates(prepared, journal_path, binding_path, workspace, actor, output):
    """Audit raw candidates and review their candidate role; never accept content."""
    records = read_candidates(prepared, journal_path)
    out = Path(output)
    out.mkdir(parents=True, exist_ok=False)
    result = []
    for index, record in enumerate(records):
        item = record["item"]
        answer = next((a for a in record["answers"] if a["question_id"] == "source_relation"), None)
        claim = {key: item["claim"][key] for key in ("id", "statement", "subject_revision", "scope")}
        # Missing execution remains unknown. No positive defaults fill evidence.
        fields = {"assertion_kind": "engineering_claim", "source_relation": answer["answer"]["choice"] if answer else "not_established",
                  "admission_basis": "model_candidate", "source_verified": True}
        path = _generated_record(out, "candidate-"+str(index), claim, "pattern-claim", fields,
            {"source": item["source"], "candidate": answer, "deployment_sha256": prepared["deployment_sha256"],
             "meaning": "Source selection and stored model response verified. The asserted support relation is not adopted."})
        check = execute_record(path, out, binding_path, workspace, actor, out / ("check-"+str(index)), project_id=prepared["input"]["project_id"])
        result.append({"item_id": item["id"], "claim": claim, "candidate_review": check,
                       "required_methods": item["required_methods"], "outstanding_engineering_reviews": item["required_methods"],
                       "fact_admitted": False})
    report = {"schema": "ontology-engineering.candidate-review/v1", "items": result, "fact_admission": "not_performed",
              "input_sha256": prepared["input_sha256"], "deployment_sha256": prepared["deployment_sha256"],
              "meaning": "All source relations, including counterclaims, remain candidate assertions. Required engineering reviews are preserved."}
    write_new(out / "summary.json", report)
    return report


def project_impact(record_path, evidence_root):
    """Project source-declared edges; dependency closure runs only in Semantica."""
    raw = Path(record_path).read_bytes()
    record = strict_json(raw)
    _keys(record, {"schema", "project_id", "source"}, "impact record")
    if record["schema"] != "ontology-engineering.support-impact/v1":
        raise ValueError("unsupported_impact_record")
    facts = _ref(evidence_root, record["source"])
    _keys(facts, {"scope", "revision", "inventory_complete", "nodes", "edges", "triggers"}, "impact source")
    for key in ("scope", "revision"):
        _text(facts[key], "impact."+key)
    if type(facts["inventory_complete"]) is not bool:
        raise ValueError("invalid_impact_completeness")
    spec, payload, _ = load_bundle("engineering-judgment-intake")
    manifest = strict_json(payload[spec["manifest"]])
    asset = next((x for x in manifest["assets"] if x["asset_id"] == "impact-profile"), None)
    if asset is None:
        raise ValueError("impact_capability_absent_in_selected_package")
    profile = strict_json(payload[asset["path"]])
    ns = profile["namespace"]
    base = ns + "impact/" + digest({"project":record["project_id"],"scope":facts["scope"],"revision":facts["revision"]})+"/"
    focus = base+"review"
    literal = lambda value: encoded(value).decode()
    iri = lambda identity: base+"node/"+quote(identity, safe="")
    rows = [f'<{focus}> a <{profile["focus_type"]}> ; <{ns}impactInventoryComplete> {literal(facts["inventory_complete"])} .']
    nodes = {}
    if not isinstance(facts["nodes"], list) or not facts["nodes"]:
        raise ValueError("impact_node_inventory_missing")
    for node in facts["nodes"]:
        _keys(node, set(profile["node_fields"]), "impact node")
        for value in node.values():
            _text(value, "node field")
        if node["id"] in nodes:
            raise ValueError("duplicate_impact_node")
        nodes[node["id"]] = node
        rows.extend([f'<{focus}> <{ns}impactMember> <{iri(node["id"])}> .',
                     f'<{iri(node["id"])}> <{ns}subjectRevision> {literal(node["revision"])} ; <{ns}objectKind> {literal(node["kind"])} .'])
    if not isinstance(facts["edges"], list) or not isinstance(facts["triggers"], list):
        raise ValueError("impact_edges_or_triggers_missing")
    seen = set()
    for edge in facts["edges"]:
        _keys(edge, set(profile["edge_fields"]), "impact edge")
        if edge["from"] not in nodes or edge["to"] not in nodes or edge["kind"] not in profile["relation_properties"]:
            raise ValueError("impact_edge_identity_mismatch")
        signature = (edge["from"], edge["to"], edge["kind"])
        if signature in seen:
            raise ValueError("duplicate_impact_edge")
        seen.add(signature)
        prop = profile["relation_properties"][edge["kind"]]
        rows.append(f'<{iri(edge["from"])}> <{ns}{prop}> <{iri(edge["to"])}> .')
    seen = set()
    for trigger in facts["triggers"]:
        _keys(trigger, {"node", "kind"}, "impact trigger")
        if trigger["node"] not in nodes or trigger["kind"] not in profile["trigger_kinds"] or trigger["node"] in seen:
            raise ValueError("impact_trigger_identity_mismatch")
        seen.add(trigger["node"])
        rows.extend([f'<{focus}> <{ns}impactTrigger> <{iri(trigger["node"])}> .',
                     f'<{iri(trigger["node"])}> <{ns}triggerKind> {literal(trigger["kind"])} .'])
    rows.append(f'<{focus}> <{ns}sourceSha256> {literal(record["source"]["sha256"])} ; <{ns}sourcePointer> {literal(record["source"]["pointer"])} .')
    rdf = ("\n".join(rows)+"\n").encode()
    return rdf, {"method":"support-change-impact", "record_sha256":digest(raw), "projection_sha256":digest(rdf),
                 "focus":focus, "focus_type":profile["focus_type"], "scope":facts["scope"],
                 "query_asset":profile["query_asset"], "shape_asset":profile["shape_asset"],
                 "package":{k:spec[k] for k in ("package_id","package_version","package_sha256","sha256")},
                 "project_id":record["project_id"], "source":record["source"],
                 "nodes":{iri(k):v for k,v in nodes.items()}, "semantic_execution":"not_run",
                 "meaning":"Edges and triggers are source assertions. Transport projection does not infer dependency closure or physical causality."}


def review_impact(record_path, evidence_root, binding_path, workspace, actor, output):
    rdf, audit = project_impact(record_path, evidence_root)
    return execute_projection(rdf, audit, binding_path, workspace, actor, output, project_id=audit["project_id"])


def _ref(root, ref):
    _keys(ref, {"path", "sha256", "pointer"}, "record reference")
    data = _inside(Path(root).resolve(), ref["path"]).read_bytes()
    if digest(data) != ref["sha256"]:
        raise ValueError("review_source_hash_mismatch")
    return pointer_value(strict_json(data), ref["pointer"])


def review_plan(plan_path, evidence_root, binding_path, workspace, actor, output):
    """Execute declared method records, then ask native Semantica about coverage.

    Function-to-claim mapping and completeness are source assertions. Summation
    uses only freshly returned, identity-checked results; this function offers
    no result-import option and never changes project approval status.
    """
    plan = strict_json(Path(plan_path).read_bytes())
    _keys(plan, {"schema", "project_id", "claims", "coverage"}, "review plan")
    if plan["schema"] != "ontology-engineering.method-review-plan/v1":
        raise ValueError("unsupported_review_plan")
    _binding(binding_path, plan["project_id"], contracts()["identity"])
    if not isinstance(plan["claims"], list) or not plan["claims"]:
        raise ValueError("review_claim_inventory_missing")
    out = Path(output); out.mkdir(parents=True, exist_ok=False)
    results, ids = [], set()
    for index, entry in enumerate(plan["claims"]):
        _keys(entry, {"claim", "required_methods", "records"}, "claim review")
        claim = entry["claim"]
        _keys(claim, {"id", "statement", "subject_revision", "scope"}, "review claim")
        for value in claim.values():
            _text(value, "claim field")
        if claim["id"] in ids:
            raise ValueError("duplicate_review_claim")
        ids.add(claim["id"])
        required = entry["required_methods"]
        if not isinstance(required, list) or not required or len(set(required)) != len(required) or set(required) - set(contracts()["profiles"]["profiles"]):
            raise ValueError("invalid_review_obligations")
        if not isinstance(entry["records"], list):
            raise ValueError("review_records_not_list")
        checks = []
        methods = set()
        for j, ref in enumerate(entry["records"]):
            _keys(ref, {"path", "sha256"}, "method record")
            path = _inside(Path(evidence_root).resolve(), ref["path"])
            raw = path.read_bytes()
            if digest(raw) != ref["sha256"]:
                raise ValueError("method_record_hash_mismatch")
            record = strict_json(raw)
            if record["claim"] != claim:
                raise ValueError("method_record_claim_identity_mismatch")
            method = record["method"]
            if method == "pattern-realization":
                raise ValueError("coverage_requires_fresh_session_aggregation")
            if method not in required or method in methods:
                raise ValueError("method_record_obligation_mismatch")
            methods.add(method)
            checks.append(execute_record(path, evidence_root, binding_path, workspace, actor, out / f"claim-{index}-method-{j}", project_id=plan["project_id"]))
        results.append({"claim": claim, "required_methods": required, "checks": checks,
                        "missing_methods": sorted(set(required)-methods), "fact_admitted": False})
    coverage = plan["coverage"]
    _keys(coverage, {"claim", "source"}, "coverage")
    facts = _ref(evidence_root, coverage["source"])
    _keys(facts, {"required_functions", "realized_functions", "challenged_functions", "inventory_complete", "function_claims"}, "coverage source")
    mapping = facts["function_claims"]
    for key in ("required_functions", "realized_functions", "challenged_functions"):
        values = facts[key]
        if not isinstance(values, list) or any(not isinstance(v, str) or not v.strip() for v in values) or len(set(values)) != len(values):
            raise ValueError("invalid_function_inventory")
    if type(facts["inventory_complete"]) is not bool:
        raise ValueError("invalid_inventory_completeness")
    if not isinstance(mapping, dict) or set(mapping) - set(facts["required_functions"]):
        raise ValueError("invalid_function_claim_mapping")
    by_claim = {r["claim"]["id"]: r for r in results}
    verified = []
    for function in facts["required_functions"]:
        support = mapping.get(function, [])
        if not isinstance(support, list) or len(set(support)) != len(support) or set(support)-ids:
            raise ValueError("unknown_or_duplicate_support_claim")
        if any(any(by_claim[c]["claim"][k] != coverage["claim"][k] for k in ("scope", "subject_revision")) for c in support):
            raise ValueError("function_support_configuration_mismatch")
        # This is execution bookkeeping, not a new physical entailment rule.
        if support and all(not by_claim[c]["missing_methods"] and all(check["status"] == "clear" for check in by_claim[c]["checks"]) for c in support):
            verified.append(function)
    fields = {k: facts[k] for k in ("required_functions", "realized_functions", "challenged_functions", "inventory_complete")}
    fields.update(verified_functions=verified, review_origin="controlled_native_execution")
    path = _generated_record(out, "coverage", coverage["claim"], "pattern-realization", fields,
        {"plan_sha256": digest(Path(plan_path).read_bytes()), "coverage_source": coverage["source"], "executed_checks": results,
         "meaning": "Coverage of actual scoped semantic reviews, not physical validation or evidence independence."})
    aggregate = execute_record(path, out, binding_path, workspace, actor, out / "coverage-review", project_id=plan["project_id"])
    report = {"schema": "ontology-engineering.method-review-session/v1", "plan_sha256": digest(Path(plan_path).read_bytes()),
              "claims": results, "coverage": aggregate, "semantic_reviewed_functions": verified,
              "engineering_verdict": "not_assessed", "fact_admission": "not_performed", "product_release": "not_performed"}
    write_new(out / "summary.json", report)
    return report
