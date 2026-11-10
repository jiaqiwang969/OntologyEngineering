//! Source-only revision identity and geometric correspondence. A later issue is a candidate,
//! never a release decision. Handles identify occurrences, not editing history.
use crate::{expression_text, native_readback::Drawing};
use anyhow::Result;
use oe_contracts::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Entity {
    handle: String,
    kind: String,
    tags: Vec<(i32, String)>,
}
impl Entity {
    fn get(&self, code: i32) -> Option<&str> {
        self.tags.iter().find(|t| t.0 == code).map(|t| t.1.as_str())
    }
    fn n(&self, code: i32) -> f64 {
        self.get(code).and_then(|s| s.parse().ok()).unwrap_or(0.)
    }
    fn anchor(&self) -> [f64; 2] {
        if self.kind == "TEXT"
            && (self.n(72) != 0. || self.n(73) != 0.)
            && ![3., 5.].contains(&self.n(72))
            && self.get(11).is_some()
        {
            return [self.n(11), self.n(21)];
        }
        let mut active = !matches!(self.kind.as_str(), "HATCH" | "POLYLINE");
        for (i, (c, v)) in self.tags.iter().enumerate() {
            if (*c == 91 && self.kind == "HATCH") || *c == 9998 {
                active = true;
            }
            if active && *c == 10 {
                return [
                    v.parse().unwrap_or(0.),
                    self.tags[i + 1..]
                        .iter()
                        .find(|t| t.0 == 20)
                        .and_then(|t| t.1.parse().ok())
                        .unwrap_or(0.),
                ];
            }
        }
        [0., 0.]
    }
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn translated(p: [f64; 2], d: [f64; 2]) -> [f64; 2] {
    [p[0] - d[0], p[1] - d[1]]
}
fn offset(a: &Entity, b: &Entity) -> [f64; 2] {
    let p = a.anchor();
    let q = b.anchor();
    [q[0] - p[0], q[1] - p[1]]
}
fn normalize(e: &Entity, d: [f64; 2], keep_style: bool, keep_text: bool) -> Vec<(i32, String)> {
    let mut hatch = false;
    let mut vertex = false;
    let mut out = Vec::new();
    for (c, v) in &e.tags {
        // Autodesk TEXT: justified placement comes from the second alignment point; the first
        // is recomputed from the string. Aligned/fit text keeps both endpoints in this finite profile.
        if e.kind == "TEXT" && ![3., 5.].contains(&e.n(72)) {
            let justified = e.n(72) != 0. || e.n(73) != 0.;
            if (justified && [10, 20, 30].contains(c)) || (!justified && [11, 21, 31].contains(c)) {
                continue;
            }
        }
        if [5, 105].contains(c) || (320..=369).contains(c) || (*c >= 1000 && *c != 9998) {
            continue;
        }
        if e.kind == "DIMENSION" && [2, 12, 22, 32].contains(c) {
            continue;
        }
        if !keep_style && [8, 6, 62, 370, 420, 430, 48, 60].contains(c) {
            continue;
        }
        if !keep_text
            && (matches!(e.kind.as_str(), "TEXT" | "MTEXT") && [1, 3].contains(c)
                || e.kind == "DIMENSION" && *c == 1)
        {
            continue;
        }
        if *c == 9998 {
            vertex = true;
        }
        if e.kind == "HATCH" {
            if *c == 91 {
                hatch = true;
            }
            if *c == 75 {
                hatch = false;
            }
        }
        let coords: &[i32] = match e.kind.as_str() {
            "LINE" => &[10, 11],
            "CIRCLE" | "ARC" | "POINT" | "MTEXT" | "INSERT" | "LWPOLYLINE" => &[10],
            "SOLID" => &[10, 11, 12, 13],
            "TEXT" => {
                if [3., 5.].contains(&e.n(72)) {
                    &[10, 11]
                } else if e.n(72) != 0. || e.n(73) != 0. {
                    &[11]
                } else {
                    &[10]
                }
            }
            "DIMENSION" => &[10, 11, 13, 14, 15, 16],
            "HATCH" if hatch => &[10, 11],
            "POLYLINE" if vertex => &[10],
            _ => &[],
        };
        let value = if coords.contains(c) {
            v.parse::<f64>().ok().map(|x| (x - d[0]).to_string())
        } else if coords.contains(&(c - 10)) {
            v.parse::<f64>().ok().map(|x| (x - d[1]).to_string())
        } else {
            None
        };
        out.push((*c, value.unwrap_or_else(|| v.clone())));
    }
    out
}
fn equal(a: &Entity, b: &Entity, d: [f64; 2], style: bool, text: bool, tol: f64) -> bool {
    if a.kind != b.kind {
        return false;
    }
    let aa = normalize(a, [0., 0.], style, text);
    let bb = normalize(b, d, style, text);
    aa.len() == bb.len() && aa.iter().zip(&bb).all(|((c, x), (k, y))| {
        c == k
            && (x == y
                || matches!((x.parse::<f64>(),y.parse::<f64>()),(Ok(u),Ok(v)) if (u-v).abs()<=tol))
    })
}
fn raw_entities(d: &Drawing, roots: &BTreeSet<&str>) -> Vec<Entity> {
    let mut out: Vec<Entity> = Vec::new();
    for r in d.records.iter().filter(|r| r.section == "ENTITIES") {
        if r.kind == "VERTEX" {
            if let Some(e) = out.last_mut().filter(|e| e.kind == "POLYLINE") {
                e.tags.push((9998, "VERTEX".into()));
                e.tags.extend(r.tags.iter().skip(1).cloned());
            }
            continue;
        }
        if let Some(h) = r.get(5).filter(|h| roots.contains(h)) {
            out.push(Entity {
                handle: h.into(),
                kind: r.kind.clone(),
                tags: r.tags.clone(),
            });
        }
    }
    out
}
fn delta(a: &Entity, bs: &[&Entity], kind: &str, shift: [f64; 2]) -> MdEntityDifference {
    MdEntityDifference {
        id: format!("delta:{}", a.handle),
        category: kind.into(),
        older_handles: vec![a.handle.clone()],
        newer_handles: bs.iter().map(|b| b.handle.clone()).collect(),
        translation: shift.to_vec(),
        evidence_ids: vec![
            "card:drawing_revision".into(),
            "source_geometry_correspondence_not_edit_history".into(),
        ],
    }
}
fn compare(
    old: &[Entity],
    new: &[Entity],
    shift: [f64; 2],
    tol: f64,
    fraction: f64,
) -> Vec<MdEntityDifference> {
    let mut useda = BTreeSet::new();
    let mut usedb = BTreeSet::new();
    let mut out = Vec::new();
    let mut shifts = vec![shift];
    // First align sheet frames; then infer repeated local-view translations from remaining shapes.
    for pass in 0..4 {
        let t = shifts[pass];
        for style in [true, false] {
            for (i, a) in old.iter().enumerate() {
                if useda.contains(&i) {
                    continue;
                }
                if let Some((j, b)) = new
                    .iter()
                    .enumerate()
                    .find(|(j, b)| !usedb.contains(j) && equal(a, b, t, style, true, tol))
                {
                    let category = if distance(t, shift) > tol {
                        "moved"
                    } else if style {
                        "same"
                    } else {
                        "style_changed"
                    };
                    out.push(delta(a, &[b], category, t));
                    useda.insert(i);
                    usedb.insert(j);
                }
            }
        }
        let mut votes: BTreeMap<(i64, i64), (usize, [f64; 2])> = BTreeMap::new();
        for (i, a) in old.iter().enumerate().filter(|(i, _)| !useda.contains(i)) {
            let _ = i;
            for (_, b) in new.iter().enumerate().filter(|(j, _)| !usedb.contains(j)) {
                let t = offset(a, b);
                if equal(a, b, t, false, true, tol) {
                    let k = ((t[0] / tol).round() as i64, (t[1] / tol).round() as i64);
                    votes.entry(k).or_insert((0, t)).0 += 1;
                }
            }
        }
        let Some((count, next)) = votes.values().max_by_key(|x| x.0).copied() else {
            break;
        };
        if count < 3 || pass == 3 {
            break;
        }
        shifts.push(next);
    }
    // Unique same-shape annotations can move independently of the view.
    for (i, a) in old
        .iter()
        .enumerate()
        .filter(|(_, a)| matches!(a.kind.as_str(), "INSERT" | "DIMENSION" | "TEXT" | "MTEXT"))
    {
        if useda.contains(&i) {
            continue;
        }
        let candidates: Vec<_> = new
            .iter()
            .enumerate()
            .filter(|(j, b)| !usedb.contains(j) && equal(a, b, offset(a, b), true, true, tol))
            .collect();
        if candidates.len() == 1 {
            let (j, b) = candidates[0];
            out.push(delta(a, &[b], "moved", offset(a, b)));
            useda.insert(i);
            usedb.insert(j);
        }
    }
    // A split retains a single old correspondence and several new occurrences. Multiplicity is not discarded.
    for (i, a) in old.iter().enumerate().filter(|(_, a)| a.kind == "LINE") {
        if useda.contains(&i) {
            continue;
        }
        let p = a.anchor();
        let q = [a.n(11), a.n(21)];
        let len = distance(p, q);
        if len <= tol {
            continue;
        }
        for t in &shifts {
            let along =
                |x: [f64; 2]| ((x[0] - p[0]) * (q[0] - p[0]) + (x[1] - p[1]) * (q[1] - p[1])) / len;
            let on = |x: [f64; 2]| {
                ((x[0] - p[0]) * (q[1] - p[1]) - (x[1] - p[1]) * (q[0] - p[0])).abs() / len <= tol
                    && along(x) >= -tol
                    && along(x) <= len + tol
            };
            let cs: Vec<_> = new
                .iter()
                .enumerate()
                .filter(|(j, b)| {
                    !usedb.contains(j)
                        && b.kind == "LINE"
                        && on(translated(b.anchor(), *t))
                        && on(translated([b.n(11), b.n(21)], *t))
                })
                .collect();
            let mut chosen = None;
            for (x, (j, b)) in cs.iter().enumerate() {
                for (k, c) in cs.iter().skip(x + 1) {
                    let endpoints = [
                        translated(b.anchor(), *t),
                        translated([b.n(11), b.n(21)], *t),
                        translated(c.anchor(), *t),
                        translated([c.n(11), c.n(21)], *t),
                    ];
                    let lo = endpoints
                        .iter()
                        .map(|p| along(*p))
                        .fold(f64::INFINITY, f64::min);
                    let hi = endpoints
                        .iter()
                        .map(|p| along(*p))
                        .fold(f64::NEG_INFINITY, f64::max);
                    let coverage = distance(b.anchor(), [b.n(11), b.n(21)])
                        + distance(c.anchor(), [c.n(11), c.n(21)]);
                    if lo.abs() <= tol
                        && (hi - len).abs() <= tol
                        && coverage >= fraction * len
                        && coverage <= len + tol
                    {
                        chosen = Some((*j, *k, *b, *c));
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }
            if let Some((j, k, b, c)) = chosen {
                out.push(delta(a, &[b, c], "geometry_changed", *t));
                useda.insert(i);
                usedb.insert(j);
                usedb.insert(k);
                break;
            }
        }
    }
    // Remaining edits: same dimension identity, nearby text, or nearest similar line endpoints.
    let mut candidates = Vec::new();
    for (i, a) in old.iter().enumerate().filter(|(i, _)| !useda.contains(i)) {
        for (j, b) in new
            .iter()
            .enumerate()
            .filter(|(j, b)| !usedb.contains(j) && a.kind == b.kind)
        {
            for t in &shifts {
                let pa = a.anchor();
                let pb = translated(b.anchor(), *t);
                let gap = distance(pa, pb);
                let score = match a.kind.as_str() {
                    "LINE" => {
                        let qa = [a.n(11), a.n(21)];
                        let qb = translated([b.n(11), b.n(21)], *t);
                        let s = (gap + distance(qa, qb)).min(distance(pa, qb) + distance(qa, pb));
                        (s <= distance(pa, qa).max(distance(pb, qb)) * fraction + tol).then_some(s)
                    }
                    "DIMENSION"
                        if a.get(1) == b.get(1)
                            && (a.n(42) - b.n(42)).abs() <= tol
                            && gap <= tol =>
                    {
                        Some(gap)
                    }
                    "TEXT" | "MTEXT"
                        if gap <= a.n(40).max(b.n(40)) * fraction + tol
                            && equal(a, b, offset(a, b), true, false, tol) =>
                    {
                        Some(gap)
                    }
                    _ => None,
                };
                if let Some(s) = score {
                    candidates.push((s, i, j, *t));
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, i, j, t) in candidates {
        if !useda.contains(&i) && !usedb.contains(&j) {
            let k = if matches!(old[i].kind.as_str(), "TEXT" | "MTEXT") {
                "text_changed"
            } else {
                "geometry_changed"
            };
            out.push(delta(&old[i], &[&new[j]], k, t));
            useda.insert(i);
            usedb.insert(j);
        }
    }
    for (i, a) in old.iter().enumerate().filter(|(i, _)| !useda.contains(i)) {
        let _ = i;
        out.push(delta(a, &[], "deleted", shift));
    }
    for (j, b) in new.iter().enumerate().filter(|(j, _)| !usedb.contains(j)) {
        let _ = j;
        out.push(MdEntityDifference {
            id: format!("added:{}", b.handle),
            category: "added".into(),
            older_handles: vec![],
            newer_handles: vec![b.handle.clone()],
            translation: shift.to_vec(),
            evidence_ids: vec!["source_unpaired_occurrence".into()],
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}
fn compact(s: &str) -> String {
    expression_text::decode(s)
        .text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}
fn field(
    g: &MdEngineeringExpressionGraph,
    d: &Drawing,
    sheet: &str,
    concept: &str,
) -> Option<(String, String)> {
    g.members
        .iter()
        .filter(|m| {
            m.view_id.as_deref() == Some(sheet)
                && m.evidence_ids.contains(&format!("field:{concept}"))
        })
        .find_map(|m| {
            d.entity(&m.source_handle)
                .ok()?
                .get(1)
                .map(|s| (compact(s), m.source_handle.clone()))
        })
}
fn gap(p: [f64; 2], b: &[f64]) -> f64 {
    (b[0] - p[0])
        .max(p[0] - b[2])
        .max(0.)
        .hypot((b[1] - p[1]).max(p[1] - b[3]).max(0.))
}
fn date(v: &MdDrawingView) -> Option<String> {
    let s = v
        .evidence_ids
        .iter()
        .find_map(|s| s.strip_prefix("title_block_date:"))?;
    (s.len() == 8 && s.bytes().all(|c| c.is_ascii_digit()))
        .then(|| format!("{}-{}-{}", &s[..4], &s[4..6], &s[6..]))
}

pub fn discover(
    bytes: &[u8],
    g: &MdEngineeringExpressionGraph,
) -> Result<Option<MdDrawingRevisions>> {
    let sheets: Vec<_> = g
        .views
        .iter()
        .filter(|v| v.kind == "sheet" && v.bbox.len() == 4)
        .collect();
    if sheets.len() < 2 {
        return Ok(None);
    }
    let d = Drawing::parse(bytes)?;
    let knowledge = crate::engineering_expression::representation_knowledge()?;
    let card = knowledge
        .cards
        .iter()
        .find(|c| c.id == "drawing_revision")
        .expect("validated revision card");
    let mut groups: BTreeMap<(String, String), Vec<(&MdDrawingView, MdDrawingRevision)>> =
        BTreeMap::new();
    for v in &sheets {
        let (Some((number, nh)), Some((name, th)), Some(date)) = (
            field(g, &d, &v.id, "DC-G9-018"),
            field(g, &d, &v.id, "DC-G9-019"),
            date(v),
        ) else {
            continue;
        };
        let notes: Vec<_> = v
            .evidence_ids
            .iter()
            .filter_map(|s| s.strip_prefix("revision_note:"))
            .filter(|h| {
                d.entity(h)
                    .ok()
                    .and_then(|r| Some((r.xy(10).ok()?, r.get(1)?)))
                    .is_some_and(|(p, s)| {
                        sheets
                            .iter()
                            .min_by(|a, b| gap(p, &a.bbox).total_cmp(&gap(p, &b.bbox)))
                            .is_some_and(|closest| closest.id == v.id)
                            && card
                                .markers
                                .iter()
                                .any(|m| s.to_lowercase().contains(&m.to_lowercase()))
                    })
            })
            .map(String::from)
            .collect();
        let mut evidence = vec![
            "card:drawing_revision".into(),
            format!("drawing_number:{number}"),
            format!("drawing_name:{name}"),
            format!("title_date:{date}"),
        ];
        evidence.extend(
            notes
                .iter()
                .map(|h| format!("attributed_revision_note:{h}")),
        );
        let mut handles = vec![nh, th];
        handles.extend(notes);
        groups.entry((number, name)).or_default().push((
            v,
            MdDrawingRevision {
                id: format!("revision:{}", v.id),
                sheet_id: v.id.clone(),
                date,
                status: "historical_candidate".into(),
                evidence_ids: evidence,
                source_handles: handles,
            },
        ));
    }
    let roots = g
        .source
        .entities
        .iter()
        .map(|e| e.handle.as_str())
        .collect();
    let entities = raw_entities(&d, &roots);
    let mut result = MdDrawingRevisions {
        identities: Vec::new(),
        comparisons: Vec::new(),
    };
    for ((number, name), mut versions) in groups {
        if versions.len() < 2 {
            continue;
        }
        versions.sort_by(|a, b| a.1.date.cmp(&b.1.date));
        if versions.windows(2).any(|w| {
            w[0].1.date == w[1].1.date
                || !w[1]
                    .1
                    .evidence_ids
                    .iter()
                    .any(|e| e.starts_with("attributed_revision_note:"))
        }) {
            continue;
        }
        versions.last_mut().unwrap().1.status = "current_candidate".into();
        for pair in versions.windows(2) {
            let (a, b) = (pair[0].0, pair[1].0);
            let shift = [b.bbox[0] - a.bbox[0], b.bbox[1] - a.bbox[1]];
            let assigned = |id: &str| {
                entities
                    .iter()
                    .filter(|e| {
                        sheets
                            .iter()
                            .min_by(|a, b| {
                                gap(e.anchor(), &a.bbox).total_cmp(&gap(e.anchor(), &b.bbox))
                            })
                            .is_some_and(|v| v.id == id)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let ratio = |name: &str| {
                card.parameters
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.value)
                    .ok_or_else(|| anyhow::anyhow!("revision_parameter_missing:{name}"))
            };
            let tol = (a.bbox[2] - a.bbox[0]).hypot(a.bbox[3] - a.bbox[1])
                * ratio("alignment_tolerance")?;
            result.comparisons.push(MdRevisionComparison {
                id: format!("comparison:{}:{}", a.id, b.id),
                older: pair[0].1.id.clone(),
                newer: pair[1].1.id.clone(),
                translation: shift.to_vec(),
                differences: compare(
                    &assigned(&a.id),
                    &assigned(&b.id),
                    shift,
                    tol.max(f64::EPSILON),
                    ratio("similarity_fraction")?,
                ),
            });
        }
        result.identities.push(MdDrawingIdentity {
            id: format!("drawing:{}", result.identities.len() + 1),
            number,
            name,
            revisions: versions.into_iter().map(|v| v.1).collect(),
        });
    }
    Ok((!result.identities.is_empty()).then_some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ent(h: &str, k: &str, t: &[(i32, &str)]) -> Entity {
        Entity {
            handle: h.into(),
            kind: k.into(),
            tags: std::iter::once((0, k.to_owned()))
                .chain(t.iter().map(|(c, v)| (*c, (*v).into())))
                .collect(),
        }
    }
    fn line(h: &str, y: &str, end: &str) -> Entity {
        ent(h, "LINE", &[(10, "0"), (20, y), (11, end), (21, y)])
    }
    fn txt(h: &str, x: &str, y: &str, s: &str) -> Entity {
        ent(h, "TEXT", &[(10, x), (20, y), (1, s), (40, "4")])
    }
    #[test]
    fn revision_comparison_preserves_seven_categories_and_split_multiplicity() {
        let old = vec![
            line("10", "0", "30"),
            txt("11", "2", "10", "Q"),
            line("12", "20", "36"),
            line("13", "30", "100"),
            txt("14", "2", "40", "before"),
            ent("15", "CIRCLE", &[(10, "2"), (20, "60"), (40, "9")]),
        ];
        let mut new = vec![
            line("20", "0", "30"),
            txt("21", "12", "12", "Q"),
            line("22", "20", "36"),
            line("23", "30", "45"),
            ent(
                "24",
                "LINE",
                &[(10, "55"), (20, "30"), (11, "100"), (21, "30")],
            ),
            txt("25", "3", "40", "after"),
            ent(
                "26",
                "ARC",
                &[(10, "2"), (20, "60"), (40, "3"), (50, "0"), (51, "90")],
            ),
        ];
        new[2].tags.push((62, "5".into()));
        let diffs = compare(&old, &new, [0., 0.], 1e-7, 0.5);
        let counts = diffs.iter().fold(BTreeMap::new(), |mut m, d| {
            *m.entry(d.category.as_str()).or_insert(0) += 1;
            m
        });
        assert_eq!(
            counts,
            BTreeMap::from([
                ("same", 1),
                ("moved", 1),
                ("style_changed", 1),
                ("geometry_changed", 1),
                ("text_changed", 1),
                ("added", 1),
                ("deleted", 1)
            ])
        );
        assert_eq!(
            diffs
                .iter()
                .find(|d| d.category == "geometry_changed")
                .unwrap()
                .newer_handles
                .len(),
            2
        );
        let mut duplicates = vec![line("31", "0", "30"), line("32", "0", "30")];
        let d = compare(&duplicates, &duplicates[..1], [0., 0.], 1e-7, 0.5);
        assert_eq!(d.iter().filter(|d| d.category == "deleted").count(), 1);
        duplicates[0].handle = "33".into();
        assert_eq!(
            compare(&duplicates, &duplicates, [0., 0.], 1e-7, 0.5).len(),
            2
        );
    }
    #[test]
    fn revision_justified_text_uses_the_semantic_alignment_point() {
        let mut old = txt("91", "2", "10", "old title");
        old.tags
            .extend([(72, "1".into()), (11, "15".into()), (21, "10".into())]);
        let mut new = old.clone();
        new.handle = "92".into();
        new.tags.iter_mut().find(|t| t.0 == 10).unwrap().1 = "-2".into();
        assert_eq!(
            compare(&[old.clone()], &[new.clone()], [0., 0.], 1e-7, 0.5)[0].category,
            "same"
        );
        new.tags.iter_mut().find(|t| t.0 == 1).unwrap().1 = "new longer title".into();
        assert_eq!(
            compare(&[old], &[new], [0., 0.], 1e-7, 0.5)[0].category,
            "text_changed"
        );
        let a = txt("93", "2", "10", "Q");
        let mut b = a.clone();
        b.handle = "94".into();
        b.tags.extend([(11, "44".into()), (21, "22".into())]);
        assert_eq!(compare(&[a], &[b], [0., 0.], 1e-7, 0.5)[0].category, "same");
    }
    // These unit fixtures supply title-field recognition to the revision owner. The texts themselves
    // still come from independent synthetic DXF bytes; title recognition is covered in view_layout.
    fn candidate(
        name: &str,
        date2: &str,
        note: bool,
        note_x: f64,
    ) -> (Vec<u8>, MdEngineeringExpressionGraph) {
        let mut es = vec![
            txt("A1", "40", "5", "PX.731.92"),
            txt("A2", "40", "9", "夹持座"),
            txt("B1", "160", "5", "PX.731.92"),
            txt("B2", "160", "9", name),
            txt("C1", &note_x.to_string(), "-4", "增加支承"),
            line("D1", "25", "20"),
        ];
        let mut bytes = String::from("0\nSECTION\n2\nENTITIES\n");
        for e in &mut es {
            e.tags.insert(1, (5, e.handle.clone()));
            e.tags.push((8, "0".into()));
            for (c, v) in &e.tags {
                bytes.push_str(&format!("{c}\n{v}\n"));
            }
        }
        bytes.push_str("0\nENDSEC\n0\nEOF\n");
        let mut g = crate::engineering_expression::discover(bytes.as_bytes()).unwrap();
        let template = g.members[0].clone();
        g.members.clear();
        g.views.clear();
        for (sid, x, date, hs) in [
            ("old", 0., "20310417", ["A1", "A2"]),
            ("new", 120., date2, ["B1", "B2"]),
        ] {
            let mut ev = vec![format!("title_block_date:{date}")];
            if note && sid == "new" {
                ev.push("revision_note:C1".into());
            }
            g.views.push(MdDrawingView {
                id: sid.into(),
                kind: "sheet".into(),
                label: sid.into(),
                scale_text: None,
                bbox: vec![x, 0., x + 90., 70.],
                label_source_ids: vec![],
                state: "candidate".into(),
                evidence_ids: ev,
                unknowns: vec![],
            });
            for (h, c) in hs.into_iter().zip(["DC-G9-018", "DC-G9-019"]) {
                let mut m = template.clone();
                m.view_id = Some(sid.into());
                m.source_handle = h.into();
                m.evidence_ids = vec![format!("field:{c}")];
                g.members.push(m);
            }
        }
        (bytes.into_bytes(), g)
    }
    #[test]
    fn revision_identity_requires_matching_fields_and_attributed_note() {
        let (b, g) = candidate("夹持座", "20320919", true, 150.);
        let r = discover(&b, &g).unwrap().unwrap();
        assert_eq!(r.identities.len(), 1);
        assert_eq!(r.identities[0].revisions.len(), 2);
        assert_eq!(r.identities[0].revisions[1].status, "current_candidate");
        assert!(
            r.identities[0].revisions[1]
                .source_handles
                .contains(&"C1".into())
        );
    }
    #[test]
    fn revision_identity_rejects_mismatched_ambiguous_or_unattributed_sources() {
        for (name, date, note, x) in [
            ("导向座", "20320919", true, 150.),
            ("夹持座", "20310417", true, 150.),
            ("夹持座", "20320919", false, 150.),
            ("夹持座", "20320919", true, 20.),
        ] {
            let (b, g) = candidate(name, date, note, x);
            assert!(discover(&b, &g).unwrap().is_none());
        }
    }
    #[test]
    fn revision_single_sheet_and_changed_handles_are_not_edits() {
        let (b, mut g) = candidate("夹持座", "20320919", true, 150.);
        g.views.truncate(1);
        assert!(discover(&b, &g).unwrap().is_none());
        assert_eq!(
            compare(
                &[line("E1", "0", "26")],
                &[line("F1", "0", "26")],
                [0., 0.],
                1e-7,
                0.5
            )[0]
            .category,
            "same"
        );
    }
}
