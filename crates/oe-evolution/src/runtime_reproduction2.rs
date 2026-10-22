//! Explicit /2 reproduction conditions, borrowing the original builder and deadline.
//! The fixed adapter preserves debug maps, objects and signature bytes. It does
//! not certify the complete compiler/SDK/environment closure or grant eligibility.
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{SkillRoot, fingerprint_file, read_regular_bytes, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LIMIT: usize = 16 * 1024 * 1024;
pub const SCHEMA: &str = "oev2.rust-runtime-reproduction-request/2";

fn read(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    reference
        .validate()
        .map_err(|e| anyhow::anyhow!("runtime2_reference:{e:?}"))?;
    let path = root.private_path(Path::new(&reference.path))?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "runtime2_reference_changed"
    );
    Ok(strict_json(&bytes)?)
}
fn reference(root: &SkillRoot, path: &Path) -> Result<ArtifactReference> {
    let path = root.private_path(path)?;
    Ok(ArtifactReference {
        path: path.to_str().context("runtime2_path_utf8")?.into(),
        sha256: sha256(read_regular_bytes(&path, LIMIT)?),
    })
}
pub fn inspect_conditions(
    root: &SkillRoot,
    pin: &ArtifactReference,
) -> Result<CodeRustRuntimeReproductionConditions2> {
    let c: CodeRustRuntimeReproductionConditions2 = serde_json::from_value(read(root, pin)?)?;
    c.validate()
        .map_err(|e| anyhow::anyhow!("runtime2_conditions:{e:?}"))?;
    ensure!(
        c.schema == "oev2.rust-runtime-reproduction-conditions/2"
            && c.incremental == "disabled"
            && c.debug_info == "full"
            && c.split_debug_info == "unpacked"
            && c.strip == "none"
            && c.debug_map == "target_root_relative"
            && c.debug_source_paths == "target_root_to_logical_debug_only"
            && c.signature == "embedded_adhoc_required",
        "runtime2_conditions_unsupported"
    );
    for tool in [
        &c.linker_adapter,
        &c.rustc_adapter,
        &c.cc,
        &c.native_linker,
        &c.dsymutil,
        &c.dwarfdump,
        &c.codesign,
    ] {
        ensure!(
            Path::new(&tool.source_root) == root.path(),
            "runtime2_tool_root_mismatch"
        );
        verify_binary(tool)?;
    }
    Ok(c)
}

