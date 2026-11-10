//! How a view was obtained, stated apart from its place in the arrangement (cards
//! `section_view_type` and `view_callout`; GB/T 17452-1998 5, 6; GB/T 4458.6-2002 5.3–5.5, 6.1).
//!
//! A view has two independent facts: its role in the arrangement (principal, left, top …), which
//! is its kind and name, and the way it was obtained (an outside view; a full, half or local
//! section; cut by one plane, by parallel planes or by intersecting planes). The second is stated
//! here as evidence on the view; kinds and ids never change.
//!
//! * A cutting symbol (strokes, arrows, letters) names the view titled `X—X` with its letters on
//!   its own sheet, whatever that view's role; a detail view `X scale` of the same letter is never
//!   what it names. A callout that has a cutting stroke is a cutting symbol, not a
//!   viewing-direction arrow, whether it is drawn as one block or with loose lines.
//! * Where two strokes of the symbol meet at an oblique angle the cutting planes intersect and the
//!   named view is a section by intersecting planes turned into one plane (5.4); at a right angle
//!   the planes are parallel (5.3).
//! * A view with section lines on one side only of the centre line that halves it, outline on
//!   both sides that is not mirrored, and no thick or break line along that centre line is a half
//!   section (5.5).
//! * A view named in direction-view form (`X向`) that holds section lines and is named by a
//!   symbol with cutting strokes is a section all the same.
use super::*;

const CARD: &str = "section_view_type";

fn param(name: &str) -> Result<f64> {
    representation::param(representation::card_ref(CARD)?, name)
}

/// The sheet part of a view id (`sheet-2` of `view:sheet-2:principal`; empty on a one-sheet file).
pub(super) fn sheet_key(view: &str) -> &str {
    view.strip_prefix("view:")
        .filter(|v| v.starts_with("sheet-"))
        .and_then(|v| v.split(':').next())
        .unwrap_or("")
}

fn push(ev: &mut Vec<String>, x: &str) {
    if !ev.iter().any(|y| y == x) {
        ev.push(x.to_string());
    }
}

fn seg_dist(p: P, a: P, z: P) -> f64 {
    let v = sub(z, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), v) / n).clamp(0., 1.);
    dist(p, add(a, mul(v, t)))
}

