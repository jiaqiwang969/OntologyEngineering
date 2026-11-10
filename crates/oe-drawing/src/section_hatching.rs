//! What a section hatch says about itself (card `section_line_region`, variants
//! `pattern_signature` and `shared_boundary`).
//!
//! Section lines are equally spaced parallel thin lines (GB/T 4457.5-2013 4.1, GB/T 17453-2005):
//! their direction and spacing are the signature of the sectioned part, so every section hatch
//! states them as evidence and in its name. They are read from the pattern definition of a
//! native hatch, and from the lines themselves where the section lines are drawn one by one (loose
//! lines, or lines of a view stored as one block). The name of a hatch is its view, the word for
//! section lines, the direction and the spacing; the pattern name a CAD system stores (a vendor
//! word such as "user defined") says nothing about the drawing and is kept only as evidence.
//! Two hatch regions that share a boundary line name each other: whether they are two parts or
//! one is decided by the assembly knowledge (card `assembly_drawing`).
use super::*;

/// One family of parallel section lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Family {
    /// Direction of the lines in degrees, in [0, 180).
    pub angle: f64,
    /// Distance between neighbouring lines, in drawing units.
    pub spacing: f64,
}

/// A section hatch as read: its expression, view, line families and the boundary segments of
/// its native regions (empty for section lines drawn one by one).
#[derive(Clone, Debug)]
pub(super) struct Hatch {
    pub expr: usize,
    pub view: Option<String>,
    pub families: Vec<Family>,
    pub solid: bool,
    pub boundary: Vec<(P, P)>,
}

fn half_turn(deg: f64) -> f64 {
    let a = deg.rem_euclid(180.);
    if a > 180. - 1e-6 { 0. } else { a }
}

/// Smallest angle between two line directions given in degrees (0 to 90).
pub(super) fn direction_difference(a: f64, b: f64) -> f64 {
    let d = (half_turn(a) - half_turn(b)).abs();
    d.min(180. - d)
}

/// The line families of a native hatch: the lines of its pattern definition (direction and
/// offset to the next line as stored, already turned and scaled), or for a pattern given only
/// by angle and spacing those two. A solid fill has none.
pub(super) fn pattern_families(r: &Record) -> Vec<Family> {
    if int(r, 70) != 0 {
        return vec![];
    }
    let t = &r.tags;
    let num = |i: usize| t.get(i).and_then(|x| x.1.trim().parse::<f64>().ok());
    let mut out: Vec<Family> = Vec::new();
    if let Some(start) = t.iter().rposition(|x| x.0 == 78) {
        let mut i = start + 1;
        while i < t.len() && t[i].0 != 98 {
            if t[i].0 != 53 {
                i += 1;
                continue;
            }
            let angle = num(i);
            let (mut dx, mut dy) = (None, None);
            i += 1;
            while i < t.len() && !matches!(t[i].0, 53 | 98) {
                match t[i].0 {
                    45 => dx = num(i),
                    46 => dy = num(i),
                    _ => {}
                }
                i += 1;
            }
            if let (Some(a), Some(dx), Some(dy)) = (angle, dx, dy) {
                let u = [a.to_radians().cos(), a.to_radians().sin()];
                let spacing = cross(u, [dx, dy]).abs();
                if spacing > 1e-9 {
                    out.push(Family {
                        angle: half_turn(a),
                        spacing,
                    });
                }
            }
        }
    }
    if out.is_empty()
        && let (Ok(a), Ok(s)) = (r.number(52), r.number(41))
        && s > 1e-9
        && int(r, 76) == 0
    {
        out.push(Family {
            angle: half_turn(a),
            spacing: s,
        });
        if int(r, 77) != 0 {
            out.push(Family {
                angle: half_turn(a + 90.),
                spacing: s,
            });
        }
    }
    out
}

