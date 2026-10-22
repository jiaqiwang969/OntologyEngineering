//! Read-only inspection and bounded archive transport for historical source.
//! The Python/npm wheel builder is retired for every production feature set.

use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{
    SkillRoot, create_new_private_dir, create_new_regular_file, fingerprint_file, no_symlinks,
    open_regular_file, read_regular_bytes, sha256, write_new,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::runtime::{RuntimeSourceLock, read_runtime_source_lock};
use crate::{safe_relative, strict_json};

pub const MAX_SOURCE_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_SOURCE_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_SOURCE_MEMBERS: usize = 20_000;
const LOCK_LIMIT: usize = 1024 * 1024;
const STEP_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const SOURCE_TIMEOUT: Duration = Duration::from_secs(300);
const BUILD_TIMEOUT: Duration = Duration::from_secs(1800);
const FRONTEND_STEPS: [&str; 5] = [
    "npm ci",
    "npm run test:graph-store",
    "npm run test:graph-workspace",
    "npm run test:plugin-registry",
    "npm run build",
];

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub source_root: PathBuf,
    /// A new directory below this skill's var/. Never an existing release/vendor directory.
    pub output_dir: PathBuf,
    pub python: PathBuf,
    pub git: PathBuf,
    pub npm: PathBuf,
    pub node: PathBuf,
    /// Python build dependencies only. npm's cache/network policy is separate.
    pub wheelhouse: Option<PathBuf>,
    pub npm_offline: bool,
}

impl BuildOptions {
    pub fn new(source_root: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            source_root,
            output_dir,
            python: "python3".into(),
            git: "git".into(),
            npm: "npm".into(),
            node: "node".into(),
            wheelhouse: None,
            npm_offline: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildSpec {
    pub source_lock: RuntimeSourceLock,
    pub source_lock_sha256: String,
    pub pyproject_sha256: String,
    pub explorer_package_lock_sha256: String,
    pub source_date_epoch: u64,
    pub build_requirements: Vec<String>,
    pub frontend_steps: Vec<String>,
}

fn string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("build_lock_missing_field:{pointer}"))
}

fn hex(value: &str, n: usize) -> bool {
    value.len() == n
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn read_build_spec(root: &SkillRoot) -> Result<BuildSpec> {
    let path = root.path().join("runtime/semantica-source-lock.json");
    let bytes = read_regular_bytes(&path, LOCK_LIMIT)?;
    let raw = strict_json(&bytes)?;
    let identity = read_runtime_source_lock(root.path(), false)?;
    ensure!(
        string(&raw, "/source/commit")? == identity.commit
            && string(&raw, "/source/version")? == identity.version
            && string(&raw, "/artifact/filename")? == identity.artifact_filename
            && string(&raw, "/artifact/sha256")? == identity.artifact_sha256,
        "build_lock_identity_changed_during_read"
    );
    ensure!(
        read_regular_bytes(&path, LOCK_LIMIT)? == bytes,
        "build_lock_changed_during_read"
    );
    let pyproject = string(&raw, "/locked_inputs/pyproject_sha256")?;
    let explorer = string(&raw, "/locked_inputs/explorer_package_lock_sha256")?;
    ensure!(
        hex(pyproject, 64) && hex(explorer, 64),
        "build_input_digest_invalid"
    );
    let epoch = raw
        .pointer("/build/source_date_epoch")
        .and_then(Value::as_u64)
        .context("build_source_date_epoch_invalid")?;
    let mut requirements = Vec::new();
    for (field, name) in [
        ("python_builder", "build"),
        ("setuptools", "setuptools"),
        ("wheel", "wheel"),
    ] {
        let requirement = string(&raw, &format!("/build/{field}"))?;
        let version = requirement
            .strip_prefix(&format!("{name}=="))
            .context("build_requirement_must_pin_expected_tool")?;
        ensure!(
            !version.is_empty()
                && version
                    .split('.')
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())),
            "build_requirement_version_invalid"
        );
        requirements.push(requirement.to_owned());
    }
    let steps = raw
        .pointer("/build/frontend_steps")
        .and_then(Value::as_array)
        .context("build_frontend_steps_missing")?;
    ensure!(
        steps.iter().filter_map(Value::as_str).eq(FRONTEND_STEPS)
            && steps.len() == FRONTEND_STEPS.len(),
        "build_frontend_steps_unsupported"
    );
    ensure!(
        string(&raw, "/build/python_step")? == "python -m build --wheel --no-isolation",
        "build_python_step_unsupported"
    );
    Ok(BuildSpec {
        source_lock: identity,
        source_lock_sha256: sha256(&bytes),
        pyproject_sha256: pyproject.into(),
        explorer_package_lock_sha256: explorer.into(),
        source_date_epoch: epoch,
        build_requirements: requirements,
        frontend_steps: FRONTEND_STEPS.into_iter().map(str::to_owned).collect(),
    })
}

