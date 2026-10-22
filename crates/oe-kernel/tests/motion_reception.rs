//! Explicit private fixtures exercise the real CAD and Project owners.
//! This is not the CLI parser, native execution, or engineering qualification.
//! Expectations are independently frozen and are consulted only after execution.
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, CadMotionReceiveRequest1, CadMotionReopenRequest1};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
};

const LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Deserialize)]
struct Pin {
    path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    request: Pin,
    expected: Pin,
    #[serde(default)]
    assertions: Value,
}
fn required_env(name: &str) -> Result<String> {
    let value = std::env::var(name).with_context(|| format!("{name} required"))?;
    ensure!(!value.is_empty(), "{name} empty");
    Ok(value)
}
fn put(path: &Path, value: &Value) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(&serde_json::to_vec_pretty(value)?)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}
fn frozen(root: &SkillRoot, pin: &Pin) -> Result<Vec<u8>> {
    let path = root.private_path(Path::new(&pin.path))?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(
        bytes.len() as u64 == pin.bytes && sha256(&bytes) == pin.sha256,
        "independent_fixture_pin_differs"
    );
    Ok(bytes)
}
// Keep the exact current CLI reference boundary: private source, symlink refusal,
// regular bytes and digest. This helper does not inspect transport inventory;
// all such checks must execute in the actual CAD owner.
fn read(root: &SkillRoot, reference: &ArtifactReference) -> Result<Vec<u8>> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_reference_invalid"))?;
    let path = root.expand(Path::new(&reference.path))?;
    ensure!(
        path.starts_with(root.path().join("var")),
        "motion_source_outside_private_root"
    );
    oe_core::no_symlinks(&path)?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "motion_source_digest_differs"
    );
    Ok(bytes)
}
fn mark(trace: &mut Vec<Value>, phase: &str, event: &str) {
    trace.push(json!({"phase":phase,"event":event}));
}
fn run_one(
    root: &SkillRoot,
    pin: &Pin,
    stage: &mut &'static str,
    trace: &mut Vec<Value>,
) -> Result<Value> {
    *stage = "request_fixture_pin";
    let raw = frozen(root, pin)?;
    let reference = ArtifactReference {
        path: pin.path.clone(),
        sha256: pin.sha256.clone(),
    };
    let v = oe_core::json::strict_json(&raw)?;
    // Dispatch solely by the actual request. No expected answer is an argument.
    *stage = "request_schema";
    let reopen = match v["schema"].as_str() {
        Some("oev2.cad-motion-receive/1") => false,
        Some("oev2.cad-motion-receive-reopen/1") => true,
        _ => anyhow::bail!("outer_fixture_request_schema_unsupported"),
    };
    let (motion_reference, request_raw) = if reopen {
        let r: CadMotionReopenRequest1 = serde_json::from_value(v)?;
        let bytes = read(root, &r.motion_request)?;
        (r.motion_request, bytes)
    } else {
        (reference.clone(), raw.clone())
    };
    *stage = "request_contract";
    let request: CadMotionReceiveRequest1 =
        serde_json::from_value(oe_core::json::strict_json(&request_raw)?)?;
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_receive_request_invalid"))?;
    *stage = "project_source";
    let project = read(root, &request.project)?;
    *stage = "kernel_bind_before";
    mark(trace, stage, "started");
    let focus = oe_kernel::project::cad_focus::bind_cad_focus(
        &project,
        &request.object,
        &request.project_context,
    )?;
    mark(trace, stage, "returned");
    *stage = if reopen {
        "cad_receive_reopen"
    } else {
        "cad_receive"
    };
    mark(trace, stage, "started");
    let value = if reopen {
        oe_cad::motion::receive_reopen(root, &reference)?
    } else {
        oe_cad::motion::receive(root, &reference)?
    };
    mark(trace, stage, "returned");
    *stage = "source_recheck";
    ensure!(
        read(root, &reference)? == raw && read(root, &motion_reference)? == request_raw,
        "motion_request_changed_during_check"
    );
    let current_project = read(root, &request.project)?;
    *stage = "kernel_bind_after";
    mark(trace, stage, "started");
    focus.ensure_current(&current_project)?;
    mark(trace, stage, "returned");
    *stage = "outcome_contract";
    let code = match value["outcome"].as_str() {
        Some("accepted_bounded") => 0,
        Some("partial" | "unknown") => 1,
        _ => anyhow::bail!("motion_receive_outcome_invalid"),
    };
    *stage = "finished";
    Ok(json!({"value":value,"equivalent_cli_status":code,"cli_parser_executed":false}))
}
fn assess(expected: &Value, assertions: &Value, actual: &Value) -> Result<()> {
    ensure!(actual["execution"] != "panic", "owner_panicked");
    let returned = actual["execution"] == "returned";
    let value = &actual["result"]["value"];
    match expected["expected_state"]
        .as_str()
        .context("expected_state missing")?
    {
        "reject" => {
            ensure!(actual["execution"] == "error", "expected_rejection");
            ensure!(
                matches!(
                    actual["failure_stage"].as_str(),
                    Some(
                        "kernel_bind_before"
                            | "cad_receive"
                            | "cad_receive_reopen"
                            | "source_recheck"
                            | "kernel_bind_after"
                            | "outcome_contract"
                    )
                ),
                "fixture_or_adapter_error_is_not_an_owner_rejection"
            );
        }
        "accept" | "reopen" => {
            ensure!(
                returned && value["outcome"] == "accepted_bounded",
                "expected_bounded_reception"
            );
            ensure!(
                value["save_state"] == "received",
                "expected_saved_reception"
            );
            ensure!(
                value["engineering_adoption"] == "not_assessed",
                "unexpected_adoption"
            );
            if expected["expected_state"] == "reopen" {
                ensure!(
                    value["reopen_state"] == "received",
                    "expected_reopen_reception"
                );
            }
            for (name, field) in [
                ("samples", "locally_checked_samples"),
                ("pairs", "locally_checked_pairs"),
            ] {
                if let Some(n) = expected["expected_details"].get(name) {
                    ensure!(value.get(field) == Some(n), "expected_count_differs:{name}");
                }
            }
        }
        "scope" => {
            ensure!(returned, "expected_scoped_return");
            ensure!(
                value["engineering_adoption"] == "not_assessed",
                "unexpected_adoption"
            );
            ensure!(
                value["unresolved_claims"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty()),
                "requested_broader_scope_not_preserved"
            );
        }
        _ => anyhow::bail!("independent_expected_state_unsupported"),
    }
    if let Some(checks) = assertions.get("json_equal") {
        for c in checks.as_array().context("json_equal array")? {
            let pointer = c["pointer"].as_str().context("assertion pointer")?;
            let wanted = c.get("value").context("assertion value")?;
            ensure!(
                value.pointer(pointer) == Some(wanted),
                "independent_json_assertion_differs:{pointer}"
            );
        }
    }
    if let Some(checks) = assertions.get("error_contains") {
        for c in checks.as_array().context("error_contains array")? {
            let part = c.as_str().context("error_contains string")?;
            ensure!(!part.is_empty(), "empty_error_assertion");
            ensure!(
                actual["error"].as_str().is_some_and(|s| s.contains(part)),
                "independent_error_assertion_differs:{part}"
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires independently frozen private motion outer fixtures"]
fn independently_frozen_motion_outer_cases() -> Result<()> {
    let root = SkillRoot::new(required_env("OEV2_MOTION_OUTER_ROOT")?)?;
    let manifest_path =
        root.private_path(Path::new(&required_env("OEV2_MOTION_OUTER_MANIFEST")?))?;
    let expected_sha = required_env("OEV2_MOTION_OUTER_MANIFEST_SHA256")?;
    let manifest_raw = read_regular_bytes(&manifest_path, LIMIT)?;
    ensure!(
        sha256(&manifest_raw) == expected_sha,
        "outer_manifest_digest_differs"
    );
    let manifest = oe_core::json::strict_json(&manifest_raw)?;
    let cases = manifest["cases"]
        .as_array()
        .context("outer manifest cases required")?;
    ensure!(!cases.is_empty(), "outer manifest empty");
    let output = root.new_private_dir(Path::new(&required_env("OEV2_MOTION_OUTER_ACTUAL_DIR")?))?;
    put(&output.join("INPUT-MANIFEST.json"), &manifest)?;
    let mut results = vec![];
    let mut ids = BTreeSet::new();
    let mut failures = vec![];
    for (index, raw_case) in cases.iter().enumerate() {
        let mut stage = "fixture_case";
        let mut trace = vec![];
        let mut actual = json!({"schema":"private.motion-outer-actual/1","index":index,
            "case":raw_case,"scope":"real Rust CAD and Project owners on synthetic inputs",
            "cli_parser_executed":false,"native_executed":false,
            "target_guard_assessment":"requires_independent_review"});
        let invoked = catch_unwind(AssertUnwindSafe(|| -> Result<Value> {
            let case: Case = serde_json::from_value(raw_case.clone())?;
            ensure!(
                !case.id.is_empty() && ids.insert(case.id.clone()),
                "outer_fixture_duplicate_or_empty_id"
            );
            stage = "expected_fixture_pin";
            // The original expected file is checked for identity only here.
            // Its contents never reach run_one or select any implementation path.
            let _ = frozen(&root, &case.expected)?;
            run_one(&root, &case.request, &mut stage, &mut trace)
        }));
        match invoked {
            Ok(Ok(value)) => {
                actual["execution"] = json!("returned");
                actual["result"] = value;
            }
            Ok(Err(error)) => {
                actual["execution"] = json!("error");
                actual["error"] = json!(format!("{error:#}"));
                actual["failure_stage"] = json!(stage);
            }
            Err(payload) => {
                actual["execution"] = json!("panic");
                actual["failure_stage"] = json!(stage);
                let text = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic");
                actual["error"] = json!(text);
            }
        }
        actual["trace"] = json!(trace);
        // Assessment occurs after actual invocation, and never alters its result.
        let assessment = (|| -> Result<()> {
            let case: Case = serde_json::from_value(raw_case.clone())?;
            let expected = oe_core::json::strict_json(&frozen(&root, &case.expected)?)?;
            assess(&expected, &case.assertions, &actual)
        })();
        match assessment {
            Ok(()) => {
                actual["frozen_expectation_comparison"] = json!("matched_pending_guard_review")
            }
            Err(error) => {
                actual["frozen_expectation_comparison"] = json!("mismatch");
                actual["comparison_error"] = json!(format!("{error:#}"));
                failures.push(index);
            }
        }
        let file = output.join(format!("actual-{index:03}.json"));
        put(&file, &actual)?;
        let bytes = read_regular_bytes(&file, LIMIT)?;
        results.push(
            json!({"index":index,"id":raw_case["id"],"execution":actual["execution"],
            "comparison":actual["frozen_expectation_comparison"],
            "actual":{"path":file,"bytes":bytes.len(),"sha256":sha256(&bytes)}}),
        );
    }
    let manifest_stable = read_regular_bytes(&manifest_path, LIMIT)? == manifest_raw;
    put(
        &output.join("SUMMARY.json"),
        &json!({
            "schema":"private.motion-outer-summary/1",
            "manifest":{"path":manifest_path,"bytes":manifest_raw.len(),"sha256":expected_sha},
            "input_manifest_unchanged":manifest_stable,"cases_attempted":results.len(),
            "mismatch_indices":failures,"actual":results,
            "target_guard_assessment":"requires_independent_review",
            "cli_parser_executed":false,"native_executed":false,"engineering_adoption":"not_assessed"
        }),
    )?;
    ensure!(manifest_stable, "outer_manifest_changed_during_execution");
    ensure!(
        failures.is_empty(),
        "outer_frozen_expectation_mismatches:{failures:?}"
    );
    Ok(())
}
