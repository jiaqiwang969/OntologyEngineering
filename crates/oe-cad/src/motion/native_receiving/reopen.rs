use super::*;

/// Re-run original motion reception before comparing a separately dispatched,
/// read-only reopening of those exact saved files. A prior report is not proof.
pub fn receive_reopen(root: &SkillRoot, reference: &ArtifactReference) -> Result<Value> {
    let mut sources = Sources::new(root);
    let v = oe_core::json::strict_json(&sources.read(reference, RECORD)?)?;
    exact(
        &v,
        &["schema", "motion_request", "reopen", "requested_claims"],
    )?;
    job_source_fields(&v["reopen"])?;
    let request: CadMotionReopenRequest1 = serde_json::from_value(v)?;
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_reopen_request_invalid"))?;
    claims(&request.requested_claims)?;
    let (mut report, motion_request, before, input) =
        receive_inner(&mut sources, &request.motion_request)?;
    report.request = reference.clone();
    report.evidence_refs.extend([
        pointer(reference),
        pointer(&request.reopen.job),
        pointer(&request.reopen.fetch_snapshot),
    ]);
    if let Some(r) = &request.reopen.fetch_completion {
        report.evidence_refs.push(pointer(r));
    }
    ensure!(
        report.save_state == "received" && before.completed(),
        "motion_reopen_requires_received_save"
    );
    let after = source::receive(&mut sources, &request.reopen)?;
    let mut context = motion_request.clone();
    context.execution = request.reopen.clone();
    project_result_binding(&mut sources, &context, &after)?;
    ensure!(
        after.job.action == "inspect_assembly_positions"
            && after.job.params_schema == "oev2.nx-assembly-position-reopen-params/1",
        "motion_reopen_action_or_schema_differs"
    );
    let a = before
        .result
        .as_ref()
        .context("motion_saved_result_missing")?;
    ensure!(
        before.job.job_id != after.job.job_id && before.job.worker == after.job.worker,
        "motion_reopen_job_or_tool_identity_differs"
    );
    if let Some(process) = &after.state.process {
        ensure!(
            (
                a.process.host_id.as_str(),
                a.process.pid,
                a.process.creation_time_100ns
            ) != (
                process.host_id.as_str(),
                process.pid,
                process.creation_time_100ns
            ),
            "motion_reopen_same_process"
        );
    }
    let params: oe_nx_worker::geometry::Parameters = decode(&after.bytes("params.json", RECORD)?)?;
    params.validate()?;
    let oe_nx_worker::geometry::Parameters::InspectAssemblyPositions {
        source,
        expected,
        budget,
        ..
    } = params
    else {
        anyhow::bail!("motion_reopen_parameters_action_differs")
    };
    ensure!(
        serde_json::to_value(&budget)? == serde_json::to_value(&input.params.budget)?,
        "motion_reopen_budget_differs"
    );
    let closure: NxAssemblySourceClosure = decode(&after.input(&source, 4 * 1024 * 1024)?)?;
    motion_source::validate_source_closure(&closure)?;
    ensure!(
        closure.root_part == input.closure.root_part
            && closure.files.len() == input.closure.files.len(),
        "motion_reopen_source_inventory_differs"
    );
    let before_names: BTreeSet<_> = input
        .closure
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    for f in &closure.files {
        ensure!(
            before_names.contains(f.path.as_str()),
            "motion_reopen_unrelated_source"
        );
        let saved_name = format!("out/motion-assembly/{}", f.path);
        let saved = before
            .snapshot
            .files
            .iter()
            .find(|v| v.entry.path.as_str() == saved_name)
            .context("motion_saved_member_missing")?;
        let input_name = format!("inputs/{}", f.path);
        let reopened = after
            .job
            .inputs
            .iter()
            .find(|v| v.path.as_str() == input_name)
            .context("motion_reopen_input_missing")?;
        ensure!(
            saved.entry.identity == reopened.identity
                && saved.entry.identity.bytes == f.bytes
                && saved.entry.identity.sha256.as_str() == f.sha256,
            "motion_reopen_not_exact_saved_bytes"
        );
    }
    let expected_raw = after.input(&expected, 32 * 1024 * 1024)?;
    let saved_raw = before.bytes("out/motion-saved-position.json", 32 * 1024 * 1024)?;
    ensure!(
        expected_raw == saved_raw,
        "motion_reopen_expected_not_saved_position"
    );
    let saved: NxAssemblyPositionObservation = decode(&saved_raw)?;
    if after.completed() {
        let r = after
            .readback
            .as_ref()
            .context("motion_reopen_readback_missing")?;
        ensure!(
            r["action"] == "inspect_assembly_positions"
                && r["source"] == serde_json::to_value(&source)?
                && r["expected_position"] == serde_json::to_value(&expected)?
                && r["save_requested"] == false
                && r["motion_requested"] == false
                && r["private_copy_bytes_unchanged"] == true
                && r["whole_topology_checked"] == false
                && r["engineering_adoption"] == "not_assessed",
            "motion_reopen_scope_differs"
        );
        let loaded: Vec<String> = serde_json::from_value(r["loaded_parts"].clone())?;
        oe_nx_worker::geometry::motion_task::check_loaded(&loaded, &closure)?;
        let actual: NxAssemblyPositionObservation =
            serde_json::from_value(r["observation"].clone())?;
        oe_nx_worker::assembly_position::persistence::compare(&saved, &actual, &budget)?;
        // Independently check the claimed read-only native copy's output bytes.
        for f in &closure.files {
            let name = format!("out/motion-assembly/{}", f.path);
            let copied = after
                .snapshot
                .files
                .iter()
                .find(|v| v.entry.path.as_str() == name)
                .context("motion_reopen_copy_missing")?;
            ensure!(
                copied.entry.identity.bytes == f.bytes
                    && copied.entry.identity.sha256.as_str() == f.sha256,
                "motion_reopen_readonly_copy_changed"
            );
        }
        report.reopen_state = "received".into();
        report
            .received_scope
            .push("independent_reopen_bounded_positions_and_geometry".into());
        if !after.fetch_complete {
            report.outcome = "partial".into();
            report.issues.push("reopen fetch completion absent".into());
        }
    } else {
        report.reopen_state = "unknown".into();
        report.outcome = "partial".into();
        report
            .issues
            .push("reopen execution incomplete; preserved without retry".into());
    }
    scope(&mut report, &request.requested_claims);
    before.recheck()?;
    after.recheck()?;
    sources.recheck()?;
    report
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_reopen_report_invalid"))?;
    Ok(serde_json::to_value(report)?)
}
