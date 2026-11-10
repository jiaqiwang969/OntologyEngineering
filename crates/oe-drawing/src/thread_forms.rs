//! What a thread convention is called and which thread it is (card `thread_representation`,
//! variants `internal_thread_end_view`, `external_thread_end_view`, `thread_named_by_form` and
//! `same_thread_in_two_views`; GB/T 4459.1-1995 3.1, 3.5).
//!
//! * The root of a thread is drawn thin (3.1). In the view along the axis the thin circle of
//!   about three quarters of a turn is the root: larger than the thick full circle it is the major
//!   diameter of an internal thread, smaller it is the minor diameter of an external thread. The
//!   two readings are stated on the pieces, grouped apart and carry their own concept.
//! * A thread convention is named by what it is: its view, internal or external thread, end view
//!   or section, its diameters, and for a section whether the thread runs through or ends in the
//!   material with its depth. A source handle is never a name.
//! * An end view and a section of a thread in two views of one sheet are the same thread when the
//!   thin (major) diameters are equal and the centre of the end view lies on the axis of the
//!   section (the views are in projection), or on the cutting line of the cutting symbol that
//!   names the section view.
use super::*;

/// Qualifiers that keep thread end views of the two forms in groups of their own.
const FORMS: [(&str, &str); 2] = [
    ("internal_thread", "内螺纹（端视）"),
    ("external_thread", "外螺纹（端视）"),
];

/// The reading of a thread end view from its two radii: qualifier, evidence of both pieces,
/// evidence of the thin arc member and of the full circle member.
pub(super) struct EndView {
    pub qualifier: &'static str,
    pub evidence: Vec<String>,
    pub arc: String,
    pub circle: String,
}

/// GB/T 4459.1-1995 3.1: the thin three-quarter circle is the root. Outside the thick circle it is
/// the major diameter of an internal thread, inside it the minor diameter of an external thread.
pub(super) fn end_view(thin_radius: f64, full_radius: f64) -> EndView {
    let internal = thin_radius > full_radius;
    let (qualifier, variant, concept) = if internal {
        ("internal_thread", "internal_thread_end_view", "DC-G3-005")
    } else {
        ("external_thread", "external_thread_end_view", "DC-G3-024")
    };
    let (thin, full) = (fmt_num(2. * thin_radius), fmt_num(2. * full_radius));
    let (arc, circle) = if internal {
        (format!("major:{thin}"), format!("minor:{full}"))
    } else {
        (format!("minor:{thin}"), format!("major:{full}"))
    };
    EndView {
        qualifier,
        evidence: vec![
            "card:thread_representation".into(),
            format!("variant:{variant}"),
            format!("concept:{concept}"),
        ],
        arc,
        circle,
    }
}

/// The form qualifiers of a line piece: pieces of different forms are grouped apart.
pub(super) fn group_form(e: &MdEngineeringExpression) -> Vec<String> {
    e.qualifiers
        .iter()
        .filter(|q| FORMS.iter().any(|f| f.0 == q.as_str()))
        .cloned()
        .collect()
}

/// The name of a group of pieces of one form (none for pieces without a form).
pub(super) fn group_name(form: &[String]) -> Option<&'static str> {
    FORMS
        .iter()
        .find(|f| form.iter().any(|q| q == f.0))
        .map(|f| f.1)
}

fn push(e: &mut MdEngineeringExpression, x: String) {
    if !e.evidence_ids.contains(&x) {
        e.evidence_ids.push(x);
    }
}

fn number(ev: &[String], key: &str) -> Option<f64> {
    ev.iter()
        .filter_map(|x| x.strip_prefix(key))
        .find_map(|x| x.parse::<f64>().ok())
}

fn sheet_of(view: Option<&String>) -> &str {
    view.and_then(|v| v.strip_prefix("view:"))
        .filter(|v| v.starts_with("sheet-"))
        .and_then(|v| v.split(':').next())
        .unwrap_or("")
}

