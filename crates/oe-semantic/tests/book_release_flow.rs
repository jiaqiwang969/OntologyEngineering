//! Complete production orchestration in isolated roots. Cargo, pytest and PDF
//! tools are explicit fakes; native runtime is absent and its verdict stays
//! blocked. No upstream suite, book publication or semantic acceptance is claimed.
#![cfg(unix)]
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use oe_core::{SkillRoot, sha256};
use oe_semantic::{
    book_release as release,
    book_sources::{self as books, VOL1, VOL1_CHAPTERS, VOL2, VOL2_CHAPTERS},
    release_evidence as evidence,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use zip::write::SimpleFileOptions;
static NEXT: AtomicU64 = AtomicU64::new(0);
const DIST: &str = "semantica-9.9.9+fixture.dist-info";
const WHEEL: &str = "semantica-9.9.9+fixture-py3-none-any.whl";
const OLD: &str = "references/book-release-artifacts.json";
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn document(path: &Path, v: &Value) {
    write(path, release::canonical_bytes(v).unwrap());
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn script(path: &Path, text: &str) {
    write(path, text);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn git(path: &Path, args: &[&str]) -> String {
    let r = Command::new("git")
        .args(args)
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Flow fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Flow fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    String::from_utf8(r.stdout).unwrap().trim().into()
}
fn wheel() -> Vec<u8> {
    let mut files=BTreeMap::from([("semantica/__init__.py".into(),b"__version__='9.9.9+fixture'\n".to_vec()),(format!("{DIST}/METADATA"),b"Metadata-Version: 2.4\nName: semantica\nVersion: 9.9.9+fixture\nRequires-Python: >=3.10\n".to_vec()),(format!("{DIST}/WHEEL"),b"Wheel-Version: 1.0\nTag: py3-none-any\n".to_vec())]);
    for (volume, count) in [("vol1", 9), ("vol2", 20)] {
        for n in 1..=count {
            let ch = format!("ch{n:02}");
            let prefix = format!("semantica/chapter_packages/{volume}/{ch}");
            let asset = format!("Synthetic source for {volume}/{ch}\n").into_bytes();
            files.insert(format!("{prefix}/a.ttl"), asset.clone());
            files.insert(format!("{prefix}/manifest.yaml"),serde_json::to_vec(&json!({"package_id":format!("semantica.chapter_packages.{volume}.{ch}"),"version":"fixture.1","status":"partial","release_status":"blocked","chapter":{"volume":volume,"chapter":ch},"assets":[{"asset_id":"principle-source","role":"ontology_source","path":"a.ttl","sha256":sha256(asset)}]})).unwrap());
        }
    }
    let record = format!("{DIST}/RECORD");
    let mut text = String::new();
    for (n, b) in &files {
        text += &format!(
            "{n},sha256={},{}\n",
            URL_SAFE_NO_PAD.encode(Sha256::digest(b)),
            b.len()
        );
    }
    text += &format!("{record},,\n");
    files.insert(record, text.into_bytes());
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, b) in files {
        z.start_file(
            n,
            SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644),
        )
        .unwrap();
        z.write_all(&b).unwrap();
    }
    z.finish().unwrap().into_inner()
}
struct Case {
    path: PathBuf,
    root: SkillRoot,
    upstream: PathBuf,
    commit: String,
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}
impl Case {
    fn new() -> Self {
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = producer.join(format!(
            "var/maintenance/rust-tests/book-release-flow-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        corpus(&path);
        write(&path.join(".gitignore"), b"var/\n");
        write(&path.join("Cargo.toml"), b"[workspace]\n");
        write(&path.join("Cargo.lock"), b"version = 4\n");
        for (relative, bytes) in [
            (
                "crates/oe-semantic/src/release_evidence.rs",
                include_bytes!("../src/release_evidence.rs").as_slice(),
            ),
            (
                "crates/oe-semantic/src/book_sources.rs",
                include_bytes!("../src/book_sources.rs").as_slice(),
            ),
            (
                "crates/oe-semantic/src/books.rs",
                include_bytes!("../src/books.rs").as_slice(),
            ),
            (
                "crates/oe-semantic/src/native.rs",
                include_bytes!("../src/native.rs").as_slice(),
            ),
            (
                "crates/oe-semantic/src/runtime.rs",
                include_bytes!("../src/runtime.rs").as_slice(),
            ),
            (
                "crates/oe-semantic/src/release_sources.rs",
                include_bytes!("../src/release_sources.rs").as_slice(),
            ),
            (
                "crates/oe-semantic/src/book_release.rs",
                include_bytes!("../src/book_release.rs").as_slice(),
            ),
            (
                "crates/oe-core/src/process.rs",
                include_bytes!("../../oe-core/src/process.rs").as_slice(),
            ),
            (
                "crates/oe-core/src/privacy.rs",
                include_bytes!("../../oe-core/src/privacy.rs").as_slice(),
            ),
            (
                "crates/oe-core/src/json.rs",
                include_bytes!("../../oe-core/src/json.rs").as_slice(),
            ),
            (
                "crates/oe-wire/Cargo.toml",
                include_bytes!("../../oe-wire/Cargo.toml").as_slice(),
            ),
            (
                "crates/oe-wire/src/lib.rs",
                include_bytes!("../../oe-wire/src/lib.rs").as_slice(),
            ),
            (
                "crates/oe-wire/src/json.rs",
                include_bytes!("../../oe-wire/src/json.rs").as_slice(),
            ),
            (
                "crates/oe-wire/src/unicode_casefold.rs",
                include_bytes!("../../oe-wire/src/unicode_casefold.rs").as_slice(),
            ),
            (
                "crates/oe-core/src/lib.rs",
                include_bytes!("../../oe-core/src/lib.rs").as_slice(),
            ),
        ] {
            let target = path.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }

        for &(relative, bytes) in oe_semantic::books::rust::source_execution::IMPLEMENTATION_SOURCES
        {
            let target = path.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        let upstream = path.join("var/upstream");
        write(
            &upstream.join("source.txt"),
            b"Explicit synthetic Semantica checkout\n",
        );
        git(&upstream, &["init", "-q"]);
        git(&upstream, &["add", "."]);
        git(&upstream, &["commit", "-q", "-m", "synthetic upstream"]);
        let source_commit = git(&upstream, &["rev-parse", "HEAD"]);
        let raw = wheel();
        write(&path.join("runtime/vendor").join(WHEEL), &raw);
        document(
            &path.join("runtime/semantica-source-lock.json"),
            &json!({"$schema":oe_semantic::runtime::SOURCE_LOCK_SCHEMA,"source":{"commit":source_commit,"version":"9.9.9+fixture","canonical_repository":"https://github.com/jiaqiwang969/semantica.git"},"artifact":{"filename":WHEEL,"sha256":sha256(&raw)}}),
        );
        for book in release::BOOK_SPECS {
            write(
                &path.join(book.pdf),
                b"Synthetic PDF bytes only for explicit fake Poppler tools\n",
            );
        }
        document(
            &path.join(release::VISUAL_REVIEW_PATH),
            &json!({"$schema":"ontology-engineering.pdf-visual-review/v1","advisory_only":true,"books":[{"pages_reviewed":[1],"passed":true,"volume":"vol1"},{"pages_reviewed":[1],"passed":true,"volume":"vol2"}],"passed":true,"review_basis":"Explicit fixture; not actual human review"}),
        );
        for name in [
            "docs/PRIVACY-AND-RIGHTS.md",
            "docs/PUBLIC-RELEASE-STATUS.md",
        ] {
            write(
                &path.join(name),
                b"Synthetic rights pending; no publication approval\n",
            );
        }
        write(&path.join(OLD), b"{\"historical_v1\":true}\n");
        write(
            &path.join(format!("{OLD}.sha256")),
            b"old sidecar preserved byte-for-byte\n",
        );
        assert_eq!(
            books::check_authoring_locks(&path, true).unwrap()["passed"],
            true
        );
        script(
            &path.join("var/tools/pdfinfo"),
            "#!/bin/sh\nprintf 'Pages: 2\\n'\n",
        );
        script(
            &path.join("var/tools/pdffonts"),
            "#!/bin/sh\nprintf 'name type encoding emb sub uni object ID\\n---- ---- ---- --- --- --- --- --\\nFixture CID Type 0C Identity-H yes no yes 12 0\\n'\n",
        );
        script(
            &path.join("var/tools/pdftotext"),
            &format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", "汉🛠".repeat(600)),
        );
        script(
            &path.join("var/tools/python"),
            "#!/bin/sh\n[ \"$1\" = '-m' ] && [ \"$2\" = pytest ] && [ \"$3\" = -q ] && [ \"$4\" = tests/ontology ] && [ \"$5\" = tests/chapter_packages ] || exit 19\nprintf '2 passed, 1 skipped in 0.01s\\n'\n",
        );
        script(
            &path.join("var/tools/cargo"),
            "#!/bin/sh\n[ \"$*\" = 'test --workspace --locked --offline' ] || exit 19\nprintf 'test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\\n'\n",
        );
        let root = SkillRoot::new(&path).unwrap();
        git(&path, &["init", "-q"]);
        git(&path, &["add", "."]);
        git(&path, &["commit", "-q", "-m", "synthetic flow source"]);
        let commit = git(&path, &["rev-parse", "HEAD"]);
        Self {
            path,
            root,
            upstream,
            commit,
        }
    }
    fn options(&self, name: &str) -> evidence::EvidenceOptions {
        let mut o = evidence::EvidenceOptions::new(self.path.join("var").join(name));
        o.python = self.path.join("var/tools/python");
        o.cargo = self.path.join("var/tools/cargo");
        o.pdfinfo = self.path.join("var/tools/pdfinfo");
        o.pdffonts = self.path.join("var/tools/pdffonts");
        o.pdftotext = self.path.join("var/tools/pdftotext");
        o.semantica_root = Some(self.upstream.clone());
        o.oe_source_commit = Some(self.commit.clone());
        o.timeout = Duration::from_secs(15);
        o
    }
    fn collect(&self) -> Value {
        release::initialize_governance(&self.root, false).unwrap();
        let mut all = serde_json::Map::new();
        for (name, run) in [
            (
                "collect-static",
                evidence::collect_static
                    as fn(&SkillRoot, &evidence::EvidenceOptions) -> anyhow::Result<Value>,
            ),
            ("collect-books", evidence::collect_book_bindings),
            ("collect-regressions", evidence::collect_regressions),
            ("collect-pdf", evidence::collect_pdf_qa),
        ] {
            let d = run(&self.root, &self.options(name)).unwrap();
            for (k, v) in d.as_object().unwrap() {
                assert!(all.insert(k.clone(), v.clone()).is_none());
                evidence::validate_document(&self.root, k, v).unwrap();
            }
        }
        assert_eq!(
            all.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            release::EVIDENCE_PATHS.iter().map(|(k, _)| *k).collect()
        );
        Value::Object(all)
    }
    fn create(&self, name: &str) -> anyhow::Result<Value> {
        release::create_manifest(
            &self.root,
            &release::CreateOptions {
                oe_source_commit: self.commit.clone(),
                evidence: self.options(name),
                claim_release: false,
            },
        )
    }
    fn verify(&self, name: &str) -> anyhow::Result<Value> {
        release::verify_manifest(
            &self.root,
            &release::VerifyOptions {
                manifest: release::MANIFEST_PATH.into(),
                evidence: self.options(name),
            },
        )
    }
    fn raw(&self, doc: &Value) -> Value {
        read(
            &self
                .path
                .join(doc["execution"]["raw"]["path"].as_str().unwrap()),
        )
    }
}
fn error<T: std::fmt::Debug>(result: anyhow::Result<T>, expected: &str) {
    let e = format!("{:#}", result.unwrap_err());
    assert!(e.contains(expected), "expected {expected}: {e}");
}
#[test]
fn full_collection_blocked_candidate_create_repeated_verify_and_tamper_denials() {
    // Repeated candidate assessment means create then verify, each with all
    // seven fresh observations. A second identical verify adds no new contract.
    let c = Case::new();
    let old = fs::read(c.path.join(OLD)).unwrap();
    let old_side = fs::read(c.path.join(format!("{OLD}.sha256"))).unwrap();
    let docs = c.collect();
    assert!(!c.path.join("source.txt").exists());
    let fixed_before: BTreeMap<_, _> = release::EVIDENCE_PATHS
        .iter()
        .chain(release::GOVERNANCE_PATHS)
        .chain(release::REGRESSION_LOG_PATHS)
        .map(|(_, path)| (*path, fs::read(c.path.join(path)).unwrap()))
        .collect();
    assert_eq!(docs["authoring_locks"]["passed"], true);
    assert_eq!(docs["runtime_identity"]["passed"], false);
    assert_eq!(docs["book_source_bindings"]["passed"], false);
    let native = c.raw(&docs["book_source_bindings"]);
    assert_eq!(native["data"]["report"]["status"], "blocked", "{native}");
    assert_eq!(native["data"]["report"]["semantic_execution"], "not_run");
    assert_eq!(
        native["data"]["report"]["rust_execution"]["semantic_execution"],
        "not_run"
    );
    assert_eq!(native["data"]["report"]["result"]["check_count"], 0);
    assert!(
        native["data"]["report"]["validation_error"]
            .as_str()
            .unwrap()
            .starts_with("approved_book_source_unavailable:")
    );
    assert_ne!(
        native["data"]["report"]["native_execution"]["semantic_execution"],
        "completed"
    );
    assert_eq!(native["data"]["report"]["fact_admitted"], false);
    let created = c.create("create").unwrap();
    assert_eq!(created["passed"], true);
    assert_eq!(created["release_eligible"], false);
    assert_eq!(created["package_count"], 29);
    let manifest = fs::read(c.path.join(release::MANIFEST_PATH)).unwrap();
    let sidecar = fs::read(c.path.join(format!("{}.sha256", release::MANIFEST_PATH))).unwrap();
    let value: Value = serde_json::from_slice(&manifest).unwrap();
    assert_eq!(value["artifact_status"], "candidate");
    assert_eq!(
        value["verification"]["runtime_identity"]["status"],
        "blocked"
    );
    assert_eq!(
        value["verification"]["book_source_bindings"]["status"],
        "blocked"
    );
    assert_eq!(value["verification"].as_object().unwrap().len(), 7);
    assert_eq!(value["release_verdict"]["eligible"], false);
    let report = c.verify("verify-one").unwrap();
    assert_eq!(report, created);
    assert_eq!(
        fs::read(c.path.join(release::MANIFEST_PATH)).unwrap(),
        manifest
    );
    assert_eq!(
        fs::read(c.path.join(format!("{}.sha256", release::MANIFEST_PATH))).unwrap(),
        sidecar
    );
    for (path, bytes) in &fixed_before {
        assert_eq!(&fs::read(c.path.join(path)).unwrap(), bytes);
    }
    for name in ["create", "verify-one"] {
        let mut kinds = BTreeSet::new();
        for operation in ["static", "book-bindings", "regressions", "pdf-qa"] {
            let fresh = read(&c.path.join(format!("var/{name}/{operation}-replay.json")));
            for (kind, observed) in fresh.as_object().unwrap() {
                assert!(kinds.insert(kind.as_str().to_owned()));
                assert!(
                    observed["execution"]["raw"]["path"]
                        .as_str()
                        .unwrap()
                        .starts_with(&format!("var/{name}/"))
                );
                assert_eq!(observed["passed"], docs[kind]["passed"]);
            }
        }
        assert_eq!(
            kinds,
            release::EVIDENCE_PATHS
                .iter()
                .map(|(k, _)| k.to_string())
                .collect()
        );
    }
    write(
        &c.path.join(format!("{}.sha256", release::MANIFEST_PATH)),
        b"changed sidecar\n",
    );
    error(c.verify("sidecar-denied"), "book_manifest_sidecar_invalid");
    write(
        &c.path.join(format!("{}.sha256", release::MANIFEST_PATH)),
        &sidecar,
    );
    let pdf = c.path.join(release::BOOK_SPECS[0].pdf);
    let original = fs::read(&pdf).unwrap();
    write(&pdf, b"altered fixed generated PDF input\n");
    assert!(c.verify("pdf-denied").is_err());
    write(&pdf, original);
    let ep = c.path.join(release::evidence_path("pdf_qa").unwrap());
    let original = fs::read(&ep).unwrap();
    let mut changed: Value = serde_json::from_slice(&original).unwrap();
    changed["criteria"]["pdf_contract"] = json!("always pass");
    document(&ep, &changed);
    assert!(c.verify("rules-denied").is_err());
    write(&ep, original);
    let source = c.path.join("crates/oe-semantic/src/release_evidence.rs");
    let original = fs::read(&source).unwrap();
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n// changed evaluator basis\n");
    write(&source, changed);
    error(
        evidence::observe_static(&c.root, &c.options("compiled-rule-denied")),
        "compiled_release_evidence_source_mismatch",
    );
    write(&source, original);
    error(
        release::create_manifest(
            &c.root,
            &release::CreateOptions {
                oe_source_commit: c.commit.clone(),
                evidence: c.options("release-denied"),
                claim_release: true,
            },
        ),
        "book_candidate_cannot_claim_release",
    );
    assert!(!c.path.join("var/release-denied").exists());
    assert_eq!(fs::read(c.path.join(OLD)).unwrap(), old);
    assert_eq!(
        fs::read(c.path.join(format!("{OLD}.sha256"))).unwrap(),
        old_side
    );
    assert_eq!(
        fs::read(c.path.join(release::MANIFEST_PATH)).unwrap(),
        manifest
    );
    for (path, bytes) in &fixed_before {
        assert_eq!(&fs::read(c.path.join(path)).unwrap(), bytes);
    }
    assert!(!c.path.join("runtime/.venv").exists());
}

fn corpus(path: &Path) {
    write(&path.join("SKILL.md"), "synthetic fixture only\n");
    let one = path.join("references").join(VOL1);
    let two = path.join("references").join(VOL2);
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
