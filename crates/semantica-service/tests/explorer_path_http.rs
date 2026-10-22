use semantica_contracts::{
    SemanticDatasetResourceSelection, SemanticExplorerPathDetailsExecution,
    SemanticExplorerPathDetailsJob, SemanticRuntimeFailure, SemanticServiceBudgetCharge,
    SemanticServiceBudgetReceipt,
};
use semantica_runtime::{dataset::DatasetSession, sha256};
use semantica_service::{
    McpService, NativeServiceBudget, explorer_http::HostAuthorization,
    explorer_path_http::serve_connection, load_context_graph,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    time::{Duration, Instant},
};

fn compute(job: &SemanticExplorerPathDetailsJob) -> SemanticExplorerPathDetailsExecution {
    let mut child = DatasetSession::with_resources(&job.resources).unwrap();
    child
        .load(
            job.dataset_nquads.as_bytes(),
            semantica_service::rdf_format("nquads").unwrap(),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        child.snapshot().unwrap().dataset_sha256,
        job.expected_dataset_sha256
    );
    let value = semantica_runtime::explorer_path_details::execute(
        &child,
        &job.query,
        job.work_limit,
        Instant::now() + Duration::from_secs(3),
    );
    let (outcome, result, failure) = match value {
        Ok(v) => ("completed", Some(v), None),
        Err(e) => (
            "rejected",
            None,
            Some(SemanticRuntimeFailure {
                code: e.to_string(),
                phase: "path".into(),
                message: "control".into(),
                retryable: false,
            }),
        ),
    };
    SemanticExplorerPathDetailsExecution {
        schema: "semantica.explorer-path-details-execution/1".into(),
        input_sha256: sha256(serde_json::to_vec(job).unwrap()),
        dataset_sha256: job.expected_dataset_sha256.clone(),
        outcome: outcome.into(),
        result,
        failure,
    }
}
struct Observed {
    wire: Vec<u8>,
    calls: usize,
    input_len: usize,
    budget: SemanticServiceBudgetReceipt,
    error: Option<String>,
}
impl Observed {
    fn response(&self) -> (u16, Value, String) {
        let split = self.wire.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
        let header = std::str::from_utf8(&self.wire[..split])
            .unwrap()
            .to_string();
        let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = &self.wire[split + 4..];
        (
            status,
            if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(body).unwrap()
            },
            header,
        )
    }
}
fn request(
    method: &str,
    target: &str,
    auth: HostAuthorization,
    profile: &str,
    work: u64,
    forged: bool,
    output_limit: u64,
) -> Observed {
    let raw =
        format!("{method} {target} HTTP/1.1\r\nHost: localhost\r\nX-API-Key: fixture-key\r\n\r\n")
            .into_bytes();
    let input_len = raw.len();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = std::thread::spawn(move || {
        let mut s = TcpStream::connect(address).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(&raw).unwrap();
        s.shutdown(Shutdown::Write).unwrap();
        let mut wire = vec![];
        s.read_to_end(&mut wire).unwrap();
        wire
    });
    let mut session = DatasetSession::with_resources(&SemanticDatasetResourceSelection {
        schema: "semantica.dataset-resource-selection/1".into(),
        profile: profile.into(),
    })
    .unwrap();
    load_context_graph(&mut session,&json!({"nodes":[{"id":"a","type":"Part"},{"id":"b","type":"Part"},{"id":"z","type":"Part"},{"id":"isolated","type":"Part"},{"id":"a/b","type":"Part"},{"id":"a%2Fb","type":"Part"}],
        "edges":[{"id":"ab","source":"a","target":"b","weight":2.0},{"id":"bz","source":"b","target":"z","weight":3.0}]})).unwrap();
    let before = session.snapshot().unwrap();
    let revision = session.revision();
    let budget = NativeServiceBudget::new(1, input_len as u64, output_limit);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut calls = 0;
    let result = serve_connection(
        &McpService::new(&mut session, &budget),
        listener.accept().unwrap().0,
        &|key| {
            assert_eq!(key, Some("fixture-key"));
            auth
        },
        deadline,
        work,
        |job, seen| {
            calls += 1;
            assert_eq!(seen, deadline);
            assert_eq!(job.resources.profile, profile);
            assert_eq!(job.work_limit, work);
            assert_eq!(job.expected_dataset_sha256, before.dataset_sha256);
            let mut r = compute(job);
            if forged {
                r.dataset_sha256 = "0".repeat(64);
            }
            Ok(r)
        },
    );
    assert_eq!(session.snapshot().unwrap(), before);
    assert_eq!(session.revision(), revision);
    let claim = budget
        .charge_claim(&SemanticServiceBudgetCharge {
            kind: "framing".into(),
            bytes: 0,
        })
        .unwrap();
    Observed {
        wire: client.join().unwrap(),
        calls,
        input_len,
        budget: claim,
        error: result.err().map(|e| e.to_string()),
    }
}
const DEFAULT: &str = "semantica.dataset-default/1";
#[test]
fn path_http_uses_one_source_deadline_and_exact_wire_budget() {
    for profile in [DEFAULT, "semantica.context-50k/1"] {
        for target in [
            "/api/graph/path?source=a&target=z",
            "/api/graph/node/a/path?target=z&resource_profile=forged&work_limit=10000000",
        ] {
            let o = request(
                "GET",
                target,
                HostAuthorization::Granted,
                profile,
                1000,
                false,
                100_000,
            );
            assert!(o.error.is_none());
            let (status, body, header) = o.response();
            assert_eq!(status, 200);
            assert!(header.contains("X-Semantica-Profile: semantica.explorer-path-http/1"));
            assert_eq!(body["path"], json!(["a", "b", "z"]));
            assert_eq!(body["edge_ids"], json!(["ab", "bz"]));
            assert_eq!(body["algorithm"], "bfs");
            assert_eq!(body["total_weight"], 5.0);
            assert_eq!(body["confidence_decay"], 6.0);
            assert_eq!(body["bottleneck_node"], "b");
            assert_eq!(body["alternative_path_count"], 0);
            assert_eq!(
                body["metrics"]["alternative_path_count"]["status"],
                "computed"
            );
            assert_eq!(body["hop_count"], 2);
            assert_eq!(body["distance_band"], "near");
            assert_eq!(body["metrics"]["work_limit"], 1000);
            assert_eq!(o.budget.input_bytes, o.input_len as u64);
            assert_eq!(o.budget.output_bytes, o.wire.len() as u64);
            assert_eq!(o.budget.calls, 1);
            assert_eq!(o.calls, 1);
        }
    }
    for target in [
        "/api/graph/path?source=a%2Fb&target=a%2Fb",
        "/api/graph/node/a%252Fb/path?target=a%252Fb",
    ] {
        let o = request(
            "GET",
            target,
            HostAuthorization::Granted,
            DEFAULT,
            1000,
            false,
            100_000,
        );
        assert_eq!(o.response().0, 200);
        assert_eq!(o.response().1["hop_count"], 0);
    }
}
#[test]
fn path_http_preserves_unknown_metrics_and_unreachable() {
    let o = request(
        "GET",
        "/api/graph/path?source=a&target=z",
        HostAuthorization::Granted,
        DEFAULT,
        1,
        false,
        100_000,
    );
    let (status, body, _) = o.response();
    assert_eq!(status, 200);
    for key in [
        "semantic_similarity",
        "path_coherence_score",
        "bottleneck_node",
        "alternative_path_count",
    ] {
        assert!(body[key].is_null(), "{key}");
        assert_eq!(body["metrics"][key]["status"], "unavailable");
        assert!(!body["metrics"][key]["reason"].as_str().unwrap().is_empty());
    }
    assert_eq!(body["metrics"]["work_used"], 1);
    assert_eq!(body["confidence_decay"], 6.0);
    for target in ["isolated", "missing"] {
        let o = request(
            "GET",
            &format!("/api/graph/path?source=a&target={target}"),
            HostAuthorization::Granted,
            DEFAULT,
            1000,
            false,
            100_000,
        );
        assert_eq!(o.response().0, 404);
        assert_eq!(o.calls, 1);
        assert!(o.error.is_none());
    }
}
#[test]
fn path_http_authentication_precedes_parameter_errors() {
    for (method, target, auth, status, calls) in [
        (
            "GET",
            "/api/graph/path",
            HostAuthorization::Unauthorized,
            401,
            0,
        ),
        (
            "GET",
            "/api/graph/path",
            HostAuthorization::NotConfigured,
            503,
            0,
        ),
        ("GET", "/api/graph/path", HostAuthorization::Granted, 422, 0),
        (
            "GET",
            "/api/graph/path?source=%20&target=z",
            HostAuthorization::Granted,
            422,
            0,
        ),
        (
            "POST",
            "/api/graph/path",
            HostAuthorization::Unauthorized,
            405,
            0,
        ),
        (
            "GET",
            "/api/graph/path/?source=a&target=z",
            HostAuthorization::Unauthorized,
            307,
            0,
        ),
        ("GET", "/missing", HostAuthorization::Unauthorized, 404, 0),
        (
            "GET",
            "/api/graph/node/a%2Fb/path?target=z",
            HostAuthorization::Granted,
            404,
            0,
        ),
    ] {
        let o = request(method, target, auth, DEFAULT, 1000, false, 100_000);
        assert!(o.error.is_none());
        let (actual, body, header) = o.response();
        assert_eq!(actual, status);
        assert_eq!(o.calls, calls);
        assert_eq!(o.budget.calls, 0);
        assert_eq!(o.budget.input_bytes, o.input_len as u64);
        assert_eq!(o.budget.output_bytes, o.wire.len() as u64);
        if status == 307 {
            assert!(body.is_null());
            assert!(header.contains("Location: http://localhost/api/graph/path?source=a&target=z"));
        }
        if status == 405 {
            assert!(header.contains("Allow: GET"));
        }
    }
}
#[test]
fn path_http_refuses_forgery_and_output_over_budget() {
    let target = "/api/graph/path?source=a&target=z";
    let o = request(
        "GET",
        target,
        HostAuthorization::Granted,
        DEFAULT,
        1000,
        true,
        100_000,
    );
    assert_eq!(o.response().0, 503);
    assert_eq!(o.response().1["detail"], "explorer_worker_result_invalid");
    let o = request(
        "GET",
        target,
        HostAuthorization::Granted,
        DEFAULT,
        1000,
        false,
        1,
    );
    assert_eq!(o.error.as_deref(), Some("http_service_output_budget"));
    assert!(o.wire.is_empty());
    assert_eq!(o.calls, 1);
}
