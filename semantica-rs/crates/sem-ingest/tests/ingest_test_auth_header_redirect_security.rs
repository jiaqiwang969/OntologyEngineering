//! Port of `tests/ingest/test_auth_header_redirect_security.py` (issue #947): credentials must
//! not leak across cross-origin redirects in `request_with_ssrf_guard`, `MCPClient` and
//! `PublicAPIIngestor`.
#![allow(non_snake_case)]

mod common;

use common::*;
use indexmap::IndexMap;
use sem_ingest::mcp_client::MCPClient;
use sem_ingest::public_api_ingestor::{PublicAPIIngestor, PublicApiRequest};
use sem_ingest::ssrf::{GuardOptions, request_with_ssrf_guard};
use sem_ingest::support::http::{Auth, RequestOptions, Response, Session};
use serde_json::{Value, json};

fn public_dns() -> sem_ingest::ssrf::socket::ResolverGuard {
    dns_returning(&[PUBLIC_IP])
}

fn session_with_auth(fake: &FakeTransport, token: &str) -> Session {
    let mut s = Session::with_transport(fake.arc());
    s.headers.insert("Authorization", token);
    s
}

fn get(session: Option<&mut Session>, url: &str, options: RequestOptions) -> Result<Response, sem_utils::SemanticaError> {
    request_with_ssrf_guard("GET", url, session, &GuardOptions::default(), options)
}

fn headers(pairs: &[(&str, &str)]) -> RequestOptions {
    let map: IndexMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    RequestOptions {
        headers: Some(map),
        ..RequestOptions::default()
    }
}

fn basic(user: &str, pass: &str) -> RequestOptions {
    RequestOptions {
        auth: Some(Auth::Basic(user.to_string(), pass.to_string())),
        ..RequestOptions::default()
    }
}

// Section 1 – session.headers stripping.

#[test]
fn test_session_headers_stripping__test_session_authorization_stripped_on_cross_origin_redirect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = session_with_auth(&fake, "Bearer secret");
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[1].headers().contains_key("Authorization"));
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer secret"));
}

#[test]
fn test_session_headers_stripping__test_session_headers_cleared_before_second_hop_not_just_restored_after() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = session_with_auth(&fake, "Bearer proof-token");
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    let snapshots: Vec<IndexMap<String, String>> = fake.calls().into_iter().map(|c| c.session_headers).collect();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].get("Authorization").map(String::as_str), Some("Bearer proof-token"));
    assert!(!snapshots[1].contains_key("Authorization"));
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer proof-token"));
}

#[test]
fn test_session_headers_stripping__test_session_authorization_preserved_on_same_origin_redirect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://example.com/page2", 302), final_response(200)]);
    let mut sess = session_with_auth(&fake, "Bearer secret");
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    assert_eq!(fake.call_count(), 2);
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer secret"));
}

#[test]
fn test_session_headers_stripping__test_session_credentials_restored_after_successful_request() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = session_with_auth(&fake, "Bearer my-token");
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer my-token"));
}

#[test]
fn test_session_headers_stripping__test_session_credentials_restored_after_ssrf_exception() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(redirect("http://127.0.0.1/secret", 302));
    let mut sess = session_with_auth(&fake, "Bearer my-token");
    assert!(get(Some(&mut sess), "https://example.com/start", RequestOptions::default())
        .unwrap_err()
        .is_validation_error());
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer my-token"));
}

#[test]
fn test_session_headers_stripping__test_session_credentials_restored_after_max_redirects_exceeded() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(redirect("https://other.example/loop", 302));
    let mut sess = session_with_auth(&fake, "Bearer loop-token");
    let guard = GuardOptions {
        max_redirects: 2,
        ..GuardOptions::default()
    };
    let err = request_with_ssrf_guard("GET", "https://example.com/start", Some(&mut sess), &guard, RequestOptions::default())
        .unwrap_err();
    assert!(err.is_validation_error());
    assert!(err.to_string().contains("Exceeded maximum"));
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer loop-token"));
}

