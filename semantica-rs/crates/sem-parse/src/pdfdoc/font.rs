//! Fonts as pdfminer models them for text extraction: code -> CID decoding, CID -> Unicode
//! (ToUnicode CMaps, built-in encodings with `/Differences`, Identity maps), glyph widths
//! (`/Widths`, `/W`, or the core-14 AFM metrics) and the descent used for character boxes.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object};
use once_cell::sync::Lazy;

use super::encoding::{DiffItem, get_encoding, name2unicode};
use super::lexer::{ContentParser, Obj};
use super::num::f32_to_f64;

/// Core-14 font metrics: descriptor values and widths keyed by Unicode text.
struct CoreMetrics {
    ascent: f64,
    descent: f64,
    flags: i64,
    widths: HashMap<String, f64>,
}

static CORE14: Lazy<HashMap<String, CoreMetrics>> = Lazy::new(|| {
    let mut out = HashMap::new();
    let mut current: Option<(String, CoreMetrics)> = None;
    for line in include_str!("core14.txt").lines() {
        if let Some(rest) = line.strip_prefix("font ") {
            let mut props: HashMap<&str, &str> = HashMap::new();
            for kv in rest.split_whitespace() {
                if let Some((k, v)) = kv.split_once('=') {
                    props.insert(k, v);
                }
            }
            let num = |k: &str| props.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            let name = props.get("FontName").copied().unwrap_or("").to_string();
            current = Some((
                name,
                CoreMetrics {
                    ascent: num("Ascent"),
                    descent: num("Descent"),
                    flags: num("Flags") as i64,
                    widths: HashMap::new(),
                },
            ));
        } else if let Some(rest) = line.strip_prefix("widths ") {
            if let Some((name, mut metrics)) = current.take() {
                for kv in rest.split_whitespace() {
                    if let Some((glyph, w)) = kv.split_once('=')
                        && let (Some(text), Ok(w)) = (name2unicode(glyph), w.parse::<f64>())
                    {
                        metrics.widths.entry(text).or_insert(w);
                    }
                }
                out.insert(name, metrics);
            }
        }
    }
    out
});

/// Follow references.
pub fn deref<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
    let mut cur = obj;
    for _ in 0..32 {
        match cur {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(o) => cur = o,
                Err(_) => return &Object::Null,
            },
            _ => return cur,
        }
    }
    &Object::Null
}

/// `d.get(key)` with references followed.
pub fn dget<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    d.get(key).ok().map(|o| deref(doc, o))
}

/// A numeric object as `f64` (lopdf keeps reals as `f32`; see [`f32_to_f64`]).
pub fn num_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(f32_to_f64(*r)),
        _ => None,
    }
}

/// The dictionary of a dictionary or stream object.
pub fn as_dict(obj: &Object) -> Option<&Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

/// Decoded stream data (`stream.get_data()`); undecodable filters give the raw bytes.
pub fn stream_data(obj: &Object) -> Option<Vec<u8>> {
    match obj {
        Object::Stream(s) => Some(match s.dict.get(b"Filter") {
            Ok(_) => s.decompressed_content().unwrap_or_else(|_| s.content.clone()),
            Err(_) => s.content.clone(),
        }),
        _ => None,
    }
}

/// `nunpack`: big-endian unsigned integer of up to 4 bytes.
fn nunpack(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |acc, b| acc.wrapping_shl(8) | *b as u32)
}

fn utf16be_ignore(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    char::decode_utf16(units.iter().copied())
        .filter_map(Result::ok)
        .collect()
}

/// pdfminer's `FileUnicodeMap.add_cid2unichr`.
fn add_cid2unichr(map: &mut HashMap<u32, String>, cid: u32, text: String) {
    if text == "\u{a0}" && map.get(&cid).map(String::as_str) == Some(" ") {
        return;
    }
    map.insert(cid, text);
}

