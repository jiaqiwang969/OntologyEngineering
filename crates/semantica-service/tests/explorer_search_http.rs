use semantica_contracts::{
    SemanticExplorerSearchQuery, SemanticExplorerSearchRequest, SemanticServiceBudgetCharge,
};
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget, explorer_http::HostAuthorization,
    explorer_search_http::serve_connection, load_context_graph,
};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    time::{Duration, Instant},
};
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/explorer_search_tcp.json"
    ))
    .unwrap()
}
fn graph() -> Value {
    let mut raw = corpus()["raw_graph_before"].clone();
    for e in raw["edges"].as_array_mut().unwrap() {
        let m = e.as_object_mut().unwrap();
        for (a, b) in [("source_id", "source"), ("target_id", "target")] {
            let v = m.remove(a).unwrap();
            m.insert(b.into(), v);
        }
    }
    raw
}
fn replace(s: &mut DatasetSession, raw: &Value) {
    let mut tmp = DatasetSession::new();
    load_context_graph(&mut tmp, raw).unwrap();
    s.replace_named_graph("urn:semantica:context:records/1", tmp.dataset())
        .unwrap();
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn exchange<G: Fn() -> std::io::Result<()>>(
    service: &McpService<'_>,
    method: &str,
    path: &str,
    body: &Value,
    auth: HostAuthorization,
    guard: &G,
    d: Instant,
) -> (Vec<u8>, usize, std::io::Result<()>) {
    let bytes = serde_json::to_vec(body).unwrap();
    exchange_raw(
        service,
        method,
        path,
        &bytes,
        Some("application/json"),
        auth,
        guard,
        d,
    )
}
fn exchange_raw<G: Fn() -> std::io::Result<()>>(
    service: &McpService<'_>,
    method: &str,
    path: &str,
    body: &[u8],
    content_type: Option<&str>,
    auth: HostAuthorization,
    guard: &G,
    d: Instant,
) -> (Vec<u8>, usize, std::io::Result<()>) {
    let media = content_type.map_or(String::new(), |v| format!("Content-Type: {v}\r\n"));
    let mut raw = format!(
        "{method} {path} HTTP/1.1\r\nHost: testserver\r\n{media}Content-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    raw.extend(body);
    let size = raw.len();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        stream.write_all(&raw).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        sent.send(()).unwrap();
        let mut bytes = vec![];
        if let Err(e) = stream.read_to_end(&mut bytes) {
            assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset);
        }
        bytes
    });
    let stream = listener.accept().unwrap().0;
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    let result = serve_connection(service, stream, &|_| auth, d, guard);
    (client.join().unwrap(), size, result)
}
fn response(wire: &[u8]) -> (u16, Value, Vec<u8>) {
    let split = wire.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let head = std::str::from_utf8(&wire[..split]).unwrap();
    let body = &wire[split + 4..];
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let len: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(len, body.len());
    assert!(head.contains("X-Semantica-Profile: semantica.explorer-search-http/1"));
    if status == 500 {
        assert!(head.contains("Content-Type: application/json"));
    }
    if status == 405 {
        assert!(head.contains("Allow: POST"));
    }
    if status == 307 {
        assert!(head.contains("Location: http://testserver/api/graph/search"));
    }
    (
        status,
        serde_json::from_slice(body).unwrap_or(Value::Null),
        body.to_vec(),
    )
}
#[test]
fn original_128_search_requests_run_over_real_tcp() {
    let corpus = corpus();
    let mut raw = graph();
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &raw).unwrap();
    let budget = NativeServiceBudget::bounded();
    let mut input = 0;
    let mut output = 0;
    let mut checked = 0;
    let mut calls = 0;
    for row in corpus["cases"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        if name == "after_add" {
            raw["nodes"]
                .as_array_mut()
                .unwrap()
                .push(corpus["mutations"][0]["node"].clone());
            replace(&mut s, &raw);
        }
        if name == "after_update_old_query" {
            *raw["nodes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|n| n["id"] == "new")
                .unwrap() = corpus["mutations"][1]["node"].clone();
            replace(&mut s, &raw);
        }
        if name == "after_delete" {
            raw["nodes"]
                .as_array_mut()
                .unwrap()
                .retain(|n| n["id"] != "new");
            replace(&mut s, &raw);
        }
        let before = s.snapshot().unwrap();
        let guards = Cell::new(0);
        let (wire, size, r) = exchange(
            &McpService::new(&mut s, &budget),
            row["method"].as_str().unwrap(),
            row["path"].as_str().unwrap(),
            &row["body"],
            HostAuthorization::Granted,
            &|| {
                guards.set(guards.get() + 1);
                Ok(())
            },
            deadline(),
        );
        r.unwrap_or_else(|e| panic!("{name}: {e}"));
        let (status, body, raw_body) = response(&wire);
        assert_eq!(status as u64, row["status"], "{name}");
        assert_eq!(body, row["response"], "{name}");
        if status == 500 {
            assert_eq!(raw_body, row["raw_utf8"].as_str().unwrap().as_bytes());
        }
        if [200, 500].contains(&status) {
            assert_eq!(guards.get(), 2);
            calls += 1;
        } else {
            assert_eq!(guards.get(), 0);
        }
        assert_eq!(s.snapshot().unwrap(), before);
        input += size;
        output += wire.len();
        checked += 1;
    }
    assert_eq!(checked, 128);
    assert_eq!(calls, 113);
    let counts = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(counts.input_bytes, input as u64);
    assert_eq!(counts.output_bytes, output as u64);
    assert_eq!(counts.calls, calls);
}
#[test]
fn typed_identity_budget_and_deadline_are_owned_by_original_service() {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &graph()).unwrap();
    let before = s.snapshot().unwrap();
    let mut request = SemanticExplorerSearchRequest {
        schema: "semantica.explorer-search-request/1".into(),
        expected_revision: s.revision(),
        expected_dataset_sha256: before.dataset_sha256.clone(),
        query: SemanticExplorerSearchQuery {
            schema: "semantica.explorer-search-query/1".into(),
            body: json!({"query":"bearing"}),
        },
    };
    let budget = NativeServiceBudget::bounded();
    let result = McpService::new(&mut s, &budget)
        .execute_search(&request, deadline())
        .unwrap();
    assert_eq!(result.body["total"], 5);
    let counts = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(counts.calls, 1);
    assert_eq!(
        counts.input_bytes,
        serde_json::to_vec(&request).unwrap().len() as u64
    );
    assert_eq!(
        counts.output_bytes,
        serde_json::to_vec(&result).unwrap().len() as u64
    );
    request.expected_dataset_sha256 = "0".repeat(64);
    assert_eq!(
        McpService::new(&mut s, &budget)
            .execute_search(&request, deadline())
            .unwrap_err()
            .code,
        "service_dataset_identity_mismatch"
    );
    request.expected_dataset_sha256 = before.dataset_sha256;
    replace(&mut s, &graph());
    assert_eq!(
        McpService::new(&mut s, &budget)
            .execute_search(&request, deadline())
            .unwrap_err()
            .code,
        "service_revision_mismatch"
    );
    request.expected_revision = s.revision();
    request.expected_dataset_sha256 = s.snapshot().unwrap().dataset_sha256;
    let none = NativeServiceBudget::new(0, 100000, 100000);
    assert!(
        McpService::new(&mut s, &none)
            .execute_search(&request, deadline())
            .is_err()
    );
    assert!(
        McpService::new(&mut s, &budget)
            .execute_search(&request, Instant::now() - Duration::from_secs(1))
            .unwrap_err()
            .code
            .contains("deadline")
    );
}
#[test]
fn authentication_source_checks_and_wire_budget_do_not_publish_invalid_success() {
    for (method, path, auth, status) in [
        (
            "POST",
            "/api/graph/search",
            HostAuthorization::Unauthorized,
            401,
        ),
        (
            "POST",
            "/api/graph/search",
            HostAuthorization::NotConfigured,
            503,
        ),
        (
            "GET",
            "/api/graph/search",
            HostAuthorization::Unauthorized,
            405,
        ),
        (
            "POST",
            "/api/graph/search/",
            HostAuthorization::Unauthorized,
            307,
        ),
        ("POST", "/missing", HostAuthorization::Unauthorized, 404),
    ] {
        let mut s = DatasetSession::new();
        load_context_graph(&mut s, &graph()).unwrap();
        let budget = NativeServiceBudget::bounded();
        let guards = Cell::new(0);
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            method,
            path,
            &json!({}),
            auth,
            &|| {
                guards.set(guards.get() + 1);
                Ok(())
            },
            deadline(),
        );
        r.unwrap();
        assert_eq!(response(&wire).0, status);
        assert_eq!(guards.get(), 0);
    }
    for failure_at in [1, 2] {
        let mut s = DatasetSession::new();
        load_context_graph(&mut s, &graph()).unwrap();
        let budget = NativeServiceBudget::bounded();
        let guards = Cell::new(0);
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            "POST",
            "/api/graph/search",
            &json!({"query":"bearing"}),
            HostAuthorization::Granted,
            &|| {
                guards.set(guards.get() + 1);
                if guards.get() == failure_at {
                    Err(std::io::Error::other("source_changed"))
                } else {
                    Ok(())
                }
            },
            deadline(),
        );
        assert_eq!(r.unwrap_err().to_string(), "source_changed");
        assert!(wire.is_empty());
    }
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &graph()).unwrap();
    let budget = NativeServiceBudget::new(1, 10000, 0);
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &budget),
        "POST",
        "/api/graph/search",
        &json!({"query":"bearing"}),
        HostAuthorization::Granted,
        &|| Ok(()),
        deadline(),
    );
    assert!(r.is_err());
    assert!(wire.is_empty());
}
#[test]
fn additional_original_nonfinite_inputs_run_over_real_tcp() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/explorer_search_nonfinite.json"
    ))
    .unwrap();
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":[{"id":"a","type":"Part","properties":{"content":"bearing"}}],"edges":[]}),
    )
    .unwrap();
    let budget = NativeServiceBudget::bounded();
    for row in corpus["rows"].as_array().unwrap() {
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            "POST",
            "/api/graph/search",
            &row["body"],
            HostAuthorization::Granted,
            &|| Ok(()),
            deadline(),
        );
        r.unwrap();
        let (status, body, _) = response(&wire);
        assert_eq!(json!(status), row["status"]);
        assert_eq!(body, row["response"]);
    }
}

