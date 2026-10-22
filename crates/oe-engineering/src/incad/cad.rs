//! Current Project source preparation and append-only staging of original CAD.
use super::Prepared;
use anyhow::{Context, Result, ensure};
use oe_contracts::IncadCadSelectionRequest;
use oe_core::{
    SkillRoot, ensure_directory, read_regular_bytes, sha256, write_new, write_new_bytes,
};
use oe_knowledge::incad::cad::VerifiedSelection;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
pub mod nx_assembly;

fn handoff(selected: &Selected, dir: &Path) -> Value {
    let inventory: Vec<_> = selected.cad.files().iter().map(|f|json!({"path":format!("payload/{}",f.relative_path),
        "source_path":f.relative_path,"identity":f.identity,"role":f.role,"supplier_option":f.format})).collect();
    let native_inputs: Vec<_> = inventory.iter().filter(|f| f["role"] == "archive_member").map(|f|json!({
        "source":dir.join(f["path"].as_str().unwrap()),"relative_source":f["path"],
        "identity":f["identity"],"supplier_option":f["supplier_option"],
        "native_parameters":"must be independently authored for the existing NX or SW task contract"})).collect();
    json!({"schema":"oev2.incad-cad-source-handoff/1","files":inventory,"native_input_candidates":native_inputs,
        "project":selected.result["preparation"]["project"],"object":selected.result["preparation"]["object"],
        "work_sha256":sha256(&selected.preparation.work_bytes),"reference_exposure":"development_replay",
        "source_integrity":"verified_selected_originals_and_zip_members","assembly_reference_closure":"not_native_verified",
        "units":"native_reader_required","native_execution":"not_run","adopted":false})
}

fn verify_output_inventory(dir: &Path, selected: &Selected) -> Result<()> {
    let mut expected: BTreeSet<_> = [
        "request.json",
        "work.json",
        "project.json",
        "preparation.json",
        "handoff.json",
        "completed.json",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for f in selected.cad.files() {
        expected.insert(format!("payload/{}", f.relative_path));
    }
    let optional = [
        "cli-input.json",
        "execution-binding.json",
        "execution-recheck.json",
        "cli-completed.json",
    ];
    let mut queue = vec![dir.to_owned()];
    let mut count = 0usize;
    let mut total = 0u64;
    while let Some(folder) = queue.pop() {
        oe_core::no_symlinks(&folder)?;
        for entry in std::fs::read_dir(&folder)? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            ensure!(!metadata.file_type().is_symlink(), "incad_cad_staged_link");
            count += 1;
            ensure!(count <= 8192, "incad_cad_staged_inventory_budget");
            if metadata.is_dir() {
                queue.push(path);
                continue;
            }
            ensure!(metadata.is_file(), "incad_cad_staged_nonregular");
            let name = oe_core::portable_source_path(dir, &path)?;
            ensure!(
                expected.remove(&name) || optional.contains(&name.as_str()),
                "incad_cad_staged_unexpected_file:{name}"
            );
            total = total
                .checked_add(metadata.len())
                .context("incad_cad_staged_size_overflow")?;
            ensure!(
                metadata.len() <= 16 * 1024 * 1024 && total <= 64 * 1024 * 1024,
                "incad_cad_staged_byte_budget"
            );
        }
    }
    ensure!(expected.is_empty(), "incad_cad_staged_missing_file");
    Ok(())
}

pub struct Selected {
    pub preparation: Prepared,
    pub cad: VerifiedSelection,
    pub result: Value,
}
impl Selected {
    pub fn ensure_current(&self, root: &SkillRoot) -> Result<()> {
        self.preparation.ensure_current(root)?;
        self.cad.ensure_current()
    }
}
pub fn prepare(root: &SkillRoot, request: &IncadCadSelectionRequest) -> Result<Selected> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("incad_cad_selection_contract_invalid"))?;
    ensure!(
        request.schema == "oev2.incad-cad-selection/1",
        "incad_cad_selection_schema"
    );
    let preparation = super::prepare(root, &request.preparation)?;
    let cad = oe_knowledge::incad::cad::verify(&preparation.intake, &request.formats)?;
    let project: oe_kernel::project::Project =
        serde_json::from_value(oe_core::json::strict_json(&preparation.project_bytes)?)?;
    let ids = request
        .source_ids
        .as_object()
        .context("incad_cad_project_source_ids_missing")?;
    ensure!(
        ids.len() == cad.files().len(),
        "incad_cad_source_mapping_incomplete"
    );
    let mut unique = BTreeSet::new();
    for f in cad.files() {
        let id = ids
            .get(&f.relative_path)
            .and_then(Value::as_str)
            .context("incad_cad_source_id_missing")?;
        ensure!(
            !id.is_empty()
                && unique.insert(id)
                && project.evidence.get(id) == Some(&f.identity.sha256),
            "incad_cad_source_not_bound_to_current_project:{}",
            f.relative_path
        );
    }
    let result = json!({"schema":"oev2.incad-cad-prepared/1","request":request,
        "preparation":preparation.result,"cad":cad.audit(),"current_project_bound":true,
        "native_execution":"not_run","engineering_applicability":"not_reviewed","adopted":false});
    let selected = Selected {
        preparation,
        cad,
        result,
    };
    selected.ensure_current(root)?;
    Ok(selected)
}

