//! Excel spreadsheet parsing (`semantica/parse/excel_parser.py`).
//!
//! [`ExcelParser`] reads `.xlsx`/`.xlsm` workbooks (and `.xls`-named files holding xlsx
//! content) either the way the original's default `"pandas"` engine does
//! (`pandas.read_excel` on openpyxl's read-only workbook, then `fillna("")` and
//! `to_dict("records")`) or, for any other `engine` option, the way its openpyxl path does
//! (`load_workbook(data_only=True)` and `str()` of every cell value).
//!
//! The workbook is read from the zip package with the port's own openpyxl model
//! (`crate::ooxml::xlsx`); pandas' parsing and type inference are reproduced in
//! `crate::ooxml::pandas`. Dates, times and timedeltas appear as their `str()` text
//! (`"2024-01-15 00:00:00"`), the JSON form of pandas' `Timestamp` objects.

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{Options, PResult, py_exists, py_name, py_path_str, py_read_bytes, py_suffix};
use crate::ooxml::pandas::{self, ReadArgs};
use crate::ooxml::xlsx::{NOT_A_ZIP, NormalSheet, PyVal, Workbook};

/// `ExcelSheet` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExcelSheet {
    pub name: String,
    /// Rows as `{column: value}` (column names as `str(name)`).
    pub data: Vec<Map<String, Value>>,
    pub row_count: usize,
    pub column_count: usize,
    /// `list(df.columns)`: strings, or the numbers/booleans pandas takes from header cells
    /// (column positions with `header_row=None`).
    pub headers: Vec<Value>,
}

/// `ExcelData` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExcelData {
    pub sheet_names: Vec<String>,
    pub sheets: IndexMap<String, ExcelSheet>,
    pub metadata: Map<String, Value>,
}

/// `Union[ExcelData, ExcelSheet]`: what [`ExcelParser::parse`] returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExcelResult {
    /// All sheets (no `sheet_name`).
    Data(ExcelData),
    /// One sheet.
    Sheet(ExcelSheet),
}

impl ExcelResult {
    /// The result as `ExcelData`, if it is one.
    pub fn as_data(&self) -> Option<&ExcelData> {
        match self {
            ExcelResult::Data(d) => Some(d),
            ExcelResult::Sheet(_) => None,
        }
    }

    /// The result as `ExcelSheet`, if it is one.
    pub fn as_sheet(&self) -> Option<&ExcelSheet> {
        match self {
            ExcelResult::Sheet(s) => Some(s),
            ExcelResult::Data(_) => None,
        }
    }
}

/// `ExcelParser(**config)`: Excel document parser.
#[derive(Debug, Clone)]
pub struct ExcelParser {
    /// `self.logger` (`semantica.excel_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for ExcelParser {
    fn default() -> Self {
        ExcelParser::new(Options::new())
    }
}

/// An exception raised by pandas/openpyxl, by `str(e)`.
type LibResult<T> = Result<T, String>;

const XLS_SIGNATURES: &[&[u8]] = &[
    b"\x09\x00\x04\x00\x07\x00\x10\x00",
    b"\x09\x02\x06\x00\x00\x00\x10\x00",
    b"\x09\x04\x06\x00\x00\x00\x10\x00",
    b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1",
];

/// `pd.ExcelFile(path)`: detect the format from the content (`inspect_excel_format`) and
/// open xlsx workbooks; other formats fail as their (missing) engines do.
fn pandas_excel_file(path: &Path) -> LibResult<Workbook> {
    let bytes = py_read_bytes(path).map_err(|e| e.to_string())?;
    if XLS_SIGNATURES.iter().any(|sig| bytes.starts_with(sig)) {
        return Err("`Import xlrd` failed. Install xlrd >= 2.0.1 for xls Excel support Use pip or conda to install the xlrd package.".to_string());
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(
            "Excel file format cannot be determined, you must specify an engine manually."
                .to_string(),
        );
    }
    let names: Vec<String> = {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes))
            .map_err(|_| NOT_A_ZIP.to_string())?;
        archive
            .file_names()
            .map(|n| n.replace('\\', "/").to_lowercase())
            .collect()
    };
    if names.iter().any(|n| n == "xl/workbook.xml") {
        return Workbook::load(&crate::common::py_fs_path(path));
    }
    if names.iter().any(|n| n == "xl/workbook.bin") {
        return Err("Missing optional dependency 'pyxlsb'.  Use pip or conda to install pyxlsb.".to_string());
    }
    if names.iter().any(|n| n == "content.xml") {
        return Err("Missing optional dependency 'odfpy'.  Use pip or conda to install odfpy.".to_string());
    }
    Err("No such keys(s): 'io.excel.zip.reader'".to_string())
}

