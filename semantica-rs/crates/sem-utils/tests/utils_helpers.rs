//! Characterisation of `semantica/utils/helpers.py` (expected values from the oracle).

mod common;

use std::sync::Mutex;

use common::{err_json, expected_outcome, obj, oracle, outcome};
use sem_utils::helpers::{
    self, HashInput, Module, RetryOnError, Timestamp, classify_path_distance, register_module,
    require_mapping,
};
use sem_utils::logging::{Handler, LogLevel, get_logger};
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, ValidationError};
use serde_json::{Map, Value, json};

static LOGGER_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_format_data_matches_oracle() {
    for case in oracle("format_data").as_array().unwrap() {
        let got = outcome(helpers::format_data(&case["data"], case["format"].as_str()));
        assert_eq!(got, expected_outcome(case), "{case}");
    }
    // Default format is json.
    assert_eq!(
        helpers::format_data(&json!({"a": 1}), None).unwrap(),
        "{\n  \"a\": 1\n}"
    );
}

#[test]
fn test_clean_text_matches_oracle() {
    for case in oracle("clean_text").as_array().unwrap() {
        let text = case[0].as_str().unwrap();
        let preserve = case[1].as_bool().unwrap();
        assert_eq!(
            helpers::clean_text(text, Some(preserve)),
            case[2].as_str().unwrap(),
            "clean_text({text:?}, {preserve})"
        );
    }
}

#[test]
fn test_normalize_entities_matches_oracle() {
    let fixture = oracle("normalize_entities");
    let entities: Vec<Map<String, Value>> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(obj)
        .collect();
    let got: Vec<Value> = helpers::normalize_entities(&entities)
        .into_iter()
        .map(Value::Object)
        .collect();
    assert_eq!(Value::Array(got), fixture["expected"]);
}

#[test]
fn test_hash_data_matches_oracle() {
    for case in oracle("hash_data").as_array().unwrap() {
        let expected = case["expected"].as_str().unwrap();
        let got = match case["kind"].as_str().unwrap() {
            "str" => helpers::hash_data(case["value"].as_str().unwrap()),
            "bytes" => {
                let bytes: Vec<u8> = case["value"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b.as_u64().unwrap() as u8)
                    .collect();
                helpers::hash_data(&bytes)
            }
            _ => {
                let map = obj(&case["value"]);
                helpers::hash_data(HashInput::Dict(&map))
            }
        };
        assert_eq!(got, expected, "{case}");
    }
}

#[test]
fn test_safe_filename_matches_oracle() {
    for case in oracle("safe_filename").as_array().unwrap() {
        let name = case[0].as_str().unwrap();
        let max = case[1].as_u64().unwrap() as usize;
        assert_eq!(
            helpers::safe_filename(name, Some(max)),
            case[2].as_str().unwrap(),
            "safe_filename({name:?}, {max})"
        );
    }
    assert_eq!(helpers::safe_filename("my file.txt", None), "my_file.txt");
}

#[test]
fn test_ensure_directory_creates_and_reports_conflicts() {
    let tmp = tempfile::tempdir().unwrap();
    let nested = tmp.path().join("x").join("y");
    assert_eq!(helpers::ensure_directory(&nested).unwrap(), nested);
    assert!(nested.is_dir());
    // Idempotent.
    helpers::ensure_directory(&nested).unwrap();
    assert_eq!(
        helpers::ensure_directory("").unwrap(),
        std::path::PathBuf::from(".")
    );
    let file = tmp.path().join("f");
    std::fs::write(&file, "x").unwrap();
    let err = helpers::ensure_directory(&file).unwrap_err();
    assert_eq!(err.type_name(), "FileExistsError");
    assert_eq!(
        err.to_string(),
        format!("[Errno 17] File exists: '{}'", file.display())
    );
}

#[test]
fn test_read_and_write_json_file_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sub").join("deeper").join("out.json");
    let data = json!({"b": [1, "\u{e9}"], "a": null});
    helpers::write_json_file(&data, &path, None, None).unwrap();
    // Oracle: write_json_file({"b": [1, "é"], "a": None}, p) writes this text (45 bytes).
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\n  \"b\": [\n    1,\n    \"\u{e9}\"\n  ],\n  \"a\": null\n}"
    );
    assert_eq!(helpers::get_file_size(&path).unwrap(), 45);
    assert_eq!(helpers::read_json_file(&path).unwrap(), data);

    helpers::write_json_file(&data, &path, Some(4), Some(true)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\n    \"b\": [\n        1,\n        \"\\u00e9\"\n    ],\n    \"a\": null\n}"
    );
}

