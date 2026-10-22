//! Pre-bound real builds composed by the existing private version authority.
//! Observing an approved build establishes local compilation provenance only.
//! Neither a caller capture nor a successful build grants release eligibility.
use crate::{artifacts, registry};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CodeBudget, CodeBudgetedBuildRequest, CodeBuildSupervisionContext,
    CodeBuildSupervisionRequest, CodeBuildSupervisionResult, CodeRustRuntimeBuildCapture,
    CodeSourceArtifact,
};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{AdvisoryFileLock, SkillRoot, write_new};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);
const CLEANUP_RESERVE: Duration = Duration::from_millis(250);
fn body_remaining(deadline: Instant) -> Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let body = remaining
        .checked_sub(CLEANUP_RESERVE)
        .context("owner_build_total_budget_exhausted")?;
    ensure!(
        body >= Duration::from_millis(1),
        "owner_build_total_budget_exhausted"
    );
    Ok(body)
}

fn preflight(root: &SkillRoot, context: &CodeBuildSupervisionContext) -> Result<Value> {
    context
        .build
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_build_request:{e:?}"))?;
    ensure!(
        context.dispatch_budget.timeout_ms > 0
            && context.dispatch_budget.timeout_ms <= 3_600_000
            && context.dispatch_budget.output_bytes > 0
            && context.dispatch_budget.output_bytes <= 4 * 1024 * 1024,
        "owner_build_dispatch_budget"
    );
    // This first explicit owner profile operates on its own software root. It
    // does not copy a Project or infer permissions for a different source tree.
    ensure!(
        Path::new(&context.build.source_root) == root.path()
            && context.builder.source_root == context.build.source_root,
        "owner_build_source_root_not_bound"
    );
    root.private_path(Path::new(&context.source_profile.path))?;
    let (profile, manifest) =
        oe_evolution::runtime_identity::inspect_profile(root, &context.source_profile)?;
    ensure!(
        profile.expected_build_capture.is_none()
            && profile.source_kind == "frozen_export"
            && profile.expected_target == context.build.platform,
        "owner_build_profile_not_prior_source_conditions"
    );
    // Executable pins are approved by the already-owned policy before dispatch,
    // rather than discovered from a caller's later artifact declaration.
    oe_evolution::verify_binary(&context.builder)?;
    ensure!(
        context.builder.source_manifest_sha256 == oe_evolution::digest(&manifest)?,
        "owner_build_driver_source_not_bound"
    );
    let paths = [
        &context.build.cargo_path,
        &context.build.rustc_path,
        &context.build.ggen_path,
    ];
    ensure!(
        context.expected_tools.len() == paths.len(),
        "owner_build_tool_set"
    );
    for (pin, path) in context.expected_tools.iter().zip(paths) {
        ensure!(
            &pin.path == path
                && pin.source_root == context.build.source_root
                && pin.source_manifest_sha256 == context.builder.source_manifest_sha256,
            "owner_build_tool_binding_changed"
        );
        oe_evolution::verify_binary(pin)?;
    }
    let target = root.private_path(Path::new(&context.build.target_dir))?;
    let evidence = root.private_path(Path::new(&context.build.evidence_path))?;
    ensure!(
        !target.starts_with(&evidence) && !evidence.starts_with(&target),
        "owner_build_paths_overlap"
    );
    ensure!(!evidence.try_exists()?, "owner_build_evidence_not_fresh");
    Ok(manifest)
}

fn still_bound(
    root: &SkillRoot,
    request: &CodeBuildSupervisionRequest,
    original: &(CodeBuildSupervisionContext, String, ArtifactReference),
    manifest: &Value,
) -> Result<()> {
    ensure!(
        registry::build_context(root, request)? == *original,
        "owner_build_policy_or_context_changed"
    );
    let (profile, current) =
        oe_evolution::runtime_identity::inspect_profile(root, &original.0.source_profile)?;
    ensure!(
        profile.expected_build_capture.is_none() && &current == manifest,
        "owner_build_source_changed"
    );
    oe_evolution::verify_binary(&original.0.builder)?;
    for pin in &original.0.expected_tools {
        oe_evolution::verify_binary(pin)?;
    }
    Ok(())
}

fn capture(
    root: &SkillRoot,
    context: &CodeBuildSupervisionContext,
    artifact: &CodeSourceArtifact,
    artifact_reference: &ArtifactReference,
    folder: &Path,
) -> Result<ArtifactReference> {
    ensure!(
        artifact.artifact_id == context.build.artifact_id
            && artifact.source_root == context.build.source_root
            && artifact.platform == context.build.platform
            && artifact.generation["fixed_build_driver"]["executable_sha256"]
                == context.builder.sha256
            && artifact.generation["fixed_build_driver"]["source_manifest_sha256"]
                == context.builder.source_manifest_sha256,
        "owner_build_output_binding_changed"
    );
    ensure!(
        artifact.tools.len() == 4 && artifact.tools[..3] == context.expected_tools,
        "owner_build_output_tools_changed"
    );
    let runner_path = root
        .private_path(Path::new(&context.build.target_dir))?
        .join(&context.build.platform)
        .join("debug/oe-code-runner");
    ensure!(
        Path::new(&artifact.runner.path) == runner_path,
        "owner_build_runner_path_changed"
    );
    let evidence = root.private_path(Path::new(&context.build.evidence_path))?;
    let stage = |name: &str| artifacts::reference(root, &evidence.join(name).join("process.json"));
    let data = CodeRustRuntimeBuildCapture {
        schema: "oev2.rust-runtime-build-capture/1".into(),
        source_profile: context.source_profile.clone(),
        artifact: artifact_reference.clone(),
        driver: context.builder.clone(),
        normalizer_process: stage("build-normalizer")?,
        generation_process: stage("actual-ggen")?,
        runner_build_process: stage("build-runner")?,
        drift_process: stage("actual-ggen-check")?,
        identity_process: stage("runner-identity")?,
    };
    let path = folder.join("build-capture.json");
    write_new(&path, &serde_json::to_value(data)?)?;
    let reference = artifacts::reference(root, &path)?;
    let (profile, _) =
        oe_evolution::runtime_identity::inspect_profile(root, &context.source_profile)?;
    ensure!(
        oe_evolution::runtime_identity::verify_capture(
            root,
            &reference,
            &profile,
            artifact_reference
        )? == *artifact,
        "owner_build_capture_output_changed"
    );
    Ok(reference)
}

