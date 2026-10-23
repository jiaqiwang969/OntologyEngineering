//! Independent expected values and mutations. Existing actual replay records
//! are evidence inputs; absent fixtures fail, never silently skip a test.
use anyhow::Result;
use oe_core::process::{CommandSpec, run_command};
use oe_core::{SkillRoot, read_regular_bytes, write_new, write_new_bytes};
use oe_evolution::*;
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn root() -> SkillRoot {
    SkillRoot::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap(),
    )
    .unwrap()
}
fn original() -> CodeComparisonRequest {
    let path = root()
        .path()
        .join("var/maintenance/first-comparison/comparison-input.json");
    serde_json::from_value(
        strict_json(&read_regular_bytes(&path, 16 * 1024 * 1024).unwrap()).unwrap(),
    )
    .unwrap()
}
fn folder() -> PathBuf {
    root()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/code-protocol-tests/{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        )))
        .unwrap()
}
fn program(
    exe: &Path,
    args: Vec<std::ffi::OsString>,
    evidence: &Path,
) -> oe_core::process::ProcessObservation {
    program_with_timeout(exe, args, evidence, Duration::from_secs(30))
}
fn program_with_timeout(
    exe: &Path,
    args: Vec<std::ffi::OsString>,
    evidence: &Path,
    timeout: Duration,
) -> oe_core::process::ProcessObservation {
    run_command(
        &CommandSpec {
            executable: exe.into(),
            arguments: args,
            cwd: root().path().into(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout,
            output_limit: 4 * 1024 * 1024,
        },
        evidence,
    )
    .unwrap()
}
fn evaluate(request: &CodeComparisonRequest) -> (bool, Value) {
    let folder = folder();
    let input = folder.join("input.json");
    write_new(&input, &serde_json::to_value(request).unwrap()).unwrap();
    let output = program(
        Path::new(&request.experiment.evaluator.path),
        vec!["compare".into(), input.as_os_str().into()],
        &folder.join("process"),
    );
    (
        output.status == "completed" && output.exit_code == Some(0),
        strict_json(&output.stdout)
            .unwrap_or_else(|_| json!({"error":String::from_utf8_lossy(&output.stderr)})),
    )
}
fn driver_rejects(experiment: &CodeExperiment, folder: &Path, expected_failure: &str) -> bool {
    let input = folder.join("experiment.json");
    write_new(&input, &serde_json::to_value(experiment).unwrap()).unwrap();
    let expected = digest(&serde_json::to_value(experiment).unwrap()).unwrap();
    let exe = Path::new(&original().experiment.driver.path).to_owned();
    let output = program(
        &exe,
        vec![
            "run".into(),
            root().path().as_os_str().into(),
            input.as_os_str().into(),
            expected.into(),
            format!(
                "var/maintenance/rejected-{}",
                NEXT.fetch_add(1, Ordering::SeqCst)
            )
            .into(),
        ],
        &folder.join("driver"),
    );
    // A killed or unstarted process is not evidence that the drift guard fired.
    // oe-core calls an ordinary nonzero exit process_failed. Require complete
    // capture and the frozen rejection reason, never timeout or spawn failure.
    output.status == "process_failed"
        && output.exit_code.is_some_and(|code| code != 0)
        && output.report["capture_complete"] == true
        && String::from_utf8_lossy(&output.stderr).contains(&format!("Error: {expected_failure}"))
}
fn clone_public(source: &Path, dest: &Path) -> Result<()> {
    oe_core::create_new_private_dir(dest)?;
    let manifest = public_manifest(source)?;
    for path in manifest["files"].as_object().unwrap().keys() {
        let output = dest.join(path);
        std::fs::create_dir_all(output.parent().unwrap())?;
        std::fs::copy(source.join(path), output)?;
    }
    Ok(())
}
#[test]
fn actual_distinct_binary_runs_preserve_fixed_oracle_and_all_slots() {
    let request = original();
    assert_ne!(
        request.experiment.baseline.runner.sha256,
        request.experiment.candidate.runner.sha256
    );
    assert_ne!(
        request.experiment.baseline.source_root,
        request.experiment.candidate.source_root
    );
    assert_eq!(request.observations.len(), 12);
    for case in &request.experiment.cases {
        let observations: Vec<_> = request
            .observations
            .iter()
            .filter(|o| o.case_id == case.case_id)
            .collect();
        assert_eq!(observations.len(), 2);
        assert_eq!(
            observations[0].invocation_sha256,
            observations[1].invocation_sha256
        );
        assert_eq!(observations[0].process["status"], "completed");
        assert_eq!(observations[1].process["status"], "completed");
    }
    let (ok, result) = evaluate(&request);
    assert!(ok);
    assert_eq!(result["fixed_denominator"], 6);
    assert_eq!(result["assessment"], "inconclusive");
    assert_eq!(result["engineering_adoption"], "unknown");
    assert_eq!(result["version_selection"], "not_connected");
}
#[test]
fn executable_drift_rejects_before_any_new_replay() {
    for side in ["driver", "evaluator", "baseline"] {
        let folder = folder();
        let mut experiment = original().experiment;
        let reference = match side {
            "driver" => &mut experiment.driver,
            "evaluator" => &mut experiment.evaluator,
            _ => &mut experiment.baseline.runner,
        };
        let changed = folder.join("changed-binary");
        std::fs::copy(&reference.path, &changed).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // copy preserves the read-only frozen executable's permissions.
            std::fs::set_permissions(&changed, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        reference.path = changed.to_str().unwrap().into();
        std::fs::write(&changed, b"changed executable").unwrap();
        assert!(driver_rejects(
            &experiment,
            &folder,
            "code_executable_changed"
        ));
    }
}
#[test]
fn oracle_drift_is_not_a_new_freeze_of_the_same_experiment() {
    let folder = folder();
    let mut experiment = original().experiment;
    let oracle = folder.join("oracle.json");
    write_new_bytes(&oracle, b"[]").unwrap();
    experiment.oracle_path = oracle.to_str().unwrap().into();
    assert!(driver_rejects(&experiment, &folder, "code_oracle_changed"));
    let mut request = original();
    request.experiment = experiment;
    assert!(!evaluate(&request).0);
}
#[test]
fn compiled_source_and_full_set_drift_are_detected() {
    let folder = folder();
    let mut artifact = original().experiment.baseline;
    let copy = folder.join("source");
    clone_public(Path::new(&artifact.source_root), &copy).unwrap();
    artifact.source_root = copy.to_str().unwrap().into();
    artifact.runner.source_root = artifact.source_root.clone();
    std::fs::write(
        copy.join("crates/semantica-runtime/src/lib.rs"),
        b"old source changed",
    )
    .unwrap();
    assert!(verify_artifact(&artifact).is_err());
    std::fs::write(copy.join("unexpected-public.rs"), b"// source added").unwrap();
    assert!(verify_artifact(&artifact).is_err());
}
#[test]
fn static_manifest_resigning_cannot_hide_an_old_runner_compile() {
    let folder = folder();
    let mut experiment = original().experiment;
    let copy = folder.join("source");
    clone_public(Path::new(&experiment.candidate.source_root), &copy).unwrap();
    std::fs::write(
        copy.join("runtime/code-runner-identity.json"),
        b"{\"identity\":\"new source not compiled\"}",
    )
    .unwrap();
    let artifact = &mut experiment.candidate;
    artifact.source_root = copy.to_str().unwrap().into();
    artifact.runner.source_root = artifact.source_root.clone();
    // This retained experiment uses the original seven-field identity domain.
    // Keep that domain for the forged private descriptor so rejection must
    // reach the old runner's actual compiled identity, not an earlier profile
    // mismatch. Neither the frozen source nor its original manifest is changed.
    assert_eq!(artifact.manifest.as_object().unwrap().len(), 7);
    let mut forged = public_manifest(&copy).unwrap();
    // Strip the three current profile declarations from this deliberately forged
    // test descriptor, then independently reread it under the original 16 MiB
    // domain. Merely removing fields cannot bypass that second bounded check.
    let fields = forged.as_object_mut().unwrap();
    assert!(fields.remove("resource_profile").is_some());
    assert!(fields.remove("file_byte_limits").is_some());
    assert_eq!(
        fields.remove("public_sources"),
        Some(json!(["sources/README.md"]))
    );
    assert_eq!(fields.len(), 7);
    // The old domain excluded the entire sources tree, including any README
    // that a current public clone may now carry. Re-observe that exact domain.
    if let Some(readme) = forged["files"]
        .as_object_mut()
        .unwrap()
        .remove("sources/README.md")
    {
        forged["file_count"] = json!(forged["file_count"].as_u64().unwrap() - 1);
        forged["total_bytes"] =
            json!(forged["total_bytes"].as_u64().unwrap() - readme["bytes"].as_u64().unwrap());
    }
    artifact.manifest = public_manifest_matching(&copy, &forged).unwrap();
    artifact.manifest_sha256 = digest(&artifact.manifest).unwrap();
    artifact.runner.source_manifest_sha256 = artifact.manifest_sha256.clone();
    artifact.compiled["manifest"] = artifact.manifest.clone();
    artifact.compiled["manifest_sha256"] = json!(artifact.manifest_sha256);
    assert!(driver_rejects(
        &experiment,
        &folder,
        "code_runner_compiled_source_mismatch"
    ));
}
#[test]
fn actual_ggen_replay_detects_hand_edited_generated_bytes() {
    let folder = folder();
    let experiment = original().experiment;
    let artifact = experiment.baseline;
    let copy = folder.join("source");
    clone_public(Path::new(&artifact.source_root), &copy).unwrap();
    let product = copy.join("generated/contracts.rs");
    let mut bytes = std::fs::read(&product).unwrap();
    bytes.extend_from_slice(b"\n// hand edit\n");
    std::fs::write(product, bytes).unwrap();
    let normalizer = artifact.generation["normalizer_path"].as_str().unwrap();
    let ggen = &artifact.tools[2].path;
    let output = program_with_timeout(
        Path::new(normalizer),
        vec!["check".into(), copy.as_os_str().into(), ggen.into()],
        &folder.join("actual-ggen"),
        // This is the separate generation-check profile; replay case budgets
        // remain the original frozen 10 seconds. Core generation uses 120 s.
        Duration::from_secs(120),
    );
    assert_eq!(output.status, "completed", "a timeout is fixture failure");
    assert_eq!(
        output.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = strict_json(&output.stdout).unwrap();
    assert_eq!(result["status"], "drift");
    assert_eq!(
        result["generation"]["normalizer_binding"]["status"],
        "matched"
    );
    assert!(
        result["drift"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "contracts.rs")
    );
}
#[test]
fn runner_scores_or_extra_answer_fields_do_not_authorize_results() {
    let observation = original().observations[0].observed.clone().unwrap();
    let mut value = serde_json::to_value(&observation).unwrap();
    value["score"] = json!(1);
    assert!(serde_json::from_value::<CodeRunnerObservation>(value).is_err());
    let mut request = original();
    request.observations[0].observed.as_mut().unwrap().result["score"] = json!(1);
    let (ok, result) = evaluate(&request);
    assert!(ok);
    assert_eq!(result["slots"][0]["baseline"]["status"], "failed");
}
#[test]
fn absent_usage_and_cost_stay_unknown() {
    let request = original();
    assert!(
        request
            .observations
            .iter()
            .all(|o| o.usage.is_none() && o.cost.is_none())
    );
    let (ok, result) = evaluate(&request);
    assert!(ok);
    assert_eq!(result["cost"], "unknown");
}
#[test]
fn skipped_failed_and_unknown_slots_keep_the_denominator() {
    let mut request = original();
    request.observations.remove(1);
    request.observations[0].execution = "failed".into();
    request.observations[0].observed = None;
    let (ok, result) = evaluate(&request);
    assert!(ok);
    assert_eq!(result["fixed_denominator"], 6);
    assert_eq!(result["slots"].as_array().unwrap().len(), 6);
    assert_eq!(result["slots"][0]["baseline"]["status"], "failed");
    assert_eq!(result["slots"][0]["candidate"]["status"], "unknown");
    assert_eq!(result["assessment"], "inconclusive");
}
#[test]
fn changed_budget_or_conditions_and_oracle_are_rejected() {
    for mutation in ["budget", "conditions", "expected"] {
        let mut request = original();
        match mutation {
            "budget" => request.experiment.cases[0].budget.output_bytes = 1024,
            "conditions" => request.experiment.cases[0].conditions_sha256 = "a".repeat(64),
            _ => request.experiment.cases[0].expected = json!({"kind":"ask","value":false}),
        }
        request.experiment.case_set_sha256 =
            digest(&serde_json::to_value(&request.experiment.cases).unwrap()).unwrap();
        assert!(!evaluate(&request).0);
    }
}
#[test]
fn duplicate_or_extra_observation_cannot_inflate_coverage() {
    let mut request = original();
    request.observations.push(request.observations[0].clone());
    assert!(!evaluate(&request).0);
}
#[test]
fn select_bags_ignore_row_order_but_keep_duplicate_and_unbound_semantics() {
    let mut request = original();
    let index = request
        .experiment
        .cases
        .iter()
        .position(|c| c.comparison == "select_bag")
        .unwrap();
    let id = request.experiment.cases[index].case_id.clone();
    let o = request
        .observations
        .iter_mut()
        .find(|o| o.side == "candidate" && o.case_id == id)
        .unwrap();
    o.observed.as_mut().unwrap().result["rows"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let (ok, result) = evaluate(&request);
    assert!(ok);
    assert_eq!(result["slots"][index]["candidate"]["status"], "matched");
    let o = request
        .observations
        .iter_mut()
        .find(|o| o.side == "candidate" && o.case_id == id)
        .unwrap();
    o.observed.as_mut().unwrap().result["rows"][0]["missing"] = json!({"kind":"literal","value":"","datatype":"http://www.w3.org/2001/XMLSchema#string","language":null});
    let (ok, result) = evaluate(&request);
    assert!(ok);
    assert_eq!(result["slots"][index]["candidate"]["status"], "failed");
}
#[test]
fn explicit_unsupported_keeps_original_required_behavior_failed() {
    let request = original();
    let (ok, result) = evaluate(&request);
    assert!(ok);
    let last = result["slots"].as_array().unwrap().last().unwrap();
    assert_eq!(last["candidate"]["safe_explicit_rejection"], true);
    assert_eq!(last["candidate"]["status"], "failed");
    assert_eq!(last["candidate"]["expected_match"], false);
}
#[test]
fn all_four_owner_operations_remain_not_connected() {
    for operation in ["select", "use", "feedback", "recover"] {
        let status = owner_connection(operation).unwrap();
        assert_eq!(status.status, "not_connected");
    }
}
#[test]
fn frozen_protocol_requirements_are_all_nineteen_not_all_verified() {
    let experiment = original().experiment;
    assert_eq!(experiment.protocol_case_ids.len(), 19);
    assert!(
        experiment
            .uncovered
            .iter()
            .any(|s| s.contains("no real 169"))
    );
    assert_ne!(experiment.scope_sha256, experiment.case_set_sha256);
}
