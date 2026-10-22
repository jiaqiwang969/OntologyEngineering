//! Source-intent membership, not a native assembly or collision observation.
//! Keep occurrence identity separate from reusable prototype file identity.
use anyhow::{Context, Result, ensure};
use oe_contracts::{NxAssemblyBudget, NxAssemblySceneTimeline};
use std::collections::{BTreeMap, BTreeSet};

/// Indices into the immutable input members; memory is O(members + samples).
/// A native executor must still construct/read/verify the actual assembly.
#[derive(Debug, PartialEq)]
pub struct ScenePlan {
    pub initially_present: Vec<usize>,
    pub introduced_by_sample: Vec<Vec<usize>>,
    pub active_counts: Vec<u64>,
}

struct SampleView<'a> {
    sample_id: &'a str,
    targets: &'a [oe_contracts::NxAssemblyPoseTarget],
}
pub fn plan(t: &NxAssemblySceneTimeline, budget: &NxAssemblyBudget) -> Result<ScenePlan> {
    crate::assembly::budget_valid(budget)?;
    t.validate()
        .map_err(|_| anyhow::anyhow!("scene_generated_contract_invalid"))?;
    ensure!(
        t.schema == "oev2.nx-assembly-scene-timeline/1",
        "scene_schema_invalid"
    );
    let mut previous = None;
    for s in &t.samples {
        super::sample_valid(s)?;
        ensure!(
            previous.is_none_or(|p| s.time_s > p),
            "scene_time_not_increasing"
        );
        previous = Some(s.time_s);
    }
    let samples = t
        .samples
        .iter()
        .map(|s| SampleView {
            sample_id: &s.sample_id,
            targets: &s.targets,
        })
        .collect::<Vec<_>>();
    plan_members(&t.members, &samples, budget)
}
/// Ordinal geometric samples have no duration, velocity or acceleration meaning.
pub fn plan_ordered(
    t: &oe_contracts::NxAssemblyOrderedScene,
    budget: &NxAssemblyBudget,
) -> Result<ScenePlan> {
    crate::assembly::budget_valid(budget)?;
    t.validate()
        .map_err(|_| anyhow::anyhow!("scene_generated_contract_invalid"))?;
    ensure!(
        t.schema == "oev2.nx-assembly-ordered-scene/2",
        "scene_schema_invalid"
    );
    for (i, s) in t.samples.iter().enumerate() {
        ensure!(s.ordinal == i as u64, "scene_ordinal_not_contiguous");
        ensure!(
            !s.sample_id.is_empty() && s.sample_id.len() <= 256,
            "motion_sample_invalid"
        );
        super::targets_valid(&s.targets)?;
    }
    let samples = t
        .samples
        .iter()
        .map(|s| SampleView {
            sample_id: &s.sample_id,
            targets: &s.targets,
        })
        .collect::<Vec<_>>();
    plan_members(&t.members, &samples, budget)
}
fn plan_members(
    members: &[oe_contracts::NxAssemblySceneMember],
    samples: &[SampleView<'_>],
    budget: &NxAssemblyBudget,
) -> Result<ScenePlan> {
    ensure!(
        !members.is_empty()
            && members.len() as u64 <= budget.max_occurrences
            && !samples.is_empty()
            && samples.len() <= 4096,
        "scene_input_budget"
    );
    let mut sample_ids = BTreeMap::new();
    for (i, s) in samples.iter().enumerate() {
        ensure!(
            sample_ids.insert(s.sample_id, i).is_none(),
            "scene_duplicate_sample"
        );
    }
    let mut paths = BTreeMap::new();
    let mut prototypes = BTreeSet::new();
    let mut start = Vec::with_capacity(members.len());
    let mut plan = ScenePlan {
        initially_present: Vec::new(),
        introduced_by_sample: vec![Vec::new(); samples.len()],
        active_counts: Vec::new(),
    };
    for (i, m) in members.iter().enumerate() {
        let p = &m.initial_pose;
        ensure!(
            !p.occurrence_path.is_empty()
                && p.occurrence_path.len() as u64 <= budget.max_depth
                && p.occurrence_path
                    .iter()
                    .all(|n| !n.is_empty() && n.len() <= 256 && !n.chars().any(char::is_control)),
            "scene_member_path_invalid"
        );
        ensure!(
            paths.insert(p.occurrence_path.clone(), i).is_none(),
            "scene_duplicate_occurrence_path"
        );
        oe_nx_protocol::RelativePath::new(p.prototype_id.clone())?;
        super::proper(&p.local_to_root_m)?;
        prototypes.insert(p.prototype_id.as_str());
        let at = m
            .introduction_sample
            .as_ref()
            .map(|id| {
                sample_ids
                    .get(id.as_str())
                    .copied()
                    .context("scene_introduction_sample_missing")
            })
            .transpose()?;
        match at {
            None => plan.initially_present.push(i),
            Some(s) => plan.introduced_by_sample[s].push(i),
        }
        start.push(at);
    }
    // The output root is another prototype, not one of its placed members.
    ensure!(
        (prototypes.len() as u64) < budget.max_prototypes,
        "scene_prototype_budget_including_root"
    );
    for (i, m) in members.iter().enumerate() {
        let path = &m.initial_pose.occurrence_path;
        if path.len() > 1 {
            let parent = *paths
                .get(&path[..path.len() - 1])
                .context("scene_parent_missing")?;
            // None means present before every sample (including sample zero).
            ensure!(start[parent] <= start[i], "scene_parent_enters_after_child");
        }
    }
    let sort = |indices: &mut Vec<usize>| {
        indices.sort_by(|a, b| {
            let pa = &members[*a].initial_pose.occurrence_path;
            let pb = &members[*b].initial_pose.occurrence_path;
            pa.len().cmp(&pb.len()).then(pa.cmp(pb))
        })
    };
    sort(&mut plan.initially_present);
    let mut count = plan.initially_present.len() as u64;
    for (sidx, s) in samples.iter().enumerate() {
        sort(&mut plan.introduced_by_sample[sidx]);
        let mut targeted = BTreeMap::new();
        for target in s.targets {
            let member = *paths
                .get(&target.occurrence_path)
                .context("scene_target_undeclared")?;
            ensure!(
                start[member].is_none_or(|at| at <= sidx),
                "scene_target_before_introduction"
            );
            ensure!(
                target.prototype_id == members[member].initial_pose.prototype_id,
                "scene_target_prototype_differs"
            );
            targeted.insert(member, target);
        }
        for member in &plan.introduced_by_sample[sidx] {
            let target = targeted
                .get(member)
                .context("scene_introduction_pose_missing")?;
            ensure!(
                super::proper(&target.local_to_root_m)?.near(super::proper(
                    &members[*member].initial_pose.local_to_root_m
                )?),
                "scene_introduction_pose_differs"
            );
        }
        count += plan.introduced_by_sample[sidx].len() as u64;
        plan.active_counts.push(count);
    }
    ensure!(
        count == members.len() as u64,
        "scene_final_membership_incomplete"
    );
    Ok(plan)
}
