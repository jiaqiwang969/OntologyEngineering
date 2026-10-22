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

fn expanded(phase: &str, field: &str, angular: bool, jitter: bool) -> Value {
    let mut c = contract();
    c["maximum_travel"] = q(30., "mm");
    let old = c["samples"].as_array().unwrap().clone();
    let mut samples = Vec::new();
    for s in old {
        if s["phase"] == phase {
            let origin = s[field]["value"].as_f64().unwrap();
            for i in 0..4 {
                let mut v = s.clone();
                let delta = if jitter {
                    if i % 2 == 0 { 0.75e-6 } else { -0.75e-6 }
                } else {
                    (i + 1) as f64 * 0.75e-6
                };
                v[field] = q(origin + delta, if angular { "deg" } else { "mm" });
                if angular {
                    v["driver_angle"] = q(723. + delta, "deg");
                }
                samples.push(v);
            }
        } else {
            samples.push(s);
        }
    }
    // Continue from the held final angle so the original one-step comparison
    // cannot detect the counterexample at the next phase boundary.
    if angular && !jitter {
        for s in &mut samples {
            if s["phase"] == "tool_release" || s["phase"] == "parked" {
                s["screw_angle"] = q(720. + 3e-6, "deg");
                s["driver_angle"] = q(723. + 3e-6, "deg");
            }
        }
    }
    if phase == "tool_release" && !jitter {
        for s in &mut samples {
            if s["phase"] == "parked" {
                s["screw_travel"] = q(20. + 3e-6, "mm");
            }
        }
    }
    for (i, s) in samples.iter_mut().enumerate() {
        s["sample_id"] = json!(format!("r{i}"));
        s["time"] = q(i as f64, "s");
    }
    c["samples"] = json!(samples);
    c
}
#[test]
fn retained_angles_reject_accumulated_subtolerance_drift() {
    let r = run(expanded("held_extract", "screw_angle", true, false));
    assert_eq!(r["conclusion"], "violated", "{r}");
    assert!(
        r["failures"]
            .to_string()
            .contains("motion_accumulated_angle_reset_or_changed_after_thread")
    );
}
#[test]
fn retained_screw_position_rejects_accumulated_release_drift() {
    let r = run(expanded("tool_release", "screw_travel", false, false));
    assert_eq!(r["conclusion"], "violated", "{r}");
    assert!(
        r["failures"]
            .to_string()
            .contains("motion_screw_moves_when_driver_released")
    );
}
#[test]
fn retained_park_rejects_accumulated_driver_drift() {
    let r = run(expanded("parked", "driver_travel", false, false));
    assert_eq!(r["conclusion"], "violated", "{r}");
    assert!(
        r["failures"]
            .to_string()
            .contains("motion_parked_pose_changed")
    );
}
#[test]
fn retained_pose_allows_bounded_jitter_about_original_reference() {
    for (phase, field, angular) in [
        ("held_extract", "screw_angle", true),
        ("tool_release", "screw_travel", false),
        ("parked", "driver_travel", false),
    ] {
        let r = run(expanded(phase, field, angular, true));
        assert_eq!(r["conclusion"], "pass", "{phase}: {r}");
        assert_eq!(r["physical_holding_and_transmission"], "not_established");
    }
}
