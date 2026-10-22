//! Fresh, pinned OS facts for one explicitly reviewed failed dispatch.
use crate::{adapter::*, windows::*};
use anyhow::{Result, ensure};
use oe_nx_protocol as p;
use std::{collections::BTreeMap, ffi::OsStr};

pub struct DispatchSnapshotGuard {
    pub facts: p::DispatchWriterFacts,
    files: Vec<PinnedFile>,
    directories: Vec<StableRoot>,
}
fn directory_identity(root: &StableRoot, dir: &PinnedDirectory<'_>) -> p::RootIdentity {
    p::RootIdentity {
        host_id: root_identity(root).host_id,
        volume_id: dir.identity().volume_id.clone(),
        directory_id: dir.identity().file_id.clone(),
    }
}
fn collect(
    root: &StableRoot,
    dir: &PinnedDirectory<'_>,
    prefix: &str,
    facts: &mut p::DispatchWriterSnapshot,
    records: &mut BTreeMap<String, Vec<u8>>,
    pins: &mut Vec<PinnedFile>,
    dirs: &mut Vec<StableRoot>,
) -> Result<()> {
    ensure!(
        prefix.bytes().filter(|b| *b == b'/').count() <= 64,
        "directory depth bound"
    );
    // Owned root handles retain even empty directory identities/ancestors.
    let pin = StableRoot::open(dir.path(), None)?;
    ensure!(
        root_identity(&pin) == directory_identity(root, dir),
        "directory pin identity differs"
    );
    dirs.push(pin);
    for child in std::fs::read_dir(dir.path())? {
        let child = child?;
        let name = child.file_name();
        let text = name
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("directory member encoding"))?;
        let path = p::RelativePath::new(format!("{prefix}{text}"))?;
        if child.file_type()?.is_dir() {
            let next = dir.directory(&name)?;
            facts.directories.push(p::ReviewedDirectoryIdentity {
                path: path.clone(),
                identity: directory_identity(root, &next),
            });
            collect(
                root,
                &next,
                &format!("{}/", path.as_str()),
                facts,
                records,
                pins,
                dirs,
            )?;
        } else {
            let mut file = dir
                .pin_file(&name, p::MAX_PAYLOAD_BYTES)?
                .ok_or_else(|| anyhow::anyhow!("job file disappeared"))?;
            let total = facts
                .files
                .iter()
                .map(|f| f.identity.bytes)
                .sum::<u64>()
                .checked_add(file.size())
                .ok_or_else(|| anyhow::anyhow!("inventory byte overflow"))?;
            ensure!(
                total <= p::MAX_PAYLOAD_BYTES,
                "complete job inventory byte bound"
            );
            let identity = file_identity(&mut file)?;
            if p::CONTROL_RECORDS.contains(&path.as_str())
                || p::OPTIONAL_DIAGNOSTIC_RECORDS.contains(&path.as_str())
                || path.as_str() == "params.json"
            {
                records.insert(
                    path.as_str().into(),
                    file.read_all(p::MAX_RECORD_BYTES as u64)?,
                );
            }
            facts.files.push(p::FileEntry { path, identity });
            pins.push(file);
        }
        ensure!(
            facts.files.len() <= p::MAX_FILES && facts.directories.len() <= p::MAX_FILES,
            "job inventory bound"
        );
    }
    dir.verify()?;
    Ok(())
}
fn process(expected: &p::ProcessIdentity) -> p::DispatchProcessObservation {
    let observation = match ProcessWatch::observe_historical(ProcessIdentity {
        pid: expected.pid,
        creation_time_100ns: expected.creation_time_100ns,
    }) {
        Ok(HistoricalProcessObservation::Absent) => p::DispatchProcessState::Absent,
        Ok(HistoricalProcessObservation::DifferentCreation(actual)) => {
            p::DispatchProcessState::DifferentCreation {
                actual_creation_time_100ns: actual.creation_time_100ns,
            }
        }
        Ok(HistoricalProcessObservation::ExitedSameIdentity { exit_code }) => {
            p::DispatchProcessState::ExitedSameIdentity {
                exit_code: exit_code as i32,
            }
        }
        Ok(HistoricalProcessObservation::LiveSameIdentity) => {
            p::DispatchProcessState::LiveSameIdentity
        }
        Err(_) => p::DispatchProcessState::Uncertain,
    };
    p::DispatchProcessObservation {
        expected: expected.clone(),
        observation,
    }
}
fn tree_names(
    dir: &PinnedDirectory<'_>,
    prefix: &str,
    files: &mut Vec<String>,
    dirs: &mut Vec<(String, String, String)>,
) -> Result<()> {
    ensure!(
        prefix.bytes().filter(|b| *b == b'/').count() <= 64,
        "directory depth bound"
    );
    for child in std::fs::read_dir(dir.path())? {
        let child = child?;
        let name = child.file_name();
        let text = name
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("directory member encoding"))?;
        let path = p::RelativePath::new(format!("{prefix}{text}"))?;
        if child.file_type()?.is_dir() {
            let next = dir.directory(&name)?;
            dirs.push((
                path.as_str().into(),
                next.identity().volume_id.clone(),
                next.identity().file_id.clone(),
            ));
            tree_names(&next, &format!("{}/", path.as_str()), files, dirs)?;
        } else {
            files.push(path.as_str().into());
        }
        ensure!(
            files.len() <= p::MAX_FILES && dirs.len() <= p::MAX_FILES,
            "job enumeration bound"
        );
    }
    dir.verify()?;
    Ok(())
}
impl DispatchSnapshotGuard {
    /// Caller holds the same shared root writer lock; no state is written.
    pub fn observe(
        lock: &WriterLock<'_>,
        review: &p::VerifiedDispatchWriterReview,
    ) -> Result<Self> {
        Self::collect_for_review(lock, review.job_id(), review.worker(), review.supervisor())
    }
    /// Evidence collection only: requires explicit pinned root/job/processes,
    /// creates no review, release, state transition or execution permission.
    pub fn collect_for_review(
        lock: &WriterLock<'_>,
        id: &p::JobId,
        worker: &p::ProcessIdentity,
        supervisor: &p::ProcessIdentity,
    ) -> Result<Self> {
        lock.verify()?;
        let root = lock.root();
        worker.validate()?;
        supervisor.validate()?;
        ensure!(
            worker.host_id == root_identity(root).host_id
                && supervisor.host_id == root_identity(root).host_id,
            "observation process host differs"
        );
        let dir = root.directory(OsStr::new(id.as_str()))?;
        let mut snapshot = p::DispatchWriterSnapshot {
            directory: directory_identity(root, &dir),
            files: Vec::new(),
            directories: Vec::new(),
        };
        let mut records = BTreeMap::new();
        let mut files = Vec::new();
        let mut directories = Vec::new();
        collect(
            root,
            &dir,
            "",
            &mut snapshot,
            &mut records,
            &mut files,
            &mut directories,
        )?;
        snapshot.files.sort_by(|a, b| a.path.cmp(&b.path));
        snapshot.directories.sort_by(|a, b| a.path.cmp(&b.path));
        let guard = Self {
            facts: p::DispatchWriterFacts {
                snapshot,
                records,
                worker: process(worker),
                supervisor: process(supervisor),
            },
            files,
            directories,
        };
        guard.verify(root, id)?;
        let job: p::JobManifest = p::decode(
            guard
                .facts
                .records
                .get("job.json")
                .ok_or_else(|| anyhow::anyhow!("job record missing"))?,
        )?;
        let state: p::StateRecord = p::decode(
            guard
                .facts
                .records
                .get("state.json")
                .ok_or_else(|| anyhow::anyhow!("state record missing"))?,
        )?;
        state.validate(&job)?;
        ensure!(
            job.job_id == *id
                && job.root == root_identity(root)
                && state.process.as_ref() == Some(worker),
            "observation original job/process differs"
        );
        let record: serde_json::Value = p::decode(
            guard
                .facts
                .records
                .get("receipts/supervisor-process.json")
                .ok_or_else(|| anyhow::anyhow!("supervisor record missing"))?,
        )?;
        ensure!(
            serde_json::from_value::<p::ProcessIdentity>(record["process"].clone())? == *supervisor,
            "observation supervisor differs"
        );
        Ok(guard)
    }
    pub fn verify(&self, root: &StableRoot, id: &p::JobId) -> Result<()> {
        for file in &self.files {
            file.verify()?;
        }
        for dir in &self.directories {
            dir.verify()?;
        }
        let dir = root.directory(OsStr::new(id.as_str()))?;
        ensure!(
            directory_identity(root, &dir) == self.facts.snapshot.directory,
            "job directory changed"
        );
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        tree_names(&dir, "", &mut files, &mut dirs)?;
        files.sort();
        dirs.sort();
        let expected_files: Vec<_> = self
            .facts
            .snapshot
            .files
            .iter()
            .map(|f| f.path.as_str().to_owned())
            .collect();
        let expected_dirs: Vec<_> = self
            .facts
            .snapshot
            .directories
            .iter()
            .map(|d| {
                (
                    d.path.as_str().to_owned(),
                    d.identity.volume_id.clone(),
                    d.identity.directory_id.clone(),
                )
            })
            .collect();
        ensure!(
            files == expected_files && dirs == expected_dirs,
            "job inventory changed during observation"
        );
        let latest = observe_job_records(root, OsStr::new(id.as_str()));
        let p::HoldInput::StateBytes(bytes) = hold_input(&latest) else {
            anyhow::bail!("pending/orphan/unknown state while reviewing dispatch")
        };
        ensure!(
            self.facts.records.get("state.json") == Some(&bytes),
            "state changed during review"
        );
        Ok(())
    }
}
