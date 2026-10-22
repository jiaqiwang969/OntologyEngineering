//! Independent byte limits for derived RDF; no engineering success is inferred.
use oe_design::generation;
use std::{fs, path::Path};

const OLD_BYTES: usize = 16 * 1024 * 1024;
const MODEL_BYTES: usize = 32 * 1024 * 1024;

#[test]
fn only_the_exact_public_derived_model_path_has_the_larger_profile() {
    use oe_core::source_limits::public_file_bytes;
    assert_eq!(
        public_file_bytes(Path::new("generated/model.ttl")),
        33_554_432
    );
    for path in [
        "model.ttl",
        "other/model.ttl",
        "generated/nested/model.ttl",
        "nested/generated/model.ttl",
        "generated/model.ttl.bak",
        "generated/Model.ttl",
        "./generated/model.ttl",
        "generated/./model.ttl",
        "generated//model.ttl",
        "generated/../generated/model.ttl",
        "/generated/model.ttl",
        "specs/engineering.toml",
        "templates/pack/contracts.rs.tera",
    ] {
        assert_eq!(public_file_bytes(Path::new(path)), 16_777_216, "{path}");
    }
    #[cfg(not(windows))]
    assert_eq!(
        public_file_bytes(Path::new("generated\\model.ttl")),
        16_777_216
    );
    #[cfg(windows)]
    assert_eq!(
        public_file_bytes(Path::new("generated\\model.ttl")),
        33_554_432
    );
}

#[test]
fn generation_reads_complete_model_bytes_at_both_boundaries_and_rejects_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let path = root.join("model.ttl");
    let mut bytes = vec![b'x'; OLD_BYTES];
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        generation::snapshot(&root).unwrap()["model.ttl"],
        oe_core::sha256(&bytes)
    );
    assert_eq!(
        oe_core::read_regular_bytes(&path, OLD_BYTES).unwrap(),
        bytes
    );
    bytes.push(b'Z');
    fs::write(&path, &bytes).unwrap();
    let before = generation::snapshot(&root).unwrap();
    assert_eq!(before["model.ttl"], oe_core::sha256(&bytes));
    assert!(oe_core::read_regular_bytes(&path, OLD_BYTES).is_err());
    // The byte outside the former profile must participate in the identity.
    bytes[OLD_BYTES] = b'Y';
    fs::write(&path, &bytes).unwrap();
    let after = generation::snapshot(&root).unwrap();
    assert_eq!(after["model.ttl"], oe_core::sha256(&bytes));
    assert_ne!(before["model.ttl"], after["model.ttl"]);
    bytes.resize(MODEL_BYTES, b' ');
    bytes[MODEL_BYTES - 1] = b'Q';
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        generation::snapshot(&root).unwrap()["model.ttl"],
        oe_core::sha256(&bytes)
    );
    bytes.push(b'!');
    fs::write(&path, &bytes).unwrap();
    assert!(generation::snapshot(&root).is_err());
    assert_eq!(fs::metadata(path).unwrap().len(), 33_554_433);
}

#[test]
fn ordinary_and_nested_stage_files_keep_the_original_sixteen_mib_limit() {
    let bytes = vec![b'x'; OLD_BYTES + 1];
    for relative in [
        "contracts.rs",
        "model-copy.ttl",
        "nested/model.ttl",
        "generated/model.ttl",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &bytes[..OLD_BYTES]).unwrap();
        let before = generation::snapshot(&root).unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(
            before.values().next().unwrap(),
            &oe_core::sha256(&bytes[..OLD_BYTES])
        );
        fs::write(&path, &bytes).unwrap();
        assert!(generation::snapshot(&root).is_err(), "{relative}");
    }
}

#[test]
fn editable_spec_still_rejects_one_byte_over_its_original_four_mib_limit() {
    // This is the original standalone design/1 4MiB boundary. The current
    // canonical design/2 container requires its relative fragments and is
    // tested separately by composition.rs with independent /2 budgets.
    let mut bytes = include_bytes!("fixtures/composition-r2-foundation.toml").to_vec();
    assert_eq!(
        oe_core::sha256(&bytes),
        "5c6e03bc11739e504d96bca839e59ce0f22fedc5ba11e897f2cee975cd29cfef"
    );
    assert!(bytes.len() < 4 * 1024 * 1024);
    bytes.push(b'\n');
    bytes.resize(4 * 1024 * 1024, b' ');
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap().join("engineering.toml");
    fs::write(&path, &bytes).unwrap();
    oe_design::load(&path).unwrap();
    bytes.push(b' ');
    fs::write(&path, &bytes).unwrap();
    assert!(oe_design::load(&path).is_err());
}
