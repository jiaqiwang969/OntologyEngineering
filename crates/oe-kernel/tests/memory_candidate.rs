//! Independent, frozen expectations for memory recall candidates (oev2.memory-candidate-check/1).
use oe_contracts::{ArtifactReference, CapabilityRegistry, ObjectIdentity};
use oe_core::{SkillRoot, sha256};
use oe_kernel::project::{Project, context, memory, tool_result, validate};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const USER_LINE: &str = r#"{"timestamp":"2026-10-21T00:00:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"按这个方法测"}]}}"#;
const ASSISTANT_LINE: &str = r#"{"timestamp":"2026-10-21T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"先固定基线"}]}}"#;

struct Fixture {
    _dir: tempfile::TempDir,
    _history: tempfile::TempDir,
    root: SkillRoot,
    transcript: PathBuf,
    project: Value,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("SKILL.md"), "fixture").unwrap();
    let history = tempfile::tempdir().unwrap();
    let transcript = fs::canonicalize(history.path())
        .unwrap()
        .join("rollout.jsonl");
    fs::write(&transcript, format!("{USER_LINE}\n{ASSISTANT_LINE}\n")).unwrap();
    let root = SkillRoot::new(fs::canonicalize(dir.path()).unwrap()).unwrap();
    let project = json!({"schema": "oev2.project/1", "project_id": "p", "revision": "r1", "purpose": "memory fixture",
        "objects": {"obj": "rev-1"}, "conditions": {"cond": "c"}, "evidence": {"src": sha256(b"source")},
        "decisions": [{"id": "D1", "object_id": "obj", "object_revision": "rev-1", "condition_scope": "cond",
                       "source_sha256": sha256(b"source"), "conclusion": "unknown", "adopted": false, "basis": []}],
        "dependencies": {}});
    Fixture {
        _dir: dir,
        _history: history,
        root,
        transcript,
        project,
    }
}

fn anchors(f: &Fixture) -> (String, Vec<Value>) {
    // Sorted-key compact text written by hand: the independent oracle for memory's record digest.
    let p = f.transcript.to_str().unwrap();
    let (u, a) = (format!("{USER_LINE}\n"), format!("{ASSISTANT_LINE}\n"));
    let (u0, u1) = (0, u.len());
    let (a0, a1) = (u.len(), u.len() + a.len());
    let canon = |s: usize, e: usize, digest: &str, role: &str, text: &str, at: &str| {
        format!(
            r#"{{"byte_end":{e},"byte_start":{s},"event_at":"{at}","message_id":null,"parent_id":null,"profile":null,"provider":"codex","role":"{role}","session_id":"s1","sha256":"{digest}","source_path":"{p}","text":"{text}"}}"#
        )
    };
    let texts = [
        canon(
            u0,
            u1,
            &sha256(u.as_bytes()),
            "user",
            "按这个方法测",
            "2026-10-21T00:00:00.000Z",
        ),
        canon(
            a0,
            a1,
            &sha256(a.as_bytes()),
            "assistant",
            "先固定基线",
            "2026-10-21T00:00:01.000Z",
        ),
    ];
    let record = format!(
        r#"{{"causal_claims":[],"conditions":["c"],"id":"exp-1","method":null,"project_id":"p","purpose":"固定基线后再比较","recorded_at":"2026-10-21T00:00:02Z","revision":2,"schema":"engineering-memory.experience/2","sources":[{},{}],"status":"reviewed","supersedes":1,"timeline":[],"unknowns":["收益未测"]}}"#,
        texts[0], texts[1]
    );
    let values = texts
        .iter()
        .map(|t| serde_json::from_str(t).unwrap())
        .collect();
    (record, values)
}

/// The handoff with the record's keys deliberately out of order (memory sorts them before hashing).
fn handoff(f: &Fixture) -> Value {
    let (canonical, sources) = anchors(f);
    let record = json!({"status": "reviewed", "id": "exp-1", "schema": "engineering-memory.experience/2", "revision": 2,
        "sources": sources, "purpose": "固定基线后再比较", "unknowns": ["收益未测"], "conditions": ["c"], "timeline": [],
        "causal_claims": [], "method": null, "project_id": "p", "recorded_at": "2026-10-21T00:00:02Z", "supersedes": 1});
    let p: Project = serde_json::from_value(f.project.clone()).unwrap();
    let flags = |mut v: Value| {
        v["project_write"] = json!(false);
        v["engineering_adoption"] = json!("not_granted");
        v["rrsi_validation"] = json!("not_run");
        v["required_next"] = json!("verify in v2");
        v
    };
    let candidate = flags(
        json!({"schema": "engineering-memory.v2-candidate/1", "record": record,
        "record_sha256": sha256(canonical.as_bytes()), "target_project": "p",
        "semantic_review": {"scope_status": "review_required", "reasons": ["conditions_unconfirmed"], "missing_conditions": ["m"]},
        "feedback": [], "source_captures": []}),
    );
    flags(
        json!({"schema": "engineering-memory.session-handoff/1", "context_id": "ctx-1", "context_sha256": "c".repeat(64),
        "session_id": "s1", "task": "t", "candidate": candidate,
        "project": {"path": "/abs/project.json", "project_id": "p", "revision": p.revision, "sha256": "0".repeat(64),
                    "object": {"object_id": "obj", "object_revision": "rev-1", "condition_scope": "cond", "condition_sha256": sha256(b"c")}}}),
    )
}

