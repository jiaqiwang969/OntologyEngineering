//! Native owner readback under an explicit separate project store. Synthetic
//! tool records test integrity and scope, not physical measurement authenticity.

use oe_contracts::{ArtifactReference, ObjectIdentity, WorkspaceStage, WorkspaceStageReceipt};
use oe_core::{SkillRoot, sha256};
use oe_engineering::{harness, project_workspace::receipts};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{fs, path::Path};

struct Fixture {
    _software: tempfile::TempDir,
    _data: tempfile::TempDir,
    root: SkillRoot,
    project: Project,
    project_ref: ArtifactReference,
    stage: WorkspaceStage,
    tool: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let software = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        fs::write(
            software.path().join("SKILL.md"),
            "Synthetic workspace owner test",
        )
        .unwrap();
        let root = SkillRoot::new(fs::canonicalize(software.path()).unwrap())
            .unwrap()
            .with_project_data_root(fs::canonicalize(data.path()).unwrap())
            .unwrap();
        let tool = fs::canonicalize(software.path())
            .unwrap()
            .join("synthetic-tool");
        fs::write(
            &tool,
            b"synthetic executable identity, not run by receipt verification",
        )
        .unwrap();
        let project:Project=serde_json::from_value(json!({
            "schema":"oev2.project/1","project_id":"receipt-project","revision":"r1","purpose":"Preserve the original purpose",
            "objects":{"object":"r1"},"conditions":{"service":"load=10 N"},"evidence":{"source":sha256(b"source")},
            "decisions":[{"id":"d1","object_id":"object","object_revision":"r1","condition_scope":"service",
                "source_sha256":sha256(b"source"),"conclusion":"unknown","adopted":false,"basis":[]}],"dependencies":{}
        })).unwrap();
        let project_ref = write(
            &root,
            "project-before.json",
            &serde_json::to_vec(&project).unwrap(),
        );
        Self {
            _software: software,
            _data: data,
            root,
            project,
            project_ref,
            stage: WorkspaceStage {
                id: "stage-1".into(),
                acceptance_ids: vec!["a1".into()],
                intent: "Inspect actual records".into(),
                execution: "results_recorded".into(),
                receipt: None,
            },
            tool,
        }
    }

    fn context(&self) -> Value {
        json!(
            project::context(
                &self.project,
                &ObjectIdentity {
                    object_id: "object".into(),
                    revision: "r1".into(),
                    condition_scope: "service".into(),
                    source_sha256: sha256(b"source")
                }
            )
            .unwrap()
        )
    }

    fn tool_result(&self) -> (Value, ArtifactReference) {
        let executable = sha256(fs::read(&self.tool).unwrap());
        let registry = json!({"schema":"oev2.capability-registry/1","capabilities":[{
            "name":"synthetic","display":"Synthetic test tool","skill":"synthetic","role":"fixture","outputs":["report"],
            "version":"1.0","executable":self.tool,"executable_sha256":executable,"invocation":["cli"],
            "acceptance":"Source identity only","acceptance_sha256":sha256(b"acceptance"),"status":"qualified","limits":[]
        }]});
        let registry_ref = write(
            &self.root,
            "registry.json",
            &serde_json::to_vec(&registry).unwrap(),
        );
        let raw = write(
            &self.root,
            "runs/raw.txt",
            b"synthetic observation; unmeasured conditions remain unknown",
        );
        let result = json!({"schema":"oev2.tool-result/1","result_id":"result-1","route_stage":"stage-1","project":self.context(),
            "purpose":"Inspect source identity","decisions":["d1"],"acceptance":["a1"],
            "tool":{"name":"synthetic","version":"1.0","executable_sha256":executable,"skill":"synthetic","invocation":"cli"},
            "subject":{"kind":"none","identity":"no code subject"},
            "inputs":{"arguments":[],"conditions":["read only"],"environment":["synthetic fixture"],"files":[]},
            "execution":{"status":"completed","started_at":"2026-10-22T01:00:00Z","finished_at":"2026-10-22T01:00:01Z","exit_code":0,"cleanup":"no process started"},
            "records":[{"record_id":"raw","kind":"raw_output","artifact":raw}],
            "findings":[{"finding_id":"f1","statement":"Source was retained","kind":"observed","basis":["raw"]}],
            "measurements":[],"behavior":"not_applicable","coverage":{"covered":["source bytes"],"not_covered":["engineering suitability"]},"unknowns":["Physical truth is not tested"]
        });
        (result, registry_ref)
    }

    fn wrap(
        &mut self,
        owner: &str,
        artifact: ArtifactReference,
        registry: Option<ArtifactReference>,
    ) {
        let wrapper = WorkspaceStageReceipt {
            schema: "oev2.workspace-stage-receipt/1".into(),
            stage_id: self.stage.id.clone(),
            owner: owner.into(),
            artifact,
            registry,
            project: self.project_ref.clone(),
        };
        self.stage.receipt = Some(write(
            &self.root,
            "stage-receipt.json",
            &serde_json::to_vec(&wrapper).unwrap(),
        ));
    }

    fn wrap_tool(&mut self, result: &Value, registry: ArtifactReference) {
        let artifact = write(
            &self.root,
            "runs/result.json",
            &serde_json::to_vec(result).unwrap(),
        );
        self.wrap("tool_result", artifact, Some(registry));
    }

    fn checkpoint(&mut self) -> ArtifactReference {
        let input = json!({"schema":"oev2.harness-checkpoint-input/1","task_id":self.stage.id,
            "context":"Current conditions remain explicit", "request":"Retain complete source records",
            "harness":{"project_id":self.project.project_id,"decision_id":"d1","object_id":"object","revision":"r1",
                "problem_class":"source-continuity","goal":self.project.purpose,
                "conditions":["load=10 N"],"constraints":["No physical conclusion from byte identity"],"unknowns":["Physical acceptance"],
                "semantic_modules":[],"review_requests":[],"observations":[],"current_project":self.project_ref,
                "project_context":self.context(),
                "checkpoint_policy":{"schema":"oev2.harness-checkpoint-policy/1","mode":"verified_no_model","byte_budget":16777216,"reason":"Preserve complete original state"}}});
        harness::step(&self.root, &input, Path::new("runs/checkpoint")).unwrap();
        reference(&self.root, "runs/checkpoint/iteration.json")
    }
}

