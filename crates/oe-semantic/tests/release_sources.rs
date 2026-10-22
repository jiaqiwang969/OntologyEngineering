//! Isolated Git/wheel fixtures test provenance, never Semantica semantics.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use oe_core::{SkillRoot, sha256};
use oe_semantic::{
    release_sources::*,
    runtime::{read_runtime_source_lock, read_wheel_identity},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zip::write::SimpleFileOptions;
static NEXT: AtomicU64 = AtomicU64::new(0);
const DIST: &str = "semantica-9.9.9+fixture.dist-info";
const WHEEL: &str = "semantica-9.9.9+fixture-py3-none-any.whl";
const MANIFEST: &str = "semantica/chapter_packages/vol1/ch01/manifest.yaml";
const VOL1: &str = "references/ontology-engineering-book/authoring-sources.sha256";
fn installed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .into()
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap()
}
fn fails<T: std::fmt::Debug>(result: anyhow::Result<T>, message: &str) {
    let e = format!("{:#}", result.unwrap_err());
    assert!(e.contains(message), "expected {message:?}: {e}")
}
struct Case {
    root: SkillRoot,
    payload: BTreeMap<String, Vec<u8>>,
    git: PathBuf,
}
impl Case {
    fn new() -> Self {
        let path = installed().join(format!(
            "var/maintenance/rust-tests/release-sources-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        write(&path.join("SKILL.md"), b"isolated source fixture\n");
        write(&path.join(".gitignore"), b"var/\n");
        let mut payload=BTreeMap::from([("semantica/__init__.py".into(),b"__version__='9.9.9+fixture'\n".to_vec()),(format!("{DIST}/METADATA"),b"Metadata-Version: 2.4\nName: semantica\nVersion: 9.9.9+fixture\nRequires-Python: >=3.10\n".to_vec()),(format!("{DIST}/WHEEL"),b"Wheel-Version: 1.0\nTag: py3-none-any\n".to_vec()),(format!("{DIST}/LICENSE"),b"fixture-only\n".to_vec())]);
        for (volume, count) in [("vol1", 9), ("vol2", 20)] {
            for i in 1..=count {
                let chapter = format!("ch{i:02}");
                let prefix = format!("semantica/chapter_packages/{volume}/{chapter}");
                let asset = format!("source-bound fixture {volume}/{chapter}\n").into_bytes();
                payload.insert(format!("{prefix}/a.ttl"), asset.clone());
                payload.insert(format!("{prefix}/manifest.yaml"),serde_json::to_vec(&json!({"package_id":format!("semantica.chapter_packages.{volume}.{chapter}"),"version":"fixture.1","status":"partial","release_status":"blocked","chapter":{"volume":volume,"chapter":chapter},"assets":[{"asset_id":"原理-source","role":"ontology_source","path":"a.ttl","sha256":sha256(asset)}]})).unwrap());
            }
        }
        let case = Self {
            root: SkillRoot::new(path).unwrap(),
            payload,
            git: oe_core::process::resolve_executable(Path::new("git")).unwrap(),
        };
        for p in [
            VOL1,
            "references/product-trustworthiness-book/handbook/current-source.sha256",
            "references/product-trustworthiness-book/handbook/authoring-assets.sha256",
            "references/product-trustworthiness-book/handbook/formal-search-guides.sha256",
        ] {
            write(&case.root.path().join(p), b"fixture authoring lock bytes\n")
        }
        case
    }
    fn mutate_manifest(&mut self, f: impl FnOnce(&mut Value)) {
        let mut d: Value = serde_json::from_slice(&self.payload[MANIFEST]).unwrap();
        f(&mut d);
        self.payload
            .insert(MANIFEST.into(), serde_json::to_vec(&d).unwrap());
    }
    fn record(&mut self) {
        let name = format!("{DIST}/RECORD");
        self.payload.remove(&name);
        let mut out = String::new();
        for (n, b) in &self.payload {
            out += &format!(
                "{n},sha256={},{}\n",
                URL_SAFE_NO_PAD.encode(Sha256::digest(b)),
                b.len()
            )
        }
        out += &format!("{name},,\n");
        self.payload.insert(name, out.into_bytes());
    }
    fn archive(&self) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (n, b) in &self.payload {
            z.start_file(
                n,
                SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .unix_permissions(0o644),
            )
            .unwrap();
            z.write_all(b).unwrap()
        }
        z.finish().unwrap().into_inner()
    }
    fn install_bytes(&self, bytes: &[u8]) {
        write(&self.root.path().join("runtime/vendor").join(WHEEL), bytes);
        write(&self.root.path().join("runtime/semantica-source-lock.json"),serde_json::to_vec(&json!({"$schema":oe_semantic::runtime::SOURCE_LOCK_SCHEMA,"source":{"commit":"1".repeat(40),"version":"9.9.9+fixture"},"artifact":{"filename":WHEEL,"sha256":sha256(bytes)}})).unwrap());
    }
    fn seal(&mut self) {
        self.record();
        self.install_bytes(&self.archive())
    }
    fn git_at(&self, cwd: &Path, args: &[&str]) -> String {
        let o = Command::new(&self.git)
            .current_dir(cwd)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap().trim().into()
    }
    fn git(&self, args: &[&str]) -> String {
        self.git_at(self.root.path(), args)
    }
    fn commit(&self) -> String {
        self.git(&["init", "-q"]);
        self.git(&["add", "."]);
        self.git(&["commit", "-q", "-m", "fixture snapshot"]);
        self.git(&["rev-parse", "HEAD"])
    }
    fn out(&self, n: &str) -> PathBuf {
        self.root.path().join("var").join(n)
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(self.root.path()).unwrap();
        }
    }
}
#[test]
fn actual_locked_wheel_retains_all_29_packages_and_333_source_bound_roles() {
    let root = SkillRoot::new(installed()).unwrap();
    let p = package_inventory(&root).unwrap();
    let a = p.as_array().unwrap();
    assert_eq!(a.len(), 29);
    assert_eq!(
        a.iter()
            .map(|v| v["assets"].as_array().unwrap().len())
            .sum::<usize>(),
        333
    );
    let lock = read_runtime_source_lock(root.path(), false).unwrap();
    let w = read_locked_package_wheel(&root).unwrap();
    assert_eq!(w.identity(), &lock);
    w.verify_complete_record().unwrap();
    for p in a {
        let raw = w.member(p["manifest"]["member"].as_str().unwrap()).unwrap();
        assert_eq!(sha256(raw), p["manifest"]["sha256"]);
        let d: Value = serde_json::from_slice(raw).unwrap();
        for asset in p["assets"].as_array().unwrap() {
            assert_eq!(
                sha256(w.member(asset["member"].as_str().unwrap()).unwrap()),
                asset["sha256"]
            );
            let original = d["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["asset_id"] == asset["asset_id"])
                .unwrap();
            assert_eq!(asset["role"], original["role"]);
        }
    }
}

fn install_registered_source(case: &Case) {
    for path in [
        oe_semantic::books::rust::ARCHIVE,
        "sources/continuity.json",
        oe_semantic::book_release::SOURCE_LOCK_PATH,
    ] {
        write(
            &case.root.path().join(path),
            fs::read(installed().join(path)).unwrap(),
        );
    }
}

#[test]
fn registered_source_lock_and_approved_archive_are_jointly_bound() {
    let case = Case::new();
    install_registered_source(&case);
    assert!(!case.root.path().join("runtime/vendor").exists());
    let inventory = package_inventory(&case.root).unwrap();
    assert_eq!(inventory.as_array().unwrap().len(), 29);
    assert_eq!(
        inventory
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["assets"].as_array().unwrap().len())
            .sum::<usize>(),
        333
    );
    let wheel = read_locked_package_wheel(&case.root).unwrap();
    let lock = read_runtime_source_lock(case.root.path(), false).unwrap();
    assert_eq!(wheel.identity(), &lock);
    assert_eq!(
        lock.artifact_sha256,
        oe_semantic::books::rust::APPROVED_ARCHIVE_SHA256
    );
    wheel.verify_complete_record().unwrap();

    for kind in [
        "missing_registration",
        "missing_continuity",
        "unapproved_bytes",
        "wrong_filename",
        "wrong_sha",
        "wrong_version",
        "wrong_commit",
        "synthetic_lock_with_registration",
    ] {
        let mut case = Case::new();
        case.seal();
        install_registered_source(&case);
        let lock_path = case
            .root
            .path()
            .join(oe_semantic::book_release::SOURCE_LOCK_PATH);
        let mut lock: Value = serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
        match kind {
            "missing_registration" => {
                let path = case.root.path().join("sources/continuity.json");
                let mut continuity: Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                continuity["source_files"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|item| {
                        item["path"]
                            != oe_semantic::books::rust::ARCHIVE.trim_start_matches("sources/")
                    });
                write(&path, serde_json::to_vec(&continuity).unwrap());
                fails(
                    package_inventory(&case.root),
                    "chapter_archive_identity_drift_or_not_registered",
                );
                continue;
            }
            "missing_continuity" => {
                fs::remove_file(case.root.path().join("sources/continuity.json")).unwrap();
                assert!(package_inventory(&case.root).is_err());
                continue;
            }
            "unapproved_bytes" => {
                let path = case.root.path().join(oe_semantic::books::rust::ARCHIVE);
                let mut raw = fs::read(&path).unwrap();
                raw[20] ^= 1;
                write(&path, raw);
                fails(
                    package_inventory(&case.root),
                    "book_archive_not_approved_source_release",
                );
                continue;
            }
            "wrong_filename" => {
                lock["source"]["version"] = json!("9.9.9+fixture");
                lock["artifact"]["filename"] = json!(WHEEL);
            }
            "wrong_sha" => lock["artifact"]["sha256"] = json!("0".repeat(64)),
            "wrong_version" => lock["source"]["version"] = json!("9.9.9+fixture"),
            "wrong_commit" => lock["source"]["commit"] = json!("invalid-source-revision"),
            "synthetic_lock_with_registration" => {
                case.install_bytes(&case.archive());
                fails(
                    package_inventory(&case.root),
                    "release_registered_source_lock_filename_mismatch",
                );
                continue;
            }
            _ => unreachable!(),
        }
        write(&lock_path, serde_json::to_vec(&lock).unwrap());
        if kind == "wrong_filename" {
            fails(
                package_inventory(&case.root),
                "release_registered_source_lock_filename_mismatch",
            );
        } else if kind == "wrong_sha" {
            fails(
                package_inventory(&case.root),
                "release_registered_source_lock_sha256_mismatch",
            );
        } else {
            assert!(package_inventory(&case.root).is_err(), "{kind}");
        }
    }
}

