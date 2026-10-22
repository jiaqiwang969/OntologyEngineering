//! Real Rust orchestration with explicit synthetic external tools and private Git
//! roots. These fixtures are not evidence of upstream suites or visual approval.
#![cfg(unix)]
use oe_core::{SkillRoot, sha256};
use oe_semantic::{book_release as release, release_evidence as evidence};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case {
    path: PathBuf,
    root: SkillRoot,
    upstream: PathBuf,
    commit: String,
    retain: bool,
}
fn git(path: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .args(args)
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Synthetic evidence")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Synthetic evidence")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}
fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
impl Case {
    fn new() -> Self {
        let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let path = producer
            .join("var/maintenance/rust-rewrite-20260927/phase10-evidence/fixtures")
            .join(format!(
                "case {}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(path.join("runtime/vendor")).unwrap();
        fs::create_dir_all(path.join("var/tools")).unwrap();
        fs::create_dir_all(path.join("crates/oe-semantic/src")).unwrap();
        fs::write(path.join("SKILL.md"), "synthetic local evidence root\n").unwrap();
        fs::write(path.join(".gitignore"), "var/\n").unwrap();
        fs::write(path.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(path.join("Cargo.lock"), "version = 4\n").unwrap();
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
        fs::create_dir_all(&upstream).unwrap();
        fs::write(
            upstream.join("source.txt"),
            "explicit synthetic upstream source\n",
        )
        .unwrap();
        git(&upstream, &["init", "-q"]);
        git(&upstream, &["add", "."]);
        git(&upstream, &["commit", "-q", "-m", "synthetic source"]);
        let source_commit = git(&upstream, &["rev-parse", "HEAD"]);
        let wheel = b"fixture identity bytes; not an installed wheel\n";
        fs::write(path.join("runtime/vendor/semantica-fixture.whl"), wheel).unwrap();
        let lock = json!({"$schema":"ontology-engineering.semantica-source-lock/v1","source":{"commit":source_commit,"version":"0.0+fixture","canonical_repository":"https://github.com/jiaqiwang969/semantica.git"},"artifact":{"filename":"semantica-fixture.whl","sha256":sha256(wheel)}});
        fs::write(
            path.join("runtime/semantica-source-lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        for book in release::BOOK_SPECS {
            let p = path.join(book.pdf);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"synthetic PDF input for explicit fake tools\n").unwrap();
        }
        let root = SkillRoot::new(&path).unwrap();
        let mut c = Self {
            path,
            root,
            upstream,
            commit: String::new(),
            retain: false,
        };
        c.visual(json!([1]), true);
        script(
            &c.path.join("var/tools/pdfinfo"),
            "#!/bin/sh\nprintf 'Pages: 2\\n'\n",
        );
        script(
            &c.path.join("var/tools/pdffonts"),
            "#!/bin/sh\nprintf 'name type encoding emb sub uni object ID\\n---- ---- ---- --- --- --- --- --\\nLongNameWithWideFontType CID Type 0C Identity-H yes no yes 12 0\\n'\n",
        );
        script(
            &c.path.join("var/tools/pdftotext"),
            &format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", "汉🛠".repeat(600)),
        );
        script(
            &c.path.join("var/tools/python"),
            "#!/bin/sh\n[ \"$1\" = '-m' ] && [ \"$2\" = pytest ] && [ \"$3\" = -q ] && [ \"$4\" = tests/ontology ] && [ \"$5\" = tests/chapter_packages ] || exit 19\nprintf '2 passed, 1 skipped in 0.01s\\n'\n",
        );
        script(
            &c.path.join("var/tools/cargo"),
            "#!/bin/sh\n[ \"$*\" = 'test --workspace --locked --offline' ] || exit 19\nprintf 'test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\\n\\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s\\n'\n",
        );
        git(&c.path, &["init", "-q"]);
        git(&c.path, &["add", "."]);
        git(
            &c.path,
            &["commit", "-q", "-m", "synthetic evidence inputs"],
        );
        c.commit = git(&c.path, &["rev-parse", "HEAD"]);
        c
    }
    fn visual(&mut self, pages: Value, passed: bool) {
        let value = json!({"$schema":"ontology-engineering.pdf-visual-review/v1","advisory_only":true,"books":[{"pages_reviewed":pages,"passed":passed,"volume":"vol1"},{"pages_reviewed":pages,"passed":passed,"volume":"vol2"}],"passed":passed,"review_basis":"Explicit synthetic fixture, not actual human review"});
        let path = self.path.join(release::VISUAL_REVIEW_PATH);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, release::canonical_bytes(&value).unwrap()).unwrap();
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
        o.timeout = Duration::from_secs(10);
        o
    }
    fn raw(&self, doc: &Value) -> Value {
        serde_json::from_slice(
            &fs::read(
                self.path
                    .join(doc["execution"]["raw"]["path"].as_str().unwrap()),
            )
            .unwrap(),
        )
        .unwrap()
    }
}

#[test]
fn pdf_complete_fake_tool_receipts_preserve_unicode_and_advisory_identity() {
    let c = Case::new();
    let docs = evidence::collect_pdf_qa(&c.root, &c.options("pdf")).unwrap();
    let d = &docs["pdf_qa"];
    assert_eq!(d["passed"], true, "{d}");
    assert_eq!(d["observation"]["books"][0]["text_characters"], 1200);
    assert_eq!(d["observation"]["books"][0]["font_count"], 1);
    assert_eq!(c.raw(d)["processes"].as_array().unwrap().len(), 6);
    evidence::validate_document(&c.root, "pdf_qa", d).unwrap();
    assert!(
        c.path
            .join(release::evidence_path("pdf_qa").unwrap())
            .is_file()
    );
    assert!(
        !c.path
            .join("references/release-evidence/pdf-qa.json")
            .exists()
    );
}
#[test]
fn observations_do_not_overwrite_fixed_evidence_and_replay_ignores_only_execution_locations() {
    let c = Case::new();
    let old = evidence::collect_pdf_qa(&c.root, &c.options("old")).unwrap();
    let path = c.path.join(release::evidence_path("pdf_qa").unwrap());
    let original = fs::read(&path).unwrap();
    let fresh = evidence::observe_pdf_qa(&c.root, &c.options("new")).unwrap();
    assert_eq!(fs::read(path).unwrap(), original);
    evidence::compare_replay(&c.root, "pdf_qa", &old["pdf_qa"], &fresh["pdf_qa"]).unwrap();
}
#[test]
fn visual_false_remains_blocked_and_out_of_range_is_not_accepted() {
    let mut c = Case::new();
    c.visual(json!([1]), false);
    let d = evidence::observe_pdf_qa(&c.root, &c.options("false")).unwrap();
    assert_eq!(d["pdf_qa"]["passed"], false);
    c.visual(json!([3]), true);
    let d = evidence::observe_pdf_qa(&c.root, &c.options("range")).unwrap();
    assert_eq!(d["pdf_qa"]["passed"], false);
    assert_eq!(
        d["pdf_qa"]["observation"]["books"][0]["error_code"],
        "pdf_observation_invalid"
    );
}
#[test]
fn strict_visual_schema_duplicate_unsorted_and_claimed_authority_reject_before_process() {
    for change in [
        json!({"advisory_only":false}),
        json!({"unknown":true}),
        json!({"books":[{"volume":"vol1","passed":true,"pages_reviewed":[2,1]},{"volume":"vol2","passed":true,"pages_reviewed":[1]}]}),
    ] {
        let c = Case::new();
        let p = c.path.join(release::VISUAL_REVIEW_PATH);
        let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        for (k, value) in change.as_object().unwrap() {
            v[k] = value.clone();
        }
        fs::write(&p, release::canonical_bytes(&v).unwrap()).unwrap();
        let o = c.options("denied");
        assert!(evidence::observe_pdf_qa(&c.root, &o).is_err());
        assert!(!o.output.exists());
    }
}
#[test]
fn embedding_flag_is_emb_not_sub_or_uni_and_invalid_utf8_cannot_pass() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/pdffonts"),
        "#!/bin/sh\nprintf 'name type encoding emb sub uni object ID\\n----\\nType1Font Type 1 Standard no yes yes 1 0\\n'\n",
    );
    let d = evidence::observe_pdf_qa(&c.root, &c.options("unembedded")).unwrap();
    assert_eq!(d["pdf_qa"]["passed"], false);
    assert_eq!(
        d["pdf_qa"]["observation"]["books"][0]["fonts_embedded"],
        false
    );
    script(
        &c.path.join("var/tools/pdftotext"),
        "#!/bin/sh\nprintf '\\377'\n",
    );
    let d = evidence::observe_pdf_qa(&c.root, &c.options("badtext")).unwrap();
    assert_eq!(d["pdf_qa"]["passed"], false);
}
#[test]
fn raw_stream_and_summary_tampering_are_not_accepted() {
    let c = Case::new();
    let docs = evidence::observe_pdf_qa(&c.root, &c.options("original")).unwrap();
    let d = &docs["pdf_qa"];
    let mut forged = d.clone();
    forged["observation"]["books"][0]["text_characters"] = json!(9000);
    assert!(evidence::validate_document(&c.root, "pdf_qa", &forged).is_err());
    let raw = c.raw(d);
    let stream = c
        .path
        .join(raw["processes"][0]["stdout"]["path"].as_str().unwrap());
    fs::write(stream, b"Pages: 999\n").unwrap();
    assert!(evidence::validate_document(&c.root, "pdf_qa", d).is_err());
}
#[test]
fn source_and_tool_changes_invalidate_replay_and_do_not_update_criteria_to_green() {
    let c = Case::new();
    let old = evidence::observe_pdf_qa(&c.root, &c.options("old")).unwrap();
    fs::write(
        c.path.join(release::BOOK_SPECS[0].pdf),
        b"changed original\n",
    )
    .unwrap();
    let fresh = evidence::observe_pdf_qa(&c.root, &c.options("new")).unwrap();
    assert!(evidence::compare_replay(&c.root, "pdf_qa", &old["pdf_qa"], &fresh["pdf_qa"]).is_err());
    script(
        &c.path.join("var/tools/pdfinfo"),
        "#!/bin/sh\nprintf 'Pages: 3\\n'\n",
    );
    assert!(evidence::validate_document(&c.root, "pdf_qa", &fresh["pdf_qa"]).is_err());
}
#[test]
fn no_symlink_or_outside_output_and_existing_output_are_adopted() {
    let c = Case::new();
    let mut o = c.options("safe");
    o.output = c.path.join("references/new-output");
    assert!(evidence::observe_pdf_qa(&c.root, &o).is_err());
    let o = c.options("existing");
    fs::create_dir_all(&o.output).unwrap();
    assert!(evidence::observe_pdf_qa(&c.root, &o).is_err());
    let pdf = c.path.join(release::BOOK_SPECS[0].pdf);
    fs::remove_file(&pdf).unwrap();
    symlink(c.path.join(release::BOOK_SPECS[1].pdf), pdf).unwrap();
    assert!(evidence::observe_pdf_qa(&c.root, &c.options("symlink")).is_err());
}
#[test]
fn both_declared_regressions_execute_and_preserve_all_cargo_summaries() {
    let c = Case::new();
    let d = evidence::collect_regressions(&c.root, &c.options("regressions")).unwrap();
    for kind in ["ontology_engineering_regression", "semantica_regression"] {
        assert_eq!(d[kind]["passed"], true, "{}", d[kind]);
        evidence::validate_document(&c.root, kind, &d[kind]).unwrap();
        assert!(!d[kind]["execution"]["published_log"].is_null());
    }
    assert_eq!(
        d["ontology_engineering_regression"]["observation"]["counts"]["passed_count"],
        5
    );
    assert_eq!(
        d["ontology_engineering_regression"]["observation"]["counts"]["suite_count"],
        2
    );
    assert_eq!(
        d["ontology_engineering_regression"]["observation"]["counts"]["ignored_count"],
        1
    );
    assert_eq!(
        d["semantica_regression"]["observation"]["counts"]["passed_count"],
        2
    );
    assert_eq!(
        d["semantica_regression"]["observation"]["command"][4],
        "tests/ontology"
    );
    let again = evidence::observe_regressions(&c.root, &c.options("again")).unwrap();
    for kind in ["ontology_engineering_regression", "semantica_regression"] {
        evidence::compare_replay(&c.root, kind, &d[kind], &again[kind]).unwrap();
    }
}
#[test]
fn failed_suite_retains_partial_stdout_and_still_runs_other_declared_suite() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/python"),
        "#!/bin/sh\nprintf '1 failed, 2 passed in 0.01s\\n'\nexit 1\n",
    );
    let d = evidence::observe_regressions(&c.root, &c.options("failed")).unwrap();
    assert_eq!(d["semantica_regression"]["passed"], false);
    assert_eq!(d["ontology_engineering_regression"]["passed"], true);
    assert_eq!(d["semantica_regression"]["observation"]["return_code"], 1);
    assert!(
        c.raw(&d["semantica_regression"])["processes"]
            .as_array()
            .unwrap()
            .len()
            == 2
    );
}
#[test]
fn changed_checkout_during_suite_blocks_both_after_preserving_executions() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/python"),
        "#!/bin/sh\nprintf 'changed\\n' >> source.txt\nprintf '2 passed in 0.01s\\n'\n",
    );
    let d = evidence::observe_regressions(&c.root, &c.options("drift")).unwrap();
    for kind in ["semantica_regression", "ontology_engineering_regression"] {
        assert_eq!(d[kind]["passed"], false);
        assert_eq!(c.raw(&d[kind])["processes"].as_array().unwrap().len(), 2);
    }
}
#[test]
fn filtered_owned_suite_is_not_a_full_workspace_regression() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/cargo"),
        "#!/bin/sh\nprintf 'test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.01s\\n'\n",
    );
    let d = evidence::observe_regressions(&c.root, &c.options("filtered")).unwrap();
    assert_eq!(d["ontology_engineering_regression"]["passed"], false);
}
#[test]
fn nonzero_process_with_green_summary_and_missing_summary_never_pass() {
    for script_text in [
        "#!/bin/sh\nprintf '2 passed in 0.01s\\n'\nexit 3\n",
        "#!/bin/sh\nprintf 'not actually a suite\\n'\n",
    ] {
        let c = Case::new();
        script(&c.path.join("var/tools/python"), script_text);
        let d = evidence::observe_regressions(&c.root, &c.options("bad")).unwrap();
        assert_eq!(d["semantica_regression"]["passed"], false);
    }
}
#[test]
fn stale_commit_and_missing_explicit_source_reject_before_suite() {
    let c = Case::new();
    let mut o = c.options("missing");
    o.oe_source_commit = None;
    assert!(evidence::observe_regressions(&c.root, &o).is_err());
    assert!(!o.output.exists());
    let mut o = c.options("stale");
    o.oe_source_commit = Some("0".repeat(40));
    assert!(evidence::observe_regressions(&c.root, &o).is_err());
    assert!(!o.output.join("semantica_regression").exists());
}
#[test]
fn process_limits_preserve_raw_capture_and_fail_closed() {
    let c = Case::new();
    let mut o = c.options("limited");
    o.stream_limit = 100;
    let d = evidence::observe_pdf_qa(&c.root, &o).unwrap();
    assert_eq!(d["pdf_qa"]["passed"], false);
    let r = c.raw(&d["pdf_qa"]);
    assert!(r["processes"].as_array().unwrap().iter().any(|p| {
        let v: Value = serde_json::from_slice(
            &fs::read(c.path.join(p["report"]["path"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        v["status"] == "output_limit"
    }));
}

#[test]
fn missing_authoring_and_uninstalled_runtime_are_retained_blocked_not_empty_success() {
    let c = Case::new();
    let docs = evidence::observe_static(&c.root, &c.options("static")).unwrap();
    for kind in ["authoring_locks", "runtime_identity"] {
        assert_eq!(docs[kind]["passed"], false, "{docs}");
        assert!(!c.raw(&docs[kind])["errors"].as_array().unwrap().is_empty());
        evidence::validate_document(&c.root, kind, &docs[kind]).unwrap();
    }
    assert_eq!(
        docs["privacy"]["observation"]["scope"],
        "git_tracked_and_unignored"
    );
    evidence::validate_document(&c.root, "privacy", &docs["privacy"]).unwrap();
    let books = evidence::observe_book_bindings(&c.root, &c.options("bindings")).unwrap();
    assert_eq!(books["book_source_bindings"]["passed"], false);
    evidence::validate_document(
        &c.root,
        "book_source_bindings",
        &books["book_source_bindings"],
    )
    .unwrap();
}

#[test]
fn changing_shared_access_lock_during_external_execution_retains_blocked_receipts() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/pdfinfo"),
        "#!/bin/sh\n/bin/mv var/state/runtime-access.lock var/state/runtime-access.previous\nprintf replacement > var/state/runtime-access.lock\nprintf 'Pages: 2\\n'\n",
    );
    let docs = evidence::observe_pdf_qa(&c.root, &c.options("lock-drift")).unwrap();
    assert_eq!(docs["pdf_qa"]["passed"], false);
    let raw = c.raw(&docs["pdf_qa"]);
    assert!(
        raw["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|error| error["phase"] == "runtime_access")
    );
    assert_eq!(raw["processes"].as_array().unwrap().len(), 6);
}

#[test]
fn deselected_external_suite_cannot_claim_complete_regression() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/python"),
        "#!/bin/sh\nprintf '2 passed, 1 deselected in 0.01s\\n'\n",
    );
    let docs = evidence::observe_regressions(&c.root, &c.options("deselected")).unwrap();
    assert_eq!(docs["semantica_regression"]["passed"], false);
    assert_eq!(docs["ontology_engineering_regression"]["passed"], true);
}

#[test]
fn long_pytest_duration_preserves_all_counts_and_still_rejects_failures() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/python"),
        "#!/bin/sh\nprintf '238 passed, 3 skipped in 125.83s (0:02:05)\\n'\n",
    );
    let docs = evidence::observe_regressions(&c.root, &c.options("long-duration")).unwrap();
    assert_eq!(docs["semantica_regression"]["passed"], true);
    assert_eq!(
        docs["semantica_regression"]["observation"]["counts"]["passed_count"],
        238
    );
    assert_eq!(
        docs["semantica_regression"]["observation"]["counts"]["ignored_count"],
        3
    );
    script(
        &c.path.join("var/tools/python"),
        "#!/bin/sh\nprintf '1 failed, 238 passed in 125.83s (0:02:05)\\n'\n",
    );
    let docs = evidence::observe_regressions(&c.root, &c.options("long-failed")).unwrap();
    assert_eq!(docs["semantica_regression"]["passed"], false);
}

