//! Explicit preparation on the existing native session. No full-build capture.
use crate::native_runtime::{deadline_timeout, read, reference};
use crate::native_runtime6::{NativeBuildSession6, verify_pins};
use crate::*;

pub const FILE: &str = "native-normalizer-preparation.json";

pub fn prepare_normalizer(
    input: &CodeNativeNormalizerPreparationRequest1,
) -> Result<CodeNativeNormalizerPreparation1> {
    let started = Instant::now();
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("normalizer_preparation_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.native-normalizer-preparation-request/1",
        "normalizer_preparation_schema"
    );
    let request = &input.request;
    crate::native_stream11::verify_resources11(&request.resources)?;
    ensure!(
        (1..=3_600_000).contains(&request.budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&request.budget.output_bytes),
        "normalizer_preparation_budget_bounds"
    );
    let deadline = started
        .checked_add(Duration::from_millis(request.budget.timeout_ms))
        .context("normalizer_preparation_deadline_overflow")?;
    let session = NativeBuildSession6::new_parts(
        &request.build,
        &request.budget,
        &request.side_context,
        deadline,
    )?;
    ensure!(
        crate::native_link8::is_adapter8(&session.recipe)?,
        "compiler7_selected_adapter_required"
    );
    let built = crate::prepare_normalizer_with_deadline(&request.build, &session)?;
    let root = &session.root;
    let evidence = root.private_path(Path::new(&request.build.evidence_path))?;
    let preparation = CodeNativeNormalizerPreparation1 {
        schema: "oev2.native-normalizer-preparation/1".into(),
        source_profile: session.side.source_profile.clone(),
        side_context: session.side_ref.clone(),
        build_driver: built.driver,
        normalizer: built.normalizer,
        tools: built.tools,
        source_manifest_sha256: digest(&built.source)?,
        process: reference(root, &evidence.join("build-normalizer/raw-stream11/process.json"))?,
        stream_capture: reference(root, &evidence.join("build-normalizer/stream-capture11.json"))?,
        full_build: "not_run".into(),
        release_eligibility: "unknown".into(),
        uncovered: vec![
            "Preparation observes one guarded compilation only; no full native capture".into(),
            "Actual generation, runner, drift, identity, both debug images, complete loader/OSO/signature and reproducibility remain unverified".into(),
            "Full Semantica behavior, selection, feedback, release and engineering adoption remain separately unestablished".into(),
        ],
    };
    verify_preparation(root, &preparation)?;
    verify_pins(root, &session.recipe)?;
    deadline_timeout(deadline)?;
    write_new(&evidence.join(FILE), &serde_json::to_value(&preparation)?)?;
    ensure!(
        read(root, &reference(root, &evidence.join(FILE))?)? == serde_json::to_value(&preparation)?,
        "normalizer_preparation_receipt_changed"
    );
    deadline_timeout(deadline)?;
    Ok(preparation)
}

