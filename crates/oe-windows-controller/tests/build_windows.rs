//! Actual filesystem fixtures, opt-in on Windows. Synthetic compiler/SDK bytes
//! are never loaded/executed and cannot establish a usable installed toolchain.
#![cfg(all(windows, feature = "acceptance-fixtures"))]
use oe_nx_protocol::BlobIdentity;
use oe_windows_controller::{
    adapter,
    build_execution::{ExecutionSpecV2, prepare_plan},
    build_source::VerifiedSourceArchive,
    build_windows::materialize,
    windows::StableRoot,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "support/build_execution_fixture.rs"]
mod support;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn fixture(name: &str) -> (StableRoot, ExecutionSpecV2, VerifiedSourceArchive, PathBuf) {
    let base = PathBuf::from(std::env::var_os("OE_OWNED_BUILD_TEST_ROOT").expect(
        "Windows fixtures require an explicit existing task-owned OE_OWNED_BUILD_TEST_ROOT",
    ));
    let base_pin = StableRoot::open(&base, None).unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let unique = format!(
        "prep-材料 空间-{name}-{}-{stamp}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let directory = base_pin
        .create_directory_new(std::ffi::OsStr::new(&unique))
        .unwrap();
    let path = directory.path().to_owned();
    let root = StableRoot::open(&path, None).unwrap();
    let (mut spec, source) = support::fixture();
    // Plan paths are ordinary drive paths; the OS receipt preserves final paths.
    spec.build.root_directory = path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    spec.build.root_identity = adapter::root_identity(&root);
    for (name, tool) in [
        ("cargo", &mut spec.build.tools.cargo),
        ("rustc", &mut spec.build.tools.rustc),
        ("gnu", &mut spec.build.tools.gnu_linker),
        ("msvc", &mut spec.build.tools.msvc_cl),
        ("msvc", &mut spec.build.tools.msvc_link),
    ] {
        let filename = tool.path.rsplit('/').next().unwrap().to_owned();
        let file = path.join("tools").join(name).join(&filename);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let bytes = format!("anonymous {name} {filename}; never executed").into_bytes();
        std::fs::write(&file, &bytes).unwrap();
        tool.path = format!("{}/tools/{name}/{filename}", spec.build.root_directory);
        tool.identity = BlobIdentity::of(&bytes);
    }
    spec.build.tools.windows_directory = format!("{}/windows", spec.build.root_directory);
    spec.build.tools.include_directories = vec![format!("{}/include", spec.build.root_directory)];
    spec.build.tools.library_directories = vec![format!("{}/lib", spec.build.root_directory)];
    for name in ["windows/System32", "include", "lib"] {
        std::fs::create_dir_all(path.join(name)).unwrap();
    }
    spec.build.sdk.root = format!("{}/sdk", spec.build.root_directory);
    for entry in spec
        .build
        .sdk
        .files
        .iter_mut()
        .chain(spec.build.sdk.runtime.libraries.iter_mut())
    {
        let file = path.join("sdk").join(entry.path.as_str());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let bytes = format!("anonymous SDK {} ; never loaded", entry.path.as_str()).into_bytes();
        std::fs::write(file, &bytes).unwrap();
        entry.identity = BlobIdentity::of(&bytes);
    }
    (root, spec, source, path)
}

#[test]
fn complete_readback_pins_inputs_and_refuses_repeated_build_id() {
    let (root, spec, source, path) = fixture("complete");
    let plan = prepare_plan(&spec, &source).unwrap();
    let mut prepared = materialize(&root, &spec, &plan, &source).unwrap();
    assert_eq!(prepared.receipt().status, "prepared_not_executed");
    assert_eq!(prepared.receipt().execution, "not_started");
    assert_eq!(
        prepared.receipt().source_files,
        source.members().len() as u32
    );
    assert!(
        prepared
            .receipt()
            .ancestor_config
            .iter()
            .all(|a| a.configuration_files_absent)
    );
    prepared.revalidate_source_and_inputs().unwrap();
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(path.join("tools/cargo/cargo.exe"))
            .is_err()
    );
    let source_file = prepared.output_path().join("source/workspace/Cargo.lock");
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(source_file)
            .is_err()
    );
    assert!(materialize(&root, &spec, &plan, &source).is_err());
    assert!(
        prepared
            .output_path()
            .join("receipts/materialization.json")
            .is_file()
    );
}

