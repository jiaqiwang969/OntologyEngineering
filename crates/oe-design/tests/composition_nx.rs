//! Frozen original R2 and N3 inputs: source composition is not native evidence.
use oe_design::{Constraint, Type, Validation, composition};
use std::path::{Path, PathBuf};

const SOURCE_FILES: [&str; 4] = [
    "specs/engineering.toml",
    "specs/foundation/engineering-v1.toml",
    "specs/modules/design-composition.toml",
    "specs/modules/nx-native-assembly.toml",
];
const ORIGINAL_APPEND: &[u8] = include_bytes!("fixtures/nx-native-assembly-original-append.toml");

fn current_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    std::fs::write(path.join("SKILL.md"), "Frozen R2/N3 source control").unwrap();
    let nx = [
        b"schema = \"oev2.design-fragment/2\"\n\n".as_slice(),
        ORIGINAL_APPEND,
    ]
    .concat();
    let original_root =
        std::str::from_utf8(include_bytes!("fixtures/composition-r2-root.toml")).unwrap();
    let manifest = format!(
        "{original_root}\n[[fragments]]\npath = \"modules/nx-native-assembly.toml\"\nformat = \"oev2.design-fragment/2\"\nsha256 = \"{}\"\n",
        oe_core::sha256(&nx)
    );
    // Exact original semantic sources, rather than later evolving modules.
    for (name, bytes) in [
        (SOURCE_FILES[0], manifest.as_bytes()),
        (
            SOURCE_FILES[1],
            include_bytes!("fixtures/composition-r2-foundation.toml").as_slice(),
        ),
        (
            SOURCE_FILES[2],
            include_bytes!("fixtures/composition-r2-module.toml").as_slice(),
        ),
        (SOURCE_FILES[3], nx.as_slice()),
    ] {
        std::fs::create_dir_all(path.join(name).parent().unwrap()).unwrap();
        std::fs::write(path.join(name), bytes).unwrap();
    }
    (dir, path)
}

fn nx_original() -> toml::Value {
    assert_eq!(ORIGINAL_APPEND.len(), 68654);
    assert_eq!(
        oe_core::sha256(ORIGINAL_APPEND),
        "bb71e6c53df4655e98c9bb86364d46f16ea07346e8c01d8c2e024ba225c4fd8f"
    );
    toml::from_str(std::str::from_utf8(ORIGINAL_APPEND).unwrap()).unwrap()
}

fn resign_nx(root: &Path) {
    let bytes = std::fs::read(root.join("specs/modules/nx-native-assembly.toml")).unwrap();
    let root_path = root.join("specs/engineering.toml");
    let mut manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(&root_path).unwrap()).unwrap();
    let entry = manifest["fragments"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["path"].as_str() == Some("modules/nx-native-assembly.toml"))
        .unwrap();
    entry["sha256"] = toml::Value::String(oe_core::sha256(bytes));
    std::fs::write(root_path, toml::to_string(&manifest).unwrap()).unwrap();
}

