//! Word document parsing (`semantica/parse/docx_parser.py`).
//!
//! [`DOCXParser`] extracts paragraphs, tables, heading-based sections, core-property
//! metadata and (optionally) run formatting from `.docx` files. The original reads the file
//! with python-docx; the port reads it with its own python-docx model
//! ([`Document`], built on `zip` + `quick-xml`), reproducing python-docx's views (body-level
//! paragraphs and tables, `Paragraph.text`, style names, merged table cells, core
//! properties) and its failure messages.
//!
//! [`DOCXParser::document_loader`] is the injection point the original's tests patch
//! (`semantica.parse.docx_parser.Document`): any function returning a [`DocxDocument`].

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::pycompat::fmt::{is_space, strip};
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{
    Options, PResult, attribute_error, opt_truthy, py_exists, py_name, py_path_str, py_suffix,
    with_opt,
};
pub use crate::ooxml::docx::{
    DocxBlock, DocxCoreProperties, DocxDocument, DocxParagraph, DocxRow, DocxRun, DocxStyle,
    DocxTable, DocxTc,
};
use crate::ooxml::docx::{paragraph_alignment, st_on_off, underline_value};

/// `DocxSection` dataclass: a heading and the content up to the next heading.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DocxSection {
    pub heading: String,
    pub level: i64,
    pub content: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Map<String, Value>>,
}

/// `DocxMetadata` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DocxMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub comments: Option<String>,
    pub category: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub last_modified_by: Option<String>,
}

/// The document loader [`DOCXParser`] calls (`docx.Document(str(file_path))`).
pub type DocumentLoader =
    Arc<dyn Fn(&Path) -> Result<DocxDocument, SemanticaError> + Send + Sync + 'static>;

/// `docx.Document(path)`: load a `.docx` file into the python-docx view. Errors carry the
/// message of the exception python-docx raises (`Package not found at '<path>'` for files
/// that are not zip packages, `file '<path>' is not a Word file, content type is '<ct>'`
/// for other Office packages, ...).
#[allow(non_snake_case)]
pub fn Document(path: &Path) -> Result<DocxDocument, SemanticaError> {
    crate::ooxml::docx::Document(path).map_err(|message| {
        let type_name = if message.starts_with("Package not found") {
            "PackageNotFoundError"
        } else if message.starts_with('"') || message.starts_with('\'') {
            "KeyError"
        } else {
            "ValueError"
        };
        SemanticaError::Other {
            module: "docx".to_string(),
            type_name: type_name.to_string(),
            message,
        }
    })
}

/// `DOCXParser(**config)`: DOCX document parser.
#[derive(Clone)]
pub struct DOCXParser {
    /// `self.logger` (`semantica.docx_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
    /// The `Document` constructor used to load files (python-docx's `Document` in the
    /// original; replace it to inject documents, as the original tests patch it).
    pub document_loader: DocumentLoader,
}

impl fmt::Debug for DOCXParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DOCXParser")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Default for DOCXParser {
    fn default() -> Self {
        DOCXParser::new(Options::new())
    }
}

/// Python's `str.split()` (whitespace runs).
fn py_split(s: &str) -> Vec<&str> {
    s.split(is_space).filter(|p| !p.is_empty()).collect()
}

fn processing(message: String) -> SemanticaError {
    ProcessingError::new(message).into()
}

/// An exception raised while reading the document (python-docx's own errors).
fn docx_error(message: String) -> SemanticaError {
    SemanticaError::Other {
        module: "docx".to_string(),
        type_name: "ValueError".to_string(),
        message,
    }
}

impl DOCXParser {
    /// `DOCXParser(**config)`.
    pub fn new(config: Options) -> Self {
        DOCXParser {
            logger: get_logger("docx_parser"),
            config,
            progress_tracker: get_progress_tracker(),
            document_loader: Arc::new(Document),
        }
    }

