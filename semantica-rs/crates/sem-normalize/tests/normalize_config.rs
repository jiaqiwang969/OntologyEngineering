//! Characterisation of `config.NormalizeConfig` / `normalize_config` against the oracle.
//!
//! The method-configuration test mutates the global `normalize_config`, so this file is its
//! own test binary.

mod common;

use sem_normalize::{NormalizeConfig, methods, normalize_config};
use serde_json::{Map, Value, json};

use common::{kw, oracle};

#[test]
fn test_config_files_match_oracle() {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in oracle("config_files").as_object().unwrap() {
        std::fs::write(dir.path().join(name), content.as_str().unwrap()).unwrap();
    }
    for case in oracle("config_load").as_array().unwrap() {
        let path = dir.path().join(case["file"].as_str().unwrap());
        let config = NormalizeConfig::new(Some(path.to_str().unwrap()));
        assert_eq!(
            json!({"ok": config.get_all()}),
            case["result"],
            "case {case}"
        );
    }
    assert_eq!(
        json!({"ok": NormalizeConfig::new(None).get_all()}),
        *oracle("config_none")
    );
}

#[test]
fn test_config_operations_match_oracle() {
    let expected = oracle("config_ops");
    let config = NormalizeConfig::default();
    config.set("unicode_form", "NFKD");
    config.set("count", 3);
    config.set_method_config("text", kw(json!({"case": "lower", "unicode_form": "NFC"})));
    config.set_method_config("date", Map::new());
    assert_eq!(
        json!({"ok": config.get("unicode_form", Value::Null)}),
        expected["get_set"]
    );
    assert_eq!(
        json!({"ok": config.get("count", json!(0))}),
        expected["get_count"]
    );
    assert_eq!(
        json!({"ok": config.get("missing", Value::Null)}),
        expected["get_missing"]
    );
    assert_eq!(config.get_value("missing"), None);
    assert_eq!(
        json!({"ok": config.get("missing", json!(5))}),
        expected["get_missing_default"]
    );
    assert_eq!(
        json!({"ok": config.get("missing", json!("dflt"))}),
        expected["get_missing_default_str"]
    );
    assert_eq!(
        json!({"ok": config.get_method_config("text")}),
        expected["method_text"]
    );
    assert_eq!(
        json!({"ok": config.get_method_config("date")}),
        expected["method_date"]
    );
    assert_eq!(
        json!({"ok": config.get_method_config("nope")}),
        expected["method_missing"]
    );
    assert_eq!(json!({"ok": config.get_all()}), expected["all"]);
}

#[test]
fn test_method_config_persists_keyword_arguments_like_oracle() {
    let expected = oracle("method_config_mutation");
    normalize_config.set_method_config("number", kw(json!({"precision": 2})));
    assert_eq!(
        json!(normalize_config.get_method_config("number")),
        expected["before"]
    );
    let call = methods::normalize_number("5", None, &kw(json!({"foo": 1}))).unwrap();
    assert_eq!(json!({"ok": call.to_value()}), expected["call"]);
    assert_eq!(
        json!(normalize_config.get_method_config("number")),
        expected["after"]
    );

    assert_eq!(
        json!(normalize_config.get_method_config("entity")),
        expected["absent_before"]
    );
    let call = methods::normalize_entity("x", None, None, &kw(json!({"bar": 2}))).unwrap();
    assert_eq!(json!({"ok": call}), expected["absent_call"]);
    assert_eq!(
        json!(normalize_config.get_method_config("entity")),
        expected["absent_after"]
    );
}
