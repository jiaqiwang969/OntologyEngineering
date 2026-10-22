use oe_core::SkillRoot;
use std::path::Path;
#[test]
fn changed_normalizer_and_build_inputs_refuse_formal_execution_but_keep_diagnostics() {
    let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let temp = tempfile::tempdir_in(actual.join("var/builds")).unwrap();
    let root = temp.path();
    std::fs::write(root.join("SKILL.md"), "private compiler binding fixture").unwrap();
    for p in oe_design::provenance::compiled().unwrap()["files"]
        .as_object()
        .unwrap()
        .keys()
    {
        std::fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
        std::fs::copy(actual.join(p), root.join(p)).unwrap();
    }
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::copy(
        actual.join("specs/engineering.toml"),
        root.join("specs/engineering.toml"),
    )
    .unwrap();
    // Diagnostic loading retains the complete selected /2 input set even when
    // executing code is changed; no source check is replaced by missing data.
    for relative in oe_design::provenance::compiled().unwrap()["composition_files"]
        .as_object()
        .into_iter()
        .flat_map(|v| v.keys())
    {
        let output = root.join(relative);
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::copy(actual.join(relative), output).unwrap();
    }
    let r = SkillRoot::new(root).unwrap();
    assert_eq!(
        oe_design::provenance::require(&r).unwrap()["status"],
        "matched"
    );
    let p = root.join("crates/oe-design/src/lib.rs");
    let bytes = std::fs::read_to_string(&p).unwrap();
    std::fs::write(
        &p,
        bytes.replace(
            "self.schema == \"oev2.design/1\"",
            "self.schema == \"oev2.design/2\"",
        ),
    )
    .unwrap();
    assert!(
        oe_design::load_current(&r, &root.join("specs/engineering.toml"))
            .unwrap_err()
            .to_string()
            .contains("compiled_source_binding_mismatch")
    );
    assert_eq!(
        oe_design::provenance::binding(&r).unwrap()["status"],
        "mismatch"
    );
    assert!(
        oe_design::load(&root.join("specs/engineering.toml")).is_ok(),
        "diagnostic loading has no current-source authority"
    );
    std::fs::write(&p, &bytes).unwrap();
    std::fs::write(root.join("Cargo.lock"), "changed lock").unwrap();
    assert!(oe_design::provenance::require(&r).is_err());
}

#[test]
fn changed_embedded_vocabulary_refuses_formal_execution() {
    let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let temp = tempfile::tempdir_in(actual.join("var/builds")).unwrap();
    let root = temp.path();
    std::fs::write(root.join("SKILL.md"), "private embedded-input fixture").unwrap();
    for path in oe_design::provenance::compiled().unwrap()["files"]
        .as_object()
        .unwrap()
        .keys()
    {
        std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        std::fs::copy(actual.join(path), root.join(path)).unwrap();
    }
    let r = SkillRoot::new(root).unwrap();
    assert_eq!(
        oe_design::provenance::require(&r).unwrap()["status"],
        "matched"
    );
    for relative in [
        "modules/engineering-memory/runtime/src/main.rs",
        "crates/semantica-json/src/data/python_xml_name_unicode151.json",
    ] {
        let path = root.join(relative);
        let original = std::fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n");
        std::fs::write(&path, changed).unwrap();
        let binding = oe_design::provenance::binding(&r).unwrap();
        assert_eq!(binding["status"], "mismatch");
        assert!(
            binding["changed"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row.as_str() == Some(relative) || row["path"] == relative)
        );
        assert!(oe_design::provenance::require(&r).is_err());
        std::fs::write(&path, original).unwrap();
        assert_eq!(
            oe_design::provenance::require(&r).unwrap()["status"],
            "matched"
        );
    }
}