    /// `parse(file_path, pipeline_id=None, **options)`: parse a DOCX document into
    /// `{"metadata", "full_text", "paragraphs", "tables", "sections", "comments",
    /// "formatting", "total_paragraphs", "total_tables"}`. Options: `extract_tables`
    /// (default true), `extract_comments` (always an empty list), `extract_formatting`
    /// (default false).
    pub fn parse(
        &self,
        file_path: impl AsRef<Path>,
        pipeline_id: Option<&str>,
        options: &Options,
    ) -> PResult<Map<String, Value>> {
        let path = file_path.as_ref();
        let display = py_path_str(path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&display),
            Some("parse"),
            Some("DOCXParser"),
            &format!("DOCX: {}", py_name(path)),
            pipeline_id,
        );
        let result = (|| -> PResult<Map<String, Value>> {
            if !py_exists(path)? {
                return Err(ValidationError::new(format!("DOCX file not found: {display}")).into());
            }
            let suffix = py_suffix(path).to_lowercase();
            if suffix != ".docx" && suffix != ".doc" {
                return Err(ValidationError::new(format!("File is not a DOCX: {display}")).into());
            }
            match self.parse_document(path, options) {
                Ok((result, paragraphs, tables)) => {
                    self.progress_tracker.stop_tracking(
                        &tracking_id,
                        Some("completed"),
                        &format!("Parsed {paragraphs} paragraphs, {tables} tables"),
                        None,
                    );
                    Ok(result)
                }
                Err(e) => {
                    self.progress_tracker.stop_tracking(
                        &tracking_id,
                        Some("failed"),
                        &e.to_string(),
                        None,
                    );
                    self.logger
                        .error(format!("Failed to parse DOCX {display}: {e}"));
                    Err(processing(format!("Failed to parse DOCX: {e}")))
                }
            }
        })();
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn parse_document(
        &self,
        path: &Path,
        options: &Options,
    ) -> PResult<(Map<String, Value>, usize, usize)> {
        let doc = (self.document_loader)(path)?;
        let metadata = extract_metadata(&doc)?;
        let paragraphs: Vec<String> = doc
            .paragraphs
            .iter()
            .map(|p| strip(&p.text))
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect();
        let mut tables = Vec::new();
        if opt_truthy(options, "extract_tables", true) {
            for table in &doc.tables {
                tables.push(Value::Object(extract_table(table)?));
            }
        }
        let sections = extract_sections(&doc)?;
        let comments: Vec<Value> = Vec::new();
        let formatting = if opt_truthy(options, "extract_formatting", false) {
            Value::Object(extract_formatting(&doc)?)
        } else {
            Value::Null
        };
        let (n_paragraphs, n_tables) = (paragraphs.len(), tables.len());
        let mut out = Map::new();
        out.insert(
            "metadata".to_string(),
            serde_json::to_value(&metadata).unwrap_or(Value::Null),
        );
        out.insert("full_text".to_string(), Value::String(paragraphs.join("\n\n")));
        out.insert(
            "paragraphs".to_string(),
            Value::Array(paragraphs.into_iter().map(Value::String).collect()),
        );
        out.insert("tables".to_string(), Value::Array(tables));
        out.insert(
            "sections".to_string(),
            serde_json::to_value(&sections).unwrap_or(Value::Null),
        );
        out.insert("comments".to_string(), Value::Array(comments));
        out.insert("formatting".to_string(), formatting);
        out.insert("total_paragraphs".to_string(), Value::from(n_paragraphs));
        out.insert("total_tables".to_string(), Value::from(n_tables));
        Ok((out, n_paragraphs, n_tables))
    }

    /// `extract_text(file_path)`: the document's `full_text` (paragraphs joined by blank
    /// lines).
    pub fn extract_text(&self, file_path: impl AsRef<Path>) -> PResult<String> {
        let mut options = Options::new();
        options.insert("extract_tables".to_string(), Value::Bool(false));
        options.insert("extract_comments".to_string(), Value::Bool(false));
        options.insert("extract_formatting".to_string(), Value::Bool(false));
        let result = self.parse(file_path, None, &options)?;
        Ok(match result.get("full_text") {
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        })
    }

