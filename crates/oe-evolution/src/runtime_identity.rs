//! Explicit Rust host observations using the existing inventory, process and builder.
//! This module has no Project, registry, credentials or behavior-coverage authority.
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{fingerprint_file, read_regular_bytes, write_new};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::Path,
    time::{Duration, Instant},
};

pub const PROFILE: &str = "semantica.rust-runtime-source-bound/1";
const LIMIT: usize = 16 * 1024 * 1024;
const STAGES: [&str; 5] = [
    "build-normalizer",
    "actual-ggen",
    "build-runner",
    "actual-ggen-check",
    "runner-identity",
];

fn reference(root: &SkillRoot, file: &Path) -> Result<ArtifactReference> {
    let path = root.private_path(file)?;
    let id = fingerprint_file(&path, LIMIT as u64)?;
    Ok(ArtifactReference {
        path: path.to_str().context("rust_runtime_non_utf8")?.into(),
        sha256: id.sha256,
    })
}
fn read(root: &SkillRoot, r: &ArtifactReference) -> Result<Value> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_reference:{e:?}"))?;
    let path = root.private_path(Path::new(&r.path))?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(sha256(&bytes) == r.sha256, "rust_runtime_reference_changed");
    Ok(strict_json(&bytes)?)
}
fn budget(b: &CodeBudget, maximum: u64) -> Result<Instant> {
    b.validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_budget:{e:?}"))?;
    ensure!(
        b.timeout_ms > 0
            && b.timeout_ms <= maximum
            && b.output_bytes > 0
            && b.output_bytes <= 4 * 1024 * 1024,
        "rust_runtime_budget"
    );
    Instant::now()
        .checked_add(Duration::from_millis(b.timeout_ms))
        .context("rust_runtime_deadline_overflow")
}
fn profile(root: &SkillRoot, r: &ArtifactReference) -> Result<(CodeRustRuntimeProfile, Value)> {
    let raw = read(root, r)?;
    let p: CodeRustRuntimeProfile = serde_json::from_value(raw)?;
    p.validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_profile:{e:?}"))?;
    ensure!(
        p.schema == "oev2.rust-runtime-profile/1" && p.profile == PROFILE && p.language == "rust",
        "rust_runtime_profile_unsupported"
    );
    let stages = read(root, &p.stage_expectations)?;
    ensure!(
        stages == json!({"schema":"oev2.rust-runtime-stages/1","stages":STAGES}),
        "rust_runtime_stage_expectations_changed"
    );
    ensure!(
        read(root, &p.network_policy)?
            == json!({"schema":"oev2.rust-runtime-network/1","cargo":"locked_offline","model_assets":"no_fetch","providers":"not_run"}),
        "rust_runtime_network_policy_unsupported"
    );
    let wanted = read(root, &p.source_manifest)?;
    ensure!(
        wanted == public_manifest(root.path())?,
        "rust_runtime_source_manifest_mismatch"
    );
    let package: toml::Value = toml::from_str(std::str::from_utf8(&read_regular_bytes(
        &root.path().join("Cargo.toml"),
        LIMIT,
    )?)?)?;
    ensure!(
        package
            .get("workspace")
            .and_then(|v| v.get("package"))
            .and_then(|v| v.get("version"))
            .and_then(toml::Value::as_str)
            == Some(p.expected_package_version.as_str()),
        "rust_runtime_source_version_mismatch"
    );
    if p.source_kind == "frozen_export" {
        ensure!(
            p.expected_git_commit.is_none() && p.git.is_none(),
            "rust_runtime_export_git_claim"
        );
    } else {
        let commit = p
            .expected_git_commit
            .as_deref()
            .context("rust_runtime_git_commit_missing")?;
        ensure!(
            commit.len() == 40
                && commit
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "rust_runtime_git_commit_invalid"
        );
        verify_binary(p.git.as_ref().context("rust_runtime_git_tool_missing")?)?;
    }
    Ok((p, wanted))
}
/// Reuse the same source/profile preflight at the unique version-owner boundary.
/// This validates bytes and conditions; it grants no compilation authority.
pub fn inspect_profile(
    root: &SkillRoot,
    r: &ArtifactReference,
) -> Result<(CodeRustRuntimeProfile, Value)> {
    profile(root, r)
}
fn git_source(
    root: &SkillRoot,
    p: &CodeRustRuntimeProfile,
    evidence: &Path,
    b: &CodeBudget,
    deadline: Instant,
) -> Result<Vec<ArtifactReference>> {
    if p.source_kind == "frozen_export" {
        return Ok(vec![]);
    }
    let tool = p.git.as_ref().context("rust_runtime_git_tool_missing")?;
    let mut records = vec![];
    let mut invoke = |label: &str, args: &[&str], absent: bool| -> Result<Vec<u8>> {
        verify_binary(tool)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero(),
            "rust_runtime_inspect_deadline_exhausted"
        );
        let dir = evidence.join(label);
        let observed = run_command(
            &CommandSpec {
                executable: tool.path.clone().into(),
                arguments: args.iter().map(OsString::from).collect(),
                cwd: root.path().into(),
                environment: BTreeMap::from([
                    ("GIT_OPTIONAL_LOCKS".into(), "0".into()),
                    ("GIT_NO_LAZY_FETCH".into(), "1".into()),
                    ("GIT_TERMINAL_PROMPT".into(), "0".into()),
                    ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
                    ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
                ]),
                remove_environment: vec![
                    "GIT_DIR".into(),
                    "GIT_WORK_TREE".into(),
                    "GIT_INDEX_FILE".into(),
                    "GIT_OBJECT_DIRECTORY".into(),
                    "GIT_ALTERNATE_OBJECT_DIRECTORIES".into(),
                ],
                timeout: remaining.min(Duration::from_secs(300)),
                output_limit: usize::try_from(b.output_bytes)?,
            },
            &dir,
        )?;
        records.push(reference(root, &dir.join("process.json"))?);
        let allowed_absent = absent
            && observed.status == "process_failed"
            && observed.exit_code == Some(1)
            && observed.stdout.is_empty()
            && observed.stderr.is_empty();
        ensure!(
            (observed.status == "completed" && observed.exit_code == Some(0)) || allowed_absent,
            "rust_runtime_git_process_failed:{label}"
        );
        verify_binary(tool)?;
        Ok(observed.stdout)
    };
    ensure!(
        Path::new(
            std::str::from_utf8(&invoke(
                "git-root",
                &["rev-parse", "--show-toplevel"],
                false
            )?)?
            .trim_end()
        ) == root.path(),
        "rust_runtime_git_root_mismatch"
    );
    ensure!(
        std::str::from_utf8(&invoke("git-head", &["rev-parse", "HEAD"], false)?)?.trim_end()
            == p.expected_git_commit.as_deref().unwrap(),
        "rust_runtime_git_commit_mismatch"
    );
    ensure!(
        invoke(
            "git-shallow",
            &["rev-parse", "--is-shallow-repository"],
            false
        )? == b"false\n",
        "rust_runtime_git_shallow"
    );
    let sparse = invoke(
        "git-sparse",
        &["config", "--bool", "--get", "core.sparseCheckout"],
        true,
    )?;
    ensure!(
        sparse.is_empty() || sparse == b"false\n",
        "rust_runtime_git_sparse"
    );
    ensure!(
        invoke(
            "git-partial",
            &[
                "config",
                "--get-regexp",
                "^(extensions\\.partialclone|remote\\..*\\.promisor)$"
            ],
            true
        )?
        .is_empty(),
        "rust_runtime_git_partial_clone"
    );
    ensure!(
        invoke(
            "git-status",
            &["status", "--porcelain", "--untracked-files=all"],
            false
        )?
        .is_empty(),
        "rust_runtime_git_dirty"
    );
    invoke("git-fsck", &["fsck", "--full", "--no-progress"], false)?;
    Ok(records)
}
pub fn inspect(r: &CodeRustRuntimeInspectRequest) -> Result<CodeRustRuntimeSourceObservation> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_inspect_request:{e:?}"))?;
    ensure!(
        r.schema == "oev2.rust-runtime-inspect-request/1",
        "rust_runtime_inspect_schema"
    );
    let deadline = budget(&r.budget, 300_000)?;
    let root = SkillRoot::new(&r.source_root)?;
    let (p, wanted) = profile(&root, &r.profile)?;
    ensure!(
        Instant::now() < deadline,
        "rust_runtime_inspect_deadline_exhausted"
    );
    root.private_path(Path::new(&r.evidence_path))?;
    let output = root.new_private_dir(Path::new(&r.evidence_path))?;
    let git = git_source(&root, &p, &output, &r.budget, deadline)?;
    ensure!(
        profile(&root, &r.profile)? == (p.clone(), wanted.clone()),
        "rust_runtime_inspect_inputs_changed"
    );
    ensure!(
        Instant::now() < deadline,
        "rust_runtime_inspect_deadline_exhausted"
    );
    let result = CodeRustRuntimeSourceObservation {
        schema: "oev2.rust-runtime-source-observation/1".into(),
        profile_sha256: r.profile.sha256.clone(),
        source_manifest_sha256: digest(&wanted)?,
        source_kind: p.source_kind,
        source_version: p.expected_package_version,
        identity: "bound".into(),
        proof: json!({"owned_public_manifest":wanted,"git_processes":git,"build_dispatched":false,"semantic_execution":"not_run"}),
        uncovered: vec![
            "External Cargo/feature/native closure and full13494 behavior are unknown".into(),
        ],
    };
    write_new(
        &output.join("source-observation.json"),
        &serde_json::to_value(&result)?,
    )?;
    Ok(result)
}
pub fn probe(r: &CodeRustRuntimeProbeRequest) -> Result<CodeRustRuntimeObservation> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_probe_request:{e:?}"))?;
    ensure!(
        r.schema == "oev2.rust-runtime-probe-request/1",
        "rust_runtime_probe_schema"
    );
    let deadline = budget(&r.budget, 30_000)?;
    let root = SkillRoot::new(&r.source_root)?;
    let (p, wanted) = profile(&root, &r.profile)?;
    let expected = p
        .expected_build_capture
        .as_ref()
        .context("rust_runtime_original_build_capture_missing")?;
    let a = verify_capture(&root, expected, &p, &r.artifact)?;
    ensure!(
        a.source_root == r.source_root && a.platform == p.expected_target && a.manifest == wanted,
        "rust_runtime_artifact_profile_mismatch"
    );
    let output = root.new_private_dir(Path::new(&r.evidence_path))?;
    let git_before = output.join("source-before");
    if p.source_kind == "git_checkout" {
        std::fs::create_dir(&git_before)?;
    }
    git_source(&root, &p, &git_before, &r.budget, deadline)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    ensure!(
        !remaining.is_zero(),
        "rust_runtime_probe_deadline_exhausted"
    );
    let capture: CodeRustRuntimeBuildCapture = serde_json::from_value(read(&root, expected)?)?;
    let producer = observe_identity(
        &capture.driver,
        &CodeBudget {
            timeout_ms: u64::try_from(remaining.as_millis())?.min(30000),
            output_bytes: r.budget.output_bytes,
        },
        &output.join("build-driver-identity"),
    )?;
    ensure!(
        producer.status == "completed"
            && producer.exit_code == Some(0)
            && producer.report["capture_complete"] == true,
        "rust_runtime_original_driver_identity_failed"
    );
    let producer_compiled = strict_json(&producer.stdout)?;
    ensure!(
        producer_compiled["manifest_sha256"] == capture.driver.source_manifest_sha256
            && producer_compiled["manifest"]
                == public_manifest(Path::new(&capture.driver.source_root))?
            && producer_compiled["package_name"] == "oe-evolution"
            && producer_compiled["package_version"] == p.expected_package_version
            && producer_compiled["language"] == "rust"
            && producer_compiled["target"] == p.expected_target,
        "rust_runtime_original_driver_identity_mismatch"
    );
    let remaining = deadline.saturating_duration_since(Instant::now());
    ensure!(
        !remaining.is_zero(),
        "rust_runtime_probe_deadline_exhausted"
    );
    let process = observe_identity(
        &a.runner,
        &CodeBudget {
            timeout_ms: u64::try_from(remaining.as_millis())?,
            output_bytes: r.budget.output_bytes,
        },
        &output.join("identity"),
    )?;
    let completed = process.status == "completed"
        && process.exit_code == Some(0)
        && process.report["capture_complete"] == true
        && process.report["stdout"]["truncated"] == false
        && process.report["stderr"]["truncated"] == false;
    let parsed = if completed {
        strict_json(&process.stdout).ok()
    } else {
        None
    };
    let matches = parsed.as_ref().is_some_and(|v| {
        *v == a.compiled
            && v["language"] == "rust"
            && v["package_name"] == "oe-code-runner"
            && v["package_version"] == p.expected_package_version
            && v["target"] == p.expected_target
            && v["manifest"] == wanted
            && v["manifest_sha256"] == a.manifest_sha256
    });
    ensure!(
        profile(&root, &r.profile)? == (p.clone(), wanted.clone())
            && serde_json::from_value::<CodeSourceArtifact>(read(&root, &r.artifact)?)? == a,
        "rust_runtime_probe_inputs_changed"
    );
    verify_artifact(&a)?;
    let git_after = output.join("source-after");
    if p.source_kind == "git_checkout" {
        std::fs::create_dir(&git_after)?;
    }
    git_source(&root, &p, &git_after, &r.budget, deadline)?;
    verify_capture(&root, expected, &p, &r.artifact)?;
    ensure!(
        Instant::now() < deadline,
        "rust_runtime_probe_deadline_exhausted"
    );
    let result = CodeRustRuntimeObservation {
        schema: "oev2.rust-runtime-observation/1".into(),
        profile_sha256: r.profile.sha256.clone(),
        artifact_sha256: r.artifact.sha256.clone(),
        language: "rust".into(),
        execution: if completed && parsed.is_some() {
            "completed"
        } else {
            "failed"
        }
        .into(),
        identity: if completed && parsed.is_some() && !matches { "mismatch" } else { "unknown" }.into(),
        observed_match: if matches { "matched" } else if completed && parsed.is_some() { "mismatch" } else { "unknown" }.into(),
        capture_trust: "untrusted".into(),
        compilation_origin: "unknown".into(),
        process: reference(&root, &output.join("identity/process.json"))?,
        stdout: reference(&root, &output.join("identity/stdout"))?,
        stderr: reference(&root, &output.join("identity/stderr"))?,
        compiled: parsed,
        semantic_execution: "not_run".into(),
        uncovered: vec![
            format!(
                "Actual {} process observed only; full13494 behavior and activation/use/feedback/recovery remain unknown",
                r.phase
            ),
            "External Cargo/feature/native closure not established".into(),
            "Capture is checked against the caller independently frozen original expectation; a newly self-authored trace is not an authenticated compilation certificate or eligibility authority".into(),
        ],
    };
    write_new(
        &output.join("observation.json"),
        &serde_json::to_value(&result)?,
    )?;
    Ok(result)
}
/// Raw process observation only. This grants no bound-runtime, behavior or version verdict.
pub fn observe_identity(
    binary: &CodeBinaryRef,
    budget: &CodeBudget,
    evidence: &Path,
) -> Result<oe_core::process::ProcessObservation> {
    crate::identity_process(binary, budget, evidence)
}
fn build_targets(
    root: &SkillRoot,
    first: &CodeBuildRequest,
    second: &CodeBuildRequest,
) -> Result<()> {
    for b in [first, second] {
        b.validate()
            .map_err(|e| anyhow::anyhow!("rust_runtime_build_request:{e:?}"))?;
        ensure!(
            Path::new(&b.source_root) == root.path(),
            "rust_runtime_build_source_root_mismatch"
        );
        root.private_path(Path::new(&b.evidence_path))?;
        let target = root.private_path(Path::new(&b.target_dir))?;
        ensure!(!target.exists(), "rust_runtime_build_target_not_fresh");
    }
    let a = root.private_path(Path::new(&first.target_dir))?;
    let b = root.private_path(Path::new(&second.target_dir))?;
    let first_evidence = root.private_path(Path::new(&first.evidence_path))?;
    let second_evidence = root.private_path(Path::new(&second.evidence_path))?;
    let paths = [&a, &b, &first_evidence, &second_evidence];
    for (i, left) in paths.iter().enumerate() {
        for right in paths.iter().skip(i + 1) {
            ensure!(
                !left.starts_with(right) && !right.starts_with(left),
                "rust_runtime_build_paths_overlap"
            );
        }
    }
    ensure!(
        !first_evidence.exists() && !second_evidence.exists(),
        "rust_runtime_build_evidence_not_fresh"
    );
    ensure!(
        first.cargo_path == second.cargo_path
            && first.rustc_path == second.rustc_path
            && first.ggen_path == second.ggen_path
            && first.cargo_home == second.cargo_home
            && first.platform == second.platform,
        "rust_runtime_build_conditions_differ"
    );
    Ok(())
}
pub fn reproduce(
    r: &CodeRustRuntimeReproductionRequest,
) -> Result<CodeRustRuntimeReproductionObservation> {
    Ok(reproduce_owned(r, None)?.0)
}
pub(crate) fn reproduce_with_conditions2(
    r: &CodeRustRuntimeReproductionRequest,
    conditions: &ArtifactReference,
) -> Result<CodeRustRuntimeReproductionObservation2> {
    let (base, products) = reproduce_owned(r, Some(conditions))?;
    let root = SkillRoot::new(&r.first.source_root)?;
    let result = CodeRustRuntimeReproductionObservation2 {
        schema: "oev2.rust-runtime-reproduction-observation/2".into(),
        conditions: conditions.clone(),
        base_observation: base,
        first_products: products.first().cloned(),
        second_products: products.get(1).cloned(),
        qualification: "unknown".into(),
        uncovered: vec![
            "Exact complete binary, full DWARF and embedded signature comparison under explicit /2 conditions only".into(),
            "Compiler/SDK/environment closure, authenticated capture authority, full13494, selection/install and engineering benefit remain unknown".into(),
        ],
    };
    write_new(
        &root
            .private_path(Path::new(&r.evidence_path))?
            .join("reproduction2.json"),
        &serde_json::to_value(&result)?,
    )?;
    Ok(result)
}
fn reproduce_owned(
    r: &CodeRustRuntimeReproductionRequest,
    conditions2: Option<&ArtifactReference>,
) -> Result<(
    CodeRustRuntimeReproductionObservation,
    Vec<ArtifactReference>,
)> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_reproduction_request:{e:?}"))?;
    ensure!(
        r.schema == "oev2.rust-runtime-reproduction-request/1",
        "rust_runtime_reproduction_schema"
    );
    let deadline = budget(&r.budget, 3_600_000)?;
    let root = SkillRoot::new(&r.first.source_root)?;
    let (p, wanted) = profile(&root, &r.profile)?;
    ensure!(
        p.expected_build_capture.is_none(),
        "rust_runtime_build_capture_must_not_be_inherited"
    );
    ensure!(
        r.first.platform == p.expected_target,
        "rust_runtime_build_target_mismatch"
    );
    build_targets(&root, &r.first, &r.second)?;
    root.private_path(Path::new(&r.evidence_path))?;
    let reproduction = root.private_path(Path::new(&r.evidence_path))?;
    for build in [&r.first, &r.second] {
        for path in [&build.target_dir, &build.evidence_path] {
            let path = root.private_path(Path::new(path))?;
            ensure!(
                !path.starts_with(&reproduction) && !reproduction.starts_with(&path),
                "rust_runtime_reproduction_paths_overlap"
            );
        }
    }
    let out = root.new_private_dir(Path::new(&r.evidence_path))?;
    let git_dir = out.join("git-source");
    if p.source_kind == "git_checkout" {
        std::fs::create_dir(&git_dir)?;
    }
    git_source(&root, &p, &git_dir, &r.budget, deadline)?;
    let mut refs: Vec<ArtifactReference> = vec![];
    let mut artifacts = vec![];
    let mut captures: Vec<ArtifactReference> = vec![];
    let mut products = vec![];
    let mut product_refs = vec![];
    let mut error = None;
    for (i, b) in [&r.first, &r.second].into_iter().enumerate() {
        let session = conditions2
            .map(|conditions| {
                crate::runtime_reproduction2::BuildSession2::new(
                    SkillRoot::new(root.path())?,
                    b.clone(),
                    conditions.clone(),
                    deadline,
                    usize::try_from(r.budget.output_bytes)?,
                )
            })
            .transpose()?;
        let outcome = match session.as_ref() {
            Some(session) => crate::build_with_deadline_mode(
                b,
                Some(deadline),
                usize::try_from(r.budget.output_bytes)?,
                crate::BuildMode::RuntimeReproduction2(session),
            ),
            None => crate::build_with_deadline(
                b,
                Some(deadline),
                usize::try_from(r.budget.output_bytes)?,
            ),
        };
        match outcome {
            Ok(a) => {
                ensure!(
                    a.manifest == wanted
                        && a.compiled["package_version"] == p.expected_package_version
                        && a.compiled["target"] == p.expected_target
                        && a.compiled["language"] == "rust",
                    "rust_runtime_built_identity_mismatch"
                );
                let side = root.private_path(Path::new(&b.evidence_path))?;
                for stage in STAGES {
                    for leaf in ["process.json", "stdout", "stderr"] {
                        reference(&root, &side.join(stage).join(leaf))?;
                    }
                    let process = strict_json(&read_regular_bytes(
                        &side.join(stage).join("process.json"),
                        LIMIT,
                    )?)?;
                    ensure!(
                        process["status"] == "completed"
                            && process["exit_code"] == 0
                            && process["capture_complete"] == true,
                        "rust_runtime_stage_not_completed:{stage}"
                    );
                }
                let file = out.join(format!("side-{i}.json"));
                write_new(&file, &serde_json::to_value(&a)?)?;
                let artifact_ref = reference(&root, &file)?;
                if let Some(session) = session.as_ref() {
                    let folder = out.join(format!("products-{i}"));
                    std::fs::create_dir(&folder)?;
                    match session.capture_products(&a, artifact_ref.clone(), &folder) {
                        Ok(product) => {
                            let file = folder.join("products.json");
                            write_new(&file, &serde_json::to_value(&product)?)?;
                            product_refs.push(reference(&root, &file)?);
                            products.push(product);
                        }
                        Err(e) => {
                            error = Some(format!("{e:#}"));
                            break;
                        }
                    }
                }
                let driver_root = &crate::driver_root().context("rust_runtime_driver_root")?;
                let capture = CodeRustRuntimeBuildCapture {
                    schema: "oev2.rust-runtime-build-capture/1".into(),
                    source_profile: r.profile.clone(),
                    artifact: artifact_ref.clone(),
                    driver: binary_reference(&std::env::current_exe()?, driver_root)?,
                    normalizer_process: reference(
                        &root,
                        &side.join("build-normalizer/process.json"),
                    )?,
                    generation_process: reference(&root, &side.join("actual-ggen/process.json"))?,
                    runner_build_process: reference(
                        &root,
                        &side.join("build-runner/process.json"),
                    )?,
                    drift_process: reference(&root, &side.join("actual-ggen-check/process.json"))?,
                    identity_process: reference(&root, &side.join("runner-identity/process.json"))?,
                };
                let capture_path = out.join(format!("capture-{i}.json"));
                write_new(&capture_path, &serde_json::to_value(&capture)?)?;
                captures.push(reference(&root, &capture_path)?);
                refs.push(artifact_ref);
                artifacts.push(a);
            }
            Err(e) => {
                error = Some(format!("{e:#}"));
                break;
            }
        }
        ensure!(
            profile(&root, &r.profile)? == (p.clone(), wanted.clone()),
            "rust_runtime_reproduction_inputs_changed"
        );
    }
    for product in &products {
        crate::runtime_reproduction2::verify_products(&root, product)?;
    }
    let equal = artifacts.len() == 2
        && artifacts[0].runner.sha256 == artifacts[1].runner.sha256
        && artifacts[0].runner.bytes == artifacts[1].runner.bytes
        && artifacts[0].compiled == artifacts[1].compiled
        && (conditions2.is_none()
            || (products.len() == 2
                && crate::runtime_reproduction2::products_equal(&products[0], &products[1])));
    let published = if equal {
        for a in &artifacts {
            verify_artifact(a)?
        }
        for product in &products {
            crate::runtime_reproduction2::verify_products(&root, product)?;
        }
        ensure!(
            Instant::now() < deadline,
            "rust_runtime_reproduction_deadline_exhausted"
        );
        let file = out.join("staging-descriptor.json");
        write_new(&file, &serde_json::to_value(&artifacts[0])?)?;
        Some(reference(&root, &file)?)
    } else {
        None
    };
    let result=CodeRustRuntimeReproductionObservation {schema:"oev2.rust-runtime-reproduction-observation/1".into(),profile_sha256:r.profile.sha256.clone(),first:refs.first().cloned(),second:refs.get(1).cloned(),first_capture:captures.first().cloned(),second_capture:captures.get(1).cloned(),execution:if artifacts.len()==2{"completed"}else{"failed"}.into(),reproduction:if equal{"equal"}else if artifacts.len()==2{"different"}else{"unknown"}.into(),published_descriptor:published,uncovered:vec![format!("Partial build error: {error:?}; raw stage artifacts retained"),"No qualification/selection/install performed; full13494/original11/6/19/22 remain distinct".into(),"External Cargo/feature/native closure and engineering benefit unknown".into()]};
    write_new(
        &out.join("reproduction.json"),
        &serde_json::to_value(&result)?,
    )?;
    Ok((result, product_refs))
}
pub struct RuntimeInspector;
impl CodeRustRuntimeInspectPort for RuntimeInspector {
    fn inspect(
        &self,
        r: &CodeRustRuntimeInspectRequest,
    ) -> std::result::Result<CodeRustRuntimeSourceObservation, CodeEvolutionFailure> {
        inspect(&r).map_err(|e| CodeEvolutionFailure {
            code: "rust_runtime_inspect".into(),
            message: format!("{e:#}"),
        })
    }
}
pub struct RuntimeProber;
impl CodeRustRuntimeProbePort for RuntimeProber {
    fn probe(
        &self,
        r: &CodeRustRuntimeProbeRequest,
    ) -> std::result::Result<CodeRustRuntimeObservation, CodeEvolutionFailure> {
        probe(&r).map_err(|e| CodeEvolutionFailure {
            code: "rust_runtime_probe".into(),
            message: format!("{e:#}"),
        })
    }
}
pub struct RuntimeReproducer;
impl CodeRustRuntimeReproductionPort for RuntimeReproducer {
    fn reproduce(
        &self,
        r: &CodeRustRuntimeReproductionRequest,
    ) -> std::result::Result<CodeRustRuntimeReproductionObservation, CodeEvolutionFailure> {
        reproduce(&r).map_err(|e| CodeEvolutionFailure {
            code: "rust_runtime_reproduction".into(),
            message: format!("{e:#}"),
        })
    }
}
/// Upper composition calls the frozen independent executable. It never invokes
/// the builder in the caller process or grants qualification from child JSON.
pub fn dispatch(
    root: &SkillRoot,
    r: &CodeRustRuntimeDispatchRequest,
) -> Result<CodeRustRuntimeDispatchObservation> {
    dispatch_owned(root, r, false)
}
pub fn dispatch_reproduction2(
    root: &SkillRoot,
    r: &CodeRustRuntimeDispatchRequest,
) -> Result<CodeRustRuntimeDispatchObservation> {
    ensure!(r.operation == "build", "runtime2_dispatch_operation");
    dispatch_owned(root, r, true)
}
fn dispatch_owned(
    root: &SkillRoot,
    r: &CodeRustRuntimeDispatchRequest,
    reproduction2: bool,
) -> Result<CodeRustRuntimeDispatchObservation> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_dispatch_request:{e:?}"))?;
    ensure!(
        r.schema
            == if reproduction2 {
                "oev2.rust-runtime-dispatch-request/2"
            } else {
                "oev2.rust-runtime-dispatch-request/1"
            },
        "rust_runtime_dispatch_schema"
    );
    let deadline = budget(&r.budget, 3_600_000)?;
    let input = root.private_path(Path::new(&r.request.path))?;
    read(root, &r.request)?;
    verify_binary(&r.driver)?;
    ensure!(
        Path::new(&r.driver.source_root) == root.path(),
        "rust_runtime_dispatch_source_root"
    );
    let out = root.new_private_dir(Path::new(&r.evidence_path))?;
    let identity = observe_identity(
        &r.driver,
        &CodeBudget {
            timeout_ms: 30_000.min(r.budget.timeout_ms),
            output_bytes: r.budget.output_bytes,
        },
        &out.join("driver-identity"),
    )?;
    ensure!(
        identity.status == "completed"
            && identity.exit_code == Some(0)
            && identity.report["capture_complete"] == true,
        "rust_runtime_dispatch_identity_failed"
    );
    let compiled = strict_json(&identity.stdout)?;
    ensure!(
        compiled["manifest_sha256"] == r.driver.source_manifest_sha256
            && compiled["manifest"] == public_manifest(root.path())?
            && compiled["language"] == "rust"
            && compiled["package_name"] == "oe-evolution",
        "rust_runtime_dispatch_driver_mismatch"
    );
    read(root, &r.request)?;
    verify_binary(&r.driver)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    ensure!(!remaining.is_zero(), "rust_runtime_dispatch_deadline");
    let observed = run_command(
        &CommandSpec {
            executable: r.driver.path.clone().into(),
            arguments: vec![
                format!(
                    "runtime-{}{}",
                    r.operation,
                    if reproduction2 { "2" } else { "" }
                )
                .into(),
                input.as_os_str().into(),
            ],
            cwd: root.path().into(),
            environment: BTreeMap::new(),
            remove_environment: vec![
                "RUSTFLAGS".into(),
                "CARGO_ENCODED_RUSTFLAGS".into(),
                "RUSTC_WRAPPER".into(),
                "RUSTC_WORKSPACE_WRAPPER".into(),
            ],
            timeout: remaining,
            output_limit: usize::try_from(r.budget.output_bytes)?,
        },
        &out.join("dispatch"),
    )?;
    read(root, &r.request)?;
    verify_binary(&r.driver)?;
    let complete = observed.status == "completed"
        && observed.exit_code == Some(0)
        && observed.report["capture_complete"] == true;
    let result = if complete {
        Some(strict_json(&observed.stdout)?)
    } else {
        None
    };
    let response = CodeRustRuntimeDispatchObservation {
        schema: if reproduction2 {
            "oev2.rust-runtime-dispatch-observation/2"
        } else {
            "oev2.rust-runtime-dispatch-observation/1"
        }
        .into(),
        operation: r.operation.clone(),
        execution: if complete { "completed" } else { "failed" }.into(),
        process: reference(root, &out.join("dispatch/process.json"))?,
        stdout: reference(root, &out.join("dispatch/stdout"))?,
        stderr: reference(root, &out.join("dispatch/stderr"))?,
        result,
        uncovered: vec![
            "Independent native driver invocation only; qualification and full13494 remain unknown"
                .into(),
        ],
    };
    write_new(
        &out.join("dispatch-observation.json"),
        &serde_json::to_value(&response)?,
    )?;
    Ok(response)
}
pub struct RuntimeDispatcher<'a> {
    pub root: &'a SkillRoot,
}
impl CodeRustRuntimeDispatchPort for RuntimeDispatcher<'_> {
    fn dispatch(
        &self,
        r: &CodeRustRuntimeDispatchRequest,
    ) -> std::result::Result<CodeRustRuntimeDispatchObservation, CodeEvolutionFailure> {
        dispatch(self.root, r).map_err(|e| CodeEvolutionFailure {
            code: "rust_runtime_dispatch".into(),
            message: format!("{e:#}"),
        })
    }
}

