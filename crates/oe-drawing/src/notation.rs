//! Text-anchored notations read from recognition cards (wave 1).
//!
//! Four cards of `knowledge/drawing-representation.toml` carry the knowledge used here as a
//! lexicon tied to catalogue concepts:
//! * `dimension_text` — the written forms of dimension values (GB/T 4458.4, GB/T 4458.5,
//!   GB/T 16675.2, GB/T 197, GB/T 4459.1): size symbols, repetition counts, tolerance classes,
//!   deviations, thread designations, hole-callout symbols and angle units;
//! * `technical_note` — the wording of technical-requirement items (general tolerances,
//!   untoleranced chamfers and fillets, edges and burrs, heat treatment, hardness);
//! * `title_block_field` — the zones and fields of the title block (GB/T 10609.1, GB/T 14690);
//! * `series_table` — tabular drawings whose variable dimensions are letters listed in a table.
//!
//! The code implements only the token structure (a count before a size symbol, a class code
//! after a nominal value, a grid of cells); which symbols, letters and words mean which concept
//! is read from the cards. Every expression these rules touch names the card, the variant and
//! the concepts (`concept:<id>` evidence, moved into `concept_ids` at the end).
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
fn entries<'a>(c: &'a MdRepresentationCard, entry: &str) -> Vec<(&'a str, Vec<&'a str>)> {
    c.lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == entry)
        .map(|l| {
            (
                l.concept.as_str(),
                l.values.iter().map(String::as_str).collect(),
            )
        })
        .collect()
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn is_num_char(c: char) -> bool {
    c.is_ascii_digit() || c == '.'
}
/// Leading unsigned number of `s` (digits with one optional decimal point) and the rest.
fn number_prefix(s: &str) -> Option<(&str, &str)> {
    let end = s
        .char_indices()
        .find(|(_, c)| !is_num_char(*c))
        .map_or(s.len(), |(i, _)| i);
    let n = &s[..end];
    (!n.is_empty() && n.chars().any(|c| c.is_ascii_digit()) && n.matches('.').count() <= 1)
        .then(|| (n, &s[end..]))
}
fn signed_number(s: &str) -> bool {
    let t = s.trim().trim_start_matches(['+', '-', '−']);
    !t.is_empty() && t.chars().all(is_num_char) && t.chars().any(|c| c.is_ascii_digit())
}
fn is_zero(s: &str) -> bool {
    let t = s.trim().trim_start_matches(['+', '-', '−']);
    !t.is_empty() && t.chars().all(|c| c == '0' || c == '.') && t.contains('0')
}

