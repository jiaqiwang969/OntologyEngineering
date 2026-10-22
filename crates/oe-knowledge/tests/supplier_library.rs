//! Synthetic native source libraries only; no existing supplier data is touched.
use oe_core::{SkillRoot, json::canonical_digest, sha256};
use oe_knowledge::{sources::Provider, supplier_library, text};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const CODES: [&str; 2] = ["fabiaozhunpin202210", "fajingjixing202306"];
struct Fixture {
    outer: PathBuf,
    root: SkillRoot,
    archive: PathBuf,
    index: PathBuf,
}
fn save(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn identity(path: &Path, name: &str) -> Value {
    let raw = fs::read(path).unwrap();
    json!({"path":name,"sha256":sha256(&raw),"bytes":raw.len()})
}
fn pdf() -> Vec<u8> {
    let text = b"BT /F1 12 Tf 20 80 Td (Shaft conditions before engineering use.) Tj ET";
    let objs=[b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_vec(),b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),[format!("<< /Length {} >>\nstream\n",text.len()).as_bytes(),text,b"\nendstream"].concat()];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).bytes());
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .bytes(),
    );
    out
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let outer = producer.join("var/maintenance/rust-tests").join(format!(
            "supplier-library-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(outer.join("skill/sources/archive")).unwrap();
        fs::write(outer.join("skill/SKILL.md"), "synthetic fixture").unwrap();
        let root = SkillRoot::new(outer.join("skill")).unwrap();
        let archive = root.path().join("sources/archive");
        let index = root.path().join("sources/catalog.sqlite3");
        let f = Self {
            outer,
            root,
            archive,
            index,
        };
        f.create();
        f
    }
    fn target(&self, name: &str) -> PathBuf {
        self.root.path().join("sources/.indexes").join(name)
    }
    fn create(&self) {
        let db = Connection::open(&self.index).unwrap();
        db.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
          CREATE TABLE catalogs(code TEXT PRIMARY KEY,label TEXT,title TEXT,page_count INTEGER,pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT);
          CREATE TABLE chapters(id TEXT PRIMARY KEY,catalog TEXT,title TEXT,start_page INTEGER,end_page INTEGER,source_json TEXT);
          CREATE TABLE pages(id TEXT PRIMARY KEY,catalog TEXT,page_id INTEGER,pdf_page INTEGER,printed_page TEXT,chapter_id TEXT,source_relative_path TEXT,source_url TEXT,source_sha256 TEXT,source_bytes INTEGER,body TEXT,text_sha256 TEXT,raw_text_sha256 TEXT,raw_text_chars INTEGER,text_chars INTEGER,native_text_sparse INTEGER,titles_json TEXT,series_json TEXT,series_count INTEGER,source_json TEXT);
          CREATE TABLE hotspots(page_id TEXT,ordinal INTEGER,kind TEXT,keyword TEXT,keyword_normalized TEXT,target TEXT,source_json TEXT,PRIMARY KEY(page_id,ordinal));
          CREATE INDEX hotspot_model ON hotspots(keyword_normalized,kind);
          CREATE INDEX page_chapter ON pages(chapter_id,pdf_page);
          CREATE INDEX page_position ON pages(catalog,pdf_page);
          CREATE VIRTUAL TABLE search USING fts5(models,titles,body,chapter);").unwrap();
        let mut source_identity = Vec::new();
        for (i, code) in CODES.iter().enumerate() {
            let page_path = format!("tmp/pdfs/{code}/pages/0001.pdf");
            let full_path = format!("output/pdf/{code}.pdf");
            let raw = pdf();
            for path in [&page_path, &full_path] {
                let p = self.archive.join(path);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, &raw).unwrap();
            }
            let info = json!({"id":i+1,"pageNo":1,"pageNoReal":1,"pdfUrl":"0001.pdf","hotArea":[]});
            let chapter = json!({"id":1,"pageId":i+1,"title":"Shaft conditions"});
            let metadata = format!("tmp/pdfs/{code}/metadata.json");
            let manifest = format!("下载记录/{code}-manifest.json");
            let assembly = format!("下载记录/{code}-assembly.json");
            let qc = format!("output/qc/{code}.json");
            save(
                &self.archive.join(&metadata),
                &json!({"ebook":{"catalogCode":code,"catalogTitle":"Synthetic source","pages":{"info":[info]},"class":[chapter]}}),
            );
            save(
                &self.archive.join(&manifest),
                &json!([{"page_number":1,"page_id":i+1,"filename":"0001.pdf","printed_page":1,"bytes":raw.len(),"sha256":sha256(&raw),"url":"https://example.invalid/fixture.pdf"}]),
            );
            save(
                &self.archive.join(&assembly),
                &json!({"book":code,"pages":1,"file":full_path,"sha256":sha256(&raw),"qc_report":qc}),
            );
            save(
                &self.archive.join(&qc),
                &json!({"historical_observation":true,"passed":false}),
            );
            let meta_sha = sha256(fs::read(self.archive.join(metadata)).unwrap());
            let manifest_sha = sha256(fs::read(self.archive.join(manifest)).unwrap());
            source_identity.push(json!([code, meta_sha, manifest_sha, sha256(&raw)]));
            db.execute(
                "INSERT INTO catalogs VALUES (?,?,?,?,?,?,?,?)",
                params![
                    code,
                    "Synthetic source",
                    "Synthetic source",
                    1,
                    full_path,
                    sha256(&raw),
                    meta_sha,
                    manifest_sha
                ],
            )
            .unwrap();
            let cid = format!("{code}:chapter:1");
            let pid = format!("{code}:page:{}", i + 1);
            db.execute(
                "INSERT INTO chapters VALUES (?,?,?,?,?,?)",
                params![cid, code, "Shaft conditions", 1, 1, chapter.to_string()],
            )
            .unwrap();
            let body = "Shaft conditions before engineering use.";
            db.execute(
                "INSERT INTO pages VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                params![
                    pid,
                    code,
                    (i + 1) as i64,
                    1,
                    "1",
                    cid,
                    page_path,
                    "https://example.invalid/fixture.pdf",
                    sha256(&raw),
                    raw.len() as i64,
                    body,
                    sha256(body),
                    sha256(body),
                    body.len() as i64,
                    body.len() as i64,
                    1,
                    "[]",
                    "[]",
                    0,
                    info.to_string()
                ],
            )
            .unwrap();
            db.execute(
                "INSERT INTO search(rowid,models,titles,body,chapter) VALUES (?,?,?,?,?)",
                params![
                    i + 1,
                    "",
                    "",
                    text::tokens(body).join(" "),
                    text::tokens("Shaft conditions").join(" ")
                ],
            )
            .unwrap();
        }
        let mut meta = json!({"schema":"misumi.catalog-index/v1","version":"fixture","built_at":"2026-09-28T00:00:00Z","source_identity":source_identity,"counts":{"pages":2,"catalogs":2,"chapters":2},"extraction":"synthetic index for closure tests","text_origin":"fixture_text_not_extracted"});
        meta["fingerprint"] = json!(canonical_digest(&meta).unwrap());
        for (k, v) in meta.as_object().unwrap() {
            db.execute("INSERT INTO meta VALUES (?,?)", params![k, v.to_string()])
                .unwrap();
        }
    }
    fn source_catalogs(&self) -> Value {
        let db = Connection::open(&self.index).unwrap();
        let get = |key: &str| -> Value {
            let text: String = db
                .query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get(0))
                .unwrap();
            serde_json::from_str(&text).unwrap()
        };
        json!({"source_identity":get("source_identity"),"counts":get("counts"),"index_fingerprint":get("fingerprint")})
    }
    fn pack(&self, full: bool) -> (PathBuf, Value) {
        let downloads = self.root.path().join("sources/downloads");
        fs::create_dir(&downloads).unwrap();
        let mut metadata = vec![(
            "catalog.sqlite3".to_string(),
            fs::read(&self.index).unwrap(),
            false,
        )];
        let mut pages = Vec::new();
        let mut assemblies = Vec::new();
        let mut books = Vec::new();
        for code in CODES {
            for p in [
                format!("tmp/pdfs/{code}/metadata.json"),
                format!("下载记录/{code}-manifest.json"),
                format!("下载记录/{code}-assembly.json"),
                format!("output/qc/{code}.json"),
            ] {
                metadata.push((
                    format!("archive/{p}"),
                    fs::read(self.archive.join(p)).unwrap(),
                    false,
                ));
            }
            let page = format!("tmp/pdfs/{code}/pages/0001.pdf");
            pages.push((
                page.clone(),
                fs::read(self.archive.join(page)).unwrap(),
                false,
            ));
            let path = format!("output/pdf/{code}.pdf");
            let name = format!("full/{code}.pdf");
            let raw = fs::read(self.archive.join(&path)).unwrap();
            assemblies.push(json!({"catalog":code,"path":path,"sha256":sha256(&raw),"bytes":raw.len(),"download_path":name,"download_url":null}));
            books.push((name, raw, false));
        }
        let meta = archive(&downloads, "catalog-and-metadata.tar.xz", &metadata);
        let mut segment = archive(&downloads, "segments/pages.tar.xz", &pages);
        segment["id"] = json!("fixture-pages");
        let mut manifest = json!({"schema":"ontology-engineering.misumi-source-pack/v1","index":meta["files"][0],"metadata":meta,"segments":[segment],"assembled_pdfs":assemblies,"source_catalogs":self.source_catalogs()});
        if full {
            manifest["full_pdfs_archive"] = archive(&downloads, "full-pdfs.tar.xz", &books);
        }
        save(&downloads.join("source-pack.json"), &manifest);
        (downloads, manifest)
    }
    fn consumer(&self, target: &Path) {
        let p = Provider::open(&self.root, "misumi", Some(target)).unwrap();
        let row = p.page(&format!("{}:page:1", CODES[0])).unwrap();
        assert_eq!(row["source_sha256"], sha256(pdf()));
        assert_eq!(row["text_origin"], "fixture_text_not_extracted");
        assert_eq!(p.chapters().unwrap().len(), 2);
        assert!(!p.retrieve("shaft", 2, &[]).unwrap().is_empty());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.outer).unwrap();
    }
}