pub(crate) struct BuildSession2 {
    root: SkillRoot,
    build: CodeBuildRequest,
    conditions: ArtifactReference,
    deadline: Instant,
    output_limit: usize,
}
impl BuildSession2 {
    pub(crate) fn new(
        root: SkillRoot,
        build: CodeBuildRequest,
        conditions: ArtifactReference,
        deadline: Instant,
        output_limit: usize,
    ) -> Result<Self> {
        ensure!(
            build.platform == "aarch64-apple-darwin",
            "runtime2_target_unsupported"
        );
        inspect_conditions(&root, &conditions)?;
        Ok(Self {
            root,
            build,
            conditions,
            deadline,
            output_limit,
        })
    }
    pub(crate) fn configure_cargo(&self, spec: &mut CommandSpec, stage: &str) -> Result<()> {
        ensure!(
            ["build-normalizer", "build-runner"].contains(&stage),
            "runtime2_stage"
        );
        let c = inspect_conditions(&self.root, &self.conditions)?;
        let context_dir = self
            .root
            .private_path(Path::new(&self.build.evidence_path))?
            .join("runtime2-contexts");
        std::fs::create_dir_all(&context_dir)?;
        let remaining = self
            .deadline
            .saturating_duration_since(Instant::now())
            .min(spec.timeout);
        ensure!(!remaining.is_zero(), "runtime2_deadline");
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .checked_add(remaining.as_millis())
            .context("runtime2_deadline_overflow")?;
        let context = CodeRustRuntimeLinkerContext2 {
            schema: "oev2.rust-runtime-linker-context/2".into(),
            conditions: self.conditions.clone(),
            source_root: self
                .root
                .path()
                .to_str()
                .context("runtime2_path_utf8")?
                .into(),
            source_manifest_sha256: digest(&public_manifest(self.root.path())?)?,
            target_root: self
                .root
                .private_path(Path::new(&self.build.target_dir))?
                .to_str()
                .context("runtime2_path_utf8")?
                .into(),
            invocation_root: self
                .root
                .private_path(Path::new(&self.build.evidence_path))?
                .join("runtime2-linker-calls")
                .join(stage)
                .to_str()
                .context("runtime2_path_utf8")?
                .into(),
            parent_stage: stage.into(),
            deadline_epoch_ms: u64::try_from(epoch)?,
            output_bytes: u64::try_from(self.output_limit)?,
            rustc: binary_reference(Path::new(&self.build.rustc_path), self.root.path())?,
            compiler_invocation_root: self
                .root
                .private_path(Path::new(&self.build.evidence_path))?
                .join("runtime2-compiler-calls")
                .join(stage)
                .to_str()
                .context("runtime2_path_utf8")?
                .into(),
        };
        let path = context_dir.join(format!("{stage}.json"));
        write_new(&path, &serde_json::to_value(&context)?)?;
        std::fs::create_dir_all(&context.invocation_root)?;
        std::fs::create_dir_all(&context.compiler_invocation_root)?;
        // No caller-supplied Rust flags. These finite Cargo conditions and the
        // fixed adapter are explicit inputs, retained with the context bytes.
        for (key, value) in [
            ("CARGO_INCREMENTAL", "0"),
            ("CARGO_PROFILE_DEV_INCREMENTAL", "false"),
            ("CARGO_PROFILE_DEV_DEBUG", "2"),
            ("CARGO_PROFILE_DEV_SPLIT_DEBUGINFO", "unpacked"),
            ("CARGO_PROFILE_DEV_STRIP", "none"),
            ("CARGO_ENCODED_RUSTFLAGS", ""),
        ] {
            spec.environment.insert(key.into(), value.into());
        }
        spec.environment.insert(
            "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER".into(),
            c.linker_adapter.path.clone().into(),
        );
        spec.environment
            .insert("RUSTC_WRAPPER".into(), c.rustc_adapter.path.clone().into());
        spec.environment.insert(
            "OE_RUNTIME_REPRODUCTION_CONTEXT".into(),
            path.as_os_str().into(),
        );
        spec.environment.insert(
            "OE_RUNTIME_REPRODUCTION_CONTEXT_SHA256".into(),
            sha256(read_regular_bytes(&path, LIMIT)?).into(),
        );
        preserve_fixed_overrides(spec)?;
        write_new(
            &context_dir.join(format!("{stage}-applied.json")),
            &json!({
                "schema":"oev2.rust-runtime-applied-conditions/2", "conditions":self.conditions,
                "context":reference(&self.root, &path)?, "parent_stage":stage,
                "cargo_incremental":"0", "dev_incremental":"false", "dev_debug":"2",
                "dev_split_debuginfo":"unpacked", "dev_strip":"none", "encoded_rustflags":"",
            "target_linker":c.linker_adapter, "rustc_adapter":c.rustc_adapter, "debug_source_paths":c.debug_source_paths, "qualification":"not_granted",
                "scope":"Rust-owned explicit input application; raw linked commands separately observed"
            }),
        )?;
        Ok(())
    }
    fn timeout(&self, max: Duration) -> Result<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        ensure!(!remaining.is_zero(), "runtime2_deadline");
        Ok(remaining.min(max))
    }
    pub(crate) fn bind_artifact(&self, artifact: &mut CodeSourceArtifact) -> Result<()> {
        inspect_conditions(&self.root, &self.conditions)?;
        let contexts = self
            .root
            .private_path(Path::new(&self.build.evidence_path))?
            .join("runtime2-contexts");
        let mut applied = Vec::new();
        for stage in ["build-normalizer", "build-runner"] {
            applied.push(reference(
                &self.root,
                &contexts.join(format!("{stage}-applied.json")),
            )?);
        }
        artifact.generation["runtime_reproduction2"] = json!({"schema":"oev2.rust-runtime-build-conditions/2", "conditions":self.conditions,"applied":applied,
            "scope":"Explicit finite input application only; original current builder owns stages and budget; no authenticated compiler/SDK origin or eligibility"});
        artifact.uncovered = vec!["Actual current Rust build under explicit /2 conditions; complete output comparison separately required".into(),
            "Compiler/SDK/environment closure, capture authority, semantic behavior/full13494 and selection/install remain unknown".into()];
        Ok(())
    }
    pub(crate) fn capture_products(
        &self,
        artifact: &CodeSourceArtifact,
        artifact_reference: ArtifactReference,
        out: &Path,
    ) -> Result<CodeRustRuntimeProducts2> {
        let c = inspect_conditions(&self.root, &self.conditions)?;
        let runner = Path::new(&artifact.runner.path);
        let binary = file_binding(runner)?;
        let bytes = read_regular_bytes(runner, 512 * 1024 * 1024)?;
        let (uuid, signature, _) = macho(&bytes)?;
        ensure!(!signature.is_empty(), "runtime2_embedded_signature_missing");
        require_adhoc(&signature)?;
        let signature_path = out.join("embedded-signature.bin");
        write_new_bytes(&signature_path, &signature)?;
        let dwarf = runner.with_extension("dwarf");
        ensure!(!dwarf.exists(), "runtime2_debug_output_not_fresh");
        let invoke =
            |tool: &CodeBinaryRef, args: Vec<String>, name: &str| -> Result<ArtifactReference> {
                inspect_conditions(&self.root, &self.conditions)?;
                let result = run_command(
                    &CommandSpec {
                        executable: tool.path.clone().into(),
                        arguments: args.into_iter().map(Into::into).collect(),
                        cwd: PathBuf::from(&self.build.target_dir),
                        environment: BTreeMap::new(),
                        remove_environment: vec![
                            "RUSTFLAGS".into(),
                            "CARGO_ENCODED_RUSTFLAGS".into(),
                            "RUSTC_WRAPPER".into(),
                            "RUSTC_WORKSPACE_WRAPPER".into(),
                        ],
                        timeout: self.timeout(Duration::from_secs(300))?,
                        output_limit: self.output_limit,
                    },
                    &out.join(name),
                )?;
                ensure!(
                    result.status == "completed"
                        && result.exit_code == Some(0)
                        && result.report["capture_complete"] == true,
                    "runtime2_debug_or_signature_observer_failed:{name}"
                );
                inspect_conditions(&self.root, &self.conditions)?;
                Ok(reference(&self.root, &out.join(name).join("process.json"))?)
            };
        let debug_process = invoke(
            &c.dsymutil,
            vec![
                // The command cwd is the actual target root. Resolve the
                // relative owned OSO paths there; keep absolute sysroot paths
                // absolute. A global prepend corrupts those original paths.
                runner.to_str().context("runtime2_path_utf8")?.into(),
                "--flat".into(),
                "--verify".into(),
                "--num-threads".into(),
                "1".into(),
                "-o".into(),
                dwarf.to_str().context("runtime2_path_utf8")?.into(),
            ],
            "link-full-debug",
        )?;
        // Missing debug objects produce warnings even with a zero dsymutil exit.
        let debug_stderr =
            read_regular_bytes(&out.join("link-full-debug/stderr"), self.output_limit)?;
        ensure!(
            !String::from_utf8_lossy(&debug_stderr)
                .to_ascii_lowercase()
                .contains("warning:"),
            "runtime2_debug_capture_warning"
        );
        let full_debug = file_binding(&dwarf)?;
        let debug_bytes = read_regular_bytes(&dwarf, 512 * 1024 * 1024)?;
        let (_, _, sections) = macho(&debug_bytes)?;
        ensure!(
            sections.get("__DWARF/__debug_info").is_some_and(|n| *n > 0)
                && sections.get("__DWARF/__debug_line").is_some_and(|n| *n > 0),
            "runtime2_full_debug_sections_missing"
        );
        let debug_verification = invoke(
            &c.dwarfdump,
            vec![
                "--verify".into(),
                dwarf.to_str().context("runtime2_path_utf8")?.into(),
            ],
            "verify-full-debug",
        )?;
        let signature_verification = invoke(
            &c.codesign,
            vec![
                "--verify".into(),
                "--strict".into(),
                runner.to_str().context("runtime2_path_utf8")?.into(),
            ],
            "verify-embedded-signature",
        )?;
        ensure!(
            file_binding(runner)? == binary,
            "runtime2_binary_changed_during_observation"
        );
        ensure!(
            file_binding(&dwarf)? == full_debug
                && read_regular_bytes(&signature_path, 512 * 1024 * 1024)? == signature,
            "runtime2_debug_or_signature_changed_during_observation"
        );
        let mut linker_invocations = Vec::new();
        let mut compiler_invocations = Vec::new();
        for stage in ["build-normalizer", "build-runner"] {
            let calls = self
                .root
                .private_path(Path::new(&self.build.evidence_path))?
                .join("runtime2-linker-calls")
                .join(stage);
            let mut dirs: Vec<_> = std::fs::read_dir(calls)?.collect::<std::io::Result<_>>()?;
            dirs.sort_by_key(|d| d.file_name());
            ensure!(!dirs.is_empty(), "runtime2_linker_not_observed:{stage}");
            for d in dirs {
                let path = d.path().join("invocation.json");
                let value = strict_json(&read_regular_bytes(&path, LIMIT)?)?;
                ensure!(
                    value["conditions"] == serde_json::to_value(&self.conditions)?
                        && value["parent_stage"] == stage
                        && value["status"] == "completed"
                        && value["exit_code"] == 0,
                    "runtime2_linker_observation_mismatch"
                );
                let process: ArtifactReference = serde_json::from_value(value["process"].clone())?;
                let process = read(&self.root, &process)?;
                ensure!(
                    process["capture_complete"] == true
                        && process["status"] == "completed"
                        && process["exit_code"] == 0,
                    "runtime2_linker_capture_incomplete"
                );
                linker_invocations.push(reference(&self.root, &path)?);
            }
            let calls = self
                .root
                .private_path(Path::new(&self.build.evidence_path))?
                .join("runtime2-compiler-calls")
                .join(stage);
            let mut dirs: Vec<_> = std::fs::read_dir(calls)?.collect::<std::io::Result<_>>()?;
            dirs.sort_by_key(|d| d.file_name());
            ensure!(!dirs.is_empty(), "runtime2_compiler_not_observed:{stage}");
            let mut produced_link = false;
            for dir in dirs {
                let path = dir.path().join("invocation.json");
                let value = strict_json(&read_regular_bytes(&path, LIMIT)?)?;
                ensure!(
                    value["conditions"] == serde_json::to_value(&self.conditions)?,
                    "runtime2_compiler_observation_mismatch"
                );
                let args: Vec<String> =
                    serde_json::from_value(value["received_arguments"].clone())?;
                let role = compiler_role(&args, Path::new(&self.build.target_dir));
                let success = value["status"] == "completed" && value["exit_code"] == 0;
                let failed_probe = role == "metadata_probe"
                    && value["status"] == "process_failed"
                    && value["exit_code"] == 1;
                ensure!(
                    success || failed_probe,
                    "runtime2_compiler_observation_mismatch"
                );
                produced_link |= role == "link_producing" && success;
                let process: ArtifactReference = serde_json::from_value(value["process"].clone())?;
                let p = read(&self.root, &process)?;
                ensure!(
                    p["capture_complete"] == true
                        && p["status"] == value["status"]
                        && p["exit_code"] == value["exit_code"],
                    "runtime2_compiler_capture_incomplete"
                );
                compiler_invocations.push(reference(&self.root, &path)?);
            }
            ensure!(
                produced_link,
                "runtime2_successful_link_compiler_missing:{stage}"
            );
        }
        Ok(CodeRustRuntimeProducts2 {
            schema: "oev2.rust-runtime-products/2".into(),
            conditions: self.conditions.clone(),
            artifact: artifact_reference,
            binary,
            full_debug,
            embedded_signature: file_binding(&signature_path)?,
            uuid,
            debug_process,
            debug_verification,
            signature_verification,
            linker_invocations,
            compiler_invocations,
        })
    }
}

