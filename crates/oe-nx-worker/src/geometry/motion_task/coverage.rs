//! Bounded streaming lifecycle checker; no per-pair history retained.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeSet;
pub struct SampleCoverage {
    sample: String,
    remaining: BTreeSet<String>,
    pending_move: Option<String>,
    pending_pair: Option<Value>,
    stage: u8,
    expected_pairs: u64,
    pub pairs: u64,
    pub interference: u64,
    pub touching: u64,
}
impl SampleCoverage {
    pub fn new(sample: String, targets: BTreeSet<String>, expected_pairs: u64) -> Self {
        Self {
            sample,
            remaining: targets,
            pending_move: None,
            pending_pair: None,
            stage: 0,
            expected_pairs,
            pairs: 0,
            interference: 0,
            touching: 0,
        }
    }
    pub fn observe(&mut self, e: &Value) -> Result<()> {
        let phase = e["phase"]
            .as_str()
            .context("motion_coverage_phase_missing")?;
        match phase {
            "motion_sample_before" => {
                ensure!(
                    self.stage == 0 && e["sample_id"] == self.sample,
                    "motion_coverage_sample_before"
                );
                self.stage = 1;
            }
            "motion_reposition_pending" => {
                let id = e["occurrence_id"]
                    .as_str()
                    .context("motion_coverage_move_id")?;
                ensure!(
                    self.stage == 1
                        && self.pending_move.is_none()
                        && e["sample_id"] == self.sample
                        && self.remaining.contains(id),
                    "motion_coverage_move_pending"
                );
                self.pending_move = Some(id.into());
            }
            "motion_reposition_returned" => {
                let id = e["occurrence_id"]
                    .as_str()
                    .context("motion_coverage_move_id")?;
                ensure!(
                    self.stage == 1
                        && self.pending_move.as_deref() == Some(id)
                        && e["sample_id"] == self.sample,
                    "motion_coverage_move_returned"
                );
                ensure!(
                    e["wrapper_code"] == 0 && e["native_code"] == 0 && e["update_code"] == 0,
                    "motion_coverage_move_failed"
                );
                ensure!(self.remaining.remove(id), "motion_coverage_move_duplicate");
                self.pending_move = None;
            }
            "motion_sample_after" => {
                ensure!(
                    self.stage == 1
                        && self.remaining.is_empty()
                        && self.pending_move.is_none()
                        && e["sample_id"] == self.sample,
                    "motion_coverage_sample_after"
                );
                self.stage = 2;
            }
            "motion_pairs_before" => {
                ensure!(
                    self.stage == 2 && e["expected_pairs"].as_u64() == Some(self.expected_pairs),
                    "motion_coverage_pairs_before"
                );
                self.stage = 3;
            }
            "interference_pending" => {
                let id = &e["identity"];
                ensure!(
                    self.stage == 3
                        && self.pending_pair.is_none()
                        && self.pairs < self.expected_pairs
                        && id["pair_index"].as_u64() == Some(self.pairs),
                    "motion_coverage_pair_pending"
                );
                let a = id["a"]["body_tag"]
                    .as_u64()
                    .context("motion_coverage_body_a")?;
                let b = id["b"]["body_tag"]
                    .as_u64()
                    .context("motion_coverage_body_b")?;
                ensure!(
                    a > 0 && b > 0 && a <= u32::MAX as u64 && b <= u32::MAX as u64 && a != b,
                    "motion_coverage_body_invalid"
                );
                self.pending_pair = Some(id.clone());
            }
            "interference_returned" => {
                ensure!(
                    self.stage == 3 && self.pending_pair.as_ref() == Some(&e["identity"]),
                    "motion_coverage_pair_returned"
                );
                ensure!(
                    e["query_code"] == 0 && e["cleanup_code"] == 0,
                    "motion_coverage_pair_failed"
                );
                match e["result_raw"].as_i64() {
                    Some(1) => self.interference += 1,
                    Some(2) => {}
                    Some(3) => self.touching += 1,
                    _ => anyhow::bail!("motion_coverage_pair_unknown"),
                }
                self.pairs += 1;
                self.pending_pair = None;
            }
            "motion_pairs_after_cleanup" => {
                ensure!(
                    self.stage == 3
                        && self.pending_pair.is_none()
                        && self.pairs == self.expected_pairs,
                    "motion_coverage_pairs_incomplete"
                );
                self.stage = 4;
            }
            _ => anyhow::bail!("motion_coverage_unexpected_phase"),
        }
        Ok(())
    }
    pub fn finish(&self, summary: &Value) -> Result<()> {
        ensure!(
            self.stage == 4
                && self.pending_move.is_none()
                && self.pending_pair.is_none()
                && self.remaining.is_empty(),
            "motion_coverage_incomplete"
        );
        ensure!(
            summary["checked_pairs"].as_u64() == Some(self.pairs)
                && summary["interference_pairs"].as_u64() == Some(self.interference)
                && summary["touching_pairs"].as_u64() == Some(self.touching)
                && summary["no_interference_pairs"].as_u64()
                    == Some(self.pairs - self.interference - self.touching)
                && summary["continuous_sweep_checked"] == false
                && summary["engineering_adoption"] == "not_assessed",
            "motion_coverage_summary_differs"
        );
        Ok(())
    }
}
