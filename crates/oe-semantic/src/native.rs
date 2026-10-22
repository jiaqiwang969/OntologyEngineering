#![allow(dead_code, unused_imports)]
// Historical transport API only. Production Python execution is retired.
//! Bounded calls into the source-locked external Semantica API.
//! Rust owns input identity, path policy and process limits. The static FFI
//! only constructs public native DTOs and invokes their methods. A completed
//! call is not a semantic pass, physical acceptance, or promotion authority.

use crate::runtime::{
    InstalledRuntimeObservation, RuntimeVerification, installed_runtime_artifact_sha256,
    read_runtime_source_lock, read_wheel_identity, verify_installed_runtime_record,
    verify_runtime_source_identity,
};
use crate::{canonical_json, digest, strict_json};
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, create_new_private_dir};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_NATIVE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_NATIVE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize)]
pub struct NativeExecution {
    pub operation: String,
    pub semantic_execution: String,
    pub runtime_identity: Option<RuntimeVerification>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub elapsed_seconds: f64,
    pub stderr: String,
}

impl NativeExecution {
    pub fn into_result(self) -> Result<Value> {
        ensure!(
            self.semantic_execution == "completed",
            "native_execution_{}: {}",
            self.semantic_execution,
            self.error.as_deref().unwrap_or("result unavailable")
        );
        self.result.context("native_result_missing")
    }
}

