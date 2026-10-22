//! Project identity and local storage primitives. No semantic decisions live here.
#[cfg(all(test, unix))]
mod directory_fd_tests;
pub mod json;
pub mod privacy;
pub mod private_file;
pub mod process;
#[path = "../../../build_support/source_limits.rs"]
pub mod source_limits;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
#[cfg(not(unix))]
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

pub use oe_wire::csv;
pub use oe_wire::sha256;

pub const PRIVATE_DIRECTORY_ATTEMPTS: u32 = 32;
static PRIVATE_DIRECTORY_SERIAL: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Stable source metadata, independent of the host's path separator. This does
/// not resolve symlinks or authorize filesystem access; callers retain those
/// checks. Reject lossy UTF-8 and non-normal components instead of rewriting
/// their identity. Package-specific filename rules still apply separately.
pub fn portable_source_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .context("portable_source_path_outside_root")?;
    let mut names = Vec::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("portable_source_path_invalid");
        };
        let name = name.to_str().context("portable_source_path_not_utf8")?;
        ensure!(
            !name.is_empty()
                && !name.contains(['/', '\\', ':'])
                && !name.chars().any(char::is_control),
            "portable_source_path_invalid"
        );
        names.push(name);
    }
    ensure!(!names.is_empty(), "portable_source_path_invalid");
    Ok(names.join("/"))
}

/// Persistent advisory lock; dropping a guard releases it without unlinking the
/// shared coordination inode. Callers choose the permitted private pathname.
pub struct AdvisoryFileLock {
    file: fs::File,
    path: PathBuf,
}

impl AdvisoryFileLock {
    pub fn acquire(path: &Path, exclusive: bool) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            use std::os::unix::fs::MetadataExt;
            let parent = unix_directory(path.parent().context("lock_parent_missing")?, true)?;
            let name = unix_name(path.file_name().context("lock_name_missing")?)?;
            let fd = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC
                        | libc::O_NONBLOCK,
                    0o600,
                )
            };
            ensure!(fd >= 0, "lock_open_failed");
            let file = unsafe { fs::File::from_raw_fd(fd) };
            let m = file.metadata()?;
            ensure!(
                m.is_file() && m.len() == 0 && m.nlink() == 1,
                "lock_not_private_regular_empty_file"
            );
            let operation = if exclusive {
                libc::LOCK_EX
            } else {
                libc::LOCK_SH
            };
            ensure!(
                unsafe { libc::flock(file.as_raw_fd(), operation | libc::LOCK_NB) } == 0,
                "resource_busy"
            );
            let guard = Self {
                file,
                path: path.to_owned(),
            };
            guard.ensure_current()?;
            Ok(guard)
        }
        #[cfg(not(unix))]
        {
            let _ = (path, exclusive);
            bail!("advisory_lock_unsupported_platform")
        }
    }

    pub fn ensure_current(&self) -> Result<()> {
        let selected = open_regular_file(&self.path, 0)?;
        let a = self.file.metadata()?;
        let b = selected.metadata()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                a.dev() == b.dev() && a.ino() == b.ino() && a.nlink() == 1,
                "lock_identity_changed"
            );
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = (a, b);
            bail!("advisory_lock_unsupported_platform")
        }
    }
}

/// Validate existing ancestors, including a not-yet-created destination.
pub fn no_symlinks(path: &Path) -> Result<()> {
    let mut ancestor = PathBuf::new();
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir),
            "parent_traversal"
        );
        ancestor.push(component);
        #[cfg(windows)]
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&ancestor) {
            Ok(metadata) => ensure!(!metadata.file_type().is_symlink(), "symlink_path"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("path_metadata_unavailable"),
        }
    }
    Ok(())
}

/// Move a directory without replacing any existing destination. Parent file
/// descriptors keep both sides anchored; callers retain their identity checks.
pub fn rename_directory_exclusive(from: &Path, to: &Path) -> Result<()> {
    no_symlinks(from)?;
    no_symlinks(to)?;
    ensure!(
        fs::symlink_metadata(from)?.is_dir(),
        "rename_source_not_directory"
    );
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::fd::AsRawFd;
        let source = unix_directory(from.parent().context("missing_parent")?, false)?;
        let destination = unix_directory(to.parent().context("missing_parent")?, false)?;
        let source_name = unix_name(from.file_name().context("missing_name")?)?;
        let destination_name = unix_name(to.file_name().context("missing_name")?)?;
        #[cfg(target_os = "macos")]
        let result = unsafe {
            libc::renameatx_np(
                source.as_raw_fd(),
                source_name.as_ptr(),
                destination.as_raw_fd(),
                destination_name.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                source.as_raw_fd(),
                source_name.as_ptr(),
                destination.as_raw_fd(),
                destination_name.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error())
                .context("exclusive_directory_rename_failed");
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        oe_windows_controller::windows::rename_directory_exclusive(from, to)
            .context("exclusive_directory_rename_failed")
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        bail!("exclusive_directory_rename_unsupported_platform")
    }
}

