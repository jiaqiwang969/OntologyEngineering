//! PIL's EXIF/TIFF tag reading as `image_parser.py` sees it through Pillow 12: TIFF image file
//! directories (`TiffImagePlugin.ImageFileDirectory_v2`), the value shapes Pillow gives tags
//! (scalars for single values, tuples otherwise, `IFDRational`s, latin-1 strings, raw
//! bytes), `Image.Exif` with its `_get_merged_dict()` (IFD0 + Exif sub-IFD + GPS dict) in
//! exactly Pillow's key order — which comes from CPython `set` iteration, emulated here by
//! [`PySet`] — and `str()`/`repr()` of the values.

use std::collections::HashMap;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde_json::Value;

use sem_utils::pycompat::fmt::{float_repr, str_repr};

// ------------------------------------------------------------------------------ tables

struct TagInfo {
    length: Option<u32>,
}

struct Tables {
    v2: HashMap<u16, TagInfo>,
    groups: HashMap<u32, HashMap<u16, TagInfo>>,
    names: HashMap<u16, String>,
    open_info: HashMap<String, String>,
    compression: HashMap<u32, String>,
}

static TABLES: Lazy<Tables> = Lazy::new(|| {
    let mut t = Tables {
        v2: HashMap::new(),
        groups: HashMap::new(),
        names: HashMap::new(),
        open_info: HashMap::new(),
        compression: HashMap::new(),
    };
    let length = |s: &str| if s == "-" { None } else { s.parse().ok() };
    for line in include_str!("exif/tables.txt").lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        match parts.first().copied() {
            Some("v2") if parts.len() >= 4 => {
                if let Ok(tag) = parts[1].parse() {
                    t.v2.insert(tag, TagInfo { length: length(parts[3]) });
                }
            }
            Some("group") if parts.len() >= 5 => {
                if let (Ok(group), Ok(tag)) = (parts[1].parse(), parts[2].parse()) {
                    t.groups
                        .entry(group)
                        .or_default()
                        .insert(tag, TagInfo { length: length(parts[4]) });
                }
            }
            Some("name") if parts.len() >= 3 => {
                if let Ok(tag) = parts[1].parse() {
                    t.names.insert(tag, parts[2..].join(" "));
                }
            }
            Some("open") if parts.len() >= 8 => {
                t.open_info.insert(parts[1..7].join(" "), parts[7].to_string());
            }
            Some("comp") if parts.len() >= 3 => {
                if let Ok(code) = parts[1].parse() {
                    t.compression.insert(code, parts[2].to_string());
                }
            }
            _ => {}
        }
    }
    t
});

/// `ExifTags.TAGS.get(tag)`.
pub fn tag_name(tag: u16) -> Option<&'static str> {
    TABLES.names.get(&tag).map(String::as_str)
}

/// `TiffImagePlugin.OPEN_INFO[(byteorder, photo, sampleformat, fillorder, bps, extra)]`.
pub fn tiff_open_info(byteorder: &str, photo: i64, sf: &[i64], fillorder: i64, bps: &[i64], extra: &[i64]) -> Option<String> {
    let join = |v: &[i64]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
    let extra = if extra.is_empty() { "-".to_string() } else { join(extra) };
    let key = format!("{byteorder} {photo} {} {fillorder} {} {extra}", join(sf), join(bps));
    TABLES.open_info.get(&key).cloned()
}

/// `TiffImagePlugin.COMPRESSION_INFO[code]`.
pub fn tiff_compression(code: i64) -> Option<String> {
    u32::try_from(code).ok().and_then(|c| TABLES.compression.get(&c).cloned())
}

fn lookup_length(tag: u16, group: Option<u32>) -> Option<u32> {
    match group {
        Some(g) => TABLES.groups.get(&g).and_then(|t| t.get(&tag)).and_then(|i| i.length),
        None => TABLES.v2.get(&tag).and_then(|i| i.length),
    }
}

// ------------------------------------------------------------------------- PyValue

