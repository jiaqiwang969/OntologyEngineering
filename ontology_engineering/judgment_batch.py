"""Resumable candidate-only batch execution; SQLite is a job journal, not an ontology.

The controller keeps input/version identity, failures and mandatory review work.
Formal interpretation and project reviews remain Semantica-owned.
"""
from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
from contextlib import contextmanager
from copy import deepcopy
from datetime import datetime, timezone
import fcntl
import json
import math
import os
from pathlib import Path
import sqlite3
import statistics
import threading
import time

from ontology_engineering.jev_transport import TransportError, strict_json, validate_response
from ontology_engineering.judgment_contracts import digest, encoded


def now():
    return datetime.now(timezone.utc).isoformat()


@contextmanager
def journal(path, project_id, access_scope):
    path = Path(path)
    path.parent.mkdir(parents=True,exist_ok=True)
    if path.is_symlink() or path.with_suffix(path.suffix+".lock").is_symlink():
        raise ValueError("journal_symlink")
    fd = os.open(str(path)+".lock",os.O_CREAT|os.O_RDWR|getattr(os,"O_NOFOLLOW",0),0o600)
    try:
        try:
            fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError("journal_already_running") from None
        connection = sqlite3.connect(path,check_same_thread=False)
        try:
            os.chmod(path,0o600)
            connection.row_factory=sqlite3.Row
            connection.execute("PRAGMA journal_mode=WAL")
            connection.execute("PRAGMA foreign_keys=ON")
            connection.executescript('''
              CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
              CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, input_sha TEXT NOT NULL, lock_sha TEXT NOT NULL,
                transport TEXT NOT NULL, manifest TEXT NOT NULL, started_at TEXT NOT NULL);
              CREATE TABLE IF NOT EXISTS attempts (id INTEGER PRIMARY KEY, run_id TEXT NOT NULL,
                item_id TEXT NOT NULL, request_sha TEXT NOT NULL, request TEXT NOT NULL, response TEXT,
                error TEXT, kind TEXT NOT NULL, elapsed REAL NOT NULL, created_at TEXT NOT NULL);
              CREATE TABLE IF NOT EXISTS candidates (run_id TEXT NOT NULL,item_id TEXT NOT NULL,
                question_id TEXT NOT NULL, candidate_id TEXT NOT NULL, answer TEXT NOT NULL,
                attempt_id INTEGER NOT NULL, PRIMARY KEY(run_id,item_id,question_id),
                FOREIGN KEY(attempt_id) REFERENCES attempts(id));
              CREATE TABLE IF NOT EXISTS cache (request_sha TEXT PRIMARY KEY,response TEXT NOT NULL,
                response_sha TEXT NOT NULL, origin_attempt INTEGER NOT NULL,
                FOREIGN KEY(origin_attempt) REFERENCES attempts(id));
            ''')
            identity=encoded({"schema":"judgment-journal/v1","project_id":project_id,"access_scope":access_scope}).decode()
            previous=connection.execute("SELECT value FROM metadata WHERE key='identity'").fetchone()
            if previous and previous[0]!=identity:
                raise ValueError("journal_project_or_access_scope_mismatch")
            connection.execute("INSERT OR IGNORE INTO metadata VALUES ('identity',?)",(identity,))
            connection.commit()
            yield connection
        finally:
            connection.close()
    finally:
        os.close(fd)


def _ask(transport, payload, limit, sleeper, on_start, on_finish):
    records=[]
    for attempt in range(limit):
        attempt_id=on_start()
        started=time.monotonic()
        response=None
        retry=False
        delay=0
        try:
            response=transport(deepcopy(payload))
            valid, errors=validate_response(response,payload)
            records.append(dict(response=response,valid=valid,errors=errors,error=None,
                                elapsed=time.monotonic()-started,created_at=now()))
        except TransportError as exc:
            records.append(dict(response=response if isinstance(response,dict) else None,valid={},errors={},error=exc.code,
                                elapsed=time.monotonic()-started,created_at=now()))
            retry=exc.retryable and attempt+1<limit
            delay=min(30,exc.retry_after if exc.retry_after is not None else 2**attempt)
        except Exception:
            # Unknown transport error text may contain headers or source content.
            records.append(dict(response=None,valid={},errors={},error="unexpected_transport_error",
                                elapsed=time.monotonic()-started,created_at=now()))
        on_finish(attempt_id,records[-1])
        if not retry:
            return records
        sleeper(max(0,delay))
    return records