pub fn supervise(
    root: &SkillRoot,
    request: &CodeBuildSupervisionRequest,
) -> Result<CodeBuildSupervisionResult> {
    let owner_started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_build_supervision_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.code-build-supervision-request/1",
        "owner_build_supervision_schema"
    );
    let prior = registry::build_context(root, request)?;
    let manifest = preflight(root, &prior.0)?;
    let total_deadline = if prior.0.schema == "oev2.code-build-supervision-context/2" {
        let end = owner_started
            .checked_add(Duration::from_millis(prior.0.dispatch_budget.timeout_ms))
            .context("owner_build_total_deadline")?;
        body_remaining(end)?;
        Some(end)
    } else {
        None
    };
    let owner = registry::owner_folder(root, &request.binding_id)?;
    oe_core::ensure_directory(&owner)?;
    // Share the existing owner's selection lock; no competing version ledger.
    let lock = AdvisoryFileLock::acquire(&owner.join("selection.lock"), true)?;
    still_bound(root, request, &prior, &manifest)?;
    let serial = SERIAL.fetch_add(1, Ordering::SeqCst);
    let relative = format!(
        "{}/build-observations/{}-{}-{serial}",
        owner
            .strip_prefix(root.path())?
            .to_str()
            .context("owner_build_path")?,
        &prior.2.sha256[..16],
        std::process::id()
    );
    let folder = root.new_private_dir(Path::new(&relative))?;
    let build_file = folder.join("build-request.json");
    let command = if let Some(deadline) = total_deadline {
        let body = body_remaining(deadline)?;
        let bounded = CodeBudgetedBuildRequest {
            schema: "oev2.code-budgeted-build-request/1".into(),
            build: prior.0.build.clone(),
            budget: CodeBudget {
                timeout_ms: u64::try_from(body.as_millis())?,
                output_bytes: prior.0.dispatch_budget.output_bytes,
            },
        };
        bounded
            .validate()
            .map_err(|e| anyhow::anyhow!("owner_budgeted_build_request:{e:?}"))?;
        write_new(&build_file, &serde_json::to_value(bounded)?)?;
        "build-budgeted"
    } else {
        write_new(&build_file, &serde_json::to_value(&prior.0.build)?)?;
        "build"
    };
    let dispatch_timeout = if let Some(deadline) = total_deadline {
        // Keep the inner 250ms cleanup reserve inside this same owner deadline.
        // Subtracting it again here would terminate the driver before its child
        // can flush the original partial stage capture on its own deadline.
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(
            remaining >= Duration::from_millis(1),
            "owner_build_total_budget_exhausted"
        );
        remaining
    } else {
        Duration::from_millis(prior.0.dispatch_budget.timeout_ms)
    };
    let process = run_command(
        &CommandSpec {
            executable: prior.0.builder.path.clone().into(),
            arguments: vec![command.into(), build_file.into_os_string()],
            cwd: root.path().into(),
            environment: BTreeMap::new(),
            remove_environment: vec![
                "RUSTFLAGS".into(),
                "CARGO_ENCODED_RUSTFLAGS".into(),
                "RUSTC_WRAPPER".into(),
                "RUSTC_WORKSPACE_WRAPPER".into(),
            ],
            timeout: dispatch_timeout,
            output_limit: usize::try_from(prior.0.dispatch_budget.output_bytes)?,
        },
        &folder.join("builder-dispatch"),
    )?;
    let process_reference =
        artifacts::reference(root, &folder.join("builder-dispatch/process.json"))?;
    let mut artifact_reference = None;
    let mut capture_reference = None;
    let evaluated = (|| -> Result<()> {
        ensure!(
            process.status == "completed" && process.exit_code == Some(0),
            "owner_build_process_not_completed"
        );
        let artifact: CodeSourceArtifact =
            serde_json::from_value(oe_core::json::strict_json(&process.stdout)?)?;
        ensure!(
            artifact.manifest == manifest,
            "owner_build_output_source_changed"
        );
        oe_evolution::verify_artifact(&artifact)?;
        let from_builder = artifacts::reference(
            root,
            &root
                .private_path(Path::new(&prior.0.build.evidence_path))?
                .join("artifact.json"),
        )?;
        ensure!(
            artifacts::json(root, &from_builder)? == serde_json::to_value(&artifact)?,
            "owner_build_stdout_artifact_changed"
        );
        let selected_capture = capture(root, &prior.0, &artifact, &from_builder, &folder)?;
        still_bound(root, request, &prior, &manifest)?;
        lock.ensure_current()?;
        if let Some(deadline) = total_deadline {
            ensure!(
                Instant::now() < deadline,
                "owner_build_total_budget_exhausted"
            );
        }
        artifact_reference = Some(from_builder);
        capture_reference = Some(selected_capture);
        Ok(())
    })();
    let completed =
        evaluated.is_ok() && total_deadline.is_none_or(|deadline| Instant::now() < deadline);
    let reason = evaluated
        .err()
        .map(|e| format!("{e:#}"))
        .or_else(|| (!completed).then(|| "owner_build_total_budget_exhausted".into()));
    if !completed {
        artifact_reference = None;
        capture_reference = None;
    }
    // Keep this evidence under the same binding. It is not a selection event
    // and therefore cannot turn the existing Unselected state into Active.
    let observation_file = folder.join("supervision.json");
    write_new(
        &observation_file,
        &json!({
            "schema":"oev2.code-build-supervision-observation/1", "request":request,
            "policy_sha256":prior.1, "context":prior.2, "process":process_reference,
            "artifact":artifact_reference, "capture":capture_reference,
            "execution":if completed {"completed"} else {"failed"},
            "compilation_origin":if completed {"owner_observed"} else {"unknown"},
            "reason":reason, "release_eligibility":"unknown", "selection":"not_run",
            "budget_observation":{"profile":if total_deadline.is_some(){"owner_supervise_total/2"}else{"dispatch_only/1"},
                "entry":"supervise function entry; CLI request loading and compilation guard excluded", "timeout_ms":prior.0.dispatch_budget.timeout_ms,
                "elapsed_ms":owner_started.elapsed().as_millis(),"cleanup_reserve_ms":if total_deadline.is_some(){250}else{0},
                "per_stream_output_bytes":prior.0.dispatch_budget.output_bytes,
                "limit":"local monotonic cancellation and verification boundary; blocking I/O/final persistence is not certified hard realtime or total resource cost"},
            "engineering_adoption":"not_established", "publication":"not_performed"
        }),
    )?;
    let result = CodeBuildSupervisionResult {
        schema: "oev2.code-build-supervision-result/1".into(),
        binding_id: request.binding_id.clone(), revision: request.revision.clone(), scope_id: request.scope_id.clone(),
        execution: if completed { "completed" } else { "failed" }.into(),
        compilation_origin: if completed { "owner_observed" } else { "unknown" }.into(),
        artifact: artifact_reference, capture: capture_reference,
        observation: artifacts::reference(root, &observation_file)?,
        release_eligibility: "unknown".into(),
        uncovered: vec!["Local owned policy supervision; caller JSON grants no authority and no hostile-code sandbox is established".into(),
            "One real debug build is not reproducible-build, full13494 behavior, platform or 19 frozen protocol acceptance".into(),
            "Whole-code activation/use/Project feedback/recovery and engineering adoption remain separate".into()],
    };
    result
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_build_result_contract:{e:?}"))?;
    Ok(result)
}

