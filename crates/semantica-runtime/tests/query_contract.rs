//! Independent equivalent RDF documents and query answers; no engineering adoption asserted.
use semantica_contracts::{SemanticQueryPort, SemanticQueryRequest};
use semantica_runtime::runtime::RustSemanticRuntime;

fn request(data: &str, format: &str, query: &str) -> SemanticQueryRequest {
    SemanticQueryRequest {
        schema: "semantica.query-request/1".into(),
        data: data.into(),
        data_sha256: semantica_runtime::sha256(data.as_bytes()),
        rdf_format: format.into(),
        query: query.into(),
    }
}

#[test]
fn query_formats_preserve_identity_and_empty_graph() {
    let documents = [
        (
            "turtle",
            "@prefix ex:<urn:test:>. ex:part ex:name \"轴\"@zh.",
        ),
        ("ntriples", "<urn:test:part> <urn:test:name> \"轴\"@zh ."),
        (
            "rdfxml",
            "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:ex=\"urn:test:\"><rdf:Description rdf:about=\"urn:test:part\"><ex:name xml:lang=\"zh\">轴</ex:name></rdf:Description></rdf:RDF>",
        ),
    ];
    for (format, data) in documents {
        let input = request(
            data,
            format,
            "SELECT ?name WHERE { <urn:test:part> <urn:test:name> ?name }",
        );
        let response = RustSemanticRuntime.query(&input).unwrap();
        assert_eq!(response.execution, "completed");
        assert_eq!(response.data_sha256, input.data_sha256);
        assert_eq!(response.result["rows"][0]["name"]["value"], "轴");
        assert_eq!(response.result["rows"][0]["name"]["language"], "zh");
    }
    let empty = RustSemanticRuntime
        .query(&request("", "turtle", "ASK { ?s ?p ?o }"))
        .unwrap();
    assert_eq!(empty.query_kind, "ask");
    assert_eq!(empty.result["value"], false);
}

#[test]
fn query_source_mismatch_and_unsupported_format_fail_closed() {
    let mut input = request("<urn:s> <urn:p> <urn:o>.", "turtle", "ASK { ?s ?p ?o }");
    input.data.push('\n');
    assert_eq!(
        RustSemanticRuntime.query(&input).unwrap_err().code,
        "source_identity_mismatch"
    );
    input = request("", "unsupported", "ASK { ?s ?p ?o }");
    assert_eq!(
        RustSemanticRuntime.query(&input).unwrap_err().phase,
        "contract"
    );
    input = request(
        "",
        "turtle",
        "ASK { SERVICE <https://example.invalid/sparql> { ?s ?p ?o } }",
    );
    let denied = RustSemanticRuntime.query(&input).unwrap_err();
    assert_eq!(denied.code, "query_execution_failed");
    assert!(denied.message.contains("remote_service_unsupported"));
}

#[test]
fn malformed_sources_queries_and_unknown_fields_remain_errors() {
    assert!(
        RustSemanticRuntime
            .query(&request("<unterminated", "turtle", "ASK {}"))
            .is_err()
    );
    assert!(
        RustSemanticRuntime
            .query(&request("", "turtle", "this is not SPARQL"))
            .is_err()
    );
    assert!(
        RustSemanticRuntime
            .query(&request("", "turtle", ""))
            .is_err()
    );
    let mut value = serde_json::to_value(request("", "turtle", "ASK {}")).unwrap();
    value["engineering_adopted"] = true.into();
    assert!(serde_json::from_value::<SemanticQueryRequest>(value).is_err());
}

#[test]
fn graph_query_contract_preserves_construct_and_describe_identity() {
    // A graph result and the operation that created it are different fields.
    // This expectation comes from SPARQL query kinds and the frozen contract.
    for (query, expected_kind) in [
        ("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }", "construct"),
        ("DESCRIBE <urn:s>", "describe"),
    ] {
        let response = RustSemanticRuntime
            .query(&request("<urn:s> <urn:p> <urn:o>.", "turtle", query))
            .unwrap();
        assert_eq!(response.execution, "completed");
        assert_eq!(response.query_kind, expected_kind);
        assert_eq!(response.result["kind"], "graph");
        assert_eq!(response.result["triples"].as_array().unwrap().len(), 1);
    }
}
