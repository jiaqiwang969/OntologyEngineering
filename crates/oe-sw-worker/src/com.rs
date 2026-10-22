//! STA-only Automation. No coercion, script execution, retry, or application discovery.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    ffi::c_void, fs::File, io::Write, marker::PhantomData, mem::ManuallyDrop, rc::Rc, time::Instant,
};
use windows::{
    Win32::{
        System::{Com::*, Ole::*, Variant::*},
        UI::WindowsAndMessaging::*,
    },
    core::{BSTR, GUID, IUnknown, Interface, VARIANT},
};

pub struct Apartment(PhantomData<Rc<()>>);
impl Apartment {
    pub fn enter() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self(PhantomData))
    }
    pub fn pump(&self) {
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub enum Arg {
    Bool(bool),
    I4(i32),
    R8(f64),
    Text(String),
    NullDispatch,
    I4Ref(Box<i32>),
    R8Ref(Box<f64>),
}
impl Arg {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::I4(_) => "i4",
            Self::R8(_) => "r8",
            Self::Text(_) => "bstr",
            Self::NullDispatch => "dispatch",
            Self::I4Ref(_) => "ptr:i4",
            Self::R8Ref(_) => "ptr:r8",
        }
    }
    fn value(&self) -> Value {
        match self {
            Self::Bool(v) => json!(v),
            Self::I4(v) => json!(v),
            Self::R8(v) => json!(v),
            Self::Text(v) => json!(v),
            Self::NullDispatch => Value::Null,
            Self::I4Ref(v) => json!(**v),
            Self::R8Ref(v) => json!(**v),
        }
    }
    fn variant(&mut self) -> VARIANT {
        match self {
            Self::Bool(v) => VARIANT::from(*v),
            Self::I4(v) => VARIANT::from(*v),
            Self::R8(v) => VARIANT::from(*v),
            Self::Text(v) => VARIANT::from(v.as_str()),
            _ => unsafe {
                let mut raw = *VARIANT::new().as_raw();
                match self {
                    Self::NullDispatch => {
                        raw.Anonymous.Anonymous.vt = VT_DISPATCH.0;
                        raw.Anonymous.Anonymous.Anonymous.pdispVal = std::ptr::null_mut();
                    }
                    Self::I4Ref(v) => {
                        raw.Anonymous.Anonymous.vt = (VT_BYREF | VT_I4).0;
                        raw.Anonymous.Anonymous.Anonymous.plVal = &mut **v;
                    }
                    Self::R8Ref(v) => {
                        raw.Anonymous.Anonymous.vt = (VT_BYREF | VT_R8).0;
                        raw.Anonymous.Anonymous.Anonymous.pdblVal = &mut **v;
                    }
                    _ => unreachable!(),
                }
                VARIANT::from_raw(raw)
            },
        }
    }
}
pub fn vt(v: &VARIANT) -> u16 {
    unsafe { v.as_raw().Anonymous.Anonymous.vt }
}
pub fn integer(v: &VARIANT) -> Result<i32> {
    ensure!(vt(v) == VT_I4.0, "solidworks_expected_i4");
    Ok(unsafe { v.as_raw().Anonymous.Anonymous.Anonymous.lVal })
}
pub fn boolean(v: &VARIANT) -> Result<bool> {
    ensure!(vt(v) == VT_BOOL.0, "solidworks_expected_bool");
    let raw = unsafe { v.as_raw().Anonymous.Anonymous.Anonymous.boolVal };
    ensure!(
        raw == 0 || raw == -1,
        "solidworks_noncanonical_variant_bool"
    );
    Ok(raw != 0)
}
pub fn real(v: &VARIANT) -> Result<f64> {
    ensure!(vt(v) == VT_R8.0, "solidworks_expected_r8");
    let value = unsafe { v.as_raw().Anonymous.Anonymous.Anonymous.dblVal };
    ensure!(value.is_finite(), "solidworks_nonfinite_native_value");
    Ok(value)
}
pub fn string(v: &VARIANT) -> Result<String> {
    ensure!(vt(v) == VT_BSTR.0, "solidworks_expected_bstr");
    let text = BSTR::try_from(v)?.to_string();
    ensure!(
        text.len() <= 32768 && !text.contains('\0'),
        "solidworks_native_string_bound"
    );
    Ok(text)
}
pub fn dispatch(v: &VARIANT) -> Result<IDispatch> {
    unsafe {
        match vt(v) {
            tag if tag == VT_DISPATCH.0 => {
                let ptr = v.as_raw().Anonymous.Anonymous.Anonymous.pdispVal;
                ensure!(!ptr.is_null(), "solidworks_null_dispatch");
                // The VARIANT owns its reference. Borrow then AddRef before returning.
                Ok(IDispatch::from_raw_borrowed(&ptr)
                    .context("solidworks_dispatch_null")?
                    .clone())
            }
            tag if tag == VT_UNKNOWN.0 => {
                let ptr = v.as_raw().Anonymous.Anonymous.Anonymous.punkVal;
                ensure!(!ptr.is_null(), "solidworks_null_unknown");
                Ok(IUnknown::from_raw_borrowed(&ptr)
                    .context("solidworks_unknown_null")?
                    .cast()?)
            }
            _ => anyhow::bail!("solidworks_expected_interface"),
        }
    }
}
pub fn query(object: &IDispatch, iid: &GUID) -> Result<IDispatch> {
    let mut ptr = std::ptr::null_mut();
    unsafe {
        object.query(iid, &mut ptr).ok()?;
        ensure!(!ptr.is_null(), "solidworks_query_null");
        Ok(IDispatch::from_raw(ptr))
    }
}
fn array(v: &VARIANT, maximum: usize) -> Result<(*mut SAFEARRAY, i32, i32, u16)> {
    let tag = vt(v);
    ensure!(
        tag & VT_ARRAY.0 != 0 && tag & VT_BYREF.0 == 0,
        "solidworks_expected_owned_safearray"
    );
    let base = tag & !VT_ARRAY.0;
    ensure!(
        matches!(base, 3 | 5 | 9 | 12 | 13),
        "solidworks_safearray_type_not_supported"
    );
    let ptr = unsafe {
        v.as_raw()
            .Anonymous
            .Anonymous
            .Anonymous
            .parray
            .cast::<SAFEARRAY>()
    };
    ensure!(!ptr.is_null(), "solidworks_null_safearray");
    unsafe {
        ensure!(SafeArrayGetDim(ptr) == 1, "solidworks_safearray_rank");
        ensure!(
            SafeArrayGetVartype(ptr)?.0 == base,
            "solidworks_safearray_vartype_mismatch"
        );
        let low = SafeArrayGetLBound(ptr, 1)?;
        let high = SafeArrayGetUBound(ptr, 1)?;
        let count = i64::from(high) - i64::from(low) + 1;
        ensure!(
            count >= 0 && count as u64 <= maximum as u64,
            "solidworks_safearray_bounds"
        );
        let expected = match base {
            3 => 4,
            5 => 8,
            9 | 13 => std::mem::size_of::<*mut c_void>(),
            12 => std::mem::size_of::<VARIANT>(),
            _ => unreachable!(),
        };
        ensure!(
            SafeArrayGetElemsize(ptr) as usize == expected,
            "solidworks_safearray_element_size"
        );
        Ok((ptr, low, high, base))
    }
}
pub fn reals(v: &VARIANT, maximum: usize) -> Result<Vec<f64>> {
    let (ptr, low, high, base) = array(v, maximum)?;
    ensure!(
        base == VT_R8.0 || base == VT_VARIANT.0,
        "solidworks_expected_real_array"
    );
    let mut out = Vec::new();
    for index in low..=high {
        let value = if base == VT_R8.0 {
            let mut value = 0.0f64;
            unsafe {
                SafeArrayGetElement(ptr, &index, (&mut value as *mut f64).cast())?;
            }
            value
        } else {
            let mut value = VARIANT::new();
            unsafe {
                SafeArrayGetElement(ptr, &index, (&mut value as *mut VARIANT).cast())?;
            }
            real(&value)?
        };
        ensure!(value.is_finite(), "solidworks_array_nonfinite");
        out.push(value);
    }
    Ok(out)
}
pub fn objects(v: &VARIANT, maximum: usize) -> Result<Vec<IDispatch>> {
    if vt(v) == VT_EMPTY.0 {
        return Ok(Vec::new());
    }
    let (ptr, low, high, base) = array(v, maximum)?;
    ensure!(
        matches!(base, 9 | 12 | 13),
        "solidworks_expected_interface_array"
    );
    let mut out = Vec::new();
    for index in low..=high {
        if base == VT_VARIANT.0 {
            let mut value = VARIANT::new();
            unsafe {
                SafeArrayGetElement(ptr, &index, (&mut value as *mut VARIANT).cast())?;
            }
            out.push(dispatch(&value)?);
        } else {
            let mut raw: *mut c_void = std::ptr::null_mut();
            unsafe {
                SafeArrayGetElement(ptr, &index, (&mut raw as *mut *mut c_void).cast())?;
            }
            ensure!(!raw.is_null(), "solidworks_null_array_interface");
            out.push(unsafe {
                if base == VT_DISPATCH.0 {
                    IDispatch::from_raw(raw)
                } else {
                    IUnknown::from_raw(raw).cast()?
                }
            });
        }
    }
    Ok(out)
}
struct Exception(EXCEPINFO);
impl Drop for Exception {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.0.bstrSource);
            ManuallyDrop::drop(&mut self.0.bstrDescription);
            ManuallyDrop::drop(&mut self.0.bstrHelpFile);
        }
    }
}
pub struct Trace {
    pub rows: Vec<Value>,
    file: File,
    _sta: PhantomData<Rc<()>>,
}
impl Trace {
    pub fn new(file: File) -> Self {
        Self {
            rows: Vec::new(),
            file,
            _sta: PhantomData,
        }
    }
    pub fn push(&mut self, value: Value) -> Result<()> {
        ensure!(self.rows.len() < 8192, "solidworks_call_budget");
        let raw = serde_json::to_vec(&value)?;
        ensure!(raw.len() <= 64 * 1024, "solidworks_call_record_budget");
        self.file.write_all(&raw)?;
        self.file.write_all(b"\n")?;
        self.file.sync_all()?;
        self.rows.push(value);
        Ok(())
    }
    pub fn invoke(
        &mut self,
        object: &IDispatch,
        api: &crate::installation::Api,
        key: &str,
        mut args: Vec<Arg>,
    ) -> Result<(VARIANT, Vec<Value>)> {
        let spec = api.member(key)?;
        ensure!(
            args.len() == spec.arguments.len()
                && args
                    .iter()
                    .zip(spec.arguments)
                    .all(|(a, b)| argument_matches(a, b)),
            "solidworks_invoke_argument_shape:{key}"
        );
        ensure!(
            args.iter().all(|a| match a {
                Arg::R8(v) => v.is_finite(),
                Arg::Text(v) => v.len() <= 32768 && !v.contains('\0'),
                _ => true,
            }),
            "solidworks_invoke_value_invalid"
        );
        let object = query(object, &spec.iid)?;
        let types: Vec<_> = args.iter().map(Arg::kind).collect();
        let before: Vec<_> = args.iter().map(Arg::value).collect();
        self.push(json!({"phase":"before_invoke","member":key,"dispid":spec.dispid,"invoke_kind":spec.kind,"types_declaration_order":types,"arguments_declaration_order":before,"outcome":"unknown_until_return"}))?;
        // Boxes keep BYREF addresses stable when variants are reversed.
        let mut variants: Vec<_> = args.iter_mut().map(Arg::variant).collect();
        variants.reverse();
        let mut named = DISPID_PROPERTYPUT;
        let params = DISPPARAMS {
            rgvarg: variants.as_mut_ptr(),
            rgdispidNamedArgs: if spec.kind == 4 {
                &mut named
            } else {
                std::ptr::null_mut()
            },
            cArgs: variants.len() as u32,
            cNamedArgs: u32::from(spec.kind == 4),
        };
        let mut result = VARIANT::new();
        let mut ex = Exception(EXCEPINFO::default());
        let mut argument_error = u32::MAX;
        let begin = Instant::now();
        // windows-rs Result wrapper erases successful nonzero HRESULTs. Preserve the ABI value.
        let status = unsafe {
            (object.vtable().Invoke)(
                object.as_raw(),
                spec.dispid,
                &GUID::zeroed(),
                0,
                DISPATCH_FLAGS(spec.kind),
                &params,
                (&mut result as *mut VARIANT).cast(),
                &mut ex.0,
                &mut argument_error,
            )
        };
        let mut deferred = None;
        if status.0 as u32 == 0x80020009 {
            if let Some(fill) = ex.0.pfnDeferredFillIn.take() {
                deferred = Some(format!("0x{:08x}", unsafe { fill(&mut ex.0) }.0 as u32));
            }
        }
        drop(variants);
        let after: Vec<_> = args.iter().map(Arg::value).collect();
        let hresult = status.0 as u32;
        self.push(json!({"phase":"after_invoke","member":key,"hresult":format!("0x{hresult:08x}"),"elapsed_ms":begin.elapsed().as_millis(),"result_vartype":vt(&result),"arguments_declaration_order":after,"argument_error_reverse_index":if argument_error==u32::MAX {None}else{Some(argument_error)},"exception":{"wCode":ex.0.wCode,"scode":ex.0.scode,"source":ex.0.bstrSource.to_string(),"description":ex.0.bstrDescription.to_string(),"help_file":ex.0.bstrHelpFile.to_string(),"help_context":ex.0.dwHelpContext,"deferred_hresult":deferred}}))?;
        status
            .ok()
            .with_context(|| format!("solidworks_invoke_failed:{key}:0x{hresult:08x}"))?;
        Ok((result, after))
    }
}
fn argument_matches(arg: &Arg, expected: &str) -> bool {
    match arg {
        Arg::NullDispatch => matches!(expected, "dispatch" | "ptr:ICallout"),
        _ => arg.kind() == expected,
    }
}
#[cfg(test)]
#[path = "com_tests.rs"]
mod tests;
