//! Real Git/SQLite fixtures. No original corpus, database or secret is accessed.
use oe_core::{
    SkillRoot,
    privacy::{ScanOptions, scan},
};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let path = base.join(format!(
            "var/maintenance/rust-tests/privacy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "synthetic public candidate\n").unwrap();
        Self {
            root: SkillRoot::new(path).unwrap(),
        }
    }
    fn put(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let p = self.root.path().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    fn options(&self, name: &str) -> ScanOptions {
        let mut o = ScanOptions::new("git".into(), PathBuf::from(format!("var/{name}")));
        o.include_ignored = true;
        o
    }
    fn scan(&self, name: &str) -> Value {
        scan(&self.root, &self.options(name)).unwrap()
    }
    fn git(&self, args: &[&str]) {
        let o = Command::new("git")
            .current_dir(self.root.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "git fixture failed");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(self.root.path()).unwrap();
        }
    }
}
fn rules(v: &Value, path: &str) -> Vec<String> {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["path"] == path)
        .map(|v| v["rule"].as_str().unwrap().into())
        .collect()
}
fn has(v: &Value, path: &str, rule: &str) {
    assert!(
        rules(v, path).iter().any(|s| s == rule),
        "missing {path}: {rule}; {}",
        v["findings"]
    );
}
fn token() -> String {
    format!("{}{}", "apikey_", "7".repeat(48))
}

