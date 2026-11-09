//! Vectorisation of the ink into line and arc strokes, and merging of collinear line strokes.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use crate::Ink;
use crate::geom::{
    P, add, dist, dist_point_segment, dot, fit_circle, fit_line, median, mul, perp, sub,
    undirected_angle, unit, wrap_pi,
};
use crate::members::SUPPORT_RADIUS;
use crate::pattern::{self, Dsu};
use crate::skeleton;
use crate::stroke::{Pattern, Primitive, Stroke, line_direction};

/// Douglas–Peucker tolerance: a pixel run whose chord deviation stays within this is straight.
const LINE_TOL: f64 = 1.0;
/// Arc acceptance: RMS and maximum radial residual of the skeleton pixels (pixels).
const ARC_RMS_TOL: f64 = 0.8;
const ARC_MAX_TOL: f64 = 2.0;
/// Radii outside this range are not arcs (the run is split into lines instead).
const MIN_RADIUS: f64 = 2.0;
const MAX_RADIUS: f64 = 1e5;
/// Minimum skeleton pixels for an arc attempt.
const MIN_ARC_PIXELS: usize = 6;
/// Half-width search cap of the distance transform (pixels).
const MAX_HALF_WIDTH: i64 = 64;
/// Widths are measured on at most this many skeleton pixels per stroke.
const WIDTH_SAMPLES: usize = 64;

/// Vectorise the ink into strokes.
///
/// Pipeline: Zhang–Suen thinning pruned to a minimal skeleton → tracing into pixel chains between
/// endpoints and junctions (8-connectivity; branches share the junction pixel) → recursive
/// Douglas–Peucker split (tolerance 1 px): a run whose chord deviation is within tolerance is a
/// `Line` through the orthogonal least-squares fit of its pixels; otherwise a least-squares circle is
/// tried and accepted as an `Arc` when the RMS radial residual is below 0.8 px (maximum 2 px), the
/// radius lies in `[2, 1e5]` px and the pixels progress monotonically around the centre; otherwise the
/// run is split at its farthest pixel. Closed chains are tried as full circles first.
///
/// Width is the median over skeleton pixels of the perpendicular ink extent, capped by twice the
/// distance to the nearest background pixel. Line endpoints are extended along the line by up to
/// half the width while the ink continues, to undo the end recession of thinning. Support is the
/// fraction of 1-px samples along the primitive with ink within 1.5 px; `pixels` is the number of
/// skeleton pixels in the run.
///
/// Patterns are inferred from collinear / co-circular pieces with regular gaps. Strokes shorter than
/// `min_length_px` are dropped unless they belong to a `Dashed` or `DashDot` run (dots are short by
/// nature). Nothing else is filtered: blobs come out as short, wide strokes for the caller to judge.
pub fn vectorize(ink: &Ink, min_length_px: f64) -> Vec<Stroke> {
    vectorize_in(ink, min_length_px, &pattern::PatternContext::default())
}

/// [`vectorize`] with the page resolution and the original page ink for the line patterns: the ink vectorized may
/// be a remainder with strokes erased from it, and a gap between pieces counts as a dash gap only where the
/// original page is blank.
pub fn vectorize_in(ink: &Ink, min_length_px: f64, ctx: &pattern::PatternContext) -> Vec<Stroke> {
    let grid = skeleton::thin(ink);
    let chains = skeleton::trace(&grid);
    let mut strokes = Vec::new();
    for chain in &chains {
        let pts: Vec<P> = chain
            .pts
            .iter()
            .map(|&(x, y)| (x as f64, y as f64))
            .collect();
        for (mut prim, run) in segment_chain(&pts, chain.closed) {
            let width_px = stroke_width(ink, &run, &prim);
            if let Primitive::Line { p, q } = prim {
                let ext = ((width_px - 1.0) * 0.5).max(0.0);
                prim = Primitive::Line {
                    p: extend_to_ink(ink, q, p, ext),
                    q: extend_to_ink(ink, p, q, ext),
                };
            }
            let support = primitive_support(ink, &prim);
            strokes.push(Stroke {
                primitive: prim,
                width_px,
                pattern: Pattern::Continuous,
                support,
                pixels: run.len(),
            });
        }
    }
    pattern::assign(&mut strokes, ctx);
    let min_len = if min_length_px.is_finite() {
        min_length_px.max(0.0)
    } else {
        0.0
    };
    strokes.retain(|s| {
        s.primitive.length() >= min_len || matches!(s.pattern, Pattern::Dashed | Pattern::DashDot)
    });
    strokes
}

