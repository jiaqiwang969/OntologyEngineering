//! Install the complete external runtime at its final venv path. Rust owns
//! preparation, identity checks and recovery; no owned Python installer runs.
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, ProcessObservation, resolve_executable, run_command};
use oe_core::{
    SkillRoot, create_new_private_dir, fingerprint_file, no_symlinks, read_regular_bytes,
    rename_directory_exclusive, sha256, write_new, write_new_bytes,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::runtime::{
    InstalledRuntimeObservation, RuntimeAccessGuard, RuntimeSourceLock, VerifiedWheel,
    doctor_preflight, installed_runtime_artifact_sha256, read_runtime_source_lock,
    read_wheel_identity, verify_installed_runtime_record,
};
use crate::strict_json;

const INPUT_LIMIT: usize = 64 * 1024 * 1024;
const WHEEL_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const PYTHON_VERSION: &str = "import json,sys;print(json.dumps(list(sys.version_info[:3])))";

use super::InstallOptions;

// Private seam for recovery tests; callers cannot substitute an installed
// identity observation through the public install API.
trait Backend {
    fn origin(&self) -> &'static str;
    fn run(&self, spec: &CommandSpec, evidence: &Path) -> Result<ProcessObservation>;
    fn verify(
        &self,
        root: &SkillRoot,
        python: &Path,
        target: &Path,
        wheel: &VerifiedWheel,
        lock: &RuntimeSourceLock,
    ) -> Result<Value>;
}
// This module is #[cfg(test)] only. The unchanged old negative fixture supplies
// a shell executable printing synthetic JSON; no production interpreter probe
// is restored, and this observation cannot establish current Rust execution.
fn read_synthetic_observation(
    root: &SkillRoot,
    executable: &Path,
) -> Result<InstalledRuntimeObservation> {
    let result = run_command(
        &CommandSpec {
            executable: executable.to_owned(),
            arguments: Vec::new(),
            cwd: root.path().to_owned(),
            environment: BTreeMap::new(),
            remove_environment: Vec::new(),
            timeout: Duration::from_secs(30),
            output_limit: 65536,
        },
        &root.private_path(Path::new("var/historical-identity-fixture"))?,
    )?;
    ensure!(
        result.status == "completed" && result.exit_code == Some(0),
        "historical_fixture_process_failed"
    );
    Ok(serde_json::from_value(strict_json(&result.stdout)?)?)
}

struct External;
impl Backend for External {
    fn origin(&self) -> &'static str {
        "historical_recovery_fixture_only"
    }
    fn run(&self, spec: &CommandSpec, evidence: &Path) -> Result<ProcessObservation> {
        run_command(spec, evidence)
    }
    fn verify(
        &self,
        root: &SkillRoot,
        python: &Path,
        target: &Path,
        wheel: &VerifiedWheel,
        lock: &RuntimeSourceLock,
    ) -> Result<Value> {
        let observed = read_synthetic_observation(root, python)?;
        // A correct Semantica installed somewhere else is not this transaction.
        ensure!(
            observed.environment_prefix == target && observed.python_executable == python,
            "installed_runtime_target_mismatch"
        );
        ensure!(
            observed.package_root.starts_with(target)
                && observed.imported_file.starts_with(&observed.package_root)
                && !observed.imported_paths.is_empty()
                && observed
                    .imported_paths
                    .iter()
                    .all(|p| p.starts_with(&observed.package_root)),
            "installed_runtime_import_escapes_target"
        );
        ensure!(
            observed.imported_version == lock.version
                && observed.distribution_version == lock.version,
            "installed_runtime_version_mismatch"
        );
        ensure!(
            installed_runtime_artifact_sha256(observed.direct_url.as_bytes())?
                == lock.artifact_sha256,
            "installed_runtime_artifact_mismatch"
        );
        let count = verify_installed_runtime_record(wheel, &observed)?;
        Ok(json!({"status":"verified", "observation":observed,
            "record_files":count, "semantic_execution":"not_run"}))
    }
}

