use oxigraph::io::RdfFormat;
use semantica_contracts::{
    SemanticMcpPort, SemanticMcpRequest, SemanticServiceBudgetCharge, SemanticServiceBudgetPort,
    SemanticServiceQueryRequest,
};
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    MAX_FRAME, McpService, NativeServiceBudget, StdioProfile, load_context_graph, serve_stdio,
};
use serde_json::{Value, json};
use std::io::Cursor;

fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/original-mcp-isolated-oracle.json")).unwrap()
}
fn graph() -> Value {
    json!({"nodes":[{"id":"a","type":"Person","label":"轴"},{"id":"d","type":"decision","properties":{"approved":false}},{"id":"b","type":"Part"}],"edges":[{"id":"e1","source":"a","target":"b","familyId":"motion","metadata":{"value":1}},{"id":"e2","source":"a","target":"b","familyId":"evidence","metadata":{"value":2}}]})
}
fn query_request(dataset: &mut DatasetSession) -> SemanticServiceQueryRequest {
    SemanticServiceQueryRequest {
        schema: "semantica.service-query-request/1".into(),
        expected_revision: dataset.revision(),
        expected_dataset_sha256: dataset.snapshot().unwrap().dataset_sha256,
        query: "SELECT ?name WHERE { <urn:test:part> <urn:test:name> ?name }".into(),
        use_cache: true,
    }
}
#[test]
fn canonical_original_dispatch_oracles() {
    let oracle = oracle();
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget);
    for case in oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["id"] != "nonempty-summary")
    {
        let result = service
            .handle(&case["request"])
            .unwrap_or_else(|e| panic!("{}: {:?}", case["id"], e));
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            case["expected"],
            "{}",
            case["id"]
        );
    }
}
#[test]
fn context_records_preserve_identity_and_do_not_count_rdf_subjects() {
    let mut dataset = DatasetSession::new();
    load_context_graph(&mut dataset, &graph()).unwrap();
    dataset
        .load(
            b"<urn:unrelated> <urn:p> <urn:o>.",
            RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
    let budget = NativeServiceBudget::bounded();
    {
        let service = McpService::new(&mut dataset, &budget);
        let case = oracle()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "nonempty-summary")
            .unwrap()
            .clone();
        assert_eq!(
            service.handle(&case["request"]).unwrap().unwrap(),
            case["expected"]
        );
    }
    let data = dataset.serialize(RdfFormat::NQuads, None).unwrap();
    assert!(
        data.contains("familyId")
            && data.contains("e1")
            && data.contains("e2")
            && data.contains("轴")
    );
    let before = dataset.snapshot().unwrap();
    assert!(load_context_graph(&mut dataset, &graph()).is_err());
    assert_eq!(
        before.dataset_sha256,
        dataset.snapshot().unwrap().dataset_sha256
    );
    let mut invalid = graph();
    invalid["nodes"][2]["id"] = json!("a");
    let mut fresh = DatasetSession::new();
    assert!(load_context_graph(&mut fresh, &invalid).is_err());
    assert_eq!(fresh.quad_count(), 0);
    assert_eq!(fresh.revision(), 0);
    let mut arbitrary = DatasetSession::new();
    arbitrary
        .load(b"<urn:x> <urn:p> <urn:y>.", RdfFormat::NTriples, None, None)
        .unwrap();
    let service = McpService::new(&mut arbitrary, &budget);
    let summary = service
        .handle(&json!({"id":1,"method":"tools/call","params":{"name":"get_graph_summary"}}))
        .unwrap()
        .unwrap();
    assert_eq!(summary["result"]["isError"], true);
    let payload: Value =
        serde_json::from_str(summary["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["error"]["code"], "context_projection_unavailable");
}
#[test]
fn full_catalog_keeps_unavailable_tools_and_resources_visible() {
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget);
    let actual = service
        .handle(&json!({"id":1,"method":"tools/list"}))
        .unwrap()
        .unwrap();
    let catalog: Value =
        serde_json::from_str(include_str!("fixtures/original-mcp-catalog.json")).unwrap();
    assert_eq!(actual["result"]["tools"].as_array().unwrap().len(), 17);
    for (a, b) in actual["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .zip(catalog["tools"].as_array().unwrap())
    {
        for key in ["name", "description", "inputSchema"] {
            assert_eq!(a[key], b[key]);
        }
        assert_eq!(
            a["_meta"]["semantica"]["availability"],
            if a["name"] == "get_graph_summary" {
                "available"
            } else {
                "unavailable"
            }
        );
    }
    for tool in catalog["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["name"] != "get_graph_summary")
    {
        let r=service.handle(&json!({"id":2,"method":"tools/call","params":{"name":tool["name"],"arguments":{"passed":true,"approved":true}}})).unwrap().unwrap();
        assert_eq!(r["result"]["isError"], true);
        let data: Value =
            serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(data["execution"], "not_run");
        assert_eq!(data["error"]["code"], "tool_unavailable");
    }
    let resources = service
        .handle(&json!({"id":3,"method":"resources/list"}))
        .unwrap()
        .unwrap();
    assert_eq!(
        resources["result"]["resources"].as_array().unwrap().len(),
        4
    );
    for (a, b) in resources["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .zip(catalog["fixed_resources"].as_array().unwrap())
    {
        for key in ["uri", "name", "description", "mimeType"] {
            assert_eq!(a[key], b[key]);
        }
        assert_eq!(
            a["_meta"]["semantica"]["dynamic_manifests"],
            "not_connected"
        );
    }
    let unknown=service.handle(&json!({"id":4,"method":"resources/read","params":{"uri":"semantica://packages/manifest/../../private"}})).unwrap().unwrap();
    let data: Value =
        serde_json::from_str(unknown["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        data["error"]["code"],
        "package_manifest_provider_unavailable"
    );
    let init = service
        .handle(&json!({"id":5,"method":"initialize","params":{"protocolVersion":"unsupported"}}))
        .unwrap()
        .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(
        init["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
}
#[test]
fn queries_share_session_cache_revision_and_typed_identity() {
    let mut dataset = DatasetSession::new();
    dataset
        .load(
            "<urn:test:part> <urn:test:name> \"轴\"@zh .".as_bytes(),
            RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
    let input = query_request(&mut dataset);
    let budget = NativeServiceBudget::bounded();
    {
        let service = McpService::new(&mut dataset, &budget);
        let first = service.execute_query(&input).unwrap();
        assert_eq!(first.result.result["rows"][0]["name"]["value"], "轴");
        assert_eq!(first.result.result["rows"][0]["name"]["language"], "zh");
        assert!(!first.result.cache_hit);
        assert!(service.execute_query(&input).unwrap().result.cache_hit);
        let mut bad = input.clone();
        bad.expected_revision += 1;
        assert_eq!(
            service.execute_query(&bad).unwrap_err().code,
            "service_revision_mismatch"
        );
        bad = input.clone();
        bad.expected_dataset_sha256 = "0".repeat(64);
        assert_eq!(
            service.execute_query(&bad).unwrap_err().code,
            "service_dataset_identity_mismatch"
        );
        for text in [
            "ASK { SERVICE <https://example.invalid/> { ?s ?p ?o } }",
            "INSERT DATA {<urn:s> <urn:p> <urn:o>}",
            "SELECT WHERE { broken",
        ] {
            bad = input.clone();
            bad.query = text.into();
            assert_eq!(
                service.execute_query(&bad).unwrap_err().code,
                "service_query_failed"
            );
        }
        let mut forged = serde_json::to_value(&input).unwrap();
        forged["passed"] = json!(true);
        assert!(serde_json::from_value::<SemanticServiceQueryRequest>(forged).is_err());
    }
    dataset
        .load(
            b"<urn:test:other> <urn:p> <urn:o>.",
            RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
    let fresh = query_request(&mut dataset);
    assert_ne!(fresh.expected_dataset_sha256, input.expected_dataset_sha256);
    let service = McpService::new(&mut dataset, &budget);
    assert_eq!(
        service.execute_query(&input).unwrap_err().code,
        "service_revision_mismatch"
    );
    assert!(!service.execute_query(&fresh).unwrap().result.cache_hit);
    let wire = SemanticMcpRequest {
        schema: "semantica.mcp-service-request/1".into(),
        expected_revision: input.expected_revision,
        request: json!({"id":1,"method":"ping"}),
    };
    assert_eq!(
        service.dispatch(&wire).unwrap_err().code,
        "service_revision_mismatch"
    );
}
#[test]
fn service_budget_is_shared_and_fail_closed() {
    let mut dataset = DatasetSession::new();
    let input = SemanticServiceQueryRequest {
        schema: "semantica.service-query-request/1".into(),
        expected_revision: 0,
        expected_dataset_sha256: dataset.snapshot().unwrap().dataset_sha256,
        query: "ASK { ?s ?p ?o }".into(),
        use_cache: true,
    };
    let budget = NativeServiceBudget::new(2, 1000, 1000);
    let service = McpService::new(&mut dataset, &budget);
    service.handle(&json!({"id":1,"method":"ping"})).unwrap();
    assert_eq!(
        service.execute_query(&input).unwrap().result.result["value"],
        false
    );
    assert_eq!(
        service
            .handle(&json!({"id":2,"method":"ping"}))
            .unwrap_err()
            .code,
        "service_budget_exceeded"
    );
    let r = budget
        .charge(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(r.calls, 2);
    assert_eq!(
        budget
            .charge(&SemanticServiceBudgetCharge {
                kind: "response".into(),
                bytes: u64::MAX
            })
            .unwrap_err()
            .code,
        "service_budget_overflow"
    );
    assert_eq!(
        budget
            .charge(&SemanticServiceBudgetCharge {
                kind: "query".into(),
                bytes: MAX_FRAME as u64 + 1
            })
            .unwrap_err()
            .code,
        "service_frame_budget_exceeded"
    );
    assert_eq!(
        budget
            .charge(&SemanticServiceBudgetCharge {
                kind: "framing".into(),
                bytes: 0
            })
            .unwrap()
            .calls,
        2
    );
    let mut output_limited = DatasetSession::new();
    let limited = NativeServiceBudget::new(2, 1000, 1);
    let service = McpService::new(&mut output_limited, &limited);
    assert_eq!(
        service
            .handle(&json!({"id":1,"method":"ping"}))
            .unwrap_err()
            .code,
        "service_budget_exceeded"
    );
}
#[test]
fn stdio_frames_profiles_and_errors_are_real() {
    let bytes=b"  {\"id\":1,\"method\":\"ping\"}  \r\n{\"method\":\"notifications/initialized\"}\n[ {\"id\":2,\"method\":\"ping\"} ]\n{broken}\n";
    for profile in [StdioProfile::Canonical, StdioProfile::CompatibilityBatch] {
        let mut dataset = DatasetSession::new();
        let budget = NativeServiceBudget::bounded();
        let service = McpService::new(&mut dataset, &budget);
        let mut out = Vec::new();
        let mut diagnostics = Vec::new();
        serve_stdio(
            &service,
            &mut Cursor::new(bytes),
            &mut out,
            &mut diagnostics,
            profile,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.ends_with('\n'));
        let rows = text
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows[0], json!({"jsonrpc":"2.0","id":1,"result":{}}));
        assert_eq!(rows.last().unwrap()["error"]["code"], -32700);
        if profile == StdioProfile::Canonical {
            assert_eq!(rows.len(), 2);
            assert!(
                String::from_utf8(diagnostics)
                    .unwrap()
                    .contains("batch unsupported")
            );
        } else {
            assert_eq!(rows.len(), 3);
            assert_eq!(rows[1][0]["id"], 2);
            assert!(diagnostics.is_empty());
        }
        let counts = budget
            .charge(&SemanticServiceBudgetCharge {
                kind: "framing".into(),
                bytes: 0,
            })
            .unwrap();
        assert_eq!(counts.input_bytes, bytes.len() as u64);
        assert_eq!(counts.output_bytes, text.len() as u64);
    }
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget);
    let mut out = Vec::new();
    assert!(
        serve_stdio(
            &service,
            &mut Cursor::new(vec![b'x'; MAX_FRAME + 1]),
            &mut out,
            &mut Vec::new(),
            StdioProfile::Canonical
        )
        .is_err()
    );
    assert!(out.is_empty());
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::new(2, 8, 1000);
    let service = McpService::new(&mut dataset, &budget);
    let mut out = Vec::new();
    assert!(
        serve_stdio(
            &service,
            &mut Cursor::new(b"          {\"id\":1,\"method\":\"ping\"}\n"),
            &mut out,
            &mut Vec::new(),
            StdioProfile::Canonical
        )
        .is_err()
    );
    assert!(out.is_empty());
}

#[test]
fn stdio_empty_lines_consume_input_budget() {
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::new(4, 2, 1000);
    let service = McpService::new(&mut dataset, &budget);
    let mut out = Vec::new();
    assert!(
        serve_stdio(
            &service,
            &mut Cursor::new(b"\n\n\n"),
            &mut out,
            &mut Vec::new(),
            StdioProfile::Canonical
        )
        .is_err(),
        "blank wire bytes must not bypass the host input budget"
    );
    assert!(out.is_empty());
}

#[test]
fn context_projection_rejects_literal_identity_mismatch() {
    let mut dataset = DatasetSession::new();
    dataset.load(b"<urn:semantica:context:root> <urn:semantica:context:schema> \"semantica.context-record-projection/1\"@en <urn:semantica:context:records/1> .",RdfFormat::NQuads,None,None).unwrap();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget);
    let response = service
        .handle(&json!({"id":1,"method":"tools/call","params":{"name":"get_graph_summary"}}))
        .unwrap()
        .unwrap();
    assert_eq!(
        response["result"]["isError"], true,
        "language-tagged RDF is not the declared UTF8 context-schema identity"
    );
}

#[test]
fn stdio_preserves_original_literal_object_instead_of_private_number_tag() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/original-literal-object-oracle.json")).unwrap();
    let request = oracle["request_raw"].as_str().unwrap().to_owned() + "\n";
    // The expected raw response comes from original json.loads + exact _handle.
    // Reuse the neutral parser for the expectation so the bad Value parser cannot contaminate both sides.
    let expected =
        semantica_runtime::providers::parse_protocol_json(oracle["response_raw"].as_str().unwrap())
            .unwrap();
    assert!(expected["id"].is_object());
    assert_eq!(expected["id"]["$serde_json::private::Number"], "42");
    let mut dataset = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut dataset, &budget);
    let mut out = Vec::new();
    serve_stdio(
        &service,
        &mut Cursor::new(request.as_bytes()),
        &mut out,
        &mut Vec::new(),
        StdioProfile::Canonical,
    )
    .unwrap();
    let response =
        semantica_runtime::providers::parse_protocol_json(std::str::from_utf8(&out).unwrap())
            .unwrap();
    assert_eq!(
        response, expected,
        "original literal object must not be silently replaced by a number"
    );
    assert_eq!(
        budget
            .charge(&SemanticServiceBudgetCharge {
                kind: "framing".into(),
                bytes: 0
            })
            .unwrap()
            .input_bytes,
        request.len() as u64
    );
}