/// `validate_header_arg(header)` -> the header row (`None` for `header=None`).
fn header_arg(value: Option<&Value>) -> LibResult<Option<i64>> {
    match value {
        None => Ok(Some(0)),
        Some(Value::Null) => Ok(None),
        Some(Value::Bool(_)) => Err("Passing a bool to header is invalid. Use header=None for no header or header=int or list-like of ints to specify the row(s) making up the column names".to_string()),
        Some(Value::Number(n)) if n.as_i64().is_some() => {
            let h = n.as_i64().unwrap_or(0);
            if h < 0 {
                Err("Passing negative integer to header is invalid. For no header, use header=None instead".to_string())
            } else {
                Ok(Some(h))
            }
        }
        Some(_) => Err("header must be integer or list of integers".to_string()),
    }
}

/// `skiprows` (an integer or `None`).
fn skiprows_arg(value: Option<&Value>) -> LibResult<Option<i64>> {
    match value {
        None => Ok(Some(0)),
        Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) if n.as_i64().is_some() => Ok(n.as_i64()),
        Some(Value::Bool(b)) => Ok(Some(*b as i64)),
        Some(other) => Err(format!(
            "skiprows of type {} is not supported by the port",
            sem_utils::pycompat::fmt::type_name(other)
        )),
    }
}

/// `validate_integer("nrows", nrows)`.
fn nrows_arg(value: Option<&Value>) -> LibResult<Option<i64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let v = if let Some(i) = n.as_i64() {
                i
            } else {
                let f = n.as_f64().unwrap_or(f64::NAN);
                if f.fract() != 0.0 || !f.is_finite() {
                    return Err("'nrows' must be an integer >=0".to_string());
                }
                f as i64
            };
            if v < 0 {
                Err("'nrows' must be an integer >=0".to_string())
            } else {
                Ok(Some(v))
            }
        }
        Some(_) => Err("'nrows' must be an integer >=0".to_string()),
    }
}

fn sheet_from_frame(name: &str, frame: &pandas::Frame) -> ExcelSheet {
    let data = frame.records();
    ExcelSheet {
        name: name.to_string(),
        row_count: data.len(),
        data,
        column_count: frame.columns.len(),
        headers: frame.headers(),
    }
}

/// `_parse_with_pandas(file_path, sheet_name, **options)`.
fn parse_with_pandas(path: &Path, sheet_name: Option<&str>, options: &Options) -> LibResult<ExcelResult> {
    let read_args = || -> LibResult<ReadArgs> {
        let header = header_arg(options.get("header_row"))?;
        let nrows = nrows_arg(options.get("max_rows"))?;
        let skiprows = skiprows_arg(options.get("skip_rows"))?;
        Ok(ReadArgs {
            header,
            skiprows,
            nrows,
        })
    };
    let wb = pandas_excel_file(path)?;
    match sheet_name {
        Some(name) => {
            let args = read_args()?;
            if !wb.worksheet_names().iter().any(|n| n == name) {
                return Err(format!("Worksheet named '{name}' not found"));
            }
            let frame = pandas::read_sheet(&wb, name, args)?;
            Ok(ExcelResult::Sheet(sheet_from_frame(name, &frame)))
        }
        None => {
            let names = wb.worksheet_names();
            let mut sheets = IndexMap::new();
            for name in &names {
                let args = read_args()?;
                let frame = pandas::read_sheet(&wb, name, args)?;
                sheets.insert(name.clone(), sheet_from_frame(name, &frame));
            }
            let mut metadata = Map::new();
            metadata.insert("file_path".to_string(), Value::String(py_path_str(path)));
            metadata.insert("total_sheets".to_string(), Value::from(names.len()));
            Ok(ExcelResult::Data(ExcelData {
                sheet_names: names,
                sheets,
                metadata,
            }))
        }
    }
}

