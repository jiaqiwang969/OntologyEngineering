//! Tiny synthetic supplier originals; actual native Poppler, never a full library scan.
use oe_core::{SkillRoot, sha256};
use oe_knowledge::{
    index::ReadIndex,
    poppler::PopplerTools,
    sources::Provider,
    supplier_index::{self, BuildInput, BuildOptions, CATALOG_CODES},
};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    archive: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("var/maintenance/rust-rewrite-20260927/phase6-supplier-index-tests")
            .join(format!(
                "test-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        let archive = root.join("sources/original/archive");
        fs::create_dir_all(&archive).unwrap();
        fs::write(root.join("SKILL.md"), "synthetic supplier fixture").unwrap();
        for (ci, code) in CATALOG_CODES.iter().enumerate() {
            let pages = if ci == 0 { 2 } else { 1 };
            let base = archive.join(format!("tmp/pdfs/{code}"));
            fs::create_dir_all(base.join("pages")).unwrap();
            fs::create_dir_all(archive.join("下载记录")).unwrap();
            fs::create_dir_all(archive.join("output/pdf")).unwrap();
            fs::create_dir_all(archive.join("output/校验")).unwrap();
            let mut info = Vec::new();
            let mut manifest = Vec::new();
            for n in 1..=pages {
                let pid = ci * 100 + n;
                let filename = format!("{n:04}.pdf");
                let mut raw = unicode_pdf_pages(1);
                if n == 2 {
                    let start = raw.windows(3).position(|b| b == b"BT ").unwrap();
                    let end = raw[start..].windows(2).position(|b| b == b"ET").unwrap() + start + 2;
                    raw[start..end].fill(b' ');
                }
                fs::write(base.join("pages").join(&filename), &raw).unwrap();
                let hotspots = if ci == 0 && n == 1 {
                    json!([{"id":1,"type":"目次","keyword":"Declared 条件 heading","target":2,"x":12,"extra":{"retain":true}},{"id":2,"type":"形式Tpye","keyword":"SFJ12","target":"https://example.invalid/model","rectangle":[1,2,3,4]}])
                } else {
                    json!([])
                };
                info.push(json!({"id":pid,"pageNo":n,"pageNoReal":if n==1 {"封面"}else{"1"},"pdfUrl":filename,"hotArea":hotspots,"raw_extra":{"retained":"yes"}}));
                manifest.push(json!({"page_number":n,"page_id":pid,"filename":filename,"printed_page":if n==1 {"封面"}else{"1"},"bytes":raw.len(),"sha256":sha256(&raw),"url":"https://example.invalid/source.pdf"}));
            }
            let assembled = unicode_pdf_pages(pages);
            let rel = format!("output/pdf/{code}.pdf");
            fs::write(archive.join(&rel), &assembled).unwrap();
            save(
                &base.join("metadata.json"),
                &json!({"ebook":{"catalogCode":code,"catalogTitle":"Synthetic supplier目录","pages":{"info":info},"class":[{"id":1,"pageId":ci*100+1,"title":"Source 条件 chapter","unknown":"retain"}]}}),
            );
            save(
                &archive.join(format!("下载记录/{code}-manifest.json")),
                &json!(manifest),
            );
            let qc = format!("output/校验/{code}-qc.json");
            save(
                &archive.join(&qc),
                &json!({"status":"historical_observation_only"}),
            );
            save(
                &archive.join(format!("下载记录/{code}-assembly.json")),
                &json!({"book":code,"pages":pages,"file":rel,"sha256":sha256(assembled),"qc_report":qc}),
            );
        }
        Self { root, archive }
    }
    fn options(&self, name: &str) -> BuildOptions {
        BuildOptions {
            data_root: PathBuf::from(format!("sources/.indexes/{name}")),
            tools: PopplerTools {
                pdfinfo: "/opt/homebrew/bin/pdfinfo".into(),
                pdftotext: "/opt/homebrew/bin/pdftotext".into(),
                pdftoppm: "/opt/homebrew/bin/pdftoppm".into(),
            },
            ..Default::default()
        }
    }
    fn archive_build(&self, name: &str) -> anyhow::Result<Value> {
        supplier_index::build(
            &self.root,
            &BuildInput::Archive(self.archive.clone()),
            &self.options(name),
        )
    }
    fn page(&self) -> PathBuf {
        self.archive
            .join(format!("tmp/pdfs/{}/pages/0001.pdf", CATALOG_CODES[0]))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn save(p: &Path, v: &Value) {
    fs::write(p, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}
fn load(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn inventory(root: &Path) -> std::collections::BTreeMap<PathBuf, String> {
    fn visit(root: &Path, dir: &Path, found: &mut std::collections::BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), found);
            } else {
                found.insert(
                    entry.path().strip_prefix(root).unwrap().into(),
                    sha256(fs::read(entry.path()).unwrap()),
                );
            }
        }
    }
    let mut found = std::collections::BTreeMap::new();
    visit(root, root, &mut found);
    found
}
fn unicode_pdf_pages(count: usize) -> Vec<u8> {
    let cmap=b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Fixture def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange 6 beginbfchar <0001> <5DE5> <0002> <7A0B> <0003> <0020> <0004> <00650301> <0005> <D842DFB7> <0006> <67614EF6> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
    fn stream(bytes: &[u8]) -> Vec<u8> {
        let mut v = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
        v.extend(bytes);
        v.extend(b"\nendstream");
        v
    }
    let mut objs=vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),Vec::new(),b"<< /Type /Font /Subtype /Type0 /BaseFont /Arial /Encoding /Identity-H /DescendantFonts [4 0 R] /ToUnicode 6 0 R >>".to_vec(),b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Arial /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 5 0 R /DW 600 /CIDToGIDMap /Identity >>".to_vec(),b"<< /Type /FontDescriptor /FontName /Arial /Flags 32 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 >>".to_vec(),stream(cmap),b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\n\xff\x00\x00\nendstream".to_vec()];
    let mut kids = Vec::new();
    for content in [
        b"BT /F1 16 Tf 20 750 Td <00010002000300040003000500030006> Tj ET".as_slice(),
        b"",
        b"q 240 0 0 240 40 40 cm /Im0 Do Q",
    ]
    .into_iter()
    .take(count)
    {
        let id = objs.len() + 1;
        kids.push(format!("{id} 0 R"));
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 3 0 R >> /XObject << /Im0 7 0 R >> >> /Contents {} 0 R >>",id+1).into_bytes());
        objs.push(stream(content));
    }
    objs[1] = format!(
        "<< /Type /Pages /Count {count} /Kids [{}] >>",
        kids.join(" ")
    )
    .into_bytes();
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn shard(path: &Path, files: &[(String, Vec<u8>)]) -> Value {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let output = fs::File::create(path).unwrap();
    let xz = xz2::write::XzEncoder::new(output, 1);
    let mut tar = tar::Builder::new(xz);
    for (name, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        tar.append_data(&mut header, name, bytes.as_slice())
            .unwrap();
    }
    let xz = tar.into_inner().unwrap();
    xz.finish().unwrap();
    let raw = fs::read(path).unwrap();
    json!({"path":path.file_name().unwrap().to_str().unwrap(),"bytes":raw.len(),"sha256":sha256(raw),"files":files.iter().map(|(p,b)|json!({"path":p,"sha256":sha256(b),"bytes":b.len()})).collect::<Vec<_>>()})
}
fn pack(f: &Fixture) -> PathBuf {
    let dir = f.root.join("sources/downloaded");
    fs::create_dir(&dir).unwrap();
    let mut files = vec![(
        "catalog.sqlite3".into(),
        b"An old index is verified as bytes, never used as extracted text.".to_vec(),
    )];
    let mut identities = Vec::new();
    let mut pages = Vec::new();
    let mut assemblies = Vec::new();
    for code in CATALOG_CODES {
        let meta = format!("tmp/pdfs/{code}/metadata.json");
        let manifest = format!("下载记录/{code}-manifest.json");
        let assembly = format!("下载记录/{code}-assembly.json");
        let qc = format!("output/校验/{code}-qc.json");
        for name in [&meta, &manifest, &assembly, &qc] {
            files.push((
                format!("archive/{name}"),
                fs::read(f.archive.join(name)).unwrap(),
            ));
        }
        let a = load(&f.archive.join(&assembly));
        let full = fs::read(f.archive.join(a["file"].as_str().unwrap())).unwrap();
        identities.push(json!([
            code,
            sha256(fs::read(f.archive.join(&meta)).unwrap()),
            sha256(fs::read(f.archive.join(&manifest)).unwrap()),
            a["sha256"]
        ]));
        assemblies.push(json!({"catalog":code,"path":a["file"],"sha256":a["sha256"],"bytes":full.len(),"download_path":format!("full/{code}.pdf")}));
        for entry in load(&f.archive.join(&manifest)).as_array().unwrap() {
            let name = format!(
                "tmp/pdfs/{code}/pages/{}",
                entry["filename"].as_str().unwrap()
            );
            pages.push((name.clone(), fs::read(f.archive.join(name)).unwrap()));
        }
    }
    let metadata = shard(&dir.join("metadata.tar.xz"), &files);
    let mut segment = shard(&dir.join("segments/pages.tar.xz"), &pages);
    segment["path"] = json!("segments/pages.tar.xz");
    segment["id"] = json!("test-pages");
    let document = json!({"schema":"ontology-engineering.misumi-source-pack/v1","index":metadata["files"][0],"metadata":metadata,"source_catalogs":{"source_identity":identities,"counts":{"old_extractor":true},"index_fingerprint":"old-not-reused"},"segments":[segment],"assembled_pdfs":assemblies});
    save(&dir.join("source-pack.json"), &document);
    dir.join("source-pack.json")
}

