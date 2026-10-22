//! Explicit full Cargo stream11 execution on the original five-stage owner.
//! Shared source/SDK8/probe10 checks do not inherit any prior execution success.
use crate::native_runtime::{STAGES, deadline_timeout, read, reference};
use crate::native_runtime6::{NativeBuildSession6, verify_pins};
use crate::native_runtime10::{SharedFullCaptureEvidence, verify_full_capture_evidence};
use crate::*;
use oe_core::create_new_private_dir;
pub const PROFILE: &str = "semantica.rust-full-native-cargo-stream/11";

fn invocation_paths11(root: &SkillRoot, stage_root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for stage in ["build-normalizer", "build-runner"] {
        let folder = root.private_path(&stage_root.join(stage))?;
        no_symlinks(&folder)?;
        let mut calls = std::fs::read_dir(&folder)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        calls.retain(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("call-"))
        });
        calls.sort();
        ensure!(
            !calls.is_empty() && calls.len() <= 4096,
            "stream11_invocation_inventory_empty_or_limit"
        );
        for (ordinal, call) in calls.iter().enumerate() {
            ensure!(
                call.file_name().and_then(|name| name.to_str())
                    == Some(format!("call-{ordinal:06}").as_str()),
                "stream11_invocation_inventory_gap"
            );
            no_symlinks(call)?;
            paths.push(call.join("invocation.json"));
        }
    }
    Ok(paths)
}

/// File-content binding only. The supplied expected records are produced by
/// the original complete native_link8 checker in the execution path below.
/// Each original record keeps the unchanged 16 MiB reader limit. Serialization
/// is one record at a time, without storing a second inline inventory.
fn bind_invocation_inventory_content11(
    root: &SkillRoot,
    stage_root: &Path,
    selected: Option<&[ArtifactReference]>,
    expected: impl ExactSizeIterator<Item = Result<(String, u64, Value)>>,
) -> Result<Vec<ArtifactReference>> {
    let paths = invocation_paths11(root, stage_root)?;
    ensure!(
        paths.len() == expected.len() && selected.is_none_or(|items| items.len() == paths.len()),
        "stream11_invocation_inventory_count_changed"
    );
    let mut references = Vec::with_capacity(paths.len());
    for (index, (path, expected)) in paths.iter().zip(expected).enumerate() {
        let (stage, ordinal, value) = expected?;
        ensure!(
            ["build-normalizer", "build-runner"].contains(&stage.as_str())
                && *path
                    == root.private_path(
                        &stage_root
                            .join(stage)
                            .join(format!("call-{ordinal:06}"))
                            .join("invocation.json")
                    )?,
            "stream11_invocation_original_order_changed"
        );
        let current = reference(root, path)?;
        ensure!(
            selected.is_none_or(|items| items[index] == current),
            "stream11_invocation_reference_changed"
        );
        ensure!(
            read(root, &current)? == value,
            "stream11_invocation_original_content_changed"
        );
        ensure!(
            reference(root, path)? == current,
            "stream11_invocation_changed_during_read"
        );
        references.push(current);
    }
    ensure!(
        invocation_paths11(root, stage_root)? == paths,
        "stream11_invocation_inventory_changed_after_read"
    );
    for current in &references {
        // Rechecks the original per-file bound and current content digest.
        read(root, current)?;
    }
    Ok(references)
}

fn invocation_references11(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    actual: &[CodeNativeInvocationEnvelope8],
    selected: Option<&[ArtifactReference]>,
) -> Result<Vec<ArtifactReference>> {
    bind_invocation_inventory_content11(
        root,
        Path::new(&side.stage_root),
        selected,
        actual.iter().map(|envelope| {
            crate::native_link8::inspect_envelope(envelope)?;
            let record = if let Some(record) = &envelope.archive3 {
                (
                    record.parent_stage.clone(),
                    record.ordinal,
                    serde_json::to_value(record)?,
                )
            } else if let Some(record) = &envelope.compiler7 {
                (
                    record.parent_stage.clone(),
                    record.ordinal,
                    serde_json::to_value(record)?,
                )
            } else {
                let record = envelope
                    .sdk8
                    .as_ref()
                    .context("stream11_invocation_actual_record_missing")?;
                (
                    record.parent_stage.clone(),
                    record.ordinal,
                    serde_json::to_value(record)?,
                )
            };
            Ok(record)
        }),
    )
}

