//! Independent original74 observations precede this implementation. Strict old
//! controls and unresolved nonstandard outputs are distinct from profile3 parity.
use semantica_contracts::{SemanticQueryOptionsRequestV2, SemanticQueryOptionsRequestV3};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};

fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/aggregate-context-original74.json")).unwrap()
}
fn request(case: &Value) -> Value {
    json!({"schema":"semantica.query-options-request/3", "profile":"rdflib760-initial-shadow/3",
        "data":case["data"], "data_sha256":semantica_runtime::sha256(case["data"].as_str().unwrap().as_bytes()),
        "rdf_format":case["rdf_format"], "query":case["query"], "namespaces":case["namespaces"], "bindings":case["bindings"],
        "budget":{"data_bytes":16777216,"query_bytes":1048576,"parameter_count":128,"parameter_bytes":16777216,
            "quads":100000,"rows":10000,"triples":100000,"ast_depth":96,"ast_nodes":100000,"stack_bytes":8388608},
        "guard":{"schema":"semantica.query-shadow-guard/3","assignment_scope":"external_initial_targets_only",
            "query_scope":"genuine_parser_declarations","initial_context":"immutable_exact_terms",
            "computed_scope":"marked_tuple_shadow","project_scope":"selected_tuple_domain","dataset_scope":"same_owner_dataset",
            "group_guard":"aggregate_target_refused","remote_guard":"service_from_refused"}})
}
fn execute(input: Value) -> Result<Value, Value> {
    let input: SemanticQueryOptionsRequestV3 = serde_json::from_value(input).unwrap();
    RustSemanticRuntime
        .execute_query_with_options_v3(&input)
        .map(|v| serde_json::to_value(v).unwrap())
        .map_err(|v| serde_json::to_value(v).unwrap())
}
fn n3(term: &Value) -> String {
    match term["kind"].as_str().unwrap() {
        "iri" => format!("<{}>", term["value"].as_str().unwrap()),
        "blank" => format!("_:{}", term["value"].as_str().unwrap()),
        "literal" => {
            let value = serde_json::to_string(term["value"].as_str().unwrap()).unwrap();
            if let Some(language) = term["language"].as_str() {
                format!("{value}@{language}")
            } else if let Some(datatype) = term["datatype"]
                .as_str()
                .filter(|v| *v != "http://www.w3.org/2001/XMLSchema#string")
            {
                format!("{value}^^<{datatype}>")
            } else {
                value
            }
        }
        _ => panic!("unrecognized exact term"),
    }
}
fn original_bag(original: &Value) -> Vec<String> {
    let mut rows: Vec<_> = original["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let values: std::collections::BTreeMap<_, _> = row
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, term)| (name.clone(), term["n3"].as_str().unwrap().to_owned()))
                .collect();
            serde_json::to_string(&values).unwrap()
        })
        .collect();
    rows.sort();
    rows
}
fn current_bag(result: &Value) -> Vec<String> {
    let mut rows: Vec<_> = result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let values: std::collections::BTreeMap<_, _> = row
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, term)| (name.clone(), n3(term)))
                .collect();
            serde_json::to_string(&values).unwrap()
        })
        .collect();
    rows.sort();
    rows
}
fn matches(original: &Value, result: &Value) -> bool {
    original["status"] != "original_error"
        && result["kind"] == "select"
        && original["columns"] == result["columns"]
        && original["row_count"] == result["row_count"]
        && original_bag(original) == current_bag(result)
}
#[test]
fn shadow3_original_external_assignment_and_scope_cases() {
    let fixture = oracle();
    let ids = fixture["initial_reassignment_original_success_ids"]
        .as_array()
        .unwrap();
    let residual = fixture["known_distinct_legacy_semantic_ids"]
        .as_array()
        .unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        if ids.contains(&case["input"]["id"]) && !residual.contains(&case["input"]["id"]) {
            let response = execute(request(&case["input"]))
                .unwrap_or_else(|err| panic!("{}: {err}", case["input"]["id"]));
            assert!(
                matches(&case["original"], &response["query_result"]["result"]),
                "{} original={} current={response}",
                case["input"]["id"],
                case["original"]
            );
            count += 1;
        }
    }
    assert_eq!(ids.len(), 35);
    assert_eq!(count, 31);
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 74);
}
#[test]
fn shadow3_preserves_other_strict_results_and_checks_count_zero_and_scope_guards() {
    let fixture = oracle();
    let baseline: Value = serde_json::from_str(include_str!(
        "fixtures/aggregate-context-strict2-baseline74.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut value = request(&case["input"]);
        value["schema"] = json!("semantica.query-options-request/2");
        value["profile"] = json!("rdflib760-describe-context/2");
        value.as_object_mut().unwrap().remove("guard");
        value.as_object_mut().unwrap().remove("budget");
        let input: SemanticQueryOptionsRequestV2 = serde_json::from_value(value).unwrap();
        let actual = RustSemanticRuntime
            .execute_query_with_options_v2(&input)
            .map(|v| serde_json::to_value(v).unwrap())
            .unwrap_or_else(|v| serde_json::to_value(v).unwrap());
        let expected = baseline["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == case["input"]["id"])
            .unwrap();
        // Preserve every old raw fixture byte. The authorized parser bugfix
        // changes only variable-AS aliases: the genuine alias now exists, so
        // strict2's existing reassignment guard is reached for external targets.
        // No-initial alias is checked against the original independent result.
        // COUNT arithmetic errors have a separately frozen complete standard
        // response; the profile2 initial Count-target refusal is unchanged.
        match case["input"]["id"].as_str().unwrap() {
            "group_alias_no_initial" => {
                assert!(matches(
                    &case["original"],
                    &actual["query_result"]["result"]
                ));
            }
            "group_alias_part" | "group_alias_other" => {
                assert_eq!(actual["code"], "query_execution_failed");
                assert!(
                    actual["message"]
                        .as_str()
                        .unwrap()
                        .contains("query_initial_binding_reassignment_unsupported")
                );
            }
            "count_arithmetic_error_no_alias" => {
                // Independently frozen COUNT-COMPLETE-RESPONSE expectations;
                // only the previously unbound count cell becomes integer0.
                assert_eq!(
                    actual,
                    json!({
                        "profile":"rdflib760-describe-context/2",
                        "query_result":{
                            "data_sha256":"893293e6adca58b1c80aeb52193b1a2d0c26a216ec5a004bfeb7c59abc877287",
                            "execution":"completed","query_kind":"select",
                            "result":{"kind":"select","columns":["count"],
                                "rows":[{"count":{"kind":"literal","value":"0",
                                    "datatype":"http://www.w3.org/2001/XMLSchema#integer","language":null}}],
                                "row_count":1}
                        },
                        "request_sha256":"d2a802f2a0f434cb90da62acc5a2aba7772da2bd82ea524302b312764a1f342f",
                        "schema":"semantica.query-options-result/2"
                    })
                );
            }
            _ => assert_eq!(actual, expected["raw"], "strict2 {}", case["input"]["id"]),
        }
    }
    let case = &fixture["cases"][0]["input"];
    let positive = request(case);
    for (path, value) in [
        ("schema", json!("semantica.query-options-request/2")),
        ("profile", json!("neutral/1")),
        ("data_sha256", json!("a".repeat(64))),
    ] {
        let mut input = positive.clone();
        input[path] = value;
        assert!(execute(input).is_err());
    }
    for field in [
        "assignment_scope",
        "query_scope",
        "initial_context",
        "computed_scope",
        "project_scope",
        "dataset_scope",
        "group_guard",
        "remote_guard",
    ] {
        let mut input = positive.clone();
        input["guard"][field] = json!("unrestricted");
        assert!(execute(input).is_err(), "{field}");
    }
    for field in [
        "data_bytes",
        "query_bytes",
        "parameter_count",
        "parameter_bytes",
        "quads",
        "rows",
        "triples",
        "ast_depth",
        "ast_nodes",
        "stack_bytes",
    ] {
        for invalid in [0, u64::MAX] {
            let mut input = positive.clone();
            input["budget"][field] = json!(invalid);
            assert!(execute(input).is_err(), "{field}={invalid}");
        }
    }
    for field in ["unknown_policy", "python_fallback"] {
        let mut input = positive.clone();
        input[field] = json!(true);
        assert!(serde_json::from_value::<SemanticQueryOptionsRequestV3>(input).is_err());
    }
    for query in [
        "SELECT ?name WHERE { BIND(\"one\" AS ?name) BIND(\"two\" AS ?name) }",
        "SELECT ?name WHERE { ?s ex:name ?name BIND(\"next\" AS ?name) }",
        "SELECT ?name WHERE { VALUES ?name { \"prior\" } BIND(\"next\" AS ?name) }",
        "SELECT ?name WHERE { SERVICE <https://example.invalid/sparql> { ?s ex:name ?name } }",
        "SELECT ?name FROM <https://example.invalid/data> WHERE { ?s ex:name ?name }",
    ] {
        let mut input = positive.clone();
        input["query"] = json!(query);
        input["bindings"] = json!({"name":"\"prior\""});
        assert!(execute(input).is_err(), "{query}");
    }
    for bindings in [
        json!({"name":"\"a\"","?name":"\"b\""}),
        json!({"name":"not-a-term"}),
        json!({"name":"\"a\" . <urn:b> <urn:p> \"b\""}),
    ] {
        let mut input = positive.clone();
        input["bindings"] = bindings;
        assert!(execute(input).is_err());
    }
}
#[test]
fn shadow3_tighter_budgets_are_enforced_in_the_actual_owner() {
    let fixture = oracle();
    let mut input = request(&fixture["cases"][0]["input"]);
    for (field, token) in [
        ("data_bytes", "input_budget_exceeded"),
        ("query_bytes", "input_budget_exceeded"),
        ("quads", "rdf_quad_budget"),
        ("ast_nodes", "query_initial_binding_ast_node_budget"),
    ] {
        let mut smaller = input.clone();
        smaller["budget"][field] = json!(1);
        let error = execute(smaller).unwrap_err();
        assert!(error.to_string().contains(token), "{field}: {error}");
    }
    input["query"] = json!("SELECT ?s WHERE { ?s ex:tag ?tag }");
    input["budget"]["rows"] = json!(1);
    assert!(
        execute(input.clone())
            .unwrap_err()
            .to_string()
            .contains("query_row_budget")
    );
    input["query"] = json!("DESCRIBE ex:part");
    input["budget"]["triples"] = json!(1);
    assert!(
        execute(input)
            .unwrap_err()
            .to_string()
            .contains("query_triple_budget")
    );
}
