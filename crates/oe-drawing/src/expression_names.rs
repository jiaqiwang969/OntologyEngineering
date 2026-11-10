//! Labels say what the thing is (contract of the correction round). A rule that has nothing better
//! to call what it found than the handles of its sources (one handle, or several joined with `+`)
//! leaves the name to this pass, which names it by its view and kind, the way line pieces grouped
//! by view are named; a surface texture symbol written without a value is named by what its form
//! requires.
use super::*;

/// Whether a label is made of source handles only.
fn handles_only(label: &str, handles: &BTreeSet<&str>) -> bool {
    !label.is_empty() && label.split('+').all(|p| handles.contains(p.trim()))
}

/// The words a surface texture symbol written without a value is named by, from its form (card
/// `surface_texture_symbol`, variant `symbol_only_named_by_form`; GB/T 131-2006 4.1.2, 4.1.3):
/// the closing bar asks for material removal, the inscribed circle forbids it.
fn texture_words(e: &MdEngineeringExpression) -> Result<Option<String>> {
    let c = representation::card_ref("surface_texture_symbol")?;
    let words = |concept: &str| {
        c.lexicon
            .iter()
            .flatten()
            .find(|l| l.entry == "symbol_only_word" && l.concept == concept)
            .and_then(|l| l.values.first().cloned())
    };
    let has = |id: &str| e.evidence_ids.iter().any(|x| x == &format!("concept:{id}"));
    Ok(if has("DC-G6-063") {
        words("DC-G6-063")
    } else if has("DC-G6-072") {
        words("DC-G6-072")
    } else {
        None
    })
}

pub(super) fn state(cx: &Ctx, b: &mut Builder, regions: &[Region]) -> Result<()> {
    let handles: BTreeSet<&str> = cx.src.entities.iter().map(|e| e.id.as_str()).collect();
    for e in b.exprs.iter_mut() {
        // a texture symbol without a value: its form says what it requires
        if e.kind == "surface_roughness"
            && let Some(rest) = e.label.strip_suffix('√')
            && let Some(w) = texture_words(e)?
        {
            e.label = format!("{}{w}", rest);
            if !e
                .evidence_ids
                .iter()
                .any(|x| x == "variant:symbol_only_named_by_form")
            {
                e.evidence_ids
                    .push("variant:symbol_only_named_by_form".into());
            }
            continue;
        }
        if e.kind == "unresolved" || !handles_only(&e.label, &handles) {
            continue;
        }
        let view = regions
            .iter()
            .find(|r| Some(&r.id) == e.view_id.as_ref())
            .map_or("未定视图".to_string(), view_layout::shown_label);
        e.label = format!("{view} · {}", kind_cn(&e.kind));
    }
    Ok(())
}
