//! PowerPoint parsing (`semantica/parse/pptx_parser.py`).
//!
//! [`PPTXParser`] extracts slide titles, text, speaker notes, picture and shape
//! information and core-property metadata from `.pptx` files. The original reads the file
//! with python-pptx; the port reads it with its own python-pptx model (on `zip` +
//! `quick-xml`) reproducing python-pptx's views: slides in presentation order, top-level
//! shapes of each slide (`slide.shapes`), placeholder indices, `shape.text` (paragraphs
//! joined by `"\n"`, line breaks as `"\v"`), `shape.shape_type`, picture formats and
//! inherited placeholder sizes, and the notes slide's body placeholder.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::pycompat::fmt::strip;
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{
    Options, PResult, opt_truthy, py_exists, py_path_str, py_stem, py_suffix, with_opt,
};
use crate::ooxml::pptx::{self, ImageAttr, PptxPresentation, PptxSlide, ShapeKind};

/// `SlideContent` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SlideContent {
    pub slide_number: i64,
    pub title: String,
    pub text: String,
    pub notes: String,
    /// `{"type": MSO_SHAPE_TYPE, "has_text": bool}` per shape (when `extract_shapes`); the
    /// shape type is the enum member's integer value (what `json.dumps` writes), `null` when
    /// python-pptx returns `None`.
    pub shapes: Vec<Map<String, Value>>,
    /// `{"width", "height", "format"}` per picture (when `extract_images`).
    pub images: Vec<Map<String, Value>>,
}

/// `PPTXData` dataclass.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PPTXData {
    pub title: String,
    pub slides: Vec<SlideContent>,
    pub metadata: Map<String, Value>,
}

/// `PPTXParser(**config)`: PPTX document parser.
#[derive(Debug, Clone)]
pub struct PPTXParser {
    /// `self.logger` (`semantica.pptx_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker` (unused by the original's parsing).
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for PPTXParser {
    fn default() -> Self {
        PPTXParser::new(Options::new())
    }
}

/// An exception python-pptx raises while reading.
fn pptx_error(message: String) -> SemanticaError {
    SemanticaError::Other {
        module: "pptx".to_string(),
        type_name: "ValueError".to_string(),
        message,
    }
}

impl PPTXParser {
    /// `PPTXParser(**config)`.
    pub fn new(config: Options) -> Self {
        PPTXParser {
            logger: get_logger("pptx_parser"),
            config,
            progress_tracker: get_progress_tracker(),
        }
    }

    /// `parse(file_path, **options)`: parse a presentation. Options: `extract_images`
    /// (default false), `extract_notes` (default true), `extract_shapes` (default false).
    pub fn parse(&self, file_path: impl AsRef<Path>, options: &Options) -> PResult<PPTXData> {
        let path = file_path.as_ref();
        let display = py_path_str(path);
        if !py_exists(path)? {
            return Err(ValidationError::new(format!("PPTX file not found: {display}")).into());
        }
        if py_suffix(path).to_lowercase() != ".pptx" {
            return Err(ValidationError::new(format!("File is not a PPTX: {display}")).into());
        }
        let result = (|| -> PResult<PPTXData> {
            let prs = pptx::presentation(path).map_err(pptx_error)?;
            let metadata = extract_metadata(&prs);
            let mut slides = Vec::new();
            for (idx, slide) in prs.slides.iter().enumerate() {
                slides.push(parse_slide(slide, idx as i64 + 1, options)?);
            }
            let mut title = match metadata.get("title") {
                Some(Value::String(t)) => t.clone(),
                Some(other) => sem_utils::pycompat::fmt::py_str(other),
                None => format!("Presentation_{}", py_stem(path)),
            };
            if let Some(first) = slides.first()
                && !first.title.is_empty()
            {
                title = first.title.clone();
            }
            let mut meta = metadata;
            meta.insert("total_slides".to_string(), Value::from(slides.len()));
            meta.insert("file_path".to_string(), Value::String(display.clone()));
            Ok(PPTXData {
                title,
                slides,
                metadata: meta,
            })
        })();
        result.map_err(|e| {
            self.logger
                .error(format!("Failed to parse PPTX {display}: {e}"));
            ProcessingError::new(format!("Failed to parse PPTX: {e}")).into()
        })
    }

