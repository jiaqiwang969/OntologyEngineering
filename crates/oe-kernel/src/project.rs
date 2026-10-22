//! Source/condition changes invalidate affected decisions; preserve original history.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub mod cad_focus;
pub mod decide;
pub mod memory;
pub mod route;
pub mod tool_result;

pub fn context(
    project: &Project,
    object: &oe_contracts::ObjectIdentity,
) -> Result<oe_contracts::ProjectReviewContext> {
    validate(project)?;
    object
        .validate()
        .map_err(|_| anyhow::anyhow!("project_context_object_invalid"))?;
    ensure!(
        project.objects.get(&object.object_id) == Some(&object.revision)
            && project
                .evidence
                .values()
                .any(|s| s == &object.source_sha256),
        "project_context_identity_mismatch"
    );
    let condition = project
        .conditions
        .get(&object.condition_scope)
        .context("project_context_condition_missing")?;
    Ok(oe_contracts::ProjectReviewContext {
        schema: "oev2.project-review-context/1".into(),
        project_id: project.project_id.clone(),
        project_revision: project.revision.clone(),
        // Decisions are outputs: adding a review must not create a self-referential hash.
        // Evidence contains source inputs; receipts stay in the existing history ledger.
        project_sha256: oe_core::sha256(oe_wire::json::canonical_json(&json!({
            "project_id":project.project_id,"revision":project.revision,"purpose":project.purpose,
            "objects":project.objects,"conditions":project.conditions,"evidence":project.evidence,
            "dependencies":project.dependencies
        }))?),
        object_id: object.object_id.clone(),
        object_revision: object.revision.clone(),
        condition_scope: object.condition_scope.clone(),
        condition_sha256: oe_core::sha256(condition.as_bytes()),
        source_sha256: object.source_sha256.clone(),
    })
}

/// Check content without suppressing the separate historical RDF diagnostics.
pub fn check_current_context(
    project: &Project,
    object: &oe_contracts::ObjectIdentity,
    expected: Option<&oe_contracts::ProjectReviewContext>,
) -> crate::CheckResult {
    let checked = (|| -> Result<oe_contracts::ProjectReviewContext> {
        let expected = expected.context("current_project_context_missing")?;
        expected
            .validate()
            .map_err(|_| anyhow::anyhow!("current_project_context_invalid"))?;
        ensure!(
            expected.schema == "oev2.project-review-context/1",
            "current_project_context_schema"
        );
        let actual = context(project, object)?;
        ensure!(*expected == actual, "current_project_context_changed");
        Ok(actual)
    })();
    match checked {
        Ok(actual) => crate::CheckResult {
            id: "current_project_context".into(),
            execution: "completed".into(),
            conclusion: "pass".into(),
            coverage: vec!["Project content, object, condition contents and source".into()],
            failures: vec![],
            result: json!({"context":actual,"current_use":"content_bound","physical_acceptance":"not_established"}),
        },
        Err(error) => crate::CheckResult {
            id: "current_project_context".into(),
            execution: "completed".into(),
            conclusion: if expected.is_none() {
                "unknown"
            } else {
                "violated"
            }
            .into(),
            coverage: vec!["current Project identity".into()],
            failures: vec![error.to_string()],
            result: json!({"current_use":"blocked","reason":error.to_string(),"historical_review":"retained"}),
        },
    }
}

pub fn review_current(
    root: &oe_core::SkillRoot,
    project: &Project,
    evidence: &[u8],
    historical: crate::Receipt,
) -> Result<crate::Receipt> {
    let object = oe_contracts::ObjectIdentity {
        object_id: historical.binding.object_id.clone(),
        revision: historical.binding.object_revision.clone(),
        condition_scope: historical.binding.condition_scope.clone(),
        source_sha256: historical.binding.source_sha256.clone(),
    };
    let mut checks = historical.checks;
    checks.extend(current_checks(
        root,
        project,
        &object,
        historical.binding.project_context.as_ref(),
        evidence,
        historical.binding.rdf_focus.as_ref(),
    ));
    let mut receipt = crate::evaluate(historical.binding, evidence, checks)?;
    receipt
        .adoption_conditions
        .extend(historical.adoption_conditions);
    receipt.adoption_conditions.push("current Project contents checked; engineering models and physical adoption still need independent evidence".into());
    Ok(receipt)
}

