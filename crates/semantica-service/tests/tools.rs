use semantica_runtime::dataset::DatasetSession;
use semantica_service::{McpService, NativeServiceBudget};
use serde_json::{Value, json};

fn call(service: &McpService<'_>, name: &str, arguments: Value) -> Value {
    let response=service.handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}})).unwrap().unwrap();
    assert!(response.get("error").is_none(), "{response}");
    serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}
#[test]
fn recorded_entity_changes_the_same_session_and_is_observable() {
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget).with_native_tools();
    let added = call(
        &service,
        "add_entity",
        json!({"id":"shaft","label":"轴","type":"Part","metadata":{"unit":"mm"}}),
    );
    assert_eq!(added, json!({"status":"added","id":"shaft"}));
    assert_eq!(
        call(&service, "get_graph_summary", json!({})),
        json!({"node_count":1,"decision_count":0,"graph_ready":true})
    );
    assert_eq!(service.revision().unwrap(), 1);
}
#[test]
fn same_context_decisions_precedents_causal_reasoning_and_export_are_actual_operations() {
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget).with_native_tools();
    let a=call(&service,"record_decision",json!({"category":"guide","scenario":"shaft precision","reasoning":"bearing stiffness","outcome":"selected","confidence":"0.9","entities":["shaft"]}))["decision_id"].as_str().unwrap().to_owned();
    let b=call(&service,"record_decision",json!({"category":"guide","scenario":"shaft precision","reasoning":"bearing stiffness","outcome":"measured","confidence":0.8,"entities":["shaft"]}))["decision_id"].as_str().unwrap().to_owned();
    assert_eq!(
        call(&service, "query_decisions", json!({}))["decisions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        call(&service, "query_decisions", json!({"category":"guide"}))["decisions"],
        json!([])
    ); // Original nested category projection.
    assert_eq!(
        call(
            &service,
            "find_precedents",
            json!({"scenario":"shaft precision","max_results":1})
        )["precedents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(
        &service,
        "add_relationship",
        json!({"source":a,"target":b,"type":"CAUSED"}),
    );
    let chain = call(&service, "get_causal_chain", json!({"decision_id":a}));
    assert_eq!(chain["chain"][0]["decision_id"], b);
    assert_eq!(chain["chain"][0]["metadata"]["causal_distance"], 1);
    assert_eq!(
        call(
            &service,
            "run_reasoning",
            json!({"facts":["Shaft(a)"],"rules":["IF Shaft(?x) THEN Part(?x)"]})
        )["derived_facts"],
        json!(["Part(a)"])
    );
    let analytics = call(&service, "get_graph_analytics", json!({}));
    assert!(analytics["top_nodes_by_pagerank"].is_array());
    assert!(analytics["community_count"].as_u64().unwrap() > 0);
    let exported = call(&service, "export_graph", json!({"format":"json"}));
    assert_eq!(
        exported["data"]["nodes"].as_array().unwrap().len(),
        call(&service, "get_graph_summary", json!({}))["node_count"]
            .as_u64()
            .unwrap() as usize
    );
}
#[test]
fn original_actual_exceptions_remain_separate_from_corrected_native_operations() {
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget).with_original_tools();
    let actual=service.handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"extract_relations","arguments":{"text":"Alice works at Acme"}}})).unwrap().unwrap();
    assert_eq!(actual["error"]["code"], -32603);
    let original: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/original-context-supplement.json"
    ))
    .unwrap();
    assert_eq!(
        call(&service, "get_graph_analytics", json!({}))["error"],
        original["analytics"]["canonical_error"]
    );
    // The supplement's RDF section used __new__ without constructor initialization.
    // Its immutable partial-construction observations are not the MCP construction contract.
    assert_eq!(
        call(&service, "export_graph", json!({"format":"json"}))["error"],
        "JSONExporter.export() missing 1 required positional argument: 'file_path'"
    );
}
#[test]
fn fully_initialized_original_exporter_matches_frozen_five_format_errors() {
    let original: Value = serde_json::from_str(include_str!(
        "fixtures/original-rdf-export-initialized.json"
    ))
    .unwrap();
    assert_eq!(
        original["profile"],
        "pinned-original-rdf-full-initialization/1"
    );
    for control in [
        "serializer_namespace_manager_initialized",
        "exporter_namespace_manager_initialized",
        "validator_initialized",
        "entity_linker_initialized",
        "dictionary_input_serializes_all_five_formats",
    ] {
        assert_eq!(original["controls"][control], true);
    }
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget).with_original_tools();
    let cases = original["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        assert_eq!(
            call(&service, "export_graph", json!({"format":case["format"]})),
            case["tool_result"]
        );
    }
}
#[test]
fn native_stdio_executes_entity_extraction_and_mutation_without_another_runtime() {
    use std::io::Cursor;
    let frames = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"add_entity\",\"arguments\":{\"id\":\"shaft\"}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"extract_entities\",\"arguments\":{\"text\":\"Alice Wang in 2026\"}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"extract_relations\",\"arguments\":{\"text\":\"Alice Wang founded Acme LLC.\"}}}\n"
    );
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget).with_native_tools();
    let mut out = Vec::new();
    let mut err = Vec::new();
    semantica_service::serve_stdio(
        &service,
        &mut Cursor::new(frames.as_bytes()),
        &mut out,
        &mut err,
        semantica_service::StdioProfile::Canonical,
    )
    .unwrap();
    assert!(err.is_empty());
    let output = String::from_utf8(out).unwrap();
    let lines: Vec<Value> = output
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    let payload = |i: usize| {
        serde_json::from_str::<Value>(lines[i]["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    assert_eq!(payload(0)["status"], "added");
    assert_eq!(payload(1)["_meta"]["model_execution"], "not_run");
    assert_eq!(payload(2)["relations"][0]["type"], "founded_by");
    assert_eq!(service.revision().unwrap(), 1);
}

#[test]
fn a_failed_output_budget_cannot_commit_a_context_mutation() {
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::new(4, 1024, 1);
    let service = McpService::new(&mut dataset, &budget).with_native_tools();
    let result=service.handle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"add_entity","arguments":{"id":"must-not-publish"}}}));
    assert!(result.is_err());
    assert_eq!(service.revision().unwrap(), 0);
}
#[test]
fn original_skos_value_error_and_native_failure_preserve_the_same_dataset() {
    for original in [false, true] {
        let mut dataset = DatasetSession::new();
        let budget = NativeServiceBudget::bounded();
        let service = McpService::new(&mut dataset, &budget);
        let service = if original {
            service.with_original_tools()
        } else {
            service.with_native_tools()
        };
        call(
            &service,
            "add_relationship",
            json!({"source":"a","target":"b","type":"skos:broader"}),
        );
        let revision = service.revision().unwrap();
        let response=service.handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_relationship","arguments":{"source":"b","target":"a","type":"skos:broader"}}})).unwrap().unwrap();
        if original {
            assert_eq!(response["error"]["code"], -32603);
            assert_eq!(
                response["error"]["message"],
                "Tool 'add_relationship' failed (ValueError). See server logs for details."
            );
        } else {
            assert_eq!(response["result"]["isError"], true);
            let payload: Value =
                serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert!(
                payload["error"]
                    .as_str()
                    .unwrap()
                    .starts_with("SKOS hierarchy contains a cycle involving '")
            );
        }
        assert_eq!(service.revision().unwrap(), revision);
        assert_eq!(
            call(&service, "get_graph_summary", json!({}))["node_count"],
            2
        );
    }
}
