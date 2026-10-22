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
fn support_must_precede_screw_motion_not_just_appear_in_same_sample() {
    for status in ["absent", "unknown"] {
        let mut c = contract();
        c["samples"][1]["next_support"] = json!(status);
        let r = run(c);
        assert_eq!(r["conclusion"], "unknown", "{r}");
        assert!(
            r["unknown"]
                .to_string()
                .contains("motion_support_predecessor_not_established")
        );
        assert_eq!(r["physical_holding_and_transmission"], "not_established");
    }
    assert_eq!(run(contract())["conclusion"], "pass");
}
#[test]
fn no_previous_sample_cannot_prove_preexisting_support() {
    let mut c = contract();
    c["samples"].as_array_mut().unwrap().drain(0..2);
    let r = run(c);
    assert!(
        r["unknown"]
            .to_string()
            .contains("motion_support_predecessor_not_established"),
        "{r}"
    );
}
#[test]
fn stationary_threaded_sample_can_establish_support_before_later_motion() {
    let mut c = contract();
    c["samples"][1]["next_support"] = json!("absent");
    let mut s = c["samples"][2].clone();
    s["sample_id"] = json!("support-arrives");
    s["time"] = q(1.5, "s");
    s["screw_angle"] = q(0., "deg");
    s["driver_angle"] = q(3., "deg");
    s["screw_travel"] = q(0., "mm");
    s["driver_travel"] = q(0., "mm");
    c["samples"].as_array_mut().unwrap().insert(2, s);
    assert_eq!(run(c)["conclusion"], "pass");
}
#[test]
fn reverse_motion_and_negative_lead_still_need_prior_support() {
    let mut c = contract();
    c["lead"] = q(-0.5, "mm/rev");
    for s in c["samples"].as_array_mut().unwrap().iter_mut().skip(2) {
        s["screw_angle"] = q(-720., "deg");
        s["driver_angle"] = q(-717., "deg");
    }
    assert_eq!(run(c.clone())["conclusion"], "pass");
    c["samples"][1]["next_support"] = json!("absent");
    assert_eq!(run(c)["conclusion"], "unknown");
}
