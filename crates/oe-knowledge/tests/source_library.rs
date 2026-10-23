use oe_core::{SkillRoot, sha256};
use oe_knowledge::{
    poppler::PopplerTools, registration::RegistrationOptions, source_library, sources::Provider,
};
use serde_json::{Value, json};
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
    fn entries(&self, entries: Vec<Value>) {
        fs::write(
            self.0.join("runtime/source-delivery.json"),
            serde_json::to_vec(&json!({
                "schema":"ontology-engineering.source-delivery/v1", "files":entries
            }))
            .unwrap(),
        )
        .unwrap();
    }
    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
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

fn entry(path: &str, id: &str, bytes: &[u8]) -> Value {
    json!({
        "relative_path":path, "title":id, "bytes":bytes.len(), "sha256":sha256(bytes),
        "drive":{"file_id":format!("fixture_drive_{id}_0001")},
        "registration":{"id":id,"edition":"1","chapters":[]}
    })
}

#[test]
fn same_basename_nested_import_and_existing_fetch_are_unambiguous() {
    let f = Fixture::new();
    let originals = [
        (
            "books/a/original.pdf",
            "book-a",
            b"first original".as_slice(),
        ),
        (
            "books/b/original.pdf",
            "book-b",
            b"second original".as_slice(),
        ),
    ];
    f.entries(
        originals
            .iter()
            .map(|(path, id, bytes)| entry(path, id, bytes))
            .collect(),
    );
    for (path, _, bytes) in originals {
        f.write(&format!("incoming/{path}"), bytes);
    }
    let root = SkillRoot::new(&f.0).unwrap();
    let report = source_library::import(
        &root,
        Path::new("runtime/source-delivery.json"),
        None,
        &f.0.join("incoming"),
    )
    .unwrap();
    assert_eq!(report["files"].as_array().unwrap().len(), 2);
    for (path, _, bytes) in originals {
        assert_eq!(fs::read(f.0.join("sources").join(path)).unwrap(), bytes);
        assert_eq!(fs::read(f.0.join("incoming").join(path)).unwrap(), bytes);
    }
    // Reusing verified originals must never require a downloader, even when
    // their filenames coincide.
    let report = source_library::fetch(
        &root,
        Path::new("runtime/source-delivery.json"),
        "books",
        &source_library::FetchOptions {
            executable: f.0.join("must-not-run"),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|file| file["status"] == "already_verified")
    );
}

#[test]
fn mixed_nested_and_flat_same_basename_uses_distinct_inputs() {
    let f = Fixture::new();
    f.entries(vec![
        entry("books/a/original.pdf", "a", b"nested"),
        entry("books/b/original.pdf", "b", b"flat"),
    ]);
    f.write("incoming/books/a/original.pdf", b"nested");
    f.write("incoming/original.pdf", b"flat");
    source_library::import(
        &SkillRoot::new(&f.0).unwrap(),
        Path::new("runtime/source-delivery.json"),
        None,
        &f.0.join("incoming"),
    )
    .unwrap();
    assert_eq!(
        fs::read(f.0.join("sources/books/a/original.pdf")).unwrap(),
        b"nested"
    );
    assert_eq!(
        fs::read(f.0.join("sources/books/b/original.pdf")).unwrap(),
        b"flat"
    );
}

#[test]
fn shared_flat_input_is_rejected_before_any_original_is_published() {
    for first in ["books/a/original.pdf", "original.pdf"] {
        let f = Fixture::new();
        f.entries(vec![
            entry("books/independent.pdf", "independent", b"independent"),
            entry(first, "a", b"shared"),
            entry("books/b/original.pdf", "b", b"shared"),
        ]);
        f.write("incoming/independent.pdf", b"independent");
        f.write("incoming/original.pdf", b"shared");
        let error = source_library::import(
            &SkillRoot::new(&f.0).unwrap(),
            Path::new("runtime/source-delivery.json"),
            None,
            &f.0.join("incoming"),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("source_library_ambiguous_download_names")
        );
        assert!(!f.0.join("sources/books/independent.pdf").exists());
        assert!(!f.0.join("sources").join(first).exists());
        assert!(!f.0.join("sources/books/b/original.pdf").exists());
    }
}

