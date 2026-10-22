//! Project lifecycle orchestration on the current source-bound Rust adapter.
//! Original data control and actual verification/engineering authority remain separate.
//! A stable response preserves execution, regression, receipt, release and
//! adoption separately. Promotion only proposes a successor project binding.

use crate::contracts::{self, ProjectBinding, SemanticTaskEnvelope};
use crate::delta;
use crate::native::{
    DecisionReviewRequest, NativeExecution, NativeRuntime, OpenEngagementRequest, ProposeRequest,
    RegistryRequest, RegistryRunRequest, TransitionRequest, VerifyRequest,
};
use crate::refinery_runtime::RustRuntime;
use crate::{canonical_json, digest, strict_json};
use anyhow::{Context, Result, ensure};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const RESPONSE_SCHEMA: &str = "ontology-engineering.semantic-engagement-response/v1";
const STATES: [&str; 6] = [
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted",
];
const MAX_DOCUMENT: usize = 16 * 1024 * 1024;

pub fn default_runtime(root: &Path) -> Result<RustRuntime> {
    RustRuntime::new(root)
}

fn learning(binding: Option<&ProjectBinding>) -> Value {
    json!({"status":"not_run", "verdict":"no_delta", "state_model":STATES,
        "current_state":null,"completed_states":[],
        "promotion":{"status":"not_requested","target":binding.map(|b|&b.promotion_target)},
        "publication":{"status":"not_requested","decision":"external_decision_required",
            "published":null,"decision_authority":binding.map(|b|&b.decision_authority)}})
}

fn envelope(
    command: &str,
    binding: Option<&ProjectBinding>,
    task: Option<&SemanticTaskEnvelope>,
) -> Value {
    json!({"$schema":RESPONSE_SCHEMA,"command":command,"command_verdict":"blocked",
        "transport":{"status":"completed","exit_code_semantics":"exit 0 confirms stable JSON delivery only; inspect every semantic section"},
        "runtime_source":{},"binding":binding.map(ProjectBinding::as_dict),
        "task":task.map(SemanticTaskEnvelope::as_dict),
        "corpus_found":{"status":"not_run"},"execution":{"status":"not_run"},
        "regression":{"status":"not_run"},"receipt":{"status":"not_run"},
        "release":{"status":"not_checked"},"learning":learning(binding),
        "capabilities":{"status":"not_checked"},"diagnostics":[]})
}

#[derive(Debug)]
struct FailedNativeResponse(Value);
impl std::fmt::Display for FailedNativeResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("source-locked native operation did not complete")
    }
}
impl std::error::Error for FailedNativeResponse {}

/// Preserve evidence already received even when an owned projection fails.
/// A completed child is not retried or reclassified as never run. This is local
/// response retention, not a claim that malformed output is authoritative.
fn finish_response(mut response: Value, result: Result<()>) -> Result<Value> {
    if let Err(error) = result {
        if error.downcast_ref::<FailedNativeResponse>().is_some() {
            return Err(error);
        }
        let kind = error
            .downcast_ref::<contracts::ContractError>()
            .map(|error| error.kind.name())
            .unwrap_or("SemanticEngagementError");
        response["command_verdict"] = "blocked".into();
        response["execution"]["status"] = "blocked".into();
        response["execution"]["error_type"] = kind.into();
        response["execution"]["message"] = error.to_string().into();
        response["learning"]["status"] = "blocked".into();
        response["learning"]["reason"] =
            "semantic engagement failed closed; retained native evidence must be inspected".into();
        response["diagnostics"]
            .as_array_mut()
            .expect("stable envelope diagnostics")
            .push(json!({"code":kind,"message":error.to_string()}));
        return Err(FailedNativeResponse(response).into());
    }
    Ok(response)
}

fn with_context(
    mut context: ContextState,
    operation: impl FnOnce(&mut ContextState) -> Result<()>,
) -> Result<Value> {
    let result = (|| {
        check_sources(&context.sources)?;
        operation(&mut context)?;
        check_sources(&context.sources)
    })();
    finish_response(context.response, result)
}

/// Emit one complete response even when a native process fails after mutation.
/// The retained NativeExecution distinguishes not_run from failed and carries
/// its raw result; an error must never be interpreted as an absent history.
pub fn error_response(command: &str, error: &anyhow::Error) -> Value {
    if let Some(failed) = error.downcast_ref::<FailedNativeResponse>() {
        return failed.0.clone();
    }
    let mut response = envelope(command, None, None);
    let kind = error
        .downcast_ref::<contracts::ContractError>()
        .map(|e| e.kind.name())
        .unwrap_or("SemanticEngagementError");
    response["execution"] = json!({"status":"blocked","error_type":kind,
        "message":error.to_string()});
    response["learning"]["status"] = "blocked".into();
    response["learning"]["reason"] = "semantic engagement failed closed".into();
    response["diagnostics"] = json!([{"code":kind,"message":error.to_string()}]);
    response
}

fn consume(response: &mut Value, execution: NativeExecution) -> Result<Value> {
    // The full execution record is retained, including stderr and a possible
    // partial native result. It is local output and is never sent to a model here.
    let retained = serde_json::to_value(&execution)?;
    if response.get("native_executions").is_none() {
        response["native_executions"] = json!([]);
    }
    response["native_executions"]
        .as_array_mut()
        .expect("native trace array")
        .push(retained.clone());
    response["native_execution"] = retained;
    if let Some(proof) = &execution.runtime_identity {
        let lock = &proof.source_lock;
        let source = json!({"runtime_id":"semantica", "commit":lock.commit,
            "version":lock.version,"wheel_filename":lock.artifact_filename,
            "wheel_sha256":lock.artifact_sha256,"vendored_wheel_verified":true,
            "installed_version":lock.version,"installed_version_matches":true,
            "installed_wheel_sha256":lock.artifact_sha256,"installed_wheel_matches":true});
        if response["runtime_source"].get("commit").is_some()
            && ["commit", "version", "wheel_filename", "wheel_sha256"]
                .iter()
                .any(|key| response["runtime_source"][*key] != source[*key])
        {
            return finish_response(
                response.clone(),
                Err(anyhow::anyhow!(
                    "native runtime source identity changed between command operations"
                )),
            );
        }
        response["runtime_source"] = source;
    }

    if let Some(source) = execution
        .result
        .as_ref()
        .and_then(|r| r.get("rust_execution"))
        .filter(|source| !source.is_null())
    {
        if response["runtime_source"].get("schema").is_some() {
            ensure!(
                response["runtime_source"] == *source,
                "rust_refinery_source_changed_between_operations"
            );
        }
        response["runtime_source"] = source.clone();
    }
    if execution.semantic_execution != "completed" || execution.result.is_none() {
        response["command_verdict"] = "blocked".into();
        response["execution"] = json!({"status":"blocked","operation":execution.operation,
            "semantic_execution":execution.semantic_execution,"error_type":"NativeExecutionError",
            "message":execution.error});
        response["learning"]["status"] = "blocked".into();
        return Err(FailedNativeResponse(response.clone()).into());
    }
    Ok(execution.result.expect("checked"))
}

fn private(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    root.private_path(path)
}

fn document(root: &SkillRoot, path: &Path) -> Result<Value> {
    let path = private(root, path)?;
    strict_json(&oe_core::read_regular_bytes(&path, MAX_DOCUMENT)?)
}

fn engagement_input(value: Value) -> Result<Value> {
    ensure!(value.is_object(), "engagement evidence must be an object");
    contracts::forbid_selection(&value)?;
    if value["$schema"] == RESPONSE_SCHEMA {
        let receipt = value["execution"]
            .get("native_engagement_receipt")
            .filter(|receipt| receipt.is_object())
            .context("semantic response has no native engagement receipt")?;
        Ok(receipt.clone())
    } else {
        Ok(value)
    }
}
fn current_engagement_input(value: Value) -> Result<Value> {
    if value["schema"] == crate::refinery_full::FULL_SCHEMA {
        crate::refinery_full::full_request(&value)?;
        return Ok(value);
    }
    if value["schema"] == "semantica.refinery-observation-request/1" {
        crate::refinery_runtime::observation_request(&value)?;
        Ok(value)
    } else {
        engagement_input(value)
    }
}

fn capabilities(runtime: &RustRuntime, response: &mut Value) -> Result<()> {
    let value = consume(response, runtime.capabilities())?;
    response["capabilities"] = project_capabilities(&value)?;
    ensure!(
        response["capabilities"]["native"]["available"] == true,
        "source-locked native refinery API is not available"
    );
    Ok(())
}