fn put(f: &Fixture, rel: &str, v: &Value) -> ArtifactReference {
    let bytes = serde_json::to_vec_pretty(v).unwrap();
    let p = f.root.path().join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, &bytes).unwrap();
    ArtifactReference {
        path: rel.into(),
        sha256: sha256(&bytes),
    }
}

fn check(f: &Fixture, h: &Value, name: &str) -> oe_contracts::MemoryCandidateCheck {
    let art = put(f, &format!("var/m/{name}.json"), h);
    let p: Project = serde_json::from_value(f.project.clone()).unwrap();
    memory::verify(&f.root, &p, &"0".repeat(64), &art).unwrap()
}

fn redigest(h: &mut Value) {
    h["candidate"]["record_sha256"] =
        json!(memory::record_digest(&h["candidate"]["record"]).unwrap());
}

#[test]
fn native_handoff_is_verified_and_registered_as_one_evidence_item() {
    let f = fixture();
    let h = handoff(&f);
    let (canonical, _) = anchors(&f);
    let c = check(&f, &h, "ok");
    assert_eq!(c.status, "accepted", "{:?}", c.failures);
    assert_eq!(
        (c.sources_verified, c.project_binding.as_str()),
        (2, "matched")
    );
    assert_eq!(
        c.record_sha256.as_deref(),
        Some(sha256(canonical.as_bytes()).as_str())
    );
    assert_eq!(c.evidence_id.as_deref(), Some("memory-candidate:exp-1@r2"));
    assert_eq!(
        (c.memory_scope.as_deref(), c.missing_conditions.clone()),
        (Some("review_required"), vec!["m".to_string()])
    );
    let art = put(&f, "var/m/registered.json", &h);
    let pb = serde_json::to_vec(&f.project).unwrap();
    let (out, reg) = memory::register(&f.root, &pb, &art, "r2").unwrap();
    assert_eq!(reg.status, "registered", "{:?}", reg.failures);
    let next: Value = serde_json::from_slice(&out.unwrap()).unwrap();
    let mut expected = f.project.clone();
    expected["revision"] = json!("r2");
    expected["evidence"]["memory-candidate:exp-1@r2"] = json!(art.sha256);
    assert_eq!(next, expected);
    let (none, again) =
        memory::register(&f.root, &serde_json::to_vec(&next).unwrap(), &art, "r3").unwrap();
    assert!(
        none.is_none()
            && again
                .failures
                .contains(&"candidate_already_registered".to_string())
    );
    let (none, same) = memory::register(&f.root, &pb, &art, "r1").unwrap();
    assert!(
        none.is_none()
            && same.status == "refused"
            && same
                .failures
                .contains(&"project_revision_not_new".to_string())
    );
}

#[test]
fn changed_or_overclaiming_handoffs_are_rejected() {
    let f = fixture();
    let mut cases: Vec<(Value, &str)> = Vec::new();
    let mut h = handoff(&f);
    h["candidate"]["record"]["purpose"] = json!("改过的目的");
    cases.push((h, "record_digest_mismatch"));
    let mut h = handoff(&f);
    h["candidate"]["record"]["sources"][0]["text"] = json!("没说过的话");
    redigest(&mut h);
    cases.push((h, "source_text_mismatch:0"));
    let mut h = handoff(&f);
    h["candidate"]["engineering_adoption"] = json!("granted");
    cases.push((h, "candidate_claims_authority"));
    let mut h = handoff(&f);
    h["project_write"] = json!(true);
    cases.push((h, "candidate_claims_authority"));
    let mut h = handoff(&f);
    h["candidate"]["record"]["status"] = json!("rejected");
    redigest(&mut h);
    cases.push((h, "record_rejected"));
    let mut h = handoff(&f);
    h["project"]["object"]["object_revision"] = json!("rev-0");
    cases.push((h, "project_binding:object_changed"));
    let mut h = handoff(&f);
    h["project"]["object"]["condition_sha256"] = json!(sha256(b"other"));
    cases.push((h, "project_binding:condition_changed"));
    let mut h = handoff(&f);
    h["candidate"]["record"]["sources"] = json!([]);
    redigest(&mut h);
    cases.push((h, "sources_missing"));
    for (i, (h, code)) in cases.into_iter().enumerate() {
        let c = check(&f, &h, &format!("case{i}"));
        assert_eq!(c.status, "rejected", "{code}");
        assert!(
            c.failures.iter().any(|x| x == code),
            "{code}: {:?}",
            c.failures
        );
    }
    // The transcript itself changes after the handoff (same length, one byte differs).
    let changed = format!(
        "{USER_LINE}\n{}\n",
        ASSISTANT_LINE.replace("先固定基线", "先固定基准")
    );
    fs::write(&f.transcript, changed).unwrap();
    let c = check(&f, &handoff(&f), "changed-transcript");
    assert!(
        c.failures.contains(&"source_changed:1".to_string()),
        "{:?}",
        c.failures
    );
}

