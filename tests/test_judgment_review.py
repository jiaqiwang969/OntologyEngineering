"""Review transport integrity; mock execution cases do not claim native semantics."""
from copy import deepcopy
import json
from pathlib import Path
import sqlite3

import pytest

from ontology_engineering.judgment_contracts import ROOT, deployment_lock, digest, encoded, prepare_batch
from ontology_engineering.judgment_batch import execute
from ontology_engineering import judgment_review as review


EXAMPLES = ROOT / "examples/judgment_intake"


class Fixture:
    identity = "synthetic-review-transport/v1"
    kind = "fixture"

    def __call__(self, payload):
        answers = {}
        for qid, question in payload["questions"].items():
            choice = next(iter(question["criteria"]))
            answers[qid] = {"type":"choice", "choice":choice, "confidence":1.0,
                            "probabilities":{k:float(k == choice) for k in question["criteria"]}}
        return {"model":payload["model"], "answers":answers, "usage":{"input_tokens":0,"output_tokens":0}}


def run_fixture(tmp_path):
    doc = json.loads((EXAMPLES / "batch.json").read_text())
    doc["items"] = doc["items"][:2]
    p = prepare_batch(doc, EXAMPLES, deployment_lock(model="jev-1.13.0"))
    db = tmp_path / "jobs.sqlite"
    execute(p, db, Fixture())
    return p, db


def test_candidate_raw_response_and_obligations_survive_projection(tmp_path):
    prepared, db = run_fixture(tmp_path)
    result = review.read_candidates(prepared, db)
    assert len(result) == 2 and len(result[0]["answers"]) == 13
    assert all(a["fact_admitted"] is False for r in result for a in r["answers"])
    assert result[0]["item"]["required_methods"] == prepared["items"][0]["required_methods"]


@pytest.mark.parametrize("change", ["answer", "request", "attempt", "candidate_id"])
def test_tampered_candidates_cannot_enter_semantic_projection(tmp_path, change):
    prepared, path = run_fixture(tmp_path)
    with sqlite3.connect(path) as db:
        if change == "answer":
            row = db.execute("SELECT rowid,answer FROM candidates LIMIT 1").fetchone()
            answer = json.loads(row[1]); answer["confidence"] = 0.2
            db.execute("UPDATE candidates SET answer=? WHERE rowid=?", (json.dumps(answer), row[0]))
        elif change == "request":
            row = db.execute("SELECT id,request FROM attempts LIMIT 1").fetchone()
            payload = json.loads(row[1]); payload["state"]["source_text"] = "different source"
            db.execute("UPDATE attempts SET request=? WHERE id=?", (json.dumps(payload), row[0]))
        elif change == "attempt":
            db.execute("UPDATE attempts SET item_id='different-item' WHERE id=(SELECT MIN(id) FROM attempts)")
        else:
            db.execute("UPDATE candidates SET candidate_id='forged'")
    with pytest.raises(ValueError):
        review.read_candidates(prepared, path)


def test_old_configuration_cannot_be_reviewed_under_new_input(tmp_path):
    prepared, db = run_fixture(tmp_path)
    prepared["input_sha256"] = "0"*64
    with pytest.raises(ValueError, match="run_identity"):
        review.read_candidates(prepared, db)


def test_realization_cannot_import_pass_as_an_input_field(tmp_path):
    plan = json.loads((EXAMPLES / "cad-review-plan.json").read_text())
    plan["verified_functions"] = ["structure_support", "revision_applicability"]
    path = tmp_path / "plan.json"; path.write_bytes(encoded(plan))
    with pytest.raises(ValueError, match="missing or unknown"):
        review.review_plan(path, EXAMPLES, "unused", "unused", "actor", tmp_path / "review")


def fake_binding(*args):
    return {}


