#![cfg(windows)]
use std::path::Path;

fn ordinary_local_path(path: &Path) -> std::path::PathBuf {
    use std::path::{Component, Prefix};
    let mut parts = path.components();
    let drive = match parts.next().unwrap() {
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(d) | Prefix::VerbatimDisk(d) => d,
            _ => panic!("test fixture must use the supported local disk owner"),
        },
        _ => panic!("absolute fixture required"),
    };
    let mut result = std::path::PathBuf::from(format!("{}:\\", drive as char));
    for part in parts {
        match part {
            Component::RootDir => {}
            Component::Normal(v) => result.push(v),
            _ => panic!("non-normal actual fixture"),
        }
    }
    result
}

#[test]
fn windows_current_composition_spellings_keep_complete_capture_and_metadata() {
    let root = oe_core::SkillRoot::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap(),
    )
    .unwrap();
    let canonical = root.path().join("specs/engineering.toml");
    let ordinary = ordinary_local_path(&canonical);
    assert_ne!(canonical, ordinary);
    let canonical_loaded = oe_design::composition::load(root.path(), &canonical).unwrap();
    let ordinary_loaded = oe_design::composition::load(root.path(), &ordinary).unwrap();
    assert_eq!(
        canonical_loaded.snapshot.root,
        ordinary_loaded.snapshot.root
    );
    assert_eq!(
        canonical_loaded.snapshot.files,
        ordinary_loaded.snapshot.files
    );
    assert_eq!(
        canonical_loaded.snapshot.hashes,
        ordinary_loaded.snapshot.hashes
    );
    assert_eq!(
        canonical_loaded.source_bytes().unwrap(),
        ordinary_loaded.source_bytes().unwrap()
    );
    assert_eq!(ordinary_loaded.snapshot.files.len(), 4);
    for loaded in [&canonical_loaded, &ordinary_loaded] {
        assert_eq!(
            oe_design::composition::require_current(&root, loaded).unwrap()["status"],
            "matched"
        );
    }
    let anchored = oe_design::composition::transport::anchored(root.path(), &ordinary).unwrap();
    assert_eq!(anchored, canonical);
    assert_eq!(
        oe_core::portable_source_path(root.path(), &anchored).unwrap(),
        "specs/engineering.toml"
    );
    // Both actual public /2 entrances retain the same complete semantic AST.
    let direct = oe_design::load(&ordinary).unwrap();
    let current = oe_design::load_current(&root, &ordinary).unwrap();
    assert_eq!(
        direct.canonical().unwrap(),
        canonical_loaded.design.canonical().unwrap()
    );
    assert_eq!(
        current.canonical().unwrap(),
        canonical_loaded.design.canonical().unwrap()
    );
    let legacy = root.path().join("specs/foundation/engineering-v1.toml");
    let legacy_ordinary = ordinary_local_path(&legacy);
    assert_eq!(
        oe_design::load(&legacy).unwrap().canonical().unwrap(),
        oe_design::load(&legacy_ordinary)
            .unwrap()
            .canonical()
            .unwrap()
    );

    // Exercise the real current generation entrance, including its editable
    // metadata path. A missing tool is the precise gate after matched capture
    // and before stage allocation or dispatch; this is not a ggen success.
    let private = tempfile::tempdir_in(root.path().join("var")).unwrap();
    let missing_tool = private.path().join("absent-ggen.exe");
    assert!(!missing_tool.exists());
    let before = oe_design::generation::snapshot(&root.path().join("generated")).unwrap();
    for spec in [&ordinary, &canonical] {
        assert_eq!(
            oe_design::generation::run_spec(&root, spec, &missing_tool)
                .unwrap_err()
                .to_string(),
            "ggen_binary_missing",
            "current ordinary and verbatim spellings must reach the same tool gate"
        );
    }
    assert_eq!(
        before,
        oe_design::generation::snapshot(&root.path().join("generated")).unwrap()
    );
}

