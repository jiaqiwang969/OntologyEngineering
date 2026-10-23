//! Synthetic originals, actual Poppler registration and read-only audit.
//! Native fixture scope: macOS with the explicitly selected Homebrew Poppler.
//! No existing library, OCR, model transport or engineering adoption is involved.
use oe_core::{SkillRoot, sha256};
use oe_knowledge::{
    library_audit,
    poppler::PopplerTools,
    registration::{self, RegistrationOptions},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    path: PathBuf,
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = producer
            .join("var/maintenance/library-audit-tests")
            .join(format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(path.join("sources/books/new")).unwrap();
        fs::create_dir_all(path.join("var/inputs")).unwrap();
        fs::write(path.join("SKILL.md"), "synthetic library audit").unwrap();
        let root = SkillRoot::new(&path).unwrap();
        let f = Self { path, root };
        let a = pdf(&[
            "Synthetic shaft reference has enough native text to exceed eighty visible characters. This is only a software fixture and supplies no engineering acceptance or applicability.",
            "",
        ]);
        f.write("sources/books/a.pdf", &a);
        f.write("sources/books/duplicate.pdf", &a);
        f.write(
            "sources/books/new/b.PDF",
            &pdf(&["A different synthetic reference, intentionally not registered."]),
        );
        f.write("sources/books/notes.txt", b"Private note, not a PDF.");
        f.write(
            "sources/other/outside.pdf",
            &pdf(&["Outside the explicit books scope."]),
        );
        f.register("a", "sources/books/a.pdf", 2, false);
        f
    }
    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn register(&self, id: &str, relative: &str, pages: u64, merge: bool) {
        let original = self.path.join(relative);
        let manifest = self.path.join("var/inputs/registration.json");
        fs::write(&manifest, serde_json::to_vec(&json!({
            "schema":"ontology-engineering.local-pdf-manifest/v1",
            "sources":[{"id":id,"title":format!("Synthetic {id}"),"edition":"fixture-1",
                "path":original,"sha256":sha256(fs::read(&original).unwrap()),
                "chapters":[{"id":"all","title":"Declared navigation","start_page":1,"end_page":pages,"aliases":[]}]}]
        })).unwrap()).unwrap();
        let tools = PopplerTools {
            pdfinfo: "/opt/homebrew/bin/pdfinfo".into(),
            pdftotext: "/opt/homebrew/bin/pdftotext".into(),
            pdftoppm: "/opt/homebrew/bin/pdftoppm".into(),
        };
        registration::register(
            &self.path,
            &manifest,
            &RegistrationOptions {
                tools,
                merge,
                ..Default::default()
            },
        )
        .unwrap();
    }
    fn audit(&self, directory: &str) -> Value {
        let before = self.snapshot();
        let report = library_audit::audit(
            &self.root,
            Path::new(directory),
            Path::new("sources/.indexes/local-pdf"),
        )
        .unwrap();
        assert_eq!(
            self.snapshot(),
            before,
            "audit must preserve every original/index byte and link"
        );
        assert_eq!(report["status"], "audited", "{report:#}");
        report
    }
    fn snapshot(&self) -> BTreeMap<String, String> {
        fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
            for item in fs::read_dir(dir).unwrap() {
                let entry = item.unwrap();
                let path = entry.path();
                let ty = entry.file_type().unwrap();
                let name = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned();
                if ty.is_symlink() {
                    out.insert(
                        name,
                        format!("symlink:{}", fs::read_link(path).unwrap().display()),
                    );
                } else if ty.is_dir() {
                    walk(base, &path, out);
                } else if ty.is_file() {
                    out.insert(name, sha256(fs::read(path).unwrap()));
                } else {
                    out.insert(name, "special-file-unread".to_owned());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.path, &self.path.join("sources"), &mut out);
        out
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}

fn pdf(pages: &[&str]) -> Vec<u8> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        Vec::new(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    let mut children = Vec::new();
    for text in pages {
        let number = objects.len() + 1;
        children.push(format!("{number} 0 R"));
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1000 500] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", number + 1).into_bytes());
        let stream = if text.is_empty() {
            String::new()
        } else {
            format!("BT /F1 6 Tf 20 450 Td ({text}) Tj ET")
        };
        objects.push(
            format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            )
            .into_bytes(),
        );
    }
    objects[1] = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        pages.len(),
        children.join(" ")
    )
    .into_bytes();
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        bytes.extend(object);
        bytes.extend(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        bytes.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

fn file<'a>(report: &'a Value, path: &str) -> &'a Value {
    report["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == path)
        .unwrap_or_else(|| panic!("missing file {path}: {report:#}"))
}
fn source<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == id)
        .unwrap_or_else(|| panic!("missing source {id}: {report:#}"))
}