/// The family of section lines drawn one by one: the direction of the longest line (the others
/// are parallel to it) and the median step between their distinct offsets across it.
pub(super) fn line_family(lines: &[(P, P)], parallel_deg: f64) -> Option<Family> {
    let (a, z) = lines
        .iter()
        .copied()
        .max_by(|x, y| dist(x.0, x.1).total_cmp(&dist(y.0, y.1)))?;
    if dist(a, z) <= 1e-9 {
        return None;
    }
    let u = unit(sub(z, a));
    let sin = parallel_deg.to_radians().sin();
    let mut offs: Vec<f64> = lines
        .iter()
        .filter(|(p, q)| dist(*p, *q) > 1e-9 && cross(u, unit(sub(*q, *p))).abs() <= sin)
        .map(|(p, q)| cross(u, mid(*p, *q)))
        .collect();
    offs.sort_by(f64::total_cmp);
    // pieces of one section line (cut by a hole) share an offset
    let mut lanes: Vec<f64> = Vec::new();
    for o in offs {
        if lanes
            .last()
            .is_none_or(|l| o - l > 1e-6 * (1. + o.abs()))
        {
            lanes.push(o);
        }
    }
    let spacing = median(lanes.windows(2).map(|w| w[1] - w[0]).collect())?;
    Some(Family {
        angle: half_turn(u[1].atan2(u[0]).to_degrees()),
        spacing,
    })
}

fn rounded(v: f64, digits: i32) -> String {
    let k = 10f64.powi(digits);
    fmt_num((v * k).round() / k)
}

fn push(e: &mut MdEngineeringExpression, x: String) {
    if !e.evidence_ids.contains(&x) {
        e.evidence_ids.push(x);
    }
}

/// Every section hatch of the reading with its signature and native boundary.
pub(super) fn hatches(cx: &Ctx, b: &Builder) -> Result<Vec<Hatch>> {
    let card = representation::card_ref("section_line_region")?;
    let parallel = representation::param(card, "angle_tolerance_deg")?;
    let mut out = Vec::new();
    for (i, e) in b.exprs.iter().enumerate() {
        if e.kind != "section_hatch" {
            continue;
        }
        let ms: Vec<&MdExpressionMember> = b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id && m.member_role == "hatch_fill")
            .collect();
        let native: Vec<&Record> = ms
            .iter()
            .filter(|m| m.native_member_id.is_none())
            .filter_map(|m| cx.rec.get(&m.source_handle).copied())
            .filter(|r| r.kind == "HATCH")
            .collect();
        let solid = !native.is_empty() && native.iter().all(|r| int(r, 70) != 0);
        let mut families: Vec<Family> = Vec::new();
        for f in native.iter().flat_map(|r| pattern_families(r)) {
            if !families.iter().any(|x| {
                direction_difference(x.angle, f.angle) < 1e-6
                    && (x.spacing - f.spacing).abs() <= 1e-6 * f.spacing.max(1.)
            }) {
                families.push(f);
            }
        }
        if families.is_empty() && !solid {
            // section lines drawn one by one: loose lines, or the lines of a block view
            let lines: Vec<(P, P)> = ms
                .iter()
                .filter(|m| {
                    m.native_member_id.is_some() || cx.lines.contains_key(&m.source_handle)
                })
                .map(|m| member_points(cx, m))
                .filter(|p| p.len() == 2)
                .map(|p| (p[0], p[1]))
                .collect();
            if let Some(mut f) = line_family(&lines, parallel) {
                // the spacing the grouping rule found is the rule's own evidence
                if let Some(s) = e
                    .evidence_ids
                    .iter()
                    .filter_map(|x| x.strip_prefix("spacing:"))
                    .find_map(|x| x.parse::<f64>().ok())
                    .filter(|s| *s > 1e-9)
                {
                    f.spacing = s;
                }
                families.push(f);
            }
        }
        let boundary = native
            .iter()
            .flat_map(|r| representation::hatch_boundary(r))
            .collect();
        out.push(Hatch {
            expr: i,
            view: e.view_id.clone(),
            families,
            solid,
            boundary,
        });
    }
    Ok(out)
}

/// Length two boundaries run along each other (collinear segments overlapping within `tol`).
pub(super) fn shared_length(a: &[(P, P)], b: &[(P, P)], tol: f64) -> f64 {
    let mut total = 0.;
    for (p, q) in a {
        let l = dist(*p, *q);
        if l <= tol {
            continue;
        }
        let u = unit(sub(*q, *p));
        for (r, s) in b {
            if dist(*r, *s) <= tol
                || cross(u, unit(sub(*s, *r))).abs() > 1e-3
                || cross(u, sub(*r, *p)).abs() > tol
                || cross(u, sub(*s, *p)).abs() > tol
            {
                continue;
            }
            let (t0, t1) = (dot(sub(*r, *p), u), dot(sub(*s, *p), u));
            let overlap = t0.max(t1).min(l) - t0.min(t1).max(0.);
            if overlap > tol {
                total += overlap;
            }
        }
    }
    total
}

