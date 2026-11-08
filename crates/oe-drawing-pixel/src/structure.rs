//! Structure from the ink that no recognised member explains: the recognised members are erased from
//! a copy of the ink, the remainder is vectorised, strokes are classified by width and pattern, and
//! collinear contour pieces are grouped into structural lines (design §4: inferred structure is
//! structure; a gap explained by a text or a section region is bridged, an unexplained one is not).

use std::collections::BTreeMap;

use crate::geom::{
    P, add, clip_line_to_box, dist, dist_point_segment, dot, mul, perp, sub, undirected_angle, unit,
};
use crate::pattern::Dsu;
use crate::stroke::{Pattern, Primitive, Stroke};
use crate::vector::{merge_collinear, vectorize};
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

/// A classified stroke of the remainder.
#[derive(Clone, Debug, PartialEq)]
pub struct Classified {
    pub stroke: Stroke,
    pub class: StrokeClass,
    pub thick: bool,
}

/// Vectorise the remaining ink and classify the strokes. Dashed and dash-dot pieces are first
/// merged into whole runs (3°, 1.5 px, gaps up to the merge gap) so that a hidden line or an axis
/// is one stroke; the pattern of every line stroke is then measured again on the page ink `page`,
/// since erasing crossing members leaves artificial gaps in the remainder. `thin_reference_px` is
/// the median width of the refined thin members; when none exists, the 30th percentile of the
/// stroke widths stands in. `centre_lines` are the refined proposed centre lines, which a dash-dot
/// stroke may duplicate.
pub fn vectorise_remainder(
    rest: &Ink,
    page: &Ink,
    erased: &[Segment],
    thin_reference_px: Option<f64>,
    centre_lines: &[(P, P)],
    params: &StructureParams,
) -> (Vec<Classified>, f64, usize) {
    let mut strokes = vectorize(rest, params.min_length_px);
    let before = strokes.len();
    strokes.retain(|s| !remnant_of_erased(s, erased));
    let remnants = before - strokes.len();
    let (patterned, plain): (Vec<Stroke>, Vec<Stroke>) = strokes
        .into_iter()
        .partition(|s| matches!(s.pattern, Pattern::Dashed | Pattern::DashDot));
    let merged = merge_collinear(
        patterned,
        params.merge_angle_rad,
        params.merge_offset_px,
        params.merge_gap_px,
    );
    let mut all: Vec<Stroke> = plain.into_iter().chain(merged).collect();
    for s in &mut all {
        if let Primitive::Line { p, q } = s.primitive {
            s.pattern = crate::refine::measure_pattern(page, p, q);
        }
    }
    let thin = thin_reference_px.unwrap_or_else(|| {
        let mut ws: Vec<f64> = all.iter().map(|s| s.width_px).filter(|w| *w > 0.0).collect();
        if ws.is_empty() {
            return 2.0;
        }
        ws.sort_by(|a, b| a.total_cmp(b));
        ws[(ws.len() - 1) * 3 / 10]
    });
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
    let out = all
        .into_iter()
        .map(|s| {
            let thick = s.width_px >= params.thick_ratio * thin;
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
            Classified {
                stroke: s,
                class,
                thick,
            }
        })
        .collect();
    (out, thin, remnants)
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
    classified
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(i, c)| merged.remove(&i).unwrap_or(c))
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
}
