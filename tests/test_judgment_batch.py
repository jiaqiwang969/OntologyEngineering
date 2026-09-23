"""Behavioral tests for execution failures, identity and candidate-only boundaries.

Fixture transport is explicitly recorded; these tests do not evaluate model quality.
"""
from copy import deepcopy
import json
import sqlite3
import threading

import pytest

from ontology_engineering.jev_transport import TransportError,validate_response
from ontology_engineering.judgment_contracts import digest,question_stages
from ontology_engineering.judgment_batch import execute


QUESTION={"type":"choice","instructions":"Does the supplied source state the claim?",
          "criteria":{"supports":"Explicit same-scope assertion","not_established":"Insufficient information"}}


def answer(choice="supports"):
    return {"type":"choice","choice":choice,"confidence":1.0,
            "probabilities":{k:float(k==choice) for k in QUESTION["criteria"]}}


def prepared(batch_id="batch-A",items=2,strategy="batched"):
    manifest={"batch_id":batch_id,"project_id":"synthetic-project","access_scope":"synthetic-evaluation"}
    data=[]
    for i in range(items):
        data.append({"id":f"item-{i}","source":{"id":f"source-{i}","sha256":digest(str(i).encode()),"lineage_group":"shared-origin","context_status":"partial"},
                     "claim":{"id":f"claim-{i}","subject_revision":"rev-A","statement":f"statement-{i}"},
                     "state":{"source_text":f"synthetic-{i}","claim":{"id":f"claim-{i}"}},
                     "question_ids":["q1","q2"],"stages":[["q1","q2"]],
                     "required_methods":["pattern-claim","pattern-scope"]})
    lock={"model":"jev-1.13.0","strategy":strategy,"workers":2,"max_attempts":2}
    return {"input":manifest,"items":data,"deployment":lock,"input_sha256":digest({"manifest":manifest,"items":data}),
            "deployment_sha256":digest(lock),"catalog":{"questions":{"q1":QUESTION,"q2":QUESTION},"unknown_choices":["not_established"]}}


class Fixture:
    identity="test-fixture/v1"
    kind="fixture"
    def __init__(self,handler=None):
        self.handler=handler
        self.calls=[]
        self.lock=threading.Lock()
    def __call__(self,payload):
        with self.lock:
            self.calls.append(deepcopy(payload))
            number=len(self.calls)
        if self.handler:
            return self.handler(payload,number)
        return {"model":payload["model"],"answers":{q:answer() for q in payload["questions"]},"usage":{"input_tokens":20,"output_tokens":2}}


def response(payload,answers):
    return {"model":payload["model"],"answers":answers,"usage":{"input_tokens":20,"output_tokens":2}}


def test_out_of_order_answers_remain_bound_to_source(tmp_path):
    def handler(p,n):
        choice="not_established" if p["state"]["claim"]["id"]=="claim-0" else "supports"
        return response(p,{q:answer(choice) for q in reversed(p["questions"])})
    r=execute(prepared(),tmp_path/"jobs.sqlite",Fixture(handler))
    assert r["counts"]==dict(candidate=2,unknown=2,execution_error=0,pending=0)
    assert r["items"][0]["answers"][0]["answer"]["choice"]=="not_established"
    assert r["live_attempts"]==0 and r["fixture_attempts"]==2
    assert all(not a["fact_admitted"] for i in r["items"] for a in i["answers"])
    assert r["items"][0]["required_methods"]==["pattern-claim","pattern-scope"]


def test_partial_response_resume_preserves_first_valid_candidate(tmp_path):
    transport=Fixture(lambda p,n:response(p,{"q1":answer()}))
    p=prepared(items=1);db=tmp_path/"jobs.sqlite"
    first=execute(p,db,transport)
    assert first["status"]=="incomplete" and first["counts"]["execution_error"]==1
    original=first["items"][0]["answers"][0]["candidate_id"]
    second_transport=Fixture()
    second=execute(p,db,second_transport)
    assert list(second_transport.calls[0]["questions"])==["q2"]
    assert second["status"]=="completed"
    assert second["items"][0]["answers"][0]["candidate_id"]==original
    third=execute(p,db,Fixture())
    assert third["attempts"]==2


