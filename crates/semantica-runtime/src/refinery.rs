//! One source-compatible industry registry. Storage and authority are injected;
//! this module never creates a Project, version registry, or engineering truth.
use crate::refinery_contracts as wire;
use anyhow::{Context, Result, bail, ensure};
use semantica_contracts::{
    SemanticRefineryFailure, SemanticRefineryPort, SemanticRefineryRequest, SemanticRefineryResult,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

pub const CONTRACT: &str = "semantica.ontology.refinery/v1";
pub const DATA_PROFILE: &str = "semantica.refinery-data-control/1";
pub const RUST_PROFILE: &str = "semantica.refinery-rust-observation/1";
pub const STATES: &[&str] = &[
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted",
];
pub const CATEGORIES: &[&str] = &[
    "ontology",
    "competency_questions",
    "shapes",
    "queries",
    "rules",
    "cases",
    "contract",
    "provenance",
];
pub const MAX_EVENTS: usize = 6;
const LIMIT: usize = 16 * 1024 * 1024;
static STAGE_SERIAL: AtomicU64 = AtomicU64::new(0);

/// The application supplies its existing workspace, lexical containment,
/// exclusive operation lock, regular-file reads and durable atomic replacement.
/// Implementations must reject symlinks and must not silently reset failures.
pub trait RefineryStore {
    fn read(&self, relative: &str) -> Result<Option<Vec<u8>>>;
    fn write_new(&self, relative: &str, bytes: &[u8]) -> Result<()>;
    fn replace(&self, relative: &str, expected: &[u8], bytes: &[u8]) -> Result<()>;
    fn remove(&self, relative: &str, expected: &[u8]) -> Result<()>;
    fn list(&self, relative: &str) -> Result<Vec<String>>;
    fn create_dir(&self, relative: &str) -> Result<()>;
    fn ensure_dir(&self, relative: &str) -> Result<()> {
        if self.list(relative).is_ok() {
            Ok(())
        } else {
            self.create_dir(relative)
        }
    }
    fn publish_dir(&self, stage: &str, destination: &str) -> Result<()>;
    /// Remove only a freshly created unpublished operation stage. On failure
    /// the adapter must retain evidence instead of resetting the registry.
    fn discard_stage(&self, _stage: &str) -> Result<()> {
        bail!("refinery_stage_cleanup_adapter_unavailable")
    }
}

pub fn hash(value: &Value) -> Result<String> {
    Ok(crate::sha256(semantica_contracts::json::canonical_json(
        value,
    )?))
}
pub fn encoded(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = semantica_contracts::json::canonical_json(value)?;
    bytes.push(b'\n');
    ensure!(bytes.len() <= LIMIT, "refinery_document_budget");
    Ok(bytes)
}
pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().context("refinery_text_missing")
}
pub fn valid_sha(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "refinery_sha256_invalid"
    );
    Ok(())
}
pub(crate) fn values<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value[key].as_array().context("refinery_array_missing")
}
pub(crate) fn includes(value: &Value, key: &str, token: &str) -> bool {
    value[key]
        .as_array()
        .is_some_and(|v| v.iter().any(|v| v == token))
}
pub fn uri_within(uri: &str, root: &str) -> bool {
    wire::uri_within(uri, root)
}
pub(crate) fn required(value: &Value, key: &str) -> Result<Value> {
    value
        .get(key)
        .cloned()
        .context("refinery_required_value_missing")
}
pub(crate) fn keyed_sha(mut value: Value, key: &str) -> Result<Value> {
    value[key] = json!(hash(&value)?);
    Ok(value)
}
pub(crate) fn verify_sha(value: &Value, key: &str) -> Result<()> {
    let expected = text(value, key)?;
    valid_sha(expected)?;
    let mut content = value.clone();
    content
        .as_object_mut()
        .context("refinery_not_mapping")?
        .remove(key);
    ensure!(
        hash(&content)? == expected,
        "refinery_content_hash_mismatch:{key}"
    );
    Ok(())
}

