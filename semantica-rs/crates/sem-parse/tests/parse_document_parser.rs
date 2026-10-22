//! Characterisation tests for `document_parser.rs` against the original (fixtures/core).

mod common;

use std::path::PathBuf;

use common::{assert_case, files_dir, load_oracle, oracle_form, quiet_progress};
use once_cell::sync::Lazy;
use sem_parse::document_parser::{DocumentItem, DocumentSource};
use sem_parse::{DocumentParser, Options, Source};
use serde_json::{Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| load_oracle("core"));

fn f(name: &str) -> PathBuf {
    files_dir("core").join(name)
}

fn s(name: &str) -> Source {
    Source::Text(f(name).display().to_string())
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

fn check(key: &str, actual: Value) {
    assert_case(&ORACLE, "document", key, actual);
}

#[test]
fn test_parse_document_text_routes() {
    quiet_progress();
    let p = DocumentParser::default();
    let none = Options::new();
    check(
        "parse_txt",
        oracle_form("core", p.parse_document(f("notes.txt"), None, &none)),
    );
    check(
        "parse_txt_type",
        oracle_form(
            "core",
            p.parse_document(f("unknown.xyz"), Some("text"), &none),
        ),
    );
    check(
        "parse_alias",
        oracle_form("core", p.parse(f("notes.txt"), &none)),
    );
    check(
        "unknown_format",
        oracle_form("core", p.parse_document(f("unknown.xyz"), None, &none)),
    );
    check(
        "missing",
        oracle_form("core", p.parse_document(f("missing.txt"), None, &none)),
    );
    check(
        "bad_type",
        oracle_form("core", p.parse_document(f("notes.txt"), Some("rtf"), &none)),
    );
    check(
        "parse_encoding",
        oracle_form(
            "core",
            p.parse_document(f("latin1.txt"), None, &opts(json!({"encoding": "latin-1"}))),
        ),
    );
    assert_eq!(
        serde_json::to_value(&p.supported_formats).unwrap(),
        ORACLE["document"]["supported_formats"]
    );
}

#[test]
fn test_extract_text_and_metadata() {
    quiet_progress();
    let p = DocumentParser::default();
    let none = Options::new();
    check(
        "extract_text",
        oracle_form("core", p.extract_text(f("notes.txt"), &none)),
    );
    check(
        "extract_text_crlf",
        oracle_form("core", p.extract_text(f("crlf.txt"), &none)),
    );
    check(
        "extract_text_latin1",
        oracle_form(
            "core",
            p.extract_text(f("latin1.txt"), &opts(json!({"encoding": "latin-1"}))),
        ),
    );
    check(
        "extract_text_utf8_ignore",
        oracle_form("core", p.extract_text(f("latin1.txt"), &none)),
    );
    check(
        "extract_text_missing",
        oracle_form("core", p.extract_text(f("missing.txt"), &none)),
    );
    check(
        "extract_text_unknown",
        oracle_form("core", p.extract_text(f("unknown.xyz"), &none)),
    );
    check(
        "extract_metadata",
        oracle_form("core", p.extract_metadata(f("notes.txt"))),
    );
}

#[test]
fn test_parse_batch_and_lists() {
    quiet_progress();
    let p = DocumentParser::default();
    let none = Options::new();
    check(
        "batch",
        oracle_form(
            "core",
            p.parse_batch(
                &[
                    s("notes.txt"),
                    s("missing.txt"),
                    s("unknown.xyz"),
                    s("crlf.txt"),
                ],
                &none,
            ),
        ),
    );
    check(
        "batch_stop",
        oracle_form(
            "core",
            p.parse_batch(
                &[s("notes.txt"), s("missing.txt"), s("crlf.txt")],
                &opts(json!({"continue_on_error": false})),
            ),
        ),
    );
    check(
        "batch_empty",
        oracle_form("core", p.parse_batch(&[], &none)),
    );
    let items: Vec<DocumentItem> = ["notes.txt", "missing.txt", "crlf.txt"]
        .iter()
        .map(|n| DocumentItem::from(f(n)))
        .collect();
    check(
        "parse_list",
        oracle_form("core", p.parse(DocumentSource::Many(items), &none)),
    );
    let bad = vec![
        DocumentItem::from(f("notes.txt")),
        DocumentItem::Unsupported("int".to_string()),
    ];
    check(
        "parse_list_bad_item",
        oracle_form("core", p.parse(DocumentSource::Many(bad), &none)),
    );
}
