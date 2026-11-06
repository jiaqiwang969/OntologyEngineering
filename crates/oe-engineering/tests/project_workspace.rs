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

// These files deliberately share bytes but have distinct paths: the budget is
// a path budget, not a digest budget. Their digest is existing Project evidence.
fn source_capacity_request(f: &Fixture, schema: &str, paths: usize) -> WorkspaceCommitRequest {
    let mut q = f.request("initial-commit.json");
    q.schema = schema.into();
    let raw = fs::read(f.home.join("measurement.txt")).unwrap();
    // Project, anchor, original request and measurement are four distinct paths.
    for i in 4..paths {
        q.sources
            .push(f.put_bytes(&format!("capacity/source-{i}.txt"), &raw));
    }
    q
}

#[test]
fn versioned_source_capacity_checks_exact_path_boundaries_and_cold_resume() {
    for (schema, boundary) in [
        ("oev2.workspace-commit-request/1", 256),
        ("oev2.workspace-commit-request/2", 1024),
        ("oev2.workspace-commit-request/3", 1024),
        ("oev2.workspace-commit-request/4", 2048),
        ("oev2.workspace-commit-request/5", 4096),
    ] {
        for count in [boundary, boundary + 1] {
            let f = Fixture::new();
            f.initialize();
            let q = source_capacity_request(&f, schema, count);
            if count > boundary {
                rejects(
                    workspace::commit(&f.open(), &q),
                    "workspace_source_count_limit",
                );
                assert!(!f.home.join("HEAD.json").exists());
                continue;
            }
            let committed = workspace::commit(&f.open(), &q).unwrap();
            assert_eq!(committed["status"], "committed");
            let resumed = workspace::resume(&f.open()).unwrap();
            assert_eq!(resumed["status"], "ready");
            assert_eq!(resumed["acceptance"][0]["status"], "open");
            assert_eq!(resumed["engineering_adoption"], "not_granted");
            assert_eq!(resumed["root_requests"].as_array().unwrap().len(), 1);
        }
    }
}

fn cached_request(f: &Fixture, raw: &[u8], count: usize) -> WorkspaceCommitRequest {
    let mut q = f.request("initial-commit.json");
    q.schema = "oev2.workspace-commit-request/3".into();
    let mut p = f.value("project.json");
    p["evidence"]["cache-probe"] = json!(oe_core::sha256(raw));
    q.project = f.put("cache-project.json", &p);
    for i in 0..count {
        q.sources.push(f.put_bytes(&format!("cache-{i}.bin"), raw));
    }
    q
}

#[test]
fn cached_profile_cold_resume_preserves_original_bytes_and_legacy_raw_limit() {
    let f = Fixture::new();
    f.initialize();
    let raw = vec![b'Q'; 16 * 1024 * 1024];
    let mut q = cached_request(&f, &raw, 5);
    q.schema = "oev2.workspace-commit-request/2".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    assert!(!f.home.join("HEAD.json").exists());
    q.schema = "oev2.workspace-commit-request/3".into();
    let committed = workspace::commit(&f.open(), &q).unwrap();
    assert_eq!(committed["status"], "committed");
    let c = f.value(committed["head"]["commit"]["path"].as_str().unwrap());
    assert_eq!(c["schema"], "oev2.workspace-commit/3");
    let fresh = workspace::resume(&f.open()).unwrap();
    assert_eq!(fresh["status"], "ready");
    assert_eq!(fresh["task_view"]["schema"], "oev2.current-task-view/4");
    assert!(fresh["source_budget"]["logical_bytes"].as_u64().unwrap() > 80 * 1024 * 1024);
    assert!(
        fresh["source_budget"]["retained_payload_bytes"]
            .as_u64()
            .unwrap()
            < 1024 * 1024
    );
    assert_eq!(
        fresh["task_view"]["source_observations"]
            .as_array()
            .unwrap()
            .len(),
        q.sources.len()
    );
    assert_eq!(fresh["acceptance"][0]["status"], "open");
    assert_eq!(fresh["events"], json!(q.events));
    assert_eq!(fresh["corrections"], json!(q.corrections));
    assert_eq!(
        fs::read(f.home.join(format!("artifacts/{}", oe_core::sha256(&raw)))).unwrap(),
        raw
    );
    assert_eq!(fs::read(f.home.join("cache-4.bin")).unwrap(), raw);
    let parent = serde_json::from_value(committed["head"]["commit"].clone()).unwrap();
    q.expected_parent = Some(parent);
    q.commit_id = "explicit-downgrade".into();
    q.schema = "oev2.workspace-commit-request/2".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    assert_eq!(f.value("HEAD.json"), committed["head"]);
    // Archived copies never stand in for missing or modified original sources.
    fs::write(f.home.join("cache-4.bin"), b"drift").unwrap();
    assert!(workspace::resume(&f.open()).is_err());
    fs::remove_file(f.home.join("cache-4.bin")).unwrap();
    assert!(workspace::resume(&f.open()).is_err());
}

