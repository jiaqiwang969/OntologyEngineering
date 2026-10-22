//! Bound local operations for the unified `oe cad nx` command. Remote
//! observation failures return errors with raw evidence retained; they never
//! manufacture FAILED/COMPLETED states and never redispatch a used job.
use super::{
    Package,
    transport::{RemoteProfile, invoke, invoke_tracked},
};
use anyhow::{Context, Result, bail, ensure};
use oe_core::SkillRoot;
use oe_nx_protocol::*;
use serde::de::DeserializeOwned;
use std::{
    cell::Cell,
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

/// A local observation boundary, never a fabricated remote StateRecord.
/// Downcast the anyhow error to retain this distinction while its original
/// error chain and private transport bytes remain available for diagnosis.
#[derive(Debug)]
pub struct DispatchUncertain {
    pub job_id: JobId,
    pub manifest_digest: Digest,
    pub evidence: PathBuf,
    /// Stable public code; remote error text stays in the private evidence/chain.
    pub reason: String,
}
impl fmt::Display for DispatchUncertain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: inspect the same job {} and its retained evidence; do not redispatch",
            self.reason,
            self.job_id.as_str()
        )
    }
}
impl std::error::Error for DispatchUncertain {}

pub struct Client<'a> {
    pub root: &'a SkillRoot,
    pub profile: &'a RemoteProfile,
}
impl Client<'_> {
    fn expected(&self) -> Result<&RootIdentity> {
        self.profile
            .expected_root
            .as_ref()
            .context("probe and pin remote root first")
    }
    fn call(
        &self,
        request: &Request,
        body: &[u8],
        evidence: &Path,
        wait: Duration,
    ) -> Result<Frame> {
        self.call_tracked(request, body, evidence, wait, None)
    }
    fn call_tracked(
        &self,
        request: &Request,
        body: &[u8],
        evidence: &Path,
        wait: Duration,
        started: Option<&Cell<bool>>,
    ) -> Result<Frame> {
        let mut bytes = Vec::new();
        write_request(&mut bytes, request, body)?;
        let observation = if let Some(started) = started {
            invoke_tracked(self.root, self.profile, &bytes, evidence, wait, started)?
        } else {
            invoke(self.root, self.profile, &bytes, evidence, wait)?
        };
        let frame = observation.response_frame()?;
        if frame
            .record
            .get("schema")
            .and_then(serde_json::Value::as_str)
            == Some(ERROR_RESPONSE_SCHEMA)
        {
            frame.validate_payload(None)?;
            let error: ErrorResponse = serde_json::from_value(frame.record)?;
            error.validate()?;
            // The peer may have persisted an intent before this error. Error
            // text is not an execution-state adjudication or permission to retry.
            bail!(
                "remote operation returned an error: {}; inspect the same job and retained evidence",
                error.error
            );
        }
        Ok(frame)
    }
    fn record<T: serde::Serialize>(&self, evidence: &Path, name: &str, value: &T) -> Result<()> {
        let directory = self.root.expand(evidence)?;
        oe_core::no_symlinks(&directory)?;
        oe_core::write_new(&directory.join(name), &serde_json::to_value(value)?)
    }
    /// One explicit transfer-only request. A lost reply must be observed with
    /// transfer_inspect; this method never retries, stages, or dispatches.
    pub fn transfer(
        &self,
        request: &Request,
        body: &[u8],
        evidence: &Path,
    ) -> Result<transfer::TransferResponse> {
        transfer::validate_request(request)?;
        if let Request::TransferBegin { plan_record } = request {
            ensure!(
                transfer::plan(plan_record)?.root == *self.expected()?,
                "transfer root differs from profile"
            );
        }
        let frame = self.call(request, body, evidence, Duration::from_secs(300))?;
        let response: transfer::TransferResponse = no_payload(frame)?;
        response.validate(request, self.expected()?)?;
        self.record(evidence, "transfer-response.json", &response)?;
        Ok(response)
    }
    pub fn stage_published(
        &self,
        request: &published_stage::PublishedStageRequest,
        evidence: &Path,
    ) -> Result<published_stage::PublishedStageResponse> {
        request.validate()?;
        ensure!(
            request.job.root == *self.expected()?,
            "published_stage_profile_root_differs"
        );
        let frame = self.call(&request.wire()?, &[], evidence, Duration::from_secs(300))?;
        let response: published_stage::PublishedStageResponse = no_payload(frame)?;
        response.validate(request, self.expected()?)?;
        self.record(evidence, "published-stage-response.json", &response)?;
        Ok(response)
    }
    pub fn probe(&self, evidence: &Path) -> Result<ProbeResponse> {
        let frame = self.call(&Request::Probe {}, &[], evidence, Duration::from_secs(60))?;
        let response: ProbeResponse = no_payload(frame)?;
        response.validate_binding(
            self.profile.expected_root.as_ref(),
            &self.profile.controller,
        )?;
        self.record(evidence, "probe-response.json", &response)?;
        Ok(response)
    }
    pub fn stage(&self, package: &Package, evidence: &Path) -> Result<StageResponse> {
        package.verify()?;
        super::cylinder_worker_role(&package.job.worker, &package.job.action)?;
        ensure!(
            package.job.root == *self.expected()?,
            "prepared root differs from profile"
        );
        let bytes = package.archive()?;
        let archive = BlobIdentity::of(&bytes);
        let request = Request::Stage {
            job: Box::new(package.job.clone()),
            archive: archive.clone(),
        };
        let frame = self.call(&request, &bytes, evidence, Duration::from_secs(300))?;
        let response: StageResponse = no_payload(frame)?;
        response.validate_binding(&package.job, &archive)?;
        self.record(evidence, "stage-response.json", &response)?;
        Ok(response)
    }
    pub fn run(
        &self,
        job: &JobManifest,
        wait_seconds: u16,
        evidence: &Path,
    ) -> Result<StatusObservation> {
        job.validate()?;
        ensure!(
            job.root == *self.expected()?,
            "prepared root differs from profile"
        );
        let manifest_digest = job.digest()?;
        let evidence = self.root.private_path(evidence)?;
        let request = Request::Run {
            job_id: job.job_id.clone(),
            manifest_digest: manifest_digest.clone(),
            wait_seconds,
        };
        let started = Cell::new(false);
        let result: Result<StatusObservation> = (|| {
            let frame = self.call_tracked(
                &request,
                &[],
                &evidence,
                Duration::from_secs(u64::from(wait_seconds) + 30),
                Some(&started),
            )?;
            frame.validate_payload(None)?;
            let response = StatusObservation::from_value(frame.record)?;
            response.validate_binding(&job.job_id, &job.root)?;
            if matches!(response, StatusObservation::Known(_)) {
                response.validate_job(job)?;
            }
            self.record(&evidence, "run-observation.json", &response)?;
            Ok(response)
        })();
        match result {
            Err(error) if started.get() => Err(error.context(DispatchUncertain {
                job_id: job.job_id.clone(),
                manifest_digest,
                evidence,
                reason: "run_response_unconfirmed".into(),
            })),
            other => other,
        }
    }
    pub fn status(&self, job_id: &JobId, evidence: &Path) -> Result<StatusObservation> {
        let expected = self.expected()?;
        let request = Request::Status {
            job_id: job_id.clone(),
        };
        let frame = self.call(&request, &[], evidence, Duration::from_secs(60))?;
        frame.validate_payload(None)?;
        let response = StatusObservation::from_value(frame.record)?;
        response.validate_binding(job_id, expected)?;
        self.record(evidence, "status-observation.json", &response)?;
        Ok(response)
    }
    /// Retrieve a whole observed job without a single large ZIP. Partial local
    /// writes remain evidence; only both final checks produce fetched.json.
    pub fn fetch_large(
        &self,
        job_id: &JobId,
        digest: &Digest,
        evidence: &Path,
        output: &Path,
        chunk_bytes: u64,
    ) -> Result<PathBuf> {
        use large_fetch::*;
        use std::io::Write;
        ensure!(
            (1..=MAX_PAYLOAD_BYTES).contains(&chunk_bytes),
            "fetch_chunk_bound"
        );
        let expected = self.expected()?;
        let evidence = self.root.new_private_dir(evidence)?;
        let request = Request::FetchSnapshot {
            job_id: job_id.clone(),
            manifest_digest: digest.clone(),
        };
        let initial = FetchSnapshotResponse::decode_frame(
            self.call(
                &request,
                &[],
                &evidence.join("snapshot-start"),
                Duration::from_secs(300),
            )?,
            expected,
            job_id,
            digest,
        )?;
        let directory = self.root.new_private_dir(output)?;
        oe_core::write_new(
            &directory.join("fetch-snapshot.json"),
            &serde_json::to_value(&initial)?,
        )?;
        let tree = directory.join("job");
        oe_core::create_new_private_dir(&tree)?;
        for name in &initial.directories {
            oe_core::ensure_directory(&tree.join(name.as_str()))?;
        }
        let requests = batches(&initial, chunk_bytes)?;
        let mut sequence = 0usize;
        // Open only the current file. Contiguous batches may span many files or
        // continue a large file, without exhausting file descriptors.
        let mut current: Option<(usize, u64, std::fs::File)> = None;
        let mut next_file = 0usize;
        for request in &requests {
            let frame = self.call(
                &Request::FetchBatch {
                    request_record: serde_json::to_string(request)?,
                },
                &[],
                &evidence.join(format!("batch-{sequence:06}")),
                Duration::from_secs(300),
            )?;
            let response: FetchBatchResponse = serde_json::from_value(frame.record)?;
            response.validate(request, &frame.payload)?;
            let mut offset = 0usize;
            for slice in &request.slices {
                while current.is_none() {
                    let entry = initial
                        .files
                        .get(next_file)
                        .context("fetch_unexpected_slice")?;
                    let path = tree.join(entry.entry.path.as_str());
                    let file = oe_core::create_new_regular_file(&path)?;
                    if entry.entry.identity.bytes == 0 {
                        file.sync_all()?;
                        next_file += 1;
                    } else {
                        current = Some((next_file, 0, file));
                    }
                }
                let (index, written, file) = current.as_mut().unwrap();
                ensure!(
                    initial.files[*index] == slice.file && *written == slice.offset,
                    "fetch_noncontiguous_local_slice"
                );
                let end = offset + slice.bytes as usize;
                file.write_all(&frame.payload[offset..end])?;
                *written += slice.bytes;
                offset = end;
                if *written == slice.file.entry.identity.bytes {
                    file.sync_all()?;
                    current = None;
                    next_file += 1;
                }
            }
            ensure!(offset == frame.payload.len(), "fetch_unconsumed_batch");
            sequence += 1;
        }
        ensure!(current.is_none(), "fetch_partial_local_file");
        while next_file < initial.files.len() {
            let entry = &initial.files[next_file];
            ensure!(entry.entry.identity.bytes == 0, "fetch_missing_local_file");
            let file = oe_core::create_new_regular_file(&tree.join(entry.entry.path.as_str()))?;
            file.sync_all()?;
            next_file += 1;
        }
        let final_snapshot = FetchSnapshotResponse::decode_frame(
            self.call(
                &request,
                &[],
                &evidence.join("snapshot-finish"),
                Duration::from_secs(300),
            )?,
            expected,
            job_id,
            digest,
        )?;
        ensure!(
            initial == final_snapshot,
            "fetch_source_changed_before_completion"
        );
        verify_received_tree(&tree, &initial)?;
        oe_core::write_new(
            &directory.join("fetched.json"),
            &serde_json::json!({"schema":"oev2.nx-large-fetch-complete/1","snapshot_digest":initial.digest()?,"binding":initial.binding,"files":initial.files.len(),"batches":sequence,"qualification":"verified_job_bytes_only","native_result_adjudication":"not_performed","engineering_acceptance":"not_evaluated"}),
        )?;
        Ok(directory)
    }
    pub fn fetch(&self, job_id: &JobId, evidence: &Path, output: &Path) -> Result<PathBuf> {
        let expected = self.expected()?;
        let request = Request::Fetch {
            job_id: job_id.clone(),
        };
        let frame = self.call(&request, &[], evidence, Duration::from_secs(300))?;
        let response: FetchResponse = serde_json::from_value(frame.record.clone())?;
        frame.validate_payload(Some(&response.archive))?;
        let directory = publish_fetch(
            self.root,
            &response,
            job_id,
            expected,
            &frame.payload,
            output,
        )?;
        self.record(evidence, "fetch-response.json", &response)?;
        Ok(directory)
    }
}

