//! Freeze complete original attempts, execute their existing owners, and hand
//! controlled comparisons to kernel::learning. This is not another Project.
use crate::workflow::{Reference, WorkInput, read_reference};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, EngineeringDesignAttemptRecordRequest, EngineeringDesignAttemptRequest,
    EngineeringDesignComparisonRequest,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use oe_kernel::{
    learning::design_comparison::{self as evaluator, Side},
    project::Project,
};
use oe_knowledge::poppler::PopplerTools;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const LIMIT: usize = 16 * 1024 * 1024;
const TOTAL: usize = 64 * 1024 * 1024;

fn json(raw: &[u8]) -> Result<Value> {
    Ok(oe_core::json::strict_json(raw)?)
}
fn read(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    r.validate()
        .map_err(|_| anyhow::anyhow!("design_comparison_reference_invalid"))?;
    read_reference(
        root,
        &Reference {
            path: r.path.clone().into(),
            sha256: r.sha256.clone(),
        },
    )
}
fn path(root: &SkillRoot, p: &Path) -> Result<PathBuf> {
    let p = root.expand(p)?;
    ensure!(
        p.starts_with(root.path()),
        "design_comparison_path_outside_root"
    );
    Ok(p)
}
fn path_ref(root: &SkillRoot, p: &Path) -> Result<ArtifactReference> {
    let p = path(root, p)?;
    let b = read_regular_bytes(&p, LIMIT)?;
    Ok(ArtifactReference {
        path: oe_core::portable_source_path(root.path(), &p)?,
        sha256: sha256(&b),
    })
}
struct Inputs {
    raw: BTreeMap<String, Vec<u8>>,
    total: usize,
}
impl Inputs {
    fn new() -> Self {
        Self {
            raw: BTreeMap::new(),
            total: 0,
        }
    }
    fn get(&mut self, root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
        let raw = read(root, r)?;
        let p = path(root, Path::new(&r.path))?;
        let name = oe_core::portable_source_path(root.path(), &p)?;
        if let Some(old) = self.raw.get(&name) {
            ensure!(*old == raw, "design_comparison_source_changed");
        } else {
            self.total = self
                .total
                .checked_add(raw.len())
                .context("design_comparison_input_budget")?;
            ensure!(
                self.total <= TOTAL && self.raw.len() < 256,
                "design_comparison_input_budget"
            );
            self.raw.insert(name, raw.clone());
        }
        Ok(raw)
    }
    fn ensure_current(&self, root: &SkillRoot) -> Result<()> {
        for (p, b) in &self.raw {
            ensure!(
                read_regular_bytes(&path(root, Path::new(p))?, LIMIT)? == *b,
                "design_comparison_source_changed"
            );
        }
        Ok(())
    }
    fn manifest(&self) -> Value {
        json!(
            self.raw
                .iter()
                .map(|(path, b)| json!({"path":path,"sha256":sha256(b),"bytes":b.len()}))
                .collect::<Vec<_>>()
        )
    }
}
struct Attempt {
    request: EngineeringDesignAttemptRecordRequest,
    work: WorkInput,
    prepared: crate::design_attempt::Prepared,
    attempt: EngineeringDesignAttemptRequest,
}
impl Attempt {
    fn side(&self, execution: &'static str) -> Side<'_> {
        Side {
            object: &self.work.object,
            calculations: &self.work.calculations,
            execution,
        }
    }
}
struct Checked {
    request: EngineeringDesignComparisonRequest,
    parent: WorkInput,
    project: Project,
    baseline: Attempt,
    candidate: Attempt,
    inputs: Inputs,
}

fn attempt(root: &SkillRoot, r: &ArtifactReference, inputs: &mut Inputs) -> Result<Attempt> {
    let request: EngineeringDesignAttemptRecordRequest =
        serde_json::from_value(json(&inputs.get(root, r)?)?)?;
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("design_comparison_attempt_invalid"))?;
    ensure!(
        request.schema == "oev2.design-attempt-record/1",
        "design_comparison_attempt_invalid"
    );
    let attempt: EngineeringDesignAttemptRequest =
        serde_json::from_value(json(&inputs.get(root, &request.attempt)?)?)?;
    crate::design_attempt::verify_preparation(root, &attempt, Path::new(&request.prepared))?;
    let terminal = path_ref(root, &Path::new(&request.prepared).join("completed.json"))?;
    let terminal_value = json(&inputs.get(root, &terminal)?)?;
    ensure!(
        terminal_value["input"] == json!(request.attempt),
        "design_comparison_attempt_preparation_mismatch"
    );
    let p = crate::design_attempt::prepare(root, &attempt)?;
    let work: WorkInput = serde_json::from_value(json(&p.work_bytes)?)?;
    let incad_raw = inputs.get(root, &attempt.incad_request)?;
    let incad: oe_contracts::IncadWorkPreparationRequest =
        serde_json::from_value(json(&incad_raw)?)?;
    inputs.get(root, &incad.work)?;
    inputs.get(root, &attempt.independent_intent)?;
    for artifact in &request.artifacts {
        inputs.get(root, &artifact.artifact)?;
    }
    if let Some(review) = &request.review {
        inputs.get(root, &review.binding)?;
        inputs.get(root, &review.evidence)?;
        if let Some(prior) = &review.prior {
            inputs.get(root, prior)?;
        }
    }
    Ok(Attempt {
        request,
        work,
        prepared: p,
        attempt,
    })
}
fn checked(root: &SkillRoot, input: &ArtifactReference) -> Result<Checked> {
    let mut inputs = Inputs::new();
    let request: EngineeringDesignComparisonRequest =
        serde_json::from_value(json(&inputs.get(root, input)?)?)?;
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("design_comparison_contract_invalid"))?;
    let parent_raw = inputs.get(root, &request.parent_work)?;
    let parent: WorkInput = serde_json::from_value(json(&parent_raw)?)?;
    ensure!(
        parent.schema == "oev2.engineering-work/3",
        "design_comparison_parent_work_version"
    );
    let project_raw = inputs.get(
        root,
        &ArtifactReference {
            path: parent.project.path.to_string_lossy().into(),
            sha256: parent.project.sha256.clone(),
        },
    )?;
    let project: Project = serde_json::from_value(json(&project_raw)?)?;
    crate::workflow::verify_project_binding(&project, &parent)?;
    let baseline = attempt(root, &request.baseline, &mut inputs)?;
    let candidate = attempt(root, &request.candidate, &mut inputs)?;
    // The existing attempt owner admits at most 32 artifacts. Keep every
    // original candidate artifact and reserve one slot for the actual result.
    ensure!(
        candidate.request.artifacts.len() <= 31,
        "design_comparison_feedback_capacity"
    );
    for side in [&baseline, &candidate] {
        ensure!(
            side.work.schema == "oev2.engineering-work/3"
                && side.prepared.project_bytes == project_raw
                && json!(side.work.project) == json!(parent.project),
            "design_comparison_other_project"
        );
        crate::workflow::verify_project_binding(&project, &side.work)?;
        ensure!(
            side.work.acceptance == parent.acceptance,
            "design_comparison_acceptance_changed"
        );
    }
    // A comparison of design variables does not also change retrieval, missing
    // obligations or exposure. Each distinct method scope needs a new contract.
    ensure!(
        json!(baseline.work.source_requests) == json!(candidate.work.source_requests)
            && json!(baseline.work.gaps) == json!(candidate.work.gaps)
            && baseline.work.necessary_functions == candidate.work.necessary_functions
            && baseline.work.physical_mechanisms == candidate.work.physical_mechanisms,
        "design_comparison_uncontrolled_work_change"
    );
    ensure!(
        json!(baseline.attempt.reference_exposure) == json!(candidate.attempt.reference_exposure)
            && json!(baseline.request.reference_exposure)
                == json!(candidate.request.reference_exposure),
        "design_comparison_exposure_mismatch"
    );
    evaluator::validate(
        root,
        &project,
        &parent.object,
        &parent.acceptance,
        &request,
        &baseline.side("not_run"),
        &candidate.side("not_run"),
    )?;
    baseline.prepared.ensure_current(root)?;
    candidate.prepared.ensure_current(root)?;
    inputs.ensure_current(root)?;
    Ok(Checked {
        request,
        parent,
        project,
        baseline,
        candidate,
        inputs,
    })
}

