use engineering_memory_runtime::{organization::execute, store::sha};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

struct Fixture {
    root: TempDir,
    home: std::path::PathBuf,
    method: Value,
}

fn a(root: &Path, name: &str, text: &str) -> Value {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    json!({"id":name.replace(['/', '.'], "-"),"path":path.to_str().unwrap(),"sha256":sha(text.as_bytes())})
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("memory");
        let evidence = a(
            root.path(),
            "exploration.txt",
            "Host tested this limited method; artificial test fixture.",
        );
        let method = json!({"schema":"engineering-memory.internal-method/2","id":"test-method","revision":1,
            "title":"Local semantic reading then calculation","scope":["test objects only"],
            "skill":a(root.path(),"method/SKILL.md","Read one local label; escalate uncertainty."),
            "resources":[],"exploration_evidence":[evidence.clone()],"pilot_evidence":[evidence],
            "delegable_roles":["agent","program"],"limitations":["test evidence is not a real model call"]});
        execute(&home, "method-register", method.clone()).unwrap();
        Self { root, home, method }
    }
    fn plan(&self, name: &str) -> Value {
        let m = execute(
            &self.home,
            "method-show",
            json!({"id":"test-method","revision":1}),
        )
        .unwrap();
        json!({"schema":"engineering-memory.organization-plan/2","id":name,"goal":"Read a test label and compute a result",
            "conditions":["test fixture"],"method_id":"test-method","method_revision":1,"method_sha256":m["method_sha256"],
            "project_id":null,"host":"test-host","execution_mode":"development_replay",
            "inputs":[a(self.root.path(),"image-a.txt","pixels-a"),a(self.root.path(),"image-b.txt","pixels-b")],
            "claims":[
                {"id":"label","question":"Read local label","role":"agent","required":true,"depends_on":[],"input_ids":["image-a-txt"],"acceptance":"Visible label with source review"},
                {"id":"unit","question":"Read unit","role":"agent","required":true,"depends_on":[],"input_ids":["image-b-txt"],"acceptance":"Visible unit with source review"},
                {"id":"result","question":"Calculate","role":"program","required":true,"depends_on":["label"],"input_ids":[],"acceptance":"Correct under reviewed assumptions"}]})
    }
    fn create(&self, name: &str) {
        execute(&self.home, "create", self.plan(name)).unwrap();
    }
    fn packet(&self, name: &str, claim: &str) -> Value {
        execute(&self.home, "packet", json!({"id":name,"claim_id":claim})).unwrap()
    }
    fn finding(&self, name: &str, claim: &str, fid: &str, status: &str) -> Value {
        let p = self.packet(name, claim);
        json!({"schema":"engineering-memory.organization-finding/2","organization_id":name,"id":fid,"claim_id":claim,
            "packet_sha256":p["packet_sha256"],"status":status,"summary":"Explicit test fixture judgment",
            "producer":if claim=="result"{"program"}else{"agent"},
            "evidence":[a(self.root.path(),&format!("{fid}-evidence.txt"),"observed result")],
            "review":a(self.root.path(),&format!("{fid}-review.txt"),"host checked the selected fixture"),
            "supersedes":p["packet"]["current_finding"]})
    }
    fn record(&self, name: &str, claim: &str, fid: &str, status: &str) -> Value {
        execute(&self.home, "record", self.finding(name, claim, fid, status)).unwrap()
    }
    fn close(&self, name: &str, outcome: &str) -> Value {
        let s = execute(&self.home, "status", json!({"id":name})).unwrap();
        json!({"schema":"engineering-memory.organization-close/1","organization_id":name,"id":"close-1","outcome":outcome,
            "expected_heads_sha256":s["heads_sha256"],"summary":"Host decision about fixture",
            "review":a(self.root.path(),"close-review.txt","Review of declared goal and remaining work")})
    }
    fn handoff(&self, parent: &str, child: &str) -> Value {
        let current = execute(&self.home, "frontier", json!({"id":parent})).unwrap();
        json!({"schema":"engineering-memory.organization-handoff/2",
            "id":format!("handoff-{child}"),"parent_id":parent,"child_id":child,
            "expected_parent_plan_sha256":current["snapshot"]["plan_sha256"],
            "expected_heads_sha256":current["snapshot"]["heads_sha256"],
            "changes":[{"claim_id":"label","role":"host",
                "acceptance":"Host reviews the same visible label and records uncertainty",
                "reason":"Explicit host takeover of the bounded judgment"}],
            "host":"next-test-host","execution_mode":"development_replay",
            "summary":"Preserve the original scope and unknown; no inherited findings",
            "review":a(self.root.path(),"handoff-review.txt","Host reviewed why this role change applies; fixture only")})
    }
    fn stopped_parent(&self) {
        self.create("parent");
        self.record("parent", "label", "parent-label", "unknown");
        self.record("parent", "unit", "parent-unit", "supported");
        execute(&self.home, "close", self.close("parent", "stopped")).unwrap();
    }
}