#[test]
fn cached_profile_rejects_incompressible_aggregate_and_oversize_originals() {
    let f = Fixture::new();
    f.initialize();
    let mut state = 0x5eeda5beef123456u64;
    let raw: Vec<u8> = (0..16 * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect();
    let q = cached_request(&f, &raw, 5);
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    assert!(!f.home.join("HEAD.json").exists());
    let q = cached_request(&f, &vec![0; 16 * 1024 * 1024 + 1], 1);
    assert!(workspace::commit(&f.open(), &q).is_err());
    assert!(!f.home.join("HEAD.json").exists());
}

#[test]
fn cached_profile_preserves_conflict_archive_dispatch_and_stale_parent_guards() {
    let f = Fixture::new();
    let (_, parent) = f.initial();
    let mut q = cached_request(&f, &vec![8; 1024 * 1024], 2);
    q.expected_parent = Some(parent);
    q.commit_id = "cache-dispatch".into();
    q.stage.as_mut().unwrap().execution = "dispatched".into();
    let c = workspace::commit(&f.open(), &q).unwrap();
    assert_eq!(c["status"], "committed");
    assert_eq!(
        workspace::resume(&f.open()).unwrap()["status"],
        "effect_unknown"
    );
    q.commit_id = "stale-cache".into();
    assert_eq!(
        workspace::commit(&f.open(), &q).unwrap()["status"],
        "conflict"
    );
    assert_eq!(f.value("HEAD.json"), c["head"]);
    let r = q.sources.last().unwrap().clone();
    fs::write(f.home.join(format!("artifacts/{}", r.sha256)), b"corrupt").unwrap();
    q.expected_parent = Some(serde_json::from_value(c["head"]["commit"].clone()).unwrap());
    q.commit_id = "bad-cache-archive".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_archive_changed",
    );
    assert_eq!(f.value("HEAD.json"), c["head"]);
    let mut conflict = r;
    conflict.sha256 = "0".repeat(64);
    q.sources.push(conflict);
    q.commit_id = "conflicting-cache-path".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_conflicting_reference",
    );
}

#[test]
fn extended_cache_preserves_large_material_and_rejects_downgrade_and_overflow() {
    let f = Fixture::new();
    let (_, parent) = f.initial();
    let initial_head = f.value("HEAD.json");
    let mut state = 0x4a17b9e3f251c68du64;
    let raw: Vec<u8> = (0..16 * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect();
    let mut q = cached_request(&f, &raw, 7);
    q.expected_parent = Some(parent);
    q.commit_id = "extended-dispatch".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    assert_eq!(f.value("HEAD.json"), initial_head);
    q.schema = "oev2.workspace-commit-request/4".into();
    q.stage.as_mut().unwrap().execution = "dispatched".into();
    let committed = workspace::commit(&f.open(), &q).unwrap();
    let fresh = workspace::resume(&f.open()).unwrap();
    assert_eq!(fresh["status"], "effect_unknown");
    assert_eq!(fresh["task_view"]["schema"], "oev2.current-task-view/5");
    assert_eq!(
        fresh["source_budget"]["profile"],
        "oev2.workspace-source-cache/2"
    );
    assert_eq!(fresh["source_budget"]["path_limit"], 2048);
    assert_eq!(
        fresh["source_budget"]["payload_limit_bytes"],
        128 * 1024 * 1024
    );
    assert_eq!(
        fresh["source_budget"]["maximum_logical_bytes"],
        32u64 * 1024 * 1024 * 1024
    );
    assert!(
        fresh["source_budget"]["retained_payload_bytes"]
            .as_u64()
            .unwrap()
            > 112 * 1024 * 1024
    );
    assert_eq!(fresh["acceptance"][0]["status"], "open");
    assert_eq!(fresh["events"], json!(q.events));
    assert_eq!(fresh["corrections"], json!(q.corrections));
    assert_eq!(
        fs::read(f.home.join(format!("artifacts/{}", oe_core::sha256(&raw)))).unwrap(),
        raw
    );
    assert_eq!(fs::read(f.home.join("cache-6.bin")).unwrap(), raw);
    q.commit_id = "stale-extended".into();
    assert_eq!(
        workspace::commit(&f.open(), &q).unwrap()["status"],
        "conflict"
    );
    q.expected_parent = Some(serde_json::from_value(committed["head"]["commit"].clone()).unwrap());
    q.commit_id = "extended-downgrade".into();
    q.schema = "oev2.workspace-commit-request/3".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    q.schema = "oev2.workspace-commit-request/4".into();
    q.commit_id = "extended-overflow".into();
    q.sources.push(f.put_bytes("cache-7.bin", &raw));
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    assert_eq!(f.value("HEAD.json"), committed["head"]);
    q.sources.pop();
    let archive = f.home.join(format!("artifacts/{}", oe_core::sha256(&raw)));
    fs::write(&archive, b"corrupt").unwrap();
    q.commit_id = "extended-bad-archive".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_archive_changed",
    );
    assert_eq!(f.value("HEAD.json"), committed["head"]);
    fs::write(&archive, &raw).unwrap();
    fs::write(f.home.join("cache-6.bin"), b"changed").unwrap();
    assert!(workspace::resume(&f.open()).is_err());
    fs::remove_file(f.home.join("cache-6.bin")).unwrap();
    assert!(workspace::resume(&f.open()).is_err());
}

