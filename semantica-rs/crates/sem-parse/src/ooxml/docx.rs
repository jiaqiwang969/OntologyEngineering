//! A Word document as python-docx 1.2.0 exposes it (`docx.Document(path)`): body-level
//! paragraphs and tables, paragraph text/style/alignment/runs, table rows and cells, and
//! the core properties, plus the document order of body blocks and list numbering
//! (which python-docx's `Document` keeps in the XML, used by Docling-style conversion).
//!
//! The model is public so callers can build documents directly (the parser's
//! `document_loader` injection point mirrors patching `docx.Document`). Values python-docx
//! converts lazily from the XML (on/off flags, alignment, underline, sizes, spans) are kept
//! as the raw attribute text and converted with python-docx's rules (and errors) on use.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::dom::{Element, Node};
use super::opc::{Flavor, Package, ct, rt};

/// WordprocessingML main namespace.
pub(crate) const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const CP: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const DCTERMS: &str = "http://purl.org/dc/terms/";

/// A resolved paragraph style (`paragraph.style`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxStyle {
    /// `w:styleId`.
    pub style_id: Option<String>,
    /// `style.name`: the UI name (`BabelFish.internal2ui` of `w:name/@w:val`), `None` when
    /// the style has no name.
    pub name: Option<String>,
}

impl DocxStyle {
    /// A style with an id and UI name.
    pub fn named(name: &str) -> Self {
        DocxStyle {
            style_id: Some(name.replace(' ', "")),
            name: Some(name.to_string()),
        }
    }
}

/// A run (`w:r`) as python-docx's `Run` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxRun {
    /// `run.text`.
    pub text: String,
    /// `w:rPr/w:b`: `None` when absent, `Some(None)` for `<w:b/>`, else the `w:val` text.
    pub bold: Option<Option<String>>,
    /// `w:rPr/w:i` (same encoding as `bold`).
    pub italic: Option<Option<String>>,
    /// `w:rPr/w:u` (same encoding; the value is a `WD_UNDERLINE` XML value).
    pub underline: Option<Option<String>>,
    /// `w:rPr/w:rFonts/@w:ascii`.
    pub font_name: Option<String>,
    /// `w:rPr/w:sz/@w:val` (half-points).
    pub font_size: Option<String>,
}

/// A paragraph (`w:p`) as python-docx's `Paragraph` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxParagraph {
    /// `paragraph.text`.
    pub text: String,
    /// `paragraph.style` (`None` when the document defines no default paragraph style).
    pub style: Option<DocxStyle>,
    /// `w:pPr/w:jc/@w:val` (the raw alignment value).
    pub alignment: Option<String>,
    /// `paragraph.runs`.
    pub runs: Vec<DocxRun>,
    /// `w:pPr/w:numPr` as `(numId, ilvl)` raw values (list numbering).
    pub num_pr: Option<(Option<String>, Option<String>)>,
    /// `w:pPr/w:outlineLvl/@w:val`.
    pub outline_level: Option<String>,
}

impl DocxParagraph {
    /// A plain paragraph with the given text and style name.
    pub fn new(text: &str, style_name: &str) -> Self {
        DocxParagraph {
            text: text.to_string(),
            style: Some(DocxStyle::named(style_name)),
            runs: vec![DocxRun {
                text: text.to_string(),
                ..DocxRun::default()
            }],
            ..DocxParagraph::default()
        }
    }
}

/// A table cell element (`w:tc`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxTc {
    /// `cell.text`: the cell's paragraphs' text joined by `"\n"`.
    pub text: String,
    /// The cell's paragraphs (direct `w:p` children), in order.
    pub paragraphs: Vec<DocxParagraph>,
    /// `w:tcPr/w:gridSpan/@w:val` (raw).
    pub grid_span: Option<String>,
    /// `w:tcPr/w:vMerge`: `None` when absent, else its `w:val` (`"continue"` when the
    /// attribute is omitted).
    pub v_merge: Option<String>,
}

/// A table row (`w:tr`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxRow {
    /// `w:trPr/w:gridBefore/@w:val` (raw).
    pub grid_before: Option<String>,
    /// The `w:tc` elements of the row.
    pub tcs: Vec<DocxTc>,
}

/// A table (`w:tbl`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxTable {
    /// `table.rows`.
    pub rows: Vec<DocxRow>,
    /// `len(table.columns)`: the `w:gridCol` count (`None` when `w:tblGrid` is missing,
    /// which python-docx rejects).
    pub column_count: Option<usize>,
}

