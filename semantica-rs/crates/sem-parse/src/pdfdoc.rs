//! A pdfplumber-style PDF model on lopdf (what `pdf_parser.py` reads through `pdfplumber`):
//! a [`PdfDocument`] with Info-dictionary `metadata` (decoded like pdfplumber's
//! `resolve_and_decode`) and [`PdfPage`]s with `width`/`height` (pdfplumber's normalised
//! MediaBox, swapped for 90/270-degree rotation), `extract_text()`, `extract_tables()` and
//! `images`.
//!
//! Page content is interpreted lazily, the first time text, tables or images are requested,
//! following pdfminer's interpreter and layout objects (see `interp`), pdfplumber's word and
//! line grouping (`text`) and its "lines" table finder (`table`).
//!
//! The model is public and constructible ([`PdfDocument::new`], [`PdfPage::new`] with
//! `with_text`/`with_tables`/`with_images`) so callers can stand in for the PDF engine, as the
//! original tests do by patching `pdfplumber`.

mod encoding;
mod font;
mod interp;
mod lexer;
mod num;
mod table;
mod text;
mod truetype;

use std::fmt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use lopdf::{Dictionary, Object, ObjectId};
use serde_json::{Map, Value};

use sem_utils::SemanticaError;

use crate::common::{PResult, py_os_error, py_read_bytes};

pub use interp::{Char, ImageObj, Layout, PathKind, PathObj};
pub use num::Num;

/// A table: rows of cell texts (`None` for a missing cell), as `page.extract_tables()`.
pub type Table = Vec<Vec<Option<String>>>;

/// An opened PDF (`pdfplumber.PDF`).
#[derive(Debug, Clone)]
pub struct PdfDocument {
    /// `pdf.metadata`: the Info dictionary with values decoded.
    pub metadata: Map<String, Value>,
    /// `pdf.pages`.
    pub pages: Vec<PdfPage>,
}

impl PdfDocument {
    /// A document from its metadata and pages.
    pub fn new(metadata: Map<String, Value>, pages: Vec<PdfPage>) -> Self {
        PdfDocument { metadata, pages }
    }
}

enum Source {
    Fixed {
        text: String,
        tables: Vec<Table>,
        images: Vec<Map<String, Value>>,
    },
    Pdf {
        doc: Arc<lopdf::Document>,
        page_id: ObjectId,
        ctm: [f64; 6],
        resources: Option<Dictionary>,
        initial_doctop: f64,
        layout: OnceLock<Layout>,
    },
}

/// A page (`pdfplumber.Page`).
pub struct PdfPage {
    /// `page.page_number` (1-based).
    pub page_number: usize,
    /// `page.width` (an `int` when the MediaBox is integral).
    pub width: Num,
    /// `page.height`.
    pub height: Num,
    source: Source,
}

impl fmt::Debug for PdfPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdfPage")
            .field("page_number", &self.page_number)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Clone for PdfPage {
    fn clone(&self) -> Self {
        let source = match &self.source {
            Source::Fixed { text, tables, images } => Source::Fixed {
                text: text.clone(),
                tables: tables.clone(),
                images: images.clone(),
            },
            Source::Pdf {
                doc,
                page_id,
                ctm,
                resources,
                initial_doctop,
                layout,
            } => {
                let cell = OnceLock::new();
                if let Some(l) = layout.get() {
                    let _ = cell.set(l.clone());
                }
                Source::Pdf {
                    doc: doc.clone(),
                    page_id: *page_id,
                    ctm: *ctm,
                    resources: resources.clone(),
                    initial_doctop: *initial_doctop,
                    layout: cell,
                }
            }
        };
        PdfPage {
            page_number: self.page_number,
            width: self.width,
            height: self.height,
            source,
        }
    }
}

impl PdfPage {
    /// A page with fixed (empty) content: no text, tables or images.
    pub fn new(width: impl Into<Num>, height: impl Into<Num>) -> Self {
        PdfPage {
            page_number: 1,
            width: width.into(),
            height: height.into(),
            source: Source::Fixed {
                text: String::new(),
                tables: Vec::new(),
                images: Vec::new(),
            },
        }
    }

    /// Set what `extract_text()` returns (fixed pages only).
    pub fn with_text(mut self, value: impl Into<String>) -> Self {
        if let Source::Fixed { text, .. } = &mut self.source {
            *text = value.into();
        }
        self
    }

    /// Set what `extract_tables()` returns (fixed pages only).
    pub fn with_tables(mut self, value: Vec<Table>) -> Self {
        if let Source::Fixed { tables, .. } = &mut self.source {
            *tables = value;
        }
        self
    }

    /// Set `page.images` (fixed pages only).
    pub fn with_images(mut self, value: Vec<Map<String, Value>>) -> Self {
        if let Source::Fixed { images, .. } = &mut self.source {
            *images = value;
        }
        self
    }

