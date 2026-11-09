//! Structure from the ink that no recognised member explains: the recognised members are erased from
//! a copy of the ink, the remainder is vectorised, strokes are classified by width and pattern, and
//! collinear contour pieces are grouped into structural lines (design §4: inferred structure is
//! structure; a gap explained by a text or a section region is bridged, an unexplained one is not).

use std::collections::BTreeMap;

use crate::geom::{
    P, add, clip_line_to_box, dist, dist_point_segment, dot, median, mul, perp, sub, undirected_angle, unit,
};
use crate::linetypes::bounds;
use crate::pattern::Dsu;
use crate::linetypes::steps;
use crate::refine::{InkRuns, LineFamily, measure_pattern_round3, measure_runs_texts, stroke_pattern};
use crate::stroke::{Pattern, Primitive, Stroke};
use crate::pattern::PatternContext;
use crate::vector::{merge_collinear_groups, vectorize_in, vectorize_in_rescued};
use crate::{Ink, Segment};

/// Thresholds of the structure step, in pixels at 300 dpi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructureParams {
    /// Strokes shorter than this are specks (3 mm).
    pub min_length_px: f64,
    /// A stroke at least this many times the thin reference width is thick (outline).
    pub thick_ratio: f64,
    /// Collinear merging: angle, offset and plain gap.
    pub merge_angle_rad: f64,
    pub merge_offset_px: f64,
    pub merge_gap_px: f64,
    /// Larger gaps are bridged only when a text box or section region explains them.
    pub explained_gap_px: f64,
    /// Gaps up to this are a touching joint, no span is recorded.
    pub touch_gap_px: f64,
    /// A dash-dot stroke this close to a proposed centre line (and parallel to it) is that line.
    pub centre_cover_px: f64,
    /// Page pixels per paper millimetre, for the paper lengths of the line types.
    pub px_per_mm: f64,
}

impl Default for StructureParams {
    fn default() -> Self {
        StructureParams {
            min_length_px: 3.0 * 300.0 / 25.4,
            thick_ratio: 1.6,
            merge_angle_rad: 0.052_36,
            merge_offset_px: 1.5,
            merge_gap_px: 30.0,
            explained_gap_px: 150.0,
            touch_gap_px: 8.0,
            centre_cover_px: 12.0,
            px_per_mm: 300.0 / 25.4,
        }
    }
}

impl StructureParams {
    /// The thresholds at a page resolution: every length scales with `dpi / 300`, the defaults being lengths at
    /// 300 dpi (3 mm of paper is 35 px there and 71 px at 600 dpi).
    pub fn at_dpi(dpi: f64) -> Self {
        let k = if dpi.is_finite() && dpi > 0.0 { dpi / 300.0 } else { 1.0 };
        let d = Self::default();
        StructureParams {
            min_length_px: d.min_length_px * k,
            merge_offset_px: d.merge_offset_px * k,
            merge_gap_px: d.merge_gap_px * k,
            explained_gap_px: d.explained_gap_px * k,
            touch_gap_px: d.touch_gap_px * k,
            centre_cover_px: d.centre_cover_px * k,
            px_per_mm: d.px_per_mm * k,
            ..d
        }
    }
}

/// Even–odd point-in-polygon test.
pub fn point_in_polygon(p: P, polygon: &[P]) -> bool {
    let n = polygon.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.1 > p.1) != (b.1 > p.1) {
            let x = a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1);
            if p.0 < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Clear every pixel whose centre lies inside one of the polygons (section regions).
pub fn erase_polygons(ink: &mut Ink, polygons: &[Vec<P>]) {
    for poly in polygons {
        if poly.len() < 3 || poly.iter().any(|p| !(p.0.is_finite() && p.1.is_finite())) {
            continue;
        }
        let y0 = (poly.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).ceil() as i64).max(0);
        let y1 = (poly.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).floor() as i64).min(ink.height as i64 - 1);
        for y in y0..=y1 {
            let yf = y as f64;
            let mut xs: Vec<f64> = Vec::new();
            let n = poly.len();
            for i in 0..n {
                let (a, b) = (poly[i], poly[(i + 1) % n]);
                if (a.1 > yf) != (b.1 > yf) {
                    xs.push(a.0 + (yf - a.1) * (b.0 - a.0) / (b.1 - a.1));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.chunks_exact(2) {
                let xa = (pair[0].ceil() as i64).max(0);
                let xb = (pair[1].floor() as i64).min(ink.width as i64 - 1);
                for x in xa..=xb {
                    ink.set_dark(x as usize, y as usize, false);
                }
            }
        }
    }
}

/// Whether the segment `p`–`q` passes through the box (inclusive), with the box grown by `margin`.
pub fn segment_hits_box(p: P, q: P, b: [f64; 4], margin: f64) -> bool {
    let d = sub(q, p);
    if dot(d, d) < 1e-18 {
        return p.0 >= b[0] - margin && p.0 <= b[2] + margin && p.1 >= b[1] - margin && p.1 <= b[3] + margin;
    }
    match clip_line_to_box(p, d, b[0] - margin, b[1] - margin, b[2] + margin, b[3] + margin) {
        Some((t0, t1)) => t1 >= 0.0 && t0 <= 1.0,
        None => false,
    }
}

