// Native, test-only Windows equivalent of the Unix directory-relative reader.
// References: Microsoft Learn NtCreateFile (winternl.h), GetFileType,
// GetFileInformationByHandleEx, FILE_ID_INFO, FILE_BASIC_INFO.
// A local drive root is the only absolute acquisition. Every following open is
// one validated component relative to a held directory, with reparse processing
// disabled. No Win32 pathname reopen obtains the bytes returned to a consumer.
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, RawHandle},
    },
    path::Prefix,
    ptr::null_mut,
};

const READ_DATA: u32 = 0x0001;
const READ_ATTRIBUTES: u32 = 0x0080;
const SYNCHRONIZE: u32 = 0x0010_0000;
const SHARE_ALL: u32 = 1 | 2 | 4;
const FILE_OPEN: u32 = 1;
const SYNCHRONOUS_NONALERT: u32 = 0x0020;
const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const CASE_INSENSITIVE: u32 = 0x0040;
const DIRECTORY_ATTRIBUTE: u32 = 0x0010;
const REPARSE_ATTRIBUTE: u32 = 0x0400;
const FILE_TYPE_DISK: u32 = 1;
const FILE_DEVICE_DISK: u32 = 0x0007;
const FILE_REMOTE_DEVICE: u32 = 0x0010;
const FILE_FS_DEVICE_INFORMATION: i32 = 4;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: RawHandle,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void,
}

#[repr(C)]
struct IoStatusBlock {
    // Native union { NTSTATUS Status; PVOID Pointer; } has pointer size/alignment.
    status_or_pointer: usize,
    information: usize,
}

#[repr(C)]
struct FileIdInformation {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[repr(C)]
struct FileBasicInformation {
    creation_time: i64,
    last_access_time: i64,
    last_write_time: i64,
    change_time: i64,
    file_attributes: u32,
}

#[repr(C)]
struct FileStandardInformation {
    allocation_size: i64,
    end_of_file: i64,
    number_of_links: u32,
    delete_pending: u8,
    directory: u8,
}

#[repr(C)]
struct FileFsDeviceInformation {
    device_type: u32,
    characteristics: u32,
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        file_handle: *mut RawHandle,
        desired_access: u32,
        object_attributes: *mut ObjectAttributes,
        io_status_block: *mut IoStatusBlock,
        allocation_size: *mut i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *mut c_void,
        ea_length: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
    fn NtQueryVolumeInformationFile(
        file_handle: RawHandle,
        io_status_block: *mut IoStatusBlock,
        information: *mut c_void,
        length: u32,
        information_class: i32,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileType(handle: RawHandle) -> u32;
    fn GetFileInformationByHandleEx(
        handle: RawHandle,
        class: i32,
        information: *mut c_void,
        bytes: u32,
    ) -> i32;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    volume: u64,
    file: [u8; 16],
    bytes: u64,
    created: i64,
    written: i64,
    changed: i64,
    attributes: u32,
    links: u32,
    delete_pending: u8,
}

fn component_name(name: &OsStr) -> io::Result<Vec<u16>> {
    let value: Vec<u16> = name.encode_wide().collect();
    // Exclude DOS aliases/streams and namespace separators even for a synthetic
    // binding. Frozen catalog names remain unchanged. NtCreateFile otherwise
    // treats one relative NT name literally rather than doing Win32 cleanup.
    if value.is_empty()
        || value.len() > 32766
        || value.iter().any(|v| matches!(*v, 0 | 0x2f | 0x5c | 0x3a))
        || matches!(value.last(), Some(0x2e | 0x20))
        || name == OsStr::new(".")
        || name == OsStr::new("..")
    {
        return Err(refusal("private_fixture_path_invalid", "Windows component"));
    }
    let alias = name.to_string_lossy().to_ascii_uppercase();
    let stem = alias.split('.').next().unwrap_or_default();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
    };
    if matches!(stem, "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || numbered("COM")
        || numbered("LPT")
    {
        return Err(refusal(
            "private_fixture_path_invalid",
            "Windows device alias",
        ));
    }
    Ok(value)
}

fn native_open(parent: Option<&File>, mut name: Vec<u16>, directory: bool) -> io::Result<File> {
    let code = if directory {
        "private_fixture_directory_missing_or_unreadable"
    } else {
        "private_fixture_missing_or_unreadable"
    };
    let length = name
        .len()
        .checked_mul(2)
        .and_then(|n| u16::try_from(n).ok())
        .ok_or_else(|| refusal("private_fixture_path_invalid", "Windows name length"))?;
    let mut unicode = UnicodeString {
        length,
        maximum_length: length,
        buffer: name.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: parent.map_or(null_mut(), |v| v.as_raw_handle()),
        object_name: &mut unicode,
        attributes: CASE_INSENSITIVE,
        security_descriptor: null_mut(),
        security_quality_of_service: null_mut(),
    };
    let mut output = null_mut();
    let mut status: IoStatusBlock = unsafe { zeroed() };
    // SAFETY: all pointers refer to live correctly aligned native-layout values;
    // each parent is held throughout the call. FILE_OPEN creates nothing, the
    // handle is non-inheritable, and no data is read before type verification.
    // A type-neutral open avoids relying on undocumented combinations of the
    // FILE_DIRECTORY_FILE and FILE_OPEN_REPARSE_POINT flags.
    let result = unsafe {
        NtCreateFile(
            &mut output,
            READ_ATTRIBUTES | SYNCHRONIZE | if directory { 0 } else { READ_DATA },
            &mut attributes,
            &mut status,
            null_mut(),
            0,
            SHARE_ALL,
            FILE_OPEN,
            SYNCHRONOUS_NONALERT | OPEN_REPARSE_POINT,
            null_mut(),
            0,
        )
    };
    if result != 0 {
        return Err(refusal(
            code,
            io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(result) } as i32),
        ));
    }
    if output.is_null() || output as isize == -1 {
        return Err(refusal(code, "native open returned no owned handle"));
    }
    // SAFETY: a successful NtCreateFile transfers one unique owned handle.
    let file = unsafe { File::from_raw_handle(output) };
    identity(&file, directory)?;
    Ok(file)
}

