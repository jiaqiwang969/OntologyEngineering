//! Contract-locked subject execution and full content replay in the existing
//! industry workspace. Original legacy data and actual current Rust execution
//! have separate source contracts; public JSON is never an execution grant.
use crate::{
    refinery::{
        self, RefineryEngine, encoded, hash, includes, keyed_sha, text, valid_sha, values,
        verify_sha,
    },
    refinery_contracts as wire, refinery_gate_algorithms as algorithms,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const CURRENT_SNAPSHOT: &str = "semantica.refinery-rust-subject-snapshot/1";
pub const CONTEXT_ORDER: &[&str] = &[
    "candidate",
    "proposed",
    "committed",
    "execute_candidate",
    "derive_regression_gate",
    "regression_passed",
    "derive_release_gate",
    "release_complete",
    "promoted",
];
pub const CASE_KINDS: &[&str] = &["positive", "negative", "ambiguity", "prior_release"];

pub(crate) fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let object = value.as_object().context("refinery_mapping_required")?;
    ensure!(
        required.iter().all(|k| object.contains_key(*k))
            && object
                .keys()
                .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str())),
        "refinery_closed_schema"
    );
    Ok(())
}
pub(crate) fn nonempty<'a>(value: &'a Value) -> Result<&'a str> {
    let s = value.as_str().context("refinery_string_required")?;
    ensure!(
        !s.is_empty() && semantica_contracts::json::python_strip(s) == s,
        "refinery_string_empty"
    );
    Ok(s)
}
pub(crate) fn texts(value: &Value, empty: bool) -> Result<Vec<String>> {
    let raw = value.as_array().context("refinery_text_array_required")?;
    ensure!(empty || !raw.is_empty(), "refinery_text_array_empty");
    let strings = raw
        .iter()
        .map(|v| Ok(nonempty(v)?.to_owned()))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        strings.iter().collect::<BTreeSet<_>>().len() == strings.len(),
        "refinery_text_array_duplicate"
    );
    Ok(strings)
}
pub(crate) fn sha_map(value: &Value) -> Result<()> {
    let raw = value.as_object().context("refinery_digest_map_required")?;
    for (k, v) in raw {
        nonempty(&json!(k))?;
        valid_sha(nonempty(v)?)?;
    }
    Ok(())
}
pub(crate) fn content_hash(value: &Value, key: &str, excluded: &[&str]) -> Result<()> {
    let mut content = value.clone();
    let digest = nonempty(&content[key])?.to_owned();
    valid_sha(&digest)?;
    let map = content
        .as_object_mut()
        .context("refinery_mapping_required")?;
    map.remove(key);
    for k in excluded {
        map.remove(*k);
    }
    ensure!(
        hash(&content)? == digest,
        "refinery_content_hash_mismatch:{key}"
    );
    Ok(())
}
pub(crate) fn runtime_source(value: &Value) -> Result<bool> {
    if value.get("schema").is_some() {
        keys(
            value,
            &[
                "schema",
                "source_sha256",
                "executable_sha256",
                "executable_size_bytes",
                "status",
                "compilation_origin",
                "engineering_adoption",
            ],
            &[],
        )?;
        let dto: semantica_contracts::SemanticRefineryRustSource =
            serde_json::from_value(value.clone())?;
        dto.validate()
            .map_err(|e| anyhow::anyhow!("refinery_rust_source_contract:{e:?}"))?;
        ensure!(
            dto.executable_size_bytes > 0
                && dto.compilation_origin == "unknown"
                && dto.engineering_adoption == "unknown",
            "refinery_rust_source_scope"
        );
        Ok(dto.status == "source_bound")
    } else {
        let source = wire::runtime_source(value)?;
        Ok(source["runtime_commit"]
            .as_str()
            .is_some_and(|v| v.len() == 40)
            && !source["runtime_artifact_sha256"].is_null()
            && !source["runtime_version"].is_null())
    }
}
fn case_assets(value: &Value, unique: bool) -> Result<()> {
    let entries = value.as_array().context("refinery_case_array_required")?;
    let mut tuples = Vec::new();
    let mut ids = BTreeSet::new();
    for entry in entries {
        keys(entry, &["case_kind", "asset_id", "sha256"], &[])?;
        let kind = text(entry, "case_kind")?;
        ensure!(CASE_KINDS.contains(&kind), "refinery_case_kind_invalid");
        wire::opaque_identifier(&entry["asset_id"])?;
        valid_sha(text(entry, "sha256")?)?;
        let pair = (
            kind.to_owned(),
            text(entry, "asset_id")?.to_owned(),
            text(entry, "sha256")?.to_owned(),
        );
        ensure!(
            !unique || ids.insert((pair.0.clone(), pair.1.clone())),
            "refinery_duplicate_case"
        );
        tuples.push(pair);
    }
    let mut ordered = tuples.clone();
    ordered.sort();
    ensure!(ordered == tuples, "refinery_cases_not_sorted");
    Ok(())
}
pub fn scenario_run(value: &Value) -> Result<()> {
    keys(
        value,
        &[
            "scenario_id",
            "run_result_object_sha256",
            "receipt_sha256",
            "receipt_object_sha256",
            "executor_package_id",
            "executor_package_version",
            "executor_package_digest",
            "cq_ids",
            "case_assets",
            "status",
            "release_status",
        ],
        &[],
    )?;
    for k in [
        "run_result_object_sha256",
        "receipt_sha256",
        "receipt_object_sha256",
        "executor_package_digest",
    ] {
        valid_sha(text(value, k)?)?;
    }
    for k in [
        "scenario_id",
        "executor_package_id",
        "executor_package_version",
    ] {
        nonempty(&value[k])?;
    }
    wire::opaque_identifier(&value["executor_package_id"])?;
    texts(&value["cq_ids"], true)?;
    case_assets(&value["case_assets"], true)?;
    ensure!(
        ["passed", "failed", "blocked"].contains(&text(value, "status")?)
            && ["complete", "blocked"].contains(&text(value, "release_status")?),
        "refinery_run_status"
    );
    Ok(())
}
pub fn scenario_binding(value: &Value) -> Result<()> {
    keys(
        value,
        &[
            "scenario_id",
            "input_asset_hashes",
            "case_assets",
            "output_hashes",
            "run_result_object_sha256",
            "receipt_sha256",
            "receipt_object_sha256",
            "native_provenance_bundle_sha256",
        ],
        &[],
    )?;
    nonempty(&value["scenario_id"])?;
    for k in [
        "run_result_object_sha256",
        "receipt_sha256",
        "receipt_object_sha256",
        "native_provenance_bundle_sha256",
    ] {
        valid_sha(text(value, k)?)?;
    }
    sha_map(&value["input_asset_hashes"])?;
    sha_map(&value["output_hashes"])?;
    case_assets(&value["case_assets"], false)
}
pub fn closure(value: &Value) -> Result<()> {
    keys(
        value,
        &[
            "phase",
            "workspace_id",
            "delta_sha256",
            "package_id",
            "package_version",
            "package_sha256",
            "manifest_sha256",
            "execution_projection_sha256",
            "source_evidence",
            "candidate_envelope_sha256",
            "candidate_binding_sha256",
            "engagement_receipt_sha256",
            "transition_contexts",
            "runtime_source",
            "provenance_evidence_assets",
            "scenarios",
            "prior_closure_sha256",
            "closure_sha256",
            "schema_version",
        ],
        &[],
    )?;
    ensure!(
        value["schema_version"] == "1.0"
            && ["execution", "regression", "release", "promotion"].contains(&text(value, "phase")?),
        "refinery_closure_schema_phase"
    );
    for k in ["workspace_id", "package_id"] {
        wire::opaque_identifier(&value[k])?;
    }
    nonempty(&value["package_version"])?;
    for k in [
        "delta_sha256",
        "package_sha256",
        "manifest_sha256",
        "execution_projection_sha256",
        "candidate_envelope_sha256",
        "candidate_binding_sha256",
        "engagement_receipt_sha256",
        "closure_sha256",
    ] {
        valid_sha(text(value, k)?)?;
    }
    if !value["prior_closure_sha256"].is_null() {
        valid_sha(text(value, "prior_closure_sha256")?)?;
    }
    let mut ids = BTreeSet::new();
    let sources = values(value, "source_evidence")?;
    ensure!(!sources.is_empty(), "refinery_closure_sources_empty");
    for s in sources {
        let s = wire::source(s)?;
        ensure!(
            ids.insert(s["source_id"].clone().to_string()),
            "refinery_duplicate_source"
        );
    }
    let mut previous = None;
    ensure!(
        !values(value, "transition_contexts")?.is_empty(),
        "refinery_closure_contexts_empty"
    );
    for ctx in values(value, "transition_contexts")? {
        keys(
            ctx,
            &["action", "context_sha256", "context_object_sha256"],
            &[],
        )?;
        let order = CONTEXT_ORDER
            .iter()
            .position(|a| Some(*a) == ctx["action"].as_str())
            .context("refinery_context_action")?;
        ensure!(
            previous.is_none_or(|p| order > p),
            "refinery_context_order_or_duplicate"
        );
        previous = Some(order);
        valid_sha(text(ctx, "context_sha256")?)?;
        valid_sha(text(ctx, "context_object_sha256")?)?;
    }
    sha_map(&value["provenance_evidence_assets"])?;
    ensure!(
        !value["provenance_evidence_assets"]
            .as_object()
            .unwrap()
            .is_empty(),
        "refinery_closure_provenance_assets_empty"
    );
    ensure!(
        !values(value, "scenarios")?.is_empty(),
        "refinery_closure_scenarios_empty"
    );
    ids.clear();
    for s in values(value, "scenarios")? {
        scenario_binding(s)?;
        ensure!(
            ids.insert(text(s, "scenario_id")?.to_owned()),
            "refinery_duplicate_scenario"
        );
    }
    ensure!(
        runtime_source(&value["runtime_source"])?,
        "refinery_closure_source_incomplete"
    );
    content_hash(value, "closure_sha256", &[])
}
pub fn suite(value: &Value) -> Result<()> {
    keys(
        value,
        &[
            "workspace_id",
            "delta_sha256",
            "subject_package_id",
            "subject_package_version",
            "subject_package_sha256",
            "subject_manifest_sha256",
            "execution_projection_sha256",
            "execution_asset_hashes",
            "transition_context_sha256",
            "transition_context_object_sha256",
            "provenance_closure_sha256",
            "provenance_closure_object_sha256",
            "required_prior_cq_bindings",
            "required_prior_case_bindings",
            "cq_registry_bindings",
            "required_scenario_ids",
            "required_cq_ids",
            "required_case_kinds",
            "runs",
            "runtime_source",
            "observed_capabilities",
            "status",
            "created_at",
            "suite_sha256",
            "schema_version",
        ],
        &[],
    )?;
    ensure!(value["schema_version"] == "1.0", "refinery_suite_schema");
    for k in ["workspace_id", "subject_package_id"] {
        wire::opaque_identifier(&value[k])?;
    }
    nonempty(&value["subject_package_version"])?;
    for k in [
        "delta_sha256",
        "subject_package_sha256",
        "subject_manifest_sha256",
        "execution_projection_sha256",
        "transition_context_sha256",
        "transition_context_object_sha256",
        "provenance_closure_sha256",
        "provenance_closure_object_sha256",
        "suite_sha256",
    ] {
        valid_sha(text(value, k)?)?;
    }
    for k in [
        "execution_asset_hashes",
        "required_prior_cq_bindings",
        "required_prior_case_bindings",
        "cq_registry_bindings",
    ] {
        sha_map(&value[k])?;
    }
    for k in [
        "required_scenario_ids",
        "required_cq_ids",
        "required_case_kinds",
        "observed_capabilities",
    ] {
        texts(&value[k], false)?;
    }
    ensure!(
        texts(&value["required_case_kinds"], false)?
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == CASE_KINDS.iter().copied().collect(),
        "refinery_all_case_kinds_required"
    );
    let mut ids = BTreeSet::new();
    let runs = values(value, "runs")?;
    ensure!(!runs.is_empty(), "refinery_suite_runs_empty");
    for run in runs {
        scenario_run(run)?;
        ensure!(
            ids.insert(text(run, "scenario_id")?.to_owned()),
            "refinery_duplicate_run"
        );
    }
    let required = texts(&value["required_scenario_ids"], false)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let complete = ids == required
        && runs
            .iter()
            .all(|r| r["status"] == "passed" && r["release_status"] == "complete");
    ensure!(
        value["status"] == if complete { "complete" } else { "blocked" },
        "refinery_suite_derived_status"
    );
    runtime_source(&value["runtime_source"])?;
    wire::timestamp(&value["created_at"])?;
    content_hash(value, "suite_sha256", &[])
}
pub fn gate(value: &Value) -> Result<()> {
    keys(
        value,
        &[
            "gate",
            "package_sha256",
            "execution_suite_sha256",
            "execution_suite_object_sha256",
            "transition_context_sha256",
            "transition_context_object_sha256",
            "provenance_closure_sha256",
            "provenance_closure_object_sha256",
            "runtime_source",
            "required_capabilities",
            "observed_capabilities",
            "checks",
            "status",
            "recorded_at",
            "missing_capabilities",
            "evidence_sha256",
        ],
        &[],
    )?;
    let expected = match text(value, "gate")? {
        "regression" => &algorithms::REGRESSION_REQUIRED_CHECK_IDS,
        "release" => &algorithms::RELEASE_REQUIRED_CHECK_IDS,
        _ => bail!("refinery_gate_invalid"),
    };
    for k in [
        "package_sha256",
        "execution_suite_sha256",
        "execution_suite_object_sha256",
        "transition_context_sha256",
        "transition_context_object_sha256",
        "provenance_closure_sha256",
        "provenance_closure_object_sha256",
        "evidence_sha256",
    ] {
        valid_sha(text(value, k)?)?;
    }
    let required = texts(&value["required_capabilities"], false)?;
    let observed = texts(&value["observed_capabilities"], true)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let missing = required
        .into_iter()
        .filter(|v| !observed.contains(v))
        .collect::<BTreeSet<_>>();
    ensure!(
        value["missing_capabilities"] == json!(missing),
        "refinery_missing_capability_derivation"
    );
    let checks = values(value, "checks")?;
    ensure!(checks.len() == expected.len(), "refinery_fixed_gate_checks");
    for (check, id) in checks.iter().zip(expected) {
        keys(
            check,
            &["check_id", "passed", "message", "output_sha256"],
            &[],
        )?;
        ensure!(
            check["check_id"] == *id && check["passed"].is_boolean(),
            "refinery_gate_check_order_or_boolean"
        );
        nonempty(&check["message"])?;
        valid_sha(text(check, "output_sha256")?)?;
    }
    let complete = runtime_source(&value["runtime_source"])?
        && missing.is_empty()
        && checks.iter().all(|v| v["passed"] == true);
    ensure!(
        value["status"] == if complete { "complete" } else { "blocked" },
        "refinery_gate_derived_status"
    );
    wire::timestamp(&value["recorded_at"])?;
    content_hash(value, "evidence_sha256", &["missing_capabilities"])
}

