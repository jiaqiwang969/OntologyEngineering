//! Public synthetic records exercise the durable owner itself, without models,
//! professional tools, or any claim of physical engineering acceptance.
use oe_contracts::{ArtifactReference, WorkspaceCommitRequest};
use oe_core::SkillRoot;
use oe_engineering::project_workspace as workspace;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    _install: tempfile::TempDir,
    _store: tempfile::TempDir,
    install: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let install = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let install_path = fs::canonicalize(install.path()).unwrap();
        let home = fs::canonicalize(store.path()).unwrap();
        fs::write(install_path.join("SKILL.md"), "software identity fixture").unwrap();
        let sources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/project-workspace/fixtures");
        for file in fs::read_dir(sources).unwrap() {
            let file = file.unwrap();
            fs::copy(file.path(), home.join(file.file_name())).unwrap();
        }
        Self {
            _install: install,
            _store: store,
            install: install_path,
            home,
        }
    }

    fn open(&self) -> SkillRoot {
        SkillRoot::new(&self.install)
            .unwrap()
            .with_project_data_root(&self.home)
            .unwrap()
    }

    fn initialize(&self) {
        workspace::initialize(&self.open(), "bracket-demo", "bracket-continuation-demo").unwrap();
    }

    fn value(&self, path: &str) -> Value {
        serde_json::from_slice(&fs::read(self.home.join(path)).unwrap()).unwrap()
    }

    fn request(&self, name: &str) -> WorkspaceCommitRequest {
        serde_json::from_value(self.value(name)).unwrap()
    }

    fn put_bytes(&self, path: &str, bytes: &[u8]) -> ArtifactReference {
        let destination = self.home.join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, bytes).unwrap();
        ArtifactReference {
            path: path.into(),
            sha256: oe_core::sha256(bytes),
        }
    }

    fn put(&self, path: &str, value: &Value) -> ArtifactReference {
        self.put_bytes(path, &serde_json::to_vec_pretty(value).unwrap())
    }

    fn initial(&self) -> (WorkspaceCommitRequest, ArtifactReference) {
        self.initialize();
        let q = self.request("initial-commit.json");
        let result = workspace::commit(&self.open(), &q).unwrap();
        assert_eq!(result["status"], "committed");
        (
            q,
            serde_json::from_value(result["head"]["commit"].clone()).unwrap(),
        )
    }
}

fn rejects<T: std::fmt::Debug>(result: anyhow::Result<T>, reason: &str) {
    let error = result.unwrap_err();
    assert!(
        format!("{error:#}").contains(reason),
        "expected {reason}, got {error:#}"
    );
}

#[test]
fn independent_sessions_reopen_external_store_without_installing_a_skill_in_it() {
    let f = Fixture::new();
    let (q, reference) = f.initial();
    assert!(!f.home.join("SKILL.md").exists());
    assert!(!f.install.join("HEAD.json").exists());
    let manifest = f.value("manifest.json");
    assert_eq!(manifest["workspace_id"], "bracket-demo");
    assert_eq!(manifest["project_id"], "bracket-continuation-demo");
    let first_session = workspace::resume(&f.open()).unwrap();
    let second_session = workspace::resume(&f.open()).unwrap();
    assert_eq!(first_session, second_session);
    assert_eq!(second_session["status"], "ready");
    assert_eq!(second_session["head"]["commit"], json!(reference));
    assert_eq!(second_session["acceptance"][0]["status"], "open");
    assert_eq!(second_session["automatic_dispatch"], false);
    assert_eq!(second_session["engineering_adoption"], "not_granted");
    assert_eq!(
        second_session["task_view"]["decisions"][0]["current_value"].is_string(),
        true
    );
    assert_eq!(f.value("project.json")["decisions"][0]["adopted"], false);
    assert_eq!(second_session["events"], json!(q.events));
    let check = workspace::begin(&f.open(), &reference, "S1").unwrap();
    assert_eq!(check["status"], "ready");
    assert_eq!(check["automatic_dispatch"], false);
    workspace::render_resume(&f.open(), Path::new("views/session-two.md")).unwrap();
    let text = fs::read_to_string(f.home.join("views/session-two.md")).unwrap();
    assert!(text.contains("Complete acceptance") && text.contains("A1"));
    assert!(workspace::render_resume(&f.open(), Path::new("views/session-two.md")).is_err());
    assert!(workspace::initialize(&f.open(), "other", "different-project").is_err());
    assert_eq!(f.value("manifest.json"), manifest);
}