/// A thread drawn in section: expression, view, a point of its axis, the axis direction and the
/// distance between its thin (major-diameter) lines.
struct Section {
    expr: usize,
    view: Option<String>,
    origin: P,
    dir: P,
    major: f64,
}

/// The thin arc of a thread end view: expression, member, source, view, centre and diameter.
struct Arc {
    expr: usize,
    member: usize,
    source: String,
    view: Option<String>,
    centre: P,
    diameter: f64,
}

fn sections(cx: &Ctx, b: &Builder) -> Vec<Section> {
    let mut out = Vec::new();
    for (i, e) in b.exprs.iter().enumerate() {
        if e.kind != "thread_convention" || !e.rule.starts_with("card:thread_representation/") {
            continue;
        }
        let thin: Vec<(P, P)> = b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id && m.member_role == "thread_line")
            .filter_map(|m| cx.lines.get(&m.source_handle).copied())
            .collect();
        let [(a1, z1), (a2, _)] = thin.as_slice() else {
            continue;
        };
        if dist(*a1, *z1) <= 1e-9 {
            continue;
        }
        let u = unit(sub(*z1, *a1));
        let d = cross(u, sub(*a2, *a1));
        if d.abs() <= 1e-9 {
            continue;
        }
        out.push(Section {
            expr: i,
            view: e.view_id.clone(),
            origin: add(*a1, mul([-u[1], u[0]], d / 2.)),
            dir: u,
            major: d.abs(),
        });
    }
    out
}

fn end_view_arcs(cx: &Ctx, b: &Builder) -> Vec<Arc> {
    let index: BTreeMap<&str, usize> = b
        .exprs
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    b.members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.member_role == "thread_line" && m.native_member_id.is_none())
        .filter_map(|(k, m)| {
            let i = *index.get(m.expression_id.as_str())?;
            let e = &b.exprs[i];
            // the thin arc is the major diameter of an internal thread only
            if e.kind != "thread_convention" || !e.qualifiers.iter().any(|q| q == "internal_thread")
            {
                return None;
            }
            let (c, r, ..) = cx.arc(&m.source_handle)?;
            Some(Arc {
                expr: i,
                member: k,
                source: m.source_handle.clone(),
                view: e.view_id.clone(),
                centre: c,
                diameter: 2. * r,
            })
        })
        .collect()
}

/// A cutting symbol: expression id, the view it stands in, the view it names and its strokes.
type CuttingLine = (String, Option<String>, String, Vec<(P, P)>);

/// The cutting symbols that name a view.
fn cutting_lines(cx: &Ctx, b: &Builder) -> Vec<CuttingLine> {
    b.exprs
        .iter()
        .filter(|e| e.kind == "view_callout")
        .filter_map(|e| {
            let target = e
                .evidence_ids
                .iter()
                .find_map(|x| x.strip_prefix("references_view:"))?;
            let strokes: Vec<(P, P)> = b
                .members
                .iter()
                .filter(|m| m.expression_id == e.id && m.member_role == "cutting_stroke")
                .map(|m| member_points(cx, m))
                .filter(|p| p.len() == 2 && dist(p[0], p[1]) > 1e-9)
                .map(|p| (p[0], p[1]))
                .collect();
            (!strokes.is_empty())
                .then(|| (e.id.clone(), e.view_id.clone(), target.to_string(), strokes))
        })
        .collect()
}

