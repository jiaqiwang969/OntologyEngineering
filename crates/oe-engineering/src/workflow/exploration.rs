//! General inquiry uses the existing Project/work, comparison and Harness owners.
//! Frozen branches change scoped, unadopted decisions in a new Project copy.
pub mod host;
mod plan;
use super::{Reference, WorkInput, read_reference, verify_project_binding};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CurrentTaskViewInput, EngineeringExplorationPort,
    EngineeringExplorationRunRequest, EngineeringExplorationRunResult, FailureRecord,
};
use oe_core::{SkillRoot, json::strict_json, sha256, write_new, write_new_bytes};
use oe_kernel::{
    engineering,
    project::{self, Project},
};
use plan::{
    EngineeringExplorationAction, EngineeringExplorationOutcome, EngineeringExplorationPlan,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Instant,
};

const LIMIT: usize = 16 * 1024 * 1024;
const TOTAL: usize = 64 * 1024 * 1024;
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn stated(v: &[String]) -> bool {
    !v.is_empty() && v.iter().all(|s| nonempty(s))
}
fn dto(r: std::result::Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|_| anyhow::anyhow!("exploration_generated_contract_invalid"))
}
fn read(root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
    dto(r.validate())?;
    read_reference(
        root,
        &Reference {
            path: r.path.clone().into(),
            sha256: r.sha256.clone(),
        },
    )
}
fn reference(root: &SkillRoot, p: &Path) -> Result<ArtifactReference> {
    Ok(ArtifactReference {
        path: oe_core::portable_source_path(root.path(), p)?,
        sha256: sha256(oe_core::read_regular_bytes(p, LIMIT)?),
    })
}
struct Inputs {
    bytes: BTreeMap<String, Vec<u8>>,
    total: usize,
}
impl Inputs {
    fn new() -> Self {
        Self {
            bytes: BTreeMap::new(),
            total: 0,
        }
    }
    fn get(&mut self, root: &SkillRoot, r: &ArtifactReference) -> Result<Vec<u8>> {
        let bytes = read(root, r)?;
        let name = oe_core::portable_source_path(root.path(), &root.expand(Path::new(&r.path))?)?;
        if let Some(old) = self.bytes.get(&name) {
            ensure!(*old == bytes, "exploration_input_changed");
        } else {
            self.total = self
                .total
                .checked_add(bytes.len())
                .context("exploration_input_budget")?;
            ensure!(
                self.total <= TOTAL && self.bytes.len() < 128,
                "exploration_input_budget"
            );
            self.bytes.insert(name, bytes.clone());
        }
        Ok(bytes)
    }
    fn evidence(
        &mut self,
        root: &SkillRoot,
        project: &Project,
        r: &ArtifactReference,
    ) -> Result<()> {
        ensure!(
            project.evidence.values().any(|s| s == &r.sha256),
            "exploration_source_not_project_evidence"
        );
        self.get(root, r)?;
        Ok(())
    }
    fn current(&self, root: &SkillRoot) -> Result<()> {
        for (p, b) in &self.bytes {
            ensure!(
                read(
                    root,
                    &ArtifactReference {
                        path: p.clone(),
                        sha256: sha256(b)
                    }
                )? == *b,
                "exploration_input_changed"
            );
        }
        Ok(())
    }
}
struct Checked {
    plan: EngineeringExplorationPlan,
    parent: WorkInput,
    project: Project,
    candidates: BTreeMap<String, WorkInput>,
    inputs: Inputs,
    expected: Value,
}
fn checked(root: &SkillRoot, input: &ArtifactReference) -> Result<Checked> {
    ensure!(
        sha256(plan::FIELD_SOURCE) == plan::FIELD_SHA256,
        "exploration_owner_field_source_drift"
    );
    let mut inputs = Inputs::new();
    inputs.get(
        root,
        &ArtifactReference {
            path: "specs/owner-schemas/engineering-exploration.json".into(),
            sha256: plan::FIELD_SHA256.into(),
        },
    )?;
    let plan: EngineeringExplorationPlan =
        serde_json::from_value(strict_json(&inputs.get(root, input)?)?)?;
    dto(plan.validate())?;
    ensure!(
        plan.schema == "oev2.engineering-exploration/1"
            && plan.exposure == "development_replay"
            && valid_id(&plan.exploration_id),
        "exploration_identity_or_exposure_invalid"
    );
    ensure!(
        (1..=8).contains(&plan.candidates.len())
            && (1..=16).contains(&plan.actions.len())
            && (1..=32).contains(&plan.hypotheses.len())
            && plan.outcomes.len() <= 64
            && plan.observations.len() <= 32
            && plan.unknowns.len() <= 64,
        "exploration_budget"
    );
    let parent: WorkInput =
        serde_json::from_value(strict_json(&inputs.get(root, &plan.parent_work)?)?)?;
    ensure!(
        parent.schema == "oev2.engineering-work/3",
        "exploration_requires_current_work3"
    );
    let project_ref = ArtifactReference {
        path: parent.project.path.to_string_lossy().into(),
        sha256: parent.project.sha256.clone(),
    };
    let project_raw = inputs.get(root, &project_ref)?;
    let project: Project = serde_json::from_value(strict_json(&project_raw)?)?;
    verify_project_binding(&project, &parent)?;
    let parent_basis = engineering::basis(&project, &parent.object)?
        .context("exploration_parent_basis_missing")?;
    for r in &plan.observations {
        inputs.evidence(root, &project, r)?;
    }
    if let Some(r) = &plan.prior_contract {
        inputs.get(root, r)?;
    }
    let expected = strict_json(&inputs.get(root, &plan.independent_expected)?)?;
    ensure!(
        expected["schema"] == "oev2.exploration-expectations/1"
            && expected["exposure"] == plan.exposure
            && expected["quality"]["preserve_complete_acceptance"] == true
            && expected["cost"]["maximum_actions"]
                .as_u64()
                .is_some_and(|v| v <= 16)
            && expected["cost"]["maximum_model_calls"].as_u64().is_some(),
        "exploration_expected_contract_invalid"
    );
    let mut candidates = BTreeMap::new();
    for candidate in &plan.candidates {
        dto(candidate.validate())?;
        ensure!(
            valid_id(&candidate.candidate_id)
                && nonempty(&candidate.principle)
                && nonempty(&candidate.representation)
                && stated(&candidate.applicability)
                && !candidate.source_refs.is_empty()
                && candidate.remaining_obligations == parent.acceptance,
            "exploration_candidate_obligations_missing"
        );
        let work: WorkInput =
            serde_json::from_value(strict_json(&inputs.get(root, &candidate.work)?)?)?;
        verify_project_binding(&project, &work)?;
        ensure!(
            work.schema == "oev2.engineering-work/3"
                && json!(work.project) == json!(parent.project)
                && work.acceptance == parent.acceptance,
            "exploration_complete_parent_task_changed"
        );
        let basis = engineering::basis(&project, &work.object)?
            .context("exploration_candidate_basis_missing")?;
        if work.object != parent.object {
            ensure!(
                basis.protocol() == "oev2.engineering-basis/2"
                    && basis.original_value()["goal_support"]["parent_object"]
                        == json!(parent.object),
                "exploration_parent_goal_unbound"
            );
            ensure!(
                engineering::checks_current(root, &project, &work.object)
                    .iter()
                    .any(|c| c.id == "engineering_goal_support"
                        && c.execution == "completed"
                        && c.conclusion == "pass"),
                "exploration_parent_goal_invalid"
            );
        }
        // Preserve the parent's external conditions on every route, including
        // unknowns. A changed model cannot justify lowering the load or threshold.
        for fact in &parent_basis.facts {
            let current = basis
                .facts
                .iter()
                .find(|f| f.property == fact.property)
                .context("exploration_parent_condition_missing")?;
            ensure!(
                json!(current) == json!(fact),
                "exploration_parent_condition_changed"
            );
        }
        for r in &candidate.source_refs {
            inputs.evidence(root, &project, r)?;
        }
        ensure!(
            candidates
                .insert(candidate.candidate_id.clone(), work)
                .is_none(),
            "exploration_candidate_duplicate"
        );
    }
    if plan.comparison_mode == "parameter" {
        ensure!(
            plan.candidates.len() == 2,
            "exploration_parameter_requires_pair"
        );
        let a = &candidates[&plan.candidates[0].candidate_id];
        let b = &candidates[&plan.candidates[1].candidate_id];
        oe_kernel::learning::design_comparison::validate_rules(
            root,
            &project,
            &parent.object,
            &parent.acceptance,
            &plan.parameter_rules,
            &oe_kernel::learning::design_comparison::Side {
                object: &a.object,
                calculations: &a.calculations,
                execution: "not_run",
            },
            &oe_kernel::learning::design_comparison::Side {
                object: &b.object,
                calculations: &b.calculations,
                execution: "not_run",
            },
        )?;
        ensure!(
            plan.candidates[0].principle == plan.candidates[1].principle
                && plan.candidates[0].representation == plan.candidates[1].representation,
            "exploration_parameter_principle_changed"
        );
    } else {
        ensure!(
            plan.comparison_mode == "mechanism"
                && plan.parameter_rules.is_empty()
                && nonempty(&plan.comparison_extension_reason),
            "exploration_explicit_new_comparison_required"
        );
    }
    let mut hypotheses = BTreeMap::new();
    let mut decisions = BTreeSet::new();
    for h in &plan.hypotheses {
        dto(h.validate())?;
        let work = candidates
            .get(&h.candidate_id)
            .context("exploration_hypothesis_candidate_unbound")?;
        let d = project
            .decisions
            .iter()
            .find(|d| d.id == h.decision_id)
            .context("exploration_hypothesis_decision_missing")?;
        ensure!(
            valid_id(&h.hypothesis_id)
                && nonempty(&h.statement)
                && stated(&h.scope)
                && !d.adopted
                && d.object_id == work.object.object_id
                && d.object_revision == work.object.revision
                && d.condition_scope == work.object.condition_scope
                && d.source_sha256 == work.object.source_sha256
                && work.gaps.iter().any(|g| g.gap_id == h.gap_id)
                && decisions.insert(h.decision_id.clone())
                && hypotheses.insert(h.hypothesis_id.clone(), h).is_none(),
            "exploration_hypothesis_scope_invalid"
        );
        ensure!(
            h.grounding != "grounded_explanation" || !h.source_refs.is_empty(),
            "exploration_grounding_missing"
        );
        for r in &h.source_refs {
            inputs.evidence(root, &project, r)?;
        }
    }
    let mut actions = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    for o in &plan.outcomes {
        dto(o.validate())?;
        let mut routes = BTreeSet::new();
        ensure!(
            o.retained_candidates
                .iter()
                .chain(&o.rejected_candidates)
                .chain(&o.review_candidates)
                .all(|id| candidates.contains_key(id) && routes.insert(id)),
            "exploration_route_impact_invalid"
        );
        let mut ids = BTreeSet::new();
        ensure!(
            valid_id(&o.outcome_id)
                && outcomes.insert(o.outcome_id.clone(), o).is_none()
                && o.supported
                    .iter()
                    .chain(&o.rejected)
                    .chain(&o.review)
                    .all(|id| hypotheses.contains_key(id) && ids.insert(id)),
            "exploration_outcome_hypotheses_invalid"
        );
        ensure!(!ids.is_empty(), "exploration_outcome_impact_missing");
        if ["lt", "le", "gt", "ge"].contains(&o.operator.as_str()) {
            quantity(&o.value)?;
        }
        if ["present", "absent"].contains(&o.operator.as_str()) {
            ensure!(o.value.is_null(), "exploration_presence_value_invalid");
        }
    }
    for (position, a) in plan.actions.iter().enumerate() {
        dto(a.validate())?;
        let work = candidates
            .get(&a.candidate_id)
            .context("exploration_action_candidate_unbound")?;
        ensure!(
            valid_id(&a.action_id)
                && actions.insert(a.action_id.clone(), position).is_none()
                && work.gaps.iter().any(|g| g.gap_id == a.gap_id)
                && parent.acceptance.contains(&a.requirement)
                && stated(&a.hypotheses)
                && a.hypotheses.iter().all(|id| hypotheses
                    .get(id)
                    .is_some_and(|h| h.candidate_id == a.candidate_id && h.gap_id == a.gap_id))
                && nonempty(&a.purpose)
                && nonempty(&a.expected_impact)
                && nonempty(&a.selection_reason)
                && (a.pointer.is_empty() || a.pointer.starts_with('/')),
            "exploration_action_unbound"
        );
        ensure!(
            a.hypotheses.len() <= 32
                && a.required_facts.len() <= 128
                && a.when_outcomes.len() <= 16
                && a.cost_sources.len() <= 8,
            "exploration_action_budget"
        );
        match a.kind.as_str() {
            "calculation" => ensure!(
                a.source.is_none()
                    && a.calculation_id.as_ref().is_some_and(|id| work
                        .calculations
                        .iter()
                        .any(|c| &c.calculation_id == id)),
                "exploration_calculation_unbound"
            ),
            "readback" | "integrity" => {
                ensure!(
                    a.calculation_id.is_none(),
                    "exploration_action_has_two_owners"
                );
                inputs.evidence(
                    root,
                    &project,
                    a.source
                        .as_ref()
                        .context("exploration_action_source_missing")?,
                )?;
            }
            "external" => ensure!(
                a.source.is_none() && a.calculation_id.is_none(),
                "exploration_external_action_implicit_execution"
            ),
            _ => anyhow::bail!("exploration_action_unsupported"),
        }
        ensure!(
            a.cost_basis == "unknown" || !a.cost_sources.is_empty(),
            "exploration_cost_estimate_source_missing"
        );
        for r in &a.cost_sources {
            inputs.evidence(root, &project, r)?;
        }
        let branches: Vec<_> = plan
            .outcomes
            .iter()
            .filter(|o| o.action_id == a.action_id)
            .collect();
        ensure!(
            branches.len() >= 2
                && branches.iter().all(|o| o
                    .supported
                    .iter()
                    .chain(&o.rejected)
                    .chain(&o.review)
                    .all(|id| a.hypotheses.contains(id))
                    && o.retained_candidates
                        .iter()
                        .chain(&o.rejected_candidates)
                        .chain(&o.review_candidates)
                        .all(|id| id == &a.candidate_id)),
            "exploration_discrimination_missing"
        );
        let impacts: BTreeSet<_> = branches
            .iter()
            .map(|o| {
                format!(
                    "{:?}/{:?}/{:?}",
                    o.supported.iter().collect::<BTreeSet<_>>(),
                    o.rejected.iter().collect::<BTreeSet<_>>(),
                    o.review.iter().collect::<BTreeSet<_>>()
                )
            })
            .collect();
        ensure!(
            impacts.len() >= 2,
            "exploration_branches_cannot_distinguish"
        );
    }
    for a in &plan.actions {
        for id in &a.when_outcomes {
            let o = outcomes.get(id).context("exploration_dependency_unbound")?;
            ensure!(
                actions
                    .get(&o.action_id)
                    .is_some_and(|p| p < &actions[&a.action_id]),
                "exploration_dependency_cycle_or_forward"
            );
        }
        let basis = engineering::basis(&project, &candidates[&a.candidate_id].object)?.unwrap();
        ensure!(
            a.required_facts
                .iter()
                .all(|name| basis.facts.iter().any(|f| &f.property == name)),
            "exploration_required_fact_unbound"
        );
    }
    ensure!(
        plan.outcomes
            .iter()
            .all(|o| actions.contains_key(&o.action_id)),
        "exploration_outcome_action_unbound"
    );
    ensure!(
        plan.candidates.iter().all(|candidate| {
            let work = &candidates[&candidate.candidate_id];
            plan.actions
                .iter()
                .any(|a| a.candidate_id == candidate.candidate_id)
                || !work.calculations.is_empty()
                || !work.source_requests.is_empty()
        }),
        "exploration_candidate_verification_method_missing"
    );
    ensure!(
        expected["quality"]["action_order"].is_array()
            && expected["quality"]["decision_conclusions"]
                .as_object()
                .is_some_and(|m| m.len() == decisions.len()
                    && m.iter().all(|(id, v)| decisions.contains(id)
                        && v.as_str()
                            .is_some_and(|s| ["supported", "refuted", "unknown"].contains(&s)))),
        "exploration_expected_scope_invalid"
    );
    let mut expected_actions = BTreeSet::new();
    ensure!(
        expected["quality"]["action_order"]
            .as_array()
            .unwrap()
            .iter()
            .all(|id| id
                .as_str()
                .is_some_and(|id| actions.contains_key(id) && expected_actions.insert(id))),
        "exploration_expected_action_unbound_or_duplicate"
    );
    inputs.current(root)?;
    Ok(Checked {
        plan,
        parent,
        project,
        candidates,
        inputs,
        expected,
    })
}

