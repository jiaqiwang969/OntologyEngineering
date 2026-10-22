//! New-only materialization and retained Windows input handles. No compiler,
//! child process, NX library, job dispatch, or authenticated execution result.
use crate::{
    adapter,
    build::{MAX_BUILD_TREE_BYTES, MAX_BUILD_TREE_FILES, normalize},
    build_config::{CargoConfigRecord, PreparedCargoConfig, prepare_config},
    build_execution::{ExecutionSpecV2, PreparedExecutionPlan},
    build_source::VerifiedSourceArchive,
    windows::{PinnedDirectory, PinnedFile, StableRoot},
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    BlobIdentity, Digest, FileEntry, JobId, MAX_PAYLOAD_BYTES, MAX_RECORD_BYTES, RelativePath,
    RootIdentity,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryObservation {
    pub final_path: String,
    pub volume_id: String,
    pub directory_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputObservation {
    pub role: String,
    pub requested_path: String,
    pub final_path: String,
    pub volume_id: String,
    pub file_id: String,
    pub bytes: BlobIdentity,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AncestorConfigObservation {
    pub directory: DirectoryObservation,
    pub cargo_directory: Option<DirectoryObservation>,
    pub configuration_files_absent: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializationReceipt {
    pub schema: String,
    pub status: String,
    pub build_id: JobId,
    pub plan_digest: Digest,
    pub root: RootIdentity,
    pub output: DirectoryObservation,
    pub workspace: DirectoryObservation,
    pub source_inventory: FileEntry,
    pub source_files: u32,
    pub source_bytes: u64,
    pub cargo_config: CargoConfigRecord,
    pub selected_inputs: Vec<InputObservation>,
    pub retained_directories: Vec<DirectoryObservation>,
    pub ancestor_config: Vec<AncestorConfigObservation>,
    pub source_scope: String,
    pub toolchain_scope: String,
    pub configuration_scope: String,
    pub execution: String,
    pub automatic_retry: bool,
}

/// Owns live file/ancestor handles, not execution success. There is no decoding
/// or external-receipt constructor. Dropping it releases pins but keeps files.
pub struct PreparedBuild<'a> {
    output: PinnedDirectory<'a>,
    workspace: PinnedDirectory<'a>,
    cargo_home: PinnedDirectory<'a>,
    receipt: MaterializationReceipt,
    source_expected: BTreeMap<String, BlobIdentity>,
    _source_pins: Vec<PinnedFile>,
    input_pins: Vec<PinnedFile>,
    directory_pins: Vec<StableRoot>,
    _owned_directories: Vec<PinnedDirectory<'a>>,
    config: PreparedCargoConfig,
    ancestor_start: PathBuf,
}
impl PreparedBuild<'_> {
    pub fn receipt(&self) -> &MaterializationReceipt {
        &self.receipt
    }
    pub fn output_path(&self) -> &Path {
        self.output.path()
    }
    /// Fresh observation, not an atomic exclusion of future directory additions.
    /// A future runner must perform this again immediately before launching.
    pub fn revalidate_source_and_inputs(&mut self) -> Result<()> {
        self.output.verify()?;
        self.workspace.verify()?;
        require_members(
            &self.output,
            &[
                "build-intent.json",
                "source",
                "cargo-home",
                "receipts",
                "target",
                "temp",
                "artifacts",
            ],
        )?;
        require_members(&self._owned_directories[0], &["workspace"])?;
        for fresh in self._owned_directories.iter().skip(2) {
            require_members(fresh, &[])?;
        }
        for pin in &self.directory_pins {
            pin.verify()?;
        }
        for pin in &mut self.input_pins {
            pin.verify()?;
        }
        for pin in &mut self._source_pins {
            pin.verify()?;
        }
        let (entries, _pins) = exact_source_tree(&self.workspace, &self.source_expected)?;
        self.receipt
            .source_inventory
            .identity
            .verify(&serde_json::to_vec(&entries)?)?;
        require_members(&self.cargo_home, &["config.toml"])?;
        let mut config = self
            .cargo_home
            .pin_file(OsStr::new("config.toml"), MAX_RECORD_BYTES as u64)?
            .context("generated Cargo config disappeared")?;
        self.config
            .record()
            .generated_config
            .identity
            .verify(&config.read_all(MAX_RECORD_BYTES as u64)?)?;
        let (ancestors, _pins) = observe_ancestor_configs(&self.ancestor_start)?;
        ensure!(
            ancestors == self.receipt.ancestor_config,
            "ancestor Cargo configuration observation changed"
        );
        Ok(())
    }
}

pub fn materialize<'a>(
    root: &'a StableRoot,
    spec: &ExecutionSpecV2,
    plan: &PreparedExecutionPlan,
    source: &VerifiedSourceArchive,
) -> Result<PreparedBuild<'a>> {
    plan.record().validate_against(spec, source)?;
    let config = prepare_config(plan, source)?;
    ensure!(
        adapter::root_identity(root) == spec.build.root_identity,
        "materialization root identity differs"
    );
    ensure!(
        root.as_directory()?
            .pin_file(
                OsStr::new(crate::windows::LOCK_NAME),
                MAX_RECORD_BYTES as u64
            )?
            .is_none(),
        "NX writer root cannot be used as a build materialization root"
    );
    // Also bind the pathname used later by the plan's argv to this exact root.
    let declared_root = StableRoot::open(Path::new(&spec.build.root_directory), None)?;
    ensure!(
        adapter::root_identity(&declared_root) == spec.build.root_identity,
        "declared root pathname resolves differently"
    );
    let (_, mut directory_pins) = observe_ancestor_configs(declared_root.path())?;
    directory_pins.push(declared_root);
    let (selected_inputs, mut input_pins, mut installed_directories) = pin_installed_inputs(spec)?;
    directory_pins.append(&mut installed_directories);
    // No mutation precedes input/config preflight. Any existing build-ID tree
    // is rejected here, without opening, deleting, resuming or overwriting it.
    let output = root.create_directory_new(OsStr::new(&format!(
        "build-{}",
        spec.build.build_id.as_str()
    )))?;
    let mut phase = "write_intent";
    let attempt = (|| -> Result<_> {
        write_record(
            &output,
            "build-intent.json",
            &serde_json::json!({
                "schema":"cad-agent.nx-build-materialization-intent/v1",
                "build_id":spec.build.build_id,"plan_digest":plan.digest()?,
                "root":spec.build.root_identity,"source":plan.record().source,
                "execution":"not_started","automatic_retry":false
            }),
        )?;
        phase = "create_owned_directories";
        let source_parent = output.create_directory_new(OsStr::new("source"))?;
        let workspace = source_parent.create_directory_new(OsStr::new("workspace"))?;
        let cargo_home = output.create_directory_new(OsStr::new("cargo-home"))?;
        let receipts = output.create_directory_new(OsStr::new("receipts"))?;
        let mut owned = vec![source_parent, receipts];
        for name in ["target", "temp", "artifacts"] {
            owned.push(output.create_directory_new(OsStr::new(name))?);
        }
        phase = "materialize_verified_source";
        for (name, bytes) in source.members() {
            create_source_new(&workspace, &RelativePath::new(name)?, bytes)?;
        }
        phase = "verify_complete_source_tree";
        let expected = source
            .members()
            .iter()
            .map(|(n, b)| (n.clone(), BlobIdentity::of(b)))
            .collect();
        let (inventory, source_pins) = exact_source_tree(&workspace, &expected)?;
        let inventory_raw = serde_json::to_vec(&inventory)?;
        let inventory_entry =
            FileEntry::from_bytes("receipts/source-inventory.json", &inventory_raw)?;
        owned[1].create_file_new(
            OsStr::new("source-inventory.json"),
            &inventory_raw,
            MAX_RECORD_BYTES as u64,
        )?;
        phase = "write_owned_cargo_config";
        cargo_home.create_file_new(
            OsStr::new("config.toml"),
            config.bytes(),
            MAX_RECORD_BYTES as u64,
        )?;
        let mut generated = cargo_home
            .pin_file(OsStr::new("config.toml"), MAX_RECORD_BYTES as u64)?
            .context("new Cargo config missing")?;
        ensure!(
            adapter::file_identity(&mut generated)? == config.record().generated_config.identity,
            "generated Cargo config changed"
        );
        input_pins.push(generated);
        require_members(&cargo_home, &["config.toml"])?;
        phase = "observe_applicable_config_ancestors";
        let ancestor_start = workspace
            .path()
            .parent()
            .context("workspace has no parent")?
            .to_owned();
        let (ancestor_config, mut ancestor_pins) = observe_ancestor_configs(&ancestor_start)?;
        directory_pins.append(&mut ancestor_pins);
        let receipt = MaterializationReceipt {
            schema: "cad-agent.nx-build-materialization/v1".into(),
            status: "prepared_not_executed".into(),
            build_id: spec.build.build_id.clone(), plan_digest: plan.digest()?,
            root: spec.build.root_identity.clone(), output: directory_observation(&output),
            workspace: directory_observation(&workspace), source_inventory: inventory_entry,
            source_files: source.assessment().expanded_files, source_bytes: source.assessment().expanded_bytes,
            cargo_config: config.record().clone(), selected_inputs, retained_directories: directory_pins.iter().map(root_observation).collect(),
            ancestor_config,
            source_scope: "complete_byte_and_member_readback_with_retained_regular_file_pins".into(),
            toolchain_scope: "selected_file_bytes_and_ids_only_not_executable_roles_or_transitive_dependency_closure".into(),
            configuration_scope: "point_in_time_absence_requires_prelaunch_recheck".into(),
            execution: "not_started".into(), automatic_retry: false,
        };
        Ok((
            workspace,
            cargo_home,
            receipt,
            expected,
            source_pins,
            owned,
            ancestor_start,
        ))
    })();
    match attempt {
        Ok((
            workspace,
            cargo_home,
            receipt,
            source_expected,
            source_pins,
            owned,
            ancestor_start,
        )) => {
            let mut prepared = PreparedBuild {
                output,
                workspace,
                cargo_home,
                receipt,
                source_expected,
                _source_pins: source_pins,
                input_pins,
                directory_pins,
                _owned_directories: owned,
                config,
                ancestor_start,
            };
            if let Err(error) = prepared.revalidate_source_and_inputs() {
                return Err(record_failure(
                    &prepared.output,
                    "final_revalidation",
                    error,
                    plan,
                ));
            }
            if let Err(error) = write_record(
                &prepared._owned_directories[1],
                "materialization.json",
                &prepared.receipt,
            ) {
                return Err(record_failure(
                    &prepared.output,
                    "persist_prepared_receipt",
                    error,
                    plan,
                ));
            }
            Ok(prepared)
        }
        Err(error) => Err(record_failure(&output, phase, error, plan)),
    }
}