    /// The interpreted page content (`None` for fixed pages).
    pub fn layout(&self) -> Option<&Layout> {
        match &self.source {
            Source::Fixed { .. } => None,
            Source::Pdf {
                doc,
                page_id,
                ctm,
                resources,
                initial_doctop,
                layout,
            } => Some(layout.get_or_init(|| {
                let data = doc.get_page_content(*page_id).unwrap_or_default();
                let mut fonts = std::collections::HashMap::new();
                let mut it = interp::Interpreter::new(doc, &mut fonts, self.height.f(), *initial_doctop);
                it.render_contents(resources.as_ref(), &data, *ctm);
                it.out
            })),
        }
    }

    /// `page.extract_text()`.
    pub fn extract_text(&self) -> String {
        match &self.source {
            Source::Fixed { text, .. } => text.clone(),
            Source::Pdf { .. } => text::page_text(&self.layout().map(|l| l.chars.clone()).unwrap_or_default()),
        }
    }

    /// `page.extract_tables()`.
    pub fn extract_tables(&self) -> Vec<Table> {
        match &self.source {
            Source::Fixed { tables, .. } => tables.clone(),
            Source::Pdf { .. } => match self.layout() {
                Some(l) => table::extract_tables(&l.paths, &l.chars),
                None => Vec::new(),
            },
        }
    }

    /// `page.images`: one dict per image with pdfplumber's attributes (`x0`, `y0`, `x1`,
    /// `y1`, `width`, `height`, `name`, `srcsize`, `imagemask`, `bits`, `colorspace`,
    /// `object_type`, `page_number`, `top`, `bottom`, `doctop`).
    pub fn images(&self) -> Vec<Map<String, Value>> {
        match &self.source {
            Source::Fixed { images, .. } => images.clone(),
            Source::Pdf { .. } => {
                let f = |x: f64| serde_json::Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null);
                self.layout()
                    .map(|l| {
                        l.images
                            .iter()
                            .map(|img| {
                                let mut m = Map::new();
                                m.insert("x0".into(), f(img.x0));
                                m.insert("y0".into(), f(img.y0));
                                m.insert("x1".into(), f(img.x1));
                                m.insert("y1".into(), f(img.y1));
                                m.insert("width".into(), f(img.x1 - img.x0));
                                m.insert("height".into(), f(img.y1 - img.y0));
                                m.insert("name".into(), Value::String(img.name.clone()));
                                m.insert(
                                    "srcsize".into(),
                                    Value::Array(vec![img.srcsize.0.clone(), img.srcsize.1.clone()]),
                                );
                                m.insert("imagemask".into(), img.imagemask.clone());
                                m.insert("bits".into(), img.bits.clone());
                                m.insert("colorspace".into(), img.colorspace.clone());
                                m.insert("object_type".into(), Value::String("image".into()));
                                m.insert("page_number".into(), Value::from(self.page_number));
                                m.insert("top".into(), f(img.top));
                                m.insert("bottom".into(), f(img.bottom));
                                m.insert("doctop".into(), f(img.doctop));
                                m
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
        }
    }
}

/// `pdfplumber.open(path)` errors: pdfminer's exceptions wrapped by pdfplumber.
fn pdfminer_error(message: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "pdfplumber.utils.exceptions".to_string(),
        type_name: "PdfminerException".to_string(),
        message: message.to_string(),
    }
}

fn num_of(obj: &Object) -> Option<Num> {
    match obj {
        Object::Integer(i) => Some(Num::Int(*i)),
        Object::Real(r) => Some(Num::Float(num::f32_to_f64(*r))),
        _ => None,
    }
}

/// An inheritable page attribute (`Resources`, `MediaBox`, `CropBox`, `Rotate`).
fn inherited<'a>(doc: &'a lopdf::Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut node = page;
    for _ in 0..64 {
        if let Ok(v) = node.get(key) {
            return Some(font::deref(doc, v));
        }
        let parent = node.get(b"Parent").ok().map(|p| font::deref(doc, p))?;
        node = font::as_dict(parent)?;
    }
    None
}

/// pdfplumber's `resolve_and_decode` of an Info value.
fn metadata_value(doc: &lopdf::Document, obj: &Object, depth: usize) -> Value {
    if depth > 16 {
        return Value::Null;
    }
    match font::deref(doc, obj) {
        Object::String(s, _) => Value::String(encoding::decode_text(s)),
        Object::Name(n) => Value::String(encoding::decode_text(n)),
        Object::Array(items) => Value::Array(items.iter().map(|o| metadata_value(doc, o, depth + 1)).collect()),
        Object::Dictionary(d) => Value::Object(
            d.iter()
                .map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), metadata_value(doc, v, depth + 1)))
                .collect(),
        ),
        other => interp::pdf_value(doc, other, depth),
    }
}

