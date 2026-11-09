//! View layout of a drawing, read from cards `view_arrangement`, `drawing_sheet`, `detail_view`
//! and the view-name variants of `view_callout`.
//!
//! * Sheets (card `drawing_sheet`): one model space may hold several sheets, each a sheet frame
//!   with its own title block in it (GB/T 14689, GB/T 10609.1). Views, view ids and the pairing of
//!   dimension terminators stay inside one sheet. Two sheets with the same drawing number are two
//!   editions of one drawing: the later dated one with a revision note of its date supersedes the
//!   other (recorded as sheet evidence).
//! * Principal view by the first-angle arrangement (card `view_arrangement`, GB/T 14692 5.2.1,
//!   GB/T 4458.1 4.1): the view flanked by aligned untitled views in its row or column, or aligned
//!   with untitled views both in its row and its column. A titled axial section can be that view
//!   (disc and cover parts). The other aligned untitled views are named by their position. A
//!   region lying inside another view's region belongs to that view.
//! * Detail views written with loose entities (card `detail_view`): the boundary arc or circle of
//!   the enlarged view, and its title written inside that boundary.
use super::*;

// ---------------------------------------------------------------- sheets

/// One sheet of a model space that holds several: a sheet frame with a title block in it.
#[derive(Clone, Debug)]
pub(super) struct Sheet {
    pub(super) frame: [f64; 4],
    frame_source: String,
    /// Title-block expressions (builder indexes) inside the frame.
    titles: Vec<usize>,
}

fn param(card: &str, name: &str) -> Result<f64> {
    representation::param(representation::card_ref(card)?, name)
}

fn contains(outer: [f64; 4], inner: [f64; 4], margin: f64) -> bool {
    inner[0] >= outer[0] - margin
        && inner[1] >= outer[1] - margin
        && inner[2] <= outer[2] + margin
        && inner[3] <= outer[3] + margin
}

fn center(b: [f64; 4]) -> P {
    [(b[0] + b[2]) / 2., (b[1] + b[3]) / 2.]
}

fn block_member(b: &Builder, e: usize) -> Option<&str> {
    let id = &b.exprs[e].id;
    b.members
        .iter()
        .find(|m| {
            &m.expression_id == id
                && matches!(m.member_role.as_str(), "frame_border" | "native_block")
        })
        .map(|m| m.source_handle.as_str())
}

/// The sheets of a drawing whose model space holds more than one: every sheet-size border
/// block (rule `sheet_size_rectangular_border`) with a title block inside it. One sheet or none
/// gives an empty list: the whole model space is one sheet.
pub(super) fn sheet_frames(cx: &Ctx, b: &Builder) -> Vec<Sheet> {
    let Ok(within) = param("drawing_sheet", "title_block_inside") else {
        return vec![];
    };
    let within = within * cx.scale;
    let titles: Vec<(usize, [f64; 4])> = b
        .exprs
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "title_block")
        .filter_map(|(i, _)| {
            let h = block_member(b, i)?;
            Some((i, cx.bbox(h)?))
        })
        .collect();
    let mut sheets: Vec<Sheet> = b
        .exprs
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "sheet_frame" && e.rule == "sheet_size_rectangular_border")
        .filter_map(|(i, _)| {
            let h = block_member(b, i)?;
            let s = cx.shapes.get(h)?;
            // the largest rectangle of the block is the trimmed paper edge (or the frame)
            let frame = s
                .paths
                .iter()
                .filter_map(|p| cell_rect(p))
                .max_by(|x, y| area(*x).total_cmp(&area(*y)))
                .or_else(|| cx.bbox(h))?;
            let inside: Vec<usize> = titles
                .iter()
                .filter(|(_, t)| inside(center(*t), frame, within))
                .map(|(i, _)| *i)
                .collect();
            (!inside.is_empty()).then(|| Sheet {
                frame,
                frame_source: h.to_string(),
                titles: inside,
            })
        })
        .collect();
    if sheets.len() < 2 {
        return vec![];
    }
    // left to right, then top to bottom
    sheets.sort_by(|x, y| {
        x.frame[0]
            .total_cmp(&y.frame[0])
            .then(y.frame[3].total_cmp(&x.frame[3]))
    });
    sheets
}

/// The sheet holding a point (none on a one-sheet drawing or outside every sheet).
pub(super) fn sheet_at(sheets: &[Sheet], p: P) -> Option<usize> {
    sheets.iter().position(|s| inside(p, s.frame, 0.))
}

/// The sheet a region or expression belongs to: the one holding the centre of its box, else
/// the one it overlaps most, else the nearest.
fn sheet_of(sheets: &[Sheet], bb: [f64; 4]) -> Option<usize> {
    if sheets.is_empty() {
        return None;
    }
    if let Some(i) = sheet_at(sheets, center(bb)) {
        return Some(i);
    }
    let overlap = |s: &Sheet| {
        let w = (s.frame[2].min(bb[2]) - s.frame[0].max(bb[0])).max(0.);
        let h = (s.frame[3].min(bb[3]) - s.frame[1].max(bb[1])).max(0.);
        w * h
    };
    sheets
        .iter()
        .enumerate()
        .max_by(|x, y| {
            overlap(x.1)
                .total_cmp(&overlap(y.1))
                .then(bbox_gap(y.1.frame, bb).total_cmp(&bbox_gap(x.1.frame, bb)))
        })
        .map(|x| x.0)
}

/// Dimension terminators, leaders and other paired pieces never join across sheets.
pub(super) fn same_sheet(sheets: &[Sheet], p: P, q: P) -> bool {
    sheets.is_empty() || sheet_at(sheets, p) == sheet_at(sheets, q)
}

/// View id of the sheet region holding the given points (`view:sheet` on a one-sheet drawing).
pub(super) fn sheet_view(sheets: &[Sheet], pts: &[P]) -> String {
    match bbox_of(pts).and_then(|bb| sheet_of(sheets, bb)) {
        Some(i) => format!("view:sheet-{}", i + 1),
        None => "view:sheet".into(),
    }
}

fn prefix(sheets: &[Sheet], s: Option<usize>) -> String {
    match s {
        Some(i) if sheets.len() > 1 => format!("sheet-{}:", i + 1),
        _ => String::new(),
    }
}

/// A date written in a title block or a note: eight digits `yyyymmdd`, or year, month and day
/// separated by the date separators of card `drawing_sheet`. Returns `yyyymmdd`.
fn date_of(text: &str) -> Option<String> {
    let ok = |y: &str, m: &str, d: &str| {
        let (y, m, d) = (
            y.parse::<u32>().ok()?,
            m.parse::<u32>().ok()?,
            d.parse::<u32>().ok()?,
        );
        ((1900..2200).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d))
            .then(|| format!("{y:04}{m:02}{d:02}"))
    };
    let digits: Vec<&str> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(x) = digits
        .iter()
        .filter(|g| g.len() == 8)
        .find_map(|g| ok(&g[..4], &g[4..6], &g[6..]))
    {
        return Some(x);
    }
    let c = representation::card_ref("drawing_sheet").ok()?;
    let seps: Vec<char> = c
        .lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == "date_separator")
        .flat_map(|l| l.values.iter().flat_map(|v| v.chars()))
        .collect();
    // year, month and day written apart: digit groups joined only by date separators
    let mut groups: Vec<Vec<String>> = vec![vec![]];
    let mut cur = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            cur.push(ch);
            continue;
        }
        if !cur.is_empty() {
            groups.last_mut()?.push(std::mem::take(&mut cur));
        }
        if !seps.contains(&ch) {
            groups.push(vec![]);
        }
    }
    groups.iter().find_map(|g| {
        g.windows(3)
            .find(|w| w[0].len() == 4 && w[1].len() <= 2 && w[2].len() <= 2)
            .and_then(|w| ok(&w[0], &w[1], &w[2]))
    })
}

