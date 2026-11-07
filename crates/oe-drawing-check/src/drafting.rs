//! Drafting rules evaluated as SHACL shapes of the drafting ontology over typed facts, for any drawing.
//!
//! The knowledge lives in `knowledge/drafting-ontology.toml`: an ontology of drawing objects and the
//! relations between them, and one shape per drafting rule with its concept, standard clause and message.
//! This module only exports facts and runs the framework's SHACL engine; it holds no rule of its own.
//! Objects take their class from the member roles of the frozen graph (the recognition's identity) and
//! their geometry from the source DXF bytes, read here without the classifier. Generic geometry predicates
//! give the relations (crossed by, coincident with, end on outline, angle). Facts are exported in three
//! layers and the same shapes are evaluated on each: `source` is the drawing as drawn, `recognition` is
//! what the graph claims (member intervals and labels), `presentation` is what the review page draws
//! (merged structural lines and inferred spans). A finding is a deviation to review; nothing is changed.
use crate::dxf;
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oxigraph::model::Term;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::Path;

pub const SCHEMA: &str = "oev2.drawing-drafting-rules/1";
pub const REQUEST_SCHEMA: &str = "oev2.drawing-drafting-rules-request/1";
pub const KNOWLEDGE_SCHEMA: &str = "oev2.drafting-ontology/1";
/// The ontology and the shapes, compiled in so that a change rebinds the program.
pub const KNOWLEDGE: &str = include_str!("../knowledge/drafting-ontology.toml");
pub const LAYERS: [&str; 3] = ["source", "recognition", "presentation"];
const NS: &str = "urn:oev2:drafting:";
const SH: &str = "http://www.w3.org/ns/shacl#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LIMIT: usize = 64 * 1024 * 1024;
const QUAD_BUDGET: usize = 100_000;
const SCOPE: &str = "Drafting rules of the ontology evaluated on facts exported from the source bytes and the frozen graph; a finding is a deviation to review, not a correction and not engineering acceptance.";

/// Every class of the fact vocabulary; the ontology and the spec declare the same set (tested).
pub const CLASSES: [&str; 27] = [
    "Line",
    "VisibleOutline",
    "HiddenOutline",
    "CentreLine",
    "BreakBoundary",
    "WavyBreakBoundary",
    "HatchLine",
    "SectionRegion",
    "DimensionLine",
    "ExtensionLine",
    "Arrowhead",
    "LeaderLine",
    "ThreadLine",
    "DetailCircle",
    "SheetLine",
    "SymbolLine",
    "InferredSpan",
    "OtherLine",
    "Text",
    "DimensionText",
    "AngularDimensionText",
    "ToleranceText",
    "NoteText",
    "TableText",
    "ViewLabel",
    "DatumLetter",
    "Construction",
];
/// Every predicate of the fact vocabulary (data and object properties); same set in the ontology and the spec.
pub const PREDICATES: [&str; 13] = [
    "layer",
    "sourceHandle",
    "memberId",
    "expressionId",
    "expressionKind",
    "view",
    "lineType",
    "text",
    "rotationDeg",
    "angleDeg",
    "endOnOutline",
    "crossedBy",
    "coincidentWith",
];

pub struct DraftingRuleChecker<'a> {
    pub root: &'a SkillRoot,
}
impl MdDraftingRulePort for DraftingRuleChecker<'_> {
    fn check_rules(
        &self,
        input: &MdDraftingRuleRequest,
    ) -> std::result::Result<MdDraftingRuleReport, FailureRecord> {
        check_rules(self.root, input).map_err(|e| crate::graph::failure("drawing_drafting_rules", e))
    }
}

/// The two Turtle documents of the knowledge file and the digest of the file.
pub struct Knowledge {
    pub ontology: String,
    pub shapes: String,
    pub sha256: String,
}
pub fn knowledge() -> Result<Knowledge> {
    let doc: toml::Value = toml::from_str(KNOWLEDGE).context("drafting_knowledge_toml")?;
    ensure!(
        doc.get("schema").and_then(toml::Value::as_str) == Some(KNOWLEDGE_SCHEMA),
        "drafting_knowledge_schema"
    );
    let part = |name: &str| -> Result<String> {
        let t = doc
            .get(name)
            .with_context(|| format!("drafting_knowledge_part_missing:{name}"))?;
        ensure!(
            t.get("format").and_then(toml::Value::as_str) == Some("text/turtle"),
            "drafting_knowledge_format:{name}"
        );
        Ok(t.get("text")
            .and_then(toml::Value::as_str)
            .with_context(|| format!("drafting_knowledge_text_missing:{name}"))?
            .to_owned())
    };
    Ok(Knowledge {
        ontology: part("ontology")?,
        shapes: part("shapes")?,
        sha256: sha256(KNOWLEDGE.as_bytes()),
    })
}

/// Classes (`owl:Class`) and fact properties (`owl:ObjectProperty`, `owl:DatatypeProperty`) the ontology declares,
/// as local names under the drafting namespace.
pub fn ontology_vocabulary(ontology: &str) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    let store = semantica_runtime::graph::load(ontology.as_bytes())?;
    let (mut classes, mut properties) = (BTreeSet::new(), BTreeSet::new());
    for q in store.iter() {
        let q = q.into_owned();
        if q.predicate.as_str() != RDF_TYPE {
            continue;
        }
        let Term::NamedNode(object) = &q.object else {
            continue;
        };
        let subject: Term = q.subject.into();
        let Term::NamedNode(s) = &subject else {
            continue;
        };
        let Some(local) = s.as_str().strip_prefix(NS) else {
            continue;
        };
        match object.as_str().strip_prefix(OWL) {
            Some("Class") => {
                classes.insert(local.to_owned());
            }
            Some("ObjectProperty" | "DatatypeProperty") => {
                properties.insert(local.to_owned());
            }
            _ => {}
        }
    }
    Ok((classes, properties))
}

/// One rule of the shapes document: the node shape, its metadata and its property shapes.
#[derive(Debug, Clone)]
pub struct Rule {
    pub shape: String,
    pub id: String,
    pub group: String,
    pub concept_ids: Vec<String>,
    pub clause: String,
    pub label: String,
    pub messages: Vec<String>,
    pub severity: String,
    pub target_classes: Vec<String>,
    pub property_shapes: Vec<String>,
    pub referenced: BTreeSet<String>,
}

/// Rules of the shapes document, read from the shapes graph; every IRI under the drafting namespace that the
/// shapes mention is collected so the vocabulary test can check it against the ontology.
pub fn rules(shapes: &str) -> Result<Vec<Rule>> {
    let store = semantica_runtime::graph::load(shapes.as_bytes())?;
    let mut by_subject: BTreeMap<String, Vec<(String, Term)>> = BTreeMap::new();
    for q in store.iter() {
        let q = q.into_owned();
        let subject: Term = q.subject.into();
        by_subject
            .entry(subject.to_string())
            .or_default()
            .push((q.predicate.as_str().to_owned(), q.object));
    }
    let iri = |s: &str| format!("<{s}>");
    let text = |t: &Term| match t {
        Term::Literal(l) => l.value().to_owned(),
        Term::NamedNode(n) => n.as_str().to_owned(),
        other => other.to_string(),
    };
    let mut rules = Vec::new();
    for (subject, triples) in &by_subject {
        let is_rule = triples
            .iter()
            .any(|(p, o)| p == RDF_TYPE && o.to_string() == iri(&format!("{SH}NodeShape")));
        if !is_rule {
            continue;
        }
        let value = |p: &str| -> Option<String> {
            triples.iter().find(|(k, _)| k == p).map(|(_, o)| text(o))
        };
        let values = |p: &str| -> Vec<String> {
            triples
                .iter()
                .filter(|(k, _)| k == p)
                .map(|(_, o)| text(o))
                .collect()
        };
        let property_shapes: Vec<String> = triples
            .iter()
            .filter(|(k, _)| *k == format!("{SH}property"))
            .map(|(_, o)| o.to_string())
            .collect();
        let mut messages = Vec::new();
        let mut referenced = BTreeSet::new();
        let mut nested: Vec<String> = Vec::new();
        let mut stack: Vec<String> = triples
            .iter()
            .filter(|(p, _)| p.starts_with(SH))
            .filter_map(|(_, o)| matches!(o, Term::BlankNode(_) | Term::NamedNode(_)).then(|| o.to_string()))
            .collect();
        let mut seen = BTreeSet::new();
        while let Some(node) = stack.pop() {
            if !seen.insert(node.clone()) {
                continue;
            }
            if node.starts_with(&format!("<{NS}rule:")) {
                nested.push(node.clone());
            }
            for (p, o) in by_subject.get(&node).map(Vec::as_slice).unwrap_or_default() {
                if *p == format!("{SH}message") {
                    messages.push(text(o));
                }
                if let Term::NamedNode(n) = o
                    && let Some(local) = n.as_str().strip_prefix(NS)
                    && !local.contains(':')
                {
                    referenced.insert(local.to_owned());
                }
                match o {
                    Term::BlankNode(_) | Term::NamedNode(_) => stack.push(o.to_string()),
                    _ => {}
                }
            }
        }
        let target_classes: Vec<String> = values(&format!("{SH}targetClass"))
            .into_iter()
            .filter_map(|c| c.strip_prefix(NS).map(str::to_owned))
            .collect();
        referenced.extend(target_classes.iter().cloned());
        let severity = value(&format!("{SH}severity"))
            .and_then(|s| s.strip_prefix(SH).map(str::to_lowercase))
            .unwrap_or_else(|| "violation".into());
        rules.push(Rule {
            shape: subject.clone(),
            id: value(&format!("{NS}ruleId")).unwrap_or_default(),
            group: value(&format!("{NS}group")).unwrap_or_default(),
            concept_ids: values(&format!("{NS}concept"))
                .into_iter()
                .filter_map(|c| {
                    c.strip_prefix(&format!("{NS}concept:"))
                        .map(str::to_owned)
                })
                .collect(),
            clause: value(&format!("{NS}clause")).unwrap_or_default(),
            label: value(&format!("{RDFS}label")).unwrap_or_default(),
            messages,
            severity,
            target_classes,
            property_shapes: {
                let mut all = property_shapes;
                for n in nested {
                    if !all.contains(&n) {
                        all.push(n);
                    }
                }
                all
            },
            referenced,
        });
    }
    rules.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rules)
}