#[derive(Debug, Clone)]
pub struct DecisionReviewRequest {
    pub workspace: PathBuf,
    /// Exact ProjectOntologyBinding DTO, projected by the Rust caller.
    pub binding: Value,
    pub evidence: Vec<u8>,
    pub evidence_sha256: String,
    pub evidence_format: String,
    pub evidence_id: String,
    pub evidence_uri: String,
    pub evidence_captured_at: String,
    pub scope_id: String,
    pub focus_iri: String,
    pub focus_type_iri: String,
    pub query_asset_id: String,
    pub shape_asset_id: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenEngagementRequest {
    pub workspace: PathBuf,
    pub binding: Value,
    pub envelope: Value,
}

#[derive(Debug, Clone)]
pub struct RegistryRequest {
    pub workspace: PathBuf,
    pub binding: Value,
}

#[derive(Debug, Clone)]
pub struct RegistryRunRequest {
    pub registry: RegistryRequest,
    pub scenario_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransitionRequest {
    pub registry: RegistryRequest,
    pub delta_sha256: String,
    pub envelope: Value,
    pub recorded_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProposeRequest {
    pub registry: RegistryRequest,
    pub delta: Value,
    pub candidate_envelope: Value,
    pub proposed_envelope: Value,
    pub engagement: Value,
    pub recorded_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VerifyRequest {
    pub registry: RegistryRequest,
    pub delta_sha256: String,
    pub envelopes: BTreeMap<String, Value>,
    pub recorded_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NativeRuntime {
    root: SkillRoot,
    python: PathBuf,
    timeout: Duration,
    output_limit: usize,
}

impl NativeRuntime {
    pub fn new(root: &Path, python: &Path) -> Result<Self> {
        Self::with_limits(root, python, MAX_NATIVE_TIMEOUT, MAX_NATIVE_BYTES)
    }

    pub fn with_limits(
        root: &Path,
        python: &Path,
        timeout: Duration,
        output_limit: usize,
    ) -> Result<Self> {
        ensure!(
            !timeout.is_zero() && timeout <= MAX_NATIVE_TIMEOUT,
            "invalid_native_timeout"
        );
        ensure!(
            output_limit > 0 && output_limit <= MAX_NATIVE_BYTES,
            "invalid_native_output_limit"
        );
        let python = if !python.is_absolute() && python.components().count() > 1 {
            std::env::current_dir()?.join(python)
        } else {
            python.to_path_buf()
        };
        Ok(Self {
            root: SkillRoot::new(root)?,
            python,
            timeout,
            output_limit,
        })
    }

    fn execute<F>(&self, operation: &str, work: F) -> NativeExecution
    where
        F: FnOnce(&mut NativeExecution, &mut bool) -> Result<Value>,
    {
        let start = Instant::now();
        let mut result = NativeExecution {
            operation: operation.into(),
            semantic_execution: "not_run".into(),
            runtime_identity: None,
            result: None,
            error: None,
            elapsed_seconds: 0.0,
            stderr: String::new(),
        };
        let mut started = false;
        match work(&mut result, &mut started) {
            Ok(value) => {
                result.result = Some(value);
                result.semantic_execution = "completed".into();
            }
            Err(error) => {
                result.error = Some(format!("{error:#}"));
                if started {
                    result.semantic_execution = "failed".into();
                }
            }
        }
        result.elapsed_seconds = start.elapsed().as_secs_f64();
        result
    }

    fn identity(&self, _execution: &mut NativeExecution) -> Result<Value> {
        anyhow::bail!("legacy_semantica_execution_retired_in_v2_use_rust_kernel")
    }

    /// Creates the native ontology-runtime profile; no registry is modified.
    pub fn create_runtime(&self) -> NativeExecution {
        self.execute("create_runtime", |result, started| {
            self.identity(result)?;
            self.invoke(&json!({"operation":"create_runtime"}), started, result)
        })
    }

    /// Six public book APIs. Rust validates paths, arguments and returned bytes;
    /// the fixed external call retains Semantica's own implementation and DTOs.
    pub fn book_api(&self, request: &Value) -> NativeExecution {
        let operation = request["operation"].as_str().unwrap_or("invalid_book_api");
        self.execute(operation, |result, started| {
            crate::books::validate_native_request(&self.root, request)?;
            self.identity(result)?;
            self.invoke(request, started, result)
        })
    }

    /// Run and verify using one retained native runner in the same child.
    pub fn run_and_verify_package(
        &self,
        package_id: &str,
        scenario_id: Option<&str>,
    ) -> NativeExecution {
        self.run_and_verify_package_at(package_id, scenario_id, None)
    }

    /// The native package descriptor, manifest and loaded identity accompany
    /// the complete run and the same runner's verification. A blocked native
    /// release remains blocked; this boundary checks identities, not the gate.
    pub fn run_and_verify_package_at(
        &self,
        package_id: &str,
        scenario_id: Option<&str>,
        created_at: Option<&str>,
    ) -> NativeExecution {
        self.execute("run_and_verify_package", |result, started| {
            nonempty(package_id, "package_id")?;
            validate_run_options(scenario_id, created_at)?;
            let source = self.identity(result)?;
            let value = self.invoke(
                &json!({"operation":"run_and_verify_package", "package_id":package_id,
                "scenario_id":scenario_id, "created_at":created_at,"runtime_source":source}),
                started,
                result,
            )?;
            finish_builtin_run(result, value, package_id, scenario_id, created_at, &source)
        })
    }

    /// Public native registry diagnostics, including every domain's issue
    /// list. Unlike package discovery this invokes the native validators.
    pub fn registry_diagnostics(&self) -> NativeExecution {
        self.execute("registry_diagnostics", |result, started| {
            self.identity(result)?;
            let value = self.invoke(
                &json!({"operation":"registry_diagnostics"}),
                started,
                result,
            )?;
            validate_diagnostics(&value)?;
            Ok(value)
        })
    }

    /// Read-only execution of the current promoted package. A separate
    /// preflight rejects a stale binding before a run, while the actual method
    /// process returns and binds its registry and current subject again.
    pub fn run_registry(&self, request: &RegistryRunRequest) -> NativeExecution {
        self.execute("run_registry", |result, started| {
            validate_run_options(
                request.scenario_id.as_deref(),
                request.created_at.as_deref(),
            )?;
            let workspace = self.workspace(&request.registry.workspace, false)?;
            nonempty(
                text(&request.registry.binding, "workspace_id")?,
                "workspace_id",
            )?;
            nonempty(text(&request.registry.binding, "package_id")?, "package_id")?;
            nonempty(
                text(&request.registry.binding, "baseline_version")?,
                "baseline_version",
            )?;
            valid_hash(text(&request.registry.binding, "baseline_package_sha256")?)?;
            let source = self.identity(result)?;
            let before = self.invoke(
                &json!({"operation":"registry_subject", "workspace":workspace,
                "binding":request.registry.binding}),
                &mut false,
                result,
            )?;
            validate_subject(&before, &request.registry.binding)?;
            let value = self.invoke(
                &json!({"operation":"run_registry", "workspace":workspace,
                "binding":request.registry.binding,"scenario_id":request.scenario_id,
                "created_at":request.created_at,"runtime_source":source}),
                started,
                result,
            )?;
            finish_registry_run(result, value, request, &source)
        })
    }

    /// Native DTO parsing/hashing only; it neither opens nor adopts a project.
    pub fn task_and_binding(&self, envelope: &Value, binding: &Value) -> NativeExecution {
        self.execute("task_and_binding", |result, started| {
            ensure!(
                envelope.is_object() && binding.is_object(),
                "native_dto_not_object"
            );
            self.identity(result)?;
            self.invoke(
                &json!({"operation":"task_and_binding","envelope":envelope,"binding":binding}),
                started,
                result,
            )
        })
    }

    /// Record a new blocked engagement. No imported success receipt is accepted.
    /// Existing registries retain native append-only engagement semantics.
    pub fn open_engagement(&self, request: &OpenEngagementRequest) -> NativeExecution {
        self.execute("open_engagement", |result, started| {
            let workspace = self.workspace(&request.workspace, true)?;
            ensure!(request.envelope["requested_actions"] == json!(["engagement"]), "open_requires_exact_engagement_action");
            ensure!(request.envelope["project_id"] == request.binding["project_id"]
                && request.envelope["domain"] == request.binding["domain"], "task_binding_project_mismatch");
            let create = !workspace.exists();
            if create {
                let empty = json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]});
                ensure!(request.binding["baseline_version"] == "0"
                    && request.binding["baseline_package_sha256"] == digest(&canonical_json(&empty)?),
                    "only_empty_native_baseline_may_create_workspace");
            }
            let source = self.identity(result)?;
            let dto = self.invoke(&json!({"operation":"task_and_binding","envelope":request.envelope,
                "binding":request.binding}), &mut false, result)?;
            if !create {
                let registry = self.invoke(&json!({"operation":"registry_identity", "workspace":workspace}),
                    &mut false, result)?;
                ensure!(registry["registry_id"] == request.binding["workspace_id"], "registry_binding_identity_mismatch");
            }
            let phases: Vec<_> = ["execution","regression","receipt","release"].iter().map(|name| json!({
                "name":name,"required_capabilities":dto["envelope"]["required_capabilities"],
                "observed_capabilities":[],"evidence_sha256":if *name == "execution" { dto["envelope"]["envelope_sha256"].clone() } else { Value::Null },
                "details":{"reason":"engagement opened; no execution evidence submitted"},"execution_status":"blocked"
            })).collect();
            let value = self.invoke(&json!({"operation":"open_engagement", "workspace":workspace,"create_workspace":create,
                "envelope":dto["envelope"],"binding":dto["binding"],"runtime_source":source,"phases":phases,
                "engagement_id":format!("engagement:{}", text(&dto["envelope"],"task_id")?),
                "learning":{"status":"no_delta","rationale":"opening a task does not assert reusable industry knowledge"}}),
                started, result)?;
            ensure!(text(&value,"execution_registry_id")?==text(&request.binding,"workspace_id")?,"registry_binding_identity_mismatch");
            Ok(value)
        })
    }

    pub fn decision_review(&self, request: &DecisionReviewRequest) -> NativeExecution {
        self.execute("decision_review", |result, started| {
            let workspace = self.workspace(&request.workspace, false)?;
            ensure!(request.binding.is_object(), "native_binding_not_object");
            ensure!(!request.evidence.is_empty() && request.evidence.len() <= MAX_NATIVE_BYTES,
                "native_evidence_byte_budget");
            ensure!(digest(&request.evidence) == request.evidence_sha256, "native_evidence_hash_mismatch");
            let evidence = std::str::from_utf8(&request.evidence).context("native_evidence_not_utf8")?;
            ensure!(["turtle","nt","nquads","trig","xml","json-ld"].contains(&request.evidence_format.as_str()),
                "unsupported_native_evidence_format");
            for value in [&request.evidence_id,&request.evidence_uri,&request.evidence_captured_at,
                &request.scope_id,&request.focus_iri,&request.focus_type_iri,&request.query_asset_id,&request.shape_asset_id] {
                nonempty(value,"review_field")?;
            }
            self.identity(result)?;
            let subject_raw = self.invoke(&json!({"operation":"registry_subject", "workspace":workspace,
                "binding":request.binding}), &mut false, result)?;
            let subject = validate_subject(&subject_raw, &request.binding)?;
            // None resolves the current promoted package inside the native
            // gate. A stale baseline cannot explicitly select an old version.
            let native = self.invoke(&json!({"operation":"decision_review","workspace":workspace,
                "binding":request.binding,"evidence_utf8":evidence,"arguments":{
                    "evidence_sha256":request.evidence_sha256,"evidence_format":request.evidence_format,
                    "evidence_id":request.evidence_id,"evidence_uri":request.evidence_uri,
                    "evidence_captured_at":request.evidence_captured_at,"scope_id":request.scope_id,
                    "focus_iri":request.focus_iri,"focus_type_iri":request.focus_type_iri,
                    "query_asset_id":request.query_asset_id,"shape_asset_id":request.shape_asset_id,
                    "created_at":request.created_at}}), started, result)?;
            finish_decision_review(result, subject, native, request)
        })
    }

    /// Source-locked declarations. No claim that every declared package is complete.
    pub fn capabilities(&self) -> NativeExecution {
        self.execute("capabilities", |result, started| {
            self.identity(result)?;
            self.invoke(&json!({"operation":"capabilities"}), started, result)
        })
    }

    /// Public built-in descriptors with native loaded package identities.
    pub fn discover_packages(&self) -> NativeExecution {
        self.execute("discover_packages", |result, started| {
            self.identity(result)?;
            self.invoke(&json!({"operation":"discover_packages"}), started, result)
        })
    }

    pub fn normalize_delta(&self, value: &Value) -> NativeExecution {
        self.normalize("normalize_delta", value)
    }

    pub fn normalize_authorization(&self, value: &Value) -> NativeExecution {
        self.normalize("normalize_authorization", value)
    }

    /// Obtain the external engine's own runnable acceptance fixture. This does
    /// not approve a project, mutate a registry, or invent semantic assets.
    pub fn acceptance_delta(
        &self,
        package_id: &str,
        evidence: &Value,
        created_by: &str,
        created_at: &str,
        target_version: &str,
    ) -> NativeExecution {
        self.execute("acceptance_delta", |result,started| {
            self.identity(result)?;
            self.invoke(&json!({"operation":"acceptance_delta","package_id":package_id,"source_evidence":evidence,
                "created_by":created_by,"created_at":created_at,"target_version":target_version}),started,result)
        })
    }

    fn normalize(&self, operation: &str, value: &Value) -> NativeExecution {
        self.execute(operation, |result, started| {
            ensure!(value.is_object(), "native_dto_not_object");
            self.identity(result)?;
            self.invoke(
                &json!({"operation":operation,"value":value}),
                started,
                result,
            )
        })
    }

    /// Native DTO construction only. This does not require a registry to exist.
    pub fn transition_context(&self, request: &TransitionRequest, action: &str) -> NativeExecution {
        self.execute("transition_context", |result, started| {
            validate_transition_input(request, action)?;
            self.identity(result)?;
            let value = self.invoke(
                &json!({"operation":"transition_context","binding":request.registry.binding,
                "envelope":request.envelope,"action":action,"delta_sha256":request.delta_sha256}),
                started,
                result,
            )?;
            result.result = Some(value.clone());
            validate_context(&value, request, action)?;
            Ok(value)
        })
    }

    pub fn discover(&self, request: &RegistryRequest) -> NativeExecution {
        self.execute("discover", |result, started| {
            let workspace = self.workspace(&request.workspace, false)?;
            self.identity(result)?;
            let value = self.invoke(&json!({"operation":"registry_subject","workspace":workspace,
                "binding":request.binding}), started, result)?;
            result.result = Some(value.clone());
            let subject = validate_subject(&value, &request.binding)?;
            Ok(json!({"registry_id":value["registry_id"],"subject":subject,"descriptor":value["descriptor"],"manifest":value["manifest"]}))
        })
    }

    pub fn status(&self, registry: &RegistryRequest, delta_sha256: &str) -> NativeExecution {
        self.registry_read("status", registry, delta_sha256)
    }

    /// A missing/corrupt candidate is an error, never an empty event list.
    pub fn history(&self, registry: &RegistryRequest, delta_sha256: &str) -> NativeExecution {
        self.registry_read("history", registry, delta_sha256)
    }

    fn registry_read(
        &self,
        operation: &str,
        registry: &RegistryRequest,
        delta: &str,
    ) -> NativeExecution {
        self.execute(operation, |result, started| {
            valid_hash(delta)?;
            let workspace = self.workspace(&registry.workspace, false)?;
            self.identity(result)?;
            self.registry_call(
                json!({"operation":operation,"workspace":workspace,"delta_sha256":delta}),
                registry,
                started,
                result,
            )
        })
    }

    fn registry_call(
        &self,
        mut request: Value,
        registry: &RegistryRequest,
        started: &mut bool,
        result: &mut NativeExecution,
    ) -> Result<Value> {
        request["binding"] = registry.binding.clone();
        let value = self.invoke(&request, started, result)?;
        result.result = Some(value.clone());
        ensure!(
            text(&value, "registry_id")? == text(&registry.binding, "workspace_id")?,
            "registry_binding_identity_mismatch"
        );
        if let Some(state) = value.get("state") {
            ensure!(
                state["package_id"] == registry.binding["package_id"],
                "native_state_package_mismatch"
            );
            ensure!(
                state["delta_sha256"] == request["delta_sha256"],
                "native_state_candidate_mismatch"
            );
        }
        if let Some(events) = value.get("events") {
            let events = events.as_array().context("native_history_not_array")?;
            ensure!(!events.is_empty(), "native_history_empty");
            for event in events {
                ensure!(
                    event["package_id"] == registry.binding["package_id"]
                        && event["delta_sha256"] == request["delta_sha256"],
                    "native_history_identity_mismatch"
                );
            }
        }
        Ok(value)
    }

    /// Receipt evidence is parsed by native DTOs, not upgraded into passing evidence here.
    pub fn open_engagement_with_evidence(
        &self,
        request: &OpenEngagementRequest,
        engagement: &Value,
    ) -> NativeExecution {
        self.execute("open_engagement_with_evidence", |result, started| {
            ensure!(
                request.envelope["requested_actions"] == json!(["engagement"]),
                "open_requires_exact_engagement_action"
            );
            ensure!(
                text(&request.envelope, "project_id")? == text(&request.binding, "project_id")?
                    && text(&request.envelope, "domain")? == text(&request.binding, "domain")?,
                "task_binding_project_mismatch"
            );
            let workspace = self.workspace(&request.workspace, true)?;
            let create = !workspace.exists();
            if create {
                validate_empty_binding(&request.binding)?;
            }
            let source = self.identity(result)?;
            validate_engagement(engagement, &source)?;
            let registry = RegistryRequest {
                workspace: workspace.clone(),
                binding: request.binding.clone(),
            };
            self.registry_call(
                json!({"operation":"open_evidence","workspace":workspace,"create_workspace":create,
                "envelope":request.envelope,"engagement":engagement,"runtime_source":source}),
                &registry,
                started,
                result,
            )
        })
    }

    pub fn propose_candidate(&self, request: &ProposeRequest) -> NativeExecution {
        self.execute("propose_candidate", |result, started| {
            let workspace = self.workspace(&request.registry.workspace, false)?;
            let source = self.identity(result)?;
            let delta = self.invoke(&json!({"operation":"normalize_delta","value":request.delta}), &mut false, result)?;
            let hash = text(&delta,"delta_sha256")?;
            let candidate = TransitionRequest {registry:request.registry.clone(),delta_sha256:hash.into(),envelope:request.candidate_envelope.clone(),recorded_at:request.recorded_at.clone()};
            let proposed = TransitionRequest {envelope:request.proposed_envelope.clone(),..candidate.clone()};
            validate_transition_input(&candidate,"candidate")?;
            validate_transition_input(&proposed,"proposed")?;
            let mut engagement = request.engagement.clone();
            if engagement.get("receipt_sha256").is_none() && engagement["learning"]["status"] == "candidate" {
                engagement["learning"]["delta_sha256"] = hash.into();
            }
            validate_engagement(&engagement,&source)?;
            let value = self.registry_call(json!({"operation":"propose_candidate","workspace":workspace,
                "delta":delta,"delta_sha256":hash,"candidate_envelope":request.candidate_envelope,
                "proposed_envelope":request.proposed_envelope,"engagement":engagement,"runtime_source":source,
                "recorded_at":request.recorded_at}), &request.registry, started, result)?;
            validate_context(&value["transition_contexts"]["candidate"],&candidate,"candidate")?;
            validate_context(&value["transition_contexts"]["proposed"],&proposed,"proposed")?;
            ensure!(value["state"]["state"] == "proposed", "native_proposal_state_mismatch");
            Ok(value)
        })
    }

    pub fn commit_candidate(
        &self,
        request: &TransitionRequest,
        authorization: &Value,
    ) -> NativeExecution {
        self.authorized_transition("commit_candidate", "committed", request, authorization)
    }

    pub fn promote_candidate(
        &self,
        request: &TransitionRequest,
        authorization: &Value,
    ) -> NativeExecution {
        self.authorized_transition("promote_candidate", "promoted", request, authorization)
    }

    fn authorized_transition(
        &self,
        operation: &str,
        action: &str,
        request: &TransitionRequest,
        authorization: &Value,
    ) -> NativeExecution {
        self.execute(operation, |result, started| {
            validate_transition_input(request, action)?;
            let workspace = self.workspace(&request.registry.workspace, false)?;
            self.identity(result)?;
            let value = self.registry_call(
                json!({"operation":operation,"workspace":workspace,
                "delta_sha256":request.delta_sha256,"envelope":request.envelope,"action":action,
                "authorization":authorization,"recorded_at":request.recorded_at}),
                &request.registry,
                started,
                result,
            )?;
            validate_context(&value["transition_context"], request, action)?;
            if operation == "promote_candidate" {
                ensure!(
                    value["descriptor"]["package_id"] == request.registry.binding["package_id"],
                    "native_promotion_package_mismatch"
                );
            } else {
                ensure!(
                    value["state"]["state"] == "committed",
                    "native_commit_state_mismatch"
                );
            }
            Ok(value)
        })
    }

    /// Recovery is explicit: read verified state/history, then call the native
    /// verification API. Failed reads are never interpreted as initial state.
    pub fn verify_candidate(&self, request: &VerifyRequest) -> NativeExecution {
        self.execute("verify_candidate", |result, started| {
            let actions = ["execute_candidate","derive_regression_gate","regression_passed","derive_release_gate","release_complete"];
            ensure!(request.envelopes.len() == actions.len() && actions.iter().all(|a| request.envelopes.contains_key(*a)), "native_verification_actions_mismatch");
            for action in actions {
                validate_transition_input(&TransitionRequest {registry:request.registry.clone(),delta_sha256:request.delta_sha256.clone(),envelope:request.envelopes[action].clone(),recorded_at:request.recorded_at.clone()},action)?;
            }
            let workspace = self.workspace(&request.registry.workspace,false)?;
            let source = self.identity(result)?;
            let base = json!({"workspace":workspace,"delta_sha256":request.delta_sha256,"recorded_at":request.recorded_at});
            let mut status = base.clone(); status["operation"] = "status".into();
            let status = self.registry_call(status,&request.registry,&mut false,result)?;
            let state = text(&status["state"],"state")?;
            ensure!(["committed","regression_passed","release_complete"].contains(&state),"native_verification_invalid_current_state");
            let active = if state == "committed" { &actions[..] } else { &actions[3..] };
            let mut contexts = json!({});
            for action in active {
                let transition = TransitionRequest {registry:request.registry.clone(),delta_sha256:request.delta_sha256.clone(),envelope:request.envelopes[*action].clone(),recorded_at:request.recorded_at.clone()};
                let context = expected_transition_context(&transition,action)?;
                contexts[*action] = context;
            }
            let mut suite = Value::Null;
            let mut regression = Value::Null;
            let suite_hash;
            if state == "committed" {
                let mut call = base.clone(); call["operation"]="execute_candidate".into();
                call["context"]=contexts["execute_candidate"].clone(); call["runtime_source"]=source;
                let value = self.registry_call(call,&request.registry,started,result)?;
                ensure!(value["transition_context"] == contexts["execute_candidate"],"native_execution_context_mismatch");
                suite = value["execution_suite"].clone();
                suite_hash = text(&suite,"suite_sha256")?.to_owned();
                let mut call = base.clone(); call["operation"]="derive_gate_evidence".into();
                call["context"]=contexts["derive_regression_gate"].clone(); call["gate"]="regression".into(); call["execution_suite_sha256"]=suite_hash.clone().into();
                let value = self.registry_call(call,&request.registry,started,result)?;
                ensure!(value["transition_context"] == contexts["derive_regression_gate"],"native_gate_context_mismatch");
                regression = value["evidence"].clone();
            } else {
                let mut call = base.clone(); call["operation"]="history".into();
                let history = self.registry_call(call,&request.registry,&mut false,result)?;
                let event = history["events"].as_array().context("native_history_not_array")?.iter().find(|e| e["state"] == "regression_passed").context("native_regression_event_missing")?;
                suite_hash = text(&event["payload"],"execution_suite_sha256")?.to_owned();
            }
            valid_hash(&suite_hash)?;
            let mut call = base;
            call["operation"]="verify_candidate".into(); call["contexts"]=contexts.clone();
            call["regression_evidence"]=regression; call["execution_suite_sha256"]=suite_hash.clone().into();
            let value = self.registry_call(call,&request.registry,started,result)?;
            ensure!(value["transition_contexts"] == contexts,"native_verification_context_mismatch");
            let verification = &value["native_verification"];
            ensure!(verification["state"]["state"] == "release_complete" && verification["state"]["package_id"] == request.registry.binding["package_id"]
                && verification["state"]["delta_sha256"] == request.delta_sha256 && verification["execution_suite_sha256"] == suite_hash,"native_verification_identity_mismatch");
            Ok(json!({"registry_id":value["registry_id"],"state":verification["state"],"execution_suite":suite,"execution_suite_sha256":suite_hash,
                "regression_evidence":verification["regression_evidence"],"release_evidence":verification["release_evidence"],
                "native_verification":verification,"transition_contexts":contexts}))
        })
    }

    pub fn run_manifest(
        &self,
        manifest: &Path,
        scenario: &str,
        created_at: Option<&str>,
    ) -> NativeExecution {
        let mut execution = self.run_manifest_batch(manifest, &[scenario.to_owned()], created_at);
        execution.operation = "run_manifest".into();
        if execution.semantic_execution == "completed" {
            execution.result = execution
                .result
                .as_ref()
                .and_then(|v| v["checks"].as_array())
                .and_then(|v| v.first())
                .cloned();
        }
        execution
    }

    /// One bounded method process per explicit chunk (1..=64 scenarios), with
    /// one retained native runner. All asset bytes are snapshotted in Rust.
    pub fn run_manifest_batch(
        &self,
        manifest: &Path,
        scenarios: &[String],
        created_at: Option<&str>,
    ) -> NativeExecution {
        self.execute("run_manifest_batch", |result, started| {
            ensure!(
                !scenarios.is_empty() && scenarios.len() <= 64,
                "native_scenario_batch_limit"
            );
            for scenario in scenarios {
                nonempty(scenario, "scenario_id")?;
            }
            let manifest = self.manifest_snapshot(manifest)?;
            let source = self.identity(result)?;
            let value = self.invoke(
                &json!({"operation":"run_manifest_batch","manifest":manifest,"scenarios":scenarios,
                "runtime_source":source,"created_at":created_at}),
                started,
                result,
            )?;
            result.result = Some(value.clone());
            let checks = value["checks"]
                .as_array()
                .context("native_authoring_checks_missing")?;
            ensure!(
                checks.len() == scenarios.len(),
                "native_authoring_checks_count"
            );
            for (check, scenario) in checks.iter().zip(scenarios) {
                ensure!(
                    check["execution"]["scenario_id"] == *scenario,
                    "native_authoring_scenario_mismatch"
                );
            }
            Ok(value)
        })
    }

    fn manifest_snapshot(&self, manifest: &Path) -> Result<Value> {
        let manifest = self.root.private_path(manifest)?;
        let bytes = oe_core::read_regular_bytes(&manifest, MAX_NATIVE_BYTES)?;
        let mut value = strict_json(&bytes)?;
        let parent = manifest.parent().context("native_manifest_parent")?;
        let assets = value["assets"]
            .as_array_mut()
            .context("native_manifest_assets_missing")?;
        let mut size = bytes.len();
        for asset in assets {
            ensure!(
                asset.get("path").is_some() != asset.get("data").is_some(),
                "native_manifest_asset_source_ambiguous"
            );
            if let Some(path) = asset.get("path") {
                let path = path
                    .as_str()
                    .context("native_manifest_asset_path_not_text")?;
                let relative = crate::safe_relative(path)?;
                let path = self.root.private_path(&parent.join(relative))?;
                let data =
                    oe_core::read_regular_bytes(&path, MAX_NATIVE_BYTES.saturating_sub(size))?;
                size = size
                    .checked_add(data.len())
                    .context("native_manifest_byte_budget")?;
                ensure!(size <= MAX_NATIVE_BYTES, "native_manifest_byte_budget");
                ensure!(
                    asset["sha256"] == digest(&data),
                    "native_manifest_asset_hash_mismatch"
                );
                ensure!(
                    asset["format"].is_string(),
                    "native_manifest_explicit_format_required"
                );
                asset
                    .as_object_mut()
                    .context("native_asset_not_object")?
                    .remove("path");
                asset["data"] = std::str::from_utf8(&data)
                    .context("native_manifest_asset_not_utf8")?
                    .into();
            }
        }
        Ok(value)
    }

    fn workspace(&self, path: &Path, allow_missing: bool) -> Result<PathBuf> {
        let path = self.root.private_path(path)?;
        if path.exists() {
            ensure!(path.is_dir(), "native_workspace_not_directory");
            let mut count = 0;
            inspect_workspace(&path, 0, &mut count)?;
        } else {
            ensure!(allow_missing, "native_workspace_missing");
        }
        Ok(path)
    }

    fn invoke(
        &self,
        _request: &Value,
        _started: &mut bool,
        _execution: &mut NativeExecution,
    ) -> Result<Value> {
        anyhow::bail!("legacy_semantica_execution_retired_in_v2_use_rust_kernel")
    }
}

fn nonempty(value: &str, name: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "invalid_{name}");
    Ok(())
}

fn valid_hash(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid_native_hash"
    );
    Ok(())
}