impl RefineryEngine<'_> {
    pub(crate) fn retain(&self, value: &Value, persist: bool) -> Result<String> {
        let bytes = encoded(value)?;
        let sha = crate::sha256(&bytes);
        if persist {
            self.put(&bytes)
        } else {
            ensure!(
                self.object(&sha)? == *value,
                "refinery_retained_object_replay_changed"
            );
            Ok(sha)
        }
    }
    /// Read-only exact candidate preview. Incoming bytes are validated against
    /// their original delta SHA; the immutable baseline is read from this Store.
    /// This neither retains objects nor grants lifecycle execution authority.
    pub fn preview_delta_manifest(&self, delta: &Value, binding: &Value) -> Result<Value> {
        let delta = wire::delta(delta)?;
        let binding = wire::binding(binding)?;
        self.baseline(&delta, &binding)?;
        self.materialize_with(&delta, false)
    }
    pub(crate) fn materialize_package(&self, delta: &Value) -> Result<Value> {
        self.materialize_with(delta, true)
    }
    fn materialize_with(&self, delta: &Value, retained: bool) -> Result<Value> {
        let mut assets = BTreeMap::<(String, String), Value>::new();
        if delta["base_version"] != "0" {
            let descriptor = self.resolve_package(
                text(delta, "package_id")?,
                Some(text(delta, "base_version")?),
            )?;
            ensure!(
                descriptor["package_sha256"] == delta["base_package_sha256"],
                "refinery_immutable_baseline_digest"
            );
            let base = self.object(text(&descriptor, "manifest_sha256")?)?;
            verify_sha(&base, "package_sha256")?;
            for a in values(&base, "assets")? {
                self.raw_asset(a)?;
                assets.insert(
                    (text(a, "category")?.into(), text(a, "asset_id")?.into()),
                    a.clone(),
                );
            }
        }
        for category in refinery::CATEGORIES {
            for asset in values(delta, category)? {
                let key = (
                    text(asset, "category")?.into(),
                    text(asset, "asset_id")?.into(),
                );
                let prior = assets.get(&key);
                match text(asset, "operation")? {
                    "add" => ensure!(prior.is_none(), "refinery_add_overwrites_baseline"),
                    "replace" | "remove" => ensure!(
                        prior.is_some_and(|v| v["sha256"] == asset["replaces_sha256"]),
                        "refinery_mutation_exact_prior_hash"
                    ),
                    _ => bail!("refinery_mutation_operation"),
                };
                if asset["operation"] == "remove" {
                    assets.remove(&key);
                } else {
                    if retained {
                        self.raw_asset(asset)?;
                    } else {
                        let bytes = wire::asset_content(asset)?
                            .context("refinery_preview_incoming_bytes_missing")?;
                        ensure!(
                            crate::sha256(bytes) == asset["sha256"],
                            "refinery_preview_incoming_bytes_changed"
                        );
                    }
                    assets.insert(key,json!({"category":asset["category"],"asset_id":asset["asset_id"],"media_type":asset["media_type"],"sha256":asset["sha256"],"role":asset["role"],"case_kind":asset["case_kind"]}));
                }
            }
        }
        ensure!(
            assets
                .values()
                .map(|a| text(a, "asset_id"))
                .collect::<Result<BTreeSet<_>>>()?
                .len()
                == assets.len(),
            "refinery_global_asset_id_duplicate"
        );
        keyed_sha(
            json!({"schema_version":"1.0","contract":refinery::CONTRACT,"package_id":delta["package_id"],"version":delta["target_version"],"base_version":delta["base_version"],"base_package_sha256":delta["base_package_sha256"],"delta_sha256":delta["delta_sha256"],"required_capabilities":delta["required_capabilities"],"source_evidence":delta["source_evidence"],"book_impact":delta["book_impact"],"assets":assets.into_values().collect::<Vec<_>>()}),
            "package_sha256",
        )
    }
    pub fn execution_projection(&self, manifest: &Value) -> Result<(Value, Value)> {
        verify_sha(manifest, "package_sha256")?;
        let assets = values(manifest, "assets")?;
        for a in assets {
            self.raw_asset(a)?;
        }
        let contracts = assets
            .iter()
            .filter(|a| a["category"] == "contract")
            .collect::<Vec<_>>();
        ensure!(
            contracts.len() == 1,
            "executable package requires exactly one contract asset"
        );
        let mut projection = self.object(text(contracts[0], "sha256")?)?;
        validate_projection(&projection, assets)?;
        let mut runtime_assets = Vec::new();
        for asset in assets {
            let id = text(asset, "asset_id")?;
            let settings = &projection["assets"][id];
            let mut runtime = json!({"asset_id":id,"role":settings["role"],"path":format!("objects/sha256/{}/{}",&text(asset,"sha256")?[..2],text(asset,"sha256")?),"sha256":asset["sha256"],"format":settings["format"],"kind":settings["kind"],"load_into_dataset":settings["load_into_dataset"]});
            if !settings["graph_name"].is_null() {
                runtime["graph_name"] = settings["graph_name"].clone();
            }
            runtime_assets.push(runtime);
        }
        self.validate_registries(manifest, &projection)?;
        projection["_projection_sha256"] = contracts[0]["sha256"].clone();
        Ok((
            json!({"schema_version":"1.0","package_id":manifest["package_id"],"version":manifest["version"],"namespace":projection["namespace"],"release_status":projection["release_status"],"execution":projection["execution"],"assets":runtime_assets}),
            projection,
        ))
    }
    fn registry_asset(&self, manifest: &Value, projection: &Value, field: &str) -> Result<Value> {
        let id = text(&projection["execution"], field)?;
        let matches = values(manifest, "assets")?
            .iter()
            .filter(|a| a["asset_id"] == id)
            .collect::<Vec<_>>();
        ensure!(matches.len() == 1, "refinery_registry_asset_ambiguous");
        self.object(text(matches[0], "sha256")?)
    }
    pub(crate) fn cq_bindings(&self, manifest: &Value, projection: &Value) -> Result<Value> {
        let registry = self.registry_asset(manifest, projection, "cq_registry_asset_id")?;
        let mut result = json!({});
        let entries = values(&registry, "competency_questions")?;
        ensure!(
            !entries.is_empty(),
            "CQ registry has no competency questions"
        );
        for entry in entries {
            let id = wire::opaque_identifier(&entry["id"])?;
            ensure!(result.get(id).is_none(), "CQ registry IDs are duplicated");
            result[id] = json!(hash(entry)?);
        }
        Ok(result)
    }
    fn validate_registries(&self, manifest: &Value, projection: &Value) -> Result<()> {
        let registry = self.registry_asset(manifest, projection, "scenario_registry_asset_id")?;
        ensure!(
            registry["runner_contract"] == crate::book_runner::RUNNER_CONTRACT,
            "scenario registry runner_contract is unsupported"
        );
        let scenarios = values(&registry, "scenarios")?;
        ensure!(!scenarios.is_empty(), "scenario registry has no scenarios");
        let mut ids = BTreeSet::new();
        for s in scenarios {
            ensure!(
                ids.insert(nonempty(&s["id"])?),
                "scenario registry IDs are duplicated"
            );
        }
        let cq = self.registry_asset(manifest, projection, "cq_registry_asset_id")?;
        let mut cqs = BTreeMap::new();
        for c in values(&cq, "competency_questions")? {
            let cids = texts(&c["scenario_ids"], false)?;
            ensure!(
                cids.iter().all(|id| ids.contains(id.as_str())),
                "CQ registry binds unknown scenarios"
            );
            ensure!(
                cqs.insert(nonempty(&c["id"])?, cids).is_none(),
                "CQ registry IDs are duplicated"
            );
        }
        ensure!(!cqs.is_empty(), "CQ registry has no competency questions");
        for id in &algorithms::REGRESSION_REQUIRED_CHECK_IDS {
            let check = &projection["gate_suite"][*id];
            let scenarios = texts(&check["scenario_ids"], false)?;
            ensure!(
                scenarios.iter().all(|s| ids.contains(s.as_str())),
                "gate binds unknown scenario"
            );
            for cq in texts(&check["cq_ids"], true)? {
                let bindings = cqs.get(cq.as_str()).context("gate binds unknown CQ")?;
                ensure!(
                    bindings.iter().any(|v| scenarios.contains(v)),
                    "CQ is not bound to its declared scenarios"
                );
            }
        }
        Ok(())
    }
    pub(crate) fn prior_requirements(
        &self,
        delta: &Value,
        manifest: &Value,
        projection: &Value,
        events: &[Value],
    ) -> Result<(Value, Value)> {
        if delta["base_version"] == "0" {
            let cq = self.cq_bindings(manifest, projection)?;
            let mut prior_cq = json!({});
            for id in texts(&projection["gate_suite"]["cq.prior"]["cq_ids"], false)? {
                prior_cq[&id] = cq
                    .get(&id)
                    .context("bootstrap prior requirement is absent from target assets")?
                    .clone();
            }
            let mut cases = json!({});
            for id in texts(
                &projection["gate_suite"]["case.prior_release"]["case_asset_ids"],
                false,
            )? {
                let a = values(manifest, "assets")?
                    .iter()
                    .find(|a| a["category"] == "cases" && a["asset_id"] == id)
                    .context("bootstrap prior requirement is absent from target assets")?;
                cases[&id] = a["sha256"].clone();
            }
            return Ok((prior_cq, cases));
        }
        let descriptor = self.resolve_package(
            text(delta, "package_id")?,
            Some(text(delta, "base_version")?),
        )?;
        ensure!(
            descriptor["package_sha256"] == delta["base_package_sha256"],
            "immutable base descriptor digest mismatch"
        );
        let base = self.object(text(&descriptor, "manifest_sha256")?)?;
        let (_, base_projection) = self.execution_projection(&base)?;
        let cqs = self.cq_bindings(&base, &base_projection)?;
        let commit = events
            .iter()
            .find(|e| e["state"] == "committed")
            .context("refinery_commit_missing")?;
        let auth = wire::authorization(
            &self.object(text(&commit["payload"], "authorization_object_sha256")?)?,
        )?;
        let mutations = values(&auth, "decisions")?;
        let mut cases = json!({});
        for asset in values(&base, "assets")?
            .iter()
            .filter(|a| a["category"] == "cases")
        {
            let changed = mutations.iter().any(|m| {
                m["category"] == "cases"
                    && m["asset_id"] == asset["asset_id"]
                    && (m["operation"] == "replace" || m["operation"] == "remove")
                    && m["replaces_sha256"] == asset["sha256"]
            });
            if !changed {
                cases[text(asset, "asset_id")?] = asset["sha256"].clone();
            }
        }
        Ok((cqs, cases))
    }
    pub(crate) fn provenance_assets(
        &self,
        delta: &Value,
        manifest: &Value,
        projection: &Value,
    ) -> Result<Value> {
        let expected = json!({"schema_version":"1.0","kind":"semantica.source-evidence-binding","source_evidence":delta["source_evidence"]});
        let mut bindings = json!({});
        for id in texts(&projection["provenance_evidence_asset_ids"], false)? {
            let asset = values(manifest, "assets")?
                .iter()
                .find(|a| a["asset_id"] == id && a["category"] == "provenance")
                .context("named provenance-evidence asset is absent")?;
            let actual = self.object(text(asset, "sha256")?)?;
            ensure!(
                actual == expected,
                "provenance-evidence asset does not exactly bind delta source_evidence"
            );
            bindings[&id] = asset["sha256"].clone();
        }
        Ok(bindings)
    }
}