impl DocxTable {
    /// A simple table of text cells.
    pub fn from_rows(rows: &[Vec<&str>]) -> Self {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        DocxTable {
            rows: rows
                .iter()
                .map(|r| DocxRow {
                    grid_before: None,
                    tcs: r
                        .iter()
                        .map(|t| DocxTc {
                            text: t.to_string(),
                            paragraphs: vec![DocxParagraph::new(t, "Normal")],
                            ..DocxTc::default()
                        })
                        .collect(),
                })
                .collect(),
            column_count: Some(width),
        }
    }

    /// `row.cells` for row `row_idx`: each `w:tc` repeated for its grid span, and
    /// vertically merged continuation cells resolved to the cell above (python-docx's
    /// `_Row.cells`). Errors carry python-docx's exception text.
    pub fn row_cells(&self, row_idx: usize) -> Result<Vec<&DocxTc>, String> {
        let row = &self.rows[row_idx];
        let mut out = Vec::new();
        for tc_idx in 0..row.tcs.len() {
            let resolved = self.resolve_tc(row_idx, tc_idx)?;
            for _ in 0..py_range_len(grid_span(resolved)?) {
                out.push(resolved);
            }
        }
        Ok(out)
    }

    fn resolve_tc(&self, row_idx: usize, tc_idx: usize) -> Result<&DocxTc, String> {
        let tc = &self.rows[row_idx].tcs[tc_idx];
        if tc.v_merge.as_deref() != Some("continue") {
            return Ok(tc);
        }
        if row_idx == 0 {
            return Err("no tr above topmost tr in w:tbl".to_string());
        }
        let offset = self.grid_offset(row_idx, tc_idx)?;
        let above = &self.rows[row_idx - 1];
        let mut remaining = offset - py_int(above.grid_before.as_deref(), 0)?;
        for (i, candidate) in above.tcs.iter().enumerate() {
            if remaining < 0 {
                break;
            }
            if remaining == 0 {
                return self.resolve_tc(row_idx - 1, i);
            }
            remaining -= grid_span(candidate)?;
        }
        Err(format!("no `tc` element at grid_offset={offset}"))
    }

    fn grid_offset(&self, row_idx: usize, tc_idx: usize) -> Result<i64, String> {
        let row = &self.rows[row_idx];
        let mut offset = py_int(row.grid_before.as_deref(), 0)?;
        for tc in &row.tcs[..tc_idx] {
            offset += grid_span(tc)?;
        }
        Ok(offset)
    }
}

fn py_range_len(n: i64) -> usize {
    if n > 0 { n as usize } else { 0 }
}

fn grid_span(tc: &DocxTc) -> Result<i64, String> {
    py_int(tc.grid_span.as_deref(), 1)
}

/// `int(value)` for an XML attribute (`default` when absent).
pub(crate) fn py_int(value: Option<&str>, default: i64) -> Result<i64, String> {
    let Some(raw) = value else {
        return Ok(default);
    };
    let s = raw.trim();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let valid = !digits.is_empty()
        && !digits.starts_with('_')
        && !digits.ends_with('_')
        && !digits.contains("__")
        && digits.chars().all(|c| c.is_ascii_digit() || c == '_');
    if !valid {
        return Err(format!(
            "invalid literal for int() with base 10: {}",
            sem_utils::pycompat::fmt::str_repr(raw)
        ));
    }
    let clean: String = digits.chars().filter(|c| *c != '_').collect();
    let n: i64 = clean.parse().map_err(|_| "Python int too large".to_string())?;
    Ok(if neg { -n } else { n })
}

/// A block in the document body, in document order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocxBlock {
    /// Index into [`DocxDocument::paragraphs`].
    Paragraph(usize),
    /// Index into [`DocxDocument::tables`].
    Table(usize),
}

/// `document.core_properties`. Text properties are `""` when absent, as python-docx
/// returns them; date properties keep the element's raw text.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxCoreProperties {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub comments: String,
    pub category: String,
    pub last_modified_by: String,
    /// `dcterms:created`: `None` when absent, `Some(None)` for an empty element, else its
    /// W3CDTF text.
    pub created: Option<Option<String>>,
    /// `dcterms:modified` (same encoding as `created`).
    pub modified: Option<Option<String>>,
}

