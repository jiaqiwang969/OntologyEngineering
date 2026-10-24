//! Source/artifact projection for REA, LeanToken and GitContribute.
//!
//! The external module remains the producer of source analysis. This boundary
//! only binds its exact bytes to the current Project, validates the typed
//! artifact/edge envelope, copies the declared evidence into the private store,
//! and emits a reviewable projection. It never runs a reverse-engineering,
//! search, or contribution service and never turns static evidence into a
//! runtime claim.
use crate::projection_evidence::{archive, retain};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use oe_contracts::{
    ArtifactReference, SourceNativeObservation1, SourceProjection1, SourceProjectionFile1,
    SourceProjectionReceipt1, SourceProjectionRequest1,
};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256, write_new, write_new_bytes};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const INPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Subcommand)]
pub enum SourceProjectionCommand {
    /// Project one canonical source/artifact observation into the current Project.
    Project {
        /// Strict source-projection-request/1 JSON.
        #[arg(long)]
        request: PathBuf,
        /// Strict source-native-observation/1 JSON.
        #[arg(long)]
        source: PathBuf,
        /// New private output directory beneath the selected installation/project store.
        #[arg(long)]
        output: PathBuf,
    },
}

fn private_input(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.data_expand(path)?;
    ensure!(
        path.starts_with(root.data_path()),
        "source_projection_private_input_required"
    );
    no_symlinks(&path)?;
    Ok(path)
}

fn decode_value(root: &SkillRoot, path: &Path) -> Result<(Vec<u8>, Value)> {
    let path = private_input(root, path)?;
    let bytes = read_regular_bytes(&path, INPUT_BYTES)?;
    let value = oe_core::json::strict_json(&bytes)?;
    Ok((bytes, value))
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .context(format!("{label}_object_required"))
}

fn exact_keys(value: &Value, label: &str, allowed: &[&str]) -> Result<()> {
    let actual = object(value, label)?;
    let allowed: BTreeSet<&str> = allowed.iter().copied().collect();
    let unknown: Vec<_> = actual
        .keys()
        .filter(|k| !allowed.contains(k.as_str()))
        .cloned()
        .collect();
    ensure!(unknown.is_empty(), "{label}_unknown_fields:{unknown:?}");
    for key in allowed {
        ensure!(actual.contains_key(key), "{label}_field_missing:{key}");
    }
    Ok(())
}

fn check_artifact_shape(value: &Value, label: &str) -> Result<()> {
    exact_keys(
        value,
        label,
        &[
            "artifact_id",
            "kind",
            "source",
            "status",
            "source_pointer",
            "notes",
        ],
    )?;
    let source = value
        .get("source")
        .context("source_projection_artifact_source_missing")?;
    exact_keys(source, &format!("{label}_source"), &["path", "sha256"])?;
    Ok(())
}

fn check_edge_shape(value: &Value, label: &str) -> Result<()> {
    exact_keys(
        value,
        label,
        &[
            "edge_id",
            "kind",
            "from_artifact",
            "to_artifact",
            "status",
            "basis",
        ],
    )
}

fn check_finding_shape(value: &Value, label: &str) -> Result<()> {
    exact_keys(value, label, &["finding_id", "statement", "kind", "basis"])
}

fn check_request_shape(value: &Value) -> Result<()> {
    exact_keys(
        value,
        "source_projection_request",
        &[
            "schema",
            "project_context",
            "module",
            "operation",
            "subject_kind",
            "subject_identity",
            "source",
            "policy",
            "acceptance",
        ],
    )?;
    exact_keys(
        value
            .get("source")
            .context("source_projection_request_source_missing")?,
        "source_projection_request_source",
        &["path", "sha256"],
    )
}

fn check_observation_shape(value: &Value) -> Result<()> {
    exact_keys(
        value,
        "source_native_observation",
        &[
            "schema",
            "project_context",
            "module",
            "operation",
            "subject_kind",
            "subject_identity",
            "artifacts",
            "edges",
            "findings",
            "covered",
            "not_covered",
            "limitations",
            "engineering_adoption",
        ],
    )?;
    let artifacts = value
        .get("artifacts")
        .and_then(Value::as_array)
        .context("source_native_observation_artifacts_array_required")?;
    for (i, item) in artifacts.iter().enumerate() {
        check_artifact_shape(item, &format!("source_native_observation_artifacts_{i}"))?;
    }
    let edges = value
        .get("edges")
        .and_then(Value::as_array)
        .context("source_native_observation_edges_array_required")?;
    for (i, item) in edges.iter().enumerate() {
        check_edge_shape(item, &format!("source_native_observation_edges_{i}"))?;
    }
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .context("source_native_observation_findings_array_required")?;
    for (i, item) in findings.iter().enumerate() {
        check_finding_shape(item, &format!("source_native_observation_findings_{i}"))?;
    }
    Ok(())
}

