//! Explicit /2 save capture. A durable pre-call marker does not prove UF entry.
//! The legacy save API is not routed through this module.
use anyhow::{Result, bail, ensure};
use oe_contracts::{NxMotionSaveFileError2, NxMotionSaveObservation2};

pub const SCHEMA: &str = "oev2.nx-motion-save-observation/2";
pub const LIMIT: i32 = 4096;

/// Mockable native boundary. Nonzero status and invalid headers are returned as
/// raw observations, not errors. Err means no return observation was acquired.
pub trait SaveApi2 {
    fn loaded_part_count(&mut self) -> i32;
    fn owned_part_path(&mut self, index: i32) -> Result<String>;
    fn call_save(&mut self) -> Result<RawSave2>;
}
pub trait SaveCaptureSink2 {
    /// Read-only, exact current stream budget check. Never poisons/writes.
    fn can_record(&self, observation: &NxMotionSaveObservation2) -> Result<bool>;
    /// Must be durable before returning Ok; any error stops this task.
    fn record(&mut self, observation: &NxMotionSaveObservation2) -> Result<()>;
}
#[derive(Debug)]
pub struct RawSave2 {
    pub return_code: i32,
    pub file_error_count: i32,
    pub file_error_storage: String,
    pub file_errors_capture: String,
    pub file_errors: Vec<NxMotionSaveFileError2>,
    pub diagnostic_codes: Vec<String>,
}

/// Guard before invoking read_entry, whose native implementation dereferences
/// vendor arrays. NULL shape and signed header are retained, including count0.
/// These checks do not establish allocation size or make arbitrary pointers safe.
pub fn copy_file_errors2(
    return_code: i32,
    count: i32,
    parts_present: bool,
    codes_present: bool,
    mut read_entry: impl FnMut(usize) -> Result<(u32, i32)>,
) -> RawSave2 {
    let storage = match (parts_present, codes_present) {
        (false, false) => "null_both",
        (true, false) => "parts_only",
        (false, true) => "codes_only",
        (true, true) => "both_present",
    };
    let mut result = RawSave2 {
        return_code,
        file_error_count: count,
        file_error_storage: storage.into(),
        file_errors_capture: "complete".into(),
        file_errors: Vec::new(),
        diagnostic_codes: Vec::new(),
    };
    if !(0..=LIMIT).contains(&count) {
        result.file_errors_capture = "invalid_shape".into();
        result
            .diagnostic_codes
            .push("native_save_count_out_of_range".into());
    } else if count > 0 && (!parts_present || !codes_present) {
        result.file_errors_capture = "invalid_shape".into();
        result
            .diagnostic_codes
            .push("native_save_error_array_missing".into());
    } else {
        for index in 0..count as usize {
            match read_entry(index) {
                Ok((part_tag, code)) => result.file_errors.push(NxMotionSaveFileError2 {
                    index: index as u64,
                    part_tag: u64::from(part_tag),
                    return_code: i64::from(code),
                }),
                Err(_) => {
                    result.file_errors_capture = "partial".into();
                    result
                        .diagnostic_codes
                        .push("native_save_capture_stopped".into());
                    break;
                }
            }
        }
    }
    result
}