fn native_information<T>(file: &File, class: i32, value: &mut T) -> io::Result<()> {
    // SAFETY: callers pass the corresponding Win32 ABI struct and exact size;
    // the destination remains valid for the synchronous call.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            class,
            (value as *mut T).cast(),
            size_of::<T>() as u32,
        )
    } == 0
    {
        Err(refusal(
            "private_fixture_identity_unavailable",
            io::Error::last_os_error(),
        ))
    } else {
        Ok(())
    }
}

pub(super) fn identity(file: &File, directory: bool) -> io::Result<Identity> {
    let code = if directory {
        "private_fixture_directory_not_regular"
    } else {
        "private_fixture_not_regular"
    };
    // FILE_NON_DIRECTORY_FILE alone would also accept a device or volume.
    if unsafe { GetFileType(file.as_raw_handle()) } != FILE_TYPE_DISK {
        return Err(refusal(code, "disk-backed file required"));
    }
    local_volume(file)?;
    let mut basic: FileBasicInformation = unsafe { zeroed() };
    let mut standard: FileStandardInformation = unsafe { zeroed() };
    let mut id: FileIdInformation = unsafe { zeroed() };
    native_information(file, 0, &mut basic)?; // FileBasicInfo
    native_information(file, 1, &mut standard)?; // FileStandardInfo
    native_information(file, 18, &mut id)?; // FileIdInfo: full 128-bit ID
    if basic.file_attributes & REPARSE_ATTRIBUTE != 0
        || (basic.file_attributes & DIRECTORY_ATTRIBUTE != 0) != directory
        || (standard.directory != 0) != directory
        || standard.delete_pending != 0
    {
        return Err(refusal(code, "reparse, deleted, or wrong file type"));
    }
    let bytes = u64::try_from(standard.end_of_file)
        .map_err(|_| refusal("private_fixture_size_invalid", "negative native file size"))?;
    Ok(Identity {
        volume: id.volume_serial_number,
        file: id.file_id,
        bytes,
        created: basic.creation_time,
        written: basic.last_write_time,
        changed: basic.change_time,
        attributes: basic.file_attributes,
        links: standard.number_of_links,
        delete_pending: standard.delete_pending,
    })
}

pub(super) fn validate_local_device(device_type: u32, characteristics: u32) -> io::Result<()> {
    // A mapped network drive can have a drive-letter prefix and FILE_TYPE_DISK.
    // Require the native opened handle's volume to report a disk device, and
    // reject the remote-device characteristic independently of device type.
    if characteristics & FILE_REMOTE_DEVICE != 0 {
        return Err(refusal(
            "private_fixture_remote_volume",
            "remote volume is outside the local input profile",
        ));
    }
    if device_type != FILE_DEVICE_DISK {
        return Err(refusal(
            "private_fixture_volume_not_local_disk",
            "native local disk device required",
        ));
    }
    Ok(())
}

fn local_volume(file: &File) -> io::Result<()> {
    let mut information: FileFsDeviceInformation = unsafe { zeroed() };
    let mut status: IoStatusBlock = unsafe { zeroed() };
    // SAFETY: the held handle originated in NtCreateFile; native-layout output
    // buffers stay live/aligned. Query the same object, never GetDriveType(path).
    let result = unsafe {
        NtQueryVolumeInformationFile(
            file.as_raw_handle(),
            &mut status,
            (&mut information as *mut FileFsDeviceInformation).cast(),
            size_of::<FileFsDeviceInformation>() as u32,
            FILE_FS_DEVICE_INFORMATION,
        )
    };
    if result != 0 {
        return Err(refusal(
            "private_fixture_volume_identity_unavailable",
            io::Error::from_raw_os_error(unsafe { RtlNtStatusToDosError(result) } as i32),
        ));
    }
    if status.information != size_of::<FileFsDeviceInformation>() {
        return Err(refusal(
            "private_fixture_volume_identity_unavailable",
            "native volume result size mismatch",
        ));
    }
    validate_local_device(information.device_type, information.characteristics)
}