/// Private dispatch choice, never a public protocol projection or another owner.
#[derive(Clone, Copy, PartialEq)]
enum NativeVersion {
    Direct2,
    Full3,
    Loader6,
    Compiler7,
    Sdk8,
    Probe9,
    Probe10,
    Stream11,
}
struct NativeProfileRefs {
    baseline: ArtifactReference,
    recipe: ArtifactReference,
}
impl NativeVersion {
    fn profile(self, root: &SkillRoot, reference: &ArtifactReference) -> Result<NativeProfileRefs> {
        match self {
            Self::Direct2 => {
                let p = oe_evolution::native_profile::inspect_profile(root, reference)?;
                Ok(NativeProfileRefs {
                    baseline: p.baseline_profile,
                    recipe: p.recipe,
                })
            }
            Self::Full3 => {
                let p = oe_evolution::native_profile3::inspect_profile(root, reference)?;
                oe_evolution::native_profile3::load_recipe(root, reference)?;
                let images = oe_evolution::native_profile3::load_debug_images(root, reference)?;
                ensure!(
                    images.images.len() == 2
                        && images
                            .images
                            .iter()
                            .any(|x| x.image_role == "normalizer" && x.package == "oe-design")
                        && images
                            .images
                            .iter()
                            .any(|x| x.image_role == "runner" && x.package == "oe-code-runner"),
                    "owner_full_native_debug_image_packages"
                );
                Ok(NativeProfileRefs {
                    baseline: p.baseline_profile,
                    recipe: p.recipe,
                })
            }
            Self::Loader6
            | Self::Compiler7
            | Self::Sdk8
            | Self::Probe9
            | Self::Probe10
            | Self::Stream11 => {
                let p = oe_evolution::native_profile6::inspect_profile(root, reference)?;
                oe_evolution::native_profile6::load_recipe(root, reference)?;
                let images = oe_evolution::native_profile6::load_debug_images(root, reference)?;
                ensure!(
                    images.images.len() == 2
                        && images
                            .images
                            .iter()
                            .any(|x| x.image_role == "normalizer" && x.package == "oe-design")
                        && images
                            .images
                            .iter()
                            .any(|x| x.image_role == "runner" && x.package == "oe-code-runner"),
                    "owner_full_native_debug_image_packages"
                );
                Ok(NativeProfileRefs {
                    baseline: p.baseline_profile,
                    recipe: p.recipe,
                })
            }
        }
    }
    fn command(self) -> &'static str {
        match self {
            Self::Direct2 => "build-native-budgeted",
            Self::Full3 => "build-full-native-budgeted",
            Self::Loader6 => "build-loader6-native-budgeted",
            Self::Compiler7 => "build-compiler7-native-budgeted",
            Self::Sdk8 => "build-sdk8-native-budgeted",
            Self::Probe9 => "build-probe9-native-budgeted",
            Self::Probe10 => "build-probe10-native-budgeted",
            Self::Stream11 => "build-stream11-native-budgeted",
        }
    }
    fn capture_file(self) -> &'static str {
        match self {
            Self::Direct2 => "native-capture.json",
            Self::Full3 => "native-full-capture.json",
            Self::Loader6 => "native-loader6-capture.json",
            Self::Compiler7 => "native-compiler7-capture.json",
            Self::Sdk8 => "native-sdk8-capture.json",
            Self::Probe9 => "native-probe9-capture.json",
            Self::Probe10 => "native-probe10-capture.json",
            Self::Stream11 => "native-stream11-capture.json",
        }
    }
    fn observation_schema(self) -> &'static str {
        match self {
            Self::Direct2 => "oev2.native-build-supervision-observation/2",
            Self::Full3 => "oev2.native-build-supervision-observation/3",
            Self::Loader6 => "oev2.native-build-supervision-observation/6",
            Self::Compiler7 => "oev2.native-build-supervision-observation/7",
            Self::Sdk8 => "oev2.native-build-supervision-observation/8",
            Self::Probe9 => "oev2.native-build-supervision-observation/9",
            Self::Probe10 => "oev2.native-build-supervision-observation/10",
            Self::Stream11 => "oev2.native-build-supervision-observation/11",
        }
    }
    fn entry(self) -> &'static str {
        match self {
            Self::Direct2 => {
                "supervise_native function entry; CLI loading and compilation guard excluded"
            }
            Self::Full3 => {
                "supervise_full_native function entry; CLI loading and compilation guard excluded"
            }
            Self::Loader6
            | Self::Compiler7
            | Self::Sdk8
            | Self::Probe9
            | Self::Probe10
            | Self::Stream11 => {
                "supervise_full_native function entry; CLI loading and compilation guard excluded"
            }
        }
    }
    fn side(
        self,
        root: &SkillRoot,
        prior: &(CodeBuildSupervisionContext, String, ArtifactReference),
        recipe_sha256: String,
        side_id: String,
        stage_root: String,
        budget: CodeBudget,
    ) -> Result<NativeOwnedSide> {
        match self {
            Self::Direct2 => {
                let side = oe_contracts::CodeNativeSideContext {
                    schema: "oev2.native-side-context/2".into(),
                    source_profile: prior.0.source_profile.clone(),
                    recipe_sha256,
                    owner_context: prior.2.clone(),
                    side_id,
                    build: prior.0.build.clone(),
                    stage_root,
                    body_budget: budget,
                    logical_debug_root: "oe-native-debug".into(),
                };
                oe_evolution::native_profile::inspect_side(root, &prior.0.source_profile, &side)?;
                Ok(NativeOwnedSide::Direct2(side))
            }
            Self::Full3 => {
                let side = oe_contracts::CodeNativeSideContext3 {
                    schema: "oev2.native-side-context/3".into(),
                    source_profile: prior.0.source_profile.clone(),
                    recipe_sha256,
                    owner_context: prior.2.clone(),
                    side_id,
                    build: prior.0.build.clone(),
                    stage_root,
                    body_budget: budget,
                    logical_debug_root: "oe-native-debug".into(),
                };
                oe_evolution::native_profile3::inspect_side(root, &prior.0.source_profile, &side)?;
                Ok(NativeOwnedSide::Full3(side))
            }
            Self::Loader6
            | Self::Compiler7
            | Self::Sdk8
            | Self::Probe9
            | Self::Probe10
            | Self::Stream11 => {
                let side = oe_contracts::CodeNativeSideContext6 {
                    schema: "oev2.native-side-context/6".into(),
                    source_profile: prior.0.source_profile.clone(),
                    recipe_sha256,
                    owner_context: prior.2.clone(),
                    side_id,
                    build: prior.0.build.clone(),
                    stage_root,
                    body_budget: budget,
                    logical_debug_root: "oe-native-debug".into(),
                };
                oe_evolution::native_profile6::inspect_side(root, &prior.0.source_profile, &side)?;
                Ok(NativeOwnedSide::Loader6(side))
            }
        }
    }
    fn bounded(
        self,
        build: oe_contracts::CodeBuildRequest,
        budget: CodeBudget,
        side_context: ArtifactReference,
    ) -> Result<Value> {
        match self {
            Self::Direct2 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest {
                    schema: "oev2.native-budgeted-build-request/2".into(),
                    build,
                    budget,
                    side_context,
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_native_budgeted_request:{e:?}"))?;
                Ok(serde_json::to_value(r)?)
            }
            Self::Full3 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest3 {
                    schema: "oev2.native-budgeted-build-request/3".into(),
                    build,
                    budget,
                    side_context,
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_full_native_budgeted_request:{e:?}"))?;
                Ok(serde_json::to_value(r)?)
            }
            Self::Loader6 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest6 {
                    schema: "oev2.native-budgeted-build-request/6".into(),
                    build,
                    budget,
                    side_context,
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_full_native_budgeted_request:{e:?}"))?;
                Ok(serde_json::to_value(r)?)
            }
            Self::Compiler7 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest7 {
                    schema: "oev2.native-budgeted-build-request/7".into(),
                    build,
                    budget,
                    side_context,
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_compiler7_budgeted_request:{e:?}"))?;
                Ok(serde_json::to_value(r)?)
            }
            Self::Sdk8 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest8 {
                    schema: "oev2.native-budgeted-build-request/8".into(),
                    build,
                    budget,
                    side_context,
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_sdk8_budgeted_request:{e:?}"))?;
                Ok(serde_json::to_value(r)?)
            }
            Self::Probe9 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest9 {
                    schema: "oev2.native-budgeted-build-request/9".into(),
                    build,
                    budget,
                    side_context,
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_probe9_budgeted_request:{e:?}"))?;
                Ok(serde_json::to_value(r)?)
            }
            Self::Probe10 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest10 {
                    schema: "oev2.native-budgeted-build-request/10".into(),
                    build,
                    budget,
                    side_context,
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_probe10_budgeted_request:{e:?}"))?;
                Ok(serde_json::to_value(r)?)
            }
            Self::Stream11 => {
                let r = oe_contracts::CodeNativeBudgetedBuildRequest11 {
                    schema: "oev2.native-budgeted-build-request/11".into(),
                    build,
                    budget,
                    side_context,
                    resources: oe_evolution::native_stream11::selected_resources11(),
                };
                r.validate()
                    .map_err(|e| anyhow::anyhow!("owner_stream11_budgeted_request:{e:?}"))?;
                oe_evolution::native_stream11::verify_resources11(&r.resources)?;
                Ok(serde_json::to_value(r)?)
            }
        }
    }
    fn capture(
        self,
        root: &SkillRoot,
        evidence: &Path,
        stdout: &[u8],
    ) -> Result<NativeOwnedCapture> {
        let value = if self == Self::Stream11 {
            stream11_dispatch_content(root, evidence, stdout)?.1
        } else {
            oe_core::json::strict_json(stdout)?
        };
        Ok(match self {
            Self::Direct2 => NativeOwnedCapture::Direct2(serde_json::from_value(value)?),
            Self::Full3 => NativeOwnedCapture::Full3(serde_json::from_value(value)?),
            Self::Loader6 => NativeOwnedCapture::Loader6(serde_json::from_value(value)?),
            Self::Compiler7 => NativeOwnedCapture::Compiler7(serde_json::from_value(value)?),
            Self::Sdk8 => NativeOwnedCapture::Sdk8(serde_json::from_value(value)?),
            Self::Probe9 => NativeOwnedCapture::Probe9(serde_json::from_value(value)?),
            Self::Probe10 => NativeOwnedCapture::Probe10(serde_json::from_value(value)?),
            Self::Stream11 => NativeOwnedCapture::Stream11(serde_json::from_value(value)?),
        })
    }
}
enum NativeOwnedSide {
    Direct2(oe_contracts::CodeNativeSideContext),
    Full3(oe_contracts::CodeNativeSideContext3),
    Loader6(oe_contracts::CodeNativeSideContext6),
}
impl NativeOwnedSide {
    fn value(&self) -> Result<Value> {
        Ok(match self {
            Self::Direct2(s) => serde_json::to_value(s)?,
            Self::Full3(s) => serde_json::to_value(s)?,
            Self::Loader6(s) => serde_json::to_value(s)?,
        })
    }
    fn reread(&self, root: &SkillRoot, reference: &ArtifactReference) -> Result<()> {
        let unchanged = match self {
            Self::Direct2(s) => oe_evolution::native_profile::load_side(root, reference)? == *s,
            Self::Full3(s) => oe_evolution::native_profile3::load_side(root, reference)? == *s,
            Self::Loader6(s) => oe_evolution::native_profile6::load_side(root, reference)? == *s,
        };
        ensure!(unchanged, "owner_native_side_changed");
        Ok(())
    }
}
enum NativeOwnedCapture {
    Direct2(oe_contracts::CodeNativeBuildCapture),
    Full3(oe_contracts::CodeNativeBuildCapture3),
    Loader6(oe_contracts::CodeNativeBuildCapture6),
    Compiler7(oe_contracts::CodeNativeBuildCapture7),
    Sdk8(oe_contracts::CodeNativeBuildCapture8),
    Probe9(oe_contracts::CodeNativeBuildCapture9),
    Probe10(oe_contracts::CodeNativeBuildCapture10),
    Stream11(oe_contracts::CodeNativeBuildCapture11),
}
impl NativeOwnedCapture {
    fn bindings(
        &self,
    ) -> (
        &ArtifactReference,
        &ArtifactReference,
        &oe_contracts::CodeBinaryRef,
        &ArtifactReference,
    ) {
        match self {
            Self::Direct2(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
            Self::Full3(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
            Self::Loader6(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
            Self::Compiler7(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
            Self::Sdk8(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
            Self::Probe9(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
            Self::Probe10(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
            Self::Stream11(c) => (&c.source_profile, &c.side_context, &c.driver, &c.artifact),
        }
    }
    fn value(&self) -> Result<Value> {
        Ok(match self {
            Self::Direct2(c) => serde_json::to_value(c)?,
            Self::Full3(c) => serde_json::to_value(c)?,
            Self::Loader6(c) => serde_json::to_value(c)?,
            Self::Compiler7(c) => serde_json::to_value(c)?,
            Self::Sdk8(c) => serde_json::to_value(c)?,
            Self::Probe9(c) => serde_json::to_value(c)?,
            Self::Probe10(c) => serde_json::to_value(c)?,
            Self::Stream11(c) => serde_json::to_value(c)?,
        })
    }
    fn verify(
        &self,
        root: &SkillRoot,
        p: &ArtifactReference,
        s: &ArtifactReference,
        a: &ArtifactReference,
    ) -> Result<()> {
        match self {
            Self::Direct2(c) => {
                oe_evolution::native_runtime::verify_native_capture(root, p, s, a, c)
            }
            Self::Full3(c) => {
                oe_evolution::native_runtime3::verify_full_native_capture(root, p, s, a, c)
            }
            Self::Loader6(c) => {
                oe_evolution::native_runtime6::verify_full_native_capture(root, p, s, a, c)
            }
            Self::Compiler7(c) => {
                oe_evolution::native_runtime7::verify_full_native_capture(root, p, s, a, c)
            }
            Self::Sdk8(c) => {
                oe_evolution::native_runtime8::verify_full_native_capture(root, p, s, a, c)
            }
            Self::Probe9(c) => {
                oe_evolution::native_runtime9::verify_full_native_capture(root, p, s, a, c)
            }
            Self::Probe10(c) => {
                oe_evolution::native_runtime10::verify_full_native_capture(root, p, s, a, c)
            }
            Self::Stream11(c) => {
                oe_evolution::native_stream11::verify_resources11(&c.resources)?;
                oe_evolution::native_runtime11::verify_full_native_capture(root, p, s, a, c)
            }
        }
    }
}
/// Internal common outcome only; public types are generated, no duplicate DTO.
#[derive(Clone, Copy, PartialEq)]
enum NativeTask {
    FullBuild,
    PrepareNormalizer,
}
struct NativeOwnedOutcome {
    preparation: Option<ArtifactReference>,
    execution: String,
    compilation_origin: String,
    artifact: Option<ArtifactReference>,
    capture: Option<ArtifactReference>,
    observation: ArtifactReference,
    release_eligibility: String,
    uncovered: Vec<String>,
}

/// Content inspection for the explicit native profile. Authorization stays in
/// registry::build_context; reading these inputs cannot establish build origin.
fn preflight_native(
    root: &SkillRoot,
    context: &CodeBuildSupervisionContext,
    version: NativeVersion,
) -> Result<Value> {
    ensure!(
        context.schema == "oev2.code-build-supervision-context/2",
        "owner_native_requires_total_budget_context2"
    );
    context
        .build
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_native_build_request:{e:?}"))?;
    ensure!(
        (1..=3_600_000).contains(&context.dispatch_budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&context.dispatch_budget.output_bytes),
        "owner_native_dispatch_budget"
    );
    ensure!(
        Path::new(&context.build.source_root) == root.path()
            && context.builder.source_root == context.build.source_root,
        "owner_native_source_root_not_bound"
    );
    root.private_path(Path::new(&context.source_profile.path))?;
    let profile = version.profile(root, &context.source_profile)?;
    let (baseline, manifest) =
        oe_evolution::runtime_identity::inspect_profile(root, &profile.baseline)?;
    ensure!(
        baseline.expected_build_capture.is_none()
            && baseline.source_kind == "frozen_export"
            && baseline.expected_target == context.build.platform,
        "owner_native_profile_not_prior_source_conditions"
    );
    oe_evolution::verify_binary(&context.builder)?;
    ensure!(
        context.builder.source_manifest_sha256 == oe_evolution::digest(&manifest)?,
        "owner_native_driver_source_not_bound"
    );
    let paths = [
        &context.build.cargo_path,
        &context.build.rustc_path,
        &context.build.ggen_path,
    ];
    ensure!(
        context.expected_tools.len() == paths.len(),
        "owner_native_tool_set"
    );
    for (pin, path) in context.expected_tools.iter().zip(paths) {
        ensure!(
            &pin.path == path
                && pin.source_root == context.build.source_root
                && pin.source_manifest_sha256 == context.builder.source_manifest_sha256,
            "owner_native_tool_binding_changed"
        );
        oe_evolution::verify_binary(pin)?;
    }
    native_paths_fresh(root, context)?;
    Ok(manifest)
}

fn native_paths_fresh(root: &SkillRoot, context: &CodeBuildSupervisionContext) -> Result<()> {
    let (target, stage, evidence) = native_paths(root, context)?;
    ensure!(!target.try_exists()?, "owner_native_target_not_fresh");
    ensure!(!stage.try_exists()?, "owner_native_stage_not_fresh");
    ensure!(!evidence.try_exists()?, "owner_native_evidence_not_fresh");
    Ok(())
}

fn native_paths(
    root: &SkillRoot,
    context: &CodeBuildSupervisionContext,
) -> Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    let target = root.private_path(Path::new(&context.build.target_dir))?;
    let leaf = target
        .file_name()
        .and_then(|s| s.to_str())
        .context("owner_native_target_leaf")?;
    let stage = root.private_path(&target.with_file_name(format!("{leaf}-native-stage")))?;
    let evidence = root.private_path(Path::new(&context.build.evidence_path))?;
    ensure!(
        !target.starts_with(&evidence)
            && !evidence.starts_with(&target)
            && !stage.starts_with(&evidence)
            && !evidence.starts_with(&stage),
        "owner_native_paths_overlap"
    );
    Ok((target, stage, evidence))
}

fn still_bound_native(
    root: &SkillRoot,
    request: &CodeBuildSupervisionRequest,
    original: &(CodeBuildSupervisionContext, String, ArtifactReference),
    manifest: &Value,
    version: NativeVersion,
) -> Result<()> {
    ensure!(
        registry::build_context(root, request)? == *original,
        "owner_native_policy_or_context_changed"
    );
    let profile = version.profile(root, &original.0.source_profile)?;
    let (baseline, current) =
        oe_evolution::runtime_identity::inspect_profile(root, &profile.baseline)?;
    ensure!(
        baseline.expected_build_capture.is_none() && &current == manifest,
        "owner_native_source_changed"
    );
    oe_evolution::verify_binary(&original.0.builder)?;
    for pin in &original.0.expected_tools {
        oe_evolution::verify_binary(pin)?;
    }
    Ok(())
}

// Native capture references use private absolute paths, like runtime_identity;
// no absolute reference is passed to the separate relative public-artifact API.
fn native_reference(root: &SkillRoot, file: &Path) -> Result<ArtifactReference> {
    let path = root.private_path(file)?;
    let bytes = oe_core::read_regular_bytes(&path, 16 * 1024 * 1024)?;
    Ok(ArtifactReference {
        path: path.to_str().context("owner_native_non_utf8")?.into(),
        sha256: oe_core::sha256(&bytes),
    })
}
pub(crate) fn native_json(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    reference
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_native_reference:{e:?}"))?;
    let path = root.private_path(Path::new(&reference.path))?;
    let bytes = oe_core::read_regular_bytes(&path, 16 * 1024 * 1024)?;
    ensure!(
        oe_core::sha256(&bytes) == reference.sha256,
        "owner_native_reference_changed"
    );
    Ok(oe_core::json::strict_json(&bytes)?)
}

/// Small explicit11 wire content identifies a complete retained capture. This
/// checker grants no native or owner qualification; the common owner performs
/// all original capture/source/process checks after decoding the full file.
fn stream11_dispatch_content(
    root: &SkillRoot,
    evidence: &Path,
    stdout: &[u8],
) -> Result<(ArtifactReference, Value)> {
    ensure!(
        stdout.len() <= 4 * 1024 * 1024,
        "owner_stream11_dispatch_output_limit"
    );
    let receipt: oe_contracts::CodeNativeBuildDispatchReceipt11 =
        serde_json::from_value(oe_core::json::strict_json(stdout)?)?;
    receipt
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_stream11_dispatch_contract:{e:?}"))?;
    ensure!(
        receipt.schema == "oev2.native-build-dispatch-receipt/11",
        "owner_stream11_dispatch_schema"
    );
    oe_evolution::native_stream11::verify_resources11(&receipt.resources)?;
    let evidence = root.private_path(evidence)?;
    let capture = native_reference(root, &evidence.join("native-stream11-capture.json"))?;
    ensure!(
        receipt.capture == capture,
        "owner_stream11_dispatch_capture_reference_changed"
    );
    let receipt_path =
        root.private_path(&evidence.join("native-stream11-dispatch-receipt.json"))?;
    let before = oe_core::read_regular_bytes(&receipt_path, 4 * 1024 * 1024)?;
    ensure!(
        before == stdout,
        "owner_stream11_dispatch_receipt_bytes_changed"
    );
    let content = native_json(root, &capture)?;
    ensure!(
        oe_core::read_regular_bytes(&receipt_path, 4 * 1024 * 1024)? == stdout
            && native_reference(root, &evidence.join("native-stream11-capture.json"))? == capture,
        "owner_stream11_dispatch_changed_during_read"
    );
    Ok((capture, content))
}

fn verify_native_stdout_capture(
    version: NativeVersion,
    root: &SkillRoot,
    evidence: &Path,
    capture_reference: &ArtifactReference,
    capture: &NativeOwnedCapture,
    stdout: &[u8],
) -> Result<()> {
    if version == NativeVersion::Stream11 {
        let (reference, content) = stream11_dispatch_content(root, evidence, stdout)?;
        ensure!(
            reference == *capture_reference && content == capture.value()?,
            "owner_stream11_dispatch_full_capture_changed"
        );
    } else {
        let raw = oe_core::read_regular_bytes(
            &root.private_path(Path::new(&capture_reference.path))?,
            16 * 1024 * 1024,
        )?;
        ensure!(
            raw == stdout,
            "owner_full_native_stdout_capture_bytes_changed"
        );
    }
    Ok(())
}

/// Execute an explicit native build through the same private version owner.
/// This entry cannot select a profile, tool, target or budget supplied by caller.
pub fn supervise_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult> {
    let started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/2",
        "owner_native_request_schema"
    );
    let o = supervise_native_common(
        root,
        &request.owner_request,
        NativeVersion::Direct2,
        started,
        serde_json::to_value(request)?,
    )?;
    let r = oe_contracts::CodeNativeBuildSupervisionResult {
        schema: "oev2.native-build-supervision-result/2".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("owner_native_result_contract:{e:?}"))?;
    Ok(r)
}
/// Strict full3 selector, same owner, lock, policy, context and total deadline.
pub fn supervise_full_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest3,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult3> {
    let started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/3",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common(
        root,
        &request.owner_request,
        NativeVersion::Full3,
        started,
        serde_json::to_value(request)?,
    )?;
    let r = oe_contracts::CodeNativeBuildSupervisionResult3 {
        schema: "oev2.native-build-supervision-result/3".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_result_contract:{e:?}"))?;
    Ok(r)
}
fn supervise_native_common(
    root: &SkillRoot,
    input: &CodeBuildSupervisionRequest,
    version: NativeVersion,
    owner_started: Instant,
    request_value: Value,
) -> Result<NativeOwnedOutcome> {
    supervise_native_common_with_deadline(root, input, version, owner_started, request_value, None)
}
fn supervise_native_common_with_deadline(
    root: &SkillRoot,
    input: &CodeBuildSupervisionRequest,
    version: NativeVersion,
    owner_started: Instant,
    request_value: Value,
    borrowed_deadline: Option<Instant>,
) -> Result<NativeOwnedOutcome> {
    supervise_native_task(
        root,
        input,
        version,
        owner_started,
        request_value,
        borrowed_deadline,
        NativeTask::FullBuild,
    )
}
fn supervise_native_task(
    root: &SkillRoot,
    input: &CodeBuildSupervisionRequest,
    version: NativeVersion,
    owner_started: Instant,
    request_value: Value,
    borrowed_deadline: Option<Instant>,
    task: NativeTask,
) -> Result<NativeOwnedOutcome> {
    ensure!(
        task == NativeTask::FullBuild || version == NativeVersion::Stream11,
        "owner_normalizer_requires_stream11"
    );
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_native_owner_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.code-build-supervision-request/1",
        "owner_native_owner_request_schema"
    );
    let prior = registry::build_context(root, input)?;
    let manifest = preflight_native(root, &prior.0, version)?;
    let deadline = owner_started
        .checked_add(Duration::from_millis(prior.0.dispatch_budget.timeout_ms))
        .context("owner_native_total_deadline")?;
    let deadline = borrowed_deadline.map_or(deadline, |caller| deadline.min(caller));
    body_remaining(deadline)?;
    let owner = registry::owner_folder(root, &input.binding_id)?;
    oe_core::ensure_directory(&owner)?;
    let lock = AdvisoryFileLock::acquire(&owner.join("selection.lock"), true)?;
    still_bound_native(root, input, &prior, &manifest, version)?;
    // LOCK_NB refuses a presently held lock; a prior holder can still have
    // completed between the first freshness read and our successful acquire.
    native_paths_fresh(root, &prior.0)?;
    body_remaining(deadline)?;
    let serial = SERIAL.fetch_add(1, Ordering::SeqCst);
    let side_id = format!("{}-{}-{serial}", &prior.2.sha256[..16], std::process::id());
    let relative = format!(
        "{}/build-observations/native-{side_id}",
        owner
            .strip_prefix(root.path())?
            .to_str()
            .context("owner_native_path")?
    );
    let folder = root.new_private_dir(Path::new(&relative))?;
    let profile = version.profile(root, &prior.0.source_profile)?;
    let (_, stage, _) = native_paths(root, &prior.0)?;
    let body = body_remaining(deadline)?;
    let budget = CodeBudget {
        timeout_ms: u64::try_from(body.as_millis())?,
        output_bytes: prior.0.dispatch_budget.output_bytes,
    };
    let side = version.side(
        root,
        &prior,
        profile.recipe.sha256.clone(),
        side_id,
        stage.to_str().context("owner_native_non_utf8")?.into(),
        budget.clone(),
    )?;
    let side_file = folder.join("native-side.json");
    write_new(&side_file, &side.value()?)?;
    let side_reference = native_reference(root, &side_file)?;
    let bounded = version.bounded(prior.0.build.clone(), budget, side_reference.clone())?;
    let bounded = if task == NativeTask::PrepareNormalizer {
        serde_json::to_value(oe_contracts::CodeNativeNormalizerPreparationRequest1 {
            schema: "oev2.native-normalizer-preparation-request/1".into(),
            request: serde_json::from_value(bounded)?,
        })?
    } else {
        bounded
    };
    let build_file = folder.join("native-build-request.json");
    write_new(&build_file, &bounded)?;
    let dispatch_timeout = deadline.saturating_duration_since(Instant::now());
    ensure!(
        dispatch_timeout >= Duration::from_millis(1),
        "owner_build_total_budget_exhausted"
    );
    let command = CommandSpec {
        executable: prior.0.builder.path.clone().into(),
        arguments: vec![
            (if task == NativeTask::PrepareNormalizer {
                "prepare-stream11-normalizer"
            } else {
                version.command()
            })
            .into(),
            build_file.into_os_string(),
        ],
        cwd: root.path().into(),
        environment: BTreeMap::new(),
        remove_environment: vec![
            "RUSTFLAGS".into(),
            "CARGO_ENCODED_RUSTFLAGS".into(),
            "RUSTC_WRAPPER".into(),
            "RUSTC_WORKSPACE_WRAPPER".into(),
        ],
        timeout: dispatch_timeout,
        output_limit: usize::try_from(prior.0.dispatch_budget.output_bytes)?,
    };
    let process = if borrowed_deadline.is_some() {
        oe_core::process::run_command_with_deadline(
            &command,
            &folder.join("builder-dispatch"),
            deadline,
        )?
    } else {
        run_command(&command, &folder.join("builder-dispatch"))?
    };
    let process_reference =
        artifacts::reference(root, &folder.join("builder-dispatch/process.json"))?;
    let mut artifact_reference = None;
    let mut capture_reference = None;
    let mut preparation_reference = None;
    let evaluated = (|| -> Result<()> {
        ensure!(
            process.status == "completed" && process.exit_code == Some(0),
            "owner_native_process_not_completed"
        );
        let evidence = root.private_path(Path::new(&prior.0.build.evidence_path))?;
        if task == NativeTask::PrepareNormalizer {
            let prepared: oe_contracts::CodeNativeNormalizerPreparation1 =
                serde_json::from_value(oe_core::json::strict_json(&process.stdout)?)?;
            let path = evidence.join(oe_evolution::native_normalizer::FILE);
            let saved = native_reference(root, &path)?;
            ensure!(
                native_json(root, &saved)? == serde_json::to_value(&prepared)?
                    && oe_core::read_regular_bytes(&path, 16 * 1024 * 1024)? == process.stdout,
                "owner_normalizer_stdout_changed"
            );
            ensure!(
                prepared.source_profile == prior.0.source_profile
                    && prepared.side_context == side_reference
                    && prepared.build_driver == prior.0.builder
                    && prepared.source_manifest_sha256 == oe_evolution::digest(&manifest)?
                    && prepared.tools == prior.0.expected_tools,
                "owner_normalizer_binding_changed"
            );
            oe_evolution::native_normalizer::verify_preparation(root, &prepared)?;
            side.reread(root, &side_reference)?;
            still_bound_native(root, input, &prior, &manifest, version)?;
            ensure!(
                native_reference(root, &path)? == saved,
                "owner_normalizer_receipt_changed"
            );
            lock.ensure_current()?;
            ensure!(
                Instant::now() < deadline,
                "owner_build_total_budget_exhausted"
            );
            preparation_reference = Some(saved);
            return Ok(());
        }
        let capture = version.capture(root, &evidence, &process.stdout)?;
        let (capture_profile, capture_side, capture_driver, capture_artifact) = capture.bindings();
        ensure!(
            *capture_profile == prior.0.source_profile
                && *capture_side == side_reference
                && *capture_driver == prior.0.builder,
            "owner_native_capture_binding_changed"
        );
        let from_builder = native_reference(root, &evidence.join(version.capture_file()))?;
        ensure!(
            native_json(root, &from_builder)? == capture.value()?,
            "owner_native_stdout_capture_changed"
        );
        let artifact_from_builder = native_reference(root, &evidence.join("artifact.json"))?;
        ensure!(
            *capture_artifact == artifact_from_builder,
            "owner_native_artifact_path_changed"
        );
        let artifact: CodeSourceArtifact =
            serde_json::from_value(native_json(root, capture_artifact)?)?;
        ensure!(
            artifact.manifest == manifest
                && artifact.artifact_id == prior.0.build.artifact_id
                && artifact.source_root == prior.0.build.source_root
                && artifact.platform == prior.0.build.platform
                && artifact.tools.len() == 4
                && artifact.tools[..3] == prior.0.expected_tools
                && artifact.generation["fixed_build_driver"]["executable_sha256"]
                    == prior.0.builder.sha256
                && artifact.generation["fixed_build_driver"]["source_manifest_sha256"]
                    == prior.0.builder.source_manifest_sha256,
            "owner_native_artifact_binding_changed"
        );
        let (target, _, _) = native_paths(root, &prior.0)?;
        ensure!(
            Path::new(&artifact.runner.path)
                == target
                    .join(&prior.0.build.platform)
                    .join("debug/oe-code-runner"),
            "owner_native_runner_path_changed"
        );
        if matches!(
            version,
            NativeVersion::Full3
                | NativeVersion::Loader6
                | NativeVersion::Compiler7
                | NativeVersion::Sdk8
                | NativeVersion::Probe9
                | NativeVersion::Probe10
                | NativeVersion::Stream11
        ) {
            verify_native_stdout_capture(
                version,
                root,
                &evidence,
                &from_builder,
                &capture,
                &process.stdout,
            )?;
            ensure!(
                Path::new(&artifact.tools[3].path) == target.join("debug/oev2-design-bootstrap"),
                "owner_full_native_normalizer_path_changed"
            );
            let refs = version.profile(root, &prior.0.source_profile)?;
            let (baseline, _) =
                oe_evolution::runtime_identity::inspect_profile(root, &refs.baseline)?;
            ensure!(
                artifact.compiled["package_version"] == baseline.expected_package_version
                    && artifact.compiled["target"] == baseline.expected_target
                    && artifact.compiled["language"] == "rust",
                "owner_full_native_compiled_package_or_target_changed"
            );
        }
        oe_evolution::verify_artifact(&artifact)?;
        capture.verify(
            root,
            &prior.0.source_profile,
            &side_reference,
            &artifact_from_builder,
        )?;
        side.reread(root, &side_reference)?;
        still_bound_native(root, input, &prior, &manifest, version)?;
        if matches!(
            version,
            NativeVersion::Full3
                | NativeVersion::Loader6
                | NativeVersion::Compiler7
                | NativeVersion::Sdk8
                | NativeVersion::Probe9
                | NativeVersion::Probe10
                | NativeVersion::Stream11
        ) {
            ensure!(
                native_reference(root, &evidence.join(version.capture_file()))? == from_builder
                    && native_json(root, &from_builder)? == capture.value()?
                    && (version == NativeVersion::Stream11
                        || oe_core::read_regular_bytes(
                            &root.private_path(Path::new(&from_builder.path))?,
                            16 * 1024 * 1024
                        )? == process.stdout),
                "owner_full_native_final_capture_changed"
            );
            if version == NativeVersion::Stream11 {
                verify_native_stdout_capture(
                    version,
                    root,
                    &evidence,
                    &from_builder,
                    &capture,
                    &process.stdout,
                )?;
            }
            ensure!(
                native_reference(root, &evidence.join("artifact.json"))? == artifact_from_builder,
                "owner_full_native_final_artifact_changed"
            );
        }
        lock.ensure_current()?;
        ensure!(
            Instant::now() < deadline,
            "owner_build_total_budget_exhausted"
        );
        artifact_reference = Some(artifact_from_builder);
        capture_reference = Some(from_builder);
        Ok(())
    })();
    let completed = evaluated.is_ok() && Instant::now() < deadline;
    let reason = evaluated
        .err()
        .map(|e| format!("{e:#}"))
        .or_else(|| (!completed).then(|| "owner_build_total_budget_exhausted".into()));
    if !completed {
        artifact_reference = None;
        capture_reference = None;
        preparation_reference = None;
    }
    let observation_file = folder.join("native-supervision.json");
    let mut observation = json!({
        "schema":version.observation_schema(), "request":request_value,
        "policy_sha256":prior.1, "context":prior.2, "side_context":side_reference,
        "process":process_reference, "artifact":artifact_reference, "capture":capture_reference,
        "execution":if completed {"completed"} else {"failed"},
        "compilation_origin":if completed {"owner_observed"} else {"unknown"},
        "reason":reason, "release_eligibility":"unknown", "selection":"not_run",
        "budget_observation":{"profile":"owner_supervise_total/2",
            "entry":version.entry(),
            "timeout_ms":prior.0.dispatch_budget.timeout_ms, "elapsed_ms":owner_started.elapsed().as_millis(),
            "cleanup_reserve_ms":250,"per_stream_output_bytes":prior.0.dispatch_budget.output_bytes,
            "limit":"local monotonic cancellation and verification boundary; blocking I/O/final persistence is not certified hard realtime or total resource cost"},
        "engineering_adoption":"not_established", "publication":"not_performed"
    });
    if task == NativeTask::PrepareNormalizer {
        observation["schema"] = json!("oev2.native-normalizer-supervision-observation/1");
        observation["preparation"] = serde_json::to_value(&preparation_reference)?;
        observation["full_build"] = json!("not_run");
        observation["budget_observation"]["entry"] =
            json!("supervise_normalizer entry; original owner budget and lock");
    }
    write_new(&observation_file, &observation)?;
    let result = NativeOwnedOutcome {
        preparation: preparation_reference,
        execution: if completed { "completed" } else { "failed" }.into(),
        compilation_origin: if completed { "owner_observed" } else { "unknown" }.into(),
        artifact: artifact_reference, capture: capture_reference,
        observation: artifacts::reference(root, &observation_file)?,
        release_eligibility: "unknown".into(),
        uncovered: vec!["Same private version authority and selection lock; content checks do not authorize caller or establish hostile-code containment".into(),
            "Observed native build does not prove reproducibility, full13494 behavior, all platforms, original19 protocols/22 owners or engineering delivery".into(),
            "Activation, use, Project feedback, recovery, publication and engineering adoption remain separately unestablished".into()],
    };
    Ok(result)
}

/// Explicit current3 composition borrows one caller Instant; no new authority.
pub(crate) fn supervise_full_native_with_deadline(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest3,
    deadline: Instant,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult3> {
    let started = Instant::now();
    ensure!(started < deadline, "owner_build_total_budget_exhausted");
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/3",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common_with_deadline(
        root,
        &request.owner_request,
        NativeVersion::Full3,
        started,
        serde_json::to_value(request)?,
        Some(deadline),
    )?;
    Ok(oe_contracts::CodeNativeBuildSupervisionResult3 {
        schema: "oev2.native-build-supervision-result/3".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    })
}

#[cfg(test)]
mod native_reproduction_deadline_tests {
    use super::*;
    #[test]
    fn expired_borrowed_native3_deadline_refuses_before_owner_or_spawn() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "expired deadline independent control",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let request = oe_contracts::CodeNativeBuildSupervisionRequest3 {
            schema: "oev2.native-build-supervision-request/3".into(),
            owner_request: CodeBuildSupervisionRequest {
                schema: "oev2.code-build-supervision-request/1".into(),
                binding_id: "code".into(),
                revision: "r1".into(),
                scope_id: "scope".into(),
                actor: "actor".into(),
            },
        };
        let error = supervise_full_native_with_deadline(
            &root,
            &request,
            Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner_build_total_budget_exhausted")
        );
        assert!(!root.path().join("var").exists());
    }
}

pub fn supervise_loader6_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest6,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult6> {
    let started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/6",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common(
        root,
        &request.owner_request,
        NativeVersion::Loader6,
        started,
        serde_json::to_value(request)?,
    )?;
    let r = oe_contracts::CodeNativeBuildSupervisionResult6 {
        schema: "oev2.native-build-supervision-result/6".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_result_contract:{e:?}"))?;
    Ok(r)
}

pub(crate) fn supervise_loader6_native_with_deadline(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest6,
    deadline: Instant,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult6> {
    let started = Instant::now();
    ensure!(started < deadline, "owner_build_total_budget_exhausted");
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/6",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common_with_deadline(
        root,
        &request.owner_request,
        NativeVersion::Loader6,
        started,
        serde_json::to_value(request)?,
        Some(deadline),
    )?;
    Ok(oe_contracts::CodeNativeBuildSupervisionResult6 {
        schema: "oev2.native-build-supervision-result/6".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    })
}

pub fn supervise_compiler7_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest7,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult7> {
    let started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/7",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common(
        root,
        &request.owner_request,
        NativeVersion::Compiler7,
        started,
        serde_json::to_value(request)?,
    )?;
    let r = oe_contracts::CodeNativeBuildSupervisionResult7 {
        schema: "oev2.native-build-supervision-result/7".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_result_contract:{e:?}"))?;
    Ok(r)
}

pub(crate) fn supervise_compiler7_native_with_deadline(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest7,
    deadline: Instant,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult7> {
    let started = Instant::now();
    ensure!(started < deadline, "owner_build_total_budget_exhausted");
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/7",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common_with_deadline(
        root,
        &request.owner_request,
        NativeVersion::Compiler7,
        started,
        serde_json::to_value(request)?,
        Some(deadline),
    )?;
    Ok(oe_contracts::CodeNativeBuildSupervisionResult7 {
        schema: "oev2.native-build-supervision-result/7".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    })
}

#[cfg(test)]
mod compiler7_borrowed_deadline_tests {
    use super::*;
    #[test]
    fn expired_borrowed_compiler7_deadline_refuses_before_owner_or_spawn() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "independent compiler7 deadline control",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let request = oe_contracts::CodeNativeBuildSupervisionRequest7 {
            schema: "oev2.native-build-supervision-request/7".into(),
            owner_request: CodeBuildSupervisionRequest {
                schema: "oev2.code-build-supervision-request/1".into(),
                binding_id: "missing".into(),
                revision: "r1".into(),
                scope_id: "scope".into(),
                actor: "actor".into(),
            },
        };
        let error = supervise_compiler7_native_with_deadline(
            &root,
            &request,
            Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner_build_total_budget_exhausted")
        );
        assert!(!root.path().join("var").exists());
    }
}

pub fn supervise_sdk8_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest8,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult8> {
    let started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/8",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common(
        root,
        &request.owner_request,
        NativeVersion::Sdk8,
        started,
        serde_json::to_value(request)?,
    )?;
    let r = oe_contracts::CodeNativeBuildSupervisionResult8 {
        schema: "oev2.native-build-supervision-result/8".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_result_contract:{e:?}"))?;
    Ok(r)
}

pub(crate) fn supervise_sdk8_native_with_deadline(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest8,
    deadline: Instant,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult8> {
    let started = Instant::now();
    ensure!(started < deadline, "owner_build_total_budget_exhausted");
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/8",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common_with_deadline(
        root,
        &request.owner_request,
        NativeVersion::Sdk8,
        started,
        serde_json::to_value(request)?,
        Some(deadline),
    )?;
    Ok(oe_contracts::CodeNativeBuildSupervisionResult8 {
        schema: "oev2.native-build-supervision-result/8".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    })
}

#[cfg(test)]
mod sdk8_borrowed_deadline_tests {
    use super::*;
    #[test]
    fn expired_borrowed_sdk8_deadline_refuses_before_owner_or_spawn() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "independent compiler7 deadline control",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let request = oe_contracts::CodeNativeBuildSupervisionRequest8 {
            schema: "oev2.native-build-supervision-request/8".into(),
            owner_request: CodeBuildSupervisionRequest {
                schema: "oev2.code-build-supervision-request/1".into(),
                binding_id: "missing".into(),
                revision: "r1".into(),
                scope_id: "scope".into(),
                actor: "actor".into(),
            },
        };
        let error = supervise_sdk8_native_with_deadline(
            &root,
            &request,
            Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner_build_total_budget_exhausted")
        );
        assert!(!root.path().join("var").exists());
    }
}

// Explicit9 mode delegates to the same owner and unchanged pair deadline.
pub fn supervise_probe9_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest9,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult9> {
    let started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/9",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common(
        root,
        &request.owner_request,
        NativeVersion::Probe9,
        started,
        serde_json::to_value(request)?,
    )?;
    let r = oe_contracts::CodeNativeBuildSupervisionResult9 {
        schema: "oev2.native-build-supervision-result/9".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_result_contract:{e:?}"))?;
    Ok(r)
}

pub(crate) fn supervise_probe9_native_with_deadline(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest9,
    deadline: Instant,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult9> {
    let started = Instant::now();
    ensure!(started < deadline, "owner_build_total_budget_exhausted");
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/9",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common_with_deadline(
        root,
        &request.owner_request,
        NativeVersion::Probe9,
        started,
        serde_json::to_value(request)?,
        Some(deadline),
    )?;
    Ok(oe_contracts::CodeNativeBuildSupervisionResult9 {
        schema: "oev2.native-build-supervision-result/9".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    })
}

#[cfg(test)]
mod probe9_borrowed_deadline_tests {
    use super::*;
    #[test]
    fn expired_borrowed_probe9_deadline_refuses_before_owner_or_spawn() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "independent compiler7 deadline control",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let request = oe_contracts::CodeNativeBuildSupervisionRequest9 {
            schema: "oev2.native-build-supervision-request/9".into(),
            owner_request: CodeBuildSupervisionRequest {
                schema: "oev2.code-build-supervision-request/1".into(),
                binding_id: "missing".into(),
                revision: "r1".into(),
                scope_id: "scope".into(),
                actor: "actor".into(),
            },
        };
        let error = supervise_probe9_native_with_deadline(
            &root,
            &request,
            Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner_build_total_budget_exhausted")
        );
        assert!(!root.path().join("var").exists());
    }
}

