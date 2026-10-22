//! Frozen original1357 foundation plus its original explicit composition delta.
use oe_design::{Constraint, Validation, composition};

#[test]
fn canonical_composition_preserves_complete_foundation_and_exact_module_delta() {
    // Retain the exact original R2 three-source control. Later explicitly
    // declared modules have their own complete source-bound integration test.
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap();
    let original_root = include_bytes!("fixtures/composition-r2-root.toml");
    assert_eq!(
        oe_core::sha256(original_root),
        "45d2b059e39a1c40823df95bdd9765e19c69e2aa947e377730208a366ccf1b6f"
    );
    std::fs::create_dir_all(root.join("specs/foundation")).unwrap();
    std::fs::create_dir_all(root.join("specs/modules")).unwrap();
    std::fs::write(root.join("specs/engineering.toml"), original_root).unwrap();
    for (name, bytes) in [
        (
            "specs/foundation/engineering-v1.toml",
            include_bytes!("fixtures/composition-r2-foundation.toml").as_slice(),
        ),
        (
            "specs/modules/design-composition.toml",
            include_bytes!("fixtures/composition-r2-module.toml").as_slice(),
        ),
    ] {
        std::fs::write(root.join(name), bytes).unwrap();
    }
    let foundation = root.join("specs/foundation/engineering-v1.toml");
    let raw = std::fs::read(&foundation).unwrap();
    assert_eq!(
        oe_core::sha256(&raw),
        "5c6e03bc11739e504d96bca839e59ce0f22fedc5ba11e897f2cee975cd29cfef"
    );
    let mut expected = oe_design::load(&foundation).unwrap();
    let fragment: toml::Value = toml::from_str(
        &std::fs::read_to_string(root.join("specs/modules/design-composition.toml")).unwrap(),
    )
    .unwrap();
    let constraints: Vec<Constraint> = fragment["constraints"].clone().try_into().unwrap();
    let validations: Vec<Validation> = fragment["validations"].clone().try_into().unwrap();
    assert_eq!(constraints.len(), 5);
    assert_eq!(validations.len(), 10);
    expected.constraints.extend(constraints);
    expected.validations.extend(validations);
    let expected = expected.normalize().unwrap();
    let actual = composition::load(&root, &root.join("specs/engineering.toml")).unwrap();
    assert_eq!(
        expected.canonical().unwrap(),
        actual.design.canonical().unwrap()
    );
    assert_eq!(expected.rdf().unwrap(), actual.design.rdf().unwrap());
    assert_eq!(actual.snapshot.files.len(), 3);
    assert_eq!(
        actual.snapshot.files["specs/foundation/engineering-v1.toml"],
        raw
    );
    assert!(actual.snapshot.total_bytes > composition::transport::FILE_BYTES);
    let normalized: serde_json::Value =
        serde_json::from_slice(&actual.normalized().unwrap()).unwrap();
    assert_eq!(normalized["input_schema"], "oev2.design/2");
    assert_eq!(normalized["semantic_ast_schema"], "oev2.design/1");
    assert_eq!(
        normalized["design"],
        serde_json::to_value(expected).unwrap()
    );
}