fn archive(root: &Path, name: &str, members: &[(String, Vec<u8>, bool)]) -> Value {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let encoder = xz2::write::XzEncoder::new(fs::File::create(&path).unwrap(), 1);
    let mut tar = tar::Builder::new(encoder);
    for (name, raw, link) in members {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o600);
        header.set_size(if *link { 0 } else { raw.len() as u64 });
        if *link {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("outside.pdf").unwrap();
        } else {
            header.set_entry_type(tar::EntryType::Regular);
        }
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();
        tar.append(&header, if *link { &[][..] } else { raw.as_slice() })
            .unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap();
    let mut result = identity(&path, name);
    result["files"] = json!(
        members
            .iter()
            .map(|(path, raw, _)| json!({"path":path,"sha256":sha256(raw),"bytes":raw.len()}))
            .collect::<Vec<_>>()
    );
    result
}
fn members(path: &Path) -> Vec<(String, Vec<u8>, bool)> {
    use std::io::Read;
    let mut tar = tar::Archive::new(xz2::read::XzDecoder::new(fs::File::open(path).unwrap()));
    tar.entries()
        .unwrap()
        .map(|e| {
            let mut e = e.unwrap();
            let name = e.path().unwrap().to_str().unwrap().to_owned();
            let mut bytes = Vec::new();
            e.read_to_end(&mut bytes).unwrap();
            (name, bytes, false)
        })
        .collect()
}
fn rewrite_archive(
    downloads: &Path,
    manifest: &mut Value,
    key: &str,
    values: &[(String, Vec<u8>, bool)],
    rebind_members: bool,
) {
    let path = manifest[key]["path"].as_str().unwrap().to_owned();
    let changed = archive(downloads, &path, values);
    manifest[key]["sha256"] = changed["sha256"].clone();
    manifest[key]["bytes"] = changed["bytes"].clone();
    if rebind_members {
        manifest[key]["files"] = changed["files"].clone();
    }
    save(&downloads.join("source-pack.json"), manifest);
}

