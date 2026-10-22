//! Finite causal motion checks; nominal geometry is not holding/transmission capacity.
use anyhow::{Context, Result, ensure};
use oe_contracts::{DriveContract, Quantity};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub mod history;
pub mod native_receiving;
pub use native_receiving::{receive, receive_reopen};
fn q(v: &Quantity, unit: &str) -> Result<f64> {
    v.validate()
        .map_err(|_| anyhow::anyhow!("motion_quantity_invalid"))?;
    ensure!(v.unit == unit, "motion_unit_mismatch:{unit}");
    Ok(v.value)
}
pub fn check_drive(c: &DriveContract) -> Result<Value> {
    c.validate()
        .map_err(|e| anyhow::anyhow!("motion_contract_invalid:{}", e.join(",")))?;
    ensure!(
        !c.samples.is_empty() && c.samples.len() <= 4096 && !c.source_refs.is_empty(),
        "motion_sample_or_source_budget"
    );
    let lead = q(&c.lead, "mm/rev")?;
    let takeup = q(&c.takeup_angle, "deg")?;
    let max = q(&c.maximum_travel, "mm")?;
    let lt = q(&c.linear_tolerance, "mm")?;
    let at = q(&c.angular_tolerance, "deg")?;
    ensure!(
        lead != 0.0 && takeup >= 0.0 && max > 0.0 && lt > 0.0 && at > 0.0,
        "motion_parameters_outside_contract"
    );
    let mut ids = BTreeSet::new();
    let mut failures = Vec::new();
    let mut unknown = Vec::new();
    let mut previous: Option<(f64, f64, f64, f64)> = None;
    // Held poses are fixed references, never a rolling per-sample allowance.
    let mut retained_angles: Option<(f64, f64)> = None;
    let mut release_screw: Option<f64> = None;
    let mut parked_pose: Option<(f64, f64)> = None;
    let mut previous_support: Option<(&str, &str)> = None;
    let mut last_time = None;
    let mut stage = 0usize;
    let mut phases = BTreeSet::new();
    for s in &c.samples {
        s.validate()
            .map_err(|_| anyhow::anyhow!("motion_sample_invalid"))?;
        ensure!(ids.insert(&s.sample_id), "motion_duplicate_sample");
        let (sa, da, st, dt) = (
            q(&s.screw_angle, "deg")?,
            q(&s.driver_angle, "deg")?,
            q(&s.screw_travel, "mm")?,
            q(&s.driver_travel, "mm")?,
        );
        let phase = [
            "engage",
            "takeup",
            "threaded",
            "held_extract",
            "tool_release",
            "parked",
        ]
        .iter()
        .position(|p| *p == s.phase)
        .context("motion_phase_unsupported")?;
        phases.insert(s.phase.as_str());
        if phase < stage {
            failures.push(json!({"sample_id":s.sample_id,"reason":"motion_phase_reversal","prior_phase":stage,"phase":s.phase}));
        }
        stage = phase;
        if st.abs() > max + lt || dt.abs() > max + lt {
            failures.push(json!({"sample_id":s.sample_id,"reason":"motion_travel_limit_exceeded","screw_mm":st,"driver_mm":dt}));
        }
        if c.time_status == "bound" {
            let time = q(s.time.as_ref().context("motion_global_time_missing")?, "s")?;
            if time < 0.0 || last_time.is_some_and(|last| time <= last) {
                failures.push(
                    json!({"sample_id":s.sample_id,"reason":"motion_global_time_nonmonotonic"}),
                );
            }
            last_time = Some(time);
        } else if let Some(time) = &s.time {
            q(time, "s")?;
        }
        let mut add = |reason: &str| {
            failures.push(json!({"sample_id":s.sample_id,"phase":s.phase,"reason":reason}))
        };
        match s.phase.as_str() {
            "engage" | "takeup" => {
                if sa.abs() > at || st.abs() > lt || dt.abs() > lt || da < -at || da > takeup + at {
                    add("motion_takeup_moves_screw_or_exceeds_clearance");
                }
            }
            "threaded" => {
                retained_angles = Some((sa, da));
                if (da - sa - takeup).abs() > at {
                    add("motion_driver_screw_not_coupled");
                }
                if (st - lead * sa / 360.0).abs() > lt || (dt - st).abs() > lt {
                    add("motion_helical_lead_or_axial_coupling_mismatch");
                }
                // A support appearing in the already-displaced sample cannot
                // establish that takeover preceded loosening. Compare against
                // the declared zero pose, not rolling sample deltas: small
                // increments must not hide entry into a loosened state.
                if (sa.abs() > at || st.abs() > lt)
                    && !previous_support.is_some_and(|(_, status)| status == "present")
                {
                    unknown.push(json!({"sample_id":s.sample_id,
                        "reason":"motion_support_predecessor_not_established",
                        "prior_sample_id":previous_support.map(|(id, _)| id),
                        "prior_support":previous_support.map(|(_, status)| status)}));
                }
                match s.next_support.as_str() {
                    "absent"=>add("motion_next_support_absent_before_loosen"),
                    "unknown"=>unknown.push(json!({"sample_id":s.sample_id,"reason":"motion_next_support_unknown_before_loosen"})),
                    _=>()
                }
            }
            "held_extract" | "tool_release" | "parked" => {
                if let Some((last_sa, last_da, last_st, last_dt)) = previous {
                    let (held_sa, held_da) = retained_angles.unwrap_or((last_sa, last_da));
                    if (sa - held_sa).abs() > at || (da - held_da).abs() > at {
                        add("motion_accumulated_angle_reset_or_changed_after_thread");
                    }
                    if s.phase == "tool_release" {
                        let held_st = *release_screw.get_or_insert(last_st);
                        if (st - held_st).abs() > lt {
                            add("motion_screw_moves_when_driver_released");
                        }
                    }
                    if s.phase == "parked" {
                        let (held_st, held_dt) = *parked_pose.get_or_insert((last_st, last_dt));
                        if (st - held_st).abs() > lt || (dt - held_dt).abs() > lt {
                            add("motion_parked_pose_changed");
                        }
                    }
                } else {
                    add("motion_extraction_without_predecessor");
                }
                if s.phase == "held_extract" && (dt - st).abs() > lt {
                    add("motion_extract_participants_axially_separated");
                }
                match s.screw_holder.as_str(){
                    "absent"=>add("motion_independent_screw_holder_absent"),
                    "unknown"=>unknown.push(json!({"sample_id":s.sample_id,"reason":"motion_independent_screw_holder_unknown"})),_=>()
                }
            }
            _ => (),
        }
        previous = Some((sa, da, st, dt));
        previous_support = Some((&s.sample_id, &s.next_support));
    }
    if c.time_status != "bound" {
        unknown.push(json!({"reason":"motion_animation_global_time_not_bound","trajectory_id":c.trajectory_id,"time_basis":c.time_basis}));
    }
    if c.lead_status != "confirmed" {
        unknown.push(json!({"reason":"motion_real_lead_or_handedness_unconfirmed","declared_lead":c.lead,"status":c.lead_status}));
    }
    for required in ["takeup", "threaded", "held_extract", "tool_release"] {
        if !phases.contains(required) {
            unknown.push(json!({"reason":"motion_required_phase_not_covered","phase":required}));
        }
    }
    let conclusion = if !failures.is_empty() {
        "violated"
    } else if !unknown.is_empty() {
        "unknown"
    } else {
        "pass"
    };
    Ok(
        json!({"schema":"oev2.coupled-drive-check/1","execution":"completed","conclusion":conclusion,"failures":failures,"unknown":unknown,
        "trajectory_id":c.trajectory_id,"coordinate_frame":c.coordinate_frame,"sample_count":c.samples.len(),
        "conditional_geometry":if failures.is_empty(){"consistent"}else{"violated"},
        "physical_holding_and_transmission":"not_established","real_disengagement":"not_established","engineering_adoption":"not_established",
        "scope":"same-sample scalar helical motion, limits, ordering and declared support/holder; full envelope, actual force, vacuum, tolerance and final registration require independent evidence"}),
    )
}
