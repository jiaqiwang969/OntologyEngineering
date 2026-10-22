//! Python `str` semantics the normalizers rely on: character classes (`isalpha`, `isdigit`,
//! `isdecimal`, `isspace`, `isupper`), case mappings (`title`, `capitalize`, `lower`,
//! `upper`), whitespace `split`/`strip`, the `float()`/`int()` string parsers and the regex
//! classes that match Python's Unicode `\s` and `\w`.
//!
//! Character properties come from Rust's Unicode tables plus two small tables generated from
//! CPython 3.11 (Unicode 14.0): the characters whose full titlecase mapping differs from the
//! uppercase one, and the characters that are `isdigit()` without being `isdecimal()`.

use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::str_repr;

pub(crate) use sem_utils::pycompat::fmt::{is_space, strip};

/// Python's Unicode `\s` as a regex character-class body (use inside `[...]`).
pub(crate) const PY_SPACE: &str = r"\s\x{1c}-\x{1f}";
/// Python's Unicode `\w` (`str.isalnum()` or `_`) as a regex character-class body.
pub(crate) const PY_WORD: &str = r"\p{L}\p{N}_";

fn class(pattern: &str) -> Regex {
    Regex::new(&format!("^[{pattern}]$")).expect("valid character class")
}

static ALPHA: Lazy<Regex> = Lazy::new(|| class(r"\p{L}"));
static DECIMAL: Lazy<Regex> = Lazy::new(|| class(r"\p{Nd}"));
static TITLECASE_LETTER: Lazy<Regex> = Lazy::new(|| class(r"\p{Lt}"));
static NONSPACING_MARK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\p{Mn}").expect("valid class"));
static CASE_IGNORABLE: Lazy<Regex> = Lazy::new(|| {
    class(
        "\\p{Mn}\\p{Me}\\p{Cf}\\p{Lm}\\p{Sk}'.:\u{b7}\u{387}\u{55f}\u{5f4}\u{2018}\u{2019}\
         \u{2024}\u{2027}\u{fe13}\u{fe52}\u{fe55}\u{ff07}\u{ff0e}\u{ff1a}",
    )
});

fn matches(re: &Regex, c: char) -> bool {
    let mut buf = [0u8; 4];
    re.is_match(c.encode_utf8(&mut buf))
}

/// `c.isalpha()`: general category L (Lu, Ll, Lt, Lm, Lo).
pub(crate) fn is_alpha(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic();
    }
    matches(&ALPHA, c)
}

/// `c.isdecimal()`: general category Nd.
pub(crate) fn is_decimal(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    matches(&DECIMAL, c)
}

/// Characters with `Numeric_Type=Digit` (`isdigit()` but not `isdecimal()`), Unicode 14.0.
const DIGIT_EXTRA: &[(u32, u32)] = &[
    (0xB2, 0xB3),
    (0xB9, 0xB9),
    (0x1369, 0x1371),
    (0x19DA, 0x19DA),
    (0x2070, 0x2070),
    (0x2074, 0x2079),
    (0x2080, 0x2089),
    (0x2460, 0x2468),
    (0x2474, 0x247C),
    (0x2488, 0x2490),
    (0x24EA, 0x24EA),
    (0x24F5, 0x24FD),
    (0x24FF, 0x24FF),
    (0x2776, 0x277E),
    (0x2780, 0x2788),
    (0x278A, 0x2792),
    (0x10A40, 0x10A43),
    (0x10E60, 0x10E68),
    (0x11052, 0x1105A),
    (0x1F100, 0x1F10A),
];

/// `c.isdigit()`.
pub(crate) fn is_digit(c: char) -> bool {
    is_decimal(c) || {
        let cp = c as u32;
        DIGIT_EXTRA.iter().any(|&(lo, hi)| (lo..=hi).contains(&cp))
    }
}

/// `str.isdigit()` for a whole string (false when empty).
pub(crate) fn str_is_digit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_digit)
}

/// `c.isupper()` (the Unicode `Uppercase` property).
pub(crate) fn is_upper(c: char) -> bool {
    c.is_uppercase()
}