    /// `extract_tables(file_path)`: the document's tables
    /// (`{"rows", "row_count", "col_count"}` each).
    pub fn extract_tables(&self, file_path: impl AsRef<Path>) -> PResult<Vec<Value>> {
        let options = with_opt(
            &with_opt(&Options::new(), "extract_formatting", Value::Bool(false)),
            "extract_comments",
            Value::Bool(false),
        );
        let result = self.parse(file_path, None, &options)?;
        Ok(match result.get("tables") {
            Some(Value::Array(t)) => t.clone(),
            _ => Vec::new(),
        })
    }
}

/// `str(datetime)` of a python-docx core-property date (`None` when absent or unparseable;
/// an empty element raises python-docx's `TypeError`).
fn docx_date(value: &Option<Option<String>>) -> PResult<Option<String>> {
    match value {
        None => Ok(None),
        Some(None) => Err(SemanticaError::Type(
            "'NoneType' object is not subscriptable".to_string(),
        )),
        Some(Some(text)) => Ok(crate::ooxml::w3cdtf::parse(text)
            .map(|dt| format!("{}+00:00", dt.format("%Y-%m-%d %H:%M:%S")))),
    }
}

/// `_extract_metadata(doc)`.
fn extract_metadata(doc: &DocxDocument) -> PResult<DocxMetadata> {
    let core = &doc.core_properties;
    Ok(DocxMetadata {
        title: Some(core.title.clone()),
        author: Some(core.author.clone()),
        subject: Some(core.subject.clone()),
        keywords: Some(core.keywords.clone()),
        comments: Some(core.comments.clone()),
        category: Some(core.category.clone()),
        created: docx_date(&core.created)?,
        modified: docx_date(&core.modified)?,
        last_modified_by: Some(core.last_modified_by.clone()),
    })
}

/// `_extract_table(table)`.
fn extract_table(table: &DocxTable) -> PResult<Map<String, Value>> {
    let mut out = Map::new();
    let row_count = table.rows.len();
    let col_count = if row_count > 0 {
        table.column_count.ok_or_else(|| {
            docx_error("required ``<w:tblGrid>`` child element not present".to_string())
        })?
    } else {
        0
    };
    let mut rows = Vec::new();
    for idx in 0..row_count {
        let cells = table.row_cells(idx).map_err(docx_error)?;
        rows.push(Value::Array(
            cells
                .iter()
                .map(|c| Value::String(strip(&c.text).to_string()))
                .collect(),
        ));
    }
    out.insert("rows".to_string(), Value::Array(rows));
    out.insert("row_count".to_string(), Value::from(row_count));
    out.insert("col_count".to_string(), Value::from(col_count));
    Ok(out)
}

/// `paragraph.style.name` (`AttributeError` when the document has no default paragraph
/// style).
fn style_name(paragraph: &DocxParagraph) -> PResult<Option<&str>> {
    match &paragraph.style {
        None => Err(attribute_error("NoneType", "name")),
        Some(style) => Ok(style.name.as_deref()),
    }
}

/// `_extract_sections(doc, options)`: heading-delimited sections.
fn extract_sections(doc: &DocxDocument) -> PResult<Vec<DocxSection>> {
    let mut sections = Vec::new();
    let mut current: Option<DocxSection> = None;
    for para in &doc.paragraphs {
        let name = style_name(para)?.ok_or_else(|| attribute_error("NoneType", "startswith"))?;
        if name.starts_with("Heading") {
            let parts = py_split(name);
            let last = parts.last().copied().unwrap_or("");
            let level = if !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()) {
                last.parse::<i64>().unwrap_or(i64::MAX)
            } else {
                1
            };
            if let Some(section) = current.take() {
                sections.push(section);
            }
            current = Some(DocxSection {
                heading: strip(&para.text).to_string(),
                level,
                content: String::new(),
                paragraphs: Vec::new(),
                tables: Vec::new(),
            });
        } else if let Some(section) = current.as_mut() {
            let text = strip(&para.text);
            if !text.is_empty() {
                section.paragraphs.push(text.to_string());
                section.content.push_str(text);
                section.content.push_str("\n\n");
            }
        }
    }
    if let Some(section) = current {
        sections.push(section);
    }
    Ok(sections)
}

