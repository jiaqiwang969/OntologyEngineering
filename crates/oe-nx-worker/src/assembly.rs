//! Full assembly math and structural checks. Native acquisition stays in Rust;
//! NX tags are meaningful only within the bound outer native-result process.
use crate::geometry::{NativeApi, Workspace};
use anyhow::{Context, Result, ensure};
use oe_contracts::{NxAssemblyBudget, NxAssemblyFixtureIntent, NxAssemblyObservation};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const PARAMS_SCHEMA: &str = "cad-agent.nx-native-assembly-params/v1";
pub const NXOPEN_PARAMS_SCHEMA: &str = "cad-agent.nx-native-assembly-params/v2";
pub const OBSERVATION_SCHEMA: &str = "oev2.nx-native-assembly-observation/1";
pub const POSITION_TOLERANCE_M: f64 = 1e-10;
pub const ROTATION_TOLERANCE: f64 = 1e-12;

/// The cycle selects the SDK's solid object type, not body subtype alone.
/// SDK subtypes1..6 are known solid subobjects; unknown/query-failed values
/// cannot establish a complete prototype body inventory.
pub fn prototype_body_candidate(
    type_code: i32,
    object_type: i32,
    subtype: i32,
    solid_type: i32,
    owner_code: i32,
    owner: u32,
    expected_owner: u32,
    occurrence: u8,
) -> Result<bool> {
    ensure!(
        type_code == 0,
        "assembly_cycle_type_query_failed:{type_code}"
    );
    ensure!(
        owner_code == 0 && owner != 0,
        "assembly_cycle_owner_query_failed:{owner_code}"
    );
    ensure!(occurrence <= 1, "assembly_cycle_occurrence_logical_invalid");
    ensure!(
        object_type == solid_type && (0..=6).contains(&subtype),
        "assembly_cycle_object_type_or_subtype_unknown:{object_type}/{subtype}"
    );
    Ok(subtype == 0 && owner == expected_owner && occurrence == 0)
}

pub fn validate_construction_profile(
    profile: &oe_contracts::NxAssemblyConstructionProfile,
) -> Result<()> {
    ensure!(
        profile.schema == "oev2.nx-assembly-construction-profile/1"
            && profile.backend == "nxopen_loaded_part_entire",
        "nxopen_construction_profile_unknown"
    );
    Ok(())
}
/// SDK AddComponent orientation is column order. Keep the source row-major
/// rotation separate; the explicit profile accepts proper rigid rotations.
pub fn nxopen_column_rotation(rotation: &[f64]) -> Result<[f64; 9]> {
    Rigid::from_rotation_origin(rotation, &[0., 0., 0.])?;
    let r = rotation;
    let determinant = r[0] * (r[4] * r[8] - r[5] * r[7]) - r[1] * (r[3] * r[8] - r[5] * r[6])
        + r[2] * (r[3] * r[7] - r[4] * r[6]);
    ensure!(
        (determinant - 1.).abs() <= ROTATION_TOLERANCE,
        "nxopen_profile_requires_proper_rotation"
    );
    Ok(std::array::from_fn(|i| r[(i % 3) * 3 + i / 3]))
}
pub fn check_nxopen_add(
    value: &oe_contracts::NxAssemblyAddOutcome,
    parent: u32,
    child: u32,
) -> Result<()> {
    ensure!(
        value.normal_return
            && value.status_present
            && value.status_copied
            && value.status_released
            && value.complete
            && value.unloaded_count == 0,
        "nxopen_add_status_incomplete_or_partial"
    );
    ensure!(
        value.component_tag != 0
            && value.definition_instance_tag != 0
            && value.prototype_tag == u64::from(child)
            && value.definition_parent_tag == u64::from(parent),
        "nxopen_add_parent_or_prototype_differs"
    );
    ensure!(
        !value.entire_reference_set.is_empty() && value.reference_set == value.entire_reference_set,
        "nxopen_add_reference_set_not_entire"
    );
    Ok(())
}

pub fn unit_to_m(unit: &str) -> Result<f64> {
    match unit {
        "mm" => Ok(0.001),
        "inch" => Ok(0.0254),
        _ => anyhow::bail!("assembly_native_unit_unresolved"),
    }
}
pub fn budget_valid(value: &NxAssemblyBudget) -> Result<()> {
    ensure!(
        (1..=100_000).contains(&value.max_occurrences)
            && (1..=4096).contains(&value.max_prototypes)
            && (1..=64).contains(&value.max_depth),
        "assembly_resource_budget_invalid"
    );
    Ok(())
}

