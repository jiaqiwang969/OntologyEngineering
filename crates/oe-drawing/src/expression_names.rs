//! No expression is named by a source handle (contract of the correction round: labels say what
//! the thing is). A rule that has nothing better to call what it found than the handles of its
//! sources (one handle, or several joined with `+`) leaves the name to this pass, which names it
//! by its view and kind, the way line pieces grouped by view are named.
use super::*;

/// Whether a label is made of source handles only.
fn handles_only(label: &str, handles: &BTreeSet<&str>) -> bool {
    !label.is_empty() && label.split('+').all(|p| handles.contains(p.trim()))
}

pub(super) fn state(cx: &Ctx, b: &mut Builder, regions: &[Region]) {
    let handles: BTreeSet<&str> = cx.src.entities.iter().map(|e| e.id.as_str()).collect();
    for e in b.exprs.iter_mut() {
        if e.kind == "unresolved" || !handles_only(&e.label, &handles) {
            continue;
        }
        let view = regions
            .iter()
            .find(|r| Some(&r.id) == e.view_id.as_ref())
            .map_or("未定视图".to_string(), view_layout::shown_label);
        e.label = format!("{view} · {}", kind_cn(&e.kind));
    }
}