pub fn validate_projection(projection: &Value, assets: &[Value]) -> Result<()> {
    keys(
        projection,
        &[
            "schema_version",
            "runner_contract",
            "namespace",
            "release_status",
            "execution",
            "assets",
            "gate_suite",
            "rights_evidence_asset_ids",
            "provenance_evidence_asset_ids",
        ],
        &[],
    )?;
    ensure!(
        projection["schema_version"] == "1.0"
            && projection["runner_contract"] == crate::book_runner::RUNNER_CONTRACT
            && projection["release_status"] == "complete",
        "refinery_projection_contract"
    );
    absolute_iri(nonempty(&projection["namespace"])?)?;
    let mut by_id = BTreeMap::new();
    for asset in assets {
        let id = wire::opaque_identifier(&asset["asset_id"])?;
        ensure!(
            by_id.insert(id, asset).is_none(),
            "refinery_projection_duplicate_asset"
        );
    }
    let settings = projection["assets"]
        .as_object()
        .context("refinery_projection_assets_mapping")?;
    ensure!(
        settings.keys().map(String::as_str).collect::<BTreeSet<_>>()
            == by_id.keys().copied().collect(),
        "projection asset map differs from manifest"
    );
    let mut contracts = 0;
    for (id, setting) in settings {
        keys(
            setting,
            &["role", "format", "kind", "load_into_dataset"],
            &["graph_name"],
        )?;
        let role = nonempty(&setting["role"])?;
        let kind = nonempty(&setting["kind"])?;
        let format = nonempty(&setting["format"])?;
        let load = setting["load_into_dataset"]
            .as_bool()
            .context("projection asset load_into_dataset must be boolean")?;
        if !setting["graph_name"].is_null() {
            absolute_iri(nonempty(&setting["graph_name"])?)?;
            ensure!(
                load,
                "projection graph_name is only valid for a loaded RDF asset"
            );
        }
        ensure!(
            !load || kind.to_lowercase() == "rdf",
            "only projection assets with kind rdf may load into the dataset"
        );
        let category = &by_id[id.as_str()]["category"];
        if category == "cases" {
            ensure!(
                kind.to_lowercase() == "rdf" && !load,
                "case assets must be RDF and scenario-loaded, not preloaded"
            );
        }
        if category == "queries" {
            ensure!(
                role == "sparql",
                "query assets must use the sparql runner role"
            );
        }
        if category == "contract" {
            contracts += 1;
            ensure!(
                role == "contract"
                    && format.to_lowercase() == "json"
                    && kind.to_lowercase() == "structured"
                    && !load,
                "contract asset must be structured JSON with role contract and no preload"
            );
        } else {
            ensure!(
                !["contract", "chapter_contract"].contains(
                    &semantica_contracts::json::python_strip(role)
                        .to_lowercase()
                        .as_str()
                ),
                "only the manifest contract asset may use a contract runner role"
            );
        }
    }
    ensure!(
        contracts == 1,
        "projection requires exactly one contract role"
    );
    let execution = &projection["execution"];
    keys(
        execution,
        &["scenario_registry_asset_id", "cq_registry_asset_id"],
        &[],
    )?;
    ensure!(
        execution["scenario_registry_asset_id"] != execution["cq_registry_asset_id"],
        "scenario and CQ registries must be distinct assets"
    );
    for field in ["scenario_registry_asset_id", "cq_registry_asset_id"] {
        let id = wire::opaque_identifier(&execution[field])?;
        ensure!(
            by_id.contains_key(id),
            "execution registry absent from manifest"
        );
        let s = &settings[id];
        ensure!(
            text(s, "format")?.to_lowercase() == "json"
                && text(s, "kind")?.to_lowercase() == "structured"
                && s["load_into_dataset"] == false,
            "execution registry assets must be non-preloaded structured JSON"
        );
    }
    keys(
        &projection["gate_suite"],
        &algorithms::REGRESSION_REQUIRED_CHECK_IDS,
        &[],
    )?;
    for (id, kind) in algorithms::REGRESSION_REQUIRED_CHECK_IDS.iter().zip([
        None,
        None,
        Some("positive"),
        Some("negative"),
        Some("ambiguity"),
        Some("prior_release"),
    ]) {
        let check = &projection["gate_suite"][*id];
        keys(check, &["scenario_ids", "cq_ids", "case_asset_ids"], &[])?;
        for s in texts(&check["scenario_ids"], false)? {
            wire::opaque_identifier(&json!(s))?;
        }
        let cqs = texts(&check["cq_ids"], true)?;
        ensure!(
            !id.starts_with("cq.") || !cqs.is_empty(),
            "CQ gate must bind at least one CQ"
        );
        for cq in cqs {
            wire::opaque_identifier(&json!(cq))?;
        }
        let cases = texts(&check["case_asset_ids"], true)?;
        ensure!(
            kind.is_none() || !cases.is_empty(),
            "case gate must bind at least one case asset"
        );
        for c in cases {
            let asset = by_id
                .get(c.as_str())
                .context("gate case asset absent from manifest")?;
            ensure!(
                asset["category"] == "cases" && kind.is_none_or(|kind| asset["case_kind"] == kind),
                "gate case asset category/kind mismatch"
            );
        }
    }
    for field in ["rights_evidence_asset_ids", "provenance_evidence_asset_ids"] {
        for id in texts(&projection[field], false)? {
            wire::opaque_identifier(&json!(id))?;
            ensure!(
                by_id
                    .get(id.as_str())
                    .is_some_and(|a| a["category"] == "provenance"),
                "invalid execution projection: {}",
                if field == "rights_evidence_asset_ids" {
                    "rights evidence must name a content-bound provenance asset"
                } else {
                    "provenance evidence must name a content-bound provenance asset"
                }
            );
        }
    }
    Ok(())
}
fn absolute_iri(value: &str) -> Result<()> {
    let scheme = value
        .split_once(':')
        .map(|v| v.0)
        .context("projection must use absolute IRI")?;
    ensure!(
        !scheme.is_empty()
            && scheme.as_bytes()[0].is_ascii_alphabetic()
            && scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || [b'+', b'-', b'.'].contains(&b))
            && !value
                .chars()
                .any(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)),
        "projection absolute IRI invalid"
    );
    Ok(())
}