#[test]
fn extended_cache_keeps_original_file_limit_and_path_downgrade_guard() {
    let f = Fixture::new();
    f.initialize();
    let mut q = cached_request(&f, &vec![0; 16 * 1024 * 1024 + 1], 1);
    q.schema = "oev2.workspace-commit-request/4".into();
    assert!(workspace::commit(&f.open(), &q).is_err());
    assert!(!f.home.join("HEAD.json").exists());
    let mut q = source_capacity_request(&f, "oev2.workspace-commit-request/4", 1025);
    let committed = workspace::commit(&f.open(), &q).unwrap();
    q.expected_parent = Some(serde_json::from_value(committed["head"]["commit"].clone()).unwrap());
    q.commit_id = "path-downgrade".into();
    q.schema = "oev2.workspace-commit-request/3".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_count_limit",
    );
    assert_eq!(f.value("HEAD.json"), committed["head"]);
}

#[test]
fn large_extended_cache_preserves_material_and_rejects_downgrade_and_overflow() {
    let f = Fixture::new();
    let (_, parent) = f.initial();
    let initial_head = f.value("HEAD.json");
    let mut state = 0x6c8e9cf570932bd5u64;
    let raw: Vec<u8> = (0..16 * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect();
    // Fifteen incompressible 16 MiB originals exceed /4 but fit /5 with the Project files.
    let mut q = cached_request(&f, &raw, 15);
    q.schema = "oev2.workspace-commit-request/4".into();
    q.expected_parent = Some(parent);
    q.commit_id = "large-extended-dispatch".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    assert_eq!(f.value("HEAD.json"), initial_head);
    q.schema = "oev2.workspace-commit-request/5".into();
    q.stage.as_mut().unwrap().execution = "dispatched".into();
    let committed = workspace::commit(&f.open(), &q).unwrap();
    let c = f.value(committed["head"]["commit"]["path"].as_str().unwrap());
    assert_eq!(c["schema"], "oev2.workspace-commit/5");
    let fresh = workspace::resume(&f.open()).unwrap();
    assert_eq!(fresh["status"], "effect_unknown");
    assert_eq!(fresh["task_view"]["schema"], "oev2.current-task-view/6");
    assert_eq!(
        fresh["source_budget"]["profile"],
        "oev2.workspace-source-cache/3"
    );
    assert_eq!(fresh["source_budget"]["path_limit"], 4096);
    assert_eq!(
        fresh["source_budget"]["payload_limit_bytes"],
        256 * 1024 * 1024
    );
    assert_eq!(
        fresh["source_budget"]["maximum_logical_bytes"],
        64u64 * 1024 * 1024 * 1024
    );
    assert!(
        fresh["source_budget"]["retained_payload_bytes"]
            .as_u64()
            .unwrap()
            > 240 * 1024 * 1024
    );
    assert_eq!(fresh["acceptance"][0]["status"], "open");
    assert_eq!(fresh["events"], json!(q.events));
    assert_eq!(fresh["corrections"], json!(q.corrections));
    assert_eq!(
        fs::read(f.home.join(format!("artifacts/{}", oe_core::sha256(&raw)))).unwrap(),
        raw
    );
    assert_eq!(fs::read(f.home.join("cache-14.bin")).unwrap(), raw);
    q.commit_id = "stale-large-extended".into();
    assert_eq!(
        workspace::commit(&f.open(), &q).unwrap()["status"],
        "conflict"
    );
    q.expected_parent = Some(serde_json::from_value(committed["head"]["commit"].clone()).unwrap());
    q.commit_id = "large-extended-downgrade".into();
    q.schema = "oev2.workspace-commit-request/4".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    q.schema = "oev2.workspace-commit-request/5".into();
    q.commit_id = "large-extended-overflow".into();
    q.sources.push(f.put_bytes("cache-15.bin", &raw));
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    assert_eq!(f.value("HEAD.json"), committed["head"]);
    q.sources.pop();
    let archive = f.home.join(format!("artifacts/{}", oe_core::sha256(&raw)));
    fs::write(&archive, b"corrupt").unwrap();
    q.commit_id = "large-extended-bad-archive".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_archive_changed",
    );
    assert_eq!(f.value("HEAD.json"), committed["head"]);
    fs::write(&archive, &raw).unwrap();
    fs::write(f.home.join("cache-14.bin"), b"changed").unwrap();
    assert!(workspace::resume(&f.open()).is_err());
    fs::remove_file(f.home.join("cache-14.bin")).unwrap();
    assert!(workspace::resume(&f.open()).is_err());
}

