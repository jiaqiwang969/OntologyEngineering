//! Source-bound electronic-design projection.
//!
//! The EDA programs remain the producers of schematic, PCB, net, model and
//! wiring evidence. This boundary only verifies the exact bytes and their
//! Project binding, then stores a reviewable projection and receipt. It never
//! starts EasyEDA, downloads a model, or turns an unknown check into a pass.
use crate::projection_evidence::{archive, retain};
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use oe_contracts::{
    ArtifactReference, ElectronicsNativeObservation1, ElectronicsProjection1,
    ElectronicsProjectionFile1, ElectronicsProjectionReceipt1, ElectronicsProjectionRequest1,
};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const INPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Subcommand)]
pub enum ElectronicsCommand {
    /// Project one canonical native EDA observation into the current Project.
    Project {
        /// Strict electronics-projection-request/1 JSON.
        #[arg(long)]
        request: PathBuf,
        /// Canonical electronics-native-observation/1 JSON.
        #[arg(long)]
        source: PathBuf,
        /// Additional request-declared artifacts as role=path pairs.
        #[arg(long = "aux")]
        auxiliary: Vec<String>,
        /// New private output directory beneath the selected installation/project store.
        #[arg(long)]
        output: PathBuf,
    },
}

fn private_input(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.data_expand(path)?;
    ensure!(
        path.starts_with(root.data_path()),
        "electronics_private_input_required"
    );
    no_symlinks(&path)?;
    Ok(path)
}

fn decode<T: serde::de::DeserializeOwned>(path: &Path) -> Result<(Vec<u8>, T)> {
    let bytes = read_regular_bytes(path, INPUT_BYTES)?;
    let value = serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    Ok((bytes, value))
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
        "electronics_source_path_mismatch"
    );
    ensure!(
        sha256(bytes) == reference.sha256,
        "electronics_source_digest_mismatch"
    );
    Ok(())
}

fn parse_auxiliary(
    root: &SkillRoot,
    values: &[String],
) -> Result<BTreeMap<String, (PathBuf, Vec<u8>)>> {
    let mut result = BTreeMap::new();
    for value in values {
        let (role, path) = value
            .split_once('=')
            .context("electronics_auxiliary_requires_role_equals_path")?;
        ensure!(!role.trim().is_empty(), "electronics_auxiliary_role_empty");
        ensure!(
            result
                .insert(role.to_owned(), (PathBuf::new(), Vec::new()))
                .is_none(),
            "electronics_auxiliary_role_duplicate"
        );
        let path = private_input(root, Path::new(path))?;
        let bytes = read_regular_bytes(&path, INPUT_BYTES)?;
        result.insert(role.to_owned(), (path, bytes));
    }
    Ok(result)
}

fn verify_declared_auxiliary(
    root: &SkillRoot,
    request: &ElectronicsProjectionRequest1,
    supplied: &BTreeMap<String, (PathBuf, Vec<u8>)>,
) -> Result<Vec<oe_contracts::ElectronicsProjectionAuxiliary1>> {
    let declared: BTreeSet<_> = request.auxiliary.iter().map(|a| a.role.as_str()).collect();
    let actual: BTreeSet<_> = supplied.keys().map(String::as_str).collect();
    ensure!(
        declared.len() == request.auxiliary.len(),
        "electronics_duplicate_auxiliary_role"
    );
    ensure!(declared == actual, "electronics_auxiliary_set_mismatch");
    let mut copied = Vec::with_capacity(request.auxiliary.len());
    for item in &request.auxiliary {
        let (path, bytes) = supplied
            .get(&item.role)
            .context("electronics_auxiliary_missing")?;
        artifact_matches(root, &item.artifact, path, bytes)?;
        copied.push(item.clone());
    }
    Ok(copied)
}

fn safe_role(role: &str) -> Result<()> {
    ensure!(!role.is_empty(), "electronics_auxiliary_role_empty");
    ensure!(
        role.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'),
        "electronics_auxiliary_role_invalid"
    );
    Ok(())
}

fn required_observation_keys(operation: &str) -> &'static [&'static str] {
    match operation {
        "easyeda_snapshot" => &["project_uuid", "document_uuid", "checks", "gates"],
        "easyeda_api_trace" => &["bridge_service", "handshake", "calls"],
        "schematic_net_fanout" => &[
            "components",
            "pins",
            "assignments",
            "wires",
            "flags_ports",
            "nc_pins",
            "coordinate_units",
        ],
        "model_3d" => &[
            "roles",
            "package_audit",
            "model_status",
            "import_allowed",
            "formal_fabrication_release_allowed",
        ],
        "worker_wiring_card" => &[
            "objects",
            "anchors",
            "rows",
            "evidence",
            "render",
            "first_power_allowed",
        ],
        _ => &[],
    }
}

