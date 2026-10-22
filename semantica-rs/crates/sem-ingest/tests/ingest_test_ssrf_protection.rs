//! Port of `tests/ingest/test_ssrf_protection.py` (SSRF protection for web and API ingestors,
//! issue #867). Every test installs the directory's `conftest.mock_dns` first.
#![allow(non_snake_case)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use common::*;
use sem_ingest::api_ingestor::{EndpointRequest, RESTIngestor};
use sem_ingest::ssrf::{
    self, GuardOptions, parse_bool, request_with_ssrf_guard, validate_url_for_request,
};
use sem_ingest::support::http::{RequestOptions, Response, Session};
use sem_ingest::web_ingestor::{SitemapCrawler, WebIngestor, WebIngestorOptions};
use serde_json::{Value, json};

fn kwargs(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

fn assert_validation(err: sem_utils::SemanticaError, needle: &str) {
    assert!(err.is_validation_error(), "expected ValidationError, got {err:?}");
    assert!(err.to_string().contains(needle), "{err} does not match {needle}");
}

// --- TestParseBool ---

#[test]
fn test_parse_bool__test_bool_passthrough() {
    let _dns = mock_dns();
    assert!(parse_bool(&json!(true), false).unwrap());
    assert!(!parse_bool(&json!(false), false).unwrap());
}

#[test]
fn test_parse_bool__test_none_uses_default() {
    let _dns = mock_dns();
    assert!(!parse_bool(&Value::Null, false).unwrap());
    assert!(parse_bool(&Value::Null, true).unwrap());
}

#[test]
fn test_parse_bool__test_string_true_values() {
    let _dns = mock_dns();
    for value in ["true", "TRUE", "1", "yes", "on", " Yes "] {
        assert!(parse_bool(&json!(value), false).unwrap());
    }
}

#[test]
fn test_parse_bool__test_string_false_values() {
    let _dns = mock_dns();
    for value in ["false", "FALSE", "0", "no", "off", " No "] {
        assert!(!parse_bool(&json!(value), false).unwrap());
    }
}

#[test]
fn test_parse_bool__test_int_zero_one() {
    let _dns = mock_dns();
    assert!(!parse_bool(&json!(0), false).unwrap());
    assert!(parse_bool(&json!(1), false).unwrap());
}

#[test]
fn test_parse_bool__test_rejects_unknown_string() {
    let _dns = mock_dns();
    assert_validation(parse_bool(&json!("maybe"), false).unwrap_err(), "Invalid boolean");
}

// --- TestValidateUrlForRequest ---

#[test]
fn test_validate_url_for_request__test_accepts_https() {
    let _dns = mock_dns();
    let _g = dns_returning(&["93.184.216.34"]);
    validate_url_for_request("https://example.com/path", false).unwrap();
}

#[test]
fn test_validate_url_for_request__test_rejects_file_scheme() {
    let _dns = mock_dns();
    assert_validation(
        validate_url_for_request("file://localhost/etc/passwd", false).unwrap_err(),
        "not permitted",
    );
}

#[test]
fn test_validate_url_for_request__test_rejects_gopher_scheme() {
    let _dns = mock_dns();
    assert_validation(
        validate_url_for_request("gopher://example.com/1", false).unwrap_err(),
        "not permitted",
    );
}

#[test]
fn test_validate_url_for_request__test_rejects_literal_private_ips() {
    let _dns = mock_dns();
    for url in [
        "http://10.0.0.1/",
        "http://192.168.1.1/",
        "http://172.16.5.5/internal",
        "http://127.0.0.1:9999/internal",
        "http://169.254.169.254/latest/meta-data/",
    ] {
        assert_validation(validate_url_for_request(url, false).unwrap_err(), "blocked");
    }
}

#[test]
fn test_validate_url_for_request__test_rejects_localhost_hostname() {
    let _dns = mock_dns();
    assert_validation(
        validate_url_for_request("http://localhost/admin", false).unwrap_err(),
        "not allowed",
    );
}

#[test]
fn test_validate_url_for_request__test_rejects_hostname_resolving_to_private_ip() {
    let _dns = mock_dns();
    let _g = dns_returning(&["10.0.0.5"]);
    assert_validation(
        validate_url_for_request("http://internal.corp/secret", false).unwrap_err(),
        "blocked",
    );
}

#[test]
fn test_validate_url_for_request__test_allow_private_ips_opt_in() {
    let _dns = mock_dns();
    validate_url_for_request("http://127.0.0.1:8080/health", true).unwrap();
    assert_validation(
        validate_url_for_request("file://localhost/x", true).unwrap_err(),
        "not permitted",
    );
}

#[test]
fn test_validate_url_for_request__test_dns_failure_raises() {
    let _dns = mock_dns();
    let _g = dns_failing("name or service not known");
    assert_validation(
        validate_url_for_request("http://does-not-resolve.invalid/path", false).unwrap_err(),
        "could not be resolved safely",
    );
}

#[test]
fn test_validate_url_for_request__test_dns_timeout_raises() {
    // `Future.result` raising TimeoutError: a lookup that outlives a short bound.
    let _dns = mock_dns();
    let _t = ssrf::set_thread_dns_resolve_timeout(0.05);
    let _g = ssrf::socket::set_thread_getaddrinfo(|_| {
        std::thread::sleep(std::time::Duration::from_millis(300));
        Ok(vec!["93.184.216.34".to_string()])
    });
    assert_validation(
        validate_url_for_request("http://slow-dns.example/path", false).unwrap_err(),
        "could not be resolved safely",
    );
}

#[test]
fn test_validate_url_for_request__test_hung_getaddrinfo_returns_within_timeout_bound() {
    let _dns = mock_dns();
    let hang_seconds = 1.5;
    let bound = 0.1;
    let _t = ssrf::set_thread_dns_resolve_timeout(bound);
    let _g = ssrf::socket::set_thread_getaddrinfo(move |_| {
        std::thread::sleep(std::time::Duration::from_secs_f64(hang_seconds));
        Ok(vec!["93.184.216.34".to_string()])
    });
    let start = Instant::now();
    let err = ssrf::_hostname_resolves_to_blocked("hanging.example").unwrap_err();
    let elapsed = start.elapsed().as_secs_f64();
    assert_validation(err, "could not be resolved safely");
    assert!(elapsed < hang_seconds / 2.0);
    assert!(elapsed < bound + 0.75);
}

// --- TestRequestWithSsrfGuardRedirects ---

fn guarded(session: &mut Session, url: &str, options: RequestOptions) -> Result<Response, sem_utils::SemanticaError> {
    let _g = dns_returning(&["93.184.216.34"]);
    request_with_ssrf_guard("GET", url, Some(session), &GuardOptions::default(), options)
}

fn auth_header(value: &str) -> RequestOptions {
    let mut headers = indexmap::IndexMap::new();
    headers.insert("Authorization".to_string(), value.to_string());
    RequestOptions {
        headers: Some(headers),
        ..RequestOptions::default()
    }
}

#[test]
fn test_request_with_ssrf_guard_redirects__test_blocks_redirect_to_loopback() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(redirect("http://127.0.0.1/secret", 302));
    let mut session = Session::with_transport(fake.arc());
    let err = guarded(&mut session, "https://example.com/start", RequestOptions::default()).unwrap_err();
    assert_validation(err, "blocked");
    assert_eq!(fake.call_count(), 1);
    assert_eq!(fake.last_call().options.allow_redirects, Some(false));
}

