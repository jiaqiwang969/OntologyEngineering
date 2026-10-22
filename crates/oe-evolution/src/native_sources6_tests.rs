use super::*;
use std::io::Write;

struct Fixture {
    _temp: tempfile::TempDir,
    root: SkillRoot,
    build: CodeBuildRequest,
    metadata: Value,
    tree: Vec<u8>,
    packages: BTreeMap<String, PathBuf>,
    archive: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().canonicalize().unwrap();
        std::fs::write(path.join("SKILL.md"), "private synthetic fixture").unwrap();
        let local = path.join("crates/oe-code-runner");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(
            local.join("Cargo.toml"),
            "[package]\nname='oe-code-runner'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(local.join("lib.rs"), "local original\n").unwrap();
        let home = path.join("cargo");
        let mut packages = BTreeMap::new();
        let mut rows = vec![
            json!({"id":"path:runner","name":"oe-code-runner","version":"0.1.0","source":null,"manifest_path":local.join("Cargo.toml"),"targets":[]}),
        ];
        let mut tree = format!("oe-code-runner v0.1.0 ({})\t\n", local.display());
        let mut lock = "version=4\n".to_owned();
        let archive = home.join("registry/cache/test-index/bb-0.1.0.crate");
        for name in ["aa", "bb", "cc", "dd", "ee", "ff", "gg", "hh"] {
            let dir = home
                .join("registry/src/test-index")
                .join(format!("{name}-0.1.0"));
            std::fs::create_dir_all(&dir).unwrap();
            let files = BTreeMap::from([
                (
                    "Cargo.toml",
                    format!("[package]\nname='{name}'\nversion='0.1.0'\n").into_bytes(),
                ),
                ("lib.rs", b"pub fn example() {}\n".to_vec()),
            ]);
            for (file, bytes) in &files {
                std::fs::write(dir.join(file), bytes).unwrap();
            }
            let checksum = if name == "bb" {
                std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
                let out = std::fs::File::create(&archive).unwrap();
                let encoder = flate2::write::GzEncoder::new(out, flate2::Compression::default());
                let mut tar = tar::Builder::new(encoder);
                for (file, bytes) in &files {
                    let mut header = tar::Header::new_gnu();
                    header.set_size(bytes.len() as u64);
                    header.set_mode(0o644);
                    header.set_cksum();
                    tar.append_data(&mut header, format!("bb-0.1.0/{file}"), bytes.as_slice())
                        .unwrap();
                }
                tar.into_inner().unwrap().finish().unwrap();
                std::fs::write(dir.join(".cargo-ok"), b"{\"v\":1}").unwrap();
                sha256(&std::fs::read(&archive).unwrap())
            } else {
                let checksum = sha256(name.as_bytes());
                let hashes = files
                    .iter()
                    .map(|(name, bytes)| ((*name).to_owned(), sha256(bytes)))
                    .collect::<BTreeMap<_, _>>();
                std::fs::write(
                    dir.join(".cargo-checksum.json"),
                    serde_json::to_vec(&json!({"package":checksum,"files":hashes})).unwrap(),
                )
                .unwrap();
                checksum
            };
            lock.push_str(&format!("[[package]]\nname='{name}'\nversion='0.1.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\nchecksum='{checksum}'\n"));
            rows.push(json!({"id":format!("registry:{name}"),"name":name,"version":"0.1.0","source":"registry+https://github.com/rust-lang/crates.io-index","manifest_path":dir.join("Cargo.toml"),"targets":[]}));
            tree.push_str(&format!("{name} v0.1.0\t\n"));
            packages.insert(name.to_owned(), dir);
        }
        std::fs::write(path.join("Cargo.lock"), lock).unwrap();
        let nodes = rows
            .iter()
            .map(|r| json!({"id":r["id"]}))
            .collect::<Vec<_>>();
        let metadata =
            json!({"workspace_root":path,"version":1,"packages":rows,"resolve":{"nodes":nodes}});
        let build = CodeBuildRequest {
            artifact_id: "fixture".into(),
            cargo_home: home.to_str().unwrap().into(),
            cargo_path: "unused".into(),
            evidence_path: "unused".into(),
            ggen_path: "unused".into(),
            platform: "fixture".into(),
            rustc_path: "unused".into(),
            source_root: path.to_str().unwrap().into(),
            target_dir: "unused".into(),
        };
        Self {
            _temp: temp,
            root: SkillRoot::new(path).unwrap(),
            build,
            metadata,
            tree: tree.into_bytes(),
            packages,
            archive,
        }
    }
    fn run(&self) -> Result<Vec<CodeNativeCompilationSourceRoot4>> {
        derive_roots(
            &self.root,
            &self.build,
            "build-runner",
            &self.metadata,
            &self.tree,
        )
    }
}

