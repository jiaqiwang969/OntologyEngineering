//! Port of `tests/ingest/test_public_api_ingestor.py`. Every test installs the directory's
//! `conftest.mock_dns` first; `patch("requests.Session")` with `mock_session.headers = {}` is
//! [`patch_session_class`].
#![allow(non_snake_case)]

mod common;

use std::collections::HashSet;

use common::*;
use sem_ingest::api_ingestor::APIData;
use sem_ingest::methods::{IngestOutput, Source, ingest, ingest_public_api, list_available_methods};
use sem_ingest::public_api_ingestor::{PublicAPIDetection, PublicAPIExamples, PublicAPIIngestor, PublicApiRequest};
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use serde_json::{Value, json};

/// `_mock_response(status_code=200, json_payload=None, text="", headers=None)`.
fn mock_response(status_code: u16, json_payload: Option<Value>, text: &str, headers: &[(&str, &str)]) -> Response {
    let mut response = Response::new(status_code).with_text(text);
    for (k, v) in headers {
        response = response.with_header(k, v);
    }
    response = match json_payload {
        Some(payload) => response.with_json(payload),
        None => response.with_json_error("not json"),
    };
    if status_code >= 400 {
        response = response.with_raise_for_status(format!("{status_code} error"));
    }
    response
}

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

fn ingestor() -> PublicAPIIngestor {
    PublicAPIIngestor::new(None, Some(0.0), None, Kwargs::new()).unwrap()
}

fn request(v: Value) -> PublicApiRequest {
    PublicApiRequest::from_kwargs(&kwargs(v))
}

#[test]
fn test_public_api_examples_catalog_lists_endpoints_and_samples() {
    let _dns = mock_dns();
    let names = PublicAPIExamples::names(None);
    let endpoints = PublicAPIExamples::endpoints();
    let testing_examples = PublicAPIExamples::list_examples(Some("testing"));
    let sample = PublicAPIExamples::sample_response("jsonplaceholder_posts").unwrap();

    assert!(names.contains(&"jsonplaceholder_posts".to_string()));
    assert!(endpoints["jsonplaceholder_posts"].starts_with("https://"));
    let found: HashSet<String> = testing_examples.iter().map(|e| e.name.clone()).collect();
    for name in ["jsonplaceholder_posts", "jsonplaceholder_users", "jsonplaceholder_todos"] {
        assert!(found.contains(name));
    }
    assert_eq!(sample[0]["title"], "sample post");
}

#[test]
fn test_public_api_ingestor_ingests_json_records() {
    let _dns = mock_dns();
    let payload = json!([{"id": 1, "title": "hello"}]);
    let fake = FakeTransport::returning(mock_response(
        200,
        Some(payload.clone()),
        r#"[{"id": 1, "title": "hello"}]"#,
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    let result: APIData = ingestor()
        .ingest_public_api("https://jsonplaceholder.typicode.com/posts", &PublicApiRequest::default())
        .unwrap();
    assert_eq!(result.data, payload);
    assert_eq!(result.metadata["public_api"], json!(true));
    assert_eq!(result.metadata["authentication"], "none");
    assert_eq!(result.metadata["response_format"], "json");
    assert_eq!(result.metadata["record_count"], 1);
}

#[test]
fn test_public_api_ingestor_extracts_nested_record_path() {
    let _dns = mock_dns();
    let payload = json!({
        "success": true,
        "result": {"count": 1, "results": [{"id": "dataset-1", "title": "Dataset"}]},
    });
    let fake = FakeTransport::returning(mock_response(
        200,
        Some(payload),
        r#"{"success": true}"#,
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    let result = ingestor()
        .ingest_public_api(
            "https://catalog.data.gov/api/3/action/package_search",
            &request(json!({"record_path": "result.results"})),
        )
        .unwrap();
    assert_eq!(result.data, json!([{"id": "dataset-1", "title": "Dataset"}]));
    assert_eq!(result.metadata["record_path"], "result.results");
}

#[test]
fn test_public_api_ingestor_parses_csv_records() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(
        200,
        None,
        "id,name\n1,Ada\n2,Grace\n",
        &[("Content-Type", "text/csv")],
    ));
    let _session = patch_session_class(&fake);
    let result = ingestor()
        .ingest_public_api("https://example.com/data.csv", &PublicApiRequest::default())
        .unwrap();
    assert_eq!(result.data, json!([{"id": "1", "name": "Ada"}, {"id": "2", "name": "Grace"}]));
    assert_eq!(result.metadata["response_format"], "csv");
}

#[test]
fn test_public_api_ingestor_parses_xml_records_with_record_path() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(
        200,
        None,
        "<items><item id='1'>Ada</item><item id='2'>Grace</item></items>",
        &[("Content-Type", "application/xml")],
    ));
    let _session = patch_session_class(&fake);
    let result = ingestor()
        .ingest_public_api(
            "https://example.com/data.xml",
            &request(json!({"record_path": "children"})),
        )
        .unwrap();
    assert_eq!(result.metadata["response_format"], "xml");
    assert_eq!(result.data[0]["tag"], "item");
    assert_eq!(result.data[0]["attributes"], json!({"id": "1"}));
    assert_eq!(result.data[0]["text"], "Ada");
}

