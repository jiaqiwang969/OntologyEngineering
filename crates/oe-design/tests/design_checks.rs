use std::path::Path;

fn baseline() -> oe_design::Design {
    oe_design::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("specs/engineering.toml"),
    )
    .unwrap()
}

#[test]
fn test_helpers_cannot_satisfy_a_production_constraint() {
    let d = baseline();
    let c = &d.constraints[0];
    let m = d.modules.iter().find(|m| m.id == c.module).unwrap();
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let directory = root_path.join(&m.rust_path);
    std::fs::create_dir_all(directory.join("src")).unwrap();
    std::fs::create_dir_all(directory.join("tests")).unwrap();
    std::fs::write(directory.join("src/lib.rs"), "pub fn unrelated() {}\n").unwrap();
    std::fs::write(
        directory.join("tests/helper.rs"),
        format!("fn {}() {{}}\n", c.check),
    )
    .unwrap();
    let report = oe_design::inspection::inspect(&root_path, &d).unwrap();
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "constraint_execution_missing" && f["subject"] == c.id)
    );
}

#[test]
fn conditional_test_modules_do_not_count_and_not_test_functions_do_count() {
    let d = baseline();
    let c = &d.constraints[0];
    let cases = [
        (
            format!(
                "#[cfg(all(test, feature = \"fixture\"))] mod fixture {{ pub fn {}() {{}} }}",
                c.check
            ),
            false,
        ),
        (
            format!(
                "#[cfg(test)] mod fixture {{ struct Check; impl Check {{ fn {}() {{}} }} }}",
                c.check
            ),
            false,
        ),
        (format!("#[cfg(not(test))] pub fn {}() {{}}", c.check), true),
        (
            format!(
                "#[cfg_attr(not(test), cfg(test))] pub fn {}() {{}}",
                c.check
            ),
            false,
        ),
    ];
    let mut mismatches = Vec::new();
    for (source, expected_present) in cases {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let path = root.join(&c.rust_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &source).unwrap();
        let report = oe_design::inspection::inspect(&root, &d).unwrap();
        let missing = report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "constraint_execution_missing" && f["subject"] == c.id);
        if missing == expected_present {
            mismatches.push(source);
        }
    }
    assert!(
        mismatches.is_empty(),
        "misclassified production checks: {}",
        mismatches.join("\n")
    );
}

#[test]
fn an_unrelated_same_name_checker_cannot_satisfy_the_declared_source() {
    let d = baseline();
    let c = &d.constraints[0];
    let m = d.modules.iter().find(|m| m.id == c.module).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(&m.rust_path).join("src")).unwrap();
    let declared = root.join(&c.rust_path);
    std::fs::create_dir_all(declared.parent().unwrap()).unwrap();
    std::fs::write(&declared, "pub fn unrelated() {}\n").unwrap();
    std::fs::write(
        root.join(&m.rust_path).join("src/unrelated.rs"),
        format!("pub fn {}() {{}}\n", c.check),
    )
    .unwrap();
    let report = oe_design::inspection::inspect(&root, &d).unwrap();
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["subject"] == c.id && f["code"] == "constraint_execution_missing")
    );
}

#[test]
fn malformed_identity_and_rdf_collisions_are_rejected() {
    let mut d = baseline();
    d.requirements[0].id = "malformed> <urn:injected>".into();
    assert!(d.validate().is_err());
    let mut d = baseline();
    d.sources[0].id = d.modules[0].id.clone();
    assert!(d.validate().is_err());
}

#[test]
fn type_changes_reach_the_interface_owner_and_callers() {
    let a = baseline();
    let mut b = a.clone();
    b.types
        .iter_mut()
        .find(|t| t.name == "ReviewRequest")
        .unwrap()
        .fields[0]
        .meaning
        .push_str(" revised meaning");
    let changes = oe_design::changes(&a, &b).unwrap();
    let affected = b.impact(&changes);
    assert!(affected.contains("kernel"));
    assert!(affected.contains("cli"));
}

#[test]
fn normalization_and_rdf_ignore_editor_order() {
    let a = baseline();
    let mut b = a.clone();
    b.modules.reverse();
    b.requirements.reverse();
    b.types.reverse();
    for t in &mut b.types {
        t.fields.reverse();
    }
    assert_eq!(
        a.canonical().unwrap(),
        b.clone().normalize().unwrap().canonical().unwrap()
    );
    assert_eq!(a.rdf().unwrap(), b.normalize().unwrap().rdf().unwrap());
}