/// Exactly one execution of the current Project, model link and engineering checks.
fn current_checks(
    root: &oe_core::SkillRoot,
    project: &Project,
    object: &oe_contracts::ObjectIdentity,
    expected: Option<&oe_contracts::ProjectReviewContext>,
    evidence: &[u8],
    mapping: Option<&oe_contracts::RdfObjectFocus>,
) -> Vec<crate::CheckResult> {
    let mut checks = vec![
        check_current_context(project, object, expected),
        check_condition_model_mapped(project, object, evidence, mapping),
    ];
    checks.extend(crate::engineering::checks_current(root, project, object));
    checks
}

/// An explicit model-to-condition link prevents rebinding fresh metadata to old RDF.
/// This is a content identity check, not a proof of a physical model's correctness.
pub fn check_condition_model(
    project: &Project,
    object: &oe_contracts::ObjectIdentity,
    evidence: &[u8],
) -> crate::CheckResult {
    check_condition_model_mapped(project, object, evidence, None)
}
fn check_condition_model_mapped(
    project: &Project,
    object: &oe_contracts::ObjectIdentity,
    evidence: &[u8],
    mapping: Option<&oe_contracts::RdfObjectFocus>,
) -> crate::CheckResult {
    use oxigraph::model::{NamedOrBlankNode, Term};
    let checked = (|| -> Result<Option<bool>> {
        let actual = context(project, object)?;
        let graph = crate::graph::load(evidence)?;
        let focus = NamedOrBlankNode::NamedNode(crate::rdf_focus::resolve(
            &object.object_id,
            mapping,
            &graph,
        )?);
        let values: Vec<_> = graph
            .iter()
            .filter(|quad| {
                quad.subject == focus.as_ref()
                    && quad.predicate.as_str() == "urn:oev2:project-context:conditionSha256"
            })
            .map(|quad| quad.object.into_owned())
            .collect();
        if values.is_empty() {
            return Ok(None);
        }
        ensure!(values.len() == 1, "current_condition_model_ambiguous");
        Ok(Some(matches!(&values[0], Term::Literal(value)
            if value.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#string"
            && value.value() == actual.condition_sha256)))
    })();
    let (execution, conclusion, reason) = match checked {
        Ok(Some(true)) => ("completed", "pass", None),
        Ok(Some(false)) => (
            "completed",
            "violated",
            Some("current_condition_model_changed".to_owned()),
        ),
        Ok(None) => (
            "completed",
            "unknown",
            Some("current_condition_model_missing".to_owned()),
        ),
        Err(error) => (
            "failed",
            "unknown",
            Some(format!("current_condition_model_unavailable: {error}")),
        ),
    };
    crate::CheckResult {
        id: "current_condition_model".into(),
        execution: execution.into(),
        conclusion: conclusion.into(),
        coverage: vec!["RDF focus declaration of current condition content hash".into()],
        failures: reason.clone().into_iter().collect(),
        result: json!({"reason":reason,"physical_acceptance":"not_established","content_link":"conditionSha256"}),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema: String,
    pub project_id: String,
    pub revision: String,
    pub purpose: String,
    pub objects: BTreeMap<String, String>,
    pub conditions: BTreeMap<String, String>,
    pub evidence: BTreeMap<String, String>,
    pub decisions: Vec<Decision>,
    pub dependencies: BTreeMap<String, BTreeSet<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub id: String,
    pub object_id: String,
    pub object_revision: String,
    pub condition_scope: String,
    pub source_sha256: String,
    pub conclusion: String,
    pub adopted: bool,
    pub basis: Vec<String>,
}
pub fn validate(p: &Project) -> Result<()> {
    let nonempty = |s: &str| !s.trim().is_empty();
    let digest = |s: &str| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit());
    ensure!(
        p.schema == "oev2.project/1"
            && nonempty(&p.project_id)
            && nonempty(&p.revision)
            && nonempty(&p.purpose),
        "project_identity_invalid"
    );
    ensure!(
        p.objects
            .iter()
            .chain(&p.conditions)
            .all(|(id, value)| nonempty(id) && nonempty(value))
            && p.evidence
                .iter()
                .all(|(id, value)| nonempty(id) && digest(value))
            && p.dependencies.iter().all(
                |(id, dependencies)| nonempty(id) && dependencies.iter().all(|id| nonempty(id))
            ),
        "project_binding_identity_invalid"
    );
    let mut ids = BTreeSet::new();
    for d in &p.decisions {
        ensure!(
            nonempty(&d.id)
                && nonempty(&d.object_revision)
                && nonempty(&d.condition_scope)
                && digest(&d.source_sha256)
                && d.basis.iter().all(|id| nonempty(id))
                && d.basis.iter().collect::<BTreeSet<_>>().len() == d.basis.len()
                && ids.insert(&d.id)
                && p.objects.contains_key(&d.object_id)
                && ["supported", "refuted", "unknown"].contains(&d.conclusion.as_str()),
            "project_decision_invalid"
        );
        ensure!(
            !matches!(d.conclusion.as_str(), "supported" | "refuted")
                || !d.basis.iter().any(|b| memory::is_memory_evidence(b)),
            "project_memory_evidence_not_decisive"
        );
        ensure!(
            !d.adopted || d.conclusion == "supported",
            "project_unknown_cannot_be_adopted"
        );
        ensure!(
            !d.adopted || !d.basis.is_empty(),
            "project_adoption_basis_missing"
        );
    }
    Ok(())
}
pub fn impact(p: &Project, changes: &BTreeSet<String>) -> Result<Value> {
    validate(p)?;
    let mut affected = changes.clone();
    loop {
        let before = affected.len();
        for (target, deps) in &p.dependencies {
            if deps.iter().any(|s| affected.contains(s)) {
                affected.insert(target.clone());
            }
        }
        for d in &p.decisions {
            if affected.contains(&d.object_id)
                || affected.contains(&d.condition_scope)
                || affected.contains(&d.source_sha256)
                || d.basis.iter().any(|s| affected.contains(s))
            {
                affected.insert(d.id.clone());
            }
        }
        if affected.len() == before {
            break;
        }
    }
    let decisions:Vec<_>=p.decisions.iter().filter(|d|affected.contains(&d.id)||affected.contains(&d.object_id)||d.basis.iter().any(|s|affected.contains(s))).map(|d|json!({"decision_id":d.id,"prior_conclusion":d.conclusion,"current_status":"requires_review","reason":"linked object/source/condition changed; previous history retained"})).collect();
    Ok(
        json!({"schema":"oev2.project-impact/1","project_id":p.project_id,"affected":affected,"decisions":decisions,"unknown_dependencies":"unlisted links remain unverified"}),
    )
}
/// The same declared purpose dependency is used by current premise checks and
/// before/after reconciliation. Absent dependency declarations require a
/// conservative review; explicit unrelated dependencies remain unrelated.
pub(crate) fn purpose_changes(projects: &[&Project]) -> BTreeSet<String> {
    let mut changes = BTreeSet::from(["project:purpose".into(), "purpose".into()]);
    let purpose_bound = projects.iter().any(|project| {
        project
            .dependencies
            .values()
            .any(|deps| deps.contains("purpose") || deps.contains("project:purpose"))
            || project.decisions.iter().any(|decision| {
                decision
                    .basis
                    .iter()
                    .any(|id| id == "purpose" || id == "project:purpose")
            })
    });
    if !purpose_bound {
        changes.extend(
            projects
                .iter()
                .flat_map(|p| p.decisions.iter().map(|d| d.id.clone())),
        );
    }
    changes
}

