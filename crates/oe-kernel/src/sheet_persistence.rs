//! Current persistence obligations reuse the CAD receipt owner and Project graph.
use crate::{CheckResult, engineering, project::Project};
use anyhow::{Context, Result, ensure};
use oe_contracts::{EngineeringAssertion, ObjectIdentity, SheetReferencePersistenceIntent};
use oe_core::{SkillRoot, read_regular_bytes};
use serde_json::{Value, json};
use std::path::Path;

fn declaration(
    project: &Project,
    object: &ObjectIdentity,
    a: &EngineeringAssertion,
) -> Result<SheetReferencePersistenceIntent> {
    let intent: SheetReferencePersistenceIntent = serde_json::from_value(a.requirements.clone())
        .context("sheet_persistence_intent_missing_or_invalid")?;
    intent
        .validate()
        .map_err(|_| anyhow::anyhow!("sheet_persistence_intent_invalid"))?;
    intent
        .object
        .validate()
        .map_err(|_| anyhow::anyhow!("sheet_persistence_object_invalid"))?;
    intent
        .sheet
        .validate()
        .map_err(|_| anyhow::anyhow!("sheet_persistence_sheet_invalid"))?;
    oe_cad::drawing::intent::validate(&intent.reference)?;
    ensure!(
        intent.schema == "oev2.sheet-reference-persistence-intent/1"
            && a.kind == "cad_sheet_reference_persistence"
            && intent.object == *object
            && a.subject == intent.reference.intent_id,
        "sheet_persistence_current_identity_mismatch"
    );
    crate::project::context(project, object)?;
    ensure!(
        intent.sheet.object == intent.reference.object
            && intent.sheet.unit == intent.reference.unit
            && object.object_id == intent.reference.object.object_id
            && object.condition_scope == intent.reference.object.condition_scope,
        "sheet_persistence_creation_identity_mismatch"
    );
    ensure!(
        intent.saved_result_ref != intent.reopened_result_ref,
        "sheet_persistence_result_roles_mismatch"
    );
    let refs = [
        intent.saved_result_ref.as_str(),
        intent.reopened_result_ref.as_str(),
    ];
    for id in refs {
        ensure!(
            project.evidence.contains_key(id) && a.source_refs.iter().any(|s| s == id),
            "sheet_persistence_result_source_unbound"
        );
    }
    ensure!(
        !intent.reference.source_refs.is_empty()
            && intent
                .reference
                .source_refs
                .iter()
                .all(|id| {
                    project.evidence.contains_key(id)
                        && a.source_refs.contains(id)
                        && refs.iter().all(|r| {
                            id.as_str() != *r && project.evidence[id] != project.evidence[*r]
                        })
                }),
        "sheet_persistence_independent_source_unbound"
    );
    // A source check must actually invalidate its declared dependent decisions.
    for id in &a.source_refs {
        let impact =
            crate::project::impact(project, &std::collections::BTreeSet::from([id.clone()]))?;
        let affected = impact["affected"]
            .as_array()
            .context("sheet_persistence_impact_missing")?;
        ensure!(
            !a.affected_decisions.is_empty()
                && a.affected_decisions
                    .iter()
                    .all(|d| affected.contains(&json!(d))),
            "sheet_persistence_dependency_closure_missing"
        );
    }
    let basis = engineering::basis(project, object)?.context("sheet_persistence_basis_missing")?;
    let f = basis
        .facts
        .iter()
        .find(|f| f.property == format!("sheet_reference_persistence:{}", a.subject))
        .context("sheet_persistence_fact_missing")?;
    ensure!(
        f.status == "declared"
            && f.value == a.requirements
            && a.source_refs.iter().all(|s| f.source_refs.contains(s)),
        "sheet_persistence_current_declaration_mismatch"
    );
    Ok(intent)
}
fn result(
    project: &Project,
    a: &EngineeringAssertion,
    current: bool,
    checked: Result<Value>,
) -> CheckResult {
    let id = format!(
        "{}:{}",
        if current {
            "engineering_native_source"
        } else {
            "engineering"
        },
        a.assertion_id
    );
    let mut r = match checked {
        Ok(v) => engineering::result(&id, "pass", None, v),
        Err(e) => {
            let reason = e.to_string();
            let violated = [
                "sheet_persistence_current_identity_mismatch",
                "sheet_persistence_creation_identity_mismatch",
                "sheet_persistence_current_declaration_mismatch",
                "sheet_persistence_original_intent_mismatch",
                "sheet_persistence_result_source_mismatch",
                "sheet_persistence_saved_source_mismatch",
                "sheet_persistence_inputs_changed",
            ]
            .contains(&reason.as_str());
            engineering::result(
                &id,
                if violated { "violated" } else { "unknown" },
                Some(reason),
                json!({"engineering_adoption":"not_established"}),
            )
        }
    };
    r.result["details"]["affected_decisions"] = json!(engineering::affected(project, a));
    r.coverage=vec![if current{"current original jobs, source-bound independent plan, unique parents/member view and saved/reopened reference"}else{"declared current obligation and source roles only; original native receipts not recomputed"}.into()];
    r
}
pub fn declared(
    project: &Project,
    object: &ObjectIdentity,
    a: &EngineeringAssertion,
) -> CheckResult {
    result(project,a,false,declaration(project,object,a).map(|i|json!({"object":i.object,"native_persistence":"not_recomputed","engineering_adoption":"not_established"})))
}
pub fn current(
    root: &SkillRoot,
    project: &Project,
    object: &ObjectIdentity,
    a: &EngineeringAssertion,
) -> CheckResult {
    let check = (|| -> Result<Value> {
        let i = declaration(project, object, a)?;
        let saved = root.private_path(Path::new(&i.saved_job_directory))?;
        let reopened = root.private_path(Path::new(&i.reopened_job_directory))?;
        let s = read_regular_bytes(&saved.join("native-result.json"), 16 * 1024 * 1024)?;
        let r = read_regular_bytes(&reopened.join("native-result.json"), 16 * 1024 * 1024)?;
        let p = read_regular_bytes(&saved.join("params.json"), 16 * 1024 * 1024)?;
        ensure!(
            project.evidence[&i.saved_result_ref] == oe_core::sha256(&s)
                && project.evidence[&i.reopened_result_ref] == oe_core::sha256(&r),
            "sheet_persistence_result_source_mismatch"
        );
        let params = oe_core::json::strict_json(&p)?;
        let actual_sheet: oe_contracts::NativeDraftingPlan =
            serde_json::from_value(params["sheet"].clone())?;
        let actual_reference: oe_contracts::DrawingReferenceIntent =
            serde_json::from_value(params["reference"].clone())?;
        ensure!(
            actual_sheet == i.sheet && actual_reference == i.reference,
            "sheet_persistence_original_intent_mismatch"
        );
        let compared = oe_cad::drawing::associations::compare_sheet_reference(&saved, &reopened)?;
        let parts = compared["parts"]
            .as_array()
            .context("sheet_persistence_parts_missing")?;
        ensure!(
            parts.len() == 1
                && parts[0]["observation_persistence"] == "matches"
                && parts[0]["sheet_reference_check"]["conclusion"] == "pass",
            "sheet_persistence_unproven"
        );
        ensure!(
            parts[0]["saved_input_identity"]["sha256"] == object.source_sha256,
            "sheet_persistence_saved_source_mismatch"
        );
        ensure!(
            s == read_regular_bytes(&saved.join("native-result.json"), 16 * 1024 * 1024)?
                && r == read_regular_bytes(&reopened.join("native-result.json"), 16 * 1024 * 1024)?
                && p == read_regular_bytes(&saved.join("params.json"), 16 * 1024 * 1024)?,
            "sheet_persistence_inputs_changed"
        );
        Ok(
            json!({"comparison":compared,"object":object,"original_creation_object":i.reference.object,"visual_readability":"not_assessed","manufacturing_drawing":"not_established","engineering_adoption":"not_established"}),
        )
    })();
    result(project, a, true, check)
}