/// Sheet regions; on a drawing with several sheets each records its frame, its title block, its
/// title-block date and drawing number, and which sheet supersedes which.
pub(super) fn sheet_regions(cx: &Ctx, b: &Builder, sheets: &[Sheet]) -> Vec<Region> {
    if sheets.len() < 2 {
        return vec![Region {
            id: "view:sheet".into(),
            kind: "sheet",
            label: "图纸".into(),
            bbox: cx.extent,
            scale: None,
            label_sources: vec![],
            state: "observed",
            evidence: vec!["drawing_extent".into()],
            nodes: 0,
        }];
    }
    // title-block date and drawing number of each sheet
    let facts: Vec<(Option<String>, Option<String>)> = sheets
        .iter()
        .map(|s| {
            let mut date = None;
            let mut number = None;
            for &t in &s.titles {
                let id = &b.exprs[t].id;
                for m in b.members.iter().filter(|m| &m.expression_id == id) {
                    let Some(tf) = cx.texts.get(&m.source_handle) else {
                        continue;
                    };
                    if m.evidence_ids.iter().any(|x| x == "field:DC-G9-018") {
                        number.get_or_insert_with(|| tf.dec.text.replace(' ', ""));
                    } else if let Some(d) = date_of(&tf.dec.text) {
                        date = date.max(Some(d));
                    }
                }
            }
            (date, number)
        })
        .collect();
    let in_title = |h: &str| {
        sheets.iter().flat_map(|s| s.titles.iter()).any(|&t| {
            let id = &b.exprs[t].id;
            b.members
                .iter()
                .any(|m| &m.expression_id == id && m.source_handle == h)
        })
    };
    // revision notes: texts outside every sheet frame carrying a date
    let notes: Vec<(&String, String)> = cx
        .texts
        .values()
        .filter(|t| sheet_at(sheets, t.center).is_none() && !in_title(&t.id))
        .filter_map(|t| date_of(&t.dec.text).map(|d| (&t.id, d)))
        .collect();
    let mut out = Vec::new();
    for (i, s) in sheets.iter().enumerate() {
        let mut evidence = vec![
            "sheet_frame_with_title_block".to_string(),
            "card:drawing_sheet".into(),
            "variant:frame_with_title_block".into(),
            format!("frame:{}", s.frame_source),
        ];
        let (date, number) = &facts[i];
        if let Some(d) = date {
            evidence.push(format!("title_block_date:{d}"));
        }
        if let Some(n) = number {
            evidence.push(format!("drawing_number:{n}"));
        }
        for (j, (od, on)) in facts.iter().enumerate() {
            if j == i || number.is_none() || on != number {
                continue;
            }
            evidence.push(format!("same_drawing_number:view:sheet-{}", j + 1));
            // the later edition, confirmed by a revision note of its date, supersedes the other
            let note = |d: &Option<String>| {
                notes
                    .iter()
                    .find(|(_, x)| Some(x) == d.as_ref())
                    .map(|n| n.0)
            };
            match (date, od) {
                (Some(a), Some(c)) if a > c => {
                    if let Some(n) = note(date) {
                        evidence.push("variant:later_revision".into());
                        evidence.push(format!("revision_note:{n}"));
                        evidence.push(format!("supersedes:view:sheet-{}", j + 1));
                    }
                }
                (Some(a), Some(c)) if a < c && note(od).is_some() => {
                    evidence.push(format!("superseded_by:view:sheet-{}", j + 1));
                }
                _ => {}
            }
        }
        out.push(Region {
            id: format!("view:sheet-{}", i + 1),
            kind: "sheet",
            label: format!("图纸 {}", i + 1),
            bbox: s.frame,
            scale: None,
            label_sources: vec![],
            state: "observed",
            evidence,
            nodes: 0,
        });
    }
    out
}

// ---------------------------------------------------------------- regions and their arrangement

/// A view cluster while the layout is read.
struct Cl {
    nodes: Vec<usize>,
    bbox: [f64; 4],
    sheet: Option<usize>,
}

/// Merge stray fragments only within one sheet (used by the fragment merge in `views`).
pub(super) fn same_sheet_boxes(sheets: &[Sheet], a: [f64; 4], b: [f64; 4]) -> bool {
    sheets.is_empty() || sheet_of(sheets, a) == sheet_of(sheets, b)
}

/// Members of a detail-view boundary (card `detail_view`): their circle and the region holding
/// them take a title written inside the boundary.
fn boundary_circles(cx: &Ctx, b: &Builder) -> BTreeMap<String, (P, f64)> {
    b.exprs
        .iter()
        .filter(|e| e.rule.starts_with("card:detail_view/boundary_"))
        .flat_map(|e| b.members.iter().filter(move |m| m.expression_id == e.id))
        .filter_map(|m| {
            cx.arc(&m.source_handle)
                .map(|(c, r, ..)| (m.source_handle.clone(), (c, r)))
        })
        .collect()
}

/// Indexes of the view nodes that are detail-view boundaries: they are not clustered by
/// proximity (their box covers what is near the arc) but join the view they enclose.
pub(super) fn boundary_nodes(b: &Builder, nodes: &[(String, [f64; 4])]) -> BTreeSet<usize> {
    let sources: BTreeSet<&str> = b
        .exprs
        .iter()
        .filter(|e| e.rule.starts_with("card:detail_view/boundary_"))
        .flat_map(|e| b.members.iter().filter(move |m| m.expression_id == e.id))
        .map(|m| m.source_handle.as_str())
        .collect();
    nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| sources.contains(n.0.as_str()))
        .map(|(i, _)| i)
        .collect()
}

/// Titles attached to clusters: a title written inside a detail boundary first, then a title
/// directly above a region (within its width ± 3 h, from 0.5 h below its top to 12 h above).
/// Returns the title of each cluster and the titles attached inside a boundary.
fn attach(
    cl: &[Cl],
    nodes: &[(String, [f64; 4])],
    titles: &[ViewTitle],
    sheets: &[Sheet],
    boundary: &BTreeMap<String, (P, f64)>,
) -> (BTreeMap<usize, usize>, BTreeSet<usize>) {
    let mut taken = BTreeSet::new();
    let mut title_of: BTreeMap<usize, usize> = BTreeMap::new();
    let mut done = BTreeSet::new();
    for (ti, t) in titles
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind == "detail_view")
    {
        let ts = sheet_at(sheets, t.center);
        let hit = cl.iter().enumerate().find(|(i, c)| {
            !taken.contains(i)
                && (sheets.is_empty() || c.sheet == ts)
                && c.nodes.iter().any(|&k| {
                    boundary
                        .get(&nodes[k].0)
                        .is_some_and(|(o, r)| dist(t.center, *o) < *r && t.center[1] > o[1])
                })
        });
        if let Some((i, _)) = hit {
            taken.insert(i);
            title_of.insert(i, ti);
            done.insert(ti);
        }
    }
    for (ti, t) in titles.iter().enumerate() {
        if done.contains(&ti) {
            continue;
        }
        let h = t.height;
        let ts = sheet_at(sheets, t.center);
        let best = cl
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                !taken.contains(i)
                    && (sheets.is_empty() || ts.is_none() || c.sheet == ts)
                    && t.center[0] >= c.bbox[0] - 3. * h
                    && t.center[0] <= c.bbox[2] + 3. * h
                    && t.center[1] >= c.bbox[3] - 0.5 * h
                    && t.center[1] - c.bbox[3] <= 12. * h
            })
            .min_by(|x, y| (t.center[1] - x.1.bbox[3]).total_cmp(&(t.center[1] - y.1.bbox[3])))
            .map(|(i, _)| i);
        if let Some(i) = best {
            taken.insert(i);
            title_of.insert(i, ti);
        }
    }
    (title_of, done)
}

/// Extent of a cluster's visible outline (its whole box when it has none).
fn outline(b: &Builder, nodes: &[(String, [f64; 4])], c: &Cl) -> [f64; 4] {
    c.nodes
        .iter()
        .filter(|&&k| b.kinds_of(&nodes[k].0).contains("visible_contour"))
        .map(|&k| nodes[k].1)
        .reduce(bbox_union)
        .unwrap_or(c.bbox)
}

/// Where `q` lies from `p` when the two are aligned: (axis, side) with axis 1 = row (高平齐,
/// side +1 right) and axis 0 = column (长对正, side +1 above).
fn aligned(p: [f64; 4], q: [f64; 4], min: f64) -> Option<(usize, i32)> {
    let ratio = |a: usize| {
        let ov = p[a + 2].min(q[a + 2]) - p[a].max(q[a]);
        let len = (p[a + 2] - p[a]).max(q[a + 2] - q[a]);
        if len > 1e-9 { ov / len } else { 0. }
    };
    if ratio(1) >= min {
        if q[0] >= p[2] {
            return Some((1, 1));
        }
        if q[2] <= p[0] {
            return Some((1, -1));
        }
    }
    if ratio(0) >= min {
        if q[1] >= p[3] {
            return Some((0, 1));
        }
        if q[3] <= p[1] {
            return Some((0, -1));
        }
    }
    None
}

/// The principal view of one sheet and the first-angle names of the other basic views.
struct Arrangement {
    principal: Option<usize>,
    evidence: Vec<String>,
    names: BTreeMap<usize, (&'static str, &'static str)>,
}

