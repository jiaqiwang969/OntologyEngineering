//! Pure-byte tests. Synthetic sources/checkouts only; no native Semantica, models,
//! book publication, installed runtime, source lock or production registry writes.
use oe_core::json::strict_json;
use oe_core::{read_regular_bytes, sha256};
use oe_semantic::book_sources::{
    self as books, SearchOptions, VOL1, VOL1_CHAPTERS, VOL2, VOL2_CHAPTERS,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
            .join(format!(
                "var/maintenance/rust-tests/book-sources-{label}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        oe_core::create_new_private_dir(&path).unwrap();
        Self(path)
    }
    fn book(&self, volume: &str) -> PathBuf {
        self.0.join("references").join(volume)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn write(path: &Path, text: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn json_write(path: &Path, value: &Value) {
    let mut raw = serde_json::to_vec_pretty(value).unwrap();
    raw.push(b'\n');
    write(path, raw);
}
fn read(path: &Path) -> Value {
    strict_json(&read_regular_bytes(path, 16 * 1024 * 1024).unwrap()).unwrap()
}
fn snapshot(root: &Path) -> BTreeMap<String, String> {
    fn visit(base: &Path, path: &Path, map: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(base, &path, map);
            } else {
                map.insert(
                    path.strip_prefix(base).unwrap().to_str().unwrap().into(),
                    sha256(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut map = BTreeMap::new();
    visit(root, root, &mut map);
    map
}
fn corpus(f: &Fixture) {
    write(&f.0.join("SKILL.md"), "synthetic fixture only\n");
    let one = f.book(VOL1);
    let two = f.book(VOL2);
    for p in [
        "README.md",
        "resources/README.md",
        "handbook/README.md",
        "handbook/main.tex",
        "handbook/preamble.tex",
        "handbook/chapters/appB-glossary.tex",
        "handbook/build_handbook.py",
        "handbook/figures/image.png",
        "handbook/fragments/generated.tex",
    ] {
        write(&one.join(p), format!("fixture {p}\n"));
    }
    for (i, ch) in VOL1_CHAPTERS.iter().enumerate() {
        write(&one.join(ch).join("README.md"), "reader guide\n");
        write(
            &one.join(format!("handbook/chapters/ch{:02}.tex", i + 1)),
            "source 可信測試 UniqueBookNeedle\n",
        );
    }
    for p in [
        "README.md",
        "propositions-index.md",
        "handbook/README.md",
        "front-matter/preface.md",
        "handbook/book-metadata.tex",
        "handbook/build_handbook.py",
        "handbook/build_isolated.py",
        "handbook/main.tex",
        "handbook/preamble.tex",
        "handbook/test_build_handbook.py",
        "handbook/figures-imagegen/a.png",
        "handbook/figures-rendered/a.png",
    ] {
        write(&two.join(p), format!("fixture {p}\n"));
    }
    for ch in VOL2_CHAPTERS {
        write(
            &two.join(ch).join("chapter.md"),
            "source 可信測試 UniqueBookNeedle\n",
        );
        write(&two.join(ch).join("README.md"), "reader guide\n");
    }
    for name in [
        "a-semiconductor",
        "b-motorcycle-truck",
        "c-glossary",
        "d-method-tables",
    ] {
        write(
            &two.join(format!("appendices/appendix-{name}.md")),
            "appendix\n",
        );
    }
}
fn locked(f: &Fixture) {
    corpus(f);
    assert_eq!(
        books::check_authoring_locks(&f.0, true).unwrap()["passed"],
        true
    );
}

#[test]
fn four_locks_are_explicit_sorted_bound_and_idempotent() {
    let f = Fixture::new("locks");
    corpus(&f);
    write(
        &f.book(VOL1).join("handbook/unpublished.pdf"),
        "pdf artifact excluded",
    );
    let before = snapshot(&f.0.join("references"));
    let check = books::check_authoring_locks(&f.0, false).unwrap();
    assert_eq!(check["passed"], false);
    assert!(!f.0.join("var").exists());
    assert_eq!(before, snapshot(&f.0.join("references")));
    let written = books::check_authoring_locks(&f.0, true).unwrap();
    assert_eq!(written["passed"], true);
    assert_eq!(written["locks"].as_array().unwrap().len(), 4);
    assert!(
        written["locks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["updated"] == true)
    );
    let stage =
        f.0.join(written["write_recovery"]["recovery"].as_str().unwrap());
    assert!(stage.join("plan.json").is_file());
    assert_eq!(read(&stage.join("result.json"))["atomic_batch"], false);
    let raw = fs::read_to_string(f.book(VOL1).join("authoring-sources.sha256")).unwrap();
    assert!(!raw.contains("unpublished.pdf"));
    assert!(raw.contains("handbook/build_handbook.py"));
    assert!(raw.contains("handbook/figures/image.png"));
    let lines: Vec<_> = raw
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split_once("  ").unwrap().1)
        .collect();
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted);
    assert_eq!(
        books::check_authoring_locks(&f.0, false).unwrap()["passed"],
        true
    );
    let clean = books::check_authoring_locks(&f.0, true).unwrap();
    assert!(
        clean["locks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["updated"] == false)
    );
    assert!(clean.get("write_recovery").is_none());
    write(
        &f.book(VOL2).join("ch20-assurance-ontology/README.md"),
        "new guide\n",
    );
    let drift = books::check_authoring_locks(&f.0, false).unwrap();
    assert_eq!(drift["locks"][3]["matched"], false);
    assert_eq!(drift["locks"][1]["matched"], true);
}

#[test]
fn formal_search_retains_both_books_unicode_hashes_and_excludes_locked_tools() {
    let f = Fixture::new("search");
    corpus(&f);
    write(
        &f.book(VOL1).join("handbook/build_handbook.py"),
        "ToolOnlyNeedle\n",
    );
    write(
        &f.book(VOL2).join("historical-notes.md"),
        "ArchiveOnlyNeedle\n",
    );
    books::check_authoring_locks(&f.0, true).unwrap();
    let opts = SearchOptions {
        limit: 100,
        ..Default::default()
    };
    let hits = books::search(&f.0, "可信測試 UniqueBookNeedle", &opts).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 29);
    assert!(
        hits.as_array()
            .unwrap()
            .iter()
            .any(|h| h["path"].as_str().unwrap().starts_with(VOL1))
    );
    assert!(
        hits.as_array()
            .unwrap()
            .iter()
            .any(|h| h["path"].as_str().unwrap().starts_with(VOL2))
    );
    for hit in hits.as_array().unwrap() {
        assert_eq!(hit["line"], 1);
        assert!(hit["context"][0].as_str().unwrap().starts_with("1: source"));
        assert_eq!(
            hit["source_sha256"],
            sha256("source 可信測試 UniqueBookNeedle\n")
        );
    }
    assert!(
        books::search(&f.0, "ToolOnlyNeedle", &opts)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    let archive = SearchOptions {
        scope: "archive".into(),
        ..opts
    };
    for query in ["ArchiveOnlyNeedle", "ToolOnlyNeedle"] {
        let hits = books::search(&f.0, query, &archive).unwrap();
        assert!(!hits.as_array().unwrap().is_empty());
        for hit in hits.as_array().unwrap() {
            assert_eq!(hit["source_status"], "archive_non_authoritative");
            assert_eq!(hit["provenance_warning"], books::ARCHIVE_WARNING);
            assert!(hit.get("source_sha256").is_none());
        }
    }
}

#[test]
fn formal_drift_fails_even_no_match_or_zero_limit_and_guides_are_closed() {
    let f = Fixture::new("drift");
    locked(&f);
    let guide = f.book(VOL2).join("ch20-assurance-ontology/README.md");
    write(&guide, "changed\n");
    assert!(
        books::search(
            &f.0,
            "NoPossibleMatch",
            &SearchOptions {
                limit: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
    books::check_authoring_locks(&f.0, true).unwrap();
    let lock = f.book(VOL2).join("handbook/formal-search-guides.sha256");
    let raw = fs::read_to_string(&lock).unwrap();
    write(
        &lock,
        raw.lines()
            .filter(|l| !l.contains("ch20-assurance-ontology"))
            .map(|l| format!("{l}\n"))
            .collect::<String>(),
    );
    assert!(books::search(&f.0, "NoPossibleMatch", &SearchOptions::default()).is_err());
    write(
        &lock,
        format!(
            "{raw}{}  front-matter/preface.md\n",
            sha256(fs::read(f.book(VOL2).join("front-matter/preface.md")).unwrap())
        ),
    );
    assert!(books::search(&f.0, "NoPossibleMatch", &SearchOptions::default()).is_err());
}

#[test]
fn lock_duplicates_noncanonical_paths_and_retired_lock_are_rejected() {
    let f = Fixture::new("bad-lock");
    locked(&f);
    let lock = f.book(VOL1).join("authoring-sources.sha256");
    let raw = fs::read_to_string(&lock).unwrap();
    for path in [
        "README.md",
        "./README.md",
        "../README.md",
        "handbook//main.tex",
        "handbook\\main.tex",
        " README.md",
        "README.md ",
    ] {
        write(&lock, format!("{raw}{}  {path}\n", "0".repeat(64)));
        assert!(
            books::search(&f.0, "NoPossibleMatch", &SearchOptions::default()).is_err(),
            "{path}"
        );
    }
    write(&lock, raw);
    write(
        &f.book(VOL1).join("handbook/authoring-sources.sha256"),
        "retired\n",
    );
    assert!(books::search(&f.0, "query", &SearchOptions::default()).is_err());
    assert!(books::check_authoring_locks(&f.0, true).is_err());
}

#[test]
fn partial_bundled_corpus_cannot_be_replaced_by_external_root() {
    let f = Fixture::new("missing");
    locked(&f);
    let external = f.0.join("external/cauchyx-ai");
    write(&external.join("case.md"), "ExternalNeedle\n");
    let pde = SearchOptions {
        scope: "pde".into(),
        external_root: Some(external.clone()),
        ..Default::default()
    };
    assert_eq!(
        books::search(&f.0, "ExternalNeedle", &pde).unwrap()[0]["path"],
        "cauchyx-ai/case.md"
    );
    fs::remove_dir_all(f.book(VOL2)).unwrap();
    for scope in ["book", "all", "archive"] {
        assert!(
            books::search(
                &f.0,
                "ExternalNeedle",
                &SearchOptions {
                    scope: scope.into(),
                    ..pde.clone()
                }
            )
            .is_err()
        );
    }
    assert!(
        !books::search(&f.0, "ExternalNeedle", &pde)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn optional_sources_remain_read_only_explicit_and_absence_all_is_valid() {
    let f = Fixture::new("optional");
    locked(&f);
    assert!(
        !books::search(
            &f.0,
            "UniqueBookNeedle",
            &SearchOptions {
                scope: "all".into(),
                external_root: Some(f.0.join("not-present")),
                ..Default::default()
            }
        )
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty()
    );
    assert!(
        books::search(
            &f.0,
            "query",
            &SearchOptions {
                scope: "pde".into(),
                ..Default::default()
            }
        )
        .is_err()
    );
    let cad = f.0.join("case-source");
    write(&cad.join("pyproject.toml"), "synthetic");
    write(&cad.join("src/cad_agent/readme.md"), "CADCaseNeedle\n");
    let result = books::search(
        &f.0,
        "CADCaseNeedle",
        &SearchOptions {
            scope: "cad".into(),
            cad_root: Some(cad),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result[0]["path"], "cad-agent/src/cad_agent/readme.md");
    assert!(result[0].get("source_sha256").is_none());
}

#[test]
fn ranking_deduplicates_nearby_lines_and_preserves_context() {
    let f = Fixture::new("ranking");
    locked(&f);
    write(
        &f.book(VOL1).join("handbook/chapters/ch01.tex"),
        "UniqueRankNeedle\nUniqueRankNeedle\nspace\nspace\nUniqueRankNeedle\nspace\nspace\nUniqueRankNeedle\nspace\nspace\nUniqueRankNeedle\nspace\nspace\nUniqueRankNeedle\n",
    );
    books::check_authoring_locks(&f.0, true).unwrap();
    let result = books::search(&f.0, "UniqueRankNeedle", &SearchOptions::default()).unwrap();
    assert_eq!(result.as_array().unwrap().len(), 4);
    let lines: Vec<_> = result
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![1, 5, 8, 11]);
    assert_eq!(result[1]["context"][0], "3: space");
}

fn rebind_fixture(f: &Fixture) -> PathBuf {
    let checkout = f.0.join("external-semantica");
    let packages = checkout.join("semantica/chapter_packages");
    let mut records = Vec::new();
    let mut mappings = Vec::new();
    for (volume, count) in [("vol1", 9), ("vol2", 20)] {
        for number in 1..=count {
            let ch = format!("ch{number:02}");
            let id = format!("semantica.chapter_packages.{volume}.{ch}");
            let dir = packages.join(volume).join(&ch);
            let source = if volume == "vol1" {
                format!("references/{VOL1}/handbook/chapters/{ch}.tex")
            } else {
                format!("ontology-engineering/references/{VOL2}/{ch}/chapter.md")
            };
            let relative = source
                .strip_prefix("ontology-engineering/")
                .unwrap_or(&source);
            write(&f.0.join(relative), format!("source {volume} {ch} 可信\n"));
            let guide = if volume == "vol1" {
                f.book(VOL1)
                    .join(VOL1_CHAPTERS[number - 1])
                    .join("README.md")
            } else {
                f.0.join(relative).parent().unwrap().join("README.md")
            };
            write(&guide, "chapter guide\n");
            if volume == "vol2" {
                write(
                    &f.book(VOL2)
                        .join(format!("handbook/fragments/readme-{ch}.tex")),
                    "maintained TeX\n",
                );
            }
            json_write(
                &dir.join("contract.yaml"),
                &json!({"external_specification":{"kind":"book","stone":true,"source_anchor":format!("old/{volume}/{ch}.md"),"source_sha256":"a".repeat(64)},"extra_unicode":"保留","extra_number":1e-7}),
            );
            let mut manifest = json!({"assets":[{"asset_id":"chapter-contract","role":"chapter_contract","path":"contract.yaml","sha256":"b".repeat(64),"source_anchor":"historical/contract.yaml"},{"asset_id":"ontology","role":"ontology","path":"ontology.ttl","sha256":"c".repeat(64),"source_anchor":if volume=="vol1"{format!("old/{volume}/{ch}.md")}else{source.clone()}}],"preserved_unknown":{"value":123}});
            if volume == "vol1" {
                manifest["chapter"] = json!({"external_specification":{"source_anchor":format!("old/{volume}/{ch}.md")}});
            } else {
                manifest["book_source"] = json!({"logical_anchor":source,"sha256":"a".repeat(64)});
                mappings.push(json!({"new_package":id,"new_asset_id":"chapter-contract","new_sha256":"b".repeat(64)}));
            }
            json_write(&dir.join("manifest.yaml"), &manifest);
            records.push(json!({"volume":volume,"chapter":ch,"package_id":id,"manifest":format!("{volume}/{ch}/manifest.yaml")}));
        }
    }
    json_write(
        &packages.join("registry.yaml"),
        &json!({"packages":records}),
    );
    json_write(
        &packages.join("vol2/migration-map.json"),
        &json!({"schema_version":"1.0","mappings":mappings}),
    );
    checkout
}

#[test]
fn rebind_exact_29_updates_source_guide_tex_contract_and_migration_and_preserves_unknowns() {
    let f = Fixture::new("rebind");
    let checkout = rebind_fixture(&f);
    let before = snapshot(&checkout);
    let books_before = snapshot(&f.0.join("references"));
    let dry = books::rebind(&f.0, &checkout, false).unwrap();
    assert_eq!(dry["package_count"], 29);
    assert_eq!(dry["changed_count"], 59);
    assert_eq!(dry["error_count"], 0);
    assert_eq!(dry["passed"], false);
    assert_eq!(before, snapshot(&checkout));
    let changed = books::rebind(&f.0, &checkout, true).unwrap();
    assert_eq!(changed["passed"], true);
    assert_eq!(books_before, snapshot(&f.0.join("references")));
    assert_eq!(
        books::rebind(&f.0, &checkout, false).unwrap()["changed_count"],
        0
    );
    let dir = checkout.join("semantica/chapter_packages/vol2/ch01");
    let manifest = read(&dir.join("manifest.yaml"));
    let contract = read(&dir.join("contract.yaml"));
    let migration = read(&checkout.join("semantica/chapter_packages/vol2/migration-map.json"));
    let hash = sha256(fs::read(dir.join("contract.yaml")).unwrap());
    assert_eq!(manifest["assets"][0]["sha256"], hash);
    assert_eq!(migration["mappings"][0]["new_sha256"], hash);
    assert_eq!(contract["external_specification"]["stone"], true);
    assert_eq!(contract["extra_unicode"], "保留");
    assert_eq!(manifest["preserved_unknown"]["value"], 123);
    assert!(
        fs::read_to_string(dir.join("contract.yaml"))
            .unwrap()
            .contains("1e-07")
    );
    assert_eq!(
        contract["book_source"]["tex_sha256"],
        sha256("maintained TeX\n")
    );
    write(
        &f.book(VOL2).join("ch01/chapter.md"),
        "new source revision\n",
    );
    let drift = books::rebind(&f.0, &checkout, false).unwrap();
    assert_eq!(drift["passed"], false);
    assert_eq!(drift["changed_count"], 3);
}

#[test]
fn rebind_late_missing_source_has_all_errors_but_zero_writes() {
    let f = Fixture::new("rebind-missing");
    let checkout = rebind_fixture(&f);
    let before = snapshot(&checkout);
    fs::remove_file(f.book(VOL2).join("handbook/fragments/readme-ch20.tex")).unwrap();
    fs::remove_file(f.book(VOL2).join("ch19/README.md")).unwrap();
    let result = books::rebind(&f.0, &checkout, true).unwrap();
    assert_eq!(result["passed"], false);
    assert_eq!(result["error_count"], 2);
    assert_eq!(result["errors"].as_array().unwrap().len(), 2);
    assert!(result.get("write_recovery").is_none());
    assert_eq!(before, snapshot(&checkout));
}

#[test]
fn rebind_registry_spoofs_and_ambiguous_assets_cannot_write() {
    let f = Fixture::new("rebind-invalid");
    let checkout = rebind_fixture(&f);
    let registry = checkout.join("semantica/chapter_packages/registry.yaml");
    let original = read(&registry);
    for mutation in 0..3 {
        let mut value = original.clone();
        match mutation {
            0 => value["packages"][28] = value["packages"][0].clone(),
            1 => value["packages"][0]["manifest"] = "../outside.yaml".into(),
            _ => value["packages"][0]["package_id"] = "other".into(),
        };
        json_write(&registry, &value);
        let before = snapshot(&checkout);
        assert!(books::rebind(&f.0, &checkout, true).is_err());
        assert_eq!(before, snapshot(&checkout));
    }
    json_write(&registry, &original);
    let manifest = checkout.join("semantica/chapter_packages/vol1/ch01/manifest.yaml");
    let mut value = read(&manifest);
    let duplicate = value["assets"][0].clone();
    value["assets"].as_array_mut().unwrap().push(duplicate);
    json_write(&manifest, &value);
    let before = snapshot(&checkout);
    let report = books::rebind(&f.0, &checkout, true).unwrap();
    assert_eq!(report["passed"], false);
    assert_eq!(report["error_count"], 1);
    assert_eq!(before, snapshot(&checkout));
}

#[test]
fn rebind_rejects_installed_distribution_roots_without_mutation() {
    for location in ["runtime/installed", "arbitrary/site-packages", "pip-target"] {
        let f = Fixture::new("installed-reject");
        let original = rebind_fixture(&f);
        let checkout = f.0.join(location);
        fs::create_dir_all(checkout.parent().unwrap()).unwrap();
        fs::rename(original, &checkout).unwrap();
        if location == "pip-target" {
            fs::create_dir(checkout.join("semantica-0.1.0.dist-info")).unwrap();
        }
        let before = snapshot(&checkout);
        for write in [false, true] {
            let error = books::rebind(&f.0, &checkout, write).unwrap_err();
            assert!(error.to_string().contains("requires_source_checkout"));
        }
        assert_eq!(before, snapshot(&checkout));
    }
}

#[cfg(unix)]
#[test]
fn literal_backslash_cannot_alias_a_source_inventory_path() {
    let f = Fixture::new("backslash");
    locked(&f);
    let lock = f.book(VOL1).join("authoring-sources.sha256");
    let before = fs::read(&lock).unwrap();
    write(
        &f.book(VOL1).join("handbook/figures/a\\b.png"),
        b"literal filename",
    );
    write(
        &f.book(VOL1).join("handbook/figures/a/b.png"),
        b"different file",
    );
    assert!(books::check_authoring_locks(&f.0, true).is_err());
    assert_eq!(before, fs::read(lock).unwrap());
}

#[cfg(unix)]
#[test]
fn symlink_sources_and_lock_destinations_fail_without_following() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new("symlink");
    locked(&f);
    let outside = f.0.join("outside.txt");
    write(&outside, "unrelated\n");
    let guide = f.book(VOL2).join("ch01-introduction/README.md");
    fs::remove_file(&guide).unwrap();
    symlink(&outside, &guide).unwrap();
    assert!(books::search(&f.0, "query", &SearchOptions::default()).is_err());
    assert!(books::check_authoring_locks(&f.0, true).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "unrelated\n");
    fs::remove_file(guide).unwrap();
    write(
        &f.book(VOL2).join("ch01-introduction/README.md"),
        "reader guide\n",
    );
    let lock = f.book(VOL1).join("authoring-sources.sha256");
    fs::remove_file(&lock).unwrap();
    symlink(&outside, &lock).unwrap();
    assert!(books::check_authoring_locks(&f.0, true).is_err());
    assert_eq!(fs::read_to_string(outside).unwrap(), "unrelated\n");
}

#[test]
fn search_relocates_without_original_paths_or_external_book_substitution() {
    let f = Fixture::new("relocation");
    locked(&f);
    let before = books::search(&f.0, "UniqueBookNeedle", &SearchOptions::default()).unwrap();
    let copy = Fixture::new("receiver");
    fn cp(src: &Path, dst: &Path) {
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.path().is_dir() {
                fs::create_dir_all(&target).unwrap();
                cp(&entry.path(), &target);
            } else {
                write(&target, fs::read(entry.path()).unwrap());
            }
        }
    }
    fs::create_dir_all(copy.0.join("references")).unwrap();
    cp(&f.0.join("references"), &copy.0.join("references"));
    fs::rename(f.0.join("references"), f.0.join("producer-hidden")).unwrap();
    let after = books::search(&copy.0, "UniqueBookNeedle", &SearchOptions::default()).unwrap();
    assert_eq!(before, after);
}

#[test]
#[ignore = "Read-only actual two-book authoring tree and explicit retained Semantica checkout"]
fn real_current_books_locks_and_29_bindings_read_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let checkout = root.join("var/maintenance/release-0.7.0/semantica-source");
    let paths = [
        format!("references/{VOL1}/authoring-sources.sha256"),
        format!("references/{VOL2}/handbook/current-source.sha256"),
        format!("references/{VOL2}/handbook/authoring-assets.sha256"),
        format!("references/{VOL2}/handbook/formal-search-guides.sha256"),
    ];
    let before: BTreeMap<_, _> = paths
        .iter()
        .map(|p| (p.clone(), sha256(fs::read(root.join(p)).unwrap())))
        .collect();
    let package_before = snapshot(&checkout.join("semantica/chapter_packages"));
    let locks = books::check_authoring_locks(&root, false).unwrap();
    let hits = books::search(
        &root,
        "工程",
        &SearchOptions {
            limit: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let rebind = books::rebind(&root, &checkout, false).unwrap();
    let after: BTreeMap<_, _> = paths
        .iter()
        .map(|p| (p.clone(), sha256(fs::read(root.join(p)).unwrap())))
        .collect();
    let package_after = snapshot(&checkout.join("semantica/chapter_packages"));
    let output = root.join(format!(
        "var/maintenance/rust-rewrite-20260927/phase9-audit/real-readonly-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    oe_core::create_new_private_dir(&output).unwrap();
    oe_core::write_new(&output.join("report.json"),&json!({"execution_kind":"actual_read_only_bytes","semantic_execution":"not_run","model_calls":0,"writes_to_originals":false,"authoring_locks":locks,"search_hits":hits,"rebind":rebind,"lock_before":before,"lock_after":after,"package_before":package_before,"package_after":package_after})).unwrap();
    println!("readonly evidence: {}", output.display());
    assert_eq!(before, after);
    assert_eq!(package_before, package_after);
    assert_eq!(locks["passed"], true);
    assert_eq!(rebind["passed"], true);
    assert_eq!(rebind["package_count"], 29);
    assert!(!hits.as_array().unwrap().is_empty());
}
