//! Characterisation tests for `semantica/ingest/public_api_ingestor.py` (expected values from
//! the original run in the oracle environment with `requests.Session` and DNS patched).

mod common;

use common::*;
use sem_ingest::public_api_ingestor::{PublicAPIExamples, PublicAPIIngestor, PublicApiRequest};
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use serde_json::{Value, json};

fn json_response(status: u16, payload: Option<Value>) -> Response {
    let mut r = Response::new(status).with_header("Content-Type", "application/json");
    r = match payload {
        Some(p) => r.with_json(p),
        None => r.with_json_error("not json"),
    };
    if status >= 400 {
        r = r.with_raise_for_status(format!("{status} error"));
    }
    r
}

fn ingestor(extra: Value) -> PublicAPIIngestor {
    PublicAPIIngestor::new(None, Some(0.0), None, extra.as_object().cloned().unwrap_or_default()).unwrap()
}

#[test]
fn examples_get_and_names() {
    let e = PublicAPIExamples::get("jsonplaceholder_posts").unwrap();
    assert_eq!(e.name, "jsonplaceholder_posts");
    assert_eq!(e.endpoint, "https://jsonplaceholder.typicode.com/posts");
    assert_eq!(e.method, "GET");
    assert!(e.params.is_empty());
    assert!(e.headers.is_empty());
    assert_eq!(e.response_format, "json");
    assert_eq!(e.record_path, None);
    assert_eq!(e.tags, vec!["json", "testing", "placeholder"]);
    assert_eq!(e.rate_limit_delay, Some(0.5));
    assert_eq!(e.description, "Fake blog post records for REST API testing.");

    let err = PublicAPIExamples::get("nope").unwrap_err();
    assert!(err.is_validation_error());
    assert_eq!(
        err.to_string(),
        "Unknown public API example: nope. Available examples: data_gov_datasets, jsonplaceholder_posts, jsonplaceholder_todos, jsonplaceholder_users, open_meteo_forecast, rest_countries_all"
    );
    assert_eq!(
        PublicAPIExamples::names(Some("testing")),
        vec!["jsonplaceholder_posts", "jsonplaceholder_users", "jsonplaceholder_todos"]
    );
    assert_eq!(
        PublicAPIExamples::names(None),
        vec![
            "jsonplaceholder_posts",
            "jsonplaceholder_users",
            "jsonplaceholder_todos",
            "rest_countries_all",
            "data_gov_datasets",
            "open_meteo_forecast"
        ]
    );
}

#[test]
fn is_public_api_and_examples_ingestion() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(json_response(200, Some(json!([{"id": 1}]))));
    let _session = patch_session_class(&fake);
    // The examples carry `rate_limit_delay=0.5`; skip the real sleeps.
    let _sleep = sem_ingest::support::pytime::SLEEP.set(Box::new(|_| {}));
    let mut ing = ingestor(json!({}));
    assert!(
        ing.is_public_api("https://jsonplaceholder.typicode.com/posts", &PublicApiRequest::default())
            .unwrap()
    );
    let r = ing.ingest_example("jsonplaceholder_posts", &Kwargs::new()).unwrap();
    assert_eq!(r.data, json!([{"id": 1}]));
    assert_eq!(r.endpoint, "https://jsonplaceholder.typicode.com/posts");
    assert_eq!(r.metadata["example_name"], "jsonplaceholder_posts");
    assert_eq!(r.metadata["example_tags"], json!(["json", "testing", "placeholder"]));
    assert_eq!(r.metadata["record_count"], 1);
    assert_eq!(r.metadata["method"], "GET");
    let call = fake.last_call();
    assert_eq!((call.method.as_str(), call.url.as_str()), ("GET", "https://jsonplaceholder.typicode.com/posts"));
    assert_eq!(call.options.params, None);

    let rs = ing
        .ingest_examples(&["jsonplaceholder_posts", "jsonplaceholder_users"], &Kwargs::new())
        .unwrap();
    let names: Vec<&Value> = rs.iter().map(|x| &x.metadata["example_name"]).collect();
    assert_eq!(names, vec!["jsonplaceholder_posts", "jsonplaceholder_users"]);
}

#[test]
fn batch_public_apis_skips_failures_unless_fail_fast() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![
        json_response(200, Some(json!([{"id": 1}]))),
        Response::new(500).with_json_error("not json").with_raise_for_status("500 error"),
        json_response(200, Some(json!({"a": 1}))),
    ]);
    let _session = patch_session_class(&fake);
    let mut ing = ingestor(json!({}));
    let rs = ing
        .batch_public_apis(
            &["https://a.example.com/1", "https://a.example.com/2", "https://a.example.com/3"],
            &PublicApiRequest::default(),
        )
        .unwrap();
    assert_eq!(rs.len(), 2);
    assert_eq!(rs.iter().map(|x| x.data.clone()).collect::<Vec<_>>(), vec![json!([{"id": 1}]), json!([{"a": 1}])]);
    assert_eq!(
        rs.iter().map(|x| x.endpoint.as_str()).collect::<Vec<_>>(),
        vec!["https://a.example.com/1", "https://a.example.com/3"]
    );

    let fake = FakeTransport::sequence(vec![
        json_response(200, Some(json!([{"id": 1}]))),
        Response::new(500).with_json_error("not json").with_raise_for_status("500 error"),
    ]);
    let _session = patch_session_class(&fake);
    let mut ing = ingestor(json!({"fail_fast": true}));
    let err = ing
        .batch_public_apis(
            &["https://a.example.com/1", "https://a.example.com/2"],
            &PublicApiRequest::default(),
        )
        .unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
    assert_eq!(err.to_string(), "Failed to ingest public API: 500 error");
}
