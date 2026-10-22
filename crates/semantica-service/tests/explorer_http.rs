use oxigraph::io::RdfFormat;
use semantica_contracts::{
    SemanticExplorerGraphStats, SemanticExplorerGraphStatsPort, SemanticExplorerGraphStatsRequest,
    SemanticServiceBudgetCharge, SemanticServiceBudgetPort,
};
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget,
    explorer_http::{HostAuthorization, MAX_HEADER_BYTES, serve_connection},
    load_context_graph,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/original-explorer-stats-oracle.json")).unwrap()
}
fn inputs() -> Value {
    serde_json::from_str(include_str!("fixtures/original-explorer-stats-inputs.json")).unwrap()
}
fn ready(input: &Value) -> DatasetSession {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, input).unwrap();
    s
}
fn empty() -> DatasetSession {
    ready(&json!({"nodes":[],"edges":[]}))
}
fn request(extra: &str) -> Vec<u8> {
    format!("GET /api/graph/stats HTTP/1.1\r\nHost: localhost\r\n{extra}\r\n").into_bytes()
}
fn exchange<F: Fn(Option<&str>) -> HostAuthorization>(
    service: Option<&McpService<'_>>,
    raw: Vec<u8>,
    auth: &F,
) -> (Vec<u8>, std::io::Result<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let client = std::thread::spawn(move || {
        let reset_allowed = raw.len() > MAX_HEADER_BYTES
            || raw
                .windows(b"HTTP/1.1".len())
                .filter(|w| *w == b"HTTP/1.1")
                .count()
                > 1;
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        s.write_all(&raw).unwrap();
        s.shutdown(Shutdown::Write).unwrap();
        let mut bytes = Vec::new();
        let read = s.read_to_end(&mut bytes);
        if let Err(ref e) = read {
            eprintln!(
                "actual HTTP read terminal: {:?}; {} response bytes",
                e.kind(),
                bytes.len()
            );
            assert!(
                reset_allowed,
                "ordinary full request must reach clean EOF: {read:?}"
            );
            assert!(
                !bytes.is_empty(),
                "connection failed without response: {read:?}"
            );
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset,
                "complete response must never mask a read timeout"
            );
        }
        bytes
    });
    let (stream, _) = listener.accept().unwrap();
    let result = serve_connection(service, stream, auth, Duration::from_secs(2));
    (client.join().unwrap(), result)
}
fn response(raw: &[u8]) -> (u16, Value) {
    let at = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = std::str::from_utf8(&raw[..at]).unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    assert!(head.contains("Connection: close"));
    assert!(head.contains("X-Semantica-Profile: semantica.explorer-graph-stats-http/1"));
    assert!(head.contains("X-Semantica-Stats-Scope: canonical-context-records"));
    assert!(head.contains("X-Semantica-Historical-Index: not-reconstructed"));
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(length, raw.len() - at - 4);
    (status, serde_json::from_slice(&raw[at + 4..]).unwrap())
}
fn granted(_: Option<&str>) -> HostAuthorization {
    HostAuthorization::Granted
}

