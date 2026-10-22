//! Independent, frozen expectations for native decision updates (oev2.decision-update/1).
use oe_contracts::{CapabilityRegistry, DecisionUpdate, ObjectIdentity};
use oe_core::{SkillRoot, sha256};
use oe_kernel::project::{Project, context, decide, tool_result};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

struct Fixture {
    _dir: tempfile::TempDir,
    _tools: tempfile::TempDir,
    root: SkillRoot,
    registry: CapabilityRegistry,
    project: Value,
}

fn write(root: &Path, rel: &str, bytes: &[u8]) -> Value {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, bytes).unwrap();
    json!({"path": rel, "sha256": sha256(bytes)})
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("SKILL.md"), "fixture").unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = fs::canonicalize(tools.path()).unwrap().join("faketool");
    fs::write(&tool, b"#!/bin/sh\necho fake\n").unwrap();
    let root = SkillRoot::new(fs::canonicalize(dir.path()).unwrap()).unwrap();
    let registry = serde_json::from_value(json!({"schema": "oev2.capability-registry/1", "capabilities": [{
        "name": "faketool", "display": "FakeTool", "skill": "faketool", "role": "fixture", "outputs": [],
        "version": "1.0", "executable": tool.to_str().unwrap(), "executable_sha256": sha256(b"#!/bin/sh\necho fake\n"),
        "invocation": ["cli"], "acceptance": "fixture", "acceptance_sha256": sha256(b"a"), "status": "qualified", "limits": []}]})).unwrap();
    let d = |id: &str| {
        json!({"id": id, "object_id": "obj", "object_revision": "rev-1", "condition_scope": "cond",
        "source_sha256": sha256(b"source"), "conclusion": "unknown", "adopted": false, "basis": []})
    };
    let project = json!({"schema": "oev2.project/1", "project_id": "p", "revision": "r1", "purpose": "decide fixture",
        "objects": {"obj": "rev-1"}, "conditions": {"cond": "c"}, "evidence": {"src": sha256(b"source")},
        "decisions": [d("D1"), d("D2")], "dependencies": {"D2": ["D1"]}});
    Fixture {
        _dir: dir,
        _tools: tools,
        root,
        registry,
        project,
    }
}

fn result(f: &Fixture, id: &str, decisions: &[&str]) -> (Value, Vec<u8>) {
    let p: Project = serde_json::from_value(f.project.clone()).unwrap();
    let ctx = context(
        &p,
        &ObjectIdentity {
            object_id: "obj".into(),
            revision: "rev-1".into(),
            condition_scope: "cond".into(),
            source_sha256: sha256(b"source"),
        },
    )
    .unwrap();
    let raw = write(
        f.root.path(),
        &format!("var/tr/{id}/raw.json"),
        b"{\"ok\":true}",
    );
    let r = json!({"schema": "oev2.tool-result/1", "result_id": id, "project": ctx, "purpose": "p", "decisions": decisions, "acceptance": [],
        "tool": {"name": "faketool", "version": "1.0", "executable_sha256": f.registry.capabilities[0].executable_sha256, "skill": "faketool", "invocation": "cli"},
        "subject": {"kind": "none", "identity": "none"},
        "inputs": {"arguments": [], "conditions": [], "environment": [], "files": []},
        "execution": {"status": "completed", "started_at": "2026-10-21T01:00:00Z", "finished_at": "2026-10-21T01:00:01Z", "cleanup": "exited"},
        "records": [{"record_id": "raw", "kind": "raw_output", "artifact": raw}],
        "findings": [{"finding_id": "F1", "statement": "observed", "kind": "observed", "basis": ["raw"]},
                     {"finding_id": "F2", "statement": "imported", "kind": "imported", "basis": ["raw"]}],
        "measurements": [], "behavior": "not_applicable", "coverage": {"covered": ["x"], "not_covered": []}, "unknowns": ["u"]});
    let bytes = serde_json::to_vec(&r).unwrap();
    let artifact = write(f.root.path(), &format!("var/tr/{id}.json"), &bytes);
    (artifact, bytes)
}

fn register(f: &mut Fixture, bytes: &[u8], rev: &str) {
    let pb = serde_json::to_vec(&f.project).unwrap();
    let (out, reg) =
        tool_result::register(&f.root, &pb, &f.registry, bytes, rev, "var/p.json").unwrap();
    assert_eq!(reg.status, "registered", "{:?}", reg.check.failures);
    f.project = serde_json::from_slice(&out.unwrap()).unwrap();
}

