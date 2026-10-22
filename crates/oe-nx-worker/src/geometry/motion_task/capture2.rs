//! Explicit /2 producer orchestration; original /1 execute and checks stay intact.
use super::save_capture2::{SaveCaptureSink2, outer_phase};
use super::*;
use crate::motion_evidence::sink::EvidenceSink;
pub const PARAMS_SCHEMA: &str = "oev2.nx-assembly-motion-params/2";
struct SaveSink2<'a> {
    sink: &'a mut dyn EvidenceSink,
}
impl SaveCaptureSink2 for SaveSink2<'_> {
    fn can_record(&self, value: &NxMotionSaveObservation2) -> Result<bool> {
        self.sink
            .can_append(outer_phase(value)?, &serde_json::to_value(value)?)
    }
    fn record(&mut self, value: &NxMotionSaveObservation2) -> Result<()> {
        self.sink
            .append(outer_phase(value)?, serde_json::to_value(value)?)
    }
}
/// Reuse original execution-side coverage only. Persisted /2 bytes are never
/// rewritten; full identity replay belongs to the separate production receiver.
fn coverage_event(event: &Value) -> Result<Option<Value>> {
    let phase = event["phase"]
        .as_str()
        .context("motion_native_phase_missing")?;
    if matches!(phase, "motion_body_binding" | "interference_identity_after") {
        return Ok(None);
    }
    if matches!(phase, "interference_pending" | "interference_returned") {
        let pair: NxMotionPairObservation2 = serde_json::from_value(event["pair"].clone())?;
        let identity = json!({"pair_index":pair.pair_index,
            "a":{"occurrence_id":pair.a.occurrence_id,"prototype_id":pair.a.prototype_id,"body_tag":pair.a.body_tag},
            "b":{"occurrence_id":pair.b.occurrence_id,"prototype_id":pair.b.prototype_id,"body_tag":pair.b.body_tag}});
        return Ok(Some(if phase == "interference_pending" {
            json!({"phase":phase,"identity":identity})
        } else {
            json!({"phase":phase,"identity":identity,"query_code":pair.query_code,
                "result_raw":pair.result_raw,"cleanup_code":pair.cleanup_code})
        }));
    }
    Ok(Some(event.clone()))
}
pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    parameters: &ArtifactReference,
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
    let mut minimum_events = 7u64; // original events plus explicit capture binding
    for sample in &intent.samples {
        let targets = motion::resolve_position_targets(&initial, budget, sample)?;
        minimum_events = minimum_events
            .checked_add(
                6 + 2 * targets.len() as u64 + plan.bodies.len() as u64 + 3 * plan.pairs_per_sample,
            )
            .context("motion_event_count_overflow")?;
    }
    ensure!(
        evidence.max_events >= minimum_events,
        "motion_evidence_event_budget_below_plan"
    );
    let capture = NxMotionCaptureBinding2 {
        schema: "oev2.nx-motion-capture/2".into(),
        parameters: parameters.clone(),
        binding: binding.clone(),
    };
    let mut sink = w.create_motion_evidence(evidence.clone(), binding)?;
    sink.append("motion_capture", serde_json::to_value(capture)?)?;
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
            if let Some(legacy) = coverage_event(&event)? {
                coverage.observe(&legacy)?;
            }
            Ok(())
        };
        let moved = api.move_assembly_sample2(root, &namespace, budget, sample, &mut record)?;
        motion::check_position_sample(&last, &moved, budget, sample)?;
        check_loaded(&api.loaded_assembly_files(&namespace)?, closure)?;
        let pairs = api.check_assembly_pairs2(
            root,
            &namespace,
            budget,
            &intent.budget,
            intent.samples.len() as u64,
            index as u64,
            &sample.sample_id,
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
    let save = api.save_all_owned_assembly2(&namespace, &mut SaveSink2 { sink: &mut *sink })?;
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
    let result = json!({"action":"run_assembly_motion","capture_schema":"oev2.nx-motion-capture/2","object":intent.object,"intent":reference,
  "samples_checked":intent.samples.len(),"checked_pairs":checked,"interference_pairs":interference,"touching_pairs":touching,
  "no_interference_pairs":checked-interference-touching,"evidence_directory":"out/motion-evidence","evidence":summary,
  "saved_position":{"path":"out/motion-saved-position.json","sha256":sha256(&raw)},"save":save,
  "continuous_sweep_checked":false,"independent_reopen_checked":false,"engineering_adoption":"not_assessed"});
    w.record("motion_task_discrete_checks_complete", result.clone())?;
    Ok(result)
}
