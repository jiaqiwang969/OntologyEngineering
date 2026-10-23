//! Fixed code-comparison orchestration; no version or Project authority.
use anyhow::{Context, Result, ensure};
pub use oe_contracts::*;
use oe_core::process::{CommandSpec, run_command};
use oe_core::{
    SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256, write_new,
    write_new_bytes,
};
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
static PROBE_SERIAL: AtomicU64 = AtomicU64::new(0);
mod generation_artifact;
#[allow(dead_code)]
#[path = "../../../build_support/code_identity.rs"]
mod inventory;
pub mod native_compiler_link7;
pub mod native_compiler_probe10;
pub mod native_compiler_probe9;
mod native_debug;
mod native_host_compiler10;
mod native_host_metadata8;
mod native_input_content7;
mod native_link;
mod native_link3;
mod native_link8;
pub mod native_link_files7;
mod native_loader11;
pub mod native_loader11_content;
pub mod native_normalizer;
pub mod native_probe_exec10;
pub mod native_probe_exec9;
pub mod native_profile;
pub mod native_runtime;
pub mod native_runtime10;
pub mod native_runtime11;
pub mod native_runtime3;
pub mod native_runtime8;
pub mod native_runtime9;
pub mod native_sdk;
mod native_sources4;
mod native_stdin_file9;
pub mod native_stream11;
pub mod native_system_link8;
mod normalizer_stage;
mod public_source_checks;
pub mod runtime_identity;
pub mod runtime_reproduction2;
pub fn compiled() -> Result<Value> {
    Ok(strict_json(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/code-source.json"
    )))?)
}
/// Public source tree this program was built from: `skill/` next to the installed `bin/`, or the tree named by
/// `OEV2_DRIVER_SOURCE_ROOT` in development builds. Callers still compare its manifest with the compiled
/// `manifest_sha256`, so the location confers no trust.
pub fn driver_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("OEV2_DRIVER_SOURCE_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let exe = std::fs::canonicalize(std::env::current_exe()?)?;
    let skill = exe
        .parent()
        .and_then(Path::parent)
        .map(|install| install.join("skill"))
        .context("driver_root_unavailable")?;
    ensure!(skill.join("SKILL.md").is_file(), "driver_root_unavailable");
    Ok(skill)
}
pub fn digest(value: &Value) -> Result<String> {
    Ok(sha256(canonical_json(value)?))
}
pub fn public_manifest(root: &Path) -> Result<Value> {
    public_manifest_with_profile(
        root,
        inventory::PublicManifestProfile::SourceReadmeDerived32,
    )
}
fn public_manifest_with_profile(
    root: &Path,
    profile: inventory::PublicManifestProfile,
) -> Result<Value> {
    no_symlinks(root)?;
    let first = inventory::manifest_with_profile(root, profile)?;
    let files = first["files"].as_object().context("code_manifest_files")?;
    match profile {
        inventory::PublicManifestProfile::SourceReadmeDerived32 => {
            public_source_checks::check_files(root, files)?
        }
        inventory::PublicManifestProfile::CurrentDerived32
        | inventory::PublicManifestProfile::Legacy16 => {
            public_source_checks::check_files_with_profile(root, files, profile)?
        }
    }
    ensure!(
        first == inventory::manifest_with_profile(root, profile)?,
        "code_public_source_changed"
    );
    Ok(first)
}
fn manifest_profile(expected: &Value) -> Result<inventory::PublicManifestProfile> {
    use inventory::PublicManifestProfile::{CurrentDerived32, Legacy16, SourceReadmeDerived32};
    let object = expected
        .as_object()
        .context("code_manifest_profile_unknown")?;
    let base = [
        "schema",
        "files",
        "file_count",
        "total_bytes",
        "excluded_private_roots",
        "uncovered",
        "status",
    ];
    // Original manifests have seven keys (schema plus six payload fields).
    let legacy = object.len() == base.len() && base.iter().all(|k| object.contains_key(*k));
    let derived = base.iter().all(|k| object.contains_key(*k))
        && expected["resource_profile"] == oe_core::source_limits::PROFILE
        && expected["file_byte_limits"]
            == json!({"default":16*1024*1024,"generated/model.ttl":32*1024*1024});
    let previous_derived = object.len() == base.len() + 2 && derived;
    let public_sources = object.len() == base.len() + 3
        && derived
        && expected["public_sources"] == json!(["sources/README.md"]);
    ensure!(
        legacy || previous_derived || public_sources,
        "code_manifest_profile_unknown"
    );
    ensure!(
        expected["schema"] == "oev2.code-public-manifest/1"
            && expected["excluded_private_roots"] == json!(["var", "sources", ".git"])
            && expected["uncovered"]
                == json!([
                    "external Cargo source bytes and resolved transitive feature graph",
                    "native SDK/runtime/provider closure",
                    "unlisted compile environment"
                ])
            && expected["status"] == "bounded_public_identity",
        "code_manifest_profile_unknown"
    );
    let profile = if legacy {
        Legacy16
    } else if previous_derived {
        CurrentDerived32
    } else {
        SourceReadmeDerived32
    };
    let files = expected["files"]
        .as_object()
        .context("code_manifest_files")?;
    ensure!(
        files.len() <= 10_000 && expected["file_count"].as_u64() == Some(files.len() as u64),
        "code_manifest_file_budget"
    );
    let mut total = 0u64;
    for (path, id) in files {
        let file = id.as_object().context("code_manifest_file_shape")?;
        ensure!(
            file.len() == 3
                && ["sha256", "bytes", "mode"]
                    .iter()
                    .all(|k| file.contains_key(*k)),
            "code_manifest_file_shape"
        );
        let hash = id["sha256"].as_str().context("code_manifest_file_shape")?;
        ensure!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "code_manifest_file_shape"
        );
        let bytes = id["bytes"].as_u64().context("code_manifest_file_shape")?;
        ensure!(
            bytes <= profile.file_bytes(Path::new(path)),
            "code_manifest_file_budget"
        );
        ensure!(
            id["mode"].as_u64().is_some_and(|mode| mode <= 0o7777),
            "code_manifest_file_shape"
        );
        total = total
            .checked_add(bytes)
            .context("code_manifest_total_budget")?;
    }
    ensure!(
        total <= 256 * 1024 * 1024 && expected["total_bytes"].as_u64() == Some(total),
        "code_manifest_total_budget"
    );
    Ok(profile)
}
/// Re-observe a complete known historical/current domain without rewriting its identity.
/// Unknown shapes and caller-supplied resource limits never select an execution policy.
pub fn public_manifest_matching(root: &Path, expected: &Value) -> Result<Value> {
    let actual = public_manifest_with_profile(root, manifest_profile(expected)?)?;
    ensure!(actual == *expected, "code_public_manifest_mismatch");
    Ok(actual)
}
fn public_manifest_matching_digest(root: &Path, expected: &str) -> Result<Value> {
    use inventory::PublicManifestProfile::{CurrentDerived32, Legacy16, SourceReadmeDerived32};
    // Digest-only CodeBinaryRef has no profile field. Re-observe only these
    // three frozen domains, each with its own real traversal and resource guards.
    // A new public README can be invalid while remaining private in an old domain.
    for profile in [SourceReadmeDerived32, CurrentDerived32, Legacy16] {
        if let Ok(observed) = public_manifest_with_profile(root, profile)
            && digest(&observed)? == expected
        {
            return Ok(observed);
        }
    }
    anyhow::bail!("code_binary_source_changed")
}
pub fn binary_reference(path: &Path, source_root: &Path) -> Result<CodeBinaryRef> {
    no_symlinks(path)?;
    let id = fingerprint_file(path, 512 * 1024 * 1024)?;
    Ok(CodeBinaryRef {
        path: path
            .canonicalize()?
            .to_str()
            .context("code_non_utf8")?
            .into(),
        sha256: id.sha256,
        bytes: id.bytes,
        source_root: source_root
            .canonicalize()?
            .to_str()
            .context("code_non_utf8")?
            .into(),
        source_manifest_sha256: digest(&public_manifest(source_root)?)?,
    })
}
fn verify_binary_bytes(reference: &CodeBinaryRef) -> Result<()> {
    reference
        .validate()
        .map_err(|e| anyhow::anyhow!("code_binary_reference_invalid:{e:?}"))?;
    let id = fingerprint_file(Path::new(&reference.path), 512 * 1024 * 1024)?;
    ensure!(
        id.sha256 == reference.sha256 && id.bytes == reference.bytes,
        "code_executable_changed"
    );
    Ok(())
}
pub fn verify_binary(reference: &CodeBinaryRef) -> Result<()> {
    verify_binary_bytes(reference)?;
    public_manifest_matching_digest(
        Path::new(&reference.source_root),
        &reference.source_manifest_sha256,
    )?;
    Ok(())
}
fn verify_binary_with_manifest(reference: &CodeBinaryRef, expected: &Value) -> Result<()> {
    verify_binary_bytes(reference)?;
    ensure!(
        digest(expected)? == reference.source_manifest_sha256,
        "code_binary_source_changed"
    );
    public_manifest_matching(Path::new(&reference.source_root), expected)?;
    Ok(())
}
fn command(
    exe: &Path,
    args: Vec<std::ffi::OsString>,
    cwd: &Path,
    timeout: Duration,
    bytes: usize,
) -> CommandSpec {
    CommandSpec {
        executable: exe.into(),
        arguments: args,
        cwd: cwd.into(),
        environment: BTreeMap::new(),
        remove_environment: vec![
            "RUSTFLAGS".into(),
            "CARGO_ENCODED_RUSTFLAGS".into(),
            "RUSTC_WRAPPER".into(),
            "RUSTC_WORKSPACE_WRAPPER".into(),
        ],
        timeout,
        output_limit: bytes,
    }
}
fn probe(binary: &CodeBinaryRef, evidence: &Path) -> Result<Value> {
    probe_until(binary, evidence, None, 4 * 1024 * 1024)
}
fn probe_until(
    binary: &CodeBinaryRef,
    evidence: &Path,
    deadline: Option<Instant>,
    output_limit: u64,
) -> Result<Value> {
    let remaining = deadline
        .map(|d| d.saturating_duration_since(Instant::now()))
        .unwrap_or(Duration::from_secs(10))
        .min(Duration::from_secs(10));
    ensure!(!remaining.is_zero(), "code_build_total_deadline_exhausted");
    let observation = identity_process(
        binary,
        &CodeBudget {
            timeout_ms: u64::try_from(remaining.as_millis())?,
            output_bytes: output_limit,
        },
        evidence,
    )?;
    ensure!(
        observation.status == "completed" && observation.exit_code == Some(0),
        "code_identity_probe_failed"
    );
    let value = strict_json(&observation.stdout)?;
    ensure!(
        value["manifest_sha256"] == binary.source_manifest_sha256
            && digest(&value["manifest"])? == binary.source_manifest_sha256,
        "code_runner_compiled_source_mismatch"
    );
    Ok(value)
}
pub(crate) fn identity_process(
    binary: &CodeBinaryRef,
    budget: &CodeBudget,
    evidence: &Path,
) -> Result<oe_core::process::ProcessObservation> {
    budget
        .validate()
        .map_err(|e| anyhow::anyhow!("code_identity_budget:{e:?}"))?;
    ensure!(
        budget.timeout_ms > 0
            && budget.timeout_ms <= 30_000
            && budget.output_bytes > 0
            && budget.output_bytes <= 4 * 1024 * 1024,
        "code_identity_budget"
    );
    verify_binary(binary)?;
    let observation = run_command(
        &command(
            Path::new(&binary.path),
            vec!["identity".into()],
            Path::new(&binary.source_root),
            Duration::from_millis(budget.timeout_ms),
            usize::try_from(budget.output_bytes)?,
        ),
        evidence,
    )?;
    verify_binary(binary)?;
    Ok(observation)
}
pub fn verify_artifact(artifact: &CodeSourceArtifact) -> Result<()> {
    artifact
        .validate()
        .map_err(|e| anyhow::anyhow!("code_artifact_invalid:{e:?}"))?;
    ensure!(
        digest(&artifact.manifest)? == artifact.manifest_sha256,
        "code_artifact_manifest_digest"
    );
    ensure!(
        artifact.runner.source_root == artifact.source_root
            && artifact.runner.source_manifest_sha256 == artifact.manifest_sha256
            && artifact.compiled["manifest"] == artifact.manifest
            && artifact.compiled["manifest_sha256"] == artifact.manifest_sha256,
        "code_artifact_compiled_source_mismatch"
    );
    ensure!(
        artifact.compiled["target"] == artifact.platform,
        "code_artifact_platform_changed"
    );
    generation_artifact::verify(artifact)?;
    verify_binary_with_manifest(&artifact.runner, &artifact.manifest)?;
    for tool in &artifact.tools {
        verify_binary(tool)?;
    }
    Ok(())
}
pub fn build(request: &CodeBuildRequest) -> Result<CodeSourceArtifact> {
    build_with_deadline_mode(request, None, 4 * 1024 * 1024, BuildMode::Legacy)
}
/// Explicit budget profile. Legacy build() above remains unchanged.
/// The caller (existing owner) has already removed its elapsed time/cleanup
/// reserve; no caller-provided request grants build-origin or eligibility.
pub fn build_budgeted(input: &CodeBudgetedBuildRequest) -> Result<CodeSourceArtifact> {
    let started = Instant::now();
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("code_budgeted_build_request:{e:?}"))?;
    ensure!(
        input.schema == "oev2.code-budgeted-build-request/1",
        "code_budgeted_build_schema"
    );
    ensure!(
        (1..=3_600_000).contains(&input.budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&input.budget.output_bytes),
        "code_budgeted_build_budget"
    );
    let deadline = started
        .checked_add(Duration::from_millis(input.budget.timeout_ms))
        .context("code_budgeted_build_deadline")?;
    let result = build_with_deadline_mode(
        &input.build,
        Some(deadline),
        usize::try_from(input.budget.output_bytes)?,
        BuildMode::Legacy,
    )?;
    ensure!(
        Instant::now() < deadline,
        "code_build_total_deadline_exhausted"
    );
    Ok(result)
}