/// The Unicode `Cased` property.
fn is_cased(c: char) -> bool {
    c.is_lowercase() || c.is_uppercase() || matches(&TITLECASE_LETTER, c)
}

fn is_case_ignorable(c: char) -> bool {
    matches(&CASE_IGNORABLE, c)
}

/// Full titlecase mappings that differ from the full uppercase mapping (CPython 3.11).
const TITLE_DIFF: &[(u32, &str)] = &[
    (0xDF, "Ss"),
    (0x1C4, "\u{1c5}"),
    (0x1C5, "\u{1c5}"),
    (0x1C6, "\u{1c5}"),
    (0x1C7, "\u{1c8}"),
    (0x1C8, "\u{1c8}"),
    (0x1C9, "\u{1c8}"),
    (0x1CA, "\u{1cb}"),
    (0x1CB, "\u{1cb}"),
    (0x1CC, "\u{1cb}"),
    (0x1F1, "\u{1f2}"),
    (0x1F2, "\u{1f2}"),
    (0x1F3, "\u{1f2}"),
    (0x587, "\u{535}\u{582}"),
    (0x1F80, "\u{1f88}"),
    (0x1F81, "\u{1f89}"),
    (0x1F82, "\u{1f8a}"),
    (0x1F83, "\u{1f8b}"),
    (0x1F84, "\u{1f8c}"),
    (0x1F85, "\u{1f8d}"),
    (0x1F86, "\u{1f8e}"),
    (0x1F87, "\u{1f8f}"),
    (0x1F88, "\u{1f88}"),
    (0x1F89, "\u{1f89}"),
    (0x1F8A, "\u{1f8a}"),
    (0x1F8B, "\u{1f8b}"),
    (0x1F8C, "\u{1f8c}"),
    (0x1F8D, "\u{1f8d}"),
    (0x1F8E, "\u{1f8e}"),
    (0x1F8F, "\u{1f8f}"),
    (0x1F90, "\u{1f98}"),
    (0x1F91, "\u{1f99}"),
    (0x1F92, "\u{1f9a}"),
    (0x1F93, "\u{1f9b}"),
    (0x1F94, "\u{1f9c}"),
    (0x1F95, "\u{1f9d}"),
    (0x1F96, "\u{1f9e}"),
    (0x1F97, "\u{1f9f}"),
    (0x1F98, "\u{1f98}"),
    (0x1F99, "\u{1f99}"),
    (0x1F9A, "\u{1f9a}"),
    (0x1F9B, "\u{1f9b}"),
    (0x1F9C, "\u{1f9c}"),
    (0x1F9D, "\u{1f9d}"),
    (0x1F9E, "\u{1f9e}"),
    (0x1F9F, "\u{1f9f}"),
    (0x1FA0, "\u{1fa8}"),
    (0x1FA1, "\u{1fa9}"),
    (0x1FA2, "\u{1faa}"),
    (0x1FA3, "\u{1fab}"),
    (0x1FA4, "\u{1fac}"),
    (0x1FA5, "\u{1fad}"),
    (0x1FA6, "\u{1fae}"),
    (0x1FA7, "\u{1faf}"),
    (0x1FA8, "\u{1fa8}"),
    (0x1FA9, "\u{1fa9}"),
    (0x1FAA, "\u{1faa}"),
    (0x1FAB, "\u{1fab}"),
    (0x1FAC, "\u{1fac}"),
    (0x1FAD, "\u{1fad}"),
    (0x1FAE, "\u{1fae}"),
    (0x1FAF, "\u{1faf}"),
    (0x1FB2, "\u{1fba}\u{345}"),
    (0x1FB3, "\u{1fbc}"),
    (0x1FB4, "\u{386}\u{345}"),
    (0x1FB7, "\u{391}\u{342}\u{345}"),
    (0x1FBC, "\u{1fbc}"),
    (0x1FC2, "\u{1fca}\u{345}"),
    (0x1FC3, "\u{1fcc}"),
    (0x1FC4, "\u{389}\u{345}"),
    (0x1FC7, "\u{397}\u{342}\u{345}"),
    (0x1FCC, "\u{1fcc}"),
    (0x1FF2, "\u{1ffa}\u{345}"),
    (0x1FF3, "\u{1ffc}"),
    (0x1FF4, "\u{38f}\u{345}"),
    (0x1FF7, "\u{3a9}\u{342}\u{345}"),
    (0x1FFC, "\u{1ffc}"),
    (0xFB00, "Ff"),
    (0xFB01, "Fi"),
    (0xFB02, "Fl"),
    (0xFB03, "Ffi"),
    (0xFB04, "Ffl"),
    (0xFB05, "St"),
    (0xFB06, "St"),
    (0xFB13, "\u{544}\u{576}"),
    (0xFB14, "\u{544}\u{565}"),
    (0xFB15, "\u{544}\u{56b}"),
    (0xFB16, "\u{54e}\u{576}"),
    (0xFB17, "\u{544}\u{56d}"),
];