fn segments_cross(a: P, b: P, c: P, d: P) -> bool {
    let orient = |p: P, q: P, r: P| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let (o1, o2, o3, o4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

/// Whether the segment enters a polygon: an endpoint inside or an edge crossed.
pub fn segment_hits_polygon(p: P, q: P, polygon: &[P]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    if point_in_polygon(p, polygon) || point_in_polygon(q, polygon) {
        return true;
    }
    let n = polygon.len();
    (0..n).any(|i| segments_cross(p, q, polygon[i], polygon[(i + 1) % n]))
}

/// Extra width given to an erased stroke: a fitted line a fraction of a degree off the pixel grid
/// leaves one-pixel slivers of a thin stroke otherwise.
pub const ERASE_WIDTH_MARGIN_PX: f64 = 1.6;

/// Arrowheads are erased as a stroke this long and this wide back from the tip.
const ARROW_ERASE_LENGTH_PX: f64 = 40.0;
const ARROW_ERASE_WIDTH_PX: f64 = 14.0;

/// The erased strokes, including those of the arrowheads, as `erase` receives them.
pub fn erased_strokes(
    segments: &[Segment],
    arrows: &[(P, P)],
) -> Vec<Segment> {
    let mut segs: Vec<Segment> = segments
        .iter()
        .map(|&(p, q, w)| (p, q, w + ERASE_WIDTH_MARGIN_PX))
        .collect();
    for &(tip, along) in arrows {
        if let Some(a) = unit(along) {
            segs.push((tip, sub(tip, mul(a, ARROW_ERASE_LENGTH_PX)), ARROW_ERASE_WIDTH_PX));
        } else {
            segs.push((tip, tip, 24.0));
        }
    }
    segs
}

/// The ink left after the recognised members are erased: strokes `(p, q, width)` with the erase
/// margin, boxes, polygons and arrowheads (erased as a short thick stroke back from the tip along
/// the arrow).
pub fn remaining_ink(
    ink: &Ink,
    segments: &[Segment],
    boxes: &[(f64, f64, f64, f64)],
    polygons: &[Vec<P>],
    arrows: &[(P, P)],
) -> Ink {
    let mut rest = ink.clone();
    crate::members::erase(&mut rest, &erased_strokes(segments, arrows), boxes);
    erase_polygons(&mut rest, polygons);
    rest
}

/// Whether a stroke is a remnant of an erased member: at least seven of nine samples along it lie
/// within the erased stroke's half-width plus 2 px of one of the erased strokes.
pub fn remnant_of_erased(s: &Stroke, erased: &[Segment]) -> bool {
    let mut covered = 0;
    for i in 0..9 {
        let pt = s.primitive.point_at(i as f64 / 8.0);
        if erased
            .iter()
            .any(|&(p, q, w)| dist_point_segment(pt, p, q) <= w * 0.5 + 2.0)
        {
            covered += 1;
        }
    }
    covered >= 7
}

/// Class of a vectorised stroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StrokeClass {
    VisibleContour,
    HiddenContour,
    CentreAxis,
    /// Thin continuous ink no member explains.
    ThinUnresolved,
    /// Dash-dot ink already covered by a proposed centre line.
    CoveredCentre,
}

/// What decided a stroke's line family or class (line-type knowledge).
#[derive(Clone, Debug, PartialEq)]
pub enum LineEvidence {
    /// Its own runs on the page ink: the family, the number of dashes and the median gap.
    Pattern { family: LineFamily, dashes: usize, gap_px: f64 },
    /// Two equal end dashes and one blank gap matching the page's dashed calibration (two pieces merged into one).
    TwoDash { ends_px: (f64, f64), gap_px: f64 },
    /// Continuous ink shorter than one dashed repeat, joined to a hidden line confirmed by its pattern or by the
    /// closure (index `to` in the returned strokes): end to end at a corner (hidden edges join into polylines), as its
    /// collinear continuation across an erased text, or across a pair of parallel hidden lines of one feature (the
    /// termination line of a hidden thread).
    Closure { kind: &'static str, to: usize, length_px: f64, repeat_px: f64 },
}

/// A classified stroke of the remainder.
#[derive(Clone, Debug, PartialEq)]
pub struct Classified {
    pub stroke: Stroke,
    pub class: StrokeClass,
    pub thick: bool,
    pub evidence: Option<LineEvidence>,
}

/// One family's elements on the page, from its reliable runs (three or more dashes, gaps blank on the original page).
#[derive(Clone, Debug, PartialEq)]
pub struct FamilyCalibration {
    /// `known`, `absent` (no run of the family on the page) or `conflict` (fewer than half of the runs agree on the gap).
    pub state: &'static str,
    /// Dashed: [gap, interior dash]; chain and double chain: [long dash, short element, gap] — page pixels.
    pub elements_px: Vec<f64>,
    pub runs: usize,
    pub consistent_runs: usize,
    /// The gaps the runs agree on, with the number of runs each: one mode per linetype scale on the page; the first
    /// is the page's.
    pub modes: Vec<(f64, usize)>,
}

impl FamilyCalibration {
    /// Whether a run's gap matches a mode of the page backed by at least two runs (or the page's mode).
    fn matches(&self, gap: f64, tol: f64) -> bool {
        self.modes.iter().enumerate().any(|(k, &(g, n))| (k == 0 || n >= 2) && (gap - g).abs() <= tol * g)
    }
}

/// The page's line patterns: one calibration per family, and what could not be judged without one.
#[derive(Clone, Debug, PartialEq)]
pub struct LinePatterns {
    pub dashed: FamilyCalibration,
    pub chain: FamilyCalibration,
    pub double_chain: FamilyCalibration,
    /// Pairs of equal collinear pieces with one blank gap left undecided because the dashed family is not known.
    pub two_dash_unjudged: usize,
    pub two_dash_merged: usize,
    pub closure_accepted: usize,
    /// Continuous pieces joined to a hidden line but forming a corner with a long continuous line (kept as they are).
    pub closure_blocked: usize,
    /// Patterned runs whose gap disagrees with their family's page calibration (their pattern dropped).
    pub inconsistent_runs: usize,
}

impl LinePatterns {
    /// The page's dashed gap and interior dash, when the family is known.
    pub fn dashed_elements(&self) -> Option<(f64, f64)> {
        (self.dashed.state == "known").then(|| (self.dashed.elements_px[0], self.dashed.elements_px[1]))
    }
}

/// Calibrate one family from the runs of its strokes: per run the gap median (and the interior dash, or the long and
/// short medians). The elements of one line type are equal across one drawing (textbook p17); a CAD drawing may hold
/// several linetype scales, each repeated: the runs' gaps are grouped into modes within the knowledge's tolerance, the
/// page's values are those of the mode most runs agree on, and the family is in conflict when fewer than half do.
fn calibrate(runs: &[&InkRuns], family: LineFamily, px_per_mm: f64) -> FamilyCalibration {
    let tol = bounds().page_gap_tolerance;
    let mut per_run: Vec<Vec<f64>> = Vec::new();
    for r in runs.iter().filter(|r| r.family == family && r.ons.len() >= 3) {
        let mut gaps = r.offs.clone();
        let g = median(&mut gaps).unwrap_or(0.0);
        let seen = r.ons.clone();
        let interior: Vec<f64> = if seen.len() >= 3 { seen[1..seen.len() - 1].to_vec() } else { seen.clone() };
        match family {
            LineFamily::Dashed => {
                let mut d = if interior.is_empty() { seen.clone() } else { interior };
                per_run.push(vec![g, median(&mut d).unwrap_or(0.0)]);
            }
            _ => {
                let lo = seen.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = seen.iter().copied().fold(0.0, f64::max);
                let b = bounds();
                let mut long: Vec<f64> = interior.iter().copied().filter(|&l| l >= b.element_long_share * hi).collect();
                let mut short: Vec<f64> =
                    seen.iter().copied().filter(|&l| l <= lo * b.element_short_spread + b.element_spread_slack_paper_mm * px_per_mm).collect();
                if long.is_empty() {
                    long = seen.iter().copied().filter(|&l| l >= b.element_long_share * hi).collect();
                }
                per_run.push(vec![median(&mut long).unwrap_or(0.0), median(&mut short).unwrap_or(0.0), g]);
            }
        }
    }
    if per_run.is_empty() {
        return FamilyCalibration { state: "absent", elements_px: Vec::new(), runs: 0, consistent_runs: 0, modes: Vec::new() };
    }
    let gap_at = if family == LineFamily::Dashed { 0 } else { 2 };
    // Modes: runs sorted by gap, a new mode where a gap leaves the tolerance of the current mode's first gap.
    let mut order: Vec<&Vec<f64>> = per_run.iter().collect();
    order.sort_by(|a, b| a[gap_at].total_cmp(&b[gap_at]));
    let mut groups: Vec<Vec<&Vec<f64>>> = Vec::new();
    for v in order {
        match groups.last_mut() {
            Some(g) if v[gap_at] <= g[0][gap_at] * (1.0 + 2.0 * tol) => g.push(v),
            _ => groups.push(vec![v]),
        }
    }
    groups.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0][gap_at].total_cmp(&b[0][gap_at])));
    let modes: Vec<(f64, usize)> = groups
        .iter()
        .map(|g| {
            let mut xs: Vec<f64> = g.iter().map(|v| v[gap_at]).collect();
            (median(&mut xs).unwrap_or(0.0), g.len())
        })
        .collect();
    let page_gap = modes[0].0;
    let agreeing: Vec<&Vec<f64>> = per_run.iter().filter(|v| (v[gap_at] - page_gap).abs() <= tol * page_gap).collect();
    let elements_px: Vec<f64> = (0..per_run[0].len())
        .map(|k| {
            let mut xs: Vec<f64> = agreeing.iter().map(|v| v[k]).collect();
            median(&mut xs).unwrap_or(0.0)
        })
        .collect();
    let state = if agreeing.len() * 2 >= per_run.len() { "known" } else { "conflict" };
    FamilyCalibration { state, elements_px, runs: per_run.len(), consistent_runs: agreeing.len(), modes }
}

/// Two line pieces on one line (directions within `angle`, the second's ends within `offset` of the first's line) and
/// apart: the gap between them, the first's and the second's inner ends and outer ends.
struct OnOneLine {
    gap: f64,
    inner: (P, P),
    outer: (P, P),
}

fn on_one_line(a: (P, P), b: (P, P), angle: f64, offset: f64) -> Option<OnOneLine> {
    let (d, e) = (unit(sub(a.1, a.0))?, unit(sub(b.1, b.0))?);
    if undirected_angle(d, e) > angle {
        return None;
    }
    let n = perp(d);
    if dot(sub(b.0, a.0), n).abs() > offset || dot(sub(b.1, a.0), n).abs() > offset {
        return None;
    }
    let la = dist(a.0, a.1);
    let (t0, t1) = (dot(sub(b.0, a.0), d), dot(sub(b.1, a.0), d));
    let (near, far, tn, tf) = if t0 <= t1 { (b.0, b.1, t0, t1) } else { (b.1, b.0, t1, t0) };
    if tn >= la {
        Some(OnOneLine { gap: tn - la, inner: (a.1, near), outer: (a.0, far) })
    } else if tf <= 0.0 {
        Some(OnOneLine { gap: -tf, inner: (a.0, far), outer: (a.1, near) })
    } else {
        None
    }
}

/// The line pieces of the strokes (arcs give none).
fn segment(s: &Stroke) -> Option<(P, P)> {
    match s.primitive {
        Primitive::Line { p, q } => Some((p, q)),
        Primitive::Arc { .. } => None,
    }
}

/// One stroke from line pieces on one line: the segment from `a` to `b`, widths and supports weighted by length.
fn joined(pieces: &[&Stroke], a: P, b: P, pattern: Pattern) -> Stroke {
    let total: f64 = pieces.iter().map(|s| s.primitive.length()).sum::<f64>().max(1e-9);
    Stroke {
        primitive: Primitive::Line { p: a, q: b },
        width_px: pieces.iter().map(|s| s.width_px * s.primitive.length()).sum::<f64>() / total,
        pattern,
        support: pieces.iter().map(|s| s.support * s.primitive.length()).sum::<f64>() / total,
        pixels: pieces.iter().map(|s| s.pixels).sum(),
    }
}

