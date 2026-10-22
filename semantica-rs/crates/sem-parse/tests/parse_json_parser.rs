//! Characterisation tests for `json_parser.rs` against the original (fixtures/core).

mod common;

use std::path::PathBuf;

use common::{assert_case, files_dir, load_oracle, oracle_form, quiet_progress};
use once_cell::sync::Lazy;
use sem_parse::{JSONParser, Options};
use serde_json::{Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| load_oracle("core"));

fn f(name: &str) -> String {
    files_dir("core").join(name).display().to_string()
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

fn check(key: &str, actual: Value) {
    assert_case(&ORACLE, "json", key, actual);
}

#[test]
fn test_parse_files_and_options() {
    quiet_progress();
    let p = JSONParser::default();
    check(
        "parse_file",
        oracle_form("core", p.parse(f("data.json"), &Options::new())),
    );
    check(
        "parse_nested",
        oracle_form("core", p.parse(f("nested.json"), &Options::new())),
    );
    check(
        "parse_flatten",
        oracle_form(
            "core",
            p.parse(f("nested.json"), &opts(json!({"flatten": true}))),
        ),
    );
    check(
        "parse_paths",
        oracle_form(
            "core",
            p.parse(f("nested.json"), &opts(json!({"extract_paths": true}))),
        ),
    );
    check(
        "parse_flatten_paths",
        oracle_form(
            "core",
            p.parse(
                f("nested.json"),
                &opts(json!({"flatten": true, "extract_paths": true})),
            ),
        ),
    );
    check(
        "latin1_file",
        oracle_form(
            "core",
            p.parse(f("latin1.json"), &opts(json!({"encoding": "latin-1"}))),
        ),
    );
    check(
        "crlf_file",
        oracle_form("core", p.parse(f("crlf.json"), &Options::new())),
    );
}

#[test]
fn test_parse_strings() {
    quiet_progress();
    let p = JSONParser::new(Options::new());
    let none = Options::new();
    check(
        "parse_string",
        oracle_form(
            "core",
            p.parse(r#"{"a": [1, {"b": 2}], "c": 1.0, "d": 1e5}"#, &none),
        ),
    );
    check(
        "parse_array_string",
        oracle_form("core", p.parse(r#"[{"x":1},{"x":2}]"#, &none)),
    );
    check(
        "parse_str_scalar",
        oracle_form("core", p.parse("\"hello\"", &none)),
    );
    check(
        "parse_int_scalar",
        oracle_form("core", p.parse("42", &none)),
    );
    check(
        "parse_bool_scalar",
        oracle_form("core", p.parse("true", &none)),
    );
    check(
        "parse_null_scalar",
        oracle_form("core", p.parse("null", &none)),
    );
    check(
        "parse_float_scalar",
        oracle_form("core", p.parse("1.5", &none)),
    );
}

#[test]
fn test_parse_errors() {
    quiet_progress();
    let p = JSONParser::default();
    let none = Options::new();
    check(
        "invalid_string",
        oracle_form("core", p.parse(r#"{"a": }"#, &none)),
    );
    check(
        "invalid_string2",
        oracle_form("core", p.parse("[1, 2,]", &none)),
    );
    check(
        "invalid_string3",
        oracle_form("core", p.parse("{\"a\": 1}\nextra", &none)),
    );
    check(
        "invalid_file",
        oracle_form("core", p.parse(f("invalid.json"), &none)),
    );
    check(
        "missing_path",
        oracle_form("core", p.parse(PathBuf::from(f("missing.json")), &none)),
    );
    check(
        "missing_str",
        oracle_form("core", p.parse(f("missing.json"), &none)),
    );
    check("empty_string", oracle_form("core", p.parse("", &none)));
    let long = format!("{{\"k\": \"{}\"}}", "v".repeat(2000));
    let actual = oracle_form("core", p.parse(long.as_str(), &none));
    // The message embeds the whole 2000-character name; compare it in full.
    check("long_string", actual);
    check(
        "bom_file",
        oracle_form("core", p.parse(f("bom.json"), &none)),
    );
    check(
        "badutf8_file",
        oracle_form("core", p.parse(f("badutf8.json"), &none)),
    );
    check(
        "unknown_encoding",
        oracle_form(
            "core",
            p.parse(f("data.json"), &opts(json!({"encoding": "nope"}))),
        ),
    );
}

#[test]
fn test_parse_to_dict_and_list() {
    quiet_progress();
    let p = JSONParser::default();
    let none = Options::new();
    check(
        "to_dict_object",
        oracle_form("core", p.parse_to_dict(f("data.json"), &none)),
    );
    check(
        "to_dict_array1",
        oracle_form("core", p.parse_to_dict(f("array1.json"), &none)),
    );
    check(
        "to_dict_array2",
        oracle_form("core", p.parse_to_dict(f("array2.json"), &none)),
    );
    check(
        "to_dict_ints",
        oracle_form("core", p.parse_to_dict(f("ints.json"), &none)),
    );
    check(
        "to_dict_scalar",
        oracle_form("core", p.parse_to_dict(f("scalar.json"), &none)),
    );
    check(
        "to_dict_flat_array",
        oracle_form(
            "core",
            p.parse_to_dict(f("array2.json"), &opts(json!({"flatten": true}))),
        ),
    );
    check(
        "to_list_array",
        oracle_form("core", p.parse_to_list(f("ints.json"), &none)),
    );
    check(
        "to_list_object",
        oracle_form("core", p.parse_to_list(f("data.json"), &none)),
    );
    check(
        "to_list_scalar",
        oracle_form("core", p.parse_to_list(f("scalar.json"), &none)),
    );
    check(
        "to_list_flat_array",
        oracle_form(
            "core",
            p.parse_to_list(f("array2.json"), &opts(json!({"flatten": true}))),
        ),
    );
}

#[test]
fn test_extract_values() {
    quiet_progress();
    let p = JSONParser::default();
    let none = Options::new();
    check(
        "values_user_name",
        oracle_form(
            "core",
            p.extract_values(f("nested.json"), "user.name", &none),
        ),
    );
    check(
        "values_items_name",
        oracle_form(
            "core",
            p.extract_values(f("nested.json"), "items.name", &none),
        ),
    );
    check(
        "values_missing",
        oracle_form(
            "core",
            p.extract_values(f("nested.json"), "user.missing", &none),
        ),
    );
    check(
        "values_tags",
        oracle_form(
            "core",
            p.extract_values(f("nested.json"), "user.tags", &none),
        ),
    );
    check(
        "values_empty_path",
        oracle_form("core", p.extract_values("{\"\": 1}", "", &none)),
    );
}

#[test]
fn test_json_data_fields_and_tracker() {
    let p = JSONParser::default();
    assert!(p.progress_tracker.enabled());
    assert_eq!(p.logger.name(), "semantica.json_parser");
    let data = p.parse("[1]", &Options::new()).unwrap();
    assert_eq!(data.r#type, "array");
    assert_eq!(data.data, json!([1]));
}
