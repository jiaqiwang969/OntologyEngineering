//! Drafting representation knowledge read when recognition runs.
//!
//! `knowledge/drawing-representation.toml` holds one card per drafting concept: meaning,
//! components, the ways it is drawn, counter-examples, sources and geometric relations as
//! drawing-domain ratios. The card types come from the ggen contract (`MdRepresentation*`).
//! The rules below take their parameters and markers from the cards and name the card (and
//! the variant) in every expression or member they produce, so a reading can be traced to
//! the discipline knowledge behind it. Expert corrections are sources of the knowledge,
//! never an input: nothing here reads a reference, an old label or a real drawing.
use super::*;
use oe_contracts::{MdRepresentationCard, MdRepresentationKnowledge};
use std::sync::OnceLock;

const SOURCE: &str = include_str!("../knowledge/drawing-representation.toml");
const SCHEMA_K: &str = "oev2.drawing-representation-knowledge/1";

/// Cards that recognition uses; a test checks this list against the file both ways.
pub const USED_CARDS: [&str; 10] = [
    "datum_feature_symbol",
    "center_hole_symbol",
    "gdt_frame_symbol",
    "tolerance_zone_modifier",
    "special_characteristic_mark",
    "angle_unit",
    "gdt_leader_extension",
    "section_line_region",
    "detail_circle_interruption",
    "visible_edge_line",
];

/// Concept catalogue: one card per drafting representation concept, by group.
const CONCEPT_FILES: [(&str, &str); 9] = [
    ("G1", include_str!("../knowledge/concepts/G1.toml")),
    ("G2", include_str!("../knowledge/concepts/G2.toml")),
    ("G3", include_str!("../knowledge/concepts/G3.toml")),
    ("G4", include_str!("../knowledge/concepts/G4.toml")),
    ("G5", include_str!("../knowledge/concepts/G5.toml")),
    ("G6", include_str!("../knowledge/concepts/G6.toml")),
    ("G7", include_str!("../knowledge/concepts/G7.toml")),
    ("G8", include_str!("../knowledge/concepts/G8.toml")),
    ("G9", include_str!("../knowledge/concepts/G9.toml")),
];
const SCHEMA_C: &str = "oev2.drawing-concept-catalogue/1";

/// The parsed and checked concept catalogue; every concept card is complete, ids are unique and
/// recognition cards and concept cards name each other.
pub fn drawing_concepts() -> Result<&'static Vec<oe_contracts::MdDrawingConceptGroup>> {
    static K: OnceLock<std::result::Result<Vec<oe_contracts::MdDrawingConceptGroup>, String>> = OnceLock::new();
    K.get_or_init(|| {
        let cards = representation_knowledge().map_err(|e| e.to_string())?;
        let mut bad: Vec<String> = Vec::new();
        let mut groups = Vec::new();
        let mut ids = BTreeSet::new();
        let mut linked: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (g, text) in CONCEPT_FILES {
            let grp: oe_contracts::MdDrawingConceptGroup =
                toml::from_str(text).map_err(|e| format!("drawing_concept_parse:{g}: {e}"))?;
            bad.extend(grp.validate().err().unwrap_or_default());
            if grp.schema != SCHEMA_C || grp.group != g {
                bad.push(format!("group_header:{g}"));
            }
            for c in &grp.concepts {
                bad.extend(c.validate().err().unwrap_or_default());
                for s in &c.sources {
                    bad.extend(s.validate().err().unwrap_or_default());
                }
                if !ids.insert(c.id.clone()) || c.group != g || !c.id.starts_with(&format!("DC-{g}-")) {
                    bad.push(format!("concept_id:{}", c.id));
                }
                if c.meaning.trim().is_empty() || c.components.is_empty() || c.dxf_forms.is_empty() || c.recognition.trim().is_empty() || c.sources.is_empty() {
                    bad.push(format!("incomplete_concept:{}", c.id));
                }
                for r in &c.recognition_cards {
                    linked.entry(r.clone()).or_default().insert(c.id.clone());
                }
                if (c.recognizer == "card") != !c.recognition_cards.is_empty() {
                    bad.push(format!("recognizer_status:{}", c.id));
                }
            }
            groups.push(grp);
        }
        for card in &cards.cards {
            let named: BTreeSet<String> = card.concepts.iter().cloned().collect();
            if named.is_empty() || named.iter().any(|x| !ids.contains(x)) || linked.get(&card.id).cloned().unwrap_or_default() != named {
                bad.push(format!("card_concept_link:{}", card.id));
            }
        }
        for r in linked.keys() {
            if !cards.cards.iter().any(|c| &c.id == r) {
                bad.push(format!("unknown_recognition_card:{r}"));
            }
        }
        if bad.is_empty() {
            Ok(groups)
        } else {
            Err(format!("drawing_concept_contract: {}", bad.join(",")))
        }
    })
    .as_ref()
    .map_err(|e| anyhow::anyhow!(e.clone()))
}

/// The parsed and checked knowledge (parsed once per process).
pub fn representation_knowledge() -> Result<&'static MdRepresentationKnowledge> {
    static K: OnceLock<std::result::Result<MdRepresentationKnowledge, String>> = OnceLock::new();
    K.get_or_init(|| {
        let k: MdRepresentationKnowledge =
            toml::from_str(SOURCE).map_err(|e| format!("drawing_representation_knowledge_parse: {e}"))?;
        let mut bad: Vec<String> = k.validate().err().unwrap_or_default();
        if k.schema != SCHEMA_K || k.profile != PROFILE {
            bad.push("schema_or_profile".into());
        }
        let mut ids = BTreeSet::new();
        for c in &k.cards {
            bad.extend(c.validate().err().unwrap_or_default());
            if !ids.insert(c.id.clone()) {
                bad.push(format!("duplicate_card:{}", c.id));
            }
            if c.meaning.trim().is_empty()
                || c.components.is_empty()
                || c.variants.is_empty()
                || c.counter_examples.is_empty()
                || c.sources.is_empty()
            {
                bad.push(format!("incomplete_card:{}", c.id));
            }
            for x in &c.components {
                bad.extend(x.validate().err().unwrap_or_default());
            }
            for v in &c.variants {
                bad.extend(v.validate().err().unwrap_or_default());
                if v.tests.is_empty() {
                    bad.push(format!("variant_without_test:{}:{}", c.id, v.id));
                }
            }
            for x in &c.counter_examples {
                bad.extend(x.validate().err().unwrap_or_default());
            }
            for x in &c.parameters {
                bad.extend(x.validate().err().unwrap_or_default());
            }
        }
        for u in USED_CARDS {
            if !ids.contains(u) {
                bad.push(format!("used_card_missing:{u}"));
            }
        }
        if bad.is_empty() {
            Ok(k)
        } else {
            Err(format!("drawing_representation_knowledge_contract: {}", bad.join(",")))
        }
    })
    .as_ref()
    .map_err(|e| anyhow::anyhow!(e.clone()))
}