/// Collinear chains of the line pieces the vectorizer left without a pattern (continuous or undetermined), with gaps no
/// longer than a dash gap on paper, measured again along their extent on the page ink — the ink is the evidence, the
/// pieces only where to look: dashes cut by crossings, by erased members or by the minimum length are read from the ink
/// between them. The longest sub-chains whose ink shows a pattern of three or more elements become one stroke of that
/// family; two pieces whose ink shows exactly two dashes and one gap are returned as two-dash candidates (their indices
/// in the rebuilt list), judged later against the page's calibration.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn remeasure_chains(
    all: Vec<Stroke>,
    runs: Vec<Option<InkRuns>>,
    evidence: Vec<Option<LineEvidence>>,
    rescued: Vec<bool>,
    page: &Ink,
    text_boxes: &[[f64; 4]],
    in_view: &dyn Fn(P) -> bool,
    params: &StructureParams,
) -> (Vec<Stroke>, Vec<Option<InkRuns>>, Vec<Option<LineEvidence>>, Vec<bool>, Vec<(usize, usize)>) {
    // Pieces as far apart as a lost dash between two gaps still chain: the ink between them is measured.
    let b = bounds();
    let max_gap = (2.0 * b.max_dash_gap_paper_mm + b.dashed_max_dash_paper_mm) * params.px_per_mm;
    let b_edge = b.closure_join_paper_mm * params.px_per_mm;
    let open: Vec<usize> = (0..all.len())
        .filter(|&i| {
            segment(&all[i]).is_some()
                && in_view(all[i].primitive.mid())
                && runs[i].as_ref().is_some_and(|r| matches!(r.family, LineFamily::Continuous | LineFamily::Undetermined))
        })
        .collect();
    // Merged strokes replace their first piece; the others are dropped. Two-dash candidates by original index.
    let mut replace: BTreeMap<usize, (Stroke, InkRuns, LineEvidence)> = BTreeMap::new();
    let mut drop: Vec<bool> = vec![false; all.len()];
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    // First the chains, of the pieces the vectorizer kept (rescued pieces are candidates of the two-dash rule and the
    // closure only, never joined into chains).
    let kept: Vec<usize> = open.iter().copied().filter(|&i| !rescued[i]).collect();
    for (c, d, stretches) in line_stretches(&all, &kept, max_gap, params) {
        let at = |t: f64| add(c, mul(d, t));
        for st in stretches.iter().filter(|st| st.len() >= 2) {
            let k = st.len();
            let mut start = 0usize;
            while start + 1 < k {
                let mut advanced = false;
                for end in (start + 1..k).rev() {
                    let (a, b) = (at(st[start].0), at(st[start..=end].iter().map(|x| x.1).fold(f64::MIN, f64::max)));
                    let r = measure_runs_texts(page, a, b, params.px_per_mm, text_boxes);
                    // (Chains of chain-line pieces serve scope item 3, of dashed pieces item 4.)
                    let on = match r.family {
                        LineFamily::Dashed => steps().two_dash,
                        LineFamily::Chain | LineFamily::DoubleChain => steps().chain_short_dash,
                        _ => false,
                    };
                    if on && !gap_edge_walled(page, a, b, b_edge) {
                        let pieces: Vec<&Stroke> = st[start..=end].iter().map(|x| &all[x.2]).collect();
                        let mut g = r.offs.clone();
                        let ev = LineEvidence::Pattern { family: r.family, dashes: r.ons.len(), gap_px: median(&mut g).unwrap_or(0.0) };
                        replace.insert(st[start].2, (joined(&pieces, a, b, r.pattern), r, ev));
                        for x in &st[start + 1..=end] {
                            drop[x.2] = true;
                        }
                        start = end + 1;
                        advanced = true;
                        break;
                    }
                }
                if !advanced {
                    start += 1;
                }
            }
        }
    }
    // Then every adjacent pair of the pieces left (rescued ones included) whose ink shows exactly two dashes and one gap
    // is a two-dash candidate; the judgment against the page's calibration chooses among overlapping ones.
    let left: Vec<usize> = open.iter().copied().filter(|&i| !drop[i] && !replace.contains_key(&i)).collect();
    for (c, d, stretches) in line_stretches(&all, &left, max_gap, params) {
        let at = |t: f64| add(c, mul(d, t));
        for st in stretches.iter().filter(|st| st.len() >= 2) {
            for w in st.windows(2) {
                let r = measure_runs_texts(page, at(w[0].0), at(w[1].1), params.px_per_mm, text_boxes);
                if r.ons.len() == 2 && r.offs.len() == 1 {
                    candidates.push((w[0].2, w[1].2));
                }
            }
        }
    }
    let mut index: Vec<Option<usize>> = vec![None; all.len()];
    let (mut a2, mut r2, mut e2, mut m2) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, (((s, r), e), m)) in all.into_iter().zip(runs).zip(evidence).zip(rescued).enumerate() {
        if drop[i] {
            continue;
        }
        index[i] = Some(a2.len());
        match replace.remove(&i) {
            Some((stroke, run, ev)) => {
                a2.push(stroke);
                r2.push(Some(run));
                e2.push(Some(ev));
                m2.push(false);
            }
            None => {
                a2.push(s);
                r2.push(r);
                e2.push(e);
                m2.push(m);
            }
        }
    }
    let candidates = candidates.into_iter().filter_map(|(i, j)| Some((index[i]?, index[j]?))).collect();
    (a2, r2, e2, m2, candidates)
}

/// The two-dash candidates that are one dashed line of two end dashes and one gap (GB/T 14665-2012 4.2.2: the minimum
/// dashed line; CAD makes both end dashes alike): the ink along the pair shows two dashes equal within the knowledge's
/// tolerance, each between its shares of the page's interior dash, and a gap within the tolerance of the page's gap; no
/// other collinear piece within a dash gap beyond either outer end (a longer run is judged as a run; an outline broken by
/// grooves continues); no other line ending at the gap or in it. Without the page's dashed elements the candidates are
/// only counted. Returns the accepted pairs with their measured ends and gap.
fn two_dash_pairs(all: &[Stroke], candidates: &[(usize, usize)], page: &Ink, text_boxes: &[[f64; 4]], elements: Option<(f64, f64)>, params: &StructureParams) -> Vec<(usize, usize, (f64, f64), f64)> {
    let b = bounds();
    let edge = b.closure_join_paper_mm * params.px_per_mm;
    let Some((g, d)) = elements else { return Vec::new() };
    let mut out = Vec::new();
    for &(i, j) in candidates {
        let (si, sj) = (segment(&all[i]).expect("a line"), segment(&all[j]).expect("a line"));
        let Some(rel) = on_one_line(si, sj, params.merge_angle_rad, params.merge_offset_px + 0.5) else { continue };
        let r = measure_runs_texts(page, rel.outer.0, rel.outer.1, params.px_per_mm, text_boxes);
        if r.ons.len() != 2 || r.offs.len() != 1 {
            continue;
        }
        let (la, lb, gap) = (r.ons[0], r.ons[1], r.offs[0]);
        if (la - lb).abs() > b.two_dash_end_equal_tolerance * la.max(lb) + 2.0
            || (gap - g).abs() > b.page_gap_tolerance * g
            || la.min(lb) < b.two_dash_min_end_to_dash * d
            || la.max(lb) > b.two_dash_max_end_to_dash * d
        {
            continue;
        }
        // No other piece on the line within a dash gap beyond the outer ends.
        let reach = g * (1.0 + b.page_gap_tolerance) + 3.0;
        let continues = (0..all.len()).any(|k| {
            k != i && k != j && segment(&all[k]).is_some_and(|sk| {
                [(si, rel.outer.0), (sj, rel.outer.1)].iter().any(|&(own, outer)| {
                    on_one_line(own, sk, params.merge_angle_rad, params.merge_offset_px + 1.0)
                        .is_some_and(|rr| rr.gap <= reach && dist(rr.inner.0, outer) < 1.0)
                })
            })
        });
        if continues {
            continue;
        }
        // Lines ending at the measured gap or in it bound it (an outline broken by grooves); the gap is where the ink
        // shows it, not between the kept pieces (a dash fragment may have been dropped), and the walls are read on the ink
        // (a short wall is no stroke the minimum length keeps).
        let bounded = unit(sub(rel.outer.1, rel.outer.0)).is_some_and(|da| {
            crate::pattern::line_ends_in_gap(all, &|k| k == i || k == j, rel.outer.0, da, la, la + gap, edge, false)
                || [la, la + gap].iter().any(|&t| wall_at(page, add(rel.outer.0, mul(da, t)), da, edge))
        });
        if bounded {
            continue;
        }
        out.push((i, j, (la, lb), gap));
    }
    out
}

/// Where a candidate piece joins a node piece as one hidden outline: `corner` (their ends within the join — hidden edges
/// join end to end into polylines, DC-G1-001) or `text_break` (collinear across an erased text box). A line ending on a
/// hidden line or crossing it is no evidence: any line meets a dashed line on its dashes (DC-G1-061).
fn junction(c: (P, P), n: (P, P), text_boxes: &[[f64; 4]], join: f64, params: &StructureParams) -> Option<&'static str> {
    let ce = [c.0, c.1];
    let ne = [n.0, n.1];
    if ce.iter().any(|&a| ne.iter().any(|&b| dist(a, b) <= join)) {
        return Some("corner");
    }
    if let Some(rel) = on_one_line(c, n, params.merge_angle_rad, params.merge_offset_px + 0.5)
        && rel.gap <= params.explained_gap_px
        && text_boxes.iter().any(|bx| segment_hits_box(rel.inner.0, rel.inner.1, *bx, 3.0))
    {
        return Some("text_break");
    }
    None
}

/// The crossing point of two segments that properly cross.
fn crossing_point(a: (P, P), b: (P, P)) -> Option<P> {
    if !segments_cross(a.0, a.1, b.0, b.1) {
        return None;
    }
    let r = sub(a.1, a.0);
    let s = sub(b.1, b.0);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-12 {
        return None;
    }
    let t = ((b.0.0 - a.0.0) * s.1 - (b.0.1 - a.0.1) * s.0) / den;
    Some(add(a.0, mul(r, t)))
}

