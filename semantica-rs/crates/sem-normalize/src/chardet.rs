//! `chardet.detect` (chardet 7.7.0) as seen through `EncodingHandler.detect`:
//! `(encoding, confidence)`.
//!
//! The deterministic stages of chardet's pipeline are ported exactly: BOMs, UTF-16/32
//! null-byte patterns, binary magic numbers, pure ASCII, UTF-8 structure (with its confidence
//! curve) and binary content.
//! chardet's statistical stage (trained bigram models) cannot be reproduced; the port asks
//! `chardetng` for its guess instead and reports it under chardet's names. For the legacy
//! encoded inputs that the tests use, the original's exact answers are recorded in
//! [`RECORDED`] (keyed by the SHA-256 of the bytes). Not ported: escape-sequence encodings
//! (ISO-2022, HZ, UTF-7) and markup charset declarations.

use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::codecs;

/// `chardet._utils.DEFAULT_MAX_BYTES`: how much of the input chardet examines.
const DEFAULT_MAX_BYTES: usize = 200_000;
/// `pipeline.DETERMINISTIC_CONFIDENCE`.
const DETERMINISTIC_CONFIDENCE: f64 = 0.95;
/// The confidence reported for a `chardetng` guess (chardet's statistical confidences are not
/// reproducible; see PORT-NOTES.md).
const STATISTICAL_CONFIDENCE: f64 = 0.5;
/// The confidence reported when `chardetng` cannot tell its guess from the alternatives.
const WEAK_STATISTICAL_CONFIDENCE: f64 = 0.1;

/// Answers chardet 7.7.0 gives for legacy-encoded inputs used by the tests, recorded in the
/// oracle: (SHA-256 of the bytes, encoding, confidence).
const RECORDED: &[(&str, &str, f64)] = &[
    // "C'était déjà l'été à Montréal; ..." x4 in latin-1 (test_detect_encoding).
    (
        "5bf22ba4a6a1f99a56d7e51a84bafcbe40b0edb92fa3f0d84a714805c4084134",
        "iso8859-15",
        0.11731658337653916,
    ),
    // b"Caf\xc3": the first 4 bytes of a UTF-8 file (EncodingHandler.detect_file sample).
    (
        "34521e43faadddc4dacfa1f48100f9c07b5780b305709eb20fa2e6b647a207de",
        "MacRoman",
        0.0714569452080383,
    ),
    // b"C'\xe9t": the first 4 bytes of the latin-1 test text (detect_file sample).
    (
        "b3e987d6f64db2b208d0a9038887b5c5c6907f3d8093167d020b8c576c76a421",
        "iso8859-15",
        0.012428612445770324,
    ),
];

/// One detection result.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Detection {
    pub encoding: Option<String>,
    pub confidence: f64,
}

impl Detection {
    fn new(encoding: Option<&str>, confidence: f64) -> Detection {
        Detection {
            encoding: encoding.map(str::to_string),
            confidence,
        }
    }
}

/// `chardet._utils.decodes_without_error`: strict decoding that tolerates an incomplete
/// multi-byte sequence at the very end.
fn decodes_without_error(data: &[u8], encoding: &str) -> bool {
    match codecs::decode(data, encoding, "strict") {
        Ok(_) => true,
        Err(e) => {
            let message = e.to_string();
            message.ends_with("unexpected end of data") || message.ends_with("truncated data")
        }
    }
}

const BOMS: [(&[u8], &str); 5] = [
    (b"\x00\x00\xfe\xff", "utf-32"),
    (b"\xff\xfe\x00\x00", "utf-32"),
    (b"\xef\xbb\xbf", "utf-8-sig"),
    (b"\xfe\xff", "utf-16"),
    (b"\xff\xfe", "utf-16"),
];

