//! Host sessions submit one result for the next frozen action. The existing
//! Project, work, calculation, branch and Harness owners retain their authority.
use super::*;
use oe_contracts::{
    HostExplorationInput, HostExplorationReceipt, HostExplorationState, HostSourceObservation,
};

fn pending(c: &Checked, e: &Evaluation, receipts: &[HostExplorationReceipt]) -> Vec<Value> {
    c.plan.actions.iter().filter(|a| {
        a.kind == "external"
            && !receipts.iter().any(|r| r.action_id == a.action_id)
            && e.actions.iter().any(|r| r["action_id"] == a.action_id && r["selection"] == "selected")
    }).map(|a| {
        let candidate = c.plan.candidates.iter().find(|v| v.candidate_id == a.candidate_id).unwrap();
        let work = &c.candidates[&a.candidate_id];
        json!({"action":a,"object":work.object,"project":work.project,"work":candidate.work,
            "conditions":c.project.conditions[&work.object.condition_scope],
            "criteria":c.plan.outcomes.iter().filter(|o|o.action_id==a.action_id).collect::<Vec<_>>(),
            "acceptance":c.parent.acceptance,"remaining_obligations":candidate.remaining_obligations,
            "hypotheses":c.plan.hypotheses.iter().filter(|h|a.hypotheses.contains(&h.hypothesis_id)).collect::<Vec<_>>(),
            "execution":"not_run","authorization":"existing_tool_and_user_authorization_still_required",
            "supported_receipt_owners":["project_work","source_statement"],
            "project_work_entry":["project","work","--input",candidate.work.path,"--output-dir","NEW_PRIVATE_DIRECTORY"],
            "source_statement_scope":"bound source statements only; no native or physical certification"})
    }).collect()
}

fn readbacks(
    root: &SkillRoot,
    c: &mut Checked,
    receipts: &[HostExplorationReceipt],
    frozen: &ArtifactReference,
) -> Result<BTreeMap<String, Value>> {
    ensure!(receipts.len() <= 16, "exploration_host_receipt_budget");
    let mut rows = BTreeMap::new();
    for r in receipts {
        dto(r.validate())?;
        let a = c
            .plan
            .actions
            .iter()
            .find(|a| a.action_id == r.action_id && a.kind == "external")
            .context("exploration_host_action_unbound")?;
        let work = &c.candidates[&a.candidate_id];
        let bytes = c.inputs.get(root, &r.artifact)?;
        let value = strict_json(&bytes)?;
        let raw = match r.owner.as_str() {
            "project_work" => {
                // This adapter verifies the original deterministic owner again.
                // Other work (retrieval/CAD) needs its own result adapter.
                ensure!(
                    work.source_requests.is_empty()
                        && work.handoff.is_none()
                        && !work.calculations.is_empty(),
                    "exploration_host_work_adapter_scope"
                );
                ensure!(
                    value["schema"] == "oev2.engineering-work-result/1"
                        && value["project_reference"] == json!(work.project)
                        && value["project_id"] == c.project.project_id
                        && value["project_revision"] == c.project.revision
                        && value["object"] == json!(work.object)
                        && value["acceptance"] == json!(work.acceptance)
                        && value["necessary_functions"] == json!(work.necessary_functions)
                        && value["physical_mechanisms"] == json!(work.physical_mechanisms)
                        && value["source_results"] == json!([])
                        && value["adopted"] == false,
                    "exploration_host_work_binding_mismatch"
                );
                let checks = engineering::checks_current(root, &c.project, &work.object);
                let calculations: Vec<_> = work.calculations.iter().map(|calc| {
                    let check = engineering::calculation_check(&c.project, calc);
                    let mut v = match super::super::calculate(calc) {
                        Ok(v) => v,
                        Err(e) => json!({"calculation_id":calc.calculation_id,"execution":"failed","conclusion":"unknown","reason":e.to_string(),"engineering_adoption":"not_established"}),
                    };
                    v["current_support"] = json!(if check.conclusion == "pass" {"declared_model_applicable"} else {"not_established"});
                    v["model_applicability"] = json!(check);
                    v
                }).collect();
                let partial = checks.iter().any(|r| r.conclusion != "pass")
                    || calculations.iter().any(|r| {
                        r["execution"] != "completed"
                            || r["model_applicability"]["conclusion"] != "pass"
                    });
                ensure!(
                    value["calculations"] == json!(calculations)
                        && value["engineering_checks"] == json!(checks)
                        && value["status"]
                            == if partial {
                                "partial"
                            } else {
                                "analysis_completed"
                            },
                    "exploration_host_work_readback_mismatch"
                );
                json!({"execution":if partial {"partial"} else {"completed"},"decision_support":!partial,
                    "observation":value,"source":r.artifact,"authority":"original_declared_model_checks_reexecuted_not_physical_confirmation"})
            }
            "source_statement" => {
                let observation: HostSourceObservation = serde_json::from_value(value)?;
                dto(observation.validate())?;
                ensure!(
                    json!(observation.frozen) == json!(frozen)
                        && observation.action_id == a.action_id
                        && json!(observation.project) == json!(work.project)
                        && observation.object == work.object
                        && !observation.sources.is_empty()
                        && observation.sources.len() <= 16
                        && stated(&observation.limitations),
                    "exploration_host_observation_unbound"
                );
                for source in &observation.sources {
                    c.inputs.get(root, source)?;
                }
                // A host-authored source report cannot certify a model or cause.
                let statement_only = c
                    .plan
                    .hypotheses
                    .iter()
                    .filter(|h| a.hypotheses.contains(&h.hypothesis_id))
                    .all(|h| h.kind == "source_statement");
                json!({"execution":observation.execution,
                    "decision_support":observation.execution == "completed" && statement_only,
                    "observation":observation.observation,"source":r.artifact,
                    "sources":observation.sources,"limitations":observation.limitations,
                    "authority":"host_recorded_source_statement_not_independent_tool_or_physical_verification"})
            }
            _ => anyhow::bail!("exploration_host_receipt_owner_unsupported"),
        };
        ensure!(
            rows.insert(r.action_id.clone(), raw).is_none(),
            "exploration_host_duplicate_receipt"
        );
    }
    Ok(rows)
}