fn write(root: &SkillRoot, path: &str, bytes: &[u8]) -> ArtifactReference {
    let destination = root.data_path().join(path);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, bytes).unwrap();
    ArtifactReference {
        path: path.into(),
        sha256: sha256(bytes),
    }
}

fn reference(root: &SkillRoot, path: &str) -> ArtifactReference {
    ArtifactReference {
        path: path.into(),
        sha256: sha256(fs::read(root.data_path().join(path)).unwrap()),
    }
}

fn rejects(result: anyhow::Result<Value>, reason: &str) {
    let error = result.unwrap_err();
    assert!(
        format!("{error:#}").contains(reason),
        "expected {reason}, got {error:#}"
    );
}

#[test]
fn tool_owner_rechecks_original_records_and_registered_followup_project() {
    let mut f = Fixture::new();
    let (result, registry) = f.tool_result();
    f.wrap_tool(&result, registry);
    let report = receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref).unwrap();
    assert_eq!(report["status"], "records_verified");
    assert_eq!(report["engineering_adoption"], "not_granted");
    assert!(
        report["references"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == "runs/raw.txt")
    );
    let mut current = f.project.clone();
    current.revision = "r2".into();
    current
        .evidence
        .insert("new-report".into(), sha256(b"new report"));
    let current_ref = write(
        &f.root,
        "project-after.json",
        &serde_json::to_vec(&current).unwrap(),
    );
    let after = receipts::verify(&f.root, &f.stage, &current, &current_ref).unwrap();
    assert_eq!(
        after["native"]["current_project_check"]["project_revised_since_run"],
        true
    );
    assert!(!current.decisions[0].adopted);
}

#[test]
fn completed_flag_or_rehashed_wrapper_cannot_hide_a_changed_original_record() {
    let mut f = Fixture::new();
    let bare = write(&f.root, "bare.json", br#"{"completed":true}"#);
    f.stage.receipt = Some(bare);
    assert!(receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref).is_err());
    let (result, registry) = f.tool_result();
    f.wrap_tool(&result, registry);
    fs::write(
        f.root.data_path().join("runs/raw.txt"),
        b"changed after execution",
    )
    .unwrap();
    rejects(
        receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref),
        "workspace_stage_execution_result_rejected",
    );
}

