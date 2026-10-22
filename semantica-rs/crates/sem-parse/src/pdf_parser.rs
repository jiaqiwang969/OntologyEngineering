//! PDF document parsing (`semantica/parse/pdf_parser.py`).
//!
//! The original reads PDFs with pdfplumber (pdfminer underneath); the port reads them with
//! [`crate::pdfdoc`], a pdfplumber-style model on lopdf: page sizes, text extraction (word
//! and line grouping as pdfplumber's `extract_text()`), ruled-table extraction (pdfplumber's
//! "lines" strategy), image placements and the Info-dictionary metadata.
//!
//! The engine is reached through [`PDFParser::pdf_opener`], the counterpart of the module's
//! `pdfplumber` import: tests replace it as the original tests patch
//! `semantica.parse.pdf_parser.pdfplumber`.

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::pycompat::fmt::py_str;
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{Options, PResult, opt_truthy, py_exists, py_name, py_path_str, py_suffix, type_error};
pub use crate::pdfdoc::{Num, PdfDocument, PdfPage, Table};

/// `PDFPage` dataclass: one parsed page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PDFPage {
    /// `page_number` (1-based; negative page indices give `index + 1`).
    pub page_number: i64,
    /// `text` (`""` unless text was extracted).
    pub text: String,
    /// `width` (an integer when the PDF's MediaBox is integral, as pdfplumber reports it).
    pub width: Number,
    /// `height`.
    pub height: Number,
    /// `images`: `{x0, y0, x1, y1, width, height}` per image.
    pub images: Vec<Map<String, Value>>,
    /// `tables`: `{"data": rows, "bbox": None}` per table.
    pub tables: Vec<Map<String, Value>>,
}

impl Default for PDFPage {
    fn default() -> Self {
        PDFPage {
            page_number: 0,
            text: String::new(),
            width: Number::from(0),
            height: Number::from(0),
            images: Vec::new(),
            tables: Vec::new(),
        }
    }
}

/// `PDFMetadata` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PDFMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    pub page_count: i64,
}

/// The PDF engine entry point: `pdfplumber.open(path)`.
pub type PdfOpener = Arc<dyn Fn(&Path) -> PResult<PdfDocument> + Send + Sync>;

/// The default engine: [`crate::pdfdoc::open`] (lopdf).
pub fn default_pdf_opener() -> PdfOpener {
    Arc::new(|path: &Path| crate::pdfdoc::open(path))
}

/// `PDFParser(**config)`: PDF document parser.
#[derive(Clone)]
pub struct PDFParser {
    /// `self.logger` (`semantica.pdf_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
    /// How PDFs are opened (the module's `pdfplumber`); replace it to stand in for the
    /// engine.
    pub pdf_opener: PdfOpener,
}

impl fmt::Debug for PDFParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PDFParser")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Default for PDFParser {
    fn default() -> Self {
        Self::new(Options::new())
    }
}

/// Python's `pages` iteration: the page indices `parse` visits, or the `TypeError` Python
/// raises for the given value (`pages=None` -> `'NoneType' object is not iterable`).
fn page_indices(pages: Option<&Value>, total: usize) -> PResult<Vec<(i64, usize)>> {
    let Some(pages) = pages else {
        return Ok((0..total).map(|i| (i as i64, i)).collect());
    };
    let items: Vec<Value> = match pages {
        Value::Array(items) => items.clone(),
        Value::Null => return Err(type_error("'NoneType' object is not iterable")),
        Value::Bool(_) => return Err(type_error("'bool' object is not iterable")),
        Value::Number(n) if n.is_f64() => return Err(type_error("'float' object is not iterable")),
        Value::Number(_) => return Err(type_error("'int' object is not iterable")),
        Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
        Value::Object(m) => m.keys().map(|k| Value::String(k.clone())).collect(),
    };
    let total_i = total as i64;
    let mut out = Vec::new();
    for item in items {
        let index = match &item {
            Value::Bool(b) => *b as i64,
            Value::Number(n) if n.is_i64() || n.is_u64() => n.as_i64().unwrap_or(i64::MAX),
            Value::Number(n) => {
                if n.as_f64().unwrap_or(f64::NAN) < total as f64 {
                    return Err(type_error("list indices must be integers or slices, not float"));
                }
                continue;
            }
            other => {
                let name = sem_utils::pycompat::fmt::type_name(other);
                return Err(type_error(format!(
                    "'<' not supported between instances of '{name}' and 'int'"
                )));
            }
        };
        if index < total_i {
            let resolved = if index < 0 { index + total_i } else { index };
            if resolved < 0 {
                return Err(SemanticaError::Index("list index out of range".to_string()));
            }
            out.push((index, resolved as usize));
        }
    }
    Ok(out)
}

