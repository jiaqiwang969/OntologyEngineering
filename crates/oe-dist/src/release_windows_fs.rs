//! Native Windows release filesystem policy. All public release objects and
//! state files have a protected DACL granting only the current user, SYSTEM or
//! Administrators. Source-workspace reads additionally allow ordinary read-only
//! ACEs, but never mutate those inputs. No POSIX permission emulation.
use anyhow::{Context, Result, ensure};
use oe_windows_controller::windows::{StableRoot, validate_source_component};
use std::{
    ffi::c_void,
    fs::{self, File, Metadata},
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        fs::MetadataExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
    ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Threading::*,
};

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[repr(C)]
struct IoStatus {
    status_or_pointer: usize,
    information: usize,
}
#[repr(C)]
struct DeviceInformation {
    device_type: u32,
    characteristics: u32,
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationFile(
        handle: HANDLE,
        status: *mut IoStatus,
        information: *mut c_void,
        length: u32,
        information_class: i32,
    ) -> i32;
    fn NtQueryVolumeInformationFile(
        handle: HANDLE,
        status: *mut IoStatus,
        information: *mut c_void,
        length: u32,
        information_class: i32,
    ) -> i32;
}

fn user_sid() -> Result<Vec<u8>> {
    let mut token = 0;
    ensure!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "release_windows_token_unavailable"
    );
    let token = Token(token);
    let mut count = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut count);
    }
    ensure!(
        (size_of::<TOKEN_USER>() as u32..=65536).contains(&count),
        "release_windows_token_size_invalid"
    );
    let mut storage = vec![0usize; (count as usize).div_ceil(size_of::<usize>())];
    ensure!(
        unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                storage.as_mut_ptr().cast(),
                count,
                &mut count,
            )
        } != 0,
        "release_windows_token_unavailable"
    );
    sid_bytes(unsafe { (*(storage.as_ptr().cast::<TOKEN_USER>())).User.Sid })
}
fn sid_bytes(sid: PSID) -> Result<Vec<u8>> {
    ensure!(
        !sid.is_null() && unsafe { IsValidSid(sid) } != 0,
        "release_windows_sid_invalid"
    );
    let length = unsafe { GetLengthSid(sid) } as usize;
    ensure!((8..=68).contains(&length), "release_windows_sid_invalid");
    Ok(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec())
}
fn trusted(sid: PSID, user: &[u8]) -> Result<bool> {
    Ok(sid_bytes(sid)? == user
        || unsafe {
            IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
        })
}
fn descriptor() -> Result<Local> {
    let mut user = user_sid()?;
    let mut text = ptr::null_mut();
    ensure!(
        unsafe { ConvertSidToStringSidW(user.as_mut_ptr().cast(), &mut text) } != 0,
        "release_windows_sid_conversion_failed"
    );
    let _text = Local(text.cast());
    let mut len = 0;
    while len < 256 && unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    ensure!(len < 256, "release_windows_sid_string_invalid");
    let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, len) })?;
    // Protected at creation. Child objects are also explicitly protected, so
    // inherited ACL relaxation cannot silently change the release policy.
    let sddl = format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
    let wide: Vec<u16> = sddl.encode_utf16().chain([0]).collect();
    let mut sd = ptr::null_mut();
    ensure!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                1,
                &mut sd,
                ptr::null_mut(),
            )
        } != 0,
        "release_windows_descriptor_creation_failed"
    );
    Ok(Local(sd))
}
fn security(file: &File, protected: bool) -> Result<Vec<u8>> {
    let mut owner = ptr::null_mut();
    let mut acl = ptr::null_mut();
    let mut sd = ptr::null_mut();
    let result = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut sd,
        )
    };
    ensure!(result == 0, "release_windows_acl_unavailable");
    let _sd = Local(sd);
    let user = user_sid()?;
    ensure!(trusted(owner, &user)?, "release_windows_untrusted_owner");
    ensure!(
        !acl.is_null() && unsafe { IsValidAcl(acl) } != 0,
        "release_windows_dacl_required"
    );
    let mut control = 0;
    let mut revision = 0;
    ensure!(
        unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) } != 0,
        "release_windows_descriptor_invalid"
    );
    ensure!(
        control & SE_DACL_PRESENT != 0 && (!protected || control & SE_DACL_PROTECTED != 0),
        "release_windows_protected_dacl_required"
    );
    let size = unsafe { (*acl).AclSize } as usize;
    let count = unsafe { (*acl).AceCount } as u32;
    ensure!(
        size >= size_of::<ACL>() && count <= 4096,
        "release_windows_acl_limit"
    );
    for index in 0..count {
        let mut ace = ptr::null_mut();
        ensure!(
            unsafe { GetAce(acl, index, &mut ace) } != 0 && !ace.is_null(),
            "release_windows_ace_invalid"
        );
        let offset = (ace as usize)
            .checked_sub(acl as usize)
            .context("release_windows_ace_invalid")?;
        ensure!(
            offset >= size_of::<ACL>() && offset + size_of::<ACE_HEADER>() <= size,
            "release_windows_ace_invalid"
        );
        let header = unsafe { ptr::read_unaligned(ace.cast::<ACE_HEADER>()) };
        let length = header.AceSize as usize;
        ensure!(
            length >= 16 && offset + length <= size && matches!(header.AceType, 0 | 1),
            "release_windows_ace_unsupported"
        );
        let bytes = unsafe { std::slice::from_raw_parts(ace.cast::<u8>(), length) };
        let sid_length = 8 + 4 * bytes[9] as usize;
        ensure!(
            sid_length <= 68 && 8 + sid_length <= length,
            "release_windows_ace_sid_invalid"
        );
        let sid = unsafe { ace.cast::<u8>().add(8).cast() };
        let mask = u32::from_le_bytes(bytes[4..8].try_into()?);
        if header.AceType == 0
            && header.AceFlags & INHERIT_ONLY_ACE as u8 == 0
            && mask != 0
            && !trusted(sid, &user)?
        {
            let read_only =
                FILE_GENERIC_READ | FILE_GENERIC_EXECUTE | GENERIC_READ | GENERIC_EXECUTE;
            ensure!(
                !protected && mask & !read_only == 0,
                "release_windows_untrusted_access"
            );
        }
    }
    let mut identity = sid_bytes(owner)?;
    identity.extend_from_slice(&control.to_le_bytes());
    identity.extend_from_slice(unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), size) });
    Ok(identity)
}