fn set_source_binding(index: &Path, binding: Value) {
    let db = Connection::open(index).unwrap();
    let mut meta = serde_json::Map::new();
    let mut statement = db.prepare("SELECT key,value FROM meta").unwrap();
    for row in statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
    {
        let (key, value) = row.unwrap();
        if key != "fingerprint" {
            meta.insert(key, serde_json::from_str(&value).unwrap());
        }
    }
    drop(statement);
    meta.insert("source_binding".into(), binding);
    let mut document = Value::Object(meta);
    document["fingerprint"] = json!(canonical_digest(&document).unwrap());
    for key in ["source_binding", "fingerprint"] {
        db.execute(
            "INSERT OR REPLACE INTO meta VALUES (?,?)",
            params![key, document[key].to_string()],
        )
        .unwrap();
    }
}

#[test]
fn native_expanded_build_provenance_survives_explicit_copy_or_registration() {
    let f = Fixture::new();
    set_source_binding(
        &f.index,
        json!({"kind":"expanded_archive","archive_root":"sources/previous-build-location"}),
    );
    for name in ["registered", "imported"] {
        let target = f.target(name);
        if name == "registered" {
            supplier_library::register(&f.root, &f.archive, &f.index, &target).unwrap();
        } else {
            supplier_library::import_data(&f.root, &f.archive, &f.index, &target).unwrap();
        }
        assert_eq!(
            supplier_library::verify(&f.root, &target).unwrap()["status"],
            "verified"
        );
        f.consumer(&target);
    }
}

