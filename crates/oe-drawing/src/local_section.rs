//! The region of a local section and what is drawn only because of it (card
//! `local_section_boundary`).
//!
//! GB/T 17452-1998 6.3, GB/T 4458.6-2002 5.6: a local section cuts open a part of a view. The
//! break curve (a wavy line, a zigzag line, a chain of short lines drawn as one) separates it
//! from the rest of the view, which goes on beyond the curve. The region is the area enclosed by
//! the break curve(s) and the outline between their ends. It includes the voids where the cutting
//! plane meets no material and nothing is hatched, so it is not the extent of the hatch. Three
//! ways of closing it are read:
//! * one break curve whose two ends lie on the same outline line or circle;
//! * two break curves across the same pair of outlines: the band between them;
//! * a local section drawn inside a detail-indicator circle: the break curve runs from the
//!   outline to the circle, and the circle limits the hatch.
//!
//! The outline between the ends is drawn: it goes on from each end of the break curve along the
//! boundary and covers most of it (where the cut opens a feature into that outline, the outline
//! may be interrupted inside the region).
//!
//! Not a local section: a break curve at which the outline stops (the limit of a partial or
//! broken view, a section broken off), a break curve with no hatch inside, and a hatch with no
//! break curve (a view that is itself a section; an indicator circle lying over it bounds
//! nothing). A view named as a section (`X-X`) holds no local section either.
//!
//! Inside the region, a thick piece that does not continue across the boundary in the same line
//! type exists only because of the cut: the hidden edge drawn as cut (co-circular or collinear
//! with a hidden line ending at the same points), or the profile of the cut face. With the hatch
//! and the break curve it is section-only, and so is the same piece where it is lent to a
//! dimension as the outline its extension line starts from. Outline outside the region, a line
//! that crosses the region unchanged and a feature drawn identically outside the region (the hole
//! or pad of a pattern) are not.
use super::super::*;
use std::f64::consts::TAU;

/// A local section: its region and the evidence that bounds and fills it.
pub(super) struct LocalSection {
    /// Closed boundary: the break curve(s) and the outline between their ends.
    boundary: Vec<P>,
    bbox: [f64; 4],
    /// Break-curve sources.
    pub(super) curves: Vec<String>,
    /// Hatch pieces inside the region.
    pub(super) hatch: Vec<String>,
    /// Detail-indicator circle that limits it (variant `inside_detail_circle`).
    circle: Option<String>,
    variant: &'static str,
    /// Distance within which an interrupted end belongs to the region: a share of the text
    /// height, and no more than a share of the size of the region itself.
    pub(super) near: f64,
}

impl LocalSection {
    /// `p` lies in the region or within `margin` of its boundary.
    pub(super) fn holds(&self, p: P, margin: f64) -> bool {
        let b = &self.bbox;
        if p[0] < b[0] - margin
            || p[0] > b[2] + margin
            || p[1] < b[1] - margin
            || p[1] > b[3] + margin
        {
            return false;
        }
        inside(p, &self.boundary) || edge_dist(p, &self.boundary) <= margin
    }
    /// What bounds and fills the region, as cause evidence of an interrupted end.
    pub(super) fn evidence(&self) -> Vec<String> {
        let mut hs: Vec<&String> = self
            .curves
            .iter()
            .chain(&self.hatch)
            .chain(&self.circle)
            .collect();
        hs.sort();
        hs.dedup();
        hs.into_iter().map(|h| format!("section:{h}")).collect()
    }
    /// Evidence id carried by every member that exists only because of this section.
    fn mark(&self) -> String {
        format!(
            "section_profile:{}",
            self.hatch.last().map_or("", String::as_str)
        )
    }
}

/// Distance ratios of the card.
struct Reach {
    /// An end lies on a curve, a piece lies in the region (drawing units).
    on: f64,
    /// An interrupted end belongs to the region (drawing units).
    near: f64,
    /// The same, as a share of the size of the region (the square root of its area).
    near_share: f64,
}

