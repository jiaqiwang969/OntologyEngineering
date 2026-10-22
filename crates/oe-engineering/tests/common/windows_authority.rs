//! Native ACL setup for owned synthetic test files only. Production never repairs ACLs.
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree},
    Security::{Authorization::*, *},
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub fn set_policy_acl(path: &Path, include_everyone_read: bool) {
    let mut token = 0;
    assert_ne!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) },
        0
    );
    let mut count = 0;
    unsafe {
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut count);
    }
    assert!((std::mem::size_of::<TOKEN_USER>() as u32..=65536).contains(&count));
    let mut data = vec![0usize; (count as usize).div_ceil(std::mem::size_of::<usize>())];
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            data.as_mut_ptr().cast(),
            count,
            &mut count,
        )
    };
    unsafe {
        CloseHandle(token);
    }
    assert_ne!(ok, 0);
    let sid = unsafe { (*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    let mut text = ptr::null_mut();
    assert_ne!(unsafe { ConvertSidToStringSidW(sid, &mut text) }, 0);
    let mut length = 0usize;
    while length < 256 && unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    assert!(length < 256);
    let user = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).unwrap();
    unsafe {
        LocalFree(text.cast::<c_void>());
    }
    let mut sddl = format!("D:P(A;;FA;;;{user})(A;;FA;;;SY)(A;;FA;;;BA)");
    if include_everyone_read {
        sddl.push_str("(A;;FR;;;WD)");
    }
    let wide: Vec<_> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = ptr::null_mut();
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        },
        0
    );
    let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        SetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    unsafe {
        LocalFree(descriptor);
    }
    assert_ne!(ok, 0);
}
