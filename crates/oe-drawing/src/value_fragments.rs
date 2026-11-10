//! The value of a native dimension written in several text fragments (card `dimension_text`,
//! variant `fragments_in_reading_order`; DC-G4-017; GB/T 4458.4-2003 4.1.5, 4.2).
//!
//! A dimension block may write its value in pieces, for example the diameter sign and the number
//! as two texts. The pieces are read in reading order, not in the order the file stores them:
//! line by line from the top, and along the text direction within a line. A piece that is a size
//! symbol alone (Ø, R, S, □ …) is the prefix of the number that follows it.
use super::*;

const CARD: &str = "dimension_text";

/// The text a dimension block writes directly (its TEXT and MTEXT entities), in reading order.
pub(super) fn direct_text(brecs: &[&Record]) -> Result<String> {
    let c = representation::card_ref(CARD)?;
    let row_gap = representation::param(c, "fragment_row_gap")?;
    let lone: BTreeSet<&str> = c
        .lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == "lone_prefix")
        .flat_map(|l| l.values.iter().map(String::as_str))
        .collect();
    let texts: Vec<&Record> = brecs
        .iter()
        .copied()
        .filter(|x| matches!(x.kind.as_str(), "MTEXT" | "TEXT"))
        .collect();
    let raws: Vec<String> = texts.iter().map(|x| dd::raw_text(x)).collect();
    let facts: Option<Vec<TextFact>> = texts.iter().map(|x| text_fact(x)).collect();
    let (Some(facts), true) = (facts, texts.len() > 1) else {
        return Ok(raws.join(" "));
    };
    // reading direction of the first piece; lines from the top, pieces along the direction
    let dir = facts[0].dir;
    let up = [-dir[1], dir[0]];
    let h = facts.iter().map(|f| f.height).fold(0., f64::max);
    let mut idx: Vec<usize> = (0..facts.len()).collect();
    idx.sort_by(|&a, &b| dot(facts[b].center, up).total_cmp(&dot(facts[a].center, up)));
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for i in idx {
        match rows.last_mut() {
            Some(r) if dot(facts[r[0]].center, up) - dot(facts[i].center, up) <= row_gap * h => {
                r.push(i)
            }
            _ => rows.push(vec![i]),
        }
    }
    let mut order: Vec<usize> = Vec::new();
    for mut r in rows {
        r.sort_by(|&a, &b| dot(facts[a].center, dir).total_cmp(&dot(facts[b].center, dir)));
        order.extend(r);
    }
    let mut out = String::new();
    for (k, &i) in order.iter().enumerate() {
        out.push_str(&raws[i]);
        let prefix = lone.contains(facts[i].dec.text.trim());
        if k + 1 < order.len() && !prefix {
            out.push(' ');
        }
    }
    Ok(out)
}
