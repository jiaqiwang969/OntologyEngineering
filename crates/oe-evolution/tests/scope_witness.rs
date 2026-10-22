use oe_contracts::{
    ArtifactReference, CodeBehaviorWitness, CodeScopeItemEvidence, CodeScopeWitnessReport,
};
use oe_evolution::scope_witness::{self, DENOMINATOR, LOCK_SHA, SCOPE_SHA};
use std::path::Path;
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}
fn unknown_report() -> CodeScopeWitnessReport {
    let items = scope_witness::unknown_bindings(root())
        .unwrap()
        .into_iter()
        .map(|b| CodeScopeItemEvidence {
            scope_id: b.scope_id,
            observed_case_ids: vec![],
            behavior_status: "unknown".into(),
        })
        .collect();
    CodeScopeWitnessReport {
        schema: "oev2.code-scope-witness-report/1".into(),
        scope_sha256: SCOPE_SHA.into(),
        scope_lock_sha256: LOCK_SHA.into(),
        fixed_scope_count: DENOMINATOR as u64,
        witness: CodeBehaviorWitness {
            schema: "oev2.code-behavior-witness/1".into(),
            experiment_sha256: "a".repeat(64),
            driver_sha256: "a".repeat(64),
            evaluator_sha256: "a".repeat(64),
            oracle_sha256: "a".repeat(64),
            baseline_sha256: "a".repeat(64),
            candidate_sha256: "a".repeat(64),
            execution: "partial".into(),
            comparison: None,
            process: ArtifactReference {
                path: "var/not-executed/process.json".into(),
                sha256: "a".repeat(64),
            },
            uncovered: vec!["structural unknown fixture, no execution witness".into()],
        },
        items,
        behavior_status: "unknown".into(),
        eligibility: "unknown".into(),
        whole_code_cycle: "not_connected".into(),
        uncovered: vec!["no actual execution".into()],
    }
}
#[test]
fn complete_original_ids_are_required() {
    let rows = scope_witness::unknown_bindings(root()).unwrap();
    assert_eq!(rows.len(), 13_494);
    scope_witness::validate_bindings(root(), &rows).unwrap();
    let mut missing = rows.clone();
    missing.pop();
    assert!(
        scope_witness::validate_bindings(root(), &missing)
            .unwrap_err()
            .to_string()
            .contains("missing_scope_id")
    );
    let mut duplicate = rows.clone();
    duplicate[1] = duplicate[0].clone();
    assert!(
        scope_witness::validate_bindings(root(), &duplicate)
            .unwrap_err()
            .to_string()
            .contains("duplicate_scope_id")
    );
    let mut unknown = rows;
    unknown[0].scope_id = "nonexistent|method|fake|1".into();
    assert!(
        scope_witness::validate_bindings(root(), &unknown)
            .unwrap_err()
            .to_string()
            .contains("unknown_scope_id")
    );
}
#[test]
fn unapproved_case_to_api_binding_is_rejected() {
    let mut rows = scope_witness::unknown_bindings(root()).unwrap();
    rows[0].case_ids = vec!["ask-ground".into()];
    assert!(
        scope_witness::validate_bindings(root(), &rows)
            .unwrap_err()
            .to_string()
            .contains("unapproved_case_to_api")
    );
    rows[0].case_ids.clear();
    let datalog = rows
        .iter_mut()
        .find(|r| {
            r.scope_id == "semantica/reasoning/datalog_reasoner.py|method|DatalogReasoner.query|344"
        })
        .unwrap();
    datalog.case_ids = vec![
        "datalog-recursive-multivar".into(),
        "datalog-repeated-variable".into(),
    ];
    scope_witness::validate_bindings(root(), &rows).unwrap();
}
#[test]
fn unexecuted_and_self_verified_reports_are_rejected() {
    let mut report = unknown_report();
    // This is a structural partial/unknown fixture only, never execution proof.
    scope_witness::validate_report(root(), &report).unwrap();
    report.items[0].behavior_status = "behavior_verified".into();
    assert!(scope_witness::validate_report(root(), &report).is_err());
    report.items[0].behavior_status = "unknown".into();
    report
        .items
        .iter_mut()
        .find(|i| {
            i.scope_id == "semantica/reasoning/datalog_reasoner.py|method|DatalogReasoner.query|344"
        })
        .unwrap()
        .observed_case_ids = vec!["datalog-repeated-variable".into()];
    assert!(
        scope_witness::validate_report(root(), &report)
            .unwrap_err()
            .to_string()
            .contains("unearned_case_observation")
    );
}
#[test]
fn report_denominator_scope_and_owner_status_cannot_be_rewritten() {
    let original = unknown_report();
    let mut missing = original.clone();
    missing.items.pop();
    assert!(scope_witness::validate_report(root(), &missing).is_err());
    let mut dup = original.clone();
    dup.items[1] = dup.items[0].clone();
    assert!(scope_witness::validate_report(root(), &dup).is_err());
    let mut scope = original.clone();
    scope.scope_sha256 = "b".repeat(64);
    assert!(scope_witness::validate_report(root(), &scope).is_err());
    let mut eligible = original;
    eligible.eligibility = "eligible".into();
    assert!(scope_witness::validate_report(root(), &eligible).is_err());
}
