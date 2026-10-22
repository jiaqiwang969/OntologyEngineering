//! A test feature must not enable retired Python execution in a distributable library.
//! The executable is a canary: actual invocation leaves a marker in the selected root.
#![cfg(unix)]

use oe_core::SkillRoot;
use oe_semantic::{runtime, runtime_build, runtime_install};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

struct Fixture {
    path: PathBuf,
    root: SkillRoot,
    canary: PathBuf,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .canonicalize()
            .unwrap()
            .join("var/maintenance/runtime-retirement-tests")
            .join(format!("{}-{name}", std::process::id()));
        oe_core::create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "isolated retired runtime fixture\n").unwrap();
        let canary = path.join("canary-python");
        let marker = path.join("EXECUTED");
        let quoted = marker.to_str().unwrap().replace('\'', "'\\''");
        fs::write(
            &canary,
            format!("#!/bin/sh\nprintf triggered > '{quoted}'\nexit 0\n"),
        )
        .unwrap();
        fs::set_permissions(&canary, fs::Permissions::from_mode(0o700)).unwrap();
        let root = SkillRoot::new(&path).unwrap();
        Self { path, root, canary }
    }
    fn assert_untouched(&self) {
        assert!(
            !self.path.join("EXECUTED").exists(),
            "retired executable ran"
        );
        assert!(
            !self.path.join("var").exists(),
            "retired entry created evidence/cache"
        );
        assert!(
            !self.path.join("runtime").exists(),
            "retired entry created a runtime"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}

#[test]
fn test_feature_cannot_reenable_external_runtime_probe() {
    let f = Fixture::new("probe");
    let error = runtime::probe_external_runtime_with_limits(
        f.root.path(),
        &f.canary,
        Duration::from_secs(2),
        65536,
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "legacy_python_runtime_probe_retired_in_v2"
    );
    f.assert_untouched();
}

#[test]
fn test_feature_cannot_reenable_external_runtime_preflight() {
    let f = Fixture::new("preflight");
    let error = runtime_install::preflight(&f.root, &f.canary, std::path::Path::new("var/output"))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "legacy_python_runtime_install_retired_in_v2"
    );
    f.assert_untouched();
}

#[test]
fn test_feature_cannot_reenable_external_runtime_install() {
    let f = Fixture::new("install");
    let options = runtime_install::InstallOptions::new(f.canary.clone(), "var/output".into());
    let error = runtime_install::install(&f.root, &options).unwrap_err();
    assert_eq!(
        error.to_string(),
        "legacy_python_runtime_install_retired_in_v2"
    );
    f.assert_untouched();
}

#[test]
fn test_feature_cannot_reenable_external_wheel_build() {
    let f = Fixture::new("build");
    let mut options = runtime_build::BuildOptions::new(f.path.join("source"), "var/output".into());
    options.python = f.canary.clone();
    options.git = f.canary.clone();
    options.npm = f.canary.clone();
    options.node = f.canary.clone();
    let error = runtime_build::build_semantica(&f.root, &options).unwrap_err();
    assert_eq!(
        error.to_string(),
        "legacy_python_runtime_build_retired_in_v2"
    );
    f.assert_untouched();
}
