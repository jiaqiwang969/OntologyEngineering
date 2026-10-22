//! Explicit cross-version use of the original source/change and occurrence owners.
//! This supports bounded declared instance correspondence, not native lineage.
use super::{Sources, occurrence};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    CadCorrespondenceRequest1, CadFeatureBinding1, CadFeatureChange1, CadFeatureIntent1,
    CadInstanceCorrespondenceRequest2, CadOccurrenceBindingRequest1, ObjectIdentity,
};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct VerifiedInstanceCorrespondence {
    request: CadInstanceCorrespondenceRequest2,
    previous: occurrence::VerifiedOccurrenceBindings,
    current: occurrence::VerifiedOccurrenceBindings,
    report: Value,
    seeds: BTreeSet<String>,
    qualified: BTreeMap<String, Value>,
}
impl VerifiedInstanceCorrespondence {
    pub fn request(&self) -> &CadInstanceCorrespondenceRequest2 {
        &self.request
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn previous_bindings(&self) -> &occurrence::VerifiedOccurrenceBindings {
        &self.previous
    }
    pub fn current_bindings(&self) -> &occurrence::VerifiedOccurrenceBindings {
        &self.current
    }
    pub fn impact_seed_ids(&self) -> BTreeSet<String> {
        self.seeds.clone()
    }
    /// A root-frame geometric use remains derived, not a native occurrence edge.
    pub fn require_current_feature(
        &self,
        feature_id: &str,
        assembly: &ObjectIdentity,
        assembly_native_sha256: &str,
        definition_native_sha256: &str,
    ) -> Result<Value> {
        self.current.require_current_feature(
            feature_id,
            assembly,
            assembly_native_sha256,
            definition_native_sha256,
        )?;
        self.qualified
            .get(feature_id)
            .cloned()
            .context("cad_instance_correspondence_current_feature_unqualified")
    }
}
struct Use {
    intent: CadFeatureIntent1,
    observed: Value,
    qualified: Option<Value>,
}
fn uses(
    sources: &mut Sources<'_>,
    input: &CadOccurrenceBindingRequest1,
    checked: &occurrence::VerifiedOccurrenceBindings,
    bindings: &[CadFeatureBinding1],
) -> Result<Vec<Use>> {
    let rows = checked.report()["bindings"]
        .as_array()
        .context("cad_instance_correspondence_bindings_missing")?;
    bindings
        .iter()
        .map(|binding| {
            let intent: CadFeatureIntent1 = sources.pointer(&binding.intent)?;
            let found: Vec<_> = rows
                .iter()
                .filter(|v| v["binding_id"] == binding.binding_id)
                .collect();
            ensure!(
                found.len() == 1,
                "cad_instance_correspondence_current_binding_missing"
            );
            let observed = found[0].clone();
            ensure!(
                observed["feature_id"] == intent.selector.feature_id,
                "cad_instance_correspondence_feature_source_differs"
            );
            let qualified = if observed["status"] == "bounded_instance_use" {
                Some(
                    checked.require_current_feature(
                        &intent.selector.feature_id,
                        &input.assembly.object,
                        checked.report()["assembly"]["native_result_sha256"]
                            .as_str()
                            .context("cad_instance_correspondence_native_digest_missing")?,
                        checked.report()["definition"]["native_result_sha256"]
                            .as_str()
                            .context("cad_instance_correspondence_native_digest_missing")?,
                    )?,
                )
            } else {
                None
            };
            Ok(Use {
                intent,
                observed,
                qualified,
            })
        })
        .collect()
}
fn classify(
    change: &CadFeatureChange1,
    before: &[Use],
    after: &[Use],
    request: &CadInstanceCorrespondenceRequest2,
) -> (&'static str, &'static str) {
    if ["split", "merged"].contains(&change.kind.as_str()) {
        return ("unresolved", "cad_correspondence_split_merge_not_supported");
    }
    if change.kind == "ambiguous" {
        return (
            "ambiguous",
            "cad_instance_correspondence_declared_ambiguity",
        );
    }
    if change.kind == "unresolved" {
        return (
            "unresolved",
            "cad_instance_correspondence_declared_unresolved",
        );
    }
    if before.iter().chain(after).any(|u| u.qualified.is_none()) {
        return (
            "unresolved",
            "cad_instance_correspondence_endpoint_unqualified",
        );
    }
    if change.kind != "retained" {
        return (
            "declared_only",
            "cad_correspondence_change_lineage_not_verified",
        );
    }
    if change.previous.object_id != change.current.object_id
        || request.previous.assembly.object.object_id != request.current.assembly.object.object_id
        || before[0].intent.selector.feature_id != after[0].intent.selector.feature_id
    {
        return (
            "unresolved",
            "cad_instance_correspondence_engineering_identity_not_retained",
        );
    }
    // A stable declared full path is the bounded identity supported here.
    // Renaming/reassignment needs additional lineage; two valid endpoint poses
    // must not silently exchange two engineering feature identities.
    if before[0].observed["occurrence_path"] != after[0].observed["occurrence_path"] {
        return (
            "unresolved",
            "cad_instance_correspondence_path_change_requires_lineage",
        );
    }
    (
        "bounded_declared_instance_correspondence",
        "source_intent_and_both_instance_observations_match",
    )
}
fn native_identity_changed(before: &Value, after: &Value) -> bool {
    before["assembly"]["native_result_sha256"] != after["assembly"]["native_result_sha256"]
        || before["definition"]["native_result_sha256"]
            != after["definition"]["native_result_sha256"]
}
/// The original /1 owner retains its exact result; this explicit /2 combines
/// freshly checked opaque endpoint capabilities with the same change contract.
pub fn verify(
    root: &SkillRoot,
    request: &CadInstanceCorrespondenceRequest2,
) -> Result<VerifiedInstanceCorrespondence> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_instance_correspondence_request_invalid"))?;
    ensure!(
        request.schema == "oev2.cad-correspondence/2",
        "cad_instance_correspondence_schema_invalid"
    );
    ensure!(
        request.previous.definition.project_context.project_id
            == request.current.definition.project_context.project_id,
        "cad_instance_correspondence_project_identity_mismatch"
    );
    let original_request = CadCorrespondenceRequest1 {
        schema: "oev2.cad-correspondence/1".into(),
        previous: request.previous.definition.clone(),
        current: request.current.definition.clone(),
        changes: request.changes.clone(),
    };
    // Original shape, exact binding references, full coverage, copy identity,
    // source budgets and partial/ambiguous observations all remain in that owner.
    let original = super::verify(root, &original_request)?;
    let previous = occurrence::verify_for_correspondence(root, &request.previous)?;
    let current = occurrence::verify_for_correspondence(root, &request.current)?;
    let mut sources = Sources::new(root);
    let mut results = Vec::new();
    let mut seeds = BTreeSet::new();
    let mut qualified = BTreeMap::new();
    for (index, reference) in request.changes.iter().enumerate() {
        let change: CadFeatureChange1 = sources.pointer(reference)?;
        let old = &original.report()["changes"][index];
        ensure!(
            old["declaration"] == serde_json::to_value(&change)?,
            "cad_instance_correspondence_change_sources_differ"
        );
        let before = uses(
            &mut sources,
            &request.previous,
            &previous,
            &change.before_bindings,
        )?;
        let after = uses(
            &mut sources,
            &request.current,
            &current,
            &change.after_bindings,
        )?;
        let (status, reason) = classify(&change, &before, &after, request);
        // Two valid endpoint observations can name different instances. A
        // same-instance change requires the retained identity to be supported.
        let comparable = status == "bounded_declared_instance_correspondence"
            && before.len() == 1
            && after.len() == 1
            && before[0].qualified.is_some()
            && after[0].qualified.is_some();
        let placement_changed = comparable
            .then(|| before[0].observed["local_to_root_m"] != after[0].observed["local_to_root_m"]);
        let root_geometry_changed = comparable.then(|| {
            before[0].observed["derived_circle_root_m"]
                != after[0].observed["derived_circle_root_m"]
        });
        let evidence_changed = old["evidence_rebinding_required"] == true
            || request.previous.definition.part_input != request.current.definition.part_input
            || request.previous.definition.native_result_ref
                != request.current.definition.native_result_ref
            || request.previous.assembly.object != request.current.assembly.object
            || request.previous.assembly.job != request.current.assembly.job
            || request.previous.assembly.part_input != request.current.assembly.part_input
            || request.previous.assembly.native_result_ref
                != request.current.assembly.native_result_ref
            || native_identity_changed(previous.report(), current.report());
        let needs_review = status != "bounded_declared_instance_correspondence"
            || evidence_changed
            || placement_changed == Some(true)
            || root_geometry_changed == Some(true)
            || !old["declared_design_changes"]
                .as_array()
                .context("cad_instance_correspondence_design_changes_missing")?
                .is_empty();
        if needs_review {
            for input in [&request.previous, &request.current] {
                seeds.extend([
                    input.definition.object.object_id.clone(),
                    input.assembly.object.object_id.clone(),
                    input.definition.native_result_ref.clone(),
                    input.assembly.native_result_ref.clone(),
                ]);
            }
            for used in before.iter().chain(&after) {
                seeds.extend(used.intent.dependency_ids.iter().cloned());
            }
        }
        let review = if needs_review {
            "required"
        } else {
            "no_correspondence_delta"
        };
        if status == "bounded_declared_instance_correspondence" {
            let mut value = after[0]
                .qualified
                .clone()
                .context("cad_instance_correspondence_supported_without_current_use")?;
            value["assembly_object"] = json!(request.current.assembly.object);
            value["change_id"] = json!(change.change_id);
            value["correspondence_status"] = json!(status);
            value["review"] = json!(review);
            qualified.insert(after[0].intent.selector.feature_id.clone(), value);
        }
        results.push(json!({"change_id":change.change_id,"declared_kind":change.kind,"declaration":change,"change_source":reference,
            "correspondence_status":status,"reason":reason,"before_bindings":before.iter().map(|u|&u.observed).collect::<Vec<_>>(),
            "after_bindings":after.iter().map(|u|&u.observed).collect::<Vec<_>>(),"declared_design_changes":old["declared_design_changes"],
            "observed_instance_placement_changed":placement_changed,"observed_root_geometry_changed":root_geometry_changed,
            "evidence_rebinding_required":evidence_changed,"review":review,"creator_lineage":"not_verified_by_this_profile",
            "native_occurrence_edge_observed":false,"engineering_adoption":"not_established"}));
    }
    sources.verify_unchanged()?;
    let original_again = super::verify(root, &original_request)?;
    let previous_again = occurrence::verify_for_correspondence(root, &request.previous)?;
    let current_again = occurrence::verify_for_correspondence(root, &request.current)?;
    ensure!(
        original_again.report() == original.report()
            && previous_again.report() == previous.report()
            && current_again.report() == current.report(),
        "cad_instance_correspondence_sources_changed_during_check"
    );
    sources.verify_unchanged()?;
    let report = json!({"schema":"oev2.cad-correspondence-report/2",
        "request_sha256":oe_core::json::canonical_digest(&serde_json::to_value(request)?)?,
        "previous":previous.report(),"current":current.report(),"changes":results,"sources":sources.report(),
        "legacy_definition_contract":original.report(),"impact_seed_ids":seeds,"project_review":"requires_existing_kernel_check",
        "geometry_basis":"derived from selected prototype circle and source-backed assembly placement; no native occurrence edge",
        "native_dispatch":"not_run","creator_lineage":"not_verified_by_this_profile",
        "persistent_topology_naming":"not_established","engineering_adoption":"not_established"});
    Ok(VerifiedInstanceCorrespondence {
        request: request.clone(),
        previous,
        current,
        report,
        seeds,
        qualified,
    })
}
