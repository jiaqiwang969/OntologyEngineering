//! Content that is in the file but never on the sheet (card `not_plotted_content`, DC-G1-101).
//!
//! A CAD layer may be excluded from plotting: the definition-point layer never plots, a layer may
//! have its plot switch off, and a layer that is frozen or switched off is neither shown nor
//! plotted (GB/T 14665-2012 clause 8, GB/T 18229-2000 9.1: layers organise what is shown). What
//! lies on such a layer is read, because the file has it, and marked `not_plotted`:
//!
//! * a curve there is no outline, hidden outline or centre line of the sheet, gives no junction
//!   to the lines that are plotted and is no piece of a structural line; unless an annotation
//!   uses it, it is kept as construction geometry (`construction_reference`);
//! * a part of a dimension or another annotation there stays that part; the expression says in
//!   its unknowns which part is missing on the sheet;
//! * every member whose source lies there carries the evidence id `not_plotted`, and an
//!   expression all of whose members do carries the qualifier `not_plotted`.
use super::*;

const CARD: &str = "not_plotted_content";
/// Evidence id of a member whose source is not on the sheet, and qualifier of an expression all of
/// whose members are not.
pub(super) const MARK: &str = "not_plotted";

/// The layers that never plot, by upper-cased name, with the card variant that says why: the
/// definition-point layer named in the card lexicon (any letter case), a layer whose plot switch
/// (group 290) is off, and a layer that is frozen (group 70 bit 1) or switched off (negative
/// colour number).
pub(super) fn layers(d: &Drawing) -> Result<BTreeMap<String, &'static str>> {
    let c = representation::card_ref(CARD)?;
    let named: Vec<String> = c
        .lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == "definition_point_layer")
        .flat_map(|l| l.values.iter().map(|v| v.to_uppercase()))
        .collect();
    let mut out: BTreeMap<String, &'static str> = named
        .iter()
        .map(|n| (n.clone(), "definition_point_layer"))
        .collect();
    for r in d.records.iter().filter(|r| r.kind == "LAYER") {
        let Some(name) = r.get(2).map(str::to_uppercase) else {
            continue;
        };
        if out.contains_key(&name) {
            continue;
        }
        if r.get(290).is_some_and(|v| v.trim() == "0") {
            out.insert(name, "plot_switch_off");
        } else if int(r, 70) & 1 != 0 || int(r, 62) < 0 {
            out.insert(name, "layer_off_or_frozen");
        }
    }
    Ok(out)
}

/// The card variant by which the layer never plots.
pub(super) fn layer_variant(cx: &Ctx, layer: &str) -> Option<&'static str> {
    cx.unplotted.get(&layer.to_uppercase()).copied()
}

/// The root source lies on a layer that never plots.
pub(super) fn source(cx: &Ctx, id: &str) -> bool {
    cx.style
        .get(id)
        .is_some_and(|s| layer_variant(cx, &s.layer).is_some())
}

/// Ends of plotted root curves (source, end at parameter 1) all of whose incidences are with
/// curves that are not plotted: on the sheet such an end is free.
pub(super) fn free_ends(cx: &Ctx) -> BTreeSet<(String, bool)> {
    if cx.unplotted.is_empty()
        || !cx
            .style
            .values()
            .any(|s| layer_variant(cx, &s.layer).is_some())
    {
        return BTreeSet::new();
    }
    // per end: (meets a plotted curve, meets a curve that is not plotted)
    let mut seen: BTreeMap<(String, bool), (bool, bool)> = BTreeMap::new();
    #[allow(clippy::manual_range_contains)]
    let end = |t: f64| {
        (t < 1e-6)
            .then_some(false)
            .or((t > 1. - 1e-6).then_some(true))
    };
    for r in &cx.src.relations {
        if r.kind != "endpoint_on_curve" {
            continue;
        }
        let Some(to) = r.to_id.as_deref() else {
            continue;
        };
        for (own, t, other) in [
            (r.from_id.as_str(), r.from_parameter, to),
            (to, r.to_parameter, r.from_id.as_str()),
        ] {
            if let Some(e) = t.and_then(end)
                && !source(cx, own)
            {
                let x = seen.entry((own.to_string(), e)).or_default();
                if source(cx, other) {
                    x.1 = true;
                } else {
                    x.0 = true;
                }
            }
        }
    }
    seen.into_iter()
        .filter(|(_, (plotted, not))| *not && !*plotted)
        .map(|(k, _)| k)
        .collect()
}

