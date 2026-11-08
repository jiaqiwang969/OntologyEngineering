//! Synthesis of the pixel expression graph (design §3–§5): refined proposals become entities,
//! members and expressions in the vocabulary of the DXF route; the ink no member explains is
//! vectorised into contour, hidden and axis strokes; collinear contour pieces become structural
//! lines with implied spans across explained interruptions. Every entity carries its provenance.
//! The engine makes no model calls: the observation files are read-only inputs.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};

use crate::Ink;
use crate::calibrate::{Calibration, calibrate};
use crate::dedupe::{DedupeParams, Merge, dedupe};
use crate::geom::{P, add, dist, dot, mul, sub, unit};
use crate::observe::{
    Category, Layout, ObservationFile, Proposal, Shape, Verification, load_observation,
    load_tile_index, page_observations, sha256_hex,
};
use crate::overlay::{Frame, Overlay};
use crate::refine::{RefineParams, RefinedMember, RefinedProposal, extension_span, measure_pattern, measure_width, refine};
use crate::members::segment_support;
use crate::stroke::{Pattern, Primitive};
use crate::structure::{
    StrokeClass, StructureParams, erased_strokes, group_collinear, remaining_ink, segment_hits_box,
    segment_hits_polygon, vectorise_remainder,
};
use crate::textform::{TextMeasure, text_form, text_measure};

pub const SCHEMA: &str = "oev2.drawing-expression-graph/1";
pub const USAGE_SCHEMA: &str = "oev2.drawing-usage-graph/1";
pub const PROFILE: &str = "pixel-expression-rules/1";
pub const RULE: &str = "pixel-route/1";
const LAYER: &str = "pixel";
const IMPLIED_UNKNOWN: &str = "Inferred from a drafting convention; no source line exists between the two ends";
const LINE_IMPLIED_UNKNOWN: &str = "Part of the line is inferred across an explained interruption; no source line is drawn there";

/// Inputs of one page synthesis. All bytes are read by the caller; nothing is opened here.
pub struct PageInput<'a> {
    /// Page key of the tile index.
    pub page: &'a str,
    pub page_png: &'a [u8],
    pub tile_index: &'a [u8],
    /// Observation files as `(path label, bytes)`.
    pub observations: &'a [(String, Vec<u8>)],
    /// Tile images by name, for digest verification (any subset).
    pub tile_pngs: &'a BTreeMap<String, Vec<u8>>,
    /// Scale note overriding the L0 title-block scale (`1:1.5`).
    pub scale_note: Option<&'a str>,
    /// Grey threshold of the ink (dark below).
    pub threshold: u8,
}

/// Outputs of one page synthesis.
pub struct SynthesisOutput {
    pub graph: Vec<u8>,
    pub provenance: Vec<u8>,
    pub refined_members: Vec<u8>,
    pub calibration: Vec<u8>,
    pub overlay_svg: String,
    pub summary: Value,
    pub page_px: (u32, u32),
    pub page_sha256: String,
    /// `[0, 0, width, height]` in drawing units.
    pub drawing_rectangle: [f64; 4],
}

fn fin(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}

fn shape_json(s: &Shape) -> Value {
    let pt = |p: &P| json!([fin(p.0), fin(p.1)]);
    match s {
        Shape::Segment(p, q) => json!({"type": "segment", "p": pt(p), "q": pt(q)}),
        Shape::Tip { approx, along } => json!({"type": "tip", "p": pt(approx), "along": pt(along)}),
        Shape::TextBox { bbox, vertical } => json!({"type": "text_box", "bbox": bbox.map(fin), "vertical": vertical}),
        Shape::Point(p) => json!({"type": "point", "p": pt(p)}),
        Shape::Polyline(v) => json!({"type": "polyline", "points": v.iter().map(pt).collect::<Vec<_>>()}),
        Shape::Hatch { polygon, angle_deg, spacing_px } => json!({"type": "hatch", "polygon": polygon.iter().map(pt).collect::<Vec<_>>(), "angle_deg": fin(*angle_deg), "spacing_px": fin(*spacing_px)}),
    }
}

fn verification_json(v: Option<&Verification>) -> Value {
    match v {
        Some(v) => json!({"item_text": v.item_text, "p_proposal": fin(v.p_proposal), "top_choice": v.top_choice,
            "predicate_p": v.predicate_p.map(fin), "model": v.model, "file": v.file, "verified": v.verified()}),
        None => Value::Null,
    }
}

fn pattern_name(p: Option<Pattern>) -> &'static str {
    match p {
        Some(Pattern::Continuous) => "continuous",
        Some(Pattern::Dashed) => "dashed",
        Some(Pattern::DashDot) => "dash_dot",
        Some(Pattern::Unknown) => "unknown",
        None => "not_curve",
    }
}

/// Geometric line type of a stroke from its measured pattern and its width against the thin
/// reference width.
fn stroke_line_type(pattern: Option<Pattern>, thick: bool) -> &'static str {
    match (pattern, thick) {
        (Some(Pattern::Continuous), false) => "thin_continuous",
        (Some(Pattern::Continuous), true) => "thick_continuous",
        (Some(Pattern::Dashed), false) => "thin_dashed",
        (Some(Pattern::Dashed), true) => "thick_dashed",
        (Some(Pattern::DashDot), false) => "thin_dash_dot",
        (Some(Pattern::DashDot), true) => "thick_dash_dot",
        _ => "unknown",
    }
}

/// Expression kind, family, label and qualifiers of a proposal (design §3).
fn expression_identity(p: &Proposal) -> (&'static str, &'static str, String, Vec<String>) {
    let label = text_form(&p.text).label;
    match p.category {
        Category::Dimension => {
            let kind = match p.kind.as_str() {
                "diameter" => "dimension_diameter",
                "radius" => "dimension_radius",
                "angular" => "dimension_angle",
                "chamfer" => "dimension_chamfer",
                "leader" => "leader",
                _ => "dimension_linear",
            };
            let family = if kind == "leader" { "note" } else { "dimension" };
            let mut q = Vec::new();
            if label.contains('☆') {
                q.push("key_characteristic".to_owned());
            }
            if text_form(&p.text).stacked || label.contains('±') {
                q.push("toleranced".to_owned());
            }
            if label.contains('(') {
                q.push("reference".to_owned());
            }
            (kind, family, if label.is_empty() { "尺寸".into() } else { label }, q)
        }
        Category::SurfaceTexture => ("surface_roughness", "surface", if label.is_empty() { "粗糙度".into() } else { label }, vec![]),
        Category::GdtFrame => ("geometric_tolerance", "tolerance", if label.is_empty() { "形位公差框".into() } else { label }, vec![]),
        Category::Datum => ("datum_symbol", "tolerance", if label.is_empty() { "基准".into() } else { label }, vec![]),
        Category::CentreLine => ("center_axis", "geometry", "中心线".into(), vec![]),
        Category::Hatch => {
            let angle = p
                .members
                .iter()
                .find_map(|m| match m.shape {
                    Shape::Hatch { angle_deg, .. } => Some(angle_deg),
                    _ => None,
                })
                .unwrap_or(45.0);
            ("section_hatch", "geometry", format!("剖面线 {angle}°"), vec![])
        }
        Category::Label => {
            if p.kind == "other" {
                ("technical_general", "note", label, vec![])
            } else {
                ("view_callout", "view", label, vec![p.kind.clone()])
            }
        }
        Category::Note => {
            if p.members.iter().any(|m| m.role == "leader_line") {
                ("leader", "note", label, vec![])
            } else {
                ("technical_general", "note", label, vec![])
            }
        }
        Category::Mark => ("construction_reference", "construction", if label.is_empty() { "☆".into() } else { label }, vec!["special_mark".into()]),
        Category::BreakLine => ("break_line", "geometry", "断裂线".into(), vec![]),
        Category::TechnicalLine => ("technical_general", "note", label, vec!["general_requirement".into()]),
    }
}

#[derive(Clone, Debug)]
struct ViewRegion {
    id: String,
    label: String,
    kind: String,
    name: String,
    bbox_px: [f64; 4],
    scale_text: Option<String>,
}

/// A contour piece awaiting its structural line (pixel coordinates).
#[derive(Clone, Debug)]
struct Piece {
    entity: String,
    member: String,
    p: P,
    q: P,
    view: String,
    class: &'static str,
}

struct Builder<'a> {
    ink: &'a Ink,
    cal: &'a Calibration,
    s: f64,
    page_h: f64,
    frame: Frame,
    overlay: Overlay,
    views: Vec<ViewRegion>,
    layout: Option<&'a Layout>,
    counters: BTreeMap<String, usize>,
    entities: Vec<Value>,
    entity_members: BTreeMap<String, Vec<String>>,
    native_members: Vec<Value>,
    line_styles: Vec<Value>,
    members: Vec<Value>,
    expressions: Vec<Value>,
    expr_members: Vec<Vec<String>>,
    structural_lines: Vec<Value>,
    implied_spans: Vec<Value>,
    structure_ends: Vec<Value>,
    end_ids: BTreeSet<String>,
    unknowns: Vec<String>,
    provenance: Map<String, Value>,
    refined_records: Vec<Value>,
    // Material for the structure step.
    erase_segments: Vec<crate::Segment>,
    erase_boxes: Vec<(f64, f64, f64, f64)>,
    hatch_polygons: Vec<(Vec<P>, String)>,
    arrows: Vec<(P, P)>,
    text_boxes: Vec<([f64; 4], String)>,
    centre_segments: Vec<(P, P)>,
    thin_widths: Vec<f64>,
    thin_reference: Option<f64>,
    label_texts: Vec<(P, String)>,
    /// Line entities of annotation strokes: id, pixel endpoints, line type (for shared members).
    line_index: Vec<(String, P, P, String)>,
    shared_uses: BTreeMap<String, Vec<Value>>,
    width_contrast: Value,
    /// Tolerance around an L0 view box: one text height, at least 2 mm.
    view_margin_px: f64,
    unreadable_texts: usize,
    unread_members_omitted: usize,
    shared_members: usize,
    table_texts: usize,
    centre_axes_count: usize,
    // Counts for the summary.
    unsupported: usize,
    doubtful: usize,
    unplaced_proposals: usize,
    stroke_counts: BTreeMap<String, usize>,
}

