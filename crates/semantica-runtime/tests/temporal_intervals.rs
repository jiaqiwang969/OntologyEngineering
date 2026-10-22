use semantica_contracts::{SemanticRuntimeFailure, SemanticTemporalPort, SemanticTemporalRequest};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};

fn run(value: Value) -> Result<Value, SemanticRuntimeFailure> {
    let input: SemanticTemporalRequest = serde_json::from_value(value).unwrap();
    RustSemanticRuntime
        .temporal(&input)
        .map(|v| serde_json::to_value(v).unwrap())
}
fn req(operation: &str, intervals: Value) -> Value {
    json!({"schema":"semantica.temporal-request/1","operation":operation,"intervals":intervals,"point":null,"domain":null,"granularity":null})
}
fn iv(start: &str, end: Option<&str>) -> Value {
    json!({"start":start,"end":end,"label":null})
}
#[test]
fn temporal_original66_cases() {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/temporal-interval-original66.json")).unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 66);
    for case in cases {
        let result = run(case["request"].clone());
        if case["expected_error"] == true {
            assert!(result.is_err(), "{}", case["id"]);
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("{}: {}:{}", case["id"], e.code, e.message));
        assert_eq!(result["schema"], "semantica.temporal-result/1");
        assert_eq!(result["profile"], "semantica.source-temporal-intervals/1");
        for (key, value) in case["expected"].as_object().unwrap() {
            assert_eq!(&result[key], value, "{}:{key}", case["id"]);
        }
        for key in [
            "relation",
            "overlaps",
            "contains",
            "active",
            "point",
            "coverage_fraction",
        ] {
            if case["expected"].get(key).is_none() {
                assert!(result[key].is_null(), "{}:{key}", case["id"]);
            }
        }
    }
}
#[test]
fn temporal_independent_boundaries_and_identity() {
    let top = "9999-12-31T23:59:59.999999Z";
    let mut value = req("active", json!([iv("2024-01-01", None)]));
    value["point"] = json!(top);
    assert_eq!(run(value.clone()).unwrap()["active"], true);
    value["intervals"][0]["end"] = json!(top);
    assert_eq!(run(value).unwrap()["active"], false);
    let mut value = req("normalize-interval", json!([iv(top, Some(top))]));
    value["granularity"] = json!("month");
    assert_eq!(run(value.clone()).unwrap()["intervals"][0]["end"], top);
    value["granularity"] = json!("day");
    assert!(run(value).is_err());
    let first = req(
        "merge",
        json!([iv("2024-01-01T01:00:00+01:00", Some("2024-01-02"))]),
    );
    let mut second = first.clone();
    second["intervals"][0]["start"] = json!("2024-01-01T00:00:00Z");
    let a = run(first.clone()).unwrap();
    let b = run(second).unwrap();
    assert_eq!(a["intervals"], b["intervals"]);
    assert_ne!(a["request_sha256"], b["request_sha256"]);
    let typed: SemanticTemporalRequest = serde_json::from_value(first).unwrap();
    assert_eq!(
        a["request_sha256"],
        semantica_runtime::sha256(
            semantica_contracts::json::canonical_json(&serde_json::to_value(typed).unwrap())
                .unwrap()
        )
    );
    let mut normalize = req("normalize-point", json!([]));
    normalize["point"] = json!("2024-06-15T01:00:00.000001Z");
    normalize["granularity"] = json!("week");
    let one = run(normalize.clone()).unwrap();
    normalize["point"] = one["point"].clone();
    assert_eq!(run(normalize).unwrap()["point"], one["point"]);
    let mut subsecond = req(
        "coverage",
        json!([iv(
            "2024-01-01T00:00:00.000001Z",
            Some("2024-01-01T00:00:00.000003Z")
        )]),
    );
    subsecond["domain"] = iv("2024-01-01T00:00:00Z", Some("2024-01-01T00:00:00.000004Z"));
    assert_eq!(run(subsecond).unwrap()["coverage_fraction"], 0.5);
}
#[test]
fn temporal_request_guards_and_budgets() {
    let sample = req("merge", json!([iv("2024-01-01", Some("2024-01-02"))]));
    for i in 0..10 {
        let mut bad = sample.clone();
        match i {
            0 => bad["schema"] = json!("semantica.temporal-request/99"),
            1 => bad["operation"] = json!("adopt"),
            2 => bad["point"] = json!("2024-01-01"),
            3 => bad["domain"] = iv("2024-01-01", Some("2024-01-02")),
            4 => bad["granularity"] = json!("day"),
            5 => bad["operation"] = json!("compare"),
            6 => bad["intervals"][0]["label"] = json!("x".repeat(4097)),
            7 => bad["intervals"][0]["start"] = json!(" ".repeat(257)),
            8 => bad["intervals"] = json!(vec![iv("2024-01-01", None); 4097]),
            _ => bad["intervals"][0]["label"] = json!("x".repeat(1024 * 1024)),
        }
        assert!(run(bad).is_err(), "case{i}");
    }
    let mut unknown = sample.clone();
    unknown["authority"] = json!("approved");
    assert!(serde_json::from_value::<SemanticTemporalRequest>(unknown).is_err());
    let mut unknown = sample.clone();
    unknown["intervals"][0]["confidence"] = json!(1);
    assert!(serde_json::from_value::<SemanticTemporalRequest>(unknown).is_err());
    let mut numeric = sample;
    numeric["intervals"][0]["start"] = json!(0);
    assert!(serde_json::from_value::<SemanticTemporalRequest>(numeric).is_err());
    assert_eq!(
        run(req("merge", json!([]))).unwrap()["intervals"],
        json!([])
    );
    let maximum = req("merge", json!(vec![iv("2024-01-01", None); 4096]));
    assert_eq!(
        run(maximum).unwrap()["intervals"].as_array().unwrap().len(),
        1
    );
}
