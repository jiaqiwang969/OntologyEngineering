//! Assemble one explicit public source inventory and the seven release programs.
//! Source closure, byte-copy and privacy checks retain their existing owners.
use crate::{package, release};
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, fingerprint_file};
use std::path::{Path, PathBuf};
#[path = "../../../build_support/code_identity.rs"]
#[allow(dead_code)]
mod source_identity;

/// The release source list uses exactly the producer's existing public identity
/// owner, including runtime assets; there is no independent runtime whitelist.
pub fn inventory(root: &SkillRoot) -> Result<package::PortableManifest> {
    let observed = source_identity::manifest(root.path())?;
    let mut files = Vec::new();
    for (path, value) in observed["files"]
        .as_object()
        .context("release_source_manifest_invalid")?
    {
        files.push(package::SourceFile {
            path: path.clone(),
            sha256: value["sha256"]
                .as_str()
                .context("release_source_digest_missing")?
                .into(),
            bytes: value["bytes"]
                .as_u64()
                .context("release_source_size_missing")?,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = package::PortableManifest {
        format: package::FORMAT.into(),
        entry: "SKILL.md".into(),
        scope: package::SCOPE.into(),
        files,
    };
    manifest.validate()?;
    Ok(manifest)
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = oe_core::open_regular_file(path, package::MAX_FILE_BYTES)?;
        file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        Ok(())
    }
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    {
        ensure!(mode == 0, "release_windows_mode_must_be_zero");
        release::windows_fs::stamp(path, true)?;
        Ok(())
    }
    #[cfg(not(any(unix, all(windows, target_arch = "x86_64", target_env = "msvc"))))]
    {
        let _ = (path, mode);
        anyhow::bail!("release_platform_unsupported")
    }
}

fn unchanged_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = oe_core::open_regular_file(path, package::MAX_FILE_BYTES)?;
        ensure!(
            file.metadata()?.permissions().mode() & 0o7777 == mode,
            "release_source_mode_changed"
        );
        Ok(())
    }
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    {
        ensure!(mode == 0, "release_windows_mode_must_be_zero");
        release::windows_fs::check_source(path)
    }
    #[cfg(not(any(unix, all(windows, target_arch = "x86_64", target_env = "msvc"))))]
    {
        let _ = (path, mode);
        anyhow::bail!("release_platform_unsupported")
    }
}

fn copy_source(source: &Path, destination: &Path, expected: &package::SourceFile) -> Result<u32> {
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    {
        release::windows_fs::copy_source(source, destination, expected)
    }
    #[cfg(not(all(windows, target_arch = "x86_64", target_env = "msvc")))]
    {
        package::copy_locked(source, destination, expected)
    }
}
fn create_dir(path: &Path) -> Result<()> {
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    {
        release::windows_fs::create_dir(path)
    }
    #[cfg(not(all(windows, target_arch = "x86_64", target_env = "msvc")))]
    {
        oe_core::create_new_private_dir(path)
    }
}
fn ensure_stage_dir(root: &Path, path: &Path) -> Result<()> {
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    {
        release::windows_fs::ensure_directories(root, path)
    }
    #[cfg(not(all(windows, target_arch = "x86_64", target_env = "msvc")))]
    {
        let _ = root;
        oe_core::ensure_directory(path)
    }
}