/// The pieces `ids` grouped by line (collinear, gaps up to `max_gap`), each group with its reference line (origin and
/// direction of its longest piece) and its stretches: the pieces sorted along the line, cut where a gap exceeds `max_gap`,
/// as (start, end, index) in the line's coordinate.
#[allow(clippy::type_complexity)]
fn line_stretches(all: &[Stroke], ids: &[usize], max_gap: f64, params: &StructureParams) -> Vec<(P, P, Vec<Vec<(f64, f64, usize)>>)> {
    let mut dsu = Dsu::new(ids.len());
    for a in 0..ids.len() {
        for b in a + 1..ids.len() {
            let (sa, sb) = (segment(&all[ids[a]]).expect("a line"), segment(&all[ids[b]]).expect("a line"));
            if on_one_line(sa, sb, params.merge_angle_rad, params.merge_offset_px + 0.5).is_some_and(|r| r.gap <= max_gap) {
                dsu.union(a, b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &i) in ids.iter().enumerate() {
        groups.entry(dsu.find(k)).or_default().push(i);
    }
    let mut out = Vec::new();
    for members in groups.values().filter(|m| m.len() >= 2) {
        let reference = *members.iter().max_by(|&&a, &&b| all[a].primitive.length().total_cmp(&all[b].primitive.length())).expect("members");
        let (c, q) = segment(&all[reference]).expect("a line");
        let Some(d) = unit(sub(q, c)) else { continue };
        let mut items: Vec<(f64, f64, usize)> = members
            .iter()
            .map(|&i| {
                let (p0, p1) = segment(&all[i]).expect("a line");
                let (t0, t1) = (dot(sub(p0, c), d), dot(sub(p1, c), d));
                (t0.min(t1), t0.max(t1), i)
            })
            .collect();
        items.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.2.cmp(&y.2)));
        let mut stretches: Vec<Vec<(f64, f64, usize)>> = vec![vec![items[0]]];
        for it in &items[1..] {
            let last = stretches.last().expect("a stretch").last().expect("an item");
            if it.0 - last.1 > max_gap {
                stretches.push(vec![*it]);
            } else {
                stretches.last_mut().expect("a stretch").push(*it);
            }
        }
        out.push((c, d, stretches));
    }
    out
}

/// Whether one of the interior gaps of the ink along `a`–`b` lies between two walls on one side: lines ending square to
/// the line at both edges of the gap (a groove's walls, the strokes of two letters). A line crossing a gap, or one line
/// ending in it, is a CAD intersection falling in a gap (DC-G1-061) and leaves it a dash gap; walls at both edges make it
/// no dash gap — an outline broken by grooves, the tops of a row of letters.
fn gap_edge_walled(page: &Ink, a: P, b: P, reach: f64) -> bool {
    let Some(d) = unit(sub(b, a)) else { return false };
    let len = dist(a, b);
    let n = len.ceil() as usize + 1;
    let step = len / (n - 1) as f64;
    let on: Vec<bool> = (0..n)
        .map(|i| {
            let x = add(a, mul(d, i as f64 * step));
            page.has_dark_within(x.0, x.1, 1.5)
        })
        .collect();
    let (Some(first), Some(last)) = (on.iter().position(|&o| o), on.iter().rposition(|&o| o)) else { return false };
    let mut k = first;
    while k < last {
        if on[k] {
            k += 1;
            continue;
        }
        let start = k;
        while k < last && !on[k] {
            k += 1;
        }
        if k - start >= 3 {
            let (t0, t1) = ((start - 1) as f64 * step, k as f64 * step);
            let side = wall_side(page, add(a, mul(d, t0)), d, reach);
            if side.is_some() && side == wall_side(page, add(a, mul(d, t1)), d, reach) {
                return true;
            }
        }
    }
    false
}

/// Whether ink leaves the point `e` of a line with direction `d` square to it on one side only, for at least `reach`:
/// a wall ending at the line there (a groove's), not a line crossing it.
fn wall_at(page: &Ink, e: P, d: P, reach: f64) -> bool {
    wall_side(page, e, d, reach).is_some()
}

/// The side (+1 or -1 along the normal of `d`) on which a wall leaves the point `e` (see [`wall_at`]).
fn wall_side(page: &Ink, e: P, d: P, reach: f64) -> Option<i8> {
    let n = perp(d);
    let side = |sgn: f64| {
        let steps = reach.ceil() as usize;
        (3..=steps).all(|k| {
            let x = add(e, mul(n, sgn * k as f64));
            page.has_dark_within(x.0, x.1, 1.0)
        })
    };
    match (side(1.0), side(-1.0)) {
        (true, false) => Some(1),
        (false, true) => Some(-1),
        _ => None,
    }
}

/// The ends of the line of `p`–`q` as drawn: from the segment outwards both ways along its line until a blank stretch
/// longer than two pixels, at most `limit` beyond each end (the vectorizer stops pieces short of a junction).
fn drawn_ends(page: &Ink, p: P, q: P, limit: f64) -> (P, P) {
    let Some(d) = unit(sub(q, p)) else { return (p, q) };
    let walk = |start: P, dir: P| -> P {
        let (mut last, mut blank) = (0usize, 0usize);
        let mut k = 1usize;
        while (k as f64) <= limit {
            let x = add(start, mul(dir, k as f64));
            if page.has_dark_within(x.0, x.1, 1.0) {
                last = k;
                blank = 0;
            } else {
                blank += 1;
                if blank > 2 {
                    break;
                }
            }
            k += 1;
        }
        add(start, mul(dir, last as f64))
    };
    (walk(p, mul(d, -1.0)), walk(q, d))
}

/// Length of the ink along the line of `p`–`q` on the page, from the segment outwards both ways until a blank stretch
/// longer than two pixels (crossings and joints do not stop it), up to `limit` in all: how long the line is as drawn.
fn solid_extent(page: &Ink, p: P, q: P, limit: f64) -> f64 {
    let Some(d) = unit(sub(q, p)) else { return 0.0 };
    let mut total = dist(p, q);
    for (start, dir) in [(q, d), (p, mul(d, -1.0))] {
        let mut blank = 0usize;
        let mut k = 1usize;
        while total < limit {
            let x = add(start, mul(dir, k as f64));
            if page.has_dark_within(x.0, x.1, 1.0) {
                total += (blank + 1) as f64;
                blank = 0;
            } else {
                blank += 1;
                if blank > 2 {
                    break;
                }
            }
            k += 1;
        }
    }
    total
}

/// The closure of hidden outlines (line-type knowledge: a CAD segment shorter than one pattern repeat is drawn
/// continuous; hidden edges join end to end into polylines): continuous thin line pieces shorter than one repeat of the
/// page's dashed calibration, joined end to end (or across an erased text) to a hidden line — confirmed by its pattern,
/// or by this closure — and forming no corner with a long continuous line (a visible outline's corner). Returns the
/// accepted pieces with their evidence and the number blocked.
#[allow(clippy::too_many_arguments)]
fn hidden_closure(
    all: &[Stroke],
    runs: &[Option<InkRuns>],
    thick: &[bool],
    page: &Ink,
    text_boxes: &[[f64; 4]],
    (gap, dash): (f64, f64),
    in_view: &dyn Fn(P) -> bool,
    params: &StructureParams,
) -> (Vec<(usize, LineEvidence)>, usize) {
    let b = bounds();
    let repeat = (gap + dash) * (1.0 + b.closure_repeat_slack);
    let join = b.closure_join_paper_mm * params.px_per_mm;
    let family = |i: usize| runs[i].as_ref().map(|r| r.family);
    let mut nodes: Vec<usize> = (0..all.len()).filter(|&i| all[i].pattern == Pattern::Dashed && segment(&all[i]).is_some()).collect();
    // A candidate is short as drawn: the knowledge says a segment shorter than one repeat is drawn continuous, so a piece
    // whose ink runs on along its line (through crossings) past one repeat is part of a long continuous line.
    let candidates: Vec<usize> = (0..all.len())
        .filter(|&i| {
            segment(&all[i]).is_some_and(|(p, q)| solid_extent(page, p, q, repeat) < repeat)
                && in_view(all[i].primitive.mid())
                && family(i) == Some(LineFamily::Continuous)
                && !thick[i]
                && all[i].primitive.length() < repeat
        })
        .collect();
    let blockers: Vec<usize> = (0..all.len())
        .filter(|&i| all[i].pattern == Pattern::Continuous && all[i].primitive.length() >= repeat)
        .collect();
    let blocked = |c: usize, drawn: &[Option<(P, P)>]| {
        let ends = drawn[c].map_or([all[c].primitive.start(), all[c].primitive.end()], |(p, q)| [p, q]);
        blockers.iter().any(|&k| {
            k != c && [all[k].primitive.start(), all[k].primitive.end()].iter().any(|&e| ends.iter().any(|&a| dist(a, e) <= join))
        })
    };
    // Junctions are read between the lines as drawn (pieces extended along their own lines to where the ink ends).
    let drawn: Vec<Option<(P, P)>> = all.iter().map(|s| segment(s).map(|(p, q)| drawn_ends(page, p, q, 2.0 * join))).collect();
    // A piece crossing two parallel hidden lines on their ink, about square to them and ending close beyond them, is a
    // line of that hidden feature (GB/T 4459.1: every line of an invisible thread is dashed, its termination line across
    // the thread's lines; a single crossing is no evidence).
    let crosses_pair = |c: (P, P), nodes: &[usize]| -> Option<usize> {
        let dc = unit(sub(c.1, c.0))?;
        let crossed: Vec<(usize, P)> = nodes
            .iter()
            .filter_map(|&nd| {
                let sn = drawn[nd]?;
                let x = crossing_point(c, sn)?;
                let dn = unit(sub(sn.1, sn.0))?;
                (page.has_dark_within(x.0, x.1, 1.5) && undirected_angle(dc, dn) >= 1.05).then_some((nd, x))
            })
            .collect();
        for (a, &(na, xa)) in crossed.iter().enumerate() {
            for &(nb, xb) in &crossed[a + 1..] {
                let (sa, sb) = (drawn[na]?, drawn[nb]?);
                let (da, db) = (unit(sub(sa.1, sa.0))?, unit(sub(sb.1, sb.0))?);
                let spacing = dist(xa, xb);
                let beyond = |e: P| dist_point_segment(e, xa, xb);
                if undirected_angle(da, db) <= 0.052_36 && spacing > join && beyond(c.0) <= 0.5 * spacing && beyond(c.1) <= 0.5 * spacing {
                    return Some(na);
                }
            }
        }
        None
    };
    let mut accepted: Vec<(usize, LineEvidence)> = Vec::new();
    let mut taken: Vec<bool> = vec![false; all.len()];
    let mut blocked_count = 0usize;
    let mut blocked_seen: Vec<bool> = vec![false; all.len()];
    loop {
        let mut changed = false;
        for &c in &candidates {
            if taken[c] {
                continue;
            }
            let sc = drawn[c].expect("a line");
            let hit = nodes
                .iter()
                .find_map(|&nd| junction(sc, drawn[nd].expect("a line"), text_boxes, join, params).map(|kind| (kind, nd)))
                .or_else(|| crosses_pair(sc, &nodes).map(|nd| ("crossing_pair", nd)));
            let Some((kind, to)) = hit else { continue };
            if blocked(c, &drawn) {
                if !blocked_seen[c] {
                    blocked_seen[c] = true;
                    blocked_count += 1;
                }
                continue;
            }
            taken[c] = true;
            nodes.push(c);
            accepted.push((c, LineEvidence::Closure { kind, to, length_px: all[c].primitive.length(), repeat_px: repeat }));
            changed = true;
        }
        if !changed {
            break;
        }
    }
    (accepted, blocked_count)
}

/// Vectorise the remaining ink and classify the strokes. Dashed and dash-dot pieces are first
/// merged into whole runs (3°, 1.5 px, gaps up to the merge gap) so that a hidden line or an axis
/// is one stroke; the pattern of every line stroke is then measured again on the page ink `page`,
/// since erasing crossing members leaves artificial gaps in the remainder. The page's line patterns are calibrated
/// from the reliable runs; with a known dashed family, two-dash lines are merged and the hidden closure applied (line-type
/// knowledge). `thin_reference_px` is the median width of the refined thin members; when none exists, the 30th percentile
/// of the stroke widths stands in. `centre_lines` are the refined proposed centre lines, which a dash-dot stroke may
/// duplicate; `text_boxes` the erased texts across which a hidden line may continue; `other_runs` the runs measured on
/// lines placed before this step (the proposed centre lines), which calibrate the page's families as well;
/// `in_view` whether a point lies inside a drawing view: sheet ink (frame, title block, tables) calibrates nothing.
#[allow(clippy::too_many_arguments)]
pub fn vectorise_remainder(
    rest: &Ink,
    page: &Ink,
    erased: &[Segment],
    thin_reference_px: Option<f64>,
    centre_lines: &[(P, P)],
    text_boxes: &[[f64; 4]],
    other_runs: &[InkRuns],
    in_view: &dyn Fn(P) -> bool,
    params: &StructureParams,
) -> (Vec<Classified>, f64, usize, LinePatterns) {
    // Patterns are read with the original page beside the remainder: a gap the page inks is a crossing that was
    // erased, not a dash gap.
    let step = steps();
    // The line-type knowledge of this round is read in the views: the frame and the title block hold continuous lines and
    // text (frame: GB/T 14689-2008 4.1, DC-G1-085), and keep the round-3 reading.
    let ctx = PatternContext { px_per_mm: params.px_per_mm, original: Some(page), in_view: Some(in_view) };
    // Short pieces of crossed lines, rejoined, serve the two-dash rule and the hidden closure (scope items 4 and 5): they
    // are candidates only, kept where they become hidden members.
    let (mut strokes, mut rescued) =
        if step.two_dash { vectorize_in_rescued(rest, params.min_length_px, &ctx) } else { (vectorize_in(rest, params.min_length_px, &ctx), Vec::new()) };
    let before = strokes.len();
    strokes.retain(|s| !remnant_of_erased(s, erased));
    let remnants = before - strokes.len();
    rescued.retain(|s| !remnant_of_erased(s, erased) && in_view(s.primitive.mid()));
    let (patterned, plain): (Vec<Stroke>, Vec<Stroke>) = strokes
        .into_iter()
        .partition(|s| matches!(s.pattern, Pattern::Dashed | Pattern::DashDot));
    // The dashes of one line are merged across their own gaps, which may be longer than the plain merge gap (a
    // hidden line plotted with a large linetype scale): up to the longest dash gap on paper the pattern inference
    // allows.
    // A merged run is kept when the page ink along it confirms a family; otherwise its pieces go back among the plain
    // ones, for the chains and the two-dash rule to read (the vectorizer's run may join pieces of different lines).
    let groups = merge_collinear_groups(
        patterned,
        params.merge_angle_rad,
        params.merge_offset_px,
        params.merge_gap_px.max(bounds().max_dash_gap_paper_mm * params.px_per_mm),
    );
    let mut plain = plain;
    let mut merged: Vec<Stroke> = Vec::new();
    for (stroke, members) in groups {
        // (The confirmation serves the two-dash rule, scope item 4: an unconfirmed run may hide a two-dash line.)
        let confirmed = !step.two_dash || !in_view(stroke.primitive.mid()) || match stroke.primitive {
            Primitive::Line { p, q } => members.len() < 2
                || matches!(measure_runs_texts(page, p, q, params.px_per_mm, text_boxes).family, LineFamily::Dashed | LineFamily::Chain | LineFamily::DoubleChain),
            Primitive::Arc { .. } => true,
        };
        if confirmed {
            merged.push(stroke);
        } else {
            plain.extend(members);
        }
    }
    let mut all: Vec<Stroke> = plain.into_iter().chain(merged).collect();
    let main = all.len();
    all.extend(rescued);
    let is_rescued: Vec<bool> = (0..all.len()).map(|i| i >= main).collect();
    let runs: Vec<Option<InkRuns>> = all
        .iter_mut()
        .map(|s| match s.primitive {
            Primitive::Line { p, q } => {
                let r = measure_runs_texts(page, p, q, params.px_per_mm, text_boxes);
                s.pattern = if in_view(s.primitive.mid()) { stroke_pattern(page, p, q, &r) } else { measure_pattern_round3(page, p, q) };
                Some(r)
            }
            Primitive::Arc { .. } => None,
        })
        .collect();
    let thin = thin_reference_px.unwrap_or_else(|| {
        let mut ws: Vec<f64> = all[..main].iter().map(|s| s.width_px).filter(|w| *w > 0.0).collect();
        if ws.is_empty() {
            return 2.0;
        }
        ws.sort_by(|a, b| a.total_cmp(b));
        ws[(ws.len() - 1) * 3 / 10]
    });
    let evidence: Vec<Option<LineEvidence>> = runs
        .iter()
        .zip(&all)
        .map(|(r, s)| {
            r.as_ref().filter(|r| step.chain_short_dash && in_view(s.primitive.mid()) && matches!(r.family, LineFamily::Dashed | LineFamily::Chain | LineFamily::DoubleChain)).map(|r| {
                let mut g = r.offs.clone();
                LineEvidence::Pattern { family: r.family, dashes: r.ons.len(), gap_px: median(&mut g).unwrap_or(0.0) }
            })
        })
        .collect();
    let (mut all, mut runs, mut evidence, mut is_rescued, candidates) = remeasure_chains(all, runs, evidence, is_rescued, page, text_boxes, in_view, params);
    let measured: Vec<&InkRuns> = runs
        .iter()
        .zip(&all)
        .zip(&is_rescued)
        .filter(|((_, s), r)| !**r && in_view(s.primitive.mid()))
        .filter_map(|((r, _), _)| r.as_ref())
        .chain(other_runs.iter())
        .collect();
    let mut patterns = LinePatterns {
        dashed: calibrate(&measured, LineFamily::Dashed, params.px_per_mm),
        chain: calibrate(&measured, LineFamily::Chain, params.px_per_mm),
        double_chain: calibrate(&measured, LineFamily::DoubleChain, params.px_per_mm),
        two_dash_unjudged: 0,
        two_dash_merged: 0,
        closure_accepted: 0,
        closure_blocked: 0,
        inconsistent_runs: 0,
    };
    // The elements of one line type are equal across one drawing (textbook p17), each linetype scale repeated: a patterned
    // stroke whose gap matches no mode of its family backed by two runs (once the page has one) is not of that family.
    let tol = bounds().page_gap_tolerance;
    let mut inconsistent = 0usize;
    for (i, r) in runs.iter_mut().enumerate() {
        let Some(r) = r else { continue };
        // (The consistency with the page serves the chain line's reading, scope item 3; in the views.)
        if !step.chain_short_dash {
            break;
        }
        if !in_view(all[i].primitive.mid()) {
            continue;
        }
        let calibration = match r.family {
            LineFamily::Dashed => &patterns.dashed,
            LineFamily::Chain => &patterns.chain,
            LineFamily::DoubleChain => &patterns.double_chain,
            _ => continue,
        };
        if calibration.modes.iter().all(|m| m.1 < 2) || r.ons.len() < 3 {
            continue;
        }
        let mut g = r.offs.clone();
        let gap = median(&mut g).unwrap_or(0.0);
        if !calibration.matches(gap, tol) {
            r.family = LineFamily::Undetermined;
            r.pattern = Pattern::Unknown;
            all[i].pattern = Pattern::Unknown;
            evidence[i] = None;
            inconsistent += 1;
        }
    }
    patterns.inconsistent_runs = inconsistent;
    let elements = patterns.dashed_elements();
    if elements.is_none() {
        patterns.two_dash_unjudged = candidates.len();
    } else if step.two_dash {
        let mut pairs = two_dash_pairs(&all, &candidates, page, text_boxes, elements, params);
        // Overlapping candidates: the pair whose two dashes agree best is taken, each piece once.
        pairs.sort_by(|x, y| ((x.2.0 - x.2.1).abs() / x.2.0.max(x.2.1)).total_cmp(&((y.2.0 - y.2.1).abs() / y.2.0.max(y.2.1))).then(x.0.cmp(&y.0)));
        let mut used: Vec<bool> = vec![false; all.len()];
        pairs.retain(|&(i, j, _, _)| {
            let free = !used[i] && !used[j];
            if free {
                used[i] = true;
                used[j] = true;
            }
            free
        });
        patterns.two_dash_merged = pairs.len();
        let mut drop: Vec<usize> = Vec::new();
        for &(i, j, (la, lb), gap) in &pairs {
            let (si, sj) = (segment(&all[i]).expect("a line"), segment(&all[j]).expect("a line"));
            let rel = on_one_line(si, sj, params.merge_angle_rad, params.merge_offset_px + 0.5).expect("paired on one line");
            let merged = joined(&[&all[i], &all[j]], rel.outer.0, rel.outer.1, Pattern::Dashed);
            all[i] = merged;
            is_rescued[i] = false;
            runs[i] = Some(InkRuns { pattern: Pattern::Dashed, family: LineFamily::Dashed, ons: vec![la, lb], offs: vec![gap] });
            evidence[i] = Some(LineEvidence::TwoDash { ends_px: (la, lb), gap_px: gap });
            drop.push(j);
        }
        drop.sort_unstable();
        for j in drop.into_iter().rev() {
            all.remove(j);
            runs.remove(j);
            evidence.remove(j);
            is_rescued.remove(j);
        }
    }
    let thick: Vec<bool> = all.iter().map(|s| s.width_px >= params.thick_ratio * thin).collect();
    let mut closure: BTreeMap<usize, LineEvidence> = BTreeMap::new();
    if let Some(el) = elements.filter(|_| step.closure) {
        let (accepted, blocked) = hidden_closure(&all, &runs, &thick, page, text_boxes, el, in_view, params);
        patterns.closure_accepted = accepted.len();
        patterns.closure_blocked = blocked;
        closure.extend(accepted);
    }
    let covered = |s: &Stroke| -> bool {
        let Primitive::Line { p, q } = s.primitive else {
            return false;
        };
        let Some(d) = unit(sub(q, p)) else {
            return false;
        };
        let m = s.primitive.mid();
        centre_lines.iter().any(|&(a, b)| {
            let Some(e) = unit(sub(b, a)) else {
                return false;
            };
            undirected_angle(d, e) < 0.1 && {
                let n = perp(e);
                let t = dot(sub(m, a), e);
                dot(sub(m, a), n).abs() <= params.centre_cover_px
                    && t >= -params.centre_cover_px
                    && t <= dist(a, b) + params.centre_cover_px
            }
        })
    };
    // Rescued pieces the two-dash rule and the closure did not take are dropped, as the vectorizer drops short pieces;
    // the closure's references follow the indices that remain.
    let keep: Vec<bool> = (0..all.len()).map(|i| !is_rescued[i] || closure.contains_key(&i)).collect();
    let mut new_index: Vec<usize> = vec![usize::MAX; all.len()];
    let mut next = 0usize;
    for i in 0..all.len() {
        if keep[i] {
            new_index[i] = next;
            next += 1;
        }
    }
    for e in closure.values_mut() {
        if let LineEvidence::Closure { to, .. } = e {
            *to = new_index[*to];
        }
    }
    let out = all
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(i, s)| {
            let thick = thick[i];
            let (class, evidence) = if let Some(e) = closure.remove(&i) {
                (StrokeClass::HiddenContour, Some(e))
            } else {
                let class = match s.pattern {
                    Pattern::Dashed => StrokeClass::HiddenContour,
                    Pattern::DashDot => {
                        if covered(&s) {
                            StrokeClass::CoveredCentre
                        } else {
                            StrokeClass::CentreAxis
                        }
                    }
                    _ if thick => StrokeClass::VisibleContour,
                    _ => StrokeClass::ThinUnresolved,
                };
                (class, evidence[i].take())
            };
            Classified {
                stroke: s,
                class,
                thick,
                evidence,
            }
        })
        .collect();
    (out, thin, remnants, patterns)
}

