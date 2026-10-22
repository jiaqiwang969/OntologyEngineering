//! Independent synthetic input/expected outcomes; no native execution implied.
#[path = "support/drafting_reopen_fixture.rs"]
mod fixture;
use fixture::Fixture;
use oe_cad::drawing::{associations, drafting};
use serde_json::json;

#[test]
fn prototype_drafting_reopen_binds_saved_bytes_and_keeps_old_dimension_obligations() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path());
    let report = drafting::compare_saved_reopened(&f.saved, &f.reopened).unwrap();
    assert_eq!(report["schema"], "oev2.prototype-drafting-reopen/1");
    assert_eq!(report["conclusion"], "pass");
    assert_eq!(report["checks"]["independent_reopen"]["conclusion"], "pass");
    assert_ne!(report["original_identity"], report["saved_identity"]);
    assert_eq!(
        report["original_plan"]["object"]["source_sha256"],
        f.plan.object.source_sha256
    );
    assert_eq!(
        report["saved_observation_plan"]["object"]["source_sha256"],
        report["saved_identity"]["sha256"]
    );
    assert_eq!(report["observations"]["after_save"]["part_tag"], 50);
    assert_eq!(
        report["observations"]["independent_reopen"]["part_tag"],
        1050
    );
    assert_eq!(
        report["observations"]["independent_reopen"]["annotations"]["dimensions"][0]["computed_size"]
            ["value"],
        40
    );
    assert_eq!(
        report["observations"]["independent_reopen"]["annotations"]["dimensions"][0]["retained"],
        true
    );
    for field in [
        "existing_annotation_validity",
        "independent_geometry_equivalence",
        "visual_readability",
    ] {
        assert_eq!(report[field], "not_assessed", "{field}");
    }
    assert_eq!(report["dimension_obligations"], "unchanged");
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(report["manufacturing_drawing"], "not_established");
    assert!(associations::compare(&f.saved, &f.reopened).is_err());
    assert!(associations::compare_sheet_reference(&f.saved, &f.reopened).is_err());
}

#[test]
fn prototype_drafting_reopen_rejects_process_source_artifact_and_namespace_substitution() {
    for fault in [
        "same-process",
        "source-bytes",
        "output-bytes",
        "missing-artifact",
        "source-plan",
        "reopen-inputs",
        "same-leaf-path",
        "tool",
        "failed-native",
        "creation-incomplete",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let f = Fixture::new(directory.path());
        match fault {
            "same-process" => f.use_same_process(),
            "source-bytes" => std::fs::write(f.saved.join("sources/main.rs"), b"changed").unwrap(),
            "output-bytes" => {
                std::fs::write(f.saved.join("out/plates/part.prt"), b"changed").unwrap()
            }
            "missing-artifact" => f.edit_result(true, |r| r.artifacts.clear()),
            "source-plan" => f.edit_parameters(true, |v| {
                v["plan"]["object"]["source_sha256"] = json!("a".repeat(64))
            }),
            "reopen-inputs" => f.edit_parameters(false, |v| v["parts"] = json!(["other/part.prt"])),
            "same-leaf-path" => f.edit_result(false, |r| {
                r.readback["parts"][0]["path"] = json!("C:/other/inputs/other/part.prt")
            }),
            "tool" => f.edit_result(false, |r| {
                r.worker = oe_nx_protocol::BlobIdentity::of(b"another worker")
            }),
            "failed-native" => f.edit_result(true, |r| {
                r.outcome = oe_nx_protocol::ResultOutcome::Failed;
                r.errors.push("synthetic failed save".into());
            }),
            "creation-incomplete" => {
                f.edit_result(true, |r| r.readback["creation"]["complete"] = json!(false))
            }
            _ => unreachable!(),
        }
        assert!(
            drafting::compare_saved_reopened(&f.saved, &f.reopened).is_err(),
            "{fault}"
        );
    }
}

#[test]
fn prototype_drafting_reopen_preserves_unknown_and_violated_page_checks() {
    for (pointer, value, expected) in [
        (
            "/drawing_sheets/0/views/0/rotation/return_code",
            json!(17),
            "unknown",
        ),
        ("/drawing_sheets/0/sheet_info/width", json!(999), "violated"),
        (
            "/drawing_sheets/0/views/0/stale_after",
            json!(true),
            "violated",
        ),
        (
            "/drawing_sheets/0/views/0/reference_point/point",
            json!([170, 110]),
            "violated",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let f = Fixture::new(directory.path());
        f.edit_result(false, |r| {
            *r.readback["parts"][0].pointer_mut(pointer).unwrap() = value
        });
        let report = drafting::compare_saved_reopened(&f.saved, &f.reopened).unwrap();
        assert_eq!(report["conclusion"], expected, "{pointer}");
        assert_eq!(
            report["checks"]["independent_reopen"]["conclusion"], expected,
            "{pointer}"
        );
        assert_eq!(report["engineering_adoption"], "not_established");
    }
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path());
    f.edit_result(false, |r| {
        r.readback["parts"][0]["drawing_sheets"][0]["views"][0]["visible_objects"]["visible_count"] = json!(0);
        r.readback["parts"][0]["drawing_sheets"][0]["views"][0]["visible_objects"]["visible_tags"] = json!([]);
    });
    let report = drafting::compare_saved_reopened(&f.saved, &f.reopened).unwrap();
    assert_eq!(
        report["conclusion"], "violated",
        "empty prototype view cannot qualify"
    );
    assert_eq!(
        report["checks"]["independent_reopen"]["reason"],
        "native_drafting_empty_or_clipped_view"
    );
}

#[test]
fn prototype_drafting_reopen_rejects_changed_geometry_annotations_and_incomplete_inventory() {
    for fault in [
        "geometry",
        "annotations",
        "missing-catalog",
        "duplicate-sheet",
        "refresh",
        "save-requested",
        "wrong-owner",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let f = Fixture::new(directory.path());
        match fault {
            "geometry" => f.edit_result(true, |r| {
                r.readback["parts"][0]["after_save"]["bbox"] = json!([])
            }),
            "annotations" => f.edit_result(true, |r| {
                r.readback["parts"][0]["after_save"]["annotations"]["dimensions"] = json!([])
            }),
            "missing-catalog" => f.edit_result(false, |r| {
                r.readback["parts"][0]["drawing_catalog_complete"] = json!(false)
            }),
            "duplicate-sheet" => f.edit_result(false, |r| {
                let sheet = r.readback["parts"][0]["drawing_sheets"][0].clone();
                r.readback["parts"][0]["drawing_sheets"]
                    .as_array_mut()
                    .unwrap()
                    .push(sheet);
                r.readback["parts"][0]["drawing_sheet_count"] = json!(2);
            }),
            "refresh" => f.edit_result(false, |r| {
                r.readback["parts"][0]["view_refresh_requested"] = json!(true)
            }),
            "save-requested" => {
                f.edit_result(false, |r| r.readback["save_requested"] = json!(true))
            }
            "wrong-owner" => f.edit_result(true, |r| {
                r.readback["parts"][0]["after_save"]["part_tag"] = json!(999)
            }),
            _ => unreachable!(),
        }
        assert!(
            drafting::compare_saved_reopened(&f.saved, &f.reopened).is_err(),
            "{fault}"
        );
    }
}