#[test]
fn native_pack_binding_cannot_be_dropped_or_changed_to_an_expanded_claim() {
    let f = Fixture::new();
    let (downloads, _) = f.pack(false);
    let manifest = downloads.join("source-pack.json");
    let manifest_sha = sha256(fs::read(&manifest).unwrap());
    set_source_binding(
        &f.index,
        json!({"kind":"local_lossless_shards","source_pack_manifest":"sources/downloads/source-pack.json","source_pack_manifest_sha256":manifest_sha}),
    );
    assert!(
        supplier_library::register(
            &f.root,
            &f.archive,
            &f.index,
            &f.target("downgrade-register")
        )
        .is_err()
    );
    assert!(
        supplier_library::import_data(&f.root, &f.archive, &f.index, &f.target("downgrade-import"))
            .is_err()
    );
    let target = f.target("packed-native");
    let path = target.join("source.json");
    let registration = json!({"schema":"ontology-engineering.misumi-source-location/v1","archive_root":"../../downloads/cache/archive","index_path":"../../catalog.sqlite3","index_sha256":sha256(fs::read(&f.index).unwrap()),"source_catalogs":f.source_catalogs(),"verification":"source-bound test registration","source_pack_manifest":"../../downloads/source-pack.json","source_pack_manifest_sha256":manifest_sha});
    save(&path, &registration);
    assert_eq!(
        supplier_library::verify(&f.root, &target).unwrap()["status"],
        "verified"
    );
    f.consumer(&target);
    let mut bad = registration;
    bad.as_object_mut().unwrap().remove("source_pack_manifest");
    bad.as_object_mut()
        .unwrap()
        .remove("source_pack_manifest_sha256");
    bad["archive_root"] = json!("../../archive");
    save(&path, &bad);
    assert!(supplier_library::verify(&f.root, &target).is_err());
    assert!(Provider::open(&f.root, "misumi", Some(&target)).is_err());
}

#[test]
fn corrupt_input_copy_or_optional_whole_books_preserve_no_page_expansion() {
    let f = Fixture::new();
    let source = f
        .archive
        .join(format!("tmp/pdfs/{}/pages/0001.pdf", CODES[0]));
    let mut raw = fs::read(&source).unwrap();
    raw[8] ^= 1;
    fs::write(source, raw).unwrap();
    let target = f.target("bad-import");
    assert!(supplier_library::import_data(&f.root, &f.archive, &f.index, &target).is_err());
    assert!(!target.exists());
    let f = Fixture::new();
    let (downloads, manifest) = f.pack(false);
    for book in manifest["assembled_pdfs"].as_array().unwrap() {
        let target = downloads.join(book["download_path"].as_str().unwrap());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, pdf()).unwrap();
    }
    let target = f.target("optional-books");
    let result = supplier_library::install_pack(&f.root, &downloads, &target).unwrap();
    assert_eq!(result["full_pdfs_installed"], 2);
    assert_eq!(result["pages_expanded"], 0);
    for code in CODES {
        assert_eq!(
            fs::read(target.join(format!("cache/archive/output/pdf/{code}.pdf"))).unwrap(),
            pdf()
        );
    }
    assert!(!target.join("cache/archive/tmp").exists());
}