#[test]
fn test_request_with_ssrf_guard_redirects__test_blocks_redirect_to_metadata_ip() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(redirect("http://169.254.169.254/latest/meta-data/", 301));
    let mut session = Session::with_transport(fake.arc());
    let err = guarded(&mut session, "https://example.com/start", RequestOptions::default()).unwrap_err();
    assert_validation(err, "blocked");
}

#[test]
fn test_request_with_ssrf_guard_redirects__test_strips_authorization_on_cross_host_redirect() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![
        redirect("https://other-host.example/final", 302),
        final_response(200),
    ]);
    let mut session = Session::with_transport(fake.arc());
    guarded(&mut session, "https://example.com/start", auth_header("Bearer secret-token")).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[1].headers().contains_key("Authorization"));
    assert_eq!(
        calls[0].headers().get("Authorization").map(String::as_str),
        Some("Bearer secret-token")
    );
}

#[test]
fn test_request_with_ssrf_guard_redirects__test_keeps_authorization_on_same_host_redirect() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://example.com/final", 302), final_response(200)]);
    let mut session = Session::with_transport(fake.arc());
    guarded(&mut session, "https://example.com/start", auth_header("Bearer secret-token")).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1].headers().get("Authorization").map(String::as_str),
        Some("Bearer secret-token")
    );
}