pub fn reconcile(previous: &Project, current: &Project) -> Result<Value> {
    validate(previous)?;
    validate(current)?;
    ensure!(
        previous.project_id == current.project_id,
        "project_identity_cannot_change"
    );
    let mut changes = BTreeSet::new();
    // Purpose is an engineering input, including when object/source/scope names
    // stay unchanged. Honor declared purpose dependencies; if none exist the
    // boundary is unknown and all decisions require review conservatively.
    if previous.purpose != current.purpose {
        changes.extend(purpose_changes(&[previous, current]));
    }
    for (before, after) in [
        (&previous.objects, &current.objects),
        (&previous.conditions, &current.conditions),
    ] {
        for key in before.keys().chain(after.keys()) {
            if before.get(key) != after.get(key) {
                changes.insert(key.clone());
            }
        }
    }
    for key in previous.evidence.keys().chain(current.evidence.keys()) {
        if previous.evidence.get(key) != current.evidence.get(key) {
            changes.insert(key.clone());
            // Dependencies name source IDs, but decisions bind source content.
            // Seed both identities only for an actual evidence mapping change;
            // generic impact callers may name unchanged supporting sources.
            changes.extend(previous.evidence.get(key).cloned());
            changes.extend(current.evidence.get(key).cloned());
        }
    }
    let old: BTreeMap<_, _> = previous.decisions.iter().map(|d| (&d.id, d)).collect();
    for d in &current.decisions {
        if old.get(&d.id).is_some_and(|p| {
            p.object_id != d.object_id
                || p.object_revision != d.object_revision
                || p.condition_scope != d.condition_scope
                || p.source_sha256 != d.source_sha256
                || p.basis != d.basis
                || p.conclusion != d.conclusion
                || p.adopted != d.adopted
        }) {
            changes.insert(d.id.clone());
        }
    }
    let current_ids: BTreeSet<_> = current.decisions.iter().map(|d| &d.id).collect();
    changes.extend(
        previous
            .decisions
            .iter()
            .filter(|d| !current_ids.contains(&d.id))
            .map(|d| d.id.clone()),
    );
    for target in previous
        .dependencies
        .keys()
        .chain(current.dependencies.keys())
    {
        if previous.dependencies.get(target) != current.dependencies.get(target) {
            changes.insert(target.clone());
        }
    }
    // Use both dependency baselines: removing a link cannot erase the old impact.
    let prior = impact(previous, &changes)?;
    let mut report = impact(current, &changes)?;
    let affected: BTreeSet<String> = prior["affected"]
        .as_array()
        .context("impact_affected_missing")?
        .iter()
        .chain(
            report["affected"]
                .as_array()
                .context("impact_affected_missing")?,
        )
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .context("impact_identity_invalid")
        })
        .collect::<Result<_>>()?;
    let affected_decisions: BTreeSet<_> = previous
        .decisions
        .iter()
        .chain(&current.decisions)
        .filter(|d| {
            affected.contains(&d.id)
                || affected.contains(&d.object_id)
                || d.basis.iter().any(|s| affected.contains(s))
        })
        .map(|d| d.id.clone())
        .collect();
    let decisions:Vec<_>=current.decisions.iter().filter(|d|affected_decisions.contains(&d.id)).map(|d|json!({"decision_id":d.id,"prior_conclusion":old.get(&d.id).map(|p|&p.conclusion),"current_conclusion":d.conclusion,"current_status":"requires_review","reason":"linked object/source/condition changed; previous history retained"})).collect();
    let mut stale = Vec::new();
    for d in &current.decisions {
        let object = current
            .objects
            .get(&d.object_id)
            .context("object_missing")?;
        let evidence = current
            .evidence
            .values()
            .any(|digest| digest == &d.source_sha256);
        if d.adopted
            && (object != &d.object_revision || !evidence || affected_decisions.contains(&d.id))
        {
            stale.push(d.id.clone());
        }
    }
    stale.sort();
    report["affected"] = json!(affected);
    report["decisions"] = json!(decisions);
    report["stale_adoptions"] = json!(stale);
    report["adoption_validation"] = json!(
        "fresh binding-bound execution evidence is required for affected decisions; this input schema does not attest fresh review"
    );
    Ok(report)
}