fn project_capabilities(raw: &Value) -> Result<Value> {
    if raw["schema"] == "semantica.refinery-rust-capabilities/1" {
        ensure!(
            raw["engine"] == "semantica.rust-runtime"
                && raw["coverage"] == "partial"
                && raw["operations"].is_array()
                && raw["unsupported"].is_array(),
            "rust_refinery_capabilities_invalid"
        );
        let review = raw["decision_review_contract"]
            == semantica_runtime::promoted_review::REVIEW_CONTRACT
            && raw["decision_review_available"] == true
            && raw["decision_review_profile"] == semantica_runtime::promoted_review::PROFILE
            && raw["operations"]
                .as_array()
                .is_some_and(|v| v.iter().any(|o| o == "decision_review"));
        return Ok(
            json!({"profile":raw["profile"],"engine":raw["engine"],"coverage":"partial",
            "native":{"contract":contracts::NATIVE_REFINERY_CONTRACT,"available":true,"status":"source_bound_partial","operations":raw["operations"],"unsupported":raw["unsupported"],"original_full_contract_available":false},
            "decision_review":{"contract":raw["decision_review_contract"],"available":review,
                "status":if review {"available_scoped"}else{"not_connected"},
                "profile":raw["decision_review_profile"],"scope":raw["decision_review_scope"],
                "typed_project_checks":"not_run","engineering_adoption":"unknown","physical_acceptance":false}}),
        );
    }

    let operations = [
        "build_refinery_acceptance_delta",
        "open_engagement",
        "propose_candidate",
        "commit_candidate",
        "execute_candidate",
        "derive_gate_evidence",
        "verify_candidate",
        "promote_candidate",
        "history",
        "resolve_package",
        "execution_manifest",
        "run_registry",
    ];
    let symbols = [
        "CandidateVerificationDTO",
        "EngagementPhaseDTO",
        "ExecutionReceiptReferenceDTO",
        "IndustryOntologyRegistry",
        "IndustryPackageDescriptorDTO",
        "LearningResultDTO",
        "PackageDelta",
        "ProjectOntologyBinding",
        "RefineryAuthorizationDTO",
        "RefineryGateEvidenceDTO",
        "RefineryStateDTO",
        "RuntimeSourceIdentityDTO",
        "SemanticEngagementReceipt",
        "SemanticTaskEnvelope",
        "SourceEvidenceDTO",
        "SubjectExecutionSuiteDTO",
        "TransitionContextDTO",
    ];
    let categories = [
        "ontology",
        "competency_questions",
        "shapes",
        "queries",
        "rules",
        "cases",
        "contract",
        "provenance",
    ];
    let expected = json!({"contract":contracts::NATIVE_REFINERY_CONTRACT,"schema_version":"1.0","states":STATES,
        "asset_categories":categories,"delta_categories":["ontology","competency_questions","shapes","queries","rules","cases","contract","provenance","book_impact"],
        "book_impacts":["none","vol1-method","vol2-iso-exemplar","both"],"case_kinds":["positive","negative","ambiguity","prior_release"],
        "regression_check_ids":["cq.prior","cq.current","case.positive","case.negative","case.ambiguity","case.prior_release"],
        "release_check_ids":["package.coverage","capability.coverage","receipt.binding","provenance.binding","source.rights","io.binding"],
        "transition_context_actions":["candidate","proposed","committed","execute_candidate","derive_regression_gate","regression_passed","derive_release_gate","release_complete","promoted"],
        "transition_context_required_operations":["propose_candidate","commit_candidate","execute_candidate","derive_gate_evidence","verify_candidate","promote_candidate"],
        "runner_contract":"semantica.chapter_packages.SemanticPackageRunner/v1","operations":operations,"publication_owned_externally":true});
    let declaration = raw["native"]
        .as_object()
        .context("native capability declaration missing")?;
    let mismatches = expected
        .as_object()
        .expect("fixed object")
        .iter()
        .filter(|(k, v)| declaration.get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let missing_operations = operations
        .iter()
        .filter(|key| raw["operation_presence"][**key] != true)
        .copied()
        .collect::<Vec<_>>();
    let present = operations
        .iter()
        .filter(|key| raw["operation_presence"][**key] == true)
        .copied()
        .collect::<Vec<_>>();
    let missing_symbols = symbols
        .iter()
        .filter(|key| raw["symbol_presence"][**key] != true)
        .copied()
        .collect::<Vec<_>>();
    let empty = json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]});
    let constants = json!({"REFINERY_CONTRACT":contracts::NATIVE_REFINERY_CONTRACT,"REFINERY_SCHEMA_VERSION":"1.0","EMPTY_PACKAGE_SHA256":digest(&canonical_json(&empty)?)});
    let constant_mismatches = constants
        .as_object()
        .expect("fixed object")
        .iter()
        .filter(|(k, v)| raw["constants"].get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    let available = mismatches.is_empty()
        && missing_operations.is_empty()
        && missing_symbols.is_empty()
        && constant_mismatches.is_empty();
    let review = raw["decision_review_contract"] == "semantica.ontology.decision-review/v1"
        && raw["decision_review_available"] == true;
    Ok(
        json!({"native":{"contract":contracts::NATIVE_REFINERY_CONTRACT,"available":available,
        "status":if available {"available"} else {"blocked"},"operations":present,"missing_operations":missing_operations,
        "missing_symbols":missing_symbols,"constant_mismatches":constant_mismatches,
        "contract_matches":raw["native"]["contract"]==contracts::NATIVE_REFINERY_CONTRACT,
        "declaration_matches":mismatches.is_empty(),"declaration_mismatches":mismatches,"declaration":raw["native"],"declaration_error":null},
        "decision_review":{"contract":"semantica.ontology.decision-review/v1","available":review,
        "status":if review {"available"} else {"blocked"},"scope":"read-only promoted-package project ABox review","physical_acceptance":false}}),
    )
}

fn package_descriptors(raw: &Value) -> Result<Value> {
    let mut result = Vec::new();
    for (key, kind) in [("chapters", "chapter"), ("domains", "domain")] {
        for item in raw[key]
            .as_array()
            .context("native descriptor list missing")?
        {
            let mut value = item["descriptor"]
                .as_object()
                .context("native descriptor missing")?
                .clone();
            value.insert("type".into(), kind.into());
            for field in [
                "package_digest",
                "required_capabilities",
                "available_capabilities",
            ] {
                value.insert(
                    field.into(),
                    item.get(field)
                        .context("native package metadata missing")?
                        .clone(),
                );
            }
            result.push(Value::Object(value));
        }
    }
    result.sort_by(|a, b| a["package_id"].as_str().cmp(&b["package_id"].as_str()));
    Ok(json!(result))
}

struct ContextState {
    runtime: RustRuntime,
    binding: ProjectBinding,
    task: Option<SemanticTaskEnvelope>,
    registry: RegistryRequest,
    response: Value,
    sources: Vec<SourceSnapshot>,
}

struct SourceSnapshot {
    path: PathBuf,
    sha256: String,
}

fn check_sources(sources: &[SourceSnapshot]) -> Result<()> {
    for source in sources {
        ensure!(
            digest(oe_core::read_regular_bytes(&source.path, MAX_DOCUMENT)?) == source.sha256,
            "semantic input source changed during command"
        );
    }
    Ok(())
}

fn bound_inputs(
    root: &SkillRoot,
    command: &str,
    binding_path: &Path,
    task_path: Option<&Path>,
) -> Result<(
    ProjectBinding,
    Option<SemanticTaskEnvelope>,
    Vec<SourceSnapshot>,
)> {
    let path = private(root, binding_path)?;
    let binding = contracts::read_project_binding(&path)?;
    contracts::authorize(&binding, command)?;
    let mut sources = vec![SourceSnapshot {
        path,
        sha256: binding.source_sha256.clone(),
    }];
    let task = task_path
        .map(|path| {
            let path = private(root, path)?;
            let task = contracts::read_task_envelope(&path, &binding)?;
            contracts::authorize_task(&binding, &task, command)?;
            sources.push(SourceSnapshot {
                path,
                sha256: task.source_sha256.clone(),
            });
            Ok::<_, anyhow::Error>(task)
        })
        .transpose()?;
    check_sources(&sources)?;
    Ok((binding, task, sources))
}

fn recorded_time(value: Option<&str>) -> Result<Option<String>> {
    value
        .map(|value| contracts::timestamp(&json!(value), "recorded_at"))
        .transpose()
}

fn context(
    root: &Path,
    command: &str,
    binding_path: &Path,
    task_path: Option<&Path>,
    workspace: &Path,
) -> Result<ContextState> {
    let root = SkillRoot::new(root)?;
    let (binding, task, sources) = bound_inputs(&root, command, binding_path, task_path)?;
    let native_binding = contracts::native_binding(&binding)?;
    let registry = RegistryRequest {
        workspace: private(&root, workspace)?,
        binding: native_binding,
    };
    let mut response = envelope(command, Some(&binding), task.as_ref());
    let runtime = default_runtime(root.path())?;
    let checked = capabilities(&runtime, &mut response).and_then(|()| check_sources(&sources));
    let response = finish_response(response, checked)?;
    Ok(ContextState {
        runtime,
        binding,
        task,
        registry,
        response,
        sources,
    })
}

fn candidate_digest(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "candidate requires exact lowercase SHA-256"
    );
    Ok(())
}

fn found(response: &mut Value, binding: &ProjectBinding, candidate: Option<&str>) {
    response["corpus_found"] = json!({"status":"found","workspace_id":binding.workspace_id,
        "package_id":binding.package_id,"baseline_version":binding.baseline_version,
        "baseline_package_sha256":binding.baseline_digest});
    if let Some(candidate) = candidate {
        response["corpus_found"]["candidate_sha256"] = candidate.into();
    }
}

fn candidate_learning(
    response: &mut Value,
    state: &str,
    candidate: &str,
    native: &Value,
) -> Result<()> {
    let position = STATES
        .iter()
        .position(|s| *s == state)
        .context("unknown native lifecycle state")?;
    response["learning"]["status"] = "passed".into();
    response["learning"]["verdict"] = "candidate".into();
    response["learning"]["current_state"] = state.into();
    response["learning"]["completed_states"] = json!(&STATES[..=position]);
    response["learning"]["candidate"] = json!({"candidate_sha256":candidate});
    response["learning"]["native_state"] = native.clone();
    Ok(())
}