fn artifact_matches(
    root: &SkillRoot,
    reference: &ArtifactReference,
    path: &Path,
    bytes: &[u8],
) -> Result<()> {
    let relative = root.portable_data_path(path)?;
    ensure!(
        relative == reference.path,
        "source_projection_source_path_mismatch"
    );
    ensure!(
        sha256(bytes) == reference.sha256,
        "source_projection_source_digest_mismatch"
    );
    Ok(())
}

fn check_edges(source: &SourceNativeObservation1) -> Result<()> {
    let ids: BTreeSet<_> = source
        .artifacts
        .iter()
        .map(|a| a.artifact_id.as_str())
        .collect();
    let mut edges = BTreeSet::new();
    for edge in &source.edges {
        ensure!(
            edge.validate().is_ok(),
            "source_projection_edge_contract_invalid"
        );
        ensure!(
            edges.insert(edge.edge_id.as_str()),
            "source_projection_edge_id_duplicate"
        );
        ensure!(
            ids.contains(edge.from_artifact.as_str()),
            "source_projection_edge_from_missing"
        );
        ensure!(
            ids.contains(edge.to_artifact.as_str()),
            "source_projection_edge_to_missing"
        );
        ensure!(!edge.basis.is_empty(), "source_projection_edge_basis_empty");
    }
    let mut finding_ids = BTreeSet::new();
    for finding in &source.findings {
        ensure!(
            finding.validate().is_ok(),
            "source_projection_finding_contract_invalid"
        );
        ensure!(
            finding_ids.insert(&finding.finding_id),
            "source_projection_duplicate_finding"
        );
        ensure!(
            !finding.basis.is_empty(),
            "source_projection_finding_basis_empty"
        );
    }
    Ok(())
}