/// `run.bold`-style tri-state flags.
fn on_off(value: &Option<Option<String>>) -> PResult<Value> {
    match value {
        None => Ok(Value::Null),
        Some(None) => Ok(Value::Bool(true)),
        Some(Some(v)) => st_on_off(v).map(Value::Bool).map_err(docx_error),
    }
}

/// `run.underline`: None, True (single), False (none) or the `WD_UNDERLINE` member (its
/// integer value, as `json.dumps` writes it).
fn underline(value: &Option<Option<String>>) -> PResult<Value> {
    match value {
        None | Some(None) => Ok(Value::Null),
        Some(Some(v)) => {
            let member = underline_value(v).map_err(docx_error)?;
            Ok(match member {
                1 => Value::Bool(true),
                0 => Value::Bool(false),
                other => Value::from(other),
            })
        }
    }
}

/// `run.font.size.pt` (`None` when absent or zero).
fn font_size_pt(value: &Option<String>) -> PResult<Value> {
    let Some(raw) = value else {
        return Ok(Value::Null);
    };
    let emu: i64 = if raw.contains('m') || raw.contains('n') || raw.contains('p') {
        // ST_UniversalMeasure: "12pt", "3.5mm", ...
        let chars: Vec<char> = raw.chars().collect();
        let split = chars.len().saturating_sub(2);
        let quantity: String = chars[..split].iter().collect();
        let units: String = chars[split..].iter().collect();
        let quantity: f64 = crate::config::py_float(&quantity)
            .ok_or_else(|| docx_error(format!("could not convert string to float: '{quantity}'")))?;
        let multiplier = match units.as_str() {
            "mm" => 36000.0,
            "cm" => 360000.0,
            "in" => 914400.0,
            "pt" => 12700.0,
            "pc" | "pi" => 152400.0,
            other => return Err(SemanticaError::Key(other.to_string())),
        };
        (quantity * multiplier).round() as i64
    } else {
        let half_points = crate::ooxml::docx::py_int(Some(raw), 0).map_err(docx_error)?;
        ((half_points as f64 / 2.0) * 12700.0) as i64
    };
    if emu == 0 {
        return Ok(Value::Null);
    }
    Ok(serde_json::Number::from_f64(emu as f64 / 12700.0)
        .map(Value::Number)
        .unwrap_or(Value::Null))
}

/// `_extract_formatting(doc)`.
fn extract_formatting(doc: &DocxDocument) -> PResult<Map<String, Value>> {
    let mut paragraphs = Vec::new();
    for para in &doc.paragraphs {
        let mut p = Map::new();
        p.insert("text".to_string(), Value::String(para.text.clone()));
        let style = style_name(para)?;
        p.insert(
            "style".to_string(),
            style.map(|s| Value::String(s.to_string())).unwrap_or(Value::Null),
        );
        let alignment = match &para.alignment {
            None => Value::Null,
            Some(xml) => {
                let (name, value) = paragraph_alignment(xml).map_err(docx_error)?;
                if value == 0 {
                    Value::Null
                } else {
                    Value::String(format!("{name} ({value})"))
                }
            }
        };
        p.insert("alignment".to_string(), alignment);
        let mut runs = Vec::new();
        for run in &para.runs {
            let mut r = Map::new();
            r.insert("text".to_string(), Value::String(run.text.clone()));
            r.insert("bold".to_string(), on_off(&run.bold)?);
            r.insert("italic".to_string(), on_off(&run.italic)?);
            r.insert("underline".to_string(), underline(&run.underline)?);
            r.insert(
                "font_name".to_string(),
                match &run.font_name {
                    Some(n) if !n.is_empty() => Value::String(n.clone()),
                    _ => Value::Null,
                },
            );
            r.insert("font_size".to_string(), font_size_pt(&run.font_size)?);
            runs.push(Value::Object(r));
        }
        p.insert("runs".to_string(), Value::Array(runs));
        paragraphs.push(Value::Object(p));
    }
    let mut out = Map::new();
    out.insert("paragraphs".to_string(), Value::Array(paragraphs));
    Ok(out)
}
