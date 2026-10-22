//! Explicit promoted adoption on the original engineering adoption owner.
//! A typed review is evidence; only the existing private POLICY and independent
//! approval permit this owner to publish one scoped Project output.
#[cfg(test)]
use semantica_runtime::sha256 as private_fixture_sha256;
#[cfg(test)]
#[path = "../../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use super::{POLICY, artifacts, read_authority_policy};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, EngineeringPromotedAdoptionApproval2 as Approval,
    EngineeringPromotedAdoptionAuthority2 as Authority,
    EngineeringPromotedAdoptionRequest2 as Request,
    EngineeringPromotedAdoptionResult2 as AdoptionResult,
    EngineeringPromotedAdoptionVerifyRequest2 as VerifyRequest,
    EngineeringPromotedAdoptionVerifyResult2 as VerifyResult, FailureRecord, ProjectReviewContext,
    TypedPromotedProjectReviewReceipt1 as Review,
};
use oe_core::{SkillRoot, sha256};
use oe_kernel::project::Project;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const REQUEST: &str = "oev2.engineering-promoted-adoption-request/2";
const RESULT: &str = "oev2.engineering-promoted-adoption-result/2";
const RECORD: &str = "oev2.engineering-promoted-adoption-record/2";
const AUTHORITY: &str = "oev2.engineering-adoption-authority/2";
const APPROVAL: &str = "oev2.engineering-adoption-approval/2";
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct RetainedFailure(Value);
impl std::fmt::Display for RetainedFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "promoted_adoption2_failed: {}", self.0["reason"])
    }
}
impl std::error::Error for RetainedFailure {}
fn retain(error: anyhow::Error, publication: &Value) -> anyhow::Error {
    RetainedFailure(json!({"execution":"failed","conclusion":"unknown",
        "engineering_adoption":"not_established","reason":format!("{error:#}"),
        "native_observation":oe_semantic::lifecycle::error_response("review", &error),
        "publication":publication}))
    .into()
}
fn failure(error: anyhow::Error, phase: &str) -> FailureRecord {
    let partial = error.downcast_ref::<RetainedFailure>().map(|e| e.0.clone())
        .unwrap_or_else(|| json!({"execution":"failed","conclusion":"unknown","engineering_adoption":"not_established","reason":format!("{error:#}"),"native_observation":oe_semantic::lifecycle::error_response("review", &error)}));
    FailureRecord {
        code: oe_contracts::ErrorCode::GovernanceDenied,
        phase: phase.into(),
        retryable: false,
        partial_result: Some(partial),
    }
}
fn same<T: serde::Serialize>(a: &T, b: &T) -> Result<bool> {
    Ok(serde_json::to_value(a)? == serde_json::to_value(b)?)
}
fn set(values: &[String]) -> Result<BTreeSet<String>> {
    let selected = values.iter().cloned().collect::<BTreeSet<_>>();
    ensure!(
        !selected.is_empty()
            && selected.len() == values.len()
            && values.iter().all(|v| !v.trim().is_empty()),
        "adoption2_required_set_invalid"
    );
    Ok(selected)
}
fn decision(project: &Project, id: &str) -> Result<usize> {
    let selected = project
        .decisions
        .iter()
        .enumerate()
        .filter(|(_, d)| d.id == id)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 1,
        "adoption2_decision_missing_or_ambiguous"
    );
    Ok(selected[0])
}
struct Prepared {
    project: Project,
    decision: usize,
    context: ProjectReviewContext,
    authority: Authority,
    approval: Approval,
    policy_sha256: String,
}
impl Prepared {
    fn load(root: &SkillRoot, input: &Request) -> Result<Self> {
        input
            .validate()
            .map_err(|e| anyhow::anyhow!("adoption2_request_invalid:{e:?}"))?;
        ensure!(input.schema == REQUEST, "adoption2_schema_unsupported");
        let (policy, policy_sha256) = read_authority_policy(root)?;
        ensure!(
            policy["schema"] == AUTHORITY,
            "adoption2_explicit_authority_version_required"
        );
        let authority: Authority = serde_json::from_value(policy)?;
        authority
            .validate()
            .map_err(|e| anyhow::anyhow!("adoption2_authority_invalid:{e:?}"))?;
        ensure!(
            authority.decision_id == input.decision_id
                && authority.claim_scope == input.claim_scope
                && input.claim_scope == input.review.object.condition_scope,
            "adoption2_authority_scope_mismatch"
        );
        ensure!(
            authority
                .actor_scopes
                .get(&input.actor)
                .is_some_and(|scopes| scopes.contains(&input.claim_scope)),
            "adoption2_actor_not_authorized"
        );
        let required = set(&authority.required_checks)?;
        ensure!(
            required == set(&input.review.required_checks)?,
            "adoption2_required_review_changed"
        );
        ensure!(
            !authority.required_sources.is_empty()
                && !authority.approvers.is_empty()
                && !authority.approved_approvals.is_empty(),
            "adoption2_authority_requirements_empty"
        );
        let project: Project =
            serde_json::from_value(artifacts::json(root, &input.review.project)?)?;
        oe_kernel::project::validate(&project)?;
        ensure!(
            project.project_id == authority.project_id,
            "adoption2_project_scope_mismatch"
        );
        let context = oe_kernel::project::context(&project, &input.review.object)?;
        ensure!(
            same(&context, &input.review.expected_context)?,
            "adoption2_current_context_changed"
        );
        let basis = oe_kernel::engineering::basis(&project, &input.review.object)?
            .context("adoption2_current_engineering_basis_required")?;
        ensure!(
            !basis.assertions.is_empty() && !basis.acceptance.is_empty(),
            "adoption2_engineering_obligations_empty"
        );
        ensure!(
            basis
                .facts
                .iter()
                .flat_map(|f| &f.source_refs)
                .chain(basis.assertions.iter().flat_map(|a| &a.source_refs))
                .all(|id| authority.required_sources.contains_key(id)),
            "adoption2_engineering_sources_not_bound"
        );
        let decision = decision(&project, &input.decision_id)?;
        let d = &project.decisions[decision];
        let o = &input.review.object;
        ensure!(
            d.object_id == o.object_id
                && d.object_revision == o.revision
                && d.condition_scope == o.condition_scope
                && d.source_sha256 == o.source_sha256,
            "adoption2_decision_binding_mismatch"
        );
        ensure!(
            authority
                .approved_approvals
                .contains(&input.approval.sha256),
            "adoption2_approval_not_authority_bound"
        );
        let approval: Approval = serde_json::from_value(artifacts::json(root, &input.approval)?)?;
        approval
            .validate()
            .map_err(|e| anyhow::anyhow!("adoption2_approval_invalid:{e:?}"))?;
        ensure!(
            approval.schema == APPROVAL
                && approval.decision_id == input.decision_id
                && approval.claim_scope == input.claim_scope
                && same(&approval.project_context, &context)?,
            "adoption2_approval_not_current"
        );
        ensure!(
            approval.approver != input.actor && authority.approvers.contains(&approval.approver),
            "adoption2_independent_approval_required"
        );
        ensure!(
            same(&approval.registry, &authority.approved_registry)?
                && set(&approval.required_checks)? == required,
            "adoption2_approval_registry_or_coverage_changed"
        );
        let sources = authority
            .required_sources
            .iter()
            .map(|(id, source)| (id.clone(), source.sha256.clone()))
            .collect::<BTreeMap<_, _>>();
        ensure!(
            approval.sources == sources
                && authority
                    .required_sources
                    .iter()
                    .all(|(id, source)| project.evidence.get(id) == Some(&source.sha256)),
            "adoption2_source_obligations_missing"
        );
        let prepared = Self {
            project,
            decision,
            context,
            authority,
            approval,
            policy_sha256,
        };
        prepared.recheck(root, input)?;
        Ok(prepared)
    }
    fn extra_sources(&self, input: &Request) -> Vec<ArtifactReference> {
        let mut sources = vec![
            ArtifactReference {
                path: POLICY.into(),
                sha256: self.policy_sha256.clone(),
            },
            input.approval.clone(),
        ];
        sources.extend(self.authority.required_sources.values().cloned());
        let mut seen = BTreeSet::new();
        sources.retain(|r| seen.insert((r.path.clone(), r.sha256.clone())));
        sources
    }
    fn review(&self, input: &Request, review: &Review) -> Result<()> {
        review
            .validate()
            .map_err(|e| anyhow::anyhow!("adoption2_review_invalid:{e:?}"))?;
        ensure!(
            review.request_sha256
                == sha256(oe_semantic::canonical_json(&serde_json::to_value(
                    &input.review
                )?)?),
            "adoption2_review_request_changed"
        );
        let mut content = serde_json::to_value(review)?;
        content
            .as_object_mut()
            .context("adoption2_review_object")?
            .remove("report_sha256");
        ensure!(
            review.report_sha256 == sha256(oe_semantic::canonical_json(&content)?),
            "adoption2_review_digest_changed"
        );
        ensure!(
            same(&review.registry, &self.authority.approved_registry)?,
            "adoption2_registry_not_authority_bound"
        );
        ensure!(
            same(&review.project_context, &self.context)?
                && same(&review.project, &input.review.project)?
                && same(&review.object, &input.review.object)?,
            "adoption2_review_context_changed"
        );
        let required = set(&self.authority.required_checks)?;
        let mut seen = BTreeSet::new();
        ensure!(
            review.execution == "completed"
                && review.conclusion == "pass"
                && review.project_execution == "completed"
                && review.project_conclusion == "pass"
                && review.checks.iter().all(|c| seen.insert(c.id.clone())
                    && c.execution == "completed"
                    && c.conclusion == "pass"
                    && c.failures.is_empty())
                && required.iter().all(|id| seen.contains(id))
                && seen
                    .iter()
                    .all(|id| id == "required_project_review_coverage" || required.contains(id)),
            "adoption2_required_review_not_passed"
        );
        ensure!(
            review.semantic_review.execution == "completed"
                && review.semantic_review.conclusion == "pass"
                && review.semantic_review.review_v1["status"] == "clear"
                && review.engineering_adoption == "unknown"
                && !review.fact_admitted,
            "adoption2_semantic_observation_not_clear"
        );
        Ok(())
    }
    fn recheck(&self, root: &SkillRoot, input: &Request) -> Result<()> {
        ensure!(
            read_authority_policy(root)?.1 == self.policy_sha256,
            "adoption2_policy_changed"
        );
        for source in [
            &input.review.project,
            &input.review.semantic_binding,
            &input.review.task,
            &input.review.evidence,
            &input.approval,
        ] {
            artifacts::read(root, source)?;
        }
        for source in self.authority.required_sources.values() {
            artifacts::read(root, source)?;
        }
        Ok(())
    }
}
fn encoded(value: &impl serde::Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn future_reference(root: &SkillRoot, path: &Path, bytes: &[u8]) -> Result<ArtifactReference> {
    Ok(ArtifactReference {
        path: oe_core::portable_source_path(root.path(), path)?,
        sha256: sha256(bytes),
    })
}
struct Publication {
    stage: PathBuf,
    built: PathBuf,
    output: PathBuf,
    files: BTreeMap<String, Vec<u8>>,
    result: Value,
}
impl Publication {
    fn prepare(
        root: &SkillRoot,
        input: &Request,
        prepared: &Prepared,
        review: &Review,
        output: &Path,
        diagnostic: &RefCell<Value>,
    ) -> Result<Self> {
        let output = root.private_path(output)?;
        let registry = root.private_path(Path::new(&input.review.workspace))?;
        ensure!(
            !output.starts_with(&registry),
            "adoption2_output_inside_registry"
        );
        ensure!(!output.try_exists()?, "adoption2_output_exists");
        let parent = output.parent().context("adoption2_output_parent_missing")?;
        oe_core::ensure_directory(parent)?;
        let stage = parent.join(format!(
            ".adoption2-prepared-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        *diagnostic.borrow_mut() = json!({"execution":"not_completed","publication":"not_performed","publisher_invoked":false,"stage":stage,"target":output,"stage_creation":"attempted"});
        oe_core::create_new_private_dir(&stage)?;
        let built = stage.join("payload");
        oe_core::create_new_private_dir(&built)?;
        oe_core::write_new(
            &stage.join("intent.json"),
            &json!({"schema":"oev2.engineering-adoption-preparation/2","execution":"not_completed","publication":"not_performed","target":output,"decision_id":input.decision_id}),
        )?;
        #[cfg(test)]
        tests::hook("before_prepare_files", &stage)?;
        let review_bytes = encoded(review)?;
        let review_ref = future_reference(root, &output.join("review.json"), &review_bytes)?;
        let mut project = prepared.project.clone();
        project.decisions[prepared.decision].conclusion = "supported".into();
        project.decisions[prepared.decision].adopted = true;
        project.decisions[prepared.decision].basis =
            vec![review_ref.sha256.clone(), input.approval.sha256.clone()];
        oe_kernel::project::validate(&project)?;
        let project_bytes = encoded(&project)?;
        let project_ref = future_reference(root, &output.join("project.json"), &project_bytes)?;
        let record = json!({"schema":RECORD,"execution":"completed","decision_id":input.decision_id,"claim_scope":input.claim_scope,
            "actor":input.actor,"approver":prepared.approval.approver,"request":input,"authority_sha256":prepared.policy_sha256,
            "project_context":prepared.context,"registry":review.registry,"review":review_ref,"project":project_ref,
            "engine_id":oe_kernel::ENGINE_ID,"engine_version":oe_kernel::ENGINE_VERSION,"executable_sha256":oe_kernel::executable_digest()?,
            "publication":{"record_path":oe_core::portable_source_path(root.path(), &output.join("adoption.json"))?,"target":oe_core::portable_source_path(root.path(), &output)?},
            "adoption":"scope_bound","broader_adoption":"not_established","authority_boundary":"original locally owned private POLICY and independent scope approval; not organizational cryptographic authentication"});
        let record_bytes = encoded(&record)?;
        let record_ref = future_reference(root, &output.join("adoption.json"), &record_bytes)?;
        let result = AdoptionResult {
            schema: RESULT.into(),
            execution: "completed".into(),
            decision_id: input.decision_id.clone(),
            claim_scope: input.claim_scope.clone(),
            project: project_ref,
            review: review_ref,
            record: record_ref,
            publication: "published".into(),
        };
        result
            .validate()
            .map_err(|e| anyhow::anyhow!("adoption2_result_invalid:{e:?}"))?;
        let result = serde_json::to_value(result)?;
        // There is no completed adoption record on disk before final guards.
        oe_core::write_new_bytes(&built.join("review.json"), &review_bytes)?;
        oe_core::write_new_bytes(&built.join("project.json"), &project_bytes)?;
        Ok(Self {
            stage,
            built,
            output,
            files: BTreeMap::from([
                ("review.json".into(), review_bytes),
                ("project.json".into(), project_bytes),
                ("adoption.json".into(), record_bytes),
            ]),
            result,
        })
    }
    fn publish(self, diagnostic: &RefCell<Value>) -> Result<Value> {
        *diagnostic.borrow_mut() = json!({"execution":"not_completed","publication":"not_performed","publisher_invoked":true,"stage":self.stage,"target":self.output});
        // This write is reached only after every owner/runtime/input guard.
        #[cfg(test)]
        tests::hook("before_write", &self.stage)?;
        oe_core::write_new_bytes(
            &self.built.join("adoption.json"),
            &self.files["adoption.json"],
        )?;
        for (name, expected) in &self.files {
            ensure!(
                oe_core::read_regular_bytes(&self.built.join(name), 16 * 1024 * 1024)? == *expected,
                "adoption2_prepared_output_changed"
            );
        }
        let names = std::fs::read_dir(&self.built)?
            .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        ensure!(
            names == self.files.keys().cloned().collect(),
            "adoption2_prepared_inventory_changed"
        );
        // Consumers bind exact final paths and reject this noncompleted stage.
        #[cfg(test)]
        tests::hook("before_rename", &self.stage)?;
        *diagnostic.borrow_mut() = json!({"execution":"not_completed","publication":"unknown","publisher_invoked":true,"stage":self.stage,"target":self.output});
        oe_core::rename_directory_exclusive(&self.built, &self.output)?;
        #[cfg(test)]
        tests::hook("after_rename", &self.stage)?;
        std::fs::File::open(self.output.parent().context("adoption2_parent_missing")?)?
            .sync_all()?;
        *diagnostic.borrow_mut() = json!({"execution":"completed","publication":"published","publisher_invoked":true,"stage":self.stage,"target":self.output});
        Ok(self.result)
    }
}

pub fn adopt(root: &SkillRoot, input: &Request, output: &Path) -> Result<AdoptionResult> {
    let diagnostic = RefCell::new(
        json!({"execution":"not_completed","publication":"not_performed","publisher_invoked":false}),
    );
    let attempt = (|| -> Result<AdoptionResult> {
        let prepared = Prepared::load(root, input)?;
        let publication = RefCell::new(None);
        let value = oe_kernel::project::review_promoted_with_commit(
            root,
            &input.review,
            &prepared.extra_sources(input),
            &mut |review| {
                prepared.review(input, review)?;
                let staged =
                    Publication::prepare(root, input, &prepared, review, output, &diagnostic)?;
                *diagnostic.borrow_mut() = json!({"execution":"not_completed","publication":"not_performed","publisher_invoked":false,"stage":staged.stage,"target":staged.output});
                #[cfg(test)]
                tests::hook("after_prepare", &staged.stage)?;
                prepared.recheck(root, input)?;
                #[cfg(test)]
                tests::hook("after_owner_guards", &staged.stage)?;
                *publication.borrow_mut() = Some(staged);
                Ok(())
            },
            |_| {
                publication
                    .borrow_mut()
                    .take()
                    .context("adoption2_prepared_publication_missing")?
                    .publish(&diagnostic)
            },
        )?;
        let result: AdoptionResult = serde_json::from_value(value)?;
        Ok(result)
    })();
    attempt.map_err(|error| retain(error, &diagnostic.borrow()))
}

pub fn verify(root: &SkillRoot, input: &VerifyRequest) -> Result<VerifyResult> {
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("adoption2_verify_request_invalid:{e:?}"))?;
    ensure!(
        input.schema == "oev2.engineering-promoted-adoption-verify-request/2",
        "adoption2_verify_schema"
    );
    let record = artifacts::json(root, &input.record)?;
    ensure!(
        record["schema"] == RECORD
            && record["execution"] == "completed"
            && record["publication"]["record_path"] == input.record.path,
        "adoption2_record_not_published_at_bound_path"
    );
    ensure!(
        record["executable_sha256"] == oe_kernel::executable_digest()?,
        "adoption2_executor_changed_requires_revalidation"
    );
    let original: Request = serde_json::from_value(record["request"].clone())?;
    let mut request = original.clone();
    request.review.project = input.current_project.clone();
    let prepared = Prepared::load(root, &request)?;
    ensure!(
        record["authority_sha256"] == prepared.policy_sha256
            && record["project_context"] == serde_json::to_value(&prepared.context)?
            && record["decision_id"] == request.decision_id
            && record["claim_scope"] == request.claim_scope
            && record["actor"] == request.actor
            && record["approver"] == prepared.approval.approver,
        "adoption2_record_context_changed"
    );
    let review_ref: ArtifactReference = serde_json::from_value(record["review"].clone())?;
    let saved: Review = serde_json::from_value(artifacts::json(root, &review_ref)?)?;
    prepared.review(&original, &saved)?;
    ensure!(
        record["registry"] == serde_json::to_value(&saved.registry)?,
        "adoption2_record_registry_changed"
    );
    let selected = &prepared.project.decisions[prepared.decision];
    ensure!(
        selected.adopted
            && selected.conclusion == "supported"
            && selected.basis == vec![review_ref.sha256.clone(), request.approval.sha256.clone()],
        "adoption2_current_decision_no_longer_adopted"
    );
    let published_ref: ArtifactReference = serde_json::from_value(record["project"].clone())?;
    let published: Project = serde_json::from_value(artifacts::json(root, &published_ref)?)?;
    oe_kernel::project::validate(&published)?;
    let mut original_project: Project =
        serde_json::from_value(artifacts::json(root, &original.review.project)?)?;
    let original_decision = decision(&original_project, &original.decision_id)?;
    original_project.decisions[original_decision].conclusion = "supported".into();
    original_project.decisions[original_decision].adopted = true;
    original_project.decisions[original_decision].basis =
        vec![review_ref.sha256.clone(), original.approval.sha256.clone()];
    ensure!(
        same(&published, &original_project)?,
        "adoption2_published_output_changed"
    );
    // A no-write final observer uses the same held-lock guards. It cannot publish
    // and returns only this call's actual current review observation.
    let mut sources = prepared.extra_sources(&request);
    sources.extend([
        input.record.clone(),
        review_ref,
        published_ref,
        original.review.project.clone(),
    ]);
    let value = oe_kernel::project::review_promoted_with_commit(
        root,
        &request.review,
        &sources,
        &mut |review| {
            prepared.review(&request, review)?;
            prepared.recheck(root, &request)?;
            #[cfg(test)]
            tests::hook("verify_precommit", root.path())?;
            Ok(())
        },
        |review| Ok(serde_json::to_value(review)?),
    )?;
    let review: Review = serde_json::from_value(value)?;
    Ok(VerifyResult {
        schema: "oev2.engineering-promoted-adoption-verify-result/2".into(),
        execution: "completed".into(),
        status: "current_scope_bound".into(),
        decision_id: request.decision_id,
        claim_scope: request.claim_scope,
        current_project: input.current_project.clone(),
        current_context: prepared.context,
        registry: review.registry.clone(),
        review,
        record: input.record.clone(),
        broader_adoption: "not_established".into(),
    })
}

pub struct PromotedAdoptionExecutor2<'a> {
    pub root: &'a SkillRoot,
    pub output: &'a Path,
}
impl oe_contracts::EngineeringPromotedAdoptionPort2 for PromotedAdoptionExecutor2<'_> {
    fn adopt(&self, input: &Request) -> std::result::Result<AdoptionResult, FailureRecord> {
        adopt(self.root, input, self.output)
            .map_err(|e| failure(e, "engineering_promoted_adoption2"))
    }
}
pub struct PromotedAdoptionVerifyExecutor2<'a> {
    pub root: &'a SkillRoot,
}
impl oe_contracts::EngineeringPromotedAdoptionVerifyPort2 for PromotedAdoptionVerifyExecutor2<'_> {
    fn verify(&self, input: &VerifyRequest) -> std::result::Result<VerifyResult, FailureRecord> {
        verify(self.root, input).map_err(|e| failure(e, "engineering_promoted_adoption_verify2"))
    }
}

