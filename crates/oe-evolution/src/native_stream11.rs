//! Explicit selected Cargo raw-file resources and bounded retained CONTENT.
//! The stage adapter borrows NativeBuildSession6; it creates no owner or clock.
//! Full five-stage capture/registry integration is not supplied by this module.
use crate::native_runtime::{deadline_timeout, read, reference};
use crate::native_runtime6::{NativeBuildSession6, verify_pins};
use crate::*;
use oe_core::process::{SpoolLimits, run_command_spooled};
use sha2::{Digest, Sha256};
use std::ffi::OsString;

pub fn selected_resources11() -> CodeNativeCargoStreamResources11 {
    CodeNativeCargoStreamResources11 {
        schema: "oev2.native-cargo-stream-resources/11".into(),
        profile: "cargo-raw64MiB-preview4MiB-segment1MiB/11".into(),
        raw_stream_bytes: 64 * 1024 * 1024,
        preview_bytes: 4 * 1024 * 1024,
        segment_bytes: 1024 * 1024,
        maximum_line_bytes: 64 * 1024,
        maximum_lines: 1_000_000,
        maximum_page_bytes: 4 * 1024 * 1024,
        maximum_derived_bytes: 256 * 1024 * 1024,
    }
}

pub fn verify_resources11(resources: &CodeNativeCargoStreamResources11) -> Result<()> {
    resources
        .validate()
        .map_err(|e| anyhow::anyhow!("stream11_resources:{e:?}"))?;
    ensure!(
        serde_json::to_value(resources)? == serde_json::to_value(selected_resources11())?,
        "stream11_unselected_resource_baseline"
    );
    Ok(())
}

fn private_file(path: &Path, maximum: usize) -> Result<Value> {
    no_symlinks(path)?;
    let before = std::fs::symlink_metadata(path)?;
    ensure!(
        before.is_file() && !before.file_type().is_symlink(),
        "stream11_not_regular"
    );
    let identity = fingerprint_file(path, maximum as u64)?;
    let after = std::fs::symlink_metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            before.dev() == after.dev()
                && before.ino() == after.ino()
                && before.permissions().mode() & 0o777 == 0o600
                && after.permissions().mode() & 0o777 == 0o600,
            "stream11_physical_file_changed_or_not_private"
        );
        Ok(
            json!({"identity": identity, "mode": 0o600, "device": after.dev(), "inode": after.ino()}),
        )
    }
    #[cfg(not(unix))]
    {
        ensure!(
            before.len() == after.len() && before.modified()? == after.modified()?,
            "stream11_physical_file_changed"
        );
        Ok(json!({"identity": identity, "mode": u32::from(after.permissions().readonly())}))
    }
}