#[test]
fn explicit_packet_contains_only_selected_input_and_dependencies() {
    let f = Fixture::new();
    f.create("one");
    let p = f.packet("one", "label");
    assert_eq!(p["packet"]["worker"]["inputs"].as_array().unwrap().len(), 1);
    assert_eq!(p["packet"]["worker"]["inputs"][0]["id"], "image-a-txt");
    assert_eq!(p["executor_bound"], false);
    assert!(p["packet"]["worker"].get("skill").is_none());
    assert!(execute(&f.home, "packet", json!({"id":"one","claim_id":"result"})).is_err());
    f.record("one", "label", "label-1", "supported");
    f.record("one", "unit", "unit-1", "supported");
    let p = f.packet("one", "result");
    assert_eq!(
        p["packet"]["worker"]["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        p["packet"]["worker"]["dependencies"][0]["claim_id"],
        "label"
    );
}

#[test]
fn correction_invalidates_dependant_without_erasing_other_claims() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "supported");
    f.record("one", "unit", "unit-1", "supported");
    f.record("one", "result", "result-1", "supported");
    let s = f.record("one", "label", "label-2", "unknown");
    assert_eq!(s["claims"][0]["state"], "unknown");
    assert_eq!(s["claims"][1]["state"], "supported");
    assert_eq!(s["claims"][2]["state"], "stale");
    assert!(
        f.home
            .join("organizations/one/organization/findings/label-1.json")
            .exists()
    );
    assert!(execute(&f.home, "close", f.close("one", "completed")).is_err());
    f.record("one", "label", "label-3", "supported");
    let s = execute(&f.home, "status", json!({"id":"one"})).unwrap();
    assert_eq!(s["claims"][2]["state"], "stale");
    f.record("one", "result", "result-2", "supported");
    let close = f.close("one", "completed");
    let r = execute(&f.home, "close", close.clone()).unwrap();
    assert_eq!(r["engineering_adoption"], "not_granted");
    assert_eq!(execute(&f.home, "close", close).unwrap(), r);
    assert!(
        f.home
            .join("internal-methods/test-method/usage/one.json")
            .exists()
    );
    assert!(execute(&f.home, "packet", json!({"id":"one","claim_id":"label"})).is_err());
}

#[test]
fn packets_and_findings_cannot_cross_organization_boundaries() {
    let f = Fixture::new();
    f.create("one");
    f.create("two");
    let mut v = f.finding("one", "label", "label-1", "supported");
    v["organization_id"] = json!("two");
    assert!(
        execute(&f.home, "record", v)
            .unwrap_err()
            .to_string()
            .contains("packet_is_stale_or_cross_scope")
    );
    let s = execute(&f.home, "status", json!({"id":"two"})).unwrap();
    assert_eq!(s["claims"][0]["state"], "pending");
}

#[test]
fn source_drift_is_rejected_at_use_and_after_close() {
    let f = Fixture::new();
    f.create("one");
    let v = f.finding("one", "label", "label-1", "supported");
    fs::write(f.root.path().join("image-a.txt"), "changed").unwrap();
    assert!(execute(&f.home, "record", v).is_err());
    assert!(execute(&f.home, "status", json!({"id":"one"})).is_err());
}

