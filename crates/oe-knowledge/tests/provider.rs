use oe_core::{
    SkillRoot, fingerprint_file,
    json::{canonical_digest, canonical_json},
    sha256,
};
use oe_knowledge::{
    sources::{Provider, RetrievalRequest},
    text,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const TABLES: &str = "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE chapters(id TEXT PRIMARY KEY,catalog TEXT,title TEXT,start_page INTEGER,end_page INTEGER,source_json TEXT);
CREATE TABLE pages(id TEXT PRIMARY KEY,catalog TEXT,page_id INTEGER,pdf_page INTEGER,printed_page TEXT,chapter_id TEXT,source_relative_path TEXT,source_url TEXT,source_sha256 TEXT,source_bytes INTEGER,body TEXT,text_sha256 TEXT,raw_text_sha256 TEXT,raw_text_chars INTEGER,text_chars INTEGER,native_text_sparse INTEGER,titles_json TEXT,series_json TEXT,series_count INTEGER,source_json TEXT);
CREATE INDEX page_chapter ON pages(chapter_id,pdf_page);
CREATE INDEX page_position ON pages(catalog,pdf_page);
CREATE VIRTUAL TABLE search USING fts5(models,titles,body,chapter);";

struct Fixture {
    path: PathBuf,
    root: SkillRoot,
    index: PathBuf,
    registration: Value,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../var/maintenance/rust-rewrite-20260927/phase5-provider/fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        let path = fs::canonicalize(path).unwrap();
        fs::write(path.join("SKILL.md"), "fixture").unwrap();
        fs::create_dir_all(path.join("sources/books")).unwrap();
        let index = path.join("sources/.indexes/local-pdf");
        fs::create_dir_all(&index).unwrap();
        let db = Connection::open(index.join("catalog.sqlite3")).unwrap();
        db.execute_batch(TABLES).unwrap();
        let mut sources = Vec::new();
        for id in ["a", "b"] {
            let raw = format!("synthetic pdf bytes {id}");
            fs::write(path.join(format!("sources/books/{id}.pdf")), &raw).unwrap();
            let chapter = json!({"id":"shaft","title":"传动轴","start_page":1,"end_page":2,"aliases":["shaft"]});
            sources.push(json!({"id":id,"title":format!("Book {id}"),"edition":"fixture","path":format!("../../books/{id}.pdf"),"sha256":sha256(&raw),"bytes":raw.len(),"page_count":2,"chapters":[chapter]}));
            let code = format!("local_pdf:{id}");
            let cid = format!("{code}:chapter:shaft");
            db.execute(
                "INSERT INTO chapters VALUES(?,?,?,?,?,?)",
                params![
                    cid,
                    code,
                    "传动轴",
                    1,
                    2,
                    serde_json::to_string(&chapter).unwrap()
                ],
            )
            .unwrap();
            for (n, body) in [
                (1, "😀传动轴😀 shaft explanation"),
                (2, if id == "a" { "Another shaft page" } else { "" }),
            ] {
                let pid = format!("{code}:page:{n}");
                let titles =
                    json!([{"text":"传动轴","origin":"declared_chapter_navigation"}]).to_string();
                db.execute(
                    "INSERT INTO pages VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                    params![
                        pid,
                        code,
                        n,
                        n,
                        Option::<String>::None,
                        cid,
                        "",
                        "",
                        sha256(&raw),
                        raw.len() as i64,
                        body,
                        sha256(body),
                        sha256(body),
                        body.chars().count() as i64,
                        body.chars().count() as i64,
                        1,
                        titles,
                        "[]",
                        0,
                        json!({"document_id":id,"text_origin":"poppler_native_text"}).to_string()
                    ],
                )
                .unwrap();
                let rowid = db.last_insert_rowid();
                db.execute(
                    "INSERT INTO search(rowid,models,titles,body,chapter) VALUES(?,?,?,?,?)",
                    params![
                        rowid,
                        "",
                        "传动 动轴 传动轴 shaft",
                        text::tokens(body).join(" "),
                        "传动 动轴 传动轴"
                    ],
                )
                .unwrap();
            }
        }
        let registration =
            json!({"schema":"ontology-engineering.local-pdf-registration/v1","sources":sources});
        let mut meta = json!({"schema":"ontology-engineering.local-pdf-index/v1","counts":{"catalogs":2,"pages":4,"chapters":2},"registration_sha256":canonical_digest(&registration).unwrap(),"extraction":"fixture"});
        meta["fingerprint"] = json!(canonical_digest(&meta).unwrap());
        for (key, value) in meta.as_object().unwrap() {
            db.execute(
                "INSERT INTO meta VALUES(?,?)",
                params![key, value.to_string()],
            )
            .unwrap();
        }
        drop(db);
        let root = SkillRoot::new(&path).unwrap();
        let mut result = Self {
            path,
            root,
            index,
            registration,
        };
        result.bind();
        result
    }
    fn bind(&mut self) {
        self.registration["index_sha256"] = json!(
            fingerprint_file(&self.index.join("catalog.sqlite3"), 2 * 1024 * 1024 * 1024)
                .unwrap()
                .sha256
        );
        fs::write(
            self.index.join("registration.json"),
            canonical_json(&self.registration).unwrap(),
        )
        .unwrap();
    }
    fn edit(&mut self, sql: &str) {
        let db = Connection::open(self.index.join("catalog.sqlite3")).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        self.bind();
    }
    fn open(&self) -> anyhow::Result<Provider> {
        Provider::open(&self.root, "local_pdf", None)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}

