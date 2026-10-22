//! Independent public counterexamples for the project continuation owner.
//! Files are synthetic. No model, service, native tool or engineering adoption.
#![cfg(unix)]

use oe_contracts::{ArtifactReference, WorkspaceCommitRequest};
use oe_core::{SkillRoot, sha256};
use oe_engineering::project_workspace as workspace;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

struct Case {
    _directory: tempfile::TempDir,
    root: SkillRoot,
    project: Value,
    anchor: Value,
    sources: Vec<ArtifactReference>,
    request: ArtifactReference,
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn route_stage(id: &str, object: &str, root_object: &str) -> Value {
    json!({"id":id,"object_id":object,"relation":if object==root_object {"root"} else {"component"},
        "relation_target":root_object,"advances":["A1"],"outcome":"advanced"})
}

impl Case {
    fn new(project: Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(directory.path()).unwrap();
        let software = base.join("software");
        let data = base.join("project/.engineering");
        fs::create_dir(&software).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(software.join("SKILL.md"), "synthetic software root").unwrap();
        let root = SkillRoot::new(software)
            .unwrap()
            .with_project_data_root(data)
            .unwrap();
        let purpose = "Keep the complete requirement and recheck changed conditions.";
        let request = Self::write_bytes(&root, "root-user.txt", purpose.as_bytes());
        let root_object = if project["objects"].get("part").is_some() {
            "part"
        } else {
            "urn:fixture:part"
        };
        let mut stages = vec![route_stage("S1", root_object, root_object)];
        for object in project["objects"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|id| id.as_str() != root_object)
        {
            stages.push(route_stage(
                &format!("S{}", stages.len() + 1),
                object,
                root_object,
            ));
        }
        let last_stage = stages.last().unwrap()["id"].clone();
        let anchor = json!({"schema":"oev2.route-anchor/1","project_id":project["project_id"],"root_object":root_object,
            "deliverable":"Complete source-bound review with unresolved conditions retained",
            "requests":[{"id":"U1","kind":"root","text":purpose,"source":"root-user.txt","affects":[]}],
            "functions":[{"id":"F1","statement":"Keep the declared function","structures":[root_object]}],
            "acceptance":[{"id":"A1","statement":"Every declared condition and downstream obligation remains visible","functions":["F1"],"status":"open"}],
            "stages":stages,"reviews":[{"id":"R1","after_stage":last_stage,"source":"synthetic route review"}],
            "policy":{"off_root_stage_limit":2,"no_progress_stage_limit":2,"review_interval":5}});
        workspace::initialize(
            &root,
            "workspace-test",
            project["project_id"].as_str().unwrap(),
        )
        .unwrap();
        Self {
            _directory: directory,
            root,
            project,
            anchor,
            sources: vec![],
            request,
        }
    }

    fn simple() -> Self {
        Self::new(
            json!({"schema":"oev2.project/1","project_id":"workspace-test-project","revision":"p1",
            "purpose":"Retain complete source-bound engineering work","objects":{"part":"r1"},
            "conditions":{},"evidence":{},"decisions":[],"dependencies":{}}),
        )
    }

    fn condition_fixture() -> (Self, PathBuf) {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/current-task-view/condition-change");
        let mut case = Self::new(read(&fixture.join("before.project.json")));
        for name in [
            "primary.txt",
            "downstream.txt",
            "independent.txt",
            "unrelated.txt",
        ] {
            case.sources.push(Self::write_bytes(
                &case.root,
                &format!("sources/{name}"),
                &fs::read(fixture.join(name)).unwrap(),
            ));
        }
        (case, fixture)
    }

    fn write_bytes(root: &SkillRoot, name: &str, bytes: &[u8]) -> ArtifactReference {
        let path = root.data_path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        ArtifactReference {
            path: name.into(),
            sha256: sha256(bytes),
        }
    }

    fn pin(&self, name: &str, value: &Value) -> ArtifactReference {
        Self::write_bytes(&self.root, name, &serde_json::to_vec_pretty(value).unwrap())
    }

    fn proposal(&self, id: &str, parent: Option<&Value>) -> Value {
        json!({"schema":"oev2.workspace-commit-request/1","workspace_id":"workspace-test","commit_id":id,
            "expected_parent":parent,"project":self.pin("project.json",&self.project),"anchor":self.pin("anchor.json",&self.anchor),
            "requests":[{"request_id":"U1","source":self.request}],"corrections":[],"causal_claims":[],"reviews":[],
            "events":[],"sources":self.sources,"work":null,"checkpoint":null,"stage":null})
    }

    fn commit(&self, proposal: Value) -> anyhow::Result<Value> {
        let request: WorkspaceCommitRequest = serde_json::from_value(proposal)?;
        workspace::commit(&self.root, &request)
    }

    fn committed(&self, proposal: Value) -> Value {
        let result = self.commit(proposal).unwrap();
        assert_eq!(result["status"], "committed", "{result:#}");
        result["head"]["commit"].clone()
    }

    fn change_current_commit(&self, change: impl FnOnce(&mut Value)) {
        let mut head = read(&self.root.data_path().join("HEAD.json"));
        let path = head["commit"]["path"].as_str().unwrap().to_owned();
        let mut commit = read(&self.root.data_path().join(&path));
        change(&mut commit);
        let reference = self.pin(&path, &commit);
        head["sequence"] = commit["sequence"].clone();
        head["commit"] = json!(reference);
        self.pin("HEAD.json", &head);
    }