/// Current typed Project review on the original promoted-registry owner. The
/// historical archive Binding is intentionally not involved in this identity.
pub fn review_promoted_current(
    root: &oe_core::SkillRoot,
    input: &oe_contracts::TypedPromotedProjectReviewRequest1,
) -> Result<oe_contracts::TypedPromotedProjectReviewReceipt1> {
    let required = validate_promoted_request(input)?;
    let binding_path = root.private_path(std::path::Path::new(&input.semantic_binding.path))?;
    let task_path = root.private_path(std::path::Path::new(&input.task.path))?;
    let evidence_path = root.private_path(std::path::Path::new(&input.evidence.path))?;
    let workspace = root.private_path(std::path::Path::new(&input.workspace))?;
    let request = oe_semantic::lifecycle::ProjectReviewRequest {
        binding: &binding_path,
        workspace: &workspace,
        task: &task_path,
        evidence_file: &evidence_path,
        source_id: &input.source_id,
        evidence_format: "turtle",
        scope_id: &input.object.condition_scope,
        focus_iri: &input.object.object_id,
        focus_type_iri: &input.focus_type_iri,
        query_asset_id: &input.query_asset_id,
        shape_asset_id: &input.shape_asset_id,
    };
    let sources = [
        input.project.clone(),
        input.semantic_binding.clone(),
        input.task.clone(),
        input.evidence.clone(),
    ];
    let mut observe = |native: &oe_semantic::native::DecisionReviewRequest,
                       semantic: &semantica_contracts::SemanticPromotedReviewResult1,
                       sources: &[Vec<u8>]|
     -> Result<Value> {
        typed_promoted_observation(root, input, &required, native, semantic, sources)
    };
    let owner = oe_semantic::lifecycle::project_review_observed_at(
        root.path(),
        &request,
        None,
        &sources,
        Some(&mut observe),
    )?;
    typed_promoted_receipt(input, &workspace, &sources, &owner)
}