#[derive(Debug, Clone)]
pub struct SkillRoot(PathBuf);

impl SkillRoot {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        no_symlinks(path)?;
        let root = fs::canonicalize(path).context("skill_root_unavailable")?;
        ensure!(root.join("SKILL.md").is_file(), "skill_root_missing_entry");
        Ok(Self(root))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn expand(&self, value: &Path) -> Result<PathBuf> {
        // Joining a relative path to a verbatim Windows root can remove '..'.
        // Preserve the input rejection before PathBuf performs that rewrite.
        #[cfg(windows)]
        ensure!(
            !value
                .components()
                .any(|c| matches!(c, Component::ParentDir)),
            "parent_traversal"
        );
        let path = if let Ok(tail) = value.strip_prefix("~") {
            let home = std::env::var_os("HOME").context("home_unavailable")?;
            PathBuf::from(home).join(tail)
        } else if value.is_absolute() {
            value.to_path_buf()
        } else {
            self.0.join(value)
        };
        #[cfg(windows)]
        return windows_future_path(&path);
        #[cfg(not(windows))]
        Ok(path)
    }

    pub fn private_path(&self, value: &Path) -> Result<PathBuf> {
        let path = self.expand(value)?;
        let base = self.0.join("var");
        ensure!(
            path.starts_with(&base) && path != base,
            "path_outside_private_root"
        );
        no_symlinks(&path)?;
        Ok(path)
    }

    pub fn new_private_dir(&self, value: &Path) -> Result<PathBuf> {
        let path = self.private_path(value)?;
        create_new_private_dir(&path)?;
        Ok(path)
    }

    /// Names are opaque storage candidates, never execution/source authority.
    /// Only exclusive mkdir grants a new directory; history is not read/reused.
    pub fn unique_private_dir(&self, parent: &Path, prefix: &str) -> Result<PathBuf> {
        ensure!(
            prefix
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "private_directory_prefix_invalid"
        );
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("execution_timestamp_before_epoch")?
            .as_nanos();
        let serial = PRIVATE_DIRECTORY_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let identity = format!("{}-{}-{serial}", std::process::id(), nanos);
        let stem = if prefix.is_empty() {
            identity
        } else {
            format!("{prefix}-{identity}")
        };
        self.reserve_private_directory(parent, &stem)
    }

    /// Bounded reservation of an opaque caller-supplied basename. This low-level
    /// primitive grants no PID/time/binding truth, just as new_private_dir does.
    pub fn reserve_private_directory(&self, parent: &Path, stem: &str) -> Result<PathBuf> {
        ensure!(
            !stem.is_empty()
                && stem.len() <= 192
                && stem
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "private_directory_prefix_invalid"
        );
        for attempt in 0..PRIVATE_DIRECTORY_ATTEMPTS {
            let candidate = parent.join(format!("{stem}-{attempt}"));
            match self.new_private_dir(&candidate) {
                Ok(path) => return Ok(path),
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists) =>
                {
                    // A link inserted during the failed mkdir is an error,
                    // not permission to silently choose a different target.
                    self.private_path(&candidate)?;
                }
                Err(error) => return Err(error),
            }
        }
        // Preserve the existing CLI's bounded collision diagnostic.
        bail!("execution_directory_collision_budget_exceeded")
    }
}

/// Preserve the same physical Windows prefix/case for both an existing root
/// and a not-yet-created output. Check links before canonicalization, then keep
/// the literal missing tail; parent traversal is never normalized into authority.
#[cfg(windows)]
fn windows_future_path(path: &Path) -> Result<PathBuf> {
    no_symlinks(path)?;
    let mut existing = path;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(existing.file_name().context("windows_path_leaf_missing")?);
                existing = existing.parent().context("windows_path_ancestor_missing")?;
            }
            Err(error) => return Err(error).context("windows_path_ancestor_unavailable"),
        }
    }
    let mut normalized = fs::canonicalize(existing).context("windows_path_identity_unavailable")?;
    for name in missing.into_iter().rev() {
        normalized.push(name);
    }
    no_symlinks(&normalized)?;
    Ok(normalized)
}