#[test]
fn unicode_tokens_match_existing_order_prefixes_and_hyphens() {
    assert_eq!(text::normalize("ＡＢＣ−１２ Ⅰ ㎜"), "abc-12 i mm");
    assert_eq!(
        text::tokens("ＳＦＵ－１６０５ 2.5mm 传动轴 中 😀𠀀"),
        vec!["sfu-1605", "sfu", "2.5mm", "传动", "动轴", "传动轴", "中"]
    );
    assert_eq!(
        text::tokens("a/b_2 a- x12 123 12.ab 123abc12"),
        vec!["a/b_2", "a", "x12", "ab", "123abc"]
    );
    assert_eq!(
        text::match_expression("传动轴 传动轴 ＳＦＵ－１６０５"),
        "\"sfu-1605\" OR \"sfu\" OR \"传动\" OR \"动轴\" OR \"传动轴\""
    );
    let many = (0..200)
        .map(|n| format!("word{n}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(text::match_expression(&many).split(" OR ").count(), 160);
}

#[test]
fn excerpt_and_character_spans_keep_non_bmp_and_original_bytes() {
    let body = "😀甲乙😀目标段𠀀尾";
    assert_eq!(text::char_slice(body, 4, 8).unwrap(), "目标段𠀀");
    assert!(text::char_slice(body, 1, 100).is_err());
    assert_eq!(
        text::excerpt(body, "目标", 4),
        json!({"text":"乙😀目标","start":2,"end":6})
    );
    let raw = "ＡＢＣ\t😀\n";
    assert_eq!(
        text::excerpt(raw, "abc", 700),
        json!({"text":raw,"start":0,"end":6})
    );
    assert_eq!(text::excerpt("unchanged", "unmatched", 4)["start"], 0);
}

#[test]
fn existing_schema_chapters_page_identity_and_source_json_are_preserved() {
    let fixture = Fixture::new();
    let before = fingerprint_file(&fixture.index.join("catalog.sqlite3"), u64::MAX).unwrap();
    let provider = fixture.open().unwrap();
    let chapters = provider.chapters().unwrap();
    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0]["aliases"], json!(["shaft"]));
    let page = provider.page("local_pdf:a:page:1").unwrap();
    assert_eq!(page["body"], "😀传动轴😀 shaft explanation");
    assert!(page["source_json"].is_string());
    assert_eq!(page["source_locator"]["physical_page"], 1);
    assert_eq!(page["source_file_page"], 1);
    assert_eq!(page["original_verified"], false);
    assert_eq!(
        page["source_pdf"],
        json!(fixture.path.join("sources/books/a.pdf"))
    );
    assert_eq!(provider.status()["status"], "ready");
    assert_eq!(
        fingerprint_file(&fixture.index.join("catalog.sqlite3"), u64::MAX).unwrap(),
        before
    );
    assert_eq!(fs::read_dir(&fixture.index).unwrap().count(), 2);
}

