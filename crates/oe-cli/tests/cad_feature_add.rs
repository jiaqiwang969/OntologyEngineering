//! Actual CLI against independent synthetic fixtures; no native dispatch/API.
#[path = "../../oe-cad/tests/support/feature_add_fixture.rs"]
mod fixture;
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn call(root: &Path, directory: &Path, f: &fixture::AddFixture, label: &str) -> (Output, PathBuf) {
    let input =
        f.f.write_json(label, &serde_json::to_value(&f.request).unwrap());
    let output = directory.join(label);
    let argv = vec![
        "--root".to_owned(),
        root.to_string_lossy().into_owned(),
        "cad".into(),
        "evidence".into(),
        "verify-addition".into(),
        "--input".into(),
        root.join(input.path).to_string_lossy().into_owned(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
    ];
    let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
    command
        .args(&argv)
        .env_clear()
        .env("PATH", "")
        .env("HOME", directory.join("home"));
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let result = command.output().unwrap();
    fs::write(directory.join(format!("{label}.stdout")), &result.stdout).unwrap();
    fs::write(directory.join(format!("{label}.stderr")), &result.stderr).unwrap();
    fs::write(directory.join(format!("{label}.invocation.json")),serde_json::to_vec_pretty(&json!({
        "program":env!("CARGO_BIN_EXE_oev2"),"argv":argv,"exit_code":result.status.code(),
        "fixture":"authored synthetic protocol only; no NX file, native execution, model/API or engineering adoption",
        "credentials_inherited":false})).unwrap()).unwrap();
    (result, output)
}

#[test]
fn addition_cli_reviews_old_and_new_dependencies() {
    exercise_addition_cli(false);
}

#[test]
fn addition_cli_same_leaf_target_preserves_old_and_new_project_review() {
    exercise_addition_cli(true);
}

fn exercise_addition_cli(same_leaf_target: bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let directory = tempfile::tempdir_in(root.join("var")).unwrap().keep();
    println!("retained_feature_add_cli_fixture={}", directory.display());
    let mut f = if same_leaf_target {
        fixture::AddFixture::in_root_same_leaf_target(
            SkillRoot::new(&root).unwrap(),
            &directory.join("fixture"),
        )
    } else {
        fixture::AddFixture::in_root(SkillRoot::new(&root).unwrap(), &directory.join("fixture"))
    };
    let independent = f.f.write_json(
        "independent-source",
        &json!({"scope":"unchanged independent synthetic deliverable"}),
    );
    f.edit_projects(|old,new| {
        for p in [old,new].iter_mut() {
            p["objects"]["deliverable"]=json!("stable-v1");
            p["conditions"]["independent"]=json!("Unchanged condition, separate from modified plate");
            p["evidence"]["independent-source"]=json!(independent.sha256);
            p["decisions"]=json!([
                ("old-only-decision","old-downstream"),
                ("new-only-decision","new-downstream"),
                ("unrelated-decision","independent-source")
            ].map(|(id,basis)|json!({"id":id,"object_id":"deliverable","object_revision":"stable-v1",
                "condition_scope":"independent","source_sha256":independent.sha256,
                "conclusion":"unknown","adopted":false,"basis":[basis]})));
        }
    });
    f.edit_projects(|old, new| {
        old["dependencies"]["old-downstream"] = json!(["inspection:left"]);
        old["dependencies"]["new-downstream"] = json!(["independent-source"]);
        new["dependencies"]
            .as_object_mut()
            .unwrap()
            .remove("old-downstream");
        new["dependencies"]["new-downstream"] = json!(["inspection:left"]);
    });
    let old = fs::read(root.join(&f.correspondence.previous.project.path)).unwrap();
    let new = fs::read(root.join(&f.correspondence.current.project.path)).unwrap();
    let (result, output) = call(&root, &directory, &f, "addition-complete");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["schema"], "oev2.current-cad-feature-add/1");
    assert_eq!(report["addition"]["independent_reinspection_checked"], true);
    assert_eq!(
        report["addition"]["current_target_mapping"],
        "unique_in_complete_reopened_geometry"
    );
    assert_eq!(
        report["requires_review"],
        json!(["new-only-decision", "old-only-decision"])
    );
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(report["project_write"], "not_run");
    assert_eq!(
        report["unknown_dependencies"],
        "unlisted links remain unverified"
    );
    assert_eq!(
        fs::read(root.join(&f.correspondence.previous.project.path)).unwrap(),
        old
    );
    assert_eq!(
        fs::read(root.join(&f.correspondence.current.project.path)).unwrap(),
        new
    );

    if same_leaf_target {
        assert_eq!(
            report["addition"]["assembly"]["assembly"]["root_prototype_id"],
            "root.prt"
        );
        assert_eq!(
            report["addition"]["assembly"]["assembly"]["prototypes"][1]["prototype_id"],
            "parts/root.prt"
        );
        return;
    }

    let original = f.correspondence.clone();
    let original_request = f.request.clone();
    for fault in [
        "missing_add_intent",
        "missing_add_native",
        "missing_assembly_native",
        "project_id",
        "stale_context",
    ] {
        f.correspondence = original.clone();
        f.request = original_request.clone();
        let mut p = f.f.read_json(&f.correspondence.current.project);
        match fault {
            "missing_add_intent" => {
                p["evidence"]
                    .as_object_mut()
                    .unwrap()
                    .remove("addition-intent");
            }
            "missing_add_native" => {
                p["evidence"]
                    .as_object_mut()
                    .unwrap()
                    .remove("addition-native");
            }
            "missing_assembly_native" => {
                p["evidence"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&f.request.assembly.native_result_ref);
            }
            "project_id" => p["project_id"] = json!("different-project"),
            _ => p["revision"] = json!("changed-without-context"),
        }
        f.correspondence.current.project = f.f.write_json(fault, &p);
        if fault == "stale_context" {
            f.request.correspondence = f.f.write_json(
                "stale-correspondence",
                &serde_json::to_value(&f.correspondence).unwrap(),
            );
        } else {
            f.persist_correspondence();
        }
        let (result, output) = call(&root, &directory, &f, fault);
        assert_eq!(
            result.status.code(),
            Some(2),
            "{fault}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            !output.exists(),
            "rejected {fault} must not create a receiving report"
        );
    }
}