#[test]
fn bare_candidate_is_unbound_and_a_foreign_target_is_refused() {
    let f = fixture();
    let bare = handoff(&f)["candidate"].clone();
    let c = check(&f, &bare, "bare");
    assert_eq!(
        (c.status.as_str(), c.project_binding.as_str()),
        ("accepted", "unbound"),
        "{:?}",
        c.failures
    );
    assert!(c.warnings.contains(&"project_unbound".to_string()));
    let mut foreign = bare;
    foreign["target_project"] = json!("other-project");
    let c = check(&f, &foreign, "foreign");
    assert!(
        c.failures
            .contains(&"project_binding:project_mismatch".to_string()),
        "{:?}",
        c.failures
    );
}

#[test]
fn memory_evidence_never_supports_a_decision_or_stands_in_for_a_tool_record() {
    for id in ["memory-candidate:exp-1@r2", "memory-0123456789abcdef-001"] {
        assert!(memory::is_memory_evidence(id), "{id}");
        for (conclusion, ok) in [("supported", false), ("refuted", false), ("unknown", true)] {
            let f = fixture();
            let mut p = f.project.clone();
            p["evidence"][id] = json!("a".repeat(64));
            p["decisions"][0]["conclusion"] = json!(conclusion);
            p["decisions"][0]["basis"] = json!([id]);
            let p: Project = serde_json::from_value(p).unwrap();
            let r = validate(&p);
            assert_eq!(r.is_ok(), ok, "{id} {conclusion}");
            if !ok {
                assert!(
                    r.unwrap_err()
                        .to_string()
                        .contains("project_memory_evidence_not_decisive")
                );
            }
        }
    }
    for id in [
        "memory-layout",
        "memory-0123456789abcdef-x",
        "tool-result:memory-0123456789abcdef-001",
        "src",
    ] {
        assert!(!memory::is_memory_evidence(id), "{id}");
    }
    // A registered memory handoff cannot re-enter as a tool record.
    let f = fixture();
    let art = put(&f, "var/m/registered.json", &handoff(&f));
    let (out, _) = memory::register(
        &f.root,
        &serde_json::to_vec(&f.project).unwrap(),
        &art,
        "r2",
    )
    .unwrap();
    let project: Project = serde_json::from_slice(&out.unwrap()).unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = fs::canonicalize(tools.path()).unwrap().join("faketool");
    fs::write(&tool, b"#!/bin/sh\necho fake\n").unwrap();
    let registry: CapabilityRegistry = serde_json::from_value(json!({"schema": "oev2.capability-registry/1", "capabilities": [{
        "name": "faketool", "display": "FakeTool", "skill": "faketool", "role": "fixture", "outputs": [],
        "version": "1.0", "executable": tool.to_str().unwrap(), "executable_sha256": sha256(b"#!/bin/sh\necho fake\n"),
        "invocation": ["cli"], "acceptance": "fixture", "acceptance_sha256": sha256(b"a"), "status": "qualified", "limits": []}]})).unwrap();
    let ctx = context(
        &project,
        &ObjectIdentity {
            object_id: "obj".into(),
            revision: "rev-1".into(),
            condition_scope: "cond".into(),
            source_sha256: sha256(b"source"),
        },
    )
    .unwrap();
    let result = json!({"schema": "oev2.tool-result/1", "result_id": "R1", "project": ctx, "purpose": "p", "decisions": ["D1"], "acceptance": [],
        "tool": {"name": "faketool", "version": "1.0", "executable_sha256": sha256(b"#!/bin/sh\necho fake\n"), "skill": "faketool", "invocation": "cli"},
        "subject": {"kind": "none", "identity": "none"}, "inputs": {"arguments": [], "conditions": [], "environment": [], "files": []},
        "execution": {"status": "completed", "started_at": "2026-10-21T01:00:00Z", "finished_at": "2026-10-21T01:00:01Z", "cleanup": "exited"},
        "records": [{"record_id": "raw", "kind": "raw_output", "artifact": {"path": art.path, "sha256": art.sha256}}],
        "findings": [{"finding_id": "F1", "statement": "observed", "kind": "observed", "basis": ["raw"]}],
        "measurements": [], "behavior": "not_applicable", "coverage": {"covered": ["x"], "not_covered": []}, "unknowns": ["u"]});
    let v = tool_result::verify(
        &f.root,
        &project,
        &registry,
        &serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    assert_eq!(v.check.status, "rejected");
    assert!(
        v.check
            .failures
            .iter()
            .any(|x| x == "record_is_memory_candidate:raw"),
        "{:?}",
        v.check.failures
    );
}

#[test]
fn feedback_is_memory_contract_bound_to_the_current_project() {
    let f = fixture();
    let h = handoff(&f);
    let art = put(&f, "var/m/registered.json", &h);
    let req = |outcome: &str, sources: Vec<Value>| memory::FeedbackRequest {
        id: format!("fb-{outcome}"),
        outcome: outcome.into(),
        result: "按候选先测基线；比较结论仍待更多样本".into(),
        sources,
        elapsed_ms: None,
        unknowns: vec!["收益未测".into()],
        context: None,
    };
    let pb = serde_json::to_vec(&f.project).unwrap();
    let path = Path::new("/abs/current/project-r2.json");
    let refused = memory::feedback(&f.root, path, &pb, &art, &req("not_used", vec![])).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("memory_candidate_not_registered")
    );
    let (out, _) = memory::register(&f.root, &pb, &art, "r2").unwrap();
    let current = out.unwrap();
    let fb = memory::feedback(&f.root, path, &current, &art, &req("not_used", vec![])).unwrap();
    assert_eq!(fb["schema"], "engineering-memory.session-feedback/1");
    assert_eq!(
        (fb["context_id"].as_str(), fb["context_sha256"].as_str()),
        (Some("ctx-1"), Some("c".repeat(64).as_str()))
    );
    let inner = &fb["feedback"];
    assert_eq!(
        (
            inner["record_id"].as_str(),
            inner["revision"].as_u64(),
            inner["used"].as_bool()
        ),
        (Some("exp-1"), Some(2), Some(false))
    );
    assert_eq!(inner["project_id"], "p");
    let rp = &fb["result_project"];
    assert_eq!(
        (rp["path"].as_str(), rp["revision"].as_str()),
        (Some("/abs/current/project-r2.json"), Some("r2"))
    );
    assert_eq!(rp["sha256"], json!(sha256(&current)));
    assert_eq!(
        rp["object"],
        json!({"object_id": "obj", "object_revision": "rev-1", "condition_scope": "cond", "condition_sha256": sha256(b"c")})
    );
    let e = memory::feedback(&f.root, path, &current, &art, &req("helped", vec![])).unwrap_err();
    assert!(
        e.to_string()
            .contains("feedback_sources_required_for_outcome")
    );
    let (_, sources) = anchors(&f);
    let fb = memory::feedback(
        &f.root,
        path,
        &current,
        &art,
        &req("helped", vec![sources[1].clone()]),
    )
    .unwrap();
    assert_eq!(fb["feedback"]["used"], true);
    let e = memory::feedback(&f.root, path, &current, &art, &req("great", vec![])).unwrap_err();
    assert!(e.to_string().contains("feedback_outcome_invalid"));
}