fn actual_result(value: &Value) -> Result<(&Value, Option<&Value>)> {
    if value["profile"] == CURRENT_SNAPSHOT {
        keys(value, &["profile", "actual_run", "release_content"], &[])?;
        Ok((&value["actual_run"], Some(&value["release_content"])))
    } else {
        Ok((value, None))
    }
}
fn report(value: &Value, kind: &str) -> Result<()> {
    ensure!(
        crate::execution_provenance::verify_report(value)? && value["kind"] == kind,
        "refinery_run_report_integrity:{kind}"
    );
    Ok(())
}
fn loaded_assets(result: &Value) -> Result<Value> {
    let mut assets = json!({});
    for op in values(result, "operations")? {
        if ["load", "load_asset"].contains(&text(op, "operation")?) {
            let id = nonempty(&op["payload"]["asset_id"])?;
            let digest = nonempty(&op["payload"]["asset_sha256"])?;
            valid_sha(digest)?;
            ensure!(
                assets.get(id).is_none_or(|old| old == digest),
                "refinery_loaded_asset_hash_inconsistent"
            );
            assets[id] = json!(digest);
        }
    }
    Ok(assets)
}
fn actual_cqs(result: &Value) -> Value {
    json!(
        result["cq_report"]["payload"]["competency_question_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .collect::<BTreeSet<_>>()
    )
}
fn actual_cases(result: &Value, manifest: &Value) -> Result<Value> {
    let loaded = loaded_assets(result)?;
    let mut cases = Vec::new();
    for a in values(manifest, "assets")?
        .iter()
        .filter(|a| a["category"] == "cases")
    {
        if loaded.get(text(a, "asset_id")?) == Some(&a["sha256"]) {
            cases.push((
                text(a, "case_kind")?.to_owned(),
                text(a, "asset_id")?.to_owned(),
                text(a, "sha256")?.to_owned(),
            ));
        }
    }
    cases.sort();
    Ok(json!(
        cases
            .into_iter()
            .map(|(kind, id, sha)| json!({"case_kind":kind,"asset_id":id,"sha256":sha}))
            .collect::<Vec<_>>()
    ))
}
fn capabilities(result: &Value, receipt: &Value, current: bool) -> Result<BTreeSet<String>> {
    if current {
        current_observed_capabilities(receipt)
    } else {
        Ok(
            result["capability_report"]["payload"]["profile"]["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }
}
/// Content projection of the current actual capability report. This does not
/// create private execution authority or certify caller-provided report bytes.
pub fn current_observed_capabilities(receipt: &Value) -> Result<BTreeSet<String>> {
    let payload = &receipt["capability_report"]["payload"];
    let supported = payload["supported_profiles"]
        .as_object()
        .context("refinery_current_supported_profiles")?;
    let excluded = texts(&payload["unknown"], true)?
        .into_iter()
        .chain(texts(&payload["missing"], true)?)
        .collect::<BTreeSet<_>>();
    Ok(supported
        .keys()
        .filter(|id| !excluded.contains(*id))
        .cloned()
        .collect())
}
fn required_ids(projection: &Value, field: &str) -> Result<Value> {
    let mut ids = BTreeSet::new();
    for check in projection["gate_suite"]
        .as_object()
        .context("refinery_gate_suite_mapping")?
        .values()
    {
        ids.extend(texts(&check[field], field != "scenario_ids")?);
    }
    Ok(json!(ids))
}
fn asset_hashes(manifest: &Value) -> Result<Value> {
    let mut map = json!({});
    for a in values(manifest, "assets")? {
        let id = text(a, "asset_id")?;
        ensure!(map.get(id).is_none(), "refinery_asset_id_duplicate");
        map[id] = a["sha256"].clone();
    }
    Ok(map)
}
fn source_identity(source: &Value, when: &str) -> Result<Value> {
    ensure!(
        source.get("schema").is_some() && runtime_source(source)?,
        "refinery_current_source_required"
    );
    Ok(
        json!({"created_at":when,"runtime_version":env!("CARGO_PKG_VERSION"),"runtime_commit":format!("semantica.rust-source/{}",text(source,"source_sha256")?),"runtime_artifact_sha256":source["executable_sha256"]}),
    )
}

impl RefineryEngine<'_> {
    pub(crate) fn context_checked(
        &self,
        action: &str,
        docs: &Value,
        context: &Value,
    ) -> Result<()> {
        let expected = self.context(
            action,
            &docs["delta"],
            &context["envelope"],
            &docs["binding"],
        )?;
        ensure!(
            expected == *context && context["binding"] == docs["binding"],
            "refinery_exact_transition_context:{action}"
        );
        Ok(())
    }
    pub(crate) fn suite_index(&self, delta: &str, digest: &str) -> Result<(Value, String)> {
        valid_sha(delta)?;
        valid_sha(digest)?;
        let index = self.document(&format!("execution-suites/{delta}/{digest}.json"))?;
        keys(
            &index,
            &["schema_version", "suite_object_sha256", "suite_sha256"],
            &[],
        )?;
        ensure!(
            index["schema_version"] == "1.0" && index["suite_sha256"] == digest,
            "refinery_suite_index_identity"
        );
        let sha = text(&index, "suite_object_sha256")?.to_owned();
        let value = self.object(&sha)?;
        suite(&value)?;
        ensure!(
            value["suite_sha256"] == digest && value["delta_sha256"] == delta,
            "refinery_suite_object_identity"
        );
        Ok((value, sha))
    }
    pub(crate) fn closure_scenarios(&self, runs: &[Value]) -> Result<Value> {
        let mut scenarios = Vec::new();
        for run in runs {
            let snapshot = self.object(text(run, "run_result_object_sha256")?)?;
            let receipt = self.object(text(run, "receipt_object_sha256")?)?;
            let (result, _) = actual_result(&snapshot)?;
            let value = json!({"scenario_id":run["scenario_id"],"input_asset_hashes":loaded_assets(result)?,"case_assets":run["case_assets"],"output_hashes":receipt["output_hashes"],"run_result_object_sha256":run["run_result_object_sha256"],"receipt_sha256":run["receipt_sha256"],"receipt_object_sha256":run["receipt_object_sha256"],"native_provenance_bundle_sha256":receipt["provenance_bundle"]["bundle_sha256"]});
            scenario_binding(&value)?;
            scenarios.push(value);
        }
        scenarios.sort_by(|a, b| a["scenario_id"].as_str().cmp(&b["scenario_id"].as_str()));
        Ok(json!(scenarios))
    }
    fn execution_closure(
        &self,
        docs: &Value,
        events: &[Value],
        manifest: &Value,
        projection: &Value,
        runs: &[Value],
        source: &Value,
        context: &Value,
        context_sha: &str,
    ) -> Result<Value> {
        let committed = events
            .iter()
            .find(|e| e["state"] == "committed")
            .context("refinery_commit_missing")?;
        let mut contexts=events.iter().take(3).map(|e|json!({"action":e["state"],"context_sha256":e["payload"]["transition_context_sha256"],"context_object_sha256":e["payload"]["transition_context_object_sha256"]})).collect::<Vec<_>>();
        contexts.push(json!({"action":"execute_candidate","context_sha256":context["context_sha256"],"context_object_sha256":context_sha}));
        let value = keyed_sha(
            json!({"schema_version":"1.0","phase":"execution","workspace_id":self.registry_id,"delta_sha256":docs["delta"]["delta_sha256"],"package_id":docs["delta"]["package_id"],"package_version":docs["delta"]["target_version"],"package_sha256":manifest["package_sha256"],"manifest_sha256":committed["payload"]["manifest_sha256"],"execution_projection_sha256":projection["_projection_sha256"],"source_evidence":docs["delta"]["source_evidence"],"candidate_envelope_sha256":docs["envelope"]["envelope_sha256"],"candidate_binding_sha256":docs["binding"]["binding_sha256"],"engagement_receipt_sha256":docs["engagement"]["receipt_sha256"],"transition_contexts":contexts,"runtime_source":source,"provenance_evidence_assets":self.provenance_assets(&docs["delta"],manifest,projection)?,"scenarios":self.closure_scenarios(runs)?,"prior_closure_sha256":null}),
            "closure_sha256",
        )?;
        closure(&value)?;
        Ok(value)
    }
    /// Executes all contract-declared scenarios using actual current BookRunner
    /// state and a staging Dataset. No caller-supplied checks enter this path.
    pub fn execute_candidate_current(
        &self,
        delta: &str,
        context: &Value,
        current_source: &Value,
        when: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(when))?;
        let (state, docs, events) = self.load(delta)?;
        ensure!(
            ["committed", "regression_passed", "release_complete"]
                .contains(&text(&state, "state")?),
            "execute_candidate requires a committed subject package"
        );
        self.context_checked("execute_candidate", &docs, context)?;
        let context_object = self.retain(context, true)?;
        let commit = events
            .iter()
            .find(|e| e["state"] == "committed")
            .context("refinery_commit_missing")?;
        let manifest = self.object(text(&commit["payload"], "manifest_sha256")?)?;
        let (execution, projection) = self.execution_projection(&manifest)?;
        let (prior_cq, prior_cases) =
            self.prior_requirements(&docs["delta"], &manifest, &projection, &events)?;
        self.provenance_assets(&docs["delta"], &manifest, &projection)?;
        let scenarios = required_ids(&projection, "scenario_ids")?;
        let cqs = required_ids(&projection, "cq_ids")?;
        let identity = source_identity(current_source, when)?;
        let provider = CasProvider { engine: self };
        let mut runs = Vec::new();
        let mut observed = BTreeSet::new();
        for scenario in texts(&scenarios, false)? {
            let snapshot = crate::refinery_runner::run_current(
                &provider,
                &execution,
                &scenario,
                &identity,
                current_source,
            )?;
            let result = &snapshot["actual_run"];
            let receipt = &snapshot["release_content"]["receipt"];
            let verdict = &snapshot["release_content"]["content_verdict"];
            let result_sha = self.retain(&snapshot, true)?;
            let receipt_sha = self.retain(receipt, true)?;
            observed.extend(capabilities(result, receipt, true)?);
            let run = json!({"scenario_id":scenario,"run_result_object_sha256":result_sha,"receipt_sha256":receipt["receipt_sha256"],"receipt_object_sha256":receipt_sha,"executor_package_id":result["package_id"],"executor_package_version":result["package_version"],"executor_package_digest":result["package_digest"],"cq_ids":actual_cqs(result),"case_assets":actual_cases(result,&manifest)?,"status":result["status"],"release_status":verdict["status"]});
            scenario_run(&run)?;
            runs.push(run);
        }
        let closure = self.execution_closure(
            &docs,
            &events,
            &manifest,
            &projection,
            &runs,
            current_source,
            context,
            &context_object,
        )?;
        let closure_object = self.retain(&closure, true)?;
        let complete = runs
            .iter()
            .all(|r| r["status"] == "passed" && r["release_status"] == "complete");
        let value = keyed_sha(
            json!({"schema_version":"1.0","workspace_id":self.registry_id,"delta_sha256":delta,"subject_package_id":docs["delta"]["package_id"],"subject_package_version":docs["delta"]["target_version"],"subject_package_sha256":manifest["package_sha256"],"subject_manifest_sha256":commit["payload"]["manifest_sha256"],"execution_projection_sha256":projection["_projection_sha256"],"execution_asset_hashes":asset_hashes(&execution)?,"transition_context_sha256":context["context_sha256"],"transition_context_object_sha256":context_object,"provenance_closure_sha256":closure["closure_sha256"],"provenance_closure_object_sha256":closure_object,"required_prior_cq_bindings":prior_cq,"required_prior_case_bindings":prior_cases,"cq_registry_bindings":self.cq_bindings(&manifest,&projection)?,"required_scenario_ids":scenarios,"required_cq_ids":cqs,"required_case_kinds":CASE_KINDS,"runs":runs,"runtime_source":current_source,"observed_capabilities":observed,"status":if complete{"complete"}else{"blocked"},"created_at":when}),
            "suite_sha256",
        )?;
        suite(&value)?;
        let typed: semantica_contracts::SemanticRefineryRustSubjectExecutionSuite =
            serde_json::from_value(value.clone())?;
        typed
            .validate()
            .map_err(|e| anyhow::anyhow!("refinery_current_suite_contract:{e:?}"))?;
        let sha = self.retain(&value, true)?;
        self.store
            .ensure_dir(&format!("execution-suites/{delta}"))?;
        let path = format!(
            "execution-suites/{delta}/{}.json",
            text(&value, "suite_sha256")?
        );
        let index = json!({"schema_version":"1.0","suite_sha256":value["suite_sha256"],"suite_object_sha256":sha});
        let bytes = encoded(&index)?;
        match self.store.read(&path)? {
            Some(current) => ensure!(current == bytes, "refinery_suite_index_collision"),
            None => self.store.write_new(&path, &bytes)?,
        }
        Ok(value)
    }
    pub fn verify_suite_content(&self, value: &Value) -> Result<()> {
        let (_, docs, events) = self.load_structural(text(value, "delta_sha256")?)?;
        self.verify_suite_with(value, &docs, &events).map(|_| ())
    }
    pub(crate) fn verify_suite_with(
        &self,
        value: &Value,
        docs: &Value,
        events: &[Value],
    ) -> Result<(Value, Value, Value)> {
        suite(value)?;
        let context = self.object(text(value, "transition_context_object_sha256")?)?;
        ensure!(
            context["context_sha256"] == value["transition_context_sha256"],
            "refinery_suite_context_identity"
        );
        self.context_checked("execute_candidate", docs, &context)?;
        let commit = events
            .iter()
            .find(|e| e["state"] == "committed")
            .context("refinery_commit_missing")?;
        let manifest = self.object(text(&commit["payload"], "manifest_sha256")?)?;
        let (execution, projection) = self.execution_projection(&manifest)?;
        let (prior_cq, prior_cases) =
            self.prior_requirements(&docs["delta"], &manifest, &projection, events)?;
        let expected = json!({"workspace_id":self.registry_id,"delta_sha256":docs["delta"]["delta_sha256"],"subject_package_id":docs["delta"]["package_id"],"subject_package_version":docs["delta"]["target_version"],"subject_package_sha256":manifest["package_sha256"],"subject_manifest_sha256":commit["payload"]["manifest_sha256"],"execution_projection_sha256":projection["_projection_sha256"],"execution_asset_hashes":asset_hashes(&execution)?,"required_scenario_ids":required_ids(&projection,"scenario_ids")?,"required_cq_ids":required_ids(&projection,"cq_ids")?,"required_case_kinds":CASE_KINDS,"required_prior_cq_bindings":prior_cq,"required_prior_case_bindings":prior_cases,"cq_registry_bindings":self.cq_bindings(&manifest,&projection)?});
        for (k, v) in expected.as_object().unwrap() {
            ensure!(
                value[k] == *v,
                "execution suite {k} differs from committed subject"
            );
        }
        let closure_value = self.object(text(value, "provenance_closure_object_sha256")?)?;
        closure(&closure_value)?;
        ensure!(
            closure_value["closure_sha256"] == value["provenance_closure_sha256"],
            "suite provenance closure identity mismatch"
        );
        let catalog = crate::books::Catalog::from_execution_manifest(&execution)?;
        let mut staging = crate::dataset::DatasetSession::new();
        let registered = catalog.load_package(
            &CasProvider { engine: self },
            text(value, "subject_package_id")?,
            &mut staging,
        )?;
        let identity = &registered["identity"];
        let mut observed = BTreeSet::new();
        for run in values(value, "runs")? {
            ensure!(
                run["executor_package_id"] == identity["package_id"]
                    && run["executor_package_version"] == identity["version"]
                    && run["executor_package_digest"] == identity["digest"],
                "run executor identity differs from current projection"
            );
            let snapshot = self.object(text(run, "run_result_object_sha256")?)?;
            let receipt = self.object(text(run, "receipt_object_sha256")?)?;
            let (result, current) = actual_result(&snapshot)?;
            verify_run(value, run, result, &receipt, current)?;
            ensure!(
                actual_cases(result, &manifest)? == run["case_assets"],
                "run actual case I/O differs from suite"
            );
            observed.extend(capabilities(result, &receipt, current.is_some())?);
        }
        ensure!(
            value["observed_capabilities"] == json!(observed),
            "suite observed capabilities differ from native runner reports"
        );
        let expected_closure = self.execution_closure(
            docs,
            events,
            &manifest,
            &projection,
            values(value, "runs")?,
            &value["runtime_source"],
            &context,
            text(value, "transition_context_object_sha256")?,
        )?;
        ensure!(
            closure_value == expected_closure,
            "suite provenance closure full replay mismatch"
        );
        Ok((manifest, projection, closure_value))
    }
    fn extended_closure(
        &self,
        docs: &Value,
        events: &[Value],
        suite: &Value,
        gate_kind: &str,
        context: &Value,
        context_object: &str,
        persist: bool,
    ) -> Result<(Value, String)> {
        let mut prior = if gate_kind == "regression" {
            self.object(text(suite, "provenance_closure_object_sha256")?)?
        } else {
            let event = events
                .iter()
                .find(|e| e["state"] == "regression_passed")
                .context("release gate derivation requires exact regression_passed state")?;
            let evidence = self.object(text(
                &event["payload"],
                "regression_evidence_object_sha256",
            )?)?;
            gate(&evidence)?;
            self.object(text(&evidence, "provenance_closure_object_sha256")?)?
        };
        closure(&prior)?;
        ensure!(
            prior["phase"]
                == if gate_kind == "regression" {
                    "execution"
                } else {
                    "regression"
                },
            "refinery_prior_closure_phase"
        );
        let prior_digest = prior["closure_sha256"].clone();
        let mut contexts = values(&prior, "transition_contexts")?.clone();
        if gate_kind == "release" {
            let e = events
                .iter()
                .find(|e| e["state"] == "regression_passed")
                .unwrap();
            contexts.push(json!({"action":"regression_passed","context_sha256":e["payload"]["transition_context_sha256"],"context_object_sha256":e["payload"]["transition_context_object_sha256"]}));
        }
        contexts.push(json!({"action":context["action"],"context_sha256":context["context_sha256"],"context_object_sha256":context_object}));
        contexts.sort_by_key(|v| {
            CONTEXT_ORDER
                .iter()
                .position(|s| Some(*s) == v["action"].as_str())
                .unwrap_or(usize::MAX)
        });
        prior["phase"] = json!(gate_kind);
        prior["transition_contexts"] = json!(contexts);
        prior["prior_closure_sha256"] = prior_digest;
        prior.as_object_mut().unwrap().remove("closure_sha256");
        prior["closure_sha256"] = json!(hash(&prior)?);
        closure(&prior)?;
        ensure!(
            prior["delta_sha256"] == docs["delta"]["delta_sha256"],
            "refinery_gate_closure_delta"
        );
        let sha = self.retain(&prior, persist)?;
        Ok((prior, sha))
    }
    pub fn derive_gate(
        &self,
        delta: &str,
        gate_kind: &str,
        suite_digest: &str,
        context: &Value,
        when: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(when))?;
        let (_, docs, events) = self.load(delta)?;
        if gate_kind == "release" {
            ensure!(
                events
                    .last()
                    .is_some_and(|e| e["state"] == "regression_passed"),
                "release gate derivation requires exact regression_passed state"
            );
        }
        self.derive_gate_with(&docs, &events, gate_kind, suite_digest, context, when, true)
    }
    pub(crate) fn derive_gate_with(
        &self,
        docs: &Value,
        events: &[Value],
        gate_kind: &str,
        suite_digest: &str,
        context: &Value,
        when: &str,
        persist: bool,
    ) -> Result<Value> {
        let action = match gate_kind {
            "regression" => "derive_regression_gate",
            "release" => "derive_release_gate",
            _ => bail!("gate must be regression or release"),
        };
        self.context_checked(action, docs, context)?;
        let context_object = self.retain(context, persist)?;
        let (suite, suite_object) =
            self.suite_index(text(&docs["delta"], "delta_sha256")?, suite_digest)?;
        let (manifest, projection, execution_closure) =
            self.verify_suite_with(&suite, docs, events)?;
        let mut regression = json!({});
        for id in algorithms::REGRESSION_REQUIRED_CHECK_IDS {
            regression[id] =
                algorithms::regression(id, &projection["gate_suite"][id], &suite, &manifest)?;
        }
        let evaluations = if gate_kind == "regression" {
            algorithms::REGRESSION_REQUIRED_CHECK_IDS
                .iter()
                .map(|id| regression[*id].clone())
                .collect::<Vec<_>>()
        } else {
            let evidence = json!({"bindings":self.provenance_assets(&docs["delta"],&manifest,&projection)?,"scenarios":self.closure_scenarios(values(&suite,"runs")?)?});
            algorithms::release(
                &docs["delta"],
                &suite,
                &manifest,
                &projection,
                &regression,
                &evidence,
                &execution_closure,
            )?
        };
        let (closure_value, closure_object) = self.extended_closure(
            docs,
            events,
            &suite,
            gate_kind,
            context,
            &context_object,
            persist,
        )?;
        let mut checks = Vec::new();
        for evaluation in evaluations {
            let output = json!({"schema_version":"1.0","gate":gate_kind,"check_id":evaluation["check_id"],"delta_sha256":docs["delta"]["delta_sha256"],"package_sha256":manifest["package_sha256"],"execution_suite_sha256":suite["suite_sha256"],"transition_context_sha256":context["context_sha256"],"provenance_closure_sha256":closure_value["closure_sha256"],"passed":evaluation["passed"],"message":evaluation["message"],"details":evaluation["details"]});
            let sha = self.retain(&output, persist)?;
            checks.push(json!({"check_id":evaluation["check_id"],"passed":evaluation["passed"],"message":evaluation["message"],"output_sha256":sha}));
        }
        let required = texts(&docs["delta"]["required_capabilities"], false)?;
        let observed = texts(&suite["observed_capabilities"], true)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let missing = required
            .iter()
            .filter(|id| !observed.contains(*id))
            .cloned()
            .collect::<BTreeSet<_>>();
        let complete = runtime_source(&suite["runtime_source"])?
            && missing.is_empty()
            && checks.iter().all(|c| c["passed"] == true);
        let mut value = keyed_sha(
            json!({"gate":gate_kind,"package_sha256":manifest["package_sha256"],"execution_suite_sha256":suite["suite_sha256"],"execution_suite_object_sha256":suite_object,"transition_context_sha256":context["context_sha256"],"transition_context_object_sha256":context_object,"provenance_closure_sha256":closure_value["closure_sha256"],"provenance_closure_object_sha256":closure_object,"runtime_source":suite["runtime_source"],"required_capabilities":required,"observed_capabilities":observed,"checks":checks,"status":if complete{"complete"}else{"blocked"},"recorded_at":when}),
            "evidence_sha256",
        )?;
        value["missing_capabilities"] = json!(missing);
        gate(&value)?;
        Ok(value)
    }
    pub fn record_gate(
        &self,
        delta: &str,
        evidence: &Value,
        context: &Value,
        when: &str,
    ) -> Result<Value> {
        ensure!(
            evidence["runtime_source"].get("schema").is_none(),
            "refinery_current_gate_requires_actual_execution"
        );
        self.record_gate_impl(delta, evidence, context, when)
    }
    pub(crate) fn record_gate_impl(
        &self,
        delta: &str,
        evidence: &Value,
        context: &Value,
        when: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(when))?;
        gate(evidence)?;
        let (state, docs, events) = self.load(delta)?;
        let kind = text(evidence, "gate")?;
        let (action, prior) = if kind == "regression" {
            ("regression_passed", "committed")
        } else {
            ("release_complete", "regression_passed")
        };
        ensure!(state["state"] == prior, "refinery_record_gate_wrong_state");
        self.context_checked(action, &docs, context)?;
        ensure!(
            includes(&docs["binding"], "allowed_actions", action),
            "refinery_gate_action_not_allowed"
        );
        ensure!(
            evidence["status"] == "complete",
            "refinery_gate_evidence_blocked"
        );
        ensure!(
            evidence["runtime_source"] == docs["engagement"]["runtime_source"]
                && evidence["required_capabilities"] == docs["delta"]["required_capabilities"],
            "refinery_gate_engagement_or_capabilities_mismatch"
        );
        let derive_context = self.object(text(evidence, "transition_context_object_sha256")?)?;
        let expected = self.derive_gate_with(
            &docs,
            &events,
            kind,
            text(evidence, "execution_suite_sha256")?,
            &derive_context,
            text(evidence, "recorded_at")?,
            false,
        )?;
        ensure!(
            *evidence == expected,
            "regression evidence is not the exact Semantica-derived gate output"
        );
        if kind == "release" {
            let regression = events.last().context("refinery_empty_history")?;
            ensure!(
                regression["payload"]["execution_suite_sha256"]
                    == evidence["execution_suite_sha256"]
                    && regression["payload"]["execution_suite_object_sha256"]
                        == evidence["execution_suite_object_sha256"],
                "release evidence suite differs from regression suite"
            );
        }
        let context_object = self.retain(context, true)?;
        let evidence_object = self.retain(evidence, true)?;
        let mut payload = json!({"execution_suite_sha256":evidence["execution_suite_sha256"],"execution_suite_object_sha256":evidence["execution_suite_object_sha256"],"transition_context_sha256":context["context_sha256"],"transition_context_object_sha256":context_object,"provenance_closure_sha256":evidence["provenance_closure_sha256"],"provenance_closure_object_sha256":evidence["provenance_closure_object_sha256"]});
        payload[format!("{kind}_evidence_sha256")] = evidence["evidence_sha256"].clone();
        payload[format!("{kind}_evidence_object_sha256")] = json!(evidence_object);
        self.append(
            events.last().context("refinery_empty_history")?,
            action,
            Some(text(evidence, "package_sha256")?),
            when,
            payload,
        )
    }
    pub(crate) fn verify_lifecycle_objects(
        &self,
        _state: &Value,
        docs: &Value,
        events: &[Value],
    ) -> Result<()> {
        for (i, event) in events.iter().enumerate().skip(3) {
            if i > 4 {
                self.verify_promotion_event(docs, events, event)?;
                continue;
            }
            let kind = if i == 3 { "regression" } else { "release" };
            let payload = &event["payload"];
            let evidence =
                self.object(text(payload, &format!("{kind}_evidence_object_sha256"))?)?;
            gate(&evidence)?;
            ensure!(
                evidence["evidence_sha256"] == payload[format!("{kind}_evidence_sha256")]
                    && evidence["status"] == "complete"
                    && evidence["runtime_source"] == docs["engagement"]["runtime_source"],
                "refinery_recorded_gate_identity"
            );
            for key in [
                "execution_suite_sha256",
                "execution_suite_object_sha256",
                "provenance_closure_sha256",
                "provenance_closure_object_sha256",
            ] {
                ensure!(
                    evidence[key] == payload[key],
                    "refinery_event_gate_binding:{key}"
                );
            }
            let derive_context =
                self.object(text(&evidence, "transition_context_object_sha256")?)?;
            let expected = self.derive_gate_with(
                docs,
                &events[..i],
                kind,
                text(&evidence, "execution_suite_sha256")?,
                &derive_context,
                text(&evidence, "recorded_at")?,
                false,
            )?;
            ensure!(
                evidence == expected,
                "refinery_recorded_gate_full_replay_mismatch"
            );
            if i == 4 {
                ensure!(
                    payload["execution_suite_sha256"]
                        == events[3]["payload"]["execution_suite_sha256"]
                        && payload["execution_suite_object_sha256"]
                            == events[3]["payload"]["execution_suite_object_sha256"],
                    "refinery_release_regression_suite_changed"
                );
            }
        }
        Ok(())
    }
}
pub(crate) struct CasProvider<'a, 'b> {
    pub(crate) engine: &'a RefineryEngine<'b>,
}
impl crate::books::SourceProvider for CasProvider<'_, '_> {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        crate::books::relative(id)?;
        let sha = id
            .strip_prefix("objects/sha256/")
            .and_then(|s| s.split_once('/'))
            .context("refinery_provider_source_scope")?;
        valid_sha(sha.1)?;
        ensure!(sha.0 == &sha.1[..2], "refinery_provider_digest_path");
        self.engine.raw_asset(&json!({"sha256":sha.1}))
    }
}