def test_rate_limit_retry_does_not_duplicate_candidates(tmp_path):
    def handler(p,n):
        if n==1:
            raise TransportError("http_429",retryable=True,retry_after=2)
        return response(p,{q:answer() for q in p["questions"]})
    delays=[]
    r=execute(prepared(items=1),tmp_path/"jobs.sqlite",Fixture(handler),sleeper=delays.append)
    assert delays==[2] and r["attempts"]==2 and r["counts"]["candidate"]==2
    assert r["usage"]["unreported_attempts"]==1


def test_error_is_neither_unknown_nor_unrelated(tmp_path):
    def failure(p,n):
        raise TransportError("http_529",retryable=True)
    r=execute(prepared(items=1),tmp_path/"jobs.sqlite",Fixture(failure),sleeper=lambda _:None)
    assert r["counts"]==dict(candidate=0,unknown=0,execution_error=2,pending=0)
    assert r["attempts"]==2 and r["status"]=="incomplete"


def test_in_flight_caller_update_does_not_mix_snapshots(tmp_path):
    p=prepared(items=8);p['deployment']['workers']=1
    def handler(payload,n):
        if n==1:
            p['deployment']['model']='jev-99.0.0'
            p['catalog']['questions']['q1']['instructions']='new question'
        assert payload['model']=='jev-1.13.0'
        assert payload['questions']['q1']['instructions']!= 'new question'
        return response(payload,{q:answer() for q in payload['questions']})
    r=execute(p,tmp_path/'jobs.sqlite',Fixture(handler))
    assert r['counts']['candidate']==16


def test_high_score_and_unrelated_candidates_are_audited_with_obligations(tmp_path):
    p=prepared(items=40)
    p['catalog']['questions']={q:{'type':'choice','instructions':'synthetic relation',
        'criteria':{'related':'related','unrelated':'unrelated','not_established':'unknown'}} for q in ['q1','q2']}
    def handler(payload,n):
        return response(payload,{q:{'type':'choice','choice':'unrelated','confidence':1.0,
            'probabilities':{'related':0.0,'unrelated':1.0,'not_established':0.0}} for q in payload['questions']})
    r=execute(p,tmp_path/'jobs.sqlite',Fixture(handler))
    audit=[i for i in r['items'] if i['audit_sample']]
    assert audit and all(i['required_methods']==['pattern-claim','pattern-scope'] for i in audit)
    assert r['lineage']=={'declared_groups':1,'records':40,'independence_verified':False}


def test_large_queue_is_bounded_and_accounts_for_every_declared_question(tmp_path):
    import time
    p=prepared(items=512);p['deployment']['workers']=4
    active=0;maximum=0;guard=threading.Lock()
    def handler(payload,n):
        nonlocal active,maximum
        with guard:
            active+=1;maximum=max(maximum,active)
        time.sleep(0.001)
        with guard:active-=1
        return response(payload,{q:answer() for q in payload['questions']})
    r=execute(p,tmp_path/'jobs.sqlite',Fixture(handler))
    assert 1 <= maximum <= 4
    assert r['expected_questions']==1024 and r['counts']==dict(candidate=1024,unknown=0,execution_error=0,pending=0)
    assert r['fixture_attempts']==512 and r['live_attempts']==0


@pytest.mark.parametrize("change",["model","input","transport"])
def test_resume_rejects_changed_identity(tmp_path,change):
    p=prepared(items=1);db=tmp_path/"jobs.sqlite";execute(p,db,Fixture())
    other=deepcopy(p);transport=Fixture()
    if change=="model":
        other["deployment"]["model"]="jev-1.14.0";other["deployment_sha256"]=digest(other["deployment"])
    elif change=="input":
        other["input_sha256"]=digest(b"changed source")
    else:
        transport.identity="other-transport/v1"
    with pytest.raises(ValueError,match="resume_identity"):
        execute(other,db,transport)
    assert not transport.calls


