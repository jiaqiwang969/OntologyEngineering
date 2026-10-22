//! Immutable private version definitions, explicit selection, real use and restoration.
//! Authority is a locally owned policy, not an organization or publication signature.
use crate::artifacts;
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ControlledVersionDefinition, ControlledVersionPort, ControlledVersionState, FailureRecord,
    VersionSelectionRequest, VersionSelectionResult,
};
use oe_core::{AdvisoryFileLock, SkillRoot, sha256, write_new};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const POLICY: &str = "var/state/controlled-version-authority.json";
const BASE: &str = "var/state/controlled-versions";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Authority {
    schema: String,
    bindings: BTreeMap<String, BindingAuthority>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingAuthority {
    kind: String,
    scope_id: String,
    actor_scopes: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    code_qualification: Option<oe_contracts::ArtifactReference>,
    #[serde(default)]
    code_build_supervision: Option<oe_contracts::ArtifactReference>,
}

fn policy(
    root: &SkillRoot,
    id: &str,
    actor: Option<(&str, &str)>,
) -> Result<(BindingAuthority, String)> {
    let (v, digest) = artifacts::private_policy(root, POLICY)?;
    let mut p: Authority = serde_json::from_value(v.clone())?;
    ensure!(
        [
            "oev2.controlled-version-authority/1",
            "oev2.controlled-version-authority/2",
            "oev2.controlled-version-authority/3"
        ]
        .contains(&p.schema.as_str()),
        "version_authority_schema"
    );
    // Presence, including null, belongs to the new explicit protocol. Option
    // deserialization must not silently widen the original /1 strict parser.
    ensure!(
        v["bindings"]
            .as_object()
            .context("version_authority_bindings")?
            .values()
            .all(|b| b.get("code_qualification").is_none()
                || ([
                    "oev2.controlled-version-authority/2",
                    "oev2.controlled-version-authority/3"
                ]
                .contains(&p.schema.as_str())
                    && b["kind"] == "code_artifact")),
        "version_code_context_requires_explicit_policy2"
    );
    ensure!(
        v["bindings"]
            .as_object()
            .context("version_authority_bindings")?
            .values()
            .all(|b| b.get("code_build_supervision").is_none()
                || (p.schema == "oev2.controlled-version-authority/3"
                    && b["kind"] == "code_artifact")),
        "version_build_context_requires_explicit_policy3"
    );
    let binding = p
        .bindings
        .remove(id)
        .context("version_binding_not_authorized")?;
    if let Some((actor, action)) = actor {
        ensure!(
            binding
                .actor_scopes
                .get(actor)
                .is_some_and(|s| s.contains(action)),
            "version_actor_not_authorized"
        );
    }
    Ok((binding, digest))
}
fn base(root: &SkillRoot, id: &str) -> Result<PathBuf> {
    ensure!(!id.trim().is_empty(), "version_binding_empty");
    root.private_path(Path::new(&format!("{BASE}/{}", sha256(id))))
}
fn definition_path(root: &SkillRoot, id: &str, revision: &str) -> Result<PathBuf> {
    Ok(base(root, id)?
        .join("definitions")
        .join(format!("{}.json", sha256(revision))))
}
fn definition(root: &SkillRoot, id: &str, revision: &str) -> Result<ControlledVersionDefinition> {
    let reference = artifacts::reference(root, &definition_path(root, id, revision)?)?;
    let d: ControlledVersionDefinition =
        serde_json::from_value(artifacts::json(root, &reference)?)?;
    d.validate()
        .map_err(|_| anyhow::anyhow!("version_definition_invalid"))?;
    ensure!(
        d.schema == "oev2.controlled-version-definition/1"
            && d.binding_id == id
            && d.revision == revision,
        "version_definition_identity_changed"
    );
    Ok(d)
}
fn definition_hash(d: &ControlledVersionDefinition) -> Result<String> {
    Ok(sha256(oe_core::json::canonical_json(
        &serde_json::to_value(d)?,
    )?))
}
fn check_scope(p: &BindingAuthority, d: &ControlledVersionDefinition) -> Result<()> {
    let family = if d.kind == "method_baseline" {
        "operation_method"
    } else {
        &d.kind
    };
    ensure!(
        p.kind == family && p.scope_id == d.scope_id,
        "version_authority_scope_changed"
    );
    Ok(())
}
fn eligibility(root: &SkillRoot, d: &ControlledVersionDefinition) -> Result<Value> {
    artifacts::read(root, &d.source)?;
    artifacts::read(root, &d.assurance)?;
    match d.kind.as_str() {
        "operation_method" | "method_baseline" => {
            let path = root.private_path(Path::new(&d.source.path))?;
            ensure!(
                path.file_name().is_some_and(|n| n == "freeze.json"),
                "version_method_source_not_freeze"
            );
            let verified =
                oe_kernel::learning::registered_plan(root, path.parent().unwrap(), &d.kind)?;
            ensure!(
                verified["assurance"] == serde_json::to_value(&d.assurance)?,
                "version_method_assurance_mismatch"
            );
            Ok(verified)
        }
        "semantic_bundle" => {
            root.private_path(Path::new(&d.source.path))?;
            oe_kernel::governance::verified_promoted_at(
                root,
                &format!("private:{}", d.source.path),
                Path::new(&d.assurance.path),
            )
        }
        "code_artifact" => {
            use oe_contracts::CodeArtifactEligibilityPort;
            let request = oe_contracts::CodeArtifactEligibilityRequest {
                schema: "oev2.code-artifact-eligibility-request/1".into(),
                binding_id: d.binding_id.clone(),
                revision: d.revision.clone(),
                scope_id: d.scope_id.clone(),
                source: d.source.clone(),
                assurance: d.assurance.clone(),
            };
            let result = CodeArtifactEligibilityExecutor { root }
                .qualify(&request)
                .map_err(|e| anyhow::anyhow!("version_code_qualification:{e:?}"))?;
            result
                .validate()
                .map_err(|_| anyhow::anyhow!("version_code_qualification_contract"))?;
            ensure!(
                result.schema == "oev2.code-artifact-eligibility-result/1"
                    && result.artifact_sha256 == d.source.sha256
                    && result.comparison_sha256 == d.assurance.sha256,
                "version_code_qualification_binding_changed"
            );
            ensure!(
                result.execution == "completed"
                    && result.eligibility == "eligible"
                    && result.uncovered.is_empty(),
                "version_code_qualification_not_connected"
            );
            Ok(serde_json::to_value(result)?)
        }
        _ => anyhow::bail!("version_kind_unsupported"),
    }
}

/// Registration records exact assets; activation always has a separate fresh gate.
pub fn register(root: &SkillRoot, d: &ControlledVersionDefinition, actor: &str) -> Result<Value> {
    d.validate()
        .map_err(|_| anyhow::anyhow!("version_definition_invalid"))?;
    ensure!(
        d.schema == "oev2.controlled-version-definition/1"
            && [
                "method_baseline",
                "operation_method",
                "semantic_bundle",
                "code_artifact"
            ]
            .contains(&d.kind.as_str()),
        "version_definition_schema_or_kind"
    );
    let (p, policy_sha) = policy(root, &d.binding_id, Some((actor, "register")))?;
    check_scope(&p, d)?;
    artifacts::read(root, &d.source)?;
    artifacts::read(root, &d.assurance)?;
    let file = definition_path(root, &d.binding_id, &d.revision)?;
    oe_core::ensure_directory(file.parent().unwrap())?;
    ensure!(
        policy(root, &d.binding_id, Some((actor, "register")))?.1 == policy_sha,
        "version_policy_changed"
    );
    write_new(&file, &serde_json::to_value(d)?)?;
    Ok(
        json!({"status":"registered","definition":artifacts::reference(root,&file)?,
        "activation":"not_run","engineering_adoption":"not_established"}),
    )
}
fn history(root: &SkillRoot, id: &str, policy_sha: &str) -> Result<Vec<Value>> {
    let folder = base(root, id)?.join("events");
    if !folder.exists() {
        return Ok(vec![]);
    }
    let mut files = Vec::new();
    for item in std::fs::read_dir(&folder)? {
        let p = item?.path();
        if p.extension().is_some_and(|e| e == "json") {
            files.push(p);
        }
    }
    files.sort();
    let mut h = Vec::<Value>::new();
    for (i, p) in files.iter().enumerate() {
        ensure!(
            p.file_name().and_then(|n| n.to_str()) == Some(&format!("{i:06}.json")),
            "version_history_sequence"
        );
        let r = artifacts::reference(root, p)?;
        let v = artifacts::json(root, &r)?;
        let previous = h
            .last()
            .map(|v| v["event_sha256"].clone())
            .unwrap_or(Value::Null);
        let mut body = v.clone();
        body.as_object_mut()
            .context("version_history_invalid")?
            .remove("event_sha256");
        ensure!(
            v["schema"] == "oev2.version-selection-event/1"
                && v["binding_id"] == id
                && v["sequence"] == i
                && v["previous_sha256"] == previous
                && v["policy_sha256"] == policy_sha
                && v["event_sha256"] == sha256(oe_core::json::canonical_json(&body)?),
            "version_history_changed"
        );
        let action = v["action"].as_str().context("version_history_action")?;
        ensure!(
            ["activate", "restore"].contains(&action),
            "version_history_action"
        );
        let from = if h.is_empty() {
            ControlledVersionState::Unselected
        } else {
            ControlledVersionState::Active
        };
        let event = if action == "restore" {
            "Restore"
        } else {
            "Activate"
        };
        ensure!(
            from.transition(event)
                .is_some_and(|(next, _)| next == ControlledVersionState::Active)
                && v["from"] == serde_json::to_value(from)?
                && v["state"] == "active",
            "version_history_state"
        );
        let actor = v["actor"].as_str().context("version_history_actor")?;
        policy(root, id, Some((actor, action)))?;
        let d = definition(
            root,
            id,
            v["revision"].as_str().context("version_history_revision")?,
        )?;
        ensure!(
            v["definition_sha256"] == definition_hash(&d)?,
            "version_definition_changed"
        );
        if action == "restore" {
            ensure!(
                h.iter().any(|e| e["revision"] == v["revision"]),
                "version_restore_target_never_selected"
            );
        }
        h.push(v);
    }
    Ok(h)
}

pub struct SelectionExecutor<'a> {
    pub root: &'a SkillRoot,
    pub restore: bool,
}
impl ControlledVersionPort for SelectionExecutor<'_> {
    fn select(
        &self,
        input: &VersionSelectionRequest,
    ) -> std::result::Result<VersionSelectionResult, FailureRecord> {
        select(self.root, input, self.restore)
            .map_err(|e| artifacts::failure("controlled_version", e))
    }
}
pub fn select(
    root: &SkillRoot,
    r: &VersionSelectionRequest,
    restore: bool,
) -> Result<VersionSelectionResult> {
    r.validate()
        .map_err(|_| anyhow::anyhow!("version_selection_invalid"))?;
    ensure!(
        r.schema == "oev2.version-selection-request/1" && !r.reason.trim().is_empty(),
        "version_selection_reason_or_schema"
    );
    let action = if restore { "restore" } else { "activate" };
    let (p, policy_sha) = policy(root, &r.binding_id, Some((&r.actor, action)))?;
    let d = definition(root, &r.binding_id, &r.revision)?;
    check_scope(&p, &d)?;
    let verified = eligibility(root, &d)?;
    let folder = base(root, &r.binding_id)?;
    oe_core::ensure_directory(&folder)?;
    let lock = AdvisoryFileLock::acquire(&folder.join("selection.lock"), true)?;
    let h = history(root, &r.binding_id, &policy_sha)?;
    let head = h
        .last()
        .and_then(|v| v["event_sha256"].as_str())
        .map(str::to_owned);
    ensure!(
        head == r.expected_head_sha256,
        "version_selection_head_changed"
    );
    if restore {
        ensure!(
            h.iter().any(|v| v["revision"] == r.revision),
            "version_restore_target_never_selected"
        );
    }
    let from = if h.is_empty() {
        ControlledVersionState::Unselected
    } else {
        ControlledVersionState::Active
    };
    let event_name = if restore { "Restore" } else { "Activate" };
    ensure!(
        from.transition(event_name)
            .is_some_and(|(next, _)| next == ControlledVersionState::Active),
        "version_selection_state_refused"
    );
    let mut event = json!({"schema":"oev2.version-selection-event/1","sequence":h.len(),
        "binding_id":r.binding_id,"revision":r.revision,"kind":d.kind,"scope_id":d.scope_id,
        "actor":r.actor,"action":action,"reason":r.reason,"previous_sha256":head,"from":from,"state":"active",
        "policy_sha256":policy_sha,"definition_sha256":definition_hash(&d)?,"eligibility":verified,
        "engineering_adoption":"not_established","publication":"not_performed"});
    event["event_sha256"] = json!(sha256(oe_core::json::canonical_json(&event)?));
    ensure!(
        policy(root, &r.binding_id, Some((&r.actor, action)))?.1 == policy_sha,
        "version_policy_changed"
    );
    artifacts::read(root, &d.source)?;
    artifacts::read(root, &d.assurance)?;
    if d.kind == "code_artifact" {
        ensure!(
            eligibility(root, &d)? == verified,
            "version_code_qualification_changed_before_commit"
        );
    }
    lock.ensure_current()?;
    let events = folder.join("events");
    oe_core::ensure_directory(&events)?;
    let file = events.join(format!("{:06}.json", h.len()));
    write_new(&file, &event)?;
    Ok(VersionSelectionResult {
        schema: "oev2.version-selection-result/1".into(),
        status: if restore { "restored" } else { "selected" }.into(),
        binding_id: r.binding_id.clone(),
        revision: r.revision.clone(),
        kind: d.kind,
        source: d.source,
        event: artifacts::reference(root, &file)?,
    })
}
pub fn active(root: &SkillRoot, id: &str) -> Result<(ControlledVersionDefinition, Value)> {
    let (p, digest) = policy(root, id, None)?;
    let h = history(root, id, &digest)?;
    let head = h
        .last()
        .context("version_binding_has_no_selection")?
        .clone();
    let d = definition(
        root,
        id,
        head["revision"]
            .as_str()
            .context("version_revision_missing")?,
    )?;
    check_scope(&p, &d)?;
    eligibility(root, &d)?;
    ensure!(
        policy(root, id, None)?.1 == digest,
        "version_policy_changed"
    );
    Ok((d, head))
}
pub fn use_method(root: &SkillRoot, id: &str, input: &Value, output: &Path) -> Result<Value> {
    let folder = base(root, id)?;
    ensure!(folder.is_dir(), "version_binding_has_no_selection");
    let selection_lock = AdvisoryFileLock::acquire(&folder.join("selection.lock"), true)?;
    let (d, head) = active(root, id)?;
    ensure!(
        ["method_baseline", "operation_method"].contains(&d.kind.as_str()),
        "version_is_not_method"
    );
    let path = root.private_path(Path::new(&d.source.path))?;
    let sequence = head["sequence"]
        .as_u64()
        .context("version_selection_sequence_missing")?;
    let selection = artifacts::reference(
        root,
        &folder.join("events").join(format!("{sequence:06}.json")),
    )?;
    ensure!(
        artifacts::json(root, &selection)? == head,
        "version_selection_changed_before_use"
    );
    let mut result = oe_kernel::learning::apply_registered_bound(
        root,
        path.parent().unwrap(),
        &d.kind,
        input,
        output,
        &selection,
    )?;
    selection_lock.ensure_current()?;
    let (_, current) = active(root, id)?;
    ensure!(
        head["event_sha256"] == current["event_sha256"],
        "version_changed_during_use"
    );
    result["version_selection"] = head;
    result["definition"] = serde_json::to_value(d)?;
    let dir = root.private_path(output)?;
    write_new(&dir.join("version-use.json"), &result)?;
    Ok(result)
}
pub fn rule_bundle(root: &SkillRoot, id: &str) -> Result<(String, Value)> {
    let (d, head) = active(root, id)?;
    ensure!(
        d.kind == "semantic_bundle",
        "version_is_not_semantic_bundle"
    );
    Ok((format!("private:{}", d.source.path), head))
}