fn domain_status(
    source: &ElectronicsNativeObservation1,
) -> (&'static str, Vec<String>, Vec<String>) {
    let mut issues = Vec::new();
    let mut limitations = source.limitations.clone();
    for key in required_observation_keys(&source.operation) {
        if source.observations.get(*key).is_none_or(Value::is_null) {
            issues.push(format!("observation_field_missing:{}", key));
        }
    }
    if source.operation == "model_3d" {
        if source.observations.get("import_allowed") != Some(&Value::Bool(false))
            || source
                .observations
                .get("formal_fabrication_release_allowed")
                != Some(&Value::Bool(false))
        {
            issues.push("model_3d_release_or_import_true_forbidden".into());
        }
    }
    if source.operation == "worker_wiring_card"
        && source.observations.get("first_power_allowed") != Some(&Value::Bool(false))
    {
        issues.push("worker_wiring_first_power_true_forbidden".into());
    }
    if source.validations.is_empty() {
        issues.push("validation_coverage_missing".into());
    }
    if source.artifacts.iter().any(|a| a.status != "available") {
        issues.push("incomplete_artifacts".into());
    }
    for check in &source.validations {
        if check.execution != "completed"
            || check.verdict != "pass"
            || check.evidence.is_empty()
            || !check.unknowns.is_empty()
        {
            issues.push(format!(
                "{}:{}:{}",
                check.check_id, check.execution, check.verdict
            ));
        }
        if !check.unknowns.is_empty() {
            limitations.extend(check.unknowns.clone());
        }
    }
    if issues.iter().any(|issue| {
        issue.starts_with("observation_field_missing:") || issue.ends_with("forbidden")
    }) {
        ("invalid", issues, limitations)
    } else if !issues.is_empty() {
        ("blocked", issues, limitations)
    } else {
        ("projected", issues, limitations)
    }
}

