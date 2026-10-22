use oe_cad::drawing::associations::{check_sheet_persistence, compare, compare_sheet_reference};
use oe_nx_worker::geometry::Parameters;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
fn read(p: impl AsRef<std::path::Path>) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
#[test]
#[ignore = "requires original v2 sheet creation and independent reopen jobs"]
fn actual_sheet_reference_persistence_and_semantic_counterexamples() {
    let root = PathBuf::from(
        std::env::var("OE_SHEET_REFERENCE_ORIGINALS").expect("explicit original root required"),
    );
    let created = root.join("originals/create/job");
    let reopened = root.join("originals/reopen/job");
    assert_eq!(
        compare(&created, &reopened).unwrap_err().to_string(),
        "drawing_reopen_action_mismatch"
    );
    let result = compare_sheet_reference(&created, &reopened).unwrap();
    assert_eq!(result["schema"], "oev2.sheet-reference-reopen/1");
    assert_eq!(result["parts"][0]["observation_persistence"], "matches");
    assert_eq!(
        result["parts"][0]["sheet_reference_check"]["conclusion"],
        "pass"
    );
    assert!(compare_sheet_reference(&created, &created).is_err());
    let params: Parameters =
        oe_nx_protocol::decode(&fs::read(created.join("params.json")).unwrap()).unwrap();
    let Parameters::CreateSheetReferenceDimension {
        sheet,
        reference,
        text_origin_drawing,
        part,
        ..
    } = params
    else {
        panic!("unexpected original action")
    };
    let a = read(created.join("native-result.json"));
    let b = read(reopened.join("native-result.json"));
    let states = &a["readback"]["parts"][0];
    let observed = &b["readback"]["parts"][0];
    let tag = a["readback"]["creation"]["dimension_tag"].as_u64().unwrap() as u32;
    let original = fs::read(created.join(format!("inputs/{part}"))).unwrap();
    let saved = fs::read(created.join(format!("out/{part}"))).unwrap();
    let check = |p: &Value| {
        check_sheet_persistence(
            &sheet,
            &reference,
            text_origin_drawing,
            &original,
            &saved,
            &states["before"],
            &states["after_save"],
            p,
            tag,
        )
    };
    assert!(check(observed).is_ok());
    for fault in [
        "value",
        "retained",
        "view",
        "parent",
        "duplicate",
        "refresh",
        "geometry",
    ] {
        let mut p = observed.clone();
        let d = &mut p["annotations"]["dimensions"][0];
        match fault {
            "value" => d["computed_size"]["value"] = json!(999.),
            "retained" => d["retained"] = json!(true),
            "view" => d["associativity_parents"]["entries"][0]["view"]["tag"] = json!(1),
            "parent" => d["associativity_parents"]["entries"][0]["object_1"]["tag"] = json!(1),
            "duplicate" => {
                let mut other = d.clone();
                other["dimension_tag"] = json!(999999);
                p["annotations"]["dimensions"]
                    .as_array_mut()
                    .unwrap()
                    .push(other);
            }
            "refresh" => p["view_refresh_requested"] = json!(true),
            "geometry" => p["model_edges"]["entries"] = json!([]),
            _ => unreachable!(),
        }
        assert!(check(&p).is_err(), "{fault}");
    }
    assert!(
        check_sheet_persistence(
            &sheet,
            &reference,
            text_origin_drawing,
            b"wrong original",
            &saved,
            &states["before"],
            &states["after_save"],
            observed,
            tag
        )
        .is_err()
    );
}