fn update(
    f: &Fixture,
    decision: &str,
    conclusion: &str,
    results: Vec<Value>,
    findings: &[&str],
    rev: &str,
) -> (Option<Value>, oe_contracts::DecisionUpdateReceipt) {
    let req: DecisionUpdate = serde_json::from_value(json!({"schema": "oev2.decision-update/1", "decision_id": decision, "conclusion": conclusion,
        "results": results, "findings": findings, "rationale": "the observed record shows it", "claim_scope": "fixture scope"})).unwrap();
    let pb = serde_json::to_vec(&f.project).unwrap();
    let (out, receipt) =
        decide::decide(&f.root, &pb, &f.registry, &req, rev, "var/p2.json").unwrap();
    (out.map(|b| serde_json::from_slice(&b).unwrap()), receipt)
}

#[test]
fn registered_observed_result_updates_only_that_decision() {
    let mut f = fixture();
    let (art, bytes) = result(&f, "R1", &["D1"]);
    register(&mut f, &bytes, "r2");
    let before = f.project.clone();
    let (out, r) = update(&f, "D1", "supported", vec![art], &["R1#F1"], "r3");
    assert_eq!(r.status, "updated", "{:?}", r.failures);
    let next = out.unwrap();
    assert_eq!(next["revision"], "r3");
    assert_eq!(next["decisions"][0]["conclusion"], "supported");
    assert_eq!(next["decisions"][0]["basis"], json!(["tool-result:R1"]));
    assert_eq!(next["decisions"][0]["adopted"], false);
    assert_eq!(next["decisions"][1], before["decisions"][1]);
    assert_eq!(next["evidence"], before["evidence"]);
    assert_eq!(r.affected_decisions, vec!["D2"]);
}

#[test]
fn decisive_updates_need_registered_targeted_observed_evidence() {
    let mut f = fixture();
    let (art, bytes) = result(&f, "R1", &["D1"]);
    // Not yet registered.
    let (_, r) = update(&f, "D1", "supported", vec![art.clone()], &["R1#F1"], "r2");
    assert!(
        r.status == "refused"
            && r.failures
                .iter()
                .any(|x| x.starts_with("result_not_registered")),
        "{:?}",
        r.failures
    );
    register(&mut f, &bytes, "r2");
    let cases: Vec<(&str, &str, &[&str], &str)> = vec![
        (
            "D2",
            "supported",
            &["R1#F1"],
            "no_registered_observed_result_for_decision",
        ),
        ("D1", "supported", &["R1#F9"], "finding_unknown:R1#F9"),
        (
            "D1",
            "refuted",
            &["R1#F2"],
            "finding_not_observed_or_inferred:R1#F2",
        ),
        ("D1", "supported", &[], "findings_not_cited"),
        ("DX", "supported", &["R1#F1"], "decision_unknown:DX"),
    ];
    for (d, c, fs_, code) in cases {
        let (out, r) = update(&f, d, c, vec![art.clone()], fs_, "r3");
        assert!(out.is_none() && r.status == "refused", "{code}");
        assert!(
            r.failures.iter().any(|x| x.starts_with(code)),
            "{code}: {:?}",
            r.failures
        );
    }
    let (_, r) = update(&f, "D1", "supported", vec![art.clone()], &["R1#F1"], "r2");
    assert!(r.failures.iter().any(|x| x == "project_revision_not_new"));
    // Unknown may be recorded with a rationale and no decisive evidence.
    let (out, r) = update(&f, "D2", "unknown", vec![], &[], "r3");
    assert!(
        out.is_some() && r.status == "updated" && r.basis.is_empty(),
        "{:?}",
        r.failures
    );
}

#[test]
fn adopted_decisions_and_tampered_results_are_refused() {
    let mut f = fixture();
    let (art, bytes) = result(&f, "R1", &["D1"]);
    register(&mut f, &bytes, "r2");
    fs::write(f.root.path().join("var/tr/R1/raw.json"), b"{\"ok\":false}").unwrap();
    let (_, r) = update(&f, "D1", "supported", vec![art.clone()], &["R1#F1"], "r3");
    assert!(
        r.failures.iter().any(|x| x.starts_with("result_rejected")),
        "{:?}",
        r.failures
    );
    fs::write(f.root.path().join("var/tr/R1/raw.json"), b"{\"ok\":true}").unwrap();
    f.project["decisions"][0]["conclusion"] = json!("supported");
    f.project["decisions"][0]["adopted"] = json!(true);
    f.project["decisions"][0]["basis"] = json!(["tool-result:R1"]);
    let (_, r) = update(&f, "D1", "refuted", vec![art], &["R1#F1"], "r3");
    assert!(
        r.failures
            .iter()
            .any(|x| x == "decision_adopted_use_adoption_flow"),
        "{:?}",
        r.failures
    );
}

