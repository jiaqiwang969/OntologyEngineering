//! A parallel lattice is a hatch candidate, not a material region by itself.
//! Keep native patterns; require boundary contacts or a traced closed cut region.
use super::*;

const CARD: &str = "loose_hatch_region_guard";

fn key(m: &MdExpressionMember) -> String {
    m.native_member_id
        .clone()
        .unwrap_or_else(|| m.source_handle.clone())
}

fn owner(cx: &Ctx, source: &str) -> Option<String> {
    cx.rec
        .get(source)
        .and_then(|r| r.get(330))
        .map(str::to_owned)
}

#[derive(Clone)]
struct Stroke {
    key: String,
    owner: Option<String>,
    a: P,
    z: P,
}

enum Shape {
    Segment(P, P),
    Arc(P, f64, f64, f64),
    Circle(P, f64),
}

struct Boundary {
    key: String,
    owner: Option<String>,
    shape: Shape,
}

fn stroke(cx: &Ctx, m: &MdExpressionMember) -> Option<Stroke> {
    let (a, z) = if let Some(path) = &m.native_member_id {
        match &cx.blocks.part(path)?.geom {
            annotation_blocks::Geom::Line(a, z) => (*a, *z),
            _ => return None,
        }
    } else {
        *cx.lines.get(&m.source_handle)?
    };
    Some(Stroke {
        key: key(m),
        owner: owner(cx, &m.source_handle),
        a: lerp(a, z, m.interval[0]),
        z: lerp(a, z, m.interval[1]),
    })
}

fn segment_distance(p: P, a: P, z: P) -> f64 {
    let v = sub(z, a);
    let n = dot(v, v);
    if n <= 1e-18 {
        return dist(p, a);
    }
    dist(p, add(a, mul(v, (dot(sub(p, a), v) / n).clamp(0., 1.))))
}

impl Boundary {
    fn touches(&self, p: P, tol: f64) -> bool {
        match self.shape {
            Shape::Segment(a, z) => segment_distance(p, a, z) <= tol,
            Shape::Circle(c, r) => (dist(p, c) - r).abs() <= tol,
            Shape::Arc(c, r, start, sweep) => {
                let d =
                    ((p[1] - c[1]).atan2(p[0] - c[0]) - start).rem_euclid(std::f64::consts::TAU);
                (dist(p, c) - r).abs() <= tol
                    && (d <= sweep + tol / r.max(tol)
                        || std::f64::consts::TAU - d <= tol / r.max(tol))
            }
        }
    }
}

