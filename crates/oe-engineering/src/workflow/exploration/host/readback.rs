//! Recheck a host result without writing a new run or dispatching an action.
use super::*;

fn elapsed(value: &Value) -> Result<&str> {
    let text = value
        .as_str()
        .context("exploration_readback_elapsed_missing")?;
    ensure!(
        text.parse::<u128>().is_ok(),
        "exploration_readback_elapsed_invalid"
    );
    Ok(text)
}

fn saved(root: &SkillRoot, c: &mut Checked, path: &Path) -> Result<Value> {
    let r = reference(root, path)?;
    Ok(strict_json(&c.inputs.get(root, &r)?)?)
}

fn snapshot(root: &SkillRoot, c: &mut Checked, state: &HostExplorationState) -> Result<()> {
    ensure!(
        state.method_benefit == "unknown",
        "exploration_readback_method_claim_changed"
    );
    let request: HostExplorationInput =
        serde_json::from_value(strict_json(&c.inputs.get(root, &state.request)?)?)?;
    let run_dir = root.data_expand(Path::new(&request.output))?.join("run");
    let result = strict_json(&c.inputs.get(root, &state.run.result)?)?;
    let rows = readbacks(root, c, &state.receipts, &state.frozen)?;
    let e = evaluate(root, c, Some(&rows))?;
    let mut expected = result_value(c, &e, true, elapsed(&result["cost"]["local_elapsed_ns"])?)?;
    let actual_actions = result["actions"]
        .as_array()
        .context("exploration_readback_actions_missing")?;
    let expected_actions = expected["actions"].as_array_mut().unwrap();
    ensure!(
        actual_actions.len() == expected_actions.len(),
        "exploration_readback_actions_changed"
    );
    // Runtime duration is a retained measurement, not a reproducible oracle.
    // All non-timing fields must equal the deterministic native readback.
    for (actual, expected) in actual_actions.iter().zip(expected_actions) {
        if expected.get("actual_local_elapsed_ns").is_some() {
            expected["actual_local_elapsed_ns"] =
                json!(elapsed(&actual["actual_local_elapsed_ns"])?);
        }
    }
    ensure!(result == expected, "exploration_readback_result_changed");
    ensure!(
        saved(root, c, &run_dir.join("completed.json"))? == json!(state.run),
        "exploration_readback_run_changed"
    );
    ensure!(
        saved(root, c, &run_dir.join("assessment.json"))? == assessment_host(&c.expected, &result)?,
        "exploration_readback_assessment_changed"
    );

    let view: oe_contracts::CurrentTaskView =
        serde_json::from_value(strict_json(&c.inputs.get(root, &state.run.task_view)?)?)?;
    let expected_view_input = CurrentTaskViewInput {
        schema: "oev2.current-task-view-input/1".into(),
        current: state.run.project.clone(),
        previous: Some(ArtifactReference {
            path: c.parent.project.path.to_string_lossy().into(),
            sha256: c.parent.project.sha256.clone(),
        }),
        work: Some(state.run.work.clone()),
        sources: vec![state.run.result.clone()],
    };
    ensure!(
        view.input == expected_view_input,
        "exploration_readback_view_binding_changed"
    );
    crate::current_task_view::verify_view(root, &view)?;

    let project: Project =
        serde_json::from_value(strict_json(&c.inputs.get(root, &state.run.project)?)?)?;
    let work: WorkInput =
        serde_json::from_value(strict_json(&c.inputs.get(root, &state.run.work)?)?)?;
    let expected_input = harness_input_value(
        c,
        &state.frozen,
        &state.run.result,
        &state.run.project,
        &project,
        &work,
    );
    ensure!(
        saved(root, c, &run_dir.join("harness-input.json"))? == expected_input,
        "exploration_readback_harness_input_changed"
    );
    let summary = strict_json(&c.inputs.get(root, &state.run.harness)?)?;
    let harness_dir = run_dir.join("harness");
    let checkpoint_ref = reference(root, &harness_dir.join("iteration.json"))?;
    c.inputs.get(root, &checkpoint_ref)?;
    let checkpoint = crate::harness::load_step(root, &json!(checkpoint_ref))?;
    ensure!(
        checkpoint == summary && checkpoint["task_id"] == c.plan.exploration_id,
        "exploration_readback_harness_changed"
    );
    ensure!(
        saved(root, c, &harness_dir.join("input.json"))? == expected_input,
        "exploration_readback_harness_archive_input_changed"
    );
    for (name, digest) in checkpoint["artifacts"]
        .as_object()
        .context("exploration_readback_harness_artifacts_missing")?
    {
        oe_semantic::safe_relative(name)?;
        let r = reference(root, &harness_dir.join(name))?;
        ensure!(
            json!(r.sha256) == *digest,
            "exploration_readback_harness_artifact_changed"
        );
        c.inputs.get(root, &r)?;
    }
    Ok(())
}

/// Reconstruct the frozen action/receipt history and verify every retained
/// result, current task view and complete Harness archive through its owner.
/// Returned references are project artifacts; the installed schema is checked
/// independently at the software root. This does not authenticate host reports,
/// replay elapsed time, certify physical results or grant method improvement.
pub(in crate::workflow::exploration) fn checked_recorded(
    root: &SkillRoot,
    artifact: &ArtifactReference,
) -> Result<(HostExplorationState, Checked)> {
    let state: HostExplorationState = serde_json::from_value(strict_json(&read(root, artifact)?)?)?;
    dto(state.validate())?;
    let mut c = frozen(root, &state.frozen)?;
    previous(root, &mut c, artifact, &state.frozen, 0)?;
    let mut next = Some(artifact.clone());
    let mut count = 0;
    while let Some(r) = next {
        count += 1;
        ensure!(count <= 17, "exploration_host_history_budget");
        let node: HostExplorationState =
            serde_json::from_value(strict_json(&c.inputs.get(root, &r)?)?)?;
        snapshot(root, &mut c, &node)?;
        next = node.previous;
    }
    c.inputs.current(root)?;
    Ok((state, c))
}

pub fn verify_recorded(
    root: &SkillRoot,
    artifact: &ArtifactReference,
) -> Result<(HostExplorationState, Vec<ArtifactReference>)> {
    let (state, c) = checked_recorded(root, artifact)?;
    let references = c
        .inputs
        .bytes
        .iter()
        .map(|(path, raw)| ArtifactReference {
            path: path.clone(),
            sha256: sha256(raw),
        })
        .collect();
    Ok((state, references))
}
