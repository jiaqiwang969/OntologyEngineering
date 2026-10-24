//! Verified NX assembly observations to a source-traced visual projection.
//!
//! The adapter keeps the CAD occurrence/prototype/solid facts and emits an
//! explicitly lossy box proxy for the existing presentation/Anatomy renderer.
//! It never reads a CAD file, invents motion, or turns a visual result into an
//! engineering adoption decision.
use anyhow::{Result, ensure};
use oe_contracts::{
    CadPresentationGeometry1, CadPresentationMember1, CadPresentationProjection1,
    CadPresentationProjectionRequest1, CadPresentationSample1, NxAssemblyObservation,
    NxAssemblyOrderedScene, PresentationPart1, PresentationScene1, PresentationState1,
};
use std::collections::{BTreeMap, BTreeSet};

const EPS: f64 = 1e-9;
const MAX_RENDER_PARTS: usize = 64;

fn finite_vector(values: &[f64]) -> bool {
    values.iter().all(|value| value.is_finite())
}

fn matrix16(values: &[f64]) -> Result<()> {
    ensure!(
        values.len() == 16 && finite_vector(values),
        "cad_projection_transform_invalid"
    );
    // All matrices in the observation contract are row-major homogeneous
    // transforms applied to column vectors.  Reject affine scale/shear here;
    // a renderer proxy may approximate shape, but not source placement.
    ensure!(
        (values[12]).abs() <= EPS
            && (values[13]).abs() <= EPS
            && (values[14]).abs() <= EPS
            && (values[15] - 1.0).abs() <= EPS,
        "cad_projection_transform_invalid"
    );
    let x = [values[0], values[1], values[2]];
    let y = [values[4], values[5], values[6]];
    let z = [values[8], values[9], values[10]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    ensure!(
        (dot(x, x) - 1.0).abs() <= 1e-6
            && (dot(y, y) - 1.0).abs() <= 1e-6
            && (dot(z, z) - 1.0).abs() <= 1e-6
            && dot(x, y).abs() <= 1e-6
            && dot(x, z).abs() <= 1e-6
            && dot(y, z).abs() <= 1e-6,
        "cad_projection_transform_invalid"
    );
    Ok(())
}

fn point(matrix: &[f64], xyz: [f64; 3]) -> [f64; 3] {
    [
        matrix[0] * xyz[0] + matrix[1] * xyz[1] + matrix[2] * xyz[2] + matrix[3],
        matrix[4] * xyz[0] + matrix[5] * xyz[1] + matrix[6] * xyz[2] + matrix[7],
        matrix[8] * xyz[0] + matrix[9] * xyz[1] + matrix[10] * xyz[2] + matrix[11],
    ]
}

fn corners(values: &[f64]) -> Result<Vec<[f64; 3]>> {
    ensure!(
        values.len() == 24 && finite_vector(values),
        "cad_projection_solid_shape_invalid"
    );
    Ok(values
        .chunks_exact(3)
        .map(|chunk| [chunk[0], chunk[1], chunk[2]])
        .collect())
}

fn flatten(points: &[[f64; 3]]) -> Vec<f64> {
    points
        .iter()
        .flat_map(|point| point.iter().copied())
        .collect()
}

fn identifier(raw: &str, ordinal: usize) -> String {
    let mut out: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("part");
    }
    if out.len() > 58 {
        out.truncate(58);
    }
    format!("cad-{}-{}", out, ordinal)
}

fn proxy_part(id: String, label: String, corners: &[[f64; 3]]) -> Result<PresentationPart1> {
    ensure!(!corners.is_empty(), "cad_projection_solid_shape_invalid");
    let mut min = corners[0];
    let mut max = corners[0];
    for corner in &corners[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(corner[axis]);
            max[axis] = max[axis].max(corner[axis]);
        }
    }
    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    ensure!(
        size.iter()
            .all(|value| value.is_finite() && *value >= 0.000_001),
        "cad_projection_solid_shape_invalid"
    );
    Ok(PresentationPart1 {
        axis: vec![0.0, 0.0, 0.0],
        id,
        label,
        origin: min.to_vec(),
        parameter_id: None,
        primitive: "box".into(),
        representation: "analytic_proxy".into(),
        size: size.to_vec(),
    })
}

