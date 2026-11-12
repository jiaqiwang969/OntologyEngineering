//! Structure topology audit for the expression graph.
//!
//! Every free end of a visible or hidden outline span is an obligation that
//! carries a typed interruption cause. Generic drafting evidence only:
//! * GB/T 4458.4: a dimension number may not be crossed by any line, so lines
//!   are broken where a text sits; a gap between two collinear outline pieces
//!   that is covered by a text box (and the dimension lines and terminators
//!   next to it) is such a break;
//! * GB/T 4458.1 local detail: a thin detail circle only marks the enlarged part;
//!   collinear outline pieces whose facing ends both lie on one detail circle are
//!   one line cut at the circle (card `detail_circle_interruption`);
//! * GB/T 4458.4: the outline is kept clear of dimension lines and terminators
//!   in the same way; a gap crossed only by the dimension line or terminator of
//!   a dimension, close to it at both ends, is such a break too (card
//!   `annotation_break`, variant `broken_for_dimension_line`);
//! * GB/T 17452 6.3 / GB/T 4458.6 5.6: inside a local section the outline is
//!   replaced by the section; the region is the area enclosed by the break
//!   curve(s) and the outline between their ends (module `local_section`), and
//!   an end in it is interrupted by that section. What is drawn only because of
//!   the cut is left out of the structure; an end that meets nothing else is free.
//!
//! Outline inferred across an explained interruption is kept apart from source
//! members (complete-structure representation only). A gap whose facing ends
//! meet other structure (a slot cutting through a line) is a real opening and
//! is never bridged. Straight lines continue collinearly and arcs along their
//! circle. An arc continues only to another arc, across a gap that is small
//! against what is drawn of the circle: a fillet never closes into a circle and
//! an arc is never joined to its own other end.
//!
//! Structural lines: within one view and outline class, the collinear (or
//! co-circular) connected source pieces and inferred spans are one drawn line,
//! however the source split it (a text, a local section, drafter segmentation,
//! a duplicate, a tiny offset or gap). Only a real opening, other geometry or
//! another view separates lines. The line type follows the structure (the
//! highest-precedence member type), never the evidence that bridged a gap.
//! Nothing here reads old labels, regions or paired models.
use super::*;
use oe_contracts::{MdImpliedSpan, MdStructuralEnd, MdStructuralLine};

#[path = "local_section.rs"]
mod local_section;
use local_section::LocalSection;

/// Text box: centre, unit direction, half width, half height, height, handle.
type TextBox = (P, P, f64, f64, f64, String);
/// Annotation symbol drawn as a block: strokes, extent and handle.
pub(super) type Symbol = (Vec<(P, P)>, f64, String);

/// Evidence that can explain an interruption of an outline line.
pub(super) struct Cover {
    texts: Vec<TextBox>,
    /// Annotation segments (dimension, extension and leader lines, terminators):
    /// end points, half width and source handle.
    strips: Vec<(P, P, f64, String)>,
    /// Strokes of dimensions an outline is kept clear of: dimension lines and the bodies of
    /// terminators (end points and source handle), root sources and parts of native dimensions.
    dimension_strokes: Vec<(P, P, String)>,
    /// Usual terminator length of the drawing.
    arrow: f64,
    /// Local sections: the region enclosed by the break curve(s) and the outline between their
    /// ends, with the hatch in it.
    sections: Vec<LocalSection>,
    /// Detail circles: centre, radius and source handle.
    circles: Vec<(P, f64, String)>,
    /// Annotation symbols drawn as blocks (texture, datum, frame, view symbols): strokes in
    /// drawing coordinates, extent and source handle (card `annotation_break`).
    symbols: Vec<Symbol>,
}

impl Cover {
    /// A closed cut region must be traced through its break curves and actual
    /// outline. Its box alone cannot witness that a loose lattice is enclosed.
    pub(super) fn encloses_hatch(&self, sources: &[&str], points: &[P], tol: f64) -> bool {
        !points.is_empty()
            && self.sections.iter().any(|s| {
                sources.iter().all(|h| s.hatch.iter().any(|x| x == h))
                    && points.iter().all(|p| s.holds(*p, tol))
            })
    }
}

/// A collinear outline piece that may continue a line across a gap.
pub(super) struct Partner {
    pub handle: String,
    /// Span end points and their source parameters.
    pub ends: [P; 2],
    pub t: [f64; 2],
    /// Whether each end is free (meets no other structure).
    pub free: [bool; 2],
}

pub(super) struct Cause {
    pub kind: &'static str,
    /// Facing end across the gap: handle, source parameter, point, free.
    pub partner: Option<(String, f64, P, bool)>,
    pub evidence: Vec<String>,
}

/// Width of the band about a carrier within which pieces count as one drawn
/// line: a small fraction of a line width, for drafting imprecision only.
fn band(cx: &Ctx) -> f64 {
    0.02 * (cx.scale.max(1.) / 3.5).max(1.)
}

fn in_box(x: P, t: &TextBox, c: f64) -> bool {
    let v = sub(x, t.0);
    dot(v, t.1).abs() <= t.2 + c && cross(t.1, v).abs() <= t.3 + c
}

fn seg_dist(x: P, a: P, b: P) -> f64 {
    let v = sub(b, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return dist(x, a);
    }
    let t = (dot(sub(x, a), v) / n).clamp(0., 1.);
    dist(x, add(a, mul(v, t)))
}

/// Text boxes, annotation strips and section clusters of the assembled expressions.
pub(super) fn cover(cx: &Ctx, b: &Builder) -> Cover {
    let texts = cx
        .texts
        .values()
        .map(|t| {
            (
                t.center,
                t.dir,
                t.width / 2.,
                t.height / 2.,
                t.height,
                t.id.clone(),
            )
        })
        .collect();
    let mut strips = Vec::new();
    for m in &b.members {
        let h = m.source_handle.as_str();
        match m.member_role.as_str() {
            "dimension_line" | "extension_line" | "leader_line" => {
                if let Some((a, c)) = cx.lines.get(h) {
                    let (p, q) = (lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1]));
                    strips.push((p, q, 0., h.to_string()));
                } else if let Some((pts, _, _)) = cx.polys.get(h) {
                    for w in pts.windows(2) {
                        strips.push((w[0], w[1], 0., h.to_string()));
                    }
                }
            }
            "arrowhead" => {
                if let Some(a) = cx.arrow(h) {
                    strips.push((
                        a.tip,
                        sub(a.tip, mul(a.dir, a.len)),
                        0.25 * a.len,
                        h.to_string(),
                    ));
                }
            }
            _ => {}
        }
    }
    // Dimension lines and terminators of dimensions, also those inside native dimensions (placed
    // by the block offset of the dimension).
    let dimension: BTreeSet<&str> = b
        .exprs
        .iter()
        .filter(|e| e.kind.starts_with("dimension_") || e.kind == "thread_note")
        .map(|e| e.id.as_str())
        .collect();
    let mut dimension_strokes = Vec::new();
    let mut arrows = Vec::new();
    for m in b
        .members
        .iter()
        .filter(|m| dimension.contains(m.expression_id.as_str()))
    {
        let h = m.source_handle.as_str();
        let stroke = match (m.member_role.as_str(), m.native_member_id.as_deref()) {
            ("dimension_line", None) => cx
                .lines
                .get(h)
                .map(|(a, c)| (lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1]))),
            ("arrowhead", None) => cx.arrow(h).map(|a| {
                arrows.push(a.len);
                (a.tip, sub(a.tip, mul(a.dir, a.len)))
            }),
            ("dimension_line" | "arrowhead", Some(id)) => cx
                .rec
                .get(h)
                .filter(|r| r.kind == "DIMENSION")
                .zip(id.rsplit('/').next().and_then(|n| cx.block_rec.get(n)))
                .and_then(|(r, x)| {
                    let off = r.xy(12).unwrap_or([0., 0.]);
                    match x.kind.as_str() {
                        "LINE" if m.member_role == "dimension_line" => x
                            .xy(10)
                            .ok()
                            .zip(x.xy(11).ok())
                            .map(|(p, q)| (add(p, off), add(q, off))),
                        "SOLID" | "TRACE" => arrow_of(x).map(|a| {
                            arrows.push(a.len);
                            let tip = add(a.tip, off);
                            (tip, sub(tip, mul(a.dir, a.len)))
                        }),
                        _ => None,
                    }
                }),
            _ => None,
        };
        if let Some((p, q)) = stroke {
            dimension_strokes.push((p, q, h.to_string()));
        }
    }
    let arrow = median(arrows).unwrap_or(0.);
    let sections = local_section::find(cx, b);
    let circles = b
        .members
        .iter()
        .filter(|m| m.member_role == "detail_circle")
        .filter_map(|m| cx.arc(&m.source_handle).map(|(c, r, ..)| (c, r, m.source_handle.clone())))
        .collect();
    Cover {
        texts,
        strips,
        dimension_strokes,
        arrow,
        sections,
        circles,
        symbols: super::line_evidence::annotation_symbols(cx, b),
    }
}

