//! Planning for explicit occurrence motion. No function in this module executes
//! NX, accepts external native tags, or establishes collision/physical clearance.
pub mod binding;
pub mod execution;
pub mod scene;
use crate::assembly::{ROTATION_TOLERANCE, Rigid, unit_to_m, validate_observation};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    NxAssemblyBudget, NxAssemblyMotionBudget, NxAssemblyMotionIntent, NxAssemblyMotionSample,
    NxAssemblyObservation,
};
use oe_nx_protocol::{Digest, RelativePath};
use std::collections::BTreeSet;

fn proper(values: &[f64]) -> Result<Rigid> {
    let r = Rigid::checked(values)?;
    let a = r.0;
    let d = a[0] * (a[5] * a[10] - a[6] * a[9]) - a[1] * (a[4] * a[10] - a[6] * a[8])
        + a[2] * (a[4] * a[9] - a[5] * a[8]);
    ensure!(
        (d - 1.).abs() <= ROTATION_TOLERANCE,
        "motion_pose_reflection_not_actuation"
    );
    Ok(r)
}
fn budget(b: &NxAssemblyMotionBudget) -> Result<()> {
    ensure!(
        (1..=4096).contains(&b.max_samples)
            && (1..=100_000).contains(&b.max_placed_bodies)
            && (1..=1_000_000_000_000).contains(&b.max_pair_checks),
        "motion_budget_invalid"
    );
    Ok(())
}
pub(crate) fn sample_valid(s: &NxAssemblyMotionSample) -> Result<()> {
    ensure!(
        !s.sample_id.is_empty()
            && s.sample_id.len() <= 256
            && s.time_s.is_finite()
            && s.time_s >= 0.
            && !s.targets.is_empty()
            && s.targets.len() <= 100_000,
        "motion_sample_invalid"
    );
    targets_valid(&s.targets)
}
pub(crate) fn targets_valid(targets: &[oe_contracts::NxAssemblyPoseTarget]) -> Result<()> {
    ensure!(
        !targets.is_empty() && targets.len() <= 100_000,
        "motion_sample_invalid"
    );
    let mut paths = BTreeSet::new();
    for t in targets {
        ensure!(
            !t.occurrence_path.is_empty()
                && t.occurrence_path.len() <= 64
                && t.occurrence_path
                    .iter()
                    .all(|n| !n.is_empty() && n.len() <= 256 && !n.chars().any(char::is_control))
                && paths.insert(t.occurrence_path.clone()),
            "motion_target_path_empty_or_duplicate"
        );
        RelativePath::new(t.prototype_id.clone())?;
        proper(&t.local_to_root_m)?;
    }
    Ok(())
}
pub fn validate_intent(i: &NxAssemblyMotionIntent) -> Result<()> {
    i.validate()
        .map_err(|_| anyhow::anyhow!("motion_generated_intent_invalid"))?;
    i.object
        .validate()
        .map_err(|_| anyhow::anyhow!("motion_object_invalid"))?;
    ensure!(
        i.schema == "oev2.nx-assembly-motion-intent/1",
        "motion_schema_invalid"
    );
    RelativePath::new(i.assembly_source.path.clone())?;
    Digest::new(i.assembly_source.sha256.clone())?;
    RelativePath::new(i.root_part.clone())?;
    budget(&i.budget)?;
    ensure!(
        !i.samples.is_empty()
            && i.samples.len() as u64 <= i.budget.max_samples
            && !i.source_refs.is_empty()
            && i.source_refs.len() <= 4096,
        "motion_input_budget"
    );
    let mut sources = BTreeSet::new();
    ensure!(
        i.source_refs
            .iter()
            .all(|s| !s.is_empty() && sources.insert(s)),
        "motion_source_empty_or_duplicate"
    );
    let mut ids = BTreeSet::new();
    let mut previous = None;
    for s in &i.samples {
        sample_valid(s)?;
        ensure!(ids.insert(&s.sample_id), "motion_duplicate_sample_id");
        ensure!(
            previous.is_none_or(|p| s.time_s > p),
            "motion_time_not_increasing"
        );
        previous = Some(s.time_s);
    }
    Ok(())
}
/// SDK takes a delta in displayed-root native coordinates. Re-read current
/// native pose before each call, especially after moving an ancestor.
pub fn delta_for_target(
    current_m: &[f64],
    target_m: &[f64],
    root_unit: &str,
) -> Result<[[f64; 4]; 4]> {
    let current = proper(current_m)?;
    let target = proper(target_m)?;
    let mut delta = target.compose(current.inverse()?)?.0;
    let scale = unit_to_m(root_unit)?;
    for k in [3, 7, 11] {
        delta[k] /= scale;
    }
    ensure!(
        delta.iter().all(|v| v.is_finite()),
        "motion_native_delta_nonfinite"
    );
    Ok(std::array::from_fn(|r| {
        std::array::from_fn(|c| delta[r * 4 + c])
    }))
}
/// Internal planning result only, not a native observation or wire DTO.
#[derive(Debug, PartialEq)]
pub struct ResolvedTarget {
    pub occurrence_id: String,
    pub desired_root_m: Rigid,
}
pub fn resolve_targets(
    observation: &NxAssemblyObservation,
    inventory_budget: &NxAssemblyBudget,
    sample: &NxAssemblyMotionSample,
) -> Result<Vec<ResolvedTarget>> {
    validate_observation(observation, inventory_budget)?;
    resolve_targets_checked(observation, sample)
}
fn resolve_targets_checked(
    observation: &NxAssemblyObservation,
    sample: &NxAssemblyMotionSample,
) -> Result<Vec<ResolvedTarget>> {
    sample_valid(sample)?;
    let mut selected = Vec::new();
    let mut ids = BTreeSet::new();
    for target in &sample.targets {
        let mut parent: Option<&str> = None;
        let mut found = None;
        for name in &target.occurrence_path {
            let matches = observation
                .occurrences
                .iter()
                .filter(|o| o.parent_occurrence_id.as_deref() == parent && &o.name == name)
                .collect::<Vec<_>>();
            ensure!(
                matches.len() == 1,
                "motion_occurrence_path_missing_or_ambiguous"
            );
            found = Some(matches[0]);
            parent = Some(matches[0].occurrence_id.as_str());
        }
        let o = found.context("motion_occurrence_unresolved")?;
        ensure!(
            o.prototype_id == target.prototype_id,
            "motion_target_prototype_differs"
        );
        ensure!(ids.insert(&o.occurrence_id), "motion_target_alias");
        selected.push((
            target.occurrence_path.len(),
            ResolvedTarget {
                occurrence_id: o.occurrence_id.clone(),
                desired_root_m: proper(&target.local_to_root_m)?,
            },
        ));
    }
    // Each child target is absolute. The native executor must re-read its pose
    // after the parent update; these entries intentionally contain no stale delta.
    selected.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.occurrence_id.cmp(&b.1.occurrence_id))
    });
    Ok(selected.into_iter().map(|(_, v)| v).collect())
}
/// Current inventory placement identity. prototype_body_tag is NOT the body
/// occurrence tag required by the native interference API.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlacedBody {
    pub occurrence_id: Option<String>,
    pub prototype_id: String,
    pub prototype_body_tag: u64,
}
#[derive(Debug)]
pub struct PairPlan {
    pub bodies: Vec<PlacedBody>,
    pub pairs_per_sample: u64,
    pub total_pair_checks: u64,
}
impl PairPlan {
    pub fn pairs(&self) -> impl Iterator<Item = (&PlacedBody, &PlacedBody)> {
        self.bodies
            .iter()
            .enumerate()
            .flat_map(move |(i, a)| self.bodies[i + 1..].iter().map(move |b| (a, b)))
    }
}
pub fn plan_pairs(
    o: &NxAssemblyObservation,
    inventory_budget: &NxAssemblyBudget,
    b: &NxAssemblyMotionBudget,
    samples: u64,
) -> Result<PairPlan> {
    validate_observation(o, inventory_budget)?;
    plan_pairs_checked(o, b, samples)
}
fn plan_pairs_checked(
    o: &NxAssemblyObservation,
    b: &NxAssemblyMotionBudget,
    samples: u64,
) -> Result<PairPlan> {
    budget(b)?;
    ensure!(
        samples > 0 && samples <= b.max_samples,
        "motion_sample_budget"
    );
    let mut prototype_tags = BTreeSet::new();
    for p in &o.prototypes {
        for body in &p.solids {
            ensure!(
                prototype_tags.insert(body.body_tag),
                "motion_prototype_body_tag_aliased"
            );
        }
    }
    let mut bodies = Vec::new();
    let mut add = |id: Option<&str>, prototype: &str| -> Result<()> {
        let p = o
            .prototypes
            .iter()
            .find(|p| p.prototype_id == prototype)
            .context("motion_prototype_missing")?;
        ensure!(p.sheet_count == 0, "motion_sheet_geometry_not_covered");
        for body in &p.solids {
            ensure!(
                (bodies.len() as u64) < b.max_placed_bodies,
                "motion_body_budget_exceeded"
            );
            bodies.push(PlacedBody {
                occurrence_id: id.map(str::to_owned),
                prototype_id: prototype.into(),
                prototype_body_tag: body.body_tag,
            });
        }
        Ok(())
    };
    add(None, &o.root_prototype_id)?;
    for occurrence in &o.occurrences {
        add(Some(&occurrence.occurrence_id), &occurrence.prototype_id)?;
    }
    bodies.sort();
    ensure!(!bodies.is_empty(), "motion_no_solid_inventory");
    let n = bodies.len() as u64;
    let pairs = n.checked_mul(n - 1).context("motion_pair_count_overflow")? / 2;
    let total = pairs
        .checked_mul(samples)
        .context("motion_pair_count_overflow")?;
    ensure!(total <= b.max_pair_checks, "motion_pair_budget_exceeded");
    Ok(PairPlan {
        bodies,
        pairs_per_sample: pairs,
        total_pair_checks: total,
    })
}
/// Raw SDK classification only. Touching is never promoted to clearance here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterferenceStatus {
    Interference,
    NoInterference,
    Touching,
}
pub fn interference_status(return_code: i32, result: i32) -> Result<InterferenceStatus> {
    ensure!(
        return_code == 0,
        "motion_interference_query_failed:{return_code}"
    );
    match result {
        1 => Ok(InterferenceStatus::Interference),
        2 => Ok(InterferenceStatus::NoInterference),
        3 => Ok(InterferenceStatus::Touching),
        _ => anyhow::bail!("motion_interference_result_unknown:{result}"),
    }
}