/// Full titlecase mapping of one character (`_PyUnicode_ToTitleFull`).
fn to_title_full(c: char, out: &mut String) {
    let cp = c as u32;
    // Georgian Mkhedruli letters titlecase to themselves (their uppercase is Mtavruli).
    if (0x10D0..=0x10FA).contains(&cp) || (0x10FD..=0x10FF).contains(&cp) {
        out.push(c);
        return;
    }
    match TITLE_DIFF.binary_search_by_key(&cp, |&(k, _)| k) {
        Ok(i) => out.push_str(TITLE_DIFF[i].1),
        Err(_) => out.extend(c.to_uppercase()),
    }
}

/// Lowercase `chars[i]` the way CPython's `lower_ucs4` does (final sigma aware).
fn lower_in_context(chars: &[char], i: usize, out: &mut String) {
    let c = chars[i];
    if c == '\u{3a3}' {
        out.push(if is_final_sigma(chars, i) {
            '\u{3c2}'
        } else {
            '\u{3c3}'
        });
        return;
    }
    out.extend(c.to_lowercase());
}

fn is_final_sigma(chars: &[char], i: usize) -> bool {
    // Preceded by a cased letter (skipping case-ignorable characters)...
    let mut j = i;
    let mut preceded = false;
    while j > 0 {
        j -= 1;
        let c = chars[j];
        if is_case_ignorable(c) {
            continue;
        }
        preceded = is_cased(c);
        break;
    }
    if !preceded {
        return false;
    }
    // ...and not followed by one.
    for &c in &chars[i + 1..] {
        if is_case_ignorable(c) {
            continue;
        }
        return !is_cased(c);
    }
    true
}

/// `str.title()`.
pub(crate) fn title(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut previous_is_cased = false;
    for (i, &c) in chars.iter().enumerate() {
        if previous_is_cased {
            lower_in_context(&chars, i, &mut out);
        } else {
            to_title_full(c, &mut out);
        }
        previous_is_cased = is_cased(c);
    }
    out
}

/// `str.capitalize()` (first character titlecased, the rest lowercased; Python >= 3.8).
pub(crate) fn capitalize(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if i == 0 {
            to_title_full(c, &mut out);
        } else {
            lower_in_context(&chars, i, &mut out);
        }
    }
    out
}

/// `str.lower()`.
pub(crate) fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// `str.upper()`.
pub(crate) fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// `str.split()` with no arguments.
pub(crate) fn split_whitespace(s: &str) -> Vec<&str> {
    s.split(is_space).filter(|p| !p.is_empty()).collect()
}

/// `''.join(c for c in s if unicodedata.category(c) != "Mn")`.
pub(crate) fn remove_nonspacing_marks(s: &str) -> String {
    NONSPACING_MARK.replace_all(s, "").into_owned()
}

/// `_PyUnicode_TransformDecimalAndSpaceToASCII`: Unicode decimal digits become ASCII digits,
/// Unicode whitespace becomes a space, and the first other non-ASCII character ends the
/// string with `?` (which no number parser accepts).
fn transform_decimal_and_space(s: &str) -> String {
    if s.is_ascii() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if (c as u32) < 127 {
            out.push(c);
        } else if is_space(c) {
            out.push(' ');
        } else if let Some(d) = decimal_value(c) {
            out.push(char::from(b'0' + d));
        } else {
            out.push('?');
            break;
        }
    }
    out
}

