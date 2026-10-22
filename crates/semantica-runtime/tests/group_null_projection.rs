//! Expectations derive from immutable original15 and explicit new scope guards.
use semantica_contracts::{
    SemanticQueryOptionsPortV4, SemanticQueryOptionsRequestV2, SemanticQueryOptionsRequestV3,
    SemanticQueryOptionsRequestV4,
};
use semantica_runtime::{graph, runtime::RustSemanticRuntime};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const FIXTURE: &str = include_str!("fixtures/group-variable-alias-original15.json");
fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}
fn request(input: &Value) -> Value {
    json!({"schema":"semantica.query-options-request/4","profile":"rdflib760-group-null-projection/4",
        "data":input["data"],"data_sha256":semantica_runtime::sha256(input["data"].as_str().unwrap().as_bytes()),
        "rdf_format":input["rdf_format"],"query":input["query"],"namespaces":input["namespaces"],"bindings":input["bindings"],
        "budget":{"data_bytes":16777216,"query_bytes":1048576,"parameter_count":128,"parameter_bytes":16777216,
            "quads":100000,"rows":10000,"triples":100000,"ast_depth":96,"ast_nodes":100000,"stack_bytes":8388608},
        "guard":{"schema":"semantica.query-shadow-guard/3","assignment_scope":"external_initial_targets_only",
            "query_scope":"genuine_parser_declarations","initial_context":"immutable_exact_terms","computed_scope":"marked_tuple_shadow",
            "project_scope":"selected_tuple_domain","dataset_scope":"same_owner_dataset","group_guard":"aggregate_target_refused",
            "remote_guard":"service_from_refused"}})
}
fn typed(v: &Value) -> SemanticQueryOptionsRequestV4 {
    serde_json::from_value(v.clone()).unwrap()
}
fn unbound_request() -> Value {
    let f = fixture();
    request(
        &f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["input"]["id"] == "unbound_variable_group_alias")
            .unwrap()["input"],
    )
}
fn n3(v: &Value) -> String {
    match v["kind"].as_str().unwrap() {
        "iri" => format!("<{}>", v["value"].as_str().unwrap()),
        "blank" => format!("_:{}", v["value"].as_str().unwrap()),
        "literal" => {
            let s = serde_json::to_string(v["value"].as_str().unwrap()).unwrap();
            if let Some(lang) = v["language"].as_str() {
                format!("{s}@{lang}")
            } else if let Some(dt) = v["datatype"]
                .as_str()
                .filter(|d| *d != "http://www.w3.org/2001/XMLSchema#string")
            {
                format!("{s}^^<{dt}>")
            } else {
                s
            }
        }
        _ => panic!("unexpected RDF term"),
    }
}
fn bag(rows: &Value, original: bool) -> Vec<String> {
    let mut result = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let values: BTreeMap<_, _> = row
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, t)| {
                    (
                        k.clone(),
                        if t.is_null() {
                            Value::Null
                        } else if original {
                            Value::String(t["n3"].as_str().unwrap().to_owned())
                        } else {
                            Value::String(n3(t))
                        },
                    )
                })
                .collect();
            serde_json::to_string(&values).unwrap()
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}
#[test]
fn group_null_original15_preserves_exact_null_and_declared_residuals() {
    assert_eq!(
        semantica_runtime::sha256(FIXTURE.as_bytes()),
        "ee8670c12b2c257ee05310c54102150055cbf5ec08191166fb737c8ecf823596"
    );
    let mut matched = 0;
    for c in fixture()["cases"].as_array().unwrap() {
        let id = c["input"]["id"].as_str().unwrap();
        let result = RustSemanticRuntime.query_with_options_v4(&typed(&request(&c["input"])));
        if [
            "query_target_redeclared",
            "duplicate_alias_target",
            "self_alias_query_target",
        ]
        .contains(&id)
        {
            assert!(
                result
                    .unwrap_err()
                    .message
                    .contains("GROUP BY alias overrides an existing query variable")
            );
        } else if id == "nested_group_variable_alias" {
            let e = result.unwrap_err();
            assert_eq!(e.code, "query_execution_failed");
            assert!(
                e.message
                    .contains("query_group_null_projection_scope_unsupported")
            );
        } else {
            let actual = result.unwrap().query_result.result;
            if id == "empty_group_alias_unresolved_control" {
                assert_eq!(c["original"]["rows"], json!([{}]));
                assert_eq!(actual["row_count"], 0);
                assert_eq!(actual["rows"], json!([]));
                continue;
            }
            assert_eq!(actual["columns"], c["original"]["columns"], "{id}");
            assert_eq!(actual["row_count"], c["original"]["row_count"], "{id}");
            assert_eq!(
                bag(&actual["rows"], false),
                bag(&c["original"]["rows"], true),
                "{id}"
            );
            if id == "unbound_variable_group_alias" {
                assert!(actual["rows"][0].as_object().unwrap().contains_key("group"));
                assert_eq!(actual["rows"][0]["group"], Value::Null);
                assert_eq!(actual["rows"][0]["count"]["value"], "3");
            }
            matched += 1;
        }
    }
    assert_eq!(matched, 10);
}
#[test]
fn group_null_scope_does_not_invent_missing_columns_or_rows() {
    for (query, expected_rows) in [
        ("SELECT ?missing ?s WHERE { ?s ex:tag ?tag }", 3),
        (
            "SELECT (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag } GROUP BY (?missing AS ?group)",
            1,
        ),
        (
            "SELECT ?group (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag } GROUP BY (?missing AS ?group) LIMIT 0",
            0,
        ),
    ] {
        let mut v = unbound_request();
        v["query"] = query.into();
        let a = RustSemanticRuntime
            .query_with_options_v4(&typed(&v))
            .unwrap()
            .query_result
            .result;
        assert_eq!(a["row_count"], expected_rows);
        for row in a["rows"].as_array().unwrap() {
            assert!(!row.as_object().unwrap().contains_key("missing"));
            assert!(!row.as_object().unwrap().contains_key("group"));
        }
        if query.ends_with("LIMIT 0") {
            assert_eq!(a["row_count"], 0);
        }
    }
    for query in [
        "SELECT ?group ?count WHERE { { SELECT ?group (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag } GROUP BY (?missing AS ?group) } }",
        "SELECT ?group ?count ?other WHERE { { SELECT ?group (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag } GROUP BY (?missing AS ?group) } UNION { BIND(\"outside\" AS ?other) } }",
        "SELECT ?seed ?group ?count WHERE { VALUES ?seed { 1 } OPTIONAL { { SELECT ?group (COUNT(?tag) AS ?count) WHERE { ?s ex:absent ?tag } GROUP BY (?missing AS ?group) } } }",
        "SELECT ?seed ?group ?count WHERE { VALUES ?seed { 1 } { SELECT ?group (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag } GROUP BY (?missing AS ?group) } }",
        "SELECT (?group AS ?copy) (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag } GROUP BY (?missing AS ?group)",
        "SELECT ?group (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag } GROUP BY (?missing AS ?group) HAVING(BOUND(?group))",
    ] {
        let mut v = unbound_request();
        v["query"] = query.into();
        let e = RustSemanticRuntime
            .query_with_options_v4(&typed(&v))
            .unwrap_err();
        assert_eq!(e.code, "query_execution_failed", "{query}");
        assert!(
            e.message
                .contains("query_group_null_projection_scope_unsupported"),
            "{query}: {e:?}"
        );
    }
    // Ordinary OPTIONAL within the input of the one final Group is supported.
    let mut v = unbound_request();
    v["query"]="SELECT ?group (COUNT(?tag) AS ?count) WHERE { ?s ex:tag ?tag OPTIONAL { ?s ex:absent ?unused } } GROUP BY (?missing AS ?group)".into();
    let a = RustSemanticRuntime
        .query_with_options_v4(&typed(&v))
        .unwrap()
        .query_result
        .result;
    assert_eq!(a["rows"][0]["count"]["value"], "3");
    assert!(a["rows"][0].as_object().unwrap().contains_key("group"));
    assert!(a["rows"][0]["group"].is_null());
}
#[test]
fn group_null_profile_keeps_old_results_and_source_budget_guards() {
    let v = unbound_request();
    let input = typed(&v);
    let r = RustSemanticRuntime.query_with_options_v4(&input).unwrap();
    let canonical = semantica_json::canonical_json(&serde_json::to_value(&input).unwrap()).unwrap();
    assert_eq!(r.request_sha256, semantica_runtime::sha256(canonical));
    assert_eq!(
        r.query_result.data_sha256,
        v["data_sha256"].as_str().unwrap()
    );
    let mut old = v.clone();
    old["schema"] = "semantica.query-options-request/3".into();
    old["profile"] = "rdflib760-initial-shadow/3".into();
    let old: SemanticQueryOptionsRequestV3 = serde_json::from_value(old).unwrap();
    let a = RustSemanticRuntime
        .execute_query_with_options_v3(&old)
        .unwrap()
        .query_result
        .result;
    assert!(!a["rows"][0].as_object().unwrap().contains_key("group"));
    let mut v2 = v.clone();
    v2["schema"] = "semantica.query-options-request/2".into();
    v2["profile"] = "rdflib760-describe-context/2".into();
    v2.as_object_mut().unwrap().remove("guard");
    v2.as_object_mut().unwrap().remove("budget");
    let v2: SemanticQueryOptionsRequestV2 = serde_json::from_value(v2).unwrap();
    let a = RustSemanticRuntime
        .execute_query_with_options_v2(&v2)
        .unwrap()
        .query_result
        .result;
    assert!(!a["rows"][0].as_object().unwrap().contains_key("group"));
    let store = graph::load(v["data"].as_str().unwrap().as_bytes()).unwrap();
    let a = graph::query_with_options(
        &store,
        v["query"].as_str().unwrap(),
        &BTreeMap::from([("ex".into(), "urn:".into())]),
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(!a["rows"][0].as_object().unwrap().contains_key("group"));
    for (key, value, code) in [
        (
            "schema",
            json!("semantica.query-options-request/3"),
            "invalid_contract",
        ),
        (
            "profile",
            json!("rdflib760-initial-shadow/3"),
            "invalid_contract",
        ),
        (
            "data",
            json!(format!("{} ", v["data"].as_str().unwrap())),
            "source_identity_mismatch",
        ),
    ] {
        let mut changed = v.clone();
        changed[key] = value;
        assert_eq!(
            RustSemanticRuntime
                .query_with_options_v4(&typed(&changed))
                .unwrap_err()
                .code,
            code
        );
    }
    let mut tiny = v.clone();
    tiny["budget"]["data_bytes"] = json!(1);
    assert_eq!(
        RustSemanticRuntime
            .query_with_options_v4(&typed(&tiny))
            .unwrap_err()
            .code,
        "input_budget_exceeded"
    );
    let mut enlarged = v.clone();
    enlarged["budget"]["ast_depth"] = json!(97);
    assert_eq!(
        RustSemanticRuntime
            .query_with_options_v4(&typed(&enlarged))
            .unwrap_err()
            .code,
        "invalid_owned_budget"
    );
}