#[test]
fn method_revision_is_pinned_and_old_bytes_are_not_overwritten() {
    let f = Fixture::new();
    f.create("one");
    let mut m = f.method.clone();
    m["revision"] = json!(2);
    m["title"] = json!("A second method revision");
    execute(&f.home, "method-register", m.clone()).unwrap();
    let p = f.packet("one", "label");
    assert_eq!(p["packet"]["host"]["method_revision"], 1);
    m["revision"] = json!(1);
    assert!(execute(&f.home, "method-register", m).is_err());
    assert_eq!(
        execute(&f.home, "method-list", json!({})).unwrap()["methods"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn global_skill_and_unqualified_role_handoff_are_rejected() {
    let f = Fixture::new();
    let mut m = f.method.clone();
    m["id"] = json!("unqualified-role");
    m["delegable_roles"] = json!(["unsupported-provider"]);
    assert!(
        execute(&f.home, "method-register", m)
            .unwrap_err()
            .to_string()
            .contains("invalid_method_role")
    );
    let mut m = f.method.clone();
    m["id"] = json!("global");
    m["skill"] = a(
        f.root.path(),
        ".codex/skills/example/SKILL.md",
        "global install forbidden",
    );
    m["skill"]["id"] = json!("global-skill");
    assert!(
        execute(&f.home, "method-register", m)
            .unwrap_err()
            .to_string()
            .contains("internal_method_in_global_skills")
    );
}

#[test]
fn scope_and_dependency_errors_are_rejected_before_work() {
    let f = Fixture::new();
    let mut p = f.plan("one");
    p["claims"][0]["input_ids"] = json!(["not-in-this-task"]);
    assert!(execute(&f.home, "create", p).is_err());
    let mut p = f.plan("two");
    p["claims"][0]["depends_on"] = json!(["result"]);
    assert!(
        execute(&f.home, "create", p)
            .unwrap_err()
            .to_string()
            .contains("dependency_cycle")
    );
    let mut p = f.plan("three");
    p["claims"][0]["depends_on"] = json!(["missing"]);
    assert!(execute(&f.home, "create", p).is_err());
}

#[test]
fn unknown_can_be_archived_as_stopped_but_not_completed() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "unknown");
    assert!(execute(&f.home, "close", f.close("one", "completed")).is_err());
    let r = execute(&f.home, "close", f.close("one", "stopped")).unwrap();
    assert_eq!(r["request"]["outcome"], "stopped");
    assert_eq!(r["claim_states"][0]["state"], "unknown");
}

#[test]
fn concurrent_old_packet_and_wrong_producer_are_rejected() {
    let f = Fixture::new();
    f.create("one");
    let mut old = f.finding("one", "label", "old-request", "supported");
    let mut wrong = old.clone();
    wrong["producer"] = json!("program");
    assert!(execute(&f.home, "record", wrong).is_err());
    let first = f.finding("one", "label", "first", "supported");
    execute(&f.home, "record", first.clone()).unwrap();
    assert_eq!(execute(&f.home, "record", first).unwrap()["replayed"], true);
    old["supersedes"] = json!("first");
    assert!(
        execute(&f.home, "record", old)
            .unwrap_err()
            .to_string()
            .contains("packet_is_stale_or_cross_scope")
    );
}

#[test]
fn close_rejects_a_stale_snapshot_and_evidence_drift() {
    let f = Fixture::new();
    f.create("one");
    let close = f.close("one", "stopped");
    f.record("one", "label", "label-1", "supported");
    assert!(
        execute(&f.home, "close", close)
            .unwrap_err()
            .to_string()
            .contains("close_heads_changed")
    );
    fs::write(
        f.root.path().join("label-1-evidence.txt"),
        "changed observed result",
    )
    .unwrap();
    assert!(execute(&f.home, "status", json!({"id":"one"})).is_err());
}

#[test]
fn closed_status_revalidates_final_host_review() {
    let f = Fixture::new();
    f.create("one");
    execute(&f.home, "close", f.close("one", "stopped")).unwrap();
    fs::write(f.root.path().join("close-review.txt"), "changed acceptance").unwrap();
    assert!(
        execute(&f.home, "status", json!({"id":"one"}))
            .unwrap_err()
            .to_string()
            .contains("artifact_changed")
    );
    let listed = execute(&f.home, "list", json!({})).unwrap();
    assert_eq!(listed["organizations"][0]["source_verified"], false);
}

#[test]
fn record_retry_recovers_event_after_partial_persistence() {
    let f = Fixture::new();
    f.create("one");
    let finding = f.finding("one", "label", "label-1", "supported");
    let dir = f.home.join("organizations/one/organization");
    fs::rename(dir.join("events"), dir.join("events-saved")).unwrap();
    fs::write(dir.join("events"), "simulate interrupted event directory").unwrap();
    assert!(execute(&f.home, "record", finding.clone()).is_err());
    assert!(dir.join("findings/label-1.json").exists());
    fs::rename(dir.join("events"), dir.join("event-failure-original")).unwrap();
    fs::rename(dir.join("events-saved"), dir.join("events")).unwrap();
    assert_eq!(
        execute(&f.home, "record", finding).unwrap()["replayed"],
        true
    );
    let mut recovered = 0;
    for e in fs::read_dir(dir.join("events")).unwrap() {
        let v: Value = serde_json::from_slice(&fs::read(e.unwrap().path()).unwrap()).unwrap();
        if v["kind"] == "organization_finding_recorded" {
            recovered += 1;
        }
    }
    assert_eq!(recovered, 1);
}

#[test]
fn stored_finding_content_is_revalidated() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "unknown");
    let p = f
        .home
        .join("organizations/one/organization/findings/label-1.json");
    let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["finding"]["status"] = json!("supported");
    fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(
        execute(&f.home, "status", json!({"id":"one"}))
            .unwrap_err()
            .to_string()
            .contains("stored_finding_changed")
    );
}

fn frontier(f: &Fixture, name: &str) -> Value {
    execute(&f.home, "frontier", json!({"id":name})).unwrap()
}

fn frontier_row<'a>(value: &'a Value, claim: &str) -> &'a Value {
    value["claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == claim)
        .unwrap()
}

#[test]
fn frontier_exposes_parallel_independent_work_and_blocking_reasons() {
    let f = Fixture::new();
    f.create("one");
    let view = frontier(&f, "one");
    let ready = view["ready_by_role"]["agent"].as_array().unwrap();
    assert_eq!(ready.len(), 2);
    assert_eq!(ready[0]["id"], "label");
    assert_eq!(ready[1]["id"], "unit");
    assert_eq!(frontier_row(&view, "label")["action"], "prepare_packet");
    assert_eq!(view["ready_by_role"]["program"], json!([]));
    assert_eq!(view["needs_host_review"], json!([]));
    assert_eq!(view["blocked"][0]["id"], "result");
    assert_eq!(view["blocked"][0]["blocked_by"][0]["id"], "label");
    assert_eq!(view["blocked"][0]["blocked_by"][0]["status"], "pending");
    assert_eq!(
        view["blocked"][0]["blocked_by"][0]["reason"],
        "dependency_not_supported"
    );
    let status = execute(&f.home, "status", json!({"id":"one"})).unwrap();
    assert_eq!(view["snapshot"]["heads_sha256"], status["heads_sha256"]);
    assert_eq!(view["snapshot"]["plan"]["id"], "one");
    assert_eq!(view["snapshot"]["heads"], json!({}));
    assert_eq!(view["lease_granted"], false);
    assert_eq!(view["automatic_dispatch"], false);
}