/// Composition point for the one version owner. No proof DTO grants itself eligibility.
pub struct CodeArtifactEligibilityExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeArtifactEligibilityPort for CodeArtifactEligibilityExecutor<'_> {
    fn qualify(
        &self,
        input: &oe_contracts::CodeArtifactEligibilityRequest,
    ) -> std::result::Result<oe_contracts::CodeArtifactEligibilityResult, FailureRecord> {
        code_eligibility(self.root, input)
            .map_err(|e| artifacts::failure("code_artifact_eligibility", e))
    }
}
impl oe_contracts::CodeScopeAssociationPort for CodeArtifactEligibilityExecutor<'_> {
    fn associate(
        &self,
        input: &oe_contracts::CodeArtifactEligibilityRequest,
    ) -> std::result::Result<oe_contracts::CodeScopeAssociationResult, FailureRecord> {
        crate::code_qualification::associate(self.root, input)
            .map_err(|e| artifacts::failure("code_scope_association", e))
    }
}
pub fn code_eligibility(
    root: &SkillRoot,
    input: &oe_contracts::CodeArtifactEligibilityRequest,
) -> Result<oe_contracts::CodeArtifactEligibilityResult> {
    crate::code_qualification::qualify(root, input)
}

/// The existing version authority selects build inputs before any dispatch.
pub struct CodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeBuildSupervisionPort for CodeBuildSupervisionExecutor<'_> {
    fn supervise(
        &self,
        input: &oe_contracts::CodeBuildSupervisionRequest,
    ) -> std::result::Result<oe_contracts::CodeBuildSupervisionResult, FailureRecord> {
        crate::code_build_supervision::supervise(self.root, input)
            .map_err(|e| artifacts::failure("code_build_supervision", e))
    }
}

