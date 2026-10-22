//! `bytes.decode("utf-8")` with CPython's `UnicodeDecodeError` messages
//! (`'utf-8' codec can't decode byte 0xff in position 0: invalid start byte`).

use sem_utils::SemanticaError;

/// The `UnicodeDecodeError` message for `err` raised while decoding `bytes`.
pub fn utf8_error_message(bytes: &[u8], err: &std::str::Utf8Error) -> String {
    let start = err.valid_up_to();
    let (end, reason) = match err.error_len() {
        None => (bytes.len() - 1, "unexpected end of data"),
        Some(n) => {
            let first = bytes[start];
            let invalid_start = (0x80..=0xbf).contains(&first) || first == 0xc0 || first == 0xc1 || first >= 0xf5;
            let reason = if invalid_start { "invalid start byte" } else { "invalid continuation byte" };
            (start + n - 1, reason)
        }
    };
    if end > start {
        format!("'utf-8' codec can't decode bytes in position {start}-{end}: {reason}")
    } else {
        format!(
            "'utf-8' codec can't decode byte 0x{:02x} in position {start}: {reason}",
            bytes[start]
        )
    }
}

/// `bytes.decode("utf-8")`.
pub fn decode_utf8(bytes: &[u8]) -> Result<String, SemanticaError> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(e) => Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "UnicodeDecodeError".to_string(),
            message: utf8_error_message(bytes, &e),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_cpython() {
        let msg = |b: &[u8]| decode_utf8(b).unwrap_err().to_string();
        assert_eq!(msg(b"\xff"), "'utf-8' codec can't decode byte 0xff in position 0: invalid start byte");
        assert_eq!(msg(b"a\xc3\x28"), "'utf-8' codec can't decode byte 0xc3 in position 1: invalid continuation byte");
        assert_eq!(msg(b"\xc3"), "'utf-8' codec can't decode byte 0xc3 in position 0: unexpected end of data");
        assert_eq!(msg(b"\xe2\x82"), "'utf-8' codec can't decode bytes in position 0-1: unexpected end of data");
        assert_eq!(msg(b"\xe2\x82\x28"), "'utf-8' codec can't decode bytes in position 0-1: invalid continuation byte");
        assert_eq!(decode_utf8("café".as_bytes()).unwrap(), "café");
    }
}
