//! Terminators of native dimensions whatever entity stores them (card `dimension_line_parts`,
//! variant `terminator_any_entity`; DC-G4-019; GB/T 4458.4-2003 4.2.1).
//!
//! A dimension line ends in an arrow. In CAD data the arrow may be a SOLID, a TRACE, a block
//! reference, a tapered polyline or a hatch filled solid whose boundary is a slender triangle. A
//! filled triangle inside a dimension whose tip lies on one of the dimension's own dimension
//! lines (straight or arc) and whose axis runs along that line is the dimension's terminator. A
//! block reference inside the dimension that draws nothing but text is part of the value, not a
//! terminator.
use super::*;

const CARD: &str = "dimension_line_parts";
const VARIANT: &str = "terminator_any_entity";

/// A filled triangle: tip, unit direction from the base to the tip, length and base width. The
/// base is the shortest side; the tip lies on its perpendicular bisector.
fn triangle(pts: &[P]) -> Option<(P, P, f64, f64)> {
    let mut v: Vec<P> = Vec::new();
    for p in pts {
        if !v.iter().any(|q| dist(*q, *p) < 1e-9) {
            v.push(*p);
        }
    }
    if v.len() != 3 {
        return None;
    }
    let (i, j, k) = [(0, 1, 2), (1, 2, 0), (2, 0, 1)]
        .into_iter()
        .min_by(|a, b| dist(v[a.0], v[a.1]).total_cmp(&dist(v[b.0], v[b.1])))?;
    let (base, width, tip) = (mid(v[i], v[j]), dist(v[i], v[j]), v[k]);
    let len = dist(tip, base);
    let symmetric = (dist(tip, v[i]) - dist(tip, v[j])).abs() <= 0.1 * len;
    (width > 1e-9 && len > 1e-9 && symmetric).then(|| (tip, unit(sub(tip, base)), len, width))
}

/// The terminator a nested entity draws, in the block's frame shifted by `off`: a hatch filled
/// solid whose boundary is a slender triangle, or a polyline tapering to a point.
fn shape(x: &Record, off: P, aspect: (f64, f64)) -> Option<Arrow> {
    let a = match x.kind.as_str() {
        "HATCH" if int(x, 70) & 1 != 0 => {
            let pts: Vec<P> = representation::hatch_boundary(x)
                .into_iter()
                .flat_map(|(p, q)| [p, q])
                .collect();
            triangle(&pts)
                .filter(|(_, _, len, w)| (aspect.0..=aspect.1).contains(&(len / w)))
                .map(|(tip, dir, len, _)| Arrow { tip, dir, len })
        }
        "LWPOLYLINE" => tapered_arrow(x),
        _ => None,
    }?;
    Some(Arrow {
        tip: add(a.tip, off),
        ..a
    })
}

/// A dimension line of the block in drawing coordinates.
enum Carrier {
    Line(P, P),
    /// Centre, radius, start angle and counter-clockwise sweep (radians).
    Arc(P, f64, f64, f64),
}

impl Carrier {
    fn of(x: &Record, off: P) -> Option<Self> {
        match x.kind.as_str() {
            "LINE" => Some(Carrier::Line(add(x.xy(10).ok()?, off), add(x.xy(11).ok()?, off))),
            "ARC" => {
                let s0 = x.number(50).ok()?.to_radians();
                let s1 = x.number(51).ok()?.to_radians();
                Some(Carrier::Arc(
                    add(x.xy(10).ok()?, off),
                    x.number(40).ok()?,
                    s0,
                    (s1 - s0).rem_euclid(std::f64::consts::TAU),
                ))
            }
            _ => None,
        }
    }
    /// The arrow's tip lies on the line within `tol` and its axis runs along it.
    fn ends(&self, a: &Arrow, tol: f64, sin_max: f64) -> bool {
        match self {
            Carrier::Line(p, q) => {
                on_seg(a.tip, *p, *q, tol).is_some() && cross(a.dir, unit(sub(*q, *p))).abs() <= sin_max
            }
            Carrier::Arc(c, r, s0, sw) => {
                let ang = ((a.tip[1] - c[1]).atan2(a.tip[0] - c[0]) - s0)
                    .rem_euclid(std::f64::consts::TAU);
                let slack = tol / r.max(1e-9);
                (dist(a.tip, *c) - r).abs() <= tol
                    && (ang <= sw + slack || std::f64::consts::TAU - ang <= slack)
                    && dot(a.dir, unit(sub(a.tip, *c))).abs() <= sin_max
            }
        }
    }
}