fn write_record<T: Serialize>(dir: &PinnedDirectory<'_>, name: &str, record: &T) -> Result<()> {
    dir.create_file_new(
        OsStr::new(name),
        &serde_json::to_vec(record)?,
        MAX_RECORD_BYTES as u64,
    )?;
    Ok(())
}
fn record_failure(
    dir: &PinnedDirectory<'_>,
    phase: &str,
    primary: anyhow::Error,
    plan: &PreparedExecutionPlan,
) -> anyhow::Error {
    let message: String = format!("{primary:#}").chars().take(4096).collect();
    let record = serde_json::json!({"schema":"cad-agent.nx-build-materialization-failure/v1",
        "status":"preparation_failed","phase":phase,"error":message,
        "build_id":plan.record().build_id,"spec_digest":plan.record().spec_digest,
        "execution":"not_started","automatic_retry":false,"partial_directory_retained":true});
    match write_record(dir, "materialization-failed.json", &record) {
        Ok(()) => primary,
        Err(recording) => {
            primary.context(format!("failure receipt could not be saved: {recording:#}"))
        }
    }
}
fn directory_observation(dir: &PinnedDirectory<'_>) -> DirectoryObservation {
    DirectoryObservation {
        final_path: dir.path().to_string_lossy().into_owned(),
        volume_id: dir.identity().volume_id.clone(),
        directory_id: dir.identity().file_id.clone(),
    }
}
fn root_observation(root: &StableRoot) -> DirectoryObservation {
    DirectoryObservation {
        final_path: root.path().to_string_lossy().into_owned(),
        volume_id: root.identity().volume_id.clone(),
        directory_id: root.identity().directory_id.clone(),
    }
}

