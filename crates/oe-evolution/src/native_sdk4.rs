//! Explicit SDK /4 selected-library CONTENT, through the existing SDK owner.
//! Four real regular files and six exact leaf aliases are inspected. Neither
//! serialized data nor these checks attest native loading or owner permission.
//! The original SDK /3 and native /3 argument guards retain their own domains.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const LIMIT: usize = 16 * 1024 * 1024;
const FILES: [(&str, &str); 4] = [
    ("SDKSettings", "SDKSettings.json"),
    ("libSystem_tbd", "usr/lib/libSystem.B.tbd"),
    ("libiconv_tbd", "usr/lib/libiconv.2.tbd"),
    ("libcharset_tbd", "usr/lib/libcharset.1.tbd"),
];
// (logical role, fixed filename, exact raw edge, terminal regular-file role).
const ALIASES: [(&str, &str, &str, &str); 6] = [
    (
        "libSystem_tbd",
        "libSystem.tbd",
        "libSystem.B.tbd",
        "libSystem_tbd",
    ),
    ("libc_tbd", "libc.tbd", "libSystem.tbd", "libSystem_tbd"),
    ("libm_tbd", "libm.tbd", "libSystem.tbd", "libSystem_tbd"),
    (
        "libproc_tbd",
        "libproc.tbd",
        "libSystem.tbd",
        "libSystem_tbd",
    ),
    (
        "libiconv_tbd",
        "libiconv.tbd",
        "libiconv.2.tbd",
        "libiconv_tbd",
    ),
    (
        "libcharset_tbd",
        "libcharset.tbd",
        "libcharset.1.tbd",
        "libcharset_tbd",
    ),
];

fn read(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    reference
        .validate()
        .map_err(|e| anyhow::anyhow!("sdk4_reference:{e:?}"))?;
    let bytes = read_regular_bytes(&root.private_path(Path::new(&reference.path))?, LIMIT)?;
    ensure!(sha256(&bytes) == reference.sha256, "sdk4_reference_changed");
    Ok(strict_json(&bytes)?)
}
fn mode(metadata: &std::fs::Metadata) -> Result<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(u64::from(metadata.permissions().mode() & 0o7777))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        anyhow::bail!("sdk4_alias_platform_uncovered")
    }
}
fn check_root(root: &Path, declared: &str) -> Result<()> {
    ensure!(root.is_absolute(), "sdk4_root_not_absolute");
    no_symlinks(root)?;
    let meta = std::fs::symlink_metadata(root)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "sdk4_root_not_directory"
    );
    ensure!(
        std::fs::canonicalize(root)?.to_str() == Some(declared),
        "sdk4_root_not_canonical"
    );
    Ok(())
}
fn check_layout4(layout: &CodeNativeSdkLayout4) -> Result<()> {
    layout
        .validate()
        .map_err(|e| anyhow::anyhow!("sdk4_layout:{e:?}"))?;
    ensure!(layout.schema == "oev2.native-sdk-layout/4", "sdk4_schema");
    let root = Path::new(&layout.canonical_root);
    check_root(root, &layout.canonical_root)?;
    ensure!(layout.files.len() == FILES.len(), "sdk4_files_incomplete");
    let mut files = BTreeMap::new();
    for item in &layout.files {
        item.validate()
            .map_err(|e| anyhow::anyhow!("sdk4_file:{e:?}"))?;
        let (_, relative) = FILES
            .iter()
            .find(|(role, _)| *role == item.role)
            .context("sdk4_file_role_unknown")?;
        ensure!(
            files.insert(item.role.as_str(), &item.file).is_none(),
            "sdk4_file_role_duplicate"
        );
        ensure!(
            root.join(relative).to_str() == Some(item.file.path.as_str()),
            "sdk4_file_role_path"
        );
        crate::native_profile::verify_file(&item.file)?;
        ensure!(
            std::fs::canonicalize(&item.file.path)?.to_str() == Some(item.file.path.as_str()),
            "sdk4_file_not_canonical"
        );
    }
    ensure!(
        layout.aliases.len() == ALIASES.len(),
        "sdk4_aliases_incomplete"
    );
    let mut aliases = BTreeMap::new();
    for item in &layout.aliases {
        item.validate()
            .map_err(|e| anyhow::anyhow!("sdk4_alias:{e:?}"))?;
        let (_, filename, edge, target_role) = ALIASES
            .iter()
            .find(|(role, _, _, _)| *role == item.role)
            .context("sdk4_alias_role_unknown")?;
        let wanted = root.join("usr/lib").join(filename);
        ensure!(
            wanted.to_str() == Some(item.link_path.as_str()),
            "sdk4_alias_logical_path"
        );
        ensure!(
            item.link_text == *edge,
            "sdk4_alias_edge_outside_fixed_domain"
        );
        ensure!(
            aliases.insert(wanted.clone(), item).is_none(),
            "sdk4_alias_role_duplicate"
        );
        no_symlinks(wanted.parent().context("sdk4_alias_parent")?)?;
        let before = std::fs::symlink_metadata(&wanted)?;
        ensure!(
            before.file_type().is_symlink(),
            "sdk4_alias_not_leaf_symlink"
        );
        let link = std::fs::read_link(&wanted)?;
        let text = link.to_str().context("sdk4_link_encoding_uncovered")?;
        ensure!(
            text == item.link_text
                && sha256(text.as_bytes()) == item.link_text_sha256
                && mode(&before)? == item.link_mode,
            "sdk4_alias_raw_identity_changed"
        );
        ensure!(
            Some(&item.target) == files.get(target_role).copied(),
            "sdk4_alias_target_role_mismatch"
        );
        crate::native_profile::verify_file(&item.target)?;
        let after = std::fs::symlink_metadata(&wanted)?;
        ensure!(
            after.file_type().is_symlink()
                && mode(&after)? == item.link_mode
                && std::fs::read_link(&wanted)? == link,
            "sdk4_alias_changed_during_read"
        );
    }
    // Follow only the declared fixed graph, retaining every intermediate edge.
    for (start, item) in &aliases {
        let terminal = Path::new(&item.target.path);
        let mut at = start.clone();
        let mut visited = std::collections::BTreeSet::new();
        for _ in 0..=ALIASES.len() {
            if at == terminal {
                break;
            }
            ensure!(visited.insert(at.clone()), "sdk4_alias_cycle");
            let edge = aliases
                .get(&at)
                .context("sdk4_alias_unrecorded_intermediate")?;
            at = at
                .parent()
                .context("sdk4_link_parent")?
                .join(&edge.link_text);
            ensure!(at.starts_with(root), "sdk4_alias_escapes_root");
        }
        ensure!(
            at == terminal
                && std::fs::canonicalize(start)?.to_str() == Some(item.target.path.as_str()),
            "sdk4_alias_resolution_changed"
        );
    }
    check_root(root, &layout.canonical_root)?;
    for file in &layout.files {
        crate::native_profile::verify_file(&file.file)?;
    }
    Ok(())
}

