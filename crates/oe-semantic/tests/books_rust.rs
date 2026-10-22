//! Actual current Rust execution, using the locked data wheel and authorized
//! synthetic extracts. No Python runtime, NativeRuntime fixture, or client data.
use oe_core::{SkillRoot, create_new_private_dir, read_regular_bytes, sha256, write_new_bytes};
use oe_semantic::books::{self, EngraveOptions};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const PART1: &str =
    "part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json";
const PART3: &str =
    "part-03-concept-phase/native-full/ISO 26262-3-2018/auto/ISO 26262-3-2018_content_list_v2.json";
fn installed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_owned()
}
fn save(path: &Path, bytes: &[u8]) {
    oe_core::ensure_directory(path.parent().unwrap()).unwrap();
    write_new_bytes(path, bytes).unwrap();
}
struct Case {
    root: SkillRoot,
}
impl Case {
    fn new() -> Self {
        let root = installed().join(format!(
            "var/maintenance/books-rust-tests/{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        create_new_private_dir(&root).unwrap();
        fs::write(root.join("SKILL.md"), "Synthetic Rust book test").unwrap();
        for &(relative, bytes) in books::rust::source_execution::IMPLEMENTATION_SOURCES {
            let target = root.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        let archive = books::rust::ARCHIVE;
        let raw = read_regular_bytes(&installed().join(archive), 32 * 1024 * 1024).unwrap();
        save(&root.join(archive), &raw);
        save(&root.join("sources/continuity.json"),&serde_json::to_vec(&json!({"source_files":[{"path":archive.trim_start_matches("sources/"),"sha256":sha256(&raw)}]})).unwrap());
        create_new_private_dir(&root.join("var/project/source")).unwrap();
        create_new_private_dir(&root.join("var/project/book")).unwrap();
        Self {
            root: SkillRoot::new(root).unwrap(),
        }
    }
    fn path(&self, p: &str) -> PathBuf {
        self.root.path().join(p)
    }
    fn options(&self) -> EngraveOptions {
        save(&self.path(&format!("var/project/source/{PART1}")),&serde_json::to_vec(&json!([[{"content":{"paragraphs":[{"content":"3.1 RESTRICTED-STANDARD-WORDING-MUST-NOT-LEAVE-SOURCE"}]}}]])).unwrap());
        save(&self.path(&format!("var/project/source/{PART3}")),&serde_json::to_vec(&json!([[{"content":{"paragraphs":[{"content":"5.4.1 RESTRICTED-STANDARD-WORDING-MUST-NOT-LEAVE-SOURCE the item definition shall be available"}]}},{"content":{"paragraphs":[{"content":"8.1 RESTRICTED-STANDARD-WORDING-MUST-NOT-LEAVE-SOURCE structural heading"}]}}]])).unwrap());
        save(
            &self.path("var/project/book/appendices/appendix-c-glossary.md"),
            "- **测试术语（test term）**（1-3.1；ch04）——这是作者自己的公开转述。\n".as_bytes(),
        );
        EngraveOptions {
            source_root: "var/project/source".into(),
            book_root: "var/project/book".into(),
            output: "var/engraved".into(),
            parts: vec![1, 3],
            overwrite: false,
        }
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.root.path());
    }
}
#[test]
fn real_registered_maps_resolution_and_full_asset_text_work_without_legacy_runtime() {
    let c = Case::new();
    for volume in ["vol1", "vol2"] {
        let report = books::read_migration_map(&c.root, volume).unwrap();
        assert_eq!(report["status"], "completed");
        assert_eq!(report["engine"], "semantica.rust-runtime");
        let map = &report["result"];
        assert_eq!(
            map["entry_count"].as_u64().unwrap() as usize,
            map["entries"].as_array().unwrap().len()
        );
        assert_eq!(
            map["retired_runtime_entry_count"].as_u64().unwrap() as usize,
            map["retired_runtime_entries"].as_array().unwrap().len()
        );
        let first = &map["entries"][0];
        let resolved =
            books::resolve_successor(&c.root, first["old_path"].as_str().unwrap(), Some(volume))
                .unwrap();
        assert_eq!(resolved["status"], "completed");
        assert_eq!(resolved["result"]["candidates"][0], *first);
        let full = books::asset_text(
            &c.root,
            first["package_id"].as_str().unwrap(),
            first["asset_id"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(full["status"], "completed");
        assert_eq!(
            sha256(full["result"]["text"].as_str().unwrap().as_bytes()),
            first["successor_sha256"]
        );
    }
    assert_eq!(
        books::resolve_successor(&c.root, "synthetic-missing.md", None).unwrap()["result"]["resolved"],
        false
    );
    let chapters = books::rust::ArchiveProvider::open(c.root.path())
        .unwrap()
        .catalog()
        .unwrap();
    assert_eq!(chapters.list_chapter_packages(None).len(), 29);
    assert_eq!(chapters.list_domain_packages().len(), 1);
    assert_eq!(
        books::rust::validate_chapter_registry(c.root.path(), true).unwrap()["valid"],
        true
    );
}
#[test]
fn actual_book_binding_checks_retain_full_missing_source_set() {
    let c = Case::new();
    let report = books::verify_book_sources(&c.root).unwrap();
    assert_eq!(report["status"], "blocked");
    let result = &report["result"];
    assert_eq!(result["check_count"], 238);
    assert_eq!(result["checks"].as_array().unwrap().len(), 238);
    assert_eq!(result["passed"], false);
    assert!(
        result["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["check_id"] == "book.contract.tex-binding")
    );
    assert!(
        result["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["check_id"] == "book.asset-source.ontology"
                && c["package_id"] == "semantica.chapter_packages.vol2.ch01")
    );
}
#[test]
fn actual_normative_seeds_engrave_and_every_output_is_independently_read_back() {
    let c = Case::new();
    let options = c.options();
    let report = books::engrave(&c.root, &options).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["fact_admitted"], false);
    assert_eq!(
        report["result"]["parts"],
        json!([{"part":1,"unit_count":1,"glossed_count":1,"pending_count":0},{"part":3,"unit_count":2,"glossed_count":1,"pending_count":1}])
    );
    for artifact in report["result"]["artifacts"].as_array().unwrap() {
        let path = c
            .path("var/engraved")
            .join(artifact["relative_path"].as_str().unwrap());
        let raw = fs::read(&path).unwrap();
        assert_eq!(artifact["sha256"], sha256(&raw));
        assert_eq!(artifact["size_bytes"].as_u64(), Some(raw.len() as u64));
        assert!(
            !String::from_utf8_lossy(&raw)
                .contains("RESTRICTED-STANDARD-WORDING-MUST-NOT-LEAVE-SOURCE")
        );
    }
    let ttl = fs::read_to_string(c.path("var/engraved/part3-concept-phase.ttl")).unwrap();
    assert!(ttl.contains("isoN:modality isoN:Shall"));
    assert!(ttl.contains("相关项的需求"));
    assert!(books::engrave(&c.root, &options).is_err());
    fs::write(c.path("var/engraved/keep.txt"), "keep unrelated").unwrap();
    let mut options = options;
    options.overwrite = true;
    let again = books::engrave(&c.root, &options).unwrap();
    assert_eq!(
        again["result"]["bundle_sha256"],
        report["result"]["bundle_sha256"]
    );
    assert_eq!(
        fs::read_to_string(c.path("var/engraved/keep.txt")).unwrap(),
        "keep unrelated"
    );
}
#[test]
fn missing_source_and_overlapping_output_never_create_or_replace_artifacts() {
    let c = Case::new();
    let mut options = c.options();
    fs::remove_file(c.path(&format!("var/project/source/{PART3}"))).unwrap();
    assert!(books::engrave(&c.root, &options).is_err());
    assert!(!c.path("var/engraved").exists());
    create_new_private_dir(&c.path("var/engraved")).unwrap();
    fs::write(c.path("var/engraved/part1-cards.md"), "previous intact").unwrap();
    options.overwrite = true;
    assert!(books::engrave(&c.root, &options).is_err());
    assert_eq!(
        fs::read_to_string(c.path("var/engraved/part1-cards.md")).unwrap(),
        "previous intact"
    );
    options.output = "var/project/book/output".into();
    assert!(books::engrave(&c.root, &options).is_err());
    assert!(!c.path("var/project/book/output").exists());
}
#[cfg(unix)]
#[test]
fn output_symlink_is_refused_before_any_generated_file_is_replaced() {
    let c = Case::new();
    let mut options = c.options();
    create_new_private_dir(&c.path("var/engraved")).unwrap();
    fs::write(c.path("var/protected.txt"), "protected").unwrap();
    std::os::unix::fs::symlink(
        c.path("var/protected.txt"),
        c.path("var/engraved/normative-tbox.ttl"),
    )
    .unwrap();
    options.overwrite = true;
    assert!(books::engrave(&c.root, &options).is_err());
    assert_eq!(
        fs::read_to_string(c.path("var/protected.txt")).unwrap(),
        "protected"
    );
    assert!(!c.path("var/engraved/engraving-report.json").exists());
}

#[test]
fn archive_bytes_are_rechecked_after_using_the_frozen_catalog() {
    use std::io::Write;
    let c = Case::new();
    let provider = books::rust::ArchiveProvider::open(c.root.path()).unwrap();
    let catalog = provider.catalog().unwrap();
    assert_eq!(catalog.list_chapter_packages(None).len(), 29);
    std::fs::OpenOptions::new()
        .append(true)
        .open(c.path(books::rust::ARCHIVE))
        .unwrap()
        .write_all(b"changed after verified load")
        .unwrap();
    assert!(provider.verify().is_err());
}

#[test]
fn matching_self_declared_continuity_cannot_replace_the_approved_book_archive() {
    let c = Case::new();
    let path = c.path(books::rust::ARCHIVE);
    let mut bytes = fs::read(&path).unwrap();
    // Modify a valid EOCD comment only. Every package manifest and asset has
    // exactly its original bytes; malformed ZIP is not the expected refusal.
    let end = bytes.windows(4).rposition(|w| w == b"PK\x05\x06").unwrap();
    let comment_length = u16::from_le_bytes([bytes[end + 20], bytes[end + 21]]) as usize;
    assert_eq!(end + 22 + comment_length, bytes.len());
    let comment = b"different valid archive identity";
    bytes[end + 20..end + 22].copy_from_slice(&(comment.len() as u16).to_le_bytes());
    bytes.truncate(end + 22);
    bytes.extend_from_slice(comment);
    zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    fs::write(&path, &bytes).unwrap();
    let continuity = json!({"source_files":[{"path":books::rust::ARCHIVE.trim_start_matches("sources/"),"sha256":sha256(&bytes)}]});
    fs::write(
        c.path("sources/continuity.json"),
        serde_json::to_vec(&continuity).unwrap(),
    )
    .unwrap();
    assert!(
        books::rust::archive_bytes(c.root.path()).is_err(),
        "matching self-declared SHA must not authorize a different historical source release"
    );
}