fn no_payload<T: DeserializeOwned>(frame: Frame) -> Result<T> {
    frame.validate_payload(None)?;
    Ok(serde_json::from_value(frame.record)?)
}

/// Independently verify all received bytes before creating any local output.
/// Malformed legacy state may be preserved as evidence without being executed
/// or promoted. Repeating publication requires an explicit new destination.
pub fn publish_fetch(
    root: &SkillRoot,
    response: &FetchResponse,
    job_id: &JobId,
    expected_root: &RootIdentity,
    archive: &[u8],
    output: &Path,
) -> Result<PathBuf> {
    let members = response.validate_binding(job_id, expected_root, archive)?;
    let directories = validate_directory_envelope(&members, job_id, expected_root)?;
    let directory = root.new_private_dir(output)?;
    oe_core::write_new(
        &directory.join("fetch-intent.json"),
        &serde_json::json!({
            "schema":"cad-agent.nx-local-fetch-intent/v2", "job_id":job_id,
            "root":expected_root,"archive":response.archive,
            "native_execution":"not_performed_by_fetch","engineering_acceptance":"not_evaluated"
        }),
    )?;
    oe_core::write_new_bytes(&directory.join("job.zip"), archive)?;
    let job = directory.join("job");
    oe_core::create_new_private_dir(&job)?;
    for name in &directories {
        oe_core::ensure_directory(&job.join(name))?;
    }
    for (name, bytes) in &members {
        let path = job.join(name);
        oe_core::ensure_directory(path.parent().context("member parent unavailable")?)?;
        oe_core::write_new_bytes(&path, bytes)?;
        let actual = oe_core::fingerprint_file(&path, MAX_PAYLOAD_BYTES)?;
        let expected = BlobIdentity::of(bytes);
        ensure!(
            actual.sha256 == expected.sha256.as_str() && actual.bytes == expected.bytes,
            "fetched write differs"
        );
    }
    oe_core::write_new(
        &directory.join("fetch-response.json"),
        &serde_json::to_value(response)?,
    )?;
    oe_core::write_new(
        &directory.join("fetched.json"),
        &serde_json::json!({
            "schema":"cad-agent.nx-local-fetch-complete/v2","job_id":job_id,"root":expected_root,
            "archive":response.archive,"members":response.members.len(),"state_file":response.state_file,
            "observed_directory_envelope":directories,"native_result_adjudication":"not_performed_by_archive_integrity","engineering_acceptance":"not_evaluated"
        }),
    )?;
    Ok(directory)
}

