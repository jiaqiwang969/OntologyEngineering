use oe_contracts::ObjectIdentity;
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn baseline() -> Project {
    serde_json::from_value(json!({
        "schema":"oev2.project/1", "project_id":"invalidation-regression", "revision":"p1",
        "purpose":"bounded source and dependency change diagnosis",
        "objects":{"urn:audit:part":"r1"},
        "conditions":{"service":"force=10 N; support=cantilever"},
        "evidence":{
            "source-primary":"a".repeat(64), "source-secondary":"b".repeat(64),
            "source-independent":"c".repeat(64)
        },
        "decisions":[
            {"id":"select", "object_id":"urn:audit:part", "object_revision":"r1",
             "condition_scope":"service", "source_sha256":"a".repeat(64),
             "conclusion":"supported", "adopted":true, "basis":["support-premise"]},
            {"id":"cost", "object_id":"urn:audit:part", "object_revision":"r1",
             "condition_scope":"service", "source_sha256":"b".repeat(64),
             "conclusion":"supported", "adopted":true, "basis":["select"]},
            {"id":"independent", "object_id":"urn:audit:part", "object_revision":"r1",
             "condition_scope":"service", "source_sha256":"c".repeat(64),
             "conclusion":"supported", "adopted":true, "basis":["standalone-premise"]}
        ],
        "dependencies":{
            "support-premise":["source-primary"], "cost":["select"],
            "standalone-premise":["source-independent"]
        }
    }))
    .unwrap()
}

fn ids(values: &Value, field: Option<&str>) -> BTreeSet<String> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            field
                .map_or(value, |field| &value[field])
                .as_str()
                .unwrap()
                .into()
        })
        .collect()
}

fn expected_ids(expected: &[&str]) -> BTreeSet<String> {
    expected.iter().map(|id| (*id).into()).collect()
}

// Every counterexample first establishes a legal, unaffected project with fresh
// context. Historical snapshots and current input records must remain unchanged.
fn normal_control(project: &Project) {
    project::validate(project).unwrap();
    for decision in &project.decisions {
        let object = ObjectIdentity {
            object_id: decision.object_id.clone(),
            revision: decision.object_revision.clone(),
            condition_scope: decision.condition_scope.clone(),
            source_sha256: decision.source_sha256.clone(),
        };
        let context = project::context(project, &object).unwrap();
        assert_eq!(
            project::check_current_context(project, &object, Some(&context)).conclusion,
            "pass"
        );
    }
    let unchanged = project::reconcile(project, project).unwrap();
    assert_eq!(unchanged["affected"], json!([]));
    assert_eq!(unchanged["decisions"], json!([]));
    assert_eq!(unchanged["stale_adoptions"], json!([]));
}

fn assert_review(before: &Project, after: &Project, review: &[&str], stale: &[&str]) -> Value {
    project::validate(after).unwrap();
    let old_bytes = serde_json::to_vec(before).unwrap();
    let current_bytes = serde_json::to_vec(after).unwrap();
    let report = project::reconcile(before, after).unwrap();
    eprintln!("{}", serde_json::to_string_pretty(&report).unwrap());
    assert_eq!(serde_json::to_vec(before).unwrap(), old_bytes);
    assert_eq!(serde_json::to_vec(after).unwrap(), current_bytes);
    assert_eq!(
        ids(&report["decisions"], Some("decision_id")),
        expected_ids(review)
    );
    assert_eq!(ids(&report["stale_adoptions"], None), expected_ids(stale));
    for decision in report["decisions"].as_array().unwrap() {
        assert_eq!(decision["current_status"], "requires_review");
    }
    assert_eq!(
        report["unknown_dependencies"],
        "unlisted links remain unverified"
    );
    report
}

#[test]
fn audit_deleted_dependency_entry_reopens_its_dependents() {
    let before = baseline();
    normal_control(&before);
    let mut after = before.clone();
    after.dependencies.remove("support-premise");
    assert_review(&before, &after, &["select", "cost"], &["select", "cost"]);
}

#[test]
fn audit_source_digest_change_reopens_direct_and_downstream_decisions() {
    let mut before = baseline();
    // Isolate the binding: no declared dependency leads from source-primary to
    // select, so propagation must use its changed registered content digest.
    before.dependencies.remove("support-premise");
    before.decisions[0].basis = vec!["source-secondary".into()];
    normal_control(&before);
    let mut after = before.clone();
    after
        .evidence
        .insert("source-primary".into(), "d".repeat(64));
    assert_review(&before, &after, &["select", "cost"], &["select", "cost"]);
}