#[test]
fn test_read_json_file_errors_match_oracle() {
    let err = helpers::read_json_file("/nonexistent/dir/x.json").unwrap_err();
    assert_eq!(
        err_json(&err),
        json!({"error": "FileNotFoundError", "message": "File not found: /nonexistent/dir/x.json"})
    );
    assert_eq!(
        format!("{err:?}"),
        "FileNotFoundError('File not found: /nonexistent/dir/x.json')"
    );
    let err = helpers::read_json_file("./a//b.json").unwrap_err();
    assert_eq!(err.to_string(), "File not found: a/b.json");

    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.json");
    std::fs::write(&bad, "{bad").unwrap();
    let err = helpers::read_json_file(&bad).unwrap_err();
    assert_eq!(err.type_name(), "JSONDecodeError");
    assert!(err.is_value_error());

    let err = helpers::read_json_file(tmp.path()).unwrap_err();
    assert_eq!(err.type_name(), "IsADirectoryError");
    assert_eq!(
        err.to_string(),
        format!("[Errno 21] Is a directory: '{}'", tmp.path().display())
    );
}

#[test]
fn test_get_file_size_missing_file_matches_oracle() {
    let err = helpers::get_file_size("/nonexistent/dir/x.bin").unwrap_err();
    assert_eq!(err.type_name(), "FileNotFoundError");
    assert_eq!(
        err.to_string(),
        "[Errno 2] No such file or directory: '/nonexistent/dir/x.bin'"
    );
    assert_eq!(
        format!("{err:?}"),
        "FileNotFoundError(2, 'No such file or directory')"
    );
    let err = helpers::get_file_size("./a//b.json").unwrap_err();
    assert_eq!(
        err.to_string(),
        "[Errno 2] No such file or directory: 'a/b.json'"
    );
}

fn dt_value(dt: &PyDateTime) -> Value {
    use chrono::{Datelike, Timelike};
    let wall = dt.wall();
    json!({
        "wall": [wall.year(), wall.month(), wall.day(), wall.hour(), wall.minute(), wall.second(), wall.nanosecond() / 1000],
        "offset": dt.offset().map(|o| o.local_minus_utc()),
        "str": dt.to_string(),
    })
}

#[test]
fn test_parse_timestamp_matches_oracle() {
    for case in oracle("parse_timestamp").as_array().unwrap() {
        let data = case["data"].as_str().unwrap();
        let got = match helpers::parse_timestamp(data, None) {
            Ok(dt) => json!({"ok": dt_value(&dt)}),
            Err(e) => err_json(&e),
        };
        assert_eq!(got, expected_outcome(case), "parse_timestamp({data:?})");
    }
    for case in oracle("parse_timestamp_with_format").as_array().unwrap() {
        let got =
            match helpers::parse_timestamp(case["data"].as_str().unwrap(), case["format"].as_str())
            {
                Ok(dt) => json!({"ok": dt_value(&dt)}),
                Err(e) => err_json(&e),
            };
        assert_eq!(got, expected_outcome(case), "{case}");
    }
}

#[test]
fn test_format_timestamp_matches_oracle() {
    for case in oracle("format_timestamp_aware").as_array().unwrap() {
        let ts = case["ts"].as_f64().unwrap();
        let got = outcome(helpers::format_timestamp(
            Some(Timestamp::Epoch(ts)),
            case["format"].as_str(),
            Some(true),
        ));
        assert_eq!(got, expected_outcome(case), "format_timestamp({ts})");
    }
    for case in oracle("format_timestamp_datetime").as_array().unwrap() {
        let spec = &case["dt"];
        let dt = make_dt(spec);
        assert_eq!(
            helpers::format_timestamp(Some(dt.into()), case["format"].as_str(), None).unwrap(),
            case["ok"].as_str().unwrap()
        );
    }
    // Defaults: the current UTC time in `%Y-%m-%d %H:%M:%S`.
    let now = helpers::format_timestamp(None, None, None).unwrap();
    assert_eq!(now.len(), 19);
    assert!(helpers::parse_timestamp(&now, None).is_ok());
    assert!(helpers::format_timestamp(Some(Timestamp::from(0i64)), None, Some(false)).is_ok());
}

