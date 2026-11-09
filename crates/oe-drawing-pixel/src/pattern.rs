//! Line-pattern inference: group collinear line pieces and co-circular arc pieces, then read the
//! dash-length and gap statistics of each regular run.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use crate::Ink;
use crate::geom::{P, add, dist, dot, median, mul, perp, sub, undirected_angle, unit, wrap_pi};
use crate::stroke::{Pattern, Primitive, Stroke, line_direction};

/// Collinearity: directions within 3° and endpoints within 1.5 px of the reference line.
const ANGLE_TOL: f64 = 0.052_36;
const OFFSET_TOL: f64 = 1.5;
/// Two pieces belong to the same candidate run when their gap is at most
/// `GAP_FACTOR × longer piece + GAP_FLOOR` (dash gaps are shorter than dashes in every standard).
const GAP_FACTOR: f64 = 1.5;
const GAP_FLOOR: f64 = 3.0;
/// Runs whose median gap is below this are touching fragments of one continuous stroke.
const TOUCH_GAP: f64 = 1.0;
/// The gaps of a dashed or dash-dot line are short: 3d in GB/T 4457.4 (about 1.5–2 mm for d of 0.5–0.7 mm). Pieces
/// further apart on paper than this are separate lines, never one run, however regular their spacing elsewhere on the
/// same row is.
pub(crate) const MAX_DASH_GAP_PAPER_MM: f64 = 6.0;
/// A dash of a 虚线 is 12d (about 6–8.4 mm); a longer piece is not a dash of a hidden line.
const MAX_DASH_PAPER_MM: f64 = 10.0;
/// The long dash of a 点画线 is 24d (about 12–17 mm; drawn up to some 30 mm); a longer piece is not one.
const MAX_LONG_DASH_PAPER_MM: f64 = 35.0;
/// A gap is a dash gap only where the original page is blank: at most this share of its samples dark. A crossing
/// stroke erased before vectorizing leaves an inked gap — the line runs on through it.
const BLANK_GAP_MAX_DARK: f64 = 0.2;

/// What the pattern inference reads besides the strokes: the page resolution, for the paper lengths of the line
/// types, and the original page ink, on which a gap between pieces must be blank to be a dash gap. Without the
/// original ink every gap counts as blank (strokes vectorized from the page itself).
#[derive(Clone, Copy)]
pub struct PatternContext<'a> {
    pub px_per_mm: f64,
    pub original: Option<&'a Ink>,
}

impl Default for PatternContext<'_> {
    fn default() -> Self {
        PatternContext { px_per_mm: 300.0 / 25.4, original: None }
    }
}

impl PatternContext<'_> {
    /// Whether the original ink is blank along the sample points of a gap.
    fn blank(&self, points: impl Iterator<Item = P>) -> bool {
        let Some(ink) = self.original else {
            return true;
        };
        let (mut n, mut dark) = (0usize, 0usize);
        for p in points {
            n += 1;
            if ink.has_dark_within(p.0, p.1, 1.0) {
                dark += 1;
            }
        }
        n == 0 || (dark as f64) <= BLANK_GAP_MAX_DARK * n as f64
    }
}

/// A dashed line's gaps are a fixed share of its dashes: GB/T 4457.4-2002 表 1 draws 虚线 as 画 12d、短间隔 3d
/// (AutoCAD HIDDEN and DASHED use 1:2). Pieces of similar length whose median gap is below this share of the
/// median piece are one continuous line cut where other strokes crossed it (erased centre, dimension and
/// extension lines), not dashes: on the I1 page the solid contour circles came out as "dashed" this way,
/// pieces of 9–27 mm between gaps of 1–2 mm.
const DASH_GAP_RATIO: f64 = 0.12;
/// The dot of a 细点画线 is ≤ 0.5d (画 24d、间隔 3d); plotted dots are at most about 1.5 mm. A "short piece"
/// longer than this is a cut dash, not a dot (the bound `refine::measure_pattern` uses as well).
const DOT_MAX_PX: f64 = 20.0;
/// Co-circularity tolerances (absolute px or relative to the radius, whichever is larger).
const CIRCLE_CENTRE_TOL: f64 = 2.0;
const CIRCLE_RADIUS_TOL: f64 = 1.5;
const CIRCLE_REL_TOL: f64 = 0.02;