/// Row-major SI matrix, acting on SI column points. Reflection is permitted by
/// the installed UF assembly API; shear, scaling and singular frames are not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rigid(pub [f64; 16]);
impl Rigid {
    pub fn identity() -> Self {
        Self([
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ])
    }
    pub fn checked(values: &[f64]) -> Result<Self> {
        let a: [f64; 16] = values
            .try_into()
            .map_err(|_| anyhow::anyhow!("assembly_matrix16_required"))?;
        ensure!(
            a.iter().all(|v| v.is_finite()),
            "assembly_transform_nonfinite"
        );
        ensure!(
            a[12].abs() <= ROTATION_TOLERANCE
                && a[13].abs() <= ROTATION_TOLERANCE
                && a[14].abs() <= ROTATION_TOLERANCE
                && (a[15] - 1.).abs() <= ROTATION_TOLERANCE,
            "assembly_canonical_homogeneous_row_invalid"
        );
        for i in 0..3 {
            for j in 0..3 {
                let dot = (0..3).map(|k| a[k * 4 + i] * a[k * 4 + j]).sum::<f64>();
                ensure!(
                    (dot - if i == j { 1. } else { 0. }).abs() <= ROTATION_TOLERANCE,
                    "assembly_rotation_not_orthogonal"
                );
            }
        }
        Ok(Self(a))
    }
    pub fn from_rotation_origin(rotation: &[f64], origin_m: &[f64]) -> Result<Self> {
        ensure!(
            rotation.len() == 9 && origin_m.len() == 3,
            "assembly_rotation9_origin3_required"
        );
        let mut a = Self::identity().0;
        for row in 0..3 {
            for col in 0..3 {
                a[row * 4 + col] = rotation[row * 3 + col];
            }
            a[row * 4 + 3] = origin_m[row];
        }
        Self::checked(&a)
    }
    /// UF component CSYS contains three axis vectors, the transpose of the
    /// returned full transform's upper 3x3. Origins are parent native units.
    pub fn from_parent_component(
        origin_native: &[f64],
        csys: &[f64],
        parent_unit: &str,
    ) -> Result<Self> {
        ensure!(
            origin_native.len() == 3 && csys.len() == 9,
            "assembly_native_component_shape_invalid"
        );
        let scale = unit_to_m(parent_unit)?;
        let mut rotation = [0.; 9];
        for r in 0..3 {
            for c in 0..3 {
                rotation[r * 3 + c] = csys[c * 3 + r];
            }
        }
        Self::from_rotation_origin(
            &rotation,
            &origin_native.iter().map(|x| x * scale).collect::<Vec<_>>(),
        )
    }
    /// Installed UF_ASSEM_ask_comp_position documents root-absolute data and
    /// a homogeneous scale. Divide each origin coordinate by w, then convert
    /// root-native lengths to SI. Preserve raw16 separately for the oracle.
    pub fn from_root_native(raw: &[f64], root_unit: &str) -> Result<Self> {
        ensure!(
            raw.len() == 16 && raw.iter().all(|v| v.is_finite()),
            "assembly_raw_transform16_invalid"
        );
        ensure!(
            raw[12].abs() <= ROTATION_TOLERANCE
                && raw[13].abs() <= ROTATION_TOLERANCE
                && raw[14].abs() <= ROTATION_TOLERANCE
                && raw[15] > 0.,
            "assembly_native_homogeneous_scale_invalid"
        );
        let unit = unit_to_m(root_unit)?;
        let mut a: [f64; 16] = raw.try_into().unwrap();
        for index in [3, 7, 11] {
            a[index] = a[index] / raw[15] * unit;
        }
        a[15] = 1.;
        Self::checked(&a)
    }
    pub fn compose(self, child: Self) -> Result<Self> {
        let mut a = [0.; 16];
        for r in 0..4 {
            for c in 0..4 {
                a[r * 4 + c] = (0..4).map(|k| self.0[r * 4 + k] * child.0[k * 4 + c]).sum();
            }
        }
        Self::checked(&a)
    }
    pub fn inverse(self) -> Result<Self> {
        let mut a = Self::identity().0;
        for r in 0..3 {
            for c in 0..3 {
                a[r * 4 + c] = self.0[c * 4 + r];
            }
            a[r * 4 + 3] = -(0..3)
                .map(|k| a[r * 4 + k] * self.0[k * 4 + 3])
                .sum::<f64>();
        }
        Self::checked(&a)
    }
    pub fn point(self, point_m: [f64; 3]) -> Result<[f64; 3]> {
        ensure!(
            point_m.iter().all(|x| x.is_finite()),
            "assembly_point_nonfinite"
        );
        let out = std::array::from_fn(|r| {
            (0..3).map(|c| self.0[r * 4 + c] * point_m[c]).sum::<f64>() + self.0[r * 4 + 3]
        });
        ensure!(
            out.iter().all(|x| x.is_finite()),
            "assembly_transformed_point_nonfinite"
        );
        Ok(out)
    }
    /// Transform a direction without translation or large-origin cancellation.
    pub fn vector(self, vector: [f64; 3]) -> Result<[f64; 3]> {
        ensure!(vector.iter().all(|v| v.is_finite()), "assembly_vector_nonfinite");
        let out = std::array::from_fn(|r| {
            (0..3).map(|c| self.0[r * 4 + c] * vector[c]).sum::<f64>()
        });
        ensure!(out.iter().all(|v| v.is_finite()), "assembly_transformed_vector_nonfinite");
        Ok(out)
    }
    pub fn near(self, other: Self) -> bool {
        self.0.iter().zip(other.0).enumerate().all(|(i, (a, b))| {
            (*a - b).abs()
                <= if [3, 7, 11].contains(&i) {
                    POSITION_TOLERANCE_M
                } else {
                    ROTATION_TOLERANCE
                }
        })
    }
}