pub(super) fn build_context(
    root: &SkillRoot,
    input: &oe_contracts::CodeBuildSupervisionRequest,
) -> Result<(
    oe_contracts::CodeBuildSupervisionContext,
    String,
    oe_contracts::ArtifactReference,
)> {
    let (binding, policy_sha256) = policy(
        root,
        &input.binding_id,
        Some((&input.actor, "supervise_build")),
    )?;
    ensure!(
        binding.kind == "code_artifact" && binding.scope_id == input.scope_id,
        "code_build_supervision_policy_scope_changed"
    );
    let reference = binding
        .code_build_supervision
        .context("code_build_supervision_context_missing")?;
    root.private_path(Path::new(&reference.path))?;
    let context: oe_contracts::CodeBuildSupervisionContext =
        serde_json::from_value(artifacts::json(root, &reference)?)?;
    context
        .validate()
        .map_err(|_| anyhow::anyhow!("code_build_supervision_context_invalid"))?;
    ensure!(
        [
            "oev2.code-build-supervision-context/1",
            "oev2.code-build-supervision-context/2"
        ]
        .contains(&context.schema.as_str())
            && context.purpose == "observe_compilation_origin"
            && context.binding_id == input.binding_id
            && context.revision == input.revision
            && context.scope_id == input.scope_id,
        "code_build_supervision_context_binding_changed"
    );
    Ok((context, policy_sha256, reference))
}