#[test]
fn test_session_headers_stripping__test_proxy_authorization_stripped_on_cross_origin_redirect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    sess.headers.insert("Proxy-Authorization", "Basic cHJveHk6cGFzcw==");
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    assert!(!fake.calls()[1].headers().contains_key("Proxy-Authorization"));
    assert!(sess.headers.contains_key("Proxy-Authorization"));
}

#[test]
fn test_session_headers_stripping__test_both_auth_headers_stripped_simultaneously() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    sess.headers.insert("Authorization", "Bearer tok");
    sess.headers.insert("Proxy-Authorization", "Basic abc");
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    let second = fake.calls()[1].headers();
    assert!(!second.contains_key("Authorization"));
    assert!(!second.contains_key("Proxy-Authorization"));
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer tok"));
    assert_eq!(sess.headers.get("Proxy-Authorization"), Some("Basic abc"));
}

// Section 1b – auth= kwarg and session.auth stripping.

#[test]
fn test_auth_handler_stripping__test_kwargs_auth_stripped_on_cross_origin_redirect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let _s = patch_session_request(&fake);
    get(None, "https://example.com/start", basic("user", "secret-password")).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].options.auth,
        Some(Auth::Basic("user".to_string(), "secret-password".to_string()))
    );
    assert_eq!(calls[1].options.auth, None);
}

#[test]
fn test_auth_handler_stripping__test_kwargs_auth_preserved_on_same_origin_redirect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://example.com/new-path", 302), final_response(200)]);
    let _s = patch_session_request(&fake);
    get(None, "https://example.com/start", basic("user", "secret-password")).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1].options.auth,
        Some(Auth::Basic("user".to_string(), "secret-password".to_string()))
    );
}

#[test]
fn test_auth_handler_stripping__test_session_auth_cleared_before_cross_origin_hop() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    let creds = Auth::Basic("user".to_string(), "secret-password".to_string());
    sess.auth = Some(creds.clone());
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    let snapshots: Vec<Option<Auth>> = fake.calls().into_iter().map(|c| c.session_auth).collect();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0], Some(creds.clone()));
    assert_eq!(snapshots[1], None);
    assert_eq!(sess.auth, Some(creds));
}

#[test]
fn test_auth_handler_stripping__test_session_auth_preserved_on_same_origin_redirect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://example.com/page2", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    let creds = Auth::Basic("user".to_string(), "secret-password".to_string());
    sess.auth = Some(creds.clone());
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    let snapshots: Vec<Option<Auth>> = fake.calls().into_iter().map(|c| c.session_auth).collect();
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0], Some(creds.clone()));
    assert_eq!(snapshots[1], Some(creds.clone()));
    assert_eq!(sess.auth, Some(creds));
}

#[test]
fn test_auth_handler_stripping__test_session_auth_restored_after_successful_request() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    let creds = Auth::Basic("user".to_string(), "secret-password".to_string());
    sess.auth = Some(creds.clone());
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    assert_eq!(sess.auth, Some(creds));
}

#[test]
fn test_auth_handler_stripping__test_session_auth_restored_after_ssrf_exception() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(redirect("http://127.0.0.1/secret", 302));
    let mut sess = Session::with_transport(fake.arc());
    let creds = Auth::Basic("user".to_string(), "secret-password".to_string());
    sess.auth = Some(creds.clone());
    assert!(get(Some(&mut sess), "https://example.com/start", RequestOptions::default())
        .unwrap_err()
        .is_validation_error());
    assert_eq!(sess.auth, Some(creds));
}

#[test]
fn test_auth_handler_stripping__test_session_auth_none_by_default_remains_none() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    assert_eq!(sess.auth, None);
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    assert_eq!(sess.auth, None);
}

#[test]
fn test_auth_handler_stripping__test_kwargs_auth_does_not_reappear_in_multihop_chain() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![
        redirect("https://other.example/step2", 302),
        redirect("https://other.example/final", 302),
        final_response(200),
    ]);
    let _s = patch_session_request(&fake);
    get(None, "https://example.com/start", basic("user", "pass")).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].options.auth, Some(Auth::Basic("user".to_string(), "pass".to_string())));
    assert_eq!(calls[1].options.auth, None);
    assert_eq!(calls[2].options.auth, None);
}