#[test]
fn test_public_api_ingestor_rejects_malicious_xml_entities() {
    let _dns = mock_dns();
    let xml_text = r#"<?xml version="1.0"?>
<!DOCTYPE items [
<!ENTITY xxe SYSTEM "file:///etc/passwd">
]>
<items><item>&xxe;</item></items>
"#;
    let fake = FakeTransport::returning(mock_response(
        200,
        None,
        xml_text,
        &[("Content-Type", "application/xml")],
    ));
    let _session = patch_session_class(&fake);
    let err = ingestor()
        .ingest_public_api("https://example.com/data.xml", &PublicApiRequest::default())
        .unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
    assert!(err.to_string().contains("Failed to parse XML public API response"));
}

#[test]
fn test_public_api_detection_reports_public_endpoint() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(
        200,
        None,
        "",
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    let detection: PublicAPIDetection = ingestor()
        .detect_public_api("https://jsonplaceholder.typicode.com/posts", &PublicApiRequest::default())
        .unwrap();
    assert!(detection.is_public);
    assert!(!detection.requires_auth);
    assert_eq!(detection.response_status, Some(200));
}

#[test]
fn test_public_api_detection_reports_auth_required() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(401, None, "", &[("WWW-Authenticate", "Bearer")]));
    let _session = patch_session_class(&fake);
    let _resolver = dns_returning(&["93.184.216.34"]);
    let detection = ingestor()
        .detect_public_api("https://api.example.com/private", &PublicApiRequest::default())
        .unwrap();
    assert!(!detection.is_public);
    assert!(detection.requires_auth);
    assert_eq!(detection.response_status, Some(401));
}

#[test]
fn test_detect_public_api_propagates_ssrf_validation_error() {
    let _dns = mock_dns();
    let fake = FakeTransport::new();
    let _session = patch_session_class(&fake);
    let _resolver = dns_returning(&["127.0.0.1"]);
    let err = ingestor()
        .detect_public_api("https://blocked.example.com/data", &PublicApiRequest::default())
        .unwrap_err();
    assert!(err.is_validation_error(), "{err:?}");
}

#[test]
fn test_detect_public_api_rejects_duplicate_session_and_allow_private_ips_kwargs() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(
        200,
        None,
        "",
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    // `session=object()` is represented by an opaque placeholder value.
    let detection = ingestor()
        .detect_public_api(
            "https://jsonplaceholder.typicode.com/posts",
            &request(json!({"allow_private_ips": true, "session": "<object>"})),
        )
        .unwrap();
    assert!(detection.is_public);
}

#[test]
fn test_ingest_public_api_rejects_duplicate_session_and_allow_private_ips_kwargs() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(
        200,
        Some(json!([{"id": 1}])),
        "",
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    let result = ingestor()
        .ingest_public_api(
            "https://jsonplaceholder.typicode.com/posts",
            &request(json!({"allow_private_ips": true, "session": "<object>"})),
        )
        .unwrap();
    assert_eq!(result.response_status, 200);
}

#[test]
fn test_public_api_ingestor_rejects_authentication_inputs() {
    let _dns = mock_dns();
    let fake = FakeTransport::new();
    let _session = patch_session_class(&fake);
    let mut ingestor = ingestor();
    let err = ingestor
        .ingest_public_api(
            "https://api.example.com/data",
            &request(json!({"headers": {"Authorization": "Bearer token"}})),
        )
        .unwrap_err();
    assert!(err.is_validation_error());
    assert!(err.to_string().contains("no-auth endpoints"));
    assert_eq!(fake.call_count(), 0);
}

