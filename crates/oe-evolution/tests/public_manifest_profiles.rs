//! Independently constructed historical domains and the public-source exception.
use oe_core::sha256;
use oe_evolution::{
    CodeBinaryRef, binary_reference, digest, public_manifest, public_manifest_matching,
    verify_binary,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

fn root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    (temp, root)
}
fn file(root: &Path, name: &str, bytes: &[u8]) -> Value {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mode = if cfg!(unix) { 0o600 } else { 0o644 };
    json!({"sha256":sha256(bytes),"bytes":bytes.len(),"mode":mode})
}
// Independent literal original constructor: no stripping fields from runtime output.
fn legacy(files: BTreeMap<String, Value>) -> Value {
    let total: u64 = files.values().map(|v| v["bytes"].as_u64().unwrap()).sum();
    json!({"schema":"oev2.code-public-manifest/1","files":files,"file_count":files.len(),
        "total_bytes":total,"excluded_private_roots":["var","sources",".git"],
        "uncovered":["external Cargo source bytes and resolved transitive feature graph","native SDK/runtime/provider closure","unlisted compile environment"],
        "status":"bounded_public_identity"})
}
fn derived(mut original: Value) -> Value {
    original["resource_profile"] = json!("oev2.derived-design-model-resource/2");
    original["file_byte_limits"] = json!({"default":16777216,"generated/model.ttl":33554432});
    original
}
fn sample(root: &Path) -> Value {
    legacy(BTreeMap::from([(
        "value.rs".into(),
        file(root, "value.rs", b"pub fn value() -> u32 { 7 }\n"),
    )]))
}
fn reference(root: &Path, expected: &Value) -> CodeBinaryRef {
    let executable = root.join("var/program");
    file(
        root,
        "var/program",
        b"identity-only fixture bytes; never executed",
    );
    CodeBinaryRef {
        path: executable.to_str().unwrap().into(),
        bytes: fs::metadata(&executable).unwrap().len(),
        sha256: sha256(fs::read(&executable).unwrap()),
        source_root: root.to_str().unwrap().into(),
        source_manifest_sha256: digest(expected).unwrap(),
    }
}

#[test]
fn legacy_manifest_exact_identity_and_current_capture_remain_distinct() {
    let (_temp, root) = root();
    let original = sample(&root);
    assert_eq!(original.as_object().unwrap().len(), 7);
    assert_eq!(
        public_manifest_matching(&root, &original).unwrap(),
        original
    );
    let frozen = serde_json::to_vec(&original).unwrap();
    let old = reference(&root, &original);
    verify_binary(&old).unwrap();
    let current = public_manifest(&root).unwrap();
    let previous_derived = derived(original.clone());
    assert_eq!(previous_derived.as_object().unwrap().len(), 9);
    assert_eq!(
        public_manifest_matching(&root, &previous_derived).unwrap(),
        previous_derived
    );
    verify_binary(&reference(&root, &previous_derived)).unwrap();
    let mut expected_current = previous_derived.clone();
    expected_current["public_sources"] = json!(["sources/README.md"]);
    assert_eq!(current, expected_current);
    assert_eq!(current.as_object().unwrap().len(), 10);
    assert_ne!(
        digest(&current).unwrap(),
        digest(&previous_derived).unwrap()
    );
    assert_ne!(digest(&current).unwrap(), old.source_manifest_sha256);
    assert_eq!(public_manifest_matching(&root, &current).unwrap(), current);
    let new = binary_reference(Path::new(&old.path), &root).unwrap();
    assert_eq!(new.source_manifest_sha256, digest(&current).unwrap());
    verify_binary(&new).unwrap();
    assert_eq!(serde_json::to_vec(&original).unwrap(), frozen);
}

#[test]
fn legacy_rechecks_bytes_modes_membership_and_executable_without_rebinding() {
    let (_temp, root) = root();
    let original = sample(&root);
    let old = reference(&root, &original);
    let source = root.join("value.rs");
    let bytes = fs::read(&source).unwrap();
    fs::write(&source, b"pub fn value() -> u32 { 8 }\n").unwrap();
    assert!(verify_binary(&old).is_err());
    assert!(public_manifest_matching(&root, &original).is_err());
    fs::write(&source, &bytes).unwrap();
    verify_binary(&old).unwrap();
    file(&root, "extra.txt", b"new public file");
    assert!(verify_binary(&old).is_err());
    fs::remove_file(root.join("extra.txt")).unwrap();
    fs::remove_file(&source).unwrap();
    assert!(verify_binary(&old).is_err());
    file(&root, "value.rs", &bytes);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(verify_binary(&old).is_err());
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        fs::rename(&source, root.join("var/original")).unwrap();
        std::os::unix::fs::symlink(root.join("var/original"), &source).unwrap();
        assert!(verify_binary(&old).is_err());
        fs::remove_file(&source).unwrap();
        fs::rename(root.join("var/original"), &source).unwrap();
    }
    verify_binary(&old).unwrap();
    fs::write(&old.path, b"changed executable").unwrap();
    assert!(
        verify_binary(&old)
            .unwrap_err()
            .to_string()
            .contains("code_executable_changed")
    );
}

