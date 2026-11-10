//! Bounded, source-bound DXF correction. Unedited source bytes are never reserialized.
//! No rule or expert decision is inferred here: only certain, explicitly identified issues apply.
//! DXF ownership follows Autodesk's BLOCK/BLOCK_RECORD/common-entity group-code reference.
use anyhow::{Context, Result, bail, ensure};
use oe_contracts::*;
use oe_core::sha256;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Debug)]
struct Tag {
    code: i32,
    value: String,
    start: usize,
    end: usize,
    pair: Vec<u8>,
}
#[derive(Clone, Debug)]
struct Record {
    kind: String,
    section: String,
    block: String,
    table: String,
    start: usize,
    end: usize,
    tags: Vec<Tag>,
}
impl Record {
    fn get(&self, c: i32) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.code == c)
            .map(|t| t.value.as_str())
    }
    fn handle(&self) -> Option<&str> {
        self.get(if self.kind == "DIMSTYLE" { 105 } else { 5 })
    }
}
struct Raw<'a> {
    bytes: &'a [u8],
    records: Vec<Record>,
    newline: &'static str,
}
impl<'a> Raw<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self> {
        ensure!(bytes.len() <= 64 * 1024 * 1024, "correction_source_budget");
        // The independent native reader also rejects malformed encodings and record structure.
        crate::native_readback::Drawing::parse(bytes)?;
        let lines: Vec<_> = bytes
            .split_inclusive(|c| *c == b'\n')
            .scan(0, |p, s| {
                let a = *p;
                *p += s.len();
                Some((a, s))
            })
            .collect();
        ensure!(lines.len().is_multiple_of(2), "correction_pair_count");
        let mut records: Vec<Record> = Vec::new();
        let (mut section, mut block, mut table) = (String::new(), String::new(), String::new());
        for pair in lines.chunks_exact(2) {
            let code = std::str::from_utf8(pair[0].1)?.trim().parse::<i32>()?;
            let value = String::from_utf8_lossy(pair[1].1).trim().to_owned();
            if code == 0 {
                if let Some(r) = records.last_mut() {
                    r.end = pair[0].0;
                }
                if value == "SECTION" {
                    section.clear();
                }
                if value == "BLOCK" {
                    block.clear();
                }
                records.push(Record {
                    kind: value.clone(),
                    section: section.clone(),
                    block: block.clone(),
                    table: table.clone(),
                    start: pair[0].0,
                    end: bytes.len(),
                    tags: Vec::new(),
                });
            }
            let r = records.last_mut().context("correction_record_start")?;
            if code == 2 {
                match r.kind.as_str() {
                    "SECTION" => {
                        section = value.clone();
                        r.section = section.clone();
                    }
                    "BLOCK" => {
                        block = value.clone();
                        r.block = block.clone();
                    }
                    "TABLE" => {
                        table = value.clone();
                        r.table = table.clone();
                    }
                    _ => {}
                }
            }
            let s = pair[1].1;
            let left = s
                .iter()
                .position(|c| !c.is_ascii_whitespace())
                .unwrap_or(s.len());
            let right = s
                .iter()
                .rposition(|c| !c.is_ascii_whitespace())
                .map_or(left, |i| i + 1);
            r.tags.push(Tag {
                code,
                value,
                start: pair[1].0 + left,
                end: pair[1].0 + right,
                pair: bytes[pair[0].0..pair[1].0 + pair[1].1.len()].to_vec(),
            });
            if code == 0 {
                match r.kind.as_str() {
                    "ENDSEC" => section.clear(),
                    "ENDBLK" => block.clear(),
                    "ENDTAB" => table.clear(),
                    _ => {}
                }
            }
        }
        let r = Self {
            bytes,
            records,
            newline: if bytes.windows(2).any(|s| s == b"\r\n") {
                "\r\n"
            } else {
                "\n"
            },
        };
        r.handles()?;
        Ok(r)
    }
    fn handles(&self) -> Result<BTreeSet<u64>> {
        let mut h = BTreeSet::new();
        for r in &self.records {
            if r.section == "HEADER" {
                continue;
            }
            if let Some(v) = r.handle() {
                let n = u64::from_str_radix(v, 16).context("correction_handle_not_hex")?;
                ensure!(n > 0 && h.insert(n), "correction_duplicate_handle:{v}");
            }
        }
        Ok(h)
    }
    fn entity(&self, h: &str, section: &str, block: &str) -> Result<usize> {
        let rs: Vec<_> = self
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.section == section
                    && r.block == block
                    && r.handle().is_some_and(|v| v.eq_ignore_ascii_case(h))
            })
            .map(|(i, _)| i)
            .collect();
        ensure!(
            rs.len() == 1,
            "correction_occurrence_missing_or_ambiguous:{h}"
        );
        Ok(rs[0])
    }
    fn section_end(&self, section: &str) -> Result<usize> {
        let rs: Vec<_> = self
            .records
            .iter()
            .filter(|r| r.section == section && r.kind == "ENDSEC")
            .collect();
        ensure!(
            rs.len() == 1,
            "correction_section_missing_or_ambiguous:{section}"
        );
        Ok(rs[0].start)
    }
}
#[derive(Clone)]
struct Patch {
    start: usize,
    end: usize,
    bytes: Vec<u8>,
    issue: String,
}
fn splice(source: &[u8], changes: &[Patch]) -> Result<Vec<u8>> {
    let mut sorted = changes.to_vec();
    sorted.sort_by_key(|p| p.start);
    let mut cursor = 0;
    let mut out = Vec::new();
    for p in sorted {
        ensure!(
            cursor <= p.start && p.start <= p.end && p.end <= source.len(),
            "correction_overlapping_or_invalid_ranges"
        );
        out.extend_from_slice(&source[cursor..p.start]);
        out.extend(p.bytes);
        cursor = p.end;
    }
    out.extend_from_slice(&source[cursor..]);
    Ok(out)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    ensure!(s.is_ascii() && s.len().is_multiple_of(2), "correction_hex");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).context("correction_hex"))
        .collect()
}
fn safe(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty() && !s.contains(['\n', '\r', '\0']),
        "correction_invalid_field"
    );
    Ok(())
}
fn dxf_text(s: &str) -> Result<String> {
    safe(s)?;
    Ok(s.encode_utf16()
        .map(|c| {
            if (32..127).contains(&c) && c != 92 && c != 37 {
                char::from_u32(c as u32).unwrap().to_string()
            } else {
                format!("\\U+{c:04X}")
            }
        })
        .collect())
}
fn record_edit(raw: &Raw<'_>, r: &Record, values: &BTreeMap<i32, String>) -> Result<Vec<u8>> {
    let mut ps = Vec::new();
    let mut add = String::new();
    for (c, v) in values {
        safe(v)?;
        let tags: Vec<_> = r.tags.iter().filter(|t| t.code == *c).collect();
        ensure!(tags.len() <= 1, "correction_repeated_field:{c}");
        if let Some(t) = tags.first() {
            ps.push(Patch {
                start: t.start - r.start,
                end: t.end - r.start,
                bytes: v.as_bytes().to_vec(),
                issue: String::new(),
            });
        } else {
            add.push_str(&format!("{c}{}{v}{}", raw.newline, raw.newline));
        }
    }
    if !add.is_empty() {
        let at = r.tags.iter().find(|t| t.code == 1001).map_or(r.end, |t| {
            raw.bytes[..t.start]
                .iter()
                .rposition(|c| *c == b'\n')
                .unwrap_or(t.start)
        });
        ensure!(at == r.end, "correction_cannot_insert_before_xdata");
        ps.push(Patch {
            start: at - r.start,
            end: at - r.start,
            bytes: add.into_bytes(),
            issue: String::new(),
        });
    }
    splice(&raw.bytes[r.start..r.end], &ps)
}
struct Editor<'a> {
    raw: Raw<'a>,
    next: u64,
    fresh: Vec<String>,
    changes: Vec<Patch>,
    blocks: Vec<u8>,
    block_records: Vec<u8>,
    added_blocks: usize,
    applied: Vec<MdCorrectionApplied>,
}
impl Editor<'_> {
    fn handle(&mut self) -> Result<String> {
        let h = format!("{:X}", self.next);
        self.next = self
            .next
            .checked_add(1)
            .context("correction_handle_space_exhausted")?;
        self.fresh.push(h.clone());
        Ok(h)
    }
    fn leaf(
        &mut self,
        index: usize,
        op: &MdDrawingCorrectionOperation,
        mut replacements: BTreeMap<i32, String>,
    ) -> Result<Vec<u8>> {
        let r = &self.raw.records[index];
        let expected = op
            .expected_record_sha256
            .as_ref()
            .context("correction_record_precondition_required")?;
        ensure!(
            sha256(&self.raw.bytes[r.start..r.end]) == *expected,
            "correction_record_precondition"
        );
        ensure!(
            !r.tags.iter().any(|t| [102, 360].contains(&t.code)),
            "correction_attached_object_unsupported"
        );
        let result_handle = replacements
            .get(&5)
            .cloned()
            .or_else(|| r.handle().map(String::from))
            .context("correction_entity_handle")?;
        match op.kind.as_str() {
            "set_style" => {
                ensure!(!op.style.is_empty(), "correction_empty_style");
                for edit in &op.style {
                    let c = i32::try_from(edit.code)?;
                    ensure!(
                        [8, 6, 62, 370, 420, 430, 48, 60].contains(&c),
                        "correction_style_field_forbidden"
                    );
                    safe(&edit.value)?;
                    ensure!(
                        edit.value.is_ascii(),
                        "correction_style_encoding_unsupported"
                    );
                    if c == 8 {
                        ensure!(
                            edit.value == "0"
                                || self
                                    .raw
                                    .records
                                    .iter()
                                    .any(|r| r.kind == "LAYER"
                                        && r.get(2) == Some(edit.value.as_str())),
                            "correction_layer_missing"
                        );
                    }
                    if c == 6 {
                        ensure!(
                            ["BYLAYER", "BYBLOCK", "CONTINUOUS"].contains(&edit.value.as_str())
                                || self
                                    .raw
                                    .records
                                    .iter()
                                    .any(|r| r.kind == "LTYPE"
                                        && r.get(2) == Some(edit.value.as_str())),
                            "correction_linetype_missing"
                        );
                    }
                    if [62, 370, 420, 60].contains(&c) {
                        let v = edit.value.parse::<i64>()?;
                        ensure!(
                            match c {
                                62 => (0..=256).contains(&v),
                                370 => (-3..=211).contains(&v),
                                420 => (0..=16777215).contains(&v),
                                60 => [0, 1].contains(&v),
                                _ => false,
                            },
                            "correction_style_value"
                        );
                    }
                    if c == 48 {
                        ensure!(
                            edit.value
                                .parse::<f64>()
                                .is_ok_and(|v| v.is_finite() && v > 0.),
                            "correction_style_scale"
                        );
                    }
                    ensure!(
                        replacements.insert(c, edit.value.clone()).is_none(),
                        "correction_duplicate_style_field"
                    );
                }
            }
            "move" => {
                ensure!(
                    op.translation.len() == 2 && op.translation.iter().all(|n| n.is_finite()),
                    "correction_translation"
                );
                ensure!(
                    r.get(210).is_none_or(|v| v == "0")
                        && r.get(220).is_none_or(|v| v == "0")
                        && r.get(230).is_none_or(|v| v == "1"),
                    "correction_nonplanar_move"
                );
                let points: &[i32] = match r.kind.as_str() {
                    "LINE" => &[10, 11],
                    "TEXT"
                        if r.get(72).is_some_and(|v| v != "0")
                            || r.get(73).is_some_and(|v| v != "0") =>
                    {
                        &[10, 11]
                    }
                    "TEXT" | "MTEXT" | "CIRCLE" | "ARC" | "POINT" | "INSERT" => &[10],
                    "SOLID" => &[10, 11, 12, 13],
                    _ => bail!("correction_move_kind_unsupported:{}", r.kind),
                };
                for c in points {
                    for (code, shift) in [(*c, op.translation[0]), (*c + 10, op.translation[1])] {
                        let n = r
                            .get(code)
                            .context("correction_move_point_missing")?
                            .parse::<f64>()?;
                        ensure!((n + shift).is_finite(), "correction_nonfinite");
                        replacements.insert(code, (n + shift).to_string());
                    }
                }
            }
            "delete" => {
                ensure!(
                    !["POLYLINE", "VERTEX", "SEQEND", "ATTRIB", "BLOCK", "ENDBLK"]
                        .contains(&r.kind.as_str())
                        && r.get(66).is_none_or(|v| v == "0"),
                    "correction_compound_delete_unsupported"
                );
                let h = r.handle().context("correction_entity_handle")?;
                ensure!(
                    !self.raw.records.iter().enumerate().any(|(i, r)| i != index
                        && r.tags
                            .iter()
                            .any(|t| ((320..=369).contains(&t.code) || t.code == 1005)
                                && t.value.eq_ignore_ascii_case(h))),
                    "correction_delete_referenced_entity"
                );
                self.applied.push(MdCorrectionApplied {
                    operation_id: op.id.clone(),
                    issue_id: op.issue_id.clone(),
                    source_occurrence: op.occurrence.clone(),
                    result_handles: vec![],
                });
                return Ok(vec![]);
            }
            _ => bail!("correction_operation_kind"),
        }
        let out = record_edit(&self.raw, r, &replacements)?;
        self.applied.push(MdCorrectionApplied {
            operation_id: op.id.clone(),
            issue_id: op.issue_id.clone(),
            source_occurrence: op.occurrence.clone(),
            result_handles: vec![result_handle],
        });
        Ok(out)
    }
    // Clone every definition on a selected occurrence path once. Other INSERTs retain their block name.
    fn branch(
        &mut self,
        index: usize,
        depth: usize,
        ops: &[&MdDrawingCorrectionOperation],
        mut values: BTreeMap<i32, String>,
    ) -> Result<Vec<u8>> {
        ensure!(depth < 32, "correction_occurrence_depth");
        if ops[0].occurrence.len() == depth + 1 {
            ensure!(ops.len() == 1, "correction_conflicting_operations");
            return self.leaf(index, ops[0], values);
        }
        ensure!(
            ops.iter().all(|o| o.occurrence.len() > depth + 1),
            "correction_parent_child_conflict"
        );
        let insert = &self.raw.records[index];
        ensure!(
            insert.kind == "INSERT" && insert.get(66).is_none_or(|v| v == "0"),
            "correction_occurrence_requires_plain_insert"
        );
        ensure!(
            insert.tags.iter().all(|t| ![102, 360].contains(&t.code)),
            "correction_insert_attached_object"
        );
        let name = insert.get(2).context("correction_block_name")?.to_owned();
        let ids: Vec<usize> = self
            .raw
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.section == "BLOCKS" && r.block == name)
            .map(|(i, _)| i)
            .collect();
        ensure!(
            ids.first()
                .is_some_and(|i| self.raw.records[*i].kind == "BLOCK")
                && ids
                    .last()
                    .is_some_and(|i| self.raw.records[*i].kind == "ENDBLK"),
            "correction_block_bounds"
        );
        let brs: Vec<_> = self
            .raw
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == "BLOCK_RECORD" && r.get(2) == Some(name.as_str()))
            .map(|(i, _)| i)
            .collect();
        ensure!(
            brs.len() == 1,
            "correction_block_record_missing_or_ambiguous"
        );
        let bri = brs[0];
        let br = self.handle()?;
        let new_name = format!("OE_CORR_{br}");
        ensure!(
            !self
                .raw
                .records
                .iter()
                .any(|r| r.kind == "BLOCK" && r.get(2) == Some(new_name.as_str())),
            "correction_block_name_collision"
        );
        let old_br = self.raw.records[bri]
            .handle()
            .context("correction_block_record_handle")?
            .to_owned();
        let mut mapping = BTreeMap::from([(old_br.to_ascii_uppercase(), br.clone())]);
        for i in &ids {
            let r = &self.raw.records[*i];
            ensure!(
                [
                    "BLOCK",
                    "ENDBLK",
                    "LINE",
                    "TEXT",
                    "MTEXT",
                    "CIRCLE",
                    "ARC",
                    "POINT",
                    "SOLID",
                    "LWPOLYLINE",
                    "INSERT"
                ]
                .contains(&r.kind.as_str()),
                "correction_clone_kind_unsupported:{}",
                r.kind
            );
            ensure!(
                r.tags.iter().all(|t| ![102, 360, 1001].contains(&t.code)),
                "correction_clone_attached_data_unsupported"
            );
            let h = r
                .handle()
                .context("correction_clone_handle_missing")?
                .to_owned();
            let fresh = self.handle()?;
            mapping.insert(h.to_ascii_uppercase(), fresh);
        }
        let mut grouped: BTreeMap<usize, Vec<&MdDrawingCorrectionOperation>> = BTreeMap::new();
        for op in ops {
            let j = self
                .raw
                .entity(&op.occurrence[depth + 1], "BLOCKS", &name)?;
            grouped.entry(j).or_default().push(op);
        }
        let mut cloned = Vec::new();
        for i in ids {
            let r = &self.raw.records[i];
            let mut replace = BTreeMap::new();
            for t in &r.tags {
                if (t.code == 5 || (320..=369).contains(&t.code) || t.code == 1005)
                    && let Some(v) = mapping.get(&t.value.to_ascii_uppercase())
                {
                    ensure!(
                        replace.insert(t.code, v.clone()).is_none(),
                        "correction_clone_repeated_pointer"
                    );
                }
            }
            if r.kind == "BLOCK" {
                ensure!(
                    r.get(70).is_none_or(|s| s == "0" || s == "1"),
                    "correction_external_block_unsupported"
                );
                replace.insert(2, new_name.clone());
                if r.get(3).is_some() {
                    replace.insert(3, new_name.clone());
                }
                replace.insert(70, "0".into());
            }
            let bytes = if let Some(ops) = grouped.get(&i) {
                self.branch(i, depth + 1, ops, replace)?
            } else {
                record_edit(&self.raw, &self.raw.records[i], &replace)?
            };
            cloned.extend(bytes);
        }
        let r = &self.raw.records[bri];
        ensure!(
            r.get(340).is_none_or(|v| v == "0")
                && r.tags.iter().all(|t| ![102, 360, 1001].contains(&t.code)),
            "correction_block_record_attached_data"
        );
        self.block_records.extend(record_edit(
            &self.raw,
            r,
            &BTreeMap::from([(5, br), (2, new_name.clone())]),
        )?);
        self.blocks.extend(cloned);
        self.added_blocks += 1;
        values.insert(2, new_name);
        record_edit(&self.raw, &self.raw.records[index], &values)
    }
}