struct Inputs {
    python: PathBuf,
    replaced_environment: PathBuf,
    executable_target: PathBuf,
    executable_sha256: String,
    lock: RuntimeSourceLock,
    wheel: VerifiedWheel,
    wheel_path: PathBuf,
    requirements: Vec<u8>,
    hashes: BTreeMap<PathBuf, String>,
}
fn stable_bootstrap_python(python: &Path, target: &Path) -> Result<()> {
    let mut cursor = python.to_owned();
    for _ in 0..64 {
        let parent = fs::canonicalize(cursor.parent().context("bootstrap_parent_missing")?)?;
        ensure!(
            !parent.starts_with(target),
            "bootstrap_python_inside_replaced_environment"
        );
        let leaf = parent.join(cursor.file_name().context("bootstrap_name_missing")?);
        if !fs::symlink_metadata(&leaf)?.file_type().is_symlink() {
            return Ok(());
        }
        let link = fs::read_link(&leaf)?;
        cursor = if link.is_absolute() {
            link
        } else {
            parent.join(link)
        };
    }
    anyhow::bail!("bootstrap_python_symlink_limit")
}
fn inputs(root: &SkillRoot, python: &Path) -> Result<Inputs> {
    let python = resolve_executable(python)?;
    stable_bootstrap_python(&python, &root.path().join("runtime/.venv"))?;
    let executable_target = fs::canonicalize(&python)?;
    let executable_sha256 = fingerprint_file(&executable_target, 512 * 1024 * 1024)?.sha256;
    let lock_path = root.path().join("runtime/semantica-source-lock.json");
    let lock_bytes = read_regular_bytes(&lock_path, INPUT_LIMIT)?;
    let declared = strict_json(&lock_bytes)?;
    let lock = read_runtime_source_lock(root.path(), true)?;
    ensure!(
        declared["source"]["commit"] == lock.commit
            && declared["source"]["version"] == lock.version
            && declared["artifact"]["filename"] == lock.artifact_filename
            && declared["artifact"]["sha256"] == lock.artifact_sha256,
        "runtime_install_input_changed"
    );
    let wheel_path = root
        .path()
        .join("runtime/vendor")
        .join(&lock.artifact_filename);
    let wheel = read_wheel_identity(&wheel_path, &lock)?;
    let requirement_path = root.path().join("runtime/requirements.txt");
    let requirements = read_regular_bytes(&requirement_path, INPUT_LIMIT)?;
    let required = format!("semantica[ontology-runtime]=={}", lock.version);
    let mut pinned = false;
    // This is the project's requirements format, not a partial pip parser.
    // URLs, nested files and index directives would defeat a prepared offline
    // closure, so unsupported input is rejected before touching the old venv.
    for line in std::str::from_utf8(&requirements)?.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line == "--find-links runtime/vendor" {
            continue;
        }
        ensure!(
            !line.starts_with('-')
                && line
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-[],=<>!~+ ".contains(&b)),
            "unsupported_runtime_requirement_directive"
        );
        if line.starts_with("semantica") {
            ensure!(
                line == required,
                "runtime_requirement_differs_from_source_lock"
            );
            pinned = true;
        }
    }
    ensure!(pinned, "runtime_requirement_missing_locked_semantica");
    let mut hashes = BTreeMap::new();
    hashes.insert(lock_path, sha256(lock_bytes));
    hashes.insert(requirement_path, sha256(&requirements));
    hashes.insert(wheel_path.clone(), lock.artifact_sha256.clone());
    let result = Inputs {
        python,
        replaced_environment: root.path().join("runtime/.venv"),
        executable_target,
        executable_sha256,
        lock,
        wheel,
        wheel_path,
        requirements,
        hashes,
    };
    result.unchanged()?;
    Ok(result)
}
impl Inputs {
    fn unchanged(&self) -> Result<()> {
        stable_bootstrap_python(&self.python, &self.replaced_environment)?;
        ensure!(
            fs::canonicalize(&self.python)? == self.executable_target
                && fingerprint_file(&self.executable_target, 512 * 1024 * 1024)?.sha256
                    == self.executable_sha256,
            "bootstrap_python_changed"
        );
        for (path, expected) in &self.hashes {
            ensure!(
                &sha256(read_regular_bytes(path, INPUT_LIMIT)?) == expected,
                "runtime_install_input_changed"
            );
        }
        Ok(())
    }
}