/// Link every end view to the section of the same thread in another view of its sheet.
fn link(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let card = representation::card_ref("thread_representation")?;
    let tol = representation::param(card, "same_thread_tolerance")?;
    let (secs, arcs, cuts) = (sections(cx, b), end_view_arcs(cx, b), cutting_lines(cx, b));
    for s in &secs {
        let t = tol * s.major;
        for a in &arcs {
            if a.view == s.view
                || sheet_of(a.view.as_ref()) != sheet_of(s.view.as_ref())
                || (a.diameter - s.major).abs() > t
            {
                continue;
            }
            let on_axis = cross(s.dir, sub(a.centre, s.origin)).abs() <= t;
            let on_cut = cuts.iter().find(|(_, at, target, strokes)| {
                at == &a.view
                    && Some(target) == s.view.as_ref()
                    && strokes
                        .iter()
                        .any(|(p, q)| cross(unit(sub(*q, *p)), sub(a.centre, *p)).abs() <= t)
            });
            let reason = match (on_axis, on_cut) {
                (true, _) => "end_view_on_section_axis".to_string(),
                (false, Some(c)) => format!("end_view_on_cutting_line:{}", c.0),
                _ => continue,
            };
            let (sid, aid) = (b.exprs[s.expr].id.clone(), b.exprs[a.expr].id.clone());
            let e = &mut b.exprs[s.expr];
            push(e, format!("same_thread_as:{aid}"));
            push(e, format!("end_view:{}", a.source));
            push(e, reason);
            push(e, "variant:same_thread_in_two_views".into());
            let e = &mut b.exprs[a.expr];
            push(e, format!("same_thread_as:{sid}"));
            push(e, "variant:same_thread_in_two_views".into());
            let m = &mut b.members[a.member];
            if !m
                .evidence_ids
                .iter()
                .any(|x| x == &format!("same_thread_as:{sid}"))
            {
                m.evidence_ids.push(format!("same_thread_as:{sid}"));
            }
        }
    }
    Ok(())
}

/// Name every thread convention by its view, form and sizes, and link the end view and the
/// section of one thread.
pub(super) fn state(cx: &Ctx, b: &mut Builder, regions: &[Region]) -> Result<()> {
    link(cx, b)?;
    let label_of = |view: Option<&String>| {
        regions
            .iter()
            .find(|r| Some(&r.id) == view)
            .map_or("未定视图".to_string(), view_layout::shown_label)
    };
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "thread_convention" {
            continue;
        }
        let view = label_of(b.exprs[i].view_id.as_ref());
        if b.exprs[i].rule.starts_with("card:thread_representation/") {
            let ev = &b.exprs[i].evidence_ids;
            let mut label = format!("{view} · 内螺纹（剖视）");
            if let (Some(d), Some(d1)) = (number(ev, "major:"), number(ev, "minor:")) {
                label.push_str(&format!(" 大径 {} 小径 {}", fmt_num(d), fmt_num(d1)));
            }
            if ev.iter().any(|x| x == "through_thread") {
                label.push_str(" 通孔");
            } else if ev.iter().any(|x| x == "blind_thread") {
                label.push_str(" 盲孔");
                if let Some(l) = number(ev, "thread_depth:") {
                    label.push_str(&format!(" 螺纹深 {}", fmt_num(l)));
                }
            }
            b.exprs[i].label = label;
            continue;
        }
        let form = group_form(&b.exprs[i]);
        let Some(name) = group_name(&form) else {
            continue;
        };
        // the holes of the group: one thin arc each, with the full circle of the same centre
        let id = b.exprs[i].id.clone();
        let size = |m: &MdExpressionMember, key: &str| number(&m.evidence_ids, key);
        let (mut majors, mut minors, mut holes) = (Vec::new(), Vec::new(), 0);
        for m in b.members.iter().filter(|m| m.expression_id == id) {
            if m.evidence_ids
                .iter()
                .any(|x| x.starts_with("duplicate_of:"))
            {
                continue;
            }
            holes += usize::from(m.member_role == "thread_line");
            majors.extend(size(m, "major:"));
            minors.extend(size(m, "minor:"));
        }
        let same = |v: &[f64]| {
            v.first()
                .filter(|x| v.iter().all(|y| (*y - **x).abs() <= 1e-6 * x.abs().max(1.)))
                .copied()
        };
        let mut label = format!("{view} · {name}");
        match (same(&majors), same(&minors)) {
            (Some(d), Some(d1)) => {
                label.push_str(&format!(" 大径 {} 小径 {}", fmt_num(d), fmt_num(d1)));
                if holes > 1 {
                    label.push_str(&format!(" ×{holes}"));
                }
            }
            _ if holes > 1 => label.push_str(&format!(" {holes} 处")),
            _ => {}
        }
        b.exprs[i].label = label;
    }
    Ok(())
}

