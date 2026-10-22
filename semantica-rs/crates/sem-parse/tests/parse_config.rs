//! Characterisation tests for `config.rs` against the original (fixtures/core).

mod common;

use common::{files_dir, load_oracle};
use once_cell::sync::Lazy;
use sem_parse::{Options, ParseConfig, parse_config};
use serde_json::{Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| load_oracle("core"));

fn f(name: &str) -> String {
    files_dir("core").join(name).display().to_string()
}

fn expected(key: &str) -> &'static Value {
    &ORACLE["config"][key]
}

fn all(c: &ParseConfig) -> Value {
    Value::Object(c.get_all())
}

fn no_env() -> Vec<(String, String)> {
    Vec::new()
}

#[test]
fn test_config_files() {
    for (key, file) in [
        ("yaml", "data.yaml"),
        ("json", "config.json"),
        ("toml", "config.toml"),
        ("bad_yaml", "bad.yaml"),
        ("list_yaml", "list.yaml"),
        ("missing", "nope.yaml"),
    ] {
        let c = ParseConfig::with_environment(Some(&f(file)), no_env());
        assert_eq!(&all(&c), expected(key), "config file case {key}");
    }
    let c = ParseConfig::with_environment(Some(&f("data.yaml")), no_env());
    assert_eq!(
        &Value::Object(c.get_method_config("document")),
        expected("yaml_get_method")
    );
}

#[test]
fn test_environment() {
    let env: Vec<(String, String)> = [
        ("PARSE_EXTRACT_TABLES", "Yes"),
        ("PARSE_OCR_LANGUAGE", "fra"),
        ("PARSE_MAX_PAGES", "12"),
        ("PARSE_THRESHOLD", "0.75"),
        ("PARSE_FLAG", "TRUE"),
        ("PARSE_NAME", "custom"),
        ("PARSE_EMPTY", ""),
        ("PARSE_EXP", "1e3"),
        ("PARSE_NEG", "-5"),
        ("OTHER", "x"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let c = ParseConfig::with_environment(None, env.clone());
    assert_eq!(&all(&c)["config"], expected("env"));

    let mut later = env.clone();
    later.extend(
        [
            ("PARSE_TIMEOUT", "30"),
            ("PARSE_RATIO", "2.5"),
            ("PARSE_ON", "1"),
            ("PARSE_WORD", "yes"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string())),
    );
    let lookup_first = |name: &str| env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    let lookup_later = |name: &str| {
        later
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let got = json!({
        "configured": c.get_with_environment("max_pages", Value::Null, lookup_first),
        "missing_default": c.get_with_environment("missing", json!("dflt"), lookup_first),
        "missing_none": c.get_with_environment("missing", Value::Null, lookup_first),
        "int_default": c.get_with_environment("timeout", json!(5), lookup_later),
        "float_default": c.get_with_environment("ratio", json!(1.0), lookup_later),
        "bool_default_digit": c.get_with_environment("on", json!(false), lookup_later),
        "bool_default_word": c.get_with_environment("word", json!(false), lookup_later),
        "str_default": c.get_with_environment("word", json!("x"), lookup_later),
        "int_default_bad": c.get_with_environment("word", json!(7), lookup_later),
    });
    assert_eq!(&got, expected("env_get"));
}

#[test]
fn test_programmatic_config() {
    let c = ParseConfig::with_environment(None, no_env());
    c.set("default_encoding", json!("ascii"));
    let mut doc = Options::new();
    doc.insert("extract_tables".into(), json!(false));
    doc.insert("pages".into(), json!([1, 2]));
    c.set_method_config("document", doc);
    assert_eq!(&c.get("default_encoding", Value::Null), expected("api_get"));
    assert_eq!(
        &Value::Object(c.get_method_config("document")),
        expected("api_method")
    );
    assert_eq!(
        &Value::Object(c.get_method_config("web")),
        expected("api_method_missing")
    );
    assert_eq!(&all(&c), expected("api_all"));
    // The global instance exists and is usable.
    assert!(parse_config.get_all().contains_key("config"));
    let _ = ParseConfig::new(None);
}
