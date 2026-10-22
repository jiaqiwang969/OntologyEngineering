//! Characterisation tests for `xml_parser` (XMLParser, XMLData, XMLElement) and its XPath
//! subset against the original (fixtures/markup: 112 documents with both engines, 142 XPath
//! queries, XML files).

mod common;

use std::path::PathBuf;

use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{Value, json};

use sem_parse::{Options, XMLData, XMLElement, XMLParser};
use sem_utils::SemanticaError;

static ORACLE: Lazy<Value> = Lazy::new(|| common::load_oracle("markup"));

fn fixtures() -> PathBuf {
    common::fixture("markup")
}

fn outcome<T: Serialize>(result: Result<T, SemanticaError>) -> Value {
    match result {
        Ok(v) => json!({ "ok": serde_json::to_value(v).expect("serialisable") }),
        Err(e) => common::err_json(&e),
    }
}

fn resolve(v: &Value) -> Value {
    let text = serde_json::to_string(v).unwrap();
    let dir = fixtures().display().to_string();
    serde_json::from_str(&text.replace("@FIXTURES@", &dir)).unwrap()
}

fn engine(e: &str) -> Options {
    let mut o = Options::new();
    o.insert("engine".to_string(), json!(e));
    o
}

#[test]
fn test_xml_documents_match_oracle() {
    common::quiet_progress();
    let p = XMLParser::default();
    let cases = ORACLE["xml_cases"].as_array().unwrap();
    assert!(cases.len() >= 100);
    let mut no_ns = Options::new();
    no_ns.insert("extract_namespaces".to_string(), json!(false));
    for case in cases {
        let s = case["input"].as_str().unwrap();
        for e in ["lxml", "etree"] {
            assert_eq!(outcome(p.parse(s, &engine(e))), resolve(&case[e]), "{e} parse of {s:?}");
        }
        assert_eq!(outcome(p.to_dict(s, &Options::new())), resolve(&case["to_dict"]), "to_dict of {s:?}");
        assert_eq!(
            outcome(p.extract_by_tag(s, "b", &Options::new())),
            resolve(&case["by_tag_b"]),
            "extract_by_tag of {s:?}"
        );
        assert_eq!(outcome(p.parse(s, &no_ns)), resolve(&case["no_ns"]), "no-namespace parse of {s:?}");
    }
}

#[test]
fn test_xpath_queries_match_oracle() {
    common::quiet_progress();
    let p = XMLParser::default();
    let cases = ORACLE["xpath_cases"].as_array().unwrap();
    assert!(cases.len() >= 140);
    for case in cases {
        let doc = case["doc"].as_str().unwrap();
        let xpath = case["xpath"].as_str().unwrap();
        let e = case["engine"].as_str().unwrap();
        assert_eq!(
            outcome(p.find_elements(doc, xpath, &engine(e))),
            resolve(&case["result"]),
            "find_elements({xpath:?}) with {e} on {doc:?}"
        );
    }
}

#[test]
fn test_xml_files_match_oracle() {
    common::quiet_progress();
    let p = XMLParser::default();
    let none = Options::new();
    for (name, rec) in ORACLE["xml_files"].as_object().unwrap() {
        let path = fixtures().join("xml").join(name);
        assert_eq!(outcome(p.parse(&path, &none)), resolve(&rec["lxml"]), "{name}: lxml");
        assert_eq!(outcome(p.parse(&path, &engine("etree"))), resolve(&rec["etree"]), "{name}: etree");
        assert_eq!(outcome(p.to_dict(&path, &none)), resolve(&rec["to_dict"]), "{name}: to_dict");
        assert_eq!(
            outcome(p.find_elements(&path, "//book[@id='b2'] | //line[2]", &none)),
            resolve(&rec["find"]),
            "{name}: find"
        );
        assert_eq!(outcome(p.find_elements(&path, "//dc:title", &none)), resolve(&rec["find_ns"]), "{name}: find_ns");
        assert_eq!(outcome(p.extract_by_tag(&path, "title", &none)), resolve(&rec["by_tag"]), "{name}: by_tag");
        assert_eq!(
            outcome(p.parse(path.display().to_string(), &none)),
            resolve(&rec["str_path"]),
            "{name}: str path"
        );
    }
    let missing = fixtures().join("xml").join("missing.xml");
    assert_eq!(outcome(p.parse(&missing, &none)), resolve(&ORACLE["xml_missing_path"]));
    let long = format!("<a>{}</a>", "x".repeat(400));
    assert_eq!(outcome(p.parse(long.as_str(), &none)), resolve(&ORACLE["xml_long_string"]));
}

#[test]
fn test_xml_dataclasses() {
    let root = XMLElement::new("r", "t");
    let data = XMLData::new(root.clone());
    assert_eq!(data.root, root);
    assert_eq!(serde_json::to_value(&data).unwrap()["root"]["tag"], "r");
    assert_eq!(XMLParser::new(Options::new()).logger.name(), "semantica.xml_parser");
}