fn empty() -> NxMotionSaveObservation2 {
    NxMotionSaveObservation2 {
        schema: SCHEMA.into(),
        api: "UF_PART_save_all".into(),
        phase: "preflight_refused".into(),
        preflight_status: "rejected".into(),
        loaded_part_count_raw: None,
        loaded_owned_paths: Vec::new(),
        loaded_paths_complete: false,
        call_entered: Some(false),
        call_returned: Some(false),
        return_code: None,
        file_error_count_raw: None,
        file_error_storage: "not_observed".into(),
        file_errors_capture: "not_observed".into(),
        file_errors: Vec::new(),
        diagnostic_codes: Vec::new(),
    }
}
fn diagnostic(observation: &mut NxMotionSaveObservation2, code: &str) {
    if !observation.diagnostic_codes.iter().any(|x| x == code) {
        observation.diagnostic_codes.push(code.into());
    }
}
pub fn outer_phase(observation: &NxMotionSaveObservation2) -> Result<&'static str> {
    match observation.phase.as_str() {
        "preflight_refused" => Ok("motion_save_preflight_refused"),
        "call_pending" => Ok("motion_save_pending"),
        "call_returned" => Ok("motion_save_returned"),
        _ => bail!("motion_save_capture_phase_invalid"),
    }
}
/// Retain the largest fitting actual continuous prefix. The native header is
/// never shortened. No append attempt is made until the selected record fits.
fn fit_returned(
    sink: &dyn SaveCaptureSink2,
    observation: &mut NxMotionSaveObservation2,
) -> Result<()> {
    if sink.can_record(observation)? {
        return Ok(());
    }
    ensure!(
        !observation.file_errors.is_empty()
            && matches!(
                observation.file_errors_capture.as_str(),
                "complete" | "partial"
            ),
        "native_save_return_metadata_exceeds_budget"
    );
    let entries = std::mem::take(&mut observation.file_errors);
    observation.file_errors_capture = "partial".into();
    diagnostic(observation, "native_save_capture_budget_exhausted");
    ensure!(
        sink.can_record(observation)?,
        "native_save_return_metadata_exceeds_budget"
    );
    let (mut low, mut high) = (0, entries.len());
    while low < high {
        let mid = low + (high - low + 1) / 2;
        observation.file_errors = entries[..mid].to_vec();
        if sink.can_record(observation)? {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    observation.file_errors = entries[..low].to_vec();
    // Even if a shorter spelling made the full count fit, an actual complete
    // capture cannot be labelled partial with length == original header.
    if observation.file_errors.len() as i64 == observation.file_error_count_raw.unwrap_or(-1) {
        observation.file_errors.pop();
    }
    ensure!(
        sink.can_record(observation)?,
        "native_save_return_metadata_exceeds_budget"
    );
    Ok(())
}

pub fn execute(
    api: &mut dyn SaveApi2,
    sink: &mut dyn SaveCaptureSink2,
) -> Result<NxMotionSaveObservation2> {
    let mut observation = empty();
    let count = api.loaded_part_count();
    observation.loaded_part_count_raw = Some(i64::from(count));
    if !(1..=LIMIT).contains(&count) {
        diagnostic(&mut observation, "native_loaded_part_count_invalid");
        sink.record(&observation)?;
        bail!("native_loaded_part_count_invalid");
    }
    for index in 0..count {
        match api.owned_part_path(index) {
            Ok(path) => observation.loaded_owned_paths.push(path),
            Err(error) => {
                let code = if error
                    .chain()
                    .any(|e| e.to_string() == "native_loaded_part_missing")
                {
                    "native_loaded_part_missing"
                } else {
                    "native_namespace_rejected"
                };
                diagnostic(&mut observation, code);
                sink.record(&observation)?;
                return Err(error);
            }
        }
    }
    observation.phase = "call_pending".into();
    observation.preflight_status = "passed".into();
    observation.loaded_paths_complete = true;
    observation.call_entered = None;
    observation.call_returned = None;
    sink.record(&observation)?;
    let raw = api.call_save()?;
    observation.phase = "call_returned".into();
    observation.call_entered = Some(true);
    observation.call_returned = Some(true);
    observation.return_code = Some(i64::from(raw.return_code));
    observation.file_error_count_raw = Some(i64::from(raw.file_error_count));
    observation.file_error_storage = raw.file_error_storage;
    observation.file_errors_capture = raw.file_errors_capture;
    observation.file_errors = raw.file_errors;
    observation.diagnostic_codes = raw.diagnostic_codes;
    fit_returned(sink, &mut observation)?;
    sink.record(&observation)?;
    ensure!(
        raw.return_code == 0,
        "native_assembly_save_all:{}",
        raw.return_code
    );
    ensure!(
        raw.file_error_count == 0,
        "native_assembly_save_file_errors"
    );
    ensure!(
        observation.file_errors_capture == "complete",
        "native_assembly_save_capture_incomplete"
    );
    Ok(observation)
}
