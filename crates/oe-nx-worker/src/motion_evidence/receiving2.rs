//! Bounded semantic receiving of actual /2 evidence. Caller pins the original
//! task, parameters, source closure, SDK receipt and files; this module never
//! executes native code or adopts an engineering claim.
mod facts;
mod prefix;
pub mod save;
mod state;
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
pub use save::SaveState;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};

/// Values from the actual qualified worker ABI receipt, never JSON task knobs.
#[derive(Debug, Clone, Copy)]
pub struct NativeBodyAbi {
    pub solid_type: i32,
    pub solid_body: i32,
}

pub struct Inputs2<'a> {
    pub parameters: &'a ArtifactReference,
    pub params: &'a NxAssemblyMotionParams2,
    pub intent: &'a NxAssemblyMotionIntent,
    pub closure: &'a NxAssemblySourceClosure,
    pub binding: &'a NxMotionEvidenceBinding,
    pub receiver_budget: &'a NxMotionEvidenceBudget,
    pub abi: NativeBodyAbi,
    /// Actual owned native path -> declared relative source path. The outer
    /// adapter derives this from the verified job workspace, never event claims.
    pub owned_paths: &'a BTreeMap<String, String>,
    pub completed: bool,
    pub saved_position: Option<&'a NxAssemblyPositionObservation>,
}

#[derive(Debug)]
pub struct Reception2 {
    pub outcome: &'static str,
    pub stream_state: &'static str,
    pub motion_state: &'static str,
    pub pair_identity_state: &'static str,
    pub query_state: &'static str,
    pub samples: u64,
    pub pairs: u64,
    pub interference: u64,
    pub no_interference: u64,
    pub touching: u64,
    pub save_state: SaveState,
    /// Actual saved event equals the separately pinned saved-position output.
    pub saved_position_checked: bool,
    pub issues: Vec<String>,
}

/// The generated table supplies physical field names, including Option fields:
/// absent and explicit null have different observation meanings in /2.
pub(super) fn fields(name: &str, value: &Value) -> Result<()> {
    let object = value.as_object().context("motion2_expected_object")?;
    let names = CONTRACT_FIELDS
        .iter()
        .filter(|(owner, _, _)| *owner == name)
        .map(|(_, field, _)| *field)
        .collect::<BTreeSet<_>>();
    ensure!(
        !names.is_empty()
            && object.len() == names.len()
            && object.keys().all(|k| names.contains(k.as_str())),
        "motion2_missing_or_extra_physical_field:{name}"
    );
    Ok(())
}
pub(super) fn decode<T: DeserializeOwned>(name: &str, value: &Value) -> Result<T> {
    fields(name, value)?;
    Ok(serde_json::from_value(value.clone())?)
}

/// A source callback must open exactly the requested pinned regular file and
/// verify its identity after EOF. All semantic state is provisional until that
/// callback and all stream checks return. No side effects are performed here.
pub fn receive(
    input: Inputs2<'_>,
    summary: Option<&NxMotionEvidenceSummary>,
    observed_names: &[String],
    source: impl FnMut(&str, &mut dyn FnMut(&mut dyn Read) -> Result<()>) -> Result<()>,
) -> Result<Reception2> {
    input
        .params
        .validate()
        .map_err(|_| anyhow::anyhow!("motion2_params_generated"))?;
    ensure!(
        input.params.schema == "oev2.nx-assembly-motion-params/2"
            && input.params.action == "run_assembly_motion",
        "motion2_parameters_version"
    );
    crate::geometry::motion_task::reference_valid(input.parameters)?;
    crate::geometry::motion_task::reference_valid(&input.params.intent)?;
    crate::assembly_motion::validate_intent(input.intent)?;
    crate::motion_source::validate_source_closure(input.closure)?;
    crate::assembly::budget_valid(&input.params.budget)?;
    super::validate_stream_inputs(&input.params.evidence, input.binding)?;
    super::validate_stream_inputs(input.receiver_budget, input.binding)?;
    ensure!(
        input.binding.intent == input.params.intent
            && input.binding.object == input.intent.object
            && input.binding.samples == input.intent.samples.len() as u64
            && input.intent.root_part == input.closure.root_part,
        "motion2_input_binding"
    );
    for (actual, limit) in [
        (
            input.receiver_budget.max_bytes,
            input.params.evidence.max_bytes,
        ),
        (
            input.receiver_budget.max_events,
            input.params.evidence.max_events,
        ),
        (
            input.receiver_budget.max_event_bytes,
            input.params.evidence.max_event_bytes,
        ),
        (
            input.receiver_budget.max_segments,
            input.params.evidence.max_segments,
        ),
        (
            input.receiver_budget.segment_bytes,
            input.params.evidence.segment_bytes,
        ),
    ] {
        ensure!(actual <= limit, "motion2_receiver_budget_widened");
    }
    let declared = input
        .closure
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut mapped = BTreeSet::new();
    ensure!(
        input.owned_paths.len() <= 4096,
        "motion2_owned_mapping_bound"
    );
    for (native, relative) in input.owned_paths {
        ensure!(
            !native.is_empty()
                && !native.chars().any(char::is_control)
                && declared.contains(relative.as_str())
                && mapped.insert(relative.as_str()),
            "motion2_owned_mapping_invalid"
        );
    }
    let completed = input.completed;
    let budget = input.receiver_budget;
    let binding = input.binding;
    let mut state = state::State::new(input);
    let truncated;
    if let Some(summary) = summary {
        super::receiving::verify_evidence_with(
            summary,
            budget,
            binding,
            observed_names,
            source,
            &mut |event| state.observe(event),
        )?;
        truncated = false;
    } else {
        ensure!(!completed, "motion2_completed_missing_summary");
        truncated = prefix::visit(budget, binding, observed_names, source, &mut |event| {
            state.observe(event)
        })?;
    }
    state.finish(summary.is_some(), truncated)
}
