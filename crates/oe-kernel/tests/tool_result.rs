//! Independent, frozen expectations for tool-result handback (oev2.tool-result/1).
use oe_contracts::{CapabilityRegistry, ObjectIdentity, ToolComparisonRequest};
use oe_core::{SkillRoot, sha256};
use oe_kernel::project::{Project, context, tool_result};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

struct Fixture {
    _dir: tempfile::TempDir,
    _tools: tempfile::TempDir,
    root: SkillRoot,
    tool: std::path::PathBuf,
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
    fs::write(&tool, b"#!/bin/sh\necho fake 1.0\n").unwrap();
    let root = SkillRoot::new(fs::canonicalize(dir.path()).unwrap()).unwrap();
    let project = json!({
        "schema": "oev2.project/1", "project_id": "p", "revision": "r1", "purpose": "tool handback fixture",
        "objects": {"obj": "rev-1"}, "conditions": {"cond": "load 10 N at 20 C"},
        "evidence": {"src": sha256(b"source")},
        "decisions": [{"id": "D1", "object_id": "obj", "object_revision": "rev-1", "condition_scope": "cond",
            "source_sha256": sha256(b"source"), "conclusion": "unknown", "adopted": false, "basis": []}],
        "dependencies": {}
    });
    Fixture {
        _dir: dir,
        _tools: tools,
        root,
        tool,
        project,
    }
}

fn registry(f: &Fixture) -> CapabilityRegistry {
    let bytes = fs::read(fs::canonicalize(&f.tool).unwrap()).unwrap();
    serde_json::from_value(json!({"schema": "oev2.capability-registry/1", "capabilities": [{
        "name": "faketool", "display": "FakeTool", "skill": "faketool", "role": "fixture", "outputs": ["report"],
        "version": "1.0", "executable": f.tool.to_str().unwrap(), "executable_sha256": sha256(&bytes),
        "invocation": ["cli"], "acceptance": "fixture acceptance", "acceptance_sha256": sha256(b"acceptance"),
        "status": "qualified", "limits": []}]}))
    .unwrap()
}

fn project(v: &Value) -> Project {
    serde_json::from_value(v.clone()).unwrap()
}

fn ctx(p: &Project) -> Value {
    let id = ObjectIdentity {
        object_id: "obj".into(),
        revision: "rev-1".into(),
        condition_scope: "cond".into(),
        source_sha256: sha256(b"source"),
    };
    serde_json::to_value(context(p, &id).unwrap()).unwrap()
}

fn result(f: &Fixture, id: &str, record: &[u8]) -> Value {
    let raw = write(f.root.path(), &format!("var/tr/{id}/raw.json"), record);
    let reg = registry(f);
    json!({
        "schema": "oev2.tool-result/1", "result_id": id, "project": ctx(&project(&f.project)),
        "purpose": "locate the implementation", "decisions": ["D1"], "acceptance": ["A1"],
        "tool": {"name": "faketool", "version": "1.0", "executable_sha256": reg.capabilities[0].executable_sha256,
                 "skill": "faketool", "invocation": "cli"},
        "subject": {"kind": "none", "identity": "no code subject"},
        "inputs": {"arguments": ["--root", "x"], "conditions": ["read only"], "environment": ["macOS arm64"], "files": []},
        "execution": {"status": "completed", "started_at": "2026-10-21T01:00:00Z", "finished_at": "2026-10-21T01:00:02.5Z",
                      "exit_code": 0, "cleanup": "process exited; no resources left"},
        "records": [{"record_id": "raw", "kind": "raw_output", "artifact": raw}],
        "findings": [{"finding_id": "F1", "statement": "symbol found at a.rs:10", "kind": "observed", "basis": ["raw"]},
                     {"finding_id": "F2", "statement": "the call site is the only writer", "kind": "inferred", "basis": ["F1"]}],
        "measurements": [], "behavior": "not_applicable",
        "coverage": {"covered": ["crate a"], "not_covered": ["crate b"]},
        "unknowns": ["dynamic dispatch not resolved"]
    })
}

fn check(f: &Fixture, r: &Value) -> oe_contracts::ToolResultCheck {
    let bytes = serde_json::to_vec(r).unwrap();
    tool_result::verify(&f.root, &project(&f.project), &registry(f), &bytes)
        .unwrap()
        .check
}

fn rejected_with(f: &Fixture, r: &Value, code: &str) {
    let c = check(f, r);
    assert_eq!(c.status, "rejected", "{code}: {:?}", c.failures);
    assert!(
        c.failures.iter().any(|x| x.starts_with(code)),
        "{code}: {:?}",
        c.failures
    );
}