fn make_dt(spec: &Value) -> PyDateTime {
    use chrono::{FixedOffset, NaiveDate, TimeZone};
    let f = |v: &Value| -> Vec<i64> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect()
    };
    let (fields, offset) = match spec.get("naive") {
        Some(naive) => (f(naive), None),
        None => {
            let a = f(&spec["aware"]);
            (a.clone(), Some(a[7]))
        }
    };
    let naive = NaiveDate::from_ymd_opt(fields[0] as i32, fields[1] as u32, fields[2] as u32)
        .unwrap()
        .and_hms_micro_opt(
            fields[3] as u32,
            fields[4] as u32,
            fields[5] as u32,
            fields[6] as u32,
        )
        .unwrap();
    match offset {
        None => PyDateTime::Naive(naive),
        Some(seconds) => PyDateTime::Aware(
            FixedOffset::east_opt(seconds as i32)
                .unwrap()
                .from_local_datetime(&naive)
                .unwrap(),
        ),
    }
}

#[test]
fn test_merge_dicts_matches_oracle() {
    for case in oracle("merge_dicts").as_array().unwrap() {
        let dicts: Vec<Map<String, Value>> = case[0].as_array().unwrap().iter().map(obj).collect();
        let refs: Vec<&Map<String, Value>> = dicts.iter().collect();
        let deep = case[1].as_bool().unwrap();
        let merged = helpers::merge_dicts(&refs, Some(deep));
        assert_eq!(Value::Object(merged.clone()), case[2], "{case}");
        // Key order follows Python's dict insertion order.
        let keys: Vec<&String> = merged.keys().collect();
        let expected_keys: Vec<&String> = case[2].as_object().unwrap().keys().collect();
        assert_eq!(keys, expected_keys);
    }
}

#[test]
fn test_chunk_list_matches_oracle() {
    for case in oracle("chunk_list").as_array().unwrap() {
        let items = case[0].as_array().unwrap();
        let size = case[1].as_u64().unwrap() as usize;
        let chunks: Vec<Value> = helpers::chunk_list(items, size)
            .unwrap()
            .into_iter()
            .map(Value::Array)
            .collect();
        assert_eq!(Value::Array(chunks), case[2]);
    }
    let zero = helpers::chunk_list(&[1], 0).unwrap_err();
    assert_eq!(err_json(&zero), *oracle("chunk_list_zero"));
}

#[test]
fn test_flatten_dict_matches_oracle() {
    for case in oracle("flatten_dict").as_array().unwrap() {
        let data = obj(&case[0]);
        let got = outcome(
            helpers::flatten_dict(&data, case[1].as_str(), case[2].as_str()).map(Value::Object),
        );
        assert_eq!(got, case[3], "{case}");
    }
}

#[test]
fn test_get_nested_value_matches_oracle() {
    let fixture = oracle("get_nested_value");
    let data = obj(&fixture["data"]);
    for case in fixture["cases"].as_array().unwrap() {
        let default = if case[1].is_null() {
            None
        } else {
            Some(case[1].clone())
        };
        let got = outcome(helpers::get_nested_value(
            &data,
            case[0].as_str().unwrap(),
            default,
            case[2].as_str(),
        ));
        assert_eq!(got, case[3], "{case}");
    }
}

#[test]
fn test_set_nested_value_matches_oracle() {
    for case in oracle("set_nested_value").as_array().unwrap() {
        let mut data = obj(&case["data"]);
        let result = helpers::set_nested_value(
            &mut data,
            case["key"].as_str().unwrap(),
            case["value"].clone(),
            case["sep"].as_str(),
        );
        let got = match result {
            Ok(()) => json!({"ok": null}),
            Err(e) => err_json(&e),
        };
        assert_eq!(got, case["result"], "{case}");
        assert_eq!(Value::Object(data), case["after"], "{case}");
    }
}

#[test]
fn test_classify_path_distance_matches_oracle() {
    for case in oracle("classify_path_distance").as_array().unwrap() {
        assert_eq!(
            classify_path_distance(case[0].as_i64().unwrap()),
            case[1].as_str().unwrap()
        );
    }
}

