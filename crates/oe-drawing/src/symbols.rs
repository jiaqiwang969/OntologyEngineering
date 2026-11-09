//! Symbol notations read from recognition cards (wave 1).
//!
//! * `gdt_characteristic` — the fourteen geometric characteristic symbols of GB/T 1182-2018
//!   table 1: the expression kind of each, its Unicode glyph, the character of the common CAD
//!   GD&T font, and the stroke signature it is drawn with. The code implements the signature
//!   tests (one circle, two concentric circles, a parallelogram …); which signature is which
//!   characteristic is read from the card.
//! * `gdt_frame_content` — what the cells of a tolerance frame say (tolerance value, zone shape,
//!   single, common and multiple datums), how its leader meets the toleranced feature, and frames
//!   drawn as one outline divided by lines.
//! * `surface_texture_symbol` — the graphical symbols of GB/T 131 (basic, material removal
//!   required or not allowed, complete), the value written in them or beside them, their
//!   reading direction and what their tip stands on.
//! * `view_callout` — viewing-direction and cutting-plane indications and view names
//!   (GB/T 4458.1, GB/T 4458.6, GB/T 17452).
use super::*;
use oe_contracts::MdRepresentationCard;

fn values<'a>(c: &'a MdRepresentationCard, concept: &str, entry: &str) -> Vec<&'a str> {
    c.lexicon
        .iter()
        .flatten()
        .filter(|l| (concept.is_empty() || l.concept == concept) && l.entry == entry)
        .flat_map(|l| l.values.iter().map(String::as_str))
        .collect()
}
fn push_ev(e: &mut MdEngineeringExpression, x: String) {
    if !e.evidence_ids.contains(&x) {
        e.evidence_ids.push(x);
    }
}
fn name(e: &mut MdEngineeringExpression, card: &str, variant: &str, concepts: &[&str]) {
    push_ev(e, format!("card:{card}"));
    push_ev(e, format!("variant:{variant}"));
    for c in concepts {
        push_ev(e, format!("concept:{c}"));
    }
}

// ---------------------------------------------------------------- characteristic signatures

fn closed_triangles(s: &dd::Shape) -> Vec<Vec<P>> {
    s.paths
        .iter()
        .filter_map(|p| {
            let mut q: Vec<P> = Vec::new();
            for v in p {
                if q.last().is_none_or(|w| dist(*w, *v) > 1e-9) {
                    q.push(*v);
                }
            }
            if q.len() == 4 && dist(q[0], q[3]) < 1e-9 {
                q.pop();
            }
            (q.len() == 3).then_some(q)
        })
        .collect()
}

/// Closed triangles drawn as three separate lines joined end to end (an arrowhead drawn with
/// loose lines, card gdt_characteristic variant drawn_arrow_head), with the indices of their sides.
pub(super) fn line_triangles(lines: &[(P, P)], join: f64) -> Vec<(Vec<P>, [usize; 3])> {
    let mut out = Vec::new();
    let n = lines.len();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let sides = [lines[i], lines[j], lines[k]];
                let longest = sides.iter().map(|(a, b)| dist(*a, *b)).fold(0., f64::max);
                if longest < 1e-9 {
                    continue;
                }
                let tol = join * longest;
                let ends: Vec<P> = sides.iter().flat_map(|(a, b)| [*a, *b]).collect();
                // every end meets exactly one end of another side
                let closed = (0..6).all(|x| {
                    (0..6)
                        .filter(|y| *y / 2 != x / 2 && dist(ends[x], ends[*y]) <= tol)
                        .count()
                        == 1
                });
                if !closed {
                    continue;
                }
                let mut verts: Vec<P> = Vec::new();
                for p in &ends {
                    if !verts.iter().any(|v| dist(*v, *p) <= tol) {
                        verts.push(*p);
                    }
                }
                if verts.len() == 3 && cross(sub(verts[1], verts[0]), sub(verts[2], verts[0])).abs() > 1e-12 {
                    out.push((verts, [i, j, k]));
                }
            }
        }
    }
    out
}

/// Arrow signatures of strokes with closed heads (the heads' own sides excluded): one oblique
/// arrow is circular run-out, two parallel oblique arrows total run-out.
fn arrow_signature(strokes: &[(P, P)], tris: &[Vec<P>]) -> Option<&'static str> {
    let lens: Vec<f64> = strokes.iter().map(|(a, b)| dist(*a, *b)).collect();
    let maxl = lens.iter().copied().fold(0., f64::max);
    let dir = |l: &(P, P)| unit(sub(l.1, l.0));
    let heads = |l: &(P, P)| -> bool {
        let len = dist(l.0, l.1);
        tris.iter().any(|t| {
            let centroid = [(t[0][0] + t[1][0] + t[2][0]) / 3., (t[0][1] + t[1][1] + t[2][1]) / 3.];
            [(l.0, l.1), (l.1, l.0)].iter().any(|(tip, other)| {
                t.iter().any(|v| dist(*v, *tip) < 0.05 * len)
                    && dist(centroid, *tip) < 0.8 * len
                    && dot(unit(sub(centroid, *tip)), unit(sub(*other, *tip))) > 0.7
            })
        })
    };
    let oblique = |l: &(P, P)| {
        let u = dir(l);
        u[0].abs() > 0.2 && u[1].abs() > 0.2
    };
    let arrows: Vec<&(P, P)> = strokes
        .iter()
        .filter(|l| oblique(l) && dist(l.0, l.1) > 0.5 * maxl && heads(l))
        .collect();
    if arrows.len() >= 2 && cross(dir(arrows[0]), dir(arrows[1])).abs() < 0.03 {
        return Some("two_oblique_arrows_on_base");
    }
    let (i, _) = lens.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    (oblique(&strokes[i]) && heads(&strokes[i])).then_some("single_oblique_arrow")
}

