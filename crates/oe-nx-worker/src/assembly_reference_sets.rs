//! Explicit UF instance/part-occurrence observations. Raw names stay separate.
//! NX 2412 uf_assem.h documents Entire Part -> "None" for this API, and warns
//! that an instance and its part occurrence may use different reference sets.
use anyhow::{Result, ensure};
use oe_contracts::{NxAssemblyObservation, NxAssemblyReferenceSets2};
use std::collections::BTreeMap;

pub const PARAMS_SCHEMA: &str = "oev2.nx-assembly-reference-sets-params/2";
pub const SCHEMA: &str = "oev2.nx-assembly-reference-sets/2";

pub fn validate(value: &NxAssemblyReferenceSets2, assembly: &NxAssemblyObservation) -> Result<()> {
    value
        .validate()
        .map_err(|_| anyhow::anyhow!("assembly_reference_sets_contract_invalid"))?;
    ensure!(
        value.schema == SCHEMA
            && value.api == "UF_ASSEM_ask_component_data"
            && value.complete
            && value.root_prototype_id == assembly.root_prototype_id,
        "assembly_reference_sets_identity_or_completeness_differs"
    );
    ensure!(
        value.members.len() == assembly.occurrences.len(),
        "assembly_reference_sets_coverage_differs"
    );
    let mut members = BTreeMap::new();
    for member in &value.members {
        ensure!(
            members
                .insert(member.occurrence_id.as_str(), member)
                .is_none(),
            "assembly_reference_sets_duplicate_occurrence"
        );
    }
    for occurrence in &assembly.occurrences {
        let member = members
            .get(occurrence.occurrence_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("assembly_reference_sets_occurrence_missing"))?;
        ensure!(
            member.part_occurrence_tag == occurrence.part_occurrence_tag
                && member.instance_tag == occurrence.instance_tag
                && member.instance_reference_set == occurrence.reference_set
                && !member.instance_reference_set.is_empty()
                && !member.part_occurrence_reference_set.is_empty(),
            "assembly_reference_sets_tags_or_definition_differs"
        );
    }
    Ok(())
}

/// An observation of a definition never establishes the occurrence's selection.
pub fn require_entire(
    value: &NxAssemblyReferenceSets2,
    assembly: &NxAssemblyObservation,
) -> Result<()> {
    validate(value, assembly)?;
    ensure!(
        value.members.iter().all(
            |m| m.instance_reference_set == "None" && m.part_occurrence_reference_set == "None"
        ),
        "assembly_reference_sets_entire_part_required"
    );
    Ok(())
}
