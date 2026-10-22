//! Anonymous in-memory policy counterexamples, not Cargo execution evidence.
use oe_windows_controller::{build_config::*, build_execution::prepare_plan};
#[path = "support/build_execution_fixture.rs"]
mod support;

#[test]
fn supported_workspace_target_is_bound_and_generated_config_is_offline_only() {
    let (spec, source) = support::fixture();
    let plan = prepare_plan(&spec, &source).unwrap();
    let config = prepare_config(&plan, &source).unwrap();
    assert_eq!(config.bytes(), b"[net]\noffline = true\n");
    assert!(config.record().workspace_config.is_none());
    assert_eq!(config.record().cargo_resolution, "not_executed");
    assert_eq!(config.record().vendor_checksums, "not_evaluated");
    let raw = b"[build]\ntarget-dir = \"var/builds/rust-target\"\n";
    let (spec, source) = support::fixture_config(Some((".cargo/config.toml", raw)), false, false);
    let plan = prepare_plan(&spec, &source).unwrap();
    let config = prepare_config(&plan, &source).unwrap();
    config
        .record()
        .workspace_config
        .as_ref()
        .unwrap()
        .identity
        .verify(raw)
        .unwrap();
    assert_eq!(
        config.record().target_override_environment,
        "CARGO_TARGET_DIR"
    );
}

#[test]
fn arbitrary_effective_configuration_and_legacy_precedence_are_rejected() {
    for raw in [
        "[build]\ntarget-dir='elsewhere'\n",
        "[build]\ntarget-dir='var/builds/rust-target'\nrustc-wrapper='evil.exe'\n",
        "[env]\nRUSTC_WRAPPER='evil.exe'\n",
        "[source.crates-io]\nreplace-with='external'\n",
        "[target.x86_64-pc-windows-gnu]\nlinker='evil.exe'\n",
        "[net]\noffline=false\n",
        "[build]\ntarget-dir='var/builds/rust-target'\n[alias]\nbuild='!evil.exe'\n",
        "[build]\ntarget-dir='var/builds/rust-target'\ntarget-dir='other'\n",
    ] {
        let (spec, source) =
            support::fixture_config(Some((".cargo/config.toml", raw.as_bytes())), false, false);
        let plan = prepare_plan(&spec, &source).unwrap();
        assert!(prepare_config(&plan, &source).is_err(), "{raw}");
    }
    let (spec, source) = support::fixture_config(Some((".cargo/config", b"")), false, false);
    assert!(prepare_config(&prepare_plan(&spec, &source).unwrap(), &source).is_err());
}

#[test]
fn registry_dependencies_require_source_vendor_without_claiming_resolution() {
    let (spec, source) = support::fixture_config(None, true, false);
    assert!(prepare_config(&prepare_plan(&spec, &source).unwrap(), &source).is_err());
    let (mut spec, source) = support::fixture_config(None, true, true);
    spec.build.root_directory = "C:/builds/材料 %literal%".into();
    let plan = prepare_plan(&spec, &source).unwrap();
    let config = prepare_config(&plan, &source).unwrap();
    let decoded: toml::Table =
        toml::from_str(std::str::from_utf8(config.bytes()).unwrap()).unwrap();
    assert_eq!(
        decoded["source"]["oe-owned-vendor"]["directory"]
            .as_str()
            .unwrap(),
        format!("{}/vendor", plan.record().source_directory)
    );
    assert_eq!(
        decoded["source"]["crates-io"]["replace-with"].as_str(),
        Some("oe-owned-vendor")
    );
    assert_eq!(decoded["net"]["offline"].as_bool(), Some(true));
    assert_eq!(config.record().registry_packages, 1);
    assert_eq!(config.record().cargo_resolution, "not_executed");
    // Third-party nested config is retained source, not a workspace ancestor.
    let (spec, source) = support::fixture_config(
        Some((
            "vendor/dependency/.cargo/config.toml",
            b"[net]\noffline=false\n",
        )),
        true,
        true,
    );
    prepare_config(&prepare_plan(&spec, &source).unwrap(), &source).unwrap();
}

#[test]
fn config_cannot_use_a_different_verified_archive() {
    let (spec, source) = support::fixture();
    let (_, other) = support::fixture_config(None, true, true);
    assert!(prepare_config(&prepare_plan(&spec, &source).unwrap(), &other).is_err());
}