#[test]
fn complete_result_is_accepted_and_registers_a_new_project_copy() {
    let f = fixture();
    let r = result(&f, "R1", b"{\"hits\":1}");
    let c = check(&f, &r);
    assert_eq!(c.status, "accepted", "{:?}", c.failures);
    assert_eq!(c.decision_support, "observed_findings");
    assert_eq!(
        (c.records_verified, c.findings_observed, c.findings_inferred),
        (1, 1, 1)
    );
    let pbytes = serde_json::to_vec_pretty(&f.project).unwrap();
    let rbytes = serde_json::to_vec(&r).unwrap();
    let (out, reg) = tool_result::register(
        &f.root,
        &pbytes,
        &registry(&f),
        &rbytes,
        "r2",
        "var/p-r2.json",
    )
    .unwrap();
    assert_eq!(reg.status, "registered");
    let next: Value = serde_json::from_slice(&out.unwrap()).unwrap();
    assert_eq!(next["revision"], "r2");
    assert_eq!(next["evidence"]["tool-result:R1"], sha256(&rbytes));
    assert!(next["evidence"]["tool-record:R1:raw"].is_string());
    assert_eq!(next["dependencies"]["D1"], json!(["tool-result:R1"]));
    assert_eq!(
        next["decisions"], f.project["decisions"],
        "conclusions and adoption unchanged"
    );
    assert_eq!(reg.affected_decisions, vec!["D1"]);
    assert_eq!(reg.previous_project_sha256, sha256(&pbytes));
}

#[test]
fn provenance_counterexamples_are_rejected() {
    let f = fixture();
    let base = result(&f, "R1", b"{}");
    let cases: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
        (
            "tool:unregistered",
            Box::new(|r: &mut Value| r["tool"]["name"] = json!("othertool")),
        ),
        (
            "tool:version_mismatch",
            Box::new(|r: &mut Value| r["tool"]["version"] = json!("2.0")),
        ),
        (
            "decision_unknown:DX",
            Box::new(|r: &mut Value| r["decisions"] = json!(["DX"])),
        ),
        (
            "observed_without_record_basis",
            Box::new(|r: &mut Value| r["findings"][0]["basis"] = json!([])),
        ),
        (
            "inferred_without_basis",
            Box::new(|r: &mut Value| r["findings"][1]["basis"] = json!(["missing"])),
        ),
        (
            "coverage_missing",
            Box::new(|r: &mut Value| r["coverage"]["covered"] = json!([" "])),
        ),
        (
            "execution_time_order",
            Box::new(|r: &mut Value| r["execution"]["finished_at"] = json!("2026-10-21T00:59:59Z")),
        ),
        (
            "execution_time_format",
            Box::new(|r: &mut Value| r["execution"]["started_at"] = json!("2026-10-21 01:00:00")),
        ),
        (
            "record_id_invalid_or_duplicate",
            Box::new(|r: &mut Value| {
                let rec = r["records"][0].clone();
                r["records"].as_array_mut().unwrap().push(rec);
            }),
        ),
        (
            "failed_without_failure_text",
            Box::new(|r: &mut Value| r["execution"]["status"] = json!("failed")),
        ),
        (
            "records_missing",
            Box::new(|r: &mut Value| {
                r["records"] = json!([]);
                r["findings"] = json!([{"finding_id": "F9", "statement": "nothing", "kind": "unknown", "basis": []}]);
            }),
        ),
        (
            "project_binding:project_mismatch",
            Box::new(|r: &mut Value| r["project"]["project_id"] = json!("q")),
        ),
        (
            "measurement_without_record",
            Box::new(
                |r: &mut Value| r["measurements"] = json!([{"measurement_id": "m", "metric": "t", "unit": "s", "value": 1.0, "samples": 3, "record_id": "nope"}]),
            ),
        ),
    ];
    for (code, mutate) in cases {
        let mut r = base.clone();
        mutate(&mut r);
        rejected_with(&f, &r, code);
    }
    let mut extra = base.clone();
    extra["surprise"] = json!(true);
    rejected_with(&f, &extra, "tool_result_contract_invalid");
}

#[test]
fn changed_project_executable_or_record_bytes_are_rejected() {
    let mut f = fixture();
    let r = result(&f, "R1", b"{\"hits\":1}");
    // Record tampered after the run.
    fs::write(f.root.path().join("var/tr/R1/raw.json"), b"{\"hits\":2}").unwrap();
    rejected_with(&f, &r, "record:raw");
    fs::write(f.root.path().join("var/tr/R1/raw.json"), b"{\"hits\":1}").unwrap();
    assert_eq!(check(&f, &r).status, "accepted");
    // Object revision and condition content changed in the Project.
    let original = f.project.clone();
    f.project["objects"]["obj"] = json!("rev-2");
    f.project["decisions"][0]["object_revision"] = json!("rev-2");
    rejected_with(&f, &r, "project_binding:object_changed");
    f.project = original.clone();
    f.project["conditions"]["cond"] = json!("load 12 N at 20 C");
    rejected_with(&f, &r, "project_binding:condition_changed");
    f.project = original;
    // Tool executable changed after registration.
    let reg = registry(&f);
    let bytes = serde_json::to_vec(&r).unwrap();
    fs::write(&f.tool, b"#!/bin/sh\necho patched\n").unwrap();
    let c = tool_result::verify(&f.root, &project(&f.project), &reg, &bytes)
        .unwrap()
        .check;
    assert_eq!(c.status, "rejected");
    assert!(
        c.failures.iter().any(|x| x == "tool:executable_changed"),
        "{:?}",
        c.failures
    );
    assert_eq!(tool_result::registry_status(&reg)["status"], "mismatch");
}