/// Fraction of 1-px samples along the primitive with ink within 1.5 px.
fn primitive_support(ink: &Ink, prim: &Primitive) -> f64 {
    let len = prim.length();
    if !len.is_finite() {
        return 0.0;
    }
    let n = (len.ceil() as usize).max(1) + 1;
    let mut hits = 0usize;
    for i in 0..n {
        let (x, y) = prim.point_at(i as f64 / (n - 1) as f64);
        if ink.has_dark_within(x, y, SUPPORT_RADIUS) {
            hits += 1;
        }
    }
    hits as f64 / n as f64
}

/// Move `end` away from `from` by up to `max_ext` px in 0.5-px steps while the ink stays dark.
fn extend_to_ink(ink: &Ink, from: P, end: P, max_ext: f64) -> P {
    let Some(d) = unit(sub(end, from)) else {
        return end;
    };
    let mut last = 0.0;
    let mut s = 0.5;
    while s <= max_ext + 1e-9 {
        let pt = add(end, mul(d, s));
        if ink.is_dark(pt.0.round() as i64, pt.1.round() as i64) {
            last = s;
        } else {
            break;
        }
        s += 0.5;
    }
    add(end, mul(d, last))
}

/// Split a chain into primitives, each with the skeleton pixels that produced it.
fn segment_chain(pts: &[P], closed: bool) -> Vec<(Primitive, Vec<P>)> {
    let mut out = Vec::new();
    if pts.len() < 2 {
        return out;
    }
    if closed {
        if let Some(arc) = try_arc(pts, true) {
            out.push((arc, pts.to_vec()));
            return out;
        }
        let mut v = pts.to_vec();
        v.push(pts[0]);
        split(&v, 0, v.len() - 1, &mut out);
    } else {
        split(pts, 0, pts.len() - 1, &mut out);
    }
    out
}

fn split(pts: &[P], lo: usize, hi: usize, out: &mut Vec<(Primitive, Vec<P>)>) {
    if hi <= lo {
        return;
    }
    let run = &pts[lo..=hi];
    let (dev, far) = farthest(run);
    if dev <= LINE_TOL || run.len() <= 3 {
        if let Some(line) = line_of(run) {
            out.push((line, run.to_vec()));
        }
        return;
    }
    if let Some(arc) = try_arc(run, false) {
        out.push((arc, run.to_vec()));
        return;
    }
    let mid = lo + far;
    if mid == lo || mid == hi {
        if let Some(line) = line_of(run) {
            out.push((line, run.to_vec()));
        }
        return;
    }
    split(pts, lo, mid, out);
    split(pts, mid, hi, out);
}

/// Largest distance of an interior point from the chord (Douglas–Peucker), with its index.
fn farthest(run: &[P]) -> (f64, usize) {
    let a = run[0];
    let b = run[run.len() - 1];
    let mut best = (0.0, 0);
    for (i, p) in run.iter().enumerate().take(run.len() - 1).skip(1) {
        let d = dist_point_segment(*p, a, b);
        if d > best.0 {
            best = (d, i);
        }
    }
    best
}

/// Line through the orthogonal fit of the run, cut at the projections of the run's end pixels.
fn line_of(run: &[P]) -> Option<Primitive> {
    let f = fit_line(run)?;
    let a = run[0];
    let b = run[run.len() - 1];
    let p = add(f.c, mul(f.d, dot(sub(a, f.c), f.d)));
    let q = add(f.c, mul(f.d, dot(sub(b, f.c), f.d)));
    if dist(p, q) < 0.5 {
        return None;
    }
    Some(Primitive::Line { p, q })
}

fn try_arc(run: &[P], closed: bool) -> Option<Primitive> {
    if run.len() < MIN_ARC_PIXELS {
        return None;
    }
    let cf = fit_circle(run)?;
    if !(cf.r >= MIN_RADIUS && cf.r <= MAX_RADIUS)
        || cf.rms > ARC_RMS_TOL
        || cf.max_dev > ARC_MAX_TOL
    {
        return None;
    }
    let angle = |p: &P| (p.1 - cf.c.1).atan2(p.0 - cf.c.0);
    let start = angle(&run[0]);
    let mut prev = start;
    let mut total = 0.0;
    let mut abs_total = 0.0;
    for p in &run[1..] {
        let a = angle(p);
        let d = wrap_pi(a - prev);
        total += d;
        abs_total += d.abs();
        prev = a;
    }
    if closed {
        let d = wrap_pi(start - prev);
        total += d;
        abs_total += d.abs();
    }
    // The pixels must progress monotonically around the centre (no zig-zag fitted by accident).
    if abs_total - total.abs() > 0.1 * abs_total + 0.05 {
        return None;
    }
    let sweep = if closed { TAU.copysign(total) } else { total };
    if !closed && sweep.abs() * cf.r < 1.0 {
        return None;
    }
    Some(Primitive::Arc {
        c: cf.c,
        r: cf.r,
        start_rad: start.rem_euclid(TAU),
        sweep_rad: sweep,
    })
}

