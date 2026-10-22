//! Actual fetched bytes and protocol identities, including incomplete execution.
//! This does not relax the existing success-only drawing receipt owner.
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, CadMotionJobSource1};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256};
use oe_nx_protocol::{JobManifest, NativeResult, State, StateRecord, large_fetch::FetchSnapshot};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const RECORD: usize = 16 * 1024 * 1024;
const TOTAL: usize = 128 * 1024 * 1024;

pub(super) struct Sources<'a> {
    root: &'a SkillRoot,
    pins: BTreeMap<PathBuf, (String, usize)>,
    bytes: usize,
}
impl<'a> Sources<'a> {
    pub fn new(root: &'a SkillRoot) -> Self {
        Self {
            root,
            pins: BTreeMap::new(),
            bytes: 0,
        }
    }
    pub fn path(&self, reference: &ArtifactReference) -> Result<PathBuf> {
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("motion_source_reference_invalid"))?;
        oe_nx_protocol::Digest::new(reference.sha256.clone())?;
        let path = self.root.expand(Path::new(&reference.path))?;
        ensure!(
            path.starts_with(self.root.path().join("var")),
            "motion_source_outside_private_root"
        );
        no_symlinks(&path)?;
        Ok(path)
    }
    pub fn read(&mut self, reference: &ArtifactReference, limit: usize) -> Result<Vec<u8>> {
        ensure!(limit <= TOTAL, "motion_source_read_bound");
        let path = self.path(reference)?;
        let raw = read_regular_bytes(&path, limit)?;
        ensure!(
            sha256(&raw) == reference.sha256,
            "motion_source_digest_differs"
        );
        if let Some((prior, count)) = self.pins.get(&path) {
            ensure!(
                prior == &reference.sha256 && *count == raw.len(),
                "motion_source_identity_conflict"
            );
        } else {
            self.bytes = self
                .bytes
                .checked_add(raw.len())
                .context("motion_source_size_overflow")?;
            ensure!(self.bytes <= TOTAL, "motion_source_total_budget");
            self.pins
                .insert(path, (reference.sha256.clone(), raw.len()));
        }
        Ok(raw)
    }
    pub fn recheck(&self) -> Result<()> {
        for (path, (digest, count)) in &self.pins {
            no_symlinks(path)?;
            let raw = read_regular_bytes(path, *count)?;
            ensure!(
                raw.len() == *count && sha256(&raw) == *digest,
                "motion_source_changed_during_check"
            );
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchCompletion {
    schema: String,
    snapshot_digest: oe_nx_protocol::Digest,
    binding: oe_nx_protocol::large_fetch::FetchBinding,
    files: usize,
    batches: u64,
    qualification: String,
    native_result_adjudication: String,
    engineering_acceptance: String,
}

pub(super) struct ReceivedJob {
    pub tree: PathBuf,
    pub snapshot: FetchSnapshot,
    pub job: JobManifest,
    pub state: StateRecord,
    pub result: Option<NativeResult>,
    pub readback: Option<Value>,
    pub fetch_complete: bool,
}
impl ReceivedJob {
    pub fn member(&self, name: &str) -> Result<PathBuf> {
        oe_nx_protocol::RelativePath::new(name)?;
        ensure!(
            self.snapshot
                .files
                .iter()
                .any(|f| f.entry.path.as_str() == name),
            "motion_source_not_in_snapshot:{name}"
        );
        let path = self.tree.join(name);
        no_symlinks(&path)?;
        Ok(path)
    }
    pub fn bytes(&self, name: &str, bound: usize) -> Result<Vec<u8>> {
        let file = self
            .snapshot
            .files
            .iter()
            .find(|f| f.entry.path.as_str() == name)
            .context("motion_snapshot_member_missing")?;
        ensure!(
            file.entry.identity.bytes <= bound as u64,
            "motion_member_read_bound"
        );
        let bytes = read_regular_bytes(&self.member(name)?, bound)?;
        file.entry.identity.verify(&bytes)?;
        Ok(bytes)
    }
    pub fn input(&self, reference: &ArtifactReference, bound: usize) -> Result<Vec<u8>> {
        oe_nx_worker::geometry::motion_task::reference_valid(reference)?;
        let name = format!("inputs/{}", reference.path);
        let entry = self
            .job
            .inputs
            .iter()
            .find(|f| f.path.as_str() == name)
            .context("motion_source_not_in_job_inputs")?;
        ensure!(
            entry.identity.sha256.as_str() == reference.sha256,
            "motion_input_reference_differs"
        );
        self.bytes(&name, bound)
    }
    pub fn recheck(&self) -> Result<()> {
        crate::nx::client::verify_received_tree(&self.tree, &self.snapshot)
    }
    pub fn completed(&self) -> bool {
        self.state.state == State::Completed
            && self
                .result
                .as_ref()
                .is_some_and(|r| r.outcome == oe_nx_protocol::ResultOutcome::Succeeded)
    }
    /// Read the original owned-copy receipt. Do not derive an expected namespace
    /// from the pair/save observation being checked.
    pub fn namespace(&self, root_part: &str, file_count: usize) -> Result<String> {
        let mut found = None;
        for file in &self.snapshot.files {
            let name = file.entry.path.as_str();
            if !name.starts_with("receipts/") || !name.ends_with("-motion_copy_verified.json") {
                continue;
            }
            let v = oe_core::json::strict_json(&self.bytes(name, RECORD)?)?;
            ensure!(
                found.is_none()
                    && v["schema"] == "cad-agent.nx-native-event/v2"
                    && v["job_id"] == self.job.job_id.as_str()
                    && v["phase"] == "motion_copy_verified"
                    && v["sequence"].as_u64().is_some_and(|s| s > 0),
                "motion_copy_receipt_binding"
            );
            let o = &v["observation"];
            let namespace = o["namespace"]
                .as_str()
                .context("motion_owned_namespace_missing")?;
            let normalized = namespace.replace('\\', "/");
            let suffix = format!("/{}/out/motion-assembly", self.job.job_id.as_str());
            ensure!(
                normalized.ends_with(&suffix)
                    && normalized.len() <= 32768
                    && !normalized.chars().any(char::is_control)
                    && !normalized.split('/').any(|p| p == ".." || p == ".")
                    && o["root_part"].as_str().is_some_and(
                        |s| s.replace('\\', "/") == format!("{normalized}/{root_part}")
                    )
                    && o["files"].as_u64() == Some(file_count as u64)
                    && o["source_read_pins_retained"] == true
                    && o["output_directory_pins_retained"] == true,
                "motion_owned_namespace_receipt_differs"
            );
            found = Some(namespace.to_owned());
        }
        found.context("motion_owned_namespace_not_recorded")
    }
}

pub(super) fn receive(
    sources: &mut Sources<'_>,
    reference: &CadMotionJobSource1,
) -> Result<ReceivedJob> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_job_source_invalid"))?;
    let raw = sources.read(&reference.job, RECORD)?;
    let path = sources.path(&reference.job)?;
    ensure!(
        path.file_name().and_then(|n| n.to_str()) == Some("job.json"),
        "motion_job_locator"
    );
    let tree = path.parent().context("motion_job_parent")?.to_path_buf();
    let job: JobManifest = oe_nx_protocol::decode(&raw)?;
    job.validate()?;
    let snapshot: FetchSnapshot = oe_nx_protocol::decode(&sources.read(
        &reference.fetch_snapshot,
        oe_nx_protocol::large_fetch::MAX_SNAPSHOT_BYTES,
    )?)?;
    snapshot.validate()?;
    ensure!(snapshot.job == job, "motion_snapshot_job_differs");
    snapshot.binding.job_record.verify(&raw)?;
    crate::nx::client::verify_received_tree(&tree, &snapshot)?;
    let fetch_complete = if let Some(r) = &reference.fetch_completion {
        let f: FetchCompletion = oe_nx_protocol::decode(&sources.read(r, RECORD)?)?;
        ensure!(
            f.schema == "oev2.nx-large-fetch-complete/1"
                && f.snapshot_digest == snapshot.digest()?
                && f.binding == snapshot.binding
                && f.files == snapshot.files.len()
                && f.batches > 0
                && f.qualification == "verified_job_bytes_only"
                && f.native_result_adjudication == "not_performed"
                && f.engineering_acceptance == "not_evaluated",
            "motion_fetch_completion_differs"
        );
        true
    } else {
        false
    };
    let state_file = snapshot
        .files
        .iter()
        .find(|f| f.entry.path.as_str() == "state.json")
        .context("motion_state_record_missing")?;
    let state_raw = read_regular_bytes(&tree.join("state.json"), RECORD)?;
    state_file.entry.identity.verify(&state_raw)?;
    snapshot
        .binding
        .state_record
        .as_ref()
        .context("motion_snapshot_state_missing")?
        .verify(&state_raw)?;
    let state: StateRecord = oe_nx_protocol::decode(&state_raw)?;
    state.validate(&job)?;
    let mut received = ReceivedJob {
        tree,
        snapshot,
        job,
        state,
        result: None,
        readback: None,
        fetch_complete,
    };
    received
        .job
        .validate_parameters(&received.bytes("params.json", RECORD)?)?;
    let has_result = received
        .snapshot
        .files
        .iter()
        .any(|f| f.entry.path.as_str() == "native-result.json");
    if has_result {
        let native: NativeResult =
            oe_nx_protocol::decode(&received.bytes("native-result.json", RECORD)?)?;
        let dispatch = received
            .state
            .dispatch
            .as_ref()
            .context("motion_dispatch_missing")?;
        let process = received
            .state
            .process
            .as_ref()
            .context("motion_process_missing")?;
        let staged_payload: Vec<_> = received
            .snapshot
            .files
            .iter()
            .filter(|f| {
                received
                    .job
                    .payload_files()
                    .iter()
                    .any(|p| p.path == f.entry.path)
            })
            .map(|f| f.entry.clone())
            .collect();
        let artifacts: Vec<_> = received
            .snapshot
            .files
            .iter()
            .filter(|f| f.entry.path.as_str().starts_with("out/"))
            .map(|f| f.entry.clone())
            .collect();
        native.validate_recorded_binding(
            &received.job,
            dispatch,
            process,
            &staged_payload,
            &artifacts,
        )?;
        let digest = oe_nx_protocol::record_digest(&native)?;
        if let Some(expected) = &received.state.result_digest {
            ensure!(*expected == digest, "motion_state_result_digest_differs");
        }
        if received.state.state == State::Completed {
            ensure!(
                received.state.result_digest.as_ref() == Some(&digest)
                    && native.outcome == oe_nx_protocol::ResultOutcome::Succeeded,
                "motion_completed_result_inconsistent"
            );
        }
        let readback = match native.readback_schema.as_str() {
            "cad-agent.nx-native-readback/v2" => native.readback.clone(),
            "oev2.nx-native-indexed-readback/1" => oe_nx_worker::event_index::receive_readback(
                received.job.job_id.as_str(),
                &native.readback_schema,
                &native.readback,
                &mut |name| received.bytes(name, RECORD),
            )?,
            _ => anyhow::bail!("motion_native_readback_schema_unsupported"),
        };
        if received.state.state == State::Completed {
            ensure!(
                readback["lifecycle"]["terminate_return"] == 0,
                "motion_native_termination_incomplete"
            );
        }
        received.result = Some(native);
        received.readback = Some(readback);
    } else {
        ensure!(
            received.state.state != State::Completed && received.state.result_digest.is_none(),
            "motion_completed_result_absent"
        );
    }
    received.recheck()?;
    Ok(received)
}