#[test]
fn archive_preserves_schema_navigation_hotspots_raw_identity_and_actual_unicode_text() {
    let f = Fixture::new();
    let original = fs::read(f.page()).unwrap();
    let report = f.archive_build("archive").unwrap();
    assert_eq!(report["status"], "indexed");
    assert_eq!(report["index"]["text_origin"], "poppler_native_text");
    assert!(report["index"].get("pymupdf_version").is_none());
    assert_eq!(report["index"]["counts"]["pages"], 3);
    assert_eq!(report["index"]["counts"]["hotspots"], 2);
    assert_eq!(report["index"]["counts"]["native_text_missing"], 1);
    let dir = f.root.join("sources/.indexes/archive");
    let root = SkillRoot::new(&f.root).unwrap();
    let index = ReadIndex::open(&root, &dir, "misumi").unwrap();
    assert_eq!(index.identity()["sha256"], report["index_sha256"]);
    let db = Connection::open(dir.join("catalog.sqlite3")).unwrap();
    let (body,text_hash,raw_chars,printed,physical,source_json):(String,String,u64,String,u64,String)=db.query_row("SELECT body,text_sha256,raw_text_chars,printed_page,pdf_page,source_json FROM pages WHERE id=?",[format!("{}:page:1",CATALOG_CODES[0])],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert!(
        body.contains("工程") && body.contains("e\u{301}") && body.contains('𠮷'),
        "{body:?}"
    );
    assert_eq!(text_hash, sha256(&body));
    assert!(raw_chars < body.len() as u64);
    assert_eq!(printed, "封面");
    assert_eq!(physical, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&source_json).unwrap()["raw_extra"]["retained"],
        "yes"
    );
    let headings: String = db
        .query_row(
            "SELECT titles_json FROM pages WHERE id=?",
            [format!("{}:page:2", CATALOG_CODES[0])],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&headings).unwrap()[0],
        json!({"text":"Declared 条件 heading","source_page_id":1,"hotspot_id":1})
    );
    let hot: String = db
        .query_row(
            "SELECT source_json FROM hotspots WHERE ordinal=0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&hot).unwrap()["extra"]["retain"],
        true
    );
    assert_eq!(fs::read(f.page()).unwrap(), original);
    assert_eq!(report["engineering_parameter_verification"], "not_run");
    let location = load(&dir.join("source.json"));
    assert_eq!(location["index_sha256"], report["index_sha256"]);
    assert!(!location["archive_root"].as_str().unwrap().starts_with('/'));
    let provider = Provider::open(&root, "misumi", Some(&dir)).unwrap();
    assert_eq!(
        provider
            .page(&format!("{}:page:1", CATALOG_CODES[0]))
            .unwrap()["text_origin"],
        "poppler_native_text"
    );
}

