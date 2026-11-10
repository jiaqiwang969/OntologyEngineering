//! Whether a sheet is an assembly drawing, and which part an item number names (card
//! `assembly_drawing`; concepts DC-G8-013 and DC-G2-009).
//!
//! An assembly drawing is told by what only it carries (GB/T 10609.2-2009 3.1, 3.2; GB/T
//! 4458.2-2003 3.1, 3.3, 4.4; GB/T 4457.5-2013 4.7; GB/T 17453-2005 5; GB/T 4458.6-2002 4.4;
//! GB/T 4458.5-2003 5.1):
//!
//! * E1 a parts list on the sheet;
//! * E2 item numbers whose numbers agree with the rows of that list;
//! * E3 two section regions of one view that share a boundary line and are hatched apart
//!   (direction or spacing differs): two adjacent parts;
//! * E4 a standard part drawn unsectioned (a library block named by an item number);
//! * E5 a fit designation in a dimension (a hole class over a shaft class, `H7/g6`).
//!
//! A parts list with item numbers that agree with it decides; otherwise two of the evidence items
//! make the sheet a candidate. The item numbers also say which part each region is: the section
//! region holding the dot an item number's leader ends in is that item, and in one view every
//! region with the same direction and spacing is the same part (4.7: one part has one hatch).
use super::*;
use section_hatching::{Family, Hatch, direction_difference};

const CARD: &str = "assembly_drawing";

fn push(ev: &mut Vec<String>, x: &str) {
    if !ev.iter().any(|y| y == x) {
        ev.push(x.to_string());
    }
}

/// The sheet a view id belongs to (`sheet-2` of `view:sheet-2:A-A`, empty on a one-sheet file).
fn sheet_of(view: Option<&String>) -> String {
    view.map_or("", |v| view_types::sheet_key(v)).to_string()
}

fn sheet_view(key: &str) -> String {
    if key.is_empty() {
        "view:sheet".into()
    } else {
        format!("view:{key}")
    }
}

/// Two hatches of one part: the same number of line families, each matching one of the other's
/// in direction and spacing (GB/T 4457.5-2013 4.1, 4.7).
fn same_signature(a: &[Family], b: &[Family], dir_min: f64, spacing_min: f64) -> bool {
    let close = |f: &Family, g: &Family| {
        direction_difference(f.angle, g.angle) < dir_min
            && f.spacing.max(g.spacing) < spacing_min * f.spacing.min(g.spacing)
    };
    !a.is_empty() && a.len() == b.len() && a.iter().all(|f| b.iter().any(|g| close(f, g)))
}

/// A fit designation (GB/T 4458.5-2003 5.1): a hole tolerance class (capital letters and the
/// grade) and a shaft class (small letters and the grade) separated by a slash, after the size.
pub(super) fn fit_designation(text: &str) -> bool {
    let c: Vec<char> = text.chars().collect();
    let count = |from: usize, step: isize, f: fn(&char) -> bool| {
        let mut n = 0;
        let mut i = from as isize;
        while i >= 0 && (i as usize) < c.len() && f(&c[i as usize]) {
            n += 1;
            i += step;
        }
        n
    };
    (0..c.len()).filter(|&i| c[i] == '/').any(|i| {
        if i == 0 {
            return false;
        }
        let d1 = count(i - 1, -1, char::is_ascii_digit);
        let l1 = if i > d1 {
            count(i - 1 - d1, -1, char::is_ascii_uppercase)
        } else {
            0
        };
        let l2 = count(i + 1, 1, char::is_ascii_lowercase);
        let d2 = count(i + 1 + l2, 1, char::is_ascii_digit);
        (1..=2).contains(&d1) && (1..=2).contains(&l1) && (1..=2).contains(&l2) && (1..=2).contains(&d2)
    })
}

/// An item number: expression, number, view and the centre of its dot (none for an arrow).
struct Item {
    expr: usize,
    number: String,
    view: Option<String>,
    dot: Option<P>,
}

fn items(cx: &Ctx, b: &Builder) -> Vec<Item> {
    b.exprs
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind == "item_number")
        .map(|(i, e)| {
            let dot = b
                .members
                .iter()
                .find(|m| {
                    m.expression_id == e.id
                        && m.member_role == "arrowhead"
                        && m.evidence_ids.iter().any(|x| x == "dot_terminator")
                })
                .and_then(|m| member_points(cx, m).first().copied());
            Item {
                expr: i,
                number: e.label.trim().to_string(),
                view: e.view_id.clone(),
                dot,
            }
        })
        .collect()
}

