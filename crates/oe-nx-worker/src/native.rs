//! Fixed reviewed vendor entrypoints. No script or arbitrary symbol dispatch.
use crate::geometry::{
    CONTROLLED_REFSET, Component, ControlledReferenceSet, ExactBounds, Expression,
    JT_MAPPING_SCHEMA, JtMapping, NativeApi, PartInfo, REPRESENTATION_BASELINE,
    require_controlled_refset,
};
use anyhow::{Context, Result, bail, ensure};
use oe_nx_protocol::{BlobIdentity, RuntimeIdentity};
use oe_windows_controller::{
    adapter,
    windows::{PinnedFile, StableRoot},
};
use serde_json::{Value, json};
use std::ffi::{CString, OsStr, c_char, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
mod assembly;
mod assembly_save_capture2;
mod component_copy;
mod feature_delete;
mod mass_probe;
mod motion;
mod motion_capture2;

type Handle = *mut c_void;
type Tag = u32;
type List = *mut c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetDefaultDllDirectories(flags: u32) -> i32;
    fn AddDllDirectory(path: *const u16) -> Handle;
    fn LoadLibraryExW(path: *const u16, file: Handle, flags: u32) -> Handle;
    fn GetProcAddress(module: Handle, name: *const u8) -> *mut c_void;
    fn GetModuleFileNameW(module: Handle, path: *mut u16, count: u32) -> u32;
    fn GetLastError() -> u32;
}
fn wide(v: impl AsRef<OsStr>) -> Vec<u16> {
    v.as_ref().encode_wide().chain(Some(0)).collect()
}
fn text(v: &str) -> Result<CString> {
    ensure!(v.is_ascii(), "controlled_case_requires_ascii_argument");
    CString::new(v).context("native_string_nul")
}
fn path_text(v: &Path) -> Result<CString> {
    text(v.to_str().context("native_path_not_unicode")?)
}
struct Library {
    module: Handle,
    _expected: PinnedFile,
    _loaded: PinnedFile,
}
fn plain_path(path: &Path) -> Result<PathBuf> {
    let text = path.to_str().context("native_path_not_unicode")?;
    Ok(PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(text)))
}
fn pin_library(path: &Path) -> Result<PinnedFile> {
    ensure!(path.is_absolute(), "native_library_path_not_absolute");
    let parent = StableRoot::open(
        path.parent().context("native_library_parent_missing")?,
        None,
    )?;
    let parent_path = plain_path(parent.path())?;
    let above = StableRoot::open(
        parent_path
            .parent()
            .context("native_library_parent_root_missing")?,
        None,
    )?;
    let directory = above.directory(
        parent_path
            .file_name()
            .context("native_library_parent_name_missing")?,
    )?;
    let pin = directory
        .pin_file(
            path.file_name().context("native_library_name_missing")?,
            64 * 1024 * 1024,
        )?
        .context("native_library_missing")?;
    parent.verify()?;
    Ok(pin)
}
fn preserve_observed_failure(observed: Result<()>, recorded: Result<()>) -> Result<()> {
    match (observed, recorded) {
        (Err(error), Err(record_error)) => Err(error.context(format!(
            "native_observation_not_persisted: {record_error:#}"
        ))),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), recorded) => recorded.context("native_observation_persistence"),
    }
}
fn verify_loaded_library(
    pinned: &mut PinnedFile,
    actual_path: &Path,
    expected: &BlobIdentity,
    observe: &mut dyn FnMut(&str, Value) -> Result<()>,
) -> Result<PinnedFile> {
    let mut loaded = pin_library(actual_path)?;
    let actual_bytes = adapter::file_identity(&mut loaded)?;
    let expected_after = adapter::file_identity(pinned)?;
    let same_file = pinned.identity() == loaded.identity();
    let recorded = observe(
        "library_identity_observed",
        json!({"expected_final_path":pinned.path(),"module_path":actual_path,"loaded_final_path":loaded.path(),"volume_id":loaded.identity().volume_id,"file_id":loaded.identity().file_id,"same_file_identity":same_file,"expected":expected,"loaded":actual_bytes,"expected_after":expected_after,"transitive_dll_identity_scope":"not_verified"}),
    );
    let observed = if same_file && actual_bytes == *expected && expected_after == *expected {
        Ok(())
    } else {
        Err(anyhow::anyhow!("native_loaded_module_identity_differs"))
    };
    preserve_observed_failure(observed, recorded)?;
    Ok(loaded)
}
impl Library {
    fn load(
        path: &Path,
        expected: &BlobIdentity,
        observe: &mut dyn FnMut(&str, Value) -> Result<()>,
    ) -> Result<Self> {
        let mut pinned = pin_library(path)?;
        let bytes = pinned.read_all(64 * 1024 * 1024)?;
        expected.verify(&bytes)?;
        let offset = u32::from_le_bytes(
            bytes
                .get(0x3c..0x40)
                .context("missing_pe_offset")?
                .try_into()?,
        ) as usize;
        ensure!(
            bytes.get(offset..offset + 4) == Some(b"PE\0\0")
                && bytes.get(offset + 4..offset + 6) == Some(&[0x64, 0x86]),
            "native_library_not_x64_pe"
        );
        observe(
            "library_load_intent",
            json!({"requested_path":path,"expected":expected,"expected_final_path":pinned.path(),"volume_id":pinned.identity().volume_id,"file_id":pinned.identity().file_id,"vendor_dllmain_may_execute":true,"explicit_uf_calls":0}),
        )?;
        let module =
            unsafe { LoadLibraryExW(wide(path).as_ptr(), null_mut(), 0x100 | 0x800 | 0x400) };
        let error = if module.is_null() {
            Some(unsafe { GetLastError() })
        } else {
            None
        };
        let recorded = observe(
            "library_load_returned",
            json!({"requested_path":path,"module_nonnull":!module.is_null(),"win32_error":error}),
        );
        let observed = match error {
            Some(code) => Err(anyhow::anyhow!("native_library_load_win32_{code}")),
            None => Ok(()),
        };
        preserve_observed_failure(observed, recorded)?;
        let mut name = vec![0u16; 32768];
        let count =
            unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
        ensure!(
            count > 0 && count < name.len(),
            "native_loaded_path_missing"
        );
        let actual = String::from_utf16(&name[..count])?;
        observe(
            "library_path_observed",
            json!({"requested_path":path,"module_path":actual}),
        )?;
        let loaded = verify_loaded_library(&mut pinned, Path::new(&actual), expected, observe)?;
        // Both normal-file handles and their ancestor pins are retained through
        // explicit termination. Text aliases alone never establish identity.
        Ok(Self {
            module,
            _expected: pinned,
            _loaded: loaded,
        })
    }
    fn symbol(&self, name: &'static [u8]) -> Result<*mut c_void> {
        let p = unsafe { GetProcAddress(self.module, name.as_ptr()) };
        ensure!(!p.is_null(), "fixed_native_export_missing");
        Ok(p)
    }
}
#[cfg(test)]
mod file_identity_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "tests/.loader-file-identity-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            std::fs::create_dir(path.join("sub")).unwrap();
            std::fs::write(
                path.join("sub/pinned.dll"),
                b"file-identity-fixture; not a DLL",
            )
            .unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // These exercise real Windows file identity, without loading any DLL or NX.
    #[test]
    fn slash_and_extended_path_aliases_resolve_to_the_same_pinned_file() {
        let fixture = Fixture::new();
        let requested = fixture.0.join("sub/pinned.dll");
        let mut pin = pin_library(&requested).unwrap();
        let expected = adapter::file_identity(&mut pin).unwrap();
        let actual = std::fs::canonicalize(&requested).unwrap();
        assert_ne!(requested.to_str(), actual.to_str());
        let mut events = Vec::new();
        let loaded = verify_loaded_library(&mut pin, &actual, &expected, &mut |_, event| {
            events.push(event);
            Ok(())
        })
        .unwrap();
        assert_eq!(pin.identity(), loaded.identity());
        assert_eq!(events[0]["same_file_identity"], true);
    }

    #[test]
    fn identical_bytes_at_another_file_identity_do_not_authorize_a_module() {
        let fixture = Fixture::new();
        let requested = fixture.0.join("sub/pinned.dll");
        let copy = fixture.0.join("copy.dll");
        std::fs::copy(&requested, &copy).unwrap();
        let mut pin = pin_library(&requested).unwrap();
        let expected = adapter::file_identity(&mut pin).unwrap();
        let mut events = Vec::new();
        let result = verify_loaded_library(&mut pin, &copy, &expected, &mut |_, event| {
            events.push(event);
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(events[0]["same_file_identity"], false);
        assert_eq!(events[0]["expected"], events[0]["loaded"]);
    }

    #[test]
    fn same_file_with_a_different_expected_digest_is_rejected() {
        let fixture = Fixture::new();
        let requested = fixture.0.join("sub/pinned.dll");
        let mut pin = pin_library(&requested).unwrap();
        let mut expected = adapter::file_identity(&mut pin).unwrap();
        expected.sha256 = oe_nx_protocol::Digest::of(b"different source");
        let result = verify_loaded_library(&mut pin, &requested, &expected, &mut |_, _| Ok(()));
        assert!(result.is_err());
    }

    #[test]
    fn returned_pin_keeps_file_and_ancestors_locked_until_drop() {
        let fixture = Fixture::new();
        let requested = fixture.0.join("sub/pinned.dll");
        let pin = pin_library(&requested).unwrap();
        // pin_library's temporary roots/directories have already been dropped.
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&requested)
                .is_err()
        );
        assert!(std::fs::rename(fixture.0.join("sub"), fixture.0.join("moved")).is_err());
        pin.verify().unwrap();
        drop(pin);
        let writable = std::fs::OpenOptions::new()
            .write(true)
            .open(&requested)
            .unwrap();
        drop(writable);
        std::fs::rename(fixture.0.join("sub"), fixture.0.join("moved")).unwrap();
    }

    #[test]
    fn observed_failure_survives_a_failed_durable_record() {
        let error = preserve_observed_failure(
            Err(anyhow::anyhow!("native_library_load_win32_126")),
            Err(anyhow::anyhow!("disk_full")),
        )
        .unwrap_err();
        let chain = format!("{error:#}");
        assert!(chain.contains("native_library_load_win32_126"));
        assert!(chain.contains("disk_full"));
        assert_eq!(
            error.root_cause().to_string(),
            "native_library_load_win32_126"
        );
    }

    #[test]
    fn missing_durable_observation_rejects_an_otherwise_successful_step() {
        assert!(preserve_observed_failure(Ok(()), Err(anyhow::anyhow!("disk_full"))).is_err());
    }
}
#[repr(C)]
#[derive(Default, Debug)]
pub struct Abi {
    version: u32,
    structure_bytes: u32,
    tag_bytes: u32,
    logical_bytes: u32,
    load_status_bytes: u32,
    load_status_failed: u32,
    load_status_abort: u32,
    load_status_count: u32,
    load_status_names: u32,
    load_status_codes: u32,
    metric: i32,
    english: i32,
    solid_type: i32,
    solid_body: i32,
    sheet_body: i32,
    planar_face: i32,
    null_sign: i32,
    step_ap214: i32,
    step_existing_part: i32,
    step_entire_part: i32,
    step_new_part: i32,
    load_from_directory: i32,
    load_all: i32,
    max_path_bytes: u32,
    max_refset_bytes: u32,
    max_instance_bytes: u32,
    jt_observation_version: u32,
    jt_observation_bytes: u32,
    jt_observation_offsets: [u32; 12],
    faceted_type: i32,
    jt_body_subtype: i32,
}
#[repr(C)]
#[derive(Default)]
struct BodyJtObservation {
    version: u32,
    structure_bytes: u32,
    owner: u32,
    solid: u32,
    solid_owner: u32,
    call_entered: u32,
    call_returned: u32,
    has_jt: u32,
    jt: u32,
    up_to_date: u32,
    complete: u32,
    exception_code: i32,
}
impl BodyJtObservation {
    fn offsets() -> [u32; 12] {
        [
            std::mem::offset_of!(Self, version) as u32,
            std::mem::offset_of!(Self, structure_bytes) as u32,
            std::mem::offset_of!(Self, owner) as u32,
            std::mem::offset_of!(Self, solid) as u32,
            std::mem::offset_of!(Self, solid_owner) as u32,
            std::mem::offset_of!(Self, call_entered) as u32,
            std::mem::offset_of!(Self, call_returned) as u32,
            std::mem::offset_of!(Self, has_jt) as u32,
            std::mem::offset_of!(Self, jt) as u32,
            std::mem::offset_of!(Self, up_to_date) as u32,
            std::mem::offset_of!(Self, complete) as u32,
            std::mem::offset_of!(Self, exception_code) as u32,
        ]
    }
    fn valid_header(&self, owner: Tag, solid: Tag) -> bool {
        self.version == 1
            && self.structure_bytes == std::mem::size_of::<Self>() as u32
            && self.owner == owner
            && self.solid == solid
            && [
                self.call_entered,
                self.call_returned,
                self.has_jt,
                self.up_to_date,
                self.complete,
            ]
            .into_iter()
            .all(|v| v <= 1)
    }
    fn returned(&self, code: i32, owner: Tag, solid: Tag) -> bool {
        self.valid_header(owner, solid)
            && code == 0
            && self.exception_code == 0
            && self.complete == 1
            && self.call_entered == 1
            && self.call_returned == 1
            && self.solid_owner == owner
            && ((self.has_jt == 0 && self.jt == 0) || (self.has_jt == 1 && self.jt != 0))
    }
}
#[repr(C)]
#[derive(Default)]
struct LoadStatus {
    failed: u8,
    user_abort: u8,
    n_parts: i32,
    file_names: *mut *mut c_char,
    statuses: *mut i32,
}
fn valid_load_status_logicals(status: &LoadStatus) -> Result<()> {
    ensure!(
        status.failed <= 1 && status.user_abort <= 1,
        "native_load_status_logical_invalid: failed_raw={}; user_abort_raw={}",
        status.failed,
        status.user_abort
    );
    Ok(())
}
// Copy vendor-owned status data before UF_PART_free_load_status invalidates it.
// The bounded copy is diagnostic only; it does not turn warnings or failure into success.
unsafe fn load_status_snapshot(status: &LoadStatus, name_limit: usize) -> Result<Value> {
    ensure!(
        (0..=4096).contains(&status.n_parts),
        "native_load_status_count_invalid"
    );
    ensure!(
        (1..=65536).contains(&name_limit),
        "native_load_status_name_limit_invalid"
    );
    let mut parts = Vec::new();
    let mut total_name_bytes = 0usize;
    if status.n_parts > 0 {
        ensure!(
            !status.file_names.is_null() && !status.statuses.is_null(),
            "native_load_status_arrays_missing"
        );
    }
    for i in 0..status.n_parts as usize {
        let name = unsafe { *status.file_names.add(i) };
        ensure!(!name.is_null(), "native_load_status_filename_missing");
        let mut raw = Vec::new();
        let mut terminated = false;
        for j in 0..name_limit {
            let byte = unsafe { *name.add(j) } as u8;
            if byte == 0 {
                terminated = true;
                break;
            }
            raw.push(byte);
            total_name_bytes += 1;
            ensure!(
                total_name_bytes <= 131072,
                "native_load_status_names_exceed_record_budget"
            );
        }
        ensure!(terminated, "native_load_status_filename_unterminated");
        parts.push(json!({"filename_utf8":std::str::from_utf8(&raw).ok(),"filename_bytes":raw,"status_code":unsafe{*status.statuses.add(i)}}));
    }
    Ok(
        json!({"failed":status.failed,"user_abort":status.user_abort,"n_parts":status.n_parts,"parts":parts}),
    )
}
fn queue_load_observation(
    queue: &mut Vec<Value>,
    used: &mut usize,
    mut observation: Value,
) -> Result<bool> {
    const BUDGET: usize = 512 * 1024;
    let raw = serde_json::to_vec(&observation)?;
    let complete = raw.len().checked_add(*used).is_some_and(|n| n <= BUDGET);
    if !complete {
        observation = json!({"operation":observation["operation"],"return_code":observation["return_code"],"failed":observation["failed"],"user_abort":observation["user_abort"],"n_parts":observation["n_parts"],"free_status_return_code":observation["free_status_return_code"],"diagnostics_complete":false,"omitted_observation":BlobIdentity::of(&raw),"reason":"native_load_status_total_budget_exceeded"});
    }
    *used += serde_json::to_vec(&observation)?.len();
    queue.push(observation);
    // The caller stops the operation on overflow; at most one small summary
    // follows the total budget. It never reports the omitted details as complete.
    Ok(complete)
}
#[cfg(test)]
mod load_status_tests {
    use super::*;

    #[test]
    fn noncanonical_load_status_logicals_are_preserved_and_refused() {
        for (failed, user_abort) in [(0, 3), (3, 0), (255, 1), (1, 255)] {
            let status = LoadStatus {
                failed,
                user_abort,
                ..LoadStatus::default()
            };
            let copy = unsafe { load_status_snapshot(&status, 128) }.unwrap();
            assert_eq!(copy["failed"], failed);
            assert_eq!(copy["user_abort"], user_abort);
            let error = valid_load_status_logicals(&status).unwrap_err().to_string();
            assert!(error.starts_with("native_load_status_logical_invalid:"));
        }
        for failed in [0, 1] {
            for user_abort in [0, 1] {
                valid_load_status_logicals(&LoadStatus {
                    failed,
                    user_abort,
                    ..LoadStatus::default()
                })
                .unwrap();
            }
        }
    }

    #[test]
    fn load_failure_names_and_codes_are_owned_before_vendor_release() {
        let name = CString::new("part.prt").unwrap();
        let mut names = [name.as_ptr() as *mut c_char];
        let mut codes = [640058];
        let status = LoadStatus {
            failed: 1,
            user_abort: 0,
            n_parts: 1,
            file_names: names.as_mut_ptr(),
            statuses: codes.as_mut_ptr(),
        };
        let copy = unsafe { load_status_snapshot(&status, 128) }.unwrap();
        drop(name);
        codes[0] = 0;
        assert_eq!(copy["failed"], 1);
        assert_eq!(copy["parts"][0]["filename_utf8"], "part.prt");
        assert_eq!(copy["parts"][0]["status_code"], 640058);
        assert_eq!(codes[0], 0);
    }

    #[test]
    fn undecodable_vendor_name_keeps_raw_bytes() {
        let mut name = [255u8, 0];
        let mut names = [name.as_mut_ptr().cast::<c_char>()];
        let mut codes = [123];
        let status = LoadStatus {
            failed: 1,
            user_abort: 1,
            n_parts: 1,
            file_names: names.as_mut_ptr(),
            statuses: codes.as_mut_ptr(),
        };
        let copy = unsafe { load_status_snapshot(&status, 2) }.unwrap();
        assert!(copy["parts"][0]["filename_utf8"].is_null());
        assert_eq!(copy["parts"][0]["filename_bytes"], json!([255]));
        assert_eq!(copy["user_abort"], 1);
    }

    #[test]
    fn invalid_vendor_count_or_missing_arrays_remain_errors() {
        for count in [-1, 4097, 1] {
            let status = LoadStatus {
                n_parts: count,
                ..LoadStatus::default()
            };
            assert!(unsafe { load_status_snapshot(&status, 128) }.is_err());
        }
        let empty = unsafe { load_status_snapshot(&LoadStatus::default(), 128) }.unwrap();
        assert_eq!(empty["parts"], json!([]));
    }

    #[test]
    fn unterminated_name_is_not_silently_truncated() {
        let mut name = [b'a'; 4];
        let mut names = [name.as_mut_ptr().cast::<c_char>()];
        let mut codes = [123];
        let status = LoadStatus {
            n_parts: 1,
            file_names: names.as_mut_ptr(),
            statuses: codes.as_mut_ptr(),
            ..LoadStatus::default()
        };
        assert!(unsafe { load_status_snapshot(&status, 4) }.is_err());
    }

