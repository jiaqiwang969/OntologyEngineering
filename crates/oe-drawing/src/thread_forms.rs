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

/// Cutting symbols: expression id, the view they stand in, the view they name and their strokes.
fn cutting_lines(cx: &Ctx, b: &Builder) -> Vec<(String, Option<String>, String, Vec<(P, P)>)> {
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
            (!strokes.is_empty()).then(|| {
                (
                    e.id.clone(),
                    e.view_id.clone(),
                    target.to_string(),
                    strokes,
                )
            })
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
            if !m.evidence_ids.iter().any(|x| x == &format!("same_thread_as:{sid}")) {
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
            .map_or("未定视图", |r| r.label.as_str())
            .to_string()
    };
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "thread_convention" {
            continue;
        }
        let view = label_of(b.exprs[i].view_id.as_ref());
        if b.exprs[i]
            .rule
            .starts_with("card:thread_representation/")
        {
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
            if m.evidence_ids.iter().any(|x| x.starts_with("duplicate_of:")) {
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
