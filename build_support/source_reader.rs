//! Raw source I/O shared by runtime normalization and build input capture.
//! Unix holds directory descriptors; Windows reuses the platform pinning owner.
#[cfg(unix)]
use std::{
    fs::{File, Metadata},
    io::Read,
};
use std::{io, path::Path};

fn failed(error: io::Error) -> String {
    error.to_string()
}
fn invalid(token: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, token)
}

pub fn read(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    bounded(path, maximum, false).map(|v| v.expect("strict source read returns bytes"))
}
/// Missing or oversized legacy root probes remain uncaptured. Links and
/// nonregular inputs are failures; an empty capture never validates a /2 load.
pub fn probe(path: &Path, maximum: usize) -> Result<Option<Vec<u8>>, String> {
    bounded(path, maximum, true)
}

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Checkpoint {
    ParentsOpened,
    FileOpened,
    ReadComplete,
}

#[cfg(unix)]
struct Opened {
    directories: Vec<File>,
    file: File,
}

#[cfg(unix)]
fn open_anchored(path: &Path, after_parents: impl FnOnce()) -> io::Result<Opened> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
        path::Component,
    };
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| invalid("design_composition_parent_missing"))?;
    let leaf = absolute
        .file_name()
        .ok_or_else(|| invalid("design_composition_file_leaf_required"))?;
    let name = |s: &std::ffi::OsStr| {
        CString::new(s.as_bytes()).map_err(|_| invalid("design_composition_unsafe_path"))
    };
    let fd = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut directories = vec![unsafe { File::from_raw_fd(fd) }];
    for component in parent.components() {
        let component = match component {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(value) => name(value)?,
            _ => return Err(invalid("design_composition_unsafe_path")),
        };
        let next = unsafe {
            libc::openat(
                directories.last().unwrap().as_raw_fd(),
                component.as_ptr(),
                libc::O_RDONLY
                    | libc::O_DIRECTORY
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | libc::O_CLOEXEC,
            )
        };
        if next < 0 {
            return Err(io::Error::last_os_error());
        }
        directories.push(unsafe { File::from_raw_fd(next) });
    }
    after_parents();
    let leaf = name(leaf)?;
    let fd = unsafe {
        libc::openat(
            directories.last().unwrap().as_raw_fd(),
            leaf.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Opened {
        directories,
        file: unsafe { File::from_raw_fd(fd) },
    })
}

#[cfg(unix)]
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    (
        a.dev(),
        a.ino(),
        a.len(),
        a.nlink(),
        a.mtime(),
        a.mtime_nsec(),
        a.ctime(),
        a.ctime_nsec(),
    ) == (
        b.dev(),
        b.ino(),
        b.len(),
        b.nlink(),
        b.mtime(),
        b.mtime_nsec(),
        b.ctime(),
        b.ctime_nsec(),
    )
}

#[cfg(unix)]
fn verify(path: &Path, opened: &Opened, before: &Metadata) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    if !same_file(before, &opened.file.metadata()?) {
        return Err(invalid("design_composition_changed_during_read"));
    }
    // Rewalk from / via no-follow descriptors: a renamed held parent does not
    // grant authority to bytes beneath the new pathname, even at equal mtime.
    let current = open_anchored(path, || {})?;
    if current.directories.len() != opened.directories.len() {
        return Err(invalid("design_composition_changed_during_read"));
    }
    for (old, new) in opened.directories.iter().zip(&current.directories) {
        let a = old.metadata()?;
        let b = new.metadata()?;
        if (a.dev(), a.ino()) != (b.dev(), b.ino()) {
            return Err(invalid("design_composition_changed_during_read"));
        }
    }
    if !same_file(before, &current.file.metadata()?) {
        return Err(invalid("design_composition_changed_during_read"));
    }
    Ok(())
}