pub fn discover(
    root: &Path,
    binding_path: Option<&Path>,
    workspace: Option<&Path>,
) -> Result<Value> {
    let root = SkillRoot::new(root)?;
    let binding = binding_path
        .map(|p| contracts::read_project_binding(&private(&root, p)?))
        .transpose()?;
    if let Some(binding) = &binding {
        contracts::authorize(binding, "discover")?;
    }
    let mut response = envelope("discover", binding.as_ref(), None);
    let result = (|| -> Result<()> {
        let runtime = default_runtime(root.path())?;
        capabilities(&runtime, &mut response)?;
        let packages = package_descriptors(&consume(&mut response, runtime.discover_packages())?)?;
        let selected = if let Some(binding) = &binding {
            if binding.target_kind == "workspace" {
                let registry = RegistryRequest {
                    workspace: private(
                        &root,
                        workspace.context("workspace discovery requires workspace")?,
                    )?,
                    binding: contracts::native_binding(binding)?,
                };
                let native = consume(&mut response, runtime.discover(&registry))?;
                native["subject"].clone()
            } else {
                let packages = packages
                    .as_array()
                    .context("native package descriptors are not an array")?;
                let matches = packages
                    .iter()
                    .filter(|p| p["package_id"] == binding.package_id)
                    .collect::<Vec<_>>();
                ensure!(matches.len() == 1, "bound package absent or ambiguous");
                let package = matches[0];
                ensure!(
                    package["version"].as_str() == binding.package_version.as_deref()
                        && package["package_digest"] == binding.baseline_digest,
                    "built-in package baseline mismatch"
                );
                package.clone()
            }
        } else {
            Value::Null
        };
        let empty = json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]});
        response["corpus_found"] = json!({"status":"found","selected":selected,
        "subject":if binding.as_ref().is_some_and(|b|b.target_kind=="workspace") {selected.clone()} else {Value::Null},
        "packages":packages,"native_workspace_bootstrap":{"status":"available",
            "semantic_api":"semantica.ontology.refinery/v1","baseline_version":"0",
            "baseline_digest":digest(&canonical_json(&empty)?),"required_capabilities":["semantic.engagement"]},
        "note":"discovery returns a hash-checked binding coordinate; release truth still requires a source-locked execution receipt"});
        response["learning"]["status"] = "passed".into();
        response["learning"]["reason"] =
            "discovery alone produced no reusable ontology delta".into();
        response["command_verdict"] = "passed".into();
        Ok(())
    })();
    finish_response(response, result)
}

/// Narrow injection boundary for tests. Production delegates only to the
/// source-locked native runtime, never to a legacy owned Python entry point.
pub trait ExecutionBackend {
    fn capabilities(&self) -> NativeExecution;
    fn registry_diagnostics(&self) -> NativeExecution;
    fn discover_packages(&self) -> NativeExecution;
    fn run_package(
        &self,
        package: &str,
        scenario: Option<&str>,
        recorded_at: Option<&str>,
    ) -> NativeExecution;
    fn run_registry(&self, request: &RegistryRunRequest) -> NativeExecution;
}

impl ExecutionBackend for RustRuntime {
    fn capabilities(&self) -> NativeExecution {
        RustRuntime::capabilities(self)
    }
    fn registry_diagnostics(&self) -> NativeExecution {
        RustRuntime::registry_diagnostics(self)
    }
    fn discover_packages(&self) -> NativeExecution {
        RustRuntime::discover_packages(self)
    }
    fn run_package(
        &self,
        package: &str,
        scenario: Option<&str>,
        recorded_at: Option<&str>,
    ) -> NativeExecution {
        RustRuntime::run_package(self, package, scenario, recorded_at)
    }
    fn run_registry(&self, request: &RegistryRunRequest) -> NativeExecution {
        RustRuntime::run_registry(self, request)
    }
}

impl ExecutionBackend for NativeRuntime {
    fn capabilities(&self) -> NativeExecution {
        NativeRuntime::capabilities(self)
    }
    fn registry_diagnostics(&self) -> NativeExecution {
        NativeRuntime::registry_diagnostics(self)
    }
    fn discover_packages(&self) -> NativeExecution {
        NativeRuntime::discover_packages(self)
    }
    fn run_package(
        &self,
        package: &str,
        scenario: Option<&str>,
        recorded_at: Option<&str>,
    ) -> NativeExecution {
        self.run_and_verify_package_at(package, scenario, recorded_at)
    }
    fn run_registry(&self, request: &RegistryRunRequest) -> NativeExecution {
        NativeRuntime::run_registry(self, request)
    }
}

/// Full registry/API/source diagnosis. Identity-only/staging inspection remains
/// the separate runtime API and does not gain formal execution authority here.
pub fn doctor(root: &Path, binding: Option<&Path>) -> Result<Value> {
    doctor_with(root, binding, &default_runtime(root)?)
}

pub fn doctor_with(
    root: &Path,
    binding: Option<&Path>,
    backend: &dyn ExecutionBackend,
) -> Result<Value> {
    let root = SkillRoot::new(root)?;
    let (binding, sources) = if let Some(path) = binding {
        let (binding, _, sources) = bound_inputs(&root, "doctor", path, None)?;
        (Some(binding), sources)
    } else {
        (None, vec![])
    };
    let mut response = envelope("doctor", binding.as_ref(), None);
    let result = (|| {
        check_sources(&sources)?;
        let raw = consume(&mut response, backend.capabilities())?;
        response["capabilities"] = project_capabilities(&raw)?;
        check_sources(&sources)?;
        let raw = consume(&mut response, backend.registry_diagnostics())?;
        response["registry_diagnostics"] = raw.clone();
        let mut issues = Vec::new();
        for key in ["chapter_issues", "domain_issues"] {
            for issue in raw[key]
                .as_array()
                .context("native registry issues missing")?
            {
                ensure!(issue.is_string(), "native registry issue is not text");
                issues.push(issue.clone());
            }
        }
        let chapters = raw["chapters"]
            .as_array()
            .context("native chapter descriptors missing")?;
        let domains = raw["domains"]
            .as_array()
            .context("native domain descriptors missing")?;
        let rust = raw["schema"] == "semantica.refinery-rust-diagnostics/1";
        if rust {
            ensure!(
                response["runtime_source"]["status"] == "source_bound"
                    && response["runtime_source"]["compilation_origin"] == "unknown",
                "rust_refinery_current_source_identity_missing"
            );
        } else {
            ensure!(
                response["runtime_source"]["vendored_wheel_verified"] == true,
                "native runtime source identity missing"
            );
        }
        response["corpus_found"] = json!({"status":if issues.is_empty() {"found"} else {"blocked"},
            "chapter_package_count":chapters.len(),"domain_package_count":domains.len(),"registry_issues":issues});
        response["execution"] = json!({"status":"not_run","source_lock_verified":!rust,"rust_source_bound":rust,
            "native_refinery_status":response["capabilities"]["native"]["status"],
            "scope":"Source-bound catalog/data diagnostics only; no scenario, original full release or engineering adoption executed"});
        let available = response["capabilities"]["native"]["available"] == true;
        response["learning"]["status"] = if available { "passed" } else { "blocked" }.into();
        response["learning"]["reason"] = if available {
            "native refinery API is available"
        } else {
            "native refinery capability is missing; no compatibility fallback was selected"
        }
        .into();
        response["command_verdict"] = if available && issues.is_empty() {
            "passed"
        } else {
            "blocked"
        }
        .into();
        check_sources(&sources)
    })();
    finish_response(response, result)
}

/// A current execution request, never a request to verify a caller-supplied
/// historical receipt. Package verify runs and verifies with one native runner.
pub struct ExecutionRequest<'a> {
    pub binding: &'a Path,
    pub task: Option<&'a Path>,
    pub workspace: Option<&'a Path>,
    pub scenario_id: Option<&'a str>,
    pub recorded_at: Option<&'a str>,
}

pub fn run(root: &Path, request: &ExecutionRequest<'_>) -> Result<Value> {
    run_with(root, request, &default_runtime(root)?)
}
pub fn run_with(
    root: &Path,
    request: &ExecutionRequest<'_>,
    backend: &dyn ExecutionBackend,
) -> Result<Value> {
    execute_with(root, "run", request, backend)
}
pub fn verify_package(root: &Path, request: &ExecutionRequest<'_>) -> Result<Value> {
    verify_package_with(root, request, &default_runtime(root)?)
}
pub fn verify_package_with(
    root: &Path,
    request: &ExecutionRequest<'_>,
    backend: &dyn ExecutionBackend,
) -> Result<Value> {
    execute_with(root, "verify", request, backend)
}

fn bound_package<'a>(packages: &'a Value, binding: &ProjectBinding) -> Result<&'a Value> {
    let matches = packages
        .as_array()
        .context("native packages missing")?
        .iter()
        .filter(|p| p["package_id"] == binding.package_id)
        .collect::<Vec<_>>();
    ensure!(matches.len() == 1, "bound package absent or ambiguous");
    let selected = matches[0];
    ensure!(
        selected["version"].as_str() == binding.package_version.as_deref()
            && selected["package_digest"] == binding.baseline_digest,
        "built-in package baseline mismatch"
    );
    Ok(selected)
}