/// Concepts of a dimension text (`dimension_text` card) with the variant that found each.
pub(crate) fn dimension_concepts(
    c: &MdRepresentationCard,
    raw: &str,
) -> Vec<(String, &'static str)> {
    let dec = tx::decode(raw);
    let mut out: Vec<(String, &'static str)> = Vec::new();
    let mut add = |id: &str, v: &'static str| {
        if !out.iter().any(|(x, _)| x == id) {
            out.push((id.to_string(), v));
        }
    };
    let text: String = dec
        .text
        .replace(['φ', 'Φ', '⌀'], "Ø")
        .chars()
        .filter(|c| !matches!(c, '☆' | '★'))
        .collect();
    let t = compact(&text);
    if t.is_empty() {
        return Vec::new();
    }
    let has = |vs: &[&str]| vs.iter().any(|v| !v.is_empty() && t.contains(v));
    let first_alpha = |s: &str, p: &str| -> bool {
        s.strip_prefix(p).is_some_and(|r| {
            r.chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '(')
        })
    };
    // Units: every dimension value is in millimetres unless it carries an angle unit or a word.
    let units = values(c, "DC-G4-099", "unit");
    if t.chars().any(|ch| ch.is_ascii_digit()) || has(&units) {
        add("DC-G4-099", "angle_units");
    }
    if t.chars().any(|ch| ch.is_ascii_digit()) {
        add("DC-G4-017", "size_symbols");
    }
    // Repetition count: n× or the older n- before a size symbol or value.
    let seps = values(c, "DC-G4-094", "count_separator");
    let mut body: &str = &t;
    if let Some((n, rest)) = number_prefix(&t)
        && !n.contains('.')
        && let Some(sep) = seps.iter().find(|s| rest.starts_with(**s))
    {
        let after = &rest[sep.len()..];
        // `a×b°` alone is a chamfer or an angle step, not a count.
        let angle_after =
            number_prefix(after).is_some_and(|(_, r)| r.starts_with('°') && !r.contains('='));
        let old_dash = *sep == "-"
            && !after
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == 'Ø');
        if !after.is_empty() && !angle_after && !old_dash {
            add("DC-G4-094", "count_prefix");
            body = after;
        }
    }
    let sym_dia = values(c, "DC-G4-055", "prefix");
    let sym_rad = values(c, "DC-G4-041", "prefix");
    let sym_ch = values(c, "DC-G4-040", "prefix");
    let threads = values(c, "DC-G3-088", "prefix");
    let pipes = values(c, "DC-G3-088", "prefix_with_fraction");
    let thread_words = values(c, "DC-G3-088", "word");
    // Thread designation: a profile code followed by the nominal size (B needs ×pitch, pipe
    // threads an inch size with a fraction).
    let digit_after = |p: &str| {
        body.strip_prefix(p)
            .filter(|r| r.chars().next().is_some_and(|d| d.is_ascii_digit()))
    };
    let thread = threads
        .iter()
        .find(|p| digit_after(p).is_some_and(|r| **p != "B" || r.contains('×')))
        .or_else(|| {
            pipes
                .iter()
                .find(|p| digit_after(p).is_some_and(|r| r.contains('/')))
        });
    // Size symbols.
    if sym_dia.iter().any(|s| body.contains(s)) {
        add("DC-G4-055", "size_symbols");
        add("DC-G4-030", "size_symbols");
    }
    let radius = thread.is_none()
        && sym_rad.iter().any(|p| {
            body == *p
                || first_alpha(body, p)
                || body.split(['，', ',', '为']).any(|w| {
                    w.contains(p)
                        && w.rsplit(p)
                            .next()
                            .is_some_and(|r| r.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
                })
        });
    if radius {
        add("DC-G4-041", "size_symbols");
        add("DC-G4-006", "size_symbols");
        add("DC-G3-080", "size_symbols");
    }
    let chamfer_c = sym_ch.iter().any(|p| {
        first_alpha(body, p)
            || t.char_indices().any(|(i, ch)| {
                p.starts_with(ch)
                    && t[i + ch.len_utf8()..]
                        .chars()
                        .next()
                        .is_some_and(|d| d.is_ascii_digit())
                    && (i == 0
                        || !t[..i]
                            .chars()
                            .last()
                            .is_some_and(|q| q.is_ascii_alphanumeric()))
            })
    }) && !has(&thread_words);
    // a×b° chamfer (45° is the C chamfer written in the older way).
    let chamfer_angle = number_prefix(body).and_then(|(_, r)| {
        let r = seps
            .iter()
            .filter(|s| **s != "-")
            .find_map(|s| r.strip_prefix(*s))?;
        let (a, rest) = number_prefix(r)?;
        (rest == "°").then_some(a)
    });
    if chamfer_c || chamfer_angle.is_some() {
        add("DC-G3-091", "size_symbols");
        if chamfer_c || chamfer_angle == Some("45") {
            add("DC-G4-040", "size_symbols");
        }
    }
    if thread.is_some() || has(&thread_words) {
        add("DC-G3-088", "thread_designation");
        if thread == Some(&"M") {
            add("DC-G3-223", "thread_designation");
        }
    }
    let positions = values(c, "DC-G3-232", "thread_positions");
    let thread_class = |s: &str| -> bool {
        // one or two (grade digit, tolerance position letter) pairs, e.g. 6H, 5g6g
        let cs: Vec<char> = s.chars().collect();
        (cs.len() == 2 || cs.len() == 4)
            && cs.chunks(2).all(|p| {
                p.len() == 2
                    && ('3'..='9').contains(&p[0])
                    && positions.iter().any(|x| x.chars().eq([p[1]]))
            })
    };
    let segments: Vec<&str> = body.split(['-', '/']).collect();
    if (thread.is_some() && segments.iter().skip(1).any(|s| thread_class(s)))
        || (thread.is_none() && segments.iter().all(|s| thread_class(s)))
    {
        add("DC-G3-232", "thread_designation");
    }
    let depth = values(c, "DC-G3-089", "glyph");
    if thread.is_some() && depth.iter().any(|g| body.contains(g)) {
        add("DC-G3-089", "thread_designation");
    }
    // Hole callouts: depth, counterbore and countersink symbols or words after a size.
    let hole = values(c, "DC-G4-088", "glyph");
    if hole.iter().any(|g| body.contains(g)) && (body.contains('Ø') || thread.is_some()) {
        add("DC-G4-088", "hole_callout");
    }
    // Angles.
    let angle = values(c, "DC-G4-032", "glyph");
    if angle.iter().any(|g| t.contains(g))
        && t.chars().any(|ch| ch.is_ascii_digit())
        && chamfer_angle.is_none()
    {
        add("DC-G4-032", "angle_units");
    }
    // Tolerances.
    let pm = values(c, "DC-G4-126", "glyph");
    if pm.iter().any(|g| t.contains(g)) {
        add("DC-G4-126", "deviations");
    }
    let letters = values(c, "DC-G4-114", "deviation_letters");
    let grades = values(c, "DC-G4-114", "grades");
    let class_at = |s: &str| -> Option<usize> {
        // fundamental-deviation letters (one or two, same case) followed by an IT grade
        for l in letters.iter().filter(|l| s.starts_with(**l)) {
            let r = &s[l.len()..];
            let g: String = r.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !g.is_empty() && grades.contains(&g.as_str()) {
                let rest = &r[g.len()..];
                if !rest
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '.')
                {
                    return Some(l.len() + g.len());
                }
            }
        }
        None
    };
    let mut class = false;
    if thread.is_none() {
        let mut i = 0;
        let chars: Vec<(usize, char)> = body.char_indices().collect();
        while i < chars.len() {
            let (pos, ch) = chars[i];
            let prev_digit = i > 0
                && (chars[i - 1].1.is_ascii_digit()
                    || chars[i - 1].1 == '('
                    || chars[i - 1].1 == '（');
            if ch.is_ascii_alphabetic()
                && (prev_digit || pos == 0)
                && class_at(&body[pos..]).is_some()
            {
                // `C1` chamfer and `R5` radius are size symbols, not classes; GB/T references are not classes.
                if !(pos == 0 && (chamfer_c || radius)) && !body[..pos].contains("GB") {
                    class = true;
                }
            }
            i += 1;
        }
        if let Some(r) = t.strip_prefix("IT")
            && grades.contains(&r)
        {
            class = true;
        }
    }
    if class {
        add("DC-G4-114", "tolerance_class");
    }
    // Deviations: stacked fields from the decoder, or a written `upper/lower` pair.
    let pair = |s: &str| -> Option<(String, String)> {
        let s = s.trim_matches(|c| c == '(' || c == ')' || c == '（' || c == '）');
        let (a, b) = s.split_once('/')?;
        (signed_number(a)
            && signed_number(b)
            && (a.starts_with(['+', '-', '−'])
                || b.starts_with(['+', '-', '−'])
                || is_zero(a)
                || is_zero(b)))
        .then(|| (a.to_string(), b.to_string()))
    };
    let mut devs: Vec<(String, String)> = Vec::new();
    if let (Some(u), Some(l)) = (&dec.upper, &dec.lower)
        && (signed_number(u) || signed_number(l))
    {
        devs.push((u.clone(), l.clone()));
    }
    for w in
        text.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '（' || c == '）')
    {
        // also forms glued to a class code: `H7(+0.030/0)` is split above at the bracket
        if let Some(p) = pair(w)
            && !devs.contains(&p)
        {
            devs.push(p);
        }
    }
    if !devs.is_empty() {
        add("DC-G4-132", "deviations");
        if devs.iter().any(|(a, b)| is_zero(a) || is_zero(b)) {
            add("DC-G4-103", "deviations");
        }
        if class {
            add("DC-G4-113", "deviations");
        }
    }
    out
}

