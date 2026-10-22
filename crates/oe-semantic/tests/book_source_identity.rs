//! Independent source drift expectations; the original 238 book checks remain separate.
use oe_core::SkillRoot;
use oe_semantic::books;
use std::{fs, path::Path};

#[test]
fn added_owned_sources_and_templates_require_a_new_compiled_identity() {
    let directory = tempfile::tempdir().unwrap();
    let physical_directory = directory.path().canonicalize().unwrap();
    let root = physical_directory.as_path();
    fs::write(
        root.join("SKILL.md"),
        "Synthetic compiled book-source identity",
    )
    .unwrap();
    for &(relative, raw) in books::rust::source_execution::IMPLEMENTATION_SOURCES {
        let target = root.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, raw).unwrap();
    }
    let skill = SkillRoot::new(root).unwrap();
    assert_eq!(
        books::verify_book_sources(&skill).unwrap()["semantic_execution"],
        "not_run"
    );
    for relative in [
        "crates/semantica-runtime/src/additional_uncompiled.rs",
        "vendor/spareval/src/additional_uncompiled.rs",
        "templates/pack/additional_uncompiled.tmpl",
    ] {
        let path = root.join(relative);
        assert!(!path.exists());
        fs::write(&path, "// new owned source not in compiled member set\n").unwrap();
        let error = books::verify_book_sources(&skill)
            .expect_err("new source members cannot inherit an older compiled source identity");
        assert_eq!(error.to_string(), "book_execution_source_set_changed");
        fs::remove_file(path).unwrap();
        assert_eq!(
            books::verify_book_sources(&skill).unwrap()["semantic_execution"],
            "not_run"
        );
    }
}

#[test]
fn local_dependencies_and_core_drift_reject_before_book_prerequisites() {
    let directory = tempfile::tempdir().unwrap();
    let physical_directory = directory.path().canonicalize().unwrap();
    let root = physical_directory.as_path();
    fs::write(
        root.join("SKILL.md"),
        "Synthetic compiled book-source identity",
    )
    .unwrap();
    for &(relative, raw) in books::rust::source_execution::IMPLEMENTATION_SOURCES {
        let target = root.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, raw).unwrap();
    }
    let skill = SkillRoot::new(root).unwrap();
    let control = books::verify_book_sources(&skill).unwrap();
    assert_eq!(control["engine"], "semantica.rust-runtime");
    assert_eq!(control["semantic_execution"], "not_run");
    assert_eq!(control["status"], "blocked");
    for relative in [
        "vendor/spargebra/src/parser.rs",
        "vendor/spareval/src/eval.rs",
        "crates/oe-core/src/lib.rs",
        "generated/semantic-contracts.rs",
    ] {
        let path = root.join(Path::new(relative));
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n// controlled compiled dependency drift\n");
        fs::write(&path, changed).unwrap();
        let error = books::verify_book_sources(&skill).expect_err(
            "changed dependency bytes must reject before issuing current Rust identity",
        );
        assert!(
            error.to_string().contains(&format!(
                "compiled_book_source_execution_mismatch:{relative}"
            )),
            "{error}"
        );
        fs::write(&path, original).unwrap();
        let restored = books::verify_book_sources(&skill).unwrap();
        assert_eq!(restored["semantic_execution"], "not_run");
        assert_eq!(restored["status"], "blocked");
    }
}
