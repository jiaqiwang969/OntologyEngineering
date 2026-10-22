//! Source-derived synthetic cross-version controls; no real NX invocation.
#[path = "support/occurrence_fixture.rs"]
mod fixture;
use fixture::definition::change;
use fixture::*;
use oe_cad::evidence::correspondence::{instances, occurrence};
use oe_contracts::{CadInstanceCorrespondenceRequest2, CadOccurrenceBindingRequest1};
use serde_json::{Value, json};

fn endpoint_pair(f: &Fixture) -> (CadOccurrenceBindingRequest1, CadOccurrenceBindingRequest1) {
    (
        endpoint(
            f,
            "before",
            "a1",
            observation("before"),
            direct_graph(0., 0.04),
            false,
        ),
        endpoint(
            f,
            "after",
            "a2",
            observation("before"),
            direct_graph(0.01, 0.04),
            false,
        ),
    )
}
fn request(
    f: &Fixture,
    previous: CadOccurrenceBindingRequest1,
    mut current: CadOccurrenceBindingRequest1,
    changes: Vec<Value>,
) -> CadInstanceCorrespondenceRequest2 {
    let original = f.request(
        previous.definition.clone(),
        current.definition.clone(),
        changes,
    );
    current.definition = original.current;
    refresh_project(f, &mut current, "cross-current-project");
    CadInstanceCorrespondenceRequest2 {
        schema: "oev2.cad-correspondence/2".into(),
        previous,
        current,
        changes: original.changes,
    }
}
fn retained(
    f: &Fixture,
    previous: CadOccurrenceBindingRequest1,
    current: CadOccurrenceBindingRequest1,
) -> CadInstanceCorrespondenceRequest2 {
    let declarations = (0..previous.definition.bindings.len())
        .map(|i| {
            change(
                &format!("retained-{i}"),
                "retained",
                &previous.definition,
                &current.definition,
                &[i],
                &[i],
            )
        })
        .collect();
    request(f, previous, current, declarations)
}
fn current_capability(
    c: &instances::VerifiedInstanceCorrespondence,
    r: &CadInstanceCorrespondenceRequest2,
    id: &str,
) -> anyhow::Result<Value> {
    c.require_current_feature(
        id,
        &r.current.assembly.object,
        c.report()["current"]["assembly"]["native_result_sha256"]
            .as_str()
            .unwrap(),
        c.report()["current"]["definition"]["native_result_sha256"]
            .as_str()
            .unwrap(),
    )
}
#[test]
fn identical_instance_endpoints_keep_no_delta_and_the_old_contract_unresolved() {
    let f = Fixture::new();
    let before = endpoint(
        &f,
        "same",
        "a1",
        observation("before"),
        direct_graph(0., 0.04),
        false,
    );
    let r = retained(&f, before.clone(), before);
    let c = instances::verify(&f.root, &r).unwrap();
    assert!(c.impact_seed_ids().is_empty());
    for row in c.report()["changes"].as_array().unwrap() {
        assert_eq!(
            row["correspondence_status"],
            "bounded_declared_instance_correspondence"
        );
        assert_eq!(row["review"], "no_correspondence_delta");
        assert_eq!(row["evidence_rebinding_required"], false);
        assert_eq!(row["observed_instance_placement_changed"], false);
    }
    assert!(current_capability(&c, &r, "use:left").is_ok());
    assert!(current_capability(&c, &r, "use:right").is_ok());
    for row in c.report()["legacy_definition_contract"]["changes"]
        .as_array()
        .unwrap()
    {
        assert_eq!(row["correspondence_status"], "unresolved");
    }
}
#[test]
fn one_instance_movement_does_not_invent_a_second_design_change() {
    let f = Fixture::new();
    let (before, after) = endpoint_pair(&f);
    assert_eq!(before.definition.object, after.definition.object);
    let r = retained(&f, before, after);
    let c = instances::verify(&f.root, &r).unwrap();
    let rows = c.report()["changes"].as_array().unwrap();
    assert_eq!(rows[0]["declared_design_changes"], json!(["occurrence"]));
    assert_eq!(rows[1]["declared_design_changes"], json!([]));
    assert_eq!(rows[0]["observed_instance_placement_changed"], true);
    assert_eq!(rows[1]["observed_instance_placement_changed"], false);
    assert_eq!(rows[0]["observed_root_geometry_changed"], true);
    assert_eq!(rows[1]["observed_root_geometry_changed"], false);
    for row in rows {
        assert_eq!(
            row["correspondence_status"],
            "bounded_declared_instance_correspondence"
        );
        assert_eq!(row["evidence_rebinding_required"], true);
        assert_eq!(row["review"], "required");
    }
    let left = current_capability(&c, &r, "use:left").unwrap();
    let right = current_capability(&c, &r, "use:right").unwrap();
    assert!((left["derived_circle_root_m"]["center"][0].as_f64().unwrap() + 0.01).abs() < 1e-12);
    assert!(
        (right["derived_circle_root_m"]["center"][0]
            .as_f64()
            .unwrap()
            - 0.02)
            .abs()
            < 1e-12
    );
    assert_eq!(c.report()["engineering_adoption"], "not_established");
}
#[test]
fn swapping_two_valid_endpoint_uses_cannot_exchange_retained_engineering_identities() {
    let f = Fixture::new();
    let (before, mut after) = endpoint_pair(&f);
    edit_intents(&f, &mut after, |v| {
        v["features"]["left_before"]["occurrence"] = json!({"occurrence_path":["B"],"prototype_id":"plate.prt","local_to_root_m":pose(0.04,0.,0.)});
        v["features"]["right_before"]["occurrence"] = json!({"occurrence_path":["A"],"prototype_id":"plate.prt","local_to_root_m":pose(0.01,0.,0.)});
    });
    let endpoint = occurrence::verify(&f.root, &after).unwrap();
    assert!(
        endpoint.report()["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["status"] == "bounded_instance_use")
    );
    let r = retained(&f, before, after);
    let c = instances::verify(&f.root, &r).unwrap();
    for row in c.report()["changes"].as_array().unwrap() {
        assert_eq!(row["correspondence_status"], "unresolved");
        assert_eq!(
            row["reason"],
            "cad_instance_correspondence_path_change_requires_lineage"
        );
        assert!(row["observed_instance_placement_changed"].is_null());
        assert!(row["observed_root_geometry_changed"].is_null());
    }
    assert!(current_capability(&c, &r, "use:left").is_err());
    assert!(current_capability(&c, &r, "use:right").is_err());
}
fn with_copy(
    f: &Fixture,
    mut after: CadOccurrenceBindingRequest1,
    reuse_identity: bool,
) -> CadOccurrenceBindingRequest1 {
    edit_assembly(f, &mut after, |g| {
        let mut c = g["occurrences"][1].clone();
        c["occurrence_id"] = json!("root/2");
        c["name"] = json!("C");
        c["part_occurrence_tag"] = json!(22);
        c["instance_tag"] = json!(122);
        for key in ["local_to_parent_m", "local_to_root_m"] {
            c[key][3] = json!(0.08);
        }
        for key in ["raw_parent_transform", "raw_root_transform"] {
            c[key][3] = json!(80.);
        }
        c["raw_parent_origin"][0] = json!(80.);
        g["occurrences"].as_array_mut().unwrap().push(c);
    });
    edit_intents(f, &mut after, |v| {
        let mut c = v["features"]["left_before"].clone();
        c["binding_id"] = json!("copy");
        c["selector"]["feature_id"] = json!(if reuse_identity {
            "use:left"
        } else {
            "use:copy"
        });
        c["occurrence"] = json!({"occurrence_path":["C"],"prototype_id":"plate.prt","local_to_root_m":pose(0.08,0.,0.)});
        v["features"]["copy"] = c;
    });
    let source = after.definition.bindings[0].intent.source.clone();
    after.definition.bindings.push(
        serde_json::from_value(
            json!({"binding_id":"copy","intent":{"source":source,"pointer":"/features/copy"}}),
        )
        .unwrap(),
    );
    after
}
#[test]
fn new_and_copied_uses_do_not_inherit_correspondence_or_engineering_qualification() {
    for kind in ["new", "copied"] {
        let f = Fixture::new();
        let (before, after) = endpoint_pair(&f);
        let after = with_copy(&f, after, false);
        let mut changes = vec![
            change(
                "left",
                "retained",
                &before.definition,
                &after.definition,
                &[0],
                &[0],
            ),
            change(
                "right",
                "retained",
                &before.definition,
                &after.definition,
                &[1],
                &[1],
            ),
        ];
        changes.push(change(
            "new-use",
            kind,
            &before.definition,
            &after.definition,
            if kind == "new" { &[] } else { &[0] },
            &[2],
        ));
        let r = request(&f, before, after, changes);
        let c = instances::verify(&f.root, &r).unwrap();
        assert_eq!(
            c.report()["changes"][2]["correspondence_status"],
            "declared_only"
        );
        assert!(current_capability(&c, &r, "use:copy").is_err());
        assert_eq!(
            c.report()["changes"][2]["engineering_adoption"],
            "not_established"
        );
    }
    let f = Fixture::new();
    let (before, after) = endpoint_pair(&f);
    let after = with_copy(&f, after, true);
    let changes = vec![
        change(
            "left",
            "retained",
            &before.definition,
            &after.definition,
            &[0],
            &[0],
        ),
        change(
            "right",
            "retained",
            &before.definition,
            &after.definition,
            &[1],
            &[1],
        ),
        change(
            "copy",
            "copied",
            &before.definition,
            &after.definition,
            &[0],
            &[2],
        ),
    ];
    let r = request(&f, before, after, changes);
    assert!(
        instances::verify(&f.root, &r)
            .unwrap_err()
            .to_string()
            .contains("cad_correspondence_copy_requires_new_feature_identity")
    );
}
#[test]
fn deleted_uses_retain_old_review_obligations_even_for_an_empty_endpoint() {
    for all in [false, true] {
        let f = Fixture::new();
        let (before, mut after) = endpoint_pair(&f);
        edit_assembly(&f, &mut after, |g| {
            let a = g["occurrences"].as_array_mut().unwrap();
            if all {
                a.clear()
            } else {
                a.remove(0);
            }
        });
        if all {
            after.definition.bindings.clear();
        } else {
            after.definition.bindings.remove(0);
        }
        let mut changes = vec![change(
            "delete-left",
            "deleted",
            &before.definition,
            &after.definition,
            &[0],
            &[],
        )];
        changes.push(change(
            "right",
            if all { "deleted" } else { "retained" },
            &before.definition,
            &after.definition,
            &[1],
            if all { &[] } else { &[0] },
        ));
        let r = request(&f, before, after, changes);
        let c = instances::verify(&f.root, &r).unwrap();
        assert_eq!(
            c.report()["changes"][0]["correspondence_status"],
            "declared_only"
        );
        assert!(c.impact_seed_ids().contains("fit:left"));
        assert!(current_capability(&c, &r, "use:left").is_err());
        if all {
            assert!(current_capability(&c, &r, "use:right").is_err());
            assert!(occurrence::verify(&f.root, &r.current).is_err());
        }
    }
}
#[test]
fn unqualified_current_geometry_is_unknown_and_does_not_imply_deletion_or_no_change() {
    for name in ["deleted", "ambiguous", "partial"] {
        let f = Fixture::new();
        let before = endpoint(
            &f,
            "before",
            "a1",
            observation("before"),
            direct_graph(0., 0.04),
            false,
        );
        let after = endpoint(
            &f,
            "after",
            "a2",
            observation(name),
            direct_graph(0., 0.04),
            false,
        );
        let r = retained(&f, before, after);
        let c = instances::verify(&f.root, &r).unwrap();
        let row = &c.report()["changes"][0];
        assert_eq!(row["declared_kind"], "retained");
        assert_eq!(row["correspondence_status"], "unresolved");
        assert!(row["observed_root_geometry_changed"].is_null());
        assert!(row["observed_instance_placement_changed"].is_null());
        assert!(current_capability(&c, &r, "use:left").is_err());
    }
}
#[test]
fn split_merge_and_declared_uncertainty_are_not_promoted_by_unique_endpoint_geometry() {
    for kind in ["split", "merged", "ambiguous", "unresolved"] {
        let f = Fixture::new();
        let (mut before, mut after) = endpoint_pair(&f);
        let declarations = if kind == "split" {
            before.definition.bindings.truncate(1);
            vec![change(
                "topology",
                kind,
                &before.definition,
                &after.definition,
                &[0],
                &[0, 1],
            )]
        } else if kind == "merged" {
            after.definition.bindings.truncate(1);
            vec![change(
                "topology",
                kind,
                &before.definition,
                &after.definition,
                &[0, 1],
                &[0],
            )]
        } else {
            vec![
                change(
                    "left",
                    kind,
                    &before.definition,
                    &after.definition,
                    &[0],
                    &[0],
                ),
                change(
                    "right",
                    kind,
                    &before.definition,
                    &after.definition,
                    &[1],
                    &[1],
                ),
            ]
        };
        let r = request(&f, before, after, declarations);
        let c = instances::verify(&f.root, &r).unwrap();
        for row in c.report()["changes"].as_array().unwrap() {
            assert_eq!(
                row["correspondence_status"],
                if kind == "ambiguous" {
                    "ambiguous"
                } else {
                    "unresolved"
                }
            );
        }
        assert!(current_capability(&c, &r, "use:left").is_err());
    }
}
#[test]
fn original_full_coverage_reuse_and_project_identity_guards_still_execute() {
    for kind in ["coverage", "reused-current", "project"] {
        let f = Fixture::new();
        let (before, after) = endpoint_pair(&f);
        let mut changes = vec![change(
            "left",
            "retained",
            &before.definition,
            &after.definition,
            &[0],
            &[0],
        )];
        if kind != "coverage" {
            changes.push(change(
                "right",
                "retained",
                &before.definition,
                &after.definition,
                &[1],
                if kind == "reused-current" { &[0] } else { &[1] },
            ));
        }
        let mut r = request(&f, before, after, changes);
        if kind == "project" {
            r.current.definition.project_context.project_id = "other".into();
        }
        let error = instances::verify(&f.root, &r).unwrap_err().to_string();
        assert!(
            error.contains(match kind {
                "coverage" => "cad_correspondence_binding_coverage_incomplete",
                "reused-current" => "cad_correspondence_current_binding_reused",
                _ => "cad_instance_correspondence_project_identity_mismatch",
            }),
            "{kind}: {error}"
        );
    }
}
#[test]
fn root_object_and_current_observation_are_required_for_the_new_capability() {
    let f = Fixture::new();
    let (before, after) = endpoint_pair(&f);
    let r = retained(&f, before, after);
    let c = instances::verify(&f.root, &r).unwrap();
    let a = c.report()["current"]["assembly"]["native_result_sha256"]
        .as_str()
        .unwrap();
    let d = c.report()["current"]["definition"]["native_result_sha256"]
        .as_str()
        .unwrap();
    assert!(
        c.require_current_feature("use:left", &r.current.assembly.object, &"0".repeat(64), d)
            .is_err()
    );
    assert!(
        c.require_current_feature("use:left", &r.current.assembly.object, a, &"0".repeat(64))
            .is_err()
    );
    assert!(
        serde_json::from_value::<CadInstanceCorrespondenceRequest2>(c.report().clone()).is_err()
    );
    let f = Fixture::new();
    let (before, mut after) = endpoint_pair(&f);
    after.assembly.object.object_id = "different-root".into();
    let r = retained(&f, before, after);
    let c = instances::verify(&f.root, &r).unwrap();
    for row in c.report()["changes"].as_array().unwrap() {
        assert_eq!(row["correspondence_status"], "unresolved");
        assert_eq!(
            row["reason"],
            "cad_instance_correspondence_engineering_identity_not_retained"
        );
        assert!(row["observed_instance_placement_changed"].is_null());
        assert!(row["observed_root_geometry_changed"].is_null());
    }
}