#[test]
fn test_auth_handler_stripping__test_session_trust_env_disabled_before_cross_origin_hop() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    sess.trust_env = true;
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    let snapshots: Vec<bool> = fake.calls().into_iter().map(|c| c.session_trust_env).collect();
    assert_eq!(snapshots, vec![true, false]);
    assert!(sess.trust_env);
}

#[test]
fn test_auth_handler_stripping__test_session_trust_env_restored_after_exception() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(redirect("http://127.0.0.1/secret", 302));
    let mut sess = Session::with_transport(fake.arc());
    sess.trust_env = true;
    assert!(get(Some(&mut sess), "https://example.com/start", RequestOptions::default())
        .unwrap_err()
        .is_validation_error());
    assert!(sess.trust_env);
}

#[test]
fn test_auth_handler_stripping__test_session_trust_env_false_stays_false_after_call() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let mut sess = Session::with_transport(fake.arc());
    sess.trust_env = false;
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    assert!(!sess.trust_env);
}

// Section 2 – credential resurrection prevention.

#[test]
fn test_credential_resurrection__test_credentials_do_not_reappear_after_cross_origin_hop() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![
        redirect("https://other.example/step2", 302),
        redirect("https://other.example/final", 302),
        final_response(200),
    ]);
    let _s = patch_session_request(&fake);
    get(None, "https://example.com/start", headers(&[("Authorization", "Bearer secret")])).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].headers().get("Authorization").map(String::as_str), Some("Bearer secret"));
    assert!(!calls[1].headers().contains_key("Authorization"));
    assert!(!calls[2].headers().contains_key("Authorization"));
}

#[test]
fn test_credential_resurrection__test_session_auth_does_not_reappear_in_multihop_chain() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![
        redirect("https://other.example/step2", 302),
        redirect("https://other.example/final", 302),
        final_response(200),
    ]);
    let mut sess = session_with_auth(&fake, "Bearer multi");
    get(Some(&mut sess), "https://example.com/start", RequestOptions::default()).unwrap();
    assert_eq!(sess.headers.get("Authorization"), Some("Bearer multi"));
    let calls = fake.calls();
    assert!(!calls[1].headers().contains_key("Authorization"));
    assert!(!calls[2].headers().contains_key("Authorization"));
}

// Section 3 – redirect types and origin changes.

fn strips_with(status: u16, location: &str, start: &str) -> IndexMap<String, String> {
    let fake = FakeTransport::sequence(vec![redirect(location, status), final_response(200)]);
    let _s = patch_session_request(&fake);
    get(None, start, headers(&[("Authorization", "Bearer tok")])).unwrap();
    fake.calls()[1].headers()
}

#[test]
fn test_redirect_types_and_origin_changes__test_strips_on_307_cross_origin() {
    let _dns = mock_dns();
    let _g = public_dns();
    assert!(!strips_with(307, "https://other.example/final", "https://example.com/start").contains_key("Authorization"));
}

#[test]
fn test_redirect_types_and_origin_changes__test_strips_on_308_cross_origin() {
    let _dns = mock_dns();
    let _g = public_dns();
    assert!(!strips_with(308, "https://other.example/final", "https://example.com/start").contains_key("Authorization"));
}

#[test]
fn test_redirect_types_and_origin_changes__test_strips_on_port_change() {
    let _dns = mock_dns();
    let _g = public_dns();
    assert!(!strips_with(302, "https://example.com:8443/final", "https://example.com/start").contains_key("Authorization"));
}

#[test]
fn test_redirect_types_and_origin_changes__test_strips_on_subdomain_change() {
    let _dns = mock_dns();
    let _g = public_dns();
    assert!(!strips_with(302, "https://api.example.com/final", "https://example.com/start").contains_key("Authorization"));
}