fn compiler_role(args: &[String], target: &Path) -> &'static str {
    let emit = args
        .iter()
        .find_map(|a| a.strip_prefix("--emit=").map(str::to_owned))
        .or_else(|| {
            args.windows(2)
                .find(|a| a[0] == "--emit")
                .map(|a| a[1].clone())
        });
    if emit
        .as_ref()
        .is_some_and(|e| e.split(',').any(|v| v == "link"))
    {
        return "link_producing";
    }
    let out = args
        .windows(2)
        .find(|a| a[0] == "--out-dir")
        .map(|a| Path::new(&a[1]));
    if emit.as_deref() == Some("dep-info,metadata")
        && out.is_some_and(|p| {
            p.starts_with(target)
                && p.file_name().is_some_and(|s| s == "probe")
                && p.parent()
                    .is_some_and(|p| p.file_name().is_some_and(|s| s == "out"))
                && p.components().any(|c| c.as_os_str() == "build")
        })
        && !args.iter().any(|a| a == "-o")
    {
        return "metadata_probe";
    }
    if args
        .iter()
        .any(|a| a.starts_with("--print") || a == "-vV" || a == "--version")
    {
        "query"
    } else {
        "compiler"
    }
}

fn preserve_fixed_overrides(spec: &mut CommandSpec) -> Result<()> {
    // Core intentionally applies env_remove after envs. Only these two keys
    // have finite owned replacement values in /2; every other inherited flag
    // or workspace wrapper stays on the original removal list.
    for key in ["RUSTC_WRAPPER", "CARGO_ENCODED_RUSTFLAGS"] {
        ensure!(
            spec.environment.contains_key(std::ffi::OsStr::new(key)),
            "runtime2_fixed_override_missing"
        );
    }
    ensure!(
        spec.environment[std::ffi::OsStr::new("CARGO_ENCODED_RUSTFLAGS")].is_empty(),
        "runtime2_arbitrary_encoded_flags"
    );
    spec.remove_environment
        .retain(|k| k != "RUSTC_WRAPPER" && k != "CARGO_ENCODED_RUSTFLAGS");
    Ok(())
}

