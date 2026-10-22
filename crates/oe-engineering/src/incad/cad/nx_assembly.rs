//! The selected-source bridge prepares the existing NX job locally. It never
//! dispatches CAD, rewrites original handoffs or changes a Project/adoption.
use super::Selected;
use anyhow::{Context, Result, ensure};
use oe_contracts::{IncadNxAssemblyBridgeRequest, IncadNxAssemblyBridgeResult};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use oe_nx_protocol::{BlobIdentity, JobId, RelativePath, RootIdentity, inventory_digest};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const FILE_LIMIT: usize = 16 * 1024 * 1024;
struct Snapshot {
    selected: Selected,
    staged: PathBuf,
    source_binding: Value,
    parameters: Vec<u8>,
    inputs: BTreeMap<String, Vec<u8>>,
    root_identity: RootIdentity,
    job_id: JobId,
}
impl Snapshot {
    fn current(&self, root: &SkillRoot, request: &IncadNxAssemblyBridgeRequest) -> Result<()> {
        super::verify_staged(root, &request.selection, &self.staged)?;
        self.selected.ensure_current(root)
    }
}
fn read_snapshot(root: &SkillRoot, request: &IncadNxAssemblyBridgeRequest) -> Result<Snapshot> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("incad_nx_bridge_contract_invalid"))?;
    ensure!(
        request.schema == "oev2.incad-nx-assembly-bridge/1"
            && request.source_role == "supplier_reference_assembly",
        "incad_nx_bridge_role_or_schema"
    );
    let handoff_path = root.private_path(Path::new(&request.staged_handoff.path))?;
    ensure!(
        handoff_path
            .file_name()
            .is_some_and(|v| v == "handoff.json"),
        "incad_nx_bridge_handoff_name"
    );
    let handoff_bytes = read_regular_bytes(&handoff_path, FILE_LIMIT)?;
    ensure!(
        sha256(&handoff_bytes) == request.staged_handoff.sha256,
        "incad_nx_bridge_handoff_sha_changed"
    );
    let staged = handoff_path
        .parent()
        .context("incad_nx_bridge_staged_parent")?
        .to_owned();
    super::verify_staged(root, &request.selection, &staged)?;
    let selected = super::prepare(root, &request.selection)?;
    let step = selected
        .cad
        .files()
        .iter()
        .find(|f| f.relative_path == request.step_source.path)
        .context("incad_nx_bridge_step_not_selected")?;
    ensure!(
        step.role == "archive_member"
            && step.identity.sha256 == request.step_source.sha256
            && matches!(
                Path::new(&step.relative_path)
                    .extension()
                    .and_then(|v| v.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("step" | "stp")
            ),
        "incad_nx_bridge_step_identity_or_type"
    );
    let header = oe_cad::supplier::artifacts::inspect_step_header(&step.bytes)?;
    let step_input = format!("inputs/reference/payload/{}", step.relative_path);
    let parameters = oe_cad::nx::import_assembly_parameters(&step_input, &request.budget)?;
    let root_identity: RootIdentity = serde_json::from_value(request.root_identity.clone())?;
    root_identity.validate()?;
    let job_id = JobId::new(request.job_id.clone())?;
    let mut inputs = BTreeMap::new();
    for file in selected.cad.files() {
        let name = format!("inputs/reference/payload/{}", file.relative_path);
        RelativePath::new(name.clone())?;
        ensure!(
            inputs.insert(name, file.bytes.clone()).is_none(),
            "incad_nx_bridge_duplicate_source"
        );
    }
    // These are the original records, including original local path strings.
    // Parameters use job-relative inputs instead of changing those records.
    for name in [
        "request.json",
        "work.json",
        "project.json",
        "preparation.json",
        "handoff.json",
        "completed.json",
    ] {
        let bytes = read_regular_bytes(&staged.join(name), FILE_LIMIT)?;
        ensure!(
            inputs
                .insert(format!("inputs/reference/records/{name}"), bytes)
                .is_none(),
            "incad_nx_bridge_duplicate_record"
        );
    }
    ensure!(
        inputs["inputs/reference/records/work.json"] == selected.preparation.work_bytes
            && inputs["inputs/reference/records/handoff.json"] == handoff_bytes,
        "incad_nx_bridge_original_records_changed"
    );
    let input_identity: BTreeMap<_, _> = inputs
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                json!({"bytes":bytes.len(),"sha256":sha256(bytes)}),
            )
        })
        .collect();
    let source_binding = json!({"schema":"oev2.incad-nx-assembly-source-binding/1",
        "project":selected.preparation.result["project"],"object":selected.preparation.result["object"],
        "project_context":selected.preparation.result["project_context"],
        "project_sha256":sha256(&inputs["inputs/reference/records/project.json"]),
        "work_sha256":sha256(&selected.preparation.work_bytes),"handoff_sha256":sha256(&handoff_bytes),
        "selection":request.selection,"staged_handoff":request.staged_handoff,"original_source_audit":selected.preparation.intake.audit(),
        "cad_source_audit":selected.cad.audit(),"complete_original_inputs":input_identity,
        "selected_step":{"original_path":step.relative_path,"job_input":step_input,"identity":step.identity,
            "supplier_option":step.format,"actual_header":header},"budget":request.budget,
        "source_role":"supplier_reference_assembly","reference_exposure":"development_replay",
        "units":"native_reader_required","native_execution":"not_run","engineering_adoption":"not_assessed"});
    let snapshot = Snapshot {
        selected,
        staged,
        source_binding,
        parameters,
        inputs,
        root_identity,
        job_id,
    };
    snapshot.current(root, request)?;
    Ok(snapshot)
}
fn result(
    snapshot: &Snapshot,
    package: &oe_cad::nx::Package,
    status: &str,
) -> Result<IncadNxAssemblyBridgeResult> {
    let value = json!({"schema":"oev2.incad-nx-assembly-bridge-result/1","status":status,
        "source_binding":snapshot.source_binding,
        "job_binding":{"schema":"oev2.incad-nx-assembly-job-binding/1","job_id":package.job.job_id,
            "root_identity":package.job.root,"job_digest":package.job.digest()?,
            "input_digest":inventory_digest(&package.job.inputs)?,"worker_digest":package.job.worker.digest()?,
            "parameter_identity":package.job.params,"action":package.job.action,"parameter_schema":package.job.params_schema,
            "source_binding_input":"inputs/reference/source-binding.json"},
        "source_role":"supplier_reference_assembly","reference_exposure":"development_replay",
        "native_execution":"not_run","engineering_adoption":"not_assessed"});
    let r: IncadNxAssemblyBridgeResult = serde_json::from_value(value)?;
    r.validate()
        .map_err(|_| anyhow::anyhow!("incad_nx_bridge_result_contract"))?;
    Ok(r)
}
/// Create a new task-owned directory and existing NX package; no remote call.
pub fn prepare(
    root: &SkillRoot,
    request: &IncadNxAssemblyBridgeRequest,
    output: &Path,
) -> Result<IncadNxAssemblyBridgeResult> {
    let snapshot = read_snapshot(root, request)?;
    oe_cad::nx::verify_worker(root, Path::new(&request.worker_directory))?;
    let out = root.new_private_dir(output)?;
    write_new(&out.join("request.json"), &serde_json::to_value(request)?)?;
    write_new_bytes(&out.join("parameters.json"), &snapshot.parameters)?;
    let original_dir = out.join("originals");
    oe_core::create_new_private_dir(&original_dir)?;
    let mut bindings = Vec::new();
    for (member, bytes) in &snapshot.inputs {
        let name = member
            .strip_prefix("inputs/")
            .context("incad_nx_bridge_input_prefix")?;
        let source = original_dir.join(name);
        oe_core::ensure_directory(source.parent().context("incad_nx_bridge_source_parent")?)?;
        write_new_bytes(&source, bytes)?;
        bindings.push(oe_cad::nx::InputBinding {
            source,
            member: RelativePath::new(member.clone())?,
        });
    }
    let source = out.join("source-binding.json");
    write_new(&source, &snapshot.source_binding)?;
    bindings.push(oe_cad::nx::InputBinding {
        source,
        member: RelativePath::new("inputs/reference/source-binding.json")?,
    });
    let package = oe_cad::nx::prepare(
        root,
        &oe_cad::nx::PrepareRequest {
            worker_directory: request.worker_directory.clone().into(),
            parameter_file: out.join("parameters.json"),
            inputs: bindings,
            job_id: snapshot.job_id.clone(),
            root_identity: snapshot.root_identity.clone(),
            action: "import_step_assembly".into(),
            parameter_schema: "cad-agent.nx-native-assembly-params/v1".into(),
        },
    )?;
    oe_cad::nx::publish(&package, root, &out.join("prepared"))?;
    snapshot.current(root, request)?;
    let terminal = result(&snapshot, &package, "prepared")?;
    write_new(
        &out.join("completed.json"),
        &serde_json::to_value(&terminal)?,
    )?;
    verify(root, request, output)?;
    Ok(terminal)
}
/// Recheck all current sources, preserved records, parameters and actual job.
pub fn verify(
    root: &SkillRoot,
    request: &IncadNxAssemblyBridgeRequest,
    prepared: &Path,
) -> Result<IncadNxAssemblyBridgeResult> {
    let snapshot = read_snapshot(root, request)?;
    let out = root.private_path(prepared)?;
    let read = |p: &Path| read_regular_bytes(p, FILE_LIMIT);
    ensure!(
        oe_core::json::strict_json(&read(&out.join("request.json"))?)?
            == serde_json::to_value(request)?
            && read(&out.join("parameters.json"))? == snapshot.parameters
            && oe_core::json::strict_json(&read(&out.join("source-binding.json"))?)?
                == snapshot.source_binding,
        "incad_nx_bridge_preserved_input_changed"
    );
    let package = oe_cad::nx::from_directory(root, &out.join("prepared/job"))?;
    let current_worker = oe_cad::nx::verify_worker(root, Path::new(&request.worker_directory))?;
    ensure!(
        current_worker.worker_manifest_digest == package.job.worker.digest()?,
        "incad_nx_bridge_current_worker_changed"
    );
    ensure!(
        package.job.job_id == snapshot.job_id
            && package.job.root == snapshot.root_identity
            && package.job.action == "import_step_assembly"
            && package.job.params_schema == "cad-agent.nx-native-assembly-params/v1"
            && package.members["params.json"] == snapshot.parameters,
        "incad_nx_bridge_job_or_parameters_changed"
    );
    let expected_inputs: BTreeSet<_> = snapshot
        .inputs
        .keys()
        .cloned()
        .chain(Some("inputs/reference/source-binding.json".into()))
        .collect();
    ensure!(
        package
            .job
            .inputs
            .iter()
            .map(|f| f.path.as_str().to_owned())
            .collect::<BTreeSet<_>>()
            == expected_inputs,
        "incad_nx_bridge_job_input_closure_changed"
    );
    for (member, bytes) in &snapshot.inputs {
        let original = out
            .join("originals")
            .join(member.strip_prefix("inputs/").unwrap());
        ensure!(
            read(&original)? == *bytes && package.members[member] == *bytes,
            "incad_nx_bridge_source_bytes_changed"
        );
    }
    ensure!(
        oe_core::json::strict_json(&package.members["inputs/reference/source-binding.json"])?
            == snapshot.source_binding,
        "incad_nx_bridge_job_source_binding_changed"
    );
    let expected = result(&snapshot, &package, "prepared")?;
    ensure!(
        oe_core::json::strict_json(&read(&out.join("completed.json"))?)?
            == serde_json::to_value(expected)?,
        "incad_nx_bridge_terminal_changed"
    );
    let archive = read_regular_bytes(
        &out.join("prepared/job.zip"),
        oe_nx_protocol::MAX_PAYLOAD_BYTES as usize,
    )?;
    let archived = oe_cad::nx::from_archive(&archive)?;
    ensure!(
        archived.job == package.job && archived.members == package.members,
        "incad_nx_bridge_archive_closure_changed"
    );
    let archive_identity = BlobIdentity::of(&archive);
    let intent = json!({"schema":"cad-agent.nx-prepare-intent/v2","job_id":package.job.job_id,
        "manifest_digest":package.job.digest()?,"archive":archive_identity,"dispatch":"not_performed"});
    let prepared = json!({"schema":"cad-agent.nx-prepared/v2","job_id":package.job.job_id,
        "manifest_digest":package.job.digest()?,"payload_digest":inventory_digest(&package.job.payload_files())?,
        "archive":archive_identity,"native_execution":"not_run","engineering_acceptance":"not_evaluated"});
    ensure!(
        oe_core::json::strict_json(&read(&out.join("prepared/prepare-intent.json"))?)? == intent
            && oe_core::json::strict_json(&read(&out.join("prepared/prepared.json"))?)? == prepared,
        "incad_nx_bridge_preparation_authority_changed"
    );
    snapshot.current(root, request)?;
    result(&snapshot, &package, "verified")
}