/// Rechecks limited preparation content; registry authority remains with owner.
/// Never accepts this result as a CodeNativeBuildCapture11 or CodeSourceArtifact.
pub fn verify_preparation(
    root: &SkillRoot,
    value: &CodeNativeNormalizerPreparation1,
) -> Result<()> {
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("normalizer_preparation:{e:?}"))?;
    ensure!(
        value.schema == "oev2.native-normalizer-preparation/1"
            && value.full_build == "not_run"
            && value.release_eligibility == "unknown",
        "normalizer_preparation_scope"
    );
    let (side, _, recipe) =
        crate::native_profile6::load_compiler_entry(root, &value.side_context)?.into_parts();
    ensure!(
        side.source_profile == value.source_profile
            && side.build.source_root
                == root
                    .path()
                    .to_str()
                    .context("normalizer_preparation_root")?,
        "normalizer_preparation_side_binding"
    );
    ensure!(
        crate::native_link8::is_adapter8(&recipe)?,
        "compiler7_selected_adapter_required"
    );
    verify_pins(root, &recipe)?;
    let manifest = digest(&public_manifest(root.path())?)?;
    ensure!(
        manifest == value.source_manifest_sha256,
        "normalizer_preparation_source_changed"
    );
    let expected = [
        &side.build.cargo_path,
        &side.build.rustc_path,
        &side.build.ggen_path,
    ];
    ensure!(
        value.tools.len() == expected.len(),
        "normalizer_preparation_tools"
    );
    for (tool, path) in value.tools.iter().zip(expected) {
        ensure!(
            *tool == binary_reference(Path::new(path), root.path())?,
            "normalizer_preparation_tool_changed"
        );
    }
    ensure!(
        value.normalizer
            == binary_reference(
                &Path::new(&side.build.target_dir).join("debug/oev2-design-bootstrap"),
                root.path()
            )?,
        "normalizer_preparation_binary_changed"
    );
    verify_binary(&value.build_driver)?;
    let evidence = root.private_path(Path::new(&side.build.evidence_path))?;
    ensure!(
        value.process
            == reference(
                root,
                &evidence.join("build-normalizer/raw-stream11/process.json")
            )?
            && value.stream_capture
                == reference(
                    root,
                    &evidence.join("build-normalizer/stream-capture11.json")
                )?,
        "normalizer_preparation_stage_path"
    );
    let content = crate::native_stream11::inspect_capture_content11(root, &value.stream_capture)?;
    let plan: CodeNativeCargoStreamPlan11 =
        serde_json::from_value(read(root, &content.capture().plan)?)?;
    ensure!(
        plan.parent_stage == "build-normalizer"
            && plan.side_context == value.side_context
            && plan.source_manifest_sha256 == manifest,
        "normalizer_preparation_stream_binding"
    );
    let (_, configuration) = crate::native_runtime10::verify_cargo_linker_configuration_stage(
        root,
        &side,
        &recipe,
        "build-normalizer",
    )?;
    crate::native_runtime6::verify_normalizer_command11(
        root,
        &value.side_context,
        &side,
        &recipe,
        &configuration.environment,
    )?;
    crate::native_stream11::verify_native_stage11(
        root,
        &value.stream_capture,
        &value.process,
        &configuration.cargo,
        root.path(),
        &[
            "build",
            "--locked",
            "--offline",
            "-j4",
            "-p",
            "oe-design",
            "--bin",
            "oev2-design-bootstrap",
        ]
        .map(str::to_owned),
    )?;
    for path in [
        "actual-ggen",
        "build-runner",
        "actual-ggen-check",
        "runner-identity",
        "artifact.json",
        "native-stream11-capture.json",
    ] {
        ensure!(
            !evidence.join(path).try_exists()?,
            "normalizer_preparation_full_stage_present:{path}"
        );
    }
    content.verify_current()?;
    // Recheck mutable executable inputs after inspecting retained stage content.
    for tool in &value.tools {
        verify_binary(tool)?;
    }
    verify_binary(&value.normalizer)?;
    verify_binary(&value.build_driver)?;
    ensure!(
        digest(&public_manifest(root.path())?)? == manifest,
        "normalizer_preparation_source_changed"
    );
    Ok(())
}

pub struct NativeNormalizerPreparer;
impl CodeNativeNormalizerPreparation1Port for NativeNormalizerPreparer {
    fn prepare_normalizer(
        &self,
        input: &CodeNativeNormalizerPreparationRequest1,
    ) -> std::result::Result<CodeNativeNormalizerPreparation1, FailureRecord> {
        prepare_normalizer(input).map_err(|e| FailureRecord {
            code: ErrorCode::InvalidContract,
            phase: "native_normalizer_preparation".into(),
            retryable: false,
            partial_result: Some(json!({"execution":"failed", "reason":format!("{e:#}"),
                "full_build":"not_run", "release_eligibility":"unknown"})),
        })
    }
}