fn card(id: &str) -> Result<&'static MdRepresentationCard> {
    representation_knowledge()?
        .cards
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| anyhow::anyhow!("drawing_representation_card_missing: {id}"))
}

fn param(c: &MdRepresentationCard, name: &str) -> Result<f64> {
    c.parameters
        .iter()
        .find(|p| p.name == name)
        .map(|p| p.value)
        .filter(|v| v.is_finite())
        .ok_or_else(|| anyhow::anyhow!("drawing_representation_parameter_missing: {}.{name}", c.id))
}

fn tag(c: &MdRepresentationCard) -> String {
    format!("card:{}", c.id)
}
fn variant_tag(v: &str) -> String {
    format!("variant:{v}")
}

fn seg_d(p: P, a: P, b: P) -> f64 {
    let v = sub(b, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), v) / n).clamp(0., 1.);
    dist(p, add(a, mul(v, t)))
}

fn is_letter(s: &str) -> bool {
    let t = s.trim();
    t.chars().count() == 1 && t.chars().all(|c| c.is_ascii_uppercase())
}

/// Segments of a straight LINE root or a two-vertex open polyline.
fn straight(cx: &Ctx, id: &str) -> Option<(P, P)> {
    if let Some(l) = cx.lines.get(id) {
        return Some(*l);
    }
    cx.polys
        .get(id)
        .filter(|(p, closed, bulged)| p.len() == 2 && !closed && !bulged)
        .map(|(p, _, _)| (p[0], p[1]))
}

fn solid_points(cx: &Ctx, id: &str) -> Option<Vec<P>> {
    let r = cx.rec.get(id)?;
    if !matches!(r.kind.as_str(), "SOLID" | "TRACE") {
        return None;
    }
    let mut pts: Vec<P> = Vec::new();
    for c in 10..=13 {
        if let Ok(p) = r.xy(c)
            && !pts.iter().any(|q| dist(*q, p) < 1e-9)
        {
            pts.push(p);
        }
    }
    (pts.len() == 3).then_some(pts)
}

// ---------------------------------------------------------------- datum feature symbol

enum Boundary {
    Circle(String, P, f64),
    Square(String, [f64; 4]),
}
impl Boundary {
    fn id(&self) -> &str {
        match self {
            Boundary::Circle(h, ..) | Boundary::Square(h, _) => h,
        }
    }
    fn center(&self) -> P {
        match self {
            Boundary::Circle(_, c, _) => *c,
            Boundary::Square(_, b) => mid([b[0], b[1]], [b[2], b[3]]),
        }
    }
    fn half(&self) -> f64 {
        match self {
            Boundary::Circle(_, _, r) => *r,
            Boundary::Square(_, b) => 0.5 * (b[2] - b[0]).max(b[3] - b[1]),
        }
    }
    fn gap(&self, p: P) -> f64 {
        match self {
            Boundary::Circle(_, c, r) => (dist(p, *c) - r).abs(),
            Boundary::Square(_, b) => {
                let corners = [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]];
                (0..4)
                    .map(|i| seg_d(p, corners[i], corners[(i + 1) % 4]))
                    .fold(f64::INFINITY, f64::min)
            }
        }
    }
}