#[test]
fn stale_parent_preserves_candidate_and_never_replaces_current_head() {
    let f = Fixture::new();
    let (mut stale, initial) = f.initial();
    let before = fs::read(f.home.join("HEAD.json")).unwrap();
    stale.commit_id = "stale-session".into();
    let conflict = workspace::commit(&f.open(), &stale).unwrap();
    assert_eq!(conflict["status"], "conflict");
    assert_eq!(fs::read(f.home.join("HEAD.json")).unwrap(), before);
    assert_eq!(conflict["current"]["commit"], json!(initial));
    assert!(f.home.join("commits/stale-session/commit.json").is_file());
    assert!(f.home.join("commits/stale-session/conflict.json").is_file());
    assert_eq!(
        workspace::resume(&f.open()).unwrap()["head"]["commit"],
        json!(initial)
    );
}

#[test]
fn source_drift_refuses_resume_and_never_searches_the_installation_for_old_bytes() {
    let f = Fixture::new();
    let (q, _) = f.initial();
    let head = fs::read(f.home.join("HEAD.json")).unwrap();
    let original = fs::read(f.home.join("measurement.txt")).unwrap();
    fs::write(
        f.home.join("measurement.txt"),
        b"changed load without a new source revision",
    )
    .unwrap();
    rejects(workspace::resume(&f.open()), "artifact_source_changed");
    fs::write(f.install.join("measurement.txt"), &original).unwrap();
    fs::remove_file(f.home.join("measurement.txt")).unwrap();
    assert!(workspace::resume(&f.open()).is_err());
    assert_eq!(fs::read(f.home.join("HEAD.json")).unwrap(), head);
    assert_eq!(
        fs::read(f.home.join(format!("artifacts/{}", q.sources[0].sha256))).unwrap(),
        original
    );
}

#[test]
fn acknowledged_correction_survives_resume_and_cannot_be_claimed_resolved() {
    let f = Fixture::new();
    let (_, parent) = f.initial();
    let mut q = f.request("corrected-commit.template.json");
    q.expected_parent = Some(parent);
    let result = workspace::commit(&f.open(), &q).unwrap();
    assert_eq!(result["status"], "committed");
    let resumed = workspace::resume(&f.open()).unwrap();
    assert_eq!(resumed["corrections"][0]["status"], "acknowledged");
    assert_eq!(
        resumed["root_requests"][1]["text"],
        f.value("corrected-anchor.json")["requests"][1]["text"]
    );
    assert_eq!(resumed["acceptance"][0]["status"], "open");
    let committed_head = fs::read(f.home.join("HEAD.json")).unwrap();
    q.expected_parent = Some(serde_json::from_value(result["head"]["commit"].clone()).unwrap());
    q.commit_id = "unearned-resolution".into();
    q.corrections[0].status = "resolved".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_correction_resolution_missing",
    );
    assert_eq!(fs::read(f.home.join("HEAD.json")).unwrap(), committed_head);

    // Closing the acceptance cannot hide an acknowledged obligation even if
    // someone supplies a Project declaration that now says supported.
    q.commit_id = "unearned-closure".into();
    q.corrections[0].status = "acknowledged".into();
    let mut project = f.value("project.json");
    project["revision"] = json!("r2");
    project["decisions"][0]["conclusion"] = json!("supported");
    q.project = f.put("project-declared-supported.json", &project);
    let mut anchor = f.value("corrected-anchor.json");
    anchor["acceptance"][0]["status"] = json!("closed");
    q.anchor = f.put("anchor-declared-closed.json", &anchor);
    assert!(workspace::commit(&f.open(), &q).is_err());
    assert_eq!(fs::read(f.home.join("HEAD.json")).unwrap(), committed_head);
}

#[test]
fn request_history_and_root_goal_cannot_be_rewritten_without_new_user_source() {
    let f = Fixture::new();
    let (mut q, parent) = f.initial();
    q.expected_parent = Some(parent);
    q.commit_id = "rewritten-request".into();
    let mut anchor = f.value("route-anchor.json");
    anchor["requests"][0]["text"] = json!("A narrower replacement request");
    q.requests[0].source = f.put_bytes("rewritten-request.txt", b"A narrower replacement request");
    q.anchor = f.put("rewritten-anchor.json", &anchor);
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_request_history_rewritten",
    );
    let mut q = f.request("initial-commit.json");
    q.expected_parent =
        Some(serde_json::from_value(f.value("HEAD.json")["commit"].clone()).unwrap());
    q.commit_id = "silent-goal-change".into();
    let mut anchor = f.value("route-anchor.json");
    anchor["deliverable"] = json!("Only a drawing, with no strength evaluation");
    q.anchor = f.put("narrowed-goal.json", &anchor);
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_goal_change_needs_source_request",
    );
    assert_eq!(
        workspace::resume(&f.open()).unwrap()["root_requests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn dispatched_stage_is_not_repeated_or_erased_after_context_recovery() {
    let f = Fixture::new();
    let (mut q, parent) = f.initial();
    q.expected_parent = Some(parent.clone());
    q.commit_id = "dispatch-record".into();
    q.stage.as_mut().unwrap().execution = "dispatched".into();
    let result = workspace::commit(&f.open(), &q).unwrap();
    assert_eq!(result["status"], "committed");
    let next: ArtifactReference = serde_json::from_value(result["head"]["commit"].clone()).unwrap();
    let resumed = workspace::resume(&f.open()).unwrap();
    assert_eq!(resumed["status"], "effect_unknown");
    assert_eq!(resumed["effects_require_readback"], true);
    rejects(
        workspace::begin(&f.open(), &next, "S1"),
        "workspace_not_ready_for_stage",
    );
    rejects(
        workspace::begin(&f.open(), &parent, "S1"),
        "workspace_stale_resume",
    );
    q.expected_parent = Some(next);
    q.commit_id = "hide-dispatch".into();
    q.stage = None;
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_unfinished_action_cannot_disappear",
    );
}