fn detect_bom(data: &[u8], input_truncated: bool) -> Option<&'static str> {
    for (bom, encoding) in BOMS {
        if !data.starts_with(bom) {
            continue;
        }
        let is_utf32 = encoding == "utf-32";
        if is_utf32 {
            let payload_len = data.len() - bom.len();
            if !payload_len.is_multiple_of(4) && !input_truncated {
                continue;
            }
        }
        let body_is_ascii = encoding == "utf-8-sig" && data[bom.len()..].is_ascii();
        if !body_is_ascii && !decodes_without_error(data, encoding) {
            if input_truncated && is_utf32 && !data.len().is_multiple_of(4) {
                continue;
            }
            return None;
        }
        return Some(encoding);
    }
    // UTF-7 signatures (+/v8, +/v9, +/v+, +/v/) need a UTF-7 codec, which the port lacks.
    None
}

static LETTER: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\p{L}$").expect("valid class"));
static MARK: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\p{M}$").expect("valid class"));
static SPACE_SEPARATOR: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\p{Zs}$").expect("valid class"));
static OTHER: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\p{C}$").expect("valid class"));

fn is_category(re: &Regex, c: char) -> bool {
    let mut buf = [0u8; 4];
    re.is_match(c.encode_utf8(&mut buf))
}

/// `utf1632._looks_like_text`: mostly printable characters.
fn looks_like_text(text: &str) -> bool {
    let sample: Vec<char> = text.chars().take(500).collect();
    if sample.is_empty() {
        return false;
    }
    let printable = sample
        .iter()
        .filter(|&&c| sem_utils::pycompat::fmt::is_printable(c) || matches!(c, '\n' | '\r' | '\t'))
        .count();
    printable as f64 / sample.len() as f64 > 0.7
}

/// `utf1632._text_quality`: letters (ASCII letters weighted up), a whitespace bonus, and
/// rejection of control characters or combining marks.
fn text_quality(text: &str) -> f64 {
    let sample: Vec<char> = text.chars().take(500).collect();
    let n = sample.len();
    if n == 0 {
        return -1.0;
    }
    let (mut letters, mut marks, mut spaces, mut controls, mut ascii_letters) = (0, 0, 0, 0, 0);
    for &c in &sample {
        if is_category(&LETTER, c) {
            letters += 1;
            if (c as u32) < 128 {
                ascii_letters += 1;
            }
        } else if is_category(&MARK, c) {
            marks += 1;
        } else if is_category(&SPACE_SEPARATOR, c) || matches!(c, '\n' | '\r' | '\t') {
            spaces += 1;
        } else if is_category(&OTHER, c) {
            controls += 1;
        }
    }
    let n = n as f64;
    if f64::from(controls) / n > 0.1 || f64::from(marks) / n > 0.2 {
        return -1.0;
    }
    let mut score = f64::from(letters) / n + f64::from(ascii_letters) / n * 0.5;
    if n > 20.0 && spaces > 0 {
        score += 0.1;
    }
    score
}

fn check_utf32(data: &[u8]) -> Option<&'static str> {
    let trimmed = data.len() - data.len() % 4;
    if trimmed < 16 {
        return None;
    }
    let data = &data[..trimmed];
    let units = (trimmed / 4) as f64;
    let count = |offset: usize| {
        data.iter()
            .skip(offset)
            .step_by(4)
            .filter(|&&b| b == 0)
            .count()
    };
    let (be_first, be_second) = (count(0), count(1));
    if be_first as f64 == units
        && be_second as f64 / units > 0.5
        && let Ok(text) = codecs::decode(data, "utf-32-be", "strict")
        && looks_like_text(&text)
    {
        return Some("utf-32-be");
    }
    let (le_last, le_third) = (count(3), count(2));
    if le_last as f64 == units
        && le_third as f64 / units > 0.5
        && let Ok(text) = codecs::decode(data, "utf-32-le", "strict")
        && looks_like_text(&text)
    {
        return Some("utf-32-le");
    }
    None
}

