use super::*;

struct Fixture {
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        let skill = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let name = format!(
            "book-release-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            WRITE_SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let path = skill.join("var/maintenance/rust-tests").join(name);
        oe_core::create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "fixture only\n").unwrap();
        let result = Self {
            root: SkillRoot::new(path).unwrap(),
        };
        for p in RIGHTS_SOURCES {
            result.put(p, b"source-authority-unconfirmed\n");
        }
        result
    }
    fn put(&self, relative: &str, bytes: &[u8]) {
        let path = self.root.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(self.root.path()).unwrap();
        }
    }
}

#[test]
fn strict_canonical_bytes_reject_duplicate_nonfinite_noncanonical_inputs() {
    let f = Fixture::new();
    let canonical = canonical_bytes(&json!({"z":[],"a":"原页😀"})).unwrap();
    assert_eq!(canonical, "{\"a\":\"原页😀\",\"z\":[]}\n".as_bytes());
    for bad in [
        b"{\"a\":1,\"a\":2}\n".as_slice(),
        b"{\"a\":NaN}\n",
        b"{ \"a\":1}\n",
        b"{\"a\":1}",
        b"[]\n",
        b"{\"a\":Infinity}\n",
    ] {
        f.put("input.json", bad);
        assert!(read_document(&f.root, "input.json", true).is_err());
    }
    f.put("input.json", &canonical);
    assert_eq!(
        read_document(&f.root, "input.json", true).unwrap(),
        json!({"a":"原页😀","z":[]})
    );
}

#[test]
fn artifact_reference_requires_exact_source_bytes_path_and_integer_size() {
    let f = Fixture::new();
    f.put("data/原文.txt", "条件、反例与原图引用😀\n".as_bytes());
    let original = artifact_ref(&f.root, "data/原文.txt").unwrap();
    verify_artifact_ref(&f.root, &original, "test").unwrap();
    for bad in [
        "../outside",
        "./data/原文.txt",
        "data//原文.txt",
        "data\\原文.txt",
        "/etc/hosts",
    ] {
        assert!(artifact_ref(&f.root, bad).is_err());
    }
    for size in [json!(true), json!(-1), json!(1.25), Value::Null] {
        let mut forged = original.clone();
        forged["size_bytes"] = size;
        assert!(verify_artifact_ref(&f.root, &forged, "test").is_err());
    }
    let mut extra = original.clone();
    extra["trusted"] = json!(true);
    assert!(verify_artifact_ref(&f.root, &extra, "test").is_err());
    f.put("data/原文.txt", b"different original source\n");
    assert!(verify_artifact_ref(&f.root, &original, "test").is_err());
}

#[test]
fn parsed_document_and_sidecar_references_bind_the_captured_bytes() {
    let f = Fixture::new();
    let first = canonical_bytes(&json!({"revision":1,"content":"原始证据"})).unwrap();
    f.put("bound.json", &first);
    let (reference, parsed) = read_bound_document(&f.root, "bound.json", true).unwrap();
    f.put(
        "bound.json",
        &canonical_bytes(&json!({"revision":2})).unwrap(),
    );
    assert_eq!(reference["sha256"], sha256(&first));
    assert_eq!(reference["size_bytes"], first.len());
    assert_eq!(parsed["revision"], 1);
    assert!(verify_artifact_ref(&f.root, &reference, "captured").is_err());
    let sidecar = format!("{}  bound.json\n", sha256(&first));
    f.put("bound.json.sha256", sidecar.as_bytes());
    let (reference, bytes) = read_bound_bytes(&f.root, "bound.json.sha256", 4096).unwrap();
    f.put("bound.json.sha256", b"replacement\n");
    assert_eq!(bytes, sidecar.as_bytes());
    assert_eq!(reference["sha256"], sha256(&bytes));
    assert!(verify_artifact_ref(&f.root, &reference, "sidecar").is_err());
}

