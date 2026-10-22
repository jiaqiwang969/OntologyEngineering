//! TSV source content is independent of execution authority. No tool/process,
//! filesystem, model, Project or source registry is owned here.
use crate::{
    books::SourceProvider,
    page_content3::{ContentLimits, DecodedPageImage, read_bound, read_png_source},
};
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticContentAsset3, SemanticOcrTsvRow3, SemanticOcrTsvSourceContent5,
    SemanticPageImageBinding3, SemanticPageImageContent3,
};
use std::collections::BTreeMap;

const HEADER: &str =
    "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";
const UNKNOWN: &[&str] = &[
    "actual_execution_authority_is_owned_by_caller",
    "transcription_accuracy",
    "actual_tool_language_and_runtime_loading",
    "reading_order_semantics",
    "OCR_hierarchy_references_are_not_engineering_bbox_containment",
    "source_applicability",
    "numeric_verification",
    "aggregate_source_read_deadline",
    "all_original_ocr_interfaces",
    "engineering_adoption",
];
fn contract(r: Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|e| anyhow::anyhow!("tsv_source5_contract:{}", e.join(";")))
}
/// Parse every original row. The source need not be a fixture; this neutral
/// result still cannot attest that an OCR process happened or was accurate.
pub fn read_tsv_source5(
    provider: &dyn SourceProvider,
    image: &DecodedPageImage,
    asset: &SemanticContentAsset3,
    limits: &ContentLimits,
) -> Result<SemanticOcrTsvSourceContent5> {
    limits.validate()?;
    read_bound(
        provider,
        &image.content().binding.image_asset,
        limits.encoded_bytes,
    )?;
    let raw = read_bound(provider, asset, limits.tsv_bytes)?;
    let (header_raw, rows, word_text_in_order, noncontaining_parent_rows) =
        source_rows5(&raw, image.content(), limits)?;
    let content = SemanticOcrTsvSourceContent5 {
        schema: "semantica.ocr-tsv-source-content/5".into(),
        image: image.content().clone(),
        raw_asset: asset.clone(),
        raw_sha256: crate::sha256(&raw),
        raw_bytes: raw.len() as u64,
        header_raw,
        rows,
        word_text_in_order,
        noncontaining_parent_rows,
        reading_order: "tsv_raw_order_only".into(),
        execution_authority: "not_owned_by_content_library".into(),
        qualification: "source_content_only".into(),
        unknown: UNKNOWN.iter().map(|s| (*s).into()).collect(),
    };
    contract(content.validate())?;
    read_bound(provider, asset, limits.tsv_bytes)?;
    read_bound(
        provider,
        &image.content().binding.image_asset,
        limits.encoded_bytes,
    )?;
    Ok(content)
}
pub fn verify_tsv_source5(
    provider: &dyn SourceProvider,
    binding: &SemanticPageImageBinding3,
    asset: &SemanticContentAsset3,
    content: &SemanticOcrTsvSourceContent5,
    limits: &ContentLimits,
) -> Result<()> {
    contract(content.validate())?;
    let image = read_png_source(provider, binding, limits)?;
    ensure!(
        read_tsv_source5(provider, &image, asset, limits)? == *content,
        "tsv_source5_replay_changed"
    );
    Ok(())
}