/// Exclusive private directory creation. The caller chooses its allowed root;
/// this primitive anchors all parent lookups and never adopts a final path.
pub fn create_new_private_dir(path: &Path) -> Result<()> {
    let parent = path.parent().context("missing_parent")?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let directory = unix_directory(parent, true)?;
        let name = unix_name(path.file_name().context("missing_name")?)?;
        if unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error()).context("private_directory_create_failed");
        }
    }
    #[cfg(not(unix))]
    {
        no_symlinks(path)?;
        fs::create_dir_all(parent).context("parent_create_failed")?;
        no_symlinks(parent)?;
        fs::create_dir(path).context("private_directory_create_failed")?;
    }
    Ok(())
}

/// Ensure a directory exists without following directory links. Unix lookups
/// use parent descriptors; Windows retains pinned parent handles. A changed
/// pathname is rejected after creation; callers must still anchor later I/O.
pub fn ensure_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        ensure_directory_with(path, |_| Ok(()))
    }
    #[cfg(windows)]
    {
        oe_windows_controller::windows::ensure_directory(path)
            .context("anchored_directory_creation_failed")
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        bail!("anchored_directory_creation_platform_unsupported")
    }
}

#[cfg(unix)]
fn ensure_directory_with(
    path: &Path,
    before_component: impl FnMut(&Path) -> Result<()>,
) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let created = unix_directory_with(path, true, before_component)?;
    let current = unix_directory(path, false)?;
    let before = created.metadata()?;
    let after = current.metadata()?;
    ensure!(
        before.dev() == after.dev() && before.ino() == after.ino(),
        "directory_path_changed"
    );
    Ok(())
}

/// Append-only artifact creation; callers select a private path before writing.
pub fn write_new(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_new_bytes(path, &bytes)
}

pub fn write_new_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = create_new_regular_file(path)?;
    file.write_all(bytes).context("artifact_write_failed")?;
    file.sync_all().context("artifact_sync_failed")
}

/// Open a fresh private regular output without following directory or leaf links.
/// The caller chooses the allowed root and bounds any streaming writes.
pub fn create_new_regular_file(path: &Path) -> Result<fs::File> {
    #[cfg(unix)]
    let file = {
        use std::os::fd::{AsRawFd, FromRawFd};
        let directory = unix_directory(path.parent().context("missing_parent")?, false)?;
        let name = unix_name(path.file_name().context("missing_name")?)?;
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        ensure!(descriptor >= 0, "artifact_create_failed");
        // Ownership of this fresh descriptor passes to File exactly once.
        unsafe { fs::File::from_raw_fd(descriptor) }
    };
    #[cfg(not(unix))]
    let file = {
        no_symlinks(path)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        options.open(path).context("artifact_create_failed")?
    };
    Ok(file)
}

/// Read one bounded regular file without following Unix symlink components.
/// Callers still select the allowed root and verify the expected content hash.
/// Nonblocking open lets us reject a FIFO before attempting any read.
pub fn read_regular_bytes(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let read_limit = u64::try_from(limit)?
        .checked_add(1)
        .context("invalid_read_limit")?;
    let file = open_regular_file(path, u64::try_from(limit)?)?;
    let mut bytes = Vec::new();
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .context("input_read_failed")?;
    ensure!(bytes.len() <= limit, "input_exceeds_limit");
    Ok(bytes)
}