#[test]
fn test_request_with_ssrf_guard_redirects__test_strips_authorization_on_scheme_downgrade() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![redirect("http://example.com/final", 302), final_response(200)]);
    let mut session = Session::with_transport(fake.arc());
    guarded(&mut session, "https://example.com/start", auth_header("Bearer secret-token")).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[1].headers().contains_key("Authorization"));
    assert_eq!(
        calls[0].headers().get("Authorization").map(String::as_str),
        Some("Bearer secret-token")
    );
}

#[test]
fn test_request_with_ssrf_guard_redirects__test_keeps_authorization_on_scheme_upgrade() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![redirect("https://example.com/final", 302), final_response(200)]);
    let mut session = Session::with_transport(fake.arc());
    guarded(&mut session, "http://example.com/start", auth_header("Bearer secret-token")).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1].headers().get("Authorization").map(String::as_str),
        Some("Bearer secret-token")
    );
}

#[test]
fn test_request_with_ssrf_guard_redirects__test_follows_safe_redirect() {
    let _dns = mock_dns();
    let final_resp = final_response(200).with_text("final");
    let fake = FakeTransport::sequence(vec![redirect("https://example.com/final", 302), final_resp]);
    let mut session = Session::with_transport(fake.arc());
    let response = guarded(&mut session, "https://example.com/start", RequestOptions::default()).unwrap();
    assert_eq!(response.text(), "final");
    assert_eq!(fake.call_count(), 2);
    assert!(fake.calls().iter().all(|c| c.options.allow_redirects == Some(false)));
}

// --- TestWebIngestorSSRF ---

fn web(allow: Value) -> WebIngestor {
    WebIngestor::new(WebIngestorOptions {
        respect_robots: false,
        delay: 0.0,
        allow_private_ips: allow,
        ..WebIngestorOptions::default()
    })
    .unwrap()
}

#[test]
fn test_web_ingestor_ssrf__test_private_ip_never_reaches_urllib3() {
    let _dns = mock_dns();
    let mut ingestor = web(json!(false));
    // The wire layer: any attempt to send is recorded (urllib3 `urlopen` in the original).
    let attempts = FakeTransport::new();
    ingestor.session.set_transport(attempts.arc());
    for url in [
        "http://10.0.0.1/",
        "http://192.168.1.1/",
        "http://127.0.0.1:9999/internal",
        "http://169.254.169.254/latest/meta-data/",
    ] {
        let err = ingestor.ingest_url(url, None).unwrap_err();
        assert!(err.is_validation_error());
        assert_eq!(attempts.call_count(), 0, "SSRF target reached the wire: {url}");
    }
}

#[test]
fn test_web_ingestor_ssrf__test_file_scheme_rejected_by_semantica() {
    let _dns = mock_dns();
    let mut ingestor = web(json!(false));
    assert_validation(
        ingestor.ingest_url("file://localhost/etc/passwd", None).unwrap_err(),
        "not permitted",
    );
}