// ------------------------------------------------------------------------------------------------ geometry

type P = (f64, f64);
const EPS_COINCIDENT: f64 = 0.15;
const MIN_OVERLAP: f64 = 1.0;
const END_TOLERANCE: f64 = 0.5;
/// A straight line that ends on a wavy curve meets it inside the wave's band: pieces of the curve up to this far from
/// the line's end may still intersect it without the line crossing the curve.
const WAVE_BAND: f64 = 2.0;
/// Stacked tolerances are drawn one above the other at reduced size, so a text with a stack is this many heights tall.
const STACK_HEIGHT: f64 = 1.3;

/// Geometry of one drawing object: straight pieces (chains of points) and, for text, a rotated box.
#[derive(Debug, Clone, Default)]
struct Geometry {
    /// Polylines: a LINE is one two-point chain; arcs and circles are sampled; splines use their points.
    chains: Vec<Vec<P>>,
    /// A text box: anchor of the lower-left corner in drawing coordinates, rotation, width and height.
    text_box: Option<TextBox>,
}
#[derive(Debug, Clone, Copy)]
struct TextBox {
    origin: P,
    rotation_rad: f64,
    width: f64,
    height: f64,
}
impl TextBox {
    /// Box coordinates of a point: x along the text, y up from the lower-left corner.
    fn local(&self, p: P) -> P {
        let (c, s) = (self.rotation_rad.cos(), self.rotation_rad.sin());
        let (dx, dy) = (p.0 - self.origin.0, p.1 - self.origin.1);
        (dx * c + dy * s, -dx * s + dy * c)
    }
    /// A straight segment parallel to the text that runs along the bottom band of the box.
    fn is_shelf(&self, a: P, b: P) -> bool {
        let (p0, p1) = (self.local(a), self.local(b));
        let band = 0.3 * self.height;
        (p1.1 - p0.1).abs() <= 0.05 * dist(a, b).max(1e-9) && p0.1.abs() <= band && p1.1.abs() <= band
    }
    /// The segment in box coordinates (x along the text, y up), clipped to the box; its length inside.
    fn chord(&self, a: P, b: P) -> f64 {
        let (c, s) = (self.rotation_rad.cos(), self.rotation_rad.sin());
        let local = |p: P| {
            let (dx, dy) = (p.0 - self.origin.0, p.1 - self.origin.1);
            (dx * c + dy * s, -dx * s + dy * c)
        };
        let (p0, p1) = (local(a), local(b));
        // Liang–Barsky against the box shrunk by a tenth of the height on every side: glyphs do not fill the em
        // box, and a line along the box edge (a leader shelf under its note) is not a crossing.
        let m = 0.1 * self.height;
        let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
        let (mut t0, mut t1) = (0f64, 1f64);
        for (p, q) in [
            (-dx, p0.0 - m),
            (dx, self.width - m - p0.0),
            (-dy, p0.1 - m),
            (dy, self.height - m - p0.1),
        ] {
            if p.abs() < 1e-12 {
                if q < 0. {
                    return 0.;
                }
                continue;
            }
            let r = q / p;
            if p < 0. {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return 0.;
            }
        }
        ((t1 - t0) * (dx * dx + dy * dy).sqrt()).max(0.)
    }
}

fn dist(a: P, b: P) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
fn sample_arc(c: P, r: f64, a0: f64, a1: f64) -> Vec<P> {
    let mut sweep = a1 - a0;
    while sweep <= 0. {
        sweep += 2. * PI;
    }
    let n = ((sweep / (2. * PI)) * 64.).ceil().clamp(8., 64.) as usize;
    (0..=n)
        .map(|i| {
            let t = a0 + sweep * i as f64 / n as f64;
            (c.0 + r * t.cos(), c.1 + r * t.sin())
        })
        .collect()
}
/// Distance from a point to a segment and the parameter of the nearest point.
fn point_segment(p: P, a: P, b: P) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    if l2 < 1e-18 {
        return (dist(p, a), 0.);
    }
    let u = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0., 1.);
    (dist(p, (a.0 + u * dx, a.1 + u * dy)), u)
}
/// Intersection point of two segments, ends included; none when parallel.
fn segment_intersection(a: P, b: P, c: P, d: P) -> Option<P> {
    let (r, s) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
    let denominator = r.0 * s.1 - r.1 * s.0;
    if denominator.abs() < 1e-12 || dist(a, b) < 1e-9 || dist(c, d) < 1e-9 {
        return None;
    }
    let qp = (c.0 - a.0, c.1 - a.1);
    let t = (qp.0 * s.1 - qp.1 * s.0) / denominator;
    let u = (qp.0 * r.1 - qp.1 * r.0) / denominator;
    let inside = |x: f64| (-1e-9..=1. + 1e-9).contains(&x);
    (inside(t) && inside(u)).then(|| (a.0 + t * r.0, a.1 + t * r.1))
}
/// Two curves cross when some pieces intersect at a point away from the ends of both curves: a corner shared by
/// two outlines and a line ending on another are not crossings; a line through a vertex of a polyline is.
fn chains_cross(ca: &[P], cb: &[P]) -> bool {
    let (Some(a0), Some(a1), Some(b0), Some(b1)) = (ca.first(), ca.last(), cb.first(), cb.last()) else {
        return false;
    };
    let ends = [*a0, *a1, *b0, *b1];
    // A straight line ending on a curve (a T-junction) is not a crossing even where the curve's waves run back
    // across the line near that end.
    let t_junction = |p: P| -> bool {
        let check = |straight: &[P], curve: &[P]| {
            straight.len() == 2
                && curve.len() >= 3
                && straight.iter().any(|e| {
                    dist(*e, p) <= WAVE_BAND
                        && curve
                            .windows(2)
                            .any(|w| point_segment(*e, w[0], w[1]).0 <= END_TOLERANCE)
                })
        };
        check(ca, cb) || check(cb, ca)
    };
    ca.windows(2).any(|wa| {
        cb.windows(2).any(|wb| {
            segment_intersection(wa[0], wa[1], wb[0], wb[1])
                .is_some_and(|p| ends.iter().all(|e| dist(*e, p) > END_TOLERANCE) && !t_junction(p))
        })
    })
}
/// Two straight segments on the same line, overlapping along it.
fn segments_coincident(a: P, b: P, c: P, d: P) -> bool {
    let (la, lc) = (dist(a, b), dist(c, d));
    if la < 1e-9 || lc < 1e-9 {
        return false;
    }
    let ua = ((b.0 - a.0) / la, (b.1 - a.1) / la);
    let uc = ((d.0 - c.0) / lc, (d.1 - c.1) / lc);
    let cross = ua.0 * uc.1 - ua.1 * uc.0;
    if cross.abs() > (1f64).to_radians().sin() {
        return false;
    }
    let offset = |p: P| ((p.0 - a.0) * -ua.1 + (p.1 - a.1) * ua.0).abs();
    if offset(c) > EPS_COINCIDENT || offset(d) > EPS_COINCIDENT {
        return false;
    }
    let along = |p: P| (p.0 - a.0) * ua.0 + (p.1 - a.1) * ua.1;
    let (c0, c1) = (along(c).min(along(d)), along(c).max(along(d)));
    (c1.min(la) - c0.max(0.)) >= MIN_OVERLAP
}
/// A curve chain lying along a straight segment: most of its points on the segment, parallel overall.
fn chain_coincident(chain: &[P], a: P, b: P) -> bool {
    if chain.len() < 3 {
        return false;
    }
    let on = chain
        .iter()
        .filter(|p| point_segment(**p, a, b).0 <= EPS_COINCIDENT)
        .count();
    if on * 2 < chain.len() {
        return false;
    }
    let (first, last) = (chain[0], chain[chain.len() - 1]);
    let l = dist(first, last);
    let lab = dist(a, b);
    if l < 1e-9 || lab < 1e-9 {
        return false;
    }
    let cross = ((last.0 - first.0) * (b.1 - a.1) - (last.1 - first.1) * (b.0 - a.0)) / (l * lab);
    cross.abs() <= (10f64).to_radians().sin()
}
fn angle_deg(a: P, b: P) -> f64 {
    let mut d = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
    while d < 0. {
        d += 180.;
    }
    while d >= 180. {
        d -= 180.;
    }
    (d * 1000.).round() / 1000.
}

