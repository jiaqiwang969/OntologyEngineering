//! `pandas.read_excel(...)` (pandas 3.0.6, openpyxl engine) followed by
//! `df.fillna("").to_dict("records")`, as `ExcelParser._parse_with_pandas` uses it.
//!
//! The pipeline follows pandas: `OpenpyxlReader.get_sheet_data` (cell conversion, trailing
//! empty cells/rows trimmed, rows padded to the widest row, reading stopped after the rows
//! `nrows` needs), the Python parser's header handling (`skiprows`, `header`, `Unnamed: i`
//! names and `.1` de-duplication), per-column type inference (`maybe_convert_numeric`,
//! `sanitize_objects`, `maybe_convert_bool`, datetime/timedelta/string inference), then
//! `fillna("")` (which leaves `NaT` in datetime columns) and the records.
//!
//! Values are given in their JSON form: numbers, booleans and strings as such; `Timestamp`,
//! `datetime`, `date`, `time` and `timedelta` values as their `str()`; non-finite floats as
//! `null`.

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::xlsx::{PyVal, Workbook, datetime_str, pandas_timedelta_str, time_str, timedelta_str};

/// pandas' default NA strings (`STR_NA_VALUES`).
const NA_STRINGS: &[&str] = &[
    "", "#N/A", "#N/A N/A", "#NA", "-1.#IND", "-1.#QNAN", "-NaN", "-nan", "1.#IND", "1.#QNAN",
    "<NA>", "N/A", "NA", "NULL", "NaN", "None", "n/a", "nan", "null",
];

/// A parsed sheet: column names and the records' values (row-major).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frame {
    pub columns: Vec<PyVal>,
    pub rows: Vec<Vec<Value>>,
}

