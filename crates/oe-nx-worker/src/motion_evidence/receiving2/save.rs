//! Explicit raw save observation. Valid wire is separate from save success.
use super::{decode, fields};
use anyhow::{Context, Result, ensure};
use oe_contracts::NxMotionSaveObservation2;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveState {
    NotStarted,
    PendingUnknown,
    Failed,
    Incomplete,
    CallSucceeded,
}
impl SaveState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::PendingUnknown => "pending_unknown",
            Self::Failed => "failed",
            Self::Incomplete => "incomplete",
            Self::CallSucceeded => "call_succeeded",
        }
    }
}

pub fn parse_save(value: &Value) -> Result<NxMotionSaveObservation2> {
    fields("NxMotionSaveObservation2", value)?;
    for entry in value["file_errors"]
        .as_array()
        .context("motion2_save_error_array")?
    {
        fields("NxMotionSaveFileError2", entry)?;
    }
    let record = decode("NxMotionSaveObservation2", value)?;
    validate_save(&record)?;
    Ok(record)
}

pub fn validate_save(s: &NxMotionSaveObservation2) -> Result<SaveState> {
    s.validate()
        .map_err(|_| anyhow::anyhow!("motion2_save_generated_invalid"))?;
    ensure!(
        s.schema == "oev2.nx-motion-save-observation/2" && s.api == "UF_PART_save_all",
        "motion2_save_version"
    );
    for value in [
        s.loaded_part_count_raw,
        s.return_code,
        s.file_error_count_raw,
    ]
    .into_iter()
    .flatten()
    {
        i32::try_from(value).context("motion2_save_i32_range")?;
    }
    ensure!(
        s.loaded_owned_paths.len() <= 4096
            && s.file_errors.len() <= 4096
            && s.diagnostic_codes.len() <= 16,
        "motion2_save_array_bound"
    );
    let mut paths = BTreeSet::new();
    ensure!(
        s.loaded_owned_paths
            .iter()
            .all(|p| !p.is_empty() && !p.chars().any(char::is_control) && paths.insert(p)),
        "motion2_save_path_empty_or_duplicate"
    );
    let mut diagnostics = BTreeSet::new();
    ensure!(
        s.diagnostic_codes
            .iter()
            .all(|d| (1..=96).contains(&d.len())
                && d.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.:-".contains(&b))
                && diagnostics.insert(d)),
        "motion2_save_diagnostic_bound"
    );
    for (index, entry) in s.file_errors.iter().enumerate() {
        ensure!(entry.index == index as u64, "motion2_save_error_index");
        u32::try_from(entry.part_tag).context("motion2_save_error_tag_range")?;
        i32::try_from(entry.return_code).context("motion2_save_error_code_range")?;
    }
    let no_return = s.return_code.is_none()
        && s.file_error_count_raw.is_none()
        && s.file_error_storage == "not_observed"
        && s.file_errors_capture == "not_observed"
        && s.file_errors.is_empty();
    if s.phase == "preflight_refused" {
        ensure!(
            s.preflight_status == "rejected"
                && !s.loaded_paths_complete
                && s.call_entered == Some(false)
                && s.call_returned == Some(false)
                && no_return
                && !s.diagnostic_codes.is_empty(),
            "motion2_save_refusal_combination"
        );
        match s.loaded_part_count_raw {
            Some(n) if (1..=4096).contains(&n) => ensure!(
                s.loaded_owned_paths.len() < n as usize,
                "motion2_save_refusal_prefix"
            ),
            _ => ensure!(
                s.loaded_owned_paths.is_empty(),
                "motion2_save_invalid_inventory_iterated"
            ),
        }
        return Ok(SaveState::NotStarted);
    }
    let count = s
        .loaded_part_count_raw
        .context("motion2_save_inventory_not_observed")?;
    ensure!(
        s.preflight_status == "passed"
            && s.loaded_paths_complete
            && (1..=4096).contains(&count)
            && s.loaded_owned_paths.len() == count as usize,
        "motion2_save_preflight_incomplete"
    );
    if s.phase == "call_pending" {
        ensure!(
            s.call_entered.is_none()
                && s.call_returned.is_none()
                && no_return
                && s.diagnostic_codes.is_empty(),
            "motion2_save_pending_is_not_entry"
        );
        return Ok(SaveState::PendingUnknown);
    }
    ensure!(
        s.phase == "call_returned" && s.call_entered == Some(true) && s.call_returned == Some(true),
        "motion2_save_return_combination"
    );
    let code = s.return_code.context("motion2_save_return_missing")?;
    let n = s
        .file_error_count_raw
        .context("motion2_save_header_missing")?;
    ensure!(
        ["null_both", "parts_only", "codes_only", "both_present"]
            .contains(&s.file_error_storage.as_str()),
        "motion2_save_storage_not_observed"
    );
    let valid_shape = (0..=4096).contains(&n) && (n == 0 || s.file_error_storage == "both_present");
    if !valid_shape {
        ensure!(
            s.file_errors_capture == "invalid_shape"
                && s.file_errors.is_empty()
                && !s.diagnostic_codes.is_empty(),
            "motion2_save_invalid_shape_not_complete"
        );
        return Ok(SaveState::Failed);
    }
    if n == 0 {
        // Retain original owner semantics: zero count permits any NULL shape.
        ensure!(
            s.file_errors_capture == "complete" && s.file_errors.is_empty(),
            "motion2_save_zero_header_capture"
        );
    } else {
        match s.file_errors_capture.as_str() {
            "complete" => ensure!(
                s.file_errors.len() == n as usize,
                "motion2_save_complete_count"
            ),
            "partial" => {
                ensure!(
                    s.file_errors.len() < n as usize && !s.diagnostic_codes.is_empty(),
                    "motion2_save_partial_prefix"
                );
                return Ok(SaveState::Incomplete);
            }
            _ => anyhow::bail!("motion2_save_valid_shape_capture"),
        }
    }
    Ok(if code == 0 && n == 0 {
        SaveState::CallSucceeded
    } else {
        SaveState::Failed
    })
}
