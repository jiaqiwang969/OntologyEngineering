//! Explicit task orchestration. Discrete native checks are not continuous clearance.
use super::{NativeApi, Workspace, input_relative};
use crate::{assembly_motion as motion, assembly_position as position, motion_source};
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use oe_core::{json::strict_json, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io::Write};
pub mod capture2;
pub mod coverage;
pub mod save_capture2;
pub const PARAMS_SCHEMA: &str = "oev2.nx-assembly-motion-params/1";
pub fn reference_valid(r: &ArtifactReference) -> Result<()> {
    r.validate()
        .map_err(|_| anyhow::anyhow!("motion_artifact_invalid"))?;
    ensure!(
        !r.path.starts_with("inputs/"),
        "motion_unprefixed_input_required"
    );
    input_relative(&r.path)?;
    oe_nx_protocol::Digest::new(r.sha256.clone())?;
    Ok(())
}
pub(super) fn read<T: serde::de::DeserializeOwned>(
    w: &dyn Workspace,
    r: &ArtifactReference,
    limit: usize,
) -> Result<T> {
    reference_valid(r)?;
    let path = w.input(&input_relative(&r.path)?)?;
    let raw = read_regular_bytes(&path, limit)?;
    ensure!(sha256(&raw) == r.sha256, "motion_artifact_changed");
    Ok(serde_json::from_value(strict_json(&raw)?)?)
}
pub fn source(
    w: &dyn Workspace,
    r: &ArtifactReference,
) -> Result<(NxAssemblyMotionIntent, NxAssemblySourceClosure)> {
    let intent: NxAssemblyMotionIntent = read(w, r, 32 * 1024 * 1024)?;
    motion::validate_intent(&intent)?;
    let closure: NxAssemblySourceClosure = read(w, &intent.assembly_source, 4 * 1024 * 1024)?;
    motion_source::validate_source_closure(&closure)?;
    ensure!(
        intent.root_part == closure.root_part,
        "motion_root_differs_from_source"
    );
    for f in &closure.files {
        input_relative(&f.path)?;
        ensure!(
            f.path.to_ascii_lowercase().ends_with(".prt"),
            "motion_source_native_part_required"
        );
    }
    Ok((intent, closure))
}
/// Includes loaded non-placement dependencies. Extra declared sources are not
/// claimed to be loaded; every actual loaded part must be declared and owned.
pub fn check_loaded(files: &[String], closure: &NxAssemblySourceClosure) -> Result<()> {
    motion_source::validate_source_closure(closure)?;
    let declared: BTreeSet<_> = closure.files.iter().map(|f| f.path.as_str()).collect();
    let mut seen = BTreeSet::new();
    ensure!(
        !files.is_empty() && files.len() <= 4096,
        "motion_loaded_parts_invalid"
    );
    for name in files {
        ensure!(
            seen.insert(name.as_str()) && declared.contains(name.as_str()),
            "motion_loaded_part_undeclared_or_duplicate"
        );
    }
    ensure!(
        seen.contains(closure.root_part.as_str()),
        "motion_loaded_root_missing"
    );
    Ok(())
}
fn unchanged(
    a: &NxAssemblyPositionObservation,
    b: &NxAssemblyPositionObservation,
    budget: &NxAssemblyBudget,
) -> Result<()> {
    let av = position::checked_inventory(a, budget)?;
    let bv = position::checked_inventory(b, budget)?;
    crate::assembly::same_geometry(&av, &bv)?;
    ensure!(
        a.root_occurrence_tag == b.root_occurrence_tag
            && serde_json::to_value(&a.position_overrides)?
                == serde_json::to_value(&b.position_overrides)?,
        "motion_save_position_metadata_changed"
    );
    Ok(())
}
pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    reference: &ArtifactReference,
    intent: &NxAssemblyMotionIntent,
    closure: &NxAssemblySourceClosure,
    budget: &NxAssemblyBudget,
    evidence: &NxMotionEvidenceBudget,
) -> Result<Value> {
    motion::validate_intent(intent)?;
    crate::assembly::budget_valid(budget)?;
    let binding = NxMotionEvidenceBinding {
        job_id: w.motion_job_id()?,
        object: intent.object.clone(),
        intent: reference.clone(),
        samples: intent.samples.len() as u64,
    };
    crate::motion_evidence::validate_stream_inputs(evidence, &binding)?;
    validate_delivery(evidence, closure, w.motion_output_bounds()?)?;
    let (root_path, namespace) = w.prepare_motion_source(closure)?;
    let root = api.open_part(&root_path)?;
    let loaded = api.loaded_assembly_files(&namespace)?;
    check_loaded(&loaded, closure)?;
    api.prepare_owned_assembly_context(root, &namespace)?;
    let initial = api.assembly_positions(root, &namespace, budget, "reference_observation")?;
    ensure!(
        initial.root_prototype_id == intent.root_part,
        "motion_native_root_differs"
    );
    let plan = motion::plan_position_pairs(&initial, budget, &intent.budget, binding.samples)?;
    let mut minimum_events = 6u64; // open/seal, initial, save pending/returned, saved position
    for sample in &intent.samples {
        let targets = motion::resolve_position_targets(&initial, budget, sample)?;
        minimum_events = minimum_events
            .checked_add(6 + 2 * targets.len() as u64 + 2 * plan.pairs_per_sample)
            .context("motion_event_count_overflow")?;
    }
    ensure!(
        evidence.max_events >= minimum_events,
        "motion_evidence_event_budget_below_plan"
    );
    let mut sink = w.create_motion_evidence(evidence.clone(), binding)?;
    sink.append("motion_initial",json!({"observation":initial,"loaded_parts":loaded,"samples":intent.samples.len(),"pairs_per_sample":plan.pairs_per_sample,"minimum_events":minimum_events}))?;
    let mut last = initial;
    let mut checked = 0u64;
    let mut interference = 0u64;
    let mut touching = 0u64;
    for (index, sample) in intent.samples.iter().enumerate() {
        check_loaded(&api.loaded_assembly_files(&namespace)?, closure)?;
        let targets = motion::resolve_position_targets(&last, budget, sample)?;
        let expected_targets = targets.iter().map(|t| t.occurrence_id.clone()).collect();
        let mut coverage = coverage::SampleCoverage::new(
            sample.sample_id.clone(),
            expected_targets,
            plan.pairs_per_sample,
        );
        sink.append(
            "motion_sample_start",
            json!({"sample_index":index,"sample_id":sample.sample_id,"time_s":sample.time_s}),
        )?;
        let mut record = |event: Value| -> Result<()> {
            let phase = event["phase"]
                .as_str()
                .context("motion_native_phase_missing")?
                .to_owned();
            // Persist even a failing returned code before interpreting it.
            sink.append(&phase,json!({"sample_index":index,"sample_id":sample.sample_id,"time_s":sample.time_s,"event":event}))?;
            coverage.observe(&event)
        };
        let moved = api.move_assembly_sample(root, &namespace, budget, sample, &mut record)?;
        motion::check_position_sample(&last, &moved, budget, sample)?;
        check_loaded(&api.loaded_assembly_files(&namespace)?, closure)?;
        let pairs = api.check_assembly_pairs(
            root,
            &namespace,
            budget,
            &intent.budget,
            intent.samples.len() as u64,
            &mut record,
        )?;
        drop(record);
        coverage.finish(&pairs)?;
        checked += coverage.pairs;
        interference += coverage.interference;
        touching += coverage.touching;
        sink.append(
            "motion_sample_checked",
            json!({"sample_index":index,"sample_id":sample.sample_id,"pairs":pairs}),
        )?;
        last = moved;
    }
    ensure!(
        checked == plan.total_pair_checks,
        "motion_total_pair_coverage_differs"
    );
    check_loaded(&api.loaded_assembly_files(&namespace)?, closure)?;
    sink.append("motion_save_pending", json!({"namespace":namespace}))?;
    let save = api.save_all_owned_assembly(&namespace)?;
    sink.append("motion_save_returned", save.clone())?;
    let saved = api.assembly_positions(root, &namespace, budget, "reference_observation")?;
    unchanged(&last, &saved, budget)?;
    check_loaded(&api.loaded_assembly_files(&namespace)?, closure)?;
    sink.append("motion_saved_position", serde_json::to_value(&saved)?)?;
    let summary = sink.seal_and_verify()?;
    drop(sink);
    w.verify_motion_source()?;
    let raw = serde_json::to_vec_pretty(&saved)?;
    let output = w.output("motion-saved-position.json")?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    f.write_all(&raw)?;
    f.sync_all()?;
    drop(f);
    ensure!(
        w.require_regular_output("motion-saved-position.json")? == raw,
        "motion_saved_position_output_changed"
    );
    let result = json!({"action":"run_assembly_motion","object":intent.object,"intent":reference,
  "samples_checked":intent.samples.len(),"checked_pairs":checked,"interference_pairs":interference,"touching_pairs":touching,
  "no_interference_pairs":checked-interference-touching,"evidence_directory":"out/motion-evidence","evidence":summary,
  "saved_position":{"path":"out/motion-saved-position.json","sha256":sha256(&raw)},"save":save,
  "continuous_sweep_checked":false,"independent_reopen_checked":false,"engineering_adoption":"not_assessed"});
    w.record("motion_task_discrete_checks_complete", result.clone())?;
    Ok(result)
}