pub fn validate_observation(
    value: &NxAssemblyObservation,
    budget: &NxAssemblyBudget,
) -> Result<()> {
    validate_observation_with_overrides(value, budget, &BTreeSet::new())
}

// Called only after the explicit /2 override inventory is checked.
pub(crate) fn validate_observation_with_overrides(
    value: &NxAssemblyObservation,
    budget: &NxAssemblyBudget,
    overridden: &BTreeSet<String>,
) -> Result<()> {
    budget_valid(budget)?;
    value
        .validate()
        .map_err(|_| anyhow::anyhow!("assembly_generated_observation_invalid"))?;
    ensure!(
        value.schema == OBSERVATION_SCHEMA
            && value.complete
            && value.engineering_adoption == "not_assessed",
        "assembly_observation_scope_incomplete"
    );
    ensure!(
        !value.prototypes.is_empty()
            && value.prototypes.len() as u64 <= budget.max_prototypes
            && value.occurrences.len() as u64 <= budget.max_occurrences,
        "assembly_native_inventory_budget"
    );
    let mut prototypes = BTreeMap::new();
    let mut part_tags = BTreeSet::new();
    for p in &value.prototypes {
        let scale = unit_to_m(&p.native_units)?;
        oe_nx_protocol::RelativePath::new(p.prototype_id.clone())?;
        ensure!(
            p.native_part_tag > 0
                && p.native_part_tag <= u32::MAX as u64
                && p.fully_loaded
                && p.unit_to_m == scale
                && !p.part_path.is_empty()
                && prototypes.insert(p.prototype_id.as_str(), p).is_none()
                && part_tags.insert(p.native_part_tag),
            "assembly_prototype_unresolved_or_aliased"
        );
        let mut body_tags = BTreeSet::new();
        for s in &p.solids {
            ensure!(
                s.body_tag > 0
                    && s.body_tag <= u32::MAX as u64
                    && body_tags.insert(s.body_tag)
                    && s.owning_part_tag == p.native_part_tag
                    && !s.is_occurrence,
                "assembly_prototype_body_owner_or_occurrence_differs"
            );
            ensure!(
                s.exact_box_corners_native.len() == 24
                    && s.exact_box_corners_m.len() == 24
                    && s.exact_box_corners_native
                        .iter()
                        .chain(&s.exact_box_corners_m)
                        .all(|v| v.is_finite()),
                "assembly_prototype_native_geometry_incomplete"
            );
            ensure!(
                s.exact_box_corners_native
                    .iter()
                    .zip(&s.exact_box_corners_m)
                    .all(|(n, m)| (n * scale - m).abs() <= POSITION_TOLERANCE_M),
                "assembly_geometry_unit_conversion_differs"
            );
            if let Some(volume) = s.volume_m3 {
                ensure!(
                    volume.is_finite() && volume > 0.,
                    "assembly_solid_volume_invalid"
                );
            }
        }
    }
    let root = prototypes
        .get(value.root_prototype_id.as_str())
        .context("assembly_root_prototype_missing")?;
    let mut occurrences = BTreeMap::new();
    let mut occurrence_tags = BTreeSet::new();
    for o in &value.occurrences {
        ensure!(
            !o.occurrence_id.is_empty()
                && occurrences.insert(o.occurrence_id.as_str(), o).is_none()
                && o.part_occurrence_tag > 0
                && occurrence_tags.insert(o.part_occurrence_tag)
                && o.part_occurrence_tag <= u32::MAX as u64
                && o.instance_tag > 0
                && o.instance_tag <= u32::MAX as u64
                && !o.name.is_empty()
                && prototypes.contains_key(o.prototype_id.as_str()),
            "assembly_occurrence_or_prototype_missing_or_duplicated"
        );
    }
    for o in &value.occurrences {
        let parent = if let Some(id) = &o.parent_occurrence_id {
            let parent = occurrences
                .get(id.as_str())
                .context("assembly_parent_occurrence_missing")?;
            (
                Rigid::checked(&parent.local_to_root_m)?,
                *prototypes.get(parent.prototype_id.as_str()).unwrap(),
            )
        } else {
            (Rigid::identity(), *root)
        };
        let local = Rigid::checked(&o.local_to_parent_m)?;
        let global = Rigid::checked(&o.local_to_root_m)?;
        ensure!(
            parent.0.compose(local)?.near(global),
            "assembly_nested_transform_composition_differs"
        );
        ensure!(
            Rigid::from_root_native(&o.raw_root_transform, &root.native_units)?.near(global),
            "assembly_raw_root_transform_differs"
        );
        ensure!(
            o.raw_parent_transform.len() == 16
                && o.raw_parent_transform.iter().all(|v| v.is_finite()),
            "assembly_full_parent_transform_missing"
        );
        // "None" is the installed SDK's entire-part reference-set sentinel.
        // Other selections retain their raw data; their refset CSYS can offset
        // component-data origins, so root-absolute native transforms are primary.
        if o.reference_set == "None" && !overridden.contains(&o.occurrence_id) {
            ensure!(
                Rigid::from_parent_component(
                    &o.raw_parent_origin,
                    &o.raw_parent_csys,
                    &parent.1.native_units
                )?
                .near(local),
                "assembly_native_parent_frame_differs"
            );
            ensure!(
                Rigid::from_root_native(&o.raw_parent_transform, &parent.1.native_units)?
                    .near(local),
                "assembly_full_parent_transform_differs"
            );
        }
        let mut ancestors = BTreeSet::new();
        let mut next = Some(o);
        let mut depth = 0u64;
        while let Some(node) = next {
            ensure!(
                ancestors.insert(node.occurrence_id.as_str()),
                "assembly_hierarchy_cycle"
            );
            depth += 1;
            ensure!(depth <= budget.max_depth, "assembly_depth_budget_exceeded");
            next = match node.parent_occurrence_id.as_deref() {
                Some(id) => Some(
                    *occurrences
                        .get(id)
                        .context("assembly_parent_occurrence_missing")?,
                ),
                None => None,
            };
        }
    }
    Ok(())
}

