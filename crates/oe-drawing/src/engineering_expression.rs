//! Complete engineering-expression assembly (profile `dxf-expression-rules/2`).
//!
//! The layer sits above the lossless [`MdUsageGraph`]. A native DXF entity is
//! evidence for one or more expression members; it is not the expression kind.
//! Three axes stay independent: discipline expression kind, geometric line type
//! and native primitive. Every member keeps its source identity and a bounded
//! source interval; ambiguous views and part targets stay unresolved.
//!
//! Generic evidence used here:
//! * actual LTYPE dash elements (DXF group 49) for dashed / dash-dot / dash-double-dot;
//! * GB/T 14665-2012 Table 6 layer identifiers, only after the drawing's own
//!   layer linetypes are found consistent with that table;
//! * GB/T 14665-2012 4.3 precedence of coincident lines (visible outline first,
//!   extension and boundary lines last) for shared source intervals;
//! * display text of dimensions (diameter sign, radius, thread, chamfer, angle)
//!   instead of the native DIMENSION enum alone;
//! * terminator / dimension-line / extension-line / text composition for
//!   exploded dimensions (GB/T 4458.4, GB/T 14665-2012 clause 6).
//!
//! * drafting representation knowledge cards (`knowledge/drawing-representation.toml`),
//!   read when recognition runs; the rules that use a card name it in what they produce.
//!
//! No old labels, repaired intervals, manual regions or paired models are read.
use crate::{
    discipline_discovery as dd,
    expression_text::{self as tx, Lex},
    native_readback::{Drawing, Record},
    usage_discovery,
};
use anyhow::{Result, ensure};
use oe_contracts::*;
use std::collections::{BTreeMap, BTreeSet};

#[path = "structure_topology.rs"]
mod topology;

#[path = "representation.rs"]
mod representation;
pub use representation::{USED_CARDS, drawing_concepts, representation_knowledge};

pub const PROFILE: &str = "dxf-expression-rules/2";
pub const SCHEMA: &str = "oev2.drawing-expression-graph/1";
type P = [f64; 2];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: P, k: f64) -> P {
    [a[0] * k, a[1] * k]
}
fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn norm(a: P) -> f64 {
    a[0].hypot(a[1])
}
fn dist(a: P, b: P) -> f64 {
    norm(sub(a, b))
}
fn mid(a: P, b: P) -> P {
    [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.]
}
fn unit(a: P) -> P {
    let n = norm(a);
    if n < 1e-12 {
        [0., 0.]
    } else {
        [a[0] / n, a[1] / n]
    }
}
fn lerp(a: P, b: P, t: f64) -> P {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
/// Parameter of the projection of `p` on segment `ab` and its perpendicular distance.
fn project(p: P, a: P, b: P) -> (f64, f64) {
    let v = sub(b, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return (0., dist(p, a));
    }
    let t = dot(sub(p, a), v) / n;
    (t, cross(sub(p, a), v).abs() / n.sqrt())
}
/// `p` lies on segment `ab` within `tol` (drawing units); returns the clamped parameter.
fn on_seg(p: P, a: P, b: P, tol: f64) -> Option<f64> {
    let (t, d) = project(p, a, b);
    let l = dist(a, b).max(1e-12);
    (d <= tol && t >= -tol / l && t <= 1. + tol / l).then(|| t.clamp(0., 1.))
}
fn bbox_of(points: &[P]) -> Option<[f64; 4]> {
    if points.is_empty() {
        return None;
    }
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in points {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    Some(b)
}
fn bbox_union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn bbox_gap(a: [f64; 4], b: [f64; 4]) -> f64 {
    let dx = (a[0] - b[2]).max(b[0] - a[2]).max(0.);
    let dy = (a[1] - b[3]).max(b[1] - a[3]).max(0.);
    dx.hypot(dy)
}
fn inside(p: P, b: [f64; 4], margin: f64) -> bool {
    p[0] >= b[0] - margin && p[0] <= b[2] + margin && p[1] >= b[1] - margin && p[1] <= b[3] + margin
}
fn median(mut v: Vec<f64>) -> Option<f64> {
    v.retain(|x| x.is_finite());
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[v.len() / 2])
}

// ---------------------------------------------------------------- line styles

/// Pattern class from the actual LTYPE dash elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Continuous,
    Dashed,
    DashDot,
    DashDoubleDot,
    ByBlock,
    NotCurve,
    Unknown,
}
impl Pattern {
    pub fn name(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::Dashed => "dashed",
            Self::DashDot => "dash_dot",
            Self::DashDoubleDot => "dash_double_dot",
            Self::ByBlock => "byblock_unresolved",
            Self::NotCurve => "not_curve",
            Self::Unknown => "unknown",
        }
    }
}

/// Classify dash elements (positive = mark, zero = dot, negative = gap).
/// Long marks are at least 60% of the longest mark; dots and short dashes are
/// "short". The names and colours of linetypes never enter this decision.
pub fn classify_pattern(elements: &[f64]) -> Pattern {
    if elements.iter().any(|x| !x.is_finite()) {
        return Pattern::Unknown;
    }
    if elements.is_empty() || !elements.iter().any(|x| *x < 0.) {
        return Pattern::Continuous;
    }
    let marks: Vec<f64> = elements.iter().copied().filter(|x| *x >= 0.).collect();
    let max = marks.iter().copied().fold(0., f64::max);
    if marks.is_empty() || max <= 0. {
        return Pattern::Unknown;
    }
    let long = marks.iter().filter(|x| **x >= 0.6 * max).count();
    let short = marks.len() - long;
    if short == 0 {
        Pattern::Dashed
    } else if short == long {
        Pattern::DashDot
    } else if short == 2 * long {
        Pattern::DashDoubleDot
    } else {
        Pattern::Unknown
    }
}

/// GB/T 14665-2012 Table 6 identifiers (13–15 are user selected and carry no meaning).
const TABLE6: [(&str, &str, Pattern, &str); 12] = [
    ("01", "粗实线", Pattern::Continuous, "thick_continuous"),
    (
        "02",
        "细实线、波浪线、双折线",
        Pattern::Continuous,
        "thin_continuous",
    ),
    ("03", "粗虚线", Pattern::Dashed, "thick_dashed"),
    ("04", "细虚线", Pattern::Dashed, "thin_dashed"),
    ("05", "细点画线", Pattern::DashDot, "thin_dash_dot"),
    ("06", "粗点画线", Pattern::DashDot, "thick_dash_dot"),
    (
        "07",
        "细双点画线",
        Pattern::DashDoubleDot,
        "thin_dash_double_dot",
    ),
    (
        "08",
        "尺寸线、投影连线、尺寸终端与符号细实线、尺寸和公差",
        Pattern::Continuous,
        "thin_continuous",
    ),
    (
        "09",
        "参考圆，包括引出线及其终端（如箭头）",
        Pattern::Continuous,
        "thin_continuous",
    ),
    ("10", "剖面符号", Pattern::Continuous, "thin_continuous"),
    (
        "11",
        "文本（细实线）",
        Pattern::Continuous,
        "thin_continuous",
    ),
    (
        "12",
        "文本（粗实线）",
        Pattern::Continuous,
        "thick_continuous",
    ),
];
fn table6(layer: &str) -> Option<&'static (&'static str, &'static str, Pattern, &'static str)> {
    TABLE6.iter().find(|x| x.0 == layer)
}

#[derive(Clone, Debug)]
struct LayerInfo {
    color: i32,
    linetype: String,
}

struct Convention {
    qualified: bool,
    evidence: Vec<String>,
}

/// The layer convention is evidence only when every present Table 6 layer has a
/// linetype pattern consistent with the table and at least two non-continuous
/// layers confirm it. Otherwise layer names are ignored.
fn convention(
    layers: &BTreeMap<String, LayerInfo>,
    patterns: &BTreeMap<String, Pattern>,
) -> Convention {
    let mut evidence = Vec::new();
    let (mut ok, mut bad, mut ok_broken) = (0, 0, 0);
    for (id, desc, want, _) in TABLE6.iter() {
        let Some(layer) = layers.get(*id) else {
            continue;
        };
        let got = pattern_of(&layer.linetype, patterns);
        if got == *want {
            ok += 1;
            if *want != Pattern::Continuous {
                ok_broken += 1;
            }
            evidence.push(format!("layer:{id}:{}:consistent:{desc}", got.name()));
        } else {
            bad += 1;
            evidence.push(format!(
                "layer:{id}:{}:inconsistent:expected_{}",
                got.name(),
                want.name()
            ));
        }
    }
    let qualified = ok >= 4 && bad == 0 && ok_broken >= 2;
    evidence.push(format!(
        "gbt14665_2012_table6:{}:consistent={ok}:inconsistent={bad}:broken_patterns={ok_broken}",
        if qualified {
            "qualified"
        } else {
            "not_qualified"
        }
    ));
    Convention {
        qualified,
        evidence,
    }
}

fn pattern_of(name: &str, patterns: &BTreeMap<String, Pattern>) -> Pattern {
    let u = name.to_uppercase();
    if u == "BYBLOCK" {
        return Pattern::ByBlock;
    }
    if u == "CONTINUOUS" {
        return Pattern::Continuous;
    }
    patterns.get(&u).copied().unwrap_or(Pattern::Unknown)
}

#[derive(Clone, Debug)]
struct Style {
    layer: String,
    pattern: Pattern,
    color: i32,
    explicit_color: Option<i32>,
    byblock: bool,
}

fn style_of(
    r: &Record,
    layers: &BTreeMap<String, LayerInfo>,
    patterns: &BTreeMap<String, Pattern>,
) -> Style {
    let layer = r.get(8).unwrap_or("0").to_string();
    let info = layers.get(&layer);
    let name = r.get(6).unwrap_or("BYLAYER");
    let lt = if name.eq_ignore_ascii_case("BYLAYER") {
        info.map_or("CONTINUOUS".to_string(), |l| l.linetype.clone())
    } else {
        name.to_string()
    };
    let explicit_color = r
        .get(62)
        .and_then(|v| v.trim().parse::<i32>().ok())
        .filter(|c| *c != 256);
    let color = match explicit_color {
        Some(c) => c,
        None => info.map_or(7, |l| l.color.abs()),
    };
    let pattern = pattern_of(&lt, patterns);
    Style {
        layer,
        pattern,
        color,
        explicit_color,
        byblock: pattern == Pattern::ByBlock || explicit_color == Some(0),
    }
}

fn is_curve_kind(kind: &str) -> bool {
    matches!(
        kind,
        "LINE" | "ARC" | "CIRCLE" | "LWPOLYLINE" | "POLYLINE" | "SPLINE" | "ELLIPSE"
    )
}

/// Base line type before any role rule: (line type, basis, Table 6 description).
fn base_line_type(
    kind: &str,
    st: &Style,
    conv: &Convention,
) -> (&'static str, &'static str, Option<String>) {
    match kind {
        "TEXT" | "MTEXT" | "ATTRIB" => return ("text", "native_kind", None),
        "SOLID" | "TRACE" | "HATCH" => return ("fill", "native_kind", None),
        "POINT" => return ("point", "native_kind", None),
        "INSERT" => return ("block", "native_kind", None),
        "DIMENSION" => return ("dimension", "native_kind", None),
        k if !is_curve_kind(k) => return ("unknown", "unknown", None),
        _ => {}
    }
    let rule = conv
        .qualified
        .then(|| table6(&st.layer).map(|t| format!("GB/T 14665-2012 表6 {} {}", t.0, t.1)))
        .flatten();
    let thick_layer = |id: &str| conv.qualified && st.layer == id;
    match st.pattern {
        Pattern::Dashed => (
            if thick_layer("03") {
                "thick_dashed"
            } else {
                "thin_dashed"
            },
            "observed_pattern",
            rule,
        ),
        Pattern::DashDot => (
            if thick_layer("06") {
                "thick_dash_dot"
            } else {
                "thin_dash_dot"
            },
            "observed_pattern",
            rule,
        ),
        Pattern::DashDoubleDot => ("thin_dash_double_dot", "observed_pattern", rule),
        Pattern::Continuous => match (conv.qualified, table6(&st.layer)) {
            (true, Some(t)) if t.2 == Pattern::Continuous => (t.3, "layer_convention", rule),
            _ => ("continuous", "observed_pattern", rule),
        },
        Pattern::ByBlock => ("continuous", "observed_pattern", rule),
        Pattern::NotCurve | Pattern::Unknown => ("unknown", "unknown", rule),
    }
}

// ---------------------------------------------------------------- text, arrows, polylines

#[derive(Clone, Debug)]
struct TextFact {
    id: String,
    dec: tx::Decoded,
    lex: Lex,
    center: P,
    dir: P,
    height: f64,
    width: f64,
}

fn int(r: &Record, code: i32) -> i32 {
    r.get(code)
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(0)
}

fn text_fact(r: &Record) -> Option<TextFact> {
    let raw = dd::raw_text(r);
    let dec = tx::decode(&raw);
    if dec.text.is_empty() {
        return None;
    }
    let h = r.number(40).unwrap_or(2.5).abs().max(1e-6);
    let rot = if r.kind == "MTEXT" {
        match (r.number(11), r.number(21)) {
            (Ok(x), Ok(y)) if x.hypot(y) > 1e-12 => y.atan2(x),
            _ => r.number(50).unwrap_or(0.).to_radians(),
        }
    } else {
        r.number(50).unwrap_or(0.).to_radians()
    };
    let dir = [rot.cos(), rot.sin()];
    let n = [-dir[1], dir[0]];
    let lines_n = raw.matches("\\P").count() as f64 + 1.;
    // Stacked deviations are drawn at reduced height after the main value.
    let stack = dec
        .upper
        .as_deref()
        .map_or(0., tx::width_factor)
        .max(dec.lower.as_deref().map_or(0., tx::width_factor));
    let main = if dec.main.trim().is_empty() {
        &dec.text
    } else {
        &dec.main
    };
    // TEXT group 41 is the relative width factor of the glyphs.
    let wfac = if r.kind == "TEXT" {
        r.number(41).unwrap_or(1.).abs().clamp(0.1, 10.)
    } else {
        1.
    };
    let width = h * wfac * (tx::width_factor(main) / lines_n + 0.6 * stack);
    let total_h = h * (1. + 1.6 * (lines_n - 1.));
    let center = if r.kind == "MTEXT" {
        let p = r.xy(10).ok()?;
        let att = int(r, 71).clamp(1, 9);
        let fx = [0., 0.5, 1.][((att - 1) % 3) as usize];
        let fy = [1., 0.5, 0.][((att - 1) / 3) as usize];
        add(
            add(p, mul(dir, width * (0.5 - fx))),
            mul(n, total_h * (0.5 - fy)),
        )
    } else {
        let h72 = int(r, 72);
        let v73 = int(r, 73);
        let p10 = r.xy(10).ok()?;
        let p11 = r.xy(11).unwrap_or(p10);
        if matches!(h72, 3 | 5) && dist(p10, p11) > 1e-9 {
            add(mid(p10, p11), mul(n, h * 0.5))
        } else {
            let p = if h72 != 0 || v73 != 0 { p11 } else { p10 };
            let fx = match h72 {
                1 | 4 => 0.5,
                2 => 1.,
                _ => 0.,
            };
            let fy = if h72 == 4 {
                0.5
            } else {
                match v73 {
                    3 => 1.,
                    2 => 0.5,
                    _ => 0.,
                }
            };
            add(add(p, mul(dir, width * (0.5 - fx))), mul(n, h * (0.5 - fy)))
        }
    };
    let lex = tx::lex(&dec.text);
    Some(TextFact {
        id: r.handle().ok()?.to_string(),
        dec,
        lex,
        center,
        dir,
        height: h,
        width,
    })
}

#[derive(Clone, Debug)]
struct Arrow {
    tip: P,
    dir: P,
    len: f64,
}

/// LWPOLYLINE vertices as (point, start width, end width, bulge); the constant
/// width (group 43) applies where no per-vertex width is given.
fn lw_vertices(r: &Record) -> Vec<(P, f64, f64, f64)> {
    let mut verts: Vec<(P, f64, f64, f64)> = Vec::new();
    let mut x = None;
    let constant = r.number(43).unwrap_or(0.);
    for (c, v) in &r.tags {
        let num = || v.trim().parse::<f64>().unwrap_or(0.);
        match c {
            10 => x = v.trim().parse::<f64>().ok(),
            20 => {
                if let (Some(a), Ok(b)) = (x.take(), v.trim().parse::<f64>()) {
                    verts.push(([a, b], constant, constant, 0.));
                }
            }
            40 => {
                if let Some(last) = verts.last_mut() {
                    last.1 = num();
                }
            }
            41 => {
                if let Some(last) = verts.last_mut() {
                    last.2 = num();
                }
            }
            42 => {
                if let Some(last) = verts.last_mut() {
                    last.3 = num();
                }
            }
            _ => {}
        }
    }
    verts
}

/// A straight segment tapering from a positive width to zero is a filled
/// arrowhead whose tip is the zero-width end.
fn taper(a: P, b: P, ws: f64, we: f64) -> Option<Arrow> {
    let l = dist(a, b);
    let (tip, base, w) = if ws > 1e-9 && we.abs() < 1e-9 {
        (b, a, ws)
    } else if we > 1e-9 && ws.abs() < 1e-9 {
        (a, b, we)
    } else {
        return None;
    };
    (l > 1e-9 && (2.0..=12.).contains(&(l / w))).then(|| Arrow {
        tip,
        dir: unit(sub(tip, base)),
        len: l,
    })
}

/// A two-vertex LWPOLYLINE tapering from a positive width to zero is a filled
/// arrowhead, the same terminator as a SOLID.
fn tapered_arrow(r: &Record) -> Option<Arrow> {
    if r.kind != "LWPOLYLINE" {
        return None;
    }
    let v = lw_vertices(r);
    if v.len() != 2 || v.iter().any(|x| x.3 != 0.) {
        return None;
    }
    taper(v[0].0, v[1].0, v[0].1, v[0].2)
}

/// An open LWPOLYLINE of plain straight segments whose last (or first) segment
/// tapers to zero width is a leader drawn with its own arrowhead. Returns the
/// leader path without the tapered segment, and the arrowhead.
fn inline_arrow(r: &Record) -> Option<(Vec<P>, Arrow)> {
    if r.kind != "LWPOLYLINE" || int(r, 70) & 1 != 0 {
        return None;
    }
    let mut v = lw_vertices(r);
    v.dedup_by(|later, kept| {
        let same = dist(later.0, kept.0) < 1e-9;
        if same {
            kept.1 = later.1;
            kept.2 = later.2;
            kept.3 = later.3;
        }
        same
    });
    let n = v.len();
    if n < 3 || v.iter().any(|x| x.3 != 0.) {
        return None;
    }
    let plain = |rg: std::ops::Range<usize>| {
        rg.into_iter()
            .all(|i| v[i].1.abs() < 1e-9 && v[i].2.abs() < 1e-9)
    };
    if let Some(a) =
        taper(v[n - 2].0, v[n - 1].0, v[n - 2].1, v[n - 2].2).filter(|_| plain(0..n - 2))
    {
        return Some((v[..n - 1].iter().map(|x| x.0).collect(), a));
    }
    if let Some(a) = taper(v[0].0, v[1].0, v[0].1, v[0].2).filter(|_| plain(1..n - 1)) {
        return Some((v[1..].iter().map(|x| x.0).collect(), a));
    }
    None
}

fn arrow_of(r: &Record) -> Option<Arrow> {
    if let Some((tip, dir, len)) = dd::arrow(r) {
        return Some(Arrow { tip, dir, len });
    }
    tapered_arrow(r)
}

/// Base point of an arrowhead.
fn base_of(a: &Arrow) -> P {
    sub(a.tip, mul(a.dir, a.len))
}

/// The arrowhead ends a line arriving at `end` from `from`: the line stops on the
/// head's axis between its base and tip (drawn up to, into or through the head),
/// and the head points onward along the line.
fn continues(a: &Arrow, end: P, from: P, tol: f64) -> bool {
    let along = sub(end, from);
    let s = dot(sub(end, base_of(a)), a.dir);
    let off = cross(a.dir, sub(end, base_of(a))).abs();
    s >= -tol
        && s <= a.len + tol
        && off <= tol
        && (norm(along) < 1e-9 || dot(a.dir, unit(along)) > 0.9)
}

/// Vertices of a planar polyline without repeated consecutive points; bulged
/// polylines are reported so callers do not treat arcs as straight segments.
/// Spline-fit POLYLINEs keep their frame control points as VERTEX flag 16;
/// only the fitted vertices lie on the drawn curve.
fn poly_points(d: &Drawing, r: &Record) -> (Vec<P>, bool, bool) {
    let mut pts = Vec::new();
    let mut bulged = false;
    let closed = int(r, 70) & 1 != 0;
    if r.kind == "LWPOLYLINE" {
        for (p, _, _, bulge) in lw_vertices(r) {
            pts.push(p);
            bulged |= bulge != 0.;
        }
    } else if r.kind == "POLYLINE" {
        let spline = int(r, 70) & 4 != 0;
        for child in d.records.iter().skip(r.index + 1) {
            if child.kind != "VERTEX" {
                break;
            }
            if spline && int(child, 70) & 16 != 0 {
                continue;
            }
            if let Ok(p) = child.xy(10) {
                pts.push(p);
            }
            bulged |= child.number(42).is_ok_and(|b| b != 0.);
        }
    }
    pts.dedup_by(|a, b| dist(*a, *b) < 1e-9);
    (pts, closed, bulged)
}

/// Axis-aligned rectangle covered by a 4- or 5-vertex polyline whose edges are
/// at least three sides of that rectangle (an open cell shares its fourth side).
fn cell_rect(pts: &[P]) -> Option<[f64; 4]> {
    if !(4..=5).contains(&pts.len()) {
        return None;
    }
    let b = bbox_of(pts)?;
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if w < 1e-6 || h < 1e-6 {
        return None;
    }
    let tol = 1e-4 * (w + h);
    let on_side = |p: &P| {
        (p[0] - b[0]).abs() < tol
            || (p[0] - b[2]).abs() < tol
            || (p[1] - b[1]).abs() < tol
            || (p[1] - b[3]).abs() < tol
    };
    let corners = pts.iter().filter(|p| {
        ((p[0] - b[0]).abs() < tol || (p[0] - b[2]).abs() < tol)
            && ((p[1] - b[1]).abs() < tol || (p[1] - b[3]).abs() < tol)
    });
    (pts.iter().all(on_side)
        && corners.count() >= 4
        && pts
            .windows(2)
            .all(|w| (w[0][0] - w[1][0]).abs() < tol || (w[0][1] - w[1][1]).abs() < tol))
    .then_some(b)
}

// ---------------------------------------------------------------- expression builder

fn family(kind: &str) -> &'static str {
    match kind {
        "visible_contour" | "hidden_contour" | "center_axis" | "phantom_line"
        | "thread_convention" | "break_line" | "section_hatch" => "geometry",
        "thread_note" | "leader" | "center_hole" => "dimension",
        k if k.starts_with("dimension_") => "dimension",
        "geometric_tolerance" | "datum_symbol" => "tolerance",
        k if k.starts_with("gps_") => "tolerance",
        "surface_roughness" => "surface",
        "view_callout" => "view",
        "technical_general"
        | "material_heat"
        | "process_requirement"
        | "protection_requirement" => "note",
        "sheet_frame" | "title_block" => "sheet",
        "construction_reference" => "construction",
        _ => "unresolved",
    }
}

fn knowledge(kind: &str) -> Vec<String> {
    let ids: &[&str] = match kind {
        "visible_contour" => &["K03", "K09", "K14"],
        "hidden_contour" | "center_axis" => &["K03", "K09"],
        "phantom_line" => &["K03", "K13"],
        "thread_convention" => &["K13", "K34"],
        "break_line" => &["K03", "K13"],
        "section_hatch" => &["K12", "K03"],
        "dimension_linear" => &["K17", "K18", "K55"],
        "dimension_diameter" => &["K17", "K19", "K20"],
        "dimension_radius" => &["K17", "K20", "K39"],
        "dimension_angle" => &["K17", "K20"],
        "dimension_chamfer" => &["K17", "K39"],
        "thread_note" => &["K17", "K34", "K39"],
        "gps_position" => &["K23", "K24", "K26"],
        "gps_direction"
        | "gps_perpendicularity"
        | "gps_parallelism"
        | "gps_runout"
        | "gps_symmetry"
        | "geometric_tolerance" => &["K23", "K24", "K25"],
        "datum_symbol" => &["K24"],
        "center_hole" => &["K39"],
        "surface_roughness" => &["K31", "K32"],
        "view_callout" => &["K11", "K13"],
        "technical_general" => &["K14", "K21"],
        "material_heat" => &["K40"],
        "process_requirement" => &["K41"],
        "protection_requirement" => &["K33"],
        "sheet_frame" => &["K03", "K61"],
        "title_block" => &["K01", "K61"],
        "construction_reference" => &["K51", "K55"],
        "leader" => &["K55"],
        _ => &["K14"],
    };
    ids.iter().map(|s| s.to_string()).collect()
}

