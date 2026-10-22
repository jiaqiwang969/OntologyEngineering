//! Read-only, versioned bridge from a historical motion artifact to current
//! contracts. No historical verdict, time base or physical capacity is inherited.
use anyhow::{Context, Result, ensure};
use oe_contracts::{DriveContract, DriveSample, ObjectIdentity, Quantity};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionLimits {
    pub maximum_travel: Quantity,
    pub linear_tolerance: Quantity,
    pub angular_tolerance: Quantity,
}
fn arr<'a>(v: &'a Value, k: &str) -> Result<&'a Vec<Value>> {
    v[k].as_array()
        .with_context(|| format!("motion_history_array_missing:{k}"))
}
fn name<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .with_context(|| format!("motion_history_identity_missing:{k}"))
}
fn number(v: &Value, k: &str) -> Result<f64> {
    let n = v[k]
        .as_f64()
        .with_context(|| format!("motion_history_number_missing:{k}"))?;
    ensure!(n.is_finite(), "motion_history_nonfinite");
    Ok(n)
}
fn q(value: f64, unit: &str) -> Quantity {
    Quantity {
        value,
        unit: unit.into(),
        precision: "historical declared scalar; not measured accuracy".into(),
    }
}
fn find<'a>(a: &'a Value, s: &'a Value, id: &str) -> Result<&'a Value> {
    let moving = arr(s, "moving_participants")?
        .iter()
        .filter(|p| p["id"] == id)
        .collect::<Vec<_>>();
    ensure!(moving.len() <= 1, "motion_history_duplicate_participant");
    if let Some(p) = moving.first() {
        return Ok(*p);
    }
    let fixed = arr(a, "stationary_participants")?
        .iter()
        .filter(|p| p["id"] == id)
        .collect::<Vec<_>>();
    ensure!(
        fixed.len() == 1,
        "motion_history_stationary_participant_missing_or_duplicate"
    );
    Ok(fixed[0])
}
fn pose(p: &Value, lt: f64, at: f64) -> Result<(f64, f64, Vec<Value>)> {
    let r: [[f64; 3]; 3] = serde_json::from_value(p["rotation_rows"].clone())?;
    let t: [f64; 3] = serde_json::from_value(p["translation_mm"].clone())?;
    let m: [[f64; 4]; 4] = serde_json::from_value(p["transform4x4_column_vector"].clone())?;
    ensure!(
        r.iter()
            .flatten()
            .chain(t.iter())
            .chain(m.iter().flatten())
            .all(|n| n.is_finite()),
        "motion_history_nonfinite_matrix"
    );
    let mut failures = Vec::new();
    let declared = p.get("angle_unwrapped_deg").is_some();
    let (angle, travel, expected_r, expected_t) = if declared {
        let angle = number(p, "angle_unwrapped_deg")?;
        let travel = number(p, "axial_translation_mm")?;
        let axis: [f64; 3] = serde_json::from_value(p["axis_world"].clone())?;
        ensure!(axis == [1., 0., 0.], "motion_history_axis_unsupported");
        let pivot: [f64; 3] = serde_json::from_value(p["pivot_world_mm"].clone())?;
        ensure!(
            pivot.iter().all(|n| n.is_finite()),
            "motion_history_pivot_invalid"
        );
        let (sn, cs) = angle.rem_euclid(360.).to_radians().sin_cos();
        let er = [[1., 0., 0.], [0., cs, -sn], [0., sn, cs]];
        let et = [
            travel,
            pivot[1] - cs * pivot[1] + sn * pivot[2],
            pivot[2] - sn * pivot[1] - cs * pivot[2],
        ];
        if (number(p, "angle_mod360_deg")? - angle.rem_euclid(360.)).abs() > at {
            failures.push(json!({"reason":"motion_history_wrapped_angle_mismatch"}));
        }
        (angle, travel, er, et)
    } else {
        // A stationary pre-takeup screw has the baked WORLD_Q0 identity. It
        // cannot acquire an angle or axial travel merely from an affine offset.
        (
            0.,
            0.,
            [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            [0., 0., 0.],
        )
    };
    let rt = at.to_radians().max(1e-12);
    for i in 0..3 {
        for j in 0..3 {
            if (r[i][j] - expected_r[i][j]).abs() > rt || (m[i][j] - r[i][j]).abs() > rt {
                failures.push(json!({"reason":"motion_history_rotation_convention_mismatch","row":i,"column":j}));
            }
        }
        if (t[i] - expected_t[i]).abs() > lt || (m[i][3] - t[i]).abs() > lt {
            failures.push(json!({"reason":"motion_history_affine_offset_is_not_axial_travel","axis":i,"observed":t[i],"expected":expected_t[i]}));
        }
    }
    if m[3] != [0., 0., 0., 1.] {
        failures.push(json!({"reason":"motion_history_homogeneous_row_invalid"}));
    }
    Ok((angle, travel, failures))
}
pub fn project_v262(
    v: &Value,
    object: &ObjectIdentity,
    source_ref: &str,
    limits: &ProjectionLimits,
) -> Result<Value> {
    ensure!(
        v["schema"] == "autoplate169.v262.coupled-motion.v1",
        "motion_history_schema_unsupported"
    );
    ensure!(
        v["units"]["length"] == "mm"
            && v["units"]["angle"] == "deg"
            && v["transform_convention"]["vector"] == "column",
        "motion_history_units_or_vector_unsupported"
    );
    object
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_history_object_invalid"))?;
    ensure!(!source_ref.is_empty(), "motion_history_source_missing");
    let lt = super::q(&limits.linear_tolerance, "mm")?;
    let at = super::q(&limits.angular_tolerance, "deg")?;
    ensure!(
        lt > 0. && at > 0. && super::q(&limits.maximum_travel, "mm")? > 0.,
        "motion_history_limits_invalid"
    );
    let takeup = number(v, "takeup_angle_deg")?;
    let branches = arr(v, "branches")?;
    ensure!(
        !branches.is_empty() && branches.len() <= 16,
        "motion_history_branch_budget"
    );
    let mut projections = Vec::new();
    let mut total = 0usize;
    for branch in branches {
        let actions = arr(branch, "actions")?;
        ensure!(actions.len() <= 256, "motion_history_action_budget");
        for (index, start) in actions.iter().enumerate() {
            if start["kind"] != "relative_takeup_to_ideal_corner_contact" {
                continue;
            }
            ensure!(
                index + 3 < actions.len(),
                "motion_history_coupled_sequence_incomplete"
            );
            let sequence = &actions[index..index + 4];
            for (a, kind) in sequence.iter().zip([
                "relative_takeup_to_ideal_corner_contact",
                "conditional_coupled_helical_withdrawal",
                "coupled_straight_withdrawal_under_independent_screw_hold",
                "driver_retract_from_held_parked_screw",
            ]) {
                ensure!(a["kind"] == kind, "motion_history_coupled_sequence_changed");
                ensure!(
                    a["geometry_frame"] == "WORLD_Q0_baked;fixed_G002_no_A_delta",
                    "motion_history_geometry_frame_unsupported"
                );
            }
            let helical = &sequence[1];
            let first = arr(helical, "samples")?
                .first()
                .context("motion_history_samples_empty")?;
            let parts = arr(first, "moving_participants")?;
            let screws = parts
                .iter()
                .filter(|p| p["kind"] == "part")
                .collect::<Vec<_>>();
            let drivers = parts
                .iter()
                .filter(|p| p["kind"] == "tool_proxy")
                .collect::<Vec<_>>();
            ensure!(
                screws.len() == 1 && drivers.len() == 1,
                "motion_history_coupled_roles_ambiguous"
            );
            let screw = name(screws[0], "id")?;
            let driver = name(drivers[0], "id")?;
            let mut samples = Vec::new();
            let mut pose_failures = Vec::new();
            let mut locators = Vec::new();
            for (a, phase) in
                sequence
                    .iter()
                    .zip(["takeup", "threaded", "held_extract", "tool_release"])
            {
                let source_samples = arr(a, "samples")?;
                ensure!(
                    !source_samples.is_empty() && source_samples.len() <= 2048,
                    "motion_history_sample_budget"
                );
                for s in source_samples {
                    total += 1;
                    ensure!(total <= 16384, "motion_history_total_budget");
                    let sp = find(a, s, screw)?;
                    let dp = find(a, s, driver)?;
                    let (sa, st, sf) = pose(sp, lt, at)?;
                    let (da, dt, df) = pose(dp, lt, at)?;
                    let sample_id = format!("{}:{}", name(a, "id")?, name(s, "sample_id")?);
                    for (id, failures) in [(screw, sf), (driver, df)] {
                        for mut f in failures {
                            f["sample_id"] = json!(sample_id);
                            f["participant"] = json!(id);
                            pose_failures.push(f);
                        }
                    }
                    locators.push(json!({"sample_id":sample_id,"source_action":name(a,"id")?,"source_sample":name(s,"sample_id")?,"phase":phase}));
                    samples.push(DriveSample {
                        sample_id,
                        phase: phase.into(),
                        time: None,
                        screw_angle: q(sa, "deg"),
                        driver_angle: q(da, "deg"),
                        screw_travel: q(st, "mm"),
                        driver_travel: q(dt, "mm"),
                        next_support: "unknown".into(),
                        screw_holder: "unknown".into(),
                    });
                }
            }
            let c = DriveContract {
                schema: "oev2.coupled-drive/1".into(),
                object: object.clone(),
                trajectory_id: format!("{}:{screw}", name(branch, "id")?),
                coordinate_frame: "WORLD_Q0_baked".into(),
                time_basis: "source parameter samples; global animation clock not established"
                    .into(),
                time_status: "unknown".into(),
                lead: q(number(branch, "lead_assumption_mm_per_rev")?, "mm/rev"),
                lead_status: "assumed".into(),
                takeup_angle: q(takeup, "deg"),
                maximum_travel: limits.maximum_travel.clone(),
                linear_tolerance: limits.linear_tolerance.clone(),
                angular_tolerance: limits.angular_tolerance.clone(),
                samples,
                source_refs: vec![source_ref.into()],
            };
            let check = super::check_drive(&c)?;
            projections.push(json!({"drive":c,"check":check,"pose_failures":pose_failures,"source_locators":locators,
                "historical_guard_status":start["guard_status"],"historical_guards":start["guards"],"holding_evidence":"required; guard text is not a physical holder"}));
        }
    }
    ensure!(
        !projections.is_empty(),
        "motion_history_no_supported_coupled_sequence"
    );
    Ok(
        json!({"schema":"oev2.historical-motion-projection/1","projections":projections,"sample_count":total,
        "history_identity":"original v262 artifact retained; current Rust projection/check has new execution identity",
        "engineering_adoption":"not_established","scope":"declared scalar and column-vector poses in bounded coupled screw sequences; other actions, full machine, continuous envelope and physical capacity remain outside this check"}),
    )
}