/// Strings of a card lexicon entry for a concept, used by the technical-note matcher.
fn note_hit(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| !w.is_empty() && text.contains(w))
}

/// Concepts of one technical-requirement item (`technical_note` card); every item belongs
/// to the technical requirements of the drawing.
pub(crate) fn note_concepts(
    c: &MdRepresentationCard,
    dims: &MdRepresentationCard,
    raw: &str,
) -> Vec<(String, &'static str)> {
    let t = compact(&tx::decode(raw).text);
    let mut out: Vec<(String, &'static str)> = vec![("DC-G9-002".into(), "item_wording")];
    let mut add = |id: &str, v: &'static str| {
        if !out.iter().any(|(x, _)| x == id) {
            out.push((id.to_string(), v));
        }
    };
    for (concept, words) in entries(c, "word") {
        let words: Vec<String> = words.iter().map(|w| compact(w)).collect();
        let ws: Vec<&str> = words.iter().map(String::as_str).collect();
        if note_hit(&t, &ws) {
            add(concept, "item_wording");
        }
    }
    // Words that need a companion (e.g. 其余 with a roughness value).
    for l in c
        .lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == "word_with")
    {
        let (first, second): (Vec<&str>, Vec<&str>) = l
            .values
            .iter()
            .map(String::as_str)
            .partition(|w| !w.starts_with('&'));
        let second: Vec<&str> = second.iter().map(|w| w.trim_start_matches('&')).collect();
        if note_hit(&t, &first) && note_hit(&t, &second) {
            add(&l.concept, "item_wording");
        }
    }
    // Hardness values: a number (or range or toleranced value) followed by a hardness scale.
    let scales = values(c, "DC-G7-089", "hardness_scale");
    let hard = scales.iter().any(|s| {
        t.match_indices(s).any(|(i, _)| {
            let before = t[..i].chars().last();
            before.is_some_and(|b| b.is_ascii_digit() || b == ')' || b == '）')
        })
    });
    if hard {
        add("DC-G7-089", "heat_treatment");
    }
    // Case depth of a hardened layer: CHD, CD followed by a value.
    let depth = values(c, "DC-G7-026", "case_depth");
    if depth.iter().any(|d| {
        t.match_indices(d).any(|(i, _)| {
            t[i + d.len()..]
                .chars()
                .next()
                .is_some_and(|x| x.is_ascii_digit() || x == '=')
        })
    }) {
        add("DC-G7-026", "heat_treatment");
    }
    // Size tokens inside a note (未注倒角C1, 其余圆角R4) carry their own dimension concepts.
    for (id, _) in dimension_concepts(dims, raw) {
        if matches!(
            id.as_str(),
            "DC-G4-040" | "DC-G3-091" | "DC-G4-041" | "DC-G3-080"
        ) {
            add(&id, "item_wording");
        }
    }
    out
}

