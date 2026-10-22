//! Explicit position observation version. Never relabel /2 output as /1 evidence.
use crate::assembly::{self, Rigid};
use anyhow::{Context, Result, ensure};
use oe_contracts::{NxAssemblyBudget, NxAssemblyObservation, NxAssemblyPositionObservation};
use std::collections::{BTreeMap, BTreeSet};
pub const SCHEMA: &str = "oev2.nx-native-assembly-position-observation/2";
pub fn validate_position_observation(
    value: &NxAssemblyPositionObservation,
    budget: &NxAssemblyBudget,
) -> Result<()> {
    value
        .validate()
        .map_err(|_| anyhow::anyhow!("position_generated_observation_invalid"))?;
    ensure!(
        value.schema == SCHEMA,
        "position_observation_version_differs"
    );
    assembly::budget_valid(budget)?;
    ensure!(
        value.occurrences.len() as u64 <= budget.max_occurrences
            && value.position_overrides.len() == value.occurrences.len(),
        "position_override_inventory_incomplete"
    );
    ensure!(
        value.root_occurrence_tag <= u32::MAX as u64
            && (value.occurrences.is_empty() || value.root_occurrence_tag != 0),
        "position_root_occurrence_missing"
    );
    let mut nodes = BTreeMap::new();
    for o in &value.occurrences {
        ensure!(
            nodes.insert(o.occurrence_id.as_str(), o).is_none()
                && o.part_occurrence_tag != value.root_occurrence_tag,
            "position_occurrence_identity_aliased"
        );
    }
    let mut seen = BTreeSet::new();
    let mut overridden = BTreeSet::new();
    for p in &value.position_overrides {
        let node = *nodes
            .get(p.occurrence_id.as_str())
            .context("position_override_occurrence_missing")?;
        ensure!(
            seen.insert(p.occurrence_id.as_str())
                && p.part_occurrence_tag == node.part_occurrence_tag,
            "position_override_identity_differs"
        );
        ensure!(
            p.parent_occurrence_tag <= u32::MAX as u64,
            "position_override_parent_tag_invalid"
        );
        match p.raw_type {
            0 => ensure!(
                p.parent_occurrence_tag == 0,
                "position_no_override_has_parent"
            ),
            2..=4 => {
                ensure!(
                    p.parent_occurrence_tag != 0,
                    "position_override_parent_missing"
                );
                let mut ancestors = BTreeSet::new();
                let mut next = node.parent_occurrence_id.as_deref();
                let mut found = p.parent_occurrence_tag == value.root_occurrence_tag;
                while let Some(id) = next {
                    ensure!(
                        ancestors.insert(id) && ancestors.len() as u64 <= budget.max_depth,
                        "position_override_ancestor_cycle"
                    );
                    let parent = *nodes
                        .get(id)
                        .context("position_override_ancestor_missing")?;
                    found |= parent.part_occurrence_tag == p.parent_occurrence_tag;
                    next = parent.parent_occurrence_id.as_deref();
                }
                ensure!(found, "position_override_parent_not_ancestor");
                overridden.insert(p.occurrence_id.clone());
            }
            _ => anyhow::bail!("position_override_unloaded_or_unknown"),
        }
    }
    // Reuse the existing complete geometry, hierarchy, unit and root-transform
    // checks. This in-memory adapter is never serialized or emitted as /1.
    let view = inventory_view(value);
    assembly::validate_observation_with_overrides(&view, budget, &overridden)?;
    // Override exemptions remove equality to definition, never validity of the
    // original definition's coordinates or rotation.
    for o in &value.occurrences {
        let parent_proto = match &o.parent_occurrence_id {
            Some(id) => nodes[id.as_str()].prototype_id.as_str(),
            None => value.root_prototype_id.as_str(),
        };
        let units = &value
            .prototypes
            .iter()
            .find(|p| p.prototype_id == parent_proto)
            .unwrap()
            .native_units;
        let definition =
            Rigid::from_parent_component(&o.raw_parent_origin, &o.raw_parent_csys, units)?;
        let matrix = Rigid::from_root_native(&o.raw_parent_transform, units)?;
        if o.reference_set == "None" {
            ensure!(
                definition.near(matrix),
                "position_definition_representations_differ"
            );
        }
    }
    Ok(())
}

fn inventory_view(value: &NxAssemblyPositionObservation) -> NxAssemblyObservation {
    NxAssemblyObservation {
        schema: assembly::OBSERVATION_SCHEMA.into(),
        root_prototype_id: value.root_prototype_id.clone(),
        prototypes: value.prototypes.clone(),
        occurrences: value.occurrences.clone(),
        complete: value.complete,
        source_role: value.source_role.clone(),
        engineering_adoption: value.engineering_adoption.clone(),
    }
}
pub(crate) fn checked_inventory(
    value: &NxAssemblyPositionObservation,
    budget: &NxAssemblyBudget,
) -> Result<NxAssemblyObservation> {
    validate_position_observation(value, budget)?;
    Ok(inventory_view(value))
}

pub mod persistence;
