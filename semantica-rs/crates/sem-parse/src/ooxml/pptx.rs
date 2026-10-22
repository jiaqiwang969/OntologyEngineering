//! A presentation as python-pptx 1.0.2 exposes it (`pptx.Presentation(path)`): slides in
//! presentation order, each slide's top-level shapes (`slide.shapes`) with the attributes
//! `PPTXParser` reads, the notes slide, and the core properties.
//!
//! Attribute access that python-pptx performs lazily (and that can raise) is recorded per
//! shape as the outcome the parser observes: `hasattr(shape, "text")`, `shape.shape_type`,
//! `hasattr(shape, "image")` and `shape.image.ext`, `notes_slide.notes_text_frame`.

use std::path::Path;

use super::dom::Element;
use super::opc::{self, Flavor, Package, ct, rt};

pub(crate) const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub(crate) const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub(crate) const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CP: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const DCTERMS: &str = "http://purl.org/dc/terms/";

const URI_CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const URI_TABLE: &str = "http://schemas.openxmlformats.org/drawingml/2006/table";
const URI_OLEOBJ: &str = "http://schemas.openxmlformats.org/presentationml/2006/ole";

/// The element kind of a shape in `p:spTree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeKind {
    /// `p:sp` (auto shape, text box, freeform or placeholder).
    Sp,
    /// `p:pic` (picture).
    Pic,
    /// `p:pic` holding a video (`Movie`).
    Movie,
    /// `p:graphicFrame` (table, chart, OLE object, diagram).
    GraphicFrame,
    /// `p:grpSp`.
    Group,
    /// `p:cxnSp`.
    Connector,
    /// `p:contentPart` (python-pptx's bare `BaseShape`).
    Other,
}

/// What `hasattr(shape, "image")` and `shape.image.ext` give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImageAttr {
    /// The shape class has no `image` attribute.
    Absent,
    /// Evaluating `shape.image` raises (propagates out of `hasattr`).
    Raises(String),
    /// `shape.image` exists; `ext` is `shape.image.ext` (`Err` when PIL cannot identify a
    /// supported format).
    Present { ext: Result<String, String> },
}

/// A shape on a slide.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PptxShape {
    pub kind: ShapeKind,
    /// `p:cNvPr/@name`.
    pub name: String,
    /// The `p:ph` element as `(type, idx)` raw attribute values, for placeholders.
    pub ph: Option<(Option<String>, Option<String>)>,
    /// `shape.text` for shapes that have it (`p:sp`), else `None`.
    pub text: Option<String>,
    /// Text paragraphs as `(level, text)` (`a:pPr/@lvl`), for `p:sp` shapes.
    pub paragraphs: Vec<(i64, String)>,
    /// `shape.shape_type` as the `MSO_SHAPE_TYPE` value (`Ok(None)` when python-pptx returns
    /// `None`), or the exception text.
    pub shape_type: Result<Option<i64>, String>,
    /// `shape.width` / `shape.height` in EMU (inherited for placeholder pictures).
    pub width: Result<Option<i64>, String>,
    pub height: Result<Option<i64>, String>,
    pub image: ImageAttr,
    /// Table cell text (`a:tbl` rows of `a:tc` text), for table graphic frames.
    pub table: Option<Vec<Vec<String>>>,
    /// Child shapes of a group.
    pub children: Vec<PptxShape>,
}

/// A slide.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PptxSlide {
    pub shapes: Vec<PptxShape>,
    /// `None` without a notes slide; else `notes_slide.notes_text_frame.text`
    /// (`Ok(None)` when the notes slide has no body placeholder, `Err` when looking it up
    /// raises).
    pub notes: Option<Result<Option<String>, String>>,
}

/// `prs.core_properties` (text values are `""` when absent; dates are `str(datetime)`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PptxCoreProperties {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub comments: String,
    pub last_modified_by: String,
    pub created: Option<String>,
    pub modified: Option<String>,
}

