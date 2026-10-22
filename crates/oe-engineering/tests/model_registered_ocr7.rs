//! Finite pre-source parser controls; no synthetic object creates opaque6.
use oe_engineering::model_registered_ocr7 as source;
use serde_json::{Value, json};
fn selected() -> Value {
    let r: Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("OE_OCR7_REQUEST6").expect("frozen real request6 path"))
            .unwrap(),
    )
    .unwrap();
    json!({"schema":source::INPUT_SCHEMA,"native_assistance":{"path":"var/native-input.json","sha256":"a".repeat(64)},"ocr_pages":[{"source_ref":"trust-scope-page24","request":r,"expected_selected_row_sha256":null,"expected_raw_tsv_sha256":null}]})
}
#[test]
fn explicit7_parser_refuses_audit_authority() {
    let original = selected();
    assert!(source::parse_input7(&original).is_ok());
    for key in [
        "producer",
        "audit",
        "passed",
        "execution_authority",
        "model",
        "Project",
        "report",
    ] {
        let mut v = original.clone();
        v[key] = json!(true);
        assert!(source::parse_input7(&v).is_err(), "{key}");
    }
    let mut v = original.clone();
    v["schema"] = json!("oev2.model-page-assistance-input/2");
    assert!(source::parse_input7(&v).is_err());
    let mut v = original.clone();
    v["ocr_pages"][0]["request"]["report"] = json!({"passed":true});
    assert!(source::parse_input7(&v).is_err());
    let mut v = original.clone();
    v["ocr_pages"][0]["expected_raw_tsv_sha256"] = json!("arbitrary-producer");
    assert!(source::parse_input7(&v).is_err());
    let mut v = original.clone();
    v["ocr_pages"] = json!([]);
    assert!(source::parse_input7(&v).is_err());
    let mut v = original.clone();
    v["ocr_pages"]
        .as_array_mut()
        .unwrap()
        .push(original["ocr_pages"][0].clone());
    assert!(source::parse_input7(&v).is_err());
}