#[test]
fn unknown_manifest_shapes_and_claimed_resource_limits_are_not_profiles() {
    let (_temp, root) = root();
    let original = sample(&root);
    let current = public_manifest(&root).unwrap();
    let mut malformed = Vec::new();
    for key in ["resource_profile", "file_byte_limits"] {
        let mut v = current.clone();
        v.as_object_mut().unwrap().remove(key);
        malformed.push(v);
    }
    for (key, value) in [
        ("resource_profile", json!("unknown/9")),
        ("public_sources", json!([])),
        ("public_sources", json!(["sources/readme.md"])),
        (
            "public_sources",
            json!(["sources/README.md", "sources/books/original.pdf"]),
        ),
        (
            "file_byte_limits",
            json!({"default":999999999,"generated/model.ttl":999999999}),
        ),
    ] {
        let mut v = current.clone();
        v[key] = value;
        malformed.push(v);
    }
    for (key, value) in [
        ("schema", json!("other/1")),
        ("status", json!("passed")),
        ("file_count", json!(0)),
        ("total_bytes", json!(0)),
        (
            "excluded_private_roots",
            json!(["var", "sources", ".git", "extra"]),
        ),
        ("uncovered", json!([])),
        ("unknown", json!(true)),
    ] {
        let mut v = original.clone();
        v[key] = value;
        malformed.push(v);
    }
    for (key, value) in [
        ("bytes", json!(16777217)),
        ("mode", json!(8192)),
        ("sha256", json!("passed")),
        ("unknown", json!(true)),
    ] {
        let mut v = original.clone();
        v["files"]["value.rs"][key] = value;
        malformed.push(v);
    }
    for value in malformed {
        assert!(public_manifest_matching(&root, &value).is_err(), "{value}");
    }
    let mut wrong_digest = reference(&root, &original);
    wrong_digest.source_manifest_sha256 = "0".repeat(64);
    assert!(verify_binary(&wrong_digest).is_err());
    assert_eq!(
        public_manifest_matching(&root, &original).unwrap(),
        original
    );
}

#[test]
fn sources_readme_is_public_only_in_new_profile_and_private_bytes_stay_excluded() {
    let (_temp, root) = root();
    let original = sample(&root);
    let previous = derived(original.clone());
    let old_refs = [reference(&root, &original), reference(&root, &previous)];
    let readme = file(
        &root,
        "sources/README.md",
        b"Public source library instructions\n",
    );
    for name in [
        "sources/books/original.pdf",
        "sources/misumi/original.pdf",
        "sources/.indexes/local-pdf/catalog.sqlite3",
        "sources/books/README.md",
    ] {
        file(&root, name, b"private original");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        root.join("absent-private-original"),
        root.join("sources/unreadable-original"),
    )
    .unwrap();
    let mut files = original["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    files.insert("sources/README.md".into(), readme);
    let mut expected = derived(legacy(files));
    expected["public_sources"] = json!(["sources/README.md"]);
    assert_eq!(public_manifest(&root).unwrap(), expected);
    let current = reference(&root, &expected);
    verify_binary(&current).unwrap();
    for historic in [&original, &previous] {
        assert_eq!(
            public_manifest_matching(&root, historic).unwrap(),
            *historic
        );
    }
    for old in &old_refs {
        verify_binary(old).unwrap();
    }
    file(
        &root,
        "sources/books/original.pdf",
        b"changed private bytes",
    );
    file(&root, "sources/new-private-file", b"new private bytes");
    assert_eq!(public_manifest(&root).unwrap(), expected);
    verify_binary(&current).unwrap();
    file(&root, "sources/README.md", b"changed public instructions");
    assert!(public_manifest_matching(&root, &expected).is_err());
    assert!(verify_binary(&current).is_err());
    for old in &old_refs {
        verify_binary(old).unwrap();
    }
    fs::remove_file(root.join("sources/README.md")).unwrap();
    assert!(public_manifest_matching(&root, &expected).is_err());
    // Historical matching still does not acquire the new file's authority.
    assert_eq!(
        public_manifest_matching(&root, &previous).unwrap(),
        previous
    );
}

#[test]
fn source_readme_exception_requires_exact_name_and_regular_file() {
    let (_temp, root) = root();
    let original = sample(&root);
    file(&root, "sources/readme.md", b"not the public entry name");
    let value = public_manifest(&root).unwrap();
    assert_eq!(value["files"], original["files"]);
    fs::remove_file(root.join("sources/readme.md")).unwrap();
    fs::create_dir(root.join("sources/README.md")).unwrap();
    file(&root, "sources/README.md/child", b"must not be traversed");
    assert!(public_manifest(&root).is_err());
    fs::remove_dir_all(root.join("sources/README.md")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("value.rs"), root.join("sources/README.md")).unwrap();
        assert!(public_manifest(&root).is_err());
        // New-domain rejection cannot rewrite a digest-only historical domain.
        verify_binary(&reference(&root, &original)).unwrap();
        verify_binary(&reference(&root, &derived(original.clone()))).unwrap();
        fs::remove_file(root.join("sources/README.md")).unwrap();
        fs::remove_dir(root.join("sources")).unwrap();
        std::os::unix::fs::symlink(root.join("var"), root.join("sources")).unwrap();
        assert!(public_manifest(&root).is_err());
        verify_binary(&reference(&root, &original)).unwrap();
    }
}