/// Validate a caller's pre-fixed original builder capture. Typed declarations
/// alone do not qualify it; captured raw streams, argv and executable pins agree.
pub fn verify_capture(
    root: &SkillRoot,
    expected: &ArtifactReference,
    current_profile: &CodeRustRuntimeProfile,
    supplied_artifact: &ArtifactReference,
) -> Result<CodeSourceArtifact> {
    let capture: CodeRustRuntimeBuildCapture = serde_json::from_value(read(root, expected)?)?;
    capture
        .validate()
        .map_err(|e| anyhow::anyhow!("rust_runtime_capture:{e:?}"))?;
    ensure!(
        capture.schema == "oev2.rust-runtime-build-capture/1",
        "rust_runtime_capture_schema"
    );
    let (original, _) = profile(root, &capture.source_profile)?;
    ensure!(
        original.expected_build_capture.is_none(),
        "rust_runtime_capture_source_profile_recursive"
    );
    let mut wanted = current_profile.clone();
    wanted.expected_build_capture = None;
    ensure!(
        original == wanted,
        "rust_runtime_capture_source_conditions_changed"
    );
    ensure!(
        supplied_artifact.sha256 == capture.artifact.sha256
            && read(root, supplied_artifact)? == read(root, &capture.artifact)?,
        "rust_runtime_capture_artifact_changed"
    );
    let artifact: CodeSourceArtifact = serde_json::from_value(read(root, &capture.artifact)?)?;
    verify_artifact(&artifact)?;
    verify_binary(&capture.driver)?;
    ensure!(
        artifact.generation["fixed_build_driver"]["executable_sha256"] == capture.driver.sha256
            && artifact.generation["fixed_build_driver"]["source_manifest_sha256"]
                == capture.driver.source_manifest_sha256,
        "rust_runtime_capture_driver_changed"
    );
    ensure!(
        artifact.tools.len() == 4 && artifact.build_processes.len() == 4,
        "rust_runtime_capture_original_stages_missing"
    );
    let process_refs = [
        &capture.normalizer_process,
        &capture.generation_process,
        &capture.runner_build_process,
        &capture.drift_process,
        &capture.identity_process,
    ];
    let executables = [
        &artifact.tools[0],
        &artifact.tools[3],
        &artifact.tools[0],
        &artifact.tools[3],
        &artifact.runner,
    ];
    let args: Vec<Vec<String>> = vec![
        vec![
            "build".into(),
            "--locked".into(),
            "--offline".into(),
            "-j4".into(),
            "-p".into(),
            "oe-design".into(),
            "--bin".into(),
            "oev2-design-bootstrap".into(),
        ],
        vec![
            "generate".into(),
            artifact.source_root.clone(),
            artifact.tools[2].path.clone(),
        ],
        vec![
            "build".into(),
            "--locked".into(),
            "--offline".into(),
            "-j4".into(),
            "--target".into(),
            artifact.platform.clone(),
            "-p".into(),
            "oe-code-runner".into(),
        ],
        vec![
            "check".into(),
            artifact.source_root.clone(),
            artifact.tools[2].path.clone(),
        ],
        vec!["identity".into()],
    ];
    for (i, r) in process_refs.iter().enumerate() {
        let record = read(root, r)?;
        let exe = executables[i];
        verify_binary(exe)?;
        ensure!(
            record["schema"] == "ontology-engineering.external-process/v1"
                && record["status"] == "completed"
                && record["exit_code"] == 0
                && record["capture_complete"] == true
                && record["executable"]["unchanged"] == true,
            "rust_runtime_capture_stage_incomplete"
        );
        ensure!(
            record["cwd"] == artifact.source_root
                && record["arguments"] == serde_json::to_value(&args[i])?
                && record["executable"]["path"] == exe.path
                && record["executable"]["identity"]["sha256"] == exe.sha256
                && record["executable"]["identity"]["bytes"] == exe.bytes,
            "rust_runtime_capture_invocation_mismatch"
        );
        if i < 4 {
            ensure!(
                artifact.build_processes[i] == record,
                "rust_runtime_capture_process_changed"
            );
        }
        let parent = Path::new(&r.path)
            .parent()
            .context("rust_runtime_capture_parent")?;
        for stream in ["stdout", "stderr"] {
            ensure!(
                record[stream]["path"] == stream && record[stream]["truncated"] == false,
                "rust_runtime_capture_stream_invalid"
            );
            let path = root.private_path(&parent.join(stream))?;
            let bytes = read_regular_bytes(&path, 4 * 1024 * 1024)?;
            ensure!(
                record[stream]["sha256"] == sha256(&bytes)
                    && record[stream]["bytes"] == bytes.len() as u64,
                "rust_runtime_capture_stream_changed"
            );
            if stream == "stdout" {
                match i {
                    1 => {
                        let generated = strict_json(&bytes)
                            .context("rust_runtime_capture_generation_stdout_invalid")?;
                        ensure!(
                            generated == artifact.generation["receipt"],
                            "rust_runtime_capture_generation_stdout_changed"
                        );
                    }
                    3 => {
                        let checked = strict_json(&bytes)
                            .context("rust_runtime_capture_check_stdout_invalid")?;
                        ensure!(
                            checked == artifact.generation["check"]
                                && checked["schema"] == "oev2.generation-drift/1"
                                && checked["status"] == "consistent"
                                && checked["drift"] == serde_json::json!([])
                                && checked["generation"] == artifact.generation["receipt"],
                            "rust_runtime_capture_check_stdout_changed"
                        );
                    }
                    4 => ensure!(
                        strict_json(&bytes)? == artifact.compiled,
                        "rust_runtime_capture_compiled_changed"
                    ),
                    // Cargo's two build roles have no structured stdout protocol;
                    // their exact captured bytes, status and invocation are checked above.
                    _ => {}
                }
            }
        }
    }
    Ok(artifact)
}
