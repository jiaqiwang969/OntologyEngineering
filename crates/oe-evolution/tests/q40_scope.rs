//! Structural Q40 scope tests. The partial/unknown witness is intentionally
//! synthetic and cannot stand in for a fresh independent evaluator process.
use oe_contracts::{
    ArtifactReference, CodeBehaviorWitness, CodeBudget, CodeScopeCaseRelation,
    CodeScopeItemEvidence, CodeScopeWitnessReport, CodeScopeWitnessReport2,
    CodeScopeWitnessRequest,
};
use oe_evolution::scope_witness::{self, DENOMINATOR, LOCK_SHA, SCOPE_SHA, SelectedReport, q40};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

const CATALOG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/q40-catalog.json"
));
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}
fn catalog() -> Value {
    assert_eq!(
        oe_core::sha256(CATALOG),
        "31a5ac5f7b95b976c75d809293f613fbd0fb946d0a330fd48d4064550981bbc4"
    );
    strict_json(CATALOG).unwrap()
}
fn unknown_report() -> CodeScopeWitnessReport2 {
    let observation = CodeScopeWitnessReport {
        schema: "oev2.code-scope-witness-observation/2".into(),
        scope_sha256: SCOPE_SHA.into(),
        scope_lock_sha256: LOCK_SHA.into(),
        fixed_scope_count: DENOMINATOR as u64,
        witness: CodeBehaviorWitness {
            schema: "oev2.code-behavior-witness/1".into(),
            experiment_sha256: "a".repeat(64),
            driver_sha256: "a".repeat(64),
            evaluator_sha256: "b".repeat(64),
            oracle_sha256: q40::ORACLE_SHA.into(),
            baseline_sha256: "c".repeat(64),
            candidate_sha256: "d".repeat(64),
            execution: "partial".into(),
            comparison: None,
            process: ArtifactReference {
                path: "var/synthetic-not-executed/process.json".into(),
                sha256: "e".repeat(64),
            },
            uncovered: vec!["structural unknown fixture; no execution witness".into()],
        },
        items: scope_witness::unknown_bindings(root())
            .unwrap()
            .into_iter()
            .map(|row| CodeScopeItemEvidence {
                scope_id: row.scope_id,
                observed_case_ids: vec![],
                behavior_status: "unknown".into(),
            })
            .collect(),
        behavior_status: "unknown".into(),
        eligibility: "unknown".into(),
        whole_code_cycle: "not_connected".into(),
        uncovered: vec!["structural unknown fixture; no execution witness".into()],
    };
    // Populate from the independently frozen catalog, not the function under test.
    let relations = catalog()["relations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| CodeScopeCaseRelation {
            scope_id: row["scope_id"].as_str().unwrap().into(),
            case_id: row["case_id"].as_str().unwrap().into(),
            role: row["role"].as_str().unwrap().into(),
            observation: "unknown".into(),
            behavior_status: "unknown".into(),
        })
        .collect();
    CodeScopeWitnessReport2 {
        schema: "oev2.code-scope-witness-report/2".into(),
        observation,
        catalog_sha256: q40::CATALOG_SHA.into(),
        relations,
    }
}
fn request() -> CodeScopeWitnessRequest {
    CodeScopeWitnessRequest {
        schema: "oev2.code-scope-witness-request/2".into(),
        experiment_sha256: "a".repeat(64),
        dispatch_budget: CodeBudget {
            timeout_ms: 600000,
            output_bytes: 4 * 1024 * 1024,
        },
        items: q40::bindings(root()).unwrap(),
    }
}

