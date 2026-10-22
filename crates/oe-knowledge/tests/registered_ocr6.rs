use oe_core::SkillRoot;
use oe_knowledge::{ocr_page5, poppler::PopplerTools, registered_ocr6};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn request() -> Value {
    let asset = |p: &str| json!({"path":p,"sha256":"a".repeat(64),"bytes":1});
    json!({"schema":"oev2.knowledge-registered-page-ocr-request/6","document_id":"vol2","page_count":353,
 "page":{"schema":"oev2.knowledge-page-content-request/4","registration":asset("sources/.indexes/local-pdf/registration.json"),
 "index":asset("sources/.indexes/local-pdf/catalog.sqlite3"),"page_id":"local_pdf:vol2:page:24","document_sha256":"a".repeat(64),
 "text_sha256":"b".repeat(64),"raw_text_sha256":"c".repeat(64),"physical_page":24,"dpi":144,"renderer":asset("/selected/pdftoppm")},
 "ocr":{"schema":"oev2.knowledge-ocr-source-request/5","tool":asset("/selected/tesseract"),
 "config":{"path":"/selected/tsv","sha256":"59d079bb75d8b3d7c839a3564580cb559e362c93a9d70f234e421c0c3e767e04","bytes":22},
 "languages":[{"name":"chi_sim","source":asset("/selected/chi_sim.traineddata")},{"name":"eng","source":asset("/selected/eng.traineddata")}]}})
}
#[test]
fn registered_ocr6_explicit_domain_retains_old5_refusal() {
    let r = registered_ocr6::parse_request6(&request()).unwrap();
    let skill_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let root = SkillRoot::new(skill_root).unwrap();
    let tools = PopplerTools::default();
    // No files or process are touched: the original version5 factory refuses
    // this new legal page at its original development-domain admission.
    let e = ocr_page5::prepare_page_and_ocr5(
        &root,
        &r.page,
        &r.ocr,
        &tools,
        Instant::now() + Duration::from_secs(5),
        "unused-refused-domain6",
    )
    .unwrap_err();
    assert!(e.to_string().contains("ocr_page5_development_page_domain"));
    let e = registered_ocr6::prepare_registered_page_ocr6(
        &root,
        &r,
        &tools,
        Instant::now(),
        "unused-expired6",
    )
    .unwrap_err();
    assert!(e.to_string().contains("registered_ocr6_deadline_expired"));
}
#[test]
fn registered_ocr6_refuses_fabricated_scope_authority_and_extra_nested_inputs() {
    let valid = request();
    assert!(registered_ocr6::parse_request6(&valid).is_ok());
    for (k, v) in [
        ("schema", json!("oev2.knowledge-ocr-source-request/5")),
        ("document_id", json!("interchangeability")),
        ("page_count", json!(23)),
        ("producer_receipt", json!({})),
        ("model", json!("run")),
        ("timeout_seconds", json!(240)),
        ("source_pdf", json!("/arbitrary.pdf")),
    ] {
        let mut changed = valid.clone();
        changed[k] = v;
        assert!(registered_ocr6::parse_request6(&changed).is_err(), "{k}");
    }
    for path in [
        ["page", "producer_verified"],
        ["ocr", "process_observation"],
        ["page", "source_pdf"],
    ] {
        let mut changed = valid.clone();
        changed[path[0]][path[1]] = json!(true);
        assert!(registered_ocr6::parse_request6(&changed).is_err());
    }
    let mut changed = valid.clone();
    changed["page"]["registration"]["path"] = json!("/outside/registration.json");
    assert!(registered_ocr6::parse_request6(&changed).is_err());
    let mut changed = valid.clone();
    changed["ocr"]["languages"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(registered_ocr6::parse_request6(&changed).is_err());
    let mut changed = valid.clone();
    changed["page"]["physical_page"] = json!(354);
    changed["page"]["page_id"] = json!("local_pdf:vol2:page:354");
    assert!(registered_ocr6::parse_request6(&changed).is_err());
}
