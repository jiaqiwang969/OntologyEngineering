//! Explicit label edit/update evidence; never an observer-side refresh.
use anyhow::{Context, Result, ensure};
use oe_contracts::DrawingLocalUpdateCounts;
use serde_json::Value;
pub fn check(v: &Value, part: u32, tag: u32) -> Result<()> {
    ensure!(
        v["api"] == "NXOpen::Update::StartLocalUpdate/DoUpdate/EndLocalUpdate"
            && v["abi_version"] == 1
            && v["wrapper_code"] == 0
            && v["part_tag"] == part
            && v["dimension_tag"] == tag,
        "drawing_local_update_identity"
    );
    let counts: DrawingLocalUpdateCounts = serde_json::from_value(v["counts"].clone())?;
    counts
        .validate()
        .map_err(|_| anyhow::anyhow!("drawing_local_update_counts"))?;
    ensure!(
        counts.errors_before == 0
            && counts.errors_after_update == 0
            && counts.errors_final == 0
            && counts.delete_before == 0
            && counts.delete_final == 0
            && counts.cleanup_errors == 0,
        "drawing_local_update_errors"
    );
    for k in [
        "display_part_matches",
        "mark_created",
        "local_started",
        "write_called",
        "update_called",
        "update_returned",
        "local_ended",
        "mark_deleted",
        "prior_marks_preserved",
        "own_mark_absent",
    ] {
        ensure!(v[k] == true, "drawing_local_update_lifecycle:{k}");
    }
    for k in [
        "locked_before",
        "locked_final",
        "rollback_called",
        "rollback_returned",
        "error_list_cleared",
    ] {
        ensure!(v[k] == false, "drawing_local_update_unexpected:{k}");
    }
    ensure!(
        v["mark_id"].as_i64().is_some_and(|n| n > 0)
            && v["update_errors"] == 0
            && v["write_code"] == 0
            && v["readback_code"] == 0
            && v["error_codes"].as_array().is_some_and(|a| a.is_empty()),
        "drawing_local_update_result"
    );
    Ok(())
}
pub fn check_report(r: &Value, plan: &Value) -> Result<()> {
    let part = r["before"]["part_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("drawing_local_update_part")? as u32;
    let targets = plan["targets"]
        .as_array()
        .context("drawing_local_update_targets")?;
    for key in ["labels_written", "labels_restored"] {
        let rows = r[key].as_array().context("drawing_local_update_records")?;
        ensure!(rows.len() == targets.len(), "drawing_local_update_coverage");
        for (row, t) in rows.iter().zip(targets) {
            check(&row["update"], part, super::drawing_annotation::target(t)?)?;
        }
    }
    Ok(())
}