fn validate_promoted_request(
    input: &oe_contracts::TypedPromotedProjectReviewRequest1,
) -> Result<BTreeSet<String>> {
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("typed_project_contract:{e:?}"))?;
    ensure!(
        input.schema == "oev2.typed-promoted-project-review-request/1",
        "typed_project_schema"
    );
    let required: BTreeSet<_> = input.required_checks.iter().cloned().collect();
    ensure!(
        !required.is_empty() && required.len() == input.required_checks.len(),
        "typed_project_required_checks_invalid"
    );
    Ok(required)
}

fn typed_promoted_observation(
    root: &oe_core::SkillRoot,
    input: &oe_contracts::TypedPromotedProjectReviewRequest1,
    required: &BTreeSet<String>,
    native: &oe_semantic::native::DecisionReviewRequest,
    semantic: &semantica_contracts::SemanticPromotedReviewResult1,
    sources: &[Vec<u8>],
) -> Result<Value> {
    let project: Project = serde_json::from_value(oe_core::json::strict_json(&sources[0])?)?;
    validate(&project)?;
    ensure!(
        native.binding["project_id"] == project.project_id,
        "typed_project_owner_identity_mismatch"
    );
    ensure!(
        project.evidence.get(&input.source_id) == Some(&native.evidence_sha256)
            && native.evidence == sources[3],
        "typed_project_evidence_identity_mismatch"
    );
    let mut checks = promoted_observation_checks(input, semantic)?;
    // Preserve actual query/shape/focus even when this retained Turtle guard
    // cannot execute. A failed guard is unknown, never an absent focus.
    let bound = crate::port::bound_focus_context(
        &input.object,
        &native.evidence,
        std::slice::from_ref(&input.shape_asset_id),
        &checks,
    )
    .unwrap_or_else(|error| crate::CheckResult {
        id: "bound_focus_context".into(),
        execution: "failed".into(),
        conclusion: "unknown".into(),
        coverage: vec![],
        failures: vec![format!("{error:#}")],
        result: Value::Null,
    });
    let mut project_checks = vec![bound];
    project_checks.extend(current_checks(
        root,
        &project,
        &input.object,
        Some(&input.expected_context),
        &native.evidence,
        None,
    ));
    let project_required = project_checks.iter().map(|c| c.id.clone()).collect();
    let project_summary = crate::summarize_checks(&project_required, &project_checks)?;
    checks.extend(project_checks);
    // Required coverage includes every executed engineering obligation. A
    // caller cannot shorten its list to turn a missing obligation into pass.
    let mandatory: BTreeSet<_> = checks.iter().map(|c| c.id.clone()).collect();
    let omitted = mandatory.difference(&required).cloned().collect::<Vec<_>>();
    checks.push(crate::CheckResult {
            id: "required_project_review_coverage".into(), execution: "completed".into(),
            conclusion: if omitted.is_empty() { "pass" } else { "violated" }.into(),
            coverage: mandatory.into_iter().collect(),
            failures: omitted.iter().map(|id| format!("typed_project_required_check_omitted:{id}")).collect(),
            result: json!({"omitted":omitted,"rule":"all semantic, focus, current-context, model-link and actual engineering checks are mandatory"}),
        });
    let combined = crate::summarize_checks(&required, &checks)?;
    Ok(
        json!({"execution":combined.execution,"conclusion":combined.conclusion,
            "project_execution":project_summary.execution,"project_conclusion":project_summary.conclusion,
            "coverage":combined.coverage,"failures":combined.failures,"checks":checks,
            "engineering_adoption":"unknown"}),
    )
}

