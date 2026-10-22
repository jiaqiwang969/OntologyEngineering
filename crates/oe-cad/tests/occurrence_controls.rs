//! Frozen source-derived software controls. No fixture is native evidence.
#[path = "support/occurrence_fixture.rs"]
mod fixture;
use fixture::*;
use oe_cad::evidence::correspondence::occurrence;
use oe_contracts::CadOccurrenceBindingRequest1;
use serde_json::{Value, json};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture() -> Fixture {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = oe_core::SkillRoot::new(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap(),
    )
    .unwrap();
    let path = root.path().join("var/maintenance/rust-tests").join(format!(
        "occurrence-controls-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    Fixture::in_root(root, &path)
}
fn nested(f: &Fixture) -> CadOccurrenceBindingRequest1 {
    endpoint(
        f,
        "nested",
        "a1",
        observation("before"),
        nested_graph(),
        true,
    )
}
fn row(report: &Value) -> &Value {
    &report["bindings"][0]
}
fn admitted(f: &Fixture, r: &CadOccurrenceBindingRequest1) -> bool {
    let c = occurrence::verify(&f.root, r).unwrap();
    c.require_current_feature(
        "use:left",
        &r.assembly.object,
        c.report()["assembly"]["native_result_sha256"]
            .as_str()
            .unwrap(),
        c.report()["definition"]["native_result_sha256"]
            .as_str()
            .unwrap(),
    )
    .is_ok()
}
#[test]
fn nested_mixed_units_are_received_by_the_actual_occurrence_owner() {
    let f = fixture();
    let r = nested(&f);
    let checked = occurrence::verify(&f.root, &r).unwrap();
    let b = row(checked.report());
    println!("nested-current-owner={}", b);
    assert_eq!(b["status"], "bounded_instance_use");
    for (got, wanted) in b["derived_circle_root_m"]["center"]
        .as_array()
        .unwrap()
        .iter()
        .zip([0.1, 0.0054, 0.01])
    {
        assert!((got.as_f64().unwrap() - wanted).abs() < 1e-12);
    }
    assert!((b["derived_circle_root_m"]["radius"].as_f64().unwrap() - 0.005).abs() < 1e-12);
    assert_eq!(
        b["derived_circle_root_m"]["plane_normal"],
        json!([0., 0., 1.])
    );
    assert_eq!(b["parent_chain"].as_array().unwrap().len(), 2);
    assert_eq!(b["occurrence_path"], json!(["SUB", "PLATE"]));
    assert_eq!(b["native_occurrence_edge_observed"], false);
    assert_eq!(checked.report()["engineering_adoption"], "not_established");
    assert!(admitted(&f, &r));
}
#[test]
fn parent_unit_and_composition_disagreement_cannot_reach_a_capability() {
    for kind in ["unit", "composed"] {
        let f = fixture();
        let mut r = nested(&f);
        edit_assembly(&f, &mut r, |g| {
            if kind == "unit" {
                g["occurrences"][1]["raw_parent_origin"][0] = json!(25.4)
            } else {
                g["occurrences"][1]["local_to_root_m"][7] = json!(0.04)
            }
        });
        let failure = occurrence::verify(&f.root, &r).unwrap_err().to_string();
        assert!(
            failure.contains(if kind == "unit" {
                "assembly_native_parent_frame_differs"
            } else {
                "assembly_nested_transform_composition_differs"
            }),
            "{kind}: {failure}"
        );
    }
}
#[test]
fn missing_and_same_parent_ambiguous_paths_stay_unresolved() {
    for kind in ["missing", "duplicate"] {
        let f = fixture();
        let mut r = nested(&f);
        if kind == "missing" {
            edit_intents(&f, &mut r, |v| {
                v["features"]["left_before"]["occurrence"]["occurrence_path"] =
                    json!(["SUB", "MISSING"])
            });
        } else {
            edit_assembly(&f, &mut r, |g| {
                let mut duplicate = g["occurrences"][1].clone();
                duplicate["occurrence_id"] = json!("root/0/1");
                duplicate["part_occurrence_tag"] = json!(22);
                duplicate["instance_tag"] = json!(122);
                g["occurrences"].as_array_mut().unwrap().push(duplicate);
            });
        }
        let c = occurrence::verify(&f.root, &r).unwrap();
        assert_eq!(row(c.report())["status"], "unresolved");
        assert_eq!(
            row(c.report())["reason"],
            "motion_occurrence_path_missing_or_ambiguous"
        );
        assert!(!admitted(&f, &r));
    }
}
#[test]
fn identical_child_names_in_other_parent_paths_do_not_create_false_ambiguity() {
    let f = fixture();
    let mut r = nested(&f);
    edit_assembly(&f, &mut r, |g| {
        let mut parent = g["occurrences"][0].clone();
        parent["occurrence_id"] = json!("root/1");
        parent["name"] = json!("OTHER");
        parent["part_occurrence_tag"] = json!(22);
        parent["instance_tag"] = json!(122);
        for key in ["local_to_parent_m", "local_to_root_m"] {
            parent[key][3] = json!(0.2);
        }
        for key in ["raw_parent_transform", "raw_root_transform"] {
            parent[key][3] = json!(200.);
        }
        parent["raw_parent_origin"][0] = json!(200.);
        let mut child = g["occurrences"][1].clone();
        child["occurrence_id"] = json!("root/1/0");
        child["parent_occurrence_id"] = json!("root/1");
        child["part_occurrence_tag"] = json!(23);
        child["instance_tag"] = json!(123);
        child["local_to_root_m"][3] = json!(0.2);
        child["raw_root_transform"][3] = json!(200.);
        g["occurrences"]
            .as_array_mut()
            .unwrap()
            .extend([parent, child]);
    });
    assert!(admitted(&f, &r));
}
#[test]
fn ambiguous_missing_and_partial_definition_geometry_keep_original_diagnostics() {
    for name in ["ambiguous", "deleted", "partial"] {
        let f = fixture();
        let r = endpoint(&f, name, "a1", observation(name), nested_graph(), true);
        let c = occurrence::verify(&f.root, &r).unwrap();
        assert_eq!(
            row(c.report())["status"],
            "unresolved",
            "{name}: {}",
            c.report()
        );
        assert_eq!(
            row(c.report())["reason"],
            "cad_occurrence_definition_geometry_unqualified"
        );
        assert_ne!(row(c.report())["definition_check"]["conclusion"], "pass");
        assert!(!admitted(&f, &r));
    }
}
#[test]
fn unreceived_refsets_reflection_inventory_and_units_do_not_gain_authority() {
    for index in [0usize, 1] {
        let f = fixture();
        let mut r = nested(&f);
        edit_assembly(&f, &mut r, |g| {
            g["occurrences"][index]["reference_set"] = json!("MODEL")
        });
        let c = occurrence::verify(&f.root, &r).unwrap();
        assert_eq!(row(c.report())["status"], "unresolved");
        assert_eq!(
            row(c.report())["reason"],
            "cad_occurrence_reference_set_not_received"
        );
    }
    let f = fixture();
    let mut r = nested(&f);
    edit_assembly(&f, &mut r, |g| g["complete"] = json!(false));
    assert!(
        occurrence::verify(&f.root, &r)
            .unwrap_err()
            .to_string()
            .contains("assembly_observation_scope_incomplete")
    );
    let f = fixture();
    let mut r = nested(&f);
    edit_assembly(&f, &mut r, |g| {
        let p = &mut g["prototypes"][2];
        p["native_units"] = json!("inch");
        p["unit_to_m"] = json!(0.0254);
        let native = p["solids"][0]["exact_box_corners_native"]
            .as_array()
            .unwrap()
            .clone();
        p["solids"][0]["exact_box_corners_m"] = json!(
            native
                .iter()
                .map(|v| v.as_f64().unwrap() * 0.0254)
                .collect::<Vec<_>>()
        );
    });
    let c = occurrence::verify(&f.root, &r).unwrap();
    assert_eq!(
        row(c.report())["reason"],
        "cad_occurrence_prototype_units_differ"
    );
    let f = fixture();
    let mut r = endpoint(
        &f,
        "reflect",
        "a1",
        observation("before"),
        direct_graph(0., 0.04),
        false,
    );
    edit_assembly(&f, &mut r, |g| {
        for key in [
            "local_to_parent_m",
            "local_to_root_m",
            "raw_parent_transform",
            "raw_root_transform",
        ] {
            g["occurrences"][0][key][0] = json!(-1.);
        }
        g["occurrences"][0]["raw_parent_csys"][0] = json!(-1.);
    });
    edit_intents(&f, &mut r, |v| {
        v["features"]["left_before"]["occurrence"]["local_to_root_m"][0] = json!(-1.)
    });
    let c = occurrence::verify(&f.root, &r).unwrap();
    assert_eq!(row(c.report())["status"], "unresolved");
    assert!(
        row(c.report())["reason"]
            .as_str()
            .unwrap()
            .contains("reflection")
    );
    assert!(!admitted(&f, &r));
}
#[test]
fn original_source_and_receipt_bytes_cannot_be_changed_under_stale_identity() {
    for kind in ["project", "intent", "job", "input", "params", "native"] {
        let f = fixture();
        let r = nested(&f);
        assert!(admitted(&f, &r));
        let directory = assembly_directory(&f, &r);
        let path = match kind {
            "project" => f.root.path().join(&r.definition.project.path),
            "intent" => f
                .root
                .path()
                .join(&r.definition.bindings[0].intent.source.path),
            "job" => directory.join("job.json"),
            "input" => directory.join("inputs/plate.prt"),
            "params" => directory.join("params.json"),
            "native" => directory.join("native-result.json"),
            _ => unreachable!(),
        };
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b'X');
        fs::write(&path, bytes).unwrap();
        assert!(
            occurrence::verify(&f.root, &r).is_err(),
            "stale {kind} was accepted"
        );
    }
}
#[test]
fn public_nonempty_profile_and_current_observation_capability_guards_are_preserved() {
    let f = fixture();
    let mut r = nested(&f);
    let c = occurrence::verify(&f.root, &r).unwrap();
    let assembly = c.report()["assembly"]["native_result_sha256"]
        .as_str()
        .unwrap();
    let definition = c.report()["definition"]["native_result_sha256"]
        .as_str()
        .unwrap();
    let mut wrong = r.assembly.object.clone();
    wrong.source_sha256 = "0".repeat(64);
    assert!(
        c.require_current_feature("use:left", &wrong, assembly, definition)
            .is_err()
    );
    assert!(
        c.require_current_feature("use:left", &r.assembly.object, &"0".repeat(64), definition)
            .is_err()
    );
    assert!(
        c.require_current_feature("use:left", &r.assembly.object, assembly, &"0".repeat(64))
            .is_err()
    );
    assert!(serde_json::from_value::<CadOccurrenceBindingRequest1>(c.report().clone()).is_err());
    r.definition.bindings.clear();
    assert!(occurrence::verify(&f.root, &r).is_err());
}
