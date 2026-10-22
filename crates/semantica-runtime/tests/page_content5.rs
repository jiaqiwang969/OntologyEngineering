use semantica_contracts::{SemanticContentAsset3, SemanticPageImageBinding3};
use semantica_runtime::{
    books::SourceProvider,
    page_content3::{ContentLimits, read_fixed_tsv_source, read_png_source},
    page_content5::{read_tsv_source5, verify_tsv_source5},
};
use std::collections::BTreeMap;
const RAW: &[u8] = include_bytes!("fixtures/page-content5/complete-independent-synthetic.tsv");
const MISSING_PARENT: &[u8] = include_bytes!("fixtures/page-content5/missing-parent-original.tsv");
struct Selected(BTreeMap<String, Vec<u8>>);
impl SourceProvider for Selected {
    fn read(&self, id: &str) -> anyhow::Result<Vec<u8>> {
        self.0
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("not_selected"))
    }
}
fn asset(id: &str, b: &[u8]) -> SemanticContentAsset3 {
    SemanticContentAsset3 {
        logical_id: id.into(),
        bytes: b.len() as u64,
        sha256: semantica_runtime::sha256(b),
    }
}
fn context(raw: &[u8]) -> (Selected, SemanticPageImageBinding3, SemanticContentAsset3) {
    let png =
        std::fs::read(std::env::var("SEMANTICA_STAGE1_PNG").expect("original frozen PNG required"))
            .unwrap();
    assert_eq!(
        semantica_runtime::sha256(&png),
        "6c9b8ebbbf1c39a2d0b5b728a4e1fa91b0b4efa381d28349ad89f2eae274cbc6"
    );
    let binding = SemanticPageImageBinding3 {
        schema: "semantica.page-image-binding/3".into(),
        document_sha256: "baf6363d11afb072bdb960c9a735821393771f213e7495f981ee97c3d9732259".into(),
        document_bytes: 35986251,
        physical_page: 188,
        page_count: 201,
        dpi: 144,
        image_asset: asset("selected/image.png", &png),
    };
    (
        Selected(BTreeMap::from([
            ("selected/image.png".into(), png),
            ("selected/raw.tsv".into(), raw.to_vec()),
        ])),
        binding,
        asset("selected/raw.tsv", raw),
    )
}
#[test]
fn source_tsv5_retains_all_raw_rows_and_unknown_confidence() {
    let (p, b, a) = context(RAW);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    let content = read_tsv_source5(&p, &image, &a, &ContentLimits::default()).unwrap();
    assert_eq!(content.rows.len(), 7);
    assert_eq!(content.word_text_in_order, ["测量", "", "A0"]);
    assert_eq!(content.rows[4].confidence, Some(4.25));
    assert_eq!(content.rows[5].confidence, Some(94.18));
    assert_eq!(content.rows[6].confidence, None);
    assert_eq!(content.rows[6].confidence_raw, "-1");
    assert_eq!(content.rows[5].text, "");
    let mut complete = content.header_raw.clone();
    for (i, r) in content.rows.iter().enumerate() {
        assert_eq!(r.ordinal, i as u64);
        assert_eq!(r.byte_offset, complete.len() as u64);
        assert_eq!(r.page_num, 1);
        complete.push_str(&r.raw_line);
    }
    assert_eq!(complete.as_bytes(), RAW);
    assert!(!complete.ends_with('\n'));
    assert_eq!(content.image.binding.physical_page, 188);
    assert_eq!(content.execution_authority, "not_owned_by_content_library");
    assert_eq!(content.qualification, "source_content_only");
    verify_tsv_source5(&p, &b, &a, &content, &ContentLimits::default()).unwrap();
    let mut changed = content.clone();
    changed.unknown.clear();
    assert!(verify_tsv_source5(&p, &b, &a, &changed, &ContentLimits::default()).is_err());
}
#[test]
fn old_fixture3_meaning_is_preserved_with_same_raw_parser() {
    let raw = include_bytes!("fixtures/page-content3/fixed-eight-rows.tsv");
    let (p, b, a) = context(raw);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    let old = read_fixed_tsv_source(&p, &image, &a, &ContentLimits::default()).unwrap();
    let new = read_tsv_source5(&p, &image, &a, &ContentLimits::default()).unwrap();
    assert_eq!(old.schema, "semantica.ocr-tsv-fixture-content/3");
    assert_eq!(old.ocr_execution, "not_run");
    assert_eq!(old.qualification, "fixed_fixture_content_only");
    assert_eq!(old.unknown.len(), 8);
    assert_eq!(old.rows, new.rows);
    assert_eq!(old.header_raw, new.header_raw);
    assert_ne!(old.schema, new.schema);
}
#[test]
fn legal_bound_source_counterexamples_reach_the_actual_tsv_branch() {
    let text = std::str::from_utf8(RAW).unwrap();
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (MISSING_PARENT.to_vec(), "parent_missing"),
        (
            text.replacen("1\t1\t0\t0\t0\t0", "1\t188\t0\t0\t0\t0", 1)
                .into_bytes(),
            "single_image_page",
        ),
        (
            text.replacen("10\t20\t20\t10\t4.25", "-1\t20\t20\t10\t4.25", 1)
                .into_bytes(),
            "unsigned",
        ),
        (
            text.replacen("10\t20\t20\t10\t4.25", "940\t20\t20\t10\t4.25", 1)
                .into_bytes(),
            "bbox_outside_image",
        ),
        (text.replace("4.25", "101").into_bytes(), "confidence"),
        (text.replace("4.25", "NaN").into_bytes(), "confidence"),
        (text.replace("测量", "测\t量").into_bytes(), "field_count"),
        (text.replace("测量", "测\0量").into_bytes(), "NUL"),
    ];
    for (raw, reason) in cases {
        let (p, b, a) = context(&raw);
        let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
        let e = read_tsv_source5(&p, &image, &a, &ContentLimits::default())
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains(reason), "{reason}:{e}");
    }
    let (p, b, a) = context(RAW);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    for limits in [
        ContentLimits {
            tsv_bytes: RAW.len() - 1,
            ..Default::default()
        },
        ContentLimits {
            line_bytes: 16,
            ..Default::default()
        },
        ContentLimits {
            rows: 6,
            ..Default::default()
        },
    ] {
        assert!(read_tsv_source5(&p, &image, &a, &limits).is_err());
    }
}