pub fn rustc_entry(mut received: Vec<String>) -> Result<i32> {
    ensure!(!received.is_empty(), "runtime2_compiler_missing");
    let actual_rustc = received.remove(0);
    let path = PathBuf::from(
        std::env::var("OE_RUNTIME_REPRODUCTION_CONTEXT").context("runtime2_context_missing")?,
    );
    let expected = std::env::var("OE_RUNTIME_REPRODUCTION_CONTEXT_SHA256")
        .context("runtime2_context_pin_missing")?;
    let raw = read_regular_bytes(&path, LIMIT)?;
    ensure!(sha256(&raw) == expected, "runtime2_context_changed");
    let context: CodeRustRuntimeLinkerContext2 = serde_json::from_value(strict_json(&raw)?)?;
    context
        .validate()
        .map_err(|e| anyhow::anyhow!("runtime2_context:{e:?}"))?;
    ensure!(
        context.schema == "oev2.rust-runtime-linker-context/2",
        "runtime2_context_schema"
    );
    let root = SkillRoot::new(&context.source_root)?;
    root.private_path(&path)?;
    let target = root.private_path(Path::new(&context.target_root))?;
    let manifest = public_manifest(root.path())?;
    ensure!(
        digest(&manifest)? == context.source_manifest_sha256
            && crate::compiled()?["manifest_sha256"] == context.source_manifest_sha256,
        "runtime2_compiler_source_mismatch"
    );
    let c: CodeRustRuntimeReproductionConditions2 =
        serde_json::from_value(read(&root, &context.conditions)?)?;
    c.validate()
        .map_err(|e| anyhow::anyhow!("runtime2_conditions:{e:?}"))?;
    ensure!(
        c.debug_source_paths == "target_root_to_logical_debug_only"
            && Path::new(&actual_rustc) == Path::new(&context.rustc.path),
        "runtime2_compiler_binding"
    );
    let own = fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)?;
    ensure!(
        own.sha256 == c.rustc_adapter.sha256 && own.bytes == c.rustc_adapter.bytes,
        "runtime2_compiler_adapter_changed"
    );
    let before = fingerprint_file(Path::new(&actual_rustc), 512 * 1024 * 1024)?;
    ensure!(
        before.sha256 == context.rustc.sha256 && before.bytes == context.rustc.bytes,
        "runtime2_compiler_changed"
    );
    ensure!(
        !received
            .iter()
            .any(|s| s.starts_with("--remap-path-prefix") || s.starts_with("--remap-path-scope")),
        "runtime2_compiler_conflicting_mapping"
    );
    let suffix = if received.iter().any(|s| s == "--crate-name") {
        vec![
            format!("--remap-path-prefix={}=/oev2/target", target.display()),
            "--remap-path-scope=debuginfo".into(),
        ]
    } else {
        vec![]
    };
    let mut actual = received.clone();
    actual.extend(suffix.clone());
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    ensure!(
        context.deadline_epoch_ms > now
            && context.output_bytes > 0
            && context.output_bytes <= 4 * 1024 * 1024,
        "runtime2_compiler_budget"
    );
    let invocation_root = root.private_path(Path::new(&context.compiler_invocation_root))?;
    let (_, out) = crate::native_link::fresh_invocation(&root, &invocation_root)?;
    let observed = run_command(
        &CommandSpec {
            executable: actual_rustc.clone().into(),
            arguments: actual.iter().map(Into::into).collect(),
            cwd: std::env::current_dir()?,
            environment: BTreeMap::new(),
            remove_environment: vec!["RUSTC_WRAPPER".into(), "RUSTC_WORKSPACE_WRAPPER".into()],
            timeout: Duration::from_millis((context.deadline_epoch_ms - now).min(1_200_000)),
            output_limit: usize::try_from(context.output_bytes)?,
        },
        &out.join("process"),
    )?;
    ensure!(
        fingerprint_file(Path::new(&actual_rustc), 512 * 1024 * 1024)? == before
            && sha256(read_regular_bytes(&path, LIMIT)?) == expected
            && public_manifest(root.path())? == manifest,
        "runtime2_compiler_inputs_changed"
    );
    let code = if observed.status == "completed" && observed.report["capture_complete"] == true {
        observed.exit_code.unwrap_or(1)
    } else {
        1
    };
    write_new(
        &out.join("invocation.json"),
        &json!({"schema":"oev2.rust-runtime-compiler-invocation/2","conditions":context.conditions,"parent_stage":context.parent_stage,
        "received_arguments":received,"fixed_suffix":suffix,"actual_arguments":actual,"executable":context.rustc,"status":observed.status,"exit_code":code,
        "observed_role":compiler_role(&received,&target),
        "process":reference(&root,&out.join("process/process.json"))?,"scope":"Actual compiler transport and fixed debuginfo mapping only; complete closure and eligibility unknown"}),
    )?;
    std::io::stdout().lock().write_all(&observed.stdout)?;
    std::io::stderr().lock().write_all(&observed.stderr)?;
    Ok(code)
}