/// A Pillow tag value as Python sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum PyVal {
    Int(i64),
    Float(f64),
    /// `IFDRational(num, den)`.
    Rational(i64, i64),
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<PyVal>),
    /// A dict with int keys (the GPS IFD).
    Dict(Vec<(i64, PyVal)>),
    None,
}

/// `repr(bytes)`.
pub fn bytes_repr(b: &[u8]) -> String {
    let quote = if b.contains(&b'\'') && !b.contains(&b'"') { '"' } else { '\'' };
    let mut out = String::from("b");
    out.push(quote);
    for &c in b {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            c if c as char == quote => {
                out.push('\\');
                out.push(c as char);
            }
            0x20..=0x7e => out.push(c as char),
            c => out.push_str(&format!("\\x{c:02x}")),
        }
    }
    out.push(quote);
    out
}

impl PyVal {
    /// `float(IFDRational)`.
    fn rational_f64(num: i64, den: i64) -> f64 {
        if den == 0 { f64::NAN } else { num as f64 / den as f64 }
    }

    /// `repr(value)`.
    pub fn py_repr(&self) -> String {
        match self {
            PyVal::Int(i) => i.to_string(),
            PyVal::Float(f) => float_repr(*f),
            PyVal::Rational(n, d) => float_repr(Self::rational_f64(*n, *d)),
            PyVal::Str(s) => str_repr(s),
            PyVal::Bytes(b) => bytes_repr(b),
            PyVal::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(PyVal::py_repr).collect();
                if parts.len() == 1 {
                    format!("({},)", parts[0])
                } else {
                    format!("({})", parts.join(", "))
                }
            }
            PyVal::Dict(items) => {
                let parts: Vec<String> = items.iter().map(|(k, v)| format!("{k}: {}", v.py_repr())).collect();
                format!("{{{}}}", parts.join(", "))
            }
            PyVal::None => "None".to_string(),
        }
    }

    /// `str(value)`.
    pub fn py_str(&self) -> String {
        match self {
            PyVal::Str(s) => s.clone(),
            other => other.py_repr(),
        }
    }

    /// The JSON form used for `info` values: numbers as numbers (`IFDRational` as its float),
    /// strings as strings, tuples as arrays, bytes as their Python `repr` text.
    pub fn to_json(&self) -> Value {
        let num = |f: f64| serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null);
        match self {
            PyVal::Int(i) => Value::from(*i),
            PyVal::Float(f) => num(*f),
            PyVal::Rational(n, d) => num(Self::rational_f64(*n, *d)),
            PyVal::Str(s) => Value::String(s.clone()),
            PyVal::Bytes(b) => Value::String(bytes_repr(b)),
            PyVal::Tuple(items) => Value::Array(items.iter().map(PyVal::to_json).collect()),
            PyVal::Dict(items) => Value::Object(items.iter().map(|(k, v)| (k.to_string(), v.to_json())).collect()),
            PyVal::None => Value::Null,
        }
    }

    /// The integer value, if this is an int (or a 1-tuple of one).
    pub fn as_int(&self) -> Option<i64> {
        match self {
            PyVal::Int(i) => Some(*i),
            PyVal::Tuple(items) if items.len() == 1 => items[0].as_int(),
            _ => None,
        }
    }

    /// The value as `f64` (ints, floats and rationals).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PyVal::Int(i) => Some(*i as f64),
            PyVal::Float(f) => Some(*f),
            PyVal::Rational(n, d) => Some(Self::rational_f64(*n, *d)),
            _ => None,
        }
    }

    /// The items of a tuple (a scalar as a 1-item list).
    pub fn items(&self) -> Vec<PyVal> {
        match self {
            PyVal::Tuple(items) => items.clone(),
            other => vec![other.clone()],
        }
    }
}

// ------------------------------------------------------------------------------- IFD

/// Byte order of a TIFF structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

fn rd_u16(b: &[u8], e: Endian) -> u16 {
    let a = [b[0], b[1]];
    match e {
        Endian::Little => u16::from_le_bytes(a),
        Endian::Big => u16::from_be_bytes(a),
    }
}

fn rd_u32(b: &[u8], e: Endian) -> u32 {
    let a = [b[0], b[1], b[2], b[3]];
    match e {
        Endian::Little => u32::from_le_bytes(a),
        Endian::Big => u32::from_be_bytes(a),
    }
}

