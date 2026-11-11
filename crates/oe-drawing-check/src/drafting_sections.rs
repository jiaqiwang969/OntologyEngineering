//! Region-scoped drafting facts. Geometry comes from native DXF, context from the frozen graph.
//! This exporter does not decide a drafting violation: the two rule groups and their exceptions
//! live in drafting-ontology.toml. It never changes the graph, source types or member intervals.
use super::*;
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Edge {
    Line(P, P),
    Arc {
        c: P,
        r: f64,
        start: f64,
        sweep: f64,
    },
}
impl Edge {
    fn at(self, t: f64) -> P {
        match self {
            Self::Line(a, b) => (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)),
            Self::Arc { c, r, start, sweep } => {
                let a = start + t * sweep;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            }
        }
    }
    fn length(self) -> f64 {
        match self {
            Self::Line(a, b) => dist(a, b),
            Self::Arc { r, sweep, .. } => r * sweep.abs(),
        }
    }
    fn slice(self, lo: f64, hi: f64) -> Self {
        match self {
            Self::Line(_, _) => Self::Line(self.at(lo), self.at(hi)),
            Self::Arc { c, r, start, sweep } => Self::Arc {
                c,
                r,
                start: start + lo * sweep,
                sweep: (hi - lo) * sweep,
            },
        }
    }
    fn parameters(self, p: P) -> Vec<f64> {
        match self {
            Self::Line(a, b) => {
                let l2 = (b.0 - a.0).powi(2) + (b.1 - a.1).powi(2);
                if l2 <= f64::EPSILON {
                    vec![]
                } else {
                    vec![((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / l2]
                }
            }
            Self::Arc {
                c, start, sweep, ..
            } => {
                let a = (p.1 - c.1).atan2(p.0 - c.0);
                (-3..=3)
                    .map(|k| (a - start + f64::from(k) * TAU) / sweep)
                    .collect()
            }
        }
    }
    fn contains(self, p: P, tol: f64) -> bool {
        self.parameters(p)
            .iter()
            .any(|t| *t >= -1e-10 && *t <= 1. + 1e-10 && dist(self.at(t.clamp(0., 1.)), p) <= tol)
    }
    /// Positive-length overlap measured in this edge's parameter. An arc's chord is never an edge.
    fn overlaps(self, other: Self, tol: f64) -> Vec<[f64; 2]> {
        let compatible = match (self, other) {
            (Self::Line(a, b), Self::Line(c, d)) => {
                let len = dist(a, b);
                len > tol
                    && ((c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)).abs() / len <= tol
                    && ((d.0 - a.0) * (b.1 - a.1) - (d.1 - a.1) * (b.0 - a.0)).abs() / len <= tol
            }
            (Self::Arc { c, r, .. }, Self::Arc { c: c2, r: r2, .. }) => {
                dist(c, c2) <= tol && (r - r2).abs() <= tol
            }
            _ => false,
        };
        if !compatible {
            return vec![];
        }
        let mut cuts = vec![0., 1.];
        for p in [other.at(0.), other.at(1.)] {
            cuts.extend(
                self.parameters(p)
                    .into_iter()
                    .filter(|t| *t > 0. && *t < 1.),
            );
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        cuts.windows(2)
            .filter(|w| {
                (w[1] - w[0]) * self.length() > tol
                    && other.contains(self.at((w[0] + w[1]) / 2.), tol)
            })
            .map(|w| [w[0], w[1]])
            .collect()
    }
    fn sample(self, sagitta: f64) -> Option<Vec<P>> {
        let count = match self {
            Self::Line(_, _) => 1,
            Self::Arc { r, sweep, .. } => {
                if !r.is_finite() || r <= 0. || !sweep.is_finite() {
                    return None;
                }
                let step = (2. * (1. - (sagitta / r).min(1.)).acos()).max(1e-6);
                (sweep.abs() / step).ceil().max(1.) as usize
            }
        };
        if count > 4096 {
            return None;
        }
        Some(
            (0..=count)
                .map(|i| self.at(i as f64 / count as f64))
                .collect(),
        )
    }
}

#[derive(Default)]
struct Face {
    edges: Vec<Edge>,
    polygon_edges: Vec<(P, P)>,
    complete: bool,
}

fn bulged(a: P, b: P, bulge: f64) -> Edge {
    if bulge.abs() < 1e-12 {
        return Edge::Line(a, b);
    }
    let c = (
        (a.0 + b.0) / 2. - (b.1 - a.1) * (1. - bulge * bulge) / (4. * bulge),
        (a.1 + b.1) / 2. + (b.0 - a.0) * (1. - bulge * bulge) / (4. * bulge),
    );
    Edge::Arc {
        c,
        r: dist(c, a),
        start: (a.1 - c.1).atan2(a.0 - c.0),
        sweep: 4. * bulge.atan(),
    }
}

/// Native straight/circular HATCH paths, with holes retained by even-odd containment.
/// Unsupported, open or malformed paths keep partial geometry and cannot support interior claims.
fn face(r: &dxf::Record, tol: f64, sagitta: f64) -> Face {
    let mut out = Face::default();
    if r.kind != "HATCH"
        || r.tag(75).is_some_and(|_| r.float(75).ok() != Some(0.))
        || r.tag(210).is_some_and(|_| r.float(210).ok() != Some(0.))
        || r.tag(220).is_some_and(|_| r.float(220).ok() != Some(0.))
        || r.tag(230).is_some_and(|_| r.float(230).ok() != Some(1.))
    {
        return out;
    }
    let Some(start) = r.tags.iter().position(|t| t.0 == 91) else {
        return out;
    };
    let end = r
        .tags
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, t)| t.0 == 75)
        .map_or(r.tags.len(), |(i, _)| i);
    let starts: Vec<usize> = (start + 1..end).filter(|i| r.tags[*i].0 == 92).collect();
    out.complete =
        !starts.is_empty() && r.tags[start].1.parse::<usize>().ok() == Some(starts.len());
    for (n, &i) in starts.iter().enumerate() {
        let ts = &r.tags[i + 1..starts.get(n + 1).copied().unwrap_or(end)];
        let num = |code| {
            ts.iter()
                .find(|t| t.0 == code)
                .and_then(|t| t.1.parse::<f64>().ok())
        };
        let mut edges = Vec::new();
        if r.tags[i].1.parse::<i64>().unwrap_or(0) & 2 != 0 {
            let mut vertices = Vec::new();
            for k in 0..ts.len().saturating_sub(1) {
                if ts[k].0 == 10 && ts[k + 1].0 == 20 {
                    if let (Ok(x), Ok(y)) = (ts[k].1.parse::<f64>(), ts[k + 1].1.parse::<f64>()) {
                        let b = ts
                            .get(k + 2)
                            .filter(|v| v.0 == 42)
                            .and_then(|v| v.1.parse::<f64>().ok())
                            .unwrap_or(0.);
                        vertices.push(((x, y), b));
                    } else {
                        out.complete = false;
                    }
                }
            }
            let closed = num(73) == Some(1.);
            if !closed || num(93) != Some(vertices.len() as f64) {
                out.complete = false;
            }
            for j in 0..vertices.len().saturating_sub(usize::from(!closed)) {
                if vertices.len() > 1 {
                    edges.push(bulged(
                        vertices[j].0,
                        vertices[(j + 1) % vertices.len()].0,
                        vertices[j].1,
                    ));
                }
            }
        } else {
            let indices: Vec<usize> = ts
                .iter()
                .enumerate()
                .filter(|(_, t)| t.0 == 72)
                .map(|(i, _)| i)
                .collect();
            if num(93) != Some(indices.len() as f64) {
                out.complete = false;
            }
            for (j, &k) in indices.iter().enumerate() {
                let vals = &ts[k + 1..indices.get(j + 1).copied().unwrap_or(ts.len())];
                let get = |code| {
                    vals.iter()
                        .find(|v| v.0 == code)
                        .and_then(|v| v.1.parse::<f64>().ok())
                };
                match ts[k].1.as_str() {
                    "1" => {
                        if let (Some(x), Some(y), Some(u), Some(v)) =
                            (get(10), get(20), get(11), get(21))
                        {
                            edges.push(Edge::Line((x, y), (u, v)));
                        } else {
                            out.complete = false;
                        }
                    }
                    "2" => {
                        if let (Some(x), Some(y), Some(radius), Some(a), Some(z)) =
                            (get(10), get(20), get(40), get(50), get(51))
                        {
                            let sweep = if (z - a).abs() >= 360. - 1e-9 {
                                TAU
                            } else {
                                (z - a).rem_euclid(360.).to_radians()
                            };
                            let direction = if get(73) == Some(0.) { -1. } else { 1. };
                            edges.push(Edge::Arc {
                                c: (x, y),
                                r: radius,
                                start: direction * a.to_radians(),
                                sweep: direction * sweep,
                            });
                        } else {
                            out.complete = false;
                        }
                    }
                    _ => out.complete = false,
                }
            }
        }
        if edges.is_empty()
            || edges
                .iter()
                .any(|e| !e.length().is_finite() || e.length() <= 0.)
        {
            out.complete = false;
        }
        // Edge paths may list an edge backwards; closure is topological, independent of winding.
        for e in &edges {
            for p in [e.at(0.), e.at(1.)] {
                let degree = edges
                    .iter()
                    .flat_map(|v| [v.at(0.), v.at(1.)])
                    .filter(|q| dist(p, *q) <= tol)
                    .count();
                if degree != 2 {
                    out.complete = false;
                }
            }
            if let Some(ps) = e.sample(sagitta) {
                out.polygon_edges
                    .extend(ps.windows(2).map(|w| (w[0], w[1])));
            } else {
                out.complete = false;
            }
        }
        out.edges.extend(edges);
    }
    if out.edges.len() > 4096 {
        return Face::default();
    }
    // A native hatch may partition one filled face into adjacent closed paths. Their coincident
    // seams are not the face's exterior. Normal hatch filling uses parity, including hole paths.
    // Split overlaps before cancelling even coverage, so a partly shared edge keeps its exterior.
    let mut exterior = Vec::new();
    for (i, &edge) in out.edges.iter().enumerate() {
        let overlaps: Vec<(usize, [f64; 2])> = out
            .edges
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .flat_map(|(j, other)| {
                edge.overlaps(*other, tol)
                    .into_iter()
                    .map(move |span| (j, span))
            })
            .collect();
        let mut cuts = vec![0., 1.];
        cuts.extend(overlaps.iter().flat_map(|(_, span)| *span));
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        for w in cuts.windows(2) {
            let t = (w[0] + w[1]) / 2.;
            let covering: Vec<usize> = overlaps
                .iter()
                .filter(|(_, span)| span[0] < t && t < span[1])
                .map(|(j, _)| *j)
                .collect();
            if covering.len().is_multiple_of(2) && covering.iter().all(|j| *j > i) {
                exterior.push(edge.slice(w[0], w[1]));
            }
        }
    }
    out.edges = exterior;
    out.polygon_edges.clear();
    for edge in &out.edges {
        if let Some(ps) = edge.sample(sagitta) {
            out.polygon_edges
                .extend(ps.windows(2).map(|w| (w[0], w[1])));
        } else {
            out.complete = false;
        }
    }
    out
}