// Explicit10 mode delegates to the same owner and unchanged pair deadline.
pub fn supervise_probe10_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest10,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult10> {
    let started = Instant::now();
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/10",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common(
        root,
        &request.owner_request,
        NativeVersion::Probe10,
        started,
        serde_json::to_value(request)?,
    )?;
    let r = oe_contracts::CodeNativeBuildSupervisionResult10 {
        schema: "oev2.native-build-supervision-result/10".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_result_contract:{e:?}"))?;
    Ok(r)
}

pub(crate) fn supervise_probe10_native_with_deadline(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest10,
    deadline: Instant,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult10> {
    let started = Instant::now();
    ensure!(started < deadline, "owner_build_total_budget_exhausted");
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_full_native_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/10",
        "owner_full_native_request_schema"
    );
    let o = supervise_native_common_with_deadline(
        root,
        &request.owner_request,
        NativeVersion::Probe10,
        started,
        serde_json::to_value(request)?,
        Some(deadline),
    )?;
    Ok(oe_contracts::CodeNativeBuildSupervisionResult10 {
        schema: "oev2.native-build-supervision-result/10".into(),
        owner_request: request.owner_request.clone(),
        execution: o.execution,
        compilation_origin: o.compilation_origin,
        artifact: o.artifact,
        capture: o.capture,
        observation: o.observation,
        release_eligibility: o.release_eligibility,
        uncovered: o.uncovered,
    })
}

