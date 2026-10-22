//! Software capture seams only; no SDK or native execution qualification.
use anyhow::{Context, Result, bail, ensure};
use oe_contracts::NxMotionSaveObservation2;
use oe_nx_worker::geometry::motion_task::save_capture2::{
    self as capture, RawSave2, SaveApi2, SaveCaptureSink2,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Trace {
    calls: u64,
    reads: u64,
    order: Vec<String>,
}
struct MockApi {
    input: Value,
    trace: Rc<RefCell<Trace>>,
    stop: Option<&'static str>,
}
fn paths(input: &Value) -> Vec<String> {
    input
        .get("owned_paths")
        .or_else(|| input.get("checked_prefix"))
        .map(|v| serde_json::from_value(v.clone()).unwrap())
        .unwrap_or_else(|| {
            vec![
                r"C:\synthetic-motion\job\out\motion-assembly\root.prt".into(),
                r"C:\synthetic-motion\job\out\motion-assembly\pin.prt".into(),
            ]
        })
}
impl SaveApi2 for MockApi {
    fn loaded_part_count(&mut self) -> i32 {
        self.input["inventory_count"]
            .as_i64()
            .map(|v| i32::try_from(v).unwrap())
            .unwrap_or(paths(&self.input).len() as i32)
    }
    fn owned_part_path(&mut self, index: i32) -> Result<String> {
        if self.input["refused_at_ordinal"].as_i64() == Some(i64::from(index)) {
            bail!("{}", self.input["refusal"].as_str().context("refusal")?);
        }
        paths(&self.input)
            .get(index as usize)
            .cloned()
            .context("mock undeclared path")
    }
    fn call_save(&mut self) -> Result<RawSave2> {
        if self.stop == Some("before") {
            bail!("mock stop before UF entry");
        }
        self.trace.borrow_mut().calls += 1;
        self.trace.borrow_mut().order.push("UF_call".into());
        if self.stop == Some("after") {
            bail!("mock stop after UF entry without return");
        }
        let r = &self.input["raw_abi"];
        let code = i32::try_from(r["return_code"].as_i64().context("raw code")?)?;
        let count = i32::try_from(r["file_error_count_raw"].as_i64().context("raw count")?)?;
        let parts = r["allocated_parts"].as_array();
        let codes = r["allocated_codes"].as_array();
        self.trace.borrow_mut().order.push("copy_raw".into());
        Ok(capture::copy_file_errors2(
            code,
            count,
            parts.is_some(),
            codes.is_some(),
            |i| {
                self.trace.borrow_mut().reads += 1;
                Ok((
                    u32::try_from(
                        parts
                            .context("unexpected NULL parts read")?
                            .get(i)
                            .context("mock part bounds")?
                            .as_u64()
                            .context("part")?,
                    )?,
                    i32::try_from(
                        codes
                            .context("unexpected NULL codes read")?
                            .get(i)
                            .context("mock code bounds")?
                            .as_i64()
                            .context("code")?,
                    )?,
                ))
            },
        ))
    }
}
struct MockSink {
    input: Value,
    events: Vec<Value>,
    trace: Rc<RefCell<Trace>>,
}
impl SaveCaptureSink2 for MockSink {
    fn can_record(&self, o: &NxMotionSaveObservation2) -> Result<bool> {
        if o.phase == "call_returned" && self.input["fail_at"] == "returned_metadata_capacity" {
            return Ok(false);
        }
        Ok(self.input["fixture_retained_error_capacity_entries"]
            .as_u64()
            .is_none_or(|n| o.file_errors.len() as u64 <= n))
    }
    fn record(&mut self, o: &NxMotionSaveObservation2) -> Result<()> {
        if o.phase == "call_pending" && self.input["fail_at"] == "pending_commit" {
            bail!("injected pending persistence failure");
        }
        if o.phase == "call_returned" && self.input["fail_at"] == "returned_commit" {
            bail!("injected returned persistence failure");
        }
        ensure!(self.can_record(o)?, "mock record exceeded frozen capacity");
        self.events.push(serde_json::to_value(o)?);
        self.trace.borrow_mut().order.push(
            match o.phase.as_str() {
                "call_pending" => "pending_commit",
                "call_returned" => "returned_commit",
                _ => "preflight_refused_commit",
            }
            .into(),
        );
        Ok(())
    }
}
fn run(
    input: Value,
    stop: Option<&'static str>,
) -> (
    Result<NxMotionSaveObservation2>,
    Vec<Value>,
    Rc<RefCell<Trace>>,
) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut api = MockApi {
        input: input.clone(),
        trace: trace.clone(),
        stop,
    };
    let mut sink = MockSink {
        input,
        events: vec![],
        trace: trace.clone(),
    };
    let r = capture::execute(&mut api, &mut sink);
    if r.is_err() {
        trace.borrow_mut().order.push("classify_failure".into());
    }
    (r, sink.events, trace)
}

