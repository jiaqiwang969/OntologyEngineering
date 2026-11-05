//! One source-bound relation projection for inquiry, host handoff and recovery.
//! Selection explains the declared plan; Project impact remains its original owner.
use super::*;
use oe_contracts::{
    EngineeringReasoningAction, EngineeringReasoningCheck, EngineeringReasoningInput,
    EngineeringReasoningNode, EngineeringReasoningPort, EngineeringReasoningRelation,
    EngineeringReasoningView, HostExplorationState,
};

const INPUT: &str = "oev2.engineering-reasoning-input/1";
const VIEW: &str = "oev2.engineering-reasoning-view/2";

fn segment(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

struct Graph<'a> {
    inputs: &'a Inputs,
    json: BTreeMap<String, Value>,
    nodes: BTreeMap<String, EngineeringReasoningNode>,
    relations: Vec<EngineeringReasoningRelation>,
}
impl<'a> Graph<'a> {
    fn value(
        &mut self,
        source: &ArtifactReference,
        pointer: &str,
        embedded: Option<&str>,
    ) -> Result<Value> {
        if !self.json.contains_key(&source.path) {
            let raw = self
                .inputs
                .bytes
                .get(&source.path)
                .context("reasoning_source_not_checked")?;
            ensure!(
                sha256(raw) == source.sha256,
                "reasoning_source_identity_changed"
            );
            self.json.insert(source.path.clone(), strict_json(raw)?);
        }
        let value = self.json[&source.path]
            .pointer(pointer)
            .context("reasoning_source_location_missing")?;
        if let Some(pointer) = embedded {
            let inner = strict_json(
                value
                    .as_str()
                    .context("reasoning_embedded_condition_not_string")?
                    .as_bytes(),
            )?;
            Ok(inner
                .pointer(pointer)
                .context("reasoning_embedded_location_missing")?
                .clone())
        } else {
            Ok(value.clone())
        }
    }
    fn node(
        &mut self,
        id: &str,
        kind: &str,
        status: &str,
        source: &ArtifactReference,
        pointer: &str,
        embedded: Option<&str>,
    ) -> Result<()> {
        let value = self.value(source, pointer, embedded)?;
        let node = EngineeringReasoningNode {
            id: id.into(),
            kind: kind.into(),
            value,
            status: status.into(),
            source: source.clone(),
            pointer: pointer.into(),
            embedded_pointer: embedded.map(str::to_owned),
        };
        dto(node.validate())?;
        if let Some(old) = self.nodes.get(id) {
            ensure!(old == &node, "reasoning_role_identity_collision");
        } else {
            ensure!(self.nodes.len() < 4096, "reasoning_node_budget");
            self.nodes.insert(id.into(), node);
        }
        Ok(())
    }
    fn edge(
        &mut self,
        from: &str,
        to: &str,
        kind: &str,
        source: &ArtifactReference,
        pointer: &str,
    ) -> Result<()> {
        ensure!(self.relations.len() < 8192, "reasoning_relation_budget");
        self.value(source, pointer, None)?;
        self.relations.push(EngineeringReasoningRelation {
            from: from.into(),
            to: to.into(),
            kind: kind.into(),
            source: source.clone(),
            pointer: pointer.into(),
        });
        Ok(())
    }
}

fn references(c: &Checked) -> Vec<ArtifactReference> {
    c.inputs
        .bytes
        .iter()
        .map(|(path, raw)| ArtifactReference {
            path: path.clone(),
            sha256: sha256(raw),
        })
        .collect()
}