#[test]
fn old_installed_metadata_layout_supports_native_registration_without_tar_fallback_on_error() {
    let f = Fixture::new();
    let (downloads, _) = f.pack(true);
    let installed = f.target("installed");
    supplier_library::install_pack(&f.root, &downloads, &installed).unwrap();
    assert!(!installed.join("catalog-and-metadata.tar.xz").exists());
    let pack = installed.join("source-pack.json");
    let pack_sha = sha256(fs::read(&pack).unwrap());
    set_source_binding(
        &f.index,
        json!({"kind":"local_lossless_shards","source_pack_manifest":"sources/.indexes/installed/source-pack.json","source_pack_manifest_sha256":pack_sha,"metadata_transport":"verified_installed_files"}),
    );
    let target = f.target("new-native");
    save(
        &target.join("source.json"),
        &json!({"schema":"ontology-engineering.misumi-source-location/v1","archive_root":"../installed/cache/archive","index_path":"../../catalog.sqlite3","index_sha256":sha256(fs::read(&f.index).unwrap()),"source_catalogs":f.source_catalogs(),"verification":"new index registered against unchanged installed pack","source_pack_manifest":"../installed/source-pack.json","source_pack_manifest_sha256":pack_sha}),
    );
    assert_eq!(
        supplier_library::verify(&f.root, &target).unwrap()["status"],
        "verified"
    );
    f.consumer(&target);
    assert_eq!(
        supplier_library::restore_books(&f.root, &target).unwrap()["restored"],
        2
    );
    fs::write(
        installed.join("catalog-and-metadata.tar.xz"),
        b"existing corrupt tar must not fall back",
    )
    .unwrap();
    assert!(supplier_library::verify(&f.root, &target).is_err());
    assert!(supplier_library::verify(&f.root, &installed).is_err());
    assert!(supplier_library::restore_books(&f.root, &target).is_err());
    fs::remove_file(installed.join("catalog-and-metadata.tar.xz")).unwrap();
    let metadata = installed.join(format!("archive/tmp/pdfs/{}/metadata.json", CODES[0]));
    let mut raw = fs::read(&metadata).unwrap();
    raw.push(b' ');
    fs::write(metadata, raw).unwrap();
    assert!(supplier_library::verify(&f.root, &target).is_err());
}

#[test]
fn registered_pack_cannot_redirect_verification_or_book_publication() {
    let f = Fixture::new();
    let (downloads, _) = f.pack(true);
    let manifest_sha = sha256(fs::read(downloads.join("source-pack.json")).unwrap());
    set_source_binding(
        &f.index,
        json!({"kind":"local_lossless_shards","source_pack_manifest":"sources/downloads/source-pack.json","source_pack_manifest_sha256":manifest_sha}),
    );
    let target = f.target("pack-native");
    let path = target.join("source.json");
    let mut registration = json!({"schema":"ontology-engineering.misumi-source-location/v1","archive_root":"../../downloads/cache/archive","index_path":"../../catalog.sqlite3","index_sha256":sha256(fs::read(&f.index).unwrap()),"source_catalogs":f.source_catalogs(),"verification":"source-bound test registration","source_pack_manifest":"../../downloads/source-pack.json","source_pack_manifest_sha256":manifest_sha});
    save(&path, &registration);
    assert_eq!(
        supplier_library::verify(&f.root, &target).unwrap()["status"],
        "verified"
    );
    f.consumer(&target);
    let redirected = f.root.path().join("sources/unrequested-books");
    for existing in [false, true] {
        if existing {
            fs::create_dir_all(&redirected).unwrap();
            fs::write(redirected.join("owner.txt"), b"owner bytes").unwrap();
        }
        registration["archive_root"] = json!("../../unrequested-books");
        save(&path, &registration);
        for result in [
            supplier_library::verify(&f.root, &target),
            supplier_library::restore_books(&f.root, &target),
        ] {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("registered_source_pack_archive_mismatch")
            );
        }
        assert!(Provider::open(&f.root, "misumi", Some(&target)).is_err());
        if existing {
            assert_eq!(fs::read_dir(&redirected).unwrap().count(), 1);
            assert_eq!(
                fs::read(redirected.join("owner.txt")).unwrap(),
                b"owner bytes"
            );
        } else {
            assert!(!redirected.exists());
        }
        assert!(!downloads.join("cache/archive/output").exists());
    }
}

#[test]
fn mixed_library_manifests_cannot_select_different_verification_and_query_sources() {
    let f = Fixture::new();
    let (downloads, _) = f.pack(true);
    let target = f.target("installed");
    supplier_library::install_pack(&f.root, &downloads, &target).unwrap();
    assert!(supplier_library::register(&f.root, &f.archive, &f.index, &target).is_err());
    assert!(!target.join("source.json").exists());
    save(
        &target.join("source.json"),
        &json!({"schema":"ontology-engineering.misumi-source-location/v1"}),
    );
    assert!(supplier_library::verify(&f.root, &target).is_err());
    assert!(supplier_library::restore_books(&f.root, &target).is_err());
}