/// A loaded Word document.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocxDocument {
    /// `document.paragraphs` (body-level `w:p` elements).
    pub paragraphs: Vec<DocxParagraph>,
    /// `document.tables` (body-level `w:tbl` elements).
    pub tables: Vec<DocxTable>,
    /// `document.core_properties`.
    pub core_properties: DocxCoreProperties,
    /// Body blocks in document order (empty for hand-built documents that only set
    /// `paragraphs`/`tables`).
    pub body: Vec<DocxBlock>,
    /// The numbering definitions: `numId` -> level -> number format (`"bullet"`,
    /// `"decimal"`, ...), for list detection.
    pub numbering: Vec<(String, Vec<(String, String)>)>,
}

impl DocxDocument {
    /// A document with the given paragraphs and core-property title.
    pub fn from_paragraphs(paragraphs: Vec<DocxParagraph>, title: &str) -> Self {
        let body = (0..paragraphs.len()).map(DocxBlock::Paragraph).collect();
        DocxDocument {
            paragraphs,
            core_properties: DocxCoreProperties {
                title: title.to_string(),
                ..DocxCoreProperties::default()
            },
            body,
            ..DocxDocument::default()
        }
    }
}

// ----------------------------------------------------------------------------- loading

struct StyleDef {
    style_type: Option<String>,
    style_id: Option<String>,
    name: Option<String>,
    default: Option<String>,
}

struct Styles {
    defs: Vec<StyleDef>,
}

/// `ST_OnOff` conversion (python-docx raises `InvalidXmlError` for other values).
pub(crate) fn st_on_off(value: &str) -> Result<bool, String> {
    match value {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        other => Err(format!(
            "value must be one of '1', '0', 'true', 'false', 'on', or 'off', got '{other}'"
        )),
    }
}

/// `BabelFish.internal2ui`.
fn internal2ui(name: &str) -> String {
    match name {
        "caption" => "Caption".to_string(),
        "footer" => "Footer".to_string(),
        "header" => "Header".to_string(),
        n if n.len() == 9 && n.starts_with("heading ") && n.as_bytes()[8].is_ascii_digit() && n.as_bytes()[8] != b'0' => {
            format!("Heading {}", &n[8..])
        }
        other => other.to_string(),
    }
}

/// `WD_STYLE_TYPE.from_xml` (raises for unknown values).
fn style_type(value: &str) -> Result<&'static str, String> {
    match value {
        "character" => Ok("character"),
        "numbering" => Ok("numbering"),
        "paragraph" => Ok("paragraph"),
        "table" => Ok("table"),
        other => Err(format!("WD_STYLE_TYPE has no XML mapping for '{other}'")),
    }
}

impl Styles {
    fn from_xml(root: Option<&Element>) -> Styles {
        let Some(root) = root else {
            // python-docx adds its default styles part (whose only paragraph style is the
            // default "Normal") when the document has none.
            return Styles {
                defs: vec![StyleDef {
                    style_type: Some("paragraph".to_string()),
                    style_id: Some("Normal".to_string()),
                    name: Some("Normal".to_string()),
                    default: Some("1".to_string()),
                }],
            };
        };
        let defs = root
            .children_named(W, "style")
            .map(|s| StyleDef {
                style_type: s.attr(Some(W), "type").map(str::to_string),
                style_id: s.attr(Some(W), "styleId").map(str::to_string),
                name: s
                    .child(W, "name")
                    .and_then(|n| n.attr(Some(W), "val"))
                    .map(str::to_string),
                default: s.attr(Some(W), "default").map(str::to_string),
            })
            .collect();
        Styles { defs }
    }

    fn def_type(def: &StyleDef) -> Result<Option<&'static str>, String> {
        def.style_type.as_deref().map(style_type).transpose()
    }

    /// `Styles.default(WD_STYLE_TYPE.PARAGRAPH)`: the last default paragraph style.
    fn default_paragraph(&self) -> Result<Option<&StyleDef>, String> {
        let mut found = None;
        for def in &self.defs {
            if Self::def_type(def)? == Some("paragraph") {
                let is_default = match def.default.as_deref() {
                    Some(v) => st_on_off(v)?,
                    None => false,
                };
                if is_default {
                    found = Some(def);
                }
            }
        }
        Ok(found)
    }

    /// `part.get_style(style_id, WD_STYLE_TYPE.PARAGRAPH)`.
    fn paragraph_style(&self, style_id: Option<&str>) -> Result<Option<DocxStyle>, String> {
        let def = match style_id.filter(|s| !s.is_empty()) {
            None => self.default_paragraph()?,
            Some(id) => {
                let found = self.defs.iter().find(|d| d.style_id.as_deref() == Some(id));
                match found {
                    Some(d) if Self::def_type(d)? == Some("paragraph") => Some(d),
                    _ => self.default_paragraph()?,
                }
            }
        };
        Ok(def.map(|d| DocxStyle {
            style_id: d.style_id.clone(),
            name: d.name.as_deref().map(internal2ui),
        }))
    }
}

