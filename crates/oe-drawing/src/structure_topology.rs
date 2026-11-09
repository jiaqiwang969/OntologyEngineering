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
//! * GB/T 4458.1 / 4457.4: inside a local section the outline is replaced by
//!   the section; an end inside a hatched region bounded by a break curve or a
//!   detail circle is interrupted by that section.
//!
//! Outline inferred across an explained interruption is kept apart from source
//! members (complete-structure representation only). A gap whose facing ends
//! meet other structure (a slot cutting through a line) is a real opening and
//! is never bridged. Straight lines continue collinearly and arcs along their
//! circle, so a circle broken for a text is treated like a broken line.
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

/// Text box: centre, unit direction, half width, half height, height, handle.
type TextBox = (P, P, f64, f64, f64, String);

/// Evidence that can explain an interruption of an outline line.
pub(super) struct Cover {
    texts: Vec<TextBox>,
    /// Annotation segments (dimension, extension and leader lines, terminators):
    /// end points, half width and source handle.
    strips: Vec<(P, P, f64, String)>,
    /// Section hatch clusters: bounding box, evidence handles (hatch pieces,
    /// boundary curves and detail circles meeting the cluster) and hatch pieces.
    sections: Vec<([f64; 4], Vec<String>, Vec<String>)>,
    /// Detail circles: centre, radius and source handle.
    circles: Vec<(P, f64, String)>,
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
    // Hatch pieces clustered by proximity, so a grouping by view cannot merge
    // separate sections into one region.
    let kind_of: BTreeMap<&str, &str> = b
        .exprs
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let s = cx.scale.max(1.);
    let mut pieces: Vec<([f64; 4], String)> = b
        .members
        .iter()
        .filter(|m| kind_of.get(m.expression_id.as_str()) == Some(&"section_hatch"))
        // parts of a block reference are placed through the block, not by their root
        .filter(|m| m.native_member_id.is_none())
        .filter_map(|m| {
            cx.bbox(&m.source_handle)
                .map(|bb| (bb, m.source_handle.clone()))
        })
        .collect();
    pieces.sort_by(|x, y| x.1.cmp(&y.1));
    pieces.dedup_by(|x, y| x.1 == y.1);
    let near = |x: &[f64; 4], y: &[f64; 4], g: f64| {
        x[0] - g <= y[2] && y[0] - g <= x[2] && x[1] - g <= y[3] && y[1] - g <= x[3]
    };
    let mut clusters: Vec<([f64; 4], Vec<String>)> = Vec::new();
    for (bb, h) in pieces {
        let mut merged = ([bb[0], bb[1], bb[2], bb[3]], vec![h]);
        let mut rest = Vec::new();
        for c in clusters {
            if near(&c.0, &merged.0, 1.5 * s) {
                merged.0 = [
                    merged.0[0].min(c.0[0]),
                    merged.0[1].min(c.0[1]),
                    merged.0[2].max(c.0[2]),
                    merged.0[3].max(c.0[3]),
                ];
                merged.1.extend(c.1);
            } else {
                rest.push(c);
            }
        }
        rest.push(merged);
        clusters = rest;
    }
    // A section is bounded by a break curve or a detail circle that meets it.
    let mut sections = Vec::new();
    for (bb, hatch) in clusters {
        let mut hs = hatch.clone();
        let bounds: Vec<String> = b
            .members
            .iter()
            .filter(|m| matches!(m.member_role.as_str(), "break_curve" | "detail_circle"))
            .filter(|m| m.native_member_id.is_none())
            .filter(|m| {
                cx.bbox(&m.source_handle)
                    .is_some_and(|x| near(&x, &bb, 0.5 * s))
            })
            .map(|m| m.source_handle.clone())
            .collect();
        if bounds.is_empty() {
            continue;
        }
        hs.extend(bounds);
        hs.sort();
        hs.dedup();
        sections.push((bb, hs, hatch));
    }
    let circles = b
        .members
        .iter()
        .filter(|m| m.member_role == "detail_circle")
        .filter_map(|m| cx.arc(&m.source_handle).map(|(c, r, ..)| (c, r, m.source_handle.clone())))
        .collect();
    Cover {
        texts,
        strips,
        sections,
        circles,
    }
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
    {
        return Some(Cause {
            kind: "text_break",
            partner: Some(facing(k, i)),
            evidence: ev,
        });
    }
    for (bb, hs, _) in &cov.sections {
        let g = 0.5 * s;
        let inside =
            |q: P| q[0] >= bb[0] - g && q[0] <= bb[2] + g && q[1] >= bb[1] - g && q[1] <= bb[3] + g;
        if !inside(p) {
            continue;
        }
        let partner = best
            .filter(|(len, k, i)| inside(k.ends[*i]) && inside(add(p, mul(d, len / 2.))))
            .map(|(_, k, i)| facing(k, i));
        return Some(Cause {
            kind: "local_section",
            partner,
            evidence: hs.iter().map(|h| format!("section:{h}")).collect(),
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
/// covered by a text, or both ends inside a local section. Returns the cause
/// and the gap arc (start angle, counter-clockwise sweep). An arc may face its
/// own other end (a circle broken once).
fn arc_interruption(
    cx: &Ctx,
    cov: &Cover,
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
    for q in partners {
        if dist(q.c, own.c) > w || (q.r - own.r).abs() > w {
            continue;
        }
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
    if q.free[fi] {
        let along = |i: usize, n: usize| at(start + delta * i as f64 / n as f64);
        if let Some(ev) = text_break_along(cov, &along, delta * own.r, delta * own.r) {
            return Some((
                Cause {
                    kind: "text_break",
                    partner: Some(facing),
                    evidence: ev,
                },
                gap,
            ));
        }
    }
    let g = 0.5 * s;
    for (bb, hs, _) in &cov.sections {
        let inside =
            |x: P| x[0] >= bb[0] - g && x[0] <= bb[2] + g && x[1] >= bb[1] - g && x[1] <= bb[3] + g;
        if inside(p) && inside(qp) && inside(at(start + delta / 2.)) {
            return Some((
                Cause {
                    kind: "local_section",
                    partner: Some(facing),
                    evidence: hs.iter().map(|h| format!("section:{h}")).collect(),
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
/// inferred across explained interruptions.
pub(super) fn audit(
    cx: &Ctx,
    b: &Builder,
    cov: &Cover,
) -> (Vec<MdStructuralEnd>, Vec<MdImpliedSpan>) {
    let s = cx.scale.max(1.);
    let tol = 1e-3 * s;
    let exprs: BTreeMap<&str, &MdEngineeringExpression> =
        b.exprs.iter().map(|e| (e.id.as_str(), e)).collect();
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
            _ => continue,
        };
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
    let gap = 0.05 * (s / 3.5).max(1.);
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
                    &own,
                    k,
                    &arc_partners[&(sp.class, sp.view.clone())],
                )
            }) {
                gap_arc = sp.arc.map(|a| vec![a.0[0], a.0[1], a.1, g[0], g[1]]);
                Some(c)
            } else {
                let ps_ = &partners[&(sp.class, sp.view.clone())];
                interruption(cx, cov, &sp.handle, ps[k], dirs[k], ps_)
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

/// Members drawn only because a local section replaces the outline: inside a
/// hatched local section that an inferred outline span crosses, the section
/// profile (outline lying wholly in the region and not continuing collinearly
/// outside it), the hatch and the section boundary. The complete-structure
/// representation hides them; the as-drawn representation keeps them. A section
/// no inferred outline crosses (a sectional detail view) is left alone.
pub(super) fn section_profile(
    cx: &Ctx,
    b: &Builder,
    cov: &Cover,
    implied: &[MdImpliedSpan],
) -> Vec<(usize, String)> {
    let s = cx.scale.max(1.);
    let g = 0.5 * s;
    let tol = 2e-3 * s;
    let kind_of: BTreeMap<&str, &str> = b
        .exprs
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let member_points = |m: &MdExpressionMember| -> Vec<P> {
        if let Some((a, c)) = cx.lines.get(&m.source_handle) {
            return vec![lerp(*a, *c, m.interval[0]), lerp(*a, *c, m.interval[1])];
        }
        if let Some((c, r, s0, sw, _)) = cx.arc(&m.source_handle) {
            return (0..=8)
                .map(|i| {
                    let a = s0 + sw * i as f64 / 8.;
                    add(c, mul([a.cos(), a.sin()], r))
                })
                .collect();
        }
        cx.points(&m.source_handle)
    };
    let mut marks = Vec::new();
    for (bb, _, hatch) in &cov.sections {
        let inside =
            |q: P| q[0] >= bb[0] - g && q[0] <= bb[2] + g && q[1] >= bb[1] - g && q[1] <= bb[3] + g;
        let crossed = implied
            .iter()
            .any(|x| x.cause == "local_section" && inside(implied_mid(x)));
        let Some(label) = hatch.first() else { continue };
        if !crossed {
            continue;
        }
        let label = format!("section_profile:{label}");
        for (i, m) in b.members.iter().enumerate() {
            let kind = kind_of.get(m.expression_id.as_str()).copied().unwrap_or("");
            let pts = member_points(m);
            if pts.is_empty() || !pts.iter().all(|q| inside(*q)) {
                continue;
            }
            let take = match (kind, m.member_role.as_str()) {
                ("section_hatch", _) => hatch.contains(&m.source_handle),
                (_, "break_curve") => true,
                ("visible_contour", "contour_segment") => {
                    // Outline continuing collinearly outside the region is external outline.
                    let continued = pts.len() == 2 && dist(pts[0], pts[1]) > tol && {
                        let d = unit(sub(pts[1], pts[0]));
                        b.members.iter().enumerate().any(|(j, o)| {
                            j != i
                                && kind_of.get(o.expression_id.as_str()) == Some(&"visible_contour")
                                && o.member_role == "contour_segment"
                                && cx.lines.get(&o.source_handle).is_some_and(|(a, c)| {
                                    let (p, q) =
                                        (lerp(*a, *c, o.interval[0]), lerp(*a, *c, o.interval[1]));
                                    !(inside(p) && inside(q))
                                        && cross(d, sub(p, pts[0])).abs() <= tol
                                        && cross(d, sub(q, pts[0])).abs() <= tol
                                        && {
                                            let (s0, s1) =
                                                (dot(sub(p, pts[0]), d), dot(sub(q, pts[0]), d));
                                            s0.min(s1) <= dist(pts[0], pts[1]) + tol
                                                && s0.max(s1) >= -tol
                                        }
                                })
                        })
                    };
                    !continued
                }
                _ => false,
            };
            if take {
                marks.push((i, label.clone()));
            }
        }
    }
    marks
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
    let g = 0.5 * s;
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
    let crossed: Vec<([f64; 4], Vec<&MdImpliedSpan>)> = cov
        .sections
        .iter()
        .filter_map(|(bb, _, _)| {
            let inside = |q: P| {
                q[0] >= bb[0] - g && q[0] <= bb[2] + g && q[1] >= bb[1] - g && q[1] <= bb[3] + g
            };
            let xs: Vec<&MdImpliedSpan> = implied
                .iter()
                .filter(|x| x.cause == "local_section" && inside(implied_mid(x)))
                .collect();
            (!xs.is_empty()).then_some((*bb, xs))
        })
        .collect();
    let in_bb = |q: P, bb: &[f64; 4]| {
        q[0] >= bb[0] - g && q[0] <= bb[2] + g && q[1] >= bb[1] - g && q[1] <= bb[3] + g
    };
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
                let (_, xs) = crossed.iter().find(|(bb, _)| in_bb(pe, bb))?;
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