#[test]
fn arbitrary_completed_flag_or_bare_receipt_cannot_finish_a_stage() {
    let f = Fixture::new();
    let (mut q, parent) = f.initial();
    q.expected_parent = Some(parent);
    q.commit_id = "skip-execution".into();
    q.stage.as_mut().unwrap().execution = "completed".into();
    assert!(workspace::commit(&f.open(), &q).is_err());
    q.stage.as_mut().unwrap().execution = "results_recorded".into();
    q.stage.as_mut().unwrap().receipt =
        Some(f.put("fake-receipt.json", &json!({"status":"completed"})));
    assert!(workspace::commit(&f.open(), &q).is_err());
    assert_eq!(
        workspace::resume(&f.open()).unwrap()["stage"]["execution"],
        "planned"
    );
}

#[test]
fn manifest_rejects_cross_workspace_and_cross_project_binding() {
    let f = Fixture::new();
    f.initialize();
    let mut q = f.request("initial-commit.json");
    q.workspace_id = "some-other-workspace".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_identity_mismatch",
    );
    let g = Fixture::new();
    workspace::initialize(&g.open(), "bracket-demo", "different-project").unwrap();
    let q = g.request("initial-commit.json");
    rejects(
        workspace::commit(&g.open(), &q),
        "workspace_project_identity_mismatch",
    );
    assert!(!f.home.join("HEAD.json").exists() && !g.home.join("HEAD.json").exists());
    rejects(
        workspace::initialize(&SkillRoot::new(&f.install).unwrap(), "w", "p"),
        "workspace_explicit_project_home_required",
    );
}

#[test]
fn root_correction_invalidates_old_rrsi_goal_before_any_candidate_freeze() {
    let f = Fixture::new();
    let (_, parent) = f.initial();
    let exported =
        workspace::learning_goal(&f.open(), Path::new("learning/old-goal.json")).unwrap();
    let original_goal = fs::read(f.home.join("learning/old-goal.json")).unwrap();
    let mut q = f.request("corrected-commit.template.json");
    q.expected_parent = Some(parent);
    assert_eq!(
        workspace::commit(&f.open(), &q).unwrap()["status"],
        "committed"
    );
    // Deliberately no legacy RRSI fields: stale root binding must reject first.
    rejects(
        workspace::learning_freeze(
            &f.open(),
            &json!({"goal":exported["goal"]}),
            Path::new("learning/stale-candidate"),
        ),
        "workspace_learning_root_changed",
    );
    assert!(!f.home.join("learning/stale-candidate").exists());
    assert_eq!(
        fs::read(f.home.join("learning/old-goal.json")).unwrap(),
        original_goal
    );
    workspace::learning_goal(&f.open(), Path::new("learning/current-goal.json")).unwrap();
    assert_eq!(
        f.value("learning/current-goal.json")["root_anchor"],
        json!(q.anchor)
    );
    assert_eq!(
        f.value("learning/current-goal.json")["acceptance_ids"],
        json!(["A1"])
    );
}

#[test]
fn concurrent_writers_publish_one_head_and_retain_the_other_candidate() {
    let f = Fixture::new();
    let (q, parent) = f.initial();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let workers: Vec<_> = ["parallel-a", "parallel-b"]
            .into_iter()
            .map(|id| {
                let root = f.open();
                let mut proposal = q.clone();
                proposal.commit_id = id.into();
                proposal.expected_parent = Some(parent.clone());
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    workspace::commit(&root, &proposal).unwrap()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results
            .iter()
            .filter(|r| r["status"] == "committed")
            .count(),
        1
    );
    assert_eq!(
        results.iter().filter(|r| r["status"] == "conflict").count(),
        1
    );
    for name in ["parallel-a", "parallel-b"] {
        assert!(f.home.join(format!("commits/{name}/commit.json")).is_file());
    }
    let winner = results.iter().find(|r| r["status"] == "committed").unwrap();
    assert_eq!(
        workspace::resume(&f.open()).unwrap()["head"],
        winner["head"]
    );
}
