//! Finite worker-build planning and external-record packaging, separate from NX
//! job dispatch. This module never starts a compiler or authenticates a caller's
//! execution claims. A Windows build-only runner remains a separate integration.
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    BlobIdentity, Capability, Digest, FileEntry, JobId, MAX_FILES, MAX_PAYLOAD_BYTES,
    MAX_RECORD_BYTES, RelativePath, RootIdentity, RuntimeIdentity, WORKER_SCHEMA, WorkerManifest,
    bundle::insert, decode, record_digest, validate_inventory,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const BUILD_SPEC_SCHEMA: &str = "cad-agent.nx-worker-build-spec/v1";
pub const BUILD_EVIDENCE_SCHEMA: &str = "cad-agent.nx-worker-build-observations/v1";
pub const BUILD_RECIPE_SCHEMA: &str = "cad-agent.nx-worker-build-recipe/v1";
// Compilation trees are not job payloads: the received, fully vendored workspace
// already exceeds the job ZIP's expanded limits. These bounds never change the
// 100 MiB / 10,000-file protocol or worker-package limits.
pub const MAX_BUILD_TREE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_BUILD_TREE_FILES: u32 = 20_000;
const ARCHIVE: &str = "sources/workspace.zip";
const SOURCE_INVENTORY: &str = "sources/workspace-files.json";
const LOCK: &str = "sources/Cargo.lock";
const EXE: &str = "worker/oe-nx-worker.exe";
const SHIM: &str = "worker/nx_shim.dll";
const PLAN_FILE: &str = "sources/build/plan.json";
const SPEC_FILE: &str = "sources/build/spec.json";
const EVIDENCE_FILE: &str = "sources/build/observations.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledFile {
    /// An actual installed executable/file, not a rustup proxy or shell command.
    /// This is a declaration; only a future runner can observe the actual file.
    pub path: String,
    pub identity: BlobIdentity,
}
impl InstalledFile {
    fn validate(&self) -> Result<()> {
        windows_path(&self.path)?;
        ensure!(
            self.identity.bytes > 0 && self.identity.bytes <= MAX_PAYLOAD_BYTES,
            "installed file identity outside bound"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildTools {
    pub cargo: InstalledFile,
    pub rustc: InstalledFile,
    pub gnu_linker: InstalledFile,
    pub msvc_cl: InstalledFile,
    pub msvc_link: InstalledFile,
    pub windows_directory: String,
    /// Explicit MSVC/Windows Kit search directories, not a caller-supplied shell
    /// environment. Their contents are NOT authenticated by this planning core.
    pub include_directories: Vec<String>,
    pub library_directories: Vec<String>,
}
impl BuildTools {
    fn files(&self) -> [&InstalledFile; 5] {
        [
            &self.cargo,
            &self.rustc,
            &self.gnu_linker,
            &self.msvc_cl,
            &self.msvc_link,
        ]
    }
    fn validate(&self) -> Result<()> {
        for file in self.files() {
            file.validate()?;
        }
        for (file, name) in [
            (&self.cargo, "cargo.exe"),
            (&self.rustc, "rustc.exe"),
            (&self.msvc_cl, "cl.exe"),
            (&self.msvc_link, "link.exe"),
        ] {
            ensure!(
                basename(&file.path).eq_ignore_ascii_case(name),
                "unexpected compiler executable name"
            );
        }
        ensure!(
            matches!(
                basename(&self.gnu_linker.path)
                    .to_ascii_lowercase()
                    .as_str(),
                "gcc.exe" | "x86_64-w64-mingw32-gcc.exe"
            ),
            "unsupported GNU linker name"
        );
        ensure!(
            parent(&self.msvc_cl.path).eq_ignore_ascii_case(&parent(&self.msvc_link.path)),
            "MSVC compiler/linker directory differs"
        );
        windows_path(&self.windows_directory)?;
        for directories in [&self.include_directories, &self.library_directories] {
            ensure!(
                !directories.is_empty() && directories.len() <= 32,
                "invalid compiler search directories"
            );
            let mut seen = BTreeSet::new();
            for directory in directories {
                windows_path(directory)?;
                ensure!(
                    seen.insert(normalize(directory).to_ascii_lowercase()),
                    "duplicate compiler search directory"
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledSdk {
    pub root: String,
    /// Selected headers and import libraries observed on the licensed target.
    /// This list is not a claim of the complete transitive SDK/toolchain closure.
    pub files: Vec<FileEntry>,
    pub runtime: RuntimeIdentity,
}
impl InstalledSdk {
    fn validate(&self) -> Result<()> {
        windows_path(&self.root)?;
        self.runtime.validate()?;
        ensure!(
            self.runtime.product == "Siemens NX" && self.runtime.architecture == "x86_64",
            "SDK/runtime target mismatch"
        );
        ensure!(
            self.runtime.libraries.len() == 3
                && [
                    "NXBIN/libufun.dll",
                    "NXBIN/libnxopencpp.dll",
                    "NXBIN/libnxopencpp_assemblies.dll",
                ]
                .iter()
                .all(|name| {
                    self.runtime
                        .libraries
                        .iter()
                        .any(|file| file.path.as_str() == *name && file.identity.bytes > 0)
                }),
            "fixed native worker runtime libraries differ"
        );
        validate_inventory(&self.files)?;
        for file in &self.files {
            ensure!(
                file.path.as_str().starts_with("UGOPEN/"),
                "SDK identity outside UGOPEN"
            );
        }
        for required in [
            "UGOPEN/uf.h",
            "UGOPEN/uf_defs.h",
            "UGOPEN/uf_part.h",
            "UGOPEN/uf_modl_types.h",
            "UGOPEN/uf_object_types.h",
            "UGOPEN/NXOpen/Session.hxx",
            "UGOPEN/libufun.lib",
            "UGOPEN/libnxopencpp.lib",
            "UGOPEN/libnxopencpp_assemblies.lib",
        ] {
            ensure!(
                self.files
                    .iter()
                    .any(|file| file.path.as_str() == required && file.identity.bytes > 0),
                "missing explicit SDK input identity"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    Debug,
    Release,
}
impl BuildProfile {
    fn directory(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSpec {
    pub schema: String,
    pub build_id: JobId,
    /// A dedicated build-input parent, never a remote-profile/default NX jobroot.
    pub root_directory: String,
    pub root_identity: RootIdentity,
    pub source_archive: FileEntry,
    pub source_inventory: FileEntry,
    pub cargo_lock: FileEntry,
    pub expanded_source_files: u32,
    pub expanded_source_bytes: u64,
    pub tools: BuildTools,
    pub sdk: InstalledSdk,
    pub profile: BuildProfile,
    pub jobs: u8,
    pub max_seconds: u16,
    pub worker_id: String,
    pub worker_version: String,
    pub declared_capabilities: Vec<Capability>,
}
impl BuildSpec {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == BUILD_SPEC_SCHEMA,
            "build specification schema mismatch"
        );
        windows_path(&self.root_directory)?;
        self.root_identity.validate()?;
        for (entry, path) in [
            (&self.source_archive, ARCHIVE),
            (&self.source_inventory, SOURCE_INVENTORY),
            (&self.cargo_lock, LOCK),
        ] {
            ensure!(
                entry.path.as_str() == path && entry.identity.bytes > 0,
                "build source path/identity mismatch"
            );
        }
        validate_inventory(&[
            self.source_archive.clone(),
            self.source_inventory.clone(),
            self.cargo_lock.clone(),
        ])?;
        ensure!(
            self.source_inventory.identity.bytes <= MAX_RECORD_BYTES as u64
                && self.cargo_lock.identity.bytes <= MAX_RECORD_BYTES as u64,
            "source metadata exceeds record bound"
        );
        ensure!(
            self.expanded_source_files > 0
                && self.expanded_source_files <= MAX_BUILD_TREE_FILES
                && self.expanded_source_bytes > 0
                && self.expanded_source_bytes <= MAX_BUILD_TREE_BYTES,
            "expanded build tree exceeds separate bound"
        );
        ensure!(
            (1..=8).contains(&self.jobs) && (1..=1800).contains(&self.max_seconds),
            "build budget outside bound"
        );
        self.tools.validate()?;
        self.sdk.validate()?;
        for capability in &self.declared_capabilities {
            ensure!(
                matches!(
                    capability.action.as_str(),
                    "create" | "inspect" | "import_step"
                ) && capability.params_schema == "cad-agent.nx-native-acceptance-params/v2",
                "capability outside fixed native worker interface"
            );
        }
        // Reuse the current worker protocol for identifiers/capabilities/target
        // validation; no new worker manifest or capability registry is defined.
        self.worker_manifest(
            FileEntry::from_bytes(EXE, b"identity-shape-only")?,
            FileEntry::from_bytes(SHIM, b"identity-shape-only")?,
            vec![self.source_archive.clone()],
            FileEntry::from_bytes("build/recipe.json", b"identity-shape-only")?,
        )
        .validate()?;
        Ok(())
    }
    pub fn digest(&self) -> Result<Digest> {
        self.validate()?;
        record_digest(self)
    }
    /// Inspect the actual archive bytes, not a caller's source-tree observation.
    /// The returned assessment does not authenticate a compiler or disk tree.
    pub fn verify_sources(
        &self,
        archive: &[u8],
        inventory: &[u8],
        lock: &[u8],
    ) -> Result<crate::build_source::VerifiedSourceArchive> {
        self.validate()?;
        self.source_archive.identity.verify(archive)?;
        self.source_inventory.identity.verify(inventory)?;
        self.cargo_lock.identity.verify(lock)?;
        let source = crate::build_source::verify_source_archive(archive, inventory, lock)?;
        ensure!(
            source.assessment().expanded_files == self.expanded_source_files
                && source.assessment().expanded_bytes == self.expanded_source_bytes,
            "actual source expansion differs from build declaration"
        );
        Ok(source)
    }
    pub fn plan(&self) -> Result<BuildPlan> {
        self.validate()?;
        let root = normalize(&self.root_directory);
        let output = format!("{root}/build-{}", self.build_id.as_str());
        let workspace = format!("{root}/source/workspace");
        let mut path = BTreeSet::new();
        for tool in self.tools.files() {
            path.insert(parent(&tool.path));
        }
        path.insert(format!(
            "{}/System32",
            normalize(&self.tools.windows_directory)
        ));
        let environment = BTreeMap::from([
            ("CARGO_HOME".into(), format!("{output}/cargo-home")),
            ("CARGO_TARGET_DIR".into(), format!("{output}/target")),
            ("CARGO_NET_OFFLINE".into(), "true".into()),
            ("RUSTC".into(), normalize(&self.tools.rustc.path)),
            (
                "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER".into(),
                normalize(&self.tools.gnu_linker.path),
            ),
            (
                "SystemRoot".into(),
                normalize(&self.tools.windows_directory),
            ),
            (
                "PATH".into(),
                path.into_iter().collect::<Vec<_>>().join(";"),
            ),
        ]);
        let commands = self.fixed_commands(&output, &workspace, environment);
        Ok(BuildPlan {
            schema: "cad-agent.nx-worker-build-plan/v1".into(),
            spec_digest: self.digest()?,
            root_identity: self.root_identity.clone(),
            source_directory: workspace,
            new_output_directory: output,
            commands,
            max_seconds: self.max_seconds,
            output_must_be_new: true,
            network_allowed: false,
            native_execution_requested: false,
        })
    }
    // Both versions share the exact finite argv generator. Layout/environment
    // remain versioned: the historical v1 record digest must not drift.
    pub(crate) fn fixed_commands(
        &self,
        output: &str,
        workspace: &str,
        mut environment: BTreeMap<String, String>,
    ) -> Vec<PlannedCommand> {
        let mut cargo_arguments = vec![
            "build".into(),
            "--locked".into(),
            "--offline".into(),
            "--target".into(),
            "x86_64-pc-windows-gnu".into(),
            "-p".into(),
            "oe-nx-worker".into(),
            "--bin".into(),
            "oe-nx-worker".into(),
            "--jobs".into(),
            self.jobs.to_string(),
        ];
        if self.profile == BuildProfile::Release {
            cargo_arguments.push("--release".into());
        }
        let cargo_environment = environment.clone();
        environment.insert(
            "INCLUDE".into(),
            self.tools
                .include_directories
                .iter()
                .map(|v| normalize(v))
                .collect::<Vec<_>>()
                .join(";"),
        );
        environment.insert(
            "LIB".into(),
            self.tools
                .library_directories
                .iter()
                .map(|v| normalize(v))
                .collect::<Vec<_>>()
                .join(";"),
        );
        let sdk = normalize(&self.sdk.root);
        vec![
            PlannedCommand {
                kind: BuildStep::CargoWorker,
                executable: self.tools.cargo.clone(),
                working_directory: workspace.to_owned(),
                arguments: cargo_arguments,
                environment: cargo_environment,
                clear_inherited_environment: true,
                expected_output: format!(
                    "{output}/target/x86_64-pc-windows-gnu/{}/oe-nx-worker.exe",
                    self.profile.directory()
                ),
            },
            PlannedCommand {
                kind: BuildStep::MsvcShim,
                executable: self.tools.msvc_cl.clone(),
                working_directory: output.to_owned(),
                arguments: vec![
                    "/nologo".into(),
                    "/std:c++17".into(),
                    "/EHsc".into(),
                    "/MD".into(),
                    "/W4".into(),
                    "/WX".into(),
                    "/external:W0".into(),
                    format!("/external:I{sdk}/UGOPEN"),
                    "/LD".into(),
                    format!("/Fo{output}/nx_shim.obj"),
                    format!("{workspace}/crates/oe-nx-worker/shim/nx_shim.cpp"),
                    "/link".into(),
                    format!("{sdk}/UGOPEN/libufun.lib"),
                    format!("{sdk}/UGOPEN/libnxopencpp.lib"),
                    format!("{sdk}/UGOPEN/libnxopencpp_assemblies.lib"),
                    format!("/OUT:{output}/nx_shim.dll"),
                ],
                environment,
                clear_inherited_environment: true,
                expected_output: format!("{output}/nx_shim.dll"),
            },
        ]
    }
    fn worker_manifest(
        &self,
        executable: FileEntry,
        shim: FileEntry,
        sources: Vec<FileEntry>,
        recipe: FileEntry,
    ) -> WorkerManifest {
        WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: self.worker_id.clone(),
            version: self.worker_version.clone(),
            target: "x86_64-pc-windows-gnu".into(),
            executable,
            runtime_files: vec![shim],
            source_files: sources,
            build_recipe: recipe,
            toolchain: "declared Cargo GNU + MSVC; see bound build records".into(),
            required_runtime: self.sdk.runtime.clone(),
            capabilities: self.declared_capabilities.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildStep {
    CargoWorker,
    MsvcShim,
}
impl BuildStep {
    fn log_prefix(self) -> &'static str {
        match self {
            Self::CargoWorker => "cargo",
            Self::MsvcShim => "shim",
        }
    }
    fn artifact(self) -> &'static str {
        match self {
            Self::CargoWorker => EXE,
            Self::MsvcShim => SHIM,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedCommand {
    pub kind: BuildStep,
    pub executable: InstalledFile,
    pub working_directory: String,
    pub arguments: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub clear_inherited_environment: bool,
    pub expected_output: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildPlan {
    pub schema: String,
    pub spec_digest: Digest,
    pub root_identity: RootIdentity,
    pub source_directory: String,
    pub new_output_directory: String,
    pub commands: Vec<PlannedCommand>,
    pub max_seconds: u16,
    pub output_must_be_new: bool,
    pub network_allowed: bool,
    pub native_execution_requested: bool,
}
impl BuildPlan {
    pub fn digest(&self) -> Result<Digest> {
        record_digest(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedBuildStep {
    pub kind: BuildStep,
    pub command_digest: Digest,
    pub process: oe_nx_protocol::ProcessIdentity,
    pub exit_code: Option<u32>,
    pub stdout: FileEntry,
    pub stderr: FileEntry,
    pub output: FileEntry,
}
/// Untrusted external observations. A matching exit 0 is never authenticated
/// merely because a caller can serialize this structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildEvidence {
    pub schema: String,
    pub build_id: JobId,
    pub plan_digest: Digest,
    pub root_identity: RootIdentity,
    pub source_inventory_before: BlobIdentity,
    pub source_inventory_after: BlobIdentity,
    pub steps: Vec<ObservedBuildStep>,
}
impl BuildEvidence {
    pub fn validate_bindings(&self, spec: &BuildSpec) -> Result<()> {
        let plan = spec.plan()?;
        ensure!(
            self.schema == BUILD_EVIDENCE_SCHEMA
                && self.build_id == spec.build_id
                && self.root_identity == spec.root_identity
                && self.plan_digest == plan.digest()?,
            "build observation binding mismatch"
        );
        ensure!(
            self.source_inventory_before == spec.source_inventory.identity
                && self.source_inventory_after == self.source_inventory_before,
            "build source observation changed"
        );
        ensure!(
            self.steps.len() == plan.commands.len(),
            "missing/extra build step"
        );
        let mut process_instances = BTreeSet::new();
        for (step, command) in self.steps.iter().zip(&plan.commands) {
            step.process.validate()?;
            ensure!(
                step.process.host_id == spec.root_identity.host_id
                    && step.process.image == command.executable.identity,
                "build process/tool identity mismatch"
            );
            ensure!(
                process_instances.insert((step.process.pid, step.process.creation_time_100ns)),
                "build steps reuse process instance"
            );
            ensure!(
                step.kind == command.kind && step.command_digest == record_digest(command)?,
                "build command differs"
            );
            ensure!(
                step.exit_code == Some(0),
                "build failed or exit observation unknown"
            );
            for (entry, suffix) in [(&step.stdout, "stdout"), (&step.stderr, "stderr")] {
                ensure!(
                    entry.path.as_str()
                        == format!("sources/build/{}.{}", step.kind.log_prefix(), suffix)
                        && entry.identity.bytes <= MAX_RECORD_BYTES as u64,
                    "build log path or bound differs"
                );
            }
            ensure!(
                step.output.path.as_str() == step.kind.artifact()
                    && step.output.identity.bytes > 0
                    && step.output.identity.bytes <= MAX_PAYLOAD_BYTES,
                "build artifact differs"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildAssessment {
    pub record_bindings: String,
    pub compilation_execution: String,
    pub source_tree_verification: String,
    pub installed_toolchain_and_sdk: String,
    pub native_acceptance: String,
    pub engineering_acceptance: String,
}
#[derive(Debug, Clone)]
pub struct WorkerBuildPackage {
    manifest: WorkerManifest,
    members: BTreeMap<String, Vec<u8>>,
    assessment: BuildAssessment,
}
impl WorkerBuildPackage {
    pub fn manifest(&self) -> &WorkerManifest {
        &self.manifest
    }
    pub fn members(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.members
    }
    pub fn assessment(&self) -> &BuildAssessment {
        &self.assessment
    }
    pub fn into_members(self) -> BTreeMap<String, Vec<u8>> {
        self.members
    }
    pub fn verify(&self) -> Result<()> {
        self.manifest.validate()?;
        let raw = self
            .members
            .get("worker.json")
            .context("missing worker manifest bytes")?;
        let decoded: WorkerManifest = decode(raw)?;
        ensure!(decoded == self.manifest, "worker manifest bytes differ");
        let expected = self.manifest.payload_files();
        let names = expected
            .iter()
            .map(|v| v.path.as_str())
            .chain(Some("worker.json"))
            .collect::<BTreeSet<_>>();
        ensure!(
            names == self.members.keys().map(String::as_str).collect(),
            "worker package closure differs"
        );
        let mut total = 0u64;
        for (name, bytes) in &self.members {
            total = total
                .checked_add(bytes.len() as u64)
                .context("worker package byte overflow")?;
            if name != "worker.json" {
                expected
                    .iter()
                    .find(|v| v.path.as_str() == name)
                    .context("unlisted member")?
                    .identity
                    .verify(bytes)?;
            }
        }
        ensure!(
            total <= MAX_PAYLOAD_BYTES && self.members.len() <= MAX_FILES,
            "worker package exceeds existing bound"
        );
        ensure!(
            self.assessment == external_assessment(),
            "external observations cannot authenticate compilation"
        );
        Ok(())
    }
}
fn external_assessment() -> BuildAssessment {
    BuildAssessment {
        record_bindings: "verified_against_supplied_bytes".into(),
        compilation_execution: "not_authenticated_external_records".into(),
        source_tree_verification: "not_authenticated_external_records".into(),
        installed_toolchain_and_sdk: "declared_file_identities_only_not_observed".into(),
        native_acceptance: "not_evaluated".into(),
        engineering_acceptance: "not_evaluated".into(),
    }
}

/// Assemble a byte-complete original WorkerManifest from externally supplied
/// records and bytes. This is deliberately NOT called `build` or `compiled`:
/// matching records (including exit 0) do not prove a compiler actually ran.
/// No filesystem output, shell, process, NX job or SDK call occurs here.
pub fn package_from_external_records(
    spec: &BuildSpec,
    evidence: &BuildEvidence,
    files: BTreeMap<String, Vec<u8>>,
) -> Result<WorkerBuildPackage> {
    spec.validate()?;
    evidence.validate_bindings(spec)?;
    let mut expected = vec![
        spec.source_archive.clone(),
        spec.source_inventory.clone(),
        spec.cargo_lock.clone(),
    ];
    for step in &evidence.steps {
        expected.extend([
            step.stdout.clone(),
            step.stderr.clone(),
            step.output.clone(),
        ]);
    }
    validate_inventory(&expected)?;
    ensure!(
        files.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == expected.iter().map(|v| v.path.as_str()).collect(),
        "build input byte closure differs"
    );
    let mut members = BTreeMap::new();
    for entry in &expected {
        let bytes = files
            .get(entry.path.as_str())
            .context("missing build member")?;
        entry.identity.verify(bytes)?;
        insert(&mut members, entry.path.as_str(), bytes.clone())?;
    }
    let plan = spec.plan()?;
    for (path, raw) in [
        (SPEC_FILE, serde_json::to_vec_pretty(spec)?),
        (PLAN_FILE, serde_json::to_vec_pretty(&plan)?),
        (EVIDENCE_FILE, serde_json::to_vec_pretty(evidence)?),
    ] {
        ensure!(raw.len() <= MAX_RECORD_BYTES, "build record exceeds bound");
        insert(&mut members, path, raw)?;
    }
    let assessment = external_assessment();
    let recipe = serde_json::to_vec_pretty(&serde_json::json!({
        "schema":BUILD_RECIPE_SCHEMA,"build_id":spec.build_id,"spec_digest":spec.digest()?,"plan_digest":plan.digest()?,
        "actual_execution":"not_authenticated_external_records","assessment":assessment,
        "record_paths":{"spec":SPEC_FILE,"plan":PLAN_FILE,"observations":EVIDENCE_FILE},
        "source_archive":spec.source_archive,"cargo_lock":spec.cargo_lock,
        "sdk_payload":"not_included","native_execution":"not_requested_by_plan"
    }))?;
    ensure!(
        recipe.len() <= MAX_RECORD_BYTES,
        "build recipe exceeds record bound"
    );
    insert(&mut members, "build/recipe.json", recipe)?;
    let sources = members
        .iter()
        .filter(|(name, _)| name.starts_with("sources/"))
        .map(|(name, bytes)| FileEntry::from_bytes(name, bytes))
        .collect::<Result<Vec<_>>>()?;
    let manifest = spec.worker_manifest(
        FileEntry::from_bytes(EXE, &members[EXE])?,
        FileEntry::from_bytes(SHIM, &members[SHIM])?,
        sources,
        FileEntry::from_bytes("build/recipe.json", &members["build/recipe.json"])?,
    );
    manifest.validate()?;
    let raw = serde_json::to_vec_pretty(&manifest)?;
    ensure!(
        raw.len() <= MAX_RECORD_BYTES,
        "worker manifest exceeds record bound"
    );
    insert(&mut members, "worker.json", raw)?;
    let result = WorkerBuildPackage {
        manifest,
        members,
        assessment,
    };
    result.verify()?;
    Ok(result)
}

pub(crate) fn normalize(value: &str) -> String {
    value.replace('\\', "/")
}
fn basename(value: &str) -> &str {
    value.rsplit(['/', '\\']).next().unwrap_or("")
}
pub(crate) fn parent(value: &str) -> String {
    let v = normalize(value);
    v.rsplit_once('/').map_or(String::new(), |(v, _)| v.into())
}
pub(crate) fn windows_path(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    ensure!(
        value.len() >= 4
            && value.len() <= 1024
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'),
        "expected explicit drive-absolute Windows path"
    );
    ensure!(
        !value
            .chars()
            .any(|c| c.is_control() || matches!(c, ';' | '"' | '\'')),
        "unsafe compiler path"
    );
    RelativePath::new(&normalize(value)[3..])?;
    Ok(())
}