fn stage(e: &Evaluation, pending: &[Value], harness: &Value) -> &'static str {
    if !pending.is_empty() {
        "requires_host_agent"
    } else if e
        .actions
        .iter()
        .any(|a| a["selection"] == "selected" && a["disposition"] != "distinguished")
        || harness["schema"] != "oev2.harness-checkpoint/1"
    {
        "needs_review"
    } else {
        "results_recorded"
    }
}

// Reconstruct history from the original requests and receipts. Serialized
// "completed"/"pending" labels do not themselves authorize advancement.
fn previous(
    root: &SkillRoot,
    c: &mut Checked,
    reference: &ArtifactReference,
    frozen: &ArtifactReference,
    depth: usize,
) -> Result<Vec<HostExplorationReceipt>> {
    ensure!(depth <= 16, "exploration_host_history_budget");
    let state: HostExplorationState =
        serde_json::from_value(strict_json(&c.inputs.get(root, reference)?)?)?;
    dto(state.validate())?;
    ensure!(
        json!(state.frozen) == json!(frozen)
            && !state.adopted
            && !state.run.adopted
            && !state.run.original_project_mutated,
        "exploration_host_history_scope_changed"
    );
    let request: HostExplorationInput =
        serde_json::from_value(strict_json(&c.inputs.get(root, &state.request)?)?)?;
    dto(request.validate())?;
    ensure!(
        json!(request.frozen) == json!(frozen)
            && json!(request.previous) == json!(state.previous)
            && root.data_expand(Path::new(&reference.path))?
                == root
                    .data_expand(Path::new(&request.output))?
                    .join("completed.json"),
        "exploration_host_history_request_changed"
    );
    let receipts = extend(root, c, &request, depth + 1)?;
    ensure!(
        json!(state.receipts) == json!(receipts),
        "exploration_host_history_receipts_changed"
    );
    let rows = readbacks(root, c, &receipts, frozen)?;
    let mut e = evaluate(root, c, Some(&rows))?;
    let pending = pending(c, &e, &receipts);
    let mut contents = BTreeMap::new();
    for (name, r) in [
        ("results.json", &state.run.result),
        ("project.json", &state.run.project),
        ("work.json", &state.run.work),
        ("task-view.json", &state.run.task_view),
        ("harness.json", &state.run.harness),
    ] {
        ensure!(
            root.data_expand(Path::new(&r.path))?
                == root
                    .data_expand(Path::new(&request.output))?
                    .join("run")
                    .join(name),
            "exploration_host_history_artifact_location_changed"
        );
        contents.insert(name, strict_json(&c.inputs.get(root, r)?)?);
    }
    let mut actions = e.actions.clone();
    let mut recorded = contents["results.json"]["actions"]
        .as_array()
        .context("exploration_host_history_actions_missing")?
        .clone();
    for a in actions.iter_mut().chain(recorded.iter_mut()) {
        if let Some(v) = a.as_object_mut() {
            v.remove("actual_local_elapsed_ns");
        }
    }
    e.updated.evidence.insert(
        format!("exploration:{}", c.plan.exploration_id),
        state.run.result.sha256.clone(),
    );
    e.updated.revision = format!(
        "{}:exploration:{}",
        c.project.revision, c.plan.exploration_id
    );
    let mut expected_work = c.parent.clone();
    expected_work.project = Reference {
        path: state.run.project.path.clone().into(),
        sha256: state.run.project.sha256.clone(),
    };
    expected_work.project_context = Some(project::context(&e.updated, &expected_work.object)?);
    ensure!(
        contents["results.json"]["schema"] == "oev2.exploration-results/2"
            && actions == recorded
            && contents["project.json"] == json!(e.updated)
            && contents["work.json"] == json!(expected_work)
            && state.pending == pending
            && state.status == stage(&e, &pending, &contents["harness.json"]),
        "exploration_host_history_replay_mismatch"
    );
    Ok(receipts)
}