pub fn stage(root: &SkillRoot, request: &IncadCadSelectionRequest, output: &Path) -> Result<Value> {
    let selected = prepare(root, request)?;
    let dir = root.new_private_dir(output)?;
    for f in selected.cad.files() {
        let relative = format!("payload/{}", f.relative_path);
        let destination = dir.join(&relative);
        ensure_directory(destination.parent().context("incad_cad_output_parent")?)?;
        write_new_bytes(&destination, &f.bytes)?;
        ensure!(
            read_regular_bytes(&destination, 16 * 1024 * 1024)? == f.bytes,
            "incad_cad_staged_bytes_differ"
        );
    }
    write_new(&dir.join("request.json"), &serde_json::to_value(request)?)?;
    write_new_bytes(&dir.join("work.json"), &selected.preparation.work_bytes)?;
    write_new_bytes(
        &dir.join("project.json"),
        &selected.preparation.project_bytes,
    )?;
    write_new(&dir.join("preparation.json"), &selected.result)?;
    let handoff = handoff(&selected, &dir);
    write_new(&dir.join("handoff.json"), &handoff)?;
    selected.ensure_current(root)?;
    let completed = json!({"schema":"oev2.incad-cad-stage-completed/1","status":"staged",
        "request_sha256":sha256(oe_core::json::canonical_json(&serde_json::to_value(request)?)?),
        "preparation_sha256":sha256(read_regular_bytes(&dir.join("preparation.json"),16*1024*1024)?),
        "handoff_sha256":sha256(read_regular_bytes(&dir.join("handoff.json"),16*1024*1024)?),
        "file_count":selected.cad.files().len(),"native_execution":"not_run","adopted":false});
    write_new(&dir.join("completed.json"), &completed)?;
    verify_staged(root, request, output)?;
    Ok(completed)
}

pub fn verify_staged(
    root: &SkillRoot,
    request: &IncadCadSelectionRequest,
    output: &Path,
) -> Result<Value> {
    let selected = prepare(root, request)?;
    let dir = root.private_path(output)?;
    let read =
        |name: &str| -> Result<Vec<u8>> { read_regular_bytes(&dir.join(name), 16 * 1024 * 1024) };
    let completed = oe_core::json::strict_json(&read("completed.json")?)?;
    ensure!(
        completed["schema"] == "oev2.incad-cad-stage-completed/1"
            && completed["status"] == "staged",
        "incad_cad_stage_not_completed"
    );
    ensure!(
        completed.as_object().is_some_and(|v| v.len() == 8)
            && completed["native_execution"] == "not_run"
            && completed["adopted"] == false,
        "incad_cad_stage_authority_changed"
    );
    let normalized_request = oe_core::json::canonical_json(&serde_json::to_value(request)?)?;
    ensure!(
        completed["request_sha256"] == sha256(normalized_request)
            && oe_core::json::strict_json(&read("request.json")?)?
                == serde_json::to_value(request)?,
        "incad_cad_staged_request_changed"
    );
    ensure!(
        read("work.json")? == selected.preparation.work_bytes
            && read("project.json")? == selected.preparation.project_bytes,
        "incad_cad_staged_project_or_work_changed"
    );
    let raw = read("preparation.json")?;
    ensure!(
        completed["preparation_sha256"] == sha256(&raw)
            && oe_core::json::strict_json(&raw)? == selected.result,
        "incad_cad_staged_preparation_changed"
    );
    let handoff_raw = read("handoff.json")?;
    ensure!(
        completed["handoff_sha256"] == sha256(&handoff_raw),
        "incad_cad_staged_handoff_changed"
    );
    let handoff = oe_core::json::strict_json(&handoff_raw)?;
    ensure!(
        handoff == self::handoff(&selected, &dir),
        "incad_cad_staged_handoff_binding_changed"
    );
    let rows = handoff["files"]
        .as_array()
        .context("incad_cad_staged_files_missing")?;
    ensure!(
        rows.len() == selected.cad.files().len()
            && completed["file_count"].as_u64() == Some(rows.len() as u64),
        "incad_cad_staged_inventory_incomplete"
    );
    for (row, f) in rows.iter().zip(selected.cad.files()) {
        let relative = format!("payload/{}", f.relative_path);
        ensure!(
            row["path"] == relative
                && row["source_path"] == f.relative_path
                && row["identity"] == serde_json::to_value(&f.identity)?
                && row["role"] == f.role
                && row["supplier_option"] == f.format,
            "incad_cad_staged_file_binding_changed"
        );
        ensure!(
            read(&relative)? == f.bytes,
            "incad_cad_staged_payload_changed"
        );
    }
    verify_output_inventory(&dir, &selected)?;
    selected.ensure_current(root)?;
    Ok(
        json!({"schema":"oev2.incad-cad-stage-revalidation/1","status":"current_selected_cad_sources_verified",
        "file_count":rows.len(),"native_execution":"not_run","engineering_applicability":"not_reviewed","adopted":false}),
    )
}