fn typed_promoted_receipt(
    input: &oe_contracts::TypedPromotedProjectReviewRequest1,
    workspace: &std::path::Path,
    sources: &[oe_contracts::ArtifactReference],
    owner: &Value,
) -> Result<oe_contracts::TypedPromotedProjectReviewReceipt1> {
    use oe_contracts::{
        ProjectCheckObservation1, PromotedRegistryIdentity1, TypedPromotedProjectReviewReceipt1,
    };
    let native = &owner["native_execution"]["result"];
    let semantic: semantica_contracts::SemanticPromotedReviewResult1 =
        serde_json::from_value(native["review_observation"].clone())?;
    let observation = &native["typed_project_observation"];
    let descriptor = &semantic.subject["descriptor"];
    let text = |value: &Value, key: &str| -> Result<String> {
        Ok(value[key]
            .as_str()
            .with_context(|| format!("typed_project_receipt_field:{key}"))?
            .into())
    };
    let raw_checks: Vec<crate::CheckResult> =
        serde_json::from_value(observation["checks"].clone())?;
    let checks = raw_checks
        .into_iter()
        .map(|c| ProjectCheckObservation1 {
            id: c.id,
            execution: c.execution,
            conclusion: c.conclusion,
            coverage: c.coverage,
            failures: c.failures,
            result: c.result,
        })
        .collect();
    let mut receipt = TypedPromotedProjectReviewReceipt1 {
        schema: "oev2.typed-promoted-project-review-receipt/1".into(),
        request_sha256: oe_core::sha256(oe_wire::json::canonical_json(&serde_json::to_value(
            input,
        )?)?),
        project: input.project.clone(),
        project_context: input.expected_context.clone(),
        object: input.object.clone(),
        registry: PromotedRegistryIdentity1 {
            registry_id: text(&semantic.subject, "registry_id")?,
            workspace: workspace.to_string_lossy().into_owned(),
            package_id: text(descriptor, "package_id")?,
            version: text(descriptor, "version")?,
            logical_package_sha256: text(descriptor, "package_sha256")?,
            manifest_sha256: text(descriptor, "manifest_sha256")?,
            delta_sha256: text(descriptor, "delta_sha256")?,
            execution_projection_sha256: text(&semantic.subject, "execution_projection_sha256")?,
        },
        semantic_review: semantic,
        checks,
        execution: text(observation, "execution")?,
        conclusion: text(observation, "conclusion")?,
        project_execution: text(observation, "project_execution")?,
        project_conclusion: text(observation, "project_conclusion")?,
        coverage: serde_json::from_value(observation["coverage"].clone())?,
        failures: serde_json::from_value(observation["failures"].clone())?,
        source_observation: json!({"owner":"original semantic lifecycle/registry lock and authority",
            "command_verdict":owner["command_verdict"],"task_sha256":owner["execution"]["task_sha256"],
            "sources":sources,"source_recheck":native["source_recheck"],"rust_execution":native["rust_execution"],
            "typed_project_recheck":native["typed_project_recheck"],"input_source_recheck":"matched",
            "profile":"Turtle/default graph; current declared engineering premises; adoption not evaluated"}),
        engineering_adoption: "unknown".into(),
        fact_admitted: false,
        report_sha256: String::new(),
    };
    let mut content = serde_json::to_value(&receipt)?;
    content
        .as_object_mut()
        .context("typed_project_receipt_object")?
        .remove("report_sha256");
    receipt.report_sha256 = oe_core::sha256(oe_wire::json::canonical_json(&content)?);
    receipt
        .validate()
        .map_err(|e| anyhow::anyhow!("typed_project_receipt_contract:{e:?}"))?;
    Ok(receipt)
}

