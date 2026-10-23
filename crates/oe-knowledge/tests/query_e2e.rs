//! Real Rust query orchestration over tiny synthetic original PDFs and indexes.
//! No model transport is used; source checks do not establish applicability.
use oe_core::{
    SkillRoot,
    json::{canonical_digest, strict_json},
    sha256,
};
use oe_knowledge::{
    citations, context, feedback, packet,
    poppler::{self, PopplerLimits, PopplerTools},
    query::{self, ProviderSpec, QueryOptions},
    registration::{self, RegistrationOptions},
    text,
};
use rusqlite::{Connection, params};
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    path: PathBuf,
    root: SkillRoot,
}
impl Fixture {
    fn new(sources: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = producer
            .join("var/tests/unified-system/rust-tests")
            .join(format!(
                "knowledge-query-e2e-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(path.join("var/projects/p")).unwrap();
        fs::create_dir_all(path.join("sources/books")).unwrap();
        fs::write(path.join("SKILL.md"), "synthetic test root").unwrap();
        for relative in [
            "references/supplier-knowledge-policy.json",
            "references/context-routing-instructions.json",
            oe_semantic::LOCK_PATH,
            oe_semantic::RUNTIME_LOCK_PATH,
        ] {
            copy(producer, &path, relative);
        }
        let lock = strict_json(&fs::read(producer.join(oe_semantic::LOCK_PATH)).unwrap()).unwrap();
        copy(
            producer,
            &path,
            lock["bundles"]["engineering-judgment-intake"]["path"]
                .as_str()
                .unwrap(),
        );
        let root = SkillRoot::new(&path).unwrap();
        let f = Self { path, root };
        f.project("r1");
        if sources {
            f.sources();
        }
        f
    }
    fn write(&self, path: &str, value: &Value) {
        let p = self.path.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
    fn project(&self, revision: &str) {
        self.write("var/projects/p/source.json",&json!({"project":{"id":"synthetic-project","revision":revision},"object":{"id":"synthetic-object","revision":revision,"name":"工程"},"decision_id":"compare-conditions","goal":"Compare source conditions before adoption","statements":[{"id":"condition","text":"Fixture operating condition only","status":"requirement","source_refs":[]}],"unknowns":["Actual load has not been measured"]}));
    }
    fn projection(&self) -> Value {
        json!({"schema":context::PROJECTION_SCHEMA,"base":{"schema":context::SCHEMA,"task_id":"fixture-query"},"fields":(["/project/id","/project/revision","/object/id","/object/revision","/object/name","/decision_id","/goal","/statements","/unknowns"].map(|p|json!({"source":"var/projects/p/source.json","pointer":p,"target":p})))})
    }
    fn options(&self, name: &str) -> QueryOptions {
        QueryOptions {
            retrieval_order: "context_first".into(),
            query: "工程 条件".into(),
            context: Some(self.projection()),
            history: vec![],
            providers: vec![
                ProviderSpec {
                    kind: "local_pdf".into(),
                    index: None,
                },
                ProviderSpec {
                    kind: "misumi".into(),
                    index: None,
                },
            ],
            candidates: 4,
            top: 2,
            output: format!("var/projects/p/{name}").into(),
            full_text: true,
            tools: tools(),
        }
    }
    fn sources(&self) {
        let local = pdf(false, 3);
        fs::write(self.path.join("sources/books/条件-original.pdf"), &local).unwrap();
        self.write("sources/books/manifest.json",&json!({"schema":"ontology-engineering.local-pdf-manifest/v1","sources":[{"id":"conditions","title":"Synthetic book","edition":"fixture-1","path":"条件-original.pdf","sha256":sha256(&local),"chapters":[{"id":"conditions","title":"工程条件","start_page":1,"end_page":3,"aliases":[]}]}]}));
        registration::register(
            &self.path,
            &self.path.join("sources/books/manifest.json"),
            &RegistrationOptions {
                tools: tools(),
                ..Default::default()
            },
        )
        .unwrap();
        let single = pdf(true, 1);
        let selected = self.path.join("sources/books/supplier-single.pdf");
        fs::write(&selected, &single).unwrap();
        let body = poppler::extract(
            &self.path,
            &selected,
            &sha256(&single),
            &tools(),
            &PopplerLimits::default(),
        )
        .unwrap()
        .pages
        .remove(0);
        assert!(
            body.contains("工程")
                && body.contains("供应")
                && body.contains('𠮷')
                && body.contains("e\u{301}")
        );
        let directory = self.path.join("sources/.indexes/misumi");
        fs::create_dir_all(directory.join("segments")).unwrap();
        let db = Connection::open(directory.join("catalog.sqlite3")).unwrap();
        db.execute_batch(registration::TABLES).unwrap();
        db.execute_batch("CREATE TABLE catalogs(code TEXT PRIMARY KEY,label TEXT,title TEXT,page_count INTEGER,pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT);CREATE TABLE hotspots(page_id TEXT,ordinal INTEGER,kind TEXT,keyword TEXT,keyword_normalized TEXT,target TEXT,source_json TEXT,PRIMARY KEY(page_id,ordinal));CREATE INDEX hotspot_model ON hotspots(keyword_normalized,kind);").unwrap();
        db.execute(
            "INSERT INTO catalogs VALUES(?,?,?,?,?,?,?,?)",
            params![
                "supplier",
                "Synthetic supplier",
                "Synthetic historical supplier",
                42,
                "books/assembly.pdf",
                "0".repeat(64),
                "1".repeat(64),
                "2".repeat(64)
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO chapters VALUES(?,?,?,?,?,?)",
            params!["supplier:chapter", "supplier", "工程条件", 42, 42, "{}"],
        )
        .unwrap();
        db.execute(
            "INSERT INTO pages VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                "supplier:page:42",
                "supplier",
                42,
                42,
                "四十",
                "supplier:chapter",
                "pages/selected.pdf",
                "https://example.invalid/fixture.pdf",
                sha256(&single),
                single.len() as i64,
                &body,
                sha256(&body),
                sha256(&body),
                body.chars().count() as i64,
                body.chars().count() as i64,
                0,
                "[]",
                "[]",
                0,
                "{}"
            ],
        )
        .unwrap();
        let row = db.last_insert_rowid();
        db.execute(
            "INSERT INTO search(rowid,models,titles,body,chapter) VALUES(?,?,?,?,?)",
            params![
                row,
                "",
                "工程 条件",
                text::tokens(&body).join(" "),
                "工程 条件"
            ],
        )
        .unwrap();
        let mut meta = json!({"schema":"misumi.catalog-index/v1","version":"0.1.0","counts":{"pages":1,"chapters":1,"catalogs":1}});
        meta["fingerprint"] = json!(canonical_digest(&meta).unwrap());
        for (k, v) in meta.as_object().unwrap() {
            db.execute("INSERT INTO meta VALUES(?,?)", params![k, v.to_string()])
                .unwrap();
        }
        drop(db);
        let compressed = directory.join("segments/small.tar.xz");
        let encoder = xz2::write::XzEncoder::new(fs::File::create(&compressed).unwrap(), 6);
        let mut tar = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(single.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        tar.append_data(&mut header, "pages/selected.pdf", single.as_slice())
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let index = fs::read(directory.join("catalog.sqlite3")).unwrap();
        let archive = fs::read(&compressed).unwrap();
        self.write("sources/.indexes/misumi/source-pack.json",&json!({"schema":"ontology-engineering.misumi-source-pack/v1","index":{"path":"catalog.sqlite3","bytes":index.len(),"sha256":sha256(&index)},"segments":[{"id":"small","path":"segments/small.tar.xz","bytes":archive.len(),"sha256":sha256(&archive),"files":[{"path":"pages/selected.pdf","bytes":single.len(),"sha256":sha256(&single)}]}],"assembled_pdfs":[{"catalog":"supplier","path":"books/assembly.pdf","sha256":"0".repeat(64),"bytes":42,"download_url":"https://example.invalid/fixture-book.pdf"}]}));
        // The query must recover the exact compressed member, not borrow this input.
        fs::remove_file(selected).unwrap();
    }
    fn install_supplier_assembly(&self) -> PathBuf {
        let directory = self.path.join("sources/.indexes/misumi");
        let assembled = directory.join("cache/archive/books/assembly.pdf");
        let bytes = pdf(true, 42);
        fs::create_dir_all(assembled.parent().unwrap()).unwrap();
        fs::write(&assembled, &bytes).unwrap();
        let db = Connection::open(directory.join("catalog.sqlite3")).unwrap();
        db.execute(
            "UPDATE catalogs SET pdf_sha256=? WHERE code='supplier'",
            params![sha256(&bytes)],
        )
        .unwrap();
        drop(db);
        let index = fs::read(directory.join("catalog.sqlite3")).unwrap();
        let mut manifest =
            strict_json(&fs::read(directory.join("source-pack.json")).unwrap()).unwrap();
        manifest["index"]["sha256"] = json!(sha256(&index));
        manifest["index"]["bytes"] = json!(index.len());
        manifest["assembled_pdfs"][0]["sha256"] = json!(sha256(&bytes));
        manifest["assembled_pdfs"][0]["bytes"] = json!(bytes.len());
        self.write("sources/.indexes/misumi/source-pack.json", &manifest);
        assembled
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
fn copy(from: &Path, to: &Path, name: &str) {
    let dest = to.join(name);
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::copy(from.join(name), dest).unwrap();
}
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for item in fs::read_dir(from).unwrap() {
        let item = item.unwrap();
        let kind = item.file_type().unwrap();
        assert!(!kind.is_symlink());
        if kind.is_dir() {
            copy_tree(&item.path(), &to.join(item.file_name()));
        } else {
            assert!(kind.is_file());
            fs::copy(item.path(), to.join(item.file_name())).unwrap();
        }
    }
}
fn tools() -> PopplerTools {
    PopplerTools {
        pdfinfo: "/opt/homebrew/bin/pdfinfo".into(),
        pdftotext: "/opt/homebrew/bin/pdftotext".into(),
        pdftoppm: "/opt/homebrew/bin/pdftoppm".into(),
    }
}

// Actual Unicode text, two text pages and a bitmap-only final page. The supplier
// member is one physical page even though the archive catalog calls it page 42.
fn pdf(supplier: bool, count: usize) -> Vec<u8> {
    fn stream(b: &[u8]) -> Vec<u8> {
        let mut r = format!("<< /Length {} >>\nstream\n", b.len()).into_bytes();
        r.extend(b);
        r.extend(b"\nendstream");
        r
    }
    let cmap=b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Fixture def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange 7 beginbfchar <0001> <5DE5> <0002> <7A0B> <0003> <0020> <0004> <00650301> <0005> <D842DFB7> <0006> <67614EF6> <0007> <4F9B5E94> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
    let mut objects=vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),vec![],b"<< /Type /Font /Subtype /Type0 /BaseFont /Arial /Encoding /Identity-H /DescendantFonts [4 0 R] /ToUnicode 6 0 R >>".to_vec(),b"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /Arial /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 5 0 R /DW 600 /CIDToGIDMap /Identity >>".to_vec(),b"<< /Type /FontDescriptor /FontName /Arial /Flags 32 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 >>".to_vec(),stream(cmap),b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\n\xff\x00\x00\nendstream".to_vec()];
    let mut text = String::from("BT /F1 12 Tf 20 750 Td ");
    for _ in 0..20 {
        text.push_str(if supplier {
            "<0007000300010002000300040003000500030006> Tj 0 -20 Td "
        } else {
            "<00010002000300040003000500030006> Tj 0 -20 Td "
        });
    }
    text.push_str("ET");
    let mut kids = vec![];
    for i in 0..count {
        let n = objects.len() + 1;
        kids.push(format!("{n} 0 R"));
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 3 0 R >> /XObject << /Im0 7 0 R >> >> /Contents {} 0 R >>",n+1).into_bytes());
        objects.push(stream(if i == 2 {
            b"q 240 0 0 240 40 40 cm /Im0 Do Q"
        } else {
            text.as_bytes()
        }));
    }
    objects[1] = format!(
        "<< /Type /Pages /Count {count} /Kids [{}] >>",
        kids.join(" ")
    )
    .into_bytes();
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for n in offsets {
        out.extend(format!("{n:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[test]
fn native_query_two_sources_originals_packet_deferred_feedback_and_explicit_next_revision() {
    let f = Fixture::new(true);
    let report = query::run(&f.root, &f.options("query-1")).unwrap();
    assert_eq!(
        report["status"],
        "completed",
        "{}",
        serde_json::to_string_pretty(&report).unwrap()
    );
    assert_eq!(report["results"].as_array().unwrap().len(), 2);
    let providers = report["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["provider"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        providers,
        std::collections::BTreeSet::from(["local_pdf", "misumi"])
    );
    assert_eq!(report["usage"]["network_requests"], 0);
    assert_eq!(report["usage"]["model_calls"], 0);
    assert_eq!(
        report["context_snapshot"]["unknowns"][0],
        "Actual load has not been measured"
    );
    assert_eq!(report["semantic_review"], "not_run");
    for result in report["results"].as_array().unwrap() {
        assert!(!Path::new(result["source_pdf"].as_str().unwrap()).is_absolute());
        let raw = fs::read(f.path.join(result["source_pdf"].as_str().unwrap())).unwrap();
        assert_eq!(result["source_sha256"], sha256(&raw));
        assert!(result["native_text"].as_str().unwrap().contains('𠮷'));
        assert!(
            result["knowledge_units"]
                .as_array()
                .unwrap()
                .iter()
                .all(|u| u["applicability"] == "not_assessed")
        );
        let view = citations::view(&f.path, result, 72, &tools()).unwrap();
        assert_eq!(view["identity"]["whole_page"], true);
        assert_eq!(view["numeric_verification"], "not_run");
        if result["provider"] == "local_pdf" {
            assert!(result.get("source_relative_path").is_none());
        }
        if result["provider"] == "misumi" {
            assert_eq!(result["pdf_page"], 42);
            assert_eq!(result["source_file_page"], 1);
            assert_eq!(result["printed_page"], "四十");
            assert_eq!(result["assembled_pdf_status"], "not_downloaded");
        }
    }
    let c = report["context_snapshot"].clone();
    let provider = oe_knowledge::sources::Provider::open(&f.root, "local_pdf", None).unwrap();
    let image_page = provider.page("local_pdf:conditions:page:3").unwrap();
    assert_eq!(image_page["native_text_missing"], true);
    let original = provider.source_result(&image_page, "", &tools()).unwrap();
    let image_view = citations::view(&f.path, &original, 72, &tools()).unwrap();
    assert_eq!(image_view["identity"]["whole_page"], true);
    assert_eq!(image_view["identity"]["source_file_page"], 3);
    assert_eq!(image_view["interpretation"], "not_run");
    let mut unavailable_tools = tools();
    unavailable_tools.pdfinfo = f.path.join("missing-pdfinfo");
    assert!(packet::make_with_tools(&f.root, &report, &c, &unavailable_tools).is_err());
    let p = packet::make_with_tools(&f.root, &report, &c, &tools()).unwrap();
    let mut missing_locator = p.clone();
    let local_source = missing_locator["evidence_candidates"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| {
            e["source"]["catalog"]
                .as_str()
                .unwrap()
                .starts_with("local_pdf:")
        })
        .unwrap();
    local_source["source"]
        .as_object_mut()
        .unwrap()
        .remove("source_locator");
    assert!(
        packet::verify_with_tools(&f.root, &missing_locator, &c, &tools()).is_err(),
        "removing a multi-page local locator must not downgrade physical-page checks"
    );
    f.write("var/projects/p/packet-1.json", &p);
    assert_eq!(
        packet::verify_with_tools(&f.root, &p, &c, &tools()).unwrap()["status"],
        "ready_for_agent_review"
    );
    let first = &p["evidence_candidates"][0];
    let record=feedback::append_with_tools(&f.root,&json!({"packet_path":"var/projects/p/packet-1.json","context":c,"candidate_id":first["id"],"disposition":"deferred","reason":"Need actual load measurement","next_check":"Measure load before applicability assessment","output":"var/projects/p/feedback/first.json"}),&tools()).unwrap();
    assert_eq!(record["semantic_review"], "not_run");
    let reference = json!({"path":"var/projects/p/feedback/first.json","sha256":sha256(fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap())});
    f.project("r2");
    let mut options = f.options("query-2");
    options.history = vec![reference];
    let next = query::run(&f.root, &options).unwrap();
    assert_eq!(next["status"], "completed");
    assert_eq!(next["context_snapshot"]["object"]["revision"], "r2");
    assert_eq!(
        next["explicit_history"][0]["historical_context_source_freshness"]["status"],
        "stale"
    );
    assert_eq!(
        next["explicit_history"][0]["model_observation"]["historical_only"],
        true
    );
    assert_eq!(
        next["explicit_history"][0]["model_observation"]["disposition"],
        "deferred"
    );

    let old = fs::read(f.path.join("var/projects/p/feedback/first.json")).unwrap();
    assert_eq!(strict_json(&old).unwrap(), record);
    let next_packet =
        packet::make_with_tools(&f.root, &next, &next["context_snapshot"], &tools()).unwrap();
    f.write("var/projects/p/packet-2.json", &next_packet);
    let receiver = Fixture::new(false);
    for relative in [
        "sources",
        "var/projects",
        "var/cache/knowledge/source-pages",
    ] {
        copy_tree(&f.path.join(relative), &receiver.path.join(relative));
    }
    fs::write(
        f.path.join("sources/books/条件-original.pdf"),
        b"producer source deliberately changed after relocation",
    )
    .unwrap();
    let relocated_context = context::project(&receiver.root, &receiver.projection()).unwrap();
    assert_eq!(
        packet::verify_with_tools(&receiver.root, &next_packet, &relocated_context, &tools())
            .unwrap()["status"],
        "ready_for_agent_review"
    );
    let historical = feedback::load_reference_with_tools(
        &receiver.root,
        &options.history[0],
        &relocated_context,
        &tools(),
    )
    .unwrap();
    assert_eq!(historical["status"], "history_verified");
    assert_eq!(historical["model_observation"]["historical_only"], true);
    assert!(
        packet::verify_with_tools(&f.root, &next_packet, &next["context_snapshot"], &tools())
            .is_err()
    );
}

#[test]
fn local_query_preserves_names_while_skipping_unsearchable_internal_fragments() {
    let f = Fixture::new(true);
    for (index, name) in [
        "000210 工程",
        "000210 11 22 33 44 55 66 工程",
        "!!! 000210 ;，， 工程",
        "000210 !!!",
    ]
    .iter()
    .enumerate()
    {
        let source_path = f.path.join("var/projects/p/source.json");
        let mut source = strict_json(&fs::read(&source_path).unwrap()).unwrap();
        source["object"]["name"] = json!(name);
        f.write("var/projects/p/source.json", &source);
        let before = fs::read(&source_path).unwrap();
        let mut options = f.options(&format!("local-name-{index}"));
        options.providers.truncate(1);
        let report = query::run(&f.root, &options).unwrap();
        assert_eq!(
            report["status"], "completed",
            "{name}: {}",
            report["errors"]
        );
        assert!(!report["results"].as_array().unwrap().is_empty(), "{name}");
        assert_eq!(report["query"], options.query);
        assert_eq!(report["context_snapshot"]["object"]["name"], *name);
        assert_eq!(
            report["context_snapshot"]["statements"],
            source["statements"]
        );
        assert_eq!(report["context_snapshot"]["unknowns"], source["unknowns"]);
        assert_eq!(fs::read(&source_path).unwrap(), before);
        assert_eq!(report["usage"]["network_requests"], 0);
        assert_eq!(report["usage"]["fixture_requests"], 0);
        assert_eq!(report["routing"]["topic_judgment"], "not_run");
        assert_eq!(report["review_plan"]["adoption"], "pending_agent_review");
        assert_eq!(report["review_plan"]["execution"], "not_run");
        let labels: Vec<_> = report["retrieval"]["candidate_replays"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|replay| replay["pools"].as_array().unwrap())
            .filter_map(|pool| pool["label"].as_str())
            .filter(|label| label.starts_with("explicit_object:"))
            .collect();
        if name.contains("工程") {
            assert_eq!(labels, vec!["explicit_object:工程"], "{name}");
        } else {
            assert!(labels.is_empty(), "{name}: {labels:?}");
        }
    }
}

#[test]
fn external_body_search_still_rejects_empty_tokenizer_terms() {
    let f = Fixture::new(true);
    let provider = oe_knowledge::sources::Provider::open(&f.root, "local-pdf", None).unwrap();
    assert_eq!(
        provider
            .retrieve_body("000210 !!!", "", 1, &[])
            .unwrap_err()
            .to_string(),
        "body_query_no_search_terms"
    );
    assert!(
        !provider
            .retrieve_body("工程", "", 1, &[])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn no_registered_library_keeps_question_and_unknowns_without_dispatch() {
    let f = Fixture::new(false);
    let report = query::run(&f.root, &f.options("empty")).unwrap();
    assert_eq!(report["status"], "not_ready");
    assert_eq!(report["usage"]["model_calls"], 0);
    assert_eq!(
        report["context_snapshot"]["goal"],
        "Compare source conditions before adoption"
    );
    assert_eq!(report["knowledge_gaps"][0]["status"], "unresolved");
    assert_eq!(report["semantic_review"], "not_run");
}

#[test]
fn source_library_query_packet_and_feedback_preserve_source_boundaries() {
    let f = Fixture::new(false);
    let bytes = pdf(false, 3);
    fs::write(f.path.join("sources/books/条件-original.pdf"), &bytes).unwrap();
    f.write("sources/books/manifest.json", &json!({
        "schema":"ontology-engineering.local-pdf-manifest/v1",
        "sources":[{"id":"conditions","title":"Synthetic book","edition":"fixture-1",
            "path":"条件-original.pdf","sha256":sha256(&bytes),
            "chapters":[{"id":"conditions","title":"工程条件","start_page":1,"end_page":3,"aliases":[]}]}]
    }));
    registration::register(
        &f.path,
        &f.path.join("sources/books/manifest.json"),
        &RegistrationOptions {
            tools: tools(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut options = f.options("books-query");
    options.providers.retain(|p| p.kind == "local_pdf");
    let report = query::run(&f.root, &options).unwrap();
    assert_eq!(report["status"], "completed");
    let results = report["results"].as_array().unwrap();
    assert!(!results.is_empty());
    assert!(
        results
            .iter()
            .all(|r| r["source_pdf"] == "sources/books/条件-original.pdf")
    );
    let context = report["context_snapshot"].clone();
    let packet = packet::make_with_tools(&f.root, &report, &context, &tools()).unwrap();
    assert_eq!(
        packet::verify_with_tools(&f.root, &packet, &context, &tools()).unwrap()["status"],
        "ready_for_agent_review"
    );
    f.write("var/projects/p/books-packet.json", &packet);
    let feedback = feedback::append_with_tools(
        &f.root,
        &json!({
            "packet_path":"var/projects/p/books-packet.json", "context":context,
            "candidate_id":packet["evidence_candidates"][0]["id"], "disposition":"deferred",
            "reason":"Fixture requires actual engineering conditions", "next_check":"Measure load",
            "output":"var/projects/p/feedback/books.json"
        }),
        &tools(),
    )
    .unwrap();
    assert_eq!(feedback["semantic_review"], "not_run");
    // Equal bytes outside the source library must not acquire source authority.
    fs::write(f.path.join("var/projects/p/outside.pdf"), &bytes).unwrap();
    for denied in [
        "var/projects/p/outside.pdf",
        "sources/books/../../var/projects/p/outside.pdf",
    ] {
        let mut bad_report = report.clone();
        bad_report["results"][0]["source_pdf"] = json!(denied);
        let error = packet::make_with_tools(&f.root, &bad_report, &context, &tools()).unwrap_err();
        assert!(
            error.to_string().contains("source_outside_root"),
            "{error:#}"
        );
        let mut bad_packet = packet.clone();
        bad_packet["evidence_candidates"][0]["source"]["source_pdf"] = json!(denied);
        let error =
            packet::verify_with_tools(&f.root, &bad_packet, &context, &tools()).unwrap_err();
        assert!(
            error.to_string().contains("source_outside_root"),
            "{error:#}"
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            f.path.join("var/projects/p/outside.pdf"),
            f.path.join("sources/books/linked.pdf"),
        )
        .unwrap();
        let mut bad_report = report.clone();
        bad_report["results"][0]["source_pdf"] = json!("sources/books/linked.pdf");
        let error = packet::make_with_tools(&f.root, &bad_report, &context, &tools()).unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error:#}");
        let mut bad_packet = packet.clone();
        bad_packet["evidence_candidates"][0]["source"]["source_pdf"] =
            json!("sources/books/linked.pdf");
        let error =
            packet::verify_with_tools(&f.root, &bad_packet, &context, &tools()).unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error:#}");
    }
}

// A source can remain physically verifiable after its current library entry has
// changed. These cases exercise the public query/packet path across real native
// registration updates, keeping the old PDF and packet as historical evidence.
fn source_currentness_fixture() -> (Fixture, Value, Value, Value, Value) {
    let f = Fixture::new(false);
    let bytes = pdf(false, 3);
    fs::write(f.path.join("sources/books/条件-original.pdf"), &bytes).unwrap();
    let manifest = json!({
        "schema":"ontology-engineering.local-pdf-manifest/v1",
        "sources":[{"id":"conditions","title":"Synthetic book","edition":"fixture-1",
            "path":"条件-original.pdf","sha256":sha256(&bytes),
            "chapters":[{"id":"conditions","title":"工程条件","start_page":1,"end_page":3,"aliases":[]}]}]
    });
    apply_currentness_manifest(&f, &manifest, false, false);
    let mut options = f.options("source-currentness-query");
    options.providers.retain(|p| p.kind == "local_pdf");
    options.top = 1;
    let report = query::run(&f.root, &options).unwrap();
    assert_eq!(report["status"], "completed", "{report:#}");
    assert_eq!(report["results"].as_array().unwrap().len(), 1);
    let context = report["context_snapshot"].clone();
    let packet = packet::make_with_tools(&f.root, &report, &context, &tools()).unwrap();
    f.write("var/projects/p/source-currentness-packet.json", &packet);
    (f, manifest, report, packet, context)
}

fn apply_currentness_manifest(f: &Fixture, manifest: &Value, merge: bool, replace: bool) -> Value {
    f.write("sources/books/currentness-manifest.json", manifest);
    registration::register(
        &f.path,
        &f.path.join("sources/books/currentness-manifest.json"),
        &RegistrationOptions {
            tools: tools(),
            merge,
            replace,
            ..Default::default()
        },
    )
    .unwrap()
}

fn currentness_second_book(f: &Fixture, manifest: &Value) -> Value {
    let bytes = pdf(true, 1);
    fs::write(f.path.join("sources/books/unrelated.pdf"), &bytes).unwrap();
    let mut source = manifest["sources"][0].clone();
    source["id"] = json!("unrelated");
    source["title"] = json!("Independent second book");
    source["path"] = json!("unrelated.pdf");
    source["sha256"] = json!(sha256(&bytes));
    source["chapters"][0]["end_page"] = json!(1);
    json!({"schema":manifest["schema"],"sources":[source]})
}

fn verify_currentness(f: &Fixture, packet: &Value, context: &Value, status: &str) -> Value {
    let checked = packet::verify_with_tools(&f.root, packet, context, &tools()).unwrap();
    assert_eq!(checked["status"], "ready_for_agent_review", "{checked:#}");
    let freshness = &checked["source_registration_freshness"];
    assert_eq!(freshness["status"], status, "{checked:#}");
    assert_eq!(freshness["candidates"].as_array().unwrap().len(), 1);
    assert_eq!(freshness["candidates"][0]["status"], status, "{checked:#}");
    checked
}

#[test]
fn local_source_currentness_survives_unrelated_incremental_addition() {
    let (f, manifest, report, packet, context) = source_currentness_fixture();
    let binding = &report["results"][0]["source_registration_binding"];
    assert_eq!(
        binding["schema"],
        "ontology-engineering.local-pdf-source-binding/v1"
    );
    assert_eq!(binding["index_directory"], "sources/.indexes/local-pdf");
    assert_eq!(binding["source"]["id"], "conditions");
    assert_eq!(binding["source"]["path"], "sources/books/条件-original.pdf");
    assert_eq!(
        binding["source"]["chapters_sha256"],
        canonical_digest(&manifest["sources"][0]["chapters"]).unwrap()
    );
    let mut unsigned = binding.clone();
    unsigned.as_object_mut().unwrap().remove("sha256");
    assert_eq!(binding["sha256"], canonical_digest(&unsigned).unwrap());
    assert_eq!(
        packet["evidence_candidates"][0]["source"]["source_registration_binding"],
        *binding
    );
    verify_currentness(&f, &packet, &context, "current");
    let index = f.path.join("sources/.indexes/local-pdf/catalog.sqlite3");
    let before = sha256(fs::read(&index).unwrap());
    let second = currentness_second_book(&f, &manifest);
    let result = apply_currentness_manifest(&f, &second, true, false);
    assert_eq!(result["update"]["retained"], json!(["conditions"]));
    assert_ne!(before, sha256(fs::read(&index).unwrap()));
    let checked = verify_currentness(&f, &packet, &context, "current");
    assert_eq!(
        checked["source_registration_freshness"]["candidates"][0]["changed_fields"],
        json!([])
    );
}

#[test]
fn local_source_currentness_distinguishes_metadata_and_navigation_changes() {
    for field in ["title", "edition", "chapters_sha256"] {
        let (f, mut manifest, _, packet, context) = source_currentness_fixture();
        let before = fs::read(f.path.join("sources/books/条件-original.pdf")).unwrap();
        match field {
            "title" => manifest["sources"][0]["title"] = json!("Corrected synthetic title"),
            "edition" => manifest["sources"][0]["edition"] = json!("corrected-fixture-edition"),
            _ => manifest["sources"][0]["chapters"][0]["aliases"] = json!(["corrected navigation"]),
        }
        f.write("sources/books/currentness-manifest.json", &manifest);
        let result = registration::register(
            &f.path,
            &f.path.join("sources/books/currentness-manifest.json"),
            &RegistrationOptions {
                merge: true,
                tools: PopplerTools {
                    pdfinfo: f.path.join("absent-pdfinfo"),
                    pdftotext: f.path.join("absent-pdftotext"),
                    pdftoppm: f.path.join("absent-pdftoppm"),
                },
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result["update"]["updated"], json!(["conditions"]));
        assert_eq!(result["update"]["extracted_source_ids"], json!([]));
        assert_eq!(result["update"]["reused_source_ids"], json!(["conditions"]));
        assert_eq!(
            before,
            fs::read(f.path.join("sources/books/条件-original.pdf")).unwrap()
        );
        let checked = verify_currentness(&f, &packet, &context, "changed");
        assert_eq!(
            checked["source_registration_freshness"]["candidates"][0]["changed_fields"],
            json!([field]),
            "{checked:#}"
        );
        let mut options = f.options("updated-description-query");
        options.providers.retain(|p| p.kind == "local_pdf");
        options.top = 1;
        let report = query::run(&f.root, &options).unwrap();
        let new_packet = packet::make_with_tools(&f.root, &report, &context, &tools()).unwrap();
        verify_currentness(&f, &new_packet, &context, "current");
    }
}

#[test]
#[cfg(unix)]
fn local_source_unicode_space_colon_paths_survive_query_and_metadata_reuse() {
    let f = Fixture::new(false);
    let source_relative = "sources/books/资料 空格/𠮷　条件:第一版.pdf";
    let index_relative = "sources/.indexes/自定义 索引:𠮷";
    let original = f.path.join(source_relative);
    fs::create_dir_all(original.parent().unwrap()).unwrap();
    let bytes = pdf(false, 3);
    fs::write(&original, &bytes).unwrap();
    let mut manifest = json!({
        "schema":"ontology-engineering.local-pdf-manifest/v1",
        "sources":[{"id":"conditions","title":"Synthetic Unicode path book","edition":"fixture-1",
            "path":"资料 空格/𠮷　条件:第一版.pdf","sha256":sha256(&bytes),
            "chapters":[{"id":"conditions","title":"工程条件","start_page":1,"end_page":3,"aliases":[]}]}]
    });
    let manifest_relative = "sources/books/路径 清单.json";
    f.write(manifest_relative, &manifest);
    let mut registration_options = RegistrationOptions {
        data_root: index_relative.into(),
        tools: tools(),
        ..Default::default()
    };
    registration::register(
        &f.path,
        &f.path.join(manifest_relative),
        &registration_options,
    )
    .unwrap();
    let mut query_options = f.options("unicode-path-query-before");
    query_options.providers = vec![ProviderSpec {
        kind: "local_pdf".into(),
        index: Some(index_relative.into()),
    }];
    query_options.top = 1;
    let report = query::run(&f.root, &query_options).unwrap();
    assert_eq!(report["results"].as_array().unwrap().len(), 1);
    let binding = &report["results"][0]["source_registration_binding"];
    assert_eq!(binding["source"]["path"], source_relative);
    assert_eq!(binding["index_directory"], index_relative);
    let context = report["context_snapshot"].clone();
    let packet = packet::make_with_tools(&f.root, &report, &context, &tools()).unwrap();
    verify_currentness(&f, &packet, &context, "current");
    f.write("var/projects/p/unicode-path-packet.json", &packet);
    let packet_bytes = fs::read(f.path.join("var/projects/p/unicode-path-packet.json")).unwrap();
    // Absolute manifest spelling resolves to the same original. Descriptive
    // changes must continue to reuse text across the admitted Unicode paths.
    manifest["sources"][0]["path"] = json!(original);
    manifest["sources"][0]["title"] = json!("Corrected title");
    manifest["sources"][0]["edition"] = json!("corrected-description");
    manifest["sources"][0]["chapters"][0]["aliases"] = json!(["corrected navigation"]);
    f.write(manifest_relative, &manifest);
    registration_options.merge = true;
    registration_options.tools = PopplerTools {
        pdfinfo: f.path.join("absent-pdfinfo"),
        pdftotext: f.path.join("absent-pdftotext"),
        pdftoppm: f.path.join("absent-pdftoppm"),
    };
    let updated = registration::register(
        &f.path,
        &f.path.join(manifest_relative),
        &registration_options,
    )
    .unwrap();
    assert_eq!(updated["update"]["updated"], json!(["conditions"]));
    assert_eq!(updated["update"]["extracted_source_ids"], json!([]));
    assert_eq!(
        updated["update"]["reused_source_ids"],
        json!(["conditions"])
    );
    verify_currentness(&f, &packet, &context, "changed");
    query_options.output = "var/projects/p/unicode-path-query-after".into();
    let report = query::run(&f.root, &query_options).unwrap();
    let fresh = packet::make_with_tools(&f.root, &report, &context, &tools()).unwrap();
    verify_currentness(&f, &fresh, &context, "current");
    let index = f.path.join(index_relative);
    let snapshot = || {
        (
            fs::read(index.join("catalog.sqlite3")).unwrap(),
            fs::read(index.join("registration.json")).unwrap(),
        )
    };
    let before = snapshot();
    let noop = registration::register(
        &f.path,
        &f.path.join(manifest_relative),
        &registration_options,
    )
    .unwrap();
    assert_eq!(noop["status"], "unchanged");
    assert_eq!(snapshot(), before);
    assert_eq!(fs::read(&original).unwrap(), bytes);
    assert_eq!(
        fs::read(f.path.join("var/projects/p/unicode-path-packet.json")).unwrap(),
        packet_bytes
    );
}

#[test]
fn local_source_currentness_preserves_old_version_and_historical_feedback() {
    let (f, mut manifest, _, packet, context) = source_currentness_fixture();
    let packet_path = f.path.join("var/projects/p/source-currentness-packet.json");
    let old_packet = fs::read(&packet_path).unwrap();
    let old_original = fs::read(f.path.join("sources/books/条件-original.pdf")).unwrap();
    let record = feedback::append_with_tools(
        &f.root,
        &json!({
            "packet_path":"var/projects/p/source-currentness-packet.json","context":context,
            "candidate_id":packet["evidence_candidates"][0]["id"],"disposition":"deferred",
            "reason":"Synthetic source still requires engineering verification",
            "next_check":"Check actual conditions before adoption",
            "output":"var/projects/p/feedback/before-source-update.json"
        }),
        &tools(),
    )
    .unwrap();
    let feedback_path = f
        .path
        .join("var/projects/p/feedback/before-source-update.json");
    let old_feedback = fs::read(&feedback_path).unwrap();
    let bytes = pdf(true, 2);
    fs::write(f.path.join("sources/books/条件-revision-2.pdf"), &bytes).unwrap();
    manifest["sources"][0]["path"] = json!("条件-revision-2.pdf");
    manifest["sources"][0]["edition"] = json!("fixture-2");
    manifest["sources"][0]["sha256"] = json!(sha256(&bytes));
    manifest["sources"][0]["chapters"][0]["end_page"] = json!(2);
    apply_currentness_manifest(&f, &manifest, true, false);
    let checked = verify_currentness(&f, &packet, &context, "changed");
    let changes = checked["source_registration_freshness"]["candidates"][0]["changed_fields"]
        .as_array()
        .unwrap();
    for field in [
        "path",
        "edition",
        "sha256",
        "bytes",
        "page_count",
        "chapters_sha256",
    ] {
        assert!(changes.contains(&json!(field)), "{checked:#}");
    }
    let historical = feedback::load_reference_with_tools(&f.root, &json!({
        "path":"var/projects/p/feedback/before-source-update.json","sha256":sha256(&old_feedback)
    }), &context, &tools()).unwrap();
    assert_eq!(historical["status"], "history_verified");
    assert_eq!(historical["model_observation"]["historical_only"], true);
    assert_eq!(
        historical["model_observation"]["disposition"],
        record["disposition"]
    );
    assert_eq!(fs::read(&packet_path).unwrap(), old_packet);
    assert_eq!(fs::read(&feedback_path).unwrap(), old_feedback);
    assert_eq!(
        fs::read(f.path.join("sources/books/条件-original.pdf")).unwrap(),
        old_original
    );
}

#[test]
fn local_source_currentness_distinguishes_removed_from_unavailable_index() {
    let (f, manifest, _, packet, context) = source_currentness_fixture();
    let second = currentness_second_book(&f, &manifest);
    apply_currentness_manifest(&f, &second, false, true);
    verify_currentness(&f, &packet, &context, "removed");
    assert!(f.path.join("sources/books/条件-original.pdf").is_file());
    fs::rename(
        f.path.join("sources/.indexes/local-pdf"),
        f.path.join("var/projects/p/preserved-index"),
    )
    .unwrap();
    verify_currentness(&f, &packet, &context, "unavailable");
}

#[test]
fn local_source_currentness_legacy_packet_is_not_bound() {
    let (f, _, _, mut packet, context) = source_currentness_fixture();
    packet["evidence_candidates"][0]["source"]
        .as_object_mut()
        .unwrap()
        .remove("source_registration_binding")
        .expect("new query must emit binding");
    verify_currentness(&f, &packet, &context, "not_bound");
}

#[test]
fn local_source_currentness_rejects_corrupt_binding_and_candidate_identity() {
    let (f, _, _, packet, context) = source_currentness_fixture();
    for fault in 0..5 {
        let mut bad = packet.clone();
        let binding = &mut bad["evidence_candidates"][0]["source"]["source_registration_binding"];
        match fault {
            0 => binding["sha256"] = json!("0".repeat(64)),
            1 => binding["source"]["id"] = json!("another-source"),
            2 => binding["source"]["sha256"] = json!("0".repeat(64)),
            3 => binding["source"]["path"] = json!("sources/books/another-original.pdf"),
            _ => binding["schema"] = json!("unknown-binding-schema"),
        }
        if fault != 0 {
            let mut unsigned = binding.clone();
            unsigned.as_object_mut().unwrap().remove("sha256");
            binding["sha256"] = json!(canonical_digest(&unsigned).unwrap());
        }
        assert!(
            packet::verify_with_tools(&f.root, &bad, &context, &tools()).is_err(),
            "corrupt binding fault {fault} must fail"
        );
    }
}