def _save(db, run_id, item_id, request_sha, payload, record, kind, attempt_id=None):
    response=record["response"]
    raw_response=encoded(response).decode() if response is not None else None
    with db:
        error=record["error"] or (encoded(record["errors"]).decode() if record["errors"] else None)
        if attempt_id is None:
            cursor=db.execute("INSERT INTO attempts(run_id,item_id,request_sha,request,response,error,kind,elapsed,created_at) VALUES (?,?,?,?,?,?,?,?,?)",
                (run_id,item_id,request_sha,encoded(payload).decode(),raw_response,error,kind,record["elapsed"],record["created_at"]))
            aid=cursor.lastrowid
        else:
            cursor=db.execute("UPDATE attempts SET response=?,error=?,elapsed=? WHERE id=? AND run_id=? AND item_id=? AND request_sha=? AND error='started_unconfirmed'",
                              (raw_response,error,record["elapsed"],attempt_id,run_id,item_id,request_sha))
            if cursor.rowcount!=1:
                raise ValueError("attempt_completion_identity_mismatch")
            aid=attempt_id
        for qid,answer in record["valid"].items():
            cid=digest({"run":run_id,"item":item_id,"question":qid,"request":request_sha,"answer":answer})
            # First valid candidate is immutable, even if a later retry disagrees.
            db.execute("INSERT OR IGNORE INTO candidates VALUES (?,?,?,?,?,?)",
                       (run_id,item_id,qid,cid,encoded(answer).decode(),aid))
        if response is not None and not record["error"] and not record["errors"] and kind!="replay":
            db.execute("INSERT OR IGNORE INTO cache VALUES (?,?,?,?)",
                       (request_sha,raw_response,digest(response),aid))


def _answers(db,run_id,item_id):
    return {r["question_id"]:strict_json(r["answer"]) for r in db.execute(
        "SELECT question_id,answer FROM candidates WHERE run_id=? AND item_id=?",(run_id,item_id))}


