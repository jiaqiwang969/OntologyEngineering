//! A workbook as openpyxl 3.1.5 reads it with `data_only=True`: sheet list, shared
//! strings, date number formats, and worksheet cells (`WorkSheetParser`), with the two views
//! the original uses: read-only rows (`ReadOnlyWorksheet.rows`, used by pandas) and the
//! normal-mode cell grid (`Worksheet._cells`, including merged cells).
//!
//! Cell values are the Python objects openpyxl produces ([`PyVal`]): `int`/`float` from the
//! number text (`_cast_number`), `str`, `bool`, `datetime`/`time`/`timedelta` for
//! date-formatted numbers (`from_excel`), ISO dates for `t="d"` cells, and error strings.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

use super::dom::{self, Element};
use super::opc::normpath;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const XLSM: &str = "application/vnd.ms-excel.sheet.macroEnabled.main+xml";
const XLTX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml";
const XLTM: &str = "application/vnd.ms-excel.template.macroEnabled.main+xml";
const SHARED_STRINGS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";

/// A Python value as openpyxl (and pandas) hands it over.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PyVal {
    Str(String),
    Int(i128),
    Float(f64),
    Bool(bool),
    DateTime(NaiveDateTime),
    Date(NaiveDate),
    Time(NaiveTime),
    /// A `datetime.timedelta`, in microseconds.
    TimeDelta(i64),
    /// `float('nan')` (pandas' value for error cells).
    Nan,
}

fn micros_str(micros: u32) -> String {
    if micros == 0 {
        String::new()
    } else {
        format!(".{micros:06}")
    }
}

/// `str(datetime)`.
pub(crate) fn datetime_str(dt: &NaiveDateTime) -> String {
    format!(
        "{}{}",
        dt.format("%Y-%m-%d %H:%M:%S"),
        micros_str(dt.nanosecond() / 1000)
    )
}

/// `str(time)`.
pub(crate) fn time_str(t: &NaiveTime) -> String {
    format!("{}{}", t.format("%H:%M:%S"), micros_str(t.nanosecond() / 1000))
}

/// `str(datetime.timedelta)`.
pub(crate) fn timedelta_str(micros: i64) -> String {
    let days = micros.div_euclid(86_400_000_000);
    let rem = micros.rem_euclid(86_400_000_000);
    let secs = rem / 1_000_000;
    let us = rem % 1_000_000;
    let hms = format!("{}:{:02}:{:02}{}", secs / 3600, (secs / 60) % 60, secs % 60, micros_str(us as u32));
    if days == 0 {
        hms
    } else {
        let plural = if days.abs() != 1 { "s" } else { "" };
        format!("{days} day{plural}, {hms}")
    }
}

/// `str(pandas.Timedelta)`.
pub(crate) fn pandas_timedelta_str(micros: i64) -> String {
    let days = micros.div_euclid(86_400_000_000);
    let rem = micros.rem_euclid(86_400_000_000);
    let secs = rem / 1_000_000;
    let us = rem % 1_000_000;
    let sign = if days < 0 { "+" } else { "" };
    format!(
        "{days} days {sign}{:02}:{:02}:{:02}{}",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60,
        micros_str(us as u32)
    )
}

impl PyVal {
    /// `str(value)`.
    pub(crate) fn py_str(&self) -> String {
        match self {
            PyVal::Str(s) => s.clone(),
            PyVal::Int(i) => i.to_string(),
            PyVal::Float(f) => sem_utils::pycompat::fmt::float_repr(*f),
            PyVal::Bool(true) => "True".to_string(),
            PyVal::Bool(false) => "False".to_string(),
            PyVal::DateTime(dt) => datetime_str(dt),
            PyVal::Date(d) => d.format("%Y-%m-%d").to_string(),
            PyVal::Time(t) => time_str(t),
            PyVal::TimeDelta(us) => timedelta_str(*us),
            PyVal::Nan => "nan".to_string(),
        }
    }

    /// `type(value).__name__`.
    #[allow(dead_code)] // kept for the Docling office backends
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            PyVal::Str(_) => "str",
            PyVal::Int(_) => "int",
            PyVal::Float(_) | PyVal::Nan => "float",
            PyVal::Bool(_) => "bool",
            PyVal::DateTime(_) => "datetime",
            PyVal::Date(_) => "date",
            PyVal::Time(_) => "time",
            PyVal::TimeDelta(_) => "timedelta",
        }
    }
}

