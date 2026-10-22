//! Frozen hand-derived fixtures; these tests make no claim of native execution.
use oe_nx_worker::geometry::stepped_profile::{AxialRegion, SteppedProfile};

fn a() -> SteppedProfile {
    SteppedProfile::new(
        [11.0, -17.0, 3.0],
        &[(16.0, 12.0), (30.0, 20.0), (20.0, 15.0)],
    )
    .unwrap()
}
fn region(radius_mm: f64, z_start_mm: f64, z_end_mm: f64) -> AxialRegion {
    AxialRegion {
        radius_mm,
        z_start_mm,
        z_end_mm,
    }
}

#[test]
fn frozen_axial_profiles_match_independent_numeric_expectations() {
    let a = a();
    assert_eq!(a.bounds_mm(), [-4.0, -32.0, 3.0, 26.0, -2.0, 50.0]);
    assert!((a.volume_mm3() - 21262.29907949572).abs() < 1e-8);
    assert!((a.surface_area_mm2() / std::f64::consts::PI - 1542.0).abs() < 1e-10);
    let expected_a = [
        region(8.0, 3.0, 15.0),
        region(15.0, 15.0, 35.0),
        region(10.0, 35.0, 50.0),
    ];
    assert_eq!(a.regions(), expected_a);
    a.verify_axial_regions(&[expected_a[2], expected_a[0], expected_a[1]])
        .unwrap();

    let b = SteppedProfile::new(
        [-23.0, 31.0, -9.0],
        &[(28.0, 8.0), (12.0, 19.0), (24.0, 11.0)],
    )
    .unwrap();
    assert_eq!(b.bounds_mm(), [-37.0, 17.0, -9.0, -9.0, 45.0, 29.0]);
    assert!((b.volume_mm3() - 12051.149419170446).abs() < 1e-8);
    assert!((b.surface_area_mm2() / std::f64::consts::PI - 1324.0).abs() < 1e-10);
    let expected_b = [
        region(14.0, -9.0, -1.0),
        region(6.0, -1.0, 18.0),
        region(12.0, 18.0, 29.0),
    ];
    assert_eq!(b.regions(), expected_b);
    b.verify_axial_regions(&expected_b).unwrap();
}

#[test]
fn identical_volume_and_bounds_do_not_hide_wrong_axial_order() {
    let wrong = SteppedProfile::new(
        [11.0, -17.0, 3.0],
        &[(20.0, 15.0), (30.0, 20.0), (16.0, 12.0)],
    )
    .unwrap();
    assert_eq!(a().volume_mm3(), wrong.volume_mm3());
    assert_eq!(a().bounds_mm(), wrong.bounds_mm());
    assert_eq!(a().surface_area_mm2(), wrong.surface_area_mm2());
    assert!(a().verify_axial_regions(wrong.regions()).is_err());
}

#[test]
fn incomplete_duplicate_nonfinite_and_out_of_tolerance_regions_refuse() {
    let profile = a();
    let original = [
        region(8.0, 3.0, 15.0),
        region(15.0, 15.0, 35.0),
        region(10.0, 35.0, 50.0),
    ];
    assert!(profile.verify_axial_regions(&original[..2]).is_err());
    let mut extra = original.to_vec();
    extra.push(original[0]);
    assert!(profile.verify_axial_regions(&extra).is_err());
    let mut duplicate = original;
    duplicate[2] = original[0];
    assert!(profile.verify_axial_regions(&duplicate).is_err());
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for field in 0..3 {
            let mut wrong = original;
            match field {
                0 => wrong[0].radius_mm = invalid,
                1 => wrong[0].z_start_mm = invalid,
                _ => wrong[0].z_end_mm = invalid,
            }
            assert!(profile.verify_axial_regions(&wrong).is_err());
        }
    }
    for field in 0..3 {
        let mut wrong = original;
        match field {
            0 => wrong[0].radius_mm += 1e-5,
            1 => wrong[0].z_start_mm += 1e-5,
            _ => wrong[0].z_end_mm += 1e-5,
        }
        assert!(profile.verify_axial_regions(&wrong).is_err());
    }
    let mut reversed = original;
    reversed[0] = region(8.0, 15.0, 3.0);
    assert!(profile.verify_axial_regions(&reversed).is_err());
}

#[test]
fn dimensions_and_derived_extents_refuse_before_native_work() {
    let valid = [(16.0, 12.0), (30.0, 20.0)];
    for invalid in [
        0.0,
        -1.0,
        1e-7,
        100_001.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert!(SteppedProfile::new([0.0; 3], &[(invalid, 12.0), valid[1]]).is_err());
        assert!(SteppedProfile::new([0.0; 3], &[(16.0, invalid), valid[1]]).is_err());
    }
    for coordinate in [f64::NAN, f64::INFINITY, 1_000_001.0, -1_000_001.0] {
        for axis in 0..3 {
            let mut origin = [0.0; 3];
            origin[axis] = coordinate;
            assert!(SteppedProfile::new(origin, &valid).is_err());
        }
    }
    assert!(SteppedProfile::new([1_000_000.0, 0.0, 0.0], &valid).is_err());
    assert!(SteppedProfile::new([0.0, 0.0, 999_999.0], &valid).is_err());
    assert!(SteppedProfile::new([0.0; 3], &[(16.0, 60_000.0), (30.0, 60_000.0)]).is_err());
    assert!(SteppedProfile::new([0.0; 3], &[]).is_err());
    assert!(SteppedProfile::new([0.0; 3], &valid[..1]).is_err());
    assert!(SteppedProfile::new([0.0; 3], &[(16.0, 1.0); 33]).is_err());
    assert!(SteppedProfile::new([0.0; 3], &[(16.0, 1.0), (16.0, 2.0)]).is_err());
    assert!(SteppedProfile::new([0.0; 3], &[(16.0, 1.0), (16.0 + 1e-6, 2.0)]).is_err());
}

#[test]
fn separated_repeated_radii_remain_distinct_regions() {
    let profile = SteppedProfile::new([0.0; 3], &[(10.0, 5.0), (20.0, 4.0), (10.0, 3.0)]).unwrap();
    profile
        .verify_axial_regions(&[
            region(5.0, 9.0, 12.0),
            region(5.0, 0.0, 5.0),
            region(10.0, 5.0, 9.0),
        ])
        .unwrap();
    assert!(
        profile
            .verify_axial_regions(&[
                region(5.0, 0.0, 5.0),
                region(5.0, 0.0, 5.0),
                region(10.0, 5.0, 9.0)
            ])
            .is_err()
    );
}