pub(crate) fn verify_products(root: &SkillRoot, p: &CodeRustRuntimeProducts2) -> Result<()> {
    p.validate()
        .map_err(|e| anyhow::anyhow!("runtime2_products:{e:?}"))?;
    read(root, &p.conditions)?;
    read(root, &p.artifact)?;
    for file in [&p.binary, &p.full_debug, &p.embedded_signature] {
        ensure!(
            file_binding(&root.private_path(Path::new(&file.path))?)? == *file,
            "runtime2_complete_product_changed"
        );
    }
    let bytes = read_regular_bytes(Path::new(&p.binary.path), 512 * 1024 * 1024)?;
    let (uuid, signature, _) = macho(&bytes)?;
    ensure!(
        uuid == p.uuid
            && sha256(&signature) == p.embedded_signature.sha256
            && signature.len() as u64 == p.embedded_signature.bytes,
        "runtime2_embedded_signature_or_uuid_changed"
    );
    for pin in [
        &p.debug_process,
        &p.debug_verification,
        &p.signature_verification,
    ] {
        read(root, pin)?;
    }
    for pin in p
        .linker_invocations
        .iter()
        .chain(p.compiler_invocations.iter())
    {
        read(root, pin)?;
    }
    Ok(())
}

pub fn linker_entry(received: Vec<String>) -> Result<i32> {
    let path = PathBuf::from(
        std::env::var("OE_RUNTIME_REPRODUCTION_CONTEXT").context("runtime2_context_missing")?,
    );
    let expected = std::env::var("OE_RUNTIME_REPRODUCTION_CONTEXT_SHA256")
        .context("runtime2_context_pin_missing")?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(sha256(&bytes) == expected, "runtime2_context_changed");
    let context: CodeRustRuntimeLinkerContext2 = serde_json::from_value(strict_json(&bytes)?)?;
    context
        .validate()
        .map_err(|e| anyhow::anyhow!("runtime2_context:{e:?}"))?;
    ensure!(
        context.schema == "oev2.rust-runtime-linker-context/2"
            && ["build-normalizer", "build-runner"].contains(&context.parent_stage.as_str()),
        "runtime2_context_schema"
    );
    let root = SkillRoot::new(&context.source_root)?;
    root.private_path(&path)?;
    let target = root.private_path(Path::new(&context.target_root))?;
    let invocation_root = root.private_path(Path::new(&context.invocation_root))?;
    let manifest = public_manifest(root.path())?;
    ensure!(
        digest(&manifest)? == context.source_manifest_sha256
            && crate::compiled()?["manifest_sha256"] == context.source_manifest_sha256,
        "runtime2_linker_source_mismatch"
    );
    let c = inspect_conditions(&root, &context.conditions)?;
    let own = fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)?;
    ensure!(
        own.sha256 == c.linker_adapter.sha256 && own.bytes == c.linker_adapter.bytes,
        "runtime2_linker_executable_mismatch"
    );
    let outputs: Vec<_> = received.windows(2).filter(|p| p[0] == "-o").collect();
    ensure!(outputs.len() == 1, "runtime2_linker_output_count");
    let output = PathBuf::from(&outputs[0][1]);
    let output = if output.is_absolute() {
        output
    } else {
        std::env::current_dir()?.join(output)
    };
    ensure!(
        root.private_path(&output)?.starts_with(&target) && !output.exists(),
        "runtime2_linker_output_scope"
    );
    ensure!(
        !received.iter().any(|s| s.contains("oso_prefix")
            || s.contains("random_uuid")
            || s.contains("no_adhoc_codesign")
            || matches!(
                s.as_str(),
                "-S" | "-s" | "-x" | "-Wl,-S" | "-Wl,-s" | "-Wl,-x"
            )),
        "runtime2_linker_conflicting_argument"
    );
    let mut actual = received.clone();
    let suffix = vec![
        "-v".into(),
        format!("-fuse-ld={}", c.native_linker.path),
        "-Xlinker".into(),
        "-reproducible".into(),
        "-Xlinker".into(),
        "-oso_prefix".into(),
        "-Xlinker".into(),
        format!("{}/", target.display()),
    ];
    actual.extend(suffix.clone());
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    ensure!(
        context.deadline_epoch_ms > now
            && context.output_bytes > 0
            && context.output_bytes <= 4 * 1024 * 1024,
        "runtime2_linker_budget"
    );
    let (_, out) = crate::native_link::fresh_invocation(&root, &invocation_root)?;
    let observation = run_command(
        &CommandSpec {
            executable: c.cc.path.clone().into(),
            arguments: actual.iter().map(Into::into).collect(),
            cwd: std::env::current_dir()?,
            environment: BTreeMap::new(),
            remove_environment: vec![
                "OE_RUNTIME_REPRODUCTION_CONTEXT".into(),
                "OE_RUNTIME_REPRODUCTION_CONTEXT_SHA256".into(),
            ],
            timeout: Duration::from_millis((context.deadline_epoch_ms - now).min(300_000)),
            output_limit: usize::try_from(context.output_bytes)?,
        },
        &out.join("process"),
    )?;
    inspect_conditions(&root, &context.conditions)?;
    ensure!(
        sha256(read_regular_bytes(&path, LIMIT)?) == expected
            && public_manifest(root.path())? == manifest,
        "runtime2_linker_inputs_changed"
    );
    let code =
        if observation.status == "completed" && observation.report["capture_complete"] == true {
            observation.exit_code.unwrap_or(1)
        } else {
            1
        };
    write_new(
        &out.join("invocation.json"),
        &json!({
            "schema":"oev2.rust-runtime-linker-invocation/2", "conditions":context.conditions,
            "context":{"path":path,"sha256":expected}, "parent_stage":context.parent_stage,
            "received_arguments":received, "fixed_suffix":suffix, "actual_arguments":actual,
        "executable":c.cc, "status":observation.status, "exit_code":code, "cc_actual_exit_code":observation.exit_code,
            "output":if code == 0 {Some(file_binding(&output)?)} else {None},
            "process":reference(&root, &out.join("process/process.json"))?,
            "scope":"Actual fixed Rust transport and input bytes; compiler/SDK closure and eligibility unknown"
        }),
    )?;
    std::io::stdout().lock().write_all(&observation.stdout)?;
    std::io::stderr().lock().write_all(&observation.stderr)?;
    Ok(code)
}