pub(super) fn owner_folder(root: &SkillRoot, id: &str) -> Result<PathBuf> {
    base(root, id)
}

/// Explicit native execution uses this same authority, context slot and lock.
pub struct NativeCodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeBuildSupervisionPort for NativeCodeBuildSupervisionExecutor<'_> {
    fn supervise_native(
        &self,
        input: &oe_contracts::CodeNativeBuildSupervisionRequest,
    ) -> std::result::Result<oe_contracts::CodeNativeBuildSupervisionResult, FailureRecord> {
        crate::code_build_supervision::supervise_native(self.root, input)
            .map_err(|e| artifacts::failure("native_code_build_supervision", e))
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct OwnedQualificationContext {
    pub qualification: oe_contracts::CodeQualificationContext,
    pub full_scope_request: Option<oe_contracts::ArtifactReference>,
}

/// Resolve a predeclared revision without changing the policy or its history.
/// All leaves are complete /2 contracts; only the selected leaf's actual
/// artifacts are consumed by the unchanged qualification executor.
fn revision_qualification_context(
    value: Value,
    input: &oe_contracts::CodeArtifactEligibilityRequest,
) -> Result<OwnedQualificationContext> {
    let contexts: oe_contracts::CodeRevisionQualificationContext3 = serde_json::from_value(value)?;
    contexts
        .validate()
        .map_err(|_| anyhow::anyhow!("code_qualification_revision_set_invalid"))?;
    ensure!(
        contexts.schema == "oev2.code-qualification-context/3"
            && contexts.binding_id == input.binding_id
            && contexts.scope_id == input.scope_id,
        "code_qualification_revision_set_binding_changed"
    );
    ensure!(
        !contexts.revisions.is_empty(),
        "code_qualification_revision_set_empty"
    );
    let mut revisions = BTreeSet::new();
    let mut selected = None;
    for leaf in contexts.revisions {
        leaf.validate()
            .map_err(|_| anyhow::anyhow!("code_qualification_revision_leaf_invalid"))?;
        let context = &leaf.qualification;
        ensure!(
            leaf.schema == "oev2.code-qualification-context/2"
                && context.schema == "oev2.code-qualification-context/1"
                && context.purpose == "full_code_release"
                && context.binding_id == contexts.binding_id
                && context.scope_id == contexts.scope_id,
            "code_qualification_revision_leaf_binding_changed"
        );
        ensure!(
            !context.revision.trim().is_empty() && revisions.insert(context.revision.clone()),
            "code_qualification_revision_empty_or_duplicate"
        );
        // The original base DTO uses typed fields whose validators are invoked
        // by the execution path. Check their structure in every new /3 leaf,
        // without reading unselected files or dispatching any process.
        for reference in [
            &context.source,
            &context.assurance,
            &context.experiment,
            &context.required_protocol,
        ] {
            reference
                .validate()
                .map_err(|_| anyhow::anyhow!("code_qualification_revision_reference_invalid"))?;
        }
        for binary in [&context.driver, &context.evaluator] {
            binary
                .validate()
                .map_err(|_| anyhow::anyhow!("code_qualification_revision_binary_invalid"))?;
        }
        context
            .dispatch_budget
            .validate()
            .map_err(|_| anyhow::anyhow!("code_qualification_revision_budget_invalid"))?;
        if context.revision == input.revision {
            selected = Some(OwnedQualificationContext {
                qualification: leaf.qualification,
                full_scope_request: Some(leaf.full_scope_request),
            });
        }
    }
    selected.context("code_qualification_revision_not_registered")
}

pub(super) fn qualification_context(
    root: &SkillRoot,
    input: &oe_contracts::CodeArtifactEligibilityRequest,
) -> Result<
    Option<(
        OwnedQualificationContext,
        String,
        oe_contracts::ArtifactReference,
    )>,
> {
    let path = root.private_path(Path::new(POLICY))?;
    if !path.try_exists()? {
        return Ok(None);
    }
    let (binding, policy_sha256) = policy(root, &input.binding_id, None)?;
    let Some(reference) = binding.code_qualification else {
        return Ok(None);
    };
    ensure!(
        binding.kind == "code_artifact" && binding.scope_id == input.scope_id,
        "code_qualification_policy_scope_changed"
    );
    root.private_path(Path::new(&reference.path))?;
    let raw_context = artifacts::json(root, &reference)?;
    // A new optional DTO field must not widen the original strict /1 wire
    // protocol. Presence, including null, is an explicit /2 request.
    match raw_context["schema"].as_str() {
        Some("oev2.code-qualification-context/1") => ensure!(
            raw_context.get("full_scope_request").is_none(),
            "code_qualification_scope_request_requires_context2"
        ),
        Some("oev2.code-qualification-context/2") => ensure!(
            raw_context
                .get("full_scope_request")
                .is_some_and(|v| !v.is_null()),
            "code_qualification_full_scope_request_required"
        ),
        Some("oev2.code-qualification-context/3") => (),
        _ => anyhow::bail!("code_qualification_context_schema"),
    }
    let owned = if raw_context["schema"] == "oev2.code-qualification-context/3" {
        revision_qualification_context(raw_context, input)?
    } else if raw_context["schema"] == "oev2.code-qualification-context/2" {
        let wrapper: oe_contracts::CodeFullScopeQualificationContext =
            serde_json::from_value(raw_context)?;
        wrapper
            .validate()
            .map_err(|_| anyhow::anyhow!("code_qualification_full_scope_context_invalid"))?;
        OwnedQualificationContext {
            qualification: wrapper.qualification,
            full_scope_request: Some(wrapper.full_scope_request),
        }
    } else {
        OwnedQualificationContext {
            qualification: serde_json::from_value(raw_context)?,
            full_scope_request: None,
        }
    };
    let context = &owned.qualification;
    context
        .validate()
        .map_err(|_| anyhow::anyhow!("code_qualification_context_invalid"))?;
    ensure!(
        context.schema == "oev2.code-qualification-context/1"
            && context.purpose == "full_code_release"
            && context.binding_id == input.binding_id
            && context.revision == input.revision
            && context.scope_id == input.scope_id
            && context.source == input.source
            && context.assurance == input.assurance,
        "code_qualification_context_binding_changed"
    );
    Ok(Some((owned, policy_sha256, reference)))
}

pub(super) fn not_connected_code_eligibility(
    root: &SkillRoot,
    input: &oe_contracts::CodeArtifactEligibilityRequest,
) -> Result<oe_contracts::CodeArtifactEligibilityResult> {
    input
        .validate()
        .map_err(|_| anyhow::anyhow!("code_eligibility_request_invalid"))?;
    ensure!(
        input.schema == "oev2.code-artifact-eligibility-request/1",
        "code_eligibility_schema_unsupported"
    );
    root.private_path(Path::new(&input.source.path))?;
    root.private_path(Path::new(&input.assurance.path))?;
    // These bounded descriptors are inputs, not authentication of a large runner.
    // The one fixed driver's full-artifact verifier must supply the missing production adapter.
    artifacts::json(root, &input.source)?;
    artifacts::json(root, &input.assurance)?;
    Ok(oe_contracts::CodeArtifactEligibilityResult {
        schema: "oev2.code-artifact-eligibility-result/1".into(),
        execution: "not_run".into(),
        eligibility: "not_connected".into(),
        artifact_sha256: input.source.sha256.clone(),
        comparison_sha256: input.assurance.sha256.clone(),
        uncovered: vec![
            "fixed_driver_artifact_eligibility_adapter".into(),
            "whole_code_use_feedback_recover".into(),
        ],
        reason: "full artifact and independently fixed driver qualification not connected".into(),
    })
}

/// Full native3 delegates to the same version authority and supervision body.
pub struct NativeFullCodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeBuildSupervision3Port for NativeFullCodeBuildSupervisionExecutor<'_> {
    fn supervise_full_native(
        &self,
        input: &oe_contracts::CodeNativeBuildSupervisionRequest3,
    ) -> std::result::Result<oe_contracts::CodeNativeBuildSupervisionResult3, FailureRecord> {
        crate::code_build_supervision::supervise_full_native(self.root, input)
            .map_err(|e| artifacts::failure("full_native_code_build_supervision", e))
    }
}