#[test]
fn compressed_rebuild_never_uses_old_index_or_changes_pack_and_keeps_no_page_cache() {
    let f = Fixture::new();
    let manifest = pack(&f);
    let original = fs::read(&manifest).unwrap();
    let report = supplier_index::build(
        &f.root,
        &BuildInput::SourcePack(manifest.clone()),
        &f.options("pack"),
    )
    .unwrap();
    assert_eq!(report["index"]["counts"]["pages"], 3);
    assert_eq!(report["source_binding"]["old_index_text_reused"], false);
    assert_eq!(
        report["assembled_pdf_verification"][0]["status"],
        "not_installed_not_verified"
    );
    assert_eq!(fs::read(&manifest).unwrap(), original);
    assert!(!manifest.parent().unwrap().join("cache/archive").exists());
    assert!(!f.root.join("var/cache/knowledge/source-pages").exists());
    let location = load(&f.root.join("sources/.indexes/pack/source.json"));
    assert_eq!(location["source_pack_manifest_sha256"], sha256(original));
    assert_ne!(location["index_sha256"], load(&manifest)["index"]["sha256"]);
    let provider = Provider::open(
        &SkillRoot::new(&f.root).unwrap(),
        "misumi",
        Some(Path::new("sources/.indexes/pack")),
    )
    .unwrap();
    assert_eq!(
        provider
            .page(&format!("{}:page:1", CATALOG_CODES[0]))
            .unwrap()["text_origin"],
        "poppler_native_text"
    );
}