#[test]
fn export_lists_project_records_by_digest() {
    let f = fixture();
    let art = put(&f, "var/projects/p/memory/exp-1.json", &handoff(&f));
    let (out, _) = memory::register(
        &f.root,
        &serde_json::to_vec(&f.project).unwrap(),
        &art,
        "r2",
    )
    .unwrap();
    let mut project: Value = serde_json::from_slice(&out.unwrap()).unwrap();
    project["evidence"]["tool-result:R9"] = json!("b".repeat(64));
    let bytes = serde_json::to_vec_pretty(&project).unwrap();
    let ppath = f.root.path().join("var/projects/p/project-r2.json");
    fs::write(&ppath, &bytes).unwrap();
    put(
        &f,
        "var/projects/p/receipts/decide-D1.json",
        &json!({"status": "refused"}),
    );
    let x = memory::export(&ppath, &bytes).unwrap();
    let ids: Vec<&str> = x["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            "project:r2",
            "memory-candidate:exp-1@r2",
            "receipts:decide-D1.json"
        ]
    );
    assert_eq!(x["missing"], json!(["tool-result:R9"]));
    assert_eq!(
        x["artifacts"][1]["path"],
        json!(
            f.root
                .path()
                .join("var/projects/p/memory/exp-1.json")
                .to_str()
                .unwrap()
        )
    );
}
