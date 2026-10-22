use semantica_contracts::{
    SemanticExplorerDecisionQuery, SemanticExplorerDecisionRequest, SemanticServiceBudgetCharge,
};
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget, explorer_decisions_http::serve_connection,
    explorer_http::HostAuthorization, load_context_graph,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    time::{Duration, Instant},
};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn exchange<G: Fn() -> std::io::Result<()>>(
    s: &McpService<'_>,
    path: &str,
    auth: HostAuthorization,
    guard: &G,
) -> (Vec<u8>, usize, std::io::Result<()>) {
    exchange_method(s, "GET", path, auth, guard)
}
fn exchange_method<G: Fn() -> std::io::Result<()>>(
    s: &McpService<'_>,
    method: &str,
    path: &str,
    auth: HostAuthorization,
    guard: &G,
) -> (Vec<u8>, usize, std::io::Result<()>) {
    let raw = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let size = raw.len();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut wire = vec![];
        stream.read_to_end(&mut wire).unwrap();
        wire
    });
    let stream = listener.accept().unwrap().0;
    let result = serve_connection(s, stream, &|_| auth, deadline(), guard);
    (client.join().unwrap(), size, result)
}
fn response(raw: &[u8]) -> (u64, Value) {
    let split = raw.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let head = std::str::from_utf8(&raw[..split]).unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = &raw[split + 4..];
    let len: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(body.len(), len);
    (
        status,
        if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(body).unwrap()
        },
    )
}
fn publish(s: &mut DatasetSession, raw: &Value) {
    let mut temp = DatasetSession::new();
    load_context_graph(&mut temp, raw).unwrap();
    s.replace_named_graph("urn:semantica:context:records/1", temp.dataset())
        .unwrap();
}
#[test]
fn original_decision_routes_and_parameter_errors_run_over_tcp() {
    let c: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/explorer_decisions_observations-04.json"
    ))
    .unwrap();
    let mut raw = c["raw_graph_before"].clone();
    for edge in raw["edges"].as_array_mut().unwrap() {
        let m = edge.as_object_mut().unwrap();
        for (a, b) in [("source_id", "source"), ("target_id", "target")] {
            if let Some(v) = m.remove(a) {
                m.insert(b.into(), v);
            }
        }
    }
    let mut session = DatasetSession::new();
    load_context_graph(&mut session, &raw).unwrap();
    let budget = NativeServiceBudget::bounded();
    let (mut input, mut output) = (0, 0);
    for row in c["rows"].as_array().unwrap() {
        if row["phase"] == "literal_id_added" {
            raw["nodes"].as_array_mut().unwrap().push(json!({"id":"causal-distance","type":"decision","content":"literal route identity","properties":{"category":"literal","scenario":"route priority","confidence":0.5}}));
            publish(&mut session, &raw);
        }
        let params = row["params"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k}={}",
                    v.as_str().map(str::to_owned).unwrap_or(v.to_string())
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let target = format!(
            "{}{}{}",
            row["path"].as_str().unwrap(),
            if params.is_empty() { "" } else { "?" },
            params
        );
        let (wire, size, r) = exchange(
            &McpService::new(&mut session, &budget),
            &target,
            HostAuthorization::Granted,
            &|| Ok(()),
        );
        r.unwrap();
        let (status, body) = response(&wire);
        assert_eq!(json!(status), row["status"], "{target}");
        assert_eq!(body, row["response"], "{target}");
        input += size;
        output += wire.len();
    }
    let totals = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    assert_eq!(totals.input_bytes, input as u64);
    assert_eq!(totals.output_bytes, output as u64);
    assert_eq!(totals.calls, 17);
}
#[test]
fn original_scalar_timestamp_and_error_http_statuses_are_preserved() {
    for (plan, obs) in [
        (
            include_str!(
                "../../semantica-runtime/tests/fixtures/explorer_decisions_scalar-plan-02.json"
            ),
            include_str!(
                "../../semantica-runtime/tests/fixtures/explorer_decisions_scalar-observations-02.json"
            ),
        ),
        (
            include_str!(
                "../../semantica-runtime/tests/fixtures/explorer_decisions_timestamp-plan-03.json"
            ),
            include_str!(
                "../../semantica-runtime/tests/fixtures/explorer_decisions_timestamp-observations-03.json"
            ),
        ),
        (
            include_str!(
                "../../semantica-runtime/tests/fixtures/explorer_decisions_error-plan-06.json"
            ),
            include_str!(
                "../../semantica-runtime/tests/fixtures/explorer_decisions_error-observations-06.json"
            ),
        ),
    ] {
        let p: Value = serde_json::from_str(plan).unwrap();
        let o: Value = serde_json::from_str(obs).unwrap();
        let mut s = DatasetSession::new();
        load_context_graph(&mut s, &json!({"nodes":p["nodes"],"edges":[]})).unwrap();
        let b = NativeServiceBudget::bounded();
        for row in o["rows"].as_array().unwrap() {
            let (wire, _, r) = exchange(
                &McpService::new(&mut s, &b),
                row["path"].as_str().unwrap(),
                HostAuthorization::Granted,
                &|| Ok(()),
            );
            r.unwrap();
            let (status, body) = response(&wire);
            assert_eq!(json!(status), row["status"], "{row}");
            assert_eq!(body, row["response"], "{row}");
        }
    }
}
#[test]
fn service_identity_deadline_budget_and_http_source_refusal() {
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &json!({"nodes":[],"edges":[]})).unwrap();
    let snap = s.snapshot().unwrap();
    let q = SemanticExplorerDecisionQuery {
        schema: "semantica.explorer-decision-query/1".into(),
        operation: "list".into(),
        decision_id: None,
        category: None,
        skip: "0".into(),
        limit: 50,
    };
    let mut request = SemanticExplorerDecisionRequest {
        schema: "semantica.explorer-decision-request/1".into(),
        expected_revision: s.revision(),
        expected_dataset_sha256: snap.dataset_sha256,
        query: q,
    };
    let budget = NativeServiceBudget::bounded();
    {
        let service = McpService::new(&mut s, &budget);
        let result = service.execute_decision_read(&request, deadline()).unwrap();
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
            service
                .execute_decision_read(&request, deadline())
                .unwrap_err()
                .code,
            "service_dataset_identity_mismatch"
        );
        request.expected_revision += 1;
        assert_eq!(
            service
                .execute_decision_read(&request, deadline())
                .unwrap_err()
                .code,
            "service_revision_mismatch"
        );
        assert_eq!(
            service
                .execute_decision_read(&request, Instant::now() - Duration::from_secs(1))
                .unwrap_err()
                .code,
            "explorer_decisions_deadline_exceeded"
        );
    }
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &budget),
        "/api/decisions",
        HostAuthorization::Granted,
        &|| Err(std::io::Error::other("source changed")),
    );
    assert!(r.is_err());
    assert!(wire.is_empty());
    let denied = NativeServiceBudget::new(0, 100000, 100000);
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &denied),
        "/api/decisions",
        HostAuthorization::Granted,
        &|| Ok(()),
    );
    r.unwrap();
    assert_eq!(response(&wire).0, 503);
}
#[test]
fn original_auth_parameter_method_and_redirect_order() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_decisions_transport.json")).unwrap();
    assert_eq!(corpus["rows"].as_array().unwrap().len(), 48);
    let mut s = DatasetSession::new();
    load_context_graph(&mut s,&json!({"nodes":[{"id":"d1","type":"decision","content":"fixture","properties":{"category":"Design","confidence":0.5}}],"edges":[]})).unwrap();
    let b = NativeServiceBudget::bounded();
    for row in corpus["rows"].as_array().unwrap() {
        let auth = match row["auth"].as_str().unwrap() {
            "anonymous" => HostAuthorization::Granted,
            "unauthorized" => HostAuthorization::Unauthorized,
            _ => HostAuthorization::NotConfigured,
        };
        let method = row["method"].as_str().unwrap();
        let (wire, _, r) = exchange_method(
            &McpService::new(&mut s, &b),
            method,
            row["path"].as_str().unwrap(),
            auth,
            &|| Ok(()),
        );
        r.unwrap();
        let split = wire.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
        let head = std::str::from_utf8(&wire[..split]).unwrap();
        let (status, body) = if method == "HEAD" {
            assert!(wire[split + 4..].is_empty());
            (
                head.split_whitespace().nth(1).unwrap().parse().unwrap(),
                Value::Null,
            )
        } else {
            response(&wire)
        };
        assert_eq!(json!(status), row["status"], "{row}");
        assert_eq!(body, row["response"], "{row}");
        if status == 307 {
            let location = head
                .lines()
                .find_map(|l| l.strip_prefix("Location: "))
                .unwrap();
            assert_eq!(json!(location), row["headers"]["location"]);
        }
    }
}
#[test]
fn corrected_causal_uri_executes_original_function_without_relabeling_legacy_lookup() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/explorer_causal_distance.json"
    ))
    .unwrap();
    for row in corpus["rows"].as_array().unwrap() {
        let mut raw = row["graph"].clone();
        for e in raw["edges"].as_array_mut().unwrap() {
            let m = e.as_object_mut().unwrap();
            for (a, b) in [("source_id", "source"), ("target_id", "target")] {
                if let Some(v) = m.remove(a) {
                    m.insert(b.into(), v);
                }
            }
        }
        let mut s = DatasetSession::new();
        load_context_graph(&mut s, &raw).unwrap();
        let budget = NativeServiceBudget::bounded();
        let target = format!(
            "/api/v2/decisions/causal-distance?source={}&target={}",
            row["source"].as_str().unwrap(),
            row["target"].as_str().unwrap()
        );
        let (wire, _, r) = exchange(
            &McpService::new(&mut s, &budget),
            &target,
            HostAuthorization::Granted,
            &|| Ok(()),
        );
        r.unwrap();
        let (status, body) = response(&wire);
        assert_eq!(status, 200);
        assert_eq!(body, row["result"]);
        assert!(
            std::str::from_utf8(&wire)
                .unwrap()
                .contains("X-Semantica-Profile: semantica.explorer-causal-distance-http/2")
        );
    }
    let mut s = DatasetSession::new();
    load_context_graph(&mut s,&json!({"nodes":[{"id":"causal-distance","type":"decision","content":"literal ID"}],"edges":[]})).unwrap();
    let b = NativeServiceBudget::bounded();
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &b),
        "/api/decisions/causal-distance?source=a&target=b",
        HostAuthorization::Granted,
        &|| Ok(()),
    );
    r.unwrap();
    assert_eq!(response(&wire).1["decision_id"], "causal-distance");
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &b),
        "/api/v2/decisions/causal-distance",
        HostAuthorization::Granted,
        &|| Ok(()),
    );
    r.unwrap();
    assert_eq!(response(&wire).0, 422);
    assert_eq!(response(&wire).1["detail"].as_array().unwrap().len(), 2);
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &b),
        "/api/v2/decisions/causal-distance",
        HostAuthorization::Unauthorized,
        &|| panic!("unauthorized must not read source"),
    );
    r.unwrap();
    assert_eq!(response(&wire).0, 401);
}
#[test]
fn causal_service_has_same_source_revision_and_single_budget_ledger() {
    use semantica_contracts::{
        SemanticExplorerCausalDistanceQuery, SemanticExplorerCausalDistanceRequest,
    };
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &json!({"nodes":[{"id":"a"}],"edges":[]})).unwrap();
    let snapshot = s.snapshot().unwrap();
    let revision = s.revision();
    let q = SemanticExplorerCausalDistanceQuery {
        schema: "semantica.explorer-causal-distance-query/2".into(),
        source_id: Some("a".into()),
        target_id: Some("a".into()),
    };
    let mut request = SemanticExplorerCausalDistanceRequest {
        schema: "semantica.explorer-causal-distance-request/2".into(),
        expected_revision: revision,
        expected_dataset_sha256: snapshot.dataset_sha256,
        query: q,
    };
    let b = NativeServiceBudget::bounded();
    {
        let service = McpService::new(&mut s, &b);
        let result = service
            .execute_causal_distance(&request, deadline())
            .unwrap();
        assert_eq!(result.body["causal_path"], json!(["a"]));
        let counts = b
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
            service
                .execute_causal_distance(&request, deadline())
                .unwrap_err()
                .code,
            "service_dataset_identity_mismatch"
        );
        request.expected_revision += 1;
        assert_eq!(
            service
                .execute_causal_distance(&request, deadline())
                .unwrap_err()
                .code,
            "service_revision_mismatch"
        );
        assert!(
            service
                .execute_causal_distance(&request, Instant::now() - Duration::from_secs(1))
                .is_err()
        );
    }
    let denied = NativeServiceBudget::new(0, 10000, 10000);
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &denied),
        "/api/v2/decisions/causal-distance?source=a&target=a",
        HostAuthorization::Granted,
        &|| Ok(()),
    );
    r.unwrap();
    assert_eq!(response(&wire).0, 503);
    let (wire, _, r) = exchange(
        &McpService::new(&mut s, &b),
        "/api/v2/decisions/causal-distance?source=a&target=a",
        HostAuthorization::Granted,
        &|| Err(std::io::Error::other("source changed")),
    );
    assert!(r.is_err());
    assert!(wire.is_empty());
}
