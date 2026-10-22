//! Bounded correspondence under a source-bound engineering change declaration.
//! This is neither general persistent topology naming nor engineering adoption.
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CadCorrespondenceRequest1, CadCorrespondenceState1, CadFeatureBinding1,
    CadFeatureChange1, CadFeatureIntent1, CadSourcePointer1, NativeDrawingObservation,
    ObjectIdentity,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub mod component_copy;
pub mod construction;
pub mod feature_add;
pub mod feature_delete;
pub mod instances;
pub mod occurrence;

const SOURCE_BYTES: usize = 16 * 1024 * 1024;
const TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_BINDINGS: usize = 256;
const MAX_CHANGES: usize = 256;

#[derive(Debug)]
struct CurrentFeature {
    feature_id: String,
    object: ObjectIdentity,
    native_result_sha256: String,
    value: Value,
}

/// Only verify constructs this capability; serialized reports cannot restore it.
#[derive(Debug)]
pub struct VerifiedCorrespondence {
    request: CadCorrespondenceRequest1,
    report: Value,
    seeds: BTreeSet<String>,
    current: Vec<CurrentFeature>,
}
impl VerifiedCorrespondence {
    pub fn request(&self) -> &CadCorrespondenceRequest1 {
        &self.request
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn impact_seed_ids(&self) -> BTreeSet<String> {
        self.seeds.clone()
    }

    /// A second consumer must still run its original geometric/drawing checks.
    /// This checks the current endpoint of a bounded, supported correspondence.
    pub fn require_current_feature(
        &self,
        feature_id: &str,
        object: &ObjectIdentity,
        native_result_sha256: &str,
    ) -> Result<Value> {
        ensure!(
            self.request.current.object == *object,
            "cad_correspondence_current_object_differs"
        );
        let found: Vec<_> = self
            .current
            .iter()
            .filter(|v| v.feature_id == feature_id)
            .collect();
        ensure!(
            found.len() == 1,
            "cad_correspondence_current_feature_missing_or_ambiguous"
        );
        let current = found[0];
        ensure!(
            current.object == *object && current.native_result_sha256 == native_result_sha256,
            "cad_correspondence_current_observation_differs"
        );
        Ok(current.value.clone())
    }
}

struct Sources<'a> {
    root: &'a SkillRoot,
    captured: BTreeMap<PathBuf, (ArtifactReference, Vec<u8>, usize)>,
    bytes: usize,
}
impl<'a> Sources<'a> {
    fn new(root: &'a SkillRoot) -> Self {
        Self {
            root,
            captured: BTreeMap::new(),
            bytes: 0,
        }
    }
    fn read(&mut self, reference: &ArtifactReference) -> Result<Vec<u8>> {
        self.read_limit(reference, SOURCE_BYTES)
    }
    fn read_limit(&mut self, reference: &ArtifactReference, limit: usize) -> Result<Vec<u8>> {
        ensure!(limit <= TOTAL_BYTES, "cad_correspondence_source_budget");
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("cad_correspondence_source_reference_invalid"))?;
        let path = self.root.expand(Path::new(&reference.path))?;
        ensure!(
            path.starts_with(self.root.path().join("var")),
            "cad_correspondence_source_outside_private_root"
        );
        oe_core::no_symlinks(&path)?;
        if let Some((prior, bytes, _)) = self.captured.get(&path) {
            ensure!(bytes.len() <= limit, "cad_correspondence_source_budget");
            ensure!(
                prior.sha256 == reference.sha256,
                "cad_correspondence_source_identity_conflict"
            );
            return Ok(bytes.clone());
        }
        let bytes = read_regular_bytes(&path, limit)?;
        ensure!(
            sha256(&bytes) == reference.sha256,
            "cad_correspondence_source_changed"
        );
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .context("cad_correspondence_source_budget")?;
        ensure!(
            self.bytes <= TOTAL_BYTES,
            "cad_correspondence_source_budget"
        );
        self.captured
            .insert(path, (reference.clone(), bytes.clone(), limit));
        Ok(bytes)
    }
    fn pointer<T: DeserializeOwned>(&mut self, reference: &CadSourcePointer1) -> Result<T> {
        ensure!(
            reference.pointer.len() <= 2048,
            "cad_correspondence_pointer_budget"
        );
        let bytes = self.read(&reference.source)?;
        let value = oe_core::json::strict_json(&bytes)?;
        let selected = super::common::pointer(&value, &reference.pointer)?;
        serde_json::from_value(selected.clone())
            .context("cad_correspondence_source_pointer_contract")
    }
    fn verify_unchanged(&self) -> Result<()> {
        for (path, (_, before, limit)) in &self.captured {
            oe_core::no_symlinks(path)?;
            ensure!(
                read_regular_bytes(path, *limit)? == *before,
                "cad_correspondence_source_changed_during_check"
            );
        }
        Ok(())
    }
    fn report(&self) -> Value {
        json!(
            self.captured
                .iter()
                .map(
                    |(path, (r, b, _))| json!({"path":path,"reference":r,"bytes":b.len(),
            "sha256":r.sha256})
                )
                .collect::<Vec<_>>()
        )
    }
}

