use semantica_contracts::{SemanticContentAsset3, SemanticPageImageBinding3};
use semantica_runtime::{books::SourceProvider, page_content3::*};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::BTreeMap, io::Write};

const FIXTURE: &[u8] = include_bytes!("fixtures/page-content3/fixed-eight-rows.tsv");
fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
struct Memory {
    bytes: BTreeMap<String, Vec<u8>>,
    reads: RefCell<Vec<String>>,
    mutate: bool,
}
impl Memory {
    fn one(raw: &[u8]) -> Self {
        Self {
            bytes: [("selected/image.png".into(), raw.to_vec())].into(),
            reads: RefCell::new(Vec::new()),
            mutate: false,
        }
    }
    fn with_tsv(raw: &[u8], tsv: &[u8]) -> Self {
        let mut p = Self::one(raw);
        p.bytes.insert("selected/fixture.tsv".into(), tsv.to_vec());
        p
    }
}
impl SourceProvider for Memory {
    fn read(&self, id: &str) -> anyhow::Result<Vec<u8>> {
        let mut raw = self
            .bytes
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("unselected asset"))?
            .clone();
        let repeated = self.reads.borrow().iter().any(|x| x == id);
        self.reads.borrow_mut().push(id.into());
        if self.mutate && repeated {
            raw[0] ^= 1;
        }
        Ok(raw)
    }
}
fn asset(id: &str, raw: &[u8]) -> SemanticContentAsset3 {
    SemanticContentAsset3 {
        logical_id: id.into(),
        sha256: sha(raw),
        bytes: raw.len() as u64,
    }
}
fn binding(raw: &[u8]) -> SemanticPageImageBinding3 {
    SemanticPageImageBinding3 {
        schema: "semantica.page-image-binding/3".into(),
        document_sha256: "baf6363d11afb072bdb960c9a735821393771f213e7495f981ee97c3d9732259".into(),
        document_bytes: 35986251,
        physical_page: 188,
        page_count: 201,
        dpi: 144,
        image_asset: asset("selected/image.png", raw),
    }
}
fn real_png() -> Vec<u8> {
    let path = std::env::var("SEMANTICA_STAGE1_PNG")
        .expect("exact original PNG must be supplied for this finite review test");
    let raw = std::fs::read(path).unwrap();
    assert_eq!(raw.len(), 432263);
    assert_eq!(
        sha(&raw),
        "6c9b8ebbbf1c39a2d0b5b728a4e1fa91b0b4efa381d28349ad89f2eae274cbc6"
    );
    raw
}
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(&(data.len() as u32).to_be_bytes());
    raw.extend_from_slice(kind);
    raw.extend_from_slice(data);
    let crc = crc32fast::hash(&raw[4..]);
    raw.extend_from_slice(&crc.to_be_bytes());
    raw
}
fn compressed(raw: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(raw).unwrap();
    e.finish().unwrap()
}
fn tiny_with(
    width: u32,
    height: u32,
    depth: u8,
    colour: u8,
    interlace: u8,
    data: &[u8],
) -> Vec<u8> {
    let mut head = Vec::new();
    head.extend_from_slice(&width.to_be_bytes());
    head.extend_from_slice(&height.to_be_bytes());
    head.extend_from_slice(&[depth, colour, 0, 0, interlace]);
    let mut raw = b"\x89PNG\r\n\x1a\n".to_vec();
    raw.extend(chunk(b"IHDR", &head));
    raw.extend(chunk(b"IDAT", data));
    raw.extend(chunk(b"IEND", &[]));
    raw
}
fn tiny() -> Vec<u8> {
    tiny_with(1, 1, 8, 2, 0, &compressed(&[0, 19, 23, 29]))
}
fn rejection(raw: &[u8], reason: &str) {
    let p = Memory::one(raw);
    let error = read_png_source(&p, &binding(raw), &ContentLimits::default())
        .err()
        .expect("finite counterexample must refuse")
        .to_string();
    assert!(error.contains(reason), "expected {reason}, got {error}");
}
fn tsv_rejection(tsv: &[u8], reason: &str) {
    let raw = real_png();
    let p = Memory::with_tsv(&raw, tsv);
    let b = binding(&raw);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    let error = read_fixed_tsv_source(
        &p,
        &image,
        &asset("selected/fixture.tsv", tsv),
        &ContentLimits::default(),
    )
    .err()
    .expect("fixed TSV counterexample must refuse")
    .to_string();
    assert!(error.contains(reason), "expected {reason}, got {error}");
}