fn action_views(
    c: &Checked,
    current: &Project,
    state: Option<&HostExplorationState>,
    result: &Value,
) -> Result<Vec<EngineeringReasoningAction>> {
    let rows = result
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let matched: BTreeSet<String> = rows
        .iter()
        .filter(|r| r["disposition"] == "distinguished")
        .flat_map(|r| r["matched_outcomes"].as_array().into_iter().flatten())
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let pending: BTreeSet<String> = state
        .into_iter()
        .flat_map(|s| &s.pending)
        .filter_map(|p| p["action"]["action_id"].as_str().map(str::to_owned))
        .collect();
    let attempted = rows
        .iter()
        .filter(|r| {
            r["selection"] == "selected" && !pending.contains(r["action_id"].as_str().unwrap_or(""))
        })
        .count() as u64;
    let budget = c.expected["cost"]["maximum_actions"]
        .as_u64()
        .context("reasoning_action_budget_missing")?;
    let mut actions = Vec::new();
    for a in &c.plan.actions {
        let work = &c.candidates[&a.candidate_id];
        let basis =
            engineering::basis(current, &work.object)?.context("reasoning_basis_missing")?;
        let unresolved: Vec<_> =
            a.required_facts
                .iter()
                .filter(|name| {
                    basis.facts.iter().any(|f| {
                        &f.property == *name && (f.status != "declared" || f.value.is_null())
                    })
                })
                .cloned()
                .collect();
        let waiting: Vec<_> = a
            .when_outcomes
            .iter()
            .filter(|id| !matched.contains(*id))
            .cloned()
            .collect();
        let row = rows.iter().find(|r| r["action_id"] == a.action_id);
        let (status, next) = if pending.contains(&a.action_id) {
            (
                "awaiting_host",
                "Obtain the current action's authorized result and read it back; unresolved premises remain investigation targets, not applicability proof.",
            )
        } else if let Some(r) = row.filter(|r| r["selection"] == "selected") {
            if r["disposition"] == "distinguished" {
                (
                    "recorded",
                    "Continue only along the uniquely established branch; the claim retains its source or model scope and all engineering obligations.",
                )
            } else {
                (
                    "needs_review",
                    "Inspect the retained observation, premises and comparison; obtain new discriminating evidence or revise the frozen inquiry before retrying.",
                )
            }
        } else if !waiting.is_empty() {
            (
                "blocked_by_outcome",
                "Establish the listed prerequisite outcomes before this action; failure, ambiguity and unobserved branches cannot unlock it.",
            )
        } else if !pending.is_empty() {
            (
                "blocked_by_pending_result",
                "Read back the already pending action before continuing the frozen sequence.",
            )
        } else if attempted >= budget {
            (
                "budget_exhausted",
                "Preserve remaining obligations and explicitly prepare a new bounded inquiry before further work.",
            )
        } else {
            (
                "ready",
                "First eligible work follows frozen order; clarify unresolved conditions and existing authorization before execution. Readiness is not model applicability.",
            )
        };
        let seeds: BTreeSet<_> = c
            .plan
            .hypotheses
            .iter()
            .filter(|h| a.hypotheses.contains(&h.hypothesis_id))
            .map(|h| h.decision_id.clone())
            .collect();
        let impact = project::decision_impact(current, &seeds)?;
        let affected = impact["decisions"]
            .as_array()
            .context("reasoning_impact_invalid")?
            .iter()
            .map(|d| {
                d["decision_id"]
                    .as_str()
                    .map(str::to_owned)
                    .context("reasoning_impact_decision_missing")
            })
            .collect::<Result<Vec<_>>>()?;
        actions.push(EngineeringReasoningAction {
            action_id: a.action_id.clone(),
            status: status.into(),
            requirement: a.requirement.clone(),
            unresolved_premises: unresolved,
            waiting_outcomes: waiting,
            affected_decisions: affected,
            engineering_obligations: Some(vec![]),
            next_step: next.into(),
        });
    }
    Ok(actions)
}