// ------------------------------------------------------------------------------------------------ DXF reading

/// Records of the DXF indexed for geometry: by handle, block members by block name, polyline vertices.
struct Dxf<'a> {
    by_handle: HashMap<&'a str, &'a dxf::Record>,
    blocks: HashMap<&'a str, Vec<&'a dxf::Record>>,
    vertices: HashMap<&'a str, Vec<P>>,
    dim_text_height: HashMap<&'a str, f64>,
    /// Width factor of each text style (STYLE group 41); MTEXT and unscaled TEXT take it from their style.
    style_width: HashMap<&'a str, f64>,
}
impl<'a> Dxf<'a> {
    fn new(records: &'a [dxf::Record]) -> Result<Self> {
        let mut d = Dxf {
            by_handle: HashMap::new(),
            blocks: HashMap::new(),
            vertices: HashMap::new(),
            dim_text_height: HashMap::new(),
            style_width: HashMap::new(),
        };
        let mut block: Option<&'a str> = None;
        let mut polyline: Option<&'a str> = None;
        for r in records {
            if let Some(h) = r.tag(5).filter(|h| !h.is_empty()) {
                d.by_handle.insert(h, r);
            }
            match r.kind.as_str() {
                "BLOCK" => block = r.tag(2),
                "ENDBLK" => block = None,
                "DIMSTYLE" => {
                    if let (Some(name), Some(h)) = (r.tag(2), r.tag(140)) {
                        d.dim_text_height.insert(name, h.parse().unwrap_or(2.5));
                    }
                }
                "STYLE" => {
                    if let (Some(name), Ok(w)) = (r.tag(2), r.float(41))
                        && w > 0.
                    {
                        d.style_width.insert(name, w);
                    }
                }
                "POLYLINE" => polyline = r.tag(5),
                "VERTEX" => {
                    if let (Some(p), Ok(x), Ok(y)) = (polyline, r.float(10), r.float(20)) {
                        d.vertices.entry(p).or_default().push((x, y));
                    }
                }
                "SEQEND" => polyline = None,
                _ => {}
            }
            if let Some(name) = block
                && r.kind != "BLOCK"
            {
                d.blocks.entry(name).or_default().push(r);
            }
        }
        Ok(d)
    }

    /// Straight chains of a curve record, in drawing coordinates plus an offset.
    fn chains(&self, r: &dxf::Record, offset: P) -> Result<Vec<Vec<P>>> {
        let pt = |x: i64, y: i64| -> Result<P> { Ok((r.float(x)? + offset.0, r.float(y)? + offset.1)) };
        Ok(match r.kind.as_str() {
            "LINE" => vec![vec![pt(10, 20)?, pt(11, 21)?]],
            "ARC" => vec![sample_arc(
                pt(10, 20)?,
                r.float(40)?,
                r.float(50)?.to_radians(),
                r.float(51)?.to_radians(),
            )],
            "CIRCLE" => vec![sample_arc(pt(10, 20)?, r.float(40)?, 0., 2. * PI)],
            "LWPOLYLINE" => {
                let mut pts = Vec::new();
                let mut x = None;
                for (code, value) in &r.tags {
                    match code {
                        10 => x = Some(value.parse::<f64>().context("dxf_vertex_x")?),
                        20 => {
                            if let Some(px) = x.take() {
                                pts.push((px + offset.0, value.parse::<f64>().context("dxf_vertex_y")? + offset.1));
                            }
                        }
                        _ => {}
                    }
                }
                if r.tag(70).and_then(|f| f.parse::<i64>().ok()).unwrap_or(0) & 1 == 1
                    && pts.len() > 2
                {
                    pts.push(pts[0]);
                }
                vec![pts]
            }
            "POLYLINE" => {
                let pts: Vec<P> = self
                    .vertices
                    .get(r.tag_or(5, ""))
                    .map(|v| v.iter().map(|p| (p.0 + offset.0, p.1 + offset.1)).collect())
                    .unwrap_or_default();
                vec![pts]
            }
            "SPLINE" => {
                let mut fit = Vec::new();
                let mut control = Vec::new();
                let (mut fx, mut cx) = (None, None);
                for (code, value) in &r.tags {
                    match code {
                        11 => fx = Some(value.parse::<f64>().context("dxf_fit_x")?),
                        21 => {
                            if let Some(x) = fx.take() {
                                fit.push((x + offset.0, value.parse::<f64>().context("dxf_fit_y")? + offset.1));
                            }
                        }
                        10 => cx = Some(value.parse::<f64>().context("dxf_control_x")?),
                        20 => {
                            if let Some(x) = cx.take() {
                                control.push((x + offset.0, value.parse::<f64>().context("dxf_control_y")? + offset.1));
                            }
                        }
                        _ => {}
                    }
                }
                vec![if fit.len() >= 2 { fit } else { control }]
            }
            _ => Vec::new(),
        })
    }

    /// Text box of a TEXT, MTEXT or ATTRIB record; the cleaned text and its rotation.
    fn text_box(&self, r: &dxf::Record, offset: P) -> Result<Option<(TextBox, String, f64)>> {
        let height = match r.float(40) {
            Ok(h) if h > 0. => h,
            _ => return Ok(None),
        };
        let raw = r
            .tags
            .iter()
            .filter(|(c, _)| *c == 3)
            .map(|(_, v)| v.as_str())
            .chain(r.tag(1))
            .collect::<String>();
        let (cleaned, width_units, stacked) = clean_text(&raw);
        let style_width = d_style_width(self, r);
        let width_factor = r
            .float(41)
            .ok()
            .filter(|w| *w > 0. && r.kind != "MTEXT")
            .unwrap_or(style_width);
        let width = (width_units * height * width_factor).max(0.5 * height);
        let mut rotation = r.float(50).unwrap_or(0.);
        if r.kind == "MTEXT"
            && let (Ok(dx), Ok(dy)) = (r.float(11), r.float(21))
            && (dx.abs() > 1e-9 || dy.abs() > 1e-9)
        {
            rotation = dy.atan2(dx).to_degrees();
        }
        let rad = rotation.to_radians();
        let (c, s) = (rad.cos(), rad.sin());
        // Anchor and its position on the box: TEXT uses 72/73 with the alignment point; MTEXT uses 71.
        let (anchor, ax, ay) = if r.kind == "MTEXT" {
            let attachment = r.tag(71).and_then(|v| v.parse::<i64>().ok()).unwrap_or(1);
            let ax = match (attachment - 1) % 3 {
                0 => 0.,
                1 => 0.5,
                _ => 1.,
            };
            let ay = match (attachment - 1) / 3 {
                0 => 1.,
                1 => 0.5,
                _ => 0.,
            };
            ((r.float(10)? + offset.0, r.float(20)? + offset.1), ax, ay)
        } else {
            let h = r.tag(72).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let v = r.tag(73).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let aligned = (h != 0 || v != 0) && r.float(11).is_ok() && r.float(21).is_ok();
            let anchor = if aligned {
                (r.float(11)? + offset.0, r.float(21)? + offset.1)
            } else {
                (r.float(10)? + offset.0, r.float(20)? + offset.1)
            };
            let ax = match h {
                1 | 4 => 0.5,
                2 => 1.,
                3 | 5 => 0.,
                _ => 0.,
            };
            let ay = match (h, v) {
                (4, _) => 0.5,
                (_, 2) => 0.5,
                (_, 3) => 1.,
                _ => 0.,
            };
            (anchor, ax, ay)
        };
        // A stacked tolerance rises above and below the main line: the box grows to STACK_HEIGHT heights around it.
        let height_box = if stacked { STACK_HEIGHT * height } else { height };
        let extra = height_box - height;
        // Lower-left corner: move back from the anchor along the text and up directions.
        let (bx, by) = (-ax * width, -ay * height - extra * (1. - ay) * 0.5 - extra * ay * 0.5);
        let origin = (anchor.0 + bx * c - by * s, anchor.1 + bx * s + by * c);
        let height = height_box;
        let mut normalized = rotation % 180.;
        if normalized < 0. {
            normalized += 180.;
        }
        Ok(Some((
            TextBox {
                origin,
                rotation_rad: rad,
                width,
                height,
            },
            cleaned,
            (normalized * 1000.).round() / 1000.,
        )))
    }
}

