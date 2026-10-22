//! inCAD preparation consumes the existing current Project and work contract.
use crate::workflow::{Reference, WorkInput, read_reference, verify_project_binding};
use anyhow::{Context, Result, ensure};
use oe_contracts::IncadWorkPreparationRequest;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oe_knowledge::incad::VerifiedIntake;
use serde_json::{Value, json};
pub mod cad;

pub struct Prepared {
    pub work_bytes: Vec<u8>,
    pub intake: VerifiedIntake,
    pub result: Value,
    request: IncadWorkPreparationRequest,
    project_bytes: Vec<u8>,
    project_reference: Reference,
}
impl Prepared {
    pub fn ensure_current(&self, root: &SkillRoot) -> Result<()> {
        self.intake.ensure_current()?;
        let p = root.expand(std::path::Path::new(&self.request.work.path))?;
        ensure!(
            read_regular_bytes(&p, 16 * 1024 * 1024)? == self.work_bytes,
            "incad_work_changed"
        );
        ensure!(
            read_reference(root, &self.project_reference)? == self.project_bytes,
            "incad_project_changed"
        );
        Ok(())
    }
}
pub fn prepare(root: &SkillRoot, request: &IncadWorkPreparationRequest) -> Result<Prepared> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("incad_preparation_generated_contract_invalid"))?;
    ensure!(
        request.schema == "oev2.incad-work-preparation/1",
        "incad_preparation_schema"
    );
    let reference = Reference {
        path: request.work.path.clone().into(),
        sha256: request.work.sha256.clone(),
    };
    let work_bytes = read_reference(root, &reference)?;
    let work: WorkInput = serde_json::from_value(oe_core::json::strict_json(&work_bytes)?)?;
    ensure!(
        work.schema == "oev2.engineering-work/3",
        "incad_current_work3_required"
    );
    let project_bytes = read_reference(root, &work.project)?;
    let project = serde_json::from_value(oe_core::json::strict_json(&project_bytes)?)?;
    verify_project_binding(&project, &work)?;
    let intake = oe_knowledge::incad::verify(&request.intake)?;
    let ids = request
        .source_ids
        .as_object()
        .context("incad_project_source_ids_missing")?;
    ensure!(
        ids.len() == intake.roles().len(),
        "incad_project_source_roles_changed"
    );
    let mut unique = std::collections::BTreeSet::new();
    for (role, pin) in intake.roles() {
        let id = ids
            .get(role)
            .and_then(Value::as_str)
            .context("incad_project_source_role_missing")?;
        ensure!(
            unique.insert(id) && project.evidence.get(id) == Some(&pin.sha256),
            "incad_source_not_registered_in_current_project:{role}"
        );
    }
    let result = json!({"schema":"oev2.incad-work-prepared/1","request":request,
        "project":work.project,"project_id":project.project_id,"project_revision":project.revision,
        "object":work.object,"project_context":work.project_context,"work_sha256":sha256(&work_bytes),
        "intake":intake.audit(),"design_window":intake.design_window(),
        "work_execution":"not_run","model_execution":"not_run","native_execution":"not_run",
        "engineering_applicability":"not_reviewed","task_work_goal_relation":"not_reviewed",
        "project_mutated":false,"engineering_adoption":"not_assessed",
        "reuse":"reverify complete original request, current Project and selected source closure before use"});
    let prepared = Prepared {
        work_bytes,
        intake,
        result,
        request: request.clone(),
        project_bytes,
        project_reference: work.project,
    };
    prepared.ensure_current(root)?;
    Ok(prepared)
}
pub fn verify_preparation(
    root: &SkillRoot,
    request: &IncadWorkPreparationRequest,
    previous: &Value,
) -> Result<Value> {
    let current = prepare(root, request)?;
    ensure!(*previous == current.result, "incad_preparation_changed");
    current.ensure_current(root)?;
    Ok(
        json!({"schema":"oev2.incad-work-revalidation/1","status":"current_source_preparation_verified",
        "prepared_sha256":sha256(oe_core::json::canonical_json(previous)?),"project_mutated":false,
        "engineering_applicability":"not_reviewed","task_work_goal_relation":"not_reviewed",
        "engineering_adoption":"not_assessed"}),
    )
}