fn result_bound(
    f: &Fixture,
    id: &str,
    decision: &str,
    revision: &str,
    condition: &str,
    source: &[u8],
) -> (Value, Vec<u8>) {
    let p: Project = serde_json::from_value(f.project.clone()).unwrap();
    let ctx = context(
        &p,
        &ObjectIdentity {
            object_id: "obj".into(),
            revision: revision.into(),
            condition_scope: condition.into(),
            source_sha256: sha256(source),
        },
    )
    .unwrap();
    let raw = write(
        f.root.path(),
        &format!("var/tr/{id}/raw.json"),
        b"{\"ok\":true}",
    );
    let r = json!({"schema": "oev2.tool-result/1", "result_id": id, "project": ctx, "purpose": "p", "decisions": [decision], "acceptance": [],
        "tool": {"name": "faketool", "version": "1.0", "executable_sha256": f.registry.capabilities[0].executable_sha256, "skill": "faketool", "invocation": "cli"},
        "subject": {"kind": "none", "identity": "none"},
        "inputs": {"arguments": [], "conditions": [], "environment": [], "files": []},
        "execution": {"status": "completed", "started_at": "2026-10-21T01:00:00Z", "finished_at": "2026-10-21T01:00:01Z", "cleanup": "exited"},
        "records": [{"record_id": "raw", "kind": "raw_output", "artifact": raw}],
        "findings": [{"finding_id": "F1", "statement": "observed", "kind": "observed", "basis": ["raw"]}],
        "measurements": [], "behavior": "not_applicable", "coverage": {"covered": ["x"], "not_covered": []}, "unknowns": ["u"]});
    let bytes = serde_json::to_vec(&r).unwrap();
    (
        write(f.root.path(), &format!("var/tr/{id}.json"), &bytes),
        bytes,
    )
}

#[test]
fn results_must_bind_the_decision_object_and_rebind_to_its_current_revision() {
    let mut f = fixture();
    // Run under another condition: registered, but it cannot speak for D1 (bound to "cond").
    f.project["conditions"]["cond2"] = json!("c2");
    let (art, bytes) = result_bound(&f, "R2", "D1", "rev-1", "cond2", b"source");
    register(&mut f, &bytes, "r2");
    let (out, r) = update(&f, "D1", "supported", vec![art], &["R2#F1"], "r3");
    assert!(
        out.is_none() && r.failures.iter().any(|x| x == "result_binding_mismatch:R2"),
        "{:?}",
        r.failures
    );
    // The object is revised with a new source; fresh results on it rebind D1 to that binding.
    f.project["objects"]["obj"] = json!("rev-2");
    f.project["evidence"]["src2"] = json!(sha256(b"source2"));
    f.project["revision"] = json!("r3");
    let (art, bytes) = result_bound(&f, "R3", "D1", "rev-2", "cond", b"source2");
    register(&mut f, &bytes, "r4");
    let (art_old_source, bytes) = result_bound(&f, "R4", "D1", "rev-2", "cond", b"source");
    register(&mut f, &bytes, "r5");
    let (out, r) = update(
        &f,
        "D1",
        "supported",
        vec![art.clone(), art_old_source],
        &["R3#F1", "R4#F1"],
        "r6",
    );
    assert!(
        out.is_none() && r.failures.iter().any(|x| x == "result_binding_conflict"),
        "{:?}",
        r.failures
    );
    let (out, r) = update(&f, "D1", "supported", vec![art], &["R3#F1"], "r6");
    assert_eq!(r.status, "updated", "{:?}", r.failures);
    let next = out.unwrap();
    assert_eq!(next["decisions"][0]["object_revision"], "rev-2");
    assert_eq!(next["decisions"][0]["source_sha256"], sha256(b"source2"));
    assert_eq!(next["decisions"][1]["object_revision"], "rev-1");
    assert_eq!(next["decisions"][1]["source_sha256"], sha256(b"source"));
}
