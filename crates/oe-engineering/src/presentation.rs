//! Presentation intent uses the existing Project/context and source owners.
use anyhow::{Result, ensure};
use oe_contracts::{ObjectIdentity, PresentationRequest1, PresentationScene1};
use oe_core::SkillRoot;
use oe_kernel::project::{self, Project};
use std::path::Path;

pub fn load_project(root: &SkillRoot, path: &Path) -> Result<Project> {
    let bytes = oe_core::read_regular_bytes(&root.expand(path)?, 4 * 1024 * 1024)?;
    let project: Project = serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    project::validate(&project)?;
    Ok(project)
}
pub fn bind(
    root: &SkillRoot,
    project: &Project,
    source: &Path,
    object_id: &str,
    condition: &str,
    title: &str,
) -> Result<PresentationRequest1> {
    let source = crate::artifacts::reference(root, &root.expand(source)?)?;
    let object = ObjectIdentity {
        object_id: object_id.into(),
        revision: project
            .objects
            .get(object_id)
            .ok_or_else(|| anyhow::anyhow!("presentation_object_missing"))?
            .clone(),
        source_sha256: source.sha256.clone(),
        condition_scope: condition.into(),
    };
    let request = PresentationRequest1 {
        schema: "oev2.presentation-request/1".into(),
        id: format!("{object_id}-presentation"),
        title: title.into(),
        project_context: project::context(project, &object)?,
        object,
        source,
        scene_pointer: "/scene".into(),
    };
    prepare(root, project, &request)?;
    Ok(request)
}
pub fn prepare(
    root: &SkillRoot,
    project: &Project,
    request: &PresentationRequest1,
) -> Result<PresentationScene1> {
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("presentation_request_contract:{e:?}"))?;
    ensure!(
        request.id.len() <= 160
            && request.title.len() <= 240
            && !request.title.chars().any(char::is_control)
            && request.scene_pointer.starts_with('/')
            && request.scene_pointer.len() <= 512,
        "presentation_request_bounds"
    );
    ensure!(
        request.source.sha256 == request.object.source_sha256,
        "presentation_source_object_mismatch"
    );
    let check =
        project::check_current_context(project, &request.object, Some(&request.project_context));
    ensure!(
        check.conclusion == "pass",
        "presentation_project_stale:{:?}",
        check.failures
    );
    let source = crate::artifacts::json(root, &request.source)?;
    let scene: PresentationScene1 = serde_json::from_value(
        source
            .pointer(&request.scene_pointer)
            .ok_or_else(|| anyhow::anyhow!("presentation_scene_missing"))?
            .clone(),
    )?;
    scene
        .validate()
        .map_err(|e| anyhow::anyhow!("presentation_scene_contract:{e:?}"))?;
    Ok(scene)
}