fn path_dist(p: P, pts: &[P]) -> f64 {
    match pts {
        [] => f64::INFINITY,
        [a] => dist(p, *a),
        _ => pts
            .windows(2)
            .map(|w| seg_dist(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min),
    }
}

// ---------------------------------------------------------------- which view a cutting symbol names

/// A cutting symbol names the view titled `X—X` with its letters on its own sheet, whatever the
/// kind of that view; failing that the view named `X向` (a section named in direction-view
/// form). It never names a detail view of the same letter.
pub(super) fn cutting_references(
    b: &Builder,
    regions: &[Region],
    vmap: &BTreeMap<usize, String>,
    refs: &mut BTreeMap<usize, String>,
) {
    let stroked: BTreeSet<&str> = b
        .members
        .iter()
        .filter(|m| m.member_role == "cutting_stroke")
        .map(|m| m.expression_id.as_str())
        .collect();
    for (i, e) in b.exprs.iter().enumerate() {
        if e.kind != "view_callout" || !stroked.contains(e.id.as_str()) {
            continue;
        }
        let key = e.label.split_whitespace().next().unwrap_or("");
        if key.is_empty() {
            continue;
        }
        let own = vmap.get(&i).map_or("", |v| sheet_key(v));
        let here = |r: &&Region| r.kind != "sheet" && sheet_key(&r.id) == own;
        let target = regions
            .iter()
            .filter(here)
            .find(|r| view_layout::section_letters(&r.label).as_deref() == Some(key))
            .or_else(|| {
                regions
                    .iter()
                    .filter(here)
                    .find(|r| r.label == format!("{key}向"))
            });
        match target {
            Some(r) => {
                refs.insert(i, r.id.clone());
            }
            None => {
                refs.remove(&i);
            }
        }
    }
}

// ---------------------------------------------------------------- cutting symbols and the views they name

/// The strokes of a callout as segments.
fn strokes(cx: &Ctx, b: &Builder, id: &str) -> Vec<(P, P)> {
    b.members
        .iter()
        .filter(|m| m.expression_id == id && m.member_role == "cutting_stroke")
        .map(|m| member_points(cx, m))
        .filter(|p| p.len() == 2 && dist(p[0], p[1]) > 1e-9)
        .map(|p| (p[0], p[1]))
        .collect()
}

/// Turns of a cutting path: the point where two strokes meet and the cosine of their angle.
fn turns(strokes: &[(P, P)], tol: f64) -> Vec<(P, f64)> {
    let mut out = Vec::new();
    for (k, (a, z)) in strokes.iter().enumerate() {
        for (p, q) in strokes.iter().skip(k + 1) {
            let shared = [a, z]
                .into_iter()
                .find(|x| dist(**x, *p) <= tol || dist(**x, *q) <= tol);
            let cos = dot(unit(sub(*z, *a)), unit(sub(*q, *p))).abs();
            if let Some(x) = shared.filter(|_| cos < 0.999) {
                out.push((*x, cos));
            }
        }
    }
    out
}

/// The cutting symbols that name a view: expression id and the kinds of cutting planes its path shows.
type Named = BTreeMap<String, Vec<(String, Vec<&'static str>)>>;

/// A callout with a cutting stroke is a cutting symbol; the turns of its path say whether the
/// cutting planes are parallel or intersect. Returns the symbols by the view each names.
fn cutting_symbols(cx: &Ctx, b: &mut Builder) -> Result<Named> {
    let callout = representation::card_ref("view_callout")?;
    let tol = representation::param(callout, "join")? * cx.scale;
    let right = representation::param(callout, "turn_right_angle_cos_max")?;
    // views of the circles and arcs, to find a turn at the centre of a round view
    let index: BTreeMap<&str, &MdEngineeringExpression> =
        b.exprs.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut round: Vec<(P, f64, String, Option<String>)> = Vec::new();
    for m in &b.members {
        let Some(e) = index.get(m.expression_id.as_str()) else {
            continue;
        };
        if !GEOMETRY.contains(&e.kind.as_str()) || m.native_member_id.is_some() {
            continue;
        }
        if let Some((c, r, ..)) = cx.arc(&m.source_handle) {
            round.push((c, r, m.source_handle.clone(), e.view_id.clone()));
        }
    }
    drop(index);
    let mut named = Named::new();
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "view_callout" {
            continue;
        }
        let id = b.exprs[i].id.clone();
        let ss = strokes(cx, b, &id);
        if ss.is_empty() {
            continue;
        }
        let view = b.exprs[i].view_id.clone();
        let e = &mut b.exprs[i];
        if let Some(q) = e.qualifiers.iter_mut().find(|q| *q == "view_direction") {
            *q = "cutting_plane".into();
            push(&mut e.evidence_ids, "card:view_callout");
            push(&mut e.evidence_ids, "variant:cutting_symbol_by_stroke");
        }
        let mut types: Vec<&str> = Vec::new();
        for (at, cos) in turns(&ss, tol) {
            let (name, concept, section) = if cos < right {
                ("parallel_cutting_planes", "DC-G2-015", "parallel_planes")
            } else {
                (
                    "intersecting_cutting_planes",
                    "DC-G2-016",
                    "intersecting_planes",
                )
            };
            push(&mut e.evidence_ids, "card:view_callout");
            push(&mut e.evidence_ids, name);
            push(&mut e.evidence_ids, &format!("concept:{concept}"));
            if !types.contains(&section) {
                types.push(section);
            }
            // the turn at the centre of a round view (the axis the planes intersect in)
            if let Some(c) = round
                .iter()
                .filter(|c| c.3 == view && dist(c.0, at) <= tol)
                .max_by(|x, y| x.1.total_cmp(&y.1))
            {
                push(&mut e.evidence_ids, &format!("turn_at_centre_of:{}", c.2));
            }
        }
        if let Some(target) = e
            .evidence_ids
            .iter()
            .find_map(|x| x.strip_prefix("references_view:"))
        {
            named
                .entry(target.to_string())
                .or_default()
                .push((id, types));
        }
    }
    Ok(named)
}

// ---------------------------------------------------------------- section views by their own content

/// What a view shows, gathered for the section tests.
struct Content {
    /// Visible outline pieces as point runs.
    outline: Vec<Vec<P>>,
    /// Centre lines: source and end points.
    axes: Vec<(String, P, P)>,
    /// Section hatches: expression index and the points of its regions or lines.
    hatches: Vec<(usize, Vec<P>)>,
    /// Break lines as point runs.
    breaks: Vec<Vec<P>>,
}

fn content(cx: &Ctx, b: &Builder, view: &str) -> Content {
    let mut c = Content {
        outline: vec![],
        axes: vec![],
        hatches: vec![],
        breaks: vec![],
    };
    for (i, e) in b.exprs.iter().enumerate() {
        if e.view_id.as_deref() != Some(view) {
            continue;
        }
        let ms = b.members.iter().filter(|m| m.expression_id == e.id);
        match e.kind.as_str() {
            "visible_contour" => c.outline.extend(
                ms.filter(|m| m.member_role == "contour_segment")
                    .map(|m| member_points(cx, m))
                    .filter(|p| p.len() >= 2),
            ),
            "center_axis" => c.axes.extend(ms.filter_map(|m| {
                let (p, q) = cx.lines.get(&m.source_handle).copied()?;
                (m.native_member_id.is_none() && dist(p, q) > 1e-9)
                    .then(|| (m.source_handle.clone(), p, q))
            })),
            "section_hatch" => {
                let pts: Vec<P> = ms
                    .filter(|m| m.member_role == "hatch_fill")
                    .flat_map(|m| member_points(cx, m))
                    .collect();
                if !pts.is_empty() {
                    c.hatches.push((i, pts));
                }
            }
            "break_line" => c.breaks.extend(
                ms.filter(|m| m.member_role == "break_curve")
                    .map(|m| member_points(cx, m))
                    .filter(|p| p.len() >= 2),
            ),
            _ => {}
        }
    }
    c
}

/// Hatches of the view that no break line bounds: a hatch whose region a wavy or zigzag line
/// touches is a local section (GB/T 4458.6-2002 5.6).
fn unbroken(cx: &Ctx, c: &Content) -> Result<Vec<usize>> {
    let reach = param("break_line_reach")? * cx.scale;
    Ok((0..c.hatches.len())
        .filter(|&k| {
            let pts = &c.hatches[k].1;
            !c.breaks.iter().any(|br| {
                br.iter().any(|p| path_dist(*p, pts) <= reach)
                    || pts.iter().any(|p| path_dist(*p, br) <= reach)
            })
        })
        .collect())
}

/// The centre line a half section is halved by: the section lines of the view lie on one side of
/// it only and run along most of it, the outline lies on both sides and is not mirrored, and no
/// outline runs along the centre line itself.
fn half_axis(cx: &Ctx, c: &Content, free: &[usize]) -> Result<Option<(String, Vec<usize>)>> {
    let span_min = param("axis_span_min")?;
    let offset_max = param("symmetry_offset_max")?;
    let hatch_min = param("hatch_span_min")?;
    let side_min = param("side_outline_min")?;
    let asym_min = param("asymmetry_min")?;
    let tol = param("on_axis_tolerance")? * cx.scale;
    let mirror = param("mirror_tolerance")? * cx.scale;
    if free.is_empty() || c.outline.is_empty() {
        return Ok(None);
    }
    // samples of the outline with the length each stands for
    let mut samples: Vec<(P, f64)> = Vec::new();
    for run in &c.outline {
        for w in run.windows(2) {
            let l = dist(w[0], w[1]);
            if l <= 1e-9 {
                continue;
            }
            let n = if run.len() == 2 { 5 } else { 1 };
            for k in 0..n {
                let t = (k as f64 + 0.5) / n as f64;
                samples.push((lerp(w[0], w[1], t), l / n as f64));
            }
        }
    }
    let total: f64 = samples.iter().map(|s| s.1).sum();
    if total <= 1e-9 {
        return Ok(None);
    }
    for (h, p, q) in &c.axes {
        let u = unit(sub(*q, *p));
        let along = |x: P| dot(sub(x, *p), u);
        let off = |x: P| cross(u, sub(x, *p));
        // the centre line runs along the whole outline and halves its extent
        let ends = c.outline.iter().flatten();
        let (mut s0, mut s1, mut o0, mut o1) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for x in ends {
            s0 = s0.min(along(*x));
            s1 = s1.max(along(*x));
            o0 = o0.min(off(*x));
            o1 = o1.max(off(*x));
        }
        let length = dist(*p, *q);
        let covered = s1.min(length) - s0.max(0.);
        if s1 - s0 <= 1e-9
            || covered < span_min * (s1 - s0)
            || o0 >= -tol
            || o1 <= tol
            || (o1 + o0).abs() > offset_max * (o1 - o0)
        {
            continue;
        }
        // section lines on one side only, along most of the centre line
        let mut side = 0;
        let (mut h0, mut h1) = (f64::INFINITY, f64::NEG_INFINITY);
        let mut one_sided = true;
        for &k in free {
            let pts = &c.hatches[k].1;
            let s = if pts.iter().all(|x| off(*x) >= -tol) {
                1
            } else if pts.iter().all(|x| off(*x) <= tol) {
                -1
            } else {
                0
            };
            if s == 0 || (side != 0 && s != side) {
                one_sided = false;
                break;
            }
            side = s;
            for x in pts {
                h0 = h0.min(along(*x));
                h1 = h1.max(along(*x));
            }
        }
        if !one_sided || h1 - h0 < hatch_min * (s1 - s0) {
            continue;
        }
        // no outline along the centre line: the halves are divided by the centre line alone
        let on_axis = c.outline.iter().any(|run| {
            run.len() == 2
                && dist(run[0], run[1]) > 4. * tol
                && off(run[0]).abs() <= tol
                && off(run[1]).abs() <= tol
        });
        if on_axis {
            continue;
        }
        // outline on both sides, and the two sides are not mirror images
        let (mut plus, mut minus, mut unmatched) = (0., 0., 0.);
        for (x, l) in &samples {
            let o = off(*x);
            if o.abs() <= tol {
                continue;
            }
            if o > 0. {
                plus += l;
            } else {
                minus += l;
            }
            let image = sub(*x, mul([-u[1], u[0]], 2. * o));
            if !c.outline.iter().any(|run| path_dist(image, run) <= mirror) {
                unmatched += l;
            }
        }
        if plus < side_min * total || minus < side_min * total || unmatched < asym_min * total {
            continue;
        }
        return Ok(Some((h.clone(), free.to_vec())));
    }
    Ok(None)
}

/// State on every view how it was obtained when its name, the symbol that names it or its own
/// content says so. Whether a section shows the cut face only (a cut, 断面) or also what lies
/// behind it (a sectional view, 剖视) is not told apart here, so a full section is not stated.
fn section_views(cx: &Ctx, b: &mut Builder, regions: &mut [Region], named: &Named) -> Result<()> {
    let card = format!("card:{CARD}");
    for r in regions.iter_mut() {
        if matches!(r.kind, "sheet" | "detail_view") {
            continue;
        }
        let c = content(cx, b, &r.id);
        let free = unbroken(cx, &c)?;
        let symbols = named.get(&r.id).map(Vec::as_slice).unwrap_or_default();
        for (id, _) in symbols {
            push(&mut r.evidence, &format!("cutting_symbol:{id}"));
        }
        // a section by its name `X—X`, or named in direction-view form by a cutting symbol while
        // it holds section lines
        let letters = view_layout::section_letters(&r.label);
        let direction_form = letters.is_none()
            && !symbols.is_empty()
            && r.label.ends_with('向')
            && !c.hatches.is_empty();
        let section = letters.is_some() || direction_form;
        if let Some(l) = &letters {
            push(&mut r.evidence, "view_type:section");
            push(&mut r.evidence, &format!("section_name:{l}"));
        }
        if direction_form {
            push(&mut r.evidence, "view_type:section");
            push(&mut r.evidence, &card);
            push(&mut r.evidence, "variant:section_named_as_direction_view");
        }
        if section {
            for t in symbols.iter().flat_map(|s| s.1.iter()) {
                push(&mut r.evidence, &format!("section_type:{t}"));
                push(&mut r.evidence, &card);
                push(&mut r.evidence, &format!("variant:by_{t}"));
            }
        }
        // half section: the section lines on one side of the centre line that halves the view
        if let Some((axis, hs)) = half_axis(cx, &c, &free)? {
            push(&mut r.evidence, "view_type:section");
            push(&mut r.evidence, "section_type:half");
            push(&mut r.evidence, &card);
            push(&mut r.evidence, "variant:half_section");
            push(&mut r.evidence, &format!("half_section_axis:{axis}"));
            for k in hs {
                let ev = &mut b.exprs[c.hatches[k].0].evidence_ids;
                push(ev, &card);
                push(ev, "variant:half_section");
                push(ev, "concept:DC-G2-020");
            }
        }
    }
    Ok(())
}

/// Cutting symbols, the views they name, and the section type each view shows.
pub(super) fn state(cx: &Ctx, b: &mut Builder, regions: &mut [Region]) -> Result<()> {
    let named = cutting_symbols(cx, b)?;
    section_views(cx, b, regions, &named)
}