    /// `extract_text(file_path)`: `"Slide N: <title>\n<text>"` for each slide, joined by
    /// blank lines.
    pub fn extract_text(&self, file_path: impl AsRef<Path>) -> PResult<String> {
        let options = with_opt(
            &with_opt(&Options::new(), "extract_images", Value::Bool(false)),
            "extract_shapes",
            Value::Bool(false),
        );
        let result = self.parse(file_path, &options)?;
        Ok(result
            .slides
            .iter()
            .map(|s| format!("Slide {}: {}\n{}", s.slide_number, s.title, s.text))
            .collect::<Vec<_>>()
            .join("\n\n"))
    }

    /// `extract_slides(file_path, **options)`: the parsed slides.
    pub fn extract_slides(
        &self,
        file_path: impl AsRef<Path>,
        options: &Options,
    ) -> PResult<Vec<SlideContent>> {
        Ok(self.parse(file_path, options)?.slides)
    }
}

/// `_extract_metadata(prs)`.
fn extract_metadata(prs: &PptxPresentation) -> Map<String, Value> {
    let core = &prs.core_properties;
    let mut m = Map::new();
    m.insert("title".to_string(), Value::String(core.title.clone()));
    m.insert("author".to_string(), Value::String(core.author.clone()));
    m.insert("subject".to_string(), Value::String(core.subject.clone()));
    m.insert("keywords".to_string(), Value::String(core.keywords.clone()));
    m.insert("comments".to_string(), Value::String(core.comments.clone()));
    let date = |d: &Option<String>| d.clone().map(Value::String).unwrap_or(Value::Null);
    m.insert("created".to_string(), date(&core.created));
    m.insert("modified".to_string(), date(&core.modified));
    m.insert(
        "last_modified_by".to_string(),
        Value::String(core.last_modified_by.clone()),
    );
    m
}

/// `_parse_slide(slide, slide_number, options)`.
fn parse_slide(slide: &PptxSlide, slide_number: i64, options: &Options) -> PResult<SlideContent> {
    let mut title = String::new();
    let mut text_parts: Vec<String> = Vec::new();
    let mut notes = String::new();
    let mut shapes = Vec::new();
    let mut images = Vec::new();
    let extract_images = opt_truthy(options, "extract_images", false);
    let extract_shapes = opt_truthy(options, "extract_shapes", false);
    for shape in &slide.shapes {
        let is_placeholder = shape.ph.is_some() && shape.kind != ShapeKind::Other;
        if is_placeholder {
            let idx = crate::ooxml::docx::py_int(
                shape.ph.as_ref().and_then(|(_, i)| i.as_deref()),
                0,
            )
            .map_err(pptx_error)?;
            if idx == 0
                && let Some(text) = &shape.text
            {
                title = strip(text).to_string();
            }
        }
        if let Some(text) = &shape.text {
            let stripped = strip(text);
            if !stripped.is_empty() {
                text_parts.push(stripped.to_string());
            }
        }
        if extract_images {
            match &shape.image {
                ImageAttr::Absent => {}
                ImageAttr::Raises(e) => return Err(pptx_error(e.clone())),
                ImageAttr::Present { ext } => {
                    if let (Ok(width), Ok(height), Ok(ext)) = (&shape.width, &shape.height, ext) {
                        let mut info = Map::new();
                        info.insert("width".to_string(), width.map(Value::from).unwrap_or(Value::Null));
                        info.insert(
                            "height".to_string(),
                            height.map(Value::from).unwrap_or(Value::Null),
                        );
                        info.insert("format".to_string(), Value::String(ext.clone()));
                        images.push(info);
                    }
                }
            }
        }
        if extract_shapes {
            let shape_type = shape.shape_type.clone().map_err(|e| SemanticaError::NotImplemented(e))?;
            let mut info = Map::new();
            info.insert(
                "type".to_string(),
                shape_type.map(Value::from).unwrap_or(Value::Null),
            );
            info.insert("has_text".to_string(), Value::Bool(shape.text.is_some()));
            shapes.push(info);
        }
    }
    if opt_truthy(options, "extract_notes", true)
        && let Some(found) = &slide.notes
    {
        match found {
            Ok(Some(text)) => notes = text.clone(),
            Ok(None) => {
                return Err(crate::common::attribute_error("NoneType", "text"));
            }
            Err(e) => return Err(pptx_error(e.clone())),
        }
    }
    if title.is_empty() && !text_parts.is_empty() {
        title = text_parts.remove(0);
    }
    Ok(SlideContent {
        slide_number,
        title,
        text: text_parts.join("\n"),
        notes,
        shapes,
        images,
    })
}
