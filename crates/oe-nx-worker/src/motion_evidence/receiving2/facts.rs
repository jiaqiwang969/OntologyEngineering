//! Replay actual UF observations against the existing placement binding owner.
use super::{NativeBodyAbi, decode, fields};
use crate::assembly_motion::{
    PlacedBody,
    binding::{ExpectedBody, QueriedBody, validate_binding},
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    NxAssemblyPositionObservation, NxMotionBodyBindingFacts2, NxMotionNativeQueryFact2,
};
use serde_json::Value;

#[derive(Debug)]
pub enum FactOutcome {
    Valid(QueriedBody),
    Unknown,
    Failed,
}

pub fn parse_facts(value: &Value) -> Result<NxMotionBodyBindingFacts2> {
    fields("NxMotionBodyBindingFacts2", value)?;
    fields("NxMotionPlacedBodyIdentity2", &value["placement"])?;
    for call in value["calls"].as_array().context("motion2_call_array")? {
        fields("NxMotionNativeQueryFact2", call)?;
    }
    decode("NxMotionBodyBindingFacts2", value)
}

fn raw_valid(call: &NxMotionNativeQueryFact2) -> Result<()> {
    call.validate()
        .map_err(|_| anyhow::anyhow!("motion2_query_generated"))?;
    let (arity, outputs, status) = match call.api.as_str() {
        "UF_OBJ_ask_type_and_subtype" => (1, 2, true),
        "UF_MODL_ask_body_type" | "UF_OBJ_ask_owning_part" => (1, 1, true),
        "UF_ASSEM_is_occurrence"
        | "UF_ASSEM_ask_prototype_of_occ"
        | "UF_ASSEM_ask_part_occurrence" => (1, 1, false),
        "UF_ASSEM_find_occurrence" => (2, 1, false),
        _ => anyhow::bail!("motion2_query_api_unknown"),
    };
    ensure!(
        call.arguments.len() == arity,
        "motion2_query_argument_count"
    );
    for &tag in &call.arguments {
        ensure!(
            tag != 0 && tag <= u32::MAX as u64,
            "motion2_query_argument_tag"
        );
    }
    if !call.captured {
        ensure!(
            call.return_code.is_none() && call.raw_values.is_empty(),
            "motion2_uncaptured_has_values"
        );
        return Ok(());
    }
    ensure!(
        call.attempted && call.raw_values.len() == outputs,
        "motion2_capture_without_call_or_outputs"
    );
    if status {
        i32::try_from(call.return_code.context("motion2_status_missing")?)
            .context("motion2_status_range")?;
    } else {
        ensure!(
            call.return_code.is_none(),
            "motion2_direct_api_invented_status"
        );
    }
    for &raw in &call.raw_values {
        match call.api.as_str() {
            "UF_OBJ_ask_type_and_subtype" | "UF_MODL_ask_body_type" => {
                i32::try_from(raw).context("motion2_raw_i32")?;
            }
            "UF_ASSEM_is_occurrence" => {
                u8::try_from(raw).context("motion2_raw_logical_range")?;
            }
            _ => {
                u32::try_from(raw).context("motion2_raw_tag_range")?;
            }
        }
    }
    Ok(())
}

struct Calls<'a> {
    values: &'a [NxMotionNativeQueryFact2],
    at: usize,
    failed: bool,
}
impl Calls<'_> {
    fn take(&mut self, api: &str, args: &[u64]) -> Result<Option<Vec<i64>>> {
        let Some(call) = self.values.get(self.at) else {
            return Ok(None);
        };
        self.at += 1;
        ensure!(
            call.api == api && call.arguments == args,
            "motion2_actual_query_order_or_arguments"
        );
        if !call.attempted || !call.captured {
            return Ok(None);
        }
        if call.return_code.is_some_and(|code| code != 0) {
            self.failed = true;
            return Ok(None);
        }
        Ok(Some(call.raw_values.clone()))
    }
    fn unresolved(&self) -> FactOutcome {
        if self.failed {
            FactOutcome::Failed
        } else {
            FactOutcome::Unknown
        }
    }
    fn solid_owner(&mut self, body: u64, abi: NativeBodyAbi) -> Result<Option<u32>> {
        let Some(kind) = self.take("UF_OBJ_ask_type_and_subtype", &[body])? else {
            return Ok(None);
        };
        ensure!(
            kind == [abi.solid_type as i64, 0],
            "motion2_not_solid_object"
        );
        let Some(kind) = self.take("UF_MODL_ask_body_type", &[body])? else {
            return Ok(None);
        };
        ensure!(kind == [abi.solid_body as i64], "motion2_not_solid_body");
        let Some(owner) = self.take("UF_OBJ_ask_owning_part", &[body])? else {
            return Ok(None);
        };
        let owner = u32::try_from(owner[0])?;
        ensure!(owner != 0, "motion2_owner_missing");
        Ok(Some(owner))
    }
}