fn reach(cx: &Ctx) -> Reach {
    let c = representation::card_ref("local_section_boundary").ok();
    let k = |name: &str, default: f64| {
        c.and_then(|c| representation::param(c, name).ok())
            .unwrap_or(default)
    };
    let s = cx.scale.max(1.);
    Reach {
        on: k("end_on_outline", 0.1) * s,
        near: k("end_near_region", 0.75) * s,
        near_share: k("end_near_region_share", 0.25),
    }
}

fn inside(p: P, poly: &[P]) -> bool {
    let n = poly.len();
    let mut c = false;
    for i in 0..n {
        let (a, z) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (z[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (z[0] - a[0]) / (z[1] - a[1]);
            if x > p[0] {
                c = !c;
            }
        }
    }
    c
}

fn edge_dist(p: P, poly: &[P]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| super::seg_dist(p, poly[i], poly[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

fn area(poly: &[P]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| cross(poly[i], poly[(i + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.
}

/// No two edges of the closed polygon cross each other, except within `tol` of one of the
/// points `near` (a freehand curve may wave across the outline it ends on).
fn simple(poly: &[P], near: &[P], tol: f64) -> bool {
    let n = poly.len();
    let side = |a: P, b: P, c: P| cross(sub(b, a), sub(c, a));
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        for j in i + 2..n {
            if (j + 1) % n == i {
                continue;
            }
            let (c, d) = (poly[j], poly[(j + 1) % n]);
            let (s1, s2) = (side(a, b, c), side(a, b, d));
            if s1 * s2 < 0. && side(c, d, a) * side(c, d, b) < 0. {
                let x = lerp(c, d, s1 / (s1 - s2));
                if !near.iter().any(|e| dist(*e, x) <= tol) {
                    return false;
                }
            }
        }
    }
    true
}

/// A drawn outline curve: a straight piece, or an arc (centre, radius, start, sweep, full).
enum Shape {
    Seg(P, P),
    Arc(P, f64, f64, f64, bool),
}

/// The infinite line (point, unit direction) or the whole circle a shape lies on.
#[derive(Clone, Copy)]
enum Carrier {
    Line(P, P),
    Circle(P, f64),
}

impl Shape {
    fn dist(&self, p: P) -> f64 {
        match *self {
            Shape::Seg(a, b) => super::seg_dist(p, a, b),
            Shape::Arc(c, r, s0, sw, full) => super::arc_dist(p, (c, r, s0, sw, full)),
        }
    }
    fn carrier(&self) -> Carrier {
        match *self {
            Shape::Seg(a, b) => Carrier::Line(a, unit(sub(b, a))),
            Shape::Arc(c, r, ..) => Carrier::Circle(c, r),
        }
    }
    /// Points of the drawn curve at `step` along it on either side of the point nearest `e`.
    fn beside(&self, e: P, step: f64) -> Vec<P> {
        match *self {
            Shape::Seg(a, b) => {
                let l = dist(a, b).max(1e-12);
                let t = project(e, a, b).0.clamp(0., 1.);
                [-1., 1.]
                    .iter()
                    .map(|s| t + s * step / l)
                    .filter(|t| (0. ..=1.).contains(t))
                    .map(|t| lerp(a, b, t))
                    .collect()
            }
            Shape::Arc(c, r, s0, sw, full) => {
                let at = |a: f64| add(c, mul([a.cos(), a.sin()], r));
                let ang = ((e[1] - c[1]).atan2(e[0] - c[0]) - s0).rem_euclid(TAU);
                // beyond the sweep the nearest point is one of the two ends
                let ang = if full || ang <= sw {
                    ang
                } else if ang - sw < TAU - ang {
                    sw
                } else {
                    0.
                };
                [-1., 1.]
                    .iter()
                    .map(|s| ang + s * step / r.max(1e-9))
                    .filter(|a| full || (0. ..=sw).contains(a))
                    .map(|a| at(s0 + a))
                    .collect()
            }
        }
    }
}

fn same(a: &Carrier, b: &Carrier, tol: f64) -> bool {
    match (*a, *b) {
        (Carrier::Line(p, u), Carrier::Line(q, v)) => {
            cross(u, v).abs() < 1e-3 && cross(u, sub(q, p)).abs() <= tol
        }
        (Carrier::Circle(c, r), Carrier::Circle(d, s)) => dist(c, d) <= tol && (r - s).abs() <= tol,
        _ => false,
    }
}

/// The ways from `a` to `b` along a carrier, without the two ends: the straight way, or the two
/// arcs of the circle.
fn ways(c: &Carrier, a: P, b: P) -> Vec<Vec<P>> {
    match *c {
        Carrier::Line(..) => vec![vec![]],
        Carrier::Circle(o, r) => {
            let aa = (a[1] - o[1]).atan2(a[0] - o[0]);
            let ccw = ((b[1] - o[1]).atan2(b[0] - o[0]) - aa).rem_euclid(TAU);
            [ccw, ccw - TAU]
                .iter()
                .map(|sweep| {
                    let n = ((sweep.abs() / 3f64.to_radians()).ceil() as usize).max(1);
                    (1..n)
                        .map(|i| {
                            let x = aa + sweep * i as f64 / n as f64;
                            add(o, mul([x.cos(), x.sin()], r))
                        })
                        .collect()
                })
                .collect()
        }
    }
}

/// A way with its two ends.
fn whole(a: P, way: Vec<P>, b: P) -> Vec<P> {
    std::iter::once(a).chain(way).chain([b]).collect()
}

/// A break curve as drawn: one source, or pieces joined end to end.
struct Path {
    pts: Vec<P>,
    ids: Vec<String>,
}

fn break_paths(cx: &Ctx, b: &Builder, tol: f64) -> Vec<Path> {
    let mut seen = BTreeSet::new();
    let mut pieces: Vec<(Vec<P>, String)> = Vec::new();
    for m in b
        .members
        .iter()
        .filter(|m| m.member_role == "break_curve" && m.native_member_id.is_none())
    {
        let h = m.source_handle.as_str();
        if !seen.insert(h) {
            continue;
        }
        // a break curve is open: it ends on the outline
        let pts = if let Some((a, c)) = cx.lines.get(h) {
            vec![*a, *c]
        } else if let Some((p, false, _)) = cx.polys.get(h) {
            p.clone()
        } else {
            continue;
        };
        if pts.len() >= 2 {
            pieces.push((pts, h.to_string()));
        }
    }
    pieces.sort_by(|x, y| x.1.cmp(&y.1));
    let n = pieces.len();
    let end = |i: usize, e: usize| {
        let p = &pieces[i].0;
        if e == 0 { p[0] } else { p[p.len() - 1] }
    };
    // Pieces whose end lies at `p`.
    let at = |p: P, own: usize| -> Vec<(usize, usize)> {
        (0..n)
            .filter(|&j| j != own)
            .flat_map(|j| (0..2).map(move |e| (j, e)))
            .filter(|&(j, e)| dist(end(j, e), p) <= tol)
            .collect()
    };
    let mut used = vec![false; n];
    let mut out = Vec::new();
    // Chains from their free ends first, then whatever is left (a chain closed on itself).
    for from_free_end in [true, false] {
        for i in 0..n {
            if used[i] {
                continue;
            }
            let start = if from_free_end {
                (0..2).find(|&e| at(end(i, e), i).is_empty())
            } else {
                Some(0)
            };
            let Some(start) = start else { continue };
            used[i] = true;
            let mut pts = pieces[i].0.clone();
            if start == 1 {
                pts.reverse();
            }
            let mut ids = vec![pieces[i].1.clone()];
            loop {
                let tail = pts[pts.len() - 1];
                let next: Vec<(usize, usize)> = at(tail, usize::MAX)
                    .into_iter()
                    .filter(|x| !used[x.0])
                    .collect();
                let [(j, e)] = next[..] else { break };
                used[j] = true;
                let mut q = pieces[j].0.clone();
                if e == 1 {
                    q.reverse();
                }
                pts.extend(q.into_iter().skip(1));
                ids.push(pieces[j].1.clone());
            }
            out.push(Path { pts, ids });
        }
    }
    out
}

/// Curves that can be outline: drawn thick by the drawing's own evidence, or dashed (hidden
/// outline), and not taken as a whole by an annotation.
fn outline_shapes(cx: &Ctx, b: &Builder) -> Vec<Shape> {
    let widths = line_evidence::Widths::of(cx);
    let mut out = Vec::new();
    for (h, r) in &cx.rec {
        if !matches!(r.kind.as_str(), "LINE" | "ARC" | "CIRCLE")
            || cx.arrows.contains_key(h)
            || exclusive(b, h)
        {
            continue;
        }
        if !(cx.style[h].pattern == Pattern::Dashed || widths.thick(cx, h)) {
            continue;
        }
        if let Some((a, c)) = cx.lines.get(h) {
            out.push(Shape::Seg(*a, *c));
        } else if let Some((c, r, s0, sw, full)) = cx.arc(h) {
            out.push(Shape::Arc(c, r, s0, sw, full));
        }
    }
    out
}

/// Points inside a hatch piece, which say where it lies: points along a section line between
/// its ends (the ends lie on the boundary of the hatched area), points of the filled area of a
/// hatch entity (the middles of its spans on a few scan lines).
fn hatch_points(cx: &Ctx, h: &str) -> Vec<P> {
    if let Some((a, c)) = cx.lines.get(h) {
        return [0.25, 0.5, 0.75].iter().map(|t| lerp(*a, *c, *t)).collect();
    }
    let Some(r) = cx.rec.get(h).filter(|r| r.kind == "HATCH") else {
        return vec![];
    };
    let segs = representation::hatch_boundary(r);
    let ends: Vec<P> = segs.iter().flat_map(|s| [s.0, s.1]).collect();
    let Some(bb) = bbox_of(&ends) else {
        return vec![];
    };
    let mut out = Vec::new();
    for i in 1..8 {
        let y = bb[1] + (bb[3] - bb[1]) * i as f64 / 8.;
        let mut xs: Vec<f64> = segs
            .iter()
            .filter(|(a, z)| (a[1] > y) != (z[1] > y))
            .map(|(a, z)| a[0] + (y - a[1]) * (z[0] - a[0]) / (z[1] - a[1]))
            .collect();
        xs.sort_by(f64::total_cmp);
        out.extend(xs.chunks_exact(2).map(|w| [(w[0] + w[1]) / 2., y]));
    }
    out
}

struct Candidate {
    poly: Vec<P>,
    paths: Vec<usize>,
    circle: Option<String>,
    variant: &'static str,
    /// Ends of the break curve(s) on the outline, where the view must go on.
    ends: Vec<P>,
    /// The closing ways along the outline, each with its two ends: one of them at least is
    /// drawn (the outline between the ends of the break curve).
    ways: Vec<Vec<P>>,
}

/// The local sections of the drawing, found from the break curves read so far.
pub(super) fn find(cx: &Ctx, b: &Builder) -> Vec<LocalSection> {
    let reach = reach(cx);
    let tol = reach.on;
    let paths = break_paths(cx, b, line_evidence::join_gap(cx));
    if paths.is_empty() {
        return vec![];
    }
    let kind_of: BTreeMap<&str, &str> = b
        .exprs
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let mut hatch: Vec<(String, Vec<P>)> = Vec::new();
    for m in b.members.iter().filter(|m| {
        m.native_member_id.is_none()
            && kind_of.get(m.expression_id.as_str()) == Some(&"section_hatch")
    }) {
        if hatch.iter().all(|x| x.0 != m.source_handle) {
            hatch.push((m.source_handle.clone(), hatch_points(cx, &m.source_handle)));
        }
    }
    hatch.sort_by(|x, y| x.0.cmp(&y.0));
    if hatch.is_empty() {
        return vec![];
    }
    let shapes = outline_shapes(cx, b);
    let circles: Vec<(P, f64, String)> = b
        .members
        .iter()
        .filter(|m| m.member_role == "detail_circle" && m.native_member_id.is_none())
        .filter_map(|m| {
            cx.arc(&m.source_handle)
                .map(|(c, r, ..)| (c, r, m.source_handle.clone()))
        })
        .collect();
    // Carriers of the outline a point lies on, one of each.
    let carriers = |p: P| -> Vec<Carrier> {
        let mut v: Vec<Carrier> = Vec::new();
        for s in shapes.iter().filter(|s| s.dist(p) <= tol) {
            let c = s.carrier();
            if !v.iter().any(|x| same(x, &c, tol)) {
                v.push(c);
            }
        }
        v
    };
    let shared = |p: P, q: P| -> Vec<Carrier> {
        let at_q = carriers(q);
        carriers(p)
            .into_iter()
            .filter(|c| at_q.iter().any(|d| same(c, d, tol)))
            .collect()
    };
    let open: Vec<usize> = (0..paths.len())
        .filter(|&i| dist(paths[i].pts[0], *paths[i].pts.last().unwrap()) > tol)
        .collect();
    let mut cands: Vec<Candidate> = Vec::new();
    for &i in &open {
        let pts = &paths[i].pts;
        let (e0, e1) = (pts[0], pts[pts.len() - 1]);
        // One break curve, both ends on the same outline.
        for c in shared(e0, e1) {
            for way in ways(&c, e1, e0) {
                let mut poly = pts.clone();
                poly.extend(way.iter().copied());
                cands.push(Candidate {
                    poly,
                    paths: vec![i],
                    circle: None,
                    variant: "between_curve_ends",
                    ends: vec![e0, e1],
                    ways: vec![whole(e1, way, e0)],
                });
            }
        }
        // Drawn inside a detail-indicator circle: from the outline to the circle.
        for (dc, dr, id) in &circles {
            // one end on the circle, the other inside it and farther from it
            let off = |p: P| dist(p, *dc) - dr;
            let reaches =
                |p: P, q: P| off(p).abs() <= tol && off(q) < 0. && off(q).abs() > off(p).abs();
            let (inner, run) = if reaches(e1, e0) {
                (e0, pts.iter().rev().copied().collect::<Vec<P>>())
            } else if reaches(e0, e1) {
                (e1, pts.clone())
            } else {
                continue;
            };
            let limit = run[0];
            for s in &shapes {
                let Shape::Seg(a, z) = *s else { continue };
                if s.dist(inner) > tol {
                    continue;
                }
                // where the drawn line leaves the circle
                let (u, l) = (unit(sub(z, a)), dist(a, z));
                let f = sub(a, *dc);
                let (bq, cq) = (dot(f, u), dot(f, f) - dr * dr);
                let disc = bq * bq - cq;
                if disc <= 0. {
                    continue;
                }
                for t in [-bq - disc.sqrt(), -bq + disc.sqrt()] {
                    if t < -tol || t > l + tol {
                        continue;
                    }
                    let x = add(a, mul(u, t));
                    for way in ways(&Carrier::Circle(*dc, *dr), x, limit) {
                        let mut poly = run.clone();
                        poly.push(x);
                        poly.extend(way);
                        cands.push(Candidate {
                            poly,
                            paths: vec![i],
                            circle: Some(id.clone()),
                            variant: "inside_detail_circle",
                            ends: vec![inner],
                            ways: vec![vec![inner, x]],
                        });
                    }
                }
            }
        }
    }
    // Two break curves across the same pair of outlines.
    for (n, &i) in open.iter().enumerate() {
        for &j in &open[n + 1..] {
            let (a, z) = (&paths[i].pts, &paths[j].pts);
            let (a0, a1) = (a[0], a[a.len() - 1]);
            for flip in [false, true] {
                let other: Vec<P> = if flip {
                    z.clone()
                } else {
                    z.iter().rev().copied().collect()
                };
                // `other` runs from the end facing a1 to the end facing a0
                let (b1, b0) = (other[0], other[other.len() - 1]);
                for c1 in shared(a1, b1) {
                    for c0 in shared(b0, a0) {
                        for w1 in ways(&c1, a1, b1) {
                            for w0 in ways(&c0, b0, a0) {
                                let mut poly = a.clone();
                                poly.extend(w1.iter().copied());
                                poly.extend(other.iter().copied());
                                poly.extend(w0.iter().copied());
                                cands.push(Candidate {
                                    poly,
                                    paths: vec![i, j],
                                    circle: None,
                                    variant: "band_between_break_curves",
                                    ends: vec![a0, a1, b0, b1],
                                    ways: vec![
                                        whole(a1, w1.clone(), b1),
                                        whole(b0, w0.clone(), a0),
                                    ],
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    let mut found: Vec<(f64, Candidate, Vec<String>)> = Vec::new();
    for c in cands {
        let size = area(&c.poly);
        if c.poly.len() < 3 || size <= tol * tol || !simple(&c.poly, &c.ends, tol) {
            continue;
        }
        // the outline between the ends of the break curve is drawn: it goes on from each end along
        // the way and covers most of it
        let drawn = |way: &Vec<P>| {
            let mut on: Vec<bool> = Vec::new();
            for w in way.windows(2) {
                let n = ((dist(w[0], w[1]) / (0.5 * tol)).ceil() as usize).clamp(1, 4000);
                for i in 0..n {
                    let q = lerp(w[0], w[1], i as f64 / n as f64);
                    on.push(shapes.iter().any(|s| s.dist(q) <= tol));
                }
            }
            on.push(true);
            let k = 6.min(on.len());
            on[..k].iter().all(|x| *x)
                && on[on.len() - k..].iter().all(|x| *x)
                && 2 * on.iter().filter(|x| **x).count() >= on.len()
        };
        if !c.ways.iter().any(drawn) {
            continue;
        }
        let holds = |p: P, margin: f64| inside(p, &c.poly) || edge_dist(p, &c.poly) <= margin;
        // no other break curve inside
        if paths.iter().enumerate().any(|(k, p)| {
            let m = p.pts[p.pts.len() / 2];
            !c.paths.contains(&k) && inside(m, &c.poly) && edge_dist(m, &c.poly) > tol
        }) {
            continue;
        }
        let filled: Vec<String> = hatch
            .iter()
            .filter(|(_, pts)| {
                !pts.is_empty()
                    && 2 * pts.iter().filter(|p| inside(**p, &c.poly)).count() > pts.len()
            })
            .map(|x| x.0.clone())
            .collect();
        if filled.is_empty() {
            continue;
        }
        // the view goes on beyond the break curve: at each of its ends on the outline, an
        // outline through the end leaves it outside the region
        let goes_on = |e: P| {
            shapes.iter().filter(|s| s.dist(e) <= tol).any(|s| {
                s.beside(e, 3. * tol)
                    .into_iter()
                    .any(|q| !holds(q, 0.5 * tol))
            })
        };
        if !c.ends.iter().all(|e| goes_on(*e)) {
            continue;
        }
        found.push((size, c, filled));
    }
    found.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut taken: BTreeSet<usize> = BTreeSet::new();
    let mut out = Vec::new();
    for (size, c, filled) in found {
        if c.paths.iter().any(|k| taken.contains(k)) {
            continue;
        }
        taken.extend(c.paths.iter().copied());
        let mut curves: Vec<String> = c
            .paths
            .iter()
            .flat_map(|&k| paths[k].ids.iter().cloned())
            .collect();
        curves.sort();
        out.push(LocalSection {
            bbox: bbox_of(&c.poly).unwrap_or([0.; 4]),
            boundary: c.poly,
            curves,
            hatch: filled,
            circle: c.circle,
            variant: c.variant,
            near: reach.near.min(reach.near_share * size.sqrt()),
        });
    }
    out
}

/// Members that exist only because of a local section, with the evidence they carry, and which
/// of the sections stand once the views are known.
pub(super) struct SectionOnly {
    pub(super) live: Vec<bool>,
    pub(super) marks: BTreeMap<usize, Vec<String>>,
}

/// Sample points of the span of a member along its source.
fn span_points(cx: &Ctx, m: &MdExpressionMember) -> Vec<P> {
    let h = m.source_handle.as_str();
    if let Some((a, c)) = cx.lines.get(h) {
        return (0..=4)
            .map(|i| {
                lerp(
                    *a,
                    *c,
                    m.interval[0] + (m.interval[1] - m.interval[0]) * i as f64 / 4.,
                )
            })
            .collect();
    }
    if let Some((c, r, s0, sw, _)) = cx.arc(h) {
        let n = ((sw / 5f64.to_radians()).ceil() as usize).clamp(4, 72);
        return (0..=n)
            .map(|i| {
                let a = s0 + sw * i as f64 / n as f64;
                add(c, mul([a.cos(), a.sin()], r))
            })
            .collect();
    }
    cx.points(h)
}

/// Whether two members lie on one carrier: collinear lines, or co-circular arcs and circles.
fn one_carrier(cx: &Ctx, m: &MdExpressionMember, o: &MdExpressionMember, tol: f64) -> bool {
    let (h, k) = (m.source_handle.as_str(), o.source_handle.as_str());
    if let (Some((a, c)), Some((p, q))) = (cx.lines.get(h), cx.lines.get(k)) {
        let u = unit(sub(*c, *a));
        return cross(u, sub(*p, *a)).abs() <= tol && cross(u, sub(*q, *a)).abs() <= tol;
    }
    if let (Some((c, r, ..)), Some((d, s, ..))) = (cx.arc(h), cx.arc(k)) {
        return dist(c, d) <= tol && (r - s).abs() <= tol;
    }
    false
}

pub(super) fn section_only(cx: &Ctx, b: &Builder, secs: &[LocalSection]) -> SectionOnly {
    let mut out = SectionOnly {
        live: vec![true; secs.len()],
        marks: BTreeMap::new(),
    };
    if secs.is_empty() {
        return out;
    }
    let reach = reach(cx);
    let tol = reach.on;
    let band = super::band(cx);
    let kind_of: BTreeMap<&str, &str> = b
        .exprs
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let kind =
        |m: &MdExpressionMember| kind_of.get(m.expression_id.as_str()).copied().unwrap_or("");
    // A view named as a section is cut as a whole: nothing in it is a local section.
    let section_views: BTreeSet<&str> = b
        .exprs
        .iter()
        .filter(|e| {
            e.kind == "view_callout"
                && e.qualifiers.iter().any(|q| q == "view_title")
                && view_layout::section_letters(&e.label).is_some()
        })
        .filter_map(|e| e.view_id.as_deref())
        .collect();
    for (i, s) in secs.iter().enumerate() {
        out.live[i] = !b.members.iter().any(|m| {
            m.member_role == "break_curve"
                && s.curves.contains(&m.source_handle)
                && m.view_id
                    .as_deref()
                    .is_some_and(|v| section_views.contains(v))
        });
    }
    let visible: Vec<(usize, &MdExpressionMember, Vec<P>)> = b
        .members
        .iter()
        .enumerate()
        .filter(|(_, m)| kind(m) == "visible_contour" && m.member_role == "contour_segment")
        .map(|(i, m)| (i, m, span_points(cx, m)))
        .collect();
    let hidden: Vec<&MdExpressionMember> = b
        .members
        .iter()
        .filter(|m| kind(m) == "hidden_contour" && m.member_role == "hidden_segment")
        .collect();
    let live: Vec<&LocalSection> = secs
        .iter()
        .zip(&out.live)
        .filter(|x| *x.1)
        .map(|x| x.0)
        .collect();
    let in_a_section = |pts: &[P]| live.iter().any(|s| pts.iter().all(|p| s.holds(*p, tol)));
    let ends = |pts: &[P]| [pts[0], pts[pts.len() - 1]];
    for s in &live {
        let mark = s.mark();
        // outline members marked for this section: source and interval
        let mut profile: Vec<(&str, [f64; 2])> = Vec::new();
        for (i, m) in b.members.iter().enumerate() {
            let own = match (kind(m), m.member_role.as_str()) {
                ("section_hatch", _) => s.hatch.contains(&m.source_handle),
                (_, "break_curve") => s.curves.contains(&m.source_handle),
                _ => false,
            };
            if own && m.member_role == "break_curve" {
                out.marks.insert(
                    i,
                    vec![
                        mark.clone(),
                        "card:local_section_boundary".into(),
                        format!("variant:{}", s.variant),
                    ],
                );
            } else if own {
                out.marks.insert(i, vec![mark.clone()]);
            }
        }
        for (i, m, pts) in &visible {
            // A contour drawn twice is shown by its primary, which takes the mark.
            if m.evidence_ids
                .iter()
                .any(|x| x.starts_with("duplicate_of:"))
                || pts.is_empty()
                || !pts.iter().all(|p| s.holds(*p, tol))
            {
                continue;
            }
            // The same line goes on outside the region in the same line type.
            let continued = visible.iter().any(|(j, o, q)| {
                j != i
                    && o.view_id == m.view_id
                    && one_carrier(cx, m, o, band)
                    && q.iter().any(|p| !s.holds(*p, tol))
                    && q.iter().any(|p| s.holds(*p, s.near))
            });
            // The same feature is drawn identically outside every local section: a circle of the
            // same size, an arc of the same sweep on the same circle (a pattern position).
            let repeated = cx.arc(&m.source_handle).is_some_and(|(c, r, _, sw, full)| {
                visible.iter().any(|(j, o, q)| {
                    j != i
                        && o.view_id == m.view_id
                        && !in_a_section(q)
                        && cx
                            .arc(&o.source_handle)
                            .is_some_and(|(d, r2, _, sw2, full2)| {
                                full == full2
                                    && (r - r2).abs() <= band
                                    && (full
                                        || (dist(c, d) <= band && ((sw - sw2) * r).abs() <= tol))
                            })
                })
            });
            if continued || repeated {
                continue;
            }
            // A hidden line on the same carrier ends where this piece ends: the hidden edge
            // itself, drawn thick where the cut lays it open.
            let revealed = hidden.iter().any(|o| {
                o.view_id == m.view_id && one_carrier(cx, m, o, band) && {
                    let q = span_points(cx, o);
                    !q.is_empty()
                        && ends(&q)
                            .iter()
                            .any(|e| ends(pts).iter().any(|p| dist(*e, *p) <= tol))
                }
            });
            let ev = vec![
                mark.clone(),
                "card:local_section_boundary".into(),
                format!(
                    "variant:{}",
                    if revealed {
                        "revealed_hidden_edge"
                    } else {
                        "cut_face_profile"
                    }
                ),
            ];
            profile.push((m.source_handle.as_str(), [m.interval[0], m.interval[1]]));
            out.marks.insert(*i, ev);
        }
        // The outline an extension line starts from may be lent to the dimension
        // (`extension_origin`, same line type): that membership is the same piece of the same
        // line, so it is section-only wherever the outline is. Only a lent span lying within
        // the marked outline takes the mark; one that also covers a use outside it does not.
        let eps = 1e-6;
        for (j, m) in b.members.iter().enumerate() {
            if m.member_role != "extension_origin" || out.marks.contains_key(&j) {
                continue;
            }
            let mut ivs: Vec<[f64; 2]> = profile
                .iter()
                .filter(|(h, iv)| {
                    *h == m.source_handle.as_str()
                        && iv[1].min(m.interval[1]) - iv[0].max(m.interval[0]) > eps
                })
                .map(|x| x.1)
                .collect();
            ivs.sort_by(|a, z| a[0].total_cmp(&z[0]));
            let mut cursor = m.interval[0];
            for iv in &ivs {
                if iv[0] <= cursor + eps {
                    cursor = cursor.max(iv[1]);
                }
            }
            if !ivs.is_empty() && cursor >= m.interval[1] - eps {
                out.marks.insert(
                    j,
                    vec![
                        mark.clone(),
                        "card:local_section_boundary".into(),
                        "variant:lent_to_dimension".into(),
                    ],
                );
            }
        }
    }
    out
}