/// Hash the exact selected native record, including its original whitespace, for a plan precondition.
pub fn record_sha256(bytes: &[u8], occurrence: &[String]) -> Result<String> {
    let raw = Raw::parse(bytes)?;
    let mut section = "ENTITIES";
    let mut block = String::new();
    let mut found = None;
    for (depth, h) in occurrence.iter().enumerate() {
        let i = raw.entity(h, section, &block)?;
        let r = &raw.records[i];
        found = Some(i);
        if depth + 1 < occurrence.len() {
            ensure!(r.kind == "INSERT", "correction_occurrence_parent");
            block = r.get(2).context("correction_occurrence_block")?.to_owned();
            section = "BLOCKS";
        }
    }
    let r = &raw.records[found.context("correction_occurrence_empty")?];
    Ok(sha256(&bytes[r.start..r.end]))
}

/// Produce bytes and a receipt in memory. Failure produces neither a partial drawing nor a success receipt.
pub fn rewrite(
    source: &[u8],
    plan: &MdDrawingCorrectionPlan,
) -> Result<(Vec<u8>, MdDrawingCorrectionReceipt)> {
    ensure!(
        plan.validate().is_ok() && sha256(source) == plan.source_sha256,
        "correction_source_precondition"
    );
    let raw = Raw::parse(source)?;
    let used = raw.handles()?;
    let mut next = used
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .context("correction_handle_space_exhausted")?;
    let seed = raw
        .records
        .iter()
        .find(|r| r.section == "HEADER")
        .and_then(|r| {
            r.tags
                .windows(2)
                .find(|t| t[0].value == "$HANDSEED")
                .map(|t| t[1].clone())
        });
    if let Some(t) = &seed {
        ensure!(t.code == 5, "correction_handseed_code");
        next = next.max(u64::from_str_radix(&t.value, 16)?);
    }
    let issues: BTreeMap<_, _> = plan.issues.iter().map(|i| (i.id.as_str(), i)).collect();
    ensure!(
        issues.len() == plan.issues.len()
            && plan.issues.iter().all(|i| i.validate().is_ok()
                && !i.id.is_empty()
                && !i.rule.is_empty()
                && !i.clause.is_empty()),
        "correction_issue_contract"
    );
    let mut editor = Editor {
        raw,
        next,
        fresh: vec![],
        changes: vec![],
        blocks: vec![],
        block_records: vec![],
        added_blocks: 0,
        applied: vec![],
    };
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut groups: BTreeMap<usize, Vec<&MdDrawingCorrectionOperation>> = BTreeMap::new();
    let mut additions = Vec::new();
    for op in &plan.operations {
        ensure!(
            op.validate().is_ok() && !op.id.is_empty() && ids.insert(&op.id),
            "correction_operation_contract"
        );
        let issue = issues
            .get(op.issue_id.as_str())
            .context("correction_issue_missing")?;
        ensure!(
            issue.confidence == "certain",
            "correction_uncertain_issue_refused"
        );
        if op.kind == "add_text" {
            ensure!(
                op.occurrence.is_empty()
                    && op.expected_record_sha256.is_none()
                    && op.style.is_empty()
                    && op.translation.is_empty(),
                "correction_add_fields"
            );
            let t = op.text.as_ref().context("correction_text_missing")?;
            ensure!(
                t.position.len() == 2
                    && t.position.iter().all(|n| n.is_finite())
                    && t.height.is_finite()
                    && t.height > 0.
                    && t.rotation.is_finite(),
                "correction_text_geometry"
            );
            safe(&t.layer)?;
            ensure!(t.layer.is_ascii(), "correction_layer_encoding");
            ensure!(
                t.layer == "0"
                    || editor
                        .raw
                        .records
                        .iter()
                        .any(|r| r.kind == "LAYER" && r.get(2) == Some(t.layer.as_str())),
                "correction_layer_missing"
            );
            let h = editor.handle()?;
            let owner = editor
                .raw
                .records
                .iter()
                .find(|r| {
                    r.kind == "BLOCK_RECORD"
                        && r.get(2)
                            .is_some_and(|s| s.eq_ignore_ascii_case("*Model_Space"))
                })
                .and_then(|r| r.handle())
                .or_else(|| {
                    editor
                        .raw
                        .records
                        .iter()
                        .find(|r| r.section == "ENTITIES" && r.handle().is_some())
                        .and_then(|r| r.get(330))
                })
                .context("correction_model_owner_missing")?;
            ensure!(
                u64::from_str_radix(owner, 16).is_ok_and(|h| used.contains(&h)),
                "correction_model_owner_unresolved"
            );
            let value = dxf_text(&t.value)?;
            let n = editor.raw.newline;
            let text = format!(
                "0{n}TEXT{n}5{n}{h}{n}330{n}{owner}{n}100{n}AcDbEntity{n}8{n}{}{n}100{n}AcDbText{n}10{n}{}{n}20{n}{}{n}30{n}0{n}40{n}{}{n}1{n}{value}{n}50{n}{}{n}100{n}AcDbText{n}",
                t.layer, t.position[0], t.position[1], t.height, t.rotation
            );
            additions.extend(text.as_bytes());
            editor.applied.push(MdCorrectionApplied {
                operation_id: op.id.clone(),
                issue_id: op.issue_id.clone(),
                source_occurrence: vec![],
                result_handles: vec![h],
            });
        } else {
            ensure!(
                !op.occurrence.is_empty()
                    && paths.insert(op.occurrence.clone())
                    && op.text.is_none(),
                "correction_duplicate_or_empty_occurrence"
            );
            ensure!(
                (op.kind == "move" || op.translation.is_empty())
                    && (op.kind == "set_style" || op.style.is_empty()),
                "correction_extraneous_fields"
            );
            let i = editor.raw.entity(&op.occurrence[0], "ENTITIES", "")?;
            groups.entry(i).or_default().push(op);
        }
    }
    for (i, ops) in groups {
        let data = editor.branch(i, 0, &ops, BTreeMap::new())?;
        let r = &editor.raw.records[i];
        editor.changes.push(Patch {
            start: r.start,
            end: r.end,
            bytes: data,
            issue: ops[0].issue_id.clone(),
        });
    }
    let issue = plan
        .operations
        .first()
        .map(|o| o.issue_id.clone())
        .unwrap_or_default();
    if !additions.is_empty() {
        let at = editor.raw.section_end("ENTITIES")?;
        editor.changes.push(Patch {
            start: at,
            end: at,
            bytes: additions,
            issue: issue.clone(),
        });
    }
    if editor.added_blocks > 0 {
        let at = editor.raw.section_end("BLOCKS")?;
        editor.changes.push(Patch {
            start: at,
            end: at,
            bytes: editor.blocks,
            issue: issue.clone(),
        });
        let rs: Vec<_> = editor
            .raw
            .records
            .iter()
            .filter(|r| r.kind == "ENDTAB" && r.table == "BLOCK_RECORD")
            .collect();
        ensure!(rs.len() == 1, "correction_block_table_end");
        let at = rs[0].start;
        editor.changes.push(Patch {
            start: at,
            end: at,
            bytes: editor.block_records,
            issue: issue.clone(),
        });
        let table = editor
            .raw
            .records
            .iter()
            .find(|r| r.kind == "TABLE" && r.get(2) == Some("BLOCK_RECORD"))
            .context("correction_block_table")?;
        if let Some(t) = table.tags.iter().find(|t| t.code == 70) {
            let count = t
                .value
                .parse::<usize>()?
                .checked_add(editor.added_blocks)
                .context("correction_table_count")?;
            editor.changes.push(Patch {
                start: t.start,
                end: t.end,
                bytes: count.to_string().into_bytes(),
                issue: issue.clone(),
            });
        }
    }
    if !editor.fresh.is_empty()
        && let Some(t) = seed
    {
        editor.changes.push(Patch {
            start: t.start,
            end: t.end,
            bytes: format!("{:X}", editor.next).into_bytes(),
            issue: issue.clone(),
        });
    }
    editor.changes.sort_by_key(|p| p.start);
    let corrected = splice(source, &editor.changes)?;
    let new = Raw::parse(&corrected)?;
    let actual_new: BTreeSet<_> = new.handles()?.difference(&used).copied().collect();
    let declared: BTreeSet<_> = editor
        .fresh
        .iter()
        .map(|s| u64::from_str_radix(s, 16))
        .collect::<std::result::Result<_, _>>()?;
    // A clone's deleted leaf receives no live handle. Allocation is still monotonic; receipt names only live additions.
    let live_fresh: Vec<_> = editor
        .fresh
        .into_iter()
        .filter(|h| actual_new.contains(&u64::from_str_radix(h, 16).unwrap()))
        .collect();
    ensure!(
        actual_new.is_subset(&declared),
        "correction_undeclared_handle"
    );
    let changed: BTreeSet<_> = editor.applied.iter().map(|a| a.issue_id.as_str()).collect();
    let receipt = MdDrawingCorrectionReceipt {
        source_sha256: sha256(source),
        corrected_sha256: sha256(&corrected),
        source_path: None,
        corrected_path: None,
        remaining_issue_ids: plan
            .issues
            .iter()
            .filter(|i| !changed.contains(i.id.as_str()))
            .map(|i| i.id.clone())
            .collect(),
        applied: editor.applied,
        changes: editor
            .changes
            .into_iter()
            .map(|p| MdDxfByteChange {
                start: p.start as u64,
                end: p.end as u64,
                replacement_hex: hex(&p.bytes),
                issue_id: p.issue,
            })
            .collect(),
        new_handles: live_fresh,
        checks: vec![],
    };
    verify(source, &corrected, plan, &receipt)?;
    Ok((corrected, receipt))
}

