//! Python codecs: `bytes.decode(encoding, errors)` and `str.encode(encoding, errors)` with
//! Python's codec names and aliases, error handlers and error messages.
//!
//! UTF-8, UTF-8-SIG, ASCII, Latin-1, cp1252 and UTF-16/32 are implemented exactly. The other
//! legacy encodings use `encoding_rs` (WHATWG tables), which agree with Python's codecs on
//! almost every byte; see PORT-NOTES.md.

use encoding_rs::Encoding;
use sem_utils::SemanticaError;

/// A codec Python's `codecs.lookup` would find.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Codec {
    Utf8,
    Utf8Sig,
    Ascii,
    Latin1,
    Cp1252,
    Utf16,
    Utf16Le,
    Utf16Be,
    Utf32,
    Utf32Le,
    Utf32Be,
    /// A legacy encoding decoded with `encoding_rs`; the name is Python's codec name.
    Whatwg(&'static Encoding, &'static str),
}

impl Codec {
    /// The codec name Python's error messages use.
    fn message_name(&self) -> &'static str {
        match self {
            Codec::Utf8 => "utf-8",
            Codec::Utf8Sig => "utf-8",
            Codec::Ascii => "ascii",
            Codec::Latin1 => "latin-1",
            Codec::Cp1252 => "charmap",
            Codec::Utf16 => "utf-16",
            Codec::Utf16Le => "utf-16-le",
            Codec::Utf16Be => "utf-16-be",
            Codec::Utf32 => "utf-32",
            Codec::Utf32Le => "utf-32-le",
            Codec::Utf32Be => "utf-32-be",
            Codec::Whatwg(_, name) => name,
        }
    }
}

fn lookup_error(message: String) -> SemanticaError {
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "LookupError".to_string(),
        message,
    }
}

fn decode_error(message: String) -> SemanticaError {
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "UnicodeDecodeError".to_string(),
        message,
    }
}

fn encode_error(message: String) -> SemanticaError {
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "UnicodeEncodeError".to_string(),
        message,
    }
}

/// `_Py_normalize_encoding`: ASCII letters, digits and `.` are kept (lowercased); every run
/// of other characters becomes one `_` (none at the start).
fn normalize_name(name: &str) -> String {
    let mut out = String::new();
    let mut punct = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            if punct && !out.is_empty() {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            punct = false;
        } else {
            punct = true;
        }
    }
    out
}

fn whatwg(label: &str, python_name: &'static str) -> Codec {
    let encoding = Encoding::for_label(label.as_bytes())
        .unwrap_or_else(|| panic!("encoding_rs knows {label}"));
    Codec::Whatwg(encoding, python_name)
}

