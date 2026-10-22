//! Characterisation of `semantica/utils/exceptions.py` (expected values from the oracle).

mod common;

use common::{obj, oracle};
use sem_utils::exceptions::{
    ErrorInfo, FormatExceptionOptions, OsError, TEMPORAL_AMBIGUITY_WARNING,
    TemporalAmbiguityWarning,
};
use sem_utils::logging::{Handler, LogLevel, get_logger};
use sem_utils::pycompat::warnings::{USER_WARNING, catch_warnings, warn};
use sem_utils::{
    ConfigurationError, ProcessingError, QualityError, SemanticaError, TemporalValidationError,
    ValidationError, format_exception, handle_exception,
};
use serde_json::{Map, Value, json};

fn map(value: Value) -> Map<String, Value> {
    obj(&value)
}

/// The exceptions of the fixture's `exceptions` section, built the same way.
fn fixture_exceptions() -> Vec<SemanticaError> {
    vec![
        SemanticaError::new("base"),
        SemanticaError::with_details(
            "ctx",
            Some(map(json!({"stage": "load", "n": 2}))),
            map(json!({"error_code": "X9", "extra": [1, "a"]})),
        ),
        ValidationError::with_details(
            "bad",
            None,
            map(json!({"field": "name", "value": null, "constraint": "required"})),
        )
        .into(),
        ValidationError::with_context("bad", map(json!({"row": 3}))).into(),
        TemporalValidationError::with_details(
            "tbad",
            Some(map(json!({"a": 1}))),
            map(json!({"x": [1, "b"]})),
        )
        .into(),
        ProcessingError::with_details(
            "proc",
            Some(map(json!({"s": "x"}))),
            map(json!({"stage": "parse", "input_data": {"k": 1}})),
        )
        .into(),
        ConfigurationError::new("cfg")
            .detail("config_key", "k")
            .detail("config_value", 3)
            .detail("expected_type", "str")
            .into(),
        QualityError::new("qual")
            .detail("quality_score", 0.4)
            .detail("threshold", 0.7)
            .into(),
        QualityError::new("qual2")
            .detail("metrics", json!({"m": 1}))
            .into(),
    ]
}

fn attrs(err: &SemanticaError) -> Value {
    let mut out = Map::new();
    let opt = |v: Option<&Value>| v.cloned().unwrap_or(Value::Null);
    match err {
        SemanticaError::Validation(e) => {
            out.insert("field".into(), opt(e.field()));
            out.insert("value".into(), opt(e.value()));
            out.insert("constraint".into(), opt(e.constraint()));
            out.insert(
                "validation_context".into(),
                Value::Object(e.validation_context().clone()),
            );
        }
        SemanticaError::TemporalValidation(e) => {
            out.insert("field".into(), opt(e.field()));
            out.insert("value".into(), opt(e.value()));
            out.insert("constraint".into(), opt(e.constraint()));
            out.insert(
                "validation_context".into(),
                Value::Object(e.validation_context().clone()),
            );
        }
        SemanticaError::Processing(e) => {
            out.insert("stage".into(), opt(e.stage()));
            out.insert("input_data".into(), opt(e.input_data()));
            out.insert("output_data".into(), opt(e.output_data()));
            out.insert(
                "processing_context".into(),
                Value::Object(e.processing_context().clone()),
            );
        }
        SemanticaError::Configuration(e) => {
            out.insert("config_key".into(), opt(e.config_key()));
            out.insert("config_value".into(), opt(e.config_value()));
            out.insert("expected_type".into(), opt(e.expected_type()));
            out.insert(
                "config_context".into(),
                Value::Object(e.config_context().clone()),
            );
        }
        SemanticaError::Quality(e) => {
            out.insert("quality_score".into(), opt(e.quality_score()));
            out.insert("threshold".into(), opt(e.threshold()));
            out.insert("metrics".into(), e.metrics());
            out.insert(
                "quality_context".into(),
                Value::Object(e.quality_context().clone()),
            );
        }
        _ => {}
    }
    Value::Object(out)
}

fn keys(value: &Value) -> Vec<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

fn no_log() -> Map<String, Value> {
    map(json!({"log_error": false}))
}