/// Twice verify exact SDK /4 data and current original file/link bytes. Readonly;
/// no compiler, model, process, registry, build dispatch or adoption is invoked.
pub fn inspect_sdk4(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<CodeNativeSdkLayout4> {
    let layout: CodeNativeSdkLayout4 = serde_json::from_value(read(root, reference)?)?;
    check_layout4(&layout)?;
    check_layout4(&layout)?;
    ensure!(
        read(root, reference)? == serde_json::to_value(&layout)?,
        "sdk4_reference_changed_during_inspection"
    );
    Ok(layout)
}

fn string_list(value: &Value, label: &str) -> Result<Vec<String>> {
    value
        .as_array()
        .with_context(|| format!("sdk4_tbd_{label}_not_array"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .with_context(|| format!("sdk4_tbd_{label}_not_string"))
        })
        .collect()
}
fn parse_tbd_documents4(
    role: &str,
    source: &CodeNativeFileBinding,
    bytes: &[u8],
) -> Result<Vec<CodeNativeSdkTbdDocument4>> {
    ensure!(
        sha256(bytes) == source.sha256 && bytes.len() as u64 == source.bytes,
        "sdk4_tbd_bytes_changed"
    );
    let text = std::str::from_utf8(bytes).context("sdk4_tbd_not_utf8")?;
    let mut offsets = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches('\n') == "--- !tapi-tbd" {
            offsets.push(offset);
        }
        offset += line.len();
    }
    ensure!(offsets.first() == Some(&0), "sdk4_tbd_exact_header_missing");
    offsets.push(bytes.len());
    let mut documents = Vec::new();
    for (ordinal, range) in offsets.windows(2).enumerate() {
        let raw = &bytes[range[0]..range[1]];
        let yaml: serde_yaml::Value = serde_yaml::from_slice(raw)?;
        let serde_yaml::Value::Tagged(tagged) = yaml else {
            anyhow::bail!("sdk4_tbd_tag_missing")
        };
        ensure!(
            tagged.tag.to_string() == "!tapi-tbd",
            "sdk4_tbd_tag_unsupported"
        );
        let content = serde_json::to_value(tagged.value)?;
        ensure!(content.is_object(), "sdk4_tbd_document_not_object");
        let version = content["tbd-version"]
            .as_u64()
            .context("sdk4_tbd_version_not_integer")?;
        ensure!(version == 4, "sdk4_tbd_version_unsupported");
        let targets = string_list(&content["targets"], "targets")?;
        ensure!(!targets.is_empty(), "sdk4_tbd_targets_empty");
        let install_name = content["install-name"]
            .as_str()
            .context("sdk4_tbd_install_name_not_string")?
            .to_owned();
        for section in ["exports", "reexported-libraries"] {
            if let Some(groups) = content.get(section) {
                for group in groups.as_array().context("sdk4_tbd_groups_not_array")? {
                    ensure!(group.is_object(), "sdk4_tbd_group_not_object");
                    ensure!(
                        !string_list(&group["targets"], "group_targets")?.is_empty(),
                        "sdk4_tbd_group_targets_empty"
                    );
                    for (name, values) in group.as_object().context("sdk4_tbd_group_not_object")? {
                        string_list(values, name)?;
                    }
                }
            }
        }
        let item = CodeNativeSdkTbdDocument4 {
            file_role: role.into(),
            source: source.clone(),
            document_ordinal: ordinal as u64,
            byte_start: range[0] as u64,
            byte_end: range[1] as u64,
            raw_document_sha256: sha256(raw),
            tbd_version: version,
            targets,
            install_name,
            content,
        };
        item.validate()
            .map_err(|e| anyhow::anyhow!("sdk4_tbd_document:{e:?}"))?;
        documents.push(item);
    }
    Ok(documents)
}