fn arguments(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
fn step(
    backend: &dyn Backend,
    root: &SkillRoot,
    output: &Path,
    id: &str,
    executable: &Path,
    arguments: Vec<OsString>,
    steps: &mut Vec<Value>,
) -> Result<ProcessObservation> {
    let mut environment: BTreeMap<OsString, OsString> = [
        ("TMPDIR", output.join("tmp").into_os_string()),
        ("XDG_CACHE_HOME", output.join("cache").into_os_string()),
        ("PIP_CACHE_DIR", output.join("cache/pip").into_os_string()),
        ("PIP_NO_INPUT", "1".into()),
        ("PIP_DISABLE_PIP_VERSION_CHECK", "1".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect();
    if arguments.iter().any(|value| value == "--isolated") {
        // pip's isolated mode alone still reads global/site configuration and
        // PIP_CONFIG_FILE. Disable all config before any final-environment write.
        environment.insert("PIP_CONFIG_FILE".into(), "/dev/null".into());
    }
    let observation = backend.run(
        &CommandSpec {
            executable: executable.to_owned(),
            arguments,
            cwd: root.path().to_owned(),
            environment,
            remove_environment: [
                "PYTHONPATH",
                "PYTHONHOME",
                "VIRTUAL_ENV",
                "PYTHONSTARTUP",
                "PIP_BUILD_TRACKER",
            ]
            .iter()
            .map(OsString::from)
            .collect(),
            timeout: Duration::from_secs(3600),
            output_limit: 8 * 1024 * 1024,
        },
        &output.join("steps").join(id),
    )?;
    steps.push(json!({"id":id,"process":observation.report}));
    ensure!(
        observation.status == "completed" && observation.exit_code == Some(0),
        "runtime_install_step_failed:{id}"
    );
    Ok(observation)
}
fn pip(output: &Path, operation: &str, isolated: bool) -> Vec<OsString> {
    let mut args = arguments(&["-I", "-B", "-m", "pip"]);
    if isolated {
        args.push("--isolated".into());
    }
    args.extend(arguments(&[
        "--disable-pip-version-check",
        "--no-input",
        "--cache-dir",
    ]));
    args.push(output.join("cache/pip").into_os_string());
    args.push(operation.into());
    args
}
fn prepare_output(root: &SkillRoot, output: &Path) -> Result<PathBuf> {
    let output = root.private_path(output)?;
    let coordination = root.path().join("var/state/runtime-access.lock");
    ensure!(
        !output.starts_with(&coordination) && !coordination.starts_with(&output),
        "runtime_output_overlaps_coordination_lock"
    );
    let output = root.new_private_dir(&output)?;
    for name in ["tmp", "cache", "steps", "wheels"] {
        create_new_private_dir(&output.join(name))?;
    }
    Ok(output)
}
fn preflight_at(
    backend: &dyn Backend,
    root: &SkillRoot,
    input: &Inputs,
    output: &Path,
    steps: &mut Vec<Value>,
) -> Result<Value> {
    let observed = step(
        backend,
        root,
        output,
        "python-version",
        &input.python,
        arguments(&["-I", "-B", "-c", PYTHON_VERSION]),
        steps,
    )?;
    let version: [u64; 3] = serde_json::from_value(strict_json(&observed.stdout)?)?;
    let checks = doctor_preflight(root.path(), version);
    input.unchanged()?;
    let passed = checks
        .iter()
        .all(|c| matches!(c.level.as_str(), "ok" | "info"));
    let result = json!({"schema":"ontology-engineering.runtime-preflight/v1",
        "status":if passed {"ready"} else {"blocked"}, "passed":passed,
        "python_version":version, "checks":checks, "input_hashes":input.hashes,
        "dependencies_available":"not_established", "runtime_modified":false,
        "semantic_execution":"not_run", "origin":backend.origin()});
    write_new(&output.join("preflight.json"), &result)?;
    ensure!(passed, "runtime_preflight_blocked");
    Ok(result)
}
pub fn preflight(root: &SkillRoot, python: &Path, output: &Path) -> Result<Value> {
    ensure!(
        cfg!(feature = "historical-contract-tests"),
        "legacy_python_runtime_install_retired_in_v2"
    );
    let input = inputs(root, python)?;
    let output = prepare_output(root, output)?;
    let mut steps = Vec::new();
    let result = match preflight_at(&External, root, &input, &output, &mut steps) {
        Ok(value) => value,
        Err(error) => json!({"schema":"ontology-engineering.runtime-preflight/v1",
            "status":"blocked","passed":false,"reason":error.to_string(),"steps":steps,
            "runtime_modified":false,"semantic_execution":"not_run"}),
    };
    write_new(&output.join("report.json"), &result)?;
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct DirectoryIdentity {
    device: u64,
    inode: u64,
}
fn directory(path: &Path) -> Result<DirectoryIdentity> {
    no_symlinks(path)?;
    let m = fs::symlink_metadata(path)?;
    ensure!(m.is_dir(), "runtime_path_not_directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(DirectoryIdentity {
            device: m.dev(),
            inode: m.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        anyhow::bail!("runtime_install_unsupported_platform")
    }
}
fn absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn wheel_inventory(path: &Path) -> Result<Value> {
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        ensure!(
            path.extension().is_some_and(|v| v == "whl"),
            "unexpected_prepared_wheel_entry"
        );
        let id = fingerprint_file(&path, WHEEL_LIMIT)?;
        entries.insert(
            path.file_name()
                .context("missing_wheel_name")?
                .to_string_lossy()
                .into_owned(),
            json!({"sha256":id.sha256,"bytes":id.bytes}),
        );
    }
    ensure!(!entries.is_empty(), "prepared_wheel_closure_empty");
    Ok(json!(entries))
}

struct Replacement {
    target: PathBuf,
    backup: PathBuf,
    failed: PathBuf,
    parent: DirectoryIdentity,
    original: Option<DirectoryIdentity>,
    created: Option<DirectoryIdentity>,
    old_moved: bool,
}
impl Replacement {
    fn new(root: &SkillRoot, output: &Path) -> Result<Self> {
        let target = root.path().join("runtime/.venv");
        let parent = directory(target.parent().unwrap())?;
        ensure!(
            directory(output)?.device == parent.device,
            "runtime_backup_requires_same_filesystem"
        );
        let original = if absent(&target)? {
            None
        } else {
            let id = directory(&target)?;
            resolve_executable(&target.join("bin/python"))
                .context("existing_runtime_incomplete_preserved")?;
            Some(id)
        };
        Ok(Self {
            target,
            backup: output.join("previous-venv"),
            failed: output.join("failed-venv"),
            parent,
            original,
            created: None,
            old_moved: false,
        })
    }
    fn parent_current(&self) -> Result<()> {
        ensure!(
            directory(self.target.parent().unwrap())? == self.parent,
            "runtime_parent_changed"
        );
        Ok(())
    }
    fn start(&mut self, output: &Path) -> Result<()> {
        self.parent_current()?;
        if let Some(original) = &self.original {
            ensure!(
                &directory(&self.target)? == original,
                "existing_runtime_changed"
            );
        } else {
            ensure!(absent(&self.target)?, "unexpected_runtime_target");
        }
        write_new(
            &output.join("replacement-intent.json"),
            &json!({"target":self.target,"backup":self.backup,"failed":self.failed,
                "original":self.original,"parent":self.parent,
                "recovery":"quarantine_owned_new_directory_then_restore_original_without_replacement"}),
        )?;
        if self.original.is_some() {
            rename_directory_exclusive(&self.target, &self.backup)?;
            self.old_moved = true;
            ensure!(
                Some(directory(&self.backup)?) == self.original,
                "runtime_backup_identity_changed"
            );
        }
        create_new_private_dir(&self.target)?;
        self.created = Some(directory(&self.target)?);
        write_new(
            &output.join("replacement-started.json"),
            &json!({"created":self.created,"old_moved":self.old_moved}),
        )?;
        Ok(())
    }
    fn target_current(&self) -> Result<()> {
        self.parent_current()?;
        ensure!(
            self.created.is_some() && Some(directory(&self.target)?) == self.created,
            "new_runtime_directory_changed"
        );
        Ok(())
    }
    fn recover(&self) -> Result<Value> {
        self.parent_current()?;
        if self.created.is_some() && !absent(&self.target)? {
            self.target_current()?;
            rename_directory_exclusive(&self.target, &self.failed)?;
            ensure!(
                Some(directory(&self.failed)?) == self.created,
                "failed_runtime_identity_changed"
            );
        } else if self.created.is_none() && !self.old_moved {
            return Ok(json!({"status":"original_untouched"}));
        }
        ensure!(absent(&self.target)?, "runtime_recovery_target_occupied");
        if self.old_moved {
            ensure!(
                Some(directory(&self.backup)?) == self.original,
                "runtime_recovery_backup_changed"
            );
            rename_directory_exclusive(&self.backup, &self.target)?;
            ensure!(
                Some(directory(&self.target)?) == self.original,
                "restored_runtime_identity_changed"
            );
        }
        Ok(
            json!({"status":if self.old_moved {"original_directory_restored"} else {"no_previous_runtime"},
            "failed_environment":self.failed,"byte_rollback_claimed":false}),
        )
    }
}

pub fn install(root: &SkillRoot, options: &InstallOptions) -> Result<Value> {
    ensure!(
        cfg!(feature = "historical-contract-tests"),
        "legacy_python_runtime_install_retired_in_v2"
    );
    install_with(root, options, &External)
}
fn install_with(
    root: &SkillRoot,
    options: &InstallOptions,
    backend: &dyn Backend,
) -> Result<Value> {
    let input = inputs(root, &options.python)?;
    // Validate existing target before network preparation. Recheck under the
    // exclusive guard immediately before replacement.
    let target = root.path().join("runtime/.venv");
    no_symlinks(&target)?;
    let initial_parent = directory(target.parent().unwrap())?;
    let initial_target = if !absent(&target)? {
        let identity = directory(&target)?;
        resolve_executable(&target.join("bin/python"))
            .context("existing_runtime_incomplete_preserved")?;
        Some(identity)
    } else {
        None
    };
    let wheelhouse = options
        .wheelhouse
        .as_ref()
        .map(|p| root.expand(p))
        .transpose()?;
    if let Some(path) = &wheelhouse {
        directory(path)?;
    }
    let output = prepare_output(root, &options.output)?;
    let mut steps = Vec::new();
    let mut replacement: Option<Replacement> = None;
    let mut access: Option<RuntimeAccessGuard> = None;
    let outcome = (|| -> Result<Value> {
        preflight_at(backend, root, &input, &output, &mut steps)?;
        write_new_bytes(&output.join("requirements.txt"), &input.requirements)?;
        let prep = output.join("prepare-venv");
        let mut args = arguments(&["-I", "-B", "-m", "venv"]);
        args.push(prep.clone().into_os_string());
        step(
            backend,
            root,
            &output,
            "prepare-venv",
            &input.python,
            args,
            &mut steps,
        )?;
        let prep_python = prep.join("bin/python");
        let wheels = output.join("wheels");
        let mut args = pip(&output, "wheel", false);
        args.extend(arguments(&["--quiet", "--wheel-dir"]));
        args.push(wheels.clone().into_os_string());
        args.push("--find-links".into());
        args.push(root.path().join("runtime/vendor").into_os_string());
        if options.offline {
            args.push("--no-index".into());
        }
        if let Some(path) = &wheelhouse {
            args.push("--find-links".into());
            args.push(path.clone().into_os_string());
        }
        args.push("--requirement".into());
        args.push(output.join("requirements.txt").into_os_string());
        args.push("pip".into());
        step(
            backend,
            root,
            &output,
            "prepare-full-dependencies",
            &prep_python,
            args,
            &mut steps,
        )?;
        let inventory = wheel_inventory(&wheels)?;
        write_new(&output.join("prepared-wheels.json"), &inventory)?;
        ensure!(
            inventory[&input.lock.artifact_filename]["sha256"] == input.lock.artifact_sha256,
            "prepared_semantica_differs_from_lock"
        );
        input.unchanged()?;
        ensure!(
            read_regular_bytes(&output.join("requirements.txt"), INPUT_LIMIT)?
                == input.requirements,
            "prepared_requirements_changed"
        );
        access = Some(RuntimeAccessGuard::exclusive(root.path())?);
        let mut swap = Replacement::new(root, &output)?;
        ensure!(
            swap.parent == initial_parent && swap.original == initial_target,
            "runtime_target_changed_during_preparation"
        );
        let start = swap.start(&output);
        replacement = Some(swap);
        start?;
        access.as_ref().unwrap().ensure_current()?;
        let mut args = arguments(&["-I", "-B", "-m", "venv"]);
        args.push(target.clone().into_os_string());
        step(
            backend,
            root,
            &output,
            "create-final-venv",
            &input.python,
            args,
            &mut steps,
        )?;
        let python = target.join("bin/python");
        for (id, extra) in [
            ("install-pip", arguments(&["--upgrade", "pip"])),
            (
                "install-full-dependencies",
                vec![
                    "--requirement".into(),
                    output.join("requirements.txt").into_os_string(),
                ],
            ),
            (
                "install-locked-semantica",
                vec![
                    "--force-reinstall".into(),
                    "--no-deps".into(),
                    input.wheel_path.clone().into_os_string(),
                ],
            ),
        ] {
            replacement.as_ref().unwrap().target_current()?;
            access.as_ref().unwrap().ensure_current()?;
            input.unchanged()?;
            ensure!(
                wheel_inventory(&wheels)? == inventory,
                "prepared_wheels_changed"
            );
            ensure!(
                read_regular_bytes(&output.join("requirements.txt"), INPUT_LIMIT)?
                    == input.requirements,
                "prepared_requirements_changed"
            );
            let mut args = pip(&output, "install", true);
            args.extend(arguments(&["--quiet", "--no-index", "--find-links"]));
            args.push(wheels.clone().into_os_string());
            args.extend(extra);
            step(backend, root, &output, id, &python, args, &mut steps)?;
        }
        step(
            backend,
            root,
            &output,
            "dependency-check",
            &python,
            pip(&output, "check", true),
            &mut steps,
        )?;
        let mut freeze = pip(&output, "freeze", true);
        freeze.push("--all".into());
        step(
            backend,
            root,
            &output,
            "installed-dependency-versions",
            &python,
            freeze,
            &mut steps,
        )?;
        let verification = backend.verify(root, &python, &target, &input.wheel, &input.lock)?;
        write_new(&output.join("installed-identity.json"), &verification)?;
        input.unchanged()?;
        ensure!(
            wheel_inventory(&wheels)? == inventory,
            "prepared_wheels_changed"
        );
        replacement.as_ref().unwrap().target_current()?;
        access.as_ref().unwrap().ensure_current()?;
        // Write the success receipt while recovery is still possible. Never
        // delete the previous environment; its original location is recorded.
        let ready = json!({"status":"ready","target":target,"verification":verification,
            "prepared_wheels":inventory,"previous_environment":replacement.as_ref().unwrap().original.as_ref().map(|_| &replacement.as_ref().unwrap().backup)});
        write_new(&output.join("ready.json"), &ready)?;
        Ok(ready)
    })();
    let result = match outcome {
        Ok(ready) => json!({"schema":"ontology-engineering.runtime-install/v1",
            "status":"ready","origin":backend.origin(),"installation":ready,
            "steps":steps,"source_lock":input.lock,"input_hashes":input.hashes,
            "semantic_execution":"not_run","service_switchover":"not_atomic",
            "lock_scope":"participating_Rust_native_calls_only"}),
        Err(error) => {
            let recovery = match replacement.as_ref() {
                None => json!({"status":"original_untouched"}),
                Some(value) => match access
                    .as_ref()
                    .context("runtime_guard_missing")
                    .and_then(RuntimeAccessGuard::ensure_current)
                    .and_then(|()| value.recover())
                {
                    Ok(value) => value,
                    Err(recovery_error) => json!({"status":"blocked",
                        "reason":recovery_error.to_string(),"target":value.target,
                        "backup":value.backup,"failed":value.failed,
                        "instruction":"inspect_retained_identity_and_step_receipts_before_manual_recovery"}),
                },
            };
            json!({"schema":"ontology-engineering.runtime-install/v1","status":"blocked",
                "origin":backend.origin(),"reason":error.to_string(),"steps":steps,
                "recovery":recovery,"semantic_execution":"not_run","runtime_ready":false})
        }
    };
    write_new(&output.join("report.json"), &result)?;
    drop(access);
    Ok(result)
}

#[cfg(test)]
#[path = "runtime_install_tests.rs"]
mod tests;