pub(crate) fn validate_path(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "release_absolute_path_required");
    ensure!(
        matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))),
        "release_windows_local_drive_required"
    );
    for component in path.components() {
        match component {
            Component::Normal(name) => validate_source_component(name)?,
            Component::Prefix(_) | Component::RootDir => {}
            _ => anyhow::bail!("release_windows_non_normal_path"),
        }
    }
    Ok(())
}
pub(crate) fn reject_reparse(metadata: &Metadata) -> Result<()> {
    ensure!(
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "release_windows_reparse_rejected"
    );
    Ok(())
}
fn wide(path: &Path) -> Result<Vec<u16>> {
    validate_path(path)?;
    let mut result: Vec<u16> = path.as_os_str().encode_wide().collect();
    ensure!(
        !result.contains(&0) && result.len() < 32767,
        "release_windows_path_invalid"
    );
    result.push(0);
    Ok(result)
}
fn open(path: &Path, access: u32, share: u32, creation: u32, sd: Option<&Local>) -> Result<File> {
    let name = wide(path)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.map_or(ptr::null_mut(), |v| v.0),
        bInheritHandle: 0,
    };
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            share,
            if sd.is_some() {
                &attributes
            } else {
                ptr::null()
            },
            creation,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_handle(handle as _) })
}
fn raw_info<T>(file: &File, class: FILE_INFO_BY_HANDLE_CLASS, value: &mut T) -> Result<()> {
    ensure!(
        unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle() as HANDLE,
                class,
                (value as *mut T).cast(),
                size_of::<T>() as u32,
            )
        } != 0,
        "release_windows_file_information_unavailable"
    );
    Ok(())
}
fn local_ntfs(file: &File) -> Result<()> {
    ensure!(
        unsafe { GetFileType(file.as_raw_handle() as HANDLE) } == FILE_TYPE_DISK,
        "release_windows_disk_file_required"
    );
    let mut status: IoStatus = unsafe { zeroed() };
    let mut device: DeviceInformation = unsafe { zeroed() };
    let result = unsafe {
        NtQueryVolumeInformationFile(
            file.as_raw_handle() as HANDLE,
            &mut status,
            (&mut device as *mut DeviceInformation).cast(),
            size_of::<DeviceInformation>() as u32,
            4,
        )
    };
    ensure!(
        result == 0
            && status.information == size_of::<DeviceInformation>()
            && device.device_type == 7
            && device.characteristics & 0x10 == 0,
        "release_windows_local_volume_required"
    );
    let mut filesystem = [0u16; 64];
    ensure!(
        unsafe {
            GetVolumeInformationByHandleW(
                file.as_raw_handle() as HANDLE,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        } != 0,
        "release_windows_volume_information_unavailable"
    );
    let length = filesystem
        .iter()
        .position(|v| *v == 0)
        .context("release_windows_volume_name_invalid")?;
    ensure!(
        String::from_utf16(&filesystem[..length])? == "NTFS",
        "release_windows_ntfs_required"
    );
    Ok(())
}
const STREAM_INFORMATION_LIMIT: usize = 64 * 1024;

/// The only accepted NTFS payload shape is one unnamed data stream. Because
/// any additional stream is forbidden, a nonzero NextEntryOffset is rejected
/// immediately rather than followed through potentially malformed input. NTFS
/// directories without data streams return a successful zero-byte result.
fn validate_stream_information(data: &[u8], directory: bool, bytes: u64) -> Result<()> {
    ensure!(
        data.len() <= STREAM_INFORMATION_LIMIT,
        "release_windows_stream_information_limit"
    );
    if directory {
        ensure!(
            data.is_empty(),
            "release_windows_directory_data_stream_rejected"
        );
        return Ok(());
    }
    ensure!(
        data.len() >= 24,
        "release_windows_stream_information_truncated"
    );
    let next = u32::from_le_bytes(data[0..4].try_into()?);
    let name_length = u32::from_le_bytes(data[4..8].try_into()?) as usize;
    let stream_bytes = i64::from_le_bytes(data[8..16].try_into()?);
    let allocation = i64::from_le_bytes(data[16..24].try_into()?);
    ensure!(next == 0, "release_windows_nondefault_data_stream_rejected");
    ensure!(
        name_length.is_multiple_of(2) && name_length.checked_add(24) == Some(data.len()),
        "release_windows_stream_information_length_invalid"
    );
    // No lossy decoding/normalization: the exact NTFS unnamed stream spelling.
    const DEFAULT_NAME: &[u8] = b":\0:\0$\0D\0A\0T\0A\0";
    ensure!(
        &data[24..] == DEFAULT_NAME,
        "release_windows_nondefault_data_stream_rejected"
    );
    ensure!(
        stream_bytes >= 0 && allocation >= 0 && stream_bytes as u64 == bytes,
        "release_windows_stream_size_changed"
    );
    Ok(())
}
fn unnamed_stream_only(file: &File, directory: bool, bytes: u64) -> Result<()> {
    // Native FILE_STREAM_INFORMATION requires 8-byte alignment. A complete
    // successful bounded result is mandatory; unsupported/overflow/pending are
    // refusals, never evidence that a file has no extra streams.
    let mut storage = vec![0u64; STREAM_INFORMATION_LIMIT / size_of::<u64>()];
    let mut status: IoStatus = unsafe { zeroed() };
    let result = unsafe {
        NtQueryInformationFile(
            file.as_raw_handle() as HANDLE,
            &mut status,
            storage.as_mut_ptr().cast(),
            STREAM_INFORMATION_LIMIT as u32,
            22,
        )
    };
    ensure!(
        result == 0 && status.information <= STREAM_INFORMATION_LIMIT,
        "release_windows_stream_information_unavailable"
    );
    let data =
        unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), status.information) };
    validate_stream_information(data, directory, bytes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stamp {
    volume: u64,
    id: [u8; 16],
    pub bytes: u64,
    created: i64,
    written: i64,
    changed: i64,
    attributes: u32,
    links: u32,
    pub directory: bool,
    security: Vec<u8>,
}
impl Stamp {
    pub fn same_object(&self, other: &Self) -> bool {
        self.volume == other.volume
            && self.id == other.id
            && self.directory == other.directory
            && self.security == other.security
    }
}
fn file_stamp(file: &File, protected: bool, check_security: bool) -> Result<Stamp> {
    local_ntfs(file)?;
    let mut basic: FILE_BASIC_INFO = unsafe { zeroed() };
    let mut standard: FILE_STANDARD_INFO = unsafe { zeroed() };
    let mut id: FILE_ID_INFO = unsafe { zeroed() };
    raw_info(file, FileBasicInfo, &mut basic)?;
    raw_info(file, FileStandardInfo, &mut standard)?;
    raw_info(file, FileIdInfo, &mut id)?;
    let directory = basic.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    ensure!(
        basic.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
            && (standard.Directory != 0) == directory
            && standard.DeletePending == 0
            && (directory || standard.NumberOfLinks == 1),
        "release_windows_special_or_linked_file"
    );
    let bytes = u64::try_from(standard.EndOfFile)?;
    if protected {
        unnamed_stream_only(file, directory, bytes)?;
    }
    Ok(Stamp {
        volume: id.VolumeSerialNumber,
        id: id.FileId.Identifier,
        bytes,
        created: basic.CreationTime,
        written: basic.LastWriteTime,
        changed: basic.ChangeTime,
        attributes: basic.FileAttributes,
        links: standard.NumberOfLinks,
        directory,
        security: if check_security {
            security(file, protected)?
        } else {
            Vec::new()
        },
    })
}
pub(crate) fn directory(path: &Path) -> Result<StableRoot> {
    validate_path(path)?;
    let root = StableRoot::open(path, None)?;
    let file = open(
        root.path(),
        FILE_READ_ATTRIBUTES | READ_CONTROL,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        None,
    )?;
    ensure!(
        file_stamp(&file, false, false)?.directory,
        "release_directory_required"
    );
    root.verify()?;
    Ok(root)
}
pub(crate) fn stamp(path: &Path, protected: bool) -> Result<Stamp> {
    let parent = directory(path.parent().context("release_parent_missing")?)?;
    let file = open(
        path,
        FILE_READ_ATTRIBUTES | READ_CONTROL,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        None,
    )?;
    let result = file_stamp(&file, protected, protected)?;
    parent.verify()?;
    Ok(result)
}
pub(crate) struct ReadFile {
    pub file: File,
    parent: StableRoot,
    path: PathBuf,
    protected: bool,
}
impl ReadFile {
    pub fn stamp(&self) -> Result<Stamp> {
        self.parent.verify()?;
        let result = file_stamp(&self.file, self.protected, true)?;
        ensure!(!result.directory, "release_regular_file_required");
        Ok(result)
    }
}
pub(crate) fn open_file(path: &Path, limit: u64, protected: bool) -> Result<ReadFile> {
    let parent = directory(path.parent().context("release_parent_missing")?)?;
    let file = open(
        path,
        GENERIC_READ | READ_CONTROL,
        FILE_SHARE_READ,
        OPEN_EXISTING,
        None,
    )?;
    let result = ReadFile {
        file,
        parent,
        path: path.to_owned(),
        protected,
    };
    let info = result.stamp()?;
    ensure!(info.bytes <= limit, "release_file_size_limit");
    Ok(result)
}
pub(crate) fn read_bytes(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut file = open_file(path, limit as u64, true)?;
    let before = file.stamp()?;
    let mut bytes = Vec::new();
    (&mut file.file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == before.bytes
            && before == file.stamp()?
            && before == stamp(path, true)?,
        "release_read_changed"
    );
    Ok(bytes)
}
pub(crate) fn create_dir(path: &Path) -> Result<()> {
    let parent = directory(path.parent().context("release_parent_missing")?)?;
    let name = wide(path)?;
    let sd = descriptor()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    if unsafe { CreateDirectoryW(name.as_ptr(), &attributes) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    ensure!(
        stamp(path, true)?.directory,
        "release_created_directory_invalid"
    );
    parent.verify()?;
    Ok(())
}
pub(crate) fn ensure_directories(root: &Path, destination: &Path) -> Result<()> {
    let root_pin = directory(root)?;
    let relative = destination
        .strip_prefix(root)
        .context("release_directory_outside_stage")?;
    let mut cursor = root.to_owned();
    for component in relative.components() {
        ensure!(
            matches!(component, Component::Normal(_)),
            "release_windows_non_normal_path"
        );
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_dir(&cursor)?,
            Err(e) => return Err(e.into()),
            Ok(_) => ensure!(
                stamp(&cursor, true)?.directory,
                "release_directory_required"
            ),
        }
    }
    root_pin.verify()?;
    Ok(())
}
pub(crate) struct WriteFile {
    file: File,
    _parent: StableRoot,
}
impl std::ops::Deref for WriteFile {
    type Target = File;
    fn deref(&self) -> &File {
        &self.file
    }
}
impl std::ops::DerefMut for WriteFile {
    fn deref_mut(&mut self) -> &mut File {
        &mut self.file
    }
}
pub(crate) fn create_file(path: &Path) -> Result<WriteFile> {
    let parent = directory(path.parent().context("release_parent_missing")?)?;
    let sd = descriptor()?;
    let file = open(
        path,
        GENERIC_READ | GENERIC_WRITE | READ_CONTROL,
        FILE_SHARE_READ,
        CREATE_NEW,
        Some(&sd),
    )?;
    ensure!(
        !file_stamp(&file, true, true)?.directory,
        "release_regular_file_required"
    );
    Ok(WriteFile {
        file,
        _parent: parent,
    })
}
pub(crate) fn write_new_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = create_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
/// Apply only to a just-created, task-owned staging object. No source-input or
/// existing installation caller may use this function to repair permissions.
pub(crate) fn protect_new(path: &Path) -> Result<()> {
    let parent = directory(path.parent().context("release_parent_missing")?)?;
    let file = open(
        path,
        READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
        None,
    )?;
    file_stamp(&file, false, false)?;
    let sd = descriptor()?;
    let mut present = 0;
    let mut defaulted = 0;
    let mut acl = ptr::null_mut();
    ensure!(
        unsafe { GetSecurityDescriptorDacl(sd.0, &mut present, &mut acl, &mut defaulted) } != 0
            && present != 0
            && !acl.is_null(),
        "release_windows_descriptor_invalid"
    );
    let result = unsafe {
        SetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl,
            ptr::null_mut(),
        )
    };
    ensure!(result == 0, "release_windows_protect_failed");
    file_stamp(&file, true, true)?;
    parent.verify()?;
    Ok(())
}
/// Byte-copy keeps the package owner. A native source pin additionally denies
/// concurrent writes/deletion, captures full ID and ACL, and retains ancestors.
pub(crate) fn copy_source(
    source: &Path,
    destination: &Path,
    expected: &crate::package::SourceFile,
) -> Result<u32> {
    let input = open_file(source, expected.bytes, false)?;
    let before = input.stamp()?;
    ensure!(
        before.bytes == expected.bytes,
        "release_source_size_changed"
    );
    let _destination_parent = directory(destination.parent().context("release_parent_missing")?)?;
    crate::package::copy_locked(source, destination, expected)?;
    protect_new(destination)?;
    ensure!(before == input.stamp()?, "release_source_changed");
    let current = open_file(&input.path, expected.bytes, false)?;
    ensure!(before == current.stamp()?, "release_source_path_changed");
    Ok(0)
}
pub(crate) fn source_stamp(path: &Path) -> Result<Stamp> {
    open_file(path, crate::package::MAX_FILE_BYTES, false)?.stamp()
}
pub(crate) fn check_source(path: &Path) -> Result<()> {
    source_stamp(path)?;
    Ok(())
}

