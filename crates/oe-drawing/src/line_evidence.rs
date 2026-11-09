//! Line knowledge decided from the drawing's own evidence (new-drawings correction round).
//!
//! * `line_width_evidence` — a mechanical drawing uses two line widths, thick and thin, about 2:1
//!   (GB/T 4457.4, GB/T 14665 4.2). Which lines are thick is read from the drawing itself: the
//!   lineweights it declares (entity or layer group 370) when they fall into two such classes,
//!   else, when it plots by colour, the colour of its thick-line layer against the colours of its
//!   thin-line layers (DC-G1-079). The GB/T 14665 table 6 layer meaning is used where it agrees. A
//!   continuous line on a table-6 thin layer drawn in the thick colour, or on a thick layer in a
//!   thin colour, is decided by how it joins the outline: joined to the outline at its ends it is
//!   visible outline (whatever the layer or colour); a dimension or extension line keeps its role.
//! * `outline_joins` — outline pieces meet at corners, T-junctions and tangent joins. A gap below a
//!   small fraction of the text height (invisible on paper) is a join, between arcs as between
//!   lines; an outline end on a thread circle is interrupted by the hole.
//! * `section_line_region` — section lines drawn one by one are equally spaced parallel thin
//!   lines: grouped by their own direction and spacing, not by the text height; a short stroke
//!   left inside a hatched region along its pattern belongs to that hatch.
//! * `hole_pattern` — the equal circles a count × diameter callout names are the holes' outlines,
//!   whatever layer they are drawn on, in the view the callout spans and where the same pattern
//!   is drawn again.
//! * `thread_representation` — the end view of a tapped hole (a thin three-quarter circle about a
//!   full circle) is read in a ratio window from the card; below the usual window it is still
//!   read when nothing contradicts it, and a callout spanning the hole is the cross-check.
use super::*;

/// Width class of a line from the drawing's own evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Width {
    Thick,
    Thin,
}

/// The drawing's own width evidence (card `line_width_evidence`).
pub(super) struct Widths {
    /// Width class of each root curve by its effective lineweight, when the drawing's lineweights
    /// fall into a thin and a thick class (variant `lineweight_classes`).
    by_weight: BTreeMap<String, Width>,
    /// The colour the drawing's thick-line layer plots in, when the drawing plots by colour under
    /// the checked layer convention and no lineweights decide.
    thick_colour: Option<i32>,
}

fn lineweight(v: Option<&str>) -> Option<i32> {
    v.and_then(|x| x.trim().parse::<i32>().ok())
}

impl Widths {
    pub(super) fn of(cx: &Ctx) -> Self {
        let c = representation::card_ref("line_width_evidence").ok();
        let ratio = c
            .and_then(|c| representation::param(c, "weight_ratio_min").ok())
            .unwrap_or(f64::INFINITY);
        let layer_weight: BTreeMap<&str, i32> =
            cx.d.records
                .iter()
                .filter(|r| r.kind == "LAYER")
                .filter_map(|r| Some((r.get(2)?, lineweight(r.get(370))?)))
                .collect();
        // Effective lineweight: the entity's own value, else ByLayer (-1 or absent) the layer's;
        // ByBlock (-2) and the default (-3) say nothing.
        let mut weights: BTreeMap<String, i32> = BTreeMap::new();
        for (h, r) in &cx.rec {
            if !is_curve_kind(&r.kind) {
                continue;
            }
            let own = lineweight(r.get(370));
            let w = match own {
                Some(w) if w >= 0 => Some(w),
                None | Some(-1) => layer_weight.get(cx.style[h].layer.as_str()).copied(),
                _ => None,
            };
            if let Some(w) = w.filter(|w| *w > 0) {
                weights.insert(h.clone(), w);
            }
        }
        let thin = weights.values().copied().min();
        let mut by_weight = BTreeMap::new();
        if let Some(thin) = thin {
            let class = |w: i32| {
                if w as f64 >= ratio * thin as f64 {
                    Width::Thick
                } else {
                    Width::Thin
                }
            };
            let thick_continuous = weights.iter().any(|(h, w)| {
                cx.style[h].pattern == Pattern::Continuous && class(*w) == Width::Thick
            });
            // Broken lines (centre, hidden) are thin lines: most of them must fall in the thin class.
            let broken: Vec<Width> = weights
                .iter()
                .filter(|(h, _)| {
                    matches!(
                        cx.style[*h].pattern,
                        Pattern::Dashed | Pattern::DashDot | Pattern::DashDoubleDot
                    )
                })
                .map(|(_, w)| class(*w))
                .collect();
            let thin_broken = broken.iter().filter(|w| **w == Width::Thin).count();
            if thick_continuous && 2 * thin_broken >= broken.len() {
                by_weight = weights
                    .iter()
                    .map(|(h, w)| (h.clone(), class(*w)))
                    .collect();
            }
        }
        // Colour: only when no lineweights decide, the drawing plots by colour (a colour-dependent
        // plot style, the AutoCAD default), the layer convention is checked and its thick layer has
        // a colour none of its thin layers or centre-line layer uses.
        let by_colour = !cx
            .d
            .header("$PSTYLEMODE", 290)
            .is_ok_and(|v| v.trim() == "0");
        let colour_of = |l: &str| {
            cx.d.records
                .iter()
                .find(|r| r.kind == "LAYER" && r.get(2) == Some(l))
                .map(|r| int(r, 62).abs())
        };
        let thick_colour = (by_weight.is_empty() && by_colour && cx.conv.qualified)
            .then(|| colour_of("01"))
            .flatten()
            .filter(|c| !cx.thin_colors.contains(c) && colour_of("05") != Some(*c));
        Widths {
            by_weight,
            thick_colour,
        }
    }
    /// Width class of a root source by its lineweight, when lineweights decide.
    pub(super) fn weight(&self, id: &str) -> Option<Width> {
        self.by_weight.get(id).copied()
    }
    pub(super) fn weights_decide(&self) -> bool {
        !self.by_weight.is_empty()
    }
    /// A continuous line drawn thick by the drawing's own evidence: its lineweight class, else the
    /// outline style of the checked layer convention, or the thick colour on any layer when the
    /// drawing plots by colour (an explicit thin colour is thin).
    pub(super) fn thick(&self, cx: &Ctx, id: &str) -> bool {
        let Some(st) = cx.style.get(id) else {
            return false;
        };
        if st.pattern != Pattern::Continuous || st.byblock {
            return false;
        }
        if let Some(w) = self.weight(id) {
            return w == Width::Thick;
        }
        let thin_explicit = st
            .explicit_color
            .is_some_and(|c| cx.thin_colors.contains(&c.abs()));
        cx.structural_style(id) || !thin_explicit && self.thick_colour == Some(st.color)
    }
}

/// Outline decided by the drawing's own evidence, for `structure()`.
#[derive(Default)]
pub(super) struct OutlineEvidence {
    /// Sources admitted as outline candidates by colour and joins, with the card variant.
    pub admitted: BTreeMap<String, &'static str>,
    /// Sources that are thick by their lineweight (no junction needed, like a thick layer).
    pub thick_by_weight: BTreeSet<String>,
    /// Sources that are thin by their lineweight (not outline candidates).
    pub thin_by_weight: BTreeSet<String>,
}