/// Width factor of the style a text record names (group 7), 1 when the style is unknown.
fn d_style_width(d: &Dxf, r: &dxf::Record) -> f64 {
    d.style_width
        .get(r.tag_or(7, "STANDARD"))
        .copied()
        .unwrap_or(1.)
}

/// Text without MTEXT formatting codes, its width in text heights (stacked parts at their scale), and whether it
/// holds a stacked part. Besides MTEXT `\S` stacks, TEXT strings of Chinese CAD fonts stack a tolerance with the
/// private-use marks U+E1D6 (upper part begins), U+E1DA (upper ends), U+E1D7 (lower begins), U+E1D9 (lower ends):
/// the two parts are drawn one above the other at reduced size, so together they are as wide as the wider one.
fn clean_text(raw: &str) -> (String, f64, bool) {
    let mut out = String::new();
    let mut width = 0f64;
    let mut scale = 1f64;
    let mut stacked = false;
    let mut stack_scales: Vec<f64> = Vec::new();
    // Font stacking marks: (part being collected, its width so far, width of the other part already closed).
    let mut mark_part: Option<(char, f64)> = None;
    let mut mark_pending = 0f64;
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    let char_width = |c: char| -> f64 {
        if c.is_ascii() {
            match c {
                '.' | ',' | ':' | ';' | '\'' | ' ' => 0.45,
                '1' | 'i' | 'l' | 'I' | '|' | '(' | ')' => 0.6,
                _ => 0.8,
            }
        } else {
            1.0
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, '\u{E1D6}' | '\u{E1D7}') {
            stacked = true;
            mark_part = Some((c, 0.));
            if c == '\u{E1D7}' && !out.ends_with('/') && mark_pending > 0. {
                out.push('/');
            }
            i += 1;
            continue;
        }
        if matches!(c, '\u{E1DA}' | '\u{E1D9}') {
            if let Some((_, w)) = mark_part.take() {
                if c == '\u{E1DA}' {
                    mark_pending = w;
                } else {
                    width += w.max(mark_pending) * 0.6;
                    mark_pending = 0.;
                }
            }
            i += 1;
            continue;
        }
        if ('\u{E000}'..='\u{F8FF}').contains(&c) {
            i += 1;
            continue;
        }
        if let Some((part, w)) = mark_part.as_mut() {
            let _ = part;
            out.push(c);
            *w += char_width(c);
            i += 1;
            continue;
        }
        if c == '%' && i + 2 < chars.len() && chars[i + 1] == '%' {
            let code = chars[i + 2].to_ascii_lowercase();
            let symbol = match code {
                'c' => Some('Ø'),
                'd' => Some('°'),
                'p' => Some('±'),
                _ => None,
            };
            if let Some(s) = symbol {
                out.push(s);
                width += char_width(s) * scale;
                i += 3;
                continue;
            }
        }
        if c == '\\' && i + 1 < chars.len() {
            let code = chars[i + 1];
            match code {
                'P' | 'X' => {
                    out.push(' ');
                    i += 2;
                    continue;
                }
                '~' => {
                    out.push(' ');
                    width += 0.45 * scale;
                    i += 2;
                    continue;
                }
                '\\' | '{' | '}' => {
                    out.push(code);
                    width += char_width(code) * scale;
                    i += 2;
                    continue;
                }
                'S' => {
                    // Stacked fraction up to ';': the two parts share the width of the wider one.
                    let end = chars[i + 2..]
                        .iter()
                        .position(|c| *c == ';')
                        .map(|p| i + 2 + p)
                        .unwrap_or(chars.len());
                    let body: String = chars[i + 2..end].iter().collect();
                    let parts: Vec<&str> = body.splitn(2, ['^', '/', '#']).collect();
                    let wide = parts
                        .iter()
                        .map(|p| p.chars().map(char_width).sum::<f64>())
                        .fold(0., f64::max);
                    width += wide * scale;
                    stacked = true;
                    out.push_str(&parts.join("/"));
                    i = end + 1;
                    continue;
                }
                'H' => {
                    let end = chars[i + 2..]
                        .iter()
                        .position(|c| *c == ';')
                        .map(|p| i + 2 + p)
                        .unwrap_or(chars.len());
                    let body: String = chars[i + 2..end].iter().collect();
                    if let Some(factor) = body.strip_suffix('x').and_then(|f| f.parse::<f64>().ok()) {
                        scale *= factor;
                    }
                    i = end + 1;
                    continue;
                }
                _ => {
                    // Other codes (\A1; \f...; \C1; \L \O \W \Q \T) run to ';' or are single letters.
                    let end = chars[i + 2..]
                        .iter()
                        .position(|c| *c == ';')
                        .map(|p| i + 2 + p);
                    i = match (code, end) {
                        ('L' | 'O' | 'l' | 'o' | 'K' | 'k' | 'N', _) => i + 2,
                        (_, Some(e)) => e + 1,
                        _ => i + 2,
                    };
                    continue;
                }
            }
        }
        if c == '{' {
            stack_scales.push(scale);
            i += 1;
            continue;
        }
        if c == '}' {
            scale = stack_scales.pop().unwrap_or(1.);
            i += 1;
            continue;
        }
        out.push(c);
        width += char_width(c) * scale;
        i += 1;
    }
    if let Some((_, w)) = mark_part.take() {
        width += w.max(mark_pending) * 0.6;
    } else if mark_pending > 0. {
        width += mark_pending * 0.6;
    }
    (out.trim().to_owned(), width, stacked)
}

// ------------------------------------------------------------------------------------------------ objects

/// One object of a layer with its classes, identities and geometry.
#[derive(Debug, Clone)]
struct Object {
    id: String,
    classes: Vec<&'static str>,
    source_handles: Vec<String>,
    member_ids: Vec<String>,
    expression_id: Option<String>,
    expression_kind: Option<String>,
    view: Option<String>,
    line_type: Option<String>,
    text: Option<String>,
    rotation_deg: Option<f64>,
    angle_deg: Option<f64>,
    geometry: Geometry,
}
impl Object {
    fn is(&self, class: &str) -> bool {
        self.classes.contains(&class)
    }
    fn is_line(&self) -> bool {
        self.is("Line")
    }
    fn is_text(&self) -> bool {
        self.is("Text")
    }
}