fn arrange(
    b: &Builder,
    nodes: &[(String, [f64; 4])],
    cl: &[Cl],
    group: &[usize],
    title_of: &BTreeMap<usize, usize>,
    titles: &[ViewTitle],
) -> Result<Arrangement> {
    let min = param("view_arrangement", "alignment_min")?;
    let pieces = param("view_arrangement", "min_pieces")? as usize;
    let out: BTreeMap<usize, [f64; 4]> = group
        .iter()
        .map(|&i| (i, outline(b, nodes, &cl[i])))
        .collect();
    let untitled = |i: usize| !title_of.contains_key(&i) && cl[i].nodes.len() >= pieces;
    let section = |i: usize| {
        title_of
            .get(&i)
            .is_some_and(|&t| titles[t].kind == "section_view")
            && cl[i].nodes.len() >= pieces
    };
    // aligned untitled neighbours of each candidate: (axis, side, index)
    let partners = |h: usize| -> Vec<(usize, i32, usize)> {
        group
            .iter()
            .filter(|&&p| p != h && untitled(p))
            .filter_map(|&p| aligned(out[&h], out[&p], min).map(|(a, s)| (a, s, p)))
            .collect()
    };
    let mut hubs: Vec<(usize, usize, &'static str)> = Vec::new();
    for &h in group.iter().filter(|&&h| untitled(h) || section(h)) {
        let ps = partners(h);
        let has = |a: usize, s: i32| ps.iter().any(|p| p.0 == a && p.1 == s);
        let variant = if (has(1, 1) && has(1, -1)) || (has(0, 1) && has(0, -1)) {
            "flanked"
        } else if (has(1, 1) || has(1, -1)) && (has(0, 1) || has(0, -1)) {
            "row_and_column"
        } else {
            continue;
        };
        hubs.push((h, ps.len(), variant));
    }
    hubs.sort_by(|x, y| {
        y.1.cmp(&x.1)
            .then(cl[y.0].nodes.len().cmp(&cl[x.0].nodes.len()))
    });
    let mut evidence = vec![];
    let principal = if let Some(&(h, _, variant)) = hubs.first() {
        evidence.push("first_angle_arrangement_centre".to_string());
        evidence.push("card:view_arrangement".into());
        evidence.push(format!("variant:{variant}"));
        if section(h) {
            evidence.push("variant:section_principal".into());
        }
        Some(h)
    } else {
        // two aligned views only: the principal is the left one of a row or the upper one of a
        // column (left and top views lie right of and below it)
        let pair = group
            .iter()
            .filter(|&&h| untitled(h) || section(h))
            .filter_map(|&h| {
                let ps = partners(h);
                (ps.len() == 1
                    && ((ps[0].0 == 1 && ps[0].1 == 1) || (ps[0].0 == 0 && ps[0].1 == -1)))
                    .then_some(h)
            })
            .max_by(|x, y| cl[*x].nodes.len().cmp(&cl[*y].nodes.len()));
        if let Some(h) = pair {
            evidence.push("first_angle_arrangement_pair".to_string());
            evidence.push("card:view_arrangement".into());
            evidence.push("variant:two_aligned_views".into());
            if section(h) {
                evidence.push("variant:section_principal".into());
            }
            Some(h)
        } else {
            group
                .iter()
                .copied()
                .find(|i| !title_of.contains_key(i))
                .inspect(|_| evidence.push("largest_untitled_region".to_string()))
        }
    };
    let mut names = BTreeMap::new();
    if let Some(h) = principal.filter(|_| evidence.iter().any(|x| x == "card:view_arrangement")) {
        let mut side: BTreeMap<(usize, i32), Vec<usize>> = BTreeMap::new();
        for (a, s, p) in partners(h) {
            side.entry((a, s)).or_default().push(p);
        }
        for ((a, s), mut ps) in side {
            // nearest first
            ps.sort_by(|x, y| bbox_gap(out[&h], out[x]).total_cmp(&bbox_gap(out[&h], out[y])));
            let named: &[(&'static str, &'static str)] = match (a, s) {
                (1, 1) => &[("左视图", "left_view"), ("后视图", "rear_view")],
                (1, -1) => &[("右视图", "right_view")],
                (0, -1) => &[("俯视图", "top_view")],
                _ => &[("仰视图", "bottom_view")],
            };
            for (p, n) in ps.iter().zip(named) {
                names.insert(*p, *n);
            }
        }
    }
    Ok(Arrangement {
        principal,
        evidence,
        names,
    })
}

/// Regions of the clusters of one drawing: detail views are titled through their boundary,
/// untitled fragments inside another view's region join it, and on every sheet the principal
/// view and the basic views are read from the first-angle arrangement.
pub(super) fn regions(
    cx: &Ctx,
    b: &Builder,
    nodes: &[(String, [f64; 4])],
    clusters: Vec<(Vec<usize>, [f64; 4])>,
    titles: &[ViewTitle],
    sheets: &[Sheet],
) -> Result<(Vec<Region>, BTreeMap<String, String>)> {
    let margin = param("view_arrangement", "containment_margin")? * cx.scale;
    let boundary = boundary_circles(cx, b);
    let mut cl: Vec<Cl> = clusters
        .into_iter()
        .map(|(nodes, bbox)| Cl {
            nodes,
            bbox,
            sheet: sheet_of(sheets, bbox),
        })
        .collect();
    // A detail boundary joins the region with the most pieces inside its circle.
    let lone: Vec<usize> = (0..cl.len())
        .filter(|&k| {
            cl[k]
                .nodes
                .iter()
                .all(|&n| boundary.contains_key(&nodes[n].0))
        })
        .collect();
    for &k in lone.iter().rev() {
        let (o, r) = boundary[&nodes[cl[k].nodes[0]].0];
        let inside_of = |c: &Cl| {
            c.nodes
                .iter()
                .filter(|&&n| {
                    !boundary.contains_key(&nodes[n].0) && dist(center(nodes[n].1), o) <= r
                })
                .count()
        };
        let target = (0..cl.len())
            .filter(|&j| j != k && cl[j].sheet == cl[k].sheet)
            .max_by_key(|&j| inside_of(&cl[j]))
            .filter(|&j| inside_of(&cl[j]) > 0);
        if let Some(j) = target {
            let moved = cl.remove(k);
            let j = if j > k { j - 1 } else { j };
            cl[j].nodes.extend(moved.nodes);
            cl[j].bbox = bbox_union(cl[j].bbox, moved.bbox);
        }
    }
    // A region lying inside another view's region is part of it, unless a title or a detail
    // boundary makes it a view of its own.
    loop {
        let (title_of, _) = attach(&cl, nodes, titles, sheets, &boundary);
        let own = |i: usize| {
            title_of.contains_key(&i)
                || cl[i]
                    .nodes
                    .iter()
                    .any(|&k| boundary.contains_key(&nodes[k].0))
        };
        let hit = (0..cl.len()).filter(|&k| !own(k)).find_map(|k| {
            (0..cl.len())
                .filter(|&j| {
                    j != k
                        && cl[j].sheet == cl[k].sheet
                        && area(cl[j].bbox) > area(cl[k].bbox)
                        && contains(cl[j].bbox, cl[k].bbox, margin)
                })
                .min_by(|x, y| area(cl[*x].bbox).total_cmp(&area(cl[*y].bbox)))
                .map(|j| (k, j))
        });
        let Some((k, j)) = hit else { break };
        let moved = cl.remove(k);
        let j = if j > k { j - 1 } else { j };
        cl[j].nodes.extend(moved.nodes);
        cl[j].bbox = bbox_union(cl[j].bbox, moved.bbox);
    }
    #[allow(clippy::unnecessary_sort_by)]
    cl.sort_by(|a, c| c.nodes.len().cmp(&a.nodes.len()));
    let (title_of, inside_boundary) = attach(&cl, nodes, titles, sheets, &boundary);
    // the arrangement of each sheet
    let mut groups: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for (i, c) in cl.iter().enumerate() {
        groups
            .entry(if sheets.is_empty() { None } else { c.sheet })
            .or_default()
            .push(i);
    }
    let mut principal = BTreeMap::new();
    let mut names = BTreeMap::new();
    for g in groups.values() {
        let a = arrange(b, nodes, &cl, g, &title_of, titles)?;
        if let Some(p) = a.principal {
            principal.insert(p, a.evidence);
        }
        names.extend(a.names);
    }
    let mut extra = 0;
    let mut regions: Vec<Region> = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, c) in cl.iter().enumerate() {
        let pre = prefix(sheets, c.sheet);
        let has_break = c
            .nodes
            .iter()
            .any(|&k| b.kinds_of(&nodes[k].0).contains("break_line"));
        let titled = title_of.get(&i).map(|&ti| &titles[ti]);
        let (mut id, kind, label, scale, sources, state, mut evidence) =
            match (titled, principal.get(&i), names.get(&i)) {
                (Some(t), Some(ev), _) => (
                    format!("view:{pre}principal"),
                    "principal",
                    t.label.clone(),
                    t.scale.clone(),
                    t.sources.clone(),
                    "candidate",
                    std::iter::once("title_directly_above_region".to_string())
                        .chain(ev.iter().cloned())
                        .collect(),
                ),
                (Some(t), None, _) => (
                    format!("view:{pre}{}", t.label.replace(' ', "")),
                    if t.kind == "detail_view" || t.kind == "section_view" {
                        t.kind
                    } else if has_break {
                        "partial_view"
                    } else {
                        "direction_view"
                    },
                    t.label.clone(),
                    t.scale.clone(),
                    t.sources.clone(),
                    "candidate",
                    vec![if title_of
                        .get(&i)
                        .is_some_and(|ti| inside_boundary.contains(ti))
                    {
                        "title_inside_detail_boundary".to_string()
                    } else {
                        "title_directly_above_region".to_string()
                    }],
                ),
                (None, Some(ev), _) => (
                    format!("view:{pre}principal"),
                    "principal",
                    "主视图".to_string(),
                    None,
                    vec![],
                    "candidate",
                    ev.clone(),
                ),
                (None, None, Some((cn, en))) => (
                    format!("view:{pre}{cn}"),
                    "direction_view",
                    cn.to_string(),
                    None,
                    vec![],
                    "candidate",
                    vec![
                        format!("first_angle_position:{en}"),
                        "card:view_arrangement".into(),
                        "variant:basic_view_names".into(),
                    ],
                ),
                (None, None, None) => {
                    extra += 1;
                    (
                        format!("view:{pre}untitled-{extra}"),
                        "partial_view",
                        format!("未命名视图 {extra}"),
                        None,
                        vec![],
                        "unknown",
                        vec!["untitled_region".to_string()],
                    )
                }
            };
        if !pre.is_empty() {
            evidence.push(format!(
                "on_sheet:view:sheet-{}",
                c.sheet.map_or(0, |s| s + 1)
            ));
        }
        // view ids are unique: a repeated title on one sheet gets a running number
        let base = id.clone();
        let mut n = 1;
        while !seen.insert(id.clone()) {
            n += 1;
            id = format!("{base}-{n}");
        }
        regions.push(Region {
            id,
            kind,
            label,
            bbox: c.bbox,
            scale,
            label_sources: sources,
            state,
            evidence,
            nodes: c.nodes.len(),
        });
    }
    let node_view: BTreeMap<String, String> = cl
        .iter()
        .enumerate()
        .flat_map(|(i, c)| c.nodes.iter().map(move |&k| (nodes[k].0.clone(), i)))
        .map(|(s, i)| (s, regions[i].id.clone()))
        .collect();
    Ok((regions, node_view))
}