#[test]
fn installed_pack_without_metadata_transport_verifies_every_expanded_member_and_never_masks_bad_tar()
 {
    for mode in [
        "valid",
        "changed_old_index",
        "changed_json",
        "present_broken_tar",
    ] {
        let f = Fixture::new();
        let manifest_path = pack(&f);
        let manifest = load(&manifest_path);
        let pack_root = manifest_path.parent().unwrap();
        for entry in manifest["metadata"]["files"].as_array().unwrap() {
            let relative = entry["path"].as_str().unwrap();
            let path = pack_root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let bytes = if relative == "catalog.sqlite3" {
                b"An old index is verified as bytes, never used as extracted text.".to_vec()
            } else {
                fs::read(f.archive.join(relative.strip_prefix("archive/").unwrap())).unwrap()
            };
            fs::write(path, bytes).unwrap();
        }
        let archive_path = pack_root.join(manifest["metadata"]["path"].as_str().unwrap());
        fs::remove_file(&archive_path).unwrap();
        match mode {
            "changed_old_index" => {
                fs::write(pack_root.join("catalog.sqlite3"), b"changed old bytes").unwrap()
            }
            "changed_json" => fs::write(
                pack_root.join(format!(
                    "archive/tmp/pdfs/{}/metadata.json",
                    CATALOG_CODES[0]
                )),
                b"{}",
            )
            .unwrap(),
            "present_broken_tar" => {
                fs::write(archive_path, b"present but corrupt transport").unwrap()
            }
            _ => {}
        }
        let observed = inventory(pack_root);
        let result = supplier_index::build(
            &f.root,
            &BuildInput::SourcePack(manifest_path.clone()),
            &f.options("installed"),
        );
        if mode == "valid" {
            let report = result.unwrap();
            assert_eq!(
                report["source_binding"]["metadata_transport"],
                "installed_verified_members"
            );
            assert_eq!(report["source_binding"]["old_index_text_reused"], false);
            let root = SkillRoot::new(&f.root).unwrap();
            let provider = Provider::open(
                &root,
                "misumi",
                Some(Path::new("sources/.indexes/installed")),
            )
            .unwrap();
            assert!(!provider.retrieve("SFJ12", 4, &[]).unwrap().is_empty());
        } else {
            assert!(result.is_err(), "{mode}");
            assert!(!f.root.join("sources/.indexes/installed").exists());
        }
        assert_eq!(inventory(pack_root), observed);
    }
}