struct Operation<'a> {
    root: &'a SkillRoot,
    options: &'a BuildOptions,
    spec: BuildSpec,
    output: PathBuf,
    source: PathBuf,
    steps: Vec<Value>,
    started: Instant,
}

enum CommandKind {
    Source { absent_allowed: bool },
    Frontend,
}

impl<'a> Operation<'a> {
    fn new(root: &'a SkillRoot, options: &'a BuildOptions) -> Result<Self> {
        let spec = read_build_spec(root)?;
        let source = root.expand(&options.source_root)?;
        no_symlinks(&source)?;
        let source = fs::canonicalize(source).context("build_source_missing")?;
        ensure!(source.is_dir(), "build_source_not_directory");
        let output = root.private_path(&options.output_dir)?;
        ensure!(
            !output.starts_with(&source) && !source.starts_with(&output),
            "build_source_output_overlap"
        );
        create_new_private_dir(&output)?;
        create_new_private_dir(&output.join("commands"))?;
        for directory in ["tmp", "npm-cache", "pip-cache"] {
            create_new_private_dir(&output.join(directory))?;
        }
        Ok(Self {
            root,
            options,
            spec,
            output,
            source,
            steps: Vec::new(),
            started: Instant::now(),
        })
    }

    fn command(
        &mut self,
        label: &str,
        executable: &Path,
        args: Vec<OsString>,
        cwd: &Path,
        mut environment: BTreeMap<OsString, OsString>,
        kind: CommandKind,
    ) -> Result<Vec<u8>> {
        let index = self.steps.len() + 1;
        let evidence = self
            .output
            .join("commands")
            .join(format!("{index:03}-{label}"));
        environment.insert("TMPDIR".into(), self.output.join("tmp").into_os_string());
        environment.insert("TMP".into(), self.output.join("tmp").into_os_string());
        environment.insert("TEMP".into(), self.output.join("tmp").into_os_string());
        environment.insert(
            "npm_config_cache".into(),
            self.output.join("npm-cache").into_os_string(),
        );
        environment.insert(
            "NPM_CONFIG_CACHE".into(),
            self.output.join("npm-cache").into_os_string(),
        );
        environment.insert(
            "PIP_CACHE_DIR".into(),
            self.output.join("pip-cache").into_os_string(),
        );
        environment.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
        let request = CommandSpec {
            executable: executable.to_path_buf(),
            arguments: args,
            cwd: cwd.to_path_buf(),
            environment,
            remove_environment: [
                "PYTHONPATH",
                "PYTHONHOME",
                "VIRTUAL_ENV",
                "GIT_DIR",
                "GIT_WORK_TREE",
                "GIT_INDEX_FILE",
                "GIT_OBJECT_DIRECTORY",
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            timeout: if matches!(kind, CommandKind::Frontend) {
                BUILD_TIMEOUT
            } else {
                SOURCE_TIMEOUT
            },
            output_limit: STEP_OUTPUT_LIMIT,
        };
        let observation = match run_command(&request, &evidence) {
            Ok(value) => value,
            Err(error) => {
                self.steps
                    .push(json!({"step":label,"status":"observation_failed",
                    "evidence_dir":evidence,"error":format!("{error:#}")}));
                return Err(error).context(format!("build_step_observation_failed:{label}"));
            }
        };
        self.steps.push(json!({"step":label,"status":observation.status,
            "exit_code":observation.exit_code,"evidence_dir":evidence,"observation":observation.report}));
        let absent = matches!(
            kind,
            CommandKind::Source {
                absent_allowed: true
            }
        ) && observation.status == "process_failed"
            && observation.exit_code == Some(1)
            && observation.stdout.is_empty()
            && observation.stderr.is_empty();
        ensure!(
            observation.status == "completed" || absent,
            "build_step_failed:{label}:{}",
            observation.status
        );
        Ok(observation.stdout)
    }

    fn git(&mut self, label: &str, args: &[&str], absent_allowed: bool) -> Result<String> {
        let mut environment = BTreeMap::new();
        environment.insert("GIT_OPTIONAL_LOCKS".into(), "0".into());
        environment.insert("GIT_NO_LAZY_FETCH".into(), "1".into());
        environment.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
        let out = self.command(
            label,
            &self.options.git,
            args.iter().map(OsString::from).collect(),
            &self.source.clone(),
            environment,
            CommandKind::Source { absent_allowed },
        )?;
        String::from_utf8(out).context("git_output_not_utf8")
    }

    fn inspect(&mut self, initial: bool) -> Result<Value> {
        let prefix = if initial { "source" } else { "final-source" };
        let top = self.git(
            &format!("{prefix}-root"),
            &["rev-parse", "--show-toplevel"],
            false,
        )?;
        ensure!(
            Path::new(top.trim_end_matches('\n')) == self.source,
            "build_source_not_checkout_root"
        );
        let head = self.git(&format!("{prefix}-head"), &["rev-parse", "HEAD"], false)?;
        ensure!(
            head.trim_end_matches('\n') == self.spec.source_lock.commit,
            "build_source_commit_mismatch"
        );
        let shallow = self.git(
            &format!("{prefix}-shallow"),
            &["rev-parse", "--is-shallow-repository"],
            false,
        )?;
        ensure!(
            shallow.trim_end_matches('\n') == "false",
            "build_source_shallow"
        );
        let sparse = self.git(
            &format!("{prefix}-sparse"),
            &["config", "--bool", "--get", "core.sparseCheckout"],
            true,
        )?;
        ensure!(
            sparse.is_empty() || sparse.trim_end_matches('\n') == "false",
            "build_source_sparse"
        );
        let partial = self.git(
            &format!("{prefix}-partial"),
            &[
                "config",
                "--get-regexp",
                "^(extensions\\.partialclone|remote\\..*\\.promisor)$",
            ],
            true,
        )?;
        ensure!(partial.is_empty(), "build_source_partial_clone");
        let status = self.git(
            &format!("{prefix}-status"),
            &["status", "--porcelain", "--untracked-files=all"],
            false,
        )?;
        ensure!(status.is_empty(), "build_source_dirty");
        let pyproject =
            fingerprint_file(&self.source.join("pyproject.toml"), MAX_SOURCE_FILE_BYTES)?;
        let explorer = fingerprint_file(
            &self.source.join("explorer/package-lock.json"),
            MAX_SOURCE_FILE_BYTES,
        )?;
        ensure!(
            pyproject.sha256 == self.spec.pyproject_sha256,
            "build_pyproject_hash_mismatch"
        );
        ensure!(
            explorer.sha256 == self.spec.explorer_package_lock_sha256,
            "build_explorer_lock_hash_mismatch"
        );
        if initial {
            self.git("source-fsck", &["fsck", "--full", "--no-progress"], false)?;
        }
        ensure!(
            sha256(read_regular_bytes(
                &self.root.path().join("runtime/semantica-source-lock.json"),
                LOCK_LIMIT
            )?) == self.spec.source_lock_sha256,
            "build_source_lock_changed"
        );
        Ok(
            json!({"head":self.spec.source_lock.commit,"clean":true,"shallow":false,"sparse":false,
            "partial_clone":false,"pyproject":pyproject,"explorer_package_lock":explorer,
            "full_fsck_executed":initial}),
        )
    }

    fn finish(self, action: &str, result: Result<Value>) -> Result<Value> {
        let (status, result, error) = match result {
            Ok(value) => (
                if action == "inspect" {
                    "ready"
                } else {
                    "completed"
                },
                value,
                Value::Null,
            ),
            Err(error) => ("blocked", Value::Null, Value::String(format!("{error:#}"))),
        };
        let report = json!({"$schema":"ontology-engineering.runtime-source-build/v1",
            "action":action,"status":status,"source_root":self.source,"output_dir":self.output,
            "build_spec":self.spec,"steps":self.steps,"result":result,"error":error,
            "elapsed_seconds":self.started.elapsed().as_secs_f64(),
            "network_policy":{"python_build_dependencies":if self.options.wheelhouse.is_some() {"wheelhouse_only"} else {"configured_package_index"},
                "npm":if self.options.npm_offline {"offline_cache_only"} else {"configured_registry_or_cache"}},
            "limits":{"source_step_seconds":SOURCE_TIMEOUT.as_secs(),"build_step_seconds":BUILD_TIMEOUT.as_secs(),
                "per_stream_bytes":STEP_OUTPUT_LIMIT,"total_operation_timeout_enforced":false},
            "reproduction_scope":if action=="historical-frontend-archives" {"one original frontend invocation and two supplied historical archive snapshots; no Python/current Rust build"} else {"read-only original source qualification; runtime build not dispatched"},
            "installed_runtime_mutated_by_builder":false,"source_lock_mutated_by_builder":false,"semantic_execution":"not_run"});
        write_new(&self.output.join("build-report.json"), &report)?;
        Ok(report)
    }
}

/// Inspect the original checkout through bounded read-only Git commands. The new
/// output directory holds observations; this does not create a build environment.
pub fn inspect_source(root: &SkillRoot, options: &BuildOptions) -> Result<Value> {
    let mut operation = Operation::new(root, options)?;
    let result = operation.inspect(true);
    operation.finish("inspect", result)
}

/// The old wheel builder is retired for every feature set. Current Rust
/// capabilities are built by the source-bound Cargo/ggen pipeline.
pub fn build_semantica(_root: &SkillRoot, _options: &BuildOptions) -> Result<Value> {
    anyhow::bail!("legacy_python_runtime_build_retired_in_v2")
}

/// Reconcile original frontend behavior and two explicitly supplied wheel
/// archives. These remain historical source/assets, not a current Rust runtime
/// build, wheel execution, installation or reproduction qualification.
/// Current Rust software reproduction continues through the existing version
/// owner and its Cargo/ggen stage and native-input contracts.
pub fn capture_historical_frontend_archive_evidence(
    root: &SkillRoot,
    options: &BuildOptions,
    original_first: &Path,
    original_second: &Path,
) -> Result<Value> {
    let mut operation = Operation::new(root, options)?;
    let result = (|| -> Result<Value> {
        operation.inspect(true)?;
        let out = operation.output.clone();
        operation.command(
            "node-version",
            &options.node,
            vec!["--version".into()],
            &out,
            BTreeMap::new(),
            CommandKind::Frontend,
        )?;
        operation.command(
            "npm-version",
            &options.npm,
            vec!["--version".into()],
            &out,
            BTreeMap::new(),
            CommandKind::Frontend,
        )?;
        let archive = out.join("source.tar");
        let commit = operation.spec.source_lock.commit.clone();
        operation.git(
            "source-archive",
            &[
                "archive",
                "--format=tar",
                "--output",
                archive.to_str().context("archive_path_utf8")?,
                &commit,
            ],
            false,
        )?;
        let exported = out.join("source");
        extract_source_archive(&archive, &exported)?;
        let original_a = read_regular_bytes(original_first, MAX_SOURCE_ARCHIVE_BYTES as usize)?;
        let original_b = read_regular_bytes(original_second, MAX_SOURCE_ARCHIVE_BYTES as usize)?;
        let first = out.join("original-archive-a");
        let second = out.join("original-archive-b");
        create_new_private_dir(&first)?;
        create_new_private_dir(&second)?;
        let name = operation.spec.source_lock.artifact_filename.clone();
        oe_core::write_new_bytes(&first.join(&name), &original_a)?;
        oe_core::write_new_bytes(&second.join(&name), &original_b)?;
        for (label, args) in [
            ("npm-ci", vec!["ci"]),
            ("npm-test-graph-store", vec!["run", "test:graph-store"]),
            (
                "npm-test-graph-workspace",
                vec!["run", "test:graph-workspace"],
            ),
            (
                "npm-test-plugin-registry",
                vec!["run", "test:plugin-registry"],
            ),
            ("npm-build", vec!["run", "build"]),
        ] {
            let mut env = BTreeMap::new();
            env.insert(
                "npm_config_offline".into(),
                options.npm_offline.to_string().into(),
            );
            env.insert(
                "NPM_CONFIG_OFFLINE".into(),
                options.npm_offline.to_string().into(),
            );
            operation.command(
                label,
                &options.npm,
                args.into_iter().map(Into::into).collect(),
                &exported.join("explorer"),
                env,
                CommandKind::Frontend,
            )?;
        }
        operation.inspect(false)?;
        ensure!(
            original_a == original_b,
            "historical_archive_reproduction_mismatch"
        );
        ensure!(
            sha256(&original_a) == operation.spec.source_lock.artifact_sha256,
            "historical_archive_locked_identity_mismatch"
        );
        let wheel =
            crate::runtime::read_wheel_identity(&first.join(&name), &operation.spec.source_lock)?;
        let second_wheel =
            crate::runtime::read_wheel_identity(&second.join(&name), &operation.spec.source_lock)?;
        ensure!(
            wheel.identity() == second_wheel.identity(),
            "historical_archive_reproduction_mismatch"
        );
        oe_core::write_new_bytes(&out.join(&name), &original_a)?;
        Ok(
            json!({"frontend_builds":1,"provided_original_archives":2,"archive_pair_equal":true,
            "wheel_identity":wheel.identity(),"python_processes":0,"rust_software_builds":0,
            "scope":"Original frontend compatibility and supplied historical archives only; no current Rust reproduction inferred"}),
        )
    })();
    let mut report = operation.finish("historical-frontend-archives", result)?;
    report["current_runtime_execution"] = json!("not_run");
    report["python_build_dispatched"] = json!(false);
    report["reproduction_scope"] = json!(
        "one actual original frontend invocation; two supplied original archive byte snapshots, not wheel builds"
    );
    write_new(
        &root
            .private_path(&options.output_dir)?
            .join("historical-evidence-report.json"),
        &report,
    )?;
    Ok(report)
}

/// Safe bounded Git archive transport. No extract-all, links, device files,
/// duplicate members, traversal, or overwrites. Only a fresh destination exists.
pub fn extract_source_archive(archive: &Path, destination: &Path) -> Result<Value> {
    let before = fingerprint_file(archive, MAX_SOURCE_ARCHIVE_BYTES)?;
    ensure!(before.bytes % 512 == 0, "build_archive_truncated_block");
    create_new_private_dir(destination)?;
    let file = open_regular_file(archive, MAX_SOURCE_ARCHIVE_BYTES)?;
    let mut archive_reader = tar::Archive::new(file.take(MAX_SOURCE_ARCHIVE_BYTES + 1));
    let mut members = BTreeSet::new();
    let mut files = Vec::new();
    let mut expanded = 0_u64;
    for entry in archive_reader.entries()? {
        let mut entry = entry.context("build_archive_entry_invalid")?;
        let kind = entry.header().entry_type();
        // Git's global pax header carries a commit comment, never a filesystem member.
        if kind.is_pax_global_extensions() {
            continue;
        }
        ensure!(
            kind.is_file() || kind.is_dir(),
            "build_archive_nonregular_member"
        );
        let bytes = entry.path_bytes();
        let name = std::str::from_utf8(&bytes).context("build_archive_path_not_utf8")?;
        let name = name.trim_end_matches('/').to_owned();
        safe_relative(&name)?;
        ensure!(
            !name.split('/').any(|v| v == ".git"),
            "build_archive_git_metadata"
        );
        ensure!(
            members.insert(name.to_owned()),
            "build_archive_duplicate_member"
        );
        ensure!(
            members.len() <= MAX_SOURCE_MEMBERS,
            "build_archive_member_limit"
        );
        let size = entry.size();
        ensure!(size <= MAX_SOURCE_FILE_BYTES, "build_archive_file_limit");
        expanded = expanded
            .checked_add(size)
            .context("build_archive_size_overflow")?;
        ensure!(
            expanded <= MAX_SOURCE_ARCHIVE_BYTES,
            "build_archive_expanded_limit"
        );
        let target = destination.join(&name);
        if kind.is_dir() {
            ensure!(size == 0, "build_archive_directory_payload");
            ensure_directory(&target)?;
        } else {
            ensure_directory(target.parent().context("build_archive_missing_parent")?)?;
            let mut file = create_new_regular_file(&target)?;
            ensure!(
                std::io::copy(&mut entry.by_ref().take(size + 1), &mut file)? == size,
                "build_archive_truncated_member"
            );
            file.flush()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(
                    if entry.header().mode()? & 0o111 == 0 {
                        0o644
                    } else {
                        0o755
                    },
                ))?;
            }
            let identity = fingerprint_file(&target, MAX_SOURCE_FILE_BYTES)?;
            files.push(json!({"path":name,"bytes":identity.bytes,"sha256":identity.sha256}));
        }
    }
    // Consume all physical trailing bytes. Nonzero trailing data is not another archive.
    let mut tail = archive_reader.into_inner();
    let mut buffer = [0_u8; 64 * 1024];
    let mut tail_bytes = 0_u64;
    loop {
        let n = tail.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        tail_bytes += n as u64;
        ensure!(
            buffer[..n].iter().all(|b| *b == 0),
            "build_archive_trailing_data"
        );
    }
    // tar stops at the first zero block; require at least its second footer block.
    ensure!(tail_bytes >= 512, "build_archive_missing_footer");
    ensure!(
        fingerprint_file(archive, MAX_SOURCE_ARCHIVE_BYTES)? == before,
        "build_archive_changed"
    );
    ensure!(!files.is_empty(), "build_archive_empty");
    Ok(
        json!({"archive":before,"members":members.len(),"regular_files":files.len(),
        "expanded_bytes":expanded,"files":files}),
    )
}

fn ensure_directory(path: &Path) -> Result<()> {
    no_symlinks(path)?;
    match fs::symlink_metadata(path) {
        Ok(meta) => ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "build_archive_parent_not_directory"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_new_private_dir(path)?,
        Err(error) => return Err(error).context("build_archive_parent_unavailable"),
    }
    Ok(())
}
