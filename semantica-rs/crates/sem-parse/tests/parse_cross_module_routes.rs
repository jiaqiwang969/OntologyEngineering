//! Characterisation tests for the routes that cross modules (documents to the HTML/DOCX
//! parsers, convenience functions to the web/XML/image/code/email parsers), against the
//! original (fixtures/core, section "routes").

mod common;

use common::{assert_case, files_dir, load_oracle, oracle_form, quiet_progress};
use once_cell::sync::Lazy;
use sem_parse::methods as m;
use sem_parse::structured_data_parser::StructuredInput;
use sem_parse::{DocumentParser, Options, StructuredDataParser};
use serde_json::{Map, Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| load_oracle("core"));

fn f(name: &str) -> String {
    files_dir("core").join(name).display().to_string()
}

fn check(key: &str, actual: Value) {
    assert_case(&ORACLE, "routes", key, actual);
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn test_document_parser_html_and_docx_routes() {
    quiet_progress();
    let p = DocumentParser::default();
    let none = Options::new();
    check("document_html", oracle_form("core", p.parse_document(f("page.html"), None, &none)));
    check("document_html_text", oracle_form("core", p.extract_text(f("page.html"), &none)));
    check(
        "document_html_text_raw",
        oracle_form("core", p.extract_text(f("page.html"), &opts(json!({"clean": false})))),
    );
    check("document_html_metadata", oracle_form("core", p.extract_metadata(f("page.html"))));
    check("document_docx", oracle_form("core", p.parse_document(f("route.docx"), None, &none)));
    check("document_docx_text", oracle_form("core", p.extract_text(f("route.docx"), &none)));
    check("document_docx_metadata", oracle_form("core", p.extract_metadata(f("route.docx"))));
}

#[test]
fn test_convenience_function_routes() {
    quiet_progress();
    let none = Options::new();
    check(
        "web_html",
        oracle_form("core", m::parse_web_content("<p>x <a href='y'>z</a></p>", None, Some("http://h/"), None, &none)),
    );
    check("web_xml", oracle_form("core", m::parse_web_content(f("route.xml"), Some("xml"), None, None, &none)));
    check("web_bad_type", oracle_form("core", m::parse_web_content("<p/>", Some("json"), None, None, &none)));
    check("xml_via_structured", oracle_form("core", m::parse_xml(f("route.xml"), None, &none)));
    check("structured_xml", oracle_form("core", m::parse_structured_data(f("route.xml"), None, None, &none)));
    check("image_via_media", oracle_form("core", m::parse_image(f("tiny.png"), None, &none)));
    check("media_attr", oracle_form("core", m::parse_media(f("tiny.png"), None, None, &none)));
    check("code", oracle_form("core", m::parse_code(f("script.py"), None, None, &none)));
    check("email", oracle_form("core", m::parse_email(std::path::PathBuf::from(f("mail.eml")), None, &none)));
    check("email_text", oracle_form("core", m::parse_email("Subject: S\n\nhi\n", None, &none)));
    let p = StructuredDataParser::default();
    check(
        "convert_xml_json",
        oracle_form("core", p.convert_data(StructuredInput::from(f("route.xml")), "xml", "json")),
    );
    let _ = Map::<String, Value>::new();
}