/// Only real curves can witness a boundary. Text boxes and view boxes never do.
fn context(cx: &Ctx, b: &Builder) -> (Vec<Boundary>, Vec<Stroke>) {
    let kinds: BTreeMap<_, _> = b.exprs.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut owners = BTreeMap::<String, (bool, bool)>::new();
    for m in &b.members {
        let Some(e) = kinds.get(m.expression_id.as_str()) else {
            continue;
        };
        let flags = owners.entry(key(m)).or_default();
        flags.0 |= GEOMETRY.contains(&e.kind.as_str());
        flags.1 |= matches!(
            e.kind.as_str(),
            "visible_contour" | "break_line" | "center_axis"
        ) || (e.kind == "view_callout"
            && e.qualifiers.iter().any(|q| q == "detail_boundary"));
    }
    let widths = line_evidence::Widths::of(cx);
    let mut boundaries = Vec::new();
    let mut thin = Vec::new();
    let mut path = |id: &str, source: &str, pts: &[P], closed: bool| {
        for w in pts.windows(2) {
            boundaries.push(Boundary {
                key: id.into(),
                owner: owner(cx, source),
                shape: Shape::Segment(w[0], w[1]),
            });
        }
        if closed && pts.len() > 2 {
            boundaries.push(Boundary {
                key: id.into(),
                owner: owner(cx, source),
                shape: Shape::Segment(*pts.last().unwrap(), pts[0]),
            });
        }
    };
    // Build paths separately so the mutable boundary borrow ends before arcs are added.
    for (id, (pts, closed, bulged)) in &cx.polys {
        let flags = owners.get(id);
        if !bulged
            && flags.is_none_or(|f| f.0)
            && (widths.thick(cx, id) || flags.is_some_and(|f| f.1))
            && !not_plotted::source(cx, id)
        {
            path(id, id, pts, *closed);
        }
    }
    for bl in cx.blocks.roots.values() {
        for part in &bl.parts {
            if owners.get(&part.path).is_some_and(|f| f.1)
                && let annotation_blocks::Geom::Path(pts, closed) = &part.geom
            {
                path(&part.path, &bl.root, pts, *closed);
            }
        }
    }
    for (id, r) in &cx.rec {
        let flags = owners.get(id);
        if flags.is_some_and(|f| !f.0 && !f.1) || not_plotted::source(cx, id) {
            continue;
        }
        let boundary = widths.thick(cx, id) || flags.is_some_and(|f| f.1);
        if let Some((a, z)) = cx.lines.get(id) {
            if boundary {
                boundaries.push(Boundary {
                    key: id.clone(),
                    owner: owner(cx, id),
                    shape: Shape::Segment(*a, *z),
                });
            }
            if cx.style[id].pattern == Pattern::Continuous
                && (widths.weight(id) == Some(line_evidence::Width::Thin)
                    || cx.annotation_style(id))
            {
                thin.push(Stroke {
                    key: id.clone(),
                    owner: owner(cx, id),
                    a: *a,
                    z: *z,
                });
            }
        } else if boundary
            && matches!(r.kind.as_str(), "ARC" | "CIRCLE")
            && let Some((c, rad, start, sweep, full)) = cx.arc(id)
        {
            boundaries.push(Boundary {
                key: id.clone(),
                owner: owner(cx, id),
                shape: if full {
                    Shape::Circle(c, rad)
                } else {
                    Shape::Arc(c, rad, start, sweep)
                },
            });
        }
    }
    for bl in cx.blocks.roots.values() {
        let low = bl
            .parts
            .iter()
            .filter_map(|p| (p.weight > 0).then_some(p.weight))
            .min();
        for part in &bl.parts {
            let flags = owners.get(&part.path);
            if flags.is_some_and(|f| !f.0 && !f.1) || not_plotted::source(cx, &bl.root) {
                continue;
            }
            let boundary = flags.is_some_and(|f| f.1);
            let shape = match part.geom {
                annotation_blocks::Geom::Line(a, z) => {
                    if matches!(part.pattern, Pattern::Continuous | Pattern::ByBlock)
                        && low.is_none_or(|lo| part.weight <= lo)
                    {
                        thin.push(Stroke {
                            key: part.path.clone(),
                            owner: owner(cx, &bl.root),
                            a,
                            z,
                        });
                    }
                    Some(Shape::Segment(a, z))
                }
                annotation_blocks::Geom::Arc(c, r, s, sw) => Some(Shape::Arc(c, r, s, sw)),
                annotation_blocks::Geom::Circle(c, r) => Some(Shape::Circle(c, r)),
                _ => None,
            };
            if boundary && let Some(shape) = shape {
                boundaries.push(Boundary {
                    key: part.path.clone(),
                    owner: owner(cx, &bl.root),
                    shape,
                });
            }
        }
    }
    (boundaries, thin)
}

fn crosses(a: &Stroke, z: &Stroke, tol: f64) -> bool {
    let u = sub(a.z, a.a);
    let v = sub(z.z, z.a);
    let den = cross(u, v);
    if den.abs() <= 1e-12 {
        return false;
    }
    let d = sub(z.a, a.a);
    let t = cross(d, v) / den;
    let s = cross(d, u) / den;
    let ta = tol / dist(a.a, a.z).max(tol);
    let tz = tol / dist(z.a, z.z).max(tol);
    t > ta && t < 1. - ta && s > tz && s < 1. - tz
}

fn in_region(p: P, polygon: &[P], tol: f64) -> bool {
    let mut inside = false;
    for (a, z) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        if segment_distance(p, *a, *z) <= tol {
            return true;
        }
        if (a[1] > p[1]) != (z[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) * (z[0] - a[0]) / (z[1] - a[1])
        {
            inside = !inside;
        }
    }
    inside
}

