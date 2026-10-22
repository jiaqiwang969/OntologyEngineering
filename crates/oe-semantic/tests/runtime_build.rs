//! Isolated orchestration fixtures use real Git/tar and explicitly fake npm and
//! Python build tools. They are not a claim of an actual Semantica wheel build.
#![cfg(unix)]
use oe_core::{SkillRoot, sha256};
use oe_semantic::runtime_build::{
    BuildOptions, build_semantica, capture_historical_frontend_archive_evidence,
    extract_source_archive, inspect_source, read_build_spec,
};
use serde_json::{Value, json};
use std::fs;
use std::io::{Cursor, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zip::write::SimpleFileOptions;

static NEXT: AtomicU64 = AtomicU64::new(0);
const WHEEL: &str = "semantica-9.9.9+staging.1-py3-none-any.whl";
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
struct Fixture {
    path: PathBuf,
    root: SkillRoot,
    source: PathBuf,
    lock: Value,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn git(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Isolated fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Isolated fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
fn executable(path: &Path, script: &str) {
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
impl Fixture {
    fn new() -> Self {
        let path = root().join("var/builds/runtime-build-tests").join(format!(
            "fixture space {}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("runtime")).unwrap();
        fs::create_dir_all(path.join("var")).unwrap();
        fs::write(path.join("SKILL.md"), "fixture only\n").unwrap();
        let source = path.join("upstream source");
        fs::create_dir_all(source.join("explorer")).unwrap();
        fs::write(source.join("pyproject.toml"), "[project]\nname='fixture'\n").unwrap();
        fs::write(source.join("explorer/package-lock.json"), "{}\n").unwrap();
        git(&source, &["init", "--quiet"]);
        git(&source, &["add", "."]);
        git(&source, &["commit", "--quiet", "-m", "isolated fixture"]);
        let commit = git(&source, &["rev-parse", "HEAD"]).trim().to_owned();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in [
            (
                "semantica/__init__.py",
                "__version__ = \"9.9.9+staging.1\"\n",
            ),
            (
                "semantica-9.9.9+staging.1.dist-info/METADATA",
                "Metadata-Version: 2.4\nName: Semantica\nVersion: 9.9.9+staging.1\nRequires-Python: >=3.10\n\nfixture\n",
            ),
            (
                "semantica-9.9.9+staging.1.dist-info/WHEEL",
                "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
            ),
            (
                "semantica-9.9.9+staging.1.dist-info/RECORD",
                "semantica/__init__.py,sha256=2CJkpqbOcvl8V5H5C1UkPHYLYdjf62hJ_6pnPx8eIJc,32\r\nsemantica-9.9.9+staging.1.dist-info/RECORD,,\r\n",
            ),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        let wheel = zip.finish().unwrap().into_inner();
        fs::write(path.join("fixture.whl"), &wheel).unwrap();
        let lock = json!({"$schema":"ontology-engineering.semantica-source-lock/v1",
            "source":{"commit":commit,"version":"9.9.9+staging.1"},
            "artifact":{"filename":WHEEL,"sha256":sha256(&wheel)},
            "locked_inputs":{"pyproject_sha256":sha256(fs::read(source.join("pyproject.toml")).unwrap()),
                "explorer_package_lock_sha256":sha256(b"{}\n")},
            "build":{"source_date_epoch":1790523626_u64,"python_builder":"build==1.3.0",
                "setuptools":"setuptools==84.0.0","wheel":"wheel==0.48.0",
                "frontend_steps":["npm ci","npm run test:graph-store","npm run test:graph-workspace","npm run test:plugin-registry","npm run build"],
                "python_step":"python -m build --wheel --no-isolation"}});
        fs::write(
            path.join("runtime/semantica-source-lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        )
        .unwrap();
        executable(
            &path.join("fake-npm"),
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> fixture-steps.txt\n",
        );
        executable(
            &path.join("fake-python"),
            &format!(
                "#!/bin/sh\nset -eu\nif [ \"$1\" = --version ]; then echo fixture-python; exit 0; fi\nif [ \"$1\" = -m ] && [ \"$2\" = venv ]; then mkdir -p \"$3/bin\"; cp \"$0\" \"$3/bin/python\"; exit 0; fi\nif [ \"$1\" = -m ] && [ \"$2\" = pip ]; then printf '%s\\n' \"$@\"; exit 0; fi\nif [ \"$1\" = -m ] && [ \"$2\" = build ]; then test \"$SOURCE_DATE_EPOCH\" = 1790523626; test \"$PYTHONHASHSEED\" = 0; cp {} \"$6/{WHEEL}\"; exit 0; fi\nexit 65\n",
                quote(&path.join("fixture.whl"))
            ),
        );
        executable(
            &path.join("node"),
            "#!/bin/sh\nprintf 'v0.0.0-fixture\\n'\n",
        );
        Self {
            root: SkillRoot::new(&path).unwrap(),
            path,
            source,
            lock,
        }
    }
    fn options(&self, name: &str) -> BuildOptions {
        BuildOptions {
            source_root: self.source.clone(),
            output_dir: self.path.join("var").join(name),
            python: self.path.join("fake-python"),
            git: "git".into(),
            npm: self.path.join("fake-npm"),
            node: self.path.join("node"),
            wheelhouse: None,
            npm_offline: false,
        }
    }
    fn save_lock(&self) {
        fs::write(
            self.path.join("runtime/semantica-source-lock.json"),
            serde_json::to_vec(&self.lock).unwrap(),
        )
        .unwrap();
    }
}
fn blocked(value: &Value, reason: &str) {
    assert_eq!(value["status"], "blocked", "{value}");
    assert!(value["error"].as_str().unwrap().contains(reason), "{value}");
}

#[test]
fn strict_lock_rejects_duplicate_fields_unpinned_tools_and_missing_frontend_tests() {
    let mut f = Fixture::new();
    assert_eq!(
        read_build_spec(&f.root).unwrap().build_requirements.len(),
        3
    );
    f.lock["build"]["python_builder"] = json!("build>=1.3.0");
    f.save_lock();
    assert!(read_build_spec(&f.root).is_err());
    f.lock["build"]["python_builder"] = json!("build==1.3.0");
    f.lock["build"]["frontend_steps"]
        .as_array_mut()
        .unwrap()
        .remove(2);
    f.save_lock();
    assert!(read_build_spec(&f.root).is_err());
    fs::write(
        f.path.join("runtime/semantica-source-lock.json"),
        b"{\"build\":{},\"build\":{}}",
    )
    .unwrap();
    assert!(read_build_spec(&f.root).is_err());
}

#[test]
fn inspection_requires_exact_clean_full_source_and_never_creates_environment() {
    let f = Fixture::new();
    let options = f.options("inspection");
    let before = fs::read(f.source.join("pyproject.toml")).unwrap();
    let report = inspect_source(&f.root, &options).unwrap();
    assert_eq!(report["status"], "ready", "{report}");
    assert_eq!(report["result"]["full_fsck_executed"], true);
    assert!(!options.output_dir.join("build-venv").exists());
    assert_eq!(fs::read(f.source.join("pyproject.toml")).unwrap(), before);
    assert_eq!(
        git(
            &f.source,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        ""
    );
    assert!(inspect_source(&f.root, &options).is_err());
}

#[test]
fn dirty_wrong_commit_sparse_and_partial_inputs_stop_before_any_build() {
    let mut f = Fixture::new();
    fs::write(f.source.join("untracked"), "untouched").unwrap();
    blocked(
        &capture_historical_frontend_archive_evidence(
            &f.root,
            &f.options("dirty"),
            &f.path.join("fixture.whl"),
            &f.path.join("fixture.whl"),
        )
        .unwrap(),
        "build_source_dirty",
    );
    fs::remove_file(f.source.join("untracked")).unwrap();
    let commit = f.lock["source"]["commit"].clone();
    f.lock["source"]["commit"] = json!("1".repeat(40));
    f.save_lock();
    blocked(
        &capture_historical_frontend_archive_evidence(
            &f.root,
            &f.options("wrong"),
            &f.path.join("fixture.whl"),
            &f.path.join("fixture.whl"),
        )
        .unwrap(),
        "build_source_commit_mismatch",
    );
    f.lock["source"]["commit"] = commit;
    f.save_lock();
    git(&f.source, &["config", "core.sparseCheckout", "true"]);
    blocked(
        &capture_historical_frontend_archive_evidence(
            &f.root,
            &f.options("sparse"),
            &f.path.join("fixture.whl"),
            &f.path.join("fixture.whl"),
        )
        .unwrap(),
        "build_source_sparse",
    );
    git(&f.source, &["config", "core.sparseCheckout", "false"]);
    git(&f.source, &["config", "remote.origin.promisor", "true"]);
    blocked(
        &capture_historical_frontend_archive_evidence(
            &f.root,
            &f.options("partial"),
            &f.path.join("fixture.whl"),
            &f.path.join("fixture.whl"),
        )
        .unwrap(),
        "build_source_partial_clone",
    );
    for name in ["dirty", "wrong", "sparse", "partial"] {
        assert!(!f.path.join("var").join(name).join("build-venv").exists());
    }
}

#[test]
fn recorded_input_hash_mismatch_is_not_reinterpreted_as_source_qualification() {
    let mut f = Fixture::new();
    f.lock["locked_inputs"]["pyproject_sha256"] = json!("f".repeat(64));
    f.save_lock();
    blocked(
        &capture_historical_frontend_archive_evidence(
            &f.root,
            &f.options("hash"),
            &f.path.join("fixture.whl"),
            &f.path.join("fixture.whl"),
        )
        .unwrap(),
        "build_pyproject_hash_mismatch",
    );
}

#[test]
fn complete_fixture_build_preserves_five_npm_steps_two_wheels_and_raw_observations() {
    let f = Fixture::new();
    let options = f.options("full");
    let lock = fs::read(f.path.join("runtime/semantica-source-lock.json")).unwrap();
    let report = capture_historical_frontend_archive_evidence(
        &f.root,
        &options,
        &f.path.join("fixture.whl"),
        &f.path.join("fixture.whl"),
    )
    .unwrap();
    assert_eq!(report["status"], "completed", "{report}");
    assert_eq!(report["result"]["provided_original_archives"], 2);
    assert_eq!(report["result"]["frontend_builds"], 1);
    assert_eq!(report["semantic_execution"], "not_run");
    assert_eq!(
        fs::read(options.output_dir.join(WHEEL)).unwrap(),
        fs::read(f.path.join("fixture.whl")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(options.output_dir.join("source/explorer/fixture-steps.txt")).unwrap(),
        "ci\nrun test:graph-store\nrun test:graph-workspace\nrun test:plugin-registry\nrun build\n"
    );
    for step in report["steps"].as_array().unwrap() {
        let p = Path::new(step["evidence_dir"].as_str().unwrap());
        assert!(p.join("process.json").is_file());
        assert!(p.join("stdout").is_file());
        assert!(p.join("stderr").is_file());
    }
    assert_eq!(
        fs::read(f.path.join("runtime/semantica-source-lock.json")).unwrap(),
        lock
    );
    assert_eq!(
        git(
            &f.source,
            &["status", "--porcelain", "--untracked-files=all"]
        ),
        ""
    );
    assert!(!f.path.join("runtime/.venv").exists());
}

#[test]
fn frontend_failure_retains_prior_output_and_does_not_run_later_steps() {
    let f = Fixture::new();
    executable(
        &f.path.join("fake-npm"),
        "#!/bin/sh\nprintf 'native fixture %s\\n' \"$*\"\nif [ \"$*\" = 'run test:graph-workspace' ]; then printf 'fixture failure\\n' >&2; exit 37; fi\n",
    );
    let options = f.options("failed");
    let report = capture_historical_frontend_archive_evidence(
        &f.root,
        &options,
        &f.path.join("fixture.whl"),
        &f.path.join("fixture.whl"),
    )
    .unwrap();
    blocked(
        &report,
        "build_step_failed:npm-test-graph-workspace:process_failed",
    );
    let steps = report["steps"].as_array().unwrap();
    assert_eq!(steps.last().unwrap()["exit_code"], 37);
    let failed = Path::new(steps.last().unwrap()["evidence_dir"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(failed.join("stderr")).unwrap(),
        "fixture failure\n"
    );
    assert!(!options.output_dir.join("build-venv").exists());
    assert!(!options.output_dir.join(WHEEL).exists());
}

#[test]
fn copied_artifact_requires_two_identical_locked_wheels() {
    let f = Fixture::new();
    let wrong = f.path.join("wrong-original.whl");
    let mut bytes = fs::read(f.path.join("fixture.whl")).unwrap();
    bytes.extend_from_slice(b"drift");
    fs::write(&wrong, bytes).unwrap();
    let options = f.options("mismatch");
    let report = capture_historical_frontend_archive_evidence(
        &f.root,
        &options,
        &f.path.join("fixture.whl"),
        &wrong,
    )
    .unwrap();
    blocked(&report, "historical_archive_reproduction_mismatch");
    assert!(!options.output_dir.join(WHEEL).exists());
    assert!(
        options
            .output_dir
            .join("original-archive-a")
            .join(WHEEL)
            .exists()
    );
}

#[test]
fn source_or_lock_drift_during_frontend_execution_blocks_publication() {
    let f = Fixture::new();
    executable(
        &f.path.join("fake-npm"),
        &format!(
            "#!/bin/sh\nprintf changed > {}\n",
            quote(&f.source.join("unexpected"))
        ),
    );
    let options = f.options("drift");
    blocked(
        &capture_historical_frontend_archive_evidence(
            &f.root,
            &options,
            &f.path.join("fixture.whl"),
            &f.path.join("fixture.whl"),
        )
        .unwrap(),
        "build_source_dirty",
    );
    assert!(!options.output_dir.join(WHEEL).exists());
    assert!(
        options
            .output_dir
            .join("original-archive-a")
            .join(WHEEL)
            .exists()
    );
}

#[test]
fn python_wheelhouse_and_npm_offline_are_explicit_separate_policies() {
    let f = Fixture::new();
    let mut options = f.options("offline");
    fs::create_dir(f.path.join("wheelhouse")).unwrap();
    options.wheelhouse = Some(f.path.join("wheelhouse"));
    options.npm_offline = true;
    executable(
        &f.path.join("fake-npm"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo fixture-npm; exit 0; fi\ntest \"$npm_config_offline\" = true\n",
    );
    let report = capture_historical_frontend_archive_evidence(
        &f.root,
        &options,
        &f.path.join("fixture.whl"),
        &f.path.join("fixture.whl"),
    )
    .unwrap();
    assert_eq!(report["status"], "completed", "{report}");
    assert_eq!(
        report["network_policy"]["python_build_dependencies"],
        "wheelhouse_only"
    );
    assert_eq!(report["network_policy"]["npm"], "offline_cache_only");
    assert_eq!(report["python_build_dispatched"], false);
    assert_eq!(report["result"]["python_processes"], 0);
    assert_eq!(
        read_build_spec(&f.root).unwrap().build_requirements,
        vec!["build==1.3.0", "setuptools==84.0.0", "wheel==0.48.0"]
    );
    assert!(
        report["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| !s["step"].as_str().unwrap().contains("python"))
    );
}

fn tar_bytes(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
    let mut out = tar::Builder::new(Vec::new());
    for (name, kind, bytes) in entries {
        let mut h = tar::Header::new_gnu();
        h.set_size(bytes.len() as u64);
        h.set_mode(0o644);
        h.set_entry_type(*kind);
        if kind.is_symlink() || kind.is_hard_link() {
            h.set_link_name("../outside").unwrap();
        }
        h.set_cksum();
        out.append_data(&mut h, name, Cursor::new(bytes)).unwrap();
    }
    out.into_inner().unwrap()
}
#[test]
fn archive_preserves_exact_unicode_bytes_and_refuses_existing_destinations() {
    let f = Fixture::new();
    let bytes = tar_bytes(&[(
        "原理/a.txt",
        tar::EntryType::Regular,
        "e\u{301} 🧪\n".as_bytes(),
    )]);
    let archive = f.path.join("source.tar");
    fs::write(&archive, &bytes).unwrap();
    let dest = f.path.join("var/unpack");
    let result = extract_source_archive(&archive, &dest).unwrap();
    assert_eq!(result["regular_files"], 1);
    assert_eq!(
        fs::read(dest.join("原理/a.txt")).unwrap(),
        "e\u{301} 🧪\n".as_bytes()
    );
    assert!(extract_source_archive(&archive, &dest).is_err());
}
#[test]
fn archive_rejects_links_duplicates_traversal_footer_damage_and_trailing_data() {
    let f = Fixture::new();
    let mut traversal = tar_bytes(&[("safe", tar::EntryType::Regular, b"abc")]);
    traversal[..100].fill(0);
    traversal[..7].copy_from_slice(b"../evil");
    traversal[148..156].fill(b' ');
    let sum: u32 = traversal[..512].iter().map(|v| u32::from(*v)).sum();
    traversal[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    let mut trailing = tar_bytes(&[("a", tar::EntryType::Regular, b"a")]);
    trailing.extend_from_slice(&[42; 512]);
    let mut missing_footer = tar_bytes(&[("a", tar::EntryType::Regular, b"a")]);
    missing_footer.truncate(1024);
    for (i, bytes) in [
        tar_bytes(&[("link", tar::EntryType::Symlink, b"")]),
        tar_bytes(&[("link", tar::EntryType::Link, b"")]),
        tar_bytes(&[
            ("same", tar::EntryType::Regular, b"a"),
            ("same", tar::EntryType::Regular, b"b"),
        ]),
        traversal,
        trailing,
        missing_footer,
    ]
    .into_iter()
    .enumerate()
    {
        let archive = f.path.join(format!("bad-{i}.tar"));
        fs::write(&archive, bytes).unwrap();
        assert!(
            extract_source_archive(&archive, &f.path.join(format!("var/bad-{i}"))).is_err(),
            "case {i}"
        );
    }
    assert!(!f.path.join("var/evil").exists());
}

#[test]
fn path_guards_reject_output_in_source_and_symlink_sources_without_touching_target() {
    let f = Fixture::new();
    let mut options = f.options("path");
    options.output_dir = f.source.join("build");
    assert!(build_semantica(&f.root, &options).is_err());
    let linked = f.path.join("linked");
    symlink(&f.source, &linked).unwrap();
    options = f.options("link");
    options.source_root = linked;
    assert!(build_semantica(&f.root, &options).is_err());
    assert!(!f.path.join("var/link").exists());
}

#[test]
#[ignore = "read-only real locked Git source inspection; no npm/pip or environment creation"]
fn actual_locked_checkout_inspection_keeps_original_source_and_runtime_unchanged() {
    let root = SkillRoot::new(root()).unwrap();
    let source = root
        .path()
        .join("var/maintenance/release-0.7.0/semantica-source");
    let output = root
        .path()
        .join("var/maintenance/rust-rewrite-20260927/phase9-build-native")
        .join(format!("actual-source-inspection-{}", std::process::id()));
    let options = BuildOptions::new(source, output);
    let report = inspect_source(&root, &options).unwrap();
    assert_eq!(report["status"], "ready", "{report}");
}

#[test]
#[ignore = "actual locked upstream npm tests and two wheel builds; may use package registries; never installs the result"]
fn actual_locked_source_build_reproduces_both_wheels_without_installation() {
    let root = SkillRoot::new(root()).unwrap();
    let source = root
        .path()
        .join("var/maintenance/release-0.7.0/semantica-source");
    let output = root
        .path()
        .join("var/maintenance/rust-rewrite-20260927/phase9-build-native")
        .join(format!("actual-source-build-{}", std::process::id()));
    let protected = [
        "runtime/semantica-source-lock.json",
        "runtime/requirements.txt",
        "runtime/.venv/pyvenv.cfg",
    ];
    let before: Vec<_> = protected
        .iter()
        .map(|p| (p, fs::read(root.path().join(p)).unwrap()))
        .collect();
    let options = BuildOptions::new(source, output.clone());
    let report = build_semantica(&root, &options).unwrap();
    println!("actual source build evidence: {}", output.display());
    for (p, bytes) in before {
        assert_eq!(
            fs::read(root.path().join(p)).unwrap(),
            bytes,
            "protected {p}"
        );
    }
    assert_eq!(report["status"], "completed", "{report}");
    assert_eq!(
        report["result"]["wheel_identity"]["sha256"],
        read_build_spec(&root).unwrap().source_lock.artifact_sha256
    );
}