/// Compose the same checks with a borrowed original-owner publisher. The receipt
/// is created from this call's actual observation; no serialized receipt can
/// restore the lock or execute a publisher after this function returns.
pub fn review_promoted_with_commit(
    root: &oe_core::SkillRoot,
    input: &oe_contracts::TypedPromotedProjectReviewRequest1,
    additional_sources: &[oe_contracts::ArtifactReference],
    precommit: &mut dyn FnMut(&oe_contracts::TypedPromotedProjectReviewReceipt1) -> Result<()>,
    publish: impl FnOnce(&oe_contracts::TypedPromotedProjectReviewReceipt1) -> Result<Value>,
) -> Result<Value> {
    let required = validate_promoted_request(input)?;
    let binding = root.private_path(std::path::Path::new(&input.semantic_binding.path))?;
    let task = root.private_path(std::path::Path::new(&input.task.path))?;
    let evidence = root.private_path(std::path::Path::new(&input.evidence.path))?;
    let workspace = root.private_path(std::path::Path::new(&input.workspace))?;
    let request = oe_semantic::lifecycle::ProjectReviewRequest {
        binding: &binding,
        workspace: &workspace,
        task: &task,
        evidence_file: &evidence,
        source_id: &input.source_id,
        evidence_format: "turtle",
        scope_id: &input.object.condition_scope,
        focus_iri: &input.object.object_id,
        focus_type_iri: &input.focus_type_iri,
        query_asset_id: &input.query_asset_id,
        shape_asset_id: &input.shape_asset_id,
    };
    let base_sources = [
        input.project.clone(),
        input.semantic_binding.clone(),
        input.task.clone(),
        input.evidence.clone(),
    ];
    let mut sources = base_sources.to_vec();
    sources.extend_from_slice(additional_sources);
    let mut observer = |native: &oe_semantic::native::DecisionReviewRequest,
                        semantic: &semantica_contracts::SemanticPromotedReviewResult1,
                        bytes: &[Vec<u8>]| {
        typed_promoted_observation(root, input, &required, native, semantic, bytes)
    };
    let owner = oe_semantic::lifecycle::project_review_committed_at(
        root.path(),
        &request,
        None,
        &sources,
        &mut observer,
        &mut |owner| {
            precommit(&typed_promoted_receipt(
                input,
                &workspace,
                &base_sources,
                owner,
            )?)
        },
        |owner| {
            publish(&typed_promoted_receipt(
                input,
                &workspace,
                &base_sources,
                owner,
            )?)
        },
    )?;
    Ok(owner["native_execution"]["result"]["commit_result"].clone())
}