#[test]
fn later_project_revisions_warn_but_keep_the_binding() {
    let mut f = fixture();
    let r = result(&f, "R1", b"{}");
    f.project["evidence"]["unrelated"] = json!(sha256(b"other"));
    f.project["revision"] = json!("r5");
    let c = check(&f, &r);
    assert_eq!(c.status, "accepted", "{:?}", c.failures);
    assert!(
        c.project_revised_since_run && c.warnings.iter().any(|w| w == "project_revised_since_run")
    );
}

#[test]
fn subject_manifest_change_removes_decision_support() {
    let f = fixture();
    let subject = fs::canonicalize(f._tools.path()).unwrap().join("a.rs");
    fs::write(&subject, b"fn a() {}").unwrap();
    let manifest = json!({"schema": "oev2.subject-manifest/1", "files": [{"path": subject.to_str().unwrap(), "sha256": sha256(b"fn a() {}")}]});
    let m = write(
        f.root.path(),
        "var/tr/R1/subject.json",
        &serde_json::to_vec(&manifest).unwrap(),
    );
    let mut r = result(&f, "R1", b"{}");
    r["subject"] = json!({"kind": "file_set", "identity": "crate a working tree", "manifest": m});
    let c = check(&f, &r);
    assert_eq!(
        (
            c.status.as_str(),
            c.subject_status.as_str(),
            c.decision_support.as_str()
        ),
        ("accepted", "manifest_matched", "observed_findings")
    );
    fs::write(&subject, b"fn a() { b() }").unwrap();
    let c = check(&f, &r);
    assert_eq!(
        (
            c.status.as_str(),
            c.subject_status.as_str(),
            c.decision_support.as_str()
        ),
        ("accepted", "manifest_changed", "none")
    );
    assert!(c.warnings.iter().any(|w| w == "subject_changed_since_run"));
}

#[test]
fn register_refuses_rejected_results_and_reused_revisions() {
    let f = fixture();
    let pbytes = serde_json::to_vec(&f.project).unwrap();
    let mut bad = result(&f, "R1", b"{}");
    bad["tool"]["name"] = json!("othertool");
    let (out, reg) = tool_result::register(
        &f.root,
        &pbytes,
        &registry(&f),
        &serde_json::to_vec(&bad).unwrap(),
        "r2",
        "var/x.json",
    )
    .unwrap();
    assert!(out.is_none() && reg.status == "refused" && reg.project_sha256 == sha256(&pbytes));
    let good = serde_json::to_vec(&result(&f, "R2", b"{}")).unwrap();
    let (out, reg) =
        tool_result::register(&f.root, &pbytes, &registry(&f), &good, "r1", "var/x.json").unwrap();
    assert!(
        out.is_none()
            && reg
                .check
                .failures
                .iter()
                .any(|x| x == "project_revision_not_new")
    );
}

fn measured(
    f: &Fixture,
    id: &str,
    value: f64,
    start: &str,
    finish: &str,
    env: &str,
    behavior: &str,
    samples: u64,
) -> Value {
    let mut r = result(f, id, format!("{{\"value\":{value}}}").as_bytes());
    r["execution"]["started_at"] = json!(start);
    r["execution"]["finished_at"] = json!(finish);
    r["inputs"]["environment_sha256"] = json!(sha256(env.as_bytes()));
    r["inputs"]["input_set_sha256"] = json!(sha256(b"inputs"));
    r["measurements"] = json!([{"measurement_id": "rss", "metric": "peak_rss", "unit": "MiB", "value": value, "samples": samples, "record_id": "raw"}]);
    r["behavior"] = json!(behavior);
    r
}

fn registered(f: &mut Fixture, results: &[(&str, Value)]) -> Vec<Value> {
    let mut refs = Vec::new();
    for (i, (id, r)) in results.iter().enumerate() {
        let bytes = serde_json::to_vec(r).unwrap();
        let path = format!("var/tr/{id}.json");
        let artifact = write(f.root.path(), &path, &bytes);
        let pbytes = serde_json::to_vec(&f.project).unwrap();
        let (out, reg) = tool_result::register(
            &f.root,
            &pbytes,
            &registry(f),
            &bytes,
            &format!("r{}", i + 2),
            "var/p.json",
        )
        .unwrap();
        assert_eq!(reg.status, "registered", "{:?}", reg.check.failures);
        f.project = serde_json::from_slice(&out.unwrap()).unwrap();
        refs.push(artifact);
    }
    refs
}