/// Median stroke width over (at most 64 evenly spaced) skeleton pixels of the run.
fn stroke_width(ink: &Ink, run: &[P], prim: &Primitive) -> f64 {
    let n = run.len();
    if n == 0 {
        return 0.0;
    }
    let samples = n.min(WIDTH_SAMPLES);
    let mut ws = Vec::with_capacity(samples);
    for k in 0..samples {
        let i = if samples == n {
            k
        } else {
            k * (n - 1) / (samples - 1)
        };
        let p = run[i];
        let Some(nrm) = normal_at(prim, p) else {
            continue;
        };
        let probe = edge(ink, p, nrm, 1.0) + edge(ink, p, nrm, -1.0);
        let cap = 2.0 * background_distance(ink, p);
        ws.push(probe.min(cap));
    }
    median(&mut ws).unwrap_or(0.0)
}

fn normal_at(prim: &Primitive, p: P) -> Option<P> {
    match *prim {
        Primitive::Line { p: a, q: b } => unit(sub(b, a)).map(perp),
        Primitive::Arc { c, .. } => unit(sub(p, c)),
    }
}

/// Distance from the skeleton pixel centre to the ink edge along `sign · n`, sampled every 0.25 px
/// and interpolated halfway between the last dark and the first background sample.
fn edge(ink: &Ink, p: P, n: P, sign: f64) -> f64 {
    let step = 0.25;
    let limit = MAX_HALF_WIDTH as f64;
    let mut last_dark = 0.0;
    let mut s = step;
    while s <= limit {
        let x = (p.0 + sign * s * n.0).round() as i64;
        let y = (p.1 + sign * s * n.1).round() as i64;
        if ink.is_dark(x, y) {
            last_dark = s;
        } else {
            return 0.5 * (last_dark + s);
        }
        s += step;
    }
    limit
}

/// Euclidean distance from a pixel centre to the nearest background pixel centre (outside the image
/// counts as background), searched ring by ring up to `MAX_HALF_WIDTH`.
fn background_distance(ink: &Ink, p: P) -> f64 {
    let (cx, cy) = (p.0.round() as i64, p.1.round() as i64);
    let mut best = f64::INFINITY;
    for r in 1..=MAX_HALF_WIDTH {
        if r as f64 >= best {
            break;
        }
        let mut check = |dx: i64, dy: i64| {
            if !ink.is_dark(cx + dx, cy + dy) {
                let d = ((dx * dx + dy * dy) as f64).sqrt();
                if d < best {
                    best = d;
                }
            }
        };
        for dx in -r..=r {
            check(dx, -r);
            check(dx, r);
        }
        for dy in -r + 1..r {
            check(-r, dy);
            check(r, dy);
        }
    }
    if best.is_infinite() {
        MAX_HALF_WIDTH as f64
    } else {
        best
    }
}