#[test]
fn test_exception_classes_match_oracle() {
    let cases = oracle("exceptions").as_array().unwrap();
    let errors = fixture_exceptions();
    assert_eq!(cases.len(), errors.len());
    for (case, err) in cases.iter().zip(&errors) {
        let class = case["class"].as_str().unwrap();
        assert_eq!(err.type_name(), class);
        assert!(err.is_semantica_error());
        let info: &ErrorInfo = err.info().unwrap();
        assert_eq!(info.message, case["message"].as_str().unwrap(), "{class}");
        assert_eq!(
            Value::Object(info.context.clone()),
            case["context"],
            "{class}"
        );
        assert_eq!(
            Value::Object(info.details.clone()),
            case["details"],
            "{class}"
        );
        assert_eq!(
            keys(&Value::Object(info.details.clone())),
            keys(&case["details"])
        );
        assert_eq!(err.error_code(), case["error_code"].as_str(), "{class}");
        assert_eq!(err.message(), info.message);
        assert_eq!(err.context(), Some(&info.context));
        assert_eq!(err.details(), Some(&info.details));
        assert_eq!(
            err.to_string(),
            case["str"].as_str().unwrap(),
            "{class} str"
        );
        assert_eq!(
            format!("{err:?}"),
            case["repr"].as_str().unwrap(),
            "{class} repr"
        );
        assert_eq!(
            Value::Object(err.to_dict().unwrap()),
            case["to_dict"],
            "{class} to_dict"
        );
        assert_eq!(attrs(err), case["attrs"], "{class} attributes");
    }
}

#[test]
fn test_subclass_structs_expose_their_state() {
    let e = ValidationError::new("bad").detail("field", "name");
    assert_eq!(e.message(), "bad");
    assert_eq!(e.error_code(), "SEM001");
    assert!(e.context().is_empty());
    assert_eq!(e.details()["field"], json!("name"));
    assert_eq!(e.info().message, "bad");
    assert_eq!(e.to_dict()["error_type"], json!("ValidationError"));
    assert_eq!(e.to_string(), "bad | Details: field=name");
    let t = TemporalValidationError::new("t");
    assert_eq!(t.error_code(), "SEM001T");
    assert_eq!(t.details()["error_code"], json!("SEM001"));
    let as_enum: SemanticaError = t.into();
    assert!(as_enum.is_validation_error());
    assert_eq!(ProcessingError::new("p").error_code(), "SEM002");
    assert_eq!(ConfigurationError::new("c").error_code(), "SEM003");
    assert_eq!(QualityError::new("q").error_code(), "SEM004");
    assert_eq!(QualityError::new("q").metrics(), json!({}));
}

#[test]
fn test_format_exception_matches_oracle() {
    let cases = oracle("exceptions").as_array().unwrap();
    for (case, err) in cases.iter().zip(fixture_exceptions()) {
        let class = case["class"].as_str().unwrap();
        let fmt = |opts: FormatExceptionOptions| format_exception(&err, &opts);
        assert_eq!(
            fmt(FormatExceptionOptions::default()),
            case["format_default"].as_str().unwrap(),
            "{class}"
        );
        assert_eq!(
            fmt(FormatExceptionOptions {
                include_context: false,
                ..Default::default()
            }),
            case["format_no_context"].as_str().unwrap(),
            "{class}"
        );
        assert_eq!(
            fmt(FormatExceptionOptions {
                include_traceback: true,
                ..Default::default()
            }),
            case["format_traceback"].as_str().unwrap(),
            "{class}"
        );
        for (max, key) in [
            (20, "format_max_20"),
            (-5, "format_max_neg"),
            (0, "format_max_0"),
        ] {
            assert_eq!(
                fmt(FormatExceptionOptions {
                    max_length: Some(max),
                    ..Default::default()
                }),
                case[key].as_str().unwrap(),
                "{class} {key}"
            );
        }
    }
}

