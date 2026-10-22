//! Actual loopback HTTP contract tests. All credentials are invented fixtures;
//! these tests do not use a provider, a real account or an engineering oracle.
#![cfg(unix)]

use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use oe_engineering::model_assistance::transport::{
    HttpModelTransport, ModelTransport, endpoint_identity,
};
use serde_json::{Value, json};

fn fake_key() -> &'static str {
    static KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    KEY.get_or_init(|| ["sk-", "fake-private-model-transport-fixture"].concat())
        .as_str()
}
static NEXT: AtomicU64 = AtomicU64::new(0);

struct PrivateFixture {
    root: PathBuf,
    key: PathBuf,
}
impl PrivateFixture {
    fn new(key: &str) -> Self {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = base.join(format!(
            "oev2-model-transport-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let file = root.join("credential.txt");
        fs::write(&file, format!("{key}\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        Self { root, key: file }
    }
}
impl Drop for PrivateFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Debug)]
struct CapturedRequest {
    first_line: String,
    headers: String,
    body: Vec<u8>,
}
struct Mock {
    endpoint: String,
    captured: Arc<Mutex<Option<CapturedRequest>>>,
    thread: Option<JoinHandle<()>>,
}
impl Mock {
    fn new(status: &str, extra_headers: &str, body: Vec<u8>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!(
            "http://{}/actual/v1/responses",
            listener.local_addr().unwrap()
        );
        let captured = Arc::new(Mutex::new(None));
        let target = Arc::clone(&captured);
        let status = status.to_owned();
        let extra_headers = extra_headers.to_owned();
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let request = receive_request(&mut stream);
            *target.lock().unwrap() = Some(request);
            thread::sleep(delay);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
        });
        Self {
            endpoint,
            captured,
            thread: Some(thread),
        }
    }
    fn finish(mut self) -> CapturedRequest {
        self.thread.take().unwrap().join().unwrap();
        self.captured.lock().unwrap().clone().unwrap()
    }
}
fn receive_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut raw = Vec::new();
    let header_end = loop {
        let mut data = [0u8; 1024];
        let n = stream.read(&mut data).unwrap();
        assert!(n > 0);
        raw.extend_from_slice(&data[..n]);
        if let Some(i) = raw.windows(4).position(|v| v == b"\r\n\r\n") {
            break i + 4;
        }
        assert!(raw.len() < 65_536);
    };
    let headers = String::from_utf8(raw[..header_end].to_vec()).unwrap();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    while raw.len() - header_end < length {
        let mut data = [0u8; 8192];
        let n = stream.read(&mut data).unwrap();
        assert!(n > 0);
        raw.extend_from_slice(&data[..n]);
    }
    CapturedRequest {
        first_line: headers.lines().next().unwrap().into(),
        headers,
        body: raw[header_end..header_end + length].to_vec(),
    }
}
fn payload() -> Value {
    json!({"model":"fake-model-2026", "store":false, "truncation":"disabled", "input":[{"role":"user","content":"保留载荷未知、原条件与全部资料，不能伪造工程完成。"}], "max_output_tokens":64})
}
fn transport(endpoint: &str, fixture: &PrivateFixture, limit: u64) -> HttpModelTransport {
    HttpModelTransport::new(endpoint, &fixture.key, Duration::from_secs(2), limit).unwrap()
}