fn rd_u64(b: &[u8], e: Endian) -> u64 {
    let a = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];
    match e {
        Endian::Little => u64::from_le_bytes(a),
        Endian::Big => u64::from_be_bytes(a),
    }
}

/// Unit size of a TIFF field type (Pillow's `_load_dispatch`); `None` for types Pillow
/// skips.
fn unit_size(typ: u16) -> Option<usize> {
    match typ {
        1 | 2 | 6 | 7 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 | 13 => Some(4),
        5 | 10 | 12 | 16 => Some(8),
        _ => None,
    }
}

/// One loaded image file directory: tag -> (type, raw data) in file order.
#[derive(Debug, Clone, Default)]
pub struct Ifd {
    pub entries: IndexMap<u16, (u16, Vec<u8>)>,
    pub next: u64,
    pub endian: Option<Endian>,
    pub group: Option<u32>,
}

/// A TIFF header (`II*\0` / `MM\0*`) at the start of `data`: byte order and first IFD
/// offset.
pub fn tiff_header(data: &[u8]) -> Option<(Endian, u64)> {
    if data.len() < 8 {
        return None;
    }
    let endian = match &data[..2] {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return None,
    };
    if rd_u16(&data[2..4], endian) != 42 {
        return None;
    }
    Some((endian, rd_u32(&data[4..8], endian) as u64))
}

/// `ImageFileDirectory_v2.load(fp)` at `offset` of `data`.
pub fn load_ifd(data: &[u8], offset: usize, endian: Endian, group: Option<u32>) -> Ifd {
    let mut ifd = Ifd {
        endian: Some(endian),
        group,
        ..Ifd::default()
    };
    let Some(count_bytes) = data.get(offset..offset + 2) else {
        return ifd;
    };
    let count = rd_u16(count_bytes, endian) as usize;
    for i in 0..count {
        let pos = offset + 2 + 12 * i;
        let Some(entry) = data.get(pos..pos + 12) else {
            // A truncated read raises OSError in Pillow, which stops loading.
            return ifd;
        };
        let tag = rd_u16(&entry[0..2], endian);
        let typ = rd_u16(&entry[2..4], endian);
        let count = rd_u32(&entry[4..8], endian) as usize;
        let Some(unit) = unit_size(typ) else {
            continue;
        };
        let size = count.saturating_mul(unit);
        let value: Vec<u8> = if size > 4 {
            let at = rd_u32(&entry[8..12], endian) as usize;
            match data.get(at..at.saturating_add(size)) {
                Some(v) => v.to_vec(),
                None => {
                    // `_safe_read` raises OSError("Truncated File Read"): loading stops.
                    return ifd;
                }
            }
        } else {
            entry[8..8 + size].to_vec()
        };
        if value.is_empty() {
            continue;
        }
        ifd.entries.insert(tag, (typ, value));
    }
    if let Some(next) = data.get(offset + 2 + 12 * count..offset + 6 + 12 * count) {
        ifd.next = rd_u32(next, endian) as u64;
    }
    ifd
}