#[test]
fn test_handle_exception_matches_oracle() {
    let cases = oracle("exceptions").as_array().unwrap();
    for (case, err) in cases.iter().zip(fixture_exceptions()) {
        let class = case["class"].as_str().unwrap();
        let caller = map(json!({"caller": "x"}));
        let handled = Value::Object(handle_exception(&err, Some(&caller), &no_log()).unwrap());
        assert_eq!(handled, case["handle"], "{class}");
        assert_eq!(keys(&handled), keys(&case["handle"]), "{class}");
        let opts = map(
            json!({"log_error": false, "include_traceback": false, "recovery_action": "retry"}),
        );
        let handled = Value::Object(handle_exception(&err, None, &opts).unwrap());
        assert_eq!(handled, case["handle_no_tb"], "{class}");
    }
}

fn builtin_errors() -> Vec<SemanticaError> {
    vec![
        SemanticaError::Value("vv".into()),
        SemanticaError::Type("tt".into()),
        SemanticaError::Key("id".into()),
        SemanticaError::Value(String::new()),
        SemanticaError::Runtime("rr".into()),
        SemanticaError::Os(OsError {
            type_name: "FileNotFoundError".into(),
            errno: Some(2),
            text: "No such file or directory".into(),
            filename: Some("missing.txt".into()),
        }),
        SemanticaError::Os(OsError::new("FileNotFoundError", "File not found: x")),
    ]
}

#[test]
fn test_builtin_exceptions_match_oracle() {
    let cases = oracle("builtin_exceptions").as_array().unwrap();
    for (case, err) in cases.iter().zip(builtin_errors()) {
        let ty = case["type"].as_str().unwrap();
        assert_eq!(err.type_name(), ty);
        assert!(!err.is_semantica_error());
        assert_eq!(err.error_code(), None);
        assert_eq!(err.to_dict(), None);
        assert_eq!(err.to_string(), case["str"].as_str().unwrap(), "{ty}");
        assert_eq!(format!("{err:?}"), case["repr"].as_str().unwrap(), "{ty}");
        assert_eq!(
            format_exception(
                &err,
                &FormatExceptionOptions {
                    include_traceback: true,
                    ..Default::default()
                }
            ),
            case["format"].as_str().unwrap(),
            "{ty}"
        );
        assert_eq!(
            Value::Object(handle_exception(&err, None, &no_log()).unwrap()),
            case["handle"],
            "{ty}"
        );
    }
}

#[test]
fn test_handle_exception_logs_through_log_error() {
    let logger = get_logger("error");
    let capture = Handler::capture();
    logger.add_handler(capture.clone());
    let err: SemanticaError = ValidationError::new("bad input").into();
    let info = handle_exception(&err, None, &Map::new()).unwrap();
    logger.remove_handler(&capture);
    assert_eq!(info["recovery"], json!("log_and_continue"));
    let records = capture.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].level, LogLevel::ERROR);
    assert_eq!(records[0].message, "ValidationError: bad input");
    assert_eq!(records[0].extra["details"]["recovery_action"], Value::Null);
    assert_eq!(records[0].extra["error_code"], json!("SEM001"));

    // A non-integer `level` option fails inside log_error, as in Python.
    let bad = map(json!({"level": "ERROR"}));
    let err = handle_exception(&err, None, &bad).unwrap_err();
    assert_eq!(err.type_name(), "TypeError");
}

#[test]
fn test_io_errors_convert_to_os_errors() {
    let io = std::io::Error::from_raw_os_error(2);
    let err: SemanticaError = io.into();
    assert_eq!(err.type_name(), "FileNotFoundError");
    assert_eq!(err.to_string(), "[Errno 2] No such file or directory");
    assert_eq!(
        format!("{err:?}"),
        "FileNotFoundError(2, 'No such file or directory')"
    );
}

#[test]
fn test_temporal_ambiguity_warning_is_a_user_warning() {
    let ((), caught) = catch_warnings(|| {
        warn(
            "Temporal expression '03/04/2022' is ambiguous (day/month ordering unknown).",
            TemporalAmbiguityWarning,
        );
    });
    assert_eq!(caught.len(), 1);
    assert_eq!(caught[0].category.name(), "TemporalAmbiguityWarning");
    assert!(caught[0].category.is_subclass_of(TemporalAmbiguityWarning));
    assert!(caught[0].category.is_subclass_of(USER_WARNING));
    assert!(caught[0].message.to_lowercase().contains("ambiguous"));
    assert_eq!(TEMPORAL_AMBIGUITY_WARNING.parent(), Some(&USER_WARNING));
}
