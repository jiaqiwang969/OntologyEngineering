use semantica_contracts::{SemanticSnapshotChecksumPort, SemanticSnapshotChecksumRequest};
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::{Value, json};

fn request(case: &Value) -> Value {
    json!({
        "schema":"semantica.snapshot-checksum-request/1",
        "operation":case["operation"],
        "representation":if case["value"]["schema"]=="semantica.native-value/1"{"native_value"}else{"plain_json"},
        "value":case["value"],"options":{}
    })
}
fn run(value: Value) -> Result<Value, semantica_contracts::SemanticRuntimeFailure> {
    RustSemanticRuntime
        .snapshot_checksum(
            &serde_json::from_value::<SemanticSnapshotChecksumRequest>(value).unwrap(),
        )
        .map(|out| serde_json::to_value(out).unwrap())
}

#[test]
fn snapshot_checksum_original141() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/snapshot-checksum-original141.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 141);
    for case in cases {
        let got = run(request(&case["input"])).unwrap();
        let expected = &case["expected"];
        assert_eq!(got["result"], expected["result"], "{}", case["input"]["id"]);
        if let Some(text) = expected.get("serialized_ascii") {
            assert_eq!(&got["serialized_ascii"], text, "{}", case["input"]["id"]);
        }
        if expected["error"].is_null() {
            assert!(got["failure"].is_null(), "{got}");
        } else {
            assert_eq!(
                got["failure"]["message"],
                format!(
                    "{}: {}",
                    expected["error"]["type"].as_str().unwrap(),
                    expected["error"]["message"].as_str().unwrap()
                ),
                "{}",
                case["input"]["id"]
            );
        }
    }
}

#[test]
fn snapshot_checksum_native_admission_preserves_original_short_circuit() {
    let frozen: Value = serde_json::from_str(include_str!(
        "fixtures/snapshot-checksum-resource-original.json"
    ))
    .unwrap();
    let digits = frozen["integer_digits"].as_u64().unwrap() as usize;
    let value = json!({
        "schema":"semantica.native-value/1",
        "nodes":[
            {"kind":"text","codepoints":[99,104,101,99,107,115,117,109]},
            {"kind":"bool","value":false},
            {"kind":"text","codepoints":[97]},
            {"kind":"int","decimal":format!("1{}", "0".repeat(digits-1))},
            {"kind":"dict","entries":[[0,1],[2,3]]}
        ],"root":4
    });
    for case in frozen["cases"].as_array().unwrap() {
        let got = run(request(&json!({
            "operation":case["operation"], "value":value
        })))
        .unwrap();
        assert_eq!(got["result"], case["expected"]["result"]);
        let error = &case["expected"]["error"];
        if error.is_null() {
            assert!(got["failure"].is_null());
        } else {
            assert_eq!(
                got["failure"]["message"],
                format!(
                    "{}: {}",
                    error["type"].as_str().unwrap(),
                    error["message"].as_str().unwrap()
                )
            );
        }
    }
}

#[test]
fn snapshot_checksum_contract_refusals_and_identity() {
    let base = request(&json!({"operation":"compute_checksum","value":{"a":1}}));
    let good = run(base.clone()).unwrap();
    for (key, value) in [
        ("schema", json!("unknown")),
        ("operation", json!("unknown")),
        ("representation", json!("unknown")),
    ] {
        let mut bad = base.clone();
        bad[key] = value;
        assert_eq!(run(bad).unwrap_err().code, "snapshot_checksum_invalid");
    }
    let mut malformed = base.clone();
    malformed["representation"] = json!("native_value");
    assert_eq!(
        run(malformed).unwrap_err().code,
        "snapshot_checksum_native_admission"
    );
    let mut contextual = base.clone();
    contextual["options"] = json!({"purpose":"independent context"});
    let changed = run(contextual).unwrap();
    assert_eq!(good["result"], changed["result"]);
    assert_ne!(good["request_sha256"], changed["request_sha256"]);
    // A user dictionary resembling a carrier is still an ordinary dictionary
    // when the explicit representation says plain_json.
    let mut ordinary = base;
    ordinary["value"] = json!({"schema":"semantica.native-value/1","nodes":"ordinary text"});
    assert!(run(ordinary).unwrap()["failure"].is_null());
}