fn validate_observation(observation: &NxAssemblyObservation) -> Result<()> {
    observation
        .validate()
        .map_err(|errors| anyhow::anyhow!("cad_projection_observation_invalid:{errors:?}"))?;
    ensure!(
        observation.complete,
        "cad_projection_observation_incomplete"
    );
    ensure!(
        !observation.prototypes.is_empty(),
        "cad_projection_geometry_missing"
    );
    let mut prototypes = BTreeSet::new();
    for prototype in &observation.prototypes {
        prototype
            .validate()
            .map_err(|errors| anyhow::anyhow!("cad_projection_observation_invalid:{errors:?}"))?;
        ensure!(
            prototypes.insert(&prototype.prototype_id),
            "cad_projection_prototype_duplicate"
        );
        ensure!(
            prototype.fully_loaded,
            "cad_projection_prototype_not_loaded"
        );
        ensure!(
            prototype.unit_to_m.is_finite() && prototype.unit_to_m > 0.0,
            "cad_projection_unit_invalid"
        );
        for solid in &prototype.solids {
            let native = corners(&solid.exact_box_corners_native)?;
            let si = corners(&solid.exact_box_corners_m)?;
            for (native, si) in native.iter().zip(si.iter()) {
                for axis in 0..3 {
                    ensure!(
                        (native[axis] * prototype.unit_to_m - si[axis]).abs() <= 1e-9,
                        "cad_projection_unit_mismatch"
                    );
                }
            }
        }
    }
    ensure!(
        prototypes.contains(&observation.root_prototype_id),
        "cad_projection_prototype_missing"
    );
    let mut occurrences = BTreeSet::new();
    for occurrence in &observation.occurrences {
        ensure!(
            occurrences.insert(&occurrence.occurrence_id),
            "cad_projection_occurrence_duplicate"
        );
    }
    for occurrence in &observation.occurrences {
        occurrence
            .validate()
            .map_err(|errors| anyhow::anyhow!("cad_projection_observation_invalid:{errors:?}"))?;
        ensure!(
            prototypes.contains(&occurrence.prototype_id),
            "cad_projection_prototype_missing"
        );
        if let Some(parent) = &occurrence.parent_occurrence_id {
            ensure!(
                parent != &occurrence.occurrence_id && occurrences.contains(parent),
                "cad_projection_parent_missing"
            );
        }
        matrix16(&occurrence.local_to_parent_m)?;
        matrix16(&occurrence.local_to_root_m)?;
    }
    Ok(())
}

fn validate_ordered(
    scene: &NxAssemblyOrderedScene,
    observation: &NxAssemblyObservation,
) -> Result<()> {
    scene
        .validate()
        .map_err(|errors| anyhow::anyhow!("cad_projection_scene_invalid:{errors:?}"))?;
    ensure!(
        scene.schema == "oev2.nx-assembly-ordered-scene/2",
        "cad_projection_scene_invalid"
    );
    ensure!(
        !scene.members.is_empty() && !scene.samples.is_empty(),
        "cad_projection_scene_invalid"
    );
    let occurrences: BTreeMap<_, _> = observation
        .occurrences
        .iter()
        .map(|occurrence| (occurrence.occurrence_id.as_str(), occurrence))
        .collect();
    let mut by_path = BTreeMap::new();
    let mut path_cache = BTreeMap::new();
    for occurrence in &observation.occurrences {
        if let Some(path) = occurrence_path(
            occurrence.occurrence_id.as_str(),
            &occurrences,
            &mut path_cache,
            &mut BTreeSet::new(),
        ) {
            by_path.insert(path, occurrence.occurrence_id.as_str());
        }
    }
    for member in &scene.members {
        ensure!(
            !member.initial_pose.occurrence_path.is_empty(),
            "cad_projection_scene_target_invalid"
        );
        matrix16(&member.initial_pose.local_to_root_m)?;
        ensure!(
            by_path
                .get(&member.initial_pose.occurrence_path)
                .and_then(|id| occurrences.get(id))
                .is_some_and(
                    |occurrence| occurrence.prototype_id == member.initial_pose.prototype_id
                ),
            "cad_projection_scene_target_invalid"
        );
    }
    let mut last = None;
    let mut sample_ids = BTreeSet::new();
    for sample in &scene.samples {
        sample
            .validate()
            .map_err(|errors| anyhow::anyhow!("cad_projection_scene_invalid:{errors:?}"))?;
        if let Some(previous) = last {
            ensure!(
                sample.ordinal == previous + 1,
                "cad_projection_scene_invalid"
            );
        } else {
            ensure!(sample.ordinal == 0, "cad_projection_scene_invalid");
        }
        ensure!(
            sample_ids.insert(&sample.sample_id),
            "cad_projection_scene_invalid"
        );
        ensure!(
            !sample.targets.is_empty(),
            "cad_projection_scene_target_invalid"
        );
        let mut target_paths = BTreeSet::new();
        last = Some(sample.ordinal);
        for target in &sample.targets {
            target.validate().map_err(|errors| {
                anyhow::anyhow!("cad_projection_scene_target_invalid:{errors:?}")
            })?;
            matrix16(&target.local_to_root_m)?;
            ensure!(
                target_paths.insert(&target.occurrence_path),
                "cad_projection_scene_target_invalid"
            );
            ensure!(
                by_path
                    .get(&target.occurrence_path)
                    .and_then(|id| occurrences.get(id))
                    .is_some_and(|occurrence| occurrence.prototype_id == target.prototype_id),
                "cad_projection_scene_target_invalid"
            );
        }
    }
    Ok(())
}

