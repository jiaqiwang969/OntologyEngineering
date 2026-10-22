//! Local NX preparation, finite SSH observations and bound remote operations.
//! Transport completion alone never grants native or engineering acceptance.
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes};
use oe_nx_protocol::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub mod client;
pub mod transport;
use oe_nx_protocol::bundle::insert;
pub use oe_nx_protocol::bundle::{Package, from_archive};

/// Explicit association, preserving filenames needed by assembly references.
/// A caller that changes a name must understand and review those dependencies.
pub struct InputBinding {
    pub source: PathBuf,
    pub member: RelativePath,
}

pub struct PrepareRequest {
    pub worker_directory: PathBuf,
    pub parameter_file: PathBuf,
    pub inputs: Vec<InputBinding>,
    pub job_id: JobId,
    pub root_identity: RootIdentity,
    pub action: String,
    pub parameter_schema: String,
}

/// Convert a complete job input member to the existing worker-relative name.
/// The worker owns the inputs/ prefix and path/extension/schema/resource checks.
/// This neither loads NX nor dispatches.
pub fn import_assembly_parameters(
    step: &str,
    budget: &oe_contracts::NxAssemblyBudget,
) -> Result<Vec<u8>> {
    let member = RelativePath::new(step.to_owned())?;
    let name = member
        .as_str()
        .strip_prefix("inputs/")
        .context("assembly_step_outside_inputs")?;
    ensure!(
        oe_nx_worker::geometry::input_relative(name)? == member,
        "assembly_step_worker_coordinate_differs"
    );
    let value = serde_json::json!({"schema":oe_nx_worker::assembly::PARAMS_SCHEMA,
        "action":"import_step_assembly","step":name,"budget":budget});
    let parameters: oe_nx_worker::geometry::Parameters = serde_json::from_value(value.clone())?;
    parameters.validate()?;
    Ok(oe_core::json::canonical_json(&value)?)
}

fn rooted(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.expand(path)?;
    ensure!(
        path.starts_with(root.path()) && path != root.path(),
        "input outside skill"
    );
    no_symlinks(&path)?;
    Ok(path)
}

fn snapshot(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let bytes = read_regular_bytes(path, limit)?;
    let after = oe_core::fingerprint_file(path, limit as u64)?;
    let identity = BlobIdentity::of(&bytes);
    ensure!(
        after.bytes == identity.bytes && after.sha256 == identity.sha256.as_str(),
        "source changed during snapshot"
    );
    Ok(bytes)
}

/// Include ordinary files only. No link, socket, device or unaccounted payload
/// can be carried into the remote job under the name of a declared file.
fn names(directory: &Path) -> Result<BTreeSet<String>> {
    no_symlinks(directory)?;
    ensure!(directory.is_dir(), "payload directory missing");
    let mut pending = vec![directory.to_owned()];
    let mut result = BTreeSet::new();
    let mut directories = 0usize;
    while let Some(parent) = pending.pop() {
        no_symlinks(&parent)?;
        for entry in fs::read_dir(&parent)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(!metadata.file_type().is_symlink(), "payload link rejected");
            if metadata.is_dir() {
                directories += 1;
                ensure!(
                    directories <= MAX_FILES,
                    "payload directory count exceeds bound"
                );
                pending.push(path);
            } else {
                ensure!(metadata.is_file(), "payload special file rejected");
                let relative = path
                    .strip_prefix(directory)?
                    .components()
                    .map(|c| c.as_os_str().to_str().context("non-UTF8 member"))
                    .collect::<Result<Vec<_>>>()?
                    .join("/");
                RelativePath::new(relative.clone())?;
                ensure!(
                    result.insert(relative) && result.len() <= MAX_FILES + 1,
                    "payload count exceeds bound"
                );
            }
        }
    }
    Ok(result)
}

