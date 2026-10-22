//! Existing decision-review/v1 meaning, on the one verified current registry.
//! The frozen decision_review.py contract requires SELECT ?finding, not a
//! naming convention: each returned finding must be bound and nonempty.
//! No typed Project, fact admission, promotion or adoption is performed here.
use crate::{
    books::Catalog,
    dataset::DatasetSession,
    refinery::{RefineryEngine, hash, text, valid_sha},
    refinery_contracts as wire,
    refinery_subject::CasProvider,
};
use anyhow::{Context, Result, ensure};
use oxigraph::model::{NamedNode, Term};
use semantica_contracts::{
    SemanticPromotedReviewPort1, SemanticPromotedReviewRequest1, SemanticPromotedReviewResult1,
    SemanticRuntimeFailure,
};
use serde_json::{Value, json};

pub const REQUEST: &str = "semantica.promoted-current-review-request/1";
pub const RESULT: &str = "semantica.promoted-current-review-result/1";
pub const REVIEW_CONTRACT: &str = "semantica.ontology.decision-review/v1";
pub const PROFILE: &str = "semantica.promoted-current-review-rust/default-graph/1";

fn canonical_sort(values: &mut Vec<Value>) -> Result<()> {
    let mut keyed = values
        .drain(..)
        .map(|v| Ok((semantica_contracts::json::canonical_json(&v)?, v)))
        .collect::<Result<Vec<_>>>()?;
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    values.extend(keyed.into_iter().map(|(_, value)| value));
    Ok(())
}

fn term_v1(term: &Value) -> Result<Value> {
    let kind = match term["kind"].as_str() {
        Some("iri") => "iri",
        Some("blank") => "blank_node",
        Some("literal") => "literal",
        _ => anyhow::bail!("review_unknown_rdf_term"),
    };
    let value = term["value"].as_str().context("review_rdf_value_missing")?;
    let language = term.get("language").cloned().unwrap_or(Value::Null);
    let datatype = if language.is_null() {
        term.get("datatype").cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    Ok(json!({"value":value,"term_type":kind,"datatype":datatype,"language":language}))
}

// SHACL stores N-Triples-style display terms. Parse them using the existing
// RDF parser before taking the lexical value; stripping angle/quote characters
// would corrupt escaped literals and blank-node identity.
fn display_term_value(serialized: &str) -> Result<String> {
    let graph = crate::graph::load(
        format!("<urn:review:term> <urn:review:value> {serialized} .").as_bytes(),
    )?;
    ensure!(graph.len() == 1, "review_violation_term_invalid");
    let term = graph
        .iter()
        .next()
        .context("review_violation_term_missing")?
        .object
        .into_owned();
    Ok(match term {
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(n) => n.as_str().to_owned(),
        Term::Literal(l) => l.value().to_owned(),
    })
}

fn local_name(value: &str) -> &str {
    value
        .rsplit('#')
        .next()
        .unwrap_or(value)
        .rsplit('/')
        .next()
        .unwrap_or(value)
}

/// Existing v1's five-field transport shape (runtime.py:2132-2174), retaining
/// this Rust engine's actual messages. Full raw shape results remain separate.
pub fn violations_v1(raw: &Value) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    for item in raw.as_array().context("review_shape_findings_missing")? {
        let focus = display_term_value(
            item["focus"]
                .as_str()
                .context("review_shape_focus_missing")?,
        )?;
        let path = item["path"].as_str().map(display_term_value).transpose()?;
        let source = local_name(item["source_constraint_component"].as_str().unwrap_or(""));
        let severity = local_name(item["severity"].as_str().unwrap_or(""));
        let mut messages = item["messages"]
            .as_array()
            .context("review_shape_messages_missing")?
            .iter()
            .map(|term| {
                term["value"]
                    .as_str()
                    .map(str::to_owned)
                    .context("review_shape_message_value_missing")
            })
            .collect::<Result<Vec<_>>>()?;
        if messages.is_empty() {
            // The Rust engine emits one default message when none is declared.
            messages.push(
                item["message"]
                    .as_str()
                    .context("review_shape_message_missing")?
                    .into(),
            );
        }
        messages.sort();
        result.push(json!({"focus":focus,"path":path,"source_constraint":source,
            "severity":if severity.is_empty(){"Violation"}else{severity},"message":messages.join("\n")}));
    }
    canonical_sort(&mut result)?;
    Ok(result)
}

fn failed(phase: &str, error: impl std::fmt::Display) -> Value {
    json!({"phase":phase,"code":"promoted_review_operation_failed_or_unsupported",
        "message":error.to_string(),"retryable":false})
}