#[test]
fn expanded_import_registration_and_provider_preserve_exact_sources() {
    let f = Fixture::new();
    let registered = f.target("registered");
    let imported = f.target("imported");
    assert_eq!(
        supplier_library::register(&f.root, &f.archive, &f.index, &registered).unwrap()["copied_source_files"],
        0
    );
    assert_eq!(fs::read_dir(&registered).unwrap().count(), 1);
    f.consumer(&registered);
    assert_eq!(
        supplier_library::verify(&f.root, &registered).unwrap()["status"],
        "verified"
    );
    assert!(supplier_library::register(&f.root, &f.archive, &f.index, &registered).is_err());
    let report = supplier_library::import_data(&f.root, &f.archive, &f.index, &imported).unwrap();
    assert_eq!(report["files"], 13);
    f.consumer(&imported);
    assert_eq!(
        supplier_library::verify(&f.root, &imported).unwrap()["status"],
        "verified"
    );
    for code in CODES {
        let relative = format!("tmp/pdfs/{code}/pages/0001.pdf");
        assert_eq!(
            fs::read(imported.join("archive").join(&relative)).unwrap(),
            fs::read(f.archive.join(relative)).unwrap()
        );
    }
}

#[test]
fn registered_library_moves_as_a_unit_without_rewriting_location() {
    let f = Fixture::new();
    let target = f.target("registered");
    supplier_library::register(&f.root, &f.archive, &f.index, &target).unwrap();
    let before = fs::read(target.join("source.json")).unwrap();
    assert!(!String::from_utf8_lossy(&before).contains(f.root.path().to_str().unwrap()));
    let moved = f.outer.join("moved");
    fs::rename(f.root.path(), &moved).unwrap();
    let root = SkillRoot::new(&moved).unwrap();
    let target = moved.join("sources/.indexes/registered");
    assert_eq!(
        supplier_library::verify(&root, &target).unwrap()["status"],
        "verified"
    );
    assert_eq!(fs::read(target.join("source.json")).unwrap(), before);
    assert!(
        Provider::open(&root, "misumi", Some(&target))
            .unwrap()
            .page(&format!("{}:page:1", CODES[0]))
            .is_ok()
    );
}

#[test]
fn index_drift_and_rewritten_import_manifest_cannot_override_source_mapping() {
    let f = Fixture::new();
    let registered = f.target("registered");
    supplier_library::register(&f.root, &f.archive, &f.index, &registered).unwrap();
    let imported = f.target("imported");
    supplier_library::import_data(&f.root, &f.archive, &f.index, &imported).unwrap();
    let db = Connection::open(&f.index).unwrap();
    db.execute(
        "UPDATE meta SET value='\"changed\"' WHERE key='version'",
        [],
    )
    .unwrap();
    drop(db);
    assert!(supplier_library::verify(&f.root, &registered).is_err());
    let path = imported.join("data-manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let entry = manifest["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["role"] == "source_page_pdf")
        .unwrap();
    let source = imported.join(entry["path"].as_str().unwrap());
    let mut raw = fs::read(&source).unwrap();
    raw[10] ^= 1;
    fs::write(&source, &raw).unwrap();
    entry["sha256"] = json!(sha256(raw));
    save(&path, &manifest);
    assert!(supplier_library::verify(&f.root, &imported).is_err());
}

#[test]
fn existing_destinations_and_outside_sources_are_never_overwritten() {
    let f = Fixture::new();
    let target = f.target("exists");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep"), "owner bytes").unwrap();
    assert!(supplier_library::import_data(&f.root, &f.archive, &f.index, &target).is_err());
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"owner bytes");
    assert!(
        supplier_library::register(
            &f.root,
            &f.archive,
            &f.index,
            &f.root.path().join("var/output")
        )
        .is_err()
    );
    assert!(
        supplier_library::register(&f.root, &f.outer, &f.index, &f.target("external")).is_err()
    );
    #[cfg(unix)]
    {
        let link = f.root.path().join("sources/linked");
        std::os::unix::fs::symlink(&f.archive, &link).unwrap();
        assert!(
            supplier_library::register(&f.root, &link, &f.index, &f.target("symlink")).is_err()
        );
    }
}

