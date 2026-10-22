//! Independent expectations frozen before the no-model checkpoint implementation.
//! Synthetic software contracts, not engineering adoption or an unseen task.
use oe_core::{SkillRoot, sha256};
use oe_engineering::harness::{consolidate, load_step, step};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        let installed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let root = installed.join(format!(
            "var/tests/unified-system/checkpoint-independent-tests/{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("references")).unwrap();
        fs::write(root.join("SKILL.md"), "Synthetic independent software test").unwrap();
        Self {
            root: SkillRoot::new(root).unwrap(),
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join("var").join(name)
    }
    fn write(&self, name: &str, value: &Value) -> Value {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
        reference(&path)
    }
}
fn reference(path: &Path) -> Value {
    json!({"path":path, "sha256":sha256(fs::read(path).unwrap())})
}
fn input() -> Value {
    json!({"schema":"oev2.harness-checkpoint-input/1", "task_id":"independent-checkpoint",
      "context":"Record evidence without model inference.", "request":"Retain unknown engineering conditions.",
      "harness":{"project_id":"independent-project", "decision_id":"decision-A", "object_id":"support",
        "revision":"r1", "problem_class":"load-support", "goal":"Determine whether the support meets its load requirement",
        "conditions":[], "constraints":["Unmeasured load is not zero"], "unknowns":["support stiffness"],
        "semantic_modules":[], "review_requests":[], "observations":[],
        "checkpoint_policy":{"schema":"oev2.harness-checkpoint-policy/1", "mode":"verified_no_model",
          "byte_budget":16777216, "reason":"Independent source archive; no model inference claimed"}}})
}
fn archive(iteration: Value) -> Value {
    json!({"schema":"oev2.consolidation-input/3", "checkpoint_id":"independent-archive",
      "problem_class":"load-support", "reason":"Existing concepts suffice; preserve the original process",
      "iterations":[iteration], "candidates":[], "archive_policy":{
        "schema":"oev2.consolidation-archive-policy/1", "mode":"verified_empty_checkpoint", "byte_budget":16777216}})
}

#[test]
fn complete_large_record_is_not_a_model_answer_or_engineering_acceptance() {
    let f = Fixture::new();
    let mut doc = input();
    let evidence = (0..9000)
        .map(|i| format!("condition-{i}: result unknown; load and units retained;\n"))
        .collect::<String>();
    assert!(evidence.len() > 256000);
    doc["harness"]["conditions"] = json!([evidence]);
    let result = step(&f.root, &doc, &f.path("large")).unwrap();
    assert_eq!(result["schema"], "oev2.harness-checkpoint/1");
    assert_eq!(result["status"], "recorded");
    assert_eq!(result["model_execution"], "not_run");
    assert_eq!(result["model_calls"], 0);
    assert_eq!(result["candidate_routes"], json!([]));
    assert_eq!(result["uncertain_routes"], json!([]));
    assert_eq!(result["engineering_acceptance"], "not_determined");
    assert_eq!(result["ontology_promotion"], "not_performed");
    assert_eq!(result["current_use"], "blocked_or_historical_only");
    assert_eq!(result["state"]["conditions"], doc["harness"]["conditions"]);
    let mut legacy = doc.clone();
    legacy["schema"] = json!("ontology-engineering.context-input/v1");
    legacy["harness"]
        .as_object_mut()
        .unwrap()
        .shift_remove("checkpoint_policy");
    let refused = step(&f.root, &legacy, &f.path("legacy")).unwrap_err();
    assert_eq!(refused.to_string(), "checkpoint_input_required");
    assert!(!f.path("legacy/iteration.json").exists());
    let original = fs::read(f.path("large/iteration.json")).unwrap();
    let source = reference(&f.path("large/iteration.json"));
    assert_eq!(load_step(&f.root, &source).unwrap(), result);
    let archived = consolidate(&f.root, &archive(source.clone()), &f.path("archive")).unwrap();
    assert_eq!(archived["status"], "no_delta");
    assert_eq!(archived["authority"], "author_declaration_only");
    assert_eq!(archived["execution_kind"], "not_run");
    assert_eq!(
        fs::read(f.path("archive/trajectories/00/iteration.json")).unwrap(),
        original
    );
    let mut nonempty = archive(source);
    nonempty["candidates"] = json!([{"id":"cannot-skip-evaluation", "proposal":"new concept"}]);
    assert!(consolidate(&f.root, &nonempty, &f.path("nonempty")).is_err());
    assert!(!f.path("nonempty/consolidation.json").exists());
}

#[test]
fn no_model_does_not_relax_current_project_identity() {
    let f = Fixture::new();
    let project: oe_kernel::project::Project = serde_json::from_value(json!({
        "schema":"oev2.project/1", "project_id":"independent-project", "revision":"p1", "purpose":"support load",
        "objects":{"support":"r1"}, "conditions":{"same-name":"load=10 N; support=cantilever"},
        "evidence":{"source":"a".repeat(64)}, "decisions":[], "dependencies":{}
    })).unwrap();
    let object = oe_contracts::ObjectIdentity {
        object_id: "support".into(),
        revision: "r1".into(),
        condition_scope: "same-name".into(),
        source_sha256: "a".repeat(64),
    };
    let old_context = oe_kernel::project::context(&project, &object).unwrap();
    let mut changed = project.clone();
    changed
        .conditions
        .insert("same-name".into(), "load=20 N; support=cantilever".into());
    let mut doc = input();
    doc["harness"]["conditions"] = json!([changed.conditions["same-name"]]);
    doc["harness"]["current_project"] = f.write("changed.json", &json!(changed));
    doc["harness"]["project_context"] = json!(old_context);
    let result = step(&f.root, &doc, &f.path("changed")).unwrap();
    assert_eq!(result["current_project_check"]["conclusion"], "violated");
    assert!(
        result["current_project_check"]["failures"]
            .as_array()
            .unwrap()
            .contains(&json!("current_project_context_changed"))
    );
    assert_eq!(result["current_use"], "blocked_or_historical_only");
    assert_eq!(result["feedback"]["dependent_adoption"], "blocked");
}

#[test]
fn explicit_protocol_rejects_questions_policy_mixing_and_terminal_forgery() {
    let f = Fixture::new();
    fs::remove_dir_all(f.root.path().join("references")).unwrap();
    for (i, mut bad) in [input(), input(), input(), input()].into_iter().enumerate() {
        match i {
            0 => bad["questions"] = json!({"q":"May this be adopted?"}),
            1 => {
                bad["harness"]["context_policy"] = json!({"schema":"oev2.harness-context-policy/3", "mode":"lossless_indexed_text", "byte_budget":256000})
            }
            2 => bad["harness"]["checkpoint_policy"]["byte_budget"] = json!(16777217),
            _ => bad["harness"]["checkpoint_policy"]["reason"] = json!(""),
        }
        let out = f.path(&format!("invalid-{i}"));
        assert!(step(&f.root, &bad, &out).is_err());
        assert!(!out.join("iteration.json").exists());
    }
    let mut result = step(&f.root, &input(), &f.path("valid")).unwrap();
    result["model_execution"] = json!("completed");
    fs::write(
        f.path("valid/iteration.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    assert!(load_step(&f.root, &reference(&f.path("valid/iteration.json"))).is_err());
}

#[test]
fn json_evidence_cannot_escape_the_declared_depth_budget_by_failing_to_parse() {
    let f = Fixture::new();
    let path = f.path("too-deep.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, format!("{}0{}", "[".repeat(140), "]".repeat(140))).unwrap();
    let mut doc = input();
    doc["harness"]["observations"] = json!([{
        "kind":"observed", "statement":"Deep JSON must not be reclassified as opaque evidence to escape its resource contract",
        "evidence":[reference(&path)]
    }]);
    assert!(step(&f.root, &doc, &f.path("deep")).is_err());
    assert!(!f.path("deep/iteration.json").exists());
}