fn fixture_order(intent: &NxAssemblyFixtureIntent) -> Result<Vec<String>> {
    fn visit(
        id: &str,
        intent: &NxAssemblyFixtureIntent,
        visiting: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
        out: &mut Vec<String>,
    ) -> Result<()> {
        if done.contains(id) {
            return Ok(());
        }
        ensure!(visiting.insert(id.into()), "fixture_definition_cycle");
        for e in intent
            .instances
            .iter()
            .filter(|e| e.parent_prototype_id == id)
        {
            visit(&e.prototype_id, intent, visiting, done, out)?;
        }
        visiting.remove(id);
        done.insert(id.into());
        out.push(id.into());
        Ok(())
    }
    let mut out = Vec::new();
    visit(
        &intent.root_prototype_id,
        intent,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut out,
    )?;
    ensure!(
        out.len() == intent.prototypes.len(),
        "fixture_unreachable_prototype"
    );
    Ok(out)
}
pub fn validate_fixture_intent(intent: &NxAssemblyFixtureIntent) -> Result<()> {
    intent
        .validate()
        .map_err(|_| anyhow::anyhow!("assembly_fixture_generated_intent_invalid"))?;
    ensure!(
        intent.schema == "oev2.nx-native-assembly-fixture-intent/1",
        "assembly_fixture_schema_invalid"
    );
    oe_nx_protocol::Digest::new(intent.oracle_sha256.clone())?;
    ensure!(
        !intent.prototypes.is_empty()
            && intent.prototypes.len() <= 128
            && intent.instances.len() <= 1024,
        "assembly_fixture_resource_bound"
    );
    let mut ids = BTreeSet::new();
    for p in &intent.prototypes {
        ensure!(
            !p.prototype_id.is_empty()
                && p.prototype_id.len() <= 64
                && p.prototype_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && ids.insert(p.prototype_id.as_str()),
            "assembly_fixture_prototype_id_invalid"
        );
        unit_to_m(&p.native_units)?;
        ensure!(
            (p.box_corner_native.is_empty() && p.box_lengths_native.is_empty())
                || (p.box_corner_native.len() == 3
                    && p.box_lengths_native.len() == 3
                    && p.box_corner_native.iter().all(|v| v.is_finite())
                    && p.box_lengths_native
                        .iter()
                        .all(|v| v.is_finite() && *v > 0. && *v <= 1e6)),
            "assembly_fixture_box_invalid"
        );
    }
    ensure!(
        ids.contains(intent.root_prototype_id.as_str()),
        "fixture_root_missing"
    );
    let mut edges = BTreeSet::new();
    for e in &intent.instances {
        ensure!(
            ids.contains(e.parent_prototype_id.as_str())
                && ids.contains(e.prototype_id.as_str())
                && e.parent_prototype_id != e.prototype_id
                && !e.name.is_empty()
                && e.name.len() <= 64
                && e.name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && edges.insert((e.parent_prototype_id.as_str(), e.name.as_str())),
            "fixture_instance_identity_invalid"
        );
        Rigid::from_rotation_origin(&e.rotation, &e.origin_m)?;
    }
    fixture_order(intent)?;
    Ok(())
}

