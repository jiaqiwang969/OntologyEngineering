//! Pure semantic controls. Native reconciliation replay is exercised by the
//! workspace owner tests; these fixtures stand in for its already computed
//! values and do not claim actual engineering review or causal validation.

use oe_contracts::{
    ArtifactReference, WorkspaceCausalClaim, WorkspaceCorrectionObligation, WorkspaceGoal,
    WorkspaceLearningBinding, WorkspaceRequestSource, WorkspaceResolution,
};
use oe_engineering::project_workspace::semantics::{
    validate_learning_binding, validate_learning_goal, validate_semantics,
};
use oe_kernel::project::Project;
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct Case {
    project: Project,
    project_ref: ArtifactReference,
    anchor: Value,
    sources: BTreeMap<String, Vec<u8>>,
    requests: Vec<WorkspaceRequestSource>,
    obligations: Vec<WorkspaceCorrectionObligation>,
    claims: Vec<WorkspaceCausalClaim>,
    replayed: BTreeMap<String, Value>,
}

fn put(sources: &mut BTreeMap<String, Vec<u8>>, path: &str, bytes: Vec<u8>) -> ArtifactReference {
    let reference = ArtifactReference {
        path: path.into(),
        sha256: oe_core::sha256(&bytes),
    };
    sources.insert(path.into(), bytes);
    reference
}

