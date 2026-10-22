//! Current-session solid placement acquisition. Prototype handles never become
//! interference inputs by merely relabeling an inventory record.
use super::*;
use crate::assembly_motion::binding::{ExpectedBody, QueriedBody, validate_unique_bindings};
use crate::assembly_motion::{self, PlacedBody};
use oe_contracts::{NxAssemblyBudget, NxAssemblyMotionBudget, NxAssemblyObservation};

/// Internal checked handles, valid only while this Native session and geometry
/// remain current. Not deserializable and never accepted from a caller's JSON.
#[derive(Debug)]
pub struct BoundMotionBody {
    pub placement: PlacedBody,
    pub expected: ExpectedBody,
    pub observed: QueriedBody,
}
impl Native {
    fn motion_solid_owner(&self, body: Tag) -> Result<Tag> {
        ensure!(body != 0, "native_motion_body_missing");
        let (mut kind, mut subtype, mut body_kind, mut owner) = (-1, -1, -1, 0);
        self.check(
            unsafe { (self.uf.object_type)(body, &mut kind, &mut subtype) },
            "native_motion_body_object_type",
        )?;
        ensure!(
            kind == self.abi.solid_type && subtype == 0,
            "native_motion_object_not_body"
        );
        self.check(
            unsafe { (self.uf.body_type)(body, &mut body_kind) },
            "native_motion_body_kind",
        )?;
        ensure!(
            body_kind == self.abi.solid_body,
            "native_motion_object_not_solid"
        );
        self.check(
            unsafe { (self.uf.owning_part)(body, &mut owner) },
            "native_motion_body_owner",
        )?;
        ensure!(owner != 0, "native_motion_body_owner_missing");
        Ok(owner)
    }
    /// Acquire an initial complete /1 assembly inventory, then resolve every
    /// placement against live UF identities. Position overrides need their own
    /// versioned observation path; this function does not bypass the /1 checks.
    pub fn motion_body_inventory(
        &mut self,
        part: Tag,
        namespace: &Path,
        inventory_budget: &NxAssemblyBudget,
        motion_budget: &NxAssemblyMotionBudget,
        samples: u64,
    ) -> Result<(NxAssemblyObservation, Vec<BoundMotionBody>)> {
        let observation = self.observe_assembly(
            part,
            namespace,
            inventory_budget,
            "current_motion_initial_inventory",
        )?;
        let plan =
            assembly_motion::plan_pairs(&observation, inventory_budget, motion_budget, samples)?;
        let out = self.bind_motion_bodies(part, &observation, plan.bodies)?;
        Ok((observation, out))
    }
    fn bind_motion_bodies(
        &self,
        part: Tag,
        observation: &NxAssemblyObservation,
        placements: Vec<PlacedBody>,
    ) -> Result<Vec<BoundMotionBody>> {
        let mut out = Vec::with_capacity(placements.len());
        for placement in placements {
            let p = observation
                .prototypes
                .iter()
                .find(|p| p.prototype_id == placement.prototype_id)
                .context("native_motion_prototype_missing")?;
            let proto = Tag::try_from(placement.prototype_body_tag)?;
            let proto_owner = Tag::try_from(p.native_part_tag)?;
            ensure!(
                self.motion_solid_owner(proto)? == proto_owner
                    && unsafe { (self.uf.is_occurrence)(proto) } == 0,
                "native_motion_prototype_body_differs"
            );
            let part_occurrence = placement
                .occurrence_id
                .as_ref()
                .map(|id| {
                    let o = observation
                        .occurrences
                        .iter()
                        .find(|o| &o.occurrence_id == id)
                        .context("native_motion_part_occurrence_missing")?;
                    let tag = Tag::try_from(o.part_occurrence_tag)?;
                    ensure!(
                        unsafe { (self.uf.is_occurrence)(tag) } == 1
                            && unsafe { (self.uf.prototype)(tag) } == proto_owner,
                        "native_motion_part_occurrence_prototype_differs"
                    );
                    Ok::<Tag, anyhow::Error>(tag)
                })
                .transpose()?;
            let expected = ExpectedBody {
                prototype_body: proto,
                prototype_owner: proto_owner,
                part_occurrence,
            };
            let body = match part_occurrence {
                Some(occ) => unsafe { (self.uf.find_occurrence)(occ, proto) },
                None => {
                    ensure!(proto_owner == part, "native_motion_root_owner_differs");
                    proto
                }
            };
            let owner = self.motion_solid_owner(body)?;
            let logical = unsafe { (self.uf.is_occurrence)(body) };
            ensure!(logical <= 1, "native_motion_occurrence_logical_invalid");
            let observed = QueriedBody {
                body,
                owner,
                is_solid_body: true,
                is_occurrence: logical == 1,
                prototype_body: if logical == 1 {
                    unsafe { (self.uf.prototype)(body) }
                } else {
                    body
                },
                containing_occurrence: if logical == 1 {
                    unsafe { (self.uf.containing_occurrence)(body) }
                } else {
                    0
                },
            };
            assembly_motion::binding::validate_binding(&expected, &observed)?;
            out.push(BoundMotionBody {
                placement,
                expected,
                observed,
            });
        }
        validate_unique_bindings(
            &out.iter()
                .map(|b| (b.expected.clone(), b.observed.clone()))
                .collect::<Vec<_>>(),
        )?;
        Ok(out)
    }
}

