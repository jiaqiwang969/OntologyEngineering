//! Native syntax variable sets use the existing set/hash owner. Temporary
//! parser tables carry construction history, never engineering or RDF facts.
use super::source_set::{SourceSet, SourceSetError};
use crate::numeric::{RawNumericFault, SourceHashRuntime};
use oxrdf::Variable;
use semantica_numeric::NumericFault;
use spargebra::SourceVariableSetStep;

/// translate's VS.add occurrences, followed by actual in-place VS |= set(PV)
/// for the first child projection in each mapping branch. This owns the same
/// native set/hash primitives; it is not _addVars' copy/union program or a
/// lexical order of an unordered membership view.
pub(crate) fn source_visible_projection(
    occurrences: &[Variable],
    children: &[Vec<Variable>],
    hash: &SourceHashRuntime,
) -> Result<Vec<Variable>, RawNumericFault> {
    let mut visible = SourceSet::new();
    let mut same = |a: &Variable, b: &Variable| a == b;
    let mut equal = |a: &Variable, b: &Variable| Ok::<_, RawNumericFault>(a == b);
    let mut alive = || Ok::<_, RawNumericFault>(());
    for variable in occurrences {
        visible
            .insert_hashed(
                variable.clone(),
                hash.variable_hash(variable.as_str()),
                &mut same,
                &mut equal,
                &mut alive,
            )
            .map_err(fault)?;
    }
    for projected in children {
        let mut child = SourceSet::new();
        for variable in projected {
            child
                .insert_hashed(
                    variable.clone(),
                    hash.variable_hash(variable.as_str()),
                    &mut same,
                    &mut equal,
                    &mut alive,
                )
                .map_err(fault)?;
        }
        visible
            .merge_hashed(&child, &mut same, &mut equal, &mut alive)
            .map_err(fault)?;
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(visible.len())
        .map_err(|_| resource())?;
    output.extend(visible.iter().cloned());
    Ok(output)
}

pub(crate) fn compile_source_variable_sets(
    steps: &[SourceVariableSetStep],
    hash: &SourceHashRuntime,
) -> Result<Vec<Vec<Variable>>, RawNumericFault> {
    let mut sets: Vec<SourceSet<Variable>> = Vec::new();
    sets.try_reserve(steps.len()).map_err(|_| resource())?;
    let mut same = |left: &Variable, right: &Variable| left == right;
    let mut equal = |left: &Variable, right: &Variable| Ok::<_, RawNumericFault>(left == right);
    let mut alive = || Ok::<_, RawNumericFault>(());
    for step in steps {
        let mut result = SourceSet::new();
        match step {
            SourceVariableSetStep::New(variables) => {
                for variable in variables {
                    result
                        .insert_hashed(
                            variable.clone(),
                            hash.variable_hash(variable.as_str()),
                            &mut same,
                            &mut equal,
                            &mut alive,
                        )
                        .map_err(fault)?;
                }
            }
            SourceVariableSetStep::Copy(child) => {
                let child = sets
                    .get(*child)
                    .ok_or(RawNumericFault::InvalidSourceOrigin)?;
                result = child
                    .copy_hashed(&mut same, &mut equal, &mut alive)
                    .map_err(fault)?;
            }
            SourceVariableSetStep::Union(children) => {
                for child in children {
                    let child = sets
                        .get(*child)
                        .ok_or(RawNumericFault::InvalidSourceOrigin)?;
                    let mut copy = result
                        .copy_hashed(&mut same, &mut equal, &mut alive)
                        .map_err(fault)?;
                    copy.merge_hashed(child, &mut same, &mut equal, &mut alive)
                        .map_err(fault)?;
                    result = copy;
                }
            }
        }
        sets.push(result);
    }
    let mut iterations = Vec::new();
    iterations.try_reserve(sets.len()).map_err(|_| resource())?;
    for set in sets {
        let mut members = Vec::new();
        members.try_reserve(set.len()).map_err(|_| resource())?;
        members.extend(set.iter().cloned());
        iterations.push(members);
    }
    Ok(iterations)
}
fn resource() -> RawNumericFault {
    NumericFault::ResourceLimit {
        operation: "source_syntax_variable_set",
    }
    .into()
}
fn fault(error: SourceSetError<RawNumericFault>) -> RawNumericFault {
    match error {
        SourceSetError::Callback(error) => error,
        SourceSetError::Allocation | SourceSetError::IndexOverflow => resource(),
    }
}