fn inside(p: P, edges: &[(P, P)], tol: f64) -> bool {
    if edges.iter().any(|(a, b)| {
        Edge::Line(*a, *b)
            .parameters(p)
            .iter()
            .any(|t| dist(Edge::Line(*a, *b).at(t.clamp(0., 1.)), p) <= tol)
    }) {
        return false;
    }
    edges
        .iter()
        .filter(|(a, b)| {
            (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0
        })
        .count()
        % 2
        == 1
}
fn interior_length(a: P, b: P, face: &Face, tol: f64) -> f64 {
    let e = Edge::Line(a, b);
    let mut cuts = vec![0., 1.];
    for (p, q) in &face.polygon_edges {
        if let Some(x) = segment_intersection(a, b, *p, *q) {
            cuts.extend(e.parameters(x).into_iter().filter(|t| *t > 0. && *t < 1.));
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
    cuts.windows(2)
        .filter(|w| inside(e.at((w[0] + w[1]) / 2.), &face.polygon_edges, tol))
        .map(|w| (w[1] - w[0]) * e.length())
        .sum()
}

#[derive(Clone, Copy, PartialEq)]
struct Piece {
    edge: Edge,
    lo: f64,
    hi: f64,
    parameter: &'static str,
}
fn pieces(o: &Object, d: &Dxf<'_>, layer: &str, members: &BTreeMap<&str, &Value>) -> Vec<Piece> {
    let raw = o
        .source_handles
        .first()
        .and_then(|h| d.by_handle.get(h.as_str()).copied());
    if raw.is_some_and(|r| matches!(r.kind.as_str(), "SPLINE" | "ELLIPSE")) {
        return vec![];
    }
    let direct = o.member_ids.len() == 1 && !o.id.contains(":line:") && !o.id.contains(":implied:");
    let mut edges = Vec::new();
    let mut parameter = "polyline_length";
    if direct
        && let Some(r) = raw
        && matches!(r.kind.as_str(), "ARC" | "CIRCLE")
    {
        if let (Ok(x), Ok(y), Ok(radius)) = (r.float(10), r.float(20), r.float(40)) {
            let (a, z) = if r.kind == "CIRCLE" {
                (0., 360.)
            } else {
                (r.float(50).unwrap_or(0.), r.float(51).unwrap_or(0.))
            };
            let sweep = if (z - a).abs() >= 360. - 1e-9 {
                TAU
            } else {
                (z - a).rem_euclid(360.).to_radians()
            };
            edges.push(Edge::Arc {
                c: (x, y),
                r: radius,
                start: a.to_radians(),
                sweep,
            });
            parameter = "source_arc";
        }
    } else if o.geometry.arcs.is_empty() {
        // The existing exporter already clips LINE on recognition; take original bytes for a single root LINE.
        if direct
            && let Some(r) = raw
            && r.kind == "LINE"
        {
            if let (Ok(x), Ok(y), Ok(u), Ok(v)) =
                (r.float(10), r.float(20), r.float(11), r.float(21))
            {
                edges.push(Edge::Line((x, y), (u, v)));
                parameter = "source_line";
            }
        } else {
            edges.extend(
                o.geometry
                    .chains
                    .iter()
                    .flat_map(|c| c.windows(2).map(|w| Edge::Line(w[0], w[1]))),
            );
        }
    }
    let total: f64 = edges.iter().map(|e| e.length()).sum();
    if !total.is_finite() || total <= 0. {
        return vec![];
    }
    let range = if direct && layer != "source" {
        o.member_ids
            .first()
            .and_then(|id| members.get(id.as_str()))
            .and_then(|m| m.get("interval"))
            .and_then(Value::as_array)
            .and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]))
            .unwrap_or([0., 1.])
    } else {
        [0., 1.]
    };
    if !range.iter().all(|v| v.is_finite())
        || range[0] < 0.
        || range[1] > 1.
        || range[0] >= range[1]
    {
        return vec![];
    }
    let mut offset = 0.;
    let mut out = Vec::new();
    for edge in edges {
        let next = offset + edge.length() / total;
        let lo = offset.max(range[0]);
        let hi = next.min(range[1]);
        if hi > lo {
            out.push(Piece {
                edge: edge.slice(
                    (lo - offset) / (next - offset),
                    (hi - offset) / (next - offset),
                ),
                lo,
                hi,
                parameter,
            });
        }
        offset = next;
    }
    out
}
fn evidence(v: &Value) -> Vec<&str> {
    v.get("evidence_ids")
        .or_else(|| v.get("evidence"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}
fn single<'a>(v: &'a Value, prefix: &str, allowed: &[&str]) -> &'a str {
    let values: BTreeSet<&str> = evidence(v)
        .into_iter()
        .filter_map(|s| s.strip_prefix(prefix))
        .collect();
    if values.len() == 1 && allowed.contains(values.first().unwrap()) {
        values.first().unwrap()
    } else {
        "unknown"
    }
}
fn formal<'a>(v: &'a Value, name: &str, allowed: &[&str]) -> Option<&'a str> {
    v.get(name)
        .and_then(Value::as_str)
        .filter(|value| allowed.contains(value))
}
fn origin(o: &Object, region: &str, members: &BTreeMap<&str, &Value>) -> String {
    let mut found = BTreeSet::new();
    for id in &o.member_ids {
        if let Some(m) = members.get(id.as_str()) {
            if let Some(c) = formal(
                m,
                "section_origin",
                &["pre_existing", "cut_formed", "suppressed", "unknown"],
            ) {
                found.insert(c.to_owned());
                continue;
            }
            for s in evidence(m)
                .into_iter()
                .filter_map(|s| s.strip_prefix("section_origin_region:"))
            {
                if let Ok(v) = serde_json::from_str::<Value>(s)
                    && v["region"].as_str() == Some(region)
                    && let Some(c) = v["class"].as_str()
                    && ["pre_existing", "cut_formed", "suppressed", "unknown"].contains(&c)
                {
                    found.insert(c.to_owned());
                }
            }
        }
    }
    if found.len() == 1 {
        found.into_iter().next().unwrap()
    } else {
        "unknown".into()
    }
}
fn literal(out: &mut Vec<Relation>, subject: usize, predicate: &'static str, value: &str) {
    out.push(Relation {
        subject,
        predicate,
        object: None,
        literal: Some(value.into()),
        number: None,
    });
}
fn number(out: &mut Vec<Relation>, subject: usize, predicate: &'static str, value: f64) {
    // Source parameters retain full f64 precision; the old drawing-unit rounding is not applicable.
    out.push(Relation {
        subject,
        predicate,
        object: None,
        literal: None,
        number: Some(value),
    });
}
fn occurrence(
    layer: &str,
    class: &'static str,
    region: &Object,
    line: &Object,
    serial: usize,
) -> Object {
    let mut o = line.clone();
    o.id = format!("{layer}:{class}:{}:{}:{serial}", region.id, line.id);
    o.classes = vec![class];
    o.expression_id = region.expression_id.clone();
    o.expression_kind = None;
    o.geometry = Geometry::default();
    o.angle_deg = None;
    o.spacing = None;
    o.text = None;
    o
}
fn context(
    region: &Object,
    expressions: &BTreeMap<&str, &Value>,
    views: &BTreeMap<&str, &Value>,
) -> (String, String) {
    let empty = Value::Null;
    let expression = region
        .expression_id
        .as_deref()
        .and_then(|id| expressions.get(id))
        .copied()
        .unwrap_or(&empty);
    let view = region
        .view
        .as_deref()
        .and_then(|id| views.get(id))
        .copied()
        .unwrap_or(&empty);
    let ct = formal(
        expression,
        "cross_section_type",
        &["superimposed", "removed", "unknown"],
    )
    .unwrap_or_else(|| {
        single(
            expression,
            "cross_section_type:",
            &["superimposed", "removed", "unknown"],
        )
    });
    let ev = evidence(view);
    let ee = evidence(expression);
    let extent = formal(
        expression,
        "section_extent",
        &["half", "local", "local_axis", "cross_section", "unknown"],
    )
    .unwrap_or_else(|| {
        if ev.contains(&"section_type:half") || ee.contains(&"section_context:section_type:half") {
            "half"
        } else if ee.contains(&"section_context:local") {
            "local"
        } else if ct != "unknown" {
            "cross_section"
        } else {
            "unknown"
        }
    });
    // `section_geometry` remains a graph fact, but the v7 rules exporter
    // derives its fact from the native HATCH boundary below. The two sources
    // are intentionally kept separate until a later unification decision.
    (ct.into(), extent.into())
}