#[test]
fn new_output_is_exclusive_and_failed_explicit_rebuild_preserves_owned_snapshot() {
    let f = Fixture::new();
    f.archive_build("owned").unwrap();
    let dir = f.root.join("sources/.indexes/owned");
    let before = (
        fs::read(dir.join("catalog.sqlite3")).unwrap(),
        fs::read(dir.join("source.json")).unwrap(),
    );
    assert!(
        f.archive_build("owned")
            .unwrap_err()
            .to_string()
            .contains("destination_exists")
    );
    let mut options = f.options("owned");
    options.replace = true;
    fs::write(f.page(), b"corrupted original").unwrap();
    assert!(
        supplier_index::build(&f.root, &BuildInput::Archive(f.archive.clone()), &options).is_err()
    );
    assert_eq!(fs::read(dir.join("catalog.sqlite3")).unwrap(), before.0);
    assert_eq!(fs::read(dir.join("source.json")).unwrap(), before.1);
}

#[test]
fn malformed_mapping_qc_path_or_page_identity_never_publishes_partial_index() {
    for mode in [
        "duplicate_id",
        "duplicate_number",
        "printed_page",
        "chapter_target",
        "traversal",
        "missing_qc",
        "source_hash",
        "assembly_hash",
    ] {
        let f = Fixture::new();
        let meta = f
            .archive
            .join(format!("tmp/pdfs/{}/metadata.json", CATALOG_CODES[0]));
        let manifest = f
            .archive
            .join(format!("下载记录/{}-manifest.json", CATALOG_CODES[0]));
        let mut v = load(&meta);
        let mut m = load(&manifest);
        match mode {
            "duplicate_id" => v["ebook"]["pages"]["info"][1]["id"] = json!(1),
            "duplicate_number" => v["ebook"]["pages"]["info"][1]["pageNo"] = json!(1),
            "printed_page" => m[0]["printed_page"] = json!("foreign"),
            "chapter_target" => v["ebook"]["class"][0]["pageId"] = json!(999),
            "traversal" => v["ebook"]["pages"]["info"][0]["pdfUrl"] = json!("../outside.pdf"),
            "missing_qc" => {
                fs::remove_file(
                    f.archive
                        .join(format!("output/校验/{}-qc.json", CATALOG_CODES[0])),
                )
                .unwrap();
            }
            "source_hash" => m[0]["sha256"] = json!("0".repeat(64)),
            "assembly_hash" => {
                fs::write(
                    f.archive
                        .join(format!("output/pdf/{}.pdf", CATALOG_CODES[0])),
                    b"changed",
                )
                .unwrap();
            }
            _ => unreachable!(),
        };
        save(&meta, &v);
        save(&manifest, &m);
        assert!(f.archive_build("invalid").is_err(), "{mode}");
        assert!(!f.root.join("sources/.indexes/invalid").exists(), "{mode}");
    }
}