/// Reproduction composes the same selected build authority; it owns no new registry.
pub struct NativeFullCodeReproductionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeReproduction3Port for NativeFullCodeReproductionExecutor<'_> {
    fn reproduce_full_native(
        &self,
        input: &oe_contracts::CodeNativeReproductionRequest3,
    ) -> std::result::Result<oe_contracts::CodeNativeReproductionObservation3, FailureRecord> {
        crate::native_reproduction::reproduce_full_native(self.root, input)
            .map_err(|e| artifacts::failure("native_full_reproduction", e))
    }
}

pub struct NativeLoader6CodeReproductionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeReproduction6Port for NativeLoader6CodeReproductionExecutor<'_> {
    fn reproduce_full_native(
        &self,
        input: &oe_contracts::CodeNativeReproductionRequest6,
    ) -> std::result::Result<oe_contracts::CodeNativeReproductionObservation6, FailureRecord> {
        crate::native_reproduction6::reproduce_loader6_native(self.root, input)
            .map_err(|e| artifacts::failure("native_full_reproduction", e))
    }
}

/// Explicit compiler7 mode delegates to the existing registry and selected owner.
pub struct NativeCompiler7CodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeBuildSupervision7Port
    for NativeCompiler7CodeBuildSupervisionExecutor<'_>
{
    fn supervise_full_native(
        &self,
        input: &oe_contracts::CodeNativeBuildSupervisionRequest7,
    ) -> std::result::Result<oe_contracts::CodeNativeBuildSupervisionResult7, FailureRecord> {
        crate::code_build_supervision::supervise_compiler7_native(self.root, input)
            .map_err(|e| artifacts::failure("compiler7_native_build_supervision", e))
    }
}
pub struct NativeCompiler7CodeReproductionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeReproduction7Port for NativeCompiler7CodeReproductionExecutor<'_> {
    fn reproduce_full_native(
        &self,
        input: &oe_contracts::CodeNativeReproductionRequest7,
    ) -> std::result::Result<oe_contracts::CodeNativeReproductionObservation7, FailureRecord> {
        crate::native_reproduction7::reproduce_compiler7_native(self.root, input)
            .map_err(|e| artifacts::failure("compiler7_native_reproduction", e))
    }
}