pub fn create_fixture(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    intent: &NxAssemblyFixtureIntent,
    budget: &NxAssemblyBudget,
) -> Result<Value> {
    create_fixture_backend(api, workspace, intent, budget, false)
}
pub fn create_fixture_nxopen(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    intent: &NxAssemblyFixtureIntent,
    budget: &NxAssemblyBudget,
    profile: &oe_contracts::NxAssemblyConstructionProfile,
) -> Result<Value> {
    validate_construction_profile(profile)?;
    for e in &intent.instances {
        nxopen_column_rotation(&e.rotation)?;
    }
    create_fixture_backend(api, workspace, intent, budget, true)
}
fn create_fixture_backend(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    intent: &NxAssemblyFixtureIntent,
    budget: &NxAssemblyBudget,
    nxopen: bool,
) -> Result<Value> {
    validate_fixture_intent(intent)?;
    budget_valid(budget)?;
    let mut parts = BTreeMap::new();
    let mut paths = BTreeMap::new();
    for p in &intent.prototypes {
        let file = format!("{}.prt", p.prototype_id);
        let path = workspace.output(&file)?;
        let tag = api.create_assembly_units_part(&path, &p.native_units)?;
        api.set_work_part(tag)?;
        if !p.box_lengths_native.is_empty() {
            api.create_block(
                p.box_corner_native.as_slice().try_into().unwrap(),
                std::array::from_fn(|i| p.box_lengths_native[i].to_string()),
            )?;
        }
        api.save_work_part()?;
        workspace.require_regular_output(&file)?;
        parts.insert(p.prototype_id.clone(), tag);
        paths.insert(p.prototype_id.clone(), path);
    }
    let root = parts[&intent.root_prototype_id];
    let path = &paths[&intent.root_prototype_id];
    let namespace = path.parent().context("fixture_output_parent_missing")?;
    for id in fixture_order(intent)? {
        let parent = intent
            .prototypes
            .iter()
            .find(|p| p.prototype_id == id)
            .unwrap();
        let scale = unit_to_m(&parent.native_units)?;
        for e in intent
            .instances
            .iter()
            .filter(|e| e.parent_prototype_id == id)
        {
            let origin = std::array::from_fn(|i| e.origin_m[i] / scale);
            api.prepare_owned_assembly_context(parts[&id], namespace)?;
            if nxopen {
                api.add_assembly_entire_instance_nxopen(
                    parts[&id],
                    parts[&e.prototype_id],
                    &paths[&e.prototype_id],
                    &e.name,
                    origin,
                    &e.rotation,
                )?;
            } else {
                api.add_assembly_entire_instance(
                    parts[&id],
                    parts[&e.prototype_id],
                    &paths[&e.prototype_id],
                    &e.name,
                    origin,
                    &e.rotation,
                )?;
            }
        }
    }
    let before = api.full_assembly_info(root, namespace, budget, "independent_software_fixture")?;
    workspace.record(
        "assembly_fixture_before_save",
        serde_json::to_value(&before)?,
    )?;
    check_fixture(intent, &before)?;
    let save = api.save_all_owned_assembly(namespace)?;
    workspace.record("assembly_fixture_save_returned", save.clone())?;
    let after = api.full_assembly_info(root, namespace, budget, "independent_software_fixture")?;
    workspace.record("assembly_fixture_after_save", serde_json::to_value(&after)?)?;
    check_fixture(intent, &after)?;
    same_geometry(&before, &after)?;
    Ok(
        json!({"action":if nxopen{"create_assembly_fixture_nxopen"}else{"create_assembly_fixture"},"frozen_intent":intent,"before_save":before,"after_save":after,"save":save,
        "independent_reopen":"required_separate_process","source_role":"independent_software_fixture","engineering_adoption":"not_assessed"}),
    )
}