#[cfg(test)]
mod probe10_borrowed_deadline_tests {
    use super::*;
    #[test]
    fn expired_borrowed_probe10_deadline_refuses_before_owner_or_spawn() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "explicit host condition deadline counterexample",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let request = oe_contracts::CodeNativeBuildSupervisionRequest10 {
            schema: "oev2.native-build-supervision-request/10".into(),
            owner_request: CodeBuildSupervisionRequest {
                schema: "oev2.code-build-supervision-request/1".into(),
                binding_id: "missing".into(),
                revision: "r1".into(),
                scope_id: "scope".into(),
                actor: "actor".into(),
            },
        };
        let error = supervise_probe10_native_with_deadline(
            &root,
            &request,
            Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner_build_total_budget_exhausted")
        );
        assert!(!root.path().join("var").exists());
    }
}

/// Explicit stream11 resources are selected by this execution profile, while
/// registry/context2 retains the original authority, body budget and lock.
pub fn supervise_stream11_native(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest11,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult11> {
    supervise_stream11_native_entry(root, request, None)
}

pub(crate) fn supervise_stream11_native_with_deadline(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest11,
    deadline: Instant,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult11> {
    supervise_stream11_native_entry(root, request, Some(deadline))
}

fn supervise_stream11_native_entry(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest11,
    borrowed_deadline: Option<Instant>,
) -> Result<oe_contracts::CodeNativeBuildSupervisionResult11> {
    let started = Instant::now();
    ensure!(
        borrowed_deadline.is_none_or(|d| started < d),
        "owner_build_total_budget_exhausted"
    );
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_stream11_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-build-supervision-request/11",
        "owner_stream11_request_schema"
    );
    oe_evolution::native_stream11::verify_resources11(&request.resources)?;
    let outcome = supervise_native_common_with_deadline(
        root,
        &request.owner_request,
        NativeVersion::Stream11,
        started,
        serde_json::to_value(request)?,
        borrowed_deadline,
    )?;
    let result = oe_contracts::CodeNativeBuildSupervisionResult11 {
        schema: "oev2.native-build-supervision-result/11".into(),
        owner_request: request.owner_request.clone(),
        resources: request.resources.clone(),
        execution: outcome.execution,
        compilation_origin: outcome.compilation_origin,
        artifact: outcome.artifact,
        capture: outcome.capture,
        observation: outcome.observation,
        release_eligibility: outcome.release_eligibility,
        uncovered: outcome.uncovered,
    };
    result
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_stream11_result:{e:?}"))?;
    Ok(result)
}

#[cfg(test)]
mod stream11_borrowed_deadline_tests {
    use super::*;
    #[test]
    fn private_capture_reference_domains_preserve_bytes_and_refuse_escape_symlink_or_change() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "private reference domain control",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let folder = root.new_private_dir(Path::new("var/input")).unwrap();
        let file = folder.join("capture.json");
        let content = json!({"schema":"independent.private-content/1","value":"AB"});
        write_new(&file, &content).unwrap();
        let absolute = native_reference(&root, &file).unwrap();
        let relative = artifacts::reference(&root, &file).unwrap();
        assert!(Path::new(&absolute.path).is_absolute());
        assert!(!Path::new(&relative.path).is_absolute());
        assert_eq!(native_json(&root, &absolute).unwrap(), content);
        assert_eq!(native_json(&root, &relative).unwrap(), content);
        // The relative-only public reader deliberately retains its old domain.
        assert!(artifacts::json(&root, &absolute).is_err());
        let outside = tempfile::NamedTempFile::new_in(workspace.join("var/builds")).unwrap();
        std::fs::write(outside.path(), std::fs::read(&file).unwrap()).unwrap();
        let escaped = ArtifactReference {
            path: outside.path().to_str().unwrap().into(),
            sha256: absolute.sha256.clone(),
        };
        assert!(native_json(&root, &escaped).is_err());
        #[cfg(unix)]
        {
            let link = folder.join("link.json");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            let linked = ArtifactReference {
                path: link.to_str().unwrap().into(),
                sha256: absolute.sha256.clone(),
            };
            assert!(native_json(&root, &linked).is_err());
        }
        let before = std::fs::read(&file).unwrap();
        let changed = String::from_utf8(before.clone())
            .unwrap()
            .replace("AB", "BA");
        assert_eq!(changed.len(), before.len());
        std::fs::write(&file, changed).unwrap();
        assert!(
            native_json(&root, &absolute)
                .unwrap_err()
                .to_string()
                .contains("owner_native_reference_changed")
        );
        assert!(native_json(&root, &relative).is_err());
    }
    #[test]
    fn expired_stream11_pair_deadline_refuses_before_owner_or_spawn() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "explicit stream11 deadline control",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let request = oe_contracts::CodeNativeBuildSupervisionRequest11 {
            schema: "oev2.native-build-supervision-request/11".into(),
            owner_request: CodeBuildSupervisionRequest {
                schema: "oev2.code-build-supervision-request/1".into(),
                binding_id: "missing".into(),
                revision: "r1".into(),
                scope_id: "scope".into(),
                actor: "actor".into(),
            },
            resources: oe_evolution::native_stream11::selected_resources11(),
        };
        let error = supervise_stream11_native_with_deadline(
            &root,
            &request,
            Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner_build_total_budget_exhausted")
        );
        assert!(!root.path().join("var").exists());
    }
}