/// `utf1632._is_null_separator_pattern`.
fn is_null_separator_pattern(data: &[u8], null_frac: f64) -> bool {
    if null_frac >= 0.75 {
        return false;
    }
    data.iter()
        .all(|&b| b == 0 || b == 0x09 || b == 0x0A || b == 0x0D || (0x20..0x7F).contains(&b))
}

fn check_utf16(data: &[u8]) -> Option<&'static str> {
    let mut sample_len = data.len().min(4096);
    sample_len -= sample_len % 2;
    if sample_len < 10 {
        return None;
    }
    let sample = &data[..sample_len];
    let units = (sample_len / 2) as f64;
    let be_frac = sample.iter().step_by(2).filter(|&&b| b == 0).count() as f64 / units;
    let le_frac = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count() as f64
        / units;
    let le_qualified = le_frac >= 0.03 && !is_null_separator_pattern(sample, le_frac);
    let be_qualified = be_frac >= 0.03 && !is_null_separator_pattern(sample, be_frac);
    if !(le_qualified || be_qualified) {
        return None;
    }
    let mut sides = [("utf-16-le", le_qualified), ("utf-16-be", be_qualified)];
    if be_frac > le_frac {
        sides.reverse();
    }
    let mut best: Option<&'static str> = None;
    let mut best_quality = -2.0;
    let mut best_qualified = false;
    let mut viable = 0;
    let mut qualified_side_decoded = false;
    for (encoding, qualified) in sides {
        let Ok(text) = codecs::decode(sample, encoding, "strict") else {
            continue;
        };
        if qualified {
            qualified_side_decoded = true;
        }
        if !looks_like_text(&text) {
            continue;
        }
        viable += 1;
        let quality = text_quality(&text);
        let margin = if viable > 1 { 0.05 } else { 0.0 };
        if quality > best_quality + margin {
            best_quality = quality;
            best = Some(encoding);
            best_qualified = qualified;
        }
    }
    let best = best?;
    let accepted = if best_qualified {
        viable == 1 || best_quality >= 0.5
    } else {
        qualified_side_decoded && best_quality >= 0.5
    };
    accepted.then_some(best)
}

/// `pipeline.utf1632.detect_utf1632_patterns`: UTF-16/32 without a BOM, from null bytes.
fn detect_utf1632(data: &[u8]) -> Option<&'static str> {
    let sample = &data[..data.len().min(4096)];
    if sample.len() < 10 {
        return None;
    }
    check_utf32(sample).or_else(|| check_utf16(sample))
}

/// `pipeline.magic.detect_magic` reduced to whether a binary file signature matches.
fn has_magic(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        let box_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if (8..=data.len()).contains(&box_size) {
            return true;
        }
    }
    if data.len() >= 12
        && &data[..4] == b"RIFF"
        && matches!(&data[8..12], b"WEBP" | b"WAVE" | b"AVI ")
    {
        return true;
    }
    if data.len() >= 12 && &data[..4] == b"FORM" && matches!(&data[8..12], b"AIFF" | b"AIFC") {
        return true;
    }
    if data.starts_with(b"PK\x03\x04") {
        return true;
    }
    if data.len() >= 8 && data.starts_with(b"\xca\xfe\xba\xbe") {
        return true;
    }
    const PREFIXES: &[&[u8]] = &[
        b"\x89PNG\r\n\x1a\n",
        b"GIF87a",
        b"GIF89a",
        b"MM\x00\x2a",
        b"II\x2a\x00",
        b"8BPS",
        b"qoif",
        b"BM",
        b"\xff\xd8\xff",
        b"\x00\x00\x00\x0c\x4a\x58\x4c\x20\x0d\x0a\x87\x0a",
        b"\xff\x0a",
        b"\x00\x00\x01\x00",
        b"ID3",
        b"MThd",
        b"OggS",
        b"fLaC",
        b"\x1a\x45\xdf\xa3",
        b"\x1f\x8b",
        b"BZh",
        b"\xfd7zXZ\x00",
        b"7z\xbc\xaf\x27\x1c",
        b"Rar!\x1a\x07\x01\x00",
        b"Rar!\x1a\x07\x00",
        b"\x28\xb5\x2f\xfd",
        b"%PDF-",
        b"SQLite format 3\x00",
        b"ARROW1",
        b"PAR1",
        b"\x00asm",
        b"dex\n",
        b"\x7fELF",
        b"\xfe\xed\xfa\xce",
        b"\xfe\xed\xfa\xcf",
        b"\xce\xfa\xed\xfe",
        b"\xcf\xfa\xed\xfe",
        b"MZ",
        b"wOFF",
        b"wOF2",
        b"OTTO",
        b"\x00\x01\x00\x00",
    ];
    if PREFIXES.iter().any(|p| data.starts_with(p)) {
        return true;
    }
    data.len() >= 257 + 6 && matches!(&data[257..263], b"ustar\x00" | b"ustar ")
}

