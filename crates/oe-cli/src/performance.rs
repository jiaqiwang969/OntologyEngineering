//! Source-bound performance handoff; never executes or replays a workload.
use crate::projection_evidence::{archive, retain};
use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_contracts::{
    ArtifactReference, PerformanceNativeObservation1, PerformanceProjection1,
    PerformanceProjectionFile1, PerformanceProjectionReceipt1, PerformanceProjectionRequest1,
};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const MAX_INPUT: usize = 16 * 1024 * 1024;

#[derive(Subcommand)]
pub enum PerformanceCommand {
    /// Archive an existing SmokingGun/Flameox observation and its evidence.
    Project {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn input(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.data_expand(path)?;
    root.portable_data_path(&path)?;
    no_symlinks(&path)?;
    Ok(path)
}

fn validate_source(source: &PerformanceNativeObservation1) -> Result<()> {
    ensure!(
        source.validate().is_ok() && source.project_context.validate().is_ok(),
        "performance_source_contract_invalid"
    );
    ensure!(
        source.cleanup.validate().is_ok(),
        "performance_cleanup_contract_invalid"
    );
    let mut keys = BTreeSet::new();
    for measurement in &source.measurements {
        ensure!(
            measurement.validate().is_ok(),
            "performance_measurement_contract_invalid"
        );
        ensure!(
            keys.insert((
                &measurement.phase,
                &measurement.name,
                &measurement.source_pointer
            )),
            "performance_duplicate_measurement"
        );
    }
    let mut ids = BTreeSet::new();
    for candidate in &source.candidates {
        ensure!(
            candidate.validate().is_ok(),
            "performance_candidate_contract_invalid"
        );
        ensure!(
            ids.insert(&candidate.finding_id),
            "performance_duplicate_candidate"
        );
        ensure!(
            !candidate.basis.is_empty(),
            "performance_candidate_basis_required"
        );
    }
    let compatible = match source.module.as_str() {
        "smokinggun" => matches!(
            source.operation.as_str(),
            "complexity_candidates" | "measurement" | "analysis"
        ),
        "flameox" => matches!(
            source.operation.as_str(),
            "measurement" | "capture" | "analysis" | "replay"
        ),
        _ => false,
    };
    ensure!(compatible, "performance_module_operation_mismatch");
    Ok(())
}

fn assessment(source: &PerformanceNativeObservation1) -> (&'static str, Vec<String>) {
    let mut issues = Vec::new();
    if source.artifacts.is_empty() {
        issues.push("raw_producer_artifacts_missing".into());
    }
    if source.covered.is_empty() {
        issues.push("coverage_undeclared".into());
    }
    if source.operation == "complexity_candidates" && source.candidates.is_empty() {
        // An empty scan can be meaningful only in the raw report; it cannot supply a candidate.
        issues.push("no_complexity_candidate_for_requested_projection".into());
    }
    if matches!(
        source.operation.as_str(),
        "measurement" | "capture" | "replay"
    ) && source.measurements.is_empty()
    {
        issues.push("measurements_missing".into());
    }
    for measurement in &source.measurements {
        if measurement.status != "observed" || measurement.evidence.is_empty() {
            issues.push(format!(
                "measurement_incomplete:{}:{}",
                measurement.phase, measurement.name
            ));
        }
    }
    if source.candidates.iter().any(|v| v.kind == "unknown") {
        issues.push("unknown_candidates".into());
    }
    let owns_workload = matches!(source.operation.as_str(), "capture" | "replay");
    if owns_workload && source.cleanup.status != "completed" {
        issues.push("owned_workload_cleanup_unconfirmed".into());
    }
    if source.cleanup.status == "completed" && source.cleanup.evidence.is_empty() {
        issues.push("cleanup_evidence_missing".into());
    }
    if matches!(source.cleanup.status.as_str(), "failed" | "unknown") {
        issues.push("cleanup_unresolved".into());
    }
    (
        if issues.is_empty() {
            "projected"
        } else {
            "blocked"
        },
        issues,
    )
}

pub fn run(root: &SkillRoot, command: PerformanceCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "performance_projection")?;
    let PerformanceCommand::Project {
        request,
        source,
        output,
    } = command;
    let request_bytes = read_regular_bytes(&input(root, &request)?, MAX_INPUT)?;
    let request: PerformanceProjectionRequest1 =
        serde_json::from_value(oe_core::json::strict_json(&request_bytes)?)?;
    ensure!(
        request.validate().is_ok() && request.project_context.validate().is_ok(),
        "performance_request_contract_invalid"
    );
    let source_path = input(root, &source)?;
    let source_bytes = read_regular_bytes(&source_path, MAX_INPUT)?;
    ensure!(
        request.source.path == root.portable_data_path(&source_path)?
            && request.source.sha256 == sha256(&source_bytes),
        "performance_source_reference_mismatch"
    );
    let source: PerformanceNativeObservation1 =
        serde_json::from_value(oe_core::json::strict_json(&source_bytes)?)?;
    validate_source(&source)?;
    ensure!(
        request.project_context == source.project_context,
        "performance_project_context_mismatch"
    );
    ensure!(
        request.module == source.module && request.operation == source.operation,
        "performance_request_observation_mismatch"
    );
    let mut references = source.artifacts.clone();
    references.extend(source.cleanup.evidence.clone());
    for measurement in &source.measurements {
        references.extend(measurement.evidence.clone());
    }
    let originals = retain(root, &references)?;
    let (status, issues) = assessment(&source);
    let output_dir = root.new_private_dir(&output)?;
    write_new_bytes(&output_dir.join("request.json"), &request_bytes)?;
    write_new_bytes(&output_dir.join("source-observation.json"), &source_bytes)?;
    let copied = archive(root, &output_dir, &originals)?;
    let mut files = Vec::new();
    for (index, item) in copied.values().enumerate() {
        files.push(PerformanceProjectionFile1 {
            role: format!("evidence:{index}"),
            path: item.path.clone(),
            sha256: item.sha256.clone(),
        });
    }
    let rewrite = |refs: &[ArtifactReference]| -> Vec<ArtifactReference> {
        refs.iter().map(|v| copied[&v.path].clone()).collect()
    };
    let mut measurements = source.measurements.clone();
    for measurement in &mut measurements {
        measurement.evidence = rewrite(&measurement.evidence);
    }
    let mut cleanup = source.cleanup.clone();
    cleanup.evidence = rewrite(&cleanup.evidence);
    let source_observation = ArtifactReference {
        path: root.portable_data_path(&output_dir.join("source-observation.json"))?,
        sha256: sha256(&source_bytes),
    };
    let mut limitations = source.limitations.clone();
    limitations.push("Project context is a supplied snapshot; current Project and tool identity are rechecked at ToolResult registration".into());
    limitations.push("Producer measurements and cleanup claims are preserved; this projection does not run, recompute, compare or independently confirm them".into());
    let projection = PerformanceProjection1 {
        schema: "oev2.performance-projection/1".into(),
        request,
        module: source.module.clone(),
        operation: source.operation.clone(),
        status: status.into(),
        source_sha256: sha256(&source_bytes),
        source_observation,
        workload: source.workload,
        environment: source.environment,
        budget: source.budget,
        measurements,
        artifacts: rewrite(&source.artifacts),
        candidates: source.candidates,
        cleanup,
        covered: source.covered,
        not_covered: source.not_covered,
        limitations,
        issues: issues.clone(),
        engineering_adoption: "not_assessed".into(),
    };
    ensure!(
        projection.validate().is_ok(),
        "performance_projection_contract_invalid"
    );
    write_new(
        &output_dir.join("projection.json"),
        &serde_json::to_value(&projection)?,
    )?;
    let projection_bytes = read_regular_bytes(&output_dir.join("projection.json"), MAX_INPUT)?;
    for (role, name, bytes) in [
        ("request", "request.json", &request_bytes),
        (
            "source_observation",
            "source-observation.json",
            &source_bytes,
        ),
        ("projection", "projection.json", &projection_bytes),
    ] {
        files.push(PerformanceProjectionFile1 {
            role: role.into(),
            path: root.portable_data_path(&output_dir.join(name))?,
            sha256: sha256(bytes),
        });
    }
    let receipt = PerformanceProjectionReceipt1 {
        schema: "oev2.performance-projection-receipt/1".into(),
        request_sha256: sha256(&request_bytes),
        source_sha256: sha256(&source_bytes),
        projection_sha256: sha256(&projection_bytes),
        output_dir: root.portable_data_path(&output_dir)?,
        files,
        status: status.into(),
        issues,
        warnings: vec![
            "No optimization benefit, causality or engineering adoption established".into(),
        ],
        execution_binding: execution,
        engineering_adoption: "not_assessed".into(),
    };
    write_new(
        &output_dir.join("receipt.json"),
        &serde_json::to_value(&receipt)?,
    )?;
    Ok((
        json!({"status":status,"module":source.module,"operation":source.operation,"directory":output_dir,"projection":"projection.json","receipt":"receipt.json","engineering_adoption":"not_assessed"}),
        if status == "projected" { 0 } else { 1 },
    ))
}