#[cfg(test)]
mod stream11_dispatch_receipt_tests {
    use super::*;

    fn fixture(
        payload_bytes: usize,
    ) -> (
        tempfile::TempDir,
        SkillRoot,
        std::path::PathBuf,
        Value,
        Vec<u8>,
    ) {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        std::fs::write(
            temp.path().join("SKILL.md"),
            "dispatch receipt finite content control",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        let evidence = root.new_private_dir(Path::new("var/evidence")).unwrap();
        // Deliberately not a native capture: only complete finite transport is
        // exercised. Passing this helper must not qualify this synthetic JSON.
        let content = json!({"schema":"independent.complete-private-content/1", "original":"q".repeat(payload_bytes)});
        write_new(&evidence.join("native-stream11-capture.json"), &content).unwrap();
        let receipt = oe_contracts::CodeNativeBuildDispatchReceipt11 {
            schema: "oev2.native-build-dispatch-receipt/11".into(),
            capture: native_reference(&root, &evidence.join("native-stream11-capture.json"))
                .unwrap(),
            resources: oe_evolution::native_stream11::selected_resources11(),
        };
        let file = evidence.join("native-stream11-dispatch-receipt.json");
        write_new(&file, &serde_json::to_value(receipt).unwrap()).unwrap();
        let bytes = std::fs::read(&file).unwrap();
        (temp, root, evidence, content, bytes)
    }

    #[test]
    fn real_small_receipt_transports_reference_to_complete_capture_beyond_old_stdout_limit() {
        let (_temp, root, evidence, content, receipt) = fixture(5 * 1024 * 1024);
        let captured = evidence.join("native-stream11-capture.json");
        assert!(std::fs::metadata(&captured).unwrap().len() > 4 * 1024 * 1024);
        assert!(receipt.len() < 4096);
        let make_command = |file: &Path| CommandSpec {
            executable: "/bin/cat".into(),
            arguments: vec![file.as_os_str().into()],
            cwd: root.path().into(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout: Duration::from_secs(10),
            output_limit: 4 * 1024 * 1024,
        };
        let inline = run_command(
            &make_command(&captured),
            &evidence.join("controlled-inline-dispatch"),
        )
        .unwrap();
        assert_eq!(inline.status, "output_limit");
        assert_eq!(inline.stdout.len(), 4 * 1024 * 1024);
        let actual = run_command(
            &make_command(&evidence.join("native-stream11-dispatch-receipt.json")),
            &evidence.join("controlled-small-dispatch"),
        )
        .unwrap();
        assert_eq!(actual.status, "completed");
        assert_eq!(actual.exit_code, Some(0));
        assert_eq!(actual.stdout, receipt);
        let (reference, complete) =
            stream11_dispatch_content(&root, &evidence, &actual.stdout).unwrap();
        assert_eq!(reference, native_reference(&root, &captured).unwrap());
        assert_eq!(complete, content);
        assert!(
            NativeVersion::Stream11
                .capture(&root, &evidence, &actual.stdout)
                .is_err()
        );
        assert!(!root.path().join("var/state/controlled-versions").exists());
    }

    #[test]
    fn dispatch_receipt_wrong_path_hash_resources_schema_and_inline_capture_refuse() {
        let (_temp, root, evidence, _content, original) = fixture(128);
        let baseline: Value = serde_json::from_slice(&original).unwrap();
        let alias = evidence.join("different-capture.json");
        std::fs::copy(evidence.join("native-stream11-capture.json"), &alias).unwrap();
        let mut variants = Vec::new();
        let mut path = baseline.clone();
        path["capture"]["path"] = json!(alias);
        variants.push(path);
        let mut hash = baseline.clone();
        hash["capture"]["sha256"] = json!("0".repeat(64));
        variants.push(hash);
        let mut resource = baseline.clone();
        resource["resources"]["raw_stream_bytes"] = json!(128 * 1024 * 1024);
        variants.push(resource);
        let mut schema = baseline.clone();
        schema["schema"] = json!("oev2.native-build-dispatch-receipt/10");
        variants.push(schema);
        variants.push(json!({"schema":"oev2.native-build-capture/10","invocations":[]}));
        variants.push(json!({"schema":"oev2.native-build-capture/11","invocations":[],"resources":baseline["resources"]}));
        for changed in variants {
            let bytes = serde_json::to_vec(&changed).unwrap();
            // Even when the changed file exactly matches changed stdout, the
            // fixed capture path, SHA, resource baseline and schema must hold.
            std::fs::write(
                evidence.join("native-stream11-dispatch-receipt.json"),
                &bytes,
            )
            .unwrap();
            assert!(
                stream11_dispatch_content(&root, &evidence, &bytes).is_err(),
                "{changed}"
            );
        }
        assert!(!root.path().join("var/state/controlled-versions").exists());
    }

    #[test]
    fn dispatch_final_recheck_refuses_changed_receipt_or_complete_capture_bytes() {
        let (_temp, root, evidence, _content, stdout) = fixture(128);
        stream11_dispatch_content(&root, &evidence, &stdout).unwrap();
        let receipt_path = evidence.join("native-stream11-dispatch-receipt.json");
        let mut changed = stdout.clone();
        changed.push(b'\n');
        std::fs::write(&receipt_path, changed).unwrap();
        let error = stream11_dispatch_content(&root, &evidence, &stdout)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("owner_stream11_dispatch_receipt_bytes_changed"),
            "{error}"
        );
        std::fs::write(&receipt_path, &stdout).unwrap();
        stream11_dispatch_content(&root, &evidence, &stdout).unwrap();
        let capture_path = evidence.join("native-stream11-capture.json");
        let mut bytes = std::fs::read(&capture_path).unwrap();
        let location = bytes.iter().position(|b| *b == b'q').unwrap();
        bytes[location] = b'r';
        std::fs::write(&capture_path, bytes).unwrap();
        let error = stream11_dispatch_content(&root, &evidence, &stdout)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("owner_stream11_dispatch_capture_reference_changed"),
            "{error}"
        );
    }
}