/// `codecs.lookup(name)`: the codec for a Python encoding name or alias, or `LookupError`.
pub(crate) fn lookup(name: &str) -> Result<Codec, SemanticaError> {
    let norm = normalize_name(name);
    let key = norm.replace('.', "_");
    let codec = match key.as_str() {
        "utf_8" | "u8" | "utf" | "utf8" | "utf8_ucs2" | "utf8_ucs4" | "cp65001" => Codec::Utf8,
        "utf_8_sig" => Codec::Utf8Sig,
        "ascii" | "646" | "ansi_x3_4_1968" | "ansi_x3_4_1986" | "cp367" | "csascii" | "ibm367"
        | "iso646_us" | "iso_646_irv_1991" | "iso_ir_6" | "us" | "us_ascii" => Codec::Ascii,
        "latin_1" | "8859" | "cp819" | "csisolatin1" | "ibm819" | "iso8859" | "iso8859_1"
        | "iso_8859_1" | "iso_8859_1_1987" | "iso_ir_100" | "l1" | "latin" | "latin1" => {
            Codec::Latin1
        }
        "cp1252" | "1252" | "windows_1252" => Codec::Cp1252,
        "utf_16" | "u16" | "utf16" => Codec::Utf16,
        "utf_16_le" | "utf_16le" | "unicodelittleunmarked" => Codec::Utf16Le,
        "utf_16_be" | "utf_16be" | "unicodebigunmarked" => Codec::Utf16Be,
        "utf_32" | "u32" | "utf32" => Codec::Utf32,
        "utf_32_le" | "utf_32le" => Codec::Utf32Le,
        "utf_32_be" | "utf_32be" => Codec::Utf32Be,
        "iso8859_2" | "iso_8859_2" | "csisolatin2" | "iso_8859_2_1987" | "iso_ir_101" | "l2"
        | "latin2" => whatwg("iso-8859-2", "iso8859-2"),
        "iso8859_3" | "iso_8859_3" | "csisolatin3" | "iso_8859_3_1988" | "iso_ir_109" | "l3"
        | "latin3" => whatwg("iso-8859-3", "iso8859-3"),
        "iso8859_4" | "iso_8859_4" | "csisolatin4" | "iso_8859_4_1988" | "iso_ir_110" | "l4"
        | "latin4" => whatwg("iso-8859-4", "iso8859-4"),
        "iso8859_5" | "iso_8859_5" | "csisolatincyrillic" | "cyrillic" | "iso_8859_5_1988"
        | "iso_ir_144" => whatwg("iso-8859-5", "iso8859-5"),
        "iso8859_6" | "iso_8859_6" | "arabic" | "asmo_708" | "csisolatinarabic" | "ecma_114"
        | "iso_8859_6_1987" | "iso_ir_127" => whatwg("iso-8859-6", "iso8859-6"),
        "iso8859_7" | "iso_8859_7" | "csisolatingreek" | "ecma_118" | "elot_928" | "greek"
        | "greek8" | "iso_8859_7_1987" | "iso_ir_126" => whatwg("iso-8859-7", "iso8859-7"),
        "iso8859_8" | "iso_8859_8" | "csisolatinhebrew" | "hebrew" | "iso_8859_8_1988"
        | "iso_ir_138" => whatwg("iso-8859-8", "iso8859-8"),
        "iso8859_10" | "iso_8859_10" | "csisolatin6" | "iso_8859_10_1992" | "iso_ir_157" | "l6"
        | "latin6" => whatwg("iso-8859-10", "iso8859-10"),
        "iso8859_13" | "iso_8859_13" | "l7" | "latin7" => whatwg("iso-8859-13", "iso8859-13"),
        "iso8859_14" | "iso_8859_14" | "iso_8859_14_1998" | "iso_celtic" | "iso_ir_199" | "l8"
        | "latin8" => whatwg("iso-8859-14", "iso8859-14"),
        "iso8859_15" | "iso_8859_15" | "l9" | "latin9" => whatwg("iso-8859-15", "iso8859-15"),
        "iso8859_16" | "iso_8859_16" | "iso_8859_16_2001" | "iso_ir_226" | "l10" | "latin10" => {
            whatwg("iso-8859-16", "iso8859-16")
        }
        "cp1250" | "1250" | "windows_1250" => whatwg("windows-1250", "charmap"),
        "cp1251" | "1251" | "windows_1251" => whatwg("windows-1251", "charmap"),
        "cp1253" | "1253" | "windows_1253" => whatwg("windows-1253", "charmap"),
        "cp1254" | "1254" | "windows_1254" => whatwg("windows-1254", "charmap"),
        "cp1255" | "1255" | "windows_1255" => whatwg("windows-1255", "charmap"),
        "cp1256" | "1256" | "windows_1256" => whatwg("windows-1256", "charmap"),
        "cp1257" | "1257" | "windows_1257" => whatwg("windows-1257", "charmap"),
        "cp1258" | "1258" | "windows_1258" => whatwg("windows-1258", "charmap"),
        "cp874" => whatwg("windows-874", "charmap"),
        "cp866" | "866" | "csibm866" | "ibm866" => whatwg("ibm866", "charmap"),
        "koi8_r" | "cskoi8r" => whatwg("koi8-r", "charmap"),
        "koi8_u" => whatwg("koi8-u", "charmap"),
        "mac_roman" | "macintosh" | "macroman" => whatwg("macintosh", "charmap"),
        "mac_cyrillic" | "maccyrillic" => whatwg("x-mac-cyrillic", "charmap"),
        "shift_jis" | "csshiftjis" | "shiftjis" | "sjis" | "s_jis" => {
            whatwg("shift_jis", "shift_jis")
        }
        "cp932" | "932" | "ms932" | "mskanji" | "ms_kanji" => whatwg("shift_jis", "cp932"),
        "euc_jp" | "eucjp" | "ujis" | "u_jis" => whatwg("euc-jp", "euc_jp"),
        "iso2022_jp" | "csiso2022jp" | "iso2022jp" | "iso_2022_jp" => {
            whatwg("iso-2022-jp", "iso2022_jp")
        }
        "euc_kr" | "euckr" | "korean" | "ksc5601" | "ks_c_5601" | "ks_c_5601_1987" | "ksx1001"
        | "ks_x_1001" => whatwg("euc-kr", "euc_kr"),
        "cp949" | "949" | "ms949" | "uhc" => whatwg("euc-kr", "cp949"),
        "gb2312" | "chinese" | "csiso58gb231280" | "euc_cn" | "euccn" | "eucgb2312_cn"
        | "gb2312_1980" | "gb2312_80" | "iso_ir_58" => whatwg("gbk", "gb2312"),
        "gbk" | "936" | "cp936" | "ms936" => whatwg("gbk", "gbk"),
        "gb18030" | "gb18030_2000" => whatwg("gb18030", "gb18030"),
        "big5" | "big5_tw" | "csbig5" => whatwg("big5", "big5"),
        "big5hkscs" | "big5_hkscs" | "hkscs" => whatwg("big5", "big5hkscs"),
        _ => return Err(lookup_error(format!("unknown encoding: {name}"))),
    };
    Ok(codec)
}