/// Stroke signature of a characteristic symbol drawn with lines, circles and arcs (`arcs`
/// counts arcs, which block shapes only flag).
pub(super) fn signature(s: &dd::Shape, arcs: usize) -> Option<&'static str> {
    if !s.texts.is_empty() {
        return None;
    }
    // A stroke ending in a closed head is an arrow whatever the head is drawn with: a polyline or
    // filled triangle, or three lines joined end to end (card gdt_characteristic, variant
    // drawn_arrow_head); the head's sides are not strokes of their own.
    if s.circles.is_empty() && arcs == 0 {
        let join = representation::card_ref("gdt_characteristic")
            .and_then(|c| representation::param(c, "head_join"))
            .unwrap_or(0.04);
        let all: Vec<(P, P)> = s.lines.iter().copied().filter(|(a, b)| dist(*a, *b) > 1e-9).collect();
        let drawn = line_triangles(&all, join);
        let mut tris: Vec<Vec<P>> = closed_triangles(s);
        tris.extend(drawn.iter().map(|x| x.0.clone()));
        let sides: BTreeSet<usize> = drawn.iter().flat_map(|x| x.1).collect();
        let strokes: Vec<(P, P)> = all
            .iter()
            .enumerate()
            .filter(|(i, l)| {
                !sides.contains(i) && !tris.iter().any(|t| t.iter().any(|v| dist(*v, l.0) < 1e-9) && t.iter().any(|v| dist(*v, l.1) < 1e-9))
            })
            .map(|x| *x.1)
            .collect();
        if !tris.is_empty()
            && !strokes.is_empty()
            && let Some(sig) = arrow_signature(&strokes, &tris)
        {
            return Some(sig);
        }
    }
    let lines: Vec<(P, P)> = s
        .lines
        .iter()
        .copied()
        .filter(|(a, b)| dist(*a, *b) > 1e-9)
        .collect();
    let lens: Vec<f64> = lines.iter().map(|(a, b)| dist(*a, *b)).collect();
    let maxl = lens.iter().copied().fold(0., f64::max);
    let dir = |l: &(P, P)| unit(sub(l.1, l.0));
    if s.circles.len() == 2
        && lines.is_empty()
        && dist(s.circles[0].0, s.circles[1].0) < 0.05 * s.circles[0].1.max(s.circles[1].1)
        && (s.circles[0].1 - s.circles[1].1).abs() > 0.1 * s.circles[0].1.max(s.circles[1].1)
    {
        return Some("concentric_circles");
    }
    if s.circles.len() == 1 && lines.len() == 2 {
        let (c, r) = s.circles[0];
        let (u, v) = (dir(&lines[0]), dir(&lines[1]));
        if dot(u, v).abs() < 0.05
            && lines
                .iter()
                .all(|(a, b)| dist(mid(*a, *b), c) < 0.15 * r && dist(*a, *b) > r)
        {
            return Some("circle_with_cross");
        }
        if dot(u, v).abs() > 0.995
            && lines.iter().all(|(a, b)| {
                let (_, d) = project(c, *a, *b);
                (0.8 * r..1.25 * r).contains(&d.abs())
            })
        {
            return Some("circle_two_tangent_parallels");
        }
    }
    if s.circles.len() == 1 && lines.is_empty() && arcs == 0 {
        return Some("single_circle");
    }
    if !s.circles.is_empty() {
        return None;
    }
    if arcs == 1 && lines.is_empty() {
        return Some("open_arc");
    }
    if arcs == 1 && lines.len() == 1 {
        return Some("arc_with_chord");
    }
    if arcs > 0 {
        return None;
    }
    if lines.len() == 1 {
        return Some("single_line");
    }
    if lines.len() == 2 {
        let (a, b) = lines[0];
        let (c, d) = lines[1];
        let (u, v) = (dir(&lines[0]), dir(&lines[1]));
        if dot(u, v).abs() < 0.05 {
            let touches = |p: P, q: (P, P)| dist(p, mid(q.0, q.1)) < 0.08 * dist(q.0, q.1);
            if touches(c, (a, b)) || touches(d, (a, b)) || touches(a, (c, d)) || touches(b, (c, d))
            {
                return Some("perpendicular_tee");
            }
        }
        if dot(u, v).abs() > 0.995 && project(c, a, b).1.abs() > 0.1 * maxl {
            return Some("two_parallel_obliques");
        }
        // two strokes from one point at an acute angle, one of them horizontal (∠)
        let shared = [(a, b, c, d), (a, b, d, c), (b, a, c, d), (b, a, d, c)]
            .into_iter()
            .find(|(p, _, q, _)| dist(*p, *q) < 0.05 * maxl);
        if let Some((p, x, q, y)) = shared {
            let cos = dot(unit(sub(x, p)), unit(sub(y, q)));
            let level = |w: P| w[1].abs() < 0.02 * norm(w).max(1e-12);
            if (0.70..0.95).contains(&cos) && (level(sub(x, p)) || level(sub(y, q))) {
                return Some("acute_angle");
            }
        }
    }
    if lines.len() == 3 {
        let u = dir(&lines[0]);
        if lines.iter().all(|l| cross(u, dir(l)).abs() < 0.02)
            && lens.iter().copied().fold(f64::INFINITY, f64::min) < 0.85 * maxl
        {
            return Some("three_parallels_middle_longest");
        }
    }
    if lines.len() == 4 {
        // closed quadrilateral with two pairs of parallel sides that is not a rectangle (⏥)
        let ends: Vec<P> = lines.iter().flat_map(|(a, b)| [*a, *b]).collect();
        let closed = ends
            .iter()
            .all(|p| ends.iter().filter(|q| dist(*p, **q) < 0.03 * maxl).count() == 2);
        let par = |i: usize, j: usize| cross(dir(&lines[i]), dir(&lines[j])).abs() < 0.02;
        let pairs =
            (par(0, 1) && par(2, 3)) || (par(0, 2) && par(1, 3)) || (par(0, 3) && par(1, 2));
        let right = (0..4)
            .any(|i| (0..4).any(|j| i != j && dot(dir(&lines[i]), dir(&lines[j])).abs() < 0.05));
        if closed && pairs && !right {
            return Some("parallelogram");
        }
    }
    // Runout arrows: oblique strokes ending in triangular heads; two of them on a base line is total runout.
    let tris = closed_triangles(s);
    let heads = |l: &(P, P)| -> bool {
        let len = dist(l.0, l.1);
        tris.iter().any(|t| {
            let centroid = [
                (t[0][0] + t[1][0] + t[2][0]) / 3.,
                (t[0][1] + t[1][1] + t[2][1]) / 3.,
            ];
            [(l.0, l.1), (l.1, l.0)].iter().any(|(tip, other)| {
                t.iter().any(|v| dist(*v, *tip) < 0.05 * len)
                    && dist(centroid, *tip) < 0.8 * len
                    && dot(unit(sub(centroid, *tip)), unit(sub(*other, *tip))) > 0.7
            })
        })
    };
    let oblique = |l: &(P, P)| {
        let u = dir(l);
        u[0].abs() > 0.2 && u[1].abs() > 0.2
    };
    let arrows: Vec<&(P, P)> = lines
        .iter()
        .filter(|l| oblique(l) && dist(l.0, l.1) > 0.5 * maxl && heads(l))
        .collect();
    if arrows.len() >= 2 && cross(dir(arrows[0]), dir(arrows[1])).abs() < 0.03 {
        return Some("two_oblique_arrows_on_base");
    }
    if lines.len() >= 2
        && let Some((i, _)) = lens.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))
        && oblique(&lines[i])
        && heads(&lines[i])
    {
        return Some("single_oblique_arrow");
    }
    None
}