/// The class of a member from its role, the source kind and the expression kind.
fn classes_of(role: &str, line_type: &str, source_kind: &str, expression_kind: &str) -> Vec<&'static str> {
    let text = |extra: &'static str| vec!["Text", extra];
    let line = |extra: &'static str| vec!["Line", extra];
    match role {
        "dimension_text" => {
            if expression_kind == "dimension_angle" {
                vec!["Text", "DimensionText", "AngularDimensionText"]
            } else {
                text("DimensionText")
            }
        }
        "tolerance_text" | "tolerance_value" | "roughness_value" => {
            vec!["Text", "ToleranceText", "DimensionText"]
        }
        "note_text" => text("NoteText"),
        "table_text" | "scale_text" => text("TableText"),
        "view_letter" | "view_title" => text("ViewLabel"),
        "datum_letter" | "datum_reference" => text("DatumLetter"),
        "dimension_line" => line("DimensionLine"),
        "extension_line" | "extension_origin" => line("ExtensionLine"),
        "arrowhead" | "datum_terminal" => vec!["Arrowhead"],
        "leader_line" => line("LeaderLine"),
        "center_line" => line("CentreLine"),
        "contour_segment" => line("VisibleOutline"),
        "hidden_segment" => line("HiddenOutline"),
        "break_curve" => {
            if line_type == "wavy" {
                vec!["Line", "BreakBoundary", "WavyBreakBoundary"]
            } else {
                line("BreakBoundary")
            }
        }
        "hatch_fill" => {
            if source_kind == "HATCH" {
                vec!["SectionRegion"]
            } else {
                line("HatchLine")
            }
        }
        "thread_line" => line("ThreadLine"),
        "detail_circle" => line("DetailCircle"),
        "frame_cell" | "table_line" | "frame_border" => line("SheetLine"),
        "symbol_glyph" | "datum_frame" | "roughness_symbol" | "special_mark" | "view_arrow"
        | "cutting_stroke" => {
            if matches!(source_kind, "TEXT" | "MTEXT" | "ATTRIB") {
                text("DatumLetter")
            } else {
                line("SymbolLine")
            }
        }
        "definition_point" | "native_dimension" | "native_block" | "coincident_source" => {
            vec!["Construction"]
        }
        _ => {
            if matches!(source_kind, "TEXT" | "MTEXT" | "ATTRIB") {
                text("NoteText")
            } else {
                line("OtherLine")
            }
        }
    }
}

fn get<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .with_context(|| format!("drafting_field_missing:{key}"))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    get(v, key)?
        .as_str()
        .with_context(|| format!("drafting_field_not_text:{key}"))
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    get(v, key)?
        .as_array()
        .with_context(|| format!("drafting_field_not_list:{key}"))
}
fn optional_text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn numbers(v: &Value, key: &str) -> Result<Vec<f64>> {
    list(v, key)?
        .iter()
        .map(|x| x.as_f64().context("drafting_not_a_number"))
        .collect()
}

/// Objects of one layer from the graph members and the DXF records.
fn objects(layer: &str, g: &Value, d: &Dxf) -> Result<Vec<Object>> {
    let expressions: HashMap<&str, &Value> = list(g, "expressions")?
        .iter()
        .map(|e| Ok((text(e, "id")?, e)))
        .collect::<Result<_>>()?;
    let labels: HashMap<&str, &str> = expressions
        .iter()
        .map(|(id, e)| (*id, e.get("label").and_then(Value::as_str).unwrap_or("")))
        .collect();
    let entities: HashMap<&str, &Value> = list(get(g, "source")?, "entities")?
        .iter()
        .map(|e| Ok((text(e, "id")?, e)))
        .collect::<Result<_>>()?;
    // Members replaced by merged structural lines in the presentation layer.
    let mut merged_pieces: BTreeSet<String> = BTreeSet::new();
    if layer == "presentation" {
        for line in list(g, "structural_lines")? {
            for piece in list(line, "pieces")? {
                if let Some(p) = piece.as_str().and_then(|p| p.strip_prefix("member:")) {
                    merged_pieces.insert(p.to_owned());
                }
            }
        }
    }
    // Roles every source handle carries: a line lent to another use (an outline serving as an extension line, an
    // axis as a boundary) keeps the class of what it is.
    let mut roles_by_handle: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for m in list(g, "members")? {
        roles_by_handle
            .entry(text(m, "source_handle")?)
            .or_default()
            .insert(text(m, "member_role")?);
    }
    let mut out = Vec::new();
    for m in list(g, "members")? {
        let member_id = text(m, "id")?;
        if merged_pieces.contains(member_id) {
            continue;
        }
        let root = text(m, "source_handle")?;
        let native = m.get("native_member_id").and_then(Value::as_str).unwrap_or(root);
        let handle = native.rsplit('/').next().unwrap_or(root);
        let role = text(m, "member_role")?;
        let line_type = text(m, "line_type")?;
        let expression_id = text(m, "expression_id")?;
        let expression_kind = expressions
            .get(expression_id)
            .and_then(|e| e.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let root_kind = entities
            .get(root)
            .and_then(|e| e.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(record) = d.by_handle.get(handle) else {
            continue;
        };
        let mut classes = classes_of(role, line_type, &record.kind, expression_kind);
        if classes == ["Construction"] && record.kind == "DIMENSION" {
            continue;
        }
        if classes.contains(&"Line") && handle == root {
            let shared = roles_by_handle.get(root).map(BTreeSet::iter).into_iter().flatten();
            for (other_role, class) in shared.filter_map(|r| match *r {
                "contour_segment" => Some((*r, "VisibleOutline")),
                "center_line" => Some((*r, "CentreLine")),
                "hidden_segment" => Some((*r, "HiddenOutline")),
                _ => None,
            }) {
                if other_role != role && !classes.contains(&class) {
                    classes.push(class);
                }
            }
        }
        // Members of a dimension display block carry the block offset of their DIMENSION (group 12/22).
        let offset = if handle != root && root_kind == "DIMENSION" {
            d.by_handle
                .get(root)
                .and_then(|dim| Some((dim.float(12).ok()?, dim.float(22).ok()?)))
                .unwrap_or((0., 0.))
        } else {
            (0., 0.)
        };
        let mut geometry = Geometry::default();
        let mut text_value = None;
        let mut rotation = None;
        let mut angle = None;
        match record.kind.as_str() {
            "TEXT" | "MTEXT" | "ATTRIB" => {
                if let Some((tb, cleaned, rot)) = d.text_box(record, offset)? {
                    geometry.text_box = Some(tb);
                    text_value = Some(cleaned);
                    rotation = Some(rot);
                }
            }
            "HATCH" => {
                angle = Some(record.float(52).unwrap_or(0.).rem_euclid(180.));
            }
            "DIMENSION" if classes.contains(&"Text") => {
                // A dimension text without a display-block record: the midpoint and the style height.
                let style = record.tag_or(3, "STANDARD");
                let height = d.dim_text_height.get(style).copied().unwrap_or(2.5);
                let (cleaned, width_units, stacked) = clean_text(record.tag_or(1, ""));
                if let (Ok(x), Ok(y)) = (record.float(11), record.float(21)) {
                    let width = (width_units * height).max(0.5 * height);
                    let height = if stacked { STACK_HEIGHT * height } else { height };
                    geometry.text_box = Some(TextBox {
                        origin: (x - width / 2., y - height / 2.),
                        rotation_rad: 0.,
                        width,
                        height,
                    });
                    text_value = Some(cleaned);
                    rotation = Some(0.);
                }
            }
            _ => {
                let mut chains = d.chains(record, offset)?;
                if layer == "recognition"
                    && record.kind == "LINE"
                    && let Ok(interval) = numbers(m, "interval")
                    && interval.len() == 2
                    && let Some(c) = chains.first_mut()
                    && c.len() == 2
                {
                    let (a, b) = (c[0], c[1]);
                    let at = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                    *c = vec![at(interval[0]), at(interval[1])];
                }
                if classes.contains(&"HatchLine")
                    && let Some(c) = chains.first()
                    && c.len() == 2
                {
                    angle = Some(angle_deg(c[0], c[1]));
                }
                geometry.chains = chains;
            }
        }
        if classes.contains(&"Text") && geometry.text_box.is_none() {
            continue;
        }
        if layer == "recognition" && role == "dimension_text" {
            // The recognition layer carries the label the graph gives the dimension, not the raw text.
            if let Some(label) = labels.get(expression_id).filter(|l| !l.is_empty()) {
                text_value = Some((*label).to_owned());
            }
        }
        out.push(Object {
            id: format!("{layer}:{member_id}"),
            classes,
            source_handles: vec![root.to_owned()],
            member_ids: vec![member_id.to_owned()],
            expression_id: Some(expression_id.to_owned()),
            expression_kind: Some(expression_kind.to_owned()),
            view: optional_text(m, "view_id"),
            line_type: Some(line_type.to_owned()),
            text: text_value,
            rotation_deg: rotation,
            angle_deg: angle,
            geometry,
        });
    }
    merge_boundaries(&mut out);
    if layer == "presentation" {
        let member_sources: HashMap<&str, (&str, &str)> = list(g, "members")?
            .iter()
            .filter_map(|m| {
                Some((
                    m.get("id")?.as_str()?,
                    (m.get("source_handle")?.as_str()?, m.get("view_id").and_then(Value::as_str).unwrap_or("")),
                ))
            })
            .collect();
        for (i, line) in list(g, "structural_lines")?.iter().enumerate() {
            let class = match text(line, "line_class")? {
                "visible_outline" => "VisibleOutline",
                "hidden_outline" => "HiddenOutline",
                _ => "OtherLine",
            };
            let pieces: Vec<&str> = list(line, "pieces")?
                .iter()
                .filter_map(|p| p.as_str()?.strip_prefix("member:"))
                .collect();
            let mut handles: Vec<String> = pieces
                .iter()
                .filter_map(|p| member_sources.get(p).map(|(h, _)| (*h).to_owned()))
                .collect();
            handles.sort();
            handles.dedup();
            out.push(Object {
                id: format!("{layer}:line:{i}"),
                classes: vec!["Line", class],
                source_handles: handles,
                member_ids: pieces.iter().map(|p| (*p).to_owned()).collect(),
                expression_id: None,
                expression_kind: None,
                view: optional_text(line, "view_id"),
                line_type: optional_text(line, "line_type"),
                text: None,
                rotation_deg: None,
                angle_deg: None,
                geometry: Geometry {
                    chains: vec![structural_chain(line)?],
                    text_box: None,
                },
            });
        }
        for (i, span) in list(g, "implied_spans")?.iter().enumerate() {
            let class = match text(span, "line_class")? {
                "visible_outline" => "VisibleOutline",
                "hidden_outline" => "HiddenOutline",
                _ => "OtherLine",
            };
            let handles = ["from_source_handle", "to_source_handle"]
                .iter()
                .filter_map(|k| optional_text(span, k))
                .collect();
            out.push(Object {
                id: format!("{layer}:implied:{i}"),
                classes: vec!["Line", class, "InferredSpan"],
                source_handles: handles,
                member_ids: Vec::new(),
                expression_id: None,
                expression_kind: optional_text(span, "cause"),
                view: optional_text(span, "view_id"),
                line_type: None,
                text: None,
                rotation_deg: None,
                angle_deg: None,
                geometry: Geometry {
                    chains: vec![structural_chain(span)?],
                    text_box: None,
                },
            });
        }
    }
    Ok(out)
}

/// Break boundaries drawn as several pieces (short lines, polylines) are one boundary each: pieces of the same
/// view whose ends meet are joined into one chain object, so that a rule about the boundary's ends and crossings
/// sees the whole curve.
fn merge_boundaries(objects: &mut Vec<Object>) {
    let mut by_expression: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, o) in objects.iter().enumerate() {
        if o.is("BreakBoundary") && o.geometry.chains.len() == 1 && o.geometry.chains[0].len() >= 2 {
            by_expression
                .entry(o.view.clone().unwrap_or_default())
                .or_default()
                .push(i);
        }
    }
    let mut remove = BTreeSet::new();
    let mut merged = Vec::new();
    for (_, indices) in by_expression {
        if indices.len() < 2 {
            continue;
        }
        let mut chains: Vec<(Vec<P>, Vec<usize>)> = indices
            .iter()
            .map(|i| (objects[*i].geometry.chains[0].clone(), vec![*i]))
            .collect();
        loop {
            let mut joined = None;
            'search: for a in 0..chains.len() {
                for b in (a + 1)..chains.len() {
                    let (ca, cb) = (&chains[a].0, &chains[b].0);
                    let (a0, a1) = (ca[0], ca[ca.len() - 1]);
                    let (b0, b1) = (cb[0], cb[cb.len() - 1]);
                    let order = if dist(a1, b0) <= END_TOLERANCE {
                        Some((false, false))
                    } else if dist(a1, b1) <= END_TOLERANCE {
                        Some((false, true))
                    } else if dist(a0, b0) <= END_TOLERANCE {
                        Some((true, false))
                    } else if dist(a0, b1) <= END_TOLERANCE {
                        Some((true, true))
                    } else {
                        None
                    };
                    if let Some((reverse_a, reverse_b)) = order {
                        joined = Some((a, b, reverse_a, reverse_b));
                        break 'search;
                    }
                }
            }
            let Some((a, b, reverse_a, reverse_b)) = joined else {
                break;
            };
            let (mut cb, mut ib) = chains.remove(b);
            let (ca, ia) = &mut chains[a];
            if reverse_a {
                ca.reverse();
            }
            if reverse_b {
                cb.reverse();
            }
            ca.extend(cb.into_iter().skip(1));
            ia.append(&mut ib);
        }
        for (chain, members) in chains {
            if members.len() < 2 {
                continue;
            }
            let first = &objects[members[0]];
            let mut classes = first.classes.clone();
            if members.iter().any(|i| objects[*i].is("WavyBreakBoundary")) && !classes.contains(&"WavyBreakBoundary") {
                classes.push("WavyBreakBoundary");
            }
            let mut handles: Vec<String> = members.iter().flat_map(|i| objects[*i].source_handles.clone()).collect();
            handles.sort();
            handles.dedup();
            merged.push(Object {
                id: format!("{}+{}", first.id, members.len() - 1),
                classes,
                source_handles: handles,
                member_ids: members.iter().flat_map(|i| objects[*i].member_ids.clone()).collect(),
                expression_id: first.expression_id.clone(),
                expression_kind: first.expression_kind.clone(),
                view: first.view.clone(),
                line_type: first.line_type.clone(),
                text: None,
                rotation_deg: None,
                angle_deg: None,
                geometry: Geometry {
                    chains: vec![chain],
                    text_box: None,
                },
            });
            remove.extend(members);
        }
    }
    let mut kept: Vec<Object> = objects
        .drain(..)
        .enumerate()
        .filter(|(i, _)| !remove.contains(i))
        .map(|(_, o)| o)
        .collect();
    kept.extend(merged);
    *objects = kept;
}