#[test]
fn test_web_ingestor_ssrf__test_allow_private_ips_permits_loopback_fetch() {
    let _dns = mock_dns();
    let mut ingestor = web(json!(true));
    let fake = FakeTransport::returning(Response::new(200).with_text("ok"));
    ingestor.session.set_transport(fake.arc());
    let content = ingestor.ingest_url("http://127.0.0.1:9/probe", None).unwrap();
    assert_eq!(fake.call_count(), 1);
    assert_eq!(fake.last_call().options.allow_redirects, Some(false));
    // extract_content ran once on the fetched body.
    assert_eq!(content.text, "ok");
}

#[test]
fn test_web_ingestor_ssrf__test_allow_private_ips_string_false_keeps_ssrf_on() {
    let _dns = mock_dns();
    let mut ingestor = web(json!("false"));
    assert!(!ingestor.allow_private_ips);
    assert_validation(ingestor.ingest_url("http://127.0.0.1:9/probe", None).unwrap_err(), "blocked");
}

#[test]
fn test_web_ingestor_ssrf__test_allow_private_ips_string_true_opts_in() {
    let _dns = mock_dns();
    let mut ingestor = web(json!("true"));
    assert!(ingestor.allow_private_ips);
    let fake = FakeTransport::returning(Response::new(200).with_text("ok"));
    ingestor.session.set_transport(fake.arc());
    ingestor.ingest_url("http://127.0.0.1:9/probe", None).unwrap();
    assert_eq!(fake.call_count(), 1);
}

#[test]
fn test_web_ingestor_ssrf__test_redirect_to_private_ip_blocked() {
    let _dns = mock_dns();
    let mut ingestor = web(json!(false));
    ingestor
        .session
        .set_transport(FakeTransport::returning(redirect("http://127.0.0.1/admin", 302)).arc());
    let _g = dns_returning(&["93.184.216.34"]);
    assert_validation(ingestor.ingest_url("https://example.com/public", None).unwrap_err(), "blocked");
}

// --- TestSitemapCrawlerSSRF ---

#[test]
fn test_sitemap_crawler_ssrf__test_private_sitemap_url_rejected() {
    let _dns = mock_dns();
    let crawler = SitemapCrawler::new(serde_json::Map::new()).unwrap();
    assert!(crawler.parse_sitemap("http://10.0.0.1/sitemap.xml").unwrap_err().is_validation_error());
}

#[test]
fn test_sitemap_crawler_ssrf__test_allow_private_ips_string_false_keeps_ssrf_on() {
    let _dns = mock_dns();
    let crawler = SitemapCrawler::new(kwargs(json!({"allow_private_ips": "false"}))).unwrap();
    assert!(!crawler.allow_private_ips);
    assert!(crawler.parse_sitemap("http://10.0.0.1/sitemap.xml").unwrap_err().is_validation_error());
}

#[test]
fn test_sitemap_crawler_ssrf__test_allow_private_ips_string_true_opts_in() {
    let _dns = mock_dns();
    let crawler = SitemapCrawler::new(kwargs(json!({"allow_private_ips": "true"}))).unwrap();
    assert!(crawler.allow_private_ips);
}

#[test]
fn test_sitemap_crawler_ssrf__test_redirect_to_private_ip_blocked() {
    let _dns = mock_dns();
    let crawler = SitemapCrawler::new(serde_json::Map::new()).unwrap();
    let fake = FakeTransport::returning(redirect("http://169.254.169.254/latest/meta-data/", 302));
    let _s = patch_session_request(&fake);
    let _g = dns_returning(&["93.184.216.34"]);
    assert_validation(crawler.parse_sitemap("https://example.com/sitemap.xml").unwrap_err(), "blocked");
}

// --- TestRESTIngestorSSRF ---