// ---------------------------------------------------------------- a thread in section without line widths

/// Which sides of a stretch of line a section hatch covers: the stretch starts at `start` and runs
/// `length` along `u`; returns (the hatch covering the side at +n, the hatch covering the side at
/// −n), n = u turned left, each only when one and the same hatch covers it at every probe.
fn hatched_sides(
    start: P,
    u: P,
    length: f64,
    probe: f64,
    hatches: &[Vec<(P, P)>],
) -> [Option<usize>; 2] {
    let n = [-u[1], u[0]];
    let side = |s: f64| -> Option<usize> {
        let mut found: Option<usize> = None;
        for t in [0.25, 0.5, 0.75] {
            let p = add(add(start, mul(u, t * length)), mul(n, s * probe));
            let hit = hatches
                .iter()
                .position(|segs| representation::inside_segments(p, segs))?;
            if found.is_some_and(|f| f != hit) {
                return None;
            }
            found = Some(hit);
        }
        found
    };
    [side(1.), side(-1.)]
}

/// An internal thread in section when the drawing gives no line widths (card
/// `thread_representation`, variant `root_inside_hatch`; GB/T 4459.1-1995 3.5: in a section the
/// hatch is drawn to the thick line, whether the thread is external or internal). Of two pairs of
/// parallel lines symmetric about one axis (on which a centre line runs) and overlapping along it,
/// the outer pair lying inside one continuous hatched area of one hatch (hatched on both sides) is
/// the root, the major diameter; the inner pair bounding that area (hatched on the outer side, the
/// hole open on the inner side) is the crest, the minor diameter. Only when no width evidence
/// decides thick and thin (no lineweight classes, no checked layer convention).
pub(super) fn sections_by_hatch(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let w = line_evidence::Widths::of(cx);
    if w.weights_decide() || cx.conv.qualified {
        return Ok(());
    }
    let card = representation::card_ref("thread_representation")?;
    let rmin = representation::param(card, "section_ratio_min")?;
    let rmax = representation::param(card, "section_ratio_max")?;
    let probe = representation::param(card, "hatch_probe")?;
    let hatches: Vec<Vec<(P, P)>> = cx
        .rec
        .values()
        .filter(|r| r.kind == "HATCH" && int(r, 70) == 0)
        .map(|r| representation::hatch_boundary(r))
        .filter(|s| s.len() >= 3)
        .collect();
    if hatches.is_empty() {
        return Ok(());
    }
    let lines: Vec<(String, P, P)> = cx
        .lines
        .iter()
        .filter(|(h, (a, z))| {
            dist(*a, *z) > 1e-6
                && !b.used(h)
                && cx
                    .style
                    .get(*h)
                    .is_some_and(|s| s.pattern == Pattern::Continuous)
        })
        .map(|(h, (a, z))| (h.clone(), *a, *z))
        .collect();
    let axes: Vec<(P, P)> = cx
        .lines
        .iter()
        .filter(|(h, _)| {
            cx.style
                .get(*h)
                .is_some_and(|s| s.pattern == Pattern::DashDot)
        })
        .map(|(_, l)| *l)
        .collect();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (i, (t1, a1, z1)) in lines.iter().enumerate() {
        let u = unit(sub(*z1, *a1));
        let off = |p: P| cross(u, sub(p, *a1));
        let along = |p: P| dot(sub(p, *a1), u);
        let parallel = |a: P, z: P| cross(unit(sub(z, a)), u).abs() < 0.01;
        let span = |a: P, z: P| {
            let (s0, s1) = (along(a), along(z));
            (s0.min(s1), s0.max(s1))
        };
        let s1 = span(*a1, *z1);
        for (t2, a2, z2) in &lines[i + 1..] {
            if taken.contains(t1) || taken.contains(t2) || !parallel(*a2, *z2) {
                continue;
            }
            let big = off(*a2);
            if big.abs() < 1e-6 || (off(*z2) - big).abs() > 1e-3 * big.abs() {
                continue;
            }
            let s2 = span(*a2, *z2);
            let (lo, hi) = (s1.0.max(s2.0), s1.1.min(s2.1));
            if hi - lo < 0.5 * (s1.1 - s1.0).min(s2.1 - s2.0) {
                continue;
            }
            let mid = big / 2.;
            let tol = (0.02 * big.abs()).min(0.05 * cx.scale.max(1.));
            // a centre line on the middle line
            let Some(_) = axes.iter().find(|(p, q)| {
                parallel(*p, *q)
                    && (off(*p) - mid).abs() <= tol
                    && (off(*q) - mid).abs() <= tol
                    && span(*p, *q).1.min(hi) - span(*p, *q).0.max(lo) > 0.
            }) else {
                continue;
            };
            // the crest pair inside, symmetric about the middle line, in the card's ratio window
            let inner: Vec<&(String, P, P)> = lines
                .iter()
                .filter(|(k, p, q)| {
                    k != t1
                        && k != t2
                        && !taken.contains(k)
                        && parallel(*p, *q)
                        && (off(*q) - off(*p)).abs() <= tol
                        && (off(*p) - mid).abs() < big.abs() / 2. - tol
                        && span(*p, *q).1.min(hi) - span(*p, *q).0.max(lo) > 0.5 * (hi - lo)
                })
                .collect();
            let pair = inner.iter().find_map(|x| {
                inner.iter().find_map(|y| {
                    let (ox, oy) = (off(x.1) - mid, off(y.1) - mid);
                    let d = (ox - oy).abs();
                    (x.0 < y.0
                        && (ox + oy).abs() <= tol
                        && (rmin..=rmax).contains(&(d / big.abs())))
                    .then_some((*x, *y, d))
                })
            });
            let Some((k1, k2, d)) = pair else { continue };
            let gap = (big.abs() - d) / 2.;
            let probe_at = probe * gap;
            // the stretch of each line over the common span, probed on both sides (index 0 = the
            // side of larger offset)
            let sides = |a: P| {
                let start = add(*a1, add(mul(u, lo), mul([-u[1], u[0]], off(a))));
                hatched_sides(start, u, hi - lo, probe_at, &hatches)
            };
            let root = |a: P| match sides(a) {
                [Some(x), Some(y)] if x == y => Some(x),
                _ => None,
            };
            // the crest is hatched on the side away from the axis and open towards it
            let crest = |a: P| {
                let s = sides(a);
                let (outside, inside) = if off(a) > mid {
                    (s[0], s[1])
                } else {
                    (s[1], s[0])
                };
                outside.filter(|_| inside.is_none())
            };
            let (Some(h1), Some(h2)) = (root(*a1), root(*a2)) else {
                continue;
            };
            let (Some(c1), Some(c2)) = (crest(k1.1), crest(k2.1)) else {
                continue;
            };
            // one hatch: the area through the root is the area the crest bounds
            if !(h1 == c1 && h2 == c2) {
                continue;
            }
            let e = b.expr(
                "thread_convention",
                "内螺纹（剖视）",
                "card:thread_representation/internal_thread_in_section",
                "candidate",
                vec![],
                vec![
                    "card:thread_representation".into(),
                    "variant:internal_thread_in_section".into(),
                    "variant:root_inside_hatch".into(),
                    format!("major:{:.4}", big.abs()),
                    format!("minor:{d:.4}"),
                    "GB/T 4459.1".into(),
                    "section_hatch_stops_at_minor".into(),
                    "through_thread".into(),
                ],
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
            for h in [&k1.0, &k2.0] {
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
            for h in [t1, t2, &k1.0, &k2.0] {
                taken.insert(h.clone());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- threads in an assembly

/// Nominal size and length of a screw designation `M d × l` in a parts-list name (GB/T 193 and the
/// designations of the screw standards: thread letter, nominal diameter, a multiplication sign
/// and the nominal length).
fn designation(text: &str) -> Option<(String, f64, f64)> {
    let c: Vec<char> = text.chars().collect();
    let number = |from: usize| -> (usize, String) {
        let mut i = from;
        let mut s = String::new();
        while i < c.len() && (c[i].is_ascii_digit() || (c[i] == '.' && !s.is_empty())) {
            s.push(c[i]);
            i += 1;
        }
        (i, s)
    };
    (0..c.len()).filter(|&i| c[i] == 'M').find_map(|i| {
        let (j, d) = number(i + 1);
        let times = c.get(j).is_some_and(|x| matches!(x, '×' | 'x' | 'X' | '*'));
        let (k, l) = if times {
            number(j + 1)
        } else {
            (j, String::new())
        };
        let (d, l) = (d.parse::<f64>().ok()?, l.parse::<f64>().ok()?);
        Some((c[i..k].iter().collect(), d, l))
    })
}

/// A library part drawn with an external thread (card `thread_representation`, variant
/// `external_thread_in_view`; GB/T 4459.1-1995 3.1: the crest thick, the root thin and just
/// inside it), and the tapped hole it is screwed into (variant `assembled_threads`; 3.9: the
/// engaged part is drawn as the external thread, the rest of each by its own representation).
/// The part's thin pair, symmetric about its centre line inside its thick pair in the card's ratio
/// window, becomes its thread lines; a section of an internal thread whose major lines run along
/// the part's crest lines on the same axis is the hole it is engaged in, over the length the two
/// overlap. When the parts list names the part with a designation `M d × l`, its size is checked.
pub(super) fn assembled(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let card = representation::card_ref("thread_representation")?;
    let rmin = representation::param(card, "section_ratio_min")?;
    let rmax = representation::param(card, "section_ratio_max")?;
    let tol_ratio = representation::param(card, "same_thread_tolerance")?;
    let line_of = |m: &MdExpressionMember| -> Option<(P, P)> {
        let part = cx.blocks.part(m.native_member_id.as_deref()?)?;
        match &part.geom {
            annotation_blocks::Geom::Line(a, z) => Some((*a, *z)),
            annotation_blocks::Geom::Path(v, false) if v.len() == 2 => Some((v[0], v[1])),
            _ => None,
        }
    };
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "standard_part" {
            continue;
        }
        let id = b.exprs[i].id.clone();
        let ms: Vec<(usize, String, (P, P))> = b
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.expression_id == id)
            .filter_map(|(k, m)| line_of(m).map(|l| (k, m.member_role.clone(), l)))
            .filter(|(_, _, (a, z))| dist(*a, *z) > 1e-9)
            .collect();
        let Some((_, _, (p0, q0))) = ms.iter().find(|m| m.1 == "center_line") else {
            continue;
        };
        let u = unit(sub(*q0, *p0));
        let off = |p: P| cross(u, sub(p, *p0));
        let along = |p: P| dot(sub(p, *p0), u);
        let parallel = |(a, z): &(P, P)| cross(unit(sub(*z, *a)), u).abs() < 0.01;
        let level = |(a, z): &(P, P)| (off(*a) + off(*z)) / 2.;
        let span = |(a, z): &(P, P)| (along(*a).min(along(*z)), along(*a).max(along(*z)));
        let pairs = |role: &str| -> Vec<(usize, usize, f64, (f64, f64))> {
            let ls: Vec<&(usize, String, (P, P))> = ms
                .iter()
                .filter(|m| m.1 == role && parallel(&m.2))
                .collect();
            let mut out = Vec::new();
            for (x, a) in ls.iter().enumerate() {
                for c in &ls[x + 1..] {
                    let (oa, oc) = (level(&a.2), level(&c.2));
                    if oa * oc < 0. && (oa + oc).abs() <= tol_ratio * (oa - oc).abs() {
                        let (sa, sc) = (span(&a.2), span(&c.2));
                        let ov = (sa.0.max(sc.0), sa.1.min(sc.1));
                        if ov.1 > ov.0 {
                            out.push((a.0, c.0, (oa - oc).abs(), ov));
                        }
                    }
                }
            }
            out
        };
        let crests = pairs("contour_segment");
        let roots = pairs("native_member");
        let found = crests.iter().find_map(|c| {
            roots.iter().find_map(|r| {
                let q = r.2 / c.2;
                let ov = r.3.1.min(c.3.1) - r.3.0.max(c.3.0);
                ((rmin..=rmax).contains(&q) && ov > 0.5 * (c.3.1 - c.3.0)).then_some((c, r))
            })
        });
        let Some((crest, root)) = found else { continue };
        let (major, minor) = (crest.2, root.2);
        for k in [root.0, root.1] {
            b.members[k].member_role = "thread_line".into();
            b.members[k].line_type = "thin_continuous".into();
        }
        let ev = &mut b.exprs[i].evidence_ids;
        for x in [
            "card:thread_representation".to_string(),
            "variant:external_thread_in_view".into(),
            "concept:DC-G3-024".into(),
            format!("major:{}", fmt_num((major * 1e4).round() / 1e4)),
            format!("minor:{}", fmt_num((minor * 1e4).round() / 1e4)),
        ] {
            if !ev.contains(&x) {
                ev.push(x);
            }
        }
        // the part's length along its axis: the extent of its outline
        let (lo, hi) = ms
            .iter()
            .filter(|m| m.1 == "contour_segment")
            .map(|m| span(&m.2))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |a, s| {
                (a.0.min(s.0), a.1.max(s.1))
            });
        let length = hi - lo;
        // the designation of its parts-list row, as its label carries it
        let label = b.exprs[i].label.clone();
        if let Some((text, d, l)) = designation(&label) {
            let agrees = (d - major).abs() <= tol_ratio * d && (l - length).abs() <= tol_ratio * l;
            let x = format!(
                "{}:{text}",
                if agrees {
                    "designation_agrees"
                } else {
                    "designation_differs"
                }
            );
            if !b.exprs[i].evidence_ids.contains(&x) {
                b.exprs[i].evidence_ids.push(x);
            }
        }
        // the tapped hole it is screwed into: an internal thread in section on the same axis whose
        // major lines run along the part's crest lines
        let t = tol_ratio * major;
        let hole = b.exprs.iter().position(|e| {
            e.kind == "thread_convention" && e.rule.starts_with("card:thread_representation/") && {
                let ls: Vec<(P, P)> = b
                    .members
                    .iter()
                    .filter(|m| m.expression_id == e.id && m.member_role == "thread_line")
                    .filter_map(|m| cx.lines.get(&m.source_handle).copied())
                    .collect();
                ls.len() == 2
                    && ls
                        .iter()
                        .all(|l| parallel(l) && (level(l).abs() - major / 2.).abs() <= t)
                    && level(&ls[0]) * level(&ls[1]) < 0.
                    && ls
                        .iter()
                        .all(|l| span(l).1.min(hi) - span(l).0.max(lo) > 0.)
            }
        });
        let Some(h) = hole else { continue };
        let engaged = b
            .members
            .iter()
            .filter(|m| m.expression_id == b.exprs[h].id && m.member_role == "thread_line")
            .filter_map(|m| cx.lines.get(&m.source_handle).copied())
            .map(|l| span(&l).1.min(hi) - span(&l).0.max(lo))
            .fold(f64::INFINITY, f64::min);
        let (hid, sid) = (b.exprs[h].id.clone(), id.clone());
        let len = fmt_num((engaged * 1e4).round() / 1e4);
        for (own, other) in [(h, &sid), (i, &hid)] {
            let ev = &mut b.exprs[own].evidence_ids;
            for x in [
                format!("engaged_with:{other}"),
                format!("engaged_length:{len}"),
                "card:thread_representation".into(),
                "variant:assembled_threads".into(),
                "concept:DC-G3-056".into(),
            ] {
                if !ev.contains(&x) {
                    ev.push(x);
                }
            }
        }
    }
    Ok(())
}