fn num_number(n: Num) -> Number {
    match n {
        Num::Int(i) => Number::from(i),
        Num::Float(f) => Number::from_f64(f).unwrap_or_else(|| Number::from(0)),
    }
}

fn str_opt(v: Option<&Value>) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(py_str(other)),
    }
}

impl PDFParser {
    /// `PDFParser(**config)`; like the original, enables the shared progress tracker.
    pub fn new(config: Options) -> Self {
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        PDFParser {
            logger: get_logger("pdf_parser"),
            config,
            progress_tracker,
            pdf_opener: default_pdf_opener(),
        }
    }

    /// `parse(file_path, pipeline_id=None, **options)`: parse a PDF into
    /// `{"metadata", "pages", "full_text", "total_pages"}`. Options: `extract_text` (default
    /// true), `extract_tables` (true), `extract_images` (false), `pages` (0-based indices; all
    /// pages when absent).
    pub fn parse(
        &self,
        file_path: impl AsRef<Path>,
        pipeline_id: Option<&str>,
        options: &Options,
    ) -> PResult<Map<String, Value>> {
        let file_path = file_path.as_ref();
        let shown = py_path_str(file_path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&shown),
            Some("parse"),
            Some("PDFParser"),
            &format!("PDF: {}", py_name(file_path)),
            pipeline_id,
        );
        let result = (|| -> PResult<Map<String, Value>> {
            if !py_exists(file_path)? {
                return Err(ValidationError::new(format!("PDF file not found: {shown}")).into());
            }
            if py_suffix(file_path).to_lowercase() != ".pdf" {
                return Err(ValidationError::new(format!("File is not a PDF: {shown}")).into());
            }
            let inner = (|| -> PResult<Map<String, Value>> {
                let pdf = (self.pdf_opener)(file_path)?;
                let mut metadata = Self::extract_pdf_metadata(&pdf);
                metadata.page_count = pdf.pages.len() as i64;
                self.progress_tracker.update_tracking(
                    &tracking_id,
                    None,
                    &format!("Parsing {} pages", pdf.pages.len()),
                );
                let mut pages = Vec::new();
                for (index, resolved) in page_indices(options.get("pages"), pdf.pages.len())? {
                    pages.push(self.parse_page(&pdf.pages[resolved], index + 1, options)?);
                }
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!("Parsed {} pages", pages.len()),
                    None,
                );
                let full_text = pages
                    .iter()
                    .map(|p: &PDFPage| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let mut out = Map::new();
                out.insert("metadata".into(), serde_json::to_value(&metadata).unwrap_or(Value::Null));
                out.insert("pages".into(), serde_json::to_value(&pages).unwrap_or(Value::Null));
                out.insert("full_text".into(), Value::String(full_text));
                out.insert("total_pages".into(), Value::from(pdf.pages.len()));
                Ok(out)
            })();
            inner.map_err(|e| {
                self.progress_tracker
                    .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                self.logger
                    .error(format!("Failed to parse PDF {shown}: {e}"));
                ProcessingError::new(format!("Failed to parse PDF: {e}")).into()
            })
        })();
        result.inspect_err(|e| {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        })
    }

    /// `extract_text(file_path, pages=None)`: `parse(..., extract_tables=False,
    /// extract_images=False, pages=pages)["full_text"]`. As in the original, `pages=None` is
    /// passed on explicitly, which fails (`'NoneType' object is not iterable`).
    pub fn extract_text(&self, file_path: impl AsRef<Path>, pages: Option<&[i64]>) -> PResult<String> {
        let mut options = Options::new();
        options.insert("extract_tables".into(), Value::Bool(false));
        options.insert("extract_images".into(), Value::Bool(false));
        options.insert("pages".into(), pages_value(pages));
        let result = self.parse(file_path, None, &options)?;
        Ok(result
            .get("full_text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    /// `extract_tables(file_path, pages=None)`: every table dict, tagged with its
    /// `page_number` (fails for `pages=None` like [`Self::extract_text`]).
    pub fn extract_tables(&self, file_path: impl AsRef<Path>, pages: Option<&[i64]>) -> PResult<Vec<Value>> {
        let mut options = Options::new();
        options.insert("extract_text".into(), Value::Bool(false));
        options.insert("extract_images".into(), Value::Bool(false));
        options.insert("pages".into(), pages_value(pages));
        let result = self.parse(file_path, None, &options)?;
        Ok(collect_tagged(&result, "tables"))
    }

    /// `extract_images(file_path, output_dir=None)`: every image dict, tagged with its
    /// `page_number` (`output_dir` is unused, as in the original).
    pub fn extract_images(&self, file_path: impl AsRef<Path>, output_dir: Option<&Path>) -> PResult<Vec<Value>> {
        let _ = output_dir;
        let mut options = Options::new();
        options.insert("extract_text".into(), Value::Bool(false));
        options.insert("extract_tables".into(), Value::Bool(false));
        options.insert("extract_images".into(), Value::Bool(true));
        let result = self.parse(file_path, None, &options)?;
        Ok(collect_tagged(&result, "images"))
    }

    fn parse_page(&self, page: &PdfPage, page_number: i64, options: &Options) -> PResult<PDFPage> {
        let mut data = PDFPage {
            page_number,
            text: String::new(),
            width: num_number(page.width),
            height: num_number(page.height),
            images: Vec::new(),
            tables: Vec::new(),
        };
        if opt_truthy(options, "extract_text", true) {
            data.text = page.extract_text();
        }
        if opt_truthy(options, "extract_tables", true) {
            let tables = page.extract_tables();
            data.tables = tables
                .into_iter()
                .map(|table| {
                    let rows: Vec<Value> = table
                        .into_iter()
                        .map(|row| {
                            Value::Array(
                                row.into_iter()
                                    .map(|cell| cell.map(Value::String).unwrap_or(Value::Null))
                                    .collect(),
                            )
                        })
                        .collect();
                    let mut m = Map::new();
                    m.insert("data".into(), Value::Array(rows));
                    m.insert("bbox".into(), Value::Null);
                    m
                })
                .collect();
        }
        if opt_truthy(options, "extract_images", false) {
            data.images = page
                .images()
                .into_iter()
                .map(|img| {
                    let mut m = Map::new();
                    for key in ["x0", "y0", "x1", "y1", "width", "height"] {
                        m.insert(key.into(), img.get(key).cloned().unwrap_or(Value::from(0)));
                    }
                    m
                })
                .collect();
        }
        Ok(data)
    }

    /// `_extract_metadata(pdf)`: the Info values pdfplumber decodes (`CreationDate`/`ModDate`
    /// as `str(value)`, `""` when absent).
    fn extract_pdf_metadata(pdf: &PdfDocument) -> PDFMetadata {
        let m = &pdf.metadata;
        let date = |key: &str| match m.get(key) {
            None => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => py_str(other),
        };
        PDFMetadata {
            title: str_opt(m.get("Title")),
            author: str_opt(m.get("Author")),
            subject: str_opt(m.get("Subject")),
            creator: str_opt(m.get("Creator")),
            producer: str_opt(m.get("Producer")),
            creation_date: Some(date("CreationDate")),
            modification_date: Some(date("ModDate")),
            page_count: 0,
        }
    }
}

fn pages_value(pages: Option<&[i64]>) -> Value {
    match pages {
        Some(p) => Value::Array(p.iter().map(|&i| Value::from(i)).collect()),
        None => Value::Null,
    }
}

fn collect_tagged(result: &Map<String, Value>, key: &str) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(Value::Array(pages)) = result.get("pages") {
        for page in pages {
            let number = page.get("page_number").cloned().unwrap_or(Value::Null);
            if let Some(Value::Array(items)) = page.get(key) {
                for item in items {
                    let mut item = item.clone();
                    if let Value::Object(m) = &mut item {
                        m.insert("page_number".into(), number.clone());
                    }
                    out.push(item);
                }
            }
        }
    }
    out
}