#[test]
fn explicit_owned_rebuild_replaces_both_files_and_retains_originals() {
    let f = Fixture::new();
    let first = f.archive_build("owned").unwrap();
    let meta = f
        .archive
        .join(format!("tmp/pdfs/{}/metadata.json", CATALOG_CODES[0]));
    let mut changed = load(&meta);
    changed["ebook"]["class"][0]["title"] = json!("Revised declared navigation only");
    save(&meta, &changed);
    let originals = inventory(&f.archive);
    let mut options = f.options("owned");
    options.replace = true;
    let second =
        supplier_index::build(&f.root, &BuildInput::Archive(f.archive.clone()), &options).unwrap();
    assert_ne!(first["index_sha256"], second["index_sha256"]);
    let location = load(&f.root.join("sources/.indexes/owned/source.json"));
    assert_eq!(location["index_sha256"], second["index_sha256"]);
    let root = SkillRoot::new(&f.root).unwrap();
    let provider =
        Provider::open(&root, "misumi", Some(Path::new("sources/.indexes/owned"))).unwrap();
    let page = provider
        .page(&format!("{}:page:1", CATALOG_CODES[0]))
        .unwrap();
    assert_eq!(page["chapter"], "Revised declared navigation only");
    assert_eq!(inventory(&f.archive), originals);
}

#[test]
fn source_and_destination_policy_rejects_private_originals_symlinks_and_foreign_owned_files() {
    for mode in [
        "private_originals",
        "symlink_page",
        "foreign_output",
        "outside_indexes",
    ] {
        let mut f = Fixture::new();
        let mut options = f.options("invalid-policy");
        match mode {
            "private_originals" => {
                let moved = f.root.join("var/private-originals");
                fs::create_dir_all(moved.parent().unwrap()).unwrap();
                fs::rename(&f.archive, &moved).unwrap();
                f.archive = moved;
            }
            "symlink_page" => {
                let original = f.page();
                let replacement = original.with_extension("saved");
                fs::rename(&original, &replacement).unwrap();
                std::os::unix::fs::symlink(replacement, original).unwrap();
            }
            "foreign_output" => {
                f.archive_build("invalid-policy").unwrap();
                fs::write(
                    f.root.join("sources/.indexes/invalid-policy/foreign.txt"),
                    b"keep",
                )
                .unwrap();
                options.replace = true;
            }
            "outside_indexes" => options.data_root = "sources/invalid-policy".into(),
            _ => unreachable!(),
        }
        assert!(
            supplier_index::build(&f.root, &BuildInput::Archive(f.archive.clone()), &options)
                .is_err(),
            "{mode}"
        );
        if mode == "foreign_output" {
            assert_eq!(
                fs::read(f.root.join("sources/.indexes/invalid-policy/foreign.txt")).unwrap(),
                b"keep"
            );
        } else {
            assert!(!f.root.join(options.data_root).exists(), "{mode}");
        }
    }
}

fn project_context(root: &SkillRoot) -> Value {
    use oe_knowledge::context;
    let path = root.path().join("var/projects/supplier/source.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    save(
        &path,
        &json!({"project":{"id":"synthetic-project","revision":"r1"},"object":{"id":"synthetic-object","revision":"r1"},"decision_id":"read-source","goal":"Read original supplier conditions before applying them"}),
    );
    let fields = [
        "/project/id",
        "/project/revision",
        "/object/id",
        "/object/revision",
        "/decision_id",
        "/goal",
    ]
    .map(|p| json!({"source":"var/projects/supplier/source.json","pointer":p,"target":p}));
    context::project(root,&json!({"schema":context::PROJECTION_SCHEMA,"base":{"schema":context::SCHEMA,"task_id":"synthetic-supplier-handoff"},"fields":fields})).unwrap()
}