fn verify_run(
    suite: &Value,
    run: &Value,
    result: &Value,
    receipt: &Value,
    current: Option<&Value>,
) -> Result<()> {
    let fields = [
        "schema_version",
        "runner_contract",
        "package_id",
        "package_version",
        "package_digest",
        "scenario_id",
        "status",
        "created_at",
        "operations",
        "oracle_checks",
        "capability_report",
        "cq_report",
        "shacl_report",
        "oracle_report",
        "receipt",
        "release_verdict",
        "reasons",
    ];
    keys(
        result,
        &fields,
        if current.is_some() {
            &["execution_profile"]
        } else {
            &[]
        },
    )?;
    ensure!(
        result["schema_version"] == "1.0"
            && result["runner_contract"] == crate::book_runner::RUNNER_CONTRACT,
        "refinery_run_contract"
    );
    for (result_key, run_key) in [
        ("package_id", "executor_package_id"),
        ("package_version", "executor_package_version"),
        ("package_digest", "executor_package_digest"),
        ("scenario_id", "scenario_id"),
        ("status", "status"),
    ] {
        ensure!(
            result[result_key] == run[run_key],
            "refinery_run_identity:{result_key}"
        );
    }
    ensure!(
        result["created_at"] == suite["created_at"],
        "refinery_run_timestamp"
    );
    let mut outputs = json!({});
    for op in values(result, "operations")? {
        keys(
            op,
            &["operation_id", "operation", "status", "payload", "sha256"],
            &[],
        )?;
        nonempty(&op["operation"])?;
        let id = nonempty(&op["operation_id"])?;
        ensure!(
            ["passed", "failed", "blocked"].contains(&text(op, "status")?)
                && op["payload"].is_object(),
            "refinery_operation_fields"
        );
        content_hash(op, "sha256", &[])?;
        ensure!(outputs.get(id).is_none(), "refinery_operation_id_duplicate");
        outputs[id] = op["sha256"].clone();
    }
    let checks = values(result, "oracle_checks")?;
    ensure!(!checks.is_empty(), "refinery_run_oracle_checks_missing");
    for check in checks {
        keys(
            check,
            &["check_id", "status", "expected", "actual", "message"],
            &[],
        )?;
        ensure!(
            ["passed", "failed", "blocked"].contains(&text(check, "status")?),
            "refinery_run_oracle_status"
        );
    }
    for (field, kind) in [
        ("capability_report", "capability"),
        ("cq_report", "cq"),
        ("shacl_report", "shacl"),
        ("oracle_report", "oracle"),
    ] {
        report(&result[field], kind)?;
        report(&receipt[field], kind)?;
        if !(current.is_some() && kind == "capability") {
            ensure!(
                result[field] == receipt[field],
                "refinery_result_receipt_report:{field}"
            );
        }
    }
    ensure!(
        crate::execution_provenance::verify_receipt(receipt)?
            && crate::execution_provenance::verify_bundle(&receipt["provenance_bundle"])?
            && crate::execution_provenance::verify_receipt_bindings(receipt)?,
        "refinery_full_native_receipt_integrity"
    );
    ensure!(
        receipt["receipt_sha256"] == run["receipt_sha256"]
            && receipt["asset_hashes"] == suite["execution_asset_hashes"]
            && receipt["output_hashes"] == outputs
            && receipt["chapter_contract_sha256"] == suite["execution_projection_sha256"],
        "refinery_native_receipt_assets_outputs_contract"
    );
    for (receipt_key, run_key) in [
        ("package_id", "executor_package_id"),
        ("package_version", "executor_package_version"),
        ("package_digest", "executor_package_digest"),
    ] {
        ensure!(
            receipt[receipt_key] == run[run_key],
            "refinery_native_receipt_package:{receipt_key}"
        );
    }
    let verdict = if let Some(observation) = current {
        keys(
            observation,
            &[
                "profile",
                "source_run_receipt_sha256",
                "receipt",
                "content_verdict",
                "runtime_source",
                "snapshot_profile",
                "runner_execution",
                "runtime_origin",
                "source_authenticity",
                "engineering_adoption",
                "formal_release",
            ],
            &[],
        )?;
        ensure!(
            observation["profile"] == crate::book_runner::RELEASE_CONTENT_PROFILE
                && observation["runtime_source"] == suite["runtime_source"]
                && observation["receipt"] == *receipt
                && observation["source_run_receipt_sha256"] == result["receipt"]["receipt_sha256"],
            "refinery_current_snapshot_binding"
        );
        ensure!(
            observation["runner_execution"] == "retained"
                && observation["runtime_origin"] == "unknown"
                && observation["source_authenticity"] == "unknown"
                && observation["engineering_adoption"] == "unknown"
                && observation["formal_release"] == "not_published",
            "refinery_current_snapshot_scope"
        );
        content_hash(&result["receipt"], "receipt_sha256", &[])?;
        ensure!(
            result["execution_profile"] == crate::book_runner::PROFILE
                && result["receipt"]["receipt_profile"] == crate::book_runner::PROFILE,
            "refinery_current_actual_result_profile"
        );
        for key in [
            "package_id",
            "package_version",
            "package_digest",
            "asset_hashes",
            "output_hashes",
            "dataset_sha256",
            "dataset_quad_count",
            "dataset_revision",
            "runtime_version",
            "runtime_commit",
            "runtime_artifact_sha256",
            "created_at",
        ] {
            ensure!(
                result["receipt"][key] == receipt[key],
                "refinery_current_original_receipt_binding:{key}"
            );
        }
        let source = &suite["runtime_source"];
        ensure!(
            source.get("schema").is_some()
                && runtime_source(source)?
                && receipt["runtime_commit"]
                    == format!("semantica.rust-source/{}", text(source, "source_sha256")?)
                && receipt["runtime_artifact_sha256"] == source["executable_sha256"],
            "refinery_current_source_receipt_binding"
        );
        &observation["content_verdict"]
    } else {
        ensure!(
            result["receipt"] == *receipt,
            "refinery_result_native_receipt_mismatch"
        );
        let source = &suite["runtime_source"];
        wire::runtime_source(source)?;
        for field in [
            "runtime_version",
            "runtime_commit",
            "runtime_artifact_sha256",
        ] {
            ensure!(
                receipt[field] == source[field],
                "refinery_legacy_runtime_identity:{field}"
            );
        }
        &result["release_verdict"]
    };
    keys(
        verdict,
        &[
            "schema_version",
            "status",
            "receipt_sha256",
            "checked_at",
            "checks",
            "reasons",
        ],
        if current.is_some() { &["profile"] } else { &[] },
    )?;
    ensure!(
        verdict["schema_version"] == "1.0"
            && verdict["receipt_sha256"] == run["receipt_sha256"]
            && verdict["status"] == run["release_status"],
        "refinery_release_verdict_identity"
    );
    let checks = values(verdict, "checks")?;
    ensure!(!checks.is_empty(), "refinery_release_checks_empty");
    let mut failed = Vec::new();
    for check in checks {
        keys(check, &["check_id", "passed", "message"], &[])?;
        nonempty(&check["check_id"])?;
        ensure!(
            check["passed"].is_boolean() && check["message"].is_string(),
            "refinery_release_check_fields"
        );
        if check["passed"] == false {
            failed.push(check["check_id"].clone());
        }
    }
    ensure!(
        verdict["status"]
            == if failed.is_empty() {
                "complete"
            } else {
                "blocked"
            }
            && verdict["reasons"] == json!(failed),
        "refinery_release_derived_verdict"
    );
    ensure!(
        actual_cqs(result) == run["cq_ids"],
        "refinery_run_actual_cqs"
    );
    let loaded = loaded_assets(result)?;
    for case in values(run, "case_assets")? {
        ensure!(
            loaded.get(text(case, "asset_id")?) == Some(&case["sha256"]),
            "refinery_case_not_actually_loaded"
        );
    }
    Ok(())
}

