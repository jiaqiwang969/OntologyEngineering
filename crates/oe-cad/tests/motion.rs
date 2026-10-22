use oe_cad::motion::check_drive;
use oe_contracts::DriveContract;
use serde_json::{Value, json};

fn q(value: f64, unit: &str) -> Value {
    json!({"value":value,"unit":unit,"precision":"explicit development fixture"})
}
fn contract() -> Value {
    let phases = [
        "engage",
        "takeup",
        "threaded",
        "held_extract",
        "tool_release",
        "parked",
    ];
    let screw = [0., 0., 720., 720., 720., 720.];
    let driver = [0., 3., 723., 723., 723., 723.];
    let st = [0., 0., 1., 20., 20., 20.];
    let dt = [0., 0., 1., 20., 25., 25.];
    let samples=(0..6).map(|i|json!({"sample_id":format!("s{i}"),"phase":phases[i],"time":q(i as f64,"s"),
        "screw_angle":q(screw[i],"deg"),"driver_angle":q(driver[i],"deg"),"screw_travel":q(st[i],"mm"),
        "driver_travel":q(dt[i],"mm"),"next_support":"present","screw_holder":"present"})).collect::<Vec<_>>();
    json!({"schema":"oev2.coupled-drive/1","object":{"object_id":"trial-drive","revision":"r1","condition_scope":"fixture","source_sha256":"a".repeat(64)},
        "trajectory_id":"shared-samples","coordinate_frame":"trial-world","time_basis":"fixture-clock","time_status":"bound",
        "lead":q(0.5,"mm/rev"),"lead_status":"confirmed","takeup_angle":q(3.,"deg"),"maximum_travel":q(25.,"mm"),
        "linear_tolerance":q(0.000001,"mm"),"angular_tolerance":q(0.000001,"deg"),"samples":samples,"source_refs":["fixture"]})
}
fn run(v: Value) -> Value {
    check_drive(&serde_json::from_value::<DriveContract>(v).unwrap()).unwrap()
}
#[test]
fn coupled_motion_keeps_accumulated_angles_and_distinguishes_physics() {
    let r = run(contract());
    assert_eq!(r["conclusion"], "pass");
    assert_eq!(r["conditional_geometry"], "consistent");
    assert_eq!(r["physical_holding_and_transmission"], "not_established");
    assert_eq!(r["engineering_adoption"], "not_established");
}
#[test]
fn driver_only_motion_reset_and_missing_holders_have_specific_reasons() {
    for (sample, field, value, reason) in [
        (
            2,
            "screw_angle",
            q(0., "deg"),
            "motion_driver_screw_not_coupled",
        ),
        (
            3,
            "screw_angle",
            q(0., "deg"),
            "motion_accumulated_angle_reset_or_changed_after_thread",
        ),
        (
            2,
            "next_support",
            json!("absent"),
            "motion_next_support_absent_before_loosen",
        ),
        (
            3,
            "screw_holder",
            json!("absent"),
            "motion_independent_screw_holder_absent",
        ),
    ] {
        let mut c = contract();
        c["samples"][sample][field] = value;
        let r = run(c);
        assert_eq!(r["conclusion"], "violated");
        assert!(
            r["failures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["reason"] == reason),
            "{r}"
        );
    }
}
#[test]
fn assumed_lead_unbound_clock_and_unknown_support_never_become_physical_pass() {
    for (key, value, reason) in [
        (
            "lead_status",
            "assumed",
            "motion_real_lead_or_handedness_unconfirmed",
        ),
        (
            "time_status",
            "unknown",
            "motion_animation_global_time_not_bound",
        ),
    ] {
        let mut c = contract();
        c[key] = json!(value);
        let r = run(c);
        assert_eq!(r["conclusion"], "unknown");
        assert_eq!(r["conditional_geometry"], "consistent");
        assert!(r["unknown"].to_string().contains(reason));
    }
    let mut c = contract();
    c["samples"][2]["next_support"] = json!("unknown");
    assert_eq!(run(c)["conclusion"], "unknown");
}
#[test]
fn limits_time_and_dimensions_are_independent_gates() {
    let mut c = contract();
    c["samples"][3]["time"] = q(1., "s");
    assert!(run(c)["failures"].to_string().contains("time_nonmonotonic"));
    let mut c = contract();
    c["samples"][4]["driver_travel"] = q(30., "mm");
    assert!(
        run(c)["failures"]
            .to_string()
            .contains("travel_limit_exceeded")
    );
    let mut c = contract();
    c["samples"][2]["screw_angle"] = q(720., "mm");
    assert!(
        check_drive(&serde_json::from_value::<DriveContract>(c).unwrap())
            .unwrap_err()
            .to_string()
            .contains("unit_mismatch")
    );
}