#[test]
fn retrieval_is_parameterized_bounded_fair_and_reserves_routed_empty_text() {
    let fixture = Fixture::new();
    let provider = fixture.open().unwrap();
    let rows = provider.retrieve("shaft", 2, &[]).unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0]["catalog"], rows[1]["catalog"]);
    let rows = provider
        .retrieve("absent", 1, &["local_pdf:b:chapter:shaft".to_owned()])
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["catalog"], "local_pdf:b");
    let blank = provider.page("local_pdf:b:page:2").unwrap();
    assert_eq!(blank["navigation_only"], true);
    assert_eq!(blank["native_text_missing"], true);
    assert!(provider.retrieve("shaft", 0, &[]).is_err());
    assert!(provider.retrieve("shaft", 97, &[]).is_err());
    assert!(provider.retrieve("shaft", 1, &["missing".into()]).is_err());
    assert!(
        provider
            .retrieve("\" OR * NEAR( ); DROP TABLE pages; --", 4, &[])
            .is_ok()
    );
    assert!(provider.page("x' OR 1=1 --").is_err());
}

#[test]
fn body_pools_exclude_title_only_hits_require_both_inputs_and_keep_book_fairness() {
    let fixture = Fixture::new();
    let provider = fixture.open().unwrap();
    let rows = provider
        .retrieve_body("shaft", "explanation", 2, &[])
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0]["catalog"], rows[1]["catalog"]);
    let rows = provider
        .retrieve_body("", "shaft", 4, &["local_pdf:b:chapter:shaft".into()])
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "local_pdf:b:page:1");
    assert!(
        provider
            .retrieve_body("shaft", "absent", 4, &[])
            .unwrap()
            .is_empty()
    );
    assert!(provider.retrieve_body("", "", 4, &[]).is_err());
    assert!(provider.retrieve_body("shaft", "", 0, &[]).is_err());
    assert!(
        provider
            .retrieve_body("shaft", "", 4, &["not-present".into()])
            .is_err()
    );
    assert!(
        provider
            .retrieve("shaft", 4, &[])
            .unwrap()
            .iter()
            .any(|r| r["id"] == "local_pdf:b:page:2")
    );
    assert!(
        provider
            .retrieve_body("shaft", "", 4, &[])
            .unwrap()
            .iter()
            .all(|r| r["id"] != "local_pdf:b:page:2")
    );
}

#[test]
fn retrieval_plan_matches_single_pools_and_rejects_limits_and_preexisting_drift() {
    let fixture = Fixture::new();
    let provider = fixture.open().unwrap();
    let chapter = vec!["local_pdf:b:chapter:shaft".to_owned()];
    let requests = vec![
        RetrievalRequest::Lexical {
            query: "shaft".into(),
            chapters: vec![],
            limit: 3,
        },
        RetrievalRequest::Body {
            anchor: "shaft".into(),
            terms: "explanation".into(),
            chapters: chapter.clone(),
            limit: 2,
        },
    ];
    let expected = vec![
        provider.retrieve("shaft", 3, &[]).unwrap(),
        provider
            .retrieve_body("shaft", "explanation", 2, &chapter)
            .unwrap(),
    ];
    assert_eq!(provider.retrieve_plan(&requests).unwrap(), expected);
    assert!(
        provider
            .retrieve_plan(&vec![requests[0].clone(); 33])
            .is_err()
    );
    fs::write(
        fixture.index.join("registration.json"),
        "changed before batch",
    )
    .unwrap();
    assert!(provider.retrieve_plan(&requests).is_err());
}

#[test]
fn retrieval_plan_bounds_aggregate_returned_text_across_pools() {
    let mut fixture = Fixture::new();
    let body = "x".repeat(1200 * 1024);
    let db = Connection::open(fixture.index.join("catalog.sqlite3")).unwrap();
    db.execute(
        "UPDATE pages SET body=?,text_chars=?,text_sha256=? WHERE catalog='local_pdf:a'",
        params![body, body.len() as i64, sha256(&body)],
    )
    .unwrap();
    drop(db);
    fixture.bind();
    let provider = fixture.open().unwrap();
    let request = RetrievalRequest::Lexical {
        query: "shaft".into(),
        chapters: vec![],
        limit: 1,
    };
    assert!(
        provider
            .retrieve_plan(std::slice::from_ref(&request))
            .is_ok()
    );
    assert!(
        provider
            .retrieve_plan(&vec![request; 32])
            .expect_err("expected cumulative plan byte limit")
            .to_string()
            .contains("candidate_text_bytes_limit")
    );
}

