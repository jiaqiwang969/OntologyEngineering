//! Public checks use portable inputs. Private cases are explicitly ignored and
//! require independently frozen inputs, expected values and a fresh output dir.
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_nx_worker::motion_evidence::receiving2::{self, Inputs2, NativeBodyAbi};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

fn pending() -> Value {
    json!({"schema":"oev2.nx-motion-save-observation/2","api":"UF_PART_save_all","phase":"call_pending",
        "preflight_status":"passed","loaded_part_count_raw":1,"loaded_owned_paths":["C:\\owned\\root.prt"],
        "loaded_paths_complete":true,"call_entered":null,"call_returned":null,"return_code":null,
        "file_error_count_raw":null,"file_error_storage":"not_observed","file_errors_capture":"not_observed",
        "file_errors":[],"diagnostic_codes":[]})
}
#[test]
fn pending_is_explicit_unknown_and_requires_physical_optional_fields() {
    let mut p = pending();
    assert_eq!(
        receiving2::save::validate_save(&receiving2::save::parse_save(&p).unwrap()).unwrap(),
        receiving2::SaveState::PendingUnknown
    );
    p.as_object_mut().unwrap().remove("call_entered");
    assert!(receiving2::save::parse_save(&p).is_err());
    let mut p = pending();
    p["call_entered"] = json!(true);
    assert!(receiving2::save::parse_save(&p).is_err());
}
#[test]
fn zero_code_does_not_erase_native_error_header() {
    let mut p = pending();
    p["phase"] = json!("call_returned");
    p["call_entered"] = json!(true);
    p["call_returned"] = json!(true);
    p["return_code"] = json!(0);
    p["file_error_count_raw"] = json!(1);
    p["file_error_storage"] = json!("both_present");
    p["file_errors_capture"] = json!("complete");
    p["file_errors"] = json!([{"index":0,"part_tag":0,"return_code":0}]);
    assert_eq!(
        receiving2::save::validate_save(&receiving2::save::parse_save(&p).unwrap()).unwrap(),
        receiving2::SaveState::Failed
    );
}
#[test]
fn admission_does_not_write_advance_or_poison() {
    use oe_nx_worker::motion_evidence::{DurableWrite, EvidenceWriter};
    use std::{cell::Cell, io, rc::Rc};
    struct Count(Rc<Cell<usize>>);
    impl Write for Count {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.set(self.0.get() + 1);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl DurableWrite for Count {
        fn synchronize(&mut self) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }
    let io = Rc::new(Cell::new(0));
    let cloned = io.clone();
    let binding: NxMotionEvidenceBinding=serde_json::from_value(json!({"job_id":"admission-test","object":{"object_id":"root","revision":"1","condition_scope":"fixture","source_sha256":"a".repeat(64)},"intent":{"path":"intent.json","sha256":"b".repeat(64)},"samples":1})).unwrap();
    let budget = NxMotionEvidenceBudget {
        max_events: 10,
        max_bytes: 4096,
        max_event_bytes: 1024,
        segment_bytes: 2048,
        max_segments: 2,
    };
    let mut writer =
        EvidenceWriter::new(move |_| Ok(Count(cloned.clone())), budget, binding).unwrap();
    let writes = io.get();
    let events = writer.committed_events();
    assert!(writer.can_append("raw_return", &json!({"code":0})).unwrap());
    assert!(
        !writer
            .can_append("raw_return", &json!({"raw":"x".repeat(2048)}))
            .unwrap()
    );
    assert_eq!(io.get(), writes);
    assert_eq!(writer.committed_events(), events);
    assert!(!writer.poisoned());
    writer.append("raw_return", json!({"code":0})).unwrap();
    writer.seal().unwrap();
}

fn read_json(path: &Path) -> Result<Value> {
    Ok(oe_core::json::strict_json(&oe_core::read_regular_bytes(
        path,
        64 * 1024 * 1024,
    )?)?)
}
fn reference(value: &Value) -> Result<Value> {
    let path = Path::new(value["path"].as_str().context("private_reference_path")?);
    let bytes = oe_core::read_regular_bytes(path, 64 * 1024 * 1024)?;
    ensure!(
        value["bytes"].as_u64() == Some(bytes.len() as u64)
            && value["sha256"].as_str() == Some(oe_core::sha256(&bytes).as_str()),
        "private_fixture_reference_changed"
    );
    Ok(oe_core::json::strict_json(&bytes)?)
}
fn typed<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T> {
    Ok(serde_json::from_value(reference(value)?)?)
}
fn run_core(core: &Value) -> Result<Value> {
    let parameters: ArtifactReference =
        serde_json::from_value(core["parameters_reference"].clone())?;
    let params: NxAssemblyMotionParams2 = typed(&core["parameters"])?;
    let intent: NxAssemblyMotionIntent = typed(&core["intent"])?;
    let closure: NxAssemblySourceClosure = typed(&core["closure"])?;
    let binding: NxMotionEvidenceBinding = typed(&core["binding"])?;
    let budget: NxMotionEvidenceBudget = serde_json::from_value(core["evidence_budget"].clone())?;
    let owned_paths = serde_json::from_value(core["owned_paths"].clone())?;
    let saved: Option<NxAssemblyPositionObservation> = if core["saved_position"].is_null() {
        None
    } else {
        Some(typed(&core["saved_position"])?)
    };
    let summary: Option<NxMotionEvidenceSummary> =
        if core["summary"].is_null() || core["summary_supplied"].as_bool() == Some(false) {
            None
        } else {
            Some(typed(&core["summary"])?)
        };
    let names: Vec<String> = serde_json::from_value(core["observed_names"].clone())?;
    let dir = PathBuf::from(core["job"].as_str().context("private_job_path")?)
        .join("out/motion-evidence");
    let input = Inputs2 {
        parameters: &parameters,
        params: &params,
        intent: &intent,
        closure: &closure,
        binding: &binding,
        receiver_budget: &budget,
        owned_paths: &owned_paths,
        completed: core["completed"].as_bool().context("private_completion")?,
        saved_position: saved.as_ref(),
        abi: NativeBodyAbi {
            solid_type: i32::try_from(
                core["abi"]["solid_type"]
                    .as_i64()
                    .context("private_abi_type")?,
            )?,
            solid_body: i32::try_from(
                core["abi"]["solid_body"]
                    .as_i64()
                    .context("private_abi_body")?,
            )?,
        },
    };
    let result = receiving2::receive(input, summary.as_ref(), &names, |name, visitor| {
        // A deliberately portable test adapter. Production uses pinned streamed
        // handles. Here each bounded synthetic segment is immutable in memory;
        // the original file must still be byte-identical after visitor EOF.
        oe_nx_protocol::checked_token(name)?;
        let fault = core["source_fault"].as_str().unwrap_or("");
        if fault == "zero_calls" {
            return Ok(());
        }
        if fault == "io_error" {
            anyhow::bail!("private_injected_source_io_error");
        }
        let path = dir.join(name);
        let raw = oe_core::read_regular_bytes(&path, usize::try_from(budget.segment_bytes)?)?;
        let mut reader = Cursor::new(&raw);
        visitor(&mut reader)?;
        if fault == "two_calls" {
            let mut second = Cursor::new(&raw);
            visitor(&mut second)?;
        }
        if fault == "pin_changed_after_read" {
            anyhow::bail!("private_injected_file_pin_changed_after_read");
        }
        ensure!(
            reader.position() == raw.len() as u64,
            "private_segment_not_consumed"
        );
        ensure!(
            oe_core::read_regular_bytes(&path, usize::try_from(budget.segment_bytes)?)? == raw,
            "private_segment_changed"
        );
        Ok(())
    });
    Ok(match result {
        Ok(r) => {
            json!({"outcome":r.outcome,"stream_state":r.stream_state,"motion_state":r.motion_state,
            "pair_identity_state":r.pair_identity_state,"query_state":r.query_state,"samples":r.samples,"checked_pairs":r.pairs,
            "interference_pairs":r.interference,"no_interference_pairs":r.no_interference,"touching_pairs":r.touching,
            "save_state":r.save_state.as_str(),"saved_position_checked":r.saved_position_checked,"issues":r.issues,"engineering_adoption":false})
        }
        Err(e) => json!({"outcome":"reject","error":format!("{e:#}"),"engineering_adoption":false}),
    })
}
fn write_new(path: &Path, value: &Value) -> Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(&serde_json::to_vec_pretty(value)?)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}
#[test]
#[ignore = "requires independently frozen private motion fixtures"]
fn independently_frozen_motion_cases() -> Result<()> {
    // List: [{"id":"P01","core":{"path":...,"bytes":...,"sha256":...}},...].
    // The test never generates source events or expected answers.
    let list = PathBuf::from(
        std::env::var_os("OEV2_MOTION2_CASE_LIST").context("OEV2_MOTION2_CASE_LIST required")?,
    );
    let output = PathBuf::from(
        std::env::var_os("OEV2_MOTION2_ACTUAL_DIR").context("OEV2_MOTION2_ACTUAL_DIR required")?,
    );
    ensure!(!output.exists(), "private_actual_dir_must_be_fresh");
    std::fs::create_dir(&output)?;
    let list = read_json(&list)?;
    let cases = list.as_array().context("private_case_list")?;
    ensure!(
        !cases.is_empty() && cases.len() <= 1000,
        "private_case_count"
    );
    let mut ids = BTreeSet::new();
    let mut mismatches = vec![];
    let mut rows = vec![];
    for case in cases {
        let id = case["id"].as_str().context("private_case_id")?;
        oe_nx_protocol::checked_token(id)?;
        ensure!(ids.insert(id), "private_case_duplicate");
        let core = reference(&case["core"])?;
        let expected = reference(&core["expected"])?;
        let actual = match run_core(&core) {
            Ok(v) => v,
            Err(e) => json!({"outcome":"adapter_error","error":format!("{e:#}")}),
        };
        let label = expected["expected"]
            .as_str()
            .or_else(|| expected["expected_state"].as_str())
            .context("private_expected_label")?;
        let wanted = match label {
            "accept" => "accepted_bounded",
            other => other,
        };
        let mut matched = actual["outcome"].as_str() == Some(wanted);
        for key in [
            "samples",
            "checked_pairs",
            "interference_pairs",
            "no_interference_pairs",
            "touching_pairs",
        ] {
            if let Some(value) = expected.get(key) {
                matched &= actual.get(key) == Some(value);
            }
        }
        if !matched {
            mismatches.push(id.to_owned());
        }
        let row = json!({"case":id,"core":case["core"],"expected":core["expected"],"actual":actual,"matched":matched,
            "authority":"synthetic_receiving_test_not_native_evidence"});
        write_new(&output.join(format!("{id}.json")), &row)?;
        rows.push(row);
    }
    write_new(
        &output.join("RESULTS.json"),
        &json!({"cases":rows,"mismatches":mismatches,"native_executions":0}),
    )?;
    ensure!(
        mismatches.is_empty(),
        "private_motion_cases_mismatched:{mismatches:?}"
    );
    Ok(())
}
#[test]
#[ignore = "requires independently frozen private motion fixtures"]
fn independently_frozen_save_wire_rejections() -> Result<()> {
    let manifest = PathBuf::from(
        std::env::var_os("OEV2_MOTION2_SAVE_MANIFEST")
            .context("OEV2_MOTION2_SAVE_MANIFEST required")?,
    );
    let output = PathBuf::from(
        std::env::var_os("OEV2_MOTION2_SAVE_ACTUAL")
            .context("OEV2_MOTION2_SAVE_ACTUAL required")?,
    );
    let data = read_json(&manifest)?;
    let mut actual = vec![];
    let mut failures = vec![];
    for case in data["cases"].as_array().context("private_save_cases")? {
        if case["kind"] != "wire_reject" {
            continue;
        }
        let input = reference(&case["input"])?;
        let expected = reference(&case["expected"])?;
        ensure!(
            expected["expected"]["payload_reception"] == "reject",
            "private_wire_expected_scope"
        );
        let result = receiving2::save::parse_save(&input["input"]["payload"]);
        let rejected = result.is_err();
        if !rejected {
            failures.push(case["id"].clone());
        }
        actual.push(json!({"id":case["id"],"input":case["input"],"expected":case["expected"],"rejected":rejected,"error":result.err().map(|e|format!("{e:#}"))}));
    }
    ensure!(!actual.is_empty(), "private_no_wire_cases");
    write_new(
        &output,
        &json!({"scope":"wire_reject_only_not_capture_or_legacy","actual":actual,"failures":failures,"native_executions":0}),
    )?;
    ensure!(
        failures.is_empty(),
        "private_wire_rejections_failed:{failures:?}"
    );
    Ok(())
}