/// Union–find with path halving (deterministic).
pub(crate) struct Dsu(Vec<usize>);

impl Dsu {
    pub fn new(n: usize) -> Dsu {
        Dsu((0..n).collect())
    }

    pub fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }

    pub fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Smaller root wins so the result does not depend on call order.
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.0[hi] = lo;
        }
    }
}

/// Assign patterns to all strokes: collinear line runs first, then co-circular runs (arcs plus the
/// short line pieces that dots of a dash-dot circle become).
pub(crate) fn assign(strokes: &mut [Stroke], ctx: &PatternContext) {
    assign_collinear(strokes, ctx);
    assign_cocircular(strokes, ctx);
}

/// Sample points every 2 px over the central half of a straight gap: the ink of a piece reaches past its thinned,
/// vectorized end by up to half its width, which is not the line continuing.
fn gap_points_line(a: P, b: P) -> Vec<P> {
    let l = dist(a, b);
    let Some(d) = unit(sub(b, a)) else { return Vec::new() };
    let clear = (0.25 * l).max(1.5);
    let (from, to) = (clear, l - clear);
    if to <= from {
        return Vec::new();
    }
    let n = ((to - from) / 2.0).ceil().max(1.0) as usize;
    (0..=n).map(|k| add(a, mul(d, from + (to - from) * k as f64 / n as f64))).collect()
}

fn assign_collinear(strokes: &mut [Stroke], ctx: &PatternContext) {
    let idx: Vec<usize> = (0..strokes.len())
        .filter(|&i| line_direction(&strokes[i].primitive).is_some())
        .collect();
    let n = idx.len();
    if n < 3 {
        return;
    }
    let mut dsu = Dsu::new(n);
    for a in 0..n {
        for b in a + 1..n {
            if collinear_pair(&strokes[idx[a]].primitive, &strokes[idx[b]].primitive) {
                dsu.union(a, b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &i) in idx.iter().enumerate() {
        groups.entry(dsu.find(k)).or_default().push(i);
    }
    for members in groups.values() {
        if members.len() < 3 {
            continue;
        }
        let reference = longest(strokes, members);
        let Primitive::Line { p: c, q } = strokes[reference].primitive else {
            continue;
        };
        let Some(d) = unit(sub(q, c)) else {
            continue;
        };
        let mut items: Vec<(f64, f64, usize)> = members
            .iter()
            .filter_map(|&i| match strokes[i].primitive {
                Primitive::Line { p, q } => {
                    let t0 = dot(sub(p, c), d);
                    let t1 = dot(sub(q, c), d);
                    Some((t0.min(t1), t0.max(t1), i))
                }
                Primitive::Arc { .. } => None,
            })
            .collect();
        items.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.2.cmp(&b.2))
        });
        let order: Vec<usize> = items.iter().map(|it| it.2).collect();
        let lengths: Vec<f64> = items.iter().map(|it| it.1 - it.0).collect();
        let gaps: Vec<f64> = items
            .windows(2)
            .map(|w| (w[1].0 - w[0].1).max(0.0))
            .collect();
        let at = |t: f64| add(c, mul(d, t));
        let blank: Vec<bool> = items
            .windows(2)
            .map(|w| ctx.blank(gap_points_line(at(w[0].1), at(w[1].0)).into_iter()))
            .collect();
        apply_runs(strokes, &order, &lengths, &gaps, &blank, ctx);
    }
}

fn longest(strokes: &[Stroke], members: &[usize]) -> usize {
    let mut best = members[0];
    for &i in &members[1..] {
        if strokes[i].primitive.length() > strokes[best].primitive.length() {
            best = i;
        }
    }
    best
}

