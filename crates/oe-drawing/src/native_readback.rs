//! Finite-profile ASCII DXF records with original pointers, and text decoding in the drawing code page.
//! Not a general DWG/DXF importer.
use anyhow::{Context, Result, ensure};
use std::collections::BTreeSet;

#[derive(Debug)]
pub(crate) struct Record {
    pub index: usize,
    pub kind: String,
    pub section: String,
    pub block: String,
    pub tags: Vec<(i32, String)>,
}
impl Record {
    pub fn get(&self, code: i32) -> Option<&str> {
        self.tags
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, s)| s.as_str())
    }
    pub fn one(&self, code: i32) -> Result<&str> {
        self.get(code)
            .with_context(|| format!("native_dxf_missing_tag:{}:{code}", self.kind))
    }
    pub fn number(&self, code: i32) -> Result<f64> {
        let value: f64 = self.one(code)?.parse()?;
        ensure!(value.is_finite(), "native_dxf_nonfinite");
        Ok(value)
    }
    pub fn xy(&self, code: i32) -> Result<[f64; 2]> {
        Ok([self.number(code)?, self.number(code + 10)?])
    }
    pub fn handle(&self) -> Result<&str> {
        self.one(5)
    }
    pub fn all(&self, code: i32) -> Vec<&str> {
        self.tags
            .iter()
            .filter(|(c, _)| *c == code)
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

/// The text of a DXF file: UTF-8 when the bytes are UTF-8 (AutoCAD 2007 and later always are);
/// otherwise the code page its header names in `$DWGCODEPAGE`, in which the DXF Reference says
/// earlier versions write their text. A code page this reader does not know, or bytes that are not
/// valid in it, are an error, never a guess.
pub(crate) fn dxf_text(bytes: &[u8]) -> Result<std::borrow::Cow<'_, str>> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(std::borrow::Cow::Borrowed(s));
    }
    let page = code_page_of(bytes).context("native_dxf_not_utf8_without_code_page")?;
    let enc = code_page_encoding(&page)
        .with_context(|| format!("native_dxf_code_page_unsupported:{page}"))?;
    enc.decode_without_bom_handling_and_without_replacement(bytes)
        .with_context(|| format!("native_dxf_code_page_invalid_bytes:{page}"))
}

/// `$DWGCODEPAGE` from the (ASCII) HEADER section: the value line after the name and its code.
fn code_page_of(bytes: &[u8]) -> Option<String> {
    let mut lines = bytes
        .split(|b| *b == b'\n')
        .map(|l| String::from_utf8_lossy(l).trim().to_string());
    lines.find(|l| l == "$DWGCODEPAGE")?;
    let code = lines.next()?;
    (code == "3")
        .then(|| lines.next())
        .flatten()
        .filter(|v| !v.is_empty() && v.is_ascii())
}

/// The encoding a DXF code page name stands for (Windows `ANSI_*` names, and the plain names such
/// as `GB2312` some writers use).
fn code_page_encoding(page: &str) -> Option<&'static encoding_rs::Encoding> {
    let p = page.trim().to_ascii_uppercase();
    let label = match p.as_str() {
        "ANSI_936" | "GB2312" | "GBK" | "CP936" | "DOS936" => "gbk",
        "ANSI_950" | "BIG5" | "CP950" | "DOS950" => "big5",
        "ANSI_932" | "SHIFT_JIS" | "SJIS" | "CP932" | "DOS932" => "shift_jis",
        "ANSI_949" | "KSC5601" | "CP949" | "DOS949" => "euc-kr",
        _ => match p.strip_prefix("ANSI_") {
            Some(
                n @ ("874" | "1250" | "1251" | "1252" | "1253" | "1254" | "1255" | "1256" | "1257"
                | "1258"),
            ) => {
                return encoding_rs::Encoding::for_label(format!("windows-{n}").as_bytes());
            }
            _ => return None,
        },
    };
    encoding_rs::Encoding::for_label(label.as_bytes())
}

pub(crate) struct Drawing {
    pub records: Vec<Record>,
}
impl Drawing {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 16 * 1024 * 1024, "native_dxf_size_limit");
        let s = dxf_text(bytes)?;
        let mut lines = s.lines();
        let mut records: Vec<Record> = Vec::new();
        while let Some(raw) = lines.next() {
            let code: i32 = raw.trim().parse().context("native_dxf_tag_code")?;
            let value = lines
                .next()
                .context("native_dxf_truncated_pair")?
                .trim()
                .to_owned();
            if code == 0 {
                records.push(Record {
                    index: records.len(),
                    kind: value.clone(),
                    section: String::new(),
                    block: String::new(),
                    tags: Vec::new(),
                });
            }
            records
                .last_mut()
                .context("native_dxf_record_missing")?
                .tags
                .push((code, value));
            ensure!(records.len() <= 10000, "native_dxf_record_limit");
        }
        ensure!(
            records.last().is_some_and(|r| r.kind == "EOF"),
            "native_dxf_eof_missing"
        );
        let mut section = String::new();
        let mut block = String::new();
        let mut handles = BTreeSet::new();
        for r in &mut records {
            if r.kind == "SECTION" {
                ensure!(section.is_empty(), "native_dxf_nested_section");
                section = r.one(2)?.into();
            }
            r.section = section.clone();
            if r.kind == "BLOCK" {
                ensure!(block.is_empty(), "native_dxf_nested_block");
                block = r.one(2)?.into();
            }
            r.block = block.clone();
            if r.kind == "ENDBLK" {
                ensure!(!block.is_empty(), "native_dxf_unmatched_block");
                block.clear();
            }
            if r.kind == "ENDSEC" {
                ensure!(block.is_empty(), "native_dxf_unclosed_block");
                section.clear();
            }
            if let Some(h) = r.get(5) {
                ensure!(handles.insert(h.to_owned()), "native_dxf_duplicate_handle");
            }
        }
        ensure!(
            section.is_empty() && block.is_empty(),
            "native_dxf_unclosed_section"
        );
        Ok(Self { records })
    }
    pub fn entity(&self, handle: &str) -> Result<&Record> {
        let rows: Vec<_> = self
            .records
            .iter()
            .filter(|r| r.section == "ENTITIES" && r.get(5) == Some(handle))
            .collect();
        ensure!(
            rows.len() == 1,
            "native_dxf_entity_missing_or_ambiguous:{handle}"
        );
        Ok(rows[0])
    }
    pub fn header(&self, key: &str, code: i32) -> Result<&str> {
        let r = self
            .records
            .iter()
            .find(|r| r.kind == "SECTION" && r.get(2) == Some("HEADER"))
            .context("native_dxf_header")?;
        let i = r
            .tags
            .iter()
            .position(|(c, v)| *c == 9 && v == key)
            .context("native_dxf_header_key")?;
        let tag = r.tags.get(i + 1).context("native_dxf_header_value")?;
        ensure!(tag.0 == code, "native_dxf_header_type");
        Ok(&tag.1)
    }
}
