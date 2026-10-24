//! Source-bound Jacobian mathematics projection.
//!
//! Jacobian remains the producer of the calculation/search/check. This
//! boundary preserves typed values, units, assumptions, numeric mode and
//! certificate status while binding the exact observation to the Project.
//! It never recomputes a result or treats mathematical validity as engineering
//! applicability.
use crate::projection_evidence::{archive, retain};
use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_contracts::{
    ArtifactReference, JacobianMathObservation1, JacobianMathProjection1,
    JacobianMathProjectionFile1, JacobianMathProjectionReceipt1, JacobianMathProjectionRequest1,
};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const INPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Subcommand)]
pub enum JacobianCommand {
    /// Project one canonical Jacobian observation into the current Project.
    Project {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn private_input(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.data_expand(path)?;
    ensure!(
        path.starts_with(root.data_path()),
        "jacobian_private_input_required"
    );
    no_symlinks(&path)?;
    Ok(path)
}

fn decode<T: serde::de::DeserializeOwned>(root: &SkillRoot, path: &Path) -> Result<(Vec<u8>, T)> {
    let path = private_input(root, path)?;
    let bytes = read_regular_bytes(&path, INPUT_BYTES)?;
    let value = serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    Ok((bytes, value))
}

pub fn run(root: &SkillRoot, command: JacobianCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "jacobian_math_projection")?;
    match command {
        JacobianCommand::Project {
            request,
            source,
            output,
        } => {
            let request_path = private_input(root, &request)?;
            let source_path = private_input(root, &source)?;
            let request_bytes = read_regular_bytes(&request_path, INPUT_BYTES)?;
            let request_value: JacobianMathProjectionRequest1 =
                serde_json::from_value(oe_core::json::strict_json(&request_bytes)?)?;
            ensure!(
                request_value.validate().is_ok(),
                "jacobian_request_contract_invalid"
            );
            let (source_bytes, source_value): (Vec<u8>, JacobianMathObservation1) =
                decode(root, &source)?;
            ensure!(
                source_value.validate().is_ok(),
                "jacobian_source_contract_invalid"
            );
            ensure!(
                request_value.operation == source_value.operation,
                "jacobian_operation_mismatch"
            );
            ensure!(
                request_value.project_context == source_value.project_context,
                "jacobian_project_context_mismatch"
            );
            let source_ref = ArtifactReference {
                path: root.portable_data_path(&source_path)?,
                sha256: sha256(&source_bytes),
            };
            ensure!(
                request_value.source == source_ref,
                "jacobian_source_reference_mismatch"
            );

            ensure!(
                request_value.project_context.validate().is_ok(),
                "jacobian_context_contract_invalid"
            );
            ensure!(
                source_value.certificate.validate().is_ok(),
                "jacobian_certificate_contract_invalid"
            );
            for values in [&source_value.inputs, &source_value.outputs] {
                let mut names = BTreeSet::new();
                for value in values {
                    ensure!(value.validate().is_ok(), "jacobian_value_contract_invalid");
                    ensure!(names.insert(&value.name), "jacobian_duplicate_value_name");
                }
            }
            let originals = retain(root, &source_value.certificate.evidence)?;
            let mut issues = Vec::new();
            if source_value.certificate.status != "verified" {
                issues.push(format!(
                    "certificate_status:{}",
                    source_value.certificate.status
                ));
            }
            if source_value.certificate.kind == "none"
                || source_value.certificate.evidence.is_empty()
            {
                issues.push("certificate_evidence_missing".into());
            }
            if !source_value.unknowns.is_empty() {
                issues.push("source_unknowns_present".into());
            }
            if source_value.inputs.is_empty() || source_value.outputs.is_empty() {
                issues.push("math_values_missing".into());
            }
            if matches!(source_value.numeric_mode.as_str(), "floating" | "interval")
                && source_value
                    .error_bound
                    .as_ref()
                    .is_none_or(|e| e.trim().is_empty())
            {
                issues.push("numeric_error_bound_missing".into());
            }
            let status = if issues.is_empty() {
                "projected"
            } else {
                "blocked"
            };
            let output_dir = root.new_private_dir(&output)?;
            write_new_bytes(&output_dir.join("request.json"), &request_bytes)?;
            write_new_bytes(&output_dir.join("source-observation.json"), &source_bytes)?;
            let copied = archive(root, &output_dir, &originals)?;
            let copied_evidence: Vec<_> = source_value
                .certificate
                .evidence
                .iter()
                .map(|item| copied[&item.path].clone())
                .collect();
            let mut certificate = source_value.certificate.clone();
            certificate.evidence = copied_evidence;
            let projection = JacobianMathProjection1 {
                schema: "oev2.jacobian-math-projection/1".into(),
                request: request_value.clone(),
                observation: ArtifactReference {
                    path: root.portable_data_path(&output_dir.join("source-observation.json"))?,
                    sha256: sha256(&source_bytes),
                },
                operation: source_value.operation.clone(),
                status: status.into(),
                inputs: source_value.inputs.clone(),
                outputs: source_value.outputs.clone(),
                assumptions: source_value.assumptions.clone(),
                method: source_value.method.clone(),
                numeric_mode: source_value.numeric_mode.clone(),
                error_bound: source_value.error_bound.clone(),
                certificate,
                coverage: vec![
                    "typed values, units, assumptions and certificate retained".into(),
                    "Project context and source SHA rechecked".into(),
                ],
                limitations: vec![
                    "mathematical result is not an engineering adoption or physical validation"
                        .into(),
                ],
                issues: issues.clone(),
                engineering_adoption: "not_assessed".into(),
            };
            let projection_value = serde_json::to_value(&projection)?;
            write_new(&output_dir.join("projection.json"), &projection_value)?;
            let projection_bytes =
                read_regular_bytes(&output_dir.join("projection.json"), INPUT_BYTES)?;
            let mut files = vec![
                JacobianMathProjectionFile1 {
                    role: "request".into(),
                    path: root.portable_data_path(&output_dir.join("request.json"))?,
                    sha256: sha256(&request_bytes),
                },
                JacobianMathProjectionFile1 {
                    role: "source_observation".into(),
                    path: root.portable_data_path(&output_dir.join("source-observation.json"))?,
                    sha256: sha256(&source_bytes),
                },
                JacobianMathProjectionFile1 {
                    role: "projection".into(),
                    path: root.portable_data_path(&output_dir.join("projection.json"))?,
                    sha256: sha256(&projection_bytes),
                },
            ];
            for (index, item) in projection.certificate.evidence.iter().enumerate() {
                files.push(JacobianMathProjectionFile1 {
                    role: format!("evidence-{index}"),
                    path: item.path.clone(),
                    sha256: item.sha256.clone(),
                });
            }
            let receipt = JacobianMathProjectionReceipt1 {
                schema: "oev2.jacobian-math-projection-receipt/1".into(),
                request_sha256: sha256(&request_bytes),
                source_sha256: sha256(&source_bytes),
                projection_sha256: sha256(&projection_bytes),
                output_dir: root.portable_data_path(&output_dir)?,
                files,
                status: status.into(),
                issues,
                warnings: vec![
                    "independent engineering applicability and adoption remain unassessed".into(),
                ],
                execution_binding: execution.clone(),
                engineering_adoption: "not_assessed".into(),
            };
            write_new(
                &output_dir.join("receipt.json"),
                &serde_json::to_value(&receipt)?,
            )?;
            Ok((
                json!({"status":status,"operation":source_value.operation,"directory":output_dir,"projection":"projection.json","receipt":"receipt.json","engineering_adoption":"not_assessed","execution_binding":execution}),
                if status == "projected" { 0 } else { 1 },
            ))
        }
    }
}