fn validate_empty_binding(binding: &Value) -> Result<()> {
    ensure!(
        binding["baseline_version"] == "0"
            && binding["baseline_package_sha256"]
                == digest(canonical_json(
                    &json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]})
                )?),
        "only_empty_native_baseline_may_create_workspace"
    );
    Ok(())
}

fn validate_transition_input(request: &TransitionRequest, action: &str) -> Result<()> {
    valid_hash(&request.delta_sha256)?;
    ensure!(
        [
            "candidate",
            "proposed",
            "committed",
            "execute_candidate",
            "derive_regression_gate",
            "regression_passed",
            "derive_release_gate",
            "release_complete",
            "promoted"
        ]
        .contains(&action),
        "unsupported_native_transition_action"
    );
    ensure!(
        request.envelope["requested_actions"] == json!([action]),
        "native_transition_requires_exact_action"
    );
    ensure!(
        text(&request.envelope, "project_id")? == text(&request.registry.binding, "project_id")?
            && text(&request.envelope, "domain")? == text(&request.registry.binding, "domain")?,
        "task_binding_project_mismatch"
    );
    Ok(())
}

fn validate_context(context: &Value, request: &TransitionRequest, action: &str) -> Result<()> {
    ensure!(
        *context == expected_transition_context(request, action)?,
        "native_transition_context_mismatch"
    );
    Ok(())
}