fn stable(c: &Checked) -> Value {
    json!({"schema":"oev2.design-comparison-frozen/1","request":c.request,
        "project":c.project,"parent_work":c.parent,"baseline":c.baseline.prepared.result,
        "candidate":c.candidate.prepared.result,"original_sources":c.inputs.manifest(),
        "evaluator":"oe-kernel::learning::design_comparison",
        "executable_sha256":oe_kernel::executable_digest().ok(),
        "evaluation_scope":"development_replay","design_complete":false,"adopted":false})
}
fn inventory(root: &SkillRoot, dir: &Path) -> Result<Value> {
    let mut total = 0usize;
    let mut files = BTreeMap::new();
    for item in std::fs::read_dir(dir)? {
        let p = item?.path();
        if p.is_dir() {
            continue;
        }
        let b = read_regular_bytes(&path(root, &p)?, LIMIT)?;
        total = total
            .checked_add(b.len())
            .context("design_comparison_storage_budget")?;
        ensure!(
            total <= TOTAL && files.len() < 256,
            "design_comparison_storage_budget"
        );
        files.insert(
            p.file_name()
                .context("design_comparison_filename")?
                .to_string_lossy()
                .into_owned(),
            sha256(&b),
        );
    }
    Ok(json!(files))
}

pub fn freeze(root: &SkillRoot, input: &ArtifactReference, output: &Path) -> Result<Value> {
    let c = checked(root, input)?;
    let value = stable(&c);
    ensure!(
        value["executable_sha256"].is_string(),
        "design_comparison_executable_unavailable"
    );
    let dir = root.new_private_dir(output)?;
    write_new_bytes(&dir.join("request.json"), &read(root, input)?)?;
    let mut sources = Vec::new();
    for (i, (name, bytes)) in c.inputs.raw.iter().enumerate() {
        let stored = format!("source-{i:03}.bin");
        write_new_bytes(&dir.join(&stored), bytes)?;
        sources.push(
            json!({"source":name,"stored":stored,"sha256":sha256(bytes),"bytes":bytes.len()}),
        );
    }
    write_new(&dir.join("sources.json"), &json!(sources))?;
    write_new(&dir.join("freeze.json"), &value)?;
    let again = checked(root, input)?;
    ensure!(
        stable(&again) == value,
        "design_comparison_changed_during_freeze"
    );
    let files = inventory(root, &dir)?;
    let completed = json!({"schema":"oev2.design-comparison-freeze-completed/1","status":"frozen","input":input,
        "files":files,"executable_sha256":oe_kernel::executable_digest()?,"design_complete":false,"adopted":false});
    write_new(&dir.join("completed.json"), &completed)?;
    Ok(value)
}