pub fn validate_facts(
    f: &NxMotionBodyBindingFacts2,
    stage: &str,
    expected: &PlacedBody,
    inventory: &NxAssemblyPositionObservation,
    abi: NativeBodyAbi,
) -> Result<FactOutcome> {
    ensure!(
        f.stage == stage && ["acquired", "before_query", "after_cleanup"].contains(&stage),
        "motion2_binding_stage"
    );
    ensure!(f.calls.len() <= 13, "motion2_binding_call_bound");
    for call in &f.calls {
        raw_valid(call)?;
    }
    let p = &f.placement;
    ensure!(
        p.occurrence_id == expected.occurrence_id
            && p.prototype_id == expected.prototype_id
            && p.prototype_body_tag == expected.prototype_body_tag,
        "motion2_placement_plan_identity"
    );
    let prototype = inventory
        .prototypes
        .iter()
        .find(|v| v.prototype_id == p.prototype_id)
        .context("motion2_prototype_missing")?;
    ensure!(
        p.prototype_owner_tag == prototype.native_part_tag,
        "motion2_prototype_owner_identity"
    );
    let part_occurrence = match &expected.occurrence_id {
        Some(id) => {
            let paths = crate::assembly_position::persistence::paths(inventory)?;
            let node = inventory
                .occurrences
                .iter()
                .find(|v| &v.occurrence_id == id)
                .context("motion2_occurrence_missing")?;
            ensure!(
                p.occurrence_path.as_ref() == paths.get(id)
                    && p.part_occurrence_tag == Some(node.part_occurrence_tag),
                "motion2_full_occurrence_path_identity"
            );
            Some(u32::try_from(node.part_occurrence_tag)?)
        }
        None => {
            ensure!(
                p.occurrence_path.is_none()
                    && p.part_occurrence_tag.is_none()
                    && p.prototype_id == inventory.root_prototype_id,
                "motion2_root_placement"
            );
            None
        }
    };
    let expected = ExpectedBody {
        prototype_body: u32::try_from(p.prototype_body_tag)?,
        prototype_owner: u32::try_from(p.prototype_owner_tag)?,
        part_occurrence,
    };
    let body = u32::try_from(p.body_tag)?;
    ensure!(
        expected.prototype_body != 0 && expected.prototype_owner != 0,
        "motion2_prototype_tag_missing"
    );
    // Check every present observation before classifying missing capture. An
    // unknown early call must not hide a later, explicitly contradictory fact.
    // The expected sequence comes from the original native binding owner. It
    // constrains identity, never supplies a missing UF return value.
    let mut sequence: Vec<(&str, Vec<u64>)> = Vec::new();
    let mut solid = |tag: u64| {
        sequence.push(("UF_OBJ_ask_type_and_subtype", vec![tag]));
        sequence.push(("UF_MODL_ask_body_type", vec![tag]));
        sequence.push(("UF_OBJ_ask_owning_part", vec![tag]));
    };
    if stage == "acquired" {
        solid(p.prototype_body_tag);
        sequence.push(("UF_ASSEM_is_occurrence", vec![p.prototype_body_tag]));
        if let Some(part) = part_occurrence {
            sequence.push(("UF_ASSEM_is_occurrence", vec![part as u64]));
            sequence.push(("UF_ASSEM_ask_prototype_of_occ", vec![part as u64]));
            sequence.push((
                "UF_ASSEM_find_occurrence",
                vec![part as u64, p.prototype_body_tag],
            ));
        }
    }
    sequence.push(("UF_OBJ_ask_type_and_subtype", vec![p.body_tag]));
    sequence.push(("UF_MODL_ask_body_type", vec![p.body_tag]));
    sequence.push(("UF_OBJ_ask_owning_part", vec![p.body_tag]));
    sequence.push(("UF_ASSEM_is_occurrence", vec![p.body_tag]));
    if part_occurrence.is_some() {
        sequence.push(("UF_ASSEM_ask_prototype_of_occ", vec![p.body_tag]));
        sequence.push(("UF_ASSEM_ask_part_occurrence", vec![p.body_tag]));
    }
    ensure!(
        f.calls.len() <= sequence.len(),
        "motion2_extra_or_root_synthetic_query"
    );
    let mut observed_failure = false;
    for (call, (api, args)) in f.calls.iter().zip(&sequence) {
        ensure!(
            call.api == *api && call.arguments == *args,
            "motion2_actual_query_order_or_arguments"
        );
        if !call.captured {
            continue;
        }
        if call.return_code.is_some_and(|code| code != 0) {
            observed_failure = true;
            continue;
        }
        let raw = &call.raw_values;
        match call.api.as_str() {
            "UF_OBJ_ask_type_and_subtype" => ensure!(
                raw == &[abi.solid_type as i64, 0],
                "motion2_not_solid_object"
            ),
            "UF_MODL_ask_body_type" => {
                ensure!(raw == &[abi.solid_body as i64], "motion2_not_solid_body")
            }
            "UF_OBJ_ask_owning_part" => {
                ensure!(raw[0] != 0, "motion2_owner_missing");
                if call.arguments[0] == p.prototype_body_tag || part_occurrence.is_none() {
                    ensure!(
                        raw[0] == p.prototype_owner_tag as i64,
                        "motion2_actual_prototype_owner"
                    );
                }
            }
            "UF_ASSEM_is_occurrence" => {
                let expected = if call.arguments[0] == p.prototype_body_tag {
                    0
                } else if part_occurrence.is_some() {
                    1
                } else {
                    0
                };
                ensure!(
                    raw == &[expected],
                    "motion2_known_occurrence_logical_differs"
                );
            }
            "UF_ASSEM_ask_prototype_of_occ" => {
                let expected = if Some(call.arguments[0]) == p.part_occurrence_tag {
                    p.prototype_owner_tag
                } else {
                    p.prototype_body_tag
                };
                ensure!(raw == &[expected as i64], "motion2_known_prototype_differs");
            }
            "UF_ASSEM_find_occurrence" => ensure!(
                p.body_tag != 0 && raw == &[p.body_tag as i64],
                "motion2_find_occurrence_body"
            ),
            "UF_ASSEM_ask_part_occurrence" => ensure!(
                raw == &[p
                    .part_occurrence_tag
                    .context("motion2_containing_without_occurrence")?
                    as i64],
                "motion2_known_containing_differs"
            ),
            _ => unreachable!("raw_valid rejects unsupported APIs"),
        }
    }
    if observed_failure {
        return Ok(FactOutcome::Failed);
    }
    let mut calls = Calls {
        values: &f.calls,
        at: 0,
        failed: false,
    };
    macro_rules! observed {
        ($expr:expr) => {
            match $expr? {
                Some(v) => v,
                None => return Ok(calls.unresolved()),
            }
        };
    }
    if stage == "acquired" {
        let owner = observed!(calls.solid_owner(p.prototype_body_tag, abi));
        ensure!(
            owner == expected.prototype_owner,
            "motion2_actual_prototype_owner"
        );
        ensure!(
            observed!(calls.take("UF_ASSEM_is_occurrence", &[p.prototype_body_tag])) == [0],
            "motion2_prototype_is_occurrence"
        );
        if let Some(part) = part_occurrence {
            ensure!(
                observed!(calls.take("UF_ASSEM_is_occurrence", &[part as u64])) == [1],
                "motion2_part_not_occurrence"
            );
            ensure!(
                observed!(calls.take("UF_ASSEM_ask_prototype_of_occ", &[part as u64]))
                    == [p.prototype_owner_tag as i64],
                "motion2_part_prototype"
            );
            ensure!(
                observed!(calls.take(
                    "UF_ASSEM_find_occurrence",
                    &[part as u64, p.prototype_body_tag]
                )) == [p.body_tag as i64],
                "motion2_find_occurrence_body"
            );
        }
    }
    // A not-yet-acquired handle is represented by zero only while an earlier
    // required call is absent/failed. Never interpret that initialization as a
    // successful direct UF result or send it through a fabricated body query.
    ensure!(body != 0, "motion2_body_tag_missing");
    let owner = observed!(calls.solid_owner(p.body_tag, abi));
    let logical = observed!(calls.take("UF_ASSEM_is_occurrence", &[p.body_tag]))[0];
    ensure!(
        logical == 0 || logical == 1,
        "motion2_occurrence_logical_invalid"
    );
    let (prototype_body, containing_occurrence) = if logical == 1 {
        (
            observed!(calls.take("UF_ASSEM_ask_prototype_of_occ", &[p.body_tag]))[0] as u32,
            observed!(calls.take("UF_ASSEM_ask_part_occurrence", &[p.body_tag]))[0] as u32,
        )
    } else {
        (body, 0)
    };
    ensure!(
        calls.at == f.calls.len(),
        "motion2_extra_or_root_synthetic_query"
    );
    let actual = QueriedBody {
        body,
        is_occurrence: logical == 1,
        prototype_body,
        containing_occurrence,
        owner,
        is_solid_body: true,
    };
    validate_binding(&expected, &actual)?;
    Ok(FactOutcome::Valid(actual))
}