/// Atomically publish an already prepared regular file at one destination name.
/// Directory descriptors anchor both parents on Unix. The destination may be
/// absent or a regular file; this is not a content-conditional filesystem CAS.
/// Callers retain their source/target snapshots and coordinate participating writers.
pub fn rename_regular_file(from: &Path, to: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::MetadataExt;
        let source_parent = from.parent().context("publication_source_parent_missing")?;
        let target_parent = to.parent().context("publication_target_parent_missing")?;
        let source_dir = unix_directory(source_parent, false)?;
        let target_dir = unix_directory(target_parent, false)?;
        let source_name = unix_name(
            from.file_name()
                .context("publication_source_name_missing")?,
        )?;
        let target_name = unix_name(to.file_name().context("publication_target_name_missing")?)?;
        for (directory, name, may_be_absent) in [
            (&source_dir, &source_name, false),
            (&target_dir, &target_name, true),
        ] {
            let mut metadata: libc::stat = unsafe { std::mem::zeroed() };
            let status = unsafe {
                libc::fstatat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    &mut metadata,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if status != 0 {
                let error = std::io::Error::last_os_error();
                if may_be_absent && error.raw_os_error() == Some(libc::ENOENT) {
                    continue;
                }
                return Err(error).context("publication_file_stat_failed");
            }
            ensure!(
                (metadata.st_mode & libc::S_IFMT) == libc::S_IFREG,
                "publication_file_not_regular"
            );
        }
        ensure!(
            source_dir.metadata()?.dev() == target_dir.metadata()?.dev(),
            "publication_cross_device"
        );
        let result = unsafe {
            libc::renameat(
                source_dir.as_raw_fd(),
                source_name.as_ptr(),
                target_dir.as_raw_fd(),
                target_name.as_ptr(),
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error()).context("publication_rename_failed");
        }
        for (path, original) in [(source_parent, &source_dir), (target_parent, &target_dir)] {
            let now = unix_directory(path, false)?.metadata()?;
            let before = original.metadata()?;
            ensure!(
                now.dev() == before.dev() && now.ino() == before.ino(),
                "publication_parent_changed"
            );
        }
        target_dir
            .sync_all()
            .context("publication_directory_sync_failed")?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (from, to);
        bail!("anchored_file_publication_platform_unsupported")
    }
}

/// Open one bounded regular input using anchored directory descriptors on Unix.
/// Streaming callers must also bound bytes read after open: a file can grow.
pub fn open_regular_file(path: &Path, limit: u64) -> Result<fs::File> {
    #[cfg(unix)]
    let file = {
        use std::os::fd::{AsRawFd, FromRawFd};
        let directory = unix_directory(path.parent().context("missing_parent")?, false)?;
        let name = unix_name(path.file_name().context("missing_name")?)?;
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error()).context("input_open_failed");
        }
        unsafe { fs::File::from_raw_fd(descriptor) }
    };
    #[cfg(not(unix))]
    let file = {
        no_symlinks(path)?;
        ensure!(
            fs::symlink_metadata(path)?.is_file(),
            "input_not_regular_file"
        );
        fs::File::open(path).context("input_open_failed")?
    };
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "input_not_regular_file");
    ensure!(metadata.len() <= limit, "input_exceeds_limit");
    Ok(file)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileIdentity {
    pub bytes: u64,
    pub sha256: String,
}

/// Hash large source files without loading them into memory or raising JSON limits.
/// An expected digest and any subsequent pathname reopen are the caller's checks.
pub fn fingerprint_file(path: &Path, limit: u64) -> Result<FileIdentity> {
    let mut file = open_regular_file(path, limit)?;
    let before = file.metadata()?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        let count = file.read(&mut buffer).context("input_read_failed")?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .context("input_exceeds_limit")?;
        ensure!(bytes <= limit, "input_exceeds_limit");
        hash.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    ensure!(
        bytes == before.len() && bytes == after.len() && before.modified()? == after.modified()?,
        "input_changed_during_read"
    );
    Ok(FileIdentity {
        bytes,
        sha256: format!("{:x}", hash.finalize()),
    })
}

#[cfg(unix)]
fn unix_name(name: &std::ffi::OsStr) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(name.as_bytes()).context("invalid_path_component")
}

/// Walk via directory descriptors. A swapped ancestor cannot redirect a write.
#[cfg(unix)]
fn unix_directory(path: &Path, create: bool) -> Result<fs::File> {
    unix_directory_with(path, create, |_| Ok(()))
}

#[cfg(unix)]
fn unix_directory_with(
    path: &Path,
    create: bool,
    mut before_component: impl FnMut(&Path) -> Result<()>,
) -> Result<fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let descriptor = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    ensure!(descriptor >= 0, "directory_open_failed");
    let mut directory = unsafe { fs::File::from_raw_fd(descriptor) };
    let mut component_path = PathBuf::from("/");
    for part in absolute.components() {
        let name = match part {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => unix_name(name)?,
            _ => bail!("invalid_directory_component"),
        };
        component_path.push(part.as_os_str());
        before_component(&component_path)?;
        if create {
            let status = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
            if status != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
                return Err(std::io::Error::last_os_error()).context("parent_create_failed");
            }
        }
        let next = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        ensure!(next >= 0, "directory_open_failed");
        directory = unsafe { fs::File::from_raw_fd(next) };
    }
    Ok(directory)
}