def execute(prepared, journal_path, transport, *, sleeper=time.sleep, experiment=False,
            compatibility_catalog=None, compatibility_root=None):
    """Run independent questions concurrently and dependent questions in stages.

    Reusing a batch ID requires byte-equivalent ordered inputs and a complete
    matching deployment lock. Completed answers survive partial failure/resume.
    """
    # Freeze the in-flight run even if the caller constructs a successor config
    # while requests are pending. A changed file is read only by a new prepare.
    prepared=deepcopy(prepared)
    started=time.monotonic()
    lock,catalog=prepared["deployment"],prepared["catalog"]
    manifest=prepared["input"]
    run_id=manifest["batch_id"]
    kind=getattr(transport,"kind",None)
    identity=getattr(transport,"identity",None)
    if kind not in ("live","fixture") or not isinstance(identity,str) or not identity:
        raise ValueError("transport_identity_required")
    from ontology_engineering.judgment_compatibility import run_admission
    admission = run_admission(prepared, execution_kind=kind, experiment=experiment,
                              catalog=compatibility_catalog, catalog_root=compatibility_root)
    with journal(journal_path,manifest["project_id"],manifest["access_scope"]) as db:
        expected=(prepared["input_sha256"],prepared["deployment_sha256"],identity)
        prior=db.execute("SELECT input_sha,lock_sha,transport,manifest FROM runs WHERE id=?",(run_id,)).fetchone()
        if prior and (tuple(prior)[:3]!=expected or strict_json(prior["manifest"]).get("run_admission") != admission):
            raise ValueError("batch_resume_identity_mismatch")
        with db:
            db.execute("INSERT OR IGNORE INTO runs VALUES (?,?,?,?,?,?)",
                       (run_id,*expected,encoded({"input":manifest,"deployment":lock,"run_admission":admission}).decode(),now()))
        max_stage=max(len(item["stages"]) for item in prepared["items"])
        dependencies=catalog.get("dependencies",{})
        database_lock=threading.Lock()
        def perform(job):
            iid,key,payload=job
            def on_start():
                with database_lock,db:
                    cursor=db.execute("INSERT INTO attempts(run_id,item_id,request_sha,request,response,error,kind,elapsed,created_at) VALUES (?,?,?,?,NULL,'started_unconfirmed',?,0,?)",
                                      (run_id,iid,key,encoded(payload).decode(),kind,now()))
                    return cursor.lastrowid
            def on_finish(aid,record):
                with database_lock:
                    _save(db,run_id,iid,key,payload,record,kind,attempt_id=aid)
            return _ask(transport,payload,lock["max_attempts"],sleeper,on_start,on_finish)
        for stage in range(max_stage):
            jobs=[]
            for item in prepared["items"]:
                if stage>=len(item["stages"]):
                    continue
                known=_answers(db,run_id,item["id"])
                selected=[q for q in item["stages"][stage] if q not in known]
                if not selected:
                    continue
                needs={dep for q in selected for dep in dependencies.get(q,[])}
                state=deepcopy(item["state"])
                if needs:
                    if not needs.issubset(known):
                        # No fabricated answer or stage execution. Export stays incomplete.
                        continue
                    state["prior_answers"]={q:known[q] for q in sorted(needs)}
                groups=[selected] if lock["strategy"]=="batched" else [[q] for q in selected]
                for group in groups:
                    payload={"model":lock["model"],"state":state,
                             "questions":{q:catalog["questions"][q] for q in group}}
                    key=digest({"project":manifest["project_id"],"access_scope":manifest["access_scope"],
                                "source":item["source"],"claim":item["claim"],"lock":lock,
                                "transport":identity,"payload":payload})
                    cached=db.execute("SELECT response,response_sha FROM cache WHERE request_sha=?",(key,)).fetchone()
                    if cached:
                        response=strict_json(cached["response"])
                        if digest(response)!=cached["response_sha"]:
                            raise ValueError("cache_response_integrity_failure")
                        valid,errors=validate_response(response,payload)
                        if errors:
                            raise ValueError("cache_response_contract_failure")
                        _save(db,run_id,item["id"],key,payload,
                              dict(response=response,valid=valid,errors={},error=None,elapsed=0,created_at=now()),"replay")
                    else:
                        jobs.append((item["id"],key,payload))
            # Bound in-flight work instead of submitting the whole corpus at once.
            with ThreadPoolExecutor(max_workers=lock["workers"]) as pool:
                pending={}
                iterator=iter(jobs)
                exhausted=False
                while pending or not exhausted:
                    while len(pending)<lock["workers"] and not exhausted:
                        try:
                            job=next(iterator)
                        except StopIteration:
                            exhausted=True
                            break
                        future=pool.submit(perform,job)
                        pending[future]=job
                    if not pending:
                        break
                    done,_=wait(pending,return_when=FIRST_COMPLETED)
                    for future in done:
                        pending.pop(future)
                        future.result()
        return report(db,prepared,wall_elapsed=time.monotonic()-started)