fn extend(
    root: &SkillRoot,
    c: &mut Checked,
    request: &HostExplorationInput,
    depth: usize,
) -> Result<Vec<HostExplorationReceipt>> {
    let mut receipts = if let Some(p) = &request.previous {
        previous(root, c, p, &request.frozen, depth)?
    } else {
        ensure!(
            request.receipt.is_none(),
            "exploration_host_initial_result_before_request"
        );
        vec![]
    };
    if request.previous.is_some() {
        let receipt = request
            .receipt
            .as_ref()
            .context("exploration_host_result_required")?;
        let rows = readbacks(root, c, &receipts, &request.frozen)?;
        let e = evaluate(root, c, Some(&rows))?;
        let next = pending(c, &e, &receipts);
        ensure!(
            next.len() == 1 && next[0]["action"]["action_id"] == receipt.action_id,
            "exploration_host_result_not_pending"
        );
        receipts.push(receipt.clone());
    }
    Ok(receipts)
}

/// This entry performs bounded readback/calculation only. It never dispatches
/// a model, browser, supplier, CAD operation or automatic retry.
pub fn advance(root: &SkillRoot, input: &ArtifactReference) -> Result<HostExplorationState> {
    let bytes = read(root, input)?;
    let request: HostExplorationInput = serde_json::from_value(strict_json(&bytes)?)?;
    dto(request.validate())?;
    let mut c = frozen(root, &request.frozen)?;
    c.inputs.get(root, input)?;
    let receipts = extend(root, &mut c, &request, 1)?;
    let rows = readbacks(root, &mut c, &receipts, &request.frozen)?;
    let e = evaluate(root, &c, Some(&rows))?;
    let pending = pending(&c, &e, &receipts);
    c.inputs.current(root)?;
    let dir = root.new_private_dir(Path::new(&request.output))?;
    let run = execute_with(
        root,
        &EngineeringExplorationRunRequest {
            frozen: request.frozen.clone(),
            output: root.portable_data_path(&dir.join("run"))?,
        },
        Some(&rows),
    )?;
    let harness = strict_json(&read(root, &run.harness)?)?;
    let state = HostExplorationState {
        schema: "oev2.exploration-host-state/1".into(),
        frozen: request.frozen,
        previous: request.previous,
        request: input.clone(),
        receipts,
        run,
        status: stage(&e, &pending, &harness).into(),
        pending,
        adopted: false,
        method_benefit: "unknown".into(),
    };
    dto(state.validate())?;
    c.inputs.current(root)?;
    write_new(&dir.join("completed.json"), &json!(state))?;
    Ok(state)
}
