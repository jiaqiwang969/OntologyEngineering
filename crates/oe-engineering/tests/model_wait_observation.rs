//! Independent literal-loopback wait-stage fixtures. No real provider or key.
#![cfg(unix)]
use oe_engineering::model_assistance::transport::{HttpModelTransport, ModelTransport};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const DUMMY: &str = "synthetic-wait-observation-dummy-only";
const TIMEOUT: Duration = Duration::from_millis(180);
const DELAY: Duration = Duration::from_millis(500);

struct Fixture {
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        let path = dir.path().join("dummy.txt");
        fs::write(&path, DUMMY).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        Self { dir }
    }
    fn client(&self, endpoint: &str, timeout: Duration, cap: u64) -> HttpModelTransport {
        HttpModelTransport::new(endpoint, &self.dir.path().join("dummy.txt"), timeout, cap).unwrap()
    }
}
struct Gateway {
    endpoint: String,
    thread: JoinHandle<Value>,
}
impl Gateway {
    fn start(
        status: u16,
        header_delay: Duration,
        body_delay: Duration,
        partial: usize,
        drip: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let thread = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(4);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < until, "fixture did not receive a request");
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("fixture accept: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let request = request(&mut stream);
            let body = br#"{"status":"incomplete"}"#;
            thread::sleep(header_delay);
            let header = format!(
                "HTTP/1.1 {status} fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            if drip {
                for chunk in body.chunks(4) {
                    thread::sleep(Duration::from_millis(90));
                    let _ = stream.write_all(chunk);
                }
            } else {
                if partial > 0 {
                    let _ = stream.write_all(&body[..partial]);
                }
                thread::sleep(body_delay);
                let _ = stream.write_all(&body[partial..]);
            }
            // One accepted connection only; a second dispatch is a failure.
            assert!(listener.accept().is_err(), "unexpected second dispatch");
            request
        });
        Self { endpoint, thread }
    }
    fn finish(self) {
        assert_eq!(self.thread.join().unwrap(), payload());
    }
}
fn request(stream: &mut TcpStream) -> Value {
    let mut raw = Vec::new();
    let end = loop {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        raw.extend_from_slice(&buf[..n]);
        if let Some(i) = raw.windows(4).position(|v| v == b"\r\n\r\n") {
            break i + 4;
        }
        assert!(raw.len() < 65_536);
    };
    let headers = std::str::from_utf8(&raw[..end]).unwrap();
    assert!(headers.starts_with("POST /v1/responses HTTP/1.1\r\n"));
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    while raw.len() < end + length {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        raw.extend_from_slice(&buf[..n]);
    }
    assert_eq!(
        &raw[end..end + length],
        oe_core::json::ordered_json(&payload()).unwrap()
    );
    serde_json::from_slice(&raw[end..end + length]).unwrap()
}
fn payload() -> Value {
    json!({"model":"synthetic-wait-model","store":false,"stream":false,"truncation":"disabled","input":[],"max_output_tokens":64})
}
fn view(client: &HttpModelTransport) -> Value {
    let typed = client.wait_observation();
    typed.validate().unwrap();
    for a in typed.attempts.as_ref().unwrap() {
        a.validate().unwrap();
    }
    let v = serde_json::to_value(typed).unwrap();
    assert_eq!(v["schema"], "oev2.responses-http-wait-observation/1");
    assert_eq!(v["timing_scope"], "client_api_boundaries");
    for field in [
        "dns_tls_timing",
        "server_processing_timing",
        "upstream_arrival",
        "usage_cost",
    ] {
        assert_eq!(v[field], "unknown");
    }
    assert!(!v.to_string().contains(DUMMY));
    let mut last = 0;
    for a in v["attempts"].as_array().unwrap() {
        for field in [
            "serialization_finished_us",
            "dispatch_api_started_us",
            "dispatch_api_finished_us",
            "headers_observed_us",
            "body_read_started_us",
            "body_read_finished_us",
            "terminal_us",
        ] {
            if let Some(current) = a[field].as_u64() {
                assert!(current >= last, "{field} out of order");
                last = current;
            }
        }
    }
    v
}
fn legacy(
    client: &HttpModelTransport,
    status: Option<u16>,
    read: &str,
    bytes: Option<u64>,
    parsed: &str,
    error: Option<&str>,
    dispatched: bool,
) {
    let sent = oe_core::json::ordered_json(&payload()).unwrap();
    let expected = json!({"schema":"oev2.responses-http-observation/1","status":"recorded","send_call_attempts":1,
        "client_dispatch_attempts":if dispatched {1}else{0},"observed_http_attempts":if status.is_some(){1}else{0},"upstream_attempts":"unknown",
        "attempts":[{"request_status":if dispatched{"serialized"}else{"credential_rejected"},
        "request_sha256":if dispatched {Some(oe_core::sha256(&sent))} else {None},"request_bytes":if dispatched{Some(sent.len()as u64)}else{None},
        "client_dispatch_attempted":dispatched,"http_status":status,"response_read_status":read,"response_bytes_read":bytes,
        "response_json_status":parsed,"safe_error":error}]});
    assert_eq!(
        client.observation(),
        expected,
        "legacy /1 projection must stay exact"
    );
    assert_eq!(
        serde_json::to_vec(&client.observation()).unwrap(),
        serde_json::to_vec(&expected).unwrap(),
        "legacy /1 serialization bytes"
    );
}
#[test]
fn completed_response_keeps_legacy_projection() {
    let f = Fixture::new();
    let g = Gateway::start(200, Duration::ZERO, Duration::ZERO, 0, false);
    let c = f.client(&g.endpoint, Duration::from_secs(2), 1024);
    assert_eq!(c.send(&payload()).unwrap(), json!({"status":"incomplete"}));
    let v = view(&c);
    let a = &v["attempts"][0];
    assert_eq!(a["terminal_stage"], "completed");
    assert_eq!(a["http_status"], 200);
    for field in [
        "serialization_finished_us",
        "dispatch_api_started_us",
        "dispatch_api_finished_us",
        "headers_observed_us",
        "body_read_started_us",
        "body_read_finished_us",
        "terminal_us",
    ] {
        assert!(a[field].is_u64(), "{field}");
    }
    legacy(&c, Some(200), "read", Some(23), "object", None, true);
    g.finish();
}
#[test]
fn delayed_headers_stops_at_dispatch() {
    let f = Fixture::new();
    let g = Gateway::start(200, DELAY, Duration::ZERO, 0, false);
    let c = f.client(&g.endpoint, TIMEOUT, 1024);
    let e = c.send(&payload()).unwrap_err();
    assert_eq!(e.code, "timeout");
    assert_eq!(e.http_status, None);
    assert!(e.dispatch_attempted);
    let v = view(&c);
    let a = &v["attempts"][0];
    assert_eq!(a["terminal_stage"], "dispatch");
    assert!(a["dispatch_api_finished_us"].is_u64());
    assert!(a["terminal_us"].is_u64());
    for field in [
        "http_status",
        "headers_observed_us",
        "body_read_started_us",
        "body_read_finished_us",
    ] {
        assert!(a[field].is_null());
    }
    legacy(
        &c,
        None,
        "not_started",
        None,
        "not_parsed",
        Some("timeout"),
        true,
    );
    g.finish();
}
#[test]
fn delayed_body_stops_at_body_read() {
    let f = Fixture::new();
    let g = Gateway::start(200, Duration::ZERO, DELAY, 0, false);
    let c = f.client(&g.endpoint, TIMEOUT, 1024);
    let e = c.send(&payload()).unwrap_err();
    assert_eq!(e.code, "response_read_failed");
    assert_eq!(e.http_status, Some(200));
    let v = view(&c);
    let a = &v["attempts"][0];
    assert_eq!(a["terminal_stage"], "body_read");
    assert_eq!(a["http_status"], 200);
    for field in [
        "headers_observed_us",
        "body_read_started_us",
        "body_read_finished_us",
        "terminal_us",
    ] {
        assert!(a[field].is_u64());
    }
    legacy(
        &c,
        Some(200),
        "failed",
        Some(0),
        "not_parsed",
        Some("response_read_failed"),
        true,
    );
    g.finish();
}
#[test]
fn partial_body_failure_retains_actual_bytes() {
    let f = Fixture::new();
    let g = Gateway::start(200, Duration::ZERO, DELAY, 5, false);
    let c = f.client(&g.endpoint, TIMEOUT, 1024);
    assert_eq!(c.send(&payload()).unwrap_err().code, "response_read_failed");
    let v = view(&c);
    assert_eq!(v["attempts"][0]["terminal_stage"], "body_read");
    legacy(
        &c,
        Some(200),
        "failed",
        Some(5),
        "not_parsed",
        Some("response_read_failed"),
        true,
    );
    g.finish();
}
#[test]
fn error_status_never_reads_error_body() {
    let f = Fixture::new();
    let g = Gateway::start(503, Duration::ZERO, DELAY, 0, false);
    let c = f.client(&g.endpoint, TIMEOUT, 1024);
    let e = c.send(&payload()).unwrap_err();
    assert_eq!(e.code, "http_error");
    assert_eq!(e.http_status, Some(503));
    let v = view(&c);
    let a = &v["attempts"][0];
    assert_eq!(a["terminal_stage"], "response_headers");
    assert!(a["body_read_started_us"].is_null());
    assert!(a["body_read_finished_us"].is_null());
    legacy(
        &c,
        Some(503),
        "not_read_error_status",
        None,
        "not_parsed",
        Some("http_error"),
        true,
    );
    g.finish();
}
#[test]
fn credential_rejection_has_no_dispatch() {
    let f = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let c = f.client(
        &format!("http://{}/v1/responses", listener.local_addr().unwrap()),
        TIMEOUT,
        1024,
    );
    let e = c.send(&json!({"synthetic_secret":DUMMY})).unwrap_err();
    assert_eq!(e.code, "credential_material_in_request");
    assert!(!e.dispatch_attempted);
    let v = view(&c);
    let a = &v["attempts"][0];
    assert_eq!(a["terminal_stage"], "pre_dispatch");
    assert!(a["dispatch_api_started_us"].is_null());
    assert!(a["terminal_us"].is_u64());
    legacy(
        &c,
        None,
        "not_started",
        None,
        "not_parsed",
        Some("credential_material_in_request"),
        false,
    );
    assert!(listener.accept().is_err());
}
#[test]
fn response_cap_remains_unchanged() {
    let f = Fixture::new();
    let g = Gateway::start(200, Duration::ZERO, Duration::ZERO, 0, false);
    let c = f.client(&g.endpoint, Duration::from_secs(2), 4);
    assert_eq!(c.send(&payload()).unwrap_err().code, "response_too_large");
    let v = view(&c);
    assert_eq!(v["attempts"][0]["terminal_stage"], "response_processing");
    legacy(
        &c,
        Some(200),
        "bounded_limit_exceeded",
        Some(5),
        "not_parsed",
        Some("response_too_large"),
        true,
    );
    g.finish();
}
#[test]
fn empty_view_has_no_attempt() {
    let f = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let c = f.client(
        &format!("http://{}/v1/responses", listener.local_addr().unwrap()),
        TIMEOUT,
        1024,
    );
    let v = view(&c);
    assert_eq!(v["status"], "recorded");
    assert_eq!(v["attempts"], json!([]));
    assert!(listener.accept().is_err());
}
#[test]
fn blocking_progress_does_not_claim_total_deadline() {
    let f = Fixture::new();
    let g = Gateway::start(200, Duration::ZERO, Duration::ZERO, 0, true);
    let c = f.client(&g.endpoint, TIMEOUT, 1024);
    assert_eq!(c.send(&payload()).unwrap(), json!({"status":"incomplete"}));
    let v = view(&c);
    assert!(v["attempts"][0]["terminal_us"].as_u64().unwrap() > TIMEOUT.as_micros() as u64);
    assert_eq!(v["attempts"][0]["terminal_stage"], "completed");
    legacy(&c, Some(200), "read", Some(23), "object", None, true);
    g.finish();
}