#[test]
fn test_redirect_types_and_origin_changes__test_keeps_on_https_443_explicit_to_implicit() {
    let _dns = mock_dns();
    let _g = public_dns();
    let second = strips_with(302, "https://example.com/final", "https://example.com:443/start");
    assert_eq!(second.get("Authorization").map(String::as_str), Some("Bearer tok"));
}

#[test]
fn test_redirect_types_and_origin_changes__test_case_insensitive_header_stripped() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/final", 302), final_response(200)]);
    let _s = patch_session_request(&fake);
    get(
        None,
        "https://example.com/start",
        headers(&[("authorization", "Bearer lower"), ("AUTHORIZATION", "Bearer upper")]),
    )
    .unwrap();
    for key in fake.calls()[1].headers().keys() {
        assert_ne!(key.to_lowercase(), "authorization", "variant {key:?} was not stripped");
    }
}

// TestAllowPrivateIpsOnRedirect

#[test]
fn test_allow_private_ips_on_redirect__test_cross_host_redirect_to_private_ip_is_blocked_when_pinned() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(redirect("http://169.254.169.254/latest/meta-data/", 302));
    let _s = patch_session_request(&fake);
    let guard = GuardOptions {
        allow_private_ips: true,
        allow_private_ips_on_redirect: Some(false),
        ..GuardOptions::default()
    };
    let err = request_with_ssrf_guard("GET", "https://trusted.example.com/start", None, &guard, RequestOptions::default())
        .unwrap_err();
    assert!(err.is_validation_error());
    assert!(err.to_string().contains("blocked"));
}

#[test]
fn test_allow_private_ips_on_redirect__test_same_host_redirect_keeps_private_ip_trust_when_pinned() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![redirect("http://localhost:8000/v2", 302), final_response(200)]);
    let _s = patch_session_request(&fake);
    let guard = GuardOptions {
        allow_private_ips: true,
        allow_private_ips_on_redirect: Some(false),
        ..GuardOptions::default()
    };
    request_with_ssrf_guard("GET", "http://localhost:8000/start", None, &guard, RequestOptions::default()).unwrap();
    assert_eq!(fake.call_count(), 2);
}

#[test]
fn test_allow_private_ips_on_redirect__test_default_behavior_unchanged_without_the_new_kwarg() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![
        redirect("http://169.254.169.254/latest/meta-data/", 302),
        final_response(200),
    ]);
    let _s = patch_session_request(&fake);
    request_with_ssrf_guard(
        "GET",
        "https://trusted.example.com/start",
        None,
        &GuardOptions::allow_private_ips(true),
        RequestOptions::default(),
    )
    .unwrap();
    assert_eq!(fake.call_count(), 2);
}

// Section 4 – MCPClient redirect auth-stripping.

fn mcp_response() -> Response {
    json_response(json!({"jsonrpc": "2.0", "id": 1, "result": {"serverInfo": {}, "capabilities": {}}}))
}

fn mcp_client(url: &str, auth: Option<&str>) -> MCPClient {
    let headers = auth.map(|a| {
        let mut h = IndexMap::new();
        h.insert("Authorization".to_string(), a.to_string());
        h
    });
    MCPClient::new(Some(url), headers, None, serde_json::Map::new()).unwrap()
}

fn ping() -> Value {
    json!({"jsonrpc": "2.0", "method": "ping"})
}

#[test]
fn test_mcp_client_auth_redirect__test_cross_origin_redirect_strips_authorization() {
    let _dns = mock_dns();
    let _g = public_dns();
    let mut client = mcp_client("https://mcp.example.com/mcp", Some("Bearer mcp-token"));
    let fake = FakeTransport::sequence(vec![redirect("https://other.example/mcp", 302), mcp_response()]);
    let _s = patch_session_request(&fake);
    client._send_request_http(&ping()).unwrap();
    assert_eq!(fake.call_count(), 2);
    assert!(!fake.calls()[1].headers().contains_key("Authorization"));
}