/// Deterministic request identity only, not semantic authorization or native
/// execution. Used to compare retained local requests with verified native events.
pub fn expected_transition_context(request: &TransitionRequest, action: &str) -> Result<Value> {
    validate_transition_input(request, action)?;
    let mut context =
        json!({"schema_version":"1.0","action":action,"delta_sha256":request.delta_sha256});
    for (field, input, hash_key) in [
        ("envelope", &request.envelope, "envelope_sha256"),
        ("binding", &request.registry.binding, "binding_sha256"),
    ] {
        let mut expected = input.clone();
        let object = expected
            .as_object_mut()
            .context("native_context_dto_not_object")?;
        object.remove(hash_key);
        object.entry("schema_version").or_insert(json!("1.0"));
        let hash = digest(canonical_json(&expected)?);
        expected[hash_key] = hash.into();
        if let Some(declared) = input.get(hash_key) {
            ensure!(
                *declared == expected[hash_key],
                "native_declared_dto_hash_mismatch"
            );
        }
        context[field] = expected;
    }
    context["context_sha256"] = digest(canonical_json(&context)?).into();
    Ok(context)
}

fn validate_engagement(value: &Value, source: &Value) -> Result<()> {
    let fields = value.as_object().context("native_engagement_not_object")?;
    if fields.contains_key("receipt_sha256") {
        ensure!(
            value["runtime_source"] == *source,
            "native_engagement_source_mismatch"
        );
    } else {
        let expected = [
            "engagement_id",
            "execution",
            "regression",
            "receipt",
            "release",
            "learning",
            "execution_receipts",
            "created_at",
        ];
        ensure!(
            fields.len() == expected.len() && expected.iter().all(|k| fields.contains_key(*k)),
            "native_engagement_fields_mismatch"
        );
    }
    Ok(())
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    let s = value[name]
        .as_str()
        .with_context(|| format!("missing_{name}"))?;
    nonempty(s, name)?;
    Ok(s)
}

