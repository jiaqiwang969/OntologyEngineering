//! Request gates only. Real original-page rendering is an explicit private
//! development invocation, never repeated by this ordinary test target.
use oe_core::SkillRoot;
use oe_knowledge::{
    page_content4::{parse_request, prepare_content4},
    poppler::PopplerTools,
};
use serde_json::{Value, json};

fn request() -> Value {
    let asset = |path| json!({"path":path,"sha256":"0".repeat(64),"bytes":1});
    json!({"schema":"oev2.knowledge-page-content-request/4",
        "registration":asset("sources/.indexes/local-pdf/registration.json"),
        "index":asset("sources/.indexes/local-pdf/catalog.sqlite3"),
        "page_id":"local_pdf:interchangeability:page:188",
        "document_sha256":"baf6363d11afb072bdb960c9a735821393771f213e7495f981ee97c3d9732259",
        "text_sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "raw_text_sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "physical_page":188,"dpi":144,"renderer":asset("/nonexistent/selected-renderer")})
}

#[test]
fn original_selected_native_and_actual_image_domains_remain_separate() {
    let original = request();
    let parsed = parse_request(&original).unwrap();
    assert_eq!(parsed.text_sha256, oe_core::sha256(b""));
    assert_eq!(parsed.raw_text_sha256, oe_core::sha256(b""));
    // These are deliberately forged request additions, not actual image/native
    // observations. They must never silently authorize an execution or relabel
    // the registered native channel.
    for key in [
        "producer_receipt",
        "image_content",
        "navigation_only",
        "ocr",
        "engineering_adoption",
    ] {
        let mut forged = original.clone();
        forged[key] = json!("verified");
        assert_eq!(
            parse_request(&forged).unwrap_err().to_string(),
            "page_content4_request_keys"
        );
    }
    let mut forged = original.clone();
    forged["renderer"]["version"] = json!("trusted");
    assert_eq!(
        parse_request(&forged).unwrap_err().to_string(),
        "page_content4_asset_keys"
    );
}

#[test]
fn wrong_render_domain_denied_before_any_process() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::write(temporary.path().join("SKILL.md"), "private fixture").unwrap();
    let root = SkillRoot::new(temporary.path().canonicalize().unwrap()).unwrap();
    for (key, value) in [("dpi", json!(72)), ("physical_page", json!(0))] {
        let mut input = request();
        input[key] = value;
        let parsed = parse_request(&input).unwrap();
        let failure = prepare_content4(&root, &parsed, &PopplerTools::default(), "never-rendered")
            .unwrap_err();
        assert_eq!(failure.to_string(), "page_content4_request_domain");
    }
    assert!(!temporary.path().join("var").exists());
}

#[test]
fn alternate_registered_paths_denied_before_source_or_process() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::write(temporary.path().join("SKILL.md"), "private fixture").unwrap();
    let root = SkillRoot::new(temporary.path().canonicalize().unwrap()).unwrap();
    for key in ["registration", "index"] {
        let mut input = request();
        input[key]["path"] = json!("unselected/sibling");
        let parsed = parse_request(&input).unwrap();
        assert_eq!(
            prepare_content4(&root, &parsed, &PopplerTools::default(), "never-rendered")
                .unwrap_err()
                .to_string(),
            "page_content4_exact_registered_paths"
        );
    }
    assert!(!temporary.path().join("var").exists());
}