/// The gap from `p` along `d` of length `len` is crossed by the strokes of an annotation symbol
/// no smaller than the gap: the outline was broken so the symbol stays legible (GB/T 4458.4; card
/// `annotation_break`, variant `broken_for_symbol`).
fn symbol_break(cov: &Cover, p: P, d: P, len: f64) -> Option<Vec<String>> {
    let q = add(p, mul(d, len));
    let crosses = |a: P, b: P| {
        let (r, s) = (sub(q, p), sub(b, a));
        let den = cross(r, s);
        if den.abs() < 1e-12 {
            return false;
        }
        let t = cross(sub(a, p), s) / den;
        let u = cross(sub(a, p), r) / den;
        (0. ..=1.).contains(&t) && (0. ..=1.).contains(&u)
    };
    let k = representation::card_ref("annotation_break")
        .ok()
        .and_then(|c| representation::param(c, "gap_to_symbol_max").ok())?;
    let hit = cov.symbols.iter().find(|(strokes, size, _)| {
        k * size >= len && strokes.iter().any(|(a, b)| crosses(*a, *b))
    })?;
    Some(vec![
        format!("symbol:{}", hit.2),
        "card:annotation_break".into(),
        "variant:broken_for_symbol".into(),
    ])
}

/// The gap from `p` along `d` of length `len` is crossed by the dimension line or the terminator
/// of a dimension and by no outline, and each end of the gap is within about one terminator
/// length of the outermost crossing: the outline was broken to keep the dimension line and its
/// terminator clear (GB/T 4458.4-2003 4.3.4 names the number; card `annotation_break`, variant
/// `broken_for_dimension_line`). A slot whose width is dimensioned is no such gap: its ends
/// meet the slot walls and are not free.
fn dimension_break(cov: &Cover, p: P, d: P, len: f64, partners: &[Partner]) -> Option<Vec<String>> {
    let k = representation::card_ref("annotation_break")
        .ok()
        .and_then(|c| representation::param(c, "gap_end_to_crossing_max").ok())?;
    let q = add(p, mul(d, len));
    // Where the segment `ab` crosses the gap, as a length from `p`.
    let crossing = |a: P, b: P| -> Option<f64> {
        let (r, s) = (sub(q, p), sub(b, a));
        let den = cross(r, s);
        if den.abs() < 1e-12 {
            return None;
        }
        let t = cross(sub(a, p), s) / den;
        let u = cross(sub(a, p), r) / den;
        ((0. ..=1.).contains(&t) && (0. ..=1.).contains(&u)).then_some(t * len)
    };
    let eps = 1e-6 * len.max(1.);
    if partners
        .iter()
        .any(|x| crossing(x.ends[0], x.ends[1]).is_some_and(|t| t > eps && t < len - eps))
    {
        return None;
    }
    let hits: Vec<(f64, &str)> = cov
        .dimension_strokes
        .iter()
        .filter_map(|(a, b, h)| crossing(*a, *b).map(|t| (t, h.as_str())))
        .collect();
    let lo = hits.iter().map(|x| x.0).fold(f64::INFINITY, f64::min);
    let hi = hits.iter().map(|x| x.0).fold(f64::NEG_INFINITY, f64::max);
    if hits.is_empty() || lo > k * cov.arrow || len - hi > k * cov.arrow {
        return None;
    }
    let by: BTreeSet<String> = hits.iter().map(|x| format!("dimension:{}", x.1)).collect();
    Some(
        by.into_iter()
            .chain([
                "card:annotation_break".to_string(),
                "variant:broken_for_dimension_line".to_string(),
            ])
            .collect(),
    )
}

/// The gap from `p` along `d` of length `len` is covered by text boxes, with
/// dimension lines and terminators next to them, within a clearance of a fifth
/// of the text height (GB/T 4458.4: lines are broken for a dimension number).
fn text_break(cov: &Cover, p: P, d: P, len: f64) -> Option<Vec<String>> {
    let q = add(p, mul(d, len));
    text_break_along(
        cov,
        &|i, n| add(p, mul(d, len * i as f64 / n as f64)),
        len,
        dist(p, q),
    )
}

/// The gap path sampled by `at(i, n)` (point `i` of `n` equal parts), of length
/// `len` and extent `span`, is covered by texts and the annotation next to them.
fn text_break_along(
    cov: &Cover,
    at: &dyn Fn(usize, usize) -> P,
    len: f64,
    span: f64,
) -> Option<Vec<String>> {
    let samples = |n: usize| (1..n).map(move |i| at(i, n));
    let hits: Vec<&TextBox> = cov
        .texts
        .iter()
        .filter(|t| samples(32).any(|x| in_box(x, t, 0.)))
        .collect();
    let h = hits.iter().map(|t| t.4).fold(0., f64::max);
    if hits.is_empty() || h <= 0. || span > 4. * h {
        return None;
    }
    let c = 0.2 * h;
    let n = ((len / (0.05 * h)).ceil() as usize).clamp(8, 400);
    let mut ev: BTreeSet<String> = hits.iter().map(|t| format!("text:{}", t.5)).collect();
    for x in samples(n) {
        if let Some(t) = cov.texts.iter().find(|t| in_box(x, t, c)) {
            ev.insert(format!("text:{}", t.5));
        } else if let Some(s) = cov.strips.iter().find(|s| seg_dist(x, s.0, s.1) <= s.2 + c) {
            ev.insert(format!("annotation:{}", s.3));
        } else {
            return None;
        }
    }
    Some(ev.into_iter().collect())
}

/// Cause of a free end at `p` of the outline piece `own`, which runs outward
/// along unit `d` at that end.
pub(super) fn interruption(
    cx: &Ctx,
    cov: &Cover,
    own: &str,
    p: P,
    d: P,
    partners: &[Partner],
) -> Option<Cause> {
    interruption_in(cx, cov, None, own, p, d, partners)
}

/// As `interruption`, among the local sections that stand once the views are known (`live`).
fn interruption_in(
    cx: &Ctx,
    cov: &Cover,
    live: Option<&[bool]>,
    own: &str,
    p: P,
    d: P,
    partners: &[Partner],
) -> Option<Cause> {
    let s = cx.scale.max(1.);
    let tol = 2e-3 * s;
    let w = band(cx);
    // Nearest piece beyond the end within the band of one drawn line.
    let mut best: Option<(f64, &Partner, usize)> = None;
    for k in partners.iter().filter(|k| k.handle != own) {
        if k.ends.iter().any(|q| cross(d, sub(*q, p)).abs() > w) {
            continue;
        }
        for i in 0..2 {
            let t = dot(sub(k.ends[i], p), d);
            if t > tol
                && dot(sub(k.ends[1 - i], p), d) > t
                && t <= 40. * s
                && best.as_ref().is_none_or(|b| t < b.0)
            {
                best = Some((t, k, i));
            }
        }
    }
    let facing = |k: &Partner, i: usize| (k.handle.clone(), k.t[i], k.ends[i], k.free[i]);
    if let Some((len, k, i)) = best
        && k.free[i]
        && let Some(ev) = text_break(cov, p, d, len)
            .or_else(|| symbol_break(cov, p, d, len))
            .or_else(|| dimension_break(cov, p, d, len, partners))
    {
        return Some(Cause {
            kind: "text_break",
            partner: Some(facing(k, i)),
            evidence: ev,
        });
    }
    // In a local section: the line goes on to the collinear piece across the region.
    for (n, sec) in cov.sections.iter().enumerate() {
        let near = sec.near;
        if live.is_some_and(|l| !l[n]) || !sec.holds(p, near) {
            continue;
        }
        let partner = best
            .filter(|(len, k, i)| {
                sec.holds(k.ends[*i], near) && sec.holds(add(p, mul(d, len / 2.)), near)
            })
            .map(|(_, k, i)| facing(k, i));
        return Some(Cause {
            kind: "local_section",
            partner,
            evidence: sec.evidence(),
        });
    }
    None
}

