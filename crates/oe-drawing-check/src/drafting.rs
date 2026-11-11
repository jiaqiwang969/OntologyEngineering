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

#[path = "drafting_constituents.rs"]
mod constituents;
#[path = "drafting_cutting.rs"]
mod cutting;

/// Source-bound named cutting positions, shared by the rule facts and correction planner.
pub fn cutting_sites(bytes: &[u8], graph: &Value) -> Result<Vec<Value>> { cutting::values(bytes, graph) }

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
#[path = "drafting_sections.rs"]
mod sections;

const SCOPE: &str = "Drafting rules of the ontology evaluated on facts exported from the source bytes and the frozen graph; a finding is a deviation to review, not a correction and not engineering acceptance.";

/// Every class of the fact vocabulary; the ontology and the spec declare the same set (tested).
pub const CLASSES: [&str; 41] = [
    "Line",
    "VisibleOutline",
    "HiddenOutline",
    "CentreLine",
    "BreakBoundary",
    "WavyBreakBoundary",
    "HatchLine",
    "SectionRegion",
    "DimensionLine",
    "LinearDimensionLine",
    "RadiusDimensionLine",
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
    "LinearDimensionText",
    "AngularDimensionText",
    "ToleranceText",
    "GeometricToleranceValue",
    "RoughnessValue",
    "NoteText",
    "TableText",
    "ScaleText",
    "ViewLabel",
    "ViewTitle",
    "DetailViewTitle",
    "CalloutLetter",
    "DatumLetter",
    "DatumIdentifierLetter",
    "SymbolText",
    "Construction",
    "SectionBoundarySpan",
    "SectionInteriorUse",
    "CuttingLetterSite",
];
/// Every predicate of the fact vocabulary (data and object properties); same set in the ontology and the spec.
pub const PREDICATES: [&str; 44] = [
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
    "heightUnits",
    "spacingUnits",
    "dxfLayer",
    "colourIndex",
    "layerColourIndex",
    "endOnOutline",
    "overshootUnits",
    "extensionBeyondUnits",
    "angleToDimensionLineDeg",
    "extentUnits",
    "crossedBy",
    "coincidentWith",
    "radialTo",
    "onExtensionOf",
    "alignedWith",
    "sameLetterAs",
    "scaledBy",
    "hatchDirectionDiffersFrom",
    "hatchSpacingDiffersFrom",
    "chainedWith",
    "terminatorOnCarrier",
    "boundedContinuationOf",
    "sectionRegion",
    "sourceLine",
    "boundaryCarrier",
    "sectionOrigin",
    "boundaryIntervalStart",
    "boundaryIntervalEnd",
    "boundaryParameter",
    "crossSectionType",
    "sectionExtent",
    "parentOutlineContinuation",
    "sectionGeometry",
    "cutLetterCount",
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
    pub confidence: String,
    pub proposed_correction: String,
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
            let mut values: Vec<String> = triples
                .iter()
                .filter(|(k, _)| k == p)
                .map(|(_, o)| text(o))
                .collect();
            // RDF properties are unordered; report metadata uses lexical order, not store iteration order.
            values.sort();
            values
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
            .filter(|(_, o)| matches!(o, Term::BlankNode(_) | Term::NamedNode(_)))
            .map(|(_, o)| o.to_string())
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
        // Preserve legacy child-message aggregation; root-only shapes use their authored message.
        if messages.is_empty() {
            messages = values(&format!("{SH}message"));
        }
        let target_classes: Vec<String> = values(&format!("{SH}targetClass"))
            .into_iter()
            .filter_map(|c| c.strip_prefix(NS).map(str::to_owned))
            .collect();
        referenced.extend(target_classes.iter().cloned());
        let severity = value(&format!("{SH}severity"))
            .and_then(|s| s.strip_prefix(SH).map(str::to_lowercase))
            .unwrap_or_else(|| "violation".into());
        // Preserve every authored clause (including repeats), fixing only their presentation order.
        messages.sort();
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
            confidence: value(&format!("{NS}confidence")).unwrap_or_else(|| "needs_expert".into()),
            proposed_correction: value(&format!("{NS}proposedCorrection")).unwrap_or_else(|| "请专家确认".into()),
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
type CurveChains = (Vec<Vec<P>>, Vec<(P, f64)>);
type StructuralChain = (Vec<P>, Vec<(P, f64)>);
type TextBoxContent = (TextBox, Option<TextBox>, String, f64);
const EPS_COINCIDENT: f64 = 0.15;
const MIN_OVERLAP: f64 = 1.0;
const END_TOLERANCE: f64 = 0.5;
/// A straight line that ends on a wavy curve meets it inside the wave's band: pieces of the curve up to this far from
/// the line's end may still intersect it without the line crossing the curve.
const WAVE_BAND: f64 = 2.0;
/// Stacked tolerances are drawn one above the other at reduced size, so the stacked part of a text is this many
/// heights tall; it starts 0.15 heights below the baseline of the main text.
const STACK_HEIGHT: f64 = 1.3;
const STACK_DROP: f64 = 0.15;
/// Two directions are parallel within this angle (alignment of a text with its line, collinearity, chaining).
const PARALLEL_DEG: f64 = 1.5;
/// A line lies on the extension of another when collinear and separated along the line by at most this gap.
const EXTENSION_GAP: f64 = 10.0;
/// A radial dimension line passes an arc's centre within this distance and ends on the arc within ARROW_REACH.
const RADIAL_EPS: f64 = 0.3;
/// Native dimension lines stop where their arrowheads begin and a small radius's line stops outside the arc behind
/// its arrow: a line reaches another within this distance (two arrow lengths at scale 1).
const ARROW_REACH: f64 = 6.0;
/// Two hatch regions differ when their directions differ by more than this angle or their spacings by this ratio.
const HATCH_ANGLE_DEG: f64 = 3.0;
const HATCH_SPACING_RATIO: f64 = 1.3;

/// Geometry of one drawing object: straight pieces (chains of points), the arcs among them and, for text, boxes.
#[derive(Debug, Clone, Default)]
struct Geometry {
    /// Polylines: a LINE is one two-point chain; arcs, circles and polyline bulges are sampled; splines use their points.
    chains: Vec<Vec<P>>,
    /// Centre and radius of every arc or circle the object draws (ARC, CIRCLE, polyline bulge segments).
    arcs: Vec<(P, f64)>,
    /// The main text box: anchor of the lower-left corner in drawing coordinates, rotation, width and height.
    text_box: Option<TextBox>,
    /// The box of a stacked part (tolerance stack), drawn after the main text, taller than it.
    stack_box: Option<TextBox>,
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
    (inside(t) && inside(u)).then_some((a.0 + t * r.0, a.1 + t * r.1))
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
/// Smallest angle between two directions given in degrees in [0, 180).
fn angle_between(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 180.;
    d.min(180. - d)
}
/// Perpendicular distance from a point to the infinite line through a and b, and the parameter along a→b.
fn point_line(p: P, a: P, b: P) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    if l2 < 1e-18 {
        return (dist(p, a), 0.);
    }
    let t = ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2;
    let off = ((p.0 - a.0) * -dy + (p.1 - a.1) * dx).abs() / l2.sqrt();
    (off, t)
}
/// Parameter along the segment a→b where the infinite line through c and d meets it; none when parallel.
fn line_param(a: P, b: P, c: P, d: P) -> Option<f64> {
    let (r, s) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
    let denominator = r.0 * s.1 - r.1 * s.0;
    if denominator.abs() < 1e-12 {
        return None;
    }
    let qp = (c.0 - a.0, c.1 - a.1);
    Some((qp.0 * s.1 - qp.1 * s.0) / denominator)
}
/// Two straight segments on one line (parallel within PARALLEL_DEG, offset within EPS_COINCIDENT): the signed gap
/// between them along the line, negative when they overlap.
fn collinear_gap(a: P, b: P, c: P, d: P) -> Option<f64> {
    let (la, lc) = (dist(a, b), dist(c, d));
    if la < 1e-9 || lc < 1e-9 || angle_between(angle_deg(a, b), angle_deg(c, d)) > PARALLEL_DEG {
        return None;
    }
    let ua = ((b.0 - a.0) / la, (b.1 - a.1) / la);
    let offset = |p: P| ((p.0 - a.0) * -ua.1 + (p.1 - a.1) * ua.0).abs();
    if offset(c) > EPS_COINCIDENT || offset(d) > EPS_COINCIDENT {
        return None;
    }
    let along = |p: P| (p.0 - a.0) * ua.0 + (p.1 - a.1) * ua.1;
    let (c0, c1) = (along(c).min(along(d)), along(c).max(along(d)));
    Some(-(c1.min(la) - c0.max(0.)))
}
/// The segment lengthened by `by` at both ends.
fn stretch(a: P, b: P, by: f64) -> (P, P) {
    let l = dist(a, b).max(1e-9);
    let u = ((b.0 - a.0) / l, (b.1 - a.1) / l);
    ((a.0 - u.0 * by, a.1 - u.1 * by), (b.0 + u.0 * by, b.1 + u.1 * by))
}
fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(values[values.len() / 2])
}
/// The arc of a polyline bulge between two vertices: sampled points and (centre, radius).
fn bulge_arc(p0: P, p1: P, bulge: f64) -> Option<(Vec<P>, (P, f64))> {
    let d = dist(p0, p1);
    if bulge.abs() < 1e-9 || d < 1e-9 {
        return None;
    }
    let theta = 4. * bulge.atan();
    let r = d / (2. * (theta / 2.).sin().abs());
    let m = ((p0.0 + p1.0) / 2., (p0.1 + p1.1) / 2.);
    let h = r * (theta / 2.).cos();
    let n = (-(p1.1 - p0.1) / d, (p1.0 - p0.0) / d);
    let sign = if bulge > 0. { 1. } else { -1. };
    let c = (m.0 + sign * h * n.0, m.1 + sign * h * n.1);
    let a0 = (p0.1 - c.1).atan2(p0.0 - c.0);
    let a1 = (p1.1 - c.1).atan2(p1.0 - c.0);
    let points = if bulge > 0. {
        sample_arc(c, r, a0, a1)
    } else {
        let mut p = sample_arc(c, r, a1, a0);
        p.reverse();
        p
    };
    Some((points, (c, r)))
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
    /// Colour index of each layer (LAYER group 62), the colour of entities drawn BYLAYER.
    layer_colour: HashMap<&'a str, i64>,
}
impl<'a> Dxf<'a> {
    fn new(records: &'a [dxf::Record]) -> Result<Self> {
        let mut d = Dxf {
            by_handle: HashMap::new(),
            blocks: HashMap::new(),
            vertices: HashMap::new(),
            dim_text_height: HashMap::new(),
            style_width: HashMap::new(),
            layer_colour: HashMap::new(),
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
                "LAYER" => {
                    if let (Some(name), Some(c)) = (r.tag(2), r.tag(62))
                        && let Ok(c) = c.trim().parse::<i64>()
                    {
                        d.layer_colour.insert(name, c.abs());
                    }
                }
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

    /// Straight chains of a curve record, in drawing coordinates plus an offset, and the arcs among them.
    fn chains(&self, r: &dxf::Record, offset: P) -> Result<CurveChains> {
        let pt = |x: i64, y: i64| -> Result<P> { Ok((r.float(x)? + offset.0, r.float(y)? + offset.1)) };
        let mut arcs = Vec::new();
        let chains = match r.kind.as_str() {
            "LINE" => vec![vec![pt(10, 20)?, pt(11, 21)?]],
            "ARC" => {
                let (c, radius) = (pt(10, 20)?, r.float(40)?);
                arcs.push((c, radius));
                vec![sample_arc(
                    c,
                    radius,
                    r.float(50)?.to_radians(),
                    r.float(51)?.to_radians(),
                )]
            }
            "CIRCLE" => {
                let (c, radius) = (pt(10, 20)?, r.float(40)?);
                arcs.push((c, radius));
                vec![sample_arc(c, radius, 0., 2. * PI)]
            }
            "LWPOLYLINE" => {
                // Vertices with their bulges (group 42 follows the vertex it belongs to).
                let mut vertices: Vec<(P, f64)> = Vec::new();
                let mut x = None;
                for (code, value) in &r.tags {
                    match code {
                        10 => x = Some(value.parse::<f64>().context("dxf_vertex_x")?),
                        20 => {
                            if let Some(px) = x.take() {
                                vertices.push(((px + offset.0, value.parse::<f64>().context("dxf_vertex_y")? + offset.1), 0.));
                            }
                        }
                        42 => {
                            if let Some(last) = vertices.last_mut() {
                                last.1 = value.parse::<f64>().unwrap_or(0.);
                            }
                        }
                        _ => {}
                    }
                }
                if r.tag(70).and_then(|f| f.parse::<i64>().ok()).unwrap_or(0) & 1 == 1
                    && vertices.len() > 2
                {
                    vertices.push((vertices[0].0, 0.));
                }
                let mut pts: Vec<P> = Vec::new();
                for w in vertices.windows(2) {
                    let ((p0, bulge), (p1, _)) = (w[0], w[1]);
                    if pts.is_empty() {
                        pts.push(p0);
                    }
                    match bulge_arc(p0, p1, bulge) {
                        Some((arc_points, arc)) => {
                            arcs.push(arc);
                            pts.extend(arc_points.into_iter().skip(1));
                        }
                        None => pts.push(p1),
                    }
                }
                if pts.is_empty() && vertices.len() == 1 {
                    pts.push(vertices[0].0);
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
        };
        Ok((chains, arcs))
    }

    /// Text boxes of a TEXT, MTEXT or ATTRIB record (main text and stacked part); the cleaned text and rotation.
    fn text_box(&self, r: &dxf::Record, offset: P) -> Result<Option<TextBoxContent>> {
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
        let ct = clean_text(&raw);
        let (cleaned, width_units, stacked) = (ct.text, ct.width, ct.stacked);
        let style_width = d_style_width(self, r);
        let width_factor = r
            .float(41)
            .ok()
            .filter(|w| *w > 0. && r.kind != "MTEXT")
            .unwrap_or(style_width);
        let width = (width_units * height * width_factor).max(0.5 * height);
        let main_width = if stacked {
            (ct.main_width * height * width_factor).clamp(0.5 * height, width)
        } else {
            width
        };
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
        // Lower-left corner: move back from the anchor along the text and up directions.
        let (bx, by) = (-ax * width, -ay * height);
        let origin = (anchor.0 + bx * c - by * s, anchor.1 + bx * s + by * c);
        // A stacked tolerance follows the main text and rises above and below it: its own box, STACK_HEIGHT tall,
        // begins where the main text ends and STACK_DROP heights below the baseline.
        let stack = (stacked && width - main_width > 1e-9).then(|| {
            let (sx, sy) = (main_width, -STACK_DROP * height);
            TextBox {
                origin: (origin.0 + sx * c - sy * s, origin.1 + sx * s + sy * c),
                rotation_rad: rad,
                width: width - main_width,
                height: STACK_HEIGHT * height,
            }
        });
        let mut normalized = rotation % 180.;
        if normalized < 0. {
            normalized += 180.;
        }
        Ok(Some((
            TextBox {
                origin,
                rotation_rad: rad,
                width: main_width,
                height,
            },
            stack,
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

/// A text string read from its record: the text without formatting codes, its width in text heights (stacked parts
/// at their scale), whether it holds a stacked part and the width of the main text before the first stack.
#[derive(Debug, Clone, PartialEq)]
struct CleanText {
    text: String,
    width: f64,
    stacked: bool,
    main_width: f64,
}

/// Text without MTEXT formatting codes, its width in text heights (stacked parts at their scale), and whether it
/// holds a stacked part. Besides MTEXT `\S` stacks, TEXT strings of Chinese CAD fonts stack a tolerance with the
/// private-use marks U+E1D6 (upper part begins), U+E1DA (upper ends), U+E1D7 (lower begins), U+E1D9 (lower ends):
/// the two parts are drawn one above the other at reduced size, so together they are as wide as the wider one.
fn clean_text(raw: &str) -> CleanText {
    let mut out = String::new();
    let mut width = 0f64;
    let mut scale = 1f64;
    let mut stacked = false;
    let mut first_stack: Option<f64> = None;
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
            first_stack.get_or_insert(width);
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
                    first_stack.get_or_insert(width);
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
    CleanText {
        text: out.trim().to_owned(),
        width,
        stacked,
        main_width: first_stack.unwrap_or(width),
    }
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
    /// Height of a text (main text, drawing units).
    height: Option<f64>,
    /// Spacing of a hatch (pattern offset or the median gap of loose hatch lines).
    spacing: Option<f64>,
    /// Where the source record sits: its DXF layer, its colour index (own or by layer) and its layer's colour.
    dxf_layer: Option<String>,
    colour: Option<i64>,
    layer_colour: Option<i64>,
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
    /// The one straight segment of a line drawn as a single segment.
    fn segment(&self) -> Option<(P, P)> {
        match self.geometry.chains.as_slice() {
            [c] if c.len() == 2 => Some((c[0], c[1])),
            _ => None,
        }
    }
    /// Both text boxes (main text and stack), as drawn.
    fn boxes(&self) -> impl Iterator<Item = &TextBox> {
        self.geometry.text_box.iter().chain(self.geometry.stack_box.iter())
    }
}

/// What the graph says about the member's surroundings, which decides a few classes: whether its expression
/// also holds a view title (a letter beside a title is part of the title, a letter alone is a callout) and whether
/// the member labels a detail view.
#[derive(Clone, Copy, Default)]
struct Setting {
    title_in_expression: bool,
    labels_detail_view: bool,
}

/// The class of a member from its role, the source kind, the expression kind and its setting.
fn classes_of(role: &str, line_type: &str, source_kind: &str, expression_kind: &str, setting: Setting) -> Vec<&'static str> {
    let text = |extra: &'static str| vec!["Text", extra];
    let line = |extra: &'static str| vec!["Line", extra];
    let title = || {
        let mut v = vec!["Text", "ViewLabel", "ViewTitle"];
        if setting.labels_detail_view {
            v.push("DetailViewTitle");
        }
        v
    };
    match role {
        "dimension_text" => match expression_kind {
            "dimension_angle" => vec!["Text", "DimensionText", "AngularDimensionText"],
            "dimension_linear" => vec!["Text", "DimensionText", "LinearDimensionText"],
            _ => text("DimensionText"),
        },
        "tolerance_text" => vec!["Text", "ToleranceText", "DimensionText"],
        "tolerance_value" => vec!["Text", "ToleranceText", "DimensionText", "GeometricToleranceValue"],
        "roughness_value" => vec!["Text", "ToleranceText", "DimensionText", "RoughnessValue"],
        "note_text" => text("NoteText"),
        "table_text" => text("TableText"),
        "scale_text" => text("ScaleText"),
        "view_title" => title(),
        "view_letter" => {
            if setting.title_in_expression {
                title()
            } else {
                vec!["Text", "ViewLabel", "CalloutLetter"]
            }
        }
        "datum_letter" => vec!["Text", "DatumLetter", "DatumIdentifierLetter"],
        "datum_reference" => text("DatumLetter"),
        "dimension_line" => match expression_kind {
            "dimension_radius" => vec!["Line", "DimensionLine", "RadiusDimensionLine"],
            "dimension_linear" => vec!["Line", "DimensionLine", "LinearDimensionLine"],
            _ => line("DimensionLine"),
        },
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
                text("SymbolText")
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
    // axis as a boundary) keeps the class of what it is. Roles per expression decide whether a view letter stands
    // beside a title.
    let mut roles_by_handle: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    let mut roles_by_expression: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for m in list(g, "members")? {
        roles_by_handle
            .entry(text(m, "source_handle")?)
            .or_default()
            .insert(text(m, "member_role")?);
        roles_by_expression
            .entry(text(m, "expression_id")?)
            .or_default()
            .insert(text(m, "member_role")?);
    }
    // Detail views of the graph: their ids and the handles of the texts that label them.
    let mut detail_views: BTreeSet<&str> = BTreeSet::new();
    let mut detail_labels: BTreeSet<&str> = BTreeSet::new();
    for v in g.get("views").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        if v.get("kind").and_then(Value::as_str) == Some("detail_view") {
            if let Some(id) = v.get("id").and_then(Value::as_str) {
                detail_views.insert(id);
            }
            for h in v.get("label_source_ids").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
                if let Some(h) = h.as_str() {
                    detail_labels.insert(h);
                }
            }
        }
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
        let view_id = m.get("view_id").and_then(Value::as_str).unwrap_or("");
        let setting = Setting {
            title_in_expression: roles_by_expression
                .get(expression_id)
                .is_some_and(|r| r.contains("view_title")),
            labels_detail_view: detail_labels.contains(root) || detail_views.contains(view_id),
        };
        let mut classes = classes_of(role, line_type, &record.kind, expression_kind, setting);
        if classes == ["Construction"] && record.kind == "DIMENSION" {
            continue;
        }
        // Provenance of the record: layer and colour, own or by layer (BYBLOCK pieces take their container's).
        let container = if handle != root { d.by_handle.get(root).copied() } else { None };
        let layer_of = |r: &dxf::Record| r.tag(8).map(str::to_owned);
        let own_colour = |r: &dxf::Record| r.tag(62).and_then(|c| c.trim().parse::<i64>().ok()).filter(|c| *c > 0);
        let dxf_layer = own_colour(record)
            .map(|_| layer_of(record))
            .unwrap_or_else(|| container.and_then(layer_of).or_else(|| layer_of(record)));
        let layer_colour = dxf_layer
            .as_deref()
            .and_then(|l| d.layer_colour.get(l).copied());
        let colour = own_colour(record)
            .or_else(|| container.and_then(own_colour))
            .or(layer_colour);
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
        let mut height = None;
        let mut spacing = None;
        match record.kind.as_str() {
            "TEXT" | "MTEXT" | "ATTRIB" => {
                if let Some((tb, stack, cleaned, rot)) = d.text_box(record, offset)? {
                    height = Some(tb.height);
                    geometry.text_box = Some(tb);
                    geometry.stack_box = stack;
                    text_value = Some(cleaned);
                    rotation = Some(rot);
                }
            }
            "HATCH" => {
                // The pattern line (group 53) is the direction actually drawn; group 52 is the pattern's rotation.
                let line_angle = record.float(53).or_else(|_| record.float(52)).unwrap_or(0.);
                angle = Some(line_angle.rem_euclid(180.));
                if let (Ok(dx), Ok(dy)) = (record.float(45), record.float(46)) {
                    let t = line_angle.to_radians();
                    let s = (-t.sin() * dx + t.cos() * dy).abs();
                    if s > 1e-6 {
                        spacing = Some((s * 1000.).round() / 1000.);
                    }
                }
            }
            "DIMENSION" if classes.contains(&"Text") => {
                // A dimension text without a display-block record: the midpoint and the style height.
                let style = record.tag_or(3, "STANDARD");
                let h = d.dim_text_height.get(style).copied().unwrap_or(2.5);
                let ct = clean_text(record.tag_or(1, ""));
                if let (Ok(x), Ok(y)) = (record.float(11), record.float(21)) {
                    let width = (ct.width * h).max(0.5 * h);
                    let main_width = if ct.stacked { (ct.main_width * h).clamp(0.5 * h, width) } else { width };
                    let origin = (x - width / 2., y - h / 2.);
                    geometry.text_box = Some(TextBox {
                        origin,
                        rotation_rad: 0.,
                        width: main_width,
                        height: h,
                    });
                    if ct.stacked && width - main_width > 1e-9 {
                        geometry.stack_box = Some(TextBox {
                            origin: (origin.0 + main_width, origin.1 - STACK_DROP * h),
                            rotation_rad: 0.,
                            width: width - main_width,
                            height: STACK_HEIGHT * h,
                        });
                    }
                    height = Some(h);
                    text_value = Some(ct.text);
                    rotation = Some(0.);
                }
            }
            _ => {
                let (mut chains, arcs) = d.chains(record, offset)?;
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
                // Every straight line carries its direction; a hatch line's direction is its pattern angle.
                if classes.contains(&"Line")
                    && chains.len() == 1
                    && let Some(c) = chains.first()
                    && c.len() == 2
                    && dist(c[0], c[1]) > 1e-9
                {
                    angle = Some(angle_deg(c[0], c[1]));
                }
                geometry.chains = chains;
                geometry.arcs = arcs;
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
            height,
            spacing,
            dxf_layer,
            colour,
            layer_colour,
            geometry,
        });
    }
    hatch_regions(layer, &mut out);
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
            let (chain, arcs) = structural_chain(line)?;
            let angle = (chain.len() == 2 && dist(chain[0], chain[1]) > 1e-9).then(|| angle_deg(chain[0], chain[1]));
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
                angle_deg: angle,
                height: None,
                spacing: None,
                dxf_layer: None,
                colour: None,
                layer_colour: None,
                geometry: Geometry {
                    chains: vec![chain],
                    arcs,
                    ..Geometry::default()
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
            let (chain, arcs) = structural_chain(span)?;
            let angle = (chain.len() == 2 && dist(chain[0], chain[1]) > 1e-9).then(|| angle_deg(chain[0], chain[1]));
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
                angle_deg: angle,
                height: None,
                spacing: None,
                dxf_layer: None,
                colour: None,
                layer_colour: None,
                geometry: Geometry {
                    chains: vec![chain],
                    arcs,
                    ..Geometry::default()
                },
            });
        }
    }
    Ok(out)
}

/// Hatching drawn as loose parallel lines is one section region per expression: an object with the lines'
/// median direction and the median gap between neighbouring lines, so that regions drawn as HATCH entities and
/// regions drawn line by line are compared alike.
fn hatch_regions(layer: &str, objects: &mut Vec<Object>) {
    let mut by_expression: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, o) in objects.iter().enumerate() {
        if o.is("HatchLine")
            && o.segment().is_some()
            && let Some(e) = &o.expression_id
        {
            by_expression.entry(e.clone()).or_default().push(i);
        }
    }
    let mut regions = Vec::new();
    for (expression, indices) in by_expression {
        if indices.len() < 2 {
            continue;
        }
        let mut angles: Vec<f64> = indices.iter().filter_map(|i| objects[*i].angle_deg).collect();
        let Some(angle) = median(&mut angles) else {
            continue;
        };
        let t = angle.to_radians();
        let normal = (-t.sin(), t.cos());
        let mut offsets: Vec<f64> = indices
            .iter()
            .filter_map(|i| objects[*i].segment())
            .map(|(a, b)| ((a.0 + b.0) / 2.) * normal.0 + ((a.1 + b.1) / 2.) * normal.1)
            .collect();
        offsets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut gaps: Vec<f64> = offsets.windows(2).map(|w| w[1] - w[0]).filter(|g| *g > 1e-6).collect();
        let spacing = median(&mut gaps).map(|s| (s * 1000.).round() / 1000.);
        let first = &objects[indices[0]];
        let mut handles: Vec<String> = indices.iter().flat_map(|i| objects[*i].source_handles.clone()).collect();
        handles.sort();
        handles.dedup();
        regions.push(Object {
            id: format!("{layer}:hatch:{expression}"),
            classes: vec!["SectionRegion"],
            source_handles: handles,
            member_ids: indices.iter().flat_map(|i| objects[*i].member_ids.clone()).collect(),
            expression_id: Some(expression),
            expression_kind: first.expression_kind.clone(),
            view: first.view.clone(),
            line_type: None,
            text: None,
            rotation_deg: None,
            angle_deg: Some(angle),
            height: None,
            spacing,
            dxf_layer: first.dxf_layer.clone(),
            colour: first.colour,
            layer_colour: first.layer_colour,
            geometry: Geometry::default(),
        });
    }
    objects.extend(regions);
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
                height: None,
                spacing: None,
                dxf_layer: first.dxf_layer.clone(),
                colour: first.colour,
                layer_colour: first.layer_colour,
                geometry: Geometry {
                    chains: vec![chain],
                    ..Geometry::default()
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

/// Points of a structural line or implied span: `points` [x0,y0,x1,y1] for a line, `arc` [cx,cy,r,a0,a1] otherwise;
/// with the arc's centre and radius when it is one.
fn structural_chain(v: &Value) -> Result<StructuralChain> {
    if let Some(arc) = v.get("arc").and_then(Value::as_array)
        && arc.len() == 5
    {
        let a: Vec<f64> = arc
            .iter()
            .map(|x| x.as_f64().context("drafting_arc_number"))
            .collect::<Result<_>>()?;
        return Ok((sample_arc((a[0], a[1]), a[2], a[3], a[4]), vec![((a[0], a[1]), a[2])]));
    }
    let p = numbers(v, "points")?;
    ensure!(p.len() == 4, "drafting_structural_points");
    Ok((vec![(p[0], p[1]), (p[2], p[3])], Vec::new()))
}

/// Pieces of one expression with the given class that lie on one line with piece `i` (the coincidence tolerances),
/// `i` among them, and the extreme points of all of them along that line: a dimension line broken at its text, an
/// extension line or centre line drawn in several collinear pieces is one line for the measures taken on it.
fn collinear_pieces(objects: &[Object], i: usize, class: &str) -> (Vec<usize>, (P, P)) {
    let (a, b) = objects[i].segment().expect("a straight piece");
    let mut group = vec![i];
    // A lent outline or axis keeps its own extent: it is not a piece of the extension line it serves.
    let lent = |o: &Object| class == "ExtensionLine" && (o.is("VisibleOutline") || o.is("CentreLine"));
    if objects[i].expression_id.is_some() && !lent(&objects[i]) {
        for (j, o) in objects.iter().enumerate() {
            if j != i
                && o.is(class)
                && !lent(o)
                && o.expression_id == objects[i].expression_id
                && let Some((c, d)) = o.segment()
                && collinear_gap(a, b, c, d).is_some()
            {
                group.push(j);
            }
        }
    }
    let l = dist(a, b).max(1e-9);
    let u = ((b.0 - a.0) / l, (b.1 - a.1) / l);
    let along = |p: P| (p.0 - a.0) * u.0 + (p.1 - a.1) * u.1;
    let (mut lo, mut hi) = (0f64, l);
    for &j in &group {
        if let Some((c, d)) = objects[j].segment() {
            for p in [c, d] {
                lo = lo.min(along(p));
                hi = hi.max(along(p));
            }
        }
    }
    (group, ((a.0 + u.0 * lo, a.1 + u.1 * lo), (a.0 + u.0 * hi, a.1 + u.1 * hi)))
}
/// The piece of a group whose end lies nearest to the point (an end of the group's line).
fn piece_at(objects: &[Object], group: &[usize], p: P) -> usize {
    let end_distance = |i: usize| objects[i].segment().map(|(a, b)| dist(a, p).min(dist(b, p))).unwrap_or(f64::MAX);
    *group
        .iter()
        .min_by(|x, y| end_distance(**x).partial_cmp(&end_distance(**y)).unwrap_or(std::cmp::Ordering::Equal))
        .expect("a non-empty group")
}

// ------------------------------------------------------------------------------------------------ relations

#[derive(Debug, Clone)]
struct Relation {
    subject: usize,
    predicate: &'static str,
    object: Option<usize>,
    literal: Option<String>,
    number: Option<f64>,
}
impl Relation {
    fn node(subject: usize, predicate: &'static str, object: usize) -> Self {
        Relation { subject, predicate, object: Some(object), literal: None, number: None }
    }
    fn number(subject: usize, predicate: &'static str, value: f64) -> Self {
        Relation { subject, predicate, object: None, literal: None, number: Some((value * 1000.).round() / 1000.) }
    }
}

/// The letter a view title or callout carries: the leading run of capital Latin letters (Roman numerals among
/// them) once direction words, dashes and a scale are set aside; "A—A" and "A向" give "A", "Ⅰ 2:1" gives "Ⅰ".
fn view_letter(text: &str) -> String {
    let mut letters = String::new();
    for c in text.chars() {
        if c.is_ascii_uppercase() || ('\u{2160}'..='\u{216F}').contains(&c) {
            letters.push(c);
        } else if c.is_whitespace() && letters.is_empty() {
            continue;
        } else {
            break;
        }
    }
    letters
}

/// Relations between the objects of one layer from the generic geometry predicates.
fn relations(objects: &[Object]) -> Vec<Relation> {
    let mut out = Vec::new();
    let lines: Vec<usize> = (0..objects.len())
        .filter(|i| objects[*i].is_line() && !objects[*i].geometry.chains.is_empty())
        .collect();
    // Text crossed by a line: a chord of at least 0.3 text heights inside the boxes (main text and stack).
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
                    .map(|w| t.boxes().map(|b| b.chord(w[0], w[1])).sum::<f64>())
                    .sum::<f64>()
                    >= threshold
            });
            if crossed {
                out.push(Relation::node(ti, "crossedBy", li));
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
                    out.push(Relation::node(s, "crossedBy", o));
                }
            }
            if coincident {
                out.push(Relation::node(i, "coincidentWith", j));
                out.push(Relation::node(j, "coincidentWith", i));
            }
            // A dimension line on the extension of a visible outline: collinear, not overlapping, close along the line.
            if !same_expression
                && let (Some((a0, a1)), Some((b0, b1))) = (a.segment(), b.segment())
                && let Some(gap) = collinear_gap(a0, a1, b0, b1)
                && gap > 0.
                && gap <= EXTENSION_GAP
            {
                for (s, o) in [(i, j), (j, i)] {
                    if objects[s].is("DimensionLine") && objects[o].is("VisibleOutline") {
                        out.push(Relation::node(s, "onExtensionOf", o));
                    }
                }
            }
        }
    }
    let straight = |i: usize, class: &str| objects[i].is(class) && objects[i].segment().is_some();
    let dimension_segments: Vec<(usize, P, P)> = lines
        .iter()
        .filter(|i| straight(**i, "DimensionLine"))
        .map(|i| {
            let (c, d) = objects[*i].segment().expect("straight");
            (*i, c, d)
        })
        .collect();
    let extension_segments: Vec<(usize, P, P)> = lines
        .iter()
        .filter(|i| straight(**i, "ExtensionLine"))
        .map(|i| {
            let (c, d) = objects[*i].segment().expect("straight");
            (*i, c, d)
        })
        .collect();
    // Where the infinite line through c and d meets the axis a→b: the parameter along the axis, when the meeting
    // point lies on the segment c–d lengthened by `reach`.
    let crossing = |a: P, b: P, c: P, d: P, reach: f64| -> Option<f64> {
        let t = line_param(a, b, c, d)?;
        let p = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        let (c2, d2) = stretch(c, d, reach);
        (point_segment(p, c2, d2).0 <= EPS_COINCIDENT).then_some(t)
    };
    // Extension lines (all collinear pieces of one expression as one line) against the dimension lines: the angle
    // to the expression's own dimension line, and how far the line runs past the outermost dimension line it meets
    // (negative when it stops short of its own dimension line). The measures sit on the piece holding that end.
    let mut done: BTreeSet<usize> = BTreeSet::new();
    for &(ei, _, _) in &extension_segments {
        if done.contains(&ei) || objects[ei].expression_id.is_none() {
            continue;
        }
        let (group, (a, b)) = collinear_pieces(objects, ei, "ExtensionLine");
        done.extend(group.iter().copied());
        let length = dist(a, b);
        if length < 1e-9 {
            continue;
        }
        let own: Vec<&(usize, P, P)> = dimension_segments
            .iter()
            .filter(|(di, _, _)| objects[*di].expression_id == objects[ei].expression_id && !group.contains(di))
            .collect();
        if own.is_empty() {
            continue;
        }
        let outside = |t: f64| if t < 0. { -t } else if t > 1. { t - 1. } else { 0. };
        let own_crossings: Vec<(f64, f64)> = own
            .iter()
            .filter_map(|(_, c, d)| crossing(a, b, *c, *d, ARROW_REACH).map(|t| (t, angle_between(angle_deg(a, b), angle_deg(*c, *d)))))
            .collect();
        let Some(&(t_own, angle)) = own_crossings
            .iter()
            .min_by(|x, y| outside(x.0).partial_cmp(&outside(y.0)).unwrap_or(std::cmp::Ordering::Equal))
        else {
            continue;
        };
        let (piece, beyond) = if outside(t_own) > 1e-9 {
            let end = if t_own < 0. { a } else { b };
            (piece_at(objects, &group, end), -outside(t_own) * length)
        } else {
            // Dimension lines the extension line reaches: crossings on the line, or within END_TOLERANCE past an end
            // (a member whose interval stops at the next dimension line still reaches it).
            let slack = END_TOLERANCE / length;
            let all: Vec<f64> = dimension_segments
                .iter()
                .filter(|(di, _, _)| !group.contains(di))
                .filter_map(|(_, c, d)| crossing(a, b, *c, *d, ARROW_REACH))
                .filter(|t| (-slack..=1. + slack).contains(t))
                .map(|t| t.clamp(0., 1.))
                .collect();
            let t_min = all.iter().copied().fold(t_own, f64::min);
            let t_max = all.iter().copied().fold(t_own, f64::max);
            let (from_a, from_b) = (t_min * length, (1. - t_max) * length);
            if from_a.min(from_b) <= EPS_COINCIDENT {
                // The line begins at a dimension line and lies on one side of it: a short piece is the part drawn
                // beyond the dimension line (a native dimension draws it as its own line), a long one stops there.
                let far = from_a.max(from_b);
                let end = if from_a > from_b { a } else { b };
                (piece_at(objects, &group, end), if far <= ARROW_REACH { far } else { 0. })
            } else if from_a <= from_b {
                (piece_at(objects, &group, a), from_a)
            } else {
                (piece_at(objects, &group, b), from_b)
            }
        };
        out.push(Relation::number(piece, "angleToDimensionLineDeg", angle));
        out.push(Relation::number(piece, "extensionBeyondUnits", beyond));
    }
    // Centre lines (collinear pieces of one expression as one line): how far each end runs past the edge of the
    // figure it crosses — visible outlines, or the break boundary of a partial or detail view.
    let mut done: BTreeSet<usize> = BTreeSet::new();
    for &ci in &lines {
        if done.contains(&ci) || !straight(ci, "CentreLine") {
            continue;
        }
        let (group, (a, b)) = collinear_pieces(objects, ci, "CentreLine");
        done.extend(group.iter().copied());
        let length = dist(a, b);
        if length < 1e-9 {
            continue;
        }
        let handles: BTreeSet<&str> = group
            .iter()
            .flat_map(|g| objects[*g].source_handles.iter().map(String::as_str))
            .collect();
        let mut params: Vec<f64> = Vec::new();
        for &oi in &lines {
            let o = &objects[oi];
            if group.contains(&oi)
                || !(o.is("VisibleOutline") || o.is("BreakBoundary"))
                || o.source_handles.iter().any(|h| handles.contains(h.as_str()))
            {
                continue;
            }
            for chain in &o.geometry.chains {
                for w in chain.windows(2) {
                    if let Some(p) = segment_intersection(a, b, w[0], w[1]) {
                        params.push(point_line(p, a, b).1);
                    }
                }
            }
        }
        if let (Some(min), Some(max)) = (
            params.iter().copied().reduce(f64::min),
            params.iter().copied().reduce(f64::max),
        ) {
            out.push(Relation::number(piece_at(objects, &group, a), "overshootUnits", min * length));
            out.push(Relation::number(piece_at(objects, &group, b), "overshootUnits", (1. - max) * length));
        }
    }
    // A radial dimension line (all its collinear pieces) passes through the centre of an arc and reaches it. A
    // jogged line (a radial part with a landing for the number) is radial through its radial part: a piece that
    // meets a radial piece of the same expression end to end is radial to the same arc.
    let radius_pieces: Vec<usize> = lines.iter().copied().filter(|i| straight(*i, "RadiusDimensionLine")).collect();
    let mut radial_to: HashMap<usize, Vec<usize>> = HashMap::new();
    for &ri in &radius_pieces {
        let (group, (a, b)) = collinear_pieces(objects, ri, "DimensionLine");
        let ends: Vec<P> = group
            .iter()
            .filter_map(|g| objects[*g].segment())
            .flat_map(|(c, d)| [c, d])
            .collect();
        for &oi in &lines {
            if group.contains(&oi) || objects[oi].geometry.arcs.is_empty() {
                continue;
            }
            let radial = objects[oi].geometry.arcs.iter().any(|(centre, radius)| {
                point_line(*centre, a, b).0 <= RADIAL_EPS
                    && ends.iter().any(|e| (dist(*e, *centre) - radius).abs() <= ARROW_REACH)
            });
            if radial {
                radial_to.entry(ri).or_default().push(oi);
            }
        }
    }
    for &ri in &radius_pieces {
        let mut arcs: Vec<usize> = radial_to.get(&ri).cloned().unwrap_or_default();
        if arcs.is_empty()
            && let Some((a, b)) = objects[ri].segment()
        {
            for &pi in &radius_pieces {
                if pi != ri
                    && objects[pi].expression_id == objects[ri].expression_id
                    && let Some((c, d)) = objects[pi].segment()
                    && [a, b].iter().any(|e| dist(*e, c).min(dist(*e, d)) <= END_TOLERANCE)
                    && let Some(found) = radial_to.get(&pi)
                {
                    arcs.extend(found.iter().copied());
                }
            }
        }
        arcs.sort();
        arcs.dedup();
        for oi in arcs {
            out.push(Relation::node(ri, "radialTo", oi));
        }
    }
    // The extent of a dimension (collinear pieces of its dimension line as one line): the distance between the
    // outermost crossings with the expression's own extension lines.
    let mut done: BTreeSet<usize> = BTreeSet::new();
    for &(di, _, _) in &dimension_segments {
        if done.contains(&di) || objects[di].expression_id.is_none() {
            continue;
        }
        let (group, (a, b)) = collinear_pieces(objects, di, "DimensionLine");
        done.extend(group.iter().copied());
        let length = dist(a, b);
        let params: Vec<f64> = extension_segments
            .iter()
            .filter(|(xi, _, _)| objects[*xi].expression_id == objects[di].expression_id && !group.contains(xi))
            .filter_map(|(_, c, d)| crossing(a, b, *c, *d, 1.0))
            .collect();
        if let (Some(min), Some(max)) = (params.iter().copied().reduce(f64::min), params.iter().copied().reduce(f64::max))
            && max - min > 1e-9
        {
            for &g in &group {
                out.push(Relation::number(g, "extentUnits", (max - min) * length));
            }
        }
    }
    // A linear dimension text written along the dimension line of its expression.
    for (ti, t) in objects.iter().enumerate() {
        let (Some(rotation), true) = (t.rotation_deg, t.is("LinearDimensionText")) else {
            continue;
        };
        for &li in &lines {
            let l = &objects[li];
            if l.is("DimensionLine")
                && l.expression_id.is_some()
                && l.expression_id == t.expression_id
                && let Some(angle) = l.angle_deg
                && angle_between(rotation, angle) <= PARALLEL_DEG
            {
                out.push(Relation::node(ti, "alignedWith", li));
            }
        }
    }
    // View titles and callout letters with the same letter; a title and the scale text of its expression. A title
    // written as several texts ("A" and "向") carries one letter: the letter of all the title texts of its expression.
    let titles: Vec<usize> = (0..objects.len()).filter(|i| objects[*i].is("ViewTitle")).collect();
    let callouts: Vec<usize> = (0..objects.len()).filter(|i| objects[*i].is("CalloutLetter")).collect();
    let own_letter = |i: usize| objects[i].text.as_deref().map(view_letter).unwrap_or_default();
    let letter = |i: usize| -> String {
        let own = own_letter(i);
        if !own.is_empty() || !objects[i].is("ViewTitle") {
            return own;
        }
        titles
            .iter()
            .filter(|t| objects[**t].expression_id.is_some() && objects[**t].expression_id == objects[i].expression_id)
            .map(|t| own_letter(*t))
            .find(|l| !l.is_empty())
            .unwrap_or_default()
    };
    for &ti in &titles {
        let lt = letter(ti);
        if lt.is_empty() {
            continue;
        }
        for &ci in &callouts {
            if objects[ci].expression_id != objects[ti].expression_id && letter(ci) == lt {
                out.push(Relation::node(ti, "sameLetterAs", ci));
                out.push(Relation::node(ci, "sameLetterAs", ti));
            }
        }
        for (si, s) in objects.iter().enumerate() {
            if s.is("ScaleText") && s.expression_id.is_some() && s.expression_id == objects[ti].expression_id {
                out.push(Relation::node(ti, "scaledBy", si));
            }
        }
    }
    // Section regions of one drawing compared pairwise: direction and spacing.
    let regions: Vec<usize> = (0..objects.len()).filter(|i| objects[*i].is("SectionRegion")).collect();
    for (n, &i) in regions.iter().enumerate() {
        for &j in &regions[n + 1..] {
            let (a, b) = (&objects[i], &objects[j]);
            if let (Some(aa), Some(ab)) = (a.angle_deg, b.angle_deg)
                && angle_between(aa, ab) > HATCH_ANGLE_DEG
            {
                out.push(Relation::node(i, "hatchDirectionDiffersFrom", j));
                out.push(Relation::node(j, "hatchDirectionDiffersFrom", i));
            }
            if let (Some(sa), Some(sb)) = (a.spacing, b.spacing)
                && sa > 1e-9
                && sb > 1e-9
                && (sa / sb).max(sb / sa) > HATCH_SPACING_RATIO
            {
                out.push(Relation::node(i, "hatchSpacingDiffersFrom", j));
                out.push(Relation::node(j, "hatchSpacingDiffersFrom", i));
            }
        }
    }
    // Chained dimensions: collinear dimension lines of different expressions joined by an extension line that
    // meets both (native dimension lines stop an arrow short of the extension line; the neighbour's lines overlap
    // when arrows are drawn outside).
    for (n, &(i, a0, a1)) in dimension_segments.iter().enumerate() {
        for &(j, b0, b1) in &dimension_segments[n + 1..] {
            let (a, b) = (&objects[i], &objects[j]);
            if a.expression_id.is_none() || a.expression_id == b.expression_id || collinear_gap(a0, a1, b0, b1).is_none() {
                continue;
            }
            let (sa, sb) = (stretch(a0, a1, ARROW_REACH), stretch(b0, b1, ARROW_REACH));
            let joined = extension_segments.iter().any(|(_, e0, e1)| {
                let (e0, e1) = stretch(*e0, *e1, 1.0);
                segment_intersection(e0, e1, sa.0, sa.1).is_some() && segment_intersection(e0, e1, sb.0, sb.1).is_some()
            });
            if joined {
                out.push(Relation::node(i, "chainedWith", j));
                out.push(Relation::node(j, "chainedWith", i));
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
                        number: None,
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
        if let Some(h) = o.height {
            props.push(("heightUnits", number(h)));
        }
        if let Some(s) = o.spacing {
            props.push(("spacingUnits", number(s)));
        }
        if let Some(l) = &o.dxf_layer {
            props.push(("dxfLayer", turtle_literal(l)));
        }
        if let Some(c) = o.colour {
            props.push(("colourIndex", format!("\"{c}\"^^xsd:integer")));
        }
        if let Some(c) = o.layer_colour {
            props.push(("layerColourIndex", format!("\"{c}\"^^xsd:integer")));
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
        let (object, kind, row_object) = match (r.object, &r.literal, r.number) {
            (Some(o), _, _) => (node_iri(&objects[o].id), "node", objects[o].id.clone()),
            (None, Some(l), _) => (turtle_literal(l), "text", l.clone()),
            (None, None, Some(x)) => (number(x), "number", x.to_string()),
            _ => continue,
        };
        writeln!(ttl, "{s} md:{} {object} .", r.predicate)?;
        triples += 1;
        let row = MdDraftingRelation {
            layer: layer.into(),
            subject: objects[r.subject].id.clone(),
            predicate: r.predicate.into(),
            object: row_object,
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
        let object = match (r.object, &r.literal, r.number) {
            (Some(o), _, _) => objects[o].id.clone(),
            (None, Some(l), _) => l.clone(),
            (None, None, Some(x)) => x.to_string(),
            _ => continue,
        };
        out.entry(objects[r.subject].id.clone())
            .or_default()
            .push(format!("{} {object}", r.predicate));
    }
    out
}

/// Keep every distinct reason for one finding, ordered by the authored shape ID. Equal messages from
/// different shapes occur once, at their earliest shape; message text breaks ties within one shape.
fn merge_finding_messages(mut messages: Vec<(String, String)>) -> String {
    messages.sort();
    let mut seen = BTreeSet::new();
    messages.into_iter()
        .filter_map(|(_, message)| seen.insert(message.clone()).then_some(message))
        .collect::<Vec<_>>()
        .join("；")
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
    let mut grouped = BTreeMap::new();
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
        let message = f["message"]
            .as_str()
            .filter(|m| !m.is_empty())
            .map(str::to_owned)
            .or_else(|| rule.messages.first().cloned())
            .unwrap_or_else(|| rule.label.clone());
        let (_, messages) = grouped.entry((rule.id.clone(), object.id.clone(), value.clone()))
            .or_insert_with(|| (MdDraftingFinding {
                layer: layer.into(),
                rule_id: rule.id.clone(),
                focus: object.id.clone(),
                focus_classes: object.classes.iter().map(|c| (*c).to_owned()).collect(),
                source_handles: object.source_handles.clone(),
                expression_ids: object.expression_id.iter().cloned().collect(),
                value,
                message: String::new(),
                severity: rule.severity.clone(),
            }, Vec::new()));
        messages.push((shape.to_owned(), message));
    }
    // The original deduplication key also fixes the order of each layer's exported findings.
    let findings = grouped.into_values().map(|(mut finding, messages)| {
        finding.message = merge_finding_messages(messages);
        finding
    }).collect();
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
        let mut objs = objects(layer, &g, &d)?;
        let mut rels = relations(&objs);
        rels.extend(constituents::relations(&g, &d, &objs)?);
        sections::augment(layer, &g, &d, &mut objs, &mut rels)?;
        cutting::append(layer, &g, &d, &mut objs, &mut rels)?;
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
                confidence: r.confidence.clone(),
                proposed_correction: r.proposed_correction.clone(),
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
        let mut objs = objects(layer, graph, &d)?;
        let mut rels = relations(&objs);
        rels.extend(constituents::relations(graph, &d, &objs)?);
        sections::augment(layer, graph, &d, &mut objs, &mut rels)?;
        cutting::append(layer, graph, &d, &mut objs, &mut rels)?;
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
    fn finding_messages_keep_all_reasons_in_shape_order_without_duplicates() {
        let messages: Vec<(String, String)> = [
            ("shape:z", "later reason"), ("shape:b", "shared reason"),
            ("shape:a", "first reason"), ("shape:c", "shared reason"),
            ("shape:a", "first reason"),
        ].into_iter().map(|(shape, message)| (shape.into(), message.into())).collect();
        assert_eq!(merge_finding_messages(messages.clone()), "first reason；shared reason；later reason");
        assert_eq!(merge_finding_messages(messages.into_iter().rev().collect()), "first reason；shared reason；later reason");
        assert_eq!(merge_finding_messages(vec![("shape:a".into(), "one；authored message".into())]), "one；authored message");
    }
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
        let ct = clean_text("\\A0;%%c57{\\H0.625000x;\\S+0.4^ +0.2;}");
        assert_eq!(ct.text, "Ø57+0.4/ +0.2");
        assert!(ct.width > 2.0 && ct.width < 6.0, "{}", ct.width);
        assert!(ct.stacked);
        // The main text ends where the stack begins: Ø (1.0), 5 (0.8), 7 (0.8).
        assert!((ct.main_width - 2.6).abs() < 1e-9, "{}", ct.main_width);
        assert_eq!(clean_text("30").text, "30");
        assert!(!clean_text("30").stacked);
        assert_eq!(clean_text("30").main_width, clean_text("30").width);
        assert_eq!(clean_text("%%D").text, "°");
        // Font stacking marks: the tolerance parts share the width of the wider one at reduced size.
        let ct = clean_text("%%C19\u{E1D6}+0.05\u{E1DA}\u{E1D7}+0.02\u{E1D9}");
        assert_eq!(ct.text, "Ø19+0.05/+0.02");
        assert!(ct.stacked);
        assert!((ct.main_width - 2.4).abs() < 1e-9, "{}", ct.main_width);
        let flat = clean_text("%%C19+0.05+0.02").width;
        assert!(ct.width < 0.6 * flat, "{} versus flat {flat}", ct.width);
    }
    #[test]
    fn a_stacked_text_has_a_main_box_and_a_stack_box() {
        // A horizontal text "Ø40" with a stacked tolerance: the stack box follows the main box and is taller.
        let dxf = b"0\nSECTION\n2\nENTITIES\n0\nMTEXT\n5\nA1\n8\n0\n10\n0\n20\n0\n40\n3.5\n71\n7\n1\n%%c40{\\H0.7x;\\S+0.1^ -0.1;}\n0\nENDSEC\n0\nEOF\n";
        let records = dxf::records(&dxf::decode(dxf).unwrap()).unwrap();
        let d = Dxf::new(&records).unwrap();
        let r = d.by_handle["A1"];
        let (main, stack, text, _) = d.text_box(r, (0., 0.)).unwrap().unwrap();
        assert_eq!(text, "Ø40+0.1/ -0.1");
        let stack = stack.expect("stack box");
        assert!((main.height - 3.5).abs() < 1e-9);
        assert!((stack.height - STACK_HEIGHT * 3.5).abs() < 1e-9);
        assert!((stack.origin.0 - (main.origin.0 + main.width)).abs() < 1e-9, "the stack begins where the main text ends");
        assert!((stack.origin.1 - (main.origin.1 - STACK_DROP * 3.5)).abs() < 1e-9);
        // A line through the stack region, past the end of the main text, is a crossing of the stack box only.
        let y = main.origin.1 + 0.5 * 3.5;
        let (x0, x1) = (stack.origin.0 + 0.1, stack.origin.0 + stack.width + 5.);
        assert_eq!(main.chord((x0, y), (x1, y)), 0.);
        assert!(stack.chord((x0, y), (x1, y)) > 0.3 * 3.5);
        // A line passing just above the main text, before the stack begins, crosses nothing.
        let y = main.origin.1 + 3.5 + 0.3;
        let (x0, x1) = (main.origin.0 - 5., main.origin.0 + 0.5 * main.width);
        assert_eq!(main.chord((x0, y), (x1, y)) + stack.chord((x0, y), (x1, y)), 0.);
    }
    #[test]
    fn polyline_bulges_are_arcs_with_centres() {
        // A quarter arc from (10,0) to (0,10) around the origin: bulge tan(90°/4) = 1 - √2 … = 0.41421356.
        let dxf = b"0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n5\nB1\n8\n0\n90\n2\n70\n0\n10\n10\n20\n0\n42\n0.41421356\n10\n0\n20\n10\n0\nENDSEC\n0\nEOF\n";
        let records = dxf::records(&dxf::decode(dxf).unwrap()).unwrap();
        let d = Dxf::new(&records).unwrap();
        let (chains, arcs) = d.chains(d.by_handle["B1"], (0., 0.)).unwrap();
        assert_eq!(arcs.len(), 1);
        let (c, r) = arcs[0];
        assert!(dist(c, (0., 0.)) < 1e-6 && (r - 10.).abs() < 1e-6, "{c:?} {r}");
        assert!(chains[0].iter().all(|p| (dist(*p, (0., 0.)) - 10.).abs() < 1e-6));
        assert!(dist(chains[0][0], (10., 0.)) < 1e-9 && dist(*chains[0].last().unwrap(), (0., 10.)) < 1e-6);
    }
    #[test]
    fn an_extension_line_without_a_measure_is_not_a_finding() {
        // The beyond rule is an sh:or whose last branch constrains a property; with no value of that property the
        // branch holds vacuously and the line is not a finding.
        let k = knowledge().unwrap();
        let rules = rules(&k.shapes).unwrap();
        let object = Object {
            id: "source:expression:0:m1".into(),
            classes: vec!["Line", "ExtensionLine"],
            source_handles: vec!["X1".into()],
            member_ids: vec!["expression:0:m1".into()],
            expression_id: Some("expression:0".into()),
            expression_kind: Some("dimension_diameter".into()),
            view: None,
            line_type: Some("dimension".into()),
            text: None,
            rotation_deg: None,
            angle_deg: Some(0.),
            height: None,
            spacing: None,
            dxf_layer: None,
            colour: None,
            layer_colour: None,
            geometry: Geometry { chains: vec![vec![(0., 0.), (10., 0.)]], ..Geometry::default() },
        };
        let objects = vec![object];
        let (ttl, _, _, _) = facts("source", &objects, &[]).unwrap();
        let by_iri: HashMap<String, &Object> = objects.iter().map(|o| (node_iri(&o.id), o)).collect();
        let (found, _) = evaluate("source", &ttl, &k.shapes, &rules, &by_iri, &HashMap::new()).unwrap();
        assert!(found.is_empty(), "{found:#?}\n{ttl}");
    }
    #[test]
    fn letters_gaps_and_angles() {
        assert_eq!(view_letter("A向"), "A");
        assert_eq!(view_letter("B—B"), "B");
        assert_eq!(view_letter(" K 向"), "K");
        assert_eq!(view_letter("Ⅰ 2:1"), "Ⅰ");
        assert_eq!(view_letter("2:1"), "");
        assert_eq!(collinear_gap((0., 0.), (10., 0.), (12., 0.05), (20., 0.05)), Some(2.));
        assert!(collinear_gap((0., 0.), (10., 0.), (5., 0.), (20., 0.)).is_some_and(|g| g < 0.));
        assert_eq!(collinear_gap((0., 0.), (10., 0.), (12., 2.), (20., 2.)), None);
        assert_eq!(angle_between(10., 170.), 20.);
        assert_eq!(line_param((0., 0.), (10., 0.), (2., -1.), (2., 1.)), Some(0.2));
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