/// Legacy fetches stay file-only. New directories are materialized only from
/// the exact archive-bound controller envelope, never assumed from job state.
pub fn validate_directory_envelope(
    members: &std::collections::BTreeMap<String, Vec<u8>>,
    job_id: &JobId,
    expected_root: &RootIdentity,
) -> Result<Vec<String>> {
    let Some(raw) = members.get("oev2-directory-envelope.json") else {
        return Ok(vec![]);
    };
    let envelope: serde_json::Value = oe_core::json::strict_json(raw)?;
    ensure!(
        envelope["schema"] == "oev2.nx-directory-envelope/1"
            && envelope["job_id"] == serde_json::json!(job_id)
            && envelope["root"] == serde_json::json!(expected_root)
            && envelope["origin"] == "read_only_pinned_controller_observation"
            && envelope["native_state_modified"] == false,
        "fetched_directory_envelope_identity_differs"
    );
    let entries = envelope["directories"]
        .as_array()
        .context("fetched_directory_envelope_missing")?;
    ensure!(
        entries.len() <= oe_nx_protocol::MAX_FILES,
        "fetched_directory_envelope_budget"
    );
    let mut names = std::collections::BTreeSet::new();
    for entry in entries {
        let name = entry.as_str().context("fetched_directory_name_invalid")?;
        let path = oe_nx_protocol::RelativePath::new(name)?;
        ensure!(
            names.insert(oe_nx_protocol::path_collision_key(path.as_str())),
            "fetched_directory_alias"
        );
        for file in members.keys() {
            let file = oe_nx_protocol::path_collision_key(file);
            let directory = oe_nx_protocol::path_collision_key(path.as_str());
            ensure!(
                file != directory && !directory.starts_with(&format!("{file}/")),
                "fetched_directory_file_collision"
            );
        }
    }
    Ok(entries.iter().map(|s| s.as_str().unwrap().into()).collect())
}