#[test]
fn test_mcp_client_auth_redirect__test_same_origin_redirect_preserves_authorization() {
    let _dns = mock_dns();
    let _g = public_dns();
    let mut client = mcp_client("https://mcp.example.com/mcp", Some("Bearer mcp-token"));
    let fake = FakeTransport::sequence(vec![redirect("https://mcp.example.com/mcp/v2", 301), mcp_response()]);
    let _s = patch_session_request(&fake);
    client._send_request_http(&ping()).unwrap();
    assert_eq!(fake.call_count(), 2);
    assert_eq!(
        fake.calls()[1].headers().get("Authorization").map(String::as_str),
        Some("Bearer mcp-token")
    );
}

#[test]
fn test_mcp_client_auth_redirect__test_localhost_mcp_server_is_not_blocked() {
    let _dns = mock_dns();
    let mut client = mcp_client("http://localhost:8000/mcp", None);
    let fake = FakeTransport::returning(mcp_response());
    let _s = patch_session_request(&fake);
    client._send_request_http(&ping()).unwrap();
    assert_eq!(fake.call_count(), 1);
}

#[test]
fn test_mcp_client_auth_redirect__test_loopback_ip_mcp_server_is_not_blocked() {
    let _dns = mock_dns();
    let mut client = mcp_client("http://127.0.0.1:9000/mcp", None);
    let fake = FakeTransport::returning(mcp_response());
    let _s = patch_session_request(&fake);
    client._send_request_http(&ping()).unwrap();
    assert_eq!(fake.call_count(), 1);
}

#[test]
fn test_mcp_client_auth_redirect__test_same_host_redirect_on_private_mcp_server_is_not_blocked() {
    let _dns = mock_dns();
    let mut client = mcp_client("http://localhost:8000/mcp", None);
    let fake = FakeTransport::sequence(vec![redirect("http://localhost:8000/mcp/v2", 302), mcp_response()]);
    let _s = patch_session_request(&fake);
    client._send_request_http(&ping()).unwrap();
    assert_eq!(fake.call_count(), 2);
}

#[test]
fn test_mcp_client_auth_redirect__test_redirect_to_private_ip_is_blocked() {
    let _dns = mock_dns();
    let _g = public_dns();
    let mut client = mcp_client("https://mcp.example.com/mcp", None);
    let fake = FakeTransport::returning(redirect("http://169.254.169.254/latest/meta-data/", 302));
    let _s = patch_session_request(&fake);
    let err = client._send_request_http(&ping()).unwrap_err();
    assert!(err.is_validation_error());
    assert!(err.to_string().contains("blocked"));
}

#[test]
fn test_mcp_client_auth_redirect__test_scheme_downgrade_strips_authorization() {
    let _dns = mock_dns();
    let _g = public_dns();
    let mut client = mcp_client("https://mcp.example.com/mcp", Some("Bearer downgrade-test"));
    let fake = FakeTransport::sequence(vec![redirect("http://mcp.example.com/mcp", 302), mcp_response()]);
    let _s = patch_session_request(&fake);
    client._send_request_http(&ping()).unwrap();
    assert!(!fake.calls()[1].headers().contains_key("Authorization"));
}

#[test]
fn test_mcp_client_auth_redirect__test_max_redirect_cap_respected() {
    let _dns = mock_dns();
    let _g = public_dns();
    let mut client = mcp_client("https://mcp.example.com/mcp", None);
    let fake = FakeTransport::returning(redirect("https://mcp.example.com/mcp/loop", 302));
    let _s = patch_session_request(&fake);
    let err = client._send_request_http(&ping()).unwrap_err();
    let re = regex::Regex::new("[Rr]edirect|[Ee]xceeded").unwrap();
    assert!(re.is_match(&err.to_string()));
}

#[test]
fn test_mcp_client_auth_redirect__test_allow_redirects_false_enforced() {
    let _dns = mock_dns();
    let _g = public_dns();
    let mut client = mcp_client("https://mcp.example.com/mcp", Some("Bearer tok"));
    let fake = FakeTransport::returning(mcp_response());
    let _s = patch_session_request(&fake);
    client._send_request_http(&ping()).unwrap();
    assert_eq!(fake.last_call().options.allow_redirects, Some(false));
}

