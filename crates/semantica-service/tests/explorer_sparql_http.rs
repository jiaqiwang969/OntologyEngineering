use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget, explorer_http::HostAuthorization,
    explorer_sparql_http::serve_connection,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    time::{Duration, Instant},
};
fn request(query: &str) -> (u16, Value) {
    let body = serde_json::to_vec(&json!({"query":query})).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(
            stream,
            "POST /api/sparql HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut wire = Vec::new();
        stream.read_to_end(&mut wire).unwrap();
        wire
    });
    let (stream, _) = listener.accept().unwrap();
    let mut session = DatasetSession::new();
    let before = session.snapshot().unwrap();
    let budget = NativeServiceBudget::bounded();
    serve_connection(
        &McpService::new(&mut session, &budget),
        stream,
        &|_| HostAuthorization::Granted,
        Instant::now() + Duration::from_secs(2),
        |_, _| panic!("admission refusal must never dispatch child"),
    )
    .unwrap();
    assert_eq!(session.snapshot().unwrap(), before);
    let wire = client.join().unwrap();
    let split = wire.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    let header = std::str::from_utf8(&wire[..split]).unwrap();
    (
        header.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_slice(&wire[split + 4..]).unwrap(),
    )
}
#[test]
fn original_admission_responses_match_status_and_all_fields() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/original-sparql-admission-http.json")).unwrap();
    assert_eq!(cases["cases"].as_array().unwrap().len(), 20);
    for case in cases["cases"].as_array().unwrap() {
        let (status, body) = request(case["request"]["query"].as_str().unwrap());
        assert_eq!(
            status as u64,
            case["status"].as_u64().unwrap(),
            "{}",
            case["test_id"]
        );
        assert_eq!(body, case["response"], "{}", case["test_id"]);
    }
}
#[test]
fn oversized_unicode_query_preserves_character_count_and_precedence() {
    // Independently stated production limit: scalar count, not UTF8 byte count.
    // Also invalid/read-write text: length refusal must take precedence.
    let query = "界".repeat(10_001);
    let (status, body) = request(&query);
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"columns":[],"rows":[],"total":0,"truncated":false,"error":"Query exceeds the maximum allowed length of 10,000 characters (10,001 received). Please shorten your query.","error_line":null,"error_column":null})
    );
}

#[test]
fn version2_http_uses_host_resources_and_exact_shared_wire_budget() {
    use semantica_contracts::{
        SemanticDatasetResourceSelection, SemanticExplorerSparqlExecution,
        SemanticServiceBudgetCharge,
    };
    use semantica_service::{explorer_sparql_http::serve_connection_v2, load_context_graph};
    for profile in ["semantica.dataset-default/1", "semantica.context-50k/1"] {
        let body = serde_json::to_vec(&json!({"query":"ASK {?s ?p ?o}"})).unwrap();
        let mut request = format!(
            "POST /api/sparql HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        request.extend_from_slice(&body);
        let input_len = request.len();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream.write_all(&request).unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            let mut wire = Vec::new();
            stream.read_to_end(&mut wire).unwrap();
            wire
        });
        let mut session = DatasetSession::with_resources(&SemanticDatasetResourceSelection {
            schema: "semantica.dataset-resource-selection/1".into(),
            profile: profile.into(),
        })
        .unwrap();
        load_context_graph(
            &mut session,
            &json!({"nodes":[{"id":"a","type":"Part"}],"edges":[]}),
        )
        .unwrap();
        let before = session.snapshot().unwrap();
        let budget = NativeServiceBudget::new(1, input_len as u64, 100_000);
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut calls = 0;
        serve_connection_v2(&McpService::new(&mut session,&budget),listener.accept().unwrap().0,
            &|_|HostAuthorization::Granted,deadline,|job,seen| {
                calls+=1;assert_eq!(seen,deadline);assert_eq!(job.resources.profile,profile);
                assert_eq!(job.job.expected_dataset_sha256,before.dataset_sha256);
                Ok(SemanticExplorerSparqlExecution {
                    schema:"semantica.explorer-sparql-execution/1".into(),
                    input_sha256:semantica_runtime::sha256(serde_json::to_vec(job).unwrap()),
                    dataset_sha256:before.dataset_sha256.clone(),
                    result:json!({"columns":["result"],"rows":[{"result":"true"}],"total":1,"truncated":false,"error":null,"error_line":null,"error_column":null}),
                })
            }).unwrap();
        let wire = client.join().unwrap();
        let split = wire.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
        let head = std::str::from_utf8(&wire[..split]).unwrap();
        assert!(head.starts_with("HTTP/1.1 200 "));
        assert!(head.contains("X-Semantica-Profile: semantica.explorer-sparql-http/2"));
        let payload: Value = serde_json::from_slice(&wire[split + 4..]).unwrap();
        assert_eq!(payload["rows"], json!([{"result":"true"}]));
        let receipt = budget
            .charge_claim(&SemanticServiceBudgetCharge {
                kind: "framing".into(),
                bytes: 0,
            })
            .unwrap();
        assert_eq!(receipt.input_bytes, input_len as u64);
        assert_eq!(receipt.output_bytes, wire.len() as u64);
        assert_eq!(receipt.calls, 1);
        assert_eq!(calls, 1);
        assert_eq!(session.snapshot().unwrap(), before);
    }
}

#[test]
fn version2_http_rejects_unauthorized_or_client_resource_override_before_dispatch() {
    use semantica_service::explorer_sparql_http::serve_connection_v2;
    for (authorized, body, status) in [
        (false, json!({"query":"ASK {}"}), 401),
        (
            true,
            json!({"query":"ASK {}","resources":{"profile":"semantica.context-50k/1"}}),
            400,
        ),
    ] {
        let body = serde_json::to_vec(&body).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(
                stream,
                "POST /api/sparql HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            let mut wire = Vec::new();
            stream.read_to_end(&mut wire).unwrap();
            wire
        });
        let mut session = DatasetSession::new();
        let before = session.snapshot().unwrap();
        let budget = NativeServiceBudget::bounded();
        serve_connection_v2(
            &McpService::new(&mut session, &budget),
            listener.accept().unwrap().0,
            &|_| {
                if authorized {
                    HostAuthorization::Granted
                } else {
                    HostAuthorization::Unauthorized
                }
            },
            Instant::now() + Duration::from_secs(2),
            |_, _| panic!("refusal before child dispatch"),
        )
        .unwrap();
        let wire = client.join().unwrap();
        let head = std::str::from_utf8(&wire).unwrap();
        assert!(head.starts_with(&format!("HTTP/1.1 {status} ")));
        assert_eq!(session.snapshot().unwrap(), before);
    }
}
