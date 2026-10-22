//! Character encodings for simple fonts (pdfminer's `EncodingDB`, `name2unicode`) and
//! PDFDocEncoding (pdfminer's `PDFDocEncoding`, used by pdfplumber's `decode_text`).

use std::collections::HashMap;

use once_cell::sync::Lazy;

/// The Adobe Glyph List (glyph name -> Unicode string).
static GLYPHS: Lazy<HashMap<&'static str, String>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for line in include_str!("glyphlist.txt").lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let Some((name, codes)) = line.split_once(';') else {
            continue;
        };
        let text: String = codes
            .split_whitespace()
            .filter_map(|h| u32::from_str_radix(h, 16).ok())
            .filter_map(char::from_u32)
            .collect();
        map.insert(name, text);
    }
    map
});

fn invalid_unicode(cp: u32) -> bool {
    cp > 0xD7FF && cp < 0xE000
}

/// pdfminer's `name2unicode`: the Unicode text of a glyph name (`None` where pdfminer raises
/// `KeyError`). Handles `name.suffix`, ligature names `a_b`, `uniXXXX...` and `uXXXX[XX]`.
pub fn name2unicode(name: &str) -> Option<String> {
    let name = name.split('.').next().unwrap_or("");
    let components: Vec<&str> = name.split('_').collect();
    if components.len() > 1 {
        let mut out = String::new();
        for c in components {
            out.push_str(&name2unicode(c)?);
        }
        return Some(out);
    }
    if let Some(text) = GLYPHS.get(name) {
        return Some(text.clone());
    }
    let is_hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit());
    if name.starts_with("uni") {
        // `name.strip("uni")` strips any of the characters u, n, i from both ends.
        let stripped = name.trim_matches(|c| c == 'u' || c == 'n' || c == 'i');
        if is_hex(stripped) && stripped.len() % 4 == 0 {
            let mut out = String::new();
            for chunk in stripped.as_bytes().chunks(4) {
                let cp = u32::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
                if invalid_unicode(cp) {
                    return None;
                }
                out.push(char::from_u32(cp)?);
            }
            return Some(out);
        }
    } else if name.starts_with('u') {
        let stripped = name.trim_matches('u');
        if is_hex(stripped) && (4..=6).contains(&stripped.len()) {
            let cp = u32::from_str_radix(stripped, 16).ok()?;
            if invalid_unicode(cp) {
                return None;
            }
            return char::from_u32(cp).map(|c| c.to_string());
        }
    }
    None
}

/// The built-in encodings by name: code -> glyph name.
static ENCODINGS: Lazy<HashMap<&'static str, HashMap<u8, &'static str>>> = Lazy::new(|| {
    let mut out: HashMap<&'static str, HashMap<u8, &'static str>> = HashMap::new();
    for line in include_str!("encodings.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(';');
        let (Some(enc), Some(code), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let Ok(code) = u8::from_str_radix(code.trim(), 16) else {
            continue;
        };
        let key = match enc {
            "std" => "StandardEncoding",
            "winansi" => "WinAnsiEncoding",
            "macroman" => "MacRomanEncoding",
            "macexpert" => "MacExpertEncoding",
            _ => continue,
        };
        out.entry(key).or_default().insert(code, name.trim());
    }
    out
});

/// `EncodingDB.get_encoding(name, diff)`: code -> Unicode for a built-in encoding (unknown
/// names use StandardEncoding) with `/Differences` applied.
pub fn get_encoding(name: &str, diff: &[DiffItem]) -> HashMap<u32, String> {
    let base = ENCODINGS
        .get(name)
        .or_else(|| ENCODINGS.get("StandardEncoding"))
        .expect("standard encoding");
    let mut cid2unicode: HashMap<u32, String> = HashMap::new();
    for (code, glyph) in base {
        if let Some(text) = name2unicode(glyph) {
            cid2unicode.insert(*code as u32, text);
        }
    }
    let mut cid: i64 = 0;
    for item in diff {
        match item {
            DiffItem::Code(c) => cid = *c,
            DiffItem::Name(n) => {
                if let Some(text) = name2unicode(n)
                    && cid >= 0
                {
                    cid2unicode.insert(cid as u32, text);
                }
                cid += 1;
            }
        }
    }
    cid2unicode
}

/// One `/Differences` array entry.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffItem {
    /// A code (sets the next code to assign).
    Code(i64),
    /// A glyph name.
    Name(String),
}

