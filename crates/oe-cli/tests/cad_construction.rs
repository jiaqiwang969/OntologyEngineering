//! Actual current CLI, synthetic source files; no native task is dispatched.
#[path = "../../oe-cad/tests/support/construction_fixture.rs"]
mod fixture;
#[path = "../../oe-cad/tests/support/construction2_fixture.rs"]
mod scoped_fixture;
use fixture::{ConstructionFixture, occurrence::definition};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn construction2_cli_keeps_version_and_current_project_checks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temporary = tempfile::tempdir_in(root.join("var")).unwrap();
    let mut f = scoped_fixture::scope_fixture(Some((
        SkillRoot::new(&root).unwrap(),
        &temporary.path().join("scoped-fixture"),
    )));
    let before = fs::read(root.join(&f.request.project.path)).unwrap();
    let call = |value: &Value, name: &str| {
        let request = f.f.write_json(name, value);
        let out = temporary.path().join(name);
        let response = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(&root)
            .args(["cad", "evidence", "verify-construction", "--input"])
            .arg(root.join(request.path))
            .arg("--output")
            .arg(&out)
            .output()
            .unwrap();
        (response, out)
    };
    let (old, out) = call(
        &serde_json::to_value(&f.request).unwrap(),
        "scope-legacy-refusal",
    );
    assert_eq!(old.status.code(), Some(2));
    assert!(!out.exists());
    let (current, out) = call(
        &serde_json::to_value(scoped_fixture::request(&f)).unwrap(),
        "scope-current",
    );
    assert!(
        current.status.success(),
        "{}",
        String::from_utf8_lossy(&current.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["schema"], "oev2.current-cad-assembly-construction/2");
    assert_eq!(
        report["request"]["schema"],
        "oev2.cad-assembly-construction/2"
    );
    assert_eq!(
        report["construction"]["schema"],
        "oev2.cad-assembly-construction-report/2"
    );
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(
        fs::read(root.join(&f.request.project.path)).unwrap(),
        before
    );
    let mut project = f.f.read_json(&f.request.project);
    project["evidence"]
        .as_object_mut()
        .unwrap()
        .remove("construction-intent");
    f.request.project = f.f.write_json("scope-missing-source-project", &project);
    f.request.assembly.project_context =
        fixture::occurrence::definition::context(&project, &f.request.assembly.object);
    // A new request/output preserves the successful input and original refusal.
    let request = f.f.write_json(
        "scope-missing-source",
        &serde_json::to_value(scoped_fixture::request(&f)).unwrap(),
    );
    let out = temporary.path().join("scope-missing-source-result");
    let response = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(&root)
        .args(["cad", "evidence", "verify-construction", "--input"])
        .arg(root.join(request.path))
        .arg("--output")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(response.status.code(), Some(2));
    assert!(!out.exists());
    let refusal: Value = serde_json::from_slice(&response.stderr).unwrap();
    assert_eq!(
        refusal["error"]["reason"],
        "cad_construction_source_not_in_project"
    );
}

#[test]
fn construction_cli_requires_current_project_sources_and_preserves_review() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temporary = tempfile::tempdir_in(root.join("var")).unwrap();
    let fixture_directory = temporary.path().join("fixture");
    let mut f = ConstructionFixture::in_root(SkillRoot::new(&root).unwrap(), &fixture_directory);
    let original = fs::read(root.join(&f.request.project.path)).unwrap();
    let run = |f: &ConstructionFixture, name: &str| {
        let request =
            f.f.write_json(name, &serde_json::to_value(&f.request).unwrap());
        let out = temporary.path().join(name);
        let response = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(&root)
            .args(["cad", "evidence", "verify-construction", "--input"])
            .arg(root.join(request.path))
            .arg("--output")
            .arg(&out)
            .output()
            .unwrap();
        (response, out)
    };
    let (response, out) = run(&f, "construction-ok");
    assert!(
        response.status.success(),
        "{}",
        String::from_utf8_lossy(&response.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(out.join("report.json")).unwrap()).unwrap();
    assert_eq!(
        report["construction"]["instances"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(report["project_write"], "not_run");
    assert_eq!(
        fs::read(root.join(&f.request.project.path)).unwrap(),
        original
    );
    let mut project = f.f.read_json(&f.request.project);
    project["evidence"]
        .as_object_mut()
        .unwrap()
        .remove("construction-intent");
    f.request.project = f.f.write_json("missing-construction-source", &project);
    f.request.assembly.project_context = definition::context(&project, &f.request.assembly.object);
    let (response, out) = run(&f, "construction-missing-source");
    assert_eq!(response.status.code(), Some(2));
    assert!(!out.exists());
    let failure: Value = serde_json::from_slice(&response.stderr).unwrap();
    assert_eq!(
        failure["error"]["reason"],
        "cad_construction_source_not_in_project"
    );
    assert_eq!(failure["engineering_adoption"], "blocked");
    assert_eq!(failure["retryable"], false);
    f.refresh_project();
    let mut project = f.f.read_json(&f.request.project);
    project["evidence"]["new-condition-source"] = json!("0".repeat(64));
    f.request.project = f.f.write_json("stale-construction-project", &project);
    let (response, out) = run(&f, "construction-stale-context");
    assert_eq!(response.status.code(), Some(2));
    assert!(!out.exists());
}
