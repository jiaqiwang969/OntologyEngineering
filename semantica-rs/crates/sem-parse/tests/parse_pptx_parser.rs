//! Characterisation tests for `pptx_parser` (PPTXParser, SlideContent, PPTXData) and the
//! python-pptx model behind it, against the original (fixtures/office, section "pptx").

mod office_support;

use office_support::{cases, check, file, options, to_json};
use sem_parse::{Options, PPTXData, PPTXParser, SlideContent};
use serde_json::Value;

#[test]
fn test_pptx_cases_match_oracle() {
    sem_utils::get_progress_tracker().set_displays(Vec::new());
    let parser = PPTXParser::new(Options::new());
    let all = cases("pptx");
    assert!(all.len() >= 20);
    for case in all {
        let path = file(case["file"].as_str().unwrap());
        let result = match case["call"].as_str().unwrap() {
            "parse" => parser.parse(&path, &options(&case)).map(to_json),
            "extract_text" => parser.extract_text(&path).map(Value::String),
            "extract_slides" => parser.extract_slides(&path, &options(&case)).map(to_json),
            other => panic!("unknown call {other}"),
        };
        check(&case, result);
    }
}

#[test]
fn test_pptx_dataclasses() {
    let slide = SlideContent {
        slide_number: 1,
        title: "T".into(),
        text: "x".into(),
        notes: String::new(),
        shapes: Vec::new(),
        images: Vec::new(),
    };
    let data = PPTXData {
        title: "T".into(),
        slides: vec![slide],
        metadata: Default::default(),
    };
    assert_eq!(to_json(&data)["slides"][0]["slide_number"], 1);
    assert_eq!(PPTXParser::default().logger.name(), "semantica.pptx_parser");
}