#[cfg(test)]
mod tests {
    // Real fixture mechanically reused from frozen T; controlled owned source root.
    use super::*;
    use anyhow::{Result, ensure};
    use base64::Engine;
    use oe_core::SkillRoot;
    use oe_semantic::{contracts, refinery_runtime::RustRuntime, refinery_storage::FileStore};
    use semantica_runtime::{
        books::SourceProvider, refinery::RefineryEngine, refinery_contracts as wire,
    };
    use serde_json::{Value, json};
    use std::{fs, path::PathBuf};
    const WHEN: &str = "2026-08-19T00:00:00Z";
    const REGISTRY: &str = "registry-original-full12";

    const SHAPES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:review:Shape> a sh:NodeShape; sh:targetClass <urn:review:Record>; sh:property [ sh:path <urn:review:name>; sh:minCount 1 ] .";

    struct NoExternalInput;
    impl SourceProvider for NoExternalInput {
        fn read(&self, _: &str) -> Result<Vec<u8>> {
            anyhow::bail!("unexpected external fixture read")
        }
    }
    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
    }

    fn add_asset(
        delta: &mut Value,
        projection: &mut Value,
        category: &str,
        id: &str,
        content: &str,
        role: &str,
        format: &str,
        kind: &str,
    ) {
        delta[category].as_array_mut().unwrap().push(json!({"category":category,"asset_id":id,
        "operation":"add","media_type":if category=="queries" {"application/sparql-query"} else {"text/turtle"},
        "sha256":semantica_runtime::sha256(content),
        "content_base64":base64::engine::general_purpose::STANDARD.encode(content),
        "replaces_sha256":null,"role":null,"case_kind":null}));
        projection["assets"][id] =
            json!({"role":role,"format":format,"kind":kind,"load_into_dataset":false});
    }

    fn plan() -> Result<Value> {
        let plan_fixture: &str =
            &private_test_fixtures::text("original/full12/ORIGINAL-FULL12-FIXED-PLAN.json");
        ensure!(
            semantica_runtime::sha256(plan_fixture)
                == "e364a710b4921a17c27360e325e21c9a26a931c45c6c49ccaf3cfd2d0e624f41",
            "original plan changed"
        );
        let mut p: Value = serde_json::from_str(plan_fixture)?;
        let delta = &mut p["factory_delta"];
        let mut projection: Value =
            serde_json::from_slice(&wire::asset_content(&delta["contract"][0])?.unwrap())?;
        for (id, query) in [
            (
                "review-query",
                "SELECT ?finding WHERE { ?s <urn:review:bad> ?finding }",
            ),
            (
                "ordinary-query",
                "SELECT ?name WHERE { ?s <urn:review:name> ?name }",
            ),
            ("ask-query", "ASK { ?s <urn:review:name> ?name }"),
            (
                "unbound-query",
                "SELECT ?finding WHERE { VALUES ?other { 1 } }",
            ),
            (
                "empty-finding-query",
                "SELECT ?finding WHERE { VALUES ?finding { \" \" } }",
            ),
        ] {
            add_asset(
                delta,
                &mut projection,
                "queries",
                id,
                query,
                "sparql",
                "sparql",
                "text",
            );
        }
        add_asset(
            delta,
            &mut projection,
            "shapes",
            "review-shape",
            SHAPES,
            "shapes",
            "turtle",
            "rdf",
        );
        add_asset(
            delta,
            &mut projection,
            "shapes",
            "deactivated-shape",
            "@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:review:Disabled> a sh:NodeShape; sh:targetClass <urn:review:Record>; sh:deactivated true; sh:property [ sh:path <urn:review:missing>; sh:minCount 1 ] .",
            "shapes",
            "turtle",
            "rdf",
        );
        let bytes = serde_json::to_vec(&projection)?;
        delta["contract"][0]["sha256"] = json!(semantica_runtime::sha256(&bytes));
        delta["contract"][0]["content_base64"] =
            json!(base64::engine::general_purpose::STANDARD.encode(&bytes));
        delta.as_object_mut().unwrap().remove("delta_sha256");
        p["factory_delta"] = wire::delta(delta)?;
        let delta_sha = p["factory_delta"]["delta_sha256"]
            .as_str()
            .unwrap()
            .to_owned();
        for (action, context) in p["contexts"].as_object_mut().unwrap() {
            *context = wire::transition_context(
                action,
                &delta_sha,
                &context["envelope"],
                &context["binding"],
            )?;
        }
        for key in ["commit_authorization", "promote_authorization"] {
            p[key]["delta_sha256"] = p["factory_delta"]["delta_sha256"].clone();
            p[key]
                .as_object_mut()
                .unwrap()
                .remove("authorization_sha256");
            p[key] = wire::authorization(&p[key])?;
        }
        Ok(p)
    }

    fn promote(store: &FileStore, source: &Value) -> Result<Value> {
        let p = plan()?;
        let e = RefineryEngine::new(store, REGISTRY)?;
        let delta = &p["factory_delta"];
        let digest = delta["delta_sha256"].as_str().unwrap();
        let scenarios = p["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let reference = e.engage_current(
            &NoExternalInput,
            &p["reference_execution_manifest"],
            &scenarios,
            &p["task"],
            &p["binding"],
            &json!({"status":"candidate","rationale":delta["rationale"],"delta_sha256":digest}),
            source,
            WHEN,
        )?;
        e.propose_current(
            &reference,
            delta,
            &p["contexts"]["proposed"]["envelope"],
            WHEN,
        )?;
        e.commit_current(
            &reference,
            digest,
            &p["contexts"]["committed"]["envelope"],
            &p["commit_authorization"],
            WHEN,
        )?;
        let executed =
            e.execute_current(digest, &p["contexts"]["execute_candidate"], source, WHEN)?;
        let regression = e.derive_gate(
            digest,
            "regression",
            executed.observation()["suite_sha256"].as_str().unwrap(),
            &p["contexts"]["derive_regression_gate"],
            WHEN,
        )?;
        e.verify_candidate_current(
            &executed,
            Some(&regression),
            Some(&p["contexts"]["regression_passed"]),
            &p["contexts"]["derive_release_gate"],
            &p["contexts"]["release_complete"],
            WHEN,
        )?;
        e.promote_current(
            &executed,
            &p["promote_authorization"],
            &p["contexts"]["promoted"],
            WHEN,
        )
    }

    struct Fixture {
        root: SkillRoot,
        _run: tempfile::TempDir,
        project: oe_kernel::project::Project,
        binding: Value,
        task: Value,
        evidence: String,
        input: oe_contracts::TypedPromotedProjectReviewRequest1,
        descriptor: Value,
    }
    impl Fixture {
        fn new(focus: &str) -> Result<Self> {
            use std::collections::BTreeMap;
            fs::create_dir_all(root().join("var"))?;
            let run = tempfile::Builder::new()
                .prefix("adoption2-owned-")
                .tempdir_in(root().join("var"))?;
            fs::write(
                run.path().join("SKILL.md"),
                "controlled adoption2 source fixture",
            )?;
            for &(name, bytes) in oe_semantic::books::rust::source_execution::IMPLEMENTATION_SOURCES
            {
                let path = run.path().join(name);
                fs::create_dir_all(path.parent().unwrap())?;
                oe_core::write_new_bytes(&path, bytes)?;
            }
            let root = SkillRoot::new(run.path())?;
            fs::create_dir_all(root.path().join("var/inputs"))?;
            let workspace = root.path().join("var/registry");
            let runtime = RustRuntime::new(root.path())?;
            let store = FileStore::open(&root, &workspace, REGISTRY, WHEN, true)?;
            let descriptor = promote(&store, &runtime.source()?)?;
            drop(store);
            let object = oe_contracts::ObjectIdentity {
                object_id: focus.into(),
                revision: "r1".into(),
                condition_scope: "scope:fluid".into(),
                source_sha256: oe_core::sha256(b"independent declared fluid source"),
            };
            let basis = json!({"schema":"oev2.engineering-basis/1","object_id":focus,"object_revision":"r1",
            "purpose":"declared fluid applicability","acceptance":["declared fluid in source range"],
            "facts":[{"property":"fluid","value":"glycol","status":"declared","source_refs":["source:fluid"]}],
            "assertions":[{"assertion_id":"fluid-scope","kind":"source_applicability","subject":"source:fluid",
                "requirements":{"limits":{"fluid":["glycol"]}},"source_refs":["source:fluid"],"affected_decisions":[]}]});
            let condition = serde_json::to_string(&basis)?;
            let evidence = format!(
                "<{focus}> a <urn:review:Record>; <urn:review:name> \"Ada\"; \
            <urn:semantica:engineering-evidence:subjectRevision> \"r1\"; \
            <urn:semantica:engineering-evidence:scope> \"scope:fluid\"; \
            <urn:semantica:engineering-evidence:fieldEvidence> [<urn:semantica:engineering-evidence:sourceSha256> \"{}\"]; \
            <urn:oev2:project-context:conditionSha256> \"{}\" .",
                object.source_sha256,
                oe_core::sha256(condition.as_bytes())
            );
            let project = oe_kernel::project::Project {
                schema: "oev2.project/1".into(),
                project_id: "project-original-full12".into(),
                revision: "1".into(),
                purpose: "declared fluid applicability".into(),
                objects: BTreeMap::from([(focus.into(), "r1".into())]),
                conditions: BTreeMap::from([("scope:fluid".into(), condition)]),
                evidence: BTreeMap::from([
                    ("source:fluid".into(), object.source_sha256.clone()),
                    ("source:review".into(), oe_core::sha256(evidence.as_bytes())),
                ]),
                decisions: vec![],
                dependencies: BTreeMap::new(),
            };
            let binding = json!({"$schema":contracts::BINDING_SCHEMA,"binding_id":"binding:typed-review",
            "project":{"project_id":project.project_id,"domain":"example-industry"},
            "semantic_target":{"kind":"workspace","package_id":descriptor["package_id"],"workspace_id":REGISTRY},
            "baseline":{"version":descriptor["version"],"digest":descriptor["package_sha256"]},
            "evidence":{"logical_root":"urn:review:evidence"},
            "authority":{"fact":{"authority_id":"fact.authority","scope":["review"]},
                "decision":{"authority_id":"decision.authority","scope":["review"]}},
            "allowed_actions":["propose","review"],"lifecycle_actions":["candidate"],
            "promotion":{"target":"industry-registry","requires_decision_authority":true},
            "created_at":WHEN,"semantic_api":contracts::NATIVE_REFINERY_CONTRACT});
            let task = json!({"$schema":contracts::TASK_SCHEMA,"task_id":"task:typed-review",
            "task_kind":"source-scoped-review","intent":"Review current declared fluid Project",
            "project":project.project_id,"domain":"example-industry","requested_decision":"Scoped consistency",
            "evidence":[{"source_id":"source:review","uri":"urn:review:evidence/record","sha256":oe_core::sha256(evidence.as_bytes()),"media_type":"text/turtle","captured_at":WHEN}],
            "requested_actions":["review"],"actor_id":"fact.authority",
            "required_capabilities":["semantica.ontology.decision-review/v1"],"created_at":WHEN});
            let placeholder = oe_contracts::ArtifactReference {
                path: "var/placeholder".into(),
                sha256: "0".repeat(64),
            };
            let input = oe_contracts::TypedPromotedProjectReviewRequest1 {
                schema: "oev2.typed-promoted-project-review-request/1".into(),
                project: placeholder.clone(),
                semantic_binding: placeholder.clone(),
                task: placeholder.clone(),
                evidence: placeholder,
                workspace: workspace.to_string_lossy().into_owned(),
                expected_context: oe_kernel::project::context(&project, &object)?,
                object,
                source_id: "source:review".into(),
                focus_type_iri: "urn:review:Record".into(),
                query_asset_id: "review-query".into(),
                shape_asset_id: "review-shape".into(),
                required_checks: [
                    "review-query",
                    "review-shape",
                    "promoted_input",
                    "promoted_review",
                    "promoted_focus",
                    "bound_focus_context",
                    "current_project_context",
                    "current_condition_model",
                    "engineering_purpose",
                    "engineering:fluid-scope",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            };
            let mut f = Self {
                root,
                _run: run,
                project,
                binding,
                task,
                evidence,
                input,
                descriptor,
            };
            f.write()?;
            Ok(f)
        }
        fn write(&mut self) -> Result<()> {
            let put = |name: &str, bytes: Vec<u8>| -> Result<oe_contracts::ArtifactReference> {
                let path = self.root.path().join("var/inputs").join(name);
                fs::write(&path, &bytes)?;
                Ok(oe_contracts::ArtifactReference {
                    path: path
                        .strip_prefix(self.root.path())?
                        .to_string_lossy()
                        .into_owned(),
                    sha256: oe_core::sha256(bytes),
                })
            };
            self.input.project = put("project.json", serde_json::to_vec(&self.project)?)?;
            self.input.semantic_binding = put("binding.json", serde_json::to_vec(&self.binding)?)?;
            self.input.task = put("task.json", serde_json::to_vec(&self.task)?)?;
            self.input.evidence = put("evidence.ttl", self.evidence.as_bytes().to_vec())?;
            Ok(())
        }
        fn bind_evidence(&mut self) -> Result<()> {
            let hash = oe_core::sha256(self.evidence.as_bytes());
            self.task["evidence"][0]["sha256"] = json!(hash);
            self.project.evidence.insert("source:review".into(), hash);
            self.input.expected_context =
                oe_kernel::project::context(&self.project, &self.input.object)?;
            self.write()
        }
        fn review(&self) -> Result<oe_contracts::TypedPromotedProjectReviewReceipt1> {
            oe_kernel::project::review_promoted_current(&self.root, &self.input)
        }
    }
    type Hook = Box<dyn FnMut(&str, &Path) -> Result<()>>;
    thread_local! { static HOOK: RefCell<Option<Hook>> = RefCell::new(None); }
    pub(super) fn hook(phase: &str, stage: &Path) -> Result<()> {
        HOOK.with(|h| match &mut *h.borrow_mut() {
            Some(call) => call(phase, stage),
            None => Ok(()),
        })
    }
    struct HookReset;
    impl Drop for HookReset {
        fn drop(&mut self) {
            HOOK.with(|h| *h.borrow_mut() = None);
        }
    }
    fn install(call: impl FnMut(&str, &Path) -> Result<()> + 'static) -> HookReset {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(call)));
        HookReset
    }
    fn put(root: &SkillRoot, name: &str, value: &Value) -> Result<ArtifactReference> {
        let path = root.path().join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, serde_json::to_vec(value)?)?;
        artifacts::reference(root, &path)
    }
    fn policy(f: &Fixture) -> Result<Value> {
        artifacts::private_policy(&f.root, POLICY).map(|p| p.0)
    }
    fn write_policy(f: &Fixture, value: &Value) -> Result<()> {
        put(&f.root, POLICY, value)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                f.root.path().join(POLICY),
                fs::Permissions::from_mode(0o600),
            )?;
        }
        Ok(())
    }
    fn authorized(f: &mut Fixture) -> Result<Request> {
        f.project.decisions = vec![oe_kernel::project::Decision {
            id: "select".into(),
            object_id: f.input.object.object_id.clone(),
            object_revision: "r1".into(),
            condition_scope: "scope:fluid".into(),
            source_sha256: f.input.object.source_sha256.clone(),
            conclusion: "unknown".into(),
            adopted: false,
            basis: vec![],
        }];
        f.write()?;
        let observed = f.review()?;
        // Authorization selects the actual identity; it cannot create the reviewed package.
        assert_eq!(
            observed.registry.logical_package_sha256,
            f.descriptor["package_sha256"]
        );
        fs::write(
            f.root.path().join("var/inputs/fluid-source.txt"),
            b"independent declared fluid source",
        )?;
        let source =
            artifacts::reference(&f.root, &f.root.path().join("var/inputs/fluid-source.txt"))?;
        let sources: BTreeMap<String, ArtifactReference> = BTreeMap::from([
            ("source:fluid".into(), source),
            ("source:review".into(), f.input.evidence.clone()),
        ]);
        let approval = put(
            &f.root,
            "var/inputs/approval.json",
            &json!({"schema":APPROVAL,"approver":"reviewer","decision_id":"select","claim_scope":"scope:fluid","project_context":f.input.expected_context,"sources":sources.iter().map(|(id,r)| (id.clone(),r.sha256.clone())).collect::<BTreeMap<String,String>>(),"registry":observed.registry,"required_checks":f.input.required_checks}),
        )?;
        write_policy(
            f,
            &json!({"schema":AUTHORITY,"project_id":f.project.project_id,"decision_id":"select","claim_scope":"scope:fluid","actor_scopes":{"designer":["scope:fluid"]},"approvers":["reviewer"],"approved_approvals":[approval.sha256],"approved_registry":observed.registry,"required_checks":f.input.required_checks,"required_sources":sources}),
        )?;
        Ok(Request {
            schema: REQUEST.into(),
            review: f.input.clone(),
            decision_id: "select".into(),
            claim_scope: "scope:fluid".into(),
            actor: "designer".into(),
            approval,
        })
    }
    fn fixture() -> Result<(Fixture, Request)> {
        let mut f = Fixture::new("urn:review:focus")?;
        let r = authorized(&mut f)?;
        Ok((f, r))
    }
    fn denied(f: &Fixture, input: &Request, name: &str) -> Result<Value> {
        let output = PathBuf::from(format!("var/{name}"));
        let error = adopt(&f.root, input, &output).expect_err("must not adopt");
        let retained = error
            .downcast_ref::<RetainedFailure>()
            .context("partial failure retained")?
            .0
            .clone();
        assert_eq!(
            retained["publication"]["publisher_invoked"], false,
            "{retained}"
        );
        assert!(!f.root.path().join(output).exists());
        assert_eq!(retained["engineering_adoption"], "not_established");
        Ok(retained)
    }
    fn verification(result: &AdoptionResult) -> VerifyRequest {
        VerifyRequest {
            schema: "oev2.engineering-promoted-adoption-verify-request/2".into(),
            record: result.record.clone(),
            current_project: result.project.clone(),
        }
    }
    fn snapshot(path: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut all = BTreeMap::new();
        fn walk(path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
            for e in fs::read_dir(path)? {
                let p = e?.path();
                let m = p.symlink_metadata()?;
                ensure!(!m.file_type().is_symlink(), "test symlink");
                if m.is_dir() {
                    walk(&p, out)?
                } else {
                    out.insert(p.clone(), fs::read(p)?);
                }
            }
            Ok(())
        }
        walk(path, &mut all)?;
        Ok(all)
    }

    #[test]
    fn promoted_adoption2_actual_promoted_adoption() -> Result<()> {
        let (f, input) = fixture()?;
        let before = artifacts::read(&f.root, &input.review.project)?;
        let result = adopt(&f.root, &input, Path::new("var/adopted"))?;
        assert_eq!(result.publication, "published");
        let review: Review = serde_json::from_value(artifacts::json(&f.root, &result.review)?)?;
        assert_eq!(review.checks.len(), 11);
        assert_eq!(
            review
                .checks
                .iter()
                .map(|c| &c.id)
                .collect::<BTreeSet<_>>()
                .len(),
            11
        );
        assert!(!review.fact_admitted);
        assert_eq!(review.engineering_adoption, "unknown");
        let actual: Project = serde_json::from_value(artifacts::json(&f.root, &result.project)?)?;
        let mut expected = f.project.clone();
        expected.decisions[0].adopted = true;
        expected.decisions[0].conclusion = "supported".into();
        expected.decisions[0].basis =
            vec![result.review.sha256.clone(), input.approval.sha256.clone()];
        assert!(same(&actual, &expected)?);
        assert_eq!(artifacts::read(&f.root, &input.review.project)?, before);
        let verified = verify(&f.root, &verification(&result))?;
        assert_eq!(verified.status, "current_scope_bound");
        assert_eq!(verified.broader_adoption, "not_established");
        Ok(())
    }
    #[test]
    fn promoted_adoption2_authority_independence() -> Result<()> {
        let (f, input) = fixture()?;
        let original = policy(&f)?;
        for (i, change) in [
            "actor", "scope", "approver", "approval", "project", "decision", "self",
        ]
        .iter()
        .enumerate()
        {
            let mut p = original.clone();
            let mut r = input.clone();
            match *change {
                "actor" => r.actor = "outsider".into(),
                "scope" => r.claim_scope = "other".into(),
                "approver" => p["approvers"] = json!(["other"]),
                "approval" => p["approved_approvals"] = json!(["a".repeat(64)]),
                "project" => p["project_id"] = json!("other"),
                "decision" => r.decision_id = "other".into(),
                "self" => {
                    r.actor = "reviewer".into();
                    p["actor_scopes"]["reviewer"] = json!(["scope:fluid"]);
                }
                _ => unreachable!(),
            }
            write_policy(&f, &p)?;
            denied(&f, &r, &format!("deny-authority-{i}"))?;
        }
        write_policy(&f, &original)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                f.root.path().join(POLICY),
                fs::Permissions::from_mode(0o644),
            )?;
            assert!(
                denied(&f, &input, "deny-mode")?["reason"]
                    .as_str()
                    .unwrap()
                    .contains("private_or_owned")
            );
        }
        Ok(())
    }
    #[test]
    fn promoted_adoption2_versioned_identity() -> Result<()> {
        let (f, input) = fixture()?;
        let original = policy(&f)?;
        for (i, key) in [
            "schema1",
            "schema_unknown",
            "mixed",
            "workspace",
            "registry_id",
            "version",
            "logical_package_sha256",
            "manifest_sha256",
            "delta_sha256",
            "execution_projection_sha256",
        ]
        .iter()
        .enumerate()
        {
            let mut p = original.clone();
            match *key {
                "schema1" => p["schema"] = json!("oev2.engineering-adoption-authority/1"),
                "schema_unknown" => p["schema"] = json!("oev2.engineering-adoption-authority/3"),
                "mixed" => p["approved_package_sha256"] = json!("a".repeat(64)),
                "workspace" => {
                    p["approved_registry"][key] =
                        json!(f.root.path().join("var/other").to_string_lossy())
                }
                "registry_id" => p["approved_registry"][key] = json!("other-registry"),
                "version" => p["approved_registry"][key] = json!("0.0.0"),
                _ => p["approved_registry"][key] = json!("a".repeat(64)),
            }
            write_policy(&f, &p)?;
            denied(&f, &input, &format!("deny-identity-{i}"))?;
        }
        write_policy(&f, &original)?;
        let mut stale = input.clone();
        let mut b = f.binding.clone();
        b["baseline"]["version"] = json!("0.0.0");
        stale.review.semantic_binding = put(&f.root, "var/inputs/stale-binding.json", &b)?;
        let retained = denied(&f, &stale, "deny-current-version")?;
        assert!(
            retained["reason"]
                .as_str()
                .unwrap()
                .contains("source-locked")
        );
        Ok(())
    }
    #[test]
    fn promoted_adoption2_current_input_change() -> Result<()> {
        let (mut f, input) = fixture()?;
        let original = f.project.clone();
        for (i, key) in ["purpose", "revision", "condition", "source"]
            .iter()
            .enumerate()
        {
            f.project = original.clone();
            match *key {
                "purpose" => f.project.purpose = "changed purpose".into(),
                "revision" => {
                    f.project
                        .objects
                        .insert(f.input.object.object_id.clone(), "r2".into());
                }
                "condition" => {
                    f.project
                        .conditions
                        .insert("scope:fluid".into(), "changed condition".into());
                }
                _ => {
                    f.project
                        .evidence
                        .insert("source:fluid".into(), "a".repeat(64));
                }
            }
            f.write()?;
            let mut request = input.clone();
            request.review.project = f.input.project.clone();
            denied(&f, &request, &format!("deny-project-{i}"))?;
        }
        // All current metadata is coherently refreshed, while RDF remains old.
        f.project = original;
        let mut basis: Value = serde_json::from_str(&f.project.conditions["scope:fluid"])?;
        basis["purpose"] = json!("changed declared purpose");
        f.project.purpose = "changed declared purpose".into();
        f.project
            .conditions
            .insert("scope:fluid".into(), serde_json::to_string(&basis)?);
        f.input.expected_context = oe_kernel::project::context(&f.project, &f.input.object)?;
        let current = authorized(&mut f)?;
        let retained = denied(&f, &current, "deny-stale-rdf")?;
        assert!(retained["native_observation"].is_object());
        Ok(())
    }
    #[test]
    fn promoted_adoption2_required_evidence_and_checks() -> Result<()> {
        let (mut f, input) = fixture()?;
        let original = policy(&f)?;
        let mut missing = input.clone();
        missing
            .review
            .required_checks
            .retain(|x| x != "promoted_input");
        denied(&f, &missing, "deny-required")?;
        let mut policy_missing = original.clone();
        policy_missing["required_sources"]
            .as_object_mut()
            .unwrap()
            .remove("source:fluid");
        write_policy(&f, &policy_missing)?;
        denied(&f, &input, "deny-source")?;
        write_policy(&f, &original)?;
        f.input.query_asset_id = "ordinary-query".into();
        f.input.required_checks[0] = "ordinary-query".into();
        let unknown = authorized(&mut f)?;
        denied(&f, &unknown, "deny-unknown-query")?;
        f.input.query_asset_id = "review-query".into();
        f.input.required_checks[0] = "review-query".into();
        f.input.focus_type_iri = "urn:wrong:type".into();
        let wrong = authorized(&mut f)?;
        denied(&f, &wrong, "deny-focus")?;
        let mut long = Fixture::new(&format!("urn:review:{}", "x".repeat(1_048_576)))?;
        let partial = authorized(&mut long)?;
        let observed = long.review()?;
        assert_eq!(observed.execution, "partial");
        assert_ne!(observed.conclusion, "pass");
        let retained = denied(&long, &partial, "deny-partial")?;
        assert!(retained["native_observation"].is_object());
        long.evidence = "# empty input despite a long declared focus\n".into();
        long.bind_evidence()?;
        let empty = authorized(&mut long)?;
        assert_eq!(long.review()?.conclusion, "violated");
        denied(&long, &empty, "deny-empty")?;
        let mut basis: Value = serde_json::from_str(&f.project.conditions["scope:fluid"])?;
        basis["assertions"] = json!([]);
        f.project
            .conditions
            .insert("scope:fluid".into(), serde_json::to_string(&basis)?);
        f.input.expected_context = oe_kernel::project::context(&f.project, &f.input.object)?;
        f.write()?;
        let mut empty_basis = input;
        empty_basis.review = f.input.clone();
        denied(&f, &empty_basis, "deny-empty-basis")?;
        Ok(())
    }
    #[test]
    fn promoted_adoption2_last_guards() -> Result<()> {
        let (f, input) = fixture()?;
        let mut targets = vec![
            f.root.path().join(POLICY),
            f.root.path().join(&input.approval.path),
            f.root.path().join(&input.review.project.path),
            f.root.path().join(&input.review.task.path),
            f.root.path().join(&input.review.semantic_binding.path),
            f.root.path().join(&input.review.evidence.path),
            f.root.path().join("var/inputs/fluid-source.txt"),
            f.root
                .path()
                .join("crates/oe-semantic/src/refinery_runtime.rs"),
            PathBuf::from(&input.review.workspace).join("registry.json"),
        ];
        let assets = snapshot(Path::new(&input.review.workspace))?
            .into_iter()
            .filter(|(_, b)| b == b"SELECT ?finding WHERE { ?s <urn:review:bad> ?finding }")
            .map(|(p, _)| p)
            .collect::<Vec<_>>();
        assert!(!assets.is_empty());
        targets.extend(assets);
        for (i, path) in targets.into_iter().enumerate() {
            let before = fs::read(&path)?;
            let altered = path.clone();
            let hook_guard = install(move |phase, _| {
                if phase == "after_owner_guards" {
                    fs::write(&altered, b"changed after actual review")?;
                }
                Ok(())
            });
            let rejected = denied(&f, &input, &format!("deny-last-guard-{i}"));
            drop(hook_guard);
            fs::write(&path, before)?;
            let retained = rejected?;
            assert!(retained["native_observation"].is_object());
            let stage = retained["publication"]["stage"]
                .as_str()
                .context("prepared stage retained")?;
            assert!(!Path::new(stage).join("payload/adoption.json").exists());
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(Path::new(stage).join("intent.json"))?)?
                    ["execution"],
                "not_completed"
            );
        }
        Ok(())
    }
    #[test]
    fn promoted_adoption2_lock_and_order() -> Result<()> {
        let (f, input) = fixture()?;
        let root = f.root.clone();
        let workspace = PathBuf::from(&input.review.workspace);
        let events = std::rc::Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let _guard = install(move |phase, stage| {
            captured.borrow_mut().push(phase.to_owned());
            let attempt = FileStore::open(&root, &workspace, REGISTRY, WHEN, false);
            assert!(
                attempt.is_err(),
                "original registry lock must remain held at {phase}"
            );
            assert!(format!("{:#}", attempt.err().unwrap()).contains("resource_busy"));
            if phase == "after_prepare" {
                assert!(!stage.join("payload/adoption.json").exists());
            }
            Ok(())
        });
        adopt(&f.root, &input, Path::new("var/lock-order"))?;
        assert_eq!(
            &*events.borrow(),
            &[
                "before_prepare_files",
                "after_prepare",
                "after_owner_guards",
                "before_write",
                "before_rename",
                "after_rename"
            ]
        );
        Ok(())
    }
    #[test]
    fn promoted_adoption2_publication_faults() -> Result<()> {
        let (f, input) = fixture()?;
        fs::create_dir(f.root.path().join("var/existing"))?;
        fs::write(f.root.path().join("var/existing/original"), b"untouched")?;
        assert!(adopt(&f.root, &input, Path::new("var/existing")).is_err());
        assert_eq!(
            fs::read(f.root.path().join("var/existing/original"))?,
            b"untouched"
        );
        for phase in [
            "before_prepare_files",
            "before_write",
            "before_rename",
            "after_rename",
        ] {
            let destination = f.root.path().join(format!("var/fault-{phase}"));
            let final_path = destination.clone();
            let guard = install(move |actual, stage| {
                if actual == phase {
                    match phase {
                        "before_prepare_files" => fs::write(
                            stage.join("payload/review.json"),
                            b"unexpected prepared review",
                        )?,
                        "before_write" => fs::write(
                            stage.join("payload/adoption.json"),
                            b"unexpected existing file",
                        )?,
                        "before_rename" => fs::create_dir(&final_path)?,
                        _ => anyhow::bail!("injected_directory_sync_unavailable"),
                    }
                }
                Ok(())
            });
            let error = adopt(&f.root, &input, &destination).unwrap_err();
            drop(guard);
            let partial = &error.downcast_ref::<RetainedFailure>().unwrap().0;
            assert_eq!(partial["execution"], "failed");
            assert_eq!(
                partial["publication"]["publisher_invoked"],
                phase != "before_prepare_files"
            );
            assert!(
                Path::new(
                    partial["publication"]["stage"]
                        .as_str()
                        .context("retained stage path")?
                )
                .exists()
            );
            if matches!(phase, "before_prepare_files" | "before_write") {
                assert_eq!(partial["publication"]["publication"], "not_performed");
                assert!(!destination.exists());
            } else {
                assert_eq!(partial["publication"]["publication"], "unknown");
            }
            assert_eq!(
                destination.join("adoption.json").exists(),
                phase == "after_rename"
            );
        }
        Ok(())
    }
    #[test]
    fn promoted_adoption2_verify_current_decision() -> Result<()> {
        let (f, input) = fixture()?;
        let result = adopt(&f.root, &input, Path::new("var/adopted"))?;
        let original: Project = serde_json::from_value(artifacts::json(&f.root, &result.project)?)?;
        let mut missing = serde_json::to_value(verification(&result))?;
        missing.as_object_mut().unwrap().remove("current_project");
        assert!(serde_json::from_value::<VerifyRequest>(missing).is_err());
        for (i, change) in ["unadopt", "conclusion", "basis", "duplicate", "purpose"]
            .iter()
            .enumerate()
        {
            let mut project = original.clone();
            match *change {
                "unadopt" => project.decisions[0].adopted = false,
                "conclusion" => project.decisions[0].conclusion = "unknown".into(),
                "basis" => project.decisions[0].basis = vec!["a".repeat(64)],
                "duplicate" => project.decisions.push(project.decisions[0].clone()),
                _ => project.purpose = "changed caller purpose".into(),
            }
            if matches!(*change, "unadopt" | "basis") {
                assert!(same(
                    &oe_kernel::project::context(&project, &input.review.object)?,
                    &input.review.expected_context
                )?);
            }
            let mut request = verification(&result);
            request.current_project = put(
                &f.root,
                &format!("var/inputs/changed-current-{i}.json"),
                &serde_json::to_value(project)?,
            )?;
            assert!(verify(&f.root, &request).is_err());
        }
        assert_eq!(
            verify(&f.root, &verification(&result))?.status,
            "current_scope_bound"
        );
        Ok(())
    }
    #[test]
    fn promoted_adoption2_fresh_verification_and_regressions() -> Result<()> {
        let (f, input) = fixture()?;
        let result = adopt(&f.root, &input, Path::new("var/adopted"))?;
        let before = snapshot(&f.root.path().join("var"))?;
        let guard = install(|phase, _| {
            if phase != "verify_precommit" {
                anyhow::bail!("verify must never enter owner publisher/stager:{phase}");
            }
            Ok(())
        });
        verify(&f.root, &verification(&result))?;
        assert_eq!(snapshot(&f.root.path().join("var"))?, before);
        drop(guard);
        for path in [
            &result.record.path,
            &result.review.path,
            &result.project.path,
            &input.review.project.path,
        ] {
            let target = f.root.path().join(path);
            let previous = fs::read(&target)?;
            let changed = target.clone();
            let installed = install(move |phase, _| {
                if phase == "verify_precommit" {
                    fs::write(&changed, b"changed after current review")?;
                }
                Ok(())
            });
            let observed = verify(&f.root, &verification(&result));
            drop(installed);
            fs::write(&target, previous)?;
            assert!(
                observed.is_err(),
                "historical credential must be re-read under lock"
            );
        }
        for path in [
            f.root.path().join(POLICY),
            f.root.path().join(&input.approval.path),
            f.root.path().join("var/inputs/fluid-source.txt"),
            f.root
                .path()
                .join("crates/oe-semantic/src/refinery_runtime.rs"),
            PathBuf::from(&input.review.workspace).join("registry.json"),
        ] {
            let bytes = fs::read(&path)?;
            fs::write(&path, b"changed since adoption")?;
            let observed = verify(&f.root, &verification(&result));
            fs::write(&path, bytes)?;
            assert!(observed.is_err());
        }
        Ok(())
    }
}