/// A loaded presentation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PptxPresentation {
    pub slides: Vec<PptxSlide>,
    pub core_properties: PptxCoreProperties,
    /// `prs.slide_width` / `prs.slide_height` (EMU).
    pub slide_width: Option<i64>,
    pub slide_height: Option<i64>,
}

/// `PP_PLACEHOLDER.from_xml` for the `p:ph/@type` value (default `obj`).
fn ph_type_valid(value: &str) -> Result<(), String> {
    match value {
        "body" | "chart" | "clipArt" | "ctrTitle" | "dgm" | "dt" | "ftr" | "hdr" | "media"
        | "obj" | "pic" | "sldImg" | "sldNum" | "subTitle" | "tbl" | "title" => Ok(()),
        other => Err(format!("PP_PLACEHOLDER has no XML mapping for '{other}'")),
    }
}

/// `XsdBoolean` conversion of `cNvSpPr/@txBox`.
fn xsd_boolean(value: &str) -> Result<bool, String> {
    match value {
        "1" | "true" => Ok(true),
        "0" | "false" => Ok(false),
        other => Err(format!(
            "value must be one of '1', '0', 'true' or 'false', got '{other}'"
        )),
    }
}

/// The first child element (`p:nvSpPr`, `p:nvPicPr`, ...).
fn nv_pr(el: &Element) -> Option<&Element> {
    el.elements().next()
}

fn ph_of(el: &Element) -> Option<&Element> {
    nv_pr(el)?.child(P, "nvPr")?.child(P, "ph")
}

fn shape_kind(el: &Element) -> Option<ShapeKind> {
    if el.ns.as_deref() != Some(P) {
        return None;
    }
    Some(match el.name.as_str() {
        "sp" => ShapeKind::Sp,
        "pic" => {
            let video = el
                .child(P, "nvPicPr")
                .and_then(|n| n.child(P, "nvPr"))
                .and_then(|n| n.child(A, "videoFile"));
            if video.is_some() {
                ShapeKind::Movie
            } else {
                ShapeKind::Pic
            }
        }
        "graphicFrame" => ShapeKind::GraphicFrame,
        "grpSp" => ShapeKind::Group,
        "cxnSp" => ShapeKind::Connector,
        "contentPart" => ShapeKind::Other,
        _ => return None,
    })
}

/// `p:spTree` shape elements in document order.
pub(crate) fn shape_elements(sp_tree: &Element) -> Vec<&Element> {
    sp_tree
        .elements()
        .filter(|e| shape_kind(e).is_some())
        .collect()
}

/// `TextFrame.text` paragraphs of a `p:txBody`.
fn text_paragraphs(tx_body: Option<&Element>) -> Vec<(i64, String)> {
    let Some(body) = tx_body else {
        // `get_or_add_txBody()` creates an empty text frame.
        return vec![(0, String::new())];
    };
    body.children_named(A, "p")
        .map(|p| {
            let mut text = String::new();
            for child in p.elements() {
                if child.ns.as_deref() != Some(A) {
                    continue;
                }
                match child.name.as_str() {
                    "r" | "fld" => {
                        text.push_str(child.child(A, "t").and_then(Element::text).unwrap_or(""))
                    }
                    "br" => text.push('\u{b}'),
                    _ => {}
                }
            }
            let level = p
                .child(A, "pPr")
                .and_then(|pp| pp.attr(None, "lvl"))
                .and_then(|l| l.parse().ok())
                .unwrap_or(0);
            (level, text)
        })
        .collect()
}

fn xfrm_of(el: &Element, kind: ShapeKind) -> Option<&Element> {
    match kind {
        ShapeKind::GraphicFrame => el.child(P, "xfrm"),
        ShapeKind::Group => el.child(P, "grpSpPr")?.child(A, "xfrm"),
        _ => el.child(P, "spPr")?.child(A, "xfrm"),
    }
}

