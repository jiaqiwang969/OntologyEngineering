use oe_cad::variant::{self, NativeHole, RigidPlacement};
use oe_contracts::{CenteredHoleRow, ObjectIdentity};
use serde_json::{Value, json};
fn row(l: &str, p: &str, e: &str, c: &str) -> CenteredHoleRow {
    CenteredHoleRow {
        schema: "oev2.centered-hole-row/1".into(),
        row_id: "row".into(),
        object: ObjectIdentity {
            object_id: "part".into(),
            revision: "r1".into(),
            condition_scope: "design".into(),
            source_sha256: "a".repeat(64),
        },
        local_frame: "part-local".into(),
        unit: "mm".into(),
        length: l.into(),
        pitch: p.into(),
        minimum_center_to_end: e.into(),
        center: c.into(),
        diameter: "0.01".into(),
        source_refs: vec!["fixture".into()],
    }
}
#[test]
fn fixed_decimal_count_transitions_even_symmetry_restore_and_repeat() {
    let below = variant::plan(&row("723.999999999", "80", "42", "0")).unwrap();
    let at = variant::plan(&row("724", "80", "42", "0")).unwrap();
    assert_eq!(below.count, 8);
    assert_eq!(at.count, 9);
    assert_eq!(
        variant::plan(&row(".3", ".1", ".05", "0"))
            .unwrap()
            .centers_exact,
        ["-0.1", "0", "0.1"]
    );
    let even = variant::plan(&row("524", "80", "42", "17.5")).unwrap();
    assert_eq!(
        even.centers_exact,
        ["-182.5", "-102.5", "-22.5", "57.5", "137.5", "217.5"]
    );
    for (l, n, margin) in [
        ("924", 11, "62"),
        ("724", 9, "42"),
        ("524", 6, "62"),
        ("924", 11, "62"),
    ] {
        let r = row(l, "80", "42", "0");
        let a = variant::plan(&r).unwrap();
        assert_eq!(a.count, n);
        assert_eq!(a.end_margin_exact, margin);
        assert_eq!(a, variant::plan(&r).unwrap());
    }
    assert_eq!(
        variant::plan(&row("84", "80", "42", "0"))
            .unwrap()
            .centers_exact,
        ["0"]
    );
    assert_eq!(
        variant::plan(&row("3e-1", "1e-1", "5e-2", "0"))
            .unwrap()
            .count,
        3
    );
}
#[test]
fn impossible_nonfinite_zero_and_unbounded_rows_are_refused() {
    for (l, p, e) in [
        ("83", "80", "42"),
        ("100", "0", "10"),
        ("NaN", "80", "42"),
        ("100", "80", "-1"),
        ("1", "1e-18", "0"),
        ("1e-2147483648", "1", "0"),
    ] {
        assert!(variant::plan(&row(l, p, e, "0")).is_err());
    }
}
#[test]
fn follower_uses_controller_count_not_own_longer_length() {
    let c = row("724", "80", "42", "0");
    let mut f = row("800", "80", "42", "0");
    f.row_id = "follower".into();
    let v = variant::plan_mating(&c, &[f.clone()]).unwrap();
    assert_eq!(v["followers"][0]["layout"]["count"], 9);
    assert_eq!(v["followers"][0]["layout"]["end_margin_exact"], "80");
    // At800 its independent maximum would be9 here; a substantially longer
    // member still retains nine rather than declaring unmatched extras.
    f.length = "1000".into();
    assert_eq!(variant::plan(&f).unwrap().count, 12);
    assert_eq!(
        variant::plan_mating(&c, &[f.clone()]).unwrap()["followers"][0]["layout"]["count"],
        9
    );
    let expected = variant::plan_follower(&c, &f).unwrap();
    let observed = expected
        .centers
        .iter()
        .enumerate()
        .map(|(i, x)| NativeHole {
            center: [*x, 0., 0.],
            axis: [0., 0., 1.],
            diameter: 0.01,
            native_face_id: format!("observed-{i}"),
        })
        .collect::<Vec<_>>();
    let placement = RigidPlacement {
        rotation: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        translation: [0.; 3],
    };
    assert!(
        variant::check_native(
            &variant::plan(&f).unwrap(),
            &observed,
            [0.; 2],
            &placement,
            0.001
        )
        .is_err()
    );
    assert_eq!(
        variant::check_native(&expected, &observed, [0.; 2], &placement, 0.001).unwrap()["native_geometry"],
        "pass"
    );
    let mut pitch_error = f.clone();
    pitch_error.pitch = "79".into();
    assert!(variant::plan_follower(&c, &pitch_error).is_err());
    f.length = "600".into();
    assert!(variant::plan_mating(&c, &[f]).is_err());
}
#[test]
fn parameter_binding_compares_decimal_meaning_frames_and_sources() {
    let a = row("290", "80", "42", "0");
    let mut b = a.clone();
    b.length = "2.9e2".into();
    b.pitch = "80.000".into();
    assert!(variant::design_differences(&a, &b).unwrap().is_empty());
    b.length = "158".into();
    assert_eq!(variant::design_differences(&a, &b).unwrap(), ["length"]);
    b = a.clone();
    b.local_frame = "another-frame".into();
    assert_eq!(
        variant::design_differences(&a, &b).unwrap(),
        ["local_frame"]
    );
    b = a.clone();
    b.source_refs.push("fixture".into());
    assert!(variant::design_differences(&a, &b).is_err());
}
#[test]
fn native_axes_use_rigid_assembly_transform_and_reject_diameter_phase_count_errors() {
    let mut r = row("180", "40", "10", "0");
    r.diameter = "6".into();
    let layout = variant::plan(&r).unwrap();
    let t = RigidPlacement {
        rotation: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [12.0, 25.0, 70.0],
    };
    let holes: Vec<_> = layout
        .centers
        .iter()
        .enumerate()
        .map(|(i, x)| NativeHole {
            center: t.point([*x, 3.0, 50.0]),
            axis: t.vector([0.0, 0.0, -1.0]),
            diameter: 6.0,
            native_face_id: format!("f{i}"),
        })
        .collect();
    assert_eq!(
        variant::check_native(&layout, &holes, [3.0, 0.0], &t, 0.005).unwrap()["native_geometry"],
        "pass"
    );
    let mut bad = holes.clone();
    bad[0].center[0] += 0.01;
    assert!(variant::check_native(&layout, &bad, [3.0, 0.0], &t, 0.005).is_err());
    bad = holes.clone();
    bad[0].diameter = 6.1;
    assert!(variant::check_native(&layout, &bad, [3.0, 0.0], &t, 0.005).is_err());
    assert!(variant::check_native(&layout, &holes[..4], [3.0, 0.0], &t, 0.005).is_err());
    let scale = RigidPlacement {
        rotation: [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0; 3],
    };
    assert!(scale.validate().is_err());
}
#[test]
fn historical_variant_expected_centers_and_counts_are_preserved() {
    // Independently frozen historical numbers, not a generated golden file.
    let expected: Value = json!([
        {"length":"924","count":11,"centers":[-400,-320,-240,-160,-80,0,80,160,240,320,400]},
        {"length":"724","count":9,"centers":[-320,-240,-160,-80,0,80,160,240,320]},
        {"length":"524","count":6,"centers":[-200,-120,-40,40,120,200]},
        {"length":"924","count":11,"centers":[-400,-320,-240,-160,-80,0,80,160,240,320,400]}
    ]);
    for v in expected.as_array().unwrap() {
        let p = variant::plan(&row(v["length"].as_str().unwrap(), "80", "42", "0")).unwrap();
        assert_eq!(p.count as u64, v["count"].as_u64().unwrap());
        assert_eq!(
            p.centers,
            v["centers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect::<Vec<_>>()
        );
    }
}
