//! Pinned physical publication records checked against the current state.
//! This review neither changes state nor grants dispatch on its own.
use crate::{
    adapter,
    windows::{PinnedDirectory, PinnedFile, StableRoot},
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{
    self as p,
    published_payload::{PreparedPayload, PublishedPayloadPlan},
};
use std::{collections::BTreeSet, ffi::OsStr};
pub struct PublishedOrigin {
    plan: PublishedPayloadPlan,
    prepared: PreparedPayload,
    target: p::RootIdentity,
    pins: Vec<PinnedFile>,
    history: BTreeSet<String>,
}
impl PublishedOrigin {
    pub fn observe(
        root: &StableRoot,
        dir: &PinnedDirectory<'_>,
        job: &p::JobManifest,
        state: &p::StateRecord,
    ) -> Result<Self> {
        ensure!(
            job.root == adapter::root_identity(root)
                && dir.path() == root.path().join(job.job_id.as_str()),
            "published_origin_actual_root_differs"
        );
        root.verify()?;
        dir.verify()?;
        let mut pins = Vec::new();
        fn read<T: serde::de::DeserializeOwned>(
            dir: &PinnedDirectory<'_>,
            name: &str,
            pins: &mut Vec<PinnedFile>,
        ) -> Result<T> {
            let mut pin = dir
                .pin_file(OsStr::new(name), p::MAX_RECORD_BYTES as u64)?
                .context("published_origin_record_missing")?;
            let value = p::decode(&pin.read_all(p::MAX_RECORD_BYTES as u64)?)?;
            pin.verify()?;
            pins.push(pin);
            Ok(value)
        }
        let plan = read(dir, "source-plan.json", &mut pins)?;
        let prepared = read(dir, "stage-receipt.json", &mut pins)?;
        let mut target = adapter::root_identity(root);
        target.volume_id = dir.identity().volume_id.clone();
        target.directory_id = dir.identity().file_id.clone();
        let mut staged = p::StateRecord::prepared(job)?;
        staged.mark_staged_published(job, &plan, &prepared)?;
        let expected = serde_json::to_vec(&staged)?;
        let history = history_names(dir)?;
        for name in &history {
            let mut pin = dir
                .pin_file(OsStr::new(name), p::MAX_RECORD_BYTES as u64)?
                .context("published_history_missing")?;
            ensure!(
                pin.read_all(p::MAX_RECORD_BYTES as u64)? == expected,
                "published_history_not_original_staged_state"
            );
            pin.verify()?;
            pins.push(pin);
        }
        let review = Self {
            plan,
            prepared,
            target,
            pins,
            history,
        };
        review.validate_state(job, state)?;
        review.verify_directory(dir)?;
        Ok(review)
    }
    pub fn validate_state(&self, job: &p::JobManifest, state: &p::StateRecord) -> Result<()> {
        self.verify()?;
        state.validate_published_stage(job, &self.plan, &self.prepared, &self.target)
    }
    pub fn verify(&self) -> Result<()> {
        for pin in &self.pins {
            pin.verify()?;
        }
        Ok(())
    }
    pub fn admits_history(&self, name: &str) -> bool {
        self.history.contains(name)
    }
    pub fn verify_directory(&self, dir: &PinnedDirectory<'_>) -> Result<()> {
        self.verify()?;
        dir.verify()?;
        ensure!(
            dir.identity().volume_id == self.target.volume_id
                && dir.identity().file_id == self.target.directory_id,
            "published_history_directory_changed"
        );
        ensure!(
            history_names(dir)? == self.history,
            "published_history_namespace_changed"
        );
        Ok(())
    }
    pub fn begin_dispatch(
        &self,
        dir: &PinnedDirectory<'_>,
        job: &p::JobManifest,
        state: &mut p::StateRecord,
        intent: p::DispatchIntent,
    ) -> Result<()> {
        self.verify_directory(dir)?;
        self.validate_state(job, state)?;
        state.begin_published_dispatch(job, &self.plan, &self.prepared, &self.target, intent)?;
        self.verify_directory(dir)?;
        Ok(())
    }
    pub fn into_pins(self) -> Vec<PinnedFile> {
        self.pins
    }
}

// A history file is retained evidence of a staged write, never the active state.
fn history_names(dir: &PinnedDirectory<'_>) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(dir.path())? {
        let name = entry?
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("published_history_nonunicode_name"))?;
        if !name.starts_with("stage-recovery-") {
            continue;
        }
        let stem = name
            .strip_prefix("stage-recovery-")
            .and_then(|s| s.strip_suffix(".json"))
            .context("published_history_name_invalid")?;
        let parts = stem.split('-').collect::<Vec<_>>();
        ensure!(
            parts.len() == 2
                && parts.iter().all(|s| !s.is_empty()
                    && s.bytes().all(|b| b.is_ascii_digit())
                    && s.parse::<u64>().is_ok()),
            "published_history_name_invalid"
        );
        ensure!(names.len() < 61, "published_history_count_bound");
        names.insert(name);
    }
    Ok(names)
}
