//! The application composition raises only its declared derived RDF capacity.
use oe_design::{derived_model_limit, generation};
use std::fs;

#[test]
fn profiles_are_versioned_and_unknown_versions_fail() {
    for schema in [
        "oev2.design/1",
        "oev2.design/2",
        "oev2.design/3",
        "oev2.design/4",
    ] {
        assert_eq!(derived_model_limit(schema).unwrap(), 32 * 1024 * 1024);
        assert_eq!(
            generation::resource_profile_for_input(schema).unwrap(),
            generation::resource_profile()
        );
    }
    let p = generation::resource_profile_for_input("oev2.design/5").unwrap();
    assert_eq!(p["schema"], "oev2.derived-design-model-resource/3");
    assert_eq!(p["derived_model_bytes"], 64 * 1024 * 1024);
    assert_eq!(p["other_artifact_bytes"], 16 * 1024 * 1024);
    assert_eq!(p["editable_spec_bytes"], 4 * 1024 * 1024);
    for schema in ["", "oev2.design/6", "oev2.generation/5"] {
        assert!(derived_model_limit(schema).is_err());
        assert!(generation::resource_profile_for_input(schema).is_err());
        assert!(
            generation::snapshot_for_input(tempfile::tempdir().unwrap().path(), schema).is_err()
        );
    }
}

#[test]
fn application_model_hash_includes_extended_bytes_and_rejects_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let path = root.join("model.ttl");
    let mut bytes = vec![b'x'; 32 * 1024 * 1024 + 1];
    fs::write(&path, &bytes).unwrap();
    assert!(generation::snapshot(&root).is_err());
    for schema in ["oev2.design/2", "oev2.design/3", "oev2.design/4"] {
        assert!(generation::snapshot_for_input(&root, schema).is_err());
    }
    let before = generation::snapshot_for_input(&root, "oev2.design/5").unwrap();
    assert_eq!(before["model.ttl"], oe_core::sha256(&bytes));
    *bytes.last_mut().unwrap() = b'z';
    fs::write(&path, &bytes).unwrap();
    let after = generation::snapshot_for_input(&root, "oev2.design/5").unwrap();
    assert_eq!(after["model.ttl"], oe_core::sha256(&bytes));
    assert_ne!(before["model.ttl"], after["model.ttl"]);
    bytes.resize(64 * 1024 * 1024, b' ');
    *bytes.last_mut().unwrap() = b'q';
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        generation::snapshot_for_input(&root, "oev2.design/5").unwrap()["model.ttl"],
        oe_core::sha256(&bytes)
    );
    bytes.push(b'!');
    fs::write(&path, &bytes).unwrap();
    assert!(generation::snapshot_for_input(&root, "oev2.design/5").is_err());
    assert_eq!(fs::metadata(&path).unwrap().len(), 67_108_865);
}

#[test]
fn application_profile_does_not_expand_other_artifacts() {
    let bytes = vec![b'x'; 16 * 1024 * 1024 + 1];
    for name in [
        "contracts.rs",
        "other.ttl",
        "nested/model.ttl",
        "generated/model.ttl",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(
            generation::snapshot_for_input(&root, "oev2.design/5")
                .unwrap()
                .len(),
            1
        );
        fs::write(&path, &bytes).unwrap();
        assert!(
            generation::snapshot_for_input(&root, "oev2.design/5").is_err(),
            "{name}"
        );
    }
}