fn frozen(root: &SkillRoot, reference: &ArtifactReference) -> Result<(Checked, PathBuf, Value)> {
    let terminal = json(&read(root, reference)?)?;
    ensure!(
        terminal["schema"] == "oev2.design-comparison-freeze-completed/1"
            && terminal["status"] == "frozen",
        "design_comparison_freeze_terminal_invalid"
    );
    ensure!(
        terminal["executable_sha256"] == oe_kernel::executable_digest()?,
        "design_comparison_evaluator_changed"
    );
    let directory = path(root, Path::new(&reference.path))?
        .parent()
        .context("design_comparison_frozen_parent")?
        .to_owned();
    let expected = terminal["files"]
        .as_object()
        .context("design_comparison_inventory_missing")?;
    ensure!(
        expected.contains_key("freeze.json")
            && expected.contains_key("request.json")
            && expected.contains_key("sources.json")
            && expected.len() <= 256,
        "design_comparison_inventory_invalid"
    );
    let mut total = 0usize;
    for (name, hash) in expected {
        ensure!(
            !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..",
            "design_comparison_inventory_path"
        );
        let raw = read_regular_bytes(&directory.join(name), LIMIT)?;
        total = total
            .checked_add(raw.len())
            .context("design_comparison_storage_budget")?;
        ensure!(
            total <= TOTAL && hash == &json!(sha256(&raw)),
            "design_comparison_frozen_changed"
        );
    }
    let input: ArtifactReference = serde_json::from_value(terminal["input"].clone())?;
    ensure!(
        read_regular_bytes(&directory.join("request.json"), LIMIT)? == read(root, &input)?,
        "design_comparison_original_input_changed"
    );
    let c = checked(root, &input)?;
    let stored = json(&read_regular_bytes(&directory.join("freeze.json"), LIMIT)?)?;
    ensure!(
        stable(&c) == stored,
        "design_comparison_frozen_binding_changed"
    );
    let sources = json(&read_regular_bytes(&directory.join("sources.json"), LIMIT)?)?;
    let rows = sources
        .as_array()
        .context("design_comparison_sources_invalid")?;
    ensure!(
        rows.len() == c.inputs.raw.len(),
        "design_comparison_sources_changed"
    );
    for (i, ((name, bytes), row)) in c.inputs.raw.iter().zip(rows).enumerate() {
        let stored = format!("source-{i:03}.bin");
        ensure!(
            row == &json!({"source":name,"stored":stored,"sha256":sha256(bytes),"bytes":bytes.len()})
                && read_regular_bytes(&directory.join(stored), LIMIT)? == *bytes,
            "design_comparison_source_copy_changed"
        );
    }
    Ok((c, directory, terminal))
}
pub fn verify(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    let (c, _, _) = frozen(root, reference)?;
    Ok(
        json!({"schema":"oev2.design-comparison-revalidation/1","status":"current_frozen_comparison_verified",
        "comparison_id":c.request.comparison_id,"execution":"not_run","project_mutated":false}),
    )
}

fn execute(root: &SkillRoot, a: &Attempt, out: &Path, tools: &PopplerTools) -> Value {
    match crate::design_attempt::record(root, &a.request, out, tools) {
        Ok(value) => json!({"execution":"completed","record":value,"reason":null}),
        Err(error) => json!({"execution":"failed","record":null,"reason":error.to_string()}),
    }
}
fn side_execution(result: &Value) -> &'static str {
    if result["execution"] == "completed" && result["record"]["work"]["execution"] != "failed" {
        "completed"
    } else {
        "failed"
    }
}

