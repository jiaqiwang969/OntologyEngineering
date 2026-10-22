use oe_contracts::ObjectIdentity;
use oe_kernel::{engineering, project::Project};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
fn fixture() -> (oe_core::SkillRoot, Project, ObjectIdentity) {
    let root = PathBuf::from(
        std::env::var("OE_SHEET_KERNEL_ROOT").expect("explicit original root required"),
    );
    let p =
        serde_json::from_slice(&fs::read(root.join("var/kernel-project-01/project.json")).unwrap())
            .unwrap();
    let object =
        serde_json::from_slice(&fs::read(root.join("var/kernel-project-01/object.json")).unwrap())
            .unwrap();
    (oe_core::SkillRoot::new(root).unwrap(), p, object)
}
fn alter(p: &mut Project, o: &ObjectIdentity, f: impl FnOnce(&mut Value)) {
    let mut b: Value = serde_json::from_str(&p.conditions[&o.condition_scope]).unwrap();
    f(&mut b);
    p.conditions.insert(
        o.condition_scope.clone(),
        serde_json::to_string(&b).unwrap(),
    );
}
fn native(root: &oe_core::SkillRoot, p: &Project, o: &ObjectIdentity) -> oe_kernel::CheckResult {
    engineering::checks_current(root, p, o)
        .into_iter()
        .find(|r| r.id == "engineering_native_source:sheet-persistence")
        .unwrap()
}
#[test]
#[ignore = "requires explicit original v2 jobs and source-bound Project"]
fn current_persistence_recomputes_originals_and_rejects_changed_obligations() {
    let (root, p, o) = fixture();
    let checks = engineering::checks_current(&root, &p, &o);
    let a = checks
        .iter()
        .find(|r| r.id == "engineering:sheet-persistence")
        .unwrap();
    assert_eq!(a.conclusion, "pass");
    let ok = native(&root, &p, &o);
    assert_eq!(ok.conclusion, "pass", "{ok:?}");
    let affected = ok.result["details"]["affected_decisions"]
        .as_array()
        .unwrap();
    for n in ["drawing", "inspection", "manufacturing"] {
        assert!(affected.contains(&json!(n)));
    }
    for mode in ["intent", "current-object", "missing-job"] {
        let mut p = p.clone();
        alter(&mut p, &o, |b| {
            let a = b["assertions"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|a| a["assertion_id"] == "sheet-persistence")
                .unwrap();
            match mode {
                "intent" => {
                    a["requirements"]["reference"]["dimension_value"]["value"] = json!(999.)
                }
                "current-object" => a["requirements"]["object"]["revision"] = json!("wrong"),
                "missing-job" => {
                    a["requirements"]["saved_job_directory"] = json!("var/absent-sheet-job")
                }
                _ => unreachable!(),
            }
            let req = a["requirements"].clone();
            b["facts"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|f| f["property"] == "sheet_reference_persistence:shaft-A-total-length")
                .unwrap()["value"] = req;
            b["facts"].as_array_mut().unwrap().push(json!({"property":"cached_persistence_pass","value":{"conclusion":"pass"},"status":"declared","source_refs":["native-create"]}));
        });
        assert_ne!(native(&root, &p, &o).conclusion, "pass", "{mode}");
    }
    let mut unlinked = p.clone();
    unlinked.dependencies.remove("drawing");
    assert_ne!(
        native(&root, &unlinked, &o).conclusion,
        "pass",
        "missing real source-to-decision dependency must refuse"
    );
    let mut source = p.clone();
    source
        .evidence
        .insert("native-create".into(), "0".repeat(64));
    assert_eq!(native(&root, &source, &o).conclusion, "violated");
    let mut stale = p.clone();
    alter(&mut stale, &o, |b| {
        b["facts"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|f| f["property"] == "sheet_reference_persistence:shaft-A-total-length")
            .unwrap()["status"] = json!("unknown");
    });
    assert_ne!(native(&root, &stale, &o).conclusion, "pass");
}