// Section 5 – PublicAPIIngestor redirect auth-stripping.

fn public_response(payload: Value) -> Response {
    json_response(payload)
}

fn public_ingestor(validate_no_auth: Option<bool>) -> PublicAPIIngestor {
    PublicAPIIngestor::new(None, Some(0.0), validate_no_auth.map(Value::Bool), serde_json::Map::new()).unwrap()
}

#[test]
fn test_public_api_ingestor_redirect_security__test_redirect_to_private_ip_blocked_in_detect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(redirect("http://169.254.169.254/latest/meta-data/", 302));
    let _s = patch_session_class(&fake);
    let mut ingestor = public_ingestor(None);
    let err = ingestor
        .detect_public_api("https://example.com/api", &PublicApiRequest::default())
        .unwrap_err();
    assert!(err.is_validation_error());
    assert!(err.to_string().contains("blocked"));
}

#[test]
fn test_public_api_ingestor_redirect_security__test_redirect_to_private_ip_blocked_in_ingest() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(redirect("http://10.0.0.1/internal", 302));
    let _s = patch_session_class(&fake);
    let mut ingestor = public_ingestor(None);
    let err = ingestor
        .ingest_public_api("https://example.com/api", &PublicApiRequest::default())
        .unwrap_err();
    assert!(err.is_validation_error());
    assert!(err.to_string().contains("blocked"));
}

#[test]
fn test_public_api_ingestor_redirect_security__test_session_auth_not_leaked_on_cross_origin_redirect_ingest() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![
        redirect("https://other.example/api", 302),
        public_response(json!([{"id": 1}])),
    ]);
    let _s = patch_session_class(&fake);
    let mut ingestor = public_ingestor(Some(false));
    // Inject an auth-bearing session.
    ingestor.session.headers.insert("Authorization", "Bearer leaked");
    ingestor
        .ingest_public_api("https://example.com/api", &PublicApiRequest::default())
        .unwrap();
    assert_eq!(fake.call_count(), 2);
    assert!(!fake.calls()[1].headers().contains_key("Authorization"));
}

#[test]
fn test_public_api_ingestor_redirect_security__test_allow_redirects_false_enforced_in_detect() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(public_response(json!([{"id": 1}])));
    let _s = patch_session_class(&fake);
    let mut ingestor = public_ingestor(None);
    ingestor
        .detect_public_api("https://example.com/api", &PublicApiRequest::default())
        .unwrap();
    assert_eq!(fake.last_call().options.allow_redirects, Some(false));
}

#[test]
fn test_public_api_ingestor_redirect_security__test_allow_redirects_false_enforced_in_ingest() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::returning(public_response(json!([{"id": 1}])));
    let _s = patch_session_class(&fake);
    let mut ingestor = public_ingestor(None);
    ingestor
        .ingest_public_api("https://example.com/api", &PublicApiRequest::default())
        .unwrap();
    assert_eq!(fake.last_call().options.allow_redirects, Some(false));
}

#[test]
fn test_public_api_ingestor_redirect_security__test_validate_no_auth_false_does_not_bypass_redirect_stripping() {
    let _dns = mock_dns();
    let _g = public_dns();
    let fake = FakeTransport::sequence(vec![
        redirect("https://other.example/api", 302),
        public_response(json!([{"id": 1}])),
    ]);
    let _s = patch_session_class(&fake);
    let mut ingestor = public_ingestor(Some(false));
    let mut h = IndexMap::new();
    h.insert("Authorization".to_string(), "Bearer should-be-stripped".to_string());
    let request = PublicApiRequest {
        headers: Some(h),
        ..PublicApiRequest::default()
    };
    ingestor.ingest_public_api("https://example.com/api", &request).unwrap();
    assert!(!fake.calls()[1].headers().contains_key("Authorization"));
}
