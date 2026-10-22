//! Actual production CLI and independently authored synthetic protocol inputs.
#[path = "../../oe-cad/tests/support/copy_fixture.rs"]
mod fixture;
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn copy_cli_preserves_projects_and_reviews_both_dependency_versions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // Retain the exact positive and negative caller inputs/outputs as private
    // receiving evidence; no fixture is published or treated as actual CAD.
    let directory = tempfile::tempdir_in(root.join("var")).unwrap().keep();
    println!("retained_copy_cli_fixture={}", directory.display());
    let mut f =
        fixture::CopyFixture::in_root(SkillRoot::new(&root).unwrap(), &directory.join("fixture"));
    let old = fs::read(root.join(&f.request.previous_project.path)).unwrap();
    let new = fs::read(root.join(&f.request.project.path)).unwrap();
    let call = |f: &fixture::CopyFixture, label: &str| {
        let input = f
            .base
            .f
            .write_json(label, &serde_json::to_value(&f.request).unwrap());
        let output = directory.join(label);
        let argv = vec![
            "--root".to_owned(),
            root.to_string_lossy().into_owned(),
            "cad".into(),
            "evidence".into(),
            "verify-copy".into(),
            "--input".into(),
            root.join(input.path).to_string_lossy().into_owned(),
            "--output".into(),
            output.to_string_lossy().into_owned(),
        ];
        let result = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .args(&argv)
            .output()
            .unwrap();
        fs::write(directory.join(format!("{label}.stdout")), &result.stdout).unwrap();
        fs::write(directory.join(format!("{label}.stderr")), &result.stderr).unwrap();
        fs::write(directory.join(format!("{label}.invocation.json")),serde_json::to_vec_pretty(&json!({
            "program":env!("CARGO_BIN_EXE_oev2"),"argv":argv,"exit_code":result.status.code(),"fixture":"synthetic protocol; no native dispatch"})).unwrap()).unwrap();
        (result, output)
    };
    let (result, output) = call(&f, "copy-complete");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["schema"], "oev2.current-cad-assembly-copy/1");
    assert_eq!(report["copy"]["independent_reinspection_checked"], true);
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(report["project_write"], "not_run");
    for id in ["drawing", "fit", "manufacturing", "inspection"] {
        assert!(
            report["requires_review"]
                .as_array()
                .unwrap()
                .contains(&json!(id)),
            "{id}: {report}"
        );
    }
    assert_eq!(
        fs::read(root.join(&f.request.previous_project.path)).unwrap(),
        old
    );
    assert_eq!(fs::read(root.join(&f.request.project.path)).unwrap(), new);
    for fault in [
        "missing_intent",
        "missing_native",
        "project_id",
        "stale_context",
    ] {
        let original = f.request.clone();
        let mut project = f.base.f.read_json(&f.request.project);
        match fault {
            "missing_intent" => {
                project["evidence"]
                    .as_object_mut()
                    .unwrap()
                    .remove("copy-intent");
            }
            "missing_native" => {
                project["evidence"]
                    .as_object_mut()
                    .unwrap()
                    .remove("copy-native");
            }
            "project_id" => project["project_id"] = json!("unrelated-project"),
            _ => project["revision"] = json!("changed-without-context"),
        }
        f.request.project = f.base.f.write_json(fault, &project);
        if fault != "stale_context" {
            f.request.assembly.project_context =
                fixture::construction::legacy::occurrence::definition::context(
                    &project,
                    &f.request.assembly.object,
                );
        }
        let (result, output) = call(&f, fault);
        assert_eq!(
            result.status.code(),
            Some(2),
            "{fault}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!output.exists(), "{fault}");
        f.request = original;
    }
}