#[test]
fn windows_design_entrance_rejects_a_real_owned_directory_junction() {
    use std::os::windows::fs::MetadataExt;
    use std::path::PathBuf;

    struct JunctionCleanup(PathBuf);
    impl Drop for JunctionCleanup {
        fn drop(&mut self) {
            // Remove only the junction itself, before TempDir removes its
            // owned tree. Never recursively follow a reparse target.
            if std::fs::symlink_metadata(&self.0).is_ok() {
                let _ = std::fs::remove_dir(&self.0);
            }
        }
    }

    let root = oe_core::SkillRoot::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap(),
    )
    .unwrap();
    let private = tempfile::tempdir_in(root.path().join("var")).unwrap();
    let base = private.path().canonicalize().unwrap();
    let target = base.join("owned-target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("engineering.toml"), b"owned target marker").unwrap();
    let junction = base.join("owned-junction");
    let cleanup = JunctionCleanup(junction.clone());
    let created = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(ordinary_local_path(&junction))
        .arg(ordinary_local_path(&target))
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "task-owned junction must actually exist: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let metadata = std::fs::symlink_metadata(&junction).unwrap();
    assert_ne!(metadata.file_attributes() & 0x400, 0);
    let expected = if metadata.file_type().is_symlink() {
        "design_composition_link_unsupported"
    } else {
        "design_composition_reparse_unsupported"
    };
    for incoming in [
        junction.join("engineering.toml"),
        ordinary_local_path(&junction).join("engineering.toml"),
    ] {
        assert_eq!(
            oe_design::composition::transport::anchored(&base, &incoming).unwrap_err(),
            expected
        );
        assert_eq!(
            oe_design::composition::load(&base, &incoming)
                .err()
                .unwrap()
                .to_string(),
            expected
        );
    }
    assert_eq!(
        std::fs::read(target.join("engineering.toml")).unwrap(),
        b"owned target marker"
    );
    std::fs::remove_dir(&junction).unwrap();
    drop(cleanup);
}

#[test]
fn windows_equivalent_prefix_never_authorizes_outside_parent_or_ambiguous_names() {
    use oe_design::composition::transport::{absolute, relative_under};
    let base = Path::new(r"\\?\C:\owned\source");
    assert_eq!(
        relative_under(base, Path::new(r"C:\owned\source\specs\engineering.toml")).unwrap(),
        Path::new(r"specs\engineering.toml")
    );
    assert!(relative_under(base, Path::new(r"D:\owned\source\specs\engineering.toml")).is_err());
    assert!(relative_under(base, Path::new(r"C:\owned\other\engineering.toml")).is_err());
    assert!(relative_under(base, Path::new(r"C:\owned\source-sibling\engineering.toml")).is_err());
    for path in [
        r"C:\owned\source\..\other\engineering.toml",
        r"C:relative.toml",
        r"C:\owned\source.\engineering.toml",
        r"\\?\C:\owned\source.\engineering.toml",
        r"C:\owned\source \engineering.toml",
        r"\\?\C:\owned\source \engineering.toml",
        r"C:\owned\source\NUL\engineering.toml",
        r"C:\owned\source\bad:name.toml",
        r"\\server\share\source\engineering.toml",
        r"\\?\UNC\server\share\source\engineering.toml",
        r"\\.\C:\owned\source\engineering.toml",
        r"\\?\Volume{fixture}\source\engineering.toml",
    ] {
        assert!(absolute(Path::new(path)).is_err(), "{path}");
        assert!(
            relative_under(Path::new(path), Path::new(path)).is_err(),
            "same forbidden prefix/name: {path}"
        );
    }
    assert!(absolute(Path::new("C:\\owned\\source\\bad\nname.toml")).is_err());
    // Lexical equivalence grants no link/identity permission; actual source
    // reads still go through no_links and StableRoot/PinnedFile.
}

#[test]
fn windows_source_paths_match_declared_portable_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let design = oe_design::load(&root.join("specs/engineering.toml")).unwrap();
    let files = oe_design::inspection::inventory(&root).unwrap();
    assert!(files.iter().all(|f| !f.path.contains('\\')));
    let production = files
        .iter()
        .find(|f| f.path == "crates/oe-core/src/lib.rs")
        .expect("portable source identity must match the declared production file");
    assert_eq!(
        production.sha256,
        oe_core::sha256(std::fs::read(root.join(&production.path)).unwrap())
    );
    let inspection = oe_design::inspection::inspect(&root, &design).unwrap();
    let findings = inspection["findings"].as_array().unwrap();
    assert!(
        !findings.iter().any(|f| {
            ["constraint_execution_missing", "validation_case_missing"]
                .contains(&f["code"].as_str().unwrap())
        }),
        "unchanged production checks and cases cannot disappear due to native path separators"
    );
}
