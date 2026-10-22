//! Source-bound composition for the existing version owner. The fixed external
//! driver owns replay and comparison; finite replay never grants full release.
use crate::{artifacts, registry};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    CodeArtifactEligibilityRequest, CodeArtifactEligibilityResult, CodeComparison, CodeExperiment,
    CodeQualificationContext, CodeScopeWitnessRequest, CodeSourceArtifact,
};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{SkillRoot, write_new};
use oe_evolution::scope_witness::SelectedReport;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static QUALIFICATION_SERIAL: AtomicU64 = AtomicU64::new(0);

mod association;

fn scope_request(
    root: &SkillRoot,
    context: &CodeQualificationContext,
    reference: Option<&oe_contracts::ArtifactReference>,
    experiment: &CodeExperiment,
) -> Result<Option<CodeScopeWitnessRequest>> {
    let Some(reference) = reference else {
        return Ok(None);
    };
    root.private_path(Path::new(&reference.path))?;
    let request: CodeScopeWitnessRequest =
        serde_json::from_value(artifacts::json(root, reference)?)?;
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("code_qualification_scope_request_contract:{e:?}"))?;
    ensure!(
        context.schema == "oev2.code-qualification-context/1"
            && request.experiment_sha256 == context.experiment_sha256,
        "code_qualification_full_scope_fixed_profile_changed"
    );
    ensure!(
        (1..=600_000).contains(&request.dispatch_budget.timeout_ms)
            && (1024..=4 * 1024 * 1024).contains(&request.dispatch_budget.output_bytes)
            && request
                .dispatch_budget
                .timeout_ms
                .checked_add(30_000)
                .is_some_and(|v| v <= context.dispatch_budget.timeout_ms)
            && request.dispatch_budget.output_bytes <= context.dispatch_budget.output_bytes,
        "code_qualification_scope_nested_budget"
    );
    oe_evolution::scope_witness::validate_request(
        Path::new(&context.driver.source_root),
        &request,
        experiment,
    )?;
    Ok(Some(request))
}

fn scope_result(
    context: &CodeQualificationContext,
    experiment: &CodeExperiment,
    request: &CodeScopeWitnessRequest,
    selected: &SelectedReport,
) -> Result<()> {
    selected.validate(Path::new(&context.driver.source_root), request)?;
    let report = selected.observation();
    let witness = &report.witness;
    ensure!(
        witness.experiment_sha256 == context.experiment_sha256
            && witness.driver_sha256 == context.driver.sha256
            && witness.evaluator_sha256 == context.evaluator.sha256
            && witness.oracle_sha256 == experiment.oracle_sha256
            && witness.baseline_sha256 == experiment.baseline.manifest_sha256
            && witness.candidate_sha256 == experiment.candidate.manifest_sha256,
        "code_qualification_scope_witness_identity_changed"
    );
    let driver_root = SkillRoot::new(&context.driver.source_root)?;
    let process = artifacts::json(&driver_root, &witness.process)?;
    ensure!(
        process["executable"]["identity"]["sha256"] == context.driver.sha256
            && process["executable"]["unchanged"] == true
            && process["arguments"][0] == "run"
            && process["arguments"][1] == context.driver.source_root
            && process["arguments"][3] == context.experiment_sha256
            && process["cwd"] == context.driver.source_root
            && (witness.execution != "completed"
                || (process["status"] == "completed"
                    && process["exit_code"] == 0
                    && process["capture_complete"] == true)),
        "code_qualification_retained_scope_process_changed"
    );
    let path = driver_root.private_path(Path::new(&witness.process.path))?;
    let folder = path
        .parent()
        .context("code_qualification_scope_process_parent")?;
    for stream in ["stdout", "stderr"] {
        let raw = oe_core::read_regular_bytes(&folder.join(stream), 4 * 1024 * 1024)?;
        ensure!(
            process[stream]["sha256"] == oe_core::sha256(&raw)
                && process[stream]["bytes"] == raw.len() as u64,
            "code_qualification_retained_scope_stream_changed"
        );
        if stream == "stdout" {
            if let Some(comparison) = &witness.comparison {
                ensure!(
                    oe_core::json::strict_json(&raw)? == serde_json::to_value(comparison)?,
                    "code_qualification_retained_scope_comparison_changed"
                );
            }
        }
    }
    let matched: BTreeSet<&str> = if witness.execution == "completed" {
        witness
            .comparison
            .as_ref()
            .context("code_qualification_scope_comparison_missing")?
            .slots
            .iter()
            .filter(|slot| slot["candidate"]["status"] == "matched")
            .filter_map(|slot| slot["case_id"].as_str())
            .collect()
    } else {
        BTreeSet::new()
    };
    let actual: BTreeMap<&str, &Vec<String>> = report
        .items
        .iter()
        .map(|item| (item.scope_id.as_str(), &item.observed_case_ids))
        .collect();
    for item in &request.items {
        let expected: Vec<String> = item
            .case_ids
            .iter()
            .filter(|id| matched.contains(id.as_str()))
            .cloned()
            .collect();
        ensure!(
            actual.get(item.scope_id.as_str()).copied() == Some(&expected),
            "code_qualification_scope_observations_changed"
        );
    }
    Ok(())
}

