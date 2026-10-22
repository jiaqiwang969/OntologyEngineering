//! Characterisation of `encoding_handler.EncodingHandler` against the oracle (chardet 7.7.0
//! detection, conversions, BOMs, error strategies, file handling).

mod common;

use std::path::Path;

use sem_normalize::EncodingHandler;
use sem_normalize::encoding_handler::TextOrBytes;
use sem_utils::SemanticaError;
use serde_json::{Value, json};

use common::{check, data_arg, data_json, kw, oracle};

fn cases(key: &str) -> &'static Vec<Value> {
    oracle(key).as_array().expect("a list of cases")
}

fn detection(result: (Option<String>, f64)) -> Value {
    json!({"ok": [result.0, result.1]})
}

/// Write the oracle's scratch files into `dir`.
fn write_files(dir: &Path) {
    for (name, bytes) in oracle("files").as_object().unwrap() {
        let content: Vec<u8> = bytes
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect();
        std::fs::write(dir.join(name), content).unwrap();
    }
}

/// An outcome with the scratch directory replaced by `<dir>`, as the oracle records it.
fn scrubbed<T: Into<Value>>(result: Result<T, SemanticaError>, dir: &Path) -> Value {
    let text = common::outcome(result).to_string();
    let dir = serde_json::to_string(&dir.display().to_string()).unwrap();
    let dir = &dir[1..dir.len() - 1];
    serde_json::from_str(&text.replace(dir, "<dir>")).unwrap()
}

#[test]
fn test_detect_matches_oracle() {
    let handler = EncodingHandler::default();
    for case in cases("detect") {
        check(detection(handler.detect(data_arg(&case["data"]))), case);
    }
}

#[test]
fn test_files_match_oracle() {
    let handler = EncodingHandler::default();
    let dir = tempfile::tempdir().unwrap();
    write_files(dir.path());
    for case in cases("detect_file") {
        let path = dir.path().join(case["file"].as_str().unwrap());
        let result = handler
            .detect_file(&path, case["sample_size"].as_u64().map(|s| s as usize))
            .map(|(e, c)| json!([e, c]));
        assert_eq!(scrubbed(result, dir.path()), case["result"], "case {case}");
    }
    for case in cases("convert_file_to_utf8") {
        let name = case["file"].as_str().unwrap();
        let out_path = dir.path().join("out").join(format!("{name}.utf8"));
        let result = handler.convert_file_to_utf8(dir.path().join(name), Some(&out_path));
        assert_eq!(scrubbed(result, dir.path()), case["result"], "case {case}");
        let written = std::fs::read(&out_path).ok().map(|b| json!(b));
        assert_eq!(
            written.unwrap_or(Value::Null),
            case["written"],
            "case {case}"
        );
    }
    let result = handler.convert_file_to_utf8(dir.path().join("utf8.txt"), None);
    assert_eq!(
        scrubbed(result, dir.path()),
        *oracle("convert_file_to_utf8_no_output")
    );
}

#[test]
fn test_conversions_match_oracle() {
    let handler = EncodingHandler::default();
    for case in cases("convert_to_utf8") {
        let result = handler.convert_to_utf8(data_arg(&case["data"]), case["source"].as_str());
        check(common::outcome(result), case);
    }
    for case in cases("remove_bom") {
        let result = handler.remove_bom(data_arg(&case["data"]));
        check(json!({"ok": data_json(&result)}), case);
    }
    for case in cases("validate_encoding") {
        let result =
            handler.validate_encoding(data_arg(&case["data"]), case["encoding"].as_str().unwrap());
        check(json!({"ok": result}), case);
    }
}

fn bytes_of(data: TextOrBytes) -> Vec<u8> {
    match data {
        TextOrBytes::Bytes(b) => b,
        TextOrBytes::Text(s) => s.into_bytes(),
    }
}

#[test]
fn test_handle_encoding_error_matches_oracle() {
    let handler = EncodingHandler::default();
    for case in cases("handle_encoding_error") {
        let data = bytes_of(data_arg(&case["data"]));
        let result = handler.handle_encoding_error(&data, case["strategy"].as_str());
        check(common::outcome(result), case);
    }
    let strict = EncodingHandler::new(kw(json!({
        "fallback_encodings": ["ascii"],
        "default_encoding": "ascii"
    })));
    for case in cases("handle_encoding_error_strict_fallbacks") {
        let data = bytes_of(data_arg(&case["data"]));
        let result = strict.handle_encoding_error(&data, case["strategy"].as_str());
        check(common::outcome(result), case);
    }
}

#[test]
fn test_configuration_matches_oracle() {
    let expected = oracle("encoding_config");
    let handler = EncodingHandler::new(kw(json!({
        "default_encoding": "latin-1",
        "fallback_encodings": ["utf-8"]
    })));
    assert_eq!(
        json!(handler.default_encoding),
        expected["default_encoding"]
    );
    assert_eq!(
        json!(handler.fallback_encodings),
        expected["fallback_encodings"]
    );
    assert_eq!(detection(handler.detect(b"")), expected["detect_empty"]);
    let default = EncodingHandler::default();
    assert_eq!(
        json!({
            "default_encoding": default.default_encoding,
            "fallback_encodings": default.fallback_encodings
        }),
        expected["default"]
    );
    assert!(default.progress_tracker.enabled());
}
