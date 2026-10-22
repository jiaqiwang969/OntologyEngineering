use semantica_json::{
    native_value::{NativeError, NativeLimits},
    original_json::{ParseError, ParseLimits, parse_original_json},
    original_json_bytes::{DecodeError, DecodeLimits},
};
use serde_json::{Value, json};
fn limits() -> ParseLimits {
    ParseLimits {
        decode: DecodeLimits {
            max_input_bytes: 1_000_000,
            max_codepoints: 1_000_000,
        },
        native: NativeLimits {
            max_nodes: 100_000,
            max_links: 100_000,
            max_scalar_bytes: 1_000_000,
            max_depth: 4096,
            max_wire_bytes: 10_000_000,
        },
    }
}
fn unbase64(s: &str) -> Vec<u8> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut acc, mut bits, mut out) = (0u32, 0, Vec::new());
    for b in s.bytes().take_while(|b| *b != b'=') {
        acc = (acc << 6) | alphabet.iter().position(|v| *v == b).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}
fn observed(bytes: &[u8]) -> Value {
    match parse_original_json(bytes, limits()) {
        Ok(value) => json!({"status":"ok","value":value.to_envelope(limits().native).unwrap()}),
        Err(ParseError::Syntax(e)) => {
            json!({"status":"error","class":"JSONDecodeError","message":e.to_string(),"msg":e.msg,"pos":e.pos,"lineno":e.lineno,"colno":e.colno})
        }
        Err(e @ ParseError::IntegerDigits(_)) => {
            json!({"status":"error","class":"ValueError","message":e.to_string()})
        }
        Err(ParseError::Decode(DecodeError::InvalidEncoding(e))) => {
            json!({"status":"error","class":"UnicodeDecodeError","message":e.to_string()})
        }
        Err(e) => panic!("unexpected resource error: {e}"),
    }
}
#[test]
fn original_454_json_results_preserve_values_and_exact_diagnostics() {
    let plan: Value =
        serde_json::from_slice(include_bytes!("fixtures/original_json_bytes_plan-01.json"))
            .unwrap();
    let observed_original: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_json_bytes_observations-01.json"
    ))
    .unwrap();
    compare(&plan, &observed_original, 454, 83);
}

#[test]
fn original_syntax_mutations_and_numeric_boundaries() {
    let plan: Value =
        serde_json::from_slice(include_bytes!("fixtures/original_json_syntax_plan-03.json"))
            .unwrap();
    let observed_original: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_json_syntax_observations-03.json"
    ))
    .unwrap();
    compare(&plan, &observed_original, 3399, 577);
}

fn compare(plan: &Value, observed_original: &Value, count: usize, ok_count: usize) {
    assert_eq!(plan["cases"].as_array().unwrap().len(), count);
    let mut ok = 0;
    let mut differences = Vec::new();
    for (case, row) in plan["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(observed_original["rows"].as_array().unwrap())
    {
        assert_eq!(case["id"], row["id"]);
        let actual = observed(&unbase64(case["base64"].as_str().unwrap()));
        if actual["status"] == "ok" {
            ok += 1;
        }
        if actual != row["parsed"] {
            differences.push(json!({"id":case["id"],"actual":actual,"original":row["parsed"]}));
        }
    }
    assert!(
        differences.is_empty(),
        "{} differences: {:?}",
        differences.len(),
        &differences[..differences.len().min(12)]
    );
    assert_eq!(ok, ok_count);
}
#[test]
fn original_json_resources_refuse_without_relaxing_strict_json() {
    let mut l = limits();
    l.decode.max_input_bytes = 1;
    assert!(matches!(
        parse_original_json(b"[]", l),
        Err(ParseError::Decode(DecodeError::InputLimit))
    ));
    l = limits();
    l.native.max_nodes = 2;
    assert!(matches!(
        parse_original_json(b"[1,2]", l),
        Err(ParseError::Resource(NativeError::NodeLimit))
    ));
    l = limits();
    l.native.max_links = 1;
    assert!(matches!(
        parse_original_json(b"[1,2]", l),
        Err(ParseError::Resource(NativeError::LinkLimit))
    ));
    l = limits();
    l.native.max_scalar_bytes = 3;
    assert!(matches!(
        parse_original_json(br#""a""#, l),
        Err(ParseError::Resource(NativeError::ScalarLimit))
    ));
    l = limits();
    l.native.max_depth = 1;
    assert!(matches!(
        parse_original_json(b"[[0]]", l),
        Err(ParseError::Resource(NativeError::DepthLimit))
    ));
    assert!(parse_original_json(b"[[]]", l).is_ok());
    assert!(matches!(
        parse_original_json(br#"{"a":[0],"a":0}"#, l),
        Err(ParseError::Resource(NativeError::DepthLimit))
    ));
    l.native.max_depth = 0;
    assert!(parse_original_json(b"[]", l).is_ok());
    assert!(semantica_json::strict_json(b"NaN").is_err());
    assert!(semantica_json::strict_json(br#"{"a":1,"a":2}"#).is_err());
    // Shadowed content still consumes parsing workspace. It is not a way to
    // bypass resource limits even though the final retained tree is small.
    l = limits();
    l.native.max_nodes = 4;
    assert!(matches!(
        parse_original_json(br#"{"a":[1,2,3],"a":0}"#, l),
        Err(ParseError::Resource(NativeError::NodeLimit))
    ));
}
