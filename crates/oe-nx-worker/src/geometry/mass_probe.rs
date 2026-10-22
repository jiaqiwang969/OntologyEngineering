//! Explicit diagnostic acquisition. Original geometry/copy acceptance is unchanged.
use super::{NativeApi, Workspace, input_relative};
use crate::assembly;
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, NxAssemblyBudget, NxAssemblyObservation, NxGeometricalMassObservation1,
};
use serde_json::{Value, json};

pub const OBSERVATION_SCHEMA: &str = "oev2.nx-geometrical-mass-observation/1";
pub const PARAMS_SCHEMA: &str = "oev2.nx-assembly-mass-probe-params/1";
pub const COPY_PARAMS_SCHEMA: &str = "oev2.nx-copy-mass-probe-params/1";
pub const COPY_ACTION: &str = "probe_component_copy_mass_properties";
pub const MAX_SAMPLES_PER_PHASE: usize = 16;

pub fn repetitions_valid(repetitions: u32) -> Result<()> {
    ensure!(
        (2..=4).contains(&repetitions),
        "mass_probe_repetitions_invalid"
    );
    Ok(())
}
pub fn words(values: &[f64]) -> Vec<String> {
    values
        .iter()
        .map(|v| format!("{:016x}", v.to_bits()))
        .collect()
}
pub struct Decoded {
    pub raw47: [f64; 47],
    pub statistics13: [f64; 13],
}
fn decode_words<const N: usize>(values: &[String]) -> Result<[f64; N]> {
    ensure!(values.len() == N, "mass_probe_result_count_invalid");
    let mut result = [0.; N];
    for (out, value) in result.iter_mut().zip(values) {
        ensure!(
            value.len() == 16
                && value
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "mass_probe_binary64_invalid"
        );
        *out = f64::from_bits(u64::from_str_radix(value, 16)?);
        ensure!(out.is_finite(), "mass_probe_nonfinite_result");
    }
    Ok(result)
}
/// Decode original bits, never a rounded or cached substitute for the SDK result.
pub fn decode(observation: &NxGeometricalMassObservation1) -> Result<Decoded> {
    observation
        .validate()
        .map_err(|_| anyhow::anyhow!("mass_probe_contract_invalid"))?;
    let mut accuracy = vec![0.0_f64; 11];
    accuracy[0] = 0.999999;
    ensure!(
        observation.schema == OBSERVATION_SCHEMA
            && observation.body_tag != 0
            && observation.owning_part_tag != 0
            && observation.analysis_type == 1
            && observation.units == 4
            && observation.density.to_bits() == 0f64.to_bits()
            && observation.accuracy_mode == 1
            && observation.accuracy_values.len() == accuracy.len()
            && observation
                .accuracy_values
                .iter()
                .zip(&accuracy)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
        "mass_probe_request_differs"
    );
    ensure!(
        observation.return_code == 0,
        "mass_probe_native_return_failed"
    );
    let decoded = Decoded {
        raw47: decode_words(&observation.raw47_binary64)?,
        statistics13: decode_words(&observation.statistics13_binary64)?,
    };
    ensure!(decoded.raw47[1] > 0., "mass_probe_nonpositive_solid_volume");
    Ok(decoded)
}
pub(super) fn admit(observed: &NxAssemblyObservation, repetitions: u32) -> Result<()> {
    repetitions_valid(repetitions)?;
    let count = observed
        .prototypes
        .iter()
        .map(|p| p.solids.len())
        .try_fold(0usize, |a, b| a.checked_add(b))
        .context("mass_probe_sample_budget")?;
    ensure!(
        count > 0
            && count
                .checked_mul(repetitions as usize)
                .is_some_and(|n| n <= MAX_SAMPLES_PER_PHASE),
        "mass_probe_sample_budget"
    );
    Ok(())
}
pub(super) fn capture(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    observed: &NxAssemblyObservation,
    repetitions: u32,
    stage: &str,
) -> Result<Vec<ArtifactReference>> {
    admit(observed, repetitions)?;
    ensure!(
        [
            "before",
            "after",
            "saved",
            "control_initial",
            "control_repeat"
        ]
        .contains(&stage),
        "mass_probe_stage_invalid"
    );
    let mut references = Vec::new();
    for prototype in &observed.prototypes {
        for solid in &prototype.solids {
            for iteration in 0..repetitions {
                let actual =
                    api.geometrical_mass_properties(solid.body_tag, prototype.native_part_tag)?;
                // Failed/nonfinite output words remain complete evidence before rejection.
                let reference = workspace.record_bound("assembly_mass_probe_sample", json!({
                    "stage":stage,"iteration":iteration,"prototype_id":prototype.prototype_id,
                    "expected_body_tag":solid.body_tag,"expected_owning_part_tag":prototype.native_part_tag,"observation":actual,
                    "engineering_adoption":"not_established"}))?;
                ensure!(
                    actual.body_tag == solid.body_tag
                        && actual.owning_part_tag == prototype.native_part_tag,
                    "mass_probe_current_body_or_owner_differs"
                );
                decode(&actual)?;
                references.push(reference);
            }
        }
    }
    Ok(references)
}
pub fn execute_unchanged(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    root_part: &str,
    budget: &NxAssemblyBudget,
    repetitions: u32,
) -> Result<Value> {
    repetitions_valid(repetitions)?;
    let source = workspace.input(&input_relative(root_part)?)?;
    let namespace = source.parent().context("mass_probe_input_parent_missing")?;
    let root = api.open_part(&source)?;
    let before = api.full_assembly_info(root, namespace, budget, "reference_observation")?;
    assembly::validate_observation(&before, budget)?;
    admit(&before, repetitions)?;
    let before_ref = workspace.record_bound(
        "assembly_mass_probe_control_before",
        serde_json::to_value(&before)?,
    )?;
    let first = capture(api, workspace, &before, repetitions, "control_initial")?;
    let after = api.full_assembly_info(root, namespace, budget, "reference_observation")?;
    assembly::validate_observation(&after, budget)?;
    let after_ref = workspace.record_bound(
        "assembly_mass_probe_control_after",
        serde_json::to_value(&after)?,
    )?;
    let second = capture(api, workspace, &after, repetitions, "control_repeat")?;
    Ok(
        json!({"action":"probe_assembly_mass_properties", "before":before_ref,"after":after_ref,
        "initial_samples":first,"repeat_samples":second,"repetitions":repetitions,
        "mutation_requested":false,"save_requested":false,"geometry_equivalence":"not_established",
        "engineering_adoption":"not_established","diagnostic_scope":"raw SDK measurements only; no physical uncertainty or copy qualification"}),
    )
}