fn same_tags_except(a: &Record, b: &Record, allowed: &[i32]) -> bool {
    let tags = |r: &Record| {
        r.tags
            .iter()
            .filter(|t| !allowed.contains(&t.code))
            .map(|t| (t.code, t.pair.clone()))
            .collect::<Vec<_>>()
    };
    tags(a) == tags(b)
}

/// Re-read both occurrence trees independently of the editor: cloned siblings keep their fields and order.
fn audit_occurrence(
    sources: (&Raw<'_>, &Raw<'_>),
    old: &Record,
    new: &Record,
    depth: usize,
    ops: &[&MdDrawingCorrectionOperation],
    receipt: &MdDrawingCorrectionReceipt,
    fresh: &mut BTreeSet<u64>,
) -> Result<()> {
    let (a, b) = sources;
    ensure!(depth < 32, "correction_audit_depth");
    ensure!(
        !ops.is_empty() && ops.iter().all(|o| o.occurrence.len() > depth),
        "correction_audit_path"
    );
    if ops[0].occurrence.len() == depth + 1 {
        ensure!(
            ops.len() == 1 && ops[0].kind != "delete",
            "correction_audit_leaf"
        );
        let applied = receipt
            .applied
            .iter()
            .find(|x| x.operation_id == ops[0].id)
            .context("correction_audit_applied")?;
        ensure!(
            applied.result_handles.first().map(String::as_str) == new.handle(),
            "correction_result_not_at_selected_occurrence"
        );
        return Ok(());
    }
    ensure!(
        ops.iter().all(|o| o.occurrence.len() > depth + 1),
        "correction_audit_parent_child_conflict"
    );
    ensure!(
        old.kind == "INSERT"
            && new.kind == "INSERT"
            && same_tags_except(old, new, if depth == 0 { &[2] } else { &[2, 5, 330] }),
        "correction_parent_fields_changed"
    );
    let old_name = old.get(2).context("correction_audit_old_block")?;
    let name = new.get(2).context("correction_audit_new_block")?;
    ensure!(
        old_name != name
            && !a
                .records
                .iter()
                .any(|r| r.kind == "BLOCK" && r.get(2) == Some(name)),
        "correction_clone_not_new"
    );
    let old_br = a
        .records
        .iter()
        .find(|r| r.kind == "BLOCK_RECORD" && r.get(2) == Some(old_name))
        .context("correction_audit_old_owner")?;
    let brs: Vec<_> = b
        .records
        .iter()
        .filter(|r| r.kind == "BLOCK_RECORD" && r.get(2) == Some(name))
        .collect();
    ensure!(brs.len() == 1, "correction_audit_new_owner");
    let br = brs[0];
    ensure!(
        same_tags_except(old_br, br, &[2, 5]),
        "correction_clone_table_fields"
    );
    let owner = br.handle().context("correction_clone_owner_handle")?;
    fresh.insert(u64::from_str_radix(owner, 16)?);
    let old_rows: Vec<_> = a
        .records
        .iter()
        .filter(|r| r.section == "BLOCKS" && r.block == old_name)
        .collect();
    let new_rows: Vec<_> = b
        .records
        .iter()
        .filter(|r| r.section == "BLOCKS" && r.block == name)
        .collect();
    let mut pos = 0;
    for row in old_rows {
        let here: Vec<_> = ops
            .iter()
            .copied()
            .filter(|op| {
                row.handle()
                    .is_some_and(|h| h.eq_ignore_ascii_case(&op.occurrence[depth + 1]))
            })
            .collect();
        if here.len() == 1 && here[0].occurrence.len() == depth + 2 && here[0].kind == "delete" {
            continue;
        }
        let next = *new_rows
            .get(pos)
            .context("correction_clone_member_missing")?;
        pos += 1;
        ensure!(
            row.kind == next.kind && next.get(330) == Some(owner),
            "correction_clone_order_or_owner"
        );
        fresh.insert(u64::from_str_radix(
            next.handle().context("correction_clone_member_handle")?,
            16,
        )?);
        if !here.is_empty() {
            audit_occurrence((a, b), row, next, depth + 1, &here, receipt, fresh)?;
        } else {
            let allowed: &[i32] = if row.kind == "BLOCK" {
                &[2, 3, 5, 70, 330]
            } else {
                &[5, 330]
            };
            ensure!(
                same_tags_except(row, next, allowed),
                "correction_unselected_cloned_member_changed"
            );
            if row.kind == "BLOCK" {
                ensure!(
                    next.get(2) == Some(name)
                        && next.get(70) == Some("0")
                        && next.get(3).is_none_or(|v| v == name),
                    "correction_clone_header"
                );
            }
        }
    }
    ensure!(pos == new_rows.len(), "correction_clone_extra_member");
    Ok(())
}