/// Project the already executed semantic observations without querying again.
fn promoted_observation_checks(
    input: &oe_contracts::TypedPromotedProjectReviewRequest1,
    semantic: &semantica_contracts::SemanticPromotedReviewResult1,
) -> Result<Vec<crate::CheckResult>> {
    // The input snapshot was actually observed by M before loading package
    // ontology. A missing/invalid count is unknown, never an invented zero.
    let (execution, conclusion, failure) = match semantic.evidence["input_quad_count"].as_u64() {
        Some(0) => ("completed", "violated", Some("promoted_input_empty_abox")),
        Some(_) => ("completed", "pass", None),
        None => (
            "failed",
            "unknown",
            Some("promoted_input_count_missing_or_invalid"),
        ),
    };
    let mut checks = vec![crate::CheckResult {
        id: "promoted_input".into(),
        execution: execution.into(),
        conclusion: conclusion.into(),
        coverage: vec!["actual evidence-origin RDF dataset is nonempty".into()],
        failures: failure.into_iter().map(str::to_owned).collect(),
        result: json!({"input_quad_count":semantic.evidence["input_quad_count"],
            "input_dataset_sha256":semantic.evidence["input_dataset_sha256"],
            "origin":"actual promoted review input snapshot before package load"}),
    }];
    for (id, observation, interpreted, pass) in [
        (
            &input.query_asset_id,
            &semantic.query_observation,
            semantic.interpretation["status"] == "supported",
            semantic.query_observation["result"]["rows"]
                .as_array()
                .is_some_and(Vec::is_empty),
        ),
        (
            &input.shape_asset_id,
            &semantic.shape_observation,
            semantic.shape_observation["result"]["conforms"].is_boolean(),
            semantic.shape_observation["result"]["conforms"] == true,
        ),
    ] {
        let completed = observation["execution"] == "completed";
        let conclusion = if completed && interpreted {
            if pass { "pass" } else { "violated" }
        } else {
            "unknown"
        };
        checks.push(crate::CheckResult {
            id: id.clone(),
            execution: if completed { "completed" } else { "failed" }.into(),
            conclusion: conclusion.into(),
            coverage: vec![id.clone()],
            failures: if conclusion == "unknown" {
                vec![format!("promoted_observation_unknown:{id}")]
            } else {
                vec![]
            },
            result: observation["result"].clone(),
        });
    }
    let focus = &semantic.focus_observation;
    let focus_completed = focus["execution"] == "completed";
    let focus_pass = focus["result"]["focus_present"] == true
        && focus["result"]["focus_type_present"] == true
        && focus["result"]["focus_instance_count"] == 1;
    checks.push(crate::CheckResult {
        id: "promoted_focus".into(),
        execution: if focus_completed {
            "completed"
        } else {
            "failed"
        }
        .into(),
        conclusion: if focus_completed {
            if focus_pass { "pass" } else { "violated" }
        } else {
            "unknown"
        }
        .into(),
        coverage: vec![input.object.object_id.clone(), input.focus_type_iri.clone()],
        failures: if focus_completed {
            vec![]
        } else {
            vec!["promoted_focus_unavailable".into()]
        },
        result: focus.clone(),
    });
    checks.push(crate::CheckResult {id:"promoted_review".into(),
        execution:if semantic.execution == "completed" {"completed"} else {"failed"}.into(),
        conclusion:if semantic.execution == "completed" {semantic.conclusion.clone()} else {"unknown".into()},
        coverage:semantic.coverage.clone(), failures:semantic.failures.iter().map(Value::to_string).collect(),
        result:json!({"review_v1":semantic.review_v1,"focus_observation":semantic.focus_observation,
            "interpretation":semantic.interpretation,"dataset_observation":semantic.dataset_observation})});
    Ok(checks)
}

pub struct TypedPromotedProjectReviewExecutor<'a> {
    pub root: &'a oe_core::SkillRoot,
}
impl oe_contracts::TypedPromotedProjectReviewPort1 for TypedPromotedProjectReviewExecutor<'_> {
    fn review(
        &self,
        input: &oe_contracts::TypedPromotedProjectReviewRequest1,
    ) -> std::result::Result<
        oe_contracts::TypedPromotedProjectReviewReceipt1,
        oe_contracts::FailureRecord,
    > {
        review_promoted_current(self.root, input).map_err(|error| oe_contracts::FailureRecord {
            code: oe_contracts::ErrorCode::InvalidContract,
            phase: "typed_promoted_project_review".into(),
            retryable: false,
            partial_result: Some(oe_semantic::lifecycle::error_response("review", &error)),
        })
    }
}
