//! Characterisation tests for `docx_parser` (DOCXParser, DocxSection, DocxMetadata) and the
//! python-docx model behind it, against the original (fixtures/office, section "docx":
//! python-docx-generated and hand-crafted documents, formatting and table options, error
//! paths).

mod office_support;

use office_support::{cases, check, file, options, to_json};
use sem_parse::docx_parser::Document;
use sem_parse::docx_parser::DocxDocument;
use sem_parse::{DOCXParser, DocxMetadata, DocxSection, Options};
use serde_json::Value;

#[test]
fn test_docx_cases_match_oracle() {
    sem_utils::get_progress_tracker().set_displays(Vec::new());
    let parser = DOCXParser::new(Options::new());
    let all = cases("docx");
    assert!(all.len() >= 30);
    for case in all {
        let path = file(case["file"].as_str().unwrap());
        let result = match case["call"].as_str().unwrap() {
            "parse" => parser.parse(&path, None, &options(&case)).map(Value::Object),
            "extract_text" => parser.extract_text(&path).map(Value::String),
            "extract_tables" => parser.extract_tables(&path).map(Value::Array),
            other => panic!("unknown call {other}"),
        };
        check(&case, result);
    }
}

#[test]
fn test_document_loader_and_dataclasses() {
    // `docx.Document` is the default loader; a hand-built document can replace it.
    let doc = Document(&file("basic.docx")).unwrap();
    assert!(!doc.paragraphs.is_empty());
    let mut parser = DOCXParser::default();
    let fake = DocxDocument::from_paragraphs(Vec::new(), "Fake title");
    parser.document_loader = std::sync::Arc::new(move |_path| Ok(fake.clone()));
    let result = parser.parse(file("basic.docx"), Some("pipeline-1"), &Options::new()).unwrap();
    assert_eq!(result["metadata"]["title"], "Fake title");
    assert_eq!(result["total_paragraphs"], 0);
    let section = DocxSection {
        heading: "H".into(),
        level: 1,
        content: String::new(),
        paragraphs: Vec::new(),
        tables: Vec::new(),
    };
    assert_eq!(to_json(&section)["heading"], "H");
    assert_eq!(to_json(DocxMetadata::default())["title"], Value::Null);
    assert_eq!(parser.logger.name(), "semantica.docx_parser");
}