/// Audit saved bytes, declared patch ranges, untouched roots/definitions, handle uniqueness and operation binding.
pub fn verify(
    source: &[u8],
    corrected: &[u8],
    plan: &MdDrawingCorrectionPlan,
    r: &MdDrawingCorrectionReceipt,
) -> Result<()> {
    ensure!(
        sha256(source) == r.source_sha256
            && r.source_sha256 == plan.source_sha256
            && sha256(corrected) == r.corrected_sha256,
        "correction_receipt_digest"
    );
    let a = Raw::parse(source)?;
    let b = Raw::parse(corrected)?;
    let issues: BTreeMap<_, _> = plan.issues.iter().map(|i| (i.id.as_str(), i)).collect();
    for op in &plan.operations {
        ensure!(op.validate().is_ok(), "correction_audit_operation_contract");
        ensure!(
            op.kind == "add_text" || !op.occurrence.is_empty(),
            "correction_audit_empty_occurrence"
        );
        ensure!(
            op.kind != "move"
                || (op.translation.len() == 2 && op.translation.iter().all(|n| n.is_finite())),
            "correction_audit_translation"
        );
        ensure!(
            issues
                .get(op.issue_id.as_str())
                .is_some_and(|i| i.confidence == "certain"),
            "correction_audit_uncertain_issue"
        );
        if !op.occurrence.is_empty() {
            ensure!(
                op.expected_record_sha256.as_deref()
                    == Some(record_sha256(source, &op.occurrence)?.as_str()),
                "correction_audit_record_precondition"
            );
        }
    }
    let changes: Vec<_> = r
        .changes
        .iter()
        .map(|p| {
            Ok(Patch {
                start: usize::try_from(p.start)?,
                end: usize::try_from(p.end)?,
                bytes: unhex(&p.replacement_hex)?,
                issue: p.issue_id.clone(),
            })
        })
        .collect::<Result<_>>()?;
    ensure!(
        splice(source, &changes)? == corrected,
        "correction_unrecorded_byte_change"
    );
    let targets: BTreeSet<_> = plan
        .operations
        .iter()
        .filter_map(|o| o.occurrence.first().map(String::as_str))
        .collect();
    for change in &changes {
        ensure!(
            issues.contains_key(change.issue.as_str()),
            "correction_change_issue_missing"
        );
        let target_record = a.records.iter().any(|rec| {
            rec.section == "ENTITIES"
                && rec
                    .handle()
                    .is_some_and(|h| targets.iter().any(|x| x.eq_ignore_ascii_case(h)))
                && rec.start <= change.start
                && change.end <= rec.end
        });
        let insertion = change.start == change.end
            && a.records.iter().any(|rec| {
                rec.start == change.start
                    && ((rec.kind == "ENDSEC"
                        && ["ENTITIES", "BLOCKS"].contains(&rec.section.as_str()))
                        || (rec.kind == "ENDTAB" && rec.table == "BLOCK_RECORD"))
            });
        let metadata = a.records.iter().any(|rec| {
            rec.tags.iter().enumerate().any(|(i, t)| {
                t.start == change.start
                    && t.end == change.end
                    && ((rec.section == "HEADER"
                        && t.code == 5
                        && i > 0
                        && rec.tags[i - 1].value == "$HANDSEED")
                        || (rec.kind == "TABLE" && rec.table == "BLOCK_RECORD" && t.code == 70))
            })
        });
        ensure!(
            target_record || insertion || metadata,
            "correction_patch_outside_declared_scope"
        );
    }
    for rec in &a.records {
        if rec.section == "HEADER" || rec.kind == "TABLE" {
            continue;
        }
        if let Some(h) = rec.handle() {
            if rec.section == "ENTITIES" && targets.iter().any(|x| x.eq_ignore_ascii_case(h)) {
                continue;
            }
            let rs: Vec<_> = b.records.iter().filter(|s| s.handle() == Some(h)).collect();
            ensure!(
                rs.len() == 1 && source[rec.start..rec.end] == corrected[rs[0].start..rs[0].end],
                "correction_untargeted_record_changed:{h}"
            );
        }
    }
    let ah = a.handles()?;
    let bh = b.handles()?;
    let fresh: BTreeSet<_> = r
        .new_handles
        .iter()
        .map(|s| u64::from_str_radix(s, 16))
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        fresh.len() == r.new_handles.len() && fresh == bh.difference(&ah).copied().collect(),
        "correction_new_handle_inventory"
    );
    let applied: BTreeMap<_, _> = r
        .applied
        .iter()
        .map(|a| (a.operation_id.as_str(), a))
        .collect();
    ensure!(
        applied.len() == plan.operations.len() && applied.len() == r.applied.len(),
        "correction_operation_inventory"
    );
    let mut expected_new = BTreeSet::new();
    for root in &targets {
        let ops: Vec<_> = plan
            .operations
            .iter()
            .filter(|o| {
                o.occurrence
                    .first()
                    .is_some_and(|h| h.eq_ignore_ascii_case(root))
            })
            .collect();
        let old = &a.records[a.entity(root, "ENTITIES", "")?];
        if ops.len() == 1 && ops[0].occurrence.len() == 1 && ops[0].kind == "delete" {
            ensure!(
                b.entity(root, "ENTITIES", "").is_err(),
                "correction_delete_not_applied"
            );
        } else {
            let new = &b.records[b.entity(root, "ENTITIES", "")?];
            audit_occurrence((&a, &b), old, new, 0, &ops, r, &mut expected_new)?;
        }
    }
    for op in plan.operations.iter().filter(|o| o.kind == "add_text") {
        let row = applied
            .get(op.id.as_str())
            .context("correction_added_record")?;
        for h in &row.result_handles {
            expected_new.insert(u64::from_str_radix(h, 16)?);
        }
    }
    ensure!(expected_new == fresh, "correction_unplanned_new_records");
    ensure!(
        b.records.len() + ah.difference(&bh).count() == a.records.len() + fresh.len(),
        "correction_unplanned_record_inventory"
    );
    if !fresh.is_empty()
        && let Some(seed) = b
            .records
            .iter()
            .filter(|r| r.section == "HEADER")
            .flat_map(|r| r.tags.windows(2))
            .find(|p| p[0].value == "$HANDSEED")
    {
        ensure!(
            u64::from_str_radix(&seed[1].value, 16)?
                > *bh.last().context("correction_no_live_handles")?,
            "correction_handseed_not_advanced"
        );
    }
    let applied_issues: BTreeSet<_> = r.applied.iter().map(|a| a.issue_id.as_str()).collect();
    let remaining: Vec<_> = plan
        .issues
        .iter()
        .filter(|i| !applied_issues.contains(i.id.as_str()))
        .map(|i| i.id.as_str())
        .collect();
    ensure!(
        remaining
            == r.remaining_issue_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        "correction_remaining_inventory"
    );
    for op in &plan.operations {
        let item = applied
            .get(op.id.as_str())
            .context("correction_operation_missing")?;
        ensure!(
            item.issue_id == op.issue_id && item.source_occurrence == op.occurrence,
            "correction_operation_binding"
        );
        if op.kind == "delete" {
            ensure!(item.result_handles.is_empty(), "correction_delete_result");
            continue;
        }
        ensure!(item.result_handles.len() == 1, "correction_result_count");
        let rows: Vec<_> = b
            .records
            .iter()
            .filter(|x| x.handle() == Some(item.result_handles[0].as_str()))
            .collect();
        ensure!(rows.len() == 1, "correction_result_missing");
        let after = rows[0];
        let before = if op.occurrence.is_empty() {
            None
        } else {
            let mut section = "ENTITIES";
            let mut block = String::new();
            let mut result = None;
            for h in &op.occurrence {
                let i = a.entity(h, section, &block)?;
                let record = &a.records[i];
                result = Some(record);
                block = record.get(2).unwrap_or_default().into();
                section = "BLOCKS";
            }
            result
        };
        if let Some(before) = before {
            ensure!(before.kind == after.kind, "correction_target_kind_changed");
            let mut allowed = BTreeSet::new();
            if op.occurrence.len() > 1 {
                allowed.extend([5, 330]);
            }
            if op.kind == "set_style" {
                for e in &op.style {
                    allowed.insert(i32::try_from(e.code)?);
                }
            }
            if op.kind == "move" {
                ensure!(op.translation.len() == 2, "correction_translation");
                let coords: &[i32] = match before.kind.as_str() {
                    "LINE" => &[10, 11],
                    "SOLID" => &[10, 11, 12, 13],
                    "TEXT"
                        if before.get(72).is_some_and(|v| v != "0")
                            || before.get(73).is_some_and(|v| v != "0") =>
                    {
                        &[10, 11]
                    }
                    _ => &[10],
                };
                for c in coords {
                    for (code, offset) in [(*c, op.translation[0]), (*c + 10, op.translation[1])] {
                        let old = before
                            .get(code)
                            .context("correction_source_point")?
                            .parse::<f64>()?;
                        let new = after
                            .get(code)
                            .context("correction_result_point")?
                            .parse::<f64>()?;
                        ensure!(
                            (old + offset - new).abs() < 1e-9_f64.max(old.abs() * 1e-12),
                            "correction_all_move_points"
                        );
                        allowed.insert(code);
                    }
                }
            }
            let unchanged = |r: &Record| {
                r.tags
                    .iter()
                    .filter(|t| !allowed.contains(&t.code))
                    .map(|t| (t.code, t.pair.clone()))
                    .collect::<Vec<_>>()
            };
            ensure!(
                unchanged(before) == unchanged(after),
                "correction_unplanned_target_field_change"
            );
        }
        match op.kind.as_str() {
            "add_text" => {
                let t = op.text.as_ref().context("correction_text_missing")?;
                ensure!(
                    after.kind == "TEXT"
                        && after.section == "ENTITIES"
                        && after.get(1) == Some(dxf_text(&t.value)?.as_str())
                        && after.get(40).and_then(|v| v.parse::<f64>().ok()) == Some(t.height)
                        && after.get(10).and_then(|v| v.parse::<f64>().ok())
                            == t.position.first().copied()
                        && after.get(20).and_then(|v| v.parse::<f64>().ok())
                            == t.position.get(1).copied()
                        && after.get(50).and_then(|v| v.parse::<f64>().ok()) == Some(t.rotation)
                        && after.get(8) == Some(t.layer.as_str()),
                    "correction_text_postcondition"
                );
            }
            "set_style" => {
                for t in &op.style {
                    ensure!(
                        after.get(i32::try_from(t.code)?) == Some(t.value.as_str()),
                        "correction_style_postcondition"
                    );
                }
            }
            "move" => {
                let mut sec = "ENTITIES";
                let mut blk = String::new();
                let mut before = None;
                for h in &op.occurrence {
                    let ix = a.entity(h, sec, &blk)?;
                    let rec = &a.records[ix];
                    before = Some(rec);
                    sec = "BLOCKS";
                    blk = rec.get(2).unwrap_or_default().to_owned();
                }
                let before = before.context("correction_move_source")?;
                for (c, d) in [(10, op.translation[0]), (20, op.translation[1])] {
                    let old = before
                        .get(c)
                        .context("correction_move_source_point")?
                        .parse::<f64>()?;
                    let new = after
                        .get(c)
                        .context("correction_move_result_point")?
                        .parse::<f64>()?;
                    ensure!(
                        (new - old - d).abs() < 1e-9_f64.max(old.abs() * 1e-12),
                        "correction_move_postcondition"
                    );
                }
            }
            _ => bail!("correction_kind"),
        }
    }
    Ok(())
}

