//! Ordered replay; memory is bounded by one inventory, placement set and event.
//! Pair combinations are traversed by indices, never materialized quadratically.
use super::{Inputs2, Reception2, SaveState, decode, facts, fields, save};
use crate::{
    assembly::{self, Rigid},
    assembly_motion as motion, assembly_position as position,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Open,
    Capture,
    Initial,
    Start,
    Before,
    MovePending,
    MoveReturned,
    After,
    Acquire,
    PairsBefore,
    PairPending,
    PairReturned,
    PairAfter,
    PairsAfter,
    Checked,
    SaveStart,
    SaveReturned,
    Saved,
    Seal,
    Done,
    Stopped,
}

pub(super) struct State<'a> {
    input: Inputs2<'a>,
    step: Step,
    sample: usize,
    moves: usize,
    targets: Vec<motion::ResolvedTarget>,
    plan: Vec<motion::PlacedBody>,
    expected_pairs: u64,
    last: Option<NxAssemblyPositionObservation>,
    before: Option<NxAssemblyPositionObservation>,
    after: Option<NxAssemblyPositionObservation>,
    identities: Vec<NxMotionPlacedBodyIdentity2>,
    identity_verified: Vec<bool>,
    pending_pair_verified: bool,
    unknown_fact: bool,
    sample_locally_checked: u64,
    body_tags: BTreeSet<u64>,
    pair_i: usize,
    pair_j: usize,
    sample_pairs: u64,
    sample_interference: u64,
    sample_touching: u64,
    pending_result: Option<motion::InterferenceStatus>,
    pending_save: Option<NxMotionSaveObservation2>,
    result: Reception2,
    known_failure: bool,
}
fn u(v: &Value, key: &str) -> Result<u64> {
    v[key]
        .as_u64()
        .with_context(|| format!("motion2_u64:{key}"))
}
fn i(v: &Value, key: &str) -> Result<i32> {
    Ok(i32::try_from(
        v[key]
            .as_i64()
            .with_context(|| format!("motion2_i32:{key}"))?,
    )?)
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("motion2_string:{key}"))
}
fn matrix(v: &Value) -> Result<Vec<f64>> {
    Ok(serde_json::from_value(v.clone())?)
}

/// Same-process identity is stronger than persistent reopen comparison, where
/// native handles and enumeration may legitimately differ.
fn same_identity(
    a: &NxAssemblyPositionObservation,
    b: &NxAssemblyPositionObservation,
) -> Result<()> {
    ensure!(
        a.root_occurrence_tag == b.root_occurrence_tag
            && a.root_prototype_id == b.root_prototype_id
            && a.prototypes.len() == b.prototypes.len()
            && a.occurrences.len() == b.occurrences.len(),
        "motion2_inventory_identity_count"
    );
    for p in &a.prototypes {
        let q = b
            .prototypes
            .iter()
            .find(|q| q.prototype_id == p.prototype_id)
            .context("motion2_prototype_removed")?;
        ensure!(
            p.native_part_tag == q.native_part_tag
                && p.part_path == q.part_path
                && p.solids.iter().map(|x| x.body_tag).collect::<BTreeSet<_>>()
                    == q.solids.iter().map(|x| x.body_tag).collect::<BTreeSet<_>>(),
            "motion2_prototype_native_identity_changed"
        );
    }
    for p in &a.occurrences {
        let q = b
            .occurrences
            .iter()
            .find(|q| q.occurrence_id == p.occurrence_id)
            .context("motion2_occurrence_removed")?;
        ensure!(
            p.part_occurrence_tag == q.part_occurrence_tag
                && p.instance_tag == q.instance_tag
                && p.prototype_id == q.prototype_id
                && p.name == q.name
                && p.parent_occurrence_id == q.parent_occurrence_id,
            "motion2_occurrence_native_identity_changed"
        );
    }
    Ok(())
}
fn unchanged(
    a: &NxAssemblyPositionObservation,
    b: &NxAssemblyPositionObservation,
    budget: &NxAssemblyBudget,
) -> Result<()> {
    same_identity(a, b)?;
    position::persistence::compare(a, b, budget)
}