fn occurrence_path<'a>(
    id: &'a str,
    occurrences: &BTreeMap<&'a str, &'a oe_contracts::NxAssemblyOccurrenceObservation>,
    cache: &mut BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
) -> Option<Vec<String>> {
    if let Some(path) = cache.get(id) {
        return Some(path.clone());
    }
    if !visiting.insert(id.to_owned()) {
        return None;
    }
    let occurrence = *occurrences.get(id)?;
    let mut path = match occurrence.parent_occurrence_id.as_deref() {
        Some(parent) => occurrence_path(parent, occurrences, cache, visiting)?,
        None => Vec::new(),
    };
    path.push(occurrence.name.clone());
    visiting.remove(id);
    cache.insert(id.to_owned(), path.clone());
    Some(path)
}

/// Project a complete, source-bound assembly observation into CAD provenance
/// plus a bounded renderer proxy. `ordered_scene` is preserved as source
/// samples; its ordinal is never interpreted as physical time.
pub fn project(
    request: CadPresentationProjectionRequest1,
    observation: &NxAssemblyObservation,
    ordered_scene: Option<&NxAssemblyOrderedScene>,
) -> Result<CadPresentationProjection1> {
    request
        .validate()
        .map_err(|errors| anyhow::anyhow!("cad_projection_request_invalid:{errors:?}"))?;
    request
        .assembly
        .job
        .validate()
        .map_err(|errors| anyhow::anyhow!("cad_projection_request_invalid:{errors:?}"))?;
    request
        .native_observation
        .validate()
        .map_err(|errors| anyhow::anyhow!("cad_projection_request_invalid:{errors:?}"))?;
    request
        .assembly
        .object
        .validate()
        .map_err(|errors| anyhow::anyhow!("cad_projection_request_invalid:{errors:?}"))?;
    request
        .assembly
        .project_context
        .validate()
        .map_err(|errors| anyhow::anyhow!("cad_projection_request_invalid:{errors:?}"))?;
    if let Some(ordered) = &request.ordered_scene {
        ordered
            .validate()
            .map_err(|errors| anyhow::anyhow!("cad_projection_request_invalid:{errors:?}"))?;
    }
    ensure!(
        request.projection_policy == "axis_aligned_analytic_proxy",
        "cad_projection_policy_unsupported"
    );
    validate_observation(observation)?;
    if let Some(scene) = ordered_scene {
        validate_ordered(scene, observation)?;
    }

    let prototypes: BTreeMap<_, _> = observation
        .prototypes
        .iter()
        .map(|prototype| (prototype.prototype_id.as_str(), prototype))
        .collect();
    let mut members = Vec::new();
    let mut parts = Vec::new();
    let mut losses = vec![
        "analytic_proxy_uses_axis_aligned_root_space_AABB".into(),
        "native_BRep_features_materials_and_BOM_are_not_rendered".into(),
    ];
    let mut next_part = 0usize;

    let mut add_member = |occurrence_id: String,
                          parent_occurrence_id: Option<String>,
                          prototype_id: String,
                          name: String,
                          reference_set: String,
                          local_to_parent_m: Vec<f64>,
                          local_to_root_m: Vec<f64>|
     -> Result<()> {
        let prototype = prototypes
            .get(prototype_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("cad_projection_prototype_missing"))?;
        let prototype_index = observation
            .prototypes
            .iter()
            .position(|candidate| candidate.prototype_id == prototype_id)
            .ok_or_else(|| anyhow::anyhow!("cad_projection_prototype_missing"))?;
        let mut geometries = Vec::new();
        let mut scene_part_ids = Vec::new();
        for (solid_index, solid) in prototype.solids.iter().enumerate() {
            let original = corners(&solid.exact_box_corners_m)?;
            let transformed: Vec<_> = original
                .iter()
                .map(|corner| point(&local_to_root_m, *corner))
                .collect();
            ensure!(
                finite_vector(&flatten(&transformed)),
                "cad_projection_transform_invalid"
            );
            let visual_id = identifier(&occurrence_id, next_part);
            let part = proxy_part(
                visual_id.clone(),
                format!("{} / body {}", name, solid.body_tag),
                &transformed,
            )?;
            geometries.push(CadPresentationGeometry1 {
                body_tag: solid.body_tag,
                owning_part_tag: solid.owning_part_tag,
                source_pointer: format!("/prototypes/{}/solids/{}", prototype_index, solid_index),
                exact_box_corners_m: solid.exact_box_corners_m.clone(),
                exact_box_corners_native: solid.exact_box_corners_native.clone(),
                transformed_box_corners_m: flatten(&transformed),
                native_units: prototype.native_units.clone(),
                unit_to_m: prototype.unit_to_m,
                volume_m3: solid.volume_m3,
                representation: "analytic_proxy".into(),
                projection_loss: "exact solid reduced to root-space axis-aligned box".into(),
            });
            scene_part_ids.push(visual_id);
            if parts.len() < MAX_RENDER_PARTS {
                parts.push(part);
            }
            next_part += 1;
        }
        members.push(CadPresentationMember1 {
            occurrence_id,
            parent_occurrence_id,
            prototype_id,
            name,
            reference_set,
            local_to_parent_m,
            local_to_root_m,
            geometries,
            scene_part_ids,
        });
        Ok(())
    };

    // The root is a prototype rather than an expanded occurrence.  Giving it
    // a stable synthetic visual identity makes a direct single-part NX input
    // visible while keeping the distinction explicit in the loss record.
    let root = prototypes
        .get(observation.root_prototype_id.as_str())
        .ok_or_else(|| anyhow::anyhow!("cad_projection_prototype_missing"))?;
    if !root.solids.is_empty() {
        add_member(
            format!("root:{}", root.prototype_id),
            None,
            root.prototype_id.clone(),
            root.prototype_id.clone(),
            "root_prototype".into(),
            identity(),
            identity(),
        )?;
        losses.push("root_prototype_rendered_as_explicit_synthetic_member".into());
    }
    for occurrence in &observation.occurrences {
        add_member(
            occurrence.occurrence_id.clone(),
            occurrence.parent_occurrence_id.clone(),
            occurrence.prototype_id.clone(),
            occurrence.name.clone(),
            occurrence.reference_set.clone(),
            occurrence.local_to_parent_m.clone(),
            occurrence.local_to_root_m.clone(),
        )?;
    }
    ensure!(!members.is_empty(), "cad_projection_geometry_missing");
    ensure!(
        members.iter().any(|member| !member.geometries.is_empty()),
        "cad_projection_geometry_missing"
    );
    if parts.len()
        < members
            .iter()
            .map(|member| member.scene_part_ids.len())
            .sum()
    {
        losses.push("renderer_proxy_omitted_parts_over_64_budget".into());
    }

    let samples = ordered_scene
        .map(|scene| {
            scene
                .samples
                .iter()
                .map(|sample| CadPresentationSample1 {
                    derivation: "source_absolute_targets".into(),
                    ordinal: sample.ordinal,
                    sample_id: sample.sample_id.clone(),
                    targets: sample.targets.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let renderer_proxy = if parts.is_empty() {
        None
    } else {
        if ordered_scene.is_some() {
            losses.push(
                "renderer_proxy_is_initial_pose; ordered_samples_are_retained_separately".into(),
            );
        }
        Some(PresentationScene1 {
            parameters: Vec::new(),
            parts,
            schema: "oev2.presentation-scene/1".into(),
            states: vec![PresentationState1 {
                id: "initial".into(),
                label: "CAD observation initial pose".into(),
                values: Vec::new(),
            }],
            unit: "m".into(),
        })
    };
    Ok(CadPresentationProjection1 {
        engineering_adoption: "not_assessed".into(),
        limitations: vec![
            "This result is a source-traced visual proxy, not native CAD geometry.".into(),
            "Anatomy/presentation must keep the projection receipt with the Project.".into(),
            "Engineering adoption, manufacturing suitability and BOM correctness remain unassessed.".into(),
        ],
        members,
        projection_losses: losses,
        renderer_proxy,
        request,
        samples,
        schema: "oev2.cad-presentation-projection/1".into(),
    })
}

fn identity() -> Vec<f64> {
    vec![
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use oe_contracts::{
        ArtifactReference, CadAssemblyUseSource1, NxAssemblyPrototypeObservation,
        NxAssemblySolidObservation, ObjectIdentity, ProjectReviewContext,
    };

    fn request() -> CadPresentationProjectionRequest1 {
        CadPresentationProjectionRequest1 {
            schema: "oev2.cad-presentation-projection-request/1".into(),
            assembly: CadAssemblyUseSource1 {
                job: ArtifactReference {
                    path: "var/job.json".into(),
                    sha256: "a".repeat(64),
                },
                native_result_ref: "native-result".into(),
                object: ObjectIdentity {
                    condition_scope: "test".into(),
                    object_id: "obj".into(),
                    revision: "r1".into(),
                    source_sha256: "b".repeat(64),
                },
                part_input: "root.prt".into(),
                project_context: ProjectReviewContext {
                    condition_scope: "test".into(),
                    condition_sha256: "c".repeat(64),
                    object_id: "obj".into(),
                    object_revision: "r1".into(),
                    project_id: "p".into(),
                    project_revision: "r1".into(),
                    project_sha256: "d".repeat(64),
                    schema: "oev2.project-review-context/1".into(),
                    source_sha256: "e".repeat(64),
                },
            },
            native_observation: ArtifactReference {
                path: "var/observation.json".into(),
                sha256: "f".repeat(64),
            },
            ordered_scene: None,
            projection_policy: "axis_aligned_analytic_proxy".into(),
        }
    }

    #[test]
    fn transforms_prototype_box_into_root_space() {
        let observation = NxAssemblyObservation {
            schema: "oev2.nx-native-assembly-observation/1".into(),
            root_prototype_id: "root.prt".into(),
            source_role: "independent_software_fixture".into(),
            engineering_adoption: "not_assessed".into(),
            complete: true,
            prototypes: vec![NxAssemblyPrototypeObservation {
                prototype_id: "root.prt".into(),
                part_path: "/tmp/root.prt".into(),
                native_units: "mm".into(),
                unit_to_m: 0.001,
                fully_loaded: true,
                load_state_raw: 2,
                native_part_tag: 1,
                sheet_count: 0,
                solids: vec![NxAssemblySolidObservation {
                    body_tag: 7,
                    owning_part_tag: 1,
                    is_occurrence: false,
                    volume_m3: Some(0.000001),
                    exact_box_corners_native: vec![
                        0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 0.0, 0.0,
                        10.0, 10.0, 0.0, 10.0, 0.0, 10.0, 10.0, 10.0, 10.0, 10.0,
                    ],
                    exact_box_corners_m: vec![
                        0.0, 0.0, 0.0, 0.01, 0.0, 0.0, 0.0, 0.01, 0.0, 0.01, 0.01, 0.0, 0.0, 0.0,
                        0.01, 0.01, 0.0, 0.01, 0.0, 0.01, 0.01, 0.01, 0.01, 0.01,
                    ],
                }],
            }],
            occurrences: Vec::new(),
        };
        let result = project(request(), &observation, None).unwrap();
        let scene = result.renderer_proxy.unwrap();
        crate::presentation::evaluate(&scene).unwrap();
        assert_eq!(scene.parts.len(), 1);
        assert_eq!(scene.parts[0].origin, vec![0.0, 0.0, 0.0]);
        assert_eq!(scene.parts[0].size, vec![0.01, 0.01, 0.01]);
    }
}