/// Technical-requirement topic from the card (`topic` entries); general when no topic matches.
pub(crate) fn note_topic(c: &MdRepresentationCard, raw: &str) -> &'static str {
    let t = tx::decode(raw).text;
    for (kind, label) in [
        ("material_heat", "topic:material_heat"),
        ("protection_requirement", "topic:protection_requirement"),
        ("process_requirement", "topic:process_requirement"),
    ] {
        if note_hit(&t, &values(c, "", label)) {
            return kind;
        }
    }
    "technical_general"
}

// ---------------------------------------------------------------- title block fields

fn is_scale_form(s: &str) -> bool {
    tx::is_scale(&compact(s))
}
fn material_form(c: &MdRepresentationCard, s: &str) -> bool {
    let t = compact(&s.replace("%%C", "Ø").replace("%%c", "Ø"));
    // stock words (圆钢, 棒, 板) may precede the grade: the grade is the last token
    let last = s.split_whitespace().last().unwrap_or("").trim();
    let stock = values(c, "DC-G9-022", "stock_word");
    let grade = |g: &str| -> bool {
        let g = g.trim();
        if g.is_empty() || g.contains('.') || !g.is_ascii() {
            return false;
        }
        let prefixes = values(c, "DC-G9-022", "grade_prefix");
        let alloys = values(c, "DC-G9-022", "alloy_element");
        // plain carbon steel number (two digits, e.g. 45, 20, 35)
        if g.len() == 2 && g.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        // alloy steel: two digits then alloying elements (40Cr, 20CrMnTi, 42CrMo)
        if g.len() > 2
            && g[..2].chars().all(|c| c.is_ascii_digit())
            && alloys.iter().any(|a| g[2..].starts_with(a))
        {
            return true;
        }
        // grade prefixes: HT200, QT400-15, ZG230-450, Q235, ZAlSi12, H62
        prefixes.iter().any(|p| {
            g.strip_prefix(p).is_some_and(|r| {
                r.chars().next().is_some_and(|c| {
                    c.is_ascii_digit() || (c.is_ascii_uppercase() && p.starts_with('Z'))
                })
            })
        })
    };
    grade(last) || (stock.iter().any(|w| t.contains(w)) && grade(last)) || grade(&t)
}
fn code_form(s: &str) -> bool {
    let t = compact(s);
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    let seps = t
        .chars()
        .filter(|c| matches!(c, '.' | '-' | '~' | '～' | '/'))
        .count();
    !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '~' | '～' | '/'))
        && digits >= 2
        && (seps >= 1 && t.len() >= 4 || t.chars().any(|c| c.is_ascii_uppercase()) && t.len() >= 6)
        && !tx::is_scale(&t)
        && !(t.len() == 8 && t.chars().all(|c| c.is_ascii_digit()))
}
fn cjk_name(s: &str) -> bool {
    let t = compact(s);
    let n = t.chars().count();
    (1..=10).contains(&n) && t.chars().all(tx::is_cjk)
}