impl<'a> Builder<'a> {
    fn new(ink: &'a Ink, cal: &'a Calibration, page_px: (u32, u32), layout: Option<&'a Layout>) -> Builder<'a> {
        let s = cal.px_per_unit;
        let (page_w, page_h) = (f64::from(page_px.0), f64::from(page_px.1));
        let frame = Frame {
            width_units: page_w / s,
            height_units: page_h / s,
        };
        let mut views = Vec::new();
        let mut ids = BTreeSet::new();
        for v in layout.iter().flat_map(|l| l.views.iter()) {
            let principal = v.name.contains("主视");
            let base_label = if v.label_text.is_empty() { v.name.clone() } else { v.label_text.clone() };
            let label = if principal {
                "主视图".to_owned()
            } else if v.scale_note.is_empty() {
                base_label.clone()
            } else {
                format!("{base_label} {}", v.scale_note)
            };
            let kind = if principal {
                "principal"
            } else if v.name.contains("局部放大") || !v.scale_note.is_empty() {
                "detail_view"
            } else if v.name.contains('剖') || v.label_text.contains('-') {
                "section_view"
            } else if v.name.contains("局部") {
                "partial_view"
            } else {
                "direction_view"
            };
            let mut id = if principal {
                "view:principal".to_owned()
            } else {
                format!("view:{}", label.replace(' ', ""))
            };
            let mut k = 2;
            while !ids.insert(id.clone()) {
                id = format!("view:{}:{k}", label.replace(' ', ""));
                k += 1;
            }
            views.push(ViewRegion {
                id,
                label: if label.is_empty() { "视图".into() } else { label },
                kind: kind.into(),
                name: v.name.clone(),
                bbox_px: v.bbox,
                scale_text: Some(v.scale_note.clone()).filter(|n| !n.is_empty()),
            });
        }
        Builder {
            ink,
            cal,
            s,
            page_h,
            frame,
            overlay: Overlay::default(),
            views,
            layout,
            counters: BTreeMap::new(),
            entities: Vec::new(),
            entity_members: BTreeMap::new(),
            native_members: Vec::new(),
            line_styles: Vec::new(),
            members: Vec::new(),
            expressions: Vec::new(),
            expr_members: Vec::new(),
            structural_lines: Vec::new(),
            implied_spans: Vec::new(),
            structure_ends: Vec::new(),
            end_ids: BTreeSet::new(),
            unknowns: Vec::new(),
            provenance: Map::new(),
            refined_records: Vec::new(),
            erase_segments: Vec::new(),
            erase_boxes: Vec::new(),
            hatch_polygons: Vec::new(),
            arrows: Vec::new(),
            text_boxes: Vec::new(),
            centre_segments: Vec::new(),
            thin_widths: Vec::new(),
            thin_reference: None,
            label_texts: Vec::new(),
            line_index: Vec::new(),
            shared_uses: BTreeMap::new(),
            width_contrast: Value::Null,
            view_margin_px: 2.0 * 300.0 / 25.4,
            unreadable_texts: 0,
            unread_members_omitted: 0,
            shared_members: 0,
            table_texts: 0,
            centre_axes_count: 0,
            unsupported: 0,
            doubtful: 0,
            unplaced_proposals: 0,
            stroke_counts: BTreeMap::new(),
        }
    }

    fn to_drawing(&self, p: P) -> P {
        self.cal.to_drawing(p, self.page_h)
    }

    /// The innermost L0 view containing a page point (within one text height, at least 2 mm), else
    /// the sheet.
    fn view_of(&self, p: P) -> String {
        let tol = self.view_margin_px;
        let mut best: Option<(f64, &ViewRegion)> = None;
        for v in &self.views {
            let b = v.bbox_px;
            if p.0 >= b[0] - tol && p.0 <= b[2] + tol && p.1 >= b[1] - tol && p.1 <= b[3] + tol {
                let area = (b[2] - b[0]) * (b[3] - b[1]);
                if best.is_none_or(|(a, _)| area < a) {
                    best = Some((area, v));
                }
            }
        }
        best.map_or_else(|| "view:sheet".to_owned(), |(_, v)| v.id.clone())
    }

    fn view_label(&self, id: &str) -> String {
        self.views
            .iter()
            .find(|v| v.id == id)
            .map_or_else(|| "图纸".to_owned(), |v| v.label.clone())
    }

    /// Entity ids are upper case without a colon (`PX-LINE-0001`), as the review page's handle
    /// lookup upper-cases what is typed.
    fn entity_id(&mut self, kind: &str) -> String {
        let short = match kind {
            "LWPOLYLINE" => "POLY",
            other => other,
        };
        let n = self.counters.entry(short.to_owned()).or_insert(0);
        *n += 1;
        format!("PX-{short}-{:04}", *n)
    }

    /// An existing line entity a segment lies on (3 px, 3°) within its extent (5 px slack): the
    /// stroke was drawn once, so the segment becomes a member of that entity over its own interval.
    fn shared_line(&self, p: P, q: P, deny: &[String]) -> Option<(String, String, [f64; 2])> {
        let d = unit(sub(q, p))?;
        for (id, a, b, line_type) in &self.line_index {
            if deny.contains(id) {
                continue;
            }
            let Some(e) = unit(sub(*b, *a)) else { continue };
            if dot(d, e).abs() < 0.998_63 {
                continue;
            }
            let n = (-e.1, e.0);
            if dot(sub(p, *a), n).abs() > 3.0 || dot(sub(q, *a), n).abs() > 3.0 {
                continue;
            }
            let len = dist(*a, *b);
            let (tp, tq) = (dot(sub(p, *a), e), dot(sub(q, *a), e));
            let (lo, hi) = (tp.min(tq), tp.max(tq));
            if lo < -5.0 || hi > len + 5.0 || len < 1.0 {
                continue;
            }
            let (t0, t1) = ((lo / len).clamp(0.0, 1.0), (hi / len).clamp(0.0, 1.0));
            if t1 - t0 < 0.02 {
                continue;
            }
            return Some((id.clone(), line_type.clone(), [t0, t1]));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn add_entity(
        &mut self,
        kind: &str,
        coordinates: Vec<f64>,
        geometry_kind: &str,
        geometry_limit: Option<&str>,
        line_type: &str,
        text: Option<String>,
        style: (&str, &str, Vec<String>),
    ) -> String {
        let id = self.entity_id(kind);
        self.entities.push(json!({
            "coordinates": coordinates.iter().map(|x| fin(*x)).collect::<Vec<_>>(),
            "geometry_kind": geometry_kind,
            "geometry_limit": geometry_limit,
            "handle": id,
            "id": id,
            "kind": kind,
            "layer": LAYER,
            "line_type": line_type,
            "member_ids": [],
            "native_owner": null,
            "text": text,
        }));
        self.native_members.push(json!({
            "block_path": [], "handle": id, "id": id, "kind": kind, "raw_text": text, "role": "native_member", "source_layer": LAYER,
        }));
        let (basis, pattern, evidence) = style;
        self.line_styles.push(json!({
            "basis": basis, "evidence_ids": evidence, "layer": LAYER, "layer_rule": null, "line_type": line_type,
            "pattern": pattern, "primitive": kind, "source_handle": id,
        }));
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn add_expression(
        &mut self,
        kind: &str,
        family: &str,
        label: &str,
        state: &str,
        view_id: &str,
        unknowns: Vec<String>,
        qualifiers: Vec<String>,
        evidence: Vec<String>,
    ) -> String {
        let id = format!("expression:{}", self.expressions.len());
        self.expressions.push(json!({
            "concept_ids": [], "engineering_target": null, "evidence_ids": evidence, "family": family, "id": id, "kind": kind,
            "knowledge_ids": [], "label": if label.trim().is_empty() { kind } else { label }, "member_ids": [], "qualifiers": qualifiers,
            "rule": RULE, "state": state, "unknowns": unknowns, "view_id": view_id,
        }));
        self.expr_members.push(Vec::new());
        id
    }

    #[allow(clippy::too_many_arguments)]
    fn add_member(
        &mut self,
        expression_id: &str,
        entity_id: &str,
        role: &str,
        line_type: &str,
        state: &str,
        view_id: &str,
        evidence: Vec<String>,
        unknowns: Vec<String>,
    ) -> String {
        self.add_member_at(expression_id, entity_id, role, line_type, state, view_id, evidence, unknowns, [0.0, 1.0])
    }

    #[allow(clippy::too_many_arguments)]
    fn add_member_at(
        &mut self,
        expression_id: &str,
        entity_id: &str,
        role: &str,
        line_type: &str,
        state: &str,
        view_id: &str,
        evidence: Vec<String>,
        unknowns: Vec<String>,
        interval: [f64; 2],
    ) -> String {
        let index: usize = expression_id
            .rsplit(':')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let k = self.expr_members.get(index).map_or(0, Vec::len);
        let id = format!("{expression_id}:m{k}");
        self.members.push(json!({
            "evidence_ids": evidence, "expression_id": expression_id, "id": id, "interval": [fin(interval[0]), fin(interval[1])], "line_type": line_type,
            "member_role": role, "native_member_id": null, "relation_ids": [], "source_handle": entity_id, "state": state,
            "unknowns": unknowns, "view_id": view_id,
        }));
        if let Some(list) = self.expr_members.get_mut(index) {
            list.push(id.clone());
        }
        self.entity_members
            .entry(entity_id.to_owned())
            .or_default()
            .push(id.clone());
        id
    }

    /// Geometry of a recovered text: baseline-left insertion (drawing units), height, rotation,
    /// width factor reproducing the refined box in the checker's text model, and the total width.
    fn text_geometry(&self, bbox: [f64; 4], vertical: bool, m: &TextMeasure) -> (P, f64, f64, f64, f64) {
        let s = self.s;
        let (len_px, thick_px) = if vertical {
            (bbox[3] - bbox[1] + 1.0, bbox[2] - bbox[0] + 1.0)
        } else {
            (bbox[2] - bbox[0] + 1.0, bbox[3] - bbox[1] + 1.0)
        };
        let ink_h = thick_px.max(1.0) / s;
        let h = if m.stacked { ink_h / 1.3 } else { ink_h };
        let width_u = len_px.max(1.0) / s;
        let wf = if m.width > 0.0 { (width_u / (m.width * h)).clamp(0.2, 4.0) } else { 1.0 };
        let drop = if m.stacked { 0.15 * h } else { 0.0 };
        if vertical {
            let x = (bbox[2] + 0.5) / s - drop;
            let y = (self.page_h - (bbox[3] + 0.5)) / s;
            ((x, y), h, 90.0, wf, m.width * h * wf)
        } else {
            let x = (bbox[0] - 0.5) / s;
            let y = (self.page_h - (bbox[3] + 0.5)) / s + drop;
            ((x, y), h, 0.0, wf, m.width * h * wf)
        }
    }

    /// Entity of a supported refined member; returns its id and the member line type.
    fn member_entity(&mut self, m: &RefinedMember, fallback_text: &str, doubtful: bool, overlay_class: &str, deny: &[String]) -> (String, String, [f64; 2]) {
        let support = format!("support:{:.2}", m.support_after);
        match &m.refined {
            Shape::Segment(p, q) => {
                // A stroke already carried by an entity (a leader shared by a tolerance frame and a
                // note, a frame leader on a dimension line) is referenced over its own interval.
                if let Some((id, lt, interval)) = self.shared_line(*p, *q, deny) {
                    self.shared_members += 1;
                    if m.role == "center_line" {
                        self.centre_segments.push((*p, *q));
                    }
                    return (id, lt, interval);
                }
                let thick = self
                    .thin_reference
                    .is_some_and(|t| m.width_px >= 1.6 * t && m.width_px > 0.0);
                let lt = stroke_line_type(m.pattern, thick);
                let (a, b) = (self.to_drawing(*p), self.to_drawing(*q));
                let id = self.add_entity(
                    "LINE",
                    vec![a.0, a.1, b.0, b.1],
                    "line",
                    None,
                    lt,
                    None,
                    ("observed_pattern", pattern_name(m.pattern), vec![format!("width_px:{:.2}", m.width_px), support]),
                );
                self.erase_segments.push((*p, *q, m.width_px.max(1.0)));
                if m.role == "center_line" {
                    self.centre_segments.push((*p, *q));
                }
                self.line_index.push((id.clone(), *p, *q, lt.to_owned()));
                self.overlay.line(&self.frame, &id, overlay_class, a, b);
                (id, lt.to_owned(), [0.0, 1.0])
            }
            Shape::Tip { approx, along } => {
                // Tip and base: the base measured on the ink, else six drawing units behind the tip.
                let t = self.to_drawing(*approx);
                let a = unit(*along).unwrap_or((0.0, 0.0));
                let (base_px, length_units, measured) = match (m.arrow_base, m.arrow_length_px) {
                    (Some(b), Some(l)) => (b, l / self.s, true),
                    _ => (sub(*approx, mul(a, 6.0 * self.s)), 6.0, false),
                };
                let b = self.to_drawing(base_px);
                let id = self.add_entity(
                    "SOLID",
                    vec![t.0, t.1, b.0, b.1],
                    "unqualified",
                    Some(if measured { "arrowhead_tip_and_base" } else { "arrowhead_tip_default_base" }),
                    "fill",
                    None,
                    ("native_kind", "not_curve", vec![support, format!("arrow_length_units:{length_units:.2}")]),
                );
                self.arrows.push((*approx, *along));
                self.overlay.arrowhead(&self.frame, &id, t, (a.0, -a.1), length_units);
                (id, "fill".into(), [0.0, 1.0])
            }
            Shape::Point(p) => {
                let t = self.to_drawing(*p);
                let id = self.add_entity(
                    "SOLID",
                    vec![t.0, t.1],
                    "unqualified",
                    Some("point_only"),
                    "fill",
                    None,
                    ("native_kind", "not_curve", vec![support]),
                );
                self.overlay.point(&self.frame, &id, t, 0.6);
                (id, "fill".into(), [0.0, 1.0])
            }
            Shape::TextBox { bbox, vertical } => {
                let observed = m.text.clone().unwrap_or_else(|| fallback_text.to_owned());
                let form = text_form(&observed);
                let measure = text_measure(&form.entity_text);
                let (origin, h, rot, wf, width) = self.text_geometry(*bbox, *vertical, &measure);
                let id = self.add_entity(
                    "TEXT",
                    vec![origin.0, origin.1, h, rot, wf],
                    "unqualified",
                    Some("text_box_from_ink"),
                    "text",
                    Some(form.entity_text.clone()),
                    ("native_kind", "not_curve", vec![support, format!("box_px:{:.0},{:.0},{:.0},{:.0}", bbox[0], bbox[1], bbox[2], bbox[3])]),
                );
                self.erase_boxes.push((bbox[0] - 1.0, bbox[1] - 1.0, bbox[2] + 1.0, bbox[3] + 1.0));
                self.text_boxes.push((*bbox, id.clone()));
                if ["view_title", "view_letter", "scale_text"].contains(&m.role.as_str()) {
                    self.label_texts.push((((bbox[0] + bbox[2]) * 0.5, (bbox[1] + bbox[3]) * 0.5), id.clone()));
                }
                self.overlay.text_box(&self.frame, &id, origin, h, rot, width, doubtful);
                (id, "text".into(), [0.0, 1.0])
            }
            Shape::Polyline(v) => {
                let pts: Vec<P> = v.iter().map(|p| self.to_drawing(*p)).collect();
                let mut coords = Vec::with_capacity(pts.len() * 2);
                for p in &pts {
                    coords.push(p.0);
                    coords.push(p.1);
                }
                let id = self.add_entity(
                    "LWPOLYLINE",
                    coords,
                    "unqualified",
                    Some("polyline_vertices_only"),
                    "wavy",
                    None,
                    ("native_kind", "not_curve", vec![support]),
                );
                for w in v.windows(2) {
                    self.erase_segments.push((w[0], w[1], 2.0));
                }
                self.overlay.polyline(&self.frame, &id, "break_line", &pts, false);
                (id, "wavy".into(), [0.0, 1.0])
            }
            Shape::Hatch { polygon, angle_deg, spacing_px } => {
                let id = self.add_entity(
                    "HATCH",
                    vec![*angle_deg, spacing_px / self.s],
                    "unqualified",
                    Some("pattern_only"),
                    "fill",
                    None,
                    ("native_kind", "not_curve", vec![format!("ink_density:{:.3}", m.support_after)]),
                );
                self.hatch_polygons.push((polygon.clone(), id.clone()));
                let pts: Vec<P> = polygon.iter().map(|p| self.to_drawing(*p)).collect();
                self.overlay.polyline(&self.frame, &id, "annotation", &pts, true);
                (id, "fill".into(), [0.0, 1.0])
            }
        }
    }

    fn record_member(&mut self, rp: &RefinedProposal, m: &RefinedMember, entity: Option<&str>, expression: Option<&str>) {
        let p = &rp.proposal;
        self.refined_records.push(json!({
            "proposal": {"category": p.category.name(), "kind": p.kind, "text": p.text, "tile": p.provenance.tile,
                "item_index": p.provenance.item_index, "file": p.provenance.file, "level": p.provenance.level},
            "role": m.role, "supported": m.supported, "entity_id": entity, "expression_id": expression,
            "support_before": fin(m.support_before), "support_after": fin(m.support_after), "moved_px": fin(m.moved_px),
            "width_px": fin(m.width_px), "pattern": m.pattern.map(|p| pattern_name(Some(p))),
            "proposed_px": shape_json(&m.proposed), "refined_px": shape_json(&m.refined), "merged_from": m.merged_from,
            "arrow_base_px": m.arrow_base.map(|b| [fin(b.0), fin(b.1)]), "arrow_length_px": m.arrow_length_px.map(fin),
            "discarded_components": m.discarded_components, "adjustments": m.adjustments,
        }));
    }

    /// Expressions, members and entities of the kept proposals.
    fn annotations(&mut self, kept: &[RefinedProposal]) {
        // Thin reference: the lower quartile of the refined annotation widths, which are all thin
        // lines; the quartile is robust to widths inflated by crossings and arrowheads.
        let mut widths: Vec<f64> = kept
            .iter()
            .flat_map(|rp| rp.members.iter())
            .filter(|m| m.supported && matches!(m.refined, Shape::Segment(..)) && m.width_px > 0.0)
            .map(|m| m.width_px)
            .collect();
        widths.sort_by(|a, b| a.total_cmp(b));
        self.thin_reference = (!widths.is_empty()).then(|| widths[(widths.len() - 1) / 4]);
        self.thin_widths = widths;
        // View tolerance: one text height (median of the refined text boxes), at least 2 mm.
        let mut heights: Vec<f64> = kept
            .iter()
            .flat_map(|rp| rp.members.iter())
            .filter(|m| m.supported)
            .filter_map(|m| match m.refined {
                Shape::TextBox { bbox, vertical } => Some(if vertical { bbox[2] - bbox[0] } else { bbox[3] - bbox[1] }),
                _ => None,
            })
            .collect();
        heights.sort_by(|a, b| a.total_cmp(b));
        if let Some(h) = heights.get(heights.len() / 2) {
            self.view_margin_px = self.view_margin_px.max(*h);
        }
        for rp in kept {
            let p = &rp.proposal;
            if p.category == Category::CentreLine {
                continue;
            }
            let (mut kind, mut family, label, qualifiers) = expression_identity(p);
            let verification = p.provenance.verification.as_ref();
            let doubtful = verification.is_some_and(|v| !v.verified());
            let unreadable = label.contains('?');
            let mut state = if doubtful || unreadable { "candidate" } else { "observed" };
            let origin = format!("{}#{}:{}", p.provenance.tile, p.provenance.level, p.provenance.item_index);
            let is_text = |m: &RefinedMember| matches!(m.refined, Shape::TextBox { .. });
            let unread_text = |m: &RefinedMember| is_text(m) && m.text.as_deref().unwrap_or(&p.text).contains('?');
            // Members that enter the graph: supported, and not a text the observer could not read.
            let emit: Vec<&RefinedMember> = rp.members.iter().filter(|m| m.supported && !unread_text(m)).collect();
            let mut expr_unknowns = Vec::new();
            for m in rp.members.iter().filter(|m| m.supported && unread_text(m)) {
                self.unread_members_omitted += 1;
                let note = format!("unread text '{}' omitted from {} ({origin}): no {} member is written for it", m.text.as_deref().unwrap_or(&p.text), kind, m.role);
                expr_unknowns.push(note.clone());
                self.unknowns.push(note);
            }
            if emit.is_empty() {
                self.unplaced_proposals += 1;
                self.unknowns.push(format!(
                    "no member kept: {} '{}' ({origin}), {} proposed",
                    p.category.name(),
                    label,
                    rp.members.len()
                ));
                for m in &rp.members {
                    if !m.supported {
                        self.unsupported += 1;
                    }
                    self.record_member(rp, m, None, None);
                }
                continue;
            }
            let reference = emit
                .iter()
                .find(|m| is_text(m))
                .or(emit.first())
                .map_or(p.centre, |m| m.refined.reference());
            let view = self.view_of(reference);
            // A dimension reading with no supported line, arrowhead or leader is a number in a table
            // or a note, not a dimension.
            let mut text_role: Option<&'static str> = None;
            if p.category == Category::Dimension && emit.iter().all(|m| is_text(m)) {
                self.table_texts += 1;
                if view == "view:sheet" {
                    let c = reference;
                    let in_title = self.layout.and_then(|l| l.title_block).is_some_and(|b| {
                        c.0 >= b[0] - 12.0 && c.0 <= b[2] + 12.0 && c.1 >= b[1] - 12.0 && c.1 <= b[3] + 12.0
                    });
                    kind = if in_title { "title_block" } else { "sheet_frame" };
                    family = "sheet";
                    text_role = Some("table_text");
                } else {
                    kind = "technical_general";
                    family = "note";
                    text_role = Some("note_text");
                }
                state = "candidate";
                expr_unknowns.push("Dimension reading without supported dimension line, arrowhead or leader: kept as text".into());
            }
            // Consolidated hatch pattern of a region read in several pieces: the median of the
            // measured angles and spacings, so one region carries one pattern.
            let hatch_values: Option<(f64, f64)> = if p.category == Category::Hatch {
                let measured: Vec<(f64, f64)> = emit
                    .iter()
                    .filter(|m| m.adjustments.iter().any(|a| a.starts_with("hatch_measured")))
                    .filter_map(|m| match m.refined {
                        Shape::Hatch { angle_deg, spacing_px, .. } => Some((angle_deg, spacing_px)),
                        _ => None,
                    })
                    .collect();
                if measured.is_empty() {
                    None
                } else {
                    let (mut sx, mut sy) = (0.0, 0.0);
                    for (a, _) in &measured {
                        let r = (2.0 * a).to_radians();
                        sx += r.cos();
                        sy += r.sin();
                    }
                    let angle = (0.5 * sy.atan2(sx)).to_degrees().rem_euclid(180.0);
                    let mut sp: Vec<f64> = measured.iter().map(|m| m.1).collect();
                    sp.sort_by(|a, b| a.total_cmp(b));
                    Some((angle, sp[sp.len() / 2]))
                }
            } else {
                None
            };
            if unreadable {
                self.unreadable_texts += 1;
                let note = format!("unread text '{}' ({origin}): the observer could not read part of it", label);
                expr_unknowns.push(note.clone());
                self.unknowns.push(note);
            }
            if doubtful {
                self.doubtful += 1;
                let v = verification.expect("doubtful implies verification");
                let note = format!(
                    "doubtful text '{}' ({origin}): P(proposal)={:.2}, top choice '{}'",
                    label, v.p_proposal, v.top_choice
                );
                expr_unknowns.push(note.clone());
                self.unknowns.push(note);
            }
            for m in &rp.members {
                if !m.supported {
                    self.unsupported += 1;
                    let note = format!(
                        "unsupported {} of {} '{}' ({origin}): support {:.2}→{:.2}",
                        m.role,
                        p.category.name(),
                        label,
                        m.support_before,
                        m.support_after
                    );
                    expr_unknowns.push(note.clone());
                    self.unknowns.push(note);
                }
            }
            let evidence: Vec<String> = vec![
                format!("observation:{}", p.provenance.file),
                format!("tile:{}", p.provenance.tile),
                format!("model:{}", p.provenance.model),
            ];
            let label = match hatch_values {
                Some((a, _)) => format!("剖面线 {a:.0}°"),
                None => label,
            };
            let eid = self.add_expression(kind, family, &label, state, &view, expr_unknowns, qualifiers, evidence);
            let overlay_class = match kind {
                "center_axis" => "center_axis",
                _ => "annotation",
            };
            // Entities this expression already references: never shared again within it.
            let mut used: Vec<String> = Vec::new();
            for m in &rp.members {
                if !emit.iter().any(|e| std::ptr::eq(*e, m)) {
                    self.record_member(rp, m, None, Some(&eid));
                    continue;
                }
                let consolidated;
                let m = match (&m.refined, hatch_values) {
                    (Shape::Hatch { polygon, .. }, Some((angle, spacing))) => {
                        let mut c = m.clone();
                        c.refined = Shape::Hatch { polygon: polygon.clone(), angle_deg: angle, spacing_px: spacing };
                        consolidated = c;
                        &consolidated
                    }
                    _ => m,
                };
                let role = match (text_role, is_text(m)) {
                    (Some(r), true) => r,
                    _ => m.role.as_str(),
                };
                let (entity, lt, interval) = self.member_entity(m, &p.text, doubtful, overlay_class, &used);
                used.push(entity.clone());
                let shared = interval != [0.0, 1.0] || self.shared_uses.contains_key(&entity) || self.provenance.contains_key(&entity);
                let mut member_evidence = vec![
                    format!("observation:{}#{}", p.provenance.file, p.provenance.item_index),
                    format!("support:{:.2}", m.support_after),
                ];
                if let Some(from) = &m.merged_from {
                    member_evidence.push(format!("merged_from:{from}"));
                }
                if shared {
                    member_evidence.push("shared_stroke".into());
                }
                let member_unknowns = if doubtful && is_text(m) {
                    vec!["text verification below threshold".to_owned()]
                } else {
                    vec![]
                };
                let mid = self.add_member_at(&eid, &entity, role, &lt, state, &view, member_evidence, member_unknowns, interval);
                self.record_member(rp, m, Some(&entity), Some(&eid));
                let record = json!({
                    "expression_id": eid, "member_id": mid, "role": role, "category": p.category.name(), "kind": p.kind,
                    "text": p.text, "label": label, "tile": p.provenance.tile, "tile_sha256": p.provenance.tile_sha256,
                    "level": p.provenance.level, "model": p.provenance.model, "prompt_sha256": p.provenance.prompt_sha256,
                    "file": p.provenance.file, "item_index": p.provenance.item_index, "verification": verification_json(verification),
                    "proposed_px": shape_json(&m.proposed), "refined_px": shape_json(&m.refined),
                    "support_before": fin(m.support_before), "support_after": fin(m.support_after), "moved_px": fin(m.moved_px),
                    "width_px": fin(m.width_px), "pattern": m.pattern.map(|q| pattern_name(Some(q))), "merged_from": m.merged_from,
                    "arrow_base_px": m.arrow_base.map(|b| [fin(b.0), fin(b.1)]), "arrow_length_px": m.arrow_length_px.map(fin),
                    "discarded_components": m.discarded_components, "adjustments": m.adjustments, "interval": interval,
                    "notes": p.notes,
                });
                if shared {
                    self.shared_uses.entry(entity.clone()).or_default().push(record);
                } else {
                    self.provenance.insert(entity.clone(), record);
                }
            }
        }
    }

    /// Centre lines: the collinear proposed pieces of one axis (3 px, 3°, gaps up to two text
    /// heights or across a text box) are one `center_axis` expression with one LINE entity spanning
    /// the axis, as the DXF route has one centre line entity; the pieces stay in the provenance.
    fn centre_axes(&mut self, kept: &[RefinedProposal]) {
        struct PieceRef<'p> {
            rp: &'p RefinedProposal,
            m: &'p RefinedMember,
            p: P,
            q: P,
        }
        let mut pieces: Vec<PieceRef> = Vec::new();
        for rp in kept.iter().filter(|rp| rp.proposal.category == Category::CentreLine) {
            let origin = format!("{}#{}:{}", rp.proposal.provenance.tile, rp.proposal.provenance.level, rp.proposal.provenance.item_index);
            for m in &rp.members {
                match m.refined {
                    Shape::Segment(p, q) if m.supported => pieces.push(PieceRef { rp, m, p, q }),
                    _ => {
                        self.unsupported += 1;
                        self.unknowns.push(format!("unsupported center_line piece ({origin}): support {:.2}→{:.2}", m.support_before, m.support_after));
                        self.record_member(rp, m, None, None);
                    }
                }
            }
        }
        if pieces.is_empty() {
            return;
        }
        let segs: Vec<(P, P)> = pieces.iter().map(|x| (x.p, x.q)).collect();
        let gap = 2.0 * self.view_margin_px;
        let groups = group_collinear(&segs, 0.052_36, 3.0, gap, 4.0 * gap, |a, b| self.gap_explained(a, b).is_some());
        for g in groups {
            let members: Vec<&PieceRef> = g.iter().map(|&k| &pieces[k]).collect();
            let reference = members
                .iter()
                .max_by(|a, b| dist(a.p, a.q).total_cmp(&dist(b.p, b.q)))
                .expect("non-empty group");
            let o = reference.p;
            let Some(d) = unit(sub(reference.q, reference.p)) else { continue };
            let (mut tmin, mut tmax) = (f64::INFINITY, f64::NEG_INFINITY);
            for x in &members {
                for e in [x.p, x.q] {
                    let t = dot(sub(e, o), d);
                    tmin = tmin.min(t);
                    tmax = tmax.max(t);
                }
            }
            let (a, b) = (add(o, mul(d, tmin)), add(o, mul(d, tmax)));
            let width = members.iter().map(|x| x.m.width_px).fold(0.0, f64::max);
            let pattern = measure_pattern(self.ink, a, b);
            let thick = self.thin_reference.is_some_and(|t| width >= 1.6 * t && width > 0.0);
            let lt = stroke_line_type(Some(pattern), thick);
            let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
            let view = self.view_of(mid);
            let vl = self.view_label(&view);
            self.centre_axes_count += 1;
            let eid = self.add_expression(
                "center_axis",
                "geometry",
                &format!("{vl} · 中心线 {}", self.centre_axes_count),
                "observed",
                &view,
                vec![],
                vec![],
                vec!["rule:collinear_centre_pieces".to_owned(), format!("pieces:{}", members.len())],
            );
            let (au, bu) = (self.to_drawing(a), self.to_drawing(b));
            let support = members.iter().map(|x| x.m.support_after).fold(1.0, f64::min);
            let id = self.add_entity(
                "LINE",
                vec![au.0, au.1, bu.0, bu.1],
                "line",
                None,
                lt,
                None,
                ("observed_pattern", pattern_name(Some(pattern)), vec![format!("width_px:{width:.2}"), format!("pieces:{}", members.len())]),
            );
            self.line_index.push((id.clone(), a, b, lt.to_owned()));
            self.centre_segments.push((a, b));
            for x in &members {
                self.erase_segments.push((x.p, x.q, x.m.width_px.max(1.0)));
            }
            self.overlay.line(&self.frame, &id, "center_axis", au, bu);
            let evidence: Vec<String> = members
                .iter()
                .map(|x| format!("observation:{}#{}", x.rp.proposal.provenance.file, x.rp.proposal.provenance.item_index))
                .chain(std::iter::once(format!("support:{support:.2}")))
                .collect();
            let mid_id = self.add_member(&eid, &id, "center_line", lt, "observed", &view, evidence, vec![]);
            for x in &members {
                self.record_member(x.rp, x.m, Some(&id), Some(&eid));
            }
            let first = &members[0].rp.proposal;
            self.provenance.insert(id.clone(), json!({
                "source": "centre_axis", "expression_id": eid, "member_id": mid_id, "role": "center_line", "category": "centre_line",
                "tile": first.provenance.tile, "tile_sha256": first.provenance.tile_sha256, "level": first.provenance.level, "model": first.provenance.model,
                "prompt_sha256": first.provenance.prompt_sha256, "file": first.provenance.file, "item_index": first.provenance.item_index,
                "span_px": [fin(a.0), fin(a.1), fin(b.0), fin(b.1)], "pattern": pattern_name(Some(pattern)), "width_px": fin(width),
                "pieces": members.iter().map(|x| json!({"tile": x.rp.proposal.provenance.tile, "file": x.rp.proposal.provenance.file,
                    "item_index": x.rp.proposal.provenance.item_index, "p": [fin(x.p.0), fin(x.p.1)], "q": [fin(x.q.0), fin(x.q.1)],
                    "support_after": fin(x.m.support_after), "width_px": fin(x.m.width_px)})).collect::<Vec<_>>(),
            }));
        }
    }

    /// Cause of a gap between two collinear pieces: a text box or a section region on it.
    fn gap_explained(&self, a: P, b: P) -> Option<(&'static str, String)> {
        for (bbox, id) in &self.text_boxes {
            if segment_hits_box(a, b, *bbox, 3.0) {
                return Some(("text_break", format!("text:{id}")));
            }
        }
        for (polygon, id) in &self.hatch_polygons {
            if segment_hits_polygon(a, b, polygon) {
                return Some(("local_section", format!("hatch:{id}")));
            }
        }
        None
    }

    /// Vectorise the remainder and place every stroke.
    fn structure(&mut self, params: &StructureParams) {
        let rest = remaining_ink(self.ink, &self.erase_segments, &self.erase_boxes,
            &self.hatch_polygons.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(), &self.arrows);
        let erased = erased_strokes(&self.erase_segments, &self.arrows);
        let (classified, thin, remnants) =
            vectorise_remainder(&rest, self.ink, &erased, self.thin_reference, &self.centre_segments, params);
        self.thin_reference = Some(thin);
        self.stroke_counts.insert("remnants_of_erased_members".into(), remnants);
        let title_block = self.layout.and_then(|l| l.title_block);
        // Width contrast of the page: a print that draws outlines as thin as dimension lines gives the
        // width rule nothing to find. Then continuous in-view remainder strokes are outline by
        // elimination (every annotation line was proposed and erased), as candidates.
        let views: Vec<String> = classified.iter().map(|c| self.view_of(c.stroke.primitive.mid())).collect();
        let (mut continuous_in_view, mut thick_in_view) = (0usize, 0usize);
        for (c, view) in classified.iter().zip(&views) {
            if view != "view:sheet" && matches!(c.stroke.pattern, Pattern::Continuous | Pattern::Unknown) {
                continuous_in_view += 1;
                if c.thick {
                    thick_in_view += 1;
                }
            }
        }
        // Fewer than one in twenty in-view continuous strokes thick: the print draws outlines thin.
        let by_elimination = continuous_in_view > 0 && thick_in_view * 20 < continuous_in_view;
        self.width_contrast = json!({"in_view_continuous_strokes": continuous_in_view, "in_view_thick_strokes": thick_in_view,
            "thin_reference_px": thin, "contour_by_elimination": by_elimination});
        if by_elimination {
            self.unknowns.push(format!(
                "No width contrast inside the views ({thick_in_view} thick of {continuous_in_view} continuous strokes at thin reference {thin:.2} px): continuous unexplained ink in the views is taken as visible outline by elimination, as candidates"
            ));
        }
        let technical = self.layout.and_then(|l| l.technical_requirements.as_ref().map(|t| t.0));
        let min_remnant_px = 2.0 * 300.0 / 25.4;
        let mut groups: BTreeMap<(String, &'static str), String> = BTreeMap::new();
        let mut pieces: Vec<Piece> = Vec::new();
        for (c, view) in classified.iter().zip(&views) {
            if c.stroke.primitive.length() < min_remnant_px {
                *self.stroke_counts.entry("shorter_than_2mm".into()).or_insert(0) += 1;
                continue;
            }
            let mid = c.stroke.primitive.mid();
            let class_name = match c.class {
                StrokeClass::CoveredCentre => {
                    *self.stroke_counts.entry("covered_centre".into()).or_insert(0) += 1;
                    continue;
                }
                StrokeClass::VisibleContour => "visible_contour",
                StrokeClass::HiddenContour => "hidden_contour",
                StrokeClass::CentreAxis => "center_axis",
                StrokeClass::ThinUnresolved if by_elimination && view != "view:sheet" => "visible_contour_by_elimination",
                StrokeClass::ThinUnresolved => "unresolved",
            };
            let view = view.clone();
            let (kind, family, role, state, label, overlay_class) = if view == "view:sheet" {
                // Sheet ink is frame or table structure, never outline: table lines inside the title
                // block or the technical-requirements block, frame lines elsewhere.
                let inside = |b: Option<[f64; 4]>| {
                    b.is_some_and(|b| mid.0 >= b[0] - 12.0 && mid.0 <= b[2] + 12.0 && mid.1 >= b[1] - 12.0 && mid.1 <= b[3] + 12.0)
                };
                if inside(title_block) || inside(technical) {
                    ("sheet_frame", "sheet", "table_line", "candidate", "图框与表格线".to_owned(), "sheet")
                } else {
                    ("sheet_frame", "sheet", "frame_border", "candidate", "图框与表格线".to_owned(), "sheet")
                }
            } else {
                let vl = self.view_label(&view);
                match class_name {
                    "visible_contour" => ("visible_contour", "geometry", "contour_segment", "observed", format!("{vl} · 可见轮廓"), "visible_contour"),
                    "visible_contour_by_elimination" => ("visible_contour", "geometry", "contour_segment", "candidate", format!("{vl} · 可见轮廓（按排除法）"), "visible_contour"),
                    "hidden_contour" => ("hidden_contour", "geometry", "hidden_segment", "candidate", format!("{vl} · 不可见轮廓"), "hidden_contour"),
                    "center_axis" => ("center_axis", "geometry", "center_line", "candidate", format!("{vl} · 轴线与中心线（矢量化）"), "center_axis"),
                    _ => ("unresolved", "unresolved", "unresolved_source", "unknown", format!("{vl} · 未归类细线"), "unresolved"),
                }
            };
            *self.stroke_counts.entry(if view == "view:sheet" { role } else { class_name }.to_owned()).or_insert(0) += 1;
            let key = (view.clone(), if view == "view:sheet" { role } else { class_name });
            let eid = match groups.get(&key) {
                Some(id) => id.clone(),
                None => {
                    let unknowns = match class_name {
                        "visible_contour" | "hidden_contour" => vec!["Part feature identity of each piece requires projection evidence".to_owned()],
                        "visible_contour_by_elimination" => vec![
                            "Outline by elimination: the print shows no width contrast, so continuous ink no proposal explains is taken as visible outline; the member line type is the measured thin one".to_owned(),
                            "Part feature identity of each piece requires projection evidence".to_owned(),
                        ],
                        "unresolved" => vec!["Ink no proposal or structure rule explained".to_owned()],
                        _ => vec![],
                    };
                    let rule = if class_name == "visible_contour_by_elimination" { "rule:contour_by_elimination" } else { "rule:vectorised_remainder" };
                    let id = self.add_expression(kind, family, &label, state, &view, unknowns, vec![],
                        vec![rule.to_owned(), format!("thin_reference_px:{thin:.2}")]);
                    groups.insert(key, id.clone());
                    id
                }
            };
            let lt = stroke_line_type(Some(c.stroke.pattern), c.thick);
            let evidence = vec![
                format!("support:{:.2}", c.stroke.support),
                format!("width_px:{:.2}", c.stroke.width_px),
                format!("skeleton_pixels:{}", c.stroke.pixels),
            ];
            let (entity, prov) = match c.stroke.primitive {
                Primitive::Line { p, q } => {
                    let (a, b) = (self.to_drawing(p), self.to_drawing(q));
                    let id = self.add_entity("LINE", vec![a.0, a.1, b.0, b.1], "line", None, lt, None,
                        ("observed_pattern", pattern_name(Some(c.stroke.pattern)), evidence.clone()));
                    self.overlay.line(&self.frame, &id, overlay_class, a, b);
                    (id, json!({"type": "line", "p": [fin(p.0), fin(p.1)], "q": [fin(q.0), fin(q.1)]}))
                }
                Primitive::Arc { c: centre, r, start_rad, sweep_rad } => {
                    let cu = self.to_drawing(centre);
                    let ru = r / self.s;
                    let full = sweep_rad.abs() >= std::f64::consts::TAU - 1e-6;
                    // Image angles turn the other way once y points up.
                    let start = (-start_rad).to_degrees().rem_euclid(360.0);
                    let sweep = -sweep_rad.to_degrees();
                    let id = if full {
                        self.add_entity("CIRCLE", vec![cu.0, cu.1, ru], "circle", None, lt, None,
                            ("observed_pattern", pattern_name(Some(c.stroke.pattern)), evidence.clone()))
                    } else {
                        self.add_entity("ARC", vec![cu.0, cu.1, ru, start, sweep], "arc", None, lt, None,
                            ("observed_pattern", pattern_name(Some(c.stroke.pattern)), evidence.clone()))
                    };
                    self.overlay.arc(&self.frame, &id, overlay_class, cu, ru, start, if full { 360.0 } else { sweep });
                    (id, json!({"type": "arc", "c": [fin(centre.0), fin(centre.1)], "r": fin(r), "start_rad": fin(start_rad), "sweep_rad": fin(sweep_rad)}))
                }
            };
            let mid_id = self.add_member(&eid, &entity, role, lt, state, &view, evidence, vec![]);
            self.provenance.insert(entity.clone(), json!({
                "source": "vectorize", "expression_id": eid, "member_id": mid_id, "role": role, "class": kind,
                "stroke_px": prov, "width_px": fin(c.stroke.width_px), "pattern": pattern_name(Some(c.stroke.pattern)),
                "support": fin(c.stroke.support), "skeleton_pixels": c.stroke.pixels, "thick": c.thick,
            }));
            if kind == "visible_contour" || kind == "hidden_contour" {
                let class: &'static str = if kind == "visible_contour" { "visible_outline" } else { "hidden_outline" };
                match c.stroke.primitive {
                    Primitive::Line { p, q } => pieces.push(Piece { entity, member: mid_id, p, q, view, class }),
                    Primitive::Arc { c: centre, r, start_rad, sweep_rad } => {
                        self.arc_structural_line(&entity, &mid_id, &view, class, kind, centre, r, start_rad, sweep_rad);
                    }
                }
            }
        }
        self.line_structural_lines(pieces, params);
    }

    #[allow(clippy::too_many_arguments)]
    fn arc_structural_line(&mut self, entity: &str, member: &str, view: &str, class: &str, kind: &str, centre: P, r: f64, start_rad: f64, sweep_rad: f64) {
        let cu = self.to_drawing(centre);
        let ru = r / self.s;
        let full = sweep_rad.abs() >= std::f64::consts::TAU - 1e-6;
        let start = (-start_rad).to_degrees().rem_euclid(360.0);
        let sweep = -sweep_rad.to_degrees();
        let id = format!("line:{}", self.structural_lines.len());
        let line_type = if kind == "visible_contour" { "thick_continuous" } else { "thin_dashed" };
        let (geometry, points, arc) = if full {
            ("circle", vec![cu.0, cu.1], vec![cu.0, cu.1, ru, 0.0, 360.0])
        } else {
            let at = |deg: f64| {
                let a = deg.to_radians();
                (cu.0 + ru * a.cos(), cu.1 + ru * a.sin())
            };
            let (a, b) = (at(start), at(start + sweep));
            ("arc", vec![a.0, a.1, b.0, b.1], vec![cu.0, cu.1, ru, start, sweep])
        };
        self.structural_lines.push(json!({
            "arc": arc.iter().map(|x| fin(*x)).collect::<Vec<_>>(), "coincident": [], "evidence_ids": ["pieces:1"], "geometry": geometry, "id": id,
            "line_class": class, "line_type": line_type, "pieces": [format!("member:{member}")],
            "points": points.iter().map(|x| fin(*x)).collect::<Vec<_>>(), "state": "observed", "trimmed": [], "unknowns": [], "view_id": view,
        }));
        let _ = entity;
    }

    #[allow(clippy::too_many_arguments)]
    fn add_end(&mut self, class: &str, entity: &str, parameter: f64, point: P, cause: &str, evidence: &[String], partner: &str, view: &str) -> String {
        let id = format!("end:{class}:{entity}:{parameter:.6}");
        if self.end_ids.insert(id.clone()) {
            let pu = self.to_drawing(point);
            self.structure_ends.push(json!({
                "cause": cause, "evidence_ids": evidence, "id": id, "line_class": class, "parameter": parameter,
                "partner_end_id": partner, "point": [fin(pu.0), fin(pu.1)], "source_handle": entity, "state": "candidate",
                "unknowns": [], "view_id": view,
            }));
        }
        id
    }

    /// Structural lines of the line pieces: collinear groups per view and class, joined across
    /// touching joints and explained interruptions, split at unexplained gaps.
    fn line_structural_lines(&mut self, pieces: Vec<Piece>, params: &StructureParams) {
        let mut by_key: BTreeMap<(String, &'static str), Vec<usize>> = BTreeMap::new();
        for (i, p) in pieces.iter().enumerate() {
            by_key.entry((p.view.clone(), p.class)).or_default().push(i);
        }
        for ((view, class), idx) in by_key {
            let segs: Vec<(P, P)> = idx.iter().map(|&i| (pieces[i].p, pieces[i].q)).collect();
            let groups = group_collinear(
                &segs,
                params.merge_angle_rad,
                params.merge_offset_px,
                params.merge_gap_px,
                params.explained_gap_px,
                |a, b| self.gap_explained(a, b).is_some(),
            );
            for g in groups {
                let members: Vec<&Piece> = g.iter().map(|&k| &pieces[idx[k]]).collect();
                self.structural_group(&members, &view, class, params);
            }
        }
    }

    fn structural_group(&mut self, members: &[&Piece], view: &str, class: &'static str, params: &StructureParams) {
        let line_type = if class == "visible_outline" { "thick_continuous" } else { "thin_dashed" };
        let reference = members
            .iter()
            .max_by(|a, b| dist(a.p, a.q).total_cmp(&dist(b.p, b.q)))
            .expect("non-empty group");
        let o = reference.p;
        let Some(d) = unit(sub(reference.q, reference.p)) else {
            return;
        };
        struct Item<'p> {
            piece: &'p Piece,
            lo: f64,
            hi: f64,
            forward: bool,
        }
        let mut items: Vec<Item> = members
            .iter()
            .map(|p| {
                let (ta, tb) = (dot(sub(p.p, o), d), dot(sub(p.q, o), d));
                Item {
                    piece: p,
                    lo: ta.min(tb),
                    hi: ta.max(tb),
                    forward: ta <= tb,
                }
            })
            .collect();
        items.sort_by(|a, b| a.lo.total_cmp(&b.lo).then(a.hi.total_cmp(&b.hi)));
        // Parameter of a piece at a position t along the reference.
        let param_at = |it: &Item, t: f64| -> f64 {
            let len = (it.hi - it.lo).max(1e-9);
            let u = ((t - it.lo) / len).clamp(0.0, 1.0);
            if it.forward { u } else { 1.0 - u }
        };
        struct Acc {
            pieces: Vec<String>,
            coincident: Vec<String>,
            trimmed: Vec<String>,
            implied: Vec<String>,
            tmin: f64,
            tmax: f64,
        }
        let mut lines: Vec<Acc> = Vec::new();
        let first = &items[0];
        let mut cur = Acc {
            pieces: vec![format!("member:{}", first.piece.member)],
            coincident: vec![],
            trimmed: vec![],
            implied: vec![],
            tmin: first.lo,
            tmax: first.hi,
        };
        let mut cover_hi = first.hi;
        let mut cover_item = 0usize;
        for (k, it) in items.iter().enumerate().skip(1) {
            if it.lo <= cover_hi + params.touch_gap_px {
                if it.hi <= cover_hi + 0.5 {
                    cur.coincident.push(format!("member:{}", it.piece.member));
                    continue;
                }
                if it.lo < cover_hi - 0.5 {
                    let (u0, u1) = (param_at(it, it.lo), param_at(it, cover_hi));
                    cur.trimmed.push(format!("member:{}:{:.6}:{:.6}", it.piece.member, u0.min(u1), u0.max(u1)));
                }
                cur.pieces.push(format!("member:{}", it.piece.member));
                cur.tmax = it.hi;
                cover_hi = it.hi;
                cover_item = k;
                continue;
            }
            let from = &items[cover_item];
            let g0 = add(o, mul(d, cover_hi));
            let g1 = add(o, mul(d, it.lo));
            let from_param = if from.forward { 1.0 } else { 0.0 };
            let to_param = if it.forward { 0.0 } else { 1.0 };
            match self.gap_explained(g0, g1) {
                Some((cause, evidence)) => {
                    let sid = format!("implied:{}", self.implied_spans.len());
                    let (a, b) = (self.to_drawing(g0), self.to_drawing(g1));
                    let ev = vec![evidence];
                    let end_from = format!("end:{class}:{}:{from_param:.6}", from.piece.entity);
                    let end_to = format!("end:{class}:{}:{to_param:.6}", it.piece.entity);
                    self.implied_spans.push(json!({
                        "arc": null, "cause": cause, "evidence_ids": ev, "from_parameter": from_param,
                        "from_source_handle": from.piece.entity, "id": sid, "line_class": class,
                        "points": [fin(a.0), fin(a.1), fin(b.0), fin(b.1)], "representation": "complete_structure", "state": "candidate",
                        "to_parameter": to_param, "to_source_handle": it.piece.entity, "unknowns": [IMPLIED_UNKNOWN], "view_id": view,
                    }));
                    self.add_end(class, &from.piece.entity, from_param, g0, cause, &ev, &end_to, view);
                    self.add_end(class, &it.piece.entity, to_param, g1, cause, &ev, &end_from, view);
                    cur.pieces.push(sid.clone());
                    cur.implied.push(sid);
                    cur.pieces.push(format!("member:{}", it.piece.member));
                    cur.tmax = it.hi;
                }
                None => {
                    let ev: Vec<String> = vec![format!("gap_px:{:.1}", it.lo - cover_hi)];
                    let end_from = format!("end:{class}:{}:{from_param:.6}", from.piece.entity);
                    let end_to = format!("end:{class}:{}:{to_param:.6}", it.piece.entity);
                    self.add_end(class, &from.piece.entity, from_param, g0, "drafting_gap", &ev, &end_to, view);
                    self.add_end(class, &it.piece.entity, to_param, g1, "drafting_gap", &ev, &end_from, view);
                    lines.push(cur);
                    cur = Acc {
                        pieces: vec![format!("member:{}", it.piece.member)],
                        coincident: vec![],
                        trimmed: vec![],
                        implied: vec![],
                        tmin: it.lo,
                        tmax: it.hi,
                    };
                }
            }
            cover_hi = it.hi;
            cover_item = k;
        }
        lines.push(cur);
        for acc in lines {
            let id = format!("line:{}", self.structural_lines.len());
            let (a, b) = (self.to_drawing(add(o, mul(d, acc.tmin))), self.to_drawing(add(o, mul(d, acc.tmax))));
            let n_members = acc.pieces.iter().filter(|p| p.starts_with("member:")).count();
            let mut evidence = vec![format!("pieces:{n_members}")];
            evidence.extend(acc.implied.iter().map(|s| format!("inferred:{s}")));
            let inferred = !acc.implied.is_empty();
            self.structural_lines.push(json!({
                "arc": null, "coincident": acc.coincident, "evidence_ids": evidence, "geometry": "line", "id": id,
                "line_class": class, "line_type": line_type, "pieces": acc.pieces,
                "points": [fin(a.0), fin(a.1), fin(b.0), fin(b.1)], "state": if inferred { "candidate" } else { "observed" },
                "trimmed": acc.trimmed, "unknowns": if inferred { vec![LINE_IMPLIED_UNKNOWN] } else { vec![] }, "view_id": view,
            }));
        }
    }

    fn views_json(&self) -> Vec<Value> {
        let mut out = Vec::new();
        for v in &self.views {
            let b = v.bbox_px;
            let lo = self.to_drawing((b[0], b[3]));
            let hi = self.to_drawing((b[2], b[1]));
            let reach = 80.0;
            let labels: Vec<&String> = self
                .label_texts
                .iter()
                .filter(|(c, _)| c.0 >= b[0] - reach && c.0 <= b[2] + reach && c.1 >= b[1] - reach && c.1 <= b[3] + reach)
                .map(|(_, id)| id)
                .collect();
            out.push(json!({
                "bbox": [fin(lo.0), fin(lo.1), fin(hi.0), fin(hi.1)], "evidence_ids": ["l0_layout", format!("name:{}", v.name)], "id": v.id,
                "kind": v.kind, "label": v.label, "label_source_ids": labels, "scale_text": v.scale_text, "state": "candidate",
                "unknowns": ["View region proposed by the page-layout observation; projection relation remains a candidate"],
            }));
        }
        out.push(json!({
            "bbox": [0.0, 0.0, fin(self.frame.width_units), fin(self.frame.height_units)], "evidence_ids": ["page_extent"], "id": "view:sheet",
            "kind": "sheet", "label": "图纸", "label_source_ids": [], "scale_text": null, "state": "observed", "unknowns": [],
        }));
        out
    }

    fn finish(mut self, page_sha256: &str) -> (Value, Map<String, Value>, Vec<Value>, Overlay, Frame, Value) {
        for e in &mut self.entities {
            let id = e["id"].as_str().unwrap_or_default().to_owned();
            e["member_ids"] = json!(self.entity_members.get(&id).cloned().unwrap_or_default());
        }
        for (i, x) in self.expressions.iter_mut().enumerate() {
            x["member_ids"] = json!(self.expr_members[i]);
        }
        let mut unknowns = vec![
            "Entities were recovered from a page image: coordinates are calibrated pixels, not native drawing coordinates".to_owned(),
            format!("Scale: {} px per drawing unit from {}", self.cal.px_per_unit, self.cal.basis),
            "Members below 0.8 ink support and texts below 0.6 verification probability stay out of the graph; each is listed here".to_owned(),
            "Structural lines join collinear contour pieces of one view and class; implied spans bridge interruptions explained by a text box or a section region only".to_owned(),
            "Part features, measured targets and manufacturing scope of expressions remain unresolved".to_owned(),
        ];
        unknowns.append(&mut self.unknowns);
        for (entity, uses) in std::mem::take(&mut self.shared_uses) {
            match self.provenance.get_mut(&entity) {
                Some(Value::Object(o)) => {
                    o.insert("shared_by".into(), Value::Array(uses));
                }
                _ => {
                    self.provenance.insert(entity, json!({"source": "shared_stroke", "shared_by": uses}));
                }
            }
        }
        let graph = json!({
            "expressions": self.expressions,
            "implied_spans": self.implied_spans,
            "layer_convention": "not_qualified",
            "layer_convention_evidence": ["pixel route: no native layers; line types measured on the page"],
            "line_styles": self.line_styles,
            "members": self.members,
            "profile": PROFILE,
            "schema": SCHEMA,
            "source": {
                "entities": self.entities,
                "epsilon_drawing_units": 1.0 / self.s,
                "members": self.native_members,
                "profile": PROFILE,
                "relations": [],
                "schema": USAGE_SCHEMA,
                "segments": [],
                "source_sha256": page_sha256,
                "unknowns": [
                    "Entities are members recovered from a page image by observation and ink refinement; no native source structure exists",
                    "No relations, uses or segments are derived; the expression graph carries the structure",
                ],
                "uses": [],
            },
            "source_sha256": page_sha256,
            "structural_lines": self.structural_lines,
            "structure_ends": self.structure_ends,
            "unknowns": unknowns,
            "views": self.views_json(),
        });
        let counts = json!({
            "unsupported_members": self.unsupported,
            "doubtful_texts": self.doubtful,
            "unreadable_texts": self.unreadable_texts,
            "unread_members_omitted": self.unread_members_omitted,
            "shared_members": self.shared_members,
            "table_texts_from_dimension_readings": self.table_texts,
            "centre_axes": self.centre_axes_count,
            "view_margin_px": self.view_margin_px,
            "proposals_without_support": self.unplaced_proposals,
            "strokes": self.stroke_counts,
            "thin_reference_px": self.thin_reference,
            "thin_width_samples": self.thin_widths.len(),
            "width_contrast": self.width_contrast,
        });
        (graph, self.provenance, self.refined_records, self.overlay, self.frame, counts)
    }
}

/// Re-derive every extension line's span from its dimension-line crossing (design: the span runs
/// from the overshoot through the crossing to the first perpendicular junction on the feature side,
/// never along a collinear outline). Other dimension lines, leaders and centre lines of the page are
/// passed through as crossings. Returns the number of spans re-derived.
fn trim_extension_spans(ink: &Ink, kept: &mut [RefinedProposal]) -> usize {
    let pass_through: Vec<(P, P)> = kept
        .iter()
        .flat_map(|rp| rp.members.iter())
        .filter(|m| m.supported && matches!(m.role.as_str(), "dimension_line" | "leader_line" | "center_line"))
        .filter_map(|m| match m.refined {
            Shape::Segment(p, q) => Some((p, q)),
            _ => None,
        })
        .collect();
    let mut count = 0;
    for rp in kept.iter_mut() {
        if rp.proposal.category != Category::Dimension {
            continue;
        }
        let Some(dimension) = rp.members.iter().find_map(|m| match m.refined {
            Shape::Segment(p, q) if m.supported && m.role == "dimension_line" => Some((p, q)),
            _ => None,
        }) else {
            continue;
        };
        for m in rp.members.iter_mut().filter(|m| m.supported && m.role == "extension_line") {
            let Shape::Segment(p, q) = m.refined else { continue };
            let Some((span, note)) = extension_span(ink, (p, q), dimension, m.width_px, &pass_through) else {
                m.adjustments.push("extension_span:not_derived".into());
                continue;
            };
            if dist(span.0, p) < 1.0 && dist(span.1, q) < 1.0 {
                m.adjustments.push(format!("{note},unchanged"));
                continue;
            }
            m.refined = Shape::Segment(span.0, span.1);
            m.support_after = segment_support(ink, span.0, span.1);
            m.width_px = measure_width(ink, span.0, span.1, 6.0);
            m.pattern = Some(measure_pattern(ink, span.0, span.1));
            m.supported = m.support_after >= 0.8 && dist(span.0, span.1) >= 2.0;
            m.adjustments.push(note);
            count += 1;
        }
    }
    count
}

fn count_by<'v>(items: impl Iterator<Item = &'v Value>, key: &str) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for v in items {
        *out.entry(v[key].as_str().unwrap_or("?").to_owned()).or_insert(0) += 1;
    }
    out
}

fn calibration_json(c: &Calibration, frame: &Frame) -> Value {
    json!({
        "schema": "pixel-synthesis.calibration/1",
        "dpi": c.dpi,
        "px_per_unit": c.px_per_unit,
        "basis": c.basis,
        "dimension_px_per_unit": c.dimension_px_per_unit,
        "residual": c.residual,
        "sample_count": c.samples.len(),
        "samples": c.samples.iter().map(|s| json!({"label": s.label, "kind": s.kind, "nominal": s.nominal, "px": fin(s.px), "ratio": fin(s.ratio), "basis": s.basis, "origin": s.origin})).collect::<Vec<_>>(),
        "scale_note": c.scale_note,
        "scale_note_px_per_unit": c.scale_note_px_per_unit,
        "deviation_percent": c.deviation_percent,
        "epsilon_drawing_units": 1.0 / c.px_per_unit,
        "drawing_rectangle": [0.0, 0.0, frame.width_units, frame.height_units],
    })
}

fn merges_json(merges: &[Merge]) -> Vec<Value> {
    merges
        .iter()
        .map(|m| json!({"kept": m.kept, "dropped": m.dropped, "swapped": m.swapped, "added": m.added}))
        .collect()
}

/// Synthesise one page: observations → refined proposals → de-duplication → calibration → graph.
pub fn synthesize_page(input: &PageInput) -> Result<SynthesisOutput> {
    let index = load_tile_index(input.tile_index)?;
    let tiles = index
        .pages
        .get(input.page)
        .with_context(|| format!("pixel_page_unknown:{}", input.page))?;
    tiles.verify_page_png(input.page_png)?;
    let ink = Ink::from_png(input.page_png, input.threshold)?;
    ensure!(
        (ink.width as u32, ink.height as u32) == tiles.page_px,
        "pixel_page_dimensions:{}x{}",
        ink.width,
        ink.height
    );
    let page_sha256 = sha256_hex(input.page_png);
    let mut files: Vec<ObservationFile> = Vec::new();
    for (path, bytes) in input.observations {
        files.push(load_observation(bytes, path)?);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let obs = page_observations(input.page, tiles, &files, input.tile_pngs)?;
    let refine_params = RefineParams::default();
    let refined: Vec<RefinedProposal> = obs
        .proposals
        .iter()
        .map(|p| refine(&ink, p, &refine_params))
        .collect();
    let proposed = refined.len();
    let (mut kept, merges) = dedupe(refined, &DedupeParams::default());
    let extension_spans = trim_extension_spans(&ink, &mut kept);
    let note = input
        .scale_note
        .map(str::to_owned)
        .or_else(|| obs.layout.as_ref().and_then(|l| l.scale_note.clone()));
    let cal = calibrate(&kept, note.as_deref(), index.dpi);
    let mut b = Builder::new(&ink, &cal, tiles.page_px, obs.layout.as_ref());
    b.annotations(&kept);
    b.centre_axes(&kept);
    b.structure(&StructureParams::default());
    let (graph, provenance, refined_records, overlay, frame, counts) = b.finish(&page_sha256);
    let entity_kinds = count_by(graph["source"]["entities"].as_array().into_iter().flatten(), "kind");
    let member_roles = count_by(graph["members"].as_array().into_iter().flatten(), "member_role");
    let member_line_types = count_by(graph["members"].as_array().into_iter().flatten(), "line_type");
    let expression_kinds = count_by(graph["expressions"].as_array().into_iter().flatten(), "kind");
    let dimensions = graph["expressions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| e["family"] == "dimension")
        .count();
    let summary = json!({
        "schema": "pixel-synthesis.summary/1",
        "page": input.page,
        "page_px": [tiles.page_px.0, tiles.page_px.1],
        "page_sha256": page_sha256,
        "observation_files": obs.files.iter().map(|f| json!({"path": f.path, "level": f.level, "tile": f.tile, "model": f.model,
            "image_sha256": f.image_sha256, "prompt_sha256": f.prompt_sha256, "parse_error": f.parse_error, "items": f.items})).collect::<Vec<_>>(),
        "tiles_verified": input.tile_pngs.len(),
        "warnings": obs.warnings,
        "proposals": {"observed": proposed, "kept": kept.len(), "merges": merges.len(), "extension_spans_rederived": extension_spans},
        "calibration": {"basis": cal.basis, "px_per_unit": cal.px_per_unit, "dimension_px_per_unit": cal.dimension_px_per_unit,
            "residual": cal.residual, "sample_count": cal.samples.len(), "scale_note": cal.scale_note,
            "scale_note_px_per_unit": cal.scale_note_px_per_unit, "deviation_percent": cal.deviation_percent},
        "entities_by_kind": entity_kinds,
        "members_by_role": member_roles,
        "members_by_line_type": member_line_types,
        "expressions_by_kind": expression_kinds,
        "dimensions": dimensions,
        "structural_lines": graph["structural_lines"].as_array().map_or(0, Vec::len),
        "implied_spans": graph["implied_spans"].as_array().map_or(0, Vec::len),
        "structure_ends": graph["structure_ends"].as_array().map_or(0, Vec::len),
        "views": graph["views"].as_array().map_or(0, Vec::len),
        "unknowns": graph["unknowns"].as_array().map_or(0, Vec::len),
        "counts": counts,
    });
    let provenance_doc = json!({
        "schema": "pixel-synthesis.provenance/1",
        "page": input.page,
        "page_sha256": page_sha256,
        "layout": obs.layout.as_ref().map(|l| json!({"file": l.file, "image_sha256": l.image_sha256, "model": l.model,
            "drawing_number": l.drawing_number, "part_name": l.part_name, "scale_note": l.scale_note,
            "frame_px": l.frame, "title_block_px": l.title_block, "views": l.views.iter().map(|v| json!({"name": v.name, "label_text": v.label_text, "scale_note": v.scale_note, "bbox_px": v.bbox})).collect::<Vec<_>>()})),
        "observation_files": summary["observation_files"],
        "merges": merges_json(&merges),
        "entities": Value::Object(provenance),
    });
    let refined_doc = json!({
        "schema": "pixel-synthesis.refined-members/1",
        "page": input.page,
        "parameters": {"corridor_px": refine_params.corridor_px, "max_gap_px": refine_params.max_gap_px, "centre_gap_px": refine_params.centre_gap_px,
            "arrow_window_px": refine_params.arrow_window_px, "text_pad_px": refine_params.text_pad_px, "support_min": refine_params.support_min},
        "members": refined_records,
        "merges": merges_json(&merges),
    });
    Ok(SynthesisOutput {
        graph: serde_json::to_vec_pretty(&graph)?,
        provenance: serde_json::to_vec_pretty(&provenance_doc)?,
        refined_members: serde_json::to_vec_pretty(&refined_doc)?,
        calibration: serde_json::to_vec_pretty(&calibration_json(&cal, &frame))?,
        overlay_svg: overlay.finish(&frame, 1600),
        summary,
        page_px: tiles.page_px,
        page_sha256,
        drawing_rectangle: [0.0, 0.0, frame.width_units, frame.height_units],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calibration() -> Calibration {
        Calibration {
            px_per_unit: 1.0,
            basis: "paper_mm".into(),
            dimension_px_per_unit: None,
            residual: None,
            samples: vec![],
            scale_note: None,
            scale_note_px_per_unit: None,
            deviation_percent: None,
            dpi: 300.0,
        }
    }

    fn piece(k: usize, x0: f64, x1: f64) -> Piece {
        Piece {
            entity: format!("PX-LINE-{k:04}"),
            member: format!("expression:0:m{k}"),
            p: (x0, 5.0),
            q: (x1, 5.0),
            view: "view:principal".into(),
            class: "visible_outline",
        }
    }

    #[test]
    fn collinear_pieces_join_across_a_text_and_split_at_a_drafting_gap() {
        let ink = Ink::from_mask(300, 100, vec![false; 30000]);
        let cal = calibration();
        let mut b = Builder::new(&ink, &cal, (300, 100), None);
        b.text_boxes.push(([45.0, 0.0, 55.0, 10.0], "PX-TEXT-0001".into()));
        // A, text gap, B (explained); 25 px drafting gap; C with D inside it and E overlapping its end.
        let pieces = vec![
            piece(0, 0.0, 40.0),
            piece(1, 60.0, 100.0),
            piece(2, 125.0, 200.0),
            piece(3, 150.0, 190.0),
            piece(4, 220.0, 180.0),
        ];
        b.line_structural_lines(pieces, &StructureParams::default());
        assert_eq!(b.structural_lines.len(), 2, "{:?}", b.structural_lines);
        let first = &b.structural_lines[0];
        assert_eq!(first["pieces"], json!(["member:expression:0:m0", "implied:0", "member:expression:0:m1"]));
        assert_eq!(first["state"], "candidate");
        assert_eq!(first["points"], json!([0.0, 95.0, 100.0, 95.0]));
        let second = &b.structural_lines[1];
        assert_eq!(second["pieces"], json!(["member:expression:0:m2", "member:expression:0:m4"]));
        assert_eq!(second["coincident"], json!(["member:expression:0:m3"]));
        assert_eq!(second["trimmed"], json!(["member:expression:0:m4:0.500000:1.000000"]));
        assert_eq!(second["points"], json!([125.0, 95.0, 220.0, 95.0]));
        assert_eq!(second["state"], "observed");
        // The implied span runs from the end of A to the start of B across the text.
        assert_eq!(b.implied_spans.len(), 1);
        let span = &b.implied_spans[0];
        assert_eq!(span["cause"], "text_break");
        assert_eq!(span["evidence_ids"], json!(["text:PX-TEXT-0001"]));
        assert_eq!(span["from_source_handle"], "PX-LINE-0000");
        assert_eq!(span["from_parameter"], 1.0);
        assert_eq!(span["to_source_handle"], "PX-LINE-0001");
        assert_eq!(span["to_parameter"], 0.0);
        assert_eq!(span["points"], json!([40.0, 95.0, 60.0, 95.0]));
        // Four ends: the two of the text break and the two of the drafting gap, partnered.
        assert_eq!(b.structure_ends.len(), 4);
        let gap: Vec<&Value> = b.structure_ends.iter().filter(|e| e["cause"] == "drafting_gap").collect();
        assert_eq!(gap.len(), 2);
        assert_eq!(gap[0]["partner_end_id"], gap[1]["id"]);
        assert_eq!(gap[1]["partner_end_id"], gap[0]["id"]);
        assert_eq!(gap[0]["source_handle"], "PX-LINE-0001");
        assert_eq!(gap[1]["source_handle"], "PX-LINE-0002");
    }

    #[test]
    fn views_and_text_geometry_follow_the_layout_and_the_text_model() {
        let ink = Ink::from_mask(10, 10, vec![false; 100]);
        let cal = Calibration {
            px_per_unit: 10.0,
            ..calibration()
        };
        let layout = Layout {
            file: "L0".into(),
            image_sha256: String::new(),
            model: "m".into(),
            frame: None,
            title_block: None,
            drawing_number: String::new(),
            part_name: String::new(),
            scale_note: None,
            technical_requirements: None,
            views: vec![
                crate::observe::L0View { name: "主视图".into(), label_text: String::new(), scale_note: String::new(), bbox: [0.0, 0.0, 500.0, 400.0] },
                crate::observe::L0View { name: "局部放大I".into(), label_text: "I".into(), scale_note: "2:1".into(), bbox: [100.0, 100.0, 200.0, 200.0] },
                crate::observe::L0View { name: "A向".into(), label_text: "A向".into(), scale_note: String::new(), bbox: [600.0, 0.0, 900.0, 300.0] },
            ],
            other_regions: vec![],
        };
        let b = Builder::new(&ink, &cal, (1000, 800), Some(&layout));
        assert_eq!(b.views.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["view:principal", "view:I2:1", "view:A向"]);
        assert_eq!(b.views[1].kind, "detail_view");
        assert_eq!(b.views[1].label, "I 2:1");
        assert_eq!(b.views[2].kind, "direction_view");
        // The innermost view wins; outside every view is the sheet.
        assert_eq!(b.view_of((150.0, 150.0)), "view:I2:1");
        assert_eq!(b.view_of((50.0, 50.0)), "view:principal");
        assert_eq!(b.view_of((950.0, 700.0)), "view:sheet");
        // A horizontal 40 px × 20 px box of "30" at 10 px per unit: height 2.1 (pixel footprint), the
        // width factor makes 1.6 heights × factor equal 4.1 units.
        let m = text_measure("30");
        let (origin, h, rot, wf, width) = b.text_geometry([100.0, 300.0, 139.0, 319.0], false, &m);
        assert_eq!(rot, 0.0);
        assert!((h - 2.0).abs() < 1e-9 && (width - 4.0).abs() < 1e-9 && (wf - 4.0 / (1.6 * 2.0)).abs() < 1e-9, "{h} {width} {wf}");
        assert!((origin.0 - 9.95).abs() < 1e-9 && (origin.1 - (800.0 - 319.5) / 10.0).abs() < 1e-9, "{origin:?}");
        // The same box vertical: rotation 90, insertion at the bottom-right corner.
        let (origin, h, rot, _, _) = b.text_geometry([100.0, 300.0, 119.0, 339.0], true, &m);
        assert_eq!(rot, 90.0);
        assert!((h - 2.0).abs() < 1e-9);
        assert!((origin.0 - 11.95).abs() < 1e-9 && (origin.1 - (800.0 - 339.5) / 10.0).abs() < 1e-9, "{origin:?}");
    }
}