fn execution_receipt(
    receipt: &Value,
    identity: &Value,
    integrity: &Value,
) -> Result<(String, Vec<String>)> {
    let Some(map) = receipt.as_object() else {
        return Ok((
            "missing".into(),
            vec!["execution produced no native receipt".into()],
        ));
    };
    let mut reasons = Vec::new();
    let mut content = map.clone();
    let declared = content.remove("receipt_sha256");
    if declared.as_ref() != Some(&json!(digest(canonical_json(&Value::Object(content))?))) {
        reasons.push("receipt content integrity failed".into());
    }
    if integrity != &json!(true) {
        reasons.push("native receipt integrity not established".into());
    }
    for (field, source) in [
        ("runtime_commit", "commit"),
        ("runtime_version", "version"),
        ("runtime_artifact_sha256", "wheel_sha256"),
    ] {
        if !identity[source].is_string() || receipt[field] != identity[source] {
            reasons.push(format!("receipt {field} differs from source lock"));
        }
    }
    Ok((
        if reasons.is_empty() {
            "verified"
        } else {
            "invalid"
        }
        .into(),
        reasons,
    ))
}

fn project_execution(
    response: &mut Value,
    binding: &ProjectBinding,
    task: Option<&SemanticTaskEnvelope>,
    native: &Value,
    scenario: Option<&str>,
    selected: Option<&Value>,
) -> Result<()> {
    if native["profile"] == semantica_runtime::refinery_subject::CURRENT_SNAPSHOT
        && native["actual_run"].is_object()
    {
        return project_current_execution(response, binding, task, native, scenario);
    }
    let workspace = binding.target_kind == "workspace";
    let execution = &native[if workspace { "executor" } else { "run" }];
    let release = &native[if workspace { "release" } else { "verification" }];
    ensure!(
        execution.is_object() && release.is_object(),
        "native execution or verification missing"
    );
    ensure!(
        execution["package_id"] == binding.package_id
            && execution["package_version"] == binding.baseline_version,
        "executed package identity differs from binding"
    );
    if let Some(scenario) = scenario.filter(|_| execution["status"] == "passed") {
        ensure!(
            execution["scenario_id"] == scenario,
            "native executed a different scenario"
        );
    }
    if workspace {
        ensure!(
            native["registry_id"].as_str() == binding.workspace_id.as_deref(),
            "actual registry identity differs from binding"
        );
        let subject = &native["subject"];
        ensure!(
            subject["package_id"] == binding.package_id
                && subject["version"] == binding.baseline_version
                && subject["package_sha256"] == binding.baseline_digest,
            "registry subject differs from binding"
        );
        // The registry subject and runner projection have different identities.
        if execution["receipt"].is_object() {
            ensure!(
                execution["receipt"]["chapter_contract_sha256"]
                    == subject["execution_projection_sha256"]
                    && subject["execution_projection_sha256"].is_string(),
                "executor projection differs from registry subject"
            );
        }
        response["corpus_found"] =
            json!({"status":"found","subject":subject,"baseline_subject_verified":true});
        response["execution"] = json!({"status":execution["status"],"operation":"native_run_registry","subject":subject,
            "executor":{"package_id":execution["package_id"],"version":execution["package_version"],"digest":execution["package_digest"],
                "scenario_id":execution["scenario_id"],"receipt":execution["receipt"]}});
    } else {
        ensure!(
            execution["package_digest"] == binding.baseline_digest,
            "executed package digest differs from binding"
        );
        response["corpus_found"] = json!({"status":"found","selected":selected,"package_digest":execution["package_digest"],"baseline_digest_verified":true});
        response["execution"] = json!({"status":execution["status"],"package_id":execution["package_id"],
            "package_version":execution["package_version"],"scenario_id":execution["scenario_id"]});
    }
    for field in ["operations", "oracle_checks", "reasons"] {
        ensure!(
            execution[field].is_array(),
            "native execution detail array missing"
        );
        response["execution"][field] = execution[field].clone();
    }
    ensure!(
        execution["status"].is_string()
            && execution["cq_report"]["status"].is_string()
            && release["status"].is_string(),
        "native execution status missing"
    );
    response["execution"]["task_sha256"] = json!(task.map(|t| &t.source_sha256));
    let receipt = &execution["receipt"];
    let (receipt_status, reasons) = execution_receipt(
        receipt,
        &response["runtime_source"],
        &native["receipt_integrity"],
    )?;
    if receipt.is_object() {
        for field in ["package_id", "package_version", "package_digest"] {
            ensure!(
                receipt[field] == execution[field],
                "receipt and executed package identity differ"
            );
        }
        ensure!(
            release["receipt_sha256"] == receipt["receipt_sha256"],
            "release verification is not for the executed receipt"
        );
    }
    response["regression"] =
        json!({"status":execution["cq_report"]["status"],"report":execution["cq_report"]});
    response["receipt"] = json!({"status":receipt_status,"receipt":receipt,"reasons":reasons});
    response["release"] = json!({"status":release["status"],"verdict":release});
    if let Some(task) = task {
        let available = execution["capability_report"]["payload"]["profile"]["capabilities"]
            .as_array()
            .context("native execution capability profile missing")?;
        ensure!(
            task.required_capabilities
                .iter()
                .all(|required| available.iter().any(|v| v == required)),
            "task requires capabilities absent from the actual native executor"
        );
    }
    response["learning"]["status"] = "passed".into();
    response["learning"]["reason"] =
        "execution evidence alone does not assert a reusable practice delta".into();
    let mut signals = Vec::new();
    if execution["status"] != "passed" {
        signals.push("scenario execution did not pass");
    }
    if release["status"] != "complete" {
        signals.push("package release is not complete");
    }
    if receipt_status != "verified" {
        signals.push("execution receipt is not verified");
    }
    response["learning"]["signals"] = json!(signals);
    response["command_verdict"] = if execution["status"] == "passed"
        && receipt_status == "verified"
        && execution["cq_report"]["status"] == "passed"
        && release["status"] == "complete"
    {
        "passed"
    } else {
        "blocked"
    }
    .into();
    Ok(())
}

fn execute_with(
    root: &Path,
    command: &str,
    request: &ExecutionRequest<'_>,
    backend: &dyn ExecutionBackend,
) -> Result<Value> {
    let root = SkillRoot::new(root)?;
    let recorded_at = recorded_time(request.recorded_at)?;
    if let Some(scenario) = request.scenario_id {
        contracts::exact_text(&json!(scenario), "scenario_id")?;
    }
    let (binding, task, sources) = bound_inputs(&root, command, request.binding, request.task)?;
    let mut response = envelope(command, Some(&binding), task.as_ref());
    let result = (|| {
        if command == "verify" {
            ensure!(
                binding.target_kind == "package",
                "workspace candidate verify requires task, workspace and candidate; it is not a package run"
            );
        }
        let registry = if binding.target_kind == "workspace" {
            Some(RegistryRequest {
                workspace: private(
                    &root,
                    request
                        .workspace
                        .context("workspace run requires workspace")?,
                )?,
                binding: contracts::native_binding(&binding)?,
            })
        } else {
            ensure!(
                request.workspace.is_none(),
                "built-in package execution does not accept a workspace"
            );
            None
        };
        check_sources(&sources)?;
        let raw = consume(&mut response, backend.capabilities())?;
        response["capabilities"] = project_capabilities(&raw)?;
        if registry.is_some() {
            ensure!(
                response["capabilities"]["native"]["available"] == true,
                "source-locked native refinery API is not available"
            );
        }
        check_sources(&sources)?;
        if let Some(registry) = registry {
            let raw = consume(
                &mut response,
                backend.run_registry(&RegistryRunRequest {
                    registry,
                    scenario_id: request.scenario_id.map(str::to_owned),
                    created_at: recorded_at,
                }),
            )?;
            project_execution(
                &mut response,
                &binding,
                task.as_ref(),
                &raw,
                request.scenario_id,
                None,
            )?;
        } else {
            let packages =
                package_descriptors(&consume(&mut response, backend.discover_packages())?)?;
            let selected = bound_package(&packages, &binding)?;
            if let Some(task) = &task {
                let available = selected["available_capabilities"]
                    .as_array()
                    .context("native package capability profile missing")?;
                ensure!(
                    task.required_capabilities
                        .iter()
                        .all(|required| available.iter().any(|v| v == required)),
                    "task requires unavailable package runtime capabilities"
                );
            }
            check_sources(&sources)?;
            let raw = consume(
                &mut response,
                backend.run_package(
                    &binding.package_id,
                    request.scenario_id,
                    recorded_at.as_deref(),
                ),
            )?;
            project_execution(
                &mut response,
                &binding,
                task.as_ref(),
                &raw,
                request.scenario_id,
                Some(selected),
            )?;
        }
        check_sources(&sources)
    })();
    finish_response(response, result)
}