#[test]
fn large_extended_cache_keeps_original_file_limit_and_path_downgrade_guard() {
    let f = Fixture::new();
    f.initialize();
    let mut q = cached_request(&f, &vec![0; 16 * 1024 * 1024 + 1], 1);
    q.schema = "oev2.workspace-commit-request/5".into();
    assert!(workspace::commit(&f.open(), &q).is_err());
    assert!(!f.home.join("HEAD.json").exists());
    let mut q = source_capacity_request(&f, "oev2.workspace-commit-request/5", 2049);
    let committed = workspace::commit(&f.open(), &q).unwrap();
    q.expected_parent = Some(serde_json::from_value(committed["head"]["commit"].clone()).unwrap());
    q.commit_id = "large-path-downgrade".into();
    q.schema = "oev2.workspace-commit-request/4".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_count_limit",
    );
    assert_eq!(f.value("HEAD.json"), committed["head"]);
}

// Fifteen MiB of event text per commit gives a real history over 64 MiB in a
// few commits; each commit keeps its earlier events, so every file stays below
// the 16 MiB artifact limit.
#[test]
fn versioned_history_capacity_crosses_the_legacy_budget_only_under_explicit_v6() {
    let f = Fixture::new();
    let (mut q, mut parent) = f.initial();
    let mut large = serde_json::to_value(&q.events[0]).unwrap();
    large["id"] = json!("E-LARGE-HISTORY");
    large["description"] = json!("history ".repeat(15 * 1024 * 1024 / 8));
    q.events.push(serde_json::from_value(large).unwrap());
    q.schema = "oev2.workspace-commit-request/5".into();
    for i in 0..4 {
        q.expected_parent = Some(parent.clone());
        q.commit_id = format!("legacy-history-{i}");
        let committed = workspace::commit(&f.open(), &q).unwrap();
        assert_eq!(committed["status"], "committed");
        parent = serde_json::from_value(committed["head"]["commit"].clone()).unwrap();
    }
    let legacy = workspace::resume(&f.open()).unwrap();
    assert_eq!(legacy["status"], "ready");
    assert!(legacy.get("history_budget").is_none());
    let head = f.value("HEAD.json");
    q.expected_parent = Some(parent.clone());
    q.commit_id = "legacy-history-over".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_history_byte_budget",
    );
    assert_eq!(f.value("HEAD.json"), head);
    assert!(!f.home.join("commits/legacy-history-over").exists());
    q.schema = "oev2.workspace-commit-request/6".into();
    q.commit_id = "large-history".into();
    let committed = workspace::commit(&f.open(), &q).unwrap();
    let c = f.value(committed["head"]["commit"]["path"].as_str().unwrap());
    assert_eq!(c["schema"], "oev2.workspace-commit/6");
    let fresh = workspace::resume(&f.open()).unwrap();
    assert_eq!(fresh["status"], "ready");
    assert_eq!(fresh["events"], json!(q.events));
    assert_eq!(fresh["acceptance"][0]["status"], "open");
    assert_eq!(fresh["task_view"]["schema"], "oev2.current-task-view/6");
    assert_eq!(
        fresh["source_budget"]["profile"],
        "oev2.workspace-source-cache/3"
    );
    let budget = &fresh["history_budget"];
    assert_eq!(budget["profile"], "oev2.workspace-history/2");
    assert_eq!(budget["commits"], 6);
    assert_eq!(budget["commit_limit"], 1024);
    assert_eq!(budget["commit_limit_bytes"], 256 * 1024 * 1024);
    assert_eq!(budget["archive_limit_bytes"], 256 * 1024 * 1024);
    let on_disk: u64 = fs::read_dir(f.home.join("commits"))
        .unwrap()
        .map(|d| {
            fs::metadata(d.unwrap().path().join("commit.json"))
                .unwrap()
                .len()
        })
        .sum();
    assert_eq!(budget["commit_bytes"], on_disk);
    assert!(on_disk > 64 * 1024 * 1024);
    let head = f.value("HEAD.json");
    q.expected_parent = Some(serde_json::from_value(committed["head"]["commit"].clone()).unwrap());
    q.commit_id = "large-history-downgrade".into();
    q.schema = "oev2.workspace-commit-request/5".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_history_byte_budget",
    );
    assert_eq!(f.value("HEAD.json"), head);
    assert!(!f.home.join("commits/large-history-downgrade").exists());
}

