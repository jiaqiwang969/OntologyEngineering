//! Characterisation tests for `semantica/ingest/api_ingestor.py` and
//! `semantica/ingest/config.py` (expected values from the original run in the oracle
//! environment with `requests.Session` and DNS patched).

mod common;

use common::*;
use sem_ingest::api_ingestor::{APIData, EndpointRequest, RESTIngestor};
use sem_ingest::config::IngestConfig;
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use serde_json::{Map, Value, json};

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

fn resp(status: u16, payload: Value) -> Response {
    let mut r = Response::new(status)
        .with_header("Content-Type", "application/json")
        .with_json(payload);
    if status >= 400 {
        r = r.with_raise_for_status(format!("{status} Client Error"));
    }
    r
}

#[test]
fn api_data_defaults() {
    let d = APIData::new(json!({"a": 1}), 200, "e", Map::new());
    assert!(d.metadata.is_empty());
    assert_eq!((d.response_status, d.endpoint.as_str()), (200, "e"));
}

#[test]
fn rest_batch_request_and_endpoint_options() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![resp(200, json!({"x": 1})), resp(404, json!({})), resp(200, json!([1, 2]))]);
    let _session = patch_session_class(&fake);
    let mut ing = RESTIngestor::new(Kwargs::new()).unwrap();
    let rs = ing
        .batch_request(
            &["https://a.example.com/1", "https://a.example.com/2", "https://a.example.com/3"],
            "POST",
            &EndpointRequest::default(),
        )
        .unwrap();
    let summary: Vec<Value> = rs
        .iter()
        .map(|r| {
            let mut keys: Vec<&String> = r.metadata.keys().collect();
            keys.sort();
            json!([r.endpoint, r.data, r.response_status, keys])
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            json!(["https://a.example.com/1", {"x": 1}, 200, ["content_type", "headers", "method"]]),
            json!(["https://a.example.com/3", [1, 2], 200, ["content_type", "headers", "method"]]),
        ]
    );
    let calls: Vec<(String, String)> = fake.calls().into_iter().map(|c| (c.method, c.url)).collect();
    assert_eq!(
        calls,
        vec![
            ("POST".to_string(), "https://a.example.com/1".to_string()),
            ("POST".to_string(), "https://a.example.com/2".to_string()),
            ("POST".to_string(), "https://a.example.com/3".to_string()),
        ]
    );

    let fake = FakeTransport::sequence(vec![resp(200, json!({"x": 1})), resp(404, json!({}))]);
    let _session = patch_session_class(&fake);
    let mut ing = RESTIngestor::new(kwargs(json!({"fail_fast": true}))).unwrap();
    let e = ing
        .batch_request(
            &["https://a.example.com/1", "https://a.example.com/2"],
            "GET",
            &EndpointRequest::default(),
        )
        .unwrap_err();
    assert_eq!(e.type_name(), "ProcessingError");
    assert_eq!(e.to_string(), "Failed to execute batch request: Failed to ingest endpoint: 404 Client Error");

    let fake = FakeTransport::returning(resp(200, json!({"k": "v"})).with_header("X-Rate", "5"));
    let _session = patch_session_class(&fake);
    let mut ing = RESTIngestor::new(kwargs(json!({"headers": {"X-Custom": "1"}, "timeout": 12}))).unwrap();
    let request = EndpointRequest::from_kwargs(&kwargs(json!({
        "method": "put", "headers": {"H": "2"}, "params": {"p": 1}, "json_data": {"j": 1}
    })))
    .unwrap();
    let r = ing.ingest_endpoint("https://api.example.com/v1", &request).unwrap();
    assert_eq!((r.data.clone(), r.response_status), (json!({"k": "v"}), 200));
    assert_eq!(
        Value::Object(r.metadata.clone()),
        json!({"method": "put", "headers": {"Content-Type": "application/json", "X-Rate": "5"}, "content_type": "application/json"})
    );
    let call = fake.last_call();
    assert_eq!((call.method.as_str(), call.url.as_str()), ("PUT", "https://api.example.com/v1"));
    assert_eq!(call.options.allow_redirects, Some(false));
    let headers: Vec<(String, String)> = call.headers().into_iter().collect();
    assert_eq!(
        headers,
        vec![("X-Custom".to_string(), "1".to_string()), ("H".to_string(), "2".to_string())]
    );
    assert_eq!(call.options.params, Some(vec![("p".to_string(), "1".to_string())]));
    assert_eq!(call.options.json, Some(json!({"j": 1})));
    assert_eq!(call.options.timeout, Some(12.0));
}

#[test]
fn ingest_config_files_and_values() {
    let tmp = tempfile::tempdir().unwrap();
    let yp = tmp.path().join("c.yaml");
    std::fs::write(&yp, "ingest:\n  zz_alpha: 1\n  zz_beta: [a, b]\ningest_methods:\n  web:\n    delay: 2\n").unwrap();
    let c = IngestConfig::new(Some(&yp.to_string_lossy()));
    assert_eq!(c.get("zz_alpha", Value::Null), json!(1));
    assert_eq!(c.get("zz_beta", Value::Null), json!(["a", "b"]));
    assert_eq!(Value::Object(c.get_method_config("web")), json!({"delay": 2}));
    assert_eq!(c.get("zz_missing", json!("dflt")), json!("dflt"));
    c.set("zz_gamma", json!({"x": 1}));
    c.set_method_config("feed", kwargs(json!({"timeout": 5, "validate": false})));
    assert_eq!(c.get("zz_gamma", Value::Null), json!({"x": 1}));
    assert_eq!(Value::Object(c.get_method_config("feed")), json!({"timeout": 5, "validate": false}));
    assert!(c.get_method_config("none").is_empty());
    let all = c.get_all();
    assert_eq!(all.keys().collect::<Vec<_>>(), vec!["config", "method_configs"]);
    let zz: Map<String, Value> = all["config"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| k.starts_with("zz_"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(Value::Object(zz), json!({"zz_alpha": 1, "zz_beta": ["a", "b"], "zz_gamma": {"x": 1}}));
    assert_eq!(
        all["method_configs"],
        json!({"web": {"delay": 2}, "feed": {"timeout": 5, "validate": false}})
    );

    let jp = tmp.path().join("c.json");
    std::fs::write(&jp, r#"{"ingest": {"zz_j": true}, "ingest_methods": {"db": {"x": 1}}}"#).unwrap();
    let cj = IngestConfig::new(Some(&jp.to_string_lossy()));
    assert_eq!(cj.get("zz_j", Value::Null), json!(true));
    assert_eq!(Value::Object(cj.get_method_config("db")), json!({"x": 1}));
    let tp = tmp.path().join("c.toml");
    std::fs::write(&tp, "[ingest]\nzz_t = 3\n[ingest_methods.xml]\nstrict = true\n").unwrap();
    let ct = IngestConfig::new(Some(&tp.to_string_lossy()));
    assert_eq!(ct.get("zz_t", Value::Null), json!(3));
    assert_eq!(Value::Object(ct.get_method_config("xml")), json!({"strict": true}));
    let bp = tmp.path().join("bad.json");
    std::fs::write(&bp, "{bad").unwrap();
    let cb = IngestConfig::new(Some(&bp.to_string_lossy()));
    assert!(!cb.get_all()["config"].as_object().unwrap().keys().any(|k| k.starts_with("zz")));
    let missing = IngestConfig::new(Some(&tmp.path().join("nope.yaml").to_string_lossy()));
    assert!(!missing.get_all()["config"].as_object().unwrap().keys().any(|k| k.starts_with("zz")));
}