pub fn assemble(
    root: &SkillRoot,
    inventory: &Path,
    program_dir: &Path,
    output: &Path,
) -> Result<release::VerifiedRelease> {
    ensure!(
        cfg!(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(windows, target_arch = "x86_64", target_env = "msvc")
        )),
        "release_platform_unsupported"
    );
    let inventory = root.expand(inventory)?;
    let inventory_identity = fingerprint_file(&inventory, 4 * 1024 * 1024)?;
    let source = package::read_manifest(&inventory)?;
    let public_before = source_identity::manifest(root.path())?;
    ensure!(
        source == self::inventory(root)?,
        "release_incomplete_public_inventory"
    );
    package::check_source_closure(root, &source)?;
    let programs = root.expand(program_dir)?;
    oe_core::no_symlinks(&programs)?;
    ensure!(programs.is_dir(), "release_program_directory_missing");
    let output = root.private_path(output)?;
    oe_core::no_symlinks(&output)?;
    ensure!(!output.try_exists()?, "release_output_exists");
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    let native_before: std::collections::BTreeMap<_, _> = source
        .files
        .iter()
        .map(|row| root.path().join(&row.path))
        .chain(
            release::PROGRAM_NAMES
                .iter()
                .map(|name| programs.join(name)),
        )
        .chain(std::iter::once(inventory.clone()))
        .map(|path| release::windows_fs::source_stamp(&path).map(|stamp| (path, stamp)))
        .collect::<Result<_>>()?;
    let staging = root.unique_private_dir(Path::new("var/staging"), "release")?;
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    release::windows_fs::protect_new(&staging)?;
    let source_root = staging.join("skill");
    create_dir(&source_root)?;
    create_dir(&staging.join("bin"))?;
    let mut files = Vec::new();
    for item in &source.files {
        let destination = source_root.join(&item.path);
        ensure_stage_dir(
            &staging,
            destination.parent().context("release_parent_missing")?,
        )?;
        let mode = copy_source(&root.path().join(&item.path), &destination, item)?;
        unchanged_mode(&root.path().join(&item.path), mode)?;
        set_mode(&destination, mode)?;
        files.push(release::ReleaseFile {
            path: format!("skill/{}", item.path),
            sha256: item.sha256.clone(),
            bytes: item.bytes,
            mode: u64::from(mode),
        });
    }
    let snapshot = SkillRoot::new(&source_root)?;
    let privacy = crate::privacy::scan(
        &snapshot,
        &crate::privacy::ScanOptions {
            git: PathBuf::from("git"),
            output: PathBuf::from("var/privacy"),
            tracked_only: false,
            include_ignored: true,
        },
    )?;
    package::verify_privacy(&privacy, &source)?;
    // This private directory was created by the scanner inside our new stage.
    // It is not a user's data directory and is never part of the payload.
    let scanner_output = source_root.join("var");
    oe_core::no_symlinks(&scanner_output)?;
    if scanner_output.exists() {
        std::fs::remove_dir_all(&scanner_output)?;
    }
    let mut program_sources = Vec::new();
    for name in release::PROGRAM_NAMES {
        let path = programs.join(name);
        let identity = fingerprint_file(&path, 512 * 1024 * 1024)?;
        let row = package::SourceFile {
            path: (*name).into(),
            sha256: identity.sha256,
            bytes: identity.bytes,
        };
        let mode = copy_source(&path, &staging.join("bin").join(name), &row)?;
        #[cfg(not(windows))]
        ensure!(mode & 0o111 != 0, "release_program_not_executable");
        unchanged_mode(&path, mode)?;
        set_mode(&staging.join("bin").join(name), mode)?;
        files.push(release::ReleaseFile {
            path: format!("bin/{name}"),
            sha256: row.sha256.clone(),
            bytes: row.bytes,
            mode: u64::from(mode),
        });
        program_sources.push((path, row, mode));
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let version = std::str::from_utf8(&oe_core::read_regular_bytes(
        &source_root.join("VERSION"),
        1024,
    )?)?
    .trim()
    .to_owned();
    let manifest = release::ReleaseManifest {
        schema: release::SCHEMA.into(),
        version,
        target: release::TARGET.into(),
        entry: release::ENTRY.into(),
        skill_root: "skill".into(),
        files,
    };
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    {
        let mut bytes = serde_json::to_vec_pretty(&manifest)?;
        bytes.push(b'\n');
        release::windows_fs::write_new_bytes(&staging.join(release::MANIFEST_NAME), &bytes)?;
    }
    #[cfg(not(all(windows, target_arch = "x86_64", target_env = "msvc")))]
    oe_core::write_new(
        &staging.join(release::MANIFEST_NAME),
        &serde_json::to_value(&manifest)?,
    )?;
    let digest = fingerprint_file(&staging.join(release::MANIFEST_NAME), 4 * 1024 * 1024)?.sha256;
    release::verify(&staging, &digest)?;
    package::check_source_closure(root, &source)?;
    ensure!(
        fingerprint_file(&inventory, 4 * 1024 * 1024)? == inventory_identity,
        "release_inventory_changed"
    );
    for item in &source.files {
        let actual = fingerprint_file(&root.path().join(&item.path), package::MAX_FILE_BYTES)?;
        ensure!(
            actual.sha256 == item.sha256 && actual.bytes == item.bytes,
            "release_source_changed"
        );
        let row = manifest
            .files
            .iter()
            .find(|row| row.path == format!("skill/{}", item.path))
            .context("release_source_missing")?;
        unchanged_mode(
            &root.path().join(&item.path),
            release::permission_mode(row.mode)?,
        )?;
    }
    for (path, expected, mode) in program_sources {
        let actual = fingerprint_file(&path, 512 * 1024 * 1024)?;
        ensure!(
            actual.sha256 == expected.sha256 && actual.bytes == expected.bytes,
            "release_program_changed"
        );
        unchanged_mode(&path, mode)?;
    }
    ensure!(
        source_identity::manifest(root.path())? == public_before,
        "release_public_source_changed"
    );
    #[cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]
    for (path, before) in native_before {
        ensure!(
            release::windows_fs::source_stamp(&path)? == before,
            "release_windows_source_identity_changed"
        );
    }
    oe_core::ensure_directory(output.parent().context("release_parent_missing")?)?;
    oe_core::rename_directory_exclusive(&staging, &output)?;
    release::verify(&output, &digest)
}