#[test]
fn actual_model_boundary_keeps_legacy_sixteen_and_current_thirty_two_mebibytes() {
    let (_temp, root) = root();
    let sixteen = vec![b'x'; 16 * 1024 * 1024];
    let original = legacy(BTreeMap::from([(
        "generated/model.ttl".into(),
        file(&root, "generated/model.ttl", &sixteen),
    )]));
    verify_binary(&reference(&root, &original)).unwrap();
    assert_eq!(
        public_manifest_matching(&root, &original).unwrap(),
        original
    );
    let mut larger = sixteen;
    larger.push(b'y');
    let claimed_old = legacy(BTreeMap::from([(
        "generated/model.ttl".into(),
        file(&root, "generated/model.ttl", &larger),
    )]));
    // Even a freshly computed digest of this seven-key claim cannot inherit /2 limits.
    let invalid_legacy = reference(&root, &claimed_old);
    assert!(verify_binary(&invalid_legacy).is_err());
    assert!(public_manifest_matching(&root, &claimed_old).is_err());
    let current = public_manifest(&root).unwrap();
    verify_binary(&reference(&root, &current)).unwrap();
    larger.resize(32 * 1024 * 1024, b'z');
    file(&root, "generated/model.ttl", &larger);
    let current = public_manifest(&root).unwrap();
    assert_eq!(current["files"]["generated/model.ttl"]["bytes"], 33554432);
    verify_binary(&reference(&root, &current)).unwrap();
    larger.push(b'!');
    file(&root, "generated/model.ttl", &larger);
    assert!(public_manifest(&root).is_err());
    fs::remove_file(root.join("generated/model.ttl")).unwrap();
    larger.truncate(16 * 1024 * 1024 + 1);
    file(&root, "model.ttl", &larger);
    assert!(public_manifest(&root).is_err());
}

#[test]
fn current_and_legacy_cannot_bypass_original_file_count_or_total_budget() {
    let (_temp, root) = root();
    let mut files = BTreeMap::new();
    for index in 0..10000 {
        let name = format!("files/{index:05}");
        files.insert(name.clone(), file(&root, &name, b""));
    }
    let at_count = legacy(files);
    assert_eq!(
        public_manifest_matching(&root, &at_count).unwrap(),
        at_count
    );
    let old = reference(&root, &at_count);
    file(&root, "files/overflow", b"");
    assert!(public_manifest(&root).is_err());
    assert!(verify_binary(&old).is_err());
    fs::remove_dir_all(root.join("files")).unwrap();
    // Sparse files exercise the actual traversal's unchanged 256MiB aggregate cap.
    for index in 0..17 {
        let path = root.join(format!("block-{index}"));
        fs::File::create(path)
            .unwrap()
            .set_len(16 * 1024 * 1024)
            .unwrap();
    }
    assert!(public_manifest(&root).is_err());
    assert!(verify_binary(&old).is_err());
}