/// Curves on a layer that never plots which no annotation uses are construction geometry: read,
/// and neither outline nor unexplained (variant by the layer). A curve an annotation uses in part
/// keeps that use; the rest of it is construction.
pub(super) fn construction(cx: &Ctx, b: &mut Builder) -> Result<()> {
    representation::card_ref(CARD)?;
    let ids: Vec<String> = cx
        .rec
        .iter()
        .filter(|(h, r)| is_curve_kind(&r.kind) && source(cx, h))
        .map(|(h, _)| h.clone())
        .collect();
    for h in ids {
        let mut free = Vec::new();
        let mut at = 0.;
        for iv in b.covered(&h) {
            if iv[0] > at + 1e-9 {
                free.push([at, iv[0]]);
            }
            at = f64::max(at, iv[1]);
        }
        if at < 1. - 1e-9 {
            free.push([at, 1.]);
        }
        if free.is_empty() {
            continue;
        }
        let layer = cx.style[&h].layer.clone();
        let Some(variant) = layer_variant(cx, &layer) else {
            continue;
        };
        let e = b.expr(
            "construction_reference",
            "不打印层图线",
            &format!("card:{CARD}/{variant}"),
            "observed",
            vec![],
            vec![
                format!("source:{h}"),
                format!("layer:{layer}"),
                format!("card:{CARD}"),
                format!("variant:{variant}"),
            ],
            vec!["Purpose of the construction geometry is not resolved; it is not on the plotted sheet".into()],
        );
        for iv in free {
            b.member(
                e,
                &h,
                None,
                iv,
                "native_member",
                cx.lt(&h),
                "observed",
                vec![],
                vec![],
            );
        }
    }
    Ok(())
}

/// Mark what is not on the sheet in the finished graph: every member whose source (or whose own
/// nested entity) lies on a layer that never plots carries `not_plotted`; an expression all of
/// whose members do carries the qualifier; an expression some of whose parts do says which.
pub(super) fn mark(cx: &Ctx, b: &mut Builder) {
    if cx.unplotted.is_empty() {
        return;
    }
    let nested: BTreeMap<&str, &str> = cx
        .src
        .members
        .iter()
        .filter(|m| m.block_path.len() > 1)
        .map(|m| (m.id.as_str(), m.source_layer.as_str()))
        .collect();
    let mut state: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for m in &mut b.members {
        let off = source(cx, &m.source_handle)
            || m.native_member_id
                .as_deref()
                .and_then(|n| nested.get(n))
                .is_some_and(|l| layer_variant(cx, l).is_some());
        let s = state.entry(m.expression_id.clone()).or_default();
        if !off {
            s.0 += 1;
            continue;
        }
        if !m.evidence_ids.iter().any(|x| x == MARK) {
            m.evidence_ids.push(MARK.into());
        }
        s.1.push(format!(
            "{} {}",
            m.member_role,
            m.native_member_id.as_deref().unwrap_or(&m.source_handle)
        ));
    }
    for e in &mut b.exprs {
        let Some((plotted, parts)) = state.get(&e.id) else {
            continue;
        };
        if parts.is_empty() {
            continue;
        }
        if *plotted == 0 {
            if !e.qualifiers.iter().any(|q| q == MARK) {
                e.qualifiers.push(MARK.into());
            }
            continue;
        }
        // a definition point is never on the sheet; it is no missing part
        let mut missing: Vec<&String> = parts
            .iter()
            .filter(|p| !p.starts_with("definition_point "))
            .collect();
        missing.sort();
        missing.dedup();
        for p in missing {
            e.unknowns.push(format!(
                "Part not on the plotted sheet (its layer never plots): {p}"
            ));
        }
    }
}
