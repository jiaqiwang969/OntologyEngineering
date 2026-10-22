use oe_contracts::*;
use oe_nx_worker::{assembly::Rigid, assembly_motion::scene::plan};
fn pose(path: &[&str], x: f64) -> NxAssemblyPoseTarget {
    let mut m = Rigid::identity().0.to_vec();
    m[3] = x;
    NxAssemblyPoseTarget {
        occurrence_path: path.iter().map(|s| s.to_string()).collect(),
        prototype_id: "bolt.prt".into(),
        local_to_root_m: m,
    }
}
fn fixture() -> (NxAssemblySceneTimeline, NxAssemblyBudget) {
    let a = pose(&["bolt_a"], 0.45);
    let b = pose(&["bolt_b"], -0.45);
    let tool = pose(&["tool"], 0.7);
    (
        NxAssemblySceneTimeline {
            schema: "oev2.nx-assembly-scene-timeline/1".into(),
            members: vec![
                NxAssemblySceneMember {
                    initial_pose: b,
                    introduction_sample: None,
                },
                NxAssemblySceneMember {
                    initial_pose: a.clone(),
                    introduction_sample: None,
                },
                NxAssemblySceneMember {
                    initial_pose: tool.clone(),
                    introduction_sample: Some("s1".into()),
                },
            ],
            samples: vec![
                NxAssemblyMotionSample {
                    sample_id: "s0".into(),
                    time_s: 0.,
                    targets: vec![a.clone()],
                },
                NxAssemblyMotionSample {
                    sample_id: "s1".into(),
                    time_s: 1.,
                    targets: vec![tool],
                },
                NxAssemblyMotionSample {
                    sample_id: "s2".into(),
                    time_s: 2.,
                    targets: vec![a],
                },
            ],
        },
        NxAssemblyBudget {
            max_occurrences: 10,
            max_prototypes: 4,
            max_depth: 4,
        },
    )
}
#[test]
fn duplicate_prototypes_preserve_instances_and_staged_tools() {
    let (t, b) = fixture();
    let p = plan(&t, &b).unwrap();
    assert_eq!(p.initially_present, vec![1, 0]);
    assert_eq!(p.introduced_by_sample, vec![vec![], vec![2], vec![]]);
    assert_eq!(p.active_counts, vec![2, 3, 3]);
    assert_eq!(t.members[1].initial_pose.local_to_root_m[3], 0.45);
}
fn reject(t: &NxAssemblySceneTimeline, b: &NxAssemblyBudget, reason: &str) {
    assert!(
        plan(t, b).unwrap_err().to_string().contains(reason),
        "expected {reason}"
    );
}
#[test]
fn rejects_invalid_membership_and_introduction() {
    let (t, b) = fixture();
    let mut x = t.clone();
    x.members.push(x.members[0].clone());
    reject(&x, &b, "scene_duplicate_occurrence_path");
    let mut x = t.clone();
    x.members[2].introduction_sample = Some("absent".into());
    reject(&x, &b, "scene_introduction_sample_missing");
    let mut x = t.clone();
    x.samples[0].targets.push(x.members[2].initial_pose.clone());
    reject(&x, &b, "scene_target_before_introduction");
    let mut x = t.clone();
    x.samples[1].targets = vec![x.members[0].initial_pose.clone()];
    reject(&x, &b, "scene_introduction_pose_missing");
    let mut x = t.clone();
    x.samples[1].targets[0].local_to_root_m[3] += 0.1;
    reject(&x, &b, "scene_introduction_pose_differs");
    let mut x = t.clone();
    x.samples[1].targets[0].prototype_id = "other.prt".into();
    reject(&x, &b, "scene_target_prototype_differs");
    let mut x = t.clone();
    x.samples[1].targets[0].occurrence_path = vec!["unknown".into()];
    reject(&x, &b, "scene_target_undeclared");
    let mut x = t.clone();
    x.members[0].initial_pose.occurrence_path = vec!["missing".into(), "child".into()];
    reject(&x, &b, "scene_parent_missing");
    let mut x = t.clone();
    x.members[0].initial_pose.occurrence_path = vec!["tool".into(), "child".into()];
    reject(&x, &b, "scene_parent_enters_after_child");
    let mut x = t.clone();
    x.members[0].initial_pose.local_to_root_m[0] = 2.;
    reject(&x, &b, "assembly_rotation_not_orthogonal");
    let mut x = t.clone();
    x.members[0].initial_pose.local_to_root_m[3] = f64::NAN;
    assert!(plan(&x, &b).is_err());
    let mut low = b.clone();
    low.max_occurrences = 2;
    reject(&t, &low, "scene_input_budget");
    let mut low = b.clone();
    low.max_prototypes = 1;
    reject(&t, &low, "scene_prototype_budget_including_root");
}
#[test]
fn nested_additions_are_parent_first_and_samples_are_unique() {
    let (mut t, b) = fixture();
    let child = pose(&["tool", "child"], 0.8);
    t.members.insert(
        0,
        NxAssemblySceneMember {
            initial_pose: child.clone(),
            introduction_sample: Some("s1".into()),
        },
    );
    t.samples[1].targets.push(child);
    let p = plan(&t, &b).unwrap();
    assert_eq!(p.introduced_by_sample[1], vec![3, 0]);
    assert_eq!(p.active_counts, vec![2, 4, 4]);
    let mut x = t.clone();
    x.samples[2].sample_id = "s1".into();
    reject(&x, &b, "scene_duplicate_sample");
    let mut x = t.clone();
    x.samples[2].time_s = 1.;
    reject(&x, &b, "scene_time_not_increasing");
}

#[test]
fn ordered_scene_preserves_geometry_without_fabricated_time() {
    use oe_nx_worker::assembly_motion::scene::plan_ordered;
    let (t, b) = fixture();
    let expected = plan(&t, &b).unwrap();
    let ordered = NxAssemblyOrderedScene {
        schema: "oev2.nx-assembly-ordered-scene/2".into(),
        members: t.members.clone(),
        samples: t
            .samples
            .iter()
            .enumerate()
            .map(|(i, s)| NxAssemblyOrderedSample {
                sample_id: s.sample_id.clone(),
                ordinal: i as u64,
                targets: s.targets.clone(),
            })
            .collect(),
    };
    assert_eq!(plan_ordered(&ordered, &b).unwrap(), expected);
    let encoded = serde_json::to_value(&ordered).unwrap();
    assert!(encoded["samples"][0].get("time_s").is_none());
    for n in [0, 7] {
        let mut x = ordered.clone();
        x.samples[1].ordinal = n;
        assert!(
            plan_ordered(&x, &b)
                .unwrap_err()
                .to_string()
                .contains("scene_ordinal_not_contiguous")
        );
    }
    let mut x = ordered.clone();
    x.samples[0].targets.push(x.members[2].initial_pose.clone());
    assert!(
        plan_ordered(&x, &b)
            .unwrap_err()
            .to_string()
            .contains("scene_target_before_introduction")
    );
    let mut x = ordered.clone();
    x.samples[1].sample_id = x.samples[0].sample_id.clone();
    assert!(
        plan_ordered(&x, &b)
            .unwrap_err()
            .to_string()
            .contains("scene_duplicate_sample")
    );
    let mut timed = t.clone();
    timed.samples[1].time_s = timed.samples[0].time_s;
    assert!(
        plan(&timed, &b)
            .unwrap_err()
            .to_string()
            .contains("scene_time_not_increasing")
    );
}
