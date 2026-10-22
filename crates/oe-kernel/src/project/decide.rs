//! Native decision update from registered, re-verified tool results.
//!
//! Replaces scripts that edited `project.json` directly. A decision may move to
//! supported/refuted only on registered results that target it and carry
//! observed findings; adoption is never changed here.
use super::tool_result;
use super::{Project, impact, validate};
use anyhow::Result;
use oe_contracts::{CapabilityRegistry, DecisionUpdate, DecisionUpdateReceipt};
use oe_core::{SkillRoot, sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const UPDATE_SCHEMA: &str = "oev2.decision-update/1";
pub const RECEIPT_SCHEMA: &str = "oev2.decision-update-receipt/1";
const SCOPE: &str = "Decision conclusion and basis from registered tool results only; adoption unchanged and still requires independent approval.";
const RESULT_LIMIT: usize = 64 * 1024 * 1024;

/// Apply one decision update to a copy of the Project.
pub fn decide(
    root: &SkillRoot,
    project_bytes: &[u8],
    registry: &CapabilityRegistry,
    request: &DecisionUpdate,
    revision: &str,
    output: &str,
) -> Result<(Option<Vec<u8>>, DecisionUpdateReceipt)> {
    let project: Project = serde_json::from_value(oe_core::json::strict_json(project_bytes)?)?;
    validate(&project)?;
    let previous = sha256(project_bytes);
    let mut fail = Vec::new();
    if let Err(f) = request.validate() {
        fail.extend(f.into_iter().map(|x| format!("contract_invalid:{x}")));
    }
    if request.schema != UPDATE_SCHEMA {
        fail.push("decision_update_schema".into());
    }
    let decision = project
        .decisions
        .iter()
        .find(|d| d.id == request.decision_id);
    let previous_conclusion = decision.map(|d| d.conclusion.clone()).unwrap_or_default();
    match decision {
        None => fail.push(format!("decision_unknown:{}", request.decision_id)),
        Some(d) if d.adopted => fail.push("decision_adopted_use_adoption_flow".into()),
        _ => {}
    }
    if revision.trim().is_empty() || revision == project.revision {
        fail.push("project_revision_not_new".into());
    }
    if request.rationale.trim().is_empty() || request.claim_scope.trim().is_empty() {
        fail.push("rationale_or_scope_missing".into());
    }
    // Re-verify every cited result and confirm it is the registered evidence.
    let mut basis = BTreeSet::new();
    let mut findings: BTreeMap<String, String> = BTreeMap::new();
    let mut targeted_support = false;
    let mut bindings = BTreeSet::new();
    for r in &request.results {
        let bytes = match tool_result::read_reference(root, r, RESULT_LIMIT) {
            Ok(b) => b,
            Err(e) => {
                fail.push(format!("result_unreadable:{}:{e}", r.path));
                continue;
            }
        };
        let v = tool_result::verify(root, &project, registry, &bytes)?;
        let Some(res) = v.result.as_ref() else {
            fail.push(format!("result_contract_invalid:{}", r.path));
            continue;
        };
        let id = format!("tool-result:{}", res.result_id);
        if project.evidence.get(&id) != Some(&r.sha256) {
            fail.push(format!("result_not_registered:{}", res.result_id));
            continue;
        }
        if v.check.status != "accepted" {
            fail.push(format!("result_rejected:{}", res.result_id));
            continue;
        }
        let targets = res.decisions.iter().any(|d| *d == request.decision_id);
        // A result can only speak for the decision's own object and condition.
        if let (true, Some(d)) = (targets, decision) {
            let b = &res.project;
            if (&b.object_id, &b.condition_scope) != (&d.object_id, &d.condition_scope) {
                fail.push(format!("result_binding_mismatch:{}", res.result_id));
                continue;
            }
            bindings.insert((b.object_revision.clone(), b.source_sha256.clone()));
        }
        basis.insert(id);
        if targets && v.check.decision_support == "observed_findings" {
            targeted_support = true;
        }
        for f in &res.findings {
            findings.insert(
                format!("{}#{}", res.result_id, f.finding_id),
                f.kind.clone(),
            );
        }
    }
    if bindings.len() > 1 {
        fail.push("result_binding_conflict".into());
    }
    let decisive = matches!(request.conclusion.as_str(), "supported" | "refuted");
    if decisive {
        if !targeted_support {
            fail.push("no_registered_observed_result_for_decision".into());
        }
        if request.findings.is_empty() {
            fail.push("findings_not_cited".into());
        }
    }
    for f in &request.findings {
        match findings.get(f).map(String::as_str) {
            Some("observed") | Some("inferred") => {}
            Some(_) => fail.push(format!("finding_not_observed_or_inferred:{f}")),
            None => fail.push(format!("finding_unknown:{f}")),
        }
    }
    let refused = |fail: Vec<String>| DecisionUpdateReceipt {
        schema: RECEIPT_SCHEMA.into(),
        status: "refused".into(),
        decision_id: request.decision_id.clone(),
        previous_conclusion: previous_conclusion.clone(),
        conclusion: previous_conclusion.clone(),
        basis: vec![],
        previous_project_sha256: previous.clone(),
        project_revision: project.revision.clone(),
        project_output: output.into(),
        project_sha256: previous.clone(),
        affected_decisions: vec![],
        failures: fail,
        scope: SCOPE.into(),
    };
    if !fail.is_empty() {
        return Ok((None, refused(fail)));
    }
    let mut next = project.clone();
    next.revision = revision.into();
    let d = next
        .decisions
        .iter_mut()
        .find(|d| d.id == request.decision_id)
        .expect("checked above");
    d.conclusion = request.conclusion.clone();
    d.basis = basis.iter().cloned().collect();
    // Results that speak for the decision were verified against the current object
    // revision and a Project source; the decision now rests on that binding.
    if let Some((rev, src)) = bindings.into_iter().next() {
        d.object_revision = rev;
        d.source_sha256 = src;
    }
    validate(&next)?;
    let affected = impact(&next, &BTreeSet::from([request.decision_id.clone()]))?;
    let affected_decisions: Vec<String> = affected["decisions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x["decision_id"].as_str())
                .filter(|x| *x != request.decision_id)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut bytes = serde_json::to_vec_pretty(&next)?;
    bytes.push(b'\n');
    let receipt = DecisionUpdateReceipt {
        schema: RECEIPT_SCHEMA.into(),
        status: "updated".into(),
        decision_id: request.decision_id.clone(),
        previous_conclusion,
        conclusion: request.conclusion.clone(),
        basis: basis.into_iter().collect(),
        previous_project_sha256: previous,
        project_revision: revision.into(),
        project_output: output.into(),
        project_sha256: sha256(&bytes),
        affected_decisions,
        failures: vec![],
        scope: SCOPE.into(),
    };
    receipt
        .validate()
        .map_err(|f| anyhow::anyhow!("decision_update_receipt_invalid:{}", f.join(",")))?;
    Ok((Some(bytes), receipt))
}