impl Case {
    fn new() -> Self {
        let project: Project = serde_json::from_value(json!({
            "schema":"oev2.project/1", "project_id":"workspace-semantic-controls", "revision":"r1",
            "purpose":"Recover the original engineering acceptance after a context change",
            "objects":{"root":"r1"}, "conditions":{"service":"load=10 N"},
            "evidence":{"original-source":"a".repeat(64)},
            "decisions":[
                {"id":"d1","object_id":"root","object_revision":"r1","condition_scope":"service",
                 "source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["original-source"]},
                {"id":"d2","object_id":"root","object_revision":"r1","condition_scope":"service",
                 "source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["d1"]}
            ], "dependencies":{"d2":["d1"]}
        })).unwrap();
        let anchor = json!({
            "schema":"oev2.route-anchor/2", "project_id":project.project_id,
            "root_object":"root", "deliverable":"The original complete deliverable",
            "requests":[
                {"id":"u1","kind":"root","text":"Keep both original acceptance items.","affects":[]},
                {"id":"u2","kind":"correction","text":"Recheck the load before continuing.","affects":["a1"]}
            ],
            "acceptance":[{"id":"a1","decisions":["d1","d2"]},{"id":"a2","decisions":[]}],
            "stages":[{"id":"stage-after-correction","advances":["a1"]}]
        });
        let mut sources = BTreeMap::new();
        let project_ref = put(
            &mut sources,
            "project.json",
            serde_json::to_vec(&project).unwrap(),
        );
        let requests = anchor["requests"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, u)| WorkspaceRequestSource {
                request_id: u["id"].as_str().unwrap().into(),
                source: put(
                    &mut sources,
                    &format!("requests/{i}.txt"),
                    u["text"].as_str().unwrap().as_bytes().to_vec(),
                ),
            })
            .collect();
        Self {
            project,
            project_ref,
            anchor,
            sources,
            requests,
            obligations: vec![WorkspaceCorrectionObligation {
                id: "obligation-u2".into(),
                request_id: "u2".into(),
                status: "pending_review".into(),
                affected_decisions: vec!["d1".into(), "d2".into()],
                resolutions: vec![],
                reason: "Review remains open".into(),
            }],
            claims: vec![],
            replayed: BTreeMap::new(),
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        validate_semantics(
            &self.project,
            &self.project_ref,
            &self.anchor,
            &self.requests,
            &self.obligations,
            &self.claims,
            &self.sources,
            &self.replayed,
        )
    }

    fn resolved(&mut self) {
        let request =
            json!({"schema":"oev2.reviewed-reconciliation-request/1","current":self.project_ref});
        let request_ref = put(
            &mut self.sources,
            "reviews/request.json",
            serde_json::to_vec(&request).unwrap(),
        );
        let result = json!({
            "schema":"oev2.reviewed-reconciliation-result/1",
            "request_sha256":oe_core::sha256(oe_core::json::canonical_json(&request).unwrap()),
            "current":self.project_ref,"legacy_impact":{"project_id":self.project.project_id},
            "status":"review_complete","engineering_adoption":"not_granted",
            "review_satisfied":["d1","d2"],"still_requires_review":[],
            "reviews":[
                {"decision_id":"d1","status":"replayed_current","receipt":{"execution":"completed","conclusion":"pass"}},
                {"decision_id":"d2","status":"replayed_current","receipt":{"execution":"completed","conclusion":"pass"}}
            ]
        });
        let result_ref = put(
            &mut self.sources,
            "reviews/result.json",
            serde_json::to_vec(&result).unwrap(),
        );
        self.replayed.insert(result_ref.path.clone(), result);
        self.obligations[0].status = "resolved".into();
        self.obligations[0].reason = "The existing owner replayed every affected decision".into();
        self.obligations[0].resolutions = vec![WorkspaceResolution {
            request: request_ref,
            result: result_ref,
        }];
    }

    fn replace_result(&mut self, change: impl FnOnce(&mut Value)) {
        let path = self.obligations[0].resolutions[0].result.path.clone();
        let mut value: Value = serde_json::from_slice(&self.sources[&path]).unwrap();
        change(&mut value);
        self.obligations[0].resolutions[0].result = put(
            &mut self.sources,
            &path,
            serde_json::to_vec(&value).unwrap(),
        );
        self.replayed.insert(path, value);
    }

    fn learning(&mut self) -> (ArtifactReference, WorkspaceGoal) {
        let anchor_ref = put(
            &mut self.sources,
            "route-anchor.json",
            serde_json::to_vec(&self.anchor).unwrap(),
        );
        let goal = WorkspaceGoal {
            schema: "oev2.workspace-goal/1".into(),
            project_id: self.project.project_id.clone(),
            project: self.project_ref.clone(),
            root_anchor: anchor_ref.clone(),
            acceptance_ids: vec!["a1".into(), "a2".into()],
        };
        (anchor_ref, goal)
    }
}

fn rejects(result: anyhow::Result<()>, reason: &str) {
    let error = result.unwrap_err();
    assert!(
        format!("{error:#}").contains(reason),
        "expected {reason}, got {error:#}"
    );
}

#[test]
fn complete_sources_preserve_open_obligations_without_mutating_project() {
    let mut case = Case::new();
    let before = serde_json::to_vec(&case.project).unwrap();
    case.check().unwrap();
    case.obligations[0].status = "acknowledged".into();
    case.check().unwrap();
    case.obligations[0].status = "deferred".into();
    case.check().unwrap();
    assert_eq!(serde_json::to_vec(&case.project).unwrap(), before);
    assert!(!case.project.decisions.iter().any(|d| d.adopted));
}

#[test]
fn missing_request_or_correction_is_not_hidden_by_a_later_stage() {
    let mut case = Case::new();
    case.requests.pop();
    rejects(case.check(), "workspace_request_sources_incomplete");
    let mut case = Case::new();
    case.obligations.clear();
    rejects(case.check(), "workspace_correction_obligations_incomplete");
}

#[test]
fn source_must_equal_the_complete_utterance_not_merely_contain_it() {
    let mut case = Case::new();
    let quote = case.anchor["requests"][0]["text"].as_str().unwrap();
    case.requests[0].source = put(
        &mut case.sources,
        "requests/0.txt",
        format!("Assistant said: {quote}").into_bytes(),
    );
    rejects(case.check(), "workspace_request_text_mismatch");
}

#[test]
fn source_drift_rejects_even_when_the_file_name_and_project_revision_stay_the_same() {
    let mut case = Case::new();
    case.sources
        .get_mut(&case.requests[0].source.path)
        .unwrap()
        .push(b'!');
    rejects(case.check(), "workspace_source_material_changed");
    let mut case = Case::new();
    case.project
        .conditions
        .insert("service".into(), "load=20 N".into());
    rejects(case.check(), "workspace_project_material_mismatch");
}

#[test]
fn declaring_stage_progress_cannot_resolve_a_correction() {
    let mut case = Case::new();
    case.obligations[0].status = "resolved".into();
    rejects(case.check(), "workspace_correction_resolution_missing");
}

#[test]
fn matched_replayed_review_closes_only_the_recorded_obligation() {
    let mut case = Case::new();
    case.resolved();
    let before = serde_json::to_vec(&case.project).unwrap();
    case.check().unwrap();
    assert_eq!(serde_json::to_vec(&case.project).unwrap(), before);
    assert!(
        case.project
            .decisions
            .iter()
            .all(|d| d.conclusion == "unknown" && !d.adopted)
    );
}

#[test]
fn saved_pass_without_actual_owner_replay_is_rejected() {
    let mut case = Case::new();
    case.resolved();
    case.replayed.clear();
    rejects(case.check(), "workspace_resolution_native_replay_missing");
    case.resolved();
    case.replayed.get_mut("reviews/result.json").unwrap()["review_satisfied"] = json!(["d1"]);
    rejects(case.check(), "workspace_resolution_replay_mismatch");
}

#[test]
fn resolution_requires_all_linked_decisions_and_concrete_review_rows() {
    let mut case = Case::new();
    case.resolved();
    case.obligations[0].affected_decisions = vec!["d1".into()];
    rejects(
        case.check(),
        "workspace_correction_affected_decisions_incomplete",
    );
    case.obligations[0].affected_decisions.push("d2".into());
    case.replace_result(|r| {
        r["review_satisfied"] = json!(["d1"]);
        r["still_requires_review"] = json!(["d2"]);
        r["status"] = json!("review_partial");
    });
    rejects(case.check(), "workspace_correction_review_incomplete");
    case.resolved();
    case.replace_result(|r| {
        r["reviews"] = json!([]);
    });
    rejects(case.check(), "workspace_resolution_actual_review_missing");
}

#[test]
fn stale_current_binding_and_stage_shaped_resolution_do_not_close_a_correction() {
    let mut case = Case::new();
    case.resolved();
    case.replace_result(|r| {
        r["current"]["sha256"] = json!("b".repeat(64));
    });
    rejects(case.check(), "workspace_resolution_project_changed");
    case.resolved();
    let resolution = &mut case.obligations[0].resolutions[0];
    resolution.request = put(
        &mut case.sources,
        "reviews/request.json",
        serde_json::to_vec(&json!({"schema":"oev2.route-stage/1","advances":["a1"]})).unwrap(),
    );
    rejects(case.check(), "workspace_resolution_owner_unsupported");
}

#[test]
fn causal_levels_retain_original_sources_and_never_grant_project_support() {
    let mut case = Case::new();
    let reference = put(
        &mut case.sources,
        "observations/tool-output.txt",
        b"The output changed; temperature also changed.".to_vec(),
    );
    case.claims.push(WorkspaceCausalClaim {
        id: "cause1".into(),
        statement: "Method replacement preceded the changed output".into(),
        level: "sequence".into(),
        source_refs: vec![reference],
        confounders: vec!["Temperature changed".into()],
        unknowns: vec!["No controlled comparison".into()],
    });
    let before = serde_json::to_vec(&case.project).unwrap();
    case.check().unwrap();
    assert_eq!(serde_json::to_vec(&case.project).unwrap(), before);
    case.claims[0].level = "proved".into();
    rejects(case.check(), "workspace_causal_claim_invalid");
    case.claims[0].level = "controlled_comparison".into();
    case.claims[0].source_refs.clear();
    rejects(case.check(), "workspace_causal_claim_invalid");
}

#[test]
fn learning_goal_preserves_every_acceptance_and_the_full_anchor_identity() {
    let mut case = Case::new();
    let (anchor_ref, mut goal) = case.learning();
    validate_learning_goal(&goal, &case.project_ref, &anchor_ref, &case.anchor).unwrap();
    goal.acceptance_ids.pop();
    rejects(
        validate_learning_goal(&goal, &case.project_ref, &anchor_ref, &case.anchor),
        "workspace_learning_acceptance_changed",
    );
    goal.acceptance_ids = vec!["a2".into(), "a1".into()];
    rejects(
        validate_learning_goal(&goal, &case.project_ref, &anchor_ref, &case.anchor),
        "workspace_learning_acceptance_changed",
    );
    goal.acceptance_ids = vec!["a1".into(), "a2".into()];
    let changed_anchor_ref = ArtifactReference {
        path: anchor_ref.path.clone(),
        sha256: "e".repeat(64),
    };
    rejects(
        validate_learning_goal(&goal, &case.project_ref, &changed_anchor_ref, &case.anchor),
        "workspace_learning_root_changed",
    );
}

#[test]
fn unchanged_goal_bytes_cannot_hide_a_changed_root_or_current_project() {
    let mut case = Case::new();
    let (anchor_ref, goal) = case.learning();
    let goal_ref = put(
        &mut case.sources,
        "learning/goal.json",
        serde_json::to_vec(&goal).unwrap(),
    );
    let binding = WorkspaceLearningBinding {
        schema: "oev2.workspace-learning-binding/1".into(),
        project_id: goal.project_id.clone(),
        project: case.project_ref.clone(),
        root_anchor: anchor_ref.clone(),
        goal: goal_ref.clone(),
        acceptance_ids: goal.acceptance_ids.clone(),
    };
    validate_learning_binding(
        &binding,
        &goal,
        &goal_ref,
        &case.project_ref,
        &anchor_ref,
        &case.anchor,
    )
    .unwrap();
    let changed_project = ArtifactReference {
        path: case.project_ref.path.clone(),
        sha256: "c".repeat(64),
    };
    rejects(
        validate_learning_binding(
            &binding,
            &goal,
            &goal_ref,
            &changed_project,
            &anchor_ref,
            &case.anchor,
        ),
        "workspace_learning_root_changed",
    );
    let changed_goal = ArtifactReference {
        path: goal_ref.path.clone(),
        sha256: "d".repeat(64),
    };
    rejects(
        validate_learning_binding(
            &binding,
            &goal,
            &changed_goal,
            &case.project_ref,
            &anchor_ref,
            &case.anchor,
        ),
        "workspace_learning_binding_changed",
    );
}
