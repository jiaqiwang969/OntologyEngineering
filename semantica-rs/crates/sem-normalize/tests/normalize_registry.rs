//! Characterisation of `registry.MethodRegistry` / `method_registry` against the oracle.
//!
//! The registry is process-wide, so the whole trace runs in one test in its own test binary.

mod common;

use std::sync::Arc;

use sem_normalize::methods;
use sem_normalize::registry::NormalizeMethod;
use sem_normalize::{
    MethodRegistry, get_normalize_method, list_available_methods, method_registry,
};
use serde_json::{Map, Value, json};

use common::oracle;

fn upper() -> NormalizeMethod {
    Arc::new(|args: &[Value], _kwargs: &Map<String, Value>| {
        Ok(json!(args[0].as_str().unwrap_or_default().to_uppercase()))
    })
}

fn identity() -> NormalizeMethod {
    Arc::new(|args: &[Value], _kwargs: &Map<String, Value>| Ok(args[0].clone()))
}

#[test]
fn test_registry_trace_matches_oracle() {
    let expected = oracle("registry");
    let reg = MethodRegistry;

    assert_eq!(json!(method_registry.list_all(None)), expected["initial"]);
    reg.register("text", "upper", upper());
    reg.register(
        "custom_task",
        "noop",
        Arc::new(|_: &[Value], _: &Map<String, Value>| Ok(Value::Null)),
    );
    assert_eq!(
        json!(method_registry.list_all(None)),
        expected["after_register"]
    );
    assert_eq!(
        json!(method_registry.get("text", "upper").is_none()),
        expected["get_upper_is_none"]
    );
    assert_eq!(
        json!(method_registry.get("text", "missing").is_none()),
        expected["get_missing_is_none"]
    );
    assert_eq!(
        json!(method_registry.get("nope", "upper").is_none()),
        expected["get_unknown_task_is_none"]
    );
    assert_eq!(
        json!(method_registry.list_all(Some("text"))),
        expected["list_text"]
    );
    assert_eq!(
        json!(method_registry.list_all(Some("unknown"))),
        expected["list_unknown"]
    );
    assert_eq!(
        json!(method_registry.list_all(Some(""))),
        expected["list_empty_string"]
    );
    assert_eq!(
        json!({"ok": methods::normalize_text("hello", Some("upper"), &Map::new()).unwrap()}),
        expected["normalize_text_upper"]
    );
    assert_eq!(
        json!(get_normalize_method("text", "upper").is_none()),
        expected["get_normalize_method_is_none"]
    );

    method_registry.unregister("text", "upper");
    method_registry.unregister("text", "never-registered");
    method_registry.unregister("no-task", "x");
    assert_eq!(
        json!(method_registry.list_all(None)),
        expected["after_unregister"]
    );

    reg.register("text", "a", identity());
    reg.register("entity", "b", identity());
    method_registry.clear(Some("text"));
    assert_eq!(
        json!(method_registry.list_all(None)),
        expected["after_clear_text"]
    );
    method_registry.clear(Some("not-a-task"));
    method_registry.clear(None);
    assert_eq!(
        json!(method_registry.list_all(None)),
        expected["after_clear_all"]
    );
    assert_eq!(
        json!(list_available_methods(None)),
        expected["list_available_methods"]
    );

    // A failing custom method falls back to the default implementation (logged warning).
    reg.register(
        "text",
        "broken",
        Arc::new(|_: &[Value], _: &Map<String, Value>| {
            Err(sem_utils::SemanticaError::Value("boom".to_string()))
        }),
    );
    assert_eq!(
        methods::normalize_text("Hello   World", Some("broken"), &Map::new()).unwrap(),
        methods::normalize_text("Hello   World", None, &Map::new()).unwrap()
    );
    method_registry.clear(None);
}