/// openpyxl's `_validate_archive` extension check, then `load_workbook(data_only=True)`.
fn openpyxl_load(path: &Path) -> LibResult<Workbook> {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let name = crate::common::py_name(&abs);
    let ext = match name.rfind('.') {
        Some(i) if !name[..i].trim_start_matches('.').is_empty() => name[i..].to_lowercase(),
        _ => String::new(),
    };
    if ![".xlsx", ".xlsm", ".xltx", ".xltm"].contains(&ext.as_str()) {
        return Err(match ext.as_str() {
            ".xls" => "openpyxl does not support the old .xls file format, please use xlrd to read this file, or convert it to the more recent .xlsx file format.".to_string(),
            ".xlsb" => "openpyxl does not support binary format .xlsb, please convert this file to .xlsx format if you want to open it with openpyxl".to_string(),
            other => format!("openpyxl does not support {other} file format, please check you can open it with Excel first. Supported formats are: .xlsx,.xlsm,.xltx,.xltm"),
        });
    }
    let wb = Workbook::load(&crate::common::py_fs_path(path))?;
    // Normal mode parses every worksheet while loading.
    for sheet in &wb.sheets {
        if !sheet.chartsheet {
            wb.normal_cells(&sheet.title)?;
        }
    }
    Ok(wb)
}

/// A Python integer option of the openpyxl path, with Python's errors for `None`.
fn int_option(value: Option<&Value>, default: Option<i64>) -> LibResult<Option<i64>> {
    match value {
        None => Ok(default),
        Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b as i64)),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| "'float' object cannot be interpreted as an integer".to_string()),
        Some(Value::String(_)) => Err("can only concatenate str (not \"int\") to str".to_string()),
        Some(_) => Err("unsupported operand type(s)".to_string()),
    }
}

/// `ws.iter_rows(min_row, max_row, values_only=True)` (Python `or` defaults).
fn iter_rows(ws: &NormalSheet, min_row: i64, max_row: i64) -> LibResult<Vec<Vec<Option<PyVal>>>> {
    let min_row = if min_row == 0 { 1 } else { min_row };
    let max_row = if max_row == 0 { ws.max_row() } else { max_row };
    let max_col = ws.max_column();
    let mut out = Vec::new();
    for row in min_row..=max_row {
        if row < 1 {
            return Err("Row or column values must be at least 1".to_string());
        }
        out.push((1..=max_col).map(|c| ws.value(row, c).cloned()).collect());
    }
    Ok(out)
}

/// `_parse_sheet(worksheet, header_row, skip_rows, max_rows)` of the openpyxl path.
fn parse_openpyxl_sheet(title: &str, ws: &NormalSheet, options: &Options) -> LibResult<ExcelSheet> {
    let header_row = match options.get("header_row") {
        None => 0,
        Some(Value::Number(n)) if n.as_i64().is_some() => n.as_i64().unwrap_or(0),
        Some(Value::Bool(b)) => *b as i64,
        Some(other) => {
            return Err(format!(
                "'>' not supported between instances of 'int' and '{}'",
                sem_utils::pycompat::fmt::type_name(other)
            ));
        }
    };
    let skip_rows = int_option(options.get("skip_rows"), Some(0))?.ok_or_else(|| {
        "unsupported operand type(s) for +: 'int' and 'NoneType'".to_string()
    })?;
    let max_rows = int_option(options.get("max_rows"), None)?;
    let mut headers: Vec<String> = Vec::new();
    if ws.max_row() > header_row {
        let rows = iter_rows(ws, header_row + 1, header_row + 1)?;
        let first = rows.into_iter().next().ok_or("list index out of range")?;
        headers = first
            .iter()
            .enumerate()
            .map(|(i, c)| match c {
                Some(v) => v.py_str(),
                None => format!("Column_{}", i + 1),
            })
            .collect();
    }
    let start_row = if headers.is_empty() {
        skip_rows + 1
    } else {
        header_row + skip_rows + 2
    };
    let end_row = match max_rows {
        Some(m) if m != 0 => start_row + m,
        _ => ws.max_row() + 1,
    };
    let mut data = Vec::new();
    for row in iter_rows(ws, start_row, end_row - 1)? {
        let mut m = Map::new();
        for (idx, value) in row.iter().enumerate() {
            let header = headers
                .get(idx)
                .cloned()
                .unwrap_or_else(|| format!("Column_{}", idx + 1));
            m.insert(
                header,
                Value::String(value.as_ref().map(PyVal::py_str).unwrap_or_default()),
            );
        }
        data.push(m);
    }
    Ok(ExcelSheet {
        name: title.to_string(),
        row_count: data.len(),
        data,
        column_count: headers.len(),
        headers: headers.into_iter().map(Value::String).collect(),
    })
}

