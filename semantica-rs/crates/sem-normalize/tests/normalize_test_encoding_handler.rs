//! Port of `tests/normalize/test_encoding_handler.py` (`TestEncodingHandler`).
#![allow(non_snake_case)]

use sem_normalize::EncodingHandler;
use sem_normalize::encoding_handler::TextOrBytes;

/// `bytes.decode(encoding)` (strict): `validate_encoding` checks the bytes decode strictly,
/// then `convert_to_utf8` (which decodes with `errors="replace"`) yields the same text.
fn decode(bytes: &[u8], encoding: &str) -> String {
    let handler = EncodingHandler::default();
    assert!(
        handler.validate_encoding(bytes, encoding),
        "{encoding} does not decode the bytes"
    );
    handler.convert_to_utf8(bytes, Some(encoding)).unwrap()
}

fn latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(c as u32).expect("latin-1"))
        .collect()
}

#[test]
fn test_encoding_handler__test_detect_encoding() {
    let handler = EncodingHandler::default();
    let text =
        "C'était déjà l'été à Montréal; François dégustait un café près de l'hôtel. ".repeat(4);

    // UTF-8
    let (encoding, _) = handler.detect(text.as_bytes());
    assert_eq!(encoding.unwrap().to_lowercase(), "utf-8");

    // Latin-1: any compatible codec must decode the bytes back to the text.
    let latin1_bytes = latin1(&text);
    let (encoding, _) = handler.detect(latin1_bytes.as_slice());
    let encoding = encoding.expect("an encoding");
    assert_eq!(decode(&latin1_bytes, &encoding), text);
}

#[test]
fn test_encoding_handler__test_convert_to_utf8() {
    let handler = EncodingHandler::default();
    let text = "Héllò Wörld";
    let latin1_bytes = latin1(text);
    let converted = handler
        .convert_to_utf8(latin1_bytes.as_slice(), Some("latin-1"))
        .unwrap();
    assert_eq!(converted, text);
}

#[test]
fn test_encoding_handler__test_remove_bom() {
    let handler = EncodingHandler::default();
    // UTF-8 BOM
    assert_eq!(
        handler.remove_bom(b"\xef\xbb\xbfHello"),
        TextOrBytes::Bytes(b"Hello".to_vec())
    );
    // String BOM
    assert_eq!(
        handler.remove_bom("\u{feff}Hello"),
        TextOrBytes::Text("Hello".to_string())
    );
}

#[test]
fn test_encoding_handler__test_validate_encoding() {
    let handler = EncodingHandler::default();
    assert!(handler.validate_encoding("Hello", "utf-8"));
    // Invalid sequence for ascii
    assert!(!handler.validate_encoding("Héllò", "ascii"));
}
