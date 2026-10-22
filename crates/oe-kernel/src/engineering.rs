//! Declared engineering premises in the existing Project condition snapshot.
//! These checks diagnose support relations; they never certify physical truth.
use crate::{CheckResult, project::Project};
use anyhow::{Context, Result, ensure};
use oe_contracts::{EngineeringAssertion, EngineeringBasis, EngineeringFact, ObjectIdentity};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

mod goal_support;

pub const SCHEMA: &str = "oev2.engineering-basis/1";
pub const SCHEMA_V2: &str = "oev2.engineering-basis/2";

/// Validated common fields borrowed through a read-only memory view. The full
/// original condition remains the only complete wire envelope. Clone retains
/// the original bytes and protocol; this view has no Serialize, Deserialize or
/// DerefMut implementation. Its common DTO retains the honest original /1 field
/// profile, and callers dispatch the source protocol using `protocol()`.
///
/// ```compile_fail
/// fn cannot_serialize(view: oe_kernel::engineering::EngineeringBasisView) {
///     let _ = serde_json::to_value(view);
/// }
/// ```
/// ```compile_fail
/// fn cannot_serialize_a_clone(view: oe_kernel::engineering::EngineeringBasisView) {
///     let _ = serde_json::json!(view.clone());
/// }
/// ```
/// ```compile_fail
/// fn cannot_mutate_fields(view: &mut oe_kernel::engineering::EngineeringBasisView) {
///     view.acceptance.clear();
/// }
/// ```
#[derive(Debug, Clone)]
pub struct EngineeringBasisView {
    common: EngineeringBasis,
    source_protocol: String,
    original_condition: String,
    original_value: Value,
    original_sha256: String,
}
impl std::ops::Deref for EngineeringBasisView {
    type Target = EngineeringBasis;
    fn deref(&self) -> &Self::Target {
        &self.common
    }
}
impl EngineeringBasisView {
    pub fn protocol(&self) -> &str {
        &self.source_protocol
    }
    /// Exact full UTF-8 bytes from the same Project condition, including whitespace.
    pub fn original_condition(&self) -> &str {
        &self.original_condition
    }
    pub fn original_value(&self) -> &Value {
        &self.original_value
    }
    pub fn condition_sha256(&self) -> &str {
        &self.original_sha256
    }
}

