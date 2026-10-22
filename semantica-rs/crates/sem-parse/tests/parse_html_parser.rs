//! Characterisation tests for `html_parser` (HTMLParser, HTMLData, HTMLMetadata, HTMLElement).
//!
//! Expected values come from the original (`tests/fixtures/markup/generate_oracle.py`, run in
//! the oracle environment): a corpus of HTML files (well-formed and malformed markup,
//! entities, comments, script/style, metadata, links, forms, tables) parsed with and without a
//! base URL, short content strings, and `urljoin` cases through `extract_links`.

mod common;

use std::path::PathBuf;

use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{Map, Value, json};

use sem_parse::html_parser::HTMLData;
use sem_parse::{HTMLElement, HTMLMetadata, HTMLParser, Options};
use sem_utils::SemanticaError;

static ORACLE: Lazy<Value> = Lazy::new(|| common::load_oracle("markup"));

fn fixtures() -> PathBuf {
    common::fixture("markup")
}

/// `{"ok": value}` or `{"error": type, "message": str}`.
fn outcome<T: Serialize>(result: Result<T, SemanticaError>) -> Value {
    match result {
        Ok(v) => json!({ "ok": serde_json::to_value(v).expect("serialisable") }),
        Err(e) => common::err_json(&e),
    }
}

/// The oracle value with the fixture-directory placeholder resolved.
fn expected(v: &Value) -> Value {
    let text = serde_json::to_string(v).unwrap();
    let dir = fixtures().display().to_string();
    serde_json::from_str(&text.replace("@FIXTURES@", &dir)).unwrap()
}

fn opts(pairs: &[(&str, Value)]) -> Options {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

const BASE: &str = "http://example.com/dir/page.html?x=1#top";

#[test]
fn test_html_files_match_oracle() {
    common::quiet_progress();
    let parser = HTMLParser::default();
    let files = ORACLE["html_files"].as_object().unwrap();
    assert!(files.len() >= 10);
    for (name, rec) in files {
        let path = fixtures().join("html").join(name);
        let forms = opts(&[("extract_forms", json!(true))]);
        assert_eq!(outcome(parser.parse(&path, None, &forms)), expected(&rec["parse"]), "{name}: parse");
        assert_eq!(
            outcome(parser.parse(&path, Some(BASE), &forms)),
            expected(&rec["parse_base"]),
            "{name}: parse with base_url"
        );
        let no_extract = opts(&[
            ("extract_links", json!(false)),
            ("extract_images", json!(false)),
            ("extract_tables", json!(false)),
            ("clean_text", json!(false)),
        ]);
        assert_eq!(
            outcome(parser.parse(&path, None, &no_extract)),
            expected(&rec["parse_no_extract"]),
            "{name}: parse without extraction"
        );
        assert_eq!(outcome(parser.extract_text(&path, Some(false))), expected(&rec["raw_text"]), "{name}: raw text");
        assert_eq!(outcome(parser.extract_metadata(&path)), expected(&rec["metadata"]), "{name}: metadata");
        assert_eq!(outcome(parser.extract_links(&path, Some(BASE))), expected(&rec["links_base"]), "{name}: links");
    }
}

#[test]
fn test_html_strings_match_oracle() {
    common::quiet_progress();
    let parser = HTMLParser::new(Options::new());
    for case in ORACLE["html_strings"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let label: String = input.chars().take(40).collect();
        assert_eq!(outcome(parser.parse(input, None, &Options::new())), case["parse"], "parse {label:?}");
        assert_eq!(
            outcome(parser.parse(input, Some("http://example.com/a/b"), &Options::new())),
            case["parse_base"],
            "parse_base {label:?}"
        );
        assert_eq!(outcome(parser.extract_text(input, None)), case["text"], "text {label:?}");
        assert_eq!(outcome(parser.extract_text(input, Some(false))), case["raw_text"], "raw {label:?}");
    }
}

#[test]
fn test_missing_path_raises_file_not_found() {
    common::quiet_progress();
    let path = fixtures().join("html").join("does-not-exist.html");
    let got = outcome(HTMLParser::default().parse(&path, None, &Options::new()));
    assert_eq!(got, expected(&ORACLE["missing_path"]));
}

#[test]
fn test_urljoin_matches_oracle() {
    common::quiet_progress();
    let parser = HTMLParser::default();
    for case in ORACLE["urljoin"].as_array().unwrap() {
        let base = case["base"].as_str().unwrap();
        let hrefs: Vec<&str> = case["hrefs"].as_array().unwrap().iter().map(|h| h.as_str().unwrap()).collect();
        let html: String = if hrefs.len() == 1 {
            format!("<a href=\"{}\">x</a>", hrefs[0])
        } else {
            hrefs
                .iter()
                .enumerate()
                .map(|(i, h)| format!("<a href=\"{}\">{i}</a>", h.replace('"', "&quot;")))
                .collect()
        };
        // The multi-link documents are too long for `Path(html).exists()`: OSError.
        assert_eq!(outcome(parser.extract_links(html.as_str(), Some(base))), case["links"], "base {base}");
    }
    for case in ORACLE["urljoin_files"].as_array().unwrap() {
        let base = case["base"].as_str().unwrap();
        let path = fixtures().join("urljoin").join(case["file"].as_str().unwrap());
        assert_eq!(outcome(parser.extract_links(&path, Some(base))), case["links"], "file base {base}");
    }
}

#[test]
fn test_html_dataclasses() {
    let data = HTMLData::new(Map::new(), "text", "<p>text</p>");
    assert_eq!(data.links, Vec::<Map<String, Value>>::new());
    assert_eq!(data.structure, json!([]));
    assert_eq!(HTMLData::default().text, "");
    let value = serde_json::to_value(&data).unwrap();
    assert_eq!(
        value,
        json!({"metadata": {}, "text": "text", "html": "<p>text</p>", "links": [], "images": [],
               "forms": [], "tables": [], "structure": []})
    );
    let back: HTMLData = serde_json::from_value(value).unwrap();
    assert_eq!(back, data);

    let metadata = HTMLMetadata {
        title: Some("T".into()),
        ..HTMLMetadata::default()
    };
    let dict = metadata.to_dict();
    assert_eq!(
        dict.keys().cloned().collect::<Vec<_>>(),
        [
            "title",
            "description",
            "keywords",
            "author",
            "og_title",
            "og_description",
            "og_image",
            "og_url",
            "twitter_card",
            "canonical_url"
        ]
    );
    assert_eq!(dict["title"], json!("T"));

    let mut element = HTMLElement::new("div", "hello");
    element.attributes.insert("class".into(), "x".into());
    element.children.push(HTMLElement::new("span", ""));
    assert_eq!(
        serde_json::to_value(&element).unwrap(),
        json!({"tag": "div", "text": "hello", "attributes": {"class": "x"},
               "children": [{"tag": "span", "text": "", "attributes": {}, "children": []}]})
    );
}

#[test]
fn test_parser_attributes() {
    let parser = HTMLParser::new(opts(&[("a", json!(1))]));
    assert_eq!(parser.config, opts(&[("a", json!(1))]));
    assert_eq!(parser.logger.name(), "semantica.html_parser");
}