def report(db, prepared, *, wall_elapsed=0):
    run_id=prepared["input"]["batch_id"]
    run_metadata = db.execute("SELECT manifest FROM runs WHERE id=?", (run_id,)).fetchone()
    admission = strict_json(run_metadata["manifest"]).get("run_admission") if run_metadata else None
    unknowns=set(prepared["catalog"]["unknown_choices"])
    items=[]
    totals={"candidate":0,"unknown":0,"execution_error":0,"pending":0}
    for item in prepared["items"]:
        candidates={r["question_id"]:r for r in db.execute(
            "SELECT c.*,a.kind,a.request_sha FROM candidates c JOIN attempts a ON a.id=c.attempt_id WHERE c.run_id=? AND c.item_id=?",
            (run_id,item["id"]))}
        attempts=list(db.execute("SELECT request,error FROM attempts WHERE run_id=? AND item_id=?",(run_id,item["id"])))
        attempted={q for row in attempts for q in strict_json(row["request"])["questions"]}
        answers=[]
        for q in item["question_ids"]:
            row=candidates.get(q)
            if row is None:
                status="execution_error" if q in attempted else "pending"
                answers.append({"question_id":q,"status":status,"fact_admitted":False})
            else:
                answer=strict_json(row["answer"])
                status="unknown" if answer["choice"] in unknowns else "candidate"
                answers.append({"question_id":q,"status":status,"candidate_id":row["candidate_id"],
                                "request_sha256":row["request_sha"],"attempt_id":row["attempt_id"],
                                "execution_kind":row["kind"],"answer":answer,"fact_admitted":False})
            totals[status]+=1
        items.append({"id":item["id"],"source":item["source"],"claim":item["claim"],"answers":answers,
                      "required_methods":item["required_methods"],"review_status":"not_run",
                      "context_status":item["source"]["context_status"],
                      "audit_sample":int(digest({"source":item["source"],"claim":item["claim"]})[:8],16)%10==0})
    # Inspect high-confidence and unrelated outputs as well as difficult ones.
    # Confidence bands are audit strata, never correctness or admission thresholds.
    strata = {}
    for item in items:
        item["audit_questions"] = []
        item["routing_reasons"] = []
        if item["context_status"] != "complete_for_question":
            item["routing_reasons"].append("source_context_incomplete")
        for answer in item["answers"]:
            if answer["status"] in {"unknown", "execution_error", "pending"}:
                item["routing_reasons"].append(answer["question_id"]+":"+answer["status"])
                continue
            judgment = answer["answer"]
            if judgment["choice"] in {"contradicts", "broadened", "different", "changed"}:
                item["routing_reasons"].append(answer["question_id"]+":candidate_challenge")
            key = (item["claim"].get("domain", "undeclared"), answer["question_id"], judgment["choice"],
                   "high_score" if judgment["confidence"] >= 0.9 else "other_score")
            strata.setdefault(key, []).append((digest({"source":item["source"], "claim":item["claim"], "question":answer["question_id"]}), item, answer["question_id"]))
    for members in strata.values():
        for _, item, qid in sorted(members, key=lambda x:(x[0],x[1]["id"]))[:max(1,math.ceil(len(members)*0.1))]:
            item["audit_questions"].append(qid)
    for item in items:
        item["audit_sample"] = bool(item["audit_questions"])
        if item["audit_sample"]:
            item["routing_reasons"].append("stratified_candidate_audit")
        item["routing"] = "review_queue" if item["routing_reasons"] else "candidate_backlog"
    attempts=list(db.execute("SELECT kind,response,error,elapsed FROM attempts WHERE run_id=?",(run_id,)))
    elapsed=[a["elapsed"] for a in attempts if a["kind"]!="replay"]
    usage={"input_tokens":0,"output_tokens":0,"unreported_attempts":0}
    for row in attempts:
        if row["kind"]=="replay":
            continue
        values=strict_json(row["response"]).get("usage",{}) if row["response"] else {}
        if not isinstance(values,dict):
            values={}
        values={k:v for k,v in values.items() if type(v) is int and v>=0}
        if any(values.get(k) is None for k in ("input_tokens","output_tokens")):
            usage["unreported_attempts"]+=1
        for k in ("input_tokens","output_tokens"):
            usage[k]+=values.get(k) or 0
    expected=sum(len(i["question_ids"]) for i in prepared["items"])
    if sum(totals.values())!=expected:
        raise AssertionError("question_accounting_failure")
    return {"schema":"ontology-engineering.judgment-batch-result/v1","batch_id":run_id,
            "project_id":prepared["input"]["project_id"],"access_scope":prepared["input"]["access_scope"],
            "input_sha256":prepared["input_sha256"],"deployment_sha256":prepared["deployment_sha256"],
            "run_admission":admission,
            "status":"completed" if totals["pending"]+totals["execution_error"]==0 else "incomplete",
            "counts":totals,"expected_questions":expected,"items":items,
            "attempts":len(attempts),"live_attempts":sum(a["kind"]=="live" for a in attempts),
            "unconfirmed_attempts":sum(a["error"]=="started_unconfirmed" for a in attempts),
            "fixture_attempts":sum(a["kind"]=="fixture" for a in attempts),
            "replays":sum(a["kind"]=="replay" for a in attempts),"usage":usage,
            "wall_elapsed_this_invocation_s":wall_elapsed,
            "attempt_latency_median_s":statistics.median(elapsed) if elapsed else None,
            "attempt_latency_p95_s":sorted(elapsed)[max(0,int(len(elapsed)*0.95+0.999999)-1)] if elapsed else None,
            "semantic_review":"not_run","fact_admission":"not_performed","ontology_learning":"not_assessed",
            "lineage":{"declared_groups":len({i["source"]["lineage_group"] for i in items}),
                       "records":len(items),"independence_verified":False},
            "review_queue":[{"item_id":i["id"],"claim":i["claim"],"source":i["source"],
                             "reasons":i["routing_reasons"],"audit_questions":i["audit_questions"],
                             "required_methods":i["required_methods"],"fact_admitted":False}
                            for i in items if i["routing"]=="review_queue"],
            "meaning":"Candidate annotations only. Completed processing does not mean evidence obligations or physical claims are satisfied."}