def test_missing_method_is_retained_and_not_counted_as_verified(tmp_path, monkeypatch):
    plan = json.loads((EXAMPLES / "cad-review-plan.json").read_text())
    for claim in plan["claims"]:
        claim["records"] = []
    path = tmp_path / "plan.json"; path.write_bytes(encoded(plan))
    monkeypatch.setattr(review, "_binding", fake_binding)
    calls = []
    def execute_record(path, *args, **kwargs):
        # The only expected execution is the coverage record, with empty verified list.
        record = json.loads(Path(path).read_text()); calls.append(record)
        assert record["method"] == "pattern-realization"
        return {"status":"blocked", "test_origin":"mock", "fact_admitted":False}
    monkeypatch.setattr(review, "execute_record", execute_record)
    result = review.review_plan(path, EXAMPLES, "unused", "unused", "actor", tmp_path / "review")
    assert result["semantic_reviewed_functions"] == [] and len(calls) == 1
    assert all(c["missing_methods"] for c in result["claims"])
    source = json.loads((tmp_path / "review/coverage-snapshot.json").read_text())
    assert source["fields"]["verified_functions"] == []


def test_record_for_different_claim_is_rejected_before_execution(tmp_path, monkeypatch):
    plan = json.loads((EXAMPLES / "cad-review-plan.json").read_text())
    plan["claims"][0]["claim"]["subject_revision"] = "different-revision"
    path = tmp_path / "plan.json"; path.write_bytes(encoded(plan))
    monkeypatch.setattr(review, "_binding", fake_binding)
    monkeypatch.setattr(review, "execute_record", lambda *a, **kw: pytest.fail("must not execute an unrelated record"))
    with pytest.raises(ValueError, match="claim_identity"):
        review.review_plan(path, EXAMPLES, "unused", "unused", "actor", tmp_path / "review")


def test_imported_realization_record_cannot_fill_leaf_coverage(tmp_path, monkeypatch):
    plan = json.loads((EXAMPLES / "cad-review-plan.json").read_text())
    item = plan["claims"][0]
    item["required_methods"] = ["pattern-realization"]
    record = {"claim":item["claim"], "method":"pattern-realization", "verified_functions":["anything"]}
    path = tmp_path / "record.json"; path.write_bytes(encoded(record))
    item["records"] = [{"path":path.name,"sha256":digest(path.read_bytes())}]
    pp = tmp_path / "plan.json"; pp.write_bytes(encoded(plan))
    monkeypatch.setattr(review, "_binding", fake_binding)
    with pytest.raises(ValueError, match="fresh_session_aggregation"):
        review.review_plan(pp, tmp_path, "unused", "unused", "actor", tmp_path / "review")


def test_actual_review_return_identity_must_match_request(tmp_path, monkeypatch):
    record = EXAMPLES / "review-current-support-identity_current.json"
    monkeypatch.setattr(review, "_binding", lambda *a:{"project":{"domain":"synthetic"},"evidence":{"logical_root":"evidence:test"}})
    monkeypatch.setattr(review.semantic_engagement, "review", lambda *a,**kw:{"execution":{"review":{"scope_id":"wrong-scope"}}})
    with pytest.raises(ValueError, match="actual_review_identity"):
        review.execute_record(record, EXAMPLES, "unused", "unused", "actor", tmp_path / "review", project_id="synthetic")


def test_impact_projection_keeps_physical_hypothesis_distinct_from_support():
    rdf, audit = review.project_impact(EXAMPLES / "impact-record.json", EXAMPLES)
    assert b'physicalHypothesisDependsOn' in rdf and b'supportDependsOn' in rdf
    assert audit['semantic_execution']=='not_run' and audit['projection_sha256']==digest(rdf)
    assert {node['id'] for node in audit['nodes'].values()}=={'geometry','interface-claim','process-choice','quote','schedule','temperature','unrelated'}


@pytest.mark.parametrize('change',['node','edge_kind','trigger','completeness'])
def test_impact_projection_refuses_unbound_or_unsupported_inputs(tmp_path,change):
    source=json.loads((EXAMPLES/'impact-source.json').read_text())
    if change=='node':source['edges'][0]['to']='missing-node'
    if change=='edge_kind':source['edges'][0]['kind']='physically_proves'
    if change=='trigger':source['triggers'][0]['kind']='approve_everything'
    if change=='completeness':source['inventory_complete']='true'
    path=tmp_path/'source.json';path.write_bytes(encoded(source))
    record={'schema':'ontology-engineering.support-impact/v1','project_id':'synthetic','source':{'path':path.name,'sha256':digest(path.read_bytes()),'pointer':''}}
    rp=tmp_path/'record.json';rp.write_bytes(encoded(record))
    with pytest.raises(ValueError):review.project_impact(rp,tmp_path)