/// Verify declarations against complete current source and original references.
/// This does not redo the native caller/SDK/source admission or grant authority.
pub fn verify_plan_content11(
    root: &SkillRoot,
    plan: &CodeNativeCargoStreamPlan11,
) -> Result<Value> {
    plan.validate()
        .map_err(|e| anyhow::anyhow!("stream11_plan:{e:?}"))?;
    verify_resources11(&plan.resources)?;
    ensure!(
        plan.schema == "oev2.native-cargo-stream-plan/11"
            && ["build-normalizer", "build-runner"].contains(&plan.parent_stage.as_str())
            && plan.source_root == root.path().to_str().context("stream11_utf8")?
            && (1..=4 * 1024 * 1024).contains(&plan.body_output_bytes)
            && (1..=3_600_000).contains(&plan.body_timeout_ms),
        "stream11_plan_domain"
    );
    let manifest = public_manifest(root.path())?;
    ensure!(
        digest(&manifest)? == plan.source_manifest_sha256,
        "stream11_source_changed"
    );
    let side = read(root, &plan.side_context)?;
    let stage = read(root, &plan.stage_context)?;
    let stage_root = side["stage_root"].as_str().context("stream11_stage_root")?;
    let evidence_root = side["build"]["evidence_path"]
        .as_str()
        .context("stream11_evidence_root")?;
    ensure!(
        plan.stage_context
            == reference(
                root,
                &Path::new(stage_root)
                    .join(&plan.parent_stage)
                    .join("stage-context.json")
            )?
            && plan.declared_command
                == reference(
                    root,
                    &Path::new(evidence_root)
                        .join(&plan.parent_stage)
                        .with_extension("command.json")
                )?
            && side["build"]["source_root"] == plan.source_root
            && side["body_budget"]["output_bytes"] == plan.body_output_bytes
            && side["body_budget"]["timeout_ms"] == plan.body_timeout_ms
            && stage["schema"] == "oev2.native-build-stage/6"
            && stage["source_root"] == plan.source_root
            && stage["source_manifest"] == manifest
            && stage["side_context"] == serde_json::to_value(&plan.side_context)?
            && stage["parent_stage"] == plan.parent_stage
            && stage["output_bytes"] == plan.body_output_bytes
            && stage["deadline_epoch_ms"] == plan.deadline_epoch_ms,
        "stream11_original_binding_changed"
    );
    let command = read(root, &plan.declared_command)?;
    let timeout = command["timeout_seconds"]
        .as_f64()
        .context("stream11_timeout")?;
    let prepared = command["prepared_epoch_ms"]
        .as_u64()
        .context("stream11_prepared")?;
    let argv: Vec<String> = if plan.parent_stage == "build-normalizer" {
        [
            "build",
            "--locked",
            "--offline",
            "-j4",
            "-p",
            "oe-design",
            "--bin",
            "oev2-design-bootstrap",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    } else {
        vec![
            "build".into(),
            "--locked".into(),
            "--offline".into(),
            "-j4".into(),
            "--target".into(),
            side["build"]["platform"]
                .as_str()
                .context("stream11_platform")?
                .into(),
            "-p".into(),
            "oe-code-runner".into(),
        ]
    };
    ensure!(
        command["schema"] == "oev2.native-command-before-dispatch/1"
            && command["output_bytes"] == plan.body_output_bytes
            && command["cwd"] == plan.source_root
            && command["executable"]["path"] == side["build"]["cargo_path"]
            && command["arguments"] == serde_json::to_value(argv)?
            && command["deadline_epoch_ms"] == plan.deadline_epoch_ms
            && timeout.is_finite()
            && timeout > 0.0
            && timeout <= 1200.0
            && timeout * 1000.0 <= plan.body_timeout_ms as f64 + 0.001
            && plan.deadline_epoch_ms > prepared
            && timeout * 1000.0 <= (plan.deadline_epoch_ms - prepared) as f64 + 0.001,
        "stream11_original_command_changed"
    );
    let executable: CodeNativeFileBinding = serde_json::from_value(command["executable"].clone())?;
    crate::native_profile::verify_file(&executable)?;
    Ok(command)
}

fn verify_spec_content(
    plan: &CodeNativeCargoStreamPlan11,
    command: &Value,
    spec: &CommandSpec,
) -> Result<()> {
    let environment = spec
        .environment
        .iter()
        .map(|(k, v)| {
            Ok((
                k.to_str().context("stream11_env_utf8")?,
                v.to_str().context("stream11_env_utf8")?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let removed = spec
        .remove_environment
        .iter()
        .map(|s| s.to_str().context("stream11_removed_utf8"))
        .collect::<Result<Vec<_>>>()?;
    let args = spec
        .arguments
        .iter()
        .map(|s| s.to_str().context("stream11_argv_utf8"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        command["executable"]["path"] == spec.executable.to_str().context("stream11_exe_utf8")?
            && command["arguments"] == serde_json::to_value(args)?
            && command["cwd"] == spec.cwd.to_str().context("stream11_cwd_utf8")?
            && command["environment"] == serde_json::to_value(environment)?
            && command["removed_environment"] == serde_json::to_value(removed)?
            && command["timeout_seconds"] == spec.timeout.as_secs_f64()
            && spec.output_limit as u64 == plan.body_output_bytes,
        "stream11_dispatch_differs_from_original_content"
    );
    Ok(())
}

/// Actual dispatch consumes a borrowed deadline. The serialized plan is only
/// content and cannot allocate a live owner. The full caller admits that owner.
pub(crate) fn observe_selected_stage11(
    root: &SkillRoot,
    spec: &CommandSpec,
    plan_ref: &ArtifactReference,
    deadline: Instant,
) -> Result<(CodeNativeCargoStreamCapture11, Value)> {
    deadline_timeout(deadline)?;
    let plan: CodeNativeCargoStreamPlan11 = serde_json::from_value(read(root, plan_ref)?)?;
    let command = verify_plan_content11(root, &plan)?;
    verify_spec_content(&plan, &command, spec)?;
    let plan_path = Path::new(&plan_ref.path);
    let stage_folder = plan_path.parent().context("stream11_plan_parent")?;
    ensure!(
        plan_path == stage_folder.join("stream-plan11.json")
            && stage_folder.with_extension("command.json")
                == Path::new(&plan.declared_command.path),
        "stream11_plan_path_changed"
    );
    let mut actual = CommandSpec {
        executable: spec.executable.clone(),
        arguments: spec.arguments.clone(),
        cwd: spec.cwd.clone(),
        environment: spec.environment.clone(),
        remove_environment: spec.remove_environment.clone(),
        timeout: spec.timeout,
        output_limit: plan.resources.raw_stream_bytes as usize,
    };
    // Retention/validation work consumes the existing caller deadline. Core
    // applies it without rewriting the separately frozen timeout declaration.
    deadline_timeout(deadline)?;
    let observed = run_command_spooled(
        &actual,
        &stage_folder.join("raw-stream11"),
        deadline,
        SpoolLimits {
            preview_bytes: plan.resources.preview_bytes as usize,
            segment_bytes: plan.resources.segment_bytes as usize,
        },
    )?;
    observed.verify_retained()?;
    deadline_timeout(deadline)?;
    ensure!(
        observed.status == "completed"
            && observed.exit_code == Some(0)
            && observed.report["capture_complete"] == true,
        "stream11_stage_not_completed"
    );
    actual.output_limit = spec.output_limit;
    verify_spec_content(&plan, &verify_plan_content11(root, &plan)?, &actual)?;
    let capture = CodeNativeCargoStreamCapture11 {
        schema: "oev2.native-cargo-stream-capture/11".into(),
        plan: plan_ref.clone(),
        process: reference(root, &stage_folder.join("raw-stream11/process.json"))?,
        stdout_sha256: observed.report["stdout"]["sha256"]
            .as_str()
            .context("stream11_stdout_sha")?
            .into(),
        stdout_bytes: observed.report["stdout"]["bytes"]
            .as_u64()
            .context("stream11_stdout_bytes")?,
        stderr_sha256: observed.report["stderr"]["sha256"]
            .as_str()
            .context("stream11_stderr_sha")?
            .into(),
        stderr_bytes: observed.report["stderr"]["bytes"]
            .as_u64()
            .context("stream11_stderr_bytes")?,
        qualification: "complete_bounded_stream_content_only".into(),
    };
    capture
        .validate()
        .map_err(|e| anyhow::anyhow!("stream11_capture:{e:?}"))?;
    deadline_timeout(deadline)?;
    write_new(
        &stage_folder.join("stream-capture11.json"),
        &serde_json::to_value(&capture)?,
    )?;
    let c = inspect_capture_content11(
        root,
        &reference(root, &stage_folder.join("stream-capture11.json"))?,
    )?;
    c.verify_current()?;
    observed.verify_retained()?;
    deadline_timeout(deadline)?;
    Ok((capture, observed.report))
}

/// Cargo transport inside the original five-stage builder and borrowed session.
pub(crate) fn capture_borrowed_cargo_stage11(
    session: &NativeBuildSession6,
    spec: &mut CommandSpec,
    parent_stage: &str,
) -> Result<(CodeNativeCargoStreamCapture11, Value)> {
    deadline_timeout(session.deadline)?;
    ensure!(
        spec.output_limit == session.output_limit,
        "stream11_inner_budget_changed"
    );
    crate::native_runtime10::configure_cargo(session, spec, parent_stage)?;
    let stage = Path::new(&session.side.build.evidence_path).join(parent_stage);
    let plan = CodeNativeCargoStreamPlan11 {
        schema: "oev2.native-cargo-stream-plan/11".into(),
        source_root: session.side.build.source_root.clone(),
        source_manifest_sha256: digest(&public_manifest(session.root.path())?)?,
        side_context: session.side_ref.clone(),
        stage_context: reference(
            &session.root,
            &Path::new(&session.side.stage_root)
                .join(parent_stage)
                .join("stage-context.json"),
        )?,
        parent_stage: parent_stage.into(),
        declared_command: reference(&session.root, &stage.with_extension("command.json"))?,
        body_output_bytes: session.side.body_budget.output_bytes,
        body_timeout_ms: session.side.body_budget.timeout_ms,
        deadline_epoch_ms: session.deadline_epoch_ms,
        resources: selected_resources11(),
    };
    // The stage destination belongs to the same original evidence root. It is
    // prepared exclusively; raw files live in its new explicitly named child.
    oe_core::create_new_private_dir(&session.root.private_path(&stage)?)?;
    let path = stage.join("stream-plan11.json");
    write_new(&path, &serde_json::to_value(plan)?)?;
    verify_pins(&session.root, &session.recipe)?;
    let result = observe_selected_stage11(
        &session.root,
        spec,
        &reference(&session.root, &path)?,
        session.deadline,
    )?;
    verify_pins(&session.root, &session.recipe)?;
    deadline_timeout(session.deadline)?;
    Ok(result)
}

/// Non-deserializable verified CONTENT handle. It is not a live owner handle.
pub struct VerifiedCargoStreamContent11 {
    root: SkillRoot,
    reference: ArtifactReference,
    capture: CodeNativeCargoStreamCapture11,
    plan: CodeNativeCargoStreamPlan11,
    process: Value,
}

impl VerifiedCargoStreamContent11 {
    pub fn capture_reference(&self) -> &ArtifactReference {
        &self.reference
    }
    pub fn capture(&self) -> &CodeNativeCargoStreamCapture11 {
        &self.capture
    }
    pub fn resources(&self) -> &CodeNativeCargoStreamResources11 {
        &self.plan.resources
    }
    pub fn root(&self) -> &SkillRoot {
        &self.root
    }
    pub fn evidence_directory(&self) -> &Path {
        Path::new(&self.reference.path)
            .parent()
            .expect("checked stream capture parent")
    }
    pub fn verify_current(&self) -> Result<()> {
        ensure!(
            read(&self.root, &self.reference)? == serde_json::to_value(&self.capture)?,
            "stream11_capture_changed"
        );
        ensure!(
            read(&self.root, &self.capture.plan)? == serde_json::to_value(&self.plan)?,
            "stream11_plan_changed"
        );
        ensure!(
            read(&self.root, &self.capture.process)? == self.process,
            "stream11_process_changed"
        );
        verify_plan_content11(&self.root, &self.plan)?;
        self.visit_stream("stdout", |_, _| Ok(()))?;
        self.visit_stream("stderr", |_, _| Ok(()))?;
        Ok(())
    }
    /// Each callback receives exact original bytes, one segment at a time.
    /// Callback side effects are provisional until the complete check returns.
    pub fn visit_stream(
        &self,
        name: &str,
        mut f: impl FnMut(u64, &[u8]) -> Result<()>,
    ) -> Result<()> {
        ensure!(["stdout", "stderr"].contains(&name), "stream11_stream_name");
        let stream = &self.process[name];
        let folder = Path::new(&self.capture.process.path)
            .parent()
            .context("stream11_process_parent")?
            .join(format!("{name}-segments"));
        let rows = stream["segments"].as_array().context("stream11_segments")?;
        ensure!(rows.len() <= 64, "stream11_segment_count");
        no_symlinks(&folder)?;
        let names = std::fs::read_dir(&folder)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        let wanted: BTreeSet<_> = (0..rows.len())
            .map(|i| folder.join(format!("segment-{i:05}.bin")))
            .collect();
        ensure!(names == wanted, "stream11_segment_inventory");
        let mut digest = Sha256::new();
        let mut preview = Sha256::new();
        let mut preview_bytes = 0usize;
        let mut offset = 0u64;
        for (i, row) in rows.iter().enumerate() {
            let path = folder.join(format!("segment-{i:05}.bin"));
            ensure!(
                row["ordinal"] == i as u64
                    && row["byte_offset"] == offset
                    && row["path"] == path.to_str().context("stream11_path_utf8")?,
                "stream11_segment_order"
            );
            ensure!(
                private_file(&path, self.plan.resources.segment_bytes as usize)? == row["file"],
                "stream11_segment_physical_content"
            );
            let bytes = read_regular_bytes(&path, self.plan.resources.segment_bytes as usize)?;
            ensure!(
                !bytes.is_empty()
                    && (i + 1 == rows.len()
                        || bytes.len() as u64 == self.plan.resources.segment_bytes),
                "stream11_short_middle_segment"
            );
            ensure!(
                private_file(&path, self.plan.resources.segment_bytes as usize)? == row["file"],
                "stream11_segment_after_read"
            );
            digest.update(&bytes);
            let count = bytes
                .len()
                .min(self.plan.resources.preview_bytes as usize - preview_bytes);
            preview.update(&bytes[..count]);
            preview_bytes += count;
            f(offset, &bytes)?;
            offset = offset
                .checked_add(bytes.len() as u64)
                .context("stream11_raw_overflow")?;
            ensure!(
                offset <= self.plan.resources.raw_stream_bytes,
                "stream11_raw_limit"
            );
        }
        ensure!(
            stream["bytes"] == offset
                && stream["sha256"] == format!("{:x}", digest.finalize())
                && stream["preview_bytes"] == preview_bytes as u64
                && stream["preview_sha256"] == format!("{:x}", preview.finalize())
                && stream["preview_truncated"] == (offset > preview_bytes as u64)
                && stream["eof"] == true
                && stream["truncated"] == false
                && stream["failure"].is_null()
                && stream["observed_bytes_at_least"] == offset,
            "stream11_incomplete_or_changed_raw_stream"
        );
        ensure!(
            read(&self.root, &self.capture.process)? == self.process,
            "stream11_process_changed_after_read"
        );
        Ok(())
    }
}

pub fn inspect_capture_content11(
    root: &SkillRoot,
    r: &ArtifactReference,
) -> Result<VerifiedCargoStreamContent11> {
    let capture: CodeNativeCargoStreamCapture11 = serde_json::from_value(read(root, r)?)?;
    capture
        .validate()
        .map_err(|e| anyhow::anyhow!("stream11_capture:{e:?}"))?;
    let plan: CodeNativeCargoStreamPlan11 = serde_json::from_value(read(root, &capture.plan)?)?;
    let command = verify_plan_content11(root, &plan)?;
    let process = read(root, &capture.process)?;
    let folder = Path::new(&r.path)
        .parent()
        .context("stream11_capture_parent")?;
    ensure!(
        capture.schema == "oev2.native-cargo-stream-capture/11"
            && capture.qualification == "complete_bounded_stream_content_only"
            && r.path
                == folder
                    .join("stream-capture11.json")
                    .to_str()
                    .context("stream11_path_utf8")?
            && capture.plan == reference(root, &folder.join("stream-plan11.json"))?
            && capture.process == reference(root, &folder.join("raw-stream11/process.json"))?
            && folder.with_extension("command.json") == Path::new(&plan.declared_command.path),
        "stream11_capture_binding"
    );
    ensure!(
        process["schema"] == "ontology-engineering.external-process/segmented-streams/1"
            && process["status"] == "completed"
            && process["observed_status"] == "completed"
            && process["exit_code"] == 0
            && process["capture_complete"] == true
            && process["descendant_cleanup"]["state"] == "local_cleanup_completed"
            && process["executable"]["unchanged"] == true
            && process["native_or_engineering_authority"] == "not_granted"
            && process["semantic_execution"] == "not_inferred"
            && process["stream_retention"] == "complete_raw_segments_with_bounded_preview"
            && process["environment_values"] == "not_recorded"
            && process["process_tree"]
                == (if cfg!(windows) {
                    "windows_job_object_create_suspended_assignment"
                } else {
                    "unix_process_group"
                })
            && process["per_stream_limit"] == plan.resources.raw_stream_bytes
            && process["preview_limit"] == plan.resources.preview_bytes
            && process["segment_limit"] == plan.resources.segment_bytes
            && process["timeout_seconds"] == command["timeout_seconds"]
            && process["arguments"] == command["arguments"]
            && process["cwd"] == command["cwd"]
            && process["executable"]["path"] == command["executable"]["path"]
            && process["executable"]["identity"]["sha256"] == command["executable"]["sha256"]
            && process["executable"]["identity"]["bytes"] == command["executable"]["bytes"]
            && process["stdout"]["sha256"] == capture.stdout_sha256
            && process["stdout"]["bytes"] == capture.stdout_bytes
            && process["stderr"]["sha256"] == capture.stderr_sha256
            && process["stderr"]["bytes"] == capture.stderr_bytes,
        "stream11_process_content_incomplete_or_changed"
    );
    let added = command["environment"]
        .as_object()
        .context("stream11_env")?
        .keys()
        .collect::<Vec<_>>();
    ensure!(
        process["environment_added_keys"] == serde_json::to_value(added)?
            && process["environment_removed_keys"] == command["removed_environment"],
        "stream11_process_environment_changed"
    );
    let content = VerifiedCargoStreamContent11 {
        root: SkillRoot::new(root.path())?,
        reference: r.clone(),
        capture,
        plan,
        process,
    };
    content.verify_current()?;
    Ok(content)
}

/// Actual native execution checking adds the original selected macOS process
/// backend and exact executable/argv obligations to the generic content check.
/// It does not synthesize or reinterpret a legacy raw-stream process report.
pub(crate) fn verify_native_stage11(
    root: &SkillRoot,
    capture: &ArtifactReference,
    process: &ArtifactReference,
    executable: &CodeNativeFileBinding,
    cwd: &Path,
    arguments: &[String],
) -> Result<()> {
    crate::native_profile::verify_file(executable)?;
    let content = inspect_capture_content11(root, capture)?;
    ensure!(
        content.capture().process == *process,
        "stream11_native_process_reference"
    );
    let value = read(root, process)?;
    verify_native_process_conditions11(&value)?;
    ensure!(
        value["executable"]["resolved"] == executable.path
            && value["executable"]["path"] == executable.path
            && value["executable"]["identity"]["sha256"] == executable.sha256
            && value["executable"]["identity"]["bytes"] == executable.bytes
            && value["cwd"] == cwd.to_str().context("stream11_native_cwd_utf8")?
            && value["arguments"] == serde_json::to_value(arguments)?
            && value["elapsed_seconds"]
                .as_f64()
                .is_some_and(|t| t.is_finite() && t >= 0.0),
        "stream11_native_backend_or_invocation_changed"
    );
    content.verify_current()
}

fn verify_native_process_conditions11(value: &Value) -> Result<()> {
    let declared = value["timeout_seconds"]
        .as_f64()
        .context("stream11_native_timeout")?;
    ensure!(
        value["descendant_cleanup"]["backend"] == "macos_libproc_observed_pid_birth_subtree"
            && value["process_tree"] == "unix_process_group"
            && value["borrowed_deadline"]["schema"] == "oe-core.process-borrowed-deadline/1"
            && value["borrowed_deadline"]["expired"] == false
            && value["borrowed_deadline"]["checked_phase"] == "after_spool_retention"
            && value["borrowed_deadline"]["remaining_seconds"]
                .as_f64()
                .is_some_and(|n| n.is_finite() && n > 0.0)
            && declared.is_finite()
            && declared > 0.0
            && declared <= 1200.0
            && value["effective_process_timeout_seconds"]
                .as_f64()
                .is_some_and(|n| n.is_finite() && n > 0.0 && n <= declared)
            && value["terminal_receipt"].is_null(),
        "stream11_native_backend_or_borrowed_deadline_changed"
    );
    Ok(())
}

#[cfg(test)]
mod native11_process_conditions {
    use super::*;
    #[test]
    fn native11_missing_backend_or_expired_borrowed_deadline_refuses() {
        // This selected metadata fixture checks refusal rules only; it cannot
        // replace inspect_capture_content11 or construct native authority.
        let value = json!({
            "descendant_cleanup":{"backend":"macos_libproc_observed_pid_birth_subtree"},
            "process_tree":"unix_process_group", "timeout_seconds":1200.0,
            "effective_process_timeout_seconds":1000.0,
            "borrowed_deadline":{"schema":"oe-core.process-borrowed-deadline/1",
                "expired":false,"checked_phase":"after_spool_retention","remaining_seconds":42.0}
        });
        verify_native_process_conditions11(&value).unwrap();
        for changed in [
            (
                "/descendant_cleanup/backend",
                json!("existing_unix_process_group_only"),
            ),
            ("/process_tree", json!("unchecked")),
            ("/borrowed_deadline/schema", json!("content-only")),
            ("/borrowed_deadline/expired", json!(true)),
            ("/borrowed_deadline/checked_phase", json!("before_spawn")),
            ("/borrowed_deadline/remaining_seconds", json!(0.0)),
            ("/timeout_seconds", json!(1200.1)),
            ("/effective_process_timeout_seconds", json!(1200.1)),
        ] {
            let mut invalid = value.clone();
            *invalid.pointer_mut(changed.0).unwrap() = changed.1;
            assert!(
                verify_native_process_conditions11(&invalid).is_err(),
                "{}",
                changed.0
            );
        }
        let mut invalid = value.clone();
        invalid["terminal_receipt"] = json!("deadline-rejection.json");
        assert!(verify_native_process_conditions11(&invalid).is_err());
        let mut missing = value;
        missing.as_object_mut().unwrap().remove("borrowed_deadline");
        assert!(verify_native_process_conditions11(&missing).is_err());
    }
}

/// Exact frozen content to CommandSpec for controlled content verification
/// tests and the admitted stage adapter. This reconstructs no live authority.
pub fn declared_spec_content11(
    root: &SkillRoot,
    plan: &CodeNativeCargoStreamPlan11,
) -> Result<CommandSpec> {
    let command = verify_plan_content11(root, plan)?;
    Ok(CommandSpec {
        executable: command["executable"]["path"]
            .as_str()
            .context("stream11_exe")?
            .into(),
        arguments: serde_json::from_value::<Vec<String>>(command["arguments"].clone())?
            .into_iter()
            .map(OsString::from)
            .collect(),
        cwd: plan.source_root.clone().into(),
        environment: serde_json::from_value::<BTreeMap<String, String>>(
            command["environment"].clone(),
        )?
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect(),
        remove_environment: serde_json::from_value::<Vec<String>>(
            command["removed_environment"].clone(),
        )?
        .into_iter()
        .map(OsString::from)
        .collect(),
        timeout: Duration::from_secs_f64(
            command["timeout_seconds"]
                .as_f64()
                .context("stream11_timeout")?,
        ),
        output_limit: plan.body_output_bytes as usize,
    })
}