/// The value of a Unicode decimal digit (category Nd). Decimal digits are encoded in complete
/// contiguous runs of ten, ascending from 0 (a Unicode stability guarantee); adjacent runs
/// (e.g. the mathematical digits) form chains, so the value is the distance to the start of
/// the chain modulo ten.
fn decimal_value(c: char) -> Option<u8> {
    if !is_decimal(c) {
        return None;
    }
    if let Some(d) = c.to_digit(10) {
        return Some(d as u8);
    }
    let cp = c as u32;
    let mut start = cp;
    while let Some(prev) = start.checked_sub(1).and_then(char::from_u32) {
        if !is_decimal(prev) {
            break;
        }
        start -= 1;
    }
    Some(((cp - start) % 10) as u8)
}

fn is_ascii_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

fn strip_ascii_space(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_ascii() && is_ascii_space(c as u8))
}

/// Remove underscores the way `_Py_string_to_number_with_underscores` validates them: an
/// underscore must sit between two digits. `None` when the placement is invalid.
fn remove_underscores(s: &str) -> Option<String> {
    if !s.contains('_') {
        return Some(s.to_string());
    }
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut prev = 0u8;
    for &b in bytes {
        if b == b'_' {
            if !prev.is_ascii_digit() {
                return None;
            }
        } else {
            if prev == b'_' && !b.is_ascii_digit() {
                return None;
            }
            out.push(b as char);
        }
        prev = b;
    }
    if prev == b'_' {
        return None;
    }
    Some(out)
}

fn float_error(original: &str) -> SemanticaError {
    SemanticaError::Value(format!(
        "could not convert string to float: {}",
        str_repr(original)
    ))
}

/// `float(s)` for a string: Python's grammar (sign, digits with single underscores between
/// them, optional fraction and exponent, `inf`/`infinity`/`nan` in any case, surrounding
/// whitespace, Unicode decimal digits) and correctly rounded values.
pub(crate) fn py_float(s: &str) -> Result<f64, SemanticaError> {
    let transformed = transform_decimal_and_space(s);
    let trimmed = strip_ascii_space(&transformed);
    if trimmed.is_empty() {
        return Err(float_error(s));
    }
    let cleaned = remove_underscores(trimmed).ok_or_else(|| float_error(s))?;
    parse_float_literal(&cleaned).ok_or_else(|| float_error(s))
}