/// A parsed `<c>` element (`WorkSheetParser.parse_cell`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedCell {
    pub row: i64,
    pub column: i64,
    pub value: Option<PyVal>,
    /// openpyxl's `data_type` (`n`, `s`, `b`, `e`, `d`, `inlineStr`).
    pub data_type: String,
    pub style_id: i64,
}

/// A sheet listed in the workbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SheetEntry {
    pub title: String,
    /// The worksheet part (archive member name).
    pub target: String,
    pub chartsheet: bool,
}

/// A loaded workbook.
#[derive(Debug, Clone)]
pub(crate) struct Workbook {
    members: HashMap<String, Vec<u8>>,
    /// `wb.sheetnames` order (worksheets and chartsheets).
    pub sheets: Vec<SheetEntry>,
    shared_strings: Vec<String>,
    date_formats: HashSet<i64>,
    timedelta_formats: HashSet<i64>,
    cell_style_count: usize,
    date1904: bool,
}

/// The error text of `zipfile.ZipFile(path)` on a file that is not a zip archive.
pub(crate) const NOT_A_ZIP: &str = "File is not a zip file";

fn key_error(msg: &str) -> String {
    sem_utils::pycompat::fmt::str_repr(msg)
}

/// Read every archive member.
pub(crate) fn read_zip(path: &Path) -> Result<HashMap<String, Vec<u8>>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| NOT_A_ZIP.to_string())?;
    let mut members = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| format!("Bad CRC-32 for file '{name}': {e}"))?;
        members.insert(name, buf);
    }
    Ok(members)
}

