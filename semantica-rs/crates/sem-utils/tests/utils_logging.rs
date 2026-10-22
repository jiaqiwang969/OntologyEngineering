//! Characterisation of `semantica/utils/logging.py` and the `logging` shim it runs on
//! (expected values from the oracle). The tests share global loggers, so they run one at a
//! time.

mod common;

use std::sync::{Mutex, MutexGuard};

use common::{err_json, obj, oracle};
use sem_utils::logging::{
    DataQualityOptions, Formatter, GetLoggerOptions, Handler, LogLevel, LogRecord, Logger,
    get_logger_with,
};
use sem_utils::{
    SemanticaError, ValidationError, get_logger, log_data_quality, log_error, log_execution_time,
    log_performance, setup_logging,
};
use serde_json::{Map, Value, json};

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

const EXTRA_KEYS: &[&str] = &[
    "metrics",
    "error_type",
    "error_message",
    "context",
    "details",
    "traceback",
    "error_code",
    "quality_metrics",
    "threshold",
    "report",
];

/// Run `f` with a capture handler on `semantica.<name>` (level DEBUG, no propagation), like
/// the fixture generator's `captured()`.
fn captured(name: &str, f: impl FnOnce() -> Value) -> Value {
    let logger = get_logger(name);
    let capture = Handler::capture();
    logger.add_handler(capture.clone());
    logger.set_level(LogLevel::DEBUG);
    logger.set_propagate(false);
    let result = f();
    logger.remove_handler(&capture);
    logger.set_level(LogLevel::NOTSET);
    logger.set_propagate(true);
    let records: Vec<Value> = capture
        .records()
        .iter()
        .map(|r| {
            let mut extra = Map::new();
            for key in EXTRA_KEYS {
                if let Some(v) = r.extra.get(*key) {
                    extra.insert(key.to_string(), v.clone());
                }
            }
            if let Some(Value::Object(report)) = extra.get_mut("report") {
                report.insert("timestamp".into(), json!("<timestamp>"));
            }
            json!({"level": r.level.name(), "message": r.message, "extra": extra})
        })
        .collect();
    json!({"result": result, "records": records})
}

fn unit_outcome(result: Result<(), SemanticaError>) -> Value {
    match result {
        Ok(()) => json!({"ok": null}),
        Err(e) => err_json(&e),
    }
}

fn strip_case(case: &Value) -> Value {
    json!({"result": case["result"], "records": case["records"]})
}

#[test]
fn test_log_performance_matches_oracle() {
    let _guard = serial();
    for case in oracle("log_performance").as_array().unwrap() {
        let args = case["args"].as_array().unwrap();
        let name = args[0].as_str().unwrap();
        let time = args[1].as_f64().unwrap();
        let metrics = obj(&args[2]);
        let got = captured("performance", || {
            unit_outcome(log_performance(name, time, &metrics))
        });
        assert_eq!(got, strip_case(case), "log_performance{args:?}");
    }
}

#[test]
fn test_log_error_matches_oracle() {
    let _guard = serial();
    for case in oracle("log_error").as_array().unwrap() {
        let error: SemanticaError = match case["error"].as_str().unwrap() {
            "ValidationError" => ValidationError::with_details(
                "bad input",
                Some(obj(&json!({"row": 1}))),
                obj(&json!({"field": "name"})),
            )
            .into(),
            _ => {
                let message = match case["details"].get("level") {
                    Some(Value::String(_)) => "strlevel",
                    Some(_) => "warn",
                    None => "oops",
                };
                SemanticaError::Value(message.into())
            }
        };
        let context = case["context"].as_object().cloned();
        let details = obj(&case["details"]);
        let got = captured("error", || {
            unit_outcome(log_error(&error, context.as_ref(), &details))
        });
        assert_eq!(got, strip_case(case), "{case}");
    }
}

#[test]
fn test_log_data_quality_matches_oracle() {
    let _guard = serial();
    for case in oracle("log_data_quality").as_array().unwrap() {
        let metrics = obj(&case["metrics"]);
        let o = &case["options"];
        let options = DataQualityOptions {
            threshold: o["threshold"].as_f64().unwrap_or(0.7),
            alert_on_threshold: o["alert_on_threshold"].as_bool().unwrap_or(true),
            generate_report: o["generate_report"].as_bool().unwrap_or(false),
            ..DataQualityOptions::default()
        };
        let got = captured("quality", || {
            unit_outcome(log_data_quality(&metrics, &options))
        });
        assert_eq!(got, strip_case(case), "{case}");
    }
}