/// Concepts of one title-block text (`title_block_field` card): labels of the zones, and the
/// value forms of the fields (scale, material, drawing code, name, organisation).
pub(crate) fn title_field_concepts(
    c: &MdRepresentationCard,
    raw: &str,
) -> Vec<(String, &'static str)> {
    let text = tx::decode(raw).text;
    let t = compact(&text);
    let mut out: Vec<(String, &'static str)> = Vec::new();
    let mut add = |id: &str, v: &'static str| {
        if !out.iter().any(|(x, _)| x == id) {
            out.push((id.to_string(), v));
        }
    };
    if t.is_empty() {
        return out;
    }
    // Labels: a text equal to a zone label, or a row of labels.
    let mut labelled = false;
    for (concept, labels) in entries(c, "label") {
        let ls: Vec<String> = labels.iter().map(|l| compact(l)).collect();
        let words: Vec<String> = text.split_whitespace().map(compact).collect();
        if ls.contains(&t)
            || (words.len() > 1 && words.iter().filter(|w| ls.contains(w)).count() >= 2)
            || ls
                .iter()
                .any(|l| l.chars().count() >= 3 && t.contains(l.as_str()))
        {
            add(concept, "zone_labels");
            labelled = true;
        }
    }
    // Sheet count `共 n 张 第 m 张`.
    let count = values(c, "DC-G9-028", "sheet_count");
    if count.len() >= 2 && count.iter().all(|w| t.contains(w)) {
        add("DC-G9-028", "zone_labels");
        labelled = true;
    }
    if labelled {
        add("DC-G9-028", "zone_labels");
        return out;
    }
    let unit_words = values(c, "DC-G9-017", "word");
    let fields: &[&str] = if is_scale_form(&t) {
        &["DC-G9-025", "DC-G1-114"]
    } else if unit_words.iter().any(|w| t.contains(w)) {
        &["DC-G9-017"]
    } else if material_form(c, &text) {
        &["DC-G9-022"]
    } else if code_form(&t) {
        &["DC-G9-018"]
    } else if cjk_name(&t) {
        &["DC-G9-019"]
    } else {
        &[]
    };
    for f in fields {
        add(f, "field_values");
    }
    if !fields.is_empty() {
        add("DC-G9-028", "field_values");
    }
    out
}

/// Concepts a text-anchored card finds in one written form (used by tests on the written forms
/// of the concept cards; never on a real drawing).
pub fn text_concepts(card_id: &str, text: &str) -> Result<BTreeSet<String>> {
    let c = representation::card_ref(card_id)?;
    Ok(match card_id {
        "dimension_text" => dimension_concepts(c, text)
            .into_iter()
            .map(|x| x.0)
            .collect(),
        "technical_note" => note_concepts(c, representation::card_ref("dimension_text")?, text)
            .into_iter()
            .map(|x| x.0)
            .collect(),
        "title_block_field" => title_field_concepts(c, text)
            .into_iter()
            .map(|x| x.0)
            .collect(),
        _ => anyhow::bail!("not_a_text_card:{card_id}"),
    })
}