/// Publish a new task-owned directory only. Partial failure leaves the
/// existing intent and bytes for inspection; nothing is replaced or retried.
pub fn publish(package: &Package, root: &SkillRoot, output: &Path) -> Result<PathBuf> {
    package.verify()?;
    let archive = package.archive()?;
    let directory = root.new_private_dir(output)?;
    oe_core::write_new(
        &directory.join("prepare-intent.json"),
        &serde_json::json!({
            "schema":"cad-agent.nx-prepare-intent/v2", "job_id":package.job.job_id,
            "manifest_digest":package.job.digest()?, "archive":BlobIdentity::of(&archive),
            "dispatch":"not_performed"
        }),
    )?;
    let job = directory.join("job");
    oe_core::create_new_private_dir(&job)?;
    for (name, bytes) in &package.members {
        let path = job.join(name);
        oe_core::ensure_directory(path.parent().context("missing member parent")?)?;
        oe_core::write_new_bytes(&path, bytes)?;
    }
    oe_core::write_new_bytes(&directory.join("job.zip"), &archive)?;
    let reread = from_directory(root, &job)?;
    ensure!(
        reread.job == package.job && reread.members == package.members,
        "published bytes differ"
    );
    oe_core::write_new(
        &directory.join("prepared.json"),
        &serde_json::json!({
            "schema":"cad-agent.nx-prepared/v2", "job_id":package.job.job_id,
            "manifest_digest":package.job.digest()?, "payload_digest":inventory_digest(&package.job.payload_files())?,
            "archive":BlobIdentity::of(&archive), "native_execution":"not_run", "engineering_acceptance":"not_evaluated"
        }),
    )?;
    Ok(directory)
}

/// Byte completeness of a local worker package. Declared runtime and build
/// facts are not evaluated by reading its manifest and files.
#[derive(Debug, serde::Serialize)]
pub struct WorkerVerification {
    pub schema: &'static str,
    pub status: &'static str,
    pub worker_id: String,
    pub version: String,
    pub target: String,
    pub worker_manifest_digest: Digest,
    pub worker_manifest_file: BlobIdentity,
    pub payload_digest: Digest,
    pub payload_file_count: usize,
    pub payload_bytes: u64,
    pub declared_capabilities: Vec<Capability>,
    pub compilation_provenance: &'static str,
    pub required_runtime_availability: &'static str,
    pub native_acceptance: &'static str,
    pub engineering_acceptance: &'static str,
    pub job_preparation: &'static str,
    pub execution_performed: bool,
}

struct WorkerSnapshot {
    directory: PathBuf,
    raw_manifest: Vec<u8>,
    manifest: WorkerManifest,
    expected: BTreeSet<String>,
    members: BTreeMap<String, Vec<u8>>,
}
impl WorkerSnapshot {
    fn read(root: &SkillRoot, directory: &Path) -> Result<Self> {
        let worker_dir = rooted(root, directory)?;
        let worker_raw = snapshot(&worker_dir.join("worker.json"), MAX_RECORD_BYTES)?;
        let worker: WorkerManifest = decode(&worker_raw)?;
        worker.validate()?;
        let expected = worker
            .payload_files()
            .iter()
            .map(|f| f.path.as_str().to_owned())
            .chain(Some("worker.json".to_owned()))
            .collect::<BTreeSet<_>>();
        ensure!(
            names(&worker_dir)? == expected,
            "worker directory closure differs"
        );
        let mut members = BTreeMap::new();
        for file in worker.payload_files() {
            let bytes = snapshot(
                &worker_dir.join(file.path.as_str()),
                MAX_PAYLOAD_BYTES as usize,
            )?;
            file.identity.verify(&bytes)?;
            insert(&mut members, file.path.as_str(), bytes)?;
        }
        let snapshot = Self {
            directory: worker_dir,
            raw_manifest: worker_raw,
            manifest: worker,
            expected,
            members,
        };
        snapshot.ensure_current()?;
        Ok(snapshot)
    }