pub(crate) fn cargo_stream_references(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
) -> Result<Vec<ArtifactReference>> {
    ["build-normalizer", "build-runner"]
        .into_iter()
        .map(|stage| {
            let selected = reference(
                root,
                &Path::new(&side.build.evidence_path)
                    .join(stage)
                    .join("stream-capture11.json"),
            )?;
            crate::native_stream11::inspect_capture_content11(root, &selected)?;
            Ok(selected)
        })
        .collect()
}
pub(crate) fn native_build_facts(
    root: &SkillRoot,
    side_ref: &ArtifactReference,
    side: &CodeNativeSideContext6,
    profile: &CodeNativeRuntimeProfile6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Value> {
    let mut facts =
        crate::native_runtime10::native_build_facts(root, side_ref, side, profile, recipe)?;
    facts["schema"] = json!("oev2.native-build-facts/11");
    facts["execution_profile"] = json!(PROFILE);
    facts["cargo_stream_resources"] =
        serde_json::to_value(crate::native_stream11::selected_resources11())?;
    facts["cargo_stream_captures"] = serde_json::to_value(cargo_stream_references(root, side)?)?;
    Ok(facts)
}
pub(crate) fn bind_artifact(
    session: &NativeBuildSession6,
    a: &mut CodeSourceArtifact,
) -> Result<()> {
    deadline_timeout(session.deadline)?;
    verify_pins(&session.root, &session.recipe)?;
    ensure!(
        crate::native_link8::is_adapter8(&session.recipe)?
            && a.compiled["rustflags_sha256"] == session.recipe.raw_encoded_rustflags_sha256,
        "compiler7_adapter_or_compiled_flags_changed"
    );
    a.profile = PROFILE.into();
    a.generation["native_build"] = native_build_facts(
        &session.root,
        &session.side_ref,
        &session.side,
        &session.profile,
        &session.recipe,
    )?;
    a.uncovered.extend([
        "Current11 native execution uses explicitly selected Cargo raw/preview resources; original10 output_limit failures remain failures".into(),
        "Whole Semantica/platform compatibility, engineering adoption and release require their own evidence".into(),
    ]);
    Ok(())
}
pub fn build_full_native_budgeted(
    input: &CodeNativeBudgetedBuildRequest11,
) -> Result<CodeNativeBuildCapture11> {
    let started = Instant::now();
    crate::native_stream11::verify_resources11(&input.resources)?;
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("native6_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.native-budgeted-build-request/11"
            && (1..=3_600_000).contains(&input.budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&input.budget.output_bytes),
        "native6_request_schema_or_bounds"
    );
    let deadline = started
        .checked_add(Duration::from_millis(input.budget.timeout_ms))
        .context("native6_deadline_overflow")?;
    let session =
        NativeBuildSession6::new_parts(&input.build, &input.budget, &input.side_context, deadline)?;
    ensure!(
        crate::native_link8::is_adapter8(&session.recipe)?,
        "compiler7_selected_adapter_required"
    );
    let artifact = crate::build_with_deadline_mode(
        &input.build,
        Some(deadline),
        session.output_limit,
        crate::BuildMode::Native11(&session),
    )?;
    let root = &session.root;
    let evidence = root.private_path(Path::new(&input.build.evidence_path))?;
    let artifact_ref = reference(root, &evidence.join("artifact.json"))?;
    let invocations = crate::native_link8::load_invocations(
        root,
        &session.side_ref,
        &session.side,
        &session.recipe,
    )?;
    ensure!(
        invocations
            .iter()
            .any(|x| crate::native_link8::parent_stage(x).ok() == Some("build-normalizer"))
            && invocations
                .iter()
                .any(|x| crate::native_link8::parent_stage(x).ok() == Some("build-runner")),
        "native6_stage_inventory_empty"
    );
    let loader_receipts =
        crate::native_loader11::prepare_retained(root, &session.side, &session.recipe)?;
    let images = crate::native_profile6::load_debug_images(root, &session.side.source_profile)?;
    create_new_private_dir(&evidence.join("native-debug"))?;
    let mut debug_images = Vec::new();
    for image in &images.images {
        let observation = crate::native_debug::observe_sdk8(
            &session,
            &artifact_ref,
            &artifact,
            &invocations,
            image,
            &evidence.join("native-debug").join(&image.image_role),
        )?;
        let p = evidence.join(format!(
            "native-debug-{}-observation.json",
            image.image_role
        ));
        write_new(&p, &serde_json::to_value(&observation)?)?;
        debug_images.push(reference(root, &p)?);
    }
    let refs = STAGES
        .iter()
        .map(|s| {
            reference(
                root,
                &evidence
                    .join(s)
                    .join(if ["build-normalizer", "build-runner"].contains(s) {
                        "raw-stream11/process.json"
                    } else {
                        "process.json"
                    }),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let own = &crate::driver_root().context("native6_driver_root")?;
    let probes = crate::native_probe_exec10::retained10(root, &session.side, &session.recipe)?;
    let capture = CodeNativeBuildCapture11 {
        resources: input.resources.clone(),
        cargo_stream_captures: cargo_stream_references(root, &session.side)?,
        compiler_probe_policies: probes.policies,
        compiler_probe_intents: probes.intents,
        compiler_probe_completions: probes.completions,
        schema: "oev2.native-build-capture/11".into(),
        source_profile: session.side.source_profile.clone(),
        artifact: artifact_ref.clone(),
        driver: crate::binary_reference(&std::env::current_exe()?, own)?,
        normalizer_process: refs[0].clone(),
        generation_process: refs[1].clone(),
        runner_build_process: refs[2].clone(),
        drift_process: refs[3].clone(),
        identity_process: refs[4].clone(),
        side_context: input.side_context.clone(),
        build_script_metadata_bindings: crate::native_buildscript6::retained_sdk8(
            root,
            &session.side,
            &session.recipe,
        )?,
        process_environment_bindings: crate::native_link8::process_bindings(root, &invocations)?,
        invocations: invocation_references11(root, &session.side, &invocations, None)?,
        debug_images,
        loader_receipts,
    };
    verify_full_native_capture(
        root,
        &session.side.source_profile,
        &input.side_context,
        &artifact_ref,
        &capture,
    )?;
    deadline_timeout(deadline)?;
    write_new(
        &evidence.join("native-stream11-capture.json"),
        &serde_json::to_value(&capture)?,
    )?;
    Ok(capture)
}
pub fn verify_full_native_capture(
    root: &SkillRoot,
    source_profile: &ArtifactReference,
    side_ref: &ArtifactReference,
    artifact_ref: &ArtifactReference,
    c: &CodeNativeBuildCapture11,
) -> Result<()> {
    crate::native_stream11::verify_resources11(&c.resources)?;
    c.validate()
        .map_err(|e| anyhow::anyhow!("stream11_capture:{e:?}"))?;
    ensure!(
        c.schema == "oev2.native-build-capture/11"
            && c.source_profile == *source_profile
            && c.side_context == *side_ref
            && c.artifact == *artifact_ref,
        "stream11_capture_binding_changed"
    );
    let side = crate::native_profile6::load_side(root, side_ref)?;
    crate::native_profile6::inspect_profile(root, source_profile)?;
    let recipe = crate::native_profile6::load_recipe(root, source_profile)?;
    ensure!(
        c.cargo_stream_captures == cargo_stream_references(root, &side)?,
        "stream11_capture_stage_inventory_changed"
    );
    ensure!(
        c.loader_receipts == crate::native_loader11::verify_retained(root, &side, &recipe)?,
        "stream11_loader_receipts_changed"
    );
    // References never substitute for original semantic/native checks. Load
    // the entire actual mixed inventory through its unchanged original owner.
    let actual = crate::native_link8::load_invocations(root, side_ref, &side, &recipe)?;
    invocation_references11(root, &side, &actual, Some(&c.invocations))?;
    verify_full_capture_evidence(
        root,
        source_profile,
        side_ref,
        artifact_ref,
        SharedFullCaptureEvidence {
            driver: &c.driver,
            processes: [
                &c.normalizer_process,
                &c.generation_process,
                &c.runner_build_process,
                &c.drift_process,
                &c.identity_process,
            ],
            invocations: &actual,
            debug_images: &c.debug_images,
            build_script_metadata_bindings: &c.build_script_metadata_bindings,
            process_environment_bindings: &c.process_environment_bindings,
            compiler_probe_policies: &c.compiler_probe_policies,
            compiler_probe_intents: &c.compiler_probe_intents,
            compiler_probe_completions: &c.compiler_probe_completions,
        },
        PROFILE,
        Some(&c.cargo_stream_captures),
    )?;
    // The shared verifier rechecks every full record and all image guards;
    // finish by rebinding the unchanged complete ordered retained inventory.
    invocation_references11(root, &side, &actual, Some(&c.invocations))?;
    Ok(())
}

pub struct NativeRuntimeBuilder11;
impl CodeNativeBudgetedBuild11Port for NativeRuntimeBuilder11 {
    fn build_full_native_budgeted_runtime(
        &self,
        input: &CodeNativeBudgetedBuildRequest11,
    ) -> std::result::Result<CodeNativeBuildCapture11, FailureRecord> {
        build_full_native_budgeted(input).map_err(|e| FailureRecord {
            code: ErrorCode::InvalidContract,
            phase: "native_full_stream11_budgeted_build".into(),
            retryable: false,
            partial_result: Some(json!({"execution":"failed","reason":format!("{e:#}"),
                "compilation_origin":"unknown","release_eligibility":"unknown"})),
        })
    }
}

#[cfg(test)]
mod invocation_content_controls {
    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        root: SkillRoot,
        stage_root: PathBuf,
        records: Vec<(String, u64, Value)>,
    }
    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().canonicalize().unwrap();
            std::fs::write(
                path.join("SKILL.md"),
                b"invocation file content control only",
            )
            .unwrap();
            let root = SkillRoot::new(&path).unwrap();
            let stage_root = path.join("var/stages");
            let mut records = Vec::new();
            for stage in ["build-normalizer", "build-runner"] {
                let folder = root
                    .new_private_dir(&stage_root.join(stage).join("call-000000"))
                    .unwrap();
                // Intentionally not a valid native invocation: this only tests
                // exact inventory/content handling, never native acceptance.
                let value = json!({"schema":"content-control-only", "original_arguments":[stage,"-lc","-lc"],
                    "original_unknown":["no_native_authority"]});
                write_new(&folder.join("invocation.json"), &value).unwrap();
                records.push((stage.into(), 0, value));
            }
            Self {
                _directory: directory,
                root,
                stage_root,
                records,
            }
        }
        fn bind(&self, selected: Option<&[ArtifactReference]>) -> Result<Vec<ArtifactReference>> {
            bind_invocation_inventory_content11(
                &self.root,
                &self.stage_root,
                selected,
                self.records.iter().cloned().map(Ok),
            )
        }
    }

    #[test]
    fn exact_original_invocation_files_reject_missing_extra_reordered_and_changed_references() {
        let fixture = Fixture::new();
        let original = fixture.bind(None).unwrap();
        assert_eq!(fixture.bind(Some(&original)).unwrap(), original);
        assert!(fixture.bind(Some(&original[..1])).is_err());
        let mut extra = original.clone();
        extra.push(original[0].clone());
        assert!(fixture.bind(Some(&extra)).is_err());
        let mut reversed = original.clone();
        reversed.reverse();
        assert!(fixture.bind(Some(&reversed)).is_err());
        let mut digest = original.clone();
        digest[0].sha256 = "0".repeat(64);
        assert!(fixture.bind(Some(&digest)).is_err());
        let mut changed = fixture.records[0].2.clone();
        changed["original_arguments"][1] = json!("-lchanged");
        std::fs::write(&original[0].path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(fixture.bind(Some(&original)).is_err());
        // Refreshing the hash still cannot replace original record content.
        assert!(
            fixture
                .bind(None)
                .unwrap_err()
                .to_string()
                .contains("original_content_changed")
        );
    }

    #[test]
    fn exact_original_invocation_inventory_rejects_added_removed_or_relabelled_calls() {
        let fixture = Fixture::new();
        let original = fixture.bind(None).unwrap();
        let extra = fixture.stage_root.join("build-normalizer/call-000001");
        fixture.root.new_private_dir(&extra).unwrap();
        write_new(&extra.join("invocation.json"), &fixture.records[0].2).unwrap();
        assert!(
            fixture
                .bind(Some(&original))
                .unwrap_err()
                .to_string()
                .contains("count_changed")
        );
        let fixture = Fixture::new();
        let original = fixture.bind(None).unwrap();
        std::fs::remove_file(&original[0].path).unwrap();
        assert!(fixture.bind(Some(&original)).is_err());
        let fixture = Fixture::new();
        let original = fixture.bind(None).unwrap();
        let stage = fixture.stage_root.join("build-runner");
        std::fs::rename(stage.join("call-000000"), stage.join("call-000002")).unwrap();
        assert!(
            fixture
                .bind(Some(&original))
                .unwrap_err()
                .to_string()
                .contains("inventory_gap")
        );
        let fixture = Fixture::new();
        let original = fixture.bind(None).unwrap();
        std::fs::write(&original[0].path, vec![b' '; 16 * 1024 * 1024 + 1]).unwrap();
        assert!(fixture.bind(None).is_err());
    }
}
