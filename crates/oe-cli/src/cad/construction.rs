//! Current Project consumer of the shared native construction evidence owner.
use super::correspondence::{artifact, nodes, read};
use anyhow::{Result, ensure};
use oe_cad::evidence::correspondence::construction;
use oe_contracts::{
    ArtifactReference, CadAssemblyConstructionRequest1, CadAssemblyConstructionRequest2,
    CadAssemblyUseSource1,
};
use oe_core::{SkillRoot, sha256, write_new, write_new_bytes};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::path::Path;

enum CheckedConstruction {
    Legacy(construction::VerifiedAssemblyConstruction),
    Scoped(construction::VerifiedAssemblyConstruction2),
}
impl CheckedConstruction {
    fn inputs(
        &self,
    ) -> (
        &ArtifactReference,
        &CadAssemblyUseSource1,
        &ArtifactReference,
        &str,
    ) {
        match self {
            Self::Legacy(c) => {
                let r = c.request();
                (
                    &r.project,
                    &r.assembly,
                    &r.creator_job,
                    &r.creator_native_result_ref,
                )
            }
            Self::Scoped(c) => {
                let r = c.request();
                (
                    &r.project,
                    &r.assembly,
                    &r.creator_job,
                    &r.creator_native_result_ref,
                )
            }
        }
    }
    fn report(&self) -> &Value {
        match self {
            Self::Legacy(c) => c.report(),
            Self::Scoped(c) => c.report(),
        }
    }
    fn impact_seed_ids(&self) -> std::collections::BTreeSet<String> {
        match self {
            Self::Legacy(c) => c.impact_seed_ids(),
            Self::Scoped(c) => c.impact_seed_ids(),
        }
    }
}
fn check(root: &SkillRoot, raw: &[u8]) -> Result<CheckedConstruction> {
    let value = oe_core::json::strict_json(raw)?;
    match value["schema"].as_str() {
        Some("oev2.cad-assembly-construction/1") => {
            let r: CadAssemblyConstructionRequest1 = serde_json::from_value(value)?;
            Ok(CheckedConstruction::Legacy(construction::verify(root, &r)?))
        }
        Some("oev2.cad-assembly-construction/2") => {
            let r: CadAssemblyConstructionRequest2 = serde_json::from_value(value)?;
            Ok(CheckedConstruction::Scoped(construction::verify2(
                root, &r,
            )?))
        }
        _ => anyhow::bail!("cad_construction_schema_or_source_id_invalid"),
    }
}

pub(super) fn run(root: &SkillRoot, input: &Path, output: &Path) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_assembly_construction")?;
    let raw = read(root, input)?;
    let request = oe_core::json::strict_json(&raw)?;
    let checked = check(root, &raw)?;
    let (project_source, assembly, creator_job, creator_native_result_ref) = checked.inputs();
    let project_raw = artifact(root, project_source)?;
    let p: Project = serde_json::from_value(oe_core::json::strict_json(&project_raw)?)?;
    project::cad_focus::bind_cad_focus(&project_raw, &assembly.object, &assembly.project_context)?;
    for digest in [
        assembly.job.sha256.as_str(),
        creator_job.sha256.as_str(),
        checked.report()["construction_intent"]["sha256"]
            .as_str()
            .unwrap(),
    ] {
        ensure!(
            p.evidence.values().any(|s| s == digest),
            "cad_construction_source_not_in_project"
        );
    }
    for (id, digest) in [
        (
            assembly.native_result_ref.as_str(),
            checked.report()["assembly"]["native_result_sha256"]
                .as_str()
                .unwrap(),
        ),
        (
            creator_native_result_ref,
            checked.report()["creator_native_result_sha256"]
                .as_str()
                .unwrap(),
        ),
    ] {
        ensure!(
            p.evidence.get(id).is_some_and(|s| s == digest),
            "cad_construction_native_result_not_in_project"
        );
    }
    let seeds = checked.impact_seed_ids();
    let known = nodes(&p);
    ensure!(
        seeds.iter().all(|id| known.contains(id)),
        "cad_construction_project_dependency_missing"
    );
    let impact = project::impact(&p, &seeds)?;
    let again = check(root, &raw)?;
    ensure!(
        again.report() == checked.report()
            && again.impact_seed_ids() == seeds
            && read(root, input)? == raw
            && artifact(root, project_source)? == project_raw,
        "cad_construction_sources_changed_during_project_check"
    );
    let completion = crate::v2::require_execution(root, "cad_assembly_construction_completion")?;
    for name in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            execution[name] == completion[name],
            "cad_construction_execution_identity_changed"
        );
    }
    let schema = match checked {
        CheckedConstruction::Legacy(_) => "oev2.current-cad-assembly-construction/1",
        CheckedConstruction::Scoped(_) => "oev2.current-cad-assembly-construction/2",
    };
    let report = json!({"schema":schema,"request":request,
        "input_sha256":sha256(&raw),"project_sha256":sha256(&project_raw),"construction":checked.report(),
        "dependency_impact":impact,"project_write":"not_run","native_dispatch":"not_run; recorded source chain checked",
        "engineering_adoption":"not_established","execution_binding":execution,"execution_recheck":completion});
    let output = root.new_private_dir(output)?;
    write_new_bytes(&output.join("input.json"), &raw)?;
    write_new(&output.join("report.json"), &report)?;
    Ok((report, 0))
}