impl Ifd {
    /// `ifd[tag]`: Pillow's decoded, shaped value.
    pub fn get(&self, tag: u16) -> Option<PyVal> {
        let (typ, data) = self.entries.get(&tag)?;
        let e = self.endian.unwrap_or(Endian::Little);
        let raw: PyVal = match typ {
            1 | 7 => PyVal::Bytes(data.clone()),
            2 => {
                let d = data.strip_suffix(b"\0").unwrap_or(data);
                PyVal::Str(d.iter().map(|&b| b as char).collect())
            }
            3 => PyVal::Tuple(data.chunks(2).filter(|c| c.len() == 2).map(|c| PyVal::Int(rd_u16(c, e) as i64)).collect()),
            4 | 13 => PyVal::Tuple(data.chunks(4).filter(|c| c.len() == 4).map(|c| PyVal::Int(rd_u32(c, e) as i64)).collect()),
            5 => PyVal::Tuple(
                data.chunks(8)
                    .filter(|c| c.len() == 8)
                    .map(|c| PyVal::Rational(rd_u32(&c[..4], e) as i64, rd_u32(&c[4..], e) as i64))
                    .collect(),
            ),
            6 => PyVal::Tuple(data.iter().map(|&b| PyVal::Int(b as i8 as i64)).collect()),
            8 => PyVal::Tuple(data.chunks(2).filter(|c| c.len() == 2).map(|c| PyVal::Int(rd_u16(c, e) as i16 as i64)).collect()),
            9 => PyVal::Tuple(data.chunks(4).filter(|c| c.len() == 4).map(|c| PyVal::Int(rd_u32(c, e) as i32 as i64)).collect()),
            10 => PyVal::Tuple(
                data.chunks(8)
                    .filter(|c| c.len() == 8)
                    .map(|c| PyVal::Rational(rd_u32(&c[..4], e) as i32 as i64, rd_u32(&c[4..], e) as i32 as i64))
                    .collect(),
            ),
            11 => PyVal::Tuple(data.chunks(4).filter(|c| c.len() == 4).map(|c| PyVal::Float(f32::from_bits(rd_u32(c, e)) as f64)).collect()),
            12 => PyVal::Tuple(data.chunks(8).filter(|c| c.len() == 8).map(|c| PyVal::Float(f64::from_bits(rd_u64(c, e)))).collect()),
            16 => PyVal::Tuple(data.chunks(8).filter(|c| c.len() == 8).map(|c| PyVal::Int(rd_u64(c, e) as i64)).collect()),
            _ => return None,
        };
        // `_setitem`: single values become scalars for length-1 tags, BYTE tags and
        // unspecified-length tags with one value.
        let values = match raw {
            PyVal::Tuple(items) => items,
            scalar => vec![scalar],
        };
        let length = lookup_length(tag, self.group);
        if values.is_empty() {
            return Some(PyVal::Tuple(values));
        }
        if length == Some(1) || *typ == 1 || (length.is_none() && values.len() == 1) {
            Some(values.into_iter().next().unwrap_or(PyVal::None))
        } else {
            Some(PyVal::Tuple(values))
        }
    }
}

// --------------------------------------------------------------------- CPython sets

const LINEAR_PROBES: usize = 9;
const PERTURB_SHIFT: u32 = 5;

/// A CPython 3.11 `set` of (small, non-negative) ints, reproducing its iteration order.
#[derive(Debug, Clone)]
pub struct PySet {
    table: Vec<Option<i64>>,
    fill: usize,
    used: usize,
}

impl Default for PySet {
    fn default() -> Self {
        PySet {
            table: vec![None; 8],
            fill: 0,
            used: 0,
        }
    }
}

impl PySet {
    fn mask(&self) -> usize {
        self.table.len() - 1
    }