    fn ensure_current(&self) -> Result<()> {
        ensure!(
            read_regular_bytes(&self.directory.join("worker.json"), MAX_RECORD_BYTES)?
                == self.raw_manifest,
            "worker manifest changed during snapshot"
        );
        ensure!(
            names(&self.directory)? == self.expected,
            "worker closure changed during snapshot"
        );
        Ok(())
    }
}

/// Inspect the existing directory without publishing records, running its
/// executable, resolving an SDK or requiring a remote transport profile.
pub fn verify_worker(root: &SkillRoot, directory: &Path) -> Result<WorkerVerification> {
    let snapshot = WorkerSnapshot::read(root, directory)?;
    let files = snapshot.manifest.payload_files();
    let payload_bytes = files.iter().try_fold(0u64, |total, file| {
        total
            .checked_add(file.identity.bytes)
            .context("worker payload byte overflow")
    })?;
    Ok(WorkerVerification {
        schema: "cad-agent.nx-worker-verification/v2",
        status: "package_complete",
        worker_id: snapshot.manifest.worker_id.clone(),
        version: snapshot.manifest.version.clone(),
        target: snapshot.manifest.target.clone(),
        worker_manifest_digest: snapshot.manifest.digest()?,
        worker_manifest_file: BlobIdentity::of(&snapshot.raw_manifest),
        payload_digest: inventory_digest(&files)?,
        payload_file_count: files.len(),
        payload_bytes,
        declared_capabilities: snapshot.manifest.capabilities,
        compilation_provenance: "not_evaluated",
        required_runtime_availability: "not_observed",
        native_acceptance: "not_evaluated",
        engineering_acceptance: "not_evaluated",
        job_preparation: "not_evaluated",
        execution_performed: false,
    })
}

/// Explicit declaration-level role preflight. Generic worker verification
/// retains its original completeness-only meaning for all historical packages.
pub fn verify_current_worker_role(root: &SkillRoot, directory: &Path) -> Result<serde_json::Value> {
    let complete = verify_worker(root, directory)?;
    let raw = snapshot(
        &rooted(root, directory)?.join("worker.json"),
        MAX_RECORD_BYTES,
    )?;
    ensure!(
        BlobIdentity::of(&raw) == complete.worker_manifest_file,
        "worker_manifest_changed_during_role_check"
    );
    let worker: WorkerManifest = decode(&raw)?;
    oe_nx_worker::identity::validate_declared_worker_identity(&worker)?;
    Ok(serde_json::json!({
        "schema":"oev2.nx-current-worker-role-verification/1",
        "status":"declared_role_matched",
        "expected_worker_id":oe_nx_worker::identity::NATIVE_WORKER_ID,
        "expected_executable":oe_nx_worker::identity::NATIVE_WORKER_EXECUTABLE,
        "declared_target":worker.target,
        "package":complete,
        "compiled_executable_identity":"not_observed",
        "native_acceptance":"not_evaluated",
        "engineering_acceptance":"not_evaluated",
        "execution_performed":false
    }))
}

pub(crate) fn cylinder_worker_role(worker: &WorkerManifest, action: &str) -> Result<()> {
    if matches!(
        action,
        "create_cylinder_solid"
            | "inspect_cylinder_solid"
            | "create_stepped_shaft"
            | "inspect_stepped_shaft"
    ) {
        oe_nx_worker::identity::validate_declared_worker_identity(worker)?;
    }
    Ok(())
}