/// pdfminer's `PDFDocEncoding` (undefined positions map to U+0000).
const PDFDOC_HIGH: [u16; 128] = [
    0x2022, 0x2020, 0x2021, 0x2026, 0x2014, 0x2013, 0x0192, 0x2044, 0x2039, 0x203A, 0x2212,
    0x2030, 0x201E, 0x201C, 0x201D, 0x2018, 0x2019, 0x201A, 0x2122, 0xFB01, 0xFB02, 0x0141,
    0x0152, 0x0160, 0x0178, 0x017D, 0x0131, 0x0142, 0x0153, 0x0161, 0x017E, 0x0000, 0x20AC,
    0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x00AA, 0x00AB,
    0x00AC, 0x0000, 0x00AE, 0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6,
    0x00B7, 0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF, 0x00C0, 0x00C1,
    0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7, 0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC,
    0x00CD, 0x00CE, 0x00CF, 0x00D0, 0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7,
    0x00D8, 0x00D9, 0x00DA, 0x00DB, 0x00DC, 0x00DD, 0x00DE, 0x00DF, 0x00E0, 0x00E1, 0x00E2,
    0x00E3, 0x00E4, 0x00E5, 0x00E6, 0x00E7, 0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED,
    0x00EE, 0x00EF, 0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7, 0x00F8,
    0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x00FD, 0x00FE, 0x00FF,
];

/// One PDFDocEncoding byte as a character.
pub fn pdfdoc_char(b: u8) -> char {
    match b {
        0x18 => '\u{02D8}',
        0x19 => '\u{02C7}',
        0x1A => '\u{02C6}',
        0x1B => '\u{02D9}',
        0x1C => '\u{02DD}',
        0x1D => '\u{02DB}',
        0x1E => '\u{02DA}',
        0x1F => '\u{02DC}',
        0x7F => '\u{0000}',
        0x80..=0xFF => char::from_u32(PDFDOC_HIGH[(b - 0x80) as usize] as u32).unwrap_or('\0'),
        other => other as char,
    }
}

/// pdfplumber's `decode_text`: UTF-16BE when the string starts with a BOM (undecodable
/// units ignored), PDFDocEncoding otherwise.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(body) = bytes.strip_prefix(b"\xfe\xff") {
        let units: Vec<u16> = body
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        // "ignore" error handling: lone surrogates are dropped.
        return char::decode_utf16(units.iter().copied())
            .filter_map(Result::ok)
            .collect();
    }
    bytes.iter().map(|&b| pdfdoc_char(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_names() {
        assert_eq!(name2unicode("A").as_deref(), Some("A"));
        assert_eq!(name2unicode("quoteright").as_deref(), Some("\u{2019}"));
        assert_eq!(name2unicode("f_i").as_deref(), Some("fi"));
        assert_eq!(name2unicode("uni00410042").as_deref(), Some("AB"));
        assert_eq!(name2unicode("u1F600").as_deref(), Some("\u{1F600}"));
        assert_eq!(name2unicode("a.sc").as_deref(), Some("a"));
        assert_eq!(name2unicode("g123"), None);
    }

    #[test]
    fn encodings_and_docencoding() {
        let std = get_encoding("StandardEncoding", &[]);
        assert_eq!(std.get(&0x27).map(String::as_str), Some("\u{2019}"));
        let win = get_encoding("WinAnsiEncoding", &[DiffItem::Code(65), DiffItem::Name("B".into())]);
        assert_eq!(win.get(&0x27).map(String::as_str), Some("'"));
        assert_eq!(win.get(&65).map(String::as_str), Some("B"));
        assert_eq!(decode_text(b"\xfe\xff\x00T\x00i"), "Ti");
        assert_eq!(decode_text(b"caf\xe9 \x93"), "café \u{201c}");
    }
}