/// Expression kind of a characteristic signature, read from the `gdt_characteristic` card.
pub(super) fn characteristic_kind(sig: &str) -> Result<Option<&'static str>> {
    let c = representation::card_ref("gdt_characteristic")?;
    let concept = c
        .lexicon
        .iter()
        .flatten()
        .find(|l| l.entry == "stroke_signature" && l.values.iter().any(|v| v == sig))
        .map(|l| l.concept.as_str());
    Ok(concept.and_then(|id| values(c, id, "kind").first().copied()))
}

/// Characteristic concept of an expression kind.
fn characteristic_concept(kind: &str) -> Result<Option<&'static str>> {
    let c = representation::card_ref("gdt_characteristic")?;
    Ok(c.lexicon
        .iter()
        .flatten()
        .find(|l| l.entry == "kind" && l.values.iter().any(|v| v == kind))
        .map(|l| l.concept.as_str()))
}

/// The text style font of a TEXT or MTEXT record, through the STYLE table.
fn text_font(cx: &Ctx, id: &str) -> String {
    let Some(r) = cx.rec.get(id) else {
        return String::new();
    };
    let style = r.get(7).unwrap_or("STANDARD").to_string();
    cx.d.records
        .iter()
        .find(|s| s.kind == "STYLE" && s.get(2).is_some_and(|n| n.eq_ignore_ascii_case(&style)))
        .and_then(|s| s.get(3))
        .unwrap_or("")
        .to_lowercase()
}