#[test]
fn governance_initializes_only_pending_and_preserves_existing_and_legacy_bytes() {
    let f = Fixture::new();
    f.put(
        "references/release-evidence/rights.json",
        b"legacy rights frozen\n",
    );
    let report = initialize_governance(&f.root, false).unwrap();
    assert_eq!(report["rights"], "pending");
    assert_eq!(report["publication_approval"], "pending");
    let rights = read_document(&f.root, governance_path("rights").unwrap(), true).unwrap();
    assert_eq!(rights["sources"].as_array().unwrap().len(), 3);
    let before = artifact_ref(&f.root, governance_path("rights").unwrap()).unwrap();
    assert_eq!(
        initialize_governance(&f.root, false).unwrap()["changed"],
        false
    );
    assert_eq!(
        artifact_ref(&f.root, governance_path("rights").unwrap()).unwrap(),
        before
    );
    assert_eq!(
        fs::read(
            f.root
                .path()
                .join("references/release-evidence/rights.json")
        )
        .unwrap(),
        b"legacy rights frozen\n"
    );
    f.put(RIGHTS_SOURCES[0], b"changed rights statement\n");
    assert!(initialize_governance(&f.root, false).is_err());
    assert_eq!(
        initialize_governance(&f.root, true).unwrap()["rights"],
        "pending"
    );
    assert_ne!(
        artifact_ref(&f.root, governance_path("rights").unwrap()).unwrap(),
        before
    );
}

#[test]
fn partial_governance_or_self_approval_is_never_accepted() {
    let f = Fixture::new();
    initialize_governance(&f.root, false).unwrap();
    let rights_path = governance_path("rights").unwrap();
    let mut rights = read_document(&f.root, rights_path, true).unwrap();
    rights["status"] = json!("approved");
    f.put(rights_path, &canonical_bytes(&rights).unwrap());
    assert!(initialize_governance(&f.root, false).is_err());
    fs::remove_file(
        f.root
            .path()
            .join(governance_path("publication_approval").unwrap()),
    )
    .unwrap();
    for reset in [false, true] {
        assert!(initialize_governance(&f.root, reset).is_err());
    }
}

#[test]
fn publication_is_restricted_and_retains_exact_before_after_recovery() {
    let f = Fixture::new();
    for path in [
        "runtime/semantica-source-lock.json",
        "references/book-release-artifacts.json",
        "references/release-evidence/rights.json",
        "SKILL.md",
        "var/unlisted.json",
    ] {
        assert!(write_document(&f.root, path, &json!({"passed":true})).is_err());
    }
    let path = evidence_path("privacy").unwrap();
    write_document(&f.root, path, &json!({"iteration":1})).unwrap();
    write_document(&f.root, path, &json!({"iteration":2})).unwrap();
    assert_eq!(
        read_document(&f.root, path, true).unwrap(),
        json!({"iteration":2})
    );
    let mut journal_before = Vec::new();
    for entry in fs::read_dir(f.root.path().join("var/maintenance")).unwrap() {
        let p = entry.unwrap().path();
        assert!(p.join("intent.json").is_file() && p.join("published.json").is_file());
        if p.join("before").exists() {
            journal_before.push(fs::read(p.join("before")).unwrap());
        }
    }
    assert_eq!(
        journal_before,
        [canonical_bytes(&json!({"iteration":1})).unwrap()]
    );
}

#[cfg(unix)]
#[test]
fn symlink_source_and_output_parent_are_rejected_without_following() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.put("actual.txt", b"external identity\n");
    symlink(
        f.root.path().join("actual.txt"),
        f.root.path().join("alias.txt"),
    )
    .unwrap();
    assert!(artifact_ref(&f.root, "alias.txt").is_err());
    fs::create_dir_all(f.root.path().join("outside")).unwrap();
    fs::create_dir_all(f.root.path().join("references/release-evidence")).unwrap();
    symlink(
        f.root.path().join("outside"),
        f.root.path().join("references/release-evidence/v2"),
    )
    .unwrap();
    assert!(
        write_document(
            &f.root,
            evidence_path("privacy").unwrap(),
            &json!({"passed":true})
        )
        .is_err()
    );
    assert!(
        fs::read_dir(f.root.path().join("outside"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn all_technical_greens_still_leave_governance_and_local_distribution_blockers() {
    let verification: serde_json::Map<String, Value> = EVIDENCE_PATHS
        .iter()
        .map(|(k, _)| ((*k).into(), json!({"status":"passed"})))
        .collect();
    let manifest = json!({"verification":verification,"governance":{"rights":{"status":"approved"},"publication_approval":{"status":"approved"}},"packages":[{"package_id":"example","status":"complete","release_status":"complete"}]});
    assert_eq!(
        compute_release_blockers(&manifest),
        [
            "distribution.local_candidate_evidence",
            "governance.publication_approval",
            "governance.rights"
        ]
    );
}