/// Roles under which a source may also be an outline (GB/T 14665-2012 4.3: an outline used as
/// an extension line is drawn once and serves both).
const SHARED_ROLES: [&str; 2] = ["extension_line", "extension_origin"];

/// Adjust the outline candidates of `structure()` by the drawing's own width evidence and admit
/// the lines that join the outline although their layer or colour says thin.
pub(super) fn outline_candidates(
    cx: &Ctx,
    b: &Builder,
    cand: &mut BTreeMap<String, u8>,
    breaks: &BTreeSet<String>,
    cover: &topology::Cover,
) -> OutlineEvidence {
    let w = Widths::of(cx);
    let mut out = OutlineEvidence::default();
    if w.weights_decide() {
        for (h, prec) in cand.iter_mut() {
            if w.weight(h) == Some(Width::Thick) && *prec < 2 {
                *prec = 2;
                out.thick_by_weight.insert(h.clone());
            }
        }
        let thin: Vec<String> = cand
            .keys()
            .filter(|h| w.weight(h) == Some(Width::Thin))
            .cloned()
            .collect();
        for h in thin {
            cand.remove(&h);
            out.thin_by_weight.insert(h);
        }
    }
    let Some(thick) = w.thick_colour else {
        return out;
    };
    // The pool: continuous root curves that would be outline candidates but for their colour or
    // their table-6 layer, used by no annotation except as a shared extension line.
    let mut pool: BTreeMap<String, &'static str> = BTreeMap::new();
    for (h, r) in &cx.rec {
        if !matches!(r.kind.as_str(), "LINE" | "ARC" | "LWPOLYLINE" | "POLYLINE")
            || cand.contains_key(h)
            || cx.polys.get(h).is_some_and(|p| p.1)
            || cx.arrows.contains_key(h)
            || cx.inline_arrows.contains_key(h)
            || exclusive(b, h)
        {
            continue;
        }
        let st = &cx.style[h];
        if st.pattern != Pattern::Continuous || st.byblock || not_plotted(cx, &st.layer) {
            continue;
        }
        let roles_ok = b.by_source.get(h).is_none_or(|ms| {
            ms.iter()
                .all(|&m| SHARED_ROLES.contains(&b.members[m].member_role.as_str()))
        });
        if !roles_ok {
            continue;
        }
        let thin_layer = matches!(st.layer.as_str(), "08" | "09" | "10" | "11" | "12");
        let thin_explicit = st
            .explicit_color
            .is_some_and(|c| cx.thin_colors.contains(&c.abs()));
        if thin_layer && !thin_explicit && st.color == thick {
            pool.insert(h.clone(), "outline_colour_on_thin_layer");
        } else if thin_explicit
            && !thin_layer
            && matches!(cx.lt(h), "thick_continuous" | "continuous")
        {
            pool.insert(h.clone(), "outline_in_thin_colour");
        }
    }
    if pool.is_empty() {
        return out;
    }
    // Junction parameters of each pooled source with the curves they meet, and whether the other
    // curve meets it with an end (a corner rather than a T-junction).
    let mut meets: BTreeMap<&str, Vec<(f64, String)>> = BTreeMap::new();
    let mut corner: BTreeSet<(&str, i64)> = BTreeSet::new();
    let mut open: BTreeSet<(&str, i64)> = BTreeSet::new();
    let end_key = |t: f64| (t * 1e6).round() as i64;
    let axis = |id: &str| {
        cx.style
            .get(id)
            .is_some_and(|s| s.pattern == Pattern::DashDot)
    };
    #[allow(clippy::manual_range_contains)]
    let at_end = |t: Option<f64>| t.is_some_and(|x| x < 1e-6 || x > 1. - 1e-6);
    for rel in &cx.src.relations {
        let from = rel.from_id.as_str();
        if rel.kind == "open_endpoint" && pool.contains_key(from) {
            if let Some(t) = rel.from_parameter {
                open.insert((from, end_key(t)));
            }
            continue;
        }
        if !matches!(rel.kind.as_str(), "endpoint_on_curve" | "interior_crossing") {
            continue;
        }
        let Some(to) = rel.to_id.as_deref() else {
            continue;
        };
        if let (Some(t), true) = (rel.from_parameter, pool.contains_key(from)) {
            let other = if rel.kind == "endpoint_on_curve" && axis(to) {
                "@axis".to_string()
            } else {
                to.to_string()
            };
            if rel.kind == "endpoint_on_curve" && at_end(rel.to_parameter) && other != "@axis" {
                corner.insert((from, end_key(t)));
            }
            meets.entry(from).or_default().push((t, other));
        }
        if let (Some(t), true) = (rel.to_parameter, pool.contains_key(to)) {
            if rel.kind == "endpoint_on_curve" && at_end(Some(t)) {
                corner.insert((to, end_key(t)));
            }
            meets.entry(to).or_default().push((t, from.to_string()));
        }
    }
    let gap = join_gap(cx);
    // An end without a junction may be explained like any outline end (`topology::interruption`):
    // broken for a text or a symbol facing a collinear piece, or inside a local section.
    let mut joined_ends: BTreeSet<(&str, i64)> = BTreeSet::new();
    let structural = |k: &str| cand.contains_key(k) || pool.contains_key(k) || breaks.contains(k);
    for rel in &cx.src.relations {
        if !matches!(rel.kind.as_str(), "endpoint_on_curve" | "interior_crossing") {
            continue;
        }
        let Some(to) = rel.to_id.as_deref() else {
            continue;
        };
        if let Some(t) = rel.from_parameter.filter(|_| structural(to)) {
            joined_ends.insert((rel.from_id.as_str(), end_key(t)));
        }
        if let Some(t) = rel.to_parameter.filter(|_| structural(&rel.from_id)) {
            joined_ends.insert((to, end_key(t)));
        }
    }
    let partners: Vec<topology::Partner> = cand
        .keys()
        .chain(pool.keys())
        .filter_map(|h| {
            let (a, c) = cx.lines.get(h)?;
            Some(topology::Partner {
                handle: h.clone(),
                ends: [*a, *c],
                t: [0., 1.],
                free: [
                    !joined_ends.contains(&(h.as_str(), end_key(0.))),
                    !joined_ends.contains(&(h.as_str(), end_key(1.))),
                ],
            })
        })
        .collect();
    let explained: BTreeMap<&str, Vec<f64>> = pool
        .keys()
        .filter_map(|h| {
            let (a, c) = cx.lines.get(h)?;
            let v: Vec<f64> = [(0., *a, unit(sub(*a, *c))), (1., *c, unit(sub(*c, *a)))]
                .into_iter()
                .filter(|(t, p, d)| {
                    !joined_ends.contains(&(h.as_str(), end_key(*t)))
                        && topology::interruption(cx, cover, h, *p, *d, &partners).is_some()
                })
                .map(|(t, ..)| t)
                .collect();
            (!v.is_empty()).then_some((h.as_str(), v))
        })
        .collect();
    let ends_of = |h: &str| -> Option<[P; 2]> {
        if let Some((a, c)) = cx.lines.get(h) {
            return Some([*a, *c]);
        }
        if let Some((pts, _, _)) = cx.polys.get(h) {
            return Some([*pts.first()?, *pts.last()?]);
        }
        let (c0, r0, s0, sw, _) = cx.arc(h)?;
        Some([
            add(c0, mul([s0.cos(), s0.sin()], r0)),
            add(c0, mul([(s0 + sw).cos(), (s0 + sw).sin()], r0)),
        ])
    };
    let near = |p: P, k: &str| {
        cx.lines
            .get(k)
            .is_some_and(|(a, c)| on_seg(p, *a, *c, gap).is_some())
            || cx.polys.get(k).is_some_and(|(pts, _, _)| {
                pts.windows(2).any(|w| on_seg(p, w[0], w[1], gap).is_some())
            })
            || cx
                .arc(k)
                .is_some_and(|(c0, r0, s0, sw, full)| on_arc(p, c0, r0, s0, sw, full, gap))
    };
    // Ends of a pooled source that meet the other curves of `with` (by incidence or within the
    // drafting gap): the junction parameters, and whether one of them is a settled outline.
    let joins = |h: &str, with: &BTreeSet<String>| -> (Vec<f64>, bool) {
        let partner = |k: &str| {
            k == "@axis"
                || (k != h && (cand.contains_key(k) || breaks.contains(k) || with.contains(k)))
        };
        let mut ts: Vec<f64> = explained.get(h).cloned().unwrap_or_default();
        let mut grounded = false;
        for (t, k) in meets.get(h).into_iter().flatten() {
            if partner(k) {
                ts.push(*t);
                grounded |= cand.contains_key(k.as_str());
            }
        }
        if let Some(e) = ends_of(h) {
            for (t, p) in [(0., e[0]), (1., e[1])] {
                if ts.iter().any(|x| (x - t).abs() < 1e-6) {
                    continue;
                }
                if let Some(k) = cand
                    .keys()
                    .chain(breaks.iter())
                    .chain(with.iter())
                    .find(|k| k.as_str() != h && near(p, k))
                {
                    ts.push(t);
                    grounded |= cand.contains_key(k.as_str());
                }
            }
        }
        (ts, grounded)
    };
    // The largest set of pooled sources each of which joins the outline or the set as an outline
    // piece does: a line not used by any annotation at two places, a used line (a shared extension
    // line) and an arc at both ends. Chains with a free end (leaders, extension lines) fall out.
    let mut set: BTreeSet<String> = pool.keys().cloned().collect();
    loop {
        let before = set.len();
        let keep: BTreeSet<String> = set
            .iter()
            .filter(|h| {
                let (mut ts, _) = joins(h, &set);
                let at = |t: f64| ts.iter().any(|x| (x - t).abs() < 1e-6);
                if cx.lines.contains_key(h.as_str()) && !b.used(h) {
                    ts.sort_by(f64::total_cmp);
                    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
                    // two junctions, or an outline drawn in the outline colour continued from a
                    // corner to a free end where the view stops (as for any outline piece in
                    // `structure()`); a thin-coloured line from a corner to a free end is an
                    // extension line (card visible_edge_line, counter-example)
                    ts.len() >= 2
                        || ts.len() == 1
                            && pool[h.as_str()] == "outline_colour_on_thin_layer"
                            && [(0., 1.), (1., 0.)].iter().any(|(j, f)| {
                                (ts[0] - j).abs() < 1e-6
                                    && corner.contains(&(h.as_str(), end_key(*j)))
                                    && open.contains(&(h.as_str(), end_key(*f)))
                            })
                } else {
                    at(0.) && at(1.)
                }
            })
            .cloned()
            .collect();
        set = keep;
        if set.len() == before {
            break;
        }
    }
    // Each connected group of them must meet the settled outline somewhere: a closed figure of
    // such lines standing apart from the part is not its outline.
    let mut grounded: BTreeSet<String> = set.iter().filter(|h| joins(h, &set).1).cloned().collect();
    loop {
        let more: Vec<String> = set
            .iter()
            .filter(|h| {
                !grounded.contains(*h)
                    && meets
                        .get(h.as_str())
                        .is_some_and(|v| v.iter().any(|(_, k)| grounded.contains(k)))
            })
            .cloned()
            .collect();
        if more.is_empty() {
            break;
        }
        grounded.extend(more);
    }
    for h in &grounded {
        cand.insert(h.clone(), 1);
        out.admitted.insert(h.clone(), pool[h]);
    }
    out
}

