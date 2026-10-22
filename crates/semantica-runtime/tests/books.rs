use anyhow::{Context, Result};
use semantica_runtime::books::{
    Catalog, MigrationEncoding, NormativeInput, SourceProvider, parse_teaching_cases,
    prepare_normative_engraving,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct Memory(BTreeMap<String, Vec<u8>>);
impl SourceProvider for Memory {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        self.0.get(id).cloned().context("synthetic source absent")
    }
}
fn bytes(value: Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}
fn fixture() -> Memory {
    let contract = bytes(
        json!({"package_id":"synthetic.chapter","package_version":"1.0.0","external_specification":{"source_anchor":"source.md","source_sha256":semantica_runtime::sha256(b"chapter")},"payload_status":{"rdf":"native"},"lifecycle":{"load":{"status":"native"}},"receipt":{"required_fields":["source"],"fields":{"source":"exact"}},"release":{"status":"partial"}}),
    );
    let assets=BTreeMap::from([
        ("chapter/contract.json".into(),contract),
        ("chapter/text.md".into(),"全文😀\n".as_bytes().to_vec()),
        ("domain/tbox.ttl".into(),b"@prefix x: <https://example.test/#> . x:Term a x:Type .\n".to_vec()),
        ("domain/glosses.json".into(),bytes(json!({"5.4.1":{"zh":"公开作者转述。","keywords":["需求"],"book":["ch04"]}}))),
        ("domain/cases.seed".into(),"a-case:\n  ch: ch04\n  teaches: [测试术语, 需求]\n  summary: \"关断\"——历史受限格式的作者内容。\n".as_bytes().to_vec()),
    ]);
    let mut sources = assets.clone();
    let manifests = [
        (
            "chapter/manifest.json",
            "synthetic.chapter",
            "1.0.0",
            vec![
                (
                    "chapter-contract",
                    "chapter_contract",
                    "chapter/contract.json",
                ),
                ("text", "view", "chapter/text.md"),
            ],
        ),
        (
            "domain/manifest.json",
            "synthetic.normative",
            "0.1.0",
            vec![
                ("normative-tbox", "ontology", "domain/tbox.ttl"),
                ("gloss-part3-glosses", "seed", "domain/glosses.json"),
                ("gloss-teaching-cases", "seed", "domain/cases.seed"),
            ],
        ),
    ];
    for (path, id, version, list) in manifests {
        let assets:Vec<_>=list.into_iter().map(|(id,role,path)|json!({"asset_id":id,"role":role,"path":path.split_once('/').unwrap().1,"sha256":semantica_runtime::sha256(&assets[path]),"source_anchor":"source.md","source_sha256":semantica_runtime::sha256(b"chapter"),"format":if role=="ontology"{Some("turtle")}else{None},"load_into_dataset":role=="ontology"})).collect();
        sources.insert(path.into(),bytes(json!({"schema_version":"1.0","package_id":id,"version":version,"namespace":"https://example.test/normative#","book_source":{"source_anchor":"source.md","source_sha256":semantica_runtime::sha256(b"chapter")},"assets":assets})));
    }
    sources.insert("registry.json".into(),bytes(json!({"schema_version":"1.0","policy":"policy.json","expected_package_count":1,"expected_coverage":{"custom-volume":["lesson-a"]},"packages":[{"volume":"custom-volume","chapter":"lesson-a","package_id":"synthetic.chapter","version":"1.0.0","manifest":"chapter/manifest.json"}],"expected_domain_package_count":1,"domain_packages":[{"package_id":"synthetic.normative","version":"0.1.0","manifest":"domain/manifest.json"}]})));
    sources.insert("policy.json".into(),bytes(json!({"contract_count":1,"status_vocabulary":["native","partial","absent"],"required_migration_payload_kinds":["rdf"],"required_lifecycle_stages":["load"],"required_receipt_fields":["source"]})));
    sources.insert("source.md".into(), b"chapter".to_vec());
    sources.insert("part1.json".into(),bytes(json!([[{"content":{"paragraphs":[{"content":"3.1 SECRET-RESTRICTED-STANDARD-TEXT"}]}}]])));
    sources.insert("part3.json".into(),bytes(json!([[{"content":{"paragraphs":[{"content":"5.4.1 SECRET-RESTRICTED-STANDARD-TEXT requirements shall be available"}]}},{"content":{"paragraphs":[{"content":"8.1 SECRET-RESTRICTED-STANDARD-TEXT heading"}]}}]])));
    sources.insert(
        "glossary.md".into(),
        "- **测试术语（test term）**（1-3.1；ch04）——这是**作者**的公开转述。\n"
            .as_bytes()
            .to_vec(),
    );
    Memory(sources)
}
fn input() -> NormativeInput {
    NormativeInput {
        package_id: "synthetic.normative".into(),
        extracts: BTreeMap::from([(1, "part1.json".into()), (3, "part3.json".into())]),
        glossary_source: Some("glossary.md".into()),
        parts: vec![3, 1],
    }
}