#[test]
fn q40_bindings_preserve_every_original_id_and_all40_fixed_cases() {
    let rows = q40::bindings(root()).unwrap();
    let original = scope_witness::unknown_bindings(root()).unwrap();
    let original_ids: BTreeSet<_> = original.iter().map(|row| row.scope_id.as_str()).collect();
    let ids: BTreeSet<_> = rows.iter().map(|row| row.scope_id.as_str()).collect();
    assert_eq!(rows.len(), 13494);
    assert_eq!(ids.len(), 13494);
    assert_eq!(ids, original_ids);
    q40::validate_bindings(root(), &rows).unwrap();
    let all_cases: Vec<_> = rows
        .iter()
        .flat_map(|row| row.case_ids.iter().cloned())
        .collect();
    assert_eq!(all_cases.len(), 40);
    assert_eq!(all_cases.iter().collect::<BTreeSet<_>>().len(), 40);
    let fixed = catalog();
    assert_eq!(fixed["case_ids"].as_array().unwrap().len(), 40);
    for relation in fixed["relations"].as_array().unwrap() {
        let row = rows
            .iter()
            .find(|r| r.scope_id == relation["scope_id"].as_str().unwrap())
            .unwrap();
        assert!(
            row.case_ids
                .iter()
                .any(|id| id == relation["case_id"].as_str().unwrap())
        );
    }
    assert_eq!(
        fixed["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["role"] == "api_subcontract")
            .count(),
        27
    );
    assert_eq!(
        fixed["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["role"] == "dto_wire_subcontract")
            .count(),
        11
    );
    assert_eq!(
        fixed["relations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["role"] == "adapter_guard")
            .count(),
        2
    );
    assert!(
        scope_witness::validate_bindings(root(), &rows).is_err(),
        "old /1 cannot admit new Q40 cases"
    );
    assert_eq!(
        scope_witness::ORACLE_SHA,
        "07c0481d530ff868ca777ae49dc19803bcb0523347baf6b06b7e5ca243be30fe"
    );
    assert_eq!(
        scope_witness::PROTOCOL_SHA,
        "12cf2e3bcd171ac1000373257553dc0202d9bc42a7d8ff46f910338158ea2809"
    );
}

#[test]
fn q40_bindings_reject_deleted_duplicate_relabelled_and_missing_case_rows() {
    let rows = q40::bindings(root()).unwrap();
    let mut missing = rows.clone();
    missing.pop();
    assert!(q40::validate_bindings(root(), &missing).is_err());
    let mut duplicate = rows.clone();
    duplicate[1] = duplicate[0].clone();
    assert!(q40::validate_bindings(root(), &duplicate).is_err());
    let mut fake = rows.clone();
    fake[0].scope_id = "invented|method|NotAnOriginalScope|1".into();
    assert!(q40::validate_bindings(root(), &fake).is_err());
    let i = rows
        .iter()
        .position(|row| !row.case_ids.is_empty())
        .unwrap();
    let mut dropped = rows.clone();
    dropped[i].case_ids.pop();
    assert!(q40::validate_bindings(root(), &dropped).is_err());
    let mut duplicate_case = rows.clone();
    let repeated = duplicate_case[i].case_ids[0].clone();
    duplicate_case[i].case_ids.push(repeated);
    assert!(q40::validate_bindings(root(), &duplicate_case).is_err());
    let mut wrong = rows.clone();
    wrong[i].case_ids[0] = "ask-ground".into();
    assert!(q40::validate_bindings(root(), &wrong).is_err());
    let mut transferred = rows;
    let j = transferred
        .iter()
        .position(|row| row.case_ids.is_empty())
        .unwrap();
    let moved = transferred[i].case_ids.pop().unwrap();
    transferred[j].case_ids.push(moved);
    assert!(q40::validate_bindings(root(), &transferred).is_err());
}

#[test]
fn report2_keeps_unknown_whole_behavior_and_rejects_role_or_observation_forgery() {
    let original = unknown_report();
    q40::validate_report(root(), &original).unwrap();
    assert_eq!(original.observation.items.len(), 13494);
    assert_eq!(original.relations.len(), 40);
    assert!(
        original
            .observation
            .items
            .iter()
            .all(|row| row.behavior_status == "unknown" && row.observed_case_ids.is_empty())
    );
    assert!(
        original
            .relations
            .iter()
            .all(|row| row.behavior_status == "unknown" && row.observation == "unknown")
    );
    assert!(scope_witness::validate_report(root(), &original.observation).is_err());
    let mut missing = original.clone();
    missing.observation.items.pop();
    assert!(q40::validate_report(root(), &missing).is_err());
    let mut duplicate = original.clone();
    duplicate.observation.items[1] = duplicate.observation.items[0].clone();
    assert!(q40::validate_report(root(), &duplicate).is_err());
    let mut fake_id = original.clone();
    fake_id.observation.items[0].scope_id = "invented|class|NoSource|1".into();
    assert!(q40::validate_report(root(), &fake_id).is_err());
    let mut role = original.clone();
    role.relations
        .iter_mut()
        .find(|r| r.role == "adapter_guard")
        .unwrap()
        .role = "api_subcontract".into();
    assert!(q40::validate_report(root(), &role).is_err());
    let mut matched = original.clone();
    matched.relations[0].observation = "matched".into();
    assert!(q40::validate_report(root(), &matched).is_err());
    let mut missing_relation = original.clone();
    missing_relation.relations.pop();
    assert!(q40::validate_report(root(), &missing_relation).is_err());
    let mut repeat_relation = original.clone();
    repeat_relation.relations[1] = repeat_relation.relations[0].clone();
    assert!(q40::validate_report(root(), &repeat_relation).is_err());
    let mut promoted = original.clone();
    promoted.observation.behavior_status = "passed".into();
    assert!(q40::validate_report(root(), &promoted).is_err());
    let mut eligible = original.clone();
    eligible.observation.eligibility = "eligible".into();
    assert!(q40::validate_report(root(), &eligible).is_err());
    let mut scope = original.clone();
    scope.observation.scope_sha256 = "0".repeat(64);
    assert!(q40::validate_report(root(), &scope).is_err());
    let mut lock = original.clone();
    lock.observation.scope_lock_sha256 = "0".repeat(64);
    assert!(q40::validate_report(root(), &lock).is_err());
    let mut catalog_hash = original;
    catalog_hash.catalog_sha256 = "0".repeat(64);
    assert!(q40::validate_report(root(), &catalog_hash).is_err());
}

#[test]
fn selected_report_dispatch_rejects_schema_relabelling_and_legacy_request() {
    let original = unknown_report();
    let request = request();
    let value = serde_json::to_value(&original).unwrap();
    let selected = SelectedReport::parse(value.clone()).unwrap();
    selected.validate(root(), &request).unwrap();
    assert_eq!(selected.value().unwrap(), value);
    let mut legacy_request = request.clone();
    legacy_request.schema = "oev2.code-scope-witness-request/1".into();
    assert!(selected.validate(root(), &legacy_request).is_err());
    let mut unknown_request = request.clone();
    unknown_request.schema = "oev2.code-scope-witness-request/3".into();
    assert!(selected.validate(root(), &unknown_request).is_err());
    let mut label = value.clone();
    label["schema"] = json!("oev2.code-scope-witness-report/1");
    assert!(SelectedReport::parse(label).is_err());
    let mut nested = value.clone();
    nested["observation"]["schema"] = json!("oev2.code-scope-witness-report/1");
    assert!(
        SelectedReport::parse(nested)
            .unwrap()
            .validate(root(), &request)
            .is_err()
    );
    let mut unknown = value;
    unknown["schema"] = json!("oev2.code-scope-witness-report/3");
    assert!(SelectedReport::parse(unknown).is_err());
}