/// A layer whose content is never plotted (DC-G1-101): AutoCAD's definition-point layer, or a
/// layer whose plot flag (group 290) is off. Its lines are no evidence of what the sheet shows.
pub(super) fn not_plotted(cx: &Ctx, layer: &str) -> bool {
    layer.eq_ignore_ascii_case("defpoints")
        || cx.d.records.iter().any(|r| {
            r.kind == "LAYER"
                && r.get(2) == Some(layer)
                && r.get(290).is_some_and(|v| v.trim() == "0")
        })
}

/// A straight line whose two ends are corners of the outline (each meets another structural line
/// at that line's end) is a chamfer or edge line, not a section line drawn inside the region (card
/// `section_line_region`, counter-example: a single line joining two outline pieces at a corner).
pub(super) fn corner_to_corner(cx: &Ctx, h: &str) -> bool {
    let mut ends = [false, false];
    #[allow(clippy::manual_range_contains)]
    for r in cx
        .src
        .relations
        .iter()
        .filter(|r| r.kind == "endpoint_on_curve" && r.from_id == h)
    {
        if let (Some(t), Some(tp), Some(to)) =
            (r.from_parameter, r.to_parameter, r.to_id.as_deref())
            && (tp < 1e-6 || tp > 1. - 1e-6)
            && cx.structural_style(to)
            && cx.lines.contains_key(to)
        {
            ends[(t > 0.5) as usize] = true;
        }
    }
    ends == [true, true]
}

/// Distance from `p` to the segment `ab`.
fn lines_seg_dist(p: P, a: P, b: P) -> f64 {
    let v = sub(b, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), v) / n).clamp(0., 1.);
    dist(p, add(a, mul(v, t)))
}

/// The point `p` lies on the arc (or circle) within `tol`.
fn on_arc(p: P, c0: P, r0: f64, s0: f64, sw: f64, full: bool, tol: f64) -> bool {
    let ang = ((p[1] - c0[1]).atan2(p[0] - c0[0]) - s0).rem_euclid(std::f64::consts::TAU);
    (dist(p, c0) - r0).abs() < tol
        && (full
            || ang <= sw + tol / r0.max(1e-9)
            || std::f64::consts::TAU - ang < tol / r0.max(1e-9))
}