pub(super) fn augment(
    layer: &str,
    g: &Value,
    d: &Dxf<'_>,
    objects: &mut Vec<Object>,
    relations: &mut Vec<Relation>,
) -> Result<()> {
    let knowledge: toml::Value = toml::from_str(KNOWLEDGE)?;
    let param = |key: &str| -> Result<f64> {
        knowledge["section_parameters"][key]
            .as_float()
            .filter(|v| v.is_finite() && *v > 0.)
            .context("section_parameter")
    };
    let mut heights: Vec<f64> = objects
        .iter()
        .filter_map(|o| o.height)
        .filter(|h| *h > 0. && h.is_finite())
        .collect();
    // With no text, use median hatch spacing as a drawing scale; otherwise preserve unavailable coverage.
    if heights.is_empty() {
        heights = objects
            .iter()
            .filter_map(|o| o.spacing)
            .filter(|h| *h > 0. && h.is_finite())
            .collect();
    }
    let Some(scale) = median(&mut heights) else {
        return Ok(());
    };
    let (tol, sagitta, min_chord) = (
        scale * param("coincidence_text_height")?,
        scale * param("sagitta_text_height")?,
        scale * param("interior_chord_text_height")?,
    );
    let members: BTreeMap<&str, &Value> = list(g, "members")?
        .iter()
        .filter_map(|m| Some((m["id"].as_str()?, m)))
        .collect();
    let expressions: BTreeMap<&str, &Value> = list(g, "expressions")?
        .iter()
        .filter_map(|e| Some((e["id"].as_str()?, e)))
        .collect();
    let views: BTreeMap<&str, &Value> = g["views"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| Some((v["id"].as_str()?, v)))
        .collect();
    let original_count = objects.len();
    let curves: Vec<Vec<Piece>> = objects
        .iter()
        .map(|o| pieces(o, d, layer, &members))
        .collect();
    for ri in 0..original_count {
        if !objects[ri].is("SectionRegion") {
            continue;
        }
        let region = objects[ri].clone();
        let (ctype, extent) = context(&region, &expressions, &views);
        let raw = if region.source_handles.len() == 1 {
            d.by_handle.get(region.source_handles[0].as_str()).copied()
        } else {
            None
        };
        let face = raw
            .filter(|r| r.kind == "HATCH")
            .map(|r| face(r, tol, sagitta))
            .unwrap_or_default();
        let inferred_geometry = if face.complete {
            "complete"
        } else if face.edges.is_empty() {
            "unavailable"
        } else {
            "partial"
        };
        let geometry = inferred_geometry;
        literal(relations, ri, "crossSectionType", &ctype);
        literal(relations, ri, "sectionExtent", &extent);
        literal(relations, ri, "sectionGeometry", geometry);
        let same_view = |i: usize| objects[i].view == region.view;
        let centers: Vec<usize> = (0..original_count)
            .filter(|i| objects[*i].is("CentreLine") && same_view(*i))
            .collect();
        let outlines: Vec<usize> = (0..original_count)
            .filter(|i| objects[*i].is("VisibleOutline") && same_view(*i))
            .collect();
        let interior: Vec<usize> = (0..original_count)
            .filter(|i| {
                (objects[*i].is("VisibleOutline") || objects[*i].is("ThreadLine")) && same_view(*i)
            })
            .collect();
        let region_id = region.expression_id.as_deref().unwrap_or("");
        let mut serial = 0;
        for si in centers {
            for p in &curves[si] {
                let mut spans: Vec<[f64; 2]> = face
                    .edges
                    .iter()
                    .flat_map(|edge| p.edge.overlaps(*edge, tol))
                    .collect();
                spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
                let mut merged: Vec<[f64; 2]> = Vec::new();
                for span in spans {
                    if let Some(last) = merged.last_mut()
                        && span[0] <= last[1] + 1e-10
                    {
                        last[1] = last[1].max(span[1]);
                    } else {
                        merged.push(span);
                    }
                }
                for span in merged {
                    let coverage: Vec<(usize, [f64; 2])> = outlines
                        .iter()
                        .filter(|i| **i != si)
                        .flat_map(|i| {
                            curves[*i].iter().flat_map(move |q| {
                                p.edge
                                    .overlaps(q.edge, tol)
                                    .into_iter()
                                    .map(move |s| (*i, s))
                            })
                        })
                        .collect();
                    let mut cuts = vec![span[0], span[1]];
                    cuts.extend(
                        coverage
                            .iter()
                            .flat_map(|(_, s)| *s)
                            .filter(|t| *t > span[0] && *t < span[1]),
                    );
                    cuts.sort_by(f64::total_cmp);
                    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
                    for w in cuts
                        .windows(2)
                        .filter(|w| (w[1] - w[0]) * p.edge.length() > tol)
                    {
                        let id = objects.len();
                        serial += 1;
                        objects.push(occurrence(
                            layer,
                            "SectionBoundarySpan",
                            &region,
                            &objects[si],
                            serial,
                        ));
                        relations.push(Relation::node(id, "sectionRegion", ri));
                        relations.push(Relation::node(id, "sourceLine", si));
                        relations.push(Relation::node(id, "boundaryCarrier", si));
                        let midpoint = (w[0] + w[1]) / 2.;
                        let covered: BTreeSet<usize> = coverage
                            .iter()
                            .filter(|(_, s)| s[0] <= midpoint && midpoint <= s[1])
                            .map(|(i, _)| *i)
                            .collect();
                        for i in covered {
                            relations.push(Relation::node(id, "boundaryCarrier", i));
                        }
                        number(
                            relations,
                            id,
                            "boundaryIntervalStart",
                            p.lo + (p.hi - p.lo) * w[0],
                        );
                        number(
                            relations,
                            id,
                            "boundaryIntervalEnd",
                            p.lo + (p.hi - p.lo) * w[1],
                        );
                        literal(relations, id, "boundaryParameter", p.parameter);
                        literal(relations, id, "sectionGeometry", geometry);
                        literal(
                            relations,
                            id,
                            "sectionExtent",
                            if extent == "local" && matches!(p.edge, Edge::Line(_, _)) {
                                "local_axis"
                            } else {
                                &extent
                            },
                        );
                        literal(
                            relations,
                            id,
                            "sectionOrigin",
                            &origin(&objects[si], region_id, &members),
                        );
                    }
                }
            }
        }
        if !face.complete {
            continue;
        }
        for &si in &interior {
            let samples: Option<Vec<Vec<P>>> =
                curves[si].iter().map(|p| p.edge.sample(sagitta)).collect();
            let Some(samples) = samples else {
                continue;
            };
            let inside_length: f64 = samples
                .iter()
                .flat_map(|p| p.windows(2))
                .map(|w| interior_length(w[0], w[1], &face, tol))
                .sum();
            if inside_length < min_chord {
                continue;
            }
            let closed = samples
                .first()
                .and_then(|p| p.first())
                .zip(samples.last().and_then(|p| p.last()))
                .is_some_and(|(a, b)| dist(*a, *b) <= tol);
            let connected = samples.windows(2).all(|w| {
                w[0].last()
                    .zip(w[1].first())
                    .is_some_and(|(a, b)| dist(*a, *b) <= tol)
            });
            let continues = connected
                && if closed {
                    samples
                        .iter()
                        .flatten()
                        .any(|p| !inside(*p, &face.polygon_edges, tol))
                } else {
                    samples
                        .first()
                        .and_then(|p| p.first())
                        .zip(samples.last().and_then(|p| p.last()))
                        .is_some_and(|(a, b)| {
                            !inside(*a, &face.polygon_edges, tol)
                                && !inside(*b, &face.polygon_edges, tol)
                        })
                };
            let id = objects.len();
            serial += 1;
            objects.push(occurrence(
                layer,
                "SectionInteriorUse",
                &region,
                &objects[si],
                serial,
            ));
            relations.push(Relation::node(id, "sectionRegion", ri));
            // Preserve all graph roles of the very same source curve and interval. The shape decides
            // whether any of these roles supplies an exception; no exception is applied here.
            for &alias in &interior {
                if objects[alias].source_handles == objects[si].source_handles
                    && curves[alias] == curves[si]
                {
                    relations.push(Relation::node(id, "sourceLine", alias));
                }
            }
            literal(relations, id, "sectionGeometry", "complete");
            literal(relations, id, "crossSectionType", &ctype);
            literal(
                relations,
                id,
                "parentOutlineContinuation",
                if continues { "true" } else { "false" },
            );
            literal(
                relations,
                id,
                "sectionOrigin",
                &origin(&objects[si], region_id, &members),
            );
        }
    }
    ensure!(objects.len() <= 20_000, "section_fact_object_budget");
    Ok(())
}