/// `openpyxl.styles.numbers.BUILTIN_FORMATS`.
fn builtin_format(id: i64) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "\"$\"#,##0_);(\"$\"#,##0)",
        6 => "\"$\"#,##0_);[Red](\"$\"#,##0)",
        7 => "\"$\"#,##0.00_);(\"$\"#,##0.00)",
        8 => "\"$\"#,##0.00_);[Red](\"$\"#,##0.00)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "mm-dd-yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0_);(#,##0)",
        38 => "#,##0_);[Red](#,##0)",
        39 => "#,##0.00_);(#,##0.00)",
        40 => "#,##0.00_);[Red](#,##0.00)",
        41 => r#"_(* #,##0_);_(* \(#,##0\);_(* "-"_);_(@_)"#,
        42 => r#"_("$"* #,##0_);_("$"* \(#,##0\);_("$"* "-"_);_(@_)"#,
        43 => r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#,
        44 => r#"_("$"* #,##0.00_)_("$"* \(#,##0.00\)_("$"* "-"??_)_(@_)"#,
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

/// `STRIP_RE.sub("", fmt)`: drop quoted literals and bracketed groups other than
/// `[h]`/`[hh]`/`[m]`/`[mm]`/`[s]`/`[ss]`.
fn strip_format(fmt: &str) -> String {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == '"') {
                i += end + 2;
                continue;
            }
        } else if c == '[' {
            let rest: String = chars[i + 1..].iter().collect();
            let keep = ["h]", "hh]", "m]", "mm]", "s]", "ss]"]
                .iter()
                .any(|p| rest.starts_with(p));
            if !keep && let Some(end) = chars[i + 1..].iter().position(|&x| x == ']') {
                i += end + 2;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// `is_date_format(fmt)`.
pub(crate) fn is_date_format(fmt: Option<&str>) -> bool {
    let Some(fmt) = fmt else { return false };
    let first = fmt.split(';').next().unwrap_or("");
    let stripped: Vec<char> = strip_format(first).chars().collect();
    stripped.iter().enumerate().any(|(i, &c)| {
        "dmhysDMHYS".contains(c) && (i == 0 || !matches!(stripped[i - 1], '_' | '\\'))
    })
}

/// `is_timedelta_format(fmt)`.
pub(crate) fn is_timedelta_format(fmt: Option<&str>) -> bool {
    let Some(fmt) = fmt else { return false };
    let first = fmt.split(';').next().unwrap_or("").to_lowercase();
    ["[h]", "[hh]", "[m]", "[mm]", "[s]", "[ss]"]
        .iter()
        .any(|p| first.contains(p))
}

/// `Text.from_tree(node).content` (plain `t` plus rich-text runs).
fn text_content(node: &Element) -> String {
    let mut plain: Option<String> = None;
    let mut runs = Vec::new();
    for child in node.elements() {
        if child.ns.as_deref() != Some(MAIN) {
            continue;
        }
        match child.name.as_str() {
            "t" => plain = child.text().map(str::to_string),
            "r" => {
                if let Some(t) = child.child(MAIN, "t").and_then(Element::text) {
                    runs.push(t.to_string());
                }
            }
            _ => {}
        }
    }
    let mut out = plain.unwrap_or_default();
    for r in runs {
        out.push_str(&r);
    }
    out
}

/// `posixpath.normpath(posixpath.join(parent, target))` for relationship targets.
fn rel_target(rels_path: &str, target: &str) -> String {
    if let Some(stripped) = target.strip_prefix('/') {
        return stripped.to_string();
    }
    let folder = rels_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let parent = folder.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let joined = if parent.is_empty() {
        target.to_string()
    } else {
        format!("{parent}/{target}")
    };
    normpath(&joined)
}

/// `float(text)` for openpyxl's `_cast_number`.
fn py_float(text: &str) -> Result<f64, String> {
    crate::config::py_float(text)
        .ok_or_else(|| format!("could not convert string to float: '{text}'"))
}

/// `_cast_number(value)`.
fn cast_number(text: &str) -> Result<PyVal, String> {
    if text.contains('.') || text.contains('E') || text.contains('e') {
        return py_float(text).map(PyVal::Float);
    }
    let t = text.trim();
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let clean: String = digits.chars().filter(|c| *c != '_').collect();
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid literal for int() with base 10: '{text}'"));
    }
    let n: i128 = clean.parse().map_err(|_| "int too large".to_string())?;
    Ok(PyVal::Int(if neg { -n } else { n }))
}

/// Python's `round()` of a float to an integer (round half to even).
pub(crate) fn round_half_even(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 {
        2.0 * (x / 2.0).round()
    } else {
        r
    }
}

/// CPython's `divmod(value, 1)` for floats.
fn divmod1(v: f64) -> (f64, f64) {
    let mut m = v % 1.0;
    let mut div = v - m;
    if m != 0.0 {
        if m < 0.0 {
            m += 1.0;
            div -= 1.0;
        }
    } else {
        m = 0.0;
    }
    let floordiv = if div != 0.0 {
        let f = div.floor();
        if div - f > 0.5 { f + 1.0 } else { f }
    } else {
        0.0
    };
    (floordiv, m)
}

fn epoch(date1904: bool) -> NaiveDateTime {
    if date1904 {
        NaiveDate::from_ymd_opt(1904, 1, 1).expect("date").and_hms_opt(0, 0, 0).expect("time")
    } else {
        NaiveDate::from_ymd_opt(1899, 12, 30).expect("date").and_hms_opt(0, 0, 0).expect("time")
    }
}

/// `from_excel(value, epoch, timedelta)`.
fn from_excel(value: &PyVal, date1904: bool, timedelta: bool) -> Result<PyVal, String> {
    let v = match value {
        PyVal::Int(i) => *i as f64,
        PyVal::Float(f) => *f,
        _ => return Err("bad serial".to_string()),
    };
    if timedelta {
        let mut micros = round_half_even(v * 86_400_000_000.0);
        if micros.abs() > 9.0e18 {
            return Err("days=1000000000; must have magnitude <= 999999999".to_string());
        }
        let us = micros.rem_euclid(1_000_000.0);
        if us != 0.0 {
            let total_seconds = micros / 1_000_000.0;
            let secs = total_seconds.floor();
            let ms = round_half_even(us / 1000.0) * 1000.0;
            micros = secs * 1_000_000.0 + ms;
        }
        return Ok(PyVal::TimeDelta(micros as i64));
    }
    let (mut day, fraction) = match value {
        PyVal::Int(i) => (*i as f64, 0.0),
        _ => divmod1(v),
    };
    let ms = round_half_even(fraction * 86400.0 * 1000.0) as i64;
    let diff = Duration::milliseconds(ms);
    if (0.0..1.0).contains(&v) && diff.num_days() == 0 {
        let secs = diff.num_seconds();
        let micros = (diff - Duration::seconds(secs)).num_microseconds().unwrap_or(0);
        let t = NaiveTime::from_hms_micro_opt(
            (secs / 3600) as u32,
            ((secs / 60) % 60) as u32,
            (secs % 60) as u32,
            micros as u32,
        )
        .ok_or("bad time")?;
        return Ok(PyVal::Time(t));
    }
    if 0.0 < v && v < 60.0 && !date1904 {
        day += 1.0;
    }
    if day.abs() > 3_000_000.0 {
        return Err("date value out of range".to_string());
    }
    let dt = epoch(date1904)
        .checked_add_signed(Duration::days(day as i64))
        .and_then(|d| d.checked_add_signed(diff))
        .ok_or("date value out of range")?;
    if dt.date().year_ce().1 > 9999 || dt.date() < NaiveDate::from_ymd_opt(1, 1, 1).expect("date") {
        return Err("date value out of range".to_string());
    }
    Ok(PyVal::DateTime(dt))
}

use chrono::Datelike;

/// `from_ISO8601(text)`.
fn from_iso8601(text: &str) -> Result<Option<PyVal>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    static ISO: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(
            r"^(?P<date>(?P<year>\d{4})-(?P<month>\d{2})-(?P<day>\d{2}))?T?(?P<time>(?P<hour>\d{2}):(?P<minute>\d{2})(:(?P<second>\d{2})(?P<microsecond>\.\d{1,3})?)?)?Z?",
        )
        .expect("valid regex")
    });
    static DURATION: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
        regex::Regex::new(r"^PT((?P<hours>\d+)H)?((?P<minutes>\d+)M)?((?P<seconds>\d+(\.\d{1,3})?)S)?")
            .expect("valid regex")
    });
    if let Some(c) = ISO.captures(text)
        && (c.name("date").is_some() || c.name("time").is_some())
    {
        let num = |k: &str| c.name(k).map(|m| m.as_str().parse::<u32>().unwrap_or(0)).unwrap_or(0);
        let micro = c
            .name("microsecond")
            .map(|m| (m.as_str().parse::<f64>().unwrap_or(0.0) * 1_000_000.0) as u32)
            .unwrap_or(0);
        let time = NaiveTime::from_hms_micro_opt(num("hour"), num("minute"), num("second"), micro);
        if c.name("date").is_none() {
            return time
                .map(|t| Some(PyVal::Time(t)))
                .ok_or_else(|| "hour must be in 0..23".to_string());
        }
        let date = NaiveDate::from_ymd_opt(num("year") as i32, num("month"), num("day"))
            .ok_or_else(|| "day is out of range for month".to_string())?;
        if c.name("time").is_none() {
            return Ok(Some(PyVal::Date(date)));
        }
        let time = time.ok_or_else(|| "hour must be in 0..23".to_string())?;
        return Ok(Some(PyVal::DateTime(date.and_time(time))));
    }
    if let Some(c) = DURATION.captures(text)
        && (c.name("hours").is_some() || c.name("minutes").is_some() || c.name("seconds").is_some())
    {
        let f = |k: &str| c.name(k).map(|m| m.as_str().parse::<f64>().unwrap_or(0.0)).unwrap_or(0.0);
        let micros = round_half_even((f("hours") * 3600.0 + f("minutes") * 60.0 + f("seconds")) * 1_000_000.0);
        return Ok(Some(PyVal::TimeDelta(micros as i64)));
    }
    Err(format!("Invalid datetime value {text}"))
}