fn add_asset(
    source: &mut Memory,
    package_path: &str,
    id: &str,
    role: &str,
    path: &str,
    content: &[u8],
) {
    source.0.insert(path.into(), content.to_vec());
    let mut manifest: Value = serde_json::from_slice(&source.0[package_path]).unwrap();
    manifest["assets"].as_array_mut().unwrap().push(json!({"asset_id":id,"role":role,"path":path.split_once('/').unwrap().1,"sha256":semantica_runtime::sha256(content),"source_anchor":"source.md","source_sha256":semantica_runtime::sha256(b"chapter"),"format":if role=="data"{Some("turtle")}else{None},"kind":if role=="data"{"rdf"}else{"text"},"load_into_dataset":role=="data"}));
    source.0.insert(package_path.into(), bytes(manifest));
}

#[test]
fn full_package_read_load_and_query_share_one_session_with_bindings_and_namespace() {
    use oxigraph::model::{NamedNode, Term};
    let mut source = fixture();
    add_asset(
        &mut source,
        "chapter/manifest.json",
        "data",
        "data",
        "chapter/data.ttl",
        b"<https://example.test/part> <https://example.test/kind> <https://example.test/Type> .",
    );
    add_asset(
        &mut source,
        "chapter/manifest.json",
        "query",
        "sparql",
        "chapter/query.rq",
        b"SELECT ?s WHERE { ?s x:kind ?wanted }",
    );
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = semantica_runtime::dataset::DatasetSession::new();
    let loaded = catalog
        .load_chapter_package(&source, "custom-volume", "lesson-a", &mut session)
        .unwrap();
    assert_eq!(loaded["rdf_added"], 1);
    let revision = session.revision();
    let repeated = catalog
        .load_chapter_package(&source, "custom-volume", "lesson-a", &mut session)
        .unwrap();
    assert_eq!(repeated["rdf_added"], 0);
    assert_eq!(session.revision(), revision);
    let query = catalog
        .execute_chapter_query(
            &source,
            "custom-volume",
            "lesson-a",
            "query",
            &mut session,
            &BTreeMap::from([("x".into(), "https://example.test/".into())]),
            &BTreeMap::from([(
                "wanted".into(),
                Term::NamedNode(NamedNode::new("https://example.test/Type").unwrap()),
            )]),
        )
        .unwrap();
    assert_eq!(query["row_count"], 1);
    assert_eq!(query["rows"][0]["s"]["value"], "https://example.test/part");
    assert_eq!(session.revision(), revision);
    // The existing chapter owner now populates this same session's cache.
    let namespaces = BTreeMap::from([("x".into(), "https://example.test/".into())]);
    let bindings = BTreeMap::from([(
        "wanted".into(),
        Term::NamedNode(NamedNode::new("https://example.test/Type").unwrap()),
    )]);
    let cached = session
        .query_with_options(
            "SELECT ?s WHERE { ?s x:kind ?wanted }",
            &namespaces,
            &bindings,
            true,
        )
        .unwrap();
    assert!(cached.cache_hit);
    assert_eq!(cached.result, query);
    let different = BTreeMap::from([(
        "wanted".into(),
        Term::NamedNode(NamedNode::new("https://example.test/Other").unwrap()),
    )]);
    let absent = catalog
        .execute_chapter_query(
            &source,
            "custom-volume",
            "lesson-a",
            "query",
            &mut session,
            &namespaces,
            &different,
        )
        .unwrap();
    assert_eq!(absent["row_count"], 0);
    assert_eq!(session.revision(), revision);
    assert!(
        catalog
            .execute_chapter_query(
                &source,
                "custom-volume",
                "lesson-a",
                "text",
                &mut session,
                &BTreeMap::new(),
                &BTreeMap::new()
            )
            .is_err()
    );
    // The selected text is still intact. The unrelated asset is corrupted.
    source.0.insert(
        "chapter/data.ttl".into(),
        b"edited unrelated asset".to_vec(),
    );
    // A warm cache cannot bypass the chapter/package source checks.
    assert!(
        catalog
            .execute_chapter_query(
                &source,
                "custom-volume",
                "lesson-a",
                "query",
                &mut session,
                &namespaces,
                &bindings
            )
            .is_err()
    );
    assert!(
        catalog
            .package_asset_text(&source, "synthetic.chapter", "text")
            .is_err()
    );
}