pub fn check_fixture(
    intent: &NxAssemblyFixtureIntent,
    value: &NxAssemblyObservation,
) -> Result<()> {
    validate_fixture_intent(intent)?;
    let expected_protos: BTreeMap<_, _> = intent
        .prototypes
        .iter()
        .map(|p| (format!("{}.prt", p.prototype_id), p))
        .collect();
    ensure!(
        value.root_prototype_id == format!("{}.prt", intent.root_prototype_id)
            && value.prototypes.len() == expected_protos.len(),
        "fixture_native_prototype_closure_differs"
    );
    for p in &value.prototypes {
        let source = expected_protos
            .get(&p.prototype_id)
            .context("fixture_native_prototype_unexpected")?;
        ensure!(
            p.native_units == source.native_units && p.sheet_count == 0,
            "fixture_native_units_or_sheet_differs"
        );
        if source.box_lengths_native.is_empty() {
            ensure!(p.solids.is_empty(), "fixture_empty_assembly_has_own_solid");
            continue;
        }
        ensure!(
            p.solids.len() == 1,
            "fixture_native_box_solid_count_differs"
        );
        let solid = &p.solids[0];
        let scale = unit_to_m(&p.native_units)?;
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for c in solid.exact_box_corners_m.chunks_exact(3) {
            for i in 0..3 {
                min[i] = min[i].min(c[i]);
                max[i] = max[i].max(c[i]);
            }
        }
        for i in 0..3 {
            ensure!(
                (min[i] - source.box_corner_native[i] * scale).abs() <= POSITION_TOLERANCE_M
                    && (max[i]
                        - (source.box_corner_native[i] + source.box_lengths_native[i]) * scale)
                        .abs()
                        <= POSITION_TOLERANCE_M,
                "fixture_native_box_bounds_differs"
            );
        }
        let expected = source.box_lengths_native.iter().product::<f64>() * scale.powi(3);
        let volume = solid
            .volume_m3
            .context("fixture_native_solid_volume_missing")?;
        ensure!(
            (volume - expected).abs() <= 1e-12,
            "fixture_native_solid_volume_differs"
        );
    }
    let mut expected = Vec::new();
    let mut pending = vec![(
        intent.root_prototype_id.clone(),
        None::<usize>,
        Rigid::identity(),
    )];
    while let Some((id, parent, global)) = pending.pop() {
        for e in intent
            .instances
            .iter()
            .filter(|e| e.parent_prototype_id == id)
        {
            let local = Rigid::from_rotation_origin(&e.rotation, &e.origin_m)?;
            let world = global.compose(local)?;
            let index = expected.len();
            expected.push((
                parent,
                e.name.clone(),
                format!("{}.prt", e.prototype_id),
                local,
                world,
            ));
            pending.push((e.prototype_id.clone(), Some(index), world));
        }
    }
    ensure!(
        value.occurrences.len() == expected.len(),
        "fixture_expanded_occurrence_count_differs"
    );
    let mut actual_ids = vec![String::new(); expected.len()];
    for (i, (parent, name, prototype, local, world)) in expected.iter().enumerate() {
        let parent_id = parent.map(|index| actual_ids[index].as_str());
        let matched = value
            .occurrences
            .iter()
            .filter(|o| {
                o.name == *name
                    && o.prototype_id == *prototype
                    && o.parent_occurrence_id.as_deref() == parent_id
            })
            .collect::<Vec<_>>();
        ensure!(matched.len() == 1, "fixture_named_occurrence_not_unique");
        let o = matched[0];
        ensure!(
            Rigid::checked(&o.local_to_parent_m)?.near(*local)
                && Rigid::checked(&o.local_to_root_m)?.near(*world),
            "fixture_independent_native_transform_differs"
        );
        actual_ids[i] = o.occurrence_id.clone();
    }
    Ok(())
}