/// Another candidate arc (or circle) passing within the drafting gap of the end `p` of arc `own`
/// (card `outline_joins`, variant `arc_to_arc_gap`): arcs drawn tangent end to end rarely meet
/// within the incidence tolerance of the usage graph.
pub(super) fn arc_end_near_arc(
    cx: &Ctx,
    cand: &BTreeMap<String, u8>,
    own: &str,
    p: P,
    gap: f64,
) -> Option<String> {
    cand.keys()
        .filter(|k| k.as_str() != own && matches!(cx.kind(k), "ARC" | "CIRCLE"))
        .find(|k| {
            cx.arc(k)
                .is_some_and(|(c0, r0, s0, sw, full)| on_arc(p, c0, r0, s0, sw, full, gap))
        })
        .cloned()
}

/// Drafting gap below which two outline ends join (card `outline_joins`): a fraction of the text
/// height, so it is the same invisible distance on paper at any drawing scale.
pub(super) fn join_gap(cx: &Ctx) -> f64 {
    let k = representation::card_ref("outline_joins")
        .ok()
        .and_then(|c| representation::param(c, "join_gap").ok());
    match k {
        Some(k) => (k * cx.scale).max(0.05),
        None => 0.05 * (cx.scale / 3.5).max(1.),
    }
}

/// A broken-pattern line used so far only as an extension line (or its origin) by dimensions: the
/// dimension borrows the axis or centre line, which keeps its own line type (GB/T 4458.4: axes and
/// centre lines may serve as extension lines; card `dimension_line_parts`, variant
/// `axis_as_extension_line`).
pub(super) fn lent_to_dimension(b: &Builder, h: &str) -> bool {
    b.by_source.get(h).is_some_and(|ms| {
        !ms.is_empty()
            && ms
                .iter()
                .all(|&m| SHARED_ROLES.contains(&b.members[m].member_role.as_str()))
    })
}

/// The card and variant evidence of a variant of `line_width_evidence` or `outline_joins`.
pub(super) fn naming(variant: &str) -> Vec<String> {
    let card = match variant {
        "arc_to_arc_gap" | "end_on_thread_circle" => "outline_joins",
        _ => "line_width_evidence",
    };
    vec![format!("card:{card}"), format!("variant:{variant}")]
}

/// The source meets a thread's three-quarter circle (an outline interrupted by a tapped hole).
pub(super) fn ends_on_thread(cx: &Ctx, h: &str, threads: &BTreeSet<String>) -> bool {
    !threads.is_empty()
        && cx.src.relations.iter().any(|r| {
            r.kind == "endpoint_on_curve"
                && ((r.from_id == h && r.to_id.as_deref().is_some_and(|t| threads.contains(t)))
                    || (r.to_id.as_deref() == Some(h) && threads.contains(&r.from_id)))
        })
}

/// Thin structural line candidates for `structure()`'s thin-line rule: thin continuous by the
/// checked layer convention on any table-6 layer, or thin by the drawing's lineweights.
pub(super) fn thin_structural(cx: &Ctx, ev: &OutlineEvidence, h: &str) -> bool {
    cx.style
        .get(h)
        .is_some_and(|s| s.pattern == Pattern::Continuous && !s.byblock)
        && (cx.lt(h) == "thin_continuous" || ev.thin_by_weight.contains(h))
}

// ---------------------------------------------------------------- section lines by their own spacing

/// Thin by the drawing's evidence: thin by the checked convention or an explicit thin colour, or
/// any continuous line where no convention decides (`annotation_style`), or thin by lineweight.
fn thin_line(cx: &Ctx, w: &Widths, h: &str) -> bool {
    !w.thick(cx, h)
        && (cx.annotation_style(h) || lines::thin_colour(cx, h) || w.weight(h) == Some(Width::Thin))
}

/// Section lines drawn as separate thin lines (card `section_line_region`, variant
/// `loose_lines_by_own_spacing`): in one direction, parallel lines whose offsets across that
/// direction step by one spacing (each line may be cut into pieces by a hole or a void), three
/// steps or more, neighbours overlapping along the direction. Spacing and length come from the
/// lines themselves, never from the text height (an enlarged view enlarges the spacing).
pub(super) fn section_lines_by_spacing(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("section_line_region")?;
    let par = representation::param(c, "parallel_deg")?;
    let step_tol = representation::param(c, "spacing_tolerance")?;
    let min_lines = representation::param(c, "min_lines")? as usize;
    let w = Widths::of(cx);
    let mut thin: Vec<(String, P, P, f64)> = cx
        .lines
        .iter()
        .filter(|(h, (a, z))| {
            dist(*a, *z) > 1e-6 && !b.used(h) && !exclusive(b, h) && thin_line(cx, &w, h)
        })
        .map(|(h, (a, z))| {
            let ang = (z[1] - a[1])
                .atan2(z[0] - a[0])
                .to_degrees()
                .rem_euclid(180.);
            (h.clone(), *a, *z, ang)
        })
        .filter(|x| (x.3 - 0.).abs() > 5. && (x.3 - 90.).abs() > 5. && (x.3 - 180.).abs() > 5.)
        .collect();
    thin.sort_by(|x, y| x.3.total_cmp(&y.3).then(x.0.cmp(&y.0)));
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut i = 0;
    while i < thin.len() {
        // one direction: angles within the parallel tolerance of the first
        let a0 = thin[i].3;
        let mut j = i;
        while j < thin.len() && thin[j].3 - a0 <= par {
            j += 1;
        }
        let dir: Vec<&(String, P, P, f64)> = thin[i..j].iter().collect();
        i = j;
        if dir.len() < min_lines {
            continue;
        }
        let ang = dir.iter().map(|x| x.3).sum::<f64>() / dir.len() as f64;
        let u = [ang.to_radians().cos(), ang.to_radians().sin()];
        // offset across the direction and extent along it
        let mut rows: Vec<(f64, f64, f64, &String)> = dir
            .iter()
            .map(|(h, a, z, _)| {
                let (s0, s1) = (dot(*a, u), dot(*z, u));
                (cross(u, mid(*a, *z)), s0.min(s1), s0.max(s1), h)
            })
            .collect();
        rows.sort_by(|x, y| x.0.total_cmp(&y.0));
        // distinct offsets (pieces of one section line share it)
        let mut lanes: Vec<(f64, Vec<usize>)> = Vec::new();
        for (k, r) in rows.iter().enumerate() {
            match lanes.last_mut() {
                Some(l) if (r.0 - l.0).abs() <= 1e-3 * (1. + r.0.abs()).min(1.) => l.1.push(k),
                _ => lanes.push((r.0, vec![k])),
            }
        }
        let overlap = |p: &[usize], q: &[usize], slack: f64| {
            p.iter().any(|&x| {
                q.iter()
                    .any(|&y| rows[x].1 <= rows[y].2 + slack && rows[y].1 <= rows[x].2 + slack)
            })
        };
        // runs of lanes stepping by one spacing
        let mut k = 0;
        while k + 1 < lanes.len() {
            let s = lanes[k + 1].0 - lanes[k].0;
            if s <= 1e-6 || !overlap(&lanes[k].1, &lanes[k + 1].1, s) {
                k += 1;
                continue;
            }
            let mut end = k + 1;
            while end + 1 < lanes.len() {
                let d = lanes[end + 1].0 - lanes[end].0;
                if (d - s).abs() > step_tol * s || !overlap(&lanes[end].1, &lanes[end + 1].1, s) {
                    break;
                }
                end += 1;
            }
            if end - k + 1 >= min_lines {
                let members: Vec<&String> = lanes[k..=end]
                    .iter()
                    .flat_map(|l| l.1.iter().map(|&x| rows[x].3))
                    .collect();
                // a zero-length piece on one of the run's lines, inside its extent, joins it
                let (lo, hi) = (
                    lanes[k..=end]
                        .iter()
                        .flat_map(|l| l.1.iter().map(|&x| rows[x].1))
                        .fold(f64::INFINITY, f64::min),
                    lanes[k..=end]
                        .iter()
                        .flat_map(|l| l.1.iter().map(|&x| rows[x].2))
                        .fold(f64::NEG_INFINITY, f64::max),
                );
                let points: Vec<String> = cx
                    .lines
                    .iter()
                    .filter(|(h, (a, z))| {
                        dist(*a, *z) <= 1e-6
                            && !b.used(h)
                            && !taken.contains(*h)
                            && lanes[k..=end]
                                .iter()
                                .any(|l| (cross(u, *a) - l.0).abs() <= 1e-3)
                            && (lo..=hi).contains(&dot(*a, u))
                    })
                    .map(|(h, _)| h.clone())
                    .collect();
                let e = b.expr(
                    "section_hatch",
                    &format!("{ang:.0}°"),
                    "uniform_parallel_thin_oblique_lines",
                    "candidate",
                    vec![],
                    vec![
                        format!("lines:{}", members.len()),
                        format!("spacing:{s:.3}"),
                        "card:section_line_region".into(),
                        "variant:loose_lines_by_own_spacing".into(),
                    ],
                    vec!["Sectioned material region boundary not resolved".into()],
                );
                for h in members.into_iter().cloned().chain(points) {
                    if taken.insert(h.clone()) {
                        b.member(
                            e,
                            &h,
                            None,
                            [0., 1.],
                            "hatch_fill",
                            member_line_type(cx.lt(&h), "hatch_fill"),
                            "candidate",
                            vec![],
                            vec![],
                        );
                    }
                }
                k = end + 1;
            } else {
                k += 1;
            }
        }
    }
    Ok(())
}