/// Rehash every local file and check the exact file/directory set before the
/// caller emits a completion receipt. Transport chunk hashes alone never pass.
pub fn verify_received_tree(tree: &Path, snapshot: &large_fetch::FetchSnapshot) -> Result<()> {
    use std::collections::BTreeSet;
    snapshot.validate()?;
    oe_core::no_symlinks(tree)?;
    let expected_files = snapshot
        .files
        .iter()
        .map(|f| f.entry.path.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let expected_dirs = snapshot
        .directories
        .iter()
        .map(|f| f.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    fn walk(
        base: &Path,
        dir: &Path,
        files: &mut BTreeSet<String>,
        dirs: &mut BTreeSet<String>,
    ) -> Result<()> {
        oe_core::no_symlinks(dir)?;
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let path = e.path();
            oe_core::no_symlinks(&path)?;
            let name = path
                .strip_prefix(base)?
                .to_str()
                .context("fetch_local_encoding")?
                .replace('\\', "/");
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                dirs.insert(name);
                walk(base, &path, files, dirs)?;
            } else {
                ensure!(metadata.is_file(), "fetch_local_not_regular");
                files.insert(name);
            }
        }
        Ok(())
    }
    let mut files = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    walk(tree, tree, &mut files, &mut dirs)?;
    ensure!(
        files == expected_files && dirs == expected_dirs,
        "fetch_local_namespace_differs"
    );
    for expected in &snapshot.files {
        let actual = oe_core::fingerprint_file(
            &tree.join(expected.entry.path.as_str()),
            expected.entry.identity.bytes.max(1),
        )?;
        ensure!(
            actual.bytes == expected.entry.identity.bytes
                && actual.sha256 == expected.entry.identity.sha256.as_str(),
            "fetch_local_whole_file_hash_differs"
        );
    }
    Ok(())
}
