//! New execution admission controls. No model/native/remote execution occurs.
use oe_evolution::native_stream11::selected_resources11;
use oe_evolution::*;
use serde_json::json;

fn request() -> (tempfile::TempDir, CodeNativeBudgetedBuildRequest11) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().canonicalize().unwrap();
    std::fs::write(path.join("SKILL.md"), b"controlled native11 admission").unwrap();
    let input = CodeNativeBudgetedBuildRequest11 {
        schema: "oev2.native-budgeted-build-request/11".into(),
        build: CodeBuildRequest {
            artifact_id: "native11-admission-control".into(),
            source_root: path.display().to_string(),
            evidence_path: path.join("var/evidence").display().to_string(),
            target_dir: path.join("var/target").display().to_string(),
            cargo_home: path.join("var/cargo-home").display().to_string(),
            cargo_path: path.join("never-execute").display().to_string(),
            rustc_path: path.join("never-execute").display().to_string(),
            ggen_path: path.join("never-execute").display().to_string(),
            platform: "aarch64-apple-darwin".into(),
        },
        budget: CodeBudget {
            timeout_ms: 60_000,
            output_bytes: 4 * 1024 * 1024,
        },
        side_context: ArtifactReference {
            path: path
                .join("var/unprovided-owner-side.json")
                .display()
                .to_string(),
            sha256: "0".repeat(64),
        },
        resources: selected_resources11(),
    };
    (folder, input)
}

#[test]
fn explicit_stream11_resources_cannot_expand_original_body_budget() {
    let (folder, mut input) = request();
    input.budget.output_bytes = selected_resources11().raw_stream_bytes;
    let error = native_runtime11::build_full_native_budgeted(&input).unwrap_err();
    assert!(format!("{error:#}").contains("schema_or_bounds"));
    assert!(!folder.path().join("var").exists());
}

#[test]
fn explicit_stream11_rejects_legacy_schema_and_unselected_resources_before_dispatch() {
    let (folder, input) = request();
    let mut changed = input.clone();
    changed.schema = "oev2.native-budgeted-build-request/10".into();
    let error = format!(
        "{:#}",
        native_runtime11::build_full_native_budgeted(&changed).unwrap_err()
    );
    assert!(
        error.contains("native6_request:") && error.contains("schema:allowed_values"),
        "{error}"
    );
    for key in [
        "raw_stream_bytes",
        "preview_bytes",
        "segment_bytes",
        "maximum_lines",
        "maximum_derived_bytes",
    ] {
        let mut value = serde_json::to_value(&input).unwrap();
        value["resources"][key] = json!(value["resources"][key].as_u64().unwrap() + 1);
        let changed: CodeNativeBudgetedBuildRequest11 = serde_json::from_value(value).unwrap();
        assert!(
            format!(
                "{:#}",
                native_runtime11::build_full_native_budgeted(&changed).unwrap_err()
            )
            .contains("stream11_unselected_resource_baseline"),
            "{key}"
        );
    }
    assert!(!folder.path().join("var").exists());
}

#[test]
fn selected_stream_resources_do_not_replace_actual_owner_side_source_admission() {
    let (folder, input) = request();
    let outcome =
        native_runtime11::NativeRuntimeBuilder11.build_full_native_budgeted_runtime(&input);
    let failure = outcome.unwrap_err();
    assert!(!failure.retryable);
    assert_eq!(
        failure.partial_result.as_ref().unwrap()["execution"],
        "failed"
    );
    assert_eq!(
        failure.partial_result.as_ref().unwrap()["release_eligibility"],
        "unknown"
    );
    assert!(!folder.path().join("var").exists());
}

#[test]
fn new_execution_contract_requires_explicit_resources_without_legacy_conversion() {
    let (_, input) = request();
    let mut missing = serde_json::to_value(&input).unwrap();
    missing.as_object_mut().unwrap().remove("resources");
    assert!(serde_json::from_value::<CodeNativeBudgetedBuildRequest11>(missing).is_err());
    let mut extra = serde_json::to_value(&input).unwrap();
    extra["legacy_output_bytes"] = json!(4 * 1024 * 1024);
    assert!(serde_json::from_value::<CodeNativeBudgetedBuildRequest11>(extra).is_err());
    assert_ne!(native_runtime11::PROFILE, native_runtime10::PROFILE);
}