#[test]
fn posts_exact_request_and_keeps_incomplete_provider_status() {
    let fixture = PrivateFixture::new(fake_key());
    let response = json!({"id":"resp_fixture", "status":"incomplete", "incomplete_details":{"reason":"max_output_tokens"}, "output":[], "usage":{"input_tokens":123,"output_tokens":4}});
    let mock = Mock::new(
        "200 OK",
        "Content-Type: application/json\r\n",
        serde_json::to_vec(&response).unwrap(),
        Duration::ZERO,
    );
    let client = transport(&mock.endpoint, &fixture, 10_000);
    let identity = client.identity();
    assert_eq!(client.send(&payload()).unwrap(), response);
    let observation = client.observation();
    assert_eq!(observation["send_call_attempts"], 1);
    assert_eq!(observation["client_dispatch_attempts"], 1);
    assert_eq!(observation["observed_http_attempts"], 1);
    assert_eq!(observation["attempts"][0]["http_status"], 200);
    assert_eq!(observation["attempts"][0]["response_read_status"], "read");
    assert_eq!(observation["attempts"][0]["response_json_status"], "object");
    assert_eq!(observation["upstream_attempts"], "unknown");
    assert!(
        !serde_json::to_string(&observation)
            .unwrap()
            .contains(fake_key())
    );
    assert!(
        !serde_json::to_string(&observation)
            .unwrap()
            .contains(&mock.endpoint)
    );
    assert_eq!(identity, endpoint_identity(&mock.endpoint).unwrap());
    assert!(identity.starts_with("responses-http/v1:"));
    assert!(!identity.contains(&mock.endpoint));
    let debug = format!("{client:?}");
    assert!(!debug.contains(fake_key()));
    assert!(!debug.contains(&mock.endpoint));
    let request = mock.finish();
    assert_eq!(request.first_line, "POST /actual/v1/responses HTTP/1.1");
    let frozen_wire = oe_core::json::ordered_json(&payload()).unwrap();
    assert_eq!(request.body, frozen_wire);
    assert_eq!(
        observation["attempts"][0]["request_sha256"],
        oe_core::sha256(&request.body)
    );
    assert_eq!(
        observation["attempts"][0]["request_bytes"],
        request.body.len()
    );
    assert_eq!(
        oe_core::sha256(&request.body),
        oe_core::sha256(&frozen_wire)
    );
    assert!(
        request
            .headers
            .to_ascii_lowercase()
            .contains(&["authorization: bearer ", fake_key()].concat())
    );
    assert!(
        request
            .headers
            .to_ascii_lowercase()
            .contains("content-type: application/json")
    );
}

#[test]
fn accepts_explicit_private_bearer_without_account_discovery() {
    let fixture = PrivateFixture::new("ordinary_private_fixture_only");
    let mock = Mock::new(
        "200 OK",
        "",
        b"{\"status\":\"completed\"}".to_vec(),
        Duration::ZERO,
    );
    let client = transport(&mock.endpoint, &fixture, 1024);
    assert_eq!(client.send(&payload()).unwrap()["status"], "completed");
    let captured = mock.finish();
    assert!(
        captured
            .headers
            .to_ascii_lowercase()
            .contains("authorization: bearer ordinary_private_fixture_only")
    );
    assert_eq!(
        captured.body,
        oe_core::json::ordered_json(&payload()).unwrap()
    );
}

