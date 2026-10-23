//! Owned, tiny native fixtures. No existing book/index or external API is touched.
use oe_core::{SkillRoot, sha256};
use oe_knowledge::{
    citations,
    poppler::PopplerTools,
    registration::{self, RegistrationOptions},
    source_pack,
    sources::Provider,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("var/maintenance/rust-rewrite-20260927/phase5-source-tests")
            .join(format!(
                "{label}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(root.join("sources/books")).unwrap();
        fs::write(root.join("SKILL.md"), "fixture only").unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn tools() -> PopplerTools {
    PopplerTools {
        pdfinfo: "/opt/homebrew/bin/pdfinfo".into(),
        pdftotext: "/opt/homebrew/bin/pdftotext".into(),
        pdftoppm: "/opt/homebrew/bin/pdftoppm".into(),
    }
}
fn save(path: &Path, v: &Value) {
    fs::write(path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}

/// Actual text-layer glyph-to-Unicode mappings include Chinese, a decomposed
/// combining sequence and an astral CJK character. Page 2 is blank; page 3 is a
/// bitmap without a native text layer. No directory alias proves these claims.
fn unicode_pdf() -> Vec<u8> {
    unicode_pdf_pages(3)
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
fn manifest(f: &Fixture) -> (PathBuf, Value, PathBuf) {
    let pdf = f.0.join("sources/books/中文-original.pdf");
    let bytes = unicode_pdf();
    fs::write(&pdf, &bytes).unwrap();
    let m = json!({"schema":"ontology-engineering.local-pdf-manifest/v1","sources":[{"id":"unicode","title":"Synthetic Unicode","edition":"fixture-1","path":"中文-original.pdf","sha256":sha256(&bytes),"chapters":[{"id":"conditions","title":"Declared navigation","start_page":1,"end_page":3,"aliases":["alias-not-in-body"]}]}]});
    let path = pdf.with_file_name("manifest.json");
    save(&path, &m);
    (path, m, pdf)
}
fn options() -> RegistrationOptions {
    RegistrationOptions {
        tools: tools(),
        ..Default::default()
    }
}

#[test]
fn actual_unicode_registration_all_physical_pages_and_whole_page_view() {
    let f = Fixture::new("unicode");
    let (path, _, pdf) = manifest(&f);
    let before = fs::read(&pdf).unwrap();
    let report = registration::register(&f.0, &path, &options()).unwrap();
    assert_eq!(report["index"]["counts"]["pages"], 3);
    assert_eq!(report["index"]["counts"]["native_text_missing"], 2);
    let root = SkillRoot::new(&f.0).unwrap();
    let provider = Provider::open(&root, "local_pdf", None).unwrap();
    let page = provider.page("local_pdf:unicode:page:1").unwrap();
    let body = page["body"].as_str().unwrap();
    assert!(body.contains("工程"), "{body:?}");
    assert!(body.contains("e\u{301}"), "{body:?}");
    assert!(body.contains('𠮷'), "{body:?}");
    assert!(!body.contains("alias-not-in-body"));
    assert_eq!(
        page["text_chars"].as_u64(),
        Some(body.chars().count() as u64)
    );
    assert_ne!(body.len(), body.chars().count());
    let evidence = provider.source_result(&page, "工程", &tools()).unwrap();
    assert_eq!(evidence["source_locator"]["physical_page"], 1);
    assert!(evidence["printed_page"].is_null());
    let view = citations::view(&f.0, &evidence, 72, &tools()).unwrap();
    assert_eq!(view["identity"]["whole_page"], true);
    assert_eq!(view["width"], 600);
    assert_eq!(view["height"], 800);
    assert_eq!(view["numeric_verification"], "not_run");
    assert!(
        view["identity"]["renderer_identity"]["sha256"]
            .as_str()
            .unwrap()
            .len()
            == 64
    );
    for n in [2, 3] {
        let page = provider
            .page(&format!("local_pdf:unicode:page:{n}"))
            .unwrap();
        assert_eq!(page["native_text_missing"], true);
        let evidence = provider.source_result(&page, "", &tools()).unwrap();
        let view = citations::view(&f.0, &evidence, 72, &tools()).unwrap();
        assert_eq!(view["identity"]["source_file_page"], n);
        assert_eq!(view["width"], 600);
    }
    assert_eq!(fs::read(&pdf).unwrap(), before);
    let registration: Value = serde_json::from_slice(
        &fs::read(f.0.join("sources/.indexes/local-pdf/registration.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        registration["sources"][0]["path"],
        "../../books/中文-original.pdf"
    );
    let mut forged = page.clone();
    forged["body"] = json!("forged");
    assert!(provider.source_result(&forged, "", &tools()).is_err());
    fs::write(&pdf, b"changed").unwrap();
    assert!(provider.source_result(&page, "", &tools()).is_err());
}

#[test]
fn replacement_is_explicit_and_extraction_failure_retains_previous_bytes() {
    let f = Fixture::new("replace");
    let (path, mut m, _) = manifest(&f);
    registration::register(&f.0, &path, &options()).unwrap();
    let index = f.0.join("sources/.indexes/local-pdf");
    let old_db = fs::read(index.join("catalog.sqlite3")).unwrap();
    let old_registration = fs::read(index.join("registration.json")).unwrap();
    assert!(registration::register(&f.0, &path, &options()).is_err());
    m["sources"][0]["sha256"] = json!("0".repeat(64));
    save(&path, &m);
    let mut opts = options();
    opts.replace = true;
    assert!(registration::register(&f.0, &path, &opts).is_err());
    assert_eq!(fs::read(index.join("catalog.sqlite3")).unwrap(), old_db);
    assert_eq!(
        fs::read(index.join("registration.json")).unwrap(),
        old_registration
    );
    m["sources"][0]["sha256"] = json!(sha256(unicode_pdf()));
    m["sources"][0]["edition"] = json!("fixture-2");
    save(&path, &m);
    assert_eq!(
        registration::register(&f.0, &path, &opts).unwrap()["status"],
        "registered"
    );
    assert_ne!(
        fs::read(index.join("registration.json")).unwrap(),
        old_registration
    );
}

#[cfg(unix)]
fn script(f: &Fixture, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = f.0.join(name);
    fs::write(&p,format!("#!/bin/sh\nif [ \"$1\" = '-v' ]; then echo 'fixture version 1' >&2; exit 0; fi\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
    p
}
#[test]
#[cfg(unix)]
fn subprocess_missing_timeout_output_limit_and_page_count_failure_are_bounded() {
    let f = Fixture::new("process");
    let (path, _, _) = manifest(&f);
    let mut opts = options();
    opts.tools.pdfinfo = f.0.join("missing");
    assert!(registration::register(&f.0, &path, &opts).is_err());
    opts.tools.pdfinfo = script(&f, "sleeper", "/bin/sleep 5 &\nexit 0");
    opts.limits.timeout = Duration::from_millis(50);
    let start = std::time::Instant::now();
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("timeout")
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    opts = options();
    opts.limits.output_bytes = 4;
    assert!(
        format!(
            "{:#}",
            registration::register(&f.0, &path, &opts).unwrap_err()
        )
        .contains("stdout_limit")
    );
    opts = options();
    opts.tools.pdfinfo = script(&f, "count", "printf 'Pages: 4\\nEncrypted: no\\n'");
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("page_count_mismatch")
    );
    opts = options();
    opts.limits.page_bytes = 3;
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("page_output_limit")
    );
    assert!(!f.0.join("sources/.indexes/local-pdf").exists());
}

fn shard(path: &Path, members: &[(&str, &[u8], u8)]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = fs::File::create(path).unwrap();
    let encoder = xz2::write::XzEncoder::new(file, 6);
    let mut tar = tar::Builder::new(encoder);
    for (name, data, kind) in members {
        let mut header = tar::Header::new_gnu();
        let bytes = header.as_mut_bytes();
        bytes[..100].fill(0);
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        header.set_mode(0o600);
        header.set_size(data.len() as u64);
        header.set_entry_type(tar::EntryType::new(*kind));
        if *kind == b'2' {
            header.set_link_name("../../outside").unwrap();
        }
        header.set_cksum();
        tar.append(&header, *data).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap();
}
fn pack(f: &Fixture) -> (PathBuf, Value, Vec<u8>) {
    let root = f.0.join("sources/.indexes/misumi");
    let selected = unicode_pdf();
    let segment = root.join("segments/a.tar.xz");
    shard(
        &segment,
        &[
            ("pages/selected.pdf", &selected, b'0'),
            ("pages/other.pdf", b"other exact bytes", b'0'),
        ],
    );
    let index = b"synthetic index identity";
    fs::write(root.join("catalog.sqlite3"), index).unwrap();
    let compressed = fs::read(&segment).unwrap();
    let manifest = json!({"schema":"ontology-engineering.misumi-source-pack/v1","index":{"path":"catalog.sqlite3","bytes":index.len(),"sha256":sha256(index)},"segments":[{"id":"a","path":"segments/a.tar.xz","bytes":compressed.len(),"sha256":sha256(&compressed),"files":[{"path":"pages/selected.pdf","bytes":selected.len(),"sha256":sha256(&selected)},{"path":"pages/other.pdf","bytes":17,"sha256":sha256(b"other exact bytes")}]}]});
    let path = root.join("source-pack.json");
    save(&path, &manifest);
    (path, manifest, selected)
}
#[test]
fn selected_tar_xz_recovery_is_exact_and_conflicting_cache_is_never_overwritten() {
    let f = Fixture::new("pack");
    let (path, _, selected) = pack(&f);
    let sha = sha256(&selected);
    let restored = source_pack::restore_selected(&f.0, &path, "pages/selected.pdf", &sha).unwrap();
    assert_eq!(fs::read(&restored.path).unwrap(), selected);
    assert_eq!(restored.source_file_page, 1);
    assert!(!restored.path.with_file_name("other.pdf").exists());
    assert_eq!(
        source_pack::restore_selected(&f.0, &path, "pages/selected.pdf", &sha)
            .unwrap()
            .path,
        restored.path
    );
    fs::write(&restored.path, b"collision").unwrap();
    assert!(source_pack::restore_selected(&f.0, &path, "pages/selected.pdf", &sha).is_err());
    assert_eq!(fs::read(&restored.path).unwrap(), b"collision");
}
#[test]
fn archive_damage_and_hidden_members_never_publish_selected_bytes() {
    for damage in [
        "hash",
        "page_hash",
        "traversal",
        "symlink",
        "sparse",
        "duplicate",
        "missing",
        "undeclared",
    ] {
        let f = Fixture::new(damage);
        let (path, mut m, selected) = pack(&f);
        let segment = path.parent().unwrap().join("segments/a.tar.xz");
        let mut members = vec![
            ("pages/selected.pdf", selected.as_slice(), b'0'),
            ("pages/other.pdf", b"other exact bytes".as_slice(), b'0'),
        ];
        match damage {
            "hash" => {
                fs::write(&segment, b"bad xz").unwrap();
            }
            "page_hash" => members[0].1 = b"same name but wrong bytes",
            "traversal" => members.push(("../../outside", b"evil", b'0')),
            "symlink" => members[0].2 = b'2',
            "sparse" => members[0].2 = b'S',
            "duplicate" => members.push(members[0]),
            "missing" => {
                members.pop();
            }
            "undeclared" => members.push(("unknown", b"data", b'0')),
            _ => unreachable!(),
        }
        if damage != "hash" {
            shard(&segment, &members);
            let bytes = fs::read(&segment).unwrap();
            m["segments"][0]["bytes"] = json!(bytes.len());
            m["segments"][0]["sha256"] = json!(sha256(bytes));
            save(&path, &m);
        }
        assert!(
            source_pack::restore_selected(&f.0, &path, "pages/selected.pdf", &sha256(&selected))
                .is_err(),
            "{damage}"
        );
        assert!(!f.0.join("var/cache/knowledge/source-pages").exists());
        assert!(!f.0.join("outside").exists());
    }
}
#[test]
#[cfg(unix)]
fn source_symlinks_and_noncanonical_paths_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new("paths");
    let (path, mut m, selected) = pack(&f);
    for relative in ["../escape", "/absolute", "x\\y", "x//y", ".", "", "x/./y"] {
        m["segments"][0]["files"][0]["path"] = json!(relative);
        save(&path, &m);
        assert!(source_pack::load_manifest(&f.0, &path).is_err());
    }
    let (path, _, _) = pack(&f);
    let cache = f.0.join("var/cache/knowledge");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let elsewhere = f.0.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    symlink(&elsewhere, &cache).unwrap();
    assert!(
        source_pack::restore_selected(&f.0, &path, "pages/selected.pdf", &sha256(selected))
            .is_err()
    );
    assert_eq!(fs::read_dir(elsewhere).unwrap().count(), 0);
}

#[test]
#[cfg(unix)]
fn original_mutation_encryption_and_fifo_never_publish_registration() {
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    let f = Fixture::new("freshness");
    let (path, _, pdf) = manifest(&f);
    let mut opts = options();
    opts.tools.pdftotext = script(
        &f,
        "mutate",
        "printf 'changed' >> \"$4\"\nprintf 'first\\fsecond\\fthird\\f'",
    );
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("source_changed_during_registration")
    );
    fs::write(&pdf, unicode_pdf()).unwrap();
    opts = options();
    opts.tools.pdfinfo = script(&f, "encrypted", "printf 'Pages: 3\\nEncrypted: yes\\n'");
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("encryption_invalid")
    );
    fs::remove_file(&pdf).unwrap();
    let name = std::ffi::CString::new(pdf.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let start = std::time::Instant::now();
    assert!(registration::register(&f.0, &path, &options()).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    fs::remove_file(&pdf).unwrap();
    symlink(f.0.join("SKILL.md"), &pdf).unwrap();
    assert!(registration::register(&f.0, &path, &options()).is_err());
    assert!(!f.0.join("sources/.indexes/local-pdf").exists());
}

#[test]
fn archive_expansion_and_compressed_trailing_bytes_are_rejected() {
    use std::io::{Read, Write};
    for expansion in [false, true] {
        let f = Fixture::new("compressed-tail");
        let (path, mut m, selected) = pack(&f);
        let segment = path.parent().unwrap().join("segments/a.tar.xz");
        let mut bytes = fs::read(&segment).unwrap();
        if expansion {
            let mut raw = Vec::new();
            xz2::read::XzDecoder::new(bytes.as_slice())
                .read_to_end(&mut raw)
                .unwrap();
            raw.resize(raw.len() + 2 * 1024 * 1024, 0);
            let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
            encoder.write_all(&raw).unwrap();
            bytes = encoder.finish().unwrap();
        } else {
            bytes.extend(b"hidden bytes after XZ footer");
        }
        fs::write(&segment, &bytes).unwrap();
        m["segments"][0]["bytes"] = json!(bytes.len());
        m["segments"][0]["sha256"] = json!(sha256(bytes));
        save(&path, &m);
        assert!(
            source_pack::restore_selected(&f.0, &path, "pages/selected.pdf", &sha256(&selected))
                .is_err()
        );
        assert!(!f.0.join("var/cache/knowledge/source-pages").exists());
    }
}

#[test]
fn relative_source_identity_survives_complete_library_relocation() {
    let old = Fixture::new("producer");
    let new = Fixture::new("receiver");
    let (path, _, _) = manifest(&old);
    registration::register(&old.0, &path, &options()).unwrap();
    let old_provider = Provider::open(&SkillRoot::new(&old.0).unwrap(), "local_pdf", None).unwrap();
    let page = old_provider.page("local_pdf:unicode:page:1").unwrap();
    let evidence = old_provider.source_result(&page, "工程", &tools()).unwrap();
    assert_eq!(evidence["source_pdf"], "sources/books/中文-original.pdf");
    assert_eq!(evidence["original_verified"], true);
    drop(old_provider);
    fs::remove_dir_all(new.0.join("sources")).unwrap();
    fs::rename(old.0.join("sources"), new.0.join("sources")).unwrap();
    assert!(!old.0.join("sources/books/中文-original.pdf").exists());
    let receiver = Provider::open(&SkillRoot::new(&new.0).unwrap(), "local_pdf", None).unwrap();
    let received = receiver
        .source_result(
            &receiver.page("local_pdf:unicode:page:1").unwrap(),
            "工程",
            &tools(),
        )
        .unwrap();
    for key in [
        "source_pdf",
        "source_sha256",
        "source_locator",
        "text_sha256",
        "index_identity",
    ] {
        assert_eq!(evidence[key], received[key], "{key}");
    }
    assert_eq!(
        citations::verify_locator_with_tools(&new.0, &evidence, &tools()).unwrap(),
        evidence["source_locator"]
    );
}

#[test]
fn misumi_selected_single_page_keeps_book_position_and_missing_full_book_status() {
    let f = Fixture::new("misumi-citation");
    let (path, mut m, _) = pack(&f);
    let selected = unicode_pdf_pages(1);
    let segment = path.parent().unwrap().join("segments/a.tar.xz");
    shard(
        &segment,
        &[
            ("pages/selected.pdf", &selected, b'0'),
            ("pages/other.pdf", b"other exact bytes", b'0'),
        ],
    );
    let bytes = fs::read(&segment).unwrap();
    m["segments"][0]["bytes"] = json!(bytes.len());
    m["segments"][0]["sha256"] = json!(sha256(bytes));
    m["segments"][0]["files"][0]["bytes"] = json!(selected.len());
    m["segments"][0]["files"][0]["sha256"] = json!(sha256(&selected));
    m["assembled_pdfs"] = json!([{"catalog":"fixture","path":"output/book.pdf","sha256":"0".repeat(64),"bytes":42,"download_url":"https://example.invalid/book.pdf"}]);
    save(&path, &m);
    let row = json!({"id":"fixture:42","catalog":"fixture","catalog_title":"Fixture","source_kind":"misumi","source_pack_manifest":path,"source_relative_path":"pages/selected.pdf","source_sha256":sha256(&selected),"source_bytes":selected.len(),"pdf_page":42,"page_id":42,"printed_page":"40","catalog_registration":{"pdf_relative_path":"output/book.pdf","pdf_sha256":"0".repeat(64)},"source_locator":{"physical_page":42,"status":"index_position"},"body":"工程","text_sha256":sha256("工程"),"titles_json":"[]","series_json":"[]","native_text_sparse":true});
    let evidence = citations::resolved_result(&f.0, &row, "工程", &tools()).unwrap();
    assert_eq!(evidence["pdf_page"], 42);
    assert_eq!(evidence["source_file_page"], 1);
    assert!(
        evidence["source_pdf"]
            .as_str()
            .unwrap()
            .starts_with("var/cache/knowledge/")
    );
    assert_eq!(evidence["assembled_pdf_status"], "not_downloaded");
    assert!(evidence["assembled_pdf_uri"].is_null());
    assert_eq!(evidence["catalog_source_locator"]["physical_page"], 42);
    assert_eq!(
        evidence["source_pack_manifest"],
        "sources/.indexes/misumi/source-pack.json"
    );
    assert_eq!(
        evidence["source_pack_manifest_sha256"],
        sha256(fs::read(&path).unwrap())
    );
    let view = citations::view(&f.0, &evidence, 72, &tools()).unwrap();
    assert_eq!(view["identity"]["source_file_page"], 1);
    assert_eq!(view["catalog_pdf_page"], 42);
    assert_eq!(view["height"], 800);
}

#[test]
#[ignore = "retains one tiny actual Poppler fixture for root CLI/receiver handoff"]
fn retain_actual_native_source_fixture() {
    let f = Fixture::new("retained-native");
    let (path, _, pdf) = manifest(&f);
    let registered = registration::register(&f.0, &path, &options()).unwrap();
    let root = SkillRoot::new(&f.0).unwrap();
    let provider = Provider::open(&root, "local_pdf", None).unwrap();
    let mut observations = Vec::new();
    for n in 1..=3 {
        let page = provider
            .page(&format!("local_pdf:unicode:page:{n}"))
            .unwrap();
        let evidence = provider.source_result(&page, "工程", &tools()).unwrap();
        let view = citations::view(&f.0, &evidence, 72, &tools()).unwrap();
        observations.push(json!({"page":page,"evidence":evidence,"view":view}));
    }
    let report = json!({"schema":"ontology-engineering.phase5-native-source-fixture/v1","status":"passed","scope":"synthetic 3-page ToUnicode/blank/bitmap PDF; actual Poppler registration+whole-page rendering; no engineering claim/no external API","root":f.0,"manifest":path,"original_pdf":pdf,"original_pdf_sha256":sha256(fs::read(&pdf).unwrap()),"registered":registered,"observations":observations,"network_requests":0});
    drop(provider);
    oe_core::write_new(&f.0.join("ACTUAL-NATIVE-ACCEPTANCE.json"), &report).unwrap();
    let handoff = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join(format!(
            "var/maintenance/rust-rewrite-20260927/phase5-native-source-fixture-{}.json",
            &sha256(fs::read(f.0.join("ACTUAL-NATIVE-ACCEPTANCE.json")).unwrap())[..12]
        ));
    oe_core::write_new(&handoff,&json!({"fixture_root":f.0,"acceptance":f.0.join("ACTUAL-NATIVE-ACCEPTANCE.json"),"acceptance_sha256":sha256(fs::read(f.0.join("ACTUAL-NATIVE-ACCEPTANCE.json")).unwrap())})).unwrap();
    println!("retained native source fixture: {}", f.0.display());
    std::mem::forget(f);
}

#[test]
fn replacement_rejects_self_consistent_hashes_of_non_index_bytes() {
    let f = Fixture::new("fake-old-index");
    let (path, _, _) = manifest(&f);
    let target = f.0.join("sources/.indexes/local-pdf");
    fs::create_dir_all(&target).unwrap();
    let bytes = b"not an SQLite database even with a matching recorded hash";
    fs::write(target.join("catalog.sqlite3"), bytes).unwrap();
    let record = json!({"schema":"ontology-engineering.local-pdf-registration/v1","sources":[],"index_sha256":sha256(bytes)});
    save(&target.join("registration.json"), &record);
    let mut opts = options();
    opts.replace = true;
    let error = registration::register(&f.0, &path, &opts).unwrap_err();
    assert!(format!("{error:#}").contains("index_"));
    assert_eq!(fs::read(target.join("catalog.sqlite3")).unwrap(), bytes);
}

fn index_pair(f: &Fixture) -> (Vec<u8>, Vec<u8>) {
    let dir = f.0.join("sources/.indexes/local-pdf");
    (
        fs::read(dir.join("catalog.sqlite3")).unwrap(),
        fs::read(dir.join("registration.json")).unwrap(),
    )
}
fn second_source(f: &Fixture, first: &Value) -> Value {
    let path = f.0.join("sources/books/second.pdf");
    let raw = unicode_pdf_pages(1);
    fs::write(&path, &raw).unwrap();
    let mut source = first.clone();
    source["id"] = json!("second");
    source["path"] = json!("second.pdf");
    source["sha256"] = json!(sha256(raw));
    source["chapters"][0]["end_page"] = json!(1);
    source
}
fn absent_tools(f: &Fixture) -> PopplerTools {
    PopplerTools {
        pdfinfo: f.0.join("absent-pdfinfo"),
        pdftotext: f.0.join("absent-pdftotext"),
        pdftoppm: f.0.join("absent-pdftoppm"),
    }
}
#[cfg(unix)]
fn counted_tools(f: &Fixture) -> (PopplerTools, PathBuf) {
    fn quote(path: &Path) -> String {
        format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
    }
    let log = f.0.join("tool-calls.log");
    fs::write(&log, "").unwrap();
    let pdfinfo = script(
        f,
        "counted-info",
        &format!(
            "printf '%s\\n' \"$*\" >> {}\nexec /opt/homebrew/bin/pdfinfo \"$@\"",
            quote(&log)
        ),
    );
    let pdftotext = script(
        f,
        "counted-text",
        &format!(
            "printf '%s\\n' \"$*\" >> {}\nexec /opt/homebrew/bin/pdftotext \"$@\"",
            quote(&log)
        ),
    );
    (
        PopplerTools {
            pdfinfo,
            pdftotext,
            pdftoppm: tools().pdftoppm,
        },
        log,
    )
}

#[test]
#[cfg(unix)]
fn merge_adds_and_updates_one_source_reusing_verified_pages_and_tool_observations() {
    let f = Fixture::new("incremental-reuse");
    let (path, mut m, _) = manifest(&f);
    let (tools, log) = counted_tools(&f);
    let mut opts = RegistrationOptions { tools, ..options() };
    let original = registration::register(&f.0, &path, &opts).unwrap();
    let original_pair = index_pair(&f);
    let second = second_source(&f, &m["sources"][0]);
    m["sources"] = json!([second]);
    save(&path, &m);
    opts.merge = true;
    fs::write(&log, "").unwrap();
    let merged = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(merged["update"]["added"], json!(["second"]));
    assert_eq!(merged["update"]["retained"], json!(["unicode"]));
    assert_eq!(merged["update"]["reused_source_ids"], json!(["unicode"]));
    assert_eq!(merged["update"]["extracted_source_ids"], json!(["second"]));
    assert_eq!(merged["index"]["counts"]["pages"], 4);
    assert_eq!(merged["index"]["tools"][0], original["index"]["tools"][0]);
    assert_eq!(
        merged["index"]["reused_sources"][0]["previous_index"]["sha256"],
        sha256(&original_pair.0)
    );
    let calls = fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().count(), 2, "{calls}");
    assert!(
        calls
            .lines()
            .all(|l| l.contains("second.pdf") && !l.contains("中文-original.pdf"))
    );
    let root = SkillRoot::new(&f.0).unwrap();
    let provider = Provider::open(&root, "local_pdf", None).unwrap();
    assert!(
        provider.page("local_pdf:unicode:page:1").unwrap()["body"]
            .as_str()
            .unwrap()
            .contains("工程")
    );
    assert!(
        provider.page("local_pdf:second:page:1").unwrap()["body"]
            .as_str()
            .unwrap()
            .contains("工程")
    );
    drop(provider);
    m["sources"][0]["edition"] = json!("revision-2");
    m["sources"][0]["chapters"][0]["title"] = json!("Changed navigation");
    save(&path, &m);
    fs::write(&log, "").unwrap();
    let updated = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(updated["update"]["updated"], json!(["second"]));
    assert_eq!(updated["update"]["retained"], json!(["unicode"]));
    assert_eq!(updated["index"]["tools"][0], original["index"]["tools"][0]);
    assert_eq!(updated["index"]["counts"]["pages"], 4);
    assert_eq!(updated["update"]["extracted_source_ids"], json!([]));
    assert_eq!(
        updated["update"]["reused_source_ids"],
        json!(["second", "unicode"])
    );
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 0);
    // FTS rowids are reconstructed alongside the new page rows, not copied.
    let db =
        rusqlite::Connection::open(f.0.join("sources/.indexes/local-pdf/catalog.sqlite3")).unwrap();
    let matched:String = db.query_row("SELECT p.id FROM search JOIN pages p ON p.rowid=search.rowid WHERE search MATCH 'changed'",[],|r|r.get(0)).unwrap();
    assert_eq!(matched, "local_pdf:second:page:1");
}

#[test]
fn merge_noop_preserves_pair_without_poppler_and_retains_unspecified_source() {
    let f = Fixture::new("incremental-noop");
    let (path, mut m, _) = manifest(&f);
    let second = second_source(&f, &m["sources"][0]);
    m["sources"].as_array_mut().unwrap().push(second);
    save(&path, &m);
    registration::register(&f.0, &path, &options()).unwrap();
    let old = index_pair(&f);
    m["sources"].as_array_mut().unwrap().pop();
    save(&path, &m);
    let opts = RegistrationOptions {
        merge: true,
        tools: absent_tools(&f),
        ..options()
    };
    let result = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(result["status"], "unchanged");
    assert_eq!(result["published"], false);
    assert_eq!(result["update"]["unchanged"], json!(["unicode"]));
    assert_eq!(result["update"]["retained"], json!(["second"]));
    assert_eq!(result["final_counts"]["pages"], 4);
    assert_eq!(index_pair(&f), old);
}

#[test]
fn registration_preview_hashes_inputs_without_poppler_or_publication_and_lists_removals() {
    let f = Fixture::new("incremental-preview");
    let (path, mut m, _) = manifest(&f);
    let mut opts = RegistrationOptions {
        dry_run: true,
        tools: absent_tools(&f),
        ..options()
    };
    let preview = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["update"]["added"], json!(["unicode"]));
    assert!(preview["final_counts"]["pages"].is_null());
    assert!(!f.0.join("sources/.indexes/local-pdf").exists());
    registration::register(&f.0, &path, &options()).unwrap();
    let old = index_pair(&f);
    let second = second_source(&f, &m["sources"][0]);
    m["sources"] = json!([second]);
    save(&path, &m);
    opts.merge = true;
    let preview = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(preview["update"]["retained"], json!(["unicode"]));
    assert_eq!(preview["update"]["reused_source_ids"], json!([]));
    assert_eq!(preview["reuse_candidate_ids"], json!(["unicode"]));
    assert_eq!(preview["update"]["final_source_count"], 2);
    assert_eq!(index_pair(&f), old);
    opts.merge = false;
    opts.replace = true;
    let preview = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(preview["update"]["removed"], json!(["unicode"]));
    assert_eq!(preview["update"]["retained"], json!([]));
    assert_eq!(preview["update"]["final_source_count"], 1);
    assert_eq!(index_pair(&f), old);
    m["sources"][0]["sha256"] = json!("0".repeat(64));
    save(&path, &m);
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("source_hash_mismatch")
    );
    assert_eq!(index_pair(&f), old);
}

#[test]
fn merge_rejects_retained_original_and_bound_index_drift_preserving_existing_pair() {
    let f = Fixture::new("incremental-drift");
    let (path, mut m, pdf) = manifest(&f);
    registration::register(&f.0, &path, &options()).unwrap();
    let old = index_pair(&f);
    let second = second_source(&f, &m["sources"][0]);
    m["sources"] = json!([second]);
    save(&path, &m);
    let opts = RegistrationOptions {
        merge: true,
        ..options()
    };
    fs::write(&pdf, b"changed old original").unwrap();
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("source_hash_mismatch")
    );
    assert_eq!(index_pair(&f), old);
    fs::write(&pdf, unicode_pdf()).unwrap();
    let dir = f.0.join("sources/.indexes/local-pdf");
    let mut changed = old.0.clone();
    changed.push(0);
    fs::write(dir.join("catalog.sqlite3"), &changed).unwrap();
    let drifted = index_pair(&f);
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("existing_index_identity_mismatch")
    );
    assert_eq!(index_pair(&f), drifted);
}

#[test]
fn merge_budgets_apply_to_complete_retained_plus_incoming_catalog() {
    let f = Fixture::new("incremental-budget");
    let (path, mut m, _) = manifest(&f);
    registration::register(&f.0, &path, &options()).unwrap();
    let old = index_pair(&f);
    let base = m["sources"][0].clone();
    m["sources"] = json!(
        (0..256)
            .map(|n| {
                let mut s = base.clone();
                s["id"] = json!(format!("new-{n}"));
                s
            })
            .collect::<Vec<_>>()
    );
    save(&path, &m);
    let opts = RegistrationOptions {
        merge: true,
        dry_run: true,
        tools: absent_tools(&f),
        ..options()
    };
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("source_count_invalid")
    );
    assert_eq!(index_pair(&f), old);
    let mut added = base.clone();
    added["id"] = json!("added");
    let chapter = base["chapters"][0].clone();
    added["chapters"] = json!(
        (0..4096)
            .map(|n| {
                let mut c = chapter.clone();
                c["id"] = json!(format!("chapter-{n}"));
                c
            })
            .collect::<Vec<_>>()
    );
    m["sources"] = json!([added]);
    save(&path, &m);
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("total_chapter_limit")
    );
    assert_eq!(index_pair(&f), old);
}