/// Complete expected occurrence poses, including inherited motion and untouched
/// branches. Compare against a new native observation after all updates, never
/// against a record synthesized from these expected matrices.
pub fn expected_poses(
    o: &NxAssemblyObservation,
    inventory_budget: &NxAssemblyBudget,
    sample: &NxAssemblyMotionSample,
) -> Result<std::collections::BTreeMap<String, Rigid>> {
    validate_observation(o, inventory_budget)?;
    expected_poses_checked(o, sample)
}
fn expected_poses_checked(
    o: &NxAssemblyObservation,
    sample: &NxAssemblyMotionSample,
) -> Result<std::collections::BTreeMap<String, Rigid>> {
    let targets = resolve_targets_checked(o, sample)?
        .into_iter()
        .map(|t| (t.occurrence_id, t.desired_root_m))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut result = std::collections::BTreeMap::<String, Rigid>::new();
    let mut pending = o.occurrences.iter().collect::<Vec<_>>();
    while !pending.is_empty() {
        let before = pending.len();
        let mut remaining = Vec::new();
        for occurrence in pending {
            let parent = match &occurrence.parent_occurrence_id {
                None => Rigid::identity(),
                Some(id) => match result.get(id) {
                    Some(p) => *p,
                    None => {
                        remaining.push(occurrence);
                        continue;
                    }
                },
            };
            let pose = match targets.get(&occurrence.occurrence_id) {
                Some(t) => *t,
                None => parent.compose(Rigid::checked(&occurrence.local_to_parent_m)?)?,
            };
            result.insert(occurrence.occurrence_id.clone(), pose);
        }
        ensure!(remaining.len() < before, "motion_parent_graph_unresolved");
        pending = remaining;
    }
    Ok(result)
}