def test_cache_replay_is_explicit_and_project_isolated(tmp_path):
    db=tmp_path/"jobs.sqlite";execute(prepared(items=1),db,Fixture())
    transport=Fixture();r=execute(prepared("batch-B",items=1),db,transport)
    assert not transport.calls and r["replays"]==1 and r["usage"]["input_tokens"]==0
    p=prepared("batch-C",items=1);p["input"]["project_id"]="another-project"
    with pytest.raises(ValueError,match="project_or_access"):
        execute(p,db,Fixture())


def test_tampered_cache_is_rejected(tmp_path):
    db=tmp_path/"jobs.sqlite";execute(prepared(items=1),db,Fixture())
    with sqlite3.connect(db) as con:
        con.execute("UPDATE cache SET response_sha='broken'")
    with pytest.raises(ValueError,match="cache_response_integrity"):
        execute(prepared("batch-B",items=1),db,Fixture())


def test_question_dependencies_use_separate_calls(tmp_path):
    p=prepared(items=1)
    p["items"][0]["stages"]=[["q1"],["q2"]]
    p["catalog"]["dependencies"]={"q2":["q1"]}
    transport=Fixture();r=execute(p,tmp_path/"jobs.sqlite",transport)
    assert r["status"]=="completed" and len(transport.calls)==2
    assert "prior_answers" not in transport.calls[0]["state"]
    assert transport.calls[1]["state"]["prior_answers"]["q1"]==answer()


def test_dependency_failure_leaves_followup_pending(tmp_path):
    p=prepared(items=1);p["items"][0]["stages"]=[["q1"],["q2"]];p["catalog"]["dependencies"]={"q2":["q1"]}
    def failure(p,n):
        raise TransportError("http_401")
    transport=Fixture(failure);r=execute(p,tmp_path/"jobs.sqlite",transport)
    assert len(transport.calls)==1
    assert r["counts"]==dict(candidate=0,unknown=0,execution_error=1,pending=1)


def test_dependency_cycles_and_missing_parents_are_rejected():
    with pytest.raises(ValueError,match="cycle"):
        question_stages(["a","b"],{"a":["b"],"b":["a"]})
    with pytest.raises(ValueError,match="missing"):
        question_stages(["b"],{"b":["a"]})
    assert question_stages(["b","a","c"],{"b":["a"]})==[["a","c"],["b"]]


def test_interrupted_attempt_is_preserved_on_resume(tmp_path):
    def interrupted(p,n):
        raise SystemExit("synthetic interruption")
    p=prepared(items=1);db=tmp_path/"jobs.sqlite"
    with pytest.raises(SystemExit):
        execute(p,db,Fixture(interrupted))
    with sqlite3.connect(db) as con:
        assert con.execute("SELECT error FROM attempts").fetchone()[0]=="started_unconfirmed"
    r=execute(p,db,Fixture())
    assert r["status"]=="completed" and r["unconfirmed_attempts"]==1
    assert r["attempts"]==2 and r["usage"]["unreported_attempts"]==1


@pytest.mark.parametrize("mutation",["nan","negative","wrong_choice","wrong_option","wrong_type"])
def test_malformed_answer_is_not_a_candidate(mutation):
    p={"model":"jev-1.13.0","questions":{"q":QUESTION}}
    a=answer()
    if mutation=="nan": a["confidence"]=float("nan")
    if mutation=="negative": a["probabilities"]["supports"]=-1
    if mutation=="wrong_choice": a["choice"]="not_established"
    if mutation=="wrong_option": a["probabilities"]["approve"]=0
    if mutation=="wrong_type": a["type"]="noul"
    valid,errors=validate_response(response(p,{"q":a}),p)
    assert not valid and "q" in errors


def test_resolved_model_mismatch_fails_whole_request():
    p={"model":"jev-1.13.0","questions":{"q":QUESTION}}
    r=response(p,{"q":answer()});r["model"]="jev-latest"
    with pytest.raises(TransportError,match="resolved_model"):
        validate_response(r,p)