/// Points of a structural line or implied span: `points` [x0,y0,x1,y1] for a line, `arc` [cx,cy,r,a0,a1] otherwise.
fn structural_chain(v: &Value) -> Result<Vec<P>> {
    if let Some(arc) = v.get("arc").and_then(Value::as_array)
        && arc.len() == 5
    {
        let a: Vec<f64> = arc
            .iter()
            .map(|x| x.as_f64().context("drafting_arc_number"))
            .collect::<Result<_>>()?;
        return Ok(sample_arc((a[0], a[1]), a[2], a[3], a[4]));
    }
    let p = numbers(v, "points")?;
    ensure!(p.len() == 4, "drafting_structural_points");
    Ok(vec![(p[0], p[1]), (p[2], p[3])])
}

// ------------------------------------------------------------------------------------------------ relations

#[derive(Debug, Clone)]
struct Relation {
    subject: usize,
    predicate: &'static str,
    object: Option<usize>,
    literal: Option<String>,
}

/// Relations between the objects of one layer from the generic geometry predicates.
fn relations(objects: &[Object]) -> Vec<Relation> {
    let mut out = Vec::new();
    let lines: Vec<usize> = (0..objects.len())
        .filter(|i| objects[*i].is_line() && !objects[*i].geometry.chains.is_empty())
        .collect();
    // Text crossed by a line: a chord of at least 0.3 text heights inside the box.
    for (ti, t) in objects.iter().enumerate() {
        let Some(tb) = (t.is_text()).then_some(t.geometry.text_box).flatten() else {
            continue;
        };
        let threshold = 0.3 * tb.height;
        for &li in &lines {
            if li == ti {
                continue;
            }
            // The baseline (shelf) of a note's own leader carries the text; it runs along the bottom of the box.
            if objects[li].expression_id.is_some()
                && objects[li].expression_id == t.expression_id
                && objects[li].geometry.chains.iter().any(|c| c.len() == 2 && tb.is_shelf(c[0], c[1]))
            {
                continue;
            }
            let crossed = objects[li].geometry.chains.iter().any(|chain| {
                chain
                    .windows(2)
                    .map(|w| tb.chord(w[0], w[1]))
                    .sum::<f64>()
                    >= threshold
            });
            if crossed {
                out.push(Relation {
                    subject: ti,
                    predicate: "crossedBy",
                    object: Some(li),
                    literal: None,
                });
            }
        }
    }
    // Lines crossed by lines in their interiors, and lines coincident with lines.
    for (n, &i) in lines.iter().enumerate() {
        for &j in &lines[n + 1..] {
            let (a, b) = (&objects[i], &objects[j]);
            if a.member_ids.iter().any(|m| b.member_ids.contains(m))
                || a.source_handles.iter().any(|h| b.source_handles.contains(h))
            {
                continue;
            }
            // Pieces of one symbol (a dimension's own lines) lie on each other by construction.
            let same_expression = a.expression_id.is_some() && a.expression_id == b.expression_id;
            let (mut crosses, mut coincident) = (false, false);
            for ca in &a.geometry.chains {
                for cb in &b.geometry.chains {
                    if !crosses && chains_cross(ca, cb) {
                        crosses = true;
                    }
                    if !coincident && !same_expression {
                        coincident = ca
                            .windows(2)
                            .any(|wa| cb.windows(2).any(|wb| segments_coincident(wa[0], wa[1], wb[0], wb[1])))
                            || (ca.len() > 2 && cb.len() == 2 && chain_coincident(ca, cb[0], cb[1]))
                            || (cb.len() > 2 && ca.len() == 2 && chain_coincident(cb, ca[0], ca[1]));
                    }
                }
            }
            if crosses {
                for (s, o) in [(i, j), (j, i)] {
                    out.push(Relation {
                        subject: s,
                        predicate: "crossedBy",
                        object: Some(o),
                        literal: None,
                    });
                }
            }
            if coincident {
                out.push(Relation {
                    subject: i,
                    predicate: "coincidentWith",
                    object: Some(j),
                    literal: None,
                });
                out.push(Relation {
                    subject: j,
                    predicate: "coincidentWith",
                    object: Some(i),
                    literal: None,
                });
            }
        }
    }
    // Break boundaries ending on an outline, or on an axis (a section of a body of revolution may be bounded by it).
    let outlines: Vec<usize> = lines
        .iter()
        .copied()
        .filter(|i| objects[*i].is("VisibleOutline") || objects[*i].is("CentreLine"))
        .collect();
    for &bi in &lines {
        if !objects[bi].is("BreakBoundary") {
            continue;
        }
        for chain in &objects[bi].geometry.chains {
            let (Some(first), Some(last)) = (chain.first(), chain.last()) else {
                continue;
            };
            for (name, point) in [("start", *first), ("end", *last)] {
                let on = outlines.iter().any(|&oi| {
                    oi != bi
                        && objects[oi].geometry.chains.iter().any(|c| {
                            c.windows(2)
                                .any(|w| point_segment(point, w[0], w[1]).0 <= END_TOLERANCE)
                        })
                });
                if on {
                    out.push(Relation {
                        subject: bi,
                        predicate: "endOnOutline",
                        object: None,
                        literal: Some(name.into()),
                    });
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ facts

fn turtle_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn node_iri(id: &str) -> String {
    let encoded: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':') {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect();
    format!("<{NS}fact:{encoded}>")
}

/// The facts of one layer as Turtle, with the typed contract rows for the report.
fn facts(layer: &str, objects: &[Object], relations: &[Relation]) -> Result<(String, usize, Vec<MdDraftingObject>, Vec<MdDraftingRelation>)> {
    let mut ttl = String::new();
    writeln!(ttl, "@prefix md: <{NS}> .\n@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n")?;
    let mut triples = 0;
    let mut rows = Vec::new();
    let mut relation_rows = Vec::new();
    let number = |x: f64| format!("\"{x}\"^^xsd:double");
    for o in objects {
        let s = node_iri(&o.id);
        let types: Vec<String> = o.classes.iter().map(|c| format!("md:{c}")).collect();
        writeln!(ttl, "{s} a {} ;", types.join(", "))?;
        triples += o.classes.len();
        let mut props: Vec<(&str, String)> = vec![
            ("layer", turtle_literal(layer)),
        ];
        for h in &o.source_handles {
            props.push(("sourceHandle", turtle_literal(h)));
        }
        for m in &o.member_ids {
            props.push(("memberId", turtle_literal(m)));
        }
        if let Some(e) = &o.expression_id {
            props.push(("expressionId", turtle_literal(e)));
        }
        if let Some(k) = o.expression_kind.as_deref().filter(|k| !k.is_empty()) {
            props.push(("expressionKind", turtle_literal(k)));
        }
        if let Some(v) = &o.view {
            props.push(("view", turtle_literal(v)));
        }
        if let Some(l) = &o.line_type {
            props.push(("lineType", turtle_literal(l)));
        }
        if let Some(t) = &o.text {
            props.push(("text", turtle_literal(t)));
        }
        if let Some(r) = o.rotation_deg {
            props.push(("rotationDeg", number(r)));
        }
        if let Some(a) = o.angle_deg {
            props.push(("angleDeg", number(a)));
        }
        let n = props.len();
        for (i, (p, v)) in props.iter().enumerate() {
            writeln!(ttl, "    md:{p} {v} {}", if i + 1 == n { "." } else { ";" })?;
        }
        triples += n;
        let row = MdDraftingObject {
            layer: layer.into(),
            id: o.id.clone(),
            class: o.classes.last().map(|c| (*c).to_owned()).unwrap_or_default(),
            classes: o.classes.iter().map(|c| (*c).to_owned()).collect(),
            source_handles: o.source_handles.clone(),
            member_ids: o.member_ids.clone(),
            expression_id: o.expression_id.clone(),
            view: o.view.clone(),
            line_type: o.line_type.clone(),
            text: o.text.clone(),
        };
        ensure!(row.validate().is_ok(), "drafting_object_contract:{}", o.id);
        rows.push(row);
    }
    for r in relations {
        let s = node_iri(&objects[r.subject].id);
        let (object, kind) = match (r.object, &r.literal) {
            (Some(o), _) => (node_iri(&objects[o].id), "node"),
            (None, Some(l)) => (turtle_literal(l), "text"),
            _ => continue,
        };
        writeln!(ttl, "{s} md:{} {object} .", r.predicate)?;
        triples += 1;
        let row = MdDraftingRelation {
            layer: layer.into(),
            subject: objects[r.subject].id.clone(),
            predicate: r.predicate.into(),
            object: match r.object {
                Some(o) => objects[o].id.clone(),
                None => r.literal.clone().unwrap_or_default(),
            },
            object_kind: kind.into(),
        };
        ensure!(row.validate().is_ok(), "drafting_relation_contract");
        relation_rows.push(row);
    }
    Ok((ttl, triples, rows, relation_rows))
}

/// Findings of the shapes on one layer's facts, through the framework's SHACL engine.
/// The relation facts of every object, as "predicate object" lines, so that a finding the engine reports without a
/// value (a count or qualified-count violation) still names what the focus is related to.
fn related_facts(objects: &[Object], relations: &[Relation]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for r in relations {
        let object = match (r.object, &r.literal) {
            (Some(o), _) => objects[o].id.clone(),
            (None, Some(l)) => l.clone(),
            _ => continue,
        };
        out.entry(objects[r.subject].id.clone())
            .or_default()
            .push(format!("{} {object}", r.predicate));
    }
    out
}

fn evaluate(
    layer: &str,
    facts: &str,
    shapes: &str,
    rules: &[Rule],
    objects_by_iri: &HashMap<String, &Object>,
    related: &HashMap<String, Vec<String>>,
) -> Result<(Vec<MdDraftingFinding>, u64)> {
    let store = semantica_runtime::graph::load(facts.as_bytes())?;
    ensure!(store.len() <= QUAD_BUDGET, "drafting_facts_budget:{layer}:{}", store.len());
    let report = semantica_runtime::shacl::validate_dataset(&store, shapes.as_bytes())?;
    let focus_count = report["focus_count"].as_u64().unwrap_or(0);
    let mut by_shape: HashMap<&str, &Rule> = HashMap::new();
    for r in rules {
        by_shape.insert(r.shape.as_str(), r);
        for p in &r.property_shapes {
            by_shape.insert(p.as_str(), r);
        }
    }
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    for f in report["findings"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let shape = f["source_shape"].as_str().unwrap_or("");
        let rule = by_shape
            .get(shape)
            .with_context(|| format!("drafting_finding_shape_unknown:{shape}"))?;
        let focus = f["focus"].as_str().unwrap_or("");
        let object = objects_by_iri
            .get(focus)
            .with_context(|| format!("drafting_finding_focus_unknown:{focus}"))?;
        let value = f["value"]
            .as_str()
            .map(|v| {
                objects_by_iri
                    .get(v)
                    .map(|o| o.id.clone())
                    .unwrap_or_else(|| v.to_owned())
            })
            .or_else(|| {
                related.get(&object.id).filter(|v| !v.is_empty()).map(|v| {
                    format!(
                        "related: {}",
                        v.iter().take(8).cloned().collect::<Vec<_>>().join(" | ")
                    )
                })
            });
        if !seen.insert((rule.id.clone(), object.id.clone(), value.clone())) {
            continue;
        }
        let message = f["message"]
            .as_str()
            .filter(|m| !m.is_empty())
            .map(str::to_owned)
            .or_else(|| rule.messages.first().cloned())
            .unwrap_or_else(|| rule.label.clone());
        findings.push(MdDraftingFinding {
            layer: layer.into(),
            rule_id: rule.id.clone(),
            focus: object.id.clone(),
            focus_classes: object.classes.iter().map(|c| (*c).to_owned()).collect(),
            source_handles: object.source_handles.clone(),
            expression_ids: object.expression_id.iter().cloned().collect(),
            value,
            message,
            severity: rule.severity.clone(),
        });
    }
    Ok((findings, focus_count))
}

/// The check: facts of the three layers, the shapes on each, one report and the facts files beside it.
pub fn check_rules(root: &SkillRoot, q: &MdDraftingRuleRequest) -> Result<MdDraftingRuleReport> {
    ensure!(
        q.validate().is_ok() && q.schema == REQUEST_SCHEMA,
        "drafting_rules_request_contract"
    );
    let k = knowledge()?;
    let rules = rules(&k.shapes)?;
    ensure!(!rules.is_empty(), "drafting_rules_empty");
    let request: Value = read_json(&root.private_path(Path::new(&q.discovery_request))?)?;
    let source = get(&request, "source")?;
    let raw = read_regular_bytes(&root.private_path(Path::new(text(source, "path")?))?, LIMIT)?;
    let output_directory = text(&request, "output_directory")?;
    let graph_path = root.private_path(&Path::new(output_directory).join("graph.json"))?;
    let graph_bytes = read_regular_bytes(&graph_path, LIMIT)?;
    let g: Value = oe_core::json::strict_json(&graph_bytes)?;
    let source_sha256 = sha256(&raw);
    ensure!(
        source_sha256 == text(&g, "source_sha256")? && source_sha256 == text(source, "sha256")?,
        "drafting_source_identity"
    );
    let records = dxf::records(&dxf::decode(&raw)?)?;
    let d = Dxf::new(&records)?;
    let output = root.private_path(Path::new(&q.output))?;
    let directory = output.parent().context("drafting_output_directory")?;
    let mut layers = Vec::new();
    let mut findings = Vec::new();
    for layer in LAYERS {
        let objs = objects(layer, &g, &d)?;
        let rels = relations(&objs);
        let (ttl, triple_count, rows, relation_rows) = facts(layer, &objs, &rels)?;
        let by_iri: HashMap<String, &Object> = objs.iter().map(|o| (node_iri(&o.id), o)).collect();
        let related = related_facts(&objs, &rels);
        let (found, focus_count) = evaluate(layer, &ttl, &k.shapes, &rules, &by_iri, &related)?;
        let facts_path = directory.join(format!("drafting-facts-{layer}.ttl"));
        oe_core::write_new_bytes(&facts_path, ttl.as_bytes())?;
        layers.push(MdDraftingLayer {
            layer: layer.into(),
            object_count: rows.len() as u64,
            relation_count: relation_rows.len() as u64,
            triple_count: triple_count as u64,
            focus_count,
            finding_count: found.len() as u64,
            facts_file: root.portable_data_path(&facts_path)?,
        });
        findings.extend(found);
    }
    let mut by_rule: BTreeMap<String, u64> = rules.iter().map(|r| (r.id.clone(), 0)).collect();
    for f in &findings {
        *by_rule.entry(f.rule_id.clone()).or_default() += 1;
    }
    let distinct: BTreeSet<&str> = rules.iter().map(|r| r.id.as_str()).collect();
    let report = MdDraftingRuleReport {
        schema: SCHEMA.into(),
        status: "checked".into(),
        graph_sha256: sha256(&graph_bytes),
        source_sha256,
        ontology_sha256: k.sha256,
        rule_count: distinct.len() as u64,
        rules: rules
            .iter()
            .map(|r| MdDraftingRule {
                id: r.id.clone(),
                group: r.group.clone(),
                concept_ids: r.concept_ids.clone(),
                clause: r.clause.clone(),
                label: r.label.clone(),
                message: r.messages.join("；"),
                severity: r.severity.clone(),
                target_classes: r.target_classes.clone(),
            })
            .collect(),
        layers,
        findings,
        by_rule,
        scope: SCOPE.into(),
    };
    ensure!(report.validate().is_ok(), "drafting_rules_report_contract");
    oe_core::write_new_bytes(&output, &serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

fn read_json(path: &Path) -> Result<Value> {
    Ok(oe_core::json::strict_json(&read_regular_bytes(
        path, LIMIT,
    )?)?)
}

/// Facts and findings of an authored DXF and graph, for tests and fixtures: no files are written.
pub fn evaluate_bytes(dxf_bytes: &[u8], graph: &Value) -> Result<BTreeMap<String, Vec<MdDraftingFinding>>> {
    let k = knowledge()?;
    let rules = rules(&k.shapes)?;
    let records = dxf::records(&dxf::decode(dxf_bytes)?)?;
    let d = Dxf::new(&records)?;
    let mut out = BTreeMap::new();
    for layer in LAYERS {
        let objs = objects(layer, graph, &d)?;
        let rels = relations(&objs);
        let (ttl, _, _, _) = facts(layer, &objs, &rels)?;
        let by_iri: HashMap<String, &Object> = objs.iter().map(|o| (node_iri(&o.id), o)).collect();
        let related = related_facts(&objs, &rels);
        let (found, _) = evaluate(layer, &ttl, &k.shapes, &rules, &by_iri, &related)?;
        out.insert(layer.to_owned(), found);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_chord_through_a_rotated_box_is_measured() {
        let tb = TextBox {
            origin: (0., 0.),
            rotation_rad: 0.,
            width: 10.,
            height: 3.5,
        };
        // The box is shrunk by a tenth of the height on every side.
        assert!((tb.chord((-5., 1.), (15., 1.)) - 9.3).abs() < 1e-9);
        assert_eq!(tb.chord((-5., 5.), (15., 5.)), 0.);
        assert_eq!(tb.chord((-5., 0.), (15., 0.)), 0., "a line along the edge is not a crossing");
        let vertical = TextBox {
            origin: (10., 0.),
            rotation_rad: PI / 2.,
            width: 10.,
            height: 3.5,
        };
        assert!((vertical.chord((0., 5.), (20., 5.)) - 2.8).abs() < 1e-9);
    }
    #[test]
    fn formatting_codes_are_removed_and_width_estimated() {
        let (t, w, stacked) = clean_text("\\A0;%%c57{\\H0.625000x;\\S+0.4^ +0.2;}");
        assert_eq!(t, "Ø57+0.4/ +0.2");
        assert!(w > 2.0 && w < 6.0, "{w}");
        assert!(stacked);
        assert_eq!(clean_text("30").0, "30");
        assert!(!clean_text("30").2);
        assert_eq!(clean_text("%%D").0, "°");
        // Font stacking marks: the tolerance parts share the width of the wider one at reduced size.
        let (t, w, stacked) = clean_text("%%C19\u{E1D6}+0.05\u{E1DA}\u{E1D7}+0.02\u{E1D9}");
        assert_eq!(t, "Ø19+0.05/+0.02");
        assert!(stacked);
        let (_, flat, _) = clean_text("%%C19+0.05+0.02");
        assert!(w < 0.6 * flat, "{w} versus flat {flat}");
    }
    #[test]
    fn crossing_and_coincidence_are_interior_relations() {
        assert!(chains_cross(&[(0., 0.), (10., 0.)], &[(5., -5.), (5., 5.)]));
        assert!(!chains_cross(&[(0., 0.), (10., 0.)], &[(10., -5.), (10., 5.)]));
        assert!(!chains_cross(&[(0., 0.), (10., 0.)], &[(5., 0.), (5., 5.)]));
        assert!(chains_cross(&[(0., 0.), (5., 1.), (10., 0.)], &[(5., -5.), (5., 5.)]));
        // A line ending on a wavy curve whose waves run back across the line's end is a T-junction.
        let wavy = [(0., 0.), (2., 1.), (4., -1.), (6., 1.), (8., -1.), (10., 0.)];
        assert!(!chains_cross(&wavy, &[(5., 0.), (5., 20.)]));
        assert!(chains_cross(&wavy, &[(5., -20.), (5., 20.)]));
        let tb = TextBox {
            origin: (0., 0.),
            rotation_rad: 0.,
            width: 10.,
            height: 3.5,
        };
        assert!(tb.is_shelf((-2., 0.2), (12., 0.2)));
        assert!(!tb.is_shelf((-2., 1.8), (12., 1.8)));
        assert!(segments_coincident((0., 0.), (10., 0.), (5., 0.05), (20., 0.05)));
        assert!(!segments_coincident((0., 0.), (10., 0.), (5., 0.5), (20., 0.5)));
        assert_eq!(angle_deg((0., 0.), (1., 1.)), 45.);
        assert_eq!(angle_deg((0., 0.), (-1., 1.)), 135.);
    }
}
