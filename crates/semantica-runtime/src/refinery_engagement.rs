//! Actual reference execution for engagement bootstrap. Target subject gates
//! are executed separately after commit; reference results do not waive them.
use crate::{
    refinery::{RefineryEngine, RefineryStore, hash, text, values},
    refinery_contracts as wire, refinery_subject,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub struct CurrentEngagement<'a> {
    owner: &'a dyn RefineryStore,
    task: Value,
    binding: Value,
    receipt: Value,
    snapshots: Vec<Value>,
    reference_manifest: Value,
    scenarios: Vec<String>,
}
impl CurrentEngagement<'_> {
    pub fn observation(&self) -> &Value {
        &self.receipt
    }
}
impl RefineryEngine<'_> {
    /// Execute reference cases with actual retained BookRunner contexts. No
    /// phase status or receipt from public JSON is accepted as execution proof.
    pub fn engage_current(
        &self,
        provider: &dyn crate::books::SourceProvider,
        reference_manifest: &Value,
        scenario_ids: &[String],
        task: &Value,
        binding: &Value,
        learning: &Value,
        source: &Value,
        when: &str,
    ) -> Result<CurrentEngagement<'_>> {
        wire::timestamp(&json!(when))?;
        let task = wire::task(task)?;
        let binding = wire::binding(binding)?;
        ensure!(
            binding["workspace_id"] == self.registry_id,
            "refinery_engagement_workspace_identity"
        );
        ensure!(
            refinery_subject::runtime_source(source)? && source.get("schema").is_some(),
            "refinery_engagement_current_source_required"
        );
        ensure!(
            !scenario_ids.is_empty()
                && scenario_ids.iter().collect::<BTreeSet<_>>().len() == scenario_ids.len(),
            "refinery_reference_scenarios_required_unique"
        );
        let identity = json!({"created_at":when,"runtime_version":env!("CARGO_PKG_VERSION"),"runtime_commit":format!("semantica.rust-source/{}",text(source,"source_sha256")?),"runtime_artifact_sha256":source["executable_sha256"]});
        let mut retained_manifest = reference_manifest.clone();
        let assets = reference_manifest["assets"]
            .as_array()
            .context("refinery_reference_manifest_assets")?;
        let mut retained_assets = Vec::new();
        for asset in assets {
            let bytes = crate::packages::read_asset(
                provider,
                Some(""),
                asset,
                &std::collections::BTreeMap::new(),
                crate::packages::AssetPolicy::RequireExpectedSha,
            )?;
            let digest = self.put(&bytes.content)?;
            let mut retained = asset.clone();
            retained
                .as_object_mut()
                .context("refinery_reference_asset_mapping")?
                .remove("data");
            retained["path"] = json!(format!("objects/sha256/{}/{digest}", &digest[..2]));
            retained["sha256"] = json!(digest);
            retained_assets.push(retained);
        }
        retained_manifest["assets"] = json!(retained_assets);
        let retained_provider = refinery_subject::CasProvider { engine: self };
        let mut snapshots = Vec::new();
        for scenario in scenario_ids {
            wire::opaque_identifier(&json!(scenario))?;
            snapshots.push(crate::refinery_runner::run_current(
                &retained_provider,
                &retained_manifest,
                scenario,
                &identity,
                source,
            )?);
        }
        let required = task["required_capabilities"].clone();
        let mut observed: Option<BTreeSet<String>> = None;
        let mut references = Vec::new();
        let mut receipts = Vec::new();
        let mut regressions = Vec::new();
        let mut releases = Vec::new();
        let mut execution_passed = true;
        let mut regression_passed = true;
        let mut receipt_passed = true;
        let mut release_passed = true;
        for snapshot in &snapshots {
            let actual = &snapshot["actual_run"];
            let content = &snapshot["release_content"];
            let receipt = &content["receipt"];
            let verdict = &content["content_verdict"];
            let caps = receipt["capability_report"]["payload"]["supported_profiles"]
                .as_object()
                .context("refinery_reference_capabilities")?
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>();
            let excluded = receipt["capability_report"]["payload"]["unknown"]
                .as_array()
                .context("refinery_reference_unknown")?
                .iter()
                .chain(
                    receipt["capability_report"]["payload"]["missing"]
                        .as_array()
                        .context("refinery_reference_missing")?,
                )
                .filter_map(|v| v.as_str())
                .collect::<BTreeSet<_>>();
            let caps = caps
                .into_iter()
                .filter(|v| !excluded.contains(v.as_str()))
                .collect::<BTreeSet<_>>();
            observed = Some(match observed {
                None => caps,
                Some(previous) => previous.intersection(&caps).cloned().collect(),
            });
            execution_passed &= actual["status"] == "passed";
            regression_passed &= actual["oracle_checks"]
                .as_array()
                .is_some_and(|v| !v.is_empty() && v.iter().all(|c| c["status"] == "passed"));
            receipt_passed &= crate::execution_provenance::verify_receipt(receipt)?
                && crate::execution_provenance::verify_receipt_bindings(receipt)?;
            release_passed &= verdict["status"] == "complete";
            references.push(json!({"receipt_sha256":receipt["receipt_sha256"],"package_id":receipt["package_id"],"package_version":receipt["package_version"],"package_digest":receipt["package_digest"]}));
            receipts.push(receipt.clone());
            regressions.push(actual["oracle_checks"].clone());
            releases.push(verdict.clone());
        }
        let observed = observed.unwrap_or_default();
        let mut value = json!({"schema_version":"1.0","contract":wire::CONTRACT,"profile":crate::refinery::RUST_PROFILE,"engagement_id":format!("current-reference-{}",text(&task,"envelope_sha256")?),"envelope_sha256":task["envelope_sha256"],"binding_sha256":binding["binding_sha256"],"required_capabilities":required,"runtime_source":source,"learning":learning,"execution_receipts":references,"created_at":when});
        for (name, passed, evidence) in [
            ("execution", execution_passed, json!(snapshots)),
            ("regression", regression_passed, json!(regressions)),
            ("receipt", receipt_passed, json!(receipts)),
            ("release", release_passed, json!(releases)),
        ] {
            value[name] = wire::phase(
                &json!({"name":name,"status":if passed{"passed"}else{"failed"},"required_capabilities":required,"observed_capabilities":observed,"evidence_sha256":hash(&evidence)?,"details":{"profile":"semantica.refinery-actual-reference-rust/1","reference_manifest_sha256":hash(&retained_manifest)?,"scenario_ids":scenario_ids,"subject_gate_qualification":"not_assumed","engineering_adoption":"unknown"}}),
            )?;
        }
        let receipt = wire::prepare_rust_engagement(&value)?;
        self.engagement_binding(&task, &binding, &receipt)?;
        Ok(CurrentEngagement {
            owner: self.store,
            task,
            binding,
            receipt,
            snapshots,
            reference_manifest: retained_manifest,
            scenarios: scenario_ids.to_vec(),
        })
    }
    fn current_engagement_binding(&self, proof: &CurrentEngagement<'_>) -> Result<()> {
        ensure!(
            std::ptr::eq(proof.owner, self.store),
            "refinery_reference_foreign_store_owner"
        );
        self.engagement_binding(&proof.task, &proof.binding, &proof.receipt)?;
        for snapshot in &proof.snapshots {
            ensure!(
                snapshot["release_content"]["runtime_source"] == proof.receipt["runtime_source"],
                "refinery_reference_source_changed"
            );
        }
        Ok(())
    }
    pub fn open_current(&self, proof: &CurrentEngagement<'_>) -> Result<Value> {
        self.current_engagement_binding(proof)?;
        self.open_data(&proof.task, &proof.binding, &proof.receipt)
    }
    pub fn propose_current(
        &self,
        proof: &CurrentEngagement<'_>,
        delta: &Value,
        proposed_task: &Value,
        when: &str,
    ) -> Result<Value> {
        self.current_engagement_binding(proof)?;
        let delta = wire::delta(delta)?;
        ensure!(
            proof.receipt["learning"]["delta_sha256"] == delta["delta_sha256"]
                && proof.receipt["learning"]["status"] == "candidate",
            "refinery_reference_candidate_identity"
        );
        let result = self.propose_impl(
            &delta,
            &proof.task,
            proposed_task,
            &proof.binding,
            &proof.receipt,
            when,
        )?;
        let objects = proof
            .snapshots
            .iter()
            .map(|v| self.retain(v, true))
            .collect::<Result<Vec<_>>>()?;
        let record = json!({"schema":"semantica.refinery-retained-reference-rust/1","delta_sha256":delta["delta_sha256"],"engagement_receipt_sha256":proof.receipt["receipt_sha256"],"reference_manifest":proof.reference_manifest,"reference_manifest_sha256":hash(&proof.reference_manifest)?,"scenario_ids":proof.scenarios,"snapshot_object_sha256":objects,"runtime_source":proof.receipt["runtime_source"],"created_at":proof.receipt["created_at"]});
        let path = format!(
            "refinements/{}/reference-execution.json",
            text(&delta, "delta_sha256")?
        );
        let bytes = crate::refinery::encoded(&record)?;
        match self.store.read(&path)? {
            Some(existing) => ensure!(
                existing == bytes,
                "refinery_reference_execution_record_collision"
            ),
            None => self.store.write_new(&path, &bytes)?,
        }
        Ok(result)
    }
    /// Fresh actual execution reconstitutes the capability. Retained public
    /// JSON describes expected content, never restores the private proof.
    /// Retain the explicit host input plan on this same refinery owner after
    /// proposal. A failed write leaves visible proposed history; an exact retry
    /// must execute the reference again before it can finish this publication.
    pub fn retain_current_observation_plan(
        &self,
        proof: &CurrentEngagement<'_>,
        delta: &str,
        request: &Value,
        logical_target: &Value,
    ) -> Result<()> {
        self.current_engagement_binding(proof)?;
        let (_, docs, _) = self.load(delta)?;
        ensure!(
            docs["engagement"] == proof.receipt,
            "refinery_plan_retained_engagement_changed"
        );
        let typed: semantica_contracts::SemanticRefineryFullObservationPlan =
            serde_json::from_value(request.clone())?;
        typed
            .validate()
            .map_err(|e| anyhow::anyhow!("refinery_retained_plan_contract:{e:?}"))?;
        ensure!(
            request["schema"] == "semantica.refinery-full-observation-request/1"
                && request["runtime_source"] == proof.receipt["runtime_source"],
            "refinery_plan_current_source_changed"
        );
        ensure!(
            *logical_target == self.preview_delta_manifest(&docs["delta"], &docs["binding"])?,
            "refinery_plan_logical_target_changed"
        );
        let record = json!({"schema":"semantica.refinery-full-observation-request/1","request":request,"logical_target_manifest":logical_target});
        let bytes = crate::refinery::encoded(&record)?;
        let path = format!("refinements/{delta}/input-plan.json");
        match self.store.read(&path)? {
            Some(old) => ensure!(old == bytes, "refinery_full_plan_exact_retry_changed"),
            None => self.store.write_new(&path, &bytes)?,
        };
        Ok(())
    }
    pub fn replay_current_engagement(
        &self,
        delta: &str,
        source: &Value,
    ) -> Result<CurrentEngagement<'_>> {
        let (_, docs, _) = self.load(delta)?;
        let record = self.document(&format!("refinements/{delta}/reference-execution.json"))?;
        refinery_subject::keys(
            &record,
            &[
                "schema",
                "delta_sha256",
                "engagement_receipt_sha256",
                "reference_manifest",
                "reference_manifest_sha256",
                "scenario_ids",
                "snapshot_object_sha256",
                "runtime_source",
                "created_at",
            ],
            &[],
        )?;
        ensure!(
            record["schema"] == "semantica.refinery-retained-reference-rust/1"
                && record["delta_sha256"] == delta
                && record["engagement_receipt_sha256"] == docs["engagement"]["receipt_sha256"]
                && record["runtime_source"] == *source
                && record["reference_manifest_sha256"] == hash(&record["reference_manifest"])?,
            "refinery_reference_replay_identity"
        );
        let scenarios = refinery_subject::texts(&record["scenario_ids"], false)?;
        let snapshots = values(&record, "snapshot_object_sha256")?
            .iter()
            .map(|v| self.object(v.as_str().context("refinery_reference_object_sha")?))
            .collect::<Result<Vec<_>>>()?;
        let provider = refinery_subject::CasProvider { engine: self };
        let proof = self.engage_current(
            &provider,
            &record["reference_manifest"],
            &scenarios,
            &docs["envelope"],
            &docs["binding"],
            &docs["engagement"]["learning"],
            source,
            text(&record, "created_at")?,
        )?;
        ensure!(
            proof.receipt == docs["engagement"] && proof.snapshots == snapshots,
            "refinery_reference_actual_restart_differs"
        );
        Ok(proof)
    }
    pub fn commit_current(
        &self,
        proof: &CurrentEngagement<'_>,
        delta: &str,
        task: &Value,
        authorization: &Value,
        when: &str,
    ) -> Result<Value> {
        self.current_engagement_binding(proof)?;
        let (_, docs, _) = self.load(delta)?;
        ensure!(
            docs["engagement"] == proof.receipt,
            "refinery_commit_reference_receipt_changed"
        );
        self.commit_impl(delta, task, &proof.binding, authorization, when)
    }
}