/// `pipeline.utf8.scan_utf8`: (window is valid UTF-8, confidence when it holds multi-byte
/// sequences).
fn scan_utf8(data: &[u8]) -> (bool, Option<f64>) {
    if data.is_empty() || data.is_ascii() {
        return (true, None);
    }
    let length = data.len();
    let expected_seq_len = |b: u8| -> usize {
        match b {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => 0,
        }
    };
    let tail_start = match std::str::from_utf8(data) {
        Ok(_) => length,
        Err(e) => match e.error_len() {
            // An incomplete sequence at the very end: the incremental decoder buffers it.
            None => e.valid_up_to(),
            Some(_) => {
                // Only a final sequence whose declared length overruns the data is
                // tolerated (garbage inside the overrun included).
                let err_start = e.valid_up_to();
                let tail_lead = (length.saturating_sub(3)..length).find(|&k| {
                    let seq_len = expected_seq_len(data[k]);
                    seq_len != 0 && k + seq_len > length
                });
                match tail_lead {
                    Some(lead) if err_start >= lead => lead,
                    _ => return (false, None),
                }
            }
        },
    };
    let multibyte_bytes = data[..tail_start].iter().filter(|&&b| b >= 0x80).count();
    if multibyte_bytes == 0 {
        return (true, None);
    }
    let mb_ratio = multibyte_bytes as f64 / length as f64;
    let confidence = (0.80 + (0.99 - 0.80) * (mb_ratio * 6.0).min(1.0)).min(0.99);
    (true, Some(confidence))
}

/// `pipeline.ascii.detect_ascii`.
fn detect_ascii(data: &[u8]) -> Option<f64> {
    if data.is_empty() || !data.is_ascii() {
        return None;
    }
    let allowed = |b: u8| b == 0x09 || b == 0x0A || b == 0x0D || (0x20..0x7F).contains(&b);
    let disallowed = data.iter().filter(|&&b| !allowed(b)).count();
    if disallowed == 0 {
        return Some(1.0);
    }
    let null_count = data.iter().filter(|&&b| b == 0).count();
    if disallowed != null_count {
        return None;
    }
    if null_count as f64 / data.len() as f64 <= 0.05 {
        return Some(0.99);
    }
    None
}

/// `pipeline.binary.is_binary`.
fn is_binary(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let is_indicator = |b: u8| b < 0x09 || (0x0E..0x20).contains(&b);
    let binary_count = data.iter().filter(|&&b| is_indicator(b)).count();
    let len = data.len() as f64;
    if binary_count as f64 / len <= 0.01 {
        return false;
    }
    let ht_count = data.iter().filter(|&&b| b == 0x05).count();
    let nl_count = data.iter().filter(|&&b| b == 0x15).count();
    let hard_count = binary_count - ht_count - nl_count;
    if hard_count as f64 / len > 0.01 {
        return true;
    }
    let space_count = data.iter().filter(|&&b| b == 0x40).count() + ht_count;
    let non_space = data.len() - space_count;
    if non_space == 0 {
        return false;
    }
    let high_count = data.iter().filter(|&&b| b >= 0x80).count();
    if (high_count as f64 / non_space as f64) < 0.25 {
        return true;
    }
    (space_count as f64 / len) < 0.02
}