/// Which region each item number's dot lies in, and the parts that follow from it.
fn parts(b: &mut Builder, hs: &[Hatch], its: &[Item], dir_min: f64, spacing_min: f64) {
    let card = format!("card:{CARD}");
    // the region holding the dot: the smallest native hatch region containing it
    let mut dotted: Vec<(usize, usize)> = Vec::new(); // (hatch index, item index)
    for (k, it) in its.iter().enumerate() {
        let Some(p) = it.dot else { continue };
        let hit = hs
            .iter()
            .enumerate()
            .filter(|(_, h)| h.view == it.view && !h.boundary.is_empty())
            .filter(|(_, h)| representation::inside_segments(p, &h.boundary))
            .min_by(|x, y| {
                let size = |h: &Hatch| {
                    let pts: Vec<P> = h.boundary.iter().flat_map(|s| [s.0, s.1]).collect();
                    bbox_of(&pts).map_or(f64::INFINITY, area)
                };
                size(x.1).total_cmp(&size(y.1))
            })
            .map(|(j, _)| j);
        if let Some(j) = hit {
            dotted.push((j, k));
        }
    }
    for &(j, k) in &dotted {
        let hid = b.exprs[hs[j].expr].id.clone();
        let iid = b.exprs[its[k].expr].id.clone();
        let ev = &mut b.exprs[its[k].expr].evidence_ids;
        push(ev, &format!("names_region:{hid}"));
        push(ev, &card);
        push(ev, "variant:part_by_item_dot");
        let ev = &mut b.exprs[hs[j].expr].evidence_ids;
        push(ev, &format!("item:{}", its[k].number));
        push(ev, &format!("item_number:{iid}"));
        push(ev, &card);
        push(ev, "variant:part_by_item_dot");
    }
    // a standard part named by an item number: the item names it in turn
    let named: Vec<(usize, String)> = b
        .exprs
        .iter()
        .filter(|e| e.kind == "standard_part")
        .flat_map(|e| {
            e.evidence_ids
                .iter()
                .filter_map(|x| x.strip_prefix("named_by_item:"))
                .map(|x| (x.to_string(), e.id.clone()))
                .collect::<Vec<_>>()
        })
        .filter_map(|(item, part)| {
            b.exprs
                .iter()
                .position(|e| e.id == item)
                .map(|i| (i, part))
        })
        .collect();
    for (i, part) in named {
        let ev = &mut b.exprs[i].evidence_ids;
        push(ev, &format!("names_part:{part}"));
        push(ev, &card);
        push(ev, "variant:part_by_item_dot");
    }
    // in one view, regions hatched like a dotted region are the same part, when the hatch is
    // named by one item only
    for (j, h) in hs.iter().enumerate() {
        if dotted.iter().any(|d| d.0 == j) {
            continue;
        }
        let same: Vec<&(usize, usize)> = dotted
            .iter()
            .filter(|(d, _)| {
                hs[*d].view == h.view
                    && same_signature(&hs[*d].families, &h.families, dir_min, spacing_min)
            })
            .collect();
        let numbers: BTreeSet<&str> = same.iter().map(|(_, k)| its[*k].number.as_str()).collect();
        let ([first, ..], 1) = (same.as_slice(), numbers.len()) else {
            continue;
        };
        let (d, k) = **first;
        let other = b.exprs[hs[d].expr].id.clone();
        let ev = &mut b.exprs[h.expr].evidence_ids;
        push(ev, &format!("item:{}", its[k].number));
        push(ev, &format!("same_part_as:{other}"));
        push(ev, &card);
        push(ev, "variant:part_by_signature");
    }
}

/// The parts list of a sheet with the numbers its rows give and how many item numbers agree.
struct List {
    expr: usize,
    rows: usize,
}