    fn pending(&self, reference: &Value) -> BTreeSet<String> {
        read(
            &self
                .root
                .data_path()
                .join(reference["path"].as_str().unwrap()),
        )["pending_decisions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }
}

#[test]
fn recomputing_head_hash_cannot_hide_missing_or_corrupted_parent_history() {
    for change in ["missing-parent", "broken-parent-hash", "bad-sequence"] {
        let case = Case::simple();
        let first = case.committed(case.proposal("first", None));
        case.committed(case.proposal("second", Some(&first)));
        case.change_current_commit(|c| match change {
            "missing-parent" => c["proposal"]["expected_parent"] = Value::Null,
            "broken-parent-hash" => {
                c["proposal"]["expected_parent"]["sha256"] = json!("0".repeat(64))
            }
            "bad-sequence" => c["sequence"] = json!(7),
            _ => unreachable!(),
        });
        assert!(workspace::resume(&case.root).is_err(), "accepted {change}");
    }
    let case = Case::simple();
    let first = case.committed(case.proposal("first", None));
    case.committed(case.proposal("second", Some(&first)));
    let path = case.root.data_path().join(first["path"].as_str().unwrap());
    let mut earlier = read(&path);
    earlier["proposal"]["workspace_id"] = json!("different-workspace");
    fs::write(path, serde_json::to_vec(&earlier).unwrap()).unwrap();
    assert!(
        workspace::resume(&case.root).is_err(),
        "earlier commit bytes were not verified"
    );
}

#[test]
fn finished_attempt_identity_cannot_be_reused_after_another_stage() {
    let mut case = Case::simple();
    let stage = |id: &str, execution: &str| json!({"id":id,"acceptance_ids":["A1"],"intent":"Read the explicitly requested source", "execution":execution,"receipt":null});
    let mut q = case.proposal("first", None);
    q["stage"] = stage("S1", "planned");
    let first = case.committed(q);
    let mut q = case.proposal("cancel-first", Some(&first));
    q["stage"] = stage("S1", "cancelled");
    let cancelled = case.committed(q);
    case.anchor["stages"]
        .as_array_mut()
        .unwrap()
        .push(route_stage("S2", "part", "part"));
    case.anchor["reviews"][0]["after_stage"] = json!("S2");
    let mut q = case.proposal("second", Some(&cancelled));
    q["stage"] = stage("S2", "planned");
    let second = case.committed(q);
    let mut q = case.proposal("cancel-second", Some(&second));
    q["stage"] = stage("S2", "cancelled");
    let last = case.committed(q);
    let mut q = case.proposal("reuse-first", Some(&last));
    q["stage"] = stage("S1", "planned");
    assert!(
        case.commit(q).is_err(),
        "an old attempt ID must not dispatch again"
    );
    assert_eq!(
        read(&case.root.data_path().join("HEAD.json"))["commit"],
        last
    );
}

#[test]
fn changed_conditions_remain_pending_across_a_later_unrelated_commit() {
    let (mut case, fixture) = Case::condition_fixture();
    let first = case.committed(case.proposal("initial", None));
    // Same path and same Project revision: raw source identity must preserve the
    // earlier conditions through the archived parent, not reopen the new file.
    case.project = read(&fixture.join("current.project.json"));
    let changed = case.committed(case.proposal("changed-conditions", Some(&first)));
    let expected = BTreeSet::from(["select".to_owned(), "cost".to_owned()]);
    assert_eq!(case.pending(&changed), expected);
    let statement = "A later session only recorded progress; no review was performed.";
    let note = Case::write_bytes(&case.root, "session-note.txt", statement.as_bytes());
    let mut q = case.proposal("later-session", Some(&changed));
    q["events"] = json!([{"id":"note-1","kind":"assistant_claim","actor":"synthetic-test",
        "description":statement,"occurred_at":null,"recorded_at":"2026-10-22T13:00:00Z",
        "sources":[note],"corrects":[],"acceptance_ids":[]}]);
    let last = case.committed(q);
    assert_eq!(case.pending(&last), expected);
    let resumed = workspace::resume(&case.root).unwrap();
    assert_eq!(resumed["status"], "ready"); // Restored records permit investigation; they do not close pending acceptance.
    assert_eq!(resumed["pending_decisions"], json!(["cost", "select"]));
    assert_eq!(resumed["automatic_dispatch"], false);
    case.change_current_commit(|c| c["pending_decisions"] = json!([]));
    assert!(
        workspace::resume(&case.root).is_err(),
        "saved pending fields cannot self-certify a fresh review"
    );
}

#[test]
fn deleting_a_cancelled_stage_cannot_reset_the_attempt_history() {
    let mut case = Case::simple();
    let mut first = case.proposal("first", None);
    first["stage"] = json!({"id":"S1","acceptance_ids":["A1"],"intent":"Read requested source","execution":"planned","receipt":null});
    let parent = case.committed(first);
    let mut cancelled = case.proposal("cancelled", Some(&parent));
    cancelled["stage"] = json!({"id":"S1","acceptance_ids":["A1"],"intent":"Read requested source","execution":"cancelled","receipt":null});
    let parent = case.committed(cancelled);
    case.anchor["stages"] = json!([]);
    case.anchor["reviews"] = json!([]);
    let proposal = case.proposal("erased", Some(&parent));
    assert!(case.commit(proposal).is_err());
    assert_eq!(
        read(&case.root.data_path().join("HEAD.json"))["commit"],
        parent
    );
}
