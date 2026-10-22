//! Native Windows primitives, with no NX, shell, semantic or state-machine engine.
use std::{
    ffi::{OsStr, OsString, c_void},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
    process::Child,
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};

const FILE_LIST_DIRECTORY: u32 = 1;
const GENERIC_READ: u32 = 0x80000000;
const GENERIC_WRITE: u32 = 0x40000000;
const DELETE_ACCESS: u32 = 0x00010000;
const SHARE_READ: u32 = 1;
const SHARE_WRITE: u32 = 2;
const SHARE_DELETE: u32 = 4;
const OPEN_EXISTING: u32 = 3;
const OPEN_ALWAYS: u32 = 4;
const CREATE_NEW: u32 = 1;
const DIRECTORY: u32 = 0x10;
const REPARSE: u32 = 0x400;
const BACKUP_SEMANTICS: u32 = 0x02000000;
const OPEN_REPARSE: u32 = 0x00200000;
const WRITE_THROUGH: u32 = 0x80000000;
const FILE_NORMAL: u32 = 0x80;
pub const LOCK_NAME: &str = ".nx-writer.lock";
pub const PENDING_NAME: &str = ".nx-state-write.pending";
pub const STATE_NAME: &str = "state.json";
// State and pending-state records share oe-nx-protocol::MAX_RECORD_BYTES.
// This is not the 100 MiB payload/file-transfer bound.
pub const RECORD_LIMIT: u64 = 4 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FileTime {
    low: u32,
    high: u32,
}
impl FileTime {
    fn ticks(self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
struct FileInformation {
    attributes: u32,
    created: FileTime,
    accessed: FileTime,
    written: FileTime,
    volume: u32,
    size_high: u32,
    size_low: u32,
    links: u32,
    index_high: u32,
    index_low: u32,
}
#[repr(C)]
struct FileIdInformation {
    volume: u64,
    id: [u8; 16],
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: RawHandle,
    ) -> RawHandle;
    fn GetFileInformationByHandle(handle: RawHandle, info: *mut FileInformation) -> i32;
    fn GetFileInformationByHandleEx(
        handle: RawHandle,
        class: i32,
        info: *mut c_void,
        size: u32,
    ) -> i32;
    fn GetFinalPathNameByHandleW(handle: RawHandle, buffer: *mut u16, size: u32, flags: u32)
    -> u32;
    fn GetFullPathNameW(
        name: *const u16,
        size: u32,
        buffer: *mut u16,
        filepart: *mut *mut u16,
    ) -> u32;
    fn SetFileInformationByHandle(
        handle: RawHandle,
        class: i32,
        info: *const c_void,
        size: u32,
    ) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> RawHandle;
    fn GetProcessTimes(
        handle: RawHandle,
        created: *mut FileTime,
        exited: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn GetProcessId(handle: RawHandle) -> u32;
    fn QueryFullProcessImageNameW(
        handle: RawHandle,
        flags: u32,
        name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn IsProcessInJob(handle: RawHandle, job: RawHandle, result: *mut i32) -> i32;
    fn GetCurrentProcess() -> RawHandle;
    fn DuplicateHandle(
        source_process: RawHandle,
        source: RawHandle,
        target_process: RawHandle,
        target: *mut RawHandle,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn WaitForSingleObject(handle: RawHandle, wait_ms: u32) -> u32;
    fn GetExitCodeProcess(handle: RawHandle, code: *mut u32) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        key: RawHandle,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}
fn changed(reason: &'static str) -> io::Error {
    io::Error::other(reason)
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut v: Vec<_> = value.encode_wide().collect();
    if v.contains(&0) || v.len() > 32766 {
        return Err(invalid("invalid_windows_path"));
    }
    v.push(0);
    Ok(v)
}
fn open(
    path: &Path,
    access: u32,
    share: u32,
    creation: u32,
    flags: u32,
) -> io::Result<OwnedHandle> {
    let name = wide(path.as_os_str())?;
    // Null security attributes explicitly prohibit child-handle inheritance.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            share,
            null(),
            creation,
            flags,
            null_mut(),
        )
    };
    if handle.is_null() || handle as isize == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}
fn info(handle: &OwnedHandle) -> io::Result<FileInformation> {
    let mut v = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut v) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(v)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    pub volume_id: String,
    pub file_id: String,
}
fn file_identity(handle: &OwnedHandle) -> io::Result<FileIdentity> {
    let mut v: FileIdInformation = unsafe { zeroed() };
    if unsafe {
        GetFileInformationByHandleEx(
            handle.as_raw_handle(),
            18,
            &mut v as *mut _ as *mut c_void,
            size_of::<FileIdInformation>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(FileIdentity {
        volume_id: format!("{:016x}", v.volume),
        file_id: v.id.iter().map(|b| format!("{b:02x}")).collect(),
    })
}
fn final_path(handle: &OwnedHandle) -> io::Result<PathBuf> {
    let mut buf = vec![0u16; 32768];
    let n = unsafe {
        GetFinalPathNameByHandleW(
            handle.as_raw_handle(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            0,
        )
    } as usize;
    if n == 0 {
        return Err(io::Error::last_os_error());
    }
    if n >= buf.len() {
        return Err(invalid("final_path_limit"));
    }
    Ok(PathBuf::from(OsString::from_wide(&buf[..n])))
}
fn normalized_local_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid("absolute_stable_root_required"));
    }
    match path.components().next() {
        Some(Component::Prefix(p))
            if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) => {}
        _ => return Err(invalid("local_disk_root_required")),
    }
    let name = wide(path.as_os_str())?;
    let mut buf = vec![0u16; 32768];
    let n = unsafe {
        GetFullPathNameW(
            name.as_ptr(),
            buf.len() as u32,
            buf.as_mut_ptr(),
            null_mut(),
        )
    } as usize;
    if n == 0 {
        return Err(io::Error::last_os_error());
    }
    if n >= buf.len() {
        return Err(invalid("full_path_limit"));
    }
    let normalized = PathBuf::from(OsString::from_wide(&buf[..n]));
    if normalized
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid("non_normalized_root"));
    }
    Ok(normalized)
}
pub fn host_identity() -> io::Result<String> {
    let key = wide(OsStr::new("SOFTWARE\\Microsoft\\Cryptography"))?;
    let value = wide(OsStr::new("MachineGuid"))?;
    let mut buf = [0u16; 256];
    let mut bytes = (buf.len() * 2) as u32;
    let mut kind = 0;
    let result = unsafe {
        RegGetValueW(
            (-2147483646isize) as RawHandle,
            key.as_ptr(),
            value.as_ptr(),
            2,
            &mut kind,
            buf.as_mut_ptr() as *mut c_void,
            &mut bytes,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result));
    }
    if kind != 1 || bytes < 2 || bytes as usize > buf.len() * 2 || !bytes.is_multiple_of(2) {
        return Err(invalid("machine_guid_invalid"));
    }
    let n = (bytes as usize / 2) - 1;
    if buf[n] != 0 {
        return Err(invalid("machine_guid_unterminated"));
    }
    let text = String::from_utf16(&buf[..n])
        .map_err(|_| invalid("machine_guid_encoding"))?
        .to_ascii_lowercase();
    if text.len() != 36 || !text.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(invalid("machine_guid_invalid"));
    }
    Ok(format!("windows-machine-guid:{text}"))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootIdentity {
    pub host_id: String,
    pub volume_id: String,
    pub directory_id: String,
}
pub struct StableRoot {
    path: PathBuf,
    identity: RootIdentity,
    ancestors: Vec<OwnedHandle>,
}
impl StableRoot {
    pub fn open(path: &Path, expected: Option<&RootIdentity>) -> io::Result<Self> {
        let normalized = normalized_local_path(path)?;
        let mut current = PathBuf::new();
        let mut ancestors = Vec::new();
        for component in normalized.components() {
            current.push(component.as_os_str());
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            let handle = open(
                &current,
                FILE_LIST_DIRECTORY,
                SHARE_READ | SHARE_WRITE,
                OPEN_EXISTING,
                BACKUP_SEMANTICS | OPEN_REPARSE,
            )?;
            let metadata = info(&handle)?;
            if metadata.attributes & DIRECTORY == 0 || metadata.attributes & REPARSE != 0 {
                return Err(invalid("root_reparse_or_nondirectory"));
            }
            ancestors.push(handle);
        }
        let handle = ancestors
            .last()
            .ok_or_else(|| invalid("root_directory_required"))?;
        let file = file_identity(handle)?;
        let path = final_path(handle)?;
        let identity = RootIdentity {
            host_id: host_identity()?,
            volume_id: file.volume_id,
            directory_id: file.file_id,
        };
        if expected.is_some_and(|v| v != &identity) {
            return Err(changed("stable_root_identity_changed"));
        }
        Ok(Self {
            path,
            identity,
            ancestors,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn identity(&self) -> &RootIdentity {
        &self.identity
    }
    pub fn verify(&self) -> io::Result<()> {
        let h = self
            .ancestors
            .last()
            .ok_or_else(|| invalid("root_unpinned"))?;
        let id = file_identity(h)?;
        if id.volume_id != self.identity.volume_id
            || id.file_id != self.identity.directory_id
            || final_path(h)? != self.path
        {
            return Err(changed("stable_root_changed"));
        }
        Ok(())
    }
    pub fn lock(&self) -> io::Result<WriterLock<'_>> {
        self.verify()?;
        let handle = open(
            &self.path.join(LOCK_NAME),
            GENERIC_READ | GENERIC_WRITE,
            0,
            OPEN_ALWAYS,
            FILE_NORMAL | OPEN_REPARSE | WRITE_THROUGH,
        )?;
        let metadata = info(&handle)?;
        if metadata.attributes & (REPARSE | DIRECTORY) != 0 || metadata.links != 1 {
            return Err(invalid("writer_lock_not_regular_single_link"));
        }
        let identity = file_identity(&handle)?;
        Ok(WriterLock {
            root: self,
            handle,
            identity,
        })
    }
    /// View this already-pinned root as a directory without traversing a child.
    pub fn as_directory(&self) -> io::Result<PinnedDirectory<'_>> {
        self.verify()?;
        let handle = self
            .ancestors
            .last()
            .ok_or_else(|| invalid("root_unpinned"))?
            .try_clone()?;
        Ok(PinnedDirectory {
            root: self,
            path: self.path.clone(),
            identity: file_identity(&handle)?,
            handle,
            parent_pins: Vec::new(),
        })
    }
    pub fn create_directory_new(&self, name: &OsStr) -> io::Result<PinnedDirectory<'_>> {
        validate_leaf(name)?;
        self.verify()?;
        std::fs::create_dir(self.path.join(name))?;
        self.directory(name)
    }
    pub fn directory(&self, name: &OsStr) -> io::Result<PinnedDirectory<'_>> {
        validate_leaf(name)?;
        self.verify()?;
        let path = self.path.join(name);
        let handle = open(
            &path,
            FILE_LIST_DIRECTORY,
            SHARE_READ | SHARE_WRITE,
            OPEN_EXISTING,
            BACKUP_SEMANTICS | OPEN_REPARSE,
        )?;
        let metadata = info(&handle)?;
        if metadata.attributes & DIRECTORY == 0 || metadata.attributes & REPARSE != 0 {
            return Err(invalid("job_reparse_or_nondirectory"));
        }
        let identity = file_identity(&handle)?;
        let path = final_path(&handle)?;
        Ok(PinnedDirectory {
            root: self,
            path,
            handle,
            identity,
            parent_pins: Vec::new(),
        })
    }
}
fn validate_leaf(name: &OsStr) -> io::Result<()> {
    let value = name
        .to_str()
        .ok_or_else(|| invalid("filename_unicode_required"))?;
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
    {
        return Err(invalid("single_regular_filename_required"));
    }
    let stem = value.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit())
    {
        return Err(invalid("reserved_windows_name"));
    }
    Ok(())
}