#[test]
fn registered_snapshot_binds_exact_continuity_bytes_and_current_approved_source() {
    let case = Case::new();
    install_registered_source(&case);
    let snapshot = ReleaseSourcesSnapshot::capture(&case.root).unwrap();
    assert_eq!(
        snapshot.artifact_refs()["wheel"]["path"],
        oe_semantic::books::rust::ARCHIVE
    );
    assert_eq!(
        snapshot.artifact_refs()["source_registration"]["path"],
        "sources/continuity.json"
    );
    assert_eq!(
        snapshot.artifact_refs()["source_lock"]["sha256"],
        sha256(
            fs::read(
                case.root
                    .path()
                    .join(oe_semantic::book_release::SOURCE_LOCK_PATH)
            )
            .unwrap()
        )
    );
    snapshot.ensure_current(&case.root).unwrap();
    let refs = snapshot.artifact_refs().clone();
    let path = case.root.path().join("sources/continuity.json");
    let mut continuity: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    continuity["independent_test_observation"] = json!(true);
    write(&path, serde_json::to_vec(&continuity).unwrap());
    // Approval still passes; the retained byte snapshot must reject this drift.
    oe_semantic::books::rust::ArchiveProvider::open(case.root.path()).unwrap();
    fails(
        snapshot.ensure_current(&case.root),
        "release_source_snapshot_changed:sources/continuity.json",
    );
    assert_eq!(snapshot.artifact_refs(), &refs);
}
#[test]
fn snapshot_binds_exact_used_lock_wheel_and_four_authoring_lock_bytes() {
    let mut c = Case::new();
    c.seal();
    let s = ReleaseSourcesSnapshot::capture(&c.root).unwrap();
    assert_eq!(s.packages().as_array().unwrap().len(), 29);
    assert_eq!(
        s.artifact_refs()["authoring_locks"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        s.artifact_refs()["wheel"]["size_bytes"],
        fs::metadata(c.root.path().join("runtime/vendor").join(WHEEL))
            .unwrap()
            .len()
    );
    s.ensure_current(&c.root).unwrap();
    let old = s.artifact_refs().clone();
    write(&c.root.path().join(VOL1), b"changed evaluation basis\n");
    fails(s.ensure_current(&c.root), "snapshot_changed");
    assert_eq!(s.artifact_refs(), &old);
}
#[test]
fn authoring_lock_authority_cannot_be_retired_missing_or_symlinked() {
    let c = Case::new();
    authoring_lock_paths(&c.root).unwrap();
    let retired = c
        .root
        .path()
        .join("references/ontology-engineering-book/handbook/authoring-sources.sha256");
    write(&retired, b"competing authority");
    fails(authoring_lock_paths(&c.root), "retired_vol1");
    fs::remove_file(retired).unwrap();
    fs::remove_file(c.root.path().join(VOL1)).unwrap();
    assert!(authoring_lock_paths(&c.root).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(c.root.path().join("SKILL.md"), c.root.path().join(VOL1))
            .unwrap();
        assert!(authoring_lock_paths(&c.root).is_err());
    }
}
#[test]
fn exact_coordinates_ids_and_strict_manifest_json_cannot_be_spoofed() {
    for kind in [
        "missing",
        "extra",
        "id",
        "chapter",
        "duplicate_key",
        "yaml",
        "nonfinite",
    ] {
        let mut c = Case::new();
        match kind {
            "missing" => {
                c.payload.remove(MANIFEST);
            }
            "extra" => {
                c.payload.insert(
                    "semantica/chapter_packages/vol1/ch99/manifest.yaml".into(),
                    c.payload[MANIFEST].clone(),
                );
            }
            "id" => c.mutate_manifest(|v| {
                v["package_id"] = "semantica.chapter_packages.vol9.ch99".into()
            }),
            "chapter" => c.mutate_manifest(|v| v["chapter"]["chapter"] = "ch02".into()),
            "duplicate_key" => {
                c.payload
                    .insert(MANIFEST.into(), b"{\"assets\":[],\"assets\":[]}".to_vec());
            }
            "yaml" => {
                c.payload.insert(
                    MANIFEST.into(),
                    b"package_id: example\nassets: []\n".to_vec(),
                );
            }
            "nonfinite" => {
                c.payload
                    .insert(MANIFEST.into(), b"{\"assets\":NaN}".to_vec());
            }
            _ => unreachable!(),
        }
        c.seal();
        assert!(package_inventory(&c.root).is_err(), "{kind}");
    }
}
#[test]
fn every_asset_identity_role_hash_member_and_recursive_directory_is_closed() {
    for kind in [
        "duplicate_id",
        "duplicate_member",
        "hash",
        "missing",
        "role",
        "path",
        "undeclared",
        "nested_undeclared",
    ] {
        let mut c = Case::new();
        match kind {
            "duplicate_id" => c.mutate_manifest(|v| {
                let a = v["assets"][0].clone();
                v["assets"].as_array_mut().unwrap().push(a)
            }),
            "duplicate_member" => c.mutate_manifest(|v| {
                let mut a = v["assets"][0].clone();
                a["asset_id"] = "other".into();
                v["assets"].as_array_mut().unwrap().push(a)
            }),
            "hash" => c.mutate_manifest(|v| v["assets"][0]["sha256"] = "0".repeat(64).into()),
            "missing" => {
                c.payload
                    .remove("semantica/chapter_packages/vol1/ch01/a.ttl");
            }
            "role" => c.mutate_manifest(|v| v["assets"][0]["role"] = "".into()),
            "path" => c.mutate_manifest(|v| v["assets"][0]["path"] = "../ch02/a.ttl".into()),
            "undeclared" => {
                c.payload.insert(
                    "semantica/chapter_packages/vol1/ch01/hidden.txt".into(),
                    b"hidden".to_vec(),
                );
            }
            "nested_undeclared" => {
                c.payload.insert(
                    "semantica/chapter_packages/vol1/ch01/nested/hidden.txt".into(),
                    b"hidden".to_vec(),
                );
            }
            _ => unreachable!(),
        }
        c.seal();
        assert!(package_inventory(&c.root).is_err(), "{kind}");
    }
}
#[test]
fn complete_record_closes_dist_info_and_preserves_prior_runtime_identity_contract() {
    for kind in ["unrecorded", "changed", "duplicate", "missing_self"] {
        let mut c = Case::new();
        c.record();
        let record = format!("{DIST}/RECORD");
        match kind {
            "unrecorded" => {
                c.payload
                    .insert(format!("{DIST}/secret"), b"hidden".to_vec());
            }
            "changed" => {
                c.payload
                    .insert(format!("{DIST}/LICENSE"), b"different license".to_vec());
            }
            "duplicate" => {
                let row = format!("{record},,\n");
                c.payload.get_mut(&record).unwrap().extend(row.as_bytes());
            }
            "missing_self" => {
                let raw = String::from_utf8(c.payload[&record].clone())
                    .unwrap()
                    .replace(&format!("{record},,\n"), "");
                c.payload.insert(record, raw.into_bytes());
            }
            _ => unreachable!(),
        }
        c.install_bytes(&c.archive());
        let lock = read_runtime_source_lock(c.root.path(), true).unwrap();
        let w =
            read_wheel_identity(&c.root.path().join("runtime/vendor").join(WHEEL), &lock).unwrap();
        assert!(w.verify_complete_record().is_err(), "{kind}");
        assert!(package_inventory(&c.root).is_err());
    }
}
#[test]
fn duplicate_zip_names_special_modes_and_traversal_are_rejected() {
    for kind in ["duplicate", "symlink", "fifo", "traversal"] {
        let mut c = Case::new();
        if kind == "duplicate" {
            c.payload.insert(
                "semantica/chapter_packages/vol1/ch01/b.ttl".into(),
                b"other".to_vec(),
            );
        }
        if kind == "traversal" {
            c.payload.insert("../escape".into(), b"escape".to_vec());
        }
        c.record();
        let mut raw = c.archive();
        if kind == "duplicate" {
            let from = b"semantica/chapter_packages/vol1/ch01/b.ttl";
            let to = b"semantica/chapter_packages/vol1/ch01/a.ttl";
            let positions: Vec<_> = raw
                .windows(from.len())
                .enumerate()
                .filter_map(|(i, w)| (w == from).then_some(i))
                .collect();
            assert_eq!(positions.len(), 2);
            for i in positions {
                raw[i..i + from.len()].copy_from_slice(to)
            }
        }
        if matches!(kind, "symlink" | "fifo") {
            let pos = raw.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
            let mode = if kind == "symlink" {
                0o120644u32
            } else {
                0o010644u32
            };
            raw[pos + 38..pos + 42].copy_from_slice(&(mode << 16).to_le_bytes());
        }
        c.install_bytes(&raw);
        assert!(package_inventory(&c.root).is_err(), "{kind}");
    }
}
#[test]
fn immutable_verified_members_survive_wheel_replacement_but_snapshot_recheck_denies_it() {
    let mut c = Case::new();
    c.seal();
    let s = ReleaseSourcesSnapshot::capture(&c.root).unwrap();
    let l = read_runtime_source_lock(c.root.path(), true).unwrap();
    let w = read_wheel_identity(&c.root.path().join("runtime/vendor").join(WHEEL), &l).unwrap();
    let old = w.member(MANIFEST).unwrap().to_vec();
    c.mutate_manifest(|v| v["version"] = "new-condition".into());
    c.seal();
    assert_eq!(w.member(MANIFEST).unwrap(), old);
    assert!(s.ensure_current(&c.root).is_err());
}
#[test]
fn git_source_boundary_allows_only_explicit_old_and_new_generated_artifacts() {
    let c = Case::new();
    write(&c.root.path().join("source.rs"), b"source v1\n");
    let pdf = "references/ontology-engineering-book/handbook/工程本体论-全书.pdf";
    write(&c.root.path().join(pdf), b"old pdf");
    let commit = c.commit();
    write(&c.root.path().join(pdf), b"new pdf");
    write(
        &c.root.path().join("references/book-release-artifacts.json"),
        b"{}\n",
    );
    write(
        &c.root
            .path()
            .join("references/release-evidence/v2/privacy.json"),
        b"{}\n",
    );
    let before = verify_source_boundary(&c.root, &commit, &c.git, &c.out("before")).unwrap();
    assert_eq!(before["source_identity"]["source_commit"], commit);
    assert_eq!(before["tracked_delta"], json!([pdf]));
    write(
        &c.root
            .path()
            .join("references/release-evidence/v2/semantica-regression.log"),
        b"new generated observation",
    );
    let after = ensure_source_unchanged(&c.root, &before, &c.git, &c.out("after")).unwrap();
    assert_eq!(before["source_identity"], after["source_identity"]);
    write(
        &c.root
            .path()
            .join("references/release-evidence/v2/unreviewed.json"),
        b"{}\n",
    );
    fails(
        verify_source_boundary(&c.root, &commit, &c.git, &c.out("bad")),
        "outside_generated",
    );
    let report: Value =
        serde_json::from_slice(&fs::read(c.out("bad/source-boundary.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "blocked");
    assert!(!report["steps"].as_array().unwrap().is_empty());
}
#[test]
fn source_edit_rename_and_hidden_index_flags_cannot_be_masked_by_generated_paths() {
    for kind in ["edit", "rename", "assume", "skip"] {
        let c = Case::new();
        write(&c.root.path().join("source.rs"), b"source v1\n");
        let commit = c.commit();
        match kind {
            "edit" => write(&c.root.path().join("source.rs"), b"source v2\n"),
            "rename" => {
                let p = c
                    .root
                    .path()
                    .join("references/book-release-artifacts.v2.json");
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::rename(c.root.path().join("source.rs"), p).unwrap();
                c.git(&["add", "-A"]);
            }
            "assume" => {
                c.git(&["update-index", "--assume-unchanged", "source.rs"]);
                write(&c.root.path().join("source.rs"), b"hidden drift\n");
            }
            "skip" => {
                c.git(&["update-index", "--skip-worktree", "source.rs"]);
                write(&c.root.path().join("source.rs"), b"hidden drift\n");
            }
            _ => unreachable!(),
        }
        assert!(
            verify_source_boundary(&c.root, &commit, &c.git, &c.out("proof")).is_err(),
            "{kind}"
        );
    }
}
#[test]
fn git_full_history_and_exact_stable_head_are_required() {
    for kind in ["sparse", "partial", "shallow", "wrong_commit"] {
        let c = Case::new();
        let commit = c.commit();
        match kind {
            "sparse" => {
                c.git(&["config", "core.sparseCheckout", "true"]);
            }
            "partial" => {
                c.git(&["config", "remote.origin.promisor", "true"]);
            }
            "shallow" => write(&c.root.path().join(".git/shallow"), format!("{commit}\n")),
            "wrong_commit" => {}
            _ => unreachable!(),
        }
        let selected = if kind == "wrong_commit" {
            "a".repeat(40)
        } else {
            commit
        };
        assert!(
            verify_source_boundary(&c.root, &selected, &c.git, &c.out("proof")).is_err(),
            "{kind}"
        );
    }
    let c = Case::new();
    let commit = c.commit();
    let before = verify_source_boundary(&c.root, &commit, &c.git, &c.out("before")).unwrap();
    write(
        &c.root
            .path()
            .join("references/book-release-artifacts.v2.json"),
        b"{}\n",
    );
    c.git(&["add", "."]);
    c.git(&["commit", "-q", "-m", "generated artifact"]);
    let fresh = verify_source_boundary(&c.root, &commit, &c.git, &c.out("fresh")).unwrap();
    assert_ne!(
        fresh["source_identity"]["head"],
        before["source_identity"]["head"]
    );
    fails(
        ensure_source_unchanged(&c.root, &before, &c.git, &c.out("after")),
        "identity_changed_after_replay",
    );
}
#[test]
fn semantica_checkout_must_equal_current_source_lock_and_remain_clean() {
    let mut c = Case::new();
    c.seal();
    let source = c.out("upstream checkout");
    fs::create_dir_all(&source).unwrap();
    write(&source.join("lib.py"), b"fixture upstream bytes\n");
    c.git_at(&source, &["init", "-q"]);
    c.git_at(&source, &["add", "."]);
    c.git_at(&source, &["commit", "-q", "-m", "fixture upstream"]);
    let commit = c.git_at(&source, &["rev-parse", "HEAD"]);
    let lock_path = c.root.path().join("runtime/semantica-source-lock.json");
    let mut lock: Value = serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    lock["source"]["commit"] = commit.clone().into();
    write(&lock_path, serde_json::to_vec(&lock).unwrap());
    let before =
        verify_semantica_checkout(&c.root, &source, &commit, &c.git, &c.out("clean")).unwrap();
    assert_eq!(
        before["source_identity"]["files"].as_array().unwrap().len(),
        1
    );
    assert_eq!(before["source_identity"]["head"], commit);
    write(&source.join("lib.py"), b"changed upstream bytes\n");
    assert!(verify_semantica_checkout(&c.root, &source, &commit, &c.git, &c.out("dirty")).is_err());
    fails(
        verify_semantica_checkout(&c.root, &source, &"2".repeat(40), &c.git, &c.out("wrong")),
        "differs_from_source_lock",
    );
}
#[test]
fn failed_git_execution_retains_actual_process_observation_without_claiming_source_proof() {
    let c = Case::new();
    let commit = c.commit();
    let tool = oe_core::process::resolve_executable(Path::new("false")).unwrap();
    fails(
        verify_source_boundary(&c.root, &commit, &tool, &c.out("failed-tool")),
        "git_step_failed",
    );
    let d: Value =
        serde_json::from_slice(&fs::read(c.out("failed-tool/source-boundary.json")).unwrap())
            .unwrap();
    assert_eq!(d["status"], "blocked");
    assert!(d.get("source_identity").is_none());
    assert_eq!(d["steps"][0]["observation"]["status"], "process_failed");
}
#[test]
#[ignore = "Read-only actual full locked Semantica checkout; no native regression or registry mutation"]
fn actual_locked_semantica_checkout_is_a_clean_complete_source_proof() {
    let root = SkillRoot::new(installed()).unwrap();
    let lock = read_runtime_source_lock(root.path(), true).unwrap();
    let source = root
        .path()
        .join("var/maintenance/release-0.7.0/semantica-source");
    let output = root.path().join(format!(
        "var/maintenance/rust-rewrite-20260927/phase10-sources/actual-git-proof-{}",
        std::process::id()
    ));
    let git = oe_core::process::resolve_executable(Path::new("git")).unwrap();
    let result = verify_semantica_checkout(&root, &source, &lock.commit, &git, &output).unwrap();
    assert_eq!(result["status"], "passed");
    assert!(result["source_identity"]["files"].as_array().unwrap().len() > 1000);
    println!("retained source proof: {}", output.display());
}

#[test]
fn git_clean_text_normalization_does_not_prove_actual_bytes_equal_the_commit() {
    let c = Case::new();
    write(&c.root.path().join(".gitattributes"), b"source.rs text\n");
    write(&c.root.path().join("source.rs"), b"source line\n");
    let commit = c.commit();
    write(&c.root.path().join("source.rs"), b"source line\r\n");
    assert!(
        c.git(&["diff", "--name-only", &commit, "--"]).is_empty(),
        "fixture must demonstrate Git-normalized clean bytes"
    );
    fails(
        verify_source_boundary(&c.root, &commit, &c.git, &c.out("normalized")),
        "source_bytes_differ_from_commit_blob",
    );
    let report: Value =
        serde_json::from_slice(&fs::read(c.out("normalized/source-boundary.json")).unwrap())
            .unwrap();
    assert_eq!(report["status"], "blocked");
    assert!(!report["source_snapshots"].as_array().unwrap().is_empty());
}

#[test]
fn a_git_clean_filter_cannot_hide_changed_source_or_execute_during_the_proof() {
    let c = Case::new();
    write(
        &c.root.path().join(".gitattributes"),
        b"source.rs filter=fixture\n",
    );
    write(&c.root.path().join("source.rs"), b"original\n");
    let commit = c.commit();
    c.git(&["config", "filter.fixture.clean", "sed s/altered/original/"]);
    write(&c.root.path().join("source.rs"), b"altered\n");
    assert!(
        c.git(&["diff", "--name-only", &commit, "--"]).is_empty(),
        "legacy Git boundary can report clean after a content-rewriting filter"
    );
    fails(
        verify_source_boundary(&c.root, &commit, &c.git, &c.out("filtered")),
        "clean_filter_not_permitted",
    );
    let report: Value =
        serde_json::from_slice(&fs::read(c.out("filtered/source-boundary.json")).unwrap()).unwrap();
    assert!(
        !report["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["step"] == "tracked-delta")
    );
}