#[test]
fn rejects_redirect_without_dispatching_to_destination() {
    let fixture = PrivateFixture::new(fake_key());
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    destination.set_nonblocking(true).unwrap();
    let location = format!(
        "Location: http://{}/private\r\n",
        destination.local_addr().unwrap()
    );
    let mock = Mock::new(
        "302 Found",
        &location,
        b"redirect body".to_vec(),
        Duration::ZERO,
    );
    let client = transport(&mock.endpoint, &fixture, 1024);
    let err = client.send(&payload()).unwrap_err();
    assert_eq!(err.code, "http_error");
    assert_eq!(err.http_status, Some(302));
    assert_eq!(client.observation()["attempts"][0]["http_status"], 302);
    assert_eq!(client.observation()["observed_http_attempts"], 1);
    assert!(err.dispatch_attempted);
    mock.finish();
    assert_eq!(
        destination.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn http_400_retains_status_without_echoing_private_body_or_endpoint() {
    let fixture = PrivateFixture::new(fake_key());
    let body = format!(
        "{{\"error\":\"{FAKE_KEY} private project evidence\"}}",
        FAKE_KEY = fake_key()
    );
    let mock = Mock::new("400 Bad Request", "", body.into_bytes(), Duration::ZERO);
    let client = transport(&mock.endpoint, &fixture, 1024);
    let err = client.send(&payload()).unwrap_err();
    assert_eq!(err.code, "http_error");
    assert_eq!(err.http_status, Some(400));
    let observation = client.observation();
    assert_eq!(observation["client_dispatch_attempts"], 1);
    assert_eq!(observation["observed_http_attempts"], 1);
    assert_eq!(observation["attempts"][0]["http_status"], 400);
    assert_eq!(
        observation["attempts"][0]["response_read_status"],
        "not_read_error_status"
    );
    assert_eq!(
        observation["attempts"][0]["response_bytes_read"],
        Value::Null
    );
    assert!(
        !serde_json::to_string(&observation)
            .unwrap()
            .contains(fake_key())
    );
    assert!(
        !serde_json::to_string(&observation)
            .unwrap()
            .contains("private project evidence")
    );
    assert!(err.dispatch_attempted);
    let safe = format!("{err:?} {err} {}", serde_json::to_string(&err).unwrap());
    assert!(!safe.contains(fake_key()));
    assert!(!safe.contains("private project evidence"));
    assert!(!safe.contains(&mock.endpoint));
    mock.finish();
}

#[test]
fn bounds_success_body_and_refuses_malformed_or_reflected_credentials() {
    let fixture = PrivateFixture::new(fake_key());
    for (body, limit, expected) in [
        (vec![b' '; 101], 100, "response_too_large"),
        (b"not json".to_vec(), 1024, "malformed_response"),
        (
            b"{\"status\":\"completed\",\"status\":\"failed\"}".to_vec(),
            1024,
            "malformed_response",
        ),
        (b"[]".to_vec(), 1024, "malformed_response"),
        (
            format!("{{\"output\":\"{FAKE_KEY}\"}}", FAKE_KEY = fake_key()).into_bytes(),
            1024,
            "credential_material_in_response",
        ),
        (
            format!("{{\"output\":\"{}\"}}", fake_key().replace('s', "\\u0073")).into_bytes(),
            1024,
            "credential_material_in_response",
        ),
    ] {
        let mock = Mock::new("200 OK", "", body, Duration::ZERO);
        let error = transport(&mock.endpoint, &fixture, limit)
            .send(&payload())
            .unwrap_err();
        assert_eq!(error.code, expected);
        assert!(error.dispatch_attempted);
        assert_eq!(error.http_status, Some(200));
        assert!(!serde_json::to_string(&error).unwrap().contains(fake_key()));
        mock.finish();
    }
}

#[test]
fn timeout_is_one_attempt_and_does_not_return_provider_prose() {
    let fixture = PrivateFixture::new(fake_key());
    let mock = Mock::new(
        "200 OK",
        "",
        b"{\"status\":\"completed\"}".to_vec(),
        Duration::from_millis(350),
    );
    let client = HttpModelTransport::new(
        &mock.endpoint,
        &fixture.key,
        Duration::from_millis(60),
        1024,
    )
    .unwrap();
    let error = client.send(&payload()).unwrap_err();
    assert_eq!(error.code, "timeout");
    assert!(error.dispatch_attempted);
    assert_eq!(error.http_status, None);
    let observation = client.observation();
    assert_eq!(observation["client_dispatch_attempts"], 1);
    assert_eq!(observation["observed_http_attempts"], 0);
    assert_eq!(observation["attempts"][0]["http_status"], Value::Null);
    assert_eq!(observation["attempts"][0]["safe_error"], "timeout");
    assert_eq!(
        observation["attempts"][0]["response_read_status"],
        "not_started"
    );
    let request = mock.finish();
    assert_eq!(
        request.body,
        oe_core::json::ordered_json(&payload()).unwrap()
    );
}

#[test]
fn credentials_require_private_owner_mode_and_exact_single_bearer() {
    let fixture = PrivateFixture::new(fake_key());
    fs::set_permissions(&fixture.key, fs::Permissions::from_mode(0o644)).unwrap();
    let error = HttpModelTransport::new(
        "http://127.0.0.1:1/v1/responses",
        &fixture.key,
        Duration::from_secs(1),
        1024,
    )
    .unwrap_err();
    assert_eq!(error.code, "credential_file_requires_owner_and_mode_0600");
    assert!(!error.dispatch_attempted);
    fs::set_permissions(&fixture.key, fs::Permissions::from_mode(0o600)).unwrap();
    for content in [
        "OPENAI_API_KEY=sk-fake-key",
        "first-key\nsecond-key",
        "Bearer sk-fake-key",
        "",
    ] {
        fs::write(&fixture.key, content).unwrap();
        let err = HttpModelTransport::new(
            "http://127.0.0.1:1/v1/responses",
            &fixture.key,
            Duration::from_secs(1),
            1024,
        )
        .unwrap_err();
        assert_eq!(err.code, "credential_file_requires_one_bearer");
        assert!(!err.dispatch_attempted);
    }
}

#[test]
fn refuses_symlink_credentials_and_insecure_or_secret_endpoints() {
    let fixture = PrivateFixture::new(fake_key());
    let linked = fixture.root.join("linked.txt");
    symlink(&fixture.key, &linked).unwrap();
    assert_eq!(
        HttpModelTransport::new(
            "http://127.0.0.1:1/v1/responses",
            &linked,
            Duration::from_secs(1),
            1024
        )
        .unwrap_err()
        .code,
        "credential_file_unavailable"
    );
    for endpoint in [
        "http://example.com/v1/responses",
        "http://localhost/v1/responses",
        "ftp://127.0.0.1/private",
        "https://user:secret@example.com/v1/responses",
        "https://example.com/v1/responses?key=secret",
        "https://example.com/v1/responses#secret",
    ] {
        let err = HttpModelTransport::new(endpoint, &fixture.key, Duration::from_secs(1), 1024)
            .unwrap_err();
        assert_eq!(err.code, "invalid_endpoint");
        assert!(!err.dispatch_attempted);
        assert!(!serde_json::to_string(&err).unwrap().contains(endpoint));
    }
    for (timeout, limit) in [
        (Duration::ZERO, 1024),
        (Duration::from_secs(1), 0),
        (Duration::from_secs(1), u64::MAX),
    ] {
        assert_eq!(
            HttpModelTransport::new(
                "https://example.com/v1/responses",
                &fixture.key,
                timeout,
                limit
            )
            .unwrap_err()
            .code,
            "invalid_transport_limits"
        );
    }
}

#[test]
fn credential_in_request_is_rejected_before_client_dispatch() {
    let fixture = PrivateFixture::new(fake_key());
    let client = transport("http://127.0.0.1:1/v1/responses", &fixture, 1024);
    let err = client.send(&json!({"input":fake_key()})).unwrap_err();
    assert_eq!(err.code, "credential_material_in_request");
    assert!(!err.dispatch_attempted);
    let observation = client.observation();
    assert_eq!(observation["send_call_attempts"], 1);
    assert_eq!(observation["client_dispatch_attempts"], 0);
    assert_eq!(observation["observed_http_attempts"], 0);
    assert_eq!(
        observation["attempts"][0]["request_status"],
        "credential_rejected"
    );
    assert_eq!(observation["attempts"][0]["request_sha256"], Value::Null);
}

#[test]
fn actual_wire_matches_frozen_shared_numeric_representation() {
    let fixture = PrivateFixture::new(fake_key());
    // This is a transport Value formatting counterexample, not an assertion
    // that these diagnostic-only extra fields are accepted by a live provider.
    let request: Value = serde_json::from_str(
        r#"{"model":"fixture-model","input":"preserve unknowns","temperature":1e-5,"negative_zero":-0,"large_value":1e30}"#,
    ).unwrap();
    let frozen = oe_core::json::ordered_json(&request).unwrap();
    assert_ne!(frozen, serde_json::to_vec(&request).unwrap());
    let mock = Mock::new(
        "200 OK",
        "",
        b"{\"status\":\"incomplete\"}".to_vec(),
        Duration::ZERO,
    );
    assert_eq!(
        transport(&mock.endpoint, &fixture, 1024)
            .send(&request)
            .unwrap()["status"],
        "incomplete"
    );
    let captured = mock.finish();
    assert_eq!(captured.body, frozen);
    assert_eq!(oe_core::sha256(&captured.body), oe_core::sha256(&frozen));
    assert_eq!(
        oe_core::json::strict_json(&captured.body).unwrap(),
        oe_core::json::strict_json(&frozen).unwrap()
    );
}