/// Name the concepts found, each under the first of `cards` that lists it.
fn push_concepts(b: &mut Builder, e: usize, card: &str, found: &[(String, &'static str)]) {
    let mut ev = Vec::new();
    for (id, v) in found {
        let owner = [card, "dimension_text"]
            .into_iter()
            .find(|c| representation::card_ref(c).is_ok_and(|x| x.concepts.iter().any(|k| k == id)))
            .unwrap_or(card);
        ev.push(format!("card:{owner}"));
        ev.push(format!("variant:{v}"));
        ev.push(format!("concept:{id}"));
    }
    for x in ev {
        if !b.exprs[e].evidence_ids.contains(&x) {
            b.exprs[e].evidence_ids.push(x);
        }
    }
}

/// Dimension texts of every dimension-family expression name their concepts.
pub(super) fn dimension_texts(b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("dimension_text")?;
    for i in 0..b.exprs.len() {
        let e = &b.exprs[i];
        if e.family != "dimension" || e.kind == "series_table" || e.kind == "center_hole" {
            continue;
        }
        let found = dimension_concepts(c, &e.label);
        if !found.is_empty() {
            push_concepts(b, i, "dimension_text", &found);
        }
    }
    Ok(())
}

/// Technical-requirement items and the heading name their concepts.
pub(super) fn technical_items(b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("technical_note")?;
    let d = representation::card_ref("dimension_text")?;
    for i in 0..b.exprs.len() {
        let e = &b.exprs[i];
        if e.family != "note" {
            continue;
        }
        let found = if e.rule == "technical_requirement_heading" {
            vec![("DC-G9-002".to_string(), "item_wording")]
        } else {
            note_concepts(c, d, &e.label)
        };
        push_concepts(b, i, "technical_note", &found);
    }
    Ok(())
}

// ---------------------------------------------------------------- title block fields on the drawing

/// Fields of each title block: labels inside the block, values written in it (attributes or
/// texts over the block), classified by the card; the repeated drawing code in the frame corner
/// names the drawing-code field.
pub(super) fn title_block_fields(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("title_block_field")?;
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "title_block" {
            continue;
        }
        let mut found: Vec<(String, &'static str)> = Vec::new();
        let mut member_fields: Vec<(usize, String)> = Vec::new();
        let ids: Vec<String> = b.exprs[i].member_ids.clone();
        // printed labels of the block with their positions
        let mut labels: Vec<(String, P, f64)> = Vec::new();
        for mid in &ids {
            if let Some(m) = b.members.iter().find(|m| &m.id == mid)
                && let Some(s) = cx.shapes.get(&m.source_handle)
            {
                for (t, at, h) in &s.texts {
                    if let Some((id, _)) = title_field_concepts(c, t)
                        .into_iter()
                        .find(|(_, v)| *v == "zone_labels")
                    {
                        labels.push((id, *at, *h));
                    }
                }
            }
        }
        // the label printed to the left in the same row, else the one directly above
        let label_of = |p: P, h: f64| -> Option<String> {
            let near = |keep: &dyn Fn(f64, f64, f64) -> bool| {
                labels
                    .iter()
                    .filter(|(_, at, lh)| keep(p[0] - at[0], p[1] - at[1], h.min(*lh)))
                    .min_by(|x, y| dist(x.1, p).total_cmp(&dist(y.1, p)))
                    .map(|x| x.0.clone())
            };
            near(&|dx, dy, k| dy.abs() < 0.8 * k && dx > 0. && dx < 8. * k)
                .or_else(|| near(&|dx, dy, k| dx.abs() < 2.5 * k && dy < 0. && dy > -2.5 * k))
        };
        for mid in &ids {
            let Some(mi) = b.members.iter().position(|m| &m.id == mid) else {
                continue;
            };
            let src = b.members[mi].source_handle.clone();
            let mut texts: Vec<String> = Vec::new();
            if let Some(s) = cx.shapes.get(&src) {
                texts.extend(s.texts.iter().map(|t| t.0.clone()));
            }
            if let Some(t) = cx.texts.get(&src) {
                texts.push(t.dec.text.clone());
            }
            for t in texts {
                let mut f = title_field_concepts(c, &t);
                if let Some(tf) = cx.texts.get(&src)
                    && !f.iter().any(|x| x.1 == "zone_labels")
                {
                    // a value whose form does not decide its field belongs to the label printed next to it
                    let decisive = f.iter().any(|(id, _)| {
                        matches!(
                            id.as_str(),
                            "DC-G9-025" | "DC-G9-017" | "DC-G9-022" | "DC-G9-018"
                        )
                    });
                    if !decisive
                        && let Some(l) = label_of(tf.center, tf.height)
                        && l != "DC-G9-028"
                    {
                        f = vec![(l, "field_values"), ("DC-G9-028".into(), "field_values")];
                    }
                    if let Some((id, _)) = f.iter().find(|(id, _)| id != "DC-G9-028") {
                        member_fields.push((mi, id.clone()));
                    }
                }
                for x in f {
                    if !found.iter().any(|y| y.0 == x.0) {
                        found.push(x);
                    }
                }
            }
        }
        if b.exprs[i].rule == "repeated_document_number" {
            found.push(("DC-G9-018".into(), "repeated_code"));
        }
        if !found.iter().any(|x| x.0 == "DC-G9-028") {
            found.push(("DC-G9-028".into(), "zone_labels"));
        }
        for (mi, id) in member_fields {
            let x = format!("field:{id}");
            if !b.members[mi].evidence_ids.contains(&x) {
                b.members[mi].evidence_ids.push(x);
            }
        }
        push_concepts(b, i, "title_block_field", &found);
    }
    Ok(())
}

// ---------------------------------------------------------------- series table (表格图)

/// A letter symbol standing for a variable dimension: one or two letters, optionally followed
/// by digits (L, b, L1, X1, δ).
fn variable_symbol(s: &str) -> bool {
    let t = compact(s);
    let letters: String = t
        .chars()
        .take_while(|c| c.is_alphabetic() && !tx::is_cjk(*c))
        .collect();
    let rest = &t[letters.len()..];
    (1..=2).contains(&letters.chars().count())
        && rest.chars().all(|c| c.is_ascii_digit())
        && rest.len() <= 2
}
/// The letter symbol of a dimension value: the first token after a special mark, without the
/// brackets of a reference dimension; a symbol may carry its own deviations (`L3 0/-0.5`).
fn dim_symbol(label: &str) -> Option<String> {
    let t: String = label.chars().filter(|c| !matches!(c, '☆' | '★')).collect();
    let first = t
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c| matches!(c, '(' | ')' | '（' | '）'));
    variable_symbol(first).then(|| first.to_string())
}

/// Tables of variable dimensions: a grid of free thin lines (at least three rows and three
/// columns) whose header row names letter symbols used as dimension values in the views, or
/// header words of the card with numeric rows. Grid lines and texts become one series_table
/// expression linked to the dimensions it gives values for.
pub(super) fn series_tables(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("series_table")?;
    let headers: Vec<String> = values(c, "DC-G4-098", "label")
        .iter()
        .map(|w| compact(w))
        .collect();
    let min_rows = representation::param(c, "min_rows")? as usize;
    let min_cols = representation::param(c, "min_columns")? as usize;
    let tol_k = representation::param(c, "join_tolerance")?;
    // free horizontal and vertical lines
    let mut hs: Vec<(String, f64, f64, f64)> = Vec::new();
    let mut vs: Vec<(String, f64, f64, f64)> = Vec::new();
    for (id, (a, q)) in &cx.lines {
        if b.used(id) {
            continue;
        }
        let l = dist(*a, *q);
        if l < 1e-6 {
            continue;
        }
        if (a[1] - q[1]).abs() < 1e-6 * l.max(1.) {
            hs.push((id.clone(), a[0].min(q[0]), a[0].max(q[0]), a[1]));
        } else if (a[0] - q[0]).abs() < 1e-6 * l.max(1.) {
            vs.push((id.clone(), a[1].min(q[1]), a[1].max(q[1]), a[0]));
        }
    }
    let tol = tol_k * cx.scale.max(1e-6);
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut hs_sorted = hs.clone();
    hs_sorted.sort_by(|x, y| (y.2 - y.1).total_cmp(&(x.2 - x.1)));
    for h0 in &hs_sorted {
        if taken.contains(&h0.0) {
            continue;
        }
        // rows: horizontal lines with the same x extent
        let rows: Vec<&(String, f64, f64, f64)> = hs
            .iter()
            .filter(|h| (h.1 - h0.1).abs() < tol && (h.2 - h0.2).abs() < tol)
            .collect();
        let mut ys: Vec<f64> = rows.iter().map(|h| h.3).collect();
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < tol);
        if ys.len() < min_rows + 1 {
            continue;
        }
        let (ylo, yhi) = (ys[0], ys[ys.len() - 1]);
        // columns: vertical lines spanning the full height inside the x extent
        let cols: Vec<&(String, f64, f64, f64)> = vs
            .iter()
            .filter(|v| {
                v.3 > h0.1 - tol
                    && v.3 < h0.2 + tol
                    && (v.1 - ylo).abs() < tol
                    && (v.2 - yhi).abs() < tol
            })
            .collect();
        let mut xs: Vec<f64> = cols.iter().map(|v| v.3).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < tol);
        if xs.len() < min_cols + 1
            || (xs[0] - h0.1).abs() > tol
            || (xs[xs.len() - 1] - h0.2).abs() > tol
        {
            continue;
        }
        let bb = [h0.1, ylo, h0.2, yhi];
        let cell_texts: Vec<&TextFact> = cx
            .texts
            .values()
            .filter(|t| !b.used(&t.id) && inside(t.center, bb, 0.))
            .collect();
        // header row: texts between the top two row lines
        let (top_lo, top_hi) = (ys[ys.len() - 2], ys[ys.len() - 1]);
        let header: Vec<&TextFact> = cell_texts
            .iter()
            .copied()
            .filter(|t| t.center[1] > top_lo && t.center[1] < top_hi)
            .collect();
        let body: Vec<&TextFact> = cell_texts
            .iter()
            .copied()
            .filter(|t| t.center[1] < top_lo)
            .collect();
        let symbols: Vec<String> = header
            .iter()
            .filter(|t| variable_symbol(&t.dec.text))
            .map(|t| compact(&t.dec.text))
            .collect();
        let header_words = header
            .iter()
            .filter(|t| headers.contains(&compact(&t.dec.text)))
            .count();
        let numeric_body = body
            .iter()
            .filter(|t| {
                t.dec
                    .text
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            })
            .count();
        // dimensions labelled with the header symbols
        let mut linked: Vec<(usize, String)> = Vec::new();
        for (j, e) in b.exprs.iter().enumerate() {
            if e.family == "dimension"
                && let Some(s) = dim_symbol(&e.label)
                && symbols.contains(&s)
            {
                linked.push((j, s));
            }
        }
        let variant = if !linked.is_empty() {
            "variables_named_by_dimensions"
        } else if header_words >= 1 && !symbols.is_empty() && numeric_body >= min_rows {
            "header_words_and_numeric_rows"
        } else {
            continue;
        };
        let label = format!("表格图 {}", symbols.join(" "));
        let e = b.expr(
            "series_table",
            &label,
            &format!("card:series_table/{variant}"),
            "candidate",
            vec![],
            vec![
                "card:series_table".into(),
                format!("variant:{variant}"),
                "concept:DC-G4-098".into(),
                format!("grid:{}x{}", ys.len() - 1, xs.len() - 1),
            ],
            vec!["Which variant is manufactured is chosen outside the drawing".into()],
        );
        for v in rows.iter().copied().chain(cols.iter().copied()) {
            taken.insert(v.0.clone());
            b.member(
                e,
                &v.0,
                None,
                [0., 1.],
                "table_line",
                member_line_type(cx.lt(&v.0), "table_line"),
                "candidate",
                vec![],
                vec![],
            );
        }
        for t in cell_texts {
            let part = if header.iter().any(|h| h.id == t.id) {
                "table_header"
            } else {
                "table_value"
            };
            b.member(
                e,
                &t.id,
                None,
                [0., 1.],
                "table_text",
                "text",
                "candidate",
                vec![],
                vec![part.into()],
            );
        }
        let mut seen = BTreeSet::new();
        for (j, s) in linked {
            if seen.insert(j) {
                b.links.push((e, "series_variable_of", j));
                b.exprs[j].evidence_ids.push(format!("series_variable:{s}"));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- theoretically exact dimensions

/// A box around a dimension value (card theoretically_exact_dimension): a line above the value,
/// two sides going down from its ends, and the bottom — often the shoulder the value stands on —
/// joining the sides. Returns (box line ids, rectangle, text id) for free values.
pub(super) fn ted_boxes(cx: &Ctx, b: &Builder) -> Result<Vec<(Vec<String>, [f64; 4], String)>> {
    let c = representation::card_ref("theoretically_exact_dimension")?;
    let (omin, omax) = (
        representation::param(c, "top_offset_min")?,
        representation::param(c, "top_offset_max")?,
    );
    let stol = representation::param(c, "corner_tolerance")?;
    let extra = representation::param(c, "box_width_extra")?;
    let mut out = Vec::new();
    for t in cx.texts.values().filter(|t| {
        !b.used(&t.id)
            && matches!(
                t.lex,
                Lex::Number | Lex::Diameter | Lex::Radius | Lex::Angle
            )
    }) {
        let n = [-t.dir[1], t.dir[0]];
        let h = t.height;
        let tol = stol * h;
        let off = |p: P| dot(sub(p, t.center), n);
        let alg = |p: P| dot(sub(p, t.center), t.dir);
        let free: Vec<(&String, P, P)> = cx
            .lines
            .iter()
            .filter(|(id, (a, z))| {
                !b.used(id) && !cx.structural_style(id) && dist(*a, *z) < 12. * h
            })
            .map(|(id, (a, z))| (id, *a, *z))
            .collect();
        let Some((tid, ta, tc)) = free.iter().find(|(_, a, z)| {
            cross(unit(sub(*z, *a)), t.dir).abs() < 0.03
                && dist(*a, *z) < t.width + extra * h
                && (omin * h..omax * h).contains(&off(*a))
                && (omin * h..omax * h).contains(&off(*z))
                && alg(*a).min(alg(*z)) < -0.4 * t.width
                && alg(*a).max(alg(*z)) > 0.4 * t.width
        }) else {
            continue;
        };
        let side = |end: P| {
            free.iter()
                .find(|(id, a, z)| {
                    *id != *tid
                        && dot(unit(sub(*z, *a)), t.dir).abs() < 0.05
                        && (dist(*a, end) < tol || dist(*z, end) < tol)
                        && off(*a).min(off(*z)) < -0.3 * h
                })
                .map(|(id, a, z)| ((*id).clone(), if off(*a) < off(*z) { *a } else { *z }))
        };
        let (Some((s1, b1)), Some((s2, b2))) = (side(*ta), side(*tc)) else {
            continue;
        };
        let mut ids = vec![(*tid).clone(), s1, s2];
        // bottom: a line through both side ends (it may run on as a shoulder)
        if let Some((bid, _, _)) = free.iter().find(|(id, a, z)| {
            !ids.contains(id) && seg_dist(b1, *a, *z) < tol && seg_dist(b2, *a, *z) < tol
        }) {
            ids.push((*bid).clone());
        }
        let pts = [*ta, *tc, b1, b2];
        if let Some(r) = bbox_of(&pts) {
            out.push((ids, r, t.id.clone()));
        }
    }
    Ok(out)
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

/// Name a boxed value as a theoretically exact dimension (qualifier, card, variant, concept).
pub(super) fn mark_ted(b: &mut Builder, e: usize, variant: &str) {
    if !b.exprs[e]
        .qualifiers
        .iter()
        .any(|q| q == "theoretically_exact")
    {
        b.exprs[e].qualifiers.push("theoretically_exact".into());
        b.exprs[e]
            .evidence_ids
            .push("GB/T 1182 boxed theoretically exact dimension".into());
    }
    for x in [
        "card:theoretically_exact_dimension".to_string(),
        format!("variant:{variant}"),
        "concept:DC-G4-029".to_string(),
    ] {
        if !b.exprs[e].evidence_ids.contains(&x) {
            b.exprs[e].evidence_ids.push(x);
        }
    }
}