#[derive(Debug)]
struct Binding {
    intent: CadFeatureIntent1,
    check: Value,
    status: &'static str,
}
struct State {
    bindings: BTreeMap<String, Binding>,
    observation: Option<NativeDrawingObservation>,
    report: Value,
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn unique_ids(values: &[String]) -> bool {
    values.iter().all(|v| valid_id(v))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
fn binding_status(check: &Value) -> &'static str {
    match (check["conclusion"].as_str(), check["reason"].as_str()) {
        (Some("pass"), _) => "unique",
        (_, Some("model_circle_ambiguous")) => "ambiguous",
        (_, Some("model_circle_no_edge_matches")) => "missing",
        _ => "unresolved",
    }
}

fn state(sources: &mut Sources<'_>, input: &CadCorrespondenceState1) -> Result<State> {
    input
        .object
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_correspondence_object_invalid"))?;
    input
        .project_context
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_correspondence_context_invalid"))?;
    ensure!(
        ["nx", "solidworks"].contains(&input.backend.as_str()),
        "cad_correspondence_backend_unknown"
    );
    ensure!(
        input.bindings.len() <= MAX_BINDINGS && valid_id(&input.native_result_ref),
        "cad_correspondence_state_budget_or_source_id"
    );
    // Project semantics are checked by the existing kernel at the production caller.
    sources.read(&input.project)?;
    sources.read(&input.job)?;
    let job_path = sources.root.expand(Path::new(&input.job.path))?;
    ensure!(
        job_path.file_name().and_then(|v| v.to_str()) == Some("job.json"),
        "cad_correspondence_job_locator_invalid"
    );
    let job_root = job_path
        .parent()
        .context("cad_correspondence_job_parent_missing")?;
    let mut observation = None;
    let mut failure = None;
    if input.backend == "nx" {
        match crate::drawing::intent::observe_part_bound(
            job_root,
            &input.part_input,
            &input.object,
            &input.native_result_ref,
        ) {
            Ok(actual) => observation = Some(actual),
            Err(error) => failure = Some(error.to_string()),
        }
    } else {
        failure = Some("cad_correspondence_backend_not_qualified".into());
    }
    let mut bindings = BTreeMap::new();
    let mut details = Vec::new();
    for reference in &input.bindings {
        ensure!(
            valid_id(&reference.binding_id) && !bindings.contains_key(&reference.binding_id),
            "cad_correspondence_binding_id_invalid_or_duplicate"
        );
        let intent: CadFeatureIntent1 = sources.pointer(&reference.intent)?;
        intent
            .validate()
            .map_err(|_| anyhow::anyhow!("cad_correspondence_feature_intent_invalid"))?;
        ensure!(
            intent.schema == "oev2.cad-feature-intent/1"
                && intent.object == input.object
                && intent.binding_id == reference.binding_id,
            "cad_correspondence_feature_object_mismatch"
        );
        ensure!(
            !intent.dependency_ids.is_empty()
                && intent.dependency_ids.len() <= 256
                && unique_ids(&intent.dependency_ids),
            "cad_correspondence_dependency_ids_invalid"
        );
        oe_nx_worker::geometry::model_edges::validate_v2(&intent.selector)?;
        if let Some(occurrence) = &intent.occurrence {
            ensure!(
                !occurrence.occurrence_path.is_empty()
                    && occurrence.occurrence_path.len() <= 64
                    && occurrence.occurrence_path.iter().all(|v| valid_id(v))
                    && valid_id(&occurrence.prototype_id),
                "cad_correspondence_occurrence_invalid"
            );
            oe_nx_worker::assembly::Rigid::checked(&occurrence.local_to_root_m)?;
        }
        let check = match observation.as_ref() {
            Some(actual) => {
                oe_nx_worker::geometry::model_edges::select_v2(&intent.selector, &actual.part)?
            }
            None => {
                json!({"conclusion":"unknown","reason":"cad_correspondence_native_observation_unavailable",
                "candidate_edge_tags":[],"native_failure":failure})
            }
        };
        let status = if intent.occurrence.is_some() {
            "unresolved"
        } else {
            binding_status(&check)
        };
        details.push(json!({"binding_id":reference.binding_id,"feature_id":intent.selector.feature_id,
            "intent_source":reference.intent,"intent":intent,"definition_geometry_check":check,
            "binding_status":status,"use_scope":if intent.occurrence.is_some(){"instance"}else{"definition"},
            "instance_binding":if intent.occurrence.is_some(){"unresolved_not_qualified"}else{"not_requested"}}));
        bindings.insert(
            reference.binding_id.clone(),
            Binding {
                intent,
                check,
                status,
            },
        );
    }
    let report = json!({"backend":input.backend,"project":input.project,"project_context":input.project_context,
        "object":input.object,"job":input.job,"part_input":input.part_input,"native_result_ref":input.native_result_ref,
        "native_result_sha256":observation.as_ref().map(|o|o.native_result_sha256.as_str()),
        "native_observation":if observation.is_some(){"source_bound_completed_inspection"}else{"unavailable"},
        "native_failure":failure,"bindings":details,"project_review":"requires_existing_kernel_check"});
    Ok(State {
        bindings,
        observation,
        report,
    })
}

fn change_shape(change: &CadFeatureChange1) -> Result<()> {
    let b = change.before_bindings.len();
    let a = change.after_bindings.len();
    ensure!(
        b <= MAX_BINDINGS && a <= MAX_BINDINGS,
        "cad_correspondence_change_budget"
    );
    ensure!(
        valid_id(&change.change_id)
            && unique_ids(
                &change
                    .before_bindings
                    .iter()
                    .map(|b| b.binding_id.clone())
                    .collect::<Vec<_>>()
            )
            && unique_ids(
                &change
                    .after_bindings
                    .iter()
                    .map(|b| b.binding_id.clone())
                    .collect::<Vec<_>>()
            ),
        "cad_correspondence_change_ids_invalid"
    );
    let valid = match change.kind.as_str() {
        "retained" | "copied" => b == 1 && a == 1,
        "new" => b == 0 && a == 1,
        "deleted" => b == 1 && a == 0,
        "split" => b == 1 && a >= 2,
        "merged" => b >= 2 && a == 1,
        "ambiguous" | "unresolved" => b + a > 0,
        _ => false,
    };
    ensure!(
        valid,
        "cad_correspondence_change_kind_or_cardinality_invalid"
    );
    Ok(())
}

fn select_bindings<'a>(
    state: &'a State,
    input: &CadCorrespondenceState1,
    references: &[CadFeatureBinding1],
) -> Result<Vec<&'a Binding>> {
    references
        .iter()
        .map(|reference| {
            let actual = input
                .bindings
                .iter()
                .find(|v| v.binding_id == reference.binding_id)
                .context("cad_correspondence_change_binding_missing")?;
            ensure!(
                actual == reference,
                "cad_correspondence_change_binding_source_mismatch"
            );
            state
                .bindings
                .get(&reference.binding_id)
                .context("cad_correspondence_change_binding_missing")
        })
        .collect()
}

