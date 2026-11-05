use oe_design::{Design, inspection};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn put(root: &Path, path: &str, text: &str) {
    let target = root.join(path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(target, text).unwrap();
}

fn fixture() -> (tempfile::TempDir, PathBuf, Design) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    put(&root, "crates/logic/src/lib.rs", "pub fn calculate() {}\n");
    put(
        &root,
        "crates/review/src/lib.rs",
        "pub fn check() -> bool { false }\n",
    );
    // A parsed test is deliberately not a successful execution receipt.
    put(
        &root,
        "crates/logic/tests/behavior.rs",
        "#[test] fn rejects_overload() { assert!(false); }\n",
    );
    let design = serde_json::from_value(json!({
        "schema":"oev2.design/1", "version":"fixture-1",
        "problem":"Keep the load requirement linked to its separate checker and test",
        "purpose":"Preserve declared responsibilities without inventing engineering results",
        "requirements":[
            {"id":"req_load","function":"Carry the stated load","condition":"Supported at both ends","acceptance":"Deflection under the original limit"},
            {"id":"req_review","function":"Review current inputs","condition":"Current source and object","acceptance":"Reject missing source"}
        ],
        "modules":[
            {"id":"logic","kind":"calculation","rust_path":"crates/logic","purpose":"Load calculation","realizes":["req_load"],"platform":"Declared test host","effects":"Read inputs","failures":"Unknown input remains unknown","status":"behavior_verified"},
            {"id":"review","kind":"governance","rust_path":"crates/review","purpose":"Separate review","realizes":["req_review"],"platform":"Declared test host","effects":"Read evidence","failures":"Reject invalid evidence","status":"source_only"}
        ],
        "types":[],
        "constraints":[{"id":"load_limit","requirement":"req_load","module":"review","check":"check","rust_path":"crates/review/src/lib.rs","statement":"Compare current load against the original limit","severity":"error"}],
        "validations":[{"id":"load_case","module":"logic","constraint":"load_limit","behavior":"rejects_overload","rust_path":"crates/logic/tests/behavior.rs","oracle":"Overload must be rejected","scope":"Software behavior only"}]
    })).unwrap();
    (directory, root, design)
}

fn row<'a>(rows: &'a Value, id: &str) -> &'a Value {
    rows.as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()
}

#[test]
fn declared_requirement_chain_preserves_roles_and_never_claims_execution() {
    let (_directory, root, design) = fixture();
    let report = inspection::inspect(&root, &design).unwrap();
    let trace = &report["traceability"];
    assert_eq!(trace["schema"], "oev2.design-traceability/1");
    let load = row(&trace["requirements"], "req_load");
    assert_eq!(load["condition"], "Supported at both ends");
    assert_eq!(load["acceptance"], "Deflection under the original limit");
    assert_eq!(load["realization_modules"], json!(["logic"]));
    assert_eq!(load["constraints"], json!(["load_limit"]));
    assert_eq!(load["static_gaps"], json!([]));
    let constraint = row(&trace["constraints"], "load_limit");
    assert_eq!(constraint["module"], "review");
    assert_eq!(constraint["checker"]["symbol_present"], true);
    assert_eq!(constraint["validations"], json!(["load_case"]));
    let validation = row(&trace["validations"], "load_case");
    assert_eq!(validation["module"], "logic");
    assert_eq!(validation["test"]["symbol_present"], true);
    assert_eq!(validation["oracle"], "Overload must be rejected");
    assert_eq!(validation["scope"], "Software behavior only");
    assert_eq!(trace["evidence"]["behavior_execution"], "not_run");
    assert_eq!(trace["evidence"]["oracle_independence"], "not_assessed");
    assert_eq!(
        trace["evidence"]["engineering_acceptance"],
        "not_established"
    );
    assert_eq!(trace["evidence"]["method_gain"], "not_evaluated");
}

#[test]
fn unassigned_requirements_and_missing_checks_stay_in_the_denominator() {
    let (_directory, root, mut design) = fixture();
    let mut unassigned = design.requirements[0].clone();
    unassigned.id = "req_unassigned".into();
    design.requirements.push(unassigned);
    let report = inspection::inspect(&root, &design).unwrap();
    let trace = &report["traceability"];
    assert_eq!(trace["counts"]["requirements"], 3);
    assert_eq!(trace["counts"]["requirements_with_static_gaps"], 2);
    let missing = row(&trace["requirements"], "req_unassigned");
    assert_eq!(missing["realization_modules"], json!([]));
    assert_eq!(missing["constraints"], json!([]));
    assert!(
        missing["static_gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["code"] == "requirement_owner_missing")
    );
    assert!(
        missing["static_gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["code"] == "requirement_constraint_missing")
    );
}

#[test]
fn same_name_test_helper_cannot_replace_the_bound_checker_and_current_bytes_are_reported() {
    let (_directory, root, design) = fixture();
    let before = inspection::inspect(&root, &design).unwrap();
    put(
        &root,
        "crates/review/src/lib.rs",
        "#[cfg(test)] pub fn check() {}\n",
    );
    put(
        &root,
        "crates/review/tests/helper.rs",
        "pub fn check() {}\n",
    );
    let after = inspection::inspect(&root, &design).unwrap();
    let old = row(&before["traceability"]["constraints"], "load_limit");
    let new = row(&after["traceability"]["constraints"], "load_limit");
    assert_ne!(
        old["checker"]["source_sha256"],
        new["checker"]["source_sha256"]
    );
    assert_eq!(new["checker"]["symbol_present"], false);
    let load = row(&after["traceability"]["requirements"], "req_load");
    assert!(
        load["static_gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["code"] == "constraint_execution_missing" && v["subject"] == "load_limit")
    );
    std::fs::remove_file(root.join("crates/logic/tests/behavior.rs")).unwrap();
    let after = inspection::inspect(&root, &design).unwrap();
    let load = row(&after["traceability"]["requirements"], "req_load");
    assert!(
        load["static_gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["code"] == "behavior_case_missing" && v["subject"] == "load_case")
    );
    assert_eq!(after["traceability"]["counts"]["validations"], 1);
}

#[test]
fn condition_revisions_change_trace_identity_but_editor_order_does_not() {
    let (_directory, root, mut design) = fixture();
    let before = inspection::inspect(&root, &design).unwrap();
    design.requirements.reverse();
    design.modules.reverse();
    let reordered = inspection::inspect(&root, &design).unwrap();
    assert_eq!(before["traceability"], reordered["traceability"]);
    let requirement = design
        .requirements
        .iter_mut()
        .find(|r| r.id == "req_load")
        .unwrap();
    requirement.condition = "Supported at one end".into();
    let after = inspection::inspect(&root, &design).unwrap();
    assert_ne!(
        before["traceability"]["design_sha256"],
        after["traceability"]["design_sha256"]
    );
    assert_eq!(
        row(&after["traceability"]["requirements"], "req_load")["condition"],
        "Supported at one end"
    );
    assert_eq!(
        after["traceability"]["evidence"]["engineering_acceptance"],
        "not_established"
    );
}
