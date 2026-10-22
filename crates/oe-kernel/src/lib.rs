//! Formal v2 Rust execution. Historical receipts retain their original identity.
pub mod books;
pub mod cad_intent;
pub mod calculation;
pub mod chapters;
pub mod drafting;
pub mod drawing_intent;
pub mod engineering;
pub mod forward;
pub mod governance;
pub mod graph;
pub mod learning;
pub mod mechanics;
pub mod motion;
pub mod numeric;
pub mod packages;
pub mod port;
pub mod project;
pub mod provenance;
pub mod query;
mod rdf_focus;
pub mod rules;
pub mod shacl;
pub mod sheet_persistence;
pub mod source_session;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const ENGINE_ID: &str = "oev2.rust-semantic-kernel";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub project_id: String,
    pub object_id: String,
    pub object_revision: String,
    pub package_id: String,
    pub package_version: String,
    pub package_sha256: String,
    pub source_sha256: String,
    pub condition_scope: String,
    pub required_checks: BTreeSet<String>,
    pub authority_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_context: Option<oe_contracts::ProjectReviewContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rdf_focus: Option<oe_contracts::RdfObjectFocus>,
}
impl Binding {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [
                &self.project_id,
                &self.object_id,
                &self.object_revision,
                &self.package_id,
                &self.package_version,
                &self.condition_scope,
                &self.authority_id
            ]
            .into_iter()
            .all(|s| !s.trim().is_empty()),
            "binding_identity_missing"
        );
        for s in [&self.package_sha256, &self.source_sha256] {
            ensure!(
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "binding_digest_invalid"
            );
        }
        ensure!(
            !self.required_checks.is_empty(),
            "binding_required_coverage_empty"
        );
        if let Some(mapping) = &self.rdf_focus {
            mapping
                .validate()
                .map_err(|_| anyhow::anyhow!("rdf_object_focus_contract_invalid"))?;
            ensure!(
                mapping.object_id == self.object_id,
                "rdf_object_focus_identity_mismatch"
            );
            oxigraph::model::NamedNode::new(&mapping.focus_iri)?;
        }
        if let Some(context) = &self.project_context {
            context
                .validate()
                .map_err(|_| anyhow::anyhow!("binding_project_context_invalid"))?;
            ensure!(
                context.schema == "oev2.project-review-context/1"
                    && context.project_id == self.project_id
                    && context.object_id == self.object_id
                    && context.object_revision == self.object_revision
                    && context.condition_scope == self.condition_scope
                    && context.source_sha256 == self.source_sha256,
                "binding_project_context_identity_mismatch"
            );
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(oe_core::sha256(&oe_wire::json::canonical_json(
            &serde_json::to_value(self)?,
        )?))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub id: String,
    pub execution: String,
    pub conclusion: String,
    pub coverage: Vec<String>,
    pub failures: Vec<String>,
    pub result: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: String,
    pub engine_id: String,
    pub engine_version: String,
    pub executable_sha256: String,
    pub binding: Binding,
    pub binding_sha256: String,
    pub evidence_sha256: String,
    pub execution: String,
    pub conclusion: String,
    pub coverage: BTreeSet<String>,
    pub failures: Vec<String>,
    pub adoption_conditions: Vec<String>,
    pub checks: Vec<CheckResult>,
}
pub fn executable_digest() -> Result<String> {
    let p = std::env::current_exe()?;
    Ok(oe_core::fingerprint_file(&p, 512 * 1024 * 1024)?.sha256)
}
/// One strict aggregation algorithm for archive and registry-bound reviews.
pub(crate) struct CheckSummary {
    pub execution: String,
    pub conclusion: String,
    pub coverage: BTreeSet<String>,
    pub failures: Vec<String>,
}
pub(crate) fn summarize_checks(
    required_checks: &BTreeSet<String>,
    checks: &[CheckResult],
) -> Result<CheckSummary> {
    let mut failures = Vec::new();
    let mut coverage = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for c in checks {
        ensure!(seen.insert(&c.id), "duplicate_check_identity");
        ensure!(
            ["completed", "failed", "not_run", "unsupported"].contains(&c.execution.as_str()),
            "invalid_check_execution"
        );
        ensure!(
            ["pass", "violated", "unknown"].contains(&c.conclusion.as_str()),
            "invalid_check_conclusion"
        );
        ensure!(
            c.execution == "completed" || c.conclusion == "unknown",
            "nonexecution_cannot_have_verdict"
        );
        ensure!(
            c.conclusion != "pass" || c.failures.is_empty(),
            "check_with_failures_cannot_pass"
        );
        if c.execution == "completed" {
            coverage.insert(c.id.clone());
        }
        failures.extend(c.failures.clone());
    }
    let missing: Vec<_> = required_checks.difference(&coverage).cloned().collect();
    failures.extend(
        missing
            .iter()
            .map(|s| format!("required_check_not_executed:{s}")),
    );
    let conclusion = if checks.iter().any(|c| c.conclusion == "violated") {
        "violated"
    } else if missing.is_empty()
        && checks
            .iter()
            .all(|c| c.execution == "completed" && c.conclusion == "pass")
        && !checks.is_empty()
    {
        "pass"
    } else {
        "unknown"
    };
    let execution = if checks.is_empty() {
        "not_run"
    } else if checks.iter().all(|c| c.execution == "completed") {
        "completed"
    } else if checks.iter().any(|c| c.execution == "completed") {
        "partial"
    } else if checks.iter().any(|c| c.execution == "failed") {
        "failed"
    } else {
        "not_run"
    };
    Ok(CheckSummary {
        execution: execution.into(),
        conclusion: conclusion.into(),
        coverage,
        failures,
    })
}

