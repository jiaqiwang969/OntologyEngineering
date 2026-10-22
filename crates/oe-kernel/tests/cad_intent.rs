//! Frozen counterexample: current identity alone accepted 290mm for a 158mm
//! plate. Parameters and scope must bind before it is a current design intent.
use oe_contracts::{CenteredHoleRow, ObjectIdentity};
use oe_kernel::{cad_intent, engineering, project::Project};
use serde_json::{Value, json};
fn fixture() -> (Project, CenteredHoleRow, Value) {
    let row:CenteredHoleRow=serde_json::from_value(json!({"schema":"oev2.centered-hole-row/1","row_id":"periodic","object":{"object_id":"plate","revision":"b","condition_scope":"design","source_sha256":"a".repeat(64)},
        "local_frame":"plate-local","unit":"mm","length":"158","pitch":"32","minimum_center_to_end":"15","center":"0","diameter":"4","source_refs":["original"]})).unwrap();
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"plate","object_revision":"b","purpose":"bounded variant method","acceptance":["retain mating and drawing dependencies"],
        "facts":[{"property":"cad_row:periodic","value":row,"status":"declared","source_refs":["original"]}],
        "assertions":[{"assertion_id":"row-content","kind":"cad_row_intent","subject":"periodic","requirements":row,"source_refs":["original"],"affected_decisions":["layout"]}]});
    let decisions=["layout","drawings","inspection"].map(|id|json!({"id":id,"object_id":"plate","object_revision":"b","condition_scope":"design","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["original"]}));
    let p:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"cad-intent-counterexample","revision":"b","purpose":basis["purpose"],"objects":{"plate":"b"},"conditions":{"design":serde_json::to_string(&basis).unwrap()},"evidence":{"original":"a".repeat(64)},
        "decisions":decisions,
        "dependencies":{"drawings":["layout"],"inspection":["drawings"]}})).unwrap();
    (p, row, basis)
}
fn save(p: &mut Project, b: &Value) {
    p.conditions
        .insert("design".into(), serde_json::to_string(b).unwrap());
}
#[test]
fn same_identity_different_length_is_not_current_intent_and_reopens_outputs() {
    let (mut p, row, mut b) = fixture();
    assert_eq!(cad_intent::check(&p, &row).conclusion, "pass");
    b["assertions"][0]["requirements"]["length"] = json!("290");
    save(&mut p, &b);
    let c = engineering::checks(&p, &row.object).pop().unwrap();
    assert_eq!(c.conclusion, "violated");
    assert_eq!(c.result["details"]["differences"], json!(["length"]));
    assert_eq!(
        c.result["details"]["affected_decisions"],
        json!(["drawings", "inspection", "layout"])
    );
    assert_eq!(c.result["engineering_adoption"], "not_established");
}
#[test]
fn missing_or_assumed_typed_row_preserves_math_but_blocks_current_use() {
    let (mut p, row, mut b) = fixture();
    b["facts"][0] = json!({"property":"plate_length","value":{"value":158.0,"unit":"mm"},"status":"declared","source_refs":["original"]});
    save(&mut p, &b);
    let c = cad_intent::check(&p, &row);
    assert_eq!(c.conclusion, "unknown");
    assert_eq!(c.failures, ["cad_variant_declared_row_missing"]);
    assert_eq!(
        c.result["details"]["legacy_plate_length_clue"]["value"]["value"],
        158.0
    );
    assert!(oe_cad::variant::plan(&row).is_ok());
    let (mut p, row, mut b) = fixture();
    b["facts"][0]["status"] = json!("assumed");
    save(&mut p, &b);
    assert_eq!(
        cad_intent::check(&p, &row).failures,
        ["cad_variant_declared_row_unknown"]
    );
    p.objects.insert("plate".into(), "c".into());
    assert_eq!(cad_intent::check(&p, &row).conclusion, "violated");
}
#[test]
fn equivalent_decimal_spellings_pass_but_frame_and_source_changes_do_not() {
    let (p, mut row, _) = fixture();
    row.length = "1.58e2".into();
    row.center = "0.000".into();
    assert_eq!(cad_intent::check(&p, &row).conclusion, "pass");
    row.local_frame = "machine-frame".into();
    assert_eq!(cad_intent::check(&p, &row).conclusion, "violated");
    row.source_refs.push("unregistered".into());
    assert_eq!(
        cad_intent::check(&p, &row).failures,
        ["cad_variant_source_unbound"]
    );
    let _object: &ObjectIdentity = &row.object;
}