#[test]
fn safe_full_tree_and_binary_exemptions_have_exact_inventory_and_no_self_scan() {
    let f = Fixture::new();
    f.put("README.md", "public engineering example\n");
    f.put(".env.example", "PLACEHOLDER=synthetic\n");
    f.put("artifact.WHL", b"PK\0\xff");
    let v = f.scan("scan");
    assert_eq!(v["ok"], true);
    assert_eq!(v["checked_files"], 4);
    assert_eq!(v["effective_scope"], "full_tree_including_ignored");
    assert_eq!(v["selection_reason"], "explicit_include_ignored");
    assert_eq!(v["inputs_unchanged"], true);
    assert_eq!(v["processes"], serde_json::json!([]));
    assert_eq!(
        v["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["sha256"].as_str().is_some_and(|s| s.len() == 64))
            .count(),
        4
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(f.path("var/scan/report.json")).unwrap()).unwrap();
    assert_eq!(saved, v);
}

#[test]
fn the_scanner_and_its_tests_are_public_scannable_without_fixture_whitelists() {
    let f = Fixture::new();
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    f.put(
        "privacy.rs",
        fs::read(crate_root.join("src/privacy.rs")).unwrap(),
    );
    f.put(
        "privacy-tests.rs",
        fs::read(crate_root.join("tests/privacy.rs")).unwrap(),
    );
    let v = f.scan("scan");
    assert_eq!(v["checked_files"], 3);
    assert_eq!(v["ok"], true, "{}", v["findings"]);
}
#[test]
fn every_content_rule_and_unicode_word_boundaries_match_the_legacy_contract() {
    let f = Fixture::new();
    let cases = [
        (
            format!("/{}/{}/file", "Users", "synthetic-user"),
            "macOS personal absolute path",
        ),
        (
            format!("/{}/{}/file", "home", "synthetic-user"),
            "Linux personal absolute path",
        ),
        (
            format!("C:\\{}\\{}\\file", "Users", "synthetic-user"),
            "Windows personal absolute path",
        ),
        (
            format!("/{}/{}/abc", "var", "folders"),
            "macOS attachment cache path",
        ),
        (
            format!("-----BEGIN {}PRIVATE KEY-----", "OPENSSH "),
            "private key material",
        ),
        (
            format!("{}{}", "sk-", "A".repeat(24)),
            "OpenAI-style secret token",
        ),
        (
            format!("{}{}", "ghp_", "A".repeat(24)),
            "GitHub secret token",
        ),
        (
            format!("{}{}", "github_pat_", "A".repeat(24)),
            "GitHub secret token",
        ),
        (format!("{}{}", "AKIA", "A".repeat(16)), "AWS access key"),
        (token(), "Legacy API token"),
        (
            format!("{}{}", "codex-clipboard-", "fixture"),
            "clipboard attachment identifier",
        ),
    ];
    for (i, (text, _)) in cases.iter().enumerate() {
        f.put(&format!("case{i}.md"), format!("safe\r\n{text}\rnext"));
    }
    f.put("safe-placeholders.md","/Users/<name>/file\n/home/<name>/file\nC:\\Users\\<name>\\file\napikey_SYNTHETIC_TEST_ONLY\n");
    for (i, prefix) in ["中", "𐐀", "_", "²"].iter().enumerate() {
        f.put(&format!("word{i}.md"), format!("{prefix}{}", token()));
    }
    for (i, prefix) in ["\u{0301}", "\u{0345}", "-", "🧪"].iter().enumerate() {
        f.put(&format!("boundary{i}.md"), format!("{prefix}{}", token()));
    }
    let v = f.scan("scan");
    assert_eq!(v["ok"], false);
    for (i, (text, rule)) in cases.iter().enumerate() {
        has(&v, &format!("case{i}.md"), rule);
        assert!(!v.to_string().contains(text));
        assert!(
            v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["path"] == format!("case{i}.md") && x["line"] == 2)
        );
    }
    assert!(rules(&v, "safe-placeholders.md").is_empty());
    for i in 0..4 {
        assert!(rules(&v, &format!("word{i}.md")).is_empty());
        has(&v, &format!("boundary{i}.md"), "Legacy API token");
    }
}
#[test]
fn path_controls_credentials_generated_names_and_utf8_errors_are_fail_closed() {
    let f = Fixture::new();
    for name in [
        ".env.local",
        ".env.anything",
        ".netrc",
        ".npmrc",
        ".pypirc",
        "id_ed25519",
        "id_rsa",
    ] {
        f.put(name, "synthetic");
    }
    for name in [
        ".DS_Store",
        "thing.PEM",
        "private/note.txt",
        "sessions/trace.txt",
        "rollouts/a.txt",
        "__pycache__/a.pyc",
        "sources/raw/book.bin",
        "structured/mineru/a.json",
        "reports/imagegen-events/raw/a.txt",
        ".claude/projects/a.txt",
        ".codex/sessions/a.txt",
    ] {
        f.put(name, "synthetic");
    }
    // Unix permits this original real-file scanner counterexample. Win32
    // cannot construct it; its equivalent owner check is lexical, before IO.
    #[cfg(not(windows))]
    f.put("bad\nname.md", token());
    #[cfg(windows)]
    {
        let path = f.path("bad\nname.md");
        assert_eq!(
            oe_core::portable_source_path(f.root.path(), &path)
                .unwrap_err()
                .to_string(),
            "portable_source_path_invalid"
        );
    }
    f.put("invalid.md", b"valid\n\xff");
    f.put("nul.md", b"valid\r\n\0payload");
    f.put("control.md", b"valid\n\x01\x7f");
    let v = f.scan("scan");
    #[cfg(not(windows))]
    has(&v, "bad\\nname.md", "control character in candidate path");
    assert!(!v.to_string().contains(&token()));
    #[cfg(not(windows))]
    assert!(!rules(&v, "bad\\nname.md").contains(&"Legacy API token".into()));
    has(&v, "invalid.md", "invalid UTF-8 text candidate");
    has(&v, "nul.md", "NUL byte in declared text candidate");
    has(
        &v,
        "control.md",
        "invalid control character in text candidate",
    );
    has(&v, ".DS_Store", "OS metadata file");
    has(&v, "thing.PEM", "secret or generated binary suffix");
    has(&v, ".env.anything", "credential/config filename");
    has(
        &v,
        "sources/raw/book.bin",
        "forbidden private evidence path",
    );
}
#[test]
fn private_evidence_fragments_and_reports_keep_original_portable_path_semantics() {
    let f = Fixture::new();
    for path in [
        "sources/raw/book.bin",
        "structured/mineru/book.bin",
        "reports/imagegen-events/raw/book.bin",
        ".claude/projects/session.txt",
        ".codex/sessions/session.txt",
        "mysources/rawpost/book.bin",
        "public/sources-not-raw/book.bin",
        "public/SOURCES/raw/book.bin",
    ] {
        f.put(path, "synthetic path projection fixture");
    }
    #[cfg(unix)]
    f.put(r"sources\raw\book.bin", "literal Unix filename");
    let v = f.scan("fragment-scan");
    for path in [
        "sources/raw/book.bin",
        "structured/mineru/book.bin",
        "reports/imagegen-events/raw/book.bin",
        ".claude/projects/session.txt",
        ".codex/sessions/session.txt",
        "mysources/rawpost/book.bin",
    ] {
        has(&v, path, "forbidden private evidence path");
        assert!(
            v["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["path"] == path)
        );
    }
    for path in [
        "public/sources-not-raw/book.bin",
        "public/SOURCES/raw/book.bin",
    ] {
        assert!(
            rules(&v, path).is_empty(),
            "original case-sensitive substring rule: {path}"
        );
        assert!(
            v["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["path"] == path)
        );
    }
    #[cfg(unix)]
    {
        assert!(rules(&v, r"sources\raw\book.bin").is_empty());
        assert!(
            v["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["path"] == r"sources\raw\book.bin")
        );
    }
    assert_eq!(v["excluded_generated_output"], "var/fragment-scan");
    assert_eq!(v["inputs_unchanged"], true);
    assert_eq!(v["ok"], false);
}
#[test]
fn streaming_line_limit_counts_unicode_characters_and_keeps_line_identity() {
    let f = Fixture::new();
    f.put("wide.md", "界".repeat(6 * 1024 * 1024));
    f.put(
        "too-long.md",
        format!("first\r\n{}", "a".repeat(16 * 1024 * 1024 + 1)),
    );
    let v = f.scan("scan");
    assert!(rules(&v, "wide.md").is_empty());
    has(&v, "too-long.md", "text line exceeds bounded privacy scan");
    assert!(
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["path"] == "too-long.md" && x["line"] == 2)
    );
}
#[cfg(unix)]
#[test]
fn links_fifo_and_non_utf8_names_are_never_followed_or_blocking() {
    use std::os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
    };
    let f = Fixture::new();
    symlink("/absent-private-synthetic", f.path("broken-link")).unwrap();
    symlink(f.path("var"), f.path("directory-link")).unwrap();
    let fifo = f.path("pipe");
    let raw = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
    let non_utf8 = fs::write(
        f.root
            .path()
            .join(std::ffi::OsString::from_vec(b"bad-\xff.md".to_vec())),
        b"safe",
    );
    // APFS refuses illegal UTF-8 filenames before scanner entry; the other
    // path checks still run here. Linux exercises the explicit invalid name.
    if let Err(error) = &non_utf8 {
        assert!(cfg!(target_os = "macos") && error.raw_os_error() == Some(92));
    }
    let started = std::time::Instant::now();
    let v = f.scan("scan");
    assert!(started.elapsed().as_secs() < 5);
    has(
        &v,
        "broken-link",
        "symbolic link is not allowed in a public candidate",
    );
    has(
        &v,
        "pipe",
        "special filesystem node is not allowed in a public candidate",
    );
    if non_utf8.is_ok() {
        assert!(
            v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["rule"] == "non-UTF-8 candidate path")
        );
    }
}
#[test]
fn git_selection_includes_unignored_excludes_ignored_and_ignores_deleted_tracked_paths() {
    for tracked in [false, true] {
        let f = Fixture::new();
        f.git(&["init", "-q"]);
        f.put(".gitignore", "var/\nignored.md\n");
        f.put("tracked.md", "safe");
        f.put("deleted.md", "safe");
        f.git(&["add", "SKILL.md", ".gitignore", "tracked.md", "deleted.md"]);
        fs::remove_file(f.path("deleted.md")).unwrap();
        f.put("untracked.md", token());
        f.put("ignored.md", token());
        let mut options = ScanOptions::new("git".into(), "var/scan".into());
        options.tracked_only = tracked;
        let v = scan(&f.root, &options).unwrap();
        assert_eq!(v["checked_files"], if tracked { 3 } else { 4 });
        assert_eq!(v["ok"], tracked);
        assert!(rules(&v, "ignored.md").is_empty());
        if !tracked {
            has(&v, "untracked.md", "Legacy API token");
        }
        assert_eq!(v["processes"].as_array().unwrap().len(), 2);
        assert_eq!(v["selection_unchanged"], true);
    }
}
#[test]
fn git_failure_never_falls_back_to_a_green_full_tree_and_conflicting_scope_does_not_write() {
    let f = Fixture::new();
    f.put("safe.md", "safe");
    // Stop this nested fixture inheriting the producer's enclosing repository.
    f.put(".git", "gitdir: .missing-fixture-git-dir\n");
    let options = ScanOptions::new("git".into(), "var/non-git".into());
    let v = scan(&f.root, &options).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["status"], "blocked");
    assert_eq!(v["checked_files"], 0);
    assert_eq!(v["processes"][0]["status"], "process_failed");
    assert!(f.path("var/non-git/selection-before/stderr").is_file());
    let mut bad = f.options("conflict");
    bad.tracked_only = true;
    assert!(scan(&f.root, &bad).is_err());
    assert!(!f.path("var/conflict").exists());
    let mut missing = ScanOptions::new(f.path("absent-git"), "var/no-git-tool".into());
    let v = scan(&f.root, &missing).unwrap();
    assert_eq!(v["ok"], false);
    missing.output = "../outside".into();
    assert!(scan(&f.root, &missing).is_err());
}
#[test]
fn sqlite_live_text_and_schema_are_scanned_but_blobs_are_not_decoded_or_modified() {
    let f = Fixture::new();
    let path = f.path("catalog.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE facts(text_value TEXT,payload BLOB);")
        .unwrap();
    let private = format!("/{}/{}/file", "Users", "fixture-person");
    db.execute(
        "INSERT INTO facts VALUES(?1,?2)",
        params![private, vec![0xffu8, 0, 1, 255]],
    )
    .unwrap();
    drop(db);
    let before = oe_core::fingerprint_file(&path, u64::MAX).unwrap();
    let v = f.scan("scan");
    assert_eq!(
        rules(&v, "catalog.sqlite3"),
        vec!["macOS personal absolute path"]
    );
    assert_eq!(before, oe_core::fingerprint_file(&path, u64::MAX).unwrap());
    assert!(!v.to_string().contains(&private));
    assert!(!f.path("catalog.sqlite3-wal").exists());
    assert!(f.path("var/scan/sqlite-1.db").is_file());
}

