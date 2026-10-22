//! Explicit /2 UF identity capture. Tags only originate in this live Native session.
use std::collections::BTreeMap;

use super::motion::BoundMotionBody;
use super::*;
use crate::assembly_motion::binding::{ExpectedBody, QueriedBody};
use crate::assembly_motion::{self as motion, PlacedBody};
use oe_contracts::{
    NxAssemblyBudget, NxAssemblyMotionBudget, NxAssemblyObservation, NxMotionBodyBindingFacts2,
    NxMotionNativeQueryFact2, NxMotionPairObservation2, NxMotionPlacedBodyIdentity2,
};

struct CapturedBody2 {
    body: BoundMotionBody,
    identity: NxMotionPlacedBodyIdentity2,
}
fn status(
    calls: &mut Vec<NxMotionNativeQueryFact2>,
    api: &str,
    arguments: &[Tag],
    code: i32,
    values: &[i64],
) {
    calls.push(NxMotionNativeQueryFact2 {
        api: api.into(),
        arguments: arguments.iter().map(|&v| u64::from(v)).collect(),
        attempted: true,
        captured: true,
        return_code: Some(i64::from(code)),
        raw_values: values.to_vec(),
    });
}
fn direct(calls: &mut Vec<NxMotionNativeQueryFact2>, api: &str, arguments: &[Tag], value: i64) {
    calls.push(NxMotionNativeQueryFact2 {
        api: api.into(),
        arguments: arguments.iter().map(|&v| u64::from(v)).collect(),
        attempted: true,
        captured: true,
        return_code: None,
        raw_values: vec![value],
    });
}
impl Native {
    fn solid_owner_capture2(
        &self,
        body: Tag,
        calls: &mut Vec<NxMotionNativeQueryFact2>,
    ) -> Result<Tag> {
        ensure!(body != 0, "native_motion_body_missing");
        let (mut kind, mut subtype, mut body_kind, mut owner) = (-1, -1, -1, 0);
        let code = unsafe { (self.uf.object_type)(body, &mut kind, &mut subtype) };
        status(
            calls,
            "UF_OBJ_ask_type_and_subtype",
            &[body],
            code,
            &[i64::from(kind), i64::from(subtype)],
        );
        self.check(code, "native_motion_body_object_type")?;
        ensure!(
            kind == self.abi.solid_type && subtype == 0,
            "native_motion_object_not_body"
        );
        let code = unsafe { (self.uf.body_type)(body, &mut body_kind) };
        status(
            calls,
            "UF_MODL_ask_body_type",
            &[body],
            code,
            &[i64::from(body_kind)],
        );
        self.check(code, "native_motion_body_kind")?;
        ensure!(
            body_kind == self.abi.solid_body,
            "native_motion_object_not_solid"
        );
        let code = unsafe { (self.uf.owning_part)(body, &mut owner) };
        status(
            calls,
            "UF_OBJ_ask_owning_part",
            &[body],
            code,
            &[i64::from(owner)],
        );
        self.check(code, "native_motion_body_owner")?;
        ensure!(owner != 0, "native_motion_body_owner_missing");
        Ok(owner)
    }
    fn queried_body_capture2(
        &self,
        body: Tag,
        calls: &mut Vec<NxMotionNativeQueryFact2>,
    ) -> Result<QueriedBody> {
        let owner = self.solid_owner_capture2(body, calls)?;
        let logical = unsafe { (self.uf.is_occurrence)(body) };
        direct(calls, "UF_ASSEM_is_occurrence", &[body], i64::from(logical));
        ensure!(logical <= 1, "motion_binding_logical_invalid");
        let (prototype_body, containing_occurrence) = if logical == 1 {
            let prototype = unsafe { (self.uf.prototype)(body) };
            direct(
                calls,
                "UF_ASSEM_ask_prototype_of_occ",
                &[body],
                i64::from(prototype),
            );
            let containing = unsafe { (self.uf.containing_occurrence)(body) };
            direct(
                calls,
                "UF_ASSEM_ask_part_occurrence",
                &[body],
                i64::from(containing),
            );
            (prototype, containing)
        } else {
            // Algorithm-local root meaning; no invented UF query facts.
            (body, 0)
        };
        Ok(QueriedBody {
            body,
            owner,
            is_solid_body: true,
            is_occurrence: logical == 1,
            prototype_body,
            containing_occurrence,
        })
    }
    fn bind_motion_body_capture2(
        &self,
        root: Tag,
        observation: &NxAssemblyObservation,
        placement: PlacedBody,
        paths: &BTreeMap<String, Vec<String>>,
        record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<CapturedBody2> {
        let prototype = observation
            .prototypes
            .iter()
            .find(|p| p.prototype_id == placement.prototype_id)
            .context("native_motion_prototype_missing")?;
        let proto_body = Tag::try_from(placement.prototype_body_tag)?;
        let proto_owner = Tag::try_from(prototype.native_part_tag)?;
        let part_occurrence = placement
            .occurrence_id
            .as_ref()
            .map(|id| {
                let o = observation
                    .occurrences
                    .iter()
                    .find(|o| &o.occurrence_id == id)
                    .context("native_motion_part_occurrence_missing")?;
                Ok::<Tag, anyhow::Error>(Tag::try_from(o.part_occurrence_tag)?)
            })
            .transpose()?;
        let expected = ExpectedBody {
            prototype_body: proto_body,
            prototype_owner: proto_owner,
            part_occurrence,
        };
        let mut facts = NxMotionBodyBindingFacts2 {
            stage: "acquired".into(),
            calls: Vec::new(),
            placement: NxMotionPlacedBodyIdentity2 {
                occurrence_path: placement
                    .occurrence_id
                    .as_ref()
                    .map(|id| {
                        paths
                            .get(id)
                            .cloned()
                            .context("native_motion_component_path_missing")
                    })
                    .transpose()?,
                occurrence_id: placement.occurrence_id.clone(),
                prototype_id: placement.prototype_id.clone(),
                prototype_body_tag: placement.prototype_body_tag,
                prototype_owner_tag: u64::from(proto_owner),
                part_occurrence_tag: part_occurrence.map(u64::from),
                body_tag: 0,
            },
        };
        let observed = (|| -> Result<QueriedBody> {
            let owner = self.solid_owner_capture2(proto_body, &mut facts.calls)?;
            // Preserve original short-circuiting: no logical query after a wrong owner.
            ensure!(owner == proto_owner, "native_motion_prototype_body_differs");
            let logical = unsafe { (self.uf.is_occurrence)(proto_body) };
            direct(
                &mut facts.calls,
                "UF_ASSEM_is_occurrence",
                &[proto_body],
                i64::from(logical),
            );
            ensure!(logical == 0, "native_motion_prototype_body_differs");
            let body = if let Some(occ) = part_occurrence {
                let logical = unsafe { (self.uf.is_occurrence)(occ) };
                direct(
                    &mut facts.calls,
                    "UF_ASSEM_is_occurrence",
                    &[occ],
                    i64::from(logical),
                );
                ensure!(
                    logical == 1,
                    "native_motion_part_occurrence_prototype_differs"
                );
                let actual = unsafe { (self.uf.prototype)(occ) };
                direct(
                    &mut facts.calls,
                    "UF_ASSEM_ask_prototype_of_occ",
                    &[occ],
                    i64::from(actual),
                );
                ensure!(
                    actual == proto_owner,
                    "native_motion_part_occurrence_prototype_differs"
                );
                let body = unsafe { (self.uf.find_occurrence)(occ, proto_body) };
                direct(
                    &mut facts.calls,
                    "UF_ASSEM_find_occurrence",
                    &[occ, proto_body],
                    i64::from(body),
                );
                body
            } else {
                ensure!(proto_owner == root, "native_motion_root_owner_differs");
                proto_body
            };
            facts.placement.body_tag = u64::from(body);
            let observed = self.queried_body_capture2(body, &mut facts.calls)?;
            motion::binding::validate_binding(&expected, &observed)?;
            Ok(observed)
        })();
        // Retain all safely acquired raw facts even when identity acceptance fails.
        record(json!({"phase":"motion_body_binding", "binding":&facts}))?;
        Ok(CapturedBody2 {
            body: BoundMotionBody {
                placement,
                expected,
                observed: observed?,
            },
            identity: facts.placement,
        })
    }
    fn verify_motion_body_capture2(
        &self,
        body: &CapturedBody2,
        stage: &str,
    ) -> (NxMotionBodyBindingFacts2, Result<()>) {
        let mut facts = NxMotionBodyBindingFacts2 {
            stage: stage.into(),
            placement: body.identity.clone(),
            calls: Vec::new(),
        };
        let result = self
            .queried_body_capture2(body.body.observed.body, &mut facts.calls)
            .and_then(|actual| motion::binding::validate_binding(&body.body.expected, &actual));
        (facts, result)
    }
}
struct Interference2<'a>(&'a mut Native);
impl motion::execution::InterferenceApi for Interference2<'_> {
    fn query(&mut self, a: u32, b: u32) -> (i32, i32) {
        let (mut other, mut result) = (b, -1);
        let code = unsafe { (self.0.uf.interference)(a, 1, &mut other, &mut result) };
        (code, result)
    }
    fn cleanup_update(&mut self) -> i32 {
        unsafe { (self.0.uf.motion_update)() }
    }
}
fn pair_event(phase: &str, pair: &NxMotionPairObservation2) -> Value {
    json!({"phase":phase,"pair":pair})
}
impl Native {
    pub fn check_position_pairs_capture2(
        &mut self,
        part: Tag,
        namespace: &Path,
        budget: &NxAssemblyBudget,
        motion_budget: &NxAssemblyMotionBudget,
        total_samples: u64,
        sample_index: u64,
        sample_id: &str,
        mut record: impl FnMut(Value) -> Result<()>,
    ) -> Result<Value> {
        let before =
            self.observe_assembly_positions(part, namespace, budget, "reference_observation")?;
        let plan = motion::plan_position_pairs(&before, budget, motion_budget, total_samples)?;
        let expected_count = plan.pairs_per_sample;
        let view = crate::assembly_position::checked_inventory(&before, budget)?;
        let paths = crate::assembly_position::persistence::paths(&before)?;
        let mut bodies = Vec::with_capacity(plan.bodies.len());
        for placement in plan.bodies {
            bodies.push(self.bind_motion_body_capture2(
                part,
                &view,
                placement,
                &paths,
                &mut record,
            )?);
        }
        motion::binding::validate_unique_bindings(
            &bodies
                .iter()
                .map(|b| (b.body.expected.clone(), b.body.observed.clone()))
                .collect::<Vec<_>>(),
        )?;
        record(json!({"phase":"motion_pairs_before","observation":before,
            "placed_bodies":bodies.len(),"expected_pairs":expected_count}))?;
        let (mut checked, mut interference, mut touching) = (0u64, 0u64, 0u64);
        for i in 0..bodies.len() {
            for j in i + 1..bodies.len() {
                let (a, b) = (&bodies[i], &bodies[j]);
                let (fa, ra) = self.verify_motion_body_capture2(a, "before_query");
                let (fb, rb) = if ra.is_ok() {
                    self.verify_motion_body_capture2(b, "before_query")
                } else {
                    (
                        NxMotionBodyBindingFacts2 {
                            stage: "before_query".into(),
                            placement: b.identity.clone(),
                            calls: Vec::new(),
                        },
                        Ok(()),
                    )
                };
                let mut pair = NxMotionPairObservation2 {
                    schema: "oev2.nx-motion-pair-observation/2".into(),
                    stage: "pending".into(),
                    sample_index,
                    sample_id: sample_id.into(),
                    pair_index: checked,
                    a: a.identity.clone(),
                    b: b.identity.clone(),
                    binding_facts: vec![fa, fb],
                    query_code: None,
                    result_raw: None,
                    cleanup_code: None,
                };
                if ra.is_err() || rb.is_err() {
                    record(pair_event("interference_pending", &pair))?;
                    ra?;
                    rb?;
                }
                let result = motion::execution::check_pair(
                    &mut Interference2(self),
                    a.body.observed.body,
                    b.body.observed.body,
                    |event| match event {
                        motion::execution::PairEvent::Pending { .. } => {
                            record(pair_event("interference_pending", &pair))
                        }
                        motion::execution::PairEvent::Returned(raw) => {
                            pair.stage = "returned".into();
                            pair.binding_facts.clear();
                            pair.query_code = Some(i64::from(raw.query_code));
                            pair.result_raw = Some(i64::from(raw.result_raw));
                            pair.cleanup_code = Some(i64::from(raw.cleanup_code));
                            record(pair_event("interference_returned", &pair))
                        }
                    },
                )?;
                let (fa, ra) = self.verify_motion_body_capture2(a, "after_cleanup");
                let (fb, rb) = if ra.is_ok() {
                    self.verify_motion_body_capture2(b, "after_cleanup")
                } else {
                    (
                        NxMotionBodyBindingFacts2 {
                            stage: "after_cleanup".into(),
                            placement: b.identity.clone(),
                            calls: Vec::new(),
                        },
                        Ok(()),
                    )
                };
                pair.stage = "after_cleanup".into();
                pair.binding_facts = vec![fa, fb];
                pair.query_code = None;
                pair.result_raw = None;
                pair.cleanup_code = None;
                record(pair_event("interference_identity_after", &pair))?;
                ra?;
                rb?;
                checked += 1;
                match result.1 {
                    motion::InterferenceStatus::Interference => interference += 1,
                    motion::InterferenceStatus::Touching => touching += 1,
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