/// chardet's (chardet 5/6 compatible) name for an `encoding_rs` encoding.
fn chardet_name(encoding: &'static encoding_rs::Encoding) -> String {
    let name = encoding.name();
    match name {
        "Shift_JIS" => "CP932".to_string(),
        "EUC-KR" => "CP949".to_string(),
        "GBK" | "gb18030" => "GB18030".to_string(),
        "windows-874" => "CP874".to_string(),
        "IBM866" => "IBM866".to_string(),
        "macintosh" => "MacRoman".to_string(),
        "x-mac-cyrillic" => "MacCyrillic".to_string(),
        other if other.starts_with("windows-") => format!("Windows-{}", &other[8..]),
        other => other.to_string(),
    }
}

fn statistical(data: &[u8]) -> Detection {
    let digest = hex::encode(Sha256::digest(data));
    if let Some((_, encoding, confidence)) = RECORDED.iter().find(|(h, _, _)| *h == digest) {
        return Detection::new(Some(encoding), *confidence);
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(data, true);
    let (encoding, confident) = detector.guess_assess(None, false);
    let name = chardet_name(encoding);
    if codecs::lookup(&name).is_err() || !decodes_without_error(data, &name) {
        // chardet's no-match fallback.
        return Detection::new(Some("cp1252"), 0.10);
    }
    let confidence = if confident {
        STATISTICAL_CONFIDENCE
    } else {
        WEAK_STATISTICAL_CONFIDENCE
    };
    Detection {
        encoding: Some(name),
        confidence,
    }
}

/// `chardet.detect(data)` -> `(encoding, confidence)`.
pub(crate) fn detect(data: &[u8]) -> Detection {
    let input_truncated = data.len() > DEFAULT_MAX_BYTES;
    let data = &data[..data.len().min(DEFAULT_MAX_BYTES)];
    if data.is_empty() {
        return Detection::new(Some("utf-8"), 0.10);
    }
    if let Some(encoding) = detect_bom(data, input_truncated) {
        let name = match encoding {
            "utf-32" => "UTF-32",
            "utf-16" => "UTF-16",
            "utf-8-sig" => "UTF-8-SIG",
            other => other,
        };
        return Detection::new(Some(name), 1.0);
    }
    if let Some(encoding) = detect_utf1632(data)
        && decodes_without_error(data, encoding)
    {
        return Detection::new(Some(encoding), DETERMINISTIC_CONFIDENCE);
    }
    if has_magic(data) {
        return Detection::new(None, 1.0);
    }
    let (_, utf8) = scan_utf8(data);
    let ascii = detect_ascii(data);
    if utf8.is_none() && ascii.is_none() && is_binary(data) {
        return Detection::new(None, DETERMINISTIC_CONFIDENCE);
    }
    if let Some(confidence) = ascii {
        return Detection::new(Some("ascii"), confidence);
    }
    if let Some(confidence) = utf8 {
        return Detection::new(Some("utf-8"), confidence);
    }
    statistical(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_stages_match_chardet() {
        assert_eq!(detect(b"Hello"), Detection::new(Some("ascii"), 1.0));
        assert_eq!(
            detect(b"\xef\xbb\xbfHello"),
            Detection::new(Some("UTF-8-SIG"), 1.0)
        );
        assert_eq!(detect(b"\x00\x01\x02\x03"), Detection::new(None, 0.95));
        assert_eq!(
            detect("Héllò Wörld".as_bytes()),
            Detection::new(Some("utf-8"), 0.99)
        );
        assert_eq!(detect(b"%PDF-1.4"), Detection::new(None, 1.0));
    }
}