/// Datum feature symbols drawn with loose entities: letter, circle or square, connector
/// and terminal (old thick short bar or current triangle). Block symbols are read by
/// `blocks()`; this rule names the card on them too.
pub(super) fn datum_symbols(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = card("datum_feature_symbol")?;
    let off = param(c, "letter_offset_max")?;
    let (bmin, bmax) = (
        param(c, "boundary_to_text_height_min")?,
        param(c, "boundary_to_text_height_max")?,
    );
    let on_b = param(c, "connector_on_boundary")?;
    let lmax = param(c, "connector_length_max")?;
    let (tmin, tmax) = (param(c, "terminal_length_min")?, param(c, "terminal_length_max")?);
    let tmid = param(c, "terminal_mid_offset")?;
    let perp = param(c, "perpendicular_cos_max")?;
    let ttouch = param(c, "triangle_touch")?;
    let aspect = param(c, "triangle_aspect_max")?;
    let cells = frame_rectangles(cx, b);
    // Name the card on block symbols (variant old_circle_bar, block encoding).
    for e in b.exprs.iter_mut().filter(|e| e.kind == "datum_symbol") {
        if !e.evidence_ids.contains(&tag(c)) {
            e.evidence_ids.push(tag(c));
            e.evidence_ids.push(variant_tag("old_circle_bar"));
            e.evidence_ids.push("encoding:block".into());
        }
    }
    let letters: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id) && is_letter(&t.dec.text))
        .cloned()
        .collect();
    for t in letters {
        if b.used(&t.id) {
            continue;
        }
        let mut bounds: Vec<Boundary> = Vec::new();
        for (h, r) in &cx.rec {
            if r.kind == "CIRCLE"
                && !b.used(h)
                && let Some((ctr, rad, ..)) = cx.arc(h)
                && rad >= bmin * t.height
                && rad <= bmax * t.height
                && dist(ctr, t.center) <= off * rad
            {
                bounds.push(Boundary::Circle(h.clone(), ctr, rad));
            }
        }
        for (h, (pts, _, bulged)) in &cx.polys {
            if !*bulged
                && !b.used(h)
                && let Some(rb) = cell_rect(pts)
            {
                let (w, hh) = (rb[2] - rb[0], rb[3] - rb[1]);
                let half = 0.5 * w.max(hh);
                // A square that shares a side with another cell is a frame cell, not a datum box.
                let adjacent = cells.iter().any(|(ids, r)| {
                    !ids.contains(h)
                        && r != &rb
                        && ((r[0] - rb[2]).abs() < 1e-3 * w || (r[2] - rb[0]).abs() < 1e-3 * w)
                        && (r[1] - rb[1]).abs() < 1e-3 * hh
                        && (r[3] - rb[3]).abs() < 1e-3 * hh
                });
                if !adjacent
                    && (w - hh).abs() <= 0.2 * w.max(hh)
                    && half >= bmin * t.height
                    && half <= bmax * t.height
                    && dist(mid([rb[0], rb[1]], [rb[2], rb[3]]), t.center) <= off * half
                {
                    bounds.push(Boundary::Square(h.clone(), rb));
                }
            }
        }
        bounds.sort_by(|x, y| dist(x.center(), t.center).total_cmp(&dist(y.center(), t.center)));
        'bound: for bd in &bounds {
            let r = bd.half();
            for (cid, (p, q)) in &cx.lines {
                if b.used(cid) || *cid == bd.id() || dist(*p, *q) > lmax * r || dist(*p, *q) < 0.2 * r {
                    continue;
                }
                for (near, far) in [(*p, *q), (*q, *p)] {
                    if bd.gap(near) > on_b * r || dist(far, bd.center()) <= dist(near, bd.center()) {
                        continue;
                    }
                    let u = unit(sub(far, near));
                    // Old edition: a short bar across the connector end.
                    let bar = cx
                        .lines
                        .keys()
                        .chain(cx.polys.keys())
                        .filter(|h| !b.used(h) && *h != cid && *h != bd.id())
                        .filter_map(|h| straight(cx, h).map(|s| (h.clone(), s)))
                        .find(|(_, (a, z))| {
                            let l = dist(*a, *z);
                            l >= tmin * r
                                && l <= tmax * r
                                && dot(unit(sub(*z, *a)), u).abs() <= perp
                                && dist(mid(*a, *z), far) <= tmid * l
                                && seg_d(far, *a, *z) <= 0.05 * l
                        });
                    let (term, variant, term_lt) = if let Some((h, _)) = bar {
                        (h, "old_circle_bar", "thick_continuous")
                    } else if let Some(h) = cx.rec.keys().find(|h| {
                        !b.used(h)
                            && solid_points(cx, h).is_some_and(|tri| {
                                let sides: Vec<f64> = (0..3).map(|i| dist(tri[i], tri[(i + 1) % 3])).collect();
                                let (smin, smax) = sides.iter().fold((f64::INFINITY, 0f64), |a, x| (a.0.min(*x), a.1.max(*x)));
                                smax <= aspect * smin
                                    && (tri.iter().any(|v| dist(*v, far) <= ttouch * r)
                                    || (0..3).any(|i| dist(mid(tri[i], tri[(i + 1) % 3]), far) <= ttouch * r))
                            })
                    }) {
                        (h.clone(), "new_square_triangle", "fill")
                    } else {
                        continue;
                    };
                    let e = b.expr(
                        "datum_symbol",
                        t.dec.text.trim(),
                        &format!("{}/{variant}", tag(c)),
                        "candidate",
                        vec![],
                        vec![tag(c), variant_tag(variant), "encoding:loose".into()],
                        vec!["Datum feature requires the attached feature".into()],
                    );
                    let ev = vec![tag(c)];
                    b.member(e, &t.id, None, [0., 1.], "datum_letter", "text", "candidate", vec![], ev.clone());
                    b.member(e, bd.id(), None, [0., 1.], "datum_frame", "thin_continuous", "candidate", vec![], ev.clone());
                    b.member(e, cid, None, [0., 1.], "leader_line", "thin_continuous", "candidate", vec![], ev.clone());
                    b.member(e, &term, None, [0., 1.], "datum_terminal", term_lt, "candidate", vec![], ev);
                    break 'bound;
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- centre hole symbol

/// GB/T 4459.5 centre-hole symbol: a 60° V with its apex where an end face meets a centre
/// line, opening away from the part; the designation note written along the axis.
pub(super) fn center_holes(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = card("center_hole_symbol")?;
    let s = cx.scale.max(1e-6);
    let ang = param(c, "included_angle_deg")?;
    let atol = param(c, "angle_tolerance_deg")?;
    let (lmin, lmax) = (param(c, "arm_length_min")? * s, param(c, "arm_length_max")? * s);
    let ratio = param(c, "arm_ratio_max")?;
    let tol = param(c, "apex_tolerance")? * s;
    let axis_cross = param(c, "axis_cross_max")?;
    let reach = param(c, "note_reach")?;
    let note_off = param(c, "note_offset_max")?;
    let axes: Vec<(P, P)> = cx
        .lines
        .iter()
        .filter(|(h, _)| cx.style.get(*h).is_some_and(|st| st.pattern == Pattern::DashDot))
        .map(|(_, l)| *l)
        .collect();
    let structural: Vec<(String, P, P)> = cx
        .lines
        .iter()
        .filter(|(h, _)| cx.structural_style(h))
        .map(|(h, (a, z))| (h.clone(), *a, *z))
        .collect();
    let arms: Vec<(String, P, P)> = cx
        .lines
        .iter()
        .filter(|(h, (a, z))| !b.used(h) && dist(*a, *z) >= lmin && dist(*a, *z) <= lmax)
        .filter(|(h, _)| cx.style.get(*h).is_some_and(|st| matches!(st.pattern, Pattern::Continuous | Pattern::ByBlock)))
        .map(|(h, (a, z))| (h.clone(), *a, *z))
        .collect();
    let mut done: BTreeSet<String> = BTreeSet::new();
    for i in 0..arms.len() {
        for j in (i + 1)..arms.len() {
            let (h1, h2) = (&arms[i].0, &arms[j].0);
            if done.contains(h1) || done.contains(h2) {
                continue;
            }
            // Shared apex.
            let mut apex = None;
            for (p, q) in [(arms[i].1, arms[i].2), (arms[i].2, arms[i].1)] {
                for (r, w) in [(arms[j].1, arms[j].2), (arms[j].2, arms[j].1)] {
                    if dist(p, r) <= tol {
                        apex = Some((mid(p, r), q, w));
                    }
                }
            }
            let Some((a, f1, f2)) = apex else { continue };
            let (l1, l2) = (dist(a, f1), dist(a, f2));
            if l1.max(l2) > ratio * l1.min(l2) {
                continue;
            }
            let (u1, u2) = (unit(sub(f1, a)), unit(sub(f2, a)));
            let inc = dot(u1, u2).clamp(-1., 1.).acos().to_degrees();
            if (inc - ang).abs() > atol {
                continue;
            }
            let u = unit(add(u1, u2));
            // The bisector lies on a centre line through the apex.
            let on_axis = axes.iter().any(|(p, q)| {
                cross(unit(sub(*q, *p)), u).abs() <= axis_cross && project(a, *p, *q).1 <= tol
            });
            if !on_axis {
                continue;
            }
            // An end face through the apex, perpendicular to the axis, whose ends lead into
            // the part on the side away from the opening.
            let face = structural.iter().find(|(h, p, q)| {
                h != h1 && h != h2 && seg_d(a, *p, *q) <= tol && dot(unit(sub(*q, *p)), u).abs() <= axis_cross
            });
            let Some((fh, fp, fq)) = face else { continue };
            let mut side = 0.;
            for end in [*fp, *fq] {
                for (h, p, q) in &structural {
                    if h == fh || h == h1 || h == h2 {
                        continue;
                    }
                    for (x, y) in [(*p, *q), (*q, *p)] {
                        if dist(x, end) <= tol {
                            side += dot(sub(y, x), u);
                        }
                    }
                }
            }
            if side >= 0. {
                continue;
            }
            // Not-allowed variant: a short bar across the V near the apex, outside the face.
            let bar = cx.lines.iter().find(|(h, (p, q))| {
                !b.used(h)
                    && *h != h1
                    && *h != h2
                    && *h != fh
                    && dot(unit(sub(*q, *p)), u).abs() <= axis_cross
                    && dist(*p, *q) <= 1.5 * l1.max(l2)
                    && {
                        let m = mid(*p, *q);
                        let t = dot(sub(m, a), u);
                        t > tol && t < 0.6 * l1.max(l2) && cross(u, sub(m, a)).abs() <= 0.2 * l1.max(l2)
                    }
            });
            let variant = if bar.is_some() { "not_allowed" } else { "keep" };
            // Designation note along the axis beyond the symbol.
            let far = mid(f1, f2);
            let refline = cx.lines.iter().find(|(h, (p, q))| {
                !b.used(h)
                    && *h != h1
                    && *h != h2
                    && cross(unit(sub(*q, *p)), u).abs() <= axis_cross
                    && project(a, *p, *q).1 <= 0.5 * s
                    && {
                        let (tp, tq) = (dot(sub(*p, a), u), dot(sub(*q, a), u));
                        tp.min(tq) > 0. && tp.min(tq) - dot(sub(far, a), u) <= reach * l1.max(l2)
                    }
            });
            let mut notes: Vec<String> = Vec::new();
            if let Some((_, (p, q))) = refline {
                for t in cx.texts.values() {
                    let along = project(t.center, *p, *q).0;
                    if !b.used(&t.id) && (0. ..=1.).contains(&along) && seg_d(t.center, *p, *q) <= note_off * t.height.max(s) {
                        notes.push(t.id.clone());
                    }
                }
            }
            let designated = notes.iter().any(|id| {
                let txt = &cx.texts[id].dec.text;
                c.markers.iter().any(|m| txt.contains(m.as_str()))
            });
            if !designated {
                notes.clear();
            }
            // Read the note lines from top to bottom (left to right for a vertical axis).
            let up = if u[0].abs() >= u[1].abs() { [0., 1.] } else { [-1., 0.] };
            notes.sort_by(|x, y| dot(cx.texts[y].center, up).total_cmp(&dot(cx.texts[x].center, up)));
            let label = notes
                .iter()
                .map(|id| cx.texts[id].dec.text.trim().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            let e = b.expr(
                "center_hole",
                if label.is_empty() { "中心孔符号" } else { label.as_str() },
                &format!("{}/{variant}", tag(c)),
                "candidate",
                vec![format!("center_hole:{variant}")],
                vec![tag(c), variant_tag(variant), format!("end_face:{fh}")],
                if notes.is_empty() {
                    vec!["Centre-hole designation not found near the symbol".into()]
                } else {
                    vec![]
                },
            );
            let ev = vec![tag(c)];
            for h in [h1, h2] {
                b.member(e, h, None, [0., 1.], "symbol_glyph", "thin_continuous", "candidate", vec![], ev.clone());
                done.insert(h.clone());
            }
            if let Some((h, _)) = bar {
                b.member(e, h, None, [0., 1.], "symbol_glyph", "thin_continuous", "candidate", vec![], ev.clone());
                done.insert(h.clone());
            }
            if !notes.is_empty()
                && let Some((h, _)) = refline
            {
                b.member(e, h, None, [0., 1.], "leader_line", "thin_continuous", "candidate", vec![], ev.clone());
            }
            for id in &notes {
                b.member(e, id, None, [0., 1.], "note_text", "text", "candidate", vec![], ev.clone());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- tolerance frames

/// Rectangular cells whose sides are thin LINE roots or edges of rectangular polylines
/// (a frame drawn partly with lines next to a polyline cell). Ids are the LINE roots;
/// cells made only of one polyline are left to the polyline itself.
pub(super) fn frame_rectangles(cx: &Ctx, b: &Builder) -> Vec<(Vec<String>, [f64; 4])> {
    type Side = (Option<String>, f64, f64, f64);
    let mut hs: Vec<Side> = Vec::new();
    let mut vs: Vec<Side> = Vec::new();
    for (id, (a, c)) in &cx.lines {
        let l = dist(*a, *c);
        if b.used(id) || l < 1e-6 || l > 12. * cx.scale || !cx.annotation_style(id) {
            continue;
        }
        if (a[1] - c[1]).abs() < 1e-6 * l.max(1.) {
            hs.push((Some(id.clone()), a[0].min(c[0]), a[0].max(c[0]), a[1]));
        } else if (a[0] - c[0]).abs() < 1e-6 * l.max(1.) {
            vs.push((Some(id.clone()), a[1].min(c[1]), a[1].max(c[1]), a[0]));
        }
    }
    for (id, (pts, _, bulged)) in &cx.polys {
        if *bulged || b.used(id) {
            continue;
        }
        if let Some(r) = cell_rect(pts) {
            hs.push((None, r[0], r[2], r[1]));
            hs.push((None, r[0], r[2], r[3]));
            vs.push((None, r[1], r[3], r[0]));
            vs.push((None, r[1], r[3], r[2]));
        }
    }
    let mut out: Vec<(Vec<String>, [f64; 4])> = Vec::new();
    for (i, h1) in hs.iter().enumerate() {
        for h2 in hs.iter().skip(i + 1) {
            let tol = 1e-4 * (h1.2 - h1.1).max(1e-9);
            if (h1.1 - h2.1).abs() > tol || (h1.2 - h2.2).abs() > tol || (h1.3 - h2.3).abs() < 1e-6 {
                continue;
            }
            let (ylo, yhi) = (h1.3.min(h2.3), h1.3.max(h2.3));
            let side = |x: f64| {
                vs.iter().find(|v| (v.3 - x).abs() < tol.max(1e-6) && (v.1 - ylo).abs() < tol.max(1e-6) && (v.2 - yhi).abs() < tol.max(1e-6))
            };
            if let (Some(v1), Some(v2)) = (side(h1.1), side(h1.2)) {
                let ids: Vec<String> = [&h1.0, &h2.0, &v1.0, &v2.0].iter().filter_map(|x| (*x).clone()).collect();
                let r = [h1.1, ylo, h1.2, yhi];
                if !ids.is_empty() && !out.iter().any(|o| o.1 == r) {
                    out.push((ids, r));
                }
            }
        }
    }
    out
}

/// Characteristic symbols drawn with loose lines and circles inside a frame cell.
pub(super) fn loose_glyphs(cx: &Ctx, b: &Builder) -> Result<Vec<(Vec<String>, &'static str, [f64; 4])>> {
    let c = card("gdt_frame_symbol")?;
    let inset = param(c, "cell_inset")?;
    let mut cells: Vec<[f64; 4]> = cx
        .polys
        .iter()
        .filter(|(h, (_, _, bulged))| !bulged && !b.used(h))
        .filter_map(|(_, (pts, _, _))| cell_rect(pts))
        .collect();
    cells.extend(frame_rectangles(cx, b).into_iter().map(|x| x.1));
    let mut out: Vec<(Vec<String>, &'static str, [f64; 4])> = Vec::new();
    for cell in cells {
        let h = cell[3] - cell[1];
        let shrink = [cell[0] + inset * h, cell[1] + inset * h, cell[2] - inset * h, cell[3] - inset * h];
        if cx.texts.values().any(|t| inside(t.center, shrink, 0.)) {
            continue;
        }
        let mut ids = Vec::new();
        let mut shape = dd::Shape { lines: vec![], circles: vec![], texts: vec![], paths: vec![], curved: false };
        for (id, (p, q)) in &cx.lines {
            if !b.used(id) && inside(*p, shrink, 0.) && inside(*q, shrink, 0.) {
                ids.push(id.clone());
                shape.lines.push((*p, *q));
            }
        }
        for (id, r) in &cx.rec {
            if r.kind == "CIRCLE"
                && !b.used(id)
                && let Some((ctr, rad, ..)) = cx.arc(id)
                && inside(sub(ctr, [rad, rad]), shrink, 0.)
                && inside(add(ctr, [rad, rad]), shrink, 0.)
            {
                ids.push(id.clone());
                shape.circles.push((ctr, rad));
            }
        }
        if ids.is_empty() || out.iter().any(|o| o.0.iter().any(|x| ids.contains(x))) {
            continue;
        }
        if let Some(kind) = glyph_kind(&shape) {
            let pts: Vec<P> = ids.iter().flat_map(|id| cx.points(id)).collect();
            if let Some(gb) = bbox_of(&pts) {
                out.push((ids, kind, gb));
            }
        }
    }
    Ok(out)
}

/// Tolerance-zone qualifier of a frame value: φ (cylindrical or circular), Sφ (spherical).
pub(super) fn zone_qualifier(value: &str) -> Result<Option<(&'static str, &'static str)>> {
    let c = card("tolerance_zone_modifier")?;
    let v = value.trim();
    let starts = |s: &str| c.markers.iter().any(|m| s.starts_with(m.as_str()));
    Ok(if let Some(rest) = v.strip_prefix('S').filter(|r| starts(r)) {
        let _ = rest;
        Some(("tolerance_zone:spherical", "spherical"))
    } else if starts(v) {
        Some(("tolerance_zone:cylindrical_or_circular", "cylindrical"))
    } else {
        None
    })
}

/// Name the frame card on every tolerance frame and add the zone qualifier.
pub(super) fn frame_cards(b: &mut Builder) -> Result<()> {
    let fc = card("gdt_frame_symbol")?;
    let zc = card("tolerance_zone_modifier")?;
    let values: BTreeMap<String, Vec<String>> = {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for x in b.members.iter().filter(|x| x.member_role == "tolerance_value") {
            m.entry(x.expression_id.clone()).or_default().push(x.source_handle.clone());
        }
        m
    };
    let _ = values;
    for e in b.exprs.iter_mut() {
        if !(e.kind.starts_with("gps_") || e.kind == "geometric_tolerance") {
            continue;
        }
        if !e.evidence_ids.contains(&tag(fc)) {
            e.evidence_ids.push(tag(fc));
        }
        // The label carries the value field after the symbol: "<glyph> <value> | <datum>…".
        let value = e
            .label
            .split_once(' ')
            .map(|x| x.1)
            .unwrap_or("")
            .split('|')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if let Some((q, v)) = zone_qualifier(&value)?
            && !e.qualifiers.iter().any(|x| x == q)
        {
            e.qualifiers.push(q.into());
            e.evidence_ids.push(tag(zc));
            e.evidence_ids.push(variant_tag(v));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- special marks

/// Standalone special marks (☆ …) written before a dimension value join that dimension.
pub(super) fn special_marks(
    cx: &Ctx,
    b: &mut Builder,
    native_texts: &BTreeMap<usize, TextFact>,
) -> Result<()> {
    let c = card("special_characteristic_mark")?;
    let before = param(c, "before_max")?;
    let after = param(c, "after_max")?;
    let line_off = param(c, "line_offset_max")?;
    let cosmin = param(c, "direction_cos_min")?;
    let is_mark = |s: &str| {
        let t = s.trim();
        !t.is_empty() && t.chars().all(|ch| c.markers.iter().any(|m| m.chars().next() == Some(ch)))
    };
    // Value texts of dimensions: native text geometry or text members.
    let mut values: Vec<(usize, TextFact)> = Vec::new();
    for (i, e) in b.exprs.iter().enumerate() {
        if !(e.kind.starts_with("dimension_") || e.kind == "thread_note") {
            continue;
        }
        if let Some(t) = native_texts.get(&i) {
            values.push((i, t.clone()));
        }
        for m in b.members.iter().filter(|m| m.expression_id == e.id && m.member_role == "dimension_text") {
            if let Some(t) = cx.texts.get(&m.source_handle) {
                values.push((i, t.clone()));
            }
        }
    }
    let marks: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id) && is_mark(&t.dec.text))
        .cloned()
        .collect();
    for m in marks {
        let hm = m.height.max(1e-6);
        let best = values
            .iter()
            .filter(|(_, t)| dot(t.dir, m.dir) >= cosmin)
            .filter_map(|(i, t)| {
                let start = sub(t.center, mul(t.dir, t.width / 2.));
                let along = dot(sub(m.center, start), t.dir);
                let off = cross(t.dir, sub(m.center, t.center)).abs();
                (along >= -before * hm && along <= after * hm && off <= line_off * hm).then_some((along.abs(), *i))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0));
        let Some((_, i)) = best else { continue };
        let mark = m.dec.text.trim().to_string();
        b.member(i, &m.id, None, [0., 1.], "special_mark", "text", "candidate", vec![], vec![tag(c)]);
        let e = &mut b.exprs[i];
        if !e.label.starts_with(mark.as_str()) {
            e.label = format!("{mark} {}", e.label);
        }
        if !e.qualifiers.iter().any(|q| q == "key_characteristic") {
            e.qualifiers.push("key_characteristic".into());
        }
        e.evidence_ids.push(tag(c));
        e.evidence_ids.push(variant_tag("standalone"));
    }
    // Marks written inside the dimension text.
    for e in b.exprs.iter_mut() {
        if (e.kind.starts_with("dimension_") || e.kind == "thread_note")
            && c.markers.iter().any(|mk| e.label.trim_start().starts_with(mk.as_str()))
            && !e.evidence_ids.contains(&tag(c))
        {
            e.evidence_ids.push(tag(c));
            e.evidence_ids.push(variant_tag("inline"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- angle unit

/// Angular dimension values written without a unit get "°"; the source text is kept.
pub(super) fn angle_units(b: &mut Builder) -> Result<()> {
    let c = card("angle_unit")?;
    for e in b.exprs.iter_mut().filter(|e| e.kind == "dimension_angle") {
        let l = e.label.trim().to_string();
        if c.markers.iter().any(|m| l.contains(m.as_str())) {
            e.evidence_ids.push(tag(c));
            e.evidence_ids.push(variant_tag("unit_present"));
            continue;
        }
        let numeric = !l.is_empty()
            && l.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
            && l.chars().next().is_some_and(|ch| ch.is_ascii_digit())
            && l.matches('.').count() <= 1;
        if numeric {
            e.label = format!("{l}°");
            e.qualifiers.push("unit_supplied:degree".into());
            e.evidence_ids.push(tag(c));
            e.evidence_ids.push(variant_tag("unit_missing"));
            e.evidence_ids.push(format!("source_text:{l}"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- section lines

/// Boundary segments of a HATCH record (line edges, polyline paths, arcs as chords).
fn hatch_boundary(r: &Record) -> Vec<(P, P)> {
    let t = &r.tags;
    let num = |i: usize| t.get(i).and_then(|x| x.1.trim().parse::<f64>().ok());
    let int_at = |i: usize| t.get(i).and_then(|x| x.1.trim().parse::<i64>().ok());
    let mut segs = Vec::new();
    let mut i = 0;
    while i < t.len() && t[i].0 != 91 {
        i += 1;
    }
    let paths = int_at(i).unwrap_or(0);
    i += 1;
    for _ in 0..paths {
        while i < t.len() && t[i].0 != 92 {
            i += 1;
        }
        let flag = int_at(i).unwrap_or(0);
        i += 1;
        if flag & 2 != 0 {
            // Polyline path: 72 bulge flag, 73 closed, 93 vertex count, then 10/20 [42].
            let mut pts: Vec<P> = Vec::new();
            let mut n = 0;
            while i < t.len() && t[i].0 != 93 {
                i += 1;
            }
            if let Some(k) = int_at(i) {
                n = k;
            }
            i += 1;
            while pts.len() < n as usize && i < t.len() {
                if t[i].0 == 10
                    && let (Some(x), Some(y)) = (num(i), num(i + 1))
                {
                    pts.push([x, y]);
                    i += 2;
                    continue;
                }
                i += 1;
            }
            for w in pts.windows(2) {
                segs.push((w[0], w[1]));
            }
            if pts.len() > 2 {
                segs.push((pts[pts.len() - 1], pts[0]));
            }
        } else {
            while i < t.len() && t[i].0 != 93 {
                i += 1;
            }
            let edges = int_at(i).unwrap_or(0);
            i += 1;
            for _ in 0..edges {
                while i < t.len() && t[i].0 != 72 {
                    i += 1;
                }
                let kind = int_at(i).unwrap_or(0);
                i += 1;
                let mut vals: BTreeMap<i32, f64> = BTreeMap::new();
                while i < t.len() && !matches!(t[i].0, 72 | 92 | 97 | 75) {
                    if let Some(v) = num(i) {
                        vals.entry(t[i].0).or_insert(v);
                    }
                    i += 1;
                }
                match kind {
                    1 => {
                        if let (Some(a), Some(bb), Some(c), Some(d)) =
                            (vals.get(&10), vals.get(&20), vals.get(&11), vals.get(&21))
                        {
                            segs.push(([*a, *bb], [*c, *d]));
                        }
                    }
                    2 => {
                        if let (Some(cxv), Some(cyv), Some(rad), Some(a0), Some(a1)) =
                            (vals.get(&10), vals.get(&20), vals.get(&40), vals.get(&50), vals.get(&51))
                        {
                            let ccw = vals.get(&73).is_none_or(|v| *v != 0.);
                            let (s0, mut s1) = (a0.to_radians(), a1.to_radians());
                            if s1 <= s0 {
                                s1 += std::f64::consts::TAU;
                            }
                            let pt = |a: f64| {
                                let a = if ccw { a } else { -a };
                                [cxv + rad * a.cos(), cyv + rad * a.sin()]
                            };
                            for k in 0..8 {
                                let (x0, x1) = (s0 + (s1 - s0) * k as f64 / 8., s0 + (s1 - s0) * (k + 1) as f64 / 8.);
                                segs.push((pt(x0), pt(x1)));
                            }
                        }
                    }
                    _ => {}
                }
                if i < t.len() && matches!(t[i].0, 92 | 97 | 75) {
                    break;
                }
            }
        }
    }
    segs
}

fn inside_segments(p: P, segs: &[(P, P)]) -> bool {
    let mut c = false;
    for (a, z) in segs {
        if (a[1] > p[1]) != (z[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (z[0] - a[0]) / (z[1] - a[1]);
            if x > p[0] {
                c = !c;
            }
        }
    }
    c
}

/// Lines inside a hatched region, parallel to its pattern and ending on its boundary, are
/// section lines whatever their weight or layer.
pub(super) fn section_lines(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = card("section_line_region")?;
    let atol = param(c, "angle_tolerance_deg")?;
    let btol = param(c, "boundary_tolerance")? * cx.scale.max(1e-6);
    let hatches: Vec<(String, f64, Vec<(P, P)>)> = cx
        .rec
        .iter()
        .filter(|(_, r)| r.kind == "HATCH" && int(r, 70) == 0)
        .filter_map(|(h, r)| {
            let ang = r.number(52).ok()?;
            let segs = hatch_boundary(r);
            (segs.len() >= 3).then(|| (h.clone(), ang, segs))
        })
        .collect();
    for (hh, ang, segs) in hatches {
        let Some(e) = b
            .by_source
            .get(&hh)
            .and_then(|ms| ms.iter().map(|&m| &b.members[m]).find(|m| m.member_role == "hatch_fill"))
            .and_then(|m| b.exprs.iter().position(|x| x.id == m.expression_id))
        else {
            continue;
        };
        let u = [ang.to_radians().cos(), ang.to_radians().sin()];
        let on_boundary = |p: P| segs.iter().any(|(a, z)| seg_d(p, *a, *z) <= btol);
        let lines: Vec<String> = cx
            .lines
            .iter()
            .filter(|(h, (p, q))| {
                !b.used(h)
                    && dist(*p, *q) > 4. * btol
                    && cross(unit(sub(*q, *p)), u).abs() <= atol.to_radians().sin()
                    && on_boundary(*p)
                    && on_boundary(*q)
                    && !on_boundary(mid(*p, *q))
                    && inside_segments(mid(*p, *q), &segs)
            })
            .map(|(h, _)| h.clone())
            .collect();
        for h in lines {
            b.member(e, &h, None, [0., 1.], "hatch_fill", member_line_type(cx.lt(&h), "hatch_fill"), "candidate", vec![], vec![tag(c), format!("pattern_angle:{ang:.1}")]);
            if !b.exprs[e].evidence_ids.contains(&tag(c)) {
                b.exprs[e].evidence_ids.push(tag(c));
                b.exprs[e].evidence_ids.push(variant_tag("drawn_as_line"));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- visible edges and detail circles

/// Thin circles labelled with a roman numeral, written beside the circle or at the end of
/// a thin leader (one or two segments) drawn from it.
fn labelled_circles(cx: &Ctx, b: &Builder) -> Vec<(String, P, f64)> {
    let romans: Vec<&TextFact> = cx.texts.values().filter(|t| t.lex == Lex::Roman).collect();
    cx.rec
        .keys()
        .filter(|h| cx.kind(h) == "CIRCLE" && cx.lt(h) == "thin_continuous")
        .filter_map(|h| cx.arc(h).map(|(c, r, ..)| (h.clone(), c, r)))
        .filter(|(h, c, r)| {
            if b.used(h) && !b.kinds_of(h).contains("view_callout") {
                return false;
            }
            if romans.iter().any(|t| dist(t.center, *c) < 3. * r + t.height) {
                return true;
            }
            let tol = 0.02 * cx.scale.max(1e-6);
            let mut segs: Vec<(P, P)> = Vec::new();
            for (id, (p, q)) in &cx.lines {
                if !cx.annotation_style(id) {
                    continue;
                }
                for (on, far) in [(*p, *q), (*q, *p)] {
                    // A leader drawn from the circle may start a little off it.
                    if (dist(on, *c) - r).abs() <= (0.1 * r).max(tol) && dist(far, *c) > *r {
                        segs.push((on, far));
                        for (id2, (p2, q2)) in &cx.lines {
                            if id2 != id && cx.annotation_style(id2) && (dist(*p2, far) <= tol || dist(*q2, far) <= tol) {
                                segs.push((*p2, *q2));
                            }
                        }
                    }
                }
            }
            romans.iter().any(|t| segs.iter().any(|(a, z)| seg_d(t.center, *a, *z) <= 2. * t.height))
        })
        .collect()
}

/// A straight line (or its pieces cut by a detail circle) whose two ends are corners of the
/// outline is a visible edge, whatever colour it was drawn in.
pub(super) fn visible_edges(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = card("visible_edge_line")?;
    let dc = card("detail_circle_interruption")?;
    let s = cx.scale.max(1e-6);
    let vtol = param(c, "vertex_tolerance")? * s;
    let lmin = param(c, "min_length")? * s;
    let ctol = param(dc, "on_circle_tolerance")? * s;
    let circles = labelled_circles(cx, b);
    // Outline corners: points where two or more structural roots end.
    let mut ends: Vec<(P, String)> = Vec::new();
    for (h, (p, q)) in &cx.lines {
        if cx.structural_style(h) && !cx.arrows.contains_key(h) {
            ends.push((*p, h.clone()));
            ends.push((*q, h.clone()));
        }
    }
    for (h, (pts, closed, _)) in &cx.polys {
        if cx.structural_style(h) && !closed && pts.len() >= 2 && !cx.arrows.contains_key(h) && !cx.inline_arrows.contains_key(h) {
            ends.push((pts[0], h.clone()));
            ends.push((pts[pts.len() - 1], h.clone()));
        }
    }
    let corner = |p: P| {
        let owners: BTreeSet<&str> = ends.iter().filter(|(q, _)| dist(*q, p) <= vtol).map(|(_, h)| h.as_str()).collect();
        owners.len() >= 2
    };
    let tipped = |p: P| cx.arrows.values().chain(cx.inline_arrows.values()).any(|a| dist(a.tip, p) <= vtol.max(0.05 * a.len));
    let cand: Vec<(String, P, P)> = cx
        .lines
        .iter()
        .filter(|(h, (p, q))| {
            !b.used(h)
                && !cx.structural_style(h)
                && cx.style.get(*h).is_some_and(|st| matches!(st.pattern, Pattern::Continuous | Pattern::ByBlock))
                && !(cx.conv.qualified && cx.style.get(*h).is_some_and(|st| matches!(st.layer.as_str(), "08" | "09" | "10" | "11" | "12")))
                && !tipped(*p)
                && !tipped(*q)
                && dist(*p, *q) > 1e-9
        })
        .map(|(h, (p, q))| (h.clone(), *p, *q))
        .collect();
    // Chains: one piece, or two collinear pieces whose facing ends lie on one detail circle.
    let mut chains: Vec<(Vec<String>, P, P, Option<String>)> = Vec::new();
    for (h, p, q) in &cand {
        chains.push((vec![h.clone()], *p, *q, None));
    }
    for i in 0..cand.len() {
        for j in 0..cand.len() {
            if i == j {
                continue;
            }
            let (hi, pi, qi) = &cand[i];
            let (hj, pj, qj) = &cand[j];
            for (far_i, near_i) in [(*pi, *qi), (*qi, *pi)] {
                for (near_j, far_j) in [(*pj, *qj), (*qj, *pj)] {
                    let u = unit(sub(near_i, far_i));
                    if cross(u, unit(sub(far_j, near_j))).abs() > 1e-3
                        || project(near_j, far_i, near_i).1 > vtol
                        || dot(sub(near_j, near_i), u) <= 0.
                    {
                        continue;
                    }
                    if let Some((ch, ..)) = circles.iter().find(|(_, cc, r)| {
                        (dist(near_i, *cc) - r).abs() <= ctol && (dist(near_j, *cc) - r).abs() <= ctol && dist(mid(near_i, near_j), *cc) < *r
                    }) && hi < hj
                    {
                        chains.push((vec![hi.clone(), hj.clone()], far_i, far_j, Some(ch.clone())));
                    }
                }
            }
        }
    }
    // Longest chains first; a piece joins one chain.
    chains.sort_by(|x, y| dist(y.1, y.2).total_cmp(&dist(x.1, x.2)));
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (pieces, a, z, circle) in chains {
        if pieces.iter().any(|p| taken.contains(p)) || dist(a, z) < lmin || !corner(a) || !corner(z) {
            continue;
        }
        // Not a duplicate of an outline already drawn on the same carrier.
        let dup = cx.lines.iter().any(|(h, (p, q))| {
            cx.structural_style(h) && project(*p, a, z).1 <= vtol && project(*q, a, z).1 <= vtol && {
                let (tp, tq) = (project(*p, a, z).0, project(*q, a, z).0);
                tp.max(tq) > 0.05 && tp.min(tq) < 0.95
            }
        });
        if dup {
            continue;
        }
        let mut ev = vec![tag(c), variant_tag("edge_in_annotation_colour")];
        if let Some(ch) = &circle {
            ev.push(tag(dc));
            ev.push(format!("detail_circle:{ch}"));
        }
        let e = b.expr("visible_contour", &pieces.join("+"), &tag(c), "candidate", vec![], ev, vec![]);
        for p in &pieces {
            b.member(e, p, None, [0., 1.], "contour_segment", "thick_continuous", "candidate", vec![], vec![tag(c)]);
            taken.insert(p.clone());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- leaders to outline extensions

/// Tolerance-frame leaders whose arrow does not end on the outline end on its extension:
/// a drawn thin line through the tip, collinear with an outline piece, joins the frame;
/// otherwise the extension is constructed and recorded as an inference.
pub(super) fn leader_extensions(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = card("gdt_leader_extension")?;
    let s = cx.scale.max(1e-6);
    let ttol = param(c, "tip_tolerance")? * s;
    let pcos = param(c, "perpendicular_cos_max")?;
    let gmax = param(c, "outline_gap_max")? * s;
    let reach = param(c, "constructed_reach_max")? * s;
    let kind_of: BTreeMap<String, String> = b.exprs.iter().map(|e| (e.id.clone(), e.kind.clone())).collect();
    let outline: Vec<(String, P, P)> = b
        .members
        .iter()
        .filter(|m| m.member_role == "contour_segment" && kind_of.get(&m.expression_id).is_some_and(|k| k == "visible_contour"))
        .filter_map(|m| cx.lines.get(&m.source_handle).map(|(p, q)| (m.source_handle.clone(), lerp(*p, *q, m.interval[0]), lerp(*p, *q, m.interval[1]))))
        .collect();
    let frames: Vec<usize> = b
        .exprs
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind.starts_with("gps_") || e.kind == "geometric_tolerance")
        .map(|(i, _)| i)
        .collect();
    for i in frames {
        let id = b.exprs[i].id.clone();
        let mut tips: Vec<(P, P)> = Vec::new();
        for (tip, dir) in b
            .members
            .iter()
            .filter(|m| m.expression_id == id && matches!(m.member_role.as_str(), "arrowhead" | "leader_line"))
            .filter_map(|m| cx.arrow(&m.source_handle).map(|a| (a.tip, a.dir)))
        {
            if !tips.iter().any(|(t, _)| dist(*t, tip) <= ttol) {
                tips.push((tip, dir));
            }
        }
        for (tip, dir) in tips {
            if outline.iter().any(|(_, p, q)| seg_d(tip, *p, *q) <= ttol) {
                continue;
            }
            // Drawn extension line.
            let drawn = cx.lines.iter().find_map(|(h, (p, q))| {
                if b.used(h) || dot(unit(sub(*q, *p)), dir).abs() > pcos {
                    return None;
                }
                let (near, far) = if dist(*p, tip) <= dist(*q, tip) { (*p, *q) } else { (*q, *p) };
                if seg_d(tip, *p, *q) > ttol || dist(near, tip) > ttol.max(0.02 * dist(*p, *q)) {
                    return None;
                }
                let u = unit(sub(far, near));
                outline
                    .iter()
                    .filter(|(_, a, z)| project(*a, near, far).1 <= ttol && project(*z, near, far).1 <= ttol)
                    .find(|(_, a, z)| {
                        // The outline starts within the gap beyond the far end, or the
                        // extension is drawn a little into it.
                        let (ta, tz) = (dot(sub(*a, far), u), dot(sub(*z, far), u));
                        ta.max(tz) > ttol && ta.min(tz) <= gmax && ta.min(tz) >= -dist(near, far)
                    })
                    .map(|(oh, ..)| (h.clone(), oh.clone()))
            });
            if let Some((h, oh)) = drawn {
                b.member(i, &h, None, [0., 1.], "extension_line", member_line_type(cx.lt(&h), "extension_line"), "candidate", vec![], vec![tag(c), format!("extends_outline:{oh}")]);
                let e = &mut b.exprs[i];
                e.evidence_ids.push(tag(c));
                e.evidence_ids.push(variant_tag("drawn"));
                continue;
            }
            // Constructed extension to the nearest outline piece whose carrier passes the tip.
            let built = outline
                .iter()
                .filter(|(_, p, q)| dot(unit(sub(*q, *p)), dir).abs() <= pcos && project(tip, *p, *q).1 <= ttol)
                .filter_map(|(oh, p, q)| {
                    let near = if dist(*p, tip) <= dist(*q, tip) { *p } else { *q };
                    let t = project(tip, *p, *q).0;
                    (!(0. ..=1.).contains(&t) && dist(near, tip) <= reach).then(|| (dist(near, tip), oh.clone(), near))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0));
            if let Some((_, oh, near)) = built {
                let e = &mut b.exprs[i];
                e.evidence_ids.push(tag(c));
                e.evidence_ids.push(variant_tag("constructed"));
                e.evidence_ids.push(format!(
                    "constructed_extension:{oh}:{:.6},{:.6}:{:.6},{:.6}",
                    tip[0], tip[1], near[0], near[1]
                ));
                if !e.qualifiers.iter().any(|q| q == "leader_to_outline_extension") {
                    e.qualifiers.push("leader_to_outline_extension".into());
                }
            }
        }
    }
    Ok(())
}
