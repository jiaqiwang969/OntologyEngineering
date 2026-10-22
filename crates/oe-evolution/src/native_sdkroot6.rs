//! Selected Rustc native-child SDKROOT content domain. No budget or authority owner.
#[cfg(test)]
use oe_core::sha256 as private_fixture_sha256;
#[cfg(test)]
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, no_symlinks};
use std::{ffi::OsStr, path::Path};

pub(crate) const STRATEGY: &str =
    "rustc_1_95_macosx_26_4_bound_sdkroot_keep_selected_sdk_remove_dyld/1";
const COMPILER_SHA256: &str = "b829b733131d4e1673eeebd1f34d06ae1e9ff4977b051313cf42e2a9e79ecf1c";
const ALIAS_NAME: &str = "MacOSX26.4.sdk";
const ROOT_NAME: &str = "MacOSX.sdk";

pub(crate) fn inspect_policy(policy: &CodeNativeCargoLoaderPolicy6) -> Result<()> {
    ensure!(
        [
            "record_cargo_fallback_then_remove_from_native_child/6",
            STRATEGY
        ]
        .contains(&policy.child_environment_strategy.as_str()),
        "loader6_sdkroot_strategy_uncovered"
    );
    if policy.child_environment_strategy == STRATEGY {
        ensure!(
            policy.compiler.sha256 == COMPILER_SHA256
                && policy.compiler.bytes == 412_504
                && policy.compiler.mode == 0o755,
            "loader6_sdkroot_selected_rustc_source_domain_uncovered"
        );
    }
    Ok(())
}
#[cfg(unix)]
fn alias_snapshot(canonical: &Path, raw: &str) -> Result<CodeNativeSdkRootAlias6> {
    use std::os::unix::fs::PermissionsExt;
    ensure!(
        canonical.is_absolute() && canonical.file_name() == Some(OsStr::new(ROOT_NAME)),
        "loader6_sdkroot_selected_layout_root"
    );
    no_symlinks(canonical)?;
    ensure!(
        canonical.is_dir() && canonical.canonicalize()? == canonical,
        "loader6_sdkroot_canonical_directory_changed"
    );
    let parent = canonical.parent().context("loader6_sdkroot_parent")?;
    let expected = parent.join(ALIAS_NAME);
    ensure!(
        Path::new(raw) == expected && expected.to_str() == Some(raw),
        "loader6_sdkroot_original_value_not_selected"
    );
    no_symlinks(parent)?;
    let metadata = std::fs::symlink_metadata(&expected)?;
    ensure!(
        metadata.file_type().is_symlink(),
        "loader6_sdkroot_alias_not_symlink"
    );
    let link = std::fs::read_link(&expected)?;
    let link_text = link.to_str().context("loader6_sdkroot_alias_encoding")?;
    let mode = u64::from(metadata.permissions().mode() & 0o7777);
    ensure!(
        link_text == ROOT_NAME && mode == 0o755 && expected.canonicalize()? == canonical,
        "loader6_sdkroot_alias_edge_or_mode_changed"
    );
    Ok(CodeNativeSdkRootAlias6 {
        link_path: raw.into(),
        link_text: link_text.into(),
        link_text_sha256: sha256(link_text.as_bytes()),
        link_mode: mode,
    })
}
#[cfg(unix)]
pub(crate) fn inspect_actual(
    root: &SkillRoot,
    recipe: &CodeNativeBuildRecipe6,
    policy: &CodeNativeCargoLoaderPolicy6,
    value: Option<&OsStr>,
) -> Result<Option<CodeNativeCompilerSdkRootBinding6>> {
    inspect_policy(policy)?;
    if policy.child_environment_strategy != STRATEGY {
        ensure!(
            value.is_none(),
            "loader6_unselected_native_environment:SDKROOT"
        );
        return Ok(None);
    }
    inspect_policy(policy)?;
    ensure!(
        recipe.target == "aarch64-apple-darwin",
        "loader6_sdkroot_target_uncovered"
    );
    let raw = value
        .context("loader6_sdkroot_missing")?
        .to_str()
        .context("loader6_sdkroot_encoding")?;
    let layout = crate::native_sdk4::inspect_sdk4(root, &recipe.sdk_layout)?;
    let alias = alias_snapshot(Path::new(&layout.canonical_root), raw)?;
    let output = CodeNativeCompilerSdkRootBinding6 {
        schema: "oev2.native-compiler-sdkroot-binding/6".into(),
        sdk_layout: recipe.sdk_layout.clone(),
        raw_value: raw.into(),
        canonical_root: layout.canonical_root.clone(),
        root_alias: alias.clone(),
    };
    output
        .validate()
        .map_err(|e| anyhow::anyhow!("loader6_sdkroot_binding_contract:{e:?}"))?;
    ensure!(
        alias_snapshot(Path::new(&layout.canonical_root), raw)? == alias
            && crate::native_sdk4::inspect_sdk4(root, &recipe.sdk_layout)? == layout,
        "loader6_sdkroot_layout_or_alias_changed_during_read"
    );
    Ok(Some(output))
}
#[cfg(unix)]
pub(crate) fn recheck_actual(
    root: &SkillRoot,
    recipe: &CodeNativeBuildRecipe6,
    expected: &Option<CodeNativeCompilerSdkRootBinding6>,
) -> Result<()> {
    let policy = crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?;
    ensure!(
        inspect_actual(
            root,
            recipe,
            &policy,
            std::env::var_os("SDKROOT").as_deref()
        )? == *expected,
        "loader6_actual_sdkroot_changed_before_or_after_dispatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Private fixture construction binds the fixed observed symlink mode. No
    // SDK or production guard is modified. macOS new symlinks default to 0777.
    fn fixed_alias(path: &Path, target: &str) {
        std::os::unix::fs::symlink(target, path).unwrap();
        let path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(
            unsafe {
                libc::fchmodat(
                    libc::AT_FDCWD,
                    path.as_ptr(),
                    0o755,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            },
            0
        );
    }

    #[test]
    fn sdkroot_alias_raw_and_canonical_paths_are_distinct_typed_facts() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap().join(ROOT_NAME);
        std::fs::create_dir(&root).unwrap();
        let alias = root.with_file_name(ALIAS_NAME);
        fixed_alias(&alias, ROOT_NAME);
        let fact = alias_snapshot(&root, alias.to_str().unwrap()).unwrap();
        assert_ne!(fact.link_path, root.to_str().unwrap());
        assert_eq!(fact.link_text, ROOT_NAME);
        assert_eq!(fact.link_text_sha256, sha256(ROOT_NAME.as_bytes()));
        assert_eq!(fact.link_mode, 0o755);
        for bad in [
            root.clone(),
            root.with_file_name("MacOSX99.sdk"),
            root.join("..").join(ALIAS_NAME),
        ] {
            assert!(alias_snapshot(&root, bad.to_str().unwrap()).is_err());
        }
        let path = std::ffi::CString::new(alias.to_str().unwrap()).unwrap();
        assert_eq!(
            unsafe {
                libc::fchmodat(
                    libc::AT_FDCWD,
                    path.as_ptr(),
                    0o777,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            },
            0
        );
        assert!(alias_snapshot(&root, alias.to_str().unwrap()).is_err());
    }
    #[test]
    fn sdkroot_alias_replacement_and_parent_aliases_are_not_generic_permission() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let parent = d.path().canonicalize().unwrap();
        let root = parent.join(ROOT_NAME);
        std::fs::create_dir(&root).unwrap();
        let alias = parent.join(ALIAS_NAME);
        fixed_alias(&alias, ROOT_NAME);
        let fact = alias_snapshot(&root, alias.to_str().unwrap()).unwrap();
        std::fs::remove_file(&alias).unwrap();
        symlink(format!("./{ROOT_NAME}"), &alias).unwrap();
        assert!(alias_snapshot(&root, alias.to_str().unwrap()).is_err());
        std::fs::remove_file(&alias).unwrap();
        std::fs::create_dir(&alias).unwrap();
        assert!(alias_snapshot(&root, alias.to_str().unwrap()).is_err());
        assert_eq!(fact.link_text, ROOT_NAME);
        let p = parent.join("parent-alias");
        symlink(&parent, &p).unwrap();
        assert!(alias_snapshot(&p.join(ROOT_NAME), p.join(ALIAS_NAME).to_str().unwrap()).is_err());
    }
    #[test]
    fn actual_current_sdkroot_selected_rustc_and_sdk4_are_content_only() {
        let layout_path =
            std::env::var("OE_NATIVE_ACTUAL_SDK4_LAYOUT").expect("fixed actual SDK4 input");
        let layout: CodeNativeSdkLayout4 =
            serde_json::from_slice(&std::fs::read(layout_path).unwrap()).unwrap();
        let root = Path::new(&layout.canonical_root);
        let actual = root.with_file_name(ALIAS_NAME);
        let fact = alias_snapshot(root, actual.to_str().unwrap()).unwrap();
        assert_eq!(fact.link_text, ROOT_NAME);
        assert_eq!(actual.canonicalize().unwrap(), root);
        assert!(
            crate::native_profile::verify_file(&CodeNativeFileBinding {
                path: actual.display().to_string(),
                sha256: "0".repeat(64),
                bytes: 1,
                mode: 0o755,
            })
            .is_err()
        );
        // Alias support is confined to this new typed directory edge, never a
        // relaxation of the original generic regular-file guard.
    }
    #[test]
    fn actual_sdk4_binding_missing_original_and_mutated_receipts_reject() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("SKILL.md"), "finite SDK content check only").unwrap();
        std::fs::create_dir(d.path().join("var")).unwrap();
        let root = SkillRoot::new(d.path().canonicalize().unwrap()).unwrap();
        let bytes = std::fs::read(std::env::var("OE_NATIVE_ACTUAL_SDK4_LAYOUT").unwrap()).unwrap();
        std::fs::write(root.path().join("var/layout.json"), &bytes).unwrap();
        let source: serde_json::Value = serde_json::from_slice(&private_test_fixtures::bytes(
            "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-PURE-PREDICATE-CONTEXT-01.json",
        ))
        .unwrap();
        let mut recipe: CodeNativeBuildRecipe6 =
            serde_json::from_value(source["recipe"].clone()).unwrap();
        recipe.sdk_layout = ArtifactReference {
            path: "var/layout.json".into(),
            sha256: sha256(&bytes),
        };
        let f = CodeNativeFileBinding {
            path: "content-test-no-execution-tool".into(),
            sha256: COMPILER_SHA256.into(),
            bytes: 412_504,
            mode: 0o755,
        };
        let mut p = CodeNativeCargoLoaderPolicy6 {
            schema: "oev2.native-cargo-loader-policy/6".into(),
            rustc_wrapper: f.clone(),
            compiler: f.clone(),
            cargo: f,
            metadata_probe: "cargo_rustc_version_exact_vv/1".into(),
            fallback_tail: vec!["/usr/local/lib".into(), "/usr/lib".into()],
            child_environment_strategy: STRATEGY.into(),
        };
        let layout: CodeNativeSdkLayout4 = serde_json::from_slice(&bytes).unwrap();
        let raw = Path::new(&layout.canonical_root).with_file_name(ALIAS_NAME);
        let result = inspect_actual(&root, &recipe, &p, Some(raw.as_os_str()))
            .unwrap()
            .unwrap();
        assert_eq!(result.raw_value, raw.to_str().unwrap());
        assert_eq!(result.sdk_layout, recipe.sdk_layout);
        assert_eq!(result.canonical_root, layout.canonical_root);
        assert!(inspect_actual(&root, &recipe, &p, None).is_err());
        assert!(
            inspect_actual(&root, &recipe, &p, Some(OsStr::new(&layout.canonical_root))).is_err()
        );
        let mut wrong_recipe = recipe.clone();
        wrong_recipe.target = "x86_64-apple-darwin".into();
        assert!(inspect_actual(&root, &wrong_recipe, &p, Some(raw.as_os_str())).is_err());
        let mut wrong_mode = result.clone();
        wrong_mode.root_alias.link_mode = 0o777;
        assert_ne!(
            Some(wrong_mode),
            inspect_actual(&root, &recipe, &p, Some(raw.as_os_str())).unwrap()
        );
        let mut wrong_text = result.clone();
        wrong_text.root_alias.link_text = "./MacOSX.sdk".into();
        assert_ne!(
            Some(wrong_text),
            inspect_actual(&root, &recipe, &p, Some(raw.as_os_str())).unwrap()
        );
        p.child_environment_strategy =
            "record_cargo_fallback_then_remove_from_native_child/6".into();
        assert!(inspect_actual(&root, &recipe, &p, Some(raw.as_os_str())).is_err());
        assert_eq!(inspect_actual(&root, &recipe, &p, None).unwrap(), None);
        // Current SDK content acceptance never supplies a real compiler parent
        // or owner capability; all execution paths still construct those.
    }
}

// This selected SDKROOT domain describes a macOS SDK alias with original Unix
// mode and link bytes. Content policy remains inspectable, but an unsupported
// host cannot assert an actual SDK binding or an equivalent Windows permission.
#[cfg(not(unix))]
fn alias_snapshot(_canonical: &Path, _raw: &str) -> Result<CodeNativeSdkRootAlias6> {
    anyhow::bail!("loader6_platform_unsupported")
}
#[cfg(not(unix))]
pub(crate) fn inspect_actual(
    _root: &SkillRoot,
    _recipe: &CodeNativeBuildRecipe6,
    _policy: &CodeNativeCargoLoaderPolicy6,
    _value: Option<&OsStr>,
) -> Result<Option<CodeNativeCompilerSdkRootBinding6>> {
    anyhow::bail!("loader6_platform_unsupported")
}
#[cfg(not(unix))]
pub(crate) fn recheck_actual(
    _root: &SkillRoot,
    _recipe: &CodeNativeBuildRecipe6,
    _expected: &Option<CodeNativeCompilerSdkRootBinding6>,
) -> Result<()> {
    anyhow::bail!("loader6_platform_unsupported")
}