impl Native {
    /// Owned current-session root only. Each native dispatch is preceded by a
    /// durable event callback; no external tag or unchecked pose is accepted.
    pub fn move_position_sample(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        sample: &oe_contracts::NxAssemblyMotionSample,
        record: impl FnMut(Value) -> Result<()>,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        self.move_position_sample_with_capture(part, namespace, budget, sample, false, record)
    }
    pub fn move_position_sample_capture2(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        sample: &oe_contracts::NxAssemblyMotionSample,
        record: impl FnMut(Value) -> Result<()>,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        self.move_position_sample_with_capture(part, namespace, budget, sample, true, record)
    }
    fn move_position_sample_with_capture(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        sample: &oe_contracts::NxAssemblyMotionSample,
        capture_current: bool,
        mut record: impl FnMut(Value) -> Result<()>,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        let before =
            self.observe_assembly_positions(part, namespace, budget, "reference_observation")?;
        let targets = assembly_motion::resolve_position_targets(&before, budget, sample)?;
        let library = self._libraries.last().context("native_shim_missing")?;
        let reposition: unsafe extern "C" fn(Tag, *const f64, *mut i32) -> i32 =
            unsafe { std::mem::transmute(library.symbol(b"oe_nx_reposition_occurrence_v1\0")?) };
        record(
            json!({"phase":"motion_sample_before","sample_id":sample.sample_id,"observation":before}),
        )?;
        for target in targets {
            // Earlier parent movement changes this child's current root pose.
            let current =
                self.observe_assembly_positions(part, namespace, budget, "reference_observation")?;
            let node = current
                .occurrences
                .iter()
                .find(|o| o.occurrence_id == target.occurrence_id)
                .context("native_motion_target_missing")?;
            let original = before
                .occurrences
                .iter()
                .find(|o| o.occurrence_id == target.occurrence_id)
                .unwrap();
            ensure!(
                node.part_occurrence_tag == original.part_occurrence_tag
                    && node.instance_tag == original.instance_tag
                    && node.prototype_id == original.prototype_id
                    && current.root_occurrence_tag == before.root_occurrence_tag,
                "native_motion_target_identity_changed"
            );
            let units = &current
                .prototypes
                .iter()
                .find(|p| p.prototype_id == current.root_prototype_id)
                .unwrap()
                .native_units;
            let delta = assembly_motion::delta_for_target(
                &node.local_to_root_m,
                &target.desired_root_m.0,
                units,
            )?;
            let mut pending = json!({"phase":"motion_reposition_pending","sample_id":sample.sample_id,"occurrence_id":node.occurrence_id,
                "part_occurrence_tag":node.part_occurrence_tag,"current_root_m":node.local_to_root_m,
                "desired_root_m":target.desired_root_m.0,"delta_native":delta,"root_units":units,"level":"UF_ASSEM_establish_override"});
            if capture_current {
                pending["current_observation"] = serde_json::to_value(&current)?;
            }
            record(pending)?;
            let mut native_code = -1;
            let wrapper = unsafe {
                reposition(
                    Tag::try_from(node.part_occurrence_tag)?,
                    delta.as_ptr().cast(),
                    &mut native_code,
                )
            };
            let update = unsafe { (self.uf.motion_update)() };
            record(
                json!({"phase":"motion_reposition_returned","sample_id":sample.sample_id,"occurrence_id":node.occurrence_id,
                "wrapper_code":wrapper,"native_code":native_code,"update_code":update}),
            )?;
            self.check(wrapper, "native_motion_reposition_wrapper")?;
            self.check(native_code, "UF_ASSEM_reposition_part_occurrence")?;
            self.check(update, "native_motion_reposition_update")?;
        }
        let after =
            self.observe_assembly_positions(part, namespace, budget, "reference_observation")?;
        record(
            json!({"phase":"motion_sample_after","sample_id":sample.sample_id,"observation":after}),
        )?;
        assembly_motion::check_position_sample(&before, &after, budget, sample)?;
        Ok(after)
    }
}
/// Private adapter only. Callers obtain both handles from current native body
/// inventory and recheck identities around cleanup; no JSON dispatch exposes it.
struct NativeInterference<'a>(&'a mut Native);
impl assembly_motion::execution::InterferenceApi for NativeInterference<'_> {
    fn query(&mut self, a: u32, b: u32) -> (i32, i32) {
        let mut other = b;
        let mut result = -1;
        let code = unsafe { (self.0.uf.interference)(a, 1, &mut other, &mut result) };
        (code, result)
    }
    fn cleanup_update(&mut self) -> i32 {
        unsafe { (self.0.uf.motion_update)() }
    }
}