fn file_binding(path: &Path) -> Result<CodeNativeFileBinding> {
    let id = fingerprint_file(path, 512 * 1024 * 1024)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        u64::from(std::fs::metadata(path)?.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let mode = if std::fs::metadata(path)?.permissions().readonly() {
        0o444
    } else {
        0o644
    };
    Ok(CodeNativeFileBinding {
        path: path.to_str().context("runtime2_path_utf8")?.into(),
        sha256: id.sha256,
        bytes: id.bytes,
        mode,
    })
}
fn u32le(b: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(offset..offset + 4)
            .context("runtime2_macho_truncated")?
            .try_into()?,
    ))
}
fn u64le(b: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(offset..offset + 8)
            .context("runtime2_macho_truncated")?
            .try_into()?,
    ))
}
fn macho(bytes: &[u8]) -> Result<(String, Vec<u8>, BTreeMap<String, u64>)> {
    ensure!(
        u32le(bytes, 0)? == 0xfeedfacf && u32le(bytes, 4)? == 0x0100_000c,
        "runtime2_macho_arm64_required"
    );
    let n = u32le(bytes, 16)?;
    let size = usize::try_from(u32le(bytes, 20)?)?;
    ensure!(
        n <= 4096 && size <= bytes.len().saturating_sub(32),
        "runtime2_macho_commands"
    );
    let mut offset = 32usize;
    let mut uuid = None;
    let mut signature = None;
    let mut sections = BTreeMap::new();
    for _ in 0..n {
        let cmd = u32le(bytes, offset)?;
        let len = usize::try_from(u32le(bytes, offset + 4)?)?;
        ensure!(
            len >= 8 && offset.checked_add(len).is_some_and(|v| v <= 32 + size),
            "runtime2_macho_command_size"
        );
        if cmd == 0x1b {
            ensure!(len == 24 && uuid.is_none(), "runtime2_macho_uuid");
            uuid = Some(
                bytes[offset + 8..offset + 24]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            );
        }
        if cmd == 0x1d {
            ensure!(len == 16 && signature.is_none(), "runtime2_macho_signature");
            let start = usize::try_from(u32le(bytes, offset + 8)?)?;
            let count = usize::try_from(u32le(bytes, offset + 12)?)?;
            signature = Some(
                bytes
                    .get(
                        start
                            ..start
                                .checked_add(count)
                                .context("runtime2_macho_overflow")?,
                    )
                    .context("runtime2_macho_signature_range")?
                    .to_vec(),
            );
        }
        if cmd == 0x19 {
            ensure!(len >= 72, "runtime2_macho_segment");
            let count = usize::try_from(u32le(bytes, offset + 64)?)?;
            ensure!(72 + count * 80 <= len, "runtime2_macho_sections");
            for i in 0..count {
                let p = offset + 72 + i * 80;
                let name = std::str::from_utf8(&bytes[p..p + 16])?
                    .trim_end_matches('\0')
                    .to_owned();
                let segment = std::str::from_utf8(&bytes[p + 16..p + 32])?.trim_end_matches('\0');
                let count = u64le(bytes, p + 40)?;
                ensure!(
                    sections
                        .insert(format!("{segment}/{name}"), count)
                        .is_none(),
                    "runtime2_macho_duplicate_section"
                );
            }
        }
        offset += len;
    }
    ensure!(offset == 32 + size, "runtime2_macho_commands_incomplete");
    Ok((
        uuid.context("runtime2_macho_uuid_missing")?,
        signature.unwrap_or_default(),
        sections,
    ))
}
fn require_adhoc(bytes: &[u8]) -> Result<()> {
    let be = |o: usize| -> Result<u32> {
        Ok(u32::from_be_bytes(
            bytes
                .get(o..o + 4)
                .context("runtime2_signature_truncated")?
                .try_into()?,
        ))
    };
    ensure!(
        be(0)? == 0xfade0cc0 && usize::try_from(be(4)?)? <= bytes.len(),
        "runtime2_signature_superblob"
    );
    let n = usize::try_from(be(8)?)?;
    ensure!(
        n > 0 && n <= 4096 && 12 + n * 8 <= bytes.len(),
        "runtime2_signature_index"
    );
    let mut found = false;
    for i in 0..n {
        if be(12 + i * 8)? == 0 {
            let off = usize::try_from(be(16 + i * 8)?)?;
            ensure!(
                be(off)? == 0xfade0c02 && be(off + 12)? & 2 != 0,
                "runtime2_signature_not_adhoc"
            );
            found = true;
        }
    }
    ensure!(found, "runtime2_signature_directory_missing");
    Ok(())
}
pub(crate) fn products_equal(a: &CodeRustRuntimeProducts2, b: &CodeRustRuntimeProducts2) -> bool {
    a.binary.sha256 == b.binary.sha256
        && a.binary.bytes == b.binary.bytes
        && a.binary.mode == b.binary.mode
        && a.full_debug.sha256 == b.full_debug.sha256
        && a.full_debug.bytes == b.full_debug.bytes
        && a.full_debug.mode == b.full_debug.mode
        && a.embedded_signature.sha256 == b.embedded_signature.sha256
        && a.embedded_signature.bytes == b.embedded_signature.bytes
        && a.embedded_signature.mode == b.embedded_signature.mode
        && a.conditions == b.conditions
        && a.uuid == b.uuid
}