/// An actual run produced by this module. A serialized snapshot cannot create
/// this capability; restart requires real current execution before new writes.
pub struct CurrentSubjectExecution<'a> {
    owner: &'a dyn refinery::RefineryStore,
    suite: Value,
    registry_id: String,
}
impl CurrentSubjectExecution<'_> {
    pub fn observation(&self) -> &Value {
        &self.suite
    }
}
impl RefineryEngine<'_> {
    pub fn execute_current(
        &self,
        delta: &str,
        context: &Value,
        source: &Value,
        when: &str,
    ) -> Result<CurrentSubjectExecution<'_>> {
        let suite = self.execute_candidate_current(delta, context, source, when)?;
        Ok(CurrentSubjectExecution {
            owner: self.store,
            suite,
            registry_id: self.registry_id.clone(),
        })
    }
    pub fn record_current_gate(
        &self,
        proof: &CurrentSubjectExecution<'_>,
        evidence: &Value,
        context: &Value,
        when: &str,
    ) -> Result<Value> {
        ensure!(
            std::ptr::eq(proof.owner, self.store)
                && proof.registry_id == self.registry_id
                && evidence["execution_suite_sha256"] == proof.suite["suite_sha256"]
                && evidence["runtime_source"] == proof.suite["runtime_source"],
            "refinery_actual_execution_capability_binding"
        );
        self.verify_suite_content(&proof.suite)?;
        self.record_gate_impl(text(&proof.suite, "delta_sha256")?, evidence, context, when)
    }
    /// Re-run current subject bytes in fresh runner state after restart. Historical
    /// JSON remains content evidence; it is never deserialized into a capability.
    pub fn replay_current_execution(
        &self,
        delta: &str,
        suite_digest: &str,
        source: &Value,
    ) -> Result<CurrentSubjectExecution<'_>> {
        let (saved, _) = self.suite_index(delta, suite_digest)?;
        ensure!(
            saved["runtime_source"] == *source && source.get("schema").is_some(),
            "refinery_current_replay_source"
        );
        self.verify_suite_content(&saved)?;
        // A promoted exact retry preserves the original execute_candidate state
        // domain. Restore its private proof with real read-only native reruns.
        let (state, _, events) = self.load(delta)?;
        if state["state"] == "promoted" {
            let commit = events
                .iter()
                .find(|e| e["state"] == "committed")
                .context("refinery_commit_missing")?;
            let manifest = self.object(text(&commit["payload"], "manifest_sha256")?)?;
            let (execution, _) = self.execution_projection(&manifest)?;
            let provider = CasProvider { engine: self };
            let identity = source_identity(source, text(&saved, "created_at")?)?;
            for run in values(&saved, "runs")? {
                let actual = crate::refinery_runner::run_current(
                    &provider,
                    &execution,
                    text(run, "scenario_id")?,
                    &identity,
                    source,
                )?;
                ensure!(
                    actual == self.object(text(run, "run_result_object_sha256")?)?,
                    "refinery_promoted_actual_restart_execution_differs"
                );
            }
            return Ok(CurrentSubjectExecution {
                owner: self.store,
                suite: saved,
                registry_id: self.registry_id.clone(),
            });
        }
        let context = self.object(text(&saved, "transition_context_object_sha256")?)?;
        let proof = self.execute_current(delta, &context, source, text(&saved, "created_at")?)?;
        ensure!(
            proof.suite == saved,
            "refinery_current_actual_restart_execution_differs"
        );
        Ok(proof)
    }
}