/// `coordinate_to_tuple("B7")` -> (7, 2).
fn coordinate_to_tuple(coord: &str) -> Result<(i64, i64), String> {
    let s = coord.trim_start_matches('$');
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let rest = s[letters.len()..].trim_start_matches('$');
    if letters.is_empty() || letters.len() > 3 || rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("Invalid cell coordinates ({coord})"));
    }
    let row: i64 = rest.parse().map_err(|_| format!("Invalid cell coordinates ({coord})"))?;
    if row == 0 {
        return Err(format!("There is no row 0 ({coord})"));
    }
    let mut col: i64 = 0;
    for c in letters.to_ascii_uppercase().chars() {
        col = col * 26 + (c as i64 - 'A' as i64 + 1);
    }
    Ok((row, col))
}

impl Workbook {
    /// `load_workbook(path, data_only=True)` (both modes share this loading).
    pub(crate) fn load(path: &Path) -> Result<Workbook, String> {
        let members = read_zip(path)?;
        let ct_blob = members
            .get("[Content_Types].xml")
            .ok_or_else(|| key_error("There is no item named '[Content_Types].xml' in the archive"))?;
        let ct = dom::parse(ct_blob)?;
        let overrides: Vec<(String, String)> = ct
            .children_named(CT_NS, "Override")
            .map(|o| {
                (
                    o.attr(None, "PartName").unwrap_or("").to_string(),
                    o.attr(None, "ContentType").unwrap_or("").to_string(),
                )
            })
            .collect();
        let find = |content_type: &str| {
            overrides
                .iter()
                .find(|(_, c)| c == content_type)
                .map(|(p, _)| p.clone())
        };
        // Shared strings (`read_strings`).
        let mut shared_strings = Vec::new();
        if let Some(part) = find(SHARED_STRINGS) {
            let name = part.trim_start_matches('/').to_string();
            let blob = members
                .get(&name)
                .ok_or_else(|| key_error(&format!("There is no item named '{name}' in the archive")))?;
            let root = dom::parse(blob)?;
            for si in root.children_named(MAIN, "si") {
                shared_strings.push(text_content(si).replace("x005F_", ""));
            }
        }
        // Workbook part (`_find_workbook_part`).
        let wb_part = [XLTM, XLTX, XLSM, XLSX]
            .iter()
            .find_map(|c| find(c))
            .or_else(|| {
                let defaults: Vec<&str> = ct
                    .children_named(CT_NS, "Default")
                    .filter_map(|d| d.attr(None, "ContentType"))
                    .collect();
                [XLTM, XLTX, XLSM, XLSX]
                    .iter()
                    .any(|c| defaults.contains(c))
                    .then(|| "/xl/workbook.xml".to_string())
            })
            .ok_or_else(|| "File contains no valid workbook part".to_string())?;
        let wb_name = wb_part.trim_start_matches('/').to_string();
        let wb_blob = members
            .get(&wb_name)
            .ok_or_else(|| key_error(&format!("There is no item named '{wb_name}' in the archive")))?;
        let wb_root = dom::parse(wb_blob)?;
        let date1904 = wb_root
            .child(MAIN, "workbookPr")
            .and_then(|p| p.attr(None, "date1904"))
            .is_some_and(|v| v == "1" || v == "true");
        // Workbook relationships.
        let (dir, file) = wb_name.rsplit_once('/').unwrap_or(("", wb_name.as_str()));
        let rels_path = if dir.is_empty() {
            format!("_rels/{file}.rels")
        } else {
            format!("{dir}/_rels/{file}.rels")
        };
        let mut rels: Vec<(String, String, String)> = Vec::new();
        if let Some(blob) = members.get(&rels_path) {
            let root = dom::parse(blob)?;
            for r in root.children_named(PKG_RELS, "Relationship") {
                let id = r.attr(None, "Id").unwrap_or("").to_string();
                let rtype = r.attr(None, "Type").unwrap_or("").to_string();
                let target = r.attr(None, "Target").unwrap_or("");
                let external = r.attr(None, "TargetMode") == Some("External");
                let target = if external {
                    target.to_string()
                } else {
                    rel_target(&rels_path, target)
                };
                rels.push((id, rtype, target));
            }
        }
        let mut sheets = Vec::new();
        if let Some(list) = wb_root.child(MAIN, "sheets") {
            for sheet in list.children_named(MAIN, "sheet") {
                let Some(r_id) = sheet.attr(Some(REL_NS), "id").filter(|s| !s.is_empty()) else {
                    continue;
                };
                let rel = rels
                    .iter()
                    .find(|(id, _, _)| id == r_id)
                    .ok_or_else(|| key_error(&format!("Unknown relationship: {r_id}")))?;
                if !members.contains_key(&rel.2) {
                    continue;
                }
                sheets.push(SheetEntry {
                    title: sheet.attr(None, "name").unwrap_or("").to_string(),
                    target: rel.2.clone(),
                    chartsheet: rel.1.contains("chartsheet"),
                });
            }
        }
        // Stylesheet (`apply_stylesheet` / `_normalise_numbers`).
        let mut date_formats = HashSet::new();
        let mut timedelta_formats = HashSet::new();
        let mut cell_style_count = 1;
        if let Some(blob) = members.get("xl/styles.xml") {
            let root = dom::parse(blob)?;
            let custom: HashMap<i64, String> = root
                .child(MAIN, "numFmts")
                .map(|n| {
                    n.children_named(MAIN, "numFmt")
                        .filter_map(|f| {
                            Some((
                                f.attr(None, "numFmtId")?.trim().parse().ok()?,
                                f.attr(None, "formatCode").unwrap_or("").to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let xfs: Vec<i64> = root
                .child(MAIN, "cellXfs")
                .map(|x| {
                    x.children_named(MAIN, "xf")
                        .map(|xf| {
                            xf.attr(None, "numFmtId")
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap_or(0)
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !xfs.is_empty() {
                cell_style_count = xfs.len();
                for (idx, num_fmt_id) in xfs.iter().enumerate() {
                    let fmt: Option<String> = match custom.get(num_fmt_id) {
                        Some(f) => Some(f.clone()),
                        None => builtin_format(*num_fmt_id).map(str::to_string),
                    };
                    if is_date_format(fmt.as_deref()) {
                        date_formats.insert(idx as i64);
                    }
                    if is_timedelta_format(fmt.as_deref()) {
                        timedelta_formats.insert(idx as i64);
                    }
                }
            }
        }
        Ok(Workbook {
            members,
            sheets,
            shared_strings,
            date_formats,
            timedelta_formats,
            cell_style_count,
            date1904,
        })
    }

    /// `wb.sheetnames`.
    pub(crate) fn sheetnames(&self) -> Vec<String> {
        self.sheets.iter().map(|s| s.title.clone()).collect()
    }

    /// `[ws.title for ws in wb.worksheets]` (chartsheets excluded).
    pub(crate) fn worksheet_names(&self) -> Vec<String> {
        self.sheets
            .iter()
            .filter(|s| !s.chartsheet)
            .map(|s| s.title.clone())
            .collect()
    }

    fn entry(&self, title: &str) -> Option<&SheetEntry> {
        self.sheets.iter().find(|s| s.title == title)
    }

    /// `WorkSheetParser.parse_cell`.
    fn parse_cell(&self, el: &Element, row_counter: i64, col_counter: &mut i64) -> Result<ParsedCell, String> {
        let mut data_type = el.attr(None, "t").unwrap_or("n").to_string();
        let coordinate = el.attr(None, "r");
        let style_id: i64 = match el.attr(None, "s") {
            Some(s) if !s.is_empty() => s
                .trim()
                .parse()
                .map_err(|_| format!("invalid literal for int() with base 10: '{s}'"))?,
            _ => 0,
        };
        let raw: Option<String> = if data_type == "inlineStr" {
            None
        } else {
            el.child(MAIN, "v")
                .and_then(Element::text)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        let (row, column) = match coordinate.filter(|c| !c.is_empty()) {
            Some(c) => {
                let (r, col) = coordinate_to_tuple(c)?;
                *col_counter = col;
                (r, col)
            }
            None => {
                *col_counter += 1;
                (row_counter, *col_counter)
            }
        };
        let mut value: Option<PyVal> = None;
        if let Some(text) = raw {
            match data_type.as_str() {
                "n" => {
                    let number = cast_number(&text)?;
                    if self.date_formats.contains(&style_id) {
                        data_type = "d".to_string();
                        match from_excel(&number, self.date1904, self.timedelta_formats.contains(&style_id)) {
                            Ok(v) => value = Some(v),
                            Err(_) => {
                                data_type = "e".to_string();
                                value = Some(PyVal::Str("#VALUE!".to_string()));
                            }
                        }
                    } else {
                        value = Some(number);
                    }
                }
                "s" => {
                    let idx: usize = text
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid literal for int() with base 10: '{text}'"))?;
                    let s = self
                        .shared_strings
                        .get(idx)
                        .ok_or_else(|| "list index out of range".to_string())?;
                    value = Some(PyVal::Str(s.clone()));
                }
                "b" => {
                    let n: i64 = text
                        .trim()
                        .parse()
                        .map_err(|_| format!("invalid literal for int() with base 10: '{text}'"))?;
                    value = Some(PyVal::Bool(n != 0));
                }
                "str" => {
                    data_type = "s".to_string();
                    value = Some(PyVal::Str(text));
                }
                "d" => value = from_iso8601(&text)?,
                _ => value = Some(PyVal::Str(text)),
            }
        } else if data_type == "inlineStr"
            && let Some(child) = el.child(MAIN, "is")
        {
            data_type = "s".to_string();
            value = Some(PyVal::Str(text_content(child)));
        }
        Ok(ParsedCell {
            row,
            column,
            value,
            data_type,
            style_id,
        })
    }

    /// `WorkSheetParser.parse()`: `(row index, cells)` for each `<row>`.
    pub(crate) fn parse_rows(&self, title: &str) -> Result<Vec<(i64, Vec<ParsedCell>)>, String> {
        let entry = self
            .entry(title)
            .ok_or_else(|| key_error(&format!("Worksheet {title} does not exist.")))?;
        let blob = self
            .members
            .get(&entry.target)
            .ok_or_else(|| key_error(&format!("There is no item named '{}' in the archive", entry.target)))?;
        let root = dom::parse(blob)?;
        let mut rows = Vec::new();
        let Some(data) = root.child(MAIN, "sheetData") else {
            return Ok(rows);
        };
        let mut row_counter: i64 = 0;
        for row in data.children_named(MAIN, "row") {
            match row.attr(None, "r") {
                Some(r) => {
                    row_counter = match r.trim().parse::<i64>() {
                        Ok(v) => v,
                        Err(_) => {
                            let f = py_float(r)?;
                            if f.fract() == 0.0 {
                                f as i64
                            } else {
                                return Err(format!("{r} is not a valid row number"));
                            }
                        }
                    }
                }
                None => row_counter += 1,
            }
            let mut col_counter = 0;
            let mut cells = Vec::new();
            for c in row.elements() {
                cells.push(self.parse_cell(c, row_counter, &mut col_counter)?);
            }
            rows.push((row_counter, cells));
        }
        Ok(rows)
    }

    /// The merged ranges of a worksheet as `(min_row, min_col, max_row, max_col)`.
    fn merged_ranges(&self, title: &str) -> Result<Vec<(i64, i64, i64, i64)>, String> {
        let Some(entry) = self.entry(title) else {
            return Ok(Vec::new());
        };
        let Some(blob) = self.members.get(&entry.target) else {
            return Ok(Vec::new());
        };
        let root = dom::parse(blob)?;
        let mut out = Vec::new();
        if let Some(mc) = root.child(MAIN, "mergeCells") {
            for m in mc.children_named(MAIN, "mergeCell") {
                let Some(r) = m.attr(None, "ref") else { continue };
                let (a, b) = r.split_once(':').unwrap_or((r, r));
                let (r1, c1) = coordinate_to_tuple(a)?;
                let (r2, c2) = coordinate_to_tuple(b)?;
                out.push((r1.min(r2), c1.min(c2), r1.max(r2), c1.max(c2)));
            }
        }
        Ok(out)
    }

    /// `ReadOnlyWorksheet.rows` after `reset_dimensions()`: each row from column 1 to its
    /// last cell (`None` for absent cells), empty tuples for missing rows.
    pub(crate) fn read_only_rows(&self, title: &str) -> Result<Vec<Vec<Option<ParsedCell>>>, String> {
        let mut out = Vec::new();
        let mut counter: i64 = 1;
        for (idx, cells) in self.parse_rows(title)? {
            while counter < idx {
                counter += 1;
                out.push(Vec::new());
            }
            if counter <= idx {
                let row = if cells.is_empty() {
                    Vec::new()
                } else {
                    let max_col = cells.last().map(|c| c.column).unwrap_or(0);
                    let mut new_row: Vec<Option<ParsedCell>> = vec![None; max_col.max(0) as usize];
                    for cell in cells {
                        if 1 <= cell.column && cell.column <= max_col {
                            let slot = (cell.column - 1) as usize;
                            new_row[slot] = Some(cell);
                        }
                    }
                    new_row
                };
                counter += 1;
                out.push(row);
            }
        }
        Ok(out)
    }

    /// The normal-mode cell grid (`ws._cells`) with merged ranges applied: covered cells
    /// become empty `MergedCell`s and the top-left cell always exists.
    pub(crate) fn normal_cells(&self, title: &str) -> Result<NormalSheet, String> {
        let entry = self
            .entry(title)
            .ok_or_else(|| key_error(&format!("Worksheet {title} does not exist.")))?;
        if entry.chartsheet {
            return Err(NormalSheet::CHARTSHEET.to_string());
        }
        let mut cells: BTreeMap<(i64, i64), Option<PyVal>> = BTreeMap::new();
        for (_, row) in self.parse_rows(title)? {
            for cell in row {
                if cell.style_id < 0 || cell.style_id as usize >= self.cell_style_count {
                    return Err("list index out of range".to_string());
                }
                cells.insert((cell.row, cell.column), cell.value);
            }
        }
        for (r1, c1, r2, c2) in self.merged_ranges(title)? {
            for r in r1..=r2 {
                for c in c1..=c2 {
                    if (r, c) == (r1, c1) {
                        cells.entry((r, c)).or_insert(None);
                    } else {
                        cells.insert((r, c), None);
                    }
                }
            }
        }
        Ok(NormalSheet { cells })
    }
}

/// A normal-mode worksheet's cells.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalSheet {
    pub cells: BTreeMap<(i64, i64), Option<PyVal>>,
}

impl NormalSheet {
    /// The error of using a chartsheet as a worksheet.
    pub(crate) const CHARTSHEET: &'static str = "'Chartsheet' object has no attribute 'max_row'";

    /// `ws.max_row`.
    pub(crate) fn max_row(&self) -> i64 {
        self.cells.keys().map(|(r, _)| *r).max().unwrap_or(1)
    }

    /// `ws.max_column`.
    pub(crate) fn max_column(&self) -> i64 {
        self.cells.keys().map(|(_, c)| *c).max().unwrap_or(1)
    }

    /// `ws.cell(row, column).value`.
    pub(crate) fn value(&self, row: i64, column: i64) -> Option<&PyVal> {
        self.cells.get(&(row, column)).and_then(Option::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_format_detection_matches_openpyxl() {
        assert!(is_date_format(Some("mm-dd-yy")));
        assert!(is_date_format(Some("yyyy-mm-dd h:mm:ss")));
        assert!(!is_date_format(Some("0.00")));
        assert!(!is_date_format(Some("\"days\" 0")));
        assert!(!is_date_format(Some("[Red]0.00")));
        assert!(is_date_format(Some("[h]:mm:ss")));
        assert!(!is_date_format(Some("0.00_d")));
        assert!(!is_date_format(None));
        assert!(is_timedelta_format(Some("[h]:mm:ss")));
        assert!(!is_timedelta_format(Some("h:mm:ss")));
    }

    #[test]
    fn from_excel_follows_openpyxl() {
        let s = |v: PyVal| from_excel(&v, false, false).unwrap().py_str();
        assert_eq!(s(PyVal::Int(45306)), "2024-01-15 00:00:00");
        assert_eq!(s(PyVal::Float(45306.5)), "2024-01-15 12:00:00");
        assert_eq!(s(PyVal::Float(0.5)), "12:00:00");
        assert_eq!(s(PyVal::Int(1)), "1900-01-01 00:00:00");
        assert_eq!(s(PyVal::Int(61)), "1900-03-01 00:00:00");
        let td = from_excel(&PyVal::Float(1.5), false, true).unwrap();
        assert_eq!(td.py_str(), "1 day, 12:00:00");
        assert_eq!(pandas_timedelta_str(36 * 3_600_000_000), "1 days 12:00:00");
        assert_eq!(timedelta_str(-3_600_000_000), "-1 day, 23:00:00");
    }

    #[test]
    fn coordinates_and_numbers() {
        assert_eq!(coordinate_to_tuple("B7"), Ok((7, 2)));
        assert_eq!(coordinate_to_tuple("$AA$10"), Ok((10, 27)));
        assert!(coordinate_to_tuple("A0").is_err());
        assert_eq!(cast_number("12"), Ok(PyVal::Int(12)));
        assert_eq!(cast_number("1.0"), Ok(PyVal::Float(1.0)));
        assert_eq!(cast_number("1E3"), Ok(PyVal::Float(1000.0)));
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(3.5), 4.0);
    }
}