/// Natural-language conditions remain readable historical inputs. The agent
/// normalizes a current task into this versioned envelope inside the same scope.
pub fn basis(project: &Project, object: &ObjectIdentity) -> Result<Option<EngineeringBasisView>> {
    let raw = project
        .conditions
        .get(&object.condition_scope)
        .context("engineering_condition_scope_missing")?;
    if !raw.trim_start().starts_with('{') {
        return Ok(None);
    }
    let mut value = oe_core::json::strict_json(raw.as_bytes())?;
    let Some(schema) = value["schema"].as_str() else {
        return Ok(None);
    };
    if !schema.starts_with("oev2.engineering-basis/") {
        return Ok(None);
    }
    ensure!(
        schema == SCHEMA || schema == SCHEMA_V2,
        "engineering_basis_version_unsupported"
    );
    let protocol = schema.to_owned();
    let original_value = value.clone();
    // Reuse only the original field validation. The original /2 condition and
    // body digest remain unchanged, and a broken support envelope must not
    // suppress independently executable model, numeric or CAD diagnostics.
    if protocol == SCHEMA_V2 {
        let fields = value.as_object_mut().context("engineering_basis_invalid")?;
        fields.remove("goal_support");
        fields.insert("schema".into(), Value::String(SCHEMA.into()));
    }
    let basis: EngineeringBasis = serde_json::from_value(value)?;
    basis
        .validate()
        .map_err(|_| anyhow::anyhow!("engineering_basis_invalid"))?;
    ensure!(
        basis.object_id == object.object_id && basis.object_revision == object.revision,
        "engineering_basis_object_mismatch"
    );
    ensure!(
        basis.facts.len() <= 128 && basis.assertions.len() <= 64,
        "engineering_basis_budget"
    );
    let mut properties = BTreeSet::new();
    for fact in &basis.facts {
        fact.validate()
            .map_err(|_| anyhow::anyhow!("engineering_fact_invalid"))?;
        ensure!(
            properties.insert(&fact.property),
            "engineering_fact_ambiguous"
        );
        sources(project, &fact.source_refs)?;
    }
    let mut ids = BTreeSet::new();
    let mut models = BTreeSet::new();
    for assertion in &basis.assertions {
        assertion
            .validate()
            .map_err(|_| anyhow::anyhow!("engineering_assertion_invalid"))?;
        ensure!(
            ids.insert(&assertion.assertion_id),
            "engineering_assertion_duplicate"
        );
        sources(project, &assertion.source_refs)?;
        if assertion.kind == "model_boundary" {
            ensure!(
                models.insert(&assertion.subject),
                "engineering_model_subject_ambiguous"
            );
        }
        ensure!(
            assertion
                .affected_decisions
                .iter()
                .all(|id| project.decisions.iter().any(|d| &d.id == id)),
            "engineering_decision_unbound"
        );
    }
    Ok(Some(EngineeringBasisView {
        common: basis,
        source_protocol: protocol,
        original_condition: raw.clone(),
        original_value,
        original_sha256: oe_core::sha256(raw.as_bytes()),
    }))
}
fn sources(project: &Project, refs: &[String]) -> Result<()> {
    ensure!(
        !refs.is_empty() && refs.iter().all(|id| project.evidence.contains_key(id)),
        "engineering_premise_source_unbound"
    );
    Ok(())
}
fn fact<'a>(basis: &'a EngineeringBasis, property: &str) -> Result<&'a EngineeringFact> {
    basis
        .facts
        .iter()
        .find(|f| f.property == property)
        .with_context(|| format!("engineering_condition_missing:{property}"))
}
pub(crate) fn known<'a>(basis: &'a EngineeringBasis, property: &str) -> Result<&'a Value> {
    let f = fact(basis, property)?;
    ensure!(
        f.status == "declared" && !f.value.is_null(),
        "engineering_condition_unknown:{property}"
    );
    Ok(&f.value)
}
/// Precision text is metadata, not a numeric unit conversion or tolerance.
fn comparable(value: &Value) -> Result<Value> {
    if value.is_object() && (value.get("value").is_some() || value.get("unit").is_some()) {
        let unit = value["unit"]
            .as_str()
            .context("engineering_comparison_unit_missing")?;
        let number = value["value"]
            .as_f64()
            .context("engineering_comparison_value_invalid")?;
        ensure!(
            !unit.is_empty() && number.is_finite(),
            "engineering_comparison_value_invalid"
        );
        return Ok(json!({"value":number,"unit":unit}));
    }
    if let Some(values) = value.as_array() {
        return Ok(Value::Array(
            values.iter().map(comparable).collect::<Result<_>>()?,
        ));
    }
    ensure!(
        value.is_string() || value.is_boolean(),
        "engineering_comparison_type_unsupported"
    );
    Ok(value.clone())
}
pub(crate) fn affected(project: &Project, assertion: &EngineeringAssertion) -> Vec<String> {
    let mut changes = BTreeSet::from([assertion.assertion_id.clone(), assertion.subject.clone()]);
    changes.extend(assertion.source_refs.iter().cloned());
    // Explicitly affected decisions are seeds, not merely labels appended
    // after traversing the dependency graph. Their downstream outputs must
    // also reopen even when the claim has no separate dependency edge.
    changes.extend(assertion.affected_decisions.iter().cloned());
    let closure = crate::project::impact(project, &changes).ok();
    let mut ids: BTreeSet<_> = assertion.affected_decisions.iter().cloned().collect();
    if let Some(value) = closure {
        for d in value["decisions"].as_array().into_iter().flatten() {
            if let Some(id) = d["decision_id"].as_str() {
                ids.insert(id.into());
            }
        }
    }
    ids.into_iter().collect()
}
pub(crate) fn result(
    id: &str,
    conclusion: &str,
    reason: Option<String>,
    details: Value,
) -> CheckResult {
    CheckResult {
        id: id.into(),
        execution: "completed".into(),
        conclusion: conclusion.into(),
        coverage: vec![
            "declared engineering premises and support scope; physical truth not certified".into(),
        ],
        failures: reason.clone().into_iter().collect(),
        result: json!({"reason":reason,"details":details,
            "engineering_adoption":"not_established","premise_authority":"source-bound agent declarations; original-source review required"}),
    }
}