/// Short thin strokes left in a hatched region along its pattern direction (a hatch remnant: inside
/// it or within a stroke's length of its boundary) join the hatch (card `section_line_region`,
/// variant `remnant_stroke`). A stroke joining two outline pieces at its ends is a chamfer edge.
pub(super) fn hatch_remnants(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("section_line_region")?;
    let atol = representation::param(c, "angle_tolerance_deg")?;
    let lmax = representation::param(c, "remnant_length_max")? * cx.scale.max(1e-6);
    let w = Widths::of(cx);
    for (hh, r) in &cx.rec {
        if r.kind != "HATCH" || int(r, 70) != 0 {
            continue;
        }
        let Ok(ang) = r.number(52) else {
            continue;
        };
        let segs = representation::hatch_boundary(r);
        if segs.len() < 3 {
            continue;
        }
        let Some(e) = b
            .by_source
            .get(hh)
            .and_then(|ms| {
                ms.iter()
                    .map(|&m| &b.members[m])
                    .find(|m| m.member_role == "hatch_fill")
            })
            .and_then(|m| b.exprs.iter().position(|x| x.id == m.expression_id))
        else {
            continue;
        };
        let u = [ang.to_radians().cos(), ang.to_radians().sin()];
        let near_region = |x: P| {
            representation::inside_segments(x, &segs)
                || segs.iter().any(|(a, z)| lines_seg_dist(x, *a, *z) <= lmax)
        };
        let strokes: Vec<String> = cx
            .lines
            .iter()
            .filter(|(h, (p, q))| {
                let l = dist(*p, *q);
                l > 1e-9
                    && l <= lmax
                    && !b.used(h)
                    && cx.style[*h].pattern == Pattern::Continuous
                    && thin_line(cx, &w, h)
                    && cross(unit(sub(*q, *p)), u).abs() <= atol.to_radians().sin()
                    && near_region(mid(*p, *q))
                    && !corner_to_corner(cx, h)
            })
            .map(|(h, _)| h.clone())
            .collect();
        for h in strokes {
            b.member(
                e,
                &h,
                None,
                [0., 1.],
                "hatch_fill",
                member_line_type(cx.lt(&h), "hatch_fill"),
                "candidate",
                vec![],
                vec![
                    "card:section_line_region".into(),
                    "variant:remnant_stroke".into(),
                ],
            );
            for x in ["card:section_line_region", "variant:remnant_stroke"] {
                if !b.exprs[e].evidence_ids.iter().any(|y| y == x) {
                    b.exprs[e].evidence_ids.push(x.into());
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- holes named by a count × diameter callout

/// Count and diameter of a callout such as `n×Øx` or the older `n-Øx` (separators and the
/// diameter sign from card `dimension_text`).
fn count_diameter(dt: &oe_contracts::MdRepresentationCard, raw: &str) -> Option<(usize, f64)> {
    let vals = |concept: &str, entry: &str| -> Vec<&str> {
        dt.lexicon
            .iter()
            .flatten()
            .filter(|l| l.concept == concept && l.entry == entry)
            .flat_map(|l| l.values.iter().map(String::as_str))
            .collect()
    };
    let text: String = tx::decode(raw)
        .text
        .replace(['φ', 'Φ', '⌀'], "Ø")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let digits = text.chars().take_while(|c| c.is_ascii_digit()).count();
    let n: usize = text[..digits].parse().ok()?;
    let rest = &text[digits..];
    let sep = vals("DC-G4-094", "count_separator")
        .into_iter()
        .find(|s| rest.starts_with(*s))?;
    let rest = &rest[sep.len()..];
    let sign = vals("DC-G4-055", "prefix")
        .into_iter()
        .filter(|p| !p.starts_with('S'))
        .find(|p| rest.starts_with(*p))?;
    let rest = &rest[sign.len()..];
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let x: f64 = num.parse().ok()?;
    (n >= 2 && x > 0.).then_some((n, x))
}

/// A free circle: handle, centre, radius.
type Circle = (String, P, f64);

/// Centres of `n` circles evenly spaced on one pitch circle: the pitch radius.
fn evenly_spaced(centres: &[P], rtol: f64, atol_deg: f64) -> Option<f64> {
    let n = centres.len();
    if n < 3 {
        return None;
    }
    let o = mul(
        centres.iter().fold([0., 0.], |s, p| add(s, *p)),
        1. / n as f64,
    );
    let rs: Vec<f64> = centres.iter().map(|p| dist(*p, o)).collect();
    let r = rs.iter().sum::<f64>() / n as f64;
    if r <= 1e-9 || rs.iter().any(|x| (x - r).abs() > rtol * r) {
        return None;
    }
    let mut angs: Vec<f64> = centres
        .iter()
        .map(|p| {
            (p[1] - o[1])
                .atan2(p[0] - o[0])
                .to_degrees()
                .rem_euclid(360.)
        })
        .collect();
    angs.sort_by(f64::total_cmp);
    let step = 360. / n as f64;
    let ok = (0..n).all(|i| {
        let d = if i + 1 < n {
            angs[i + 1] - angs[i]
        } else {
            angs[0] + 360. - angs[i]
        };
        (d - step).abs() <= atol_deg
    });
    ok.then_some(r)
}

/// Equal circles named by a count × diameter callout are hole outlines (card `hole_pattern`): the
/// callout's terminators stand on one circle of that diameter on opposite sides of its centre;
/// that circle and the other equal circles evenly spaced with it on one pitch circle are the
/// holes; the same pattern drawn again (another circular view) is the same holes. Their layer
/// and colour do not decide.
pub(super) fn hole_patterns(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("hole_pattern")?;
    let dt = representation::card_ref("dimension_text")?;
    let dtol = representation::param(c, "diameter_tolerance")?;
    let ptol = representation::param(c, "pitch_tolerance")?;
    let atol = representation::param(c, "spacing_tolerance_deg")?;
    let ontol = representation::param(c, "terminator_on_circle")? * cx.scale.max(1e-6);
    let free: Vec<Circle> = cx
        .rec
        .iter()
        .filter(|(h, r)| r.kind == "CIRCLE" && !b.used(h))
        .filter_map(|(h, _)| cx.arc(h).map(|(c0, r0, ..)| (h.clone(), c0, r0)))
        .collect();
    let mut done: BTreeSet<String> = BTreeSet::new();
    for (ei, e) in b.exprs.clone().iter().enumerate() {
        let ms: Vec<&MdExpressionMember> = b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id)
            .collect();
        let texts: Vec<String> = std::iter::once(e.label.clone())
            .chain(ms.iter().filter_map(|m| {
                matches!(m.member_role.as_str(), "dimension_text" | "note_text")
                    .then(|| cx.rec.get(&m.source_handle).map(|r| dd::raw_text(r)))
                    .flatten()
            }))
            .collect();
        let Some((n, x)) = texts.iter().find_map(|t| count_diameter(dt, t)) else {
            continue;
        };
        let tips: Vec<P> = ms
            .iter()
            .filter(|m| m.member_role == "arrowhead")
            .filter_map(|m| cx.arrow(&m.source_handle).map(|a| a.tip))
            .collect();
        let r = x / 2.;
        // the circle the callout spans
        let Some(spanned) = free.iter().find(|(h, c0, r0)| {
            !done.contains(h)
                && (r0 - r).abs() <= dtol * r
                && tips.iter().any(|p| {
                    (dist(*p, *c0) - r0).abs() <= ontol
                        && tips.iter().any(|q| {
                            (dist(*q, *c0) - r0).abs() <= ontol
                                && dot(sub(*p, *c0), sub(*q, *c0)) < 0.
                        })
                })
        }) else {
            continue;
        };
        let equal: Vec<&Circle> = free
            .iter()
            .filter(|(h, _, r0)| !done.contains(h) && (r0 - spanned.2).abs() <= dtol * spanned.2)
            .collect();
        // the spanned circle's pattern: it and its n - 1 nearest equal circles
        let group_of = |seed: &Circle| -> Vec<&Circle> {
            let mut g: Vec<&Circle> = equal.to_vec();
            g.sort_by(|p, q| {
                dist(p.1, seed.1)
                    .total_cmp(&dist(q.1, seed.1))
                    .then(p.0.cmp(&q.0))
            });
            g.truncate(n);
            g
        };
        let first = group_of(spanned);
        let pitch = (first.len() == n)
            .then(|| evenly_spaced(&first.iter().map(|x| x.1).collect::<Vec<_>>(), ptol, atol))
            .flatten();
        let mut groups: Vec<(Vec<&Circle>, &'static str)> = Vec::new();
        match pitch {
            Some(pr) => {
                groups.push((first.clone(), "count_diameter_callout"));
                // the same pattern drawn again
                let mut used: BTreeSet<&str> = first.iter().map(|x| x.0.as_str()).collect();
                for seed in &equal {
                    if used.contains(seed.0.as_str()) {
                        continue;
                    }
                    let g = group_of(seed);
                    if g.len() == n
                        && g.iter().all(|x| !used.contains(x.0.as_str()))
                        && evenly_spaced(&g.iter().map(|x| x.1).collect::<Vec<_>>(), ptol, atol)
                            .is_some_and(|r2| (r2 - pr).abs() <= ptol * pr)
                    {
                        used.extend(g.iter().map(|x| x.0.as_str()));
                        groups.push((g, "same_pattern_in_another_view"));
                    }
                }
            }
            None => groups.push((vec![spanned], "count_diameter_callout")),
        }
        for (g, variant) in groups {
            for (h, _, _) in g {
                done.insert(h.clone());
                let ex = b.expr(
                    "visible_contour",
                    h,
                    "piece:closed_or_connected_curve",
                    "candidate",
                    vec![],
                    vec![
                        "card:hole_pattern".into(),
                        format!("variant:{variant}"),
                        format!("callout:{}", b.exprs[ei].id),
                        format!("count:{n}"),
                    ],
                    vec![],
                );
                b.member(
                    ex,
                    h,
                    None,
                    [0., 1.],
                    "contour_segment",
                    "thick_continuous",
                    "candidate",
                    vec![],
                    vec![],
                );
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- thread end view below the usual ratio

/// The full circle of a thread end view whose radius ratio to the thin three-quarter arc lies
/// below the card's usual window but within its wider one (card `thread_representation`, variant
/// `end_view_wide_ratio`), and the callout spanning the hole when there is one. A diameter
/// dimension spanning the hole (a smooth hole) contradicts the reading.
pub(super) fn wide_thread_partner(
    cx: &Ctx,
    b: &Builder,
    h: &str,
    c: P,
    rad: f64,
) -> Option<(String, Vec<String>)> {
    let card = representation::card_ref("thread_representation").ok()?;
    let lo = representation::param(card, "end_view_ratio_wide_min").ok()?;
    let hi = representation::param(card, "end_view_ratio_min").ok()?;
    let w = Widths::of(cx);
    if !thin_line(cx, &w, h) && cx.lt(h) != "thin_continuous" {
        return None;
    }
    let partner = cx
        .rec
        .keys()
        .filter(|k| k.as_str() != h && matches!(cx.kind(k), "CIRCLE" | "ARC"))
        .find_map(|k| {
            let (c2, r2, _, sw2, _) = cx.arc(k)?;
            let q = r2 / rad;
            (dist(c2, c) < 1e-4 * rad.max(1.) && sw2.to_degrees() > 300. && q >= lo && q < hi)
                .then(|| (k.clone(), r2))
        })?;
    // A dimension or callout spanning the hole: two terminators on the arc's or the circle's
    // circumference on opposite sides of the centre.
    let tol = 0.05 * cx.scale.max(1e-6);
    let on = |p: P| {
        [(rad), (partner.1)]
            .iter()
            .any(|r| (dist(p, c) - r).abs() <= tol)
    };
    let mut ev = vec![
        "card:thread_representation".to_string(),
        "variant:end_view_wide_ratio".to_string(),
        format!("ratio:{:.3}", partner.1 / rad),
    ];
    for e in &b.exprs {
        let tips: Vec<P> = b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id && m.member_role == "arrowhead")
            .filter_map(|m| cx.arrow(&m.source_handle).map(|a| a.tip))
            .filter(|p| on(*p))
            .collect();
        let spans = tips
            .iter()
            .any(|p| tips.iter().any(|q| dot(sub(*p, c), sub(*q, c)) < 0.));
        if !spans {
            continue;
        }
        if e.kind == "thread_note" || tx::lex(&e.label) == Lex::Thread {
            ev.push(format!("thread_callout:{}", e.id));
        } else if e.kind == "dimension_diameter" {
            // a plain diameter spanning the hole: a smooth hole, not a thread
            return None;
        }
    }
    Some((partner.0, ev))
}

// ---------------------------------------------------------------- outlines broken for a symbol

/// Annotation symbols drawn as blocks (surface texture, datum, tolerance frame, view symbols):
/// their strokes in drawing coordinates, their extent and handle (card `annotation_break`).
pub(super) fn annotation_symbols(cx: &Ctx, b: &Builder) -> Vec<topology::Symbol> {
    let kinds: BTreeMap<&str, &str> = b
        .exprs
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let annotation = |k: &str| {
        matches!(
            k,
            "surface_roughness" | "datum_symbol" | "geometric_tolerance" | "view_callout"
        ) || k.starts_with("gps_")
    };
    let mut out = Vec::new();
    for (h, s) in &cx.shapes {
        let used = b.by_source.get(h).is_some_and(|ms| {
            ms.iter().any(|&m| {
                kinds
                    .get(b.members[m].expression_id.as_str())
                    .is_some_and(|k| annotation(k))
            })
        });
        if !used {
            continue;
        }
        let mut strokes: Vec<(P, P)> = s.lines.clone();
        for path in &s.paths {
            strokes.extend(path.windows(2).map(|w| (w[0], w[1])));
        }
        let mut pts: Vec<P> = strokes.iter().flat_map(|(a, c)| [*a, *c]).collect();
        pts.extend(s.texts.iter().map(|t| t.1));
        let Some(bb) = bbox_of(&pts) else {
            continue;
        };
        let size = (bb[2] - bb[0]).hypot(bb[3] - bb[1]);
        if !strokes.is_empty() {
            out.push((strokes, size, h.clone()));
        }
    }
    out
}

// ---------------------------------------------------------------- internal thread in a section

/// The length of the first dash a broken line prints with (its pattern's longest mark times the
/// drawing's and the entity's linetype scale); a line shorter than that prints as a continuous
/// line (card `thread_representation`, variant `thread_end_line_printed_solid`).
fn printed_first_dash(cx: &Ctx, id: &str) -> Option<f64> {
    let r = cx.rec.get(id)?;
    let st = cx.style.get(id)?;
    let own = r.get(6).filter(|n| !n.eq_ignore_ascii_case("BYLAYER"));
    let name = match own {
        Some(n) => n.to_string(),
        None => {
            cx.d.records
                .iter()
                .find(|x| x.kind == "LAYER" && x.get(2) == Some(st.layer.as_str()))
                .and_then(|x| x.get(6))?
                .to_string()
        }
    };
    let lt =
        cx.d.records.iter().find(|x| {
            x.kind == "LTYPE" && x.get(2).is_some_and(|n| n.eq_ignore_ascii_case(&name))
        })?;
    let mark = lt
        .all(49)
        .iter()
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .fold(0., f64::max);
    let ltscale =
        cx.d.header("$LTSCALE", 40)
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(1.);
    let own_scale = r
        .get(48)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(1.);
    (mark > 0.).then_some(mark * ltscale * own_scale)
}

/// Internal thread in a section (GB/T 4459.1; card `thread_representation`, variants
/// `internal_thread_in_section` and `blind_tapped_hole_in_section`): a pair of thin lines (major
/// diameter) just outside a pair of thick lines (minor diameter), both symmetric about one
/// middle line and overlapping along it. Corroborated by the hole's axis on that middle line, a
/// thick thread end line across the thin pair at its end (a broken line shorter than its first
/// printed dash prints solid), or a section hatch whose boundary runs along a thick line. A blind
/// hole adds the drill bottom across the thick pair beyond the thread end and the drill point.
pub(super) fn thread_sections(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let card = representation::card_ref("thread_representation")?;
    let rmin = representation::param(card, "section_ratio_min")?;
    let rmax = representation::param(card, "section_ratio_max")?;
    let point_lo = representation::param(card, "drill_point_cos_min")?;
    let point_hi = representation::param(card, "drill_point_cos_max")?;
    let w = Widths::of(cx);
    let seg = |h: &str| {
        cx.lines
            .get(h)
            .copied()
            .filter(|(a, z)| dist(*a, *z) > 1e-6)
    };
    let thick: Vec<String> = cx
        .lines
        .keys()
        .filter(|h| seg(h).is_some() && !exclusive(b, h) && w.thick(cx, h))
        .cloned()
        .collect();
    let thick_set: BTreeSet<&str> = thick.iter().map(String::as_str).collect();
    // a major-diameter line runs from the hole's mouth to the thread end: both its ends meet
    // other lines (no free end, unlike an extension line), one of them a thick line
    let ends_on_thick = |h: &str| {
        let mut ends = [false, false];
        let mut thick_end = false;
        for r in cx
            .src
            .relations
            .iter()
            .filter(|r| r.kind == "endpoint_on_curve" && r.from_id == h)
        {
            if let (Some(t), Some(to)) = (r.from_parameter, r.to_id.as_deref()) {
                ends[(t > 0.5) as usize] = true;
                thick_end |= thick_set.contains(to);
            }
        }
        ends == [true, true] && thick_end
    };
    let thin: Vec<String> = cx
        .lines
        .keys()
        .filter(|h| {
            seg(h).is_some()
                && !b.used(h)
                && cx.style[*h].pattern == Pattern::Continuous
                && thin_line(cx, &w, h)
                && ends_on_thick(h)
        })
        .cloned()
        .collect();
    let axes: Vec<String> = cx
        .lines
        .keys()
        .filter(|h| cx.style[*h].pattern == Pattern::DashDot && seg(h).is_some())
        .cloned()
        .collect();
    let hatch_edges: Vec<(P, P)> = cx
        .rec
        .values()
        .filter(|r| r.kind == "HATCH")
        .flat_map(|r| representation::hatch_boundary(r))
        .collect();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (i, t1) in thin.iter().enumerate() {
        let (a1, z1) = seg(t1).unwrap();
        let u = unit(sub(z1, a1));
        let off = |p: P| cross(u, sub(p, a1));
        let along = |p: P| dot(sub(p, a1), u);
        let span = |h: &str| {
            let (a, z) = seg(h).unwrap();
            let (s0, s1) = (along(a), along(z));
            (s0.min(s1), s0.max(s1))
        };
        let parallel = |h: &str| {
            let (a, z) = seg(h).unwrap();
            cross(unit(sub(z, a)), u).abs() < 0.01
        };
        let s1 = span(t1);
        for t2 in &thin[i + 1..] {
            if taken.contains(t1) || taken.contains(t2) || !parallel(t2) {
                continue;
            }
            let (a2, z2) = seg(t2).unwrap();
            let big_d = off(a2);
            if big_d.abs() < 1e-6 || (off(z2) - big_d).abs() > 1e-3 * big_d.abs() {
                continue;
            }
            let s2 = span(t2);
            let ov = s1.1.min(s2.1) - s1.0.max(s2.0);
            if ov < 0.5 * (s1.1 - s1.0).min(s2.1 - s2.0) {
                continue;
            }
            let mid_off = big_d / 2.;
            let tol = (0.02 * big_d.abs()).min(0.05 * cx.scale.max(1.));
            // thick lines inside the thin pair, symmetric about the middle line
            let inside: Vec<(&String, f64)> = thick
                .iter()
                .filter(|k| !taken.contains(*k) && parallel(k))
                .filter_map(|k| {
                    let (a, z) = seg(k).unwrap();
                    let o = off(a) - mid_off;
                    let s = span(k);
                    ((off(z) - off(a)).abs() <= tol
                        && o.abs() < big_d.abs() / 2. - tol
                        && s.1.min(s1.1.max(s2.1)) - s.0.max(s1.0.min(s2.0)) > 0.5 * ov)
                        .then_some((k, o))
                })
                .collect();
            let Some((k1, k2, d)) = inside.iter().find_map(|(k1, o1)| {
                inside.iter().find_map(|(k2, o2)| {
                    let d = (o1 - o2).abs();
                    (k1 < k2
                        && (o1 + o2).abs() <= tol
                        && (rmin..=rmax).contains(&(d / big_d.abs())))
                    .then(|| ((*k1).clone(), (*k2).clone(), d))
                })
            }) else {
                continue;
            };
            // corroboration
            let axis = axes.iter().find(|x| {
                parallel(x) && {
                    let (a, z) = seg(x).unwrap();
                    let s = span(x);
                    (off(a) - mid_off).abs() <= tol
                        && (off(z) - mid_off).abs() <= tol
                        && s.1.min(s1.1) - s.0.max(s1.0) > 0.
                }
            });
            let (lo, hi) = (s1.0.max(s2.0), s1.1.min(s2.1));
            let across = |h: &str, width: f64, at: &[f64]| {
                seg(h).is_some_and(|(a, z)| {
                    let v = unit(sub(z, a));
                    dot(v, u).abs() < 0.01
                        && ((off(a) - mid_off).abs() - width / 2.).abs() <= tol
                        && ((off(z) - mid_off).abs() - width / 2.).abs() <= tol
                        && (off(a) - mid_off) * (off(z) - mid_off) < 0.
                        && at.iter().any(|s| (along(a) - s).abs() <= tol)
                })
            };
            let end_line = cx.lines.keys().find(|h| {
                !exclusive(b, h)
                    && h.as_str() != t1.as_str()
                    && h.as_str() != t2.as_str()
                    && across(h, big_d.abs(), &[lo, hi])
                    && (w.thick(cx, h)
                        || cx.style[*h].pattern != Pattern::Continuous
                            && printed_first_dash(cx, h).is_some_and(|f| {
                                let (a, z) = seg(h).unwrap();
                                dist(a, z) < f
                            }))
            });
            let hatched = [&k1, &k2].iter().any(|k| {
                let (a, z) = seg(k).unwrap();
                hatch_edges.iter().any(|(p, q)| {
                    let (pa, pz) = (project(*p, a, z), project(*q, a, z));
                    pa.1 <= tol && pz.1 <= tol && pa.0.max(pz.0) > 0. && pa.0.min(pz.0) < 1.
                })
            });
            if axis.is_none() && end_line.is_none() && !hatched {
                continue;
            }
            // blind hole: the thick pair runs on past the thread end to a drill bottom and point
            let (k_lo, k_hi) = {
                let (p, q) = (span(&k1), span(&k2));
                (p.0.min(q.0), p.1.max(q.1))
            };
            let deep_end = if k_hi > hi + tol {
                Some(k_hi)
            } else if k_lo < lo - tol {
                Some(k_lo)
            } else {
                None
            };
            let mut extra: Vec<String> = Vec::new();
            if let Some(e) = deep_end {
                for (h, (a, z)) in &cx.lines {
                    if exclusive(b, h)
                        || [t1, t2, &k1, &k2].iter().any(|x| x.as_str() == h.as_str())
                    {
                        continue;
                    }
                    let near_end = |p: P| (along(p) - e).abs() <= tol;
                    let bottom = across(h, d, &[e]);
                    // drill point: from a thick line's end towards the middle line
                    let point = (near_end(*a) || near_end(*z))
                        && w.thick(cx, h)
                        && {
                            let v = unit(sub(*z, *a));
                            let cos = dot(v, u).abs();
                            (point_lo..=point_hi).contains(&cos)
                        }
                        && [*a, *z].iter().any(|p| (off(*p) - mid_off).abs() <= tol)
                        && [*a, *z]
                            .iter()
                            .any(|p| ((off(*p) - mid_off).abs() - d / 2.).abs() <= tol);
                    if bottom || point {
                        extra.push(h.clone());
                    }
                }
            }
            let variant = if deep_end.is_some() {
                "blind_tapped_hole_in_section"
            } else {
                "internal_thread_in_section"
            };
            let mut ev = vec![
                "card:thread_representation".to_string(),
                format!("variant:{variant}"),
                format!("major:{:.4}", big_d.abs()),
                format!("minor:{d:.4}"),
                "GB/T 4459.1".into(),
            ];
            if let Some(x) = axis {
                ev.push(format!("axis:{x}"));
            }
            if hatched {
                ev.push("section_hatch_stops_at_minor".into());
            }
            if let Some(x) = end_line {
                if cx.style[x].pattern != Pattern::Continuous {
                    ev.push(format!("thread_end_line_printed_solid:{x}"));
                } else {
                    ev.push(format!("thread_end_line:{x}"));
                }
            }
            let e = b.expr(
                "thread_convention",
                t1,
                &format!("card:thread_representation/{variant}"),
                "candidate",
                vec![],
                ev,
                vec!["Thread designation and depth of the hole are read from its callout, not from these lines".into()],
            );
            for h in [t1, t2] {
                b.member(
                    e,
                    h,
                    None,
                    [0., 1.],
                    "thread_line",
                    "thin_continuous",
                    "candidate",
                    vec![],
                    vec![],
                );
            }
            for h in [&k1, &k2].into_iter().chain(end_line).chain(extra.iter()) {
                b.member(
                    e,
                    h,
                    None,
                    [0., 1.],
                    "contour_segment",
                    "thick_continuous",
                    "candidate",
                    vec![],
                    vec![],
                );
            }
            for h in [t1, t2, &k1, &k2]
                .into_iter()
                .chain(end_line)
                .chain(extra.iter())
            {
                taken.insert(h.clone());
            }
        }
    }
    Ok(())
}