/// Merge collinear line strokes.
///
/// Two line strokes are mergeable when their directions differ by at most `angle_tol_rad`, both
/// endpoints of the shorter lie within `offset_tol_px` of the longer one's line, and the gap between
/// their extents along that line is at most `gap_tol_px` (overlaps count as zero gap). Mergeability
/// is chained (union–find), so a dashed line merges into one stroke even though its first and last
/// dash are far apart. Each group becomes one `Line` along the longest member's direction spanning
/// all member extents, with the maximum width, the union pattern (`Unknown < Continuous < Dashed <
/// DashDot`), the length-weighted support (gaps count as unsupported, so a merged dashed line
/// honestly reports e.g. 0.8) and the summed pixel count. Arcs and non-mergeable lines pass through;
/// output order follows the first member of each group.
pub fn merge_collinear(
    strokes: Vec<Stroke>,
    angle_tol_rad: f64,
    offset_tol_px: f64,
    gap_tol_px: f64,
) -> Vec<Stroke> {
    let n = strokes.len();
    let lines: Vec<usize> = (0..n)
        .filter(|&i| line_direction(&strokes[i].primitive).is_some())
        .collect();
    let m = lines.len();
    let mut dsu = Dsu::new(m);
    for a in 0..m {
        for b in a + 1..m {
            if mergeable(
                &strokes[lines[a]].primitive,
                &strokes[lines[b]].primitive,
                angle_tol_rad,
                offset_tol_px,
                gap_tol_px,
            ) {
                dsu.union(a, b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut root_of = vec![0usize; m];
    for k in 0..m {
        let r = dsu.find(k);
        root_of[k] = r;
        groups.entry(r).or_default().push(lines[k]);
    }
    let mut pos = vec![usize::MAX; n];
    for (k, &i) in lines.iter().enumerate() {
        pos[i] = k;
    }
    let mut emitted = vec![false; m];
    let mut out = Vec::with_capacity(n);
    for (i, stroke) in strokes.iter().enumerate() {
        let k = pos[i];
        if k == usize::MAX {
            out.push(stroke.clone());
            continue;
        }
        let r = root_of[k];
        if emitted[r] {
            continue;
        }
        emitted[r] = true;
        let members = &groups[&r];
        if members.len() == 1 {
            out.push(stroke.clone());
        } else {
            out.push(merge_group(&strokes, members));
        }
    }
    out
}

fn mergeable(a: &Primitive, b: &Primitive, angle_tol: f64, offset_tol: f64, gap_tol: f64) -> bool {
    let (Primitive::Line { p: pa, q: qa }, Primitive::Line { p: pb, q: qb }) = (a, b) else {
        return false;
    };
    // Cheap bounding-box rejection.
    let reach = gap_tol.max(0.0) + offset_tol.max(0.0);
    if pa.0.max(qa.0) + reach < pb.0.min(qb.0)
        || pb.0.max(qb.0) + reach < pa.0.min(qa.0)
        || pa.1.max(qa.1) + reach < pb.1.min(qb.1)
        || pb.1.max(qb.1) + reach < pa.1.min(qa.1)
    {
        return false;
    }
    let (la, lb) = (dist(*pa, *qa), dist(*pb, *qb));
    let (rp, rq, sp, sq, lr) = if la >= lb {
        (*pa, *qa, *pb, *qb, la)
    } else {
        (*pb, *qb, *pa, *qa, lb)
    };
    let (Some(d), Some(ds)) = (unit(sub(rq, rp)), unit(sub(sq, sp))) else {
        return false;
    };
    if undirected_angle(d, ds) > angle_tol {
        return false;
    }
    let nrm = perp(d);
    if dot(sub(sp, rp), nrm).abs() > offset_tol || dot(sub(sq, rp), nrm).abs() > offset_tol {
        return false;
    }
    let (ta, tb) = (dot(sub(sp, rp), d), dot(sub(sq, rp), d));
    let gap = (ta.min(tb) - lr).max(-ta.max(tb));
    gap <= gap_tol
}

fn merge_group(strokes: &[Stroke], members: &[usize]) -> Stroke {
    let mut reference = members[0];
    for &i in &members[1..] {
        if strokes[i].primitive.length() > strokes[reference].primitive.length() {
            reference = i;
        }
    }
    let Primitive::Line { p: c, q } = strokes[reference].primitive else {
        return strokes[reference].clone();
    };
    let Some(d) = unit(sub(q, c)) else {
        return strokes[reference].clone();
    };
    let mut tmin = f64::INFINITY;
    let mut tmax = f64::NEG_INFINITY;
    let mut width: f64 = 0.0;
    let mut pattern = strokes[members[0]].pattern;
    let mut weighted = 0.0;
    let mut pixels = 0usize;
    for &i in members {
        let s = &strokes[i];
        if let Primitive::Line { p, q } = s.primitive {
            for e in [p, q] {
                let t = dot(sub(e, c), d);
                tmin = tmin.min(t);
                tmax = tmax.max(t);
            }
        }
        width = width.max(s.width_px);
        pattern = pattern.union(s.pattern);
        weighted += s.support * s.primitive.length();
        pixels += s.pixels;
    }
    let len = tmax - tmin;
    Stroke {
        primitive: Primitive::Line {
            p: add(c, mul(d, tmin)),
            q: add(c, mul(d, tmax)),
        },
        width_px: width,
        pattern,
        support: if len > 0.0 {
            (weighted / len).min(1.0)
        } else {
            0.0
        },
        pixels,
    }
}