/// Reuse the pinning owner's exact safe-name policy for lexical source paths.
/// This validates a component only; it grants no filesystem access or identity.
pub fn validate_source_component(name: &OsStr) -> io::Result<()> {
    validate_leaf(name)
}

/// Create missing normal directories while retaining every traversed parent.
/// Existing files/reparse points are never adopted. Later I/O must pin again.
pub fn ensure_directory(path: &Path) -> io::Result<()> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(invalid("parent_traversal"));
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let absolute = normalized_local_path(&absolute)?;
    let mut ancestor = absolute.as_path();
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let name = ancestor
                    .file_name()
                    .ok_or_else(|| invalid("directory_creation_name_missing"))?;
                validate_leaf(name)?;
                missing.push(name.to_owned());
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| invalid("directory_creation_parent_missing"))?;
            }
            Err(e) => return Err(e),
        }
    }
    let root = StableRoot::open(ancestor, None)?;
    let mut directory = root.as_directory()?;
    for name in missing.into_iter().rev() {
        directory = match directory.directory(&name) {
            Ok(existing) => existing,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                match directory.create_directory_new(&name) {
                    Ok(created) => created,
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                        directory.directory(&name)?
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        };
        directory.verify()?;
    }
    let current = StableRoot::open(&absolute, None)?;
    if current.identity().volume_id != directory.identity().volume_id
        || current.identity().directory_id != directory.identity().file_id
    {
        return Err(changed("directory_path_changed"));
    }
    directory.verify()
}