/// An open detail arc alone encloses nothing. Trace its actual closing break
/// curve, with both ends matched, before using the region as a witness.
fn detail_enclosures(cx: &Ctx, b: &Builder, tol: f64) -> Vec<(Option<String>, Vec<P>)> {
    let mut regions = Vec::new();
    for e in &b.exprs {
        if e.rule != "card:detail_view/boundary_arc" {
            continue;
        }
        for m in b.members.iter().filter(|m| m.expression_id == e.id) {
            let Some((c, r, start, sweep, false)) = cx.arc(&m.source_handle) else {
                continue;
            };
            let point = |a: f64| add(c, mul([a.cos(), a.sin()], r));
            let (a, z) = (point(start), point(start + sweep));
            for (h, (pts, closed, bulged)) in &cx.polys {
                if *closed
                    || *bulged
                    || pts.len() < 2
                    || owner(cx, h) != owner(cx, &m.source_handle)
                    || !b
                        .members
                        .iter()
                        .any(|p| p.source_handle == *h && p.member_role == "break_curve")
                {
                    continue;
                }
                let mut path = pts.clone();
                if dist(path[0], a) <= tol && dist(*path.last().unwrap(), z) <= tol {
                    path.reverse();
                }
                if dist(path[0], z) > tol || dist(*path.last().unwrap(), a) > tol {
                    continue;
                }
                // Chord sagitta is bounded by a quarter of the contact tolerance.
                let angle = 2. * (1. - (tol / (4. * r)).clamp(0., 1.)).acos();
                let count = (sweep / angle.max(1e-6)).ceil().max(2.);
                if !count.is_finite() || count > 4096. {
                    continue; // Do not relax the error bound to fit a sampling budget.
                }
                let n = count as usize;
                let mut polygon: Vec<_> = (0..=n)
                    .map(|i| point(start + sweep * i as f64 / n as f64))
                    .collect();
                polygon.extend(path);
                regions.push((owner(cx, &m.source_handle), polygon));
            }
        }
    }
    regions
}

struct Limits {
    tol: f64,
    parallel: f64,
    step_tol: f64,
    min: usize,
}

fn check(
    lines: &[Stroke],
    boundaries: &[Boundary],
    thin: &[Stroke],
    limits: &Limits,
    enclosed: bool,
) -> std::result::Result<(), &'static str> {
    let Limits {
        tol,
        parallel,
        step_tol,
        min,
    } = *limits;
    let rows: Vec<_> = lines.iter().filter(|s| dist(s.a, s.z) > tol).collect();
    if rows.len() < min {
        return Err("成组散线不足，不能仅凭单根或两根斜线确认剖面");
    }
    let u = unit(sub(rows[0].z, rows[0].a));
    if rows
        .iter()
        .any(|s| s.owner != rows[0].owner || cross(u, unit(sub(s.z, s.a))).abs() > parallel)
    {
        return Err("散线不在同一空间或不属于一个平行线族");
    }
    let mut lanes: Vec<_> = rows.iter().map(|s| cross(u, mid(s.a, s.z))).collect();
    lanes.sort_by(f64::total_cmp);
    lanes.dedup_by(|a, z| (*a - *z).abs() <= tol);
    if lanes.len() < min {
        return Err("独立平行线道不足；同一直线的碎片不算多条剖面线");
    }
    let gaps: Vec<_> = lanes.windows(2).map(|w| w[1] - w[0]).collect();
    let step = gaps.iter().copied().fold(f64::INFINITY, f64::min);
    if gaps.iter().any(|g| {
        let k = (g / step).round();
        k < 1. || (g - k * step).abs() > step_tol * step
    }) {
        return Err("平行散线缺少一致间距或可核的间断晶格");
    }
    let keys: BTreeSet<_> = lines.iter().map(|s| s.key.as_str()).collect();
    if !enclosed
        && rows.iter().any(|s| {
            [s.a, s.z].into_iter().any(|p| {
                !boundaries.iter().any(|b| {
                    b.owner == s.owner && !keys.contains(b.key.as_str()) && b.touches(p, tol)
                })
            })
        })
    {
        return Err("缺少真实区域边界接触：散线至少有一端悬空，视图框或文字框不算区域");
    }
    let crossers: Vec<_> = thin
        .iter()
        .filter(|s| {
            s.owner == rows[0].owner
                && !keys.contains(s.key.as_str())
                && dist(s.a, s.z) > tol
                && cross(u, unit(sub(s.z, s.a))).abs() > parallel
                && rows.iter().filter(|r| crosses(s, r, tol)).count() >= 2
        })
        .collect();
    for (i, a) in crossers.iter().enumerate() {
        let v = unit(sub(a.z, a.a));
        if crossers[i + 1..].iter().any(|z| {
            cross(v, unit(sub(z.z, z.a))).abs() <= parallel && cross(v, sub(z.a, a.a)).abs() > tol
        }) {
            return Err("散线与另一平行线族多次相交，只有网格证据；未有原生材料填充证据");
        }
    }
    Ok(())
}