#[test]
fn normalized_logs_change_only_explicit_checkout_prefixes_and_retain_raw() {
    let c = Case::new();
    script(
        &c.path.join("var/tools/python"),
        "#!/bin/sh\nprintf '%s/source.txt\\n' \"$PWD\"\nprintf 'unrelated-relative-path unchanged\\n2 passed in 0.01s\\n'\n",
    );
    let docs = evidence::collect_regressions(&c.root, &c.options("normalized")).unwrap();
    let doc = &docs["semantica_regression"];
    assert_eq!(doc["passed"], true);
    let log = fs::read_to_string(
        c.path
            .join(doc["observation"]["log"]["path"].as_str().unwrap()),
    )
    .unwrap();
    assert!(log.contains("<semantica-root>/source.txt"));
    assert!(log.contains("unrelated-relative-path unchanged"));
    assert!(!log.contains(c.path.to_str().unwrap()));
    let raw = c.raw(doc);
    let process = raw["processes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "semantica_regression")
        .unwrap();
    let original =
        fs::read_to_string(c.path.join(process["stdout"]["path"].as_str().unwrap())).unwrap();
    assert!(original.contains(c.upstream.to_str().unwrap()));
    evidence::validate_document(&c.root, "semantica_regression", doc).unwrap();
}

fn minimal_pdf() -> Vec<u8> {
    let text = "Engineering evidence fixture requires real inspection. ".repeat(2);
    let mut commands = String::from("BT /F1 8 Tf 40 760 Td 12 TL\n");
    for _ in 0..20 {
        commands.push_str(&format!("({text}) Tj T*\n"));
    }
    commands.push_str("ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{commands}endstream", commands.len()),
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

#[test]
#[ignore = "actual Poppler on two synthetic PDFs; retains private receipts, no original books or visual-pass claim"]
fn actual_poppler_retains_all_six_calls_without_inventing_embedding_or_visual_pass() {
    let mut c = Case::new();
    for book in release::BOOK_SPECS {
        fs::write(c.path.join(book.pdf), minimal_pdf()).unwrap();
    }
    c.visual(json!([1]), false);
    let mut options = c.options("actual-poppler");
    options.pdfinfo = "pdfinfo".into();
    options.pdffonts = "pdffonts".into();
    options.pdftotext = "pdftotext".into();
    let docs = evidence::observe_pdf_qa(&c.root, &options).unwrap();
    let doc = &docs["pdf_qa"];
    assert_eq!(doc["passed"], false);
    for book in doc["observation"]["books"].as_array().unwrap() {
        assert_eq!(book["page_count"], 1);
        assert_eq!(book["fonts_embedded"], false);
        assert_eq!(book["visual_passed"], false);
        assert!(book["text_characters"].as_u64().unwrap() > 1000);
    }
    assert_eq!(c.raw(doc)["processes"].as_array().unwrap().len(), 6);
    evidence::validate_document(&c.root, "pdf_qa", doc).unwrap();
    fs::write(c.path.join("var/actual-poppler-acceptance.json"),serde_json::to_vec_pretty(&json!({"scope":"actual Poppler on synthetic PDFs, no native or model calls, no release claim","evidence":doc})).unwrap()).unwrap();
    eprintln!("retained actual Poppler fixture: {}", c.path.display());
    c.retain = true;
}

#[test]
#[ignore = "actual installed locked runtime and both original book source closures, read only; no regressions or publication"]
fn actual_static_and_native_book_bindings_keep_info_checks_and_full_native_result() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let root = SkillRoot::new(&path).unwrap();
    let output = path.join(format!(
        "var/maintenance/rust-rewrite-20260927/phase10-evidence/actual-native-{}",
        std::process::id()
    ));
    let options = evidence::EvidenceOptions::new(output.join("static"));
    let docs = evidence::observe_static(&root, &options).unwrap();
    let identity = &docs["runtime_identity"];
    assert_eq!(identity["passed"], true, "{identity}");
    assert!(
        identity["observation"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["level"] == "info" && c["code"] == "offline_boundary")
    );
    assert_eq!(identity["observation"]["semantic_execution"], "not_run");
    for kind in ["authoring_locks", "runtime_identity", "privacy"] {
        evidence::validate_document(&root, kind, &docs[kind]).unwrap();
    }
    let bindings = evidence::observe_book_bindings(
        &root,
        &evidence::EvidenceOptions::new(output.join("bindings")),
    )
    .unwrap();
    let doc = &bindings["book_source_bindings"];
    assert_eq!(doc["passed"], true, "{doc}");
    assert_eq!(doc["observation"]["native_execution"], "completed");
    assert!(
        doc["observation"]["report"]["check_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    evidence::validate_document(&root, "book_source_bindings", doc).unwrap();
    fs::write(output.join("acceptance.json"),serde_json::to_vec_pretty(&json!({"scope":"actual read-only source-locked runtime identity and native book bindings; full suite regressions not executed; privacy is independently observed, no publication","static":docs,"bindings":bindings})).unwrap()).unwrap();
    eprintln!(
        "retained actual runtime/book evidence: {}",
        output.display()
    );
}

/// The approved archive is real immutable data. Authoring files here are
/// synthetic mismatches, so this proves real Rust execution plus blocked
/// engineering conditions, never a release or upstream-runtime pass.
#[test]
fn actual_rust_book_checks_are_completed_and_bound_without_legacy_execution() {
    let c = Case::new();
    let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let archive = oe_semantic::books::rust::ARCHIVE;
    let bytes = fs::read(producer.join(archive)).unwrap();
    assert_eq!(
        sha256(&bytes),
        oe_semantic::books::rust::APPROVED_ARCHIVE_SHA256
    );
    let target = c.path.join(archive);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, &bytes).unwrap();
    fs::write(
        c.path.join("sources/continuity.json"),
        serde_json::to_vec(&json!({
            "source_files":[{"path":archive.trim_start_matches("sources/"),"sha256":sha256(&bytes)}]
        }))
        .unwrap(),
    )
    .unwrap();
    let docs =
        evidence::observe_book_bindings(&c.root, &c.options("actual-rust-bindings")).unwrap();
    let doc = &docs["book_source_bindings"];
    assert_eq!(doc["passed"], false);
    let raw = c.raw(doc);
    assert_eq!(raw["data"]["report"]["semantic_execution"], "completed");
    assert_eq!(raw["data"]["report"]["result"]["check_count"], 238);
    assert_eq!(
        doc["observation"]["semantic_execution"], "completed",
        "real Rust checker results must not disappear behind the retired native_execution protocol"
    );
    assert_eq!(
        doc["observation"]["report"],
        raw["data"]["report"]["result"]
    );
    assert_eq!(doc["observation"]["status"], "blocked");
    assert_eq!(raw["data"]["report"]["fact_admitted"], false);
    evidence::validate_document(&c.root, "book_source_bindings", doc).unwrap();
}

/// Missing immutable data is a prerequisite failure, separate from an actual
/// checker that ran and reported missing authoring conditions.
#[test]
fn missing_approved_archive_stays_not_run_with_complete_rust_identity() {
    let c = Case::new();
    let docs = evidence::observe_book_bindings(&c.root, &c.options("missing-approved")).unwrap();
    let doc = &docs["book_source_bindings"];
    assert_eq!(doc["passed"], false);
    let raw = c.raw(doc);
    let response = &raw["data"]["report"];
    assert_eq!(response["status"], "blocked");
    assert_eq!(response["semantic_execution"], "not_run");
    assert_eq!(response["result"]["check_count"], 0);
    assert_eq!(response["result"]["status"], "not_run");
    assert_eq!(
        response["rust_execution"]["source_archive_sha256"],
        Value::Null
    );
    assert_eq!(
        response["rust_execution"]["approved_archive_sha256"],
        oe_semantic::books::rust::APPROVED_ARCHIVE_SHA256
    );
    assert_eq!(
        response["rust_execution"]["executable_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        raw["source_identity"]["inputs"][oe_semantic::books::rust::ARCHIVE],
        Value::Null
    );
    assert_eq!(response["fact_admitted"], false);
    assert!(raw["errors"].as_array().unwrap().is_empty());
    evidence::validate_document(&c.root, "book_source_bindings", doc).unwrap();
}

#[test]
fn book_binding_gate_rejects_removed_checks_and_identity_drift() {
    let c = Case::new();
    let producer = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let archive = oe_semantic::books::rust::ARCHIVE;
    let bytes = fs::read(producer.join(archive)).unwrap();
    let target = c.path.join(archive);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, &bytes).unwrap();
    fs::write(
        c.path.join("sources/continuity.json"),
        serde_json::to_vec(&json!({
            "source_files":[{"path":archive.trim_start_matches("sources/"),"sha256":sha256(&bytes)}]
        }))
        .unwrap(),
    )
    .unwrap();
    let docs = evidence::observe_book_bindings(&c.root, &c.options("anti-erasure")).unwrap();
    let original = docs["book_source_bindings"].clone();
    let raw_path = original["execution"]["raw"]["path"].as_str().unwrap();
    let raw_bytes = fs::read(c.path.join(raw_path)).unwrap();
    let raw: Value = serde_json::from_slice(&raw_bytes).unwrap();
    assert_eq!(raw["data"]["report"]["result"]["check_count"], 238);

    // Both digest layers and the observed projection are updated. An error
    // from the unchanged artifact hash would not prove the semantic gate.
    for (mode, expected) in [
        ("remove_check", "book_source_report_not_reproduced"),
        ("executable", "book_execution_executable_changed"),
        ("implementation", "book_execution_implementation_changed"),
        (
            "approved_archive",
            "book_execution_approved_archive_changed",
        ),
        ("legacy_as_rust", "rust_book_execution_required"),
    ] {
        let mut modified = raw.clone();
        match mode {
            "remove_check" => {
                let digest = {
                    let result = &mut modified["data"]["report"]["result"];
                    result["checks"].as_array_mut().unwrap().pop();
                    result["check_count"] = json!(237);
                    sha256(oe_semantic::canonical_json(result).unwrap())
                };
                modified["data"]["report"]["rust_execution"]["result_sha256"] = json!(digest);
            }
            "executable" => {
                modified["data"]["report"]["rust_execution"]["executable_sha256"] =
                    json!("f".repeat(64))
            }
            "implementation" => {
                modified["data"]["report"]["rust_execution"]["implementation_sources"][0]["sha256"] =
                    json!("f".repeat(64))
            }
            "approved_archive" => {
                modified["data"]["report"]["rust_execution"]["approved_archive_sha256"] =
                    json!("f".repeat(64))
            }
            "legacy_as_rust" => {
                modified["data"]["report"]
                    .as_object_mut()
                    .unwrap()
                    .remove("rust_execution");
                modified["data"]["report"]["native_execution"] = json!({
                    "semantic_execution":"completed","runtime_identity":{
                        "source_lock":modified["source_identity"]["runtime"],
                        "record_files":1,"staging_descriptor_sha256":null},
                    "result":modified["data"]["report"]["result"],"error":null});
            }
            _ => unreachable!(),
        }
        let mut document = original.clone();
        fs::write(
            c.path.join(raw_path),
            serde_json::to_vec_pretty(&modified).unwrap(),
        )
        .unwrap();
        document["execution"]["raw"] = release::artifact_ref(&c.root, raw_path).unwrap();
        document["observation"]["report"] = modified["data"]["report"]["result"].clone();
        document["observation"]["rust_execution"] =
            modified["data"]["report"]["rust_execution"].clone();
        let err = format!(
            "{:#}",
            evidence::validate_document(&c.root, "book_source_bindings", &document).unwrap_err()
        );
        assert!(err.contains(expected), "{mode}: {err}");
    }
    fs::write(c.path.join(raw_path), &raw_bytes).unwrap();
    evidence::validate_document(&c.root, "book_source_bindings", &original).unwrap();

    // Missing source creation, code drift and archive drift are physical
    // changes. The original receipts remain unchanged and cease to replay.
    let missing = raw["data"]["report"]["rust_execution"]["input_bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["sha256"].is_null() && i["path"].as_str().unwrap().starts_with("references/"))
        .unwrap();
    let missing_path = c.path.join(missing["path"].as_str().unwrap());
    // The checker distinguishes absent parents from absent files. Restore
    // every directory created by this mutation so the original environment
    // must replay exactly, including its original missing-source reasons.
    let mut created_parents = Vec::new();
    let mut parent = missing_path.parent().unwrap();
    while !parent.exists() {
        assert!(parent.starts_with(&c.path));
        created_parents.push(parent.to_path_buf());
        parent = parent.parent().unwrap();
    }
    fs::create_dir_all(missing_path.parent().unwrap()).unwrap();
    fs::write(&missing_path, b"new source after observation").unwrap();
    assert!(
        format!(
            "{:#}",
            evidence::validate_document(&c.root, "book_source_bindings", &original).unwrap_err()
        )
        .contains("evidence_missing_input_created")
    );
    fs::remove_file(&missing_path).unwrap();
    for parent in created_parents {
        fs::remove_dir(parent).unwrap();
    }
    let code_path = c.path.join("crates/semantica-runtime/src/books.rs");
    let code_bytes = fs::read(&code_path).unwrap();
    fs::write(
        &code_path,
        [
            code_bytes.as_slice(),
            b"\n// source changed after observation\n",
        ]
        .concat(),
    )
    .unwrap();
    assert!(evidence::validate_document(&c.root, "book_source_bindings", &original).is_err());
    fs::write(&code_path, &code_bytes).unwrap();
    let mut changed_archive = bytes.clone();
    let end = changed_archive
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .unwrap();
    let comment = b"valid changed archive after observation";
    changed_archive[end + 20..end + 22].copy_from_slice(&(comment.len() as u16).to_le_bytes());
    changed_archive.truncate(end + 22);
    changed_archive.extend_from_slice(comment);
    zip::ZipArchive::new(std::io::Cursor::new(&changed_archive)).unwrap();
    fs::write(&target, changed_archive).unwrap();
    assert!(evidence::validate_document(&c.root, "book_source_bindings", &original).is_err());
    fs::write(&target, &bytes).unwrap();
    evidence::validate_document(&c.root, "book_source_bindings", &original).unwrap();
}