#[test]
fn original_body_decode_authentication_order_and_json_diagnostics() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_search_transport.json")).unwrap();
    let syntax: Value =
        serde_json::from_str(include_str!("fixtures/explorer_search_json.json")).unwrap();
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":[{"id":"a","type":"Part","properties":{"content":"bearing"}}],"edges":[]}),
    )
    .unwrap();
    let budget = NativeServiceBudget::bounded();
    let mut checked = 0;
    for row in corpus["rows"]
        .as_array()
        .unwrap()
        .iter()
        .chain(syntax["rows"].as_array().unwrap())
    {
        let auth = match row["auth"].as_str() {
            Some("unauthorized") => HostAuthorization::Unauthorized,
            Some("unconfigured") => HostAuthorization::NotConfigured,
            _ => HostAuthorization::Granted,
        };
        let media = if row.get("content_type").is_some() {
            row["content_type"].as_str()
        } else {
            Some("application/json")
        };
        let (wire, _, r) = exchange_raw(
            &McpService::new(&mut s, &budget),
            row["method"].as_str().unwrap_or("POST"),
            row["path"].as_str().unwrap_or("/api/graph/search"),
            row["body_utf8"].as_str().unwrap().as_bytes(),
            media,
            auth,
            &|| Ok(()),
            deadline(),
        );
        r.unwrap();
        let (status, body, _) = response(&wire);
        assert_eq!(json!(status), row["status"], "{row}");
        assert_eq!(body, row["response"], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 52);
}