fn inspect_workspace(path: &Path, depth: usize, count: &mut usize) -> Result<()> {
    ensure!(
        depth <= 64 && *count <= 32_768,
        "native_workspace_inventory_budget"
    );
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "native_workspace_symlink"
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            *count += 1;
            inspect_workspace(&entry?.path(), depth + 1, count)?;
        }
    } else {
        ensure!(metadata.is_file(), "native_workspace_nonregular");
    }
    Ok(())
}

fn validate_subject(value: &Value, binding: &Value) -> Result<Value> {
    ensure!(
        value["registry_id"] == binding["workspace_id"],
        "registry_binding_identity_mismatch"
    );
    let descriptor = &value["descriptor"];
    for (key, expected) in [
        ("package_id", "package_id"),
        ("version", "baseline_version"),
        ("package_sha256", "baseline_package_sha256"),
    ] {
        ensure!(
            text(descriptor, key)? == text(binding, expected)?,
            "registry_current_baseline_mismatch"
        );
    }
    let assets = value["manifest"]["assets"]
        .as_array()
        .context("native_manifest_assets_missing")?;
    let contracts: Vec<_> = assets
        .iter()
        .filter(|asset| asset["role"] == "contract")
        .collect();
    ensure!(contracts.len() == 1, "native_manifest_contract_not_unique");
    let projection = text(contracts[0], "sha256")?;
    ensure!(
        projection.len() == 64
            && projection
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid_native_projection_hash"
    );
    Ok(
        json!({"package_id":descriptor["package_id"],"version":descriptor["version"],
        "package_sha256":descriptor["package_sha256"],"manifest_sha256":descriptor["manifest_sha256"],
        "execution_projection_sha256":projection,"promotion_record_sha256":descriptor["promotion_record_sha256"]}),
    )
}

fn validate_run_options(scenario: Option<&str>, created_at: Option<&str>) -> Result<()> {
    if let Some(value) = scenario {
        nonempty(value, "scenario_id")?;
    }
    if let Some(value) = created_at {
        chrono::DateTime::parse_from_rfc3339(value).context("invalid_native_created_at")?;
    }
    Ok(())
}

fn validate_diagnostics(value: &Value) -> Result<()> {
    for key in ["chapter_issues", "domain_issues"] {
        let issues = value[key]
            .as_array()
            .context("native_diagnostics_issues_missing")?;
        ensure!(
            issues.iter().all(Value::is_string),
            "native_diagnostics_invalid_issue"
        );
    }
    let mut ids = std::collections::BTreeSet::new();
    for key in ["chapters", "domains"] {
        for item in value[key]
            .as_array()
            .context("native_diagnostics_descriptors_missing")?
        {
            ensure!(
                ids.insert(text(item, "package_id")?),
                "native_diagnostics_duplicate_package"
            );
            text(item, "version")?;
            ensure!(
                item.get("manifest_path").is_none(),
                "native_diagnostics_host_path"
            );
        }
    }
    Ok(())
}

fn manifest_assets(manifest: &Value) -> Result<Value> {
    let mut hashes = serde_json::Map::new();
    for asset in manifest["assets"]
        .as_array()
        .context("native_manifest_assets_missing")?
    {
        let id = text(asset, "asset_id")?;
        let hash = text(asset, "sha256")?;
        valid_hash(hash)?;
        ensure!(
            hashes.insert(id.into(), hash.into()).is_none(),
            "native_manifest_duplicate_asset"
        );
    }
    ensure!(!hashes.is_empty(), "native_manifest_assets_empty");
    Ok(Value::Object(hashes))
}

#[allow(clippy::too_many_arguments)]
fn validate_run_result(
    run: &Value,
    verification: &Value,
    identity: &Value,
    manifest: &Value,
    source: &Value,
    scenario: Option<&str>,
    created_at: Option<&str>,
    receipt_integrity: &Value,
) -> Result<()> {
    ensure!(
        run["runner_contract"] == "semantica.chapter_packages.SemanticPackageRunner/v1"
            && run["schema_version"] == "1.0",
        "native_runner_contract_mismatch"
    );
    ensure!(
        matches!(
            run["status"].as_str(),
            Some("passed" | "failed" | "blocked")
        ),
        "native_run_status_invalid"
    );
    for (key, expected) in [
        ("package_id", "package_id"),
        ("package_version", "version"),
        ("package_digest", "digest"),
    ] {
        ensure!(
            text(run, key)? == text(identity, expected)?,
            "native_run_package_identity_mismatch"
        );
    }
    valid_hash(text(identity, "digest")?)?;
    ensure!(
        manifest["package_id"] == run["package_id"]
            && manifest["version"] == run["package_version"],
        "native_run_manifest_identity_mismatch"
    );
    text(run, "scenario_id")?;
    if let Some(scenario) = scenario.filter(|_| run["status"] == "passed") {
        ensure!(
            run["scenario_id"] == scenario,
            "native_run_scenario_mismatch"
        );
    }
    if let Some(created_at) = created_at {
        ensure!(
            run["created_at"] == created_at && verification["checked_at"] == created_at,
            "native_run_timestamp_mismatch"
        );
    }
    for key in ["operations", "oracle_checks", "reasons"] {
        ensure!(run[key].is_array(), "native_run_details_missing");
    }
    for key in [
        "capability_report",
        "cq_report",
        "shacl_report",
        "oracle_report",
        "release_verdict",
    ] {
        ensure!(run[key].is_object(), "native_run_report_missing");
    }
    ensure!(
        matches!(
            verification["status"].as_str(),
            Some("complete" | "blocked")
        ) && verification["checks"]
            .as_array()
            .is_some_and(|v| !v.is_empty()),
        "native_verification_missing"
    );
    let receipt = run
        .get("receipt")
        .context("native_run_receipt_field_missing")?;
    if receipt.is_null() {
        ensure!(
            receipt_integrity.is_null()
                && run["status"] != "passed"
                && verification["status"] == "blocked",
            "native_missing_receipt_claimed_success"
        );
        return Ok(());
    }
    ensure!(
        *receipt_integrity == true,
        "native_receipt_integrity_failed"
    );
    for key in ["package_id", "package_version", "package_digest"] {
        ensure!(receipt[key] == run[key], "native_receipt_package_mismatch");
    }
    for key in [
        "runtime_commit",
        "runtime_version",
        "runtime_artifact_sha256",
    ] {
        ensure!(
            text(receipt, key)? == text(source, key)?,
            "native_receipt_runtime_mismatch"
        );
    }
    ensure!(
        receipt["asset_hashes"] == manifest_assets(manifest)?,
        "native_receipt_asset_mismatch"
    );
    let receipt_hash = text(receipt, "receipt_sha256")?;
    valid_hash(receipt_hash)?;
    let mut content = receipt.clone();
    content
        .as_object_mut()
        .context("native_receipt_not_object")?
        .remove("receipt_sha256");
    ensure!(
        digest(canonical_json(&content)?) == receipt_hash,
        "native_receipt_digest_mismatch"
    );
    ensure!(
        verification["receipt_sha256"] == receipt_hash
            && run["release_verdict"]["receipt_sha256"] == receipt_hash,
        "native_verification_receipt_mismatch"
    );
    Ok(())
}

