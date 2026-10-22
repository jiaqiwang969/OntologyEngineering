//! Fixed external comparator. Links contracts and bounded IO, never a semantic
//! runtime, kernel, driver implementation, candidate algorithm or Project.
use anyhow::{Context, Result, ensure};
mod semantic_outcome;
mod witness;
use oe_contracts::{
    CodeComparison, CodeComparisonRequest, CodeExperiment, CodeReplayCase, CodeReplayInvocation,
    CodeReplayObservation,
};
use oe_core::{read_regular_bytes, sha256};
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
fn digest(value: &Value) -> Result<String> {
    Ok(sha256(canonical_json(value)?))
}
fn invocation(case: &CodeReplayCase, run_id: u64) -> CodeReplayInvocation {
    CodeReplayInvocation {
        schema: "oev2.code-replay-invocation/1".into(),
        case_id: case.case_id.clone(),
        capability: case.capability.clone(),
        profile: case.profile.clone(),
        input: case.input.clone(),
        input_sha256: case.input_sha256.clone(),
        conditions_sha256: case.conditions_sha256.clone(),
        budget: case.budget.clone(),
        seed: case.seed,
        run_id,
    }
}
fn rows(value: &Value) -> Result<Vec<Vec<u8>>> {
    let object = value.as_object().context("evaluator_select_object")?;
    ensure!(
        object.len() == 4
            && ["kind", "columns", "rows", "row_count"]
                .iter()
                .all(|k| object.contains_key(*k))
            && value["kind"] == "select",
        "evaluator_select_schema"
    );
    let columns = value["columns"]
        .as_array()
        .context("evaluator_select_columns")?;
    let names = columns
        .iter()
        .map(|c| c.as_str().context("evaluator_select_column_text"))
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(names.len() == columns.len(), "evaluator_duplicate_column");
    let values = value["rows"].as_array().context("evaluator_select_rows")?;
    ensure!(
        value["row_count"].as_u64() == Some(values.len() as u64),
        "evaluator_select_row_count"
    );
    let mut bag = Vec::new();
    for row in values {
        let row = row.as_object().context("evaluator_row_object")?;
        ensure!(
            row.keys().all(|k| names.contains(k.as_str())),
            "evaluator_row_unprojected_binding"
        );
        for term in row.values() {
            let object = term.as_object().context("evaluator_term_object")?;
            match term["kind"].as_str() {
                Some("iri" | "blank") => ensure!(
                    object.len() == 2 && term["value"].is_string(),
                    "evaluator_term_schema"
                ),
                Some("literal") => ensure!(
                    object.len() == 4
                        && term["value"].is_string()
                        && term["datatype"].is_string()
                        && (term["language"].is_null() || term["language"].is_string()),
                    "evaluator_literal_schema"
                ),
                _ => anyhow::bail!("evaluator_unknown_term_kind"),
            }
        }
        bag.push(canonical_json(&serde_json::to_value(row)?)?);
    }
    bag.sort();
    Ok(bag)
}
fn equivalent(case: &CodeReplayCase, value: &Value) -> Result<bool> {
    match case.comparison.as_str() {
        "semantic_outcome/1" => semantic_outcome::equivalent(case, value),
        "exact_json" => Ok(canonical_json(value)? == canonical_json(&case.expected)?),
        "select_bag" => {
            Ok(value["columns"] == case.expected["columns"]
                && rows(value)? == rows(&case.expected)?)
        }
        _ => anyhow::bail!("evaluator_comparison_unsupported"),
    }
}
fn assess(
    case: &CodeReplayCase,
    run: u64,
    side: &str,
    observation: Option<&CodeReplayObservation>,
    request: &CodeComparisonRequest,
) -> Result<Value> {
    let Some(o) = observation else {
        return Ok(
            json!({"status":"unknown","execution":"not_run","expected_match":false,"cost":"unknown"}),
        );
    };
    o.validate()
        .map_err(|e| anyhow::anyhow!("evaluator_observation_schema:{e:?}"))?;
    let artifact = if side == "baseline" {
        &request.experiment.baseline
    } else {
        &request.experiment.candidate
    };
    ensure!(
        o.runner_sha256 == artifact.runner.sha256
            && o.artifact_manifest_sha256 == artifact.manifest_sha256
            && o.invocation_sha256 == digest(&serde_json::to_value(invocation(case, run))?)?,
        "evaluator_observation_identity_changed"
    );
    ensure!(
        o.stdout_bytes
            .is_none_or(|bytes| bytes <= case.budget.output_bytes)
            && o.stderr_bytes
                .is_none_or(|bytes| bytes <= case.budget.output_bytes),
        "evaluator_output_budget_changed"
    );
    if o.execution != "completed" {
        return Ok(
            json!({"status":if o.execution=="failed"{"failed"}else{"unknown"},"execution":o.execution,"expected_match":false,"cost":"unknown"}),
        );
    }
    ensure!(
        o.elapsed_micros.is_some() && o.stdout_bytes.is_some() && o.stderr_bytes.is_some(),
        "evaluator_completed_without_resource_observation"
    );
    let Some(inner) = &o.observed else {
        anyhow::bail!("evaluator_completed_without_observation")
    };
    inner
        .validate()
        .map_err(|e| anyhow::anyhow!("evaluator_runner_schema:{e:?}"))?;
    ensure!(
        inner.case_id == case.case_id
            && inner.input_sha256 == case.input_sha256
            && inner.conditions_sha256 == case.conditions_sha256
            && inner.compiled_manifest_sha256 == artifact.manifest_sha256,
        "evaluator_runner_identity_changed"
    );
    let necessary_checks = case
        .checks
        .iter()
        .all(|check| inner.coverage.contains(check));
    let matched = inner.execution == case.expected_execution
        && equivalent(case, &inner.result)?
        && necessary_checks
        && inner.error.is_none();
    Ok(
        json!({"status":if matched{"matched"}else{"failed"},"execution":inner.execution,"expected_match":matched,
        "necessary_checks_present":necessary_checks,"safe_explicit_rejection":inner.execution=="unsupported",
        "cost":"unknown","engineering_adoption":"unknown"}),
    )
}
fn require_fixed_evaluator(experiment: &CodeExperiment) -> Result<()> {
    let source = strict_json(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/code-source.json"
    )))?;
    ensure!(
        source["manifest_sha256"] == experiment.evaluator.source_manifest_sha256
            && oe_core::fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)?.sha256
                == experiment.evaluator.sha256,
        "evaluator_fixed_identity_changed"
    );
    for (path, id) in source["manifest"]["files"]
        .as_object()
        .context("evaluator_compiled_files")?
    {
        let observed = oe_core::fingerprint_file(
            &Path::new(&experiment.evaluator.source_root).join(path),
            oe_core::source_limits::public_file_bytes(Path::new(path)),
        )?;
        ensure!(
            observed.sha256 == id["sha256"] && observed.bytes == id["bytes"],
            "evaluator_compiled_source_changed"
        );
    }
    Ok(())
}
fn compare(request: CodeComparisonRequest) -> Result<CodeComparison> {
    request
        .experiment
        .validate()
        .map_err(|e| anyhow::anyhow!("evaluator_experiment_schema:{e:?}"))?;
    require_fixed_evaluator(&request.experiment)?;
    let oracle = read_regular_bytes(Path::new(&request.experiment.oracle_path), 4 * 1024 * 1024)?;
    ensure!(
        sha256(&oracle) == request.experiment.oracle_sha256
            && strict_json(&oracle)? == serde_json::to_value(&request.experiment.cases)?,
        "evaluator_frozen_oracle_changed"
    );
    ensure!(
        digest(&serde_json::to_value(&request.experiment.cases)?)?
            == request.experiment.case_set_sha256,
        "evaluator_case_set_changed"
    );
    let mut cases = BTreeSet::new();
    let mut slots_seen = BTreeSet::new();
    let mut denominator = 0u64;
    for c in &request.experiment.cases {
        c.validate()
            .map_err(|e| anyhow::anyhow!("evaluator_case_schema:{e:?}"))?;
        ensure!(
            cases.insert(c.case_id.as_str())
                && !c.checks.is_empty()
                && (1..=16).contains(&c.repeats)
                && (1..=60_000).contains(&c.budget.timeout_ms)
                && (1024..=16 * 1024 * 1024).contains(&c.budget.output_bytes)
                && c.conditions_sha256 == request.experiment.conditions_sha256
                && digest(&c.input)? == c.input_sha256,
            "evaluator_case_identity_or_budget"
        );
        denominator = denominator
            .checked_add(c.repeats)
            .context("evaluator_denominator_overflow")?;
    }
    ensure!(
        denominator > 0 && request.experiment.cases.len() <= 128,
        "evaluator_empty_or_excessive_cases"
    );
    for o in &request.observations {
        let c = request
            .experiment
            .cases
            .iter()
            .find(|c| c.case_id == o.case_id)
            .context("evaluator_extra_case")?;
        ensure!(
            ["baseline", "candidate"].contains(&o.side.as_str())
                && o.run_id < c.repeats
                && slots_seen.insert((o.side.clone(), o.case_id.clone(), o.run_id)),
            "evaluator_duplicate_or_extra_slot"
        );
    }
    let mut slots = Vec::new();
    let mut all_candidate = true;
    let mut repaired = false;
    for c in &request.experiment.cases {
        for run in 0..c.repeats {
            let lookup = |side: &str| {
                request
                    .observations
                    .iter()
                    .find(|o| o.side == side && o.case_id == c.case_id && o.run_id == run)
            };
            let baseline = assess(c, run, "baseline", lookup("baseline"), &request)?;
            let candidate = assess(c, run, "candidate", lookup("candidate"), &request)?;
            let good = candidate["status"] == "matched";
            all_candidate &= good;
            repaired |= good && baseline["status"] == "failed";
            slots.push(json!({"case_id":c.case_id,"run_id":run,"eligibility":c.eligibility,"baseline":baseline,"candidate":candidate,
                "required_checks":c.checks,"case_in_fixed_denominator":true}));
        }
    }
    let adapter_profile = request.experiment.cases.iter().all(|case| {
        case.profile == "local-semantic-replay/2" && case.capability == "semantic.reason-metadata/1"
    });
    Ok(CodeComparison{schema:"oev2.code-comparison/1".into(),experiment_id:request.experiment.experiment_id,
        driver_sha256:request.experiment.driver.sha256,evaluator_sha256:request.experiment.evaluator.sha256,
        oracle_sha256:request.experiment.oracle_sha256,baseline_sha256:request.experiment.baseline.manifest_sha256,candidate_sha256:request.experiment.candidate.manifest_sha256,
        fixed_denominator:denominator,slots,assessment:if all_candidate && repaired{if adapter_profile{"local_adapter_coverage_gain"}else{"local_deterministic_repair"}}else if all_candidate{"no_justified_gain"}else{"inconclusive"}.into(),
        engineering_adoption:"unknown".into(),cost:"unknown".into(),version_selection:"not_connected".into(),
        uncovered:vec![if adapter_profile {"Structured reason replay observes development adapter coverage, not improved reasoning, whole-library qualification or industrial transfer"} else {"Finite development profile, no independent engineering/industrial transfer"}.into(),
            "Graph isomorphism, units, provider stochastic noise and total cost are not implemented in this comparator".into(),
            "Selection/use/feedback/recovery owner adapter not connected".into()]})
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 2 && args[1] == "identity" {
        println!(
            "{}",
            std::str::from_utf8(include_bytes!(concat!(
                env!("OUT_DIR"),
                "/code-source.json"
            )))?
        );
        return Ok(());
    }
    if args.len() == 3 && args[1] == "witness" {
        let request: oe_contracts::CodeScopeWitnessDispatch = serde_json::from_value(strict_json(
            &read_regular_bytes(Path::new(&args[2]), 16 * 1024 * 1024)?,
        )?)?;
        println!(
            "{}",
            String::from_utf8(canonical_json(&serde_json::to_value(witness::run(
                request
            )?)?)?)?
        );
        return Ok(());
    }
    ensure!(
        args.len() == 3 && args[1] == "compare",
        "evaluator_arguments"
    );
    let request: CodeComparisonRequest = serde_json::from_value(strict_json(
        &read_regular_bytes(Path::new(&args[2]), 16 * 1024 * 1024)?,
    )?)?;
    println!(
        "{}",
        String::from_utf8(canonical_json(&serde_json::to_value(compare(request)?)?)?)?
    );
    Ok(())
}
#[cfg(test)]
mod independent_expectations {
    use super::*;
    fn select(rows: Vec<Value>) -> Value {
        json!({"kind":"select","columns":["x"],"row_count":rows.len(),"rows":rows})
    }
    #[test]
    fn distinct_row_order_is_ignored_but_duplicate_multiplicity_is_preserved() {
        let a = json!({"x":{"kind":"iri","value":"urn:a"}});
        let b = json!({"x":{"kind":"iri","value":"urn:b"}});
        let original = select(vec![a.clone(), a.clone(), b.clone()]);
        let reordered = select(vec![b.clone(), a.clone(), a.clone()]);
        let reduced = select(vec![a, b]);
        assert_ne!(
            canonical_json(&original).unwrap(),
            canonical_json(&reordered).unwrap()
        );
        assert_eq!(rows(&original).unwrap(), rows(&reordered).unwrap());
        assert_ne!(rows(&original).unwrap(), rows(&reduced).unwrap());
    }
    #[test]
    fn unbound_is_distinct_from_empty_string_and_null() {
        let unbound = select(vec![json!({})]);
        let empty = select(vec![
            json!({"x":{"kind":"literal","value":"","datatype":"http://www.w3.org/2001/XMLSchema#string","language":null}}),
        ]);
        let null = select(vec![json!({"x":null})]);
        assert_ne!(rows(&unbound).unwrap(), rows(&empty).unwrap());
        assert!(rows(&null).is_err());
    }
}

#[cfg(test)]
#[path = "../tests/support/q40_outcome.rs"]
mod q40_outcome;