#[cfg(unix)]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn script(f: &Fixture, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = f.0.join(name);
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
#[cfg(unix)]
fn same_basename_fetch_uses_distinct_drive_ids() {
    let f = Fixture::new();
    f.entries(vec![
        entry("books/a/original.pdf", "a", b"first"),
        entry("books/b/original.pdf", "b", b"second"),
    ]);
    f.write("incoming/a", b"first");
    f.write("incoming/b", b"second");
    let executable = script(
        &f,
        "fake-gws",
        &format!(
            "case \"$5\" in\n  *fixture_drive_a_0001*) /bin/cp {} \"$7\" ;;\n  *fixture_drive_b_0001*) /bin/cp {} \"$7\" ;;\n  *) exit 9 ;;\nesac",
            shell_quote(&f.0.join("incoming/a")),
            shell_quote(&f.0.join("incoming/b")),
        ),
    );
    source_library::fetch(
        &SkillRoot::new(&f.0).unwrap(),
        Path::new("runtime/source-delivery.json"),
        "books",
        &source_library::FetchOptions {
            executable,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        fs::read(f.0.join("sources/books/a/original.pdf")).unwrap(),
        b"first"
    );
    assert_eq!(
        fs::read(f.0.join("sources/books/b/original.pdf")).unwrap(),
        b"second"
    );
}

// Native one-page fixture; registration is exercised through the actual
// Poppler reader, without a user's PDFs or indexes.
fn pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 10 Tf 20 350 Td ({text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Count 1 /Kids [4 0 R] >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
    ];
    let mut result = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(result.len());
        result.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = result.len();
    result.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        result.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    result.extend(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    result
}

fn native_tools() -> PopplerTools {
    PopplerTools {
        pdfinfo: "/opt/homebrew/bin/pdfinfo".into(),
        pdftotext: "/opt/homebrew/bin/pdftotext".into(),
        pdftoppm: "/opt/homebrew/bin/pdftoppm".into(),
    }
}

fn registered_snapshot(f: &Fixture) -> (Vec<u8>, Vec<u8>) {
    let index = f.0.join("sources/.indexes/local-pdf");
    (
        fs::read(index.join("registration.json")).unwrap(),
        fs::read(index.join("catalog.sqlite3")).unwrap(),
    )
}

#[test]
fn delivery_selected_merge_preserves_previous_books_and_preview_is_read_only() {
    let f = Fixture::new();
    let a = pdf("First book about bearings.");
    let b = pdf("Second book about shafts.");
    f.entries(vec![
        entry("books/a.pdf", "a", &a),
        entry("books/b.pdf", "b", &b),
    ]);
    f.write("sources/books/a.pdf", &a);
    f.write("sources/books/b.pdf", &b);
    let root = SkillRoot::new(&f.0).unwrap();
    source_library::register(
        &root,
        Path::new("runtime/source-delivery.json"),
        Some("a"),
        false,
        native_tools(),
    )
    .unwrap();
    let previous = registered_snapshot(&f);
    let mut options = RegistrationOptions {
        tools: native_tools(),
        merge: true,
        dry_run: true,
        ..Default::default()
    };
    let preview = source_library::register_with_options(
        &root,
        Path::new("runtime/source-delivery.json"),
        Some("b"),
        &options,
    )
    .unwrap();
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["update"]["final_source_count"], 2);
    assert_eq!(registered_snapshot(&f), previous);
    options.dry_run = false;
    let merged = source_library::register_with_options(
        &root,
        Path::new("runtime/source-delivery.json"),
        Some("b"),
        &options,
    )
    .unwrap();
    assert_eq!(merged["status"], "registered");
    let provider = Provider::open(&root, "local_pdf", None).unwrap();
    assert!(provider.page("local_pdf:a:page:1").is_ok());
    assert!(provider.page("local_pdf:b:page:1").is_ok());
    let merged_snapshot = registered_snapshot(&f);
    let unchanged = source_library::register_with_options(
        &root,
        Path::new("runtime/source-delivery.json"),
        Some("b"),
        &options,
    )
    .unwrap();
    assert_eq!(unchanged["status"], "unchanged");
    assert_eq!(registered_snapshot(&f), merged_snapshot);
    assert_eq!(fs::read(f.0.join("sources/books/a.pdf")).unwrap(), a);
    assert_eq!(fs::read(f.0.join("sources/books/b.pdf")).unwrap(), b);
}

#[test]
#[cfg(unix)]
fn delivery_change_during_extraction_blocks_new_replace_and_merge_publication() {
    for mode in ["new", "replace", "merge"] {
        let f = Fixture::new();
        let root = SkillRoot::new(&f.0).unwrap();
        let old = pdf("Old book source.");
        let new = pdf("New distinct book source.");
        f.write("sources/books/old.pdf", &old);
        f.write("sources/books/new.pdf", &new);
        let previous = if mode != "new" {
            f.entries(vec![entry("books/old.pdf", "old", &old)]);
            source_library::register(
                &root,
                Path::new("runtime/source-delivery.json"),
                None,
                false,
                native_tools(),
            )
            .unwrap();
            Some(registered_snapshot(&f))
        } else {
            None
        };
        f.entries(vec![entry("books/new.pdf", "new", &new)]);
        let changed_reader = script(
            &f,
            "changing-pdftotext",
            &format!(
                "if [ \"$1\" != '-v' ]; then printf '\\n' >> {}; fi\nexec {} \"$@\"",
                shell_quote(&f.0.join("runtime/source-delivery.json")),
                shell_quote(&native_tools().pdftotext),
            ),
        );
        let tools = PopplerTools {
            pdftotext: changed_reader,
            ..native_tools()
        };
        let result = source_library::register_with_options(
            &root,
            Path::new("runtime/source-delivery.json"),
            None,
            &RegistrationOptions {
                replace: mode == "replace",
                merge: mode == "merge",
                tools,
                ..Default::default()
            },
        );
        assert!(
            format!("{:#}", result.unwrap_err()).contains("source_library_manifest_changed"),
            "mode {mode}"
        );
        if let Some(previous) = previous {
            assert_eq!(registered_snapshot(&f), previous, "mode {mode}");
        } else {
            assert!(!f.0.join("sources/.indexes/local-pdf").exists());
        }
        assert_eq!(fs::read(f.0.join("sources/books/old.pdf")).unwrap(), old);
        assert_eq!(fs::read(f.0.join("sources/books/new.pdf")).unwrap(), new);
    }
}