#[test]
fn test_public_api_ingestor_parses_string_boolean_config() {
    let _dns = mock_dns();
    let payload = json!([{"id": 1, "title": "hello"}]);
    let fake = FakeTransport::returning(mock_response(
        200,
        Some(payload.clone()),
        r#"[{"id": 1, "title": "hello"}]"#,
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    let mut ingestor = PublicAPIIngestor::new(
        Some(kwargs(json!({"validate_no_auth": "false"}))),
        Some(0.0),
        None,
        Kwargs::new(),
    )
    .unwrap();
    let _resolver = dns_returning(&["93.184.216.34"]);
    let result = ingestor
        .ingest_public_api(
            "https://api.example.com/data",
            &request(json!({"headers": {"Authorization": "Bearer token"}})),
        )
        .unwrap();
    assert!(!ingestor.validate_no_auth);
    assert_eq!(result.data, payload);
}

#[test]
fn test_public_api_convenience_methods_and_registry_dispatch() {
    let _dns = mock_dns();
    let payload = json!([{"id": 1}]);
    let fake = FakeTransport::returning(mock_response(
        200,
        Some(payload.clone()),
        r#"[{"id": 1}]"#,
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    let options = kwargs(json!({"rate_limit_delay": 0}));
    let source = Source::from("https://jsonplaceholder.typicode.com/posts");
    let direct = ingest_public_api(&source, "endpoint", &options).unwrap();
    let unified = ingest(&source, Some("public_api"), None, &options).unwrap();
    let methods = list_available_methods(Some("public_api"));

    let IngestOutput::Api(direct) = direct else {
        panic!("expected APIData, got {direct:?}")
    };
    assert_eq!(direct.data, payload);
    match unified.get("data") {
        Some(IngestOutput::Api(data)) => assert_eq!(data.data, payload),
        other => panic!("unexpected {other:?}"),
    }
    assert!(methods["public_api"].contains(&"endpoint".to_string()));
    assert!(methods["public_api"].contains(&"detect".to_string()));
}

#[test]
fn test_public_api_ingestor_rejects_auth_credential_in_url() {
    let _dns = mock_dns();
    for url in [
        "https://api.example.com/data?api_key=SECRET",
        "https://api.example.com/data?token=abc123",
        "https://api.example.com/data?access_token=xyz&other=1",
    ] {
        let fake = FakeTransport::new();
        let _session = patch_session_class(&fake);
        let err = ingestor()
            .ingest_public_api(url, &PublicApiRequest::default())
            .unwrap_err();
        assert!(err.is_validation_error());
        assert!(err.to_string().contains("no-auth endpoints"));
        assert_eq!(fake.call_count(), 0);
    }
}

#[test]
fn test_public_api_detection_returns_not_public_for_url_with_auth_param() {
    let _dns = mock_dns();
    // detect_public_api does not raise; it returns a detection result with is_public=False
    let fake = FakeTransport::new();
    let _session = patch_session_class(&fake);
    let detection = ingestor()
        .detect_public_api("https://api.example.com/data?api_key=SECRET", &PublicApiRequest::default())
        .unwrap();
    assert!(!detection.is_public);
    assert!(detection.requires_auth);
    let indicators = detection.metadata["auth_indicators"].as_array().cloned().unwrap_or_default();
    assert!(indicators.contains(&json!("url_param:api_key")));
    assert_eq!(fake.call_count(), 0);
}

#[test]
fn test_require_public_false_allows_successful_response() {
    let _dns = mock_dns();
    let payload = json!([{"id": 1}]);
    let fake = FakeTransport::returning(mock_response(
        200,
        Some(payload.clone()),
        r#"[{"id": 1}]"#,
        &[("Content-Type", "application/json")],
    ));
    let _session = patch_session_class(&fake);
    let result = ingestor()
        .ingest_public_api("https://example.com/api/data", &request(json!({"require_public": false})))
        .unwrap();
    assert_eq!(result.data, payload);
    assert_eq!(result.metadata["requires_auth"], json!(false));
}

#[test]
fn test_require_public_false_with_401_still_raises_via_raise_for_status() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(401, None, "", &[]));
    let _session = patch_session_class(&fake);
    let err = ingestor()
        .ingest_public_api("https://example.com/api/data", &request(json!({"require_public": false})))
        .unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
}

#[test]
fn test_html_response_is_not_misclassified_as_xml() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(mock_response(
        200,
        None,
        "<!DOCTYPE html><html><body>403 Forbidden</body></html>",
        &[("Content-Type", "text/html; charset=utf-8")],
    ));
    let _session = patch_session_class(&fake);
    let mut ingestor = PublicAPIIngestor::new(None, Some(0.0), Some(json!(false)), Kwargs::new()).unwrap();
    let result = ingestor
        .ingest_public_api("https://example.com/api/data", &request(json!({"require_public": false})))
        .unwrap();
    assert_eq!(result.metadata["response_format"], "text");
}