fn changed_fields(before: &CadFeatureIntent1, after: &CadFeatureIntent1) -> Result<Vec<String>> {
    let b = serde_json::to_value(&before.selector)?;
    let a = serde_json::to_value(&after.selector)?;
    let mut result: Vec<_> = b
        .as_object()
        .context("cad_correspondence_selector_object")?
        .keys()
        .filter(|key| b[*key] != a[*key])
        .map(|key| format!("selector.{key}"))
        .collect();
    if before.occurrence != after.occurrence {
        result.push("occurrence".into());
    }
    if before.dependency_ids.iter().collect::<BTreeSet<_>>()
        != after.dependency_ids.iter().collect::<BTreeSet<_>>()
    {
        result.push("dependency_ids".into());
    }
    Ok(result)
}

fn classify(
    change: &CadFeatureChange1,
    before: &[&Binding],
    after: &[&Binding],
) -> (&'static str, &'static str) {
    if change.kind == "split" || change.kind == "merged" {
        return ("unresolved", "cad_correspondence_split_merge_not_supported");
    }
    if before
        .iter()
        .chain(after)
        .any(|b| b.intent.occurrence.is_some())
    {
        return ("unresolved", "cad_correspondence_instance_not_qualified");
    }
    if before.iter().chain(after).any(|b| b.status == "ambiguous") {
        return ("ambiguous", "cad_correspondence_geometric_ambiguity");
    }
    if before.iter().chain(after).any(|b| b.status == "unresolved") {
        return ("unresolved", "cad_correspondence_observation_incomplete");
    }
    if before.iter().chain(after).any(|b| b.status == "missing") {
        return ("unresolved", "cad_correspondence_feature_missing");
    }
    if change.kind != "retained" {
        return (
            "declared_only",
            "cad_correspondence_change_lineage_not_verified",
        );
    }
    if before[0].intent.selector.feature_id != after[0].intent.selector.feature_id
        || change.previous.object_id != change.current.object_id
    {
        return (
            "unresolved",
            "cad_correspondence_engineering_identity_not_retained",
        );
    }
    (
        "bounded_declared_correspondence",
        "intent_and_observation_match",
    )
}