impl RefineryEngine<'_> {
    /// Original ordered verification facade over fully replayed content. This
    /// data API cannot establish an actual current execution capability.
    pub fn verify_candidate_data(
        &self,
        delta: &str,
        suite_digest: &str,
        regression: Option<&Value>,
        regression_context: Option<&Value>,
        release_derivation: &Value,
        release_context: &Value,
        when: &str,
    ) -> Result<Value> {
        let (_, docs, _) = self.load(delta)?;
        ensure!(
            docs["engagement"]["runtime_source"].get("schema").is_none(),
            "refinery_current_verification_requires_actual_execution"
        );
        self.verify_candidate_impl(
            delta,
            suite_digest,
            regression,
            regression_context,
            release_derivation,
            release_context,
            when,
        )
    }
    pub(crate) fn verify_candidate_impl(
        &self,
        delta: &str,
        suite_digest: &str,
        regression_evidence: Option<&Value>,
        regression_context: Option<&Value>,
        release_derivation_context: &Value,
        release_context: &Value,
        when: &str,
    ) -> Result<Value> {
        wire::timestamp(&json!(when))?;
        let mut state = self.status(delta)?;
        let regression = if state["state"] == "committed" {
            let evidence = regression_evidence
                .context("committed verification requires regression evidence and context")?;
            let context = regression_context
                .context("committed verification requires regression evidence and context")?;
            ensure!(
                evidence["execution_suite_sha256"] == suite_digest,
                "regression evidence differs from requested execution suite"
            );
            state = self.record_gate_impl(delta, evidence, context, when)?;
            evidence.clone()
        } else {
            ensure!(
                ["regression_passed", "release_complete"].contains(&text(&state, "state")?),
                "verify_candidate requires committed, regression_passed, or release_complete state"
            );
            ensure!(
                regression_evidence.is_none() && regression_context.is_none(),
                "verification restart must load recorded regression evidence"
            );
            let events = self.history(delta)?;
            let event = events
                .iter()
                .find(|e| e["state"] == "regression_passed")
                .context("refinery_regression_event_missing")?;
            let evidence = self.object(text(
                &event["payload"],
                "regression_evidence_object_sha256",
            )?)?;
            gate(&evidence)?;
            ensure!(
                evidence["evidence_sha256"] == event["payload"]["regression_evidence_sha256"]
                    && evidence["execution_suite_sha256"] == suite_digest,
                "retry regression evidence differs from recorded gate"
            );
            evidence
        };
        let release = if state["state"] == "release_complete" {
            let (_, docs, events) = self.load(delta)?;
            let event = events
                .iter()
                .find(|e| e["state"] == "release_complete")
                .context("refinery_release_event_missing")?;
            let evidence =
                self.object(text(&event["payload"], "release_evidence_object_sha256")?)?;
            ensure!(
                evidence["execution_suite_sha256"] == suite_digest,
                "retry release execution suite differs"
            );
            self.context_checked("derive_release_gate", &docs, release_derivation_context)?;
            self.context_checked("release_complete", &docs, release_context)?;
            let saved_derive = self.object(text(&evidence, "transition_context_object_sha256")?)?;
            let saved_record =
                self.object(text(&event["payload"], "transition_context_object_sha256")?)?;
            ensure!(
                *release_derivation_context == saved_derive && *release_context == saved_record,
                "retry release transition context differs from recorded gate"
            );
            evidence
        } else {
            let evidence = self.derive_gate(
                delta,
                "release",
                suite_digest,
                release_derivation_context,
                when,
            )?;
            state = self.record_gate_impl(delta, &evidence, release_context, when)?;
            evidence
        };
        ensure!(
            state["state"] == "release_complete"
                && regression["gate"] == "regression"
                && release["gate"] == "release"
                && regression["package_sha256"] == state["package_sha256"]
                && release["package_sha256"] == state["package_sha256"],
            "refinery_candidate_verification_binding"
        );
        let value = keyed_sha(
            json!({"schema_version":"1.0","state":state,"execution_suite_sha256":suite_digest,"regression_evidence":regression,"release_evidence":release}),
            "verification_sha256",
        )?;
        if value["regression_evidence"]["runtime_source"]
            .get("schema")
            .is_some()
        {
            let typed: semantica_contracts::SemanticRefineryRustCandidateVerification =
                serde_json::from_value(value.clone())?;
            typed
                .validate()
                .map_err(|e| anyhow::anyhow!("refinery_current_verification_contract:{e:?}"))?;
        }
        Ok(value)
    }
    pub fn verify_candidate_current(
        &self,
        proof: &CurrentSubjectExecution<'_>,
        regression_evidence: Option<&Value>,
        regression_context: Option<&Value>,
        release_derivation_context: &Value,
        release_context: &Value,
        when: &str,
    ) -> Result<Value> {
        ensure!(
            std::ptr::eq(proof.owner, self.store) && proof.registry_id == self.registry_id,
            "refinery_foreign_execution_owner"
        );
        self.verify_suite_content(&proof.suite)?;
        self.verify_candidate_impl(
            text(&proof.suite, "delta_sha256")?,
            text(&proof.suite, "suite_sha256")?,
            regression_evidence,
            regression_context,
            release_derivation_context,
            release_context,
            when,
        )
    }
    pub fn promote_current(
        &self,
        proof: &CurrentSubjectExecution<'_>,
        authorization: &Value,
        context: &Value,
        when: &str,
    ) -> Result<Value> {
        ensure!(
            std::ptr::eq(proof.owner, self.store) && proof.registry_id == self.registry_id,
            "refinery_foreign_execution_owner"
        );
        self.verify_suite_content(&proof.suite)?;
        let events = self.history(text(&proof.suite, "delta_sha256")?)?;
        let release = events
            .iter()
            .find(|e| e["state"] == "release_complete")
            .context("refinery_actual_release_missing")?;
        ensure!(
            release["payload"]["execution_suite_sha256"] == proof.suite["suite_sha256"],
            "refinery_promotion_actual_suite_binding"
        );
        self.promote_impl(
            text(&proof.suite, "delta_sha256")?,
            authorization,
            context,
            when,
        )
    }
    /// Execute a promoted version using the same current Catalog/Dataset/runner
    /// algorithms. Rerunning does not mutate registry/history or adopt truth.
    pub fn run_registry_current(
        &self,
        package_id: &str,
        version: Option<&str>,
        scenario: &str,
        source: &Value,
        when: &str,
    ) -> Result<Value> {
        self.run_registry_current_optional(package_id, version, Some(scenario), source, when)
    }
    pub fn run_registry_current_optional(
        &self,
        package_id: &str,
        version: Option<&str>,
        scenario: Option<&str>,
        source: &Value,
        when: &str,
    ) -> Result<Value> {
        let descriptor = self.resolve_package(package_id, version)?;
        self.verify_published_refinement(text(&descriptor, "delta_sha256")?)?;
        let manifest = self.object(text(&descriptor, "manifest_sha256")?)?;
        let (execution, _) = self.execution_projection(&manifest)?;
        let identity = source_identity(source, when)?;
        crate::refinery_runner::run_current_optional(
            &CasProvider { engine: self },
            &execution,
            scenario,
            &identity,
            source,
        )
    }
}