#[test]
fn source_inventory_parallel_preserves_current_content_and_order() {
    let f = Fixture::new();
    let before = serde_json::to_value(f.run().unwrap()).unwrap();
    assert_eq!(before.as_array().unwrap().len(), 9);
    assert_eq!(before, serde_json::to_value(f.run().unwrap()).unwrap());
    std::fs::write(
        f.root.path().join("crates/oe-code-runner/lib.rs"),
        "changed local\n",
    )
    .unwrap();
    let after = serde_json::to_value(f.run().unwrap()).unwrap();
    assert_ne!(
        before, after,
        "No retained source inventory may hide a new local source"
    );
    let roots = after.as_array().unwrap();
    let ids = roots
        .iter()
        .map(|r| r["package_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(ids.windows(2).all(|p| p[0] < p[1]));
}

#[test]
fn source_inventory_parallel_preserves_first_error() {
    let f = Fixture::new();
    std::fs::write(f.packages["aa"].join("lib.rs"), "changed\n").unwrap();
    std::fs::remove_file(f.packages["hh"].join("Cargo.toml")).unwrap();
    assert_eq!(
        format!("{:#}", f.run().unwrap_err()),
        "native4_registry_original_file_checksum_changed:lib.rs"
    );
    // A later verifier can panic on malformed TOML indexing. Its joined
    // failure must not override the earlier package's ordinary error.
    let f = Fixture::new();
    std::fs::write(f.packages["aa"].join("lib.rs"), "changed\n").unwrap();
    let bytes = b"unrelated=1\n";
    std::fs::write(f.packages["gg"].join("Cargo.toml"), bytes).unwrap();
    let checks = f.packages["gg"].join(".cargo-checksum.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&checks).unwrap()).unwrap();
    value["files"]["Cargo.toml"] = json!(sha256(bytes));
    std::fs::write(checks, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.run()));
    assert!(
        result.is_ok(),
        "Later worker panic must not replace first ordered error"
    );
    assert_eq!(
        format!("{:#}", result.unwrap().unwrap_err()),
        "native4_registry_original_file_checksum_changed:lib.rs"
    );
    // The root package and aa occupy the same worker batch. A later panic
    // within that batch must preserve its earlier completed error too.
    let f = Fixture::new();
    std::fs::write(
        f.root.path().join("crates/oe-code-runner/Cargo.toml"),
        "[package]\nname='wrong'\nversion='0.1.0'\n",
    )
    .unwrap();
    let bytes = b"unrelated=1\n";
    std::fs::write(f.packages["aa"].join("Cargo.toml"), bytes).unwrap();
    let checks = f.packages["aa"].join(".cargo-checksum.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&checks).unwrap()).unwrap();
    value["files"]["Cargo.toml"] = json!(sha256(bytes));
    std::fs::write(checks, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        format!("{:#}", f.run().unwrap_err()),
        "native4_package_manifest_name_changed"
    );
    std::fs::write(
        f.root.path().join("crates/oe-code-runner/Cargo.toml"),
        "[package]\nname='oe-code-runner'\nversion='0.1.0'\n",
    )
    .unwrap();
    assert_eq!(
        format!("{:#}", f.run().unwrap_err()),
        "native4_source_package_panicked:registry:aa"
    );
}

#[test]
fn source_inventory_parallel_preserves_archive_and_cache_rejections() {
    let f = Fixture::new();
    let original = std::fs::read(f.packages["bb"].join("lib.rs")).unwrap();
    std::fs::write(f.packages["bb"].join("lib.rs"), "changed\n").unwrap();
    assert_eq!(
        format!("{:#}", f.run().unwrap_err()),
        "native4_original_crate_source_payload_changed:lib.rs"
    );
    std::fs::write(f.packages["bb"].join("lib.rs"), original).unwrap();
    let extra = f.packages["bb"].join("extra");
    std::fs::write(&extra, "extra").unwrap();
    assert_eq!(
        format!("{:#}", f.run().unwrap_err()),
        "native4_original_crate_cache_payload_or_metadata_inventory_changed"
    );
    std::fs::remove_file(extra).unwrap();
    #[cfg(unix)]
    {
        let link = f.packages["bb"].join("link");
        std::os::unix::fs::symlink(f.packages["bb"].join("lib.rs"), &link).unwrap();
        assert_eq!(
            format!("{:#}", f.run().unwrap_err()),
            "native4_source_symlink_uncovered"
        );
        std::fs::remove_file(link).unwrap();
    }
    std::fs::OpenOptions::new()
        .append(true)
        .open(&f.archive)
        .unwrap()
        .write_all(b"changed")
        .unwrap();
    assert_eq!(
        format!("{:#}", f.run().unwrap_err()),
        "native4_original_crate_lock_checksum_changed"
    );
}

#[test]
fn source_inventory_dynamic_exact_once_and_bounded_workers() {
    use std::sync::{
        Barrier, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    let ids = (0..23).map(|i| i.to_string()).collect::<Vec<_>>();
    let selected = ids
        .iter()
        .map(|id| (id.as_str(), BTreeSet::from([format!("feature-{id}")])))
        .collect::<Vec<_>>();
    let visits = (0..23).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let barrier = Barrier::new(4);
    let completed = (Mutex::new(0usize), Condvar::new());
    let output = verify_selected_packages(&selected, |id, features| {
        let i: usize = id.parse().unwrap();
        assert_eq!(features, BTreeSet::from([format!("feature-{id}")]));
        visits[i].fetch_add(1, Ordering::SeqCst);
        let count = active.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(count, Ordering::SeqCst);
        if i < 4 {
            barrier.wait();
        }
        if i == 0 {
            let (done, wait) = completed
                .1
                .wait_timeout_while(
                    completed.0.lock().unwrap(),
                    std::time::Duration::from_secs(5),
                    |n| *n < 22,
                )
                .unwrap();
            assert!(
                !wait.timed_out() && *done == 22,
                "remaining packages must progress while first package waits"
            );
        } else {
            *completed.0.lock().unwrap() += 1;
            completed.1.notify_all();
        }
        active.fetch_sub(1, Ordering::SeqCst);
        Ok(i)
    })
    .unwrap();
    assert_eq!(output, (0..23).collect::<Vec<_>>());
    assert!(visits.iter().all(|v| v.load(Ordering::SeqCst) == 1));
    assert_eq!(peak.load(Ordering::SeqCst), 4);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[test]
fn source_inventory_dynamic_errors_panics_and_join() {
    use std::sync::{
        Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    let ids = (0..17).map(|i| i.to_string()).collect::<Vec<_>>();
    let selected = ids
        .iter()
        .map(|id| (id.as_str(), BTreeSet::new()))
        .collect::<Vec<_>>();
    let visits = (0..17).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>();
    let reached_last = (Mutex::new(false), Condvar::new());
    let err = verify_selected_packages::<usize>(&selected, |id, _| {
        let i: usize = id.parse().unwrap();
        visits[i].fetch_add(1, Ordering::SeqCst);
        if i == 1 {
            let (last, wait) = reached_last
                .1
                .wait_timeout_while(
                    reached_last.0.lock().unwrap(),
                    std::time::Duration::from_secs(5),
                    |last| !*last,
                )
                .unwrap();
            assert!(!wait.timed_out() && *last);
            anyhow::bail!("earliest-content-error");
        }
        if i == 7 {
            panic!("later-package-panic");
        }
        if i == 9 {
            anyhow::bail!("later-content-error");
        }
        if i == 16 {
            *reached_last.0.lock().unwrap() = true;
            reached_last.1.notify_all();
        }
        Ok(i)
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "earliest-content-error");
    assert!(visits.iter().all(|n| n.load(Ordering::SeqCst) == 1));
    let completed = AtomicUsize::new(0);
    let err = verify_selected_packages::<usize>(&selected, |id, _| {
        completed.fetch_add(1, Ordering::SeqCst);
        if id == "0" {
            panic!("first-package-panic");
        }
        anyhow::bail!("later-error")
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "native4_source_package_panicked:0");
    assert_eq!(completed.load(Ordering::SeqCst), 17);
}

#[test]
fn source_inventory_dynamic_empty_single_and_current_calls() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    assert!(
        verify_selected_packages::<usize>(&[], |_, _| panic!("empty domain invoked verifier"))
            .unwrap()
            .is_empty()
    );
    let calls = AtomicUsize::new(0);
    let selected = [("one", BTreeSet::new())];
    let verify = |id: &str, _| {
        assert_eq!(id, "one");
        Ok(calls.fetch_add(1, Ordering::SeqCst))
    };
    assert_eq!(
        verify_selected_packages(&selected, verify).unwrap(),
        vec![0]
    );
    assert_eq!(
        verify_selected_packages(&selected, verify).unwrap(),
        vec![1]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
