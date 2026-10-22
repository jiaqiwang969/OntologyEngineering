//! CSV document parsing (`semantica/parse/csv_parser.py`).
//!
//! [`CSVParser`] reads a CSV file with Python's `csv` semantics (`csv.DictReader` /
//! `csv.reader` with the excel dialect, `csv.Sniffer` when the delimiter is `None`, see
//! `pycsv.rs`) into [`CSVData`].
//!
//! Rows are maps from header to field text. `csv.DictReader` stores the extra fields of a
//! long row under the key `None`; Rust maps need string keys, so they are stored under
//! `"null"` (the key `json.dumps` writes for `None`). Missing fields of a short row are
//! `null` (`restval=None`).

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::{SemanticaError, ValidationError};

use crate::common::{
    DecodeErrors, Options, PResult, opt_string, py_exists, py_path_str, py_read_text_raw,
    type_error,
};
use crate::pycsv::{CsvError, Dialect, Reader, sniff_delimiter};

/// The key under which long rows keep their extra fields (`DictReader.restkey = None`).
pub const REST_KEY: &str = "null";

/// `CSVData`: CSV data representation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CSVData {
    /// `headers`.
    pub headers: Vec<String>,
    /// `rows`: one map per row.
    pub rows: Vec<Map<String, Value>>,
    /// `row_count`.
    pub row_count: usize,
    /// `metadata`: `file_path`, `delimiter`, `encoding`, `has_header`.
    pub metadata: Map<String, Value>,
}

/// `csv.DictReader(f, delimiter=...)`.
struct DictReader {
    reader: Reader,
    fieldnames: Option<Vec<String>>,
    line_num: usize,
}

impl DictReader {
    fn new(reader: Reader) -> Self {
        DictReader {
            reader,
            fieldnames: None,
            line_num: 0,
        }
    }

    /// The `fieldnames` property: reads the header record on first use.
    fn fieldnames(&mut self) -> Result<Option<Vec<String>>, CsvError> {
        if self.fieldnames.is_none() {
            if let Some(rec) = self.reader.next_record() {
                self.fieldnames = Some(rec?);
            }
        }
        self.line_num = self.reader.line_num;
        Ok(self.fieldnames.clone())
    }