fn project_view(
    root: &SkillRoot,
    c: &mut Checked,
    input: &EngineeringReasoningInput,
    state: Option<&HostExplorationState>,
) -> Result<EngineeringReasoningView> {
    let original = ArtifactReference {
        path: c.parent.project.path.to_string_lossy().into(),
        sha256: c.parent.project.sha256.clone(),
    };
    let current_ref = state.map(|s| &s.run.project).unwrap_or(&original);
    ensure!(
        input.project == *current_ref && state.is_none_or(|s| s.frozen == input.frozen),
        "reasoning_current_binding_changed"
    );
    let current: Project = serde_json::from_value(strict_json(&c.inputs.get(root, current_ref)?)?)?;
    let terminal = strict_json(&c.inputs.get(root, &input.frozen)?)?;
    let plan_ref: ArtifactReference = serde_json::from_value(terminal["plan"].clone())?;
    let result = match state {
        Some(s) => strict_json(&c.inputs.get(root, &s.run.result)?)?,
        None => Value::Null,
    };
    let mut actions = action_views(c, &current, state, &result)?;
    let mut native_checks = Vec::new();
    let matched: BTreeSet<_> = result["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["disposition"] == "distinguished")
        .flat_map(|a| a["matched_outcomes"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let mut g = Graph {
        inputs: &c.inputs,
        json: BTreeMap::new(),
        nodes: BTreeMap::new(),
        relations: Vec::new(),
    };
    g.node("goal", "goal", "declared", current_ref, "/purpose", None)?;
    for (i, _) in c.parent.acceptance.iter().enumerate() {
        let id = format!("requirement:{i}");
        let p = format!("/acceptance/{i}");
        g.node(
            &id,
            "requirement",
            "required",
            &c.plan.parent_work,
            &p,
            None,
        )?;
        g.edge("goal", &id, "requires", &c.plan.parent_work, &p)?;
    }
    for (i, candidate) in c.plan.candidates.iter().enumerate() {
        let id = format!("candidate:{}", candidate.candidate_id);
        let p = format!("/candidates/{i}");
        let work = &c.candidates[&candidate.candidate_id];
        let status = result["candidate_routes"][&candidate.candidate_id]["disposition"]
            .as_str()
            .unwrap_or("proposed");
        g.node(&id, "candidate", status, &plan_ref, &p, None)?;
        g.edge("goal", &id, "considers_candidate", &plan_ref, &p)?;
        let object = format!("object:{}", candidate.candidate_id);
        g.node(
            &object,
            "object",
            "bound_revision",
            &candidate.work,
            "/object",
            None,
        )?;
        g.edge(
            &id,
            &object,
            "concerns_object",
            &plan_ref,
            &format!("{p}/work"),
        )?;
        let condition = format!("condition:{}", candidate.candidate_id);
        g.node(
            &condition,
            "condition",
            "declared_scope",
            current_ref,
            &format!("/conditions/{}", segment(&work.object.condition_scope)),
            None,
        )?;
        g.edge(
            &object,
            &condition,
            "under_condition",
            &candidate.work,
            "/object/condition_scope",
        )?;
        let basis =
            engineering::basis(&current, &work.object)?.context("reasoning_basis_missing")?;
        let condition_pointer = format!("/conditions/{}", segment(&work.object.condition_scope));
        let checks = engineering::checks_current(root, &current, &work.object);
        for (index, assertion) in basis.assertions.iter().enumerate() {
            let node = format!("assertion:{}:{index}", candidate.candidate_id);
            let check_id = format!("engineering:{}", assertion.assertion_id);
            let status = checks
                .iter()
                .find(|c| c.id == check_id)
                .map(|c| c.conclusion.as_str())
                .unwrap_or("unknown");
            g.node(
                &node,
                "assertion",
                status,
                current_ref,
                &condition_pointer,
                Some(&format!("/assertions/{index}")),
            )?;
            g.edge(
                &condition,
                &node,
                "declares_engineering_obligation",
                current_ref,
                &condition_pointer,
            )?;
        }
        for check in checks {
            let node = match basis
                .assertions
                .iter()
                .position(|a| check.id == format!("engineering:{}", a.assertion_id))
            {
                Some(index) => format!("assertion:{}:{index}", candidate.candidate_id),
                None => {
                    let node = format!("check_basis:{}:{}", candidate.candidate_id, check.id);
                    g.node(
                        &node,
                        "check_basis",
                        &check.conclusion,
                        current_ref,
                        &condition_pointer,
                        None,
                    )?;
                    g.edge(
                        &condition,
                        &node,
                        "checked_by_existing_owner",
                        current_ref,
                        &condition_pointer,
                    )?;
                    node
                }
            };
            native_checks.push(EngineeringReasoningCheck {
                node_id: node,
                object: work.object.clone(),
                check_id: check.id,
                execution: check.execution,
                conclusion: check.conclusion,
                failures: check.failures,
                result: check.result,
            });
        }
        for (field, kind, relation, count) in [
            (
                "necessary_functions",
                "function",
                "requires_function",
                work.necessary_functions.len(),
            ),
            (
                "physical_mechanisms",
                "mechanism",
                "considers_mechanism",
                work.physical_mechanisms.len(),
            ),
        ] {
            for index in 0..count {
                let node = format!("{kind}:{}:{index}", candidate.candidate_id);
                let path = format!("/{field}/{index}");
                g.node(&node, kind, "declared", &candidate.work, &path, None)?;
                g.edge(&id, &node, relation, &candidate.work, &path)?;
            }
        }
        for (j, gap) in work.gaps.iter().enumerate() {
            let gap_id = format!("gap:{}:{}", candidate.candidate_id, gap.gap_id);
            let path = format!("/gaps/{j}");
            g.node(
                &gap_id,
                "gap",
                "open_question",
                &candidate.work,
                &path,
                None,
            )?;
            g.edge(&id, &gap_id, "has_gap", &candidate.work, &path)?;
            for (field, kind) in [
                ("necessary_function", "function"),
                ("physical_mechanism", "mechanism"),
            ] {
                let node = format!("{gap_id}:{kind}");
                let location = format!("{path}/{field}");
                g.node(
                    &node,
                    kind,
                    "declared_gap_context",
                    &candidate.work,
                    &location,
                    None,
                )?;
                g.edge(&gap_id, &node, "concerns", &candidate.work, &location)?;
            }
        }
    }
    for (i, h) in c.plan.hypotheses.iter().enumerate() {
        let id = format!("hypothesis:{}", h.hypothesis_id);
        let p = format!("/hypotheses/{i}");
        g.node(&id, "hypothesis", &h.grounding, &plan_ref, &p, None)?;
        g.edge(
            &id,
            &format!("gap:{}:{}", h.candidate_id, h.gap_id),
            "proposed_explanation_for",
            &plan_ref,
            &format!("{p}/gap_id"),
        )?;
        g.edge(
            &id,
            &format!("decision:{}", h.decision_id),
            "tests_scoped_claim",
            &plan_ref,
            &format!("{p}/decision_id"),
        )?;
        for (j, _) in h.source_refs.iter().enumerate() {
            let node = format!("source:hypothesis:{}:{j}", h.hypothesis_id);
            let pointer = format!("{p}/source_refs/{j}");
            g.node(
                &node,
                "source",
                "bound_reference_not_truth",
                &plan_ref,
                &pointer,
                None,
            )?;
            g.edge(&id, &node, "grounded_in_reference", &plan_ref, &pointer)?;
        }
    }
    let decisions: BTreeSet<String> = actions
        .iter()
        .flat_map(|a| a.affected_decisions.iter().cloned())
        .chain(c.plan.hypotheses.iter().map(|h| h.decision_id.clone()))
        .chain(native_checks.iter().flat_map(|c| {
            c.result["details"]["affected_decisions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
        }))
        .collect();
    for (i, d) in current
        .decisions
        .iter()
        .enumerate()
        .filter(|(_, d)| decisions.contains(&d.id))
    {
        g.node(
            &format!("decision:{}", d.id),
            "decision",
            &d.conclusion,
            current_ref,
            &format!("/decisions/{i}"),
            None,
        )?;
    }
    for check in &native_checks {
        for d in check.result["details"]["affected_decisions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let target = format!("decision:{d}");
            if g.nodes.contains_key(&target) {
                g.edge(
                    &check.node_id,
                    &target,
                    "constrains_decision",
                    current_ref,
                    &format!("/conditions/{}", segment(&check.object.condition_scope)),
                )?;
            }
        }
        let node = &g.nodes[&check.node_id];
        let criterion = node.value["requirements"]["criterion"]
            .as_str()
            .map(str::to_owned);
        if let Some(criterion) = criterion {
            for (i, requirement) in c
                .parent
                .acceptance
                .iter()
                .enumerate()
                .filter(|(_, r)| **r == criterion)
            {
                let _ = requirement;
                g.edge(
                    &check.node_id,
                    &format!("requirement:{i}"),
                    "evaluates_declared_acceptance",
                    current_ref,
                    &format!("/conditions/{}", segment(&check.object.condition_scope)),
                )?;
            }
        }
    }
    for (i, a) in c.plan.actions.iter().enumerate() {
        let id = format!("action:{}", a.action_id);
        let p = format!("/actions/{i}");
        let row = &mut actions[i];
        g.node(&id, "action", &row.status, &plan_ref, &p, None)?;
        g.edge(
            &id,
            &format!("gap:{}:{}", a.candidate_id, a.gap_id),
            "investigates",
            &plan_ref,
            &format!("{p}/gap_id"),
        )?;
        for (index, criterion) in c
            .parent
            .acceptance
            .iter()
            .enumerate()
            .filter(|(_, x)| *x == &a.requirement)
        {
            let _ = criterion;
            g.edge(
                &id,
                &format!("requirement:{index}"),
                "advances_requirement",
                &plan_ref,
                &format!("{p}/requirement"),
            )?;
        }
        for (j, h) in a.hypotheses.iter().enumerate() {
            g.edge(
                &id,
                &format!("hypothesis:{h}"),
                "discriminates",
                &plan_ref,
                &format!("{p}/hypotheses/{j}"),
            )?;
        }
        for (j, outcome) in a.when_outcomes.iter().enumerate() {
            g.edge(
                &format!("outcome:{outcome}"),
                &id,
                "required_before",
                &plan_ref,
                &format!("{p}/when_outcomes/{j}"),
            )?;
        }
        let work = &c.candidates[&a.candidate_id];
        let basis =
            engineering::basis(&current, &work.object)?.context("reasoning_basis_missing")?;
        let related: Vec<_> = native_checks
            .iter()
            .filter(|c| {
                c.object == work.object
                    && (c
                        .node_id
                        .starts_with(&format!("assertion:{}:", a.candidate_id))
                        || c.node_id
                            .starts_with(&format!("check_basis:{}:", a.candidate_id)))
            })
            .collect();
        row.engineering_obligations = Some(related.iter().map(|c| c.node_id.clone()).collect());
        for check in related {
            g.edge(
                &id,
                &check.node_id,
                if check.conclusion == "pass" {
                    "shares_checked_candidate_condition"
                } else {
                    "shares_unresolved_candidate_condition"
                },
                current_ref,
                &format!("/conditions/{}", segment(&work.object.condition_scope)),
            )?;
            if check.conclusion != "pass" {
                g.edge(
                    &check.node_id,
                    &format!("candidate:{}", a.candidate_id),
                    "requires_review_before_adoption",
                    current_ref,
                    &format!("/conditions/{}", segment(&work.object.condition_scope)),
                )?;
            }
        }
        for (j, property) in a.required_facts.iter().enumerate() {
            let (index, fact) = basis
                .facts
                .iter()
                .enumerate()
                .find(|(_, f)| &f.property == property)
                .context("reasoning_fact_unbound")?;
            let node = format!("premise:{}:{index}", a.candidate_id);
            g.node(
                &node,
                "premise",
                &fact.status,
                current_ref,
                &format!("/conditions/{}", segment(&work.object.condition_scope)),
                Some(&format!("/facts/{index}")),
            )?;
            g.edge(
                &id,
                &node,
                "requires_premise",
                &plan_ref,
                &format!("{p}/required_facts/{j}"),
            )?;
        }
        for d in &row.affected_decisions {
            g.edge(
                &id,
                &format!("decision:{d}"),
                "potential_review_via_project_impact",
                current_ref,
                "/dependencies",
            )?;
        }
        if let Some(state) = state {
            if let Some((index, _)) = result["actions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .find(|(_, r)| r["action_id"] == a.action_id && r.get("readback").is_some())
            {
                let node = format!("source:readback:{}", a.action_id);
                let pointer = format!("/actions/{index}/readback");
                g.node(
                    &node,
                    "source",
                    "verified_owner_readback_scope",
                    &state.run.result,
                    &pointer,
                    None,
                )?;
                g.edge(
                    &id,
                    &node,
                    "has_recorded_readback",
                    &state.run.result,
                    &pointer,
                )?;
            }
        }
    }
    for (i, o) in c.plan.outcomes.iter().enumerate() {
        let id = format!("outcome:{}", o.outcome_id);
        let p = format!("/outcomes/{i}");
        g.node(
            &id,
            "outcome",
            if matched.contains(o.outcome_id.as_str()) {
                "uniquely_observed"
            } else {
                "not_established"
            },
            &plan_ref,
            &p,
            None,
        )?;
        g.edge(
            &format!("action:{}", o.action_id),
            &id,
            "may_distinguish",
            &plan_ref,
            &format!("{p}/action_id"),
        )?;
        for (field, ids) in [
            ("supported", &o.supported),
            ("rejected", &o.rejected),
            ("review", &o.review),
        ] {
            for (j, h) in ids.iter().enumerate() {
                g.edge(
                    &id,
                    &format!("hypothesis:{h}"),
                    &format!("if_matched_{field}"),
                    &plan_ref,
                    &format!("{p}/{field}/{j}"),
                )?;
            }
        }
        for (field, ids) in [
            ("retained_candidates", &o.retained_candidates),
            ("rejected_candidates", &o.rejected_candidates),
            ("review_candidates", &o.review_candidates),
        ] {
            for (j, candidate) in ids.iter().enumerate() {
                g.edge(
                    &id,
                    &format!("candidate:{candidate}"),
                    &format!("if_matched_{field}"),
                    &plan_ref,
                    &format!("{p}/{field}/{j}"),
                )?;
            }
        }
    }
    ensure!(
        g.relations
            .iter()
            .all(|e| g.nodes.contains_key(&e.from) && g.nodes.contains_key(&e.to)),
        "reasoning_relation_endpoint_missing"
    );
    let recommended_action = actions
        .iter()
        .find(|a| a.status == "awaiting_host")
        .or_else(|| actions.iter().find(|a| a.status == "ready"))
        .map(|a| a.action_id.clone());
    let view = EngineeringReasoningView {
        schema: VIEW.into(),
        input: input.clone(),
        nodes: g.nodes.into_values().collect(),
        relations: g.relations,
        actions,
        recommended_action,
        complete_acceptance: c.parent.acceptance.clone(),
        uncovered_acceptance: c
            .parent
            .acceptance
            .iter()
            .filter(|x| !c.plan.actions.iter().any(|a| &a.requirement == *x))
            .cloned()
            .collect(),
        sources: references(c),
        checks: Some(native_checks),
        automatic_dispatch: false,
        engineering_adoption: "not_granted".into(),
    };
    dto(view.validate())?;
    ensure!(
        serde_json::to_vec(&view)?.len() <= LIMIT,
        "reasoning_output_byte_budget"
    );
    c.inputs.current(root)?;
    Ok(view)
}

pub fn derive(
    root: &SkillRoot,
    input: &EngineeringReasoningInput,
) -> Result<EngineeringReasoningView> {
    dto(input.validate())?;
    ensure!(input.schema == INPUT, "reasoning_input_schema");
    if let Some(reference) = &input.state {
        let (state, mut c) = host::checked_recorded(root, reference)?;
        project_view(root, &mut c, input, Some(&state))
    } else {
        let mut c = frozen(root, &input.frozen)?;
        project_view(root, &mut c, input, None)
    }
}

pub fn inspect_recorded(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> Result<(
    HostExplorationState,
    Vec<ArtifactReference>,
    EngineeringReasoningView,
)> {
    let (state, mut c) = host::checked_recorded(root, reference)?;
    let input = EngineeringReasoningInput {
        schema: INPUT.into(),
        project: state.run.project.clone(),
        frozen: state.frozen.clone(),
        state: Some(reference.clone()),
    };
    let view = project_view(root, &mut c, &input, Some(&state))?;
    Ok((state, references(&c), view))
}

pub fn verify(root: &SkillRoot, view: &EngineeringReasoningView) -> Result<()> {
    dto(view.validate())?;
    ensure!(
        view.schema == VIEW && derive(root, &view.input)? == *view,
        "reasoning_view_changed"
    );
    Ok(())
}

pub struct ReasoningExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl EngineeringReasoningPort for ReasoningExecutor<'_> {
    fn derive(
        &self,
        input: &EngineeringReasoningInput,
    ) -> std::result::Result<EngineeringReasoningView, FailureRecord> {
        derive(self.root, input).map_err(|e| crate::artifacts::failure("engineering_reasoning", e))
    }
}