/// Developer entry of the structure step on a page image alone (nothing observed, nothing erased): every classified
/// stroke with its class, pattern, geometry and line evidence, and the page's line patterns, in page pixels. Reads only
/// the image bytes and its resolution; used to test the line-type knowledge on controlled plots.
pub fn pixel_lines(png: &[u8], dpi: f64, threshold: u8) -> anyhow::Result<serde_json::Value> {
    use serde_json::json;
    let ink = Ink::from_png(png, threshold)?;
    let params = StructureParams::at_dpi(dpi);
    let (classified, thin, remnants, lp) = vectorise_remainder(&ink, &ink, &[], None, &[], &[], &[], &|_| true, &params);
    let classified = merge_cocircular_axes(classified);
    let r = |x: f64| (x * 1000.0).round() / 1000.0;
    let strokes: Vec<serde_json::Value> = classified
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let class = match c.class {
                StrokeClass::VisibleContour => "visible_contour",
                StrokeClass::HiddenContour => "hidden_contour",
                StrokeClass::CentreAxis => "centre_axis",
                StrokeClass::ThinUnresolved => "thin_unresolved",
                StrokeClass::CoveredCentre => "covered_centre",
            };
            let geometry = match c.stroke.primitive {
                Primitive::Line { p, q } => json!({"type": "line", "p": [r(p.0), r(p.1)], "q": [r(q.0), r(q.1)]}),
                Primitive::Arc { c: centre, r: radius, start_rad, sweep_rad } => {
                    json!({"type": "arc", "c": [r(centre.0), r(centre.1)], "r": r(radius), "start_rad": r(start_rad), "sweep_rad": r(sweep_rad)})
                }
            };
            let evidence = match &c.evidence {
                Some(LineEvidence::Pattern { family, dashes, gap_px }) => json!({"rule": "own_pattern", "family": format!("{family:?}"), "dashes": dashes, "gap_px": r(*gap_px)}),
                Some(LineEvidence::TwoDash { ends_px, gap_px }) => json!({"rule": "two_dash", "ends_px": [r(ends_px.0), r(ends_px.1)], "gap_px": r(*gap_px)}),
                Some(LineEvidence::Closure { kind, to, length_px, repeat_px }) => json!({"rule": "hidden_closure", "junction": kind, "joined_stroke": to, "length_px": r(*length_px), "repeat_px": r(*repeat_px)}),
                None => serde_json::Value::Null,
            };
            json!({"index": i, "class": class, "pattern": format!("{:?}", c.stroke.pattern), "thick": c.thick, "width_px": r(c.stroke.width_px),
                "length_px": r(c.stroke.primitive.length()), "geometry": geometry, "evidence": evidence})
        })
        .collect();
    let family = |f: &FamilyCalibration| json!({"state": f.state, "elements_px": f.elements_px.iter().map(|x| r(*x)).collect::<Vec<_>>(), "runs": f.runs, "consistent_runs": f.consistent_runs,
        "gap_modes_px": f.modes.iter().map(|(g, n)| json!([r(*g), n])).collect::<Vec<_>>()});
    Ok(json!({
        "schema": "oe-drawing-pixel.lines/1", "dpi": dpi, "threshold": threshold, "page_px": [ink.width, ink.height],
        "thin_reference_px": r(thin), "remnants_of_erased": remnants,
        "line_patterns": {"dashed": family(&lp.dashed), "chain": family(&lp.chain), "double_chain": family(&lp.double_chain),
            "two_dash_unjudged": lp.two_dash_unjudged, "two_dash_merged": lp.two_dash_merged, "closure_accepted": lp.closure_accepted, "closure_blocked": lp.closure_blocked,
            "inconsistent_runs": lp.inconsistent_runs},
        "strokes": strokes,
    }))
}

