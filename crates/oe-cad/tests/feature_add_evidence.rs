//! Independent authored protocol controls, not NX execution evidence.
#[path = "support/feature_add_fixture.rs"]
mod fixture;
use oe_cad::evidence::correspondence::feature_add;
use serde_json::json;
use std::fs;

#[test]
fn addition_receiving_requires_actual_lineage_and_independent_reopen() {
    let f = fixture::AddFixture::new();
    let got = feature_add::verify(&f.f.root, &f.request).unwrap();
    assert_eq!(got.request(), &f.request);
    assert_eq!(got.report()["added_feature_id"], "hole:left");
    assert_eq!(
        got.report()["current_target_mapping"],
        "unique_in_complete_reopened_geometry"
    );
    assert_eq!(got.report()["independent_reinspection_checked"], true);
    assert_eq!(got.report()["engineering_adoption"], "not_established");
    assert_eq!(got.report()["native_outcome"]["feature_tag"], 702);
    assert!(got.impact_seed_ids().contains("inspection:left"));
    assert!(got.impact_seed_ids().contains("addition-native"));
    for (pid, birth) in [(45, 45), (44, 44), (42, 42)] {
        let mut f = fixture::AddFixture::new();
        f.edit_add(|_, _, n| {
            n.process.pid = pid;
            n.process.creation_time_100ns = birth;
        });
        assert!(feature_add::verify(&f.f.root, &f.request).is_err());
    }
}
#[test]
fn addition_receiving_rejects_resealed_wrong_lineage_partial_save_and_order() {
    for (key, pointer, value) in [
        (
            "before_features",
            "/observation/features/0/owning_part_tag",
            json!(12),
        ),
        (
            "after_features",
            "/observation/features/1/feature_tag",
            json!(701),
        ),
        (
            "after_features",
            "/observation/features/1/face_tags",
            json!([608]),
        ),
        ("after_features", "/observation/complete", json!(false)),
        (
            "saved_features",
            "/observation/features/0/location_mm/0",
            json!(-39.),
        ),
        ("lineage", "/observation/feature_tag", json!(703)),
        ("call", "/observation/return_code", json!(17)),
        ("call", "/observation/update_code", json!(null)),
        ("call", "/observation/call_entered", json!(false)),
        ("call", "/observation/diameter_mm", json!(10.)),
        ("call", "/sequence", json!(100)),
        ("saved", "/observation/file_error_count", json!(1)),
        (
            "saved_reference_sets",
            "/observation/complete",
            json!(false),
        ),
    ] {
        let mut f = fixture::AddFixture::new();
        f.edit_event(key, |v| *v.pointer_mut(pointer).unwrap() = value);
        assert!(
            feature_add::verify(&f.f.root, &f.request).is_err(),
            "{key} {pointer}"
        );
    }
}
#[test]
fn addition_receiving_requires_complete_saved_sources_even_with_same_leaf() {
    let mut f = fixture::AddFixture::same_leaf_target();
    f.edit_add(|_, _, n| n.artifacts.reverse());
    feature_add::verify(&f.f.root, &f.request).unwrap();
    for (name, missing) in [
        ("out/build/root.prt", false),
        ("out/build/parts/root.prt", false),
        ("out/build/root.prt", true),
        ("out/build/parts/root.prt", true),
    ] {
        let mut f = fixture::AddFixture::same_leaf_target();
        f.edit_add(|dir, _, n| {
            if missing {
                n.artifacts.retain(|e| e.path.as_str() != name);
            } else {
                let bytes = b"Substituted synthetic source; not NX";
                fs::write(dir.join(name), bytes).unwrap();
                n.artifacts
                    .iter_mut()
                    .find(|e| e.path.as_str() == name)
                    .unwrap()
                    .identity = oe_nx_protocol::BlobIdentity::of(bytes);
            }
        });
        assert!(
            feature_add::verify(&f.f.root, &f.request).is_err(),
            "{name} {missing}"
        );
    }
}
