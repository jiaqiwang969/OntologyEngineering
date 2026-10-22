//! Explicit current full observation composition. A request locks reference and
//! target bytes; actual retained runners, not caller statuses, create authority.
use crate::{
    native::{
        NativeExecution, OpenEngagementRequest, ProposeRequest, RegistryRequest,
        RegistryRunRequest, TransitionRequest, VerifyRequest,
    },
    refinery_runtime::RustRuntime,
};
use anyhow::{Context, Result, ensure};
use oe_core::read_regular_bytes;
use semantica_runtime::{
    books::SourceProvider,
    refinery::{RefineryEngine, RefineryStore},
    refinery_contracts as wire, refinery_subject as subject,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub const FULL_SCHEMA: &str = "semantica.refinery-full-observation-request/1";
const LIMIT: usize = 16 * 1024 * 1024;
fn closed(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let o = value.as_object().context("full plan mapping required")?;
    ensure!(
        required.iter().all(|k| o.contains_key(*k))
            && o.keys()
                .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str())),
        "refinery_full_closed_schema"
    );
    Ok(())
}
fn clean_text(v: &Value) -> Result<&str> {
    let s = v.as_str().context("full string required")?;
    ensure!(
        !s.is_empty() && semantica_contracts::json::python_strip(s) == s,
        "full normalized string required"
    );
    Ok(s)
}

