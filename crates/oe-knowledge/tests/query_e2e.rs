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