pub(crate) struct AdvisoryFileLock {
    file: File,
    parent: StableRoot,
    path: PathBuf,
    original: Stamp,
}
impl AdvisoryFileLock {
    pub fn acquire(path: &Path, exclusive: bool) -> Result<Self> {
        let parent = directory(path.parent().context("release_parent_missing")?)?;
        ensure!(
            stamp(parent.path(), true)?.directory,
            "release_windows_store_acl_invalid"
        );
        let sd = descriptor()?;
        let file = open(
            path,
            GENERIC_READ | GENERIC_WRITE | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_ALWAYS,
            Some(&sd),
        )?;
        let original = file_stamp(&file, true, true)?;
        ensure!(
            !original.directory && original.bytes == 0,
            "release_lock_not_empty_regular"
        );
        if exclusive {
            file.try_lock()?;
        } else {
            file.try_lock_shared()?;
        }
        let lock = Self {
            file,
            parent,
            path: path.to_owned(),
            original,
        };
        lock.ensure_current()?;
        Ok(lock)
    }
    pub fn ensure_current(&self) -> Result<()> {
        self.parent.verify()?;
        ensure!(
            self.original == file_stamp(&self.file, true, true)?
                && self.original == stamp(&self.path, true)?,
            "release_lock_changed"
        );
        Ok(())
    }
}