pub(crate) fn full_request(
    input: &Value,
) -> Result<semantica_contracts::SemanticRefineryFullObservationPlan> {
    closed(
        input,
        &[
            "schema",
            "target_manifest_path",
            "target_manifest_sha256",
            "reference_manifest_path",
            "reference_manifest_sha256",
            "reference_scenario_ids",
            "required_regression_check_ids",
            "required_release_check_ids",
            "runtime_source",
        ],
        &[],
    )?;
    let plan: semantica_contracts::SemanticRefineryFullObservationPlan =
        serde_json::from_slice(&crate::canonical_json(input)?)?;
    plan.validate()
        .map_err(|e| anyhow::anyhow!("refinery_full_contract:{e:?}"))?;
    ensure!(plan.schema == FULL_SCHEMA, "refinery_full_schema");
    for sha in [
        &plan.target_manifest_sha256,
        &plan.reference_manifest_sha256,
    ] {
        ensure!(wire::valid_sha(sha), "refinery_full_manifest_digest");
    }
    ensure!(
        plan.reference_scenario_ids.len() <= 64
            && !plan.reference_scenario_ids.is_empty()
            && plan
                .reference_scenario_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == plan.reference_scenario_ids.len(),
        "refinery_full_reference_scenarios"
    );
    for id in &plan.reference_scenario_ids {
        wire::opaque_identifier(&json!(id))?;
    }
    ensure!(
        plan.required_regression_check_ids
            == semantica_runtime::refinery_gate_algorithms::REGRESSION_REQUIRED_CHECK_IDS
                .map(str::to_owned),
        "refinery_full_regression_order"
    );
    ensure!(
        plan.required_release_check_ids
            == semantica_runtime::refinery_gate_algorithms::RELEASE_REQUIRED_CHECK_IDS
                .map(str::to_owned),
        "refinery_full_release_order"
    );
    plan.runtime_source
        .validate()
        .map_err(|e| anyhow::anyhow!("refinery_full_source_contract:{e:?}"))?;
    Ok(plan)
}
struct Files<'a>(&'a crate::books::rust::FileProvider);
impl SourceProvider for Files<'_> {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        semantica_runtime::books::relative(id)?;
        self.0.read(&format!("input/{id}"))
    }
    fn metadata(&self, b: &[u8]) -> Result<Value> {
        crate::strict_json(b)
    }
}
fn provider(path: &Path) -> Result<crate::books::rust::FileProvider> {
    crate::books::rust::FileProvider::new(BTreeMap::from([(
        "input".into(),
        path.parent().context("full manifest parent")?.to_path_buf(),
    )]))
}
fn manifest(runtime: &RustRuntime, path: &str, digest: &str) -> Result<(PathBuf, Vec<u8>, Value)> {
    let path = runtime.root.private_path(Path::new(path))?;
    let bytes = read_regular_bytes(&path, LIMIT)?;
    ensure!(
        semantica_runtime::sha256(&bytes) == digest,
        "refinery_full_manifest_changed"
    );
    let value = crate::strict_json(&bytes)?;
    Ok((path, bytes, value))
}
fn recheck(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(
        read_regular_bytes(path, LIMIT)? == bytes,
        "refinery_full_manifest_changed_during_execution"
    );
    Ok(())
}
fn binding(e: &RefineryEngine<'_>, delta: &str, request: &RegistryRequest) -> Result<()> {
    ensure!(
        e.document(&format!("refinements/{delta}/binding.json"))?
            == wire::binding(&request.binding)?,
        "refinery_full_retained_binding_changed"
    );
    Ok(())
}
fn context(request: &VerifyRequest, action: &str) -> Result<Value> {
    let task = request
        .envelopes
        .get(action)
        .context(format!("refinery_full_envelope_missing:{action}"))?;
    wire::transition_context(
        action,
        &request.delta_sha256,
        task,
        &request.registry.binding,
    )
}
fn recorded_suite(e: &RefineryEngine<'_>, delta: &str) -> Result<String> {
    let events = e.history(delta)?;
    events
        .iter()
        .find(|ev| ev["state"] == "regression_passed")
        .and_then(|ev| ev["payload"]["execution_suite_sha256"].as_str())
        .map(str::to_owned)
        .context("refinery_full_recorded_suite_missing")
}
fn target(
    e: &RefineryEngine<'_>,
    store: &dyn RefineryStore,
    delta: &Value,
    binding: &Value,
    value: &Value,
    files: &dyn SourceProvider,
) -> Result<Value> {
    let logical = e.preview_delta_manifest(delta, binding)?;
    closed(
        value,
        &[
            "schema_version",
            "package_id",
            "version",
            "namespace",
            "release_status",
            "execution",
            "assets",
        ],
        &[],
    )?;
    ensure!(
        value["schema_version"] == "1.0"
            && value["package_id"] == logical["package_id"]
            && value["version"] == logical["version"],
        "refinery_full_target_identity"
    );
    let records = logical["assets"]
        .as_array()
        .context("logical target assets")?;
    let assets = value["assets"]
        .as_array()
        .context("runtime target assets")?;
    ensure!(
        records.len() == assets.len(),
        "refinery_full_target_asset_coverage"
    );
    let mut seen = BTreeSet::new();
    let mut actual = BTreeMap::new();
    for a in assets {
        let id = clean_text(&a["asset_id"])?;
        ensure!(seen.insert(id), "refinery_full_target_duplicate_asset");
        let record = records
            .iter()
            .find(|r| r["asset_id"] == id)
            .context("target asset outside exact delta")?;
        let bytes = semantica_runtime::packages::read_manifest_asset(files, a)?.content;
        ensure!(
            semantica_runtime::sha256(&bytes) == record["sha256"]
                && a["sha256"] == record["sha256"],
            "refinery_full_target_actual_asset_digest"
        );
        let changed = semantica_runtime::refinery::CATEGORIES
            .iter()
            .flat_map(|c| delta[*c].as_array().into_iter().flatten())
            .find(|c| c["asset_id"] == id && c["operation"] != "remove");
        let expected = if let Some(c) = changed {
            wire::asset_content(c)?.context("full target incoming bytes missing")?
        } else {
            let sha = record["sha256"].as_str().unwrap();
            store
                .read(&format!("objects/sha256/{}/{sha}", &sha[..2]))?
                .context("full immutable base bytes missing")?
        };
        ensure!(bytes == expected, "refinery_full_target_exact_content");
        actual.insert(id.to_owned(), bytes);
    }
    let contracts = records
        .iter()
        .filter(|r| r["category"] == "contract")
        .collect::<Vec<_>>();
    ensure!(contracts.len() == 1, "refinery_full_target_contract_count");
    let projection = crate::strict_json(&actual[contracts[0]["asset_id"].as_str().unwrap()])?;
    subject::validate_projection(&projection, records)?;
    for key in ["namespace", "release_status", "execution"] {
        ensure!(
            value[key] == projection[key],
            "refinery_full_target_projection_field:{key}"
        );
    }
    for a in assets {
        let id = a["asset_id"].as_str().unwrap();
        let settings = &projection["assets"][id];
        for field in ["role", "format", "kind", "load_into_dataset", "graph_name"] {
            ensure!(
                a[field] == settings[field],
                "refinery_full_target_asset_setting:{id}:{field}"
            );
        }
    }
    Ok(logical)
}
impl RustRuntime {
    pub(crate) fn full_open(
        &self,
        request: &OpenEngagementRequest,
        input: &Value,
    ) -> NativeExecution {
        self.execute("open_engagement",|source|{
  let plan=full_request(input)?;ensure!(serde_json::to_value(&plan.runtime_source)?==*source,"refinery_full_current_source_mismatch");
  let (refpath,refbytes,reference)=manifest(self,&plan.reference_manifest_path,&plan.reference_manifest_sha256)?;let (targetpath,targetbytes,_)=manifest(self,&plan.target_manifest_path,&plan.target_manifest_sha256)?;
  let files=provider(&refpath)?;let registry=RegistryRequest{workspace:request.workspace.clone(),binding:request.binding.clone()};let when=request.envelope["created_at"].as_str().context("full observation created_at")?;
  let create=!self.root.private_path(&registry.workspace)?.try_exists()?;let store=self.registry(&registry,create,when)?;let e=self.engine(&store,&registry)?;
  let proof=e.engage_current(&Files(&files),&reference,&plan.reference_scenario_ids,&request.envelope,&request.binding,&json!({"status":"no_delta","rationale":"Actual full reference observations retained; no reusable delta requested.","delta_sha256":null}),source,when)?;
  files.verify()?;recheck(&refpath,&refbytes)?;recheck(&targetpath,&targetbytes)?;let engagement=e.open_current(&proof)?;
  Ok(json!({"engagement":engagement,"recorded":true,"ontology_truth_mutated":false,"full_observation_profile":FULL_SCHEMA}))
 })
    }
    pub(crate) fn full_propose(&self, request: &ProposeRequest) -> NativeExecution {
        self.execute("propose_candidate",|source|{
  let plan=full_request(&request.engagement)?;ensure!(serde_json::to_value(&plan.runtime_source)?==*source,"refinery_full_current_source_mismatch");let delta=wire::delta(&request.delta)?;
  let when=request.recorded_at.as_deref().or(request.candidate_envelope["created_at"].as_str()).context("full candidate timestamp")?;
  let (refpath,refbytes,reference)=manifest(self,&plan.reference_manifest_path,&plan.reference_manifest_sha256)?;let (targetpath,targetbytes,targetvalue)=manifest(self,&plan.target_manifest_path,&plan.target_manifest_sha256)?;
  let reffiles=provider(&refpath)?;let targetfiles=provider(&targetpath)?;let store=self.registry(&request.registry,false,when)?;let e=self.engine(&store,&request.registry)?;
  let logical=target(&e,&store,&delta,&request.registry.binding,&targetvalue,&Files(&targetfiles))?;
  let proof=e.engage_current(&Files(&reffiles),&reference,&plan.reference_scenario_ids,&request.candidate_envelope,&request.registry.binding,&json!({"status":"candidate","rationale":delta["rationale"],"delta_sha256":delta["delta_sha256"]}),source,when)?;
  reffiles.verify()?;targetfiles.verify()?;recheck(&refpath,&refbytes)?;recheck(&targetpath,&targetbytes)?;
  if proof.observation()["status"]!="complete"{let engagement=e.open_current(&proof)?;return Ok(json!({"delta":delta,"engagement":engagement,"eligibility":"blocked","candidate_recorded":false,"candidate_state":"not_registered","state":null,"ontology_truth_mutated":false}));}
  let result=e.propose_current(&proof,&delta,&request.proposed_envelope,when)?;
  e.retain_current_observation_plan(&proof,delta["delta_sha256"].as_str().unwrap(),&request.engagement,&logical)?;
  Ok(result)
 })
    }
    pub(crate) fn full_commit(
        &self,
        request: &TransitionRequest,
        authorization: &Value,
    ) -> NativeExecution {
        self.execute("commit_candidate", |source| {
            let when = super::refinery_runtime::observation_time(request.recorded_at.as_deref())?;
            let store = self.registry(&request.registry, false, &when)?;
            let e = self.engine(&store, &request.registry)?;
            binding(&e, &request.delta_sha256, &request.registry)?;
            let events = e.history(&request.delta_sha256)?;
            let saved = e.document(&format!(
                "refinements/{}/engagement.json",
                request.delta_sha256
            ))?;
            ensure!(
                saved["profile"] == crate::refinery_runtime::PROFILE,
                "refinery_candidate_execution_origin_untrusted"
            );
            ensure!(
                events.first().context("refinery_empty_history")?["payload"]["engagement_status"]
                    == "complete",
                "refinery_candidate_execution_incomplete"
            );
            let proof = e.replay_current_engagement(&request.delta_sha256, source)?;
            e.commit_current(
                &proof,
                &request.delta_sha256,
                &request.envelope,
                authorization,
                &when,
            )
        })
    }
    pub(crate) fn full_verify(&self, request: &VerifyRequest) -> NativeExecution {
        self.execute("verify_candidate",|source|{
        let actions=["execute_candidate","derive_regression_gate","regression_passed","derive_release_gate","release_complete"];
        ensure!(request.envelopes.len()==actions.len()&&actions.iter().all(|a|request.envelopes.contains_key(*a)),"native_verification_actions_mismatch");
        let when=super::refinery_runtime::observation_time(request.recorded_at.as_deref())?;let store=self.registry(&request.registry,false,&when)?;let e=self.engine(&store,&request.registry)?;binding(&e,&request.delta_sha256,&request.registry)?;
        let state=e.status(&request.delta_sha256)?;let active=if state["state"]=="committed"{&actions[..]}else{&actions[3..]};
        let mut contexts=json!({});for action in active {contexts[*action]=context(request,action)?;}
        let deriverelease=&contexts["derive_release_gate"];let completerelease=&contexts["release_complete"];
        let (verification,suite)=if state["state"]=="committed"{
            let proof=e.execute_current(&request.delta_sha256,&contexts["execute_candidate"],source,&when)?;
            let regression=e.derive_gate(&request.delta_sha256,"regression",proof.observation()["suite_sha256"].as_str().unwrap(),&contexts["derive_regression_gate"],&when)?;
            (e.verify_candidate_current(&proof,Some(&regression),Some(&contexts["regression_passed"]),deriverelease,completerelease,&when)?,proof.observation().clone())
        }else{let suite=recorded_suite(&e,&request.delta_sha256)?;let proof=e.replay_current_execution(&request.delta_sha256,&suite,source)?;(e.verify_candidate_current(&proof,None,None,deriverelease,completerelease,&when)?,Value::Null)};
        Ok(json!({"registry_id":request.registry.binding["workspace_id"],"state":verification["state"],"execution_suite":suite,"execution_suite_sha256":verification["execution_suite_sha256"],"regression_evidence":verification["regression_evidence"],"release_evidence":verification["release_evidence"],"native_verification":verification,"transition_contexts":contexts}))
    })
    }
    pub(crate) fn full_promote(
        &self,
        request: &TransitionRequest,
        authorization: &Value,
    ) -> NativeExecution {
        self.execute("promote_candidate", |source| {
            let when = super::refinery_runtime::observation_time(request.recorded_at.as_deref())?;
            let store = self.registry(&request.registry, false, &when)?;
            let e = self.engine(&store, &request.registry)?;
            binding(&e, &request.delta_sha256, &request.registry)?;
            let suite = recorded_suite(&e, &request.delta_sha256)?;
            let proof = e.replay_current_execution(&request.delta_sha256, &suite, source)?;
            let context = wire::transition_context(
                "promoted",
                &request.delta_sha256,
                &request.envelope,
                &request.registry.binding,
            )?;
            let descriptor = e.promote_current(&proof, authorization, &context, &when)?;
            Ok(json!({"descriptor":descriptor,"transition_context":context}))
        })
    }
    pub(crate) fn full_registry_run(&self, request: &RegistryRunRequest) -> NativeExecution {
        self.execute("run_registry", |source| {
            let when = super::refinery_runtime::observation_time(request.created_at.as_deref())?;
            let store = self.registry(&request.registry, false, &when)?;
            let e = self.engine(&store, &request.registry)?;
            let b = wire::binding(&request.registry.binding)?;
            let package = b["package_id"].as_str().unwrap();
            let version = b["baseline_version"].as_str().unwrap();
            let descriptor = e.resolve_package(package, Some(version))?;
            ensure!(
                descriptor["package_sha256"] == b["baseline_package_sha256"],
                "refinery_registry_run_exact_bound_version"
            );
            let scenario=request.scenario_id.as_deref();
            let mut value=e.run_registry_current_optional(package,Some(version),scenario,source,&when)?;
            let committed=e.committed_manifest(descriptor["delta_sha256"].as_str().context("published delta")?)?;
            let (_,projection)=e.execution_projection(&committed)?;
            value["registry_id"]=b["workspace_id"].clone();
            value["subject"]=json!({"package_id":descriptor["package_id"],"version":descriptor["version"],"package_sha256":descriptor["package_sha256"],"manifest_sha256":descriptor["manifest_sha256"],"execution_projection_sha256":projection["_projection_sha256"]});
            Ok(value)
        })
    }
}