impl Native {
    fn verify_motion_binding(&self, body: &BoundMotionBody) -> Result<()> {
        let tag = body.observed.body;
        let owner = self.motion_solid_owner(tag)?;
        let logical = unsafe { (self.uf.is_occurrence)(tag) };
        ensure!(logical <= 1, "motion_binding_logical_invalid");
        let current = QueriedBody {
            body: tag,
            owner,
            is_solid_body: true,
            is_occurrence: logical == 1,
            prototype_body: if logical == 1 {
                unsafe { (self.uf.prototype)(tag) }
            } else {
                tag
            },
            containing_occurrence: if logical == 1 {
                unsafe { (self.uf.containing_occurrence)(tag) }
            } else {
                0
            },
        };
        assembly_motion::binding::validate_binding(&body.expected, &current)
    }
    pub fn check_position_pairs(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        motion_budget: &NxAssemblyMotionBudget,
        total_samples: u64,
        mut record: impl FnMut(Value) -> Result<()>,
    ) -> Result<Value> {
        let before =
            self.observe_assembly_positions(part, namespace, budget, "reference_observation")?;
        let plan =
            assembly_motion::plan_position_pairs(&before, budget, motion_budget, total_samples)?;
        let expected_count = plan.pairs_per_sample;
        let view = crate::assembly_position::checked_inventory(&before, budget)?;
        let bodies = self.bind_motion_bodies(part, &view, plan.bodies)?;
        record(
            json!({"phase":"motion_pairs_before","observation":before,"placed_bodies":bodies.len(),"expected_pairs":expected_count}),
        )?;
        let (mut checked, mut interference, mut touching) = (0u64, 0u64, 0u64);
        for i in 0..bodies.len() {
            for j in i + 1..bodies.len() {
                let a = &bodies[i];
                let b = &bodies[j];
                self.verify_motion_binding(a)?;
                self.verify_motion_binding(b)?;
                let result = assembly_motion::execution::check_pair(
                    &mut NativeInterference(self),
                    a.observed.body,
                    b.observed.body,
                    |event| {
                        use assembly_motion::execution::PairEvent;
                        let identity = json!({"pair_index":checked,"a":{"occurrence_id":a.placement.occurrence_id,"prototype_id":a.placement.prototype_id,"body_tag":a.observed.body},
                        "b":{"occurrence_id":b.placement.occurrence_id,"prototype_id":b.placement.prototype_id,"body_tag":b.observed.body}});
                        record(match event {
                            PairEvent::Pending { .. } => {
                                json!({"phase":"interference_pending","identity":identity})
                            }
                            PairEvent::Returned(r) => {
                                json!({"phase":"interference_returned","identity":identity,"query_code":r.query_code,"result_raw":r.result_raw,"cleanup_code":r.cleanup_code})
                            }
                        })
                    },
                )?;
                self.verify_motion_binding(a)?;
                self.verify_motion_binding(b)?;
                checked += 1;
                match result.1 {
                    assembly_motion::InterferenceStatus::Interference => interference += 1,
                    assembly_motion::InterferenceStatus::Touching => touching += 1,
                    _ => {}
                }
            }
        }
        ensure!(
            checked == expected_count,
            "native_motion_pair_coverage_incomplete"
        );
        let after =
            self.observe_assembly_positions(part, namespace, budget, "reference_observation")?;
        record(json!({"phase":"motion_pairs_after_cleanup","observation":after}))?;
        crate::assembly::same_geometry(
            &view,
            &crate::assembly_position::checked_inventory(&after, budget)?,
        )?;
        Ok(
            json!({"checked_pairs":checked,"interference_pairs":interference,"touching_pairs":touching,
            "no_interference_pairs":checked-interference-touching,"continuous_sweep_checked":false,"engineering_adoption":"not_assessed"}),
        )
    }
}
