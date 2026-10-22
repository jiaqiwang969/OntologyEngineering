//! Explicit SDK layout /3 content inspection. Only this known logical alias
//! graph permits leaf links; generic regular-file/profile /2 checks stay strict.
//! A valid layout does not grant SDK compilation, actor or owner authority.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};
const LIMIT: usize = 16 * 1024 * 1024;

fn read(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    reference
        .validate()
        .map_err(|e| anyhow::anyhow!("sdk_reference:{e:?}"))?;
    let path = root.private_path(Path::new(&reference.path))?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "sdk_layout_reference_changed"
    );
    Ok(strict_json(&bytes)?)
}
fn permission(metadata: &std::fs::Metadata) -> Result<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(u64::from(metadata.permissions().mode() & 0o7777))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        anyhow::bail!("sdk_alias_platform_uncovered")
    }
}
fn logical(root: &Path, role: &str) -> Result<PathBuf> {
    match role {
        "libSystem_tbd" => Ok(root.join("usr/lib/libSystem.tbd")),
        "libc_tbd" => Ok(root.join("usr/lib/libc.tbd")),
        "libm_tbd" => Ok(root.join("usr/lib/libm.tbd")),
        _ => anyhow::bail!("sdk_alias_role_unsupported"),
    }
}
fn check_layout(layout: &CodeNativeSdkLayout3) -> Result<()> {
    layout
        .validate()
        .map_err(|e| anyhow::anyhow!("sdk_layout:{e:?}"))?;
    ensure!(
        layout.schema == "oev2.native-sdk-layout/3",
        "sdk_layout_schema"
    );
    let root = Path::new(&layout.canonical_root);
    ensure!(root.is_absolute(), "sdk_root_not_absolute");
    no_symlinks(root)?;
    let rootmeta = std::fs::symlink_metadata(root)?;
    ensure!(
        rootmeta.is_dir() && !rootmeta.file_type().is_symlink(),
        "sdk_root_not_directory"
    );
    let canonical = std::fs::canonicalize(root)?;
    ensure!(
        canonical.to_str() == Some(layout.canonical_root.as_str()),
        "sdk_root_not_canonical"
    );
    ensure!(layout.files.len() == 2, "sdk_file_roles_incomplete");
    let mut file_roles = BTreeMap::new();
    for role in &layout.files {
        role.validate()
            .map_err(|e| anyhow::anyhow!("sdk_role_file:{e:?}"))?;
        ensure!(
            ["SDKSettings", "libSystem_tbd"].contains(&role.role.as_str())
                && file_roles.insert(role.role.as_str(), &role.file).is_none(),
            "sdk_file_role_duplicate_or_unknown"
        );
        crate::native_profile::verify_file(&role.file)?;
        ensure!(
            Path::new(&role.file.path).starts_with(root),
            "sdk_regular_target_outside_root"
        );
    }
    let settings = *file_roles
        .get("SDKSettings")
        .context("sdk_settings_missing")?;
    let system = *file_roles
        .get("libSystem_tbd")
        .context("sdk_system_missing")?;
    ensure!(
        settings.path
            == root
                .join("SDKSettings.json")
                .to_str()
                .context("sdk_path_encoding_uncovered")?,
        "sdk_settings_role_path"
    );
    let system_path = Path::new(&system.path);
    ensure!(
        system_path.parent() == Some(root.join("usr/lib").as_path())
            && std::fs::canonicalize(system_path)?.to_str() == Some(system.path.as_str()),
        "sdk_system_regular_target_path"
    );
    let logical_system = logical(root, "libSystem_tbd")?;
    no_symlinks(logical_system.parent().context("sdk_system_parent")?)?;
    let system_logical_meta = std::fs::symlink_metadata(&logical_system)?;
    let system_alias = system_logical_meta.file_type().is_symlink();
    if !system_alias {
        ensure!(
            system_logical_meta.is_file() && logical_system.to_str() == Some(system.path.as_str()),
            "sdk_system_role_not_fixed_regular"
        );
    }
    let roles = if system_alias {
        vec!["libc_tbd", "libm_tbd", "libSystem_tbd"]
    } else {
        vec!["libc_tbd", "libm_tbd"]
    };
    ensure!(
        layout.aliases.len() == roles.len(),
        "sdk_alias_roles_incomplete"
    );
    let mut aliases = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for alias in &layout.aliases {
        alias
            .validate()
            .map_err(|e| anyhow::anyhow!("sdk_alias:{e:?}"))?;
        ensure!(
            roles.contains(&alias.role.as_str()) && seen.insert(alias.role.as_str()),
            "sdk_alias_role_duplicate_or_unknown"
        );
        let wanted = logical(root, &alias.role)?;
        ensure!(
            wanted.to_str() == Some(alias.link_path.as_str()),
            "sdk_alias_logical_path"
        );
        no_symlinks(wanted.parent().context("sdk_alias_parent")?)?;
        let before = std::fs::symlink_metadata(&wanted)?;
        ensure!(
            before.file_type().is_symlink(),
            "sdk_alias_not_leaf_symlink"
        );
        let link = std::fs::read_link(&wanted)?;
        let text = link.to_str().context("sdk_link_encoding_uncovered")?;
        ensure!(
            text == alias.link_text
                && sha256(text.as_bytes()) == alias.link_text_sha256
                && permission(&before)? == alias.link_mode,
            "sdk_alias_raw_identity_changed"
        );
        ensure!(
            !link.is_absolute()
                && !alias.link_text.is_empty()
                && link.components().all(|c| matches!(c, Component::Normal(_))),
            "sdk_link_domain_unsupported"
        );
        ensure!(alias.target == *system, "sdk_alias_target_not_fixed_system");
        crate::native_profile::verify_file(&alias.target)?;
        let after = std::fs::symlink_metadata(&wanted)?;
        ensure!(
            after.file_type().is_symlink()
                && permission(&after)? == alias.link_mode
                && std::fs::read_link(&wanted)? == link,
            "sdk_alias_changed_during_read"
        );
        aliases.insert(wanted, alias);
    }
    // Resolve only these two/three declared logical edges. canonicalize alone
    // would silently follow undeclared intermediate links and lose provenance.
    for start in aliases.keys() {
        let mut at = start.clone();
        let mut visited = BTreeSet::new();
        for _ in 0..=aliases.len() {
            if at == system_path {
                break;
            }
            ensure!(visited.insert(at.clone()), "sdk_alias_cycle");
            let edge = aliases
                .get(&at)
                .context("sdk_alias_unrecorded_intermediate")?;
            at = at
                .parent()
                .context("sdk_link_parent")?
                .join(&edge.link_text);
            ensure!(at.starts_with(root), "sdk_alias_escapes_root");
        }
        ensure!(at == system_path, "sdk_alias_resolution_not_fixed_system");
        ensure!(
            std::fs::canonicalize(start)?.to_str() == Some(system.path.as_str()),
            "sdk_alias_canonical_target_changed"
        );
    }
    // Recheck root and complete regular targets; no size/mtime cache shortcut.
    no_symlinks(root)?;
    ensure!(
        std::fs::symlink_metadata(root)?.is_dir()
            && std::fs::canonicalize(root)?.to_str() == Some(layout.canonical_root.as_str()),
        "sdk_root_changed"
    );
    for file in &layout.files {
        crate::native_profile::verify_file(&file.file)?;
    }
    Ok(())
}
/// Load and twice verify exact SDK graph CONTENT. No native tool is run, no
/// availability/provenance/adoption permission is inferred from serialized data.
pub fn inspect_sdk(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<CodeNativeSdkLayout3> {
    let layout: CodeNativeSdkLayout3 = serde_json::from_value(read(root, reference)?)?;
    check_layout(&layout)?;
    check_layout(&layout)?;
    ensure!(
        read(root, reference)? == serde_json::to_value(&layout)?,
        "sdk_layout_changed_during_inspection"
    );
    Ok(layout)
}
pub struct NativeSdkInspector<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeSdkInspectPort for NativeSdkInspector<'_> {
    fn inspect_sdk(
        &self,
        input: &ArtifactReference,
    ) -> std::result::Result<CodeNativeSdkLayout3, FailureRecord> {
        inspect_sdk(self.root,input).map_err(|e|FailureRecord{code:ErrorCode::InvalidContract,phase:"native_sdk_layout_content_inspection".into(),retryable:false,partial_result:Some(json!({"inspection":"rejected","reason":format!("{e:#}"),"execution":"not_run","compilation_origin":"unknown","sdk_closure":"unknown","engineering_adoption":"not_established"}))})
    }
}