/// Whether two line pieces are collinear and close enough along the line to be dashes of one run.
fn collinear_pair(a: &Primitive, b: &Primitive) -> bool {
    let (Primitive::Line { p: pa, q: qa }, Primitive::Line { p: pb, q: qb }) = (a, b) else {
        return false;
    };
    let (la, lb) = (dist(*pa, *qa), dist(*pb, *qb));
    let (rp, rq, sp, sq, lr) = if la >= lb {
        (*pa, *qa, *pb, *qb, la)
    } else {
        (*pb, *qb, *pa, *qa, lb)
    };
    let (Some(d), Some(ds)) = (unit(sub(rq, rp)), unit(sub(sq, sp))) else {
        return false;
    };
    if undirected_angle(d, ds) > ANGLE_TOL {
        return false;
    }
    let n = perp(d);
    if dot(sub(sp, rp), n).abs() > OFFSET_TOL || dot(sub(sq, rp), n).abs() > OFFSET_TOL {
        return false;
    }
    let (ta, tb) = (dot(sub(sp, rp), d), dot(sub(sq, rp), d));
    let (t0, t1) = (ta.min(tb), ta.max(tb));
    let gap = (t0 - lr).max(-t1);
    gap <= GAP_FACTOR * lr + GAP_FLOOR
}

/// Cut a sorted sequence of pieces into runs: first where a gap is longer than any dash gap on paper (the pieces
/// on either side are separate lines), then, within each such stretch, where a gap leaves a factor two of the
/// stretch's own median gap (1 px slack); classify every run of at least three pieces.
fn apply_runs(strokes: &mut [Stroke], order: &[usize], lengths: &[f64], gaps: &[f64], blank: &[bool], ctx: &PatternContext) {
    if order.is_empty() {
        return;
    }
    let max_gap = MAX_DASH_GAP_PAPER_MM * ctx.px_per_mm;
    let mut start = 0usize;
    for i in 0..=gaps.len() {
        if i < gaps.len() && gaps[i] <= max_gap {
            continue;
        }
        // The stretch start..=i: pieces joined by gaps no longer than a dash gap.
        let mut g = gaps[start..i].to_vec();
        let gm = median(&mut g).unwrap_or(0.0);
        let mut run_start = start;
        for j in start..=i {
            let cut = j == i || gaps[j] > 2.0 * gm + 1.0 || gaps[j] < 0.5 * gm - 1.0;
            if cut {
                classify_run(strokes, &order[run_start..=j], &lengths[run_start..=j], &gaps[run_start..j], &blank[run_start..j], ctx);
                run_start = j + 1;
            }
        }
        start = i + 1;
    }
}

fn classify_run(strokes: &mut [Stroke], members: &[usize], lengths: &[f64], gaps: &[f64], blank: &[bool], ctx: &PatternContext) {
    if members.len() < 3 {
        return;
    }
    let mut g = gaps.to_vec();
    let gm = median(&mut g).unwrap_or(0.0);
    if gm < TOUCH_GAP {
        return;
    }
    // Gaps the original page inks are crossings erased before vectorizing: a run that is not mostly blank between
    // its pieces is one line cut where others crossed it.
    if blank.iter().filter(|b| **b).count() * 4 < gaps.len() * 3 {
        return;
    }
    let pattern = dash_pattern(lengths, gm, ctx);
    if pattern == Pattern::Continuous {
        return;
    }
    for &i in members {
        strokes[i].pattern = pattern;
    }
}

