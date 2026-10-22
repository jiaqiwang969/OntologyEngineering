//! A frozen design attempt over existing engineering owners; never a Project or evidence registry.
use crate::workflow::{Reference, WorkInput, read_reference};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, DesignAttemptExposure, DesignAttemptReview,
    EngineeringDesignAttemptRecordRequest, EngineeringDesignAttemptRequest,
    EngineeringDesignIntent,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use oe_kernel::project::Project;
use oe_knowledge::poppler::PopplerTools;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const LIMIT: usize = 16 * 1024 * 1024;
const TOTAL: usize = 64 * 1024 * 1024;
pub mod cad_binding;
fn generated(r: std::result::Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|_| anyhow::anyhow!("design_attempt_generated_contract_invalid"))
}
fn text(s: &str) -> bool {
    !s.trim().is_empty()
}
fn strings(s: &[String]) -> bool {
    s.iter().all(|v| text(v))
}
fn reference(r: &ArtifactReference) -> Reference {
    Reference {
        path: r.path.clone().into(),
        sha256: r.sha256.clone(),
    }
}
fn read(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    generated(r.validate())?;
    read_reference(root, &reference(r))
}
fn json(bytes: &[u8]) -> Result<Value> {
    Ok(oe_core::json::strict_json(bytes)?)
}
fn local(root: &SkillRoot, p: &Path) -> Result<PathBuf> {
    let p = root.expand(p)?;
    ensure!(
        p.starts_with(root.path()),
        "design_attempt_path_outside_root"
    );
    Ok(p)
}
fn read_path(root: &SkillRoot, p: &Path) -> Result<Vec<u8>> {
    read_regular_bytes(&local(root, p)?, LIMIT)
}
fn exposure(rows: &[DesignAttemptExposure], project: &Project, preparing: bool) -> Result<()> {
    ensure!(
        !rows.is_empty() && rows.len() <= 64,
        "design_attempt_exposure_missing_or_budget"
    );
    for row in rows {
        generated(row.validate())?;
        ensure!(
            text(&row.basis) && (!preparing || row.stage != "after_freeze"),
            "design_attempt_exposure_invalid"
        );
        ensure!(
            strings(&row.source_refs)
                && row
                    .source_refs
                    .iter()
                    .all(|id| project.evidence.contains_key(id)),
            "design_attempt_exposure_source_unbound"
        );
    }
    Ok(())
}

