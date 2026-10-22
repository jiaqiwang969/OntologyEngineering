//! Tests for `pdf_parser` (PDFParser, PDFPage, PDFMetadata) and the pdfplumber-style PDF
//! model behind it.
//!
//! pdfplumber is not installed in the reference environment (there the original raises
//! "pdfplumber is required for PDF parsing"), so the expectations come from the fixture
//! construction (`tests/fixtures/pdfmedia/generate_fixtures.py` writes the PDFs as raw PDF
//! syntax with known text, metadata, page boxes, ruled tables and image placements); the
//! text content, metadata and page sizes were cross-checked with pypdfium2 in the oracle.

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use sem_parse::pdf_parser::{PdfDocument, PdfPage};
use sem_parse::{Options, PDFMetadata, PDFPage, PDFParser};

fn f(name: &str) -> PathBuf {
    common::fixture("pdfmedia").join(name)
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

fn parser() -> PDFParser {
    common::quiet_progress();
    PDFParser::default()
}

#[test]
fn test_parse_text_and_metadata() {
    let result = parser().parse(f("simple.pdf"), None, &Options::new()).unwrap();
    assert_eq!(
        Value::Object(result),
        json!({
            "metadata": {
                "title": "Test PDF",
                "author": "Ünïcode Author",
                "subject": "Café notes",
                "creator": "fixture writer",
                "producer": "hand",
                "creation_date": "D:20240102030405Z",
                "modification_date": "D:20240203040506+01'00'",
                "page_count": 1
            },
            "pages": [{
                "page_number": 1,
                "text": "Hello World\nSecond line here.\nKerned words",
                "width": 612,
                "height": 792,
                "images": [],
                "tables": []
            }],
            "full_text": "Hello World\nSecond line here.\nKerned words",
            "total_pages": 1
        })
    );
}

#[test]
fn test_pages_sizes_rotation_and_selection() {
    let p = parser();
    let result = p.parse(f("multipage.pdf"), Some("pipe-1"), &Options::new()).unwrap();
    assert_eq!(result["total_pages"], 3);
    assert_eq!(result["metadata"]["page_count"], 3);
    // Missing Info entries: None for the strings, "" for the dates (str(metadata.get(..., ""))).
    assert_eq!(result["metadata"]["title"], Value::Null);
    assert_eq!(result["metadata"]["creation_date"], "");
    let pages = result["pages"].as_array().unwrap();
    assert_eq!(pages[0]["text"], "Page one");
    assert_eq!((pages[1]["width"].clone(), pages[1]["height"].clone()), (json!(595.276), json!(841.89)));
    assert_eq!(pages[1]["text"], "Page two");
    // The rotated page reports the rotated box.
    assert_eq!((pages[2]["width"].clone(), pages[2]["height"].clone()), (json!(792), json!(612)));
    // `pages` selects zero-based page indices; out-of-range indices are skipped.
    let some = p.parse(f("multipage.pdf"), None, &opts(json!({"pages": [0, 2, 7]}))).unwrap();
    let numbers: Vec<Value> = some["pages"].as_array().unwrap().iter().map(|pg| pg["page_number"].clone()).collect();
    assert_eq!(numbers, vec![json!(1), json!(3)]);
    assert_eq!(p.extract_text(f("multipage.pdf"), Some(&[1])).unwrap(), "Page two");
}

#[test]
fn test_tables_images_and_fonts() {
    let p = parser();
    let tables = p.extract_tables(f("table.pdf"), Some(&[0])).unwrap();
    assert_eq!(
        tables,
        vec![json!({"data": [["Name", "Age"], ["Alice", "30"], ["Bob", "25"]], "bbox": null, "page_number": 1})]
    );
    let text = p.extract_text(f("table.pdf"), Some(&[0])).unwrap();
    assert_eq!(text, "People\nName Age\nAlice 30\nBob 25");
    let images = p.extract_images(f("image.pdf"), None).unwrap();
    assert_eq!(
        images,
        vec![
            json!({"x0": 72.0, "y0": 600.0, "x1": 172.0, "y1": 650.0, "width": 100.0, "height": 50.0, "page_number": 1}),
            json!({"x0": 300.0, "y0": 400.0, "x1": 320.0, "y1": 410.0, "width": 20.0, "height": 10.0, "page_number": 1}),
        ]
    );
    // Encodings: WinAnsi with Differences (a ligature glyph name and /Euro), a Type0
    // Identity-H font with a ToUnicode CMap, and text inside a form XObject.
    let fonts = p.parse(f("fonts.pdf"), None, &Options::new()).unwrap();
    assert_eq!(fonts["full_text"], "café €\nfi€\nCID\nIn a form");
    // Without text extraction the pages have empty text.
    let no_text = p.parse(f("simple.pdf"), None, &opts(json!({"extract_text": false}))).unwrap();
    assert_eq!(no_text["full_text"], "");
}

#[test]
fn test_error_paths_follow_original() {
    let p = parser();
    let missing = p.parse(f("missing.pdf"), None, &Options::new()).unwrap_err();
    assert_eq!(missing.type_name(), "ValidationError");
    assert_eq!(missing.to_string(), format!("PDF file not found: {}", f("missing.pdf").display()));
    let wrong = p.parse(f("notes.txt"), None, &Options::new()).unwrap_err();
    assert_eq!(wrong.to_string(), format!("File is not a PDF: {}", f("notes.txt").display()));
    // pdfminer's message for a file without a trailer, wrapped as the original wraps it.
    let corrupt = p.parse(f("notapdf.pdf"), None, &Options::new()).unwrap_err();
    assert_eq!(corrupt.type_name(), "ProcessingError");
    assert_eq!(corrupt.to_string(), "Failed to parse PDF: No /Root object! - Is this really a PDF?");
    // `extract_text`/`extract_tables` pass `pages=None` on explicitly, which the original's
    // `for page_num in page_numbers` cannot iterate.
    let none_pages = p.extract_text(f("simple.pdf"), None).unwrap_err();
    assert_eq!(none_pages.to_string(), "Failed to parse PDF: 'NoneType' object is not iterable");
    let none_tables = p.extract_tables(f("simple.pdf"), None).unwrap_err();
    assert_eq!(none_tables.to_string(), "Failed to parse PDF: 'NoneType' object is not iterable");
}

#[test]
fn test_pdf_opener_injection_and_dataclasses() {
    let mut p = parser();
    p.pdf_opener = Arc::new(|_path| {
        let mut metadata = Map::new();
        metadata.insert("Title".to_string(), json!("Fake"));
        Ok(PdfDocument::new(metadata, vec![PdfPage::new(100, 200).with_text("fake page")]))
    });
    let result = p.parse(f("simple.pdf"), None, &Options::new()).unwrap();
    assert_eq!(result["metadata"]["title"], "Fake");
    assert_eq!(result["full_text"], "fake page");
    assert_eq!(result["pages"][0]["width"], 100);
    let page = PDFPage::default();
    assert_eq!(serde_json::to_value(&page).unwrap()["images"], json!([]));
    assert_eq!(serde_json::to_value(PDFMetadata::default()).unwrap()["page_count"], 0);
    assert_eq!(p.logger.name(), "semantica.pdf_parser");
}