// Explicit source-observation parser; legacy fixture3 remains strict.
fn unsigned(value: &str) -> Result<u64> {
    ensure!(
        !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()),
        "tsv_unsigned_invalid"
    );
    value.parse().context("tsv_integer_overflow")
}
fn confidence(value: &str) -> Result<Option<f64>> {
    ensure!(
        !value.is_empty() && value == value.trim(),
        "tsv_confidence_invalid"
    );
    let score: f64 = value.parse().context("tsv_confidence_invalid")?;
    if score == -1.0 {
        return Ok(None);
    }
    ensure!(
        score.is_finite() && (0.0..=100.0).contains(&score),
        "tsv_confidence_out_of_range"
    );
    Ok(Some(score))
}
fn body(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .unwrap_or(line)
}
type Key = [u64; 6];
type Box4 = [u64; 4];
fn source_rows5(
    raw: &[u8],
    image: &SemanticPageImageContent3,
    limits: &ContentLimits,
) -> Result<(String, Vec<SemanticOcrTsvRow3>, Vec<String>, Vec<u64>)> {
    ensure!(!raw.contains(&0), "tsv_NUL_invalid");
    let text = std::str::from_utf8(raw).context("tsv_UTF8_invalid")?;
    let mut lines = text.split_inclusive('\n');
    let header = lines.next().context("tsv_header_missing")?;
    ensure!(
        header.len() <= limits.line_bytes && body(header) == HEADER,
        "tsv_header_invalid"
    );
    let mut at = header.len();
    let mut parsed = Vec::new();
    let mut words = Vec::new();
    let mut ids: BTreeMap<Key, Box4> = BTreeMap::new();
    let mut noncontaining_parent_rows = Vec::new();
    for line in lines {
        ensure!(
            parsed.len() < limits.rows && line.len() <= limits.line_bytes,
            "tsv_rows_or_line_budget"
        );
        let plain = body(line);
        ensure!(
            !plain.is_empty() && !plain.contains('\r'),
            "tsv_blank_line_or_CR_invalid"
        );
        let fields: Vec<_> = plain.split('\t').collect();
        ensure!(fields.len() == 12, "tsv_field_count_invalid");
        let mut v = [0u64; 10];
        for (to, from) in v.iter_mut().zip(&fields[..10]) {
            *to = unsigned(from)?;
        }
        let [
            level,
            page_num,
            block_num,
            par_num,
            line_num,
            word_num,
            left,
            top,
            width,
            height,
        ] = v;
        ensure!(page_num == 1, "tsv_single_image_page_invalid");
        ensure!(
            left.checked_add(width).is_some_and(|x| x <= image.width)
                && top.checked_add(height).is_some_and(|y| y <= image.height),
            "tsv_bbox_outside_image"
        );
        let parent = match level {
            1 => {
                ensure!(
                    parsed.is_empty()
                        && [block_num, par_num, line_num, word_num] == [0; 4]
                        && [left, top, width, height] == [0, 0, image.width, image.height],
                    "tsv_root_invalid"
                );
                None
            }
            2 => {
                ensure!(
                    block_num > 0 && [par_num, line_num, word_num] == [0; 3],
                    "tsv_block_id_invalid"
                );
                Some([1, 1, 0, 0, 0, 0])
            }
            3 => {
                ensure!(
                    block_num > 0 && par_num > 0 && [line_num, word_num] == [0; 2],
                    "tsv_paragraph_id_invalid"
                );
                Some([2, 1, block_num, 0, 0, 0])
            }
            4 => {
                ensure!(
                    block_num > 0 && par_num > 0 && line_num > 0 && word_num == 0,
                    "tsv_line_id_invalid"
                );
                Some([3, 1, block_num, par_num, 0, 0])
            }
            5 => {
                ensure!(
                    block_num > 0 && par_num > 0 && line_num > 0 && word_num > 0,
                    "tsv_word_id_invalid"
                );
                Some([4, 1, block_num, par_num, line_num, 0])
            }
            _ => anyhow::bail!("tsv_level_invalid"),
        };
        if let Some(parent) = parent {
            let [x, y, w, h] = *ids.get(&parent).context("tsv_parent_missing_or_order")?;
            // OCR hierarchy references group observations. Reported parent
            // boxes are not an assertion that each child rectangle nests.
            // Preserve the original rectangles and record the exact row index;
            // never crop, drop or turn this into an engineering relation.
            if !(left >= x && top >= y && left + width <= x + w && top + height <= y + h) {
                noncontaining_parent_rows.push(parsed.len() as u64);
            }
        }
        let key = [level, page_num, block_num, par_num, line_num, word_num];
        ensure!(
            ids.insert(key, [left, top, width, height]).is_none(),
            "tsv_duplicate_id"
        );
        let row = SemanticOcrTsvRow3 {
            ordinal: parsed.len() as u64,
            byte_offset: at as u64,
            raw_line: line.into(),
            level,
            page_num,
            block_num,
            par_num,
            line_num,
            word_num,
            left,
            top,
            width,
            height,
            confidence_raw: fields[10].into(),
            confidence: confidence(fields[10])?,
            text: fields[11].into(),
        };
        contract(row.validate())?;
        if level == 5 {
            words.push(row.text.clone());
        }
        parsed.push(row);
        at = at
            .checked_add(line.len())
            .context("tsv_byte_offset_overflow")?;
    }
    ensure!(
        !parsed.is_empty() && at == raw.len(),
        "tsv_root_missing_or_raw_count"
    );
    Ok((header.into(), parsed, words, noncontaining_parent_rows))
}