/// Smallest spacing of a hatch (none for a solid fill or an unread pattern).
pub(super) fn spacing_of(h: &Hatch) -> Option<f64> {
    h.families
        .iter()
        .map(|f| f.spacing)
        .min_by(f64::total_cmp)
}

/// Pairs of hatches of one view whose native regions share a boundary line (indexes into `hs`).
pub(super) fn neighbours(cx: &Ctx, hs: &[Hatch]) -> Result<Vec<(usize, usize)>> {
    let card = representation::card_ref("section_line_region")?;
    let min = representation::param(card, "shared_edge_min")?;
    let tol = representation::param(card, "shared_edge_tolerance")?;
    let mut out = Vec::new();
    for (i, a) in hs.iter().enumerate() {
        for (j, c) in hs.iter().enumerate().skip(i + 1) {
            if a.view != c.view || a.boundary.is_empty() || c.boundary.is_empty() {
                continue;
            }
            let unit_len = [spacing_of(a), spacing_of(c)]
                .into_iter()
                .flatten()
                .min_by(f64::total_cmp)
                .unwrap_or(cx.scale);
            if shared_length(&a.boundary, &c.boundary, tol * unit_len) >= min * unit_len {
                out.push((i, j));
            }
        }
    }
    Ok(out)
}

/// The words of a signature: direction(s) and spacing, as they stand in a hatch name.
pub(super) fn signature_words(h: &Hatch) -> Option<String> {
    let first = h.families.first()?;
    let angles: Vec<String> = h
        .families
        .iter()
        .map(|f| format!("{}°", rounded(f.angle, 1)))
        .collect();
    Some(format!(
        "{} 间距 {}",
        angles.join("/"),
        rounded(first.spacing, 3)
    ))
}

/// State on every section hatch its direction and spacing, its neighbours across a shared
/// boundary, and its name: view, section lines, direction and spacing.
pub(super) fn state(cx: &Ctx, b: &mut Builder, regions: &[Region]) -> Result<()> {
    let hs = hatches(cx, b)?;
    let pairs = neighbours(cx, &hs)?;
    for h in &hs {
        let view = regions
            .iter()
            .find(|r| Some(&r.id) == h.view.as_ref())
            .map_or("未定视图".to_string(), view_layout::shown_label);
        let e = &mut b.exprs[h.expr];
        // a name another rule gave (the hatch of a detail view) is kept; a pattern name is not
        let base = if e.label.ends_with(" · 剖面线") {
            e.label.clone()
        } else {
            format!("{view} · 剖面线")
        };
        if h.solid {
            e.label = format!("{view} · 剖面区域（涂黑）");
            push(e, "hatch_solid".into());
            continue;
        }
        let Some(words) = signature_words(h) else {
            e.label = base;
            continue;
        };
        e.label = format!("{base} {words}");
        if let [f] = h.families.as_slice() {
            push(e, format!("hatch_angle:{}", rounded(f.angle, 1)));
            push(e, format!("hatch_spacing:{}", rounded(f.spacing, 3)));
        } else {
            for f in &h.families {
                push(
                    e,
                    format!(
                        "hatch_family:{}:{}",
                        rounded(f.angle, 1),
                        rounded(f.spacing, 3)
                    ),
                );
            }
        }
        push(e, "card:section_line_region".into());
        push(e, "variant:pattern_signature".into());
    }
    for (i, j) in pairs {
        let (a, c) = (b.exprs[hs[i].expr].id.clone(), b.exprs[hs[j].expr].id.clone());
        for (own, other) in [(hs[i].expr, &c), (hs[j].expr, &a)] {
            let e = &mut b.exprs[own];
            push(e, format!("adjacent:{other}"));
            push(e, "card:section_line_region".into());
            push(e, "variant:shared_boundary".into());
        }
    }
    Ok(())
}