/// Characteristic symbols written as text: a Unicode glyph, or a letter of a GD&T font.
pub(super) fn text_glyphs(
    cx: &Ctx,
    b: &Builder,
) -> Result<Vec<(Vec<String>, &'static str, [f64; 4])>> {
    let c = representation::card_ref("gdt_characteristic")?;
    let fonts = values(c, "", "font_file");
    let mut out = Vec::new();
    for t in cx.texts.values().filter(|t| !b.used(&t.id)) {
        let s = t.dec.text.trim();
        if s.chars().count() != 1 {
            continue;
        }
        let gdt_font = {
            let f = text_font(cx, &t.id);
            fonts.iter().any(|x| f.contains(x))
        };
        let hit = c.lexicon.iter().flatten().find(|l| {
            (l.entry == "glyph" && l.values.iter().any(|v| v == s))
                || (gdt_font && l.entry == "font_char" && l.values.iter().any(|v| v == s))
        });
        if let Some(l) = hit
            && let Some(kind) = values(c, &l.concept, "kind").first().copied()
            && let Some(bb) = cx.bbox(&t.id)
        {
            out.push((vec![t.id.clone()], kind, bb));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- frames drawn as one divided outline

/// Split a rectangle into cells at divider lines that span its full height.
pub(super) fn split_by_dividers(
    r: [f64; 4],
    dividers: &[(String, f64, f64, f64)],
    tol: f64,
) -> Vec<(Vec<String>, [f64; 4])> {
    let mut xs: Vec<(f64, String)> = dividers
        .iter()
        .filter(|d| {
            d.3 > r[0] + tol
                && d.3 < r[2] - tol
                && (d.1 - r[1]).abs() < tol
                && (d.2 - r[3]).abs() < tol
        })
        .map(|d| (d.3, d.0.clone()))
        .collect();
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    xs.dedup_by(|a, b| (a.0 - b.0).abs() < tol);
    if xs.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let mut left = (r[0], None::<String>);
    for (x, id) in xs
        .iter()
        .cloned()
        .chain(std::iter::once((r[2], String::new())))
    {
        let mut ids: Vec<String> = Vec::new();
        if let Some(l) = &left.1 {
            ids.push(l.clone());
        }
        if !id.is_empty() {
            ids.push(id.clone());
        }
        out.push((ids, [left.0, r[1], x, r[3]]));
        left = (x, (!id.is_empty()).then_some(id));
    }
    out
}

/// Frames drawn as one closed outline divided by full-height lines (card gdt_frame_content,
/// variant outline_dividers): each cell carries the outline and its dividers.
pub(super) fn divided_frames(cx: &Ctx, b: &Builder) -> Result<Vec<(Vec<String>, [f64; 4])>> {
    let c = representation::card_ref("gdt_frame_content")?;
    let aspect = representation::param(c, "outline_aspect_min")?;
    let join = representation::param(c, "divider_join")?;
    let mut verticals: Vec<(String, f64, f64, f64)> = Vec::new();
    for (id, (a, q)) in &cx.lines {
        let l = dist(*a, *q);
        if !b.used(id)
            && l > 1e-6
            && (a[0] - q[0]).abs() < 1e-6 * l.max(1.)
            && cx.annotation_style(id)
        {
            verticals.push((id.clone(), a[1].min(q[1]), a[1].max(q[1]), a[0]));
        }
    }
    let mut out = Vec::new();
    for (h, (pts, _, bulged)) in &cx.polys {
        if *bulged || b.used(h) {
            continue;
        }
        let Some(r) = cell_rect(pts) else { continue };
        let (w, ht) = (r[2] - r[0], r[3] - r[1]);
        if ht <= 1e-9 || w < aspect * ht || ht > 4. * cx.scale {
            continue;
        }
        for (mut ids, cell) in split_by_dividers(r, &verticals, join * ht) {
            ids.insert(0, h.clone());
            out.push((ids, cell));
        }
    }
    Ok(out)
}

/// Cells of a block frame drawn as one outline with divider lines.
pub(super) fn block_divided_cells(s: &dd::Shape) -> Vec<[f64; 4]> {
    let Ok(c) = representation::card_ref("gdt_frame_content") else {
        return vec![];
    };
    let Ok(join) = representation::param(c, "divider_join") else {
        return vec![];
    };
    let Ok(aspect) = representation::param(c, "outline_aspect_min") else {
        return vec![];
    };
    let rects: Vec<[f64; 4]> = s.paths.iter().filter_map(|p| cell_rect(p)).collect();
    let mut out = Vec::new();
    for r in rects {
        let ht = r[3] - r[1];
        if ht <= 1e-9 || r[2] - r[0] < aspect * ht {
            continue;
        }
        let dividers: Vec<(String, f64, f64, f64)> = s
            .lines
            .iter()
            .filter(|(a, q)| (a[0] - q[0]).abs() < 1e-6 * dist(*a, *q).max(1.))
            .map(|(a, q)| (String::new(), a[1].min(q[1]), a[1].max(q[1]), a[0]))
            .collect();
        out.extend(
            split_by_dividers(r, &dividers, join * ht)
                .into_iter()
                .map(|x| x.1),
        );
    }
    out
}

/// Frame content: characteristic, value cell, zone shape, datums, and how the leader meets the
/// feature (card gdt_frame_content); the characteristic concept from gdt_characteristic.
pub(super) fn frame_content(b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("gdt_frame_content")?;
    let zone = values(c, "DC-G5-089", "prefix");
    let tolerated: BTreeSet<usize> = b
        .links
        .iter()
        .filter(|l| l.1 == "toleranced_by_frame")
        .map(|l| l.2)
        .collect();
    for i in 0..b.exprs.len() {
        let kind = b.exprs[i].kind.clone();
        if !(kind.starts_with("gps_") || kind == "geometric_tolerance") {
            continue;
        }
        let mut concepts: Vec<&str> = vec!["DC-G5-090"];
        if let Some(k) = characteristic_concept(&kind)? {
            name(
                &mut b.exprs[i],
                "gdt_characteristic",
                "characteristic_symbol",
                &[k],
            );
        }
        let label = b.exprs[i].label.clone();
        let mut fields = label.split('|').map(str::trim);
        let first = fields.next().unwrap_or("");
        let value = first.split_once(' ').map(|x| x.1.trim()).unwrap_or("");
        if !value.is_empty() {
            concepts.push("DC-G5-091");
        }
        if zone.iter().any(|z| value.starts_with(z)) {
            concepts.push("DC-G5-089");
        }
        let datums: Vec<&str> = fields.filter(|f| !f.is_empty()).collect();
        // datum names in separate cells, or separated by a space in one cell, form a datum system
        let named: usize = datums
            .iter()
            .map(|d| {
                d.split_whitespace()
                    .filter(|w| w.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
                    .count()
                    .max(1)
            })
            .sum();
        if named >= 2 {
            concepts.push("DC-G5-004");
        }
        for d in datums.iter().flat_map(|d| d.split_whitespace()) {
            let letters: Vec<&str> = d.split(['-', '—']).map(str::trim).collect();
            if letters.len() >= 2
                && letters.iter().all(|l| {
                    !l.is_empty() && l.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                })
            {
                concepts.push("DC-G5-002");
            } else if d.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                concepts.push("DC-G5-003");
            }
        }
        let ev = b.exprs[i].evidence_ids.clone();
        let along = ev
            .iter()
            .any(|x| x.contains("along_dimension_line") || x.contains("attached_frame"))
            || tolerated.contains(&i);
        if along {
            concepts.push("DC-G5-107");
        } else if ev
            .iter()
            .any(|x| x.starts_with("leader_from_frame_border") || x == "card:gdt_leader_extension")
            || b.members
                .iter()
                .any(|m| m.expression_id == b.exprs[i].id && m.member_role == "leader_line")
        {
            concepts.push("DC-G5-108");
        }
        let variant = if ev.iter().any(|x| x == "variant:outline_dividers") {
            "outline_dividers"
        } else if ev.iter().any(|x| x == "variant:line_outline_dividers") {
            "line_outline_dividers"
        } else {
            "fields_and_datums"
        };
        name(&mut b.exprs[i], "gdt_frame_content", variant, &concepts);
    }
    Ok(())
}

// ---------------------------------------------------------------- surface texture symbols

/// The geometry of a surface texture symbol: the tip, the two arms (short and long), and the
/// optional bar (material removal), circle (removal not allowed) and extension (complete symbol).
#[derive(Clone, Debug)]
pub(super) struct Texture {
    pub tip: P,
    pub axis: P,
    pub size: f64,
    pub bar: bool,
    pub circle: bool,
    pub extension: bool,
}

/// Recognise the V of a surface texture symbol among lines: two strokes from one point at the
/// card's included angle, the longer one the card's arm ratio times the shorter.
pub(super) fn texture_of(lines: &[(P, P)], circles: &[(P, f64)]) -> Option<Texture> {
    let c = representation::card_ref("surface_texture_symbol").ok()?;
    let ang = representation::param(c, "included_angle_deg")
        .ok()?
        .to_radians()
        .cos();
    let ang_tol = representation::param(c, "angle_cos_tolerance").ok()?;
    let rmin = representation::param(c, "arm_ratio_min").ok()?;
    let rmax = representation::param(c, "arm_ratio_max").ok()?;
    for (i, (a, b)) in lines.iter().enumerate() {
        for (j, (p, q)) in lines.iter().enumerate().skip(i + 1) {
            for (t1, x, t2, y) in [
                (*a, *b, *p, *q),
                (*a, *b, *q, *p),
                (*b, *a, *p, *q),
                (*b, *a, *q, *p),
            ] {
                let (l1, l2) = (dist(t1, x), dist(t2, y));
                if l1 < 1e-9 || l2 < 1e-9 || dist(t1, t2) > 0.03 * l1.min(l2) {
                    continue;
                }
                let cos = dot(sub(x, t1), sub(y, t2)) / (l1 * l2);
                let ratio = l1.max(l2) / l1.min(l2);
                if (cos - ang).abs() > ang_tol || !(rmin..rmax).contains(&ratio) {
                    continue;
                }
                let tip = t1;
                let (short_end, long_end) = if l1 < l2 { (x, y) } else { (y, x) };
                let short = l1.min(l2);
                let axis = unit(add(unit(sub(x, tip)), unit(sub(y, tip))));
                let on = |pt: P, from: P, to: P| seg_dist(pt, from, to) < 0.06 * short;
                let others: Vec<&(P, P)> = lines
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k != i && *k != j)
                    .map(|x| x.1)
                    .collect();
                let bar = others.iter().any(|(m, n)| {
                    (on(*m, tip, short_end) && on(*n, tip, long_end))
                        || (on(*n, tip, short_end) && on(*m, tip, long_end))
                });
                let extension = others.iter().any(|(m, n)| {
                    (dist(*m, long_end) < 0.06 * short || dist(*n, long_end) < 0.06 * short)
                        && dot(unit(sub(*n, *m)), axis).abs() < 0.15
                });
                let circle = circles.iter().any(|(cc, r)| {
                    let d = dot(sub(*cc, tip), axis);
                    d > 0.
                        && d < short
                        && *r < 0.6 * short
                        && cross(axis, sub(*cc, tip)).abs() < 0.1 * short
                });
                return Some(Texture {
                    tip,
                    axis,
                    size: l1.max(l2),
                    bar,
                    circle,
                    extension,
                });
            }
        }
    }
    None
}
fn seg_dist(p: P, a: P, b: P) -> f64 {
    let v = sub(b, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), v) / n).clamp(0., 1.);
    dist(p, add(a, mul(v, t)))
}