#[test]
fn original_stats_are_served_over_actual_loopback() {
    let input = inputs();
    let original = oracle();
    for case in input["cases"].as_array().unwrap() {
        let mut s = ready(case);
        let before = s.snapshot().unwrap();
        let revision = s.revision();
        let budget = NativeServiceBudget::bounded();
        let expected = original["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == case["id"])
            .unwrap();
        let service = McpService::new(&mut s, &budget);
        let (wire, r) = exchange(Some(&service), request(""), &granted);
        r.unwrap();
        let (status, json) = response(&wire);
        assert_eq!(
            status,
            expected["status"].as_u64().unwrap() as u16,
            "{}",
            case["id"]
        );
        assert_eq!(json, expected["json"], "{}", case["id"]);
        drop(service);
        assert_eq!(s.revision(), revision);
        assert_eq!(s.snapshot().unwrap(), before);
    }
}
#[test]
fn original_auth_readiness_method_and_path_outcomes_remain() {
    let original = oracle();
    for case in original["auth_readiness_routing"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let mut s = empty();
        let budget = NativeServiceBudget::bounded();
        let service = McpService::new(&mut s, &budget);
        let raw = match id {
            "wrong-method" => b"POST /api/graph/stats HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec(),
            "wrong-path" => b"GET /api/graph/missing HTTP/1.1\r\nHost: localhost\r\n\r\n".to_vec(),
            "wrong-key" => request("X-API-Key: wrong-fixture-key\r\n"),
            "valid-key" => request("X-API-Key: private-fixture-key\r\n"),
            _ => request(""),
        };
        let calls = AtomicUsize::new(0);
        let auth = |candidate: Option<&str>| {
            calls.fetch_add(1, Ordering::SeqCst);
            match id {
                "unconfigured" => HostAuthorization::NotConfigured,
                "missing-key" | "wrong-key" | "valid-key" => {
                    if candidate == Some("private-fixture-key") {
                        HostAuthorization::Granted
                    } else {
                        HostAuthorization::Unauthorized
                    }
                }
                _ => HostAuthorization::Granted,
            }
        };
        let (wire, r) = exchange(
            if id == "not-ready" {
                None
            } else {
                Some(&service)
            },
            raw,
            &auth,
        );
        r.unwrap();
        let (status, body) = response(&wire);
        assert_eq!(status, case["status"].as_u64().unwrap() as u16, "{id}");
        assert_eq!(body, case["json"], "{id}");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            if ["wrong-method", "wrong-path"].contains(&id) {
                0
            } else {
                1
            }
        );
    }
}
#[test]
fn unrelated_rdf_and_uninitialized_dataset_do_not_claim_graph_readiness() {
    for rdf in [None, Some("<urn:a> <urn:b> <urn:c> .")] {
        let mut s = DatasetSession::new();
        if let Some(rdf) = rdf {
            s.load(rdf.as_bytes(), RdfFormat::NTriples, None, None)
                .unwrap();
        }
        let before = s.snapshot().unwrap();
        let revision = s.revision();
        let budget = NativeServiceBudget::bounded();
        let service = McpService::new(&mut s, &budget);
        let (wire, r) = exchange(Some(&service), request(""), &granted);
        r.unwrap();
        assert_eq!(
            response(&wire),
            (503, json!({"detail":"GraphSession not initialized."}))
        );
        drop(service);
        assert_eq!(s.revision(), revision);
        assert_eq!(s.snapshot().unwrap(), before);
    }
}
#[test]
fn framing_rejections_and_limits_are_actual_http() {
    let cases = [
        (request("Content-Length: 0\r\nContent-Length: 0\r\n"), 400),
        (request("Content-Length: 1\r\n"), 400),
        (request("Transfer-Encoding: chunked\r\n"), 400),
        (request("Host: other\r\n"), 400),
        (request("X-API-Key: a\r\nx-api-key: b\r\n"), 400),
        (b"GET /api/graph/stats HTTP/1.1\r\n\r\n".to_vec(), 400),
        (
            b"GET /api/graph/stats HTTP/1.1\nHost: localhost\n\n".to_vec(),
            400,
        ),
        (
            b"GET /api/graph/stats HTTP/1.1\r\nHost: localhost\r\n bad: x\r\n\r\n".to_vec(),
            400,
        ),
        (
            b"GET /api/graph/stats HTTP/1.1\r\nHost: localhost\r\nBad: \xff\r\n\r\n".to_vec(),
            400,
        ),
        (vec![b'x'; MAX_HEADER_BYTES + 1], 431),
    ];
    for (raw, status) in cases {
        let mut s = empty();
        let budget = NativeServiceBudget::bounded();
        let service = McpService::new(&mut s, &budget);
        let calls = AtomicUsize::new(0);
        let auth = |_: Option<&str>| {
            calls.fetch_add(1, Ordering::SeqCst);
            HostAuthorization::Granted
        };
        let (wire, r) = exchange(Some(&service), raw, &auth);
        r.unwrap();
        assert_eq!(response(&wire).0, status);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn one_request_is_dispatched_when_client_pipelines_two() {
    let mut s = empty();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut s, &budget);
    let calls = AtomicUsize::new(0);
    let mut raw = request("");
    raw.extend(request(""));
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let (wire, r) = exchange(Some(&service), raw, &auth);
    r.unwrap();
    assert_eq!(response(&wire).0, 200);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn source_revision_and_shared_budget_refuse_without_graph_mutation() {
    let mut s = empty();
    let sha = s.snapshot().unwrap().dataset_sha256;
    let revision = s.revision();
    let before = s.snapshot().unwrap();
    let budget = NativeServiceBudget::new(3, 1000, 10000);
    let service = McpService::new(&mut s, &budget);
    let mut input = SemanticExplorerGraphStatsRequest {
        schema: "semantica.explorer-graph-stats-request/1".into(),
        expected_revision: revision + 1,
        expected_dataset_sha256: sha.clone(),
    };
    assert_eq!(
        SemanticExplorerGraphStatsPort::graph_stats(&service, &input)
            .unwrap_err()
            .code,
        "service_revision_mismatch"
    );
    input.expected_revision = revision;
    input.expected_dataset_sha256 = "a".repeat(64);
    assert_eq!(
        SemanticExplorerGraphStatsPort::graph_stats(&service, &input)
            .unwrap_err()
            .code,
        "service_dataset_identity_mismatch"
    );
    budget
        .charge(&SemanticServiceBudgetCharge {
            kind: "request".into(),
            bytes: 0,
        })
        .unwrap();
    input.expected_dataset_sha256 = sha;
    assert!(SemanticExplorerGraphStatsPort::graph_stats(&service, &input).is_err());
    let (wire, r) = exchange(Some(&service), request(""), &granted);
    r.unwrap();
    assert_eq!(response(&wire).0, 503);
    drop(service);
    assert_eq!(s.revision(), revision);
    assert_eq!(s.snapshot().unwrap(), before);
}
#[test]
fn total_header_deadline_refuses_slow_drip_without_auth_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let client = std::thread::spawn(move || {
        let mut s = TcpStream::connect(addr).unwrap();
        for b in b"GET /api/graph/stats HTTP/1.1\r\n" {
            if s.write_all(&[*b]).is_err() {
                break;
            };
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let (stream, _) = listener.accept().unwrap();
    let calls = AtomicUsize::new(0);
    let auth = |_: Option<&str>| {
        calls.fetch_add(1, Ordering::SeqCst);
        HostAuthorization::Granted
    };
    let started = std::time::Instant::now();
    let r = serve_connection(None, stream, &auth, Duration::from_millis(120));
    assert!(r.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    client.join().unwrap();
}
#[test]
fn generated_stats_maps_are_typed_and_original_constructor_difference_is_explicit() {
    let stats: SemanticExplorerGraphStats = serde_json::from_value(
        json!({"node_count":0,"edge_count":0,"node_types":{},"edge_types":{},"density":1.5}),
    )
    .unwrap();
    stats.validate().unwrap();
    for value in [
        json!({"a":"1"}),
        json!({"a":true}),
        json!({"a":-1}),
        json!([]),
    ] {
        assert!(serde_json::from_value::<SemanticExplorerGraphStats>(json!({"node_count":0,"edge_count":0,"node_types":value,"edge_types":{},"density":0.0})).is_err());
    }
    // This strict execution DTO is not a claim of original Pydantic coercion/default compatibility.
    assert_eq!(
        oracle()["constructor_boundary"][0]["result"]["accepted"],
        true
    );
    assert!(
        serde_json::from_value::<SemanticExplorerGraphStats>(
            oracle()["constructor_boundary"][0]["input"].clone()
        )
        .is_err()
    );
}

#[test]
fn fresh_host_authorization_is_not_cached_across_connections() {
    let mut s = empty();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut s, &budget);
    let permitted = std::cell::Cell::new(false);
    let auth = |_: Option<&str>| {
        if permitted.get() {
            HostAuthorization::Granted
        } else {
            HostAuthorization::Unauthorized
        }
    };
    for (decision, expected) in [(false, 401), (true, 200), (false, 401)] {
        permitted.set(decision);
        let (wire, r) = exchange(Some(&service), request(""), &auth);
        r.unwrap();
        assert_eq!(response(&wire).0, expected);
    }
}
