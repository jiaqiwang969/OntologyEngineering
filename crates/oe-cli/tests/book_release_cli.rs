//! Source-only CLI contracts. No native runtime, external model or book release is claimed.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case {
    path: PathBuf,
}
impl Case {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = root.join(format!(
            "var/maintenance/rust-tests/book-release-cli-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "synthetic CLI root\n").unwrap();
        Self { path }
    }
    fn put(&self, relative: &str, content: &str) {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fn run(&self, args: &[&str], code: i32) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(&self.path)
            .args(args)
            .current_dir(self.path.parent().unwrap())
            .env_clear()
            .env("PATH", "")
            .env("HOME", self.path.join("var/empty-home"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}

#[test]
fn governance_uses_explicit_root_and_only_creates_pending_current_records() {
    let c = Case::new();
    for p in [
        "docs/PRIVACY-AND-RIGHTS.md",
        "docs/PUBLIC-RELEASE-STATUS.md",
        "references/product-trustworthiness-book/handbook/book-metadata.tex",
    ] {
        c.put(p, "unconfirmed fixture source\n");
    }
    c.put(
        "references/release-evidence/rights.json",
        "immutable historical record\n",
    );
    let first = c.run(&["books", "evidence", "governance"], 0);
    assert_eq!(first["result"]["rights"], "pending");
    assert_eq!(first["publication"], "not_performed");
    assert_eq!(
        c.run(&["books", "evidence", "governance"], 0)["result"]["changed"],
        false
    );
    assert_eq!(
        c.run(&["books", "evidence", "governance", "--reset-existing"], 0)["result"]["rights"],
        "pending"
    );
    assert_eq!(
        fs::read(c.path.join("references/release-evidence/rights.json")).unwrap(),
        b"immutable historical record\n"
    );
}

#[test]
fn claim_release_and_missing_input_fail_without_publishing_a_candidate() {
    let c = Case::new();
    let result = c.run(
        &[
            "books",
            "release",
            "create",
            "--oe-source-commit",
            &"0".repeat(40),
            "--output",
            "var/release",
            "--claim-release",
        ],
        2,
    );
    assert_eq!(result["status"], "blocked");
    assert!(
        !c.path
            .join("references/book-release-artifacts.v2.json")
            .exists()
    );
    assert!(!c.path.join("var/release").exists());
    let result = c.run(&["books", "release", "verify", "--output", "var/verify"], 2);
    assert_eq!(result["status"], "blocked");
    assert!(!c.path.join("var/verify").exists());
}

#[test]
fn privacy_scans_actual_complete_text_with_empty_path_and_explicit_full_tree_scope() {
    let c = Case::new();
    c.put("notes.txt", "Plain engineering fixture text.\n");
    let result = c.run(
        &["privacy", "--include-ignored", "--output", "var/check01"],
        0,
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["effective_scope"], "full_tree_including_ignored");
    // Remove our already consumed first scan's own evidence before a new full-tree scan.
    // This is test-owned output, not a skip rule in the scanner.
    fs::remove_dir_all(c.path.join("var/check01")).unwrap();
    let token = format!("{}{}", "apikey_", "x".repeat(40));
    c.put("notes.txt", &format!("Do not distribute {token}\n"));
    let result = c.run(
        &["privacy", "--include-ignored", "--output", "var/check02"],
        1,
    );
    assert_eq!(result["ok"], false);
    assert!(!result["findings"].as_array().unwrap().is_empty());
    assert!(!result.to_string().contains(&token));
}

#[test]
fn new_argument_errors_keep_json_protocol_without_echoing_supplied_tokens() {
    let c = Case::new();
    let token = format!("{}{}", "apikey_", "q".repeat(40));
    for args in [
        vec!["privacy", "--unknown", token.as_str()],
        vec!["books", "release", "create", "--unknown", token.as_str()],
        vec!["books", "evidence", "regressions", "--output", "var/a"],
    ] {
        let result = c.run(&args, 2);
        assert_eq!(result["error"]["code"], "invalid_arguments");
        assert!(!result.to_string().contains(&token));
    }
}
