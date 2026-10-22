//! Exact PNG samples and fixed TSV source content through the existing provider.
//! No PDF renderer, OCR, model, filesystem, Project or source authorization is
//! owned here. Provider read allocation and aggregate deadlines remain unknown.
use crate::books::{SourceProvider, relative};
use anyhow::{Context, Result, ensure};
use flate2::{Decompress, FlushDecompress, Status};
use semantica_contracts::{
    SemanticContentAsset3, SemanticOcrTsvContent3, SemanticOcrTsvRow3, SemanticPageImageBinding3,
    SemanticPageImageContent3, SemanticPngChunk3,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor};

const PNG_UNKNOWN: [&str; 6] = [
    "original_pdf_and_renderer_relationship",
    "provider_authorization",
    "aggregate_source_read_deadline",
    "pixel_colour_interpretation",
    "all_original_image_formats",
    "engineering_adoption",
];
const TSV_UNKNOWN: [&str; 8] = [
    "actual_ocr_execution_and_transcription",
    "ocr_tool_language_and_runtime",
    "reading_order_semantics",
    "source_applicability",
    "numeric_verification",
    "aggregate_source_read_deadline",
    "all_original_ocr_interfaces",
    "engineering_adoption",
];
const HEADER: &str =
    "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

/// Finite content limits, not a claim about provider/process memory or timing.
#[derive(Clone, Debug)]
pub struct ContentLimits {
    pub encoded_bytes: usize,
    pub pixels: u64,
    pub decoded_bytes: usize,
    pub decoder_bytes: usize,
    pub chunks: usize,
    pub tsv_bytes: usize,
    pub line_bytes: usize,
    pub rows: usize,
}
impl Default for ContentLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: 4 * 1024 * 1024,
            pixels: 8 * 1024 * 1024,
            decoded_bytes: 32 * 1024 * 1024,
            decoder_bytes: 64 * 1024 * 1024,
            chunks: 4096,
            tsv_bytes: 8 * 1024 * 1024,
            line_bytes: 64 * 1024,
            rows: 65536,
        }
    }
}
impl ContentLimits {
    pub(crate) fn validate(&self) -> Result<()> {
        let maximum = Self::default();
        for (v, m) in [
            (self.encoded_bytes, maximum.encoded_bytes),
            (self.decoded_bytes, maximum.decoded_bytes),
            (self.decoder_bytes, maximum.decoder_bytes),
            (self.chunks, maximum.chunks),
            (self.tsv_bytes, maximum.tsv_bytes),
            (self.line_bytes, maximum.line_bytes),
            (self.rows, maximum.rows),
        ] {
            ensure!(v > 0 && v <= m, "page_content_limits_invalid");
        }
        ensure!(
            self.pixels > 0 && self.pixels <= maximum.pixels,
            "page_content_limits_invalid"
        );
        Ok(())
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn generated(errors: Result<(), Vec<String>>) -> Result<()> {
    errors.map_err(|e| anyhow::anyhow!("page_content_contract_invalid:{}", e.join(";")))
}
fn unknown<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_owned).collect()
}
pub(crate) fn read_bound(
    provider: &dyn SourceProvider,
    asset: &SemanticContentAsset3,
    maximum: usize,
) -> Result<Vec<u8>> {
    generated(asset.validate())?;
    relative(&asset.logical_id)?;
    ensure!(asset.bytes <= maximum as u64, "page_content_source_budget");
    let bytes = provider.read(&asset.logical_id)?;
    // SourceProvider::read is pre-existing and returns an allocated Vec. These
    // checks bound accepted content, not the provider's allocation or deadline.
    ensure!(
        bytes.len() <= maximum && bytes.len() as u64 == asset.bytes && sha(&bytes) == asset.sha256,
        "page_content_source_identity_mismatch"
    );
    Ok(bytes)
}
fn binding_valid(binding: &SemanticPageImageBinding3) -> Result<()> {
    generated(binding.validate())?;
    ensure!(
        binding.schema == "semantica.page-image-binding/3"
            && binding.document_bytes > 0
            && binding.page_count > 0
            && binding.physical_page > 0
            && binding.physical_page <= binding.page_count
            && (72..=300).contains(&binding.dpi),
        "page_image_declared_binding_invalid"
    );
    Ok(())
}