#[test]
fn audit_refuted_predecessor_reopens_adopted_downstream_decision() {
    let before = baseline();
    normal_control(&before);
    let mut after = before.clone();
    after.decisions[0].conclusion = "refuted".into();
    after.decisions[0].adopted = false;
    let report = assert_review(&before, &after, &["select", "cost"], &["cost"]);
    let select = report["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|decision| decision["decision_id"] == "select")
        .unwrap();
    assert_eq!(select["prior_conclusion"], "supported");
    assert_eq!(select["current_conclusion"], "refuted");
}

#[test]
fn audit_deleted_predecessor_reopens_remaining_downstream_decision() {
    let before = baseline();
    normal_control(&before);
    let mut after = before.clone();
    after.decisions.retain(|decision| decision.id != "select");
    let report = assert_review(&before, &after, &["cost"], &["cost"]);
    assert!(ids(&report["affected"], None).contains("select"));
}

#[test]
fn audit_withdrawn_adoption_reopens_downstream_without_changing_conclusion() {
    let before = baseline();
    normal_control(&before);
    let mut after = before.clone();
    after.decisions[0].adopted = false;
    assert_review(&before, &after, &["select", "cost"], &["cost"]);
}

#[test]
fn audit_new_adoption_of_existing_decision_still_requires_fresh_review() {
    let mut before = baseline();
    before.decisions[0].adopted = false;
    normal_control(&before);
    let mut after = before.clone();
    after.decisions[0].adopted = true;
    assert_review(&before, &after, &["select", "cost"], &["select", "cost"]);
}

#[test]
fn audit_source_deletion_reopens_downstream_even_without_explicit_source_edge() {
    let mut before = baseline();
    before.dependencies.remove("support-premise");
    before.decisions[0].basis = vec!["source-secondary".into()];
    normal_control(&before);
    let mut after = before.clone();
    after.evidence.remove("source-primary");
    assert_review(&before, &after, &["select", "cost"], &["select", "cost"]);
}

#[test]
fn audit_retained_digest_alias_does_not_hide_a_changed_source_binding() {
    let mut before = baseline();
    before.dependencies.remove("support-premise");
    before.decisions[0].basis = vec!["source-secondary".into()];
    before
        .evidence
        .insert("archived-source".into(), "a".repeat(64));
    normal_control(&before);
    let mut after = before.clone();
    after
        .evidence
        .insert("source-primary".into(), "d".repeat(64));
    assert!(
        after
            .evidence
            .values()
            .any(|digest| digest == &after.decisions[0].source_sha256)
    );
    assert_review(&before, &after, &["select", "cost"], &["select", "cost"]);
}

#[test]
fn audit_predecessor_change_reopens_a_real_multistep_chain_only() {
    let mut before = baseline();
    for (id, prerequisite, digest) in [("drawing", "select", 'd'), ("inspection", "drawing", 'e')] {
        before
            .evidence
            .insert(format!("source-{id}"), digest.to_string().repeat(64));
        let mut decision = before.decisions[0].clone();
        decision.id = id.into();
        decision.source_sha256 = digest.to_string().repeat(64);
        decision.basis = vec![prerequisite.into()];
        before.decisions.push(decision);
    }
    before.decisions[1].basis = vec!["inspection".into()];
    before
        .dependencies
        .insert("cost".into(), BTreeSet::from(["inspection".into()]));
    normal_control(&before);
    let mut after = before.clone();
    after.decisions[0].conclusion = "unknown".into();
    after.decisions[0].adopted = false;
    assert_review(
        &before,
        &after,
        &["select", "drawing", "inspection", "cost"],
        &["drawing", "inspection", "cost"],
    );
}

#[test]
fn audit_unrelated_source_change_does_not_reopen_decisions() {
    let mut before = baseline();
    before
        .evidence
        .insert("unrelated-source".into(), "d".repeat(64));
    normal_control(&before);
    let mut after = before.clone();
    after
        .evidence
        .insert("unrelated-source".into(), "e".repeat(64));
    assert_review(&before, &after, &[], &[]);
}

#[test]
fn audit_unrelated_dependency_deletion_does_not_reopen_decisions() {
    let mut before = baseline();
    before.dependencies.insert(
        "unused-premise".into(),
        BTreeSet::from(["unlisted-input".into()]),
    );
    normal_control(&before);
    let mut after = before.clone();
    after.dependencies.remove("unused-premise");
    let report = assert_review(&before, &after, &[], &[]);
    assert_eq!(report["affected"], json!(["unused-premise"]));
}

#[test]
fn audit_adding_unrelated_review_does_not_invalidate_existing_decisions() {
    let before = baseline();
    normal_control(&before);
    let mut after = before.clone();
    let mut independent = after.decisions[2].clone();
    independent.id = "recorded-review".into();
    after.decisions.push(independent);
    assert_review(&before, &after, &[], &[]);
}