/// A distinct limited result on the existing controlled-version owner.
pub fn supervise_normalizer(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeNormalizerSupervisionRequest1,
) -> Result<oe_contracts::CodeNativeNormalizerSupervisionResult1> {
    supervise_normalizer_entry(root, request, None)
}
fn supervise_normalizer_entry(
    root: &SkillRoot,
    request: &oe_contracts::CodeNativeNormalizerSupervisionRequest1,
    borrowed_deadline: Option<Instant>,
) -> Result<oe_contracts::CodeNativeNormalizerSupervisionResult1> {
    let started = Instant::now();
    ensure!(
        borrowed_deadline.is_none_or(|d| started < d),
        "owner_build_total_budget_exhausted"
    );
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_normalizer_request:{e:?}"))?;
    ensure!(
        request.schema == "oev2.native-normalizer-supervision-request/1",
        "owner_normalizer_schema"
    );
    oe_evolution::native_stream11::verify_resources11(&request.resources)?;
    let outcome = supervise_native_task(
        root,
        &request.owner_request,
        NativeVersion::Stream11,
        started,
        serde_json::to_value(request)?,
        borrowed_deadline,
        NativeTask::PrepareNormalizer,
    )?;
    ensure!(
        outcome.artifact.is_none() && outcome.capture.is_none(),
        "owner_normalizer_full_artifact"
    );
    let result = oe_contracts::CodeNativeNormalizerSupervisionResult1 {
        schema: "oev2.native-normalizer-supervision-result/1".into(),
        owner_request: request.owner_request.clone(), execution: outcome.execution,
        compilation_origin: outcome.compilation_origin, preparation: outcome.preparation,
        observation: outcome.observation, full_build: "not_run".into(),
        release_eligibility: "unknown".into(),
        uncovered: vec!["Only normalizer preparation is observed; full native build, debug/signature, reproduction, release and engineering adoption remain unestablished".into()],
    };
    result
        .validate()
        .map_err(|e| anyhow::anyhow!("owner_normalizer_result:{e:?}"))?;
    Ok(result)
}

