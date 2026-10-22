#![cfg(unix)]
use oe_model_credentials::{CredentialPool, provider_transport::ProviderHttpTransport};
use semantica_contracts::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../var/provider-pool-tests");
        fs::create_dir_all(&root).unwrap();
        let p = root.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn key(&self, value: &str) -> PathBuf {
        let p = self.0.join("synthetic-key");
        fs::write(&p, value).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        p
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn wire(prompt: &str) -> SemanticProviderWireRequest {
    let body = json!({"model":"synthetic","messages":[{"role":"user","content":prompt}]});
    SemanticProviderWireRequest {
        provider: "openai".into(),
        custom_endpoint: true,
        operation: "generate_structured".into(),
        body_sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(&body).unwrap())),
        body,
    }
}
fn server(body: Vec<u8>, status: u16) -> (String, thread::JoinHandle<Value>) {
    server_with_headers(body, status, "")
}
fn server_with_headers(
    body: Vec<u8>,
    status: u16,
    extra: &str,
) -> (String, thread::JoinHandle<Value>) {
    let extra = extra.to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut header = Vec::new();
        let mut b = [0];
        while !header.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).unwrap();
            header.push(b[0]);
            assert!(header.len() < 16384);
        }
        let length = std::str::from_utf8(&header)
            .unwrap()
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
            })
            .unwrap();
        let mut request = vec![0; length];
        s.read_exact(&mut request).unwrap();
        let request = serde_json::from_slice(&request).unwrap();
        write!(s,"HTTP/1.1 {status} Synthetic\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
        s.write_all(&body).unwrap();
        request
    });
    (endpoint, t)
}
#[test]
fn explicit_single_bearer_is_private_and_does_not_rotate() {
    for token in [
        "sk-SYNTHETIC_ONLY",
        "gsk_SYNTHETIC_ONLY",
        "apikey_SYNTHETIC_ONLY",
    ] {
        let case = Case::new();
        let p = case.key(token);
        let pool = CredentialPool::read_single_http_bearer(&p).unwrap();
        assert_eq!(pool.status().unwrap().total, 1);
        assert!(pool.contains_in(token.as_bytes()));
        let lease = pool.lease().unwrap();
        assert!(lease.bearer().starts_with("Bearer "));
        assert!(!format!("{pool:?}{lease:?}").contains(token));
        assert!(!lease.retire().unwrap());
        assert!(pool.lease().is_err());
    }
}
#[test]
fn single_bearer_rejects_prose_whitespace_crlf_multiple_keys_and_bad_files() {
    for input in [
        "",
        "note sk-SYNTHETIC",
        "sk-SYNTHETIC\r\n",
        "sk-SYNTHETIC\nsk-SECOND",
        " sk-SYNTHETIC",
        "sk-SYNTHETIC ",
        "gsk_SYNTHETIC\t",
        "sk-中文",
        "bad=middle",
        "sk-SYNTHETIC\n\n",
    ] {
        let case = Case::new();
        let p = case.key(input);
        let err = CredentialPool::read_single_http_bearer(&p).unwrap_err();
        assert!(!err.to_string().contains(input) || input.is_empty());
    }
    let case = Case::new();
    let p = case.key("sk-SYNTHETIC");
    fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(CredentialPool::read_single_http_bearer(&p).is_err());
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    let linked = case.0.join("link");
    symlink(&p, &linked).unwrap();
    assert!(CredentialPool::read_single_http_bearer(&linked).is_err());
}
#[test]
fn synthetic_http_body_and_decoded_credential_are_guarded() {
    let case = Case::new();
    let key = &["sk-", "SYNTHETIC_ONLY_NOT_REAL"].concat();
    let p = case.key(key);
    let pool = CredentialPool::read_single_http_bearer(&p).unwrap();
    let body = serde_json::to_vec(&json!({"choices":[{"message":{"content":"ok"}}]})).unwrap();
    let (endpoint, t) = server(body, 200);
    let transport = ProviderHttpTransport::local_synthetic(
        &pool,
        "openai",
        &endpoint,
        true,
        Duration::from_secs(2),
    )
    .unwrap();
    let request = wire("hello");
    let response = transport.post(&request).unwrap();
    assert_eq!(response.transport_kind, "synthetic_loopback");
    assert_eq!(t.join().unwrap(), request.body);
    // Escaped outer JSON is decoded before the same pool guard.
    let body = format!(
        "{{\"choices\":[{{\"message\":{{\"content\":\"\\u0073k-SYNTHETIC_ONLY_NOT_REAL\"}}}}]}}"
    )
    .into_bytes();
    let (endpoint, t) = server(body, 200);
    let transport = ProviderHttpTransport::local_synthetic(
        &pool,
        "openai",
        &endpoint,
        true,
        Duration::from_secs(2),
    )
    .unwrap();
    let error = transport.post(&wire("hello")).unwrap_err();
    assert_eq!(error.code, "credential_material_in_response");
    assert!(!serde_json::to_string(&error).unwrap().contains(key));
    t.join().unwrap();
    // A parsed structured result is checked again, after a second JSON decode.
    let result:SemanticProviderResult=serde_json::from_value(json!({"schema":"semantica.provider-result/1","execution":"completed","provider":"openai","operation":"generate_structured","request_sha256":"0".repeat(64),"wire_request_sha256":"0".repeat(64),"response_sha256":"0".repeat(64),"requested_model":null,"resolved_model":null,"provider_response_id":null,"output":{"private":key},"usage":{"input_tokens":null,"output_tokens":null,"total_tokens":null},"usage_status":"unknown","transport_kind":"synthetic_loopback","transport_identity":"test"})).unwrap();
    assert_eq!(
        transport.check_result(&result).unwrap_err().code,
        "credential_material_in_response"
    );
}
#[test]
fn request_binding_scope_redirect_and_http_failure_do_not_retry() {
    let case = Case::new();
    let pool = CredentialPool::read_single_http_bearer(case.key("gsk_SYNTHETIC_LOCAL")).unwrap();
    for endpoint in [
        "file:///etc/passwd",
        "ftp://localhost/test",
        "javascript:bad",
        "http://localhost:123/test",
        "http://example.com:123/test",
    ] {
        assert!(
            ProviderHttpTransport::local_synthetic(
                &pool,
                "openai",
                endpoint,
                true,
                Duration::from_secs(2)
            )
            .is_err()
        );
    }
    let (endpoint, t) = server(b"private arbitrary body never returned".to_vec(), 401);
    let transport = ProviderHttpTransport::local_synthetic(
        &pool,
        "openai",
        &endpoint,
        true,
        Duration::from_secs(2),
    )
    .unwrap();
    let error = transport.post(&wire("hello")).unwrap_err();
    assert_eq!(error.http_status, Some(401));
    assert!(!error.retryable);
    assert!(pool.unavailable());
    assert!(!serde_json::to_string(&error).unwrap().contains("arbitrary"));
    t.join().unwrap();
    let case = Case::new();
    let pool = CredentialPool::read_single_http_bearer(case.key("sk-SYNTHETIC_LOCAL")).unwrap();
    let transport = ProviderHttpTransport::local_synthetic(
        &pool,
        "openai",
        "http://127.0.0.1:9/test",
        true,
        Duration::from_secs(2),
    )
    .unwrap();
    let mut request = wire("hello");
    request.body_sha256 = "0".repeat(64);
    assert_eq!(
        transport.post(&request).unwrap_err().code,
        "provider_transport_scope_invalid"
    );
    let mut request = wire("hello");
    request.provider = "groq".into();
    assert_eq!(
        transport.post(&request).unwrap_err().code,
        "provider_transport_scope_invalid"
    );
    assert_eq!(
        transport
            .post(&wire("sk-SYNTHETIC_LOCAL"))
            .unwrap_err()
            .code,
        "credential_material_in_request"
    );
}

#[test]
fn redirect_location_cannot_trigger_a_second_dispatch() {
    let case = Case::new();
    let pool = CredentialPool::read_single_http_bearer(case.key("sk-SYNTHETIC_REDIRECT")).unwrap();
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let location = format!(
        "Location: http://{}/canary\r\n",
        target.local_addr().unwrap()
    );
    let (endpoint, server) = server_with_headers(Vec::new(), 302, &location);
    let transport = ProviderHttpTransport::local_synthetic(
        &pool,
        "openai",
        &endpoint,
        true,
        Duration::from_secs(2),
    )
    .unwrap();
    let failure = transport.post(&wire("hello")).unwrap_err();
    assert_eq!(failure.http_status, Some(302));
    assert!(!failure.retryable);
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    server.join().unwrap();
    assert!(!pool.unavailable());
}