/// Aggregate strict execution/coverage states. Completed unknown answers stay unknown.
pub fn evaluate(binding: Binding, evidence: &[u8], checks: Vec<CheckResult>) -> Result<Receipt> {
    binding.validate()?;
    ensure!(evidence.len() <= MAX_BYTES, "evidence_budget_exceeded");
    let CheckSummary {
        execution,
        conclusion,
        coverage,
        failures,
    } = summarize_checks(&binding.required_checks, &checks)?;
    Ok(Receipt{schema:"oev2.semantic-receipt/1".into(),engine_id:ENGINE_ID.into(),engine_version:ENGINE_VERSION.into(),executable_sha256:executable_digest()?,binding_sha256:binding.digest()?,evidence_sha256:oe_core::sha256(evidence),binding,execution:execution.into(),conclusion:conclusion.into(),coverage,failures,adoption_conditions:vec!["semantic pass covers only declared checks; native tools and engineering adoption require their own evidence".into()],checks})
}
pub fn review(
    binding: Binding,
    evidence: &[u8],
    checks: Vec<CheckResult>,
    prior: Option<&Receipt>,
) -> Result<Receipt> {
    let mut current = evaluate(binding, evidence, checks)?;
    current.adoption_conditions.push("historical RDF snapshot review; current Project content not checked by this execution path".into());
    if let Some(old) = prior {
        ensure!(
            old.binding_sha256 == old.binding.digest()?,
            "prior_receipt_binding_digest_invalid"
        );
        if old.engine_id != ENGINE_ID
            || old.engine_version != ENGINE_VERSION
            || old.binding_sha256 != current.binding_sha256
            || old.evidence_sha256 != current.evidence_sha256
        {
            current.adoption_conditions.push("prior_conclusion_requires_fresh_review; original receipt retained under its own identity".into());
        }
    }
    Ok(current)
}
pub fn capabilities() -> Value {
    let mut value = json!({"engine_id":ENGINE_ID,"engine_version":ENGINE_VERSION,"rdf":"Oxigraph Turtle ingestion and RDF terms","sparql":"Oxigraph local read queries; SERVICE and remote dataset clauses refused","query_stack_bytes":graph::QUERY_STACK_BYTES,"source_session":{"profile":"semantica.rdflib760-source-session/1","entry":"kernel source-session","scope":"bounded explicit local resources, same session, load/query/update/clear/snapshot; semantic failure preserved; no engineering adoption"},"query_total_time_limit":"not_implemented","query_arbitrary_depth_safety":"not_established","shacl":shacl::supported(),"rules":"bounded unary Horn rules and positive conjunctions; no negation/function symbols","governance":"immutable content identities, append-only guarded events; no automatic promotion/publication","python_dependency":false,"engineering_acceptance":"separate"});
    value["datalog_graph"] = json!({"entry":"kernel datalog-graph","profile":"semantica.positive-function-free-datalog-graph/1","graph":"immutable dictionary node/edge snapshot","owner":"same DatalogReasoner load_from_graph","engineering_adoption":false});
    value["datalog"] = json!({"entry":"kernel datalog", "profile":semantica_runtime::datalog_port::PROFILE,"scope":"bounded stateless positive multi-variable closure and at most one query; same existing DatalogReasoner; engineering adoption separate"});
    value
}
pub fn source_identities(inputs: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    inputs
        .iter()
        .map(|(k, v)| (k.clone(), oe_core::sha256(v)))
        .collect()
}