fn same_directory(a: &File, b: &File) -> io::Result<bool> {
    let a = identity(a, true)?;
    let b = identity(b, true)?;
    Ok(a.volume == b.volume && a.file == b.file)
}

pub(super) fn absolute_directory(path: &Path) -> io::Result<File> {
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) if drive.is_ascii_alphabetic() => {
                drive
            }
            _ => {
                return Err(refusal(
                    "private_fixture_path_invalid",
                    "local Windows drive required",
                ));
            }
        },
        _ => {
            return Err(refusal(
                "private_fixture_path_invalid",
                "local Windows drive required",
            ));
        }
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(refusal(
            "private_fixture_path_invalid",
            "absolute Windows drive required",
        ));
    }
    // Validate the whole supplied path before any native open. UNC/device roots,
    // traversal, streams, and DOS alias components are outside this local profile.
    let names: Vec<_> = components
        .map(|part| match part {
            Component::Normal(name) => component_name(name),
            _ => Err(refusal(
                "private_fixture_path_invalid",
                "Windows path component",
            )),
        })
        .collect::<io::Result<_>>()?;
    let native_root: Vec<u16> = format!("\\??\\{}:\\", char::from(drive))
        .encode_utf16()
        .collect();
    let mut directory = native_open(None, native_root, true)?;
    for name in names {
        directory = native_open(Some(&directory), name, true)?;
    }
    Ok(directory)
}

pub(super) fn relative_directory(root: &File, path: &Path) -> io::Result<File> {
    let mut directory = root.try_clone()?;
    for part in path.components() {
        let Component::Normal(name) = part else {
            return Err(refusal(
                "private_fixture_path_invalid",
                "Windows relative component",
            ));
        };
        directory = native_open(Some(&directory), component_name(name)?, true)?;
    }
    Ok(directory)
}

pub(super) fn regular_file_at(parent: &File, name: &OsStr) -> io::Result<File> {
    native_open(Some(parent), component_name(name)?, false)
}

pub(super) fn check_root(path: &Path, held: &File) -> io::Result<()> {
    let current = absolute_directory(path)?;
    if !same_directory(held, &current)? {
        return Err(refusal("private_fixture_root_replaced", path.display()));
    }
    Ok(())
}

pub(super) fn verify_after(
    root_path: &Path,
    root: &File,
    relative: &Path,
    parent: &File,
    file: &File,
    opened: &Identity,
) -> io::Result<()> {
    check_root(root_path, root)?;
    let parent_path = relative
        .parent()
        .ok_or_else(|| refusal("private_fixture_path_invalid", relative.display()))?;
    let current_parent = relative_directory(root, parent_path)?;
    if !same_directory(parent, &current_parent)? {
        return Err(refusal(
            "private_fixture_directory_replaced",
            relative.display(),
        ));
    }
    let name = relative
        .file_name()
        .ok_or_else(|| refusal("private_fixture_path_invalid", relative.display()))?;
    let current_file = regular_file_at(&current_parent, name)?;
    if identity(&current_file, false)? != *opened || identity(file, false)? != *opened {
        return Err(refusal("private_fixture_replaced", relative.display()));
    }
    Ok(())
}

pub(super) fn read(root_path: &Path, root: &File, binding: &FixtureBinding) -> io::Result<Vec<u8>> {
    check_root(root_path, root)?;
    let relative = Path::new(binding.path);
    let parent_path = relative
        .parent()
        .ok_or_else(|| refusal("private_fixture_path_invalid", binding.path))?;
    let name = relative
        .file_name()
        .ok_or_else(|| refusal("private_fixture_path_invalid", binding.path))?;
    let parent = relative_directory(root, parent_path)?;
    let file = regular_file_at(&parent, name)?;
    let opened = identity(&file, false)?;
    if opened.bytes != binding.bytes {
        return Err(refusal("private_fixture_size_mismatch", binding.path));
    }
    let limit = binding
        .bytes
        .checked_add(1)
        .ok_or_else(|| refusal("private_fixture_size_invalid", binding.path))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(
            usize::try_from(limit)
                .map_err(|_| refusal("private_fixture_size_invalid", binding.path))?,
        )
        .map_err(|e| refusal("private_fixture_allocation_failed", e))?;
    (&file).take(limit).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).ok() != Some(binding.bytes) {
        return Err(refusal("private_fixture_size_mismatch", binding.path));
    }
    if private_fixture_sha256(&bytes) != binding.sha256 {
        return Err(refusal("private_fixture_digest_mismatch", binding.path));
    }
    verify_after(root_path, root, relative, &parent, &file, &opened)?;
    Ok(bytes)
}