// SDK8 delegates to the existing registry, selection lock and elapsed-time owner.
pub struct NativeSdk8CodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeBuildSupervision8Port for NativeSdk8CodeBuildSupervisionExecutor<'_> {
    fn supervise_full_native(
        &self,
        input: &oe_contracts::CodeNativeBuildSupervisionRequest8,
    ) -> std::result::Result<oe_contracts::CodeNativeBuildSupervisionResult8, FailureRecord> {
        crate::code_build_supervision::supervise_sdk8_native(self.root, input)
            .map_err(|e| artifacts::failure("compiler7_native_build_supervision", e))
    }
}
pub struct NativeSdk8CodeReproductionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeReproduction8Port for NativeSdk8CodeReproductionExecutor<'_> {
    fn reproduce_full_native(
        &self,
        input: &oe_contracts::CodeNativeReproductionRequest8,
    ) -> std::result::Result<oe_contracts::CodeNativeReproductionObservation8, FailureRecord> {
        crate::native_reproduction8::reproduce_sdk8_native(self.root, input)
            .map_err(|e| artifacts::failure("compiler7_native_reproduction", e))
    }
}

// Explicit9 delegates to the existing registry, selection lock and elapsed-time owner.
pub struct NativeProbe9CodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeBuildSupervision9Port
    for NativeProbe9CodeBuildSupervisionExecutor<'_>
{
    fn supervise_full_native(
        &self,
        input: &oe_contracts::CodeNativeBuildSupervisionRequest9,
    ) -> std::result::Result<oe_contracts::CodeNativeBuildSupervisionResult9, FailureRecord> {
        crate::code_build_supervision::supervise_probe9_native(self.root, input)
            .map_err(|e| artifacts::failure("probe9_native_build_supervision", e))
    }
}
pub struct NativeProbe9CodeReproductionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeReproduction9Port for NativeProbe9CodeReproductionExecutor<'_> {
    fn reproduce_full_native(
        &self,
        input: &oe_contracts::CodeNativeReproductionRequest9,
    ) -> std::result::Result<oe_contracts::CodeNativeReproductionObservation9, FailureRecord> {
        crate::native_reproduction9::reproduce_probe9_native(self.root, input)
            .map_err(|e| artifacts::failure("probe9_native_reproduction", e))
    }
}