pub fn run(root: &SkillRoot, command: ElectronicsCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "electronics_projection")?;
    match command {
        ElectronicsCommand::Project {
            request,
            source,
            auxiliary,
            output,
        } => {
            let request_path = private_input(root, &request)?;
            let source_path = private_input(root, &source)?;
            let (request_bytes, request_value): (Vec<u8>, ElectronicsProjectionRequest1) =
                decode(&request_path)?;
            ensure!(
                request_value.validate().is_ok(),
                "electronics_request_contract_invalid"
            );
            let (source_bytes, source_value): (Vec<u8>, ElectronicsNativeObservation1) =
                decode(&source_path)?;
            ensure!(
                source_value.validate().is_ok(),
                "electronics_source_contract_invalid"
            );
            ensure!(
                request_value.operation == source_value.operation,
                "electronics_operation_mismatch"
            );
            ensure!(
                request_value.project_context == source_value.project_context,
                "electronics_project_context_mismatch"
            );
            ensure!(
                request_value.policy == "exact_source_no_inference",
                "electronics_projection_policy_invalid"
            );
            artifact_matches(root, &request_value.source, &source_path, &source_bytes)?;

            let supplied = parse_auxiliary(root, &auxiliary)?;
            let validated_auxiliary = verify_declared_auxiliary(root, &request_value, &supplied)?;
            for item in &validated_auxiliary {
                safe_role(&item.role)?;
            }
            ensure!(
                request_value.project_context.validate().is_ok(),
                "electronics_context_contract_invalid"
            );
            let mut artifact_ids = BTreeSet::new();
            for artifact in &source_value.artifacts {
                ensure!(
                    artifact.validate().is_ok() && artifact.source.validate().is_ok(),
                    "electronics_artifact_contract_invalid"
                );
                ensure!(
                    artifact_ids.insert(&artifact.artifact_id),
                    "electronics_duplicate_artifact_id"
                );
            }
            let mut check_ids = BTreeSet::new();
            for check in &source_value.validations {
                ensure!(
                    check.validate().is_ok(),
                    "electronics_validation_contract_invalid"
                );
                ensure!(
                    check_ids.insert(&check.check_id),
                    "electronics_duplicate_check_id"
                );
            }
            let mut refs: Vec<_> = source_value
                .artifacts
                .iter()
                .filter(|a| a.status != "missing")
                .map(|a| a.source.clone())
                .collect();
            for check in &source_value.validations {
                refs.extend(check.evidence.clone());
            }
            let originals = retain(root, &refs)?;
            ensure!(
                source_value.observations.is_object(),
                "electronics_observations_object_required"
            );
            ensure!(
                source_value.engineering_adoption == "not_assessed",
                "electronics_source_adoption_forbidden"
            );
            let (status, issues, mut limitations) = domain_status(&source_value);
            limitations.push("Project context is a supplied snapshot; current Project and producer identity require ToolResult verification".into());
            limitations.push("Native EDA payload is retained; this outer bridge does not independently verify circuit or PCB semantics".into());
            let output_dir = root.new_private_dir(&output)?;
            write_new_bytes(&output_dir.join("request.json"), &request_bytes)?;
            write_new_bytes(&output_dir.join("source-observation.json"), &source_bytes)?;
            oe_core::create_new_private_dir(&output_dir.join("aux"))?;
            let mut copied_auxiliary = Vec::with_capacity(validated_auxiliary.len());
            for item in &validated_auxiliary {
                let (_, bytes) = supplied
                    .get(&item.role)
                    .context("electronics_auxiliary_missing")?;
                let copy_path = output_dir.join("aux").join(format!("{}.bin", item.role));
                write_new_bytes(&copy_path, bytes)?;
                let mut copy = item.clone();
                copy.artifact.path = root.portable_data_path(&copy_path)?;
                copy.artifact.sha256 = sha256(bytes);
                copied_auxiliary.push(copy);
            }

            let copied = archive(root, &output_dir, &originals)?;
            let mut artifacts = source_value.artifacts.clone();
            for artifact in &mut artifacts {
                if artifact.status != "missing" {
                    artifact.source = copied[&artifact.source.path].clone();
                }
            }
            let mut validations = source_value.validations.clone();
            for check in &mut validations {
                check.evidence = check
                    .evidence
                    .iter()
                    .map(|e| copied[&e.path].clone())
                    .collect();
            }
            let projection = ElectronicsProjection1 {
                schema: "oev2.electronics-projection/1".into(),
                request: request_value.clone(),
                operation: source_value.operation.clone(),
                status: status.into(),
                source_sha256: sha256(&source_bytes),
                artifacts: artifacts.clone(),
                validations,
                source_observation: ArtifactReference {
                    path: root.portable_data_path(&output_dir.join("source-observation.json"))?,
                    sha256: sha256(&source_bytes),
                },
                validated_artifacts: copied_auxiliary.clone(),
                coverage: vec![
                    "request/source schema and Project context matched".into(),
                    "declared source and auxiliary bytes re-read and SHA-256 checked".into(),
                    "native EDA semantic payload preserved in source-observation.json".into(),
                ],
                limitations,
                issues: issues.clone(),
                engineering_adoption: "not_assessed".into(),
            };
            let projection_value = serde_json::to_value(&projection)?;
            write_new(&output_dir.join("projection.json"), &projection_value)?;
            let projection_bytes =
                read_regular_bytes(&output_dir.join("projection.json"), INPUT_BYTES)?;
            let files = vec![
                ElectronicsProjectionFile1 {
                    role: "request".into(),
                    path: root.portable_data_path(&output_dir.join("request.json"))?,
                    sha256: sha256(&request_bytes),
                },
                ElectronicsProjectionFile1 {
                    role: "source_observation".into(),
                    path: root.portable_data_path(&output_dir.join("source-observation.json"))?,
                    sha256: sha256(&source_bytes),
                },
                ElectronicsProjectionFile1 {
                    role: "projection".into(),
                    path: root.portable_data_path(&output_dir.join("projection.json"))?,
                    sha256: sha256(&projection_bytes),
                },
            ];
            let mut files = files;
            for item in &copied_auxiliary {
                files.push(ElectronicsProjectionFile1 {
                    role: format!("aux:{}", item.role),
                    path: item.artifact.path.clone(),
                    sha256: item.artifact.sha256.clone(),
                });
            }
            for (index, item) in copied.values().enumerate() {
                files.push(ElectronicsProjectionFile1 {
                    role: format!("evidence:{index}"),
                    path: item.path.clone(),
                    sha256: item.sha256.clone(),
                });
            }
            let receipt = ElectronicsProjectionReceipt1 {
                schema: "oev2.electronics-projection-receipt/1".into(),
                request_sha256: sha256(&request_bytes),
                source_sha256: sha256(&source_bytes),
                projection_sha256: sha256(&projection_bytes),
                output_dir: root.portable_data_path(&output_dir)?,
                files,
                status: status.into(),
                issues,
                warnings: vec![
                    "engineering adoption and formal fabrication release remain unassessed".into(),
                ],
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

#[cfg(test)]
mod tests {
    use super::required_observation_keys;

    #[test]
    fn operation_source_fields_are_explicit() {
        assert!(required_observation_keys("easyeda_snapshot").contains(&"gates"));
        assert!(required_observation_keys("easyeda_api_trace").contains(&"calls"));
        assert!(required_observation_keys("schematic_net_fanout").contains(&"assignments"));
        assert!(
            required_observation_keys("model_3d").contains(&"formal_fabrication_release_allowed")
        );
        assert!(required_observation_keys("worker_wiring_card").contains(&"first_power_allowed"));
        assert!(required_observation_keys("unknown").is_empty());
    }
}