pub fn freeze(root: &SkillRoot, input: &ArtifactReference, output: &Path) -> Result<Value> {
    let c = checked(root, input)?;
    let dir = root.new_private_dir(output)?;
    root.new_private_dir(&dir.join("inputs"))?;
    write_new(&dir.join("plan.json"), &json!(c.plan))?;
    let mut files = Vec::new();
    for (i, (path, bytes)) in c.inputs.bytes.iter().enumerate() {
        let name = format!("inputs/{i}.bin");
        write_new_bytes(&dir.join(&name), bytes)?;
        files.push(json!({"source":{"path":path,"sha256":sha256(bytes)},"copy":name}));
    }
    c.inputs.current(root)?;
    let terminal = json!({"schema":"oev2.exploration-frozen/1","plan":reference(root,&dir.join("plan.json"))?,
        "source_input":input,"sources":files,"exposure":c.plan.exposure,"acceptance":c.parent.acceptance,
        "criteria_sha256":oe_core::json::canonical_digest(&json!(c.plan.outcomes))?,
        "execution":"not_run","adopted":false,"method_benefit":"unknown"});
    write_new(&dir.join("completed.json"), &terminal)?;
    Ok(terminal)
}
fn frozen(root: &SkillRoot, input: &ArtifactReference) -> Result<Checked> {
    let terminal = strict_json(&read(root, input)?)?;
    ensure!(
        terminal["schema"] == "oev2.exploration-frozen/1",
        "exploration_frozen_schema"
    );
    let plan_ref: ArtifactReference = serde_json::from_value(terminal["plan"].clone())?;
    let mut c = checked(root, &plan_ref)?;
    let original: ArtifactReference = serde_json::from_value(terminal["source_input"].clone())?;
    ensure!(
        json!(c.plan)
            == json!(serde_json::from_value::<EngineeringExplorationPlan>(
                strict_json(&read(root, &original)?)?
            )?)
            && terminal["acceptance"] == json!(c.parent.acceptance)
            && terminal["criteria_sha256"]
                == oe_core::json::canonical_digest(&json!(c.plan.outcomes))?,
        "exploration_frozen_criteria_changed"
    );
    let dir = root
        .expand(Path::new(&input.path))?
        .parent()
        .context("exploration_frozen_directory")?
        .to_owned();
    let rows = terminal["sources"]
        .as_array()
        .context("exploration_frozen_sources")?;
    let mut observed = BTreeMap::new();
    for row in rows {
        let source: ArtifactReference = serde_json::from_value(row["source"].clone())?;
        let name = row["copy"]
            .as_str()
            .context("exploration_frozen_copy_missing")?;
        oe_semantic::safe_relative(name)?;
        let copy_ref = reference(root, &dir.join(name))?;
        ensure!(
            copy_ref.sha256 == source.sha256,
            "exploration_frozen_copy_drift"
        );
        let bytes = read(root, &source)?;
        ensure!(
            bytes == read(root, &copy_ref)? && observed.insert(source.path, bytes).is_none(),
            "exploration_frozen_source_drift"
        );
    }
    // The canonical plan copy is an additional input; all original live sources
    // must be present in the frozen closure, not merely those the caller lists.
    let plan_path =
        oe_core::portable_source_path(root.path(), &root.expand(Path::new(&plan_ref.path))?)?;
    let original_path =
        oe_core::portable_source_path(root.path(), &root.expand(Path::new(&original.path))?)?;
    let mut expected = c.inputs.bytes.clone();
    expected.remove(&plan_path);
    expected.insert(original_path, read(root, &original)?);
    ensure!(
        observed == expected,
        "exploration_frozen_source_inventory_changed"
    );
    c.inputs.get(root, &original)?;
    c.inputs.get(root, input)?;
    for row in rows {
        c.inputs.get(
            root,
            &reference(root, &dir.join(row["copy"].as_str().unwrap()))?,
        )?;
    }
    Ok(c)
}
pub fn verify(root: &SkillRoot, input: &ArtifactReference) -> Result<Value> {
    let c = frozen(root, input)?;
    Ok(
        json!({"status":"current_frozen_exploration_verified","exploration_id":c.plan.exploration_id,"execution":"not_run"}),
    )
}
fn quantity(value: &Value) -> Result<(f64, &str)> {
    let n = value["value"]
        .as_f64()
        .context("exploration_quantity_missing")?;
    let unit = value["unit"]
        .as_str()
        .filter(|u| nonempty(u))
        .context("exploration_unit_missing")?;
    ensure!(n.is_finite(), "exploration_quantity_nonfinite");
    Ok((n, unit))
}
fn matches(o: &EngineeringExplorationOutcome, value: Option<&Value>) -> Result<bool> {
    match o.operator.as_str() {
        "present" => Ok(value.is_some()),
        "absent" => Ok(value.is_none()),
        "eq" | "ne" => {
            let Some(v) = value else { return Ok(false) };
            if o.value.get("value").is_some() || v.get("value").is_some() {
                let (a, au) = quantity(v)?;
                let (b, bu) = quantity(&o.value)?;
                ensure!(au == bu, "exploration_unit_mismatch");
                Ok(if o.operator == "eq" { a == b } else { a != b })
            } else {
                Ok(if o.operator == "eq" {
                    v == &o.value
                } else {
                    v != &o.value
                })
            }
        }
        "lt" | "le" | "gt" | "ge" => {
            let Some(v) = value else { return Ok(false) };
            let (a, au) = quantity(v)?;
            let (b, bu) = quantity(&o.value)?;
            ensure!(au == bu, "exploration_unit_mismatch");
            Ok(match o.operator.as_str() {
                "lt" => a < b,
                "le" => a <= b,
                "gt" => a > b,
                _ => a >= b,
            })
        }
        _ => anyhow::bail!("exploration_operator_unsupported"),
    }
}
fn action(root: &SkillRoot, c: &Checked, a: &EngineeringExplorationAction) -> Result<Value> {
    match a.kind.as_str() {
        "calculation" => {
            let w = &c.candidates[&a.candidate_id];
            let calc = w
                .calculations
                .iter()
                .find(|v| Some(&v.calculation_id) == a.calculation_id.as_ref())
                .unwrap();
            let check = engineering::calculation_check(&c.project, calc);
            let mut result = super::calculate(calc)?;
            result["model_applicability"] = json!(check);
            result["decision_support"] = json!(
                result["execution"] == "completed"
                    && result["model_applicability"]["conclusion"] == "pass"
            );
            Ok(result)
        }
        "integrity" => {
            let r = a.source.as_ref().unwrap();
            let bytes = read(root, r)?;
            Ok(
                json!({"execution":"completed","decision_support":true,"sha256":sha256(&bytes),"bytes":bytes.len(),"authority":"source_byte_identity_only"}),
            )
        }
        "readback" => {
            let r = a.source.as_ref().unwrap();
            let bytes = read(root, r)?;
            Ok(
                json!({"execution":"completed","decision_support":true,"source_sha256":r.sha256,
                "observation":strict_json(&bytes)?,"authority":"bound_source_statement_not_physical_confirmation"}),
            )
        }
        _ => Ok(
            json!({"execution":"not_run","decision_support":false,"conclusion":"unknown",
            "reason":"explicit_external_verification_and_current_native_or_measurement_receipt_required"}),
        ),
    }
}
/// Bounded generic exploration assessment. Frozen expectations are separate
/// inputs; replay correctness and measured local cost do not prove method gain.
pub fn assessment(expected: &Value, result: &Value) -> Result<Value> {
    assessment_version(expected, result, false)
}
fn assessment_host(expected: &Value, result: &Value) -> Result<Value> {
    let mut assessment = assessment_version(expected, result, true)?;
    if result["actions"].as_array().is_some_and(|actions| {
        actions
            .iter()
            .any(|a| a["selection"] == "selected" && a["readback"]["execution"] == "not_run")
    }) {
        assessment["behavior_quality"] = json!("pending_verification");
    }
    Ok(assessment)
}
fn assessment_version(expected: &Value, result: &Value, host: bool) -> Result<Value> {
    ensure!(
        expected["schema"] == "oev2.exploration-expectations/1"
            && result["schema"]
                == if host {
                    "oev2.exploration-results/2"
                } else {
                    "oev2.exploration-results/1"
                }
            && expected["exposure"] == "development_replay"
            && result["exposure"] == expected["exposure"],
        "exploration_assessment_scope"
    );
    let actions = result["actions"]
        .as_array()
        .context("exploration_assessment_actions")?;
    let order: Vec<_> = actions
        .iter()
        .filter(|a| a["selection"] == "selected")
        .map(|a| a["action_id"].clone())
        .collect();
    let decisions = expected["quality"]["decision_conclusions"]
        .as_object()
        .context("exploration_assessment_decisions")?;
    let quality = expected["quality"]["action_order"] == json!(order)
        && decisions
            .iter()
            .all(|(id, value)| result["decision_conclusions"][id] == *value)
        && result["complete_acceptance_preserved"] == true;
    let count = order.len() as u64;
    let cost = count
        <= expected["cost"]["maximum_actions"]
            .as_u64()
            .context("exploration_assessment_budget")?
        && result["cost"]["model_calls"]
            .as_u64()
            .zip(expected["cost"]["maximum_model_calls"].as_u64())
            .is_some_and(|(actual, maximum)| actual <= maximum);
    Ok(
        json!({"schema":if host {"oev2.exploration-assessment/2"} else {"oev2.exploration-assessment/1"},"exposure":"development_replay",
        "behavior_quality":if quality{"supported_for_frozen_replay"}else{"violated"},
        "local_cost_budget":if cost{"within_frozen_action_and_model_budget"}else{"violated"},
        "measured_local_cost":result["cost"],"local_metrics":"bound_source_or_declared_model_only",
        "candidate_feasibility":"unknown","whole_engineering_achievement":"unknown","method_benefit":"unknown",
        "existing_experience_evaluator_scope":"semantic_review_coverage_and_optional_retrieval_replay",
        "promotion":"not_admissible_from_this_assessment",
        "remaining":"freeze baseline/candidate quality, full costs, exposure and subsequent tasks before assessing generic exploration benefit"}),
    )
}
struct Evaluation {
    updated: Project,
    actions: Vec<Value>,
    routes: BTreeMap<String, Value>,
}
fn evaluate(
    root: &SkillRoot,
    c: &Checked,
    external: Option<&BTreeMap<String, Value>>,
) -> Result<Evaluation> {
    let mut updated = c.project.clone();
    let mut actions = Vec::new();
    let mut matched = BTreeSet::new();
    let result_id = format!("exploration:{}", c.plan.exploration_id);
    let mut routes = BTreeMap::<String, Value>::new();
    ensure!(
        !updated.evidence.contains_key(&result_id),
        "exploration_result_identity_already_used"
    );
    let mut waiting = false;
    for a in &c.plan.actions {
        if waiting {
            actions.push(json!({"action_id":a.action_id,"selection":"not_selected","reason":"awaiting_host_result","execution":"not_run"}));
            continue;
        }
        if !a.when_outcomes.iter().all(|id| matched.contains(id))
            || actions
                .iter()
                .filter(|v: &&Value| v["selection"] == "selected")
                .count() as u64
                >= c.expected["cost"]["maximum_actions"].as_u64().unwrap()
        {
            actions.push(json!({"action_id":a.action_id,"selection":"not_selected","reason":"frozen_prior_outcome_not_observed_or_action_budget_exhausted","execution":"not_run"}));
            continue;
        }
        let start = Instant::now();
        let attempted = if a.kind == "external" {
            if let Some(value) = external.and_then(|rows| rows.get(&a.action_id)) {
                Ok(value.clone())
            } else {
                waiting = external.is_some();
                action(root, c, a)
            }
        } else {
            action(root, c, a)
        };
        let raw = match attempted {
            Ok(v) => v,
            Err(e) => json!({"execution":"failed","decision_support":false,"reason":e.to_string()}),
        };
        let mut labels = Vec::new();
        let mut comparison_error = None;
        if raw["execution"] == "completed" && raw["decision_support"] == true {
            for o in c
                .plan
                .outcomes
                .iter()
                .filter(|o| o.action_id == a.action_id)
            {
                match matches(o, raw.pointer(&a.pointer)) {
                    Ok(true) => labels.push(o),
                    Ok(false) => (),
                    Err(e) => {
                        comparison_error = Some(e.to_string());
                    }
                }
            }
        }
        let branch = if comparison_error.is_none() && labels.len() == 1 {
            Some(labels[0])
        } else {
            None
        };
        let mut disposition = if comparison_error.is_some() {
            "comparison_failed"
        } else if labels.len() > 1 {
            "ambiguous"
        } else {
            "inconclusive"
        };
        if let Some(o) = branch {
            matched.insert(o.outcome_id.clone());
            disposition = "distinguished";
            for (ids, state) in [
                (&o.retained_candidates, "retained_for_this_scope"),
                (&o.rejected_candidates, "rejected_in_this_scope"),
                (&o.review_candidates, "requires_review"),
            ] {
                for id in ids {
                    let candidate = c
                        .plan
                        .candidates
                        .iter()
                        .find(|v| &v.candidate_id == id)
                        .unwrap();
                    routes.insert(id.clone(),json!({"disposition":state,"action_id":a.action_id,"outcome_id":o.outcome_id,
                        "object":c.candidates[id].object,"scope":candidate.applicability,"remaining_obligations":candidate.remaining_obligations,
                        "authority":"bound_source_or_declared_model_scope_only","engineering_adoption":"not_established"}));
                }
            }
            for h in c
                .plan
                .hypotheses
                .iter()
                .filter(|h| a.hypotheses.contains(&h.hypothesis_id))
            {
                let conclusion = if o.supported.contains(&h.hypothesis_id) {
                    "supported"
                } else if o.rejected.contains(&h.hypothesis_id) {
                    "refuted"
                } else {
                    "unknown"
                };
                let d = updated
                    .decisions
                    .iter_mut()
                    .find(|d| d.id == h.decision_id)
                    .unwrap();
                // A source statement or model prediction may have a bounded
                // result. Causal hypotheses still need domain verification,
                // even when their predicted number was computed.
                d.conclusion = if h.kind == "causal_hypothesis" {
                    "unknown"
                } else {
                    conclusion
                }
                .into();
                if !d.basis.contains(&result_id) {
                    d.basis.push(result_id.clone());
                }
            }
        } else {
            for h in c
                .plan
                .hypotheses
                .iter()
                .filter(|h| a.hypotheses.contains(&h.hypothesis_id))
            {
                updated
                    .decisions
                    .iter_mut()
                    .find(|d| d.id == h.decision_id)
                    .unwrap()
                    .conclusion = "unknown".into();
            }
        }
        let basis = engineering::basis(&c.project, &c.candidates[&a.candidate_id].object)?.unwrap();
        let unknown_facts: Vec<_> =
            a.required_facts
                .iter()
                .filter(|id| {
                    basis.facts.iter().any(|f| {
                        &f.property == *id && (f.status != "declared" || f.value.is_null())
                    })
                })
                .collect();
        actions.push(json!({"action_id":a.action_id,"selection":"selected","purpose":a.purpose,"expected_impact":a.expected_impact,
            "requirement":a.requirement,"selection_reason":a.selection_reason,"unknown_execution_conditions":unknown_facts,
            "cost_basis":a.cost_basis,"reported_cost_sources":a.cost_sources,"actual_local_elapsed_ns":start.elapsed().as_nanos().to_string(),
            "external_cost":"unknown","readback":raw,"disposition":disposition,"comparison_error":comparison_error,
            "matched_outcomes":labels.iter().map(|o|&o.outcome_id).collect::<Vec<_>>(),"hypothesis_is_fact":false}));
    }
    if let Some(rows) = external {
        ensure!(
            rows.keys().all(|id| actions
                .iter()
                .any(|a| a["action_id"] == *id && a["selection"] == "selected")),
            "exploration_host_unselected_receipt"
        );
    }
    Ok(Evaluation {
        updated,
        actions,
        routes,
    })
}

