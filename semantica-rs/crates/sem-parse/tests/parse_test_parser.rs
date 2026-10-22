//! Port of `tests/parse/test_parser.py`.
//!
//! The original patches `get_logger`/`get_progress_tracker` with MagicMocks; here the
//! parsers get a private tracker. Where the original replaces `JSONParser` with a mock to
//! check that `parse_data(..., data_format="json")` delegates to it, the port gives the JSON
//! parser a tracker with a recording display (the fake) and checks that `JSONParser.parse`
//! ran on the file.

mod common;

use std::sync::{Arc, Mutex};

use sem_parse::{JSONData, JSONParser, Options, StructuredDataParser};
use sem_utils::progress_tracker::{ProgressDisplay, ProgressItem, ProgressTracker};

/// A progress display that records every item update (the test double).
#[derive(Default)]
struct Recorder(Mutex<Vec<ProgressItem>>);

impl ProgressDisplay for Recorder {
    fn update(&self, item: &ProgressItem) {
        self.0.lock().unwrap().push(item.clone());
    }
    fn show_summary(&self, _items: &[ProgressItem]) {}
    fn clear(&self) {}
}

fn recording_tracker() -> (Arc<ProgressTracker>, Arc<Recorder>) {
    let tracker = Arc::new(ProgressTracker::new(true, false, 0.0));
    let recorder = Arc::new(Recorder::default());
    tracker.set_displays(vec![recorder.clone() as Arc<dyn ProgressDisplay>]);
    (tracker, recorder)
}

#[test]
fn test_json_parser_string() {
    common::quiet_progress();
    let mut parser = JSONParser::default();
    parser.progress_tracker = recording_tracker().0;
    let dir = tempfile::tempdir().unwrap();
    let tmp_path = dir.path().join("data.json");
    std::fs::write(&tmp_path, r#"{"key": "value"}"#).unwrap();

    let result = parser
        .parse(tmp_path.display().to_string(), &Options::new())
        .unwrap();
    let _: &JSONData = &result; // isinstance(result, JSONData)
    assert_eq!(result.data["key"], "value");
}

#[test]
fn test_structured_data_parser_init() {
    common::quiet_progress();
    let parser = StructuredDataParser::default();
    let _: &JSONParser = &parser.json_parser; // isinstance(parser.json_parser, JSONParser)
    let _ = &parser.csv_parser; // hasattr(parser, 'csv_parser')
    let _ = &parser.xml_parser; // hasattr(parser, 'xml_parser')
}

#[test]
fn test_structured_data_parser_delegation() {
    common::quiet_progress();
    let mut parser = StructuredDataParser::default();
    parser.progress_tracker = recording_tracker().0;
    let (json_tracker, json_calls) = recording_tracker();
    parser.json_parser.progress_tracker = json_tracker;

    let dir = tempfile::tempdir().unwrap();
    let test_json = dir.path().join("test.json");
    std::fs::write(&test_json, "{}").unwrap();
    parser
        .parse_data(
            test_json.display().to_string(),
            Some("json"),
            &Options::new(),
        )
        .unwrap();

    // mock_instance.parse.assert_called()
    let calls = json_calls.0.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|item| item.submodule.as_deref() == Some("JSONParser") && item.status == "running"),
        "JSONParser.parse was not called"
    );
}