#[test]
fn malformed_array_headers_never_read_the_mock_allocation() {
    for (count, p, c) in [
        (-1, true, true),
        (4097, true, true),
        (1, false, true),
        (1, true, false),
    ] {
        let r = capture::copy_file_errors2(17, count, p, c, |_| {
            panic!("must not dereference malformed storage")
        });
        assert_eq!(r.return_code, 17);
        assert_eq!(r.file_error_count, count);
        assert_eq!(r.file_errors_capture, "invalid_shape");
        assert!(r.file_errors.is_empty());
    }
}
#[test]
fn pending_persistence_failure_prevents_mock_uf_entry() {
    let (r, events, trace) = run(json!({"fail_at":"pending_commit"}), None);
    assert!(r.is_err());
    assert!(events.is_empty());
    assert_eq!(trace.borrow().calls, 0);
}
#[test]
fn nonzero_native_return_is_durable_before_failure_and_cleanup_does_not_normalize_it() {
    let (r, events, trace) = run(
        json!({"raw_abi":{"return_code":17,"file_error_count_raw":1,"allocated_parts":[101],"allocated_codes":[29]}}),
        None,
    );
    assert!(r.is_err());
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["call_entered"], Value::Null);
    assert_eq!(events[1]["return_code"], 17);
    assert_eq!(events[1]["file_errors"][0]["return_code"], 29);
    assert_eq!(
        trace.borrow().order,
        vec![
            "pending_commit",
            "UF_call",
            "copy_raw",
            "returned_commit",
            "classify_failure"
        ]
    );
}
#[test]
fn compiled_abi_dto_contract_is_separate_from_actual_sdk_qualification() {
    let mut abi = oe_contracts::NxMotionCompiledAbi2 {
        schema: "oev2.nx-motion-compiled-abi/2".into(),
        abi_version: 2,
        solid_type: 70,
        solid_subtype: 0,
        solid_body: 5601,
    };
    assert!(abi.validate().is_ok());
    abi.schema = "unrecognized".into();
    assert!(abi.validate().is_err());
    // These synthetic constants only exercise DTO meaning. Actual receipt
    // provenance and loaded shim remain independent platform/native obligations.
}