#[test]
fn library_audit_reports_unregistered_duplicates_and_per_source_native_coverage() {
    let f = Fixture::new();
    let report = f.audit("sources/books");
    assert_eq!(report["index"]["status"], "ready");
    assert_eq!(report["summary"]["scan_status"], "complete");
    assert_eq!(report["summary"]["registered_sources_in_scope"], 1);
    assert_eq!(report["summary"]["pdf_files"], 3);
    assert_eq!(report["summary"]["non_pdf_files"], 1);
    assert_eq!(report["summary"]["unregistered_pdf_files"], 2);
    assert_eq!(
        file(&report, "sources/books/a.pdf")["registration"],
        "registered"
    );
    assert_eq!(
        file(&report, "sources/books/new/b.PDF")["registration"],
        "unregistered"
    );
    assert_eq!(
        file(&report, "sources/books/duplicate.pdf")["registration"],
        "unregistered"
    );
    assert_eq!(file(&report, "sources/books/notes.txt")["kind"], "non_pdf");
    assert_eq!(
        file(&report, "sources/books/notes.txt")["registration"],
        "not_applicable"
    );
    let registered = source(&report, "a");
    assert_eq!(registered["original"]["status"], "verified");
    assert_eq!(registered["coverage"]["status"], "available");
    assert_eq!(registered["coverage"]["indexed_pages"], 2);
    assert_eq!(registered["coverage"]["empty_text_pages"], 1);
    assert_eq!(registered["coverage"]["sparse_text_pages"], 1);
    assert_eq!(registered["coverage"]["nonempty_sparse_text_pages"], 0);
    assert_eq!(report["duplicates"].as_array().unwrap().len(), 1);
    let duplicate = &report["duplicates"][0];
    assert_eq!(
        duplicate["sha256"],
        sha256(fs::read(f.path.join("sources/books/a.pdf")).unwrap())
    );
    let paths = duplicate["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&json!("sources/books/a.pdf")));
    assert!(paths.contains(&json!("sources/books/duplicate.pdf")));
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["format_validation"] == "not_run")
    );
}

#[test]
fn library_audit_distinguishes_missing_and_changed_registered_originals() {
    let f = Fixture::new();
    let original = f.path.join("sources/books/a.pdf");
    let preserved = f.path.join("var/inputs/preserved-a.pdf");
    fs::rename(&original, &preserved).unwrap();
    let missing = f.audit("sources/books");
    assert_eq!(source(&missing, "a")["original"]["status"], "missing");
    assert_eq!(source(&missing, "a")["coverage"]["indexed_pages"], 2);
    let mut altered = fs::read(&preserved).unwrap();
    let byte = altered.iter().position(|b| *b == b'S').unwrap();
    altered[byte] = b'R';
    fs::write(&original, altered).unwrap();
    assert_eq!(
        fs::metadata(&original).unwrap().len(),
        fs::metadata(&preserved).unwrap().len()
    );
    let changed = f.audit("sources/books");
    assert_eq!(source(&changed, "a")["original"]["status"], "changed");
    assert_eq!(
        file(&changed, "sources/books/a.pdf")["registration"],
        "registered"
    );
    assert_eq!(
        file(&changed, "sources/books/a.pdf")["original"]["status"],
        "changed"
    );
}