fn observe_ancestor_configs(
    start: &Path,
) -> Result<(Vec<AncestorConfigObservation>, Vec<StableRoot>)> {
    let mut records = Vec::new();
    let mut pins = Vec::new();
    for path in start.ancestors() {
        if !path.is_absolute() {
            continue;
        }
        let parent = StableRoot::open(path, None)?;
        let dir = parent.as_directory()?;
        let cargo = match dir.directory(OsStr::new(".cargo")) {
            Ok(cargo) => {
                for name in ["config", "config.toml"] {
                    ensure!(
                        cargo
                            .pin_file(OsStr::new(name), MAX_RECORD_BYTES as u64)?
                            .is_none(),
                        "external ancestor Cargo configuration is unsupported"
                    );
                }
                let observed = directory_observation(&cargo);
                // Retain the directory/ancestor identity even after this view ends.
                pins.push(StableRoot::open(cargo.path(), None)?);
                Some(observed)
            }
            Err(error) if error.raw_os_error() == Some(2) => None,
            Err(error) => return Err(error.into()), // reparse is never absence
        };
        records.push(AncestorConfigObservation {
            directory: root_observation(&parent),
            cargo_directory: cargo,
            configuration_files_absent: true,
        });
        pins.push(parent);
    }
    ensure!(
        !records.is_empty(),
        "no absolute configuration ancestor observed"
    );
    Ok((records, pins))
}