#[test]
fn downloaded_install_keeps_all_pages_compressed_and_provider_operational() {
    for full in [false, true] {
        let f = Fixture::new();
        let (downloads, manifest) = f.pack(full);
        let target = f.target("installed");
        let before = fs::read(downloads.join("source-pack.json")).unwrap();
        let report = supplier_library::install_pack(&f.root, &downloads, &target).unwrap();
        assert_eq!(report["pages_expanded"], 0);
        assert_eq!(report["pages_available"], 2);
        assert_eq!(report["full_pdfs_archive_preserved"], full);
        assert_eq!(fs::read(target.join("source-pack.json")).unwrap(), before);
        assert!(!target.join("cache/archive/tmp").exists());
        assert_eq!(
            supplier_library::verify(&f.root, &target).unwrap()["status"],
            "verified"
        );
        f.consumer(&target);
        assert!(supplier_library::install_pack(&f.root, &downloads, &target).is_err());
        let shard = target.join(manifest["segments"][0]["path"].as_str().unwrap());
        let mut bytes = fs::read(&shard).unwrap();
        bytes[1] ^= 1;
        fs::write(shard, bytes).unwrap();
        assert!(supplier_library::verify(&f.root, &target).is_err());
    }
}

#[test]
fn malformed_metadata_archive_never_leaves_a_partially_installed_library() {
    for bad in ["hash", "duplicate", "missing", "symlink", "traversal"] {
        let f = Fixture::new();
        let (downloads, mut manifest) = f.pack(false);
        let mut values = members(&downloads.join("catalog-and-metadata.tar.xz"));
        match bad {
            "hash" => values[1].1[0] ^= 1,
            "duplicate" => values.push(values[1].clone()),
            "missing" => {
                values.pop();
            }
            "symlink" => values[1].2 = true,
            "traversal" => values.push(("../escaped".into(), b"bad".to_vec(), false)),
            _ => unreachable!(),
        }
        rewrite_archive(&downloads, &mut manifest, "metadata", &values, false);
        let target = f.target(bad);
        assert!(
            supplier_library::install_pack(&f.root, &downloads, &target).is_err(),
            "{bad}"
        );
        assert!(!target.exists());
        assert!(!f.root.path().join("escaped").exists());
        let cache = f.root.path().join("var/cache/knowledge");
        if cache.exists() {
            assert!(fs::read_dir(cache).unwrap().all(|e| {
                !e.unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".supplier-")
            }));
        }
    }
}

#[test]
fn self_consistent_transport_hash_rewrite_does_not_bypass_metadata_index_identity() {
    let f = Fixture::new();
    let (downloads, mut manifest) = f.pack(false);
    let mut values = members(&downloads.join("catalog-and-metadata.tar.xz"));
    let item = values
        .iter_mut()
        .find(|(name, _, _)| name.ends_with("/metadata.json"))
        .unwrap();
    let mut changed: Value = serde_json::from_slice(&item.1).unwrap();
    changed["ebook"]["unreviewed_extra"] = json!(true);
    item.1 = serde_json::to_vec(&changed).unwrap();
    rewrite_archive(&downloads, &mut manifest, "metadata", &values, true);
    assert!(supplier_library::install_pack(&f.root, &downloads, &f.target("bad")).is_err());
    assert!(!f.target("bad").exists());
}

#[test]
fn full_books_are_only_restored_explicitly_and_second_restore_is_verified() {
    let f = Fixture::new();
    let (downloads, _) = f.pack(true);
    let target = f.target("installed");
    supplier_library::install_pack(&f.root, &downloads, &target).unwrap();
    assert!(!target.join("cache/archive/output").exists());
    let result = supplier_library::restore_books(&f.root, &target).unwrap();
    assert_eq!(result["restored"], 2);
    for code in CODES {
        assert_eq!(
            fs::read(target.join(format!("cache/archive/output/pdf/{code}.pdf"))).unwrap(),
            pdf()
        );
    }
    assert_eq!(
        supplier_library::restore_books(&f.root, &target).unwrap()["restored"],
        0
    );
    assert!(!target.join("cache/archive/tmp").exists());
}