/// Dash statistics of one regular run with median gap `gap`: similar lengths (interior pieces within a
/// factor two of the median, end pieces may be truncated) are `Dashed` when the gap is at least
/// `DASH_GAP_RATIO` of the median piece, else `Continuous` (a line cut by crossings); strictly alternating
/// long/short pieces whose short pieces are at most half the long ones are `DashDot` when the short ones
/// are dots (at most `DOT_MAX_PX`), else `Continuous`; anything else is `Unknown`.
fn dash_pattern(lengths: &[f64], gap: f64, ctx: &PatternContext) -> Pattern {
    let n = lengths.len();
    let mut sorted = lengths.to_vec();
    let lmed = median(&mut sorted).unwrap_or(0.0);
    let interior_ok = lengths[1..n - 1]
        .iter()
        .all(|&l| l >= 0.5 * lmed && l <= 2.0 * lmed);
    let ends_ok = lengths[0] <= 2.0 * lmed && lengths[n - 1] <= 2.0 * lmed;
    if interior_ok && ends_ok {
        return if gap >= DASH_GAP_RATIO * lmed && lmed <= MAX_DASH_PAPER_MM * ctx.px_per_mm {
            Pattern::Dashed
        } else {
            Pattern::Continuous
        };
    }
    let lmin = sorted[0];
    let lmax = sorted[n - 1];
    let thr = 0.5 * (lmin + lmax);
    let long: Vec<bool> = lengths.iter().map(|&l| l > thr).collect();
    let alternating = long.windows(2).all(|w| w[0] != w[1]);
    if alternating && long.iter().any(|&b| b) && long.iter().any(|&b| !b) {
        let mut longs: Vec<f64> = lengths
            .iter()
            .zip(&long)
            .filter(|&(_, &b)| b)
            .map(|(&l, _)| l)
            .collect();
        let mut shorts: Vec<f64> = lengths
            .iter()
            .zip(&long)
            .filter(|&(_, &b)| !b)
            .map(|(&l, _)| l)
            .collect();
        let (lm, sm) = (
            median(&mut longs).unwrap_or(0.0),
            median(&mut shorts).unwrap_or(0.0),
        );
        if sm <= 0.5 * lm {
            return if sm <= DOT_MAX_PX && lm <= MAX_LONG_DASH_PAPER_MM * ctx.px_per_mm {
                Pattern::DashDot
            } else {
                Pattern::Continuous
            };
        }
    }
    Pattern::Unknown
}

fn circle_of(s: &Stroke) -> (P, f64) {
    match s.primitive {
        Primitive::Arc { c, r, .. } => (c, r),
        Primitive::Line { .. } => ((0.0, 0.0), 0.0),
    }
}

fn same_circle(ca: P, ra: f64, cb: P, rb: f64) -> bool {
    let r = ra.max(rb);
    dist(ca, cb) <= CIRCLE_CENTRE_TOL.max(CIRCLE_REL_TOL * r)
        && (ra - rb).abs() <= CIRCLE_RADIUS_TOL.max(CIRCLE_REL_TOL * r)
}