/// Text of a surface texture symbol block whose texts carry a parameter code (`Ra 3.2`); blocks
/// with a bare number are read by the older rule.
pub(super) fn texture_block_text(s: &dd::Shape) -> Option<String> {
    let c = representation::card_ref("surface_texture_symbol").ok()?;
    if s.texts.is_empty() || s.lines.len() > 8 || texture_of(&s.lines, &s.circles).is_none() {
        return None;
    }
    let texts: Vec<&str> = s
        .texts
        .iter()
        .map(|t| t.0.as_str())
        .filter(|t| !t.trim().is_empty())
        .collect();
    texts
        .iter()
        .any(|t| parameter_code(c, t))
        .then(|| texts.join(" "))
}

/// Surface texture symbol block without a value inside: kept for `surface_textures`.
pub(super) fn texture_block(s: &dd::Shape) -> bool {
    s.texts.is_empty() && s.lines.len() <= 6 && texture_of(&s.lines, &s.circles).is_some()
}

/// Words of the simplified indication for most surfaces (card surface_texture_symbol).
fn most_surfaces_words() -> Vec<String> {
    representation::card_ref("surface_texture_symbol")
        .map(|c| values(c, "DC-G6-006", "most_surfaces_word").iter().map(|w| w.to_string()).collect())
        .unwrap_or_default()
}
fn is_most_surfaces_word(words: &[String], t: &str) -> bool {
    let s: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    words.contains(&s)
}

/// A block holding only the words of the indication for most surfaces and a texture symbol without
/// a value (card surface_texture_symbol, variant most_surfaces): the label of the requirement.
pub(super) fn most_surfaces_block(s: &dd::Shape) -> Option<String> {
    let words = most_surfaces_words();
    if s.texts.is_empty() || s.lines.len() > 8 || !s.texts.iter().all(|t| is_most_surfaces_word(&words, &t.0)) {
        return None;
    }
    texture_of(&s.lines, &s.circles)?;
    let word: String = s.texts[0].0.chars().filter(|c| !c.is_whitespace()).collect();
    Some(format!("{word} √"))
}

fn parameter_code(c: &MdRepresentationCard, t: &str) -> bool {
    let codes = values(c, "DC-G6-075", "parameter_code");
    let s: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    codes.iter().any(|k| {
        s.starts_with(k)
            && s[k.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == 'm')
    })
}
fn texture_value(c: &MdRepresentationCard, t: &TextFact) -> bool {
    matches!(t.lex, Lex::Number | Lex::ToleranceValue) || parameter_code(c, &t.dec.text)
}