// Preserve the private legacy entry used by profile/1 reproduction.
pub(crate) fn build_with_deadline(
    request: &CodeBuildRequest,
    deadline: Option<Instant>,
    output_limit: usize,
) -> Result<CodeSourceArtifact> {
    build_with_deadline_mode(request, deadline, output_limit, BuildMode::Legacy)
}

/// One pipeline owns the original five roles. Native mode only adds the
/// explicitly selected compiler environment and its independently checked facts.
pub(crate) enum BuildMode<'a> {
    Legacy,
    RuntimeReproduction2(&'a runtime_reproduction2::BuildSession2),
    Native(&'a native_runtime::NativeBuildSession),
    Native3(&'a native_runtime3::NativeBuildSession3),
    Native6(&'a native_runtime6::NativeBuildSession6),
    Native7(&'a native_runtime6::NativeBuildSession6),
    Native8(&'a native_runtime6::NativeBuildSession6),
    Native9(&'a native_runtime6::NativeBuildSession6),
    Native10(&'a native_runtime6::NativeBuildSession6),
    Native11(&'a native_runtime6::NativeBuildSession6),
}
pub(crate) struct PreparedNormalizer {
    normalizer: CodeBinaryRef,
    tools: Vec<CodeBinaryRef>,
    driver: CodeBinaryRef,
    source: Value,
}
enum BuildResult {
    Full(CodeSourceArtifact),
    Normalizer(PreparedNormalizer),
}
pub(crate) fn prepare_normalizer_with_deadline(
    request: &CodeBuildRequest,
    session: &native_runtime6::NativeBuildSession6,
) -> Result<PreparedNormalizer> {
    match build_to_boundary(
        request,
        Some(session.deadline),
        session.output_limit,
        BuildMode::Native11(session),
        true,
    )? {
        BuildResult::Normalizer(value) => Ok(value),
        BuildResult::Full(_) => anyhow::bail!("normalizer_preparation_wrong_boundary"),
    }
}
pub(crate) fn build_with_deadline_mode(
    request: &CodeBuildRequest,
    deadline: Option<Instant>,
    output_limit: usize,
    mode: BuildMode<'_>,
) -> Result<CodeSourceArtifact> {
    match build_to_boundary(request, deadline, output_limit, mode, false)? {
        BuildResult::Full(value) => Ok(value),
        BuildResult::Normalizer(_) => anyhow::bail!("full_build_wrong_boundary"),
    }
}
fn build_to_boundary(
    request: &CodeBuildRequest,
    deadline: Option<Instant>,
    output_limit: usize,
    mode: BuildMode<'_>,
    prepare_only: bool,
) -> Result<BuildResult> {
    let timeout = |maximum: Duration| -> Result<Duration> {
        let remaining = deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(maximum);
        ensure!(!remaining.is_zero(), "code_build_total_deadline_exhausted");
        Ok(maximum.min(remaining))
    };
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("code_build_request:{e:?}"))?;
    let driver_root = &crate::driver_root().context("code_driver_root")?;
    let driver_manifest = public_manifest(driver_root)?;
    ensure!(
        digest(&driver_manifest)? == compiled()?["manifest_sha256"],
        "code_build_driver_source_mismatch"
    );
    let driver_executable = fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)?;
    let root = SkillRoot::new(&request.source_root)?;
    let evidence = root.new_private_dir(Path::new(&request.evidence_path))?;
    let source_before = public_manifest(root.path())?;
    let tools = [
        (&request.cargo_path, "cargo"),
        (&request.rustc_path, "rustc"),
        (&request.ggen_path, "ggen"),
    ]
    .into_iter()
    .map(|(p, _)| binary_reference(Path::new(p), root.path()))
    .collect::<Result<Vec<_>>>()?;
    let require_tool_bytes = || -> Result<()> {
        for tool in &tools {
            let current = fingerprint_file(Path::new(&tool.path), 512 * 1024 * 1024)?;
            ensure!(
                current.sha256 == tool.sha256 && current.bytes == tool.bytes,
                "code_build_tool_changed"
            );
        }
        Ok(())
    };
    let mut processes = Vec::new();
    let invoke = |args: Vec<std::ffi::OsString>, dir: &Path| -> Result<Value> {
        require_tool_bytes()?;
        let mut spec = command(
            Path::new(&request.cargo_path),
            args,
            root.path(),
            timeout(Duration::from_secs(1200))?,
            output_limit,
        );
        spec.environment
            .insert("CARGO_HOME".into(), request.cargo_home.clone().into());
        spec.environment
            .insert("RUSTC".into(), request.rustc_path.clone().into());
        spec.environment
            .insert("CARGO_TARGET_DIR".into(), request.target_dir.clone().into());
        if let BuildMode::Native11(session) = &mode {
            let role = dir
                .file_name()
                .and_then(|v| v.to_str())
                .context("native_cargo_role")?;
            // This adapter configures the original SDK8/probe10 stage exactly
            // once, borrows Session6's deadline and retains complete raw bytes.
            let (_, report) =
                native_stream11::capture_borrowed_cargo_stage11(session, &mut spec, role)?;
            require_tool_bytes()?;
            ensure!(
                report["status"] == "completed" && report["exit_code"] == 0,
                "code_artifact_build_failed"
            );
            return Ok(report);
        }
        if !matches!(mode, BuildMode::Legacy) {
            let role = dir
                .file_name()
                .and_then(|v| v.to_str())
                .context("native_cargo_role")?;
            match &mode {
                BuildMode::Native(session) => session.configure_cargo(&mut spec, role)?,
                BuildMode::Native3(session) => session.configure_cargo(&mut spec, role)?,
                BuildMode::Native6(session) => session.configure_cargo(&mut spec, role)?,
                BuildMode::Native7(session) => session.configure_cargo(&mut spec, role)?,
                BuildMode::Native8(session) => {
                    native_runtime8::configure_cargo(session, &mut spec, role)?
                }
                BuildMode::Native9(session) => {
                    native_runtime9::configure_cargo(session, &mut spec, role)?
                }
                BuildMode::Native10(session) => {
                    native_runtime10::configure_cargo(session, &mut spec, role)?
                }
                BuildMode::RuntimeReproduction2(session) => {
                    session.configure_cargo(&mut spec, role)?
                }
                BuildMode::Legacy | BuildMode::Native11(_) => unreachable!(),
            }
            // Configuration and all pin reads consume the same body budget.
            spec.timeout = spec.timeout.min(timeout(Duration::from_secs(1200))?);
        }
        let observed = run_command(&spec, dir)?;
        require_tool_bytes()?;
        ensure!(
            observed.status == "completed" && observed.exit_code == Some(0),
            "code_artifact_build_failed"
        );
        Ok(observed.report)
    };
    let (normalizer, normalizer_process) = normalizer_stage::build(
        root.path(),
        &source_before,
        Path::new(&request.target_dir),
        &evidence,
        &invoke,
    )?;
    processes.push(normalizer_process);
    let bootstrap = Path::new(&request.target_dir).join("debug/oev2-design-bootstrap");
    require_tool_bytes()?;
    if prepare_only {
        ensure!(
            matches!(mode, BuildMode::Native11(_)),
            "normalizer_preparation_native11_only"
        );
        ensure!(
            public_manifest(driver_root)? == driver_manifest
                && fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)?
                    == driver_executable,
            "code_build_driver_changed"
        );
        let driver = binary_reference(&std::env::current_exe()?, driver_root)?;
        ensure!(
            driver.sha256 == driver_executable.sha256
                && driver.source_manifest_sha256 == digest(&driver_manifest)?,
            "code_build_driver_changed"
        );
        ensure!(
            public_manifest(root.path())? == source_before,
            "code_source_changed_during_normalizer_build"
        );
        timeout(Duration::from_secs(1200))?;
        return Ok(BuildResult::Normalizer(PreparedNormalizer {
            normalizer,
            tools,
            driver,
            source: source_before,
        }));
    }
    let generation = run_command(
        &command(
            &bootstrap,
            vec![
                "generate".into(),
                root.path().as_os_str().into(),
                request.ggen_path.clone().into(),
            ],
            root.path(),
            timeout(Duration::from_secs(300))?,
            output_limit,
        ),
        &evidence.join("actual-ggen"),
    )?;
    ensure!(
        generation.status == "completed" && generation.exit_code == Some(0),
        "code_actual_ggen_failed"
    );
    require_tool_bytes()?;
    let generation_receipt = strict_json(&generation.stdout)?;
    processes.push(generation.report);
    // Only generated/ may change in the generation step.
    let after_generation = public_manifest(root.path())?;
    for (path, identity) in source_before["files"]
        .as_object()
        .context("code_manifest_files")?
    {
        if !path.starts_with("generated/") {
            ensure!(
                after_generation["files"][path] == *identity,
                "code_source_changed_during_generation:{path}"
            );
        }
    }
    let before_keys: BTreeSet<_> = source_before["files"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|p| !p.starts_with("generated/"))
        .collect();
    let after_keys: BTreeSet<_> = after_generation["files"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|p| !p.starts_with("generated/"))
        .collect();
    ensure!(
        before_keys == after_keys,
        "code_source_set_changed_during_generation"
    );
    processes.push(invoke(
        vec![
            "build".into(),
            "--locked".into(),
            "--offline".into(),
            "-j4".into(),
            "--target".into(),
            request.platform.clone().into(),
            "-p".into(),
            "oe-code-runner".into(),
        ],
        &evidence.join("build-runner"),
    )?);
    require_tool_bytes()?;
    let drift = run_command(
        &command(
            &bootstrap,
            vec![
                "check".into(),
                root.path().as_os_str().into(),
                request.ggen_path.clone().into(),
            ],
            root.path(),
            timeout(Duration::from_secs(300))?,
            output_limit,
        ),
        &evidence.join("actual-ggen-check"),
    )?;
    ensure!(
        drift.status == "completed" && drift.exit_code == Some(0),
        "code_actual_generated_drift"
    );
    require_tool_bytes()?;
    let drift_report = strict_json(&drift.stdout)?;
    ensure!(
        drift_report["status"] == "consistent",
        "code_actual_generated_drift"
    );
    processes.push(drift.report);
    let final_manifest = public_manifest(root.path())?;
    ensure!(
        final_manifest == after_generation,
        "code_source_changed_during_runner_build"
    );
    let runner = binary_reference(
        &Path::new(&request.target_dir)
            .join(&request.platform)
            .join("debug/oe-code-runner"),
        root.path(),
    )?;
    let compiled = probe_until(
        &runner,
        &evidence.join("runner-identity"),
        deadline,
        output_limit as u64,
    )?;
    // Tool byte identity is frozen across build, while their source references
    // now bind the final generated source of this side.
    let mut bound_tools = Vec::new();
    for tool in tools {
        let current = binary_reference(Path::new(&tool.path), root.path())?;
        ensure!(
            current.sha256 == tool.sha256 && current.bytes == tool.bytes,
            "code_build_tool_changed"
        );
        bound_tools.push(current);
    }
    bound_tools.push(binary_reference(&bootstrap, root.path())?);
    let mut artifact = CodeSourceArtifact {
        artifact_id: request.artifact_id.clone(),
        source_root: root.path().to_str().context("code_non_utf8")?.into(),
        manifest_sha256: digest(&final_manifest)?,
        manifest: final_manifest,
        runner,
        compiled,
        generation: json!({"receipt":generation_receipt,"check":drift_report,
            "normalizer_executable_sha256":normalizer.sha256,"normalizer_path":bootstrap,"source_before":source_before}),
        tools: bound_tools,
        build_processes: processes,
        profile: "local-semantic-replay/1".into(),
        platform: request.platform.clone(),
        features: vec![],
        uncovered: vec![
            "external crate cache/native closure not fully frozen".into(),
            "development synthetic replay only".into(),
        ],
    };
    artifact.features = serde_json::from_value(artifact.compiled["features"].clone())?;
    ensure!(
        public_manifest(driver_root)? == driver_manifest
            && fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)? == driver_executable,
        "code_build_driver_changed"
    );
    artifact.generation["fixed_build_driver"] = json!({"executable_sha256":driver_executable.sha256,"source_manifest_sha256":digest(&driver_manifest)?,
        "source_guard":"own compiled driver root; independently built side normalizer guards each side"});
    match &mode {
        BuildMode::Native(session) => session.bind_artifact(&mut artifact)?,
        BuildMode::Native3(session) => session.bind_artifact(&mut artifact)?,
        BuildMode::Native6(session) => session.bind_artifact(&mut artifact)?,
        BuildMode::Native7(session) => native_runtime7::bind_artifact(session, &mut artifact)?,
        BuildMode::Native8(session) => native_runtime8::bind_artifact(session, &mut artifact)?,
        BuildMode::Native9(session) => native_runtime9::bind_artifact(session, &mut artifact)?,
        BuildMode::Native10(session) => native_runtime10::bind_artifact(session, &mut artifact)?,
        BuildMode::Native11(session) => native_runtime11::bind_artifact(session, &mut artifact)?,
        BuildMode::RuntimeReproduction2(session) => session.bind_artifact(&mut artifact)?,
        BuildMode::Legacy => {}
    }
    verify_artifact(&artifact)?;
    write_new(
        &evidence.join("artifact.json"),
        &serde_json::to_value(&artifact)?,
    )?;
    Ok(BuildResult::Full(artifact))
}
pub fn invocation(case: &CodeReplayCase, run_id: u64) -> Result<CodeReplayInvocation> {
    case.validate()
        .map_err(|e| anyhow::anyhow!("code_case_invalid:{e:?}"))?;
    ensure!(
        (1..=60_000).contains(&case.budget.timeout_ms)
            && (1024..=16 * 1024 * 1024).contains(&case.budget.output_bytes)
            && (1..=16).contains(&case.repeats)
            && run_id < case.repeats
            && !case.checks.is_empty(),
        "code_case_budget_or_checks"
    );
    ensure!(
        case.case_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
            && case.case_id != "."
            && case.case_id != "..",
        "code_case_id_unsafe"
    );
    ensure!(
        digest(&case.input)? == case.input_sha256,
        "code_case_input_changed"
    );
    Ok(CodeReplayInvocation {
        schema: "oev2.code-replay-invocation/1".into(),
        case_id: case.case_id.clone(),
        capability: case.capability.clone(),
        profile: case.profile.clone(),
        input: case.input.clone(),
        input_sha256: case.input_sha256.clone(),
        conditions_sha256: case.conditions_sha256.clone(),
        budget: case.budget.clone(),
        seed: case.seed,
        run_id,
    })
}
pub fn replay(request: &CodeReplayRequest, side: &str) -> Result<CodeReplayObservation> {
    ensure!(["baseline", "candidate"].contains(&side), "code_side");
    verify_artifact(&request.artifact)?;
    request
        .invocation
        .validate()
        .map_err(|e| anyhow::anyhow!("code_invocation_invalid:{e:?}"))?;
    ensure!(
        (1..=60_000).contains(&request.invocation.budget.timeout_ms)
            && (1024..=16 * 1024 * 1024).contains(&request.invocation.budget.output_bytes)
            && digest(&request.invocation.input)? == request.invocation.input_sha256,
        "code_replay_budget_or_input_changed"
    );
    let root = SkillRoot::new(&request.artifact.source_root)?;
    let evidence = root.new_private_dir(Path::new(&request.evidence_path))?;
    let data = canonical_json(&serde_json::to_value(&request.invocation)?)?;
    let input = evidence.join("invocation.json");
    write_new_bytes(&input, &data)?;
    let started = Instant::now();
    let process = run_command(
        &command(
            Path::new(&request.artifact.runner.path),
            vec!["run".into(), input.as_os_str().into()],
            root.path(),
            Duration::from_millis(request.invocation.budget.timeout_ms),
            request.invocation.budget.output_bytes.try_into()?,
        ),
        &evidence.join("process"),
    )?;
    let elapsed_micros = started.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
    ensure!(
        read_regular_bytes(&input, 1024 * 1024)? == data,
        "code_replay_input_changed"
    );
    verify_artifact(&request.artifact)?;
    let observed: Option<CodeRunnerObservation> =
        if process.status == "completed" && process.exit_code == Some(0) {
            strict_json(&process.stdout)
                .ok()
                .and_then(|v| serde_json::from_value::<CodeRunnerObservation>(v).ok())
        } else {
            None
        };
    let valid = observed.as_ref().is_some_and(|x| {
        x.validate().is_ok()
            && x.case_id == request.invocation.case_id
            && x.input_sha256 == request.invocation.input_sha256
            && x.conditions_sha256 == request.invocation.conditions_sha256
            && x.compiled_manifest_sha256 == request.artifact.manifest_sha256
    });
    let execution = if valid {
        "completed"
    } else if ["completed", "process_failed"].contains(&process.status.as_str()) {
        "failed"
    } else {
        "partial"
    };
    Ok(CodeReplayObservation {
        side: side.into(),
        case_id: request.invocation.case_id.clone(),
        run_id: request.invocation.run_id,
        invocation_sha256: sha256(&data),
        runner_sha256: request.artifact.runner.sha256.clone(),
        artifact_manifest_sha256: request.artifact.manifest_sha256.clone(),
        execution: execution.into(),
        observed: if valid { observed } else { None },
        process: process.report,
        elapsed_micros: Some(elapsed_micros),
        stdout_bytes: Some(process.stdout.len() as u64),
        stderr_bytes: Some(process.stderr.len() as u64),
        usage: None,
        cost: None,
        unknown: vec!["provider_usage_and_total_engineering_cost_unknown".into()],
    })
}
pub fn owner_connection(operation: &str) -> Result<CodeOwnerConnection> {
    ensure!(
        ["select", "use", "feedback", "recover"].contains(&operation),
        "code_owner_operation"
    );
    Ok(CodeOwnerConnection{operation:operation.into(),status:"not_connected".into(),
        original_owner:"architecture-closure-20261002-01/final-workspace/oe-engineering::registry and Project".into(),
        limitation:"Existing owner supports operation methods and semantic bundles; no whole-code activation/use/recovery adapter in this source".into()})
}
pub struct FixedExperiment {
    experiment: CodeExperiment,
    experiment_sha256: String,
}
impl FixedExperiment {
    pub fn freeze(experiment: CodeExperiment) -> Result<Self> {
        experiment
            .validate()
            .map_err(|e| anyhow::anyhow!("code_experiment_invalid:{e:?}"))?;
        let bytes = read_regular_bytes(Path::new(&experiment.oracle_path), 4 * 1024 * 1024)?;
        ensure!(
            sha256(&bytes) == experiment.oracle_sha256,
            "code_oracle_changed"
        );
        let cases: Vec<CodeReplayCase> = serde_json::from_value(strict_json(&bytes)?)?;
        ensure!(
            cases == experiment.cases
                && digest(&serde_json::to_value(&cases)?)? == experiment.case_set_sha256,
            "code_case_set_changed"
        );
        ensure!(!cases.is_empty() && cases.len() <= 128, "code_case_count");
        let mut ids = BTreeSet::new();
        for case in &cases {
            ensure!(ids.insert(&case.case_id), "code_duplicate_case");
            ensure!(
                case.conditions_sha256 == experiment.conditions_sha256,
                "code_case_conditions_changed"
            );
            invocation(case, 0)?;
        }
        ensure!(
            experiment.protocol_case_ids.len() == 19
                && experiment
                    .protocol_case_ids
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 19,
            "code_protocol_fixed_denominator"
        );
        ensure!(
            experiment.baseline.profile == experiment.candidate.profile
                && experiment.baseline.platform == experiment.candidate.platform
                && experiment.baseline.features == experiment.candidate.features
                && experiment.baseline.compiled["compiler"]
                    == experiment.candidate.compiled["compiler"]
                && experiment.baseline.compiled["rustflags_sha256"]
                    == experiment.candidate.compiled["rustflags_sha256"],
            "code_artifact_conditions_differ"
        );
        ensure!(
            experiment.baseline.compiled["compiler"] == compiled()?["compiler"]
                && experiment.baseline.compiled["target"] == compiled()?["target"],
            "code_fixed_compiler_or_platform_changed"
        );
        ensure!(
            experiment.driver.source_root != experiment.baseline.source_root
                && experiment.driver.source_root != experiment.candidate.source_root,
            "code_driver_source_must_be_independent"
        );
        let oracle = Path::new(&experiment.oracle_path).canonicalize()?;
        for artifact in [&experiment.baseline, &experiment.candidate] {
            ensure!(
                !oracle.starts_with(Path::new(&artifact.source_root)),
                "code_oracle_shared_with_runner_root"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                std::fs::metadata(&oracle)?.permissions().mode() & 0o222 == 0
                    && std::fs::metadata(oracle.parent().context("code_oracle_parent")?)?
                        .permissions()
                        .mode()
                        & 0o222
                        == 0,
                "code_oracle_directory_writable"
            );
        }
        for binary in [
            &experiment.baseline.runner,
            &experiment.candidate.runner,
            &experiment.evaluator,
        ] {
            let root = SkillRoot::new(&binary.source_root)?;
            let evidence = root.path().join(format!(
                "var/code-identity-probes/{}-{}",
                std::process::id(),
                PROBE_SERIAL.fetch_add(1, Ordering::SeqCst)
            ));
            probe(binary, &evidence)?;
        }
        let experiment_sha256 = digest(&serde_json::to_value(&experiment)?)?;
        let result = Self {
            experiment,
            experiment_sha256,
        };
        result.require()?;
        Ok(result)
    }
    pub fn identity(&self) -> &str {
        &self.experiment_sha256
    }
    pub fn experiment(&self) -> &CodeExperiment {
        &self.experiment
    }
    pub fn require(&self) -> Result<()> {
        ensure!(
            digest(&serde_json::to_value(&self.experiment)?)? == self.experiment_sha256,
            "code_experiment_changed"
        );
        verify_binary(&self.experiment.driver)?;
        ensure!(
            fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)?.sha256
                == self.experiment.driver.sha256
                && compiled()?["manifest_sha256"] == self.experiment.driver.source_manifest_sha256,
            "code_fixed_driver_identity_mismatch"
        );
        verify_binary(&self.experiment.evaluator)?;
        ensure!(
            sha256(read_regular_bytes(
                Path::new(&self.experiment.oracle_path),
                4 * 1024 * 1024
            )?) == self.experiment.oracle_sha256,
            "code_oracle_changed"
        );
        ensure!(
            sha256(read_regular_bytes(
                Path::new(&self.experiment.protocol_expectations_path),
                4 * 1024 * 1024
            )?) == self.experiment.protocol_expectations_sha256,
            "code_protocol_expectations_changed"
        );
        ensure!(
            sha256(read_regular_bytes(
                &Path::new(&self.experiment.driver.source_root)
                    .join("runtime/semantica-scope.json"),
                16 * 1024 * 1024
            )?) == self.experiment.scope_sha256,
            "code_full_scope_changed"
        );
        verify_artifact(&self.experiment.baseline)?;
        verify_artifact(&self.experiment.candidate)?;
        Ok(())
    }
    pub fn run(&self, output: &Path) -> Result<CodeComparison> {
        self.require()?;
        let root = SkillRoot::new(&self.experiment.driver.source_root)?;
        let folder = root.new_private_dir(output)?;
        let mut observations = Vec::new();
        for case in &self.experiment.cases {
            for run_id in 0..case.repeats {
                let invocation = invocation(case, run_id)?;
                for (side, artifact) in [
                    ("baseline", &self.experiment.baseline),
                    ("candidate", &self.experiment.candidate),
                ] {
                    self.require()?;
                    let request = CodeReplayRequest {
                        artifact: artifact.clone(),
                        invocation: invocation.clone(),
                        evidence_path: format!(
                            "var/code-replays/{}/{side}/{}/{run_id}",
                            folder
                                .file_name()
                                .context("code_run_name")?
                                .to_str()
                                .context("code_run_name")?,
                            case.case_id
                        ),
                    };
                    let observation = match replay(&request, side) {
                        Ok(value) => value,
                        Err(error) => CodeReplayObservation {
                            side: side.into(),
                            case_id: case.case_id.clone(),
                            run_id,
                            invocation_sha256: digest(&serde_json::to_value(&invocation)?)?,
                            runner_sha256: artifact.runner.sha256.clone(),
                            artifact_manifest_sha256: artifact.manifest_sha256.clone(),
                            execution: "failed".into(),
                            observed: None,
                            process: json!({"driver_error":error.to_string()}),
                            elapsed_micros: None,
                            stdout_bytes: None,
                            stderr_bytes: None,
                            usage: None,
                            cost: None,
                            unknown: vec!["process_not_completed; resources unmeasured".into()],
                        },
                    };
                    observations.push(observation);
                    self.require()?;
                }
            }
        }
        self.compare(
            &CodeComparisonRequest {
                experiment: self.experiment.clone(),
                observations,
            },
            &folder,
        )
    }
    pub fn compare(
        &self,
        request: &CodeComparisonRequest,
        folder: &Path,
    ) -> Result<CodeComparison> {
        self.require()?;
        ensure!(
            request.experiment == self.experiment,
            "code_comparison_experiment_changed"
        );
        let input = folder.join("comparison-input.json");
        write_new(&input, &serde_json::to_value(request)?)?;
        let process = run_command(
            &command(
                Path::new(&self.experiment.evaluator.path),
                vec!["compare".into(), input.as_os_str().into()],
                Path::new(&self.experiment.evaluator.source_root),
                Duration::from_secs(30),
                4 * 1024 * 1024,
            ),
            &folder.join("fixed-evaluator"),
        )?;
        self.require()?;
        ensure!(
            process.status == "completed" && process.exit_code == Some(0),
            "code_fixed_evaluation_failed"
        );
        let result: CodeComparison = serde_json::from_value(strict_json(&process.stdout)?)?;
        result
            .validate()
            .map_err(|e| anyhow::anyhow!("code_comparison_invalid:{e:?}"))?;
        ensure!(
            result.driver_sha256 == self.experiment.driver.sha256
                && result.evaluator_sha256 == self.experiment.evaluator.sha256
                && result.oracle_sha256 == self.experiment.oracle_sha256
                && result.baseline_sha256 == self.experiment.baseline.manifest_sha256
                && result.candidate_sha256 == self.experiment.candidate.manifest_sha256,
            "code_comparison_identity_changed"
        );
        write_new(
            &folder.join("comparison.json"),
            &serde_json::to_value(&result)?,
        )?;
        Ok(result)
    }
}
pub struct ArtifactBuilder;
impl CodeArtifactBuilder for ArtifactBuilder {
    fn build(
        &self,
        input: &CodeBuildRequest,
    ) -> std::result::Result<CodeSourceArtifact, CodeEvolutionFailure> {
        build(input).map_err(|e| CodeEvolutionFailure {
            code: "code_build_failure".into(),
            message: e.to_string(),
        })
    }
}
impl CodeBudgetedArtifactBuilder for ArtifactBuilder {
    fn build_budgeted(
        &self,
        input: &CodeBudgetedBuildRequest,
    ) -> std::result::Result<CodeSourceArtifact, CodeEvolutionFailure> {
        build_budgeted(input).map_err(|e| CodeEvolutionFailure {
            code: "code_budgeted_build_failure".into(),
            message: e.to_string(),
        })
    }
}
pub struct ReplayExecutor {
    pub side: String,
}
impl CodeReplayPort for ReplayExecutor {
    fn run(
        &self,
        input: &CodeReplayRequest,
    ) -> std::result::Result<CodeReplayObservation, CodeEvolutionFailure> {
        replay(input, &self.side).map_err(|e| CodeEvolutionFailure {
            code: "code_replay_failure".into(),
            message: e.to_string(),
        })
    }
}
pub struct ComparisonExecutor<'a> {
    pub fixed: &'a FixedExperiment,
    pub evidence_path: PathBuf,
}
impl FixedCodeComparisonPort for ComparisonExecutor<'_> {
    fn compare(
        &self,
        input: &CodeComparisonRequest,
    ) -> std::result::Result<CodeComparison, CodeEvolutionFailure> {
        (|| {
            oe_core::create_new_private_dir(&self.evidence_path)?;
            self.fixed.compare(input, &self.evidence_path)
        })()
        .map_err(|e| CodeEvolutionFailure {
            code: "code_fixed_comparison_failure".into(),
            message: e.to_string(),
        })
    }
}
pub mod scope_witness;

pub mod native_archive;

pub mod native_profile3;

/// Diagnostic content only; no execution dispatch or loader authorization.
pub mod native_runtime5_content;

mod native_link6;
mod native_link7;
pub mod native_loader6;
pub mod native_profile6;
pub mod native_runtime6;
pub mod native_runtime7;
pub mod native_sdk4;
mod native_sources6;

mod native_process6;
pub mod native_sdkroot6;

mod native_buildscript6;
mod native_producer9;

mod native_pin_checks;
