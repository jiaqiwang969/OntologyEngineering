//! Engineering activities consume the existing Project, never a second ledger.
use anyhow::{Context, Result, bail, ensure};
use oe_contracts::{
    EngineeringCalculation, EngineeringGap, EngineeringSourceRequest, ObjectIdentity,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oe_kernel::project::Project;
use oe_knowledge::{poppler::PopplerTools, sources::Provider};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};

pub mod exploration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkInput {
    pub schema: String,
    pub project: Reference,
    pub object: ObjectIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_context: Option<oe_contracts::ProjectReviewContext>,
    pub acceptance: Vec<String>,
    pub necessary_functions: Vec<String>,
    pub physical_mechanisms: Vec<String>,
    pub gaps: Vec<EngineeringGap>,
    pub source_requests: Vec<EngineeringSourceRequest>,
    pub calculations: Vec<EngineeringCalculation>,
    pub handoff: Option<Reference>,
}
fn generated(result: std::result::Result<(), Vec<String>>) -> Result<()> {
    result.map_err(|_| anyhow::anyhow!("engineering_generated_contract_invalid"))
}
fn stated(values: &[String]) -> bool {
    !values.is_empty() && values.iter().all(|s| !s.trim().is_empty())
}
pub fn read_reference(root: &SkillRoot, reference: &Reference) -> Result<Vec<u8>> {
    let path = root.data_expand(&reference.path)?;
    ensure!(
        path.starts_with(root.data_path()),
        "engineering_reference_outside_root"
    );
    let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&bytes) == reference.sha256,
        "engineering_reference_drift"
    );
    Ok(bytes)
}
pub fn verify_project_binding(project: &Project, work: &WorkInput) -> Result<()> {
    oe_kernel::project::validate(project)?;
    generated(work.object.validate())?;
    ensure!(
        [
            "oev2.engineering-work/1",
            "oev2.engineering-work/2",
            "oev2.engineering-work/3"
        ]
        .contains(&work.schema.as_str()),
        "engineering_work_schema_invalid"
    );
    if work.schema != "oev2.engineering-work/1" || work.project_context.is_some() {
        ensure!(
            oe_kernel::project::check_current_context(
                project,
                &work.object,
                work.project_context.as_ref()
            )
            .conclusion
                == "pass",
            "engineering_current_project_context_changed_or_missing"
        );
    }
    ensure!(
        project.objects.get(&work.object.object_id) == Some(&work.object.revision)
            && project
                .conditions
                .contains_key(&work.object.condition_scope)
            && project
                .evidence
                .values()
                .any(|s| s == &work.object.source_sha256),
        "engineering_work_binding_mismatch"
    );
    ensure!(
        stated(&work.acceptance)
            && stated(&work.necessary_functions)
            && stated(&work.physical_mechanisms),
        "engineering_work_purpose_missing"
    );
    if work.schema == "oev2.engineering-work/3" {
        let basis = oe_kernel::engineering::basis(project, &work.object)?
            .context("engineering_current_basis_missing")?;
        ensure!(
            basis.acceptance == work.acceptance,
            "engineering_acceptance_changed"
        );
    }
    ensure!(
        work.gaps.len() <= 32 && work.source_requests.len() <= 32 && work.calculations.len() <= 32,
        "engineering_work_budget"
    );
    let mut gaps = BTreeSet::new();
    for gap in &work.gaps {
        generated(gap.validate())?;
        ensure!(
            gaps.insert(gap.gap_id.clone())
                && stated(&gap.required_conditions)
                && stated(&gap.needed_evidence),
            "engineering_gap_invalid"
        );
    }
    for request in &work.source_requests {
        generated(request.validate())?;
        ensure!(
            gaps.contains(&request.gap_id) && (1..=4).contains(&request.maximum_pages),
            "engineering_source_request_unbound"
        );
        let document = match request.source_role.as_str() {
            "ontology_method" => "vol1",
            "trust_method" => "vol2",
            "strength" => "roloff-matek",
            "kinematics" => "norton",
            "tolerances" => "interchangeability",
            "dfma" => "dfma",
            "supplier" => {
                ensure!(
                    request.provider == "misumi" && request.document_id == "misumi-offline",
                    "engineering_source_role_mismatch"
                );
                continue;
            }
            _ => bail!("engineering_source_role_unsupported"),
        };
        ensure!(
            request.provider == "local-pdf" && request.document_id == document,
            "engineering_source_role_mismatch"
        );
    }
    let mut calculations = BTreeSet::new();
    for c in &work.calculations {
        generated(c.validate())?;
        ensure!(
            c.object == work.object && calculations.insert(c.calculation_id.clone()),
            "engineering_calculation_binding_mismatch"
        );
        ensure!(
            stated(&c.assumptions)
                && stated(&c.validity_conditions)
                && stated(&c.invalid_when)
                && stated(&c.source_refs),
            "engineering_calculation_boundary_missing"
        );
        ensure!(
            c.source_refs
                .iter()
                .all(|s| project.evidence.contains_key(s)),
            "engineering_calculation_source_unbound"
        );
    }
    Ok(())
}
/// The shared kernel owns the same tested deterministic models for work,
/// formal review, Harness and experience; physical adoption is separate.
pub fn calculate(c: &EngineeringCalculation) -> Result<Value> {
    oe_kernel::calculation::calculate(c)
}