// ---------------------------------------------------------------- references

/// Letters of a section name `X—X` (the same letters on both sides of a dash).
pub(super) fn section_letters(label: &str) -> Option<String> {
    let c = representation::card_ref("view_callout").ok()?;
    let compact: String = label.split_whitespace().next()?.to_string();
    let dashes: Vec<&str> = c
        .lexicon
        .iter()
        .flatten()
        .filter(|l| l.concept == "DC-G2-065" && l.entry == "separator")
        .flat_map(|l| l.values.iter().map(String::as_str))
        .collect();
    let sep = dashes.iter().find(|d| compact.contains(**d))?;
    let (l, r) = compact.split_once(sep)?;
    (!l.is_empty() && l == r).then(|| l.to_string())
}

/// A callout refers to the view its letter or numeral names, on its own sheet: `X向` and
/// `X …` titles by the letter, section names `X—X` by the letter, detail titles by the value of
/// the numeral (Ⅲ and III are one numeral).
pub(super) fn refs(
    b: &Builder,
    regions: &[Region],
    map: &BTreeMap<usize, String>,
    sheets: &[Sheet],
) -> BTreeMap<usize, String> {
    let sheet_of_view = |id: &str| -> Option<String> {
        if sheets.len() < 2 {
            return None;
        }
        id.strip_prefix("view:")?
            .split(':')
            .next()
            .map(str::to_string)
    };
    let mut refs = BTreeMap::new();
    for (i, e) in b.exprs.iter().enumerate() {
        if e.kind != "view_callout"
            || e.qualifiers
                .iter()
                .any(|q| q == "view_title" || q == "detail_title" || q == "local_section_name")
        {
            continue;
        }
        let key = e.label.split_whitespace().next().unwrap_or("");
        if key.is_empty() {
            continue;
        }
        let own = map.get(&i).and_then(|v| sheet_of_view(v));
        let here =
            |r: &&Region| r.kind != "sheet" && (own.is_none() || sheet_of_view(&r.id) == own);
        let value = tx::roman_value(key);
        let found = regions
            .iter()
            .filter(here)
            .find(|r| r.label == format!("{key}向") || r.label.starts_with(&format!("{key} ")))
            .or_else(|| {
                regions
                    .iter()
                    .filter(here)
                    .find(|r| section_letters(&r.label).as_deref() == Some(key))
            })
            .or_else(|| {
                value.and_then(|v| {
                    regions.iter().filter(here).find(|r| {
                        r.kind == "detail_view"
                            && r.label.split_whitespace().next().and_then(tx::roman_value)
                                == Some(v)
                    })
                })
            });
        if let Some(r) = found {
            refs.insert(i, r.id.clone());
        }
    }
    refs
}

// ---------------------------------------------------------------- view names and callouts

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let v = sub(b, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), v) / n).clamp(0., 1.);
    dist(p, add(a, mul(v, t)))
}