/// Opaque actual decode retained by callers; a deserialized report cannot stand
/// in for the image consumed by the fixed TSV parser.
pub struct DecodedPageImage {
    content: SemanticPageImageContent3,
    pixels: Vec<u8>,
}
impl DecodedPageImage {
    pub fn content(&self) -> &SemanticPageImageContent3 {
        &self.content
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}
struct PngEnvelope {
    width: u32,
    height: u32,
    colour: u8,
    channels: usize,
    row_stride: usize,
    sample_bytes: usize,
    inflated_bytes: usize,
    chunks: Vec<SemanticPngChunk3>,
    idat: Vec<u8>,
}
fn u32be(bytes: &[u8]) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes.try_into().context("png_integer_truncated")?,
    ))
}
/// Strict source envelope around the reliable PNG decoder. Unknown ancillary
/// domains and APNG are explicitly unsupported; no original payload is dropped.
fn envelope(raw: &[u8], limits: &ContentLimits) -> Result<PngEnvelope> {
    ensure!(
        raw.len() >= 8 && raw[..8] == *b"\x89PNG\r\n\x1a\n",
        "png_signature_invalid"
    );
    let mut at = 8usize;
    let mut chunks = Vec::new();
    let mut shape = None;
    let mut phys = false;
    let mut idat = Vec::new();
    let mut have_idat = false;
    let mut ended = false;
    while at < raw.len() {
        ensure!(
            chunks.len() < limits.chunks && raw.len() - at >= 12,
            "png_chunk_budget_or_truncated"
        );
        let len = u32be(&raw[at..at + 4])? as usize;
        let end = at
            .checked_add(12)
            .and_then(|n| n.checked_add(len))
            .context("png_chunk_overflow")?;
        ensure!(end <= raw.len(), "png_chunk_truncated");
        let kind = &raw[at + 4..at + 8];
        let data = &raw[at + 8..end - 4];
        ensure!(
            crc32fast::hash(&raw[at + 4..end - 4]) == u32be(&raw[end - 4..end])?,
            "png_crc_mismatch"
        );
        match kind {
            b"IHDR" => {
                ensure!(chunks.is_empty() && len == 13, "png_IHDR_order_or_size");
                let width = u32be(&data[..4])?;
                let height = u32be(&data[4..8])?;
                ensure!(
                    width > 0 && height > 0 && width <= 8192 && height <= 8192,
                    "png_dimension_limit"
                );
                ensure!(
                    data[8] == 8 && data[10] == 0 && data[11] == 0 && data[12] == 0,
                    "png_bitdepth_compression_filter_interlace_unsupported"
                );
                let channels = match data[9] {
                    0 => 1,
                    2 => 3,
                    4 => 2,
                    6 => 4,
                    _ => anyhow::bail!("png_colour_type_unsupported"),
                };
                ensure!(
                    u64::from(width) * u64::from(height) <= limits.pixels,
                    "png_pixel_limit"
                );
                let row = (width as usize)
                    .checked_mul(channels)
                    .context("png_row_overflow")?;
                let samples = row
                    .checked_mul(height as usize)
                    .context("png_sample_overflow")?;
                ensure!(samples <= limits.decoded_bytes, "png_decoded_limit");
                let inflated = row
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(height as usize))
                    .context("png_inflated_overflow")?;
                shape = Some((width, height, data[9], channels, row, samples, inflated));
            }
            b"pHYs" => {
                ensure!(
                    shape.is_some() && !phys && !have_idat && len == 9 && data[8] <= 1,
                    "png_pHYs_order_or_size"
                );
                phys = true;
            }
            b"IDAT" => {
                ensure!(shape.is_some() && !ended, "png_IDAT_order");
                ensure!(
                    idat.len()
                        .checked_add(len)
                        .is_some_and(|n| n <= limits.encoded_bytes),
                    "png_IDAT_budget"
                );
                idat.extend_from_slice(data);
                have_idat = true;
            }
            b"IEND" => {
                ensure!(
                    shape.is_some() && have_idat && !idat.is_empty() && len == 0,
                    "png_IEND_order_or_size"
                );
                ended = true;
                ensure!(end == raw.len(), "png_trailing_bytes");
            }
            _ => anyhow::bail!(
                "png_chunk_domain_unsupported:{}",
                String::from_utf8_lossy(kind)
            ),
        }
        let chunk = SemanticPngChunk3 {
            ordinal: chunks.len() as u64,
            byte_offset: at as u64,
            kind: String::from_utf8(kind.to_vec())?,
            payload_bytes: len as u64,
            payload_sha256: sha(data),
            raw_chunk_sha256: sha(&raw[at..end]),
        };
        generated(chunk.validate())?;
        chunks.push(chunk);
        at = end;
        if ended {
            break;
        }
    }
    ensure!(ended, "png_IEND_missing");
    let (width, height, colour, channels, row_stride, sample_bytes, inflated_bytes) =
        shape.context("png_IHDR_missing")?;
    Ok(PngEnvelope {
        width,
        height,
        colour,
        channels,
        row_stride,
        sample_bytes,
        inflated_bytes,
        chunks,
        idat,
    })
}
fn strict_zlib_filters(e: &PngEnvelope) -> Result<Vec<u64>> {
    let mut inflated = vec![
        0u8;
        e.inflated_bytes
            .checked_add(1)
            .context("png_inflated_overflow")?
    ];
    let mut decode = Decompress::new(true);
    let state = decode
        .decompress(&e.idat, &mut inflated, FlushDecompress::Finish)
        .context("png_zlib_invalid")?;
    ensure!(
        state == Status::StreamEnd
            && decode.total_in() == e.idat.len() as u64
            && decode.total_out() == e.inflated_bytes as u64,
        "png_zlib_incomplete_tail_or_expansion"
    );
    let mut counts = vec![0; 5];
    for row in 0..e.height as usize {
        let filter = inflated[row * (e.row_stride + 1)] as usize;
        ensure!(filter < 5, "png_scanline_filter_invalid");
        counts[filter] += 1;
    }
    Ok(counts)
}
pub fn read_png_source(
    provider: &dyn SourceProvider,
    binding: &SemanticPageImageBinding3,
    limits: &ContentLimits,
) -> Result<DecodedPageImage> {
    limits.validate()?;
    binding_valid(binding)?;
    let raw = read_bound(provider, &binding.image_asset, limits.encoded_bytes)?;
    let e = envelope(&raw, limits)?;
    let filter_counts = strict_zlib_filters(&e)?;
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(&raw),
        png::Limits {
            bytes: limits.decoder_bytes,
        },
    );
    decoder.set_transformations(png::Transformations::IDENTITY);
    decoder.ignore_checksums(false);
    let mut reader = decoder.read_info().context("png_decoder_header")?;
    ensure!(
        reader.output_buffer_size() == Some(e.sample_bytes),
        "png_decoder_buffer_mismatch"
    );
    let mut pixels = vec![0; e.sample_bytes];
    let actual = reader
        .next_frame(&mut pixels)
        .context("png_decoder_pixels")?;
    ensure!(
        actual.width == e.width
            && actual.height == e.height
            && actual.bit_depth == png::BitDepth::Eight
            && actual.color_type as u8 == e.colour
            && actual.line_size == e.row_stride
            && actual.buffer_size() == pixels.len(),
        "png_decoder_output_mismatch"
    );
    reader.finish().context("png_decoder_finish")?;
    let content = SemanticPageImageContent3 {
        schema: "semantica.page-image-content/3".into(),
        binding: binding.clone(),
        width: u64::from(e.width),
        height: u64::from(e.height),
        bit_depth: 8,
        colour_type: u64::from(e.colour),
        channels: e.channels as u64,
        row_stride: e.row_stride as u64,
        encoded_sha256: sha(&raw),
        encoded_bytes: raw.len() as u64,
        decoded_sha256: sha(&pixels),
        decoded_bytes: pixels.len() as u64,
        pixel_profile: "row_major_untransformed_u8".into(),
        filter_counts,
        chunks: e.chunks,
        qualification: "source_content_only".into(),
        unknown: unknown(PNG_UNKNOWN),
    };
    generated(content.validate())?;
    read_bound(provider, &binding.image_asset, limits.encoded_bytes)?;
    Ok(DecodedPageImage { content, pixels })
}
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
pub(crate) fn rows(
    raw: &[u8],
    image: &SemanticPageImageContent3,
    limits: &ContentLimits,
) -> Result<(String, Vec<SemanticOcrTsvRow3>, Vec<String>)> {
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
            ensure!(
                left >= x && top >= y && left + width <= x + w && top + height <= y + h,
                "tsv_bbox_outside_parent"
            );
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
        generated(row.validate())?;
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
    Ok((header.into(), parsed, words))
}
/// Explicit fixture-only content path. This cannot describe an OCR invocation.
pub fn read_fixed_tsv_source(
    provider: &dyn SourceProvider,
    image: &DecodedPageImage,
    asset: &SemanticContentAsset3,
    limits: &ContentLimits,
) -> Result<SemanticOcrTsvContent3> {
    limits.validate()?;
    read_bound(
        provider,
        &image.content.binding.image_asset,
        limits.encoded_bytes,
    )?;
    let raw = read_bound(provider, asset, limits.tsv_bytes)?;
    let (header_raw, rows, word_text_in_order) = rows(&raw, &image.content, limits)?;
    let content = SemanticOcrTsvContent3 {
        schema: "semantica.ocr-tsv-fixture-content/3".into(),
        image: image.content.clone(),
        raw_asset: asset.clone(),
        raw_sha256: sha(&raw),
        raw_bytes: raw.len() as u64,
        header_raw,
        rows,
        word_text_in_order,
        reading_order: "tsv_raw_order_only".into(),
        ocr_execution: "not_run".into(),
        qualification: "fixed_fixture_content_only".into(),
        unknown: unknown(TSV_UNKNOWN),
    };
    generated(content.validate())?;
    read_bound(provider, asset, limits.tsv_bytes)?;
    read_bound(
        provider,
        &image.content.binding.image_asset,
        limits.encoded_bytes,
    )?;
    Ok(content)
}
pub fn verify_png_content(
    provider: &dyn SourceProvider,
    binding: &SemanticPageImageBinding3,
    content: &SemanticPageImageContent3,
    limits: &ContentLimits,
) -> Result<()> {
    generated(content.validate())?;
    ensure!(
        read_png_source(provider, binding, limits)?.content == *content,
        "page_image_content_replay_mismatch"
    );
    Ok(())
}
pub fn verify_fixed_tsv_content(
    provider: &dyn SourceProvider,
    binding: &SemanticPageImageBinding3,
    asset: &SemanticContentAsset3,
    content: &SemanticOcrTsvContent3,
    limits: &ContentLimits,
) -> Result<()> {
    generated(content.validate())?;
    let image = read_png_source(provider, binding, limits)?;
    ensure!(
        read_fixed_tsv_source(provider, &image, asset, limits)? == *content,
        "tsv_fixture_content_replay_mismatch"
    );
    Ok(())
}