#[test]
fn both_source_modes_reach_verified_packets_after_whole_library_relocation() {
    use oe_knowledge::packet;
    for mode in ["archive", "pack"] {
        let mut f = Fixture::new();
        let input = if mode == "archive" {
            BuildInput::Archive(f.archive.clone())
        } else {
            BuildInput::SourcePack(pack(&f))
        };
        let options = f.options("handoff");
        supplier_index::build(&f.root, &input, &options).unwrap();
        let root = SkillRoot::new(&f.root).unwrap();
        let context = project_context(&root);
        let provider =
            Provider::open(&root, "misumi", Some(Path::new("sources/.indexes/handoff"))).unwrap();
        let retrieved = provider.retrieve("SFJ12", 4, &[]).unwrap();
        assert!(!retrieved.is_empty());
        let mut result = provider
            .source_result(&retrieved[0], "工程", &options.tools)
            .unwrap();
        assert_eq!(result["text_origin"], "poppler_native_text");
        assert_eq!(result["original_verified"], true);
        result["native_text"] = result["body"].clone();
        result["judgment"] = json!({"origin":"synthetic_handoff_only","relation":"not_assessed"});
        result["knowledge_units"] = json!([]);
        let report = json!({"schema":"ontology-engineering.knowledge-search-result/v1","status":"completed","query":"工程","context_sha256":oe_core::json::canonical_digest(&context).unwrap(),"parameter_verification":"not_run","run_id":"synthetic-no-model","results":[result]});
        let packet = packet::make_with_tools(&root, &report, &context, &options.tools).unwrap();
        assert_eq!(
            packet::verify_with_tools(&root, &packet, &context, &options.tools).unwrap()["status"],
            "ready_for_agent_review"
        );
        assert_eq!(packet["semantic_review"], "not_run");
        assert_eq!(packet["adoption"], "pending_agent_review");
        drop(provider);
        drop(root);
        let old = f.root.clone();
        let moved = old.with_file_name(format!(
            "{}-relocated",
            old.file_name().unwrap().to_str().unwrap()
        ));
        fs::rename(&old, &moved).unwrap();
        f.root = moved;
        f.archive = f.root.join("sources/original/archive");
        assert!(!old.exists());
        let receiver = SkillRoot::new(&f.root).unwrap();
        let current = project_context(&receiver);
        assert_eq!(current, context);
        let provider = Provider::open(
            &receiver,
            "misumi",
            Some(Path::new("sources/.indexes/handoff")),
        )
        .unwrap();
        let page = provider
            .page(&format!("{}:page:1", CATALOG_CODES[0]))
            .unwrap();
        let evidence = provider
            .source_result(&page, "工程", &options.tools)
            .unwrap();
        assert!(
            evidence["source_pdf"]
                .as_str()
                .unwrap()
                .starts_with(if mode == "archive" {
                    "sources/"
                } else {
                    "var/"
                })
        );
        assert_eq!(
            packet::verify_with_tools(&receiver, &packet, &current, &options.tools).unwrap()["status"],
            "ready_for_agent_review"
        );
        if mode == "pack" {
            // The index keeps its source kind; removing its independent pack
            // binding cannot relabel compressed originals as an expanded archive.
            let location_path = f.root.join("sources/.indexes/handoff/source.json");
            let mut location = load(&location_path);
            let fields = location.as_object_mut().unwrap();
            fields.remove("source_pack_manifest");
            fields.remove("source_pack_manifest_sha256");
            save(&location_path, &location);
            assert!(
                Provider::open(
                    &receiver,
                    "misumi",
                    Some(Path::new("sources/.indexes/handoff"))
                )
                .is_err()
            );
        }
    }
}

#[test]
fn late_compressed_corruption_never_publishes_or_expands_all_pages() {
    let f = Fixture::new();
    let manifest = pack(&f);
    let mut value = load(&manifest);
    let shard_path = manifest.parent().unwrap().join("segments/pages.tar.xz");
    let mut raw = fs::read(&shard_path).unwrap();
    raw.truncate(raw.len() - 8);
    fs::write(&shard_path, &raw).unwrap();
    // Even a self-consistently rehashed outer descriptor cannot hide a bad XZ footer.
    value["segments"][0]["bytes"] = json!(raw.len());
    value["segments"][0]["sha256"] = json!(sha256(&raw));
    save(&manifest, &value);
    assert!(
        supplier_index::build(
            &f.root,
            &BuildInput::SourcePack(manifest),
            &f.options("corrupt")
        )
        .is_err()
    );
    assert!(!f.root.join("sources/.indexes/corrupt").exists());
    assert!(!f.root.join("var/cache/knowledge/source-pages").exists());
}