/// Derive typed complete original TAPI4 documents. Exact full file pins and
/// contiguous raw byte ranges accompany the parsed content. No architecture
/// rewriting or ABI/loader/SDK-closure attestation occurs here.
pub fn inspect_tbd_metadata4(
    layout: &CodeNativeSdkLayout4,
) -> Result<Vec<CodeNativeSdkTbdDocument4>> {
    check_layout4(layout)?;
    let mut result = Vec::new();
    for (role, wanted_install) in [
        ("libSystem_tbd", "/usr/lib/libSystem.B.dylib"),
        ("libiconv_tbd", "/usr/lib/libiconv.2.dylib"),
        ("libcharset_tbd", "/usr/lib/libcharset.1.dylib"),
    ] {
        let file = &layout
            .files
            .iter()
            .find(|f| f.role == role)
            .context("sdk4_tbd_file_missing")?
            .file;
        let bytes = read_regular_bytes(Path::new(&file.path), LIMIT)?;
        let documents = parse_tbd_documents4(role, file, &bytes)?;
        ensure!(
            documents
                .first()
                .is_some_and(|d| d.install_name == wanted_install),
            "sdk4_tbd_root_install_name"
        );
        crate::native_profile::verify_file(file)?;
        ensure!(
            read_regular_bytes(Path::new(&file.path), LIMIT)? == bytes,
            "sdk4_tbd_changed_during_parse"
        );
        result.extend(documents);
    }
    let iconv = result
        .iter()
        .find(|d| d.file_role == "libiconv_tbd" && d.document_ordinal == 0)
        .context("sdk4_tbd_iconv_root_missing")?;
    let groups = iconv.content["reexported-libraries"]
        .as_array()
        .context("sdk4_tbd_iconv_reexport_missing")?;
    ensure!(
        groups
            .iter()
            .any(|g| g["libraries"].as_array().is_some_and(|values| values
                .iter()
                .any(|v| v.as_str() == Some("/usr/lib/libcharset.1.dylib")))),
        "sdk4_tbd_charset_dependency_missing"
    );
    check_layout4(layout)?;
    Ok(result)
}