pub fn run(root: &SkillRoot, command: SourceProjectionCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "source_projection")?;
    match command {
        SourceProjectionCommand::Project {
            request,
            source,
            output,
        } => {
            let (request_bytes, request_raw) = decode_value(root, &request)?;
            check_request_shape(&request_raw)?;
            let request_value: SourceProjectionRequest1 = serde_json::from_value(request_raw)?;
            ensure!(
                request_value.validate().is_ok(),
                "source_projection_request_contract_invalid"
            );
            let (source_bytes, source_raw) = decode_value(root, &source)?;
            check_observation_shape(&source_raw)?;
            let source_value: SourceNativeObservation1 = serde_json::from_value(source_raw)?;
            ensure!(
                source_value.validate().is_ok(),
                "source_projection_source_contract_invalid"
            );
            ensure!(
                request_value.policy == "exact_source_no_inference",
                "source_projection_policy_invalid"
            );
            ensure!(
                request_value.project_context == source_value.project_context,
                "source_projection_project_context_mismatch"
            );
            ensure!(
                request_value.module == source_value.module,
                "source_projection_module_mismatch"
            );
            ensure!(
                request_value.operation == source_value.operation,
                "source_projection_operation_mismatch"
            );
            ensure!(
                request_value.subject_kind == source_value.subject_kind,
                "source_projection_subject_kind_mismatch"
            );
            ensure!(
                request_value.subject_identity == source_value.subject_identity,
                "source_projection_subject_identity_mismatch"
            );
            artifact_matches(
                root,
                &request_value.source,
                &private_input(root, &source)?,
                &source_bytes,
            )?;
            ensure!(
                request_value.project_context.validate().is_ok(),
                "source_projection_context_contract_invalid"
            );
            ensure!(
                matches!(
                    (
                        source_value.module.as_str(),
                        source_value.operation.as_str()
                    ),
                    ("reverse_engineer_anything", "artifact_analysis")
                        | ("leantoken", "source_search")
                        | ("gitcontribute", "contribution_research")
                ),
                "source_projection_module_operation_invalid"
            );
            let mut ids = BTreeSet::new();
            for artifact in &source_value.artifacts {
                ensure!(
                    artifact.validate().is_ok() && artifact.source.validate().is_ok(),
                    "source_projection_artifact_contract_invalid"
                );
                ensure!(
                    ids.insert(&artifact.artifact_id),
                    "source_projection_artifact_id_duplicate"
                );
            }
            check_edges(&source_value)?;
            let refs: Vec<_> = source_value
                .artifacts
                .iter()
                .filter(|a| a.status != "missing")
                .map(|a| a.source.clone())
                .collect();
            let originals = retain(root, &refs)?;

            let output_dir = root.new_private_dir(&output)?;
            write_new_bytes(&output_dir.join("request.json"), &request_bytes)?;
            write_new_bytes(&output_dir.join("source-observation.json"), &source_bytes)?;
            let copied = archive(root, &output_dir, &originals)?;
            let mut artifacts = source_value.artifacts.clone();
            for artifact in &mut artifacts {
                if artifact.status != "missing" {
                    artifact.source = copied[&artifact.source.path].clone();
                }
            }
            let mut limitations = source_value.limitations.clone();
            limitations.push("Supplied Project snapshot only; current Project and producer identity require ToolResult verification".into());
            let mut issues = Vec::new();
            if source_value
                .artifacts
                .iter()
                .any(|a| a.status != "available")
            {
                issues.push("incomplete_artifacts".into());
            }
            if source_value.edges.iter().any(|e| e.status == "unknown")
                || source_value.findings.iter().any(|f| f.kind == "unknown")
            {
                issues.push("unknown_findings_or_edges".into());
            }
            if source_value.covered.is_empty() {
                issues.push("coverage_undeclared".into());
            }
            let status = if issues.is_empty() {
                "projected"
            } else {
                "blocked"
            };
            let source_observation = ArtifactReference {
                path: root.portable_data_path(&output_dir.join("source-observation.json"))?,
                sha256: sha256(&source_bytes),
            };
            let projection = SourceProjection1 {
                schema: "oev2.source-projection/1".into(),
                request: request_value.clone(),
                module: source_value.module.clone(),
                operation: source_value.operation.clone(),
                subject_identity: source_value.subject_identity.clone(),
                status: status.into(),
                source_sha256: sha256(&source_bytes),
                source_observation: source_observation.clone(),
                artifacts,
                edges: source_value.edges.clone(),
                findings: source_value.findings.clone(),
                covered: source_value.covered.clone(),
                not_covered: source_value.not_covered.clone(),
                limitations,
                issues: issues.clone(),
                engineering_adoption: "not_assessed".into(),
            };
            let projection_value = serde_json::to_value(&projection)?;
            write_new(&output_dir.join("projection.json"), &projection_value)?;
            let projection_bytes =
                read_regular_bytes(&output_dir.join("projection.json"), INPUT_BYTES)?;
            let mut files = vec![
                SourceProjectionFile1 {
                    role: "request".into(),
                    path: root.portable_data_path(&output_dir.join("request.json"))?,
                    sha256: sha256(&request_bytes),
                },
                SourceProjectionFile1 {
                    role: "source_observation".into(),
                    path: source_observation.path.clone(),
                    sha256: source_observation.sha256.clone(),
                },
                SourceProjectionFile1 {
                    role: "projection".into(),
                    path: root.portable_data_path(&output_dir.join("projection.json"))?,
                    sha256: sha256(&projection_bytes),
                },
            ];
            for (index, item) in copied.values().enumerate() {
                files.push(SourceProjectionFile1 {
                    role: format!("evidence:{index}"),
                    path: item.path.clone(),
                    sha256: item.sha256.clone(),
                });
            }
            let receipt = SourceProjectionReceipt1 {
                schema: "oev2.source-projection-receipt/1".into(),
                request_sha256: sha256(&request_bytes),
                source_sha256: sha256(&source_bytes),
                projection_sha256: sha256(&projection_bytes),
                output_dir: root.portable_data_path(&output_dir)?,
                files,
                status: status.into(),
                issues,
                warnings: vec!["static source evidence, runtime behavior and engineering adoption remain separate".into()],
                execution_binding: execution.clone(),
                engineering_adoption: "not_assessed".into(),
            };
            write_new(
                &output_dir.join("receipt.json"),
                &serde_json::to_value(&receipt)?,
            )?;
            Ok((
                json!({
                    "status": status,
                    "module": source_value.module,
                    "operation": source_value.operation,
                    "directory": output_dir,
                    "projection": "projection.json",
                    "receipt": "receipt.json",
                    "engineering_adoption": "not_assessed",
                    "execution_binding": execution,
                }),
                if status == "projected" { 0 } else { 1 },
            ))
        }
    }
}
