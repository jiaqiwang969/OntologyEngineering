//! Real inventory/receiver owners keep one precise derived-model byte exception.
//! These synthetic bytes establish transport identity, never RDF or design validity.
use oe_dist::package::{FORMAT, PortableManifest, SCOPE, SourceFile};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const ORDINARY_BYTES: usize = 16 * 1024 * 1024;
const MODEL_BYTES: usize = 32 * 1024 * 1024;
const REQUIRED: [&str; 5] = ["AGENTS.md", "LICENSE", "README.md", "SKILL.md", "VERSION"];

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in REQUIRED {
        fs::write(dir.path().join(name), b"synthetic receiver fixture\n").unwrap();
    }
    fs::create_dir_all(dir.path().join("runtime/vendor")).unwrap();
    fs::create_dir_all(dir.path().join("var")).unwrap();
    dir
}

fn command(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

fn success(result: &Output) {
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn add(root: &Path, name: &str, bytes: &[u8]) -> SourceFile {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    SourceFile {
        path: name.into(),
        sha256: oe_core::sha256(bytes),
        bytes: bytes.len() as u64,
    }
}

#[test]
fn evolution_public_identity_checks_the_complete_derived_model_and_detects_tail_drift() {
    let dir = fixture();
    let root = dir.path().canonicalize().unwrap();
    let mut bytes = vec![b' '; ORDINARY_BYTES + 1];
    bytes[ORDINARY_BYTES] = b'A';
    add(&root, "generated/model.ttl", &bytes);
    fs::write(
        root.join("var/not-executed-binary"),
        b"identity only; never executed",
    )
    .unwrap();
    let before = oe_evolution::public_manifest(&root).unwrap();
    assert_eq!(before["files"]["generated/model.ttl"]["bytes"], 16_777_217);
    assert_eq!(
        before["files"]["generated/model.ttl"]["sha256"],
        oe_core::sha256(&bytes)
    );
    let binary =
        oe_evolution::binary_reference(&root.join("var/not-executed-binary"), &root).unwrap();
    oe_evolution::verify_binary(&binary).unwrap();
    bytes[ORDINARY_BYTES] = b'B';
    fs::write(root.join("generated/model.ttl"), &bytes).unwrap();
    let changed = oe_evolution::public_manifest(&root).unwrap();
    assert_eq!(
        changed["files"]["generated/model.ttl"]["sha256"],
        oe_core::sha256(&bytes)
    );
    assert_ne!(before, changed);
    assert!(oe_evolution::verify_binary(&binary).is_err());
    fs::rename(
        root.join("generated/model.ttl"),
        root.join("generated/not-model.ttl"),
    )
    .unwrap();
    assert!(oe_evolution::public_manifest(&root).is_err());
    fs::rename(
        root.join("generated/not-model.ttl"),
        root.join("generated/model.ttl"),
    )
    .unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(root.join("generated/model.ttl"))
        .unwrap()
        .set_len(33_554_433)
        .unwrap();
    assert!(oe_evolution::public_manifest(&root).is_err());
}

#[test]
fn actual_manifest_and_receiver_transport_exact_boundary_bytes_then_reject_source_drift() {
    let dir = fixture();
    let root = dir.path().canonicalize().unwrap();
    let mut model = vec![b' '; MODEL_BYTES];
    model[MODEL_BYTES - 1] = b'Z';
    let expected = add(&root, "generated/model.ttl", &model);
    let ordinary = vec![b'x'; ORDINARY_BYTES];
    let ordinary_expected = add(&root, "generated/ordinary.bin", &ordinary);
    success(&command(
        &root,
        &["maintain", "manifest", "--output", "var/source.json"],
    ));
    let manifest: PortableManifest =
        serde_json::from_slice(&fs::read(root.join("var/source.json")).unwrap()).unwrap();
    assert!(manifest.files.contains(&expected));
    assert!(manifest.files.contains(&ordinary_expected));
    let received = command(
        &root,
        &[
            "maintain",
            "install",
            "--inventory",
            "var/source.json",
            "--target",
            "var/received",
        ],
    );
    success(&received);
    let outcome: Value = serde_json::from_slice(&received.stdout).unwrap();
    assert_eq!(outcome["status"], "source_received");
    assert_eq!(outcome["build"], "not_run");
    assert_eq!(
        fs::read(root.join("var/received/generated/model.ttl")).unwrap(),
        model
    );
    assert_eq!(
        fs::read(root.join("var/received/generated/ordinary.bin")).unwrap(),
        ordinary
    );
    model[MODEL_BYTES - 1] = b'Y';
    fs::write(root.join("generated/model.ttl"), &model).unwrap();
    let rejected = command(
        &root,
        &[
            "maintain",
            "install",
            "--inventory",
            "var/source.json",
            "--target",
            "var/drift-receiver",
        ],
    );
    assert_eq!(rejected.status.code(), Some(2));
    assert!(!root.join("var/drift-receiver").exists());
    assert_eq!(
        oe_core::sha256(fs::read(root.join("var/received/generated/model.ttl")).unwrap()),
        expected.sha256
    );
}

#[test]
fn refreshed_inventory_cannot_expand_alias_nested_or_ordinary_file_limits() {
    for (name, length) in [
        ("generated/model-copy.ttl", ORDINARY_BYTES + 1),
        ("generated/nested/model.ttl", ORDINARY_BYTES + 1),
        ("docs/generated/model.ttl", ORDINARY_BYTES + 1),
        ("generated/contracts.rs", ORDINARY_BYTES + 1),
        ("generated/model.ttl", MODEL_BYTES + 1),
    ] {
        let dir = fixture();
        let root = dir.path().canonicalize().unwrap();
        let bytes = vec![b' '; length];
        let input = add(&root, name, &bytes);
        assert!(oe_evolution::public_manifest(&root).is_err(), "{name}");
        let inventory_result = command(
            &root,
            &[
                "maintain",
                "manifest",
                "--output",
                "var/invalid-source.json",
            ],
        );
        assert_eq!(inventory_result.status.code(), Some(2), "{name}");
        assert!(!root.join("var/invalid-source.json").exists());
        // Every inventory digest and byte count is freshly correct. Correct
        // hashes cannot authorize a larger resource domain or publish a target.
        let mut files: Vec<_> = REQUIRED
            .into_iter()
            .map(|name| {
                let bytes = fs::read(root.join(name)).unwrap();
                SourceFile {
                    path: name.into(),
                    sha256: oe_core::sha256(&bytes),
                    bytes: bytes.len() as u64,
                }
            })
            .collect();
        files.push(input);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let inventory = PortableManifest {
            format: FORMAT.into(),
            entry: "SKILL.md".into(),
            scope: SCOPE.into(),
            files,
        };
        inventory.validate().unwrap();
        fs::write(
            root.join("var/rebound.json"),
            serde_json::to_vec(&inventory).unwrap(),
        )
        .unwrap();
        let rejected = command(
            &root,
            &[
                "maintain",
                "install",
                "--inventory",
                "var/rebound.json",
                "--target",
                "var/rejected",
            ],
        );
        assert_eq!(rejected.status.code(), Some(2), "{name}");
        assert!(!root.join("var/rejected").exists(), "{name}");
    }
}