/// `Emu(int(value))` of `xfrm/a:ext/@cx|cy`.
fn ext_attr(el: &Element, kind: ShapeKind, name: &str) -> Result<Option<i64>, String> {
    let Some(value) = xfrm_of(el, kind)
        .and_then(|x| x.child(A, "ext"))
        .and_then(|e| e.attr(None, name))
    else {
        return Ok(None);
    };
    super::docx::py_int(Some(value), 0).map(Some)
}

/// PIL's format for an image blob, mapped to python-pptx's canonical extension.
pub(crate) fn image_ext(blob: &[u8]) -> Result<String, String> {
    let format = if blob.starts_with(b"\x89PNG\r\n\x1a\n") && blob.len() >= 24 {
        "PNG"
    } else if blob.starts_with(&[0xff, 0xd8, 0xff]) {
        "JPEG"
    } else if blob.starts_with(b"GIF87a") || blob.starts_with(b"GIF89a") {
        "GIF"
    } else if blob.starts_with(b"BM") && blob.len() >= 26 {
        "BMP"
    } else if blob.starts_with(b"II*\0") || blob.starts_with(b"MM\0*") {
        "TIFF"
    } else if blob.starts_with(&[0xd7, 0xcd, 0xc6, 0x9a, 0, 0]) || blob.starts_with(&[1, 0, 0, 0]) {
        "WMF"
    } else if blob.len() >= 12 && &blob[..4] == b"RIFF" && &blob[8..12] == b"WEBP" {
        "WEBP"
    } else {
        return Err("cannot identify image file".to_string());
    };
    match format {
        "BMP" => Ok("bmp".to_string()),
        "GIF" => Ok("gif".to_string()),
        "JPEG" => Ok("jpg".to_string()),
        "PNG" => Ok("png".to_string()),
        "TIFF" => Ok("tiff".to_string()),
        "WMF" => Ok("wmf".to_string()),
        other => Err(format!(
            "unsupported image format, expected one of: dict_keys(['BMP', 'GIF', 'JPEG', 'PNG', 'TIFF', 'WMF']), got '{other}'"
        )),
    }
}

/// Context for resolving inherited placeholder dimensions: the slide layout and master.
struct Inheritance<'a> {
    layout: Option<&'a Element>,
    master: Option<&'a Element>,
}

fn sp_tree(part_root: &Element) -> Option<&Element> {
    part_root.child(P, "cSld")?.child(P, "spTree")
}

impl Inheritance<'_> {
    /// `LayoutPlaceholder` dimension for a slide placeholder `idx`, falling back to the
    /// master placeholder of the mapped type.
    fn dimension(&self, idx: i64, attr: &str) -> Result<Option<i64>, String> {
        let Some(layout) = self.layout.and_then(sp_tree) else {
            return Ok(None);
        };
        for el in shape_elements(layout) {
            let Some(ph) = ph_of(el) else { continue };
            let el_idx = super::docx::py_int(ph.attr(None, "idx"), 0)?;
            if el_idx != idx {
                continue;
            }
            let kind = shape_kind(el).unwrap_or(ShapeKind::Other);
            if let Some(v) = ext_attr(el, kind, attr)? {
                return Ok(Some(v));
            }
            if kind != ShapeKind::Sp {
                return Ok(None);
            }
            // LayoutPlaceholder inherits from the master placeholder of its base type.
            let ph_type = ph.attr(None, "type").unwrap_or("obj");
            let base = match ph_type {
                "ctrTitle" | "title" => "title",
                "dt" => "dt",
                "ftr" => "ftr",
                "sldNum" => "sldNum",
                _ => "body",
            };
            let Some(master) = self.master.and_then(sp_tree) else {
                return Ok(None);
            };
            for m in shape_elements(master) {
                let Some(mph) = ph_of(m) else { continue };
                if mph.attr(None, "type").unwrap_or("obj") == base {
                    let mkind = shape_kind(m).unwrap_or(ShapeKind::Other);
                    return ext_attr(m, mkind, attr);
                }
            }
            return Ok(None);
        }
        Ok(None)
    }
}