/// Persistence comparison uses a pre-save observation as its baseline, not as
/// an independent geometric design oracle. Native process tags and filenames'
/// namespace prefixes are intentionally not portable identity keys.
pub fn same_geometry(before: &NxAssemblyObservation, after: &NxAssemblyObservation) -> Result<()> {
    ensure!(
        before.root_prototype_id == after.root_prototype_id
            && before.prototypes.len() == after.prototypes.len()
            && before.occurrences.len() == after.occurrences.len(),
        "assembly_persistence_closure_differs"
    );
    for p in &before.prototypes {
        let q = after
            .prototypes
            .iter()
            .find(|q| q.prototype_id == p.prototype_id)
            .context("assembly_reopened_prototype_missing")?;
        ensure!(
            p.native_units == q.native_units
                && p.solids.len() == q.solids.len()
                && p.sheet_count == q.sheet_count,
            "assembly_persistence_native_geometry_counts_or_units_differ"
        );
        for (a, b) in p.solids.iter().zip(&q.solids) {
            ensure!(
                a.exact_box_corners_m.len() == b.exact_box_corners_m.len()
                    && a.exact_box_corners_m
                        .iter()
                        .zip(&b.exact_box_corners_m)
                        .all(|(a, b)| (a - b).abs() <= POSITION_TOLERANCE_M),
                "assembly_persistence_native_bounds_differ"
            );
            ensure!(
                match (a.volume_m3, b.volume_m3) {
                    (Some(a), Some(b)) => (a - b).abs() <= 1e-12,
                    (None, None) => true,
                    _ => false,
                },
                "assembly_persistence_native_volume_differ"
            );
        }
    }
    for o in &before.occurrences {
        let q = after
            .occurrences
            .iter()
            .find(|q| q.occurrence_id == o.occurrence_id)
            .context("assembly_reopened_occurrence_missing")?;
        ensure!(
            o.parent_occurrence_id == q.parent_occurrence_id
                && o.prototype_id == q.prototype_id
                && o.name == q.name
                && o.reference_set == q.reference_set
                && Rigid::checked(&o.local_to_parent_m)?
                    .near(Rigid::checked(&q.local_to_parent_m)?)
                && Rigid::checked(&o.local_to_root_m)?.near(Rigid::checked(&q.local_to_root_m)?),
            "assembly_persistence_hierarchy_or_transform_differs"
        );
    }
    Ok(())
}

#[cfg(test)]
mod direction_receiving {
    use super::*;
    #[test]
    fn rigid_vector_keeps_translation_out_of_directions() {
        let r=Rigid::from_rotation_origin(&[0.,-1.,0.,1.,0.,0.,0.,0.,1.],&[1e16,-1e16,3.]).unwrap();
        assert_eq!(r.vector([1.,0.,0.]).unwrap(),[0.,1.,0.]);
        assert_eq!(r.point([0.,0.,0.]).unwrap(),[1e16,-1e16,3.]);
        assert!(r.vector([f64::NAN,0.,0.]).is_err());
    }
}
