use oe_semantic::books::{
    mcp::RegisteredBookCatalog,
    rust::{APPROVED_ARCHIVE_SHA256, ARCHIVE},
};
use semantica_contracts::{SemanticBookCatalogPort, SemanticBookCatalogRequest};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn original_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}
fn copy_registered_book_source(root: &Path) {
    for id in [ARCHIVE, "sources/continuity.json"] {
        let path = root.join(id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(original_root().join(id), path).unwrap();
    }
}
fn request() -> SemanticBookCatalogRequest {
    SemanticBookCatalogRequest {
        schema: "semantica.book-catalog-request/1".into(),
        operation: "list".into(),
        arguments: json!({}),
    }
}

#[test]
fn registered_archive_returns_original_catalog_then_refuses_post_open_drift() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("books");
    copy_registered_book_source(&root);
    let catalog = RegisteredBookCatalog::open(&root).unwrap();
    let result = catalog.read(&request()).unwrap();
    assert_eq!(result.source_sha256, APPROVED_ARCHIVE_SHA256);
    assert_eq!(result.data["chapter_package_count"], 29);
    assert_eq!(result.data["domain_package_count"], 1);
    assert!(
        result.data["packages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["release_status"] == "blocked")
    );
    let path = root.join(ARCHIVE);
    let mut bytes = fs::read(&path).unwrap();
    bytes[20] ^= 1;
    fs::write(&path, bytes).unwrap();
    let failure = catalog.read(&request()).unwrap_err();
    assert_eq!(failure.code, "book_catalog_source_unavailable");
    assert!(!failure.message.contains(root.to_str().unwrap()));
}

#[test]
fn registration_drift_is_checked_again_on_each_catalog_read() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("books");
    copy_registered_book_source(&root);
    let catalog = RegisteredBookCatalog::open(&root).unwrap();
    let path = root.join("sources/continuity.json");
    let mut doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    doc["source_files"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v["path"] != ARCHIVE.trim_start_matches("sources/"));
    fs::write(path, serde_json::to_vec(&doc).unwrap()).unwrap();
    assert_eq!(
        catalog.read(&request()).unwrap_err().code,
        "book_catalog_source_unavailable"
    );
}