pub fn reproduce(
    r: &CodeRustRuntimeReproductionRequest2,
) -> Result<CodeRustRuntimeReproductionObservation2> {
    r.validate()
        .map_err(|e| anyhow::anyhow!("runtime2_request:{e:?}"))?;
    ensure!(r.schema == SCHEMA, "runtime2_reproduction_schema");
    let root = SkillRoot::new(&r.original_request.first.source_root)?;
    inspect_conditions(&root, &r.conditions)?;
    crate::runtime_identity::reproduce_with_conditions2(&r.original_request, &r.conditions)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pin() -> ArtifactReference {
        ArtifactReference {
            path: "var/synthetic/unexecuted.json".into(),
            sha256: "a".repeat(64),
        }
    }
    #[test]
    fn reproduction2_metadata_probe_keeps_failure_distinct_from_link_output() {
        let target = Path::new("/private/selected-target");
        let args = vec![
            "--emit=dep-info,metadata".into(),
            "--out-dir".into(),
            "/private/selected-target/debug/build/anyhow/out/probe".into(),
            "src/nightly.rs".into(),
        ];
        assert_eq!(compiler_role(&args, target), "metadata_probe");
        let mut link = args.clone();
        link[0] = "--emit=dep-info,metadata,link".into();
        assert_eq!(compiler_role(&link, target), "link_producing");
        let mut external = args.clone();
        external[2] = "/external/build/anyhow/out/probe".into();
        assert_eq!(compiler_role(&external, target), "compiler");
        let mut output = args.clone();
        output.extend(["-o".into(), "/private/selected-target/published".into()]);
        assert_eq!(compiler_role(&output, target), "compiler");
        let mut unknown = args;
        unknown[0] = "--emit=unknown".into();
        assert_eq!(compiler_role(&unknown, target), "compiler");
    }
    #[test]
    fn reproduction2_fixed_environment_survives_core_removal_and_blocks_inherited_flags() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .canonicalize()
            .unwrap();
        let dir = root.join(format!(
            "var/runtime2-env-regression-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut spec = CommandSpec {
            executable: "/usr/bin/printenv".into(),
            arguments: ["RUSTC_WRAPPER", "CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS"]
                .map(Into::into)
                .to_vec(),
            cwd: root,
            environment: BTreeMap::from([
                ("RUSTC_WRAPPER".into(), "/synthetic/owned-wrapper".into()),
                ("CARGO_ENCODED_RUSTFLAGS".into(), "".into()),
                ("RUSTFLAGS".into(), "MUST_NOT_INHERIT".into()),
            ]),
            remove_environment: [
                "RUSTC_WRAPPER",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTFLAGS",
                "RUSTC_WORKSPACE_WRAPPER",
            ]
            .map(Into::into)
            .to_vec(),
            timeout: Duration::from_secs(2),
            output_limit: 65536,
        };
        preserve_fixed_overrides(&mut spec).unwrap();
        // BSD printenv accepts one selected name. Query each independently;
        // do not let its ignored extra arguments pretend to check two keys.
        for (key, code, bytes) in [
            ("RUSTC_WRAPPER", 0, b"/synthetic/owned-wrapper\n".as_slice()),
            ("CARGO_ENCODED_RUSTFLAGS", 0, b"\n".as_slice()),
            ("RUSTFLAGS", 1, b"".as_slice()),
        ] {
            let mut query = spec.clone();
            query.arguments = vec![key.into()];
            let result = run_command(&query, &dir.join(key)).unwrap();
            assert_eq!(result.exit_code, Some(code));
            assert_eq!(result.stdout, bytes);
            assert_eq!(result.report["capture_complete"], true);
        }
        let mut wrong = spec.clone();
        wrong
            .environment
            .insert("CARGO_ENCODED_RUSTFLAGS".into(), "-Cstrip=symbols".into());
        assert!(preserve_fixed_overrides(&mut wrong).is_err());
    }
    #[test]
    fn reproduction2_declared_conditions_reject_arbitrary_flags_and_relaxed_debug() {
        let tool = json!({"path":"/synthetic/unexecuted-tool","sha256":"b".repeat(64),"bytes":1,"source_root":"/synthetic/unexecuted-root","source_manifest_sha256":"c".repeat(64)});
        let value = json!({"schema":"oev2.rust-runtime-reproduction-conditions/2",
            "incremental":"disabled","debug_info":"full","split_debug_info":"unpacked","strip":"none",
            "debug_map":"target_root_relative","debug_source_paths":"target_root_to_logical_debug_only","signature":"embedded_adhoc_required",
            "linker_adapter":tool,"rustc_adapter":tool,"cc":tool,"native_linker":tool,"dsymutil":tool,"dwarfdump":tool,"codesign":tool});
        let parsed: CodeRustRuntimeReproductionConditions2 =
            serde_json::from_value(value.clone()).unwrap();
        parsed.validate().unwrap();
        for (field, replacement) in [
            ("incremental", "enabled"),
            ("debug_info", "none"),
            ("split_debug_info", "omit"),
            ("strip", "debuginfo"),
            ("signature", "ignore"),
            ("debug_map", "caller_flags"),
        ] {
            let mut wrong = value.clone();
            wrong[field] = json!(replacement);
            assert!(
                serde_json::from_value::<CodeRustRuntimeReproductionConditions2>(wrong)
                    .unwrap()
                    .validate()
                    .is_err(),
                "{field}"
            );
        }
        for field in ["rustflags", "ignore_signature", "passed", "qualification"] {
            let mut wrong = value.clone();
            wrong[field] = json!(true);
            assert!(
                serde_json::from_value::<CodeRustRuntimeReproductionConditions2>(wrong).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn reproduction2_complete_comparison_rejects_each_debug_signature_and_binary_difference() {
        let file = CodeNativeFileBinding {
            path: "/synthetic/unexecuted-output".into(),
            bytes: 1234,
            mode: 0o755,
            sha256: "d".repeat(64),
        };
        let original = CodeRustRuntimeProducts2 {
            schema: "oev2.rust-runtime-products/2".into(),
            conditions: pin(),
            artifact: pin(),
            binary: file.clone(),
            full_debug: file.clone(),
            embedded_signature: file,
            uuid: "e".repeat(32),
            debug_process: pin(),
            debug_verification: pin(),
            signature_verification: pin(),
            linker_invocations: vec![pin()],
            compiler_invocations: vec![pin()],
        };
        assert!(products_equal(&original, &original));
        for field in ["binary", "full_debug", "embedded_signature"] {
            for identity in ["sha256", "bytes", "mode"] {
                let mut value = serde_json::to_value(&original).unwrap();
                value[field][identity] = match identity {
                    "sha256" => json!("f".repeat(64)),
                    "bytes" => json!(1235),
                    _ => json!(0o644),
                };
                let different: CodeRustRuntimeProducts2 = serde_json::from_value(value).unwrap();
                assert!(!products_equal(&original, &different), "{field}.{identity}");
            }
        }
        let mut different = original.clone();
        different.uuid = "f".repeat(32);
        assert!(!products_equal(&original, &different));
        different = original.clone();
        different.conditions.sha256 = "f".repeat(64);
        assert!(!products_equal(&original, &different));
        // Physical output paths may differ; complete content/permissions/condition identity may not.
        different = original.clone();
        different.binary.path = "/synthetic/other-output".into();
        assert!(products_equal(&original, &different));
    }
}
