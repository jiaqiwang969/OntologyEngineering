//! Exact annotation layout observation and bounded restoration for display/4.
//! Dimension geometry parents are checked separately by the original owner.
use super::NativeApi;
use anyhow::{Context, Result, ensure};
use oe_contracts::DrawingAnnotationLayout;
use serde_json::{Value, json};

pub fn check(
    v: &Value,
    part: u32,
    tag: u32,
    origin_write: bool,
    center_write: bool,
) -> Result<DrawingAnnotationLayout> {
    ensure!(
        v["api"] == "NXOpen::Annotations::Dimension::AnnotationLayout"
            && v["abi_version"] == 1
            && v["wrapper_code"] == 0
            && v["part_tag"] == part
            && v["dimension_tag"] == tag
            && v["alive"] == true
            && v["owner_matches"] == true
            && v["dimension_type_matches"] == true
            && v["query_complete"] == true
            && v["origin_set_called"] == origin_write
            && v["center_set_called"] == center_write
            && v["origin_set_returned"] == origin_write
            && v["center_set_returned"] == center_write,
        "annotation_layout_query_or_write_unproven"
    );
    let s: DrawingAnnotationLayout = serde_json::from_value(v["state"].clone())?;
    s.validate()
        .map_err(|_| anyhow::anyhow!("annotation_layout_contract"))?;
    ensure!(
        s.origin.len() == 3 && s.origin.iter().all(|x| x.is_finite()),
        "annotation_layout_origin_unknown"
    );
    Ok(s)
}

pub fn query(api: &mut dyn NativeApi, part: u32, tag: u32) -> Result<Value> {
    let v = api.dimension_annotation_layout(part, tag, None, None)?;
    Ok(v)
}

fn state(v: &Value, part: u32, tag: u32) -> Result<DrawingAnnotationLayout> {
    let s = check(v, part, tag, false, false)?;
    ensure!(
        !s.associative_origin,
        "annotation_layout_associative_origin_unsupported"
    );
    Ok(s)
}

pub fn original(v: &Value, part: u32, tag: u32) -> Result<()> {
    state(v, part, tag)?;
    Ok(())
}

fn writes(old: &DrawingAnnotationLayout, current: &DrawingAnnotationLayout) -> (bool, bool) {
    let center = old.centered != current.centered;
    // Changing centering may itself move the origin. Reassert the captured
    // origin after that explicit change; never call either setter on a no-op.
    (center || old.origin != current.origin, center)
}

pub fn restore(api: &mut dyn NativeApi, part: u32, tag: u32, before: &Value) -> Result<Value> {
    let old = state(before, part, tag)?;
    let current = query(api, part, tag)?;
    let now = state(&current, part, tag)?;
    let (origin_write, center_write) = writes(&old, &now);
    let after = if origin_write || center_write {
        api.dimension_annotation_layout(
            part,
            tag,
            origin_write.then(|| [old.origin[0], old.origin[1], old.origin[2]]),
            center_write.then_some(old.centered),
        )?
    } else {
        current.clone()
    };
    // Keep failed native readback available to the caller's restoration event.
    Ok(json!({"dimension_tag":tag,"before_restore":current,"after_restore":after}))
}

pub fn check_restoration(part: u32, tag: u32, before: &Value, r: &Value) -> Result<()> {
    let old = state(before, part, tag)?;
    let now = state(&r["before_restore"], part, tag)?;
    let (origin_write, center_write) = writes(&old, &now);
    let after = check(&r["after_restore"], part, tag, origin_write, center_write)?;
    ensure!(
        r["dimension_tag"] == tag && serde_json::to_value(old)? == serde_json::to_value(after)?,
        "annotation_layout_not_restored"
    );
    Ok(())
}

pub fn capture(api: &mut dyn NativeApi, part: u32, targets: &[Value]) -> Result<Value> {
    let mut out = Vec::new();
    for t in targets {
        let tag = target(t)?;
        out.push(
            query(api, part, tag)
                .unwrap_or_else(|e| json!({"dimension_tag":tag,"error":e.to_string()})),
        );
    }
    Ok(json!(out))
}
pub fn check_capture(v: &Value, part: u32, targets: &[Value]) -> Result<()> {
    let rows = v
        .as_array()
        .filter(|a| a.len() == targets.len())
        .context("annotation_layout_capture_missing")?;
    for (row, t) in rows.iter().zip(targets) {
        original(row, part, target(t)?)?;
    }
    Ok(())
}
pub fn target(t: &Value) -> Result<u32> {
    t["dimension_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .map(|n| n as u32)
        .context("annotation_layout_target_unknown")
}

pub fn check_report(r: &Value, plan: &Value) -> Result<()> {
    let part = r["before"]["part_tag"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("annotation_layout_part_unknown")? as u32;
    let targets = plan["targets"]
        .as_array()
        .context("annotation_layout_targets_missing")?;
    let originals = r["annotation_original"]
        .as_array()
        .filter(|a| a.len() == targets.len())
        .context("annotation_layout_original_missing")?;
    let restores = r["annotation_restoration"]
        .as_array()
        .filter(|a| a.len() == targets.len())
        .context("annotation_layout_restoration_missing")?;
    let phases = r["annotation_phases"]
        .as_array()
        .filter(|a| a.len() == 4)
        .context("annotation_layout_phases_missing")?;
    for (phase, expected) in phases.iter().zip([
        "after_labels",
        "after_export",
        "after_removal",
        "after_final_observation",
    ]) {
        ensure!(
            phase["phase"] == expected,
            "annotation_layout_phase_mismatch"
        );
        let rows = phase["observations"]
            .as_array()
            .filter(|a| a.len() == targets.len())
            .context("annotation_layout_phase_incomplete")?;
        for (j, (row, t)) in rows.iter().zip(targets).enumerate() {
            let tag = target(t)?;
            let s = check(row, part, tag, false, false)?;
            ensure!(
                !s.associative_origin,
                "annotation_layout_associativity_changed"
            );
            if expected == "after_final_observation" {
                ensure!(
                    row["state"] == originals[j]["state"],
                    "annotation_layout_final_changed"
                );
            }
        }
    }
    for ((t, old), restored) in targets.iter().zip(originals).zip(restores) {
        check_restoration(part, target(t)?, old, restored)?;
    }
    Ok(())
}