#[test]
fn original_numeric_request_and_error_values_are_preserved() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_search_numeric.json")).unwrap();
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":[{"id":"a","type":"Part","properties":{"content":"bearing"}}],"edges":[]}),
    )
    .unwrap();
    let budget = NativeServiceBudget::bounded();
    assert_eq!(corpus["rows"].as_array().unwrap().len(), 10);
    for row in corpus["rows"].as_array().unwrap() {
        let (wire, _, r) = exchange_raw(
            &McpService::new(&mut s, &budget),
            "POST",
            "/api/graph/search",
            row["body_utf8"].as_str().unwrap().as_bytes(),
            Some("application/json"),
            HostAuthorization::Granted,
            &|| Ok(()),
            deadline(),
        );
        r.unwrap();
        let (status, body, _) = response(&wire);
        assert_eq!(json!(status), row["status"]);
        assert_eq!(body, row["response"], "{}", row["body_utf8"]);
    }
}
#[test]
fn search_wire_input_call_and_expired_deadline_refuse_without_reset() {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &graph()).unwrap();
    for (calls, input, expected_status) in [(0, 100000, Some(503)), (1, 0, None)] {
        let budget = NativeServiceBudget::new(calls, input, 100000);
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            "POST",
            "/api/graph/search",
            &json!({"query":"bearing"}),
            HostAuthorization::Granted,
            &|| Ok(()),
            deadline(),
        );
        if let Some(status) = expected_status {
            r.unwrap();
            assert_eq!(response(&wire).0, status);
        } else {
            assert!(r.is_err());
            assert!(wire.is_empty());
        }
    }
    let budget = NativeServiceBudget::bounded();
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &budget),
        "POST",
        "/api/graph/search",
        &json!({"query":"bearing"}),
        HostAuthorization::Granted,
        &|| Ok(()),
        Instant::now() - Duration::from_secs(1),
    );
    assert!(r.is_err());
    assert!(wire.is_empty());
}