#[test]
fn frontier_keeps_unknown_failed_and_contradicted_out_of_ready() {
    for outcome in ["unknown", "failed", "contradicted"] {
        let f = Fixture::new();
        f.create("one");
        f.record("one", "label", "label-1", outcome);
        let view = frontier(&f, "one");
        assert_eq!(view["ready_by_role"]["agent"].as_array().unwrap().len(), 1);
        assert_eq!(view["ready_by_role"]["agent"][0]["id"], "unit");
        assert_eq!(view["needs_host_review"][0]["id"], "label");
        assert_eq!(view["needs_host_review"][0]["status"], outcome);
        assert_eq!(view["needs_host_review"][0]["action"], "diagnose");
        assert_eq!(frontier_row(&view, "label")["ready"], false);
        assert_eq!(view["blocked"][0]["id"], "result");
        assert_eq!(view["blocked"][0]["blocked_by"][0]["status"], outcome);
        assert_eq!(view["automatic_retry"], false);
    }
}

#[test]
fn frontier_tracks_stale_dependencies_without_requeueing_or_losing_other_support() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "supported");
    f.record("one", "unit", "unit-1", "supported");
    f.record("one", "result", "result-1", "supported");
    f.record("one", "label", "label-2", "unknown");
    let view = frontier(&f, "one");
    let result = frontier_row(&view, "result");
    assert_eq!(result["status"], "stale");
    assert_eq!(result["finding_status"], "supported");
    assert_eq!(result["action"], "resolve_dependencies_then_revalidate");
    assert_eq!(result["blocked_by"][0]["status"], "unknown");
    assert_eq!(view["supported"][0]["id"], "unit");
    assert_eq!(view["needs_host_review"].as_array().unwrap().len(), 2);

    f.record("one", "label", "label-3", "supported");
    let view = frontier(&f, "one");
    let result = frontier_row(&view, "result");
    assert_eq!(result["status"], "stale");
    assert_eq!(result["action"], "refresh_packet_and_revalidate");
    assert_eq!(result["dependencies"][0]["recorded_head"], "label-1");
    assert_eq!(result["dependencies"][0]["head"], "label-3");
    assert_eq!(result["dependencies"][0]["head_changed"], true);
    assert_eq!(result["blocked_by"], json!([]));
    assert_eq!(result["ready"], false);
    assert_eq!(view["ready_by_role"]["program"], json!([]));
    assert_eq!(frontier_row(&view, "unit")["status"], "supported");
    assert_eq!(frontier_row(&view, "unit")["head"], "unit-1");
    f.record("one", "result", "result-2", "supported");
    let view = frontier(&f, "one");
    assert_eq!(view["supported"].as_array().unwrap().len(), 3);
    assert_eq!(view["needs_host_review"], json!([]));
}

#[test]
fn frontier_preserves_closed_states_but_never_offers_executable_work() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "unknown");
    f.record("one", "unit", "unit-1", "supported");
    execute(&f.home, "close", f.close("one", "stopped")).unwrap();
    let view = frontier(&f, "one");
    assert_eq!(view["closed"], true);
    assert_eq!(view["close"]["request"]["outcome"], "stopped");
    for role in ["host", "agent", "program"] {
        assert_eq!(view["ready_by_role"][role], json!([]));
    }
    for row in view["claims"].as_array().unwrap() {
        assert_eq!(row["action"], "closed");
        assert_eq!(row["ready"], false);
    }
    assert_eq!(frontier_row(&view, "label")["status"], "unknown");
    assert_eq!(view["needs_host_review"][0]["id"], "label");
    assert_eq!(view["blocked"][0]["id"], "result");
    assert_eq!(view["supported"][0]["id"], "unit");
}