/// Line type of a member after the role rule (GB/T 4457.4: visible outlines are
/// thick continuous, dimension and leader lines thin continuous). Observed
/// broken patterns are never overridden.
fn member_line_type(base: &'static str, role: &str) -> &'static str {
    let continuous = matches!(base, "continuous" | "thick_continuous" | "thin_continuous");
    match role {
        "contour_segment" if base == "continuous" => "thick_continuous",
        "break_curve" if continuous => "wavy",
        "arrowhead" | "hatch_fill" => "fill",
        "definition_point" => "point",
        "native_dimension" => "dimension",
        "dimension_line" | "extension_line" | "leader_line" | "frame_cell" | "table_line"
        | "frame_border" | "view_arrow" | "detail_circle" | "thread_line"
            if base == "continuous" =>
        {
            "thin_continuous"
        }
        _ => base,
    }
}

/// Tolerance on normalized source parameters below which two interval ends are one.
const MEMBER_EPS: f64 = 1e-9;

fn same_interval(a: &[f64], b: &[f64]) -> bool {
    a.len() == 2
        && b.len() == 2
        && (a[0] - b[0]).abs() <= MEMBER_EPS
        && (a[1] - b[1]).abs() <= MEMBER_EPS
}

#[derive(Default)]
struct Builder {
    exprs: Vec<MdEngineeringExpression>,
    members: Vec<MdExpressionMember>,
    by_source: BTreeMap<String, Vec<usize>>,
    /// Relations between expressions (from, relation, to), resolved to final ids.
    links: Vec<(usize, &'static str, usize)>,
}
impl Builder {
    #[allow(clippy::too_many_arguments)]
    fn expr(
        &mut self,
        kind: &str,
        label: &str,
        rule: &str,
        state: &str,
        qualifiers: Vec<String>,
        evidence: Vec<String>,
        unknowns: Vec<String>,
    ) -> usize {
        let id = format!("expression:{}", self.exprs.len());
        self.exprs.push(MdEngineeringExpression {
            id,
            kind: kind.into(),
            family: family(kind).into(),
            label: label.trim().into(),
            member_ids: vec![],
            view_id: None,
            engineering_target: None,
            qualifiers,
            knowledge_ids: knowledge(kind),
            state: state.into(),
            rule: rule.into(),
            evidence_ids: evidence,
            unknowns,
        });
        self.exprs.len() - 1
    }
    #[allow(clippy::too_many_arguments)]
    fn member(
        &mut self,
        e: usize,
        src: &str,
        native: Option<String>,
        iv: [f64; 2],
        role: &str,
        line_type: &str,
        state: &str,
        relations: Vec<String>,
        evidence: Vec<String>,
    ) {
        // Interval ends within MEMBER_EPS of the source ends are the source ends, and intervals
        // that agree within it are one membership: a coordinate printed with fewer digits (one
        // DXF writer rounds to 16 significant digits, another keeps the exact double) must not
        // make a second, redundant member.
        let snap = |t: f64| {
            let t = t.clamp(0., 1.);
            if t <= MEMBER_EPS {
                0.
            } else if t >= 1. - MEMBER_EPS {
                1.
            } else {
                t
            }
        };
        let iv = [snap(iv[0]), snap(iv[1])];
        let iv = if iv[0] <= iv[1] { iv } else { [iv[1], iv[0]] };
        if self.by_source.get(src).is_some_and(|ms| {
            ms.iter().any(|&m| {
                let x = &self.members[m];
                x.expression_id == self.exprs[e].id
                    && x.native_member_id == native
                    && x.member_role == role
                    && same_interval(&x.interval, &iv)
            })
        }) {
            return;
        }
        let eid = self.exprs[e].id.clone();
        let id = format!("{eid}:m{}", self.exprs[e].member_ids.len());
        self.exprs[e].member_ids.push(id.clone());
        self.by_source
            .entry(src.to_string())
            .or_default()
            .push(self.members.len());
        self.members.push(MdExpressionMember {
            id,
            expression_id: eid,
            source_handle: src.into(),
            native_member_id: native,
            interval: iv.to_vec(),
            member_role: role.into(),
            line_type: line_type.into(),
            view_id: None,
            state: state.into(),
            relation_ids: relations,
            evidence_ids: evidence,
            unknowns: vec![],
        });
    }
    fn used(&self, src: &str) -> bool {
        self.by_source.get(src).is_some_and(|v| !v.is_empty())
    }
    fn kinds_of(&self, src: &str) -> BTreeSet<String> {
        let ids: BTreeMap<&str, &str> = self
            .exprs
            .iter()
            .map(|e| (e.id.as_str(), e.kind.as_str()))
            .collect();
        self.by_source
            .get(src)
            .map(|ms| {
                ms.iter()
                    .map(|&m| ids[self.members[m].expression_id.as_str()].to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
    fn covered(&self, src: &str) -> Vec<[f64; 2]> {
        let mut v: Vec<[f64; 2]> = self
            .by_source
            .get(src)
            .map(|ms| {
                ms.iter()
                    .map(|&m| [self.members[m].interval[0], self.members[m].interval[1]])
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut out: Vec<[f64; 2]> = Vec::new();
        for r in v {
            match out.last_mut() {
                Some(last) if r[0] <= last[1] + 1e-12 => last[1] = last[1].max(r[1]),
                _ => out.push(r),
            }
        }
        out
    }
}

// ---------------------------------------------------------------- context

struct Ctx<'a> {
    d: &'a Drawing,
    src: &'a MdUsageGraph,
    rec: BTreeMap<String, &'a Record>,
    block_rec: BTreeMap<String, &'a Record>,
    style: BTreeMap<String, Style>,
    base: BTreeMap<String, (&'static str, &'static str, Option<String>)>,
    conv: Convention,
    texts: BTreeMap<String, TextFact>,
    arrows: BTreeMap<String, Arrow>,
    inline_arrows: BTreeMap<String, Arrow>,
    lines: BTreeMap<String, (P, P)>,
    polys: BTreeMap<String, (Vec<P>, bool, bool)>,
    shapes: BTreeMap<String, dd::Shape>,
    scale: f64,
    thin_colors: BTreeSet<i32>,
    extent: [f64; 4],
}
impl Ctx<'_> {
    fn lt(&self, id: &str) -> &'static str {
        self.base.get(id).map_or("unknown", |b| b.0)
    }
    fn kind(&self, id: &str) -> &str {
        self.rec.get(id).map_or("", |r| r.kind.as_str())
    }
    /// Arrowhead of a terminator source: a SOLID or tapered polyline, or the head
    /// drawn inside a leader polyline.
    fn arrow(&self, id: &str) -> Option<&Arrow> {
        self.arrows.get(id).or_else(|| self.inline_arrows.get(id))
    }
    /// Root curve geometry as polyline points for proximity and view tests.
    fn points(&self, id: &str) -> Vec<P> {
        if let Some((a, b)) = self.lines.get(id) {
            return vec![*a, *b];
        }
        if let Some((p, _, _)) = self.polys.get(id) {
            return p.clone();
        }
        if let Some(t) = self.texts.get(id) {
            let half = mul(t.dir, t.width / 2.);
            return vec![sub(t.center, half), add(t.center, half)];
        }
        if let Some(a) = self.arrows.get(id) {
            return vec![a.tip, sub(a.tip, mul(a.dir, a.len))];
        }
        if let Some(s) = self.shapes.get(id) {
            let mut v: Vec<P> = s.lines.iter().flat_map(|(a, b)| [*a, *b]).collect();
            v.extend(
                s.circles
                    .iter()
                    .flat_map(|(c, r)| [sub(*c, [*r, *r]), add(*c, [*r, *r])]),
            );
            v.extend(s.texts.iter().map(|t| t.1));
            v.extend(s.paths.iter().flatten().copied());
            return v;
        }
        let Some(r) = self.rec.get(id) else {
            return vec![];
        };
        if let Some(e) = self.src.entities.iter().find(|e| e.id == id)
            && matches!(e.geometry_kind.as_str(), "arc" | "circle")
            && e.coordinates.len() >= 5
        {
            let (c, rad, s0, sw) = (
                [e.coordinates[0], e.coordinates[1]],
                e.coordinates[2],
                e.coordinates[3],
                e.coordinates[4],
            );
            return (0..=16)
                .map(|i| {
                    add(
                        c,
                        mul(
                            [
                                (s0 + sw * i as f64 / 16.).cos(),
                                (s0 + sw * i as f64 / 16.).sin(),
                            ],
                            rad,
                        ),
                    )
                })
                .collect();
        }
        match r.kind.as_str() {
            "POINT" => r.xy(10).map(|p| vec![p]).unwrap_or_default(),
            // Definition points; a text position written as the origin is a placeholder.
            "DIMENSION" => [10, 11, 13, 14, 15, 16]
                .iter()
                .filter_map(|c| r.xy(*c).ok())
                .filter(|p| p[0].abs() + p[1].abs() > 1e-9)
                .collect(),
            "HATCH" => {
                // Boundary path vertices only: after the path count (91) and before the
                // hatch style (75); the elevation point and seed points are excluded.
                let mut v = Vec::new();
                let mut x = None;
                let mut inside_paths = false;
                for (c, val) in &r.tags {
                    match c {
                        91 => inside_paths = true,
                        75 | 76 | 98 => inside_paths = false,
                        10 | 11 if inside_paths => x = val.trim().parse::<f64>().ok(),
                        20 | 21 if inside_paths => {
                            if let (Some(a), Ok(b)) = (x.take(), val.trim().parse::<f64>()) {
                                v.push([a, b]);
                            }
                        }
                        _ => {}
                    }
                }
                v
            }
            _ => vec![],
        }
    }
    fn bbox(&self, id: &str) -> Option<[f64; 4]> {
        bbox_of(&self.points(id))
    }
    /// Thin annotation line style: thin by the checked layer convention, exploded
    /// (ByBlock) or an explicit thin colour. Without a qualified convention every
    /// continuous line may serve, and geometry alone must decide.
    fn annotation_style(&self, id: &str) -> bool {
        let Some(st) = self.style.get(id) else {
            return false;
        };
        if !matches!(st.pattern, Pattern::Continuous | Pattern::ByBlock) {
            return false;
        }
        if st.byblock
            || self.lt(id) == "thin_continuous"
            || st
                .explicit_color
                .is_some_and(|c| self.thin_colors.contains(&c))
        {
            return true;
        }
        !self.conv.qualified && self.lt(id) == "continuous"
    }
    /// Structural line style: continuous, not exploded, not thin by convention or colour.
    fn structural_style(&self, id: &str) -> bool {
        let Some(st) = self.style.get(id) else {
            return false;
        };
        st.pattern == Pattern::Continuous
            && !st.byblock
            && !st
                .explicit_color
                .is_some_and(|c| self.thin_colors.contains(&c))
            && matches!(self.lt(id), "thick_continuous" | "continuous")
            && !(self.conv.qualified
                && matches!(st.layer.as_str(), "08" | "09" | "10" | "11" | "12"))
    }
    fn arc(&self, id: &str) -> Option<(P, f64, f64, f64, bool)> {
        let e = self.src.entities.iter().find(|e| e.id == id)?;
        (matches!(e.geometry_kind.as_str(), "arc" | "circle") && e.coordinates.len() >= 5).then(
            || {
                (
                    [e.coordinates[0], e.coordinates[1]],
                    e.coordinates[2],
                    e.coordinates[3],
                    e.coordinates[4],
                    e.geometry_kind == "circle",
                )
            },
        )
    }
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-7 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ---------------------------------------------------------------- native dimensions

/// The measurement a native DIMENSION displays for `<>`: group 42 when the file computed it. A
/// negative or non-finite value is not a measurement (AutoCAD writes -1 when it has not computed
/// one); then the number the dimension block displays stands for it, and failing that the
/// definition points of a linear, aligned, diameter or radius dimension.
fn measurement(r: &Record, dtype: i32, brecs: &[&Record]) -> Option<String> {
    if let Some(m) = r.number(42).ok().filter(|m| m.is_finite() && *m >= 0.) {
        return Some(fmt_num(m));
    }
    let shown = brecs
        .iter()
        .filter(|x| matches!(x.kind.as_str(), "MTEXT" | "TEXT"))
        .map(|x| tx::decode(&dd::raw_text(x)).text.trim().to_string())
        .find(|t| t.parse::<f64>().is_ok_and(|v| v.is_finite() && v >= 0.));
    if shown.is_some() {
        return shown;
    }
    let pt = |c: i32| r.xy(c).ok();
    let v = match dtype {
        0 => {
            let a = r.number(50).unwrap_or(0.).to_radians();
            pt(13)
                .zip(pt(14))
                .map(|(p, q)| dot(sub(q, p), [a.cos(), a.sin()]).abs())
        }
        1 => pt(13).zip(pt(14)).map(|(p, q)| dist(p, q)),
        3 | 4 => pt(10).zip(pt(15)).map(|(p, q)| dist(p, q)),
        _ => None,
    };
    v.filter(|v| v.is_finite()).map(fmt_num)
}

/// Native DIMENSION roots: kind from the displayed text, native enum as fallback.
/// The anonymous dimension block is placed at the DIMENSION's clone insertion
/// point (group 12), so block geometry is shifted by it before any comparison.
/// Root arrowheads overdrawn on a native terminator and root lines coinciding
/// with block lines join the same complete expression as shared members.
fn native_dimensions(
    cx: &Ctx,
    b: &mut Builder,
    text_anchor: &mut BTreeMap<usize, (P, f64)>,
    native_texts: &mut BTreeMap<usize, TextFact>,
) {
    for (h, r) in cx.rec.iter().filter(|(_, r)| r.kind == "DIMENSION") {
        let dtype = int(r, 70) & 7;
        let off = r.xy(12).unwrap_or([0., 0.]);
        let block = r.get(2).unwrap_or("");
        let brecs: Vec<&Record> =
            cx.d.records
                .iter()
                .filter(|x| {
                    x.section == "BLOCKS"
                        && x.block == block
                        && !matches!(x.kind.as_str(), "BLOCK" | "ENDBLK")
                })
                .collect();
        let measured = measurement(r, dtype, &brecs);
        let block_text = brecs
            .iter()
            .filter(|x| matches!(x.kind.as_str(), "MTEXT" | "TEXT"))
            .map(|x| dd::raw_text(x))
            .collect::<Vec<_>>()
            .join(" ");
        let label_raw = match r.get(1).filter(|s| !s.trim().is_empty()) {
            Some(o) => o.replace("<>", &measured.clone().unwrap_or_default()),
            None if !block_text.trim().is_empty() => block_text.clone(),
            None => measured.clone().unwrap_or_default(),
        };
        let dec = tx::decode(&label_raw);
        let text_kind = tx::dimension_kind(&dec.text);
        let native_kind = match dtype {
            2 | 5 => Some("dimension_angle"),
            3 => Some("dimension_diameter"),
            4 => Some("dimension_radius"),
            0 | 1 | 6 => Some("dimension_linear"),
            _ => None,
        };
        let kind = match (text_kind, native_kind) {
            (Some(k), _) if k != "dimension_linear" => k,
            (_, Some(n)) => n,
            (Some(k), None) => k,
            _ => "dimension_linear",
        };
        let mut ev = vec![
            format!("source:{h}"),
            format!("native_dimension_type:{dtype}"),
            format!("display_text:{}", dec.text),
        ];
        if Some(kind) != native_kind {
            ev.push("display_text_semantics_override_native_enum".into());
        }
        if off != [0., 0.] {
            ev.push(format!("block_offset_group12:{:.4},{:.4}", off[0], off[1]));
        }
        let e = b.expr(
            kind,
            &dec.text,
            "native_dimension_display_text_and_construction",
            "candidate",
            tx::qualifiers(&dec),
            ev,
            vec!["Measured part feature requires projection evidence".into()],
        );
        b.member(
            e,
            h,
            None,
            [0., 1.],
            "native_dimension",
            "dimension",
            "observed",
            vec![],
            vec![format!("source:{h}")],
        );
        let u = match dtype {
            1 => r
                .xy(13)
                .ok()
                .zip(r.xy(14).ok())
                .map(|(a, q)| unit(sub(q, a))),
            0 => {
                let a = r.number(50).unwrap_or(0.).to_radians();
                Some([a.cos(), a.sin()])
            }
            _ => None,
        };
        let mut tips: Vec<Arrow> = Vec::new();
        let mut text_pos = Vec::new();
        let mut block_lines: Vec<(P, P, &'static str)> = Vec::new();
        for m in cx
            .src
            .members
            .iter()
            .filter(|m| m.block_path.len() == 2 && m.block_path[0] == *h)
        {
            let Some(x) = cx.block_rec.get(&m.handle) else {
                continue;
            };
            let role = match x.kind.as_str() {
                "LINE" => match (x.xy(10), x.xy(11)) {
                    (Ok(p), Ok(q)) => {
                        let v = unit(sub(q, p));
                        let role = match u {
                            Some(u) if cross(u, v).abs() < 0.05 => "dimension_line",
                            Some(u) if dot(u, v).abs() < 0.05 => "extension_line",
                            Some(_) => "leader_line",
                            None => "dimension_line",
                        };
                        block_lines.push((add(p, off), add(q, off), role));
                        role
                    }
                    _ => "dimension_line",
                },
                "ARC" => "dimension_line",
                "SOLID" | "TRACE" => {
                    if let Some(a) = arrow_of(x) {
                        tips.push(Arrow {
                            tip: add(a.tip, off),
                            ..a
                        });
                    }
                    "arrowhead"
                }
                "INSERT" => "arrowhead",
                "MTEXT" | "TEXT" => {
                    if let Some(t) = text_fact(x) {
                        let c = add(t.center, off);
                        text_pos.push((c, t.height));
                        native_texts.entry(e).or_insert(TextFact { center: c, ..t });
                    }
                    let t = tx::decode(&dd::raw_text(x));
                    if t.text.starts_with(['+', '-'])
                        || (t.main.trim().is_empty() && t.upper.is_some())
                    {
                        "tolerance_text"
                    } else {
                        "dimension_text"
                    }
                }
                "POINT" => "definition_point",
                _ => "native_member",
            };
            b.member(
                e,
                h,
                Some(m.id.clone()),
                [0., 1.],
                role,
                "dimension",
                "observed",
                vec![],
                vec![format!("native_member:{}", m.id)],
            );
        }
        // Angular and other non-linear dimensions: extension lines are the radial block lines.
        if u.is_none() {
            for bl in &mut block_lines {
                if matches!(dtype, 2 | 5) {
                    bl.2 = "extension_line";
                }
            }
        }
        if let Some(p) = text_pos.first() {
            text_anchor.insert(e, *p);
        }
        for (aid, a) in &cx.arrows {
            if tips
                .iter()
                .any(|t| dist(t.tip, a.tip) <= 0.08 * t.len.max(a.len) && dot(t.dir, a.dir) > 0.95)
            {
                b.member(
                    e,
                    aid,
                    None,
                    [0., 1.],
                    "arrowhead",
                    "fill",
                    "candidate",
                    vec![],
                    vec![format!("coincident_with_native_terminator:{h}")],
                );
            }
        }
        let tol = 1e-3 * cx.scale.max(1.);
        // A root copy drawn over the native construction may sit a fraction of a
        // terminator off it (the overdrawn terminators use the same scale).
        let blen = tips.iter().map(|t| t.len).fold(0., f64::max);
        let ltol = if blen > 0. { tol.max(0.03 * blen) } else { tol };
        for (lid, (a, c)) in &cx.lines {
            for (p, q, role) in &block_lines {
                let l = dist(*p, *q);
                if l < 1e-9
                    || cross(unit(sub(*c, *a)), unit(sub(*q, *p))).abs() > 1e-4
                    || project(*a, *p, *q).1 > ltol
                    || project(*c, *p, *q).1 > ltol
                {
                    continue;
                }
                let (t0, t1) = (project(*p, *a, *c).0, project(*q, *a, *c).0);
                let iv = [t0.min(t1).max(0.), t0.max(t1).min(1.)];
                if (iv[1] - iv[0]) * dist(*a, *c) > tol {
                    b.member(
                        e,
                        lid,
                        None,
                        iv,
                        role,
                        member_line_type(cx.lt(lid), role),
                        "candidate",
                        vec![],
                        vec![format!("coincident_with_native_block_line:{h}")],
                    );
                }
            }
        }
        for us in cx
            .src
            .uses
            .iter()
            .filter(|u| u.consumer_id == *h && u.rule.starts_with("linear_"))
        {
            let role = if us.rule == "linear_dimension_line_support" {
                "dimension_line"
            } else {
                "extension_line"
            };
            b.member(
                e,
                &us.occurrence_id,
                None,
                [us.interval[0], us.interval[1]],
                role,
                member_line_type(cx.lt(&us.occurrence_id), role),
                "candidate",
                us.relation_ids.clone(),
                vec![format!("rule:{}", us.rule)],
            );
        }
        // Angular dimensions: root reference lines drawn on the two measured sides.
        let sides: Vec<(P, P)> = match dtype {
            2 => [(13, 14), (15, 10)]
                .iter()
                .filter_map(|(i, j)| r.xy(*i).ok().zip(r.xy(*j).ok()))
                .collect(),
            5 => [(15, 13), (15, 14)]
                .iter()
                .filter_map(|(i, j)| r.xy(*i).ok().zip(r.xy(*j).ok()))
                .collect(),
            _ => vec![],
        };
        for (p, q) in sides {
            let l = dist(p, q);
            if l < 1e-9 {
                continue;
            }
            let v = unit(sub(q, p));
            for (lid, (a, c)) in &cx.lines {
                if !cx.annotation_style(lid)
                    || cross(unit(sub(*c, *a)), v).abs() > 1e-3
                    || project(*a, p, q).1 > tol
                    || project(*c, p, q).1 > tol
                {
                    continue;
                }
                let (t0, t1) = (dot(sub(*a, p), v), dot(sub(*c, p), v));
                if t0.max(t1) < tol || t0.min(t1) > l - tol {
                    continue;
                }
                b.member(
                    e,
                    lid,
                    None,
                    [0., 1.],
                    "extension_line",
                    member_line_type(cx.lt(lid), "extension_line"),
                    "candidate",
                    vec![],
                    vec![format!("on_native_angle_side:{h}")],
                );
            }
        }
        // Linear dimensions: root lines on a native extension line (from the origin
        // through the dimension line with its overshoot) and their collinear pieces.
        if let (Some(u), Ok(p10)) = (u, r.xy(10)) {
            let alen = tips.iter().map(|t| t.len).fold(0.6 * cx.scale, f64::max);
            let v = [-u[1], u[0]];
            let origins: Vec<(P, P, f64)> = [13, 14]
                .iter()
                .filter_map(|oc| r.xy(*oc).ok())
                .map(|o| {
                    let t = add(p10, mul(u, dot(sub(o, p10), u)));
                    (o, t, dot(sub(o, t), v))
                })
                .collect();
            // Side of the features relative to the dimension line (from either origin).
            let side = origins
                .iter()
                .map(|x| x.2)
                .find(|so| so.abs() > tol)
                .map(f64::signum);
            for (_, t, so) in &origins {
                let iv = [so.min(0.) - 2. * alen, so.max(0.) + 2. * alen];
                // Each extension line ends a short overshoot past its own dimension
                // line; pieces of stacked dimensions beyond it are not members.
                let far = side.map(|sg| mul(v, -sg));
                for lid in collinear_chain(cx, b, *t, v, iv, tol, &BTreeSet::new(), &[], false) {
                    let (a, c) = cx.lines[&lid];
                    let clipped = match far {
                        Some(f) => clip_along(a, c, *t, f, 2. * alen, tol),
                        None => Some([0., 1.]),
                    };
                    if let Some(civ) = clipped {
                        b.member(
                            e,
                            &lid,
                            None,
                            civ,
                            "extension_line",
                            member_line_type(cx.lt(&lid), "extension_line"),
                            "candidate",
                            vec![],
                            vec![format!("on_native_extension_line:{h}")],
                        );
                    }
                }
            }
        }
    }
}

/// Member roles that consume a source exclusively for annotation; such sources
/// are not visible-outline evidence.
const EXCLUSIVE: [&str; 16] = [
    "datum_terminal",
    "hatch_fill",
    "dimension_line",
    "leader_line",
    "frame_cell",
    "table_line",
    "frame_border",
    "view_arrow",
    "detail_circle",
    "roughness_symbol",
    "datum_frame",
    "symbol_glyph",
    "native_block",
    "break_curve",
    "thread_line",
    "view_title",
];
fn exclusive(b: &Builder, id: &str) -> bool {
    b.by_source.get(id).is_some_and(|ms| {
        ms.iter()
            .any(|&m| EXCLUSIVE.contains(&b.members[m].member_role.as_str()))
    })
}

/// Annotation LINE roots on the infinite line through `o` along unit `v` (within
/// `ptol`) that overlap or touch the parameter interval `iv`, grown transitively.
/// A piece that would carry the chain past one of the `stops` (another
/// terminator) is not taken.
#[allow(clippy::too_many_arguments)]
fn collinear_chain(
    cx: &Ctx,
    b: &Builder,
    o: P,
    v: P,
    iv: [f64; 2],
    ptol: f64,
    skip: &BTreeSet<String>,
    stops: &[P],
    free_only: bool,
) -> Vec<String> {
    let gap = 1e-3 * cx.scale.max(1.);
    let ov = add(o, v);
    let mut cover = iv;
    let mut out: Vec<String> = Vec::new();
    loop {
        let mut grew = false;
        for (id, (a, c)) in &cx.lines {
            if skip.contains(id)
                || out.contains(id)
                || (free_only && b.used(id))
                || exclusive(b, id)
                || !cx.annotation_style(id)
            {
                continue;
            }
            // Drafted pieces may be slightly skewed: one end on the chain line, the
            // other within a small angular deviation (drafting imprecision).
            let skew = 0.01;
            let (da, dc) = (project(*a, o, ov).1, project(*c, o, ov).1);
            if dist(*a, *c) < 1e-9
                || cross(unit(sub(*c, *a)), v).abs() > skew
                || da.min(dc) > ptol
                || da.max(dc) > ptol + skew * dist(*a, *c)
            {
                continue;
            }
            let (ta, tc) = (dot(sub(*a, o), v), dot(sub(*c, o), v));
            let (lo, hi) = (ta.min(tc), ta.max(tc));
            if hi < cover[0] - gap || lo > cover[1] + gap {
                continue;
            }
            let passes_stop = stops.iter().any(|s| {
                let t = dot(sub(*s, o), v);
                project(*s, o, ov).1 < ptol.max(gap)
                    && t >= lo - gap
                    && t <= hi + gap
                    && (t < cover[0] - gap || t > cover[1] + gap)
            });
            if passes_stop {
                continue;
            }
            out.push(id.clone());
            cover = [cover[0].min(lo), cover[1].max(hi)];
            grew = true;
        }
        if !grew {
            break;
        }
    }
    out
}

/// Extension line at a terminator: a line across the dimension direction `u`
/// through `tip`, which it passes by a small amount (GB/T 4458.4), completed by
/// collinear annotation pieces. A structural line in that place is the contour
/// used as extension line; it is shared, not consumed.
fn extension_at(
    cx: &Ctx,
    b: &mut Builder,
    e: usize,
    tip: P,
    u: P,
    alen: f64,
    exclude: &BTreeSet<String>,
) -> Vec<P> {
    let tol = 1e-3 * cx.scale.max(1.);
    let found = cx
        .lines
        .iter()
        .filter(|(id, (a, c))| {
            !exclude.contains(*id)
                && !exclusive(b, id)
                && dist(*a, *c) > 1e-9
                && dot(unit(sub(*c, *a)), u).abs() < 0.03
                && on_seg(tip, *a, *c, 0.12 * alen).is_some()
                && dist(tip, *a).min(dist(tip, *c)) <= 6. * alen
        })
        .min_by(|x, y| {
            project(tip, x.1.0, x.1.1)
                .1
                .total_cmp(&project(tip, y.1.0, y.1.1).1)
        })
        .map(|(id, l)| (id.clone(), *l));
    let Some((id, (a, c))) = found else {
        return vec![];
    };
    let mut pts = vec![a, c];
    if cx.structural_style(&id) {
        b.member(
            e,
            &id,
            None,
            [0., 1.],
            "extension_origin",
            cx.lt(&id),
            "candidate",
            vec![],
            vec!["contour_used_as_extension_line".into()],
        );
        return pts;
    }
    b.member(
        e,
        &id,
        None,
        [0., 1.],
        "extension_line",
        member_line_type(cx.lt(&id), "extension_line"),
        "candidate",
        vec![],
        vec!["perpendicular_through_terminator_with_overshoot".into()],
    );
    let mut skip = exclude.clone();
    skip.insert(id.clone());
    // `f` runs from the origin side through the terminator; pieces end a short
    // overshoot past it.
    let (near, far_end) = if dist(tip, a) <= dist(tip, c) {
        (a, c)
    } else {
        (c, a)
    };
    let f = unit(sub(near, far_end));
    let limit = dot(sub(near, tip), f).max(2.5 * alen);
    let mut low = dot(sub(far_end, tip), f);
    let mut origin = far_end;
    for cid in collinear_chain(
        cx,
        b,
        a,
        unit(sub(c, a)),
        [0., dist(a, c)],
        tol,
        &skip,
        &[],
        false,
    ) {
        let (p, q) = cx.lines[&cid];
        let Some(iv) = clip_along(p, q, tip, f, limit, tol) else {
            continue;
        };
        pts.extend([lerp(p, q, iv[0]), lerp(p, q, iv[1])]);
        for x in [p, q] {
            let t = dot(sub(x, tip), f);
            if t < low {
                low = t;
                origin = x;
            }
        }
        b.member(
            e,
            &cid,
            None,
            iv,
            "extension_line",
            member_line_type(cx.lt(&cid), "extension_line"),
            "candidate",
            vec![],
            vec!["collinear_piece_of_extension_line".into()],
        );
    }
    // The measured feature: a contour line the extension line continues, or one
    // ending at a fillet where the extension line starts from the virtual sharp
    // corner (GB/T 4458.4 smooth transition). Otherwise it stays unknown.
    let continued = cx
        .lines
        .iter()
        .find(|(oid, (p, q))| {
            **oid != id
                && cx.structural_style(oid)
                && !exclusive(b, oid)
                && cross(unit(sub(*q, *p)), f).abs() < 0.01
                && (dist(*p, origin) < tol || dist(*q, origin) < tol)
        })
        .map(|x| (x.0.clone(), "contour_continued_by_extension_line"));
    let edge = continued.or_else(|| fillet_corner(cx, b, &id, origin, f, tol));
    if let Some((oid, why)) = edge {
        b.member(
            e,
            &oid,
            None,
            [0., 1.],
            "extension_origin",
            cx.lt(&oid),
            "candidate",
            vec![],
            vec![why.into()],
        );
    }
    pts
}

/// A structural line on the extension line's carrier that stops short of
/// `origin` where a structural fillet arc turns into a structural line whose
/// carrier passes through `origin`: the extension line starts from the virtual
/// sharp corner of the two outlines (GB/T 4458.4 smooth transition).
fn fillet_corner(
    cx: &Ctx,
    b: &Builder,
    ext: &str,
    origin: P,
    f: P,
    tol: f64,
) -> Option<(String, &'static str)> {
    let arcs: Vec<(P, P, f64)> = cx
        .rec
        .keys()
        .filter(|h| cx.kind(h) == "ARC" && cx.structural_style(h) && !exclusive(b, h))
        .filter_map(|h| cx.arc(h))
        .map(|(c, r, s0, sw, _)| {
            (
                add(c, mul([s0.cos(), s0.sin()], r)),
                add(c, mul([(s0 + sw).cos(), (s0 + sw).sin()], r)),
                r,
            )
        })
        .collect();
    for (oid, (p, q)) in &cx.lines {
        if oid == ext
            || !cx.structural_style(oid)
            || exclusive(b, oid)
            || cross(unit(sub(*q, *p)), f).abs() >= 0.01
            || project(origin, *p, *q).1 > tol
            || on_seg(origin, *p, *q, tol).is_some()
        {
            continue;
        }
        let e1 = if dist(*p, origin) <= dist(*q, origin) {
            *p
        } else {
            *q
        };
        if dot(sub(e1, origin), f) > -tol {
            continue;
        }
        for (a0, a1, r) in &arcs {
            let turn = if dist(*a0, e1) < tol {
                *a1
            } else if dist(*a1, e1) < tol {
                *a0
            } else {
                continue;
            };
            if dist(e1, origin) > 2. * r + tol {
                continue;
            }
            if cx.lines.iter().any(|(lid, (u0, u1))| {
                lid != oid
                    && lid != ext
                    && cx.structural_style(lid)
                    && on_seg(turn, *u0, *u1, tol).is_some()
                    && project(origin, *u0, *u1).1 <= tol
            }) {
                return Some((oid.clone(), "contour_extended_to_virtual_corner_of_fillet"));
            }
        }
    }
    None
}

/// Far silhouette of a diameter drawn with one terminator: where the dimension
/// line ends, a structural line parallel to the near origin (same along-span)
/// is the other measured outline (GB/T 4458.4 implied extension).
fn far_silhouette(
    cx: &Ctx,
    b: &Builder,
    far: P,
    dir: P,
    alen: f64,
    near: &[String],
) -> Option<String> {
    let reach = (0.25 * alen).max(1e-3 * cx.scale.max(1.));
    let spans: Vec<(P, P)> = near
        .iter()
        .filter_map(|n| cx.lines.get(n).copied())
        .collect();
    if spans.is_empty() {
        return None;
    }
    cx.lines
        .iter()
        .filter(|(id, (p, q))| {
            !near.contains(*id)
                && cx.structural_style(id)
                && !exclusive(b, id)
                && dot(unit(sub(*q, *p)), dir).abs() < 0.03
                && project(far, *p, *q).1 <= reach
        })
        .filter_map(|(id, (p, q))| {
            let w = unit(sub(*q, *p));
            let (t0, t1) = (dot(*p, w).min(dot(*q, w)), dot(*p, w).max(dot(*q, w)));
            let overlap = spans
                .iter()
                .map(|(a, c)| {
                    (t1.min(dot(*a, w).max(dot(*c, w))) - t0.max(dot(*a, w).min(dot(*c, w))))
                        .max(0.)
                })
                .fold(0., f64::max);
            (overlap > 0.5 * dist(*p, *q)).then(|| (id.clone(), overlap))
        })
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .map(|x| x.0)
}

/// Parameter interval of segment `ac` whose points lie at most `limit` along unit
/// `f` measured from `o`; None when less than `tol` of it remains.
fn clip_along(a: P, c: P, o: P, f: P, limit: f64, tol: f64) -> Option<[f64; 2]> {
    let (ta, tc) = (dot(sub(a, o), f), dot(sub(c, o), f));
    if (tc - ta).abs() < 1e-12 {
        return (ta <= limit + tol).then_some([0., 1.]);
    }
    let ul = (limit - ta) / (tc - ta);
    let iv = if tc > ta {
        [0., ul.min(1.)]
    } else {
        [ul.max(0.), 1.]
    };
    (iv[1] - iv[0] > 0. && (iv[1] - iv[0]) * dist(a, c) > tol).then_some(iv)
}

/// Segment `pq` meets segment `ac` (within `tol`).
fn seg_hit(p: P, q: P, a: P, c: P, tol: f64) -> bool {
    let (r, s) = (sub(q, p), sub(c, a));
    let den = cross(r, s);
    if den.abs() < 1e-12 {
        return on_seg(p, a, c, tol).is_some() || on_seg(q, a, c, tol).is_some();
    }
    let t = cross(sub(a, p), s) / den;
    let w = cross(sub(a, p), r) / den;
    let (lr, ls) = (norm(r).max(1e-12), norm(s).max(1e-12));
    t >= -tol / lr && t <= 1. + tol / lr && w >= -tol / ls && w <= 1. + tol / ls
}

/// The ray from `p` along unit `dir` meets a structural line or arc within `reach`.
fn ray_hits_structure(cx: &Ctx, b: &Builder, p: P, dir: P, reach: f64) -> bool {
    let tol = 1e-3 * cx.scale.max(1.);
    let q = add(p, mul(dir, reach));
    let lines = cx.lines.iter().any(|(id, (a, c))| {
        cx.structural_style(id) && !exclusive(b, id) && seg_hit(p, q, *a, *c, tol)
    });
    lines
        || cx.rec.keys().any(|id| {
            matches!(cx.kind(id), "ARC" | "CIRCLE")
                && cx.structural_style(id)
                && !exclusive(b, id)
                && cx.arc(id).is_some_and(|(c, r, s0, sw, full)| {
                    let w = sub(p, c);
                    let (bb, cc) = (dot(dir, w), dot(w, w) - r * r);
                    let disc = bb * bb - cc;
                    disc >= 0.
                        && [-bb - disc.sqrt(), -bb + disc.sqrt()].iter().any(|t| {
                            let x = add(p, mul(dir, *t));
                            let ang = (x[1] - c[1]).atan2(x[0] - c[0]) - s0;
                            *t >= -tol
                                && *t <= reach + tol
                                && (full || ang.rem_euclid(std::f64::consts::TAU) <= sw + 1e-9)
                        })
                })
        })
}

// ---------------------------------------------------------------- symbol blocks

/// Tolerance-characteristic glyph from symbol geometry only (GB/T 1182 symbols).
fn glyph_kind(s: &dd::Shape) -> Option<&'static str> {
    if !s.texts.is_empty() {
        return None;
    }
    let lines: Vec<(P, P)> = s
        .lines
        .iter()
        .copied()
        .filter(|(a, b)| dist(*a, *b) > 1e-9)
        .collect();
    let lens: Vec<f64> = lines.iter().map(|(a, b)| dist(*a, *b)).collect();
    let maxl = lens.iter().copied().fold(0., f64::max);
    if s.circles.len() == 2
        && lines.is_empty()
        && dist(s.circles[0].0, s.circles[1].0) < 0.05 * s.circles[0].1.max(s.circles[1].1)
    {
        return Some("geometric_tolerance");
    }
    if s.circles.len() == 1 && lines.len() == 2 {
        let (c, r) = s.circles[0];
        let (u, v) = (
            unit(sub(lines[0].1, lines[0].0)),
            unit(sub(lines[1].1, lines[1].0)),
        );
        if dot(u, v).abs() < 0.05
            && lines
                .iter()
                .all(|(a, b)| dist(mid(*a, *b), c) < 0.15 * r && dist(*a, *b) > r)
        {
            return Some("gps_position");
        }
    }
    if !s.circles.is_empty() {
        return None;
    }
    if lines.len() == 2 {
        let (a, b) = lines[0];
        let (c, d) = lines[1];
        let (u, v) = (unit(sub(b, a)), unit(sub(d, c)));
        if dot(u, v).abs() < 0.05 {
            let touches = |p: P, q: (P, P)| dist(p, mid(q.0, q.1)) < 0.08 * dist(q.0, q.1);
            if touches(c, (a, b)) || touches(d, (a, b)) || touches(a, (c, d)) || touches(b, (c, d))
            {
                return Some("gps_perpendicularity");
            }
        }
        if dot(u, v).abs() > 0.995 && project(c, a, b).1 > 0.1 * maxl {
            return Some("gps_parallelism");
        }
    }
    if lines.len() == 3 {
        let u = unit(sub(lines[0].1, lines[0].0));
        if lines
            .iter()
            .all(|(a, b)| cross(u, unit(sub(*b, *a))).abs() < 0.02)
            && lens.iter().copied().fold(f64::INFINITY, f64::min) < 0.85 * maxl
        {
            return Some("gps_symmetry");
        }
    }
    // Runout: one long oblique line whose end carries a triangular arrowhead
    // (a closed three-vertex path) pointing along the line.
    if lines.len() >= 2 {
        let (i, &l) = lens.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
        let (a, c) = lines[i];
        let u = unit(sub(c, a));
        let oblique = u[0].abs() > 0.2 && u[1].abs() > 0.2;
        let triangle = s.paths.iter().find_map(|p| {
            let mut q: Vec<P> = Vec::new();
            for v in p {
                if q.last().is_none_or(|w| dist(*w, *v) > 1e-9) {
                    q.push(*v);
                }
            }
            if q.len() == 4 && dist(q[0], q[3]) < 1e-9 {
                q.pop();
            }
            (q.len() == 3).then_some(q)
        });
        if let (true, Some(t)) = (oblique, triangle) {
            let centroid = [
                (t[0][0] + t[1][0] + t[2][0]) / 3.,
                (t[0][1] + t[1][1] + t[2][1]) / 3.,
            ];
            for (tip, other) in [(a, c), (c, a)] {
                if t.iter().any(|v| dist(*v, tip) < 0.05 * l)
                    && dist(centroid, tip) < 0.8 * l
                    && dot(unit(sub(centroid, tip)), unit(sub(other, tip))) > 0.7
                {
                    return Some("gps_runout");
                }
            }
        }
    }
    None
}

/// Complete tolerance frame inside one block: adjacent rectangular cells, glyph
/// lines strictly inside the first cell and value/datum fields in later cells.
#[allow(clippy::type_complexity)]
fn frame_of(s: &dd::Shape) -> Option<(&'static str, Vec<[f64; 4]>, Vec<String>)> {
    let mut cells: Vec<[f64; 4]> = s.paths.iter().filter_map(|p| cell_rect(p)).collect();
    cells.sort_by(|a, b| a[0].total_cmp(&b[0]));
    cells.dedup_by(|a, b| (0..4).all(|i| (a[i] - b[i]).abs() < 1e-6));
    if cells.len() < 2 {
        return None;
    }
    let h = cells[0][3] - cells[0][1];
    let tol = 0.05 * h;
    let mut chain = vec![cells[0]];
    for c in cells.iter().skip(1) {
        let last = *chain.last().unwrap();
        if (c[0] - last[2]).abs() < tol
            && (c[1] - last[1]).abs() < tol
            && (c[3] - last[3]).abs() < tol
        {
            chain.push(*c);
        }
    }
    if chain.len() < 2 {
        return None;
    }
    let first = chain[0];
    let shrink = [
        first[0] + tol,
        first[1] + tol,
        first[2] - tol,
        first[3] - tol,
    ];
    let glyph = dd::Shape {
        lines: s
            .lines
            .iter()
            .copied()
            .filter(|(a, b)| inside(*a, shrink, 0.) && inside(*b, shrink, 0.))
            .collect(),
        circles: s
            .circles
            .iter()
            .copied()
            .filter(|(c, _)| inside(*c, shrink, 0.))
            .collect(),
        texts: vec![],
        paths: vec![],
        curved: false,
    };
    let kind = glyph_kind(&glyph)?;
    let fields = chain[1..]
        .iter()
        .map(|c| {
            s.texts
                .iter()
                .filter(|t| inside(t.1, *c, tol))
                .map(|t| t.0.clone())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    Some((kind, chain, fields))
}

#[derive(Default)]
struct Deferred {
    glyphs: Vec<(String, &'static str, [f64; 4])>,
    cells: Vec<(Vec<String>, [f64; 4])>,
    boxes: Vec<String>,
    titles: Vec<(usize, [f64; 4])>,
    roughness: Vec<(usize, String)>,
}

fn blocks(cx: &Ctx, b: &mut Builder, def: &mut Deferred) {
    let area = (cx.extent[2] - cx.extent[0]).max(1e-9) * (cx.extent[3] - cx.extent[1]).max(1e-9);
    for (h, s) in &cx.shapes {
        let txt = s
            .texts
            .iter()
            .map(|t| t.0.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let compact: String = txt.chars().filter(|c| !c.is_whitespace()).collect();
        let words = tx::TITLE_WORDS
            .iter()
            .filter(|w| compact.contains(&w.replace(' ', "")))
            .count();
        let registration = ["用件登记", "描图", "描校", "底图总号"]
            .iter()
            .filter(|w| compact.contains(**w))
            .count()
            >= 2;
        let rects: Vec<[f64; 4]> = s.paths.iter().filter_map(|p| cell_rect(p)).collect();
        let ev = vec![
            format!("source:{h}"),
            "whole_native_block_shape_and_fields".into(),
        ];
        if dd::rough(s) {
            let value = s
                .texts
                .iter()
                .filter(|t| tx::lex(&t.0) == Lex::Number || tx::lex(&t.0) == Lex::ToleranceValue)
                .map(|t| t.0.clone())
                .collect::<Vec<_>>()
                .join(" ");
            let general = txt.contains("其余");
            let e = b.expr(
                "surface_roughness",
                &if general {
                    format!("其余 {value}")
                } else {
                    value.clone()
                },
                "basic_texture_symbol_with_value",
                "candidate",
                if general {
                    vec!["general_requirement".into()]
                } else {
                    vec![]
                },
                ev,
                vec!["Controlled surface requires the indicated feature".into()],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "roughness_symbol",
                "block",
                "candidate",
                vec![],
                vec![],
            );
            def.roughness.push((e, h.clone()));
        } else if dd::datum(s) {
            let e = b.expr(
                "datum_symbol",
                s.texts.first().map_or("", |t| t.0.as_str()),
                "circled_letter_with_connector",
                "candidate",
                vec![],
                ev,
                vec!["Datum feature requires the attached feature".into()],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "datum_frame",
                "block",
                "candidate",
                vec![],
                vec![],
            );
        } else if let Some((kind, cells, fields)) = frame_of(s) {
            let label = format!("{} {}", glyph_label(kind), fields.join(" | "));
            let e = b.expr(
                kind,
                &label,
                "tolerance_frame_block_glyph_and_fields",
                "candidate",
                vec![],
                ev,
                vec!["Toleranced feature requires the leader target".into()],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "native_block",
                "block",
                "candidate",
                vec![],
                vec![format!("cells:{}", cells.len())],
            );
        } else if registration && s.lines.len() >= 8 {
            // Registration panel in the binding margin belongs to the sheet frame layout.
            let e = b.expr(
                "sheet_frame",
                "借（通）用件登记栏",
                "binding_margin_registration_table",
                "candidate",
                vec![],
                ev,
                vec![],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "frame_border",
                "block",
                "candidate",
                vec![],
                vec![format!("registration_words:{words}")],
            );
        } else if words >= 2 && s.lines.len() >= 8 {
            let e = b.expr(
                "title_block",
                "标题栏",
                "table_grid_with_title_vocabulary",
                "candidate",
                vec![],
                ev,
                vec![],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "native_block",
                "block",
                "candidate",
                vec![],
                vec![format!("title_words:{words}")],
            );
            if let Some(bb) = cx.bbox(h) {
                def.titles.push((e, bb));
            }
        } else if s.texts.is_empty()
            && rects
                .iter()
                .any(|r| (r[2] - r[0]) * (r[3] - r[1]) >= 0.25 * area)
        {
            let e = b.expr(
                "sheet_frame",
                "图框",
                "sheet_size_rectangular_border",
                "candidate",
                vec![],
                ev,
                vec![],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "frame_border",
                "block",
                "candidate",
                vec![],
                vec![],
            );
        } else if let Some(kind) = glyph_kind(s) {
            if let Some(bb) = cx.bbox(h) {
                def.glyphs.push((h.clone(), kind, bb));
            }
        } else if s.texts.is_empty() && rects.len() == 1 && s.lines.len() <= 5 {
            def.cells.push((vec![h.clone()], rects[0]));
        } else if s.texts.is_empty() && s.lines.len() <= 6 {
            def.boxes.push(h.clone());
        }
    }
}

fn glyph_label(kind: &str) -> &'static str {
    match kind {
        "gps_position" => "⌖",
        "gps_symmetry" => "⌯",
        "gps_runout" => "↗",
        "gps_perpendicularity" => "⊥",
        "gps_parallelism" => "∥",
        "gps_direction" => "⊥/∥",
        _ => "GPS",
    }
}

fn hatch_and_points(cx: &Ctx, b: &mut Builder) {
    for (h, r) in &cx.rec {
        match r.kind.as_str() {
            "HATCH" => {
                let e = b.expr(
                    "section_hatch",
                    r.get(2).unwrap_or("HATCH"),
                    "native_hatch_pattern",
                    "observed",
                    vec![],
                    vec![format!("source:{h}")],
                    vec!["Sectioned material region and cutting plane remain unresolved".into()],
                );
                b.member(
                    e,
                    h,
                    None,
                    [0., 1.],
                    "hatch_fill",
                    "fill",
                    "observed",
                    vec![],
                    vec![],
                );
            }
            "POINT" => {
                let e = b.expr(
                    "construction_reference",
                    "定义点",
                    "native_point_not_printed_construction",
                    "observed",
                    vec![],
                    vec![format!("source:{h}")],
                    vec![],
                );
                b.member(
                    e,
                    h,
                    None,
                    [0., 1.],
                    "definition_point",
                    "point",
                    "observed",
                    vec![],
                    vec![format!("layer:{}", cx.style[h].layer)],
                );
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- exploded annotation assembly

fn area(r: [f64; 4]) -> f64 {
    (r[2] - r[0]).max(0.) * (r[3] - r[1]).max(0.)
}

/// Rectangles drawn with four separate axis-aligned thin LINE roots.
fn line_rectangles(cx: &Ctx, b: &Builder) -> Vec<(Vec<String>, [f64; 4])> {
    let mut hs = Vec::new();
    let mut vs = Vec::new();
    for (id, (a, c)) in &cx.lines {
        let l = dist(*a, *c);
        if b.used(id) || l < 1e-6 || l > 12. * cx.scale || !cx.annotation_style(id) {
            continue;
        }
        if (a[1] - c[1]).abs() < 1e-6 * l.max(1.) {
            hs.push((id.clone(), a[0].min(c[0]), a[0].max(c[0]), a[1]));
        } else if (a[0] - c[0]).abs() < 1e-6 * l.max(1.) {
            vs.push((id.clone(), a[1].min(c[1]), a[1].max(c[1]), a[0]));
        }
    }
    let mut out = Vec::new();
    for (i, h1) in hs.iter().enumerate() {
        for h2 in hs.iter().skip(i + 1) {
            let tol = 1e-4 * (h1.2 - h1.1).max(1e-9);
            if (h1.1 - h2.1).abs() > tol || (h1.2 - h2.2).abs() > tol || (h1.3 - h2.3).abs() < 1e-6
            {
                continue;
            }
            let (ylo, yhi) = (h1.3.min(h2.3), h1.3.max(h2.3));
            let side = |x: f64| {
                vs.iter().find(|v| {
                    (v.3 - x).abs() < tol.max(1e-6)
                        && (v.1 - ylo).abs() < tol.max(1e-6)
                        && (v.2 - yhi).abs() < tol.max(1e-6)
                })
            };
            if let (Some(v1), Some(v2)) = (side(h1.1), side(h1.2)) {
                out.push((
                    vec![h1.0.clone(), h2.0.clone(), v1.0.clone(), v2.0.clone()],
                    [h1.1, ylo, h1.2, yhi],
                ));
            }
        }
    }
    out
}

/// Free annotation segments: thin LINE roots and open, unbulged polylines of at
/// most three vertices (longer polylines are wavy lines or outlines, not leaders).
fn free_segments(cx: &Ctx, b: &Builder) -> Vec<(String, Vec<P>)> {
    let mut v: Vec<(String, Vec<P>)> = cx
        .lines
        .iter()
        .filter(|(id, _)| !b.used(id) && cx.annotation_style(id))
        .map(|(id, (a, c))| (id.clone(), vec![*a, *c]))
        .collect();
    for (id, (pts, closed, bulged)) in &cx.polys {
        if !b.used(id)
            && !*closed
            && !*bulged
            && (2..=3).contains(&pts.len())
            && !cx.arrows.contains_key(id)
            && cx.annotation_style(id)
        {
            v.push((id.clone(), pts.clone()));
        }
    }
    v
}

/// A structural line passes through `p`: the leader has reached its feature.
fn reaches_structure(cx: &Ctx, b: &Builder, p: P, taken: &BTreeSet<String>) -> bool {
    let tol = 1e-3 * cx.scale.max(1.);
    cx.lines.iter().any(|(id, (a, c))| {
        !taken.contains(id)
            && cx.structural_style(id)
            && !exclusive(b, id)
            && on_seg(p, *a, *c, tol).is_some()
    }) || cx.rec.keys().any(|id| {
        matches!(cx.kind(id), "ARC" | "CIRCLE")
            && cx.structural_style(id)
            && !taken.contains(id)
            && cx
                .arc(id)
                .is_some_and(|(c, r, ..)| (dist(p, c) - r).abs() < tol)
    })
}

/// Breadth-first leader branches from `starts` (point, previous point) through
/// exactly connected free annotation segments; a branch stops where it reaches
/// structure. Arrowheads continuing a branch end (tip or base on it, pointing
/// onward) are collected, including heads drawn inside the leader polyline.
fn branches(
    cx: &Ctx,
    b: &Builder,
    starts: &[(P, P)],
    taken: &mut BTreeSet<String>,
    depth: usize,
) -> (Vec<String>, Vec<String>, Vec<P>) {
    let segs = free_segments(cx, b);
    let tol = 1e-3 * cx.scale.max(1.);
    let mut found = Vec::new();
    let mut arrows: Vec<String> = Vec::new();
    let mut ends: Vec<(P, P)> = Vec::new();
    let mut frontier: Vec<(P, P)> = starts.to_vec();
    for step in 0..depth {
        let mut next = Vec::new();
        for &(p, from) in &frontier {
            if step > 0 && reaches_structure(cx, b, p, taken) {
                ends.push((p, from));
                continue;
            }
            let mut extended = false;
            for (id, pts) in &segs {
                if taken.contains(id) {
                    continue;
                }
                let n = pts.len();
                let other = if dist(pts[0], p) < tol {
                    Some((pts[n - 1], pts[n - 2]))
                } else if dist(pts[n - 1], p) < tol {
                    Some((pts[0], pts[1]))
                } else {
                    None
                };
                if let Some((o, prev)) = other {
                    taken.insert(id.clone());
                    found.push(id.clone());
                    extended = true;
                    if cx
                        .inline_arrows
                        .get(id)
                        .is_some_and(|a| continues(a, o, prev, 0.25 * a.len))
                    {
                        arrows.push(id.clone());
                        ends.push((o, prev));
                    } else {
                        next.push((o, prev));
                    }
                }
            }
            if !extended {
                ends.push((p, from));
            }
        }
        if next.is_empty() {
            frontier.clear();
            break;
        }
        frontier = next;
    }
    ends.extend(frontier);
    for (e, from) in &ends {
        if let Some(id) = cx
            .inline_arrows
            .iter()
            .find(|(id, a)| {
                taken.contains(*id)
                    && !arrows.contains(*id)
                    && continues(a, *e, *from, 0.25 * a.len)
            })
            .map(|x| x.0.clone())
        {
            arrows.push(id);
            continue;
        }
        let near = |a: &Arrow| dist(a.tip, *e).min(dist(base_of(a), *e));
        if let Some((id, _)) = cx
            .arrows
            .iter()
            .filter(|(id, a)| {
                !b.used(id)
                    && !taken.contains(*id)
                    && !arrows.contains(*id)
                    && continues(a, *e, *from, 0.25 * a.len)
            })
            .min_by(|x, y| near(x.1).total_cmp(&near(y.1)))
        {
            arrows.push(id.clone());
        }
    }
    for a in &arrows {
        taken.insert(a.clone());
    }
    (found, arrows, ends.into_iter().map(|x| x.0).collect())
}
fn add_leader(cx: &Ctx, b: &mut Builder, e: usize, segs: &[String], arrows: &[String], rule: &str) {
    for id in segs {
        b.member(
            e,
            id,
            None,
            [0., 1.],
            "leader_line",
            member_line_type(cx.lt(id), "leader_line"),
            "candidate",
            vec![],
            vec![format!("rule:{rule}")],
        );
    }
    for a in arrows {
        b.member(
            e,
            a,
            None,
            [0., 1.],
            "arrowhead",
            "fill",
            "candidate",
            vec![],
            vec![format!("rule:{rule}")],
        );
    }
}

#[allow(clippy::type_complexity)]
fn exploded_frames(
    cx: &Ctx,
    b: &mut Builder,
    def: &Deferred,
    anchors: &mut BTreeMap<usize, P>,
    loose: &[(Vec<String>, &'static str, [f64; 4])],
) {
    let mut cells: Vec<(Vec<String>, [f64; 4])> = def.cells.clone();
    for (h, (pts, _, bulged)) in &cx.polys {
        if !*bulged
            && !b.used(h)
            && let Some(r) = cell_rect(pts)
        {
            cells.push((vec![h.clone()], r));
        }
    }
    cells.extend(line_rectangles(cx, b));
    // Cells drawn partly with lines next to a polyline cell (card gdt_frame_symbol).
    for (ids, r) in representation::frame_rectangles(cx, b) {
        if !cells.iter().any(|c| c.1 == r) {
            cells.push((ids, r));
        }
    }
    let glyphs: Vec<(Vec<String>, &'static str, [f64; 4], bool)> = def
        .glyphs
        .iter()
        .map(|(g, k, r)| (vec![g.clone()], *k, *r, false))
        .chain(loose.iter().map(|(g, k, r)| (g.clone(), *k, *r, true)))
        .collect();
    for (gids, kind, gb, is_loose) in &glyphs {
        if gids.iter().any(|g| b.used(g)) {
            continue;
        }
        let gc = mid([gb[0], gb[1]], [gb[2], gb[3]]);
        let gsize = (gb[2] - gb[0]).max(gb[3] - gb[1]).max(1e-6);
        let Some(first) = cells
            .iter()
            .filter(|(_, r)| {
                inside(gc, *r, 0.) && (r[3] - r[1]) < 4. * gsize && (r[2] - r[0]) < 4. * gsize
            })
            .min_by(|x, y| area(x.1).total_cmp(&area(y.1)))
            .cloned()
        else {
            continue;
        };
        let h = first.1[3] - first.1[1];
        let tol = 0.06 * h;
        let mut chain = vec![first];
        loop {
            let last = chain.last().unwrap().1;
            let next = cells
                .iter()
                .filter(|(ids, r)| {
                    !chain.iter().any(|c| c.0 == *ids)
                        && (r[0] - last[2]).abs() < tol
                        && (r[1] - last[1]).abs() < tol
                        && (r[3] - last[3]).abs() < tol
                })
                .min_by(|x, y| x.1[0].total_cmp(&y.1[0]))
                .cloned();
            match next {
                Some(n) => chain.push(n),
                None => break,
            }
        }
        if chain.len() < 2 {
            continue;
        }
        let fb = chain.iter().map(|c| c.1).reduce(bbox_union).unwrap();
        let mut fields: Vec<(String, &'static str, String)> = Vec::new();
        for (i, (_, r)) in chain.iter().enumerate().skip(1) {
            for t in cx
                .texts
                .values()
                .filter(|t| !b.used(&t.id) && inside(t.center, *r, tol))
            {
                fields.push((
                    t.id.clone(),
                    if i == 1 {
                        "tolerance_value"
                    } else {
                        "datum_reference"
                    },
                    t.dec.text.clone(),
                ));
            }
        }
        let label = format!(
            "{} {}",
            glyph_label(kind),
            fields
                .iter()
                .map(|f| f.2.as_str())
                .collect::<Vec<_>>()
                .join(" | ")
        );
        let e = b.expr(
            kind,
            &label,
            "exploded_tolerance_frame_glyph_cells_fields",
            "candidate",
            vec![],
            vec![
                format!("glyph:{}", gids.join("+")),
                format!("cells:{}", chain.len()),
                "card:gdt_frame_symbol".into(),
                format!("variant:{}", if *is_loose { "loose_glyph" } else { "block_glyph" }),
            ],
            vec!["Toleranced feature requires the leader target".into()],
        );
        for gid in gids {
            b.member(
                e,
                gid,
                None,
                [0., 1.],
                "symbol_glyph",
                if *is_loose { member_line_type(cx.lt(gid), "symbol_glyph") } else { "block" },
                "candidate",
                vec![],
                vec![],
            );
        }
        for (ids, _) in &chain {
            for id in ids {
                b.member(
                    e,
                    id,
                    None,
                    [0., 1.],
                    "frame_cell",
                    member_line_type(cx.lt(id), "frame_cell"),
                    "candidate",
                    vec![],
                    vec![],
                );
            }
        }
        for (id, role, _) in &fields {
            b.member(
                e,
                id,
                None,
                [0., 1.],
                role,
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
        // Leader: a free segment starting on the frame border, followed to its targets.
        let border_tol = 0.08 * h;
        let on_border = |p: P| {
            inside(p, fb, border_tol)
                && ((p[0] - fb[0]).abs() < border_tol
                    || (p[0] - fb[2]).abs() < border_tol
                    || (p[1] - fb[1]).abs() < border_tol
                    || (p[1] - fb[3]).abs() < border_tol)
        };
        let start = free_segments(cx, b).into_iter().find_map(|(id, pts)| {
            let n = pts.len();
            let (f, l) = (pts[0], pts[n - 1]);
            if on_border(f) && !inside(l, fb, border_tol) {
                Some((id, l, pts[n - 2]))
            } else if on_border(l) && !inside(f, fb, border_tol) {
                Some((id, f, pts[1]))
            } else {
                None
            }
        });
        if let Some((sid, far, prev)) = start {
            let mut taken = BTreeSet::from([sid.clone()]);
            let (mut segs, mut arrows, ends) = branches(cx, b, &[(far, prev)], &mut taken, 4);
            segs.insert(0, sid);
            // GB/T 1182: a leader aligned with the dimension line of a feature of size
            // ends on that dimension line; its terminators and the line between them
            // belong to the size dimension and are shared with the frame.
            let nodes: Vec<P> = segs
                .iter()
                .flat_map(|s| cx.points(s))
                .chain(ends.iter().copied())
                .collect();
            let attached = arrow_pairs(cx, b).into_iter().find(|pr| {
                pr.supports.iter().any(|s| segs.contains(s))
                    || nodes
                        .iter()
                        .any(|p| dist(*p, pr.p) < 0.25 * pr.alen || dist(*p, pr.q) < 0.25 * pr.alen)
            });
            if let Some(pr) = attached {
                let u = unit(sub(pr.q, pr.p));
                let span = dist(pr.p, pr.q);
                let between = |s: &String| {
                    cx.lines.get(s).is_some_and(|(a, c)| {
                        let (ta, tc) = (dot(sub(*a, pr.p), u), dot(sub(*c, pr.p), u));
                        ta.min(tc) > -0.1 * pr.alen && ta.max(tc) < span + 0.1 * pr.alen
                    })
                };
                arrows.retain(|a| *a != pr.a && *a != pr.b);
                let own: Vec<String> = segs.iter().filter(|s| !between(s)).cloned().collect();
                add_leader(
                    cx,
                    b,
                    e,
                    &own,
                    &arrows,
                    "leader_from_frame_border_along_dimension_line",
                );
                let d = attached_dimension(cx, b, &pr, fb, &own, anchors);
                for a in [&pr.a, &pr.b] {
                    b.member(
                        e,
                        a,
                        None,
                        [0., 1.],
                        "arrowhead",
                        "fill",
                        "candidate",
                        vec![],
                        vec!["terminator_of_toleranced_size_dimension".into()],
                    );
                }
                for s in pr.supports.iter().filter(|s| between(s)) {
                    b.member(
                        e,
                        s,
                        None,
                        [0., 1.],
                        "dimension_line",
                        member_line_type(cx.lt(s), "dimension_line"),
                        "candidate",
                        vec![],
                        vec!["dimension_line_of_toleranced_feature_of_size".into()],
                    );
                }
                let dim_id = b.exprs[d].id.clone();
                let ext: Vec<(String, [f64; 2])> = b
                    .members
                    .iter()
                    .filter(|m| m.expression_id == dim_id && m.member_role == "extension_line")
                    .map(|m| (m.source_handle.clone(), [m.interval[0], m.interval[1]]))
                    .collect();
                for (src, iv) in ext {
                    b.member(
                        e,
                        &src,
                        None,
                        iv,
                        "extension_line",
                        member_line_type(cx.lt(&src), "extension_line"),
                        "candidate",
                        vec![],
                        vec!["extension_line_of_toleranced_feature_of_size".into()],
                    );
                }
                b.links.push((e, "applies_to_size_dimension", d));
                b.links.push((d, "toleranced_by_frame", e));
                anchors.insert(e, mid(pr.p, pr.q));
            } else {
                add_leader(cx, b, e, &segs, &arrows, "leader_from_frame_border");
                if let Some(p) = arrows
                    .first()
                    .and_then(|a| cx.arrow(a))
                    .map(|a| a.tip)
                    .or_else(|| ends.first().copied())
                {
                    anchors.insert(e, p);
                }
            }
        }
        // Scope note written beside the frame (e.g. a restriction to one section).
        let notes: Vec<String> = cx
            .texts
            .values()
            .filter(|t| {
                !b.used(&t.id)
                    && t.lex == Lex::Note
                    && t.center[0] >= fb[0] - 2. * h
                    && t.center[0] <= fb[2] + 2. * h
                    && (t.center[1] - fb[1]).abs().min((t.center[1] - fb[3]).abs()) < 1.6 * h
            })
            .map(|t| t.id.clone())
            .collect();
        if !notes.is_empty() {
            let row =
                notes.iter().map(|id| cx.texts[id].center[1]).sum::<f64>() / notes.len() as f64;
            let xs: Vec<f64> = notes
                .iter()
                .flat_map(|id| {
                    [
                        cx.texts[id].center[0] - cx.texts[id].width,
                        cx.texts[id].center[0] + cx.texts[id].width,
                    ]
                })
                .collect();
            let (x0, x1) = (
                xs.iter().copied().fold(f64::INFINITY, f64::min),
                xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            );
            let row_ids: Vec<String> = cx
                .texts
                .values()
                .filter(|t| {
                    !b.used(&t.id)
                        && (t.center[1] - row).abs() < 0.6 * t.height
                        && t.center[0] >= x0
                        && t.center[0] <= x1
                })
                .map(|t| t.id.clone())
                .collect();
            for id in row_ids {
                b.member(
                    e,
                    &id,
                    None,
                    [0., 1.],
                    "note_text",
                    "text",
                    "candidate",
                    vec![],
                    vec!["scope_note_beside_frame".into()],
                );
            }
        }
    }
}

struct Pair {
    a: String,
    b: String,
    p: P,
    q: P,
    supports: Vec<String>,
    alen: f64,
}

/// Opposed terminators on one thin dimension line. Both tips lie on the support
/// line itself; tails outside the terminators may start at a head's base;
/// collinear continuations are added up to another terminator.
fn arrow_pairs(cx: &Ctx, b: &Builder) -> Vec<Pair> {
    let free: Vec<(&String, &Arrow)> = cx.arrows.iter().filter(|(id, _)| !b.used(id)).collect();
    let tol = 1e-3 * cx.scale.max(1.);
    let mut out = Vec::new();
    for (i, (ia, x)) in free.iter().enumerate() {
        for (ib, y) in free.iter().skip(i + 1) {
            let alen = x.len.max(y.len);
            if dot(x.dir, y.dir) > -0.985 {
                continue;
            }
            let span = dist(x.tip, y.tip);
            if span < 0.3 * x.len.min(y.len) {
                continue;
            }
            let u = unit(sub(y.tip, x.tip));
            let inside_cfg = dot(u, y.dir) > 0. && dot(u, x.dir) < 0.;
            let outside_cfg = dot(u, x.dir) > 0. && dot(u, y.dir) < 0.;
            if !inside_cfg && !outside_cfg {
                continue;
            }
            let ptol = 0.03 * alen + tol;
            let m = 0.1 * alen;
            let mut supports: Vec<String> = cx
                .lines
                .iter()
                .filter(|(id, (a, c))| {
                    let l = dist(*a, *c);
                    !b.used(id)
                        && cx.annotation_style(id)
                        && l > 1e-9
                        && cross(unit(sub(*c, *a)), u).abs() < 0.01
                        && project(x.tip, *a, *c).1 < ptol
                        && project(y.tip, *a, *c).1 < ptol
                        && {
                            let (ta, tc) = (dot(sub(*a, x.tip), u), dot(sub(*c, x.tip), u));
                            let (lo, hi) = (ta.min(tc), ta.max(tc));
                            if inside_cfg {
                                hi > 0.1 * span && lo < 0.9 * span
                            } else {
                                (lo <= m && hi >= -x.len - m)
                                    || (lo <= span + y.len + m && hi >= span - m)
                            }
                        }
                })
                .map(|(id, _)| id.clone())
                .collect();
            if supports.is_empty() {
                continue;
            }
            let cover = supports
                .iter()
                .fold([f64::INFINITY, f64::NEG_INFINITY], |acc, s| {
                    let (a, c) = cx.lines[s];
                    let (ta, tc) = (dot(sub(a, x.tip), u), dot(sub(c, x.tip), u));
                    [acc[0].min(ta.min(tc)), acc[1].max(ta.max(tc))]
                });
            let stops: Vec<P> = cx
                .arrows
                .iter()
                .filter(|(id, _)| *id != *ia && *id != *ib)
                .map(|(_, a)| a.tip)
                .collect();
            let skip: BTreeSet<String> = supports.iter().cloned().collect();
            supports.extend(collinear_chain(
                cx, b, x.tip, u, cover, ptol, &skip, &stops, true,
            ));
            out.push(Pair {
                a: (*ia).clone(),
                b: (*ib).clone(),
                p: x.tip,
                q: y.tip,
                supports,
                alen,
            });
        }
    }
    out
}
fn is_dim_text(l: Lex) -> bool {
    matches!(
        l,
        Lex::Number | Lex::Diameter | Lex::Radius | Lex::Angle | Lex::Chamfer
    )
}

fn text_cost(t: &TextFact, p: P, q: P) -> Option<f64> {
    let u = unit(sub(q, p));
    let span = dist(p, q);
    let c = sub(t.center, p);
    let along = dot(c, u);
    let perp = cross(u, c).abs();
    if cross(t.dir, u).abs() > 0.2
        || perp > 2.2 * t.height
        || along < -(t.width + 2. * t.height)
        || along > span + t.width + 2. * t.height
    {
        return None;
    }
    Some(perp / t.height + 0.6 * (along - span / 2.).abs() / span.max(t.width))
}

/// A text sits on segment `pts` when its baseline is just above one of the
/// segment pieces and they overlap along the reading direction.
fn on_shoulder(t: &TextFact, pts: &[P]) -> bool {
    pts.windows(2).any(|w| {
        let v = unit(sub(w[1], w[0]));
        if cross(v, t.dir).abs() > 0.05 {
            return false;
        }
        let below = cross(t.dir, sub(t.center, w[0]));
        let (s0, s1) = (
            dot(sub(w[0], t.center), t.dir),
            dot(sub(w[1], t.center), t.dir),
        );
        let overlap = s0.max(s1).min(t.width / 2.) - s0.min(s1).max(-t.width / 2.);
        below > 0.2 * t.height && below < 1.6 * t.height && overlap > 0.4 * t.width
    })
}

fn add_extensions(cx: &Ctx, b: &mut Builder, e: usize, pr: &Pair, points: &[(String, P)]) {
    let u = unit(sub(pr.q, pr.p));
    let exclude: BTreeSet<String> = pr.supports.iter().cloned().collect();
    let mut ends = Vec::new();
    for tip in [pr.p, pr.q] {
        ends.extend(extension_at(cx, b, e, tip, u, pr.alen, &exclude));
        ends.push(tip);
    }
    for (ph, p) in points {
        if ends.iter().any(|q| dist(*p, *q) < 1e-3 * cx.scale.max(1.)) {
            b.member(
                e,
                ph,
                None,
                [0., 1.],
                "definition_point",
                "point",
                "candidate",
                vec![],
                vec!["coincident_definition_point".into()],
            );
        }
    }
}
fn exploded_dimensions(cx: &Ctx, b: &mut Builder, anchors: &mut BTreeMap<usize, P>) {
    let pairs = arrow_pairs(cx, b);
    let mut cand: Vec<(f64, usize, String)> = Vec::new();
    for (i, pr) in pairs.iter().enumerate() {
        for t in cx
            .texts
            .values()
            .filter(|t| !b.used(&t.id) && is_dim_text(t.lex))
        {
            if let Some(c) = text_cost(t, pr.p, pr.q) {
                cand.push((c, i, t.id.clone()));
            }
        }
    }
    cand.sort_by(|a, c| a.0.total_cmp(&c.0));
    let (mut used_arrows, mut used_texts, mut used_pairs) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut chosen = Vec::new();
    for (cost, i, tid) in cand {
        let pr = &pairs[i];
        if used_pairs.contains(&i)
            || used_texts.contains(&tid)
            || used_arrows.contains(&pr.a)
            || used_arrows.contains(&pr.b)
        {
            continue;
        }
        used_pairs.insert(i);
        used_texts.insert(tid.clone());
        used_arrows.insert(pr.a.clone());
        used_arrows.insert(pr.b.clone());
        chosen.push((i, tid, cost));
    }
    let points: Vec<(String, P)> = cx
        .rec
        .iter()
        .filter(|(_, r)| r.kind == "POINT")
        .filter_map(|(h, r)| r.xy(10).ok().map(|p| (h.clone(), p)))
        .collect();
    for (i, tid, cost) in chosen {
        let pr = &pairs[i];
        let t = &cx.texts[&tid];
        let kind = tx::dimension_kind(&t.dec.text).unwrap_or("dimension_linear");
        let e = b.expr(
            kind,
            &t.dec.text,
            "opposed_terminators_dimension_line_and_text",
            "candidate",
            tx::qualifiers(&t.dec),
            vec![
                format!("text:{tid}"),
                format!("terminators:{},{}", pr.a, pr.b),
                format!("text_cost:{cost:.3}"),
            ],
            vec!["Measured part feature requires projection evidence".into()],
        );
        b.member(
            e,
            &tid,
            None,
            [0., 1.],
            "dimension_text",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        for a in [&pr.a, &pr.b] {
            b.member(
                e,
                a,
                None,
                [0., 1.],
                "arrowhead",
                "fill",
                "candidate",
                vec![],
                vec![],
            );
        }
        for s in &pr.supports {
            b.member(
                e,
                s,
                None,
                [0., 1.],
                "dimension_line",
                member_line_type(cx.lt(s), "dimension_line"),
                "candidate",
                vec![],
                vec!["collinear_with_terminators".into()],
            );
        }
        add_extensions(cx, b, e, pr, &points);
        anchors.insert(e, mid(pr.p, pr.q));
    }
}

/// Terminators across a hole or feature whose dimension line continues as a
/// leader to a distant text (thread or diameter callout). Runs after tolerance
/// frames have taken their fields.
fn extended_pair_dimensions(cx: &Ctx, b: &mut Builder, anchors: &mut BTreeMap<usize, P>) {
    let frames = frame_boxes(cx, b);
    for pr in arrow_pairs(cx, b) {
        if b.used(&pr.a) || b.used(&pr.b) {
            continue;
        }
        let u = unit(sub(pr.q, pr.p));
        let span = dist(pr.p, pr.q);
        let mut starts = Vec::new();
        for s in &pr.supports {
            let (a, c) = cx.lines[s];
            for (e, o) in [(a, c), (c, a)] {
                let along = dot(sub(e, pr.p), u);
                if along < -0.5 * pr.alen || along > span + 0.5 * pr.alen {
                    starts.push((e, o));
                }
            }
        }
        if starts.is_empty() {
            continue;
        }
        let mut taken: BTreeSet<String> = pr.supports.iter().cloned().collect();
        let (segs, _, ends) = branches(cx, b, &starts, &mut taken, 3);
        let seg_pts: Vec<Vec<P>> = segs
            .iter()
            .filter_map(|id| {
                cx.lines
                    .get(id)
                    .map(|(a, c)| vec![*a, *c])
                    .or_else(|| cx.polys.get(id).map(|x| x.0.clone()))
            })
            .collect();
        let text = cx
            .texts
            .values()
            .filter(|t| {
                !b.used(&t.id)
                    && matches!(
                        t.lex,
                        Lex::Thread | Lex::Diameter | Lex::Radius | Lex::Number | Lex::Chamfer
                    )
            })
            .filter(|t| {
                seg_pts.iter().any(|pts| on_shoulder(t, pts))
                    || ends.iter().any(|e| dist(t.center, *e) < 2. * t.height)
            })
            .min_by(|x, y| {
                let dx = ends
                    .iter()
                    .map(|e| dist(x.center, *e))
                    .fold(f64::INFINITY, f64::min);
                let dy = ends
                    .iter()
                    .map(|e| dist(y.center, *e))
                    .fold(f64::INFINITY, f64::min);
                dx.total_cmp(&dy)
            })
            .cloned();
        let Some(t) = text else { continue };
        let kind = match t.lex {
            Lex::Thread => "thread_note",
            Lex::Diameter => "dimension_diameter",
            Lex::Radius => "dimension_radius",
            Lex::Chamfer => "dimension_chamfer",
            _ => "dimension_linear",
        };
        let group = if kind == "thread_note" {
            note_block(cx, b, &t, &frames)
        } else {
            vec![t.id.clone()]
        };
        let e = b.expr(
            kind,
            &group
                .iter()
                .map(|g| cx.texts[g].dec.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
            "terminators_across_feature_with_leader_to_text",
            "candidate",
            tx::qualifiers(&t.dec),
            vec![
                format!("text:{}", t.id),
                format!("terminators:{},{}", pr.a, pr.b),
            ],
            vec!["Target feature requires projection evidence".into()],
        );
        for g in &group {
            b.member(
                e,
                g,
                None,
                [0., 1.],
                if g == &t.id {
                    "dimension_text"
                } else {
                    "note_text"
                },
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
        for a in [&pr.a, &pr.b] {
            b.member(
                e,
                a,
                None,
                [0., 1.],
                "arrowhead",
                "fill",
                "candidate",
                vec![],
                vec![],
            );
        }
        for s in &pr.supports {
            b.member(
                e,
                s,
                None,
                [0., 1.],
                "dimension_line",
                member_line_type(cx.lt(s), "dimension_line"),
                "candidate",
                vec![],
                vec![],
            );
        }
        add_leader(cx, b, e, &segs, &[], "leader_continuing_dimension_line");
        anchors.insert(e, mid(pr.p, pr.q));
    }
}

/// Rectangles of the tolerance frames already assembled (union of frame members).
fn frame_boxes(cx: &Ctx, b: &Builder) -> Vec<[f64; 4]> {
    b.exprs
        .iter()
        .filter(|e| e.family == "tolerance" && e.kind != "datum_symbol")
        .filter_map(|e| {
            let pts: Vec<P> = b
                .members
                .iter()
                .filter(|m| {
                    m.expression_id == e.id
                        && matches!(m.member_role.as_str(), "frame_cell" | "native_block")
                })
                .flat_map(|m| cx.points(&m.source_handle))
                .collect();
            bbox_of(&pts)
        })
        .collect()
}

/// Text block of a thread or hole callout: thread, depth, count and chamfer
/// wording on one row or on rows directly above or below; rows written under a
/// tolerance frame placed beneath the block continue it across the frame. Texts
/// inside frames are never taken.
fn note_block(cx: &Ctx, b: &Builder, seed: &TextFact, frames: &[[f64; 4]]) -> Vec<String> {
    let stacked_ok = |l: Lex| {
        matches!(
            l,
            Lex::Thread | Lex::ThreadDepth | Lex::Number | Lex::Chamfer
        )
    };
    let row_ok = |t: &TextFact| {
        stacked_ok(t.lex)
            || t.lex == Lex::ToleranceValue
            || (t.lex == Lex::Other && t.dec.text.chars().count() <= 2)
    };
    let in_frame = |t: &TextFact| frames.iter().any(|f| inside(t.center, *f, 0.));
    let h0 = seed.height;
    let mut group = vec![seed.id.clone()];
    let mut changed = true;
    while changed {
        changed = false;
        let (gx0, gx1, gy0) = group.iter().fold(
            (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY),
            |acc, g| {
                let t = &cx.texts[g];
                (
                    acc.0.min(t.center[0] - t.width / 2.),
                    acc.1.max(t.center[0] + t.width / 2.),
                    acc.2.min(t.center[1] - t.height / 2.),
                )
            },
        );
        let under: Vec<[f64; 4]> = frames
            .iter()
            .copied()
            .filter(|f| {
                f[3] <= gy0 + 0.2 * h0
                    && gy0 - f[3] < 1.8 * h0
                    && f[0] < gx1 + h0
                    && f[2] > gx0 - h0
            })
            .collect();
        for t in cx.texts.values() {
            let ratio = t.height / h0;
            if group.contains(&t.id)
                || b.used(&t.id)
                || in_frame(t)
                || !(0.5..=2.0).contains(&ratio)
                || !row_ok(t)
            {
                continue;
            }
            let near = group.iter().any(|g| {
                let s = &cx.texts[g];
                let h = s.height.max(t.height);
                let d = sub(t.center, s.center);
                let across = cross(s.dir, d).abs();
                let gap = dot(s.dir, d).abs() - (s.width + t.width) / 2.;
                (across < 0.6 * h && gap < 1.5 * h)
                    || (stacked_ok(t.lex) && across >= 0.6 * h && across < 2.2 * h && gap < 0.5 * h)
            });
            let across_frame = stacked_ok(t.lex)
                && under.iter().any(|f| {
                    let top = t.center[1] + t.height / 2.;
                    top <= f[1] + 0.2 * h0
                        && f[1] - top < 2.2 * h0
                        && t.center[0] > f[0] - 2. * h0
                        && t.center[0] < f[2] + 2. * h0
                });
            if near || across_frame {
                group.push(t.id.clone());
                changed = true;
            }
        }
    }
    group
}

/// Dimension line of a feature of size carrying an attached tolerance frame: the
/// opposed terminators, the line between them, its collinear continuation along
/// the frame leader and the extension lines. The value is the text stacked
/// directly on the frame; a thread designation there makes it a thread callout.
fn attached_dimension(
    cx: &Ctx,
    b: &mut Builder,
    pr: &Pair,
    fb: [f64; 4],
    path: &[String],
    anchors: &mut BTreeMap<usize, P>,
) -> usize {
    let tol = 1e-3 * cx.scale.max(1.);
    let u = unit(sub(pr.q, pr.p));
    let mut stacked: Vec<(f64, TextFact)> = cx
        .texts
        .values()
        .filter(|t| {
            !b.used(&t.id)
                && (is_dim_text(t.lex) || t.lex == Lex::Thread)
                && t.dir[0] > 0.99
                && t.center[0] > fb[0] - 2. * t.height
                && t.center[0] < fb[2] + 2. * t.height
        })
        .filter_map(|t| {
            let above = t.center[1] - fb[3];
            let below = fb[1] - t.center[1];
            let gap = if above > 0. {
                above
            } else if below > 0. {
                below + 0.5 * t.height
            } else {
                return None;
            };
            (gap < 2.2 * t.height).then(|| (gap / t.height, t.clone()))
        })
        .collect();
    stacked.sort_by(|x, y| x.0.total_cmp(&y.0));
    let seed = stacked.first().map(|x| x.0).and_then(|g0| {
        let row: Vec<&TextFact> = stacked
            .iter()
            .filter(|x| x.0 < g0 + 0.5)
            .map(|x| &x.1)
            .collect();
        row.iter()
            .find(|t| t.lex == Lex::Thread)
            .or(row.first())
            .map(|t| (*t).clone())
    });
    let frames = frame_boxes(cx, b);
    let (kind, group) = match &seed {
        Some(t) if t.lex == Lex::Thread => ("thread_note", note_block(cx, b, t, &frames)),
        Some(t) => (
            tx::dimension_kind(&t.dec.text).unwrap_or("dimension_linear"),
            vec![t.id.clone()],
        ),
        None => ("dimension_linear", vec![]),
    };
    let label = group
        .iter()
        .map(|g| cx.texts[g].dec.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    let e = b.expr(
        kind,
        if label.is_empty() {
            "尺寸线（数值未定）"
        } else {
            &label
        },
        "dimension_line_of_feature_of_size_with_attached_frame",
        if seed.is_some() {
            "candidate"
        } else {
            "unknown"
        },
        seed.as_ref()
            .map(|t| tx::qualifiers(&t.dec))
            .unwrap_or_default(),
        vec![
            format!("terminators:{},{}", pr.a, pr.b),
            "GB/T 1182 leader aligned with the dimension line of a feature of size".into(),
        ],
        vec!["Measured part feature requires projection evidence".into()],
    );
    let seed_id = seed.as_ref().map(|t| t.id.clone());
    for g in &group {
        let role = if Some(g) == seed_id.as_ref() {
            "dimension_text"
        } else {
            "note_text"
        };
        b.member(
            e,
            g,
            None,
            [0., 1.],
            role,
            "text",
            "candidate",
            vec![],
            vec!["text_stacked_on_attached_frame".into()],
        );
    }
    for a in [&pr.a, &pr.b] {
        b.member(
            e,
            a,
            None,
            [0., 1.],
            "arrowhead",
            "fill",
            "candidate",
            vec![],
            vec![],
        );
    }
    for s in &pr.supports {
        b.member(
            e,
            s,
            None,
            [0., 1.],
            "dimension_line",
            member_line_type(cx.lt(s), "dimension_line"),
            "candidate",
            vec![],
            vec!["collinear_with_terminators".into()],
        );
    }
    for s in path {
        let collinear = cx.lines.get(s).is_some_and(|(a, c)| {
            cross(unit(sub(*c, *a)), u).abs() < 0.01
                && project(*a, pr.p, pr.q).1 < 0.03 * pr.alen + tol
                && project(*c, pr.p, pr.q).1 < 0.03 * pr.alen + tol
        });
        let (role, why) = if collinear {
            (
                "dimension_line",
                "dimension_line_continued_to_attached_frame",
            )
        } else {
            ("leader_line", "leader_path_shared_with_attached_frame")
        };
        b.member(
            e,
            s,
            None,
            [0., 1.],
            role,
            member_line_type(cx.lt(s), role),
            "candidate",
            vec![],
            vec![why.into()],
        );
    }
    let exclude: BTreeSet<String> = pr.supports.iter().chain(path.iter()).cloned().collect();
    for tip in [pr.p, pr.q] {
        extension_at(cx, b, e, tip, u, pr.alen, &exclude);
    }
    anchors.insert(e, mid(pr.p, pr.q));
    e
}

/// Angle dimensions drawn as concentric arc pieces with tangent terminators.
fn exploded_angles(cx: &Ctx, b: &mut Builder) {
    let texts: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id) && t.lex == Lex::Angle)
        .cloned()
        .collect();
    for t in texts {
        let arcs: Vec<(String, P, f64)> = cx
            .rec
            .keys()
            .filter(|h| cx.kind(h) == "ARC" && !b.used(h) && cx.annotation_style(h))
            .filter_map(|h| cx.arc(h).map(|(c, r, ..)| (h.clone(), c, r)))
            .filter(|(_, c, r)| (dist(t.center, *c) - r).abs() < 2.5 * t.height)
            .collect();
        let Some((_, c, r)) = arcs.first().cloned() else {
            continue;
        };
        let tips: Vec<String> = cx
            .arrows
            .iter()
            .filter(|(id, a)| {
                !b.used(id)
                    && (dist(a.tip, c) - r).abs() < (0.25 * a.len).max(0.06 * r)
                    && dot(a.dir, unit(sub(a.tip, c))).abs() < 0.3
            })
            .map(|(id, _)| id.clone())
            .collect();
        if tips.is_empty() {
            continue;
        }
        let e = b.expr(
            "dimension_angle",
            &t.dec.text,
            "concentric_arc_tangent_terminators_and_text",
            "candidate",
            tx::qualifiers(&t.dec),
            vec![format!("text:{}", t.id)],
            vec!["Measured angle sides require projection evidence".into()],
        );
        b.member(
            e,
            &t.id,
            None,
            [0., 1.],
            "dimension_text",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        for (h, _, _) in arcs
            .iter()
            .filter(|(_, cc, rr)| dist(*cc, c) < 1e-6 && (rr - r).abs() < 1e-6)
        {
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "dimension_line",
                member_line_type(cx.lt(h), "dimension_line"),
                "candidate",
                vec![],
                vec![],
            );
        }
        for a in &tips {
            b.member(
                e,
                a,
                None,
                [0., 1.],
                "arrowhead",
                "fill",
                "candidate",
                vec![],
                vec![],
            );
            let tip = cx.arrows[a].tip;
            let radial = |p: P, q: P| cross(unit(sub(q, p)), unit(sub(tip, c))).abs() < 0.05;
            if let Some((id, _)) = cx.lines.iter().find(|(id, (p, q))| {
                !b.used(id)
                    && cx.annotation_style(id)
                    && radial(*p, *q)
                    && project(tip, *p, *q).1 < 0.3 * cx.arrows[a].len
            }) {
                b.member(
                    e,
                    id,
                    None,
                    [0., 1.],
                    "extension_line",
                    member_line_type(cx.lt(id), "extension_line"),
                    "candidate",
                    vec![],
                    vec!["radial_through_terminator".into()],
                );
            }
        }
    }
}

/// Leaders without a dimension line: a single terminator with a collinear thin
/// support whose tail carries the text, or a thin shoulder under the text with
/// one or more branches to the feature. Candidates compete by cost globally.
fn leader_annotations(
    cx: &Ctx,
    b: &mut Builder,
    text_anchor: &BTreeMap<usize, (P, f64)>,
    anchors: &mut BTreeMap<usize, P>,
) {
    let frames = frame_boxes(cx, b);
    // Thread-depth texts stacked under a native thread dimension belong to it.
    for (e, (p, h)) in text_anchor {
        if b.exprs[*e].kind != "thread_note" {
            continue;
        }
        let extra: Vec<String> = cx
            .texts
            .values()
            .filter(|t| {
                !b.used(&t.id)
                    && matches!(t.lex, Lex::ThreadDepth | Lex::Number)
                    && dist(t.center, *p) < 4.0 * h.max(t.height)
            })
            .map(|t| t.id.clone())
            .collect();
        for id in extra {
            b.member(
                *e,
                &id,
                None,
                [0., 1.],
                "note_text",
                "text",
                "candidate",
                vec![],
                vec!["stacked_under_native_thread_text".into()],
            );
        }
    }
    // (a) single terminator with collinear thin support. A text resting on a short
    // shoulder that carries no terminator itself is left to the shoulder rule (b).
    let segs0 = free_segments(cx, b);
    let on_real_shoulder = |t: &TextFact| {
        segs0.iter().any(|(id, pts)| {
            let n = pts.len();
            let l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
            on_shoulder(t, pts)
                && (cx.polys.contains_key(id) || l <= t.width + 4. * t.height)
                && !cx.arrows.iter().any(|(aid, a)| {
                    !b.used(aid)
                        && (continues(a, pts[0], pts[1], 0.15 * a.len)
                            || continues(a, pts[n - 1], pts[n - 2], 0.15 * a.len))
                })
        })
    };
    let mut cands: Vec<(f64, String, String, String, P)> = Vec::new();
    for t in cx.texts.values().filter(|t| {
        !b.used(&t.id)
            && matches!(
                t.lex,
                Lex::Radius
                    | Lex::Diameter
                    | Lex::Chamfer
                    | Lex::Thread
                    | Lex::ThreadDepth
                    | Lex::Number
                    | Lex::Letters
            )
    }) {
        if on_real_shoulder(t) {
            continue;
        }
        let h = t.height;
        for (aid, a) in cx.arrows.iter().filter(|(id, _)| !b.used(id)) {
            for (lid, (p, q)) in &cx.lines {
                if b.used(lid)
                    || !cx.annotation_style(lid)
                    || cross(unit(sub(*q, *p)), a.dir).abs() > 0.03
                {
                    continue;
                }
                let gap = |x: P| dist(x, a.tip).min(dist(x, base_of(a)));
                let (near, far) = if gap(*p) <= gap(*q) {
                    (*p, *q)
                } else {
                    (*q, *p)
                };
                if !continues(a, near, far, 0.15 * a.len.max(1e-9)) {
                    continue;
                }
                let l = dist(a.tip, far);
                let v = unit(sub(far, a.tip));
                let along = dot(sub(t.center, a.tip), v);
                let perp = cross(v, sub(t.center, a.tip)).abs();
                if t.lex == Lex::Letters && l > 6. * a.len {
                    continue;
                }
                if perp < 2.2 * h && along > 0.1 * l && along < l + t.width + 2. * h {
                    cands.push((
                        perp / h + (along - l).abs() / l.max(h),
                        t.id.clone(),
                        aid.clone(),
                        lid.clone(),
                        far,
                    ));
                }
            }
        }
    }
    cands.sort_by(|a, c| a.0.total_cmp(&c.0));
    let (mut ut, mut ua, mut ul) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for (_, tid, aid, lid, far) in cands {
        if ut.contains(&tid) || ua.contains(&aid) || ul.contains(&lid) || b.used(&tid) {
            continue;
        }
        ut.insert(tid.clone());
        ua.insert(aid.clone());
        ul.insert(lid.clone());
        let t = cx.texts[&tid].clone();
        let kind = match t.lex {
            Lex::Radius => "dimension_radius",
            Lex::Diameter => "dimension_diameter",
            Lex::Chamfer => "dimension_chamfer",
            Lex::Thread | Lex::ThreadDepth => "thread_note",
            Lex::Letters => "view_callout",
            _ => "dimension_linear",
        };
        // Collinear pieces continuing the line (split around the text or overlapping).
        let pieces = {
            let (p, q) = cx.lines[&lid];
            let a = &cx.arrows[&aid];
            let stops: Vec<P> = cx
                .arrows
                .iter()
                .filter(|(id, _)| **id != aid)
                .map(|(_, x)| x.tip)
                .collect();
            collinear_chain(
                cx,
                b,
                p,
                unit(sub(q, p)),
                [0., dist(p, q)],
                1e-3 * cx.scale.max(1.) + 0.03 * a.len,
                &BTreeSet::from([lid.clone()]),
                &stops,
                true,
            )
        };
        let group = if kind == "thread_note" {
            note_block(cx, b, &t, &frames)
        } else {
            vec![t.id.clone()]
        };
        // GB/T 4458.4: radius dimension lines and dimension lines with one terminator
        // (past a symmetry line) are dimension lines, not leaders.
        let dim_line = matches!(
            kind,
            "dimension_radius" | "dimension_diameter" | "dimension_linear"
        );
        let rule = if kind == "view_callout" {
            "viewing_direction_arrow_and_letter"
        } else if dim_line {
            "one_terminator_dimension_line_and_text"
        } else {
            "single_terminator_leader_and_text"
        };
        let e = b.expr(
            kind,
            &group
                .iter()
                .map(|g| cx.texts[g].dec.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
            rule,
            "candidate",
            if kind == "view_callout" {
                vec!["view_direction".into()]
            } else {
                tx::qualifiers(&t.dec)
            },
            vec![format!("text:{}", t.id), format!("terminator:{aid}")],
            vec!["Target feature requires projection evidence".into()],
        );
        for g in &group {
            b.member(
                e,
                g,
                None,
                [0., 1.],
                if kind == "view_callout" {
                    "view_letter"
                } else if g == &t.id {
                    "dimension_text"
                } else {
                    "note_text"
                },
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
        let role = if kind == "view_callout" {
            "view_arrow"
        } else if dim_line {
            "dimension_line"
        } else {
            "leader_line"
        };
        b.member(
            e,
            &lid,
            None,
            [0., 1.],
            role,
            member_line_type(cx.lt(&lid), role),
            "candidate",
            vec![],
            vec![],
        );
        for pc in &pieces {
            b.member(
                e,
                pc,
                None,
                [0., 1.],
                role,
                member_line_type(cx.lt(pc), role),
                "candidate",
                vec![],
                vec!["collinear_piece".into()],
            );
        }
        b.member(
            e,
            &aid,
            None,
            [0., 1.],
            "arrowhead",
            "fill",
            "candidate",
            vec![],
            vec![],
        );
        if matches!(kind, "dimension_diameter" | "dimension_linear") {
            let a = cx.arrows[&aid].clone();
            extension_at(
                cx,
                b,
                e,
                a.tip,
                a.dir,
                a.len,
                &BTreeSet::from([lid.clone()]),
            );
            if kind == "dimension_diameter" {
                let eid = b.exprs[e].id.clone();
                let near: Vec<String> = b
                    .members
                    .iter()
                    .filter(|m| m.expression_id == eid && m.member_role == "extension_origin")
                    .map(|m| m.source_handle.clone())
                    .collect();
                if let Some(edge) = far_silhouette(cx, b, far, a.dir, a.len, &near) {
                    b.member(
                        e,
                        &edge,
                        None,
                        [0., 1.],
                        "extension_origin",
                        cx.lt(&edge),
                        "candidate",
                        vec![],
                        vec!["far_silhouette_at_dimension_line_end".into()],
                    );
                }
            }
        }
        anchors.insert(e, cx.arrows[&aid].tip);
    }
    // (b) shoulder under the text, followed through all connected thin branches.
    let seeds: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| {
            !b.used(&t.id)
                && matches!(
                    t.lex,
                    Lex::Radius
                        | Lex::Diameter
                        | Lex::Chamfer
                        | Lex::Thread
                        | Lex::ThreadDepth
                        | Lex::Roman
                )
        })
        .cloned()
        .collect();
    for t in seeds {
        if b.used(&t.id) {
            continue;
        }
        let shoulder = free_segments(cx, b).into_iter().find(|(id, pts)| {
            let l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
            on_shoulder(&t, pts) && (cx.polys.contains_key(id) || l <= t.width + 4. * t.height)
        });
        let Some((sid, pts)) = shoulder else { continue };
        let mut taken = BTreeSet::from([sid.clone(), t.id.clone()]);
        let n = pts.len();
        let (mut segs, arrows, ends) = branches(
            cx,
            b,
            &[(pts[0], pts[1]), (pts[n - 1], pts[n - 2])],
            &mut taken,
            4,
        );
        segs.insert(0, sid.clone());
        let kind = match t.lex {
            Lex::Radius => "dimension_radius",
            Lex::Diameter => "dimension_diameter",
            Lex::Chamfer => "dimension_chamfer",
            Lex::Thread | Lex::ThreadDepth => "thread_note",
            _ => "view_callout",
        };
        if t.lex == Lex::Roman {
            // Detail indicator: a branch ends on a thin circle around the detail.
            let circle = cx
                .rec
                .keys()
                .filter(|h| cx.kind(h) == "CIRCLE" && !b.used(h) && !cx.structural_style(h))
                .filter_map(|h| cx.arc(h).map(|(c, r, ..)| (h.clone(), c, r)))
                .find(|(_, c, r)| ends.iter().any(|e| (dist(*e, *c) - r).abs() < 0.08 * r));
            let Some((cid, _, _)) = circle else { continue };
            let e = b.expr(
                "view_callout",
                &t.dec.text,
                "detail_circle_leader_and_numeral",
                "candidate",
                vec!["detail_indicator".into()],
                vec![format!("text:{}", t.id), format!("circle:{cid}")],
                vec![],
            );
            b.member(
                e,
                &t.id,
                None,
                [0., 1.],
                "view_letter",
                "text",
                "candidate",
                vec![],
                vec![],
            );
            b.member(
                e,
                &cid,
                None,
                [0., 1.],
                "detail_circle",
                member_line_type(cx.lt(&cid), "detail_circle"),
                "candidate",
                vec![],
                vec![],
            );
            add_leader(cx, b, e, &segs, &arrows, "leader_shoulder_to_detail_circle");
            if let Some(p) = ends.first() {
                anchors.insert(e, *p);
            }
            continue;
        }
        let group = if kind == "thread_note" {
            note_block(cx, b, &t, &frames)
        } else {
            vec![t.id.clone()]
        };
        let e = b.expr(
            kind,
            &group
                .iter()
                .map(|g| cx.texts[g].dec.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
            "text_on_leader_shoulder",
            "candidate",
            tx::qualifiers(&t.dec),
            vec![format!("text:{}", t.id), format!("shoulder:{sid}")],
            vec!["Target feature requires projection evidence".into()],
        );
        for g in &group {
            b.member(
                e,
                g,
                None,
                [0., 1.],
                if g == &t.id {
                    "dimension_text"
                } else {
                    "note_text"
                },
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
        let sd = cx.lines.get(&sid).map(|(a, c)| unit(sub(*c, *a)));
        let straight = matches!(
            kind,
            "dimension_radius" | "dimension_diameter" | "dimension_linear"
        ) && sd.is_some_and(|d| {
            segs.iter().all(|s| {
                cx.lines
                    .get(s)
                    .is_some_and(|(a, c)| cross(unit(sub(*c, *a)), d).abs() < 0.01)
            })
        });
        if straight {
            for s in &segs {
                b.member(
                    e,
                    s,
                    None,
                    [0., 1.],
                    "dimension_line",
                    member_line_type(cx.lt(s), "dimension_line"),
                    "candidate",
                    vec![],
                    vec!["rule:straight_dimension_line_under_text".into()],
                );
            }
            for a in &arrows {
                b.member(
                    e,
                    a,
                    None,
                    [0., 1.],
                    "arrowhead",
                    "fill",
                    "candidate",
                    vec![],
                    vec![],
                );
            }
        } else {
            add_leader(cx, b, e, &segs, &arrows, "leader_shoulder");
        }
        if let Some(p) = arrows
            .first()
            .and_then(|a| cx.arrow(a))
            .map(|a| a.tip)
            .or_else(|| ends.first().copied())
        {
            anchors.insert(e, p);
        }
    }
}

/// A thread designation written without its own leader (for example beside a
/// tolerance frame that carries the leader) remains a thread note; its target
/// stays unresolved.
fn thread_texts(cx: &Ctx, b: &mut Builder) {
    let frames = frame_boxes(cx, b);
    let seeds: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id) && t.lex == Lex::Thread)
        .cloned()
        .collect();
    for t in seeds {
        if b.used(&t.id) {
            continue;
        }
        let group = note_block(cx, b, &t, &frames);
        let e = b.expr(
            "thread_note",
            &group
                .iter()
                .map(|g| cx.texts[g].dec.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
            "thread_designation_text_block",
            "candidate",
            vec![],
            vec![format!("text:{}", t.id)],
            vec!["Leader and threaded feature not resolved by this rule".into()],
        );
        for g in &group {
            b.member(
                e,
                g,
                None,
                [0., 1.],
                if g == &t.id {
                    "dimension_text"
                } else {
                    "note_text"
                },
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
    }
}

/// GB/T 1182 theoretically exact dimension: the value framed by a rectangle of
/// lines whose fourth side may be the dimension line itself.
fn boxed_values(cx: &Ctx, b: &mut Builder, native_texts: &BTreeMap<usize, TextFact>) {
    let mut facts: Vec<(usize, TextFact)> =
        native_texts.iter().map(|(e, t)| (*e, t.clone())).collect();
    for (i, e) in b.exprs.iter().enumerate() {
        if !(e.kind.starts_with("dimension_") || e.kind == "thread_note")
            || native_texts.contains_key(&i)
        {
            continue;
        }
        if let Some(t) = b
            .members
            .iter()
            .find(|m| m.expression_id == e.id && m.member_role == "dimension_text")
            .and_then(|m| cx.texts.get(&m.source_handle))
        {
            facts.push((i, t.clone()));
        }
    }
    for (e, t) in facts {
        let n = [-t.dir[1], t.dir[0]];
        let h = t.height;
        let tol = 0.25 * h;
        let free: Vec<(String, P, P)> = cx
            .lines
            .iter()
            .filter(|(id, (a, c))| {
                // Outline lines are not frames of a value (they are not yet claimed here).
                !b.used(id)
                    && !cx.structural_style(id)
                    && dist(*a, *c) < 12. * h
                    && cx.style.get(*id).is_some_and(|s| {
                        matches!(s.pattern, Pattern::Continuous | Pattern::ByBlock)
                    })
            })
            .map(|(id, (a, c))| (id.clone(), *a, *c))
            .collect();
        let off = |p: P| dot(sub(p, t.center), n);
        let alg = |p: P| dot(sub(p, t.center), t.dir);
        let top = free.iter().find(|(_, a, c)| {
            cross(unit(sub(*c, *a)), t.dir).abs() < 0.03
                && dist(*a, *c) < t.width + 4. * h
                && (0.45 * h..1.8 * h).contains(&off(*a))
                && (0.45 * h..1.8 * h).contains(&off(*c))
                && alg(*a).min(alg(*c)) < -0.4 * t.width
                && alg(*a).max(alg(*c)) > 0.4 * t.width
        });
        let Some((tid, ta, tc)) = top.cloned() else {
            continue;
        };
        let side = |end: P| {
            free.iter()
                .find(|(id, a, c)| {
                    *id != tid
                        && dot(unit(sub(*c, *a)), t.dir).abs() < 0.05
                        && (dist(*a, end) < tol || dist(*c, end) < tol)
                        && off(*a).min(off(*c)) < -0.3 * h
                })
                .map(|x| x.0.clone())
        };
        let (Some(s1), Some(s2)) = (side(ta), side(tc)) else {
            continue;
        };
        for id in [tid, s1, s2] {
            b.member(
                e,
                &id,
                None,
                [0., 1.],
                "frame_cell",
                member_line_type(cx.lt(&id), "frame_cell"),
                "candidate",
                vec![],
                vec!["box_around_dimension_value".into()],
            );
        }
        if !b.exprs[e]
            .qualifiers
            .iter()
            .any(|q| q == "theoretically_exact")
        {
            b.exprs[e].qualifiers.push("theoretically_exact".into());
            b.exprs[e]
                .evidence_ids
                .push("GB/T 1182 boxed theoretically exact dimension".into());
        }
    }
}

/// Three or more parallel thin oblique lines with uniform spacing in a small
/// region form a section-line pattern drawn with separate lines (GB/T 4457.5).
fn hatch_lines(cx: &Ctx, b: &mut Builder) {
    let thin: Vec<(String, P, P, f64)> = cx
        .lines
        .iter()
        .filter(|(h, (a, c))| !b.used(h) && cx.annotation_style(h) && dist(*a, *c) < 4. * cx.scale)
        .map(|(h, (a, c))| {
            let mut ang = (c[1] - a[1])
                .atan2(c[0] - a[0])
                .to_degrees()
                .rem_euclid(180.);
            if ang > 179.5 {
                ang -= 180.;
            }
            (h.clone(), *a, *c, ang)
        })
        .filter(|x| (x.3 - 0.).abs() > 5. && (x.3 - 90.).abs() > 5.)
        .collect();
    let mut done = BTreeSet::new();
    for (h, _, _, ang) in &thin {
        if done.contains(h) {
            continue;
        }
        let u = [ang.to_radians().cos(), ang.to_radians().sin()];
        let mut group: Vec<&(String, P, P, f64)> = vec![thin.iter().find(|x| &x.0 == h).unwrap()];
        let mut changed = true;
        while changed {
            changed = false;
            for x in &thin {
                if group.iter().any(|g| g.0 == x.0) || (x.3 - ang).abs() > 1. {
                    continue;
                }
                if group
                    .iter()
                    .any(|g| dist(mid(g.1, g.2), mid(x.1, x.2)) < 1.5 * cx.scale)
                {
                    group.push(x);
                    changed = true;
                }
            }
        }
        if group.len() < 3 {
            continue;
        }
        let mut offs: Vec<f64> = group.iter().map(|g| cross(u, mid(g.1, g.2))).collect();
        offs.sort_by(f64::total_cmp);
        let gaps: Vec<f64> = offs.windows(2).map(|w| w[1] - w[0]).collect();
        let (gmin, gmax) = (
            gaps.iter().copied().fold(f64::INFINITY, f64::min),
            gaps.iter().copied().fold(0., f64::max),
        );
        if gmin <= 1e-6 || gmax > 1.35 * gmin {
            continue;
        }
        let e = b.expr(
            "section_hatch",
            &format!("{ang:.0}°"),
            "uniform_parallel_thin_oblique_lines",
            "candidate",
            vec![],
            vec![
                format!("lines:{}", group.len()),
                format!("spacing:{gmin:.3}"),
            ],
            vec!["Sectioned material region boundary not resolved".into()],
        );
        for g in &group {
            done.insert(g.0.clone());
            b.member(
                e,
                &g.0,
                None,
                [0., 1.],
                "hatch_fill",
                member_line_type(cx.lt(&g.0), "hatch_fill"),
                "candidate",
                vec![],
                vec![],
            );
        }
    }
}

/// An arrow with a short collinear shaft pointing at an outline, with no letter
/// near it, is a viewing-direction arrow whose letter is not found.
fn unlabeled_view_arrows(cx: &Ctx, b: &mut Builder, anchors: &mut BTreeMap<usize, P>) {
    let free: Vec<(String, Arrow)> = cx
        .arrows
        .iter()
        .filter(|(id, _)| !b.used(id))
        .map(|(id, a)| (id.clone(), a.clone()))
        .collect();
    for (aid, a) in &free {
        if b.used(aid) {
            continue;
        }
        let shaft = cx
            .lines
            .iter()
            .filter(|(lid, (p, q))| {
                !b.used(lid)
                    && cx.annotation_style(lid)
                    && cross(unit(sub(*q, *p)), a.dir).abs() < 0.03
                    && dist(*p, *q) < 6. * a.len
            })
            .find_map(|(lid, (p, q))| {
                let gap = |x: P| dist(x, a.tip).min(dist(x, base_of(a)));
                let (near, far) = if gap(*p) <= gap(*q) {
                    (*p, *q)
                } else {
                    (*q, *p)
                };
                continues(a, near, far, 0.15 * a.len).then(|| (lid.clone(), far))
            });
        let Some((lid, far)) = shaft else { continue };
        let letter = cx.texts.values().any(|t| {
            !b.used(&t.id)
                && t.lex == Lex::Letters
                && dist(t.center, a.tip).min(dist(t.center, far)) < 3. * cx.scale
        });
        if letter || !ray_hits_structure(cx, b, a.tip, a.dir, 0.4 * a.len) {
            continue;
        }
        let e = b.expr(
            "view_callout",
            "视向箭头",
            "arrow_with_short_shaft_at_outline_without_letter",
            "unknown",
            vec!["view_direction".into()],
            vec![format!("terminator:{aid}")],
            vec!["View letter not found near the arrow".into()],
        );
        b.member(
            e,
            &lid,
            None,
            [0., 1.],
            "view_arrow",
            member_line_type(cx.lt(&lid), "view_arrow"),
            "candidate",
            vec![],
            vec![],
        );
        b.member(
            e,
            aid,
            None,
            [0., 1.],
            "arrowhead",
            "fill",
            "candidate",
            vec![],
            vec![],
        );
        anchors.insert(e, a.tip);
    }
}

// ---------------------------------------------------------------- view titles, notes, sheet texts

#[derive(Clone, Debug)]
struct ViewTitle {
    label: String,
    center: P,
    height: f64,
    kind: &'static str,
    scale: Option<String>,
    sources: Vec<String>,
}

fn view_titles(cx: &Ctx, b: &mut Builder) -> Vec<ViewTitle> {
    let mut out = Vec::new();
    let free = |b: &Builder| -> Vec<TextFact> {
        cx.texts
            .values()
            .filter(|t| !b.used(&t.id))
            .cloned()
            .collect()
    };
    // Letter + 向 on one row (GB/T 4458.1 viewing-direction title).
    for s in free(b).into_iter().filter(|t| t.lex == Lex::ViewSuffix) {
        let letter = free(b)
            .into_iter()
            .filter(|t| {
                matches!(t.lex, Lex::Letters | Lex::Roman) && t.dec.text.chars().count() <= 3
            })
            .filter(|t| {
                let d = sub(s.center, t.center);
                let h = t.height.max(s.height);
                cross(t.dir, d).abs() < 0.6 * h
                    && dot(t.dir, d) > 0.
                    && dot(t.dir, d) < (t.width + s.width) / 2. + 1.5 * h
            })
            .min_by(|x, y| dist(x.center, s.center).total_cmp(&dist(y.center, s.center)));
        let Some(l) = letter else { continue };
        let label = format!("{}向", l.dec.text);
        let e = b.expr(
            "view_callout",
            &label,
            "view_letter_with_direction_suffix",
            "candidate",
            vec!["view_title".into()],
            vec![format!("text:{}", l.id), format!("text:{}", s.id)],
            vec![],
        );
        b.member(
            e,
            &l.id,
            None,
            [0., 1.],
            "view_letter",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        b.member(
            e,
            &s.id,
            None,
            [0., 1.],
            "view_title",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        out.push(ViewTitle {
            label,
            center: mid(l.center, s.center),
            height: l.height.max(s.height),
            kind: "direction_view",
            scale: None,
            sources: vec![l.id.clone(), s.id.clone()],
        });
    }
    // Single text "X向".
    for t in free(b).into_iter().filter(|t| {
        let c: Vec<char> = t.dec.text.chars().filter(|c| !c.is_whitespace()).collect();
        c.len() >= 2
            && c.len() <= 4
            && c[c.len() - 1] == '向'
            && c[..c.len() - 1].iter().all(|x| x.is_ascii_uppercase())
    }) {
        let e = b.expr(
            "view_callout",
            &t.dec.text,
            "view_letter_with_direction_suffix",
            "candidate",
            vec!["view_title".into()],
            vec![format!("text:{}", t.id)],
            vec![],
        );
        b.member(
            e,
            &t.id,
            None,
            [0., 1.],
            "view_title",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        out.push(ViewTitle {
            label: t.dec.text.replace(' ', ""),
            center: t.center,
            height: t.height,
            kind: "direction_view",
            scale: None,
            sources: vec![t.id.clone()],
        });
    }
    // Roman numeral over a scale (detail-view title).
    for r in free(b).into_iter().filter(|t| t.lex == Lex::Roman) {
        let sc = free(b)
            .into_iter()
            .filter(|t| t.lex == Lex::Scale)
            .filter(|t| {
                let d = sub(r.center, t.center);
                let h = r.height.max(t.height);
                d[1] > 0.5 * h && d[1] < 3.2 * h && d[0].abs() < 2. * h
            })
            .min_by(|x, y| dist(x.center, r.center).total_cmp(&dist(y.center, r.center)));
        let Some(s) = sc else { continue };
        let label = format!("{} {}", r.dec.text, s.dec.text);
        let e = b.expr(
            "view_callout",
            &label,
            "detail_numeral_over_scale",
            "candidate",
            vec!["detail_title".into()],
            vec![format!("text:{}", r.id), format!("text:{}", s.id)],
            vec![],
        );
        b.member(
            e,
            &r.id,
            None,
            [0., 1.],
            "view_letter",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        b.member(
            e,
            &s.id,
            None,
            [0., 1.],
            "scale_text",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        let (y0, y1) = (s.center[1], r.center[1]);
        if let Some((lid, _)) = cx.lines.iter().find(|(id, (p, q))| {
            !b.used(id)
                && (p[1] - q[1]).abs() < 1e-6
                && p[1] > y0
                && p[1] < y1
                && p[0].min(q[0]) < r.center[0]
                && p[0].max(q[0]) > r.center[0]
        }) {
            b.member(
                e,
                lid,
                None,
                [0., 1.],
                "view_title",
                member_line_type(cx.lt(lid), "table_line"),
                "candidate",
                vec![],
                vec![],
            );
        }
        out.push(ViewTitle {
            label,
            center: mid(r.center, s.center),
            height: r.height.max(s.height),
            kind: "detail_view",
            scale: Some(s.dec.text.clone()),
            sources: vec![r.id.clone(), s.id.clone()],
        });
    }
    out
}

/// Thin detail circles with a nearby roman numeral and no leader.
fn detail_circles(cx: &Ctx, b: &mut Builder) {
    let circles: Vec<(String, P, f64)> = cx
        .rec
        .keys()
        .filter(|h| cx.kind(h) == "CIRCLE" && !b.used(h) && cx.lt(h) == "thin_continuous")
        .filter_map(|h| cx.arc(h).map(|(c, r, ..)| (h.clone(), c, r)))
        .collect();
    for (h, c, r) in circles {
        let Some(t) = cx
            .texts
            .values()
            .filter(|t| {
                !b.used(&t.id) && t.lex == Lex::Roman && dist(t.center, c) < 3. * r + t.height
            })
            .min_by(|x, y| dist(x.center, c).total_cmp(&dist(y.center, c)))
            .cloned()
        else {
            continue;
        };
        let e = b.expr(
            "view_callout",
            &t.dec.text,
            "detail_circle_with_numeral",
            "candidate",
            vec!["detail_indicator".into()],
            vec![format!("circle:{h}")],
            vec![],
        );
        b.member(
            e,
            &t.id,
            None,
            [0., 1.],
            "view_letter",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        b.member(
            e,
            &h,
            None,
            [0., 1.],
            "detail_circle",
            member_line_type(cx.lt(&h), "detail_circle"),
            "candidate",
            vec![],
            vec![],
        );
    }
}

fn technical_notes(cx: &Ctx, b: &mut Builder) {
    let heads: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| t.lex == Lex::Heading && !b.used(&t.id))
        .cloned()
        .collect();
    for hd in heads {
        let e0 = b.expr(
            "technical_general",
            &hd.dec.text,
            "technical_requirement_heading",
            "candidate",
            vec![],
            vec![format!("text:{}", hd.id)],
            vec![],
        );
        b.member(
            e0,
            &hd.id,
            None,
            [0., 1.],
            "note_text",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        let h = hd.height;
        let mut cands: Vec<TextFact> = cx
            .texts
            .values()
            .filter(|t| {
                !b.used(&t.id)
                    && t.dir[0] > 0.99
                    && t.center[1] < hd.center[1] - 0.5 * h
                    && t.center[0] > hd.center[0] - hd.width - 6. * h
                    && t.center[0] < hd.center[0] + 40. * h
            })
            .cloned()
            .collect();
        cands.sort_by(|a, c| c.center[1].total_cmp(&a.center[1]));
        let mut rows: Vec<Vec<TextFact>> = Vec::new();
        for t in cands {
            match rows
                .iter_mut()
                .find(|r| (r[0].center[1] - t.center[1]).abs() < 0.5 * r[0].height.max(t.height))
            {
                Some(r) => r.push(t),
                None => rows.push(vec![t]),
            }
        }
        // Keep only the contiguous block directly under the heading.
        let mut kept: Vec<Vec<TextFact>> = Vec::new();
        let mut last_y = hd.center[1];
        for mut r in rows {
            let rh = r.iter().map(|t| t.height).fold(0., f64::max);
            if last_y - r[0].center[1] > 2.6 * rh.max(h * 0.7) {
                break;
            }
            last_y = r[0].center[1];
            r.sort_by(|a, c| a.center[0].total_cmp(&c.center[0]));
            kept.push(r);
        }
        let mut items: Vec<Vec<TextFact>> = Vec::new();
        for r in kept {
            if r[0].lex == Lex::ListNumber || items.is_empty() {
                items.push(r);
            } else {
                items.last_mut().unwrap().extend(r);
            }
        }
        for item in items {
            let text: String = item
                .iter()
                .map(|t| t.dec.text.as_str())
                .collect::<Vec<_>>()
                .join("");
            let kind = tx::note_kind(&text);
            let e = b.expr(
                kind,
                &text,
                "numbered_technical_requirement_item",
                "candidate",
                vec![],
                vec![format!("heading:{}", hd.id)],
                vec!["Requirement scope and verification remain to be bound".into()],
            );
            for t in &item {
                b.member(
                    e,
                    &t.id,
                    None,
                    [0., 1.],
                    "note_text",
                    "text",
                    "candidate",
                    vec![],
                    vec![],
                );
            }
            // Short thin strokes inside the row (a dash drawn as a line) belong to the item text.
            let pts: Vec<P> = item
                .iter()
                .flat_map(|t| {
                    [
                        sub(t.center, mul(t.dir, t.width / 2.)),
                        add(t.center, mul(t.dir, t.width / 2.)),
                    ]
                })
                .collect();
            let rh = item.iter().map(|t| t.height).fold(0., f64::max);
            if let Some(rb) = bbox_of(&pts) {
                let strokes: Vec<String> = cx
                    .lines
                    .iter()
                    .filter(|(id, (a, c))| {
                        !b.used(id)
                            && cx.annotation_style(id)
                            && dist(*a, *c) < 1.5 * rh
                            && inside(mid(*a, *c), rb, 0.6 * rh)
                    })
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in strokes {
                    b.member(
                        e,
                        &id,
                        None,
                        [0., 1.],
                        "note_text",
                        member_line_type(cx.lt(&id), "note_text"),
                        "candidate",
                        vec![],
                        vec!["stroke_inside_requirement_row".into()],
                    );
                }
            }
        }
    }
}

fn sheet_texts(cx: &Ctx, b: &mut Builder, def: &Deferred) {
    let margin = 0.3 * cx.scale;
    for (e, bb) in &def.titles {
        let ids: Vec<String> = cx
            .texts
            .values()
            .filter(|t| !b.used(&t.id) && inside(t.center, *bb, margin))
            .map(|t| t.id.clone())
            .collect();
        for id in ids {
            b.member(
                *e,
                &id,
                None,
                [0., 1.],
                "table_text",
                "text",
                "candidate",
                vec![],
                vec!["text_inside_title_table".into()],
            );
        }
    }
    // A drawing number repeated outside the title block identifies the same document.
    let titled: BTreeSet<String> = b
        .exprs
        .iter()
        .filter(|e| e.kind == "title_block")
        .flat_map(|e| e.member_ids.clone())
        .filter_map(|m| {
            b.members
                .iter()
                .find(|x| x.id == m)
                .map(|x| x.source_handle.clone())
        })
        .filter_map(|s| cx.texts.get(&s).map(|t| t.dec.text.clone()))
        .filter(|s| s.chars().count() >= 6 && s.chars().any(|c| c.is_ascii_digit()))
        .collect();
    for t in cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id) && titled.contains(&t.dec.text))
        .cloned()
        .collect::<Vec<_>>()
    {
        let e = b.expr(
            "title_block",
            &format!("图号 {}", t.dec.text),
            "repeated_document_number",
            "candidate",
            vec![],
            vec![format!("text:{}", t.id)],
            vec![],
        );
        b.member(
            e,
            &t.id,
            None,
            [0., 1.],
            "table_text",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        let tb = bbox_of(&cx.points(&t.id)).unwrap_or([
            t.center[0],
            t.center[1],
            t.center[0],
            t.center[1],
        ]);
        for bx in &def.boxes {
            if !b.used(bx) && cx.bbox(bx).is_some_and(|x| bbox_gap(x, tb) < 2. * t.height) {
                let f = b.expr(
                    "sheet_frame",
                    "图号框",
                    "box_on_frame_around_repeated_document_number",
                    "candidate",
                    vec![],
                    vec![format!("text:{}", t.id)],
                    vec![],
                );
                b.member(
                    f,
                    bx,
                    None,
                    [0., 1.],
                    "frame_border",
                    "block",
                    "candidate",
                    vec![],
                    vec!["box_around_repeated_number".into()],
                );
            }
        }
    }
}

// ---------------------------------------------------------------- line-type geometry

fn line_kinds(cx: &Ctx, b: &mut Builder) {
    for (h, r) in &cx.rec {
        if !is_curve_kind(&r.kind) || b.used(h) {
            continue;
        }
        let st = &cx.style[h];
        let (kind, role) = match st.pattern {
            Pattern::DashDot => ("center_axis", "center_line"),
            Pattern::Dashed => ("hidden_contour", "hidden_segment"),
            Pattern::DashDoubleDot => ("phantom_line", "phantom_segment"),
            _ => continue,
        };
        let mut ev = vec![
            format!("pattern:{}", st.pattern.name()),
            format!("layer:{}", st.layer),
        ];
        if let Some(rule) = &cx.base[h].2 {
            ev.push(rule.clone());
        }
        let e = b.expr(
            kind,
            h,
            "piece:actual_linetype_pattern",
            "candidate",
            vec![],
            ev,
            vec![],
        );
        b.member(
            e,
            h,
            None,
            [0., 1.],
            role,
            cx.lt(h),
            "candidate",
            vec![],
            vec![],
        );
    }
}

/// GB/T 4459.1 end view: a thin three-quarter circle concentric with a full circle.
fn thread_arcs(cx: &Ctx, b: &mut Builder) {
    for h in cx.rec.keys().filter(|h| cx.kind(h) == "ARC") {
        let Some((c, rad, _, sweep, _)) = cx.arc(h) else {
            continue;
        };
        if !(250. ..=290.).contains(&sweep.to_degrees())
            || cx.style[h].pattern != Pattern::Continuous
            || cx.lt(h) == "thick_continuous"
        {
            continue;
        }
        let partner = cx
            .rec
            .keys()
            .filter(|k| *k != h && matches!(cx.kind(k), "CIRCLE" | "ARC"))
            .find_map(|k| {
                let (c2, r2, _, sw2, _) = cx.arc(k)?;
                let q = r2 / rad;
                (dist(c2, c) < 1e-4 * rad.max(1.)
                    && sw2.to_degrees() > 300.
                    && ((0.70..=0.95).contains(&q) || (0.75..=0.95).contains(&(1. / q))))
                .then(|| k.clone())
            });
        if let Some(p) = partner {
            let e = b.expr(
                "thread_convention",
                h,
                "piece:three_quarter_thin_circle_concentric_with_full_circle",
                "candidate",
                vec![],
                vec![format!("partner:{p}"), "GB/T 4459.1".into()],
                vec![],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "thread_line",
                member_line_type(cx.lt(h), "thread_line"),
                "candidate",
                vec![],
                vec![format!("concentric:{p}")],
            );
            // GB/T 4459.1 end view: the full circle is the other thread diameter of the same convention.
            let e2 = b.expr(
                "thread_convention",
                &p,
                "piece:full_circle_of_three_quarter_thread_convention",
                "candidate",
                vec![],
                vec![format!("partner:{h}"), "GB/T 4459.1".into()],
                vec![],
            );
            // Shared with the visible contour: the full circle is also the visible edge.
            b.member(
                e2,
                &p,
                None,
                [0., 1.],
                "contour_segment",
                member_line_type(cx.lt(&p), "contour_segment"),
                "candidate",
                vec![],
                vec![format!("concentric:{h}")],
            );
        }
    }
}

/// GB/T 4459.1 blind tapped hole drawn hidden: two coaxial pairs of dashed lines
/// (major and minor diameters, ratio 0.70–0.93) about a centre line, the minor
/// pair deeper than the major pair, plus the dashed thread end line.
fn thread_lines(cx: &Ctx, b: &mut Builder) {
    let axes: Vec<(P, P)> = cx
        .lines
        .iter()
        .filter(|(h, _)| cx.style[*h].pattern == Pattern::DashDot)
        .map(|(_, l)| *l)
        .collect();
    let dashed: Vec<(String, P, P)> = cx
        .lines
        .iter()
        .filter(|(h, _)| cx.style[*h].pattern == Pattern::Dashed)
        .map(|(h, (a, c))| (h.clone(), *a, *c))
        .collect();
    let mut done = BTreeSet::new();
    for (a0, a1) in axes {
        let u = unit(sub(a1, a0));
        let mut by_r: BTreeMap<i64, Vec<(String, f64, f64, f64)>> = BTreeMap::new();
        for (h, a, c) in &dashed {
            if cross(unit(sub(*c, *a)), u).abs() > 0.01 {
                continue;
            }
            let o = cross(u, sub(mid(*a, *c), a0));
            let (s0, s1) = (dot(sub(*a, a0), u), dot(sub(*c, a0), u));
            by_r.entry((o.abs() * 1e3).round() as i64)
                .or_default()
                .push((h.clone(), o, s0.min(s1), s0.max(s1)));
        }
        let radii: Vec<(i64, f64)> = by_r
            .iter()
            .filter(|(_, v)| v.iter().any(|x| x.1 > 0.) && v.iter().any(|x| x.1 < 0.))
            .map(|(k, _)| (*k, *k as f64 / 1e3))
            .filter(|(_, r)| *r > 1e-6)
            .collect();
        for (k1, r1) in &radii {
            for (k2, r2) in &radii {
                if r1 >= r2 || !(0.70..=0.93).contains(&(r1 / r2)) {
                    continue;
                }
                let e1 = &by_r[k1];
                let e2 = &by_r[k2];
                let (m1, m2) = (
                    e1.iter().map(|x| x.3).fold(f64::NEG_INFINITY, f64::max),
                    e2.iter().map(|x| x.3).fold(f64::NEG_INFINITY, f64::max),
                );
                let (n1, n2) = (
                    e1.iter().map(|x| x.2).fold(f64::INFINITY, f64::min),
                    e2.iter().map(|x| x.2).fold(f64::INFINITY, f64::min),
                );
                let overlap = m1.min(m2) - n1.max(n2);
                let deeper = (m1 > m2 + 0.5 * (r2 - r1)) || (n1 < n2 - 0.5 * (r2 - r1));
                if overlap <= 0. || !deeper {
                    continue;
                }
                let end = if m1 > m2 + 0.5 * (r2 - r1) { m2 } else { n2 };
                let mut ids: Vec<String> =
                    e1.iter().chain(e2.iter()).map(|x| x.0.clone()).collect();
                for (h, a, c) in &dashed {
                    let (s0, s1) = (dot(sub(*a, a0), u), dot(sub(*c, a0), u));
                    if dot(unit(sub(*c, *a)), u).abs() < 0.01
                        && (s0 - end).abs() < 1e-3 * r2.max(1.)
                        && (s1 - end).abs() < 1e-3 * r2.max(1.)
                    {
                        let (o0, o1) = (cross(u, sub(*a, a0)), cross(u, sub(*c, a0)));
                        if o0.abs().max(o1.abs()) <= r2 + 1e-6 && o0 * o1 < 0. {
                            ids.push(h.clone());
                        }
                    }
                }
                let key = ids.iter().min().cloned().unwrap_or_default();
                if !done.insert(key) {
                    continue;
                }
                for id in ids {
                    let e = b.expr(
                        "thread_convention",
                        &id,
                        "piece:coaxial_hidden_major_minor_pairs_with_thread_end",
                        "candidate",
                        vec![],
                        vec![format!("radii:{r1:.4}/{r2:.4}"), "GB/T 4459.1".into()],
                        vec![],
                    );
                    b.member(
                        e,
                        &id,
                        None,
                        [0., 1.],
                        "thread_line",
                        cx.lt(&id),
                        "candidate",
                        vec![],
                        vec![],
                    );
                }
            }
        }
    }
}

/// A freehand break or local-view boundary: many short segments whose turning
/// direction alternates while the curve stays close to its chord (GB/T 4457.4 wavy line).
fn wavy(points: &[P]) -> bool {
    if points.len() < 6 {
        return false;
    }
    let total: f64 = points.windows(2).map(|w| dist(w[0], w[1])).sum();
    if total < 1e-9 {
        return false;
    }
    let mut signs = Vec::new();
    for w in points.windows(3) {
        let c = cross(sub(w[1], w[0]), sub(w[2], w[1]));
        if c.abs() > 1e-10 * total * total {
            signs.push(c.signum());
        }
    }
    let changes = signs.windows(2).filter(|s| s[0] != s[1]).count();
    let first = points[0];
    let last = *points.last().unwrap();
    let chord = dist(first, last);
    let amp = if chord > 1e-9 {
        points
            .iter()
            .map(|p| project(*p, first, last).1)
            .fold(0., f64::max)
    } else {
        f64::INFINITY
    };
    changes >= 2 && changes as f64 >= 0.04 * points.len() as f64 && amp < 0.5 * chord
}

fn break_lines(cx: &Ctx, b: &mut Builder) {
    for (h, (pts, closed, bulged)) in &cx.polys {
        if b.used(h)
            || *closed
            || *bulged
            || cx.style[h].pattern != Pattern::Continuous
            || !wavy(pts)
        {
            continue;
        }
        let e = b.expr(
            "break_line",
            h,
            "piece:alternating_turns_close_to_chord",
            "candidate",
            vec![],
            vec![format!("vertices:{}", pts.len())],
            vec![],
        );
        b.member(
            e,
            h,
            None,
            [0., 1.],
            "break_curve",
            "wavy",
            "candidate",
            vec![],
            vec![],
        );
    }
    // Chains of short LINE roots joined end to end.
    let short: BTreeMap<String, (P, P)> = cx
        .lines
        .iter()
        .filter(|(h, (a, c))| {
            !b.used(h)
                && cx.style[*h].pattern == Pattern::Continuous
                && dist(*a, *c) < 0.35 * cx.scale
        })
        .map(|(h, l)| (h.clone(), *l))
        .collect();
    let key = |p: P| ((p[0] * 1e5).round() as i64, (p[1] * 1e5).round() as i64);
    let mut at: BTreeMap<(i64, i64), Vec<String>> = BTreeMap::new();
    for (h, (a, c)) in &short {
        at.entry(key(*a)).or_default().push(h.clone());
        at.entry(key(*c)).or_default().push(h.clone());
    }
    let mut seen = BTreeSet::new();
    for start in short.keys() {
        if seen.contains(start) {
            continue;
        }
        // Collect the connected component.
        let mut comp = vec![start.clone()];
        seen.insert(start.clone());
        let mut i = 0;
        while i < comp.len() {
            let (a, c) = short[&comp[i]];
            for p in [a, c] {
                for n in at.get(&key(p)).into_iter().flatten() {
                    if seen.insert(n.clone()) {
                        comp.push(n.clone());
                    }
                }
            }
            i += 1;
        }
        if comp.len() < 6
            || at
                .iter()
                .any(|(_, v)| v.len() > 2 && v.iter().any(|x| comp.contains(x)))
        {
            continue;
        }
        // Order the chain from an end point.
        let deg1 = comp.iter().find_map(|h| {
            let (a, c) = short[h];
            [a, c].into_iter().find(|p| at[&key(*p)].len() == 1)
        });
        let Some(mut cur) = deg1 else { continue };
        let mut pts = vec![cur];
        let mut used = BTreeSet::new();
        while let Some(n) = at[&key(cur)].iter().find(|x| !used.contains(*x)).cloned() {
            used.insert(n.clone());
            let (a, c) = short[&n];
            cur = if key(a) == key(cur) { c } else { a };
            pts.push(cur);
        }
        if used.len() != comp.len() || !wavy(&pts) {
            continue;
        }
        let e = b.expr(
            "break_line",
            &comp[0],
            "piece:chained_short_segments_alternating_turns",
            "candidate",
            vec![],
            vec![format!("segments:{}", comp.len())],
            vec![],
        );
        for h in &comp {
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "break_curve",
                "wavy",
                "candidate",
                vec![],
                vec![],
            );
        }
    }
    // A freehand curve (many short straight segments, at least one inflection, not
    // an arc) on the same layer as recognised wavy lines is a further boundary curve.
    let mut layers_with_breaks: BTreeMap<String, usize> = BTreeMap::new();
    for m in b.members.iter().filter(|m| m.member_role == "break_curve") {
        *layers_with_breaks
            .entry(cx.style[&m.source_handle].layer.clone())
            .or_default() += 1;
    }
    let extra: Vec<String> = cx
        .polys
        .iter()
        .filter(|(h, (pts, closed, bulged))| {
            let spline = cx
                .rec
                .get(*h)
                .is_some_and(|r| r.kind == "POLYLINE" && int(r, 70) & 4 != 0);
            !b.used(h)
                && !*closed
                && !*bulged
                && layers_with_breaks
                    .get(&cx.style[*h].layer)
                    .is_some_and(|n| *n >= 2)
                && ((spline && pts.len() >= 5)
                    || pts.len() >= 8 && {
                        let mut signs = Vec::new();
                        for w in pts.windows(3) {
                            let c = cross(sub(w[1], w[0]), sub(w[2], w[1]));
                            if c.abs() > 1e-12 {
                                signs.push(c.signum());
                            }
                        }
                        signs.windows(2).any(|s| s[0] != s[1])
                    })
        })
        .map(|(h, _)| h.clone())
        .collect();
    for h in extra {
        let e = b.expr(
            "break_line",
            &h,
            "piece:freehand_curve_on_layer_of_recognised_wavy_lines",
            "candidate",
            vec![],
            vec![format!("layer:{}", cx.style[&h].layer)],
            vec!["Classified by drawing-internal layer consistency, not by the curve alone".into()],
        );
        b.member(
            e,
            &h,
            None,
            [0., 1.],
            "break_curve",
            "wavy",
            "candidate",
            vec![],
            vec![],
        );
    }
    // Boundary of a local section (GB/T 4458.1 / 4457.4): an open free curve whose
    // vertices lie on a section-hatch boundary and whose ends lie on outline or axis.
    let tol = 1e-3 * cx.scale.max(1.);
    let hatch: Vec<P> = cx
        .rec
        .iter()
        .filter(|(_, r)| r.kind == "HATCH")
        .flat_map(|(h, _)| cx.points(h))
        .collect();
    let on_axis = |p: P| {
        cx.lines.iter().any(|(id, (a, c))| {
            cx.style[id].pattern == Pattern::DashDot && on_seg(p, *a, *c, tol).is_some()
        })
    };
    let empty = BTreeSet::new();
    let bounds: Vec<String> = cx
        .polys
        .iter()
        .filter(|(h, (pts, closed, bulged))| {
            !b.used(h)
                && !*closed
                && !*bulged
                && pts.len() >= 5
                && cx.style[*h].pattern == Pattern::Continuous
                && cx.lt(h) != "thick_continuous"
                && pts
                    .iter()
                    .filter(|p| hatch.iter().any(|q| dist(**p, *q) < tol))
                    .count()
                    >= 3
                && [pts[0], pts[pts.len() - 1]]
                    .iter()
                    .all(|p| reaches_structure(cx, b, *p, &empty) || on_axis(*p))
        })
        .map(|(h, _)| h.clone())
        .collect();
    for h in bounds {
        let e = b.expr(
            "break_line",
            &h,
            "piece:free_curve_bounding_hatched_local_section",
            "candidate",
            vec![],
            vec![
                "vertices_on_section_hatch_boundary".into(),
                "ends_on_outline_or_axis".into(),
            ],
            vec!["Local section boundary; its occluded continuation is not reconstructed".into()],
        );
        b.member(
            e,
            &h,
            None,
            [0., 1.],
            "break_curve",
            member_line_type(cx.lt(&h), "break_curve"),
            "candidate",
            vec![],
            vec![],
        );
    }
}

// ---------------------------------------------------------------- visible structure with bounded intervals

/// Visible contour candidates are continuous, not thin by convention or by an
/// explicit thin colour, not exploded (ByBlock) and not exclusive annotation
/// members. A LINE contributes only the span between its structural junctions;
/// a portion coincident with a higher-precedence structural source (thick layer
/// by convention, GB/T 14665-2012 4.3) is left to that source.
fn structure(cx: &Ctx, b: &mut Builder, cover: &topology::Cover) {
    let mut cand: BTreeMap<String, u8> = BTreeMap::new();
    for (h, r) in &cx.rec {
        if !matches!(
            r.kind.as_str(),
            "LINE" | "ARC" | "CIRCLE" | "LWPOLYLINE" | "POLYLINE"
        ) {
            continue;
        }
        if !cx.structural_style(h)
            || cx.arrows.contains_key(h)
            || cx.inline_arrows.contains_key(h)
            || exclusive(b, h)
        {
            continue;
        }
        cand.insert(
            h.clone(),
            if cx.lt(h) == "thick_continuous" { 2 } else { 1 },
        );
    }
    let breaks: BTreeSet<String> = b
        .members
        .iter()
        .filter(|m| m.member_role == "break_curve")
        .map(|m| m.source_handle.clone())
        .collect();
    let partner = |id: &str| cand.contains_key(id) || breaks.contains(id);
    // A contour ending on a centre line (half view, end face up to the axis) is joined there.
    let axis = |id: &str| {
        cx.style
            .get(id)
            .is_some_and(|s| s.pattern == Pattern::DashDot)
    };
    let mut junctions: BTreeMap<String, Vec<(f64, String)>> = BTreeMap::new();
    // Parameters where a source meets a partner's end with its own end (a corner,
    // not a T-junction).
    let mut corners: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    // Written out so a NaN parameter is not an end (a range test would call it one).
    #[allow(clippy::manual_range_contains)]
    let at_end = |t: Option<f64>| t.is_some_and(|x| x < 1e-6 || x > 1. - 1e-6);
    let mut overlaps: BTreeMap<String, Vec<([f64; 2], String)>> = BTreeMap::new();
    for rel in &cx.src.relations {
        let Some(to) = rel.to_id.as_deref() else {
            continue;
        };
        let from = rel.from_id.as_str();
        match rel.kind.as_str() {
            "endpoint_on_curve" | "interior_crossing" => {
                if rel.kind == "endpoint_on_curve"
                    && cand.contains_key(from)
                    && axis(to)
                    && let Some(t) = rel.from_parameter
                {
                    junctions
                        .entry(from.into())
                        .or_default()
                        .push((t, format!("endpoint_on_axis:{to}")));
                }
                if cand.contains_key(from)
                    && partner(to)
                    && let Some(t) = rel.from_parameter
                {
                    junctions
                        .entry(from.into())
                        .or_default()
                        .push((t, rel.id.clone()));
                    if rel.kind == "endpoint_on_curve" && at_end(rel.to_parameter) {
                        corners.entry(from.into()).or_default().push(t);
                    }
                }
                if cand.contains_key(to)
                    && partner(from)
                    && let Some(t) = rel.to_parameter
                {
                    junctions
                        .entry(to.into())
                        .or_default()
                        .push((t, rel.id.clone()));
                    if rel.kind == "endpoint_on_curve" && at_end(rel.to_parameter) {
                        corners.entry(to.into()).or_default().push(t);
                    }
                }
            }
            "collinear_overlap" if cand.contains_key(from) && cand.contains_key(to) => {
                for (x, y) in [(from, to), (to, from)] {
                    if let (Some((a, c)), Some((p, q))) = (cx.lines.get(x), cx.lines.get(y)) {
                        let (t0, t1) = (project(*p, *a, *c).0, project(*q, *a, *c).0);
                        let iv = [t0.min(t1).max(0.), t0.max(t1).min(1.)];
                        if iv[1] > iv[0] {
                            overlaps.entry(x.into()).or_default().push((iv, y.into()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Junctions with polyline partners (break lines, structural polylines).
    let poly_partners: Vec<(String, Vec<P>)> = cx
        .polys
        .iter()
        .filter(|(h, _)| partner(h))
        .map(|(h, (p, _, _))| (h.clone(), p.clone()))
        .collect();
    let tol = 1e-4 * cx.scale.max(1.);
    for (h, (a, c)) in cx.lines.iter().filter(|(h, _)| cand.contains_key(*h)) {
        for (ph, pts) in &poly_partners {
            for (t, p) in [(0., *a), (1., *c)] {
                if pts.windows(2).any(|w| on_seg(p, w[0], w[1], tol).is_some()) {
                    junctions
                        .entry(h.clone())
                        .or_default()
                        .push((t, format!("endpoint_on_polyline:{ph}")));
                }
            }
            for p in [pts[0], *pts.last().unwrap()] {
                if let Some(t) = on_seg(p, *a, *c, tol) {
                    junctions
                        .entry(h.clone())
                        .or_default()
                        .push((t, format!("polyline_end_on_line:{ph}")));
                }
            }
        }
    }
    // A free end within a small drafting gap of another candidate counts once.
    let gap = 0.05 * (cx.scale / 3.5).max(1.);
    for (h, (a, c)) in cx.lines.iter().filter(|(h, _)| cand.contains_key(*h)) {
        let js: Vec<f64> = junctions
            .get(h)
            .map(|v| v.iter().map(|x| x.0).collect())
            .unwrap_or_default();
        for (t, p) in [(0., *a), (1., *c)] {
            let side_has = js.iter().any(|x| if t == 0. { *x < 0.5 } else { *x > 0.5 });
            if side_has {
                continue;
            }
            let near = cand.keys().filter(|k| *k != h).find(|k| {
                cx.lines
                    .get(*k)
                    .is_some_and(|(p0, p1)| on_seg(p, *p0, *p1, gap).is_some())
                    || cx.arc(k).is_some_and(|(c0, r0, s0, sw, full)| {
                        let ang = ((p[1] - c0[1]).atan2(p[0] - c0[0]) - s0)
                            .rem_euclid(std::f64::consts::TAU);
                        (dist(p, c0) - r0).abs() < gap
                            && (full
                                || ang <= sw + gap / r0.max(1e-9)
                                || std::f64::consts::TAU - ang < gap / r0.max(1e-9))
                    })
            });
            if let Some(k) = near {
                let ends_of = |k: &str| -> Vec<P> {
                    if let Some((p0, p1)) = cx.lines.get(k) {
                        return vec![*p0, *p1];
                    }
                    cx.arc(k)
                        .map(|(c0, r0, s0, sw, _)| {
                            vec![
                                add(c0, mul([s0.cos(), s0.sin()], r0)),
                                add(c0, mul([(s0 + sw).cos(), (s0 + sw).sin()], r0)),
                            ]
                        })
                        .unwrap_or_default()
                };
                if ends_of(k).iter().any(|q| dist(*q, p) < gap) {
                    corners.entry(h.clone()).or_default().push(t);
                }
                junctions
                    .entry(h.clone())
                    .or_default()
                    .push((t, format!("near_endpoint_gap:{k}")));
            }
        }
    }
    // Arc ends within the same drafting gap of a candidate line end or line.
    for h in cand.keys().filter(|h| cx.kind(h) == "ARC") {
        let Some((c0, r0, s0, sw, _)) = cx.arc(h) else {
            continue;
        };
        let js: Vec<f64> = junctions
            .get(h)
            .map(|v| v.iter().map(|x| x.0).collect())
            .unwrap_or_default();
        for (t, ang) in [(0., s0), (1., s0 + sw)] {
            if js.iter().any(|x| (x - t).abs() < 1e-6) {
                continue;
            }
            let p = add(c0, mul([ang.cos(), ang.sin()], r0));
            let near = cand.keys().filter(|k| *k != h).find(|k| {
                cx.lines
                    .get(*k)
                    .is_some_and(|(p0, p1)| on_seg(p, *p0, *p1, gap).is_some())
            });
            if let Some(k) = near {
                junctions
                    .entry(h.clone())
                    .or_default()
                    .push((t, format!("near_endpoint_gap:{k}")));
            }
        }
    }
    // Thin continuous lines (by the checked convention) joined to structure at
    // both ends, not used by any annotation, are structural thin lines
    // (GB/T 4457.4: transition lines, thread roots and similar).
    for h in cx.lines.keys() {
        if cand.contains_key(h)
            || b.used(h)
            || cx.lt(h) != "thin_continuous"
            || cx.style[h].layer != "02"
        {
            continue;
        }
        let mut ends = [false, false];
        for rel in cx
            .src
            .relations
            .iter()
            .filter(|r| r.kind == "endpoint_on_curve")
        {
            if rel.from_id == *h
                && rel.to_id.as_deref().is_some_and(|t| cand.contains_key(t))
                && let Some(t) = rel.from_parameter
            {
                ends[(t > 0.5) as usize] = true;
            }
        }
        if ends == [true, true] {
            let e = b.expr(
                "visible_contour",
                h,
                "piece:thin_line_between_structural_junctions",
                "candidate",
                vec![],
                vec!["GB/T 4457.4 thin continuous structural line".into()],
                vec![
                    "Thin structural role (transition, thread root, boundary) not further resolved"
                        .into(),
                ],
            );
            b.member(
                e,
                h,
                None,
                [0., 1.],
                "contour_segment",
                "thin_continuous",
                "candidate",
                vec![],
                vec![],
            );
        }
    }
    let partners = topology::line_partners(cx, &cand, &junctions);
    for (h, prec) in &cand {
        let kind = cx.kind(h).to_string();
        let js = junctions.get(h).cloned().unwrap_or_default();
        let mut ev = vec![format!("junctions:{}", js.len())];
        if let Some(rule) = &cx.base[h].2 {
            ev.push(rule.clone());
        }
        if kind == "LINE" {
            // Distinct junction parameters: incidence is reported from both sources.
            let mut ts: Vec<f64> = js.iter().map(|x| x.0).collect();
            // An end without a junction whose interruption is explained (a text
            // the line is broken for, a local section) keeps the line to that end.
            if let Some((a, c)) = cx.lines.get(h) {
                for (t_end, p, d) in [(0., *a, unit(sub(*a, *c))), (1., *c, unit(sub(*c, *a)))] {
                    if ts.iter().any(|x| (x - t_end).abs() < 1e-6) {
                        continue;
                    }
                    if let Some(cause) = topology::interruption(cx, cover, h, p, d, &partners) {
                        ts.push(t_end);
                        ev.push(format!("explained_end:{}:{t_end}", cause.kind));
                    }
                }
            }
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
            let open = |t: f64| {
                cx.src.relations.iter().any(|r| {
                    r.kind == "open_endpoint"
                        && r.from_id == *h
                        && r.from_parameter.is_some_and(|x| (x - t).abs() < 1e-9)
                })
            };
            let mut spans: Vec<[f64; 2]> = if ts.len() >= 2 {
                vec![[ts[0], ts[ts.len() - 1]]]
            } else if *prec == 2 {
                vec![[0., 1.]]
            } else if ts.len() == 1
                && !b.used(h)
                && corners
                    .get(h)
                    .is_some_and(|c| c.iter().any(|x| (x - ts[0]).abs() < 1e-6))
                && ((ts[0] < 1e-6 && open(1.)) || (ts[0] > 1. - 1e-6 && open(0.)))
            {
                // Continuing an outline from a corner, free at the other end and used by
                // no annotation: the outline runs on to its free end (cut by a view
                // boundary). A stub from a T-junction stays unresolved.
                ev.push("one_junction_and_open_end".into());
                vec![[0., 1.]]
            } else {
                vec![]
            };
            for (iv, other) in overlaps.get(h).into_iter().flatten() {
                if cand.get(other).is_some_and(|p| p > prec)
                    && spans
                        .iter()
                        .any(|s| iv[0] <= s[0] + 1e-9 && iv[1] >= s[1] - 1e-9)
                {
                    ev.push(format!("duplicate_of_higher_precedence:{other}"));
                    continue;
                }
                if cand.get(other).is_some_and(|p| p > prec) {
                    spans = spans
                        .into_iter()
                        .flat_map(|s| {
                            let mut out = Vec::new();
                            if iv[0] > s[0] + 1e-9 {
                                out.push([s[0], iv[0].min(s[1])]);
                            }
                            if iv[1] < s[1] - 1e-9 {
                                out.push([iv[1].max(s[0]), s[1]]);
                            }
                            out.into_iter()
                                .filter(|x| x[1] - x[0] > 1e-9)
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    ev.push(format!("coincident_higher_precedence:{other}"));
                }
            }
            if spans.is_empty() {
                continue;
            }
            let e = b.expr(
                "visible_contour",
                h,
                "piece:bounded_span_between_structural_junctions",
                "candidate",
                vec![],
                ev,
                vec![],
            );
            let rels: Vec<String> = js.iter().map(|x| x.1.clone()).collect();
            for s in spans {
                b.member(
                    e,
                    h,
                    None,
                    s,
                    "contour_segment",
                    member_line_type(cx.lt(h), "contour_segment"),
                    "candidate",
                    rels.clone(),
                    vec![],
                );
            }
        } else {
            let closed = kind == "CIRCLE" || cx.polys.get(h).is_some_and(|p| p.1);
            if closed
                || !js.is_empty()
                || *prec == 2
                || cx.polys.contains_key(h)
                    && !cx.polys[h].2
                    && poly_partners.iter().any(|(ph, _)| ph != h)
                    && {
                        let pts = &cx.polys[h].0;
                        cand.keys().filter(|k| *k != h).any(|k| {
                            cx.lines.get(k).is_some_and(|(p, q)| {
                                [pts[0], *pts.last().unwrap()]
                                    .iter()
                                    .any(|e| on_seg(*e, *p, *q, tol).is_some())
                            })
                        })
                    }
            {
                let e = b.expr(
                    "visible_contour",
                    h,
                    "piece:closed_or_connected_curve",
                    "candidate",
                    vec![],
                    ev,
                    vec![],
                );
                b.member(
                    e,
                    h,
                    None,
                    [0., 1.],
                    "contour_segment",
                    member_line_type(cx.lt(h), "contour_segment"),
                    "candidate",
                    js.iter().map(|x| x.1.clone()).collect(),
                    vec![],
                );
            }
        }
    }
}

/// A contour span drawn again over another contour span of equal or higher
/// precedence (GB/T 14665-2012 4.3) keeps its contour use but names the covering
/// primary, so each physical segment is drawn once. Primary: highest precedence,
/// then longest, then smallest handle; chains name their final primary.
fn duplicate_spans(cx: &Ctx, b: &mut Builder) {
    let tol = 1e-3 * cx.scale.max(1.);
    let contour: BTreeSet<&str> = b
        .exprs
        .iter()
        .filter(|e| e.kind == "visible_contour")
        .map(|e| e.id.as_str())
        .collect();
    let spans: Vec<(usize, String, u8, P, P)> = b
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            m.member_role == "contour_segment" && contour.contains(m.expression_id.as_str())
        })
        .filter_map(|(i, m)| {
            let (a, c) = cx.lines.get(&m.source_handle)?;
            let (p, q) = (lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1]));
            let prec = if cx.lt(&m.source_handle) == "thick_continuous" {
                2
            } else {
                1
            };
            (dist(p, q) > tol).then(|| (i, m.source_handle.clone(), prec, p, q))
        })
        .collect();
    let mut dup: BTreeMap<usize, usize> = BTreeMap::new();
    for (x, (_, h, ph, a, c)) in spans.iter().enumerate() {
        let mut best: Option<(usize, u8, f64)> = None;
        for (y, (_, k, pk, p, q)) in spans.iter().enumerate() {
            if k == h || pk < ph || cross(unit(sub(*q, *p)), unit(sub(*c, *a))).abs() > 1e-6 {
                continue;
            }
            if on_seg(*a, *p, *q, tol).is_none() || on_seg(*c, *p, *q, tol).is_none() {
                continue;
            }
            let exact = on_seg(*p, *a, *c, tol).is_some() && on_seg(*q, *a, *c, tol).is_some();
            if exact && pk == ph && k > h {
                continue;
            }
            let lk = dist(*p, *q);
            let better = best.is_none_or(|(z, bp, bl)| {
                *pk > bp || (*pk == bp && (lk > bl + tol || (lk > bl - tol && k < &spans[z].1)))
            });
            if better {
                best = Some((y, *pk, lk));
            }
        }
        if let Some((y, _, _)) = best {
            dup.insert(x, y);
        }
    }
    let direct = dup.clone();
    for y in dup.values_mut() {
        for _ in 0..direct.len() {
            let Some(next) = direct.get(y) else { break };
            *y = *next;
        }
    }
    for (x, y) in dup {
        let primary = spans[y].1.clone();
        b.members[spans[x].0]
            .evidence_ids
            .insert(0, format!("duplicate_of:{primary}"));
    }
}

// ---------------------------------------------------------------- views

const GEOMETRY: [&str; 7] = [
    "visible_contour",
    "hidden_contour",
    "center_axis",
    "phantom_line",
    "thread_convention",
    "break_line",
    "section_hatch",
];
const SHEET: [&str; 6] = [
    "title_block",
    "sheet_frame",
    "technical_general",
    "material_heat",
    "process_requirement",
    "protection_requirement",
];

struct Region {
    id: String,
    kind: &'static str,
    label: String,
    bbox: [f64; 4],
    scale: Option<String>,
    label_sources: Vec<String>,
    state: &'static str,
    evidence: Vec<String>,
    nodes: usize,
}

fn member_points(cx: &Ctx, m: &MdExpressionMember) -> Vec<P> {
    if let Some((a, c)) = cx.lines.get(&m.source_handle) {
        return vec![lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1])];
    }
    cx.points(&m.source_handle)
}

fn expr_bbox(cx: &Ctx, b: &Builder, e: usize) -> Option<[f64; 4]> {
    let ids: BTreeSet<&String> = b.exprs[e].member_ids.iter().collect();
    let pts: Vec<P> = b
        .members
        .iter()
        .filter(|m| ids.contains(&m.id))
        .flat_map(|m| member_points(cx, m))
        .collect();
    bbox_of(&pts)
}

/// Cluster geometry pieces by incidence and proximity; attach view titles to the
/// region directly below them; the largest untitled region is the principal view.
fn views(
    cx: &Ctx,
    b: &Builder,
    titles: &[ViewTitle],
    anchors: &BTreeMap<usize, P>,
) -> (
    Vec<Region>,
    BTreeMap<usize, String>,
    BTreeMap<usize, String>,
) {
    let mut nodes: Vec<(String, [f64; 4])> = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, e) in b.exprs.iter().enumerate() {
        if !GEOMETRY.contains(&e.kind.as_str()) {
            continue;
        }
        for m in b
            .members
            .iter()
            .filter(|m| m.expression_id == b.exprs[i].id)
        {
            if seen.insert(m.source_handle.clone())
                && let Some(bb) = bbox_of(&member_points(cx, m))
            {
                nodes.push((m.source_handle.clone(), bb));
            }
        }
    }
    let n = nodes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut c = x;
        while p[c] != r {
            let nx = p[c];
            p[c] = r;
            c = nx;
        }
        r
    }
    let g = 1.2 * cx.scale;
    for i in 0..n {
        for j in i + 1..n {
            if bbox_gap(nodes[i].1, nodes[j].1) <= g {
                let (a, c) = (find(&mut parent, i), find(&mut parent, j));
                if a != c {
                    parent[a] = c;
                }
            }
        }
    }
    let mut clusters: BTreeMap<usize, (Vec<usize>, [f64; 4])> = BTreeMap::new();
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        let r = find(&mut parent, i);
        let entry = clusters.entry(r).or_insert((vec![], nodes[i].1));
        entry.0.push(i);
        entry.1 = bbox_union(entry.1, nodes[i].1);
    }
    let mut cl: Vec<(Vec<usize>, [f64; 4])> = clusters.into_values().collect();
    // Merge stray fragments (fewer than three pieces) into the nearest region
    // within four proximity gaps; each merge reduces the region count.
    loop {
        let mut merged = false;
        for k in 0..cl.len() {
            if cl[k].0.len() >= 3 {
                continue;
            }
            let small = cl[k].1;
            let target = cl
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != k)
                .map(|(i, c)| (i, bbox_gap(c.1, small)))
                .min_by(|x, y| x.1.total_cmp(&y.1));
            if let Some((t, gap)) = target.filter(|x| x.1 <= 4. * g) {
                let _ = gap;
                let moved = cl.remove(k);
                let t = if t > k { t - 1 } else { t };
                cl[t].0.extend(moved.0);
                cl[t].1 = bbox_union(cl[t].1, moved.1);
                merged = true;
                break;
            }
        }
        if !merged {
            break;
        }
    }
    #[allow(clippy::unnecessary_sort_by)]
    cl.sort_by(|a, c| c.0.len().cmp(&a.0.len()));
    let mut regions: Vec<Region> = Vec::new();
    let mut taken = BTreeSet::new();
    let mut title_of: BTreeMap<usize, usize> = BTreeMap::new();
    for (ti, t) in titles.iter().enumerate() {
        let h = t.height;
        let best = cl
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                !taken.contains(i)
                    && t.center[0] >= c.1[0] - 3. * h
                    && t.center[0] <= c.1[2] + 3. * h
                    && t.center[1] >= c.1[3] - 0.5 * h
                    && t.center[1] - c.1[3] <= 12. * h
            })
            .min_by(|x, y| (t.center[1] - x.1.1[3]).total_cmp(&(t.center[1] - y.1.1[3])))
            .map(|(i, _)| i);
        if let Some(i) = best {
            taken.insert(i);
            title_of.insert(i, ti);
        }
    }
    let mut extra = 0;
    let principal = (0..cl.len()).find(|i| !title_of.contains_key(i));
    for (i, (members, bb)) in cl.iter().enumerate() {
        let has_break = members
            .iter()
            .any(|&k| b.kinds_of(&nodes[k].0).contains("break_line"));
        let (id, kind, label, scale, sources, state, evidence) = if let Some(&ti) = title_of.get(&i)
        {
            let t = &titles[ti];
            let kind = if t.kind == "detail_view" {
                "detail_view"
            } else if has_break {
                "partial_view"
            } else {
                "direction_view"
            };
            (
                format!("view:{}", t.label.replace(' ', "")),
                kind,
                t.label.clone(),
                t.scale.clone(),
                t.sources.clone(),
                "candidate",
                vec!["title_directly_above_region".to_string()],
            )
        } else if Some(i) == principal {
            (
                "view:principal".to_string(),
                "principal",
                "主视图".to_string(),
                None,
                vec![],
                "candidate",
                vec!["largest_untitled_region".to_string()],
            )
        } else {
            extra += 1;
            (
                format!("view:untitled-{extra}"),
                "partial_view",
                format!("未命名视图 {extra}"),
                None,
                vec![],
                "unknown",
                vec!["untitled_region".to_string()],
            )
        };
        regions.push(Region {
            id,
            kind,
            label,
            bbox: *bb,
            scale,
            label_sources: sources,
            state,
            evidence,
            nodes: members.len(),
        });
    }
    let nodes_ref = &nodes;
    let node_view: BTreeMap<String, String> = cl
        .iter()
        .enumerate()
        .flat_map(|(i, (ms, _))| ms.iter().map(move |&k| (nodes_ref[k].0.clone(), i)))
        .map(|(s, i)| (s, regions[i].id.clone()))
        .collect();
    let nearest = |p: [f64; 4]| -> Option<String> {
        regions
            .iter()
            .min_by(|x, y| {
                bbox_gap(x.bbox, p)
                    .total_cmp(&bbox_gap(y.bbox, p))
                    .then(y.nodes.cmp(&x.nodes))
            })
            .map(|r| r.id.clone())
    };
    let margin = 0.5 * cx.scale;
    // The region holding most of the given points; the nearest one when none does.
    let assign = |pts: &[P]| -> Option<String> {
        let bb = bbox_of(pts)?;
        regions
            .iter()
            .map(|r| {
                (
                    pts.iter().filter(|p| inside(**p, r.bbox, margin)).count(),
                    bbox_gap(r.bbox, bb),
                    r,
                )
            })
            .max_by(|x, y| {
                x.0.cmp(&y.0)
                    .then(y.1.total_cmp(&x.1))
                    .then(x.2.nodes.cmp(&y.2.nodes))
            })
            .map(|x| x.2.id.clone())
    };
    let titled: BTreeMap<String, String> = regions
        .iter()
        .flat_map(|r| r.label_sources.iter().map(|s| (s.clone(), r.id.clone())))
        .collect();
    let mut map = BTreeMap::new();
    for (i, e) in b.exprs.iter().enumerate() {
        let general = e.qualifiers.iter().any(|q| q == "general_requirement");
        let v = if SHEET.contains(&e.kind.as_str()) || general {
            Some("view:sheet".to_string())
        } else if GEOMETRY.contains(&e.kind.as_str()) {
            b.members
                .iter()
                .find(|m| m.expression_id == e.id)
                .and_then(|m| node_view.get(&m.source_handle).cloned())
        } else if e
            .qualifiers
            .iter()
            .any(|q| q == "view_title" || q == "detail_title")
        {
            b.members
                .iter()
                .filter(|m| m.expression_id == e.id)
                .find_map(|m| titled.get(&m.source_handle).cloned())
                .or_else(|| expr_bbox(cx, b, i).and_then(nearest))
        } else if let Some(p) = anchors.get(&i) {
            assign(&[*p])
        } else {
            let pts: Vec<P> = b
                .members
                .iter()
                .filter(|m| m.expression_id == e.id)
                .flat_map(|m| member_points(cx, m))
                .collect();
            assign(&pts)
        };
        if let Some(v) = v {
            map.insert(i, v);
        }
    }
    // A callout refers to the view its letter or numeral names.
    let mut refs = BTreeMap::new();
    for (i, e) in b.exprs.iter().enumerate() {
        if e.kind != "view_callout"
            || e.qualifiers
                .iter()
                .any(|q| q == "view_title" || q == "detail_title")
        {
            continue;
        }
        let key = e.label.split_whitespace().next().unwrap_or("");
        if key.is_empty() {
            continue;
        }
        if let Some(r) = regions.iter().find(|r| {
            r.kind != "sheet"
                && (r.label == format!("{key}向") || r.label.starts_with(&format!("{key} ")))
        }) {
            refs.insert(i, r.id.clone());
        }
    }
    // One letterless viewing arrow and one lettered direction view no arrow names:
    // the arrow is that view's indication by elimination (kept as a candidate).
    let unlabeled: Vec<usize> = b
        .exprs
        .iter()
        .enumerate()
        .filter(|(_, e)| e.rule == "arrow_with_short_shaft_at_outline_without_letter")
        .map(|(i, _)| i)
        .collect();
    let named: BTreeSet<&String> = refs.values().collect();
    let open: Vec<&Region> = regions
        .iter()
        .filter(|r| r.label.ends_with('向') && !named.contains(&r.id))
        .collect();
    if let ([i], [r]) = (unlabeled.as_slice(), open.as_slice()) {
        refs.insert(*i, format!("{}:by_elimination", r.id));
    }
    regions.push(Region {
        id: "view:sheet".into(),
        kind: "sheet",
        label: "图纸".into(),
        bbox: cx.extent,
        scale: None,
        label_sources: vec![],
        state: "observed",
        evidence: vec!["drawing_extent".into()],
        nodes: 0,
    });
    (regions, map, refs)
}

// ---------------------------------------------------------------- finalize

fn kind_cn(kind: &str) -> &'static str {
    match kind {
        "visible_contour" => "可见轮廓",
        "hidden_contour" => "不可见轮廓",
        "center_axis" => "轴线与中心线",
        "phantom_line" => "假想线",
        "thread_convention" => "螺纹规定画法",
        "break_line" => "断裂与局部边界",
        "section_hatch" => "剖面线",
        _ => "表达",
    }
}

fn finalize(
    b: &Builder,
    vmap: &BTreeMap<usize, String>,
    regions: &[Region],
    refs: &BTreeMap<usize, String>,
) -> Builder {
    let label_of: BTreeMap<&str, &str> = regions
        .iter()
        .map(|r| (r.id.as_str(), r.label.as_str()))
        .collect();
    let mut nb = Builder::default();
    let mut groups: BTreeMap<(String, String), usize> = BTreeMap::new();
    let members: BTreeMap<&str, &MdExpressionMember> =
        b.members.iter().map(|m| (m.id.as_str(), m)).collect();
    let mut index = Vec::with_capacity(b.exprs.len());
    for (i, e) in b.exprs.iter().enumerate() {
        let view = vmap.get(&i).cloned();
        let piece = e.rule.strip_prefix("piece:");
        let ni = if let Some(rule) = piece {
            let key = (e.kind.clone(), view.clone().unwrap_or_default());
            let idx = *groups.entry(key).or_insert_with(|| {
                let label = format!(
                    "{} · {}",
                    label_of
                        .get(view.as_deref().unwrap_or(""))
                        .copied()
                        .unwrap_or("未定视图"),
                    kind_cn(&e.kind)
                );
                nb.expr(
                    &e.kind,
                    &label,
                    "grouped_line_type_pieces_by_view",
                    "candidate",
                    vec![],
                    vec![],
                    vec!["Part feature identity of each piece requires projection evidence".into()],
                )
            });
            for ev in std::iter::once(format!("rule:{rule}")).chain(
                e.evidence_ids
                    .iter()
                    .filter(|x| x.starts_with("GB/T") || x.starts_with("pattern:"))
                    .cloned(),
            ) {
                if !nb.exprs[idx].evidence_ids.contains(&ev) {
                    nb.exprs[idx].evidence_ids.push(ev);
                }
            }
            idx
        } else {
            nb.expr(
                &e.kind,
                &e.label,
                &e.rule,
                &e.state,
                e.qualifiers.clone(),
                e.evidence_ids.clone(),
                e.unknowns.clone(),
            )
        };
        nb.exprs[ni].view_id = view.clone();
        index.push(ni);
        for mid in &e.member_ids {
            let m = members[mid.as_str()];
            let mut ev = m.evidence_ids.clone();
            if piece.is_some() {
                ev.extend(
                    e.evidence_ids
                        .iter()
                        .filter(|x| {
                            x.starts_with("coincident_higher_precedence")
                                || x.starts_with("junctions:")
                                || x.starts_with("partner:")
                                || x.starts_with("radii:")
                        })
                        .cloned(),
                );
            }
            nb.member(
                ni,
                &m.source_handle,
                m.native_member_id.clone(),
                [m.interval[0], m.interval[1]],
                &m.member_role,
                &m.line_type,
                &m.state,
                m.relation_ids.clone(),
                ev,
            );
        }
    }
    for (from, rel, to) in &b.links {
        let target = nb.exprs[index[*to]].id.clone();
        nb.exprs[index[*from]]
            .evidence_ids
            .push(format!("{rel}:{target}"));
    }
    for (i, v) in refs {
        nb.exprs[index[*i]]
            .evidence_ids
            .push(format!("references_view:{v}"));
    }
    nb
}

/// Every uncovered source interval becomes an explicit unknown member. A LINE
/// span coincident with another source's structural use is marked as such.
fn unresolved(cx: &Ctx, b: &mut Builder, regions: &[Region]) {
    let structural: BTreeMap<String, Vec<[f64; 2]>> = b
        .members
        .iter()
        .filter(|m| m.member_role == "contour_segment")
        .fold(BTreeMap::new(), |mut acc, m| {
            acc.entry(m.source_handle.clone())
                .or_insert_with(Vec::new)
                .push([m.interval[0], m.interval[1]]);
            acc
        });
    for ent in &cx.src.entities {
        let cov = b.covered(&ent.id);
        let mut gaps = Vec::new();
        let mut cursor = 0.;
        for [a, c] in &cov {
            if *a > cursor + 1e-12 {
                gaps.push([cursor, *a]);
            }
            cursor = f64::max(cursor, *c);
        }
        if cursor < 1. - 1e-12 {
            gaps.push([cursor, 1.]);
        }
        if gaps.is_empty() {
            continue;
        }
        let pts = cx.points(&ent.id);
        let view = bbox_of(&pts).and_then(|bb| {
            regions
                .iter()
                .filter(|r| r.kind != "sheet")
                .min_by(|x, y| bbox_gap(x.bbox, bb).total_cmp(&bbox_gap(y.bbox, bb)))
                .filter(|r| bbox_gap(r.bbox, bb) < 6. * cx.scale)
                .map(|r| r.id.clone())
        });
        let label = format!("{} {}", ent.kind, ent.id);
        let e = b.expr(
            "unresolved",
            &label,
            "uncovered_source_remainder",
            "unknown",
            vec![],
            vec![format!("source:{}", ent.id)],
            vec!["No expression rule in this profile covered this source interval".into()],
        );
        b.exprs[e].view_id = view.clone();
        for g in gaps {
            let mut role = "unresolved_source";
            let mut ev = vec![];
            if let Some((a, c)) = cx.lines.get(&ent.id) {
                let (p, q) = (lerp(*a, *c, g[0]), lerp(*a, *c, g[1]));
                if let Some(other) = structural.iter().find_map(|(o, ivs)| {
                    let (oa, oc) = cx.lines.get(o)?;
                    if o == &ent.id
                        || cross(unit(sub(*oc, *oa)), unit(sub(*c, *a))).abs() > 1e-6
                        || project(p, *oa, *oc).1 > 1e-6
                    {
                        return None;
                    }
                    let (t0, t1) = (project(p, *oa, *oc).0, project(q, *oa, *oc).0);
                    let (lo, hi) = (t0.min(t1), t0.max(t1));
                    ivs.iter()
                        .any(|iv| iv[0] <= lo + 1e-6 && iv[1] >= hi - 1e-6)
                        .then(|| o.clone())
                }) {
                    role = "coincident_source";
                    ev.push(format!("coincident_with:{other}"));
                    ev.push("GB/T 14665-2012 4.3 precedence".into());
                }
            }
            b.member(
                e,
                &ent.id,
                None,
                g,
                role,
                cx.lt(&ent.id),
                "unknown",
                vec![],
                ev,
            );
        }
    }
}

pub fn discover(bytes: &[u8]) -> Result<MdEngineeringExpressionGraph> {
    let src = usage_discovery::discover(bytes)?;
    let d = Drawing::parse(bytes)?;
    let mut layers = BTreeMap::new();
    for r in d.records.iter().filter(|r| r.kind == "LAYER") {
        if let Some(n) = r.get(2) {
            layers.insert(
                n.to_string(),
                LayerInfo {
                    color: int(r, 62),
                    linetype: r.get(6).unwrap_or("CONTINUOUS").to_string(),
                },
            );
        }
    }
    let mut patterns = BTreeMap::new();
    for r in d.records.iter().filter(|r| r.kind == "LTYPE") {
        if let Some(n) = r.get(2) {
            let complex = r
                .tags
                .iter()
                .any(|(c, v)| *c == 74 && v.trim().parse::<i32>().is_ok_and(|x| x & 6 != 0));
            let el: Option<Vec<f64>> = r
                .all(49)
                .iter()
                .map(|x| x.trim().parse::<f64>().ok())
                .collect();
            patterns.insert(
                n.to_uppercase(),
                if complex {
                    Pattern::Unknown
                } else {
                    el.map_or(Pattern::Unknown, |e| classify_pattern(&e))
                },
            );
        }
    }
    let conv = convention(&layers, &patterns);
    let mut rec = BTreeMap::new();
    for e in &src.entities {
        rec.insert(e.id.clone(), d.entity(&e.handle)?);
    }
    let block_rec: BTreeMap<String, &Record> = d
        .records
        .iter()
        .filter(|r| r.section == "BLOCKS")
        .filter_map(|r| r.get(5).map(|h| (h.to_string(), r)))
        .collect();
    let mut style = BTreeMap::new();
    let mut base = BTreeMap::new();
    let mut texts = BTreeMap::new();
    let mut arrows = BTreeMap::new();
    let mut inline_arrows = BTreeMap::new();
    let mut polys = BTreeMap::new();
    let mut shapes = BTreeMap::new();
    for (h, r) in &rec {
        let st = style_of(r, &layers, &patterns);
        base.insert(h.clone(), base_line_type(&r.kind, &st, &conv));
        style.insert(h.clone(), st);
        if matches!(r.kind.as_str(), "TEXT" | "MTEXT")
            && let Some(t) = text_fact(r)
        {
            texts.insert(h.clone(), t);
        }
        if let Some(a) = arrow_of(r) {
            arrows.insert(h.clone(), a);
        } else if let Some((path, a)) = inline_arrow(r) {
            inline_arrows.insert(h.clone(), a);
            polys.insert(h.clone(), (path, false, false));
        } else if matches!(r.kind.as_str(), "LWPOLYLINE" | "POLYLINE") {
            polys.insert(h.clone(), poly_points(&d, r));
        }
        if r.kind == "INSERT" {
            shapes.insert(h.clone(), dd::getshape(&d, r)?);
        }
    }
    let lines: BTreeMap<String, (P, P)> = src
        .entities
        .iter()
        .filter(|e| e.geometry_kind == "line" && e.coordinates.len() == 4)
        .map(|e| {
            (
                e.id.clone(),
                (
                    [e.coordinates[0], e.coordinates[1]],
                    [e.coordinates[2], e.coordinates[3]],
                ),
            )
        })
        .collect();
    let scale = median(texts.values().map(|t: &TextFact| t.height).collect()).unwrap_or(3.5);
    let thin_colors: BTreeSet<i32> = if conv.qualified {
        ["02", "08"]
            .iter()
            .filter_map(|l| layers.get(*l).map(|x: &LayerInfo| x.color.abs()))
            .collect()
    } else {
        BTreeSet::new()
    };
    let mut all_pts: Vec<P> = lines.values().flat_map(|(a, c)| [*a, *c]).collect();
    all_pts.extend(
        polys
            .values()
            .flat_map(|p: &(Vec<P>, bool, bool)| p.0.clone()),
    );
    all_pts.extend(shapes.values().flat_map(|s: &dd::Shape| {
        s.lines
            .iter()
            .flat_map(|(a, c)| [*a, *c])
            .collect::<Vec<_>>()
    }));
    let extent = bbox_of(&all_pts).unwrap_or([0., 0., 1., 1.]);
    let cx = Ctx {
        d: &d,
        src: &src,
        rec,
        block_rec,
        style,
        base,
        conv,
        texts,
        arrows,
        inline_arrows,
        lines,
        polys,
        shapes,
        scale,
        thin_colors,
        extent,
    };
    let mut b = Builder::default();
    let mut def = Deferred::default();
    let mut text_anchor = BTreeMap::new();
    let mut native_texts = BTreeMap::new();
    let mut anchors = BTreeMap::new();
    native_dimensions(&cx, &mut b, &mut text_anchor, &mut native_texts);
    blocks(&cx, &mut b, &mut def);
    representation::datum_symbols(&cx, &mut b)?;
    representation::center_holes(&cx, &mut b)?;
    hatch_and_points(&cx, &mut b);
    representation::section_lines(&cx, &mut b)?;
    exploded_dimensions(&cx, &mut b, &mut anchors);
    exploded_angles(&cx, &mut b);
    let loose = representation::loose_glyphs(&cx, &b)?;
    exploded_frames(&cx, &mut b, &def, &mut anchors, &loose);
    extended_pair_dimensions(&cx, &mut b, &mut anchors);
    let titles = view_titles(&cx, &mut b);
    representation::visible_edges(&cx, &mut b)?;
    leader_annotations(&cx, &mut b, &text_anchor, &mut anchors);
    thread_texts(&cx, &mut b);
    boxed_values(&cx, &mut b, &native_texts);
    representation::special_marks(&cx, &mut b, &native_texts)?;
    representation::angle_units(&mut b)?;
    detail_circles(&cx, &mut b);
    unlabeled_view_arrows(&cx, &mut b, &mut anchors);
    technical_notes(&cx, &mut b);
    sheet_texts(&cx, &mut b, &def);
    line_kinds(&cx, &mut b);
    thread_arcs(&cx, &mut b);
    thread_lines(&cx, &mut b);
    break_lines(&cx, &mut b);
    hatch_lines(&cx, &mut b);
    let cover = topology::cover(&cx, &b);
    structure(&cx, &mut b, &cover);
    duplicate_spans(&cx, &mut b);
    representation::leader_extensions(&cx, &mut b)?;
    representation::frame_cards(&mut b)?;
    let (regions, vmap, refs) = views(&cx, &b, &titles, &anchors);
    let mut nb = finalize(&b, &vmap, &regions, &refs);
    unresolved(&cx, &mut nb, &regions);
    let view_of: BTreeMap<String, Option<String>> = nb
        .exprs
        .iter()
        .map(|e| (e.id.clone(), e.view_id.clone()))
        .collect();
    for m in &mut nb.members {
        m.view_id = view_of[&m.expression_id].clone();
    }
    let (structure_ends, implied_spans) = topology::audit(&cx, &nb, &cover);
    for (i, label) in topology::section_profile(&cx, &nb, &cover, &implied_spans) {
        nb.members[i].evidence_ids.push(label);
    }
    let structural_lines = topology::structural_lines(&cx, &nb, &cover, &implied_spans);
    let line_styles = cx
        .src
        .entities
        .iter()
        .map(|e| {
            let (lt, basis, rule) = cx.base[&e.id].clone();
            let st = &cx.style[&e.id];
            MdLineStyle {
                source_handle: e.id.clone(),
                primitive: e.kind.clone(),
                pattern: if is_curve_kind(&e.kind) {
                    st.pattern.name()
                } else {
                    Pattern::NotCurve.name()
                }
                .into(),
                layer: st.layer.clone(),
                layer_rule: rule,
                line_type: lt.into(),
                basis: basis.into(),
                evidence_ids: vec![
                    format!("color:{}", st.color),
                    format!("byblock:{}", st.byblock),
                ],
            }
        })
        .collect::<Vec<_>>();
    let views = regions
        .iter()
        .map(|r| MdDrawingView {
            id: r.id.clone(),
            kind: r.kind.into(),
            label: r.label.clone(),
            scale_text: r.scale.clone(),
            bbox: r.bbox.to_vec(),
            label_source_ids: r.label_sources.clone(),
            state: r.state.into(),
            evidence_ids: r.evidence.clone(),
            unknowns: if r.kind == "sheet" {
                vec![]
            } else {
                vec!["Projection relation to the principal view remains a candidate".into()]
            },
        })
        .collect::<Vec<_>>();
    let graph = MdEngineeringExpressionGraph {
        schema: SCHEMA.into(),
        profile: PROFILE.into(),
        source_sha256: src.source_sha256.clone(),
        source: src.clone(),
        layer_convention: if cx.conv.qualified { "gbt14665_2012_table6_consistent" } else { "not_qualified" }.into(),
        layer_convention_evidence: cx.conv.evidence.clone(),
        views,
        line_styles,
        members: nb.members,
        expressions: nb.exprs,
        structure_ends,
        implied_spans,
        structural_lines,
        unknowns: vec![
            "Implied spans are drafting-convention inferences for the complete-structure representation; they are not source lines or members".into(),
            "Structural lines join collinear or co-circular connected outline pieces of one view and class; the joining is a drafting interpretation, not a part feature".into(),
            "Expression kinds are rule-derived candidates from source bytes only; no old labels, repaired intervals, regions or paired models were read".into(),
            "Part features, measured targets and manufacturing scope of expressions remain unresolved without projection evidence".into(),
            "Nested members of INSERT roots are represented through their whole block root; DIMENSION members are listed individually".into(),
            "Rule thresholds and view heuristics were calibrated on development drawings; results on another drawing need independent evaluation".into(),
        ],
    };
    check(&graph)?;
    Ok(graph)
}

/// Every outline member outside a section profile and every inferred span is in
/// exactly one structural line of its own view and class.
fn check_structural_lines(g: &MdEngineeringExpressionGraph) -> Result<()> {
    let kind: BTreeMap<&str, &str> = g
        .expressions
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let mut owner: BTreeMap<String, (Option<&String>, &str, usize)> = BTreeMap::new();
    for m in &g.members {
        let class = match (
            kind.get(m.expression_id.as_str()).copied(),
            m.member_role.as_str(),
        ) {
            (Some("visible_contour"), "contour_segment") => "visible_outline",
            (Some("hidden_contour"), "hidden_segment") => "hidden_outline",
            _ => continue,
        };
        if m.evidence_ids
            .iter()
            .any(|e| e.starts_with("section_profile:"))
        {
            continue;
        }
        owner.insert(format!("member:{}", m.id), (m.view_id.as_ref(), class, 0));
    }
    for s in &g.implied_spans {
        owner.insert(s.id.clone(), (s.view_id.as_ref(), s.line_class.as_str(), 0));
    }
    let mut ids = BTreeSet::new();
    for l in &g.structural_lines {
        let n = l.points.len();
        let shape = match l.geometry.as_str() {
            "line" => n == 4 && l.arc.is_none(),
            "arc" => n == 4 && l.arc.as_ref().is_some_and(|a| a.len() == 5),
            "circle" => n == 2 && l.arc.as_ref().is_some_and(|a| a.len() == 5),
            _ => n >= 4 && n % 2 == 0 && l.arc.is_none(),
        };
        ensure!(
            ids.insert(l.id.as_str()) && shape && !l.pieces.is_empty(),
            "engineering_expression_structural_line:{}",
            l.id
        );
        for p in l.pieces.iter().chain(&l.coincident) {
            let o = owner.get_mut(p);
            ensure!(
                o.as_ref()
                    .is_some_and(|o| o.0 == l.view_id.as_ref() && o.1 == l.line_class && o.2 == 0),
                "engineering_expression_structural_line_piece:{}:{p}",
                l.id
            );
            if let Some(o) = o {
                o.2 = 1;
            }
        }
    }
    ensure!(
        owner.values().all(|o| o.2 == 1),
        "engineering_expression_structural_line_coverage"
    );
    Ok(())
}

pub(crate) fn check(g: &MdEngineeringExpressionGraph) -> Result<()> {
    ensure!(
        g.source_sha256 == g.source.source_sha256,
        "engineering_expression_source_identity"
    );
    ensure!(
        g.validate().is_ok()
            && g.members.iter().all(|m| m.validate().is_ok())
            && g.expressions.iter().all(|e| e.validate().is_ok())
            && g.views.iter().all(|v| v.validate().is_ok())
            && g.line_styles.iter().all(|s| s.validate().is_ok())
            && g.structure_ends.iter().all(|e| e.validate().is_ok())
            && g.implied_spans.iter().all(|s| s.validate().is_ok())
            && g.structural_lines.iter().all(|l| l.validate().is_ok()),
        "engineering_expression_contract"
    );
    let roots_of = |kind: &str| -> BTreeSet<&str> {
        g.source
            .entities
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| e.id.as_str())
            .collect()
    };
    let (line_roots, arc_roots) = (roots_of("LINE"), roots_of("ARC"));
    let unit_param = |t: f64| (0. ..=1.).contains(&t);
    ensure!(
        g.structure_ends.iter().all(
            |e| g.source.entities.iter().any(|r| r.id == e.source_handle)
                && unit_param(e.parameter)
                && e.point.len() == 2
        ) && g.implied_spans.iter().all(|s| {
            let roots = if s.arc.is_some() {
                &arc_roots
            } else {
                &line_roots
            };
            roots.contains(s.from_source_handle.as_str())
                && roots.contains(s.to_source_handle.as_str())
                && unit_param(s.from_parameter)
                && unit_param(s.to_parameter)
                && s.points.len() == 4
                && s.arc.as_ref().is_none_or(|a| a.len() == 5)
        }),
        "engineering_expression_structure_topology"
    );
    check_structural_lines(g)?;
    let mut ids = BTreeSet::new();
    ensure!(
        g.structure_ends.iter().all(|e| ids.insert(e.id.as_str()))
            && g.implied_spans.iter().all(|s| ids.insert(s.id.as_str())),
        "engineering_expression_topology_ids"
    );
    let roots: BTreeSet<&str> = g.source.entities.iter().map(|e| e.id.as_str()).collect();
    let exprs: BTreeSet<&str> = g.expressions.iter().map(|e| e.id.as_str()).collect();
    ensure!(
        g.members
            .iter()
            .all(|m| roots.contains(m.source_handle.as_str())
                && exprs.contains(m.expression_id.as_str())
                && m.interval.len() == 2
                && m.interval
                    .iter()
                    .all(|x| x.is_finite() && (0. ..=1.).contains(x))
                && m.interval[0] <= m.interval[1]),
        "engineering_expression_member_domain"
    );
    #[allow(clippy::type_complexity)]
    let mut seen: BTreeMap<(&str, &str, Option<&str>, &str), Vec<&[f64]>> = BTreeMap::new();
    for m in &g.members {
        let k = (
            m.expression_id.as_str(),
            m.source_handle.as_str(),
            m.native_member_id.as_deref(),
            m.member_role.as_str(),
        );
        let v = seen.entry(k).or_default();
        ensure!(
            !v.iter().any(|x| same_interval(x, &m.interval)),
            "engineering_expression_redundant_member:{}",
            m.id
        );
        v.push(&m.interval);
    }
    let mut cover: BTreeMap<&str, Vec<[f64; 2]>> = BTreeMap::new();
    for m in &g.members {
        cover
            .entry(m.source_handle.as_str())
            .or_default()
            .push([m.interval[0], m.interval[1]]);
    }
    for r in &roots {
        let mut v = cover.get(r).cloned().unwrap_or_default();
        v.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut cursor = 0.;
        for [a, b] in v {
            ensure!(
                a <= cursor + 1e-9,
                "engineering_expression_coverage_gap:{r}"
            );
            cursor = f64::max(cursor, b);
        }
        ensure!(
            cursor >= 1. - 1e-9,
            "engineering_expression_coverage_gap:{r}"
        );
    }
    ensure!(
        serde_json::to_vec(g)?.len() < 16 * 1024 * 1024,
        "engineering_expression_output_budget"
    );
    Ok(())
}