#[cfg(unix)]
fn unix_bounded(
    path: &Path,
    maximum: usize,
    optional: bool,
    mut checkpoint: impl FnMut(Checkpoint, Option<&File>),
) -> io::Result<Option<Vec<u8>>> {
    use std::os::unix::fs::MetadataExt;
    let mut opened = match open_anchored(path, || checkpoint(Checkpoint::ParentsOpened, None)) {
        Ok(v) => v,
        Err(e) if optional && e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let before = opened.file.metadata()?;
    if !before.is_file() || before.nlink() != 1 {
        return Err(invalid("design_composition_regular_unlinked_file_required"));
    }
    checkpoint(Checkpoint::FileOpened, Some(&opened.file));
    if before.len() > maximum as u64 {
        verify(path, &opened, &before)?;
        return if optional {
            Ok(None)
        } else {
            Err(invalid("design_composition_file_byte_budget"))
        };
    }
    let mut bytes = Vec::new();
    opened
        .file
        .by_ref()
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    checkpoint(Checkpoint::ReadComplete, Some(&opened.file));
    if bytes.len() > maximum || bytes.len() as u64 != before.len() {
        return Err(invalid("design_composition_changed_during_read"));
    }
    verify(path, &opened, &before)?;
    Ok(Some(bytes))
}

fn bounded(path: &Path, maximum: usize, optional: bool) -> Result<Option<Vec<u8>>, String> {
    #[cfg(unix)]
    {
        unix_bounded(path, maximum, optional, |_, _| {}).map_err(failed)
    }
    #[cfg(windows)]
    {
        use oe_windows_controller::windows::StableRoot;
        let parent = path.parent().ok_or("design_composition_parent_missing")?;
        let leaf = path
            .file_name()
            .ok_or("design_composition_file_leaf_required")?;
        let root = match StableRoot::open(parent, None) {
            Ok(v) => v,
            Err(e) if optional && e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(failed(e)),
        };
        let directory = root.as_directory().map_err(failed)?;
        // Existing platform owner caps immutable file pinning at8GiB. Source
        // bytes still enforce the caller's unchanged4MiB before any read.
        let mut file = match directory
            .pin_file(leaf, 8 * 1024 * 1024 * 1024)
            .map_err(failed)?
        {
            Some(v) => v,
            None if optional => return Ok(None),
            None => return Err("design_composition_source_missing".into()),
        };
        if file.size() > maximum as u64 {
            file.verify().map_err(failed)?;
            directory.verify().map_err(failed)?;
            return if optional {
                Ok(None)
            } else {
                Err("design_composition_file_byte_budget".into())
            };
        }
        let bytes = file.read_all(maximum as u64).map_err(failed)?;
        file.verify().map_err(failed)?;
        directory.verify().map_err(failed)?;
        Ok(Some(bytes))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, maximum, optional);
        Err("design_composition_anchored_read_unavailable".into())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{MetadataExt, symlink},
        path::PathBuf,
    };
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().canonicalize().unwrap();
        (t, p)
    }
    #[test]
    fn composition_source_anchored_parent_replacement_refuses() {
        let (_t, p) = fixture();
        let parent = p.join("inside");
        let outside = p.join("outside");
        fs::create_dir(&parent).unwrap();
        fs::create_dir(&outside).unwrap();
        let file = parent.join("source");
        fs::write(&file, b"original").unwrap();
        fs::write(outside.join("source"), b"external").unwrap();
        let original = fs::metadata(&file).unwrap().ino();
        let mut opened_inode = None;
        let result = unix_bounded(&file, 8, false, |stage, fd| {
            if stage == Checkpoint::ParentsOpened {
                fs::rename(&parent, p.join("held")).unwrap();
                symlink(&outside, &parent).unwrap();
            } else if stage == Checkpoint::FileOpened {
                opened_inode = Some(fd.unwrap().metadata().unwrap().ino());
            }
        });
        assert!(result.is_err());
        assert_eq!(opened_inode, Some(original));
        assert_eq!(fs::read(outside.join("source")).unwrap(), b"external");
    }
    #[test]
    fn composition_source_same_mtime_leaf_replacement_refuses() {
        let (_t, p) = fixture();
        let file = p.join("source");
        let replacement = p.join("replacement");
        fs::write(&file, b"original").unwrap();
        fs::write(&replacement, b"replaced").unwrap();
        let before = fs::metadata(&file).unwrap();
        let times = fs::FileTimes::new().set_modified(before.modified().unwrap());
        File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_times(times)
            .unwrap();
        assert_eq!(
            fs::metadata(&replacement).unwrap().modified().unwrap(),
            before.modified().unwrap()
        );
        let result = unix_bounded(&file, 8, false, |stage, _| {
            if stage == Checkpoint::ReadComplete {
                fs::rename(&file, p.join("held-source")).unwrap();
                fs::rename(&replacement, &file).unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&file).unwrap(), b"replaced");
    }
    #[test]
    fn composition_source_content_change_and_regular_probes_remain_bounded() {
        let (_t, p) = fixture();
        let file = p.join("source");
        fs::write(&file, b"original").unwrap();
        assert_eq!(read(&file, 8).unwrap(), b"original");
        assert!(probe(&file, 7).unwrap().is_none());
        assert!(read(&file, 7).unwrap_err().contains("byte_budget"));
        assert!(probe(&p.join("missing"), 8).unwrap().is_none());
        let old = fs::metadata(&file).unwrap().modified().unwrap();
        let result = unix_bounded(&file, 8, false, |stage, _| {
            if stage == Checkpoint::ReadComplete {
                fs::write(&file, b"changed!").unwrap();
                File::options()
                    .write(true)
                    .open(&file)
                    .unwrap()
                    .set_times(fs::FileTimes::new().set_modified(old))
                    .unwrap();
            }
        });
        assert!(result.is_err());
        let linked = p.join("linked");
        symlink(&file, &linked).unwrap();
        assert!(probe(&linked, 8).is_err());
        let hard = p.join("hard");
        fs::hard_link(&file, &hard).unwrap();
        assert!(read(&file, 8).is_err());
        assert!(read(&p, 8).is_err());
    }
}
