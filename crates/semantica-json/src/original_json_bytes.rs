//! Byte decoding used by the historical JSON importer. This is NOT strict JSON
//! admission, JSON syntax parsing, or graph mutation. Codepoints retain lone
//! surrogates instead of forcing a lossy Rust String conversion.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Sig,
    Utf16,
    Utf16Le,
    Utf16Be,
    Utf32,
    Utf32Le,
    Utf32Be,
}
impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf8Sig => "utf-8-sig",
            Self::Utf16 => "utf-16",
            Self::Utf16Le => "utf-16-le",
            Self::Utf16Be => "utf-16-be",
            Self::Utf32 => "utf-32",
            Self::Utf32Le => "utf-32-le",
            Self::Utf32Be => "utf-32-be",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeFailure {
    pub encoding: &'static str,
    pub start: usize,
    pub end: usize,
    pub reason: &'static str,
    // The original error contains one byte only when the failed interval is one byte.
    byte: u8,
}
impl fmt::Display for DecodeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.end == self.start + 1 {
            write!(
                f,
                "'{}' codec can't decode byte 0x{:02x} in position {}: {}",
                self.encoding, self.byte, self.start, self.reason
            )
        } else {
            write!(
                f,
                "'{}' codec can't decode bytes in position {}-{}: {}",
                self.encoding,
                self.start,
                self.end - 1,
                self.reason
            )
        }
    }
}
impl std::error::Error for DecodeFailure {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    InputLimit,
    CodepointLimit,
    InvalidEncoding(DecodeFailure),
}
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit => f.write_str("original_json_input_limit"),
            Self::CodepointLimit => f.write_str("original_json_codepoint_limit"),
            Self::InvalidEncoding(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for DecodeError {}
/// Caller-owned resource bounds. These do not establish a new service budget.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub max_input_bytes: usize,
    pub max_codepoints: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub detected_encoding: Encoding,
    pub codepoints: Vec<u32>,
}

pub fn detect_encoding(b: &[u8]) -> Encoding {
    use Encoding::*;
    if b.starts_with(&[0, 0, 0xfe, 0xff]) || b.starts_with(&[0xff, 0xfe, 0, 0]) {
        return Utf32;
    }
    if b.starts_with(&[0xfe, 0xff]) || b.starts_with(&[0xff, 0xfe]) {
        return Utf16;
    }
    if b.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Utf8Sig;
    }
    if b.len() >= 4 {
        if b[0] == 0 {
            return if b[1] != 0 { Utf16Be } else { Utf32Be };
        }
        if b[1] == 0 {
            return if b[2] != 0 || b[3] != 0 {
                Utf16Le
            } else {
                Utf32Le
            };
        }
    } else if b.len() == 2 {
        if b[0] == 0 {
            return Utf16Be;
        }
        if b[1] == 0 {
            return Utf16Le;
        }
    }
    Utf8
}

pub fn decode_original_bytes(input: &[u8], limits: DecodeLimits) -> Result<Decoded, DecodeError> {
    use Encoding::*;
    if input.len() > limits.max_input_bytes {
        return Err(DecodeError::InputLimit);
    }
    let detected_encoding = detect_encoding(input);
    // UTF-8-sig errors index the stripped buffer, UTF-16/32 BOM errors index
    // the original buffer. Preserve this observed difference.
    let (b, encoding, mut i) = match detected_encoding {
        Utf8Sig => (&input[3..], Utf8, 0),
        Utf16 => (input, if input[0] == 0xff { Utf16Le } else { Utf16Be }, 2),
        Utf32 => (input, if input[0] == 0xff { Utf32Le } else { Utf32Be }, 4),
        e => (input, e, 0),
    };
    let invalid = |start: usize, end: usize, reason| {
        DecodeError::InvalidEncoding(DecodeFailure {
            encoding: encoding.name(),
            start,
            end,
            reason,
            byte: b[start],
        })
    };
    let mut codepoints = Vec::new();
    while i < b.len() {
        if codepoints.len() == limits.max_codepoints {
            return Err(DecodeError::CodepointLimit);
        }
        let cp;
        match encoding {
            Utf8 => {
                let first = b[i];
                let n = match first {
                    0..=0x7f => 1,
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Err(invalid(i, i + 1, "invalid start byte")),
                };
                // surrogatepass overrides the normal surrogate rejection only
                // for a COMPLETE valid three-byte surrogate sequence.
                if first == 0xed && b.get(i + 1).is_some_and(|x| (0xa0..=0xbf).contains(x)) {
                    if b.get(i + 2).is_some_and(|x| (0x80..=0xbf).contains(x)) {
                        cp = (((first & 15) as u32) << 12)
                            | (((b[i + 1] & 63) as u32) << 6)
                            | (b[i + 2] & 63) as u32;
                        i += 3;
                        codepoints.push(cp);
                        continue;
                    }
                    return Err(invalid(i, i + 1, "invalid continuation byte"));
                }
                let mut value = (first
                    & match n {
                        1 => 0x7f,
                        2 => 0x1f,
                        3 => 0x0f,
                        _ => 0x07,
                    }) as u32;
                for j in 1..n {
                    let Some(&next) = b.get(i + j) else {
                        return Err(invalid(i, b.len(), "unexpected end of data"));
                    };
                    if !(0x80..=0xbf).contains(&next)
                        || (j == 1
                            && ((first == 0xe0 && next < 0xa0)
                                || (first == 0xf0 && next < 0x90)
                                || (first == 0xf4 && next > 0x8f)))
                    {
                        return Err(invalid(i, i + j, "invalid continuation byte"));
                    }
                    value = (value << 6) | (next & 63) as u32;
                }
                cp = value;
                i += n;
            }
            Utf16Le | Utf16Be => {
                if b.len() - i < 2 {
                    return Err(invalid(i, b.len(), "truncated data"));
                }
                let unit = |at| {
                    let a = [b[at], b[at + 1]];
                    if encoding == Utf16Le {
                        u16::from_le_bytes(a)
                    } else {
                        u16::from_be_bytes(a)
                    }
                };
                let first = unit(i);
                i += 2;
                if (0xd800..=0xdbff).contains(&first)
                    && b.len() - i >= 2
                    && (0xdc00..=0xdfff).contains(&unit(i))
                {
                    cp = 0x10000 + (((first as u32) - 0xd800) << 10) + unit(i) as u32 - 0xdc00;
                    i += 2;
                } else {
                    cp = first as u32;
                }
            }
            Utf32Le | Utf32Be => {
                if b.len() - i < 4 {
                    return Err(invalid(i, b.len(), "truncated data"));
                }
                let a = [b[i], b[i + 1], b[i + 2], b[i + 3]];
                cp = if encoding == Utf32Le {
                    u32::from_le_bytes(a)
                } else {
                    u32::from_be_bytes(a)
                };
                if cp > 0x10ffff {
                    return Err(invalid(i, i + 4, "code point not in range(0x110000)"));
                }
                i += 4;
            }
            _ => unreachable!("BOM encodings resolved before decoding"),
        }
        codepoints.push(cp);
    }
    Ok(Decoded {
        detected_encoding,
        codepoints,
    })
}
