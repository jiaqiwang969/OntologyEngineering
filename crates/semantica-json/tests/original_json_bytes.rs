use semantica_json::original_json_bytes::{
    DecodeError, DecodeLimits, decode_original_bytes, detect_encoding,
};
use serde_json::{Value, json};

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

#[test]
fn original_454_byte_decodings_keep_all_codepoints_and_exact_errors() {
    let plan: Value =
        serde_json::from_slice(include_bytes!("fixtures/original_json_bytes_plan-01.json"))
            .unwrap();
    let observations: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_json_bytes_observations-01.json"
    ))
    .unwrap();
    compare(&plan, &observations, 454, (279, 175));
}

#[test]
fn original_1096_bom_surrogate_and_arbitrary_byte_boundaries() {
    let plan: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_json_bytes_boundary_plan-02.json"
    ))
    .unwrap();
    let observations: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_json_bytes_boundary_observations-02.json"
    ))
    .unwrap();
    compare(&plan, &observations, 1096, (70, 1026));
}

fn compare(plan: &Value, observations: &Value, count: usize, expected: (usize, usize)) {
    let cases = plan["cases"].as_array().unwrap();
    let rows = observations["rows"].as_array().unwrap();
    assert_eq!(cases.len(), count);
    assert_eq!(rows.len(), cases.len());
    let mut counts = (0, 0);
    for (case, row) in cases.iter().zip(rows) {
        assert_eq!(case["id"], row["id"]);
        let bytes = unbase64(case["base64"].as_str().unwrap());
        assert_eq!(bytes.len() as u64, case["bytes"].as_u64().unwrap());
        assert_eq!(
            detect_encoding(&bytes).name(),
            row["detected_encoding"].as_str().unwrap(),
            "{}",
            case["id"]
        );
        let actual = match decode_original_bytes(
            &bytes,
            DecodeLimits {
                max_input_bytes: 1_000_000,
                max_codepoints: 1_000_000,
            },
        ) {
            Ok(v) => {
                counts.0 += 1;
                assert_eq!(
                    v.detected_encoding.name(),
                    row["detected_encoding"].as_str().unwrap()
                );
                json!({"status":"ok","codepoints":v.codepoints})
            }
            Err(DecodeError::InvalidEncoding(e)) => {
                counts.1 += 1;
                json!({"status":"error","class":"UnicodeDecodeError","encoding":e.encoding,"start":e.start,"end":e.end,"reason":e.reason,"message":e.to_string()})
            }
            Err(e) => panic!("unexpected resource error {}: {e}", case["id"]),
        };
        assert_eq!(actual, row["decoded"], "{}", case["id"]);
    }
    assert_eq!(counts, expected);
}

#[test]
fn byte_decoding_limits_and_old_strict_json_are_separate() {
    let limits = DecodeLimits {
        max_input_bytes: 6,
        max_codepoints: 1,
    };
    assert_eq!(
        decode_original_bytes(b"null", limits),
        Err(DecodeError::CodepointLimit)
    );
    assert_eq!(
        decode_original_bytes(b"1234567", limits),
        Err(DecodeError::InputLimit)
    );
    assert_eq!(
        decode_original_bytes(&[0xed, 0xa0, 0x80], limits)
            .unwrap()
            .codepoints,
        vec![0xd800]
    );
    assert_eq!(
        decode_original_bytes(
            &[0xef, 0xbb, 0xbf],
            DecodeLimits {
                max_input_bytes: 3,
                max_codepoints: 0
            }
        )
        .unwrap()
        .codepoints,
        Vec::<u32>::new()
    );
    assert_eq!(
        decode_original_bytes(
            b"",
            DecodeLimits {
                max_input_bytes: 0,
                max_codepoints: 0
            }
        )
        .unwrap()
        .codepoints,
        Vec::<u32>::new()
    );
    assert!(semantica_json::strict_json(br#""\ud800""#).is_err());
    assert!(semantica_json::strict_json(b"NaN").is_err());
    // Decoding is not syntax admission: raw NaN or malformed JSON can decode.
    assert!(
        decode_original_bytes(
            b"NaN",
            DecodeLimits {
                max_input_bytes: 3,
                max_codepoints: 3
            }
        )
        .is_ok()
    );
    assert!(decode_original_bytes(b"[", limits).is_ok());
}
