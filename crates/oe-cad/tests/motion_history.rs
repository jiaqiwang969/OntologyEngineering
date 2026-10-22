use oe_cad::motion::history::{ProjectionLimits, project_v262};
use oe_contracts::{ObjectIdentity, Quantity};
use serde_json::{Value, json};
fn q(value: f64, unit: &str) -> Quantity {
    Quantity {
        value,
        unit: unit.into(),
        precision: "explicit synthetic oracle".into(),
    }
}
fn pose(id: &str, angle: f64, travel: f64) -> Value {
    // Independent quarter-turn matrix with pivot [2,3,4]. An axial shift is
    // travel; its affine offset is [travel,7,1], not an error or travel norm.
    let quarter = angle == 90.0;
    let (r, t) = if quarter {
        (
            json!([[1, 0, 0], [0, 0, -1], [0, 1, 0]]),
            json!([travel, 7, 1]),
        )
    } else {
        (
            json!([[1, 0, 0], [0, 1, 0], [0, 0, 1]]),
            json!([travel, 0, 0]),
        )
    };
    let mut matrix = vec![];
    for i in 0..3 {
        matrix.push(json!([r[i][0], r[i][1], r[i][2], t[i]]));
    }
    matrix.push(json!([0, 0, 0, 1]));
    json!({"id":id,"kind":if id=="screw"{"part"}else{"tool_proxy"},"rotation_rows":r,"translation_mm":t,"transform4x4_column_vector":matrix,
        "axis_world":[1,0,0],"pivot_world_mm":[2,3,4],"angle_unwrapped_deg":angle,"angle_mod360_deg":angle,"axial_translation_mm":travel})
}
fn source() -> Value {
    let kinds = [
        "relative_takeup_to_ideal_corner_contact",
        "conditional_coupled_helical_withdrawal",
        "coupled_straight_withdrawal_under_independent_screw_hold",
        "driver_retract_from_held_parked_screw",
    ];
    let participants = [
        vec![pose("driver", 0., 0.)],
        vec![pose("screw", 90., 0.125), pose("driver", 90., 0.125)],
        vec![pose("screw", 90., 20.), pose("driver", 90., 20.)],
        vec![pose("driver", 90., 25.)],
    ];
    let actions=(0..4).map(|i|json!({"id":format!("action{i}"),"kind":kinds[i],"geometry_frame":"WORLD_Q0_baked;fixed_G002_no_A_delta",
        "stationary_participants":if i==0{vec![pose("screw",0.,0.)]}else if i==3{vec![pose("screw",90.,20.)]}else{vec![]},
        "samples":[{"sample_id":"s1","moving_participants":participants[i]}],"guard_status":"hypothesis","guards":["independent support required"]})).collect::<Vec<_>>();
    json!({"schema":"autoplate169.v262.coupled-motion.v1","units":{"length":"mm","angle":"deg"},"transform_convention":{"vector":"column"},
        "takeup_angle_deg":0.,"branches":[{"id":"branch","lead_assumption_mm_per_rev":0.5,"actions":actions}]})
}
fn run(v: &Value) -> Value {
    project_v262(
        v,
        &ObjectIdentity {
            object_id: "mechanism".into(),
            revision: "r1".into(),
            condition_scope: "trial".into(),
            source_sha256: "a".repeat(64),
        },
        "fixture",
        &ProjectionLimits {
            maximum_travel: q(25., "mm"),
            linear_tolerance: q(1e-6, "mm"),
            angular_tolerance: q(1e-6, "deg"),
        },
    )
    .unwrap()
}
#[test]
fn historical_poses_are_checked_without_inheriting_physics_time_or_verdict() {
    let r = run(&source());
    let p = &r["projections"][0];
    assert!(p["pose_failures"].as_array().unwrap().is_empty());
    assert_eq!(p["check"]["conditional_geometry"], "consistent");
    assert_eq!(p["check"]["conclusion"], "unknown");
    assert_eq!(p["drive"]["lead_status"], "assumed");
    assert_eq!(p["drive"]["time_status"], "unknown");
    assert_eq!(p["drive"]["samples"][1]["screw_travel"]["value"], 0.125);
    assert_eq!(r["engineering_adoption"], "not_established");
}
#[test]
fn affine_offset_angle_and_identity_mismatches_preserve_specific_diagnostics() {
    let mut s = source();
    s["branches"][0]["actions"][1]["samples"][0]["moving_participants"][0]["translation_mm"][1] =
        json!(0);
    assert!(
        run(&s)["projections"][0]["pose_failures"]
            .to_string()
            .contains("affine_offset_is_not_axial_travel")
    );
    let mut s = source();
    s["branches"][0]["actions"][1]["samples"][0]["moving_participants"][0]["angle_mod360_deg"] =
        json!(0);
    assert!(
        run(&s)["projections"][0]["pose_failures"]
            .to_string()
            .contains("wrapped_angle_mismatch")
    );
    let mut s = source();
    s["branches"][0]["actions"][3]["stationary_participants"] = json!([]);
    let object:ObjectIdentity=serde_json::from_value(json!({"object_id":"m","revision":"1","condition_scope":"s","source_sha256":"a".repeat(64)})).unwrap();
    let limits = ProjectionLimits {
        maximum_travel: q(25., "mm"),
        linear_tolerance: q(1e-6, "mm"),
        angular_tolerance: q(1e-6, "deg"),
    };
    assert!(project_v262(&s, &object, "fixture", &limits).is_err());
}
