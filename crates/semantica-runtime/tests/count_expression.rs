//! Literal standard expectations were frozen independently before this owner edit.
use semantica_contracts::{
    SemanticQueryOptionsRequestV2, SemanticQueryOptionsRequestV3, SemanticQueryRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn controls() -> Value {
    serde_json::from_str(include_str!("fixtures/count-expression-standard16.json")).unwrap()
}
fn request(input: &Value) -> Value {
    json!({"schema":"semantica.query-options-request/3","profile":"rdflib760-initial-shadow/3",
        "data":input["data"],"data_sha256":semantica_runtime::sha256(input["data"].as_str().unwrap().as_bytes()),
        "rdf_format":input["rdf_format"],"query":input["query"],"namespaces":input["namespaces"],"bindings":input["bindings"],
        "budget":{"data_bytes":16777216,"query_bytes":1048576,"parameter_count":128,"parameter_bytes":16777216,
            "quads":100000,"rows":10000,"triples":100000,"ast_depth":96,"ast_nodes":100000,"stack_bytes":8388608},
        "guard":{"schema":"semantica.query-shadow-guard/3","assignment_scope":"external_initial_targets_only",
            "query_scope":"genuine_parser_declarations","initial_context":"immutable_exact_terms","computed_scope":"marked_tuple_shadow",
            "project_scope":"selected_tuple_domain","dataset_scope":"same_owner_dataset","group_guard":"aggregate_target_refused",
            "remote_guard":"service_from_refused"}})
}
fn n3(term: &Value) -> String {
    match term["kind"].as_str().unwrap() {
        "iri" => format!("<{}>", term["value"].as_str().unwrap()),
        "blank" => format!("_:{}", term["value"].as_str().unwrap()),
        "literal" => {
            let text = serde_json::to_string(term["value"].as_str().unwrap()).unwrap();
            if let Some(language) = term["language"].as_str() {
                format!("{text}@{language}")
            } else if let Some(datatype) = term["datatype"]
                .as_str()
                .filter(|v| *v != "http://www.w3.org/2001/XMLSchema#string")
            {
                format!("{text}^^<{datatype}>")
            } else {
                text
            }
        }
        _ => panic!("unknown RDF term"),
    }
}
fn bag(rows: &Value, original: bool) -> Vec<String> {
    let mut rows: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let terms: BTreeMap<_, _> = row
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, term)| {
                    (
                        key.clone(),
                        if original {
                            term["n3"].as_str().unwrap().to_owned()
                        } else {
                            n3(term)
                        },
                    )
                })
                .collect();
            serde_json::to_string(&terms).unwrap()
        })
        .collect();
    rows.sort();
    rows
}
fn check(input: &Value, result: &Value) {
    let expected = &input["standard_expected"];
    assert_eq!(result["kind"], "select", "{}", input["id"]);
    assert_eq!(result["columns"], expected["columns"], "{}", input["id"]);
    assert_eq!(
        result["row_count"], expected["row_count"],
        "{}",
        input["id"]
    );
    assert_eq!(
        bag(&result["rows"], false),
        bag(&expected["rows"], true),
        "{}",
        input["id"]
    );
}
#[test]
fn count_standard_errors_distinct_order_and_sum_keep_independent_meanings() {
    let fixture = controls();
    let inputs = fixture["controls"].as_array().unwrap();
    assert_eq!(inputs.len(), 16);
    for input in inputs {
        let request: SemanticQueryOptionsRequestV3 =
            serde_json::from_value(request(input)).unwrap();
        let actual = RustSemanticRuntime
            .execute_query_with_options_v3(&request)
            .unwrap();
        check(input, &actual.query_result.result);
    }
}
#[test]
fn profile2_initial_target_refusal_and_neutral_real_owner_remain_distinct() {
    let fixture = controls();
    for input in fixture["controls"].as_array().unwrap() {
        let mut body = request(input);
        body["schema"] = "semantica.query-options-request/2".into();
        body["profile"] = "rdflib760-describe-context/2".into();
        body.as_object_mut().unwrap().remove("guard");
        body.as_object_mut().unwrap().remove("budget");
        let typed: SemanticQueryOptionsRequestV2 = serde_json::from_value(body).unwrap();
        if input["id"] == "initial_target_three_way_control" {
            assert_eq!(
                RustSemanticRuntime
                    .execute_query_with_options_v2(&typed)
                    .unwrap_err()
                    .message,
                "query_initial_binding_reassignment_unsupported"
            );
        } else {
            check(
                input,
                &RustSemanticRuntime
                    .execute_query_with_options_v2(&typed)
                    .unwrap()
                    .query_result
                    .result,
            );
            let neutral:SemanticQueryRequest=serde_json::from_value(json!({"schema":"semantica.query-request/1","data":input["data"],
                "data_sha256":semantica_runtime::sha256(input["data"].as_str().unwrap().as_bytes()),"rdf_format":input["rdf_format"],
                "query":format!("PREFIX ex: <urn:>\n{}",input["query"].as_str().unwrap())})).unwrap();
            check(
                input,
                &RustSemanticRuntime.execute_query(&neutral).unwrap().result,
            );
        }
    }
}
#[test]
fn count_correction_does_not_authorize_changed_source_budget_or_guard() {
    let fixture = controls();
    let input = &fixture["controls"][0];
    let mut body = request(input);
    body["data_sha256"] = "0".repeat(64).into();
    let typed: SemanticQueryOptionsRequestV3 = serde_json::from_value(body).unwrap();
    assert!(
        RustSemanticRuntime
            .execute_query_with_options_v3(&typed)
            .is_err()
    );
    let mut body = request(input);
    body["budget"]["query_bytes"] = 1.into();
    let typed: SemanticQueryOptionsRequestV3 = serde_json::from_value(body).unwrap();
    assert!(
        RustSemanticRuntime
            .execute_query_with_options_v3(&typed)
            .is_err()
    );
    let mut body = request(input);
    body["guard"]["group_guard"] = "unchecked".into();
    let typed: SemanticQueryOptionsRequestV3 = serde_json::from_value(body).unwrap();
    assert!(
        RustSemanticRuntime
            .execute_query_with_options_v3(&typed)
            .is_err()
    );
}