#[test]
fn source_hierarchy_preserves_reported_noncontaining_boxes_without_weakening_fixture3() {
    let raw = include_bytes!("fixtures/page-content5/complete-noncontaining-parent.tsv");
    let (p, b, a) = context(raw);
    let image = read_png_source(&p, &b, &ContentLimits::default()).unwrap();
    let error = read_fixed_tsv_source(&p, &image, &a, &ContentLimits::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("tsv_bbox_outside_parent"));
    let content = read_tsv_source5(&p, &image, &a, &ContentLimits::default()).unwrap();
    assert_eq!(content.rows.len(), 7);
    assert_eq!(content.noncontaining_parent_rows, [5, 6]);
    let complete = format!(
        "{}{}",
        content.header_raw,
        content
            .rows
            .iter()
            .map(|r| r.raw_line.as_str())
            .collect::<String>()
    );
    assert_eq!(complete.as_bytes(), raw);
    assert_eq!((content.rows[3].width, content.rows[3].height), (40, 40));
    assert_eq!((content.rows[5].left, content.rows[5].width), (35, 20));
    assert_eq!(content.qualification, "source_content_only");
    verify_tsv_source5(&p, &b, &a, &content, &ContentLimits::default()).unwrap();
    let mut changed = content.clone();
    changed.noncontaining_parent_rows.clear();
    assert!(verify_tsv_source5(&p, &b, &a, &changed, &ContentLimits::default()).is_err());
}