/// Parse a ToUnicode CMap (pdfminer's `CMapParser` into a `FileUnicodeMap`).
pub fn parse_to_unicode(data: &[u8]) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let mut parser = ContentParser::new(data);
    let mut stack: Vec<Obj> = Vec::new();
    while let Some(obj) = parser.next_object() {
        let Obj::Kw(kw) = &obj else {
            stack.push(obj);
            continue;
        };
        match kw.as_slice() {
            b"beginbfchar" | b"beginbfrange" | b"begincidrange" | b"begincidchar"
            | b"begincodespacerange" | b"beginnotdefrange" => stack.clear(),
            b"endbfchar" => {
                let objs = std::mem::take(&mut stack);
                for pair in objs.chunks(2) {
                    if let [Obj::Str(cid), code] = pair {
                        let cid = nunpack(cid);
                        match code {
                            Obj::Str(b) => add_cid2unichr(&mut map, cid, utf16be_ignore(b)),
                            Obj::Name(n) => {
                                if let Some(t) = name2unicode(&String::from_utf8_lossy(n)) {
                                    add_cid2unichr(&mut map, cid, t);
                                }
                            }
                            Obj::Num(n) => {
                                if let Some(c) = char::from_u32(n.f() as u32) {
                                    add_cid2unichr(&mut map, cid, c.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            b"endbfrange" => {
                let objs = std::mem::take(&mut stack);
                for triple in objs.chunks(3) {
                    let [Obj::Str(sb), Obj::Str(eb), code] = triple else {
                        continue;
                    };
                    if sb.len() != eb.len() {
                        continue;
                    }
                    let start = nunpack(sb);
                    let end = nunpack(eb);
                    if end < start {
                        continue;
                    }
                    match code {
                        Obj::Array(items) => {
                            for (cid, item) in (start..=end).zip(items.iter()) {
                                match item {
                                    Obj::Str(b) => add_cid2unichr(&mut map, cid, utf16be_ignore(b)),
                                    Obj::Name(n) => {
                                        if let Some(t) = name2unicode(&String::from_utf8_lossy(n)) {
                                            add_cid2unichr(&mut map, cid, t);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Obj::Str(code) => {
                            let split = code.len().saturating_sub(4);
                            let (prefix, var) = code.split_at(split);
                            let base = nunpack(var);
                            let vlen = var.len();
                            for i in 0..=(end - start) {
                                let value = base.wrapping_add(i).to_be_bytes();
                                let mut bytes = prefix.to_vec();
                                bytes.extend_from_slice(&value[4 - vlen..]);
                                add_cid2unichr(&mut map, start + i, utf16be_ignore(&bytes));
                            }
                        }
                        _ => {}
                    }
                }
            }
            b"endcidrange" | b"endcidchar" | b"endcodespacerange" | b"endnotdefrange" => {
                stack.clear();
            }
            _ => stack.push(obj),
        }
    }
    map
}

/// How codes become CIDs.
#[derive(Debug, Clone)]
enum CMapKind {
    /// Simple fonts: each byte is a CID.
    Bytes,
    /// `Identity-H`/`Identity-V`: 2-byte big-endian codes.
    Identity,
    /// `OneByteIdentityH`/`V`.
    OneByteIdentity,
    /// A CMap pdfminer cannot load (no predefined CMap data): decodes to nothing.
    Empty,
}

/// CID -> Unicode.
#[derive(Debug, Clone)]
enum UnicodeMap {
    File(HashMap<u32, String>),
    Identity,
}

/// A font as pdfminer's `PDFFont` subclasses model it.
#[derive(Debug, Clone)]
pub struct Font {
    /// `font.fontname`.
    pub fontname: String,
    cmap: CMapKind,
    /// Built-in/Differences encoding for simple fonts.
    cid2unicode: HashMap<u32, String>,
    unicode_map: Option<UnicodeMap>,
    int_widths: HashMap<u32, f64>,
    str_widths: HashMap<String, f64>,
    default_width: f64,
    descent: f64,
    hscale: f64,
    vscale: f64,
    multibyte: bool,
    cidcoding: String,
}

fn literal_name(obj: &Object) -> Option<String> {
    match obj {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        Object::String(s, _) => Some(String::from_utf8_lossy(s).into_owned()),
        _ => None,
    }
}

impl Font {
    /// `rsrcmgr.get_font(None, {})`: the fallback for an undefined font resource.
    pub fn unknown() -> Font {
        let mut int_widths = HashMap::new();
        for i in 0..256u32 {
            int_widths.insert(i, 0.0);
        }
        Font {
            fontname: "unknown".to_string(),
            cmap: CMapKind::Bytes,
            cid2unicode: get_encoding("StandardEncoding", &[]),
            unicode_map: None,
            int_widths,
            str_widths: HashMap::new(),
            default_width: 0.0,
            descent: 0.0,
            hscale: 0.001,
            vscale: 0.001,
            multibyte: false,
            cidcoding: String::new(),
        }
    }

    /// Build a font from its dictionary (`PDFResourceManager.get_font`).
    pub fn from_spec(doc: &Document, spec: &Dictionary) -> Font {
        let subtype = dget(doc, spec, b"Subtype")
            .and_then(literal_name)
            .unwrap_or_else(|| "Type1".to_string());
        match subtype.as_str() {
            "Type0" => {
                let dfonts = dget(doc, spec, b"DescendantFonts").and_then(|o| o.as_array().ok());
                let mut sub = dfonts
                    .and_then(|a| a.first())
                    .map(|o| deref(doc, o))
                    .and_then(as_dict)
                    .cloned()
                    .unwrap_or_default();
                for key in [b"Encoding".as_slice(), b"ToUnicode".as_slice()] {
                    if let Ok(v) = spec.get(key) {
                        sub.set(key.to_vec(), deref(doc, v).clone());
                    }
                }
                let sub_subtype = dget(doc, &sub, b"Subtype").and_then(literal_name);
                match sub_subtype.as_deref() {
                    Some("CIDFontType0") | Some("CIDFontType2") => Self::cid_font(doc, &sub),
                    _ => Self::from_spec(doc, &sub),
                }
            }
            "CIDFontType0" | "CIDFontType2" => Self::cid_font(doc, spec),
            "Type3" => Self::type3_font(doc, spec),
            _ => Self::type1_font(doc, spec),
        }
    }

    fn descriptor(doc: &Document, spec: &Dictionary) -> Dictionary {
        dget(doc, spec, b"FontDescriptor")
            .and_then(as_dict)
            .cloned()
            .unwrap_or_default()
    }

    fn desc_num(doc: &Document, d: &Dictionary, key: &[u8]) -> f64 {
        dget(doc, d, key).and_then(num_f64).unwrap_or(0.0)
    }

    /// `PDFSimpleFont.__init__`: encoding, Differences and ToUnicode.
    fn simple_encoding(doc: &Document, spec: &Dictionary) -> (HashMap<u32, String>, Option<UnicodeMap>) {
        let cid2unicode = match dget(doc, spec, b"Encoding") {
            Some(Object::Dictionary(enc)) => {
                let base = dget(doc, enc, b"BaseEncoding")
                    .and_then(literal_name)
                    .unwrap_or_else(|| "StandardEncoding".to_string());
                let diff: Vec<DiffItem> = dget(doc, enc, b"Differences")
                    .and_then(|o| o.as_array().ok())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|o| match deref(doc, o) {
                                Object::Integer(i) => Some(DiffItem::Code(*i)),
                                Object::Real(r) => Some(DiffItem::Code(*r as i64)),
                                Object::Name(n) => {
                                    Some(DiffItem::Name(String::from_utf8_lossy(n).into_owned()))
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                get_encoding(&base, &diff)
            }
            Some(other) => get_encoding(
                &literal_name(other).unwrap_or_else(|| "StandardEncoding".to_string()),
                &[],
            ),
            None => get_encoding("StandardEncoding", &[]),
        };
        let unicode_map = match dget(doc, spec, b"ToUnicode") {
            Some(obj @ Object::Stream(_)) => {
                stream_data(obj).map(|data| UnicodeMap::File(parse_to_unicode(&data)))
            }
            _ => None,
        };
        (cid2unicode, unicode_map)
    }

    /// `PDFType1Font` / `PDFTrueTypeFont`.
    fn type1_font(doc: &Document, spec: &Dictionary) -> Font {
        let basefont = dget(doc, spec, b"BaseFont")
            .and_then(literal_name)
            .unwrap_or_else(|| "unknown".to_string());
        let (cid2unicode, unicode_map) = Self::simple_encoding(doc, spec);
        let mut font = Font {
            fontname: basefont.clone(),
            cmap: CMapKind::Bytes,
            cid2unicode,
            unicode_map,
            int_widths: HashMap::new(),
            str_widths: HashMap::new(),
            default_width: 0.0,
            descent: 0.0,
            hscale: 0.001,
            vscale: 0.001,
            multibyte: false,
            cidcoding: String::new(),
        };
        if let Some(metrics) = CORE14.get(&basefont) {
            // FontMetricsDB wins over the font dictionary (widths keyed by text).
            font.str_widths = metrics.widths.clone();
            font.descent = metrics.descent;
            let _ = (metrics.ascent, metrics.flags);
        } else {
            let descriptor = Self::descriptor(doc, spec);
            let firstchar = dget(doc, spec, b"FirstChar").and_then(num_f64).unwrap_or(0.0) as i64;
            match dget(doc, spec, b"Widths").and_then(|o| o.as_array().ok()) {
                Some(items) => {
                    for (i, w) in items.iter().enumerate() {
                        let w = num_f64(deref(doc, w)).unwrap_or(0.0);
                        let cid = firstchar + i as i64;
                        if cid >= 0 {
                            font.int_widths.insert(cid as u32, w);
                        }
                    }
                }
                None => {
                    for i in 0..256 {
                        font.int_widths.insert(firstchar.max(0) as u32 + i, 0.0);
                    }
                }
            }
            font.default_width = Self::desc_num(doc, &descriptor, b"MissingWidth");
            font.descent = Self::desc_num(doc, &descriptor, b"Descent");
            if let Some(name) = dget(doc, &descriptor, b"FontName").and_then(literal_name) {
                font.fontname = name;
            }
        }
        if font.descent > 0.0 {
            font.descent = -font.descent;
        }
        font
    }

    /// `PDFType3Font`: widths and descent scaled by the font matrix.
    fn type3_font(doc: &Document, spec: &Dictionary) -> Font {
        let (cid2unicode, unicode_map) = Self::simple_encoding(doc, spec);
        let firstchar = dget(doc, spec, b"FirstChar").and_then(num_f64).unwrap_or(0.0) as i64;
        let mut int_widths = HashMap::new();
        if let Some(items) = dget(doc, spec, b"Widths").and_then(|o| o.as_array().ok()) {
            for (i, w) in items.iter().enumerate() {
                let cid = firstchar + i as i64;
                if cid >= 0 {
                    int_widths.insert(cid as u32, num_f64(deref(doc, w)).unwrap_or(0.0));
                }
            }
        }
        let matrix: Vec<f64> = dget(doc, spec, b"FontMatrix")
            .and_then(|o| o.as_array().ok())
            .map(|a| a.iter().filter_map(|o| num_f64(deref(doc, o))).collect())
            .unwrap_or_default();
        let (hscale, vscale) = if matrix.len() == 6 {
            (matrix[0] + matrix[2], matrix[1] + matrix[3])
        } else {
            (0.001, 0.001)
        };
        let bbox: Vec<f64> = match dget(doc, spec, b"FontDescriptor").and_then(as_dict) {
            Some(desc) => dget(doc, desc, b"FontBBox"),
            None => dget(doc, spec, b"FontBBox"),
        }
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|o| num_f64(deref(doc, o))).collect())
        .unwrap_or_default();
        let descent = bbox.get(1).copied().unwrap_or(0.0);
        Font {
            fontname: "unknown".to_string(),
            cmap: CMapKind::Bytes,
            cid2unicode,
            unicode_map,
            int_widths,
            str_widths: HashMap::new(),
            default_width: 0.0,
            descent,
            hscale,
            vscale,
            multibyte: false,
            cidcoding: String::new(),
        }
    }

    /// `PDFCIDFont` (horizontal writing; vertical fonts are laid out horizontally).
    fn cid_font(doc: &Document, spec: &Dictionary) -> Font {
        let basefont = dget(doc, spec, b"BaseFont")
            .and_then(literal_name)
            .unwrap_or_else(|| "unknown".to_string());
        let sysinfo = dget(doc, spec, b"CIDSystemInfo").and_then(as_dict);
        let field = |key: &[u8]| -> String {
            sysinfo
                .and_then(|d| dget(doc, d, key))
                .and_then(|o| match o {
                    Object::String(s, _) => Some(String::from_utf8_lossy(s).trim().to_string()),
                    Object::Name(n) => Some(String::from_utf8_lossy(n).trim().to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| "unknown".to_string())
        };
        let cid_ordering = field(b"Ordering");
        let cidcoding = format!("{}-{}", field(b"Registry"), cid_ordering);
        let cmap_name = match dget(doc, spec, b"Encoding") {
            Some(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
            Some(Object::Stream(s)) => s
                .dict
                .get(b"CMapName")
                .ok()
                .and_then(literal_name)
                .unwrap_or_else(|| "unknown".to_string()),
            Some(Object::Dictionary(d)) => d
                .get(b"CMapName")
                .ok()
                .and_then(literal_name)
                .unwrap_or_else(|| "unknown".to_string()),
            _ => "unknown".to_string(),
        };
        let cmap_name = match cmap_name.as_str() {
            "DLIdent-H" => "Identity-H".to_string(),
            "DLIdent-V" => "Identity-V".to_string(),
            other => other.to_string(),
        };
        let cmap = match cmap_name.as_str() {
            "Identity-H" | "Identity-V" => CMapKind::Identity,
            "OneByteIdentityH" | "OneByteIdentityV" => CMapKind::OneByteIdentity,
            _ => CMapKind::Empty,
        };
        let descriptor = Self::descriptor(doc, spec);
        let unicode_map = match dget(doc, spec, b"ToUnicode") {
            Some(obj @ Object::Stream(_)) => {
                stream_data(obj).map(|data| UnicodeMap::File(parse_to_unicode(&data)))
            }
            Some(other) => {
                let name = literal_name(other).unwrap_or_default();
                let encoding = dget(doc, spec, b"Encoding").and_then(literal_name).unwrap_or_default();
                if cid_ordering.contains("Identity") || name.contains("Identity") || encoding.contains("Identity") {
                    Some(UnicodeMap::Identity)
                } else {
                    None
                }
            }
            None => {
                if cidcoding == "Adobe-Identity" || cidcoding == "Adobe-UCS" {
                    dget(doc, &descriptor, b"FontFile2")
                        .and_then(stream_data)
                        .and_then(|data| super::truetype::unicode_map(&data))
                        .map(UnicodeMap::File)
                } else {
                    None
                }
            }
        };
        let mut int_widths = HashMap::new();
        if let Some(items) = dget(doc, spec, b"W").and_then(|o| o.as_array().ok()) {
            let mut r: Vec<f64> = Vec::new();
            for item in items {
                match deref(doc, item) {
                    Object::Array(ws) => {
                        if let Some(&char1) = r.last() {
                            for (i, w) in ws.iter().enumerate() {
                                if let Some(w) = num_f64(deref(doc, w)) {
                                    int_widths.insert(char1 as u32 + i as u32, w);
                                }
                            }
                            r.clear();
                        }
                    }
                    other => {
                        if let Some(v) = num_f64(other) {
                            r.push(v);
                            if r.len() == 3 {
                                let (c1, c2, w) = (r[0] as i64, r[1] as i64, r[2]);
                                let mut c = c1;
                                while c <= c2 {
                                    if c >= 0 {
                                        int_widths.insert(c as u32, w);
                                    }
                                    c += 1;
                                }
                                r.clear();
                            }
                        }
                    }
                }
            }
        }
        let default_width = dget(doc, spec, b"DW").and_then(num_f64).unwrap_or(1000.0);
        let mut descent = Self::desc_num(doc, &descriptor, b"Descent");
        if descent > 0.0 {
            descent = -descent;
        }
        let fontname = dget(doc, &descriptor, b"FontName")
            .and_then(literal_name)
            .unwrap_or(basefont);
        Font {
            fontname,
            cmap,
            cid2unicode: HashMap::new(),
            unicode_map,
            int_widths,
            str_widths: HashMap::new(),
            default_width,
            descent,
            hscale: 0.001,
            vscale: 0.001,
            multibyte: true,
            cidcoding,
        }
    }

    /// `font.is_multibyte()`.
    pub fn is_multibyte(&self) -> bool {
        self.multibyte
    }

    /// `font.decode(bytes)`: the CIDs of a string.
    pub fn decode(&self, bytes: &[u8]) -> Vec<u32> {
        match self.cmap {
            CMapKind::Bytes | CMapKind::OneByteIdentity => bytes.iter().map(|&b| b as u32).collect(),
            CMapKind::Identity => bytes
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]) as u32)
                .collect(),
            CMapKind::Empty => Vec::new(),
        }
    }

    /// `font.to_unichr(cid)`; `None` where pdfminer raises `PDFUnicodeNotDefined`.
    pub fn to_unichr(&self, cid: u32) -> Option<String> {
        if self.multibyte {
            return match &self.unicode_map {
                Some(UnicodeMap::File(m)) => m.get(&cid).cloned(),
                Some(UnicodeMap::Identity) => char::from_u32(cid).map(|c| c.to_string()),
                None => None,
            };
        }
        if let Some(map) = &self.unicode_map {
            match map {
                UnicodeMap::File(m) => {
                    if let Some(t) = m.get(&cid) {
                        return Some(t.clone());
                    }
                }
                UnicodeMap::Identity => return char::from_u32(cid).map(|c| c.to_string()),
            }
        }
        self.cid2unicode.get(&cid).cloned()
    }

    /// The text pdfminer records for a CID (`(cid:N)` when undefined).
    pub fn char_text(&self, cid: u32) -> String {
        self.to_unichr(cid).unwrap_or_else(|| format!("(cid:{cid})"))
    }

    /// `font.char_width(cid)` in text space units.
    pub fn char_width(&self, cid: u32) -> f64 {
        if let Some(w) = self.int_widths.get(&cid) {
            return w * self.hscale;
        }
        if let Some(text) = self.to_unichr(cid)
            && let Some(w) = self.str_widths.get(&text)
        {
            return w * self.hscale;
        }
        self.default_width * self.hscale
    }

    /// `font.get_descent()`.
    pub fn get_descent(&self) -> f64 {
        self.descent * self.vscale
    }

    /// The CID coding (`Adobe-Identity`, ...) of a CID font.
    pub fn cidcoding(&self) -> &str {
        &self.cidcoding
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_unicode_cmaps() {
        let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
            1 begincodespacerange <00> <FF> endcodespacerange\n\
            2 beginbfchar <01> <0048> <02> <00650301> endbfchar\n\
            1 beginbfrange <10> <12> <0061> endbfrange\n\
            1 beginbfrange <20> <21> [<0058> <0059>] endbfrange\n\
            endcmap CMapName currentdict /CMap defineresource pop end end";
        let m = parse_to_unicode(cmap);
        assert_eq!(m.get(&1).map(String::as_str), Some("H"));
        assert_eq!(m.get(&2).map(String::as_str), Some("e\u{301}"));
        assert_eq!(m.get(&0x11).map(String::as_str), Some("b"));
        assert_eq!(m.get(&0x21).map(String::as_str), Some("Y"));
    }

    #[test]
    fn core14_metrics() {
        let helv = CORE14.get("Helvetica").expect("Helvetica metrics");
        assert_eq!(helv.widths.get(" "), Some(&278.0));
        assert_eq!(helv.widths.get("H"), Some(&722.0));
        assert_eq!(helv.descent, -207.0);
    }
}