#[test]
fn merge_failure_and_conflicting_modes_leave_original_registration_intact() {
    let f = Fixture::new("incremental-failure");
    let (path, mut m, _) = manifest(&f);
    registration::register(&f.0, &path, &options()).unwrap();
    let old = index_pair(&f);
    m["sources"][0]["edition"] = json!("updated");
    // A changed original path still requires actual extraction, even when the
    // bytes match. A metadata-only update no longer requires Poppler.
    fs::copy(
        f.0.join("sources/books/中文-original.pdf"),
        f.0.join("sources/books/relocated.pdf"),
    )
    .unwrap();
    m["sources"][0]["path"] = json!("relocated.pdf");
    save(&path, &m);
    let mut opts = RegistrationOptions {
        merge: true,
        tools: absent_tools(&f),
        ..options()
    };
    assert!(registration::register(&f.0, &path, &opts).is_err());
    assert_eq!(index_pair(&f), old);
    opts.replace = true;
    assert!(
        registration::register(&f.0, &path, &opts)
            .unwrap_err()
            .to_string()
            .contains("replace_merge_conflict")
    );
    assert_eq!(index_pair(&f), old);
}

fn native_page_rows(f: &Fixture) -> Vec<Value> {
    let db =
        rusqlite::Connection::open(f.0.join("sources/.indexes/local-pdf/catalog.sqlite3")).unwrap();
    let mut statement = db.prepare("SELECT id,source_sha256,source_bytes,body,text_sha256,raw_text_sha256,raw_text_chars,text_chars,native_text_sparse,source_json FROM pages ORDER BY id").unwrap();
    statement
        .query_map([], |row| {
            Ok(json!(
                (0..10)
                    .map(|i| match row.get_ref(i).unwrap() {
                        rusqlite::types::ValueRef::Text(bytes) =>
                            json!(std::str::from_utf8(bytes).unwrap()),
                        rusqlite::types::ValueRef::Integer(n) => json!(n),
                        _ => panic!("unexpected native row value"),
                    })
                    .collect::<Vec<_>>()
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn metadata_navigation_merge_reuses_native_text_without_tools_and_rebuilds_search() {
    let f = Fixture::new("metadata-navigation-reuse");
    let (path, mut manifest, original_path) = manifest(&f);
    manifest["sources"][0]["chapters"][0]["aliases"] = json!(["obsoletealiasword"]);
    let second = second_source(&f, &manifest["sources"][0]);
    manifest["sources"].as_array_mut().unwrap().push(second);
    save(&path, &manifest);
    let initial = registration::register(&f.0, &path, &options()).unwrap();
    let before_pair = index_pair(&f);
    let before_native = native_page_rows(&f);
    let before_original = fs::read(&original_path).unwrap();
    let old_registration: Value = serde_json::from_slice(&before_pair.1).unwrap();
    manifest["sources"].as_array_mut().unwrap().pop();
    manifest["sources"][0]["title"] = json!("Corrected title");
    manifest["sources"][0]["edition"] = json!("Corrected bibliographic edition");
    manifest["sources"][0]["chapters"] = json!([
        {"id":"first","title":"Corrected first chapter","start_page":1,"end_page":1,"aliases":["updatedaliasword"]},
        {"id":"remaining","title":"Blank and image pages","start_page":2,"end_page":3,"aliases":[]}
    ]);
    save(&path, &manifest);
    let mut opts = RegistrationOptions {
        merge: true,
        dry_run: true,
        tools: absent_tools(&f),
        ..options()
    };
    let preview = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["update"]["updated"], json!(["unicode"]));
    assert_eq!(preview["update"]["retained"], json!(["second"]));
    assert_eq!(preview["update"]["extraction_required_ids"], json!([]));
    assert_eq!(
        preview["final_counts"],
        json!({"catalogs":2,"chapters":3,"pages":4})
    );
    assert_eq!(index_pair(&f), before_pair);
    opts.dry_run = false;
    let updated = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(updated["status"], "registered");
    assert_eq!(updated["published"], true);
    assert_eq!(updated["update"]["updated"], json!(["unicode"]));
    assert_eq!(updated["update"]["retained"], json!(["second"]));
    assert_eq!(updated["update"]["extracted_source_ids"], json!([]));
    assert_eq!(
        updated["update"]["reused_source_ids"],
        json!(["second", "unicode"])
    );
    assert_eq!(updated["index"]["counts"]["chapters"], 3);
    assert_eq!(updated["index"]["counts"]["pages"], 4);
    assert_eq!(updated["index"]["tools"], initial["index"]["tools"]);
    assert_eq!(native_page_rows(&f), before_native);
    assert_eq!(fs::read(&original_path).unwrap(), before_original);
    let after_pair = index_pair(&f);
    assert_ne!(
        after_pair, before_pair,
        "navigation update must publish a new identity"
    );
    let registered: Value = serde_json::from_slice(&after_pair.1).unwrap();
    assert_eq!(registered["sources"][1], old_registration["sources"][1]);
    assert_eq!(
        registered["sources"][0]["chapters"],
        manifest["sources"][0]["chapters"]
    );
    let db =
        rusqlite::Connection::open(f.0.join("sources/.indexes/local-pdf/catalog.sqlite3")).unwrap();
    let matched: String = db.query_row("SELECT p.id FROM search JOIN pages p ON p.rowid=search.rowid WHERE search MATCH 'updatedaliasword'", [], |r|r.get(0)).unwrap();
    assert_eq!(matched, "local_pdf:unicode:page:1");
    let obsolete: i64 = db.query_row("SELECT count(*) FROM search JOIN pages p ON p.rowid=search.rowid WHERE search MATCH 'obsoletealiasword' AND p.catalog='local_pdf:unicode'", [], |r|r.get(0)).unwrap();
    assert_eq!(obsolete, 0);
    drop(db);
    let no_op = registration::register(&f.0, &path, &opts).unwrap();
    assert_eq!(no_op["status"], "unchanged");
    assert_eq!(index_pair(&f), after_pair);
}

#[test]
fn metadata_navigation_merge_rejects_out_of_document_chapters_in_preview_and_apply() {
    let f = Fixture::new("metadata-navigation-range");
    let (path, mut manifest, _) = manifest(&f);
    registration::register(&f.0, &path, &options()).unwrap();
    let before = index_pair(&f);
    manifest["sources"][0]["chapters"][0]["end_page"] = json!(4);
    save(&path, &manifest);
    for dry_run in [true, false] {
        let opts = RegistrationOptions {
            merge: true,
            dry_run,
            tools: absent_tools(&f),
            ..options()
        };
        let error = registration::register(&f.0, &path, &opts).unwrap_err();
        assert!(
            error.to_string().contains("chapter_outside_document"),
            "{error:#}"
        );
        assert_eq!(index_pair(&f), before);
    }
}

#[test]
fn metadata_navigation_merge_rejects_rebound_rows_that_disagree_with_old_declaration() {
    for fault in ["chapter", "page"] {
        let f = Fixture::new(&format!("metadata-rebound-{fault}"));
        let (path, mut manifest, _) = manifest(&f);
        registration::register(&f.0, &path, &options()).unwrap();
        manifest["sources"][0]["chapters"][0]["title"] = json!("Corrected navigation");
        save(&path, &manifest);
        let directory = f.0.join("sources/.indexes/local-pdf");
        let db = rusqlite::Connection::open(directory.join("catalog.sqlite3")).unwrap();
        // Make a row agree with the desired NEW declaration, but not the bound
        // OLD declaration. Rebinding the whole file hash must not repair this.
        if fault == "chapter" {
            db.execute(
                "UPDATE chapters SET title=?,source_json=?",
                rusqlite::params![
                    "Corrected navigation",
                    serde_json::to_string(&manifest["sources"][0]["chapters"][0]).unwrap()
                ],
            )
            .unwrap();
        } else {
            db.execute(
                "UPDATE pages SET titles_json=?",
                [serde_json::to_string(&json!([{"text":"Corrected navigation","origin":"declared_chapter_navigation"}])).unwrap()],
            ).unwrap();
        }
        drop(db);
        let mut registration: Value =
            serde_json::from_slice(&fs::read(directory.join("registration.json")).unwrap())
                .unwrap();
        registration["index_sha256"] =
            json!(sha256(fs::read(directory.join("catalog.sqlite3")).unwrap()));
        save(&directory.join("registration.json"), &registration);
        let rebound = index_pair(&f);
        for dry_run in [true, false] {
            let opts = RegistrationOptions {
                merge: true,
                dry_run,
                tools: absent_tools(&f),
                ..options()
            };
            let error = registration::register(&f.0, &path, &opts).unwrap_err();
            let expected = if fault == "chapter" {
                "local_pdf_reuse_chapter_mismatch"
            } else {
                "local_pdf_reuse_page_binding_mismatch"
            };
            assert!(error.to_string().contains(expected), "{error:#}");
            assert_eq!(index_pair(&f), rebound);
        }
    }
}

#[test]
#[cfg(unix)]
fn changed_original_identity_and_replace_still_require_native_extraction() {
    for kind in ["path", "sha256", "replace"] {
        let f = Fixture::new(&format!("metadata-extraction-{kind}"));
        let (path, mut manifest, original) = manifest(&f);
        registration::register(&f.0, &path, &options()).unwrap();
        let before = index_pair(&f);
        match kind {
            "path" => {
                fs::copy(&original, f.0.join("sources/books/relocated.pdf")).unwrap();
                manifest["sources"][0]["path"] = json!("relocated.pdf");
            }
            "sha256" => {
                let mut bytes = fs::read(&original).unwrap();
                assert!(bytes.starts_with(b"%PDF-1.4"));
                bytes[7] = b'5';
                fs::write(&original, &bytes).unwrap();
                manifest["sources"][0]["sha256"] = json!(sha256(bytes));
            }
            _ => {}
        }
        save(&path, &manifest);
        let mut opts = RegistrationOptions {
            merge: kind != "replace",
            replace: kind == "replace",
            dry_run: true,
            tools: absent_tools(&f),
            ..options()
        };
        let preview = registration::register(&f.0, &path, &opts).unwrap();
        assert_eq!(
            preview["update"]["extraction_required_ids"],
            json!(["unicode"])
        );
        assert!(preview["final_counts"]["pages"].is_null());
        opts.dry_run = false;
        assert!(registration::register(&f.0, &path, &opts).is_err());
        assert_eq!(index_pair(&f), before);
        let (counted, log) = counted_tools(&f);
        opts.tools = counted;
        let result = registration::register(&f.0, &path, &opts).unwrap();
        assert_eq!(result["update"]["extracted_source_ids"], json!(["unicode"]));
        assert_eq!(result["update"]["reused_source_ids"], json!([]));
        assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 2);
    }
}
