//! CLI synthetic receiving only; no CAD, native execution or API calls.
#[path = "../../oe-cad/tests/support/drafting_reopen_fixture.rs"]
mod fixture;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn invoke(root: &Path, f: &fixture::Fixture, output: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
    command
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args(["cad", "drawing", "compare-drafting", "--saved-job"])
        .arg(&f.saved)
        .arg("--reopened-job")
        .arg(&f.reopened)
        .arg("--output")
        .arg(output);
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.output().unwrap()
}

#[test]
fn prototype_drafting_cli_receives_page_without_adopting_dimensions_or_engineering() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir_in(root.join("var")).unwrap();
    for kind in ["page", "empty", "same-process"] {
        let f = fixture::Fixture::new(&temp.path().join(kind));
        if kind == "same-process" {
            f.use_same_process();
        }
        if kind == "empty" {
            f.edit_result(false, |r| {
            r.readback["parts"][0]["drawing_sheets"][0]["views"][0]["visible_objects"]["visible_count"] = json!(0);
            r.readback["parts"][0]["drawing_sheets"][0]["views"][0]["visible_objects"]["visible_tags"] = json!([]);
        });
        }
        let out = temp.path().join(format!("{kind}-report.json"));
        let actual = invoke(&root, &f, &out);
        if kind == "same-process" {
            assert_eq!(actual.status.code(), Some(2));
            assert!(actual.stdout.is_empty());
            assert!(!out.exists());
            let failure: Value = serde_json::from_slice(&actual.stderr).unwrap();
            assert_eq!(
                failure,
                json!({
                    "schema": "oev2.command-failure/1",
                    "command": "cad",
                    "execution": "failed",
                    "conclusion": "unknown",
                    "coverage": [],
                    "engineering_adoption": "blocked",
                    "error": {
                        "code": "invalid_contract",
                        "reason": "input_or_output_contract_unavailable"
                    },
                    "retryable": false,
                    "next_step": "inspect_declared_contract_and_retained_private_evidence"
                })
            );
            let owner_error =
                oe_cad::drawing::drafting::compare_saved_reopened(&f.saved, &f.reopened)
                    .unwrap_err();
            assert_eq!(
                owner_error.to_string(),
                "drafting_reopen_process_or_tool_identity_mismatch"
            );
        } else {
            assert_eq!(
                actual.status.code(),
                Some(if kind == "page" { 0 } else { 2 }),
                "{}",
                String::from_utf8_lossy(&actual.stderr)
            );
            let report: Value = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
            assert_eq!(
                report["conclusion"],
                if kind == "page" { "pass" } else { "violated" }
            );
            assert_eq!(report["dimension_obligations"], "unchanged");
            assert_eq!(report["engineering_adoption"], "not_established");
            assert_eq!(report["project_revision_update"], "not_performed");
            assert!(report["execution_binding"].is_object());
        }
    }
}