/// The error handlers of `bytes.decode` / `str.encode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handler {
    Strict,
    Replace,
    Ignore,
    BackslashReplace,
    XmlCharRefReplace,
}

fn handler(errors: &str) -> Result<Handler, SemanticaError> {
    match errors {
        "strict" => Ok(Handler::Strict),
        "replace" => Ok(Handler::Replace),
        "ignore" => Ok(Handler::Ignore),
        "backslashreplace" => Ok(Handler::BackslashReplace),
        "xmlcharrefreplace" => Ok(Handler::XmlCharRefReplace),
        // `surrogateescape`/`surrogatepass` would produce lone surrogates, which a Rust
        // string cannot hold; decoding falls back to U+FFFD like `replace`.
        "surrogateescape" | "surrogatepass" => Ok(Handler::Replace),
        other => Err(lookup_error(format!(
            "unknown error handler name '{other}'"
        ))),
    }
}

fn position_text(start: usize, end: usize) -> String {
    if end - start == 1 {
        format!("in position {start}")
    } else {
        format!("in position {start}-{}", end - 1)
    }
}

/// Builds decoded text, applying the error handler to each undecodable byte range.
struct DecodeSink<'a> {
    out: String,
    errors: &'a str,
    codec: Codec,
    data: &'a [u8],
}

impl DecodeSink<'_> {
    fn error(&mut self, start: usize, end: usize, reason: &str) -> Result<(), SemanticaError> {
        match handler(self.errors)? {
            Handler::Strict | Handler::XmlCharRefReplace => {
                let name = self.codec.message_name();
                let message = if end - start == 1 {
                    format!(
                        "'{name}' codec can't decode byte 0x{:02x} in position {start}: {reason}",
                        self.data[start]
                    )
                } else {
                    format!(
                        "'{name}' codec can't decode bytes {}: {reason}",
                        position_text(start, end)
                    )
                };
                Err(decode_error(message))
            }
            Handler::Replace => {
                self.out.push('\u{fffd}');
                Ok(())
            }
            Handler::Ignore => Ok(()),
            Handler::BackslashReplace => {
                for b in &self.data[start..end] {
                    self.out.push_str(&format!("\\x{b:02x}"));
                }
                Ok(())
            }
        }
    }
}

