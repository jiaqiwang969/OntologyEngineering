//! Handwritten execution implements the interface produced by real ggen.
use anyhow::{Result, ensure};
use oe_contracts::{ErrorCode, FailureRecord, ReviewRequest, ReviewResult, SemanticReviewPort};
use std::{collections::BTreeSet, path::PathBuf};

pub struct RustSemanticPort {
    pub root: PathBuf,
    pub binding: crate::Binding,
    pub bundle: String,
    pub queries: Vec<String>,
    pub shapes: Vec<String>,
    pub evidence: Vec<u8>,
}
impl RustSemanticPort {
    pub fn execute(
        &self,
        input: &ReviewRequest,
        prior: Option<&crate::Receipt>,
    ) -> Result<crate::Receipt> {
        input
            .validate()
            .map_err(|e| anyhow::anyhow!("generated_contract_rejected:{e:?}"))?;
        ensure!(
            input.object.object_id == self.binding.object_id
                && input.object.revision == self.binding.object_revision
                && input.object.condition_scope == self.binding.condition_scope
                && input.object.source_sha256 == self.binding.source_sha256,
            "review_object_binding_mismatch"
        );
        ensure!(
            input.evidence_sha256 == oe_core::sha256(&self.evidence),
            "review_evidence_hash_mismatch"
        );
        let requested: BTreeSet<_> = input.required_checks.iter().cloned().collect();
        ensure!(
            requested == self.binding.required_checks
                && requested.len() == input.required_checks.len(),
            "review_required_checks_mismatch"
        );
        let package = oe_semantic::load_bundle(&self.root, &self.bundle)?;
        ensure!(
            self.binding.package_id == package.spec["package_id"]
                && self.binding.package_version == package.spec["package_version"]
                && self.binding.package_sha256 == package.spec["sha256"],
            "review_package_binding_mismatch"
        );
        let mut checks = crate::packages::project_checks(
            &self.root,
            &self.bundle,
            &self.evidence,
            &self.queries,
            &self.shapes,
        )?;
        checks.push(bound_focus_context_mapped(
            &input.object,
            &self.evidence,
            &self.shapes,
            &checks,
            self.binding.rdf_focus.as_ref(),
        )?);
        crate::review(self.binding.clone(), &self.evidence, checks, prior)
    }
}
impl SemanticReviewPort for RustSemanticPort {
    fn review(&self, input: &ReviewRequest) -> std::result::Result<ReviewResult, FailureRecord> {
        let receipt=self.execute(input,None).map_err(|error|FailureRecord {code:ErrorCode::InvalidContract,phase:"interface".into(),retryable:false,partial_result:Some(serde_json::json!({"execution":"failed","conclusion":"unknown","reason":error.to_string()}))})?;
        Ok(ReviewResult {
            execution: receipt.execution,
            conclusion: receipt.conclusion,
            coverage: receipt.coverage.into_iter().collect(),
            failure_reasons: receipt.failures,
            adoption_conditions: receipt.adoption_conditions,
            engine_version: receipt.engine_version,
            evidence_sha256: receipt.evidence_sha256,
        })
    }
}

/// Reuse the retained RDF focus/revision/scope/source guard without an archive binding.
pub(crate) fn bound_focus_context(
    object: &oe_contracts::ObjectIdentity,
    evidence: &[u8],
    shapes: &[String],
    checks: &[crate::CheckResult],
) -> Result<crate::CheckResult> {
    bound_focus_context_mapped(object, evidence, shapes, checks, None)
}

pub(crate) fn bound_focus_context_mapped(
    object: &oe_contracts::ObjectIdentity,
    evidence: &[u8],
    shapes: &[String],
    checks: &[crate::CheckResult],
    mapping: Option<&oe_contracts::RdfObjectFocus>,
) -> Result<crate::CheckResult> {
    // The current retained ReviewClaim profile binds the expected revision and
    // condition to the actual focus node. Other graph profiles stay unknown.
    let data = crate::graph::load(evidence)?;
    let focus = crate::rdf_focus::resolve(&object.object_id, mapping, &data)?;
    let rows = crate::graph::select(
        &data,
        &format!(
            "SELECT ?revision ?scope WHERE {{ {focus} <urn:semantica:engineering-evidence:subjectRevision> ?revision ; <urn:semantica:engineering-evidence:scope> ?scope }}"
        ),
    )?;
    let context_matches = rows.len() == 1
        && rows[0].get("revision").is_some_and(|t| {
            t == &oxigraph::model::Term::Literal(oxigraph::model::Literal::new_simple_literal(
                &object.revision,
            ))
        })
        && rows[0].get("scope").is_some_and(|t| {
            t == &oxigraph::model::Term::Literal(oxigraph::model::Literal::new_simple_literal(
                &object.condition_scope,
            ))
        });
    let focus_checked = !shapes.is_empty()
        && shapes.iter().all(|id| {
            checks.iter().find(|c| &c.id == id).is_some_and(|c| {
                c.result["focus_nodes"].as_array().is_some_and(|nodes| {
                    nodes
                        .iter()
                        .any(|n| n["kind"] == "iri" && n["value"] == focus.as_str())
                })
            })
        });
    let digest = oxigraph::model::Literal::new_simple_literal(&object.source_sha256);
    let linked_source = crate::graph::query(
        &data,
        &format!(
            "ASK {{ {focus} <urn:semantica:engineering-evidence:fieldEvidence>/<urn:semantica:engineering-evidence:sourceSha256> {digest} }}"
        ),
    )?["value"]
        == true;
    let source_matches = object.source_sha256 == oe_core::sha256(evidence) || linked_source;
    Ok(crate::CheckResult {
        id: "bound_focus_context".into(),
        execution: "completed".into(),
        conclusion: if context_matches && focus_checked && source_matches {
            "pass"
        } else {
            "unknown"
        }
        .into(),
        coverage: vec![
            object.object_id.clone(),
            object.revision.clone(),
            object.condition_scope.clone(),
        ],
        failures: Vec::new(),
        result: serde_json::json!({"context_matches":context_matches,"focus_checked":focus_checked,"source_matches":source_matches,"profile":"historical engineering-evidence ReviewClaim; unmapped profiles remain unknown"}),
    })
}
