//! Immutable version publication and durable replay over the existing store.
use crate::{
    refinery::{RefineryEngine, encoded, hash, includes, keyed_sha, text, values},
    refinery_contracts as wire,
    refinery_subject::{self, closure, gate},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn package_path(id: &str, version: &str) -> String {
    format!(
        "packages/{}/versions/{}/record.json",
        crate::sha256(id),
        crate::sha256(version)
    )
}
impl RefineryEngine<'_> {
    pub(crate) fn verified_registry(&self) -> Result<Value> {
        let original_bytes = self
            .store
            .read("registry.json")?
            .context("refinery_registry_missing")?;
        let mut pointer = semantica_contracts::json::strict_json(&original_bytes)?;
        ensure!(
            pointer["schema_version"] == "1.0" && pointer["registry_id"] == self.registry_id,
            "refinery_registry_identity"
        );
        let seq = pointer["sequence"]
            .as_u64()
            .context("refinery_registry_sequence")?;
        ensure!(
            pointer["packages"].is_object()
                && pointer["packages_sha256"] == hash(&pointer["packages"])?,
            "refinery_registry_package_hash"
        );
        let names = self.store.list("registry-events")?;
        ensure!(
            names.len() == seq as usize || names.len() == seq as usize + 1,
            "refinery_registry_event_count"
        );
        let mut events = Vec::new();
        let mut records = BTreeMap::new();
        for name in names {
            ensure!(
                !name.contains('/') && name.ends_with(".json"),
                "refinery_registry_event_name"
            );
            let event = self.document(&format!("registry-events/{name}"))?;
            crate::refinery_registry_algorithms::registry_event(&event)?;
            let id = text(&event, "package_id")?;
            let version = text(&event, "version")?;
            let record = self.document(&package_path(id, version))?;
            crate::refinery_registry_algorithms::record_data(
                &record,
                id,
                version,
                text(&event, "promotion_record_sha256")?,
            )?;
            ensure!(
                records
                    .insert((id.to_owned(), version.to_owned()), record)
                    .is_none(),
                "refinery_registry_version_duplicate"
            );
            events.push((name, event));
        }
        if events.len() == seq as usize + 1 {
            let prefix = events[..seq as usize].to_vec();
            crate::refinery_registry_algorithms::reconstruct(
                &self.registry_id,
                &pointer,
                &prefix,
                &records,
            )?;
            let event = &events.last().context("refinery_registry_empty_events")?.1;
            ensure!(
                event["sequence"] == seq + 1
                    && event["previous_event_sha256"] == pointer["last_event_sha256"],
                "refinery_registry_recovery_chain"
            );
            let mut packages = pointer["packages"].clone();
            let id = text(event, "package_id")?;
            let version = text(event, "version")?;
            let record = records
                .get(&(id.to_owned(), version.to_owned()))
                .context("refinery_registry_recovery_record")?;
            let mut package=packages.get(id).cloned().unwrap_or_else(||json!({"storage_key":crate::sha256(id),"current_version":null,"current_package_sha256":null,"versions":{}}));
            ensure!(
                package["versions"].get(version).is_none(),
                "refinery_registry_repeated_version"
            );
            package["versions"][version] = json!({"version_key":crate::sha256(version),"record_sha256":event["promotion_record_sha256"]});
            package["current_version"] = json!(version);
            package["current_package_sha256"] = record["package_sha256"].clone();
            packages[id] = package;
            let recovered = json!({"schema_version":"1.0","registry_id":self.registry_id,"sequence":seq+1,"last_event_sha256":event["event_sha256"],"packages_sha256":hash(&packages)?,"packages":packages});
            crate::refinery_registry_algorithms::reconstruct(
                &self.registry_id,
                &recovered,
                &events,
                &records,
            )?;
            self.store
                .replace("registry.json", &original_bytes, &encoded(&recovered)?)?;
            pointer = recovered;
        }
        crate::refinery_registry_algorithms::reconstruct(
            &self.registry_id,
            &pointer,
            &events,
            &records,
        )?;
        Ok(pointer)
    }
    pub fn resolve_package(&self, id: &str, version: Option<&str>) -> Result<Value> {
        wire::opaque_identifier(&json!(id))?;
        let registry = self.registry()?;
        let package = registry["packages"]
            .get(id)
            .context("refinery_package_not_found")?;
        let version = version.unwrap_or(text(package, "current_version")?);
        let binding = package["versions"]
            .get(version)
            .context("refinery_package_version_not_found")?;
        let record = self.document(&package_path(id, version))?;
        crate::refinery_registry_algorithms::record_data(
            &record,
            id,
            version,
            text(binding, "record_sha256")?,
        )?;
        crate::refinery_registry_algorithms::descriptor(&record)
    }
    fn publish_record(&self, record: &Value) -> Result<()> {
        let descriptor = crate::refinery_registry_algorithms::descriptor(record)?;
        let id = text(&descriptor, "package_id")?;
        let version = text(&descriptor, "version")?;
        let current = self
            .store
            .read("registry.json")?
            .context("refinery_registry_missing")?;
        let registry = self.registry()?;
        ensure!(
            registry["packages"][id]["versions"].get(version).is_none(),
            "refinery_immutable_package_version_exists"
        );
        let path = package_path(id, version);
        let parent = path.rsplit_once('/').context("refinery_record_path")?.0;
        let bytes = encoded(record)?;
        self.store
            .ensure_dir(&format!("packages/{}", crate::sha256(id)))?;
        self.store
            .ensure_dir(&format!("packages/{}/versions", crate::sha256(id)))?;
        if let Some(existing) = self.store.read(&path)? {
            ensure!(
                existing == bytes,
                "refinery_immutable_package_storage_conflicts"
            );
        } else {
            let nonce = crate::sha256(format!(
                "{}:{:?}:{id}:{version}",
                std::process::id(),
                std::time::SystemTime::now()
            ));
            let stage = format!("packages/{}/versions/.staging-{nonce}", crate::sha256(id));
            self.store.create_dir(&stage)?;
            let result = (|| -> Result<()> {
                self.store
                    .write_new(&format!("{stage}/record.json"), &bytes)?;
                self.store.publish_dir(&stage, parent)
            })();
            if let Err(error) = result {
                match self.store.discard_stage(&stage) {
                    Ok(()) => return Err(error),
                    Err(cleanup) => {
                        return Err(
                            error.context(format!("refinery_stage_cleanup_unverified:{cleanup:#}"))
                        );
                    }
                }
            }
        }
        let mut packages = registry["packages"].clone();
        let mut package=packages.get(id).cloned().unwrap_or_else(||json!({"storage_key":crate::sha256(id),"current_version":null,"current_package_sha256":null,"versions":{}}));
        package["versions"][version] = json!({"version_key":crate::sha256(version),"record_sha256":record["promotion_record_sha256"]});
        package["current_version"] = json!(version);
        package["current_package_sha256"] = record["package_sha256"].clone();
        packages[id] = package;
        let sequence = registry["sequence"]
            .as_u64()
            .context("refinery_registry_sequence")?
            + 1;
        let event = keyed_sha(
            json!({"schema_version":"1.0","sequence":sequence,"previous_event_sha256":registry["last_event_sha256"],"operation":"promote","package_id":id,"version":version,"package_sha256":record["package_sha256"],"promotion_record_sha256":record["promotion_record_sha256"],"packages_sha256":hash(&packages)?,"recorded_at":record["promoted_at"]}),
            "event_sha256",
        )?;
        crate::refinery_registry_algorithms::registry_event(&event)?;
        self.store.write_new(
            &format!(
                "registry-events/{sequence:06}-{}.json",
                text(&event, "event_sha256")?
            ),
            &encoded(&event)?,
        )?;
        let next = json!({"schema_version":"1.0","registry_id":self.registry_id,"sequence":sequence,"last_event_sha256":event["event_sha256"],"packages_sha256":event["packages_sha256"],"packages":packages});
        self.store
            .replace("registry.json", &current, &encoded(&next)?)?;
        self.registry()?;
        Ok(())
    }
    pub(crate) fn promotion_closure(
        &self,
        docs: &Value,
        events: &[Value],
        context: &Value,
        context_object: &str,
        persist: bool,
    ) -> Result<(Value, String)> {
        let regression = events
            .iter()
            .find(|e| e["state"] == "regression_passed")
            .context("refinery_promotion_regression_missing")?;
        let release = events
            .iter()
            .find(|e| e["state"] == "release_complete")
            .context("refinery_promotion_release_missing")?;
        let regression_evidence = self.object(text(
            &regression["payload"],
            "regression_evidence_object_sha256",
        )?)?;
        let release_evidence =
            self.object(text(&release["payload"], "release_evidence_object_sha256")?)?;
        gate(&regression_evidence)?;
        gate(&release_evidence)?;
        let (suite, _) = self.suite_index(
            text(&docs["delta"], "delta_sha256")?,
            text(&release_evidence, "execution_suite_sha256")?,
        )?;
        let execution = self.object(text(&suite, "provenance_closure_object_sha256")?)?;
        let reg = self.object(text(
            &regression_evidence,
            "provenance_closure_object_sha256",
        )?)?;
        let rel = self.object(text(&release_evidence, "provenance_closure_object_sha256")?)?;
        let mut contexts = BTreeMap::new();
        for c in [&execution, &reg, &rel] {
            closure(c)?;
            for ctx in values(c, "transition_contexts")? {
                let action = text(ctx, "action")?.to_owned();
                if let Some(previous) = contexts.insert(action, ctx.clone()) {
                    ensure!(
                        previous == *ctx,
                        "provenance closures disagree on transition context"
                    );
                }
            }
        }
        for event in events {
            let value = json!({"action":event["state"],"context_sha256":event["payload"]["transition_context_sha256"],"context_object_sha256":event["payload"]["transition_context_object_sha256"]});
            let action = text(event, "state")?.to_owned();
            if let Some(previous) = contexts.insert(action, value.clone()) {
                ensure!(
                    previous == value,
                    "event and provenance closure transition contexts differ"
                );
            }
        }
        let promoted = json!({"action":"promoted","context_sha256":context["context_sha256"],"context_object_sha256":context_object});
        if let Some(previous) = contexts.insert("promoted".into(), promoted.clone()) {
            ensure!(
                previous == promoted,
                "promotion transition context differs from lifecycle"
            );
        }
        let mut all = contexts.into_values().collect::<Vec<_>>();
        all.sort_by_key(|v| {
            refinery_subject::CONTEXT_ORDER
                .iter()
                .position(|a| Some(*a) == v["action"].as_str())
                .unwrap_or(usize::MAX)
        });
        let mut result = execution;
        result["phase"] = json!("promotion");
        result["transition_contexts"] = json!(all);
        result["prior_closure_sha256"] = rel["closure_sha256"].clone();
        result.as_object_mut().unwrap().remove("closure_sha256");
        result["closure_sha256"] = json!(hash(&result)?);
        closure(&result)?;
        let sha = self.retain(&result, persist)?;
        Ok((result, sha))
    }
    fn promotion_record(
        &self,
        docs: &Value,
        events: &[Value],
        authorization: &Value,
        auth_object: &str,
        context: &Value,
        context_object: &str,
        closure: &Value,
        closure_object: &str,
        when: &str,
    ) -> Result<Value> {
        let committed = events
            .iter()
            .find(|e| e["state"] == "committed")
            .context("refinery_commit_missing")?;
        let regression = events
            .iter()
            .find(|e| e["state"] == "regression_passed")
            .context("refinery_promotion_regression_missing")?;
        let release = events
            .iter()
            .find(|e| e["state"] == "release_complete")
            .context("refinery_promotion_release_missing")?;
        let evidence = self.object(text(&release["payload"], "release_evidence_object_sha256")?)?;
        keyed_sha(
            json!({"schema_version":"1.0","package_id":docs["delta"]["package_id"],"version":docs["delta"]["target_version"],"package_sha256":committed["package_sha256"],"manifest_sha256":committed["payload"]["manifest_sha256"],"delta_sha256":docs["delta"]["delta_sha256"],"engagement_receipt_sha256":docs["engagement"]["receipt_sha256"],"regression_evidence_sha256":regression["payload"]["regression_evidence_sha256"],"release_evidence_sha256":release["payload"]["release_evidence_sha256"],"subject_execution_suite_sha256":release["payload"]["execution_suite_sha256"],"transition_context_sha256":context["context_sha256"],"transition_context_object_sha256":context_object,"provenance_closure_sha256":closure["closure_sha256"],"provenance_closure_object_sha256":closure_object,"authorization_sha256":authorization["authorization_sha256"],"authorization_object_sha256":auth_object,"book_impact":docs["delta"]["book_impact"],"runtime_source":evidence["runtime_source"],"promoted_at":when,"status":"promoted"}),
            "promotion_record_sha256",
        )
    }
    pub(crate) fn verify_promotion_event(
        &self,
        docs: &Value,
        events: &[Value],
        event: &Value,
    ) -> Result<()> {
        let payload = &event["payload"];
        let context = self.object(text(payload, "transition_context_object_sha256")?)?;
        self.context_checked("promoted", docs, &context)?;
        let auth =
            wire::authorization(&self.object(text(payload, "authorization_object_sha256")?)?)?;
        self.promotion_authorization(docs, &auth)?;
        let (closure, closure_sha) = self.promotion_closure(
            docs,
            events,
            &context,
            text(payload, "transition_context_object_sha256")?,
            false,
        )?;
        ensure!(
            closure["closure_sha256"] == payload["provenance_closure_sha256"]
                && json!(closure_sha) == payload["provenance_closure_object_sha256"]
                && auth["authorization_sha256"] == payload["authorization_sha256"]
                && payload["promotion_target"] == docs["binding"]["promotion_target"],
            "refinery_promotion_event_objects_changed"
        );
        let record = self.promotion_record(
            docs,
            events,
            &auth,
            text(payload, "authorization_object_sha256")?,
            &context,
            text(payload, "transition_context_object_sha256")?,
            &closure,
            text(payload, "provenance_closure_object_sha256")?,
            text(event, "recorded_at")?,
        )?;
        ensure!(
            record["promotion_record_sha256"] == payload["promotion_record_sha256"],
            "promoted event does not bind deterministic registry record"
        );
        crate::refinery_registry_algorithms::descriptor(&record)?;
        Ok(())
    }
    fn promotion_authorization(&self, docs: &Value, auth: &Value) -> Result<()> {
        let binding = &docs["binding"];
        ensure!(
            auth["action"] == "promote"
                && auth["delta_sha256"] == docs["delta"]["delta_sha256"]
                && auth["package_id"] == docs["delta"]["package_id"]
                && auth["promotion_target"] == binding["promotion_target"]
                && includes(binding, "decision_authorities", text(auth, "actor_id")?)
                && includes(binding, "decision_authorities", text(auth, "authority")?)
                && includes(binding, "allowed_actions", "promoted")
                && crate::refinery::uri_within(
                    text(&auth["source"], "uri")?,
                    text(binding, "evidence_root")?
                ),
            "refinery_promotion_exact_authorization"
        );
        Ok(())
    }
    pub fn promote_data(
        &self,
        delta: &str,
        authorization: &Value,
        context: &Value,
        when: &str,
    ) -> Result<Value> {
        let (_, docs, _) = self.load(delta)?;
        ensure!(
            docs["engagement"]["runtime_source"].get("schema").is_none(),
            "refinery_current_promotion_requires_actual_execution"
        );
        self.promote_impl(delta, authorization, context, when)
    }
    pub(crate) fn promote_impl(
        &self,
        delta: &str,
        authorization: &Value,
        context: &Value,
        when: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(when))?;
        self.registry()?;
        let (state, docs, events) = self.load_structural(delta)?;
        self.verify_lifecycle_objects(&state, &docs, &events)?;
        let authorization = wire::authorization(authorization)?;
        self.promotion_authorization(&docs, &authorization)?;
        self.context_checked("promoted", &docs, context)?;
        if state["state"] == "promoted" {
            let event = events.last().context("refinery_empty_history")?;
            let payload = &event["payload"];
            ensure!(
                payload["authorization_sha256"] == authorization["authorization_sha256"],
                "retry authorization differs from promoted event"
            );
            ensure!(
                payload["transition_context_sha256"] == context["context_sha256"]
                    && payload["transition_context_object_sha256"]
                        == crate::sha256(encoded(context)?),
                "retry transition context differs from promoted event"
            );
            let (closure, closure_object) = self.promotion_closure(
                &docs,
                &events,
                context,
                text(payload, "transition_context_object_sha256")?,
                false,
            )?;
            let record = self.promotion_record(
                &docs,
                &events,
                &authorization,
                text(payload, "authorization_object_sha256")?,
                context,
                text(payload, "transition_context_object_sha256")?,
                &closure,
                &closure_object,
                text(event, "recorded_at")?,
            )?;
            if let Ok(existing) = self.resolve_package(
                text(&docs["delta"], "package_id")?,
                Some(text(&docs["delta"], "target_version")?),
            ) {
                ensure!(
                    existing["promotion_record_sha256"] == record["promotion_record_sha256"],
                    "registry version conflicts with promoted refinement"
                );
                return Ok(existing);
            }
            self.baseline(&docs["delta"], &docs["binding"])?;
            self.publish_record(&record)?;
            return crate::refinery_registry_algorithms::descriptor(&record);
        }
        ensure!(
            state["state"] == "release_complete",
            "refinery_promotion_wrong_state"
        );
        self.baseline(&docs["delta"], &docs["binding"])?;
        let context_object = self.retain(context, true)?;
        let auth_object = self.retain(&authorization, true)?;
        let (closure, closure_object) =
            self.promotion_closure(&docs, &events, context, &context_object, true)?;
        let record = self.promotion_record(
            &docs,
            &events,
            &authorization,
            &auth_object,
            context,
            &context_object,
            &closure,
            &closure_object,
            when,
        )?;
        crate::refinery_registry_algorithms::descriptor(&record)?;
        self.append(events.last().context("refinery_empty_history")?,"promoted",Some(text(&record,"package_sha256")?),when,json!({"promotion_record_sha256":record["promotion_record_sha256"],"authorization_sha256":authorization["authorization_sha256"],"authorization_object_sha256":auth_object,"promotion_target":docs["binding"]["promotion_target"],"transition_context_sha256":context["context_sha256"],"transition_context_object_sha256":context_object,"provenance_closure_sha256":closure["closure_sha256"],"provenance_closure_object_sha256":closure_object}))?;
        self.publish_record(&record).context("promotion is prepared but registry publication is incomplete; retry with the exact authorization")?;
        crate::refinery_registry_algorithms::descriptor(&record)
    }
    pub fn verify_published_refinement(&self, delta: &str) -> Result<()> {
        let (state, docs, events) = self.load(delta)?;
        ensure!(state["state"] == "promoted", "refinery_not_promoted");
        let descriptor = self.resolve_package(
            text(&docs["delta"], "package_id")?,
            Some(text(&docs["delta"], "target_version")?),
        )?;
        ensure!(
            descriptor["promotion_record_sha256"]
                == events.last().context("refinery_empty_history")?["payload"]["promotion_record_sha256"]
                && descriptor["package_sha256"] == state["package_sha256"],
            "refinery_promoted_registry_refinement_mismatch"
        );
        Ok(())
    }
}
