//! Narrow evidence-bound historical reconciliation. The migration control
//! plane supplies the exact review digest independently of the job; this is
//! neither a registry nor an additional end-user approval workflow. Original
//! records remain untouched and no native/engineering acceptance is produced.
use crate::*;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const LEGACY_REVIEW_SCHEMA: &str = "cad-agent.nx-legacy-reconciliation/v2";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyReviewScope {
    WriterHoldReleaseOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyProcessTermination {
    pub hostname: String,
    pub native_pid: u32,
    pub native_start_utc: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyReconciliationReview {
    pub schema: String,
    pub scope: LegacyReviewScope,
    pub job_id: JobId,
    pub root: RootIdentity,
    pub original_state: BlobIdentity,
    pub process: LegacyProcessTermination,
    /// Entire reviewed raw source/build/execution/termination evidence closure.
    pub references: Vec<FileEntry>,
    pub termination_record: RelativePath,
    pub source_records: Vec<RelativePath>,
    pub native_records: Vec<RelativePath>,
    pub reviewer: String,
    pub reviewed_at_utc: String,
    pub reason: String,
}

/// `pinned_review_digest` comes from the independently reviewed migration
/// control plane, never the task payload's own requested approval. `observed`
/// is the adapter's bounded, no-symlink byte inventory of all review references.
/// Any error MUST retain the original hold. This checks record/file binding;
/// the prior named review remains responsible for interpreting the raw evidence.
pub fn classify_reviewed_legacy(
    input: &HoldInput,
    directory_job_id: &JobId,
    root: &RootIdentity,
    review_bytes: &[u8],
    pinned_review_digest: &Digest,
    observed: &[FileEntry],
) -> Result<HoldAssessment> {
    ensure!(
        Digest::of(review_bytes) == *pinned_review_digest,
        "historical review differs from control-plane binding"
    );
    let review: LegacyReconciliationReview = decode(review_bytes)?;
    ensure!(
        review.schema == LEGACY_REVIEW_SCHEMA
            && review.job_id == *directory_job_id
            && review.root == *root,
        "historical review identity differs"
    );
    root.validate()?;
    for value in [
        &review.reviewer,
        &review.reviewed_at_utc,
        &review.reason,
        &review.process.hostname,
        &review.process.native_start_utc,
    ] {
        checked_label(value)?;
    }
    ensure!(
        review.process.native_pid > 0
            && !review.source_records.is_empty()
            && !review.native_records.is_empty(),
        "historical review lacks source/native/process evidence"
    );
    verify_inventory(&review.references, observed)?;
    for path in std::iter::once(&review.termination_record)
        .chain(review.source_records.iter())
        .chain(review.native_records.iter())
    {
        ensure!(
            review
                .references
                .iter()
                .any(|f| &f.path == path && f.identity.bytes > 0),
            "historical evidence reference missing"
        );
    }
    let HoldInput::StateBytes(state_bytes) = input else {
        anyhow::bail!(
            "uncertain IO or pending state write cannot be reconciled by a historical review"
        );
    };
    review.original_state.verify(state_bytes)?;
    let mut assessment = classify_job_hold(input, directory_job_id, root);
    ensure!(
        assessment.legacy_v1 && assessment.reason != "state_directory_or_root_binding_differs",
        "review requires exact legacy job state"
    );
    let state = assessment.observed_record.as_ref().unwrap();
    ensure!(
        matches!(
            state.get("state").and_then(Value::as_str),
            Some("RUNNING" | "UNKNOWN" | "UNKNOWN_AFTER_DISPATCH" | "COMPLETED" | "FAILED")
        ),
        "legacy state lacks known dispatch shape"
    );
    ensure!(
        state.get("native_pid").and_then(Value::as_u64) == Some(review.process.native_pid as u64)
            && state.get("native_start_utc").and_then(Value::as_str)
                == Some(review.process.native_start_utc.as_str())
            && state.get("hostname").and_then(Value::as_str)
                == Some(review.process.hostname.as_str()),
        "reviewed legacy process differs"
    );
    if let Some(code) = state.get("exit_code") {
        ensure!(
            code.as_i64() == Some(i64::from(review.process.exit_code)),
            "reviewed termination contradicts persisted exit"
        );
    }
    assessment.blocks_writes = false;
    assessment.reason = "reviewed_legacy_termination_writer_hold_release_only".into();
    Ok(assessment)
}
