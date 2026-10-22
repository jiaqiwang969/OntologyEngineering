use oe_core::{SkillRoot, sha256};
use oe_knowledge::source_library;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("var/maintenance/rust-rewrite-20260927/source-delivery-tests")
            .join(format!(
                "oe-source-delivery-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(path.join("runtime")).unwrap();
        fs::create_dir_all(path.join("incoming")).unwrap();
        fs::write(path.join("SKILL.md"), "fixture").unwrap();
        Self(path)
    }
    fn manifest(&self, bytes: &[u8]) {
        let entry = json!({
            "relative_path":"books/fixture.pdf", "title":"Fixture", "bytes":bytes.len(),
            "sha256":sha256(bytes), "drive":{"file_id":"fixture_drive_id_0001","view_url":"https://drive.invalid/view"},
            "registration":{"id":"fixture-book","edition":"1","chapters":[]}
        });
        fs::write(
            self.0.join("runtime/source-delivery.json"),
            serde_json::to_vec(&json!({
                "schema":"ontology-engineering.source-delivery/v1", "files":[entry]
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn list_and_import_are_hash_bound_and_offline() {
    let f = Fixture::new();
    let bytes = b"fixture source bytes";
    f.manifest(bytes);
    fs::write(f.0.join("incoming/fixture.pdf"), bytes).unwrap();
    let root = SkillRoot::new(&f.0).unwrap();
    let listed = source_library::list(
        &root,
        Path::new("runtime/source-delivery.json"),
        None,
        false,
    )
    .unwrap();
    assert_eq!(listed["status"], "listed");
    let placed = source_library::import(
        &root,
        Path::new("runtime/source-delivery.json"),
        None,
        &f.0.join("incoming"),
    )
    .unwrap();
    assert_eq!(placed["status"], "placed");
    let verified =
        source_library::list(&root, Path::new("runtime/source-delivery.json"), None, true).unwrap();
    assert_eq!(verified["status"], "verified");
    assert_eq!(
        fs::read(f.0.join("sources/books/fixture.pdf")).unwrap(),
        bytes
    );
}

#[test]
fn existing_mismatch_and_escaped_paths_fail_closed() {
    let f = Fixture::new();
    let bytes = b"fixture source bytes";
    f.manifest(bytes);
    let root = SkillRoot::new(&f.0).unwrap();
    fs::create_dir_all(f.0.join("sources/books")).unwrap();
    fs::write(f.0.join("sources/books/fixture.pdf"), b"changed").unwrap();
    let error = source_library::import(
        &root,
        Path::new("runtime/source-delivery.json"),
        None,
        &f.0.join("incoming"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("existing_file_mismatch"));
    let escaped = json!({"schema":"ontology-engineering.source-delivery/v1","files":[{"relative_path":"../outside.pdf","bytes":0,"sha256":sha256(b""),"drive":{},"registration":{}}]});
    fs::write(
        f.0.join("runtime/source-delivery.json"),
        serde_json::to_vec(&escaped).unwrap(),
    )
    .unwrap();
    let error = source_library::list(
        &root,
        Path::new("runtime/source-delivery.json"),
        None,
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("invalid_relative_path"));
}

#[test]
fn unknown_selection_is_rejected() {
    let f = Fixture::new();
    f.manifest(b"bytes");
    let root = SkillRoot::new(&f.0).unwrap();
    let error = source_library::list(
        &root,
        Path::new("runtime/source-delivery.json"),
        Some("missing"),
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown_selection"));
}