fn model(basis: &EngineeringBasis, requirement: &Value) -> Result<(Vec<Value>, Vec<Value>)> {
    let method = requirement["method"]
        .as_str()
        .context("engineering_model_method_missing")?;
    let input = requirement["inputs"]
        .as_object()
        .context("engineering_model_inputs_missing")?;
    let (parameters, categories): (&[&str], &[(&str, &str)]) = match method {
        "coulomb-single-contact-limit/1" => (
            &["friction_coefficient", "normal_force"],
            &[
                ("contact_model", "single_driven_coulomb_contact"),
                (
                    "normal_force_definition",
                    "resultant_normal_force_at_driven_contact",
                ),
                ("friction_regime", "static_limit"),
            ],
        ),
        "coulomb-required-friction/1" => (
            &["tangential_force", "normal_force"],
            &[
                ("contact_model", "single_driven_coulomb_contact"),
                (
                    "normal_force_definition",
                    "resultant_normal_force_at_driven_contact",
                ),
                (
                    "tangential_force_definition",
                    "required_tangential_contact_force",
                ),
                ("friction_regime", "static_limit"),
            ],
        ),
        "steady-traction-power/1" => (
            &["tangential_force", "linear_speed"],
            &[
                ("motion_regime", "steady_translation"),
                (
                    "tangential_force_definition",
                    "required_tangential_contact_force",
                ),
                ("power_boundary", "ideal_contact_mechanical_power"),
            ],
        ),
        "beam-cantilever-end-load/1" => (
            &["force", "span", "elastic_modulus", "second_moment"],
            &[
                ("support", "cantilever_fixed_free"),
                ("load_position", "end"),
                ("regime", "linear_elastic_small_deflection"),
            ],
        ),
        "beam-simple-center-load/1" => (
            &["force", "span", "elastic_modulus", "second_moment"],
            &[
                ("support", "simple_two_end"),
                ("load_position", "center"),
                ("regime", "linear_elastic_small_deflection"),
            ],
        ),
        "round-shaft-second-moment/1" => (&["diameter"], &[("section", "solid_round")]),
        "linear-error-worst-case/1" => (
            &["components"],
            &[
                ("error_model", "linear_worst_case"),
                ("coverage", "declared_complete"),
            ],
        ),
        "radial-error-2d/1" => (
            &["axial_error", "circumferential_error"],
            &[
                ("error_model", "euclidean_2d"),
                ("coverage", "declared_complete"),
            ],
        ),
        _ => anyhow::bail!("engineering_model_boundary_unsupported:{method}"),
    };
    let mut mismatches = Vec::new();
    let mut unknown = Vec::new();
    // Required premises come from the versioned model contract, not a mutable
    // list that can be shortened to make the evaluation pass.
    for parameter in parameters {
        let property = *parameter;
        match known(basis, property) {
            Ok(current) => {
                let expected = input
                    .get(*parameter)
                    .context("engineering_model_input_missing")?;
                if comparable(current)? != comparable(expected)? {
                    mismatches.push(
                        json!({"property":property,"model_input":expected,"current":current}),
                    );
                }
            }
            Err(error) => unknown.push(json!({"property":property,"reason":error.to_string()})),
        }
    }
    for (property, expected) in categories {
        match known(basis, property) {
            Ok(current) if current != &json!(expected) => mismatches
                .push(json!({"property":property,"model_requires":expected,"current":current})),
            Ok(_) => (),
            Err(error) => unknown.push(json!({"property":property,"reason":error.to_string()})),
        }
    }
    Ok((mismatches, unknown))
}
fn applicability(
    basis: &EngineeringBasis,
    requirement: &Value,
    ranges: bool,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let limits = requirement["limits"]
        .as_object()
        .context("engineering_applicability_limits_missing")?;
    ensure!(!limits.is_empty(), "engineering_applicability_limits_empty");
    let mut mismatches = Vec::new();
    let mut unknown = Vec::new();
    for (property, limit) in limits {
        let actual = match known(basis, property) {
            Ok(v) => v,
            Err(error) => {
                unknown.push(json!({"property":property,"reason":error.to_string()}));
                continue;
            }
        };
        let matched = if ranges && limit.is_object() && limit.get("maximum").is_some() {
            let unit = limit["unit"]
                .as_str()
                .context("engineering_range_unit_missing")?;
            let lower = limit["minimum"]
                .as_f64()
                .context("engineering_range_minimum_missing")?;
            let upper = limit["maximum"]
                .as_f64()
                .context("engineering_range_maximum_missing")?;
            ensure!(
                lower.is_finite() && upper.is_finite() && lower <= upper && !unit.is_empty(),
                "engineering_range_invalid"
            );
            let number = actual["value"]
                .as_f64()
                .context("engineering_range_value_invalid")?;
            actual["unit"] == unit && number.is_finite() && number >= lower && number <= upper
        } else {
            let options = limit
                .as_array()
                .context("engineering_applicability_options_invalid")?;
            ensure!(
                !options.is_empty(),
                "engineering_applicability_options_empty"
            );
            options
                .iter()
                .map(comparable)
                .collect::<Result<Vec<_>>>()?
                .contains(&comparable(actual)?)
        };
        if !matched {
            mismatches.push(json!({"property":property,"supported_scope":limit,"required":actual}));
        }
    }
    Ok((mismatches, unknown))
}