pub fn open(
    root: &Path,
    binding: &Path,
    task: &Path,
    workspace: &Path,
    engagement: Option<&Path>,
) -> Result<Value> {
    let ctx = context(root, "open", binding, Some(task), workspace)?;
    with_context(ctx, |ctx| {
        let task = ctx.task.as_ref().expect("task provided");
        let request = OpenEngagementRequest {
            workspace: ctx.registry.workspace.clone(),
            binding: ctx.registry.binding.clone(),
            envelope: contracts::native_task(task, "engagement")?,
        };
        let execution = if let Some(path) = engagement {
            let value = current_engagement_input(document(&SkillRoot::new(root)?, path)?)?;
            ctx.runtime.open_engagement_with_evidence(&request, &value)
        } else {
            ctx.runtime.open_engagement(&request)
        };
        let native = consume(&mut ctx.response, execution)?;
        let rust = native["execution_profile"] == crate::refinery_runtime::PROFILE;
        let receipt = if engagement.is_some() || rust {
            native
                .get("engagement")
                .context("native engagement receipt missing")?
        } else {
            &native
        };
        found(&mut ctx.response, &ctx.binding, None);
        ctx.response["execution"] = json!({"status":receipt["execution"]["status"],"operation":"native_open_engagement",
        "semantic_api":ctx.binding.semantic_api,"mutation_performed":true,"ontology_truth_mutated":false,
        "task_sha256":task.source_sha256,"native_envelope_sha256":receipt["envelope_sha256"],
        "native_binding_sha256":receipt["binding_sha256"],"native_engagement_receipt":receipt});
        ctx.response["regression"] =
            json!({"status":receipt["regression"]["status"],"phase":receipt["regression"]});
        ctx.response["receipt"] = json!({"status":if rust{"source_bound"}else{"verified"},"receipt_sha256":receipt["receipt_sha256"],
        "phase":receipt["receipt"],"runtime_source":receipt["runtime_source"]});
        ctx.response["release"] = json!({"status":if receipt["release"]["status"]=="passed" {"complete"} else {"blocked"},
        "phase":receipt["release"],"engagement_status":receipt["status"],"blocked_reasons":receipt["blocked_reasons"]});
        let verdict = receipt["learning"]["status"]
            .as_str()
            .context("native learning verdict missing")?;
        ensure!(
            matches!(verdict, "no_delta" | "candidate"),
            "unknown native learning verdict"
        );
        let status = if receipt["status"] == "complete" {
            "passed"
        } else {
            "blocked"
        };
        ctx.response["learning"]["verdict"] = verdict.into();
        ctx.response["learning"]["status"] = status.into();
        ctx.response["learning"]["native"] = receipt["learning"].clone();
        ctx.response["learning"]["reason"] = receipt["learning"]["rationale"].clone();
        ctx.response["command_verdict"] = status.into();
        Ok(())
    })
}

pub fn propose(
    root: &Path,
    binding: &Path,
    task: &Path,
    workspace: &Path,
    delta_path: &Path,
    engagement: &Path,
) -> Result<Value> {
    propose_at(root, binding, task, workspace, delta_path, engagement, None)
}

pub fn propose_at(
    root: &Path,
    binding: &Path,
    task: &Path,
    workspace: &Path,
    delta_path: &Path,
    engagement: &Path,
    recorded_at: Option<&str>,
) -> Result<Value> {
    let recorded_at = recorded_time(recorded_at)?;
    let ctx = context(root, "propose", binding, Some(task), workspace)?;
    with_context(ctx, |ctx| {
        let task = ctx.task.as_ref().expect("task provided");
        let skill = SkillRoot::new(root)?;
        let delta = delta::read_delta(&private(&skill, delta_path)?, &ctx.binding)?;
        let request = ProposeRequest {
            registry: ctx.registry.clone(),
            delta,
            candidate_envelope: contracts::native_task(task, "candidate")?,
            proposed_envelope: contracts::native_task(task, "proposed")?,
            engagement: current_engagement_input(document(&skill, engagement)?)?,
            recorded_at,
        };
        let native = consume(&mut ctx.response, ctx.runtime.propose_candidate(&request))?;

        if native["eligibility"] == "blocked" {
            let candidate = native["delta"]["delta_sha256"]
                .as_str()
                .context("Rust candidate intent digest missing")?;
            candidate_digest(candidate)?;
            found(&mut ctx.response, &ctx.binding, Some(candidate));
            ctx.response["execution"] = json!({"status":"completed","operation":"rust_propose_candidate_observation","mutation_performed":native["candidate_recorded"],"ontology_truth_mutated":false,"candidate_state":"not_registered","native_engagement_receipt":native["engagement"]});
            ctx.response["regression"] = json!({"status":"blocked","reasons":native["blocked_reasons"],"original_full_gates":"unknown"});
            ctx.response["receipt"] = json!({"status":"source_bound","receipt_sha256":native["engagement"]["receipt_sha256"],"runtime_source":native["engagement"]["runtime_source"]});
            ctx.response["release"] = json!({"status":"blocked","engineering_adoption":"unknown"});
            ctx.response["learning"]["status"] = json!("blocked");
            ctx.response["learning"]["verdict"] = json!("candidate");
            ctx.response["learning"]["candidate"] = json!({"candidate_sha256":candidate,"delta_sha256":candidate,"package_id":native["delta"]["package_id"],"target_version":native["delta"]["target_version"],"book_impact":native["delta"]["book_impact"],"task_sha256":task.source_sha256,"record_status":"intent_retained","original_candidate_state":"not_registered"});
            ctx.response["learning"]["decision_required"] = true.into();
            ctx.response["command_verdict"] = json!("blocked");
            return Ok(());
        }
        ensure!(
            native["state"]["state"] == "proposed",
            "native propose did not reach proposed state"
        );
        let candidate = native["delta"]["delta_sha256"]
            .as_str()
            .context("native delta identity missing")?;
        candidate_digest(candidate)?;
        found(&mut ctx.response, &ctx.binding, None);
        ctx.response["execution"] = json!({"status":"passed","operation":"native_propose_candidate",
        "mutation_performed":true,"ontology_truth_mutated":false,"state":native["state"],
        "native_engagement_receipt":native["engagement"],"transition_contexts":native["transition_contexts"]});
        ctx.response["regression"] =
            json!({"status":"not_run","reason":"candidate regression occurs only after commit"});
        ctx.response["receipt"] = json!({"status":"verified","receipt_sha256":native["engagement"]["receipt_sha256"],
        "runtime_source":native["engagement"]["runtime_source"]});
        ctx.response["release"] = json!({"status":"blocked","reasons":["candidate state is proposed, not release_complete"]});
        candidate_learning(&mut ctx.response, "proposed", candidate, &native["state"])?;
        ctx.response["learning"]["candidate"] = json!({"candidate_sha256":candidate,"delta_sha256":candidate,
        "package_id":native["delta"]["package_id"],"target_version":native["delta"]["target_version"],
        "book_impact":native["delta"]["book_impact"],"task_sha256":task.source_sha256});
        ctx.response["learning"]["decision_required"] = true.into();
        ctx.response["command_verdict"] = "passed".into();
        Ok(())
    })
}

