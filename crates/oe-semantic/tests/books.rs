//! Explicit fixture executors exercise Rust transport/identity/IO contracts;
//! only the opt-in test calls the actual locked external Semantica APIs.
use oe_core::{SkillRoot, create_new_private_dir, sha256, write_new_bytes};
use oe_semantic::books::{self, BookBackend, BookRequest, EngraveOptions};
use oe_semantic::native::{NativeExecution, NativeRuntime};
use oe_semantic::runtime::{RuntimeVerification, read_runtime_source_lock};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
const PART3: &str =
    "part-03-concept-phase/native-full/ISO 26262-3-2018/auto/ISO 26262-3-2018_content_list_v2.json";
fn installed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_owned()
}
fn save(path: &Path, value: &Value) {
    if !path.parent().unwrap().exists() {
        create_new_private_dir(path.parent().unwrap()).unwrap();
    }
    write_new_bytes(path, &serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
struct Case {
    root: SkillRoot,
}
impl Case {
    fn new() -> Self {
        let path = installed().join(format!(
            "var/maintenance/rust-tests/books-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        create_new_private_dir(&path.join("runtime")).unwrap();
        fs::write(path.join("SKILL.md"), "Synthetic book API fixture").unwrap();
        fs::copy(
            installed().join("runtime/semantica-source-lock.json"),
            path.join("runtime/semantica-source-lock.json"),
        )
        .unwrap();
        create_new_private_dir(&path.join("var/project/source")).unwrap();
        create_new_private_dir(&path.join("var/project/book")).unwrap();
        Self {
            root: SkillRoot::new(path).unwrap(),
        }
    }
    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }
    fn engrave(&self) -> EngraveOptions {
        save(
            &self.path(&format!("var/project/source/{PART3}")),
            &json!([[{"content":{"paragraphs":[{"content":"5.1 Synthetic shall remain a controlled test statement."}]}}]]),
        );
        EngraveOptions {
            source_root: PathBuf::from("var/project/source"),
            book_root: PathBuf::from("var/project/book"),
            output: PathBuf::from("var/output"),
            parts: vec![3],
            overwrite: false,
        }
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        fs::remove_dir_all(self.root.path()).unwrap();
    }
}
type Handler<'a> = Box<dyn FnMut(&Value) -> Value + 'a>;
struct Stub<'a> {
    root: &'a SkillRoot,
    calls: Cell<usize>,
    handler: RefCell<Handler<'a>>,
    state: &'static str,
    wrong_identity: bool,
}
impl<'a> Stub<'a> {
    fn new(root: &'a SkillRoot, handler: impl FnMut(&Value) -> Value + 'a) -> Self {
        Self {
            root,
            calls: Cell::new(0),
            handler: RefCell::new(Box::new(handler)),
            state: "completed",
            wrong_identity: false,
        }
    }
}
impl BookBackend for Stub<'_> {
    fn execute_book(&self, request: &Value) -> NativeExecution {
        self.calls.set(self.calls.get() + 1);
        let mut source = read_runtime_source_lock(self.root.path(), false).unwrap();
        if self.wrong_identity {
            source.commit = "0".repeat(40);
        }
        NativeExecution {
            operation: request["operation"].as_str().unwrap().into(),
            semantic_execution: self.state.into(),
            runtime_identity: Some(RuntimeVerification {
                source_lock: source,
                record_files: 790,
                observation_kind: "explicit_test_fixture_not_actual_native".into(),
                semantic_execution: "not_run".into(),
                staging_descriptor_sha256: None,
            }),
            result: Some((self.handler.borrow_mut())(request)),
            error: if self.state == "completed" {
                None
            } else {
                Some("synthetic external failure".into())
            },
            elapsed_seconds: 0.0,
            stderr: "fixture only".into(),
        }
    }
}
fn successor(volume: &str) -> Value {
    json!({"volume":volume,"old_path":"legacy/asset.ttl","old_sha256":"a".repeat(64),"package_id":"semantica.chapter_packages.vol1.ch02","asset_id":"ontology-core","successor_sha256":"b".repeat(64),"relation":"preserved","successor_path":"semantica/chapter_packages/vol1/ch02/ontology.ttl","source_fragment":null,"strict_gate_asset":true})
}
fn map() -> Value {
    json!({"schema_version":"1.0","volume":"vol1","source_sha256":"c".repeat(64),"entry_count":1,"entries":[successor("vol1")],"retired_runtime_entry_count":1,"retired_runtime_entries":[{"legacy_path":"old.py","copied_as_runtime":false,"replacement_entry":"native API","replacement_assets":["ontology-core"],"reason":"fixture provenance"}]})
}
#[test]
fn migration_map_retains_exact_entries_retired_history_and_source_identity() {
    let c = Case::new();
    let stub = Stub::new(&c.root, |_| map());
    let r = books::execute_with(
        &c.root,
        &BookRequest::MigrationMap {
            volume: "vol1".into(),
        },
        &stub,
    )
    .unwrap();
    assert_eq!(r["status"], "completed");
    assert_eq!(r["result"], map());
    assert_eq!(r["fact_admitted"], false);
    assert_eq!(stub.calls.get(), 1);
}
#[test]
fn resolution_keeps_missing_and_ambiguous_candidates_without_picking_first() {
    let c = Case::new();
    for candidates in [vec![], vec![successor("vol1"), successor("vol2")]] {
        let expected = json!({"old_path":"legacy/asset.ttl","resolved":!candidates.is_empty(),"ambiguous":candidates.len()>1,"candidates":candidates});
        let stub = Stub::new(&c.root, |_| expected.clone());
        let r = books::execute_with(
            &c.root,
            &BookRequest::ResolveSuccessor {
                old_path: "legacy/asset.ttl".into(),
                volume: None,
            },
            &stub,
        )
        .unwrap();
        assert_eq!(r["status"], "completed");
        assert_eq!(r["result"], expected);
    }
}
#[test]
fn malformed_map_counts_and_resolution_flags_are_blocked_with_full_raw_retained() {
    let c = Case::new();
    let mut raw = map();
    raw["entry_count"] = json!(9);
    let stub = Stub::new(&c.root, |_| raw.clone());
    let r = books::execute_with(
        &c.root,
        &BookRequest::MigrationMap {
            volume: "vol1".into(),
        },
        &stub,
    )
    .unwrap();
    assert_eq!(r["status"], "blocked");
    assert_eq!(r["result"], raw);
    let raw = json!({"old_path":"legacy/asset.ttl","resolved":true,"ambiguous":false,"candidates":[successor("vol1"),successor("vol2")]});
    let stub = Stub::new(&c.root, |_| raw.clone());
    let r = books::execute_with(
        &c.root,
        &BookRequest::ResolveSuccessor {
            old_path: "legacy/asset.ttl".into(),
            volume: None,
        },
        &stub,
    )
    .unwrap();
    assert_eq!(r["status"], "blocked");
    assert_eq!(r["result"], raw);
}
#[test]
fn asset_text_preserves_empty_unicode_and_nonbmp_bytes() {
    let c = Case::new();
    for text in ["", "原始文本😀\n"] {
        let stub = Stub::new(
            &c.root,
            |req| json!({"package_id":req["package_id"],"asset_id":req["asset_id"],"text":text}),
        );
        let r = books::execute_with(
            &c.root,
            &BookRequest::AssetText {
                package_id: "registered".into(),
                asset_id: "text".into(),
            },
            &stub,
        )
        .unwrap();
        assert_eq!(r["status"], "completed");
        assert_eq!(r["result"]["text"], text);
        assert_eq!(r["text_sha256"], sha256(text));
    }
}
#[test]
fn wrong_asset_identity_binary_text_and_runtime_identity_never_succeed() {
    let c = Case::new();
    let request = BookRequest::AssetText {
        package_id: "registered".into(),
        asset_id: "text".into(),
    };
    for raw in [
        json!({"package_id":"different","asset_id":"text","text":"raw"}),
        json!({"package_id":"registered","asset_id":"text","text":"a\u{0}b"}),
    ] {
        let stub = Stub::new(&c.root, |_| raw.clone());
        let r = books::execute_with(&c.root, &request, &stub).unwrap();
        assert_eq!(r["status"], "blocked");
        assert_eq!(r["result"], raw);
    }
    let mut stub = Stub::new(
        &c.root,
        |_| json!({"package_id":"registered","asset_id":"text","text":"raw"}),
    );
    stub.wrong_identity = true;
    assert_eq!(
        books::execute_with(&c.root, &request, &stub).unwrap()["status"],
        "blocked"
    );
}
#[test]
fn unsupported_inputs_are_denied_before_external_execution_or_output() {
    let c = Case::new();
    let stub = Stub::new(&c.root, |_| panic!("preflight must reject"));
    for request in [
        BookRequest::MigrationMap {
            volume: "vol3".into(),
        },
        BookRequest::ResolveSuccessor {
            old_path: "../escape".into(),
            volume: None,
        },
        BookRequest::ChapterAssetText {
            volume: "vol1".into(),
            chapter: "".into(),
            asset_id: "text".into(),
        },
    ] {
        assert!(books::execute_with(&c.root, &request, &stub).is_err());
    }
    let original = c.engrave();
    for parts in [vec![], vec![2], vec![1, 1]] {
        let mut o = original.clone();
        o.parts = parts;
        assert!(books::execute_with(&c.root, &BookRequest::Engrave(o), &stub).is_err());
    }
    let mut o = original.clone();
    o.output = o.source_root.clone();
    o.overwrite = true;
    assert!(books::execute_with(&c.root, &BookRequest::Engrave(o), &stub).is_err());
    let mut o = original;
    o.output = PathBuf::from("references/output");
    assert!(books::execute_with(&c.root, &BookRequest::Engrave(o), &stub).is_err());
    assert_eq!(stub.calls.get(), 0);
    assert!(!c.path("var/output").exists());
}
fn book_check(anchor: &str, sha: &str, passed: bool) -> Value {
    json!({"schema_version":"1.0","passed":passed,"status":if passed{"passed"}else{"blocked"},"check_count":1,"checks":[{"check_id":"book.primary.digest","package_id":"package","source_anchor":anchor,"expected_sha256":if passed{sha.to_owned()}else{"0".repeat(64)},"actual_sha256":sha,"status":if passed{"passed"}else{"blocked"},"reason":if passed{Value::Null}else{json!("source mismatch fixture")}}]})
}
#[test]
fn book_verification_uses_exact_private_text_snapshot_and_preserves_native_blocked_checks() {
    let c = Case::new();
    let rel = "references/ontology-engineering-book/ch01/chapter.md";
    create_new_private_dir(c.path(rel).parent().unwrap()).unwrap();
    fs::write(c.path(rel), "Source 😀").unwrap();
    fs::write(
        c.path("references/ontology-engineering-book/rendered.pdf"),
        b"PDF must not be snapshotted",
    )
    .unwrap();
    for passed in [true, false] {
        let stub = Stub::new(&c.root, |req| {
            let snapshot = Path::new(req["book_root"].as_str().unwrap());
            assert_ne!(snapshot, c.root.path());
            assert_eq!(
                fs::read(snapshot.join(rel)).unwrap(),
                b"Source \xf0\x9f\x98\x80"
            );
            assert!(
                !snapshot
                    .join("references/ontology-engineering-book/rendered.pdf")
                    .exists()
            );
            book_check(rel, &sha256("Source 😀"), passed)
        });
        let r = books::execute_with(&c.root, &BookRequest::VerifyBookSources, &stub).unwrap();
        assert_eq!(r["status"], if passed { "completed" } else { "blocked" });
        assert_eq!(r["result"]["passed"], passed);
        assert_eq!(r["input_sha256"][rel], sha256("Source 😀"));
    }
}
#[test]
fn source_change_after_snapshot_blocks_and_retains_the_native_findings() {
    let c = Case::new();
    let rel = "references/ontology-engineering-book/ch01/chapter.md";
    create_new_private_dir(c.path(rel).parent().unwrap()).unwrap();
    fs::write(c.path(rel), "original").unwrap();
    let stub = Stub::new(&c.root, |_| {
        fs::write(c.path(rel), "changed").unwrap();
        book_check(rel, &sha256("original"), true)
    });
    let r = books::execute_with(&c.root, &BookRequest::VerifyBookSources, &stub).unwrap();
    assert_eq!(r["status"], "blocked");
    assert_eq!(r["result"]["passed"], true);
    assert_eq!(r["native_execution"]["semantic_execution"], "completed");
}
#[test]
fn native_logical_repository_prefix_maps_to_the_same_source_without_rewriting_report() {
    let c = Case::new();
    let rel = "references/product-trustworthiness-book/ch01/chapter.md";
    create_new_private_dir(c.path(rel).parent().unwrap()).unwrap();
    fs::write(c.path(rel), "exact Vol.2 source").unwrap();
    let anchor = format!("ontology-engineering/{rel}");
    let expected = book_check(&anchor, &sha256("exact Vol.2 source"), true);
    let stub = Stub::new(&c.root, |_| expected.clone());
    let r = books::execute_with(&c.root, &BookRequest::VerifyBookSources, &stub).unwrap();
    assert_eq!(r["status"], "completed");
    assert_eq!(r["result"], expected);
    assert_eq!(r["input_sha256"][rel], sha256("exact Vol.2 source"));
}
#[cfg(unix)]
#[test]
fn book_symlinks_are_rejected_before_any_native_read() {
    let c = Case::new();
    create_new_private_dir(&c.path("references/ontology-engineering-book")).unwrap();
    std::os::unix::fs::symlink(
        c.path("SKILL.md"),
        c.path("references/ontology-engineering-book/source.md"),
    )
    .unwrap();
    let stub = Stub::new(&c.root, |_| panic!("no native"));
    assert!(books::execute_with(&c.root, &BookRequest::VerifyBookSources, &stub).is_err());
    assert_eq!(stub.calls.get(), 0);
}
fn fixture_engrave(request: &Value) -> Value {
    let output = Path::new(request["output"].as_str().unwrap());
    if !output.exists() {
        create_new_private_dir(output).unwrap();
    }
    let raw = b"Synthetic candidate artifact, no engine calculation";
    fs::write(output.join("candidate.ttl"), raw).unwrap();
    let artifact = json!({"relative_path":"candidate.ttl","media_type":"text/turtle","sha256":sha256(raw),"size_bytes":raw.len()});
    let source = fs::read(Path::new(request["source_root"].as_str().unwrap()).join(PART3)).unwrap();
    let mut report = json!({"schema_version":"1.0","package_id":"semantica.chapter_packages.vol2.normative","package_version":"1.0","package_digest":"a".repeat(64),"parts":[{"part":3,"unit_count":1,"glossed_count":0,"pending_count":1}],"source_hashes":{"controlled.part3.extract":sha256(source)},"artifacts":[artifact],"semantic_dataset_sha256":"b".repeat(64),"restricted_standard_text_included":false});
    report["bundle_sha256"] = json!(sha256(oe_core::json::canonical_json(&report).unwrap()));
    let report_raw = serde_json::to_vec(&report).unwrap();
    fs::write(output.join("engraving-report.json"), &report_raw).unwrap();
    let mut value = report;
    value["output_directory"] = request["output"].clone();
    value["artifacts"].as_array_mut().unwrap().push(json!({"relative_path":"engraving-report.json","media_type":"application/json","sha256":sha256(&report_raw),"size_bytes":report_raw.len()}));
    value
}
#[test]
fn engraver_keeps_explicit_parts_full_native_receipt_and_unrelated_overwrite_file() {
    let c = Case::new();
    let mut options = c.engrave();
    create_new_private_dir(&c.path("var/output")).unwrap();
    fs::write(c.path("var/output/keep.txt"), b"Unrelated").unwrap();
    options.overwrite = true;
    let stub = Stub::new(&c.root, |req| {
        assert_eq!(req["parts"], json!([3]));
        assert_ne!(req["source_root"], json!(c.path("var/project/source")));
        fixture_engrave(req)
    });
    let r = books::execute_with(&c.root, &BookRequest::Engrave(options), &stub).unwrap();
    assert_eq!(r["status"], "completed", "{r}");
    assert_eq!(r["result"]["parts"][0]["pending_count"], 1);
    assert_eq!(
        fs::read(c.path("var/output/keep.txt")).unwrap(),
        b"Unrelated"
    );
    assert_eq!(r["publication"], "not_performed");
}
#[test]
fn output_tamper_undeclared_file_and_partial_native_write_are_retained_but_blocked() {
    for mode in ["tamper", "undeclared", "native_failure"] {
        let c = Case::new();
        let options = c.engrave();
        let mut stub = Stub::new(&c.root, move |req| {
            let raw = fixture_engrave(req);
            let output = Path::new(req["output"].as_str().unwrap());
            if mode == "tamper" {
                fs::write(output.join("candidate.ttl"), b"changed").unwrap();
            }
            if mode == "undeclared" {
                fs::write(output.join("unlisted.txt"), b"extra").unwrap();
            }
            raw
        });
        if mode == "native_failure" {
            stub.state = "failed";
        }
        let r = books::execute_with(&c.root, &BookRequest::Engrave(options), &stub).unwrap();
        assert_eq!(r["status"], "blocked");
        assert!(r["result"].is_object());
        assert!(c.path("var/output/candidate.ttl").exists());
        assert_eq!(
            r["output_state"],
            "artifacts_may_exist_inspect_native_result"
        );
    }
}
#[test]
fn direct_native_boundary_rejects_unknown_fields_and_external_paths_without_python() {
    let c = Case::new();
    let native = NativeRuntime::new(c.root.path(), Path::new("/unavailable/python")).unwrap();
    for request in [
        json!({"operation":"book_migration_map","volume":"vol1","fallback":true}),
        json!({"operation":"book_verify_sources","book_root":"/tmp"}),
        json!({"operation":"unknown"}),
    ] {
        let r = native.book_api(&request);
        assert_eq!(r.semantic_execution, "not_run");
        assert!(r.runtime_identity.is_none());
    }
}

#[test]
#[ignore = "Calls six source-locked external book APIs; creates only synthetic private engraving inputs/outputs"]
fn actual_book_apis_and_synthetic_normative_engraving() {
    let root = SkillRoot::new(installed()).unwrap();
    let out = root
        .new_private_dir(Path::new(&format!(
            "var/maintenance/rust-rewrite-20260927/phase9-semantic-audit/actual-{}",
            std::process::id()
        )))
        .unwrap();
    let retain = |name: &str, v: &Value| {
        save(&out.join(format!("{name}.json")), v);
        assert_eq!(
            v["native_execution"]["semantic_execution"], "completed",
            "{name}: {v}"
        );
    };
    let map = books::read_migration_map(&root, "vol1").unwrap();
    retain("migration", &map);
    assert_eq!(map["status"], "completed");
    let entry = map["result"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["package_id"]
                .as_str()
                .unwrap()
                .starts_with("semantica.chapter_packages.vol1.ch")
        })
        .unwrap();
    let resolve =
        books::resolve_successor(&root, entry["old_path"].as_str().unwrap(), Some("vol1")).unwrap();
    retain("resolve", &resolve);
    assert_eq!(resolve["status"], "completed");
    let text = books::asset_text(
        &root,
        entry["package_id"].as_str().unwrap(),
        entry["asset_id"].as_str().unwrap(),
    )
    .unwrap();
    retain("asset", &text);
    assert_eq!(text["status"], "completed");
    let chapter = entry["package_id"]
        .as_str()
        .unwrap()
        .rsplit('.')
        .next()
        .unwrap();
    let chapter_text =
        books::chapter_asset_text(&root, "vol1", chapter, entry["asset_id"].as_str().unwrap())
            .unwrap();
    retain("chapter-asset", &chapter_text);
    assert_eq!(chapter_text["result"]["text"], text["result"]["text"]);
    let binding = books::verify_book_sources(&root).unwrap();
    retain("book-bindings", &binding);
    assert!(binding.get("validation_error").is_none(), "{binding}");
    assert_eq!(
        binding["status"],
        if binding["result"]["passed"] == true {
            "completed"
        } else {
            "blocked"
        }
    );
    assert_eq!(
        binding["result"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["package_id"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        29
    );
    create_new_private_dir(&out.join("source")).unwrap();
    create_new_private_dir(&out.join("book")).unwrap();
    save(
        &out.join("source").join(PART3),
        &json!([[{"content":{"paragraphs":[{"content":"5.1 Synthetic shall preserve this example as private test input."}]}}]]),
    );
    let options = EngraveOptions {
        source_root: out.join("source"),
        book_root: out.join("book"),
        output: out.join("candidate"),
        parts: vec![3],
        overwrite: false,
    };
    let result = books::engrave(&root, &options).unwrap();
    retain("engraving", &result);
    assert_eq!(result["status"], "completed", "{result}");
    assert_eq!(result["result"]["restricted_standard_text_included"], false);
    assert_eq!(result["publication"], "not_performed");
    println!("retained native book API evidence: {}", out.display());
}