#[test]
fn corrupt_full_archive_is_rejected_before_publishing_any_book() {
    for bad in ["hash", "missing", "symlink", "traversal"] {
        let f = Fixture::new();
        let (downloads, mut manifest) = f.pack(true);
        let mut values = members(&downloads.join("full-pdfs.tar.xz"));
        match bad {
            "hash" => values[0].1[3] ^= 1,
            "missing" => {
                values.pop();
            }
            "symlink" => values[0].2 = true,
            "traversal" => values.push(("../outside".into(), b"bad".to_vec(), false)),
            _ => unreachable!(),
        }
        rewrite_archive(
            &downloads,
            &mut manifest,
            "full_pdfs_archive",
            &values,
            false,
        );
        let target = f.target("installed");
        supplier_library::install_pack(&f.root, &downloads, &target).unwrap();
        assert!(
            supplier_library::restore_books(&f.root, &target).is_err(),
            "{bad}"
        );
        assert!(!target.join("cache/archive/output").exists());
    }
}

#[test]
fn missing_declared_full_archive_or_qc_and_symlink_shards_are_denied() {
    let f = Fixture::new();
    let (downloads, _) = f.pack(true);
    fs::remove_file(downloads.join("full-pdfs.tar.xz")).unwrap();
    assert!(supplier_library::install_pack(&f.root, &downloads, &f.target("missing")).is_err());
    fs::remove_file(f.archive.join(format!("output/qc/{}.json", CODES[0]))).unwrap();
    assert!(supplier_library::register(&f.root, &f.archive, &f.index, &f.target("qc")).is_err());
    #[cfg(unix)]
    {
        let f = Fixture::new();
        let (downloads, _) = f.pack(false);
        let source = downloads.join("segments/pages.tar.xz");
        let outside = f.root.path().join("sources/outside.tar.xz");
        fs::rename(&source, &outside).unwrap();
        std::os::unix::fs::symlink(outside, &source).unwrap();
        assert!(supplier_library::install_pack(&f.root, &downloads, &f.target("link")).is_err());
    }
}

#[test]
fn malformed_registration_and_rebound_old_index_fail_closed() {
    let f = Fixture::new();
    let target = f.target("registered");
    supplier_library::register(&f.root, &f.archive, &f.index, &target).unwrap();
    let path = target.join("source.json");
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for key in ["unexpected", "source_pack_manifest_sha256"] {
        let mut bad = original.clone();
        bad[key] = json!("extra");
        save(&path, &bad);
        assert!(supplier_library::verify(&f.root, &target).is_err());
    }
    let mut bad = original.clone();
    bad["index_path"] = json!("../../../../outside.sqlite3");
    save(&path, &bad);
    assert!(supplier_library::verify(&f.root, &target).is_err());
}

#[test]
#[ignore = "requires actual Poppler; creates tiny supplier PDFs, never reads production sources"]
fn native_downloaded_pack_rebuild_register_verify_query_and_restore() {
    use oe_knowledge::{
        poppler::PopplerTools,
        supplier_index::{self, BuildInput, BuildOptions},
    };
    let f = Fixture::new();
    let (downloads, manifest) = f.pack(true);
    let target = f.target("rebuilt");
    let before = fs::read(downloads.join("source-pack.json")).unwrap();
    let tools = PopplerTools {
        pdfinfo: "/opt/homebrew/bin/pdfinfo".into(),
        pdftotext: "/opt/homebrew/bin/pdftotext".into(),
        pdftoppm: "/opt/homebrew/bin/pdftoppm".into(),
    };
    let result = supplier_index::build(
        f.root.path(),
        &BuildInput::SourcePack(downloads.join("source-pack.json")),
        &BuildOptions {
            data_root: target.clone(),
            tools: tools.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result["status"], "indexed");
    assert_eq!(
        fs::read(downloads.join("source-pack.json")).unwrap(),
        before
    );
    assert_ne!(
        sha256(fs::read(target.join("catalog.sqlite3")).unwrap()),
        manifest["index"]["sha256"]
    );
    assert_eq!(
        supplier_library::verify(&f.root, &target).unwrap()["status"],
        "verified"
    );
    let provider = Provider::open(&f.root, "misumi", Some(&target)).unwrap();
    let page = provider.page(&format!("{}:page:1", CODES[0])).unwrap();
    assert_eq!(page["text_origin"], "poppler_native_text");
    let result = provider.source_result(&page, "shaft", &tools).unwrap();
    assert_eq!(result["original_verified"], true);
    assert_eq!(
        supplier_library::restore_books(&f.root, &target).unwrap()["restored"],
        2
    );
    assert_eq!(
        fs::read(downloads.join("source-pack.json")).unwrap(),
        before
    );
}
