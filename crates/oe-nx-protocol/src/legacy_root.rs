//! A control-plane-pinned review can release only a historical writer hold.
//! It neither changes old records nor certifies native/engineering results.
//! Adapters must observe current identities while holding the shared root lock;
//! historical process absence must never be reused as a current observation.
use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const ROOT_LEGACY_REVIEW_SCHEMA: &str = "cad-agent.nx-root-legacy-review/v2";

/// The old controller writes UTC with up to seven fractional digits. Convert
/// exactly, so a mistaken review timestamp cannot disguise the same live PID
/// as a different process. No local timezone or floating-point rounding enters.
pub fn legacy_utc_creation_time(value: &str) -> Result<u64> {
    ensure!(
        value.is_ascii() && (20..=28).contains(&value.len()),
        "invalid legacy UTC timestamp"
    );
    let bytes = value.as_bytes();
    ensure!(
        bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[10] == b'T'
            && bytes[13] == b':'
            && bytes[16] == b':'
            && bytes[value.len() - 1] == b'Z',
        "invalid legacy UTC separators"
    );
    fn number(bytes: &[u8]) -> Result<u64> {
        ensure!(
            !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit),
            "invalid timestamp digits"
        );
        Ok(bytes
            .iter()
            .fold(0, |n, digit| n * 10 + u64::from(digit - b'0')))
    }
    let year = number(&bytes[..4])?;
    let month = number(&bytes[5..7])?;
    let day = number(&bytes[8..10])?;
    let hour = number(&bytes[11..13])?;
    let minute = number(&bytes[14..16])?;
    let second = number(&bytes[17..19])?;
    ensure!(
        (1601..=9999).contains(&year)
            && (1..=12).contains(&month)
            && hour < 24
            && minute < 60
            && second < 60,
        "legacy UTC date outside supported range"
    );
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let months: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    ensure!(
        (1..=months[month as usize - 1]).contains(&day),
        "invalid Gregorian day"
    );
    let fraction = if bytes.len() == 20 {
        0
    } else {
        ensure!(
            bytes[19] == b'.' && bytes.len() >= 22,
            "invalid fractional timestamp"
        );
        let fraction = &bytes[20..bytes.len() - 1];
        number(fraction)? * 10u64.pow(7 - fraction.len() as u32)
    };
    fn before_year(year: u64) -> u64 {
        let previous = year - 1;
        365 * previous + previous / 4 - previous / 100 + previous / 400
    }
    let days = before_year(year) - before_year(1601)
        + months[..month as usize - 1].iter().sum::<u64>()
        + day
        - 1;
    Ok((days * 86400 + hour * 3600 + minute * 60 + second) * 10_000_000 + fraction)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyRecordedProcess {
    pub hostname: String,
    pub pid: u32,
    pub creation_time_100ns: u64,
    pub native_start_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LegacyDirectorySnapshot {
    /// Actual empty directory, independently linked to an abandoned upload.
    EmptyFailedStage { directory: RootIdentity },
    /// All ordinary files and all nested directories, including empty ones.
    LegacyStagedNoDispatch {
        directory: RootIdentity,
        files: Vec<FileEntry>,
        directories: Vec<RelativePath>,
    },
    /// Only the complete control-file selection below is hashed. Old CAD,
    /// images, log files, ZIPs and output directories are outside this review.
    LegacyTerminalInactive {
        directory: RootIdentity,
        control_files: Vec<FileEntry>,
        process: LegacyRecordedProcess,
    },
}

/// One top-level leaf only. The adapter must reject links/special files for all
/// selected names and enumerate the full selection, including newly added files.
pub fn legacy_terminal_control_name(name: &str) -> bool {
    if name.is_empty() || name.contains(['/', '\\']) {
        return false;
    }
    let name = path_collision_key(name);
    name.ends_with(".json")
        || name.ends_with(".ps1")
        || matches!(name.as_str(), "worker.py" | "journal.py" | "uf_probe.rs")
}

/// Inventory metadata is bounded by record/file count, not the transfer byte
/// budget: historical source archives need not fit a newly dispatched job.
fn metadata_inventory(files: &[FileEntry]) -> Result<()> {
    let mut names_only = files.to_vec();
    for file in &mut names_only {
        file.identity.bytes = 0;
    }
    validate_inventory(&names_only)
}

impl LegacyDirectorySnapshot {
    pub fn directory(&self) -> &RootIdentity {
        match self {
            Self::EmptyFailedStage { directory }
            | Self::LegacyStagedNoDispatch { directory, .. }
            | Self::LegacyTerminalInactive { directory, .. } => directory,
        }
    }

    pub fn validate(&self, root: &RootIdentity) -> Result<()> {
        let directory = self.directory();
        directory.validate()?;
        ensure!(
            directory.host_id == root.host_id
                && directory.volume_id == root.volume_id
                && directory.directory_id != root.directory_id,
            "legacy directory is not a distinct directory on the pinned root volume"
        );
        match self {
            Self::EmptyFailedStage { .. } => {}
            Self::LegacyStagedNoDispatch {
                files, directories, ..
            } => {
                metadata_inventory(files)?;
                ensure!(
                    directories.len() <= MAX_FILES,
                    "directory count exceeds bound"
                );
                let file_names = files
                    .iter()
                    .map(|f| path_collision_key(f.path.as_str()))
                    .collect::<BTreeSet<_>>();
                let mut directory_names = BTreeSet::new();
                for path in directories {
                    let name = path_collision_key(path.as_str());
                    ensure!(
                        !file_names.contains(&name) && directory_names.insert(name),
                        "duplicate or file-aliased directory"
                    );
                }
                for name in file_names.iter().chain(&directory_names) {
                    for (at, _) in name.match_indices('/') {
                        ensure!(
                            directory_names.contains(&name[..at]),
                            "incomplete directory inventory"
                        );
                    }
                }
                for required in [
                    "job.json",
                    "journal.py",
                    "worker.py",
                    "run-job.ps1",
                    "params.json",
                    "stage.json",
                ] {
                    ensure!(
                        file_names.contains(required),
                        "legacy stage source closure missing"
                    );
                }
                ensure!(
                    !file_names.iter().any(|name| {
                        name.starts_with(".nx-")
                            || matches!(
                                name.as_str(),
                                "state.json"
                                    | "state.json.tmp"
                                    | "native-result.json"
                                    | "native-result.json.tmp"
                                    | "dispatch.json"
                                    | "supervisor-process.json"
                                    | "termination.json"
                            )
                    }),
                    "staged review contains dispatch or pending evidence"
                );
            }
            Self::LegacyTerminalInactive {
                control_files,
                process,
                ..
            } => {
                metadata_inventory(control_files)?;
                ensure!(
                    control_files
                        .iter()
                        .all(|f| legacy_terminal_control_name(f.path.as_str()))
                        && control_files
                            .iter()
                            .any(|f| f.path.as_str() == "state.json"),
                    "terminal review requires the exact top-level control selection and state"
                );
                checked_label(&process.hostname)?;
                checked_label(&process.native_start_utc)?;
                ensure!(
                    process.pid > 0 && process.creation_time_100ns >= 116_444_736_000_000_000,
                    "legacy process identity missing"
                );
                ensure!(
                    legacy_utc_creation_time(&process.native_start_utc)?
                        == process.creation_time_100ns,
                    "legacy UTC start differs from native creation time"
                );
            }
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<Digest> {
        let mut ordered = self.clone();
        match &mut ordered {
            Self::EmptyFailedStage { .. } => {}
            Self::LegacyStagedNoDispatch {
                files, directories, ..
            } => {
                files.sort_by(|a, b| a.path.cmp(&b.path));
                directories.sort();
            }
            Self::LegacyTerminalInactive { control_files, .. } => {
                control_files.sort_by(|a, b| a.path.cmp(&b.path));
            }
        }
        record_digest(&ordered)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootLegacyReviewEntry {
    pub job_id: JobId,
    pub snapshot: LegacyDirectorySnapshot,
    /// These paths index the externally reviewed evidence closure below.
    pub evidence_records: Vec<RelativePath>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootLegacyReview {
    pub schema: String,
    pub scope: LegacyReviewScope,
    pub root: RootIdentity,
    pub entries: Vec<RootLegacyReviewEntry>,
    /// Bound review references; interpreting these original records is the
    /// control-plane review's job, not automatic engineering adjudication.
    pub references: Vec<FileEntry>,
    pub reviewer: String,
    pub reviewed_at_utc: String,
    pub reason: String,
}
impl RootLegacyReview {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == ROOT_LEGACY_REVIEW_SCHEMA,
            "legacy review schema differs"
        );
        self.root.validate()?;
        for label in [&self.reviewer, &self.reviewed_at_utc, &self.reason] {
            checked_label(label)?;
        }
        ensure!(
            !self.entries.is_empty() && self.entries.len() <= MAX_FILES,
            "invalid review entry count"
        );
        metadata_inventory(&self.references)?;
        let mut jobs = BTreeSet::new();
        let mut directories = BTreeSet::new();
        for entry in &self.entries {
            ensure!(
                jobs.insert(path_collision_key(entry.job_id.as_str())),
                "duplicate reviewed job"
            );
            entry.snapshot.validate(&self.root)?;
            ensure!(
                directories.insert(entry.snapshot.directory().directory_id.clone()),
                "directory reviewed under multiple jobs"
            );
            checked_label(&entry.reason)?;
            ensure!(
                !entry.evidence_records.is_empty(),
                "review entry lacks source evidence"
            );
            let mut references = BTreeSet::new();
            for path in &entry.evidence_records {
                ensure!(
                    references.insert(path)
                        && self
                            .references
                            .iter()
                            .any(|file| file.path == *path && file.identity.bytes > 0),
                    "review evidence is missing, empty or duplicated"
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LegacyCurrentProcess {
    Absent,
    DifferentCreation { actual_creation_time_100ns: u64 },
    ExitedSameIdentity { exit_code: i32 },
    LiveSameIdentity,
    Uncertain,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyProcessObservation {
    /// The PID and creation time whose inactivity was requested. The adapter
    /// must perform this OS observation afresh for every admission/status.
    pub pid: u32,
    pub creation_time_100ns: u64,
    pub observation: LegacyCurrentProcess,
}

/// Any error leaves the ordinary hold in force. This is an exact per-directory
/// exception; unlisted/new directories and v2 records never receive an exemption.
pub fn classify_reviewed_root_legacy(
    input: &HoldInput,
    id: &JobId,
    root: &RootIdentity,
    review_bytes: &[u8],
    pin: &Digest,
    current: &LegacyDirectorySnapshot,
    process: Option<&LegacyProcessObservation>,
) -> Result<HoldAssessment> {
    ensure!(
        Digest::of(review_bytes) == *pin,
        "root legacy review pin differs"
    );
    let review: RootLegacyReview = decode(review_bytes)?;
    review.validate()?;
    ensure!(review.root == *root, "legacy review root differs");
    let entry = review
        .entries
        .iter()
        .find(|entry| entry.job_id == *id)
        .ok_or_else(|| anyhow::anyhow!("directory is not independently reviewed"))?;
    current.validate(root)?;
    ensure!(
        entry.snapshot.digest()? == current.digest()?,
        "legacy directory snapshot changed"
    );
    ensure!(
        matches!(input, HoldInput::Missing | HoldInput::StateBytes(_)),
        "pending or uncertain IO cannot release a legacy hold"
    );
    let mut assessment = classify_job_hold(input, id, root);
    match current {
        LegacyDirectorySnapshot::EmptyFailedStage { .. }
        | LegacyDirectorySnapshot::LegacyStagedNoDispatch { .. } => {
            ensure!(
                matches!(input, HoldInput::Missing) && process.is_none(),
                "undispatched legacy review acquired state or process evidence"
            );
        }
        LegacyDirectorySnapshot::LegacyTerminalInactive {
            control_files,
            process: expected,
            ..
        } => {
            let HoldInput::StateBytes(bytes) = input else {
                anyhow::bail!("legacy terminal state missing")
            };
            let state_file = control_files
                .iter()
                .find(|f| f.path.as_str() == "state.json")
                .unwrap();
            state_file.identity.verify(bytes)?;
            ensure!(
                assessment.legacy_v1
                    && assessment.reason != "state_directory_or_root_binding_differs",
                "terminal review requires the same legacy v1 job"
            );
            let state = assessment.observed_record.as_ref().unwrap();
            ensure!(
                matches!(
                    state.get("state").and_then(serde_json::Value::as_str),
                    Some("COMPLETED" | "FAILED")
                ),
                "legacy review cannot clear an unresolved dispatch"
            );
            ensure!(
                state.get("native_pid").and_then(serde_json::Value::as_u64)
                    == Some(expected.pid as u64)
                    && state
                        .get("native_start_utc")
                        .and_then(serde_json::Value::as_str)
                        == Some(expected.native_start_utc.as_str())
                    && state.get("hostname").and_then(serde_json::Value::as_str)
                        == Some(expected.hostname.as_str()),
                "historical process differs from original state"
            );
            let observed =
                process.ok_or_else(|| anyhow::anyhow!("current process observation missing"))?;
            ensure!(
                observed.pid == expected.pid
                    && observed.creation_time_100ns == expected.creation_time_100ns,
                "process observation is for another identity"
            );
            match observed.observation {
                LegacyCurrentProcess::Absent | LegacyCurrentProcess::ExitedSameIdentity { .. } => {}
                LegacyCurrentProcess::DifferentCreation {
                    actual_creation_time_100ns,
                } => {
                    ensure!(
                        actual_creation_time_100ns > 0
                            && actual_creation_time_100ns != expected.creation_time_100ns,
                        "reused PID observation is not different"
                    );
                }
                LegacyCurrentProcess::LiveSameIdentity | LegacyCurrentProcess::Uncertain => {
                    anyhow::bail!("historical process inactivity remains unresolved");
                }
            }
        }
    }
    assessment.blocks_writes = false;
    assessment.reason = "reviewed_legacy_directory_writer_hold_release_only".into();
    Ok(assessment)
}