/// Give the native dimension `e` (root `h`, block shifted by `off`) the terminators its block
/// stores as a filled hatch or a tapered polyline, and take block references that draw only text
/// out of the terminators. Returns the terminators found, in drawing coordinates.
pub(super) fn native(cx: &Ctx, b: &mut Builder, e: usize, h: &str, off: P) -> Result<Vec<Arrow>> {
    let c = representation::card_ref(CARD)?;
    let aspect = (
        representation::param(c, "terminator_aspect_min")?,
        representation::param(c, "terminator_aspect_max")?,
    );
    let on = representation::param(c, "terminator_on_line")?;
    let sin_max = representation::param(c, "terminator_axis_sin_max")?;
    let eid = b.exprs[e].id.clone();
    let nested: BTreeMap<&str, &oe_contracts::MdNativeLayerMember> = cx
        .src
        .members
        .iter()
        .filter(|m| m.block_path.first().map(String::as_str) == Some(h))
        .map(|m| (m.id.as_str(), m))
        .collect();
    let record = |i: usize| {
        b.members[i]
            .native_member_id
            .as_deref()
            .and_then(|n| nested.get(n))
            .and_then(|m| cx.block_rec.get(&m.handle).copied())
    };
    let mine: Vec<usize> = (0..b.members.len())
        .filter(|&i| b.members[i].expression_id == eid && b.members[i].native_member_id.is_some())
        .collect();
    let carriers: Vec<Carrier> = mine
        .iter()
        .filter(|&&i| b.members[i].member_role == "dimension_line")
        .filter_map(|&i| record(i).and_then(|x| Carrier::of(x, off)))
        .collect();
    let parts: Vec<(usize, &Record)> = mine
        .iter()
        .filter_map(|&i| record(i).map(|x| (i, x)))
        .collect();
    let mut found = Vec::new();
    let mut named = false;
    for (i, x) in parts {
        if b.members[i].member_role == "native_member" {
            let Some(a) = shape(x, off, aspect) else {
                continue;
            };
            if !carriers.iter().any(|l| l.ends(&a, on * a.len, sin_max)) {
                continue;
            }
            let m = &mut b.members[i];
            m.member_role = "arrowhead".into();
            for v in [format!("card:{CARD}"), format!("variant:{VARIANT}")] {
                if !m.evidence_ids.contains(&v) {
                    m.evidence_ids.push(v);
                }
            }
            found.push(a);
            named = true;
        } else if b.members[i].member_role == "arrowhead" && x.kind == "INSERT" {
            // a block reference that draws only text is a piece of the value
            let path = &nested[b.members[i].native_member_id.as_deref().unwrap_or_default()].block_path;
            let inside: Vec<&str> = cx
                .src
                .members
                .iter()
                .filter(|m| m.block_path.len() > path.len() && m.block_path.starts_with(path))
                .map(|m| m.kind.as_str())
                .collect();
            let text = |k: &str| matches!(k, "TEXT" | "MTEXT" | "ATTRIB" | "ATTDEF");
            if inside.iter().any(|k| text(k)) && inside.iter().all(|k| text(k) || *k == "INSERT") {
                let m = &mut b.members[i];
                m.member_role = "dimension_text".into();
                m.evidence_ids.push(format!("card:{CARD}"));
                m.evidence_ids.push("block_draws_only_text".into());
            }
        }
    }
    if named {
        for v in [
            format!("card:{CARD}"),
            format!("variant:{VARIANT}"),
            "concept:DC-G4-019".to_string(),
        ] {
            if !b.exprs[e].evidence_ids.contains(&v) {
                b.exprs[e].evidence_ids.push(v);
            }
        }
    }
    Ok(found)
}