fn inputs(
    root: &SkillRoot,
    input: &CodeArtifactEligibilityRequest,
    context: &CodeQualificationContext,
) -> Result<(CodeSourceArtifact, CodeComparison, CodeExperiment)> {
    let source: CodeSourceArtifact = serde_json::from_value(artifacts::json(root, &input.source)?)?;
    let assurance: CodeComparison =
        serde_json::from_value(artifacts::json(root, &input.assurance)?)?;
    root.private_path(Path::new(&context.experiment.path))?;
    root.private_path(Path::new(&context.required_protocol.path))?;
    let experiment: CodeExperiment =
        serde_json::from_value(artifacts::json(root, &context.experiment)?)?;
    source
        .validate()
        .map_err(|e| anyhow::anyhow!("code_qualification_source_contract:{e:?}"))?;
    assurance
        .validate()
        .map_err(|e| anyhow::anyhow!("code_qualification_assurance_contract:{e:?}"))?;
    experiment
        .validate()
        .map_err(|e| anyhow::anyhow!("code_qualification_experiment_contract:{e:?}"))?;
    ensure!(
        oe_evolution::digest(&serde_json::to_value(&experiment)?)? == context.experiment_sha256,
        "code_qualification_experiment_changed"
    );
    ensure!(
        source == experiment.candidate
            && context.driver == experiment.driver
            && context.evaluator == experiment.evaluator,
        "code_qualification_artifact_or_tool_binding_changed"
    );
    ensure!(
        assurance.schema == "oev2.code-comparison/1"
            && assurance.experiment_id == experiment.experiment_id
            && assurance.driver_sha256 == experiment.driver.sha256
            && assurance.evaluator_sha256 == experiment.evaluator.sha256
            && assurance.oracle_sha256 == experiment.oracle_sha256
            && assurance.baseline_sha256 == experiment.baseline.manifest_sha256
            && assurance.candidate_sha256 == experiment.candidate.manifest_sha256,
        "code_qualification_comparison_binding_changed"
    );
    let protocol = artifacts::json(root, &context.required_protocol)?;
    let ids = protocol["cases"]
        .as_array()
        .context("code_qualification_protocol_cases")?
        .iter()
        .map(|row| {
            row["case_id"]
                .as_str()
                .map(str::to_owned)
                .context("code_qualification_protocol_case_id")
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        context.required_protocol.sha256 == experiment.protocol_expectations_sha256
            && ids == experiment.protocol_case_ids
            && ids.len() == 19
            && ids.iter().collect::<BTreeSet<_>>().len() == 19,
        "code_qualification_protocol_changed"
    );
    oe_evolution::verify_artifact(&experiment.baseline)?;
    oe_evolution::verify_artifact(&source)?;
    oe_evolution::verify_binary(&context.driver)?;
    oe_evolution::verify_binary(&context.evaluator)?;
    Ok((source, assurance, experiment))
}

/// An explicit owned policy context permits actual fixed replay. There is no
/// full-scope independent acceptance evaluator in this profile, so eligibility
/// stays unknown even when the unchanged finite comparison is reproduced.
pub fn qualify(
    root: &SkillRoot,
    input: &CodeArtifactEligibilityRequest,
) -> Result<CodeArtifactEligibilityResult> {
    Ok(execute(root, input, false)?.qualification)
}

/// Explicit read-only index request; the original qualification entry is unchanged.
pub fn associate(
    root: &SkillRoot,
    input: &CodeArtifactEligibilityRequest,
) -> Result<oe_contracts::CodeScopeAssociationResult> {
    execute(root, input, true)
}

fn execute(
    root: &SkillRoot,
    input: &CodeArtifactEligibilityRequest,
    association_requested: bool,
) -> Result<oe_contracts::CodeScopeAssociationResult> {
    // Preserve the old bound-byte and structural guards, including no-context
    // behavior. A missing context does not permit a self-certified JSON result.
    let disconnected = registry::not_connected_code_eligibility(root, input)?;
    let Some((owned, policy_sha256, context_reference)) =
        registry::qualification_context(root, input)?
    else {
        return Ok(oe_contracts::CodeScopeAssociationResult {
            schema: "oev2.code-scope-association-result/1".into(),
            qualification: disconnected,
            index: None,
            execution: "not_connected".into(),
            reason: "No explicit owned qualification context; no association observation".into(),
        });
    };
    let context = &owned.qualification;
    ensure!(
        context.dispatch_budget.timeout_ms > 0
            && context.dispatch_budget.timeout_ms <= 600_000
            && context.dispatch_budget.output_bytes > 0
            && context.dispatch_budget.output_bytes <= 4 * 1024 * 1024,
        "code_qualification_dispatch_budget"
    );
    let (source, assurance, experiment) = inputs(root, input, &context)?;
    let full_scope = scope_request(
        root,
        context,
        owned.full_scope_request.as_ref(),
        &experiment,
    )?;
    let candidate_snapshot = if association_requested {
        ensure!(
            full_scope.is_some(),
            "scope_association_full_scope_context_required"
        );
        Some(association::CandidateSnapshot::capture(&source)?)
    } else {
        None
    };
    let serial = QUALIFICATION_SERIAL.fetch_add(1, Ordering::SeqCst);
    let name = format!(
        "owner-qualification-{}-{serial}-{}",
        std::process::id(),
        &context_reference.sha256[..16]
    );
    let folder = root.new_private_dir(Path::new(&format!("var/code-qualification/{name}")))?;
    let output = format!("var/code-owner-replays/{name}");
    let executable = Path::new(&context.driver.path);
    let mut arguments = vec![
        if full_scope.is_some() {
            "scope-witness"
        } else {
            "run"
        }
        .into(),
        context.driver.source_root.clone().into(),
        root.private_path(Path::new(&context.experiment.path))?
            .into_os_string(),
        context.experiment_sha256.clone().into(),
    ];
    if let Some(reference) = &owned.full_scope_request {
        arguments.push(
            root.private_path(Path::new(&reference.path))?
                .into_os_string(),
        );
    }
    arguments.push(output.clone().into());
    let spec = CommandSpec {
        executable: executable.into(),
        arguments,
        cwd: context.driver.source_root.clone().into(),
        environment: BTreeMap::new(),
        remove_environment: vec![
            "RUSTFLAGS".into(),
            "CARGO_ENCODED_RUSTFLAGS".into(),
            "RUSTC_WRAPPER".into(),
            "RUSTC_WORKSPACE_WRAPPER".into(),
        ],
        timeout: Duration::from_millis(context.dispatch_budget.timeout_ms),
        output_limit: usize::try_from(context.dispatch_budget.output_bytes)?,
    };
    // Executing a separate, frozen driver preserves FixedExperiment's current_exe
    // guard. Calling its library from this version owner would fail that guard.
    let observation = run_command(&spec, &folder.join("fixed-driver"))?;
    let process_completed = observation.status == "completed" && observation.exit_code == Some(0);
    let scope_report: Option<SelectedReport> = if process_completed && full_scope.is_some() {
        let report = SelectedReport::parse(oe_core::json::strict_json(&observation.stdout)?)?;
        scope_result(&context, &experiment, full_scope.as_ref().unwrap(), &report)?;
        write_new(&folder.join("scope-witness-report.json"), &report.value()?)?;
        Some(report)
    } else {
        None
    };
    let scope_reference = scope_report
        .as_ref()
        .map(|_| artifacts::reference(root, &folder.join("scope-witness-report.json")))
        .transpose()?;
    let completed = process_completed
        && scope_report
            .as_ref()
            .is_none_or(|report| report.observation().witness.execution == "completed");
    let actual: Option<CodeComparison> = if let Some(report) = &scope_report {
        report.observation().witness.comparison.clone()
    } else if process_completed {
        let result: CodeComparison =
            serde_json::from_value(oe_core::json::strict_json(&observation.stdout)?)?;
        result
            .validate()
            .map_err(|e| anyhow::anyhow!("code_qualification_result_contract:{e:?}"))?;
        Some(result)
    } else {
        None
    };
    write_new(
        &folder.join("replay-observation.json"),
        &json!({
            "schema":"oev2.code-qualification-replay/1", "policy_sha256":policy_sha256,
            "context":context_reference, "source":input.source, "assurance":input.assurance,
            "experiment_sha256":context.experiment_sha256, "driver":context.driver,
            "evaluator":context.evaluator, "fixed_output":output, "process":observation.report,
            "comparison":actual, "full_scope_request":owned.full_scope_request,
            "scope_witness_report":scope_reference,
            "full_release_eligibility":"unknown"
        }),
    )?;
    // Same guard sequence is used at the old point, and again before the
    // optional new index receipt. No dispatcher or comparison is repeated.
    let recheck_current = || -> Result<()> {
        let fresh = registry::qualification_context(root, input)?
            .context("code_qualification_context_removed")?;
        ensure!(
            fresh.0 == owned && fresh.1 == policy_sha256 && fresh.2 == context_reference,
            "code_qualification_context_or_policy_changed_during_replay"
        );
        ensure!(
            inputs(root, input, &context)?
                == (source.clone(), assurance.clone(), experiment.clone()),
            "code_qualification_inputs_changed_during_replay"
        );
        ensure!(
            scope_request(
                root,
                context,
                owned.full_scope_request.as_ref(),
                &experiment
            )? == full_scope,
            "code_qualification_scope_request_changed_during_replay"
        );
        if let (Some(reference), Some(report), Some(request)) =
            (&scope_reference, &scope_report, &full_scope)
        {
            ensure!(
                artifacts::json(root, reference)? == report.value()?,
                "code_qualification_retained_scope_report_changed"
            );
            scope_result(context, &experiment, request, report)?;
        }
        if let Some(actual) = &actual {
            ensure!(
                *actual == assurance,
                "code_qualification_fixed_comparison_changed"
            );
        }
        Ok(())
    };
    recheck_current()?;
    let mut uncovered: BTreeSet<String> = experiment
        .uncovered
        .clone()
        .into_iter()
        .chain(assurance.uncovered.clone())
        .chain(experiment.candidate.uncovered.clone())
        .collect();
    uncovered.insert(
        "Full 13494 migration identities lack independent behavioral and transfer acceptance"
            .into(),
    );
    uncovered.insert(
        "All 19 frozen protocol requirements remain a separate acceptance denominator".into(),
    );
    uncovered.insert(
        "Whole-code activation/use/Project feedback/recovery remain blocked by full-scope unknown"
            .into(),
    );
    let result = CodeArtifactEligibilityResult {
        schema: "oev2.code-artifact-eligibility-result/1".into(),
        execution: if completed { "completed" } else { "failed" }.into(),
        eligibility: "unknown".into(), artifact_sha256: input.source.sha256.clone(),
        comparison_sha256: input.assurance.sha256.clone(), uncovered: uncovered.into_iter().collect(),
        reason: if completed {
            "Frozen external driver reproduced the bound finite comparison; independent full release criteria are absent"
        } else {
            "Frozen external driver did not complete successfully; partial process evidence is retained and qualification stays unknown"
        }.into(),
    };
    let index = if let Some(snapshot) = &candidate_snapshot {
        snapshot.recheck(&source)?;
        if completed {
            let report = scope_report
                .as_ref()
                .context("scope_association_witness_missing")?;
            let witness_ref = scope_reference
                .as_ref()
                .context("scope_association_witness_ref_missing")?;
            let verified = association::VerifiedScopeObservation::from_owner(
                report.observation(),
                witness_ref,
            )?;
            let index = association::assemble(
                snapshot,
                input,
                &source,
                &policy_sha256,
                &context_reference,
                &result,
                verified,
            )?;
            let value = wrap_association_index(index, report)?;
            let bytes = serde_json::to_vec_pretty(&value)?;
            ensure!(
                bytes.len() < association::INDEX_BYTES,
                "scope_association_index_byte_budget"
            );
            // This is an observation artifact until the final receipt below.
            // Never accept a caller-pasted index or overwrite an earlier result.
            let path = folder.join("scope-association-index.json");
            write_new(&path, &value)?;
            let reference = artifacts::reference(root, &path)?;
            snapshot.recheck(&source)?;
            recheck_current()?;
            ensure!(
                artifacts::json(root, &reference)? == value,
                "scope_association_retained_index_changed"
            );
            Some(reference)
        } else {
            None
        }
    } else {
        None
    };
    write_new(
        &folder.join("qualification.json"),
        &serde_json::to_value(&result)?,
    )?;
    let receipt = oe_contracts::CodeScopeAssociationResult {
        schema: "oev2.code-scope-association-result/1".into(),
        execution: result.execution.clone(),
        reason: if index.is_some() {
            "Fresh fixed cases associated with the selected candidate; full eligibility remains unknown"
        } else {
            "No accepted full-scope association index was produced"
        }.into(),
        qualification: result,
        index,
    };
    if let Some(snapshot) = &candidate_snapshot {
        // The private finalizer owns only this existing publication boundary.
        // Keep the real candidate snapshot followed by current-input checks in
        // exactly their original order; no caller-supplied guard is exposed.
        finish_association_receipt(root, &folder, &receipt, scope_report.as_ref(), || {
            snapshot.recheck(&source)?;
            recheck_current()
        })?;
    }
    Ok(receipt)
}

fn wrap_association_index(
    index: oe_contracts::CodeScopeAssociationIndex,
    report: &SelectedReport,
) -> Result<serde_json::Value> {
    Ok(match report {
        SelectedReport::Legacy(_) => serde_json::to_value(&index)?,
        SelectedReport::Q40(report) => {
            let wrapper = oe_contracts::CodeScopeAssociationIndex2 {
                schema: "oev2.code-scope-association-index/2".into(),
                inventory: index,
                catalog_sha256: report.catalog_sha256.clone(),
                relations: report.relations.clone(),
            };
            wrapper
                .validate()
                .map_err(|e| anyhow::anyhow!("q40_association_contract:{e:?}"))?;
            serde_json::to_value(wrapper)?
        }
    })
}

fn validate_retained_association_index(
    root: &SkillRoot,
    reference: &oe_contracts::ArtifactReference,
    report: &SelectedReport,
) -> Result<()> {
    let value = artifacts::json(root, reference)?;
    match report {
        SelectedReport::Legacy(_) => {
            let retained: oe_contracts::CodeScopeAssociationIndex = serde_json::from_value(value)?;
            retained.validate().map_err(|errors| {
                anyhow::anyhow!("scope_association_retained_contract:{errors:?}")
            })?;
        }
        SelectedReport::Q40(report) => {
            let retained: oe_contracts::CodeScopeAssociationIndex2 = serde_json::from_value(value)?;
            retained.validate().map_err(|errors| {
                anyhow::anyhow!("q40_association_retained_contract:{errors:?}")
            })?;
            ensure!(
                retained.catalog_sha256 == report.catalog_sha256
                    && retained.relations == report.relations,
                "q40_association_retained_relations_changed"
            );
        }
    }
    Ok(())
}

/// Private terminal publication used by the existing execute path only. This
/// does not dispatch, accept a witness, qualify a source or choose a version.
/// Its guard is the closure above with real snapshot/current-input owners.
/// Tests exercise actual private-file rereads and failures at this boundary;
/// only separate fresh process tests establish the whole owner execution.
fn finish_association_receipt(
    root: &SkillRoot,
    folder: &Path,
    receipt: &oe_contracts::CodeScopeAssociationResult,
    report: Option<&SelectedReport>,
    recheck: impl FnOnce() -> Result<()>,
) -> Result<()> {
    recheck()?;
    if let Some(reference) = &receipt.index {
        validate_retained_association_index(
            root,
            reference,
            report.context("scope_association_retained_report_missing")?,
        )?;
    }
    receipt
        .validate()
        .map_err(|errors| anyhow::anyhow!("scope_association_receipt_contract:{errors:?}"))?;
    write_new(
        &folder.join("scope-association-receipt.json"),
        &serde_json::to_value(receipt)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod q40_publication_tests;
