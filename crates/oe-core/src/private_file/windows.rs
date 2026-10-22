//! Native Windows owner/DACL verification. No shell or credential values.
use super::Snapshot;
use anyhow::{Context, Result, ensure};
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io::Read,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path},
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

fn user_sid() -> Result<Vec<u8>> {
    let mut token = 0;
    ensure!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "private_identity_token_unavailable"
    );
    let token = Token(token);
    let mut count = 0;
    unsafe {
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut count);
    }
    ensure!(
        (std::mem::size_of::<TOKEN_USER>() as u32..=65536).contains(&count),
        "private_identity_token_size_invalid"
    );
    // usize allocation provides TOKEN_USER alignment, unlike a Vec<u8> cast.
    let mut data = vec![0usize; (count as usize).div_ceil(std::mem::size_of::<usize>())];
    ensure!(
        unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                data.as_mut_ptr().cast(),
                count,
                &mut count,
            )
        } != 0,
        "private_identity_token_unavailable"
    );
    let sid = unsafe { (*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    sid_bytes(sid)
}

fn sid_bytes(sid: PSID) -> Result<Vec<u8>> {
    ensure!(
        !sid.is_null() && unsafe { IsValidSid(sid) } != 0,
        "private_identity_sid_invalid"
    );
    let length = unsafe { GetLengthSid(sid) } as usize;
    ensure!((8..=68).contains(&length), "private_identity_sid_invalid");
    Ok(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec())
}

fn trusted(sid: PSID, user: &[u8]) -> Result<bool> {
    Ok(sid_bytes(sid)? == user
        || unsafe {
            IsWellKnownSid(sid, WinLocalSystemSid) != 0
                || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
        })
}

fn access(file: &File, user: &[u8]) -> Result<Vec<u8>> {
    let mut owner = ptr::null_mut();
    let mut acl = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let code = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            &mut acl,
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    ensure!(code == 0, "private_identity_permissions_unavailable");
    let _descriptor = Local(descriptor);
    validate_access(owner, acl, descriptor, user)
}

// All pointers originate in a live Win32-owned security descriptor. The owner
// allocation remains held by the caller until this bounded validation returns.
fn validate_access(
    owner: PSID,
    acl: *mut ACL,
    descriptor: PSECURITY_DESCRIPTOR,
    user: &[u8],
) -> Result<Vec<u8>> {
    ensure!(trusted(owner, user)?, "private_identity_owner_not_trusted");
    ensure!(
        !acl.is_null() && unsafe { IsValidAcl(acl) } != 0,
        "private_identity_dacl_required"
    );
    let mut control = 0;
    let mut revision = 0;
    ensure!(
        unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } != 0,
        "private_identity_descriptor_invalid"
    );
    ensure!(
        control & SE_DACL_PRESENT != 0,
        "private_identity_dacl_required"
    );
    let size = unsafe { (*acl).AclSize } as usize;
    let count = unsafe { (*acl).AceCount } as u32;
    ensure!(
        size >= std::mem::size_of::<ACL>() && count <= 4096,
        "private_identity_acl_limit"
    );
    for index in 0..count {
        let mut ace = ptr::null_mut();
        ensure!(
            unsafe { GetAce(acl, index, &mut ace) } != 0 && !ace.is_null(),
            "private_identity_ace_invalid"
        );
        let offset = (ace as usize)
            .checked_sub(acl as usize)
            .context("private_identity_ace_invalid")?;
        ensure!(
            offset >= std::mem::size_of::<ACL>() && offset + 4 <= size,
            "private_identity_ace_invalid"
        );
        let header = unsafe { ptr::read_unaligned(ace.cast::<ACE_HEADER>()) };
        let length = header.AceSize as usize;
        ensure!(
            length >= 16 && offset + length <= size,
            "private_identity_ace_invalid"
        );
        // Only ordinary allow/deny ACE layouts are understood. No broadening by
        // interpreting conditional, callback or object-specific ACEs as ordinary.
        ensure!(
            header.AceType == 0 || header.AceType == 1,
            "private_identity_ace_unsupported"
        );
        let bytes = unsafe { std::slice::from_raw_parts(ace.cast::<u8>(), length) };
        let sid_length = 8 + 4 * bytes[9] as usize;
        ensure!(
            sid_length <= 68 && 8 + sid_length <= length,
            "private_identity_sid_invalid"
        );
        let sid = unsafe { ace.cast::<u8>().add(8).cast() };
        let allowed = trusted(sid, user)?;
        let mask = u32::from_le_bytes(bytes[4..8].try_into()?);
        // INHERIT_ONLY does not grant rights on this regular file. Inherited
        // effective ACEs are checked exactly like explicit effective ACEs.
        if header.AceType == 0 && header.AceFlags & INHERIT_ONLY_ACE as u8 == 0 && mask != 0 {
            ensure!(allowed, "private_identity_file_permissions_required");
        }
    }
    // Preserve policy identity without exposing usernames, content, or SID text.
    let mut binding = sid_bytes(owner)?;
    binding.extend_from_slice(&control.to_le_bytes());
    binding.extend_from_slice(unsafe { std::slice::from_raw_parts(acl.cast::<u8>(), size) });
    Ok(binding)
}