#[test]
fn late_rdf_failure_cannot_mutate_live_session_or_claim_package_identity() {
    let mut source = fixture();
    add_asset(
        &mut source,
        "chapter/manifest.json",
        "good",
        "data",
        "chapter/good.ttl",
        b"<https://example.test/a> <https://example.test/p> <https://example.test/b> .",
    );
    add_asset(
        &mut source,
        "chapter/manifest.json",
        "bad",
        "data",
        "chapter/bad.ttl",
        b"not RDF",
    );
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = semantica_runtime::dataset::DatasetSession::new();
    assert!(
        catalog
            .load_chapter_package(&source, "custom-volume", "lesson-a", &mut session)
            .is_err()
    );
    assert_eq!(session.revision(), 0);
    assert_eq!(session.quad_count(), 0);
    source.0.remove("chapter/bad.ttl");
    let mut manifest: Value = serde_json::from_slice(&source.0["chapter/manifest.json"]).unwrap();
    manifest["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["asset_id"] != "bad");
    source
        .0
        .insert("chapter/manifest.json".into(), bytes(manifest));
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    assert_eq!(
        catalog
            .load_chapter_package(&source, "custom-volume", "lesson-a", &mut session)
            .unwrap()["rdf_added"],
        1
    );
}

#[test]
fn source_member_order_is_preserved_before_clause_detection() {
    let mut source = fixture();
    // Original json.loads keeps zheading before atext; sorting object members
    // would incorrectly move the trailing phrase before the clause anchor.
    source.0.insert("part3.json".into(),br#"[[{"content":{"zheading":[{"content":"5.4.1"}],"atext":[{"content":"the item shall be available"}]}}]]"#.to_vec());
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut selected = input();
    selected.parts = vec![3];
    let prepared = prepare_normative_engraving(&catalog, &source, &source, &selected).unwrap();
    assert_eq!(prepared.result["parts"][0]["unit_count"], 1);
    assert!(
        std::str::from_utf8(&prepared.artifacts["part3-concept-phase.ttl"])
            .unwrap()
            .contains("isoN:modality isoN:Shall")
    );
}

#[test]
fn package_digest_matches_frozen_sorted_ascii_chinese_non_bmp_oracle() {
    // The oracle bytes are independently transcribed from the locked Python
    // runtime.py canonical field contract, sort_keys=True, ensure_ascii=True.
    // Hash frozen in private package-digest-oracle-ascii.json, not recomputed
    // with the Rust writer under test.
    const EXPECTED: &str = "cbb0a5e1bc78fa71875d70e2439913674b57f8600d45c36c712d6cfe8224ec80";
    let manifest = json!({"version":"1.0.0","schema_version":"1.0","package_id":"包😀","namespace":"https://example.test/术语😀#","assets":[{"path":"chapter.txt","role":"view","asset_id":"章😀","sha256":"0".repeat(64)},{"sha256":"1".repeat(64),"graph_name":"https://example.test/图😀","path":"a.ttl","format":"ttl","role":"ontology","asset_id":"ASCII-a"}]});
    assert_eq!(
        semantica_runtime::books::package_digest(&manifest).unwrap(),
        EXPECTED,
        "actual canonical metadata: {}",
        String::from_utf8(semantica_runtime::books::package_identity_bytes(&manifest).unwrap())
            .unwrap()
    );
    let reordered:Value=serde_json::from_str(&format!("{{\"namespace\":\"https://example.test/术语😀#\",\"version\":\"1.0.0\",\"package_id\":\"包😀\",\"schema_version\":\"1.0\",\"assets\":[{{\"asset_id\":\"ASCII-a\",\"role\":\"ontology\",\"format\":\"turtle\",\"path\":\"a.ttl\",\"sha256\":\"{}\",\"graph_name\":\"https://example.test/图😀\"}},{{\"asset_id\":\"章😀\",\"role\":\"view\",\"sha256\":\"{}\",\"path\":\"chapter.txt\"}}]}}","1".repeat(64),"0".repeat(64))).unwrap();
    assert_eq!(
        semantica_runtime::books::package_digest(&reordered).unwrap(),
        EXPECTED
    );
}

#[test]
fn registry_is_injected_not_fixed_to_workspace_or_chapter_count() {
    let source = fixture();
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    assert_eq!(catalog.list_chapter_packages(None).len(), 1);
    assert_eq!(
        catalog
            .chapter_package_id("custom-volume", "lesson-a")
            .unwrap(),
        "synthetic.chapter"
    );
    assert!(
        catalog
            .validate_chapter_registry(&source, true)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        catalog
            .package_asset_text(&source, "synthetic.chapter", "text")
            .unwrap()["text"],
        "全文😀\n"
    );
    assert!(catalog.chapter_package_id("vol1", "ch01").is_err());
}
#[test]
fn source_verification_retains_every_missing_check_and_binding_failure() {
    let mut source = fixture();
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let report = catalog
        .verify_book_source_bindings(&source, &source, None, true, true)
        .unwrap();
    assert_eq!(report["check_count"], 8);
    assert_eq!(report["passed"], false);
    assert_eq!(report["checks"][0]["status"], "passed");
    assert_eq!(report["checks"][2]["check_id"], "book.tex.digest");
    assert_eq!(report["checks"][2]["status"], "blocked");
    source
        .0
        .insert("source.md".into(), b"edited chapter".to_vec());
    let drift = catalog
        .verify_book_source_bindings(&source, &source, None, false, false)
        .unwrap();
    assert_eq!(drift["checks"][0]["status"], "blocked");
    assert_eq!(drift["check_count"], 4);
    // Full contract validation remains different from book source acceptance.
    assert!(
        catalog
            .validate_chapter_registry(&source, true)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn complete_source_binding_pass_is_separate_from_a_valid_but_mismatched_contract() {
    let mut source = fixture();
    source.0.insert("tex.tex".into(), b"author tex".to_vec());
    source.0.insert("guide.md".into(), b"author guide".to_vec());
    let tex = semantica_runtime::sha256(b"author tex");
    let guide = semantica_runtime::sha256(b"author guide");
    let mut manifest: Value = serde_json::from_slice(&source.0["chapter/manifest.json"]).unwrap();
    let mut contract: Value = serde_json::from_slice(&source.0["chapter/contract.json"]).unwrap();
    for binding in [
        &mut manifest["book_source"],
        &mut contract["external_specification"],
    ] {
        binding["tex_anchor"] = json!("tex.tex");
        binding["tex_sha256"] = json!(tex);
        binding["guide_anchor"] = json!("guide.md");
        binding["guide_sha256"] = json!(guide);
    }
    source
        .0
        .insert("chapter/contract.json".into(), bytes(contract.clone()));
    manifest["assets"][0]["sha256"] = json!(semantica_runtime::sha256(
        &source.0["chapter/contract.json"]
    ));
    source
        .0
        .insert("chapter/manifest.json".into(), bytes(manifest.clone()));
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let report = catalog
        .verify_book_source_bindings(&source, &source, None, true, true)
        .unwrap();
    assert_eq!(report["check_count"], 8);
    assert_eq!(report["passed"], true);
    contract["external_specification"]["guide_sha256"] =
        json!(semantica_runtime::sha256(b"other guide"));
    source
        .0
        .insert("chapter/contract.json".into(), bytes(contract));
    manifest["assets"][0]["sha256"] = json!(semantica_runtime::sha256(
        &source.0["chapter/contract.json"]
    ));
    source
        .0
        .insert("chapter/manifest.json".into(), bytes(manifest));
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    assert!(
        catalog
            .validate_chapter_registry(&source, true)
            .unwrap()
            .is_empty()
    );
    let report = catalog
        .verify_book_source_bindings(&source, &source, None, true, true)
        .unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["checks"][4]["status"], "passed");
    assert_eq!(
        report["checks"][5]["check_id"],
        "book.contract.guide-binding"
    );
    assert_eq!(report["checks"][5]["status"], "blocked");
}

#[test]
fn exact_asset_text_accepts_empty_and_refuses_declared_binary_bytes() {
    for (content, valid) in [
        (&b""[..], true),
        (&b"utf8\0binary"[..], false),
        (&b"\xff"[..], false),
    ] {
        let mut source = fixture();
        source.0.insert("chapter/text.md".into(), content.to_vec());
        let mut manifest: Value =
            serde_json::from_slice(&source.0["chapter/manifest.json"]).unwrap();
        manifest["assets"][1]["sha256"] = json!(semantica_runtime::sha256(content));
        source
            .0
            .insert("chapter/manifest.json".into(), bytes(manifest));
        let catalog = Catalog::read(&source, "registry.json").unwrap();
        assert_eq!(
            catalog
                .package_asset_text(&source, "synthetic.chapter", "text")
                .is_ok(),
            valid
        );
    }
}
#[test]
fn catalog_inline_asset_uses_shared_actual_byte_reader_with_required_hash() {
    let mut source = fixture();
    let mut manifest: Value = serde_json::from_slice(&source.0["chapter/manifest.json"]).unwrap();
    manifest["assets"][1]
        .as_object_mut()
        .unwrap()
        .remove("path");
    manifest["assets"][1]["data"] = json!("全文😀\n");
    source
        .0
        .insert("chapter/manifest.json".into(), bytes(manifest));
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let text = catalog
        .package_asset_text(&source, "synthetic.chapter", "text")
        .unwrap();
    assert_eq!(text["text"], "全文😀\n");
    manifest = serde_json::from_slice(&source.0["chapter/manifest.json"]).unwrap();
    manifest["assets"][1]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    source
        .0
        .insert("chapter/manifest.json".into(), bytes(manifest));
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    assert!(
        catalog
            .package_asset_text(&source, "synthetic.chapter", "text")
            .is_err()
    );
}
#[test]
fn migration_retirement_and_ambiguity_preserve_original_dto_meaning() {
    let mut source = fixture();
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let hash = semantica_runtime::sha256(&source.0["chapter/text.md"]);
    source.0.insert("map.json".into(),bytes(json!({"schema_version":"1.0","mapping_count":2,"mappings":[{"old_path":"legacy.md","old_sha256":hash,"new_package":"synthetic.chapter","new_asset_id":"text","new_sha256":hash,"relation":"byte-preserved"},{"old_path":"legacy.md","old_sha256":hash,"new_package":"synthetic.chapter","new_asset_id":"text","new_sha256":hash,"relation":"alternate"}],"retired_runtime_entries":[{"legacy_path":"old.py","copied_as_runtime":false,"replacement_entry":"rust library","reason":"runtime retired","replacement_assets":["text"]}]})));
    let map = catalog
        .read_migration_map(
            &source,
            "custom-volume",
            "map.json",
            MigrationEncoding::SuccessorMappings,
        )
        .unwrap();
    assert_eq!(map["entry_count"], 2);
    assert_eq!(map["retired_runtime_entry_count"], 1);
    let resolution = Catalog::resolve_successor("legacy.md", &[map.clone()]).unwrap();
    assert_eq!(resolution["resolved"], true);
    assert_eq!(resolution["ambiguous"], true);
    assert_eq!(resolution["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(
        Catalog::resolve_successor("old.py", &[map.clone()]).unwrap()["resolved"],
        false
    );
    source
        .0
        .insert("chapter/text.md".into(), b"replacement bytes".to_vec());
    assert!(
        catalog
            .read_migration_map(
                &source,
                "custom-volume",
                "map.json",
                MigrationEncoding::SuccessorMappings
            )
            .is_err()
    );
}
#[test]
fn real_engraving_emits_only_author_glosses_coordinates_and_coverage() {
    let mut source = fixture();
    // Both cases score equally for this term, but the source puts z first.
    // The legacy algorithm orders case IDs before emission and tie selection.
    let cases = "z-case:\n  ch: ch04\n  teaches: [测试术语]\n  summary: later case\na-case:\n  ch: ch04\n  teaches: [测试术语]\n  summary: earlier case\n".as_bytes();
    source.0.insert("domain/cases.seed".into(), cases.to_vec());
    let mut manifest: Value = serde_json::from_slice(&source.0["domain/manifest.json"]).unwrap();
    manifest["assets"][2]["sha256"] = json!(semantica_runtime::sha256(cases));
    source
        .0
        .insert("domain/manifest.json".into(), bytes(manifest));
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let prepared = prepare_normative_engraving(&catalog, &source, &source, &input()).unwrap();
    assert_eq!(
        prepared.result["parts"],
        json!([{"part":1,"unit_count":1,"glossed_count":1,"pending_count":0},{"part":3,"unit_count":2,"glossed_count":1,"pending_count":1}])
    );
    assert_eq!(
        prepared.result["semantic_dataset_hash_profile"],
        "semantica.rdfc-1.0-sha256/1"
    );
    for (name, payload) in &prepared.artifacts {
        assert!(!String::from_utf8_lossy(payload).contains("SECRET-RESTRICTED-STANDARD-TEXT"));
        let artifact = prepared.result["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["relative_path"] == name.as_str())
            .unwrap();
        assert_eq!(artifact["sha256"], semantica_runtime::sha256(payload));
        assert_eq!(artifact["size_bytes"], payload.len());
        if name.ends_with(".ttl") {
            semantica_runtime::graph::load(payload).unwrap();
        }
    }
    let p1 = std::str::from_utf8(&prepared.artifacts["part1-vocabulary.ttl"]).unwrap();
    assert!(p1.contains("这是作者的公开转述。"));
    assert!(p1.contains("isoN:taughtBy isoN:Case_a_case"));
    assert!(!p1.contains("isoN:taughtBy isoN:Case_z_case"));
    assert!(p1.contains("isoN:pageIndex 0 ; isoN:blockIndex 0"));
    let cases_ttl = std::str::from_utf8(&prepared.artifacts["teaching-cases.ttl"]).unwrap();
    assert!(
        cases_ttl.find("isoN:Case_a_case").unwrap() < cases_ttl.find("isoN:Case_z_case").unwrap()
    );
    let p3 = std::str::from_utf8(&prepared.artifacts["part3-concept-phase.ttl"]).unwrap();
    assert!(p3.contains("isoN:modality isoN:Shall"));
    assert!(p3.contains("isoN:glossStatus \"pending\""));
    let report: Value =
        serde_json::from_slice(&prepared.artifacts["engraving-report.json"]).unwrap();
    let mut content = report.clone();
    // Preserve emitted canonical order: remove() may swap in the final key
    // when serde_json preserve_order is enabled by workspace features.
    content
        .as_object_mut()
        .unwrap()
        .retain(|key, _| key != "bundle_sha256");
    assert_eq!(
        report["bundle_sha256"],
        semantica_runtime::sha256(serde_json::to_vec(&content).unwrap())
    );
    assert_eq!(report["artifacts"].as_array().unwrap().len(), 6);
}
#[test]
fn teaching_seed_bounded_parser_preserves_malformed_historical_scalar() {
    let cases = parse_teaching_cases(
        "a-case:\n  ch: ch04\n  teaches: [条件, 因果]\n  summary: \"关断\"——没有改写源字节。\n"
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(cases["a-case"]["summary"], "\"关断\"——没有改写源字节。");
    for invalid in [
        "x:\n  ch: ch01\n  teaches: []\n  summary: empty\n",
        "x:\n  ch: ch01\n  ch: ch02\n  teaches: [x]\n  summary: duplicate\n",
        "x:\n  ch: ch01\n  teaches: [x]\n  summary: okay\n  unknown: refused\n",
    ] {
        assert!(parse_teaching_cases(invalid.as_bytes()).is_err());
    }
}
#[test]
fn missing_source_duplicate_anchor_and_invalid_rdf_all_refuse_prepared_bundle() {
    let source = fixture();
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut bad = input();
    bad.parts = vec![3, 3];
    assert!(prepare_normative_engraving(&catalog, &source, &source, &bad).is_err());
    let mut source = fixture();
    source.0.remove("part3.json");
    assert!(prepare_normative_engraving(&catalog, &source, &source, &input()).is_err());
    let mut source = fixture();
    source.0.insert("part1.json".into(),bytes(json!([[{"content":{"paragraphs":[{"content":"3.1 anchor"}]}},{"content":{"paragraphs":[{"content":"3.1 duplicate"}]}}]])));
    assert!(prepare_normative_engraving(&catalog, &source, &source, &input()).is_err());
    let mut source = fixture();
    source
        .0
        .insert("domain/tbox.ttl".into(), b"not turtle".to_vec());
    let mut m: Value = serde_json::from_slice(&source.0["domain/manifest.json"]).unwrap();
    m["assets"][0]["sha256"] = json!(semantica_runtime::sha256(b"not turtle"));
    source.0.insert("domain/manifest.json".into(), bytes(m));
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    assert!(prepare_normative_engraving(&catalog, &source, &source, &input()).is_err());
}

#[test]
fn chapter_query_retains_prior_large_result_domain_without_caching() {
    use oxigraph::model::{Literal, Term};
    use semantica_runtime::{dataset::DatasetSession, graph};
    let mut source = fixture();
    let query = "SELECT ?v WHERE { VALUES ?copy { 1 2 } }";
    add_asset(
        &mut source,
        "chapter/manifest.json",
        "large-query",
        "sparql",
        "chapter/large-query.rq",
        query.as_bytes(),
    );
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = DatasetSession::new();
    let namespaces = BTreeMap::new();
    let bindings = BTreeMap::from([(
        "v".into(),
        Term::Literal(Literal::new_simple_literal("a".repeat(9 * 1024 * 1024))),
    )]);
    // This is the real prior Catalog execution path, with the same frozen input.
    let prior =
        graph::query_with_options(session.dataset(), query, &namespaces, &bindings).unwrap();
    assert_eq!(prior["row_count"], 2);
    assert!(serde_json::to_vec(&prior).unwrap().len() > semantica_runtime::MAX_BYTES);
    drop(prior);
    // Existing session result budgets must not be relaxed to accommodate books.
    assert!(
        session
            .query_with_options(query, &namespaces, &bindings, true)
            .unwrap_err()
            .to_string()
            .contains("dataset_query_result_byte_budget")
    );
    let result = catalog
        .execute_chapter_query(
            &source,
            "custom-volume",
            "lesson-a",
            "large-query",
            &mut session,
            &namespaces,
            &bindings,
        )
        .unwrap();
    assert_eq!(result["row_count"], 2);
    for row in result["rows"].as_array().unwrap() {
        let value = row["v"]["value"].as_str().unwrap();
        assert_eq!(value.len(), 9 * 1024 * 1024);
        assert!(value.bytes().all(|b| b == b'a'));
        assert_eq!(
            row["v"]["datatype"],
            "http://www.w3.org/2001/XMLSchema#string"
        );
    }
    // A large chapter response must not seed a cache entry that can bypass the
    // ordinary session's original limit on a subsequent query.
    assert!(
        session
            .query_with_options(query, &namespaces, &bindings, true)
            .unwrap_err()
            .to_string()
            .contains("dataset_query_result_byte_budget")
    );
}