fn parse_float_literal(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let negative = match bytes.first() {
        Some(b'-') => {
            i = 1;
            true
        }
        Some(b'+') => {
            i = 1;
            false
        }
        _ => false,
    };
    let rest = &s[i..];
    let lower = rest.to_ascii_lowercase();
    if lower == "inf" || lower == "infinity" {
        return Some(if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    if lower == "nan" {
        return Some(if negative { -f64::NAN } else { f64::NAN });
    }
    // digits [. digits] | . digits, then optional exponent.
    let rb = rest.as_bytes();
    let mut j = 0;
    let mut mantissa_digits = 0;
    while j < rb.len() && rb[j].is_ascii_digit() {
        j += 1;
        mantissa_digits += 1;
    }
    if j < rb.len() && rb[j] == b'.' {
        j += 1;
        while j < rb.len() && rb[j].is_ascii_digit() {
            j += 1;
            mantissa_digits += 1;
        }
    }
    if mantissa_digits == 0 {
        return None;
    }
    if j < rb.len() && (rb[j] == b'e' || rb[j] == b'E') {
        let mut k = j + 1;
        if k < rb.len() && (rb[k] == b'+' || rb[k] == b'-') {
            k += 1;
        }
        let start = k;
        while k < rb.len() && rb[k].is_ascii_digit() {
            k += 1;
        }
        if k == start {
            return None;
        }
        j = k;
    }
    if j != rb.len() {
        return None;
    }
    let value: f64 = rest.parse().ok()?;
    Some(if negative { -value } else { value })
}

/// The error `int(s)` raises for a string it cannot parse (repr truncated to 200 characters,
/// as `%.200R` does).
pub(crate) fn int_error(original: &str) -> SemanticaError {
    let repr: String = str_repr(original).chars().take(200).collect();
    SemanticaError::Value(format!("invalid literal for int() with base 10: {repr}"))
}

/// `int(s)` for a string in base 10. Python integers are unbounded; values outside `i128`
/// report [`IntParse::Overflow`].
pub(crate) fn py_int(s: &str) -> Result<i128, SemanticaError> {
    match parse_int(s) {
        IntParse::Value(v) => Ok(v),
        IntParse::Overflow => Err(SemanticaError::Overflow(
            "integer is too large for the Rust port (beyond i128)".to_string(),
        )),
        IntParse::Invalid => Err(int_error(s)),
    }
}

/// Outcome of parsing a base-10 integer literal.
pub(crate) enum IntParse {
    Value(i128),
    Overflow,
    Invalid,
}

pub(crate) fn parse_int(s: &str) -> IntParse {
    let transformed = transform_decimal_and_space(s);
    let trimmed = strip_ascii_space(&transformed);
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    let negative = match bytes.first() {
        Some(b'-') => {
            i = 1;
            true
        }
        Some(b'+') => {
            i = 1;
            false
        }
        _ => false,
    };
    let digits = &trimmed[i..];
    if digits.is_empty() || !digits.as_bytes()[0].is_ascii_digit() {
        return IntParse::Invalid;
    }
    let Some(cleaned) = remove_underscores(digits) else {
        return IntParse::Invalid;
    };
    if !cleaned.bytes().all(|b| b.is_ascii_digit()) {
        return IntParse::Invalid;
    }
    let mut value: i128 = 0;
    for b in cleaned.bytes() {
        let d = i128::from(b - b'0');
        value = match value.checked_mul(10).and_then(|v| v.checked_add(d)) {
            Some(v) => v,
            None => return IntParse::Overflow,
        };
    }
    IntParse::Value(if negative { -value } else { value })
}

/// Number of Unicode code points (`len(s)`).
pub(crate) fn char_len(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_capitalize_follow_python() {
        assert_eq!(title("hello world"), "Hello World");
        assert_eq!(title("they're bill's"), "They'Re Bill'S");
        assert_eq!(title("straße"), "Straße");
        assert_eq!(title("ßa"), "Ssa");
        assert_eq!(capitalize("iII"), "Iii");
        assert_eq!(capitalize("ﬁsh"), "Fish");
        assert_eq!(title("ǆemal"), "ǅemal");
    }

    #[test]
    fn float_and_int_parsers_follow_python() {
        assert_eq!(py_float(" 1_000.5 ").unwrap(), 1000.5);
        assert!(py_float("1__0").is_err());
        assert!(py_float("_1").is_err());
        assert_eq!(py_float("-Infinity").unwrap(), f64::NEG_INFINITY);
        assert!(py_float("nan").unwrap().is_nan());
        assert_eq!(py_float("\u{661}\u{662}").unwrap(), 12.0);
        assert_eq!(py_float("5.").unwrap(), 5.0);
        assert_eq!(py_float(".5").unwrap(), 0.5);
        assert!(py_float("1e").is_err());
        assert_eq!(
            py_float("abc").unwrap_err().to_string(),
            "could not convert string to float: 'abc'"
        );
        assert_eq!(py_int(" 007 ").unwrap(), 7);
        assert!(py_int("1.5").is_err());
        assert_eq!(
            py_int("x").unwrap_err().to_string(),
            "invalid literal for int() with base 10: 'x'"
        );
    }

    #[test]
    fn character_classes() {
        assert!(is_digit('\u{b2}'));
        assert!(!is_decimal('\u{b2}'));
        assert!(is_alpha('é'));
        assert!(!is_alpha('\u{301}'));
        assert!(is_space('\u{1c}'));
    }
}