    /// `next(dict_reader)`: skips blank rows.
    fn next_row(&mut self) -> Option<Result<Map<String, Value>, CsvError>> {
        if self.line_num == 0
            && let Err(e) = self.fieldnames()
        {
            return Some(Err(e));
        }
        let mut row = match self.reader.next_record()? {
            Ok(r) => r,
            Err(e) => return Some(Err(e)),
        };
        self.line_num = self.reader.line_num;
        while row.is_empty() {
            row = match self.reader.next_record()? {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
        }
        let fieldnames = self.fieldnames.clone().unwrap_or_default();
        let mut d = Map::new();
        for (k, v) in fieldnames.iter().zip(row.iter()) {
            d.insert(k.clone(), Value::String(v.clone()));
        }
        let lf = fieldnames.len();
        let lr = row.len();
        if lf < lr {
            d.insert(
                REST_KEY.to_string(),
                Value::Array(row[lf..].iter().cloned().map(Value::String).collect()),
            );
        } else if lf > lr {
            for key in &fieldnames[lr..] {
                d.insert(key.clone(), Value::Null);
            }
        }
        Some(Ok(d))
    }
}

/// `CSVParser(**config)`: CSV document parser.
#[derive(Debug, Clone)]
pub struct CSVParser {
    /// `self.logger`.
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker` (enabled on construction, as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for CSVParser {
    fn default() -> Self {
        CSVParser::new(Options::new())
    }
}

/// How the delimiter argument was given.
enum DelimiterArg {
    Given(String),
    Sniff,
    Invalid,
}

impl CSVParser {
    /// `CSVParser(**config)`.
    pub fn new(config: Options) -> Self {
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        CSVParser {
            logger: get_logger("csv_parser"),
            config,
            progress_tracker,
        }
    }

    /// `parse(file_path, delimiter=",", **options)`. `delimiter: None` is Python's
    /// `delimiter=None`: the delimiter is sniffed from the first 1024 characters; pass
    /// `Some(",")` for the original default. Options: `has_header` (default `True`),
    /// `encoding` (default `utf-8`, undecodable bytes are dropped), `skip_rows`, `max_rows`.
    pub fn parse(
        &self,
        file_path: impl AsRef<Path>,
        delimiter: Option<&str>,
        options: &Options,
    ) -> PResult<CSVData> {
        let arg = match delimiter {
            Some(d) => DelimiterArg::Given(d.to_string()),
            None => DelimiterArg::Sniff,
        };
        self.parse_inner(file_path.as_ref(), arg, options)
    }

    fn parse_inner(
        &self,
        file_path: &Path,
        delimiter: DelimiterArg,
        options: &Options,
    ) -> PResult<CSVData> {
        let path_text = py_path_str(file_path);
        if !py_exists(file_path)? {
            return Err(ValidationError::new(format!("CSV file not found: {path_text}")).into());
        }
        let has_header = options
            .get("has_header")
            .cloned()
            .unwrap_or(Value::Bool(true));
        let encoding = opt_string(options, "encoding", "utf-8");
        let skip_rows = options.get("skip_rows").cloned().unwrap_or(Value::from(0));
        let max_rows = options.get("max_rows").cloned().unwrap_or(Value::Null);

        let tracking_id = self.progress_tracker.start_tracking(
            Some(&path_text),
            Some("parse"),
            Some("CSVParser"),
            &format!("Parsing CSV file: {path_text}"),
            None,
        );
        let result = (|| -> PResult<CSVData> {
            let text = py_read_text_raw(file_path, &encoding, DecodeErrors::Ignore)?;
            let delimiter = match delimiter {
                DelimiterArg::Given(d) => d,
                DelimiterArg::Sniff => {
                    let sample: String = text.chars().take(1024).collect();
                    sniff_delimiter(&sample)?
                }
                DelimiterArg::Invalid => {
                    return Err(type_error("\"delimiter\" must be a 1-character string"));
                }
            };
            let dialect = Dialect::with_delimiter(&delimiter)?;
            let skip = py_range_count(&skip_rows)?;
            let max = max_rows_limit(&max_rows);
            let has_header_bool = sem_utils::pycompat::fmt::truthy(&has_header);

            let mut headers: Vec<String> = Vec::new();
            let mut rows: Vec<Map<String, Value>> = Vec::new();
            if has_header_bool {
                let mut reader = DictReader::new(Reader::new(&text, dialect));
                headers = reader.fieldnames()?.unwrap_or_default();
                for _ in 0..skip {
                    match reader.next_row() {
                        None => break,
                        Some(r) => {
                            r?;
                        }
                    }
                }
                let mut idx = 0usize;
                while let Some(row) = reader.next_row() {
                    let row = row?;
                    if max.is_some_and(|m| idx >= m) {
                        break;
                    }
                    rows.push(row);
                    idx += 1;
                }
            } else {
                let mut reader = Reader::new(&text, dialect);
                if let Some(first) = reader.next_record() {
                    let first = first?;
                    headers = (1..=first.len()).map(|i| format!("Column_{i}")).collect();
                    rows.push(zip_row(&headers, &first));
                }
                for _ in 0..skip {
                    match reader.next_record() {
                        None => break,
                        Some(r) => {
                            r?;
                        }
                    }
                }
                let mut idx = 0usize;
                while let Some(row) = reader.next_record() {
                    let row = row?;
                    if max.is_some_and(|m| idx >= m) {
                        break;
                    }
                    rows.push(zip_row(&headers, &row));
                    idx += 1;
                }
            }

            let mut metadata = Map::new();
            metadata.insert("file_path".to_string(), Value::String(path_text.clone()));
            metadata.insert("delimiter".to_string(), Value::String(delimiter));
            metadata.insert("encoding".to_string(), Value::String(encoding.clone()));
            metadata.insert("has_header".to_string(), has_header.clone());
            let row_count = rows.len();
            Ok(CSVData {
                headers,
                rows,
                row_count,
                metadata,
            })
        })();
        match &result {
            Ok(data) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Parsed {} rows", data.rows.len()),
                None,
            ),
            Err(e) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("failed"),
                &e.to_string(),
                None,
            ),
        }
        result
    }

    /// `parse(file_path, **options)` with `delimiter` bound from the keywords (absent: `","`).
    fn parse_with_kwargs(&self, file_path: &Path, options: &Options) -> PResult<CSVData> {
        let (delimiter, rest) = crate::common::bind_kwarg(options, "delimiter");
        let arg = match delimiter {
            None => DelimiterArg::Given(",".to_string()),
            Some(Value::Null) => DelimiterArg::Sniff,
            Some(Value::String(s)) => DelimiterArg::Given(s),
            Some(_) => DelimiterArg::Invalid,
        };
        self.parse_inner(file_path, arg, &rest)
    }

    /// `parse_to_dict(file_path, **options)`: the rows (a `delimiter` keyword is honoured).
    pub fn parse_to_dict(
        &self,
        file_path: impl AsRef<Path>,
        options: &Options,
    ) -> PResult<Vec<Map<String, Value>>> {
        Ok(self.parse_with_kwargs(file_path.as_ref(), options)?.rows)
    }

    /// `parse_to_list(file_path, **options)`: each row's values in header order (`""` for
    /// missing headers, `null` for the short-row fill value).
    pub fn parse_to_list(
        &self,
        file_path: impl AsRef<Path>,
        options: &Options,
    ) -> PResult<Vec<Vec<Value>>> {
        let data = self.parse_with_kwargs(file_path.as_ref(), options)?;
        Ok(data
            .rows
            .iter()
            .map(|row| {
                data.headers
                    .iter()
                    .map(|h| {
                        row.get(h)
                            .cloned()
                            .unwrap_or_else(|| Value::String(String::new()))
                    })
                    .collect()
            })
            .collect())
    }
}

/// `dict(zip(headers, row))`.
fn zip_row(headers: &[String], row: &[String]) -> Map<String, Value> {
    headers
        .iter()
        .zip(row.iter())
        .map(|(h, v)| (h.clone(), Value::String(v.clone())))
        .collect()
}

/// `len(range(skip_rows))` (an `int` is required).
fn py_range_count(value: &Value) -> PResult<usize> {
    match value {
        Value::Number(n) if n.is_i64() || n.is_u64() => {
            Ok(n.as_i64().map(|i| i.max(0) as usize).unwrap_or(usize::MAX))
        }
        Value::Bool(b) => Ok(usize::from(*b)),
        other => Err(SemanticaError::Type(format!(
            "'{}' object cannot be interpreted as an integer",
            sem_utils::pycompat::fmt::type_name(other)
        ))),
    }
}

/// `max_rows` as the loop uses it: a falsy value means no limit.
fn max_rows_limit(value: &Value) -> Option<usize> {
    if !sem_utils::pycompat::fmt::truthy(value) {
        return None;
    }
    match value {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f <= 0.0 {
                Some(0)
            } else {
                Some(f.ceil() as usize)
            }
        }
        Value::Bool(true) => Some(1),
        _ => None,
    }
}
