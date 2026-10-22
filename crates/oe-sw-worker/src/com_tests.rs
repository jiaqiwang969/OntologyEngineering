//! Windows-only Automation tests, not CAD observations. SAFEARRAY allocation,
//! copying and destruction use OleAut32; the counted IDispatch below is an
//! explicitly synthetic local test object and never registers a COM class.
//!
//! ABI: local windows/windows-core 0.58.0 generated definitions.
//! SafeArrayPutElement's interface argument has no extra pointer indirection:
//! https://learn.microsoft.com/en-us/windows/win32/api/oleauto/nf-oleauto-safearrayputelement
//! https://learn.microsoft.com/en-us/windows/win32/api/oleauto/nf-oleauto-safearraygetelement
use super::*;
use std::{
    mem::MaybeUninit,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};
use windows::core::{HRESULT, IUnknown_Vtbl, PCWSTR};

fn owned_array(kind: VARENUM, bounds: &[SAFEARRAYBOUND]) -> VARIANT {
    let array = unsafe { SafeArrayCreate(kind, bounds.len() as u32, bounds.as_ptr()) };
    assert!(!array.is_null(), "actual OleAut32 allocation failed");
    unsafe {
        let mut raw = *VARIANT::new().as_raw();
        raw.Anonymous.Anonymous.vt = (VT_ARRAY | kind).0;
        raw.Anonymous.Anonymous.Anonymous.parray = array.cast();
        VARIANT::from_raw(raw)
    }
}
fn vector(kind: VARENUM, lower: i32, count: u32) -> VARIANT {
    owned_array(
        kind,
        &[SAFEARRAYBOUND {
            cElements: count,
            lLbound: lower,
        }],
    )
}
fn array_pointer(value: &VARIANT) -> *mut SAFEARRAY {
    unsafe { value.as_raw().Anonymous.Anonymous.Anonymous.parray.cast() }
}
fn put_real(value: &VARIANT, index: i32, number: f64) {
    unsafe {
        SafeArrayPutElement(array_pointer(value), &index, (&number as *const f64).cast()).unwrap();
    }
}
fn put_variant(value: &VARIANT, index: i32, item: &VARIANT) {
    unsafe {
        SafeArrayPutElement(
            array_pointer(value),
            &index,
            (item as *const VARIANT).cast(),
        )
        .unwrap();
    }
}
fn rejects<T>(actual: Result<T>, reason: &str) {
    let error = match actual {
        Ok(_) => panic!("expected rejection: {reason}"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains(reason),
        "expected {reason}, actual {error:#}"
    );
}

#[test]
fn byref_boxes_survive_argument_reallocation_and_reverse_order() {
    let _sta = Apartment::enter().unwrap();
    let mut args = vec![Arg::I4Ref(Box::new(-1)), Arg::R8Ref(Box::new(0.0))];
    let mut variants: Vec<_> = args.iter_mut().map(Arg::variant).collect();
    // Reallocation moves enum values, not the boxed numeric storage.
    args.reserve(1024);
    variants.reverse();
    assert_eq!(vt(&variants[0]), (VT_BYREF | VT_R8).0);
    assert_eq!(vt(&variants[1]), (VT_BYREF | VT_I4).0);
    unsafe {
        let real = variants[0].as_raw().Anonymous.Anonymous.Anonymous.pdblVal;
        let integer = variants[1].as_raw().Anonymous.Anonymous.Anonymous.plVal;
        assert!(!real.is_null() && !integer.is_null());
        *real = 1.25;
        *integer = 42;
    }
    // VariantClear on BYREF must not free the Box that owns the target.
    drop(variants);
    assert_eq!(args[0].value(), json!(42));
    assert_eq!(args[1].value(), json!(1.25));
    let moved = args;
    assert_eq!(moved[0].value(), json!(42));
}

#[test]
fn automation_boolean_requires_zero_or_minus_one() {
    let _sta = Apartment::enter().unwrap();
    assert!(!boolean(&VARIANT::from(false)).unwrap());
    assert!(boolean(&VARIANT::from(true)).unwrap());
    for value in [1_i16, 2, i16::MIN] {
        let malformed = unsafe {
            let mut raw = *VARIANT::new().as_raw();
            raw.Anonymous.Anonymous.vt = VT_BOOL.0;
            raw.Anonymous.Anonymous.Anonymous.boolVal = value;
            VARIANT::from_raw(raw)
        };
        rejects(boolean(&malformed), "solidworks_noncanonical_variant_bool");
    }
    rejects(boolean(&VARIANT::from(1_i32)), "solidworks_expected_bool");
}

#[test]
fn real_and_variant_arrays_preserve_nonzero_bounds_and_reject_partial_reads() {
    let _sta = Apartment::enter().unwrap();
    let direct = vector(VT_R8, -7, 3);
    for (i, value) in [1.25, -2.5, 0.0].into_iter().enumerate() {
        put_real(&direct, -7 + i as i32, value);
    }
    assert_eq!(reals(&direct, 3).unwrap(), vec![1.25, -2.5, 0.0]);
    rejects(reals(&direct, 2), "solidworks_safearray_bounds");
    assert_eq!(
        reals(&direct, 3).unwrap().len(),
        3,
        "rejection does not damage array"
    );
    let variants = vector(VT_VARIANT, 11, 2);
    put_variant(&variants, 11, &VARIANT::from(0.5_f64));
    put_variant(&variants, 12, &VARIANT::from(4.0_f64));
    assert_eq!(reals(&variants, 2).unwrap(), vec![0.5, 4.0]);
    put_variant(&variants, 12, &VARIANT::from(4_i32));
    rejects(reals(&variants, 2), "solidworks_expected_r8");
    // Failure in the second cell cannot return a successful one-cell prefix.
    put_variant(&variants, 12, &VARIANT::from(8.0_f64));
    assert_eq!(reals(&variants, 2).unwrap(), vec![0.5, 8.0]);
    let empty = vector(VT_R8, 5, 0);
    assert!(reals(&empty, 0).unwrap().is_empty());
    put_real(&direct, -6, f64::NAN);
    rejects(reals(&direct, 3), "solidworks_array_nonfinite");
}

// Restore metadata before the sole owning VARIANT can destroy its array,
// including on assertion unwind; never ask OleAut32 to free a malformed shape.
struct ElementSizeRestore {
    array: *mut SAFEARRAY,
    original: u32,
}
impl Drop for ElementSizeRestore {
    fn drop(&mut self) {
        unsafe {
            (*self.array).cbElements = self.original;
        }
    }
}

#[test]
fn array_tag_rank_and_element_size_are_checked_before_element_access() {
    let _sta = Apartment::enter().unwrap();
    let direct = vector(VT_R8, 2, 1);
    put_real(&direct, 2, 9.0);
    let mismatch = unsafe {
        let mut raw = *direct.as_raw();
        raw.Anonymous.Anonymous.vt = (VT_ARRAY | VT_I4).0;
        ManuallyDrop::new(VARIANT::from_raw(raw))
    };
    rejects(array(&mismatch, 1), "solidworks_safearray_vartype_mismatch");
    // Independent nonowning view; never assign over an owning VARIANT payload.
    let byref_view = unsafe {
        let mut raw = *direct.as_raw();
        raw.Anonymous.Anonymous.vt = (VT_ARRAY | VT_BYREF | VT_R8).0;
        ManuallyDrop::new(VARIANT::from_raw(raw))
    };
    rejects(array(&byref_view, 1), "solidworks_expected_owned_safearray");
    let rank_two = owned_array(
        VT_R8,
        &[
            SAFEARRAYBOUND {
                cElements: 2,
                lLbound: 1,
            },
            SAFEARRAYBOUND {
                cElements: 2,
                lLbound: -2,
            },
        ],
    );
    rejects(reals(&rank_two, 4), "solidworks_safearray_rank");
    {
        let array = array_pointer(&direct);
        let _restore = ElementSizeRestore {
            array,
            original: unsafe { (*array).cbElements },
        };
        unsafe {
            (*array).cbElements = 1;
        }
        rejects(reals(&direct, 1), "solidworks_safearray_element_size");
    }
    assert_eq!(reals(&direct, 1).unwrap(), vec![9.0]);
}

#[test]
fn null_dispatch_cannot_be_used_as_numeric_byref_storage() {
    assert!(argument_matches(&Arg::NullDispatch, "dispatch"));
    assert!(argument_matches(&Arg::NullDispatch, "ptr:ICallout"));
    for name in [
        "ptr:i4",
        "ptr:r8",
        "ptr:variant",
        "ptr:IUnknown",
        "ptr:NotQualified",
    ] {
        assert!(!argument_matches(&Arg::NullDispatch, name), "{name}");
    }
    assert!(argument_matches(&Arg::I4Ref(Box::new(0)), "ptr:i4"));
    assert!(argument_matches(&Arg::R8Ref(Box::new(0.0)), "ptr:r8"));
}

#[derive(Default)]
struct Counts {
    refs: AtomicU32,
    adds: AtomicU32,
    releases: AtomicU32,
    calls: AtomicU32,
    destroyed: AtomicU32,
}
#[repr(C)]
struct SyntheticDispatch {
    vtable: &'static IDispatch_Vtbl,
    counts: Arc<Counts>,
}
const OK: HRESULT = HRESULT(0);
const FALSE_SUCCESS: HRESULT = HRESULT(1);
const BAD_ARGUMENT: HRESULT = HRESULT(0x80070057_u32 as i32);
const NO_INTERFACE: HRESULT = HRESULT(0x80004002_u32 as i32);
const NOT_IMPLEMENTED: HRESULT = HRESULT(0x80004001_u32 as i32);
const EXCEPTION: HRESULT = HRESULT(0x80020009_u32 as i32);
unsafe extern "system" fn synthetic_query(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    if this.is_null() || iid.is_null() || out.is_null() {
        return BAD_ARGUMENT;
    }
    unsafe {
        *out = ptr::null_mut();
        if *iid == IUnknown::IID || *iid == IDispatch::IID {
            synthetic_addref(this);
            *out = this;
            OK
        } else {
            NO_INTERFACE
        }
    }
}
unsafe extern "system" fn synthetic_addref(this: *mut c_void) -> u32 {
    unsafe {
        let counts = &(*this.cast::<SyntheticDispatch>()).counts;
        counts.adds.fetch_add(1, Ordering::SeqCst);
        counts.refs.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }
}
unsafe extern "system" fn synthetic_release(this: *mut c_void) -> u32 {
    let remaining = unsafe {
        let counts = &(*this.cast::<SyntheticDispatch>()).counts;
        counts.releases.fetch_add(1, Ordering::SeqCst);
        let remaining = counts.refs.fetch_sub(1, Ordering::SeqCst).wrapping_sub(1);
        if remaining == 0 {
            counts.destroyed.fetch_add(1, Ordering::SeqCst);
        }
        remaining
    };
    if remaining == 0 {
        unsafe {
            drop(Box::from_raw(this.cast::<SyntheticDispatch>()));
        }
    }
    remaining
}
unsafe extern "system" fn synthetic_type_count(_this: *mut c_void, count: *mut u32) -> HRESULT {
    if count.is_null() {
        return BAD_ARGUMENT;
    }
    unsafe {
        *count = 0;
    }
    OK
}
unsafe extern "system" fn synthetic_type_info(
    _this: *mut c_void,
    _index: u32,
    _lcid: u32,
    out: *mut *mut c_void,
) -> HRESULT {
    if !out.is_null() {
        unsafe {
            *out = ptr::null_mut();
        }
    }
    NOT_IMPLEMENTED
}
unsafe extern "system" fn synthetic_names(
    _this: *mut c_void,
    _iid: *const GUID,
    _names: *const PCWSTR,
    _count: u32,
    _lcid: u32,
    _ids: *mut i32,
) -> HRESULT {
    NOT_IMPLEMENTED
}
unsafe extern "system" fn fill_exception(info: *mut EXCEPINFO) -> HRESULT {
    if info.is_null() {
        return BAD_ARGUMENT;
    }
    unsafe {
        (*info).bstrDescription = ManuallyDrop::new(BSTR::from("synthetic deferred diagnostic"));
        (*info).dwHelpContext = 42;
    }
    FALSE_SUCCESS
}
unsafe extern "system" fn synthetic_invoke(
    this: *mut c_void,
    id: i32,
    _iid: *const GUID,
    _lcid: u32,
    flags: DISPATCH_FLAGS,
    params: *const DISPPARAMS,
    result: *mut MaybeUninit<VARIANT>,
    ex: *mut EXCEPINFO,
    arg_error: *mut u32,
) -> HRESULT {
    if this.is_null() || params.is_null() || flags.0 != 1 {
        return BAD_ARGUMENT;
    }
    unsafe {
        (&(*this.cast::<SyntheticDispatch>()).counts)
            .calls
            .fetch_add(1, Ordering::SeqCst);
    }
    if id == 8 {
        if ex.is_null() {
            return BAD_ARGUMENT;
        }
        unsafe {
            (*ex).bstrSource = ManuallyDrop::new(BSTR::from("synthetic COM object, not CAD"));
            (*ex).scode = 0x80004005_u32 as i32;
            (*ex).pfnDeferredFillIn = Some(fill_exception);
            if !arg_error.is_null() {
                *arg_error = 0;
            }
        }
        return EXCEPTION;
    }
    unsafe {
        let p = &*params;
        if id != 7 || p.cArgs != 3 || p.cNamedArgs != 0 || p.rgvarg.is_null() || result.is_null() {
            return BAD_ARGUMENT;
        }
        let args = std::slice::from_raw_parts_mut(p.rgvarg, 3);
        if vt(&args[0]) != (VT_BYREF | VT_R8).0
            || vt(&args[1]) != (VT_BYREF | VT_I4).0
            || vt(&args[2]) != VT_I4.0
            || args[2].as_raw().Anonymous.Anonymous.Anonymous.lVal != 17
        {
            return BAD_ARGUMENT;
        }
        let real = args[0].as_raw().Anonymous.Anonymous.Anonymous.pdblVal;
        let integer = args[1].as_raw().Anonymous.Anonymous.Anonymous.plVal;
        if real.is_null() || integer.is_null() {
            return BAD_ARGUMENT;
        }
        *real = 1.25;
        *integer = 42;
        (*result).write(VARIANT::from(true));
    }
    FALSE_SUCCESS
}
static SYNTHETIC_VTABLE: IDispatch_Vtbl = IDispatch_Vtbl {
    base__: IUnknown_Vtbl {
        QueryInterface: synthetic_query,
        AddRef: synthetic_addref,
        Release: synthetic_release,
    },
    GetTypeInfoCount: synthetic_type_count,
    GetTypeInfo: synthetic_type_info,
    GetIDsOfNames: synthetic_names,
    Invoke: synthetic_invoke,
};
fn synthetic() -> (IDispatch, Arc<Counts>) {
    let counts = Arc::new(Counts::default());
    counts.refs.store(1, Ordering::SeqCst);
    let object = Box::new(SyntheticDispatch {
        vtable: &SYNTHETIC_VTABLE,
        counts: Arc::clone(&counts),
    });
    (
        unsafe { IDispatch::from_raw(Box::into_raw(object).cast()) },
        counts,
    )
}
fn refs(counts: &Counts) -> u32 {
    counts.refs.load(Ordering::SeqCst)
}

#[test]
fn interface_arrays_keep_owned_references_after_source_variant_is_dropped() {
    let _sta = Apartment::enter().unwrap();
    for kind in [VT_DISPATCH, VT_UNKNOWN, VT_VARIANT] {
        let (object, counts) = synthetic();
        let values = vector(kind, 6, 2);
        for index in [6, 7] {
            if kind == VT_VARIANT {
                let cell = unsafe {
                    let mut raw = *VARIANT::new().as_raw();
                    raw.Anonymous.Anonymous.vt = VT_DISPATCH.0;
                    raw.Anonymous.Anonymous.Anonymous.pdispVal = object.clone().into_raw();
                    VARIANT::from_raw(raw)
                };
                put_variant(&values, index, &cell);
            } else {
                // The real OleAut32 API accepts the interface pointer itself.
                unsafe {
                    SafeArrayPutElement(array_pointer(&values), &index, object.as_raw()).unwrap();
                }
            }
        }
        assert_eq!(
            refs(&counts),
            3,
            "one caller plus two stored interface references"
        );
        let extracted = objects(&values, 2).unwrap();
        assert_eq!(extracted.len(), 2);
        assert_eq!(refs(&counts), 5);
        assert!(extracted.iter().all(|i| i.as_raw() == object.as_raw()));
        drop(values);
        assert_eq!(refs(&counts), 3);
        drop(object);
        assert_eq!(refs(&counts), 2);
        let queried = extracted[0].cast::<IUnknown>().unwrap();
        assert_eq!(refs(&counts), 3);
        drop(queried);
        drop(extracted);
        assert_eq!(refs(&counts), 0);
        assert_eq!(counts.destroyed.load(Ordering::SeqCst), 1);
        assert_eq!(
            counts.releases.load(Ordering::SeqCst),
            counts.adds.load(Ordering::SeqCst) + 1
        );
    }
}

#[test]
fn invalid_late_interface_element_releases_the_successful_prefix() {
    let _sta = Apartment::enter().unwrap();
    let (object, counts) = synthetic();
    let values = vector(VT_DISPATCH, -1, 2);
    unsafe {
        SafeArrayPutElement(array_pointer(&values), &-1, object.as_raw()).unwrap();
    }
    // The second cell remains a null interface. The successful first GetElement
    // must be released when the complete operation rejects that second cell.
    assert_eq!(refs(&counts), 2);
    rejects(objects(&values, 2), "solidworks_null_array_interface");
    assert_eq!(refs(&counts), 2);
    drop(values);
    assert_eq!(refs(&counts), 1);
    drop(object);
    assert_eq!(counts.destroyed.load(Ordering::SeqCst), 1);
}

fn synthetic_api(exception: bool) -> crate::installation::Api {
    crate::installation::Api::test(crate::installation::Member {
        key: if exception {
            "Synthetic.Exception"
        } else {
            "Synthetic.WriteBack"
        },
        iid: IDispatch::IID,
        name: if exception { "Exception" } else { "WriteBack" },
        dispid: if exception { 8 } else { 7 },
        kind: 1,
        arguments: if exception {
            &[]
        } else {
            &["i4", "ptr:i4", "ptr:r8"]
        },
        flags: if exception { &[] } else { &[1, 3, 3] },
        returns: if exception { "void" } else { "bool" },
    })
}

#[test]
fn invoke_writes_byref_outputs_and_preserves_nonzero_success_hresult() {
    let _sta = Apartment::enter().unwrap();
    let (object, counts) = synthetic();
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut trace = Trace::new(file.reopen().unwrap());
    let (result, after) = trace
        .invoke(
            &object,
            &synthetic_api(false),
            "Synthetic.WriteBack",
            vec![
                Arg::I4(17),
                Arg::I4Ref(Box::new(-1)),
                Arg::R8Ref(Box::new(0.0)),
            ],
        )
        .unwrap();
    assert!(boolean(&result).unwrap());
    assert_eq!(after, vec![json!(17), json!(42), json!(1.25)]);
    assert_eq!(counts.calls.load(Ordering::SeqCst), 1);
    assert_eq!(trace.rows.len(), 2);
    assert_eq!(trace.rows[0]["phase"], "before_invoke");
    assert_eq!(trace.rows[0]["outcome"], "unknown_until_return");
    assert_eq!(
        trace.rows[1]["hresult"], "0x00000001",
        "S_FALSE must not become fabricated S_OK"
    );
    let persisted: Vec<Value> = std::fs::read_to_string(file.path())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(persisted, trace.rows);
    assert_eq!(
        refs(&counts),
        1,
        "temporary QueryInterface must be released"
    );
    drop(object);
    assert_eq!(counts.destroyed.load(Ordering::SeqCst), 1);
}

#[test]
fn invoke_exception_keeps_original_hresult_deferred_text_and_no_retry() {
    let _sta = Apartment::enter().unwrap();
    let (object, counts) = synthetic();
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut trace = Trace::new(file.reopen().unwrap());
    rejects(
        trace.invoke(&object, &synthetic_api(true), "Synthetic.Exception", vec![]),
        "solidworks_invoke_failed:Synthetic.Exception:0x80020009",
    );
    assert_eq!(counts.calls.load(Ordering::SeqCst), 1);
    assert_eq!(trace.rows.len(), 2);
    let after = &trace.rows[1];
    assert_eq!(after["hresult"], "0x80020009");
    assert_eq!(
        after["exception"]["source"],
        "synthetic COM object, not CAD"
    );
    assert_eq!(
        after["exception"]["description"],
        "synthetic deferred diagnostic"
    );
    assert_eq!(after["exception"]["help_context"], 42);
    assert_eq!(after["exception"]["deferred_hresult"], "0x00000001");
    assert_eq!(after["argument_error_reverse_index"], 0);
    assert_eq!(refs(&counts), 1);
    drop(object);
    assert_eq!(counts.destroyed.load(Ordering::SeqCst), 1);
}