fn finish_builtin_run(
    execution: &mut NativeExecution,
    mut value: Value,
    package_id: &str,
    scenario: Option<&str>,
    created_at: Option<&str>,
    source: &Value,
) -> Result<Value> {
    execution.result = Some(value.clone());
    let packages = value["packages"]
        .as_array()
        .context("native_builtin_subject_missing")?;
    ensure!(packages.len() == 1, "native_builtin_subject_not_unique");
    let package = packages[0].clone();
    for key in ["descriptor", "manifest", "loaded_identity"] {
        value[key] = package[key].clone();
    }
    value["subject"] = json!({"kind":"builtin","package_id":value["loaded_identity"]["package_id"],
        "version":value["loaded_identity"]["version"],"package_digest":value["loaded_identity"]["digest"]});
    execution.result = Some(value.clone());
    ensure!(
        text(&value["descriptor"], "package_id")? == package_id
            && value["loaded_identity"]["package_id"] == package_id
            && value["descriptor"]["version"] == value["loaded_identity"]["version"],
        "native_builtin_subject_mismatch"
    );
    ensure!(
        value["descriptor"].get("manifest_path").is_none(),
        "native_builtin_host_path"
    );
    validate_run_result(
        &value["run"],
        &value["verification"],
        &value["loaded_identity"],
        &value["manifest"],
        source,
        scenario,
        created_at,
        &value["receipt_integrity"],
    )?;
    Ok(value)
}

fn finish_registry_run(
    execution: &mut NativeExecution,
    mut value: Value,
    request: &RegistryRunRequest,
    source: &Value,
) -> Result<Value> {
    execution.result = Some(value.clone());
    let subject = validate_subject(&value, &request.registry.binding)?;
    let current_after = &value["current_after"];
    validate_subject(current_after, &request.registry.binding)?;
    ensure!(
        current_after["descriptor"] == value["descriptor"]
            && current_after["manifest"] == value["manifest"],
        "native_registry_subject_changed_during_run"
    );
    ensure!(
        value["loaded_identity"]["package_id"] == request.registry.binding["package_id"]
            && value["loaded_identity"]["version"] == request.registry.binding["baseline_version"],
        "native_registry_loaded_identity_mismatch"
    );
    value["subject"] = subject;
    execution.result = Some(value.clone());
    validate_run_result(
        &value["executor"],
        &value["release"],
        &value["loaded_identity"],
        &value["manifest"],
        source,
        request.scenario_id.as_deref(),
        request.created_at.as_deref(),
        &value["receipt_integrity"],
    )?;
    Ok(value)
}

pub(crate) fn validate_review(review: &Value, request: &DecisionReviewRequest) -> Result<()> {
    ensure!(
        review["contract"] == "semantica.ontology.decision-review/v1",
        "native_review_contract_mismatch"
    );
    ensure!(
        matches!(review["status"].as_str(), Some("clear" | "blocked")),
        "native_review_invalid_status"
    );
    for (key, expected) in [
        ("scope_id", &request.scope_id),
        ("focus_iri", &request.focus_iri),
        ("focus_type_iri", &request.focus_type_iri),
    ] {
        ensure!(review[key] == *expected, "native_review_identity_mismatch");
    }
    let package = &review["package"];
    ensure!(
        package["id"] == request.binding["package_id"]
            && package["version"] == request.binding["baseline_version"]
            && package["registry_package_sha256"] == request.binding["baseline_package_sha256"]
            && package["origin"] == "promoted_registry"
            && package["query_asset_id"] == request.query_asset_id
            && package["shape_asset_id"] == request.shape_asset_id,
        "native_review_package_mismatch"
    );
    let evidence = &review["evidence"];
    ensure!(
        evidence["sha256"] == request.evidence_sha256
            && evidence["id"] == request.evidence_id
            && evidence["uri"] == request.evidence_uri
            && evidence["format"] == request.evidence_format
            && evidence["captured_at"] == request.evidence_captured_at,
        "native_review_evidence_mismatch"
    );
    ensure!(
        review["findings"].is_array() && review["violations"].is_array(),
        "native_review_findings_missing"
    );
    let mut content = review.clone();
    let hash = content
        .as_object_mut()
        .context("native_review_not_object")?
        .remove("report_sha256")
        .context("native_review_digest_missing")?;
    ensure!(
        hash == digest(&canonical_json(&content)?),
        "native_review_digest_mismatch"
    );
    Ok(())
}