/// State whether each sheet is an assembly drawing, and the part each item number names.
pub(super) fn state(cx: &Ctx, b: &mut Builder, regions: &mut [Region]) -> Result<()> {
    let c = representation::card_ref(CARD)?;
    let p = |n: &str| representation::param(c, n);
    let dir_min = p("direction_difference_min")?;
    let spacing_min = p("spacing_ratio_min")?;
    let rows_min = p("rows_named_min")?;
    let items_min = p("item_numbers_min")? as usize;
    let evidence_min = p("evidence_min")? as usize;
    let card = format!("card:{CARD}");
    let hs = section_hatching::hatches(cx, b)?;
    let pairs = section_hatching::neighbours(cx, &hs)?;
    let its = items(cx, b);
    parts(b, &hs, &its, dir_min, spacing_min);
    // E3: adjacent regions hatched apart are two parts (GB/T 4457.5-2013 4.7)
    let mut apart: Vec<(String, String, String)> = Vec::new(); // (sheet, a, b)
    for (i, j) in pairs {
        let (x, y) = (&hs[i], &hs[j]);
        if x.solid || y.solid || x.families.is_empty() || y.families.is_empty() {
            continue;
        }
        if same_signature(&x.families, &y.families, dir_min, spacing_min) {
            continue;
        }
        let (a, z) = (b.exprs[x.expr].id.clone(), b.exprs[y.expr].id.clone());
        for (own, other) in [(x.expr, &z), (y.expr, &a)] {
            let ev = &mut b.exprs[own].evidence_ids;
            push(ev, &format!("different_part_from:{other}"));
            push(ev, &card);
            push(ev, "variant:adjacent_parts_hatched_apart");
            push(ev, "concept:DC-G2-009");
        }
        apart.push((sheet_of(x.view.as_ref()), a, z));
    }
    // the evidence items of each sheet
    let mut sheets: BTreeSet<String> = BTreeSet::new();
    for e in &b.exprs {
        sheets.insert(sheet_of(e.view_id.as_ref()));
    }
    for key in sheets {
        let on = |e: &MdEngineeringExpression| sheet_of(e.view_id.as_ref()) == key;
        let lists: Vec<List> = b
            .exprs
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == "parts_list" && on(e))
            .map(|(i, e)| List {
                expr: i,
                rows: e.evidence_ids.iter().filter(|x| x.starts_with("row:")).count(),
            })
            .collect();
        let mine: Vec<&Item> = its.iter().filter(|it| sheet_of(it.view.as_ref()) == key).collect();
        // E2: item numbers that name rows of the list, and none that names no row
        let agree = lists.first().and_then(|l| {
            let lid = &b.exprs[l.expr].id;
            let named: BTreeSet<&str> = mine
                .iter()
                .filter(|it| {
                    b.exprs[it.expr]
                        .evidence_ids
                        .iter()
                        .any(|x| x == &format!("names_row_of:{lid}"))
                })
                .map(|it| it.number.as_str())
                .collect();
            let stray = mine.iter().any(|it| {
                !b.exprs[it.expr]
                    .evidence_ids
                    .iter()
                    .any(|x| x.starts_with("parts_list_row:"))
            });
            (!stray
                && named.len() >= items_min
                && named.len() as f64 >= rows_min * l.rows as f64)
                .then_some((l.expr, named.len()))
        });
        let parts_apart: Vec<&(String, String, String)> =
            apart.iter().filter(|x| x.0 == key).collect();
        let standard: Vec<String> = b
            .exprs
            .iter()
            .filter(|e| e.kind == "standard_part" && on(e))
            .map(|e| e.id.clone())
            .collect();
        let fits: Vec<String> = b
            .exprs
            .iter()
            .filter(|e| e.kind.starts_with("dimension_") && on(e) && fit_designation(&e.label))
            .map(|e| e.id.clone())
            .collect();
        let mut found: Vec<String> = Vec::new();
        if let Some(l) = lists.first() {
            found.push(format!("parts_list:{}", b.exprs[l.expr].id));
        }
        if let Some((_, n)) = agree {
            found.push(format!("item_numbers:{n}"));
        } else if mine.len() >= items_min {
            // numbers whose list is on another sheet, or that do not agree with the list here
            let what = if lists.is_empty() {
                "item_numbers_without_list"
            } else {
                "item_numbers_disagreeing"
            };
            found.push(format!("{what}:{}", mine.len()));
        }
        if let Some((_, a, z)) = parts_apart.first() {
            found.push(format!("adjacent_parts:{a}:{z}"));
        }
        if let Some(s) = standard.first() {
            found.push(format!("standard_part:{s}"));
        }
        if let Some(f) = fits.first() {
            found.push(format!("fit_designation:{f}"));
        }
        let Some(r) = regions.iter_mut().find(|r| r.id == sheet_view(&key)) else {
            continue;
        };
        if agree.is_some() {
            push(&mut r.evidence, "drawing_type:assembly");
            for x in &found {
                push(&mut r.evidence, x);
            }
            push(&mut r.evidence, &card);
            push(&mut r.evidence, "variant:list_on_the_sheet");
            // part-drawing content on an assembly: parts joined and machined together
            let content = b.exprs.iter().any(|e| {
                on(e)
                    && (e.kind == "surface_roughness"
                        || e.kind == "geometric_tolerance"
                        || e.kind.starts_with("gps_"))
            });
            if content {
                push(&mut r.evidence, "variant:machined_after_joining");
            }
            if !r.label.ends_with(" · 装配图") {
                r.label = format!("{} · 装配图", r.label);
            }
        } else if found.len() >= evidence_min {
            push(&mut r.evidence, "drawing_type_candidate:assembly");
            for x in &found {
                push(&mut r.evidence, x);
            }
            push(&mut r.evidence, &card);
            if lists.is_empty() {
                push(&mut r.evidence, "variant:list_on_a_separate_sheet");
            }
        }
    }
    Ok(())
}

/// Unknowns of a sheet that may be an assembly drawing.
pub(super) fn unknowns(r: &Region) -> Vec<String> {
    if r.evidence.iter().any(|x| x == "drawing_type_candidate:assembly") {
        vec!["Assembly drawing is a candidate: no parts list on the sheet agrees with item numbers; it rests on the other evidence items named".into()]
    } else {
        vec![]
    }
}