impl Frame {
    fn empty() -> Frame {
        Frame {
            columns: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// `list(df.columns)` in JSON form.
    pub(crate) fn headers(&self) -> Vec<Value> {
        self.columns.iter().map(name_json).collect()
    }

    /// `df.to_dict("records")` with `str(column)` keys.
    pub(crate) fn records(&self) -> Vec<Map<String, Value>> {
        self.rows
            .iter()
            .map(|row| {
                let mut m = Map::new();
                for (name, v) in self.columns.iter().zip(row) {
                    m.insert(name.py_str(), v.clone());
                }
                m
            })
            .collect()
    }
}

fn float_json(f: f64) -> Value {
    serde_json::Number::from_f64(f)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn int_json(i: i128) -> Value {
    if let Ok(v) = i64::try_from(i) {
        Value::from(v)
    } else if let Ok(v) = u64::try_from(i) {
        Value::from(v)
    } else {
        float_json(i as f64)
    }
}

fn name_json(v: &PyVal) -> Value {
    match v {
        PyVal::Str(s) => Value::String(s.clone()),
        PyVal::Int(i) => int_json(*i),
        PyVal::Float(f) => float_json(*f),
        PyVal::Bool(b) => Value::Bool(*b),
        PyVal::Nan => Value::Null,
        other => Value::String(other.py_str()),
    }
}

/// The JSON form of an object-column value.
fn object_json(v: &PyVal) -> Value {
    match v {
        PyVal::Nan => Value::String(String::new()),
        PyVal::DateTime(dt) => Value::String(datetime_str(dt)),
        PyVal::Time(t) => Value::String(time_str(t)),
        PyVal::TimeDelta(us) => Value::String(timedelta_str(*us)),
        other => name_json(other),
    }
}

/// Python equality/hash class of a value (`1 == 1.0 == True`), used for dictionary keys
/// and `sanitize_objects`' memo.
fn eq_key(v: &PyVal) -> String {
    match v {
        PyVal::Int(i) => format!("n{i}"),
        PyVal::Bool(b) => format!("n{}", *b as i32),
        PyVal::Float(f) => {
            if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e30 {
                format!("n{}", *f as i128)
            } else {
                format!("f{}", f.to_bits())
            }
        }
        PyVal::Nan => "nan".to_string(),
        PyVal::Str(s) => format!("s{s}"),
        PyVal::DateTime(d) => format!("t{d}"),
        PyVal::Date(d) => format!("d{d}"),
        PyVal::Time(t) => format!("h{t}"),
        PyVal::TimeDelta(us) => format!("D{us}"),
    }
}

/// `OpenpyxlReader._convert_cell`.
fn convert_cell(cell: Option<&super::xlsx::ParsedCell>) -> Result<PyVal, String> {
    let Some(cell) = cell else {
        return Ok(PyVal::Str(String::new()));
    };
    let Some(value) = &cell.value else {
        return Ok(PyVal::Str(String::new()));
    };
    if cell.data_type == "e" {
        return Ok(PyVal::Nan);
    }
    if cell.data_type == "n" {
        return match value {
            PyVal::Int(i) => Ok(PyVal::Int(*i)),
            PyVal::Float(f) => {
                if f.is_nan() {
                    Err("cannot convert float NaN to integer".to_string())
                } else if f.is_infinite() {
                    Err("cannot convert float infinity to integer".to_string())
                } else if f.fract() == 0.0 && f.abs() < 1.7e38 {
                    Ok(PyVal::Int(*f as i128))
                } else {
                    Ok(PyVal::Float(*f))
                }
            }
            other => Ok(other.clone()),
        };
    }
    Ok(value.clone())
}

/// `OpenpyxlReader.get_sheet_data(sheet, file_rows_needed)`.
pub(crate) fn get_sheet_data(
    wb: &Workbook,
    title: &str,
    file_rows_needed: Option<i64>,
) -> Result<Vec<Vec<PyVal>>, String> {
    let mut data: Vec<Vec<PyVal>> = Vec::new();
    let mut last_row_with_data: i64 = -1;
    for (row_number, row) in wb.read_only_rows(title)?.iter().enumerate() {
        let mut converted = row
            .iter()
            .map(|c| convert_cell(c.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        while matches!(converted.last(), Some(PyVal::Str(s)) if s.is_empty()) {
            converted.pop();
        }
        if !converted.is_empty() {
            last_row_with_data = row_number as i64;
        }
        data.push(converted);
        if let Some(n) = file_rows_needed
            && data.len() as i64 >= n
        {
            break;
        }
    }
    data.truncate((last_row_with_data + 1).max(0) as usize);
    if let Some(max_width) = data.iter().map(Vec::len).max() {
        for row in &mut data {
            while row.len() < max_width {
                row.push(PyVal::Str(String::new()));
            }
        }
    }
    Ok(data)
}

/// `read_excel` keyword arguments the parser passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadArgs {
    /// `header` (`None` for `header=None`).
    pub header: Option<i64>,
    /// `skiprows` as an integer (`None` for `skiprows=None`).
    pub skiprows: Option<i64>,
    /// `nrows`.
    pub nrows: Option<i64>,
}

impl ReadArgs {
    /// `BaseExcelReader._calc_rows`.
    pub(crate) fn file_rows_needed(&self) -> Option<i64> {
        let nrows = self.nrows?;
        let header_rows = match self.header {
            None => 1,
            Some(h) => 1 + h,
        };
        Some(header_rows + nrows + self.skiprows.unwrap_or(0))
    }
}

/// The outcome of pandas' header handling.
enum Parsed {
    /// `EmptyDataError` (or no data): an empty DataFrame.
    Empty,
    Frame(Frame),
}

struct TextParser<'a> {
    data: &'a [Vec<PyVal>],
    pos: usize,
    line_pos: i64,
    buf: Vec<Vec<PyVal>>,
    skiprows: i64,
}

impl TextParser<'_> {
    fn skip(&self, i: usize) -> bool {
        (i as i64) < self.skiprows
    }

    /// `_next_line` (list data, `skip_blank_lines=False`).
    fn next_line(&mut self) -> Option<Vec<PyVal>> {
        while self.skip(self.pos) {
            if self.pos >= self.data.len() {
                break;
            }
            self.pos += 1;
        }
        let mut line = self.data.get(self.pos)?.clone();
        self.pos += 1;
        if self.pos == 1 {
            check_for_bom(&mut line);
        }
        self.line_pos += 1;
        self.buf.push(line.clone());
        Some(line)
    }

    fn buffered_line(&mut self) -> Option<Vec<PyVal>> {
        match self.buf.first() {
            Some(l) => Some(l.clone()),
            None => self.next_line(),
        }
    }
}

/// `_check_for_bom` on the first line.
fn check_for_bom(line: &mut [PyVal]) {
    if let Some(PyVal::Str(first)) = line.first_mut()
        && let Some(rest) = first.strip_prefix('\u{feff}')
    {
        let rest = rest.to_string();
        *first = if let Some(quoted) = rest.strip_prefix('"') {
            match quoted.find('"') {
                Some(end) => format!("{}{}", &quoted[..end], &quoted[end + 1..]),
                None => rest.clone(),
            }
        } else {
            rest
        };
    }
}

/// Header de-duplication of `_infer_columns` (unnamed columns last, `name.N` suffixes).
fn dedup_columns(columns: &mut [PyVal], unnamed: &[usize]) {
    let mut counts: HashMap<String, i64> = HashMap::new();
    let order: Vec<usize> = (0..columns.len())
        .filter(|i| !unnamed.contains(i))
        .chain(unnamed.iter().copied())
        .collect();
    for i in order {
        let old_col = columns[i].clone();
        let mut col = old_col.clone();
        let mut cur_count = *counts.get(&eq_key(&col)).unwrap_or(&0);
        if cur_count > 0 {
            while cur_count > 0 {
                counts.insert(eq_key(&old_col), cur_count + 1);
                col = PyVal::Str(format!("{}.{}", old_col.py_str(), cur_count));
                let key = eq_key(&col);
                if columns.iter().any(|c| eq_key(c) == key) {
                    cur_count += 1;
                } else {
                    cur_count = *counts.get(&key).unwrap_or(&0);
                }
            }
        }
        counts.insert(eq_key(&col), cur_count + 1);
        columns[i] = col;
    }
}

/// `_infer_columns` + `_get_index_name` + `read(nrows)` on the sheet data.
fn text_parser(data: &[Vec<PyVal>], args: ReadArgs) -> Result<Parsed, String> {
    let mut p = TextParser {
        data,
        pos: 0,
        line_pos: 0,
        buf: Vec::new(),
        skiprows: args.skiprows.unwrap_or(0),
    };
    let columns: Vec<PyVal>;
    let num_original_columns: usize;
    match args.header {
        Some(h) => {
            let mut line = p.buffered_line();
            while line.is_some() && p.line_pos <= h {
                line = p.next_line();
            }
            let Some(line) = line else {
                if 0 < p.line_pos && p.line_pos <= h {
                    return Err(format!(
                        "Passed header=[{h}], len of 1, but only {} lines in file",
                        p.line_pos
                    ));
                }
                return Ok(Parsed::Empty);
            };
            let mut this_columns = Vec::new();
            let mut unnamed = Vec::new();
            for (i, c) in line.iter().enumerate() {
                if matches!(c, PyVal::Str(s) if s.is_empty()) {
                    unnamed.push(i);
                    this_columns.push(PyVal::Str(format!("Unnamed: {i}")));
                } else {
                    this_columns.push(c.clone());
                }
            }
            dedup_columns(&mut this_columns, &unnamed);
            num_original_columns = this_columns.len();
            columns = this_columns;
            p.buf.clear();
        }
        None => {
            let Some(line) = p.buffered_line() else {
                return Ok(Parsed::Empty);
            };
            num_original_columns = line.len();
            columns = (0..line.len() as i128).map(PyVal::Int).collect();
        }
    }
    // `_get_index_name` reads up to two lines ahead into the buffer.
    if args.header.is_some() {
        let _ = p.next_line();
    }
    let _ = p.next_line();
    // `read(nrows)` -> `_get_lines(nrows)`.
    let mut lines: Vec<Vec<PyVal>> = std::mem::take(&mut p.buf);
    let wanted = args.nrows;
    let content: Vec<Vec<PyVal>> = match wanted {
        Some(n) if lines.len() as i64 >= n => lines.drain(..n.max(0) as usize).collect(),
        _ => {
            let rows = wanted.map(|n| n - lines.len() as i64);
            if p.pos <= data.len() {
                let end = match rows {
                    None => data.len(),
                    Some(r) => (p.pos + r.max(0) as usize).min(data.len()),
                };
                for (i, row) in data[p.pos.min(data.len())..end].iter().enumerate() {
                    if !p.skip(i + p.pos) {
                        lines.push(row.clone());
                    }
                }
            }
            lines
        }
    };
    if content.is_empty() {
        return Ok(Parsed::Frame(Frame {
            columns,
            rows: Vec::new(),
        }));
    }
    // `_rows_to_cols`: pad short rows (with None, which pandas treats as NA).
    let width = num_original_columns.max(content.iter().map(Vec::len).max().unwrap_or(0));
    let mut cols: Vec<Vec<PyVal>> = vec![Vec::with_capacity(content.len()); width];
    for row in &content {
        for (j, col) in cols.iter_mut().enumerate() {
            col.push(row.get(j).cloned().unwrap_or(PyVal::Nan));
        }
    }
    cols.truncate(columns.len());
    let converted: Vec<Vec<Value>> = cols.into_iter().map(convert_column).collect();
    let rows = (0..content.len())
        .map(|r| converted.iter().map(|c| c[r].clone()).collect())
        .collect();
    Ok(Parsed::Frame(Frame { columns, rows }))
}

/// pandas' `floatify` grammar for numeric strings: `Some((value, is_integer_literal))`.
fn floatify(s: &str) -> Option<(f64, bool)> {
    let t = s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'));
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let neg = t.starts_with('-');
    let lower = body.to_ascii_lowercase();
    if lower == "inf" || lower == "infinity" {
        return Some((if neg { f64::NEG_INFINITY } else { f64::INFINITY }, false));
    }
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    let mut is_int = true;
    if i < bytes.len() && bytes[i] == b'.' {
        is_int = false;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        is_int = false;
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    if i != bytes.len() {
        return None;
    }
    let value: f64 = t.parse().ok().or_else(|| body.parse::<f64>().ok().map(|v| if neg { -v } else { v }))?;
    Some((value, is_int))
}

/// Numeric conversion (`maybe_convert_numeric`) of an object column; `None` when pandas
/// raises and falls back to object handling.
fn convert_numeric(values: &[PyVal]) -> Option<Vec<Value>> {
    #[derive(Clone, Copy)]
    enum N {
        Int(i128),
        Float(f64),
        Bool(bool),
        Null,
    }
    let mut out = Vec::with_capacity(values.len());
    let (mut seen_float, mut seen_int, mut seen_bool, mut seen_null) = (false, false, false, false);
    for v in values {
        let n = match v {
            PyVal::Str(s) if NA_STRINGS.contains(&s.as_str()) => N::Null,
            PyVal::Str(s) => {
                let (f, is_int) = floatify(s)?;
                if is_int {
                    let t = s.trim().trim_start_matches('+');
                    let i: i128 = t.parse().ok()?;
                    N::Int(i)
                } else {
                    N::Float(f)
                }
            }
            PyVal::Int(i) => N::Int(*i),
            PyVal::Float(f) if f.is_nan() => N::Null,
            PyVal::Float(f) => N::Float(*f),
            PyVal::Nan => N::Null,
            PyVal::Bool(b) => N::Bool(*b),
            _ => return None,
        };
        match n {
            N::Int(i) => {
                if i < i64::MIN as i128 || i > u64::MAX as i128 {
                    return None;
                }
                seen_int = true;
            }
            N::Float(_) => seen_float = true,
            N::Bool(_) => seen_bool = true,
            N::Null => seen_null = true,
        }
        out.push(n);
    }
    let as_f64 = |n: &N| match n {
        N::Int(i) => *i as f64,
        N::Float(f) => *f,
        N::Bool(b) => *b as i32 as f64,
        N::Null => f64::NAN,
    };
    Some(if seen_float || seen_null {
        out.iter()
            .map(|n| {
                let f = as_f64(n);
                if f.is_nan() {
                    Value::String(String::new())
                } else {
                    float_json(f)
                }
            })
            .collect()
    } else if seen_int {
        out.iter()
            .map(|n| match n {
                N::Int(i) => int_json(*i),
                N::Bool(b) => Value::from(*b as i64),
                _ => Value::Null,
            })
            .collect()
    } else if seen_bool {
        out.iter()
            .map(|n| match n {
                N::Bool(b) => Value::Bool(*b),
                _ => Value::Null,
            })
            .collect()
    } else {
        Vec::new()
    })
}

/// One column: inference, `fillna("")`, JSON values.
fn convert_column(values: Vec<PyVal>) -> Vec<Value> {
    if let Some(numeric) = convert_numeric(&values) {
        return numeric;
    }
    // `sanitize_objects`: NA strings become NaN; equal values share the first instance.
    let mut memo: HashMap<String, PyVal> = HashMap::new();
    let sanitized: Vec<PyVal> = values
        .into_iter()
        .map(|v| match &v {
            PyVal::Str(s) if NA_STRINGS.contains(&s.as_str()) => PyVal::Nan,
            PyVal::Nan => PyVal::Nan,
            _ => memo.entry(eq_key(&v)).or_insert_with(|| v.clone()).clone(),
        })
        .collect();
    // `maybe_convert_bool`, unless the first value is an int (or bool).
    let first_is_int = matches!(sanitized.first(), Some(PyVal::Int(_) | PyVal::Bool(_)));
    if !first_is_int {
        let mut bools = Vec::with_capacity(sanitized.len());
        let mut ok = true;
        for v in &sanitized {
            match v {
                PyVal::Bool(b) => bools.push(Some(*b)),
                PyVal::Str(s) if matches!(s.as_str(), "True" | "TRUE" | "true") => bools.push(Some(true)),
                PyVal::Str(s) if matches!(s.as_str(), "False" | "FALSE" | "false") => bools.push(Some(false)),
                PyVal::Nan => bools.push(None),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return bools
                .into_iter()
                .map(|b| match b {
                    Some(b) => Value::Bool(b),
                    None => Value::String(String::new()),
                })
                .collect();
        }
    }
    // DataFrame construction: datetime / timedelta inference for object columns.
    let non_na: Vec<&PyVal> = sanitized.iter().filter(|v| !matches!(v, PyVal::Nan)).collect();
    if !non_na.is_empty() && non_na.iter().all(|v| matches!(v, PyVal::DateTime(_))) {
        return sanitized
            .iter()
            .map(|v| match v {
                PyVal::DateTime(dt) => Value::String(datetime_str(dt)),
                _ => Value::String("NaT".to_string()),
            })
            .collect();
    }
    if !non_na.is_empty() && non_na.iter().all(|v| matches!(v, PyVal::TimeDelta(_))) {
        return sanitized
            .iter()
            .map(|v| match v {
                PyVal::TimeDelta(us) => Value::String(pandas_timedelta_str(*us)),
                _ => Value::String("NaT".to_string()),
            })
            .collect();
    }
    sanitized.iter().map(object_json).collect()
}

/// Read one sheet as `read_excel(..., sheet_name=title, header, skiprows, nrows)`.
/// Errors raised while parsing carry pandas' ` (sheet: <name>)` suffix.
pub(crate) fn read_sheet(wb: &Workbook, title: &str, args: ReadArgs) -> Result<Frame, String> {
    let data = get_sheet_data(wb, title, args.file_rows_needed())?;
    if data.is_empty() {
        return Ok(Frame::empty());
    }
    match text_parser(&data, args) {
        Ok(Parsed::Empty) => Ok(Frame::empty()),
        Ok(Parsed::Frame(f)) => Ok(f),
        Err(e) => Err(format!("{e} (sheet: {title})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floatify_follows_pandas() {
        assert_eq!(floatify(" 12 "), Some((12.0, true)));
        assert_eq!(floatify("+5"), Some((5.0, true)));
        assert_eq!(floatify(".5"), Some((0.5, false)));
        assert_eq!(floatify("5."), Some((5.0, false)));
        assert_eq!(floatify("1E+2"), Some((100.0, false)));
        assert_eq!(floatify("-Infinity").map(|x| x.0), Some(f64::NEG_INFINITY));
        for bad in ["1e", "1,000", "0x10", "1_000", "NAN", "nan ", "  ", "1 2", "\u{661}"] {
            assert_eq!(floatify(bad), None, "{bad}");
        }
    }

    #[test]
    fn dedup_matches_pandas() {
        let mut cols: Vec<PyVal> = ["a", "a", "b", "a"].iter().map(|s| PyVal::Str(s.to_string())).collect();
        dedup_columns(&mut cols, &[]);
        let names: Vec<String> = cols.iter().map(PyVal::py_str).collect();
        assert_eq!(names, ["a", "a.1", "b", "a.2"]);
    }
}
