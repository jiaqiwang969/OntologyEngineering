//! RDFLib 7.6.0 derivation; copyright and BSD-3-Clause terms are retained
//! in docs/RDFLIB-SNAPSHOT-LICENSE.md at the workspace root.
//! RDFLib parseQuery's input preparation and original exception payloads.
//! Source grammar checkpoints supply the failure location; no message is
//! inferred by searching the query for a token or copying a native PEG error.

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SourceParserError {
    #[error("{message} at character {location}")]
    ParseException {
        input: String,
        location: usize,
        message: &'static str,
    },
    #[error("{message}")]
    ValueError { message: String },
    #[error("Unreceived source input: {0}")]
    Unreceived(&'static str),
}

/// Original expandUnicodeEscapes uses a case-insensitive \u regex, greedily
/// reading eight hex digits when possible, then parse_string expands tabs.
pub(super) fn prepare(raw: &str) -> Result<String, SourceParserError> {
    let bytes = raw.as_bytes();
    let mut expanded = String::new();
    let mut at = 0;
    while at < bytes.len() {
        let hex = |begin: usize, count: usize| {
            bytes
                .get(begin..begin + count)
                .is_some_and(|v| v.iter().all(u8::is_ascii_hexdigit))
        };
        if bytes[at] == b'\\' && matches!(bytes.get(at + 1), Some(b'u' | b'U')) && hex(at + 2, 4) {
            let digits = if hex(at + 6, 4) { 8 } else { 4 };
            let text = &raw[at + 2..at + 2 + digits];
            let value = u32::from_str_radix(text, 16).expect("checked hex digits");
            if value > 0x10ffff {
                return Err(SourceParserError::ValueError {
                    message: format!("Invalid unicode code point: {text}"),
                });
            }
            // Python can carry isolated surrogates. A Rust UTF-8 query cannot;
            // this is explicitly unreceived, not an invented Python ValueError.
            let Some(value) = char::from_u32(value) else {
                return Err(SourceParserError::Unreceived("isolated Unicode surrogate"));
            };
            expanded.push(value);
            at += digits + 2;
        } else {
            let value = raw[at..].chars().next().expect("in bounds");
            expanded.push(value);
            at += value.len_utf8();
        }
    }
    let mut input = String::new();
    let mut column = 0usize;
    for value in expanded.chars() {
        if value == '\t' {
            let count = 8 - column % 8;
            input.extend(std::iter::repeat_n(' ', count));
            column += count;
        } else {
            input.push(value);
            column = if matches!(value, '\n' | '\r') {
                0
            } else {
                column + 1
            };
        }
    }
    Ok(input)
}
