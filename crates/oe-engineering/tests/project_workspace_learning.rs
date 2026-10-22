//! Actual workspace -> native RRSI freeze and evaluation. The source-backed
//! synthetic case measures a diagnostic procedure, not a physical improvement.
use oe_contracts::{ArtifactReference, WorkspaceCommitRequest};
use oe_core::{SkillRoot, sha256};
use oe_engineering::project_workspace as workspace;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn put(root: &SkillRoot, path: &str, bytes: &[u8]) -> ArtifactReference {
    let destination = root.data_path().join(path);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, bytes).unwrap();
    ArtifactReference {
        path: path.into(),
        sha256: sha256(bytes),
    }
}

fn json_put(root: &SkillRoot, path: &str, value: &Value) -> ArtifactReference {
    put(root, path, &serde_json::to_vec_pretty(value).unwrap())
}

#[test]
fn current_root_goal_freezes_with_native_evaluator_and_later_root_change_blocks_trial() {
    let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let store = tempfile::tempdir().unwrap();
    let home = fs::canonicalize(store.path()).unwrap();
    let root = SkillRoot::new(actual)
        .unwrap()
        .with_project_data_root(&home)
        .unwrap();
    let lock: Value =
        serde_json::from_slice(&fs::read(actual.join("runtime/semantic-bundles.json")).unwrap())
            .unwrap();
    let bundle = &lock["bundles"]["engineering-judgment-intake"];
    let source=b"@prefix ee: <urn:semantica:engineering-evidence:> .\n<urn:fixture:review> a ee:ReviewClaim ; ee:subjectRevision \"r1\" ; ee:scope \"declared_fixture\" ; ee:method \"pattern-scope\" ; ee:claim_scope \"declared_fixture\" ; ee:evidence_scope \"declared_fixture\" ; ee:required_conditions \"flow\", \"fluid\" ; ee:required_conditionsComplete true ; ee:established_conditions \"flow\" ; ee:established_conditionsComplete true ; ee:inventory_complete true ; ee:representation \"applicability_reviewed\" .\n";
    let evidence = put(&root, "inputs/evidence.ttl", source);
    let project = json!({"schema":"oev2.project/1","project_id":"workspace-learning","revision":"r1",
        "purpose":"Retain unknown physical conditions and the complete acceptance",
        "objects":{"urn:fixture:review":"r1"},"conditions":{"declared_fixture":"actual fluid unknown"},
        "evidence":{"fixture":evidence.sha256},"decisions":[],"dependencies":{}});
    let project_ref = json_put(&root, "inputs/project.json", &project);
    let request_text = b"Synthetic user request: preserve the whole diagnostic acceptance and every unknown physical condition.";
    let request_ref = put(&root, "inputs/request.txt", request_text);
    let anchor = json!({"schema":"oev2.route-anchor/2","project_id":"workspace-learning",
        "root_object":"urn:fixture:review","deliverable":"A complete source-bound condition diagnosis",
        "requests":[{"id":"U1","kind":"root","text":std::str::from_utf8(request_text).unwrap(),"source":"inputs/request.txt","affects":[]}],
        "functions":[{"id":"F1","statement":"Identify missing physical conditions","structures":[]}],
        "acceptance":[{"id":"A1","statement":"All required conditions are retained and missing ones remain unknown","functions":["F1"],"status":"open","decisions":[],"evidence":["fixture"]}],
        "stages":[{"id":"S1","object_id":"urn:fixture:review","relation":"root","relation_target":"urn:fixture:review","advances":["A1"],"outcome":"pending"}],
        "reviews":[],"policy":{"off_root_stage_limit":2,"no_progress_stage_limit":2,"review_interval":5}});
    let anchor_ref = json_put(&root, "inputs/anchor.json", &anchor);
    let mut proposal: WorkspaceCommitRequest = serde_json::from_value(json!({
        "schema":"oev2.workspace-commit-request/1","workspace_id":"workspace-learning","commit_id":"initial","expected_parent":null,
        "project":project_ref,"anchor":anchor_ref,"requests":[{"request_id":"U1","source":request_ref}],
        "corrections":[],"causal_claims":[],"events":[],"sources":[evidence],"work":null,"checkpoint":null,"reviews":[],
        "stage":{"id":"S1","acceptance_ids":["A1"],"intent":"Evaluate the existing condition diagnosis","execution":"planned","receipt":null}
    })).unwrap();
    workspace::initialize(&root, "workspace-learning", "workspace-learning").unwrap();
    let initial = workspace::commit(&root, &proposal).unwrap();
    assert_eq!(initial["status"], "committed");
    let goal_result = workspace::learning_goal(&root, Path::new("learning/goal.json")).unwrap();
    let goal = goal_result["goal"].clone();
    let binding = json_put(
        &root,
        "inputs/binding.json",
        &json!({
        "project_id":"workspace-learning","object_id":"urn:fixture:review","object_revision":"r1",
        "package_id":bundle["package_id"],"package_version":bundle["package_version"],"package_sha256":bundle["sha256"],
        "source_sha256":evidence.sha256,"condition_scope":"declared_fixture",
        "required_checks":["findings-pattern-scope","shape-pattern-scope","bound_focus_context"],"authority_id":"public-test-only"}),
    );
    let case = json!({"case_id":"case-a","source_family":"public-synthetic-family","exposure":"development_replay",
        "project":project_ref,"binding":binding,"evidence":evidence,"bundle":"engineering-judgment-intake",
        "query_ids":["findings-pattern-scope"],"shape_ids":["shape-pattern-scope"],"expected_conclusion":"unknown","required_reasons":["required_condition_not_established"]});
    let evaluation = json_put(
        &root,
        "inputs/evaluation.json",
        &json!({
        "schema":"oev2.experience-evaluation/1","goal_sha256":goal["sha256"],
        "model":format!("{}@{}",oe_kernel::ENGINE_ID,oe_kernel::ENGINE_VERSION),"repetitions":1,"budget_max_checks":16,
        "hard_conditions":["identity","units","conditions","source","unknown_state","necessary_acceptance","evaluator_integrity"],"cases":[case]}),
    );
    let baseline = json_put(
        &root,
        "inputs/baseline.json",
        &json!({"mode":"no_semantic_review"}),
    );
    let candidate = json_put(
        &root,
        "inputs/candidate.json",
        &json!({"mode":"required_semantic_review"}),
    );
    let input = json!({"schema":"oev2.experience-freeze/1","candidate":{
        "candidate_id":"workspace-native-freeze","kind":"operation_method","project_id":"workspace-learning",
        "object":{"object_id":"urn:fixture:review","revision":"r1","condition_scope":"declared_fixture","source_sha256":evidence.sha256},
        "purpose":"Diagnose conditions before any engineering adoption","function":"Execute existing condition checks",
        "mechanism":"Diagnostic procedure; physical mechanism unknown","applicability":["Source-bound condition inventory"],
        "sources":[{"source_id":"synthetic-evidence","source_sha256":evidence.sha256,"source_uri":evidence.path,"extraction_version":"public-test/1","physical_page":null}],
        "counterexamples":["Tool completion does not imply engineering acceptance"],"unknowns":["Actual fluid properties"],
        "verification_requirements":["Independent tasks and physical evidence"],"existing_expression":"pattern-scope","ontology_delta":"no_delta",
        "hypotheses":["The existing Rust review exposes missing declared conditions"]},
        "baseline":baseline,"candidate_plan":candidate,"goal":goal,"evaluation":evaluation,"prior_rejection":null,"new_basis":[]});
    let candidate_path = Path::new("learning/candidate");
    let frozen = workspace::learning_freeze(&root, &input, candidate_path).unwrap();
    assert_eq!(frozen["schema"], "oev2.workspace-learning-freeze/1");
    let workspace_binding: ArtifactReference =
        serde_json::from_value(frozen["binding"].clone()).unwrap();
    assert_eq!(
        workspace::learning_check(&root, &workspace_binding).unwrap()["status"],
        "root_consistent"
    );
    assert!(!home.join("SKILL.md").exists());
    let manifest: Value =
        serde_json::from_slice(&fs::read(home.join(candidate_path).join("freeze.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["goal_sha256"], goal["sha256"]);
    assert_eq!(
        manifest["evaluator_sha256"],
        sha256(fs::read(actual.join("crates/oe-kernel/src/learning.rs")).unwrap())
    );
    assert_eq!(
        manifest["files"]["cases/0/rules.zip"]["sha256"],
        bundle["sha256"]
    );
    let assessed =
        oe_kernel::learning::assess(&root, candidate_path, Path::new("learning/assessment.json"))
            .unwrap();
    assert!(assessed["assessment"]["verdict"].is_string());
    let assessment: Value =
        serde_json::from_slice(&fs::read(home.join("learning/assessment.json")).unwrap()).unwrap();
    assert_eq!(assessment["engineering_acceptance"], "not_established");

    // The frozen evidence remains readable, but a new source-bound user
    // correction changes the root identity and blocks continuation of the trial.
    let original_manifest = fs::read(home.join(candidate_path).join("freeze.json")).unwrap();
    let correction =
        "Synthetic user correction: include actual fluid properties in the deliverable.";
    let correction_ref = put(&root, "inputs/correction.txt", correction.as_bytes());
    let mut revised = anchor;
    revised["deliverable"] =
        json!("A complete condition diagnosis including actual fluid properties");
    revised["requests"].as_array_mut().unwrap().push(json!({"id":"U2","kind":"correction","text":correction,"source":"inputs/correction.txt","affects":["A1"]}));
    proposal.commit_id = "corrected-goal".into();
    proposal.expected_parent =
        Some(serde_json::from_value(initial["head"]["commit"].clone()).unwrap());
    proposal.anchor = json_put(&root, "inputs/corrected-anchor.json", &revised);
    proposal
        .requests
        .push(serde_json::from_value(json!({"request_id":"U2","source":correction_ref})).unwrap());
    proposal.corrections.push(serde_json::from_value(json!({"id":"C2","request_id":"U2","status":"acknowledged","affected_decisions":[],"resolutions":[],"reason":"Actual fluid evidence remains unavailable"})).unwrap());
    assert_eq!(
        workspace::commit(&root, &proposal).unwrap()["status"],
        "committed"
    );
    let failure = workspace::learning_check(&root, &workspace_binding).unwrap_err();
    assert!(format!("{failure:#}").contains("workspace_learning_root_changed"));
    assert_eq!(
        fs::read(home.join(candidate_path).join("freeze.json")).unwrap(),
        original_manifest
    );
}