    fn insert_clean(table: &mut [Option<i64>], mask: usize, key: i64) {
        let hash = key as u64;
        let mut perturb = hash;
        let mut i = (hash as usize) & mask;
        loop {
            if table[i].is_none() {
                table[i] = Some(key);
                return;
            }
            if i + LINEAR_PROBES <= mask {
                for j in 1..=LINEAR_PROBES {
                    if table[i + j].is_none() {
                        table[i + j] = Some(key);
                        return;
                    }
                }
            }
            perturb >>= PERTURB_SHIFT;
            i = (i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb as usize)) & mask;
        }
    }

    fn resize(&mut self, minused: usize) {
        let mut newsize = 8usize;
        while newsize <= minused {
            newsize <<= 1;
        }
        let old = std::mem::replace(&mut self.table, vec![None; newsize]);
        let mask = newsize - 1;
        for key in old.into_iter().flatten() {
            Self::insert_clean(&mut self.table, mask, key);
        }
        self.fill = self.used;
    }

    /// `set.add(key)` (`set_add_entry`).
    pub fn add(&mut self, key: i64) {
        let hash = key as u64;
        let mask = self.mask();
        let mut i = (hash as usize) & mask;
        let mut perturb = hash;
        loop {
            let probes = if i + LINEAR_PROBES <= mask { LINEAR_PROBES } else { 0 };
            for j in i..=i + probes {
                match self.table[j] {
                    None => {
                        self.table[j] = Some(key);
                        self.fill += 1;
                        self.used += 1;
                        if self.fill * 5 >= mask * 3 {
                            let minused = if self.used > 50000 { self.used * 2 } else { self.used * 4 };
                            self.resize(minused);
                        }
                        return;
                    }
                    Some(k) if k == key => return,
                    _ => {}
                }
            }
            perturb >>= PERTURB_SHIFT;
            i = (i.wrapping_mul(5).wrapping_add(1).wrapping_add(perturb as usize)) & mask;
        }
    }

    /// `set(d)` for a dict `d` with these keys (in insertion order).
    pub fn from_dict_keys(keys: &[i64]) -> PySet {
        let mut s = PySet::default();
        if (s.fill + keys.len()) * 5 >= s.mask() * 3 {
            s.resize((s.used + keys.len()) * 2);
        }
        for &k in keys {
            s.add(k);
        }
        s
    }

    /// `set_merge(self, other)`.
    fn merge(&mut self, other: &PySet) {
        if other.used == 0 {
            return;
        }
        if (self.fill + other.used) * 5 >= self.mask() * 3 {
            self.resize((self.used + other.used) * 2);
        }
        if self.fill == 0 && self.mask() == other.mask() && other.fill == other.used {
            self.table = other.table.clone();
            self.fill = other.fill;
            self.used = other.used;
            return;
        }
        if self.fill == 0 {
            let mask = self.mask();
            self.fill = other.used;
            self.used = other.used;
            for key in other.table.iter().flatten() {
                Self::insert_clean(&mut self.table, mask, *key);
            }
            return;
        }
        for key in other.table.iter().flatten() {
            self.add(*key);
        }
    }

    /// `a | b`.
    pub fn union(a: &PySet, b: &PySet) -> PySet {
        let mut result = PySet::default();
        result.merge(a);
        result.merge(b);
        result
    }

    /// `set.update(iterable)` for a non-set iterable.
    pub fn update_iter(&mut self, keys: impl IntoIterator<Item = i64>) {
        for k in keys {
            self.add(k);
        }
    }

    /// Iteration order.
    pub fn keys(&self) -> Vec<i64> {
        self.table.iter().flatten().copied().collect()
    }
}

// ------------------------------------------------------------------------- Image.Exif

/// `Image.Exif` loaded from EXIF bytes (`Exif.load`), with Pillow's `_data`/`_info` split.
#[derive(Debug, Clone)]
pub struct Exif {
    data: Vec<u8>,
    endian: Endian,
    /// `_data`: tag -> value, in insertion order.
    resolved: IndexMap<u16, PyVal>,
    /// `_info`: the IFD0 entries not yet moved into `_data`.
    info: Option<Ifd>,
}

impl Exif {
    /// `Exif().load(data)` (a leading `Exif\0\0` is skipped). `None` when the data does not
    /// start with a TIFF header (Pillow raises `SyntaxError`).
    pub fn load(raw: &[u8]) -> Option<Exif> {
        let mut data = raw;
        while data.starts_with(b"Exif\x00\x00") {
            data = &data[6..];
        }
        if data.is_empty() {
            return Some(Exif {
                data: Vec::new(),
                endian: Endian::Little,
                resolved: IndexMap::new(),
                info: None,
            });
        }
        let (endian, first) = tiff_header(data)?;
        let ifd = load_ifd(data, first as usize, endian, None);
        Some(Exif {
            data: data.to_vec(),
            endian,
            resolved: IndexMap::new(),
            info: Some(ifd),
        })
    }

    /// `tag in exif`.
    pub fn contains(&self, tag: u16) -> bool {
        self.resolved.contains_key(&tag) || self.info.as_ref().is_some_and(|i| i.entries.contains_key(&tag))
    }