/// Surface texture symbols whose value is written beside the symbol (a block without its own
/// text, often rotated) and symbols drawn with loose strokes: the value text is the nearest free
/// value within reach of the symbol's opening.
pub(super) fn surface_textures(cx: &Ctx, b: &mut Builder, blocks: &[String]) -> Result<()> {
    let c = representation::card_ref("surface_texture_symbol")?;
    let reach = representation::param(c, "value_reach")?;
    let arm_min = representation::param(c, "arm_min")?;
    let arm_max = representation::param(c, "arm_max")?;
    let most_reach = representation::param(c, "most_surfaces_reach")?;
    let place = |b: &mut Builder,
                 members: Vec<(String, &'static str)>,
                 tex: &Texture,
                 variant: &str| {
        let opening = add(tex.tip, mul(tex.axis, 0.7 * tex.size));
        let value = cx
            .texts
            .values()
            .filter(|t| {
                !b.used(&t.id) && texture_value(c, t) && dist(t.center, opening) < reach * tex.size
            })
            .min_by(|x, y| dist(x.center, opening).total_cmp(&dist(y.center, opening)))
            .cloned();
        // a symbol without a value is labelled with the symbol itself; the word of the indication for
        // most surfaces written just before it makes it the requirement for the other surfaces
        let general = if value.is_none() {
            let words = most_surfaces_words();
            let pts: Vec<P> = members.iter().flat_map(|(id, _)| cx.points(id)).collect();
            bbox_of(&pts).and_then(|bb| {
                cx.texts
                    .values()
                    .filter(|t| !b.used(&t.id) && is_most_surfaces_word(&words, &t.dec.text) && t.dir[0] > 0.99)
                    .find(|t| {
                        let right = t.center[0] + 0.5 * t.width;
                        right <= bb[0] + 0.5 * t.height
                            && right >= bb[0] - most_reach * t.height
                            && t.center[1] >= bb[1] - t.height
                            && t.center[1] <= bb[3] + t.height
                    })
                    .cloned()
            })
        } else {
            None
        };
        let label = match (&value, &general) {
            (Some(t), _) => t.dec.text.clone(),
            (None, Some(g)) => format!("{} √", g.dec.text.trim()),
            (None, None) => "√".to_string(),
        };
        let e = b.expr(
            "surface_roughness",
            &label,
            &format!("card:surface_texture_symbol/{variant}"),
            "candidate",
            if general.is_some() { vec!["general_requirement".into()] } else { vec![] },
            vec![
                "card:surface_texture_symbol".into(),
                format!("variant:{variant}"),
            ],
            vec!["Controlled surface requires the indicated feature".into()],
        );
        if let Some(g) = &general {
            b.member(e, &g.id, None, [0., 1.], "note_text", "text", "candidate", vec![], vec!["most_surfaces_word_before_symbol".into()]);
        }
        for (id, role) in members {
            let lt = if role == "roughness_symbol" && cx.shapes.contains_key(&id) {
                "block"
            } else {
                member_line_type(cx.lt(&id), "thin")
            };
            b.member(
                e,
                &id,
                None,
                [0., 1.],
                role,
                lt,
                "candidate",
                vec![],
                vec![],
            );
        }
        if let Some(t) = value {
            b.member(
                e,
                &t.id,
                None,
                [0., 1.],
                "roughness_value",
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
    };
    for h in blocks {
        if b.used(h) {
            continue;
        }
        let Some(s) = cx.shapes.get(h) else { continue };
        let Some(tex) = texture_of(&s.lines, &s.circles) else {
            continue;
        };
        place(
            b,
            vec![(h.clone(), "roughness_symbol")],
            &tex,
            "block_value_apart",
        );
    }
    // Loose strokes: thin free lines forming the V, with the value text near the opening.
    let free: Vec<(String, (P, P))> = cx
        .lines
        .iter()
        .filter(|(id, (a, q))| {
            !b.used(id)
                && cx.annotation_style(id)
                && (arm_min * cx.scale..arm_max * cx.scale).contains(&dist(*a, *q))
        })
        .map(|(id, l)| (id.clone(), *l))
        .collect();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (i, (ia, la)) in free.iter().enumerate() {
        for (ib, lb) in free.iter().skip(i + 1) {
            if taken.contains(ia) || taken.contains(ib) {
                continue;
            }
            let Some(tex) = texture_of(&[*la, *lb], &[]) else {
                continue;
            };
            let opening = add(tex.tip, mul(tex.axis, 0.7 * tex.size));
            let has_value = cx.texts.values().any(|t| {
                !b.used(&t.id) && texture_value(c, t) && dist(t.center, opening) < reach * tex.size
            });
            if !has_value {
                continue;
            }
            let mut members = vec![
                (ia.clone(), "roughness_symbol"),
                (ib.clone(), "roughness_symbol"),
            ];
            // bar or extension strokes touching the arms
            for (ic, lc) in &free {
                if ic == ia || ic == ib || taken.contains(ic) {
                    continue;
                }
                let lines = [*la, *lb, *lc];
                if let Some(t3) = texture_of(&lines, &[])
                    && (t3.bar || t3.extension)
                {
                    members.push((ic.clone(), "roughness_symbol"));
                }
            }
            for m in &members {
                taken.insert(m.0.clone());
            }
            place(b, members, &tex, "loose_strokes");
        }
    }
    Ok(())
}

/// Texture geometry of a recognised surface texture expression (block shape or loose strokes).
fn texture_geometry(cx: &Ctx, b: &Builder, e: usize) -> Option<(Texture, bool)> {
    let id = &b.exprs[e].id;
    let syms: Vec<&MdExpressionMember> = b
        .members
        .iter()
        .filter(|m| &m.expression_id == id && m.member_role == "roughness_symbol")
        .collect();
    let mut lines: Vec<(P, P)> = Vec::new();
    let mut circles: Vec<(P, f64)> = Vec::new();
    for m in syms {
        if let Some(s) = cx.shapes.get(&m.source_handle) {
            lines.extend(s.lines.iter().copied());
            circles.extend(s.circles.iter().copied());
        } else if let Some(l) = cx.lines.get(&m.source_handle) {
            lines.push(*l);
        }
    }
    let t = texture_of(&lines, &circles)?;
    // Upright symbols open upwards (the block's own rotation does not matter, only the drawn
    // orientation); any other opening is read from another side.
    let rotated = t.axis[1] < 0.95;
    Some((t, rotated))
}

/// A free thin line under the tip of a texture symbol that continues an outline or an extension
/// line (the symbol stands on an extension of the surface, GB/T 131 figure 4).
fn standing_line(cx: &Ctx, b: &Builder, tip: P, tol: f64) -> Option<String> {
    let c = representation::card_ref("surface_texture_symbol").ok()?;
    let ctol = representation::param(c, "collinear_tolerance").ok()? * cx.scale.max(1e-6);
    cx.lines
        .iter()
        .filter(|(id, (p, q))| {
            !b.used(id) && cx.annotation_style(id) && seg_dist(tip, *p, *q) < tol
        })
        .find(|(id, (p, q))| {
            let u = unit(sub(*q, *p));
            // collinear with a line that already plays an outline or extension-line role
            b.members.iter().any(|m| {
                (m.member_role == "contour_segment" || m.member_role == "extension_line")
                    && m.source_handle != **id
                    && cx.lines.get(&m.source_handle).is_some_and(|(a, z)| {
                        cross(unit(sub(*z, *a)), u).abs() < 0.01
                            && project(*a, *p, *q).1.abs() < ctol
                    })
            })
        })
        .map(|(id, _)| id.clone())
}

/// Name the concepts of every surface texture expression: symbol type, value form, reading
/// direction, what the tip stands on, and the simplified note for most surfaces.
pub(super) fn texture_concepts(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("surface_texture_symbol")?;
    let roles: BTreeMap<&str, Vec<&str>> = {
        let mut m: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for x in &b.members {
            m.entry(x.source_handle.as_str())
                .or_default()
                .push(x.member_role.as_str());
        }
        m
    };
    let mut plans: Vec<(usize, Vec<&'static str>, Vec<&'static str>)> = Vec::new();
    let mut standing: Vec<(usize, String)> = Vec::new();
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "surface_roughness" {
            continue;
        }
        let mut concepts: Vec<&'static str> = vec!["DC-G6-071"];
        let mut variants: Vec<&'static str> = Vec::new();
        let label = b.exprs[i].label.clone();
        let value = label
            .trim_start_matches("其余")
            .trim()
            .trim_start_matches('√')
            .trim();
        if !value.is_empty() {
            concepts.push("DC-G6-045");
            if parameter_code(c, value) {
                concepts.extend(["DC-G6-046", "DC-G6-075"]);
                variants.push("current_edition_value");
            } else {
                concepts.push("DC-G6-027");
                variants.push("old_edition_value");
            }
        }
        if b.exprs[i]
            .qualifiers
            .iter()
            .any(|q| q == "general_requirement")
            || label.starts_with("其余")
        {
            concepts.push("DC-G6-006");
            variants.push("most_surfaces");
        }
        if let Some((t, rotated)) = texture_geometry(cx, b, i) {
            if t.bar {
                concepts.push("DC-G6-072");
            }
            if t.circle {
                concepts.push("DC-G6-063");
            }
            if t.extension {
                concepts.push("DC-G6-069");
            }
            if rotated {
                concepts.push("DC-G6-049");
                variants.push("reading_direction");
            }
            // what the tip stands on
            let tol = 0.08 * t.size;
            let mut on: BTreeSet<&str> = BTreeSet::new();
            for (id, (p, q)) in &cx.lines {
                if seg_dist(t.tip, *p, *q) < tol
                    && let Some(rs) = roles.get(id.as_str())
                {
                    on.extend(rs.iter().copied());
                }
            }
            for (id, (pts, _, _)) in &cx.polys {
                if pts.windows(2).any(|w| seg_dist(t.tip, w[0], w[1]) < tol)
                    && let Some(rs) = roles.get(id.as_str())
                {
                    on.extend(rs.iter().copied());
                }
            }
            if on.is_empty()
                && let Some(line) = standing_line(cx, b, t.tip, tol)
            {
                standing.push((i, line));
                concepts.push("DC-G6-052");
                variants.push("standing_on_feature");
            } else if on.contains("contour_segment") {
                concepts.push("DC-G6-053");
                variants.push("standing_on_feature");
            } else if on.contains("extension_line") {
                concepts.push("DC-G6-052");
                variants.push("standing_on_feature");
            } else if on.iter().any(|r| r.contains("leader")) {
                concepts.push("DC-G6-032");
                variants.push("standing_on_feature");
            } else if on.contains("dimension_line") {
                concepts.push("DC-G6-051");
                variants.push("standing_on_feature");
            }
        }
        if variants.is_empty() {
            variants.push("symbol_only");
        }
        plans.push((i, concepts, variants));
    }
    for (i, line) in standing {
        let lt = member_line_type(cx.lt(&line), "extension_line");
        b.member(
            i,
            &line,
            None,
            [0., 1.],
            "extension_line",
            lt,
            "candidate",
            vec![],
            vec!["standing_line_of_texture_symbol".into()],
        );
    }
    for (i, concepts, variants) in plans {
        for v in variants {
            name(&mut b.exprs[i], "surface_texture_symbol", v, &[]);
        }
        name(
            &mut b.exprs[i],
            "surface_texture_symbol",
            "symbol_type",
            &concepts,
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- view callouts

/// Cutting-plane strokes: a short line at the tail of a viewing-direction arrow, perpendicular
/// to it, marks where the cutting plane is (GB/T 4458.6); it joins the callout.
pub(super) fn cutting_strokes(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("view_callout")?;
    let smin = representation::param(c, "stroke_min")?;
    let smax = representation::param(c, "stroke_max")?;
    let reach = representation::param(c, "stroke_reach")?;
    for i in 0..b.exprs.len() {
        if b.exprs[i].rule != "viewing_direction_arrow_and_letter" {
            continue;
        }
        let id = b.exprs[i].id.clone();
        let shaft = b
            .members
            .iter()
            .find(|m| m.expression_id == id && m.member_role == "view_arrow")
            .map(|m| m.source_handle.clone());
        let head = b
            .members
            .iter()
            .find(|m| m.expression_id == id && m.member_role == "arrowhead")
            .map(|m| m.source_handle.clone());
        let (Some(shaft), Some(head)) = (shaft, head) else {
            continue;
        };
        let Some((p, q)) = cx.lines.get(&shaft).copied().or_else(|| {
            cx.polys
                .get(&shaft)
                .filter(|x| x.0.len() == 2)
                .map(|x| (x.0[0], x.0[1]))
        }) else {
            continue;
        };
        let Some(a) = cx.arrow(&head) else { continue };
        let tail = if dist(p, a.tip) > dist(q, a.tip) {
            p
        } else {
            q
        };
        let u = unit(sub(q, p));
        let best = cx
            .lines
            .iter()
            .filter(|(sid, (m, n))| {
                let l = dist(*m, *n);
                !b.used(sid)
                    && (smin * cx.scale..smax * cx.scale).contains(&l)
                    && dot(unit(sub(*n, *m)), u).abs() < 0.1
                    && seg_dist(tail, *m, *n) < reach * cx.scale
            })
            .min_by(|x, y| seg_dist(tail, x.1.0, x.1.1).total_cmp(&seg_dist(tail, y.1.0, y.1.1)))
            .map(|(sid, _)| sid.clone());
        if let Some(sid) = best {
            let lt = cx.lt(&sid);
            b.member(
                i,
                &sid,
                None,
                [0., 1.],
                "cutting_stroke",
                lt,
                "candidate",
                vec![],
                vec![],
            );
            name(
                &mut b.exprs[i],
                "view_callout",
                "cutting_plane_stroke",
                &["DC-G2-043", "DC-G1-007"],
            );
        }
    }
    Ok(())
}

/// A viewing-direction arrow drawn as one block: a shaft line, a closed (filled) head whose tip
/// is at one end of the shaft, and a single capital letter beside it (card view_callout,
/// variant arrow_block). Returns the letter.
pub(super) fn view_arrow_block(s: &dd::Shape) -> Option<String> {
    let c = representation::card_ref("view_callout").ok()?;
    let reach = representation::param(c, "letter_reach").ok()?;
    let letters: Vec<&(String, P, f64)> =
        s.texts.iter().filter(|t| !t.0.trim().is_empty()).collect();
    if letters.len() != 1 || s.circles.len() > 0 {
        return None;
    }
    let (t, at, h) = letters[0];
    let t = t.trim();
    if !(t.chars().count() == 1 && t.chars().all(|ch| ch.is_ascii_uppercase())) {
        return None;
    }
    let heads = closed_triangles(s);
    if heads.len() != 1 {
        return None;
    }
    let head = &heads[0];
    // shafts: lines not belonging to the head outline
    let shafts: Vec<&(P, P)> = s
        .lines
        .iter()
        .filter(|(a, b)| {
            !head.iter().any(|v| dist(*v, *a) < 1e-6) || !head.iter().any(|v| dist(*v, *b) < 1e-6)
        })
        .collect();
    if shafts.len() != 1 {
        return None;
    }
    let (a, b) = *shafts[0];
    let len = dist(a, b);
    // the head's apex is the vertex farthest from the shaft's line; it must lie beyond one end
    let tip_at_end = head.iter().any(|v| {
        let (t0, d) = project(*v, a, b);
        d.abs() < 0.15 * len && (t0 > 0.95 || t0 < 0.05)
    });
    let near = seg_dist(*at, a, b) < reach * h.max(1e-6)
        || dist(*at, mid(a, b)) < reach * h.max(1e-6) + 0.5 * len;
    (tip_at_end && near).then(|| t.to_string())
}

/// Section and cut names written `X—X` (optionally with a scale) title a section view.
pub(super) fn section_titles(cx: &Ctx, b: &mut Builder) -> Result<Vec<ViewTitle>> {
    let c = representation::card_ref("view_callout")?;
    let dashes = values(c, "DC-G2-065", "separator");
    let mut out = Vec::new();
    let texts: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id))
        .cloned()
        .collect();
    for t in texts {
        let mut parts = t.dec.text.split_whitespace();
        let Some(name_part) = parts.next() else {
            continue;
        };
        let scale: Option<String> = parts.next().filter(|s| tx::is_scale(s)).map(str::to_string);
        if parts.next().is_some() {
            continue;
        }
        let Some(sep) = dashes.iter().find(|d| name_part.contains(**d)) else {
            continue;
        };
        let (l, r) = name_part.split_once(sep).unwrap_or(("", ""));
        let ok = !l.is_empty()
            && l == r
            && l.chars().count() <= 2
            && l.chars().all(|ch| ch.is_ascii_uppercase());
        if !ok {
            continue;
        }
        let mut concepts = vec!["DC-G2-065"];
        if scale.is_some() {
            concepts.extend(["DC-G2-068", "DC-G1-115"]);
        }
        let e = b.expr(
            "view_callout",
            &t.dec.text,
            "card:view_callout/section_name",
            "candidate",
            vec!["view_title".into()],
            vec![format!("text:{}", t.id)],
            vec![],
        );
        name(&mut b.exprs[e], "view_callout", "section_name", &concepts);
        b.member(
            e,
            &t.id,
            None,
            [0., 1.],
            "view_title",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        out.push(ViewTitle {
            label: name_part.to_string(),
            center: t.center,
            height: t.height,
            kind: "section_view",
            scale,
            sources: vec![t.id.clone()],
        });
    }
    Ok(out)
}

/// Name the view-callout concepts of the callout rules.
pub(super) fn callout_concepts(b: &mut Builder) -> Result<()> {
    for e in b.exprs.iter_mut().filter(|e| e.kind == "view_callout") {
        match e.rule.as_str() {
            "view_letter_with_direction_suffix" | "viewing_direction_arrow_and_letter" => {
                name(e, "view_callout", "direction_view_name", &["DC-G2-072"])
            }
            "detail_numeral_over_scale" => name(
                e,
                "view_callout",
                "name_with_scale",
                &["DC-G2-068", "DC-G1-115"],
            ),
            _ => {}
        }
    }
    Ok(())
}