pub(super) fn state(cx: &Ctx, b: &mut Builder, titles: &[ViewTitle]) -> Result<()> {
    let c = representation::card_ref(CARD)?;
    let p = |name| representation::param(c, name);
    let tol = p("endpoint_tolerance")? * cx.scale;
    let par = p("parallel_sine")?;
    let step = p("spacing_tolerance")?;
    let min = p("min_lanes")? as usize;
    // Read a detail's own circular boundary with the existing detector. Keep
    // this witness copy separate: the normal pass still sees the final geometry.
    let mut region = Builder {
        exprs: b.exprs.clone(),
        members: b.members.clone(),
        by_source: b.by_source.clone(),
        links: b.links.clone(),
    };
    let cover = topology::cover(cx, b);
    // Preview actual outline intervals before reading the detail's own boundary.
    structure(cx, &mut region, &cover);
    view_layout::detail_boundaries(cx, &mut region, titles)?;
    let (boundaries, thin) = context(cx, &region);
    let detail_regions = detail_enclosures(cx, &region, tol);
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "section_hatch" {
            continue;
        }
        let id = &b.exprs[i].id;
        let ms: Vec<_> = b
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| &m.expression_id == id)
            .map(|(j, _)| j)
            .collect();
        if ms
            .iter()
            .any(|&j| cx.kind(&b.members[j].source_handle) == "HATCH")
        {
            continue;
        }
        let lines: Vec<_> = ms
            .iter()
            .filter_map(|&j| stroke(cx, &b.members[j]))
            .collect();
        let local_enclosed = ms.iter().all(|&j| b.members[j].native_member_id.is_none())
            && cover.encloses_hatch(
                &lines.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
                &lines
                    .iter()
                    .flat_map(|s| [s.a, mid(s.a, s.z), s.z])
                    .collect::<Vec<_>>(),
                tol,
            );
        let detail_enclosed = !lines.is_empty()
            && detail_regions.iter().any(|(space, polygon)| {
                lines.iter().all(|s| {
                    &s.owner == space
                        && [s.a, mid(s.a, s.z), s.z]
                            .iter()
                            .all(|p| in_region(*p, polygon, tol))
                })
            });
        let result = if lines.len() != ms.len() {
            Err("散线编码不完整，不能核验成组与边界")
        } else if ms.iter().any(|&j| {
            b.members[j].native_member_id.is_none()
                && line_evidence::corner_to_corner(cx, &b.members[j].source_handle)
        }) {
            Err("斜线两端是轮廓转角，保留为倒角/拐角候选，不作剖面")
        } else {
            check(
                &lines,
                &boundaries,
                &thin,
                &Limits {
                    tol,
                    parallel: par,
                    step_tol: step,
                    min,
                },
                local_enclosed || detail_enclosed,
            )
        };
        match result {
            Ok(()) => {
                b.exprs[i].evidence_ids.push(format!("card:{CARD}"));
                b.exprs[i].evidence_ids.push(format!(
                    "loose_hatch_region:{}",
                    if local_enclosed {
                        "closed_local_section"
                    } else if detail_enclosed {
                        "closed_detail_boundary"
                    } else {
                        "independent_boundary_contacts"
                    }
                ));
                for &j in &ms {
                    b.members[j].evidence_ids.push(format!("card:{CARD}"));
                }
            }
            Err(reason) => {
                let e = &mut b.exprs[i];
                let previous = std::mem::take(&mut e.evidence_ids);
                e.evidence_ids = previous
                    .into_iter()
                    .map(|s| format!("rejected_hatch_evidence:{s}"))
                    .collect();
                e.evidence_ids.extend([
                    format!("rejected_hatch_rule:{}", e.rule),
                    format!("card:{CARD}"),
                ]);
                e.kind = "unresolved".into();
                e.family = family("unresolved").into();
                e.rule = "card:loose_hatch_region_guard/without_region".into();
                e.label = "散线用途未知（剖面区域证据不足）".into();
                e.state = "unknown".into();
                e.knowledge_ids = knowledge("unresolved");
                e.unknowns.push(reason.into());
                for &j in &ms {
                    let m = &mut b.members[j];
                    m.evidence_ids
                        .push(format!("rejected_hatch_member_role:{}", m.member_role));
                    m.evidence_ids.push(format!("card:{CARD}"));
                    m.member_role = "unresolved_source".into();
                    m.state = "unknown".into();
                    if m.line_type == "fill" {
                        m.line_type = cx.lt(&m.source_handle).into();
                    }
                    m.unknowns.push(reason.into());
                }
            }
        }
    }
    Ok(())
}