pub struct Prepared {
    pub result: Value,
    pub work_bytes: Vec<u8>,
    pub project_bytes: Vec<u8>,
    pub intent_bytes: Vec<u8>,
    incad: crate::incad::Prepared,
    snapshots: Vec<(Reference, Vec<u8>)>,
}
impl Prepared {
    pub fn ensure_current(&self, root: &SkillRoot) -> Result<()> {
        self.incad.ensure_current(root)?;
        for (r, raw) in &self.snapshots {
            ensure!(
                read_reference(root, r)? == *raw,
                "design_attempt_source_changed"
            );
        }
        let work: WorkInput = serde_json::from_value(json(&self.work_bytes)?)?;
        let project: Project = serde_json::from_value(json(&self.project_bytes)?)?;
        ensure!(
            json!(oe_kernel::engineering::checks_current(
                root,
                &project,
                &work.object
            )) == self.result["engineering_checks"],
            "design_attempt_engineering_source_changed"
        );
        Ok(())
    }
}
fn snapshot(root: &SkillRoot, p: &Path, s: &mut Vec<(Reference, Vec<u8>)>) -> Result<Vec<u8>> {
    let bytes = read_path(root, p)?;
    ensure!(
        s.iter()
            .map(|(_, b)| b.len())
            .sum::<usize>()
            .checked_add(bytes.len())
            .is_some_and(|n| n <= TOTAL),
        "design_attempt_source_budget"
    );
    s.push((
        Reference {
            path: p.into(),
            sha256: sha256(&bytes),
        },
        bytes.clone(),
    ));
    Ok(bytes)
}
/// Recheck the existing source preparation rather than trusting its terminal flag.
pub fn prepare(root: &SkillRoot, request: &EngineeringDesignAttemptRequest) -> Result<Prepared> {
    generated(request.validate())?;
    ensure!(
        request.schema == "oev2.design-attempt/1" && text(&request.attempt_id),
        "design_attempt_request_invalid"
    );
    let mut snapshots = Vec::new();
    let raw = snapshot(root, Path::new(&request.incad_request.path), &mut snapshots)?;
    ensure!(
        sha256(&raw) == request.incad_request.sha256,
        "design_attempt_incad_request_changed"
    );
    let incad_request: oe_contracts::IncadWorkPreparationRequest =
        serde_json::from_value(json(&raw)?)?;
    let incad = crate::incad::prepare(root, &incad_request)?;
    let work: WorkInput = serde_json::from_value(json(&incad.work_bytes)?)?;
    let project_bytes = snapshot(root, &work.project.path, &mut snapshots)?;
    ensure!(
        sha256(&project_bytes) == work.project.sha256,
        "design_attempt_project_changed"
    );
    let project: Project = serde_json::from_value(json(&project_bytes)?)?;
    let intent_bytes = snapshot(
        root,
        Path::new(&request.independent_intent.path),
        &mut snapshots,
    )?;
    ensure!(
        sha256(&intent_bytes) == request.independent_intent.sha256,
        "design_attempt_intent_changed"
    );
    let intent: EngineeringDesignIntent = serde_json::from_value(json(&intent_bytes)?)?;
    generated(intent.validate())?;
    ensure!(
        intent.schema == "oev2.design-intent/1"
            && intent.object == work.object
            && text(&intent.statement),
        "design_attempt_independent_intent_invalid"
    );
    ensure!(
        !intent.source_refs.is_empty()
            && strings(&intent.source_refs)
            && intent
                .source_refs
                .iter()
                .all(|id| project.evidence.contains_key(id)),
        "design_attempt_intent_source_unbound"
    );
    ensure!(
        strings(&intent.alternatives) && strings(&intent.unknowns),
        "design_attempt_intent_empty_item"
    );
    ensure!(
        !request.tool_plan.is_empty() && request.tool_plan.len() <= 32,
        "design_attempt_tool_plan_budget"
    );
    let mut ids = BTreeSet::new();
    for step in &request.tool_plan {
        generated(step.validate())?;
        ensure!(
            text(&step.step_id)
                && ids.insert(&step.step_id)
                && !step.acceptance.is_empty()
                && !step.required_evidence.is_empty()
                && strings(&step.required_evidence),
            "design_attempt_tool_step_invalid"
        );
        ensure!(
            step.acceptance.iter().all(|a| work.acceptance.contains(a)),
            "design_attempt_acceptance_changed"
        );
    }
    ensure!(
        work.acceptance
            .iter()
            .all(|a| request.tool_plan.iter().any(|s| s.acceptance.contains(a))),
        "design_attempt_acceptance_unplanned"
    );
    for activity in [
        "work",
        "harness",
        "cad_evidence",
        "handoff",
        "kernel_review",
        "feedback",
    ] {
        ensure!(
            request.tool_plan.iter().any(|s| s.activity == activity),
            "design_attempt_required_step_missing:{activity}"
        );
    }
    exposure(&request.reference_exposure, &project, true)?;
    let dir = Path::new(&request.incad_prepared);
    let terminal = json(&snapshot(
        root,
        &dir.join("completed.json"),
        &mut snapshots,
    )?)?;
    ensure!(
        terminal["status"] == "prepared" && terminal["work_sha256"] == sha256(&incad.work_bytes),
        "design_attempt_incad_not_prepared"
    );
    let original = json(&snapshot(
        root,
        &dir.join("preparation.json"),
        &mut snapshots,
    )?)?;
    crate::incad::verify_preparation(root, &incad_request, &original)?;
    for (name, bytes) in [
        ("request.json", raw.as_slice()),
        ("work.json", incad.work_bytes.as_slice()),
        ("specifications.json", incad.intake.specifications_bytes()),
        ("zero-design-task.json", incad.intake.task_bytes()),
    ] {
        ensure!(
            snapshot(root, &dir.join(name), &mut snapshots)? == bytes,
            "design_attempt_incad_artifact_changed:{name}"
        );
    }
    let condition = project
        .conditions
        .get(&work.object.condition_scope)
        .context("design_attempt_condition_missing")?;
    let basis = json(condition.as_bytes())?;
    let engineering_checks = oe_kernel::engineering::checks_current(root, &project, &work.object);
    let result = json!({"schema":"oev2.design-attempt-prepared/1","request":request,"project":project,"project_context":work.project_context,"object":work.object,
        "work":json(&incad.work_bytes)?,"basis":basis,"condition_text":condition,"engineering_checks":engineering_checks,
        "incad":incad.result,"intent":json(&intent_bytes)?,"intent_authority":"independent author declaration frozen before this record; not measured geometry or proof of prior non-exposure",
        "tool_plan":request.tool_plan,"reference_exposure":request.reference_exposure,"evaluation_scope":"development_replay",
        "work_execution":"not_run","native_execution":"not_run","model_execution":"not_run","engineering_conclusion":"unknown","design_complete":false,"adopted":false,"project_mutated":false,
        "remaining_acceptance":work.acceptance,"source_applicability":"existing engineering checks and source declarations; physical support not established",
        "task_work_goal_relation":"author intent and current work preserved; natural-language equivalence not established"});
    let p = Prepared {
        result,
        work_bytes: incad.work_bytes.clone(),
        project_bytes,
        intent_bytes,
        incad,
        snapshots,
    };
    p.ensure_current(root)?;
    Ok(p)
}
fn files(prepared: &Prepared, request_raw: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    Ok(BTreeMap::from([
        ("request.json".into(), request_raw.to_vec()),
        ("work.json".into(), prepared.work_bytes.clone()),
        ("project.json".into(), prepared.project_bytes.clone()),
        ("intent.json".into(), prepared.intent_bytes.clone()),
        ("incad-request.json".into(), prepared.snapshots[0].1.clone()),
        (
            "specifications.json".into(),
            prepared.incad.intake.specifications_bytes().to_vec(),
        ),
        (
            "zero-design-task.json".into(),
            prepared.incad.intake.task_bytes().to_vec(),
        ),
        (
            "preparation.json".into(),
            serde_json::to_vec_pretty(&prepared.result)?,
        ),
    ]))
}
pub fn prepare_to(root: &SkillRoot, input: &ArtifactReference, output: &Path) -> Result<Value> {
    let raw = read(root, input)?;
    let request: EngineeringDesignAttemptRequest = serde_json::from_value(json(&raw)?)?;
    let p = prepare(root, &request)?;
    let dir = root.new_private_dir(output)?;
    let files = files(&p, &raw)?;
    let mut pins = BTreeMap::new();
    for (name, bytes) in files {
        write_new_bytes(&dir.join(&name), &bytes)?;
        pins.insert(name, sha256(&bytes));
    }
    p.ensure_current(root)?;
    ensure!(read(root, input)? == raw, "design_attempt_request_changed");
    let terminal = json!({"schema":"oev2.design-attempt-completed/1","status":"prepared","input":input,"files":pins,"design_complete":false,"adopted":false,"project_mutated":false});
    write_new(&dir.join("completed.json"), &terminal)?;
    Ok(terminal)
}
pub fn verify_preparation(
    root: &SkillRoot,
    request: &EngineeringDesignAttemptRequest,
    directory: &Path,
) -> Result<Value> {
    let p = prepare(root, request)?;
    let terminal = json(&read_path(root, &directory.join("completed.json"))?)?;
    ensure!(
        terminal["schema"] == "oev2.design-attempt-completed/1"
            && terminal["status"] == "prepared"
            && terminal["design_complete"] == false
            && terminal["adopted"] == false
            && terminal["project_mutated"] == false,
        "design_attempt_not_prepared"
    );
    let input: ArtifactReference = serde_json::from_value(terminal["input"].clone())?;
    let raw = read(root, &input)?;
    ensure!(
        json(&raw)? == serde_json::to_value(request)?,
        "design_attempt_request_changed"
    );
    let expected = files(&p, &raw)?;
    let pins = terminal["files"]
        .as_object()
        .context("design_attempt_inventory_missing")?;
    ensure!(
        pins.len() == expected.len(),
        "design_attempt_inventory_changed"
    );
    for (name, bytes) in &expected {
        ensure!(
            pins.get(name) == Some(&json!(sha256(bytes)))
                && read_path(root, &directory.join(name))? == *bytes,
            "design_attempt_frozen_file_changed:{name}"
        );
    }
    p.ensure_current(root)?;
    Ok(
        json!({"schema":"oev2.design-attempt-revalidation/1","status":"current_design_attempt_verified","design_complete":false,"adopted":false,"project_mutated":false,"evaluation_scope":"development_replay"}),
    )
}
fn artifact_check(
    root: &SkillRoot,
    a: &oe_contracts::DesignAttemptArtifact,
    original: &Value,
    p: &Prepared,
    work: &WorkInput,
    project: &Project,
) -> Result<Value> {
    let path = local(root, Path::new(&a.artifact.path))?;
    match a.role.as_str() {
        "work_result" => {
            ensure!(
                original["schema"] == "oev2.engineering-work-result/1"
                    && original["work_input_sha256"] == sha256(&p.work_bytes)
                    && original["project_reference"] == json!(work.project)
                    && original["object"] == json!(work.object)
                    && original["acceptance"] == json!(work.acceptance),
                "design_attempt_work_result_binding_mismatch"
            );
            Ok(
                json!({"execution":"completed","status":"bound_historical_result","authority":"imported author/tool receipt not authenticated; current work is independently executed in this record"}),
            )
        }
        "harness" => {
            let loaded = crate::harness::load_step(root, &json!(a.artifact))?;
            let dir = path.parent().context("design_attempt_harness_parent")?;
            ensure!(
                read_regular_bytes(&dir.join("current-project.json"), LIMIT)? == p.project_bytes,
                "design_attempt_harness_project_mismatch"
            );
            let archived = json(&read_regular_bytes(&dir.join("input.json"), LIMIT)?)?;
            let h = &archived["harness"];
            let declared: Reference = serde_json::from_value(h["current_project"].clone())?;
            ensure!(
                read_reference(root, &declared)? == p.project_bytes
                    && h["project_context"] == json!(work.project_context)
                    && loaded["state"]["project_id"] == project.project_id
                    && loaded["state"]["object_id"] == work.object.object_id
                    && loaded["state"]["revision"] == work.object.revision,
                "design_attempt_harness_binding_mismatch"
            );
            Ok(
                json!({"execution":"completed","status":"existing_harness_archive_verified","retained":loaded,"authority":"source/archive consistency; historical model and tool identities retained, not executed by loading"}),
            )
        }
        "cad_evidence" | "handoff" => cad_artifact_check(root, a, original, work, project),
        "feedback" => Ok(
            json!({"execution":"not_run","status":"recorded","authority":"author_report_only","method_promotion":"not_performed","no_delta":"not_inferred"}),
        ),
        _ => anyhow::bail!("design_attempt_artifact_role_invalid"),
    }
}
// Both record versions use the same original CAD/source verifier. /2 adds an
// independently checked Project/work association; it never upgrades this audit.
fn cad_artifact_check(
    root: &SkillRoot,
    a: &oe_contracts::DesignAttemptArtifact,
    original: &Value,
    work: &WorkInput,
    project: &Project,
) -> Result<Value> {
    ensure!(
        original["project"]["project_id"] == project.project_id
            && original["project"]["model_revision"] == work.object.revision,
        "design_attempt_cad_project_mismatch"
    );
    let path = local(root, Path::new(&a.artifact.path))?;
    let source_root = local(
        root,
        Path::new(
            a.source_root
                .as_deref()
                .context("design_attempt_cad_source_root_missing")?,
        ),
    )?;
    let verification = match a.role.as_str() {
        "cad_evidence" => oe_cad::evidence::verify(&path, &source_root)?
            .audit()
            .clone(),
        "handoff" => oe_cad::handoff::verify(&path, &source_root)?
            .audit()
            .clone(),
        _ => anyhow::bail!("design_attempt_cad_role_invalid"),
    };
    Ok(
        json!({"execution":"completed","status":"existing_cad_source_integrity_verified","integrity":verification,"native_execution":"not_run","engineering_adoption":"not_assessed","object_condition_association":"not_established_by_legacy_packet_shape","authority":"same Project/model revision and source consistency only; native observations and original claims retain their own scope"}),
    )
}
fn run_review(
    root: &SkillRoot,
    r: &DesignAttemptReview,
    p: &Prepared,
    work: &WorkInput,
    project: &Project,
    dir: &Path,
    source_snapshots: &mut Vec<(ArtifactReference, Vec<u8>)>,
) -> Result<Value> {
    generated(r.validate())?;
    let mut get = |a: &ArtifactReference, name: &str| -> Result<Vec<u8>> {
        let b = read(root, a)?;
        write_new_bytes(&dir.join(name), &b)?;
        source_snapshots.push((a.clone(), b.clone()));
        Ok(b)
    };
    let binding_raw = get(&r.binding, "review-binding.json")?;
    let evidence = get(&r.evidence, "review-evidence.bin")?;
    let binding: oe_kernel::Binding = serde_json::from_value(json(&binding_raw)?)?;
    ensure!(
        binding.project_id == project.project_id
            && binding.object_id == work.object.object_id
            && binding.object_revision == work.object.revision
            && binding.condition_scope == work.object.condition_scope
            && binding.source_sha256 == work.object.source_sha256
            && binding.project_context == work.project_context,
        "design_attempt_review_binding_mismatch"
    );
    let prior: Option<oe_kernel::Receipt> = r
        .prior
        .as_ref()
        .map(|a| get(a, "review-prior.json").and_then(|b| Ok(serde_json::from_value(json(&b)?)?)))
        .transpose()?;
    let port = oe_kernel::port::RustSemanticPort {
        root: root.path().into(),
        binding,
        bundle: r.bundle.clone(),
        queries: r.queries.clone(),
        shapes: r.shapes.clone(),
        evidence,
    };
    let request = oe_contracts::ReviewRequest {
        object: work.object.clone(),
        evidence_sha256: sha256(&port.evidence),
        required_checks: port.binding.required_checks.iter().cloned().collect(),
    };
    let historical = port.execute(&request, prior.as_ref())?;
    let receipt = oe_kernel::project::review_current(root, project, &port.evidence, historical)?;
    let current_package = oe_semantic::load_bundle(root.path(), &r.bundle)?;
    ensure!(
        current_package.spec["package_id"] == port.binding.package_id
            && current_package.spec["package_version"] == port.binding.package_version
            && current_package.spec["sha256"] == port.binding.package_sha256,
        "design_attempt_review_package_changed"
    );
    p.ensure_current(root)?;
    Ok(serde_json::to_value(receipt)?)
}
/// Execute current engineering checks; imported products keep their original authority.
/// A terminal record proves only that this attempt was recorded, never design completion.
pub fn record(
    root: &SkillRoot,
    request: &EngineeringDesignAttemptRecordRequest,
    output: &Path,
    tools: &PopplerTools,
) -> Result<Value> {
    generated(request.validate())?;
    ensure!(
        request.schema == "oev2.design-attempt-record/1"
            && request.artifacts.len() <= 32
            && strings(&request.notes),
        "design_attempt_record_invalid"
    );
    let attempt_raw = read(root, &request.attempt)?;
    let attempt: EngineeringDesignAttemptRequest = serde_json::from_value(json(&attempt_raw)?)?;
    verify_preparation(root, &attempt, Path::new(&request.prepared))?;
    // Binding the explicit original request prevents another preparation with the same descriptive values.
    let terminal = json(&read_path(
        root,
        &Path::new(&request.prepared).join("completed.json"),
    )?)?;
    ensure!(
        terminal["input"] == json!(request.attempt),
        "design_attempt_original_request_mismatch"
    );
    let p = prepare(root, &attempt)?;
    let work: WorkInput = serde_json::from_value(json(&p.work_bytes)?)?;
    let project: Project = serde_json::from_value(json(&p.project_bytes)?)?;
    if !request.reference_exposure.is_empty() {
        exposure(&request.reference_exposure, &project, false)?;
    }
    let mut inputs = Vec::new();
    let mut unique = BTreeSet::new();
    let mut total = attempt_raw.len();
    for a in &request.artifacts {
        generated(a.validate())?;
        let activity = if a.role == "work_result" {
            "work"
        } else {
            &a.role
        };
        ensure!(
            attempt
                .tool_plan
                .iter()
                .any(|s| s.step_id == a.step_id && s.activity == activity),
            "design_attempt_artifact_step_mismatch"
        );
        ensure!(
            unique.insert((&a.step_id, &a.artifact.path, &a.artifact.sha256)),
            "design_attempt_artifact_duplicate"
        );
        ensure!(
            matches!(a.role.as_str(), "cad_evidence" | "handoff") || a.source_root.is_none(),
            "design_attempt_artifact_unused_source_root"
        );
        let raw = read(root, &a.artifact)?;
        total = total
            .checked_add(raw.len())
            .context("design_attempt_artifact_budget")?;
        ensure!(total <= TOTAL, "design_attempt_artifact_budget");
        let value = json(&raw)?;
        inputs.push((a, raw, value));
    }
    let dir = root.new_private_dir(output)?;
    write_new(
        &dir.join("record-request.json"),
        &serde_json::to_value(request)?,
    )?;
    write_new_bytes(&dir.join("attempt-request.json"), &attempt_raw)?;
    write_new_bytes(&dir.join("project.json"), &p.project_bytes)?;
    write_new_bytes(&dir.join("work-input.json"), &p.work_bytes)?;
    write_new_bytes(&dir.join("independent-intent.json"), &p.intent_bytes)?;
    write_new(&dir.join("frozen-preparation.json"), &p.result)?;
    let mut work_result = match crate::workflow::run(root, &work, tools) {
        Ok(v) => v,
        Err(e) => {
            json!({"execution":"failed","conclusion":"unknown","reason":e.to_string(),"engineering_conclusion":"unknown","adopted":false})
        }
    };
    // Match the existing CLI work input binding even when the work owner failed.
    // This is an input identity, never a replacement for its real status.
    work_result["schema"] = json!("oev2.engineering-work-result/1");
    work_result["work_input_sha256"] = json!(sha256(&p.work_bytes));
    work_result["project_reference"] = json!(work.project);
    work_result["object"] = json!(work.object);
    work_result["acceptance"] = json!(work.acceptance);
    write_new(&dir.join("work-result.json"), &work_result)?;
    let mut artifacts = Vec::new();
    let mut checks = Vec::new();
    for (i, (a, raw, original)) in inputs.iter().enumerate() {
        write_new_bytes(&dir.join(format!("artifact-{i:03}.json")), raw)?;
        let verification = match artifact_check(root, a, original, &p, &work, &project) {
            Ok(v) => v,
            Err(e) => {
                json!({"execution":"failed","conclusion":"unknown","reason":e.to_string(),"engineering_adoption":"not_established"})
            }
        };
        checks.push(verification.clone());
        artifacts.push(json!({"step_id":a.step_id,"role":a.role,"reference":a.artifact,"verification":verification,"original":original,
            "authority":if a.role=="feedback"{"author_report_only"}else if verification["execution"]=="completed"{"existing owner checked source consistency; imported execution claims are not authenticated by a hash"}else{"owner_verification_failed; imported_unverified"}}));
    }
    let mut review_sources = Vec::new();
    let review = match &request.review {
        Some(r) => match run_review(root, r, &p, &work, &project, &dir, &mut review_sources) {
            Ok(v) => v,
            Err(e) => {
                json!({"execution":"failed","conclusion":"unknown","reason":e.to_string(),"engineering_adoption":"not_established"})
            }
        },
        None => {
            json!({"execution":"not_run","conclusion":"unknown","required":"original current binding, package, evidence and complete required checks"})
        }
    };
    write_new(&dir.join("semantic-review.json"), &review)?;
    p.ensure_current(root)?;
    ensure!(
        read(root, &request.attempt)? == attempt_raw,
        "design_attempt_request_changed"
    );
    verify_preparation(root, &attempt, Path::new(&request.prepared))?;
    for ((a, raw, original), previous) in inputs.iter().zip(&checks) {
        ensure!(
            read(root, &a.artifact)? == *raw,
            "design_attempt_artifact_changed"
        );
        if previous["execution"] == "completed" {
            ensure!(
                artifact_check(root, a, original, &p, &work, &project)? == *previous,
                "design_attempt_artifact_sources_changed"
            );
        }
    }
    for (a, bytes) in review_sources {
        ensure!(
            read(root, &a)? == bytes,
            "design_attempt_review_source_changed"
        );
    }
    let remaining:Vec<_>=work.acceptance.iter().map(|a|json!({"acceptance":a,"engineering_acceptance":"not_established","planned_steps":attempt.tool_plan.iter().filter(|s|s.acceptance.contains(a)).map(|s|&s.step_id).collect::<Vec<_>>()})).collect();
    let result = json!({"schema":"oev2.design-attempt-recorded/1","status":"recorded","attempt":request.attempt,"attempt_id":attempt.attempt_id,"prepared":request.prepared,
        "project_reference":work.project,"project_context":work.project_context,"object":work.object,"work":work_result,"artifacts":artifacts,"semantic_review":review,
        "engineering_checks":p.result["engineering_checks"],"remaining_acceptance":remaining,"tool_plan":attempt.tool_plan,
        "reference_exposure":{"frozen":attempt.reference_exposure,"subsequent":request.reference_exposure},"notes":request.notes,"evaluation_scope":"development_replay",
        "design_complete":false,"adopted":false,"project_mutated":false,"native_execution":"not_run","model_execution":"not_run","engineering_conclusion":"unknown",
        "feedback_authority":"author observations are preserved; use existing Harness/experience for any evaluation/no_delta/promotion, none inferred here",
        "project_continuity":"frozen complete Project bytes remain the input; register later products through the existing Project and start a new explicitly bound attempt when it changes"});
    write_new(&dir.join("result.json"), &result)?;
    let mut inventory = BTreeMap::new();
    let mut stored = 0usize;
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let bytes = read_regular_bytes(&path, LIMIT)?;
        stored = stored
            .checked_add(bytes.len())
            .context("design_attempt_storage_budget")?;
        ensure!(stored <= TOTAL, "design_attempt_storage_budget");
        inventory.insert(
            path.file_name()
                .context("design_attempt_output_filename")?
                .to_string_lossy()
                .into_owned(),
            sha256(&bytes),
        );
    }
    p.ensure_current(root)?;
    write_new(
        &dir.join("completed.json"),
        &json!({"schema":"oev2.design-attempt-record-completed/1","status":"recorded","files":inventory,"design_complete":false,"adopted":false,"project_mutated":false}),
    )?;
    Ok(result)
}