/// Commit, verify or promote exactly one authorized lifecycle step.
/// No accepted step implicitly authorizes a later one.
#[allow(clippy::too_many_arguments)]
pub fn transition(
    root: &Path,
    action: &str,
    binding: &Path,
    task: &Path,
    workspace: &Path,
    candidate: &str,
    authorization: Option<&Path>,
) -> Result<Value> {
    transition_at(
        root,
        action,
        binding,
        task,
        workspace,
        candidate,
        authorization,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn transition_at(
    root: &Path,
    action: &str,
    binding: &Path,
    task: &Path,
    workspace: &Path,
    candidate: &str,
    authorization: Option<&Path>,
    recorded_at: Option<&str>,
) -> Result<Value> {
    let recorded_at = recorded_time(recorded_at)?;
    ensure!(
        matches!(action, "commit" | "verify" | "promote"),
        "unsupported lifecycle action"
    );
    candidate_digest(candidate)?;
    let ctx = context(root, action, binding, Some(task), workspace)?;
    with_context(ctx, |ctx| {
        let task = ctx.task.as_ref().expect("task provided");
        if action == "verify" {
            ensure!(
                authorization.is_none(),
                "verify does not accept a supplied gate or authorization"
            );
            let actions = [
                "execute_candidate",
                "derive_regression_gate",
                "regression_passed",
                "derive_release_gate",
                "release_complete",
            ];
            let envelopes = actions
                .into_iter()
                .map(|a| Ok((a.to_owned(), contracts::native_task(task, a)?)))
                .collect::<Result<BTreeMap<_, _>>>()?;
            let request = VerifyRequest {
                registry: ctx.registry.clone(),
                delta_sha256: candidate.into(),
                envelopes,
                recorded_at,
            };
            let native = consume(&mut ctx.response, ctx.runtime.verify_candidate(&request))?;
            return verification_response(&mut ctx.response, &ctx.binding, candidate, &native);
        }
        let authorization = delta::read_authorization(
            &private(
                &SkillRoot::new(root)?,
                authorization.context("mutation requires exact authorization")?,
            )?,
            &ctx.binding,
            action,
            candidate,
        )?;
        let native_action = if action == "commit" {
            "committed"
        } else {
            "promoted"
        };
        let request = TransitionRequest {
            registry: ctx.registry.clone(),
            delta_sha256: candidate.into(),
            envelope: contracts::native_task(task, native_action)?,
            recorded_at,
        };
        let execution = if action == "commit" {
            ctx.runtime.commit_candidate(&request, &authorization)
        } else {
            ctx.runtime.promote_candidate(&request, &authorization)
        };
        let native = consume(&mut ctx.response, execution)?;
        found(&mut ctx.response, &ctx.binding, Some(candidate));
        if action == "commit" {
            ensure!(
                native["state"]["state"] == "committed",
                "native commit did not reach committed state"
            );
            ctx.response["execution"] = json!({"status":"passed","operation":"native_commit_candidate",
            "semantic_api":ctx.binding.semantic_api,"state":native["state"],"transition_context":native["transition_context"]});
            ctx.response["regression"] = json!({"status":"not_run","reason":"committed candidate has not completed regression"});
            ctx.response["receipt"] = json!({"status":"bound","event_sha256":native["state"]["event_sha256"],"package_sha256":native["state"]["package_sha256"]});
            ctx.response["release"] = json!({"status":"blocked","reasons":["candidate state is committed, not release_complete"]});
            candidate_learning(&mut ctx.response, "committed", candidate, &native["state"])?;
        } else {
            let descriptor = &native["descriptor"];
            let next = contracts::next_binding_projection(&ctx.binding, descriptor)?;
            ctx.response["execution"] = json!({"status":"passed","operation":"native_promote_candidate",
            "descriptor":descriptor,"transition_context":native["transition_context"]});
            ctx.response["regression"] = json!({"status":"passed","source":"promotion descriptor binds regression evidence"});
            ctx.response["receipt"] = json!({"status":"verified","engagement_receipt_sha256":descriptor["engagement_receipt_sha256"],
            "promotion_record_sha256":descriptor["promotion_record_sha256"]});
            ctx.response["release"] = json!({"status":"complete","release_evidence_sha256":descriptor["release_evidence_sha256"]});
            candidate_learning(&mut ctx.response, "promoted", candidate, &Value::Null)?;
            ctx.response["learning"]["promotion"] = json!({"status":"promoted","target":ctx.binding.promotion_target,"descriptor":descriptor});
            ctx.response["learning"]["next_binding"] = next;
        }
        ctx.response["command_verdict"] = "passed".into();
        Ok(())
    })
}

fn verification_response(
    response: &mut Value,
    binding: &ProjectBinding,
    candidate: &str,
    native: &Value,
) -> Result<()> {
    ensure!(
        native["state"]["state"] == "release_complete",
        "native verify did not reach release_complete state"
    );
    let suite = &native["execution_suite"];
    let regression = &native["regression_evidence"];
    let release = &native["release_evidence"];
    let recovered = suite.is_null();
    let (subject, status, receipts, source) = if recovered {
        let source = &release["runtime_source"];
        if source["schema"] == "semantica.refinery-rust-source/1" {
            let typed: semantica_contracts::SemanticRefineryRustSource =
                serde_json::from_value(source.clone())?;
            typed
                .validate()
                .map_err(|e| anyhow::anyhow!("retained current source contract:{e:?}"))?;
            ensure!(
                source == &response["runtime_source"],
                "retained current verification source observation differs"
            );
        } else {
            for (key, actual) in [
                ("runtime_commit", "commit"),
                ("runtime_version", "version"),
                ("runtime_artifact_sha256", "wheel_sha256"),
            ] {
                ensure!(
                    source[key].is_string() && source[key] == response["runtime_source"][actual],
                    "retained native verification runtime differs from source lock"
                );
            }
        }
        (
            json!({"package_id":binding.package_id,"version":null,"package_sha256":release["package_sha256"],
            "manifest_sha256":null,"execution_projection_sha256":null}),
            "passed",
            json!([]),
            source.clone(),
        )
    } else {
        let receipts=suite["runs"].as_array().context("native suite runs missing")?.iter().map(|item|json!({
            "scenario_id":item["scenario_id"],"receipt_sha256":item["receipt_sha256"],"receipt_object_sha256":item["receipt_object_sha256"]})).collect::<Vec<_>>();
        (
            json!({"package_id":suite["subject_package_id"],"version":suite["subject_package_version"],
            "package_sha256":suite["subject_package_sha256"],"manifest_sha256":suite["subject_manifest_sha256"],
            "execution_projection_sha256":suite["execution_projection_sha256"]}),
            if suite["status"] == "complete" {
                "passed"
            } else {
                "blocked"
            },
            json!(receipts),
            suite["runtime_source"].clone(),
        )
    };
    found(response, binding, Some(candidate));
    response["corpus_found"]["subject"] = subject;
    response["execution"] = json!({"status":status,"operation":"native_execute_and_verify_candidate",
        "state":native["state"],"subject_execution_suite":suite,"subject_execution_suite_sha256":native["execution_suite_sha256"],
        "recovered_from_immutable_evidence":recovered,"transition_contexts":native["transition_contexts"],"native_verification":native["native_verification"]});
    response["regression"] = json!({"status":if regression["status"]=="complete" {"passed"} else {"blocked"},"evidence":regression});
    response["receipt"] = json!({"status":if status=="passed" {"verified"} else {"invalid"},
        "execution_suite_sha256":native["execution_suite_sha256"],"execution_receipts":receipts,"runtime_source":source,
        "recovered_from_immutable_evidence":recovered});
    response["release"] = json!({"status":if release["status"]=="complete" {"complete"} else {"blocked"},"evidence":release});
    candidate_learning(response, "release_complete", candidate, &native["state"])?;
    response["command_verdict"] = if status == "passed"
        && regression["status"] == "complete"
        && release["status"] == "complete"
    {
        "passed"
    } else {
        "blocked"
    }
    .into();
    Ok(())
}

pub fn history(root: &Path, binding: &Path, workspace: &Path, candidate: &str) -> Result<Value> {
    candidate_digest(candidate)?;
    let ctx = context(root, "history", binding, None, workspace)?;
    with_context(ctx, |ctx| {
        let native = consume(
            &mut ctx.response,
            ctx.runtime.history(&ctx.registry, candidate),
        )?;
        let events = native
            .get("events")
            .unwrap_or(&native)
            .as_array()
            .context("native history events missing")?;
        let latest = events.last().context("native candidate history is empty")?;
        let states = events
            .iter()
            .map(|event| {
                event["state"]
                    .as_str()
                    .context("native history state missing")
            })
            .collect::<Result<Vec<_>>>()?;
        found(&mut ctx.response, &ctx.binding, Some(candidate));
        ctx.response["execution"] =
            json!({"status":"passed","operation":"native_history","events":events});
        ctx.response["regression"] = json!({"status":if states.contains(&"regression_passed") {"passed"} else {"not_run"},"source":"native immutable event chain"});
        ctx.response["receipt"] = json!({"status":"bound","event_sha256":latest["event_sha256"]});
        ctx.response["release"] = json!({"status":if states.contains(&"release_complete") || states.contains(&"promoted") {"complete"} else {"blocked"},"source":"native immutable event chain"});
        candidate_learning(
            &mut ctx.response,
            states.last().context("empty native history")?,
            candidate,
            latest,
        )?;
        ctx.response["learning"]["completed_states"] = json!(states);
        ctx.response["command_verdict"] = "passed".into();
        Ok(())
    })
}

/// One exact task-evidence projection for a promoted registry review.
pub struct ProjectReviewRequest<'a> {
    pub binding: &'a Path,
    pub workspace: &'a Path,
    pub task: &'a Path,
    pub evidence_file: &'a Path,
    pub source_id: &'a str,
    pub evidence_format: &'a str,
    pub scope_id: &'a str,
    pub focus_iri: &'a str,
    pub focus_type_iri: &'a str,
    pub query_asset_id: &'a str,
    pub shape_asset_id: &'a str,
}

pub fn project_review(root: &Path, request: &ProjectReviewRequest<'_>) -> Result<Value> {
    project_review_at(root, request, None)
}

pub fn project_review_at(
    root: &Path,
    request: &ProjectReviewRequest<'_>,
    recorded_at: Option<&str>,
) -> Result<Value> {
    project_review_observed_at(root, request, recorded_at, &[], None)
}

/// Source-bound extension of the same authorized task/review owner. Additional
/// references are read before observation and rechecked by with_context after it.
/// No registry, history, permission or typed Project interpretation lives here.
pub fn project_review_observed_at(
    root: &Path,
    request: &ProjectReviewRequest<'_>,
    recorded_at: Option<&str>,
    sources: &[oe_contracts::ArtifactReference],
    mut observer: Option<
        &mut dyn FnMut(
            &DecisionReviewRequest,
            &semantica_contracts::SemanticPromotedReviewResult1,
            &[Vec<u8>],
        ) -> Result<Value>,
    >,
) -> Result<Value> {
    let recorded_at = recorded_time(recorded_at)?;
    let ctx = context(
        root,
        "review",
        request.binding,
        Some(request.task),
        request.workspace,
    )?;
    with_context(ctx, |ctx| {
        let (native_request, source_bytes) =
            prepare_project_review(root, ctx, request, recorded_at, sources, false)?;
        let executed = if let Some(observe) = observer.as_mut() {
            ctx.runtime.decision_review_observed(
                &native_request,
                Some(&mut |request, result| observe(request, result, &source_bytes)),
            )
        } else {
            ctx.runtime.decision_review(&native_request)
        };
        let native = consume(&mut ctx.response, executed)?;
        project_review_response(
            &mut ctx.response,
            &native,
            &ctx.task.as_ref().expect("task provided").source_sha256,
        );
        Ok(())
    })
}

fn prepare_project_review(
    root: &Path,
    ctx: &mut ContextState,
    request: &ProjectReviewRequest<'_>,
    recorded_at: Option<String>,
    sources: &[oe_contracts::ArtifactReference],
    allow_root_sources: bool,
) -> Result<(DecisionReviewRequest, Vec<Vec<u8>>)> {
    let mut source_bytes = Vec::new();
    for reference in sources {
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("typed_review_source_reference_invalid"))?;
        let path = if allow_root_sources {
            crate::safe_relative(&reference.path)?;
            SkillRoot::new(root)?.expand(Path::new(&reference.path))?
        } else {
            private(&SkillRoot::new(root)?, Path::new(&reference.path))?
        };
        let bytes = oe_core::read_regular_bytes(&path, MAX_DOCUMENT)?;
        ensure!(
            digest(&bytes) == reference.sha256,
            "typed_review_source_changed"
        );
        ctx.sources.push(SourceSnapshot {
            path,
            sha256: reference.sha256.clone(),
        });
        source_bytes.push(bytes);
    }
    let task = ctx.task.as_ref().expect("task provided");
    ensure!(
        task.required_capabilities
            .iter()
            .any(|c| c == "semantica.ontology.decision-review/v1"),
        "task must request decision-review/v1 capability"
    );
    let media = match request.evidence_format {
        "turtle" => "text/turtle",
        "trig" => "application/trig",
        "nt" => "application/n-triples",
        "nquads" => "application/n-quads",
        "xml" => "application/rdf+xml",
        "json-ld" => "application/ld+json",
        _ => anyhow::bail!("unsupported decision review RDF format"),
    };
    let references = task
        .evidence
        .iter()
        .filter(|item| item["source_id"] == request.source_id)
        .collect::<Vec<_>>();
    ensure!(
        references.len() == 1,
        "decision review source must occur exactly once in task evidence"
    );
    let reference = references[0];
    ensure!(
        reference["media_type"] == media,
        "decision review format differs from task evidence media_type"
    );
    let evidence_path = private(&SkillRoot::new(root)?, request.evidence_file)?;
    let evidence = oe_core::read_regular_bytes(&evidence_path, crate::native::MAX_NATIVE_BYTES)?;
    ensure!(
        digest(&evidence) == reference["sha256"],
        "decision review evidence differs from task SHA-256"
    );
    ctx.sources.push(SourceSnapshot {
        path: evidence_path,
        sha256: digest(&evidence),
    });
    let native_request = DecisionReviewRequest {
        workspace: ctx.registry.workspace.clone(),
        binding: ctx.registry.binding.clone(),
        evidence,
        evidence_sha256: reference["sha256"]
            .as_str()
            .context("source hash missing")?
            .into(),
        evidence_format: request.evidence_format.into(),
        evidence_id: request.source_id.into(),
        evidence_uri: reference["uri"]
            .as_str()
            .context("source uri missing")?
            .into(),
        evidence_captured_at: reference["captured_at"]
            .as_str()
            .context("source capture time missing")?
            .into(),
        scope_id: request.scope_id.into(),
        focus_iri: request.focus_iri.into(),
        focus_type_iri: request.focus_type_iri.into(),
        query_asset_id: request.query_asset_id.into(),
        shape_asset_id: request.shape_asset_id.into(),
        created_at: recorded_at,
    };
    Ok((native_request, source_bytes))
}