#[test]
fn quoted_identifiers_and_view_trigger_schema_are_checked_without_executing_them() {
    let f = Fixture::new();
    let path = f.path("schema.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE \"quoted\"\"name\"(\"body\"\"name\" TEXT);INSERT INTO \"quoted\"\"name\" VALUES('safe');CREATE TABLE effects(value TEXT);").unwrap();
    let personal = format!("/{}/{}/file", "home", "synthetic-owner");
    db.execute_batch(&format!("CREATE VIEW \"{}\" AS SELECT '{}'; CREATE TRIGGER only_on_insert AFTER INSERT ON effects BEGIN INSERT INTO effects VALUES('{}');END;",token(),personal,token())).unwrap();
    drop(db);
    let before = oe_core::fingerprint_file(&path, u64::MAX).unwrap();
    let v = f.scan("scan");
    has(&v, "schema.db", "Legacy API token");
    has(&v, "schema.db", "Linux personal absolute path");
    assert!(!v.to_string().contains(&token()) && !v.to_string().contains(&personal));
    assert_eq!(before, oe_core::fingerprint_file(&path, u64::MAX).unwrap());
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM effects", [], |r| r.get(0))
            .unwrap(),
        0
    );
}

#[cfg(unix)]
#[test]
fn malicious_or_incomplete_git_path_stream_is_retained_but_never_used_as_scope() {
    use std::os::unix::fs::PermissionsExt;
    for (case, stream) in [
        ("traversal", "../outside\\000"),
        ("absolute", "/outside\\000"),
        ("unterminated", "SKILL.md"),
    ] {
        let f = Fixture::new();
        let path = f.path("fixture-git");
        f.put("fixture-git", format!("#!/bin/sh\nprintf '{stream}'\n"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        let v = scan(
            &f.root,
            &ScanOptions::new(path, format!("var/{case}").into()),
        )
        .unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["checked_files"], 0);
        assert_eq!(v["processes"][0]["status"], "completed");
        assert!(!v["error_code"].is_null());
        assert!(
            f.path(&format!("var/{case}/selection-before/stdout"))
                .is_file()
        );
    }
}
#[test]
fn sqlite_fts5_exception_works_and_other_virtual_modules_are_blocked() {
    let f = Fixture::new();
    let db = Connection::open(f.path("fulltext.db")).unwrap();
    db.execute_batch("CREATE VIRTUAL TABLE words USING fts5(body); INSERT INTO words VALUES('safe unicode 工程');").unwrap();
    drop(db);
    let db = Connection::open(f.path("spatial.db")).unwrap();
    db.execute_batch("CREATE VIRTUAL TABLE spatial USING rtree(id,x0,x1);")
        .unwrap();
    drop(db);
    let db = Connection::open(f.path("disguised.db")).unwrap();
    db.execute_batch("CREATE VIRTUAL TABLE disguise USING rtree(id,x0,x1); PRAGMA writable_schema=ON;UPDATE sqlite_schema SET sql='CREATE  VIRTUAL  TABLE disguise USING rtree(id,x0,x1) /* USING fts5 */' WHERE name='disguise';PRAGMA writable_schema=OFF;").unwrap();
    drop(db);
    let v = f.scan("scan");
    assert!(rules(&v, "fulltext.db").is_empty(), "{}", v["findings"]);
    has(
        &v,
        "spatial.db",
        "unsupported SQLite virtual table for privacy scan",
    );
    has(
        &v,
        "disguised.db",
        "unsupported SQLite virtual table for privacy scan",
    );
}

#[test]
fn sqlite_nul_suffix_and_findings_before_invalid_later_rows_are_preserved() {
    let f = Fixture::new();
    let db = Connection::open(f.path("partial.db")).unwrap();
    db.execute_batch("CREATE TABLE facts(value TEXT);").unwrap();
    db.execute(
        "INSERT INTO facts VALUES(?1)",
        [format!("safe\0{}", token())],
    )
    .unwrap();
    db.execute_batch("INSERT INTO facts VALUES(CAST(x'80' AS TEXT));")
        .unwrap();
    drop(db);
    let v = f.scan("scan");
    has(&v, "partial.db", "Legacy API token");
    has(
        &v,
        "partial.db",
        "invalid control character in text candidate",
    );
    has(&v, "partial.db", "unreadable or invalid SQLite candidate");
    assert!(!v.to_string().contains(&token()));
}

#[test]
fn secret_values_in_candidate_filenames_are_not_echoed_in_findings_or_inventory() {
    let f = Fixture::new();
    f.put(&format!(".env.{}", token()), "synthetic");
    let v = f.scan("scan");
    assert_eq!(v["ok"], false);
    assert!(!v.to_string().contains(&token()));
    assert!(v["findings"].as_array().unwrap().iter().any(|x| {
        x["rule"] == "credential/config filename"
            && x["path"]
                .as_str()
                .unwrap()
                .starts_with("redacted-path-sha256:")
    }));
}

#[test]
fn token_thresholds_and_both_unicode_boundaries_are_exact() {
    let f = Fixture::new();
    for (i, (prefix, min, rule)) in [
        ("sk-", 20, "OpenAI-style secret token"),
        ("gho_", 20, "GitHub secret token"),
        ("github_pat_", 20, "GitHub secret token"),
        ("ASIA", 16, "AWS access key"),
        ("apikey_", 32, "Legacy API token"),
    ]
    .iter()
    .enumerate()
    {
        let value = format!("{prefix}{}", "A".repeat(*min));
        f.put(
            &format!("short{i}.txt"),
            format!("{prefix}{}", "A".repeat(min - 1)),
        );
        f.put(&format!("exact{i}.txt"), &value);
        f.put(&format!("suffix{i}.txt"), format!("{value}中"));
        f.put(&format!("punctuation{i}.txt"), format!("!{value}!"));
        assert!(!rule.is_empty());
    }
    let v = f.scan("scan");
    for (i, rule) in [
        "OpenAI-style secret token",
        "GitHub secret token",
        "GitHub secret token",
        "AWS access key",
        "Legacy API token",
    ]
    .iter()
    .enumerate()
    {
        assert!(rules(&v, &format!("short{i}.txt")).is_empty());
        assert!(rules(&v, &format!("suffix{i}.txt")).is_empty());
        has(&v, &format!("exact{i}.txt"), rule);
        has(&v, &format!("punctuation{i}.txt"), rule);
    }
}

#[cfg(unix)]
#[test]
fn ignored_sqlite_sidecar_appearing_after_scan_cannot_be_reported_standalone() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let db = Connection::open(f.path("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE facts(value TEXT);INSERT INTO facts VALUES('safe');")
        .unwrap();
    drop(db);
    let script = f.path("fixture-git");
    let body = format!(
        "#!/bin/sh\nif [ -d '{}' ]; then printf '%s' 'fixture sidecar' > '{}'; fi\nprintf 'SKILL.md\\000catalog.db\\000'\n",
        f.path("var/scan/selection-after").display(),
        f.path("catalog.db-wal").display()
    );
    f.put("fixture-git", body);
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let v = scan(&f.root, &ScanOptions::new(script, "var/scan".into())).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["selection_unchanged"], true);
    assert_eq!(v["inputs_unchanged"], false);
    assert_eq!(v["candidates"][1]["unchanged"], false);
    assert_eq!(v["processes"].as_array().unwrap().len(), 2);
}
#[test]
fn sqlite_sidecars_free_pages_invalid_headers_and_invalid_utf8_fail_closed() {
    let f = Fixture::new();
    for name in ["sidecar.db", "free.db", "invalid-utf8.db"] {
        let db = Connection::open(f.path(name)).unwrap();
        db.execute_batch("CREATE TABLE facts(value TEXT);").unwrap();
        if name == "free.db" {
            db.execute_batch(
                "INSERT INTO facts VALUES(printf('%.*c',100000,'a'));DROP TABLE facts;",
            )
            .unwrap();
        }
        if name == "invalid-utf8.db" {
            db.execute_batch("INSERT INTO facts VALUES(CAST(x'80' AS TEXT));")
                .unwrap();
        }
    }
    f.put("sidecar.db-wal", b"");
    f.put("bad.db", b"not sqlite");
    f.put("broken.db", b"SQLite format 3\0truncated");
    let v = f.scan("scan");
    has(
        &v,
        "sidecar.db",
        "SQLite candidate must be standalone without sidecars",
    );
    has(
        &v,
        "free.db",
        "SQLite free pages require a vacuumed distribution copy",
    );
    has(&v, "bad.db", "invalid SQLite candidate header");
    has(&v, "broken.db", "unreadable or invalid SQLite candidate");
    has(
        &v,
        "invalid-utf8.db",
        "unreadable or invalid SQLite candidate",
    );
}
#[test]
fn sqlite_one_megacharacter_limit_is_not_a_byte_limit_and_oversize_is_not_truncated() {
    let f = Fixture::new();
    let db = Connection::open(f.path("wide.db")).unwrap();
    db.execute_batch("CREATE TABLE facts(value TEXT,payload BLOB);")
        .unwrap();
    db.execute(
        "INSERT INTO facts VALUES(?1,?2)",
        params!["界".repeat(1024 * 1024), vec![0xffu8; 2 * 1024 * 1024]],
    )
    .unwrap();
    drop(db);
    let db = Connection::open(f.path("large.db")).unwrap();
    db.execute_batch("CREATE TABLE facts(value TEXT);").unwrap();
    db.execute(
        "INSERT INTO facts VALUES(?1)",
        [format!("{}{}", "a".repeat(1024 * 1024), token())],
    )
    .unwrap();
    drop(db);
    let v = f.scan("scan");
    assert!(rules(&v, "wide.db").is_empty(), "{}", v["findings"]);
    has(
        &v,
        "large.db",
        "SQLite text value exceeds bounded privacy scan",
    );
    assert!(!rules(&v, "large.db").contains(&"Legacy API token".into()));
    assert!(!v.to_string().contains(&token()));
}
#[test]
fn sqlite_malicious_generated_schema_cannot_load_external_code_or_echo_its_argument() {
    let f = Fixture::new();
    let db = Connection::open(f.path("malicious.db")).unwrap();
    db.execute_batch("CREATE TABLE facts(value TEXT); INSERT INTO facts VALUES('safe');PRAGMA writable_schema=ON;UPDATE sqlite_schema SET sql='CREATE TABLE facts(value TEXT, trap AS (load_extension(''PRIVATE_FIXTURE_LIBRARY'')))' WHERE name='facts';PRAGMA writable_schema=OFF;").unwrap();
    drop(db);
    let before = oe_core::fingerprint_file(&f.path("malicious.db"), u64::MAX).unwrap();
    let v = f.scan("scan");
    has(&v, "malicious.db", "unreadable or invalid SQLite candidate");
    assert!(!v.to_string().contains("PRIVATE_FIXTURE_LIBRARY"));
    assert_eq!(
        before,
        oe_core::fingerprint_file(&f.path("malicious.db"), u64::MAX).unwrap()
    );
}
#[cfg(unix)]
#[test]
fn source_change_after_content_scan_is_blocked_with_earlier_identity_and_git_receipts_retained() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.put("payload.txt", "safe before\n");
    let script = f.path("fixture-git");
    let changed = f.path("var/scan/selection-after");
    let payload = f.path("payload.txt");
    let body = format!(
        "#!/bin/sh\nif [ -d '{}' ]; then printf '%s' 'changed after scan' > '{}'; fi\nprintf 'SKILL.md\\000payload.txt\\000'\n",
        changed.display(),
        payload.display()
    );
    f.put("fixture-git", body);
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let options = ScanOptions::new(script, "var/scan".into());
    let v = scan(&f.root, &options).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["inputs_unchanged"], false);
    assert_eq!(v["processes"].as_array().unwrap().len(), 2);
    assert_eq!(
        v["candidates"][1]["sha256"],
        oe_core::sha256(b"safe before\n")
    );
    assert!(!v["error_code"].is_null());
}