fn lexicon(card: &str, entry: &str) -> Vec<String> {
    representation::card_ref(card)
        .map(|c| {
            c.lexicon
                .iter()
                .flatten()
                .filter(|l| l.entry == entry)
                .flat_map(|l| l.values.iter().cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn push_ev(b: &mut Builder, e: usize, x: &str) {
    if !b.exprs[e].evidence_ids.iter().any(|y| y == x) {
        b.exprs[e].evidence_ids.push(x.to_string());
    }
}

/// A view letter: one or two capital Latin letters (GB/T 4458.1, GB/T 17452).
fn view_letter(s: &str) -> bool {
    let t = s.trim();
    (1..=2).contains(&t.chars().count()) && t.chars().all(|c| c.is_ascii_uppercase())
}

/// Filled triangles of a block shape (SOLID heads and closed three-vertex outlines).
fn heads(s: &dd::Shape) -> Vec<Vec<P>> {
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

/// The shaft of an arrow drawn as one block: one line and one filled head whose apex lies at an
/// end of the line. Returns (tail, end at the head).
fn block_shaft(s: &dd::Shape) -> Option<(P, P)> {
    let hs = heads(s);
    let [head] = hs.as_slice() else { return None };
    let shafts: Vec<&(P, P)> = s
        .lines
        .iter()
        .filter(|(a, c)| {
            !head.iter().any(|v| dist(*v, *a) < 1e-6) || !head.iter().any(|v| dist(*v, *c) < 1e-6)
        })
        .collect();
    let &[&(a, c)] = shafts.as_slice() else {
        return None;
    };
    let len = dist(a, c);
    let apex_at = |e: P| head.iter().any(|v| dist(*v, e) < 0.15 * len);
    if apex_at(c) {
        Some((a, c))
    } else if apex_at(a) {
        Some((c, a))
    } else {
        None
    }
}

/// Arrows of view callouts whose letter is drawn apart from the arrow (card view_callout, variant
/// arrow_and_letter_apart): an arrow block without a letter of its own with a loose capital letter
/// beside it, or a loose arrow (filled head and collinear shaft) with a block holding only the
/// letter. Runs before dimension rules can take the head.
pub(super) fn arrows_and_letters(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let reach = param("view_callout", "letter_reach")?;
    let rule = "card:view_callout/arrow_and_letter_apart";
    let ev = |src: &str, letter: &str| {
        vec![
            format!("source:{src}"),
            format!("letter:{letter}"),
            "card:view_callout".to_string(),
            "variant:arrow_and_letter_apart".into(),
            "concept:DC-G2-072".into(),
        ]
    };
    // (a) an arrow block with no letter, and a loose letter beside its shaft
    let blocks: Vec<(String, P, P)> = cx
        .shapes
        .iter()
        .filter(|(h, s)| {
            !b.used(h) && s.texts.iter().all(|t| t.0.trim().is_empty()) && s.circles.is_empty()
        })
        .filter_map(|(h, s)| block_shaft(s).map(|(t, e)| (h.clone(), t, e)))
        .collect();
    for (h, tail, end) in blocks {
        let letter = cx
            .texts
            .values()
            .filter(|t| !b.used(&t.id) && t.lex == Lex::Letters && view_letter(&t.dec.text))
            .filter(|t| seg_dist(t.center, tail, end) < reach * t.height)
            .min_by(|x, y| seg_dist(x.center, tail, end).total_cmp(&seg_dist(y.center, tail, end)))
            .cloned();
        let Some(t) = letter else { continue };
        let e = b.expr(
            "view_callout",
            &t.dec.text,
            rule,
            "candidate",
            vec!["view_direction".into()],
            ev(&h, &t.id),
            vec![],
        );
        b.member(
            e,
            &h,
            None,
            [0., 1.],
            "view_arrow",
            "block",
            "candidate",
            vec![],
            vec![],
        );
        b.member(
            e,
            &t.id,
            None,
            [0., 1.],
            "view_letter",
            "text",
            "candidate",
            vec![],
            vec![],
        );
    }
    // (b) a block holding only a capital letter, beside a loose arrow
    let letters: Vec<(String, String, P, f64)> = cx
        .shapes
        .iter()
        .filter(|(h, s)| {
            !b.used(h)
                && s.lines.is_empty()
                && s.circles.is_empty()
                && s.paths.is_empty()
                && s.arcs.is_empty()
        })
        .filter_map(|(h, s)| {
            let [(t, p, ht)] = s.texts.as_slice() else {
                return None;
            };
            // the insertion point is the lower left of an upright letter
            (view_letter(t) && *ht > 0.).then(|| {
                (
                    h.clone(),
                    t.trim().to_string(),
                    add(*p, [0.35 * ht, 0.5 * ht]),
                    *ht,
                )
            })
        })
        .collect();
    for (h, letter, at, ht) in letters {
        let arrow = cx
            .arrows
            .iter()
            .filter(|(aid, _)| !b.used(aid))
            .filter_map(|(aid, a)| {
                cx.lines
                    .iter()
                    .filter(|(lid, (p, q))| {
                        !b.used(lid)
                            && cx.annotation_style(lid)
                            && cross(unit(sub(*q, *p)), a.dir).abs() < 0.03
                            && dist(*p, *q) < 6. * a.len
                    })
                    .find_map(|(lid, (p, q))| {
                        let gap = |x: P| dist(x, a.tip).min(dist(x, base_of(a)));
                        let (near, far) = if gap(*p) <= gap(*q) {
                            (*p, *q)
                        } else {
                            (*q, *p)
                        };
                        continues(a, near, far, 0.15 * a.len)
                            .then(|| (aid.clone(), lid.clone(), far, a.tip))
                    })
            })
            .filter(|(_, _, far, tip)| seg_dist(at, *far, *tip) < reach * ht)
            .min_by(|x, y| seg_dist(at, x.2, x.3).total_cmp(&seg_dist(at, y.2, y.3)));
        let Some((aid, lid, _, _)) = arrow else {
            continue;
        };
        let e = b.expr(
            "view_callout",
            &letter,
            rule,
            "candidate",
            vec!["view_direction".into()],
            ev(&aid, &h),
            vec![],
        );
        b.member(
            e,
            &lid,
            None,
            [0., 1.],
            "view_arrow",
            member_line_type(cx.lt(&lid), "view_arrow"),
            "candidate",
            vec![],
            vec![],
        );
        b.member(
            e,
            &aid,
            None,
            [0., 1.],
            "arrowhead",
            "fill",
            "candidate",
            vec![],
            vec![],
        );
        b.member(
            e,
            &h,
            None,
            [0., 1.],
            "view_letter",
            "block",
            "candidate",
            vec![],
            vec![],
        );
    }
    Ok(())
}

/// View names that are not one text of the form `X—X`: section names written as two letters
/// with a dash between them, detail names (Roman numeral or capital letter, possibly with the
/// word for a rotated view, over or beside the scale), and section names standing on a leader
/// shoulder (the name of a local section, not of the view below).
pub(super) fn titles(cx: &Ctx, b: &mut Builder, titles: &mut Vec<ViewTitle>) -> Result<()> {
    split_section_names(cx, b, titles)?;
    detail_names(cx, b, titles)?;
    names_on_leaders(cx, b, titles)?;
    Ok(())
}

/// Boxes of the title blocks read as blocks: their texts are fields, never view names.
fn title_block_boxes(cx: &Ctx, b: &Builder) -> Vec<[f64; 4]> {
    (0..b.exprs.len())
        .filter(|&i| b.exprs[i].kind == "title_block")
        .filter_map(|i| block_member(b, i).and_then(|h| cx.bbox(h)))
        .collect()
}

/// Two equal letters on one baseline with a short line at half their height (or a dash text)
/// between them: the section name `X—X` (card view_callout, variant section_name_split).
fn split_section_names(cx: &Ctx, b: &mut Builder, titles: &mut Vec<ViewTitle>) -> Result<()> {
    let gap_max = param("view_callout", "split_name_gap")?;
    let dashes = lexicon("view_callout", "separator");
    let fields = title_block_boxes(cx, b);
    let free: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| {
            !b.used(&t.id)
                && view_letter(&t.dec.text)
                && !fields.iter().any(|f| inside(t.center, *f, 0.))
        })
        .cloned()
        .collect();
    for l in &free {
        if b.used(&l.id) {
            continue;
        }
        let pair = free
            .iter()
            .filter(|r| r.id != l.id && !b.used(&r.id) && r.dec.text.trim() == l.dec.text.trim())
            .filter_map(|r| {
                let h = l.height.max(r.height);
                let d = sub(r.center, l.center);
                let along = dot(d, l.dir);
                let gap = along - (l.width + r.width) / 2.;
                let ok = l.height.min(r.height) > 0.8 * h
                    && cross(l.dir, r.dir).abs() < 0.05
                    && cross(l.dir, d).abs() < 0.3 * h
                    && gap > 0.2 * h
                    && gap < gap_max * h;
                ok.then_some((r, along, h))
            })
            .find_map(|(r, along, h)| {
                let between = |m: P| {
                    let s = dot(sub(m, l.center), l.dir);
                    s > l.width / 2. - 0.2 * h
                        && s < along - r.width / 2. + 0.2 * h
                        && cross(l.dir, sub(m, l.center)).abs() < 0.35 * h
                };
                let line = cx
                    .lines
                    .iter()
                    .filter(|(id, (p, q))| {
                        let len = dist(*p, *q);
                        !b.used(id)
                            && cross(unit(sub(*q, *p)), l.dir).abs() < 0.05
                            && len > 0.3 * h
                            && len < along
                            && between(mid(*p, *q))
                    })
                    .map(|(id, _)| (id.clone(), member_line_type(cx.lt(id), "table_line")))
                    .next();
                let text = cx
                    .texts
                    .values()
                    .filter(|t| {
                        !b.used(&t.id)
                            && dashes.iter().any(|d| t.dec.text.trim() == d)
                            && between(t.center)
                    })
                    .map(|t| (t.id.clone(), "text"))
                    .next();
                line.or(text).map(|dash| (r.clone(), dash, h))
            });
        let Some((r, (dash, dash_lt), h)) = pair else {
            continue;
        };
        let label = format!("{}—{}", l.dec.text.trim(), r.dec.text.trim());
        let e = b.expr(
            "view_callout",
            &label,
            "card:view_callout/section_name_split",
            "candidate",
            vec!["view_title".into()],
            vec![
                format!("text:{}", l.id),
                format!("text:{}", r.id),
                "card:view_callout".into(),
                "variant:section_name_split".into(),
                "concept:DC-G2-065".into(),
            ],
            vec![],
        );
        for (id, lt) in [(&l.id, "text"), (&dash, dash_lt), (&r.id, "text")] {
            b.member(
                e,
                id,
                None,
                [0., 1.],
                "view_title",
                lt,
                "candidate",
                vec![],
                vec![],
            );
        }
        titles.push(ViewTitle {
            label,
            center: mid(l.center, r.center),
            height: h,
            kind: "section_view",
            scale: None,
            sources: vec![l.id.clone(), dash, r.id.clone()],
        });
    }
    Ok(())
}

/// The name part and the rotation word of a detail name (`Ⅲ`, `C`, `III 旋转`), with a scale when it
/// is written on the same line.
fn detail_name(
    text: &str,
    rotation: &[String],
) -> Option<(String, Option<String>, Option<String>)> {
    let mut tokens: Vec<&str> = text.split_whitespace().collect();
    let scale = tokens
        .last()
        .filter(|t| tx::is_scale(t))
        .map(|t| tx::scale_text(t));
    if scale.is_some() {
        tokens.pop();
    }
    let joined: String = tokens.concat();
    let (name, rot) = match rotation.iter().find(|w| joined.ends_with(w.as_str())) {
        Some(w) => (
            joined[..joined.len() - w.len()].to_string(),
            Some(w.clone()),
        ),
        None => (joined, None),
    };
    let ok = tx::is_roman(&name) || (name.chars().count() == 1 && view_letter(&name));
    ok.then_some((name, rot, scale))
}

/// Detail names written apart from the existing numeral-over-scale form (card detail_view,
/// variant title_forms): a capital letter or a numeral followed by the word for a rotated view,
/// over a scale written with any colon, or the name and the scale on one line.
fn detail_names(cx: &Ctx, b: &mut Builder, titles: &mut Vec<ViewTitle>) -> Result<()> {
    let gmin = param("detail_view", "title_gap_min")?;
    let gmax = param("detail_view", "title_gap_max")?;
    let dxm = param("detail_view", "title_dx_max")?;
    let rotation = lexicon("detail_view", "rotation_word");
    let fields = title_block_boxes(cx, b);
    let free: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id) && !fields.iter().any(|f| inside(t.center, *f, 0.)))
        .cloned()
        .collect();
    for u in &free {
        if b.used(&u.id) {
            continue;
        }
        let Some((name, rot, inline)) = detail_name(&u.dec.text, &rotation) else {
            continue;
        };
        // a numeral alone over a scale is the older rule's form; a lone letter needs a scale
        let below = if inline.is_none() {
            free.iter()
                .filter(|s| s.id != u.id && !b.used(&s.id) && s.lex == Lex::Scale)
                .filter(|s| {
                    let d = sub(u.center, s.center);
                    let h = u.height.max(s.height);
                    d[1] > gmin * h && d[1] < gmax * h && d[0].abs() < dxm * h
                })
                .min_by(|x, y| dist(x.center, u.center).total_cmp(&dist(y.center, u.center)))
                .cloned()
        } else {
            None
        };
        let scale = match (&inline, &below) {
            (Some(s), _) => s.clone(),
            (None, Some(s)) => tx::scale_text(&s.dec.text),
            _ => continue,
        };
        let label = match &rot {
            Some(w) => format!("{name} {w} {scale}"),
            None => format!("{name} {scale}"),
        };
        let mut ev = vec![
            format!("text:{}", u.id),
            "card:detail_view".to_string(),
            "variant:title_forms".into(),
            "concept:DC-G2-075".into(),
            "card:view_callout".into(),
            "variant:name_with_scale".into(),
            "concept:DC-G2-068".into(),
            "concept:DC-G1-115".into(),
        ];
        if rot.is_some() {
            ev.push("concept:DC-G2-048".into());
        }
        let e = b.expr(
            "view_callout",
            &label,
            "card:detail_view/title_forms",
            "candidate",
            vec!["detail_title".into()],
            ev,
            vec![],
        );
        b.member(
            e,
            &u.id,
            None,
            [0., 1.],
            "view_letter",
            "text",
            "candidate",
            vec![],
            vec![],
        );
        let mut sources = vec![u.id.clone()];
        let mut center = u.center;
        let mut height = u.height;
        if let Some(s) = &below {
            b.member(
                e,
                &s.id,
                None,
                [0., 1.],
                "scale_text",
                "text",
                "candidate",
                vec![],
                vec![],
            );
            let (y0, y1) = (s.center[1], u.center[1]);
            if let Some((lid, _)) = cx.lines.iter().find(|(id, (p, q))| {
                !b.used(id)
                    && (p[1] - q[1]).abs() < 1e-6
                    && p[1] > y0
                    && p[1] < y1
                    && p[0].min(q[0]) < u.center[0]
                    && p[0].max(q[0]) > u.center[0]
            }) {
                let lid = lid.clone();
                b.member(
                    e,
                    &lid,
                    None,
                    [0., 1.],
                    "view_title",
                    member_line_type(cx.lt(&lid), "table_line"),
                    "candidate",
                    vec![],
                    vec![],
                );
            }
            sources.push(s.id.clone());
            center = mid(u.center, s.center);
            height = u.height.max(s.height);
        }
        titles.push(ViewTitle {
            label,
            center,
            height,
            kind: "detail_view",
            scale: Some(scale),
            sources,
        });
    }
    Ok(())
}

/// A section name standing on the shoulder of a leader names the local section the leader points
/// at, not the view below it (card view_callout, variant section_name_on_leader); the shoulder,
/// the leader and a note written under the shoulder belong to the name.
fn names_on_leaders(cx: &Ctx, b: &mut Builder, titles: &mut Vec<ViewTitle>) -> Result<()> {
    let tol = 1e-3 * cx.scale.max(1.);
    let mut keep = Vec::new();
    for t in std::mem::take(titles) {
        let first = t.sources.first().and_then(|s| cx.texts.get(s));
        let Some(first) = first.filter(|_| t.kind == "section_view") else {
            keep.push(t);
            continue;
        };
        // the whole name as one text (a split name spans its letters)
        let mut name = first.clone();
        name.center = t.center;
        name.width = t
            .sources
            .iter()
            .filter_map(|s| cx.texts.get(s))
            .map(|x| dist(x.center, t.center) * 2. + x.width)
            .fold(first.width, f64::max);
        let shoulder = free_segments(cx, b).into_iter().find(|(id, pts)| {
            let l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
            on_shoulder(&name, pts)
                && (cx.polys.contains_key(id) || l <= name.width + 4. * name.height)
        });
        let Some((sid, pts)) = shoulder else {
            keep.push(t);
            continue;
        };
        let (s0, s1) = (pts[0], pts[pts.len() - 1]);
        let u = unit(sub(s1, s0));
        let leader = free_segments(cx, b).into_iter().find(|(id, lp)| {
            let n = lp.len();
            id != &sid
                && [lp[0], lp[n - 1]]
                    .iter()
                    .any(|e| dist(*e, s0) < tol || dist(*e, s1) < tol)
                && cross(unit(sub(lp[n - 1], lp[0])), u).abs() > 0.17
        });
        let Some((lid, _)) = leader else {
            keep.push(t);
            continue;
        };
        let Some(e) = b.exprs.iter().position(|x| {
            x.kind == "view_callout"
                && b.members
                    .iter()
                    .any(|m| m.expression_id == x.id && m.source_handle == first.id)
        }) else {
            keep.push(t);
            continue;
        };
        b.exprs[e].qualifiers.retain(|q| q != "view_title");
        b.exprs[e].qualifiers.push("local_section_name".into());
        push_ev(b, e, "variant:section_name_on_leader");
        for id in [&sid, &lid] {
            b.member(
                e,
                id,
                None,
                [0., 1.],
                "leader_line",
                member_line_type(cx.lt(id), "leader_line"),
                "candidate",
                vec![],
                vec![],
            );
        }
        // a note written under the shoulder (e.g. how many such sections there are)
        let notes: Vec<String> = cx
            .texts
            .values()
            .filter(|x| !b.used(&x.id) && !t.sources.contains(&x.id))
            .filter(|x| {
                let below = cross(u, sub(x.center, s0));
                let s = dot(sub(x.center, s0), u);
                below < -0.2 * x.height && below > -1.6 * x.height && s > 0. && s < dist(s0, s1)
            })
            .map(|x| x.id.clone())
            .collect();
        for n in notes {
            b.member(
                e,
                &n,
                None,
                [0., 1.],
                "note_text",
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
    }
    *titles = keep;
    Ok(())
}

/// Letters of the section names read so far (view titles and local section names).
fn section_name_letters(b: &Builder) -> BTreeSet<String> {
    b.exprs
        .iter()
        .filter(|e| {
            e.kind == "view_callout"
                && matches!(
                    e.rule.as_str(),
                    "card:view_callout/section_name" | "card:view_callout/section_name_split"
                )
        })
        .filter_map(|e| section_letters(&e.label))
        .collect()
}

/// Cutting strokes drawn without arrows (card view_callout, variant strokes_without_arrows): two
/// short collinear strokes, each with the same letter at its outer end, the letter naming a
/// section `X—X` (GB/T 4458.6: arrows may be omitted when the section is in projection position).
fn strokes_without_arrows(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("view_callout")?;
    let smin = representation::param(c, "unarrowed_stroke_min")?;
    let smax = representation::param(c, "stroke_max")?;
    let near = representation::param(c, "stroke_letter_reach")?;
    let col = representation::param(c, "stroke_collinear")?;
    let letters = section_name_letters(b);
    let strokes: Vec<(String, P, P)> = cx
        .lines
        .iter()
        .filter(|(id, (p, q))| {
            !b.used(id)
                && cx
                    .style
                    .get(*id)
                    .is_some_and(|s| s.pattern == Pattern::Continuous)
                && (smin * cx.scale..smax * cx.scale).contains(&dist(*p, *q))
        })
        .map(|(id, (p, q))| (id.clone(), *p, *q))
        .collect();
    // each stroke with its lettered end: (stroke, outer end, inner end, letter text)
    let lettered: Vec<(String, P, P, String, String, f64)> = strokes
        .iter()
        .filter_map(|(id, p, q)| {
            cx.texts
                .values()
                .filter(|t| !b.used(&t.id) && letters.contains(t.dec.text.trim()))
                .filter_map(|t| {
                    let (o, i) = if dist(t.center, *p) <= dist(t.center, *q) {
                        (*p, *q)
                    } else {
                        (*q, *p)
                    };
                    (dist(t.center, o) < near * t.height).then(|| {
                        (
                            id.clone(),
                            o,
                            i,
                            t.id.clone(),
                            t.dec.text.trim().to_string(),
                            t.height,
                        )
                    })
                })
                .min_by(|x, y| {
                    dist(cx.texts[&x.3].center, x.1).total_cmp(&dist(cx.texts[&y.3].center, y.1))
                })
        })
        .collect();
    let mut used = BTreeSet::new();
    for (i, a) in lettered.iter().enumerate() {
        if used.contains(&a.0) {
            continue;
        }
        let u = unit(sub(a.1, a.2));
        let partner = lettered.iter().skip(i + 1).find(|z| {
            !used.contains(&z.0)
                && z.4 == a.4
                && z.3 != a.3
                && cross(u, sub(z.1, a.1)).abs() < col * a.5
                && cross(u, sub(z.2, a.1)).abs() < col * a.5
                // the two inner ends face each other between the lettered outer ends
                && dot(sub(z.2, a.2), u) < 0.
                && dot(sub(z.1, z.2), u) < 0.
        });
        let Some(z) = partner else { continue };
        used.insert(a.0.clone());
        used.insert(z.0.clone());
        let e = b.expr(
            "view_callout",
            &a.4,
            "card:view_callout/strokes_without_arrows",
            "candidate",
            vec!["cutting_plane".into()],
            vec![
                format!("text:{}", a.3),
                format!("text:{}", z.3),
                "card:view_callout".into(),
                "variant:strokes_without_arrows".into(),
                "concept:DC-G2-043".into(),
                "concept:DC-G1-007".into(),
            ],
            vec![],
        );
        for s in [a, z] {
            b.member(
                e,
                &s.0,
                None,
                [0., 1.],
                "cutting_stroke",
                cx.lt(&s.0),
                "candidate",
                vec![],
                vec![],
            );
            b.member(
                e,
                &s.3,
                None,
                [0., 1.],
                "view_letter",
                "text",
                "candidate",
                vec![],
                vec![],
            );
        }
    }
    Ok(())
}

/// The arrow of a callout: (tail, tip) of its shaft, from a loose shaft and head or from an arrow
/// block.
fn callout_arrow(cx: &Ctx, b: &Builder, e: usize) -> Option<(P, P)> {
    let id = &b.exprs[e].id;
    let ms: Vec<&MdExpressionMember> = b
        .members
        .iter()
        .filter(|m| &m.expression_id == id)
        .collect();
    let shaft = ms.iter().find(|m| m.member_role == "view_arrow")?;
    if let Some(s) = cx.shapes.get(&shaft.source_handle) {
        return block_shaft(s);
    }
    let (p, q) = cx.lines.get(&shaft.source_handle).copied()?;
    let head = ms
        .iter()
        .find(|m| m.member_role == "arrowhead")
        .and_then(|m| cx.arrow(&m.source_handle))?;
    Some(if dist(p, head.tip) > dist(q, head.tip) {
        (p, head.tip)
    } else {
        (q, head.tip)
    })
}

/// The whole cutting path of a cutting-plane symbol (card view_callout, variant cutting_path):
/// the stroke at the tail of an arrow drawn as a block or apart from its letter, and from a
/// stroke along the cutting plane the strokes that continue it: two strokes meeting at an angle
/// at a turn (with the same letter), and the end stroke with its perpendicular arrow.
pub(super) fn cutting_paths(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("view_callout")?;
    let smin = representation::param(c, "stroke_min")? * cx.scale;
    let smax = representation::param(c, "stroke_max")? * cx.scale;
    let reach = representation::param(c, "stroke_reach")? * cx.scale;
    let letter_reach = representation::param(c, "letter_reach")?;
    let cos_max = representation::param(c, "path_angle_deg")?
        .to_radians()
        .cos();
    let sheets = sheet_frames(cx, b);
    let tol = 1e-3 * cx.scale.max(1.);
    let free_stroke = |b: &Builder, id: &str, lt: &str| {
        !b.used(id)
            && cx.lt(id) == lt
            && cx
                .style
                .get(id)
                .is_some_and(|s| s.pattern == Pattern::Continuous)
            && cx
                .lines
                .get(id)
                .is_some_and(|(p, q)| (smin..smax).contains(&dist(*p, *q)))
    };
    let callouts: Vec<usize> = (0..b.exprs.len())
        .filter(|&i| {
            let e = &b.exprs[i];
            e.kind == "view_callout"
                && e.qualifiers.iter().any(|q| q == "view_direction")
                && view_letter(&e.label)
        })
        .collect();
    // the stroke at the tail of an arrow drawn as a block or apart from its letter (the loose
    // arrow with its letter is covered by `symbols::cutting_strokes`), for every callout first
    for &e in &callouts {
        let id = b.exprs[e].id.clone();
        let has_stroke = b
            .members
            .iter()
            .any(|m| m.expression_id == id && m.member_role == "cutting_stroke");
        let apart = matches!(
            b.exprs[e].rule.as_str(),
            "card:view_callout/arrow_block" | "card:view_callout/arrow_and_letter_apart"
        );
        if apart
            && !has_stroke
            && let Some((tail, tip)) = callout_arrow(cx, b, e)
        {
            let u = unit(sub(tip, tail));
            let best = cx
                .lines
                .iter()
                .filter(|(sid, (m, n))| {
                    let l = dist(*m, *n);
                    !b.used(sid)
                        && (smin..smax).contains(&l)
                        && dot(unit(sub(*n, *m)), u).abs() < 0.1
                        && seg_dist(tail, *m, *n) < reach
                })
                .min_by(|x, y| {
                    seg_dist(tail, x.1.0, x.1.1).total_cmp(&seg_dist(tail, y.1.0, y.1.1))
                })
                .map(|(sid, _)| sid.clone());
            if let Some(sid) = best {
                b.member(
                    e,
                    &sid,
                    None,
                    [0., 1.],
                    "cutting_stroke",
                    cx.lt(&sid),
                    "candidate",
                    vec![],
                    vec![],
                );
                push_ev(b, e, "card:view_callout");
                push_ev(b, e, "variant:cutting_plane_stroke");
                push_ev(b, e, "concept:DC-G2-043");
                push_ev(b, e, "concept:DC-G1-007");
            }
        }
    }
    for e in callouts {
        let id = b.exprs[e].id.clone();
        let letter = b.exprs[e].label.trim().to_string();
        // follow the path from the stroke: away from the arrow, along the stroke
        let Some(start) = b
            .members
            .iter()
            .find(|m| m.expression_id == id && m.member_role == "cutting_stroke")
            .map(|m| m.source_handle.clone())
        else {
            continue;
        };
        let Some(&(p0, q0)) = cx.lines.get(&start) else {
            continue;
        };
        let Some((tail, _)) = callout_arrow(cx, b, e) else {
            continue;
        };
        let lt = cx.lt(&start);
        let (mut at, mut u) = if dist(p0, tail) <= dist(q0, tail) {
            (q0, unit(sub(q0, p0)))
        } else {
            (p0, unit(sub(p0, q0)))
        };
        let sheet = sheet_at(&sheets, at);
        let mut added = false;
        for _ in 0..4 {
            // the next stroke ahead along the cutting plane
            let next = cx
                .lines
                .iter()
                .filter(|(sid, _)| free_stroke(b, sid, lt))
                .filter_map(|(sid, (p, q))| {
                    let (n, f) = if dist(*p, at) <= dist(*q, at) {
                        (*p, *q)
                    } else {
                        (*q, *p)
                    };
                    let ahead = dist(n, at) > tol
                        && dot(unit(sub(n, at)), u) > cos_max
                        && dot(unit(sub(f, n)), u) > cos_max;
                    (ahead && sheet_at(&sheets, n) == sheet).then(|| (sid.clone(), n, f))
                })
                .min_by(|x, y| dist(x.1, at).total_cmp(&dist(y.1, at)));
            let Some((sid, _, far)) = next else { break };
            let letter_near = |b: &Builder, p: P| {
                cx.texts
                    .values()
                    .filter(|t| {
                        !b.used(&t.id)
                            && t.dec.text.trim() == letter
                            && dist(t.center, p) < letter_reach * t.height
                    })
                    .min_by(|x, y| dist(x.center, p).total_cmp(&dist(y.center, p)))
                    .map(|t| t.id.clone())
            };
            // a turn: another stroke leaves the far end at an angle
            let turn = cx
                .lines
                .iter()
                .filter(|(tid, _)| *tid != &sid && free_stroke(b, tid, lt))
                .find_map(|(tid, (p, q))| {
                    let (n, f) = if dist(*p, far) <= dist(*q, far) {
                        (*p, *q)
                    } else {
                        (*q, *p)
                    };
                    (dist(n, far) < tol && dot(unit(sub(f, n)), unit(sub(far, at))) < cos_max)
                        .then(|| (tid.clone(), f))
                });
            if let Some((tid, f)) = turn {
                let turn_letter = letter_near(b, far);
                for (id, role, lt) in [
                    (&sid, "cutting_stroke", cx.lt(&sid)),
                    (&tid, "cutting_stroke", cx.lt(&tid)),
                ] {
                    b.member(
                        e,
                        id,
                        None,
                        [0., 1.],
                        role,
                        lt,
                        "candidate",
                        vec![],
                        vec!["cutting_path_turn".into()],
                    );
                }
                if let Some(t) = turn_letter {
                    b.member(
                        e,
                        &t,
                        None,
                        [0., 1.],
                        "view_letter",
                        "text",
                        "candidate",
                        vec![],
                        vec!["cutting_path_turn".into()],
                    );
                }
                added = true;
                u = unit(sub(f, far));
                at = f;
                continue;
            }
            // the end stroke: an arrow leaves its outer end at a right angle, or the letter is
            // written there; a stroke with neither is not taken
            let v = unit(sub(far, at));
            let arrow = cx
                .arrows
                .iter()
                .filter(|(aid, _)| !b.used(aid))
                .find_map(|(aid, a)| {
                    cx.lines
                        .iter()
                        .filter(|(lid, (p, q))| {
                            !b.used(lid)
                                && cross(unit(sub(*q, *p)), a.dir).abs() < 0.03
                                && dot(a.dir, v).abs() < 0.1
                        })
                        .find_map(|(lid, (p, q))| {
                            let (n, f) = if dist(*p, far) <= dist(*q, far) {
                                (*p, *q)
                            } else {
                                (*q, *p)
                            };
                            (dist(n, far) < reach && continues(a, f, n, 0.15 * a.len))
                                .then(|| (aid.clone(), lid.clone()))
                        })
                });
            let end_letter = letter_near(b, far);
            if arrow.is_none() && end_letter.is_none() {
                break;
            }
            b.member(
                e,
                &sid,
                None,
                [0., 1.],
                "cutting_stroke",
                cx.lt(&sid),
                "candidate",
                vec![],
                vec!["cutting_path_end".into()],
            );
            added = true;
            if let Some((aid, lid)) = arrow {
                b.member(
                    e,
                    &lid,
                    None,
                    [0., 1.],
                    "view_arrow",
                    member_line_type(cx.lt(&lid), "view_arrow"),
                    "candidate",
                    vec![],
                    vec!["cutting_path_end".into()],
                );
                b.member(
                    e,
                    &aid,
                    None,
                    [0., 1.],
                    "arrowhead",
                    "fill",
                    "candidate",
                    vec![],
                    vec!["cutting_path_end".into()],
                );
            }
            if let Some(t) = end_letter {
                b.member(
                    e,
                    &t,
                    None,
                    [0., 1.],
                    "view_letter",
                    "text",
                    "candidate",
                    vec![],
                    vec!["cutting_path_end".into()],
                );
            }
            break;
        }
        if added {
            push_ev(b, e, "variant:cutting_path");
        }
    }
    strokes_without_arrows(cx, b)
}

// ---------------------------------------------------------------- detail view boundary

/// The boundary of an enlarged detail view drawn as a thin circle, or as a thin arc whose open
/// side is closed by a wavy break line, around the whole detail geometry with the detail name
/// inside it or just above it (card detail_view, variants boundary_arc and boundary_circle).
pub(super) fn detail_boundaries(cx: &Ctx, b: &mut Builder, titles: &[ViewTitle]) -> Result<()> {
    let rmin = param("detail_view", "boundary_min_radius")? * cx.scale;
    let on_break = param("detail_view", "break_end_tolerance")? * cx.scale;
    let enclosed = param("detail_view", "enclosed_min")?;
    let above = param("detail_view", "title_above_max")?;
    let pieces = param("view_arrangement", "min_pieces")? as usize;
    let names: Vec<&ViewTitle> = titles.iter().filter(|t| t.kind == "detail_view").collect();
    if names.is_empty() {
        return Ok(());
    }
    let breaks: Vec<Vec<P>> = b
        .members
        .iter()
        .filter(|m| m.member_role == "break_curve")
        .filter_map(|m| {
            cx.polys
                .get(&m.source_handle)
                .map(|x| x.0.clone())
                .or_else(|| cx.lines.get(&m.source_handle).map(|(p, q)| vec![*p, *q]))
        })
        .collect();
    let geometry: Vec<Vec<P>> = b
        .exprs
        .iter()
        .filter(|e| GEOMETRY.contains(&e.kind.as_str()))
        .flat_map(|e| b.members.iter().filter(move |m| m.expression_id == e.id))
        .map(|m| member_points(cx, m))
        .filter(|p| !p.is_empty())
        .collect();
    let candidates: Vec<(String, P, f64, bool, P, P)> = cx
        .rec
        .keys()
        .filter(|h| matches!(cx.kind(h), "ARC" | "CIRCLE") && !b.used(h) && !cx.structural_style(h))
        .filter_map(|h| {
            let (c, r, s0, sw, circle) = cx.arc(h)?;
            let end = |a: f64| add(c, mul([a.cos(), a.sin()], r));
            (r >= rmin).then(|| (h.clone(), c, r, circle, end(s0), end(s0 + sw)))
        })
        .collect();
    for (h, c, r, circle, e0, e1) in candidates {
        // an arc's open side is closed by a recognised break line through both its ends
        let closed = circle
            || [e0, e1].iter().all(|e| {
                breaks
                    .iter()
                    .any(|pts| pts.windows(2).any(|w| seg_dist(*e, w[0], w[1]) < on_break))
            });
        if !closed {
            continue;
        }
        // the geometry meeting the disk lies inside it (an indicator circle is crossed by the view)
        let meeting: Vec<&Vec<P>> = geometry
            .iter()
            .filter(|pts| {
                pts.iter().any(|p| dist(*p, c) < r - on_break)
                    || pts
                        .windows(2)
                        .any(|w| seg_dist(c, w[0], w[1]) < r - on_break)
            })
            .collect();
        let within = meeting
            .iter()
            .filter(|pts| pts.iter().all(|p| dist(*p, c) <= r + on_break))
            .count();
        if meeting.len() < pieces || (within as f64) < enclosed * meeting.len() as f64 {
            continue;
        }
        // the detail name inside the boundary, or just above it
        let named = names.iter().any(|t| {
            let d = dist(t.center, c);
            (d < r && t.center[1] > c[1])
                || ((t.center[0] - c[0]).abs() < r
                    && t.center[1] > c[1] + r
                    && t.center[1] - (c[1] + r) < above * t.height)
        });
        if !named {
            continue;
        }
        let variant = if circle {
            "boundary_circle"
        } else {
            "boundary_arc"
        };
        let e = b.expr(
            "break_line",
            "局部放大图边界",
            &format!("card:detail_view/{variant}"),
            "candidate",
            vec![],
            vec![
                format!("source:{h}"),
                "card:detail_view".into(),
                format!("variant:{variant}"),
                "concept:DC-G2-075".into(),
            ],
            vec![],
        );
        let lt = cx.lt(&h);
        let lt = if lt == "continuous" {
            "thin_continuous"
        } else {
            lt
        };
        b.member(
            e,
            &h,
            None,
            [0., 1.],
            "break_curve",
            lt,
            "candidate",
            vec![],
            vec![],
        );
    }
    Ok(())
}
