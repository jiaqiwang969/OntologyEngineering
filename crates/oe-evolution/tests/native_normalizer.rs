//! Admission fixtures never execute a native compiler or confer owner authority.
use oe_evolution::native_stream11::selected_resources11;
use oe_evolution::*;
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
fn preparation_rejects_wrong_schema_and_expanded_budget_before_dispatch() {
    let (folder, request) = request();
    let valid = CodeNativeNormalizerPreparationRequest1 {
        schema: "oev2.native-normalizer-preparation-request/1".into(),
        request,
    };
    let mut wrong = valid.clone();
    wrong.schema = "oev2.native-budgeted-build-request/11".into();
    assert!(
        native_normalizer::prepare_normalizer(&wrong)
            .unwrap_err()
            .to_string()
            .contains("normalizer_preparation_request")
    );
    wrong = valid.clone();
    wrong.request.budget.timeout_ms = 3_600_001;
    assert!(
        native_normalizer::prepare_normalizer(&wrong)
            .unwrap_err()
            .to_string()
            .contains("budget_bounds")
    );
    wrong = valid.clone();
    wrong.request.budget.output_bytes = 4 * 1024 * 1024 + 1;
    assert!(
        native_normalizer::prepare_normalizer(&wrong)
            .unwrap_err()
            .to_string()
            .contains("budget_bounds")
    );
    wrong = valid.clone();
    wrong.request.resources.raw_stream_bytes += 1;
    assert!(native_normalizer::prepare_normalizer(&wrong).is_err());
    assert!(native_normalizer::prepare_normalizer(&valid).is_err());
    assert!(!folder.path().join("var").exists());
}

#[test]
fn preparation_result_cannot_be_used_as_full_native_capture() {
    let binary = CodeBinaryRef {
        path: "/fixture/bin".into(),
        sha256: "a".repeat(64),
        bytes: 1,
        source_root: "/fixture".into(),
        source_manifest_sha256: "b".repeat(64),
    };
    let reference = ArtifactReference {
        path: "/fixture/record.json".into(),
        sha256: "c".repeat(64),
    };
    let result = CodeNativeNormalizerPreparation1 {
        schema: "oev2.native-normalizer-preparation/1".into(),
        source_profile: reference.clone(),
        side_context: reference.clone(),
        build_driver: binary.clone(),
        normalizer: binary.clone(),
        tools: vec![binary.clone(), binary.clone(), binary],
        source_manifest_sha256: "b".repeat(64),
        process: reference.clone(),
        stream_capture: reference,
        full_build: "not_run".into(),
        release_eligibility: "unknown".into(),
        uncovered: vec!["fixture; actual full build absent".into()],
    };
    result.validate().unwrap();
    assert!(
        serde_json::from_value::<CodeNativeBuildCapture11>(serde_json::to_value(&result).unwrap())
            .is_err()
    );
    assert!(
        serde_json::from_value::<CodeSourceArtifact>(serde_json::to_value(&result).unwrap())
            .is_err()
    );
    let mut wrong = result;
    wrong.full_build = "completed".into();
    assert!(wrong.validate().is_err());
    wrong.full_build = "not_run".into();
    wrong.release_eligibility = "eligible".into();
    assert!(wrong.validate().is_err());
}