/// Open a PDF file like `pdfplumber.open(str(path))`.
pub fn open(path: &Path) -> PResult<PdfDocument> {
    let bytes = py_read_bytes(path)?;
    load_bytes(&bytes).map_err(|e| match e {
        LoadError::Pdf(msg) => pdfminer_error(&msg),
        LoadError::Password => SemanticaError::Other {
            module: "pdfminer.pdfdocument".to_string(),
            type_name: "PDFPasswordIncorrect".to_string(),
            message: String::new(),
        },
        LoadError::Io(err) => py_os_error(&err, path),
    })
}

enum LoadError {
    Pdf(String),
    Password,
    #[allow(dead_code)]
    Io(std::io::Error),
}

fn load_bytes(bytes: &[u8]) -> Result<PdfDocument, LoadError> {
    let doc = lopdf::Document::load_mem(bytes)
        .map_err(|_| LoadError::Pdf("No /Root object! - Is this really a PDF?".to_string()))?;
    if doc.is_encrypted() {
        return Err(LoadError::Password);
    }
    if doc.catalog().is_err() {
        return Err(LoadError::Pdf("No /Root object! - Is this really a PDF?".to_string()));
    }
    let mut metadata = Map::new();
    if let Ok(info) = doc.trailer.get(b"Info")
        && let Some(info) = font::as_dict(font::deref(&doc, info))
    {
        for (k, v) in info.iter() {
            metadata.insert(String::from_utf8_lossy(k).into_owned(), metadata_value(&doc, v, 0));
        }
    }
    let doc = Arc::new(doc);
    let mut pages = Vec::new();
    let mut doctop = Num::Int(0);
    let page_ids: Vec<ObjectId> = doc.page_iter().collect();
    for (index, page_id) in page_ids.into_iter().enumerate() {
        let Ok(page) = doc.get_dictionary(page_id) else {
            continue;
        };
        let rotate = inherited(&doc, page, b"Rotate")
            .and_then(num_of)
            .map(|n| n.f() as i64)
            .unwrap_or(0)
            .rem_euclid(360);
        let mediabox: Vec<Num> = inherited(&doc, page, b"MediaBox")
            .and_then(|o| o.as_array().ok())
            .map(|a| a.iter().filter_map(|o| num_of(font::deref(&doc, o))).collect())
            .unwrap_or_default();
        if mediabox.len() != 4 {
            // pdfminer: KeyError('MediaBox') / unparsable rectangle.
            return Err(LoadError::Pdf("'MediaBox'".to_string()));
        }
        // pdfplumber's `_normalize_box`.
        let (x0, x1) = if mediabox[2].f() < mediabox[0].f() { (mediabox[2], mediabox[0]) } else { (mediabox[0], mediabox[2]) };
        let (y0, y1) = if mediabox[3].f() < mediabox[1].f() { (mediabox[3], mediabox[1]) } else { (mediabox[1], mediabox[3]) };
        let (bx0, by0, bx1, by1) = if rotate == 90 || rotate == 270 { (y0, x0, y1, x1) } else { (x0, y0, x1, y1) };
        let width = bx1 - bx0;
        let height = by1 - by0;
        // pdfminer's `process_page` CTM from the (float) MediaBox as given.
        let [mx0, my0, mx1, my1] = [mediabox[0].f(), mediabox[1].f(), mediabox[2].f(), mediabox[3].f()];
        let ctm = match rotate {
            90 => [0.0, -1.0, 1.0, 0.0, -my0, mx1],
            180 => [-1.0, 0.0, 0.0, -1.0, mx1, my1],
            270 => [0.0, 1.0, -1.0, 0.0, my1, -mx0],
            _ => [1.0, 0.0, 0.0, 1.0, -mx0, -my0],
        };
        let resources = inherited(&doc, page, b"Resources").and_then(font::as_dict).cloned();
        pages.push(PdfPage {
            page_number: index + 1,
            width,
            height,
            source: Source::Pdf {
                doc: doc.clone(),
                page_id,
                ctm,
                resources,
                initial_doctop: doctop.f(),
                layout: OnceLock::new(),
            },
        });
        doctop = doctop + height;
    }
    Ok(PdfDocument { metadata, pages })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_pages_report_given_content() {
        let page = PdfPage::new(612, 792).with_text("Page text");
        assert_eq!(page.extract_text(), "Page text");
        assert!(page.extract_tables().is_empty());
        assert!(page.images().is_empty());
        assert_eq!(page.width, Num::Int(612));
    }

    #[test]
    fn garbage_is_not_a_pdf() {
        match load_bytes(b"dummy pdf content") {
            Err(LoadError::Pdf(msg)) => assert_eq!(msg, "No /Root object! - Is this really a PDF?"),
            _ => panic!("expected an error"),
        }
    }
}