fn assign_cocircular(strokes: &mut [Stroke], ctx: &PatternContext) {
    let arcs: Vec<usize> = (0..strokes.len())
        .filter(|&i| matches!(strokes[i].primitive, Primitive::Arc { .. }))
        .collect();
    if arcs.is_empty() {
        return;
    }
    let mut dsu = Dsu::new(arcs.len());
    for a in 0..arcs.len() {
        for b in a + 1..arcs.len() {
            let (ca, ra) = circle_of(&strokes[arcs[a]]);
            let (cb, rb) = circle_of(&strokes[arcs[b]]);
            if same_circle(ca, ra, cb, rb) {
                dsu.union(a, b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &i) in arcs.iter().enumerate() {
        groups.entry(dsu.find(k)).or_default().push(i);
    }
    let lines: Vec<usize> = (0..strokes.len())
        .filter(|&i| matches!(strokes[i].primitive, Primitive::Line { .. }))
        .collect();
    for members in groups.values() {
        let reference = longest(strokes, members);
        let (c, r) = circle_of(&strokes[reference]);
        if !r.is_finite() || r <= 0.0 {
            continue;
        }
        let on_circle = |p: P| (dist(p, c) - r).abs() <= OFFSET_TOL;
        // Angular intervals (a0, a1) with a1 > a0, plus the member index.
        let mut items: Vec<(f64, f64, usize)> = Vec::new();
        for &i in members {
            if let Primitive::Arc {
                start_rad,
                sweep_rad,
                ..
            } = strokes[i].primitive
            {
                let (a0, a1) = if sweep_rad >= 0.0 {
                    (start_rad, start_rad + sweep_rad)
                } else {
                    (start_rad + sweep_rad, start_rad)
                };
                items.push((a0.rem_euclid(TAU), a1 - a0, i));
            }
        }
        // Dots of a dash-dot circle come out as short chords lying on the circle.
        for &i in &lines {
            let Primitive::Line { p, q } = strokes[i].primitive else {
                continue;
            };
            let mid = ((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5);
            if dist(p, q) <= 0.5 * r && on_circle(p) && on_circle(q) && on_circle(mid) {
                let ap = (p.1 - c.1).atan2(p.0 - c.0);
                let aq = (q.1 - c.1).atan2(q.0 - c.0);
                let span = wrap_pi(aq - ap);
                let a0 = if span >= 0.0 { ap } else { aq };
                items.push((a0.rem_euclid(TAU), span.abs(), i));
            }
        }
        if items.len() < 3 {
            continue;
        }
        // items: (start angle in [0, 2π), angular length, index); sort around the circle.
        items.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.2.cmp(&b.2))
        });
        let m = items.len();
        // Cyclic gaps in px; the largest gap is where the linear sequence is cut.
        let cyc_gaps: Vec<f64> = (0..m)
            .map(|k| {
                let end = items[k].0 + items[k].1;
                let next = if k + 1 < m {
                    items[k + 1].0
                } else {
                    items[0].0 + TAU
                };
                ((next - end) * r).max(0.0)
            })
            .collect();
        let mut cut = 0usize;
        for k in 1..m {
            if cyc_gaps[k] > cyc_gaps[cut] {
                cut = k;
            }
        }
        let rotated: Vec<usize> = (0..m).map(|k| (cut + 1 + k) % m).collect();
        let order: Vec<usize> = rotated.iter().map(|&k| items[k].2).collect();
        let lengths: Vec<f64> = rotated.iter().map(|&k| items[k].1 * r).collect();
        let gaps: Vec<f64> = rotated[..m - 1].iter().map(|&k| cyc_gaps[k]).collect();
        // The gap after piece k runs along the circle from its end to the next piece's start.
        let blank: Vec<bool> = rotated[..m - 1]
            .iter()
            .map(|&k| {
                let from = items[k].0 + items[k].1;
                let span = cyc_gaps[k] / r;
                // The central half of the gap, as for straight gaps.
                let clear = (0.25 * span).max(1.5 / r);
                let (a, b) = (from + clear, from + span - clear);
                let n = (((b - a) * r) / 2.0).ceil().max(1.0) as usize;
                ctx.blank((0..=n).filter(|_| b > a).map(|j| {
                    let t = a + (b - a) * j as f64 / n as f64;
                    (c.0 + r * t.cos(), c.1 + r * t.sin())
                }))
            })
            .collect();
        apply_runs(strokes, &order, &lengths, &gaps, &blank, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dash_pattern_at(lengths: &[f64], gap: f64) -> Pattern {
        dash_pattern(lengths, gap, &PatternContext::default())
    }

    #[test]
    fn dash_statistics() {
        assert_eq!(dash_pattern_at(&[20.0, 20.0, 20.0, 20.0], 5.0), Pattern::Dashed);
        assert_eq!(
            dash_pattern_at(&[12.0, 20.0, 20.0, 20.0, 9.0], 5.0),
            Pattern::Dashed
        );
        assert_eq!(
            dash_pattern_at(&[30.0, 3.0, 30.0, 3.0, 30.0], 5.0),
            Pattern::DashDot
        );
        assert_eq!(
            dash_pattern_at(&[30.0, 3.0, 3.0, 30.0, 30.0], 5.0),
            Pattern::Unknown
        );
    }

    #[test]
    fn a_line_cut_by_crossings_is_not_dashed() {
        // The I1 contour circle (r = 291 px) between its erased centre and dimension lines: pieces of
        // 113–310 px with gaps of 13–21 px, which also alternate long/short.
        assert_eq!(
            dash_pattern_at(&[267.0, 113.0, 310.0, 167.0], 14.0),
            Pattern::Continuous
        );
        // A real hidden line of the 085 page: dashes of 78 px with gaps of about 20 px (12d : 3d).
        assert_eq!(dash_pattern_at(&[78.0, 78.0, 78.0, 78.0], 20.0), Pattern::Dashed);
        // Alternating pieces whose short ones are not dots are a cut line, not a centre line.
        assert_eq!(
            dash_pattern_at(&[200.0, 60.0, 200.0, 60.0, 200.0], 14.0),
            Pattern::Continuous
        );
        // Dots of a 细点画线: 24d long dashes, dots within the bound, gaps 3d.
        assert_eq!(
            dash_pattern_at(&[96.0, 6.0, 96.0, 6.0, 96.0], 12.0),
            Pattern::DashDot
        );
    }

    fn line(x0: f64, x1: f64, y: f64) -> Stroke {
        Stroke { primitive: Primitive::Line { p: (x0, y), q: (x1, y) }, width_px: 3.0, pattern: Pattern::Continuous, support: 1.0, pixels: (x1 - x0) as usize }
    }

    #[test]
    fn a_dash_run_is_found_beside_unrelated_pieces_on_the_same_row() {
        // The 085 hidden line: dashes of 78 px every 115 px; far along the same row, touching fragments of outline
        // (gaps of 2 px) that a row-wide median gap would let swamp the dash gaps.
        let mut strokes: Vec<Stroke> = (0..10).map(|k| line(100.0 + 22.0 * k as f64, 120.0 + 22.0 * k as f64, 50.0)).collect();
        strokes.extend((0..6).map(|k| line(2600.0 + 115.0 * k as f64, 2678.0 + 115.0 * k as f64, 50.0)));
        assign(&mut strokes, &PatternContext::default());
        assert!(strokes[10..].iter().all(|s| s.pattern == Pattern::Dashed), "{:?}", strokes[10..].iter().map(|s| s.pattern).collect::<Vec<_>>());
        assert!(strokes[..10].iter().all(|s| s.pattern != Pattern::Dashed));
    }

    #[test]
    fn gaps_the_original_page_inks_are_crossings_not_dash_gaps() {
        // A solid line drawn on the page, cut into pieces of 100 px with gaps of 15 px where crossings were erased.
        let (w, h) = (700usize, 60usize);
        let mut mask = vec![false; w * h];
        for y in 29..=31 {
            for x in 20..640 {
                mask[y * w + x] = true;
            }
        }
        let page = Ink::from_mask(w, h, mask);
        let cut = || (0..5).map(|k| line(20.0 + 115.0 * k as f64, 120.0 + 115.0 * k as f64, 30.0)).collect::<Vec<_>>();
        let mut strokes = cut();
        assign(&mut strokes, &PatternContext::default());
        assert!(strokes.iter().all(|s| s.pattern == Pattern::Dashed), "without the page every gap counts");
        let mut strokes = cut();
        assign(&mut strokes, &PatternContext { px_per_mm: 300.0 / 25.4, original: Some(&page) });
        assert!(strokes.iter().all(|s| s.pattern == Pattern::Continuous), "{:?}", strokes.iter().map(|s| s.pattern).collect::<Vec<_>>());
    }

    #[test]
    fn a_dash_is_at_most_a_paper_length() {
        // Pieces of 200 px (17 mm at 300 dpi) with gaps of 40 px are not the dashes of a hidden line; at 600 dpi the
        // same pixels are 8.5 mm dashes and are.
        let run = || (0..5).map(|k| line(240.0 * k as f64, 240.0 * k as f64 + 200.0, 30.0)).collect::<Vec<_>>();
        let mut strokes = run();
        assign(&mut strokes, &PatternContext::default());
        assert!(strokes.iter().all(|s| s.pattern != Pattern::Dashed));
        let mut strokes = run();
        assign(&mut strokes, &PatternContext { px_per_mm: 600.0 / 25.4, original: None });
        assert!(strokes.iter().all(|s| s.pattern == Pattern::Dashed));
    }
}
