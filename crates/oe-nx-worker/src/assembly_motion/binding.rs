//! Algorithm-local native query facts, not an input protocol. The native caller
//! constructs these only after successful current-session type/owner queries.
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedBody {
    pub prototype_body: u32,
    pub prototype_owner: u32,
    pub part_occurrence: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueriedBody {
    pub body: u32,
    pub is_occurrence: bool,
    pub prototype_body: u32,
    pub containing_occurrence: u32,
    /// Actual body owner query; occurrence owner semantics are retained, not
    /// guessed to equal the prototype owner or displayed assembly.
    pub owner: u32,
    pub is_solid_body: bool,
}
pub fn validate_binding(expected: &ExpectedBody, actual: &QueriedBody) -> Result<()> {
    ensure!(
        expected.prototype_body != 0
            && expected.prototype_owner != 0
            && actual.body != 0
            && actual.owner != 0
            && actual.is_solid_body,
        "motion_native_body_incomplete"
    );
    match expected.part_occurrence {
        Some(part) => ensure!(
            part != 0
                && actual.is_occurrence
                && actual.body != expected.prototype_body
                && actual.prototype_body == expected.prototype_body
                && actual.containing_occurrence == part,
            "motion_native_occurrence_body_identity_differs"
        ),
        None => ensure!(
            !actual.is_occurrence
                && actual.body == expected.prototype_body
                && actual.prototype_body == expected.prototype_body
                && actual.containing_occurrence == 0
                && actual.owner == expected.prototype_owner,
            "motion_native_root_body_identity_differs"
        ),
    }
    Ok(())
}
pub fn validate_unique_bindings(pairs: &[(ExpectedBody, QueriedBody)]) -> Result<()> {
    let mut bodies = BTreeSet::new();
    let mut placements = BTreeSet::new();
    for (expected, actual) in pairs {
        validate_binding(expected, actual)?;
        ensure!(
            bodies.insert(actual.body),
            "motion_native_body_handle_aliased"
        );
        ensure!(
            placements.insert((expected.part_occurrence, expected.prototype_body)),
            "motion_native_placement_duplicated"
        );
    }
    Ok(())
}