fn assertion_check(
    project: &Project,
    basis: &EngineeringBasis,
    assertion: &EngineeringAssertion,
    object: &ObjectIdentity,
) -> CheckResult {
    if assertion.kind == "cad_native_motion" {
        return crate::motion::declared(project, object, assertion);
    }
    if assertion.kind == "numeric_acceptance" {
        return crate::numeric::check(project, basis, assertion, object);
    }
    if assertion.kind == "cad_row_intent" {
        return crate::cad_intent::assertion(project, object, assertion);
    }
    if assertion.kind == "cad_drawing_reference" {
        return crate::drawing_intent::assertion(project, object, assertion);
    }
    if assertion.kind == "cad_sheet_reference_persistence" {
        return crate::sheet_persistence::declared(project, object, assertion);
    }
    if assertion.kind == "cad_drafting_sheet" {
        return crate::drafting::assertion(project, object, assertion);
    }
    if assertion.kind == "mechanism_motion" {
        return crate::mechanics::check(project, basis, assertion, object);
    }
    let id = format!("engineering:{}", assertion.assertion_id);
    let affected = affected(project, assertion);
    let attempted = (|| -> Result<CheckResult> {
        let req = &assertion.requirements;
        let (mismatches, unknown, code) = match assertion.kind.as_str() {
            "model_boundary" => {
                let (m, u) = model(basis, req)?;
                (m, u, "engineering_model_condition_mismatch")
            }
            "source_applicability" => {
                let (m, u) = applicability(basis, req, false)?;
                (m, u, "engineering_source_not_applicable")
            }
            "scope_transfer" => {
                let (m, u) = applicability(basis, req, true)?;
                (m, u, "engineering_evidence_scope_exceeded")
            }
            "function_continuity" => {
                let function = req["function_id"]
                    .as_str()
                    .context("engineering_function_identity_missing")?;
                let complete = req["inventory_scope"] == "declared_complete";
                let realization = req["current_realization"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty());
                let mechanism = req["mechanism"].as_str().filter(|s| !s.trim().is_empty());
                let verification = req["verification_plan"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty());
                if realization.is_none() || mechanism.is_none() || verification.is_none() {
                    let detail = json!({"function_id":function,"current_realization":realization,"mechanism":mechanism,"verification_plan":verification,"inventory_scope":req["inventory_scope"]});
                    (
                        if complete {
                            vec![detail.clone()]
                        } else {
                            vec![]
                        },
                        if complete { vec![] } else { vec![detail] },
                        "engineering_necessary_function_unassigned",
                    )
                } else {
                    (vec![], vec![], "engineering_function_continuity")
                }
            }
            "counterexample" => {
                let state = req["status"]
                    .as_str()
                    .context("engineering_challenge_status_missing")?;
                ensure!(
                    ["open", "resolved"].contains(&state),
                    "engineering_challenge_status_invalid"
                );
                // Closing a challenge requires a new referenced resolution,
                // not merely changing status or deleting an unfavorable result.
                if state == "resolved" {
                    let resolution = req["resolution_source"]
                        .as_str()
                        .context("engineering_challenge_resolution_missing")?;
                    ensure!(
                        project.evidence.contains_key(resolution)
                            && assertion.source_refs.iter().any(|s| s == resolution),
                        "engineering_challenge_resolution_unbound"
                    );
                    ensure!(
                        assertion.source_refs.iter().any(|id| id != resolution
                            && project.evidence.get(id) != project.evidence.get(resolution)),
                        "engineering_challenge_resolution_not_new"
                    );
                }
                (
                    if state == "open" {
                        vec![
                            json!({"target":assertion.subject,"evidence_scope":req["evidence_scope"],"status":"open","physical_failure":"not_inferred"}),
                        ]
                    } else {
                        vec![]
                    },
                    vec![],
                    "engineering_open_counterexample",
                )
            }
            _ => anyhow::bail!("engineering_assertion_unsupported"),
        };
        let (conclusion, reason) = if !mismatches.is_empty() {
            ("violated", Some(code.into()))
        } else if !unknown.is_empty() {
            (
                "unknown",
                Some("engineering_required_premise_unknown".into()),
            )
        } else {
            ("pass", None)
        };
        Ok(result(
            &id,
            conclusion,
            reason,
            json!({"subject":assertion.subject,"kind":assertion.kind,"mismatches":mismatches,"unknown":unknown,"affected_decisions":affected,
            "support_status":if conclusion=="pass"{"declared_premises_consistent"}else{"requires_review"}}),
        ))
    })();
    attempted.unwrap_or_else(|error| {
        result(
            &id,
            "unknown",
            Some(error.to_string()),
            json!({"subject":assertion.subject,"affected_decisions":affected}),
        )
    })
}