fn graphic_data(el: &Element) -> Option<&Element> {
    el.child(A, "graphic")?.child(A, "graphicData")
}

fn table_cells(el: &Element) -> Option<Vec<Vec<String>>> {
    let gd = graphic_data(el)?;
    let tbl = gd.child(A, "tbl")?;
    Some(
        tbl.children_named(A, "tr")
            .map(|tr| {
                tr.children_named(A, "tc")
                    .map(|tc| {
                        text_paragraphs(tc.child(A, "txBody"))
                            .into_iter()
                            .map(|(_, t)| t)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .collect()
            })
            .collect(),
    )
}

fn build_shape(
    package: &Package,
    part_name: &str,
    el: &Element,
    inherit: &Inheritance<'_>,
    slide_level: bool,
) -> PptxShape {
    let kind = shape_kind(el).unwrap_or(ShapeKind::Other);
    let name = nv_pr(el)
        .and_then(|n| n.child(P, "cNvPr"))
        .and_then(|c| c.attr(None, "name"))
        .unwrap_or("")
        .to_string();
    let ph_el = ph_of(el);
    let ph = ph_el.map(|p| {
        (
            p.attr(None, "type").map(str::to_string),
            p.attr(None, "idx").map(str::to_string),
        )
    });
    let is_ph = ph.is_some();
    let paragraphs = if kind == ShapeKind::Sp {
        text_paragraphs(el.child(P, "txBody"))
    } else {
        Vec::new()
    };
    let text = (kind == ShapeKind::Sp).then(|| {
        paragraphs
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    });
    let shape_type: Result<Option<i64>, String> = match kind {
        ShapeKind::Sp if is_ph && slide_level => Ok(Some(14)),
        ShapeKind::Sp => (|| {
            if is_ph {
                return Ok(Some(14));
            }
            let sppr = el.child(P, "spPr");
            if sppr.and_then(|s| s.child(A, "custGeom")).is_some() {
                return Ok(Some(5));
            }
            let tx_box = match nv_pr(el)
                .and_then(|n| n.child(P, "cNvSpPr"))
                .and_then(|c| c.attr(None, "txBox"))
            {
                Some(v) => Some(xsd_boolean(v)?),
                None => None,
            };
            let prst = sppr.and_then(|s| s.child(A, "prstGeom")).is_some();
            if prst && tx_box != Some(true) {
                return Ok(Some(1));
            }
            if tx_box == Some(true) {
                return Ok(Some(17));
            }
            Err("Shape instance of unrecognized shape type".to_string())
        })(),
        ShapeKind::Pic if is_ph && slide_level => Ok(Some(14)),
        ShapeKind::Pic => Ok(Some(13)),
        ShapeKind::Movie => Ok(Some(16)),
        ShapeKind::GraphicFrame => {
            let uri = graphic_data(el).and_then(|g| g.attr(None, "uri"));
            Ok(match uri {
                Some(URI_CHART) => Some(3),
                Some(URI_TABLE) => Some(19),
                Some(URI_OLEOBJ) => {
                    let ole = graphic_data(el).and_then(|g| g.child(P, "oleObj"));
                    match ole {
                        Some(o) if o.child(P, "embed").is_some() => Some(7),
                        Some(_) => Some(10),
                        None => Some(10),
                    }
                }
                _ => None,
            })
        }
        ShapeKind::Group => Ok(Some(6)),
        ShapeKind::Connector => Ok(Some(9)),
        ShapeKind::Other => Err("BaseShape does not implement `.shape_type`".to_string()),
    };
    let own_width = ext_attr(el, kind, "cx");
    let own_height = ext_attr(el, kind, "cy");
    let inherits = slide_level && is_ph && matches!(kind, ShapeKind::Sp | ShapeKind::Pic);
    let dimension = |own: Result<Option<i64>, String>, attr: &str| -> Result<Option<i64>, String> {
        match own {
            Ok(None) if inherits => {
                let idx = super::docx::py_int(ph.as_ref().and_then(|(_, i)| i.as_deref()), 0)?;
                inherit.dimension(idx, attr)
            }
            other => other,
        }
    };
    let width = dimension(own_width, "cx");
    let height = dimension(own_height, "cy");
    let image = if kind == ShapeKind::Pic {
        let r_id = el
            .child(P, "blipFill")
            .and_then(|b| b.child(A, "blip"))
            .and_then(|b| b.attr(Some(R), "embed"));
        match r_id {
            None => ImageAttr::Raises("no embedded image".to_string()),
            Some(r_id) => match package.related_by_id(part_name, r_id) {
                None => ImageAttr::Raises(sem_utils::pycompat::fmt::str_repr(&format!(
                    "no relationship with key '{r_id}'"
                ))),
                Some(target) => {
                    let blob = package.part(target).map(|p| p.blob.as_slice()).unwrap_or(&[]);
                    ImageAttr::Present {
                        ext: image_ext(blob),
                    }
                }
            },
        }
    } else {
        ImageAttr::Absent
    };
    let table = if kind == ShapeKind::GraphicFrame {
        table_cells(el)
    } else {
        None
    };
    let children = if kind == ShapeKind::Group {
        shape_elements(el)
            .into_iter()
            .map(|c| build_shape(package, part_name, c, inherit, false))
            .collect()
    } else {
        Vec::new()
    };
    PptxShape {
        kind,
        name,
        ph,
        text,
        paragraphs,
        shape_type,
        width,
        height,
        image,
        table,
        children,
    }
}

fn notes_text(root: &Element) -> Result<Option<String>, String> {
    let Some(tree) = sp_tree(root) else {
        return Ok(None);
    };
    for el in shape_elements(tree) {
        let Some(ph) = ph_of(el) else { continue };
        let ph_type = ph.attr(None, "type").unwrap_or("obj");
        ph_type_valid(ph_type)?;
        if ph_type == "body" {
            if shape_kind(el) != Some(ShapeKind::Sp) {
                // `hasattr(notes_slide, "notes_text_frame")` is False for a non-`p:sp` body.
                return Ok(Some(String::new()));
            }
            let text = text_paragraphs(el.child(P, "txBody"))
                .into_iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(Some(text));
        }
    }
    Ok(None)
}

/// python-pptx's `_parse_W3CDTF_to_datetime` followed by `str()` (naive datetimes).
fn w3cdtf_str(text: &str) -> Option<String> {
    let parsed = super::w3cdtf::parse(text)?;
    Some(parsed.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn core_properties(package: &Package) -> PptxCoreProperties {
    let root = package
        .related(&package.rels, rt::CORE_PROPERTIES)
        .ok()
        .and_then(|name| package.part(&name))
        .and_then(|p| p.xml.as_ref());
    let Some(root) = root else {
        // `CorePropertiesPart.default`: title "PowerPoint Presentation", last modified by
        // "python-pptx", modified now (naive UTC).
        return PptxCoreProperties {
            title: "PowerPoint Presentation".to_string(),
            last_modified_by: "python-pptx".to_string(),
            modified: Some(chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            ..PptxCoreProperties::default()
        };
    };
    let text = |ns: &str, name: &str| {
        root.child(ns, name)
            .and_then(Element::text)
            .unwrap_or("")
            .to_string()
    };
    let date = |name: &str| {
        root.child(DCTERMS, name)
            .and_then(Element::text)
            .and_then(w3cdtf_str)
    };
    PptxCoreProperties {
        title: text(DC, "title"),
        author: text(DC, "creator"),
        subject: text(DC, "subject"),
        keywords: text(CP, "keywords"),
        comments: text(DC, "description"),
        last_modified_by: text(CP, "lastModifiedBy"),
        created: date("created"),
        modified: date("modified"),
    }
}

/// `pptx.Presentation(path)`: load a presentation. Errors carry the text of the exception
/// python-pptx raises.
pub(crate) fn presentation(path: &Path) -> Result<PptxPresentation, String> {
    let display = crate::common::py_path_str(path);
    let package = Package::open(&crate::common::py_fs_path(path), &display, Flavor::Pptx)?;
    let main = package.main_document_part()?;
    let part = package
        .part(&main)
        .ok_or_else(|| format!("no part named '{main}'"))?;
    if part.content_type != ct::PML_PRESENTATION_MAIN && part.content_type != ct::PML_PRES_MACRO_MAIN {
        return Err(format!(
            "file '{display}' is not a PowerPoint file, content type is '{}'",
            part.content_type
        ));
    }
    let root = part.xml.as_ref().ok_or("presentation part has no XML")?;
    let sld_sz = root.child(P, "sldSz");
    let size = |name: &str| {
        sld_sz
            .and_then(|s| s.attr(None, name))
            .and_then(|v| v.parse().ok())
    };
    let mut slides = Vec::new();
    if let Some(list) = root.child(P, "sldIdLst") {
        for sld_id in list.children_named(P, "sldId") {
            let r_id = sld_id.attr(Some(R), "id").unwrap_or("");
            let slide_name = package
                .related_by_id(&main, r_id)
                .ok_or_else(|| sem_utils::pycompat::fmt::str_repr(&format!(
                    "no relationship with key '{r_id}'"
                )))?
                .to_string();
            slides.push(load_slide(&package, &slide_name)?);
        }
    }
    Ok(PptxPresentation {
        slides,
        core_properties: core_properties(&package),
        slide_width: size("cx"),
        slide_height: size("cy"),
    })
}

fn load_slide(package: &Package, slide_name: &str) -> Result<PptxSlide, String> {
    let part = package
        .part(slide_name)
        .ok_or_else(|| format!("no part named '{slide_name}'"))?;
    let root = part.xml.as_ref().ok_or("slide part has no XML")?;
    let tree = sp_tree(root).ok_or("required ``<p:cSld>`` child element not present")?;
    let layout_name = package.related(&part.rels, rt::SLIDE_LAYOUT).ok();
    let layout = layout_name
        .as_deref()
        .and_then(|n| package.part(n))
        .and_then(|p| p.xml.as_ref());
    let master = layout_name
        .as_deref()
        .and_then(|n| package.part(n))
        .and_then(|p| package.related(&p.rels, rt::SLIDE_MASTER).ok())
        .and_then(|n| package.part(&n))
        .and_then(|p| p.xml.as_ref());
    let inherit = Inheritance { layout, master };
    let shapes = shape_elements(tree)
        .into_iter()
        .map(|el| build_shape(package, slide_name, el, &inherit, true))
        .collect();
    let notes = match package.related(&part.rels, rt::NOTES_SLIDE) {
        Ok(name) => Some(
            package
                .part(&name)
                .and_then(|p| p.xml.as_ref())
                .map(notes_text)
                .unwrap_or(Ok(None)),
        ),
        Err(e) if e.starts_with("multiple") => return Err(e),
        Err(_) => None,
    };
    Ok(PptxSlide { shapes, notes })
}

#[allow(dead_code)]
fn _uses(_: &opc::Relationship) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_formats_follow_pil_and_python_pptx() {
        assert_eq!(image_ext(b"\x89PNG\r\n\x1a\n0000000000000000"), Ok("png".to_string()));
        assert_eq!(image_ext(&[0xff, 0xd8, 0xff, 0xe0]), Ok("jpg".to_string()));
        assert_eq!(image_ext(b"GIF89a.."), Ok("gif".to_string()));
        assert!(image_ext(b"RIFF\0\0\0\0WEBPVP8 ").is_err());
        assert!(image_ext(b"<svg/>").is_err());
    }
}
