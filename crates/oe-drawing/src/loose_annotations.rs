//! Annotations drawn with loose lines, solids and texts (older AutoCAD drawings), read from cards.
//!
//! * `gdt_frame_content` (variant `line_outline_dividers`) — a tolerance frame whose outline is drawn
//!   with separate lines, possibly in collinear pieces that overlap, touch or are offset along the
//!   side, divided into cells by full-height lines; the value cell read in reading order (a zone
//!   symbol written as its own text joins the value); a leader drawn in overlapping pieces or with a
//!   small miss at a join.
//! * `datum_feature_symbol` (variant `numbered_letter`) — a datum letter with a serial number (A1),
//!   accepted only when a tolerance frame names it in a datum cell.
//!
//! The code implements only the geometry (merging collinear pieces, cells, joins); the tolerances
//! are drawing-domain ratios read from the cards, and every expression names its card and variant.
use super::*;

fn push_ev(e: &mut MdEngineeringExpression, x: String) {
    if !e.evidence_ids.contains(&x) {
        e.evidence_ids.push(x);
    }
}

// ---------------------------------------------------------------- collinear runs of line pieces

/// One side drawn with collinear axis-parallel pieces: the pieces, the common coordinate across the
/// side and its extent along it.
#[derive(Clone, Debug)]
struct Run {
    ids: Vec<String>,
    at: f64,
    lo: f64,
    hi: f64,
}