/// Create a new file only; never truncate or rename over a source or an existing destination.
pub fn write_new(
    source: &Path,
    destination: &Path,
    plan: &MdDrawingCorrectionPlan,
) -> Result<MdDrawingCorrectionReceipt> {
    use std::io::Write;
    ensure!(source != destination, "correction_source_destination_equal");
    let before = oe_core::read_regular_bytes(source, 64 * 1024 * 1024)?;
    let (bytes, mut receipt) = rewrite(&before, plan)?;
    let parent = destination
        .parent()
        .context("correction_destination_parent")?;
    let temporary = parent.join(format!(
        ".correction-{}-{}.tmp",
        std::process::id(),
        receipt.corrected_sha256
    ));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .context("correction_new_temporary")?;
    let publish = (|| -> Result<()> {
        f.write_all(&bytes)?;
        f.sync_all()?;
        ensure!(
            oe_core::read_regular_bytes(source, 64 * 1024 * 1024)? == before,
            "correction_source_changed_during_write"
        );
        std::fs::hard_link(&temporary, destination)
            .context("correction_destination_must_be_new")?;
        Ok(())
    })();
    drop(f);
    let cleanup = std::fs::remove_file(&temporary);
    publish?;
    cleanup?;
    let saved = oe_core::read_regular_bytes(destination, 64 * 1024 * 1024)?;
    verify(&before, &saved, plan, &receipt)?;
    receipt.source_path = Some(source.to_string_lossy().into_owned());
    receipt.corrected_path = Some(destination.to_string_lossy().into_owned());
    receipt.checks = vec![
        "source_bytes_unchanged".into(),
        "saved_bytes_match_receipt".into(),
        "untargeted_records_identical".into(),
        "unique_live_handles".into(),
    ];
    Ok(receipt)
}