#[test]
fn frontier_is_stable_and_never_creates_or_updates_task_files() {
    fn tree_snapshot(root: &Path) -> Vec<(std::path::PathBuf, std::time::SystemTime, Vec<u8>)> {
        let mut rows = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(path) = pending.pop() {
            let metadata = fs::symlink_metadata(&path).unwrap();
            let contents = if metadata.is_dir() {
                for child in fs::read_dir(&path).unwrap() {
                    pending.push(child.unwrap().path());
                }
                Vec::new()
            } else {
                fs::read(&path).unwrap()
            };
            rows.push((
                path.strip_prefix(root).unwrap().to_path_buf(),
                metadata.modified().unwrap(),
                contents,
            ));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }

    let f = Fixture::new();
    f.create("one");
    let before = tree_snapshot(f.root.path());
    let first = frontier(&f, "one");
    let second = frontier(&f, "one");
    assert_eq!(first, second);
    assert_eq!(first["read_only"], true);
    assert_eq!(first["provider_calls"], 0);
    assert_eq!(before, tree_snapshot(f.root.path()));
    assert!(
        !f.home
            .join("organizations/one/organization/packets")
            .exists()
    );

    assert!(execute(&f.home, "frontier", json!({"id":"missing"})).is_err());
    assert_eq!(before, tree_snapshot(f.root.path()));
    let owner = fs::File::open(f.home.join("organizations/one/organization/owner.lock")).unwrap();
    fs2::FileExt::try_lock_exclusive(&owner).unwrap();
    assert!(
        execute(&f.home, "frontier", json!({"id":"one"}))
            .unwrap_err()
            .to_string()
            .contains("organization_owner_busy")
    );
    assert_eq!(before, tree_snapshot(f.root.path()));
}

#[test]
fn handoff_preserves_scope_and_parent_unknown_but_starts_child_pending() {
    let f = Fixture::new();
    f.stopped_parent();
    let parent = frontier(&f, "parent");
    let request = f.handoff("parent", "child");
    let result = execute(&f.home, "handoff", request.clone()).unwrap();
    let child = frontier(&f, "child");
    let mut expected = parent["snapshot"]["plan"].clone();
    expected["id"] = json!("child");
    expected["host"] = request["host"].clone();
    expected["claims"][0]["role"] = request["changes"][0]["role"].clone();
    expected["claims"][0]["acceptance"] = request["changes"][0]["acceptance"].clone();
    assert_eq!(child["snapshot"]["plan"], expected);
    assert!(child["snapshot"]["heads"].as_object().unwrap().is_empty());
    assert!(
        child["claims"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["status"] == "pending")
    );
    assert_eq!(result["inherited_findings"], 0);
    assert_eq!(result["provider_calls"], 0);
    assert_eq!(frontier(&f, "parent"), parent);
    assert_eq!(parent["claims"][0]["status"], "unknown");
    assert_eq!(child["ready_by_role"]["host"][0]["id"], "label");
    assert!(execute(&f.home, "create", expected).is_err());
}

#[test]
fn handoff_requires_stopped_current_parent_and_a_typed_effective_change() {
    let f = Fixture::new();
    f.create("parent");
    let request = f.handoff("parent", "child");
    assert!(execute(&f.home, "handoff", request.clone()).is_err());
    execute(&f.home, "close", f.close("parent", "stopped")).unwrap();
    let mut variants = Vec::new();
    let mut bad = request.clone();
    bad["expected_heads_sha256"] = json!("0".repeat(64));
    variants.push(bad);
    let mut bad = request.clone();
    bad["expected_parent_plan_sha256"] = json!("0".repeat(64));
    variants.push(bad);
    let mut bad = request.clone();
    bad["changes"] = json!([]);
    variants.push(bad);
    let mut bad = request.clone();
    bad["changes"] = json!([request["changes"][0].clone(), request["changes"][0].clone()]);
    variants.push(bad);
    let mut bad = request.clone();
    bad["changes"][0]["claim_id"] = json!("missing");
    variants.push(bad);
    let mut bad = request.clone();
    bad["changes"][0]["input_ids"] = json!(["secret-new-source"]);
    variants.push(bad);
    let mut bad = request.clone();
    bad["conditions"] = json!(["scope changed"]);
    variants.push(bad);
    let mut bad = request.clone();
    bad["changes"][0]["role"] = json!("agent");
    bad["changes"][0]["acceptance"] = json!("Visible label with source review");
    variants.push(bad);
    for bad in variants {
        assert!(execute(&f.home, "handoff", bad).is_err());
        assert!(!f.home.join("organization-handoffs/child.json").exists());
        assert!(
            !f.home
                .join("organizations/child/organization/plan.json")
                .exists()
        );
    }
    execute(&f.home, "handoff", request).unwrap();

    let completed = Fixture::new();
    completed.create("parent");
    completed.record("parent", "label", "label", "supported");
    completed.record("parent", "unit", "unit", "supported");
    completed.record("parent", "result", "result", "supported");
    execute(
        &completed.home,
        "close",
        completed.close("parent", "completed"),
    )
    .unwrap();
    assert!(
        execute(
            &completed.home,
            "handoff",
            completed.handoff("parent", "child")
        )
        .is_err()
    );
}

#[test]
fn handoff_allows_live_to_development_replay_but_never_promotes_replay() {
    let f = Fixture::new();
    let mut plan = f.plan("parent");
    plan["execution_mode"] = json!("live");
    execute(&f.home, "create", plan).unwrap();
    execute(&f.home, "close", f.close("parent", "stopped")).unwrap();
    let request = f.handoff("parent", "child");
    execute(&f.home, "handoff", request).unwrap();
    assert_eq!(
        frontier(&f, "child")["snapshot"]["plan"]["execution_mode"],
        "development_replay"
    );
    execute(&f.home, "close", f.close("child", "stopped")).unwrap();
    let mut next = f.handoff("child", "grandchild");
    next["changes"][0]["acceptance"] = json!("Another explicit host review under the same scope");
    next["execution_mode"] = json!("live");
    assert!(execute(&f.home, "handoff", next.clone()).is_err());
    next["execution_mode"] = json!("development_replay");
    execute(&f.home, "handoff", next).unwrap();
    assert!(
        frontier(&f, "grandchild")["snapshot"]["heads"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn handoff_never_adopts_an_existing_ordinary_child_even_if_its_plan_matches() {
    let f = Fixture::new();
    f.stopped_parent();
    let request = f.handoff("parent", "child");
    let mut plan = frontier(&f, "parent")["snapshot"]["plan"].clone();
    plan["id"] = json!("child");
    plan["host"] = request["host"].clone();
    plan["claims"][0]["role"] = request["changes"][0]["role"].clone();
    plan["claims"][0]["acceptance"] = request["changes"][0]["acceptance"].clone();
    execute(&f.home, "create", plan).unwrap();
    assert!(
        execute(&f.home, "handoff", request)
            .unwrap_err()
            .to_string()
            .contains("child_already_exists")
    );
    assert!(!f.home.join("organization-handoffs/child.json").exists());
}

#[test]
fn handoff_replays_every_unpublished_prefix_and_reserves_child_from_create() {
    for prefix in 1..=3 {
        let f = Fixture::new();
        f.stopped_parent();
        let request = f.handoff("parent", "child");
        let result = execute(&f.home, "handoff", request.clone()).unwrap();
        let child = f.home.join("organizations/child/organization");
        let outgoing = f
            .home
            .join("organizations/parent/organization/handoffs/handoff-child.json");
        // Reconstruct publication prefixes from the exact successful immutable
        // bytes, keeping displaced suffix artifacts outside the live store.
        fs::rename(
            child.join("plan.json"),
            f.root.path().join("retained-plan.json"),
        )
        .unwrap();
        fs::rename(
            child.join("events"),
            f.root.path().join("retained-child-events"),
        )
        .unwrap();
        if prefix < 3 {
            fs::rename(&outgoing, f.root.path().join("retained-outgoing.json")).unwrap();
        }
        if prefix < 2 {
            fs::rename(
                child.join("lineage.json"),
                f.root.path().join("retained-lineage.json"),
            )
            .unwrap();
        }
        let pending = execute(&f.home, "handoff-status", json!({"id":"child"})).unwrap();
        assert_eq!(pending["state"], "pending_publication");
        assert_eq!(pending["read_only"], true);
        assert!(execute(&f.home, "create", result["lineage"]["child_plan"].clone()).is_err());
        assert!(execute(&f.home, "packet", json!({"id":"child","claim_id":"label"})).is_err());
        let mut changed = request.clone();
        changed["summary"] = json!("A different intent must not overwrite the reservation");
        assert!(execute(&f.home, "handoff", changed).is_err());
        let recovered = execute(&f.home, "handoff", request).unwrap();
        assert_eq!(recovered["replayed"], true);
        assert_eq!(recovered["lineage"], result["lineage"]);
        assert_eq!(
            execute(&f.home, "handoff-status", json!({"id":"child"})).unwrap()["state"],
            "published"
        );
    }
}

#[test]
fn handoff_replay_preserves_child_findings_and_close_and_repairs_events() {
    let f = Fixture::new();
    f.stopped_parent();
    let request = f.handoff("parent", "child");
    execute(&f.home, "handoff", request.clone()).unwrap();
    let mut label = f.finding("child", "label", "child-label", "supported");
    label["producer"] = json!("host");
    execute(&f.home, "record", label).unwrap();
    f.record("child", "unit", "child-unit", "supported");
    f.record("child", "result", "child-result", "supported");
    execute(&f.home, "close", f.close("child", "completed")).unwrap();
    let before = frontier(&f, "child");
    let events = f.home.join("organizations/child/organization/events");
    for entry in fs::read_dir(&events).unwrap() {
        let path = entry.unwrap().path();
        let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if value["kind"] == "organization_handoff" {
            fs::rename(path, f.root.path().join("retained-handoff-event.json")).unwrap();
        }
    }
    let result = execute(&f.home, "handoff", request).unwrap();
    assert_eq!(result["replayed"], true);
    assert_eq!(result["status"]["close"]["request"]["outcome"], "completed");
    assert_eq!(frontier(&f, "child"), before);
    assert!(fs::read_dir(events).unwrap().any(|entry| {
        let value: Value =
            serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
        value["kind"] == "organization_handoff"
    }));
}

#[test]
fn handoff_missing_lineage_and_parent_evidence_drift_fail_closed() {
    let f = Fixture::new();
    f.stopped_parent();
    let request = f.handoff("parent", "child");
    execute(&f.home, "handoff", request.clone()).unwrap();
    let lineage = f.home.join("organizations/child/organization/lineage.json");
    fs::rename(&lineage, f.root.path().join("retained-lineage.json")).unwrap();
    assert!(execute(&f.home, "status", json!({"id":"child"})).is_err());
    assert!(execute(&f.home, "frontier", json!({"id":"child"})).is_err());
    execute(&f.home, "handoff", request.clone()).unwrap();
    fs::write(
        f.root.path().join("parent-label-evidence.txt"),
        "Changed parent evidence",
    )
    .unwrap();
    assert!(execute(&f.home, "status", json!({"id":"child"})).is_err());
    assert!(execute(&f.home, "frontier", json!({"id":"child"})).is_err());
    assert!(execute(&f.home, "handoff", request).is_err());
}

#[test]
fn handoff_concurrent_children_use_bounded_ordered_locks_and_exact_retry() {
    use std::sync::{Arc, Barrier, mpsc};
    use std::time::Duration;
    let f = Fixture::new();
    f.stopped_parent();
    let requests = [
        f.handoff("parent", "a-child"),
        f.handoff("parent", "z-child"),
    ];
    let barrier = Arc::new(Barrier::new(2));
    let (sender, receiver) = mpsc::channel();
    let handles: Vec<_> = requests
        .iter()
        .cloned()
        .map(|request| {
            let barrier = barrier.clone();
            let home = f.home.clone();
            let sender = sender.clone();
            std::thread::spawn(move || {
                barrier.wait();
                sender
                    .send(
                        execute(&home, "handoff", request)
                            .map(|_| ())
                            .map_err(|e| e.to_string()),
                    )
                    .unwrap();
            })
        })
        .collect();
    for _ in 0..2 {
        let result = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("handoff must not deadlock");
        if let Err(error) = result {
            assert!(error.contains("organization_owner_busy"), "{error}");
        }
    }
    for handle in handles {
        handle.join().unwrap();
    }
    for request in requests {
        let child = request["child_id"].as_str().unwrap().to_owned();
        execute(&f.home, "handoff", request).unwrap();
        assert!(
            frontier(&f, &child)["snapshot"]["heads"]
                .as_object()
                .unwrap()
                .is_empty()
        );
    }
}

fn inspection(org: &str, claim: &str, budget: u64) -> Value {
    json!({"schema":"engineering-memory.organization-claim-inspection-request/1",
        "organization_id":org,"claim_id":claim,"max_output_bytes":budget})
}

fn inspect(f: &Fixture, org: &str, claim: &str) -> Value {
    execute(&f.home, "inspect-claim", inspection(org, claim, 1_048_576)).unwrap()
}

/// Paths, modification times and bytes below `root`; used to prove read-only queries.
fn file_tree(root: &Path) -> Vec<(std::path::PathBuf, std::time::SystemTime, Vec<u8>)> {
    let mut rows = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).unwrap();
        let contents = if metadata.is_dir() {
            for child in fs::read_dir(&path).unwrap() {
                pending.push(child.unwrap().path());
            }
            Vec::new()
        } else {
            fs::read(&path).unwrap()
        };
        rows.push((
            path.strip_prefix(root).unwrap().to_path_buf(),
            metadata.modified().unwrap(),
            contents,
        ));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

#[test]
fn inspect_claim_pending_has_no_finding_and_creates_nothing() {
    let f = Fixture::new();
    f.create("one");
    let before = file_tree(f.root.path());
    let label = inspect(&f, "one", "label");
    assert_eq!(label["semantic_status"], "pending");
    assert_eq!(label["current_finding"], Value::Null);
    assert_eq!(label["snapshot"]["claim_head"], Value::Null);
    assert_eq!(label["claim"]["question"], "Read local label");
    assert_eq!(
        label["claim"]["acceptance"],
        "Visible label with source review"
    );
    assert_eq!(label["frontier"]["action"], "prepare_packet");
    assert!(label.get("managed_work").is_none());
    assert_eq!(label["read_only"], true);
    assert_eq!(label["provider_calls"], 0);
    let result = inspect(&f, "one", "result");
    assert_eq!(
        result["snapshot"]["dependency_heads"],
        json!({"label":null})
    );
    assert_eq!(result["frontier"]["blocked_by"][0]["id"], "label");
    let status = execute(&f.home, "status", json!({"id":"one"})).unwrap();
    assert_eq!(result["snapshot"]["heads_sha256"], status["heads_sha256"]);
    assert_eq!(label, inspect(&f, "one", "label"));
    assert!(
        !f.home
            .join("organizations/one/organization/packets")
            .exists()
    );
    assert_eq!(before, file_tree(f.root.path()));
}

#[test]
fn inspect_claim_keeps_semantic_unknown_and_stale_dependency_distinct() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "supported");
    f.record("one", "unit", "unit-1", "supported");
    f.record("one", "result", "result-1", "supported");
    f.record("one", "label", "label-2", "unknown");
    let label = inspect(&f, "one", "label");
    assert_eq!(label["semantic_status"], "unknown");
    assert_eq!(label["current_finding"]["finding"]["id"], "label-2");
    assert_eq!(label["current_finding"]["finding"]["supersedes"], "label-1");
    assert_eq!(
        label["current_finding"]["finding"]["summary"],
        "Explicit test fixture judgment"
    );
    assert_eq!(
        label["current_finding"]["finding"]["review"]["id"],
        "label-2-review-txt"
    );
    assert_eq!(label["frontier"]["action"], "diagnose");
    let result = inspect(&f, "one", "result");
    assert_eq!(result["semantic_status"], "stale");
    // The stale result keeps its own supported finding; only the claim state changes.
    assert_eq!(result["current_finding"]["finding"]["status"], "supported");
    assert_eq!(
        result["current_finding"]["packet"]["dependency_heads"]["label"],
        "label-1"
    );
    assert_eq!(result["snapshot"]["dependency_heads"]["label"], "label-2");
    assert_eq!(result["frontier"]["dependencies"][0]["head_changed"], true);
    assert_eq!(
        result["frontier"]["action"],
        "resolve_dependencies_then_revalidate"
    );
    assert_eq!(inspect(&f, "one", "unit")["semantic_status"], "supported");
}

#[test]
fn inspect_claim_reads_closed_organization_without_reopening_work() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "unknown");
    f.record("one", "unit", "unit-1", "supported");
    execute(&f.home, "close", f.close("one", "stopped")).unwrap();
    let before = file_tree(f.root.path());
    let label = inspect(&f, "one", "label");
    assert_eq!(label["snapshot"]["closed"], true);
    assert_eq!(label["snapshot"]["close"]["request"]["outcome"], "stopped");
    assert_eq!(label["frontier"]["action"], "closed");
    assert_eq!(label["frontier"]["ready"], false);
    assert_eq!(label["semantic_status"], "unknown");
    let result = inspect(&f, "one", "result");
    assert_eq!(result["semantic_status"], "pending");
    assert_eq!(result["frontier"]["action"], "closed");
    assert_eq!(before, file_tree(f.root.path()));
}