#[cfg(test)]
mod normalizer_preparation_controls {
    use super::*;
    fn input() -> oe_contracts::CodeNativeNormalizerSupervisionRequest1 {
        oe_contracts::CodeNativeNormalizerSupervisionRequest1 {
            schema: "oev2.native-normalizer-supervision-request/1".into(),
            owner_request: CodeBuildSupervisionRequest {
                schema: "oev2.code-build-supervision-request/1".into(),
                binding_id: "unowned".into(),
                revision: "r1".into(),
                scope_id: "scope".into(),
                actor: "actor".into(),
            },
            resources: oe_evolution::native_stream11::selected_resources11(),
        }
    }
    #[test]
    fn normalizer_preparation_missing_authority_never_dispatches() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("SKILL.md"), "owner preparation fixture").unwrap();
        let root = SkillRoot::new(temp.path().canonicalize().unwrap()).unwrap();
        assert!(supervise_normalizer(&root, &input()).is_err());
        assert!(!root.path().join("var").exists());
    }
    #[test]
    fn normalizer_preparation_deadline_and_schema_reject_before_owner() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("SKILL.md"), "owner preparation fixture").unwrap();
        let root = SkillRoot::new(temp.path().canonicalize().unwrap()).unwrap();
        let error = supervise_normalizer_entry(
            &root,
            &input(),
            Some(Instant::now() - Duration::from_millis(1)),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("owner_build_total_budget_exhausted")
        );
        let mut wrong = input();
        wrong.schema = "oev2.native-build-supervision-request/11".into();
        assert!(
            supervise_normalizer(&root, &wrong)
                .unwrap_err()
                .to_string()
                .contains("owner_normalizer_request")
        );
        wrong = input();
        wrong.resources.raw_stream_bytes += 1;
        assert!(supervise_normalizer(&root, &wrong).is_err());
        assert!(!root.path().join("var").exists());
    }
}