    /// `exif[tag]`: moves the tag from `_info` to `_data` (applying `_fixup`).
    pub fn get(&mut self, tag: u16) -> Option<PyVal> {
        if !self.resolved.contains_key(&tag)
            && let Some(info) = &mut self.info
            && info.entries.contains_key(&tag)
        {
            let value = fixup(info.get(tag)?);
            info.entries.shift_remove(&tag);
            self.resolved.insert(tag, value);
        }
        self.resolved.get(&tag).cloned()
    }

    /// `iter(exif)`: `set(_data)` updated with the iteration of `_info`
    /// (`set(_tagdata) | set(_tags_v2)`).
    fn iter_keys(&self) -> Vec<u16> {
        let data_keys: Vec<i64> = self.resolved.keys().map(|&k| k as i64).collect();
        let mut keys = PySet::from_dict_keys(&data_keys);
        if let Some(info) = &self.info {
            let tagdata: Vec<i64> = info.entries.keys().map(|&k| k as i64).collect();
            let union = PySet::union(&PySet::from_dict_keys(&tagdata), &PySet::default());
            keys.update_iter(union.keys());
        }
        keys.keys().into_iter().map(|k| k as u16).collect()
    }

    fn ifd_dict(&self, offset: i64, group: u32) -> Option<Vec<(i64, PyVal)>> {
        let offset = usize::try_from(offset).ok()?;
        let ifd = load_ifd(&self.data, offset, self.endian, Some(group));
        let tagdata: Vec<i64> = ifd.entries.keys().map(|&k| k as i64).collect();
        let order = PySet::union(&PySet::from_dict_keys(&tagdata), &PySet::default()).keys();
        Some(
            order
                .into_iter()
                .filter_map(|k| ifd.get(k as u16).map(|v| (k, fixup(v))))
                .collect(),
        )
    }

    /// `exif._get_merged_dict()`: IFD0, then the Exif sub-IFD, with the GPS IFD as a dict.
    pub fn merged_dict(&mut self) -> Vec<(i64, PyVal)> {
        let mut merged: IndexMap<i64, PyVal> = IndexMap::new();
        for key in self.iter_keys() {
            if let Some(v) = self.get(key) {
                merged.insert(key as i64, v);
            }
        }
        if let Some(offset) = merged.get(&34665).and_then(PyVal::as_int)
            && let Some(ifd) = self.ifd_dict(offset, 34665)
        {
            for (k, v) in ifd {
                merged.insert(k, v);
            }
        }
        if let Some(offset) = merged.get(&34853).and_then(PyVal::as_int) {
            let gps = self.ifd_dict(offset, 34853);
            merged.insert(34853, gps.map(PyVal::Dict).unwrap_or(PyVal::None));
        }
        merged.into_iter().collect()
    }
}

/// `Exif._fixup`: a 1-tuple becomes its element.
fn fixup(v: PyVal) -> PyVal {
    match v {
        PyVal::Tuple(mut items) if items.len() == 1 => items.remove(0),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_order_matches_cpython() {
        // The IFD0 tags Pillow writes for an EXIF block with Make, Model, XResolution,
        // ResolutionUnit, DateTime and the Exif pointer, as iterated by CPython 3.11.
        let s = PySet::from_dict_keys(&[296, 282, 34665, 271, 272, 306]);
        assert_eq!(s.keys(), vec![272, 306, 296, 34665, 282, 271]);
        let mut keys = PySet::from_dict_keys(&[296, 282]);
        let union = PySet::union(&PySet::from_dict_keys(&[271, 272, 306, 34665]), &PySet::default());
        assert_eq!(union.keys(), vec![272, 34665, 306, 271]);
        keys.update_iter(union.keys());
        assert_eq!(keys.keys(), vec![296, 34665, 271, 272, 306, 282]);
    }

    #[test]
    fn reprs() {
        assert_eq!(bytes_repr(b"ASCII\0\0\0hi"), "b'ASCII\\x00\\x00\\x00hi'");
        assert_eq!(PyVal::Rational(1, 3).py_str(), "0.3333333333333333");
        assert_eq!(PyVal::Rational(72, 1).py_str(), "72.0");
        assert_eq!(PyVal::Tuple(vec![PyVal::Int(1)]).py_repr(), "(1,)");
    }
}