/// A line end on a detail circle facing a collinear outline piece whose end lies on the
/// same circle across a gap inside it: one line cut at the circle.
pub(super) fn detail_crossing(
    cx: &Ctx,
    cov: &Cover,
    own: &str,
    p: P,
    d: P,
    partners: &[Partner],
) -> Option<Cause> {
    let s = cx.scale.max(1.);
    let ctol = 0.02 * s;
    let (c, r, ch) = cov.circles.iter().find(|(c, r, _)| (dist(p, *c) - r).abs() <= ctol)?;
    let mut best: Option<(f64, &Partner, usize)> = None;
    for k in partners.iter().filter(|k| k.handle != own) {
        for i in 0..2 {
            let q = k.ends[i];
            let t = dot(sub(q, p), d);
            if (dist(q, *c) - r).abs() <= ctol
                && t > ctol
                && cross(d, sub(q, p)).abs() <= ctol
                && cross(d, sub(k.ends[1 - i], p)).abs() <= ctol
                && dot(sub(k.ends[1 - i], p), d) > t
                && dist(mid(p, q), *c) < *r
                && best.as_ref().is_none_or(|b| t < b.0)
            {
                best = Some((t, k, i));
            }
        }
    }
    let (_, k, i) = best?;
    Some(Cause {
        kind: "detail_circle_crossing",
        partner: Some((k.handle.clone(), k.t[i], k.ends[i], true)),
        evidence: vec![format!("detail_circle:{ch}"), "card:detail_circle_interruption".into()],
    })
}

/// A co-circular outline arc that may continue an arc across a gap.
pub(super) struct ArcPartner {
    pub handle: String,
    pub c: P,
    pub r: f64,
    /// Start angle and counter-clockwise sweep.
    pub s0: f64,
    pub sw: f64,
    /// Whether the start and end are free.
    pub free: [bool; 2],
}

/// Cause of the free end `k` (0 start, 1 end) of the outline arc `own`, found
/// along its circle: the nearest co-circular arc end facing it across a gap
/// covered by a text, or across a local section. Returns the cause and the gap
/// arc (start angle, counter-clockwise sweep). The arc goes on to the facing arc
/// only when that end is free too and the gap is small against what is drawn of
/// the circle (card `annotation_break`: not more than the drawn sweep, not more
/// than about a quarter turn). It never goes on to its own other end: a fillet
/// does not close into a circle, and an arc broken once keeps its gap, with the
/// cause named.
fn arc_interruption(
    cx: &Ctx,
    cov: &Cover,
    live: &[bool],
    own: &ArcPartner,
    k: usize,
    partners: &[ArcPartner],
) -> Option<(Cause, [f64; 2])> {
    use std::f64::consts::TAU;
    let s = cx.scale.max(1.);
    let tol = 2e-3 * s;
    let w = band(cx);
    let at = |a: f64| add(own.c, mul([a.cos(), a.sin()], own.r));
    let e = if k == 1 { own.s0 + own.sw } else { own.s0 };
    let p = at(e);
    let mut best: Option<(f64, &ArcPartner)> = None;
    let mut drawn = 0.;
    for q in partners {
        if dist(q.c, own.c) > w || (q.r - own.r).abs() > w {
            continue;
        }
        drawn += q.sw;
        let delta = if k == 1 {
            (q.s0 - e).rem_euclid(TAU)
        } else {
            (own.s0 - (q.s0 + q.sw)).rem_euclid(TAU)
        };
        // The gap overlaps neither arc.
        let fits = if q.handle == own.handle {
            own.sw < TAU - 1e-9
        } else {
            delta + own.sw + q.sw <= TAU + 1e-9
        };
        if !fits || delta * own.r <= tol || delta * own.r > 40. * s {
            continue;
        }
        if best.as_ref().is_none_or(|b| delta < b.0) {
            best = Some((delta, q));
        }
    }
    let (delta, q) = best?;
    let fi = 1 - k;
    let qa = if fi == 0 { q.s0 } else { q.s0 + q.sw };
    let qp = add(q.c, mul([qa.cos(), qa.sin()], q.r));
    let start = if k == 1 { e } else { own.s0 - delta }.rem_euclid(TAU);
    let gap = [start, delta];
    let facing = (q.handle.clone(), fi as f64, qp, q.free[fi]);
    let card = representation::card_ref("annotation_break").ok();
    let limit = |name: &str, default: f64| {
        card.and_then(|c| representation::param(c, name).ok())
            .unwrap_or(default)
    };
    let small = delta <= limit("arc_gap_sweep_max_deg", 90.).to_radians() + 1e-9
        && delta <= limit("arc_gap_to_drawn_sweep_max", 1.) * drawn + 1e-9;
    let joins = q.handle != own.handle && q.free[fi] && small;
    if q.free[fi] {
        let along = |i: usize, n: usize| at(start + delta * i as f64 / n as f64);
        if let Some(ev) = text_break_along(cov, &along, delta * own.r, delta * own.r) {
            return Some((
                Cause {
                    kind: "text_break",
                    partner: joins.then_some(facing),
                    evidence: ev,
                },
                gap,
            ));
        }
    }
    for (n, sec) in cov.sections.iter().enumerate() {
        let near = sec.near;
        if live[n]
            && joins
            && sec.holds(p, near)
            && sec.holds(qp, near)
            && (1..8).all(|i| sec.holds(at(start + delta * i as f64 / 8.), near))
        {
            return Some((
                Cause {
                    kind: "local_section",
                    partner: Some(facing),
                    evidence: sec.evidence(),
                },
                gap,
            ));
        }
    }
    None
}

/// Point at fraction `f` along an inferred span: its gap arc or its chord.
pub(super) fn implied_at(x: &MdImpliedSpan, f: f64) -> P {
    if let Some(a) = x.arc.as_ref().filter(|a| a.len() == 5) {
        let ang = a[3] + a[4] * f;
        return add([a[0], a[1]], mul([ang.cos(), ang.sin()], a[2]));
    }
    lerp([x.points[0], x.points[1]], [x.points[2], x.points[3]], f)
}

/// Candidate LINE outline sources as partners, with free ends from the junctions.
pub(super) fn line_partners(
    cx: &Ctx,
    cand: &BTreeMap<String, u8>,
    junctions: &BTreeMap<String, Vec<(f64, String)>>,
) -> Vec<Partner> {
    cand.keys()
        .filter_map(|h| {
            let (a, c) = cx.lines.get(h)?;
            let js = junctions.get(h);
            let free = |t: f64| !js.is_some_and(|v| v.iter().any(|x| (x.0 - t).abs() < 1e-6));
            Some(Partner {
                handle: h.clone(),
                ends: [*a, *c],
                t: [0., 1.],
                free: [free(0.), free(1.)],
            })
        })
        .collect()
}

struct Span {
    member: usize,
    class: &'static str,
    handle: String,
    view: Option<String>,
    /// End points and their source parameters (open spans only).
    ends: Option<([P; 2], [f64; 2], [P; 2])>,
    pts: Vec<P>,
    arc: Option<(P, f64, f64, f64, bool)>,
}

