//! Project-bound discovery: the current Project owner supervises an isolated worker that receives only the
//! drawing bytes over stdin under a deny-default sandbox, checks the returned graph and writes it with its review.
use crate::{
    engineering_expression::{PROFILE, check},
    usage_discovery,
};
use anyhow::{Result, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oe_kernel::project::{self, Project};
use serde::Serialize;
use serde_json::Value;
use std::{fs::OpenOptions, io::Write, path::Path};

/// Hidden flag that makes the oev2 executable act as the bytes-only worker.
pub const WORKER_FLAG: &str = "--drawing-source-worker";
const LIMIT: usize = 16 * 1024 * 1024;

/// The bytes-only worker (moved from the aixcam-drawing entry point): reads the drawing from stdin and returns
/// its expression graph as JSON. It opens no root, Project, request or path; any extra argument is refused.
pub fn worker(arguments: &[std::ffi::OsString]) -> Result<String> {
    use std::io::Read;
    ensure!(arguments.len() == 2, "expression_worker_no_extra_arguments");
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= LIMIT, "expression_worker_input_budget");
    let graph = crate::engineering_expression::discover(&bytes)?;
    Ok(serde_json::to_string(&graph)?)
}

pub(crate) fn read_source(root: &SkillRoot, reference: &ArtifactReference) -> Result<Vec<u8>> {
    ensure!(reference.validate().is_ok(), "native_source_contract");
    let bytes = read_regular_bytes(&root.private_path(Path::new(&reference.path))?, LIMIT)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "native_source_changed:{}",
        reference.path
    );
    Ok(bytes)
}

pub(crate) fn current_context(
    root: &SkillRoot,
    part: &ObjectIdentity,
    context: &ProjectReviewContext,
) -> Result<Value> {
    let current = oe_engineering::project_workspace::resume(root)?;
    ensure!(
        current["route"]["status"] == "anchored",
        "native_workspace_route"
    );
    ensure!(
        current["pending_decisions"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "native_pending_decisions"
    );
    ensure!(
        matches!(current["status"].as_str(), Some("ready" | "effect_unknown")),
        "native_workspace_incomplete"
    );
    let reference: ArtifactReference = serde_json::from_value(current["project"].clone())?;
    let project: Project =
        serde_json::from_value(oe_core::json::strict_json(&read_source(root, &reference)?)?)?;
    ensure!(
        project::context(&project, part)? == *context,
        "native_current_project_changed"
    );
    Ok(current)
}

pub(crate) fn write<T: Serialize>(
    root: &SkillRoot,
    directory: &str,
    file: &str,
    value: &T,
) -> Result<ArtifactReference> {
    let path = format!("{directory}/{file}");
    let bytes = serde_json::to_vec_pretty(value)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.private_path(Path::new(&path))?)?
        .write_all(&bytes)?;
    Ok(ArtifactReference {
        path,
        sha256: sha256(&bytes),
    })
}

// ---------------------------------------------------------------- supervised review

pub struct EngineeringExpressionReviewer<'a> {
    pub root: &'a oe_core::SkillRoot,
    pub project: &'a oe_kernel::project::Project,
}
impl MdEngineeringExpressionPort for EngineeringExpressionReviewer<'_> {
    fn discover_expressions(
        &self,
        q: &MdUsageDiscoveryRequest,
    ) -> std::result::Result<MdUsageDiscoveryReview, FailureRecord> {
        review(self.root, self.project, q)
            .map_err(|e| oe_engineering::artifacts::failure("engineering_expression", e))
    }
}

/// The current Project owner supervises an isolated worker that receives only
/// the source bytes over stdin under a deny-default sandbox.
pub fn review(
    root: &oe_core::SkillRoot,
    p: &oe_kernel::project::Project,
    q: &MdUsageDiscoveryRequest,
) -> Result<MdUsageDiscoveryReview> {
    use std::{
        io::Write,
        path::Path,
        process::{Command, Stdio},
    };
    ensure!(
        q.validate().is_ok()
            && q.schema == "oev2.drawing-discovery-request/1"
            && q.profile == PROFILE,
        "engineering_expression_contract"
    );
    ensure!(
        oe_kernel::project::context(p, &q.subject)? == q.project_context,
        "engineering_expression_context"
    );
    let before = current_context(root, &q.subject, &q.project_context)?;
    let bytes = read_source(root, &q.source)?;
    ensure!(
        cfg!(target_os = "macos"),
        "engineering_expression_sandbox_platform"
    );
    let exe = std::env::current_exe()?;
    let profile = usage_discovery::worker_profile(&exe)?;
    let mut child = Command::new("/usr/bin/sandbox-exec")
        .args(["-p", &profile])
        .arg(&exe)
        .arg(WORKER_FLAG)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(&bytes)?;
    let result = child.wait_with_output()?;
    ensure!(
        result.status.success(),
        "engineering_expression_worker_failed:{}",
        String::from_utf8_lossy(&result.stderr)
    );
    ensure!(
        result.stdout.len() < 16 * 1024 * 1024,
        "engineering_expression_output_budget"
    );
    let graph: MdEngineeringExpressionGraph =
        serde_json::from_value(oe_core::json::strict_json(&result.stdout)?)?;
    check(&graph)?;
    ensure!(
        graph.source_sha256 == q.source.sha256 && graph.profile == PROFILE,
        "engineering_expression_worker_identity"
    );
    std::fs::create_dir(root.private_path(Path::new(&q.output_directory))?)?;
    let request = write(root, &q.output_directory, "request.json", q)?;
    let graph_ref = write(root, &q.output_directory, "graph.json", &graph)?;
    let review = MdUsageDiscoveryReview {
        schema: "oev2.drawing-discovery-review/1".into(),
        subject: q.subject.clone(),
        project_context: q.project_context.clone(),
        source: q.source.clone(),
        request,
        graph: graph_ref,
        worker_sha256: oe_core::sha256(&std::fs::read(exe)?),
        sandbox_profile: profile,
        status: "expression_candidates_for_review".into(),
        unknowns: graph.unknowns,
    };
    ensure!(
        review.validate().is_ok()
            && bytes == read_source(root, &q.source)?
            && before == current_context(root, &q.subject, &q.project_context)?,
        "engineering_expression_context_changed"
    );
    write(root, &q.output_directory, "review.json", &review)?;
    Ok(review)
}