fn observation(phase: &str, result: Result<Value>, failures: &mut Vec<Value>) -> Value {
    match result {
        Ok(result) => json!({"execution":"completed","result":result}),
        Err(error) => {
            let failure = failed(phase, format!("{error:#}"));
            failures.push(failure.clone());
            json!({"execution":"failed","result":null,"failure":failure})
        }
    }
}

/// Formal v1 interpretation: SELECT must expose finding and every actual row
/// must bind a nonempty RDF value. Other query results remain observations.
pub fn interpret_findings_v1(query: &Value) -> Result<Vec<Value>> {
    ensure!(query["kind"] == "select", "decision_query_must_be_select");
    ensure!(
        query["columns"]
            .as_array()
            .is_some_and(|v| v.iter().any(|c| c == "finding")),
        "decision_query_must_select_finding"
    );
    let mut findings = Vec::new();
    for row in query["rows"]
        .as_array()
        .context("decision_query_rows_missing")?
    {
        let value = row["finding"]["value"]
            .as_str()
            .context("decision_query_unbound_finding")?;
        ensure!(
            !semantica_contracts::json::python_strip(value).is_empty(),
            "decision_query_empty_finding"
        );
        let bindings = row
            .as_object()
            .context("decision_query_row_invalid")?
            .iter()
            .map(|(key, term)| Ok((key.clone(), term_v1(term)?)))
            .collect::<Result<serde_json::Map<String, Value>>>()?;
        findings.push(json!({"finding":value,"source":"package-query","bindings":bindings}));
    }
    canonical_sort(&mut findings)?;
    Ok(findings)
}

fn focus(session: &mut DatasetSession, request: &SemanticPromotedReviewRequest1) -> Result<Value> {
    let focus = NamedNode::new(&request.focus_iri)?;
    let class = NamedNode::new(&request.focus_type_iri)?;
    let present = session
        .query(&format!("ASK {{ {focus} ?p ?o }}"), false)?
        .result;
    let typed = session
        .query(&format!("ASK {{ {focus} a {class} }}"), false)?
        .result;
    let instances = session
        .query(
            &format!("SELECT DISTINCT ?s WHERE {{ ?s a {class} }}"),
            false,
        )?
        .result;
    Ok(
        json!({"focus_present":present["value"],"focus_type_present":typed["value"],
        "focus_instance_count":instances["rows"].as_array().context("focus_rows_missing")?.len(),
        "origin":"evidence_before_package_load","revision":session.revision()}),
    )
}

fn selected<'a>(manifest: &'a Value, id: &str, roles: &[&str]) -> Result<&'a Value> {
    let assets = manifest["assets"]
        .as_array()
        .context("review_assets_missing")?;
    let matched: Vec<_> = assets.iter().filter(|a| a["asset_id"] == id).collect();
    ensure!(matched.len() == 1, "review_asset_not_unique");
    let asset = matched[0];
    ensure!(
        asset["role"].as_str().is_some_and(|r| roles.contains(&r))
            && asset["load_into_dataset"] == false,
        "review_asset_wrong_role_or_preloaded"
    );
    Ok(asset)
}