pub fn prepare(root: &SkillRoot, request: &PrepareRequest) -> Result<Package> {
    let mut worker_snapshot = WorkerSnapshot::read(root, &request.worker_directory)?;
    cylinder_worker_role(&worker_snapshot.manifest, &request.action)?;
    let mut members = std::mem::take(&mut worker_snapshot.members);
    let parameters = snapshot(&rooted(root, &request.parameter_file)?, MAX_RECORD_BYTES)?;
    let params = FileEntry::from_bytes("params.json", &parameters)?;
    insert(&mut members, "params.json", parameters)?;
    let mut inputs = Vec::new();
    for input in &request.inputs {
        ensure!(
            input.member.as_str().starts_with("inputs/"),
            "input member outside inputs"
        );
        let bytes = snapshot(&rooted(root, &input.source)?, MAX_PAYLOAD_BYTES as usize)?;
        inputs.push(FileEntry::from_bytes(input.member.as_str(), &bytes)?);
        insert(&mut members, input.member.as_str(), bytes)?;
    }
    let job = JobManifest {
        capacity: None,
        auxiliary_capacity: None,
        schema: JOB_SCHEMA.into(),
        job_id: request.job_id.clone(),
        root: request.root_identity.clone(),
        worker: worker_snapshot.manifest.clone(),
        action: request.action.clone(),
        params_schema: request.parameter_schema.clone(),
        params,
        inputs,
    };
    job.validate()?;
    insert(&mut members, "job.json", serde_json::to_vec_pretty(&job)?)?;
    worker_snapshot.ensure_current()?;
    let package = Package { job, members };
    package.verify()?;
    Ok(package)
}

pub fn from_directory(root: &SkillRoot, directory: &Path) -> Result<Package> {
    let directory = rooted(root, directory)?;
    let job_raw = snapshot(&directory.join("job.json"), MAX_RECORD_BYTES)?;
    let job: JobManifest = decode(&job_raw)?;
    job.validate()?;
    let expected = job
        .payload_files()
        .iter()
        .map(|f| f.path.as_str().to_owned())
        .chain(Some("job.json".into()))
        .collect::<BTreeSet<_>>();
    ensure!(
        names(&directory)? == expected,
        "job directory closure differs"
    );
    let mut members = BTreeMap::new();
    for name in expected {
        let bytes = snapshot(&directory.join(&name), MAX_PAYLOAD_BYTES as usize)?;
        insert(&mut members, &name, bytes)?;
    }
    let package = Package { job, members };
    package.verify()?;
    Ok(package)
}

/// Dispatch preparation is separate from ZIP staging. Large jobs are checked
/// file-by-file; the controller independently admits their published origin.
pub fn dispatch_job(root: &SkillRoot, directory: &Path) -> Result<JobManifest> {
    let directory = rooted(root, directory)?;
    let raw = snapshot(&directory.join("job.json"), MAX_RECORD_BYTES)?;
    let job: JobManifest = decode(&raw)?;
    job.validate()?;
    if job.schema == JOB_SCHEMA {
        return Ok(from_directory(root, &directory)?.job);
    }
    ensure!(job.schema == LARGE_JOB_SCHEMA, "dispatch_job_schema");
    let expected = job.payload_files();
    let names_expected = expected
        .iter()
        .map(|f| f.path.as_str().to_owned())
        .chain(Some("job.json".into()))
        .collect::<BTreeSet<_>>();
    ensure!(
        names(&directory)? == names_expected,
        "dispatch_job_directory_closure_differs"
    );
    let (_, file_bound, _) = job.inventory_bounds(false)?;
    for file in &expected {
        let got = oe_core::fingerprint_file(&directory.join(file.path.as_str()), file_bound)?;
        ensure!(
            got.bytes == file.identity.bytes && got.sha256 == file.identity.sha256.as_str(),
            "dispatch_job_payload_changed"
        );
    }
    let params = snapshot(&directory.join(job.params.path.as_str()), MAX_RECORD_BYTES)?;
    job.params.identity.verify(&params)?;
    job.validate_parameters(&params)?;
    ensure!(
        snapshot(&directory.join("job.json"), MAX_RECORD_BYTES)? == raw
            && names(&directory)? == names_expected,
        "dispatch_job_changed_during_review"
    );
    Ok(job)
}