// Explicit /2 input, fully checked before internal shared planning. Never emit
// the internal legacy-shaped view as /1 evidence.
pub fn resolve_position_targets(
    o: &oe_contracts::NxAssemblyPositionObservation,
    b: &NxAssemblyBudget,
    s: &NxAssemblyMotionSample,
) -> Result<Vec<ResolvedTarget>> {
    resolve_targets_checked(&crate::assembly_position::checked_inventory(o, b)?, s)
}
pub fn plan_position_pairs(
    o: &oe_contracts::NxAssemblyPositionObservation,
    b: &NxAssemblyBudget,
    m: &NxAssemblyMotionBudget,
    samples: u64,
) -> Result<PairPlan> {
    plan_pairs_checked(
        &crate::assembly_position::checked_inventory(o, b)?,
        m,
        samples,
    )
}
pub fn expected_position_poses(
    o: &oe_contracts::NxAssemblyPositionObservation,
    b: &NxAssemblyBudget,
    s: &NxAssemblyMotionSample,
) -> Result<std::collections::BTreeMap<String, Rigid>> {
    expected_poses_checked(&crate::assembly_position::checked_inventory(o, b)?, s)
}
pub fn check_position_sample(
    before: &oe_contracts::NxAssemblyPositionObservation,
    after: &oe_contracts::NxAssemblyPositionObservation,
    b: &NxAssemblyBudget,
    s: &NxAssemblyMotionSample,
) -> Result<()> {
    let mut expected = crate::assembly_position::checked_inventory(before, b)?;
    let observed = crate::assembly_position::checked_inventory(after, b)?;
    let poses = expected_poses_checked(&expected, s)?;
    ensure!(
        before.root_occurrence_tag == after.root_occurrence_tag,
        "motion_sample_root_identity_changed"
    );
    for o in &mut expected.occurrences {
        let actual = observed
            .occurrences
            .iter()
            .find(|a| a.occurrence_id == o.occurrence_id)
            .context("motion_sample_occurrence_missing")?;
        ensure!(
            actual.part_occurrence_tag == o.part_occurrence_tag
                && actual.instance_tag == o.instance_tag,
            "motion_sample_native_occurrence_identity_changed"
        );
        let root = poses[&o.occurrence_id];
        let parent = match &o.parent_occurrence_id {
            Some(id) => poses[id],
            None => Rigid::identity(),
        };
        o.local_to_root_m = root.0.to_vec();
        o.local_to_parent_m = parent.inverse()?.compose(root)?.0.to_vec();
    }
    // Existing bounded geometry check compares units, counts, measured bounds
    // and volume; it is not a topological identity or physical clearance proof.
    crate::assembly::same_geometry(&expected, &observed)
}