impl RefineryEngine<'_> {
    /// Caller holds the existing store lock. resolve_package retains the
    /// registry owner's existing pending-event pointer recovery behavior.
    pub fn review_promoted_current(
        &self,
        request: &SemanticPromotedReviewRequest1,
    ) -> Result<SemanticPromotedReviewResult1> {
        request
            .validate()
            .map_err(|e| anyhow::anyhow!("review_contract:{e:?}"))?;
        ensure!(
            request.schema == REQUEST && request.review_contract == REVIEW_CONTRACT,
            "review_contract_mismatch"
        );
        ensure!(
            request.registry_id == self.registry_id,
            "review_registry_identity_mismatch"
        );
        valid_sha(&request.expected_registry_package_sha256)?;
        valid_sha(&request.evidence_sha256)?;
        ensure!(
            !request.evidence_bytes.is_empty() && request.evidence_bytes.len() <= crate::MAX_BYTES,
            "review_evidence_byte_budget"
        );
        ensure!(
            crate::sha256(&request.evidence_bytes) == request.evidence_sha256,
            "review_evidence_hash_mismatch"
        );
        std::str::from_utf8(&request.evidence_bytes).context("review_evidence_not_utf8")?;
        for field in [
            &request.package_id,
            &request.expected_current_version,
            &request.evidence_id,
            &request.evidence_uri,
            &request.scope_id,
            &request.query_asset_id,
            &request.shape_asset_id,
        ] {
            ensure!(!field.trim().is_empty(), "review_empty_field");
        }
        NamedNode::new(&request.focus_iri)?;
        NamedNode::new(&request.focus_type_iri)?;
        wire::timestamp(&json!(request.evidence_captured_at))?;
        wire::timestamp(&json!(request.created_at))?;

        // None is essential: a surviving old version cannot bypass current.
        let descriptor = self.resolve_package(&request.package_id, None)?;
        ensure!(
            descriptor["version"] == request.expected_current_version
                && descriptor["package_sha256"] == request.expected_registry_package_sha256,
            "review_current_baseline_mismatch"
        );
        self.verify_published_refinement(text(&descriptor, "delta_sha256")?)?;
        let manifest = self.object(text(&descriptor, "manifest_sha256")?)?;
        let (execution, projection) = self.execution_projection(&manifest)?;
        for asset in execution["assets"]
            .as_array()
            .context("review_assets_missing")?
        {
            ensure!(
                asset["load_into_dataset"] != true || asset["role"] == "ontology",
                "review_package_must_not_preload_instance_or_case_data"
            );
        }
        let query_asset = selected(&execution, &request.query_asset_id, &["sparql"])?;
        let shape_asset = selected(&execution, &request.shape_asset_id, &["shacl", "shapes"])?;
        let catalog = Catalog::from_execution_manifest(&execution)?;
        let provider = CasProvider { engine: self };
        let query_bytes = catalog.asset(&provider, &request.package_id, &request.query_asset_id)?;
        let shape_bytes = catalog.asset(&provider, &request.package_id, &request.shape_asset_id)?;
        let format = crate::packages::rdf_format(Some(&request.evidence_format))?;
        let mut session = DatasetSession::new();
        session.load(&request.evidence_bytes, format, None, None)?;
        let input = session.snapshot()?;
        let mut failures = Vec::new();
        // Evidence-origin focus is measured before the same session loads the
        // ontology, exactly as v1. A package individual cannot supply it.
        let focus_observation = observation("focus", focus(&mut session, request), &mut failures);
        let loaded = catalog.load_package(&provider, &request.package_id, &mut session)?;
        let evaluated = session.snapshot()?;
        let query_observation = observation(
            "query",
            (|| {
                let text = std::str::from_utf8(&query_bytes).context("review_query_not_utf8")?;
                Ok(session.query(text, false)?.result)
            })(),
            &mut failures,
        );
        let shape_observation = observation(
            "shape",
            (|| {
                ensure!(
                    shape_asset["format"]
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case("turtle")),
                    "review_shape_format_unsupported"
                );
                crate::shacl::validate_dataset(session.dataset(), &shape_bytes)
            })(),
            &mut failures,
        );

        let mut findings = Vec::new();
        let interpretation = if query_observation["execution"] == "completed" {
            match interpret_findings_v1(&query_observation["result"]) {
                Ok(actual) => {
                    findings.extend(actual);
                    json!({"status":"supported","contract":REVIEW_CONTRACT,
                        "rule":"SELECT finding; every returned finding bound and nonempty"})
                }
                Err(error) => {
                    failures.push(failed("query_interpretation", error));
                    json!({"status":"unknown","contract":REVIEW_CONTRACT})
                }
            }
        } else {
            json!({"status":"unknown","contract":REVIEW_CONTRACT})
        };
        let focus_result = &focus_observation["result"];
        if input.quad_count == 0 {
            findings.push(json!({"finding":"input:empty-abox","source":"runtime"}));
        }
        // An execution failure leaves focus unknown; it is not an observed
        // absence, wrong type, or non-singleton evidence set.
        if focus_observation["execution"] == "completed" {
            for (ok, code) in [
                (focus_result["focus_present"] == true, "input:focus-absent"),
                (
                    focus_result["focus_type_present"] == true,
                    "input:focus-type-mismatch",
                ),
                (
                    focus_result["focus_instance_count"] == 1,
                    "input:focus-not-singleton",
                ),
            ] {
                if !ok {
                    findings.push(json!({"finding":code,"source":"runtime"}));
                }
            }
        }
        let shape_result = &shape_observation["result"];
        let shape_targets_focus = shape_result["focus_nodes"].as_array().is_some_and(|nodes| {
            nodes
                .iter()
                .any(|node| node["kind"] == "iri" && node["value"] == request.focus_iri)
        });
        if shape_observation["execution"] == "completed" && !shape_targets_focus {
            failures.push(failed(
                "shape_coverage",
                "review_shape_does_not_target_evidence_focus",
            ));
        }
        let violations = if shape_observation["execution"] == "completed" {
            match violations_v1(&shape_result["findings"]) {
                Ok(v) => v,
                Err(error) => {
                    failures.push(failed("violation_projection", error));
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        canonical_sort(&mut findings)?;
        // Recheck current identity and every committed asset after observation.
        let final_identity = (|| -> Result<()> {
            ensure!(
                self.resolve_package(&request.package_id, None)? == descriptor,
                "review_current_package_changed_during_execution"
            );
            self.verify_published_refinement(text(&descriptor, "delta_sha256")?)?;
            ensure!(
                self.execution_projection(&manifest)? == (execution.clone(), projection.clone()),
                "review_package_assets_changed_during_execution"
            );
            Ok(())
        })();
        if let Err(error) = final_identity {
            failures.push(failed("package_recheck", error));
        }
        let all_executed = [&query_observation, &shape_observation, &focus_observation]
            .iter()
            .all(|o| o["execution"] == "completed");
        let clear = all_executed
            && failures.is_empty()
            && findings.is_empty()
            && violations.is_empty()
            && shape_result["conforms"] == true
            && shape_targets_focus;
        let conclusion = if clear {
            "pass"
        } else if !findings.is_empty() || shape_result["conforms"] == false {
            "violated"
        } else {
            "unknown"
        };
        let status = if clear { "clear" } else { "blocked" };
        let execution_status = if all_executed { "completed" } else { "partial" };
        let evidence = json!({"id":request.evidence_id,"uri":request.evidence_uri,
            "sha256":request.evidence_sha256,"format":request.evidence_format,
            "captured_at":request.evidence_captured_at,"input_quad_count":input.quad_count,
            "input_dataset_sha256":input.dataset_sha256,
            "evaluated_dataset_sha256":evaluated.dataset_sha256});
        let mut review = json!({"contract":REVIEW_CONTRACT,"status":status,
            "engine_id":crate::ENGINE_ID,"execution_profile":PROFILE,
            "dto_compatibility":"existing v1 field semantics; current Rust RDF-term normalization and diagnostic messages",
            "scope_id":request.scope_id,"focus_iri":request.focus_iri,
            "focus_type_iri":request.focus_type_iri,"evidence":evidence,
            "package":{"id":request.package_id,"version":descriptor["version"],
                "digest":loaded["identity"]["digest"],"registry_package_sha256":descriptor["package_sha256"],
                "origin":"promoted_registry","query_asset_id":request.query_asset_id,
                "shape_asset_id":request.shape_asset_id,"query_asset_sha256":query_asset["sha256"],
                "shape_asset_sha256":shape_asset["sha256"]},
            "focus_present":focus_result["focus_present"],"focus_type_present":focus_result["focus_type_present"],
            "focus_instance_count":focus_result["focus_instance_count"],"shape_conforms":shape_result["conforms"],
            "findings":findings,"violations":violations,"created_at":request.created_at,
            "execution":execution_status,"conclusion":conclusion,"failures":failures,
            "meaning":"scoped semantic-record-consistency-only; typed Project not checked; no physical acceptance"});
        review["report_sha256"] = json!(hash(&review)?);
        let mut result = SemanticPromotedReviewResult1 {
            schema: RESULT.into(),
            subject: json!({"registry_id":self.registry_id,"descriptor":descriptor,
                "execution_projection_sha256":projection["_projection_sha256"]}),
            package_assets: json!({"query":query_asset,"shape":shape_asset}),
            evidence,
            dataset_observation: json!({"input":input,"evaluated":evaluated,
                "shape_data_view":"default-graph-only","query_data_view":"dataset",
                "shape_focus_coverage":"target enumeration only; deactivated shapes may enumerate focus without evaluating constraints",
                "active_constraint_coverage":"unknown","engineering_required_checks":"not_evaluated",
                "package_load":loaded}),
            query_observation,
            shape_observation,
            focus_observation,
            interpretation,
            execution: execution_status.into(),
            conclusion: conclusion.into(),
            coverage: vec![
                request.scope_id.clone(),
                request.query_asset_id.clone(),
                request.shape_asset_id.clone(),
                request.focus_iri.clone(),
            ],
            failures,
            review_v1: review,
            engineering_adoption: "unknown".into(),
            typed_project_checks: "not_run".into(),
            fact_admitted: false,
            report_sha256: String::new(),
        };
        let mut content = serde_json::to_value(&result)?;
        content
            .as_object_mut()
            .context("review_result_mapping")?
            .remove("report_sha256");
        result.report_sha256 = hash(&content)?;
        result
            .validate()
            .map_err(|e| anyhow::anyhow!("review_result_contract:{e:?}"))?;
        Ok(result)
    }
}

impl SemanticPromotedReviewPort1 for RefineryEngine<'_> {
    fn review_promoted_current(
        &self,
        input: &SemanticPromotedReviewRequest1,
    ) -> std::result::Result<SemanticPromotedReviewResult1, SemanticRuntimeFailure> {
        RefineryEngine::review_promoted_current(self, input).map_err(|error| {
            SemanticRuntimeFailure {
                code: "promoted_review_rejected".into(),
                phase: "prepare_or_integrity".into(),
                message: format!("{error:#}"),
                retryable: false,
            }
        })
    }
}