#[test]
fn test_rest_ingestor_ssrf__test_private_endpoint_never_reaches_session() {
    let _dns = mock_dns();
    let fake = FakeTransport::new();
    let _s = patch_session_class(&fake);
    let mut ingestor = RESTIngestor::new(serde_json::Map::new()).unwrap();
    let err = ingestor
        .ingest_endpoint("http://169.254.169.254/latest/meta-data/", &EndpointRequest::default())
        .unwrap_err();
    assert!(err.is_validation_error());
    assert_eq!(fake.call_count(), 0);
}

#[test]
fn test_rest_ingestor_ssrf__test_file_scheme_rejected() {
    let _dns = mock_dns();
    let mut ingestor = RESTIngestor::new(serde_json::Map::new()).unwrap();
    assert_validation(
        ingestor
            .ingest_endpoint("file://localhost/secret", &EndpointRequest::default())
            .unwrap_err(),
        "not permitted",
    );
}

#[test]
fn test_rest_ingestor_ssrf__test_allow_private_ips_opt_in() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(json_response(json!({"ok": true})));
    let _s = patch_session_class(&fake);
    let mut ingestor = RESTIngestor::new(kwargs(json!({"allow_private_ips": true}))).unwrap();
    let data = ingestor
        .ingest_endpoint("http://127.0.0.1:8080/health", &EndpointRequest::default())
        .unwrap();
    assert_eq!(data.data, json!({"ok": true}));
    assert_eq!(fake.call_count(), 1);
    assert_eq!(fake.last_call().options.allow_redirects, Some(false));
}

#[test]
fn test_rest_ingestor_ssrf__test_allow_private_ips_string_false_keeps_ssrf_on() {
    let _dns = mock_dns();
    let fake = FakeTransport::new();
    let _s = patch_session_class(&fake);
    let mut ingestor = RESTIngestor::new(kwargs(json!({"allow_private_ips": "false"}))).unwrap();
    assert!(!ingestor.allow_private_ips);
    assert_validation(
        ingestor
            .ingest_endpoint("http://127.0.0.1:8080/health", &EndpointRequest::default())
            .unwrap_err(),
        "blocked",
    );
    assert_eq!(fake.call_count(), 0);
}

#[test]
fn test_rest_ingestor_ssrf__test_allow_private_ips_string_true_opts_in() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(json_response(json!({"ok": true})));
    let _s = patch_session_class(&fake);
    let mut ingestor = RESTIngestor::new(kwargs(json!({"allow_private_ips": "true"}))).unwrap();
    assert!(ingestor.allow_private_ips);
    let data = ingestor
        .ingest_endpoint("http://127.0.0.1:8080/health", &EndpointRequest::default())
        .unwrap();
    assert_eq!(data.data, json!({"ok": true}));
    assert_eq!(fake.call_count(), 1);
}

#[test]
fn test_rest_ingestor_ssrf__test_redirect_to_private_ip_blocked() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(redirect("http://127.0.0.1/secret", 302));
    let _s = patch_session_class(&fake);
    let mut ingestor = RESTIngestor::new(serde_json::Map::new()).unwrap();
    let _g = dns_returning(&["93.184.216.34"]);
    assert_validation(
        ingestor
            .ingest_endpoint("https://example.com/api", &EndpointRequest::default())
            .unwrap_err(),
        "blocked",
    );
    assert_eq!(fake.call_count(), 1);
}

// Characterisation: the pinned adapter carries the validated addresses to the wire.
#[test]
fn guard_pins_the_validated_addresses() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(final_response(200));
    let mut session = Session::with_transport(fake.arc());
    guarded(&mut session, "https://example.com/start", RequestOptions::default()).unwrap();
    assert_eq!(fake.last_call().pinned_ips, Some(vec!["93.184.216.34".to_string()]));
    assert_eq!(
        fake.last_call().session_headers.get("Host").map(String::as_str),
        Some("example.com")
    );
    // Restored afterwards.
    assert!(session.get_adapter("https://example.com").unwrap().pinned.is_none());
    assert!(!session.headers.contains_key("Host"));
    let _ = Arc::new(Mutex::new(()));
}