fn project_review_response(response: &mut Value, native: &Value, task_sha256: &str) {
    let report = &native["review"];
    let status = if report["status"] == "clear"
        && native
            .get("typed_project_observation")
            .is_none_or(|v| v["execution"] == "completed" && v["conclusion"] == "pass")
    {
        "passed"
    } else {
        "blocked"
    };
    response["corpus_found"] =
        json!({"status":"found","subject":native["subject"],"baseline_subject_verified":true});
    response["execution"] = json!({"status":status,"operation":"native_decision_review","review":report,
            "task_sha256":task_sha256,"review_observation":native["review_observation"],
            "typed_project_checks":"not_run","engineering_adoption":"unknown"});
    if let Some(observation) = native.get("typed_project_observation") {
        response["execution"]["typed_project_observation"] = observation.clone();
        response["execution"]["typed_project_checks"] = observation["execution"].clone();
        response["execution"]["typed_project_recheck"] = native["typed_project_recheck"].clone();
    }
    response["learning"]["reason"] =
        "a project review alone does not establish reusable industry knowledge".into();
    response["learning"]["signals"] = if status == "blocked" {
        json!(["open semantic challenge in scoped project projection"])
    } else {
        json!([])
    };
    response["command_verdict"] = status.into();
}

/// A finite sibling for the original engineering owner. The read-only API keeps
/// its original before/after order; this path performs all commit guards while
/// RustRuntime still holds the same FileStore. JSON carries no publisher.
pub fn project_review_committed_at(
    root: &Path,
    request: &ProjectReviewRequest<'_>,
    recorded_at: Option<&str>,
    sources: &[oe_contracts::ArtifactReference],
    observer: &mut dyn FnMut(
        &DecisionReviewRequest,
        &semantica_contracts::SemanticPromotedReviewResult1,
        &[Vec<u8>],
    ) -> Result<Value>,
    precommit: &mut dyn FnMut(&Value) -> Result<()>,
    publish: impl FnOnce(&Value) -> Result<Value>,
) -> Result<Value> {
    let recorded_at = recorded_time(recorded_at)?;
    let mut ctx = context(
        root,
        "review",
        request.binding,
        Some(request.task),
        request.workspace,
    )?;
    let result = (|| -> Result<()> {
        check_sources(&ctx.sources)?;
        let (native_request, source_bytes) =
            prepare_project_review(root, &mut ctx, request, recorded_at, sources, true)?;
        let task_sha = ctx
            .task
            .as_ref()
            .expect("task provided")
            .source_sha256
            .clone();
        let owner_response = |native: &Value| {
            let mut response = ctx.response.clone();
            response["native_execution"] =
                json!({"semantic_execution":"completed","result":native});
            project_review_response(&mut response, native, &task_sha);
            response
        };
        let executed = ctx.runtime.decision_review_committed(
            &native_request,
            &mut |request, result| observer(request, result, &source_bytes),
            &mut |native| {
                precommit(&owner_response(native))?;
                // Actual reads after owner preparation, before the runtime's
                // final current/package/source guards and before publication.
                check_sources(&ctx.sources)
            },
            |native| publish(&owner_response(native)),
        );
        let native = consume(&mut ctx.response, executed)?;
        project_review_response(&mut ctx.response, &native, &task_sha);
        Ok(())
    })();
    // No new first-time source check after publication. Runtime errors retain
    // the actual observation and any publication ambiguity in this response.
    finish_response(ctx.response, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fault_context(command: &str) -> ContextState {
        let binding = contracts::parse_project_binding(&canonical_json(&json!({
            "$schema":contracts::BINDING_SCHEMA,"binding_id":"binding:fault-fixture",
            "project":{"project_id":"project:fixture","domain":"test"},
            "semantic_target":{"kind":"workspace","workspace_id":"workspace:fixture","package_id":"fixture"},
            "baseline":{"version":"1.0","digest":"a".repeat(64)},
            "evidence":{"logical_root":"evidence:fixture"},
            "authority":{"fact":{"authority_id":"fact:fixture","scope":["observation"]},
                "decision":{"authority_id":"decision:fixture","scope":["promotion"]}},
            "allowed_actions":contracts::KNOWN_ACTIONS,"lifecycle_actions":contracts::STATE_MODEL,
            "promotion":{"target":"industry-registry","requires_decision_authority":true},
            "created_at":"2026-09-27T12:00:00Z","semantic_api":contracts::NATIVE_REFINERY_CONTRACT
        })).unwrap()).unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let runtime = RustRuntime::new(root).unwrap();
        let registry = RegistryRequest {
            workspace: root.join("var/fault-fixture-not-created"),
            binding: contracts::native_binding(&binding).unwrap(),
        };
        let mut response = envelope(command, Some(&binding), None);
        response["task"] = json!({"task_id":"task:fixture","source_sha256":"b".repeat(64)});
        response["runtime_source"] = json!({"commit":"c".repeat(40),"wheel_sha256":"d".repeat(64)});
        ContextState {
            runtime,
            binding,
            task: None,
            registry,
            response,
            sources: vec![],
        }
    }

    fn completed_result(operation: &str, value: Value) -> NativeExecution {
        NativeExecution {
            operation: operation.into(),
            semantic_execution: "completed".into(),
            runtime_identity: None,
            result: Some(value),
            error: None,
            elapsed_seconds: 0.1,
            stderr: "native diagnostic retained".into(),
        }
    }

    fn assert_retained(command: &str, error: anyhow::Error, original: &Value) {
        let response = error_response(command, &error);
        assert_eq!(response["command_verdict"], "blocked");
        assert_eq!(response["execution"]["status"], "blocked");
        assert_eq!(response["learning"]["status"], "blocked");
        assert_eq!(
            response["native_execution"]["semantic_execution"],
            "completed"
        );
        assert_eq!(&response["native_execution"]["result"], original);
        assert_eq!(
            response["native_execution"]["stderr"],
            "native diagnostic retained"
        );
        assert_eq!(response["binding"]["binding_id"], "binding:fault-fixture");
        assert_eq!(response["task"]["source_sha256"], "b".repeat(64));
        assert_eq!(response["runtime_source"]["commit"], "c".repeat(40));
        assert_eq!(response["diagnostics"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn capability_projection_failure_retains_completed_probe_and_bound_context() {
        let original = json!({"native":"malformed declaration","operation_presence":{},"native_output":"unaltered"});
        let error = with_context(fault_context("discover"), |ctx| {
            let raw = consume(
                &mut ctx.response,
                completed_result("capabilities", original.clone()),
            )?;
            ctx.response["capabilities"] = project_capabilities(&raw)?;
            Ok(())
        })
        .unwrap_err();
        assert_retained("discover", error, &original);
    }

    #[test]
    fn malformed_state_projection_preserves_already_completed_mutation() {
        let original = json!({"state":{"state":"unrecognized-successor"},"partial_mutation":{"object_written":true}});
        let error = with_context(fault_context("commit"), |ctx| {
            let raw = consume(
                &mut ctx.response,
                completed_result("commit_candidate", original.clone()),
            )?;
            candidate_learning(
                &mut ctx.response,
                raw["state"]["state"].as_str().unwrap(),
                &"e".repeat(64),
                &raw["state"],
            )?;
            Ok(())
        })
        .unwrap_err();
        assert_retained("commit", error, &original);
    }

    #[test]
    fn malformed_verification_projection_preserves_release_state_and_original_suite() {
        let original = json!({"state":{"state":"release_complete"},"execution_suite":{"runs":"invalid"},"regression_evidence":{"status":"complete"},"release_evidence":{"status":"complete"}});
        let error = with_context(fault_context("verify"), |ctx| {
            let raw = consume(
                &mut ctx.response,
                completed_result("verify_candidate", original.clone()),
            )?;
            verification_response(&mut ctx.response, &ctx.binding, &"e".repeat(64), &raw)
        })
        .unwrap_err();
        assert_retained("verify", error, &original);
    }

    #[test]
    fn successor_projection_failure_retains_native_promotion_evidence() {
        let original = json!({"descriptor":{"package_id":"fixture","version":"2.0","package_sha256":"e".repeat(64),"promotion_record_sha256":"f".repeat(64),"promoted_at":"2026-09-27T13:00:00Z"},"promotion_written":true});
        let error = with_context(fault_context("promote"), |ctx| {
            let raw = consume(
                &mut ctx.response,
                completed_result("promote_candidate", original.clone()),
            )?;
            ctx.response["learning"]["next_binding"] =
                contracts::next_binding_projection(&ctx.binding, &raw["descriptor"])?;
            Ok(())
        })
        .unwrap_err();
        assert_retained("promote", error, &original);
    }

    #[test]
    fn engagement_response_unwraps_only_exact_native_receipt_without_upgrading_it() {
        let receipt = json!({"receipt_sha256":"a".repeat(64),"status":"blocked","learning":{"status":"no_delta"},"execution":{"status":"blocked"}});
        let mut response = envelope("open", None, None);
        response["command_verdict"] = "passed".into(); // transport summary cannot override native receipt
        response["execution"]["native_engagement_receipt"] = receipt.clone();
        assert_eq!(
            engagement_input(response).unwrap(),
            engagement_input(receipt.clone()).unwrap()
        );
        assert_eq!(engagement_input(receipt).unwrap()["status"], "blocked");
    }

    #[test]
    fn engagement_response_with_missing_or_wrong_receipt_is_rejected() {
        for execution in [
            json!({}),
            json!({"native_engagement_receipt":null}),
            json!({"native_engagement_receipt":[]}),
            json!("missing"),
        ] {
            let mut response = envelope("open", None, None);
            response["execution"] = execution;
            assert!(engagement_input(response).is_err());
        }
        assert!(engagement_input(json!([])).is_err());
        let bare = json!({"learning":{"status":"candidate"},"fallback":"approve"});
        assert!(engagement_input(bare).is_err());
    }

    #[test]
    fn stable_error_keeps_independent_sections_and_never_reports_empty_history() {
        let error = anyhow::anyhow!("native history failed");
        let response = error_response("history", &error);
        assert_eq!(response["command_verdict"], "blocked");
        assert_eq!(response["execution"]["status"], "blocked");
        assert!(response["execution"].get("events").is_none());
        assert_eq!(response["regression"]["status"], "not_run");
        assert_eq!(response["release"]["status"], "not_checked");
        assert_eq!(
            response["learning"]["publication"]["published"],
            Value::Null
        );
    }

    #[test]
    fn native_failure_retains_partial_mutation_evidence() {
        let mut response = envelope("commit", None, None);
        let native = NativeExecution {
            operation: "commit".into(),
            semantic_execution: "failed".into(),
            runtime_identity: None,
            result: Some(json!({"raw_state":"committed"})),
            error: Some("registry binding check failed".into()),
            elapsed_seconds: 1.0,
            stderr: "native progress".into(),
        };
        let error = consume(&mut response, native).unwrap_err();
        let result = error_response("commit", &error);
        assert_eq!(
            result["native_execution"]["result"]["raw_state"],
            "committed"
        );
        assert_eq!(result["native_execution"]["stderr"], "native progress");
        assert_eq!(result["execution"]["semantic_execution"], "failed");
        assert_eq!(result["command_verdict"], "blocked");
    }

    #[test]
    fn lifecycle_state_does_not_imply_release_or_publication() {
        let mut response = envelope("commit", None, None);
        candidate_learning(
            &mut response,
            "committed",
            &"a".repeat(64),
            &json!({"state":"committed"}),
        )
        .unwrap();
        assert_eq!(
            response["learning"]["completed_states"],
            json!(["candidate", "proposed", "committed"])
        );
        assert_eq!(response["release"]["status"], "not_checked");
        assert_eq!(response["learning"]["promotion"]["status"], "not_requested");
        assert!(candidate_learning(&mut response, "published", "x", &Value::Null).is_err());
        assert!(candidate_digest(&"A".repeat(64)).is_err());
    }
}

/// Projection for an actual retained current Rust registry execution. Its full
/// content verdict and source observations retain their separate meanings.
fn project_current_execution(
    response: &mut Value,
    binding: &ProjectBinding,
    task: Option<&SemanticTaskEnvelope>,
    native: &Value,
    scenario: Option<&str>,
) -> Result<()> {
    let actual = &native["actual_run"];
    let observed = &native["release_content"];
    let receipt = &observed["receipt"];
    let verdict = &observed["content_verdict"];
    let subject = &native["subject"];
    let typed: semantica_contracts::SemanticReleaseContentObservation =
        serde_json::from_value(observed.clone())?;
    typed
        .validate()
        .map_err(|e| anyhow::anyhow!("current release observation contract:{e:?}"))?;
    ensure!(
        binding.target_kind == "workspace"
            && native["registry_id"].as_str() == binding.workspace_id.as_deref(),
        "current registry identity differs from binding"
    );
    ensure!(
        subject["package_id"] == binding.package_id
            && subject["version"] == binding.baseline_version
            && subject["package_sha256"] == binding.baseline_digest,
        "current registry subject differs from binding"
    );
    ensure!(
        actual["package_id"] == binding.package_id
            && actual["package_version"] == binding.baseline_version,
        "current executed package differs from binding"
    );
    if let Some(id) = scenario {
        ensure!(
            actual["scenario_id"] == id,
            "current executed different scenario"
        );
    }
    ensure!(
        observed["runtime_source"] == response["runtime_source"]
            && observed["runner_execution"] == "retained"
            && observed["source_run_receipt_sha256"] == actual["receipt"]["receipt_sha256"],
        "current retained run/source binding differs"
    );
    ensure!(
        semantica_runtime::execution_provenance::verify_receipt(receipt)?,
        "current complete receipt integrity failed"
    );
    ensure!(
        receipt["package_id"] == actual["package_id"]
            && receipt["package_version"] == actual["package_version"]
            && receipt["package_digest"] == actual["package_digest"]
            && receipt["chapter_contract_sha256"] == subject["execution_projection_sha256"],
        "current full receipt subject binding differs"
    );
    ensure!(
        verdict["receipt_sha256"] == receipt["receipt_sha256"]
            && observed["engineering_adoption"] == "unknown"
            && observed["runtime_origin"] == "unknown"
            && observed["source_authenticity"] == "unknown"
            && observed["formal_release"] == "not_published",
        "current content/origin/adoption domains differ"
    );
    for field in ["operations", "oracle_checks", "reasons"] {
        ensure!(
            actual[field].is_array(),
            "current execution detail array missing"
        );
    }
    if let Some(task) = task {
        let caps = semantica_runtime::refinery_subject::current_observed_capabilities(receipt)?;
        ensure!(
            task.required_capabilities.iter().all(|c| caps.contains(c)),
            "task requires capabilities absent from actual current executor"
        );
    }
    response["corpus_found"] =
        json!({"status":"found","subject":subject,"baseline_subject_verified":true});
    response["execution"] = json!({"status":actual["status"],"operation":"current_rust_run_registry","subject":subject,"executor":{"package_id":actual["package_id"],"version":actual["package_version"],"digest":actual["package_digest"],"scenario_id":actual["scenario_id"],"receipt":receipt},"operations":actual["operations"],"oracle_checks":actual["oracle_checks"],"reasons":actual["reasons"],"task_sha256":task.map(|t|&t.source_sha256),"snapshot_profile":observed["snapshot_profile"]});
    response["regression"] =
        json!({"status":actual["cq_report"]["status"],"report":actual["cq_report"]});
    response["receipt"] = json!({"status":"verified","content_integrity":"verified","receipt":receipt,"runtime_source":observed["runtime_source"],"compilation_origin":"unknown","engineering_adoption":"unknown"});
    response["release"] = json!({"status":verdict["status"],"verdict":verdict,"formal_release":"not_published","compilation_origin":"unknown","engineering_adoption":"unknown"});
    response["learning"]["status"] = json!("passed");
    response["learning"]["reason"] = json!(
        "Actual execution evidence creates no reusable delta or engineering adoption by itself."
    );
    response["command_verdict"] = json!(if actual["status"] == "passed"
        && actual["cq_report"]["status"] == "passed"
        && verdict["status"] == "complete"
    {
        "passed"
    } else {
        "blocked"
    });
    Ok(())
}
