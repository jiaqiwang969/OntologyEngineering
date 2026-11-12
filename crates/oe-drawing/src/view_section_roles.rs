//! Refine geometric components using cut-face intervals and native detail-boundary evidence.
use super::*;

const CARD: &str = "view_section_components";

pub(super) fn state(cx: &Ctx, b: &mut Builder, views: &[MdDrawingView]) -> Result<()> {
    representation::card_ref(CARD)?;
    let index: BTreeMap<_, _> = b.exprs.iter().map(|e| (e.id.as_str(), e)).collect();
    for m in &mut b.members {
        if m.view_relation_role != "component" || m.view_component_kind != "view_geometry" {
            continue;
        }
        let Some(e) = index.get(m.expression_id.as_str()) else {
            continue;
        };
        m.evidence_ids.push(format!("card:{CARD}"));
        if e.rule.starts_with("card:detail_view/boundary_") {
            m.view_component_kind = "detail_boundary".into();
        } else if e.kind == "section_hatch" && m.member_role == "hatch_fill" {
            // A native HATCH carries a region with its pattern; loose strokes carry only lines.
            // Do not duplicate the native entity into invented pattern-line members.
            m.view_component_kind = if cx.kind(&m.source_handle) == "HATCH" {
                "section_region"
            } else {
                "section_hatch"
            }
            .into();
        } else if view_composition::all_boundary(m) {
            m.view_component_kind = "section_boundary".into();
        } else if e.kind == "visible_contour" {
            if !view_composition::boundary_intervals(m).is_empty() {
                // A scalar role cannot describe two disjoint roles on one old interval.
                // Preserve that interval and its boundary receipts, rather than extending them.
                m.view_component_kind = "unknown".into();
                m.unknowns
                    .push("剖面边界仅覆盖成员的部分区间；保留原区间，整段组成角色未知".into());
            } else if m.composition_view_ids.iter().any(|id| {
                views.iter().any(|v| {
                    &v.id == id
                        && matches!(
                            v.representation_kind.as_str(),
                            "section_view" | "cross_section"
                        )
                })
            }) {
                m.view_component_kind = "section_visible_contour".into();
            }
        }
    }
    Ok(())
}
