//! Separate membership in a figure's representation from an annotation's drawing location.
use super::*;

const CARD: &str = "view_member_relations";

fn auxiliary(kind: &str) -> bool {
    kind.starts_with("gps_")
        || matches!(
            kind,
            "dimension_linear"
                | "dimension_diameter"
                | "dimension_radius"
                | "dimension_angle"
                | "dimension_chamfer"
                | "thread_note"
                | "center_hole"
                | "series_table"
                | "geometric_tolerance"
                | "surface_roughness"
                | "datum_symbol"
                | "technical_general"
                | "material_heat"
                | "process_requirement"
                | "protection_requirement"
                | "leader_note"
                | "leader"
                | "sheet_frame"
                | "title_block"
                | "parts_list"
                | "item_number"
        )
}

fn text<'a>(cx: &'a Ctx, m: &MdExpressionMember) -> Option<&'a str> {
    if let Some(path) = &m.native_member_id
        && let Some(p) = cx.blocks.part(path)
        && let annotation_blocks::Geom::Text(t, _) = &p.geom
    {
        return Some(&t.dec.text);
    }
    cx.texts.get(&m.source_handle).map(|t| t.dec.text.as_str())
}

fn mark_role(
    cx: &Ctx,
    e: &MdEngineeringExpression,
    m: &MdExpressionMember,
) -> Option<&'static str> {
    let role = m.member_role.as_str();
    if e.view_mark_kind == "view_name" {
        if role == "scale_text" || text(cx, m).is_some_and(|s| tx::is_scale(&tx::scale_text(s))) {
            return Some("view_scale");
        }
        return matches!(role, "view_title" | "view_letter" | "leader_line")
            .then_some("view_title");
    }
    if e.view_mark_kind == "detail_mark" {
        if role == "detail_circle" && e.qualifiers.iter().any(|q| q == "detail_boundary") {
            return Some("detail_boundary");
        }
        return match role {
            "detail_circle" => Some("detail_circle"),
            "leader_line" | "arrowhead" => Some("detail_leader"),
            "view_letter" => Some("detail_label"),
            "native_block" => Some("detail_indicator"),
            _ => None,
        };
    }
    match role {
        "cutting_stroke" => Some("cutting_location"),
        "view_arrow" | "arrowhead" => Some("projection_direction"),
        "view_letter" => Some("mark_letters"),
        _ => None,
    }
}

fn located(m: &mut MdExpressionMember) {
    m.view_relation_role = "located_only".into();
    m.view_component_kind = "auxiliary_annotation".into();
    m.composition_view_ids.clear();
}

fn component(m: &mut MdExpressionMember, targets: Vec<String>, role: &str) {
    m.view_relation_role = "component".into();
    m.composition_view_ids = targets;
    m.view_component_kind = role.into();
}

pub(super) fn state(cx: &Ctx, b: &mut Builder, views: &[MdDrawingView]) -> Result<()> {
    representation::card_ref(CARD)?;
    let index: BTreeMap<_, _> = b.exprs.iter().map(|e| (e.id.as_str(), e)).collect();
    for m in &mut b.members {
        let Some(e) = index.get(m.expression_id.as_str()).copied() else {
            continue;
        };
        m.evidence_ids.push(format!("card:{CARD}"));
        if e.kind == "unresolved" {
            m.view_relation_role = "unknown".into();
            m.view_component_kind = "unknown".into();
            m.composition_view_ids.clear();
            continue;
        }
        if auxiliary(&e.kind) {
            located(m);
            continue;
        }
        // A target has to exist on the source member's own sheet; the sheet itself is a container.
        let valid = |id: &String| {
            views.iter().any(|v| {
                v.kind != "sheet"
                    && &v.id == id
                    && m.view_id.as_deref().map(view_types::sheet_key)
                        == Some(view_types::sheet_key(id))
            })
        };
        if e.kind == "view_callout" {
            let targets: Vec<_> = e
                .defines_view_ids
                .iter()
                .chain(&e.references_view_ids)
                .filter(|id| valid(id))
                .fold(Vec::new(), |mut out, id| {
                    if !out.contains(id) {
                        out.push(id.clone());
                    }
                    out
                });
            if let Some(role) = mark_role(cx, e, m) {
                if !targets.is_empty() {
                    let inline_scale = role == "view_title"
                        && text(cx, m).is_some_and(|s| {
                            s.split_whitespace().any(tx::is_scale) && !tx::is_scale(s)
                        });
                    component(m, targets, role);
                    if inline_scale {
                        m.evidence_ids
                            .push("view_title_contains_inline_scale".into());
                        m.unknowns
                            .push("名称与比例共用一个原生文字成员；比例未单独计数".into());
                    }
                } else {
                    m.view_relation_role = "unknown".into();
                    m.view_component_kind = "unknown".into();
                    m.unknowns
                        .push("组成目标未知：视图标注尚无确定的定义/指向关系".into());
                }
            } else if m.member_role == "note_text" {
                located(m);
            } else {
                m.unknowns.push("视图标注成员的组成角色未知".into());
            }
        } else if GEOMETRY.contains(&e.kind.as_str()) {
            if let Some(v) = m.view_id.as_ref().filter(|v| valid(v)).cloned() {
                component(m, vec![v], "view_geometry");
            } else {
                m.unknowns
                    .push("图形成员的所在视图未知，不能组成图纸页容器".into());
            }
        } else {
            m.unknowns.push("此表达族未有可采用的视图组成角色".into());
        }
    }
    Ok(())
}
