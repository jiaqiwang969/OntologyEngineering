//! Complete Project binding for read-only native motion reception.
//! The CAD owner interprets native jobs, identities, samples and saved files.
use anyhow::{Result, ensure};
use oe_contracts::{ArtifactReference, CadMotionReceiveRequest1, CadMotionReopenRequest1};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::Value;
use std::path::Path;

mod assertion;
pub use assertion::{current, declared};

fn read(root: &SkillRoot, reference: &ArtifactReference) -> Result<Vec<u8>> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_reference_invalid"))?;
    let path = root.expand(Path::new(&reference.path))?;
    ensure!(
        path.starts_with(root.path().join("var")),
        "motion_source_outside_private_root"
    );
    oe_core::no_symlinks(&path)?;
    let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "motion_source_digest_differs"
    );
    Ok(bytes)
}

// No serialized report can construct this verified observation.
struct BoundMotion {
    value: Value,
    code: u8,
    project: crate::project::Project,
    request: CadMotionReceiveRequest1,
}

fn bound(
    root: &SkillRoot,
    reference: &ArtifactReference,
    raw: &[u8],
    reopen: bool,
) -> Result<BoundMotion> {
    let (motion_reference, request_raw) = if reopen {
        let r: CadMotionReopenRequest1 = serde_json::from_value(oe_core::json::strict_json(raw)?)?;
        let bytes = read(root, &r.motion_request)?;
        (r.motion_request, bytes)
    } else {
        (reference.clone(), raw.to_vec())
    };
    let request: CadMotionReceiveRequest1 =
        serde_json::from_value(oe_core::json::strict_json(&request_raw)?)?;
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_receive_request_invalid"))?;
    let project = read(root, &request.project)?;
    let focus = crate::project::cad_focus::bind_cad_focus(
        &project,
        &request.object,
        &request.project_context,
    )?;
    let value = if reopen {
        oe_cad::motion::receive_reopen(root, reference)?
    } else {
        oe_cad::motion::receive(root, reference)?
    };
    ensure!(
        read(root, reference)? == raw && read(root, &motion_reference)? == request_raw,
        "motion_request_changed_during_check"
    );
    focus.ensure_current(&read(root, &request.project)?)?;
    let code = match value["outcome"].as_str() {
        Some("accepted_bounded") => 0,
        Some("partial" | "unknown") => 1,
        _ => anyhow::bail!("motion_receive_outcome_invalid"),
    };
    Ok(BoundMotion {
        value,
        code,
        project: serde_json::from_value(oe_core::json::strict_json(&project)?)?,
        request,
    })
}

/// Original CLI boundary and error semantics, shared with current checks.
pub fn receive(root: &SkillRoot, input: &Path, reopen: bool) -> Result<(Value, u8)> {
    let path = root.expand(input)?;
    ensure!(
        path.starts_with(root.path().join("var")),
        "motion_request_outside_private_root"
    );
    oe_core::no_symlinks(&path)?;
    let raw = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    let reference = ArtifactReference {
        path: path.to_string_lossy().into_owned(),
        sha256: sha256(&raw),
    };
    let received = bound(root, &reference, &raw, reopen)?;
    Ok((received.value, received.code))
}