    #[test]
    fn cumulative_serialized_diagnostics_cannot_fill_the_result_record() {
        let mut queue = Vec::new();
        let mut used = 0;
        let observation = json!({"operation":"fixture","failed":true,"diagnostics_complete":true,"parts":"x".repeat(200_000)});
        assert!(queue_load_observation(&mut queue, &mut used, observation.clone()).unwrap());
        assert!(queue_load_observation(&mut queue, &mut used, observation.clone()).unwrap());
        assert!(!queue_load_observation(&mut queue, &mut used, observation).unwrap());
        assert!(used < 513 * 1024);
        assert_eq!(queue[2]["failed"], true);
        assert_eq!(queue[2]["diagnostics_complete"], false);
        assert!(queue[2]["omitted_observation"]["bytes"].as_u64().unwrap() > 200_000);
    }
}
#[repr(C)]
struct StepExport {
    structure_bytes: u32,
    application_protocol: i32,
    export_from: i32,
    selection_scope: i32,
    solids: u32,
    file_save: u32,
    process_hold: u32,
    input: *const c_char,
    output: *const c_char,
    layers: *const c_char,
}
#[repr(C)]
struct StepImport {
    structure_bytes: u32,
    import_to: i32,
    solids: u32,
    file_open: u32,
    process_hold: u32,
    input: *const c_char,
    output: *const c_char,
}
macro_rules! functions{
    ($($field:ident:$ty:ty=$symbol:literal),*$(,)?)=>{
        struct Uf{$($field:$ty,)*}
        impl Uf{fn load(l:&Library)->Result<Self>{Ok(Self{$($field:unsafe{std::mem::transmute::<*mut c_void,$ty>(l.symbol(concat!($symbol,"\0").as_bytes())?)},)*})}}
    }
}
functions! {
    set_work:unsafe extern "C" fn(Tag)->i32="UF_ASSEM_set_work_part",
    object_delete:unsafe extern "C" fn(Tag)->i32="UF_OBJ_delete_object",
    cylinder:unsafe extern "C" fn(i32,*mut f64,*mut c_char,*mut c_char,*mut f64,*mut Tag)->i32="UF_MODL_create_cyl1",
    feature_body:unsafe extern "C" fn(Tag,*mut Tag)->i32="UF_MODL_ask_feat_body",
    body_features:unsafe extern "C" fn(Tag,*mut List)->i32="UF_MODL_ask_body_feats",
    feature_faces:unsafe extern "C" fn(Tag,*mut List)->i32="UF_MODL_ask_feat_faces",
    feature_type:unsafe extern "C" fn(Tag,*mut *mut c_char)->i32="UF_MODL_ask_feat_type",
    feature_location:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_MODL_ask_feat_location",
    list_create:unsafe extern "C" fn(*mut List)->i32="UF_MODL_create_list",
    list_put:unsafe extern "C" fn(List,Tag)->i32="UF_MODL_put_list_item",
    feature_delete:unsafe extern "C" fn(List)->i32="UF_MODL_delete_feature",
    unite_bodies:unsafe extern "C" fn(Tag,Tag)->i32="UF_MODL_unite_bodies",
    face_body:unsafe extern "C" fn(Tag,*mut Tag)->i32="UF_MODL_ask_face_body",
    face_edges:unsafe extern "C" fn(Tag,*mut List)->i32="UF_MODL_ask_face_edges",
    face_data:unsafe extern "C" fn(Tag,*mut i32,*mut f64,*mut f64,*mut f64,*mut f64,*mut f64,*mut i32)->i32="UF_MODL_ask_face_data",
    drawings:unsafe extern "C" fn(*mut i32,*mut *mut Tag)->i32="UF_DRAW_ask_drawings",
    drawing_views:unsafe extern "C" fn(Tag,*mut i32,*mut *mut Tag)->i32="UF_DRAW_ask_views",
    drawing_open:unsafe extern "C" fn(Tag)->i32="UF_DRAW_open_drawing",
    drawing_update:unsafe extern "C" fn(Tag,Tag)->i32="UF_DRAW_update_one_view",
    drawing_stale:unsafe extern "C" fn(Tag,*mut u8)->i32="UF_DRAW_is_object_out_of_date",
    view_named:unsafe extern "C" fn(*const c_char,*mut Tag)->i32="UF_VIEW_ask_tag_of_view_name",
    view_type:unsafe extern "C" fn(Tag,*mut i32,*mut i32)->i32="UF_VIEW_ask_type",
    view_rotation:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_VIEW_ask_rotation",
    drawing_scale:unsafe extern "C" fn(Tag,*mut Tag,*mut f64)->i32="UF_DRAW_ask_view_scale",
    drawing_reference:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_DRAW_ask_drawing_ref_pt",
    drawing_move_view:unsafe extern "C" fn(Tag,*const f64)->i32="UF_DRAW_move_view",
    drawing_anchor:unsafe extern "C" fn(Tag,*mut Tag)->i32="UF_DRAW_ask_view_anchor",
    set_drawing_anchor:unsafe extern "C" fn(Tag,Tag)->i32="UF_DRAW_set_view_anchor",
    point_data:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_CURVE_ask_point_data",
    smart_point:unsafe extern "C" fn(Tag,*mut u8)->i32="UF_SO_is_so",
    view_center:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_VIEW_ask_center",
    set_view_center:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_VIEW_set_center",
    model_to_drawing:unsafe extern "C" fn(Tag,*mut f64,*mut f64)->i32="UF_VIEW_map_model_to_drawing",
    set_display_part:unsafe extern "C" fn(Tag)->i32="UF_PART_set_display_part",
    display_part:unsafe extern "C" fn()->Tag="UF_PART_ask_display_part",
    work_part:unsafe extern "C" fn()->Tag="UF_ASSEM_ask_work_part",
    set_drawing_display_state:unsafe extern "C" fn(i32)->i32="UF_DRAW_set_display_state",
    drawing_display_state:unsafe extern "C" fn(*mut i32)->i32="UF_DRAW_ask_display_state",
    current_drawing:unsafe extern "C" fn(*mut Tag)->i32="UF_DRAW_ask_current_drawing",
    work_view:unsafe extern "C" fn(*mut Tag)->i32="UF_VIEW_ask_work_view",
    drawing_borders:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_DRAW_ask_view_borders",
    drawing_define_boundary:unsafe extern "C" fn(Tag,i32,*mut Tag)->i32="UF_DRAW_define_bound_by_objects",
    drawing_boundary_objects:unsafe extern "C" fn(Tag,*mut i32,*mut *mut Tag)->i32="UF_DRAW_ask_bound_by_objects",
    drawing_boundary_type:unsafe extern "C" fn(Tag,*mut i32)->i32="UF_DRAW_ask_boundary_type",
    drawing_model_part:unsafe extern "C" fn(Tag,*mut c_char)->i32="UF_DRAW_get_view_model_view_part",
    drawing_of_view:unsafe extern "C" fn(Tag,*mut Tag)->i32="UF_DRAW_ask_drawing_of_view",
    view_visible:unsafe extern "C" fn(Tag,*mut i32,*mut *mut Tag,*mut i32,*mut *mut Tag)->i32="UF_VIEW_ask_visible_objects",
    annotation_retained:unsafe extern "C" fn(Tag,*mut u8)->i32="UF_DRF_is_annotation_retained",
    initialize:unsafe extern "C" fn()->i32="UF_initialize",
    initialized:unsafe extern "C" fn()->i32="UF_is_initialized",
    terminate:unsafe extern "C" fn()->i32="UF_terminate",
    message:unsafe extern "C" fn(i32,*mut c_char)->i32="UF_get_fail_message",
    num_parts:unsafe extern "C" fn()->i32="UF_PART_ask_num_parts",
    new_part:unsafe extern "C" fn(*const c_char,i32,*mut Tag)->i32="UF_PART_new",
    open_part:unsafe extern "C" fn(*const c_char,*mut Tag,*mut LoadStatus)->i32="UF_PART_open",
    free_status:unsafe extern "C" fn(*mut LoadStatus)->i32="UF_PART_free_load_status",
    save:unsafe extern "C" fn()->i32="UF_PART_save",
    units:unsafe extern "C" fn(Tag,*mut i32)->i32="UF_PART_ask_units",
    part_name:unsafe extern "C" fn(Tag,*mut c_char)->i32="UF_PART_ask_part_name",
    nth_part:unsafe extern "C" fn(i32)->Tag="UF_PART_ask_nth_part",
    load_state:unsafe extern "C" fn(Tag,*mut i32)->i32="UF_ask_load_state_of_part",
    save_all:unsafe extern "C" fn(*mut i32,*mut *mut Tag,*mut *mut i32)->i32="UF_PART_save_all",
    block:unsafe extern "C" fn(i32,*mut f64,*mut *mut c_char,*mut Tag)->i32="UF_MODL_create_block1",
    cycle:unsafe extern "C" fn(Tag,i32,*mut Tag)->i32="UF_OBJ_cycle_objs_in_part",
    object_type:unsafe extern "C" fn(Tag,*mut i32,*mut i32)->i32="UF_OBJ_ask_type_and_subtype",
    body_type:unsafe extern "C" fn(Tag,*mut i32)->i32="UF_MODL_ask_body_type",
    owning_part:unsafe extern "C" fn(Tag,*mut Tag)->i32="UF_OBJ_ask_owning_part",
    object_name:unsafe extern "C" fn(Tag,*mut c_char)->i32="UF_OBJ_ask_name",
    bbox:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_MODL_ask_bounding_box",
    // Exact NX 2412 uf_modl_utilities.h signature; directions is double[3][3].
    bbox_exact:unsafe extern "C" fn(Tag,Tag,*mut f64,*mut [f64;3],*mut f64)->i32="UF_MODL_ask_bounding_box_exact",
    // Exact NX 2412 uf_ps.h signature; caller owns and releases returned list.
    import_parasolid:unsafe extern "C" fn(*mut c_char,*mut List)->i32="UF_PS_import_data",
    body_faces:unsafe extern "C" fn(Tag,*mut List)->i32="UF_MODL_ask_body_faces",
    body_edges:unsafe extern "C" fn(Tag,*mut List)->i32="UF_MODL_ask_body_edges",
    face_type:unsafe extern "C" fn(Tag,*mut i32)->i32="UF_MODL_ask_face_type",
    edge_vertices:unsafe extern "C" fn(Tag,*mut f64,*mut f64,*mut i32)->i32="UF_MODL_ask_edge_verts",
    list_count:unsafe extern "C" fn(List,*mut i32)->i32="UF_MODL_ask_list_count",
    list_item:unsafe extern "C" fn(List,i32,*mut Tag)->i32="UF_MODL_ask_list_item",
    list_delete:unsafe extern "C" fn(*mut List)->i32="UF_MODL_delete_list",
    expressions:unsafe extern "C" fn(Tag,*mut i32,*mut *mut Tag)->i32="UF_MODL_ask_exps_of_part",
    expression_string:unsafe extern "C" fn(Tag,*mut *mut c_char)->i32="UF_MODL_ask_exp_tag_string",
    expression_value:unsafe extern "C" fn(Tag,*mut f64)->i32="UF_MODL_ask_exp_tag_value",
    free:unsafe extern "C" fn(*mut c_void)="UF_free",
    root_occurrence:unsafe extern "C" fn(Tag)->Tag="UF_ASSEM_ask_root_part_occ",
    children:unsafe extern "C" fn(Tag,*mut *mut Tag)->i32="UF_ASSEM_ask_part_occ_children",
    prototype:unsafe extern "C" fn(Tag)->Tag="UF_ASSEM_ask_prototype_of_occ",
    component_data:unsafe extern "C" fn(Tag,*mut c_char,*mut c_char,*mut c_char,*mut f64,*mut f64,*mut f64)->i32="UF_ASSEM_ask_component_data",
    all_children:unsafe extern "C" fn(Tag,*mut *mut Tag)->i32="UF_ASSEM_ask_all_part_occ_children",
    instance:unsafe extern "C" fn(Tag)->Tag="UF_ASSEM_ask_inst_of_part_occ",
    parent_of_instance:unsafe extern "C" fn(Tag)->Tag="UF_ASSEM_ask_parent_of_instance",
    child_of_instance:unsafe extern "C" fn(Tag)->Tag="UF_ASSEM_ask_child_of_instance",
    root_transform:unsafe extern "C" fn(Tag,Tag,*mut [f64;4])->i32="UF_ASSEM_ask_comp_position",
    is_occurrence:unsafe extern "C" fn(Tag)->u8="UF_ASSEM_is_occurrence",
    find_occurrence:unsafe extern "C" fn(Tag,Tag)->Tag="UF_ASSEM_find_occurrence",
    motion_update:unsafe extern "C" fn()->i32="UF_MODL_update",
    interference:unsafe extern "C" fn(Tag,i32,*mut Tag,*mut i32)->i32="UF_MODL_check_interference",
    containing_occurrence:unsafe extern "C" fn(Tag)->Tag="UF_ASSEM_ask_part_occurrence",
    mass_props:unsafe extern "C" fn(*mut Tag,i32,i32,i32,f64,i32,*mut f64,*mut f64,*mut f64)->i32="UF_MODL_ask_mass_props_3d",
    add_entire:unsafe extern "C" fn(Tag,*const c_char,*const c_char,*const c_char,*mut f64,*mut f64,i32,*mut Tag,*mut LoadStatus)->i32="UF_ASSEM_add_part_to_assembly2",
}
// Diagnostic readers only: these queries do not grant acceptance or change members.
// Exact signatures/subtype are from the captured NX 2412 uf_obj.h and
// uf_object_types.h. The name capacity comes from the existing compiled
// UF_OBJ_NAME_BUFSIZE ABI field (max_refset_bytes), not an assumed Rust constant.
struct MemberQueries {
    object_type: unsafe extern "C" fn(Tag, *mut i32, *mut i32) -> i32,
    owning_part: unsafe extern "C" fn(Tag, *mut Tag) -> i32,
    object_name: unsafe extern "C" fn(Tag, *mut c_char) -> i32,
    body_type: unsafe extern "C" fn(Tag, *mut i32) -> i32,
}
impl MemberQueries {
    fn observe(&self, tag: Tag, expected_owner: Tag, abi: &Abi) -> Value {
        if tag == 0 || !(1..=65536).contains(&abi.max_refset_bytes) {
            return json!({"tag":tag,"capture_complete":false,"reason":"invalid_member_tag_or_name_capacity","is_solid_body":null,"queries_executed":false});
        }
        let (mut kind, mut subtype, mut owner) = (0, 0, 0);
        let type_code = unsafe { (self.object_type)(tag, &mut kind, &mut subtype) };
        let owner_code = unsafe { (self.owning_part)(tag, &mut owner) };
        // Nonzero sentinel preserves evidence when a vendor call fails to write
        // a terminator; initialized zeroes must not manufacture an empty name.
        let mut name = vec![0x7f as c_char; abi.max_refset_bytes as usize];
        let name_code = unsafe { (self.object_name)(tag, name.as_mut_ptr()) };
        let nul = name.iter().position(|b| *b == 0);
        let name_bytes: Vec<u8> = name[..nul.unwrap_or(name.len())]
            .iter()
            .map(|b| *b as u8)
            .collect();
        let name_utf8 = nul.and_then(|_| std::str::from_utf8(&name_bytes).ok());
        // UF_solid_body_subtype is 0 in the captured SDK. Other solid-type
        // subobjects (faces/edges) are not legal UF_MODL_ask_body_type inputs.
        let body_candidate = type_code == 0 && kind == abi.solid_type && subtype == 0;
        let mut body_kind = 0;
        let body_code = body_candidate.then(|| unsafe { (self.body_type)(tag, &mut body_kind) });
        let is_solid = if type_code != 0 {
            None
        } else if !body_candidate {
            Some(false)
        } else if body_code == Some(0) && body_kind == abi.solid_body {
            Some(true)
        } else if body_code == Some(0) && body_kind == abi.sheet_body {
            Some(false)
        } else {
            None
        };
        json!({"tag":tag,"queries_executed":true,"capture_complete":nul.is_some(),
            "type_query":{"return_code":type_code,"type_raw":kind,"subtype_raw":subtype,"values_valid":type_code==0},
            "owning_part_query":{"return_code":owner_code,"tag_raw":owner,"value_valid":owner_code==0&&owner!=0,
                "matches_expected_owner":if owner_code==0&&owner!=0 {Some(owner==expected_owner)}else{None}},
            "name_query":{"return_code":name_code,"buffer_capacity":abi.max_refset_bytes,"nul_terminated":nul.is_some(),
                "raw_bytes":name_bytes,"utf8":name_utf8,"value_valid":name_code==0&&nul.is_some(),
                "scope":"nonzero return may mean unnamed/unsupported object; raw bytes and code retained"},
            "body_query":{"called":body_candidate,"return_code":body_code,"body_type_raw":body_code.map(|_|body_kind),
                "value_valid":body_code==Some(0),"requires_type":abi.solid_type,"requires_subtype":0,
                "solid_body_constant":abi.solid_body},"is_solid_body":is_solid})
    }
}

#[repr(C)]
struct AssemAdd {
    version: u32,
    structure_bytes: u32,
    parent: u32,
    prototype_bytes: u32,
    refset_bytes: u32,
    name_bytes: u32,
    layer: i32,
    reserved: u32,
    prototype: *const c_char,
    refset: *const c_char,
    name: *const c_char,
    origin: [f64; 3],
    csys: [f64; 6],
}
#[repr(C)]
struct NxOpenLoadItem {
    code: i32,
    name: [c_char; 1024],
    description: [c_char; 1024],
}
#[repr(C)]
struct NxOpenAddObservation {
    version: u32,
    structure_bytes: u32,
    call_entered: u32,
    call_returned: u32,
    component: u32,
    status_present: u32,
    unloaded_count: i32,
    status_copied: u32,
    status_released: u32,
    complete: u32,
    exception_code: i32,
    phase: u32,
    reference_set: [c_char; 64],
    items: [NxOpenLoadItem; 32],
}
#[repr(C)]
struct RefsetObservation {
    version: u32,
    structure_bytes: u32,
    operation: u32,
    owner: u32,
    requested_body: u32,
    reference_set: u32,
    found: u32,
    initial_count: u32,
    member_count: u32,
    total_sets: u32,
    complete: u32,
    mutation_started: u32,
    members: [u32; 32],
    name: [c_char; 64],
}
const _: () = assert!(std::mem::size_of::<AssemAdd>() == 128);
const _: () = assert!(std::mem::size_of::<NxOpenAddObservation>() == 65776);
const _: () = assert!(std::mem::size_of::<RefsetObservation>() == 240);
impl NxOpenAddObservation {
    fn accepted(&self, wrapper: i32) -> bool {
        wrapper == 0
            && self.version == 1
            && self.structure_bytes == std::mem::size_of::<Self>() as u32
            && self.call_entered == 1
            && self.call_returned == 1
            && self.component != 0
            && self.status_present == 1
            && self.unloaded_count == 0
            && self.status_copied == 1
            && self.status_released == 1
            && self.complete == 1
            && self.phase == 3
            && self.exception_code == 0
            && buffer_text(&self.reference_set).is_ok_and(|s| s == CONTROLLED_REFSET)
    }
    fn observation(&self, wrapper: i32, error: &[c_char]) -> Value {
        let mut items = Vec::new();
        if self.status_copied == 1 && (0..=32).contains(&self.unloaded_count) {
            for item in &self.items[..self.unloaded_count as usize] {
                items.push(json!({"code":item.code,"part_name":buffer_text(&item.name).ok(),"description":buffer_text(&item.description).ok()}));
            }
        }
        json!({"schema":"cad-agent.nxopen-assembly-add/v1","api":"ComponentAssembly::AddComponent(NXString path)",
   "version":self.version,"structure_bytes":self.structure_bytes,"phase":self.phase,
   "call_entered":self.call_entered,"call_returned":self.call_returned,"component_tag":self.component,
   "status_present":self.status_present,"raw_unloaded_count":self.unloaded_count,
   "status_copied":self.status_copied,"status_released":self.status_released,"diagnostics_complete":self.complete,
   "status_items":items,"reference_set":buffer_text(&self.reference_set).ok(),
   "exception_code":self.exception_code,"wrapper_code":wrapper,"wrapper_error":buffer_text(error).ok(),
   "ownership":if self.call_returned==1 && self.status_present==1 {"normal_return_owned_status"} else if self.call_entered==1 {"unconfirmed_no_query_or_delete"} else {"not_acquired"},
   "automatic_retry":false})
    }
}
type AddComponentObserved = unsafe extern "C" fn(
    *const AssemAdd,
    u32,
    *mut NxOpenAddObservation,
    u32,
    *mut c_char,
    u32,
) -> i32;
type RefsetOperation =
    unsafe extern "C" fn(u32, u32, u32, *mut RefsetObservation, u32, *mut c_char, u32) -> i32;
type BodyJt = unsafe extern "C" fn(u32, u32, *mut BodyJtObservation, u32, *mut c_char, u32) -> i32;
type CreateExpression =
    unsafe extern "C" fn(u32, *const c_char, *const c_char, *mut u32, *mut c_char, u32) -> i32;
type EditExpression =
    unsafe extern "C" fn(u32, u32, *const c_char, *mut i32, *mut c_char, u32) -> i32;
