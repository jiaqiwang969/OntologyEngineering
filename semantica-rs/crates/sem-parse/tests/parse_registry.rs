//! Characterisation tests for `registry.rs` against the original (fixtures/core). The
//! registry is process-wide, so the whole sequence runs in one test.

mod common;

use common::load_oracle;
use sem_parse::methods::ParseOutput;
use sem_parse::registry::{MethodArg, parse_method};
use sem_parse::{MethodRegistry, Options, list_available_methods, method_registry};
use serde_json::{Value, json};

fn listing(m: indexmap::IndexMap<String, Vec<String>>) -> Value {
    serde_json::to_value(m).unwrap()
}

#[test]
fn test_registry_sequence_matches_original() {
    let oracle = load_oracle("core");
    let exp = &oracle["registry"];
    assert_eq!(listing(list_available_methods(None)), exp["initial"]);
    assert_eq!(
        listing(list_available_methods(Some("document"))),
        exp["task"]
    );
    assert_eq!(
        listing(list_available_methods(Some("nope"))),
        exp["unknown_task"]
    );
    assert_eq!(listing(list_available_methods(Some(""))), exp["empty_task"]);

    let other = MethodRegistry::new();
    other.register(
        "document",
        "mine",
        parse_method(|_args: &[MethodArg], _kw: &Options| {
            Ok(ParseOutput::Value(json!({"mine": true})))
        }),
    );
    other.register(
        "newtask",
        "m1",
        parse_method(|_a, _k| Ok(ParseOutput::Value(Value::Null))),
    );
    assert_eq!(
        listing(method_registry.list_all(None)),
        exp["after_register"]
    );
    assert_eq!(
        json!(method_registry.get("document", "mine").is_some()),
        exp["get_mine"]
    );
    assert_eq!(
        json!(method_registry.get("document", "zzz").is_none()),
        exp["get_missing"]
    );
    assert_eq!(
        json!(method_registry.get("zzz", "default").is_none()),
        exp["get_missing_task"]
    );
    let out = method_registry.get("document", "mine").unwrap()(&[], &Options::new()).unwrap();
    assert_eq!(out.to_value(), json!({"mine": true}));

    method_registry.unregister("document", "mine");
    method_registry.unregister("document", "never");
    method_registry.unregister("never", "x");
    assert_eq!(
        listing(method_registry.list_all(None)),
        exp["after_unregister"]
    );

    let saved: Vec<(String, String, _)> = method_registry
        .list_all(None)
        .into_iter()
        .flat_map(|(t, names)| {
            names
                .into_iter()
                .map(move |n| (t.clone(), n.clone()))
                .collect::<Vec<_>>()
        })
        .map(|(t, n)| {
            let m = method_registry.get(&t, &n).unwrap();
            (t, n, m)
        })
        .collect();
    method_registry.clear(Some("web"));
    assert_eq!(
        listing(method_registry.list_all(None)),
        exp["after_clear_web"]
    );
    method_registry.clear(None);
    assert_eq!(
        listing(method_registry.list_all(None)),
        exp["after_clear_all"]
    );
    for (t, n, m) in saved {
        if t != "newtask" {
            method_registry.register(&t, &n, m);
        }
    }
    // `newtask` stays as an empty task here (the original deletes the key directly).
    assert_eq!(
        format!("{:?}", MethodRegistry::default()).contains("document"),
        true
    );
    let restored = listing(method_registry.list_all(None));
    let mut expected = exp["restored"].clone();
    expected
        .as_object_mut()
        .unwrap()
        .insert("newtask".to_string(), json!([]));
    assert_eq!(restored, expected);
}