/// Appended after the original RDF/query/shape diagnostics in current review.
pub fn checks(project: &Project, object: &ObjectIdentity) -> Vec<CheckResult> {
    match basis(project, object) {
        Ok(None) => vec![], // Legacy semantics keep their original proof scope.
        Err(error) => vec![result(
            "engineering_basis",
            "unknown",
            Some(error.to_string()),
            json!({"current_use":"requires_review"}),
        )],
        Ok(Some(basis)) => {
            if basis.protocol() == SCHEMA_V2 {
                let mut out = vec![goal_support::check(
                    project,
                    object,
                    &basis,
                    basis.original_value(),
                )];
                out.extend(
                    basis
                        .assertions
                        .iter()
                        .map(|a| assertion_check(project, &basis, a, object)),
                );
                if basis.assertions.is_empty() {
                    out.push(result(
                        "engineering_assertions",
                        "unknown",
                        Some("engineering_assertions_missing".into()),
                        json!({}),
                    ));
                }
                return out;
            }
            let purpose_affected = if basis.purpose == project.purpose {
                vec![]
            } else {
                let impact =
                    crate::project::impact(project, &crate::project::purpose_changes(&[project]));
                impact
                    .ok()
                    .and_then(|v| v["decisions"].as_array().cloned())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|v| v["decision_id"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            };
            let mut out = vec![result(
                "engineering_purpose",
                if basis.purpose == project.purpose {
                    "pass"
                } else {
                    "violated"
                },
                (basis.purpose != project.purpose).then(|| "engineering_purpose_changed".into()),
                json!({"model_purpose":basis.purpose,"current_purpose":project.purpose,"affected_decisions":purpose_affected}),
            )];
            out.extend(
                basis
                    .assertions
                    .iter()
                    .map(|a| assertion_check(project, &basis, a, object)),
            );
            if basis.assertions.is_empty() {
                out.push(result(
                    "engineering_assertions",
                    "unknown",
                    Some("engineering_assertions_missing".into()),
                    json!({}),
                ));
            }
            out
        }
    }
}

/// Formal current workflows require receipt bytes as well as declared facts.
/// The pure checks API retains its historical, in-memory diagnostic scope.
pub fn checks_current(
    root: &oe_core::SkillRoot,
    project: &Project,
    object: &ObjectIdentity,
) -> Vec<CheckResult> {
    let mut out = checks(project, object);
    if let Ok(Some(basis)) = basis(project, object) {
        // This new kind has no affirmative in-memory proof. Historical kinds
        // retain their existing declared/current separation unchanged.
        for a in basis
            .assertions
            .iter()
            .filter(|a| a.kind == "cad_native_motion")
        {
            if let Some(slot) = out
                .iter_mut()
                .find(|c| c.id == format!("engineering:{}", a.assertion_id))
            {
                *slot = crate::motion::current(root, project, object, a);
            }
        }
        out.extend(
            basis
                .assertions
                .iter()
                .filter_map(|a| match a.kind.as_str() {
                    "cad_drawing_reference" => Some(crate::drawing_intent::source_assertion(
                        root, project, object, a,
                    )),
                    "cad_sheet_reference_persistence" => {
                        Some(crate::sheet_persistence::current(root, project, object, a))
                    }
                    "cad_drafting_sheet" => {
                        Some(crate::drafting::source_assertion(root, project, object, a))
                    }
                    _ => None,
                }),
        );
    }
    out
}

pub fn calculation_check(
    project: &Project,
    calculation: &oe_contracts::EngineeringCalculation,
) -> CheckResult {
    let id = format!("engineering:{}", calculation.calculation_id);
    let attempted = (|| -> Result<CheckResult> {
        let basis =
            basis(project, &calculation.object)?.context("engineering_model_basis_missing")?;
        let assertion = basis
            .assertions
            .iter()
            .find(|a| a.kind == "model_boundary" && a.subject == calculation.calculation_id)
            .context("engineering_calculation_boundary_unbound")?;
        if assertion.requirements["method"] != calculation.method
            || comparable_inputs(&assertion.requirements["inputs"])?
                != comparable_inputs(&calculation.inputs)?
        {
            return Ok(result(
                &id,
                "violated",
                Some("engineering_calculation_differs_from_declared_model".into()),
                json!({"declared_method":assertion.requirements["method"],"actual_method":calculation.method,
                    "affected_decisions":affected(project,assertion)}),
            ));
        }
        Ok(assertion_check(
            project,
            &basis,
            assertion,
            &calculation.object,
        ))
    })();
    attempted.unwrap_or_else(|error| {
        result(
            &id,
            "unknown",
            Some(error.to_string()),
            json!({"calculation_id":calculation.calculation_id}),
        )
    })
}
fn comparable_inputs(value: &Value) -> Result<BTreeMap<String, Value>> {
    value
        .as_object()
        .context("engineering_model_inputs_missing")?
        .iter()
        .map(|(key, value)| Ok((key.clone(), comparable(value)?)))
        .collect()
}