#[test]
fn library_audit_missing_or_invalid_index_preserves_unknown_registration() {
    for invalid in [false, true] {
        let f = Fixture::new();
        if invalid {
            fs::write(
                f.path.join("sources/.indexes/local-pdf/catalog.sqlite3"),
                b"not an index",
            )
            .unwrap();
        } else {
            fs::rename(
                f.path.join("sources/.indexes/local-pdf"),
                f.path.join("var/inputs/preserved-index"),
            )
            .unwrap();
        }
        let report = f.audit("sources/books");
        assert_eq!(report["index"]["status"], "unavailable");
        assert_eq!(report["summary"]["unregistered_pdf_files"], 0);
        assert_eq!(report["summary"]["unknown_registration_pdf_files"], 3);
        for name in [
            "sources/books/a.pdf",
            "sources/books/new/b.PDF",
            "sources/books/duplicate.pdf",
        ] {
            assert_eq!(file(&report, name)["registration"], "unknown", "{report:#}");
        }
        assert!(
            report["sources"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["coverage"]["status"] != "available")
        );
    }
}

#[test]
fn library_audit_explicit_scope_does_not_inspect_outside_registered_originals() {
    let f = Fixture::new();
    f.register("outside", "sources/other/outside.pdf", 1, true);
    fs::rename(
        f.path.join("sources/other/outside.pdf"),
        f.path.join("var/inputs/preserved-outside.pdf"),
    )
    .unwrap();
    let report = f.audit("sources/books");
    assert_eq!(report["index"]["status"], "ready");
    assert_eq!(report["summary"]["index_registered_sources"], 2);
    assert_eq!(report["summary"]["registered_sources_in_scope"], 1);
    assert_eq!(report["summary"]["outside_scope_registered_sources"], 1);
    assert_eq!(report["sources"].as_array().unwrap().len(), 1);
    assert_eq!(report["summary"]["scan_status"], "complete");
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["path"].as_str().unwrap().starts_with("sources/books/"))
    );
    let narrow = f.audit("sources/books/new");
    assert_eq!(narrow["summary"]["registered_sources_in_scope"], 0);
    assert_eq!(narrow["summary"]["outside_scope_registered_sources"], 2);
    assert_eq!(narrow["files"].as_array().unwrap().len(), 1);
    assert_eq!(
        file(&narrow, "sources/books/new/b.PDF")["registration"],
        "unregistered"
    );
}

#[test]
#[cfg(unix)]
fn library_audit_does_not_follow_symlinked_files_or_directories() {
    let f = Fixture::new();
    std::os::unix::fs::symlink(
        f.path.join("sources/other/outside.pdf"),
        f.path.join("sources/books/linked.pdf"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        f.path.join("sources/other"),
        f.path.join("sources/books/linked-dir"),
    )
    .unwrap();
    let report = f.audit("sources/books");
    assert_eq!(report["summary"]["scan_status"], "partial");
    assert!(!report["summary"]["issues"].as_array().unwrap().is_empty());
    for path in ["sources/books/linked.pdf", "sources/books/linked-dir"] {
        assert_eq!(file(&report, path)["kind"], "symlink");
        assert_ne!(file(&report, path)["original"]["status"], "verified");
    }
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["path"] != "sources/books/linked-dir/outside.pdf")
    );
}

#[test]
fn library_audit_bounded_scan_does_not_claim_complete_coverage() {
    let f = Fixture::new();
    let deep = format!(
        "sources/books/{}/deep.pdf",
        (0..10)
            .map(|i| format!("level{i}"))
            .collect::<Vec<_>>()
            .join("/")
    );
    f.write(
        &deep,
        &pdf(&["Intentionally beyond the fixed traversal depth."]),
    );
    let report = f.audit("sources/books");
    assert_eq!(report["summary"]["scan_status"], "partial");
    assert!(!report["summary"]["issues"].as_array().unwrap().is_empty());
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["path"] != deep)
    );
    assert_eq!(source(&report, "a")["original"]["status"], "verified");
}