fn supplier_fixture() -> (Fixture, PathBuf) {
    let fixture = Fixture::new();
    let directory = fixture.path.join("sources/.indexes/misumi");
    fs::create_dir(&directory).unwrap();
    fs::copy(
        fixture.index.join("catalog.sqlite3"),
        directory.join("catalog.sqlite3"),
    )
    .unwrap();
    let db = Connection::open(directory.join("catalog.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE catalogs(code TEXT PRIMARY KEY,label TEXT,title TEXT,page_count INTEGER,pdf_relative_path TEXT,pdf_sha256 TEXT,metadata_sha256 TEXT,manifest_sha256 TEXT);
        CREATE TABLE hotspots(page_id TEXT,ordinal INTEGER,kind TEXT,keyword TEXT,keyword_normalized TEXT,target TEXT,source_json TEXT,PRIMARY KEY(page_id,ordinal));
        CREATE INDEX hotspot_model ON hotspots(keyword_normalized,kind);
        UPDATE pages SET source_relative_path='tmp/pdfs/fixture/pages/page.pdf',printed_page='I',source_url='https://example.invalid/observed.pdf';").unwrap();
    for code in ["local_pdf:a", "local_pdf:b"] {
        db.execute(
            "INSERT INTO catalogs VALUES(?,?,?,?,?,?,?,?)",
            params![
                code,
                "Supplier fixture",
                "Original supplier catalog",
                2,
                "tmp/pdfs/fixture/assembly.pdf",
                "a".repeat(64),
                "b".repeat(64),
                "c".repeat(64)
            ],
        )
        .unwrap();
    }
    db.execute("DELETE FROM meta", []).unwrap();
    let mut meta = json!({"schema":"misumi.catalog-index/v1","version":"0.1.0","counts":{"pages":4,"chapters":2,"catalogs":2}});
    meta["fingerprint"] = json!(canonical_digest(&meta).unwrap());
    for (k, v) in meta.as_object().unwrap() {
        db.execute("INSERT INTO meta VALUES(?,?)", params![k, v.to_string()])
            .unwrap();
    }
    drop(db);
    let identity = fingerprint_file(&directory.join("catalog.sqlite3"), u64::MAX).unwrap();
    let pack = json!({"schema":"ontology-engineering.misumi-source-pack/v1","index":{"path":"catalog.sqlite3","sha256":identity.sha256,"bytes":identity.bytes}});
    fs::write(directory.join("source-pack.json"), pack.to_string()).unwrap();
    (fixture, directory)
}

#[test]
fn misumi_existing_inventory_keeps_raw_source_and_catalog_locators() {
    let (fixture, directory) = supplier_fixture();
    let provider = Provider::open(&fixture.root, "misumi", None).unwrap();
    let page = provider.page("local_pdf:a:page:2").unwrap();
    assert_eq!(page["provider"], "misumi");
    assert_eq!(page["source_file_page"], 1);
    assert_eq!(page["pdf_page"], 2);
    assert_eq!(page["printed_page"], "I");
    assert_eq!(
        page["catalog_registration"]["pdf_relative_path"],
        "tmp/pdfs/fixture/assembly.pdf"
    );
    assert_eq!(
        page["source_pack_manifest"],
        json!(directory.join("source-pack.json"))
    );
    assert!(page["source_json"].is_string());
    assert_eq!(
        provider.retrieve_body("shaft", "", 2, &[]).unwrap().len(),
        2
    );
    fs::write(directory.join("source-pack.json"), "{}").unwrap();
    assert!(provider.page("local_pdf:a:page:2").is_err());
}

#[test]
fn oversized_sqlite_values_and_changed_private_snapshot_fail_closed() {
    let mut fixture = Fixture::new();
    fixture.edit(
        "UPDATE pages SET body=CAST(zeroblob(9*1024*1024) AS TEXT) WHERE id='local_pdf:a:page:1'",
    );
    let provider = fixture.open().unwrap();
    assert!(provider.page("local_pdf:a:page:1").is_err());
    drop(provider);
    let fixture = Fixture::new();
    let provider = fixture.open().unwrap();
    let snapshot = fs::read_dir(fixture.path.join("var/cache/knowledge/indexes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("catalog.sqlite3");
    fs::write(snapshot, b"changed private cache").unwrap();
    assert!(provider.page("local_pdf:a:page:1").is_err());
}

#[test]
fn stale_database_registration_and_transaction_sidecars_are_rejected() {
    for what in ["database", "registration", "sidecar"] {
        let fixture = Fixture::new();
        let provider = fixture.open().unwrap();
        match what {
            "database" => {
                let db = Connection::open(fixture.index.join("catalog.sqlite3")).unwrap();
                db.execute(
                    "UPDATE pages SET body='changed' WHERE id='local_pdf:a:page:1'",
                    [],
                )
                .unwrap();
            }
            "registration" => {
                let mut doc = fixture.registration.clone();
                doc["sources"][0]["title"] = json!("changed");
                fs::write(fixture.index.join("registration.json"), doc.to_string()).unwrap();
            }
            _ => fs::write(fixture.index.join("catalog.sqlite3-wal"), "pending").unwrap(),
        }
        assert!(provider.chapters().is_err());
        assert!(provider.retrieve("shaft", 1, &[]).is_err());
        assert!(provider.page("local_pdf:a:page:1").is_err());
        assert_eq!(provider.status()["status"], "not_ready");
    }
}

#[test]
fn forged_page_and_fake_schema_are_rejected() {
    let fixture = Fixture::new();
    let provider = fixture.open().unwrap();
    let mut page = provider.page("local_pdf:a:page:1").unwrap();
    page["body"] = json!("forged");
    assert!(
        provider
            .source_result(&page, "shaft", &Default::default())
            .unwrap_err()
            .to_string()
            .contains("candidate_identity")
    );
    drop(provider);
    for sql in [
        "CREATE VIEW evil AS SELECT * FROM pages",
        "CREATE TRIGGER evil AFTER INSERT ON pages BEGIN SELECT 1; END",
        "DROP TABLE search; CREATE TABLE search(models,titles,body,chapter)",
        "ALTER TABLE pages ADD COLUMN unauthorized TEXT",
    ] {
        let mut fixture = Fixture::new();
        fixture.edit(sql);
        assert!(fixture.open().is_err(), "accepted {sql}");
    }
}

#[test]
fn corrupt_text_hash_is_not_repaired_or_returned_as_evidence() {
    let mut fixture = Fixture::new();
    fixture.edit(
        "UPDATE pages SET body='changed but no matching digest' WHERE id='local_pdf:a:page:1'",
    );
    let provider = fixture.open().unwrap();
    assert!(
        provider
            .page("local_pdf:a:page:1")
            .unwrap_err()
            .to_string()
            .contains("text_identity")
    );
}

#[test]
fn registration_path_must_stay_in_sources_and_cannot_follow_links() {
    let fixture = Fixture::new();
    assert!(Provider::open(&fixture.root, "local_pdf", Some(Path::new("var/index"))).is_err());
    assert!(
        Provider::open(
            &fixture.root,
            "local_pdf",
            Some(Path::new("sources/.indexes/../../../var/index"))
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            fixture.path.join("sources/books/a.pdf"),
            fixture.path.join("sources/books/link.pdf"),
        )
        .unwrap();
        let mut registration = fixture.registration.clone();
        registration["sources"][0]["path"] = json!("../../books/link.pdf");
        let mut base = registration.clone();
        base.as_object_mut().unwrap().remove("index_sha256");
        let db = Connection::open(fixture.index.join("catalog.sqlite3")).unwrap();
        let mut meta = serde_json::Map::new();
        let mut stmt = db.prepare("SELECT key,value FROM meta").unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap();
        for r in rows {
            let (k, v) = r.unwrap();
            meta.insert(k, serde_json::from_str(&v).unwrap());
        }
        drop(stmt);
        meta.insert(
            "registration_sha256".into(),
            json!(canonical_digest(&base).unwrap()),
        );
        meta.remove("fingerprint");
        let h = canonical_digest(&Value::Object(meta.clone())).unwrap();
        meta.insert("fingerprint".into(), json!(h));
        for (k, v) in meta {
            db.execute(
                "UPDATE meta SET value=? WHERE key=?",
                params![v.to_string(), k],
            )
            .unwrap();
        }
        drop(db);
        registration["index_sha256"] = json!(
            fingerprint_file(&fixture.index.join("catalog.sqlite3"), u64::MAX)
                .unwrap()
                .sha256
        );
        fs::write(
            fixture.index.join("registration.json"),
            registration.to_string(),
        )
        .unwrap();
        assert!(fixture.open().is_err());
    }
}

#[test]
fn index_larger_than_global_json_limit_is_streamed_without_original_side_effects() {
    let mut fixture = Fixture::new();
    // Allocate and free real database pages; the valid database remains >64MiB.
    fixture.edit("INSERT INTO search_data(id,block) VALUES(900000,zeroblob(70*1024*1024)); DELETE FROM search_data WHERE id=900000;");
    let path = fixture.index.join("catalog.sqlite3");
    let identity = fingerprint_file(&path, u64::MAX).unwrap();
    assert!(identity.bytes > 64 * 1024 * 1024);
    let provider = fixture.open().unwrap();
    assert_eq!(provider.status()["status"], "ready");
    assert_eq!(
        provider.page("local_pdf:a:page:1").unwrap()["index_identity"]["sha256"],
        identity.sha256
    );
    assert_eq!(fingerprint_file(&path, u64::MAX).unwrap(), identity);
    assert_eq!(fs::read_dir(&fixture.index).unwrap().count(), 2);
}

#[test]
#[ignore = "bounded, read-only inspection of the operator's existing private source indexes"]
fn actual_registered_indexes_are_compatible_and_unchanged() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let root = SkillRoot::new(path).unwrap();
    for (kind, expected_chapters) in [("local_pdf", 148), ("misumi", 76)] {
        let provider = Provider::open(&root, kind, None).unwrap();
        let chapters = provider.chapters().unwrap();
        assert_eq!(chapters.len(), expected_chapters);
        assert_eq!(provider.status()["status"], "ready");
        let rows = provider
            .retrieve("轴", 1, &[chapters[0]["id"].as_str().unwrap().to_owned()])
            .unwrap();
        assert_eq!(rows.len(), 1);
        let selected = rows[0]["id"].as_str().unwrap();
        assert_eq!(provider.page(selected).unwrap()["id"], selected);
    }
}

#[test]
fn linked_supplier_registration_binds_separate_index_archive_and_extraction_identity() {
    let (fixture, directory) = supplier_fixture();
    let db = Connection::open(directory.join("catalog.sqlite3")).unwrap();
    let mut meta = json!({"schema":"misumi.catalog-index/v1","version":"native-fixture","text_origin":"poppler_native_text","counts":{"pages":4,"chapters":2,"catalogs":2},"source_identity":[["fixture","observed"]]});
    meta["fingerprint"] = json!(canonical_digest(&meta).unwrap());
    db.execute("DELETE FROM meta", []).unwrap();
    for (k, v) in meta.as_object().unwrap() {
        db.execute("INSERT INTO meta VALUES(?,?)", params![k, v.to_string()])
            .unwrap();
    }
    drop(db);
    let linked = fixture.path.join("sources/.indexes/linked");
    fs::create_dir(&linked).unwrap();
    fs::create_dir(fixture.path.join("sources/originals")).unwrap();
    let identity = fingerprint_file(&directory.join("catalog.sqlite3"), u64::MAX).unwrap();
    let mut location = json!({"schema":"ontology-engineering.misumi-source-location/v1","archive_root":"../../originals","index_path":"../misumi/catalog.sqlite3","index_sha256":identity.sha256,"source_catalogs":{"source_identity":meta["source_identity"],"counts":meta["counts"],"index_fingerprint":meta["fingerprint"]}});
    let write = |value: &Value| fs::write(linked.join("source.json"), value.to_string()).unwrap();
    write(&location);
    let provider = Provider::open(&fixture.root, "misumi", Some(&linked)).unwrap();
    let page = provider.page("local_pdf:a:page:2").unwrap();
    assert_eq!(
        page["source_archive_root"],
        json!(fixture.path.join("sources/originals"))
    );
    assert_eq!(page["text_origin"], "poppler_native_text");
    assert!(page.get("source_pack_manifest").is_none());
    location["source_catalogs"]["counts"]["pages"] = json!(5);
    write(&location);
    assert!(provider.page("local_pdf:a:page:2").is_err());
    assert!(Provider::open(&fixture.root, "misumi", Some(&linked)).is_err());
    location["source_catalogs"]["counts"]["pages"] = json!(4);
    location["index_path"] = json!("../../../var/untrusted.sqlite3");
    write(&location);
    assert!(Provider::open(&fixture.root, "misumi", Some(&linked)).is_err());
}