fn pinned(v: &Value) -> Result<Value> {
    let b = std::fs::read(v["path"].as_str().context("fixture pin path")?)?;
    ensure!(
        v["bytes"].as_u64() == Some(b.len() as u64) && v["sha256"] == oe_core::sha256(&b),
        "fixture pin differs"
    );
    Ok(serde_json::from_slice(&b)?)
}
#[test]
#[ignore = "requires explicit independent SAVE-FIXTURES-FROZEN manifest and digest; does not execute legacy Native methods"]
fn private_frozen_save_capture_cases() -> Result<()> {
    let path = std::env::var("OEV2_MOTION_SAVE_FIXTURE_MANIFEST")?;
    let expected_sha = std::env::var("OEV2_MOTION_SAVE_FIXTURE_SHA256")?;
    let b = std::fs::read(&path)?;
    ensure!(
        oe_core::sha256(&b) == expected_sha,
        "private manifest changed"
    );
    let manifest: Value = serde_json::from_slice(&b)?;
    ensure!(
        manifest["schema"] == "private.motion-save-independent-fixtures/2"
            && manifest["case_count"] == 60,
        "frozen manifest scope"
    );
    let actual_dir = std::path::PathBuf::from(std::env::var("OEV2_MOTION_SAVE_ACTUAL_DIR")?);
    ensure!(
        actual_dir.is_absolute() && !actual_dir.exists(),
        "fresh absolute actual directory required"
    );
    oe_core::no_symlinks(actual_dir.parent().context("actual parent")?)?;
    std::fs::create_dir(&actual_dir)?;
    let write_actual = |name: &str, value: &Value| -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(actual_dir.join(name))?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        Ok(())
    };
    let mut failures = Vec::new();
    let mut capture_count = 0;
    let mut deferred = vec![];
    for c in manifest["cases"].as_array().context("cases")? {
        let id = c["id"].as_str().context("id")?;
        let kind = c["kind"].as_str().context("kind")?;
        if matches!(kind, "wire_reject" | "legacy_caller") {
            deferred.push(id.to_owned());
            continue;
        }
        ensure!(
            matches!(
                kind,
                "capture_preflight" | "capture_return" | "capture_persistence" | "pending_stream"
            ),
            "unsupported capture fixture kind"
        );
        let mut actual = json!({"case_id":id,"input":c["input"],"expected":c["expected"],"executed":false,"native_executed":false,"origin":"synthetic_mock_not_native_evidence"});
        let checked = (|| -> Result<()> {
            let input = pinned(&c["input"])?;
            let expected = pinned(&c["expected"])?;
            ensure!(input["id"] == id && expected["id"] == id, "case identity");
            let stop = if kind == "pending_stream" {
                Some(if id == "SC02-stop-before-entry" {
                    "before"
                } else {
                    "after"
                })
            } else {
                None
            };
            let (result, events, trace) = run(input["input"].clone(), stop);
            let e = &expected["expected"];
            let t = trace.borrow();
            actual["executed"] = json!(true);
            actual["events"] = json!(events);
            actual["save_calls"] = json!(t.calls);
            actual["arrays_read"] = json!(t.reads);
            actual["order"] = json!(t.order);
            actual["operation_result"] = match &result {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":format!("{error:#}")}),
            };
            if !e["exact_payload"].is_null() {
                ensure!(
                    events.last() == Some(&e["exact_payload"]),
                    "{id}: exact payload differs"
                );
            }
            if let Some(a) = e["exact_surviving_events"].as_array() {
                ensure!(&events == a, "{id}: surviving events differ");
            }
            if kind == "pending_stream" {
                ensure!(
                    events
                        == input["input"]["surviving_events"]
                            .as_array()
                            .unwrap()
                            .clone(),
                    "{id}: pending bytes differ"
                );
            }
            if let Some(n) = e["save_calls"].as_u64() {
                ensure!(t.calls == n, "{id}: call count differs");
            }
            if let Some(n) = e["arrays_read"].as_u64() {
                ensure!(t.reads == n, "{id}: read count differs");
            }
            if let Some(n) = e["arrays_read_minimum"].as_u64() {
                ensure!(t.reads >= n, "{id}: read count below bound");
            }
            if let Some(n) = e["arrays_read_maximum"].as_u64() {
                ensure!(t.reads <= n, "{id}: read count above bound");
            }
            if let Some(a) = e["required_order"].as_array() {
                ensure!(
                    serde_json::to_value(&t.order)? == json!(a),
                    "{id}: operation order differs"
                );
            }
            if let Some(a) = e["emitted_outer_phases"].as_array() {
                let phases = events
                    .iter()
                    .map(|v| capture::outer_phase(&serde_json::from_value(v.clone())?))
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    serde_json::to_value(phases)? == json!(a),
                    "{id}: outer phases differ"
                );
            }
            if e["save_call"] == "success" {
                ensure!(result.is_ok(), "{id}: success expected {result:?}");
            } else {
                ensure!(result.is_err(), "{id}: failure/unknown must not succeed");
            }
            Ok(())
        })();
        actual["expected_matched"] = json!(checked.is_ok());
        if let Err(error) = checked {
            actual["check_error"] = json!(format!("{error:#}"));
            failures.push(id.to_owned());
        }
        write_actual(&format!("{id}.json"), &actual)?;
        capture_count += 1;
    }
    write_actual(
        "SUMMARY.json",
        &json!({"manifest_path":path,"manifest_sha256":expected_sha,"capture_cases":capture_count,"failed_cases":failures,"deferred":deferred,"native_runs":0,"scope":"capture software only; deferred wire and legacy cases are not passed by this run"}),
    )?;
    ensure!(
        capture_count == 28 && deferred.len() == 32,
        "unexpected independent fixture split"
    );
    ensure!(
        failures.is_empty(),
        "capture fixture mismatches: {}",
        failures.join(",")
    );
    eprintln!(
        "capture_cases={capture_count}; wire_reject=12 handled by independent receiver tests; legacy_caller=20 not exercised by this adapter"
    );
    Ok(())
}