fn pin_installed_inputs(
    spec: &ExecutionSpecV2,
) -> Result<(Vec<InputObservation>, Vec<PinnedFile>, Vec<StableRoot>)> {
    let build = &spec.build;
    // A shared rustup proxy is not a resolved Cargo+rustc pair.
    ensure!(
        build.tools.cargo.identity != build.tools.rustc.identity,
        "Cargo and rustc declarations share proxy bytes"
    );
    let mut declarations = vec![];
    for (role, tool) in [
        ("cargo", &build.tools.cargo),
        ("rustc", &build.tools.rustc),
        ("gnu_linker", &build.tools.gnu_linker),
        ("msvc_cl", &build.tools.msvc_cl),
        ("msvc_link", &build.tools.msvc_link),
    ] {
        declarations.push((role.to_owned(), tool.path.clone(), tool.identity.clone()));
    }
    for (role, files) in [
        ("sdk_selected", &build.sdk.files),
        ("nx_runtime_identity_only", &build.sdk.runtime.libraries),
    ] {
        for entry in files {
            declarations.push((
                role.into(),
                format!("{}/{}", normalize(&build.sdk.root), entry.path.as_str()),
                entry.identity.clone(),
            ));
        }
    }
    let mut observations = Vec::new();
    let mut pins = Vec::new();
    for (role, path, identity) in declarations {
        let pathbuf = PathBuf::from(&path);
        let root = StableRoot::open(
            pathbuf.parent().context("installed input has no parent")?,
            None,
        )?;
        let dir = root.as_directory()?;
        let mut file = dir
            .pin_file(
                pathbuf
                    .file_name()
                    .context("installed input has no filename")?,
                MAX_PAYLOAD_BYTES,
            )?
            .context("declared installed input missing")?;
        ensure!(
            adapter::file_identity(&mut file)? == identity,
            "selected installed file bytes differ"
        );
        observations.push(InputObservation {
            role,
            requested_path: path,
            final_path: file.path().to_string_lossy().into_owned(),
            volume_id: file.identity().volume_id.clone(),
            file_id: file.identity().file_id.clone(),
            bytes: identity,
        });
        pins.push(file);
    }
    let mut directories = Vec::new();
    for path in std::iter::once(&build.tools.windows_directory)
        .chain(build.tools.include_directories.iter())
        .chain(build.tools.library_directories.iter())
    {
        directories.push(StableRoot::open(Path::new(path), None)?);
    }
    Ok((observations, pins, directories))
}

