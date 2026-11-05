//! Native structural ownership of source members (evidence only).
use crate::native_readback::{Drawing, Record};
use anyhow::{Result, ensure};
use oe_contracts::*;
use std::collections::BTreeSet;

fn graphical(r: &Record) -> bool {
    !matches!(
        r.kind.as_str(),
        "SECTION" | "ENDSEC" | "BLOCK" | "ENDBLK" | "SEQEND"
    )
}
fn role(r: &Record, dimension: bool) -> &'static str {
    if dimension {
        match r.kind.as_str() {
            "TEXT" | "MTEXT" => "dimension_text",
            "SOLID" | "TRACE" => "dimension_arrow_fill",
            "INSERT" => "dimension_arrow_or_symbol_block",
            "POINT" => "dimension_definition_point",
            _ => "dimension_stroke_member",
        }
    } else {
        "native_member"
    }
}
fn raw_text(r: &Record) -> Option<String> {
    matches!(
        r.kind.as_str(),
        "TEXT" | "MTEXT" | "ATTRIB" | "ATTDEF" | "DIMENSION" | "TOLERANCE"
    )
    .then(|| {
        r.tags
            .iter()
            .filter(|(c, _)| *c == 1 || (r.kind == "MTEXT" && *c == 3))
            .map(|(_, s)| s.as_str())
            .collect::<String>()
    })
    .filter(|s| !s.is_empty())
}
fn collect(
    d: &Drawing,
    r: &Record,
    path: &mut Vec<String>,
    blocks: &mut Vec<String>,
    dimension: bool,
    out: &mut Vec<MdNativeLayerMember>,
) -> Result<()> {
    ensure!(
        path.len() < 16 && out.len() < 30000,
        "native_layers_expansion_budget"
    );
    let h = r.handle()?.to_owned();
    path.push(h.clone());
    out.push(MdNativeLayerMember {
        id: path.join("/"),
        handle: h,
        kind: r.kind.clone(),
        source_layer: r.get(8).unwrap_or("0").into(),
        block_path: path.clone(),
        role: role(r, dimension).into(),
        raw_text: raw_text(r),
    });
    if matches!(r.kind.as_str(), "INSERT" | "DIMENSION") {
        if let Some(block) = r.get(2) {
            ensure!(
                !blocks.iter().any(|b| b == block),
                "native_layers_recursive_block"
            );
            let members: Vec<_> = d
                .records
                .iter()
                .filter(|b| b.section == "BLOCKS" && b.block == block && graphical(b))
                .collect();
            ensure!(
                d.records
                    .iter()
                    .any(|b| b.kind == "BLOCK" && b.block == block),
                "native_layers_missing_block:{block}"
            );
            blocks.push(block.into());
            for m in members {
                collect(d, m, path, blocks, dimension || r.kind == "DIMENSION", out)?;
            }
            blocks.pop();
        } else {
            ensure!(r.kind != "INSERT", "native_layers_insert_block_missing");
        }
    }
    // Attached ATTRIB / VERTEX sequences are not separate top-level drawing objects.
    if r.section == "ENTITIES"
        && (r.kind == "POLYLINE" || (r.kind == "INSERT" && r.get(66) == Some("1")))
    {
        let mut ended = false;
        for child in d.records.iter().skip(r.index + 1) {
            if child.kind == "SEQEND" {
                ended = true;
                break;
            }
            ensure!(
                matches!(child.kind.as_str(), "VERTEX" | "ATTRIB"),
                "native_layers_attached_sequence"
            );
            collect(d, child, path, blocks, dimension, out)?;
        }
        ensure!(ended, "native_layers_unclosed_sequence");
    }
    path.pop();
    Ok(())
}

/// Source containment only, with no supplied engineering classifications.
pub(crate) fn source_members(bytes: &[u8]) -> Result<Vec<MdNativeLayerMember>> {
    let d = Drawing::parse(bytes)?;
    let mut members = Vec::new();
    for root in d.records.iter().filter(|r| {
        r.section == "ENTITIES"
            && graphical(r)
            && !matches!(r.kind.as_str(), "ATTRIB" | "VERTEX")
            && r.get(67).unwrap_or("0") == "0"
    }) {
        collect(&d, root, &mut vec![], &mut vec![], false, &mut members)?;
    }
    ensure!(
        members.len() <= 30000
            && members.iter().map(|m| &m.id).collect::<BTreeSet<_>>().len() == members.len(),
        "usage_source_members"
    );
    Ok(members)
}