/// One native same-directory commit by the already-open source handle. The old
/// record is rechecked while opened with write sharing denied; all expected
/// failures precede SetFileInformationByHandle. No fallible work after commit.
pub(crate) fn replace_current(
    from: &Path,
    to: &Path,
    bytes: &[u8],
    previous: Option<&Stamp>,
) -> Result<()> {
    ensure!(
        from.parent() == to.parent(),
        "release_current_cross_directory"
    );
    let parent = directory(from.parent().context("release_parent_missing")?)?;
    let mut source = open(
        from,
        GENERIC_READ | DELETE | READ_CONTROL,
        0,
        OPEN_EXISTING,
        None,
    )?;
    let source_stamp = file_stamp(&source, true, true)?;
    ensure!(
        !source_stamp.directory && source_stamp.bytes == bytes.len() as u64,
        "release_prepared_record_changed"
    );
    let mut actual = Vec::new();
    (&mut source)
        .take(bytes.len() as u64 + 1)
        .read_to_end(&mut actual)?;
    ensure!(
        actual == bytes && source_stamp == file_stamp(&source, true, true)?,
        "release_prepared_record_changed"
    );
    let _previous_handle = if let Some(expected) = previous {
        let file = open(
            to,
            GENERIC_READ | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            OPEN_EXISTING,
            None,
        )?;
        ensure!(
            expected == &file_stamp(&file, true, true)?,
            "release_current_changed"
        );
        Some(file)
    } else {
        ensure!(
            fs::symlink_metadata(to).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "release_current_changed"
        );
        None
    };
    parent.verify()?;
    let target = wide(
        &parent
            .path()
            .join(to.file_name().context("release_name_missing")?),
    )?;
    // FILE_RENAME_INFO's x64 layout: Flags/union at 0, HANDLE at 8,
    // FileNameLength at 16, inline UTF-16 FileName at 20. This module is only
    // compiled for x86_64-pc-windows-msvc, with aligned storage throughout.
    let length = 20usize
        .checked_add(target.len() * 2)
        .context("release_rename_size_overflow")?;
    let mut storage = vec![0u64; length.div_ceil(8)];
    let buffer = unsafe {
        std::slice::from_raw_parts_mut(storage.as_mut_ptr().cast::<u8>(), storage.len() * 8)
    };
    // A retained previous-current handle prevents concurrent writes through
    // the commit boundary. Ordinary FileRenameInfo refuses an open target
    // (reproduced as Win32 error 5); Ex's POSIX flag supports retaining it.
    // First publication keeps flags zero: a racing target is never replaced.
    // No legacy fallback may drop the handle or weaken the precondition.
    let flags: u32 = if previous.is_some() {
        0x0000_0001 | 0x0000_0002
    } else {
        0
    };
    buffer[..4].copy_from_slice(&flags.to_le_bytes());
    buffer[16..20].copy_from_slice(&u32::try_from((target.len() - 1) * 2)?.to_le_bytes());
    for (i, value) in target.into_iter().enumerate() {
        buffer[20 + i * 2..22 + i * 2].copy_from_slice(&value.to_le_bytes());
    }
    if unsafe {
        SetFileInformationByHandle(
            source.as_raw_handle() as HANDLE,
            FileRenameInfoEx,
            storage.as_ptr().cast(),
            u32::try_from(length)?,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(test)]
mod stream_information_tests {
    use super::*;
    fn record(name: &str, size: i64, allocation: i64, next: u32) -> Vec<u8> {
        let name: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut data = next.to_le_bytes().to_vec();
        data.extend_from_slice(&(name.len() as u32).to_le_bytes());
        data.extend_from_slice(&size.to_le_bytes());
        data.extend_from_slice(&allocation.to_le_bytes());
        data.extend_from_slice(&name);
        data
    }
    #[test]
    fn exact_default_file_and_empty_directory_are_distinct() {
        let default = record("::$DATA", 42, 48, 0);
        assert_eq!(default.len(), 38);
        assert!(validate_stream_information(&default, false, 42).is_ok());
        assert!(validate_stream_information(&[], true, 0).is_ok());
        assert!(validate_stream_information(&[], false, 0).is_err());
        assert!(validate_stream_information(&default, true, 0).is_err());
    }
    #[test]
    fn named_and_additional_streams_refuse_without_following_offsets() {
        assert!(
            validate_stream_information(&record(":hidden:$DATA", 42, 48, 0), false, 42).is_err()
        );
        for next in [1, 8, 24, 38, 40, u32::MAX] {
            assert!(
                validate_stream_information(&record("::$DATA", 42, 48, next), false, 42).is_err()
            );
        }
        assert!(validate_stream_information(&record(":hidden:$DATA", 42, 48, 0), true, 0).is_err());
    }
    #[test]
    fn truncated_odd_oversized_and_trailing_stream_bytes_refuse() {
        let original = record("::$DATA", 42, 48, 0);
        for n in 0..original.len() {
            assert!(validate_stream_information(&original[..n], false, 42).is_err());
        }
        for length in [0, 1, 13, 15, 16, u32::MAX] {
            let mut data = original.clone();
            data[4..8].copy_from_slice(&length.to_le_bytes());
            assert!(validate_stream_information(&data, false, 42).is_err());
        }
        let mut trailing = original.clone();
        trailing.push(0);
        assert!(validate_stream_information(&trailing, false, 42).is_err());
        assert!(
            validate_stream_information(&vec![0; STREAM_INFORMATION_LIMIT + 1], false, 0).is_err()
        );
    }
    #[test]
    fn stream_size_and_allocation_do_not_invent_file_identity() {
        assert!(validate_stream_information(&record("::$DATA", -1, 48, 0), false, 42).is_err());
        assert!(validate_stream_information(&record("::$DATA", 42, -1, 0), false, 42).is_err());
        assert!(validate_stream_information(&record("::$DATA", 41, 48, 0), false, 42).is_err());
        // Sparse/compressed allocation may be smaller than logical size.
        assert!(validate_stream_information(&record("::$DATA", 42, 0, 0), false, 42).is_ok());
        assert!(validate_stream_information(&record("::$data", 42, 48, 0), false, 42).is_err());
    }
}