fn create_source_new(dir: &PinnedDirectory<'_>, path: &RelativePath, bytes: &[u8]) -> Result<()> {
    fn inner(dir: &PinnedDirectory<'_>, parts: &[&str], bytes: &[u8]) -> Result<()> {
        if parts.len() == 1 {
            dir.create_file_new(OsStr::new(parts[0]), bytes, MAX_BUILD_TREE_BYTES)?;
            return Ok(());
        }
        let child = match dir.directory(OsStr::new(parts[0])) {
            Ok(dir) => dir,
            Err(error) if error.raw_os_error() == Some(2) => {
                dir.create_directory_new(OsStr::new(parts[0]))?
            }
            Err(error) => return Err(error.into()),
        };
        inner(&child, &parts[1..], bytes)
    }
    inner(dir, &path.as_str().split('/').collect::<Vec<_>>(), bytes)
}

fn exact_source_tree(
    dir: &PinnedDirectory<'_>,
    expected: &BTreeMap<String, BlobIdentity>,
) -> Result<(Vec<FileEntry>, Vec<PinnedFile>)> {
    ensure!(
        !expected.is_empty() && expected.len() <= MAX_BUILD_TREE_FILES as usize,
        "build inventory count exceeds bound"
    );
    let mut directories = BTreeSet::new();
    for name in expected.keys() {
        let mut prefix = String::new();
        let parts: Vec<_> = name.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            directories.insert(prefix.clone());
        }
    }
    struct Scan<'e> {
        expected: &'e BTreeMap<String, BlobIdentity>,
        directories: &'e BTreeSet<String>,
        found: BTreeMap<String, BlobIdentity>,
        pins: Vec<PinnedFile>,
        total: u64,
    }
    fn walk(dir: &PinnedDirectory<'_>, prefix: &str, scan: &mut Scan<'_>) -> Result<()> {
        for entry in std::fs::read_dir(dir.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().context("source member is not Unicode")?;
            let path = RelativePath::new(format!("{prefix}{name}"))?;
            if entry.file_type()?.is_dir() {
                ensure!(
                    scan.directories.contains(path.as_str()),
                    "unexpected source directory"
                );
                let child = dir.directory(OsStr::new(name))?;
                walk(&child, &format!("{}/", path.as_str()), scan)?;
            } else {
                let expected = scan
                    .expected
                    .get(path.as_str())
                    .context("unexpected source member")?;
                let mut pin = dir
                    .pin_file(OsStr::new(name), MAX_BUILD_TREE_BYTES)?
                    .context("source file disappeared")?;
                let actual = adapter::file_identity(&mut pin)?;
                ensure!(&actual == expected, "materialized source bytes differ");
                scan.total = scan
                    .total
                    .checked_add(actual.bytes)
                    .context("source byte count overflow")?;
                ensure!(
                    scan.total <= MAX_BUILD_TREE_BYTES
                        && scan.found.len() < MAX_BUILD_TREE_FILES as usize,
                    "build inventory exceeds bound"
                );
                ensure!(
                    scan.found.insert(path.as_str().into(), actual).is_none(),
                    "duplicate source member"
                );
                scan.pins.push(pin);
            }
        }
        dir.verify()?;
        Ok(())
    }
    let mut scan = Scan {
        expected,
        directories: &directories,
        found: BTreeMap::new(),
        pins: Vec::new(),
        total: 0,
    };
    walk(dir, "", &mut scan)?;
    ensure!(
        &scan.found == expected,
        "materialized source inventory differs"
    );
    let inventory = scan
        .found
        .into_iter()
        .map(|(path, identity)| {
            Ok(FileEntry {
                path: RelativePath::new(path)?,
                identity,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((inventory, scan.pins))
}
fn require_members(dir: &PinnedDirectory<'_>, expected: &[&str]) -> Result<()> {
    let names = std::fs::read_dir(dir.path())?
        .map(|e| {
            e?.file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("nonunicode owned member"))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        names == expected.iter().map(|s| s.to_string()).collect(),
        "owned directory has unexpected members"
    );
    dir.verify()?;
    Ok(())
}