/// `_parse_with_openpyxl(file_path, sheet_name, **options)`.
fn parse_with_openpyxl(path: &Path, sheet_name: Option<&str>, options: &Options) -> LibResult<ExcelResult> {
    let wb = openpyxl_load(path)?;
    match sheet_name {
        Some(name) => {
            let ws = wb.normal_cells(name)?;
            Ok(ExcelResult::Sheet(parse_openpyxl_sheet(name, &ws, options)?))
        }
        None => {
            let names = wb.sheetnames();
            let mut sheets = IndexMap::new();
            for name in &names {
                let ws = wb.normal_cells(name)?;
                sheets.insert(name.clone(), parse_openpyxl_sheet(name, &ws, options)?);
            }
            let mut metadata = Map::new();
            metadata.insert("file_path".to_string(), Value::String(py_path_str(path)));
            metadata.insert("total_sheets".to_string(), Value::from(names.len()));
            Ok(ExcelResult::Data(ExcelData {
                sheet_names: names,
                sheets,
                metadata,
            }))
        }
    }
}

impl ExcelParser {
    /// `ExcelParser(**config)`.
    pub fn new(config: Options) -> Self {
        ExcelParser {
            logger: get_logger("excel_parser"),
            config,
            progress_tracker: get_progress_tracker(),
        }
    }

    /// `parse(file_path, sheet_name=None, **options)`: all sheets ([`ExcelResult::Data`])
    /// or one sheet ([`ExcelResult::Sheet`]). Options: `header_row` (default 0),
    /// `skip_rows` (default 0), `max_rows`, `engine` (`"pandas"` by default; any other
    /// value uses the openpyxl path).
    pub fn parse(
        &self,
        file_path: impl AsRef<Path>,
        sheet_name: Option<&str>,
        options: &Options,
    ) -> PResult<ExcelResult> {
        let path = file_path.as_ref();
        let display = py_path_str(path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&display),
            Some("parse"),
            Some("ExcelParser"),
            &format!("Excel: {}", py_name(path)),
            None,
        );
        let result = (|| -> PResult<ExcelResult> {
            if !py_exists(path)? {
                return Err(ValidationError::new(format!("Excel file not found: {display}")).into());
            }
            let suffix = py_suffix(path).to_lowercase();
            if ![".xlsx", ".xls", ".xlsm"].contains(&suffix.as_str()) {
                return Err(ValidationError::new(format!("File is not an Excel file: {display}")).into());
            }
            let engine = options
                .get("engine")
                .cloned()
                .unwrap_or_else(|| Value::String("pandas".to_string()));
            self.progress_tracker.update_tracking(
                &tracking_id,
                None,
                &format!("Parsing with {}...", sem_utils::pycompat::fmt::py_str(&engine)),
            );
            let sheet = sheet_name.filter(|s| !s.is_empty());
            let parsed = if engine == Value::String("pandas".to_string()) {
                parse_with_pandas(path, sheet, options)
            } else {
                parse_with_openpyxl(path, sheet, options)
            };
            match parsed {
                Ok(result) => {
                    let count = match &result {
                        ExcelResult::Data(d) => d.sheets.len(),
                        ExcelResult::Sheet(_) => 1,
                    };
                    self.progress_tracker.stop_tracking(
                        &tracking_id,
                        Some("completed"),
                        &format!("Parsed {count} sheet(s)"),
                        None,
                    );
                    Ok(result)
                }
                Err(e) => {
                    self.progress_tracker
                        .stop_tracking(&tracking_id, Some("failed"), &e, None);
                    self.logger
                        .error(format!("Failed to parse Excel {display}: {e}"));
                    Err(ProcessingError::new(format!("Failed to parse Excel: {e}")).into())
                }
            }
        })();
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `extract_sheet(file_path, sheet_name, **options)`: one sheet (an empty name parses
    /// every sheet and then fails with "Sheet '' not found in Excel file").
    pub fn extract_sheet(
        &self,
        file_path: impl AsRef<Path>,
        sheet_name: &str,
        options: &Options,
    ) -> PResult<ExcelSheet> {
        match self.parse(file_path, Some(sheet_name), options)? {
            ExcelResult::Sheet(sheet) => Ok(sheet),
            ExcelResult::Data(data) => data.sheets.get(sheet_name).cloned().ok_or_else(|| {
                SemanticaError::from(ValidationError::new(format!(
                    "Sheet '{sheet_name}' not found in Excel file"
                )))
            }),
        }
    }
}
