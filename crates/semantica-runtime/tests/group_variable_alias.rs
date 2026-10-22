//! Original source/interpreter observations precede the shared parser fix.
use semantica_contracts::SemanticQueryOptionsRequestV3;
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn request(input: &Value) -> SemanticQueryOptionsRequestV3 {
    serde_json::from_value(json!({"schema":"semantica.query-options-request/3","profile":"rdflib760-initial-shadow/3",
        "data":input["data"],"data_sha256":semantica_runtime::sha256(input["data"].as_str().unwrap().as_bytes()),
        "rdf_format":input["rdf_format"],"query":input["query"],"namespaces":input["namespaces"],"bindings":input["bindings"],
        "budget":{"data_bytes":16777216,"query_bytes":1048576,"parameter_count":128,"parameter_bytes":16777216,
            "quads":100000,"rows":10000,"triples":100000,"ast_depth":96,"ast_nodes":100000,"stack_bytes":8388608},
        "guard":{"schema":"semantica.query-shadow-guard/3","assignment_scope":"external_initial_targets_only",
            "query_scope":"genuine_parser_declarations","initial_context":"immutable_exact_terms","computed_scope":"marked_tuple_shadow",
            "project_scope":"selected_tuple_domain","dataset_scope":"same_owner_dataset","group_guard":"aggregate_target_refused",
            "remote_guard":"service_from_refused"}})).unwrap()
}
fn n3(v: &Value) -> String {
    match v["kind"].as_str().unwrap() {
        "iri" => format!("<{}>", v["value"].as_str().unwrap()),
        "blank" => format!("_:{}", v["value"].as_str().unwrap()),
        "literal" => {
            let text = serde_json::to_string(v["value"].as_str().unwrap()).unwrap();
            if let Some(lang) = v["language"].as_str() {
                format!("{text}@{lang}")
            } else if let Some(dt) = v["datatype"]
                .as_str()
                .filter(|d| *d != "http://www.w3.org/2001/XMLSchema#string")
            {
                format!("{text}^^<{dt}>")
            } else {
                text
            }
        }
        _ => panic!("unknown term kind"),
    }
}
fn bag(rows: &Value, original: bool) -> Vec<String> {
    let mut result: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let values: BTreeMap<_, _> = r
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, t)| {
                    (
                        key.clone(),
                        if original {
                            t["n3"].as_str().unwrap().to_owned()
                        } else {
                            n3(t)
                        },
                    )
                })
                .collect();
            serde_json::to_string(&values).unwrap()
        })
        .collect();
    result.sort();
    result
}
fn exact(original: &Value, result: &Value) -> bool {
    original["status"] == "observed"
        && result["kind"] == "select"
        && original["columns"] == result["columns"]
        && original["row_count"] == result["row_count"]
        && bag(&original["rows"], true) == bag(&result["rows"], false)
}
#[test]
fn legal_variable_group_alias_preserves_original_terms_and_fresh_scope() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/group-variable-alias-original15.json"
    ))
    .unwrap();
    let unsafe_ids = [
        "query_target_redeclared",
        "duplicate_alias_target",
        "self_alias_query_target",
    ];
    let mut matched = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let id = case["input"]["id"].as_str().unwrap();
        let result = RustSemanticRuntime.execute_query_with_options_v3(&request(&case["input"]));
        if unsafe_ids.contains(&id) {
            let failure = serde_json::to_value(result.unwrap_err()).unwrap();
            assert!(
                failure["message"]
                    .as_str()
                    .unwrap()
                    .contains("GROUP BY alias overrides an existing query variable"),
                "{id}: {failure}"
            );
        } else if id == "unbound_variable_group_alias" {
            let actual = serde_json::to_value(result.unwrap()).unwrap();
            // Original result.bindings keeps an explicit None key; this owner
            // serializes only bound RDF terms. Preserve both representations as
            // a separate unresolved compatibility observation, not bag parity.
            assert!(
                case["original"]["rows"][0]
                    .as_object()
                    .unwrap()
                    .contains_key("group")
            );
            assert!(case["original"]["rows"][0]["group"].is_null());
            assert_eq!(actual["query_result"]["result"]["row_count"], 1);
            assert!(
                !actual["query_result"]["result"]["rows"][0]
                    .as_object()
                    .unwrap()
                    .contains_key("group")
            );
            assert_eq!(
                actual["query_result"]["result"]["rows"][0]["count"]["value"],
                "3"
            );
        } else if id == "empty_group_alias_unresolved_control" {
            let actual = serde_json::to_value(result.unwrap()).unwrap();
            // Original nonstandard empty group remains open; do not replace it
            // with a new expected success or claim this boundary was repaired.
            assert_eq!(case["original"]["row_count"], 1);
            assert_eq!(actual["query_result"]["result"]["row_count"], 0);
        } else {
            let actual =
                serde_json::to_value(result.unwrap_or_else(|e| panic!("{id}: {e:?}"))).unwrap();
            assert!(
                exact(&case["original"], &actual["query_result"]["result"]),
                "{id}: expected={} current={actual}",
                case["original"]
            );
            matched += 1;
        }
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 15);
    assert_eq!(matched, 10);
}
#[test]
fn original74_keep_other_results_and_check_alias_and_standard_count_responses() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/aggregate-context-original74.json")).unwrap();
    let prior: Value =
        serde_json::from_str(include_str!("fixtures/group-alias-before-current74.json")).unwrap();
    let fixed = [
        "group_alias_no_initial",
        "group_alias_part",
        "group_alias_other",
    ];
    let mut full_matches = 0;
    let mut unchanged = 0;
    let mut standard_count_responses = 0;
    for case in oracle["cases"].as_array().unwrap() {
        let id = case["input"]["id"].as_str().unwrap();
        let raw = RustSemanticRuntime.execute_query_with_options_v3(&request(&case["input"]));
        let before = &prior["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap()["current"];
        match raw {
            Ok(response) => {
                let value = serde_json::to_value(response).unwrap();
                if fixed.contains(&id) {
                    assert!(
                        exact(&case["original"], &value["query_result"]["result"]),
                        "{id}"
                    );
                } else if id == "count_expression_error" || id == "count_arithmetic_error_no_alias"
                {
                    // These two complete responses were frozen independently
                    // before the Count owner edit. The legacy Count3 fixtures
                    // remain byte-for-byte history and separate residuals.
                    let request_sha = match id {
                        "count_expression_error" => {
                            "60f3aa693120583e394b040c9037c3bbd589f0e80c2c8fab794808007a35e6b1"
                        }
                        "count_arithmetic_error_no_alias" => {
                            "0b15f7fa4f09039da624beab1c21c3e6a2c8782af26ef62e3d7b6ca011128a7f"
                        }
                        _ => unreachable!(),
                    };
                    assert_eq!(
                        value,
                        json!({
                            "profile":"rdflib760-initial-shadow/3",
                            "query_result":{
                                "data_sha256":"893293e6adca58b1c80aeb52193b1a2d0c26a216ec5a004bfeb7c59abc877287",
                                "execution":"completed","query_kind":"select",
                                "result":{"kind":"select","columns":["count"],
                                    "rows":[{"count":{"kind":"literal","value":"0",
                                        "datatype":"http://www.w3.org/2001/XMLSchema#integer","language":null}}],
                                    "row_count":1}
                            },
                            "request_sha256":request_sha,
                            "schema":"semantica.query-options-result/3"
                        }),
                        "standard Count response: {id}"
                    );
                    standard_count_responses += 1;
                } else {
                    assert_eq!(value, before["response"], "unchanged current success: {id}");
                    unchanged += 1;
                }
                if exact(&case["original"], &value["query_result"]["result"]) {
                    full_matches += 1;
                }
            }
            Err(failure) => {
                assert!(!fixed.contains(&id));
                let value = serde_json::to_value(failure).unwrap();
                assert_eq!(value, before["failure"], "unchanged current refusal: {id}");
                unchanged += 1;
            }
        }
    }
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 74);
    assert_eq!(unchanged, 69);
    assert_eq!(standard_count_responses, 2);
    assert_eq!(full_matches, 58);
    assert_eq!(74 - full_matches, 16); // Unresolved original16 never hidden.
}
#[test]
fn neutral_owner_supports_fresh_alias_but_initial_reassignment_guard_remains() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/group-variable-alias-original15.json"
    ))
    .unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == "group_alias_no_initial")
        .unwrap();
    let data =
        semantica_runtime::graph::load(case["input"]["data"].as_str().unwrap().as_bytes()).unwrap();
    let ns = BTreeMap::from([("ex".to_owned(), "urn:".to_owned())]);
    let result = semantica_runtime::graph::query_with_options(
        &data,
        case["input"]["query"].as_str().unwrap(),
        &ns,
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(exact(&case["original"], &result));
    let initial = BTreeMap::from([(
        "group".to_owned(),
        oxigraph::model::Term::NamedNode(oxigraph::model::NamedNode::new("urn:part").unwrap()),
    )]);
    let error = semantica_runtime::graph::query_with_options(
        &data,
        case["input"]["query"].as_str().unwrap(),
        &ns,
        &initial,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("query_initial_binding_reassignment_unsupported")
    );
}