/// Axis-parallel thin LINE pieces merged into runs: pieces whose cross coordinates agree within `off`
/// and that overlap, touch or leave a gap of at most `gap` along the side.
fn runs(cx: &Ctx, b: &Builder, vertical: bool, off: f64, gap: f64, max_len: f64) -> Vec<Run> {
    let mut pieces: Vec<(String, f64, f64, f64)> = Vec::new();
    for (id, (p, q)) in &cx.lines {
        let l = dist(*p, *q);
        if b.used(id) || l < 1e-6 || l > max_len || !cx.annotation_style(id) {
            continue;
        }
        let (across, along) = if vertical {
            ((p[0], q[0]), (p[1], q[1]))
        } else {
            ((p[1], q[1]), (p[0], q[0]))
        };
        if (across.0 - across.1).abs() > off.min(0.02 * l) {
            continue;
        }
        pieces.push((
            id.clone(),
            0.5 * (across.0 + across.1),
            along.0.min(along.1),
            along.0.max(along.1),
        ));
    }
    pieces.sort_by(|a, c| a.1.total_cmp(&c.1).then(a.2.total_cmp(&c.2)));
    let mut out: Vec<Run> = Vec::new();
    let mut used = vec![false; pieces.len()];
    for i in 0..pieces.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut run = Run {
            ids: vec![pieces[i].0.clone()],
            at: pieces[i].1,
            lo: pieces[i].2,
            hi: pieces[i].3,
        };
        loop {
            let mut grew = false;
            for j in 0..pieces.len() {
                if used[j] || (pieces[j].1 - run.at).abs() > off {
                    continue;
                }
                if pieces[j].2 <= run.hi + gap && pieces[j].3 >= run.lo - gap {
                    used[j] = true;
                    run.ids.push(pieces[j].0.clone());
                    run.lo = run.lo.min(pieces[j].2);
                    run.hi = run.hi.max(pieces[j].3);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        out.push(run);
    }
    out
}

/// Cells of tolerance frames drawn as one outline of separate lines (possibly in collinear pieces)
/// divided by full-height lines (card gdt_frame_content, variant line_outline_dividers). Each cell
/// carries the outline pieces along it and the dividers bounding it.
pub(super) fn line_frames(cx: &Ctx, b: &Builder) -> Result<Vec<(Vec<String>, [f64; 4])>> {
    let c = representation::card_ref("gdt_frame_content")?;
    let aspect = representation::param(c, "outline_aspect_min")?;
    let join = representation::param(c, "divider_join")?;
    let off = representation::param(c, "piece_offset_max")? * cx.scale.max(1e-6);
    let gap = representation::param(c, "piece_gap_max")? * cx.scale.max(1e-6);
    let (hmin, hmax) = (
        representation::param(c, "outline_height_min")? * cx.scale.max(1e-6),
        representation::param(c, "outline_height_max")? * cx.scale.max(1e-6),
    );
    let max_len = 40. * cx.scale.max(1e-6);
    let hs = runs(cx, b, false, off, gap, max_len);
    let vs = runs(cx, b, true, off, gap, max_len);
    let mut out: Vec<(Vec<String>, [f64; 4])> = Vec::new();
    for (i, top) in hs.iter().enumerate() {
        for bot in hs.iter().skip(i + 1) {
            let (lo_run, hi_run) = if top.at > bot.at {
                (bot, top)
            } else {
                (top, bot)
            };
            let ht = hi_run.at - lo_run.at;
            if ht < hmin || ht > hmax {
                continue;
            }
            let tol = join * ht;
            if (lo_run.lo - hi_run.lo).abs() > tol || (lo_run.hi - hi_run.hi).abs() > tol {
                continue;
            }
            let (x0, x1) = (0.5 * (lo_run.lo + hi_run.lo), 0.5 * (lo_run.hi + hi_run.hi));
            if x1 - x0 < aspect * ht {
                continue;
            }
            // sides and dividers span exactly the outline height
            let full =
                |v: &&Run| (v.lo - lo_run.at).abs() <= tol && (v.hi - hi_run.at).abs() <= tol;
            let side = |x: f64| vs.iter().filter(full).find(|v| (v.at - x).abs() <= tol);
            let (Some(left), Some(right)) = (side(x0), side(x1)) else {
                continue;
            };
            let dividers: Vec<(String, f64, f64, f64)> = vs
                .iter()
                .filter(full)
                .filter(|v| v.at > x0 + tol && v.at < x1 - tol)
                .flat_map(|v| v.ids.iter().map(move |id| (id.clone(), v.lo, v.hi, v.at)))
                .collect();
            let r = [x0, lo_run.at, x1, hi_run.at];
            let cells = symbols::split_by_dividers(r, &dividers, tol);
            if cells.len() < 2 {
                continue;
            }
            let n = cells.len();
            for (k, (dids, cell)) in cells.into_iter().enumerate() {
                let mut ids: Vec<String> = Vec::new();
                for run in [lo_run, hi_run] {
                    for id in &run.ids {
                        let (p, q) = cx.lines[id];
                        let (a, z) = (p[0].min(q[0]), p[0].max(q[0]));
                        if z > cell[0] + tol && a < cell[2] - tol && !ids.contains(id) {
                            ids.push(id.clone());
                        }
                    }
                }
                if k == 0 {
                    ids.extend(left.ids.iter().cloned());
                }
                if k + 1 == n {
                    ids.extend(right.ids.iter().cloned());
                }
                for d in dids {
                    // every piece of a divider run belongs to the cells it bounds
                    if let Some(v) = vs.iter().find(|v| v.ids.contains(&d)) {
                        for id in &v.ids {
                            if !ids.contains(id) {
                                ids.push(id.clone());
                            }
                        }
                    }
                }
                if !out.iter().any(|o| same_rect(o.1, cell)) {
                    out.push((ids, cell));
                }
            }
        }
    }
    Ok(out)
}

/// Two rectangles that agree within a tiny fraction of their size.
pub(super) fn same_rect(a: [f64; 4], c: [f64; 4]) -> bool {
    let s = (a[2] - a[0]).abs().max((a[3] - a[1]).abs()).max(1e-9);
    (0..4).all(|i| (a[i] - c[i]).abs() <= 1e-6 * s)
}

// ---------------------------------------------------------------- frame cell texts

/// Texts of one frame cell in reading order: a zone symbol written as its own text joins the value
/// that follows it (Ø + 0.3 → Ø0.3); other texts of the cell are separated by a space.
pub(super) fn cell_text(texts: &[&TextFact]) -> String {
    let mut ts: Vec<&&TextFact> = texts.iter().collect();
    ts.sort_by(|a, c| a.center[0].total_cmp(&c.center[0]));
    let zone = representation::card_ref("gdt_frame_content")
        .map(|c| {
            c.lexicon
                .iter()
                .flatten()
                .filter(|l| l.concept == "DC-G5-089" && l.entry == "prefix")
                .flat_map(|l| l.values.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut out = String::new();
    for t in ts {
        let s = t.dec.text.trim();
        if !out.is_empty() && !zone.iter().any(|z| out.ends_with(z.as_str())) {
            out.push(' ');
        }
        out.push_str(s);
    }
    out
}

// ---------------------------------------------------------------- leaders drawn in pieces

/// Leader branches from `starts` (point, previous point) through free thin segments, like the exact
/// leader rule, and also through pieces that join within `join`, and collinear pieces that overlap
/// the branch's last piece or continue it within `join` (card gdt_frame_content, variant
/// leader_pieces). Arrowheads continuing a branch end are collected.
pub(super) fn trace(
    cx: &Ctx,
    b: &Builder,
    starts: &[(P, P)],
    taken: &mut BTreeSet<String>,
    depth: usize,
    join: f64,
    off: f64,
) -> (Vec<String>, Vec<String>, Vec<P>) {
    let segs = free_segments(cx, b);
    let mut found = Vec::new();
    let mut arrows: Vec<String> = Vec::new();
    let mut ends: Vec<(P, P)> = Vec::new();
    let mut frontier: Vec<(P, P)> = starts.to_vec();
    for step in 0..depth {
        let mut next = Vec::new();
        for &(p, from) in &frontier {
            if step > 0 && reaches_structure(cx, b, p, taken) {
                ends.push((p, from));
                continue;
            }
            let d = unit(sub(p, from));
            let back = dist(p, from);
            let mut extended = false;
            let mut end = p;
            let mut end_from = from;
            for (id, pts) in &segs {
                if taken.contains(id) {
                    continue;
                }
                let n = pts.len();
                // joined at an end (exactly or within the small miss)
                let other = if dist(pts[0], p) <= join {
                    Some((pts[n - 1], pts[n - 2]))
                } else if dist(pts[n - 1], p) <= join {
                    Some((pts[0], pts[1]))
                } else {
                    None
                };
                if let Some((o, prev)) = other {
                    // a piece doubling back over the branch is an overlap, not a turn
                    if norm(d) > 0.
                        && cross(d, unit(sub(o, prev))).abs() < 0.01
                        && dot(d, sub(o, p)) < 0.
                        && project(o, from, p).1 <= off
                    {
                        taken.insert(id.clone());
                        found.push(id.clone());
                        continue;
                    }
                    taken.insert(id.clone());
                    found.push(id.clone());
                    extended = true;
                    if cx
                        .inline_arrows
                        .get(id)
                        .is_some_and(|a| continues(a, o, prev, 0.25 * a.len))
                    {
                        arrows.push(id.clone());
                        ends.push((o, prev));
                    } else {
                        next.push((o, prev));
                    }
                    continue;
                }
                // a collinear piece overlapping the last piece or continuing it after a small gap
                if n != 2 || norm(d) == 0. {
                    continue;
                }
                let (a, z) = (pts[0], pts[1]);
                if cross(d, unit(sub(z, a))).abs() > 0.01
                    || project(a, from, p).1 > off
                    || project(z, from, p).1 > off
                {
                    continue;
                }
                let (sa, sz) = (dot(sub(a, p), d), dot(sub(z, p), d));
                let (lo, hi) = (sa.min(sz), sa.max(sz));
                if lo > join || hi < -back {
                    continue;
                }
                taken.insert(id.clone());
                found.push(id.clone());
                if hi > dot(sub(end, p), d) + 1e-9 {
                    let far = if sa > sz { a } else { z };
                    end = far;
                    end_from = p;
                    extended = true;
                }
            }
            if end != p {
                next.push((end, end_from));
            } else if !extended {
                ends.push((p, from));
            }
        }
        if next.is_empty() {
            frontier.clear();
            break;
        }
        frontier = next;
    }
    ends.extend(frontier);
    for (e, from) in &ends {
        if let Some(id) = cx
            .inline_arrows
            .iter()
            .find(|(id, a)| {
                taken.contains(*id)
                    && !arrows.contains(*id)
                    && continues(a, *e, *from, 0.25 * a.len)
            })
            .map(|x| x.0.clone())
        {
            arrows.push(id);
            continue;
        }
        let near = |a: &Arrow| dist(a.tip, *e).min(dist(base_of(a), *e));
        if let Some((id, _)) = cx
            .arrows
            .iter()
            .filter(|(id, a)| {
                !b.used(id)
                    && !taken.contains(*id)
                    && !arrows.contains(*id)
                    && continues(a, *e, *from, 0.25 * a.len)
            })
            .min_by(|x, y| near(x.1).total_cmp(&near(y.1)))
        {
            arrows.push(id.clone());
        }
    }
    for a in &arrows {
        taken.insert(a.clone());
    }
    (found, arrows, ends.into_iter().map(|x| x.0).collect())
}

/// Leader tolerances of a frame of height `h` (card gdt_frame_content): the miss allowed at a join
/// and the offset allowed across collinear pieces.
pub(super) fn leader_tolerances(h: f64) -> (f64, f64) {
    let c = representation::card_ref("gdt_frame_content");
    let p = |n: &str, d: f64| {
        c.as_ref()
            .ok()
            .and_then(|c| representation::param(c, n).ok())
            .unwrap_or(d)
    };
    (p("leader_join", 0.15) * h, p("leader_offset", 0.04) * h)
}

/// The leader of a frame of height `h` from `starts`: followed through exactly joined pieces; when
/// that leader carries no terminator, it is followed again through overlapping or nearly joined
/// pieces (variant leader_pieces) and that reading is kept if it reaches a terminator. Returns the
/// pieces, the terminators, the branch ends and whether the pieces variant was needed.
#[allow(clippy::type_complexity)]
pub(super) fn frame_leader(
    cx: &Ctx,
    b: &Builder,
    start: &str,
    starts: &[(P, P)],
    taken: &mut BTreeSet<String>,
    depth: usize,
    h: f64,
) -> (Vec<String>, Vec<String>, Vec<P>, bool) {
    let (join, off) = leader_tolerances(h);
    // a leader is terminated by an arrowhead of its own or by reaching a terminator's tip
    let terminated = |r: &(Vec<String>, Vec<String>, Vec<P>), t: &BTreeSet<String>| {
        !r.1.is_empty()
            || t.iter()
                .any(|id| cx.inline_arrows.contains_key(id) && !b.used(id))
            || r.2.iter().any(|e| {
                cx.arrows
                    .iter()
                    .any(|(id, a)| !b.used(id) && dist(a.tip, *e) <= 0.1 * a.len.max(1e-9))
            })
    };
    let mut exact_taken = taken.clone();
    let mut res = branches(cx, b, starts, &mut exact_taken, depth);
    let mut pieces = false;
    if terminated(&res, &exact_taken) {
        *taken = exact_taken;
    } else {
        let mut loose_taken = taken.clone();
        let loose = trace(cx, b, starts, &mut loose_taken, depth, join, off);
        if terminated(&loose, &loose_taken) {
            res = loose;
            *taken = loose_taken;
            pieces = true;
        } else {
            *taken = exact_taken;
        }
    }
    // pieces lying along a leader piece within the leader's own extent (a doubled piece, or the shaft
    // under the arrowhead drawn again) belong to it
    let tips: Vec<P> = res
        .1
        .iter()
        .filter_map(|a| cx.arrow(a).map(|x| x.tip))
        .collect();
    let lead: Vec<String> = std::iter::once(start.to_string())
        .chain(res.0.iter().cloned())
        .collect();
    let mut extra: Vec<String> = Vec::new();
    for l in &lead {
        let Some((p, q)) = cx.lines.get(l).copied() else {
            continue;
        };
        let u = unit(sub(q, p));
        let (mut lo, mut hi) = (0., dist(p, q));
        for t in &tips {
            if project(*t, p, q).1 <= off {
                let s = dot(sub(*t, p), u);
                lo = f64::min(lo, s);
                hi = f64::max(hi, s);
            }
        }
        for (id, (a, z)) in &cx.lines {
            if taken.contains(id) || extra.contains(id) || b.used(id) || !cx.annotation_style(id) {
                continue;
            }
            if dist(*a, *z) < 1e-9
                || cross(u, unit(sub(*z, *a))).abs() > 0.01
                || project(*a, p, q).1 > off
                || project(*z, p, q).1 > off
            {
                continue;
            }
            let (sa, sz) = (dot(sub(*a, p), u), dot(sub(*z, p), u));
            if sa.min(sz) >= lo - off && sa.max(sz) <= hi + off {
                extra.push(id.clone());
            }
        }
    }
    if !extra.is_empty() {
        pieces = true;
        for id in &extra {
            taken.insert(id.clone());
        }
        res.0.extend(extra);
    }
    (res.0, res.1, res.2, pieces)
}

/// A dimension value followed in the same text by callout words (`4-Ø9 均布`, card callout_rows):
/// the value part.
pub(super) fn value_with_words(t: &TextFact) -> Option<String> {
    let lx = callout_lex(representation::card_ref("callout_rows").ok()?);
    let mut parts = t.dec.text.split_whitespace();
    let first = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    (!rest.is_empty()
        && rest.iter().all(|w| callout_words(&lx, w))
        && matches!(
            tx::lex(first),
            Lex::Diameter | Lex::Radius | Lex::Thread | Lex::Number | Lex::Chamfer
        ))
    .then(|| first.to_string())
}

/// A deviation written as its own small text: a signed number or zero.
pub(super) fn deviation_text(t: &TextFact) -> bool {
    let s: String = t.dec.text.chars().filter(|c| !c.is_whitespace()).collect();
    let body = s.trim_start_matches(['+', '-', '−', '±']);
    !body.is_empty()
        && body.chars().all(|c| c.is_ascii_digit() || c == '.')
        && (s.starts_with(['+', '-', '−', '±']) || body.chars().all(|c| c == '0' || c == '.'))
}

/// Name the drawn-head variant on run-out frames whose loose glyph has a closed head drawn as a filled
/// triangle or as three lines joined end to end (card gdt_characteristic, variant drawn_arrow_head).
pub(super) fn name_drawn_heads(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("gdt_characteristic")?;
    let join = representation::param(c, "head_join")?;
    for i in 0..b.exprs.len() {
        if !matches!(b.exprs[i].kind.as_str(), "gps_runout" | "gps_total_runout") {
            continue;
        }
        let id = b.exprs[i].id.clone();
        let glyph: Vec<&str> = b
            .members
            .iter()
            .filter(|m| m.expression_id == id && m.member_role == "symbol_glyph")
            .map(|m| m.source_handle.as_str())
            .collect();
        let solid = glyph
            .iter()
            .any(|g| matches!(cx.kind(g), "SOLID" | "TRACE"));
        let lines: Vec<(P, P)> = glyph
            .iter()
            .filter_map(|g| cx.lines.get(*g).copied())
            .collect();
        if solid || !symbols::line_triangles(&lines, join).is_empty() {
            name_variant(&mut b.exprs[i], "gdt_characteristic", "drawn_arrow_head");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- numbered datum letters

/// A datum letter with a serial number (A1, B2): one capital letter and one or two digits.
fn numbered_letter(s: &str) -> bool {
    let t = s.trim();
    let mut cs = t.chars();
    cs.next().is_some_and(|c| c.is_ascii_uppercase())
        && (2..=3).contains(&t.chars().count())
        && cs.all(|c| c.is_ascii_digit())
}

/// Datum feature symbols whose letter carries a serial number (card datum_feature_symbol, variant
/// numbered_letter): accepted only when a tolerance frame names the same datum in a datum cell, so a
/// zone code or a datum-target label beside a circle is not taken for a datum.
pub(super) fn numbered_datums(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let frames: BTreeSet<&str> = b
        .exprs
        .iter()
        .filter(|e| e.kind.starts_with("gps_") || e.kind == "geometric_tolerance")
        .map(|e| e.id.as_str())
        .collect();
    let names: BTreeSet<String> = b
        .members
        .iter()
        .filter(|m| m.member_role == "datum_reference" && frames.contains(m.expression_id.as_str()))
        .filter_map(|m| cx.texts.get(&m.source_handle))
        .flat_map(|t| {
            t.dec
                .text
                .split(|c: char| c.is_whitespace() || matches!(c, '-' | '—' | '|'))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|w| numbered_letter(w))
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    representation::loose_datums(
        cx,
        b,
        &|s: &str| names.contains(s.trim()),
        Some("numbered_letter"),
    )
}

// ---------------------------------------------------------------- callout rows (card callout_rows)

/// Words of the card `callout_rows` lexicon by entry.
struct CalloutLex {
    seps: Vec<String>,
    counters: Vec<String>,
    numerals: Vec<String>,
    words: Vec<String>,
    depth: Vec<String>,
    pronouns: Vec<String>,
}

fn callout_lex(c: &MdRepresentationCard) -> CalloutLex {
    let get = |entry: &str| -> Vec<String> {
        let mut v: Vec<String> = c
            .lexicon
            .iter()
            .flatten()
            .filter(|l| l.entry == entry)
            .flat_map(|l| l.values.iter().cloned())
            .collect();
        // longest first, so that a longer word wins over its prefix
        v.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
        v
    };
    let mut words = Vec::new();
    for e in ["distribution", "through", "feature", "size_word", "scope"] {
        words.extend(get(e));
    }
    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    CalloutLex {
        seps: get("count_separator"),
        counters: get("place_counter"),
        numerals: get("numeral"),
        words,
        depth: get("depth"),
        pronouns: get("pronoun"),
    }
}

/// A text made only of callout words: counts (`n-`, `n×`, `n处`, written with digits or numerals),
/// words of the lexicon (distribution, through, feature, size, scope) and a depth word with its value.
fn callout_words(lx: &CalloutLex, s: &str) -> bool {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let cs: Vec<char> = t.chars().collect();
    let mut i = 0;
    let mut tokens = 0;
    let at = |i: usize, w: &str| -> bool {
        let wc: Vec<char> = w.chars().collect();
        !wc.is_empty() && cs.len() >= i + wc.len() && cs[i..i + wc.len()] == wc[..]
    };
    while i < cs.len() {
        // a count: digits or numerals, then a separator or a place counter
        let mut j = i;
        while j < cs.len() && cs[j].is_ascii_digit() {
            j += 1;
        }
        if j == i {
            while j < cs.len() && lx.numerals.iter().any(|n| n.chars().eq([cs[j]])) {
                j += 1;
            }
        }
        if j > i
            && let Some(w) = lx.seps.iter().chain(lx.counters.iter()).find(|w| at(j, w))
        {
            i = j + w.chars().count();
            tokens += 1;
            continue;
        }
        if j > i && i == 0 && j < cs.len() && lx.words.iter().any(|w| at(j, w)) {
            // a bare count before a feature word (4 + 肋)
            i = j;
            tokens += 1;
            continue;
        }
        if let Some(w) = lx.depth.iter().find(|w| at(i, w)) {
            i += w.chars().count();
            let k = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            if i == k {
                return false;
            }
            tokens += 1;
            continue;
        }
        if let Some(w) = lx.words.iter().find(|w| at(i, w)) {
            i += w.chars().count();
            tokens += 1;
            continue;
        }
        return false;
    }
    tokens > 0
}

/// Rows of the texts of one callout in reading order (rows from top to bottom, texts along the
/// reading direction); a count written with its separator joins the next text without a space.
fn rows_label(texts: &[TextFact], dir: P) -> String {
    let n = [-dir[1], dir[0]];
    let mut ts: Vec<&TextFact> = texts.iter().collect();
    ts.sort_by(|a, c| dot(c.center, n).total_cmp(&dot(a.center, n)));
    let mut rows: Vec<Vec<&TextFact>> = Vec::new();
    for t in ts {
        match rows
            .iter_mut()
            .find(|r| (dot(sub(r[0].center, t.center), n)).abs() < 0.6 * r[0].height.max(t.height))
        {
            Some(r) => r.push(t),
            None => rows.push(vec![t]),
        }
    }
    let mut out = String::new();
    for mut r in rows {
        r.sort_by(|a, c| dot(a.center, dir).total_cmp(&dot(c.center, dir)));
        for t in r {
            let s = t.dec.text.trim();
            if !out.is_empty() && !out.ends_with(['-', '×']) {
                out.push(' ');
            }
            out.push_str(s);
        }
    }
    out
}

/// Value texts of the dimension-family expressions: the native text or the dimension text members.
fn value_texts(
    cx: &Ctx,
    b: &Builder,
    native_texts: &BTreeMap<usize, TextFact>,
) -> Vec<(usize, TextFact)> {
    let mut out = Vec::new();
    for (i, e) in b.exprs.iter().enumerate() {
        if !(e.kind.starts_with("dimension_") || e.kind == "thread_note") {
            continue;
        }
        if let Some(t) = native_texts.get(&i) {
            out.push((i, t.clone()));
        }
        for m in b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id && m.member_role == "dimension_text")
        {
            if let Some(t) = cx.texts.get(&m.source_handle) {
                out.push((i, t.clone()));
            }
        }
    }
    out
}

/// Words written with a dimension or a hole callout (card callout_rows): deviations written as two
/// small texts after the value; rows of callout words on the value's row or directly under it (on
/// and under the reference line); and a requirement sentence written along the value that refers to
/// it (此直径 …), which is read as a requirement bounded by that dimension.
pub(super) fn callouts(
    cx: &Ctx,
    b: &mut Builder,
    native_texts: &BTreeMap<usize, TextFact>,
) -> Result<()> {
    let c = representation::card_ref("callout_rows")?;
    let lx = callout_lex(c);
    let p = |n: &str| representation::param(c, n);
    let (step_min, step_max) = (p("row_step_min")?, p("row_step_max")?);
    let row_gap = p("row_gap_max")?;
    let (hr_min, hr_max) = (p("height_ratio_min")?, p("height_ratio_max")?);
    let cos_min = p("direction_cos_min")?;
    let (dev_min, dev_max) = (p("deviation_height_min")?, p("deviation_height_max")?);
    let topics = representation::card_ref("technical_note")?;
    let values = value_texts(cx, b, native_texts);
    let parallel = |a: &TextFact, t: &TextFact| dot(a.dir, t.dir) >= cos_min;
    let mut taken: BTreeSet<String> = BTreeSet::new();
    // (1) deviations written as two small texts after the value
    for (i, v) in &values {
        let n = [-v.dir[1], v.dir[0]];
        let h = v.height;
        let devs: Vec<&TextFact> = cx
            .texts
            .values()
            .filter(|t| {
                let d = sub(t.center, v.center);
                let along = dot(d, v.dir);
                !b.used(&t.id)
                    && !taken.contains(&t.id)
                    && deviation_text(t)
                    && parallel(v, t)
                    && (dev_min * h..=dev_max * h).contains(&t.height)
                    && along > 0.
                    && along < 0.5 * v.width + 2.5 * h
                    && dot(d, n).abs() < h
            })
            .collect();
        let upper = devs
            .iter()
            .filter(|t| dot(sub(t.center, v.center), n) > 0.)
            .min_by(|a, z| {
                dot(sub(a.center, v.center), v.dir).total_cmp(&dot(sub(z.center, v.center), v.dir))
            });
        let lower = devs
            .iter()
            .filter(|t| dot(sub(t.center, v.center), n) < 0.)
            .min_by(|a, z| {
                dot(sub(a.center, v.center), v.dir).total_cmp(&dot(sub(z.center, v.center), v.dir))
            });
        let (Some(u), Some(l)) = (upper, lower) else {
            continue;
        };
        if (dot(sub(u.center, l.center), v.dir)).abs() > h
            || dot(sub(u.center, l.center), n) < 0.6 * u.height.min(l.height)
        {
            continue;
        }
        for t in [u, l] {
            taken.insert(t.id.clone());
            b.member(
                *i,
                &t.id,
                None,
                [0., 1.],
                "tolerance_text",
                "text",
                "candidate",
                vec![],
                vec![
                    "card:dimension_text".into(),
                    "deviation_written_apart".into(),
                ],
            );
        }
        let e = &mut b.exprs[*i];
        e.label = format!("{} {}/{}", e.label, u.dec.text.trim(), l.dec.text.trim());
        if !e.qualifiers.iter().any(|q| q == "toleranced") {
            e.qualifiers.push("toleranced".into());
        }
        name_variant(e, "dimension_text", "deviations");
    }
    // (2) callout rows: words on the value's row and on the rows under it
    for (i, v) in &values {
        let n = [-v.dir[1], v.dir[0]];
        let eid = b.exprs[*i].id.clone();
        let mut group: Vec<TextFact> = vec![v.clone()];
        group.extend(
            b.members
                .iter()
                .filter(|m| m.expression_id == eid && m.member_role == "note_text")
                .filter_map(|m| cx.texts.get(&m.source_handle).cloned()),
        );
        let mut added: Vec<TextFact> = Vec::new();
        loop {
            let next = cx.texts.values().find(|t| {
                if b.used(&t.id)
                    || taken.contains(&t.id)
                    || !parallel(v, t)
                    || !callout_words(&lx, &t.dec.text)
                {
                    return false;
                }
                group.iter().chain(added.iter()).any(|g| {
                    let hh = g.height.max(t.height);
                    let ratio = t.height / g.height;
                    let d = sub(t.center, g.center);
                    let across = dot(d, n);
                    let gap = dot(d, v.dir).abs() - 0.5 * (g.width + t.width);
                    (hr_min..=hr_max).contains(&ratio)
                        && ((across.abs() < 0.6 * hh && gap < row_gap * hh)
                            || (-across >= step_min * hh
                                && -across <= step_max * hh
                                && gap < 0.5 * hh))
                })
            });
            let Some(t) = next.cloned() else { break };
            taken.insert(t.id.clone());
            added.push(t);
        }
        if added.is_empty() {
            continue;
        }
        for t in &added {
            b.member(
                *i,
                &t.id,
                None,
                [0., 1.],
                "note_text",
                "text",
                "candidate",
                vec![],
                vec!["card:callout_rows".into(), "callout_row".into()],
            );
        }
        let all: Vec<TextFact> = group.iter().chain(added.iter()).cloned().collect();
        // pieces of the reference line under the joined words, collinear with the callout's own line
        let own: Vec<(P, P)> = b
            .members
            .iter()
            .filter(|m| {
                m.expression_id == eid
                    && matches!(m.member_role.as_str(), "leader_line" | "dimension_line")
            })
            .filter_map(|m| cx.lines.get(&m.source_handle).copied())
            .collect();
        let (_, off) = leader_tolerances(v.height);
        let pieces: Vec<String> = cx
            .lines
            .iter()
            .filter(|(id, (a, z))| {
                !b.used(id)
                    && cx.annotation_style(id)
                    && all.iter().any(|t| on_shoulder(t, &[*a, *z]))
                    && own.iter().any(|(p, q)| {
                        cross(unit(sub(*q, *p)), unit(sub(*z, *a))).abs() < 0.01
                            && project(*a, *p, *q).1 <= off
                            && project(*z, *p, *q).1 <= off
                    })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &pieces {
            let lt = member_line_type(cx.lt(id), "leader_line");
            b.member(
                *i,
                id,
                None,
                [0., 1.],
                "leader_line",
                lt,
                "candidate",
                vec![],
                vec!["card:callout_rows".into(), "reference_line_piece".into()],
            );
        }
        let native = native_texts.contains_key(i);
        let e = &mut b.exprs[*i];
        e.label = if native {
            // the native display text stands for the value
            let mut rest = added.clone();
            rest.sort_by(|a, z| dot(z.center, n).total_cmp(&dot(a.center, n)));
            format!("{} {}", e.label, rows_label(&rest, v.dir))
        } else {
            rows_label(&all, v.dir)
        };
        name_variant(e, "callout_rows", "rows_on_reference_line");
        push_ev(e, "concept:DC-G1-111".into());
        for t in &added {
            let s: String = t.dec.text.chars().filter(|c| !c.is_whitespace()).collect();
            for (concept, entry) in [
                ("DC-G4-066", "distribution"),
                ("DC-G4-043", "distribution"),
                ("DC-G4-067", "scope"),
            ] {
                let hit = c.lexicon.iter().flatten().any(|l| {
                    l.concept == concept
                        && l.entry == entry
                        && l.values.iter().any(|w| s.contains(w.as_str()))
                });
                if hit {
                    push_ev(e, format!("concept:{concept}"));
                }
            }
        }
    }
    // (3) a requirement sentence written along the value, referring to it
    for (i, v) in &values {
        let n = [-v.dir[1], v.dir[0]];
        let h = v.height;
        let mut rows: Vec<TextFact> = Vec::new();
        loop {
            let next = cx.texts.values().find(|t| {
                let s = &t.dec.text;
                if b.used(&t.id)
                    || taken.contains(&t.id)
                    || rows.iter().any(|r| r.id == t.id)
                    || !parallel(v, t)
                {
                    return false;
                }
                if !lx.pronouns.iter().any(|w| s.contains(w.as_str())) {
                    return false;
                }
                std::iter::once(v).chain(rows.iter()).any(|g| {
                    let d = sub(t.center, g.center);
                    let across = -dot(d, n);
                    let along = dot(d, v.dir).abs();
                    across >= 0.5 * step_min * h
                        && across <= step_max * h.max(g.height)
                        && along < 0.5 * (g.width + t.width) + h
                })
            });
            let Some(t) = next.cloned() else { break };
            rows.push(t);
        }
        if rows.is_empty() {
            continue;
        }
        rows.sort_by(|a, z| dot(z.center, n).total_cmp(&dot(a.center, n)));
        // the terminator of the dimension stands on a short arc concentric with the part: the
        // boundary the sentence speaks of (range line)
        let eid = b.exprs[*i].id.clone();
        let tips: Vec<P> = b
            .members
            .iter()
            .filter(|m| m.expression_id == eid && m.member_role == "arrowhead")
            .filter_map(|m| cx.arrow(&m.source_handle).map(|a| a.tip))
            .collect();
        // a continuous (range) line, the tip within its sweep; a hidden or centre circle of the
        // same radius is not the boundary
        let boundary: Vec<String> = cx
            .rec
            .keys()
            .filter(|id| {
                cx.kind(id) == "ARC"
                    && !b.used(id)
                    && cx
                        .style
                        .get(*id)
                        .is_some_and(|st| st.pattern == Pattern::Continuous)
            })
            .filter(|id| {
                cx.arc(id).is_some_and(|(cc, r, s0, sw, _)| {
                    tips.iter().any(|tp| {
                        let rel = ((tp[1] - cc[1]).atan2(tp[0] - cc[0]) - s0)
                            .rem_euclid(std::f64::consts::TAU);
                        (dist(*tp, cc) - r).abs() <= 0.05 * h && rel <= sw + 1e-6
                    })
                })
            })
            .cloned()
            .collect();
        let text: String = rows
            .iter()
            .map(|t| t.dec.text.trim().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let kind = notation::note_topic(topics, &text);
        let src_members: Vec<MdExpressionMember> = b
            .members
            .iter()
            .filter(|m| m.expression_id == eid)
            .cloned()
            .collect();
        let label = format!("{} {}", b.exprs[*i].label, text);
        let r = b.expr(
            kind,
            &label,
            "card:callout_rows/bounded_requirement",
            "candidate",
            vec![],
            vec![
                "card:callout_rows".into(),
                "variant:bounded_requirement".into(),
                format!("bounded_by:{}", eid),
            ],
            vec!["Requirement scope follows the bounding dimension; the surfaces it covers are not resolved".into()],
        );
        for t in &rows {
            taken.insert(t.id.clone());
            b.member(
                r,
                &t.id,
                None,
                [0., 1.],
                "note_text",
                "text",
                "candidate",
                vec![],
                vec!["card:callout_rows".into()],
            );
        }
        for m in src_members {
            let iv = [m.interval[0], m.interval[1]];
            b.member(
                r,
                &m.source_handle,
                m.native_member_id,
                iv,
                &m.member_role,
                &m.line_type,
                "candidate",
                vec![],
                vec!["bounding_dimension".into()],
            );
        }
        for a in &boundary {
            let lt = cx.lt(a);
            b.member(
                r,
                a,
                None,
                [0., 1.],
                "extension_line",
                lt,
                "candidate",
                vec![],
                vec!["boundary_line_of_dimension".into()],
            );
            b.member(
                *i,
                a,
                None,
                [0., 1.],
                "extension_line",
                lt,
                "candidate",
                vec![],
                vec!["boundary_line_of_dimension".into()],
            );
            push_ev(&mut b.exprs[r], "concept:DC-G1-047".into());
        }
        b.links.push((r, "bounded_by_dimension", *i));
    }
    Ok(())
}

// ---------------------------------------------------------------- leader notes (card leader_note)

/// The distance from `p` to the nearest drawn curve that is not one of `skip`.
fn near_curve(cx: &Ctx, p: P, skip: &BTreeSet<String>) -> f64 {
    let mut d = f64::INFINITY;
    for (id, (a, z)) in &cx.lines {
        if !skip.contains(id) {
            let v = sub(*z, *a);
            let nn = dot(v, v);
            let t = if nn < 1e-18 {
                0.
            } else {
                (dot(sub(p, *a), v) / nn).clamp(0., 1.)
            };
            d = d.min(dist(p, add(*a, mul(v, t))));
        }
    }
    for id in cx.rec.keys() {
        if skip.contains(id) || !matches!(cx.kind(id), "ARC" | "CIRCLE") {
            continue;
        }
        if let Some((c, r, s0, sw, full)) = cx.arc(id) {
            let ang = (p[1] - c[1]).atan2(p[0] - c[0]);
            let rel = (ang - s0).rem_euclid(std::f64::consts::TAU);
            if full || rel <= sw {
                d = d.min((dist(p, c) - r).abs());
            }
        }
    }
    d
}

/// A note written on a reference line joined by a leader to the drawing (card leader_note): the
/// words stand on the reference line (and the row under it), the leader bends away from it and
/// ends in an arrowhead, or without a terminator close to a drawn line.
pub(super) fn leader_notes(
    cx: &Ctx,
    b: &mut Builder,
    anchors: &mut BTreeMap<usize, P>,
) -> Result<()> {
    let c = representation::card_ref("leader_note")?;
    let lx = callout_lex(representation::card_ref("callout_rows")?);
    let reach = representation::param(c, "end_reach")?;
    let bend = representation::param(c, "bend_sin_min")?;
    let shoulder_extra = representation::param(c, "shoulder_extra")?;
    let rc = representation::card_ref("callout_rows")?;
    let (step_min, step_max) = (
        representation::param(rc, "row_step_min")?,
        representation::param(rc, "row_step_max")?,
    );
    let seeds: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| {
            !b.used(&t.id)
                && t.dec.text.chars().any(tx::is_cjk)
                && !matches!(t.lex, Lex::Heading | Lex::ListNumber | Lex::ViewSuffix)
        })
        .cloned()
        .collect();
    for t in seeds {
        if b.used(&t.id) {
            continue;
        }
        let Some((sid, pts)) = free_segments(cx, b).into_iter().find(|(id, pts)| {
            let l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
            cx.lines.contains_key(id)
                && on_shoulder(&t, pts)
                && l <= t.width + shoulder_extra * t.height
        }) else {
            continue;
        };
        let (s0, s1) = (pts[0], pts[pts.len() - 1]);
        let su = unit(sub(s1, s0));
        let mut taken = BTreeSet::from([sid.clone(), t.id.clone()]);
        let starts = [(s0, s1), (s1, s0)];
        let mut exact_taken = taken.clone();
        let exact = branches(cx, b, &starts, &mut exact_taken, 4);
        let h = t.height;
        let leg = |segs: &[String]| {
            segs.iter().any(|s| {
                cx.lines
                    .get(s)
                    .is_some_and(|(a, z)| cross(su, unit(sub(*z, *a))).abs() >= bend)
            })
        };
        let ok = |r: &(Vec<String>, Vec<String>, Vec<P>), tk: &BTreeSet<String>| -> bool {
            leg(&r.0)
                && (!r.1.is_empty()
                    || r.2.iter().any(|e| {
                        dist(*e, s0) > 1e-9
                            && dist(*e, s1) > 1e-9
                            && near_curve(cx, *e, tk) <= reach * h
                    }))
        };
        let (segs, arrows, ends) = if ok(&exact, &exact_taken) {
            taken = exact_taken;
            exact
        } else {
            let (join, off) = leader_tolerances(h);
            let mut loose_taken = taken.clone();
            let loose = trace(cx, b, &starts, &mut loose_taken, 4, join, off);
            if !ok(&loose, &loose_taken) {
                continue;
            }
            taken = loose_taken;
            loose
        };
        let _ = &taken;
        // the rows: words on the reference line beside the seed, callout words under it
        let n = [-t.dir[1], t.dir[0]];
        let mut rows: Vec<TextFact> = vec![t.clone()];
        loop {
            let next = cx.texts.values().find(|o| {
                if b.used(&o.id) || rows.iter().any(|r| r.id == o.id) || dot(o.dir, t.dir) < 0.99 {
                    return false;
                }
                let on_line = on_shoulder(o, &pts);
                let under = callout_words(&lx, &o.dec.text)
                    && rows.iter().any(|r| {
                        let d = sub(o.center, r.center);
                        let across = -dot(d, n);
                        let gap = dot(d, t.dir).abs() - 0.5 * (r.width + o.width);
                        across >= step_min * r.height.max(o.height)
                            && across <= step_max * r.height.max(o.height)
                            && gap < 0.5 * h
                    });
                on_line || under
            });
            let Some(o) = next.cloned() else { break };
            rows.push(o);
        }
        let label = rows_label(&rows, t.dir);
        let variant = if arrows.is_empty() {
            "no_terminator"
        } else {
            "arrow_terminator"
        };
        let e = b.expr(
            "leader_note",
            &label,
            &format!("card:leader_note/{variant}"),
            "candidate",
            vec![],
            vec![
                "card:leader_note".into(),
                format!("variant:{variant}"),
                format!("text:{}", t.id),
                format!("shoulder:{sid}"),
            ],
            vec!["The feature the note speaks of is not resolved".into()],
        );
        for r in &rows {
            b.member(
                e,
                &r.id,
                None,
                [0., 1.],
                "note_text",
                "text",
                "candidate",
                vec![],
                vec!["card:leader_note".into()],
            );
        }
        let mut lines = vec![sid.clone()];
        lines.extend(segs);
        add_leader(cx, b, e, &lines, &arrows, "leader_note");
        if let Some(p) = arrows
            .first()
            .and_then(|a| cx.arrow(a))
            .map(|a| a.tip)
            .or_else(|| {
                ends.iter()
                    .copied()
                    .find(|e| dist(*e, s0) > 1e-9 && dist(*e, s1) > 1e-9)
            })
        {
            anchors.insert(e, p);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- loose copies of a read block

/// Insertion point of a text as the block shape keeps it (the alignment point when justified).
fn text_point(r: &Record) -> Option<P> {
    if r.get(72).is_some_and(|v| v.trim() != "0") {
        r.xy(11).or_else(|_| r.xy(10)).ok()
    } else {
        r.xy(10).ok()
    }
}

/// Loose lines and texts that repeat, one by one, the content of a block already read (the same
/// table drawn again over its block): each is a coincident source of the block's expression
/// (card coincident_redraw, GB/T 14665-2012 4.3 coincident lines are drawn once). A copy must
/// repeat at least the card's share of the block's lines; single coincidences are left alone.
pub(super) fn redrawn_copies(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("coincident_redraw")?;
    let tol = representation::param(c, "coincidence_tolerance")? * cx.scale.max(1e-6);
    let share = representation::param(c, "copy_share_min")?;
    let owners: Vec<(String, usize)> = cx
        .shapes
        .keys()
        .filter_map(|h| {
            let ms = b.by_source.get(h)?;
            let m = ms
                .iter()
                .map(|&m| &b.members[m])
                .find(|m| m.member_role != "unresolved_source")?;
            let e = b.exprs.iter().position(|e| e.id == m.expression_id)?;
            Some((h.clone(), e))
        })
        .collect();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (h, e) in owners {
        let s = &cx.shapes[&h];
        if s.lines.len() < 2 {
            continue;
        }
        let same = |a: P, z: P, p: P, q: P| {
            (dist(a, p) <= tol && dist(z, q) <= tol) || (dist(a, q) <= tol && dist(z, p) <= tol)
        };
        let lines: Vec<String> = cx
            .lines
            .iter()
            .filter(|(id, (a, z))| {
                !b.used(id)
                    && !taken.contains(*id)
                    && s.lines.iter().any(|(p, q)| same(*a, *z, *p, *q))
            })
            .map(|(id, _)| id.clone())
            .collect();
        if (lines.len() as f64) < share * s.lines.len() as f64 {
            continue;
        }
        let compact = |x: &str| x.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let texts: Vec<String> = cx
            .texts
            .values()
            .filter(|t| !b.used(&t.id) && !taken.contains(&t.id))
            .filter(|t| {
                let at = cx.rec.get(&t.id).and_then(|r| text_point(r));
                s.texts.iter().any(|(st, sp, sh)| {
                    compact(&tx::decode(st).text) == compact(&t.dec.text)
                        && at.is_some_and(|p| dist(p, *sp) <= tol.max(0.05 * sh))
                })
            })
            .map(|t| t.id.clone())
            .collect();
        for id in lines.iter().chain(texts.iter()) {
            taken.insert(id.clone());
            let lt = cx.lt(id);
            b.member(
                e,
                id,
                None,
                [0., 1.],
                "coincident_source",
                lt,
                "candidate",
                vec![],
                vec![format!("coincident_with:{h}"), "card:coincident_redraw".into()],
            );
        }
        let ex = &mut b.exprs[e];
        push_ev(ex, "card:coincident_redraw".into());
        push_ev(ex, "variant:block_redrawn_loose".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- notes in the sheet margin

fn lexicon_values(c: &MdRepresentationCard, concept: &str, entry: &str) -> Vec<String> {
    c.lexicon
        .iter()
        .flatten()
        .filter(|l| l.concept == concept && l.entry == entry)
        .flat_map(|l| l.values.iter().cloned())
        .collect()
}

/// A date written as eight digits (year, month, day).
fn has_date(s: &str) -> bool {
    let cs: Vec<char> = s.chars().collect();
    cs.windows(8).enumerate().any(|(i, w)| {
        w.iter().all(|c| c.is_ascii_digit())
            && (i == 0 || !cs[i - 1].is_ascii_digit())
            && cs.get(i + 8).is_none_or(|c| !c.is_ascii_digit())
            && {
                let m: u32 = w[4..6].iter().collect::<String>().parse().unwrap_or(0);
                let d: u32 = w[6..8].iter().collect::<String>().parse().unwrap_or(0);
                (1..=12).contains(&m) && (1..=31).contains(&d)
            }
    })
}

/// Texts written outside the frame line of a read sheet (card title_block_field): in the margin
/// between the paper edge and the frame (binding margin) they are notes of that sheet — a weight,
/// a code, a date — read as title-block information; outside every sheet, a sentence with a change
/// verb and a date is a revision note of the drawing. Texts inside the frame are left to the other
/// rules.
pub(super) fn margin_notes(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("title_block_field")?;
    let weights = lexicon_values(c, "DC-G9-026", "weight_word");
    let verbs = lexicon_values(c, "DC-G9-023", "change_verb");
    // the paper edge and the frame line of each sheet frame block
    let mut sheets: Vec<([f64; 4], [f64; 4])> = Vec::new();
    for e in b
        .exprs
        .iter()
        .filter(|e| e.kind == "sheet_frame" && e.rule == "sheet_size_rectangular_border")
    {
        for m in b.members.iter().filter(|m| m.expression_id == e.id) {
            let Some(s) = cx.shapes.get(&m.source_handle) else {
                continue;
            };
            let rects: Vec<[f64; 4]> = s.paths.iter().filter_map(|p| cell_rect(p)).collect();
            let Some(outer) = rects
                .iter()
                .copied()
                .max_by(|x, y| area(*x).total_cmp(&area(*y)))
            else {
                continue;
            };
            let inner = rects
                .iter()
                .copied()
                .filter(|r| {
                    *r != outer
                        && r[0] > outer[0]
                        && r[1] > outer[1]
                        && r[2] < outer[2]
                        && r[3] < outer[3]
                })
                .max_by(|x, y| area(*x).total_cmp(&area(*y)));
            if let Some(inner) = inner {
                sheets.push((outer, inner));
            }
        }
    }
    if sheets.is_empty() {
        return Ok(());
    }
    let free: Vec<TextFact> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id))
        .cloned()
        .collect();
    for t in free {
        let s: String = t.dec.text.chars().filter(|c| !c.is_whitespace()).collect();
        let in_margin = sheets
            .iter()
            .any(|(o, i)| inside(t.center, *o, 0.) && !inside(t.center, *i, 0.));
        let outside = !sheets.iter().any(|(o, _)| inside(t.center, *o, 0.));
        let revision = outside && verbs.iter().any(|v| s.contains(v.as_str())) && has_date(&s);
        if !in_margin && !revision {
            continue;
        }
        let (variant, concept, label) = if revision {
            (
                "revision_note",
                "DC-G9-023",
                format!("更改说明 {}", t.dec.text.trim()),
            )
        } else if weights.iter().any(|w| s.contains(w.as_str())) {
            (
                "margin_note",
                "DC-G9-026",
                format!("图框外注写 {}", t.dec.text.trim()),
            )
        } else {
            (
                "margin_note",
                "DC-G9-028",
                format!("图框外注写 {}", t.dec.text.trim()),
            )
        };
        let e = b.expr(
            "title_block",
            &label,
            &format!("card:title_block_field/{variant}"),
            "candidate",
            vec![],
            vec![
                "card:title_block_field".into(),
                format!("variant:{variant}"),
                format!("concept:{concept}"),
                format!("text:{}", t.id),
            ],
            vec!["Which sheet or version the note belongs to is not resolved".into()],
        );
        b.member(
            e,
            &t.id,
            None,
            [0., 1.],
            "note_text",
            "text",
            "candidate",
            vec![],
            vec!["card:title_block_field".into()],
        );
    }
    Ok(())
}

/// Name a variant of a card on an expression.
pub(super) fn name_variant(e: &mut MdEngineeringExpression, card: &str, variant: &str) {
    push_ev(e, format!("card:{card}"));
    push_ev(e, format!("variant:{variant}"));
}