#[test]
fn wrong_stage_or_acceptance_cannot_reuse_an_unrelated_real_result() {
    let mut f = Fixture::new();
    let (mut result, registry) = f.tool_result();
    result["route_stage"] = json!("other-stage");
    f.wrap_tool(&result, registry.clone());
    rejects(
        receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref),
        "workspace_stage_tool_route_mismatch",
    );
    result["route_stage"] = json!("stage-1");
    result["acceptance"] = json!(["other-acceptance"]);
    f.wrap_tool(&result, registry);
    rejects(
        receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref),
        "workspace_stage_tool_acceptance_mismatch",
    );
}

#[test]
fn same_revision_condition_change_and_tool_binary_change_reject_current_use() {
    let mut f = Fixture::new();
    let (result, registry) = f.tool_result();
    f.wrap_tool(&result, registry);
    let mut changed = f.project.clone();
    changed
        .conditions
        .insert("service".into(), "load=20 N".into());
    let changed_ref = write(
        &f.root,
        "changed-project.json",
        &serde_json::to_vec(&changed).unwrap(),
    );
    rejects(
        receipts::verify(&f.root, &f.stage, &changed, &changed_ref),
        "workspace_stage_current_result_rejected",
    );
    fs::write(&f.tool, b"a different executable").unwrap();
    rejects(
        receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref),
        "workspace_stage_execution_result_rejected",
    );
}

#[test]
fn failed_tool_execution_is_preserved_as_a_verified_record_not_success() {
    let mut f = Fixture::new();
    let (mut result, registry) = f.tool_result();
    result["execution"]["status"] = json!("failed");
    result["execution"]["failure"] = json!("Explicit synthetic failure");
    result["execution"]["exit_code"] = json!(1);
    f.wrap_tool(&result, registry);
    let report = receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref).unwrap();
    assert_eq!(report["native"]["execution"]["status"], "failed");
    assert_eq!(
        report["native"]["current_project_check"]["decision_support"],
        "none"
    );
    assert_eq!(report["engineering_adoption"], "not_granted");
}

#[test]
fn harness_reads_its_full_original_archive_and_rejects_rewritten_summary() {
    let mut f = Fixture::new();
    let checkpoint = f.checkpoint();
    f.wrap("harness", checkpoint, None);
    let report = receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref).unwrap();
    assert_eq!(report["native"]["status"], "recorded");
    assert_eq!(report["native"]["engineering_acceptance"], "not_determined");
    let mut altered: Value = serde_json::from_slice(
        &fs::read(f.root.data_path().join("runs/checkpoint/iteration.json")).unwrap(),
    )
    .unwrap();
    altered["state"]["goal"] = json!("A substituted goal");
    let changed = write(
        &f.root,
        "runs/checkpoint/iteration.json",
        &serde_json::to_vec(&altered).unwrap(),
    );
    f.wrap("harness", changed, None);
    rejects(
        receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref),
        "harness_summary_mismatch",
    );
}

#[test]
fn harness_snapshot_is_not_rebound_to_an_advanced_project() {
    let mut f = Fixture::new();
    let checkpoint = f.checkpoint();
    f.wrap("harness", checkpoint, None);
    let mut after = f.project.clone();
    after.revision = "r2".into();
    let after_ref = write(
        &f.root,
        "project-after.json",
        &serde_json::to_vec(&after).unwrap(),
    );
    rejects(
        receipts::verify(&f.root, &f.stage, &after, &after_ref),
        "workspace_stage_harness_project_changed",
    );
}

#[test]
fn unsupported_exploration_never_reexecutes_or_dispatches_a_stage() {
    let mut f = Fixture::new();
    let artifact = write(
        &f.root,
        "exploration.json",
        br#"{"status":"requires_host_agent"}"#,
    );
    f.wrap("exploration", artifact, None);
    let before = fs::read(f.root.data_path().join("exploration.json")).unwrap();
    rejects(
        receipts::verify(&f.root, &f.stage, &f.project, &f.project_ref),
        "workspace_stage_exploration_readback_unsupported",
    );
    assert_eq!(
        fs::read(f.root.data_path().join("exploration.json")).unwrap(),
        before
    );
    assert!(!f.root.data_path().join("completed.json").exists());
}