pub fn execute(
    root: &SkillRoot,
    input: &EngineeringExplorationRunRequest,
) -> Result<EngineeringExplorationRunResult> {
    execute_with(root, input, None)
}
fn execute_with(
    root: &SkillRoot,
    input: &EngineeringExplorationRunRequest,
    external: Option<&BTreeMap<String, Value>>,
) -> Result<EngineeringExplorationRunResult> {
    dto(input.validate())?;
    let c = frozen(root, &input.frozen)?;
    let dir = root.new_private_dir(Path::new(&input.output))?;
    let started = Instant::now();
    let Evaluation {
        mut updated,
        actions,
        routes,
    } = evaluate(root, &c, external)?;
    let result_id = format!("exploration:{}", c.plan.exploration_id);
    c.inputs.current(root)?;
    let conclusions: BTreeMap<_, _> = c
        .plan
        .hypotheses
        .iter()
        .map(|h| {
            (
                &h.decision_id,
                &updated
                    .decisions
                    .iter()
                    .find(|d| d.id == h.decision_id)
                    .unwrap()
                    .conclusion,
            )
        })
        .collect();
    let result = json!({"schema":if external.is_some(){"oev2.exploration-results/2"}else{"oev2.exploration-results/1"},"exploration_id":c.plan.exploration_id,"exposure":c.plan.exposure,
        "object":c.parent.object,"project_before":c.parent.project,"acceptance":c.parent.acceptance,
        "complete_acceptance_preserved":true,"observations":c.plan.observations,"unknowns":c.plan.unknowns,
        "hypotheses":c.plan.hypotheses,"candidates":c.plan.candidates,"criteria":c.plan.outcomes,
        "criteria_sha256":oe_core::json::canonical_digest(&json!(c.plan.outcomes))?,"actions":actions,"decision_conclusions":conclusions,
        "candidate_routes":routes,"facts_mutated":false,"adopted":false,"physical_truth":"not_established",
        "cost":{"local_elapsed_ns":started.elapsed().as_nanos().to_string(),"model_calls":0,"tokens":0,
            "external_cost":"unknown","scope":"actual inquiry owner only; host session, memory, original source creation, CAD, factory and complete task costs separate"}});
    write_new(&dir.join("results.json"), &result)?;
    let result_ref = reference(root, &dir.join("results.json"))?;
    updated
        .evidence
        .insert(result_id, result_ref.sha256.clone());
    updated.revision = format!("{}:exploration:{}", updated.revision, c.plan.exploration_id);
    project::validate(&updated)?;
    ensure!(
        updated.conditions == c.project.conditions && updated.objects == c.project.objects,
        "exploration_facts_changed"
    );
    write_new(&dir.join("project.json"), &json!(updated))?;
    let project_ref = reference(root, &dir.join("project.json"))?;
    let mut work = c.parent.clone();
    work.project = Reference {
        path: project_ref.path.clone().into(),
        sha256: project_ref.sha256.clone(),
    };
    work.project_context = Some(project::context(&updated, &work.object)?);
    verify_project_binding(&updated, &work)?;
    write_new(&dir.join("work.json"), &json!(work))?;
    let work_ref = reference(root, &dir.join("work.json"))?;
    let view_input = CurrentTaskViewInput {
        schema: "oev2.current-task-view-input/1".into(),
        current: project_ref.clone(),
        previous: Some(ArtifactReference {
            path: c.parent.project.path.to_string_lossy().into(),
            sha256: c.parent.project.sha256.clone(),
        }),
        work: Some(work_ref.clone()),
        sources: vec![result_ref.clone()],
    };
    let view = crate::current_task_view::derive(root, &view_input)?;
    write_new(&dir.join("task-view.json"), &json!(view))?;
    let harness_input = json!({"schema":crate::harness::CHECKPOINT_INPUT_SCHEMA,"task_id":c.plan.exploration_id,
        "context":{"frozen_exploration":input.frozen,"result":result_ref},"request":"Retain the actual inquiry, unresolved engineering checks and scoped decision changes.",
        "harness":{"project_id":updated.project_id,"decision_id":c.plan.hypotheses[0].decision_id,"object_id":work.object.object_id,
            "revision":work.object.revision,"problem_class":"engineering_inquiry","goal":updated.purpose,
            "conditions":[updated.conditions[&work.object.condition_scope]],"constraints":work.acceptance,"unknowns":c.plan.unknowns,
            "current_project":project_ref,"project_context":work.project_context,
            "observations":[{"kind":"observed","statement":"Bound verification results and scoped decision changes; physical truth and method gain remain unestablished.","evidence":[result_ref]}],
            "checkpoint_policy":{"schema":"oev2.harness-checkpoint-policy/1","mode":"verified_no_model","byte_budget":16777216,
                "reason":"Actual no-model inquiry checkpoint; complete constraints retained."}}});
    write_new(&dir.join("harness-input.json"), &harness_input)?;
    let harness = match crate::harness::step(root, &harness_input, &dir.join("harness")) {
        Ok(v) => v,
        Err(e) => {
            json!({"status":"failed","execution":"partial_or_failed","reason":e.to_string(),"original_project_mutated":false,"adopted":false})
        }
    };
    write_new(&dir.join("harness.json"), &harness)?;
    write_new(
        &dir.join("assessment.json"),
        &if external.is_some() {
            assessment_host(&c.expected, &result)?
        } else {
            assessment(&c.expected, &result)?
        },
    )?;
    c.inputs.current(root)?;
    let run = EngineeringExplorationRunResult {
        schema: "oev2.engineering-exploration-run/1".into(),
        result: result_ref,
        project: project_ref,
        work: work_ref,
        task_view: reference(root, &dir.join("task-view.json"))?,
        harness: reference(root, &dir.join("harness.json"))?,
        original_project_mutated: false,
        adopted: false,
    };
    write_new(&dir.join("completed.json"), &json!(run))?;
    Ok(run)
}
pub struct ExplorationExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl EngineeringExplorationPort for ExplorationExecutor<'_> {
    fn execute(
        &self,
        input: &EngineeringExplorationRunRequest,
    ) -> std::result::Result<EngineeringExplorationRunResult, FailureRecord> {
        execute(self.root, input)
            .map_err(|e| crate::artifacts::failure("engineering_exploration", e))
    }
}
