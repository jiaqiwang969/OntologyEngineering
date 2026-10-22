//! Current Rust composition for the existing industry lifecycle workspace.
//! Public receipt JSON is data, never a capability to certify its own execution.
use crate::{
    native::{
        DecisionReviewRequest, NativeExecution, OpenEngagementRequest, ProposeRequest,
        RegistryRequest, RegistryRunRequest, TransitionRequest, VerifyRequest,
    },
    refinery_storage::FileStore,
};
use anyhow::{Context, Result, bail, ensure};
use oe_core::{SkillRoot, read_regular_bytes};
use semantica_runtime::{
    books::SourceProvider, refinery::RefineryEngine, refinery_contracts as wire,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, time::Instant};

pub const PROFILE: &str = "semantica.refinery-rust-observation/1";
const LIMIT: usize = 16 * 1024 * 1024;
#[derive(Debug, Clone)]
pub struct RustRuntime {
    pub(crate) root: SkillRoot,
}
impl RustRuntime {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: SkillRoot::new(root)?,
        })
    }
    pub fn source(&self) -> Result<Value> {
        let value = crate::books::rust::source_execution::runtime_identity(&self.root)?;
        let source = json!({"schema":"semantica.refinery-rust-source/1","source_sha256":value["source_sha256"],"executable_sha256":value["runtime_artifact_sha256"],"executable_size_bytes":value["runtime_artifact_size_bytes"],"status":"source_bound","compilation_origin":"unknown","engineering_adoption":"unknown"});
        let dto: semantica_contracts::SemanticRefineryRustSource =
            serde_json::from_slice(&crate::canonical_json(&source)?)?;
        dto.validate()
            .map_err(|e| anyhow::anyhow!("refinery_source_contract:{e:?}"))?;
        Ok(source)
    }
    // One actual source read/diagnostic for both the unchanged read wrapper and
    // the commit sibling while it still owns the original registry lock.
    fn recheck_source(&self, expected: &Value) -> (Result<Value>, Value) {
        let after = self.source();
        let observation = match &after {
            Ok(current) if current == expected => json!({"status":"matched"}),
            Ok(current) => json!({"status":"changed","current_source":current}),
            Err(error) => json!({"status":"failed","error":format!("{error:#}")}),
        };
        (after, observation)
    }
    pub(crate) fn execute(
        &self,
        operation: &str,
        work: impl FnOnce(&Value) -> Result<Value>,
    ) -> NativeExecution {
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
        let mut observed_source = None;
        let mut retained = None;
        let mut source_recheck = json!({"status":"not_run"});
        let performed = (|| -> Result<Value> {
            let source = self.source()?;
            observed_source = Some(source.clone());
            let outcome = work(&source);
            if let Ok(value) = &outcome {
                retained = Some(value.clone());
            }
            let (after, observation) = self.recheck_source(&source);
            source_recheck = observation;
            if let Err(error) = &outcome {
                bail!("{error:#};source_recheck={source_recheck}");
            }
            ensure!(
                after? == source,
                "refinery_current_source_changed_during_operation"
            );
            let mut value = outcome?;
            ensure!(value.is_object(), "refinery_result_not_mapping");
            value["rust_execution"] = source;
            value["engine"] = json!("semantica.rust-runtime");
            value["execution_profile"] = json!(PROFILE);
            value["engineering_adoption"] = json!("unknown");
            value["source_recheck"] = source_recheck.clone();
            Ok(value)
        })();
        match performed {
            Ok(value) => {
                result.semantic_execution = "completed".into();
                result.result = Some(value)
            }
            Err(e) => {
                result.semantic_execution = if observed_source.is_some() {
                    "failed"
                } else {
                    "not_run"
                }
                .into();
                result.error = Some(format!("{e:#}"));
                result.result = Some(
                    json!({"status":"blocked","engine":"semantica.rust-runtime","profile":PROFILE,"error":format!("{e:#}"),"rust_execution":observed_source,"source_recheck":source_recheck,"partial_result":retained,"mutation_on_error":"not_inferred","engineering_adoption":"unknown"}),
                );
            }
        }
        result.elapsed_seconds = start.elapsed().as_secs_f64();
        result
    }
    pub fn capabilities(&self) -> NativeExecution {
        self.execute("capabilities",|source|Ok(json!({"schema":"semantica.refinery-rust-capabilities/1","engine":"semantica.rust-runtime","profile":PROFILE,"source":source,
        "operations":["open_engagement","propose_candidate","commit_candidate","verify_candidate","promote_candidate","history","run_registry","discover","run_manifest","run_manifest_batch","run_package","discover_packages","registry_diagnostics","decision_review"],
        "decision_review_contract":semantica_runtime::promoted_review::REVIEW_CONTRACT,"decision_review_available":true,
        "decision_review_profile":semantica_runtime::promoted_review::PROFILE,
        "decision_review_scope":"current promoted version and SHA; task/evidence, SELECT finding, same-session SHACL/default graph and evidence-origin focus; no typed Project or fact adoption",
        "conditional_observation_operations":["propose_candidate"],
        "full_subject_profile":crate::refinery_full::FULL_SCHEMA,"full_subject_contract":"complete target projection, actual reference and target scenarios, prior CQ, full receipt/PROV, ordered regression6/release6 and immutable registry; finite tested inputs",
        "coverage":"partial","unsupported":["named-graph SHACL and non-Turtle shapes in promoted review","complete original Python library/API/platform scope","authenticated compilation/source origin","engineering fact adoption","total operation deadline and complete directory membership transport budget"],"publication_owned_externally":true})))
    }
    pub(crate) fn registry<'a>(
        &self,
        request: &RegistryRequest,
        create: bool,
        time: &str,
    ) -> Result<FileStore> {
        let binding = wire::binding(&request.binding)?;
        if create {
            ensure!(
                binding["baseline_version"] == "0"
                    && binding["baseline_package_sha256"] == wire::EMPTY_PACKAGE_SHA256,
                "refinery_nonempty_initial_binding"
            );
        }
        FileStore::open(
            &self.root,
            &request.workspace,
            binding["workspace_id"]
                .as_str()
                .context("refinery_workspace_id_missing")?,
            time,
            create,
        )
    }
    pub(crate) fn engine<'a>(
        &self,
        store: &'a FileStore,
        request: &RegistryRequest,
    ) -> Result<RefineryEngine<'a>> {
        RefineryEngine::new(
            store,
            request.binding["workspace_id"]
                .as_str()
                .context("refinery_workspace_id_missing")?,
        )
    }
    fn blocked_receipt(
        &self,
        request: &OpenEngagementRequest,
        source: &Value,
        learning: Value,
        details: Value,
    ) -> Result<Value> {
        let binding = wire::binding(&request.binding)?;
        let task = wire::task(&request.envelope)?;
        let time = task["created_at"].clone();
        let required = task["required_capabilities"].clone();
        let evidence = wire::digest(&details)?;
        let mut value = json!({"schema_version":"1.0","contract":wire::CONTRACT,"profile":PROFILE,"engagement_id":format!("rust-engagement-{}",task["envelope_sha256"].as_str().unwrap_or("unknown")),"envelope_sha256":task["envelope_sha256"],"binding_sha256":binding["binding_sha256"],"required_capabilities":required,"runtime_source":source,"learning":learning,"execution_receipts":[],"created_at":time});
        for name in ["execution", "regression", "receipt", "release"] {
            value[name] = json!({"name":name,"status":"blocked","required_capabilities":required,"observed_capabilities":[],"missing_capabilities":required,"evidence_sha256":evidence,"details":details});
        }
        wire::prepare_rust_engagement(&value)
    }
    fn bind_delta_manifest(&self, delta: &Value, manifest: &Path) -> Result<String> {
        let path = self.root.private_path(manifest)?;
        let bytes = read_regular_bytes(&path, LIMIT)?;
        let value = crate::strict_json(&bytes)?;
        ensure!(
            value["schema_version"] == "1.0"
                && value["package_id"] == delta["package_id"]
                && value["version"] == delta["target_version"],
            "refinery_delta_manifest_identity_mismatch"
        );
        let mut changed = BTreeMap::<String, &Value>::new();
        for category in semantica_runtime::refinery::CATEGORIES {
            for asset in delta[*category]
                .as_array()
                .context("refinery_delta_array_missing")?
            {
                ensure!(
                    asset["operation"] == "add",
                    "refinery_observation_nonempty_baseline_not_connected"
                );
                let id = asset["asset_id"]
                    .as_str()
                    .context("refinery_asset_id_missing")?;
                ensure!(
                    changed.insert(id.into(), asset).is_none(),
                    "refinery_observation_ambiguous_asset_id"
                );
            }
        }
        let assets = value["assets"]
            .as_array()
            .context("refinery_manifest_assets_missing")?;
        ensure!(
            assets.len() == changed.len(),
            "refinery_delta_manifest_asset_coverage_mismatch"
        );
        let files = crate::books::rust::FileProvider::new(BTreeMap::from([(
            "input".into(),
            path.parent()
                .context("refinery_manifest_parent_missing")?
                .to_path_buf(),
        )]))?;
        let provider = ManifestProvider(&files);
        for asset in assets {
            let id = asset["asset_id"]
                .as_str()
                .context("refinery_asset_id_missing")?;
            let delta_asset = changed
                .remove(id)
                .context("refinery_manifest_asset_not_in_delta")?;
            ensure!(
                asset["sha256"] == delta_asset["sha256"] && asset["role"] == delta_asset["role"],
                "refinery_delta_manifest_asset_binding_mismatch"
            );
            let current = provider.read(
                asset["path"]
                    .as_str()
                    .context("refinery_manifest_asset_path_missing")?,
            )?;
            ensure!(
                semantica_runtime::sha256(&current) == delta_asset["sha256"],
                "refinery_delta_manifest_asset_bytes_mismatch"
            );
            ensure!(
                wire::asset_content(delta_asset)?.as_deref() == Some(current.as_slice()),
                "refinery_delta_manifest_content_mismatch"
            );
        }
        ensure!(changed.is_empty(), "refinery_delta_asset_unobserved");
        files.verify()?;
        ensure!(
            read_regular_bytes(&path, LIMIT)? == bytes,
            "refinery_manifest_changed_during_binding"
        );
        Ok(semantica_runtime::sha256(bytes))
    }
    pub fn open_engagement(&self, request: &OpenEngagementRequest) -> NativeExecution {
        self.execute("open_engagement",|source|{
            let receipt=self.blocked_receipt(request,source,json!({"status":"no_delta","rationale":"No reusable candidate and actual gate evidence supplied.","delta_sha256":null}),json!({"profile":PROFILE,"reason":"actual subject execution and original release/provenance gates are not available","engineering_adoption":"unknown"}))?;
            let registry=RegistryRequest{workspace:request.workspace.clone(),binding:request.binding.clone()};let store=self.registry(&registry,true,request.envelope["created_at"].as_str().context("refinery_created_at_missing")?)?;
            let engine=self.engine(&store,&registry)?;let engagement=engine.open_data(&request.envelope,&request.binding,&receipt)?;
            Ok(json!({"engagement":engagement,"recorded":true,"ontology_truth_mutated":false}))
        })
    }
    pub fn open_engagement_with_evidence(
        &self,
        request: &OpenEngagementRequest,
        input: &Value,
    ) -> NativeExecution {
        if input["schema"] == crate::refinery_full::FULL_SCHEMA {
            return self.full_open(request, input);
        }
        self.execute("open_engagement",|source|{
            // A caller's JSON can describe a requested observation, not passed
            // phases, source attestation or a retained production capability.
            ensure!(input["schema"]=="semantica.refinery-observation-request/1","refinery_caller_receipt_is_not_execution_evidence");
            let plan=observation_request(input)?;
            let manifest=self.root.private_path(Path::new(&plan.manifest_path))?;
            let scenarios=plan.scenario_ids;
            let checks=self.authoring(&manifest,&scenarios,source,request.envelope["created_at"].as_str().context("refinery_created_at_missing")?)?;
            let receipt=self.blocked_receipt(request,source,json!({"status":"no_delta","rationale":"Actual Rust scenario observations retained; no authorized reusable delta requested.","delta_sha256":null}),json!({"profile":PROFILE,"checks":checks,"full_original_release":"unknown","engineering_adoption":"unknown"}))?;
            let registry=RegistryRequest{workspace:request.workspace.clone(),binding:request.binding.clone()};let store=self.registry(&registry,true,request.envelope["created_at"].as_str().context("refinery_created_at_missing")?)?;let engagement=self.engine(&store,&registry)?.open_data(&request.envelope,&request.binding,&receipt)?;
            Ok(json!({"engagement":engagement,"recorded":true,"ontology_truth_mutated":false}))
        })
    }
    pub fn propose_candidate(&self, request: &ProposeRequest) -> NativeExecution {
        if request.engagement["schema"] == crate::refinery_full::FULL_SCHEMA {
            return self.full_propose(request);
        }
        self.execute("propose_candidate",|source|{
            let delta=wire::delta(&request.delta)?;let hash=delta["delta_sha256"].as_str().context("refinery_delta_digest_missing")?;
            let plan=&request.engagement;
            ensure!(plan["schema"]=="semantica.refinery-observation-request/1","refinery_caller_passed_receipt_is_untrusted");
            let plan=observation_request(plan)?;
            let manifest=self.root.private_path(Path::new(&plan.manifest_path))?;
            let scenarios=plan.scenario_ids;
            let manifest_sha=self.bind_delta_manifest(&delta,&manifest)?;
            let checks=self.authoring(&manifest,&scenarios,source,request.recorded_at.as_deref().unwrap_or(request.candidate_envelope["created_at"].as_str().context("refinery_created_at_missing")?))?;
            ensure!(self.bind_delta_manifest(&delta,&manifest)? == manifest_sha,"refinery_delta_manifest_changed_during_execution");
            let open=OpenEngagementRequest{workspace:request.registry.workspace.clone(),binding:request.registry.binding.clone(),envelope:request.candidate_envelope.clone()};
            let receipt=self.blocked_receipt(&open,source,json!({"status":"candidate","rationale":delta["rationale"],"delta_sha256":hash}),json!({"profile":PROFILE,"actual_authoring_checks":checks,"conditional_delta":delta,"original_release":"unknown","provenance_closure":"unknown","engineering_adoption":"unknown"}))?;
            let store=self.registry(&request.registry,false,request.recorded_at.as_deref().unwrap_or("1970-01-01T00:00:00Z"))?;
            let engine=self.engine(&store,&request.registry)?;let receipt=engine.open_data(&request.candidate_envelope,&request.registry.binding,&receipt)?;
            // Intent is already retained as an exact hashed engagement. It does
            // not become original candidate/proposed state with blocked gates.
            Ok(json!({"delta":delta,"engagement":receipt,"eligibility":"blocked","candidate_recorded":true,"candidate_state":"not_registered","state":null,"blocked_reasons":["actual original subject/release/provenance gates are not complete"],"ontology_truth_mutated":false}))
        })
    }
    pub fn commit_candidate(
        &self,
        request: &TransitionRequest,
        authorization: &Value,
    ) -> NativeExecution {
        self.full_commit(request, authorization)
    }
    pub fn history(&self, request: &RegistryRequest, candidate: &str) -> NativeExecution {
        self.execute("history", |_| {
            let store = self.registry(request, false, "1970-01-01T00:00:00Z")?;
            let events = self.engine(&store, request)?.history(candidate)?;
            Ok(json!({"events":events,"data_control_only":true,"engineering_adoption":"unknown"}))
        })
    }
    pub fn verify_candidate(&self, request: &VerifyRequest) -> NativeExecution {
        self.full_verify(request)
    }
    pub fn promote_candidate(
        &self,
        request: &TransitionRequest,
        authorization: &Value,
    ) -> NativeExecution {
        self.full_promote(request, authorization)
    }
    pub fn decision_review(&self, request: &DecisionReviewRequest) -> NativeExecution {
        self.decision_review_observed(request, None)
    }

    /// The observer runs once under the existing FileStore lock. Its JSON is
    /// retained observation data, never permission to promote or adopt.
    pub(crate) fn decision_review_observed(
        &self,
        request: &DecisionReviewRequest,
        observer: Option<
            &mut dyn FnMut(
                &DecisionReviewRequest,
                &semantica_contracts::SemanticPromotedReviewResult1,
            ) -> Result<Value>,
        >,
    ) -> NativeExecution {
        let mut execution = self.execute("decision_review", |_| {
            let binding = wire::binding(&request.binding)?;
            let time = observation_time(request.created_at.as_deref())?;
            let registry = RegistryRequest {
                workspace: request.workspace.clone(),
                binding: binding.clone(),
            };
            let store = self.registry(&registry, false, &time)?;
            self.review_in_store(request, &registry, &store, &binding, &time, observer)
        });
        Self::validate_decision_review_execution(request, &mut execution);
        execution
    }
    fn review_in_store(
        &self,
        request: &DecisionReviewRequest,
        registry: &RegistryRequest,
        store: &FileStore,
        binding: &Value,
        time: &String,
        observer: Option<
            &mut dyn FnMut(
                &DecisionReviewRequest,
                &semantica_contracts::SemanticPromotedReviewResult1,
            ) -> Result<Value>,
        >,
    ) -> Result<Value> {
        let input = semantica_contracts::SemanticPromotedReviewRequest1 {
            schema: semantica_runtime::promoted_review::REQUEST.into(),
            review_contract: semantica_runtime::promoted_review::REVIEW_CONTRACT.into(),
            registry_id: binding["workspace_id"]
                .as_str()
                .context("review_registry_missing")?
                .into(),
            package_id: binding["package_id"]
                .as_str()
                .context("review_package_missing")?
                .into(),
            expected_current_version: binding["baseline_version"]
                .as_str()
                .context("review_baseline_missing")?
                .into(),
            expected_registry_package_sha256: binding["baseline_package_sha256"]
                .as_str()
                .context("review_baseline_digest_missing")?
                .into(),
            evidence_bytes: request.evidence.clone(),
            evidence_sha256: request.evidence_sha256.clone(),
            evidence_format: request.evidence_format.clone(),
            evidence_id: request.evidence_id.clone(),
            evidence_uri: request.evidence_uri.clone(),
            evidence_captured_at: request.evidence_captured_at.clone(),
            scope_id: request.scope_id.clone(),
            focus_iri: request.focus_iri.clone(),
            focus_type_iri: request.focus_type_iri.clone(),
            query_asset_id: request.query_asset_id.clone(),
            shape_asset_id: request.shape_asset_id.clone(),
            created_at: time.clone(),
        };
        let result = self
            .engine(store, registry)?
            .review_promoted_current(&input)?;
        let mut output = json!({"subject":result.subject,"execution_registry_id":input.registry_id,
                "review":result.review_v1,"review_observation":result,
                "fact_admitted":false,"typed_project_checks":"not_run"});
        if let Some(observe) = observer {
            output["typed_project_observation"] = match observe(request, &result) {
                Ok(observation) => observation,
                Err(error) => json!({"execution":"failed","conclusion":"unknown",
                        "failures":[format!("{error:#}")],"engineering_adoption":"unknown"}),
            };
            // The original owner retains its lock across engineering checks.
            // Re-read current and the complete publication/assets afterward.
            let rechecked = (|| -> Result<()> {
                let engine = self.engine(store, registry)?;
                let current = engine.resolve_package(&input.package_id, None)?;
                ensure!(
                    current == result.subject["descriptor"],
                    "typed_review_current_package_changed"
                );
                engine.verify_published_refinement(
                    current["delta_sha256"]
                        .as_str()
                        .context("typed_review_delta_missing")?,
                )?;
                Ok(())
            })();
            output["typed_project_recheck"] = match rechecked {
                Ok(()) => json!({"status":"matched"}),
                Err(error) => json!({"status":"failed","error":format!("{error:#}")}),
            };
            output["typed_project_checks"] =
                output["typed_project_observation"]["execution"].clone();
        }
        // execute retains this complete output even if source recheck fails.
        Ok(output)
    }

    fn validate_decision_review_execution(
        request: &DecisionReviewRequest,
        execution: &mut NativeExecution,
    ) {
        if execution.semantic_execution == "completed" {
            let valid = (|| -> Result<()> {
                let result = execution.result.as_ref().context("review_result_missing")?;
                ensure!(
                    result["execution_registry_id"] == request.binding["workspace_id"],
                    "native_review_execution_registry_mismatch"
                );
                crate::native::validate_review(&result["review"], request)?;
                if result.get("typed_project_observation").is_some() {
                    ensure!(
                        result["typed_project_observation"]["execution"] != "failed",
                        "typed_project_observer_failed"
                    );
                    ensure!(
                        result["typed_project_recheck"]["status"] == "matched",
                        "typed_project_package_recheck_failed"
                    );
                }
                Ok(())
            })();
            if let Err(error) = valid {
                // Keep the independently returned observations on rejection.
                execution.semantic_execution = "failed".into();
                execution.error = Some(format!("{error:#}"));
            }
        }
    }

    /// Finite commit composition: no serialized receipt can enter this path as
    /// executed evidence. The original store stays alive through execute's
    /// real source recheck, owner preparation, final guards, and one publisher.
    pub(crate) fn decision_review_committed(
        &self,
        request: &DecisionReviewRequest,
        observer: &mut dyn FnMut(
            &DecisionReviewRequest,
            &semantica_contracts::SemanticPromotedReviewResult1,
        ) -> Result<Value>,
        precommit: &mut dyn FnMut(&Value) -> Result<()>,
        publish: impl FnOnce(&Value) -> Result<Value>,
    ) -> NativeExecution {
        let mut retained_store = None;
        let mut execution = self.execute("decision_review", |_| {
            let binding = wire::binding(&request.binding)?;
            let time = observation_time(request.created_at.as_deref())?;
            let registry = RegistryRequest {
                workspace: request.workspace.clone(),
                binding: binding.clone(),
            };
            retained_store = Some(self.registry(&registry, false, &time)?);
            self.review_in_store(
                request,
                &registry,
                retained_store.as_ref().context("commit_store_missing")?,
                &binding,
                &time,
                Some(observer),
            )
        });
        Self::validate_decision_review_execution(request, &mut execution);
        if execution.semantic_execution == "completed" {
            let attempted = (|| -> Result<()> {
                let output = execution.result.as_mut().context("commit_review_missing")?;
                // Caller preparation includes original authority and input guards.
                // It may retain prepared diagnostics, never completed adoption.
                precommit(output)?;
                let binding = wire::binding(&request.binding)?;
                let registry = RegistryRequest {
                    workspace: request.workspace.clone(),
                    binding,
                };
                let engine = self.engine(
                    retained_store.as_ref().context("commit_store_missing")?,
                    &registry,
                )?;
                let current = engine.resolve_package(
                    request.binding["package_id"]
                        .as_str()
                        .context("commit_package_missing")?,
                    None,
                )?;
                ensure!(
                    current == output["review_observation"]["subject"]["descriptor"],
                    "commit_current_package_changed"
                );
                engine.verify_published_refinement(
                    current["delta_sha256"]
                        .as_str()
                        .context("commit_delta_missing")?,
                )?;
                output["typed_project_recheck"] = json!({"status":"matched"});
                let expected = output["rust_execution"].clone();
                let (after, observation) = self.recheck_source(&expected);
                output["source_recheck"] = observation;
                ensure!(
                    after? == expected,
                    "refinery_current_source_changed_before_commit"
                );
                // This field records an attempt, not a rollback guarantee.
                output["commit_result"] = json!({"execution":"not_completed","finalizer_invoked":true,"publication":"not_observed_by_semantic_owner"});
                output["commit_result"] = publish(output)?;
                Ok(())
            })();
            if let Err(error) = attempted {
                execution.semantic_execution = "failed".into();
                execution.error = Some(format!("{error:#}"));
            }
        }
        // retained_store drops only here, after the publisher or rejected guard.
        execution
    }
    pub fn registry_workspace_diagnostics(&self, request: &RegistryRequest) -> NativeExecution {
        self.execute("registry_diagnostics",|_|{let store=self.registry(request,false,"1970-01-01T00:00:00Z")?;let e=self.engine(&store,request)?;let registry=e.document("registry.json")?;
        let mut packages=Vec::new();for id in registry["packages"].as_object().context("registry packages")?.keys(){let descriptor=e.resolve_package(id,None)?;e.verify_published_refinement(descriptor["delta_sha256"].as_str().context("descriptor delta")?)?;packages.push(descriptor);}
        Ok(json!({"registry":registry,"packages":packages,"promotion":"not_performed","engineering_adoption":"unknown"}))})
    }
    pub fn discover(&self, request: &RegistryRequest) -> NativeExecution {
        self.execute("discover",|_|{let binding=wire::binding(&request.binding)?;let store=self.registry(request,false,"1970-01-01T00:00:00Z")?;let e=self.engine(&store,request)?;let registry=e.document("registry.json")?;let id=binding["package_id"].as_str().unwrap();
        if registry["packages"].get(id).is_none(){ensure!(binding["baseline_version"]=="0"&&binding["baseline_package_sha256"]==wire::EMPTY_PACKAGE_SHA256,"refinery_absent_bound_baseline");return Ok(json!({"registry_id":binding["workspace_id"],"subject":{"package_id":binding["package_id"],"version":"0","package_digest":wire::EMPTY_PACKAGE_SHA256,"status":"empty_unpromoted"},"registry":registry,"descriptor":null,"manifest":null,"promoted_subject":"not_present"}));}
        let descriptor=e.resolve_package(id,if binding["baseline_version"]=="0"{None}else{binding["baseline_version"].as_str()})?;
        if binding["baseline_version"]!="0"{ensure!(descriptor["package_sha256"]==binding["baseline_package_sha256"],"refinery_discover_exact_baseline_digest");}
        e.verify_published_refinement(descriptor["delta_sha256"].as_str().context("descriptor delta")?)?;
        let manifest=e.committed_manifest(descriptor["delta_sha256"].as_str().unwrap())?;
        Ok(json!({"registry_id":binding["workspace_id"],"subject":{"package_id":descriptor["package_id"],"version":descriptor["version"],"package_digest":descriptor["package_sha256"],"status":"published_content_verified"},"registry":registry,"descriptor":descriptor,"manifest":manifest,"promoted_subject":"present","engineering_adoption":"unknown"}))})
    }
    pub fn registry_diagnostics(&self) -> NativeExecution {
        self.execute("registry_diagnostics", |_| {
            let provider = crate::books::rust::ArchiveProvider::open(self.root.path())?;
            let catalog = provider.catalog()?;
            let chapter_issues = catalog.validate_chapter_registry(&provider, false)?;
            let mut domain_issues = Vec::new();
            let domains = catalog.list_domain_packages().into_iter().map(|descriptor| {
                let id = descriptor["package_id"].as_str().context("refinery_catalog_package_id_missing")?;
                domain_issues.extend(catalog.validate_domain_package(&provider, id)?);
                Ok(descriptor)
            }).collect::<Result<Vec<_>>>()?;
            provider.verify()?;
            Ok(json!({"schema":"semantica.refinery-rust-diagnostics/1","chapters":catalog.list_chapter_packages(None),"chapter_issues":chapter_issues,"domains":domains,"domain_issues":domain_issues,"full_original_registry_execution":"unknown"}))
        })
    }
    pub fn discover_packages(&self) -> NativeExecution {
        self.execute("discover_packages",|_|{
        let provider=crate::books::rust::ArchiveProvider::open(self.root.path())?;let catalog=provider.catalog()?;
        let mut chapters=Vec::new();for descriptor in catalog.list_chapter_packages(None){let id=descriptor["package_id"].as_str().context("refinery_catalog_package_id_missing")?;let manifest=catalog.read_manifest(id)?;let digest=semantica_runtime::books::package_digest(manifest)?;chapters.push(json!({"descriptor":descriptor,"package_digest":digest,"required_capabilities":[],"available_capabilities":[]}));}
        let mut domains=Vec::new();for descriptor in catalog.list_domain_packages(){let id=descriptor["package_id"].as_str().context("refinery_catalog_package_id_missing")?;let digest=semantica_runtime::books::package_digest(catalog.read_manifest(id)?)?;domains.push(json!({"descriptor":descriptor,"package_digest":digest,"required_capabilities":[],"available_capabilities":[]}));}provider.verify()?;
        Ok(json!({"chapters":chapters,"domains":domains,"profile":"source-bound catalog discovery; capability completeness unknown"}))
    })
    }
    pub fn book_api(&self, input: &Value) -> NativeExecution {
        self.execute(input["operation"].as_str().unwrap_or("invalid_book_api"),|_|match input["operation"].as_str(){
        Some("list_chapter_packages")=>Ok(json!({"packages":crate::books::rust::list_chapter_packages(self.root.path(),None)?})),
        Some("validate_chapter_registry")=>crate::books::rust::validate_chapter_registry(self.root.path(),false),
        _=>bail!("refinery_book_operation_not_supported"),
    })
    }
    fn authoring(
        &self,
        manifest: &Path,
        scenarios: &[String],
        source: &Value,
        recorded_at: &str,
    ) -> Result<Vec<Value>> {
        wire::timestamp(&json!(recorded_at))?;
        ensure!(
            !scenarios.is_empty() && scenarios.len() <= 64,
            "refinery_authoring_batch_limit"
        );
        let manifest = self.root.private_path(manifest)?;
        let bytes = read_regular_bytes(&manifest, LIMIT)?;
        let value = crate::strict_json(&bytes)?;
        let parent = manifest
            .parent()
            .context("refinery_manifest_parent_missing")?;
        let files = crate::books::rust::FileProvider::new(BTreeMap::from([(
            "input".into(),
            parent.to_path_buf(),
        )]))?;
        let provider = ManifestProvider(&files);
        let identity = json!({"schema":"semantica.refinery-runner-runtime/1","source":source,"runtime_version":env!("CARGO_PKG_VERSION"),"runtime_commit":format!("semantica.rust-source/{}",source["source_sha256"].as_str().unwrap_or("")),"runtime_artifact_sha256":source["executable_sha256"],"created_at":recorded_at,"compilation_origin":"unknown","engineering_adoption":"unknown"});
        let mut checks = Vec::new();
        for scenario in scenarios {
            checks.push(semantica_runtime::refinery_runner::run_manifest(
                &provider,
                &value,
                Some(scenario),
                &identity,
            )?);
        }
        files.verify()?;
        ensure!(
            read_regular_bytes(&manifest, LIMIT)? == bytes,
            "refinery_manifest_changed_during_execution"
        );
        Ok(checks)
    }
    pub fn run_manifest_batch(
        &self,
        manifest: &Path,
        scenarios: &[String],
        recorded_at: Option<&str>,
    ) -> NativeExecution {
        self.execute("run_manifest_batch", |source| {
            let time = observation_time(recorded_at)?;
            Ok(json!({"checks":self.authoring(manifest,scenarios,source,&time)?,"created_at":time,"time_basis":if recorded_at.is_some(){"caller_supplied"}else{"host_utc"}}))
        })
    }
    pub fn run_package(
        &self,
        package: &str,
        scenario: Option<&str>,
        recorded_at: Option<&str>,
    ) -> NativeExecution {
        self.execute("run_package",|source|{
        let provider=crate::books::rust::ArchiveProvider::open(self.root.path())?;let catalog=provider.catalog()?;let mut state=semantica_runtime::book_runner::BookRunnerState::default();let mut session=semantica_runtime::dataset::DatasetSession::new();
        let time = observation_time(recorded_at)?;
        let identity=json!({"created_at":time,"runtime_commit":format!("semantica.rust-source/{}",source["source_sha256"].as_str().unwrap_or("")),"runtime_artifact_sha256":source["executable_sha256"],"runtime_version":env!("CARGO_PKG_VERSION"),"source":source});
        let execution=semantica_runtime::book_runner::run(&mut state,&catalog,&provider,&mut session,package,scenario,&identity)?;let verification=semantica_runtime::book_runner::verify(&state,&mut session,&execution,&identity)?;provider.verify()?;Ok(json!({"execution":execution,"verification":verification,"profile":"semantica.refinery-rust-subject-observation/1","engineering_adoption":"unknown"}))
    })
    }
    pub fn run_registry(&self, request: &RegistryRunRequest) -> NativeExecution {
        self.full_registry_run(request)
    }
}
pub(crate) fn observation_time(recorded_at: Option<&str>) -> Result<String> {
    let time = recorded_at.map(str::to_owned).unwrap_or_else(|| {
        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339()
    });
    wire::timestamp(&json!(time))?;
    Ok(time)
}
pub(crate) fn observation_request(
    input: &Value,
) -> Result<semantica_contracts::SemanticRefineryObservationRequest> {
    let request: semantica_contracts::SemanticRefineryObservationRequest =
        serde_json::from_slice(&crate::canonical_json(input)?)?;
    request
        .validate()
        .map_err(|error| anyhow::anyhow!("refinery_observation_contract:{error:?}"))?;
    ensure!(
        !request.scenario_ids.is_empty()
            && request.scenario_ids.len() <= 64
            && request
                .scenario_ids
                .iter()
                .all(|id| !id.is_empty() && id.trim() == id),
        "refinery_scenario_batch_invalid"
    );
    ensure!(
        !request.manifest_path.is_empty(),
        "refinery_manifest_path_missing"
    );
    Ok(request)
}
struct ManifestProvider<'a>(&'a crate::books::rust::FileProvider);
impl SourceProvider for ManifestProvider<'_> {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        semantica_runtime::books::relative(id)?;
        self.0.read(&format!("input/{id}"))
    }
    fn metadata(&self, bytes: &[u8]) -> Result<Value> {
        crate::strict_json(bytes)
    }
}