fn information(file: &File, directory: bool) -> Result<Vec<u8>> {
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } != 0,
        "private_identity_file_information_unavailable"
    );
    ensure!(
        info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "private_identity_reparse_rejected"
    );
    ensure!(
        (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) == directory,
        "private_identity_regular_file_required"
    );
    if !directory {
        ensure!(
            info.nNumberOfLinks == 1,
            "private_identity_hardlink_rejected"
        );
    }
    let mut out = Vec::new();
    for n in [
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
        info.nFileSizeHigh,
        info.nFileSizeLow,
        info.ftLastWriteTime.dwHighDateTime,
        info.ftLastWriteTime.dwLowDateTime,
    ] {
        out.extend_from_slice(&n.to_le_bytes());
    }
    Ok(out)
}

pub(super) fn read(path: &Path, limit: usize) -> Result<Snapshot> {
    crate::no_symlinks(path)?;
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut parents = Vec::new();
    let mut ancestor = std::path::PathBuf::new();
    for component in path
        .parent()
        .context("private_identity_parent_required")?
        .components()
    {
        ensure!(
            !matches!(component, Component::ParentDir),
            "parent_traversal"
        );
        ancestor.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&ancestor)?;
        information(&file, true)?;
        // Deny delete sharing to hold all ancestor names for this read.
        parents.push(file);
    }
    let open = || {
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
    };
    let mut file = open()?;
    let user = user_sid()?;
    let before = information(&file, false)?;
    let permissions = access(&file, &user)?;
    ensure!(
        file.metadata()?.len() <= limit as u64,
        "input_exceeds_limit"
    );
    let mut bytes = Vec::new();
    (&mut file)
        .take(
            (limit as u64)
                .checked_add(1)
                .context("invalid_read_limit")?,
        )
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "input_exceeds_limit");
    ensure!(
        information(&file, false)? == before && access(&file, &user)? == permissions,
        "private_identity_changed_during_read"
    );
    let current = open()?;
    ensure!(
        information(&current, false)? == before && access(&current, &user)? == permissions,
        "private_identity_changed_during_read"
    );
    let mut binding = before;
    binding.extend(permissions);
    let identity = crate::FileIdentity {
        bytes: bytes.len() as u64,
        sha256: crate::sha256(&bytes),
    };
    Ok(Snapshot {
        bytes,
        identity,
        binding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::windows::ffi::OsStrExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "oe-private-file-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("synthetic.json");
            std::fs::write(&path, b"synthetic only").unwrap();
            Self(path)
        }
        fn acl(&self, sddl: &str) {
            let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
            let mut sd = ptr::null_mut();
            assert_ne!(
                unsafe {
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        text.as_ptr(),
                        1,
                        &mut sd,
                        ptr::null_mut(),
                    )
                },
                0
            );
            let _sd = Local(sd);
            let path: Vec<u16> = self.0.as_os_str().encode_wide().chain(Some(0)).collect();
            assert_ne!(
                unsafe {
                    SetFileSecurityW(
                        path.as_ptr(),
                        DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                        sd,
                    )
                },
                0
            );
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }
    fn user() -> String {
        let mut sid = user_sid().unwrap();
        let mut text = ptr::null_mut();
        assert_ne!(
            unsafe { ConvertSidToStringSidW(sid.as_mut_ptr().cast(), &mut text) },
            0
        );
        let _text = Local(text.cast());
        let mut n = 0;
        unsafe {
            while *text.add(n) != 0 {
                n += 1;
            }
            String::from_utf16(std::slice::from_raw_parts(text, n)).unwrap()
        }
    }
    fn descriptor(sddl: &str) -> Local {
        let text: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut sd = ptr::null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut sd,
                    ptr::null_mut(),
                )
            },
            0
        );
        Local(sd)
    }
    fn parts(sd: &Local) -> (PSID, *mut ACL) {
        let mut owner = ptr::null_mut();
        let mut defaulted = 0;
        assert_ne!(
            unsafe { GetSecurityDescriptorOwner(sd.0, &mut owner, &mut defaulted) },
            0
        );
        let mut acl = ptr::null_mut();
        let mut present = 0;
        assert_ne!(
            unsafe { GetSecurityDescriptorDacl(sd.0, &mut present, &mut acl, &mut defaulted) },
            0
        );
        (owner, acl)
    }
    #[test]
    fn windows_native_descriptors_reject_unknown_owner_and_unsupported_aces() {
        let sid = user();
        let caller = user_sid().unwrap();
        for owner in [&sid, "BA", "SY"] {
            let sd = descriptor(&format!("O:{owner}D:P(A;;FA;;;{sid})"));
            let (o, a) = parts(&sd);
            validate_access(o, a, sd.0, &caller).unwrap();
        }
        let sd = descriptor(&format!("O:S-1-5-21-1-2-3-4567D:P(A;;FA;;;{sid})"));
        let (o, a) = parts(&sd);
        assert_eq!(
            validate_access(o, a, sd.0, &caller)
                .unwrap_err()
                .to_string(),
            "private_identity_owner_not_trusted"
        );
        let sd = descriptor(&format!("O:{sid}"));
        let (o, a) = parts(&sd);
        assert!(validate_access(o, a, sd.0, &caller).is_err());
        let sd = descriptor(&format!(
            "O:{sid}D:P(OA;;FR;bf967aba-0de6-11d0-a285-00aa003049e2;;WD)"
        ));
        let (o, a) = parts(&sd);
        assert_eq!(
            validate_access(o, a, sd.0, &caller)
                .unwrap_err()
                .to_string(),
            "private_identity_ace_unsupported"
        );
        // Native descriptor parser input: an ordinary ACE converted to the
        // callback ACE tag. No callback or file permission is executed.
        let sd = descriptor(&format!("O:{sid}D:P(A;;FA;;;{sid})"));
        let (o, a) = parts(&sd);
        let mut ace = ptr::null_mut();
        assert_ne!(unsafe { GetAce(a, 0, &mut ace) }, 0);
        unsafe {
            (*(ace.cast::<ACE_HEADER>())).AceType = 9;
        }
        assert_eq!(
            validate_access(o, a, sd.0, &caller)
                .unwrap_err()
                .to_string(),
            "private_identity_ace_unsupported"
        );
    }
    #[test]
    fn windows_inherit_only_does_not_grant_this_file_access() {
        let sid = user();
        let caller = user_sid().unwrap();
        let only = descriptor(&format!("O:{sid}D:P(A;;FA;;;{sid})(A;OIIO;FR;;;WD)"));
        let (o, a) = parts(&only);
        validate_access(o, a, only.0, &caller).unwrap();
        let effective = descriptor(&format!("O:{sid}D:P(A;;FA;;;{sid})(A;OI;FR;;;WD)"));
        let (o, a) = parts(&effective);
        assert_eq!(
            validate_access(o, a, effective.0, &caller)
                .unwrap_err()
                .to_string(),
            "private_identity_file_permissions_required"
        );
    }
    #[test]
    fn windows_private_file_rejects_reparse_leaf_and_parent() {
        let f = Fixture::new();
        let sid = user();
        f.acl(&format!("D:P(A;;FA;;;{sid})"));
        let leaf = f.0.with_extension("link");
        std::os::windows::fs::symlink_file(&f.0, &leaf).unwrap();
        assert!(read(&leaf, 64).is_err());
        std::fs::remove_file(&leaf).unwrap();
        let directory = f.0.parent().unwrap().join("alias-dir");
        std::os::windows::fs::symlink_dir(f.0.parent().unwrap(), &directory).unwrap();
        assert!(read(&directory.join("synthetic.json"), 64).is_err());
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn windows_private_acl_accepts_user_and_privileged_subjects() {
        let f = Fixture::new();
        let sid = user();
        for acl in [
            format!("D:P(A;;FA;;;{sid})"),
            format!("D:P(A;;FA;;;{sid})(A;;FA;;;SY)(A;;FA;;;BA)"),
        ] {
            f.acl(&acl);
            let v = read(&f.0, 64).unwrap();
            assert_eq!(v.bytes, b"synthetic only");
            v.recheck(&f.0, 64).unwrap();
        }
    }
    #[test]
    fn windows_private_acl_rejects_other_readers_writers_and_null_acl() {
        let f = Fixture::new();
        let sid = user();
        for other in [
            "(A;;FR;;;WD)",
            "(A;;FR;;;AU)",
            "(A;;FW;;;S-1-5-21-1-2-3-4567)",
            "(A;;WD;;;BU)",
            "(A;ID;FR;;;WD)",
        ] {
            f.acl(&format!("D:P(A;;FA;;;{sid}){other}"));
            assert!(read(&f.0, 64).is_err(), "{other}");
        }
        f.acl("D:NO_ACCESS_CONTROL");
        assert!(read(&f.0, 64).is_err());
    }
    #[test]
    fn windows_private_acl_detects_change_replacement_and_hardlinks() {
        let f = Fixture::new();
        let sid = user();
        f.acl(&format!("D:P(A;;FA;;;{sid})"));
        let before = read(&f.0, 64).unwrap();
        f.acl(&format!("D:P(A;;FA;;;{sid})(A;;FR;;;WD)"));
        assert!(before.recheck(&f.0, 64).is_err());
        f.acl(&format!("D:P(A;;FA;;;{sid})"));
        let alias = f.0.with_extension("alias");
        std::fs::hard_link(&f.0, &alias).unwrap();
        assert!(read(&f.0, 64).is_err());
        std::fs::remove_file(alias).unwrap();
        let replacement = f.0.with_extension("replacement");
        std::fs::write(&replacement, b"synthetic only").unwrap();
        std::fs::remove_file(&f.0).unwrap();
        std::fs::rename(replacement, &f.0).unwrap();
        f.acl(&format!("D:P(A;;FA;;;{sid})"));
        assert!(before.recheck(&f.0, 64).is_err());
        assert!(read(&f.0, 2).is_err());
        assert!(read(f.0.parent().unwrap(), 64).is_err());
    }
}