/// Bind exact library occurrences in the actual expanded argv, in original
/// order with repeats. Other argv semantics and directory admission belong to
/// the existing native executor. This content result grants no such permission.
pub fn bind_selected_libraries4(
    layout: &CodeNativeSdkLayout4,
    arguments: &[String],
) -> Result<Vec<CodeNativeSdkSelectedLibrary4>> {
    let _metadata = inspect_tbd_metadata4(layout)?;
    let mut selected = Vec::new();
    let mut i = 0;
    while i < arguments.len() {
        let arg = &arguments[i];
        if ["-o", "-arch", "-isysroot", "-L"].contains(&arg.as_str()) {
            ensure!(
                arguments.get(i + 1).is_some(),
                "sdk4_argument_operand_missing"
            );
            i += 2;
            continue;
        }
        if arg == "-Xlinker" {
            let operand = arguments
                .get(i + 1)
                .context("sdk4_argument_operand_missing")?;
            ensure!(
                !operand.starts_with("-l"),
                "sdk4_forwarded_library_syntax_uncovered"
            );
            i += 2;
            continue;
        }
        if arg.starts_with("-Wl,") {
            ensure!(
                !arg.split(',').skip(1).any(|p| p.starts_with("-l")),
                "sdk4_forwarded_library_syntax_uncovered"
            );
        }
        if arg.starts_with("-l") {
            let role = match arg.as_str() {
                "-lSystem" => "libSystem_tbd",
                "-lc" => "libc_tbd",
                "-lm" => "libm_tbd",
                "-lproc" => "libproc_tbd",
                "-liconv" => "libiconv_tbd",
                "-lcharset" => "libcharset_tbd",
                _ => anyhow::bail!("sdk4_library_argument_unselected"),
            };
            let alias = layout
                .aliases
                .iter()
                .find(|a| a.role == role)
                .context("sdk4_selected_alias_missing")?;
            let mut chain = vec![role.to_string()];
            if ["libc_tbd", "libm_tbd", "libproc_tbd"].contains(&role) {
                chain.push("libSystem_tbd".into());
            }
            let dependencies = if role == "libiconv_tbd" {
                vec![
                    layout
                        .files
                        .iter()
                        .find(|f| f.role == "libcharset_tbd")
                        .context("sdk4_charset_file_missing")?
                        .file
                        .clone(),
                ]
            } else {
                Vec::new()
            };
            let item = CodeNativeSdkSelectedLibrary4 {
                argument_ordinal: i as u64,
                argument: arg.clone(),
                role: role.into(),
                logical_path: alias.link_path.clone(),
                target: alias.target.clone(),
                alias_roles: chain,
                tbd_dependencies: dependencies,
            };
            item.validate()
                .map_err(|e| anyhow::anyhow!("sdk4_selected_library:{e:?}"))?;
            selected.push(item);
        }
        i += 1;
    }
    check_layout4(layout)?;
    Ok(selected)
}

/// Reject shadows of any selected SDK4 library in caller-provided directories.
/// The native owner must separately admit these directories and recheck around
/// actual dispatch; this function provides no path, execution or closure grant.
pub fn check_library_search_paths4(directories: &[PathBuf]) -> Result<()> {
    for directory in directories {
        let declared = directory
            .to_str()
            .context("sdk4_search_path_encoding_uncovered")?;
        check_root(directory, declared)?;
        for stem in ["System", "c", "m", "proc", "iconv", "charset"] {
            for suffix in ["a", "dylib", "tbd"] {
                match std::fs::symlink_metadata(directory.join(format!("lib{stem}.{suffix}"))) {
                    Ok(_) => anyhow::bail!("sdk4_selected_library_shadow_unsupported"),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
    Ok(())
}

pub struct NativeSdkInspector4<'a> {
    pub root: &'a SkillRoot,
}
impl CodeNativeSdkInspect4Port for NativeSdkInspector4<'_> {
    fn inspect_sdk4(
        &self,
        input: &ArtifactReference,
    ) -> std::result::Result<CodeNativeSdkLayout4, FailureRecord> {
        inspect_sdk4(self.root, input).map_err(|e| FailureRecord { code: ErrorCode::InvalidContract,
            phase: "native_sdk4_selected_library_content_inspection".into(), retryable: false,
            partial_result: Some(json!({"inspection":"rejected","reason":format!("{e:#}"),"execution":"not_run",
                "compilation_origin":"unknown","sdk_closure":"unknown","actual_runtime_load":"not_observed",
                "engineering_adoption":"not_established"})) })
    }
}
