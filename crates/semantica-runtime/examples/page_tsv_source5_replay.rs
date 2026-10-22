//! Source-only replay of explicit bytes; no OCR/process or opaque authority.
use anyhow::{Result, ensure};
use semantica_contracts::{SemanticContentAsset3, SemanticPageImageBinding3};
use semantica_runtime::{
    books::SourceProvider,
    page_content3::{ContentLimits, read_png_source},
    page_content5::{read_tsv_source5, verify_tsv_source5},
};
use std::{collections::BTreeMap, path::Path};
struct ExactSelected(BTreeMap<String, Vec<u8>>);
impl SourceProvider for ExactSelected {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        self.0
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unselected_source"))
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 6,
        "usage: originalPNG rawTSV expectedTSVSHA expected_noncontaining_ordinals_JSON newoutJSON"
    );
    let png = std::fs::read(&args[1])?;
    let raw = std::fs::read(&args[2])?;
    ensure!(
        semantica_runtime::sha256(&png)
            == "6c9b8ebbbf1c39a2d0b5b728a4e1fa91b0b4efa381d28349ad89f2eae274cbc6"
            && semantica_runtime::sha256(&raw) == args[3],
        "frozen_raw_source_identity"
    );
    let asset = |id: &str, b: &[u8]| SemanticContentAsset3 {
        logical_id: id.into(),
        sha256: semantica_runtime::sha256(b),
        bytes: b.len() as u64,
    };
    let binding = SemanticPageImageBinding3 {
        schema: "semantica.page-image-binding/3".into(),
        document_sha256: "baf6363d11afb072bdb960c9a735821393771f213e7495f981ee97c3d9732259".into(),
        document_bytes: 35986251,
        physical_page: 188,
        page_count: 201,
        dpi: 144,
        image_asset: asset("selected/image.png", &png),
    };
    let a = asset("selected/raw.tsv", &raw);
    let p = ExactSelected(BTreeMap::from([
        ("selected/image.png".into(), png),
        ("selected/raw.tsv".into(), raw.clone()),
    ]));
    let image = read_png_source(&p, &binding, &ContentLimits::default())?;
    let c = read_tsv_source5(&p, &image, &a, &ContentLimits::default())?;
    let expected: Vec<u64> = serde_json::from_slice(&std::fs::read(&args[4])?)?;
    ensure!(
        c.noncontaining_parent_rows == expected,
        "independent_geometry_ordinal_observation"
    );
    let mut all = c.header_raw.clone();
    for (i, r) in c.rows.iter().enumerate() {
        ensure!(
            r.ordinal == i as u64 && r.byte_offset == all.len() as u64,
            "original_raw_offsets"
        );
        all.push_str(&r.raw_line);
    }
    ensure!(all.as_bytes() == raw, "complete_raw_source_losslessness");
    verify_tsv_source5(&p, &binding, &a, &c, &ContentLimits::default())?;
    ensure!(
        c.execution_authority == "not_owned_by_content_library"
            && c.qualification == "source_content_only",
        "source_not_execution_authority"
    );
    use std::io::Write;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(&args[5]))?;
    out.write_all(&serde_json::to_vec_pretty(&serde_json::json!({"schema":"private.stage3.source-only-original-raw-replay/1","content":c,"actual_ocr_here":"not_run","production_factory_here":"not_run","opaque5_reconstructed":false,"original_failed_factory":"retained","engineering_adoption":"not_run","whole_qualification":"unknown"}))?)?;
    out.write_all(b"\n")?;
    println!(
        "source-only replay preserved {} original bytes; no actual factory authority",
        raw.len()
    );
    Ok(())
}