#[test]
fn versioned_source_capacity_migration_preserves_dispatch_and_drift_guards() {
    let f = Fixture::new();
    let (_, parent) = f.initial();
    let mut q = source_capacity_request(&f, "oev2.workspace-commit-request/2", 300);
    q.commit_id = "expanded-dispatch".into();
    q.expected_parent = Some(parent.clone());
    q.stage.as_mut().unwrap().execution = "dispatched".into();
    let result = workspace::commit(&f.open(), &q).unwrap();
    assert_eq!(result["status"], "committed");
    let next: ArtifactReference = serde_json::from_value(result["head"]["commit"].clone()).unwrap();
    assert_eq!(f.value(&next.path)["schema"], "oev2.workspace-commit/2");
    let recovered = workspace::resume(&f.open()).unwrap();
    assert_eq!(recovered["status"], "effect_unknown");
    rejects(
        workspace::begin(&f.open(), &next, "S1"),
        "workspace_not_ready_for_stage",
    );
    rejects(
        workspace::begin(&f.open(), &parent, "S1"),
        "workspace_stale_resume",
    );
    let head = fs::read(f.home.join("HEAD.json")).unwrap();
    q.expected_parent = Some(next);
    q.commit_id = "legacy-downgrade".into();
    q.schema = "oev2.workspace-commit-request/1".into();
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_count_limit",
    );
    q.schema = "oev2.workspace-commit-request/2".into();
    q.commit_id = "hide-expanded-dispatch".into();
    q.stage = None;
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_unfinished_action_cannot_disappear",
    );
    fs::write(
        f.home.join("capacity/source-299.txt"),
        b"changed current evidence",
    )
    .unwrap();
    rejects(workspace::resume(&f.open()), "artifact_source_changed");
    assert_eq!(fs::read(f.home.join("HEAD.json")).unwrap(), head);
}

#[test]
fn versioned_source_capacity_rejects_unknown_schema_and_mixed_envelope() {
    for version in [2, 3, 4, 5] {
        let f = Fixture::new();
        f.initialize();
        let mut q = source_capacity_request(&f, "oev2.workspace-commit-request/99", 4);
        assert!(workspace::commit(&f.open(), &q).is_err());
        assert!(!f.home.join("HEAD.json").exists());
        q.schema = format!("oev2.workspace-commit-request/{version}");
        let result = workspace::commit(&f.open(), &q).unwrap();
        let mut head = result["head"].clone();
        let path = head["commit"]["path"].as_str().unwrap().to_owned();
        let mut commit = f.value(&path);
        commit["schema"] = json!("oev2.workspace-commit/1");
        head["commit"] = json!(f.put(&path, &commit));
        f.put("HEAD.json", &head); // fault injection in this isolated synthetic store
        rejects(
            workspace::resume(&f.open()),
            "workspace_commit_schema_mismatch",
        );
    }
}

#[test]
fn versioned_source_capacity_retains_aggregate_and_single_file_budgets() {
    let f = Fixture::new();
    f.initialize();
    let mut q = source_capacity_request(&f, "oev2.workspace-commit-request/2", 4);
    let raw = vec![b'x'; 16 * 1024 * 1024];
    for i in 0..4 {
        q.sources.push(f.put_bytes(&format!("large-{i}.txt"), &raw));
    }
    rejects(
        workspace::commit(&f.open(), &q),
        "workspace_source_byte_limit",
    );
    q.sources.truncate(1);
    q.sources
        .push(f.put_bytes("oversize.txt", &vec![b'x'; raw.len() + 1]));
    assert!(workspace::commit(&f.open(), &q).is_err());
    assert!(!f.home.join("HEAD.json").exists());
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
