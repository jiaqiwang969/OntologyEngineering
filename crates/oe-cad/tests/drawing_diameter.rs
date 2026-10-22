#[path = "support/diameter_fixture.rs"]
mod fixture;
use serde_json::{Value, json};
fn inputs() -> (oe_contracts::SheetDiameterIntent, Value, Value, Value) {
    let (i, b, mut a) = fixture::diameter_fixture();
    let e = &mut a["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0];
    e["association_position_valid"] = json!(true);
    e["association_position"] = json!([0, 0, 0]);
    for role in ["object_1", "view"] {
        e[role]["geometry"] = json!({"called":false});
    }
    let mut reopened = a.clone();
    reopened["view_refresh_requested"] = json!(false);
    reopened["annotations"]["dimensions"][0]["dimension_tag"] = json!(999);
    reopened["model_edges"]["entries"][0]["edge_tag"] = json!(501);
    reopened["model_edges"]["bodies"][0]["edge_tags"][0] = json!(501);
    reopened["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]["object_1"]["tag"] =
        json!(501);
    (i, b, a, reopened)
}
#[test]
fn diameter_reopen_reselects_circle_and_dimension_without_reusing_tags() {
    let (i, b, a, r) = inputs();
    let v = oe_cad::drawing::diameter::check_persistence(
        &i,
        b"independent-fixture-cad",
        b"saved-cad",
        &b,
        &a,
        &r,
        200,
    )
    .unwrap();
    assert_eq!(v["reopened"]["dimension_tag"], 999);
    assert_eq!(v["engineering_adoption"], "not_established");
}
#[test]
fn diameter_reopen_rejects_wrong_parent_refresh_source_and_extra_dimensions() {
    let (i, b, a, r) = inputs();
    for (pointer, value) in [
        ("/view_refresh_requested", json!(true)),
        ("/annotations/dimensions/0/computed_size/value", json!(6)),
        (
            "/annotations/dimensions/0/associativity_parents/entries/0/object_1/tag",
            json!(102),
        ),
    ] {
        let mut bad = r.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            oe_cad::drawing::diameter::check_persistence(
                &i,
                b"independent-fixture-cad",
                b"saved-cad",
                &b,
                &a,
                &bad,
                200
            )
            .is_err(),
            "{pointer}"
        );
    }
    assert!(
        oe_cad::drawing::diameter::check_persistence(
            &i,
            b"wrong original",
            b"saved-cad",
            &b,
            &a,
            &r,
            200
        )
        .is_err()
    );
    let mut extra = r.clone();
    let mut d = r["annotations"]["dimensions"][0].clone();
    d["dimension_tag"] = json!(998);
    extra["annotations"]["dimensions"]
        .as_array_mut()
        .unwrap()
        .push(d);
    assert!(
        oe_cad::drawing::diameter::check_persistence(
            &i,
            b"independent-fixture-cad",
            b"saved-cad",
            &b,
            &a,
            &extra,
            200
        )
        .is_err()
    );
}

#[test]
fn whole_circle_reopen_preserves_raw_type_and_independent_absolute_center() {
    let (mut i, b, mut a, mut r) = inputs();
    i.schema = "oev2.sheet-diameter-intent/2".into();
    for part in [&mut a, &mut r] {
        let e = &mut part["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0];
        e["point_type_raw"] = json!(0);
        e["coordinate_basis"] = json!("UF_DRF assoc_point absolute coordinates");
    }
    let check = |intent: &oe_contracts::SheetDiameterIntent, part: &Value| {
        oe_cad::drawing::diameter::check_persistence(
            intent,
            b"independent-fixture-cad",
            b"saved-cad",
            &b,
            &a,
            part,
            200,
        )
    };
    assert_eq!(check(&i, &r).unwrap()["conclusion"], "pass");
    let mut old = i.clone();
    old.schema = "oev2.sheet-diameter-intent/1".into();
    assert!(check(&old, &r).is_err());
    r["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]["association_position"] =
        json!([0, 0, 32]);
    assert!(check(&i, &r).is_err());
}