#[test]
fn test_log_execution_time_matches_oracle() {
    let _guard = serial();
    let fixture = oracle("log_execution_time");
    let ok = captured("performance", || {
        match log_execution_time("_ok_func", || Ok::<_, SemanticaError>(42)) {
            Ok(v) => json!({"ok": v}),
            Err(e) => err_json(&e),
        }
    });
    let bad = captured("performance", || {
        match log_execution_time("_bad_func", || {
            Err::<i32, _>(SemanticaError::Value("boom".into()))
        }) {
            Ok(v) => json!({"ok": v}),
            Err(e) => err_json(&e),
        }
    });
    for (mut got, key) in [(ok, "ok"), (bad, "bad")] {
        for record in got["records"].as_array_mut().unwrap() {
            let message = record["message"].as_str().unwrap().to_string();
            assert!(message.contains(" | Execution Time: "), "{message}");
            record["message"] = json!(message.split(" | Execution Time").next().unwrap());
        }
        assert_eq!(got, fixture[key], "{key}");
    }
}

fn fixed_record(message: &str) -> LogRecord {
    LogRecord {
        name: "semantica.test".into(),
        level: LogLevel::WARNING,
        message: message.into(),
        created: 1700000000.25,
        pathname: "/x/y/mod.py".into(),
        lineno: 42,
        thread_name: "MainThread".into(),
        thread: 1,
        process: 1,
        extra: Map::new(),
    }
}

#[test]
fn test_formatter_matches_oracle() {
    for case in oracle("formatter").as_array().unwrap() {
        let fmt = case["format"].as_str().unwrap();
        let formatter = Formatter::new(Some(fmt), None).unwrap();
        assert_eq!(
            formatter.format(&fixed_record("hello world")).unwrap(),
            case["expected"].as_str().unwrap(),
            "{fmt}"
        );
    }
    let outcomes: Vec<Value> = ["hello", "%(x)", "%(message)s", "%(x)q"]
        .iter()
        .map(|f| match Formatter::new(Some(f), None) {
            Ok(_) => json!({"ok": "valid"}),
            Err(e) => err_json(&e),
        })
        .collect();
    assert_eq!(Value::Array(outcomes), *oracle("formatter_invalid"));
    let missing = Formatter::new(Some("%(nope)s"), None)
        .unwrap()
        .format(&fixed_record("hello world"))
        .unwrap_err();
    assert_eq!(err_json(&missing), *oracle("formatter_missing_field"));
    // `%(asctime)s` is local time with milliseconds.
    let asctime = Formatter::new(Some("%(asctime)s"), None)
        .unwrap()
        .format(&fixed_record("x"))
        .unwrap();
    assert_eq!(asctime.len(), "2023-11-14 22:13:20,250".len());
    assert!(asctime.ends_with(",250"), "{asctime}");
}

fn reset_semantica_logger() {
    let root = Logger::get("semantica");
    root.clear_handlers();
    root.set_level(LogLevel::NOTSET);
    root.set_propagate(true);
}

#[test]
fn test_setup_logging_rotates_files_like_oracle() {
    let _guard = serial();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("logs").join("app.log");
    let config = obj(&json!({
        "level": "DEBUG",
        "format": "%(levelname)s:%(name)s:%(message)s",
        "file": path.to_string_lossy(),
        "console": false,
        "max_bytes": 60,
        "backup_count": 2,
    }));
    let root = setup_logging(Some(&config), &Map::new()).unwrap();
    assert_eq!(root.name(), "semantica");
    assert_eq!(root.level(), LogLevel::DEBUG);
    assert!(!root.propagate());
    let handlers = root.handlers();
    assert_eq!(
        handlers.iter().map(Handler::kind).collect::<Vec<_>>(),
        vec!["rotating_file"]
    );
    assert_eq!(handlers[0].level(), LogLevel::DEBUG);
    let logger = get_logger("x");
    for i in 0..6 {
        logger.info(format!("message number {i}"));
    }
    reset_semantica_logger();
    let dir = path.parent().unwrap();
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["app.log", "app.log.1", "app.log.2"]);
    assert_eq!(read("app.log"), "INFO:semantica.x:message number 5\n");
    assert_eq!(read("app.log.1"), "INFO:semantica.x:message number 4\n");
    assert_eq!(read("app.log.2"), "INFO:semantica.x:message number 3\n");
}

#[test]
fn test_setup_logging_options_match_oracle() {
    let _guard = serial();
    let tmp = tempfile::tempdir().unwrap();

    let kwargs = obj(&json!({"level": 10, "file": "", "console": true, "format": "%(message)s"}));
    let root = setup_logging(None, &kwargs).unwrap();
    assert_eq!(
        root.handlers()
            .iter()
            .map(Handler::kind)
            .collect::<Vec<_>>(),
        vec!["stdout"]
    );
    assert_eq!(root.level(), LogLevel(10));

    let plain = tmp.path().join("plain.log");
    let kwargs = obj(
        &json!({"level": "nonsense", "file": plain.to_string_lossy(), "rotation": false, "console": false}),
    );
    let root = setup_logging(None, &kwargs).unwrap();
    let handlers = root.handlers();
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].kind(), "file");
    assert_eq!(root.level(), LogLevel::INFO);
    assert_eq!(
        handlers[0].formatter().format_string(),
        "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
    );
    get_logger("y").warning("plain warning");
    let text = std::fs::read_to_string(&plain).unwrap();
    assert!(
        text.ends_with(" - semantica.y - WARNING - plain warning\n"),
        "{text}"
    );

    let kwargs = obj(&json!({"file": "", "console": false, "format": "no fields"}));
    let err = setup_logging(None, &kwargs).unwrap_err();
    assert_eq!(
        err_json(&err),
        json!({"error": "ValueError", "message": "Invalid format 'no fields' for '%' style"})
    );
    assert!(Logger::get("semantica").handlers().is_empty());

    let kwargs = obj(&json!({"file": "", "console": false, "level": 2.5}));
    let err = setup_logging(None, &kwargs).unwrap_err();
    assert_eq!(
        err_json(&err),
        json!({"error": "TypeError", "message": "Level not an integer or a valid string: 2.5"})
    );
    reset_semantica_logger();
}