/// A preflight bound on planned evidence and unchanged source copies. Native
/// saves may grow files; complete actual outputs still require final job inventory.
pub fn validate_delivery(
    evidence: &NxMotionEvidenceBudget,
    closure: &NxAssemblySourceClosure,
    bounds: (u64, u64, usize),
) -> Result<()> {
    motion_source::validate_source_closure(closure)?;
    let (total, file, count) = bounds;
    ensure!(
        total > 0 && file > 0 && file <= total && count > 0,
        "motion_delivery_capacity_invalid"
    );
    ensure!(
        evidence.segment_bytes <= file,
        "motion_evidence_segment_exceeds_job_file_budget"
    );
    let mut source_bytes = 0u64;
    for part in &closure.files {
        ensure!(
            part.bytes <= file,
            "motion_source_copy_exceeds_job_file_budget"
        );
        source_bytes = source_bytes
            .checked_add(part.bytes)
            .context("motion_delivery_size_overflow")?;
    }
    let planned = evidence
        .max_bytes
        .checked_add(source_bytes)
        .context("motion_delivery_size_overflow")?;
    // Leave some capacity for saved-position metadata. Its actual bytes, CAD save
    // growth and all other outputs are checked together by the final inventory.
    ensure!(
        planned < total,
        "motion_planned_bytes_exceed_job_output_budget"
    );
    let files = (closure.files.len() as u64)
        .checked_add(evidence.max_segments)
        .and_then(|n| n.checked_add(1))
        .context("motion_delivery_count_overflow")?;
    ensure!(
        files <= count as u64,
        "motion_planned_files_exceed_job_output_budget"
    );
    Ok(())
}
