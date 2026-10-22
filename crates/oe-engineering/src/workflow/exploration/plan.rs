//! Strict private activity bodies follow the existing WorkInput pattern.
//! Field meaning source is pinned into the generated specification and compiler.
use oe_contracts::{ArtifactReference, DesignComparisonRule};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const FIELD_SOURCE: &[u8] =
    include_bytes!("../../../../../specs/owner-schemas/engineering-exploration.json");
pub(super) const FIELD_SHA256: &str =
    "ed999b9cae1e7c12e98c4cc9fb41780e542f57e8f979333ca3b5c9b87d69491b";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EngineeringExplorationHypothesis {
    pub hypothesis_id: String,
    pub decision_id: String,
    pub candidate_id: String,
    pub gap_id: String,
    pub statement: String,
    pub kind: String,
    pub grounding: String,
    pub source_refs: Vec<ArtifactReference>,
    pub scope: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EngineeringExplorationCandidate {
    pub candidate_id: String,
    pub work: ArtifactReference,
    pub principle: String,
    pub representation: String,
    pub applicability: Vec<String>,
    pub source_refs: Vec<ArtifactReference>,
    pub remaining_obligations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EngineeringExplorationAction {
    pub action_id: String,
    pub candidate_id: String,
    pub gap_id: String,
    pub requirement: String,
    pub kind: String,
    pub calculation_id: Option<String>,
    pub source: Option<ArtifactReference>,
    pub pointer: String,
    pub hypotheses: Vec<String>,
    pub purpose: String,
    pub expected_impact: String,
    pub required_facts: Vec<String>,
    pub when_outcomes: Vec<String>,
    pub selection_reason: String,
    pub cost_basis: String,
    pub cost_sources: Vec<ArtifactReference>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EngineeringExplorationOutcome {
    pub outcome_id: String,
    pub action_id: String,
    pub operator: String,
    pub value: Value,
    pub supported: Vec<String>,
    pub rejected: Vec<String>,
    pub review: Vec<String>,
    pub retained_candidates: Vec<String>,
    pub rejected_candidates: Vec<String>,
    pub review_candidates: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EngineeringExplorationPlan {
    pub schema: String,
    pub exploration_id: String,
    pub parent_work: ArtifactReference,
    pub comparison_mode: String,
    pub parameter_rules: Vec<DesignComparisonRule>,
    pub comparison_extension_reason: String,
    pub prior_contract: Option<ArtifactReference>,
    pub observations: Vec<ArtifactReference>,
    pub unknowns: Vec<String>,
    pub candidates: Vec<EngineeringExplorationCandidate>,
    pub hypotheses: Vec<EngineeringExplorationHypothesis>,
    pub actions: Vec<EngineeringExplorationAction>,
    pub outcomes: Vec<EngineeringExplorationOutcome>,
    pub independent_expected: ArtifactReference,
    pub exposure: String,
}
impl EngineeringExplorationPlan {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        if ["parameter", "mechanism"].contains(&self.comparison_mode.as_str()) {
            Ok(())
        } else {
            Err(vec!["exploration_comparison_mode_invalid".into()])
        }
    }
}
impl EngineeringExplorationHypothesis {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        if ["source_statement", "model_prediction", "causal_hypothesis"]
            .contains(&self.kind.as_str())
            && ["hypothesis", "grounded_explanation"].contains(&self.grounding.as_str())
        {
            Ok(())
        } else {
            Err(vec!["exploration_hypothesis_kind_invalid".into()])
        }
    }
}
impl EngineeringExplorationCandidate {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        self.work.validate()
    }
}
impl EngineeringExplorationAction {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        if ["calculation", "integrity", "readback", "external"].contains(&self.kind.as_str())
            && ["unknown", "source_estimate"].contains(&self.cost_basis.as_str())
        {
            Ok(())
        } else {
            Err(vec!["exploration_action_kind_invalid".into()])
        }
    }
}
impl EngineeringExplorationOutcome {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        if ["eq", "ne", "lt", "le", "gt", "ge", "present", "absent"]
            .contains(&self.operator.as_str())
        {
            Ok(())
        } else {
            Err(vec!["exploration_outcome_operator_invalid".into()])
        }
    }
}