pub fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    match value.as_str() {
        Some(text) if !text.trim().is_empty() => Ok(text),
        _ => bail!("invalid_{name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (SkillRoot, PathBuf) {
        let root = SkillRoot::new(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap(),
        )
        .unwrap();
        let path = root
            .new_private_dir(Path::new(&format!(
                "var/maintenance/rust-tests/core-{}-{}",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::Relaxed)
            )))
            .unwrap();
        (root, path)
    }

    #[test]
    fn artifacts_are_append_only() {
        let (_, path) = fixture();
        write_new_bytes(&path.join("evidence.txt"), b"original").unwrap();
        assert!(write_new_bytes(&path.join("evidence.txt"), b"changed").is_err());
        assert_eq!(fs::read(path.join("evidence.txt")).unwrap(), b"original");
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn reads_are_bounded_and_require_regular_files() {
        let (_, path) = fixture();
        let file = path.join("原始证据.txt");
        fs::write(&file, b"source").unwrap();
        assert_eq!(read_regular_bytes(&file, 6).unwrap(), b"source");
        assert!(read_regular_bytes(&file, 5).is_err());
        assert!(read_regular_bytes(&path, 6).is_err());
        assert!(read_regular_bytes(&path.join("missing"), 6).is_err());
        assert!(read_regular_bytes(&path.join("../outside"), 6).is_err());
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn reads_reject_final_and_parent_symlinks() {
        use std::os::unix::fs::symlink;
        let (_, path) = fixture();
        let source = path.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("data"), b"evidence").unwrap();
        symlink(&source, path.join("linked-directory")).unwrap();
        symlink(source.join("data"), path.join("linked-file")).unwrap();
        assert!(read_regular_bytes(&path.join("linked-directory/data"), 100).is_err());
        assert!(read_regular_bytes(&path.join("linked-file"), 100).is_err());
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn fifo_reads_fail_without_a_writer() {
        use std::os::unix::ffi::OsStrExt;
        let (_, path) = fixture();
        let fifo = path.join("fifo");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        assert!(read_regular_bytes(&fifo, 100).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn outputs_are_owned_by_skill() {
        let (root, path) = fixture();
        assert!(root.private_path(Path::new("sources/output")).is_err());
        assert!(
            root.private_path(Path::new("var/../sources/output"))
                .is_err()
        );
        assert!(root.new_private_dir(&path).is_err());
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_ancestor_is_rejected() {
        let (root, path) = fixture();
        std::os::unix::fs::symlink(root.path().join("sources"), path.join("linked")).unwrap();
        assert!(root.private_path(&path.join("linked/output")).is_err());
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ensure_directory_anchored_creation_does_not_follow_retargeted_parent() {
        let (_, path) = fixture();
        let parent = path.join("parent");
        let moved = path.join("held-parent");
        let alternative = path.join("alternative");
        let requested = parent.join("new/leaf");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&alternative).unwrap();
        let mut changed = false;
        let result = ensure_directory_with(&requested, |component| {
            if component == parent.join("new") {
                fs::rename(&parent, &moved)?;
                std::os::unix::fs::symlink(&alternative, &parent)?;
                changed = true;
            }
            Ok(())
        });
        assert!(changed);
        assert!(result.is_err());
        assert!(moved.join("new/leaf").is_dir());
        assert!(!alternative.join("new").exists());
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ensure_directory_rejects_replaced_regular_parent_even_when_final_path_exists() {
        let (_, path) = fixture();
        let parent = path.join("parent");
        let moved = path.join("held-parent");
        let requested = parent.join("new/leaf");
        fs::create_dir(&parent).unwrap();
        let mut changed = false;
        let result = ensure_directory_with(&requested, |component| {
            if component == parent.join("new") {
                fs::rename(&parent, &moved)?;
                fs::create_dir_all(&requested)?;
                fs::write(requested.join("owner"), b"external owner\n")?;
                changed = true;
            }
            Ok(())
        });
        assert!(changed);
        assert_eq!(result.unwrap_err().to_string(), "directory_path_changed");
        assert!(moved.join("new/leaf").is_dir());
        assert_eq!(
            fs::read(requested.join("owner")).unwrap(),
            b"external owner\n"
        );
        fs::remove_dir_all(path).unwrap();
    }
}