#[test]
fn actual_ancestor_config_blocks_before_output_creation() {
    for name in ["config", "config.toml"] {
        let (root, spec, source, path) = fixture(name);
        std::fs::create_dir(path.join(".cargo")).unwrap();
        std::fs::write(
            path.join(".cargo").join(name),
            b"[build]\nrustc-wrapper='not-allowed.exe'\n",
        )
        .unwrap();
        let plan = prepare_plan(&spec, &source).unwrap();
        assert!(materialize(&root, &spec, &plan, &source).is_err());
        assert!(
            !path
                .join(format!("build-{}", spec.build.build_id.as_str()))
                .exists()
        );
    }
}

#[test]
fn selected_input_hash_mismatch_blocks_before_owned_tree_creation() {
    let (root, spec, source, path) = fixture("mismatch");
    std::fs::write(path.join("sdk/UGOPEN/uf.h"), b"changed selected header").unwrap();
    let plan = prepare_plan(&spec, &source).unwrap();
    assert!(materialize(&root, &spec, &plan, &source).is_err());
    assert!(
        !path
            .join(format!("build-{}", spec.build.build_id.as_str()))
            .exists()
    );
}

#[test]
fn linked_inputs_and_nondirectory_cargo_or_nx_writer_roots_are_not_accepted() {
    for mode in ["hardlink", "cargo-file", "nx-root"] {
        let (root, spec, source, path) = fixture(mode);
        match mode {
            "hardlink" => std::fs::hard_link(
                path.join("tools/cargo/cargo.exe"),
                path.join("cargo-alias.exe"),
            )
            .unwrap(),
            "cargo-file" => std::fs::write(path.join(".cargo"), b"not a directory").unwrap(),
            _ => std::fs::write(path.join(".nx-writer.lock"), b"").unwrap(),
        }
        let plan = prepare_plan(&spec, &source).unwrap();
        assert!(materialize(&root, &spec, &plan, &source).is_err());
        assert!(
            !path
                .join(format!("build-{}", spec.build.build_id.as_str()))
                .exists()
        );
    }
}

#[test]
fn full_revalidation_rejects_added_source_directory_and_new_ancestor_config() {
    let (root, spec, source, path) = fixture("addition");
    let plan = prepare_plan(&spec, &source).unwrap();
    let mut prepared = materialize(&root, &spec, &plan, &source).unwrap();
    // Pins prevent modifying existing members, not adding new directory entries.
    std::fs::create_dir(
        prepared
            .output_path()
            .join("source/workspace/unlisted-empty"),
    )
    .unwrap();
    assert!(prepared.revalidate_source_and_inputs().is_err());
    drop(prepared);
    let (root2, spec2, source2, _) = fixture("config-addition");
    let plan2 = prepare_plan(&spec2, &source2).unwrap();
    let mut second = materialize(&root2, &spec2, &plan2, &source2).unwrap();
    std::fs::create_dir(second.output_path().join(".cargo")).unwrap();
    std::fs::write(
        second.output_path().join(".cargo/config.toml"),
        b"[env]\nINJECT='true'\n",
    )
    .unwrap();
    assert!(second.revalidate_source_and_inputs().is_err());
    assert!(path.exists()); // Failures retain owned evidence; no automatic deletion.
}

#[test]
fn prelaunch_revalidation_refuses_foreign_target_cache() {
    let (root, spec, source, _) = fixture("target-cache");
    let plan = prepare_plan(&spec, &source).unwrap();
    let mut prepared = materialize(&root, &spec, &plan, &source).unwrap();
    std::fs::write(
        prepared.output_path().join("target/foreign-object"),
        b"not produced by this build",
    )
    .unwrap();
    assert!(prepared.revalidate_source_and_inputs().is_err());
}
