//! Versioned capacities are part of a job identity, never dispatch permission.
use crate::*;
use anyhow::{Result, ensure};
use oe_contracts::NxNativeTaskBudget;
use serde::{Deserialize, Serialize};
pub const LARGE_JOB_SCHEMA: &str = "oev2.nx-native-job/3";
/// Explicit transport bounds for receipts and control records. Absence keeps
/// the original fetch limits and the original serialized manifest identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NativeAuxiliaryCapacity(pub oe_contracts::NxNativeAuxiliaryBudget);
impl Eq for NativeAuxiliaryCapacity {}
impl NativeAuxiliaryCapacity {
    pub fn validate(&self) -> Result<()> {
        let b = &self.0;
        b.validate()
            .map_err(|_| anyhow::anyhow!("native_auxiliary_generated_contract"))?;
        ensure!(
            (1..=65536).contains(&b.max_files),
            "native_auxiliary_count_bound"
        );
        ensure!(
            (1..=crate::large_fetch::MAX_AUX_BYTES).contains(&b.max_total_bytes),
            "native_auxiliary_total_bound"
        );
        ensure!(
            (1..=crate::large_fetch::MAX_AUX_FILE_BYTES).contains(&b.max_file_bytes)
                && b.max_file_bytes <= b.max_total_bytes,
            "native_auxiliary_file_bound"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NativeCapacity(pub NxNativeTaskBudget);
// The generated contract contains only seven u64 fields; equality is total.
impl Eq for NativeCapacity {}
impl NativeCapacity {
    pub fn validate(&self) -> Result<()> {
        let b = &self.0;
        b.validate()
            .map_err(|_| anyhow::anyhow!("native_capacity_generated_contract"))?;
        for n in [b.max_input_bytes, b.max_output_bytes] {
            ensure!(
                (1..=64 * 1024 * 1024 * 1024).contains(&n),
                "native_total_capacity_bound"
            );
        }
        for (file, total) in [
            (b.max_input_file_bytes, b.max_input_bytes),
            (b.max_output_file_bytes, b.max_output_bytes),
        ] {
            ensure!(
                (1..=8 * 1024 * 1024 * 1024).contains(&file) && file <= total,
                "native_file_capacity_bound"
            );
        }
        for n in [b.max_input_files, b.max_output_files] {
            ensure!(
                (1..=MAX_FILES as u64).contains(&n),
                "native_file_count_bound"
            );
        }
        ensure!(
            (1..=MAX_PAYLOAD_BYTES).contains(&b.max_segment_bytes),
            "native_segment_capacity_bound"
        );
        Ok(())
    }
    pub fn inventory(&self, files: &[FileEntry], output: bool) -> Result<()> {
        self.validate()?;
        let b = &self.0;
        let (total, one, count) = if output {
            (
                b.max_output_bytes,
                b.max_output_file_bytes,
                b.max_output_files,
            )
        } else {
            (b.max_input_bytes, b.max_input_file_bytes, b.max_input_files)
        };
        ensure!(files.len() as u64 <= count, "native_inventory_count");
        ensure!(
            files.iter().all(|f| f.identity.bytes <= one),
            "native_inventory_file_size"
        );
        crate::identity::validate_inventory_with_limit(files, total)
    }
}
impl JobManifest {
    pub fn validate_inventory_capacity(&self, files: &[FileEntry], output: bool) -> Result<()> {
        match (&*self.schema, &self.capacity) {
            (JOB_SCHEMA, None) => validate_inventory(files),
            (LARGE_JOB_SCHEMA, Some(b)) => b.inventory(files, output),
            _ => anyhow::bail!("native_job_version_capacity_mismatch"),
        }
    }
    pub fn require_archive_stage(&self) -> Result<()> {
        self.validate()?;
        ensure!(
            self.schema == JOB_SCHEMA,
            "native_v3_requires_published_source_admission"
        );
        Ok(())
    }
    pub fn verify_artifacts(&self, expected: &[FileEntry], observed: &[FileEntry]) -> Result<()> {
        self.validate()?;
        self.validate_inventory_capacity(expected, true)?;
        self.validate_inventory_capacity(observed, true)?;
        ensure!(
            expected
                .iter()
                .chain(observed)
                .all(|f| f.path.as_str().starts_with("out/")),
            "artifact_outside_out"
        );
        let mut a = expected.to_vec();
        let mut b = observed.to_vec();
        a.sort_by(|x, y| x.path.cmp(&y.path));
        b.sort_by(|x, y| x.path.cmp(&y.path));
        ensure!(a == b, "artifact_closure_differs");
        Ok(())
    }
}

impl JobManifest {
    /// Auxiliary records are not native outputs and do not inherit output-file
    /// allowances. Only an explicit v3 declaration changes their original bounds.
    pub fn auxiliary_bounds(&self) -> Result<(u64, u64, usize)> {
        self.validate()?;
        Ok(match &self.auxiliary_capacity {
            Some(cap) => {
                let b = &cap.0;
                (b.max_total_bytes, b.max_file_bytes, b.max_files as usize)
            }
            None => (
                crate::large_fetch::MAX_AUX_BYTES,
                crate::large_fetch::MAX_AUX_FILE_BYTES,
                crate::large_fetch::MAX_AUX_FILES,
            ),
        })
    }
    /// Validated (total bytes, per-file bytes, file count) for filesystem owners.
    pub fn inventory_bounds(&self, output: bool) -> Result<(u64, u64, usize)> {
        self.validate()?;
        match &self.capacity {
            None => Ok((MAX_PAYLOAD_BYTES, MAX_PAYLOAD_BYTES, MAX_FILES)),
            Some(cap) => {
                let b = &cap.0;
                Ok(if output {
                    (
                        b.max_output_bytes,
                        b.max_output_file_bytes,
                        b.max_output_files as usize,
                    )
                } else {
                    (
                        b.max_input_bytes,
                        b.max_input_file_bytes,
                        b.max_input_files as usize,
                    )
                })
            }
        }
    }
}