fn finish_decision_review(
    execution: &mut NativeExecution,
    subject: Value,
    invocation: Value,
    request: &DecisionReviewRequest,
) -> Result<Value> {
    // Bind the registry instance used by evaluate_registry, not only the
    // earlier discovery process. Keep its identity and report on rejection.
    let output = json!({"subject":subject,"review":invocation["review"],
        "execution_registry_id":invocation["registry_id"]});
    execution.result = Some(output.clone());
    ensure!(
        text(&invocation, "registry_id")? == text(&request.binding, "workspace_id")?,
        "native_review_execution_registry_mismatch"
    );
    validate_review(&invocation["review"], request)?;
    Ok(output)
}

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Scratch {
    path: PathBuf,
}
impl Scratch {
    fn new(root: &SkillRoot) -> Result<Self> {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = root.private_path(Path::new(&format!(
            "var/native-call-{}-{stamp}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        )))?;
        create_new_private_dir(&path)?;
        let scratch = Self { path };
        create_new_private_dir(&scratch.path.join("cache"))?;
        Ok(scratch)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// Only fixed method dispatch, native constructors and JSON serialization.
// No ontology_engineering imports, eval, shell commands or policy algorithms.
#[cfg(all(test, feature = "historical-contract-tests"))]
mod tests {
    use super::*;

    fn execution_fixture() -> (Value, Value, RegistryRunRequest) {
        let hash = digest(b"asset fixture");
        let source = json!({"runtime_commit":"1".repeat(40),"runtime_version":"fixture",
            "runtime_artifact_sha256":"2".repeat(64)});
        let binding = json!({"workspace_id":"registry","package_id":"package","baseline_version":"1",
            "baseline_package_sha256":"3".repeat(64)});
        let descriptor = json!({"package_id":"package","version":"1","package_sha256":"3".repeat(64),
            "manifest_sha256":"4".repeat(64),"promotion_record_sha256":"5".repeat(64)});
        let manifest = json!({"package_id":"package","version":"1","assets":[{"asset_id":"asset","role":"contract","sha256":hash}]});
        let identity = json!({"package_id":"package","version":"1","digest":"6".repeat(64)});
        let mut receipt = json!({"package_id":"package","package_version":"1","package_digest":"6".repeat(64),
            "runtime_commit":source["runtime_commit"],"runtime_version":source["runtime_version"],
            "runtime_artifact_sha256":source["runtime_artifact_sha256"],"asset_hashes":{"asset":hash}});
        receipt["receipt_sha256"] = digest(canonical_json(&receipt).unwrap()).into();
        let verdict = json!({"status":"blocked","checked_at":"2026-09-27T00:00:00Z",
            "receipt_sha256":receipt["receipt_sha256"],"checks":[{"check_id":"fixture.partial","passed":false}]});
        let run = json!({"schema_version":"1.0","runner_contract":"semantica.chapter_packages.SemanticPackageRunner/v1",
            "package_id":"package","package_version":"1","package_digest":"6".repeat(64),"scenario_id":"scenario",
            "created_at":"2026-09-27T00:00:00Z","status":"passed","operations":[],"oracle_checks":[],"reasons":[],
            "capability_report":{},"cq_report":{},"shacl_report":{},"oracle_report":{},"receipt":receipt,"release_verdict":verdict});
        let value = json!({"registry_id":"registry","descriptor":descriptor,"manifest":manifest,
            "loaded_identity":identity,"executor":run,"release":verdict,"receipt_integrity":true,
            "current_after":{"registry_id":"registry","descriptor":descriptor,"manifest":manifest}});
        (
            value,
            source,
            RegistryRunRequest {
                registry: RegistryRequest {
                    workspace: PathBuf::from("unused"),
                    binding,
                },
                scenario_id: Some("scenario".into()),
                created_at: Some("2026-09-27T00:00:00Z".into()),
            },
        )
    }

    #[test]
    fn actual_run_process_identity_cannot_reuse_an_earlier_registry_preflight() {
        let (value, source, request) = execution_fixture();
        let mut proof = execution();
        let valid = finish_registry_run(&mut proof, value.clone(), &request, &source).unwrap();
        assert_eq!(valid["executor"]["status"], "passed");
        assert_eq!(valid["release"]["status"], "blocked");
        for pointer in [
            "/registry_id",
            "/current_after/registry_id",
            "/current_after/descriptor/package_sha256",
        ] {
            let mut changed = value.clone();
            *changed.pointer_mut(pointer).unwrap() = "replacement".into();
            let mut proof = execution();
            assert!(finish_registry_run(&mut proof, changed.clone(), &request, &source).is_err());
            assert_eq!(proof.result.unwrap(), changed);
        }
    }

    #[test]
    fn execution_identity_checks_reject_rehashed_wrong_receipts_without_losing_details() {
        let (value, source, request) = execution_fixture();
        for pointer in [
            "/executor/receipt/runtime_commit",
            "/executor/receipt/package_digest",
            "/executor/receipt/asset_hashes/asset",
        ] {
            let mut changed = value.clone();
            *changed.pointer_mut(pointer).unwrap() = "7".repeat(64).into();
            let mut content = changed["executor"]["receipt"].clone();
            content.as_object_mut().unwrap().remove("receipt_sha256");
            let hash = digest(canonical_json(&content).unwrap());
            changed["executor"]["receipt"]["receipt_sha256"] = hash.clone().into();
            changed["executor"]["release_verdict"]["receipt_sha256"] = hash.clone().into();
            changed["release"]["receipt_sha256"] = hash.into();
            let mut proof = execution();
            assert!(finish_registry_run(&mut proof, changed.clone(), &request, &source).is_err());
            assert_eq!(proof.result.unwrap()["executor"], changed["executor"]);
        }
        let mut changed = value.clone();
        changed["release"]["receipt_sha256"] = "9".repeat(64).into();
        assert!(finish_registry_run(&mut execution(), changed, &request, &source).is_err());
        let mut changed = value;
        changed["receipt_integrity"] = false.into();
        assert!(finish_registry_run(&mut execution(), changed, &request, &source).is_err());
    }

    #[test]
    fn blocked_unresolved_execution_is_preserved_and_never_upgraded() {
        let (mut value, source, mut request) = execution_fixture();
        request.scenario_id = None;
        value["executor"]["status"] = "blocked".into();
        value["executor"]["scenario_id"] = "unresolved".into();
        value["executor"]["oracle_checks"] = json!([{"check_id":"oracle.contract","status":"blocked","actual":{"error_code":"ambiguous_scenario"}}]);
        let output =
            finish_registry_run(&mut execution(), value.clone(), &request, &source).unwrap();
        assert_eq!(output["executor"], value["executor"]);
        assert_eq!(output["release"], value["release"]);
        value["executor"]["receipt"] = Value::Null;
        value["receipt_integrity"] = Value::Null;
        let output =
            finish_registry_run(&mut execution(), value.clone(), &request, &source).unwrap();
        assert!(output["executor"]["receipt"].is_null());
        value["executor"]["status"] = "passed".into();
        assert!(finish_registry_run(&mut execution(), value, &request, &source).is_err());
    }

    #[test]
    fn registry_diagnostic_issues_are_retained_and_ambiguous_ids_rejected() {
        let value = json!({"chapter_issues":["native issue"],"domain_issues":["domain: native issue"],
            "chapters":[{"package_id":"chapter","version":"1"}],"domains":[{"package_id":"domain","version":"1"}]});
        validate_diagnostics(&value).unwrap();
        let mut bad = value.clone();
        bad["domains"][0]["package_id"] = "chapter".into();
        assert!(validate_diagnostics(&bad).is_err());
        let mut bad = value;
        bad["chapter_issues"] = json!([{"not":"a string"}]);
        assert!(validate_diagnostics(&bad).is_err());
    }

    fn fixture() -> Scratch {
        let root = SkillRoot::new(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap(),
        )
        .unwrap();
        let scratch = Scratch::new(&root).unwrap();
        fs::write(scratch.path.join("SKILL.md"), "fixture").unwrap();
        scratch
    }

    fn execution() -> NativeExecution {
        NativeExecution {
            operation: "fixture".into(),
            semantic_execution: "not_run".into(),
            runtime_identity: None,
            result: None,
            error: None,
            elapsed_seconds: 0.0,
            stderr: String::new(),
        }
    }

    #[test]
    fn absent_identity_never_starts_the_supplied_interpreter() {
        let scratch = fixture();
        let runtime = NativeRuntime::new(&scratch.path, Path::new("/does/not/exist")).unwrap();
        let result = runtime.create_runtime();
        assert_eq!(result.semantic_execution, "not_run");
        assert!(result.runtime_identity.is_none());
        assert!(result.result.is_none());
        assert!(result.into_result().is_err());
    }

    #[test]
    fn changed_registry_after_valid_discovery_rejects_same_package_and_report() {
        let binding = json!({"workspace_id":"declared-registry","package_id":"synthetic.package",
            "baseline_version":"1","baseline_package_sha256":"a".repeat(64)});
        let descriptor = json!({"package_id":"synthetic.package","version":"1",
            "package_sha256":"a".repeat(64),"manifest_sha256":"b".repeat(64),"promotion_record_sha256":"c".repeat(64)});
        let subject = validate_subject(
            &json!({"registry_id":"declared-registry","descriptor":descriptor,
            "manifest":{"assets":[{"role":"contract","sha256":"d".repeat(64)}]}}),
            &binding,
        )
        .unwrap();
        let request = DecisionReviewRequest {
            workspace: "var/synthetic".into(),
            binding,
            evidence: b"fixture".to_vec(),
            evidence_sha256: digest(b"fixture"),
            evidence_format: "turtle".into(),
            evidence_id: "source".into(),
            evidence_uri: "evidence:source".into(),
            evidence_captured_at: "2026-09-27T00:00:00Z".into(),
            scope_id: "scope".into(),
            focus_iri: "urn:focus".into(),
            focus_type_iri: "urn:Type".into(),
            query_asset_id: "query".into(),
            shape_asset_id: "shape".into(),
            created_at: None,
        };
        let mut report = json!({"contract":"semantica.ontology.decision-review/v1","status":"clear",
            "scope_id":request.scope_id,"focus_iri":request.focus_iri,"focus_type_iri":request.focus_type_iri,
            "package":{"id":"synthetic.package","version":"1","registry_package_sha256":"a".repeat(64),
                "origin":"promoted_registry","query_asset_id":"query","shape_asset_id":"shape"},
            "evidence":{"sha256":request.evidence_sha256,"id":"source","uri":"evidence:source","format":"turtle",
                "captured_at":"2026-09-27T00:00:00Z"},"findings":[],"violations":[]});
        report["report_sha256"] = digest(canonical_json(&report).unwrap()).into();
        // Both checks used before the fix pass: the discovery registry matches,
        // and exactly this returned report has the expected hash/package/evidence.
        validate_review(&report, &request).unwrap();
        let mut accepted = execution();
        finish_decision_review(
            &mut accepted,
            subject.clone(),
            json!({"registry_id":"declared-registry","review":report}),
            &request,
        )
        .unwrap();
        for actual in [json!("replacement-registry"), Value::Null] {
            let scratch = fixture();
            let runtime = NativeRuntime::new(&scratch.path, Path::new("/not/invoked")).unwrap();
            let result = runtime.execute("decision_review", |result, started| {
                *started = true;
                finish_decision_review(
                    result,
                    subject.clone(),
                    json!({"registry_id":actual,"review":report}),
                    &request,
                )
            });
            assert_eq!(result.semantic_execution, "failed");
            let retained = result.result.as_ref().unwrap();
            assert_eq!(retained["review"], report);
            assert_eq!(retained["execution_registry_id"], actual);
            assert!(result.into_result().is_err());
        }
    }

    #[test]
    fn limits_and_workspace_boundaries_fail_before_execution() {
        let scratch = fixture();
        assert!(
            NativeRuntime::with_limits(&scratch.path, Path::new("python"), Duration::ZERO, 1)
                .is_err()
        );
        let runtime = NativeRuntime::new(&scratch.path, Path::new("/does/not/exist")).unwrap();
        assert!(
            runtime
                .workspace(Path::new("sources/registry"), true)
                .is_err()
        );
        assert!(
            runtime
                .workspace(Path::new("var/../sources/registry"), true)
                .is_err()
        );
        let out = runtime.open_engagement(&OpenEngagementRequest {
            workspace: "var/workspace".into(),
            binding: json!({"baseline_version":"1"}),
            envelope: json!({"requested_actions":["engagement"]}),
        });
        assert_eq!(out.semantic_execution, "not_run");
        assert!(!scratch.path.join("var/workspace").exists());
    }

    #[cfg(unix)]
    #[test]
    fn native_workspace_rejects_internal_symlinks_and_fifo() {
        use std::os::unix::fs::symlink;
        let scratch = fixture();
        let workspace = scratch.path.join("var/workspace");
        fs::create_dir_all(&workspace).unwrap();
        let runtime = NativeRuntime::new(&scratch.path, Path::new("python")).unwrap();
        symlink(&scratch.path, workspace.join("link")).unwrap();
        assert!(runtime.workspace(&workspace, false).is_err());
        fs::remove_file(workspace.join("link")).unwrap();
        let fifo = std::ffi::CString::new(workspace.join("fifo").to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(runtime.workspace(&workspace, false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn native_process_bounds_inherited_pipes_output_and_invalid_results() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = fixture();
        let interpreter = scratch.path.join("fixture-interpreter");
        for (body, expected) in [
            ("sleep 10 &\nexit 0", "timeout"),
            ("printf '123456789'", "output_limit"),
            ("printf 'bad'", "invalid_json"),
            (
                "printf '{\"ok\":true,\"result\":{}}'",
                "observation_missing",
            ),
            (
                "printf '{\"ok\":false,\"error_type\":\"NativeError\",\"error\":\"blocked\"}'",
                "api_rejected",
            ),
        ] {
            fs::write(&interpreter, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700)).unwrap();
            let limit = if expected == "output_limit" { 8 } else { 1024 };
            let runtime = NativeRuntime::with_limits(
                &scratch.path,
                &interpreter,
                Duration::from_millis(120),
                limit,
            )
            .unwrap();
            let mut started = false;
            let start = Instant::now();
            let error = runtime
                .invoke(
                    &json!({"operation":"fixture"}),
                    &mut started,
                    &mut execution(),
                )
                .unwrap_err();
            assert!(started);
            assert!(format!("{error:#}").contains(expected), "{error:#}");
            assert!(start.elapsed() < Duration::from_secs(2));
        }
    }

    #[cfg(unix)]
    #[test]
    fn method_process_identity_rejects_unrecorded_runtime_bytes() {
        use crate::runtime::{RuntimeSourceLock, SOURCE_LOCK_SCHEMA};
        use std::io::{Cursor, Write};
        use std::os::unix::fs::PermissionsExt;
        let scratch = fixture();
        let version = "9.9.9+staging.1";
        let init = b"__version__ = \"9.9.9+staging.1\"\n";
        let dist = format!("semantica-{version}.dist-info");
        let record = format!(
            "semantica/__init__.py,sha256=2CJkpqbOcvl8V5H5C1UkPHYLYdjf62hJ_6pnPx8eIJc,32\r\n{dist}/RECORD,,\r\n"
        );
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, raw) in [
            ("semantica/__init__.py".into(), init.to_vec()),
            (
                format!("{dist}/METADATA"),
                format!("Metadata-Version: 2.4\nName: semantica\nVersion: {version}\n")
                    .into_bytes(),
            ),
            (
                format!("{dist}/WHEEL"),
                b"Wheel-Version: 1.0\nTag: py3-none-any\n".to_vec(),
            ),
            (format!("{dist}/RECORD"), record.into_bytes()),
        ] {
            archive
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default().unix_permissions(0o644),
                )
                .unwrap();
            archive.write_all(&raw).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();
        let identity = RuntimeSourceLock {
            commit: "1".repeat(40),
            version: version.into(),
            artifact_filename: format!("semantica-{version}-py3-none-any.whl"),
            artifact_sha256: digest(&bytes),
        };
        fs::create_dir_all(scratch.path.join("runtime/vendor")).unwrap();
        fs::write(
            scratch
                .path
                .join("runtime/vendor")
                .join(&identity.artifact_filename),
            bytes,
        )
        .unwrap();
        fs::write(scratch.path.join("runtime/semantica-source-lock.json"),json!({"$schema":SOURCE_LOCK_SCHEMA,
            "source":{"commit":identity.commit,"version":version},"artifact":{"filename":identity.artifact_filename,"sha256":identity.artifact_sha256}}).to_string()).unwrap();
        let package = scratch.path.join("site/semantica");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("__init__.py"), init).unwrap();
        let interpreter = scratch.path.join("fixture-interpreter");
        let observed = InstalledRuntimeObservation {
            python_version: [3, 13, 1],
            python_executable: interpreter.clone(),
            environment_prefix: scratch.path.clone(),
            imported_version: version.into(),
            distribution_version: version.into(),
            package_root: package.clone(),
            imported_file: package.join("__init__.py"),
            imported_paths: vec![package.clone()],
            direct_url: json!({"archive_info":{"hashes":{"sha256":identity.artifact_sha256}}})
                .to_string(),
        };
        let response =
            json!({"ok":true,"result":{"transport_fixture":true},"runtime_observation":observed});
        fs::write(
            &interpreter,
            format!("#!/bin/sh\ncat <<'NATIVE_FIXTURE'\n{response}\nNATIVE_FIXTURE\n"),
        )
        .unwrap();
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700)).unwrap();
        let runtime = NativeRuntime::new(&scratch.path, &interpreter).unwrap();
        let mut observation = execution();
        observation.runtime_identity = Some(RuntimeVerification {
            source_lock: identity,
            record_files: 1,
            observation_kind: "transport_fixture_not_runtime_execution".into(),
            semantic_execution: "not_run".into(),
            staging_descriptor_sha256: None,
        });
        assert_eq!(
            runtime
                .invoke(&json!({}), &mut false, &mut observation)
                .unwrap()["transport_fixture"],
            true
        );
        fs::write(package.join("unrecorded.py"), b"VALUE = 'unexpected'\n").unwrap();
        let error = runtime
            .invoke(&json!({}), &mut false, &mut observation)
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("unrecorded files"),
            "{error:#}"
        );
    }
}
