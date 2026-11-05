//! Group-code pairs of an ASCII DXF in its own code page, read here without the classifier's reader.
use anyhow::{Context, Result};

pub struct Record {
    pub kind: String,
    pub tags: Vec<(i64, String)>,
}

impl Record {
    /// First value of a group code.
    pub fn tag(&self, code: i64) -> Option<&str> {
        self.tags
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.as_str())
    }
    pub fn tag_or<'a>(&'a self, code: i64, default: &'a str) -> &'a str {
        self.tag(code).unwrap_or(default)
    }
    pub fn float(&self, code: i64) -> Result<f64> {
        let value = self
            .tag(code)
            .with_context(|| format!("dxf_group_missing:{code}"))?;
        value
            .parse::<f64>()
            .with_context(|| format!("dxf_group_not_a_number:{code}"))
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Text of a DXF before 2007 is in its code page (DXF Reference); 2007 and later are UTF-8. Chinese code
/// pages decode as GB18030 (WHATWG), which maps the user-defined area to private-use characters.
pub fn decode(bytes: &[u8]) -> Result<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Ok(text.to_owned());
    }
    let page = match find(bytes, b"$DWGCODEPAGE") {
        Some(i) => {
            let window = &bytes[i..(i + 80).min(bytes.len())];
            let value = window.split(|b| *b == b'\n').nth(2).unwrap_or_default();
            std::str::from_utf8(value)
                .context("dxf_code_page_name")?
                .trim()
                .to_uppercase()
        }
        None => String::new(),
    };
    let label = match page.as_str() {
        "ANSI_936" | "GB2312" | "GBK" => "gb18030".to_owned(),
        "ANSI_950" => "big5".to_owned(),
        "ANSI_932" => "shift_jis".to_owned(),
        "ANSI_949" => "euc-kr".to_owned(),
        other => format!("cp{}", other.strip_prefix("ANSI_").unwrap_or(other)),
    };
    let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
        .with_context(|| format!("dxf_code_page_unknown:{page}"))?;
    Ok(encoding
        .decode_without_bom_handling_and_without_replacement(bytes)
        .with_context(|| format!("dxf_code_page_decode:{page}"))?
        .into_owned())
}

/// Records in file order; a record starts at every group 0. Lines end with LF or CRLF; codes and values are
/// trimmed. Groups before the first record (comments) belong to no record.
pub fn records(text: &str) -> Result<Vec<Record>> {
    let lines: Vec<&str> = text.lines().collect();
    let mut records: Vec<Record> = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        let code: i64 = lines[i]
            .trim()
            .parse()
            .with_context(|| format!("dxf_group_code:line {}", i + 1))?;
        let value = lines[i + 1].trim().to_owned();
        if code == 0 {
            records.push(Record {
                kind: value.clone(),
                tags: Vec::new(),
            });
        }
        if let Some(r) = records.last_mut() {
            r.tags.push((code, value));
        }
        i += 2;
    }
    Ok(records)
}
