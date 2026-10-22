//! Actual Cargo source-root inventory for explicit stage-context/4.
//! Stage/3 and public pure input/3 keep their original root-only domain.
use crate::native_runtime::{deadline_timeout, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, write_new};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const SCHEMA: &str = "oev2.native-compilation-sources/4";
const LIMIT: usize = 16 * 1024 * 1024;
const CRATE_LIMIT: usize = 64 * 1024 * 1024;
const CRATE_EXPANDED_LIMIT: u64 = 256 * 1024 * 1024;

fn original_package_files(
    dir: &Path,
    archive_path: &Path,
    locked_checksum: &str,
) -> Result<(
    CodeNativeFileBinding,
    BTreeSet<String>,
    Vec<CodeNativeCompilationArchiveMetadata4>,
)> {
    let binding = crate::native_archive::bind_file(archive_path)?;
    ensure!(
        binding.sha256 == locked_checksum,
        "native4_original_crate_lock_checksum_changed"
    );
    let raw = read_regular_bytes(archive_path, CRATE_LIMIT)?;
    ensure!(
        sha256(&raw) == binding.sha256,
        "native4_original_crate_changed_before_read"
    );
    let decoder = flate2::bufread::GzDecoder::new(Cursor::new(raw));
    let mut archive = tar::Archive::new(decoder.take(CRATE_EXPANDED_LIMIT + 1));
    let prefix = dir.file_name().context("native4_crate_package_prefix")?;
    let mut packed = BTreeSet::new();
    let mut metadata = Vec::new();
    let mut pending_long: Option<Vec<u8>> = None;
    let mut ordinal = 0u64;
    for entry in archive.entries()?.raw(true) {
        let mut entry = entry?;
        let member_ordinal = ordinal;
        ordinal = ordinal
            .checked_add(1)
            .context("native4_original_crate_ordinal_overflow")?;
        ensure!(ordinal <= 100_000, "native4_original_crate_member_limit");
        if entry.header().entry_type().is_gnu_longname() {
            ensure!(
                pending_long.is_none()
                    && entry.path_bytes().as_ref() == b"././@LongLink"
                    && entry.header().link_name()?.is_none(),
                "native4_original_crate_longname_header_or_sequence"
            );
            let size = entry.header().size()?;
            ensure!(
                (102..=4096).contains(&size),
                "native4_original_crate_longname_size"
            );
            let header_sha256 = sha256(entry.header().as_bytes());
            let header_offset = entry.raw_header_position();
            let mut original = Vec::new();
            entry.read_to_end(&mut original)?;
            ensure!(
                original.len() as u64 == size
                    && original.last() == Some(&0)
                    && original[..original.len() - 1]
                        .iter()
                        .all(|b| *b != 0 && b.is_ascii()),
                "native4_original_crate_longname_payload"
            );
            let path = std::str::from_utf8(&original[..original.len() - 1])?.to_owned();
            metadata.push(CodeNativeCompilationArchiveMetadata4 {
                member_ordinal,
                header_offset,
                kind: "gnu_longname_single_regular_next".into(),
                header_sha256,
                payload_sha256: sha256(&original),
                payload_bytes: size,
                payload_path: path,
            });
            original.pop();
            pending_long = Some(original);
            continue;
        }
        ensure!(
            entry.header().entry_type().is_file(),
            "native4_original_crate_member_type_uncovered"
        );
        ensure!(
            entry.header().link_name()?.is_none(),
            "native4_original_crate_link_uncovered"
        );
        let original = if let Some(long) = pending_long.take() {
            ensure!(
                entry.path_bytes().as_ref() == &long[..100],
                "native4_original_crate_longname_next_regular_prefix_changed"
            );
            long
        } else {
            entry.path_bytes().into_owned()
        };
        let text = std::str::from_utf8(&original)?;
        let path = Path::new(text);
        ensure!(
            path.components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
                && !text.contains('\\'),
            "native4_original_crate_path_escape"
        );
        ensure!(
            path.components()
                .map(|c| c.as_os_str().to_str().unwrap())
                .collect::<Vec<_>>()
                .join("/")
                == text,
            "native4_original_crate_noncanonical_path"
        );
        let mut components = path.components();
        ensure!(
            components.next().is_some_and(|c| c.as_os_str() == prefix),
            "native4_original_crate_package_prefix_changed"
        );
        let relative = components.as_path();
        ensure!(
            !relative.as_os_str().is_empty() && !relative.starts_with(".cargo-ok"),
            "native4_original_crate_reserved_metadata_path"
        );
        let relative_text = relative.to_str().context("native4_utf8")?.to_owned();
        ensure!(
            packed.insert(relative_text.clone()) && packed.len() <= 100_000,
            "native4_original_crate_duplicate_or_member_limit"
        );
        ensure!(
            entry.header().size()? <= CRATE_LIMIT as u64,
            "native4_original_crate_member_limit"
        );
        let mut payload = Vec::new();
        entry.read_to_end(&mut payload)?;
        let file = dir.join(relative);
        let original_binding = crate::native_archive::bind_file(&file)
            .with_context(|| format!("native4_original_crate_source_missing:{relative_text}"))?;
        ensure!(
            read_regular_bytes(&file, CRATE_LIMIT)? == payload
                && sha256(&payload) == original_binding.sha256,
            "native4_original_crate_source_payload_changed:{relative_text}"
        );
        ensure!(
            original_binding == crate::native_archive::bind_file(&file)?,
            "native4_original_crate_source_changed_after_read"
        );
    }
    ensure!(
        pending_long.is_none(),
        "native4_original_crate_dangling_longname"
    );
    // Finish gzip validation and inspect all remaining tar padding. No second
    // stream, trailing opaque data, symlink, unsupported extension or duplicate is silently
    // consumed as source payload in this explicit finite crate format.
    let mut limited = archive.into_inner();
    let mut padding = [0u8; 8192];
    loop {
        let n = limited.read(&mut padding)?;
        if n == 0 {
            break;
        }
        ensure!(
            padding[..n].iter().all(|b| *b == 0),
            "native4_original_crate_trailing_tar_data"
        );
    }
    ensure!(
        limited.limit() != 0 && !packed.is_empty(),
        "native4_original_crate_expanded_limit_or_empty"
    );
    let decoder = limited.into_inner();
    let cursor = decoder.into_inner();
    ensure!(
        cursor.position() == cursor.get_ref().len() as u64,
        "native4_original_crate_trailing_gzip_data"
    );
    ensure!(
        binding == crate::native_archive::bind_file(archive_path)?,
        "native4_original_crate_changed_after_read"
    );
    Ok((binding, packed, metadata))
}
fn bytes(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    let b = read_regular_bytes(&root.private_path(Path::new(&r.path))?, LIMIT)?;
    ensure!(sha256(&b) == r.sha256, "native4_source_document_changed");
    Ok(b)
}
fn inventory_files(root: &Path) -> Result<Vec<CodeNativeFileBinding>> {
    no_symlinks(root)?;
    let mut pending = vec![root.to_owned()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            let m = std::fs::symlink_metadata(&p)?;
            ensure!(
                !m.file_type().is_symlink(),
                "native4_source_symlink_uncovered"
            );
            if m.is_dir() {
                pending.push(p);
            } else {
                ensure!(m.is_file(), "native4_source_not_regular");
                files.push(crate::native_archive::bind_file(&p)?);
            }
            ensure!(
                files.len() + pending.len() <= 100_000,
                "native4_source_inventory_limit"
            );
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}
fn package_name(stage: &str) -> Result<&'static str> {
    match stage {
        "build-normalizer" => Ok("oe-design"),
        "build-runner" => Ok("oe-code-runner"),
        _ => anyhow::bail!("native4_stage"),
    }
}
fn compiler_host(stdout: &[u8]) -> Result<String> {
    let hosts = std::str::from_utf8(stdout)?
        .lines()
        .filter_map(|s| s.strip_prefix("host: "))
        .collect::<Vec<_>>();
    ensure!(
        hosts.len() == 1
            && !hosts[0].is_empty()
            && hosts[0]
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "native4_compiler_host"
    );
    Ok(hosts[0].into())
}
fn selection(stage: &str, host: &str, build: &CodeBuildRequest) -> Result<(String, String)> {
    // These are the original shared builder's two selectors. The normalizer
    // has no --target; the runner has --target request.platform. This finite
    // implementation requires the same actual host/target for source-root
    // inventory. Other cross-platform compilation domains remain uncovered.
    ensure!(
        host == build.platform,
        "native4_cross_host_target_source_inventory_uncovered"
    );
    match stage {
        "build-normalizer" => Ok((host.into(), "host_no_explicit_target".into())),
        "build-runner" => Ok((build.platform.clone(), "explicit_target".into())),
        _ => anyhow::bail!("native4_stage"),
    }
}
fn package_label(p: &Value) -> Result<String> {
    let mut label = format!(
        "{} v{}",
        p["name"].as_str().context("native4_name")?,
        p["version"].as_str().context("native4_version")?
    );
    if p["source"].is_null() {
        let manifest = Path::new(p["manifest_path"].as_str().context("native4_manifest")?);
        label.push_str(&format!(
            " ({})",
            manifest
                .parent()
                .context("native4_package_root")?
                .to_str()
                .context("native4_utf8")?
        ));
    }
    if p["targets"]
        .as_array()
        .context("native4_targets")?
        .iter()
        .any(|t| {
            t["kind"]
                .as_array()
                .is_some_and(|k| k.iter().any(|x| x == "proc-macro"))
        })
    {
        label.push_str(" (proc-macro)");
    }
    Ok(label)
}
fn selected_packages<'a>(
    metadata: &'a Value,
    stage: &str,
    tree: &[u8],
) -> Result<BTreeMap<&'a str, BTreeSet<String>>> {
    let mut labels: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for p in metadata["packages"]
        .as_array()
        .context("native4_packages")?
    {
        let id = p["id"].as_str().context("native4_id")?;
        ensure!(ids.insert(id), "native4_duplicate_id");
        labels.entry(package_label(p)?).or_default().push(id);
    }
    let nodes = metadata["resolve"]["nodes"]
        .as_array()
        .context("native4_resolve")?;
    let mut node_ids = BTreeSet::new();
    for n in nodes {
        ensure!(
            node_ids.insert(n["id"].as_str().context("native4_id")?),
            "native4_duplicate_node"
        );
    }
    let wanted = package_name(stage)?;
    let starts = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == wanted && p["source"].is_null())
        .collect::<Vec<_>>();
    ensure!(starts.len() == 1, "native4_root_package_ambiguous");
    let start_id = starts[0]["id"].as_str().unwrap();
    let mut selected: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut first = true;
    for line in std::str::from_utf8(tree)?.lines() {
        let (label, features) = line.split_once('\t').context("native4_tree_record")?;
        ensure!(!features.contains('\t'), "native4_tree_feature_record");
        let matches = labels
            .get(label)
            .context("native4_tree_package_not_in_metadata")?;
        ensure!(matches.len() == 1, "native4_tree_package_source_ambiguous");
        let id = matches[0];
        ensure!(node_ids.contains(id), "native4_tree_resolve_node_missing");
        if first {
            ensure!(id == start_id, "native4_tree_wrong_stage_root");
            first = false;
        }
        let fields = selected.entry(id).or_default();
        if !features.is_empty() {
            let values = features.split(',').collect::<Vec<_>>();
            ensure!(
                values.iter().all(|v| !v.is_empty()
                    && v.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_+-./".contains(&c)))
                    && values.iter().copied().collect::<BTreeSet<_>>().len() == values.len(),
                "native4_tree_feature_value"
            );
            fields.extend(values.into_iter().map(str::to_owned));
        }
        ensure!(selected.len() <= 4096, "native4_package_limit");
    }
    ensure!(!first, "native4_tree_empty");
    // Cargo tree with the original package, normal/build edges, default
    // feature selection and actual target is the selected parsing upper
    // bound. It is not a claim that every row was compiled or linked.
    Ok(selected)
}
fn derive_roots(
    root: &SkillRoot,
    build: &CodeBuildRequest,
    stage: &str,
    metadata: &Value,
    tree: &[u8],
) -> Result<Vec<CodeNativeCompilationSourceRoot4>> {
    ensure!(
        metadata["workspace_root"] == root.path().to_str().context("native4_utf8")?
            && metadata["version"] == 1,
        "native4_metadata_workspace_or_version"
    );
    let packages = metadata["packages"]
        .as_array()
        .context("native4_packages")?;
    let mut ps = BTreeMap::new();
    for p in packages {
        let id = p["id"].as_str().context("native4_id")?;
        ensure!(ps.insert(id, p).is_none(), "native4_duplicate_id");
    }
    let selected = selected_packages(metadata, stage, tree)?;
    let lock_raw = read_regular_bytes(&root.path().join("Cargo.lock"), LIMIT)?;
    let lock: toml::Value = toml::from_str(std::str::from_utf8(&lock_raw)?)?;
    let lock_packages = lock["package"]
        .as_array()
        .context("native4_lock_packages")?;
    let cache = Path::new(&build.cargo_home).join("registry/src");
    let verify = |id: &str,
                  enabled_features: BTreeSet<String>|
     -> Result<CodeNativeCompilationSourceRoot4> {
        let p = ps.get(id).context("native4_package_missing")?;
        let name = p["name"].as_str().context("native4_name")?;
        let version = p["version"].as_str().context("native4_version")?;
        let manifest = PathBuf::from(p["manifest_path"].as_str().context("native4_manifest")?);
        no_symlinks(&manifest)?;
        ensure!(
            manifest.is_absolute() && manifest.file_name().is_some_and(|x| x == "Cargo.toml"),
            "native4_manifest_path"
        );
        let dir = manifest.parent().context("native4_package_root")?;
        let kind;
        let source_format;
        let mut package_archive = None;
        let mut archive_metadata = Vec::new();
        let mut cargo_cache_metadata = Vec::new();
        let mut packed_files = None;
        if p["source"].is_null() {
            ensure!(
                dir.starts_with(root.path().join("crates"))
                    || dir.starts_with(root.path().join("vendor")),
                "native4_local_source_outside_declared_root"
            );
            kind = "workspace_or_local_path";
            source_format = "source_bound_local_file_inventory";
        } else {
            let source = p["source"].as_str().context("native4_registry_source")?;
            ensure!(
                source == "registry+https://github.com/rust-lang/crates.io-index",
                "native4_registry_source_uncovered"
            );
            let relative = dir
                .strip_prefix(&cache)
                .context("native4_registry_outside_selected_cargo_home")?;
            ensure!(
                relative.components().count() == 2
                    && dir.file_name().and_then(|n| n.to_str())
                        == Some(format!("{name}-{version}").as_str()),
                "native4_registry_layout"
            );
            let matches = lock_packages
                .iter()
                .filter(|x| {
                    x["name"].as_str() == Some(name)
                        && x["version"].as_str() == Some(version)
                        && x.get("source").and_then(toml::Value::as_str) == Some(source)
                })
                .collect::<Vec<_>>();
            ensure!(matches.len() == 1, "native4_registry_lock_coordinate");
            if dir.join(".cargo-checksum.json").exists() {
                let checks: Value = strict_json(&read_regular_bytes(
                    &dir.join(".cargo-checksum.json"),
                    LIMIT,
                )?)?;
                ensure!(
                    checks["package"].as_str()
                        == matches[0].get("checksum").and_then(toml::Value::as_str),
                    "native4_registry_package_checksum_changed"
                );
                // Original packed-source files are verified independently of the
                // complete current-cache inventory; neither set is discarded.
                for (name, hash) in checks["files"]
                    .as_object()
                    .context("native4_registry_checksum_files")?
                {
                    let relative = Path::new(name);
                    ensure!(
                        relative
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_))),
                        "native4_registry_checksum_path"
                    );
                    let binding = crate::native_archive::bind_file(&dir.join(relative))?;
                    ensure!(
                        hash.as_str() == Some(binding.sha256.as_str()),
                        "native4_registry_original_file_checksum_changed:{name}"
                    );
                }
                source_format = "legacy_cargo_checksum_file_inventory";
                cargo_cache_metadata.push(crate::native_archive::bind_file(
                    &dir.join(".cargo-checksum.json"),
                )?);
            } else {
                let index = relative
                    .components()
                    .next()
                    .context("native4_registry_index")?
                    .as_os_str();
                let archive = Path::new(&build.cargo_home)
                    .join("registry/cache")
                    .join(index)
                    .join(format!("{name}-{version}.crate"));
                let checksum = matches[0]
                    .get("checksum")
                    .and_then(toml::Value::as_str)
                    .context("native4_registry_locked_checksum")?;
                let (original_archive, packed, original_metadata) =
                    original_package_files(dir, &archive, checksum)?;
                package_archive = Some(original_archive);
                archive_metadata = original_metadata;
                packed_files = Some(packed);
                let marker = dir.join(".cargo-ok");
                let marker_binding = crate::native_archive::bind_file(&marker)?;
                ensure!(
                    strict_json(&read_regular_bytes(&marker, LIMIT)?)?
                        == serde_json::json!({"v":1}),
                    "native4_cargo_cache_metadata_format_uncovered"
                );
                ensure!(
                    marker_binding == crate::native_archive::bind_file(&marker)?,
                    "native4_cargo_cache_metadata_changed_after_read"
                );
                cargo_cache_metadata.push(marker_binding);
                source_format = "locked_crate_regular_payload_gnu_longname_and_cargo_ok_v1";
            }
            kind = "locked_registry";
        }
        let binding = crate::native_archive::bind_file(&manifest)?;
        let manifest_value: toml::Value =
            toml::from_str(std::str::from_utf8(&read_regular_bytes(&manifest, LIMIT)?)?)?;
        ensure!(
            manifest_value["package"]["name"].as_str() == Some(name),
            "native4_package_manifest_name_changed"
        );
        let files = inventory_files(dir)?;
        if let Some(mut packed) = packed_files {
            packed.insert(".cargo-ok".into());
            let actual = files
                .iter()
                .map(|f| {
                    Ok(Path::new(&f.path)
                        .strip_prefix(dir)?
                        .to_str()
                        .context("native4_utf8")?
                        .to_owned())
                })
                .collect::<Result<BTreeSet<_>>>()?;
            ensure!(
                actual == packed,
                "native4_original_crate_cache_payload_or_metadata_inventory_changed"
            );
        }
        ensure!(
            files.iter().any(|x| x == &binding),
            "native4_manifest_not_in_inventory"
        );
        Ok(CodeNativeCompilationSourceRoot4 {
            package_id: id.into(),
            package_name: name.into(),
            package_version: version.into(),
            source_kind: kind.into(),
            root: dir.to_str().context("native4_utf8")?.into(),
            cargo_manifest: binding,
            files,
            enabled_features: enabled_features.into_iter().collect(),
            source_format: source_format.into(),
            package_archive,
            cargo_cache_metadata,
            archive_metadata,
        })
    };
    let selected = selected.into_iter().collect::<Vec<_>>();
    let mut roots = verify_selected_packages(&selected, verify)?;
    roots.sort_by(|a, b| a.package_id.cmp(&b.package_id));
    ensure!(
        roots.iter().map(|p| &p.root).collect::<BTreeSet<_>>().len() == roots.len(),
        "native4_duplicate_root"
    );
    Ok(roots)
}
// A call-local work queue balances differently sized packages without retaining
// physical-file results across calls. Restore the original selected order only
// after all workers finish, including failures and caught package panics.
fn verify_selected_packages<T: Send>(
    selected: &[(&str, BTreeSet<String>)],
    verify: impl Fn(&str, BTreeSet<String>) -> Result<T> + Sync,
) -> Result<Vec<T>> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let handles = (0..selected.len().min(4))
            .map(|_| {
                let verify = &verify;
                let next = &next;
                scope.spawn(move || {
                    let mut results = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((id, features)) = selected.get(index) else {
                            break;
                        };
                        let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            verify(id, features.clone())
                        }))
                        .unwrap_or_else(|_| {
                            Err(anyhow::anyhow!("native4_source_package_panicked:{id}"))
                        });
                        results.push((index, value));
                    }
                    results
                })
            })
            .collect::<Vec<_>>();
        let batches = handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("native4_source_worker_panicked"))
            })
            .collect::<Vec<_>>();
        let mut ordered = Vec::with_capacity(selected.len());
        for batch in batches {
            ordered.extend(batch?);
        }
        ordered.sort_by_key(|(index, _)| *index);
        ordered.into_iter().map(|(_, value)| value).collect()
    })
}
fn metadata_arguments(target: &str) -> Vec<String> {
    vec![
        "metadata".into(),
        "--locked".into(),
        "--offline".into(),
        "--format-version".into(),
        "1".into(),
        "--filter-platform".into(),
        target.into(),
    ]
}
fn tree_arguments(stage: &str, target: &str) -> Result<Vec<String>> {
    Ok(vec![
        "tree".into(),
        "--locked".into(),
        "--offline".into(),
        "-p".into(),
        package_name(stage)?.into(),
        "--edges".into(),
        "normal,build".into(),
        "--target".into(),
        target.into(),
        "--prefix".into(),
        "none".into(),
        "--no-dedupe".into(),
        "--format".into(),
        "{p}\t{f}".into(),
    ])
}
fn selected_environment(
    build: &CodeBuildRequest,
) -> BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    BTreeMap::from([
        ("CARGO_HOME".into(), build.cargo_home.clone().into()),
        ("RUSTC".into(), build.rustc_path.clone().into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("LC_ALL".into(), "C".into()),
    ])
}
fn run_inventory_process(
    session: &crate::native_runtime6::NativeBuildSession6,
    folder: &Path,
    kind: &str,
    executable: &str,
    arguments: Vec<String>,
) -> Result<(
    ArtifactReference,
    ArtifactReference,
    ArtifactReference,
    Vec<u8>,
)> {
    let root = &session.root;
    let env = selected_environment(&session.side.build);
    let mut removed = std::env::vars_os()
        .map(|(k, _)| k)
        .filter(|k| !env.contains_key(k))
        .collect::<Vec<_>>();
    removed.sort();
    removed.dedup();
    let process_folder = folder.join(kind);
    let mut spec = CommandSpec {
        executable: executable.into(),
        arguments: arguments.into_iter().map(Into::into).collect(),
        cwd: root.path().into(),
        environment: env,
        remove_environment: removed,
        timeout: deadline_timeout(session.deadline)?,
        output_limit: session.output_limit,
    };
    crate::native_runtime::freeze_command(
        root,
        &mut spec,
        &process_folder,
        session.deadline_epoch_ms,
    )?;
    let observed = run_command(&spec, &root.private_path(&process_folder)?)?;
    ensure!(
        observed.status == "completed" && observed.exit_code == Some(0),
        "native4_actual_inventory_process_failed:{kind}"
    );
    deadline_timeout(session.deadline)?;
    Ok((
        reference(root, &process_folder.join("process.json"))?,
        reference(root, &process_folder.join("stdout"))?,
        reference(root, &process_folder.join("stderr"))?,
        observed.stdout,
    ))
}
fn verify_inventory_process(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    stage: &str,
    end: u64,
    kind: &str,
    executable: &str,
    arguments: &[String],
    refs: [&ArtifactReference; 3],
) -> Result<Vec<u8>> {
    let expected = Path::new(&side.stage_root)
        .join(stage)
        .join("compilation-sources")
        .join(kind);
    for (r, name) in refs.iter().zip(["process.json", "stdout", "stderr"]) {
        ensure!(
            **r == reference(root, &expected.join(name))?,
            "native4_source_process_wrong_stage:{kind}"
        );
    }
    let (command, process) = crate::native_runtime::verify_frozen_command_budget(
        root,
        refs[0],
        side.body_budget.output_bytes,
        side.body_budget.timeout_ms,
        Some(end),
    )?;
    let env = serde_json::to_value(
        selected_environment(&side.build)
            .into_iter()
            .map(|(k, v)| {
                Ok((
                    k.into_string()
                        .map_err(|_| anyhow::anyhow!("native4_env"))?,
                    v.into_string()
                        .map_err(|_| anyhow::anyhow!("native4_env"))?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?,
    )?;
    ensure!(
        command["environment"] == env
            && process["environment_added_keys"]
                == serde_json::to_value(env.as_object().unwrap().keys().collect::<Vec<_>>())?
            && process["environment_removed_keys"] == command["removed_environment"]
            && process["environment_values"] == "not_recorded",
        "native4_inventory_environment_changed:{kind}"
    );
    let removed = command["removed_environment"]
        .as_array()
        .context("native4_removed_environment")?;
    let keys = removed
        .iter()
        .map(|v| v.as_str().context("native4_removed_environment_key"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        keys.windows(2).all(|w| w[0] < w[1]) && keys.iter().all(|k| env.get(k).is_none()),
        "native4_inventory_removed_environment_changed"
    );
    let tool = crate::native_archive::bind_file(Path::new(executable))?;
    let (stdout, stderr) =
        crate::native_runtime::verify_process(root, refs[0], &tool, root.path(), arguments)?;
    ensure!(
        stdout == bytes(root, refs[1])? && stderr == bytes(root, refs[2])?,
        "native4_inventory_raw_stream_changed:{kind}"
    );
    Ok(stdout)
}
pub(crate) fn prepare(
    session: &crate::native_runtime6::NativeBuildSession6,
    stage: &str,
) -> Result<ArtifactReference> {
    let root = &session.root;
    let build = &session.side.build;
    deadline_timeout(session.deadline)?;
    let before_manifest = digest(&public_manifest(root.path())?)?;
    let cargo_lock = crate::native_archive::bind_file(&root.path().join("Cargo.lock"))?;
    let folder = Path::new(&session.side.stage_root)
        .join(stage)
        .join("compilation-sources");
    oe_core::create_new_private_dir(&root.private_path(&folder)?)?;
    let (compiler_process, compiler_stdout, compiler_stderr, compiler_raw) = run_inventory_process(
        session,
        &folder,
        "compiler",
        &build.rustc_path,
        vec!["-Vv".into()],
    )?;
    let host_target = compiler_host(&compiler_raw)?;
    let (target, target_selector) = selection(stage, &host_target, build)?;
    let (metadata_process, metadata_stdout, metadata_stderr, metadata_raw) = run_inventory_process(
        session,
        &folder,
        "metadata",
        &build.cargo_path,
        metadata_arguments(&target),
    )?;
    let (tree_process, tree_stdout, tree_stderr, tree_raw) = run_inventory_process(
        session,
        &folder,
        "tree",
        &build.cargo_path,
        tree_arguments(stage, &target)?,
    )?;
    let roots = derive_roots(root, build, stage, &strict_json(&metadata_raw)?, &tree_raw)?;
    ensure!(
        cargo_lock == crate::native_archive::bind_file(&root.path().join("Cargo.lock"))?
            && before_manifest == digest(&public_manifest(root.path())?)?,
        "native4_source_changed_during_selection"
    );
    let value = CodeNativeCompilationSources4 {
        schema: SCHEMA.into(),
        source_manifest_sha256: before_manifest,
        cargo_lock,
        compiler_process,
        compiler_stdout,
        compiler_stderr,
        metadata_process,
        metadata_stdout,
        metadata_stderr,
        tree_process,
        tree_stdout,
        tree_stderr,
        host_target,
        target,
        target_selector,
        roots,
    };
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("native4_source_contract:{e:?}"))?;
    let path = folder.join("sources.json");
    write_new(&root.private_path(&path)?, &serde_json::to_value(&value)?)?;
    let r = reference(root, &path)?;
    inspect(root, &session.side, stage, session.deadline_epoch_ms, &r)?;
    deadline_timeout(session.deadline)?;
    Ok(r)
}
pub(crate) fn inspect(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    stage: &str,
    end: u64,
    r: &ArtifactReference,
) -> Result<CodeNativeCompilationSources4> {
    let raw = bytes(root, r)?;
    let value: CodeNativeCompilationSources4 = serde_json::from_value(strict_json(&raw)?)?;
    value
        .validate()
        .map_err(|e| anyhow::anyhow!("native4_source_contract:{e:?}"))?;
    ensure!(
        value.schema == SCHEMA
            && value.source_manifest_sha256 == digest(&public_manifest(root.path())?)?,
        "native4_source_context_changed"
    );
    ensure!(
        value.cargo_lock == crate::native_archive::bind_file(&root.path().join("Cargo.lock"))?,
        "native4_source_lock_changed"
    );
    let compiler = verify_inventory_process(
        root,
        side,
        stage,
        end,
        "compiler",
        &side.build.rustc_path,
        &["-Vv".into()],
        [
            &value.compiler_process,
            &value.compiler_stdout,
            &value.compiler_stderr,
        ],
    )?;
    ensure!(
        value.host_target == compiler_host(&compiler)?,
        "native4_compiler_host_changed"
    );
    let (target, selector) = selection(stage, &value.host_target, &side.build)?;
    ensure!(
        value.target == target && value.target_selector == selector,
        "native4_stage_selector_changed"
    );
    let metadata = verify_inventory_process(
        root,
        side,
        stage,
        end,
        "metadata",
        &side.build.cargo_path,
        &metadata_arguments(&target),
        [
            &value.metadata_process,
            &value.metadata_stdout,
            &value.metadata_stderr,
        ],
    )?;
    let tree = verify_inventory_process(
        root,
        side,
        stage,
        end,
        "tree",
        &side.build.cargo_path,
        &tree_arguments(stage, &target)?,
        [&value.tree_process, &value.tree_stdout, &value.tree_stderr],
    )?;
    ensure!(
        derive_roots(root, &side.build, stage, &strict_json(&metadata)?, &tree)? == value.roots,
        "native4_compilation_sources_changed"
    );
    ensure!(
        value.cargo_lock == crate::native_archive::bind_file(&root.path().join("Cargo.lock"))?
            && bytes(root, r)? == raw,
        "native4_source_changed_after_inspection"
    );
    Ok(value)
}
pub(crate) fn caller_source(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    stage: &str,
    context: &Value,
    cwd: &Path,
) -> Result<CodeNativeCompilationSourceRoot4> {
    let r: ArtifactReference = serde_json::from_value(context["compilation_sources"].clone())?;
    let value = inspect(
        root,
        side,
        stage,
        context["deadline_epoch_ms"]
            .as_u64()
            .context("native4_deadline")?,
        &r,
    )?;
    no_symlinks(cwd)?;
    ensure!(
        cwd.is_absolute() && cwd.is_dir() && cwd.canonicalize()? == cwd,
        "native4_caller_not_canonical"
    );
    ensure!(
        value
            .roots
            .iter()
            .filter(|p| Path::new(&p.root) == cwd)
            .count()
            == 1,
        "native4_caller_not_exact_selected_root"
    );
    value
        .roots
        .into_iter()
        .find(|p| Path::new(&p.root) == cwd)
        .context("native4_selected_caller_source_missing")
}

pub(crate) fn caller_root(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    stage: &str,
    context: &Value,
    cwd: &Path,
) -> Result<PathBuf> {
    Ok(PathBuf::from(
        caller_source(root, side, stage, context, cwd)?.root,
    ))
}

#[cfg(test)]
#[path = "native_sources6_tests.rs"]
mod tests;