#[test]
fn composed_nx_keeps_complete_current_r2_and_original_n3_definitions() {
    let (_dir, root) = fixture();
    let foundation = root.join("specs/foundation/engineering-v1.toml");
    assert_eq!(
        oe_core::sha256(std::fs::read(&foundation).unwrap()),
        "5c6e03bc11739e504d96bca839e59ce0f22fedc5ba11e897f2cee975cd29cfef"
    );
    let mut expected = oe_design::load(&foundation).unwrap();
    let composition_bytes =
        std::fs::read(root.join("specs/modules/design-composition.toml")).unwrap();
    assert_eq!(
        oe_core::sha256(&composition_bytes),
        "2905caf59317471ec8b04d5bf2e874fc356c2eb49476facacb1ea095594efb42"
    );
    let composition_original: toml::Value =
        toml::from_str(std::str::from_utf8(&composition_bytes).unwrap()).unwrap();
    let base_constraints: Vec<Constraint> = composition_original["constraints"]
        .clone()
        .try_into()
        .unwrap();
    let base_validations: Vec<Validation> = composition_original["validations"]
        .clone()
        .try_into()
        .unwrap();
    assert_eq!(base_constraints.len(), 5);
    assert_eq!(base_validations.len(), 10);
    expected.constraints.extend(base_constraints);
    expected.validations.extend(base_validations);
    let original = nx_original();
    let types: Vec<Type> = original["types"].clone().try_into().unwrap();
    let constraints: Vec<Constraint> = original["constraints"].clone().try_into().unwrap();
    let validations: Vec<Validation> = original["validations"].clone().try_into().unwrap();
    assert_eq!(types.len(), 13);
    assert_eq!(constraints.len(), 5);
    assert_eq!(validations.len(), 8);
    expected.types.extend(types);
    expected.constraints.extend(constraints);
    expected.validations.extend(validations);
    let expected = expected.normalize().unwrap();
    let actual = composition::load(&root, &root.join("specs/engineering.toml")).unwrap();
    assert_eq!(
        expected.canonical().unwrap(),
        actual.design.canonical().unwrap()
    );
    assert_eq!(expected.rdf().unwrap(), actual.design.rdf().unwrap());
    assert_eq!(actual.snapshot.files.len(), 4);
    assert!(actual.snapshot.total_bytes > composition::transport::FILE_BYTES);
    assert!(actual.snapshot.total_bytes <= composition::transport::TOTAL_BYTES);
    for name in SOURCE_FILES {
        let bytes = std::fs::read(root.join(name)).unwrap();
        assert_eq!(actual.snapshot.files[name], bytes);
        assert_eq!(actual.snapshot.hashes[name], oe_core::sha256(&bytes));
    }
    let output: serde_json::Value = serde_json::from_slice(&actual.normalized().unwrap()).unwrap();
    assert_eq!(output["schema"], "oev2.normalized-design/2");
    assert_eq!(output["semantic_ast_schema"], "oev2.design/1");
    assert_eq!(output["design"], serde_json::to_value(expected).unwrap());
    let source: serde_json::Value =
        serde_json::from_slice(&actual.source_bytes().unwrap()).unwrap();
    assert_eq!(source["files"].as_array().unwrap().len(), 4);
    assert!(
        !source["files"][3]["field_locators"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let historical_binding =
        composition::binding(&oe_core::SkillRoot::new(&root).unwrap(), &actual).unwrap();
    assert_eq!(historical_binding["status"], "unmatched_or_uncaptured");
    // Current compiled authority is still required on the complete current
    // source set; the frozen historical control never inherits that identity.
    let current = current_root();
    let captured = composition::load(&current, &current.join("specs/engineering.toml")).unwrap();
    let binding =
        composition::binding(&oe_core::SkillRoot::new(&current).unwrap(), &captured).unwrap();
    assert_eq!(binding["status"], "matched");
}

#[test]
fn nx_fragment_missing_or_changed_original_bytes_refuse_without_replacing_outputs() {
    let (_dir, root) = fixture();
    let sentinel = root.join("generated/original-marker");
    std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    std::fs::write(&sentinel, b"preserved historical generation").unwrap();
    let fragment = root.join("specs/modules/nx-native-assembly.toml");
    let original = std::fs::read(&fragment).unwrap();
    let loaded = composition::load(&root, &root.join("specs/engineering.toml")).unwrap();
    std::fs::remove_file(&fragment).unwrap();
    assert!(composition::load(&root, &root.join("specs/engineering.toml")).is_err());
    std::fs::write(&fragment, &original).unwrap();
    loaded.verify(&root).unwrap();
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n# changed original source\n");
    std::fs::write(&fragment, changed).unwrap();
    let error = composition::load(&root, &root.join("specs/engineering.toml"))
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("design_composition_source_sha_mismatch"),
        "{error}"
    );
    assert!(loaded.verify(&root).is_err());
    std::fs::write(&fragment, original).unwrap();
    loaded.verify(&root).unwrap();
    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"preserved historical generation"
    );
}

#[test]
fn resigning_changed_nx_source_does_not_create_current_capture_or_duplicate_authority() {
    let (_dir, root) = fixture();
    let fragment = root.join("specs/modules/nx-native-assembly.toml");
    let original = std::fs::read_to_string(&fragment).unwrap();
    let changed = original.replacen(
        "旧单part参数预算",
        "changed source scope旧单part参数预算",
        1,
    );
    assert_ne!(changed, original);
    std::fs::write(&fragment, &changed).unwrap();
    resign_nx(&root);
    let loaded = composition::load(&root, &root.join("specs/engineering.toml")).unwrap();
    let binding = composition::binding(&oe_core::SkillRoot::new(&root).unwrap(), &loaded).unwrap();
    assert_eq!(binding["status"], "unmatched_or_uncaptured");
    let duplicate = format!(
        "{original}{}",
        std::str::from_utf8(ORIGINAL_APPEND).unwrap()
    );
    std::fs::write(&fragment, duplicate).unwrap();
    resign_nx(&root);
    let error = composition::load(&root, &root.join("specs/engineering.toml"))
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("duplicate"), "{error}");
}
