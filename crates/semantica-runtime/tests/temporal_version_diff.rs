use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalVersionDiffPort, SemanticTemporalVersionDiffRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};
fn request(case: &Value, seed: u64) -> Value {
    json!({"schema":"semantica.temporal-version-diff-request/1","version1":case["version1"],"version2":case["version2"],"stored_versions":case["stored_versions"],"hash_seed":seed,"comparison_metrics":["ignored"],"options":{"unused":true}})
}
fn run(input: Value) -> Result<Value, SemanticRuntimeFailure> {
    RustSemanticRuntime
        .temporal_version_diff(
            &serde_json::from_value::<SemanticTemporalVersionDiffRequest>(input).unwrap(),
        )
        .map(|r| serde_json::to_value(r).unwrap())
}
#[test]
fn temporal_version_diff_original54() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/temporal-version-diff-original54.json"
    ))
    .unwrap();
    assert_eq!(fixture["inputs"]["cases"].as_array().unwrap().len(), 54);
    for observation in fixture["observed"].as_array().unwrap() {
        let seed = observation["seed"].as_u64().unwrap();
        for (i, case) in fixture["inputs"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let input = request(case, seed);
            let saved = input.clone();
            let got = run(input.clone()).unwrap_or_else(|e| panic!("{}:{}", case["id"], e.message));
            let expected = &observation["cases"][i];
            assert_eq!(
                serde_json::to_string(&got["result"]).unwrap(),
                serde_json::to_string(&expected["result"]).unwrap(),
                "seed {seed} {}",
                case["id"]
            );
            assert_eq!(got["storage_reads"], expected["storage_reads"]);
            assert_eq!(got["hash_seed"], seed);
            assert_eq!(input, saved);
            if expected["error"].is_null() {
                assert!(got["failure"].is_null());
            } else {
                assert!(got["result"].is_null());
                assert_eq!(
                    got["failure"]["message"],
                    format!(
                        "{}: {}",
                        expected["error"]["type"].as_str().unwrap(),
                        expected["error"]["message"].as_str().unwrap()
                    ),
                    "{}",
                    case["id"]
                );
            }
        }
    }
}
#[test]
fn temporal_version_diff_resources_and_identity() {
    let base = request(
        &json!({"version1":{},"version2":{},"stored_versions":{}}),
        0,
    );
    let baseline = run(base.clone()).unwrap();
    for (key, value) in [
        ("schema", json!("invalid")),
        ("hash_seed", json!(4294967296u64)),
        ("stored_versions", json!([])),
        ("options", json!("x".repeat(1024 * 1024))),
    ] {
        let mut r = base.clone();
        r[key] = value;
        assert!(run(r).is_err(), "{key}");
    }
    for (key, value) in [
        ("comparison_metrics", json!("still ignored")),
        ("hash_seed", json!(17)),
        ("options", json!({"extra":false})),
    ] {
        let mut r = base.clone();
        r[key] = value;
        let out = run(r).unwrap();
        assert_eq!(out["result"], baseline["result"]);
        assert_ne!(out["request_sha256"], baseline["request_sha256"]);
    }
    let mut r = base.clone();
    r["options"] = serde_json::from_str(&"9".repeat(4301)).unwrap();
    assert_eq!(
        run(r).unwrap_err().code,
        "temporal_version_diff_ingress_failure"
    );
    let mut r = base.clone();
    r["version1"] = json!({"entities":vec![json!({});16385]});
    assert_eq!(
        run(r).unwrap_err().code,
        "temporal_version_diff_resource_limit"
    );
    // Different arbitrary-size integer keys deliberately share the Python hash.
    let mut r = base;
    r["version1"] = json!({"entities":(0..8000u64).map(|i|serde_json::from_str::<Value>(&format!("{{\"id\":{}}}",i as u128*((1u128<<61)-1))).unwrap()).collect::<Vec<_>>()});
    assert_eq!(
        run(r).unwrap_err().code,
        "temporal_version_diff_resource_limit"
    );
}