#[test]
fn actual_page188_matches_preparser_independent_rgb_samples_and_original_chunks() {
    let raw = real_png();
    let p = Memory::one(&raw);
    let b = binding(&raw);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    let c = image.content();
    assert_eq!(
        (c.width, c.height, c.channels, c.bit_depth, c.colour_type),
        (941, 1406, 3, 8, 2)
    );
    assert_eq!(c.row_stride, 2823);
    assert_eq!(image.pixels().len(), 3969138);
    assert_eq!(
        c.decoded_sha256,
        "f06835b875f8b48172383b4e127486a67590e53efb523a1ecd2f17207e5e2685"
    );
    assert_eq!(c.decoded_bytes, 3969138);
    assert_eq!(c.encoded_sha256, b.image_asset.sha256);
    assert_eq!(c.filter_counts, [0, 137, 400, 29, 840]);
    let mut at = 8;
    for (i, item) in c.chunks.iter().enumerate() {
        assert_eq!(item.ordinal, i as u64);
        assert_eq!(item.byte_offset, at as u64);
        let end = at + item.payload_bytes as usize + 12;
        assert_eq!(item.raw_chunk_sha256, sha(&raw[at..end]));
        assert_eq!(item.payload_sha256, sha(&raw[at + 8..end - 4]));
        at = end;
    }
    assert_eq!(at, raw.len());
    assert_eq!(c.chunks[1].kind, "pHYs");
    assert_eq!(c.binding.physical_page, 188);
    assert_eq!(c.qualification, "source_content_only");
    assert!(
        c.unknown
            .iter()
            .any(|x| x == "aggregate_source_read_deadline")
    );
    assert_eq!(c.unknown.len(), 6);
    assert_eq!(p.reads.borrow().len(), 2);
    verify_png_content(&p, &b, c, &ContentLimits::default()).unwrap();
    println!(
        "REAL188 decoded {}B SHA{}; original={}B; provider_read_scope_only",
        c.decoded_bytes, c.decoded_sha256, c.encoded_bytes
    );
}
#[test]
fn fixed_tsv_preserves_every_original_row_newline_offset_and_low_or_unknown_confidence() {
    assert_eq!(FIXTURE.len(), 332);
    assert_eq!(
        sha(FIXTURE),
        "a914f2b624a0eab6a12ada764ac25301be605809e1c6a06da4f08fb06be0a7ec"
    );
    let raw = real_png();
    let p = Memory::with_tsv(&raw, FIXTURE);
    let b = binding(&raw);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    let a = asset("selected/fixture.tsv", FIXTURE);
    let t = read_fixed_tsv_source(&p, &image, &a, &ContentLimits::default()).unwrap();
    assert_eq!(t.rows.len(), 8);
    assert_eq!(t.word_text_in_order, ["固定样本", "A0", "0.05", ""]);
    assert_eq!(t.rows[4].confidence, Some(0.0));
    assert_eq!(t.rows[5].confidence, Some(25.5));
    assert_eq!(t.rows[6].confidence, Some(99.25));
    assert_eq!(t.rows[7].confidence, None);
    assert_eq!(t.rows[7].confidence_raw, "-1");
    let mut complete = t.header_raw.clone();
    let mut offset = complete.len();
    for (i, r) in t.rows.iter().enumerate() {
        assert_eq!(r.ordinal, i as u64);
        assert_eq!(r.byte_offset, offset as u64);
        complete.push_str(&r.raw_line);
        offset += r.raw_line.len();
        assert_eq!(r.page_num, 1);
    }
    assert_eq!(complete.as_bytes(), FIXTURE);
    assert_eq!(t.image.binding.physical_page, 188);
    assert_eq!(t.ocr_execution, "not_run");
    assert_eq!(t.qualification, "fixed_fixture_content_only");
    assert_eq!(t.reading_order, "tsv_raw_order_only");
    assert_eq!(t.unknown.len(), 8);
    verify_fixed_tsv_content(&p, &b, &a, &t, &ContentLimits::default()).unwrap();
}
#[test]
fn actual_source_and_mutation_guards_remain_separate_from_format_guards() {
    let raw = tiny();
    let p = Memory::one(&raw);
    let mut b = binding(&raw);
    b.image_asset.sha256 = "0".repeat(64);
    assert!(
        read_png_source(&p, &b, &ContentLimits::default())
            .err()
            .unwrap()
            .to_string()
            .contains("source_identity")
    );
    let mut b = binding(&raw);
    b.image_asset.bytes += 1;
    assert!(
        read_png_source(&p, &b, &ContentLimits::default())
            .err()
            .unwrap()
            .to_string()
            .contains("source_identity")
    );
    let mut p = Memory::one(&raw);
    p.mutate = true;
    assert!(
        read_png_source(&p, &binding(&raw), &ContentLimits::default())
            .err()
            .unwrap()
            .to_string()
            .contains("source_identity")
    );
}
#[test]
fn decoder_actually_consumes_no_filter_gray_alpha_and_rgb_source_samples() {
    for (colour, pixels) in [
        (0, vec![31]),
        (2, vec![19, 23, 29]),
        (4, vec![31, 77]),
        (6, vec![19, 23, 29, 77]),
    ] {
        let mut scan = vec![0];
        scan.extend_from_slice(&pixels);
        let raw = tiny_with(1, 1, 8, colour, 0, &compressed(&scan));
        let p = Memory::one(&raw);
        let image = read_png_source(&p, &binding(&raw), &ContentLimits::default()).unwrap();
        assert_eq!(image.pixels(), pixels);
        assert_eq!(image.content().filter_counts, [1, 0, 0, 0, 0]);
    }
}
#[test]
fn framing_crc_tails_and_unsupported_domains_are_explicit() {
    let good = tiny();
    read_png_source(
        &Memory::one(&good),
        &binding(&good),
        &ContentLimits::default(),
    )
    .unwrap();
    let mut bad = good.clone();
    bad[0] ^= 1;
    rejection(&bad, "signature");
    let mut bad = good.clone();
    bad[29] ^= 1;
    rejection(&bad, "crc");
    let mut bad = good.clone();
    bad[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    rejection(&bad, "truncated");
    let mut bad = good.clone();
    bad.extend([0]);
    rejection(&bad, "trailing");
    let bad = good[..good.len() - 12].to_vec();
    rejection(&bad, "IEND_missing");
    let mut bad = good[..33].to_vec();
    bad.extend(&good[8..33]);
    bad.extend(&good[33..]);
    rejection(&bad, "IHDR_order");
    for kind in [b"acTL", b"tEXt", b"ABCD", b"iCCP"] {
        let mut bad = good[..33].to_vec();
        bad.extend(chunk(kind, &[0]));
        bad.extend(&good[33..]);
        rejection(&bad, "domain_unsupported");
    }
    for (depth, colour, interlace, reason) in [
        (16, 2, 0, "bitdepth"),
        (8, 3, 0, "colour_type"),
        (8, 2, 1, "interlace"),
    ] {
        rejection(
            &tiny_with(1, 1, depth, colour, interlace, &compressed(&[0, 1, 2, 3])),
            reason,
        );
    }
    let phys = chunk(b"pHYs", &[0, 0, 0, 1, 0, 0, 0, 1, 1]);
    let mut bad = good[..33].to_vec();
    bad.extend(&phys);
    bad.extend(&phys);
    bad.extend(&good[33..]);
    rejection(&bad, "pHYs_order");
    let mut bad = good[..good.len() - 12].to_vec();
    bad.extend(&phys);
    bad.extend(&good[good.len() - 12..]);
    rejection(&bad, "pHYs_order");
}
#[test]
fn full_zlib_consumption_adler_and_expansion_are_independent_of_png_library_leniency() {
    let stream = compressed(&[0, 19, 23, 29]);
    let mut tail = stream.clone();
    tail.extend([0, 1]);
    rejection(&tiny_with(1, 1, 8, 2, 0, &tail), "zlib_incomplete_tail");
    let mut concat = stream.clone();
    concat.extend(compressed(&[0]));
    rejection(&tiny_with(1, 1, 8, 2, 0, &concat), "zlib_incomplete_tail");
    let mut adler = stream.clone();
    let last = adler.len() - 1;
    adler[last] ^= 1;
    rejection(&tiny_with(1, 1, 8, 2, 0, &adler), "zlib_invalid");
    rejection(
        &tiny_with(1, 1, 8, 2, 0, &stream[..stream.len() - 1]),
        "zlib_",
    );
    rejection(
        &tiny_with(1, 1, 8, 2, 0, &compressed(&[0, 1, 2, 3, 4])),
        "zlib_incomplete_tail_or_expansion",
    );
    rejection(
        &tiny_with(1, 1, 8, 2, 0, &compressed(&[5, 1, 2, 3])),
        "scanline_filter",
    );
    rejection(
        &tiny_with(1, 1, 8, 2, 0, &compressed(&[0, 1, 2])),
        "zlib_incomplete_tail",
    );
}
#[test]
fn declared_content_limits_reject_before_unbounded_decode_or_rows() {
    let raw = real_png();
    let p = Memory::one(&raw);
    let b = binding(&raw);
    for limits in [
        ContentLimits {
            encoded_bytes: 1,
            ..Default::default()
        },
        ContentLimits {
            pixels: 1,
            ..Default::default()
        },
        ContentLimits {
            decoded_bytes: 1,
            ..Default::default()
        },
        ContentLimits {
            decoder_bytes: 1,
            ..Default::default()
        },
        ContentLimits {
            chunks: 1,
            ..Default::default()
        },
    ] {
        assert!(read_png_source(&p, &b, &limits).is_err());
    }
    let p = Memory::with_tsv(&raw, FIXTURE);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    for limits in [
        ContentLimits {
            tsv_bytes: 1,
            ..Default::default()
        },
        ContentLimits {
            line_bytes: 16,
            ..Default::default()
        },
        ContentLimits {
            rows: 1,
            ..Default::default()
        },
    ] {
        assert!(
            read_fixed_tsv_source(&p, &image, &asset("selected/fixture.tsv", FIXTURE), &limits)
                .is_err()
        );
    }
    let too_large = tiny_with(8193, 1, 8, 2, 0, &compressed(&[0, 1, 2, 3]));
    rejection(&too_large, "dimension_limit");
}
#[test]
fn fixed_tsv_malformed_content_must_reach_the_parser_with_matching_source_sha() {
    let text = String::from_utf8(FIXTURE.to_vec()).unwrap();
    let mut invalid = FIXTURE.to_vec();
    invalid[0] = 0xff;
    tsv_rejection(&invalid, "UTF8");
    let mut nul = FIXTURE.to_vec();
    nul.push(0);
    tsv_rejection(&nul, "NUL");
    tsv_rejection(text.replacen("level", "Level", 1).as_bytes(), "header");
    tsv_rejection((text.clone() + "\n\n").as_bytes(), "blank_line");
    tsv_rejection(
        text.replace("99.25", "NaN").as_bytes(),
        "confidence_out_of_range",
    );
    tsv_rejection(
        text.replace("99.25", "inf").as_bytes(),
        "confidence_out_of_range",
    );
    tsv_rejection(
        text.replace("99.25", "100.01").as_bytes(),
        "confidence_out_of_range",
    );
    tsv_rejection(
        text.replace("99.25", "-2").as_bytes(),
        "confidence_out_of_range",
    );
    tsv_rejection(
        text.replace("99.25", " 99").as_bytes(),
        "confidence_invalid",
    );
    tsv_rejection(
        text.replace("5\t1\t1\t1\t1\t3", "5\t188\t1\t1\t1\t3")
            .as_bytes(),
        "single_image_page",
    );
    tsv_rejection(
        text.replace("5\t1\t1\t1\t1\t3", "5\t1\t1\t1\t1\t2")
            .as_bytes(),
        "duplicate_id",
    );
    tsv_rejection(
        text.replace("50\t10\t20", "950\t10\t20").as_bytes(),
        "bbox_outside_image",
    );
    tsv_rejection(
        text.replace("50\t10\t20", "500\t10\t20").as_bytes(),
        "bbox_outside_parent",
    );
    tsv_rejection(
        text.replace("50\t10\t20", "-1\t10\t20").as_bytes(),
        "unsigned_invalid",
    );
    tsv_rejection(
        text.replace("50\t10\t20", "18446744073709551616\t10\t20")
            .as_bytes(),
        "integer_overflow",
    );
    let mut rows: Vec<_> = text.split_inclusive('\n').collect();
    rows.swap(2, 3);
    tsv_rejection(rows.concat().as_bytes(), "parent_missing_or_order");
    tsv_rejection(
        text.replace("\t99.25\t0.05", "\t99.25").as_bytes(),
        "field_count",
    );
}
#[test]
fn replay_refuses_changed_samples_rows_order_and_removed_required_unknowns() {
    let raw = real_png();
    let p = Memory::with_tsv(&raw, FIXTURE);
    let b = binding(&raw);
    let limits = ContentLimits::default();
    let image = read_png_source(&p, &b, &limits).unwrap();
    let mut c = image.content().clone();
    c.unknown.clear();
    assert!(verify_png_content(&p, &b, &c, &limits).is_err());
    c = image.content().clone();
    c.decoded_sha256 = "0".repeat(64);
    assert!(verify_png_content(&p, &b, &c, &limits).is_err());
    let a = asset("selected/fixture.tsv", FIXTURE);
    let t = read_fixed_tsv_source(&p, &image, &a, &limits).unwrap();
    let mut changed = t.clone();
    changed.rows.swap(4, 5);
    assert!(verify_fixed_tsv_content(&p, &b, &a, &changed, &limits).is_err());
    let mut changed = t.clone();
    changed.rows[4].text = "adopted number".into();
    assert!(verify_fixed_tsv_content(&p, &b, &a, &changed, &limits).is_err());
    let mut changed = t.clone();
    changed.unknown.clear();
    assert!(verify_fixed_tsv_content(&p, &b, &a, &changed, &limits).is_err());
    let mut changed = t;
    changed.ocr_execution = "completed".into();
    assert!(changed.validate().is_err());
}