impl<'a> State<'a> {
    pub fn new(input: Inputs2<'a>) -> Self {
        Self {
            input,
            step: Step::Open,
            sample: 0,
            moves: 0,
            targets: vec![],
            plan: vec![],
            expected_pairs: 0,
            last: None,
            before: None,
            after: None,
            identities: vec![],
            identity_verified: vec![],
            pending_pair_verified: false,
            unknown_fact: false,
            sample_locally_checked: 0,
            body_tags: BTreeSet::new(),
            pair_i: 0,
            pair_j: 1,
            sample_pairs: 0,
            sample_interference: 0,
            sample_touching: 0,
            pending_result: None,
            pending_save: None,
            result: Reception2 {
                outcome: "unknown",
                stream_state: "unknown",
                motion_state: "not_started",
                pair_identity_state: "not_started",
                query_state: "not_started",
                samples: 0,
                pairs: 0,
                interference: 0,
                no_interference: 0,
                touching: 0,
                save_state: SaveState::NotStarted,
                saved_position_checked: false,
                issues: vec![],
            },
            known_failure: false,
        }
    }
    fn expect(&self, step: Step, phase: &str) -> Result<()> {
        ensure!(
            self.step == step,
            "motion2_phase_out_of_order:{phase}:{:?}",
            self.step
        );
        Ok(())
    }
    fn sample(&self) -> Result<&NxAssemblyMotionSample> {
        self.input
            .intent
            .samples
            .get(self.sample)
            .context("motion2_sample_out_of_range")
    }
    fn observation(&self, value: &Value) -> Result<NxAssemblyPositionObservation> {
        let o: NxAssemblyPositionObservation = serde_json::from_value(value.clone())?;
        position::validate_position_observation(&o, &self.input.params.budget)?;
        ensure!(
            o.root_prototype_id == self.input.intent.root_part,
            "motion2_position_root_changed"
        );
        for p in &o.prototypes {
            ensure!(
                self.input.owned_paths.get(&p.part_path) == Some(&p.prototype_id),
                "motion2_position_unowned_prototype_path"
            );
        }
        Ok(o)
    }
    fn sample_header(&self, value: &Value, time: bool) -> Result<()> {
        let sample = self.sample()?;
        ensure!(
            u(value, "sample_index")? == self.sample as u64
                && text(value, "sample_id")? == sample.sample_id,
            "motion2_sample_identity"
        );
        if time {
            ensure!(
                value["time_s"].as_f64() == Some(sample.time_s),
                "motion2_sample_time"
            );
        }
        Ok(())
    }
    fn native<'b>(&self, event: &'b NxMotionEvidenceEvent) -> Result<&'b Value> {
        self.sample_header(&event.observation, true)?;
        let v = &event.observation["event"];
        ensure!(
            text(v, "phase")? == event.phase,
            "motion2_native_phase_differs"
        );
        if let Some(id) = v.get("sample_id") {
            ensure!(
                id.as_str() == Some(self.sample()?.sample_id.as_str()),
                "motion2_native_sample_id"
            );
        }
        Ok(v)
    }
    fn stop(&mut self, known: bool, issue: &str) {
        self.known_failure |= known;
        self.result.issues.push(issue.to_owned());
        self.step = Step::Stopped;
    }
    fn fact(&mut self, f: &NxMotionBodyBindingFacts2, stage: &str, body: usize) -> Result<bool> {
        let expected = self.plan.get(body).context("motion2_binding_out_of_plan")?;
        let inventory = self
            .after
            .as_ref()
            .context("motion2_binding_no_inventory")?;
        match facts::validate_facts(f, stage, expected, inventory, self.input.abi)? {
            facts::FactOutcome::Valid(actual) => {
                ensure!(
                    actual.body as u64 == f.placement.body_tag,
                    "motion2_actual_body_differs"
                );
                Ok(true)
            }
            facts::FactOutcome::Unknown => {
                self.result.pair_identity_state = "unknown";
                self.unknown_fact = true;
                if !self
                    .result
                    .issues
                    .iter()
                    .any(|s| s == "motion2_required_uf_fact_not_observed")
                {
                    self.result
                        .issues
                        .push("motion2_required_uf_fact_not_observed".into());
                }
                Ok(false)
            }
            facts::FactOutcome::Failed => {
                self.result.pair_identity_state = "failed";
                self.stop(true, "motion2_identity_query_failed");
                Ok(false)
            }
        }
    }
    fn pair(&self, v: &Value, stage: &str) -> Result<NxMotionPairObservation2> {
        fields("NxMotionPairObservation2", v)?;
        fields("NxMotionPlacedBodyIdentity2", &v["a"])?;
        fields("NxMotionPlacedBodyIdentity2", &v["b"])?;
        for f in v["binding_facts"]
            .as_array()
            .context("motion2_pair_facts_array")?
        {
            facts::parse_facts(f)?;
        }
        let p: NxMotionPairObservation2 = decode("NxMotionPairObservation2", v)?;
        p.validate()
            .map_err(|_| anyhow::anyhow!("motion2_pair_generated"))?;
        ensure!(
            p.schema == "oev2.nx-motion-pair-observation/2"
                && p.stage == stage
                && p.sample_index == self.sample as u64
                && p.sample_id == self.sample()?.sample_id
                && p.pair_index == self.sample_pairs,
            "motion2_pair_scope_index"
        );
        ensure!(
            Some(&p.a) == self.identities.get(self.pair_i)
                && Some(&p.b) == self.identities.get(self.pair_j)
                && p.a.body_tag != p.b.body_tag,
            "motion2_pair_complete_identity"
        );
        for raw in [p.query_code, p.result_raw, p.cleanup_code]
            .into_iter()
            .flatten()
        {
            i32::try_from(raw).context("motion2_pair_raw_range")?;
        }
        if stage == "returned" {
            ensure!(
                p.binding_facts.is_empty()
                    && p.query_code.is_some()
                    && p.result_raw.is_some()
                    && p.cleanup_code.is_some(),
                "motion2_returned_pair_fields"
            );
        } else {
            ensure!(
                p.query_code.is_none()
                    && p.result_raw.is_none()
                    && p.cleanup_code.is_none()
                    && p.binding_facts.len() <= 2,
                "motion2_pair_identity_phase_fields"
            );
        }
        Ok(p)
    }
    fn pair_facts(&mut self, p: &NxMotionPairObservation2, stage: &str) -> Result<bool> {
        let mut valid = true;
        for (at, body) in [self.pair_i, self.pair_j].into_iter().enumerate() {
            let Some(f) = p.binding_facts.get(at) else {
                self.result.pair_identity_state = "unknown";
                self.unknown_fact = true;
                valid = false;
                if !self
                    .result
                    .issues
                    .iter()
                    .any(|s| s == "motion2_pair_facts_missing")
                {
                    self.result.issues.push("motion2_pair_facts_missing".into());
                }
                continue;
            };
            ensure!(
                f.placement
                    == *self
                        .identities
                        .get(body)
                        .context("motion2_pair_body_missing")?,
                "motion2_pair_fact_identity_differs"
            );
            valid &= self.fact(f, stage, body)?;
            if self.step == Step::Stopped {
                return Ok(false);
            }
        }
        Ok(valid)
    }
    fn save_paths(&self, s: &NxMotionSaveObservation2) -> Result<()> {
        let logical = s
            .loaded_owned_paths
            .iter()
            .map(|p| {
                self.input
                    .owned_paths
                    .get(p)
                    .cloned()
                    .context("motion2_save_unowned_path")
            })
            .collect::<Result<Vec<_>>>()?;
        if s.loaded_paths_complete {
            crate::geometry::motion_task::check_loaded(&logical, self.input.closure)?;
        }
        Ok(())
    }
    pub fn observe(&mut self, event: &NxMotionEvidenceEvent) -> Result<()> {
        let phase = event.phase.as_str();
        let v = &event.observation;
        match phase {
            "stream_open" => {
                self.expect(Step::Open, phase)?;
                self.step = Step::Capture;
            }
            "motion_capture" => {
                self.expect(Step::Capture, phase)?;
                let c: NxMotionCaptureBinding2 = decode("NxMotionCaptureBinding2", v)?;
                ensure!(
                    c.schema == "oev2.nx-motion-capture/2"
                        && c.parameters == *self.input.parameters
                        && c.binding == *self.input.binding,
                    "motion2_capture_version_or_binding"
                );
                self.step = Step::Initial;
            }
            "motion_initial" => {
                self.expect(Step::Initial, phase)?;
                let initial = self.observation(&v["observation"])?;
                let loaded: Vec<String> = serde_json::from_value(v["loaded_parts"].clone())?;
                crate::geometry::motion_task::check_loaded(&loaded, self.input.closure)?;
                let plan = motion::plan_position_pairs(
                    &initial,
                    &self.input.params.budget,
                    &self.input.intent.budget,
                    self.input.binding.samples,
                )?;
                ensure!(
                    u(v, "samples")? == self.input.binding.samples
                        && u(v, "pairs_per_sample")? == plan.pairs_per_sample,
                    "motion2_initial_plan_count"
                );
                let mut minimum = 7u64; // /2 open, capture, initial, save pending/return, saved position, seal
                for sample in &self.input.intent.samples {
                    let targets = motion::resolve_position_targets(
                        &initial,
                        &self.input.params.budget,
                        sample,
                    )?;
                    minimum = minimum
                        .checked_add(
                            6 + 2 * targets.len() as u64
                                + plan.bodies.len() as u64
                                + 3 * plan.pairs_per_sample,
                        )
                        .context("motion2_event_count_overflow")?;
                }
                ensure!(
                    u(v, "minimum_events")? == minimum
                        && minimum <= self.input.params.evidence.max_events,
                    "motion2_initial_event_budget"
                );
                self.plan = plan.bodies;
                self.expected_pairs = plan.pairs_per_sample;
                self.last = Some(initial);
                self.step = Step::Start;
            }
            "motion_sample_start" => {
                self.expect(Step::Start, phase)?;
                self.sample_header(v, true)?;
                self.targets = motion::resolve_position_targets(
                    self.last.as_ref().context("motion2_initial_missing")?,
                    &self.input.params.budget,
                    self.sample()?,
                )?;
                self.moves = 0;
                self.identities.clear();
                self.identity_verified.clear();
                self.sample_locally_checked = 0;
                self.body_tags.clear();
                self.pair_i = 0;
                self.pair_j = 1;
                self.sample_pairs = 0;
                self.sample_interference = 0;
                self.sample_touching = 0;
                self.pending_result = None;
                self.result.motion_state = "partial";
                self.step = Step::Before;
            }
            "motion_sample_before" => {
                self.expect(Step::Before, phase)?;
                let n = self.native(event)?;
                let before = self.observation(&n["observation"])?;
                unchanged(
                    self.last.as_ref().context("motion2_last_missing")?,
                    &before,
                    &self.input.params.budget,
                )?;
                self.before = Some(before);
                self.step = Step::MovePending;
            }
            "motion_reposition_pending" => {
                self.expect(Step::MovePending, phase)?;
                let n = self.native(event)?;
                let current = self.observation(&n["current_observation"])?;
                let before = self.before.as_ref().context("motion2_before_missing")?;
                same_identity(before, &current)?;
                if self.moves == 0 {
                    unchanged(before, &current, &self.input.params.budget)?;
                } else {
                    let mut partial = self.sample()?.clone();
                    let paths = position::persistence::paths(before)?;
                    let done = self.targets[..self.moves]
                        .iter()
                        .map(|t| paths[&t.occurrence_id].clone())
                        .collect::<BTreeSet<_>>();
                    partial
                        .targets
                        .retain(|t| done.contains(&t.occurrence_path));
                    motion::check_position_sample(
                        before,
                        &current,
                        &self.input.params.budget,
                        &partial,
                    )?;
                }
                let target = self.targets.get(self.moves).context("motion2_extra_move")?;
                let node = current
                    .occurrences
                    .iter()
                    .find(|o| o.occurrence_id == target.occurrence_id)
                    .context("motion2_move_node_missing")?;
                let units = &current
                    .prototypes
                    .iter()
                    .find(|p| p.prototype_id == current.root_prototype_id)
                    .context("motion2_root_missing")?
                    .native_units;
                ensure!(
                    text(n, "occurrence_id")? == target.occurrence_id
                        && u(n, "part_occurrence_tag")? == node.part_occurrence_tag
                        && text(n, "level")? == "UF_ASSEM_establish_override"
                        && text(n, "root_units")? == units,
                    "motion2_move_identity_or_order"
                );
                ensure!(
                    Rigid::checked(&matrix(&n["current_root_m"])?)?
                        .near(Rigid::checked(&node.local_to_root_m)?)
                        && Rigid::checked(&matrix(&n["desired_root_m"])?)?
                            .near(target.desired_root_m),
                    "motion2_move_stale_or_wrong_pose"
                );
                let expected = motion::delta_for_target(
                    &node.local_to_root_m,
                    &target.desired_root_m.0,
                    units,
                )?;
                let observed: [[f64; 4]; 4] = serde_json::from_value(n["delta_native"].clone())?;
                for r in 0..4 {
                    for c in 0..4 {
                        let tol = if c == 3 && r < 3 {
                            assembly::POSITION_TOLERANCE_M / assembly::unit_to_m(units)?
                        } else {
                            assembly::ROTATION_TOLERANCE
                        };
                        ensure!(
                            observed[r][c].is_finite()
                                && (observed[r][c] - expected[r][c]).abs() <= tol,
                            "motion2_native_delta_differs"
                        );
                    }
                }
                self.step = Step::MoveReturned;
            }
            "motion_reposition_returned" => {
                self.expect(Step::MoveReturned, phase)?;
                let n = self.native(event)?;
                ensure!(
                    text(n, "occurrence_id")? == self.targets[self.moves].occurrence_id,
                    "motion2_returned_move_identity"
                );
                let codes = [
                    i(n, "wrapper_code")?,
                    i(n, "native_code")?,
                    i(n, "update_code")?,
                ];
                if codes.iter().any(|c| *c != 0) {
                    self.result.motion_state = "failed";
                    self.stop(true, "motion2_native_move_failed");
                } else {
                    self.moves += 1;
                    self.step = if self.moves == self.targets.len() {
                        Step::After
                    } else {
                        Step::MovePending
                    };
                }
            }
            "motion_sample_after" => {
                self.expect(Step::After, phase)?;
                let n = self.native(event)?;
                let after = self.observation(&n["observation"])?;
                let before = self.before.as_ref().context("motion2_before_missing")?;
                same_identity(before, &after)?;
                motion::check_position_sample(
                    before,
                    &after,
                    &self.input.params.budget,
                    self.sample()?,
                )?;
                let plan = motion::plan_position_pairs(
                    &after,
                    &self.input.params.budget,
                    &self.input.intent.budget,
                    self.input.binding.samples,
                )?;
                ensure!(
                    plan.bodies == self.plan && plan.pairs_per_sample == self.expected_pairs,
                    "motion2_placed_body_inventory_changed"
                );
                self.after = Some(after);
                self.step = Step::Acquire;
            }
            "motion_body_binding" => {
                self.expect(Step::Acquire, phase)?;
                let n = self.native(event)?;
                let f = facts::parse_facts(&n["binding"])?;
                let valid = self.fact(&f, "acquired", self.identities.len())?;
                if self.step != Step::Stopped {
                    // Unknown raw capture remains unknown even if the declared
                    // placement is carried forward to verify later stream shape.
                    ensure!(
                        self.body_tags.insert(f.placement.body_tag),
                        "motion2_duplicate_native_body"
                    );
                    self.identities.push(f.placement);
                    self.identity_verified.push(valid);
                    if self.identities.len() == self.plan.len() {
                        self.step = Step::PairsBefore;
                    }
                }
            }

            "motion_pairs_before" => {
                self.expect(Step::PairsBefore, phase)?;
                let n = self.native(event)?;
                let current = self.observation(&n["observation"])?;
                unchanged(
                    self.after.as_ref().context("motion2_after_missing")?,
                    &current,
                    &self.input.params.budget,
                )?;
                ensure!(
                    u(n, "placed_bodies")? == self.plan.len() as u64
                        && u(n, "expected_pairs")? == self.expected_pairs,
                    "motion2_pair_plan_count"
                );
                if !self.unknown_fact {
                    self.result.pair_identity_state = "partial";
                }
                self.result.query_state = "partial";
                self.step = if self.expected_pairs == 0 {
                    Step::PairsAfter
                } else {
                    Step::PairPending
                };
            }
            "interference_pending" => {
                self.expect(Step::PairPending, phase)?;
                let n = self.native(event)?;
                let p = self.pair(&n["pair"], "pending")?;
                let valid = self.pair_facts(&p, "before_query")?;
                if self.step != Step::Stopped {
                    self.pending_pair_verified = valid
                        && self.identity_verified[self.pair_i]
                        && self.identity_verified[self.pair_j];
                    self.step = Step::PairReturned;
                }
            }
            "interference_returned" => {
                self.expect(Step::PairReturned, phase)?;
                let n = self.native(event)?;
                let p = self.pair(&n["pair"], "returned")?;
                let query = i32::try_from(p.query_code.context("motion2_query_missing")?)?;
                let raw = i32::try_from(p.result_raw.context("motion2_raw_missing")?)?;
                let cleanup = i32::try_from(p.cleanup_code.context("motion2_cleanup_missing")?)?;
                if query != 0 || cleanup != 0 {
                    self.result.query_state = "failed";
                    self.stop(true, "motion2_interference_or_cleanup_failed");
                } else {
                    match motion::interference_status(query, raw) {
                        Ok(status) => {
                            self.pending_result = Some(status);
                            self.step = Step::PairAfter;
                        }
                        Err(_) => {
                            self.result.query_state = "unknown_result";
                            self.stop(true, "motion2_interference_result_unrecognized");
                        }
                    }
                }
            }
            "interference_identity_after" => {
                self.expect(Step::PairAfter, phase)?;
                let n = self.native(event)?;
                let p = self.pair(&n["pair"], "after_cleanup")?;
                let valid = self.pair_facts(&p, "after_cleanup")?;
                if self.step != Step::Stopped {
                    let qualified = valid && self.pending_pair_verified;
                    let status = self
                        .pending_result
                        .take()
                        .context("motion2_pair_result_missing")?;
                    self.sample_pairs += 1;
                    if qualified {
                        self.sample_locally_checked += 1;
                        self.result.pairs += 1;
                    }
                    match status {
                        motion::InterferenceStatus::Interference => {
                            self.sample_interference += 1;
                            if qualified {
                                self.result.interference += 1;
                            }
                        }
                        motion::InterferenceStatus::Touching => {
                            self.sample_touching += 1;
                            if qualified {
                                self.result.touching += 1;
                            }
                        }
                        motion::InterferenceStatus::NoInterference => {
                            if qualified {
                                self.result.no_interference += 1;
                            }
                        }
                    }
                    self.pair_j += 1;
                    if self.pair_j == self.plan.len() {
                        self.pair_i += 1;
                        self.pair_j = self.pair_i + 1;
                    }
                    self.step = if self.sample_pairs == self.expected_pairs {
                        Step::PairsAfter
                    } else {
                        Step::PairPending
                    };
                }
            }

            "motion_pairs_after_cleanup" => {
                self.expect(Step::PairsAfter, phase)?;
                let n = self.native(event)?;
                let current = self.observation(&n["observation"])?;
                unchanged(
                    self.after.as_ref().context("motion2_after_missing")?,
                    &current,
                    &self.input.params.budget,
                )?;
                self.step = Step::Checked;
            }
            "motion_sample_checked" => {
                self.expect(Step::Checked, phase)?;
                self.sample_header(v, false)?;
                let p = &v["pairs"];
                ensure!(
                    u(p, "checked_pairs")? == self.sample_pairs
                        && self.sample_pairs == self.expected_pairs
                        && u(p, "interference_pairs")? == self.sample_interference
                        && u(p, "touching_pairs")? == self.sample_touching
                        && u(p, "no_interference_pairs")?
                            == self.sample_pairs - self.sample_interference - self.sample_touching
                        && p["continuous_sweep_checked"].as_bool() == Some(false)
                        && text(p, "engineering_adoption")? == "not_assessed",
                    "motion2_sample_summary_differs"
                );
                if self.sample_locally_checked == self.expected_pairs
                    && self.identity_verified.iter().all(|v| *v)
                {
                    self.result.samples += 1;
                }
                self.sample += 1;
                self.last = self.after.take();
                self.before = None;
                if self.sample == self.input.intent.samples.len() {
                    self.result.motion_state = "checked_discrete";
                    self.result.pair_identity_state = if self.unknown_fact {
                        "unknown"
                    } else {
                        "checked_complete"
                    };
                    self.result.query_state = if self.unknown_fact {
                        "checked_raw_identity_incomplete"
                    } else {
                        "checked_complete"
                    };
                    self.step = Step::SaveStart;
                } else {
                    self.step = Step::Start;
                }
            }
            "motion_save_pending" | "motion_save_preflight_refused" => {
                self.expect(Step::SaveStart, phase)?;
                let s = save::parse_save(v)?;
                self.save_paths(&s)?;
                let state = save::validate_save(&s)?;
                self.result.save_state = state;
                if phase == "motion_save_preflight_refused" {
                    ensure!(s.phase == "preflight_refused", "motion2_save_refusal_phase");
                    self.stop(true, "motion2_save_preflight_refused");
                } else {
                    ensure!(
                        state == SaveState::PendingUnknown,
                        "motion2_save_pending_phase"
                    );
                    self.pending_save = Some(s);
                    self.step = Step::SaveReturned;
                }
            }
            "motion_save_returned" => {
                self.expect(Step::SaveReturned, phase)?;
                let s = save::parse_save(v)?;
                self.save_paths(&s)?;
                let pending = self
                    .pending_save
                    .as_ref()
                    .context("motion2_save_no_pending")?;
                ensure!(
                    s.phase == "call_returned"
                        && s.loaded_part_count_raw == pending.loaded_part_count_raw
                        && s.loaded_owned_paths == pending.loaded_owned_paths
                        && s.loaded_paths_complete == pending.loaded_paths_complete,
                    "motion2_save_inventory_changed"
                );
                let state = save::validate_save(&s)?;
                self.result.save_state = state;
                if state == SaveState::CallSucceeded {
                    self.step = Step::Saved;
                } else {
                    self.stop(true, "motion2_save_failed_or_capture_incomplete");
                }
            }
            "motion_saved_position" => {
                self.expect(Step::Saved, phase)?;
                let current = self.observation(v)?;
                unchanged(
                    self.last.as_ref().context("motion2_last_missing")?,
                    &current,
                    &self.input.params.budget,
                )?;
                if let Some(output) = self.input.saved_position {
                    ensure!(current == *output, "motion2_saved_position_output_differs");
                    self.result.saved_position_checked = true;
                } else {
                    ensure!(
                        !self.input.completed,
                        "motion2_saved_position_output_missing"
                    );
                }
                self.step = Step::Seal;
            }
            "stream_seal" => {
                self.expect(Step::Seal, phase)?;
                self.step = Step::Done;
            }
            _ => anyhow::bail!("motion2_unexpected_phase:{phase}"),
        }
        Ok(())
    }
    pub fn finish(mut self, sealed: bool, truncated: bool) -> Result<Reception2> {
        self.result.stream_state = if sealed {
            "verified_sealed"
        } else if truncated {
            "verified_prefix_partial_line"
        } else {
            "verified_prefix"
        };
        if self.input.completed {
            ensure!(
                sealed && self.step == Step::Done && self.input.saved_position.is_some(),
                "motion2_completed_incomplete_or_failed_coverage"
            );
            self.result.outcome = if self.unknown_fact {
                "unknown"
            } else {
                "accepted_bounded"
            };
        } else {
            self.result.outcome = if self.known_failure {
                "partial"
            } else {
                "unknown"
            };
            if self.step != Step::Stopped {
                self.result
                    .issues
                    .push("motion2_native_completion_not_established".into());
            }
        }
        Ok(self.result)
    }
}