fn arc_dist(x: P, (c, r, s0, sw, full): (P, f64, f64, f64, bool)) -> f64 {
    let ang = ((x[1] - c[1]).atan2(x[0] - c[0]) - s0).rem_euclid(std::f64::consts::TAU);
    if full || ang <= sw + 1e-12 {
        return (dist(x, c) - r).abs();
    }
    let e0 = add(c, mul([s0.cos(), s0.sin()], r));
    let e1 = add(c, mul([(s0 + sw).cos(), (s0 + sw).sin()], r));
    dist(x, e0).min(dist(x, e1))
}

fn path_dist(x: P, pts: &[P], arc: Option<(P, f64, f64, f64, bool)>) -> f64 {
    if let Some(a) = arc {
        return arc_dist(x, a);
    }
    pts.windows(2)
        .map(|w| seg_dist(x, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

/// Free ends of the visible and hidden outline with their causes, and outline
/// inferred across explained interruptions. What is drawn only because of a local
/// section (its profile, hatch and break curve) is no part of the structure: an
/// end that meets nothing else is free and goes on across the region. The circle
/// of a thread convention is where an outline stops at a tapped hole: an end on
/// it is joined (card `outline_joins`, variant `end_on_thread_circle`).
pub(super) fn audit(
    cx: &Ctx,
    b: &Builder,
    cov: &Cover,
) -> (Vec<MdStructuralEnd>, Vec<MdImpliedSpan>) {
    let s = cx.scale.max(1.);
    let tol = 1e-3 * s;
    let exprs: BTreeMap<&str, &MdEngineeringExpression> =
        b.exprs.iter().map(|e| (e.id.as_str(), e)).collect();
    let section = local_section::section_only(cx, b, &cov.sections);
    let mut spans: Vec<Span> = Vec::new();
    #[allow(clippy::type_complexity)]
    let mut targets: Vec<(usize, Vec<P>, Option<(P, f64, f64, f64, bool)>)> = Vec::new();
    for (i, m) in b.members.iter().enumerate() {
        let Some(e) = exprs.get(m.expression_id.as_str()) else {
            continue;
        };
        let class = match (e.kind.as_str(), m.member_role.as_str()) {
            ("visible_contour", "contour_segment") => Some("visible_outline"),
            ("hidden_contour", "hidden_segment") => Some("hidden_outline"),
            ("center_axis", _) | ("break_line", _) => None,
            ("thread_convention", "thread_line") if cx.arc(&m.source_handle).is_some() => None,
            _ => continue,
        };
        if section.marks.contains_key(&i) {
            continue;
        }
        let h = m.source_handle.as_str();
        let (pts, arc, ends) = if let Some((a, c)) = cx.lines.get(h) {
            let (p, q) = (lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1]));
            let (u, v) = (unit(sub(p, q)), unit(sub(q, p)));
            (
                vec![p, q],
                None,
                Some(([p, q], [m.interval[0], m.interval[1]], [u, v])),
            )
        } else if let Some(a) = cx.arc(h) {
            let (c, r, s0, sw, full) = a;
            let ends = (!full).then(|| {
                let (e0, e1) = (
                    add(c, mul([s0.cos(), s0.sin()], r)),
                    add(c, mul([(s0 + sw).cos(), (s0 + sw).sin()], r)),
                );
                // Outward tangents at the start (clockwise) and end (counter-clockwise).
                (
                    [e0, e1],
                    [0., 1.],
                    [[s0.sin(), -s0.cos()], [-(s0 + sw).sin(), (s0 + sw).cos()]],
                )
            });
            (vec![], Some(a), ends)
        } else if let Some((pts, closed, _)) = cx.polys.get(h) {
            let n = pts.len();
            let ends = (!*closed && n >= 2).then(|| {
                (
                    [pts[0], pts[n - 1]],
                    [0., 1.],
                    [unit(sub(pts[0], pts[1])), unit(sub(pts[n - 1], pts[n - 2]))],
                )
            });
            (pts.clone(), None, ends)
        } else {
            continue;
        };
        targets.push((i, pts.clone(), arc));
        if let Some(class) = class {
            spans.push(Span {
                member: i,
                class,
                handle: h.to_string(),
                view: m.view_id.clone(),
                ends,
                pts,
                arc,
            });
        }
    }
    // Pass 1: free ends, with the nearest other outline, centre or boundary curve.
    let gap = super::line_evidence::join_gap(cx);
    let mut free: BTreeMap<(usize, usize), bool> = BTreeMap::new();
    let mut nearest: BTreeMap<(usize, usize), (f64, usize)> = BTreeMap::new();
    for sp in &spans {
        let Some((ps, _, _)) = &sp.ends else { continue };
        if dist(ps[0], ps[1]) < tol {
            continue;
        }
        for (k, p) in ps.iter().enumerate() {
            let near = targets
                .iter()
                .filter(|(i, _, _)| *i != sp.member)
                .map(|(i, pts, arc)| (path_dist(*p, pts, *arc), *i))
                .fold(
                    (f64::INFINITY, usize::MAX),
                    |a, x| if x.0 < a.0 { x } else { a },
                );
            free.insert((sp.member, k), near.0 > tol);
            nearest.insert((sp.member, k), near);
        }
    }
    let partners_of = |class: &str, view: &Option<String>| -> Vec<Partner> {
        spans
            .iter()
            .filter(|sp| {
                sp.class == class && &sp.view == view && sp.arc.is_none() && sp.pts.len() == 2
            })
            .filter_map(|sp| {
                let (ps, ts, _) = sp.ends.as_ref()?;
                Some(Partner {
                    handle: sp.handle.clone(),
                    ends: *ps,
                    t: *ts,
                    free: [
                        free.get(&(sp.member, 0)).copied().unwrap_or(false),
                        free.get(&(sp.member, 1)).copied().unwrap_or(false),
                    ],
                })
            })
            .collect()
    };
    // Partners come from the same view: a line never continues into another view.
    type Group = (&'static str, Option<String>);
    let groups: BTreeSet<Group> = spans.iter().map(|sp| (sp.class, sp.view.clone())).collect();
    let partners: BTreeMap<Group, Vec<Partner>> = groups
        .iter()
        .map(|k| (k.clone(), partners_of(k.0, &k.1)))
        .collect();
    let arc_of = |sp: &Span| -> Option<ArcPartner> {
        let (c, r, s0, sw, full) = sp.arc?;
        (!full).then(|| ArcPartner {
            handle: sp.handle.clone(),
            c,
            r,
            s0,
            sw,
            free: [
                free.get(&(sp.member, 0)).copied().unwrap_or(false),
                free.get(&(sp.member, 1)).copied().unwrap_or(false),
            ],
        })
    };
    let arc_partners: BTreeMap<Group, Vec<ArcPartner>> = groups
        .iter()
        .map(|k| {
            (
                k.clone(),
                spans
                    .iter()
                    .filter(|sp| sp.class == k.0 && sp.view == k.1)
                    .filter_map(arc_of)
                    .collect(),
            )
        })
        .collect();
    let end_id = |class: &str, h: &str, t: f64| format!("end:{class}:{h}:{t:.6}");
    // Pass 2: causes and inferred spans.
    let mut ends = Vec::new();
    let mut implied: Vec<MdImpliedSpan> = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    for sp in &spans {
        let Some((ps, ts, dirs)) = &sp.ends else {
            continue;
        };
        for k in 0..2 {
            let crossing = detail_crossing(
                cx,
                cov,
                &sp.handle,
                ps[k],
                dirs[k],
                &partners[&(sp.class, sp.view.clone())],
            );
            if crossing.is_none() && !free.get(&(sp.member, k)).copied().unwrap_or(false) {
                continue;
            }
            // Within the drafting gap that structure() already treats as a junction:
            // an imprecise joint, reported and never bridged.
            let (dn, ni) = nearest[&(sp.member, k)];
            // An arc continues along its circle first, then as before along its tangent.
            let mut gap_arc = None;
            let cause = if crossing.is_some() {
                crossing
            } else if dn <= gap {
                Some(Cause {
                    kind: "drafting_gap",
                    partner: None,
                    evidence: vec![format!("nearest:{}:{dn:.5}", b.members[ni].source_handle)],
                })
            } else if let Some((c, g)) = arc_of(sp).and_then(|own| {
                arc_interruption(
                    cx,
                    cov,
                    &section.live,
                    &own,
                    k,
                    &arc_partners[&(sp.class, sp.view.clone())],
                )
            }) {
                gap_arc = sp.arc.map(|a| vec![a.0[0], a.0[1], a.1, g[0], g[1]]);
                Some(c)
            } else {
                let ps_ = &partners[&(sp.class, sp.view.clone())];
                interruption_in(
                    cx,
                    cov,
                    Some(&section.live),
                    &sp.handle,
                    ps[k],
                    dirs[k],
                    ps_,
                )
            };
            let id = end_id(sp.class, &sp.handle, ts[k]);
            let (kind, evidence, partner) = match &cause {
                Some(c) => (c.kind, c.evidence.clone(), c.partner.clone()),
                None => ("unexplained", vec![], None),
            };
            ends.push(MdStructuralEnd {
                id: id.clone(),
                view_id: sp.view.clone(),
                line_class: sp.class.into(),
                source_handle: sp.handle.clone(),
                parameter: ts[k],
                point: ps[k].to_vec(),
                cause: kind.into(),
                partner_end_id: partner
                    .as_ref()
                    .filter(|x| x.3)
                    .map(|x| end_id(sp.class, &x.0, x.1)),
                evidence_ids: evidence.clone(),
                state: if kind == "unexplained" {
                    "unknown"
                } else {
                    "candidate"
                }
                .into(),
                unknowns: if kind == "unexplained" {
                    vec![
                        "No drafting cause found for this open end; it stays an open obligation"
                            .into(),
                    ]
                } else {
                    vec![]
                },
            });
            let Some((ph, pt, pp, _)) = partner else {
                continue;
            };
            let pair = if id < end_id(sp.class, &ph, pt) {
                (id.clone(), end_id(sp.class, &ph, pt))
            } else {
                (end_id(sp.class, &ph, pt), id.clone())
            };
            if !seen.insert(pair) {
                continue;
            }
            implied.push(MdImpliedSpan {
                id: format!("implied:{}", implied.len()),
                view_id: sp.view.clone(),
                line_class: sp.class.into(),
                cause: kind.into(),
                representation: "complete_structure".into(),
                from_source_handle: sp.handle.clone(),
                from_parameter: ts[k],
                to_source_handle: ph,
                to_parameter: pt,
                points: vec![ps[k][0], ps[k][1], pp[0], pp[1]],
                arc: gap_arc,
                evidence_ids: evidence,
                state: "candidate".into(),
                unknowns: vec!["Inferred from a drafting convention; no source line exists between the two ends".into()],
            });
        }
    }
    corroborate(cx, b, &exprs, &spans, &mut implied);
    (ends, implied)
}

/// Mirror image about the principal axis of the view: a revolved part shows the
/// same outline on both sides of its axis, so a mirrored span covered by outline
/// of the same class corroborates the inference.
fn corroborate(
    cx: &Ctx,
    b: &Builder,
    exprs: &BTreeMap<&str, &MdEngineeringExpression>,
    spans: &[Span],
    implied: &mut [MdImpliedSpan],
) {
    let s = cx.scale.max(1.);
    let mtol = 0.01 * (s / 3.5).max(1.);
    // Longest collinear group of centre lines per view.
    let mut axes: BTreeMap<Option<String>, (f64, P, P)> = BTreeMap::new();
    #[allow(clippy::type_complexity)]
    let mut groups: BTreeMap<(Option<String>, i64, i64), (f64, P, P)> = BTreeMap::new();
    for m in &b.members {
        if exprs.get(m.expression_id.as_str()).map(|e| e.kind.as_str()) != Some("center_axis") {
            continue;
        }
        let Some((a, c)) = cx.lines.get(&m.source_handle) else {
            continue;
        };
        let u = unit(sub(*c, *a));
        let u = if u[0] < -1e-9 || (u[0].abs() < 1e-9 && u[1] < 0.) {
            mul(u, -1.)
        } else {
            u
        };
        let off = cross(u, *a);
        let key = (
            m.view_id.clone(),
            (u[1].atan2(u[0]).to_degrees() * 10.).round() as i64,
            (off * 100.).round() as i64,
        );
        let g = groups.entry(key).or_insert((0., *a, u));
        g.0 += dist(*a, *c);
    }
    for ((view, _, _), (len, a, u)) in groups {
        let e = axes.entry(view).or_insert((0., a, u));
        if len > e.0 {
            *e = (len, a, u);
        }
    }
    let mirror = |x: P, a: P, u: P| {
        let v = sub(x, a);
        let along = mul(u, dot(v, u));
        add(a, sub(along, sub(v, along)))
    };
    type Carrier = Option<(P, f64, f64, f64, bool)>;
    let carrier = |x: &MdImpliedSpan| -> Carrier {
        x.arc
            .as_ref()
            .filter(|a| a.len() == 5)
            .map(|a| ([a[0], a[1]], a[2], a[3], a[4], false))
    };
    let all: Vec<(String, &'static str, P, P, Carrier)> = implied
        .iter()
        .map(|x| {
            let cls = if x.line_class == "visible_outline" {
                "visible_outline"
            } else {
                "hidden_outline"
            };
            (
                x.id.clone(),
                cls,
                [x.points[0], x.points[1]],
                [x.points[2], x.points[3]],
                carrier(x),
            )
        })
        .collect();
    for x in implied.iter_mut() {
        let Some((_, a, u)) = axes.get(&x.view_id) else {
            x.unknowns
                .push("No centre line in the view to check symmetry".into());
            continue;
        };
        let (p, q) = (
            mirror([x.points[0], x.points[1]], *a, *u),
            mirror([x.points[2], x.points[3]], *a, *u),
        );
        let xr: &MdImpliedSpan = x;
        let samples: Vec<P> = (0..=16)
            .map(|i| {
                if carrier(xr).is_some() {
                    mirror(implied_at(xr, i as f64 / 16.), *a, *u)
                } else {
                    lerp(p, q, i as f64 / 16.)
                }
            })
            .collect();
        let mut by: BTreeSet<String> = BTreeSet::new();
        let ok = samples.iter().all(|&y| {
            if let Some(sp) = spans
                .iter()
                .filter(|sp| sp.class == x.line_class)
                .find(|sp| path_dist(y, &sp.pts, sp.arc) <= mtol)
            {
                by.insert(sp.handle.clone());
                return true;
            }
            if let Some(o) = all
                .iter()
                .filter(|o| o.1 == x.line_class && o.0 != x.id)
                .find(|o| path_dist(y, &[o.2, o.3], o.4) <= mtol)
            {
                by.insert(o.0.clone());
                return true;
            }
            false
        });
        if ok {
            x.evidence_ids.push(format!(
                "mirror_counterpart:{}",
                by.into_iter().collect::<Vec<_>>().join(",")
            ));
        } else {
            x.unknowns
                .push("No mirror counterpart about the view axis".into());
        }
    }
}

/// Middle of an inferred span (chord middle for a straight span).
fn implied_mid(x: &MdImpliedSpan) -> P {
    if x.arc.as_ref().is_some_and(|a| a.len() == 5) {
        return implied_at(x, 0.5);
    }
    [
        (x.points[0] + x.points[2]) / 2.,
        (x.points[1] + x.points[3]) / 2.,
    ]
}

/// Members drawn only because of a local section, with the evidence they carry
/// (`section_profile:<hatch>`, and the card and variant that say why): the
/// outline in the region that does not go on outside it in the same line type,
/// the hatch and the break curve (module `local_section`). The complete-structure
/// representation leaves them out; the as-drawn representation keeps them.
pub(super) fn section_profile(
    cx: &Ctx,
    b: &Builder,
    cov: &Cover,
    _implied: &[MdImpliedSpan],
) -> Vec<(usize, String)> {
    local_section::section_only(cx, b, &cov.sections)
        .marks
        .into_iter()
        .flat_map(|(i, ev)| {
            let have = &b.members[i].evidence_ids;
            ev.into_iter()
                .filter(|e| !have.contains(e))
                .map(move |e| (i, e))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A piece of a structural line: a source span (member) or an inferred span.
struct Piece {
    label: String,
    member: Option<usize>,
    /// Straight ends (arc: start and end points).
    ends: [P; 2],
    /// Circular carrier: centre, radius, start angle, counter-clockwise sweep.
    arc: Option<(P, f64, f64, f64)>,
    full: bool,
    poly: Option<Vec<P>>,
    line_type: String,
    len: f64,
}

/// A length for ordering pieces: lengths that differ only in the last digits (one DXF writer
/// rounds coordinates to 16 significant digits, another keeps the exact double) order the same,
/// so ties fall to the label and the result does not depend on how the source was printed.
fn len_key(len: f64) -> i64 {
    if len.is_finite() {
        (len * 1e6).round() as i64
    } else {
        i64::MAX
    }
}

/// GB/T 14665-2012 4.3 precedence of the line types an outline is drawn with.
fn type_rank(t: &str) -> usize {
    [
        "thick_continuous",
        "continuous",
        "thin_continuous",
        "thick_dashed",
        "thin_dashed",
    ]
    .iter()
    .position(|x| *x == t)
    .unwrap_or(9)
}

/// The union of `sel` covers `[a, b]` within `tol`.
fn covered(sel: &[[f64; 2]], a: f64, b: f64, tol: f64) -> bool {
    let mut v = sel.to_vec();
    v.sort_by(|x, y| x[0].total_cmp(&y[0]));
    let mut cursor = a;
    for x in v {
        if x[0] <= cursor + tol {
            cursor = cursor.max(x[1]);
        }
    }
    cursor >= b - tol
}

/// Distance from `p` to the source geometry of member `m` within its interval.
fn member_dist(cx: &Ctx, m: &MdExpressionMember, p: P) -> f64 {
    let h = m.source_handle.as_str();
    if let Some((a, c)) = cx.lines.get(h) {
        return seg_dist(p, lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1]));
    }
    if let Some((c, r, s0, sw, full)) = cx.arc(h) {
        let (a0, w) = (
            s0 + sw * m.interval[0],
            sw * (m.interval[1] - m.interval[0]),
        );
        return arc_dist(p, (c, r, a0, w, full && w >= std::f64::consts::TAU - 1e-9));
    }
    let pts = cx.points(h);
    if pts.len() == 1 {
        return dist(p, pts[0]);
    }
    path_dist(p, &pts, None)
}

/// Structural lines of the visible and hidden outline. Within one view and
/// class, straight pieces whose ends lie in a narrow band about the carrier of
/// the longest piece and that touch or overlap (up to a drafting gap) along it
/// form one line; co-circular arcs that touch form one arc, or a circle once
/// they close. Inferred spans join as pieces; a source span covered by the
/// others is a coincident member (drawn again). Polylines stay single lines.
/// Inside a local section that inferred outline crosses, a straight line whose
/// end meets only section-profile geometry is cut at the inferred outline and
/// the part beyond it is recorded as trimmed (it belongs to the section).
pub(super) fn structural_lines(
    cx: &Ctx,
    b: &Builder,
    cov: &Cover,
    implied: &[MdImpliedSpan],
) -> Vec<MdStructuralLine> {
    use std::f64::consts::TAU;
    let s = cx.scale.max(1.);
    let tol = 1e-3 * s;
    let otol = band(cx);
    let gtol = 0.05 * (s / 3.5).max(1.);
    let kind_of: BTreeMap<&str, &str> = b
        .exprs
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let class_of = |m: &MdExpressionMember| -> Option<&'static str> {
        match (
            kind_of.get(m.expression_id.as_str()).copied()?,
            m.member_role.as_str(),
        ) {
            ("visible_contour", "contour_segment") => Some("visible_outline"),
            ("hidden_contour", "hidden_segment") => Some("hidden_outline"),
            _ => None,
        }
    };
    let profile = |m: &MdExpressionMember| {
        m.evidence_ids
            .iter()
            .any(|e| e.starts_with("section_profile:"))
    };
    type Key = (Option<String>, &'static str);
    let mut groups: BTreeMap<Key, Vec<Piece>> = BTreeMap::new();
    for (i, m) in b.members.iter().enumerate() {
        let Some(class) = class_of(m) else { continue };
        if profile(m) {
            continue;
        }
        let h = m.source_handle.as_str();
        let label = format!("member:{}", m.id);
        let piece = if let Some((a, c)) = cx.lines.get(h) {
            let (p, q) = (lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1]));
            Piece {
                label,
                member: Some(i),
                ends: [p, q],
                arc: None,
                full: false,
                poly: None,
                line_type: m.line_type.clone(),
                len: dist(p, q),
            }
        } else if let Some((c, r, s0, sw, full)) = cx.arc(h) {
            let (a0, w) = (
                s0 + sw * m.interval[0],
                sw * (m.interval[1] - m.interval[0]),
            );
            let at = |a: f64| add(c, mul([a.cos(), a.sin()], r));
            Piece {
                label,
                member: Some(i),
                ends: [at(a0), at(a0 + w)],
                arc: Some((c, r, a0.rem_euclid(TAU), w.min(TAU))),
                full: full && w >= TAU - 1e-9,
                poly: None,
                line_type: m.line_type.clone(),
                len: r * w,
            }
        } else if let Some((pts, closed, _)) = cx.polys.get(h).filter(|p| !p.0.is_empty()) {
            let mut path = pts.clone();
            if *closed {
                path.push(pts[0]);
            }
            Piece {
                label,
                member: Some(i),
                ends: [pts[0], pts[pts.len() - 1]],
                arc: None,
                full: false,
                poly: Some(path),
                line_type: m.line_type.clone(),
                len: pts.windows(2).map(|w| dist(w[0], w[1])).sum(),
            }
        } else {
            continue;
        };
        groups
            .entry((m.view_id.clone(), class))
            .or_default()
            .push(piece);
    }
    for x in implied {
        let class = if x.line_class == "visible_outline" {
            "visible_outline"
        } else {
            "hidden_outline"
        };
        let (p, q) = ([x.points[0], x.points[1]], [x.points[2], x.points[3]]);
        let arc = x
            .arc
            .as_ref()
            .filter(|a| a.len() == 5)
            .map(|a| ([a[0], a[1]], a[2], a[3].rem_euclid(TAU), a[4]));
        groups
            .entry((x.view_id.clone(), class))
            .or_default()
            .push(Piece {
                label: x.id.clone(),
                member: None,
                ends: [p, q],
                arc,
                full: false,
                poly: None,
                line_type: String::new(),
                len: arc.map_or(dist(p, q), |a| a.1 * a.3),
            });
    }
    // Local sections that inferred outline crosses, with the inferred spans in them.
    let crossed: Vec<(&LocalSection, Vec<&MdImpliedSpan>)> = cov
        .sections
        .iter()
        .filter_map(|sec| {
            let xs: Vec<&MdImpliedSpan> = implied
                .iter()
                .filter(|x| x.cause == "local_section" && sec.holds(implied_mid(x), sec.near))
                .collect();
            (!xs.is_empty()).then_some((sec, xs))
        })
        .collect();
    let mut out: Vec<MdStructuralLine> = Vec::new();
    for ((view, class), pieces) in &groups {
        let mut done = vec![false; pieces.len()];
        let by_len = |idx: &mut Vec<usize>| {
            idx.sort_by(|&x, &y| {
                len_key(pieces[y].len)
                    .cmp(&len_key(pieces[x].len))
                    .then(pieces[x].label.cmp(&pieces[y].label))
            })
        };
        let line_type = |taken: &[usize]| -> (String, Vec<String>) {
            let mut types: Vec<String> = taken
                .iter()
                .filter(|&&j| pieces[j].member.is_some())
                .map(|&j| pieces[j].line_type.clone())
                .collect();
            types.sort_by(|x, y| type_rank(x).cmp(&type_rank(y)).then(x.cmp(y)));
            types.dedup();
            let lt = types.first().cloned().unwrap_or_else(|| {
                if *class == "visible_outline" {
                    "thick_continuous".into()
                } else {
                    "thin_dashed".into()
                }
            });
            (lt, types)
        };
        // Members first in primary order (precedence, length, label), then
        // inferred spans; a member covered by those before it is coincident.
        let primary_order = |idx: &mut Vec<usize>| {
            idx.sort_by(|&x, &y| {
                let (px, py) = (&pieces[x], &pieces[y]);
                px.member
                    .is_none()
                    .cmp(&py.member.is_none())
                    .then(type_rank(&px.line_type).cmp(&type_rank(&py.line_type)))
                    .then(py.full.cmp(&px.full))
                    .then(len_key(py.len).cmp(&len_key(px.len)))
                    .then(px.label.cmp(&py.label))
            })
        };
        let mut finish = |taken: Vec<usize>,
                          geometry: &str,
                          points: Vec<f64>,
                          arc: Option<Vec<f64>>,
                          pcs: Vec<usize>,
                          coincident: Vec<usize>,
                          mut evidence: Vec<String>,
                          trimmed: Vec<String>| {
            let (lt, types) = line_type(&taken);
            let mut unknowns = Vec::new();
            let inferred: Vec<&str> = pcs
                .iter()
                .filter(|&&j| pieces[j].member.is_none())
                .map(|&j| pieces[j].label.as_str())
                .collect();
            if !inferred.is_empty() {
                evidence.push(format!("inferred:{}", inferred.join(",")));
                unknowns.push(
                    "Part of the line is inferred across an explained interruption; no source line is drawn there"
                        .into(),
                );
            }
            if types.len() > 1 {
                evidence.push(format!("line_types:{}", types.join(",")));
                unknowns.push("Pieces are drawn with different line types".into());
            }
            if evidence.iter().any(|e| e.starts_with("joined_gap:")) {
                unknowns.push("Pieces are joined across an imprecise drafting gap".into());
            }
            if !trimmed.is_empty() {
                unknowns.push(
                    "The part inside the local section beyond the inferred outline belongs to the section representation"
                        .into(),
                );
            }
            let state = if taken.len() == 1 && trimmed.is_empty() {
                "observed"
            } else {
                "candidate"
            };
            out.push(MdStructuralLine {
                id: String::new(),
                view_id: view.clone(),
                line_class: (*class).into(),
                geometry: geometry.into(),
                line_type: lt,
                points,
                arc,
                pieces: pcs.iter().map(|&j| pieces[j].label.clone()).collect(),
                // A set: in label order, independent of last-digit length differences.
                coincident: {
                    let mut c: Vec<String> = coincident
                        .iter()
                        .map(|&j| pieces[j].label.clone())
                        .collect();
                    c.sort();
                    c
                },
                trimmed,
                evidence_ids: evidence,
                state: state.into(),
                unknowns,
            });
        };
        // Straight lines, grown from the longest unassigned piece. A piece joins
        // when both its ends lie in the band about the chord of the line so far
        // and it touches or overlaps the line along that chord (up to a drafting
        // gap); the chord then runs between the extreme ends, and every piece
        // must still lie in the band about it, so the result does not depend on
        // which piece is longest.
        let mut straight: Vec<usize> = (0..pieces.len())
            .filter(|&j| pieces[j].arc.is_none() && pieces[j].poly.is_none())
            .collect();
        by_len(&mut straight);
        let frame = |c: (P, P)| -> (P, P) {
            let u = unit(sub(c.1, c.0));
            let u = if u == [0., 0.] {
                [1., 0.]
            } else if u[0] < -1e-12 || (u[0].abs() <= 1e-12 && u[1] < 0.) {
                mul(u, -1.)
            } else {
                u
            };
            (c.0, u)
        };
        let chord_of = |set: &[usize], u: P| -> (P, P) {
            let mut ends = set.iter().flat_map(|&k| pieces[k].ends);
            let first = ends.next().unwrap_or([0., 0.]);
            let (mut lo, mut hi) = (first, first);
            for e in ends {
                if dot(e, u) < dot(lo, u) {
                    lo = e;
                }
                if dot(e, u) > dot(hi, u) {
                    hi = e;
                }
            }
            (lo, hi)
        };
        let in_band = |set: &[usize], c: (P, P)| {
            let (o, u) = frame(c);
            set.iter()
                .flat_map(|&k| pieces[k].ends)
                .all(|e| cross(u, sub(e, o)).abs() <= otol)
        };
        for &seed in &straight {
            if done[seed] {
                continue;
            }
            done[seed] = true;
            let mut taken = vec![seed];
            let mut chord = (pieces[seed].ends[0], pieces[seed].ends[1]);
            // A degenerate seed stays alone.
            let mut grew = pieces[seed].len >= tol;
            while grew {
                grew = false;
                for &j in &straight {
                    if done[j] {
                        continue;
                    }
                    let (o, u) = frame(chord);
                    if !in_band(&[j], chord) {
                        continue;
                    }
                    let along = |e: P| dot(sub(e, o), u);
                    let (a, c) = (along(pieces[j].ends[0]), along(pieces[j].ends[1]));
                    let (lo, hi) = (
                        along(chord.0).min(along(chord.1)),
                        along(chord.0).max(along(chord.1)),
                    );
                    if a.min(c) > hi + gtol || a.max(c) < lo - gtol {
                        continue;
                    }
                    let mut trial = taken.clone();
                    trial.push(j);
                    let next = chord_of(&trial, u);
                    if !in_band(&trial, next) {
                        continue;
                    }
                    taken = trial;
                    chord = next;
                    done[j] = true;
                    grew = true;
                }
            }
            let (o, u) = frame(chord);
            let along = |e: P| dot(sub(e, o), u);
            let span = |j: usize| {
                let (a, c) = (along(pieces[j].ends[0]), along(pieces[j].ends[1]));
                [a.min(c), a.max(c)]
            };
            let mut order = taken.clone();
            primary_order(&mut order);
            let mut sel: Vec<[f64; 2]> = Vec::new();
            let (mut pcs, mut coincident) = (Vec::new(), Vec::new());
            for &j in &order {
                let x = span(j);
                if pieces[j].member.is_some() && covered(&sel, x[0], x[1], tol) {
                    coincident.push(j);
                } else {
                    sel.push(x);
                    pcs.push(j);
                }
            }
            pcs.sort_by(|&x, &y| span(x)[0].total_cmp(&span(y)[0]).then(x.cmp(&y)));
            let mut evidence = vec![format!("pieces:{}", pcs.len())];
            if !coincident.is_empty() {
                evidence.push(format!("coincident:{}", coincident.len()));
            }
            // Gaps joined along the line (drafting imprecision).
            let mut ivs: Vec<[f64; 2]> = taken.iter().map(|&j| span(j)).collect();
            ivs.sort_by(|x, y| x[0].total_cmp(&y[0]));
            let mut reach = ivs.first().map_or(0., |x| x[1]);
            for x in ivs.iter().skip(1) {
                if x[0] > reach + tol {
                    evidence.push(format!("joined_gap:{:.5}", x[0] - reach));
                }
                reach = reach.max(x[1]);
            }
            let off = taken
                .iter()
                .flat_map(|&j| pieces[j].ends)
                .map(|e| cross(u, sub(e, o)).abs())
                .fold(0., f64::max);
            if off > tol {
                evidence.push(format!("near_collinear:{off:.5}"));
            }
            // Section trim at each end: the end lies in a crossed local section and
            // meets only section-profile geometry; cut at the nearest inferred span
            // crossing the line there.
            let (mut lo, mut hi) = (
                along(chord.0).min(along(chord.1)),
                along(chord.0).max(along(chord.1)),
            );
            let (mut p_lo, mut p_hi) = if along(chord.0) <= along(chord.1) {
                (chord.0, chord.1)
            } else {
                (chord.1, chord.0)
            };
            let own: BTreeSet<&str> = taken
                .iter()
                .filter_map(|&j| pieces[j].member)
                .map(|i| b.members[i].source_handle.as_str())
                .collect();
            let cut_from = |pe: P, end: f64, inward: f64, len: f64| -> Option<(f64, P, String)> {
                let (_, xs) = crossed.iter().find(|(sec, _)| sec.holds(pe, sec.near))?;
                let contacts: Vec<&MdExpressionMember> = b
                    .members
                    .iter()
                    .filter(|m| !own.contains(m.source_handle.as_str()))
                    .filter(|m| member_dist(cx, m, pe) <= tol)
                    .collect();
                if contacts.is_empty() || !contacts.iter().all(|m| profile(m)) {
                    return None;
                }
                let mut best: Option<(f64, P, &MdImpliedSpan)> = None;
                for x in xs
                    .iter()
                    .copied()
                    .filter(|x| x.arc.is_none() && x.view_id == *view)
                {
                    let (a, c) = ([x.points[0], x.points[1]], [x.points[2], x.points[3]]);
                    let (da, dc) = (cross(u, sub(a, o)), cross(u, sub(c, o)));
                    if (da - dc).abs() < 1e-12 || da * dc > 0. {
                        continue;
                    }
                    let xp = lerp(a, c, da / (da - dc));
                    let depth = (along(xp) - end) * inward;
                    if depth <= tol || depth >= len - tol {
                        continue;
                    }
                    if best.as_ref().is_none_or(|b| depth < b.0) {
                        best = Some((depth, xp, x));
                    }
                }
                let (depth, xp, x) = best?;
                Some((end + inward * depth, xp, x.id.clone()))
            };
            let mut cuts = Vec::new();
            if let Some((at, xp, by)) = cut_from(p_lo, lo, 1., hi - lo) {
                cuts.push((lo, at, by));
                lo = at;
                p_lo = xp;
            }
            if let Some((at, xp, by)) = cut_from(p_hi, hi, -1., hi - lo) {
                cuts.push((at, hi, by));
                hi = at;
                p_hi = xp;
            }
            let mut trimmed = Vec::new();
            for (c0, c1, by) in &cuts {
                evidence.push(format!("section_trim:{by}"));
                for &j in &taken {
                    let Some(i) = pieces[j].member else { continue };
                    let x = span(j);
                    let (a, c) = (x[0].max(*c0), x[1].min(*c1));
                    if c - a <= tol {
                        continue;
                    }
                    let m = &b.members[i];
                    let Some((sa, sc)) = cx.lines.get(&m.source_handle) else {
                        continue;
                    };
                    let (ta, tc) = (
                        project(add(o, mul(u, a)), *sa, *sc).0.clamp(0., 1.),
                        project(add(o, mul(u, c)), *sa, *sc).0.clamp(0., 1.),
                    );
                    trimmed.push(format!(
                        "member:{}:{:.6}:{:.6}",
                        m.id,
                        ta.min(tc),
                        ta.max(tc)
                    ));
                }
            }
            let _ = (lo, hi);
            finish(
                taken,
                "line",
                vec![p_lo[0], p_lo[1], p_hi[0], p_hi[1]],
                None,
                pcs,
                coincident,
                evidence,
                trimmed,
            );
        }
        // Arcs and circles.
        let mut arcs: Vec<usize> = (0..pieces.len())
            .filter(|&j| pieces[j].arc.is_some())
            .collect();
        arcs.sort_by(|&x, &y| {
            pieces[y]
                .full
                .cmp(&pieces[x].full)
                .then(len_key(pieces[y].len).cmp(&len_key(pieces[x].len)))
                .then(pieces[x].label.cmp(&pieces[y].label))
        });
        for &seed in &arcs {
            if done[seed] {
                continue;
            }
            let (c0, r0, a0, w0) = pieces[seed].arc.unwrap();
            let ag = gtol / r0.max(1e-9);
            let band: Vec<usize> = arcs
                .iter()
                .copied()
                .filter(|&j| {
                    !done[j]
                        && pieces[j].arc.is_some_and(|(c, r, _, _)| {
                            dist(c, c0) <= otol && (r - r0).abs() <= otol
                        })
                })
                .collect();
            // Grow along the circle from the seed (angles relative to its start).
            let (mut lo, mut hi) = (0., if pieces[seed].full { TAU } else { w0 });
            let mut place: BTreeMap<usize, f64> = BTreeMap::from([(seed, 0.)]);
            let mut gaps = Vec::new();
            loop {
                let mut grew = false;
                for &j in &band {
                    if place.contains_key(&j) || hi - lo >= TAU - ag {
                        continue;
                    }
                    let (_, _, aj, wj) = pieces[j].arc.unwrap();
                    let wj = if pieces[j].full { TAU } else { wj };
                    let rel = (aj - a0).rem_euclid(TAU);
                    for st in [rel, rel - TAU] {
                        if st <= hi + ag && st + wj >= lo - ag {
                            let gap = (st - hi).max(lo - (st + wj));
                            if gap > tol / r0.max(1e-9) {
                                gaps.push(gap * r0);
                            }
                            lo = f64::min(lo, st);
                            hi = f64::max(hi, st + wj);
                            place.insert(j, st);
                            grew = true;
                            break;
                        }
                    }
                }
                if !grew || hi - lo >= TAU - ag {
                    break;
                }
            }
            let full = hi - lo >= TAU - ag;
            if full {
                // A closed circle holds every co-circular arc of the band.
                for &j in &band {
                    place.entry(j).or_insert_with(|| {
                        let (_, _, aj, _) = pieces[j].arc.unwrap();
                        (aj - a0 - lo).rem_euclid(TAU) + lo
                    });
                }
            }
            let taken: Vec<usize> = place.keys().copied().collect();
            for &j in &taken {
                done[j] = true;
            }
            let ext = |j: usize| {
                let st = place[&j];
                let w = if pieces[j].full {
                    TAU
                } else {
                    pieces[j].arc.unwrap().3
                };
                [st, st + w]
            };
            let mut order = taken.clone();
            primary_order(&mut order);
            let mut sel: Vec<[f64; 2]> = Vec::new();
            let (mut pcs, mut coincident) = (Vec::new(), Vec::new());
            for &j in &order {
                let x = ext(j);
                let shifted: Vec<[f64; 2]> = sel
                    .iter()
                    .flat_map(|y| [-TAU, 0., TAU].map(|k| [y[0] + k, y[1] + k]))
                    .collect();
                if pieces[j].member.is_some() && covered(&shifted, x[0], x[1], tol / r0.max(1e-9)) {
                    coincident.push(j);
                } else {
                    sel.push(x);
                    pcs.push(j);
                }
            }
            pcs.sort_by(|&x, &y| ext(x)[0].total_cmp(&ext(y)[0]).then(x.cmp(&y)));
            let mut evidence = vec![format!("pieces:{}", pcs.len())];
            if !coincident.is_empty() {
                evidence.push(format!("coincident:{}", coincident.len()));
            }
            for gp in &gaps {
                evidence.push(format!("joined_gap:{gp:.5}"));
            }
            let at = |a: f64| add(c0, mul([a.cos(), a.sin()], r0));
            let start = (a0 + lo).rem_euclid(TAU);
            let (geometry, points, arc) = if full {
                let p = at(start);
                (
                    "circle",
                    vec![p[0], p[1]],
                    vec![c0[0], c0[1], r0, start, TAU],
                )
            } else {
                let (p, q) = (at(start), at(start + hi - lo));
                (
                    "arc",
                    vec![p[0], p[1], q[0], q[1]],
                    vec![c0[0], c0[1], r0, start, hi - lo],
                )
            };
            finish(
                taken,
                geometry,
                points,
                Some(arc),
                pcs,
                coincident,
                evidence,
                vec![],
            );
        }
        // Polylines.
        for j in 0..pieces.len() {
            let Some(pts) = &pieces[j].poly else { continue };
            done[j] = true;
            finish(
                vec![j],
                "polyline",
                pts.iter().flat_map(|p| [p[0], p[1]]).collect(),
                None,
                vec![j],
                vec![],
                vec!["pieces:1".into()],
                vec![],
            );
        }
    }
    for (n, l) in out.iter_mut().enumerate() {
        l.id = format!("line:{n}");
    }
    out
}