pub struct NativeProbe10CodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeBuildSupervision10Port
    for NativeProbe10CodeBuildSupervisionExecutor<'_>
{
    fn supervise_full_native(
        &self,
        input: &oe_contracts::CodeNativeBuildSupervisionRequest10,
    ) -> std::result::Result<oe_contracts::CodeNativeBuildSupervisionResult10, FailureRecord> {
        crate::code_build_supervision::supervise_probe10_native(self.root, input)
            .map_err(|e| artifacts::failure("probe10_native_build_supervision", e))
    }
}
pub struct NativeProbe10CodeReproductionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeReproduction10Port for NativeProbe10CodeReproductionExecutor<'_> {
    fn reproduce_full_native(
        &self,
        input: &oe_contracts::CodeNativeReproductionRequest10,
    ) -> std::result::Result<oe_contracts::CodeNativeReproductionObservation10, FailureRecord> {
        crate::native_reproduction10::reproduce_probe10_native(self.root, input)
            .map_err(|e| artifacts::failure("probe10_native_reproduction", e))
    }
}

/// Explicit stream11 mode uses the existing controlled-version owner.
pub struct NativeStream11CodeBuildSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeBuildSupervision11Port
    for NativeStream11CodeBuildSupervisionExecutor<'_>
{
    fn supervise_full_native(
        &self,
        input: &oe_contracts::CodeNativeBuildSupervisionRequest11,
    ) -> std::result::Result<oe_contracts::CodeNativeBuildSupervisionResult11, FailureRecord> {
        crate::code_build_supervision::supervise_stream11_native(self.root, input)
            .map_err(|e| artifacts::failure("stream11_native_build_supervision", e))
    }
}
pub struct NativeStream11CodeReproductionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeReproduction11Port for NativeStream11CodeReproductionExecutor<'_> {
    fn reproduce_full_native(
        &self,
        input: &oe_contracts::CodeNativeReproductionRequest11,
    ) -> std::result::Result<oe_contracts::CodeNativeReproductionObservation11, FailureRecord> {
        crate::native_reproduction11::reproduce_stream11_native(self.root, input)
            .map_err(|e| artifacts::failure("stream11_native_reproduction", e))
    }
}

#[cfg(test)]
#[path = "registry_context3_tests.rs"]
mod context3_tests;

pub struct NativeNormalizerSupervisionExecutor<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::CodeNativeNormalizerSupervision1Port
    for NativeNormalizerSupervisionExecutor<'_>
{
    fn prepare_normalizer(
        &self,
        input: &oe_contracts::CodeNativeNormalizerSupervisionRequest1,
    ) -> std::result::Result<oe_contracts::CodeNativeNormalizerSupervisionResult1, FailureRecord>
    {
        crate::code_build_supervision::supervise_normalizer(self.root, input)
            .map_err(|e| artifacts::failure("native_normalizer_supervision", e))
    }
}