pub fn evaluate(
    root: &SkillRoot,
    reference: &ArtifactReference,
    output: &Path,
    tools: &PopplerTools,
) -> Result<Value> {
    let (c, _, _) = frozen(root, reference)?;
    let dir = root.new_private_dir(output)?;
    write_new(&dir.join("frozen-reference.json"), &json!(reference))?;
    let baseline = execute(root, &c.baseline, &dir.join("baseline"), tools);
    let candidate = execute(root, &c.candidate, &dir.join("candidate"), tools);
    // Recompute under the same original model owner. Neither the returned work
    // JSON nor any imported author's success flag can supply a score.
    let assessment = evaluator::assess(
        root,
        &c.project,
        &c.parent.object,
        &c.parent.acceptance,
        &c.request,
        &c.baseline.side(side_execution(&baseline)),
        &c.candidate.side(side_execution(&candidate)),
    )?;
    let remaining: Vec<_> = c
        .parent
        .acceptance
        .iter()
        .map(|a| json!({"acceptance":a,"engineering_acceptance":"not_established"}))
        .collect();
    let result = json!({"schema":"oev2.design-comparison-result/1","comparison_id":c.request.comparison_id,
        "frozen":reference,"parent_work":c.request.parent_work,"project_reference":c.parent.project,
        "baseline":baseline,"candidate":candidate,"assessment":assessment,"remaining_acceptance":remaining,
        "reference_exposure":{"scope":c.request.exposure,"source_family":c.request.source_family,
            "baseline":{"frozen":c.baseline.attempt.reference_exposure,"subsequent":c.baseline.request.reference_exposure},
            "candidate":{"frozen":c.candidate.attempt.reference_exposure,"subsequent":c.candidate.request.reference_exposure}},
        "cost":{"status":"unknown","candidate_is_cheaper":false},"model_execution":"not_run","native_execution":"not_run",
        "design_complete":false,"adopted":false,"project_mutated":false,"engineering_conclusion":"unknown",
        "method_promotion":"not_performed","executable_sha256":oe_kernel::executable_digest()?});
    write_new(&dir.join("result.json"), &result)?;
    let result_ref = path_ref(root, &dir.join("result.json"))?;
    let feedback = json!({"schema":"oev2.design-comparison-feedback/1","comparison_id":c.request.comparison_id,
        "project_reference":c.parent.project,"parent_object":c.parent.object,"result":result_ref,
        "verdict":assessment["verdict"],"selection":assessment["selection"],"counts":assessment["counts"],
        "observed_improvements":assessment["observed_improvements"],"cost":assessment["cost"],
        "authority":"current Rust comparison of frozen declared models; this is not model training or engineering adoption",
        "method_promotion":"not_performed","next_check":"Read complete model/boundary/unknown results, then use existing Harness/experience with a separately frozen method hypothesis; no current retrieval protocol accepts this as a promoted plan"});
    write_new(&dir.join("feedback.json"), &feedback)?;
    let mut continuation = c.candidate.request.clone();
    let step = c
        .candidate
        .attempt
        .tool_plan
        .iter()
        .find(|s| s.activity == "feedback")
        .context("design_comparison_feedback_step_missing")?;
    continuation
        .artifacts
        .push(oe_contracts::DesignAttemptArtifact {
            step_id: step.step_id.clone(),
            role: "feedback".into(),
            artifact: path_ref(root, &dir.join("feedback.json"))?,
            source_root: None,
        });
    continuation.notes.push("Retain actual design comparison as feedback; original method/engineering adoption authorities unchanged.".into());
    write_new(
        &dir.join("candidate-feedback-record-request.json"),
        &json!(continuation),
    )?;
    let mut children = BTreeMap::new();
    for (name, v) in [("baseline", &baseline), ("candidate", &candidate)] {
        if v["execution"] == "completed" {
            let bytes = read_regular_bytes(&dir.join(name).join("completed.json"), LIMIT)?;
            children.insert(
                name,
                json!({"path":format!("{name}/completed.json"),"sha256":sha256(bytes)}),
            );
        } else {
            children.insert(
                name,
                json!({"execution":"failed","partial_outputs":"retained; not a completed attempt"}),
            );
        }
    }
    let _ = frozen(root, reference)?;
    write_new(
        &dir.join("completed.json"),
        &json!({"schema":"oev2.design-comparison-completed/1","status":"evaluated",
        "frozen":reference,"files":inventory(root,&dir)?,"children":children,"executable_sha256":oe_kernel::executable_digest()?,
        "design_complete":false,"adopted":false,"project_mutated":false}),
    )?;
    Ok(result)
}