/// The citation DTO's excerpt is a window object, never a bare string. Preserve
/// empty/native-missing text so a model cannot receive a fabricated transcript.
pub fn source_summary(page: &Value) -> Value {
    let text = page["excerpt"]["text"].as_str().unwrap_or("");
    json!({"id":page["id"],"source_pdf":page["source_pdf"],"source_sha256":page["source_sha256"],
        "source_locator":page["source_locator"],"pdf_page":page["pdf_page"],"text_origin":page["text_origin"],
        "excerpt":text.chars().take(700).collect::<String>(),"text_available":!text.trim().is_empty(),
        "native_text_missing":page["native_text_missing"],"authority":"source candidate; applicability not established"})
}
pub fn text_coverage(pages: &[Value]) -> &'static str {
    if pages.is_empty() {
        "no_results"
    } else if pages
        .iter()
        .any(|p| p["native_text_missing"] == true || p["navigation_only"] == true)
    {
        "partial_text_coverage"
    } else {
        "source_candidates"
    }
}
pub fn run(root: &SkillRoot, work: &WorkInput, tools: &PopplerTools) -> Result<Value> {
    let project_bytes = read_reference(root, &work.project)?;
    let project: Project = serde_json::from_value(oe_core::json::strict_json(&project_bytes)?)?;
    verify_project_binding(&project, work)?;
    let mut source_results = Vec::new();
    for request in &work.source_requests {
        let attempt = (|| -> Result<Vec<Value>> {
            let provider = Provider::open(root, &request.provider, None)?;
            let rows = if request.provider == "local-pdf" {
                provider.retrieve_document(
                    &request.query,
                    request.maximum_pages as usize,
                    &request.document_id,
                )?
            } else {
                provider.retrieve(&request.query, request.maximum_pages as usize, &[])?
            };
            rows.iter()
                .map(|row| provider.source_result(row, &request.query, tools))
                .collect()
        })();
        source_results.push(match attempt {
            Ok(pages)=>json!({"gap_id":request.gap_id,"source_role":request.source_role,"document_id":request.document_id,"query":request.query,
                "execution":"completed","coverage":text_coverage(&pages),"pages":pages,"applicability":"not_established","engineering_adoption":"not_established"}),
            Err(e)=>json!({"gap_id":request.gap_id,"source_role":request.source_role,"document_id":request.document_id,"execution":"failed","coverage":"unknown","reason":e.to_string(),"pages":[],"applicability":"unknown","engineering_adoption":"not_established"}),
        });
    }
    let engineering_checks = oe_kernel::engineering::checks_current(root, &project, &work.object);
    let calculations:Vec<_>=work.calculations.iter().map(|c| {
        let applicability=oe_kernel::engineering::calculation_check(&project,c);
        let mut value=match calculate(c){Ok(r)=>r,Err(e)=>json!({"calculation_id":c.calculation_id,"execution":"failed","conclusion":"unknown","reason":e.to_string(),"engineering_adoption":"not_established"})};
        // Preserve the arithmetic diagnostic and its original inputs. A stale
        // model's valid arithmetic is not evidence for the current decision.
        value["current_support"]=json!(if applicability.conclusion=="pass" {"declared_model_applicable"} else {"not_established"});
        value["model_applicability"]=json!(applicability);
        value
    }).collect();
    let handoff = if let Some(reference) = &work.handoff {
        let bytes = read_reference(root, reference)?;
        let identity = oe_core::json::strict_json(&bytes)?;
        ensure!(
            identity["project"]["project_id"] == project.project_id
                && identity["project"]["model_revision"] == work.object.revision,
            "engineering_handoff_binding_mismatch"
        );
        let path = root.data_expand(&reference.path)?;
        let packet =
            oe_cad::handoff::verify(&path, path.parent().context("engineering_handoff_parent")?)?;
        ensure!(
            read_reference(root, reference)? == bytes,
            "engineering_handoff_changed_during_work"
        );
        json!({"execution":"completed","integrity":packet.audit(),"cad_to_process":packet.requests(oe_cad::handoff::QuestionDirection::CadToProcess),
            "process_to_cad":packet.requests(oe_cad::handoff::QuestionDirection::ProcessToCad),"native_tool_execution":"not_run","engineering_adoption":"not_established"})
    } else {
        json!({"execution":"not_run","conclusion":"unknown","required":"version-bound CAD/assembly/manufacturing/inspection evidence","engineering_adoption":"not_established"})
    };
    ensure!(
        read_reference(root, &work.project)? == project_bytes,
        "engineering_project_changed_during_work"
    );
    let methods = oe_knowledge::policy::load(root)?;
    let partial = source_results
        .iter()
        .any(|r| r["execution"] != "completed" || r["coverage"] != "source_candidates")
        || calculations.iter().any(|r| {
            r["execution"] != "completed" || r["model_applicability"]["conclusion"] != "pass"
        })
        || engineering_checks.iter().any(|r| r.conclusion != "pass");
    Ok(
        json!({"schema":"oev2.engineering-work-result/1","status":if partial{"partial"}else{"analysis_completed"},"project_id":project.project_id,"project_revision":project.revision,
        "project_reference":work.project,"object":work.object,"purpose":project.purpose,"acceptance":work.acceptance,"necessary_functions":work.necessary_functions,"physical_mechanisms":work.physical_mechanisms,
        "current_project_check":oe_kernel::project::check_current_context(&project,&work.object,work.project_context.as_ref()),
        "engineering_checks":engineering_checks,
        "current_use":if work.project_context.is_some() {"content_bound_models_require_engineering_evidence"} else {"historical_snapshot_only_current_conditions_unverified"},
        "gaps":work.gaps,"source_results":source_results,"calculations":calculations,"method_source_availability":methods.value["method_bundle"]["source_availability"],
        "handoff":handoff,"verification":"source candidates and declared models only; native CAD, manufacture, calibrated measurement and cost evidence require their own checks",
        "next_checks":work.gaps.iter().map(|g|json!({"gap_id":g.gap_id,"required_conditions":g.required_conditions,"activities":g.needed_evidence})).collect::<Vec<_>>(),
        "semantic_review":{"execution":"not_run","conclusion":"unknown","entry":"oev2 kernel review with the same Project/source/object/condition binding"},
        "engineering_conclusion":"unknown","adopted":false,"cost":{"status":"unknown"},"history":"existing Project and Harness; no parallel project state","language_model_training":"not_performed"}),
    )
}