/// `data.decode(encoding, errors)`.
pub(crate) fn decode(data: &[u8], encoding: &str, errors: &str) -> Result<String, SemanticaError> {
    let codec = lookup(encoding)?;
    decode_with(data, codec, errors)
}

pub(crate) fn decode_with(
    data: &[u8],
    codec: Codec,
    errors: &str,
) -> Result<String, SemanticaError> {
    let mut sink = DecodeSink {
        out: String::with_capacity(data.len()),
        errors,
        codec,
        data,
    };
    match codec {
        Codec::Utf8 => decode_utf8(data, 0, &mut sink)?,
        Codec::Utf8Sig => {
            let start = if data.starts_with(b"\xef\xbb\xbf") {
                3
            } else {
                0
            };
            decode_utf8(data, start, &mut sink)?
        }
        Codec::Ascii => {
            for (i, &b) in data.iter().enumerate() {
                if b < 0x80 {
                    sink.out.push(b as char);
                } else {
                    sink.error(i, i + 1, "ordinal not in range(128)")?;
                }
            }
        }
        Codec::Latin1 => sink.out.extend(data.iter().map(|&b| b as char)),
        Codec::Cp1252 => {
            for (i, &b) in data.iter().enumerate() {
                match cp1252_decode(b) {
                    Some(c) => sink.out.push(c),
                    None => sink.error(i, i + 1, "character maps to <undefined>")?,
                }
            }
        }
        Codec::Utf16 | Codec::Utf16Le | Codec::Utf16Be => decode_utf16(data, codec, &mut sink)?,
        Codec::Utf32 | Codec::Utf32Le | Codec::Utf32Be => decode_utf32(data, codec, &mut sink)?,
        Codec::Whatwg(encoding, _) => decode_whatwg(data, encoding, &mut sink)?,
    }
    Ok(sink.out)
}