/// A transient interface to the same application-owned industry workspace.
/// The caller holds its operation lock for this object's complete lifetime.
pub struct RefineryEngine<'a> {
    pub(crate) store: &'a dyn RefineryStore,
    pub(crate) registry_id: String,
}
impl<'a> RefineryEngine<'a> {
    pub fn new(store: &'a dyn RefineryStore, registry_id: &str) -> Result<Self> {
        let result = Self {
            store,
            registry_id: registry_id.into(),
        };
        let workspace = result.document("workspace.json")?;
        crate::refinery_history::workspace(&workspace, registry_id)?;
        result.registry()?;
        Ok(result)
    }
    pub fn initialize(
        store: &dyn RefineryStore,
        registry_id: &str,
        created_at: &str,
    ) -> Result<()> {
        wire::opaque_identifier(&json!(registry_id))?;
        wire::timestamp(&json!(created_at))?;
        ensure!(
            store.read("workspace.json")?.is_none() && store.read("registry.json")?.is_none(),
            "refinery_workspace_exists"
        );
        for path in [
            "objects",
            "objects/sha256",
            "engagements",
            "execution-suites",
            "refinements",
            "packages",
            "registry-events",
        ] {
            store.create_dir(path)?;
        }
        let empty = json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]});
        store.write_new("workspace.json", &encoded(&json!({"schema_version":"1.0","contract":CONTRACT,"registry_id":registry_id,"created_at":created_at,"empty_package_sha256":hash(&empty)?}))?)?;
        store.write_new("registry.json", &encoded(&json!({"schema_version":"1.0","registry_id":registry_id,"sequence":0,"last_event_sha256":null,"packages_sha256":hash(&json!({}))?,"packages":{}}))?)?;
        Ok(())
    }
    pub fn document(&self, path: &str) -> Result<Value> {
        let bytes = self
            .store
            .read(path)?
            .context("refinery_document_missing")?;
        ensure!(bytes.len() <= LIMIT, "refinery_document_budget");
        let value = semantica_contracts::json::strict_json(&bytes)?;
        ensure!(value.is_object(), "refinery_document_not_mapping");
        Ok(value)
    }
    pub(crate) fn registry(&self) -> Result<Value> {
        self.verified_registry()
    }
    pub(crate) fn put(&self, bytes: &[u8]) -> Result<String> {
        ensure!(bytes.len() <= LIMIT, "refinery_object_budget");
        let sha = crate::sha256(bytes);
        self.store
            .ensure_dir(&format!("objects/sha256/{}", &sha[..2]))?;
        let path = format!("objects/sha256/{}/{sha}", &sha[..2]);
        match self.store.read(&path)? {
            Some(current) => ensure!(current == bytes, "refinery_object_hash_collision"),
            None => self.store.write_new(&path, bytes)?,
        }
        Ok(sha)
    }
    pub(crate) fn object(&self, sha: &str) -> Result<Value> {
        valid_sha(sha)?;
        let bytes = self
            .store
            .read(&format!("objects/sha256/{}/{sha}", &sha[..2]))?
            .context("refinery_object_missing")?;
        ensure!(
            crate::sha256(&bytes) == sha,
            "refinery_object_bytes_changed"
        );
        Ok(semantica_contracts::json::strict_json(&bytes)?)
    }
    pub(crate) fn baseline(&self, delta: &Value, binding: &Value) -> Result<()> {
        let registry = self.registry()?;
        ensure!(
            delta["package_id"] == binding["package_id"]
                && delta["base_version"] == binding["baseline_version"]
                && delta["base_package_sha256"] == binding["baseline_package_sha256"],
            "refinery_baseline_binding_mismatch"
        );
        let package_id = text(delta, "package_id")?;
        if delta["base_version"] == "0" {
            ensure!(
                registry["packages"].get(package_id).is_none(),
                "refinery_baseline_not_empty"
            );
        } else {
            let descriptor =
                self.resolve_package(package_id, Some(text(delta, "base_version")?))?;
            ensure!(
                descriptor["package_sha256"] == delta["base_package_sha256"],
                "refinery_immutable_baseline_digest"
            );
        }
        Ok(())
    }
    pub(crate) fn engagement_binding(
        &self,
        task: &Value,
        binding: &Value,
        engagement: &Value,
    ) -> Result<()> {
        ensure!(
            binding["workspace_id"] == self.registry_id
                && task["project_id"] == binding["project_id"]
                && task["domain"] == binding["domain"],
            "refinery_engagement_project_binding"
        );
        ensure!(
            engagement["envelope_sha256"] == task["envelope_sha256"]
                && engagement["binding_sha256"] == binding["binding_sha256"]
                && engagement["required_capabilities"] == task["required_capabilities"],
            "refinery_engagement_exact_binding"
        );
        for source in values(task, "evidence")? {
            ensure!(
                uri_within(text(source, "uri")?, text(binding, "evidence_root")?),
                "refinery_source_outside_root"
            );
        }
        Ok(())
    }
    pub(crate) fn context(
        &self,
        action: &str,
        delta: &Value,
        task: &Value,
        binding: &Value,
    ) -> Result<Value> {
        ensure!(
            binding["workspace_id"] == self.registry_id
                && delta["package_id"] == binding["package_id"]
                && binding["semantic_api_contract"] == CONTRACT,
            "refinery_context_workspace_package"
        );
        ensure!(
            task["project_id"] == binding["project_id"]
                && task["domain"] == binding["domain"]
                && task["requested_actions"] == json!([action]),
            "refinery_context_exact_action"
        );
        for cap in values(delta, "required_capabilities")? {
            ensure!(
                values(task, "required_capabilities")?.contains(cap),
                "refinery_context_capability_missing"
            );
        }
        for source in values(task, "evidence")? {
            ensure!(
                uri_within(text(source, "uri")?, text(binding, "evidence_root")?),
                "refinery_context_source_outside_root"
            );
        }
        wire::transition_context(action, text(delta, "delta_sha256")?, task, binding)
    }
    pub fn open_data(&self, task: &Value, binding: &Value, engagement: &Value) -> Result<Value> {
        let task = wire::task(task)?;
        let binding = wire::binding(binding)?;
        let engagement = normalize_engagement(engagement)?;
        self.record_normalized_engagement(&task, &binding, &engagement)
    }
    fn record_normalized_engagement(
        &self,
        task: &Value,
        binding: &Value,
        engagement: &Value,
    ) -> Result<Value> {
        self.engagement_binding(task, binding, engagement)?;
        let record = json!({"schema_version":"1.0","envelope":task,"binding":binding,"engagement_receipt":engagement});
        let bytes = encoded(&record)?;
        let path = format!("engagements/{}.json", crate::sha256(&bytes));
        match self.store.read(&path)? {
            Some(current) => ensure!(current == bytes, "refinery_engagement_digest_collision"),
            None => self.store.write_new(&path, &bytes)?,
        }
        Ok(record["engagement_receipt"].clone())
    }
    pub(crate) fn candidate_checks(
        &self,
        delta: &Value,
        task: &Value,
        binding: &Value,
        engagement: &Value,
    ) -> Result<()> {
        self.engagement_binding(task, binding, engagement)?;
        self.baseline(delta, binding)?;
        ensure!(
            includes(binding, "fact_authorities", text(delta, "created_by")?)
                && includes(binding, "allowed_actions", "candidate"),
            "refinery_candidate_fact_authority"
        );
        ensure!(
            engagement["status"] == "complete"
                && engagement["learning"]["status"] == "candidate"
                && engagement["learning"]["delta_sha256"] == delta["delta_sha256"],
            "refinery_candidate_engagement_blocked"
        );
        for source in values(delta, "source_evidence")? {
            ensure!(
                values(task, "evidence")?.contains(source)
                    && uri_within(text(source, "uri")?, text(binding, "evidence_root")?),
                "refinery_candidate_exact_task_source"
            );
        }
        Ok(())
    }
    pub(crate) fn register_impl(
        &self,
        delta: &Value,
        candidate_task: &Value,
        binding: &Value,
        engagement: &Value,
        recorded_at: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(recorded_at))?;
        let delta = wire::delta(delta)?;
        let candidate_task = wire::task(candidate_task)?;
        let binding = wire::binding(binding)?;
        let engagement = normalize_engagement(engagement)?;
        let candidate = self.context("candidate", &delta, &candidate_task, &binding)?;
        self.candidate_checks(&delta, &candidate_task, &binding, &engagement)?;
        let digest = text(&delta, "delta_sha256")?;
        let dir = format!("refinements/{digest}");
        ensure!(
            self.store.read(&format!("{dir}/current.json"))?.is_none(),
            "refinery_candidate_already_exists"
        );
        self.record_normalized_engagement(&candidate_task, &binding, &engagement)?;
        let candidate_object = self.put(&encoded(&candidate)?)?;
        let nonce = crate::sha256(format!(
            "{}:{:?}:{}",
            std::process::id(),
            std::time::SystemTime::now(),
            STAGE_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let stage = format!("refinements/.staging-{digest}-{nonce}");
        self.store.create_dir(&stage)?;
        let result = (|| -> Result<Value> {
            self.store.create_dir(&format!("{stage}/events"))?;
            for (name, value) in [
                ("delta", &delta),
                ("envelope", &candidate_task),
                ("binding", &binding),
                ("engagement", &engagement),
            ] {
                self.store
                    .write_new(&format!("{stage}/{name}.json"), &encoded(value)?)?;
            }
            for category in CATEGORIES {
                for asset in values(&delta, category)? {
                    if let Some(bytes) = wire::asset_content(asset)? {
                        self.put(&bytes)?;
                    }
                }
            }
            let candidate_event=self.event(None,"candidate",digest,text(&delta,"package_id")?,None,recorded_at,json!({"envelope_sha256":candidate_task["envelope_sha256"],"binding_sha256":binding["binding_sha256"],"engagement_receipt_sha256":engagement["receipt_sha256"],"engagement_status":engagement["status"],"transition_context_sha256":candidate["context_sha256"],"transition_context_object_sha256":candidate_object}))?;
            self.store.write_new(
                &format!("{stage}/events/{}", event_name(&candidate_event)?),
                &encoded(&candidate_event)?,
            )?;
            self.store.write_new(
                &format!("{stage}/current.json"),
                &encoded(&pointer(&candidate_event)?)?,
            )?;
            self.store.publish_dir(&stage, &dir)?;
            state_from_pointer(pointer(&candidate_event)?)
        })();
        match result {
            Ok(state) => Ok(state),
            Err(error) => match self.store.discard_stage(&stage) {
                Ok(()) => Err(error),
                Err(cleanup) => {
                    Err(error.context(format!("refinery_stage_cleanup_unverified:{cleanup:#}")))
                }
            },
        }
    }
    pub fn register_data(
        &self,
        delta: &Value,
        task: &Value,
        binding: &Value,
        engagement: &Value,
        when: &str,
    ) -> Result<Value> {
        ensure!(
            engagement.get("profile").is_none()
                && engagement["runtime_source"].get("schema").is_none(),
            "refinery_current_reference_requires_actual_capability"
        );
        self.register_impl(delta, task, binding, engagement, when)
    }
    pub fn propose_data(
        &self,
        delta: &Value,
        candidate: &Value,
        proposed: &Value,
        binding: &Value,
        engagement: &Value,
        when: &str,
    ) -> Result<Value> {
        ensure!(
            engagement.get("profile").is_none()
                && engagement["runtime_source"].get("schema").is_none(),
            "refinery_current_reference_requires_actual_capability"
        );
        self.propose_impl(delta, candidate, proposed, binding, engagement, when)
    }
    pub fn commit_data(
        &self,
        delta: &str,
        task: &Value,
        binding: &Value,
        authorization: &Value,
        when: &str,
    ) -> Result<Value> {
        let (_, docs, _) = self.load(delta)?;
        ensure!(
            docs["engagement"].get("profile").is_none()
                && docs["engagement"]["runtime_source"].get("schema").is_none(),
            "refinery_current_commit_requires_actual_reference"
        );
        self.commit_impl(delta, task, binding, authorization, when)
    }
    pub fn propose_context_data(&self, delta: &str, context: &Value, when: &str) -> Result<Value> {
        let (_, docs, _) = self.load(delta)?;
        ensure!(
            docs["engagement"].get("profile").is_none(),
            "refinery_current_proposal_requires_actual_reference"
        );
        self.propose_context_impl(delta, context, when)
    }
    pub(crate) fn propose_context_impl(
        &self,
        delta: &str,
        context: &Value,
        recorded_at: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(recorded_at))?;
        let (state, docs, events) = self.load(delta)?;
        ensure!(
            state["state"] == "candidate",
            "refinery_propose_wrong_state"
        );
        ensure!(
            context["binding"] == docs["binding"],
            "refinery_proposed_retained_binding_changed"
        );
        let expected = self.context(
            "proposed",
            &docs["delta"],
            &context["envelope"],
            &docs["binding"],
        )?;
        ensure!(context == &expected, "refinery_proposed_exact_context");
        ensure!(
            docs["engagement"]["status"] == "complete"
                && includes(&docs["binding"], "allowed_actions", "proposed"),
            "refinery_proposed_action_or_engagement_blocked"
        );
        let object = self.put(&encoded(context)?)?;
        self.append(events.last().context("refinery_empty_history")?,"proposed",None,recorded_at,json!({"proposal":"candidate submitted for governed decision","transition_context_sha256":context["context_sha256"],"transition_context_object_sha256":object}))
    }
    pub(crate) fn propose_impl(
        &self,
        delta: &Value,
        candidate_task: &Value,
        proposed_task: &Value,
        binding: &Value,
        engagement: &Value,
        time: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(time))?;
        let delta = wire::delta(delta)?;
        let binding = wire::binding(binding)?;
        let candidate_task = wire::task(candidate_task)?;
        let proposed_task = wire::task(proposed_task)?;
        let engagement = normalize_engagement(engagement)?;
        let digest = text(&delta, "delta_sha256")?;
        let candidate = self.context("candidate", &delta, &candidate_task, &binding)?;
        if self
            .store
            .read(&format!("refinements/{digest}/current.json"))?
            .is_none()
        {
            self.register_impl(&delta, &candidate_task, &binding, &engagement, time)?;
        }
        let (state, docs, _) = self.load(digest)?;
        ensure!(
            docs["delta"] == delta
                && docs["binding"] == binding
                && docs["envelope"] == candidate_task
                && docs["engagement"] == engagement,
            "refinery_propose_retry_context_differs"
        );
        let proposed = self.context("proposed", &delta, &proposed_task, &binding)?;
        let state = if state["state"] == "candidate" {
            self.propose_context_impl(digest, &proposed, time)?
        } else {
            ensure!(
                state["state"] == "proposed",
                "refinery_propose_retry_wrong_state"
            );
            let events = self.history(digest)?;
            let event = events
                .iter()
                .find(|event| event["state"] == "proposed")
                .context("refinery_propose_retry_incomplete")?;
            ensure!(
                event["payload"]["transition_context_sha256"] == proposed["context_sha256"],
                "refinery_propose_retry_task_differs"
            );
            state
        };
        Ok(
            json!({"delta":delta,"engagement":engagement,"state":state,"transition_contexts":{"candidate":candidate,"proposed":proposed}}),
        )
    }
    pub(crate) fn event(
        &self,
        prior: Option<&Value>,
        state: &str,
        delta: &str,
        package: &str,
        package_sha: Option<&str>,
        time: &str,
        payload: Value,
    ) -> Result<Value> {
        wire::timestamp(&json!(time))?;
        let event = keyed_sha(
            json!({"schema_version":"1.0","sequence":prior.map_or(1,|p|p["sequence"].as_u64().unwrap_or(0)+1),"state":state,"delta_sha256":delta,"package_id":package,"previous_event_sha256":prior.map(|p|p["event_sha256"].clone()),"package_sha256":package_sha.map(json_string).unwrap_or_else(||prior.map_or(Value::Null,|p|p["package_sha256"].clone())),"recorded_at":time,"payload":payload}),
            "event_sha256",
        )?;
        validate_event(&event)?;
        Ok(event)
    }
    pub(crate) fn append(
        &self,
        prior: &Value,
        state: &str,
        package_sha: Option<&str>,
        time: &str,
        payload: Value,
    ) -> Result<Value> {
        let previous = text(prior, "state")?;
        let i = STATES
            .iter()
            .position(|v| *v == previous)
            .context("refinery_unknown_state")?;
        ensure!(
            STATES.get(i + 1) == Some(&state),
            "refinery_invalid_transition"
        );
        let delta = text(prior, "delta_sha256")?;
        let root = format!("refinements/{delta}");
        let current = self
            .store
            .read(&format!("{root}/current.json"))?
            .context("refinery_current_missing")?;
        ensure!(
            semantica_contracts::json::strict_json(&current)? == pointer(prior)?,
            "refinery_concurrent_tip_changed"
        );
        let event = self.event(
            Some(prior),
            state,
            delta,
            text(prior, "package_id")?,
            package_sha,
            time,
            payload,
        )?;
        let next = pointer(&event)?;
        let pending = encoded(
            &json!({"schema_version":"1.0","prior_event_sha256":prior["event_sha256"],"event":event,"pointer":next}),
        )?;
        self.store
            .write_new(&format!("{root}/pending-event.json"), &pending)?;
        self.store.write_new(
            &format!("{root}/events/{}", event_name(&event)?),
            &encoded(&event)?,
        )?;
        self.store
            .replace(&format!("{root}/current.json"), &current, &encoded(&next)?)?;
        self.store
            .remove(&format!("{root}/pending-event.json"), &pending)?;
        state_from_pointer(next)
    }
    fn recover(&self, delta: &str) -> Result<()> {
        let dir = format!("refinements/{delta}");
        let pending_path = format!("{dir}/pending-event.json");
        let Some(bytes) = self.store.read(&pending_path)? else {
            return Ok(());
        };
        let pending = semantica_contracts::json::strict_json(&bytes)?;
        ensure!(
            pending.as_object().is_some_and(|v| v.len() == 4) && pending["schema_version"] == "1.0",
            "refinery_pending_schema"
        );
        let event = &pending["event"];
        validate_event(event)?;
        ensure!(
            event["delta_sha256"] == delta
                && event["previous_event_sha256"] == pending["prior_event_sha256"]
                && pointer(event)? == pending["pointer"],
            "refinery_pending_event_pointer"
        );
        let current_bytes = self
            .store
            .read(&format!("{dir}/current.json"))?
            .context("refinery_pending_current_missing")?;
        let current = semantica_contracts::json::strict_json(&current_bytes)?;
        crate::refinery_history::pointer(&current)?;
        let next = &pending["pointer"];
        let path = format!("{dir}/events/{}", event_name(event)?);
        if let Some(observed) = self.store.read(&path)? {
            ensure!(
                observed == encoded(event)?,
                "refinery_pending_event_changed"
            );
            if current["event_sha256"] == pending["prior_event_sha256"] {
                ensure!(
                    current["sequence"].as_u64().map(|v| v + 1) == event["sequence"].as_u64(),
                    "refinery_pending_sequence"
                );
                self.store.replace(
                    &format!("{dir}/current.json"),
                    &current_bytes,
                    &encoded(next)?,
                )?;
            } else {
                ensure!(
                    current["event_sha256"] == next["event_sha256"],
                    "refinery_pending_tip_conflict"
                );
            }
        } else {
            ensure!(
                current["event_sha256"] == pending["prior_event_sha256"],
                "refinery_pending_lost_event"
            );
        }
        self.store.remove(&pending_path, &bytes)
    }
    pub(crate) fn load_structural(&self, delta: &str) -> Result<(Value, Value, Vec<Value>)> {
        valid_sha(delta)?;
        self.recover(delta)?;
        let dir = format!("refinements/{delta}");
        let current = self.document(&format!("{dir}/current.json"))?;
        crate::refinery_history::pointer(&current)?;
        ensure!(
            current["delta_sha256"] == delta,
            "refinery_pointer_delta_mismatch"
        );
        let mut docs = json!({});
        for key in ["delta", "binding", "envelope", "engagement"] {
            docs[key] = self.document(&format!("{dir}/{key}.json"))?;
        }
        docs["delta"] = wire::delta(&docs["delta"])?;
        docs["binding"] = wire::binding(&docs["binding"])?;
        docs["envelope"] = wire::task(&docs["envelope"])?;
        docs["engagement"] = normalize_engagement(&docs["engagement"])?;
        ensure!(
            docs["delta"]["delta_sha256"] == delta,
            "refinery_saved_delta_changed"
        );
        self.engagement_binding(&docs["envelope"], &docs["binding"], &docs["engagement"])?;
        let names = self.store.list(&format!("{dir}/events"))?;
        ensure!(
            !names.is_empty() && names.len() <= MAX_EVENTS,
            "refinery_event_budget"
        );
        let mut events = Vec::<Value>::new();
        for (i, name) in names.into_iter().enumerate() {
            ensure!(
                !name.contains('/') && name.ends_with(".json"),
                "refinery_event_name_invalid"
            );
            let event = self.document(&format!("{dir}/events/{name}"))?;
            validate_event(&event)?;
            ensure!(
                event["delta_sha256"] == delta
                    && event["package_id"] == docs["delta"]["package_id"]
                    && event["sequence"] == i + 1
                    && event["state"] == STATES[i]
                    && name == event_name(&event)?,
                "refinery_event_sequence_or_identity"
            );
            ensure!(
                event["previous_event_sha256"]
                    == events
                        .last()
                        .map_or(Value::Null, |p| p["event_sha256"].clone()),
                "refinery_event_hash_chain"
            );
            let payload = &event["payload"];
            let context = self.object(text(payload, "transition_context_object_sha256")?)?;
            let expected = self.context(
                STATES[i],
                &docs["delta"],
                &context["envelope"],
                &docs["binding"],
            )?;
            ensure!(
                context == expected
                    && context["context_sha256"] == payload["transition_context_sha256"],
                "refinery_event_context_changed"
            );
            if i == 0 {
                ensure!(
                    context["envelope"] == docs["envelope"]
                        && payload["envelope_sha256"] == docs["envelope"]["envelope_sha256"]
                        && payload["binding_sha256"] == docs["binding"]["binding_sha256"]
                        && payload["engagement_receipt_sha256"]
                            == docs["engagement"]["receipt_sha256"]
                        && payload["engagement_status"] == docs["engagement"]["status"],
                    "refinery_candidate_documents_changed"
                );
            }
            if event["state"] == "committed" {
                let auth = self.object(text(payload, "authorization_object_sha256")?)?;
                let auth = wire::authorization(&auth)?;
                ensure!(
                    auth["authorization_sha256"] == payload["authorization_sha256"],
                    "refinery_commit_authorization_changed"
                );
                let manifest = self.object(text(payload, "manifest_sha256")?)?;
                verify_sha(&manifest, "package_sha256")?;
                ensure!(
                    manifest["package_sha256"] == event["package_sha256"],
                    "refinery_committed_manifest_changed"
                );
                for asset in values(&manifest, "assets")? {
                    self.raw_asset(asset)?;
                }
            }

            events.push(event);
        }
        ensure!(
            pointer(events.last().context("refinery_empty_history")?)? == current,
            "refinery_current_not_latest"
        );
        Ok((state_from_pointer(current)?, docs, events))
    }
    pub(crate) fn load(&self, delta: &str) -> Result<(Value, Value, Vec<Value>)> {
        let loaded = self.load_structural(delta)?;
        self.verify_lifecycle_objects(&loaded.0, &loaded.1, &loaded.2)?;
        Ok(loaded)
    }
    pub fn history(&self, delta: &str) -> Result<Vec<Value>> {
        self.load(delta).map(|v| v.2)
    }
    pub fn status(&self, delta: &str) -> Result<Value> {
        self.load(delta).map(|v| v.0)
    }
    pub(crate) fn raw_asset(&self, asset: &Value) -> Result<Vec<u8>> {
        let sha = text(asset, "sha256")?;
        valid_sha(sha)?;
        let bytes = self
            .store
            .read(&format!("objects/sha256/{}/{sha}", &sha[..2]))?
            .context("refinery_asset_object_missing")?;
        ensure!(
            crate::sha256(&bytes) == sha,
            "refinery_asset_object_changed"
        );
        Ok(bytes)
    }
    pub fn committed_manifest(&self, delta: &str) -> Result<Value> {
        let (_, _, events) = self.load(delta)?;
        let event = events
            .iter()
            .find(|e| e["state"] == "committed")
            .context("refinery_package_not_committed")?;
        self.object(text(&event["payload"], "manifest_sha256")?)
    }
    pub fn commit_context_data(
        &self,
        delta: &str,
        authorization: &Value,
        context: &Value,
        time: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(time))?;
        let (_, docs, _) = self.load(delta)?;
        ensure!(
            context["binding"] == docs["binding"],
            "refinery_commit_retained_binding_changed"
        );
        let expected = self.context(
            "committed",
            &docs["delta"],
            &context["envelope"],
            &docs["binding"],
        )?;
        ensure!(context == &expected, "refinery_commit_exact_context");
        Ok(self.commit_data(
            delta,
            &context["envelope"],
            &docs["binding"],
            authorization,
            time,
        )?["state"]
            .clone())
    }
    pub(crate) fn commit_impl(
        &self,
        delta: &str,
        task: &Value,
        binding: &Value,
        authorization: &Value,
        time: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(time))?;
        let (current, docs, events) = self.load(delta)?;
        ensure!(
            current["state"] == "proposed",
            "refinery_commit_wrong_state"
        );
        let binding = wire::binding(binding)?;
        ensure!(
            binding == docs["binding"],
            "refinery_commit_retained_binding_changed"
        );
        let task = wire::task(task)?;
        let context = self.context("committed", &docs["delta"], &task, &binding)?;
        let auth = wire::authorization(authorization)?;
        self.baseline(&docs["delta"], &binding)?;
        ensure!(
            auth["action"] == "commit"
                && auth["delta_sha256"] == delta
                && auth["package_id"] == docs["delta"]["package_id"]
                && auth["promotion_target"] == binding["promotion_target"]
                && includes(&binding, "decision_authorities", text(&auth, "actor_id")?)
                && includes(&binding, "decision_authorities", text(&auth, "authority")?)
                && includes(&binding, "allowed_actions", "committed")
                && uri_within(
                    text(&auth["source"], "uri")?,
                    text(&binding, "evidence_root")?
                ),
            "refinery_commit_exact_authorization"
        );
        let expected: BTreeSet<Vec<String>> = CATEGORIES
            .iter()
            .flat_map(|k| docs["delta"][k].as_array().into_iter().flatten())
            .filter(|a| a["operation"] != "add")
            .map(decision_key)
            .collect();
        let actual: BTreeSet<Vec<String>> = values(&auth, "decisions")?
            .iter()
            .map(decision_key)
            .collect();
        ensure!(actual == expected, "refinery_commit_destructive_decisions");
        let manifest = self.materialize_package(&docs["delta"])?;
        let context_object = self.put(&encoded(&context)?)?;
        let manifest_object = self.put(&encoded(&manifest)?)?;
        let auth_object = self.put(&encoded(&auth)?)?;
        let state=self.append(events.last().context("refinery_empty_history")?,"committed",Some(text(&manifest,"package_sha256")?),time,json!({"manifest_sha256":manifest_object,"authorization_sha256":auth["authorization_sha256"],"authorization_object_sha256":auth_object,"engagement_receipt_sha256":docs["engagement"]["receipt_sha256"],"transition_context_sha256":context["context_sha256"],"transition_context_object_sha256":context_object}))?;
        Ok(json!({"state":state,"transition_context":context,"manifest":manifest}))
    }
}
pub fn normalize_engagement(value: &Value) -> Result<Value> {
    if value["profile"] == RUST_PROFILE {
        wire::rust_engagement(value)
    } else {
        wire::engagement(value)
    }
}
pub(crate) fn decision_key(value: &Value) -> Vec<String> {
    ["category", "asset_id", "operation", "replaces_sha256"]
        .into_iter()
        .map(|k| value[k].as_str().unwrap_or("").into())
        .collect()
}
fn json_string(value: &str) -> Value {
    json!(value)
}
pub(crate) fn event_name(event: &Value) -> Result<String> {
    Ok(format!(
        "{:06}-{}.json",
        event["sequence"]
            .as_u64()
            .context("refinery_sequence_invalid")?,
        text(event, "event_sha256")?
    ))
}
pub(crate) fn pointer(event: &Value) -> Result<Value> {
    let mut v = json!({"schema_version":"1.0"});
    for key in [
        "delta_sha256",
        "package_id",
        "state",
        "sequence",
        "event_sha256",
        "previous_event_sha256",
        "package_sha256",
        "recorded_at",
    ] {
        v[key] = required(event, key)?;
    }
    Ok(v)
}
pub(crate) fn state_from_pointer(mut value: Value) -> Result<Value> {
    crate::refinery_history::pointer(&value)?;
    value
        .as_object_mut()
        .context("refinery_pointer_not_mapping")?
        .remove("schema_version");
    Ok(value)
}
fn validate_event(value: &Value) -> Result<()> {
    crate::refinery_history::event(value)
}

