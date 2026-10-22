//! Persistent component paths, never native process tags or enumeration ordinals.
use super::*;
use oe_contracts::NxAssemblySolidObservation;
use std::cmp::Ordering;
type ComponentPath = Vec<String>;
pub(crate) fn paths(
    value: &NxAssemblyPositionObservation,
) -> Result<BTreeMap<String, ComponentPath>> {
    let nodes: BTreeMap<_, _> = value
        .occurrences
        .iter()
        .map(|x| (x.occurrence_id.as_str(), x))
        .collect();
    let mut out = BTreeMap::new();
    let mut unique = BTreeSet::new();
    for node in &value.occurrences {
        let mut path = vec![node.name.clone()];
        let mut parent = node.parent_occurrence_id.as_deref();
        while let Some(id) = parent {
            let p = nodes
                .get(id)
                .context("position_persistence_parent_missing")?;
            path.push(p.name.clone());
            parent = p.parent_occurrence_id.as_deref();
        }
        path.reverse();
        ensure!(
            unique.insert(path.clone()),
            "position_persistence_component_path_ambiguous"
        );
        out.insert(node.occurrence_id.clone(), path);
    }
    Ok(out)
}
fn compare_values(a: &[f64], b: &[f64]) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(a, b)| a.total_cmp(b))
        .find(|x| *x != Ordering::Equal)
        .unwrap_or_else(|| a.len().cmp(&b.len()))
}
fn normalize_solids(solids: &mut [NxAssemblySolidObservation]) {
    for body in solids.iter_mut() {
        let mut corners = body
            .exact_box_corners_m
            .chunks_exact(3)
            .map(|x| [x[0], x[1], x[2]])
            .collect::<Vec<_>>();
        corners.sort_by(|a, b| compare_values(a, b));
        body.exact_box_corners_m = corners.into_iter().flatten().collect();
    }
    solids.sort_by(|a, b| {
        compare_values(&a.exact_box_corners_m, &b.exact_box_corners_m).then_with(|| {
            a.volume_m3
                .unwrap_or(-1.)
                .total_cmp(&b.volume_m3.unwrap_or(-1.))
        })
    });
}
struct Persistent {
    inventory: NxAssemblyObservation,
    overrides: BTreeMap<ComponentPath, (i64, Option<ComponentPath>)>,
    definitions: BTreeMap<ComponentPath, (Rigid, Rigid)>,
}
fn normalize(
    value: &NxAssemblyPositionObservation,
    budget: &NxAssemblyBudget,
) -> Result<Persistent> {
    let mut inventory = checked_inventory(value, budget)?;
    let names = paths(value)?;
    let by_tag: BTreeMap<_, _> = value
        .occurrences
        .iter()
        .map(|x| (x.part_occurrence_tag, &names[&x.occurrence_id]))
        .collect();
    let nodes: BTreeMap<_, _> = value
        .occurrences
        .iter()
        .map(|x| (x.occurrence_id.as_str(), x))
        .collect();
    let mut overrides = BTreeMap::new();
    let mut definitions = BTreeMap::new();
    for p in &value.position_overrides {
        let parent = if p.raw_type == 0 {
            None
        } else if p.parent_occurrence_tag == value.root_occurrence_tag {
            Some(vec![])
        } else {
            Some(
                (*by_tag
                    .get(&p.parent_occurrence_tag)
                    .context("position_persistence_override_parent_missing")?)
                .clone(),
            )
        };
        overrides.insert(names[&p.occurrence_id].clone(), (p.raw_type, parent));
    }
    for o in &mut inventory.occurrences {
        let prototype = match &o.parent_occurrence_id {
            Some(id) => nodes[id.as_str()].prototype_id.as_str(),
            None => value.root_prototype_id.as_str(),
        };
        let units = &value
            .prototypes
            .iter()
            .find(|p| p.prototype_id == prototype)
            .unwrap()
            .native_units;
        definitions.insert(
            names[&o.occurrence_id].clone(),
            (
                Rigid::from_parent_component(&o.raw_parent_origin, &o.raw_parent_csys, units)?,
                Rigid::from_root_native(&o.raw_parent_transform, units)?,
            ),
        );
        o.parent_occurrence_id = o
            .parent_occurrence_id
            .as_ref()
            .map(|id| serde_json::to_string(&names[id]))
            .transpose()?;
        o.occurrence_id = serde_json::to_string(&names[&o.occurrence_id])?;
    }
    for p in &mut inventory.prototypes {
        normalize_solids(&mut p.solids);
    }
    Ok(Persistent {
        inventory,
        overrides,
        definitions,
    })
}
/// Bounded geometry multiset, current/definition poses and override ancestry.
/// This proves neither whole B-rep identity nor independent process execution.
pub fn compare(
    expected: &NxAssemblyPositionObservation,
    actual: &NxAssemblyPositionObservation,
    budget: &NxAssemblyBudget,
) -> Result<()> {
    let a = normalize(expected, budget)?;
    let b = normalize(actual, budget)?;
    assembly::same_geometry(&a.inventory, &b.inventory)?;
    ensure!(
        a.overrides == b.overrides,
        "position_persistence_override_identity_differs"
    );
    ensure!(
        a.definitions.len() == b.definitions.len(),
        "position_persistence_definition_count_differs"
    );
    for (path, (pose, matrix)) in &a.definitions {
        let (other_pose, other_matrix) = b
            .definitions
            .get(path)
            .context("position_persistence_definition_missing")?;
        ensure!(
            pose.near(*other_pose) && matrix.near(*other_matrix),
            "position_persistence_definition_changed"
        );
    }
    Ok(())
}
