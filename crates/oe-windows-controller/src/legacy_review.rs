//! OS facts for the shared, source-pinned historical writer-hold review.
use crate::{adapter::*, windows::*};
use anyhow::{Result, ensure};
use oe_nx_protocol as p;
use serde_json::{Value, json};
use std::ffi::OsStr;

fn full_staging(
    dir: &PinnedDirectory<'_>,
    prefix: &str,
    files: &mut Vec<p::FileEntry>,
    directories: &mut Vec<p::RelativePath>,
    pins: &mut Vec<PinnedFile>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir.path())? {
        let entry = entry?;
        let name = entry.file_name();
        let text = name
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("legacy member encoding"))?;
        let relative = p::RelativePath::new(format!("{prefix}{text}"))?;
        if entry.file_type()?.is_dir() {
            let child = dir.directory(&name)?;
            directories.push(relative.clone());
            full_staging(
                &child,
                &format!("{}/", relative.as_str()),
                files,
                directories,
                pins,
            )?;
        } else {
            let mut pin = dir
                .pin_file(&name, 8 * 1024 * 1024 * 1024)?
                .ok_or_else(|| anyhow::anyhow!("legacy member disappeared"))?;
            files.push(p::FileEntry {
                path: relative,
                identity: file_identity(&mut pin)?,
            });
            pins.push(pin);
        }
        ensure!(
            files.len() <= p::MAX_FILES && directories.len() <= p::MAX_FILES,
            "legacy inventory bound"
        );
    }
    dir.verify()?;
    Ok(())
}

/// The caller holds the root writer lock, including for a status review. This
/// avoids immutable control-file pins obstructing a concurrent state writer.
pub fn assess(
    root: &StableRoot,
    id: &p::JobId,
    input: &p::HoldInput,
    review_bytes: &[u8],
    pin: &p::Digest,
) -> Result<(p::HoldAssessment, Value)> {
    ensure!(
        p::Digest::of(review_bytes) == *pin,
        "root review pin differs"
    );
    let review: p::RootLegacyReview = p::decode(review_bytes)?;
    review.validate()?;
    let root_id = root_identity(root);
    ensure!(review.root == root_id, "root review identity differs");
    let entry = review
        .entries
        .iter()
        .find(|e| e.job_id == *id)
        .ok_or_else(|| anyhow::anyhow!("legacy directory not independently reviewed"))?;
    let dir = root.directory(OsStr::new(id.as_str()))?;
    let directory = p::RootIdentity {
        host_id: root_id.host_id.clone(),
        volume_id: dir.identity().volume_id.clone(),
        directory_id: dir.identity().file_id.clone(),
    };
    let mut retained = Vec::new();
    let mut current_process = None;
    let current = match &entry.snapshot {
        p::LegacyDirectorySnapshot::EmptyFailedStage { .. } => {
            ensure!(
                std::fs::read_dir(dir.path())?.next().is_none(),
                "reviewed empty directory gained members"
            );
            p::LegacyDirectorySnapshot::EmptyFailedStage { directory }
        }
        p::LegacyDirectorySnapshot::LegacyStagedNoDispatch { .. } => {
            let mut files = Vec::new();
            let mut directories = Vec::new();
            full_staging(&dir, "", &mut files, &mut directories, &mut retained)?;
            files.sort_by(|a, b| a.path.cmp(&b.path));
            directories.sort();
            p::LegacyDirectorySnapshot::LegacyStagedNoDispatch {
                directory,
                files,
                directories,
            }
        }
        p::LegacyDirectorySnapshot::LegacyTerminalInactive { .. } => {
            let p::HoldInput::StateBytes(bytes) = input else {
                anyhow::bail!("terminal state missing");
            };
            let state: Value = p::decode(bytes)?;
            let pid = state
                .get("native_pid")
                .and_then(Value::as_u64)
                .and_then(|p| u32::try_from(p).ok())
                .ok_or_else(|| anyhow::anyhow!("native PID missing"))?;
            let native_start_utc = state
                .get("native_start_utc")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("native start missing"))?
                .to_owned();
            let creation_time_100ns = p::legacy_utc_creation_time(&native_start_utc)?;
            let hostname = state
                .get("hostname")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("legacy hostname missing"))?
                .to_owned();
            let process = p::LegacyRecordedProcess {
                hostname,
                pid,
                creation_time_100ns,
                native_start_utc,
            };
            let mut control_files = Vec::new();
            for child in std::fs::read_dir(dir.path())? {
                let child = child?;
                let name = child.file_name();
                let text = name
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("legacy member encoding"))?;
                if !p::legacy_terminal_control_name(text) {
                    continue;
                }
                let mut file = dir
                    .pin_file(&name, p::MAX_RECORD_BYTES as u64)?
                    .ok_or_else(|| anyhow::anyhow!("control record disappeared"))?;
                control_files.push(p::FileEntry {
                    path: p::RelativePath::new(text)?,
                    identity: file_identity(&mut file)?,
                });
                retained.push(file);
                ensure!(
                    control_files.len() <= p::MAX_FILES,
                    "control record count exceeds bound"
                );
            }
            control_files.sort_by(|a, b| a.path.cmp(&b.path));
            let observation = match ProcessWatch::observe_historical(ProcessIdentity {
                pid,
                creation_time_100ns,
            }) {
                Ok(HistoricalProcessObservation::Absent) => p::LegacyCurrentProcess::Absent,
                Ok(HistoricalProcessObservation::DifferentCreation(actual)) => {
                    p::LegacyCurrentProcess::DifferentCreation {
                        actual_creation_time_100ns: actual.creation_time_100ns,
                    }
                }
                Ok(HistoricalProcessObservation::ExitedSameIdentity { exit_code }) => {
                    p::LegacyCurrentProcess::ExitedSameIdentity {
                        exit_code: exit_code as i32,
                    }
                }
                Ok(HistoricalProcessObservation::LiveSameIdentity) => {
                    p::LegacyCurrentProcess::LiveSameIdentity
                }
                Err(_) => p::LegacyCurrentProcess::Uncertain,
            };
            current_process = Some(p::LegacyProcessObservation {
                pid,
                creation_time_100ns,
                observation,
            });
            p::LegacyDirectorySnapshot::LegacyTerminalInactive {
                directory,
                control_files,
                process,
            }
        }
    };
    for file in &retained {
        file.verify()?;
    }
    dir.verify()?;
    let latest = observe_job_records(root, OsStr::new(id.as_str()));
    let latest_input = hold_input(&latest);
    ensure!(
        match (&latest_input, input) {
            (p::HoldInput::Missing, p::HoldInput::Missing) => true,
            (p::HoldInput::StateBytes(a), p::HoldInput::StateBytes(b)) => a == b,
            _ => false,
        },
        "control observation changed during legacy review"
    );
    let assessment = p::classify_reviewed_root_legacy(
        input,
        id,
        &root_id,
        review_bytes,
        pin,
        &current,
        current_process.as_ref(),
    )?;
    let evidence = json!({"job_id":id,"directory":current.directory(),"snapshot_digest":current.digest()?,"original_state":match input {p::HoldInput::StateBytes(bytes)=>Some(p::BlobIdentity::of(bytes)),_=>None},"process_observation":current_process,"result":"released_writer_hold_only","review_sha256":pin});
    Ok((assessment, evidence))
}