fn decode_utf8(
    data: &[u8],
    mut pos: usize,
    sink: &mut DecodeSink<'_>,
) -> Result<(), SemanticaError> {
    while pos < data.len() {
        match std::str::from_utf8(&data[pos..]) {
            Ok(s) => {
                sink.out.push_str(s);
                return Ok(());
            }
            Err(e) => {
                let valid = e.valid_up_to();
                sink.out
                    .push_str(std::str::from_utf8(&data[pos..pos + valid]).expect("valid prefix"));
                let start = pos + valid;
                match e.error_len() {
                    Some(len) => {
                        let lead = data[start];
                        let reason = if len == 1 && !(0xC2..=0xF4).contains(&lead) {
                            "invalid start byte"
                        } else {
                            "invalid continuation byte"
                        };
                        sink.error(start, start + len, reason)?;
                        pos = start + len;
                    }
                    None => {
                        sink.error(start, data.len(), "unexpected end of data")?;
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

fn decode_utf16(
    data: &[u8],
    codec: Codec,
    sink: &mut DecodeSink<'_>,
) -> Result<(), SemanticaError> {
    let mut pos = 0;
    let mut little = !matches!(codec, Codec::Utf16Be);
    if codec == Codec::Utf16 && data.len() >= 2 {
        if data.starts_with(b"\xff\xfe") {
            little = true;
            pos = 2;
        } else if data.starts_with(b"\xfe\xff") {
            little = false;
            pos = 2;
        }
    }
    if codec == Codec::Utf16 {
        sink.codec = if little {
            Codec::Utf16Le
        } else {
            Codec::Utf16Be
        };
    }
    let unit = |i: usize| -> u16 {
        if little {
            u16::from_le_bytes([data[i], data[i + 1]])
        } else {
            u16::from_be_bytes([data[i], data[i + 1]])
        }
    };
    while pos < data.len() {
        if pos + 1 >= data.len() {
            sink.error(pos, data.len(), "truncated data")?;
            break;
        }
        let u = unit(pos);
        if !(0xD800..0xE000).contains(&u) {
            sink.out
                .push(char::from_u32(u32::from(u)).expect("non-surrogate"));
            pos += 2;
            continue;
        }
        if u >= 0xDC00 {
            sink.error(pos, pos + 2, "illegal encoding")?;
            pos += 2;
            continue;
        }
        if pos + 3 >= data.len() {
            sink.error(pos, data.len(), "unexpected end of data")?;
            break;
        }
        let low = unit(pos + 2);
        if (0xDC00..0xE000).contains(&low) {
            let cp = 0x10000 + ((u32::from(u) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            sink.out.push(char::from_u32(cp).expect("valid pair"));
            pos += 4;
        } else {
            sink.error(pos, pos + 2, "illegal UTF-16 surrogate")?;
            pos += 2;
        }
    }
    Ok(())
}

fn decode_utf32(
    data: &[u8],
    codec: Codec,
    sink: &mut DecodeSink<'_>,
) -> Result<(), SemanticaError> {
    let mut pos = 0;
    let mut little = !matches!(codec, Codec::Utf32Be);
    if codec == Codec::Utf32 && data.len() >= 4 {
        if data.starts_with(b"\xff\xfe\x00\x00") {
            little = true;
            pos = 4;
        } else if data.starts_with(b"\x00\x00\xfe\xff") {
            little = false;
            pos = 4;
        }
    }
    if codec == Codec::Utf32 {
        sink.codec = if little {
            Codec::Utf32Le
        } else {
            Codec::Utf32Be
        };
    }
    while pos < data.len() {
        if pos + 4 > data.len() {
            sink.error(pos, data.len(), "truncated data")?;
            break;
        }
        let bytes = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
        let cp = if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        };
        match char::from_u32(cp) {
            Some(c) => sink.out.push(c),
            None if (0xD800..0xE000).contains(&cp) => sink.error(
                pos,
                pos + 4,
                "code point in surrogate code point range(0xd800, 0xe000)",
            )?,
            None => sink.error(pos, pos + 4, "code point not in range(0x110000)")?,
        }
        pos += 4;
    }
    Ok(())
}

fn decode_whatwg(
    data: &[u8],
    encoding: &'static Encoding,
    sink: &mut DecodeSink<'_>,
) -> Result<(), SemanticaError> {
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let mut buffer = String::with_capacity(
        decoder
            .max_utf8_buffer_length_without_replacement(data.len())
            .unwrap_or(data.len() * 3 + 16),
    );
    let mut pos = 0;
    loop {
        let (result, read) =
            decoder.decode_to_string_without_replacement(&data[pos..], &mut buffer, true);
        pos += read;
        match result {
            encoding_rs::DecoderResult::InputEmpty => {
                sink.out.push_str(&buffer);
                return Ok(());
            }
            encoding_rs::DecoderResult::OutputFull => {
                sink.out.push_str(&buffer);
                buffer.clear();
                buffer.reserve(data.len() - pos + 16);
            }
            encoding_rs::DecoderResult::Malformed(bad, extra) => {
                sink.out.push_str(&buffer);
                buffer.clear();
                let bad = usize::from(bad);
                let extra = usize::from(extra);
                let end = pos - extra;
                let start = end - bad;
                let reason =
                    if bad > 1 || matches!(sink.codec, Codec::Whatwg(_, n) if n != "charmap") {
                        "illegal multibyte sequence"
                    } else {
                        "character maps to <undefined>"
                    };
                sink.error(start, end, reason)?;
            }
        }
    }
}

fn cp1252_decode(b: u8) -> Option<char> {
    const HIGH: [u32; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
        0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    if (0x80..0xA0).contains(&b) {
        let cp = HIGH[usize::from(b - 0x80)];
        if cp == 0 { None } else { char::from_u32(cp) }
    } else {
        Some(b as char)
    }
}

fn cp1252_encode(c: char) -> Option<u8> {
    let cp = c as u32;
    if cp < 0x80 || (0xA0..0x100).contains(&cp) {
        return Some(cp as u8);
    }
    (0x80u8..0xA0).find(|&b| cp1252_decode(b) == Some(c))
}

/// Builds encoded bytes, applying the error handler to each unencodable character run.
struct EncodeSink<'a> {
    out: Vec<u8>,
    errors: &'a str,
    codec: Codec,
}

impl EncodeSink<'_> {
    fn error(
        &mut self,
        chars: &[char],
        start: usize,
        end: usize,
        reason: &str,
        encode_ascii: &dyn Fn(&str, &mut Vec<u8>),
    ) -> Result<(), SemanticaError> {
        match handler(self.errors)? {
            Handler::Strict => {
                let name = self.codec.message_name();
                let message = if end - start == 1 {
                    let c = chars[start] as u32;
                    let escaped = if c < 0x100 {
                        format!("\\x{c:02x}")
                    } else if c < 0x10000 {
                        format!("\\u{c:04x}")
                    } else {
                        format!("\\U{c:08x}")
                    };
                    format!(
                        "'{name}' codec can't encode character '{escaped}' in position {start}: {reason}"
                    )
                } else {
                    format!(
                        "'{name}' codec can't encode characters {}: {reason}",
                        position_text(start, end)
                    )
                };
                Err(encode_error(message))
            }
            Handler::Replace => {
                encode_ascii(&"?".repeat(end - start), &mut self.out);
                Ok(())
            }
            Handler::Ignore => Ok(()),
            Handler::BackslashReplace => {
                let mut text = String::new();
                for &c in &chars[start..end] {
                    let cp = c as u32;
                    if cp < 0x100 {
                        text.push_str(&format!("\\x{cp:02x}"));
                    } else if cp < 0x10000 {
                        text.push_str(&format!("\\u{cp:04x}"));
                    } else {
                        text.push_str(&format!("\\U{cp:08x}"));
                    }
                }
                encode_ascii(&text, &mut self.out);
                Ok(())
            }
            Handler::XmlCharRefReplace => {
                let mut text = String::new();
                for &c in &chars[start..end] {
                    text.push_str(&format!("&#{};", c as u32));
                }
                encode_ascii(&text, &mut self.out);
                Ok(())
            }
        }
    }
}

/// `text.encode(encoding, errors)`.
pub(crate) fn encode(text: &str, encoding: &str, errors: &str) -> Result<Vec<u8>, SemanticaError> {
    let codec = lookup(encoding)?;
    let mut sink = EncodeSink {
        out: Vec::with_capacity(text.len()),
        errors,
        codec,
    };
    let chars: Vec<char> = text.chars().collect();
    match codec {
        Codec::Utf8 => sink.out.extend_from_slice(text.as_bytes()),
        Codec::Utf8Sig => {
            sink.out.extend_from_slice(b"\xef\xbb\xbf");
            sink.out.extend_from_slice(text.as_bytes());
        }
        Codec::Ascii | Codec::Latin1 | Codec::Cp1252 => {
            let (limit, reason) = match codec {
                Codec::Ascii => (0x80, "ordinal not in range(128)"),
                Codec::Latin1 => (0x100, "ordinal not in range(256)"),
                _ => (0, "character maps to <undefined>"),
            };
            let encode_one = |c: char| -> Option<u8> {
                if codec == Codec::Cp1252 {
                    cp1252_encode(c)
                } else if (c as u32) < limit {
                    Some(c as u32 as u8)
                } else {
                    None
                }
            };
            let ascii = |s: &str, out: &mut Vec<u8>| out.extend_from_slice(s.as_bytes());
            let mut i = 0;
            while i < chars.len() {
                match encode_one(chars[i]) {
                    Some(b) => {
                        sink.out.push(b);
                        i += 1;
                    }
                    None => {
                        let mut end = i + 1;
                        while end < chars.len() && encode_one(chars[end]).is_none() {
                            end += 1;
                        }
                        sink.error(&chars, i, end, reason, &ascii)?;
                        i = end;
                    }
                }
            }
        }
        Codec::Utf16 | Codec::Utf16Le | Codec::Utf16Be => {
            let little = !matches!(codec, Codec::Utf16Be);
            if codec == Codec::Utf16 {
                sink.out.extend_from_slice(b"\xff\xfe");
            }
            let mut buf = [0u16; 2];
            for &c in &chars {
                for unit in c.encode_utf16(&mut buf) {
                    let bytes = if little {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    };
                    sink.out.extend_from_slice(&bytes);
                }
            }
        }
        Codec::Utf32 | Codec::Utf32Le | Codec::Utf32Be => {
            let little = !matches!(codec, Codec::Utf32Be);
            if codec == Codec::Utf32 {
                sink.out.extend_from_slice(b"\xff\xfe\x00\x00");
            }
            for &c in &chars {
                let cp = c as u32;
                let bytes = if little {
                    cp.to_le_bytes()
                } else {
                    cp.to_be_bytes()
                };
                sink.out.extend_from_slice(&bytes);
            }
        }
        Codec::Whatwg(encoding, _) => {
            let reason = if matches!(codec, Codec::Whatwg(_, "charmap")) {
                "character maps to <undefined>"
            } else {
                "illegal multibyte sequence"
            };
            let encode_text = |s: &str, out: &mut Vec<u8>| {
                let (bytes, _, _) = encoding.encode(s);
                out.extend_from_slice(&bytes);
            };
            let mut i = 0;
            while i < chars.len() {
                let mut tmp = [0u8; 4];
                let s = chars[i].encode_utf8(&mut tmp);
                let mut encoder = encoding.new_encoder();
                let mut out = Vec::with_capacity(16);
                let (result, _) =
                    encoder.encode_from_utf8_to_vec_without_replacement(s, &mut out, true);
                match result {
                    encoding_rs::EncoderResult::InputEmpty => {
                        sink.out.extend_from_slice(&out);
                        i += 1;
                    }
                    _ => {
                        sink.error(&chars, i, i + 1, reason, &encode_text)?;
                        i += 1;
                    }
                }
            }
        }
    }
    Ok(sink.out)
}

/// Python's universal-newlines translation on reading a text file (`\r\n` and `\r` -> `\n`).
pub(crate) fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_errors_follow_python() {
        assert_eq!(lookup("UTF-8").unwrap(), Codec::Utf8);
        assert_eq!(lookup("latin-1").unwrap(), Codec::Latin1);
        assert_eq!(lookup("ISO-8859-1").unwrap(), Codec::Latin1);
        assert_eq!(lookup("Windows-1252").unwrap(), Codec::Cp1252);
        assert!(lookup("bogus").is_err());
        assert_eq!(
            decode(b"\xff", "utf-8", "strict").unwrap_err().to_string(),
            "'utf-8' codec can't decode byte 0xff in position 0: invalid start byte"
        );
        assert_eq!(
            decode(b"a\xe2\x82", "utf-8", "strict")
                .unwrap_err()
                .to_string(),
            "'utf-8' codec can't decode bytes in position 1-2: unexpected end of data"
        );
        assert_eq!(decode(b"a\xffb", "utf-8", "replace").unwrap(), "a\u{fffd}b");
        assert_eq!(decode(b"caf\xe9", "latin-1", "strict").unwrap(), "café");
        assert!(decode(b"\x81", "cp1252", "strict").is_err());
        assert_eq!(
            encode("H\u{e9}llo", "ascii", "strict")
                .unwrap_err()
                .to_string(),
            "'ascii' codec can't encode character '\\xe9' in position 1: ordinal not in range(128)"
        );
        assert_eq!(encode("é", "latin-1", "strict").unwrap(), b"\xe9");
    }
}
