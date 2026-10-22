use semantica_contracts::{SemanticExplorerGraphReadRequest, SemanticServiceBudgetCharge};
use semantica_runtime::{dataset::DatasetSession, sha256};
use semantica_service::{
    McpService, NativeServiceBudget, explorer_http::HostAuthorization,
    explorer_read_http::serve_connection, load_context_graph,
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
        "../../semantica-runtime/tests/fixtures/explorer_read_tcp.json"
    ))
    .unwrap()
}
fn session() -> DatasetSession {
    let mut raw = corpus()["raw_graph_before"].clone();
    for edge in raw["edges"].as_array_mut().unwrap() {
        let m = edge.as_object_mut().unwrap();
        for (a, b) in [("source_id", "source"), ("target_id", "target")] {
            let v = m.remove(a).unwrap();
            m.insert(b.into(), v);
        }
    }
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &raw).unwrap();
    s
}
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn target(case: &Value) -> String {
    let pairs: Vec<(String, String)> = if let Some(a) = case["params"].as_array() {
        a.iter()
            .map(|v| (v[0].as_str().unwrap().into(), v[1].as_str().unwrap().into()))
            .collect()
    } else {
        case["params"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string()),
                )
            })
            .collect()
    };
    let query = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "{}{}{}",
        case["path"].as_str().unwrap(),
        if query.is_empty() { "" } else { "?" },
        query
    )
}
fn exchange<G: Fn() -> std::io::Result<()>>(
    service: &McpService<'_>,
    method: &str,
    target: &str,
    auth: HostAuthorization,
    guard: &G,
    deadline: Instant,
) -> (Vec<u8>, usize, std::io::Result<()>) {
    let raw = format!("{method} {target} HTTP/1.1\r\nHost: testserver\r\n\r\n").into_bytes();
    let size = raw.len();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
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
    let result = serve_connection(service, stream, &|_| auth, deadline, guard);
    (client.join().unwrap(), size, result)
}
fn response(wire: &[u8]) -> (u16, Value) {
    let split = wire.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
    let head = std::str::from_utf8(&wire[..split]).unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let bytes = &wire[split + 4..];
    let length: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(length, bytes.len());
    assert!(head.contains("X-Semantica-Profile: semantica.explorer-graph-read-http/1"));
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(bytes).unwrap()
        },
    )
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
fn insert_original_mutation(s: &mut DatasetSession) {
    use oxigraph::model::Literal;
    let node = corpus()["raw_graph_after"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "aa")
        .unwrap()
        .clone();
    let subject = format!("urn:semantica:context:node:{}", sha256("aa"));
    let record = Literal::new_simple_literal(serde_json::to_string(&node).unwrap());
    s.update(&format!("INSERT DATA {{ GRAPH <urn:semantica:context:records/1> {{ <{subject}> <urn:semantica:context:nodeId> \"aa\"; <urn:semantica:context:nodeType> \"Part\"; <urn:semantica:context:nodeRecord> {record}; <urn:semantica:context:recordOrder> \"7\" . }} }}")).unwrap();
}
#[test]
fn all_original_57_reads_run_over_real_tcp_with_one_session_and_budget() {
    let mut s = session();
    let budget = NativeServiceBudget::bounded();
    let mut input = 0;
    let mut output = 0;
    let mut checked = 0;
    for case in corpus()["cases"].as_array().unwrap() {
        if case["name"] == "after_mutation_cursor" {
            insert_original_mutation(&mut s);
        }
        let before = s.snapshot().unwrap();
        let rev = s.revision();
        let guards = Cell::new(0);
        let (wire, size, result) = exchange(
            &McpService::new(&mut s, &budget),
            "GET",
            &target(case),
            HostAuthorization::Granted,
            &|| {
                guards.set(guards.get() + 1);
                Ok(())
            },
            deadline(),
        );
        result.unwrap();
        let (status, body) = response(&wire);
        assert_eq!(
            status as u64,
            case["status"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(body, case["body"], "{}", case["name"]);
        assert_eq!(s.snapshot().unwrap(), before);
        assert_eq!(s.revision(), rev);
        if status == 200 {
            assert_eq!(guards.get(), 2);
        }
        input += size;
        output += wire.len();
        checked += 1;
    }
    assert_eq!(checked, 57);
    let counts = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(counts.input_bytes, input as u64);
    assert_eq!(counts.output_bytes, output as u64);
    // Four parameter errors and the slash route are rejected before execution.
    assert_eq!(counts.calls, 52);
}
#[test]
fn authentication_route_order_and_source_changes_prevent_success() {
    for (method, target, auth, status) in [
        (
            "GET",
            "/api/graph/nodes?limit=0",
            HostAuthorization::Unauthorized,
            401,
        ),
        (
            "GET",
            "/api/graph/nodes",
            HostAuthorization::NotConfigured,
            503,
        ),
        (
            "POST",
            "/api/graph/nodes",
            HostAuthorization::Unauthorized,
            405,
        ),
        (
            "GET",
            "/api/graph/nodes/",
            HostAuthorization::Unauthorized,
            307,
        ),
        ("GET", "/missing", HostAuthorization::Unauthorized, 404),
    ] {
        let mut s = session();
        let budget = NativeServiceBudget::bounded();
        let guards = Cell::new(0);
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            method,
            target,
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
        assert_eq!(
            budget
                .charge_claim(&SemanticServiceBudgetCharge {
                    kind: "framing".into(),
                    bytes: 0
                })
                .unwrap()
                .calls,
            0
        );
    }
    for failure_at in [1, 2] {
        let mut s = session();
        let before = s.snapshot().unwrap();
        let budget = NativeServiceBudget::bounded();
        let guards = Cell::new(0);
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            "GET",
            "/api/graph/nodes",
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
        assert_eq!(s.snapshot().unwrap(), before);
    }
}
#[test]
fn stale_typed_source_binding_and_wire_budget_exhaustion_are_rejected() {
    use semantica_service::explorer_read_route::{Parsed, parse_target};
    let mut s = session();
    let before = s.snapshot().unwrap();
    let Parsed::Query(query) = parse_target("GET", "/api/graph/nodes", "testserver") else {
        panic!()
    };
    let mut request = SemanticExplorerGraphReadRequest {
        schema: "semantica.explorer-graph-read-request/1".into(),
        expected_revision: s.revision(),
        expected_dataset_sha256: before.dataset_sha256,
        query,
    };
    let budget = NativeServiceBudget::bounded();
    request.expected_dataset_sha256 = "0".repeat(64);
    assert_eq!(
        McpService::new(&mut s, &budget)
            .execute_graph_read(&request, deadline())
            .unwrap_err()
            .code,
        "service_dataset_identity_mismatch"
    );
    request.expected_dataset_sha256 = s.snapshot().unwrap().dataset_sha256;
    insert_original_mutation(&mut s);
    assert_eq!(
        McpService::new(&mut s, &budget)
            .execute_graph_read(&request, deadline())
            .unwrap_err()
            .code,
        "service_revision_mismatch"
    );
    for (calls, bytes, status) in [(0, 100000, Some(503)), (1, 0, None)] {
        let budget = NativeServiceBudget::new(calls, 100000, bytes);
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            "GET",
            "/api/graph/nodes",
            HostAuthorization::Granted,
            &|| Ok(()),
            deadline(),
        );
        if let Some(status) = status {
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
        "GET",
        "/api/graph/nodes",
        HostAuthorization::Granted,
        &|| Ok(()),
        Instant::now() - Duration::from_secs(1),
    );
    assert!(r.is_err());
    assert!(wire.is_empty());
}

#[test]
fn original_integer_and_auth_response_corpus() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/explorer_read_parameters.json")).unwrap();
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":[{"id":"a","type":"Part","content":"A"}],"edges":[]}),
    )
    .unwrap();
    let budget = NativeServiceBudget::bounded();
    let before = s.snapshot().unwrap();
    let mut count = 0;
    for case in cases["cases"].as_array().unwrap() {
        let auth = match case["auth"].as_str().unwrap() {
            "anonymous" | "valid" => HostAuthorization::Granted,
            "unconfigured" => HostAuthorization::NotConfigured,
            _ => HostAuthorization::Unauthorized,
        };
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            case["method"].as_str().unwrap(),
            &target(case),
            auth,
            &|| Ok(()),
            deadline(),
        );
        r.unwrap();
        let (status, body) = response(&wire);
        assert_eq!(
            status as u64,
            case["status"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(body, case["body"], "{}", case["name"]);
        count += 1;
    }
    assert_eq!(count, 79);
    assert_eq!(s.snapshot().unwrap(), before);
}