#[test]
fn test_safe_import_matches_oracle() {
    let _guard = LOGGER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for case in oracle("safe_import").as_array().unwrap() {
        let (module, available) = helpers::safe_import(
            case["name"].as_str().unwrap(),
            case["package"].as_str(),
            Some(Module::new("DEFAULT")),
            None,
        );
        assert_eq!(available, case["available"].as_bool().unwrap(), "{case}");
        assert_eq!(
            module.map(|m| m.name().to_string()),
            case["module"].as_str().map(str::to_string),
            "{case}"
        );
    }
    // Registering a module makes it importable.
    assert!(!helpers::safe_import("spacy_rs_test_only", None, None, None).1);
    let registered = register_module("spacy_rs_test_only");
    let (module, available) = helpers::safe_import("spacy_rs_test_only", None, None, None);
    assert!(available);
    assert_eq!(module, Some(registered));
}

#[test]
fn test_safe_import_logs_error_message_like_oracle() {
    let _guard = LOGGER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let logger = get_logger("utils.helpers");
    let capture = Handler::capture();
    logger.add_handler(capture.clone());
    logger.set_level(LogLevel::DEBUG);
    helpers::safe_import(
        "definitely_missing_mod",
        None,
        None,
        Some("optional dep missing"),
    );
    helpers::safe_import("json.missing_sub", None, None, Some("sub"));
    helpers::safe_import("..x", Some("a"), None, Some("rel"));
    logger.remove_handler(&capture);
    logger.set_level(LogLevel::NOTSET);
    assert_eq!(
        capture.messages(),
        vec![
            "optional dep missing: No module named 'definitely_missing_mod'",
            "sub: No module named 'json.missing_sub'",
            "rel: attempted relative import beyond top-level package",
        ]
    );
}

#[test]
fn test_retry_on_error_matches_oracle() {
    // Oracle: a function failing twice then succeeding returns "ok" after 3 calls.
    let mut calls = 0;
    let result: Result<&str, SemanticaError> =
        helpers::retry_on_error(Some(3), Some(0.0), Some(2.0)).call(|| {
            calls += 1;
            if calls < 3 {
                Err(SemanticaError::Value(format!("flaky {calls}")))
            } else {
                Ok("ok")
            }
        });
    assert_eq!(result.unwrap(), "ok");
    assert_eq!(calls, 3);

    // max_retries=1: the last error after 2 calls.
    let mut calls = 0;
    let err = helpers::retry_on_error(Some(1), Some(0.0), None)
        .call(|| -> Result<(), SemanticaError> {
            calls += 1;
            Err(SemanticaError::Value(format!("always {calls}")))
        })
        .unwrap_err();
    assert_eq!(err.to_string(), "always 2");
    assert_eq!(calls, 2);

    // exceptions=(KeyError,): a ValueError is not retried.
    let mut calls = 0;
    let err = helpers::retry_on_error(Some(3), Some(0.0), None)
        .call_if(
            |e: &SemanticaError| matches!(e, SemanticaError::Key(_)),
            || -> Result<(), SemanticaError> {
                calls += 1;
                Err(SemanticaError::Value("not retried".into()))
            },
        )
        .unwrap_err();
    assert_eq!(err.to_string(), "not retried");
    assert_eq!(calls, 1);

    assert_eq!(
        helpers::retry_on_error(None, None, None),
        RetryOnError {
            max_retries: 3,
            delay: 1.0,
            backoff_factor: 2.0
        }
    );
}

#[test]
fn test_require_mapping_matches_oracle() {
    for case in oracle("require_mapping").as_array().unwrap() {
        let keys: Vec<&str> = case["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        let got = match require_mapping(&case["data"], &keys) {
            Ok(()) => json!({"ok": null}),
            Err(e) => err_json(&e),
        };
        assert_eq!(got, expected_outcome(case), "{case}");
    }
    let err = require_mapping(&json!([1]), &["entities"]).unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
    assert_eq!(err.error_code(), Some("SEM002"));
}

#[test]
fn test_normalize_graph_payload_errors_are_validation_errors() {
    let err = helpers::normalize_graph_payload(&json!({"data": [1]})).unwrap_err();
    assert!(err.is_validation_error());
    assert!(matches!(
        err,
        SemanticaError::Validation(ValidationError { .. })
    ));
}