fn on_off_child(rpr: Option<&Element>, name: &str) -> Option<Option<String>> {
    rpr?.child(W, name)
        .map(|e| e.attr(Some(W), "val").map(str::to_string))
}

/// `CT_R.text`.
fn run_text(r: &Element) -> String {
    let mut out = String::new();
    for e in r.elements() {
        if e.ns.as_deref() != Some(W) {
            continue;
        }
        match e.name.as_str() {
            "t" => out.push_str(e.text().unwrap_or("")),
            "tab" | "ptab" => out.push('\t'),
            "cr" => out.push('\n'),
            "noBreakHyphen" => out.push('-'),
            "br" => {
                if e.attr(Some(W), "type").unwrap_or("textWrapping") == "textWrapping" {
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
    out
}

fn run_from(r: &Element) -> DocxRun {
    let rpr = r.child(W, "rPr");
    DocxRun {
        text: run_text(r),
        bold: on_off_child(rpr, "b"),
        italic: on_off_child(rpr, "i"),
        underline: on_off_child(rpr, "u"),
        font_name: rpr
            .and_then(|p| p.child(W, "rFonts"))
            .and_then(|f| f.attr(Some(W), "ascii"))
            .map(str::to_string),
        font_size: rpr
            .and_then(|p| p.child(W, "sz"))
            .and_then(|s| s.attr(Some(W), "val"))
            .map(str::to_string),
    }
}

fn paragraph_from(p: &Element, styles: &Styles) -> Result<DocxParagraph, String> {
    let mut text = String::new();
    let mut runs = Vec::new();
    for child in p.elements() {
        if child.is(W, "r") {
            text.push_str(&run_text(child));
            runs.push(run_from(child));
        } else if child.is(W, "hyperlink") {
            for r in child.children_named(W, "r") {
                text.push_str(&run_text(r));
            }
        }
    }
    let ppr = p.child(W, "pPr");
    let style_id = ppr
        .and_then(|pp| pp.child(W, "pStyle"))
        .and_then(|s| s.attr(Some(W), "val"));
    let num_pr = ppr.and_then(|pp| pp.child(W, "numPr")).map(|n| {
        (
            n.child(W, "numId")
                .and_then(|e| e.attr(Some(W), "val"))
                .map(str::to_string),
            n.child(W, "ilvl")
                .and_then(|e| e.attr(Some(W), "val"))
                .map(str::to_string),
        )
    });
    Ok(DocxParagraph {
        text,
        style: styles.paragraph_style(style_id)?,
        alignment: ppr
            .and_then(|pp| pp.child(W, "jc"))
            .and_then(|j| j.attr(Some(W), "val"))
            .map(str::to_string),
        runs,
        num_pr,
        outline_level: ppr
            .and_then(|pp| pp.child(W, "outlineLvl"))
            .and_then(|o| o.attr(Some(W), "val"))
            .map(str::to_string),
    })
}

fn table_from(tbl: &Element, styles: &Styles) -> Result<DocxTable, String> {
    let column_count = tbl
        .child(W, "tblGrid")
        .map(|g| g.children_named(W, "gridCol").count());
    let mut rows = Vec::new();
    for tr in tbl.children_named(W, "tr") {
        let grid_before = tr
            .child(W, "trPr")
            .and_then(|t| t.child(W, "gridBefore"))
            .and_then(|g| g.attr(Some(W), "val"))
            .map(str::to_string);
        let mut tcs = Vec::new();
        for tc in tr.children_named(W, "tc") {
            let tcpr = tc.child(W, "tcPr");
            let paragraphs = tc
                .children_named(W, "p")
                .map(|p| paragraph_from(p, styles))
                .collect::<Result<Vec<_>, _>>()?;
            let text = paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            tcs.push(DocxTc {
                text,
                paragraphs,
                grid_span: tcpr
                    .and_then(|p| p.child(W, "gridSpan"))
                    .and_then(|g| g.attr(Some(W), "val"))
                    .map(str::to_string),
                v_merge: tcpr.and_then(|p| p.child(W, "vMerge")).map(|v| {
                    v.attr(Some(W), "val").unwrap_or("continue").to_string()
                }),
            });
        }
        rows.push(DocxRow { grid_before, tcs });
    }
    Ok(DocxTable { rows, column_count })
}

fn text_of(root: &Element, ns: &str, name: &str) -> String {
    root.child(ns, name)
        .and_then(Element::text)
        .unwrap_or("")
        .to_string()
}

fn date_of(root: &Element, name: &str) -> Option<Option<String>> {
    root.child(DCTERMS, name)
        .map(|e| e.text().map(str::to_string))
}

/// Core properties from `docProps/core.xml`, or python-docx's defaults when the package
/// has none (`CorePropertiesPart.default`: title "Word Document", last modified by
/// "python-docx", modified now).
fn core_properties(package: &Package) -> DocxCoreProperties {
    let part = package
        .related(&package.rels, rt::CORE_PROPERTIES)
        .ok()
        .and_then(|name| package.part(&name))
        .and_then(|p| p.xml.as_ref());
    match part {
        Some(root) => DocxCoreProperties {
            title: text_of(root, DC, "title"),
            author: text_of(root, DC, "creator"),
            subject: text_of(root, DC, "subject"),
            keywords: text_of(root, CP, "keywords"),
            comments: text_of(root, DC, "description"),
            category: text_of(root, CP, "category"),
            last_modified_by: text_of(root, CP, "lastModifiedBy"),
            created: date_of(root, "created"),
            modified: date_of(root, "modified"),
        },
        None => DocxCoreProperties {
            title: "Word Document".to_string(),
            last_modified_by: "python-docx".to_string(),
            modified: Some(Some(
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            )),
            ..DocxCoreProperties::default()
        },
    }
}

fn numbering(package: &Package, doc_part: &str) -> Vec<(String, Vec<(String, String)>)> {
    let Some(part) = package.part(doc_part) else {
        return Vec::new();
    };
    let Ok(name) = package.related(&part.rels, rt::NUMBERING) else {
        return Vec::new();
    };
    let Some(root) = package.part(&name).and_then(|p| p.xml.as_ref()) else {
        return Vec::new();
    };
    let mut abstract_formats: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for an in root.children_named(W, "abstractNum") {
        let id = an.attr(Some(W), "abstractNumId").unwrap_or("").to_string();
        let levels = an
            .children_named(W, "lvl")
            .map(|l| {
                (
                    l.attr(Some(W), "ilvl").unwrap_or("0").to_string(),
                    l.child(W, "numFmt")
                        .and_then(|f| f.attr(Some(W), "val"))
                        .unwrap_or("")
                        .to_string(),
                )
            })
            .collect();
        abstract_formats.push((id, levels));
    }
    root.children_named(W, "num")
        .map(|n| {
            let num_id = n.attr(Some(W), "numId").unwrap_or("").to_string();
            let abs = n
                .child(W, "abstractNumId")
                .and_then(|a| a.attr(Some(W), "val"))
                .unwrap_or("");
            let levels = abstract_formats
                .iter()
                .find(|(id, _)| id == abs)
                .map(|(_, l)| l.clone())
                .unwrap_or_default();
            (num_id, levels)
        })
        .collect()
}

/// `docx.Document(path)`: load a Word document. Errors carry the text of the exception
/// python-docx raises (e.g. `Package not found at '<path>'`).
#[allow(non_snake_case)]
pub fn Document(path: &Path) -> Result<DocxDocument, String> {
    let display = crate::common::py_path_str(path);
    let package = Package::open(&crate::common::py_fs_path(path), &display, Flavor::Docx)?;
    let main = package.main_document_part()?;
    let part = package
        .part(&main)
        .ok_or_else(|| format!("no part named '{main}'"))?;
    if part.content_type != ct::WML_DOCUMENT_MAIN {
        return Err(format!(
            "file '{display}' is not a Word file, content type is '{}'",
            part.content_type
        ));
    }
    let root = part
        .xml
        .as_ref()
        .ok_or_else(|| "document part has no XML".to_string())?;
    let styles_root = package
        .related(&part.rels, rt::STYLES)
        .ok()
        .and_then(|name| package.part(&name))
        .and_then(|p| p.xml.as_ref());
    let styles = Styles::from_xml(styles_root);
    let body = root
        .child(W, "body")
        .ok_or_else(|| "required ``<w:body>`` child element not present".to_string())?;
    let mut doc = DocxDocument {
        core_properties: core_properties(&package),
        numbering: numbering(&package, &main),
        ..DocxDocument::default()
    };
    for child in &body.children {
        let Node::Element(e) = child else { continue };
        if e.is(W, "p") {
            doc.body.push(DocxBlock::Paragraph(doc.paragraphs.len()));
            doc.paragraphs.push(paragraph_from(e, &styles)?);
        } else if e.is(W, "tbl") {
            doc.body.push(DocxBlock::Table(doc.tables.len()));
            doc.tables.push(table_from(e, &styles)?);
        }
    }
    Ok(doc)
}

// ------------------------------------------------------------ python-docx conversions

/// `WD_PARAGRAPH_ALIGNMENT.from_xml(value)` as `(NAME, value)`.
pub(crate) fn paragraph_alignment(xml: &str) -> Result<(&'static str, i64), String> {
    Ok(match xml {
        "left" => ("LEFT", 0),
        "center" => ("CENTER", 1),
        "right" => ("RIGHT", 2),
        "both" => ("JUSTIFY", 3),
        "distribute" => ("DISTRIBUTE", 4),
        "mediumKashida" => ("JUSTIFY_MED", 5),
        "highKashida" => ("JUSTIFY_HI", 7),
        "lowKashida" => ("JUSTIFY_LOW", 8),
        "thaiDistribute" => ("THAI_JUSTIFY", 9),
        other => {
            return Err(format!(
                "WD_PARAGRAPH_ALIGNMENT has no XML mapping for '{other}'"
            ));
        }
    })
}

/// `WD_UNDERLINE.from_xml(value)` as the member's integer value.
pub(crate) fn underline_value(xml: &str) -> Result<i64, String> {
    Ok(match xml {
        "none" => 0,
        "single" => 1,
        "words" => 2,
        "double" => 3,
        "dotted" => 4,
        "thick" => 6,
        "dash" => 7,
        "dotDash" => 9,
        "dotDotDash" => 10,
        "wave" => 11,
        "dottedHeavy" => 20,
        "dashedHeavy" => 23,
        "dashDotHeavy" => 25,
        "dashDotDotHeavy" => 26,
        "wavyHeavy" => 27,
        "dashLong" => 39,
        "wavyDouble" => 43,
        "dashLongHeavy" => 55,
        other => return Err(format!("WD_UNDERLINE has no XML mapping for '{other}'")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tc(text: &str, span: Option<&str>, vmerge: Option<&str>) -> DocxTc {
        DocxTc {
            text: text.to_string(),
            paragraphs: Vec::new(),
            grid_span: span.map(str::to_string),
            v_merge: vmerge.map(str::to_string),
        }
    }

    #[test]
    fn row_cells_follow_python_docx() {
        let table = DocxTable {
            rows: vec![
                DocxRow {
                    grid_before: None,
                    tcs: vec![tc("a", Some("2"), Some("restart")), tc("b", None, None)],
                },
                DocxRow {
                    grid_before: None,
                    tcs: vec![tc("", Some("2"), Some("continue")), tc("c", None, None)],
                },
            ],
            column_count: Some(3),
        };
        let r0: Vec<&str> = table.row_cells(0).unwrap().iter().map(|c| c.text.as_str()).collect();
        assert_eq!(r0, ["a", "a", "b"]);
        let r1: Vec<&str> = table.row_cells(1).unwrap().iter().map(|c| c.text.as_str()).collect();
        assert_eq!(r1, ["a", "a", "c"]);
        let bad = DocxTable {
            rows: vec![DocxRow {
                grid_before: None,
                tcs: vec![tc("x", None, Some("continue"))],
            }],
            column_count: Some(1),
        };
        assert_eq!(bad.row_cells(0).unwrap_err(), "no tr above topmost tr in w:tbl");
    }

    #[test]
    fn babelfish_and_conversions() {
        assert_eq!(internal2ui("heading 1"), "Heading 1");
        assert_eq!(internal2ui("heading 10"), "heading 10");
        assert_eq!(internal2ui("Title"), "Title");
        assert_eq!(paragraph_alignment("both").unwrap(), ("JUSTIFY", 3));
        assert!(paragraph_alignment("start").is_err());
        assert_eq!(st_on_off("on"), Ok(true));
        assert!(st_on_off("yes").is_err());
        assert_eq!(py_int(Some(" 2 "), 1), Ok(2));
        assert!(py_int(Some("x"), 1).is_err());
    }
}