/// Read original intent/change bytes and re-observe the existing completed job.
/// No network, native dispatch, Project mutation, adopted flag or new lifecycle.
pub fn verify(
    root: &SkillRoot,
    request: &CadCorrespondenceRequest1,
) -> Result<VerifiedCorrespondence> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_correspondence_request_invalid"))?;
    ensure!(
        request.schema == "oev2.cad-correspondence/1"
            && !request.changes.is_empty()
            && request.changes.len() <= MAX_CHANGES,
        "cad_correspondence_schema_or_budget"
    );
    let mut sources = Sources::new(root);
    let previous = state(&mut sources, &request.previous)?;
    let current = state(&mut sources, &request.current)?;
    let mut change_ids = BTreeSet::new();
    let mut covered_before = BTreeSet::new();
    let mut covered_after = BTreeSet::new();
    let mut seeds = BTreeSet::new();
    let mut results = Vec::new();
    let mut qualified = Vec::new();
    for reference in &request.changes {
        let change: CadFeatureChange1 = sources.pointer(reference)?;
        change
            .validate()
            .map_err(|_| anyhow::anyhow!("cad_correspondence_change_contract_invalid"))?;
        ensure!(
            change.schema == "oev2.cad-feature-change/1"
                && change.previous == request.previous.object
                && change.current == request.current.object,
            "cad_correspondence_change_object_mismatch"
        );
        change_shape(&change)?;
        ensure!(
            change_ids.insert(change.change_id.clone()),
            "cad_correspondence_duplicate_change_id"
        );
        covered_before.extend(change.before_bindings.iter().map(|b| b.binding_id.clone()));
        for binding in &change.after_bindings {
            let id = &binding.binding_id;
            ensure!(
                covered_after.insert(id.clone()),
                "cad_correspondence_current_binding_reused"
            );
        }
        let before = select_bindings(&previous, &request.previous, &change.before_bindings)?;
        let after = select_bindings(&current, &request.current, &change.after_bindings)?;
        if change.kind == "copied" {
            ensure!(
                before[0].intent.selector.feature_id != after[0].intent.selector.feature_id,
                "cad_correspondence_copy_requires_new_feature_identity"
            );
        }
        let (status, reason) = classify(&change, &before, &after);
        let differences = if before.len() == 1 && after.len() == 1 {
            changed_fields(&before[0].intent, &after[0].intent)?
        } else {
            Vec::new()
        };
        let intent_sources_changed =
            if change.before_bindings.len() == 1 && change.after_bindings.len() == 1 {
                let a = request
                    .previous
                    .bindings
                    .iter()
                    .find(|b| b.binding_id == change.before_bindings[0].binding_id)
                    .unwrap();
                let b = request
                    .current
                    .bindings
                    .iter()
                    .find(|b| b.binding_id == change.after_bindings[0].binding_id)
                    .unwrap();
                a.intent != b.intent
            } else {
                true
            };
        let evidence_changed = request.previous.object != request.current.object
            || request.previous.job != request.current.job
            || previous
                .observation
                .as_ref()
                .map(|o| &o.native_result_sha256)
                != current
                    .observation
                    .as_ref()
                    .map(|o| &o.native_result_sha256)
            || intent_sources_changed;
        let needs_review = status != "bounded_declared_correspondence"
            || evidence_changed
            || !differences.is_empty();
        if needs_review {
            seeds.extend([
                request.previous.object.object_id.clone(),
                request.current.object.object_id.clone(),
                request.previous.native_result_ref.clone(),
                request.current.native_result_ref.clone(),
            ]);
            for b in before.iter().chain(&after) {
                seeds.extend(b.intent.dependency_ids.iter().cloned());
            }
        }
        let result = json!({"change_id":change.change_id,"declared_kind":change.kind,"declaration":change,
            "change_source":reference,"correspondence_status":status,"reason":reason,
            "before_checks":before.iter().map(|b| &b.check).collect::<Vec<_>>(),
            "after_checks":after.iter().map(|b| &b.check).collect::<Vec<_>>(),
            "declared_design_changes":differences,"evidence_rebinding_required":evidence_changed,
            "review":if needs_review{"required"}else{"no_correspondence_delta"},"engineering_adoption":"not_established",
            "scope":"source-bound engineering identity declaration corroborated by each unique observation; creator lineage not verified by this profile; no general persistent topology naming"});
        if status == "bounded_declared_correspondence" {
            let native = current
                .observation
                .as_ref()
                .context("cad_correspondence_supported_without_observation")?;
            qualified.push(CurrentFeature { feature_id: after[0].intent.selector.feature_id.clone(),
                object: request.current.object.clone(), native_result_sha256: native.native_result_sha256.clone(),
                value: json!({"change_id":change.change_id,"feature_id":after[0].intent.selector.feature_id,
                    "object":request.current.object,"native_result_sha256":native.native_result_sha256,
                    "binding_id":change.after_bindings[0].binding_id,"selector":after[0].intent.selector,
                    "correspondence_status":status,"review":if needs_review{"required"}else{"no_correspondence_delta"},
                    "creator_lineage":"not_verified_by_this_profile","engineering_adoption":"not_established"}) });
        }
        results.push(result);
    }
    ensure!(
        covered_before == previous.bindings.keys().cloned().collect()
            && covered_after == current.bindings.keys().cloned().collect(),
        "cad_correspondence_binding_coverage_incomplete"
    );
    sources.verify_unchanged()?;
    // Re-read native files as well as declared JSON. A changed native source must
    // not be admitted just because the first observation succeeded.
    for (input, state) in [(&request.previous, &previous), (&request.current, &current)] {
        if let Some(first) = &state.observation {
            let path = root.expand(Path::new(&input.job.path))?;
            let second = crate::drawing::intent::observe_part_bound(
                path.parent()
                    .context("cad_correspondence_job_parent_missing")?,
                &input.part_input,
                &input.object,
                &input.native_result_ref,
            )?;
            ensure!(
                second == *first,
                "cad_correspondence_native_changed_during_check"
            );
        }
    }
    sources.verify_unchanged()?;
    let report = json!({"schema":"oev2.cad-correspondence-report/1","request_sha256":oe_core::json::canonical_digest(&serde_json::to_value(request)?)?,
        "previous":previous.report,"current":current.report,"changes":results,"sources":sources.report(),
        "impact_seed_ids":seeds,"project_review":"requires_existing_kernel_check","engineering_adoption":"not_established",
        "native_dispatch":"not_run","persistent_topology_naming":"not_established","creator_lineage":"not_verified_by_this_profile",
        "source_verification":"declared_json_checked_against_original_bytes"});
    Ok(VerifiedCorrespondence {
        request: request.clone(),
        report,
        seeds,
        current: qualified,
    })
}