fn compare(
    f: &Fixture,
    base: &Value,
    cand: &Value,
    direction: &str,
    min_samples: u64,
) -> oe_contracts::ToolComparisonReport {
    let req: ToolComparisonRequest = serde_json::from_value(json!({"schema": "oev2.tool-comparison/1", "comparison_id": "c1",
        "baseline": base, "candidate": cand, "metric": "peak_rss", "unit": "MiB", "direction": direction,
        "minimum_improvement": 0.05, "minimum_samples": min_samples})).unwrap();
    tool_result::compare(&f.root, &project(&f.project), &req).unwrap()
}

#[test]
fn baseline_first_comparison_verdicts() {
    let mut f = fixture();
    let t = (
        "2026-10-21T01:00:00Z",
        "2026-10-21T01:00:10Z",
        "2026-10-21T02:00:00Z",
        "2026-10-21T02:00:10Z",
    );
    let results = vec![
        ("B", measured(&f, "B", 100.0, t.0, t.1, "mac", "passed", 5)),
        ("C", measured(&f, "C", 80.0, t.2, t.3, "mac", "passed", 5)),
        ("E", measured(&f, "E", 80.0, t.2, t.3, "linux", "passed", 5)),
        ("X", measured(&f, "X", 80.0, t.2, t.3, "mac", "failed", 5)),
        ("S", measured(&f, "S", 80.0, t.2, t.3, "mac", "passed", 1)),
        ("Y", measured(&f, "Y", 99.0, t.2, t.3, "mac", "passed", 5)),
    ];
    let refs = registered(&mut f, &results);
    let (b, c, e, x, s, y) = (&refs[0], &refs[1], &refs[2], &refs[3], &refs[4], &refs[5]);
    let r = compare(&f, b, c, "lower_is_better", 3);
    assert_eq!(r.verdict, "improved", "{:?}", r.reasons);
    assert!((r.relative_improvement.unwrap() - 0.2).abs() < 1e-12);
    assert_eq!(
        compare(&f, b, y, "lower_is_better", 3).verdict,
        "not_improved"
    );
    assert_eq!(
        compare(&f, b, c, "higher_is_better", 3).verdict,
        "not_improved"
    );
    let r = compare(&f, b, e, "lower_is_better", 3);
    assert!(
        r.verdict == "invalid" && r.reasons.iter().any(|x| x == "environment_not_same"),
        "{:?}",
        r.reasons
    );
    let r = compare(&f, b, x, "lower_is_better", 3);
    assert!(
        r.verdict == "inconclusive" && r.reasons.iter().any(|x| x == "behavior_not_passed_on_both")
    );
    let r = compare(&f, b, s, "lower_is_better", 3);
    assert!(r.verdict == "inconclusive" && r.reasons.iter().any(|x| x == "samples_below_minimum"));
    // Candidate measured before the baseline finished.
    let r = compare(&f, c, b, "lower_is_better", 3);
    assert!(
        r.verdict == "invalid" && r.reasons.iter().any(|x| x == "baseline_not_first"),
        "{:?}",
        r.reasons
    );
    // A result that was never registered cannot be compared.
    let loose = write(
        f.root.path(),
        "var/tr/loose.json",
        &serde_json::to_vec(&measured(&f, "L", 50.0, t.2, t.3, "mac", "passed", 5)).unwrap(),
    );
    let r = compare(&f, b, &loose, "lower_is_better", 3);
    assert!(
        r.verdict == "invalid"
            && r.reasons
                .iter()
                .any(|x| x == "candidate_not_registered_in_project")
    );
}

#[test]
fn utc_parser_orders_and_rejects() {
    assert!(tool_result::utc("2026-10-21T01:00:00Z").is_some());
    assert!(tool_result::utc("2026-10-21T01:00:00.123456789Z").is_some());
    assert!(
        tool_result::utc("2026-10-21T01:00:00.5Z") > tool_result::utc("2026-10-21T01:00:00.123Z")
    );
    for bad in [
        "2026-10-21T01:00:00",
        "2026-10-21 01:00:00Z",
        "2026-13-01T00:00:00Z",
        "2026-10-21T24:00:00Z",
        "2026-10-21T01:00:00.Z",
        "2026-10-21T01:00:00.1234567890Z",
        "26-10-21T01:00:00Z",
    ] {
        assert!(tool_result::utc(bad).is_none(), "{bad}");
    }
}