impl SemanticRefineryPort for RefineryEngine<'_> {
    fn execute(
        &self,
        input: &SemanticRefineryRequest,
    ) -> std::result::Result<SemanticRefineryResult, SemanticRefineryFailure> {
        let run = (|| -> Result<Value> {
            input
                .validate()
                .map_err(|e| anyhow::anyhow!("refinery_request_contract:{e:?}"))?;
            ensure!(
                input.registry_id == self.registry_id,
                "refinery_request_registry_identity"
            );
            let task = input.task.as_ref().context("refinery_task_required");
            let candidate = input
                .candidate
                .as_deref()
                .context("refinery_candidate_required");
            match input.operation.as_str() {
                "open" => self.open_data(
                    task?,
                    &input.binding,
                    input
                        .engagement
                        .as_ref()
                        .context("refinery_engagement_required")?,
                ),
                "history" => Ok(json!({"events":self.history(candidate?)?})),
                "status" => Ok(json!({"state":self.status(candidate?)?})),
                "propose" => {
                    let mut proposed = task?.clone();
                    proposed["requested_actions"] = json!(["proposed"]);
                    proposed
                        .as_object_mut()
                        .context("refinery_task_not_mapping")?
                        .remove("envelope_sha256");
                    self.propose_data(
                        input.delta.as_ref().context("refinery_delta_required")?,
                        input.task.as_ref().context("refinery_task_required")?,
                        &proposed,
                        &input.binding,
                        input
                            .engagement
                            .as_ref()
                            .context("refinery_engagement_required")?,
                        &input.recorded_at,
                    )
                }
                "commit" => self.commit_data(
                    candidate?,
                    task?,
                    &input.binding,
                    input
                        .authorization
                        .as_ref()
                        .context("refinery_authorization_required")?,
                    &input.recorded_at,
                ),
                _ => bail!("refinery_subject_gate_or_promotion_profile_uncovered"),
            }
        })();
        run.map(|result| SemanticRefineryResult {
            schema: "semantica.refinery-result/1".into(),
            operation: input.operation.clone(),
            profile: DATA_PROFILE.into(),
            status: "completed".into(),
            result,
            uncovered: vec![
                "caller data is not trusted execution evidence".into(),
                "engineering adoption and publication".into(),
                "existing promoted package replay and destructive baseline materialization".into(),
                "full subject execution/provenance/regression/release/promotion".into(),
            ],
        })
        .map_err(|e| SemanticRefineryFailure {
            code: "refinery_operation_rejected".into(),
            phase: input.operation.clone(),
            message: format!("{e:#}"),
            retryable: false,
        })
    }
}