#[test]
fn inspect_claim_rejects_invalid_requests_budget_drift_and_busy_lock() {
    let f = Fixture::new();
    f.create("one");
    f.record("one", "label", "label-1", "supported");
    let before = file_tree(f.root.path());
    let error = |input: Value| {
        execute(&f.home, "inspect-claim", input)
            .unwrap_err()
            .to_string()
    };
    assert!(error(inspection("one", "missing", 1_048_576)).contains("claim_unknown"));
    assert!(error(inspection("missing", "label", 1_048_576)).contains("organization_missing"));
    assert!(error(inspection("../one", "label", 1_048_576)).contains("invalid_record_id"));
    assert!(error(inspection("one", "../label", 1_048_576)).contains("invalid_record_id"));
    for budget in [0, 255, 1_048_577] {
        assert!(error(inspection("one", "label", budget)).contains("output_budget_out_of_range"));
    }
    let mut extra = inspection("one", "label", 1_048_576);
    extra["truncate"] = json!(true);
    assert!(error(extra).contains("unknown field"));
    let mut schema = inspection("one", "label", 1_048_576);
    schema["schema"] = json!("engineering-memory.organization-claim-inspection-request/2");
    assert!(error(schema).contains("schema"));
    // The response echoes the request, so find the budget equal to its own output size.
    let size = |budget: u64| {
        let mut value = inspect(&f, "one", "label");
        value["request"]["max_output_bytes"] = json!(budget);
        serde_json::to_vec(&value).unwrap().len() as u64
    };
    let exact = (256..=1_048_576).find(|b| size(*b) == *b).unwrap();
    assert_eq!(size(exact - 1), exact);
    assert!(
        error(inspection("one", "label", exact - 1)).contains(&format!(
            "claim_inspection_output_budget_exceeded:required={exact}"
        ))
    );
    let fitted = execute(&f.home, "inspect-claim", inspection("one", "label", exact)).unwrap();
    assert_eq!(serde_json::to_vec(&fitted).unwrap().len() as u64, exact);
    assert_eq!(
        fitted["current_finding"],
        inspect(&f, "one", "label")["current_finding"]
    );
    assert_eq!(before, file_tree(f.root.path()));

    let owner = fs::File::open(f.home.join("organizations/one/organization/owner.lock")).unwrap();
    fs2::FileExt::try_lock_exclusive(&owner).unwrap();
    assert!(error(inspection("one", "label", 1_048_576)).contains("organization_owner_busy"));
    fs2::FileExt::unlock(&owner).unwrap();
    assert_eq!(before, file_tree(f.root.path()));

    fs::write(f.root.path().join("label-1-review.txt"), "edited later").unwrap();
    assert!(
        execute(
            &f.home,
            "inspect-claim",
            inspection("one", "label", 1_048_576)
        )
        .is_err()
    );
    fs::write(f.root.path().join("image-a.txt"), "changed").unwrap();
    assert!(
        execute(
            &f.home,
            "inspect-claim",
            inspection("one", "unit", 1_048_576)
        )
        .is_err()
    );
}