#[test]
fn test_get_logger_names_and_options_match_oracle() {
    let _guard = serial();
    let logger = get_logger("names_test");
    assert_eq!(logger.name(), "semantica.names_test");
    // getLogger returns the same logger for the same name.
    logger.set_level(LogLevel::ERROR);
    assert_eq!(get_logger("names_test").level(), LogLevel::ERROR);
    logger.set_level(LogLevel::NOTSET);

    let opts = get_logger_with(
        "opts",
        Some(LogLevel::from_name("debug")),
        &GetLoggerOptions {
            propagate: Some(false),
            format: None,
        },
    )
    .unwrap();
    assert_eq!(opts.name(), "semantica.opts");
    assert_eq!(opts.level(), LogLevel(10));
    assert!(!opts.propagate());

    let fmt = get_logger_with(
        "fmt",
        None,
        &GetLoggerOptions {
            propagate: None,
            format: Some("%(name)s|%(message)s".into()),
        },
    )
    .unwrap();
    let handlers = fmt.handlers();
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].kind(), "stdout");
    assert_eq!(
        handlers[0].formatter().format_string(),
        "%(name)s|%(message)s"
    );
    fmt.clear_handlers();
}

#[test]
fn test_logger_and_handler_reprs_match_oracle() {
    let _guard = serial();
    // Oracle: repr(logging.getLogger("semantica.dbg")), repr(logging.getLogger()) and the
    // handler reprs.
    assert_eq!(
        format!("{:?}", Logger::get("semantica.dbg")),
        "<Logger semantica.dbg (WARNING)>"
    );
    assert_eq!(
        format!("{:?}", Logger::root()),
        "<RootLogger root (WARNING)>"
    );
    assert_eq!(
        format!("{:?}", Handler::stdout()),
        "<StreamHandler <stdout> (NOTSET)>"
    );
    assert_eq!(
        format!("{:?}", Handler::stderr()),
        "<StreamHandler <stderr> (NOTSET)>"
    );
    assert_eq!(
        format!("{:?}", Handler::writer(Box::new(std::io::sink()))),
        "<StreamHandler (NOTSET)>"
    );
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("probe.log");
    let handler = Handler::file(&path).unwrap();
    assert_eq!(
        format!("{handler:?}"),
        format!("<FileHandler {} (NOTSET)>", path.display())
    );
    assert_eq!(handler.path(), Some(path));
}

#[test]
fn test_logger_hierarchy_and_levels() {
    let _guard = serial();
    // Without configuration the root's WARNING level applies.
    let logger = get_logger("hierarchy.child");
    assert_eq!(logger.effective_level(), LogLevel::WARNING);
    assert!(!logger.is_enabled_for(LogLevel::INFO));
    let parent = Logger::get("semantica");
    let capture = Handler::capture();
    capture
        .set_formatter(Formatter::new(Some("%(levelname)s:%(name)s:%(message)s"), None).unwrap());
    parent.add_handler(capture.clone());
    parent.set_level(LogLevel::INFO);
    logger.debug("hidden");
    logger.info("shown");
    logger.warning("warned");
    logger.error("failed");
    logger.exception("exception");
    logger.critical("critical");
    logger.log(LogLevel(25), "custom");
    reset_semantica_logger();
    assert_eq!(
        capture.messages(),
        vec![
            "INFO:semantica.hierarchy.child:shown",
            "WARNING:semantica.hierarchy.child:warned",
            "ERROR:semantica.hierarchy.child:failed",
            "ERROR:semantica.hierarchy.child:exception",
            "CRITICAL:semantica.hierarchy.child:critical",
            "Level 25:semantica.hierarchy.child:custom",
        ]
    );
    assert_eq!(LogLevel::from_name("warn"), LogLevel::WARNING);
    assert_eq!(LogLevel::from_name("FATAL"), LogLevel::CRITICAL);
    assert_eq!(LogLevel::from("nonsense"), LogLevel::INFO);
    assert_eq!(LogLevel::from(30), LogLevel::WARNING);
    let err = logger
        .log_with_extra(LogLevel::INFO, "x", obj(&json!({"name": 1})))
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "\"Attempt to overwrite 'name' in LogRecord\""
    );
}