type ConfigureLoad = unsafe extern "C" fn(i32, i32, u32, u32, *mut c_char, u32) -> i32;
type ExportStep = unsafe extern "C" fn(*const StepExport, *mut c_char, u32) -> i32;
type ImportStep = unsafe extern "C" fn(*const StepImport, *mut c_char, u32) -> i32;
pub struct Native {
    _libraries: Vec<Library>,
    uf: Uf,
    abi: Abi,
    create_expression: CreateExpression,
    edit_expression: EditExpression,
    configure_load: ConfigureLoad,
    add_component_observed: AddComponentObserved,
    refset_operation: RefsetOperation,
    body_jt: BodyJt,
    export_step: ExportStep,
    import_step: ImportStep,
    initialized: bool,
    load_observations: Vec<Value>,
    load_observation_bytes: usize,
    assembly_cycle_observation_count: usize,
}
#[repr(C)]
#[derive(Default)]
struct ExtensionAbi {
    version: u32,
    structure_bytes: u32,
    negative_sign: i32,
    dimension_type: i32,
    drafting_entity_type: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AnnotationBox {
    origin: [f64; 3],
    length: f64,
    height: f64,
    angle_radians: f64,
    distance: f64,
}
#[repr(C)]
#[derive(Default)]
struct AssociationAbi {
    version: u32,
    structure_bytes: u32,
    entry_bytes: u32,
    entry_offsets: [u32; 6],
    face_subtype: i32,
    edge_subtype: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Association {
    object_1: u32,
    object_2: u32,
    view: u32,
    point_type: i32,
    line_type: i32,
    position: [f64; 3],
}
#[repr(C)]
#[derive(Default)]
struct ModelAbi {
    version: u32,
    structure_bytes: u32,
    edge_bytes: u32,
    edge_offsets: [u32; 11],
}
#[repr(C)]
#[derive(Default)]
struct ModelEdge {
    initialize_code: i32,
    is_arc_code: i32,
    arc_code: i32,
    free_code: i32,
    is_arc: u32,
    is_periodic: u32,
    limits: [f64; 2],
    radius: f64,
    center: [f64; 3],
    x_axis: [f64; 3],
    y_axis: [f64; 3],
}
// Construction coordinates only. The final plan remains the acceptance contract.
fn construction_sheet_layout(
    bounds: &[[f64; 6]],
    rotation: [f64; 9],
    scale: f64,
    final_size: [f64; 2],
) -> Result<([f64; 2], [f64; 2])> {
    let center = crate::geometry::drafting::layout_anchor_from_bounds(bounds)?;
    // Reuse reviewed finite/rotation/scale validation, not its import compensation.
    crate::geometry::drafting::layout_import_reference(
        center, rotation, [0.0; 3], [0.0; 2], scale,
    )?;
    ensure!(
        final_size
            .iter()
            .all(|n| n.is_finite() && *n > 0.0 && *n <= 100_000.0),
        "native_drafting_construction_final_size_invalid"
    );
    let mut low = [0.0_f64; 2];
    let mut high = [0.0_f64; 2];
    for bound in bounds {
        for corner in 0..8 {
            let point: [f64; 3] = std::array::from_fn(|axis| {
                bound[axis + if corner & (1 << axis) == 0 { 0 } else { 3 }]
            });
            for axis in 0..2 {
                let projected = scale
                    * (0..3)
                        .map(|k| rotation[axis * 3 + k] * point[k])
                        .sum::<f64>();
                ensure!(
                    projected.is_finite(),
                    "native_drafting_construction_projection_overflow"
                );
                low[axis] = low[axis].min(projected);
                high[axis] = high[axis].max(projected);
            }
        }
    }
    let import = std::array::from_fn(|axis| final_size[axis] + (-low[axis]).max(0.0));
    let size = std::array::from_fn(|axis| import[axis] + final_size[axis] + high[axis].max(0.0));
    ensure!(
        import
            .iter()
            .chain(size.iter())
            .all(|n| n.is_finite() && *n > 0.0 && *n <= 100_000.0),
        "native_drafting_construction_size_unsupported"
    );
    Ok((import, size))
}
#[cfg(test)]
mod construction_layout_tests {
    use super::construction_sheet_layout;
    const IDENTITY: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];
    #[test]
    fn frozen_positive_negative_and_rotated_layouts_include_absolute_origin() {
        let fixtures = [
            (
                [0., 0., 0., 10., 20., 30.],
                IDENTITY,
                1.,
                [100., 50.],
                [100., 50.],
                [210., 120.],
            ),
            (
                [-20., -10., 0., -10., -5., 1.],
                IDENTITY,
                2.,
                [100., 50.],
                [140., 70.],
                [240., 120.],
            ),
            (
                [1., 2., 3., 4., 5., 6.],
                [0., -1., 0., 1., 0., 0., 0., 0., 1.],
                3.,
                [100., 50.],
                [115., 50.],
                [215., 112.],
            ),
            (
                [200., -150., 500., 230., -120., 540.],
                [0., 1., 0., 0., 0., 1., 1., 0., 0.],
                2.,
                [100., 80.],
                [400., 80.],
                [500., 1240.],
            ),
        ];
        for (bounds, rotation, scale, final_size, expected_import, expected_size) in fixtures {
            let (point, size) =
                construction_sheet_layout(&[bounds], rotation, scale, final_size).unwrap();
            for axis in 0..2 {
                assert!((point[axis] - expected_import[axis]).abs() < 1e-9);
                assert!((size[axis] - expected_size[axis]).abs() < 1e-9);
            }
        }
    }
    #[test]
    fn malformed_or_unbounded_construction_is_rejected_before_native_calls() {
        let good = [[0., 0., 0., 10., 20., 30.]];
        for scale in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(construction_sheet_layout(&good, IDENTITY, scale, [100., 50.]).is_err());
        }
        assert!(construction_sheet_layout(&[], IDENTITY, 1., [100., 50.]).is_err());
        assert!(
            construction_sheet_layout(&[[10., 0., 0., 0., 20., 30.]], IDENTITY, 1., [100., 50.])
                .is_err()
        );
        assert!(
            construction_sheet_layout(
                &good,
                [-1., 0., 0., 0., 1., 0., 0., 0., 1.],
                1.,
                [100., 50.]
            )
            .is_err()
        );
        assert!(construction_sheet_layout(&good, IDENTITY, 1., [100_000., 50.]).is_err());
        assert!(construction_sheet_layout(&good, IDENTITY, 1., [f64::NAN, 50.]).is_err());
    }
    #[test]
    fn all_solids_contribute_to_the_construction_domain() {
        let (point, size) = construction_sheet_layout(
            &[
                [0., 0., 0., 10., 20., 30.],
                [-50., -40., 0., -30., -20., 1.],
            ],
            IDENTITY,
            1.,
            [100., 50.],
        )
        .unwrap();
        assert_eq!(point, [150., 90.]);
        assert_eq!(size, [260., 160.]);
    }
}
impl Native {
    fn drafting_abi(&self) -> Result<[i32; 8]> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(*mut i32, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_drafting_abi\0")?) };
        let mut constants = [-1; 8];
        ensure!(
            unsafe { function(constants.as_mut_ptr(), 8) } == 0
                && constants[0] == 1
                && constants[1..].iter().all(|n| *n >= 0)
                && constants[2] != constants[3]
                && constants[5] != constants[6],
            "native_drafting_abi_mismatch"
        );
        Ok(constants)
    }
    fn drafting_sheet_info(&self, drawing: Tag, constants: [i32; 8]) -> Result<Value> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(Tag, *mut i32, *mut f64, *mut c_char, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_drafting_sheet_info_v2\0")?) };
        let mut raw = [-1; 6];
        let mut values = [f64::NAN; 3];
        let mut error = vec![0; 1024];
        let wrapper = unsafe {
            function(
                drawing,
                raw.as_mut_ptr(),
                values.as_mut_ptr(),
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        let unit = if raw[1] == self.abi.metric {
            Some("mm")
        } else if raw[1] == self.abi.english {
            Some("inch")
        } else {
            None
        };
        let projection = if raw[3] == constants[2] {
            Some("first")
        } else if raw[3] == constants[3] {
            Some("third")
        } else {
            None
        };
        let custom = raw[2] == constants[1] && raw[5] == 0;
        let decoded = unit.and_then(|unit| {
            crate::geometry::drafting::standard_sheet_dimensions(raw[5], raw[4], unit)
        });
        if !custom && let Some(dimensions) = decoded {
            values[0] = dimensions[0];
            values[1] = dimensions[1];
        }
        Ok(
            json!({"schema":"oev2.native-drafting-sheet-info/2","wrapper_code":wrapper,"query_code":raw[0],
            "units_raw":raw[1],"size_state_raw":raw[2],"projection_angle_raw":raw[3],
            "standard_family":raw[5],"standard_code":raw[4],
            "unit":unit,"size_state":if custom{"custom"}else if decoded.is_some(){"standard"}else{"unsupported_standard"},
            "projection_angle":projection,"width":values[0],"height":values[1],"scale":values[2],
            "value_valid":wrapper==0 && raw[0]==0 && unit.is_some() && projection.is_some()
                && (custom || decoded.is_some()) && values.iter().all(|n|n.is_finite()),
            "coordinate_basis":"UF_DRAW_info: custom size[1]/[0], or installed SDK documented standard enum dimensions",
            "error":if wrapper==0{None}else{buffer_text(&error).ok()}}),
        )
    }
    fn drafting_view_type(&self, view: Tag, constants: [i32; 8]) -> Value {
        let mut kind = -1;
        let mut subtype = -1;
        let code = unsafe { (self.uf.view_type)(view, &mut kind, &mut subtype) };
        json!({"return_code":code,"type_raw":kind,"subtype_raw":subtype,
            "kind":if code!=0{None}else if kind==constants[5]{Some("model")}
                else if kind==constants[6]{Some("drawing_member")}else{Some("other")},
            "subtype":if code==0 && subtype==constants[7]{Some("imported")}else{None}})
    }
    fn display_object(&self, part: Tag, tag: Tag, request: Option<bool>) -> Result<Value> {
        let lib = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(
            Tag,
            Tag,
            i32,
            *mut i32,
            u32,
            *mut Tag,
            *mut f64,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(lib.symbol(b"oe_nx_drawing_object_v1\0")?) };
        let mut o = [-1; 20];
        let mut tags = [0; 3];
        let mut data = [f64::NAN; 12];
        let mut err = [0i8; 512];
        let code = unsafe {
            f(
                part,
                tag,
                request.map_or(-1, i32::from),
                o.as_mut_ptr(),
                20,
                tags.as_mut_ptr(),
                data.as_mut_ptr(),
                err.as_mut_ptr(),
                512,
            )
        };
        Ok(
            json!({"tag":tag,"wrapper_code":code,"abi_version":o[0],"alive":o[1]==o[2],"status_raw":o[1],"type_raw":o[4],"subtype_raw":o[5],"owner_tag":tags[0],"owner_matches":o[6]==0&&tags[0]==part,
            "wcs_tag":tags[1],"is_wcs":if o[12]==0&&tags[1]!=0{Some(tags[1]==tag)}else{None},"is_csys":o[16]==1,
            "display_complete":code==0&&o[7]==0,"layer":o[8],"color":o[17],"font":o[18],"line_width":o[19],
            "blank":if o[7]==0&&o[9]==o[10]{Some(true)}else if o[7]==0&&o[9]==o[11]{Some(false)}else{None},
            "matrix_tag":tags[2],"origin":&data[..3],"matrix":&data[3..],"csys_complete":code==0&&o[13]==0&&o[14]==0&&data.iter().all(|v|v.is_finite()),
            "write_returned":request.is_some()&&o[15]==0,"raw_observations":o,"raw_tags":tags,"error":if code==0{None}else{buffer_text(&err).ok()}}),
        )
    }
    fn drafting_rotation(&self, view: Tag) -> Value {
        let mut matrix = [f64::NAN; 9];
        let code = unsafe { (self.uf.view_rotation)(view, matrix.as_mut_ptr()) };
        json!({"return_code":code,"matrix":matrix,"value_valid":code==0 && matrix.iter().all(|n|n.is_finite()),
            "basis":"UF_VIEW_ask_rotation; axis rows in model coordinates"})
    }
    fn drafting_visible(&self, view: Tag) -> Value {
        let (mut count, mut clipped_count) = (0, 0);
        let (mut pointer, mut clipped) = (null_mut(), null_mut());
        let code = unsafe {
            (self.uf.view_visible)(
                view,
                &mut count,
                &mut pointer,
                &mut clipped_count,
                &mut clipped,
            )
        };
        let _visible_guard = Allocation {
            p: pointer.cast(),
            free: self.uf.free,
        };
        let _clipped_guard = Allocation {
            p: clipped.cast(),
            free: self.uf.free,
        };
        let valid = code == 0
            && (0..=10_000).contains(&count)
            && (0..=10_000).contains(&clipped_count)
            && (count == 0 || !pointer.is_null())
            && (clipped_count == 0 || !clipped.is_null());
        let visible = if valid && count > 0 {
            unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec()
        } else {
            Vec::new()
        };
        let clipped_tags = if valid && clipped_count > 0 {
            unsafe { std::slice::from_raw_parts(clipped, clipped_count as usize) }.to_vec()
        } else {
            Vec::new()
        };
        json!({"return_code":code,"capture_complete":valid && visible.iter().chain(&clipped_tags).all(|tag|*tag!=0),
            "visible_count":count,"clipped_count":clipped_count,"visible_tags":visible,"clipped_tags":clipped_tags,
            "coverage":"UF_VIEW_ask_visible_objects external geometry; occlusion and visual readability not established"})
    }
    fn drafting_boundary(&self, view: Tag) -> Value {
        let mut kind = -1;
        let type_code = unsafe { (self.uf.drawing_boundary_type)(view, &mut kind) };
        let mut count = 0;
        let mut pointer = null_mut();
        let objects_code =
            unsafe { (self.uf.drawing_boundary_objects)(view, &mut count, &mut pointer) };
        let _guard = Allocation {
            p: pointer.cast(),
            free: self.uf.free,
        };
        let valid = objects_code == 0 && (1..=10_000).contains(&count) && !pointer.is_null();
        let tags = if valid {
            unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec()
        } else {
            Vec::new()
        };
        let unique = tags
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        // UF_DRAW_BOUND_BY_OBJECTS_TYPE=4 is independently asserted against the
        // installed SDK in the pinned native shim; no model geometry is hidden.
        json!({"type_query_code":type_code,"type_raw":kind,
            "kind":if type_code!=0{None}else if kind==4{Some("bound_by_objects")}else{Some("other")},
            "objects_query_code":objects_code,"capture_complete":type_code==0 && valid
                && tags.iter().all(|tag|*tag!=0) && unique.len()==tags.len(),
            "object_count":count,"object_tags":tags,
            "coverage":"actual UF_DRAW bound object inventory; complete current solid coverage requires comparison with the same part solid_tags; no annotation transfer, visual or manufacturing acceptance"})
    }
    fn drafting_view_info(&self, view: Tag, part: Tag, drawing: Tag, constants: [i32; 8]) -> Value {
        let mut expression = 0;
        let mut scale = f64::NAN;
        let scale_code = unsafe { (self.uf.drawing_scale)(view, &mut expression, &mut scale) };
        let mut reference = [f64::NAN; 2];
        let reference_code = unsafe { (self.uf.drawing_reference)(view, reference.as_mut_ptr()) };
        let mut borders = [f64::NAN; 4];
        let borders_code = unsafe { (self.uf.drawing_borders)(view, borders.as_mut_ptr()) };
        let mut owner = 0;
        let owner_code = unsafe { (self.uf.drawing_of_view)(view, &mut owner) };
        let mut name = vec![0x7f as c_char; self.abi.max_path_bytes as usize];
        let model_code = unsafe { (self.uf.drawing_model_part)(view, name.as_mut_ptr()) };
        let model_path = buffer_text(&name).ok();
        let metadata = MemberQueries {
            object_type: self.uf.object_type,
            owning_part: self.uf.owning_part,
            object_name: self.uf.object_name,
            body_type: self.uf.body_type,
        }
        .observe(view, part, &self.abi);
        json!({"metadata":metadata,"view_type":self.drafting_view_type(view,constants),
            "rotation":self.drafting_rotation(view),
            "scale":{"return_code":scale_code,"expression_tag":expression,"value":scale,"value_valid":scale_code==0 && scale.is_finite()},
            "reference_point":{"return_code":reference_code,"point":reference,"coordinate_basis":"drawing coordinates"},
            "borders":{"return_code":borders_code,"values":borders,"coordinate_basis":"drawing coordinates [xmin,ymin,xmax,ymax]"},
            "owning_drawing":{"return_code":owner_code,"drawing_tag":owner,"matches_expected":owner_code==0 && owner==drawing},
            "model_part":{"return_code":model_code,"path":model_path,"nul_terminated":name.iter().any(|n|*n==0)},
            "boundary":self.drafting_boundary(view),
            "layout_anchor":self.drafting_view_anchor(view,part),
            "visible_objects":self.drafting_visible(view)})
    }
    fn layout_lifecycle_view(&self, view: Tag) -> Value {
        // Do not call drafting_view_anchor here: it would dereference a
        // condemned point. Record the relation tag without querying that tag.
        let mut anchor = 0;
        let anchor_code = unsafe { (self.uf.drawing_anchor)(view, &mut anchor) };
        let mut reference = [f64::NAN; 2];
        let reference_code = unsafe { (self.uf.drawing_reference)(view, reference.as_mut_ptr()) };
        let mut borders = [f64::NAN; 4];
        let borders_code = unsafe { (self.uf.drawing_borders)(view, borders.as_mut_ptr()) };
        json!({"view_tag":view,"anchor_relation":{"query_code":anchor_code,"point_tag":anchor,"point_dereferenced":false},
            "reference_point":{"return_code":reference_code,"point":reference},
            "borders":{"return_code":borders_code,"values":borders},
            "boundary":self.drafting_boundary(view),"visible_objects":self.drafting_visible(view),
            "scope":"raw view queries; no point-tag coordinate or metadata query"})
    }
    fn layout_object_lifecycle(&self, object: Tag, part: Tag, point: bool) -> Result<Value> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let ask: unsafe extern "C" fn(Tag, *mut i32, *mut i32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_ask_object_state_v1\0")?) };
        let mut status = -1;
        let mut states = [-1; 4];
        let wrapper = unsafe { ask(object, &mut status, states.as_mut_ptr()) };
        let names = ["deleted", "temporary", "condemned", "alive"];
        let known = wrapper == 0
            && states
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == 4;
        let name = if known {
            states
                .iter()
                .position(|v| *v == status)
                .map(|i| names[i])
                .unwrap_or("unknown")
        } else {
            "unknown"
        };
        let mut value = json!({"schema":"oev2.native-object-lifecycle-observation/1",
            "object_tag":object,"wrapper_code":wrapper,"status_raw":status,"status":name,
            "sdk_status_constants":{"deleted":states[0],"temporary":states[1],"condemned":states[2],"alive":states[3]},
            "status_is_not_an_error_code":true,"status_observed":known && name!="unknown",
            "deep_read_executed":false,"deep_read_skip_reason":"sdk_non_alive_or_unknown_may_be_deleted_or_reused",
            "metadata":null,"point_data":null,
            "scope":"readonly lifecycle snapshot; no engineering or final drawing acceptance"});
        // The installed SDK warns that non-alive tags can disappear/reuse.
        // Only ALIVE tags receive extra metadata/coordinate queries. Retained
        // earlier anchor reader data is a separate phase, never a current read.
        if known && status == states[3] {
            let mut kind = -1;
            let mut subtype = -1;
            let type_code = unsafe { (self.uf.object_type)(object, &mut kind, &mut subtype) };
            let mut owner = 0;
            let owner_code = unsafe { (self.uf.owning_part)(object, &mut owner) };
            value["deep_read_executed"] = json!(true);
            value["deep_read_skip_reason"] = Value::Null;
            value["metadata"] = json!({"type_query_code":type_code,"type_raw":kind,"subtype_raw":subtype,
                "owner_query_code":owner_code,"owner_tag":owner,"matches_expected_owner":owner_code==0&&owner==part});
            if point {
                value["point_data"] = self.drafting_anchor_point(object, part);
            }
        }
        Ok(value)
    }
    fn layout_visibility(&self, point: Tag, part: Tag) -> Result<Value> {
        let lifecycle = self.layout_object_lifecycle(point, part, true)?;
        if lifecycle["status"] != "alive" || lifecycle["status_observed"] != true {
            return Ok(
                json!({"lifecycle":lifecycle,"query_skipped":true,"capture_complete":false,
                "reason":"visibility_and_display_properties_require_alive_object"}),
            );
        }
        let library = self._libraries.last().context("native_shim_missing")?;
        let ask: unsafe extern "C" fn(Tag, *mut i32, *mut i32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_ask_layout_visibility_v1\0")?) };
        let mut codes = [-1; 2];
        let mut values = [-1; 8];
        let wrapper = unsafe { ask(point, codes.as_mut_ptr(), values.as_mut_ptr()) };
        Ok(
            json!({"schema":"oev2.native-layout-visibility/1","point_tag":point,"lifecycle":lifecycle,
            "wrapper_code":wrapper,"query_codes":codes,"query_code_order":["UF_SO_ask_visibility_option","UF_OBJ_ask_display_properties"],
            "visibility_option_raw":values[0],"sdk_options":{"invisible":values[1],"visible":values[2],"visible_if_parent_invisible":values[3]},
            "layer":values[4],"blank_status_raw":values[5],"sdk_blank_states":{"blanked":values[6],"not_blanked":values[7]},
            "option_is_visible":wrapper==0 && codes[0]==0 && values[0]==values[2],
            "capture_complete":wrapper==0 && codes.iter().all(|code|*code==0),
            "scope":"object visibility and display properties; actual member-view visibility remains a separate query"}),
        )
    }
    fn drafting_anchor_point(&self, point: Tag, part: Tag) -> Value {
        if point == 0 {
            return json!({"point_tag":0,"point_query_code":null,"coordinates":null,
                "smart_query_code":null,"is_smart":null,"owner_query_code":null,
                "owning_part":null,"matches_expected_owner":false,"capture_complete":false});
        }
        // Diagnostic15: never add coordinate/owner/type queries on non-ALIVE
        // smart tags. State is not an API error code and numeric tags can be reused.
        let state = (|| -> Result<(i32, i32)> {
            let lib = self._libraries.last().context("native_shim_missing")?;
            let ask: unsafe extern "C" fn(Tag, *mut i32, *mut i32) -> i32 =
                unsafe { std::mem::transmute(lib.symbol(b"oe_nx_ask_object_state_v1\0")?) };
            let mut status = -1;
            let mut constants = [-1; 4];
            let code = unsafe { ask(point, &mut status, constants.as_mut_ptr()) };
            ensure!(code == 0, "diagnostic_point_status_unknown");
            Ok((status, constants[3]))
        })();
        if !state.as_ref().is_ok_and(|(status, alive)| status == alive) {
            return json!({"point_tag":point,"point_query_code":null,"coordinates":null,
                "smart_query_code":null,"is_smart":null,"owner_query_code":null,
                "owning_part":null,"matches_expected_owner":false,"capture_complete":false,
                "deep_read_skipped":true,"status_raw":state.as_ref().ok().map(|(status,_)|*status),
                "reason":"diagnostic15_non_alive_or_unknown_no_deep_read"});
        }
        let mut coordinates = [f64::NAN; 3];
        let point_code = unsafe { (self.uf.point_data)(point, coordinates.as_mut_ptr()) };
        let mut smart = 0u8;
        let smart_code = unsafe { (self.uf.smart_point)(point, &mut smart) };
        let mut owner = 0;
        let owner_code = unsafe { (self.uf.owning_part)(point, &mut owner) };
        json!({"point_tag":point,"point_query_code":point_code,"coordinates":coordinates,
            "coordinate_basis":"absolute part coordinates; layout only, not manufacturing datum or tolerance",
            "smart_query_code":smart_code,"smart_raw":smart,"is_smart":smart_code==0 && smart==1,
            "owner_query_code":owner_code,"owning_part":owner,"matches_expected_owner":owner_code==0 && owner==part,
            "capture_complete":point_code==0 && coordinates.iter().all(|v|v.is_finite())
                && smart_code==0 && smart==1 && owner_code==0 && owner==part})
    }
    fn drafting_view_anchor(&self, view: Tag, part: Tag) -> Value {
        let mut anchor = 0;
        let code = unsafe { (self.uf.drawing_anchor)(view, &mut anchor) };
        let point_data = if code == 0 {
            self.drafting_anchor_point(anchor, part)
        } else {
            Value::Null
        };
        let coordinates = point_data["coordinates"].as_array().and_then(|v| {
            let xyz: Option<Vec<f64>> = v
                .iter()
                .map(|x| x.as_f64().filter(|n| n.is_finite()))
                .collect();
            xyz.and_then(|v| <[f64; 3]>::try_from(v).ok())
        });
        let mapping=coordinates.map(|xyz|self.drafting_point_mapping(view,xyz))
            .unwrap_or_else(||json!({"return_code":null,"model_point":null,"coordinates":null,"capture_complete":false}));
        json!({"schema":"oev2.native-layout-anchor-observation/1","query_code":code,
            "point_tag":anchor,"point_data":point_data,"drawing_mapping":mapping,
            "absolute_origin_to_drawing":self.drafting_point_mapping(view,[0.0;3]),
            "role":"model smart layout anchor corresponding to drawing reference point; not engineering feature or datum"})
    }
    fn drafting_point_mapping(&self, view: Tag, point: [f64; 3]) -> Value {
        let mut input = point;
        let mut mapped = [f64::NAN; 2];
        let code =
            unsafe { (self.uf.model_to_drawing)(view, input.as_mut_ptr(), mapped.as_mut_ptr()) };
        json!({"return_code":code,"model_point":point,"coordinates":mapped,
            "capture_complete":code==0 && mapped.iter().all(|n|n.is_finite()),
            "coordinate_basis":"UF_VIEW_map_model_to_drawing: absolute model point to current member-view drawing coordinates; current plan requires equal model/drawing units"})
    }
    fn drafting_layout_reference(
        &self,
        view: Tag,
        part: Tag,
        unit: &str,
        point: [f64; 3],
    ) -> Result<Value> {
        let raw = self.drafting_point_mapping(view, point);
        let values: Option<Vec<f64>> = raw["coordinates"]
            .as_array()
            .map(|items| items.iter().map(Value::as_f64).collect::<Option<Vec<_>>>())
            .flatten();
        if let Some(values) =
            values.filter(|values| values.len() == 2 && values.iter().all(|n| n.is_finite()))
        {
            let mapping = oe_contracts::NativeModelToDrawingMapping {
                return_code: raw["return_code"]
                    .as_i64()
                    .context("native_layout_mapping_code_missing")?,
                capture_complete: raw["capture_complete"].as_bool().unwrap_or(false),
                model_point: point.to_vec(),
                coordinates: values,
            };
            let binding = oe_contracts::NativeLayoutReference {
                schema: "oev2.native-layout-reference/1".into(),
                unit: unit.into(),
                model_part_tag: u64::from(part),
                drawing_view_tag: u64::from(view),
                coordinate_basis: "absolute_current_part_bbox_midpoint".into(),
                model_coordinates: point.to_vec(),
                drawing_mapping: mapping,
            };
            return Ok(serde_json::to_value(binding)?);
        }
        // Preserve invalid/raw NULL coordinates and original code for unknown diagnostics.
        Ok(
            json!({"schema":"oev2.native-layout-reference/1","unit":unit,
            "model_part_tag":part,"drawing_view_tag":view,
            "coordinate_basis":"absolute_current_part_bbox_midpoint",
            "model_coordinates":point,"drawing_mapping":raw}),
        )
    }
    fn drafting_display_part(&self, part: Tag) -> Value {
        let code = unsafe { (self.uf.set_display_part)(part) };
        let actual = unsafe { (self.uf.display_part)() };
        json!({"return_code":code,"expected_part":part,"actual_part":actual,
            "matches_expected":code==0 && actual==part,
            "scope":"current task-owned worker headless session display state; no UI or original model mutation"})
    }
    fn drafting_display_state(&self, drawing: Tag) -> Value {
        // The installed SDK distinguishes the drafting flag from what is
        // actually displayed. Capture both actual drawing and work-view type;
        // a state flag of two alone is not visibility evidence.
        let set_code = unsafe { (self.uf.set_drawing_display_state)(2) };
        self.drafting_display_state_query(drawing, Some(set_code))
    }
    fn drafting_display_part_query(&self, part: Tag) -> Value {
        let actual = unsafe { (self.uf.display_part)() };
        json!({"return_code":null,"set_called":false,"query":"UF_PART_ask_display_part",
            "expected_part":part,"actual_part":actual,"matches_expected":actual==part && part!=0})
    }
    fn drafting_display_state_query(&self, drawing: Tag, set_code: Option<i32>) -> Value {
        let mut state = -1;
        let query_code = unsafe { (self.uf.drawing_display_state)(&mut state) };
        let mut current = 0;
        let current_code = unsafe { (self.uf.current_drawing)(&mut current) };
        let mut work = 0;
        let work_code = unsafe { (self.uf.work_view)(&mut work) };
        let mut kind = -1;
        let mut subtype = -1;
        let type_code = if work_code == 0 && work != 0 {
            Some(unsafe { (self.uf.view_type)(work, &mut kind, &mut subtype) })
        } else {
            None
        };
        let mut value = json!({"set_code":set_code,"query_code":query_code,"state_raw":state,
            "current_drawing_code":current_code,"current_drawing_tag":current,
            "work_view_code":work_code,"work_view_tag":work,
            "work_view_type":{"return_code":type_code,"type_raw":kind,"subtype_raw":subtype,
                "kind":if type_code!=Some(0){None}else if kind==2{Some("drawing_sheet")}
                    else if kind==1{Some("drawing_member")}else if kind==0{Some("model")}else{Some("other")}},
            "capture_complete":set_code.is_none_or(|code| code==0) && query_code==0 && state==2
                && current_code==0 && current==drawing && drawing!=0
                && work_code==0 && work!=0 && type_code==Some(0) && kind==2,
            "expected_drawing_tag":drawing,
            "scope":"task-owned headless drawing display precondition; flag alone does not establish visible objects"});
        if set_code.is_none() {
            value["set_called"] = json!(false);
        }
        value
    }
    fn modeling_views(&self, part: Tag, constants: [i32; 8]) -> Result<Value> {
        let mut view = 0;
        let mut seen = std::collections::BTreeSet::new();
        let mut values = Vec::new();
        let mut complete = true;
        loop {
            self.check(
                unsafe { (self.uf.cycle)(part, constants[4], &mut view) },
                "UF_OBJ_cycle_modeling_views",
            )?;
            if view == 0 {
                break;
            }
            ensure!(
                seen.insert(view) && seen.len() <= 10_000,
                "native_modeling_view_cycle_invalid"
            );
            let kind = self.drafting_view_type(view, constants);
            if kind["return_code"] != 0 {
                complete = false;
                continue;
            }
            if kind["kind"] != "model" {
                continue;
            }
            let metadata = MemberQueries {
                object_type: self.uf.object_type,
                owning_part: self.uf.owning_part,
                object_name: self.uf.object_name,
                body_type: self.uf.body_type,
            }
            .observe(view, part, &self.abi);
            complete &= metadata["owning_part_query"]["matches_expected_owner"] == true
                && metadata["name_query"]["value_valid"] == true;
            let mut center = [f64::NAN; 3];
            let center_code = unsafe { (self.uf.view_center)(view, center.as_mut_ptr()) };
            values.push(json!({"view_tag":view,"metadata":metadata,"view_type":kind,"rotation":self.drafting_rotation(view),
                "view_center":{"query_code":center_code,"coordinates":center,"coordinate_basis":"UF_VIEW_ask_center absolute CSYS; view-plane center, not drawing reference point"}}));
        }
        Ok(json!({"complete":complete,"views":values}))
    }
    fn model_abi(&self) -> Result<ModelAbi> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(*mut ModelAbi, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_model_abi\0")?) };
        let mut abi = ModelAbi::default();
        let offsets = [
            std::mem::offset_of!(ModelEdge, initialize_code),
            std::mem::offset_of!(ModelEdge, is_arc_code),
            std::mem::offset_of!(ModelEdge, arc_code),
            std::mem::offset_of!(ModelEdge, free_code),
            std::mem::offset_of!(ModelEdge, is_arc),
            std::mem::offset_of!(ModelEdge, is_periodic),
            std::mem::offset_of!(ModelEdge, limits),
            std::mem::offset_of!(ModelEdge, radius),
            std::mem::offset_of!(ModelEdge, center),
            std::mem::offset_of!(ModelEdge, x_axis),
            std::mem::offset_of!(ModelEdge, y_axis),
        ]
        .map(|n| n as u32);
        ensure!(
            unsafe { function(&mut abi, std::mem::size_of::<ModelAbi>() as u32) } == 0
                && abi.version == 1
                && abi.structure_bytes == std::mem::size_of::<ModelAbi>() as u32
                && abi.edge_bytes == std::mem::size_of::<ModelEdge>() as u32
                && abi.edge_offsets == offsets,
            "native_model_edge_abi_mismatch"
        );
        Ok(abi)
    }
    fn model_edges(&self, part: Tag, info: &PartInfo) -> Result<Value> {
        let abi = self.model_abi()?;
        let library = self._libraries.last().context("native_shim_missing")?;
        let query: unsafe extern "C" fn(Tag, *mut ModelEdge, u32, *mut c_char, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_model_edge\0")?) };
        let readers = MemberQueries {
            object_type: self.uf.object_type,
            owning_part: self.uf.owning_part,
            object_name: self.uf.object_name,
            body_type: self.uf.body_type,
        };
        let mut bodies = Vec::new();
        let mut entries = Vec::new();
        let mut complete = true;
        for body in &info.solid_tags {
            let tags = self.list(*body, self.uf.body_edges)?;
            ensure!(
                entries.len() + tags.len() <= 65536,
                "native_model_edge_budget"
            );
            let metadata = readers.observe(*body, part, &self.abi);
            complete &= metadata["owning_part_query"]["matches_expected_owner"] == true;
            bodies.push(
                json!({"body_tag":body,"metadata":metadata,"edge_list_return_code":0,
                "capture_complete":true,"edge_tags":tags}),
            );
            for edge in tags {
                let metadata = readers.observe(edge, part, &self.abi);
                let mut observation = ModelEdge::default();
                let mut error = vec![0; 1024];
                let code = unsafe {
                    query(
                        edge,
                        &mut observation,
                        std::mem::size_of::<ModelEdge>() as u32,
                        error.as_mut_ptr(),
                        error.len() as u32,
                    )
                };
                let query_complete = code == 0
                    && observation.initialize_code == 0
                    && observation.is_arc_code == 0
                    && observation.free_code == 0
                    && observation.is_arc <= 1
                    && observation.is_periodic <= 1;
                let value_valid = query_complete
                    && observation.is_arc == 1
                    && observation.arc_code == 0
                    && observation.radius.is_finite()
                    && observation.radius > 0.0
                    && observation
                        .limits
                        .iter()
                        .chain(observation.center.iter())
                        .chain(observation.x_axis.iter())
                        .chain(observation.y_axis.iter())
                        .all(|n| n.is_finite());
                complete &= query_complete
                    && (observation.is_arc == 0 || value_valid)
                    && metadata["owning_part_query"]["matches_expected_owner"] == true;
                entries.push(json!({"body_tag":body,"edge_tag":edge,"metadata":metadata,
                    "query":{"wrapper_code":code,"initialize_code":observation.initialize_code,
                        "is_arc_code":observation.is_arc_code,"arc_code":observation.arc_code,
                        "free_code":observation.free_code,"capture_complete":query_complete,
                        "is_arc":if query_complete{Some(observation.is_arc==1)}else{None}},
                    "arc":if value_valid{Some(json!({"value_valid":true,"is_periodic":observation.is_periodic==1,
                        "limits":observation.limits,"radius":observation.radius,"center":observation.center,
                        "x_axis":observation.x_axis,"y_axis":observation.y_axis}))}else{None},
                    "error":if code==0{None}else{buffer_text(&error).ok()}}));
            }
        }
        Ok(
            json!({"schema":"oev2.nx-model-edge-observation/1","complete":complete,
            "unit":info.units,"coordinate_basis":"UF_EVAL ABS coordinates; single-part solid edges",
            "solid_type_raw":self.abi.solid_type,"edge_subtype_raw":self.association_abi()?.edge_subtype,
            "bodies":bodies,"entries":entries,"abi":{"version":abi.version,"edge_bytes":abi.edge_bytes,"edge_offsets":abi.edge_offsets},
            "identity_scope":"process-local tags plus exact geometric observations; no persistent feature identity or manufacturing conclusion"}),
        )
    }
    fn association_abi(&self) -> Result<AssociationAbi> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(*mut AssociationAbi, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_association_abi\0")?) };
        let mut abi = AssociationAbi::default();
        let offsets = [
            std::mem::offset_of!(Association, object_1),
            std::mem::offset_of!(Association, object_2),
            std::mem::offset_of!(Association, view),
            std::mem::offset_of!(Association, point_type),
            std::mem::offset_of!(Association, line_type),
            std::mem::offset_of!(Association, position),
        ]
        .map(|v| v as u32);
        ensure!(
            unsafe { function(&mut abi, std::mem::size_of::<AssociationAbi>() as u32) } == 0
                && abi.version == 1
                && abi.structure_bytes == std::mem::size_of::<AssociationAbi>() as u32
                && abi.entry_bytes == std::mem::size_of::<Association>() as u32
                && abi.entry_offsets == offsets
                && abi.face_subtype != abi.edge_subtype,
            "native_association_abi_mismatch"
        );
        Ok(abi)
    }
    fn association_parent(
        &self,
        tag: Tag,
        part: Tag,
        abi: &AssociationAbi,
        unit: Option<&str>,
    ) -> Value {
        if tag == 0 {
            return json!({"tag":0,"present":false,"geometry":{"called":false,"reason":"no_second_object_or_view"}});
        }
        let readers = MemberQueries {
            object_type: self.uf.object_type,
            owning_part: self.uf.owning_part,
            object_name: self.uf.object_name,
            body_type: self.uf.body_type,
        };
        let metadata = readers.observe(tag, part, &self.abi);
        let solid_geometry = metadata["type_query"]["return_code"] == 0
            && metadata["type_query"]["type_raw"] == self.abi.solid_type
            && [
                Some(0),
                Some(abi.face_subtype as i64),
                Some(abi.edge_subtype as i64),
            ]
            .contains(&metadata["type_query"]["subtype_raw"].as_i64());
        let geometry = if solid_geometry {
            let mut bounds = [0.0; 6];
            let code = unsafe { (self.uf.bbox)(tag, bounds.as_mut_ptr()) };
            let valid = code == 0
                && bounds.iter().all(|v| v.is_finite())
                && (0..3).all(|i| bounds[i] <= bounds[i + 3])
                && unit.is_some();
            json!({"called":true,"return_code":code,"value_valid":valid,
                "absolute_bounds":if valid {Some(bounds)}else{None},"unit":unit,
                "coordinate_basis":"UF object absolute coordinates; occurrence uses assembly space",
                "precision_scope":"approximate containing box; SDK warns about trimmed/boolean geometry; not a dimension, tolerance or unique object identity"})
        } else {
            json!({"called":false,"value_valid":false,"reason":"not_a_verified_solid_body_face_or_edge; wireframe/view geometry outside this bounded reader"})
        };
        json!({"tag":tag,"present":true,"metadata":metadata,"geometry":geometry,
            "identity_scope":"process-local tag, not a durable design-object identity"})
    }
    fn extension(&self) -> Result<ExtensionAbi> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(*mut ExtensionAbi, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_extension_abi\0")?) };
        let mut abi = ExtensionAbi::default();
        ensure!(
            unsafe { function(&mut abi, std::mem::size_of::<ExtensionAbi>() as u32) } == 0
                && abi.version == 1
                && abi.structure_bytes == std::mem::size_of::<ExtensionAbi>() as u32,
            "native_extension_abi_mismatch"
        );
        Ok(abi)
    }
    fn tags_array(
        &self,
        pointer: *mut Tag,
        count: i32,
        code: i32,
        operation: &str,
    ) -> Result<Vec<Tag>> {
        let _guard = Allocation {
            p: pointer.cast(),
            free: self.uf.free,
        };
        self.check(code, operation)?;
        ensure!(
            (0..=10_000).contains(&count) && (count == 0 || !pointer.is_null()),
            "native_tag_array_invalid"
        );
        if count == 0 {
            return Ok(Vec::new());
        }
        let values = unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec();
        ensure!(values.iter().all(|tag| *tag != 0), "native_tag_array_null");
        Ok(values)
    }
    fn annotations(&self, part: Tag) -> Result<Value> {
        let abi = self.extension()?;
        let association_abi = self.association_abi()?;
        let mut raw_units = 0;
        let unit_code = unsafe { (self.uf.units)(part, &mut raw_units) };
        let unit = if unit_code != 0 {
            None
        } else if raw_units == self.abi.metric {
            Some("mm")
        } else if raw_units == self.abi.english {
            Some("inch")
        } else {
            None
        };
        let library = self._libraries.last().context("native_shim_missing")?;
        let boxes_fn: unsafe extern "C" fn(
            Tag,
            *mut AnnotationBox,
            u32,
            *mut u32,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(library.symbol(b"oe_nx_annotation_boxes\0")?) };
        let associations_fn: unsafe extern "C" fn(
            Tag,
            *mut Association,
            u32,
            *mut u32,
            *mut i32,
            *mut c_char,
            u32,
        ) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_annotation_associations\0")?) };
        let dimension_value_fn: unsafe extern "C" fn(Tag, *mut f64, *mut c_char, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_dimension_value\0")?) };
        let mut object = 0;
        let mut seen = std::collections::BTreeSet::new();
        let mut dimensions = Vec::new();
        loop {
            self.check(
                unsafe { (self.uf.cycle)(part, abi.dimension_type, &mut object) },
                "UF_dimension_cycle",
            )?;
            if object == 0 {
                break;
            }
            ensure!(
                seen.insert(object) && seen.len() <= 4096,
                "native_dimension_budget"
            );
            let mut retained = 0u8;
            let retained_code = unsafe { (self.uf.annotation_retained)(object, &mut retained) };
            let mut boxes = [AnnotationBox::default(); 64];
            let mut count = 0u32;
            let mut error = vec![0; 1024];
            let code = unsafe {
                boxes_fn(
                    object,
                    boxes.as_mut_ptr(),
                    64,
                    &mut count,
                    error.as_mut_ptr(),
                    error.len() as u32,
                )
            };
            ensure!(count <= 64, "native_annotation_box_count_invalid");
            let text_boxes = if code == 0 {
                Some(boxes[..count as usize].iter().map(|b| {
                ensure!(b.origin.iter().chain([b.length,b.height,b.angle_radians,b.distance].iter()).all(|v|v.is_finite()),"native_annotation_box_nonfinite");
                Ok(json!({"origin":b.origin,"length":b.length,"height":b.height,"angle_radians":b.angle_radians,"distance":b.distance,
                    "coordinate_basis":"native drafting-object coordinates; cross-object/frame mapping requires explicit verification"}))
            }).collect::<Result<Vec<_>>>()?)
            } else {
                None
            };
            let mut entries = [Association::default(); 256];
            let mut number = 0u32;
            let mut query_code = -1;
            let mut association_error = vec![0; 1024];
            let wrapper_code = unsafe {
                associations_fn(
                    object,
                    entries.as_mut_ptr(),
                    256,
                    &mut number,
                    &mut query_code,
                    association_error.as_mut_ptr(),
                    association_error.len() as u32,
                )
            };
            ensure!(number <= 256, "native_annotation_association_count_invalid");
            let associations = if wrapper_code == 0 && query_code == 0 {
                Some(entries[..number as usize].iter().map(|v|{
                json!({"object_1":self.association_parent(v.object_1,part,&association_abi,unit),
                    "object_2":self.association_parent(v.object_2,part,&association_abi,unit),
                    "view":self.association_parent(v.view,part,&association_abi,unit),
                    "point_type_raw":v.point_type,"line_type_raw":v.line_type,
                    "association_position":if v.position.iter().all(|x|x.is_finite()) {Some(v.position)}else{None},
                    "association_position_valid":v.position.iter().all(|x|x.is_finite())&&unit.is_some(),
                    "unit":unit,"coordinate_basis":"UF_DRF assoc_point absolute coordinates",
                    "intended_parent_relation":"unknown; requires independent design intent and object mapping"})
            }).collect::<Vec<_>>())
            } else {
                None
            };
            let mut value = f64::NAN;
            let mut value_error = vec![0; 1024];
            let value_code = unsafe {
                dimension_value_fn(
                    object,
                    &mut value,
                    value_error.as_mut_ptr(),
                    value_error.len() as u32,
                )
            };
            let value_valid = value_code == 0 && value.is_finite() && unit.is_some();
            let metadata = MemberQueries {
                object_type: self.uf.object_type,
                owning_part: self.uf.owning_part,
                object_name: self.uf.object_name,
                body_type: self.uf.body_type,
            }
            .observe(object, part, &self.abi);
            dimensions.push(json!({"dimension_tag":object,"retained":if retained_code==0{Some(retained!=0)}else{None},"retained_query_code":retained_code,
                "computed_size":{"wrapper_code":value_code,"value_valid":value_valid,"value":if value_valid{Some(value)}else{None},
                    "unit":unit,"basis":"NXOpen::Annotations::Dimension::ComputedSize","metadata":metadata,
                    "displayed_text_and_manufacturing_tolerance":"not_assessed",
                    "error":if value_code==0{None}else{buffer_text(&value_error).ok()}},
                "text_boxes":text_boxes,"text_box_query_code":code,"text_box_error":if code==0{None}else{buffer_text(&error).ok()},
                "associativity_parents":{"wrapper_code":wrapper_code,"uf_return_code":query_code,
                    "capture_complete":wrapper_code==0&&query_code==0,"entries":associations,
                    "error":if wrapper_code==0{None}else{buffer_text(&association_error).ok()},
                    "intended_parent_relation":"unknown; retained=false and native associations alone do not prove intended model-edge relation"}}));
        }
        Ok(
            json!({"schema":"oev2.nx-annotation-observation/2","dimensions":dimensions,
            "units_query":{"return_code":unit_code,"raw":raw_units,"unit":unit},
            "association_abi":{"version":association_abi.version,"entry_bytes":association_abi.entry_bytes,"entry_offsets":association_abi.entry_offsets,
                "face_subtype":association_abi.face_subtype,"edge_subtype":association_abi.edge_subtype},
            "scope":"native retained state, rotated text geometry and declared association references; FCF/notes/tables, intended-parent correctness and visual readability remain outside this query"}),
        )
    }
    pub fn load(
        nx_root: &Path,
        shim: &Path,
        shim_identity: &BlobIdentity,
        runtime: &RuntimeIdentity,
        observe: &mut dyn FnMut(&str, Value) -> Result<()>,
    ) -> Result<Self> {
        crate::identity::validate_native_runtime(runtime)?;
        ensure!(
            nx_root.as_os_str().to_str().is_some_and(str::is_ascii)
                && shim.as_os_str().to_str().is_some_and(str::is_ascii),
            "controlled_case_requires_ascii_path"
        );
        let bin = nx_root.join("NXBIN");
        ensure!(
            unsafe { SetDefaultDllDirectories(0x800 | 0x400) } != 0,
            "dll_search_policy_failed"
        );
        for directory in [&bin, shim.parent().context("shim_parent_missing")?] {
            ensure!(
                !unsafe { AddDllDirectory(wide(directory).as_ptr()) }.is_null(),
                "dll_search_directory_failed"
            );
        }
        let mut libraries = Vec::new();
        for name in crate::identity::NATIVE_RUNTIME_PATHS {
            let entry = runtime
                .libraries
                .iter()
                .find(|e| e.path.as_str() == name)
                .context("required_runtime_pin_missing")?;
            libraries.push(Library::load(
                &nx_root.join(name),
                &entry.identity,
                observe,
            )?);
        }
        let uf = Uf::load(&libraries[0])?;
        let shim_library = Library::load(shim, shim_identity, observe)?;
        let abi_fn: unsafe extern "C" fn(*mut Abi, u32) -> i32 =
            unsafe { std::mem::transmute(shim_library.symbol(b"oe_nx_abi\0")?) };
        let mut abi = Abi::default();
        ensure!(
            unsafe { abi_fn(&mut abi, std::mem::size_of::<Abi>() as u32) } == 0,
            "shim_abi_query_failed"
        );
        ensure!(
            abi.version == 2
                && abi.structure_bytes == std::mem::size_of::<Abi>() as u32
                && abi.tag_bytes == 4
                && abi.logical_bytes == 1,
            "shim_abi_version_or_scalar_width_differs"
        );
        ensure!(
            abi.load_status_bytes == std::mem::size_of::<LoadStatus>() as u32
                && abi.load_status_failed == std::mem::offset_of!(LoadStatus, failed) as u32
                && abi.load_status_abort == std::mem::offset_of!(LoadStatus, user_abort) as u32
                && abi.load_status_count == std::mem::offset_of!(LoadStatus, n_parts) as u32
                && abi.load_status_names == std::mem::offset_of!(LoadStatus, file_names) as u32
                && abi.load_status_codes == std::mem::offset_of!(LoadStatus, statuses) as u32,
            "vendor_load_status_layout_differs"
        );
        ensure!(
            abi.jt_observation_version == 1
                && abi.jt_observation_bytes == std::mem::size_of::<BodyJtObservation>() as u32
                && abi.jt_observation_offsets == BodyJtObservation::offsets()
                && abi.faceted_type == 139
                && abi.jt_body_subtype == 3,
            "vendor_jt_observation_layout_or_type_differs"
        );
        for cap in [
            abi.max_path_bytes,
            abi.max_refset_bytes,
            abi.max_instance_bytes,
        ] {
            ensure!(
                (1..=65536).contains(&cap),
                "vendor_string_capacity_unbounded"
            );
        }
        let create_expression = unsafe {
            std::mem::transmute::<*mut c_void, CreateExpression>(
                shim_library.symbol(b"oe_nx_expression_create\0")?,
            )
        };
        let edit_expression = unsafe {
            std::mem::transmute::<*mut c_void, EditExpression>(
                shim_library.symbol(b"oe_nx_expression_edit_update\0")?,
            )
        };
        let configure_load = unsafe {
            std::mem::transmute::<*mut c_void, ConfigureLoad>(
                shim_library.symbol(b"oe_nx_load_options\0")?,
            )
        };
        let add_component_observed = unsafe {
            std::mem::transmute::<*mut c_void, AddComponentObserved>(
                shim_library.symbol(b"oe_nxopen_add_component\0")?,
            )
        };
        let refset_operation = unsafe {
            std::mem::transmute::<*mut c_void, RefsetOperation>(
                shim_library.symbol(b"oe_nx_controlled_refset\0")?,
            )
        };
        let body_jt = unsafe {
            std::mem::transmute::<*mut c_void, BodyJt>(shim_library.symbol(b"oe_nx_body_jt\0")?)
        };
        let export_step = unsafe {
            std::mem::transmute::<*mut c_void, ExportStep>(
                shim_library.symbol(b"oe_nx_step_export\0")?,
            )
        };
        let import_step = unsafe {
            std::mem::transmute::<*mut c_void, ImportStep>(
                shim_library.symbol(b"oe_nx_step_import\0")?,
            )
        };
        libraries.push(shim_library);
        Ok(Self {
            _libraries: libraries,
            uf,
            abi,
            create_expression,
            edit_expression,
            configure_load,
            add_component_observed,
            refset_operation,
            body_jt,
            export_step,
            import_step,
            initialized: false,
            load_observations: Vec::new(),
            load_observation_bytes: 0,
            assembly_cycle_observation_count: 0,
        })
    }
    pub fn initialize(&mut self) -> Result<i32> {
        ensure!(!self.initialized, "native_reinitialization_forbidden");
        let code = unsafe { (self.uf.initialize)() };
        self.check(code, "UF_initialize")?;
        self.initialized = true;
        let observed = unsafe { (self.uf.initialized)() };
        ensure!(observed == 1, "UF_initialized_flag_not_one");
        Ok(code)
    }
    pub fn was_initialized(&self) -> bool {
        self.initialized
    }
    pub fn terminate(&mut self) -> Result<i32> {
        ensure!(
            self.initialized,
            "native_termination_without_initialization"
        );
        self.initialized = false;
        let code = unsafe { (self.uf.terminate)() };
        Ok(code)
    }
    /// /2 only: runtime members of the actual pinned compiled shim ABI.
    pub fn motion_abi_observation2(&self) -> oe_contracts::NxMotionCompiledAbi2 {
        oe_contracts::NxMotionCompiledAbi2 {
            schema: "oev2.nx-motion-compiled-abi/2".into(),
            abi_version: u64::from(self.abi.version),
            solid_type: i64::from(self.abi.solid_type),
            solid_subtype: 0,
            solid_body: i64::from(self.abi.solid_body),
        }
    }
    pub fn abi_observation(&self) -> serde_json::Value {
        serde_json::json!({"version":self.abi.version,"tag_bytes":self.abi.tag_bytes,"logical_bytes":self.abi.logical_bytes,"load_status_bytes":self.abi.load_status_bytes,"jt_observation_version":self.abi.jt_observation_version,"jt_observation_bytes":self.abi.jt_observation_bytes,"jt_observation_offsets":self.abi.jt_observation_offsets,"faceted_type":self.abi.faceted_type,"jt_body_subtype":self.abi.jt_body_subtype,"rust_layout_matches_compiled_vendor_header":true,"pinned_library_count":3,"all_dependency_identities_verified":false})
    }
    fn check(&self, code: i32, operation: &str) -> Result<()> {
        if code == 0 {
            return Ok(());
        }
        let mut message = vec![0 as c_char; 133];
        let _ = unsafe { (self.uf.message)(code, message.as_mut_ptr()) };
        bail!(
            "{operation}: code {code}: {}",
            buffer_text(&message).unwrap_or_else(|_| "unreadable_vendor_message".into())
        )
    }
    fn shim_check(&self, code: i32, error: &[c_char], operation: &str) -> Result<()> {
        ensure!(
            code == 0,
            "{operation}: code {code}: {}",
            buffer_text(error).unwrap_or_else(|_| "unreadable_shim_message".into())
        );
        Ok(())
    }
    pub fn take_load_observations(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.load_observations)
    }
    fn load_status(
        &mut self,
        code: i32,
        status: &mut LoadStatus,
        operation: &str,
        arguments: Value,
    ) -> Result<()> {
        let logicals = valid_load_status_logicals(status);
        let failed = status.failed != 0;
        let aborted = status.user_abort != 0;
        let count = status.n_parts;
        let snapshot = unsafe { load_status_snapshot(status, self.abi.max_path_bytes as usize) };
        let mut details = snapshot.as_ref().ok().cloned();
        if let Some(parts) = details.as_mut().and_then(|v| v["parts"].as_array_mut()) {
            for part in parts {
                let status_code = part["status_code"].as_i64().expect("copied i32 status") as i32;
                let mut message = vec![0; 1024];
                let message_code = unsafe { (self.uf.message)(status_code, message.as_mut_ptr()) };
                part["message_return_code"] = json!(message_code);
                part["message"] = json!(buffer_text(&message).ok());
            }
        }
        let freed = unsafe { (self.uf.free_status)(status) };
        let recorded = queue_load_observation(
            &mut self.load_observations,
            &mut self.load_observation_bytes,
            json!({"operation":operation,"arguments":arguments,"return_code":code,"failed":failed,"user_abort":aborted,"logical_values_valid":logicals.is_ok(),"logical_summary_scope":"nonzero raw bytes; canonical0/1 required before acceptance","n_parts":count,"load_status":details,"diagnostics_complete":snapshot.is_ok(),"snapshot_error":snapshot.as_ref().err().map(|e|format!("{e:#}")),"free_status_return_code":freed}),
        );
        self.check(code, operation)?;
        logicals?;
        ensure!(
            !failed && !aborted && count >= 0,
            "{operation}: native_part_load_failed_or_aborted; failed={failed}; aborted={aborted}; count={count}; see native_load_status observation"
        );
        snapshot?;
        ensure!(recorded?, "native_load_status_total_budget_exceeded");
        self.check(freed, "UF_PART_free_load_status")
    }
    fn controlled_refset(
        &mut self,
        owner: Tag,
        body: Tag,
        mode: u32,
    ) -> Result<ControlledReferenceSet> {
        // POD scalars and arrays only; all-zero is a valid initialization.
        let mut out: RefsetObservation = unsafe { std::mem::zeroed() };
        let mut error = vec![0; 1024];
        let code = unsafe {
            (self.refset_operation)(
                owner,
                body,
                mode,
                &mut out,
                std::mem::size_of::<RefsetObservation>() as u32,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        let members = out.members[..(out.member_count as usize).min(32)].to_vec();
        // Only a bounded, successful POD observation can supply tags to the
        // extra read-only queries. Keep original validation and error ordering.
        let inspect_members = code == 0
            && out.version == 1
            && out.structure_bytes == std::mem::size_of::<RefsetObservation>() as u32
            && out.operation == mode
            && out.owner == owner
            && out.requested_body == body
            && out.member_count <= 32
            && out.total_sets <= 32
            && out.complete == 1
            && out.found <= 1;
        let queries = MemberQueries {
            object_type: self.uf.object_type,
            owning_part: self.uf.owning_part,
            object_name: self.uf.object_name,
            body_type: self.uf.body_type,
        };
        let member_observations: Vec<_> = if inspect_members {
            members
                .iter()
                .map(|tag| queries.observe(*tag, owner, &self.abi))
                .collect()
        } else {
            vec![]
        };
        let observation = json!({"schema":"cad-agent.nx-controlled-reference-set/v1","operation":if mode==1 {"create"}else{"query"},
          "owner_tag":out.owner,"requested_body":out.requested_body,"reference_set_tag":out.reference_set,"found":out.found,
          "name":buffer_text(&out.name).ok(),"members":members,"reported_member_count":out.member_count,
          "member_observation_schema":"cad-agent.nx-reference-set-member-diagnostics/v1","member_queries_eligible":inspect_members,
          "member_observations":member_observations,"member_observation_scope":"read_only_diagnostics_not_acceptance",
          "initial_count":out.initial_count,"total_sets_before":out.total_sets,"mutation_started":out.mutation_started,
          "diagnostics_complete":out.complete,"diagnostics_complete_scope":"reference_set_wrapper_only",
          "wrapper_code":code,"wrapper_error":buffer_text(&error).ok(),"automatic_retry":false});
        let retained = queue_load_observation(
            &mut self.load_observations,
            &mut self.load_observation_bytes,
            observation,
        )?;
        self.shim_check(code, &error, "controlled_reference_set")?;
        ensure!(
            retained
                && out.version == 1
                && out.structure_bytes == std::mem::size_of::<RefsetObservation>() as u32
                && out.operation == mode
                && out.owner == owner
                && out.requested_body == body
                && out.member_count <= 32
                && out.total_sets <= 32
                && out.complete == 1
                && out.found <= 1,
            "reference_set_observation_invalid"
        );
        if mode == 1 {
            ensure!(
                out.found == 1
                    && out.initial_count == 0
                    && out.mutation_started == 1
                    && members == vec![body],
                "created_reference_set_members_differ"
            );
        }
        if mode == 2 {
            ensure!(out.mutation_started == 0, "read_only_refset_mutated");
        }
        if out.found == 1 {
            ensure!(
                out.reference_set != 0 && buffer_text(&out.name)? == CONTROLLED_REFSET,
                "controlled_reference_set_name_differs"
            );
        } else {
            ensure!(
                out.reference_set == 0 && members.is_empty(),
                "absent_reference_set_has_members"
            );
        }
        Ok(ControlledReferenceSet {
            name: CONTROLLED_REFSET.into(),
            found: out.found == 1,
            reference_set_tag: out.reference_set,
            member_tags: members,
        })
    }
    fn jt_mapping(
        &mut self,
        owner: Tag,
        solid: Tag,
        set: &ControlledReferenceSet,
    ) -> Result<JtMapping> {
        let mut out = BodyJtObservation::default();
        let mut error = vec![0; 1024];
        let code = unsafe {
            (self.body_jt)(
                owner,
                solid,
                &mut out,
                std::mem::size_of::<BodyJtObservation>() as u32,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        let header = out.valid_header(owner, solid);
        let returned = out.returned(code, owner, solid);
        let mut mapping = JtMapping {
            schema: JT_MAPPING_SCHEMA.into(),
            owner_tag: owner,
            solid_tag: solid,
            reference_set_tag: set.reference_set_tag,
            member_tags_before: set.member_tags.clone(),
            member_tags_after: None,
            reference_set_after: None,
            wrapper_code: code,
            call_entered: header && out.call_entered == 1,
            call_returned: header && out.call_returned == 1,
            complete: returned,
            exception_code: (header && out.exception_code != 0).then_some(out.exception_code),
            solid_owner_tag: (header && out.solid_owner != 0).then_some(out.solid_owner),
            jt_tag: (returned && out.has_jt == 1).then_some(out.jt),
            up_to_date: returned.then_some(out.up_to_date == 1),
            type_query_code: None,
            jt_type: None,
            jt_subtype: None,
            owner_query_code: None,
            jt_owner_tag: None,
            error: if code != 0 {
                Some(buffer_text(&error).unwrap_or_else(|_| "unreadable_body_jt_error".into()))
            } else if !returned {
                Some("body_jt_observation_invalid".into())
            } else {
                None
            },
        };
        // Never read exception out pointers. Only a completely copied successful
        // POD can authorize tag-based UF queries; each error remains explicit.
        if let Some(jt) = mapping.jt_tag {
            let (mut kind, mut subtype, mut actual_owner) = (0, 0, 0);
            let type_code = unsafe { (self.uf.object_type)(jt, &mut kind, &mut subtype) };
            let owner_code = unsafe { (self.uf.owning_part)(jt, &mut actual_owner) };
            mapping.type_query_code = Some(type_code);
            mapping.owner_query_code = Some(owner_code);
            mapping.jt_type = (type_code == 0).then_some(kind);
            mapping.jt_subtype = (type_code == 0).then_some(subtype);
            mapping.jt_owner_tag = (owner_code == 0).then_some(actual_owner);
            if type_code != 0 || owner_code != 0 {
                mapping.complete = false;
                mapping.error = Some(format!(
                    "jt_object_queries_failed:type={type_code},owner={owner_code}"
                ));
            }
        }
        // Preserve a second raw member set and its identity. The pure predicate
        // rejects changes; a failed second query is retained, never guessed equal.
        match self.controlled_refset(owner, 0, 2) {
            Ok(after) => {
                mapping.member_tags_after = Some(after.member_tags);
                mapping.reference_set_after = after.found.then_some(after.reference_set_tag);
            }
            Err(err) => {
                mapping.complete = false;
                if mapping.error.is_none() {
                    mapping.error = Some(
                        format!("refset_after_mapping_failed:{err:#}")
                            .chars()
                            .take(1024)
                            .collect(),
                    );
                }
            }
        }
        let retained = queue_load_observation(
            &mut self.load_observations,
            &mut self.load_observation_bytes,
            serde_json::to_value(&mapping)?,
        )?;
        ensure!(retained, "native_jt_mapping_total_budget_exceeded");
        Ok(mapping)
    }
    fn part_path(&self, part: Tag) -> Result<String> {
        let mut name = vec![0; self.abi.max_path_bytes as usize];
        self.check(
            unsafe { (self.uf.part_name)(part, name.as_mut_ptr()) },
            "UF_PART_ask_part_name",
        )?;
        buffer_text(&name)
    }
    fn list(
        &self,
        body: Tag,
        function: unsafe extern "C" fn(Tag, *mut List) -> i32,
    ) -> Result<Vec<Tag>> {
        let mut list = null_mut();
        let code = unsafe { function(body, &mut list) };
        self.owned_list(list, code, "UF_MODL_body_list")
    }
    fn owned_list(&self, list: List, code: i32, operation: &str) -> Result<Vec<Tag>> {
        struct Guard {
            p: List,
            free: unsafe extern "C" fn(*mut List) -> i32,
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                if !self.p.is_null() {
                    unsafe {
                        (self.free)(&mut self.p);
                    }
                }
            }
        }
        let guard = Guard {
            p: list,
            free: self.uf.list_delete,
        };
        self.check(code, operation)?;
        ensure!(!guard.p.is_null(), "native_list_missing");
        let mut count = 0;
        self.check(
            unsafe { (self.uf.list_count)(guard.p, &mut count) },
            "UF_MODL_ask_list_count",
        )?;
        ensure!((0..=100000).contains(&count), "native_list_count_bound");
        let mut result = Vec::new();
        for i in 0..count {
            let mut tag = 0;
            self.check(
                unsafe { (self.uf.list_item)(guard.p, i, &mut tag) },
                "UF_MODL_ask_list_item",
            )?;
            ensure!(tag != 0, "native_list_null_tag");
            result.push(tag);
        }
        Ok(result)
    }
    fn expressions(&self, part: Tag) -> Result<Vec<Expression>> {
        let mut count = 0;
        let mut tags = null_mut();
        let code = unsafe { (self.uf.expressions)(part, &mut count, &mut tags) };
        let _guard = Allocation {
            p: tags.cast(),
            free: self.uf.free,
        };
        self.check(code, "UF_MODL_ask_exps_of_part")?;
        ensure!(
            (0..=100000).contains(&count) && (count == 0 || !tags.is_null()),
            "native_expression_array_invalid"
        );
        let mut result = Vec::new();
        for i in 0..count {
            let tag = unsafe { *tags.add(i as usize) };
            let mut raw = null_mut();
            let code = unsafe { (self.uf.expression_string)(tag, &mut raw) };
            let _string = Allocation {
                p: raw.cast(),
                free: self.uf.free,
            };
            self.check(code, "UF_MODL_ask_exp_tag_string")?;
            let expression = unsafe { allocated_text(raw, 65536) }?;
            let Some((name, rhs)) = expression.split_once('=') else {
                bail!("expression_definition_has_no_equal")
            };
            let name = name.trim();
            if !name.starts_with("cad_") {
                continue;
            }
            let mut value = 0.0;
            self.check(
                unsafe { (self.uf.expression_value)(tag, &mut value) },
                "UF_MODL_ask_exp_tag_value",
            )?;
            ensure!(value.is_finite(), "native_expression_nonfinite");
            result.push(Expression {
                name: name.into(),
                rhs: rhs.trim().into(),
                value,
            });
        }
        Ok(result)
    }
    fn components(&self, part: Tag) -> Result<Vec<Component>> {
        let root = unsafe { (self.uf.root_occurrence)(part) };
        if root == 0 {
            return Ok(vec![]);
        }
        let mut tags = null_mut();
        let count = unsafe { (self.uf.children)(root, &mut tags) };
        let _guard = Allocation {
            p: tags.cast(),
            free: self.uf.free,
        };
        ensure!(
            (0..=100000).contains(&count) && (count == 0 || !tags.is_null()),
            "native_occurrence_count_invalid"
        );
        let mut result = Vec::new();
        for i in 0..count {
            let tag = unsafe { *tags.add(i as usize) };
            let mut part_name = vec![0; self.abi.max_path_bytes as usize];
            let mut refset = vec![0; self.abi.max_refset_bytes as usize];
            let mut name = vec![0; self.abi.max_instance_bytes as usize];
            let mut origin = [0.0; 3];
            let mut matrix = [0.0; 9];
            let mut transform = [0.0; 16];
            self.check(
                unsafe {
                    (self.uf.component_data)(
                        tag,
                        part_name.as_mut_ptr(),
                        refset.as_mut_ptr(),
                        name.as_mut_ptr(),
                        origin.as_mut_ptr(),
                        matrix.as_mut_ptr(),
                        transform.as_mut_ptr(),
                    )
                },
                "UF_ASSEM_ask_component_data",
            )?;
            ensure!(
                origin.iter().chain(matrix.iter()).all(|v| v.is_finite()),
                "native_component_transform_nonfinite"
            );
            let prototype = unsafe { (self.uf.prototype)(tag) };
            ensure!(prototype != 0, "native_prototype_missing");
            result.push(Component {
                name: buffer_text(&name)?,
                prototype_path: self.part_path(prototype)?,
                reference_set: buffer_text(&refset)?,
                origin,
                matrix,
            });
        }
        Ok(result)
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        // Emergency cleanup only: disk/report failure must not deliberately
        // leave the initialized vendor environment unreleased. No success is
        // inferred and no second call follows an explicit terminate attempt.
        if self.initialized {
            self.initialized = false;
            unsafe {
                (self.uf.terminate)();
            }
        }
    }
}
struct Allocation {
    p: *mut c_void,
    free: unsafe extern "C" fn(*mut c_void),
}
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.p.is_null() {
            unsafe {
                (self.free)(self.p);
            }
        }
    }
}
fn buffer_text(raw: &[c_char]) -> Result<String> {
    let end = raw
        .iter()
        .position(|&v| v == 0)
        .context("vendor_string_not_terminated")?;
    String::from_utf8(raw[..end].iter().map(|&v| v as u8).collect())
        .context("vendor_string_not_utf8")
}
unsafe fn allocated_text(raw: *const c_char, max: usize) -> Result<String> {
    ensure!(!raw.is_null(), "vendor_allocated_string_null");
    let mut bytes = Vec::new();
    for i in 0..max {
        let byte = unsafe { *raw.add(i) } as u8;
        if byte == 0 {
            return String::from_utf8(bytes).context("vendor_string_not_utf8");
        }
        bytes.push(byte)
    }
    bail!("vendor_allocated_string_bound")
}
impl Native {
    fn detailed_info_mode(
        &mut self,
        part: Tag,
        refresh_views: bool,
        prepared: Option<Tag>,
    ) -> Result<Value> {
        ensure!(
            !(refresh_views && prepared.is_some()),
            "prepared_observation_refresh_forbidden"
        );
        let display = if prepared.is_some() {
            self.drafting_display_part_query(part)
        } else {
            self.drafting_display_part(part)
        };
        if let Some(sheet) = prepared {
            let state = self.drafting_display_state_query(sheet, None);
            ensure!(
                crate::geometry::drafting::check_prepared_context(part, sheet, &display, &state),
                "native_prepared_context_unavailable"
            );
        }
        let info = self.part_info(part)?;
        let drafting_constants = self.drafting_abi()?;
        let current_layout_point = crate::geometry::drafting::layout_anchor_from_bounds(&info.bbox);
        let mut faces = Vec::new();
        let mut all_faces = Vec::new();
        for body in &info.solid_tags {
            for face in self.list(*body, self.uf.body_faces)? {
                let mut kind = 0;
                let mut point = [0.0; 3];
                let mut direction = [0.0; 3];
                let mut bounds = [0.0; 6];
                let (mut radius, mut minor, mut normal) = (0.0, 0.0, 0);
                self.check(
                    unsafe {
                        (self.uf.face_data)(
                            face,
                            &mut kind,
                            point.as_mut_ptr(),
                            direction.as_mut_ptr(),
                            bounds.as_mut_ptr(),
                            &mut radius,
                            &mut minor,
                            &mut normal,
                        )
                    },
                    "UF_MODL_ask_face_data",
                )?;
                ensure!(all_faces.len() < 65536, "native_full_face_inventory_budget");
                all_faces.push(json!({"body_tag":body,"face_tag":face,"query_code":0,"surface_type":kind,
                    "axis_point":point,"axis_direction":direction,"radius":radius,"face_bounds":bounds,"normal_direction":normal}));
                // Installed NX 2412 uf_modl.h: surface code16 is cylinder;
                // point lies on its axis, direction is its axis direction.
                if kind == 16 {
                    ensure!(
                        point
                            .iter()
                            .chain(direction.iter())
                            .chain(bounds.iter())
                            .chain([radius, minor].iter())
                            .all(|v| v.is_finite()),
                        "native_cylinder_data_nonfinite"
                    );
                    faces.push(json!({"body_tag":body,"face_tag":face,"surface_type":kind,"axis_point":point,"axis_direction":direction,
                        "radius":radius,"face_bounds":bounds,"normal_direction":normal,"role":"cylindrical face; hole/exterior and feature group require design-intent selection"}));
                }
            }
        }
        let mut count = 0;
        let mut pointer = null_mut();
        let code = unsafe { (self.uf.drawings)(&mut count, &mut pointer) };
        let drawings = self.tags_array(pointer, count, code, "UF_DRAW_ask_drawings")?;
        let mut sheets = Vec::new();
        for drawing in drawings {
            let opened = if prepared.is_some() {
                None
            } else {
                let code = unsafe { (self.uf.drawing_open)(drawing) };
                self.check(code, "UF_DRAW_open_drawing")?;
                Some(code)
            };
            let display_state = if prepared.is_some() {
                self.drafting_display_state_query(drawing, None)
            } else {
                self.drafting_display_state(drawing)
            };
            let mut count = 0;
            let mut pointer = null_mut();
            let code = unsafe { (self.uf.drawing_views)(drawing, &mut count, &mut pointer) };
            let tags = self.tags_array(pointer, count, code, "UF_DRAW_ask_views")?;
            let mut views = Vec::new();
            for view in tags {
                let mut before = 0u8;
                self.check(
                    unsafe { (self.uf.drawing_stale)(view, &mut before) },
                    "UF_DRAW_is_object_out_of_date_before",
                )?;
                let update_code = if refresh_views {
                    Some(unsafe { (self.uf.drawing_update)(drawing, view) })
                } else {
                    None
                };
                let mut after = 0u8;
                let after_code = unsafe { (self.uf.drawing_stale)(view, &mut after) };
                let mut observation =
                    self.drafting_view_info(view, part, drawing, drafting_constants);
                observation["layout_reference"] = if let Ok(point) = current_layout_point.as_ref() {
                    self.drafting_layout_reference(view, part, &info.units, *point)?
                } else {
                    json!({"schema":"oev2.native-layout-reference/1","unit":info.units,
                    "model_part_tag":part,"drawing_view_tag":view,
                    "coordinate_basis":"absolute_current_part_bbox_midpoint",
                    "model_coordinates":null,"drawing_mapping":null,
                    "unknown_reason":current_layout_point.as_ref().err().map(|e|e.to_string())})
                };
                observation["view_tag"] = json!(view);
                observation["stale_before"] = json!(before != 0);
                observation["update_code"] = json!(update_code);
                observation["stale_after"] = json!(if after_code == 0 {
                    Some(after != 0)
                } else {
                    None
                });
                observation["stale_query_code"] = json!(after_code);
                views.push(observation);
            }
            let metadata = MemberQueries {
                object_type: self.uf.object_type,
                owning_part: self.uf.owning_part,
                object_name: self.uf.object_name,
                body_type: self.uf.body_type,
            }
            .observe(drawing, part, &self.abi);
            let mut sheet = json!({"drawing_tag":drawing,"views":views,"metadata":metadata,
                "display_open_code":opened,
                "display_state":display_state,
                "sheet_info":self.drafting_sheet_info(drawing,drafting_constants)?,"view_catalog_complete":true});
            if prepared.is_some() {
                sheet["display_open_called"] = json!(false);
            }
            sheets.push(sheet);
        }
        let modeling_views = self.modeling_views(part, drafting_constants)?;
        let model_edges = self.model_edges(part, &info)?;
        let annotations = self.annotations(part)?;
        let mut value = serde_json::to_value(&info)?;
        value["sheet_body_count"] = json!(info.sheet_count);
        value["drawing_sheet_count"] = json!(sheets.len());
        value["drawing_sheets"] = json!(sheets);
        value["drafting_readback_schema"] = json!(if prepared.is_some() {
            "oev2.native-drafting-observation/7"
        } else {
            "oev2.native-drafting-observation/6"
        });
        value["display_part"] = display;
        value["drawing_catalog_complete"] = json!(true);
        value["modeling_view_catalog_complete"] = modeling_views["complete"].clone();
        value["modeling_views"] = modeling_views["views"].clone();
        value["cylindrical_faces"] = json!(faces);
        value["solid_face_inventory"] = json!(all_faces);
        value["solid_face_inventory_complete"] = json!(true);
        value["model_edges"] = model_edges;
        value["annotations"] = annotations;
        value["view_refresh_requested"] = json!(refresh_views);
        value["engineering_verdict"] = json!("not_assessed");
        if let Some(sheet) = prepared {
            value["context_mode"] = json!("observe_prepared");
            value["prepared_drawing_tag"] = json!(sheet);
            value["context_after"] = json!({"display_part":self.drafting_display_part_query(part),
                "display_state":self.drafting_display_state_query(sheet,None)});
            ensure!(
                crate::geometry::drafting::check_prepared_context(
                    part,
                    sheet,
                    &value["context_after"]["display_part"],
                    &value["context_after"]["display_state"]
                ),
                "native_prepared_context_changed"
            );
        }
        Ok(value)
    }
}

impl NativeApi for Native {
    fn feature_inventory(
        &mut self,
        part: u64,
        body: u64,
    ) -> Result<oe_contracts::NxFeatureInventory1> {
        self.current_feature_inventory(part, body)
    }
    fn delete_feature(
        &mut self,
        part: u64,
        body: u64,
        feature: u64,
    ) -> Result<oe_contracts::NxFeatureDeleteOutcome1> {
        self.delete_current_feature(part, body, feature)
    }
    fn geometrical_mass_properties(
        &mut self,
        body: u64,
        owning_part: u64,
    ) -> Result<oe_contracts::NxGeometricalMassObservation1> {
        self.probe_geometrical_mass(body, owning_part)
    }
    fn copy_assembly_component(
        &mut self,
        parent: u32,
        source: &oe_contracts::NxAssemblyOccurrenceObservation,
        name: &str,
    ) -> Result<oe_contracts::NxComponentCopyOutcome1> {
        self.component_copy(parent, source, name)
    }
    fn persist_load_diagnostics(
        &mut self,
        record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        crate::load_diagnostics::persist_batch(
            &mut self.load_observations,
            &mut self.load_observation_bytes,
            record,
        )
    }
    fn loaded_assembly_files(&mut self, namespace: &Path) -> Result<Vec<String>> {
        self.loaded_owned_assembly_files(namespace)
    }
    fn assembly_positions(
        &mut self,
        part: u32,
        namespace: &Path,
        budget: &oe_contracts::NxAssemblyBudget,
        role: &str,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        self.observe_assembly_positions(part, namespace, budget, role)
    }
    fn move_assembly_sample2(
        &mut self,
        part: u32,
        namespace: &Path,
        budget: &oe_contracts::NxAssemblyBudget,
        sample: &oe_contracts::NxAssemblyMotionSample,
        record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        self.move_position_sample_capture2(part, namespace, budget, sample, record)
    }
    fn check_assembly_pairs2(
        &mut self,
        part: u32,
        namespace: &Path,
        budget: &oe_contracts::NxAssemblyBudget,
        motion_budget: &oe_contracts::NxAssemblyMotionBudget,
        total_samples: u64,
        sample_index: u64,
        sample_id: &str,
        record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<Value> {
        self.check_position_pairs_capture2(
            part,
            namespace,
            budget,
            motion_budget,
            total_samples,
            sample_index,
            sample_id,
            record,
        )
    }
    fn save_all_owned_assembly2(
        &mut self,
        namespace: &Path,
        sink: &mut dyn crate::geometry::motion_task::save_capture2::SaveCaptureSink2,
    ) -> Result<Value> {
        self.save_owned_assembly_capture2(namespace, sink)
    }
    fn move_assembly_sample(
        &mut self,
        part: u32,
        namespace: &Path,
        budget: &oe_contracts::NxAssemblyBudget,
        sample: &oe_contracts::NxAssemblyMotionSample,
        record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        self.move_position_sample(part, namespace, budget, sample, record)
    }
    fn check_assembly_pairs(
        &mut self,
        part: u32,
        namespace: &Path,
        budget: &oe_contracts::NxAssemblyBudget,
        motion_budget: &oe_contracts::NxAssemblyMotionBudget,
        total_samples: u64,
        record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<Value> {
        self.check_position_pairs(
            part,
            namespace,
            budget,
            motion_budget,
            total_samples,
            record,
        )
    }

    fn import_assembly_preserved(&mut self, source: &Path, target: &Path) -> Result<()> {
        self.import_assembly(source, target)
    }
    fn create_assembly_units_part(&mut self, path: &Path, unit: &str) -> Result<Tag> {
        self.create_units_part(path, unit)
    }
    fn prepare_owned_assembly_context(&mut self, part: Tag, namespace: &Path) -> Result<()> {
        self.owned_assembly_context(part, namespace)
    }
    fn add_assembly_entire_instance(
        &mut self,
        parent: Tag,
        child: Tag,
        path: &Path,
        name: &str,
        origin: [f64; 3],
        rotation: &[f64],
    ) -> Result<()> {
        self.add_entire_instance(parent, child, path, name, origin, rotation)
    }
    fn add_assembly_entire_instance_nxopen(
        &mut self,
        parent: Tag,
        child: Tag,
        path: &Path,
        name: &str,
        origin: [f64; 3],
        rotation: &[f64],
    ) -> Result<()> {
        self.add_nxopen_entire_loaded(parent, child, path, name, origin, rotation)
    }
    fn full_assembly_info(
        &mut self,
        root: Tag,
        namespace: &Path,
        budget: &oe_contracts::NxAssemblyBudget,
        role: &str,
    ) -> Result<oe_contracts::NxAssemblyObservation> {
        self.observe_assembly(root, namespace, budget, role)
    }
    fn assembly_reference_sets(
        &mut self,
        assembly: &oe_contracts::NxAssemblyObservation,
    ) -> Result<oe_contracts::NxAssemblyReferenceSets2> {
        self.observe_assembly_reference_sets(assembly)
    }
    fn save_all_owned_assembly(&mut self, namespace: &Path) -> Result<Value> {
        self.save_owned_assembly(namespace)
    }
    fn create_drafting_sheet(
        &mut self,
        part: Tag,
        plan: &oe_contracts::NativeDraftingPlan,
    ) -> Result<Value> {
        crate::geometry::drafting::validate(plan)?;
        self.set_work_part(part)?;
        let constants = self.drafting_abi()?;
        let mut units = -1;
        self.check(
            unsafe { (self.uf.units)(part, &mut units) },
            "UF_PART_ask_drafting_units",
        )?;
        ensure!(
            (plan.unit == "mm" && units == self.abi.metric)
                || (plan.unit == "inch" && units == self.abi.english),
            "native_drafting_unit_mismatch"
        );
        let mut count = 0;
        let mut pointer = null_mut();
        let code = unsafe { (self.uf.drawings)(&mut count, &mut pointer) };
        let existing = self.tags_array(pointer, count, code, "UF_DRAW_ask_existing_drawings")?;
        for drawing in existing {
            let metadata = MemberQueries {
                object_type: self.uf.object_type,
                owning_part: self.uf.owning_part,
                object_name: self.uf.object_name,
                body_type: self.uf.body_type,
            }
            .observe(drawing, part, &self.abi);
            ensure!(
                metadata["name_query"]["value_valid"] == true
                    && metadata["owning_part_query"]["matches_expected_owner"] == true,
                "native_drafting_existing_sheet_identity_unknown"
            );
            ensure!(
                metadata["name_query"]["utf8"] != plan.sheet_name,
                "native_drafting_sheet_already_exists"
            );
        }
        let requested_model_view = text(&plan.model_view_name)?;
        let mut model_view = 0;
        self.check(
            unsafe { (self.uf.view_named)(requested_model_view.as_ptr(), &mut model_view) },
            "UF_VIEW_ask_named_model_view",
        )?;
        ensure!(model_view != 0, "native_drafting_model_view_missing");
        let model_metadata = MemberQueries {
            object_type: self.uf.object_type,
            owning_part: self.uf.owning_part,
            object_name: self.uf.object_name,
            body_type: self.uf.body_type,
        }
        .observe(model_view, part, &self.abi);
        ensure!(
            model_metadata["name_query"]["value_valid"] == true
                && model_metadata["name_query"]["utf8"] == plan.model_view_name
                && model_metadata["owning_part_query"]["matches_expected_owner"] == true
                && self.drafting_view_type(model_view, constants)["kind"] == "model",
            "native_drafting_named_view_is_not_current_model_view"
        );
        let full_model_path = self.part_path(part)?;
        let model_name = if full_model_path.len() < 132 {
            full_model_path.clone()
        } else {
            // The vendor view-info DTO has a 132-byte field. Only a single loaded
            // part permits this bounded name transport; final readback must still
            // resolve exactly to the full current-part path, never merely basename.
            ensure!(
                unsafe { (self.uf.num_parts)() } == 1,
                "native_drafting_long_path_requires_unique_loaded_part"
            );
            Path::new(&full_model_path)
                .file_name()
                .and_then(|n| n.to_str())
                .context("native_drafting_model_basename_missing")?
                .to_string()
        };
        let name = text(&plan.sheet_name)?;
        let model_name_c = text(&model_name)?;
        let projection = if plan.projection_angle == "first" {
            constants[2]
        } else {
            constants[3]
        };
        let scale = plan.scale_numerator / plan.scale_denominator;
        let reference = [plan.view_center[0], plan.view_center[1]];
        let inventory = self.part_info(part)?;
        let selected_solid_tags = inventory.solid_tags;
        ensure!(
            (1..=10_000).contains(&selected_solid_tags.len())
                && inventory.solid_count == selected_solid_tags.len(),
            "native_drafting_solid_inventory_invalid"
        );
        let unique = selected_solid_tags
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        ensure!(
            unique.len() == selected_solid_tags.len() && !unique.contains(&0),
            "native_drafting_solid_inventory_invalid"
        );
        for body in &selected_solid_tags {
            let metadata = MemberQueries {
                object_type: self.uf.object_type,
                owning_part: self.uf.owning_part,
                object_name: self.uf.object_name,
                body_type: self.uf.body_type,
            }
            .observe(*body, part, &self.abi);
            ensure!(
                metadata["is_solid_body"] == true
                    && metadata["owning_part_query"]["return_code"] == 0
                    && metadata["owning_part_query"]["value_valid"] == true
                    && metadata["owning_part_query"]["matches_expected_owner"] == true,
                "native_drafting_solid_inventory_identity_unknown"
            );
        }
        let library = self._libraries.last().context("native_shim_missing")?;
        let create: unsafe extern "C" fn(
            *const c_char,
            i32,
            f64,
            f64,
            f64,
            i32,
            *mut Tag,
            *mut i32,
            *mut c_char,
            u32,
        ) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_create_drafting_sheet\0")?) };
        let import: unsafe extern "C" fn(
            Tag,
            Tag,
            *const c_char,
            f64,
            *const f64,
            *mut Tag,
            *mut i32,
            *mut c_char,
            u32,
        ) -> i32 = unsafe {
            std::mem::transmute(library.symbol(b"oe_nx_import_drafting_view_without_anchor_v1\0")?)
        };
        let resize: unsafe extern "C" fn(
            Tag,
            i32,
            f64,
            f64,
            f64,
            i32,
            *mut i32,
            *mut c_char,
            u32,
        ) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_set_drafting_sheet_info_v1\0")?) };
        let mut result = json!({"schema":"oev2.native-drafting-create-observation/16-direct-current-part-reference","complete":false,
            "sheet_tag":0,"model_view_tag":model_view,"view_tag":0,
            "native_codes":{"set_display_part":null,"set_model_view_center":null,"create":null,"open":null,"set_drawing_display_state":null,"import":null,"define_boundary":null,"restore_anchor":null,"set_reference":null,"move_view":null,"set_final_sheet_info":null,"update":null},
            "position_policy":"derived construction sheet; optional NULL smart-anchor import; complete current solid boundary; original reference and independent ABS midpoint mapping must both match; final resize/update and /6 necessary checks before save",
            "layout_policy":"direct current-part complete-solid ABS bbox midpoint read back independently; optional smart relation remains diagnostic; not manufacturing datum/tolerance/geometry change",
            "wrapper_codes":{"create":null,"import":null},
            "selected_solid_tags":selected_solid_tags,"selected_solid_count":selected_solid_tags.len(),
            "boundary_scope":"complete_current_part_solids_prototype_geometry_only",
            "model_path":full_model_path,"model_name_transport":model_name,
            "model_name_transport_kind":if model_name==full_model_path{"full_path"}else{"single_loaded_part_basename"},
            "annotation_transfer":false,"pmi_inheritance":false,"operation":"UF_DRAW_create_drawing and UF_DRAW_import_view",
            "purpose":plan.purpose,"manufacturing_drawing":"not_established","engineering_adoption":"not_established"});
        let display = self.drafting_display_part(part);
        result["native_codes"]["set_display_part"] = display["return_code"].clone();
        result["display_part"] = display.clone();
        if display["matches_expected"] != true {
            result["error"] = json!("native_drafting_current_display_part_unavailable");
            return Ok(result);
        }
        let mut bounds = Vec::new();
        let mut bounds_codes = Vec::new();
        for body in &selected_solid_tags {
            let mut values = [f64::NAN; 6];
            let code = unsafe { (self.uf.bbox)(*body, values.as_mut_ptr()) };
            bounds_codes.push(code);
            bounds.push(values);
        }
        result["layout_anchor_inputs"] = json!({"solid_tags":selected_solid_tags,"bounding_box_query_codes":bounds_codes,
            "bounds":bounds,"coordinate_basis":"UF_MODL_ask_bounding_box absolute current part units; layout only"});
        if bounds_codes.iter().any(|code| *code != 0) {
            result["error"] = json!("native_drafting_layout_bounds_query_failed");
            return Ok(result);
        }
        let coordinates = match crate::geometry::drafting::layout_anchor_from_bounds(&bounds) {
            Ok(point) => point,
            Err(error) => {
                result["error"] = json!(error.to_string());
                return Ok(result);
            }
        };
        let rotation = self.drafting_rotation(model_view);
        let mut model_center = [f64::NAN; 3];
        let center_code = unsafe { (self.uf.view_center)(model_view, model_center.as_mut_ptr()) };
        result["layout_import_inputs"] = json!({"model_view_tag":model_view,"model_view_rotation":rotation,
            "model_view_center":{"return_code":center_code,"coordinates":model_center,"coordinate_basis":"absolute CSYS"},
            "model_anchor":coordinates,"declared_final_reference":reference,"plan_scale":scale,
            "scope":"initial zero-centered right-handed orthonormal model view; layout center changed only in the current output copy with actual readback"});
        let matrix = rotation["matrix"].as_array().and_then(|values| {
            let numbers: Option<Vec<f64>> = values.iter().map(|v| v.as_f64()).collect();
            numbers.and_then(|v| <[f64; 9]>::try_from(v).ok())
        });
        if rotation["return_code"] != 0 || center_code != 0 || matrix.is_none() {
            result["error"] = json!("native_drafting_model_projection_unavailable");
            return Ok(result);
        }
        let prior_compensated_position = match crate::geometry::drafting::layout_import_reference(
            coordinates,
            matrix.expect("checked matrix presence"),
            model_center,
            reference,
            scale,
        ) {
            Ok(point) => point,
            Err(error) => {
                result["error"] = json!(error.to_string());
                return Ok(result);
            }
        };
        let (import_position, construction_size) = match construction_sheet_layout(
            &bounds,
            matrix.expect("checked matrix presence"),
            scale,
            [plan.width, plan.height],
        ) {
            Ok(layout) => layout,
            Err(error) => {
                result["error"] = json!(error.to_string());
                return Ok(result);
            }
        };
        result["construction_layout"] = json!({"schema":"oev2.native-construction-layout/1",
            "temporary_sheet_size":construction_size,"temporary_import_reference":import_position,
            "layout_margin":[plan.width,plan.height],"margin_role":"explicit final sheet extents as construction padding; not engineering tolerances",
            "derivation":"all complete solid bbox corners plus ABS origin projected by original rotation and scale; no adaptive enlargement",
            "final_sheet_size":[plan.width,plan.height],"final_reference":reference,
            "intermediate_save":false,"final_plan_changed":false});
        result["prior_coordinate_compensation_diagnostic"] = json!({"candidate":5,
            "coordinates":prior_compensated_position,"applied":false,
            "scope":"pure arithmetic diagnostic; UF import rejects out-of-sheet inputs, so this is not the active placement algorithm"});
        result["import_position"] = json!(import_position);
        result["declared_final_reference"] = json!(reference);
        result["import_position_policy"] = json!(
            "derived positive construction reference; original final reference is mandatory after complete-solid boundary and movement"
        );
        result["import_anchor_policy"] = json!({"schema":"oev2.native-import-anchor-policy/1",
            "anchor_point":0,"sdk_meaning":"NULL_TAG optional no smart-point anchor", "helper_point_created":false});
        let mut requested_center = coordinates;
        let set_center_code =
            unsafe { (self.uf.set_view_center)(model_view, requested_center.as_mut_ptr()) };
        let mut centered = [f64::NAN; 3];
        let centered_code = unsafe { (self.uf.view_center)(model_view, centered.as_mut_ptr()) };
        let centered_rotation = self.drafting_rotation(model_view);
        let center_matches = centered
            .iter()
            .zip(coordinates)
            .all(|(a, b)| a.is_finite() && (*a - b).abs() <= 1e-7 * b.abs().max(1.0));
        let rotation_matches = centered_rotation["return_code"] == 0
            && centered_rotation["matrix"] == rotation["matrix"];
        result["native_codes"]["set_model_view_center"] = json!(set_center_code);
        result["model_view_centering"] = json!({"schema":"oev2.native-model-view-centering/1",
            "model_view_tag":model_view,"requested_center":coordinates,
            "before":{"return_code":center_code,"point":model_center},
            "set_return_code":set_center_code,
            "after":{"return_code":centered_code,"point":centered},
            "rotation_after":centered_rotation,
            "capture_complete":set_center_code==0 && centered_code==0 && center_matches && rotation_matches,
            "scope":"UF_VIEW_set_center changes layout metadata only on the current output copy; no model-body ABS coordinates or rotation changes"});
        if set_center_code != 0 || centered_code != 0 || !center_matches || !rotation_matches {
            result["error"] = json!("native_drafting_model_view_centering_incomplete");
            return Ok(result);
        }
        let mut drawing = 0;
        let mut query = -1;
        let mut error = vec![0; 1024];
        let wrapper = unsafe {
            create(
                name.as_ptr(),
                units,
                construction_size[0],
                construction_size[1],
                scale,
                projection,
                &mut drawing,
                &mut query,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        result["sheet_tag"] = json!(drawing);
        result["native_codes"]["create"] = json!(query);
        result["wrapper_codes"]["create"] = json!(wrapper);
        if wrapper != 0 || query != 0 || drawing == 0 {
            result["error"] = json!(buffer_text(&error).ok());
            return Ok(result);
        }
        let opened = unsafe { (self.uf.drawing_open)(drawing) };
        result["native_codes"]["open"] = json!(opened);
        if opened != 0 {
            result["error"] = json!("native_drafting_open_failed");
            return Ok(result);
        }
        let display_state = self.drafting_display_state(drawing);
        result["native_codes"]["set_drawing_display_state"] = display_state["set_code"].clone();
        result["display_state_after_open"] = display_state.clone();
        if display_state["capture_complete"] != true {
            result["error"] = json!("native_drafting_drawing_display_state_incomplete");
            return Ok(result);
        }
        let construction_info = self.drafting_sheet_info(drawing, constants)?;
        result["construction_sheet_info"] = construction_info.clone();
        let sheet_matches = construction_info["width"].as_f64().is_some_and(|v| {
            (v - construction_size[0]).abs() <= 1e-7 * construction_size[0].max(1.0)
        }) && construction_info["height"].as_f64().is_some_and(|v| {
            (v - construction_size[1]).abs() <= 1e-7 * construction_size[1].max(1.0)
        });
        if construction_info["value_valid"] != true
            || construction_info["wrapper_code"] != 0
            || construction_info["query_code"] != 0
            || !sheet_matches
        {
            result["error"] = json!("native_drafting_construction_sheet_mismatch");
            return Ok(result);
        }
        let mut view = 0;
        query = -1;
        error.fill(0);
        let wrapper = unsafe {
            import(
                drawing,
                model_view,
                model_name_c.as_ptr(),
                scale,
                import_position.as_ptr(),
                &mut view,
                &mut query,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        result["view_tag"] = json!(view);
        result["native_codes"]["import"] = json!(query);
        result["wrapper_codes"]["import"] = json!(wrapper);
        if wrapper != 0 || query != 0 || view == 0 {
            result["error"] = json!(buffer_text(&error).ok());
            return Ok(result);
        }
        result["raw_anchor_relation_after_import"] =
            self.layout_lifecycle_view(view)["anchor_relation"].clone();
        let independent_initial_mapping = self.drafting_point_mapping(view, coordinates);
        result["independent_initial_model_midpoint_mapping"] = independent_initial_mapping.clone();
        let initial_mapping = &independent_initial_mapping;
        if initial_mapping["return_code"] != 0 || initial_mapping["capture_complete"] != true {
            result["error"] = json!("native_drafting_imported_anchor_mapping_unavailable");
            return Ok(result);
        }
        let initial_view = self.drafting_view_info(view, part, drawing, constants);
        result["view_after_construction_import"] = initial_view.clone();
        let border = initial_view["borders"]["values"].as_array();
        let inside = border.is_some_and(|values| {
            values.len() == 4
                && values
                    .iter()
                    .all(|v| v.as_f64().is_some_and(|x| x.is_finite()))
                && values[0].as_f64().unwrap() >= 0.0
                && values[1].as_f64().unwrap() >= 0.0
                && values[2].as_f64().unwrap() <= construction_size[0]
                && values[3].as_f64().unwrap() <= construction_size[1]
                && values[0].as_f64().unwrap() < values[2].as_f64().unwrap()
                && values[1].as_f64().unwrap() < values[3].as_f64().unwrap()
        });
        if initial_view["borders"]["return_code"] != 0 || !inside {
            result["error"] = json!("native_drafting_construction_actual_borders_outside");
            return Ok(result);
        }
        // The default imported boundary can include a far-away model origin.
        // Establish complete solid coverage before trying to move that view;
        // the SDK result must succeed, including all current solid tags.
        let mut boundary_inputs = selected_solid_tags.clone();
        let defined = unsafe {
            (self.uf.drawing_define_boundary)(
                view,
                boundary_inputs.len() as i32,
                boundary_inputs.as_mut_ptr(),
            )
        };
        result["native_codes"]["define_boundary"] = json!(defined);
        let boundary = self.drafting_boundary(view);
        result["boundary_after_definition"] = boundary.clone();
        if defined != 0 {
            result["error"] = json!(
                self.check(defined, "UF_DRAW_define_bound_by_objects")
                    .err()
                    .map(|e| e.to_string())
            );
            return Ok(result);
        }
        let actual = boundary["object_tags"]
            .as_array()
            .context("native_drafting_bound_objects_readback_missing")?
            .iter()
            .filter_map(Value::as_u64)
            .collect::<std::collections::BTreeSet<_>>();
        let expected = selected_solid_tags
            .iter()
            .map(|tag| u64::from(*tag))
            .collect::<std::collections::BTreeSet<_>>();
        if boundary["capture_complete"] != true
            || boundary["kind"] != "bound_by_objects"
            || actual != expected
        {
            result["error"] = json!("native_drafting_bound_objects_readback_mismatch");
            return Ok(result);
        }
        result["view_after_boundary"] = self.layout_lifecycle_view(view);
        let pre_update = unsafe { (self.uf.drawing_update)(drawing, view) };
        result["native_codes"]["pre_move_member_view_update"] = json!(pre_update);
        result["view_after_pre_move_update"] = self.layout_lifecycle_view(view);
        result["direct_mapping_before_move"] = self.drafting_point_mapping(view, coordinates);
        if pre_update != 0 {
            result["error"] = json!("native_drafting_direct_pre_move_update_failed");
            return Ok(result);
        }
        let current_boundary = self.drafting_boundary(view);
        let actual = current_boundary["object_tags"].as_array().map(|items| {
            items
                .iter()
                .filter_map(Value::as_u64)
                .collect::<std::collections::BTreeSet<_>>()
        });
        let expected = selected_solid_tags
            .iter()
            .map(|tag| u64::from(*tag))
            .collect::<std::collections::BTreeSet<_>>();
        if current_boundary["capture_complete"] != true
            || current_boundary["kind"] != "bound_by_objects"
            || actual.as_ref() != Some(&expected)
        {
            result["error"] = json!("native_drafting_direct_boundary_incomplete");
            return Ok(result);
        }
        let moved = unsafe { (self.uf.drawing_move_view)(view, reference.as_ptr()) };
        result["native_codes"]["move_view"] = json!(moved);
        result["view_after_move"] = self.layout_lifecycle_view(view);
        let mapped = self.drafting_point_mapping(view, coordinates);
        result["direct_mapping_after_move"] = mapped.clone();
        let mapped_matches = mapped["coordinates"].as_array().is_some_and(|items| {
            items.len() == 2
                && items.iter().zip(reference).all(|(a, b)| {
                    a.as_f64()
                        .is_some_and(|n| n.is_finite() && (n - b).abs() <= 1e-7 * b.abs().max(1.0))
                })
        });
        let observed_reference = &result["view_after_move"]["reference_point"];
        let reference_matches = observed_reference["point"].as_array().is_some_and(|items| {
            items.len() == 2
                && items.iter().zip(reference).all(|(a, b)| {
                    a.as_f64()
                        .is_some_and(|n| n.is_finite() && (n - b).abs() <= 1e-7 * b.abs().max(1.0))
                })
        });
        if moved != 0
            || mapped["return_code"] != 0
            || mapped["capture_complete"] != true
            || !mapped_matches
            || observed_reference["return_code"] != 0
            || !reference_matches
        {
            result["error"] = json!("native_drafting_direct_midpoint_or_reference_mismatch");
            return Ok(result);
        }
        let mut resize_codes = [-1; 2];
        error.fill(0);
        let resized = unsafe {
            resize(
                drawing,
                units,
                plan.width,
                plan.height,
                scale,
                projection,
                resize_codes.as_mut_ptr(),
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        result["native_codes"]["set_final_sheet_info"] = json!(resize_codes[1]);
        result["final_sheet_resize"] = json!({"wrapper_code":resized,"native_codes":resize_codes,
            "native_code_order":["ask_drawing_info","set_drawing_info"],
            "requested_size":[plan.width,plan.height],"requested_scale":scale,
            "error":if resized!=0{buffer_text(&error).ok()}else{None}});
        result["sheet_after_final_resize"] = self.drafting_sheet_info(drawing, constants)?;
        result["view_after_final_resize"] = self.drafting_view_info(view, part, drawing, constants);
        if resized != 0 || resize_codes.iter().any(|code| *code != 0) {
            result["error"] = json!("native_drafting_final_sheet_resize_failed");
            return Ok(result);
        }
        let update = unsafe { (self.uf.drawing_update)(drawing, view) };
        result["native_codes"]["update"] = json!(update);
        result["direct_mapping_after_final_update"] =
            self.drafting_point_mapping(view, coordinates);
        result["complete"] = json!(update == 0);
        if update != 0 {
            result["error"] = json!("native_drafting_update_failed");
        }
        Ok(result)
    }
    fn drawing_display_inventory(&mut self, part: Tag, view: Tag) -> Result<Value> {
        let lib = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(*mut Tag, *mut i32) -> i32 =
            unsafe { std::mem::transmute(lib.symbol(b"oe_nx_drawing_csys_v1\0")?) };
        let mut wcs = 0;
        let mut kind = -1;
        let wcs_code = unsafe { f(&mut wcs, &mut kind) };
        ensure!(kind > 0, "native_csys_type_unknown");
        let visible = self.drafting_visible(view);
        let mut objects = std::collections::BTreeSet::new();
        for key in ["visible_tags", "clipped_tags"] {
            if let Some(rows) = visible[key].as_array() {
                for r in rows {
                    let n = r.as_u64().context("native_visible_tag_unknown")?;
                    ensure!(n > 0 && n <= u32::MAX as u64, "native_visible_tag_bound");
                    objects.insert(n as u32);
                }
            }
        }
        let mut object = 0;
        let mut coordinates = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        loop {
            self.check(
                unsafe { (self.uf.cycle)(part, kind, &mut object) },
                "UF_OBJ_cycle_csys",
            )?;
            if object == 0 {
                break;
            }
            ensure!(
                seen.insert(object) && seen.len() <= 4096,
                "native_csys_cycle_bound"
            );
            coordinates.push(self.display_object(part, object, None)?);
        }
        let mut visible_objects = Vec::new();
        for tag in objects {
            visible_objects.push(self.display_object(part, tag, None)?);
        }
        Ok(
            json!({"schema":"oev2.native-drawing-display/1","part_tag":part,"view_tag":view,"capture_complete":visible["capture_complete"]==true&&wcs_code==0&&wcs!=0,
            "visible":visible,"objects":visible_objects,"coordinate_systems":coordinates,"wcs_query_code":wcs_code,"wcs_tag":wcs,"wcs_object":if wcs_code==0&&wcs!=0{self.display_object(part,wcs,None)?}else{Value::Null},
            "coverage":"source-bound current objects; UF_OBJ cycle excludes temporary/sleepy/unsaved WCS; WCS queried separately; cached rendering and visual identity not established"}),
        )
    }
    fn restore_drawing_wcs_identity(&mut self, part: Tag, original: Tag) -> Result<Value> {
        let lib = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(Tag, Tag, *mut i32, u32, *mut Tag, *mut c_char, u32) -> i32 =
            unsafe { std::mem::transmute(lib.symbol(b"oe_nx_restore_drawing_wcs_v1\0")?) };
        let mut o = [-1; 11];
        let mut tags = [0; 3];
        let mut err = [0i8; 512];
        let code = unsafe {
            f(
                part,
                original,
                o.as_mut_ptr(),
                11,
                tags.as_mut_ptr(),
                err.as_mut_ptr(),
                512,
            )
        };
        Ok(
            json!({"api":"UF_CSYS_set_wcs_guarded","wrapper_code":code,"abi_version":o[0],"original_tag":original,"original_alive":o[1]==o[2],"owner_matches":o[3]==0&&tags[0]==part,"type_matches":o[4]==0&&o[5]==o[6],"query_before_code":o[7],"before_tag":tags[1],"set_code":o[8],"query_after_code":o[9],"after_tag":tags[2],"set_called":o[10]==1,"raw_observations":o,"raw_tags":tags,"error":if code==0{None}else{buffer_text(&err).ok()}}),
        )
    }
    fn drawing_csys_blank(&mut self, part: Tag, tag: Tag, blank: bool) -> Result<Value> {
        self.display_object(part, tag, Some(blank))
    }
    fn dimension_annotation_layout(
        &mut self,
        part: Tag,
        dimension: Tag,
        origin: Option<[f64; 3]>,
        centered: Option<bool>,
    ) -> Result<Value> {
        let lib = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(
            Tag,
            Tag,
            *const f64,
            i32,
            *mut i32,
            u32,
            *mut f64,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(lib.symbol(b"oe_nx_dimension_layout_v1\0")?) };
        let mut o = [-1i32; 14];
        let mut actual = [f64::NAN; 3];
        let mut err = [0i8; 512];
        let code = unsafe {
            f(
                part,
                dimension,
                origin.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
                centered.map_or(-1, i32::from),
                o.as_mut_ptr(),
                14,
                actual.as_mut_ptr(),
                err.as_mut_ptr(),
                512,
            )
        };
        Ok(
            json!({"api":"NXOpen::Annotations::Dimension::AnnotationLayout","abi_version":o[0],"wrapper_code":code,"part_tag":part,"dimension_tag":dimension,
            "alive":o[1]==o[2] && o[1]>=0,"owner_matches":o[3]==1,"dimension_type_matches":o[4]==1,"query_complete":o[12]==1,
            "state":{"origin":actual,"centered":if o[5]>=0{Some(o[5]==1)}else{None},"associative_origin":if o[6]>=0{Some(o[6]==1)}else{None}},
            "center_set_called":o[8]==1,"center_set_returned":o[9]==1,"origin_set_called":o[10]==1,"origin_set_returned":o[11]==1,
            "raw_observations":o,"error":if code==0{None}else{buffer_text(&err).ok()}}),
        )
    }
    fn dimension_label_lines_updated(
        &mut self,
        part: Tag,
        dimension: Tag,
        label: &str,
    ) -> Result<Value> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(
            Tag,
            Tag,
            *const c_char,
            *mut i32,
            u32,
            *mut c_char,
            u32,
            *mut c_char,
            u32,
            *mut i32,
            u32,
            *mut i32,
            u32,
        ) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_dimension_label_update_v1\0")?) };
        let label = std::ffi::CString::new(label)?;
        let mut u = [-1i32; 30];
        let mut codes = [0i32; 64];
        let mut o = [-1i32; 8];
        let mut below = [0i8; 512];
        let mut err = [0i8; 512];
        let code = unsafe {
            f(
                part,
                dimension,
                label.as_ptr(),
                o.as_mut_ptr(),
                8,
                below.as_mut_ptr(),
                512,
                err.as_mut_ptr(),
                512,
                u.as_mut_ptr(),
                30,
                codes.as_mut_ptr(),
                64,
            )
        };
        let b = |n: usize| if u[n] >= 0 { Some(u[n] == 1) } else { None };
        let update = json!({"api":"NXOpen::Update::StartLocalUpdate/DoUpdate/EndLocalUpdate","abi_version":u[0],"wrapper_code":code,"part_tag":part,"dimension_tag":dimension,
            "display_part_matches":b(1),"counts":{"errors_before":u[2],"delete_before":u[3],"errors_after_update":u[15],"errors_final":u[22],"delete_final":u[23],"cleanup_errors":u[28]},
            "locked_before":b(4),"mark_created":b(6),"mark_id":u[7],"local_started":b(9),"write_called":b(10),"write_code":u[11],
            "update_called":b(12),"update_returned":b(13),"update_errors":u[14],"local_ended":b(17),"rollback_called":b(18),"rollback_returned":b(19),"mark_deleted":b(21),"locked_final":b(24),"prior_marks_preserved":b(25),"own_mark_absent":b(26),"readback_code":u[27],"error_list_cleared":false,
            "error_codes":if u[29]>=0{Some(codes[..(u[29] as usize).min(64)].to_vec())}else{None},"raw_observations":u});
        Ok(
            json!({"api":"NXOpen::Annotations::Dimension::AppendedText","wrapper_code":code,"dimension_tag":dimension,"abi_version":o[0],"owner_matches":o[1]==1,"dimension_type_matches":o[2]==1,"counts":&o[3..7],"write_returned":o[7]==1,"below":buffer_text(&below)?,"raw_observations":o,"update":update,"error":if code==0{None}else{buffer_text(&err).ok()}}),
        )
    }
    fn dimension_label_lines(
        &mut self,
        part: Tag,
        dimension: Tag,
        label: Option<&str>,
    ) -> Result<Value> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(
            Tag,
            Tag,
            *const c_char,
            *mut i32,
            u32,
            *mut c_char,
            u32,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(library.symbol(b"oe_nx_dimension_label_v2\0")?) };
        let label = label.map(std::ffi::CString::new).transpose()?;
        let mut o = [-1i32; 8];
        let mut below = [0i8; 512];
        let mut err = [0i8; 512];
        let code = unsafe {
            f(
                part,
                dimension,
                label.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                o.as_mut_ptr(),
                8,
                below.as_mut_ptr(),
                512,
                err.as_mut_ptr(),
                512,
            )
        };
        Ok(
            json!({"api":"NXOpen::Annotations::Dimension::AppendedText","wrapper_code":code,"dimension_tag":dimension,"abi_version":o[0],"owner_matches":o[1]==1,"dimension_type_matches":o[2]==1,"counts":&o[3..7],"write_returned":o[7]==1,"below":buffer_text(&below)?,"raw_observations":o,"error":if code==0{None}else{buffer_text(&err).ok()}}),
        )
    }
    fn dimension_label(&mut self, part: Tag, dimension: Tag, label: Option<&str>) -> Result<Value> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(
            Tag,
            Tag,
            *const c_char,
            *mut i32,
            u32,
            *mut c_char,
            u32,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(library.symbol(b"oe_nx_dimension_label_v1\0")?) };
        let label = label.map(std::ffi::CString::new).transpose()?;
        let mut o = [-1i32; 8];
        let mut below = [0i8; 512];
        let mut err = [0i8; 512];
        let code = unsafe {
            f(
                part,
                dimension,
                label.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                o.as_mut_ptr(),
                8,
                below.as_mut_ptr(),
                512,
                err.as_mut_ptr(),
                512,
            )
        };
        Ok(
            json!({"api":"NXOpen::Annotations::Dimension::AppendedText","wrapper_code":code,"dimension_tag":dimension,"abi_version":o[0],"owner_matches":o[1]==1,"dimension_type_matches":o[2]==1,"counts":&o[3..7],"write_returned":o[7]==1,"below":buffer_text(&below)?,"raw_observations":o,"error":if code==0{None}else{buffer_text(&err).ok()}}),
        )
    }
    fn drawing_wcs(&mut self, part: Tag, visible: Option<bool>) -> Result<Value> {
        let library = self._libraries.last().context("native_shim_missing")?;
        let f: unsafe extern "C" fn(Tag, i32, *mut i32, u32, *mut c_char, u32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_drawing_wcs_v1\0")?) };
        let mut o = [-1i32; 4];
        let mut err = [0i8; 512];
        let code = unsafe {
            f(
                part,
                visible.map_or(-1, i32::from),
                o.as_mut_ptr(),
                4,
                err.as_mut_ptr(),
                512,
            )
        };
        Ok(
            json!({"api":"NXOpen::WCS::Visibility","wrapper_code":code,"abi_version":o[0],"display_part_matches":o[1]==1,"visible":if [0,1].contains(&o[2]){Some(o[2]==1)}else{None},"write_returned":o[3]==1,"raw_observations":o,"error":if code==0{None}else{buffer_text(&err).ok()}}),
        )
    }
    fn export_drawing_pdf(&mut self, part: Tag, sheet: Tag, output: &Path) -> Result<Value> {
        ensure!(part != 0 && sheet != 0, "drawing_pdf_native_null_tag");
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(
            Tag,
            Tag,
            *const c_char,
            *mut i32,
            u32,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(library.symbol(b"oe_nx_drawing_pdf_v1\0")?) };
        let path = path_text(output)?;
        let mut flags = [-1i32; 10];
        let mut error = [0i8; 512];
        let code = unsafe {
            function(
                part,
                sheet,
                path.as_ptr(),
                flags.as_mut_ptr(),
                flags.len() as u32,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        Ok(
            json!({"api":"NXOpen::PrintPDFBuilder","wrapper_code":code,"sheet_tag":sheet,"abi_version":flags[0],
          "display_part_matches":flags[1]==1,"sheet_owner_matches":flags[2]==1,"selected_count":flags[3],
          "selected_sheet_matches":flags[4]==1,"settings_match":flags[5]==1,"validated":flags[6]==1,
          "commit_entered":flags[7]>=1,"commit_returned":flags[7]==2,"builder_destroyed":flags[8]==1,
          "exception_code":flags[9],"raw_observations":flags,"error":if code==0 {None} else {buffer_text(&error).ok()},
          "settings_contract":"full_scale, metric, black_on_white, standard_widths, text, no_append, no_watermark, no_raster, native_folder"}),
        )
    }
    fn retire_retained_dimension(&mut self, part: u32, dimension: u32) -> Result<Value> {
        ensure!(part != 0 && dimension != 0, "retirement_native_tag_invalid");
        let before = self.layout_object_lifecycle(dimension, part, false)?;
        ensure!(
            before["status_observed"] == true
                && before["status"] == "alive"
                && before["metadata"]["type_query_code"] == 0
                && before["metadata"]["type_raw"] == 26
                && before["metadata"]["subtype_raw"] == 3
                && before["metadata"]["owner_query_code"] == 0
                && before["metadata"]["matches_expected_owner"] == true,
            "retirement_native_current_target_unproven"
        );
        let mut retained = 0u8;
        let query = unsafe { (self.uf.annotation_retained)(dimension, &mut retained) };
        ensure!(
            query == 0 && retained == 1,
            "retirement_native_retained_required"
        );
        let code = unsafe { (self.uf.object_delete)(dimension) };
        // SDK: zero return is not deletion proof. Lifecycle is read without dereferencing deleted tags.
        let after = self.layout_object_lifecycle(dimension, part, false)?;
        Ok(
            json!({"api":"UF_OBJ_delete_object","return_code":code,"dimension_tag":dimension,
            "before_lifecycle":before,"retained_query_code":query,"retained_raw":retained,"after_lifecycle":after}),
        )
    }
    fn create_sheet_diameter(
        &mut self,
        sheet: Tag,
        view: Tag,
        edge: Tag,
        text: [f64; 2],
    ) -> Result<Value> {
        ensure!(
            sheet != 0 && view != 0 && edge != 0 && text.iter().all(|v| v.is_finite()),
            "native_sheet_dimension_inputs_invalid"
        );
        let constants = self.drafting_abi()?;
        let mut owner = 0;
        let owner_code = unsafe { (self.uf.drawing_of_view)(view, &mut owner) };
        let kind = self.drafting_view_type(view, constants);
        ensure!(
            owner_code == 0
                && owner == sheet
                && kind["return_code"] == 0
                && kind["kind"] == "drawing_member"
                && kind["subtype"] == "imported",
            "native_sheet_dimension_view_mismatch"
        );
        let open = unsafe { (self.uf.drawing_open)(sheet) };
        self.check(open, "UF_DRAW_open_dimension_sheet")?;
        let display = self.drafting_display_state(sheet);
        ensure!(
            display["capture_complete"] == true,
            "native_sheet_dimension_display_unproven"
        );
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(
            Tag,
            Tag,
            *const f64,
            *mut Tag,
            *mut i32,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(library.symbol(b"oe_nx_sheet_diameter_v1\0")?) };
        let mut dimension = 0;
        let mut native_code = -1;
        let mut error = vec![0; 1024];
        let origin = [text[0], text[1], 0.0];
        let wrapper = unsafe {
            function(
                edge,
                view,
                origin.as_ptr(),
                &mut dimension,
                &mut native_code,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        Ok(
            json!({"schema":"oev2.native-sheet-diameter-creation/1","sheet_tag":sheet,"view_tag":view,
            "view_owner_query_code":owner_code,"open_sheet_code":open,"display_state":display,
            "wrapper_code":wrapper,"native_code":native_code,"dimension_tag":dimension,
            "complete":wrapper==0&&native_code==0&&dimension!=0,"text_origin_drawing":origin,
            "operation":"UF_DRF_create_diameter_dim with explicit member view and one circular edge",
            "error":if wrapper==0{None}else{buffer_text(&error).ok()},"engineering_adoption":"not_established"}),
        )
    }
    fn create_sheet_parallel_reference(
        &mut self,
        sheet: Tag,
        view: Tag,
        edges: [Tag; 2],
        text: [f64; 2],
    ) -> Result<Value> {
        ensure!(
            sheet != 0
                && view != 0
                && edges.iter().all(|n| *n != 0)
                && edges[0] != edges[1]
                && text.iter().all(|v| v.is_finite()),
            "native_sheet_dimension_inputs_invalid"
        );
        let constants = self.drafting_abi()?;
        let mut owner = 0;
        let owner_code = unsafe { (self.uf.drawing_of_view)(view, &mut owner) };
        let kind = self.drafting_view_type(view, constants);
        ensure!(
            owner_code == 0
                && owner == sheet
                && kind["return_code"] == 0
                && kind["kind"] == "drawing_member"
                && kind["subtype"] == "imported",
            "native_sheet_dimension_view_mismatch"
        );
        let open = unsafe { (self.uf.drawing_open)(sheet) };
        self.check(open, "UF_DRAW_open_dimension_sheet")?;
        let display = self.drafting_display_state(sheet);
        ensure!(
            display["capture_complete"] == true,
            "native_sheet_dimension_display_unproven"
        );
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(
            Tag,
            Tag,
            Tag,
            *const f64,
            *mut Tag,
            *mut i32,
            *mut c_char,
            u32,
        ) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_sheet_parallel_reference_v1\0")?) };
        let mut dimension = 0;
        let mut native_code = -1;
        let mut error = vec![0; 1024];
        let origin = [text[0], text[1], 0.0];
        let wrapper = unsafe {
            function(
                edges[0],
                edges[1],
                view,
                origin.as_ptr(),
                &mut dimension,
                &mut native_code,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        Ok(
            json!({"schema":"oev2.native-sheet-dimension-creation/1","sheet_tag":sheet,"view_tag":view,
            "view_owner_query_code":owner_code,"open_sheet_code":open,"display_state":display,
            "wrapper_code":wrapper,"native_code":native_code,"dimension_tag":dimension,
            "complete":wrapper==0&&native_code==0&&dimension!=0,"text_origin_drawing":origin,
            "operation":"UF_DRF_create_parallel_dim with explicit member view and circular-edge centers",
            "error":if wrapper==0{None}else{buffer_text(&error).ok()},"engineering_adoption":"not_established"}),
        )
    }
    fn create_parallel_reference(
        &mut self,
        edges: [Tag; 2],
        origin: [f64; 3],
        matrix: [f64; 9],
        text_origin: [f64; 3],
    ) -> Result<Value> {
        self.model_abi()?;
        let library = self._libraries.last().context("native_shim_missing")?;
        let function: unsafe extern "C" fn(
            Tag,
            Tag,
            *const f64,
            *const f64,
            *const f64,
            *mut Tag,
            *mut i32,
            *mut c_char,
            u32,
        ) -> i32 = unsafe { std::mem::transmute(library.symbol(b"oe_nx_parallel_reference\0")?) };
        let mut dimension = 0;
        let mut codes = [-1; 6];
        let mut error = vec![0; 1024];
        let code = unsafe {
            function(
                edges[0],
                edges[1],
                origin.as_ptr(),
                matrix.as_ptr(),
                text_origin.as_ptr(),
                &mut dimension,
                codes.as_mut_ptr(),
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        Ok(
            json!({"wrapper_code":code,"native_codes":codes,"dimension_tag":dimension,
            "complete":code==0 && codes.iter().all(|n|*n==0) && dimension!=0,
            "operation":"UF_DRF_create_parallel_dim with independent circular model-edge parents",
            "plane_origin_absolute":origin,"plane_axis_rows":matrix,"text_origin_wcs":text_origin,
            "error":if code==0{None}else{buffer_text(&error).ok()},"engineering_adoption":"not_established"}),
        )
    }
    fn set_work_part(&mut self, part: Tag) -> Result<()> {
        self.check(
            unsafe { (self.uf.set_work)(part) },
            "UF_ASSEM_set_work_part",
        )
    }
    fn create_subtractive_cylinder(
        &mut self,
        mut origin: [f64; 3],
        height: f64,
        diameter: f64,
    ) -> Result<Tag> {
        let abi = self.extension()?;
        let mut h = text(&height.to_string())?
            .into_bytes_with_nul()
            .into_iter()
            .map(|b| b as c_char)
            .collect::<Vec<_>>();
        let mut d = text(&diameter.to_string())?
            .into_bytes_with_nul()
            .into_iter()
            .map(|b| b as c_char)
            .collect::<Vec<_>>();
        let mut direction = [0.0, 0.0, 1.0];
        let mut feature = 0;
        self.check(
            unsafe {
                (self.uf.cylinder)(
                    abi.negative_sign,
                    origin.as_mut_ptr(),
                    h.as_mut_ptr(),
                    d.as_mut_ptr(),
                    direction.as_mut_ptr(),
                    &mut feature,
                )
            },
            "UF_MODL_create_cyl1",
        )?;
        ensure!(feature != 0, "native_cylinder_feature_null");
        Ok(feature)
    }
    fn add_hole_feature(
        &mut self,
        part: u64,
        body: u64,
        mut origin: [f64; 3],
        height: f64,
        diameter: f64,
    ) -> Result<oe_contracts::NxFeatureAddOutcome1> {
        self.feature_delete_context(Tag::try_from(part)?, Tag::try_from(body)?)?;
        ensure!(
            origin.iter().all(|v| v.is_finite())
                && height.is_finite()
                && height > 0.
                && diameter.is_finite()
                && diameter > 0.,
            "add_native_dimensions_invalid"
        );
        ensure!(
            self.load_observation_bytes <= 512 * 1024 - 16 * 1024,
            "add_diagnostic_capacity_before_call"
        );
        let abi = self.extension()?;
        let mut h = text(&height.to_string())?
            .into_bytes_with_nul()
            .into_iter()
            .map(|b| b as c_char)
            .collect::<Vec<_>>();
        let mut d = text(&diameter.to_string())?
            .into_bytes_with_nul()
            .into_iter()
            .map(|b| b as c_char)
            .collect::<Vec<_>>();
        let mut direction = [0., 0., 1.];
        let mut feature = 0;
        let input_origin = origin.to_vec();
        // Same reviewed UF_MODL_create_cyl1 ABI and negative sign as the
        // existing creator, with raw outcome retained even when creation fails.
        let code = unsafe {
            (self.uf.cylinder)(
                abi.negative_sign,
                origin.as_mut_ptr(),
                h.as_mut_ptr(),
                d.as_mut_ptr(),
                direction.as_mut_ptr(),
                &mut feature,
            )
        };
        let update = if code == 0 {
            Some(unsafe { (self.uf.motion_update)() } as i64)
        } else {
            None
        };
        let outcome = oe_contracts::NxFeatureAddOutcome1 {
            schema: crate::geometry::feature_add::OUTCOME_SCHEMA.into(),
            part_tag: part,
            body_tag: body,
            feature_tag: feature as u64,
            origin_mm: input_origin,
            height_mm: height,
            diameter_mm: diameter,
            call_entered: true,
            call_returned: true,
            return_code: code as i64,
            update_code: update,
            complete: code != 0 || update.is_some(),
        };
        self.feature_delete_diagnostic(
            json!({"operation":"UF_MODL_create_cyl1","outcome":outcome}),
        )?;
        Ok(outcome)
    }
    fn create_cylinder_solid(
        &mut self,
        mut origin: [f64; 3],
        length: f64,
        diameter: f64,
    ) -> Result<Tag> {
        let mut h = text(&length.to_string())?
            .into_bytes_with_nul()
            .into_iter()
            .map(|b| b as c_char)
            .collect::<Vec<_>>();
        let mut d = text(&diameter.to_string())?
            .into_bytes_with_nul()
            .into_iter()
            .map(|b| b as c_char)
            .collect::<Vec<_>>();
        let mut direction = [0.0, 0.0, 1.0];
        let mut feature = 0;
        self.check(
            unsafe {
                (self.uf.cylinder)(
                    self.abi.null_sign,
                    origin.as_mut_ptr(),
                    h.as_mut_ptr(),
                    d.as_mut_ptr(),
                    direction.as_mut_ptr(),
                    &mut feature,
                )
            },
            "UF_MODL_create_cyl1_new_body",
        )?;
        ensure!(feature != 0, "native_new_cylinder_feature_null");
        Ok(feature)
    }
    fn feature_body(&mut self, feature: Tag) -> Result<Tag> {
        ensure!(feature != 0, "native_feature_null");
        let mut body = 0;
        self.check(
            unsafe { (self.uf.feature_body)(feature, &mut body) },
            "UF_MODL_ask_feat_body",
        )?;
        ensure!(body != 0, "native_feature_body_null");
        Ok(body)
    }
    fn unite_bodies(&mut self, target: Tag, tool: Tag) -> Result<()> {
        ensure!(
            target != 0 && tool != 0 && target != tool,
            "native_explicit_unite_tags_invalid"
        );
        let work = unsafe { (self.uf.work_part)() };
        ensure!(work != 0, "native_explicit_unite_work_part_missing");
        for body in [target, tool] {
            let (mut owner, mut kind, mut subtype, mut body_kind) = (0, -1, -1, -1);
            self.check(
                unsafe { (self.uf.owning_part)(body, &mut owner) },
                "native_unite_owner",
            )?;
            self.check(
                unsafe { (self.uf.object_type)(body, &mut kind, &mut subtype) },
                "native_unite_type",
            )?;
            self.check(
                unsafe { (self.uf.body_type)(body, &mut body_kind) },
                "native_unite_body_kind",
            )?;
            ensure!(
                owner == work
                    && kind == self.abi.solid_type
                    && subtype == 0
                    && body_kind == self.abi.solid_body
                    && unsafe { (self.uf.is_occurrence)(body) } == 0,
                "native_explicit_unite_owned_solids_required"
            );
        }
        self.check(
            unsafe { (self.uf.unite_bodies)(target, tool) },
            "UF_MODL_unite_bodies",
        )
    }
    fn stepped_part_info(&mut self, part: Tag) -> Result<Value> {
        let mut value = self.rotating_part_info(part)?;
        let faces = value["solid_face_inventory"]
            .as_array_mut()
            .context("native_stepped_faces_missing")?;
        ensure!(faces.len() <= 65, "native_stepped_face_budget");
        for face in faces {
            let tag = face["face_tag"]
                .as_u64()
                .and_then(|n| Tag::try_from(n).ok())
                .filter(|n| *n != 0)
                .context("native_stepped_face_tag_invalid")?;
            let mut body = 0;
            self.check(
                unsafe { (self.uf.face_body)(tag, &mut body) },
                "UF_MODL_ask_face_body",
            )?;
            ensure!(
                face["body_tag"] == body && body != 0,
                "native_stepped_face_owner_mismatch"
            );
            let edges = self.list(tag, self.uf.face_edges)?;
            let unique: std::collections::BTreeSet<_> = edges.iter().copied().collect();
            ensure!(
                !edges.is_empty()
                    && edges.len() <= 4
                    && unique.len() == edges.len()
                    && !unique.contains(&0),
                "native_stepped_face_edges_invalid"
            );
            face["face_body_query_code"] = json!(0);
            face["face_body_tag"] = json!(body);
            face["face_edges_query_code"] = json!(0);
            face["face_edges_complete"] = json!(true);
            face["edge_tags"] = json!(edges);
        }
        Ok(value)
    }
    fn rotating_part_info(&mut self, part: Tag) -> Result<Value> {
        let mut tag = 0;
        let mut seen = std::collections::BTreeSet::new();
        let mut own_solids = Vec::new();
        loop {
            self.check(
                unsafe { (self.uf.cycle)(part, self.abi.solid_type, &mut tag) },
                "native_rotating_cycle",
            )?;
            if tag == 0 {
                break;
            }
            ensure!(
                seen.insert(tag) && seen.len() <= 65536,
                "native_rotating_inventory_cycle_or_budget"
            );
            let (mut kind, mut subtype, mut owner) = (-1, -1, 0);
            let tc = unsafe { (self.uf.object_type)(tag, &mut kind, &mut subtype) };
            let oc = unsafe { (self.uf.owning_part)(tag, &mut owner) };
            let occurrence = unsafe { (self.uf.is_occurrence)(tag) };
            let candidate = crate::assembly::prototype_body_candidate(
                tc,
                kind,
                subtype,
                self.abi.solid_type,
                oc,
                owner,
                part,
                occurrence,
            )?;
            ensure!(
                owner == part && occurrence == 0,
                "native_rotating_foreign_or_occurrence_object"
            );
            if !candidate {
                continue;
            }
            let mut body_kind = -1;
            self.check(
                unsafe { (self.uf.body_type)(tag, &mut body_kind) },
                "native_rotating_body_kind",
            )?;
            ensure!(
                body_kind == self.abi.solid_body,
                "native_rotating_unknown_or_sheet_body"
            );
            own_solids.push(tag);
        }
        ensure!(
            own_solids.len() == 1,
            "native_rotating_exact_own_solid_inventory"
        );
        let mut observed = self.detailed_info(part, false)?;
        let info = self.part_info(part)?;
        ensure!(
            info.solid_count == 1 && info.solid_tags == own_solids,
            "native_cylinder_single_solid_mass_query"
        );
        observed["rotating_body_inventory_complete"] = json!(true);
        observed["rotating_body_inventory_tags"] = json!(own_solids);
        let mut body = [info.solid_tags[0]];
        let mut accuracy = [0.0; 11];
        accuracy[0] = 0.99;
        let mut props = [0.0; 47];
        let mut statistics = [0.0; 13];
        let code = unsafe {
            (self.uf.mass_props)(
                body.as_mut_ptr(),
                1,
                1,
                4,
                1.0,
                1,
                accuracy.as_mut_ptr(),
                props.as_mut_ptr(),
                statistics.as_mut_ptr(),
            )
        };
        observed["native_volume"] = json!({"schema":"oev2.nx-native-solid-mass-properties/1","return_code":code,
            "body_tags":body,"object_type":1,"unit_selector":4,"unit":"m3","accuracy_mode":1,"accuracy_requested":accuracy[0],
            "volume_m3":if code==0 && props[1].is_finite(){Some(props[1])}else{None},
            "surface_area_m2":if code==0 && props[0].is_finite(){Some(props[0])}else{None},
            "raw47":props.as_slice(),"statistics13":statistics,"density_and_material":"not_qualified"});
        self.check(code, "UF_MODL_ask_mass_props_3d_cylinder")?;
        Ok(observed)
    }
    fn detailed_info(&mut self, part: Tag, refresh_views: bool) -> Result<Value> {
        self.detailed_info_mode(part, refresh_views, None)
    }
    fn observe_prepared_drawing(&mut self, part: Tag, sheet: Tag) -> Result<Value> {
        self.detailed_info_mode(part, false, Some(sheet))
    }
    fn ensure_empty_session(&mut self) -> Result<()> {
        ensure!(self.initialized, "native_not_initialized");
        ensure!(
            unsafe { (self.uf.num_parts)() } == 0,
            "native_session_not_empty"
        );
        Ok(())
    }
    fn create_mm_part(&mut self, path: &Path) -> Result<Tag> {
        let name = path_text(path)?;
        let mut tag = 0;
        self.check(
            unsafe { (self.uf.new_part)(name.as_ptr(), self.abi.metric, &mut tag) },
            "UF_PART_new",
        )?;
        ensure!(tag != 0, "new_part_null_tag");
        Ok(tag)
    }
    fn open_part(&mut self, path: &Path) -> Result<Tag> {
        let mut error = vec![0; 1024];
        let code = unsafe {
            (self.configure_load)(
                self.abi.load_from_directory,
                self.abi.load_all,
                0,
                0,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        self.shim_check(code, &error, "configure_native_load_options")?;
        let name = path_text(path)?;
        let mut tag = 0;
        let mut status = LoadStatus::default();
        let code = unsafe { (self.uf.open_part)(name.as_ptr(), &mut tag, &mut status) };
        self.load_status(
            code,
            &mut status,
            "UF_PART_open",
            json!({"path":path,"returned_tag":tag}),
        )?;
        ensure!(tag != 0, "open_part_null_tag");
        Ok(tag)
    }
    fn create_mm_expression(&mut self, part: Tag, formula: &str) -> Result<Tag> {
        let formula = text(formula)?;
        let unit = text("MilliMeter")?;
        let mut expression = 0;
        let mut error = vec![0; 1024];
        let code = unsafe {
            (self.create_expression)(
                part,
                formula.as_ptr(),
                unit.as_ptr(),
                &mut expression,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        self.shim_check(code, &error, "create_unit_expression")?;
        ensure!(expression != 0, "expression_null_tag");
        Ok(expression)
    }
    fn create_block(&mut self, mut corner: [f64; 3], lengths: [String; 3]) -> Result<Tag> {
        let mut buffers: Vec<Vec<c_char>> = lengths
            .iter()
            .map(|x| {
                text(x).map(|s| {
                    s.into_bytes_with_nul()
                        .into_iter()
                        .map(|b| b as c_char)
                        .collect()
                })
            })
            .collect::<Result<_>>()?;
        let mut pointers: Vec<_> = buffers.iter_mut().map(|s| s.as_mut_ptr()).collect();
        let mut feature = 0;
        self.check(
            unsafe {
                (self.uf.block)(
                    self.abi.null_sign,
                    corner.as_mut_ptr(),
                    pointers.as_mut_ptr(),
                    &mut feature,
                )
            },
            "UF_MODL_create_block1",
        )?;
        ensure!(feature != 0, "block_feature_null");
        Ok(feature)
    }
    fn edit_expression_update(&mut self, part: Tag, expression: Tag, formula: &str) -> Result<i32> {
        let formula = text(formula)?;
        let mut errors = 0;
        let mut error = vec![0; 1024];
        let code = unsafe {
            (self.edit_expression)(
                part,
                expression,
                formula.as_ptr(),
                &mut errors,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        self.shim_check(code, &error, "edit_expression_and_native_update")?;
        Ok(errors)
    }
    fn prepare_controlled_representation(&mut self, part: Tag) -> Result<Value> {
        let before = self.part_info(part)?;
        ensure!(
            before.solid_count == 1
                && before.sheet_count == 0
                && before.solid_tags.len() == 1
                && !before.controlled_reference_set.found,
            "controlled_representation_precondition_failed"
        );
        let after = self.controlled_refset(part, before.solid_tags[0], 1)?;
        Ok(
            json!({"schema":"cad-agent.nx-controlled-representation-created/v1","scope":"task_owned_unique_solid_only","before":before.controlled_reference_set,"after":after,"solid_tag":before.solid_tags[0]}),
        )
    }
    fn save_work_part(&mut self) -> Result<()> {
        self.check(unsafe { (self.uf.save)() }, "UF_PART_save")
    }
    fn add_component(
        &mut self,
        parent: Tag,
        prototype_part: Tag,
        prototype: &Path,
        name: &str,
        origin: [f64; 3],
    ) -> Result<()> {
        let verified = self.part_info(prototype_part)?;
        require_controlled_refset(&verified)?;
        ensure!(
            verified
                .path
                .replace('/', "\\")
                .eq_ignore_ascii_case(&prototype.to_string_lossy().replace('/', "\\")),
            "assembly_prototype_identity_differs"
        );
        let prototype = path_text(prototype)?;
        let name = text(name)?;
        let refset = text(CONTROLLED_REFSET)?;
        let options = AssemAdd {
            version: 1,
            structure_bytes: std::mem::size_of::<AssemAdd>() as u32,
            parent,
            prototype_bytes: u32::try_from(prototype.as_bytes_with_nul().len())?,
            refset_bytes: u32::try_from(refset.as_bytes_with_nul().len())?,
            name_bytes: u32::try_from(name.as_bytes_with_nul().len())?,
            layer: -1,
            reserved: 0,
            prototype: prototype.as_ptr(),
            refset: refset.as_ptr(),
            name: name.as_ptr(),
            origin,
            csys: [1., 0., 0., 0., 1., 0.],
        };
        let mut out: Box<NxOpenAddObservation> = Box::new(unsafe { std::mem::zeroed() });
        let mut error = vec![0; 1024];
        let code = unsafe {
            (self.add_component_observed)(
                &options,
                std::mem::size_of::<AssemAdd>() as u32,
                &mut *out,
                std::mem::size_of::<NxOpenAddObservation>() as u32,
                error.as_mut_ptr(),
                error.len() as u32,
            )
        };
        let mut observation = out.observation(code, &error);
        observation["arguments"] = json!({"parent_tag":parent,"prototype_part_tag":prototype_part,"prototype":prototype.to_str().ok(),"instance_name":name.to_str().ok(),"origin":origin,"layer":-1,"reference_set":CONTROLLED_REFSET});
        let retained = queue_load_observation(
            &mut self.load_observations,
            &mut self.load_observation_bytes,
            observation,
        )?;
        ensure!(
            retained && out.accepted(code),
            "nxopen_assembly_add_not_accepted: wrapper={code}; returned={}; status={}; unloaded={}; copied={}; released={}; complete={}",
            out.call_returned,
            out.status_present,
            out.unloaded_count,
            out.status_copied,
            out.status_released,
            out.complete
        );
        Ok(())
    }
    fn part_info(&mut self, part: Tag) -> Result<PartInfo> {
        let mut units = 0;
        self.check(
            unsafe { (self.uf.units)(part, &mut units) },
            "UF_PART_ask_units",
        )?;
        let mut solid_count = 0;
        let mut solid_tags = Vec::new();
        let mut sheet_count = 0;
        let mut boxes = Vec::new();
        let mut exact_boxes = Vec::new();
        let mut vertex_boxes = Vec::new();
        let mut object = 0;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            self.check(
                unsafe { (self.uf.cycle)(part, self.abi.solid_type, &mut object) },
                "UF_OBJ_cycle_objs_in_part",
            )?;
            if object == 0 {
                break;
            }
            ensure!(
                seen.insert(object) && seen.len() <= 100000,
                "native_object_cycle_invalid"
            );
            let (mut kind, mut subtype) = (0, 0);
            self.check(
                unsafe { (self.uf.object_type)(object, &mut kind, &mut subtype) },
                "UF_OBJ_ask_type_and_subtype",
            )?;
            if kind != self.abi.solid_type || subtype != 0 {
                continue;
            }
            let mut body_type = 0;
            self.check(
                unsafe { (self.uf.body_type)(object, &mut body_type) },
                "UF_MODL_ask_body_type",
            )?;
            if body_type == self.abi.sheet_body {
                sheet_count += 1;
                continue;
            }
            if body_type != self.abi.solid_body {
                continue;
            }
            solid_count += 1;
            solid_tags.push(object);
            let mut bounds = [0.0; 6];
            self.check(
                unsafe { (self.uf.bbox)(object, bounds.as_mut_ptr()) },
                "UF_MODL_ask_bounding_box",
            )?;
            ensure!(
                bounds.iter().all(|x| x.is_finite()),
                "native_bbox_nonfinite"
            );
            boxes.push(bounds);
            let (mut corner, mut directions, mut distances) = ([0.0; 3], [[0.0; 3]; 3], [0.0; 3]);
            // NULL_TAG selects the work CSYS; retain returned frame instead of
            // assuming its axes are the absolute axes or using curved vertices.
            self.check(
                unsafe {
                    (self.uf.bbox_exact)(
                        object,
                        0,
                        corner.as_mut_ptr(),
                        directions.as_mut_ptr(),
                        distances.as_mut_ptr(),
                    )
                },
                "UF_MODL_ask_bounding_box_exact",
            )?;
            exact_boxes.push(ExactBounds::new(corner, directions, distances)?);
            let faces = self.list(object, self.uf.body_faces)?;
            let mut planar = !faces.is_empty();
            for face in faces {
                let mut kind = 0;
                self.check(
                    unsafe { (self.uf.face_type)(face, &mut kind) },
                    "UF_MODL_ask_face_type",
                )?;
                planar &= kind == self.abi.planar_face;
            }
            if !planar {
                vertex_boxes.push(None);
                continue;
            }
            let mut minimum = [f64::INFINITY; 3];
            let mut maximum = [f64::NEG_INFINITY; 3];
            let mut vertices = 0;
            for edge in self.list(object, self.uf.body_edges)? {
                let (mut a, mut b, mut count) = ([0.0; 3], [0.0; 3], 0);
                self.check(
                    unsafe {
                        (self.uf.edge_vertices)(edge, a.as_mut_ptr(), b.as_mut_ptr(), &mut count)
                    },
                    "UF_MODL_ask_edge_verts",
                )?;
                ensure!((0..=2).contains(&count), "native_edge_vertex_count_invalid");
                for p in [a, b].into_iter().take(count as usize) {
                    ensure!(p.iter().all(|x| x.is_finite()), "native_vertex_nonfinite");
                    vertices += 1;
                    for i in 0..3 {
                        minimum[i] = minimum[i].min(p[i]);
                        maximum[i] = maximum[i].max(p[i]);
                    }
                }
            }
            ensure!(vertices > 0, "planar_body_has_no_vertices");
            vertex_boxes.push(Some([
                minimum[0], minimum[1], minimum[2], maximum[0], maximum[1], maximum[2],
            ]));
        }
        let controlled_reference_set = self.controlled_refset(part, 0, 2)?;
        let controlled_jt_mapping = if controlled_reference_set.found
            && solid_tags.len() == 1
            && controlled_reference_set.member_tags != solid_tags
        {
            Some(self.jt_mapping(part, solid_tags[0], &controlled_reference_set)?)
        } else {
            None
        };
        Ok(PartInfo {
            path: self.part_path(part)?,
            part_tag: part,
            representation_baseline: REPRESENTATION_BASELINE.into(),
            controlled_jt_mapping,
            units: if units == self.abi.metric {
                "mm"
            } else if units == self.abi.english {
                "inch"
            } else {
                "unknown"
            }
            .into(),
            solid_count,
            solid_tags,
            controlled_reference_set,
            sheet_count,
            bbox: boxes,
            bbox_role: "UF envelope, may include tolerance padding".into(),
            planar_vertex_bounds: vertex_boxes,
            exact_work_csys_bounds: exact_boxes,
            expressions: self.expressions(part)?,
            components: self.components(part)?,
        })
    }
    fn export_ap214(&mut self, input: &Path, output: &Path) -> Result<()> {
        let input = path_text(input)?;
        let output = path_text(output)?;
        let layers = text("1-256")?;
        let options = StepExport {
            structure_bytes: std::mem::size_of::<StepExport>() as u32,
            application_protocol: self.abi.step_ap214,
            export_from: self.abi.step_existing_part,
            selection_scope: self.abi.step_entire_part,
            solids: 1,
            file_save: 0,
            process_hold: 1,
            input: input.as_ptr(),
            output: output.as_ptr(),
            layers: layers.as_ptr(),
        };
        let mut error = vec![0; 1024];
        let code = unsafe { (self.export_step)(&options, error.as_mut_ptr(), error.len() as u32) };
        self.shim_check(code, &error, "export_ap214")
    }
    fn import_step_new_part(&mut self, input: &Path, output: &Path) -> Result<()> {
        let input = path_text(input)?;
        let output = path_text(output)?;
        let options = StepImport {
            structure_bytes: std::mem::size_of::<StepImport>() as u32,
            import_to: self.abi.step_new_part,
            solids: 1,
            file_open: 0,
            process_hold: 1,
            input: input.as_ptr(),
            output: output.as_ptr(),
        };
        let mut error = vec![0; 1024];
        let code = unsafe { (self.import_step)(&options, error.as_mut_ptr(), error.len() as u32) };
        self.shim_check(code, &error, "import_step")
    }
    fn import_parasolid(&mut self, input: &Path, part: Tag) -> Result<Vec<Tag>> {
        ensure!(
            part != 0 && unsafe { (self.uf.num_parts)() } == 1,
            "parasolid_import_requires_task_owned_single_part"
        );
        let mut name = path_text(input)?.into_bytes_with_nul();
        let mut list = null_mut();
        // UF requires mutable char*, so use an owned writable buffer. The list
        // is guarded immediately, including nonzero-error and validation paths.
        let code = unsafe { (self.uf.import_parasolid)(name.as_mut_ptr().cast(), &mut list) };
        let tags = self.owned_list(list, code, "UF_PS_import_data")?;
        ensure!(!tags.is_empty(), "parasolid_import_no_bodies");
        let mut seen = std::collections::BTreeSet::new();
        for tag in &tags {
            ensure!(seen.insert(*tag), "parasolid_import_duplicate_body");
            let mut owner = 0;
            self.check(
                unsafe { (self.uf.owning_part)(*tag, &mut owner) },
                "UF_OBJ_ask_owning_part",
            )?;
            ensure!(owner == part, "parasolid_import_body_owner_differs");
        }
        Ok(tags)
    }
}

#[cfg(test)]
mod nxopen_boundary_tests {
    use super::*;
    fn success() -> Box<NxOpenAddObservation> {
        let mut v: Box<NxOpenAddObservation> = Box::new(unsafe { std::mem::zeroed() });
        v.version = 1;
        v.structure_bytes = std::mem::size_of::<NxOpenAddObservation>() as u32;
        v.call_entered = 1;
        v.call_returned = 1;
        v.component = 10;
        v.status_present = 1;
        v.status_copied = 1;
        v.status_released = 1;
        v.complete = 1;
        v.phase = 3;
        for (i, b) in CONTROLLED_REFSET.bytes().enumerate() {
            v.reference_set[i] = b as c_char;
        }
        v
    }
    #[test]
    fn missing_partial_unreleased_or_exception_status_cannot_pass() {
        assert!(success().accepted(0));
        for field in 0..10 {
            let mut v = success();
            match field {
                0 => v.call_returned = 0,
                1 => v.status_present = 0,
                2 => v.unloaded_count = 1,
                3 => v.status_copied = 0,
                4 => v.status_released = 0,
                5 => v.complete = 0,
                6 => v.component = 0,
                7 => v.phase = 2,
                8 => v.exception_code = 1,
                _ => v.reference_set[0] = b'X' as c_char,
            };
            assert!(!v.accepted(0));
        }
        assert!(!success().accepted(-90312));
    }
    #[test]
    fn nxopen_observation_never_fabricates_uf_logical_flags() {
        let v = success().observation(0, &[0]);
        assert!(v.get("user_abort").is_none());
        assert!(v.get("failed").is_none());
        assert_eq!(v["schema"], "cad-agent.nxopen-assembly-add/v1");
    }
    #[test]
    fn unreturned_status_ownership_stays_unconfirmed() {
        let mut v = success();
        v.call_returned = 0;
        v.status_present = 0;
        v.status_released = 0;
        let observed = v.observation(-90312, &[0]);
        assert_eq!(observed["ownership"], "unconfirmed_no_query_or_delete");
        assert!(!v.accepted(-90312));
    }
}

#[cfg(test)]
mod member_observation_tests {
    use super::*;
    // Fixed Rust fixtures only. These callbacks do not invoke or mock a native PASS.
    unsafe extern "C" fn kind(tag: Tag, kind: *mut i32, subtype: *mut i32) -> i32 {
        unsafe {
            *kind = if tag == 2 { 45 } else { 70 };
            *subtype = if tag == 9 { 2 } else { 0 };
        }
        if tag == 3 { 801 } else { 0 }
    }
    unsafe extern "C" fn owner(tag: Tag, out: *mut Tag) -> i32 {
        unsafe {
            *out = if tag == 8 { 0 } else { 10 };
        }
        if tag == 5 { 802 } else { 0 }
    }
    unsafe extern "C" fn name(tag: Tag, out: *mut c_char) -> i32 {
        unsafe {
            if tag == 7 {
                return 0;
            }
            *out = if tag == 6 { 255u8 as c_char } else { 0 };
            if tag == 6 {
                *out.add(1) = 0;
            }
        }
        if tag == 1 { 803 } else { 0 }
    }
    unsafe extern "C" fn body(tag: Tag, out: *mut i32) -> i32 {
        // Calling this with a coordinate/edge/type-error tag would be a defect.
        if [2, 3, 9].contains(&tag) {
            return 990;
        }
        unsafe {
            *out = if tag == 10 { 777 } else { 1 };
        }
        if tag == 4 { 804 } else { 0 }
    }
    fn readers() -> MemberQueries {
        MemberQueries {
            object_type: kind,
            owning_part: owner,
            object_name: name,
            body_type: body,
        }
    }
    fn abi() -> Abi {
        Abi {
            max_refset_bytes: 8,
            solid_type: 70,
            solid_body: 1,
            sheet_body: 2,
            ..Default::default()
        }
    }
    #[test]
    fn unnamed_solid_preserves_name_error_without_changing_classification() {
        let v = readers().observe(1, 10, &abi());
        assert_eq!(v["is_solid_body"], true);
        assert_eq!(v["name_query"]["return_code"], 803);
        assert_eq!(v["name_query"]["raw_bytes"], json!([]));
        assert_eq!(v["name_query"]["utf8"], "");
        assert_eq!(v["name_query"]["value_valid"], false);
        assert_eq!(v["owning_part_query"]["matches_expected_owner"], true);
    }
    #[test]
    fn nonbody_or_failed_type_queries_do_not_call_body_api() {
        for tag in [2, 3, 9] {
            let v = readers().observe(tag, 10, &abi());
            assert_eq!(v["body_query"]["called"], false);
            assert!(v["body_query"]["return_code"].is_null());
            if tag == 3 {
                assert!(v["is_solid_body"].is_null());
                assert_eq!(v["type_query"]["return_code"], 801);
            } else {
                assert_eq!(v["is_solid_body"], false);
            }
        }
    }
    #[test]
    fn query_failures_and_null_owner_never_become_false_success() {
        let v = readers().observe(4, 10, &abi());
        assert_eq!(v["body_query"]["return_code"], 804);
        assert_eq!(v["body_query"]["body_type_raw"], 1);
        assert!(v["is_solid_body"].is_null());
        for tag in [5, 8] {
            let v = readers().observe(tag, 10, &abi());
            assert_eq!(v["owning_part_query"]["value_valid"], false);
            assert!(v["owning_part_query"]["matches_expected_owner"].is_null());
        }
        assert!(readers().observe(10, 10, &abi())["is_solid_body"].is_null());
        assert_eq!(
            readers().observe(1, 99, &abi())["owning_part_query"]["matches_expected_owner"],
            false
        );
    }
    #[test]
    fn nonutf8_unterminated_and_unbounded_names_preserve_evidence() {
        let v = readers().observe(6, 10, &abi());
        assert_eq!(v["name_query"]["raw_bytes"], json!([255]));
        assert!(v["name_query"]["utf8"].is_null());
        assert_eq!(v["name_query"]["nul_terminated"], true);
        let v = readers().observe(7, 10, &abi());
        assert_eq!(v["name_query"]["raw_bytes"], json!(vec![127; 8]));
        assert_eq!(v["name_query"]["nul_terminated"], false);
        assert_eq!(v["capture_complete"], false);
        assert!(v["name_query"]["utf8"].is_null());
        for cap in [0, 65537] {
            let v = readers().observe(
                1,
                10,
                &Abi {
                    max_refset_bytes: cap,
                    ..abi()
                },
            );
            assert_eq!(v["queries_executed"], false);
            assert!(v["is_solid_body"].is_null());
        }
        assert_eq!(readers().observe(0, 10, &abi())["queries_executed"], false);
    }
}

#[cfg(test)]
mod body_jt_boundary_tests {
    use super::*;
    fn success() -> BodyJtObservation {
        BodyJtObservation {
            version: 1,
            structure_bytes: 48,
            owner: 10,
            solid: 20,
            solid_owner: 10,
            call_entered: 1,
            call_returned: 1,
            has_jt: 1,
            jt: 30,
            up_to_date: 1,
            complete: 1,
            exception_code: 0,
        }
    }
    #[test]
    fn jt_pod_layout_is_explicit_and_compared_with_runtime_vendor_shim() {
        assert_eq!(std::mem::size_of::<BodyJtObservation>(), 48);
        assert_eq!(
            BodyJtObservation::offsets(),
            [0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44]
        );
        assert!(success().returned(0, 10, 20));
    }
    #[test]
    fn malformed_or_exception_jt_observations_cannot_authorize_tag_queries() {
        for fault in 0..12 {
            let mut out = success();
            match fault {
                0 => out.version = 2,
                1 => out.structure_bytes = 47,
                2 => out.owner = 11,
                3 => out.solid = 21,
                4 => out.solid_owner = 11,
                5 => out.call_entered = 0,
                6 => out.call_returned = 0,
                7 => out.complete = 0,
                8 => out.exception_code = 123,
                9 => out.jt = 0,
                10 => out.has_jt = 0,
                _ => out.up_to_date = 2,
            }
            assert!(!out.returned(0, 10, 20), "fault {fault}");
        }
        assert!(!success().returned(123, 10, 20));
    }
    #[test]
    fn absent_or_stale_jt_is_an_observation_not_an_accepted_mapping() {
        let mut out = success();
        out.up_to_date = 0;
        assert!(out.returned(0, 10, 20));
        assert_eq!(out.up_to_date, 0); // pure geometry predicate rejects stale.
        out.has_jt = 0;
        out.jt = 0;
        assert!(out.returned(0, 10, 20)); // null remains None; no UF query authorized.
    }
}