/// One same-volume directory publication. Reuse pinned ancestor identities;
/// FileRenameInfo's ReplaceIfExists is FALSE, including a concurrent target.
/// https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info
pub fn rename_directory_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    if size_of::<usize>() != 8 {
        return Err(invalid("directory_publish_requires_64_bit_windows"));
    }
    let source_parent = StableRoot::open(
        from.parent()
            .ok_or_else(|| invalid("rename_parent_missing"))?,
        None,
    )?;
    let target_parent = StableRoot::open(
        to.parent()
            .ok_or_else(|| invalid("rename_parent_missing"))?,
        None,
    )?;
    let source_name = from
        .file_name()
        .ok_or_else(|| invalid("rename_source_name_missing"))?;
    let target_name = to
        .file_name()
        .ok_or_else(|| invalid("rename_target_name_missing"))?;
    validate_leaf(source_name)?;
    validate_leaf(target_name)?;
    let target = target_parent.path().join(target_name);
    match std::fs::symlink_metadata(&target) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "directory_publish_target_exists",
            ));
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let source = open(
        &source_parent.path().join(source_name),
        FILE_LIST_DIRECTORY | DELETE_ACCESS,
        SHARE_READ | SHARE_WRITE,
        OPEN_EXISTING,
        BACKUP_SEMANTICS | OPEN_REPARSE,
    )?;
    let metadata = info(&source)?;
    if metadata.attributes & DIRECTORY == 0 || metadata.attributes & REPARSE != 0 {
        return Err(invalid("rename_source_not_normal_directory"));
    }
    let identity = file_identity(&source)?;
    if identity.volume_id != target_parent.identity().volume_id {
        return Err(invalid("directory_publish_cross_volume_refused"));
    }
    source_parent.verify()?;
    target_parent.verify()?;
    let name: Vec<u16> = target.as_os_str().encode_wide().chain([0]).collect();
    let length = 20 + name.len() * 2;
    let mut buffer = vec![0u64; length.div_ceil(8)];
    // x64 FILE_RENAME_INFO: union at 0, RootDirectory at 8, length at 16,
    // UTF-16 name at 20. The zeroed first byte means never replace a target.
    let raw = unsafe {
        std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), buffer.len() * 8)
    };
    raw[16..20].copy_from_slice(&((name.len() - 1) as u32 * 2).to_le_bytes());
    for (index, unit) in name.into_iter().enumerate() {
        raw[20 + index * 2..22 + index * 2].copy_from_slice(&unit.to_le_bytes());
    }
    // SAFETY: the owned directory handle has DELETE access; the aligned buffer
    // includes a bounded FILE_RENAME_INFO plus its UTF-16 name, both pinned
    // parents stay alive and cannot be renamed throughout the operation.
    if unsafe {
        SetFileInformationByHandle(
            source.as_raw_handle(),
            3,
            buffer.as_ptr().cast::<c_void>(),
            length as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if file_identity(&source)? != identity || final_path(&source)? != target {
        return Err(changed("directory_publish_after_rename_identity_changed"));
    }
    source_parent.verify()?;
    target_parent.verify()?;
    Ok(())
}
pub struct WriterLock<'a> {
    root: &'a StableRoot,
    handle: OwnedHandle,
    identity: FileIdentity,
}
impl WriterLock<'_> {
    pub fn root(&self) -> &StableRoot {
        self.root
    }
    pub fn verify(&self) -> io::Result<()> {
        self.root.verify()?;
        if file_identity(&self.handle)? != self.identity {
            return Err(changed("writer_lock_identity_changed"));
        }
        Ok(())
    }
}
pub struct PinnedDirectory<'a> {
    root: &'a StableRoot,
    path: PathBuf,
    handle: OwnedHandle,
    identity: FileIdentity,
    parent_pins: Vec<OwnedHandle>,
}
impl<'a> PinnedDirectory<'a> {
    /// Clone the retained identity and ancestor handles without reopening a path.
    pub fn retained_view(&self) -> io::Result<Self> {
        self.verify()?;
        Ok(Self {
            root: self.root,
            path: self.path.clone(),
            handle: self.handle.try_clone()?,
            identity: self.identity.clone(),
            parent_pins: self
                .parent_pins
                .iter()
                .map(OwnedHandle::try_clone)
                .collect::<io::Result<_>>()?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn identity(&self) -> &FileIdentity {
        &self.identity
    }
    pub fn verify(&self) -> io::Result<()> {
        self.root.verify()?;
        if file_identity(&self.handle)? != self.identity || final_path(&self.handle)? != self.path {
            return Err(changed("job_directory_changed"));
        }
        Ok(())
    }
    pub fn directory(&self, name: &OsStr) -> io::Result<PinnedDirectory<'a>> {
        validate_leaf(name)?;
        self.verify()?;
        let handle = open(
            &self.path.join(name),
            FILE_LIST_DIRECTORY,
            SHARE_READ | SHARE_WRITE,
            OPEN_EXISTING,
            BACKUP_SEMANTICS | OPEN_REPARSE,
        )?;
        let metadata = info(&handle)?;
        if metadata.attributes & DIRECTORY == 0 || metadata.attributes & REPARSE != 0 {
            return Err(invalid("nested_reparse_or_nondirectory"));
        }
        let mut parent_pins = self
            .parent_pins
            .iter()
            .map(OwnedHandle::try_clone)
            .collect::<io::Result<Vec<_>>>()?;
        parent_pins.push(self.handle.try_clone()?);
        Ok(PinnedDirectory {
            root: self.root,
            path: final_path(&handle)?,
            identity: file_identity(&handle)?,
            handle,
            parent_pins,
        })
    }
    pub fn create_directory_new(&self, name: &OsStr) -> io::Result<PinnedDirectory<'a>> {
        validate_leaf(name)?;
        self.verify()?;
        std::fs::create_dir(self.path.join(name))?;
        self.directory(name)
    }
    /// Normal single-link file, no reparse traversal; held READ share denies
    /// concurrent writers/deletion. The explicit bound is for this file class.
    pub fn pin_file(&self, name: &OsStr, max_bytes: u64) -> io::Result<Option<PinnedFile>> {
        validate_leaf(name)?;
        self.verify()?;
        if max_bytes == 0 || max_bytes > 8 * 1024 * 1024 * 1024 {
            return Err(invalid("explicit_file_bound_required"));
        }
        let path = self.path.join(name);
        let handle = match open(&path, GENERIC_READ, SHARE_READ, OPEN_EXISTING, OPEN_REPARSE) {
            Ok(h) => h,
            Err(e) if e.raw_os_error() == Some(2) => return Ok(None),
            Err(e) => return Err(e),
        };
        let before = info(&handle)?;
        let identity = file_identity(&handle)?;
        let size = (u64::from(before.size_high) << 32) | u64::from(before.size_low);
        if before.attributes & (REPARSE | DIRECTORY) != 0 || before.links != 1 || size > max_bytes {
            return Err(invalid("file_not_bounded_regular_single_link"));
        }
        let path = final_path(&handle)?;
        Ok(Some(PinnedFile {
            _directory_pins: self
                .root
                .ancestors
                .iter()
                .chain(self.parent_pins.iter())
                .chain(std::iter::once(&self.handle))
                .map(OwnedHandle::try_clone)
                .collect::<io::Result<Vec<_>>>()?,
            file: File::from(handle),
            path,
            identity,
            size,
            written: before.written,
        }))
    }
    /// Snapshot of an already-opened record. SHARE_DELETE permits the writer's
    /// atomic rename while this handle still reads the previous complete bytes.
    /// This is deliberately different from immutable payload/runtime pinning.
    pub fn read_record(&self, name: &OsStr) -> io::Result<Option<Vec<u8>>> {
        self.read_record_with_checkpoint(name, || Ok(()))
    }
    pub fn read_record_with_checkpoint<F: FnOnce() -> io::Result<()>>(
        &self,
        name: &OsStr,
        after_open: F,
    ) -> io::Result<Option<Vec<u8>>> {
        validate_leaf(name)?;
        self.verify()?;
        let handle = match open(
            &self.path.join(name),
            GENERIC_READ,
            SHARE_READ | SHARE_DELETE,
            OPEN_EXISTING,
            OPEN_REPARSE,
        ) {
            Ok(h) => h,
            Err(e) if e.raw_os_error() == Some(2) => return Ok(None),
            Err(e) => return Err(e),
        };
        let before = info(&handle)?;
        let identity = file_identity(&handle)?;
        let size = (u64::from(before.size_high) << 32) | u64::from(before.size_low);
        if before.attributes & (REPARSE | DIRECTORY) != 0
            || before.links != 1
            || size > RECORD_LIMIT
        {
            return Err(invalid("record_not_bounded_regular_single_link"));
        }
        after_open().map_err(|e| io::Error::other(format!("snapshot_checkpoint:{e}")))?;
        let file = File::from(handle);
        let mut bytes = Vec::new();
        (&file).take(RECORD_LIMIT + 1).read_to_end(&mut bytes)?;
        let handle: OwnedHandle = file.into();
        let after = info(&handle)?;
        if file_identity(&handle)? != identity
            || before.written != after.written
            || before.size_high != after.size_high
            || before.size_low != after.size_low
            || bytes.len() as u64 != size
            || after.links > 1
        {
            return Err(changed("record_changed_during_snapshot"));
        }
        self.verify()?;
        Ok(Some(bytes))
    }
    /// Never replaces an existing output; a failed write remains visible and
    /// cannot be retried as though no bytes were published.
    pub fn create_log_new(&self, name: &OsStr) -> io::Result<File> {
        validate_leaf(name)?;
        self.verify()?;
        let handle = open(
            &self.path.join(name),
            GENERIC_WRITE,
            SHARE_READ,
            CREATE_NEW,
            FILE_NORMAL | OPEN_REPARSE | WRITE_THROUGH,
        )?;
        let metadata = info(&handle)?;
        if metadata.attributes & (REPARSE | DIRECTORY) != 0 || metadata.links != 1 {
            return Err(invalid("log_not_regular_single_link"));
        }
        Ok(File::from(handle))
    }
    /// Identity only for an explicitly task-owned mutable native copy. Read
    /// sharing admits the native writer; original PinnedFile stays read-only.
    /// Never claims unchanged content while the native application holds it.
    pub fn native_mutable_file_identity(
        &self,
        name: &OsStr,
        max_bytes: u64,
    ) -> io::Result<FileIdentity> {
        validate_leaf(name)?;
        self.verify()?;
        let handle = open(
            &self.path.join(name),
            0x80,
            SHARE_READ | SHARE_WRITE,
            OPEN_EXISTING,
            FILE_NORMAL | OPEN_REPARSE,
        )?;
        let metadata = info(&handle)?;
        let size = (u64::from(metadata.size_high) << 32) | u64::from(metadata.size_low);
        if metadata.attributes & (REPARSE | DIRECTORY) != 0
            || metadata.links != 1
            || size == 0
            || size > max_bytes
        {
            return Err(invalid("native_mutable_file_identity_bound"));
        }
        let identity = file_identity(&handle)?;
        self.verify()?;
        Ok(identity)
    }
    /// Exclusive, bounded streaming output. Partial writes remain for inspection.
    pub fn create_stream_new<F: FnOnce(&mut dyn Write) -> io::Result<()>>(
        &self,
        name: &OsStr,
        expected_bytes: u64,
        write: F,
    ) -> io::Result<()> {
        validate_leaf(name)?;
        self.verify()?;
        if expected_bytes > 8 * 1024 * 1024 * 1024 {
            return Err(invalid("stream_file_bound"));
        }
        let handle = open(
            &self.path.join(name),
            GENERIC_READ | GENERIC_WRITE,
            0,
            CREATE_NEW,
            FILE_NORMAL | OPEN_REPARSE | WRITE_THROUGH,
        )?;
        let before = info(&handle)?;
        if before.attributes & (REPARSE | DIRECTORY) != 0 || before.links != 1 {
            return Err(invalid("stream_not_regular_single_link"));
        }
        let identity = file_identity(&handle)?;
        let mut file = File::from(handle);
        struct Bounded<'a> {
            file: &'a mut File,
            remaining: u64,
        }
        impl Write for Bounded<'_> {
            fn write(&mut self, data: &[u8]) -> io::Result<usize> {
                if data.len() as u64 > self.remaining {
                    return Err(invalid("stream_exceeds_expected_size"));
                }
                let n = self.file.write(data)?;
                self.remaining -= n as u64;
                Ok(n)
            }
            fn flush(&mut self) -> io::Result<()> {
                self.file.flush()
            }
        }
        let mut bounded = Bounded {
            file: &mut file,
            remaining: expected_bytes,
        };
        write(&mut bounded)?;
        if bounded.remaining != 0 {
            return Err(invalid("stream_incomplete"));
        }
        file.sync_all()?;
        let handle: OwnedHandle = file.into();
        let after = info(&handle)?;
        if file_identity(&handle)? != identity
            || after.links != 1
            || after.attributes & (REPARSE | DIRECTORY) != 0
            || ((u64::from(after.size_high) << 32) | u64::from(after.size_low)) != expected_bytes
        {
            return Err(changed("stream_output_changed"));
        }
        self.verify()
    }
    pub fn create_file_new(&self, name: &OsStr, bytes: &[u8], max_bytes: u64) -> io::Result<()> {
        validate_leaf(name)?;
        self.verify()?;
        if max_bytes == 0 || bytes.len() as u64 > max_bytes {
            return Err(invalid("output_size_limit"));
        }
        let handle = open(
            &self.path.join(name),
            GENERIC_READ | GENERIC_WRITE,
            0,
            CREATE_NEW,
            FILE_NORMAL | OPEN_REPARSE | WRITE_THROUGH,
        )?;
        let metadata = info(&handle)?;
        if metadata.attributes & (REPARSE | DIRECTORY) != 0 || metadata.links != 1 {
            return Err(invalid("output_not_regular_single_link"));
        }
        let identity = file_identity(&handle)?;
        let mut file = File::from(handle);
        file.write_all(bytes)?;
        file.sync_all()?;
        let handle: OwnedHandle = file.into();
        if file_identity(&handle)? != identity {
            return Err(changed("new_output_identity_changed"));
        }
        self.verify()
    }
}
pub struct PinnedFile {
    _directory_pins: Vec<OwnedHandle>,
    file: File,
    path: PathBuf,
    identity: FileIdentity,
    size: u64,
    written: FileTime,
}
impl PinnedFile {
    pub fn identity(&self) -> &FileIdentity {
        &self.identity
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn verify(&self) -> io::Result<()> {
        let handle: OwnedHandle = self.file.try_clone()?.into();
        let after = info(&handle)?;
        let size = (u64::from(after.size_high) << 32) | u64::from(after.size_low);
        if self.identity != file_identity(&handle)?
            || self.size != size
            || self.written != after.written
            || after.attributes & (REPARSE | DIRECTORY) != 0
            || after.links != 1
            || final_path(&handle)? != self.path
        {
            return Err(changed("pinned_file_changed"));
        }
        Ok(())
    }
    pub fn read_all(&mut self, max_bytes: u64) -> io::Result<Vec<u8>> {
        self.verify()?;
        if self.size > max_bytes || max_bytes > 8 * 1024 * 1024 * 1024 {
            return Err(invalid("file_read_bound"));
        }
        self.file.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        (&self.file).take(max_bytes + 1).read_to_end(&mut data)?;
        self.verify()?;
        if data.len() as u64 != self.size {
            return Err(changed("pinned_read_size_changed"));
        }
        Ok(data)
    }
    /// Borrowed reader for bounded streaming hashing; caller must call verify()
    /// after reaching EOF and before using the computed identity.
    pub fn reader(&mut self) -> io::Result<&mut File> {
        self.verify()?;
        self.file.seek(SeekFrom::Start(0))?;
        Ok(&mut self.file)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteCheckpoint {
    PendingSynced,
    TemporarySynced,
    ReplacedAndReadback,
}
/// Raw-byte persistence only. The caller must use the shared protocol transition.
/// Any incomplete update retains PENDING_NAME, and scanners must report it as hold.
pub fn write_state_atomic(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    expected: Option<&[u8]>,
    bytes: &[u8],
) -> io::Result<()> {
    write_state_with_checkpoint(dir, lock, expected, bytes, |_| Ok(()))
}
pub fn write_state_with_checkpoint<F: FnMut(WriteCheckpoint) -> io::Result<()>>(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    expected: Option<&[u8]>,
    bytes: &[u8],
    checkpoint: F,
) -> io::Result<()> {
    write_state_mode(dir, lock, expected, bytes, false, checkpoint)
}
/// Explicit recovery of a complete pending first write; no replacement of an
/// unrelated state or partial pending bytes. The owner revalidates its inputs.
pub fn recover_state_new(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    bytes: &[u8],
) -> io::Result<()> {
    write_state_mode(dir, lock, None, bytes, true, |_| Ok(()))
}
fn write_state_mode<F: FnMut(WriteCheckpoint) -> io::Result<()>>(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    expected: Option<&[u8]>,
    bytes: &[u8],
    resume: bool,
    mut checkpoint: F,
) -> io::Result<()> {
    lock.verify()?;
    dir.verify()?;
    if !std::ptr::eq(dir.root, lock.root) {
        return Err(invalid("writer_lock_root_mismatch"));
    }
    if bytes.is_empty() || bytes.len() as u64 > RECORD_LIMIT {
        return Err(invalid("record_size_limit"));
    }
    let current = dir.read_record(OsStr::new(STATE_NAME))?;
    let pending = dir.path.join(PENDING_NAME);
    if resume {
        let mut pin = dir
            .pin_file(OsStr::new(PENDING_NAME), RECORD_LIMIT)?
            .ok_or_else(|| invalid("recovery_pending_missing"))?;
        if pin.read_all(RECORD_LIMIT)? != bytes {
            return Err(changed("recovery_pending_differs"));
        }
        pin.verify()?;
        drop(pin);
        if current.as_deref() == Some(bytes) {
            lock.verify()?;
            dir.verify()?;
            std::fs::remove_file(&pending)?;
            if dir.read_record(OsStr::new(STATE_NAME))?.as_deref() != Some(bytes) {
                return Err(changed("recovery_state_changed"));
            }
            return Ok(());
        }
        if current.is_some() || expected.is_some() {
            return Err(changed("recovery_unrelated_state"));
        }
    } else {
        if current.as_deref() != expected {
            return Err(changed("state_compare_before_write_failed"));
        }
        let handle = open(
            &pending,
            GENERIC_READ | GENERIC_WRITE,
            0,
            CREATE_NEW,
            FILE_NORMAL | OPEN_REPARSE | WRITE_THROUGH,
        )?;
        let mut file = File::from(handle);
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
    }
    checkpoint(WriteCheckpoint::PendingSynced)?;
    let name = format!(
        ".nx-state-{}-{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let temp = dir.path.join(name);
    let handle = open(
        &temp,
        GENERIC_READ | GENERIC_WRITE | DELETE_ACCESS,
        0,
        CREATE_NEW,
        FILE_NORMAL | OPEN_REPARSE | WRITE_THROUGH,
    )?;
    let mut file = File::from(handle);
    file.write_all(bytes)?;
    file.sync_all()?;
    let handle: OwnedHandle = file.into();
    checkpoint(WriteCheckpoint::TemporarySynced)?;
    lock.verify()?;
    dir.verify()?;
    if dir.read_record(OsStr::new(STATE_NAME))?.as_deref() != expected {
        return Err(changed("state_changed_before_replace"));
    }
    // Windows 10+ POSIX replacement semantics keep a status reader's old
    // handle valid without forcing the writer into UNKNOWN. The new record
    // was flushed first; this is one same-volume namespace replacement.
    let target_path = dir.path.join(STATE_NAME);
    let name: Vec<u16> = target_path.as_os_str().encode_wide().chain([0]).collect();
    let length = 20 + name.len() * 2;
    let mut buffer = vec![0u64; length.div_ceil(8)];
    let rename_bytes =
        unsafe { std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, buffer.len() * 8) };
    rename_bytes[0..4].copy_from_slice(&3u32.to_le_bytes()); // REPLACE_IF_EXISTS | POSIX_SEMANTICS
    // SetFileInformationByHandle resolves this full DOS path; RootDirectory NULL.
    rename_bytes[8..16].copy_from_slice(&0u64.to_le_bytes());
    rename_bytes[16..20].copy_from_slice(&((name.len() - 1) as u32 * 2).to_le_bytes());
    for (index, unit) in name.iter().enumerate() {
        rename_bytes[20 + index * 2..22 + index * 2].copy_from_slice(&unit.to_le_bytes());
    }
    if unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle(),
            22,
            buffer.as_ptr() as *const c_void,
            length as u32,
        )
    } == 0
    {
        return Err(io::Error::other(format!(
            "atomic_FileRenameInfoEx:{}",
            io::Error::last_os_error()
        )));
    }
    let file = File::from(handle);
    file.sync_all()?;
    drop(file);
    if dir.read_record(OsStr::new(STATE_NAME))?.as_deref() != Some(bytes) {
        return Err(changed("state_replacement_readback_failed"));
    }
    checkpoint(WriteCheckpoint::ReplacedAndReadback)?;
    lock.verify()?;
    dir.verify()?;
    std::fs::remove_file(&pending)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub creation_time_100ns: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessObservation {
    Running,
    Exited { exit_code: u32 },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoricalProcessObservation {
    Absent,
    DifferentCreation(ProcessIdentity),
    LiveSameIdentity,
    ExitedSameIdentity { exit_code: u32 },
}
pub struct ProcessWatch {
    handle: OwnedHandle,
    identity: ProcessIdentity,
}
fn process_identity(handle: &OwnedHandle) -> io::Result<ProcessIdentity> {
    let pid = unsafe { GetProcessId(handle.as_raw_handle()) };
    if pid == 0 {
        return Err(io::Error::last_os_error());
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    if unsafe {
        GetProcessTimes(
            handle.as_raw_handle(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if created.ticks() == 0 {
        return Err(changed("process_creation_time_missing"));
    }
    Ok(ProcessIdentity {
        pid,
        creation_time_100ns: created.ticks(),
    })
}
impl ProcessWatch {
    fn open_pid_instance(pid: u32) -> io::Result<Self> {
        let raw = unsafe { OpenProcess(0x00100000 | 0x1000, 0, pid) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let identity = process_identity(&handle)?;
        Ok(Self { handle, identity })
    }
    /// Only for a just-returned native creation result; never use to reconcile
    /// an old job by a PID without its persisted creation time.
    pub fn open_observed_pid(pid: u32) -> io::Result<Self> {
        Self::open_pid_instance(pid)
    }

    /// Observe only the persisted historical identity, never dispatch or infer
    /// its former exit code from a reused PID. Keep an actual handle while
    /// comparing creation time and inspecting termination.
    pub fn observe_historical(
        expected: ProcessIdentity,
    ) -> io::Result<HistoricalProcessObservation> {
        if expected.pid == 0 || expected.creation_time_100ns == 0 {
            return Err(invalid("historical_process_identity_missing"));
        }
        let actual = match Self::open_pid_instance(expected.pid) {
            Ok(watch) => watch,
            Err(error) if error.raw_os_error() == Some(87) => {
                return Ok(HistoricalProcessObservation::Absent);
            }
            Err(error) => return Err(error),
        };
        if actual.identity.creation_time_100ns != expected.creation_time_100ns {
            return Ok(HistoricalProcessObservation::DifferentCreation(
                actual.identity,
            ));
        }
        match actual.poll()? {
            ProcessObservation::Running => Ok(HistoricalProcessObservation::LiveSameIdentity),
            ProcessObservation::Exited { exit_code } => {
                Ok(HistoricalProcessObservation::ExitedSameIdentity { exit_code })
            }
        }
    }

    pub fn current() -> io::Result<Self> {
        let mut duplicate = null_mut();
        let current = unsafe { GetCurrentProcess() };
        if unsafe { DuplicateHandle(current, current, current, &mut duplicate, 0, 0, 2) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(duplicate) };
        let identity = process_identity(&handle)?;
        Ok(Self { handle, identity })
    }
    pub fn from_child(child: &Child) -> io::Result<Self> {
        let mut duplicate = null_mut();
        let current = unsafe { GetCurrentProcess() };
        if unsafe {
            DuplicateHandle(
                current,
                child.as_raw_handle(),
                current,
                &mut duplicate,
                0,
                0,
                2,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(duplicate) };
        let identity = process_identity(&handle)?;
        if identity.pid != child.id() {
            return Err(changed("spawned_process_identity_mismatch"));
        }
        Ok(Self { handle, identity })
    }
    /// Retain an OS handle returned by an owned CreateProcess invocation.
    /// The caller owns the source handle; this watch owns only its duplicate.
    pub fn from_owned_handle(source: &OwnedHandle) -> io::Result<Self> {
        let mut duplicate = null_mut();
        let current = unsafe { GetCurrentProcess() };
        if unsafe {
            DuplicateHandle(
                current,
                source.as_raw_handle(),
                current,
                &mut duplicate,
                0,
                0,
                2,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(duplicate) };
        let identity = process_identity(&handle)?;
        Ok(Self { handle, identity })
    }
    pub fn open(expected: ProcessIdentity) -> io::Result<Self> {
        let raw = unsafe { OpenProcess(0x00100000 | 0x1000, 0, expected.pid) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let identity = process_identity(&handle)?;
        if identity != expected {
            return Err(changed("pid_creation_time_mismatch"));
        }
        Ok(Self { handle, identity })
    }
    pub fn image_path(&self) -> io::Result<PathBuf> {
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe {
            QueryFullProcessImageNameW(
                self.handle.as_raw_handle(),
                0,
                buffer.as_mut_ptr(),
                &mut length,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if length == 0 || length as usize >= buffer.len() {
            return Err(invalid("process_image_path_bound"));
        }
        Ok(PathBuf::from(OsString::from_wide(
            &buffer[..length as usize],
        )))
    }
    pub fn in_job(&self) -> io::Result<bool> {
        let mut result = 0;
        if unsafe { IsProcessInJob(self.handle.as_raw_handle(), null_mut(), &mut result) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result != 0)
    }
    pub fn identity(&self) -> ProcessIdentity {
        self.identity
    }
    pub fn poll(&self) -> io::Result<ProcessObservation> {
        if process_identity(&self.handle)? != self.identity {
            return Err(changed("process_handle_identity_changed"));
        }
        match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
            258 => Ok(ProcessObservation::Running),
            0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(ProcessObservation::Exited { exit_code: code })
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
}

/// Facts only; hold adjudication is delegated to the shared protocol crate.
pub struct DirectoryRecords {
    pub name: OsString,
    pub identity: Option<FileIdentity>,
    pub state: Option<Vec<u8>>,
    pub pending: Option<Vec<u8>>,
    pub unexpected_control_files: Vec<OsString>,
    pub error: Option<String>,
}
pub fn observe_job_records(root: &StableRoot, name: &OsStr) -> DirectoryRecords {
    let mut record = DirectoryRecords {
        name: name.to_owned(),
        identity: None,
        state: None,
        pending: None,
        unexpected_control_files: Vec::new(),
        error: None,
    };
    let result = (|| -> io::Result<()> {
        let dir = root.directory(name)?;
        record.identity = Some(dir.identity().clone());
        record.pending = dir.read_record(OsStr::new(PENDING_NAME))?;
        record.state = dir.read_record(OsStr::new(STATE_NAME))?;
        for child in std::fs::read_dir(dir.path())? {
            let n = child?.file_name();
            let Some(s) = n.to_str() else {
                return Err(invalid("job_filename_encoding"));
            };
            if s.starts_with(".nx-state-") && s.ends_with(".tmp") {
                record.unexpected_control_files.push(n)
            }
        }
        // State replacement can occur after the first pending observation.
        // Retain either observed marker; a sharing violation is also uncertain
        // and becomes a hold through the ordinary observation error path.
        if let Some(pending_after) = dir.read_record(OsStr::new(PENDING_NAME))? {
            record.pending = Some(pending_after);
        }
        dir.verify()
    })();
    if let Err(e) = result {
        record.error = Some(format!(
            "record_observation_failed:{:?}:{:?}",
            e.kind(),
            e.raw_os_error()
        ))
    }
    record
}
pub fn scan_root_records(root: &StableRoot) -> io::Result<Vec<DirectoryRecords>> {
    root.verify()?;
    let mut records = Vec::new();
    for entry in std::fs::read_dir(root.path())? {
        let entry = entry?;
        let name = entry.file_name();
        if name == OsStr::new(LOCK_NAME) {
            continue;
        }
        records.push(observe_job_records(root, &name));
        if records.len() > 10000 {
            return Err(invalid("root_job_inventory_limit"));
        }
    }
    root.verify()?;
    records.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(records)
}
/// Writer admission must hold the same legacy-compatible root lock while the
/// shared protocol adjudicates these facts and writes its dispatch intent.
pub fn scan_writer_records(lock: &WriterLock<'_>) -> io::Result<Vec<DirectoryRecords>> {
    lock.verify()?;
    let records = scan_root_records(lock.root)?;
    lock.verify()?;
    Ok(records)
}

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(algorithm: RawHandle, buffer: *mut u8, size: u32, flags: u32) -> i32;
}
pub fn random_nonce() -> io::Result<[u8; 32]> {
    let mut bytes = [0; 32];
    if unsafe { BCryptGenRandom(null_mut(), bytes.as_mut_ptr(), 32, 2) } < 0 {
        return Err(changed("system_random_failed"));
    }
    Ok(bytes)
}

/// Preserve a bounded, byte-verified record under an unused name in the same
/// pinned directory. The exclusive source handle survives the rename; no copy
/// followed by deletion, no replacement, no path traversal or reparse allowed.
pub fn rename_record_new(
    dir: &PinnedDirectory<'_>,
    lock: &WriterLock<'_>,
    from: &OsStr,
    to: &OsStr,
    expected: &[u8],
) -> io::Result<()> {
    lock.verify()?;
    dir.verify()?;
    if !std::ptr::eq(dir.root, lock.root) {
        return Err(invalid("record_rename_lock_root"));
    }
    validate_leaf(from)?;
    validate_leaf(to)?;
    if from == to || expected.is_empty() || expected.len() as u64 > RECORD_LIMIT {
        return Err(invalid("record_rename_bound"));
    }
    let source = dir.path.join(from);
    let target = dir.path.join(to);
    let handle = open(
        &source,
        GENERIC_READ | GENERIC_WRITE | DELETE_ACCESS,
        0,
        OPEN_EXISTING,
        FILE_NORMAL | OPEN_REPARSE | WRITE_THROUGH,
    )?;
    let meta = info(&handle)?;
    if meta.attributes & (REPARSE | DIRECTORY) != 0 || meta.links != 1 {
        return Err(invalid("record_rename_not_regular"));
    }
    let identity = file_identity(&handle)?;
    let mut file = File::from(handle);
    let mut raw = Vec::new();
    (&mut file).take(RECORD_LIMIT + 1).read_to_end(&mut raw)?;
    if raw != expected {
        return Err(changed("record_rename_bytes_differ"));
    }
    let handle: OwnedHandle = file.into();
    let name: Vec<u16> = target.as_os_str().encode_wide().chain([0]).collect();
    let length = 20 + name.len() * 2;
    let mut buffer = vec![0u64; length.div_ceil(8)];
    let out = unsafe {
        std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), buffer.len() * 8)
    };
    // Zero ReplaceIfExists: the kernel refuses a pre-existing or racing target.
    out[16..20].copy_from_slice(&((name.len() - 1) as u32 * 2).to_le_bytes());
    for (i, n) in name.into_iter().enumerate() {
        out[20 + i * 2..22 + i * 2].copy_from_slice(&n.to_le_bytes());
    }
    if unsafe {
        SetFileInformationByHandle(
            handle.as_raw_handle(),
            3,
            buffer.as_ptr().cast::<c_void>(),
            length as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if file_identity(&handle)? != identity || final_path(&handle)? != target {
        return Err(changed("record_rename_identity_changed"));
    }
    File::from(handle).sync_all()?;
    lock.verify()?;
    dir.verify()?;
    Ok(())
}