/// Co-circular centre-axis arcs (the dashes of a dash-dot circle, a bolt circle) merged into one arc
/// per circle: centres within 3 % of the radius (at least 3 px), radii within 3 % (at least 2 px); the
/// merged arc spans the covered angles, a full circle when they cover at least 330°.
pub fn merge_cocircular_axes(classified: Vec<Classified>) -> Vec<Classified> {
    let arcs: Vec<usize> = (0..classified.len())
        .filter(|i| classified[*i].class == StrokeClass::CentreAxis && matches!(classified[*i].stroke.primitive, Primitive::Arc { .. }))
        .collect();
    if arcs.len() < 2 {
        return classified;
    }
    let geo = |i: usize| match classified[i].stroke.primitive {
        Primitive::Arc { c, r, start_rad, sweep_rad } => (c, r, start_rad, sweep_rad),
        Primitive::Line { .. } => unreachable!("filtered to arcs"),
    };
    let mut dsu = crate::pattern::Dsu::new(arcs.len());
    for a in 0..arcs.len() {
        for b in a + 1..arcs.len() {
            let (ca, ra, ..) = geo(arcs[a]);
            let (cb, rb, ..) = geo(arcs[b]);
            let r = ra.max(rb);
            if dist(ca, cb) <= (0.03 * r).max(3.0) && (ra - rb).abs() <= (0.03 * r).max(2.0) {
                dsu.union(a, b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &arc) in arcs.iter().enumerate() {
        groups.entry(dsu.find(k)).or_default().push(arc);
    }
    let mut merged: BTreeMap<usize, Classified> = BTreeMap::new();
    let mut drop: Vec<usize> = Vec::new();
    for members in groups.values().filter(|m| m.len() > 1) {
        let mut bins = [false; 360];
        let (mut wsum, mut cx, mut cy, mut rr, mut width, mut support, mut pixels) = (0.0, 0.0, 0.0, 0.0, 0.0f64, 0.0, 0usize);
        for &i in members {
            let (c, r, start, sweep) = geo(i);
            let steps = (sweep.abs().to_degrees().ceil() as usize).max(1);
            for k in 0..=steps {
                let a = start + sweep * k as f64 / steps as f64;
                bins[(a.to_degrees().rem_euclid(360.0) as usize).min(359)] = true;
            }
            let wgt = r * sweep.abs();
            wsum += wgt;
            cx += c.0 * wgt;
            cy += c.1 * wgt;
            rr += r * wgt;
            width = width.max(classified[i].stroke.width_px);
            support += classified[i].stroke.support * wgt;
            pixels += classified[i].stroke.pixels;
        }
        if wsum <= 0.0 {
            continue;
        }
        let covered = bins.iter().filter(|b| **b).count();
        // The covered span: the longest run of covered bins around the circle.
        let (start_deg, sweep_deg) = if covered >= 330 {
            (0.0, 360.0)
        } else {
            let mut best = (0usize, 0usize);
            for s0 in 0..360 {
                if !bins[s0] || bins[(s0 + 359) % 360] {
                    continue;
                }
                let mut len = 0;
                while len < 360 && bins[(s0 + len) % 360] {
                    len += 1;
                }
                if len > best.1 {
                    best = (s0, len);
                }
            }
            (best.0 as f64, best.1 as f64)
        };
        let first = members[0];
        let mut c = classified[first].clone();
        c.stroke.primitive = Primitive::Arc {
            c: (cx / wsum, cy / wsum),
            r: rr / wsum,
            start_rad: start_deg.to_radians(),
            sweep_rad: if sweep_deg >= 360.0 { std::f64::consts::TAU } else { sweep_deg.to_radians() },
        };
        c.stroke.width_px = width;
        c.stroke.support = support / wsum;
        c.stroke.pixels = pixels;
        merged.insert(first, c);
        drop.extend(members.iter().skip(1).copied());
    }
    // Closure evidence names the hidden line it joined by index: renumber after the dropped arcs.
    let renumber: Vec<usize> = (0..classified.len()).scan(0usize, |next, i| {
        let k = *next;
        if !drop.contains(&i) {
            *next += 1;
        }
        Some(k)
    }).collect();
    classified
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(i, c)| {
            let mut c = merged.remove(&i).unwrap_or(c);
            if let Some(LineEvidence::Closure { to, .. }) = &mut c.evidence {
                *to = renumber[*to];
            }
            c
        })
        .collect()
}

/// Group collinear line pieces `(p, q)` with union–find. Two pieces group when their directions are
/// within `angle_tol`, both endpoints of the shorter lie within `offset_tol` of the longer one's line
/// and their extents overlap or are at most `gap_tol` apart; or, up to `explained_gap_tol`, when
/// `explained(gap_start, gap_end)` says the gap is interrupted by a text or a section region.
pub fn group_collinear(
    pieces: &[(P, P)],
    angle_tol: f64,
    offset_tol: f64,
    gap_tol: f64,
    explained_gap_tol: f64,
    explained: impl Fn(P, P) -> bool,
) -> Vec<Vec<usize>> {
    let n = pieces.len();
    let mut dsu = Dsu::new(n);
    let reach = gap_tol.max(explained_gap_tol) + offset_tol;
    for a in 0..n {
        for b in a + 1..n {
            let (pa, qa) = pieces[a];
            let (pb, qb) = pieces[b];
            if pa.0.max(qa.0) + reach < pb.0.min(qb.0)
                || pb.0.max(qb.0) + reach < pa.0.min(qa.0)
                || pa.1.max(qa.1) + reach < pb.1.min(qb.1)
                || pb.1.max(qb.1) + reach < pa.1.min(qa.1)
            {
                continue;
            }
            let (la, lb) = (dist(pa, qa), dist(pb, qb));
            let (rp, rq, sp, sq, lr) = if la >= lb {
                (pa, qa, pb, qb, la)
            } else {
                (pb, qb, pa, qa, lb)
            };
            let (Some(d), Some(ds)) = (unit(sub(rq, rp)), unit(sub(sq, sp))) else {
                continue;
            };
            if undirected_angle(d, ds) > angle_tol {
                continue;
            }
            let nrm = perp(d);
            if dot(sub(sp, rp), nrm).abs() > offset_tol || dot(sub(sq, rp), nrm).abs() > offset_tol {
                continue;
            }
            let (ta, tb) = (dot(sub(sp, rp), d), dot(sub(sq, rp), d));
            let (lo, hi) = (ta.min(tb), ta.max(tb));
            let gap = (lo - lr).max(-hi);
            if gap <= gap_tol {
                dsu.union(a, b);
            } else if gap <= explained_gap_tol {
                let (g0, g1) = if lo > lr {
                    (add(rp, mul(d, lr)), add(rp, mul(d, lo)))
                } else {
                    (add(rp, mul(d, hi)), rp)
                };
                if explained(g0, g1) {
                    dsu.union(a, b);
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        groups.entry(dsu.find(i)).or_default().push(i);
    }
    groups.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygons_boxes_and_segments_intersect_as_expected() {
        let square = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(point_in_polygon((5.0, 5.0), &square));
        assert!(!point_in_polygon((15.0, 5.0), &square));
        assert!(segment_hits_polygon((-5.0, 5.0), (15.0, 5.0), &square));
        assert!(!segment_hits_polygon((-5.0, 15.0), (15.0, 15.0), &square));
        assert!(segment_hits_box((-5.0, 5.0), (15.0, 5.0), [0.0, 0.0, 10.0, 10.0], 0.0));
        assert!(!segment_hits_box((-5.0, 12.0), (15.0, 12.0), [0.0, 0.0, 10.0, 10.0], 1.0));
        assert!(segment_hits_box((-5.0, 12.0), (15.0, 12.0), [0.0, 0.0, 10.0, 10.0], 3.0));
        let mut ink = Ink::from_mask(20, 20, vec![true; 400]);
        erase_polygons(&mut ink, &[square]);
        assert!(!ink.is_dark(5, 5) && ink.is_dark(15, 15) && ink.is_dark(5, 15));
    }

    #[test]
    fn collinear_pieces_group_across_explained_gaps_only() {
        let pieces = vec![
            ((0.0, 0.0), (100.0, 0.0)),
            ((110.0, 0.5), (200.0, 0.5)),
            ((300.0, 0.0), (400.0, 0.0)),
            ((0.0, 50.0), (100.0, 50.0)),
            ((600.0, 0.0), (700.0, 0.0)),
        ];
        // The 200–300 gap is explained (a text sits there); the 400–600 gap is not.
        let explained = |a: P, b: P| a.0.min(b.0) >= 190.0 && a.0.max(b.0) <= 310.0;
        let groups = group_collinear(&pieces, 0.052, 1.5, 30.0, 150.0, explained);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], vec![0, 1, 2]);
        assert_eq!(groups[1], vec![3]);
        assert_eq!(groups[2], vec![4]);
    }

    /// A page with 3 px strokes: horizontal `(y, x0, x1)` and vertical `(x, y0, y1)` lines.
    fn page(w: usize, h: usize, hlines: &[(i64, i64, i64)], vlines: &[(i64, i64, i64)]) -> Ink {
        let mut m = vec![false; w * h];
        let mut set = |x: i64, y: i64| {
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                m[y as usize * w + x as usize] = true;
            }
        };
        for &(y, x0, x1) in hlines {
            for x in x0..=x1 {
                for dy in -1..=1 {
                    set(x, y + dy);
                }
            }
        }
        for &(x, y0, y1) in vlines {
            for y in y0..=y1 {
                for dx in -1..=1 {
                    set(x + dx, y);
                }
            }
        }
        Ink::from_mask(w, h, m)
    }

    /// The pieces of a horizontal pattern from `x0`: alternating ink and gap lengths, ink first.
    fn pattern(y: i64, x0: i64, elements: &[i64]) -> Vec<(i64, i64, i64)> {
        let mut out = Vec::new();
        let mut x = x0;
        for (k, &l) in elements.iter().enumerate() {
            if k % 2 == 0 {
                out.push((y, x, x + l - 1));
            }
            x += l;
        }
        out
    }

    fn classify(ink: &Ink, texts: &[[f64; 4]]) -> (Vec<Classified>, LinePatterns) {
        let (c, _, _, lp) = vectorise_remainder(ink, ink, &[], None, &[], texts, &[], &|_| true, &StructureParams::default());
        (c, lp)
    }

    /// The classes of the strokes lying on the horizontal line `y` between `x0` and `x1`.
    fn classes_on_row(c: &[Classified], y: f64, x0: f64, x1: f64) -> Vec<StrokeClass> {
        c.iter()
            .filter(|k| matches!(k.stroke.primitive, Primitive::Line { p, q } if (p.1 - y).abs() < 2.0 && (q.1 - y).abs() < 2.0 && p.0.max(q.0) > x0 && p.0.min(q.0) < x1))
            .map(|k| k.class)
            .collect()
    }

    fn classes_on_column(c: &[Classified], x: f64, y0: f64, y1: f64) -> Vec<StrokeClass> {
        c.iter()
            .filter(|k| matches!(k.stroke.primitive, Primitive::Line { p, q } if (p.0 - x).abs() < 2.0 && (q.0 - x).abs() < 2.0 && p.1.max(q.1) > y0 && p.1.min(q.1) < y1))
            .map(|k| k.class)
            .collect()
    }

    /// A long dashed line (dashes 80, gaps 33: a CAD HIDDEN line at a large linetype scale) for the page's dashed calibration.
    fn dashed_line(y: i64) -> Vec<(i64, i64, i64)> {
        pattern(y, 20, &[80, 33, 80, 33, 80, 33, 80, 33, 80, 33, 80])
    }

    #[test]
    fn chain_lines_read_with_a_dot_or_a_short_dash_and_two_short_elements_are_no_centre_line() {
        // CAD CENTER (long 190, gap 35, short dash 40), a dot chain (long 96, gap 12, dot 4) and PHANTOM (two short
        // dashes between the long ones).
        let mut h = pattern(50, 20, &[190, 35, 40, 35, 190, 35, 40, 35, 190, 35, 40, 35, 190]);
        h.extend(pattern(150, 20, &[96, 12, 4, 12, 96, 12, 4, 12, 96, 12, 4, 12, 96]));
        h.extend(pattern(250, 20, &[190, 35, 40, 35, 40, 35, 190, 35, 40, 35, 40, 35, 190]));
        let (c, lp) = classify(&page(1100, 320, &h, &[]), &[]);
        assert!(classes_on_row(&c, 50.0, 0.0, 1100.0).iter().all(|k| *k == StrokeClass::CentreAxis), "{:?}", classes_on_row(&c, 50.0, 0.0, 1100.0));
        assert!(classes_on_row(&c, 150.0, 0.0, 1100.0).iter().all(|k| *k == StrokeClass::CentreAxis));
        assert!(classes_on_row(&c, 250.0, 0.0, 1100.0).iter().all(|k| *k != StrokeClass::CentreAxis && *k != StrokeClass::HiddenContour));
        // Two chain scales on one page, one run each: no mode backed by two runs, the family in conflict, every run kept.
        assert_ne!(lp.chain.state, "absent");
        assert_eq!(lp.double_chain.state, "known");
    }

    #[test]
    fn a_two_dash_line_is_hidden_against_the_page_calibration_and_undecided_without_it() {
        let two = pattern(200, 300, &[52, 33, 52]);
        let mut h = dashed_line(50);
        h.extend(two.iter().copied());
        let (c, lp) = classify(&page(900, 260, &h, &[]), &[]);
        assert_eq!(lp.dashed.state, "known");
        assert_eq!(classes_on_row(&c, 200.0, 290.0, 450.0), vec![StrokeClass::HiddenContour]);
        assert!(c.iter().any(|k| matches!(k.evidence, Some(LineEvidence::TwoDash { .. }))));
        let (c, lp) = classify(&page(900, 260, &two, &[]), &[]);
        assert_eq!(lp.dashed.state, "absent");
        assert!(lp.two_dash_unjudged >= 1);
        assert!(classes_on_row(&c, 200.0, 290.0, 450.0).iter().all(|k| *k != StrokeClass::HiddenContour));
    }

    #[test]
    fn an_outline_broken_by_grooves_is_no_two_dash_line() {
        // Two lands of the dash's length with a gap of the dashed gap between them, the groove walls ending at its edges.
        let mut h = dashed_line(50);
        h.extend(pattern(200, 300, &[52, 33, 52]));
        h.push((222, 352, 384));
        let v = [(352, 200, 222), (384, 200, 222)];
        let (c, lp) = classify(&page(900, 260, &h, &v), &[]);
        assert_eq!(lp.dashed.state, "known");
        assert!(classes_on_row(&c, 200.0, 290.0, 450.0).iter().all(|k| *k != StrokeClass::HiddenContour), "{:?}", classes_on_row(&c, 200.0, 290.0, 450.0));
    }

    #[test]
    fn the_closure_joins_short_hidden_edges_at_corners_and_stops_at_a_visible_corner() {
        // Seed: the dashed line ends at x 664. A short solid edge corners it (hidden); elsewhere a short edge corners a
        // seed and a long solid line (a visible outline's corner: kept as drawn).
        let mut h = dashed_line(50);
        h.extend(dashed_line(300));
        h.push((360, 664, 1100));
        let v = [(664, 50, 110), (664, 300, 360)];
        let (c, _) = classify(&page(1250, 400, &h, &v), &[]);
        assert_eq!(classes_on_column(&c, 664.0, 55.0, 105.0), vec![StrokeClass::HiddenContour]);
        assert!(c.iter().any(|k| matches!(k.evidence, Some(LineEvidence::Closure { kind: "corner", .. }))));
        assert!(classes_on_column(&c, 664.0, 305.0, 355.0).iter().all(|k| *k != StrokeClass::HiddenContour));
    }

    #[test]
    fn a_short_line_across_two_parallel_hidden_lines_is_hidden_and_one_crossing_is_not_enough() {
        // Two parallel dashed lines; x 400 lies on a dash of both. One short line crosses both (a termination line),
        // another crosses one only.
        let h = [dashed_line(100), dashed_line(160)].concat();
        let v = [(400, 90, 170), (630, 150, 200)];
        let (c, _) = classify(&page(900, 260, &h, &v), &[]);
        assert!(classes_on_column(&c, 400.0, 92.0, 168.0).contains(&StrokeClass::HiddenContour), "{:?}", classes_on_column(&c, 400.0, 92.0, 168.0));
        assert!(c.iter().any(|k| matches!(k.evidence, Some(LineEvidence::Closure { kind: "crossing_pair", .. }))));
        assert!(classes_on_column(&c, 630.0, 165.0, 198.0).iter().all(|k| *k != StrokeClass::HiddenContour));
    }

    #[test]
    fn a_piece_is_short_as_drawn_only_where_its_ink_ends() {
        let ink = page(400, 60, &[(30, 10, 380)], &[]);
        assert!(solid_extent(&ink, (100.0, 30.0), (150.0, 30.0), 200.0) >= 200.0);
        let ink = page(400, 60, &[(30, 100, 150)], &[]);
        assert!(solid_extent(&ink, (100.0, 30.0), (150.0, 30.0), 200.0) < 60.0);
        let (a, b) = drawn_ends(&page(400, 60, &[(30, 90, 160)], &[]), (100.0, 30.0), (150.0, 30.0), 30.0);
        assert!((a.0 - 89.0).abs() <= 2.0 && (b.0 - 161.0).abs() <= 2.0, "{a:?} {b:?}");
    }

    #[test]
    fn a_run_whose_gap_disagrees_with_the_page_is_not_of_the_family() {
        // Three chain lines with gaps of 35 calibrate the page; a fourth alternating line with gaps of 60 is no chain
        // line on this page.
        let mut h = Vec::new();
        for y in [40, 100, 160] {
            h.extend(pattern(y, 20, &[190, 35, 40, 35, 190, 35, 40, 35, 190]));
        }
        h.extend(pattern(240, 20, &[190, 60, 40, 60, 190, 60, 40, 60, 190]));
        let (c, lp) = classify(&page(900, 300, &h, &[]), &[]);
        assert_eq!(lp.chain.state, "known");
        assert!(lp.inconsistent_runs >= 1);
        assert!(classes_on_row(&c, 240.0, 0.0, 900.0).iter().all(|k| *k != StrokeClass::CentreAxis));
    }

    #[test]
    fn a_line_ending_in_a_gap_leaves_it_a_dash_gap_and_walls_at_both_edges_do_not() {
        // Dashes 20..99, 133..212, 246..325, ...: a vertical ends on the line inside the gap 213..245 (a CAD T junction
        // falling in a gap); elsewhere two walls stand at both edges of that gap on one side (a groove).
        let h = dashed_line(50);
        let reach = 0.7 * 300.0 / 25.4;
        let t_junction = page(900, 160, &h, &[(229, 50, 120)]);
        assert!(!gap_edge_walled(&t_junction, (20.0, 50.0), (774.0, 50.0), reach));
        let groove = page(900, 160, &h, &[(213, 50, 120), (245, 50, 120)]);
        assert!(gap_edge_walled(&groove, (20.0, 50.0), (774.0, 50.0), reach));
        // Walls on opposite sides (a line crossing at one edge, split by the skeleton, and one ending on the other side)
        // are no groove.
        let opposite = page(900, 160, &h, &[(213, 50, 120), (245, 0, 50)]);
        assert!(!gap_edge_walled(&opposite, (20.0, 50.0), (774.0, 50.0), reach));
    }

    #[test]
    fn a_crossing_erased_before_vectorizing_does_not_cut_a_dash() {
        // The page: a dashed line crossed in its third dash by a vertical; the remainder has the vertical erased, which
        // cut that dash into pieces of 24 and 50 px with a gap the page ink fills.
        let h = dashed_line(50);
        let original = page(900, 160, &h, &[(272, 0, 150)]);
        let mut cut: Vec<(i64, i64, i64)> = h.iter().copied().filter(|&(_, x0, _)| x0 != 246).collect();
        cut.extend([(50, 246, 269), (50, 276, 325)]);
        let rest = page(900, 160, &cut, &[]);
        let ctx = PatternContext { px_per_mm: 300.0 / 25.4, original: Some(&original), in_view: None };
        let strokes = vectorize_in(&rest, 35.0, &ctx);
        let on_row: Vec<&Stroke> = strokes.iter().filter(|s| matches!(s.primitive, Primitive::Line { p, q } if (p.1 - 50.0).abs() < 2.0 && (q.1 - 50.0).abs() < 2.0)).collect();
        assert!(on_row.len() >= 7 && on_row.iter().all(|s| s.pattern == Pattern::Dashed), "{on_row:?}");
    }

    #[test]
    fn rejoined_pieces_no_rule_takes_are_dropped() {
        // A 60 px line crossed by two verticals: its pieces are below the minimum; rejoined, it is a candidate only and,
        // with no dashed line on the page, nothing takes it.
        let ink = page(200, 120, &[(40, 30, 90)], &[(50, 10, 110), (70, 10, 110)]);
        let (c, lp) = classify(&ink, &[]);
        assert_eq!(lp.dashed.state, "absent");
        assert!(classes_on_row(&c, 40.0, 25.0, 95.0).is_empty(), "{:?}", classes_on_row(&c, 40.0, 25.0, 95.0));
    }

    #[test]
    fn thresholds_follow_the_page_resolution() {
        assert_eq!(StructureParams::at_dpi(300.0), StructureParams::default());
        let (a, b) = (StructureParams::at_dpi(300.0), StructureParams::at_dpi(600.0));
        for (x, y) in [(a.min_length_px, b.min_length_px), (a.merge_gap_px, b.merge_gap_px), (a.explained_gap_px, b.explained_gap_px),
            (a.touch_gap_px, b.touch_gap_px), (a.centre_cover_px, b.centre_cover_px), (a.merge_offset_px, b.merge_offset_px)] {
            assert!((y - 2.0 * x).abs() < 1e-9, "{x} {y}");
        }
        assert_eq!(a.thick_ratio, b.thick_ratio);
        assert!((b.min_length_px - 3.0 * 600.0 / 25.4).abs() < 1e-9);
    }
}
