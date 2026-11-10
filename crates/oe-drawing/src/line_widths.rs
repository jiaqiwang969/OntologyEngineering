//! Line widths as the sheet plots them (card `line_width_evidence`; DC-G1-016, DC-G1-043,
//! DC-G1-079, DC-G1-007; GB/T 4457.4-2002 4.1 and table 1, GB/T 14665-2012 4.2).
//!
//! A mechanical drawing has two line widths. Which one a line is drawn with is read from the
//! drawing's own evidence before the layer: its lineweight class when the drawing's lineweights
//! decide, else, when it plots by colour, its colour (a thin colour on the thick-line layer is a
//! thin line, the thick colour on a thin layer a thick one). The layer convention speaks where
//! that evidence is silent, except a text layer, whose table meaning is the stroke of text, not
//! a line width. Recognition keeps reading the layer convention; this module reports the result:
//!
//! * in `line_styles`, the line type each continuous root curve is plotted with;
//! * in member line types: dimension parts, leaders and the other thin annotation roles are thin
//!   unless that evidence says the line is drawn thick; cutting strokes are thick by the same
//!   evidence the outline uses; a member that only records its source (unexplained, coincident,
//!   construction) carries the source's plotted line type.
use super::line_evidence::{Width, Widths};
use super::*;

const CARD: &str = "line_width_evidence";

fn continuous(cx: &Ctx, id: &str) -> bool {
    cx.rec.get(id).is_some_and(|r| is_curve_kind(&r.kind))
        && cx
            .style
            .get(id)
            .is_some_and(|s| matches!(s.pattern, Pattern::Continuous | Pattern::ByBlock))
        && !not_plotted::source(cx, id)
}

/// The table meaning of the source's layer is the stroke of text (GB/T 14665-2012 table 6).
fn text_layer(cx: &Ctx, id: &str) -> bool {
    cx.conv.qualified
        && cx
            .style
            .get(id)
            .and_then(|s| table6(&s.layer))
            .is_some_and(|t| t.1.starts_with("文本"))
}

/// Line type, basis, layer rule and added evidence of a root source in `line_styles`.
pub(super) fn style(
    cx: &Ctx,
    w: &Widths,
    id: &str,
) -> (&'static str, &'static str, Option<String>, Vec<String>) {
    let (lt, basis, rule) = cx.base[id].clone();
    if !continuous(cx, id) {
        return (lt, basis, rule, vec![]);
    }
    let named = |v: &str| vec![format!("card:{CARD}"), format!("variant:{v}")];
    match w.evidence(cx, id) {
        Some((width, v)) => {
            let to = match width {
                Width::Thick => "thick_continuous",
                Width::Thin => "thin_continuous",
            };
            if to == lt {
                return (lt, basis, rule, vec![]);
            }
            let mut ev = named(v);
            ev.push(if v == "lineweight_classes" {
                "width_basis:lineweight".to_string()
            } else {
                format!("width_basis:plot_colour:{}", cx.style[id].plot_colour())
            });
            // the line type is observed on the entity itself (pattern and width), not taken from
            // the layer convention
            (to, "observed_pattern", rule, ev)
        }
        None if text_layer(cx, id) && lt != "continuous" => (
            "continuous",
            basis,
            rule,
            named("text_layer_is_no_line_width"),
        ),
        None => (lt, basis, rule, vec![]),
    }
}

fn lexicon(c: &oe_contracts::MdRepresentationCard, entry: &str) -> BTreeSet<String> {
    c.lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == entry)
        .flat_map(|l| l.values.iter().cloned())
        .collect()
}

/// Member line types from the drawing's own width evidence (see the module comment). Members of
/// nested block entities keep what their block reading gave them.
pub(super) fn members(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref(CARD)?;
    let (thin, thick) = (lexicon(c, "thin_roles"), lexicon(c, "thick_roles"));
    let w = Widths::of(cx);
    for m in b.members.iter_mut() {
        let h = m.source_handle.as_str();
        if m.native_member_id.is_some()
            || !matches!(
                m.line_type.as_str(),
                "continuous" | "thick_continuous" | "thin_continuous"
            )
            || !cx.rec.get(h).is_some_and(|r| is_curve_kind(&r.kind))
        {
            continue;
        }
        let (to, evidence): (&str, Vec<String>) = if thin.contains(&m.member_role) {
            let v = match w.evidence(cx, h).filter(|_| continuous(cx, h)) {
                Some((Width::Thick, v)) => ("thick_continuous", v),
                _ => ("thin_continuous", "annotation_roles_thin"),
            };
            (
                v.0,
                vec![format!("card:{CARD}"), format!("variant:{}", v.1)],
            )
        } else if thick.contains(&m.member_role) {
            if !continuous(cx, h) || !w.thick(cx, h) {
                continue;
            }
            (
                "thick_continuous",
                vec![
                    format!("card:{CARD}"),
                    "variant:cutting_stroke_by_outline_evidence".to_string(),
                ],
            )
        } else if matches!(
            m.member_role.as_str(),
            "unresolved_source" | "coincident_source" | "native_member"
        ) {
            let s = style(cx, &w, h);
            (s.0, s.3)
        } else {
            continue;
        };
        if to == m.line_type {
            continue;
        }
        m.line_type = to.into();
        for x in evidence {
            if !m.evidence_ids.contains(&x) {
                m.evidence_ids.push(x);
            }
        }
    }
    Ok(())
}
