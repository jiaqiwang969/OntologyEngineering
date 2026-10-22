//! Fixed local Win32_Process.Create through native COM. No remote WMI host,
//! credentials, script engine, shell, scheduled task or persistent service.
use crate::windows::ProcessWatch;
use std::{
    ffi::{OsStr, OsString, c_void},
    io, mem,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::null_mut,
};
#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
const IID_NULL: Guid = Guid {
    a: 0,
    b: 0,
    c: 0,
    d: [0; 8],
};
const IID_DISPATCH: Guid = Guid {
    a: 0x00020400,
    b: 0,
    c: 0,
    d: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};
#[repr(C)]
struct Variant {
    kind: u16,
    reserved: [u16; 3],
    data: [u64; 2],
}
#[repr(C)]
struct Params {
    args: *mut Variant,
    named: *mut i32,
    count: u32,
    named_count: u32,
}
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    fn CoUninitialize();
    fn CoInitializeSecurity(
        descriptor: *mut c_void,
        count: i32,
        services: *mut c_void,
        reserved: *mut c_void,
        auth: u32,
        imp: u32,
        auth_list: *mut c_void,
        cap: u32,
        reserved2: *mut c_void,
    ) -> i32;
    fn CLSIDFromProgID(name: *const u16, clsid: *mut Guid) -> i32;
    fn CoCreateInstance(
        clsid: *const Guid,
        outer: *mut c_void,
        context: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
}
#[link(name = "oleaut32")]
unsafe extern "system" {
    fn SysAllocStringLen(value: *const u16, len: u32) -> *mut u16;
    fn VariantClear(value: *mut Variant) -> i32;
}
fn error(context: &str, code: i32) -> io::Error {
    io::Error::other(format!("{context}:HRESULT:{:08x}", code as u32))
}
fn checked(context: &str, code: i32) -> io::Result<()> {
    if code < 0 {
        Err(error(context, code))
    } else {
        Ok(())
    }
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}
impl Variant {
    fn empty() -> Self {
        Self {
            kind: 0,
            reserved: [0; 3],
            data: [0; 2],
        }
    }
    fn null() -> Self {
        Self {
            kind: 1,
            ..Self::empty()
        }
    }
    fn string(text: &OsStr) -> io::Result<Self> {
        let units: Vec<u16> = text.encode_wide().collect();
        if units.contains(&0) || units.len() > 32766 {
            return Err(io::Error::other("WMI_string_bound"));
        }
        let p = unsafe { SysAllocStringLen(units.as_ptr(), units.len() as u32) };
        if p.is_null() {
            return Err(io::Error::other("BSTR_allocation_failed"));
        }
        Ok(Self {
            kind: 8,
            data: [p as u64, 0],
            ..Self::empty()
        })
    }
    fn integer(value: u32) -> Self {
        Self {
            kind: 3,
            data: [value as u64, 0],
            ..Self::empty()
        }
    }
    fn byref(value: &mut Variant) -> Self {
        Self {
            kind: 0x400c,
            data: [value as *mut Self as u64, 0],
            ..Self::empty()
        }
    }
    fn number(&self) -> io::Result<u32> {
        if ![3, 19].contains(&self.kind) || self.data[0] > u32::MAX as u64 {
            return Err(io::Error::other("unexpected_WMI_integer"));
        }
        Ok(self.data[0] as u32)
    }
    fn object(mut self) -> io::Result<Dispatch> {
        if self.kind != 9 || self.data[0] == 0 {
            return Err(io::Error::other("unexpected_WMI_dispatch"));
        }
        let p = self.data[0] as *mut c_void;
        self.kind = 0;
        self.data = [0; 2];
        Ok(Dispatch(p))
    }
}
impl Drop for Variant {
    fn drop(&mut self) {
        unsafe {
            VariantClear(self);
        }
    }
}
struct Dispatch(*mut c_void);
impl Drop for Dispatch {
    fn drop(&mut self) {
        unsafe {
            let vtable = *(self.0 as *mut *const usize);
            let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                mem::transmute(*vtable.add(2));
            release(self.0);
        }
    }
}
impl Dispatch {
    fn invoke(&self, name: &str, flags: u16, mut arguments: Vec<Variant>) -> io::Result<Variant> {
        let mut name: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let mut names = [name.as_mut_ptr()];
        let mut id = 0;
        let vtable = unsafe { *(self.0 as *mut *const usize) };
        let ids: unsafe extern "system" fn(
            *mut c_void,
            *const Guid,
            *mut *mut u16,
            u32,
            u32,
            *mut i32,
        ) -> i32 = unsafe { mem::transmute(*vtable.add(5)) };
        checked("GetIDsOfNames", unsafe {
            ids(self.0, &IID_NULL, names.as_mut_ptr(), 1, 0x400, &mut id)
        })?;
        arguments.reverse();
        let mut named = -3;
        let mut params = Params {
            args: arguments.as_mut_ptr(),
            named: if flags == 4 { &mut named } else { null_mut() },
            count: arguments.len() as u32,
            named_count: u32::from(flags == 4),
        };
        let mut result = Variant::empty();
        let mut arg_error = u32::MAX;
        let invoke: unsafe extern "system" fn(
            *mut c_void,
            i32,
            *const Guid,
            u32,
            u16,
            *mut Params,
            *mut Variant,
            *mut c_void,
            *mut u32,
        ) -> i32 = unsafe { mem::transmute(*vtable.add(6)) };
        checked("IDispatch_Invoke", unsafe {
            invoke(
                self.0,
                id,
                &IID_NULL,
                0x400,
                flags,
                &mut params,
                &mut result,
                null_mut(),
                &mut arg_error,
            )
        })?;
        Ok(result)
    }
}
/// Quote Windows argv for CreateProcess-style parsing, with no command shell.
fn quote(value: &OsStr) -> io::Result<OsString> {
    let units: Vec<_> = value.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::other("NUL_in_WMI_argument"));
    }
    let mut out = vec![34u16];
    let mut slashes = 0;
    for unit in units {
        if unit == 92 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            92,
            if unit == 34 { 2 * slashes + 1 } else { slashes },
        ));
        out.push(unit);
        slashes = 0;
    }
    out.extend(std::iter::repeat_n(92, 2 * slashes));
    out.push(34);
    use std::os::windows::ffi::OsStringExt;
    Ok(OsString::from_wide(&out))
}
/// Caller persists its single-use intent first. Any error after Invoke(Create)
/// is outcome-unknown: never retry by creating another supervisor/job.
pub fn spawn_supervisor(
    executable: &Path,
    args: &[OsString],
    directory: &Path,
) -> io::Result<ProcessWatch> {
    if !executable.is_absolute() || !directory.is_absolute() {
        return Err(io::Error::other("absolute_WMI_launch_paths_required"));
    }
    let mut command = quote(executable.as_os_str())?;
    for arg in args {
        command.push(" ");
        command.push(quote(arg)?);
    }
    checked("CoInitializeEx", unsafe { CoInitializeEx(null_mut(), 0) })?;
    let _apartment = Apartment;
    let security = unsafe {
        CoInitializeSecurity(
            null_mut(),
            -1,
            null_mut(),
            null_mut(),
            0,
            3,
            null_mut(),
            0,
            null_mut(),
        )
    };
    if security < 0 && security as u32 != 0x80010119 {
        return Err(error("CoInitializeSecurity", security));
    }
    let prog: Vec<_> = "WbemScripting.SWbemLocator"
        .encode_utf16()
        .chain([0])
        .collect();
    let mut clsid = IID_NULL;
    checked("CLSIDFromProgID", unsafe {
        CLSIDFromProgID(prog.as_ptr(), &mut clsid)
    })?;
    let mut raw = null_mut();
    checked("CoCreateInstance", unsafe {
        CoCreateInstance(&clsid, null_mut(), 1, &IID_DISPATCH, &mut raw)
    })?;
    let locator = Dispatch(raw);
    let services = locator
        .invoke(
            "ConnectServer",
            1,
            vec![
                Variant::string(OsStr::new("."))?,
                Variant::string(OsStr::new("root\\cimv2"))?,
            ],
        )?
        .object()?;
    let security = services.invoke("Security_", 2, vec![])?.object()?;
    security.invoke("ImpersonationLevel", 4, vec![Variant::integer(3)])?;
    let process = services
        .invoke(
            "Get",
            1,
            vec![Variant::string(OsStr::new("Win32_Process"))?],
        )?
        .object()?;
    let mut pid = Variant::empty();
    let result = process.invoke(
        "Create",
        1,
        vec![
            Variant::string(&command)?,
            Variant::string(directory.as_os_str())?,
            Variant::null(),
            Variant::byref(&mut pid),
        ],
    )?;
    if result.number()? != 0 {
        return Err(io::Error::other(format!(
            "WMI_process_create_return:{}",
            result.number()?
        )));
    }
    let id = pid.number()?;
    // Open the returned PID once and retain its actual creation-time handle;
    // supervisor independently verifies the persisted nonce before any worker.
    ProcessWatch::open_observed_pid(id)
}
