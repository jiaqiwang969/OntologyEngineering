//! Offline orchestration fixtures. They deliberately do not claim semantics.
use anyhow::{Context, Result, bail, ensure};
use oe_core::json::strict_json;
use oe_core::sha256;
use oe_semantic::bootstrap::{self, BootstrapExecutor, Invocation, PrepareOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let path = root().join(format!(
            "var/maintenance/rust-tests/bootstrap-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        Self(path)
    }
    fn plan(&self) -> PathBuf {
        self.0.join("recipient")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn read(path: &Path) -> Value {
    strict_json(&fs::read(path).unwrap()).unwrap()
}
fn save(path: &Path, value: &Value) {
    fs::write(path, bootstrap::encoded(value).unwrap()).unwrap();
}
fn read_checks_fixture(step_dir: &Path) -> Value {
    let index = read(&step_dir.join("authoring-checks.json"));
    json!(
        index["parts"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|part| read(&step_dir.join(part["path"].as_str().unwrap()))
                .as_array()
                .unwrap()
                .clone())
            .collect::<Vec<_>>()
    )
}
fn rewrite_checks_fixture(step_dir: &Path, checks: &Value) {
    let checks = checks.as_array().unwrap();
    let mut parts = Vec::new();
    for (ordinal, chunk) in checks
        .chunks(bootstrap::AUTHORING_BATCH_SCENARIOS)
        .enumerate()
    {
        let name = format!("authoring-checks/part-{ordinal:04}.json");
        let bytes = bootstrap::encoded(&json!(chunk)).unwrap();
        fs::write(step_dir.join(&name), &bytes).unwrap();
        parts.push(json!({"path":name,"sha256":sha256(bytes),"count":chunk.len(),"start":ordinal*bootstrap::AUTHORING_BATCH_SCENARIOS}));
    }
    let ids = checks
        .iter()
        .map(|c| c["execution"]["scenario_id"].clone())
        .collect::<Vec<_>>();
    save(
        &step_dir.join("authoring-checks.json"),
        &json!({"schema":"ontology-engineering.authoring-checks-index/v1","parts":parts,"scenario_count":checks.len(),"scenario_ids_sha256":sha256(oe_core::json::canonical_json(&json!(ids)).unwrap()),"checks_sha256":sha256(bootstrap::encoded(&json!(checks)).unwrap())}),
    );
}
fn options(name: &str) -> PrepareOptions {
    PrepareOptions {
        project: "bootstrap-fixture".into(),
        domain: "synthetic-method-tests".into(),
        actor: "fixture-local-operator".into(),
        fact_authority: "fixture-controlled-records".into(),
        evidence_root: "evidence:fixture-project".into(),
        name: name.into(),
    }
}
fn authorize(directory: &Path, action: &str) -> PathBuf {
    let plan = read(&directory.join("plan.json"));
    let mut value = json!({"schema":"ontology-engineering.method-bootstrap-authorization/v1","action":action,"actor_id":plan["actor"],"plan_sha256":sha256(bootstrap::encoded(&plan).unwrap()),"reason":"Explicit isolated test of bookkeeping; no semantic or physical approval.","issued_at":"2026-09-27T00:00:00Z"});
    if action == "adopt-project-binding" {
        value["binding_sha256"] = json!(sha256(
            fs::read(directory.join("proposed-project-binding.json")).unwrap()
        ));
    }
    let path = directory.join(format!("request-{action}.json"));
    save(&path, &value);
    path
}

#[derive(Default)]
struct FixtureExecutor {
    events: BTreeMap<String, Vec<Value>>,
    successors: BTreeMap<String, Value>,
    interrupt: Option<String>,
    scenarios: usize,
    calls: Vec<String>,
    reject_authoring: bool,
    open_failure: bool,
    mutate_before_discovery: Option<PathBuf>,
    mutate_during_discovery: bool,
    substituted_discovery_hash: bool,
}
impl FixtureExecutor {
    fn record(&mut self, state: &str, request: &Invocation<'_>) -> Result<()> {
        let event = json!({"state":state,"fixture_task_sha256":sha256(fs::read(request.task)?),"fixture_authorization_sha256":request.authorization.map(|p|fs::read(p).map(sha256)).transpose()?});
        let events = self.events.entry(request.candidate.into()).or_default();
        if let Some(old) = events.iter().find(|v| v["state"] == state) {
            ensure!(*old == event, "fixture_request_changed");
        } else {
            events.push(event);
        }
        Ok(())
    }
}
impl BootstrapExecutor for FixtureExecutor {
    fn verify_runtime(&mut self) -> Result<()> {
        self.calls.push("fixture_verify_runtime".into());
        if let Some(path) = self.mutate_before_discovery.take() {
            let mut value = read(&path);
            value["binding_id"] = json!("fixture-mutated-before-discovery");
            save(&path, &value);
        }
        Ok(())
    }
    fn discover(&mut self, binding: Option<&Path>, _: Option<&Path>) -> Result<Value> {
        self.calls.push("fixture_discover".into());
        if self.mutate_during_discovery
            && let Some(path) = binding
        {
            let mut value = read(path);
            value["binding_id"] = json!("fixture-mutated-during-discovery");
            save(path, &value);
        }
        Ok(if let Some(path) = binding {
            let hash = if self.substituted_discovery_hash {
                "0".repeat(64)
            } else {
                sha256(fs::read(path)?)
            };
            json!({"command_verdict":"passed","fixture_only":true,"binding":{"source_sha256":hash}})
        } else {
            json!({"command_verdict":"passed","corpus_found":{"native_workspace_bootstrap":{"baseline_version":"0","baseline_digest":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947"}}})
        })
    }
    fn invoke(&mut self, action: &str, request: &Invocation<'_>) -> Result<Value> {
        self.calls.push(action.into());
        let states: &[&str] = match action {
            "open" => &[],
            "propose" => &["candidate", "proposed"],
            "commit" => &["committed"],
            "verify" => &["regression_passed", "release_complete"],
            "promote" => &["promoted"],
            _ => bail!("fixture_unknown_action"),
        };
        for state in states {
            self.record(state, request)?;
        }
        let mut result = json!({"command_verdict":"passed","fixture_only":true,"learning":{"current_state":states.last()}});
        if action == "open" {
            result["command_verdict"] = json!("blocked");
            result["native_execution"] =
                json!({"semantic_execution":if self.open_failure {"not_run"} else {"completed"}});
            result["execution"] = json!({"status":"blocked","operation":"native_open_engagement","mutation_performed":!self.open_failure});
            result["receipt"] =
                json!({"status":"verified","receipt_sha256":sha256(b"fixture-open-receipt")});
        }
        if action == "propose" {
            let delta = read(request.delta.context("fixture_delta")?);
            let mut successor = read(request.binding);
            let plan = read(&request.workspace.parent().unwrap().join("plan.json"));
            let capsule = bootstrap::load_capsule(&root(), plan["bundle"].as_str().unwrap())?;
            let step = capsule.capsule["steps"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["delta_sha256"] == request.candidate)
                .unwrap();
            successor["baseline"] =
                json!({"version":delta["target_version"],"digest":step["package_sha256"]});
            successor["binding_id"] = json!(format!(
                "fixture-successor-{}",
                delta["target_version"].as_str().unwrap()
            ));
            self.successors.insert(request.candidate.into(), successor);
        }
        if action == "promote" {
            result["learning"]["next_binding"] = json!({"auto_applied":false,"document":self.successors.get(request.candidate).context("fixture_missing_successor")?});
        }
        if self.interrupt.as_deref() == Some(action) {
            self.interrupt = None;
            bail!("injected_after_fixture_{action}");
        }
        Ok(result)
    }
    fn history(&mut self, _: &Path, _: &Path, candidate: &str) -> Result<Value> {
        self.calls.push("fixture_history".into());
        Ok(
            json!({"command_verdict":"passed","execution":{"events":self.events.get(candidate).context("fixture_no_candidate")?}}),
        )
    }
    fn check_events(&mut self, _: &Path, step_dir: &Path, events: &[Value], _: &str) -> Result<()> {
        for event in events {
            let command = match event["state"].as_str().unwrap() {
                "candidate" | "proposed" => "propose",
                "committed" => "commit",
                "regression_passed" | "release_complete" => "verify",
                "promoted" => "promote",
                _ => bail!("fixture_bad_state"),
            };
            ensure!(
                event["fixture_task_sha256"]
                    == sha256(fs::read(step_dir.join(format!("task-{command}.json")))?),
                "fixture_retained_task_differs_from_native_event"
            );
            if matches!(command, "commit" | "promote") {
                ensure!(
                    event["fixture_authorization_sha256"]
                        == sha256(fs::read(
                            step_dir.join(format!("authorization-{command}.json"))
                        )?),
                    "fixture_retained_authorization_differs"
                );
            }
        }
        Ok(())
    }
    fn authoring_checks(&mut self, manifest: &Path, scenarios: &[String]) -> Result<Vec<Value>> {
        let manifest = read(manifest);
        self.scenarios += scenarios.len();
        let lock = read(&root().join("runtime/semantic-bundles.json"));
        let spec = lock["bundles"]
            .as_object()
            .unwrap()
            .values()
            .find(|s| s["package_id"] == manifest["package_id"])
            .unwrap();
        let hashes: serde_json::Map<String, Value> = manifest["assets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                (
                    a["asset_id"].as_str().unwrap().to_owned(),
                    a["sha256"].clone(),
                )
            })
            .collect();
        Ok(scenarios.iter().map(|id| {
            let digest=sha256(b"fixture-package-identity");let receipt_sha=sha256(id);
            json!({"execution":{"runner_contract":"semantica.chapter_packages.SemanticPackageRunner/v1","status":if self.reject_authoring {"failed"} else {"passed"},"scenario_id":id,"package_id":manifest["package_id"],"package_version":manifest["version"],"package_digest":digest,"receipt":{"receipt_sha256":receipt_sha,"package_id":manifest["package_id"],"package_version":manifest["version"],"package_digest":digest,"asset_hashes":hashes,"runtime_commit":spec["runtime"]["commit"],"runtime_version":spec["runtime"]["version"],"runtime_artifact_sha256":spec["runtime"]["wheel_sha256"]},"release_verdict":{"status":"complete","receipt_sha256":receipt_sha}},"verification":{"status":"complete","receipt_sha256":receipt_sha},"fixture_only":true})
        }).collect())
    }
}

#[test]
fn locked_capsules_preserve_all_versions_scenarios_and_no_decisions() {
    for (name, versions, total) in [
        (
            "engineering-judgment-intake",
            vec!["0.1.0", "0.1.2", "0.2.0", "0.2.1", "0.2.2"],
            356,
        ),
        ("engineering-evidence-methods", vec!["0.1.0"], 265),
    ] {
        let loaded = bootstrap::load_capsule(&root(), name).unwrap();
        let steps = loaded.capsule["steps"].as_array().unwrap();
        assert_eq!(
            steps
                .iter()
                .map(|s| s["version"].as_str().unwrap())
                .collect::<Vec<_>>(),
            versions
        );
        let count: usize = steps
            .iter()
            .map(|s| {
                strict_json(&loaded.payload[s["scenario_registry"].as_str().unwrap()]).unwrap()
                        ["scenarios"]
                        .as_array()
                        .unwrap()
                        .len()
            })
            .sum();
        assert_eq!(count, total);
        assert!(loaded.payload.keys().all(|name| {
            !["authorization", "registry-events", "engagements"]
                .iter()
                .any(|part| name.contains(part))
        }));
    }
}

#[test]
fn prepare_is_reviewable_without_registry_and_requires_private_new_directory() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    let result = bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    assert_eq!(result["workspace_created"], false);
    assert_eq!(result["project_adopted"], false);
    assert!(!scratch.plan().join("registry").exists());
    assert_eq!(fixture.calls, vec!["fixture_discover"]);
    assert!(
        bootstrap::prepare_with_executor(
            &root(),
            &scratch.plan(),
            &options(bootstrap::DEFAULT_BUNDLE),
            &mut fixture
        )
        .is_err()
    );
    assert!(
        bootstrap::prepare_with_executor(
            &root(),
            &root().join("outside-var-test"),
            &options(bootstrap::DEFAULT_BUNDLE),
            &mut fixture
        )
        .is_err()
    );
}

#[test]
fn explicit_authorization_rejects_actor_plan_action_time_and_extra_fields_before_native() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    for (field, value) in [
        ("actor_id", json!("other")),
        ("plan_sha256", json!("0".repeat(64))),
        ("action", json!("adopt-project-binding")),
        ("issued_at", json!("2026-09-27T00:00:00")),
        ("unexpected", json!(true)),
    ] {
        let path = authorize(&scratch.plan(), "initialize-frozen-methods");
        let mut doc = read(&path);
        doc[field] = value;
        save(&path, &doc);
        let calls = fixture.calls.len();
        assert!(
            bootstrap::apply_with_executor(&root(), &scratch.plan(), &path, &mut fixture).is_err()
        );
        assert_eq!(fixture.calls.len(), calls);
    }
    assert!(!scratch.plan().join("registry").exists());
}

#[test]
fn changed_code_or_executable_requires_separate_plan() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let plan_path = scratch.plan().join("plan.json");
    let original = read(&plan_path);
    for key in ["code", "executable_sha256", "runtime", "capsule_sha256"] {
        let mut plan = original.clone();
        plan[key] = json!("changed");
        save(&plan_path, &plan);
        let path = authorize(&scratch.plan(), "initialize-frozen-methods");
        assert!(
            bootstrap::apply_with_executor(&root(), &scratch.plan(), &path, &mut fixture)
                .unwrap_err()
                .to_string()
                .contains("configuration_changed")
        );
    }
}

#[test]
fn five_version_fixture_recovers_each_native_boundary_without_reexecuting_scenarios_and_adopts_separately()
 {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    for action in ["propose", "commit", "verify", "promote"] {
        fixture.interrupt = Some(action.into());
        assert!(
            bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
                .unwrap_err()
                .to_string()
                .contains("injected_after_fixture")
        );
    }
    let result =
        bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).unwrap();
    assert_eq!(result["project_adopted"], false);
    assert_eq!(fixture.scenarios, 356);
    assert!(!scratch.plan().join("project-binding.json").exists());
    let events = fixture.events.clone();
    bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).unwrap();
    assert_eq!(events, fixture.events);
    assert_eq!(fixture.scenarios, 356);
    for events in fixture.events.values() {
        assert_eq!(
            events
                .iter()
                .map(|e| e["state"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "candidate",
                "proposed",
                "committed",
                "regression_passed",
                "release_complete",
                "promoted"
            ]
        );
    }
    let adoption = authorize(&scratch.plan(), "adopt-project-binding");
    let result =
        bootstrap::adopt_with_executor(&root(), &scratch.plan(), &adoption, &mut fixture).unwrap();
    assert_eq!(result["status"], "project_binding_adopted");
    let binding = read(Path::new(result["binding"].as_str().unwrap()));
    assert_eq!(
        binding["authority"]["fact"]["authority_id"],
        "fixture-controlled-records"
    );
    assert_eq!(
        binding["lifecycle_actions"],
        json!(["candidate", "proposed"])
    );
    assert!(
        !binding["allowed_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "commit" || a == "promote")
    );
    let path = scratch.plan().join("step-000/task-commit.json");
    let mut task = read(&path);
    task["intent"] = json!("tampered");
    save(&path, &task);
    assert!(
        bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("retained_task_differs")
    );
}

#[test]
fn failed_authoring_preserves_findings_and_never_proposes() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor {
        reject_authoring: true,
        ..Default::default()
    };
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    assert!(
        bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("authoring_execution_did_not_pass")
    );
    assert!(!fixture.calls.iter().any(|call| call == "propose"));
    assert!(
        !fixture
            .calls
            .iter()
            .any(|call| call == "commit" || call == "promote")
    );
    let open = read(&scratch.plan().join("step-000/open.json"));
    assert_eq!(open["command_verdict"], "blocked");
    assert_eq!(open["execution"]["status"], "blocked");
    assert!(
        fs::read_dir(scratch.plan().join("step-000"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("authoring-failure-"))
    );
}

#[test]
fn unexecuted_open_keeps_native_trace_and_cannot_continue_authoring() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor {
        open_failure: true,
        ..Default::default()
    };
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    assert!(
        bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("open_receipt_not_established")
    );
    assert_eq!(fixture.scenarios, 0);
    assert!(
        !fixture
            .calls
            .iter()
            .any(|a| a == "commit" || a == "promote" || a == "propose")
    );
    assert!(
        fs::read_dir(scratch.plan().join("step-000"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("open-failure-"))
    );
}

#[test]
fn resumed_authoring_rejects_self_rehashed_incomplete_or_mismatched_evidence() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor {
        interrupt: Some("propose".into()),
        ..Default::default()
    };
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    assert!(
        bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("injected_after_fixture")
    );
    let step_dir = scratch.plan().join("step-000");
    let receipt_path = scratch.plan().join("step-000/engagement.json");
    let checks = read_checks_fixture(&step_dir);
    let receipt = read(&receipt_path);
    let events = fixture.events.clone();
    for mutation in [
        "truncate",
        "order",
        "status",
        "package",
        "runtime",
        "receipt",
        "assets",
        "references",
    ] {
        let mut changed = checks.clone();
        let mut claimed = receipt.clone();
        match mutation {
            "truncate" => {
                changed.as_array_mut().unwrap().pop();
            }
            "order" => changed.as_array_mut().unwrap().swap(0, 1),
            "status" => changed[0]["execution"]["status"] = json!("failed"),
            "package" => {
                changed[0]["execution"]["package_version"] = json!("other");
                changed[0]["execution"]["receipt"]["package_version"] = json!("other");
            }
            "runtime" => {
                changed[0]["execution"]["receipt"]["runtime_commit"] = json!("0".repeat(40))
            }
            "receipt" => changed[0]["verification"]["receipt_sha256"] = json!("0".repeat(64)),
            "assets" => changed[0]["execution"]["receipt"]["asset_hashes"] = json!({}),
            _ => {}
        }
        for phase in ["execution", "receipt", "regression", "release"] {
            claimed[phase]["evidence_sha256"] =
                json!(sha256(bootstrap::encoded(&changed).unwrap()));
        }
        claimed["execution_receipts"]=json!(changed.as_array().unwrap().iter().map(|c|json!({"package_id":c["execution"]["package_id"],"package_version":c["execution"]["package_version"],"package_digest":c["execution"]["package_digest"],"receipt_sha256":c["execution"]["receipt"]["receipt_sha256"]})).collect::<Vec<_>>());
        if mutation == "references" {
            claimed["execution_receipts"] = json!([]);
        }
        rewrite_checks_fixture(&step_dir, &changed);
        save(&receipt_path, &claimed);
        assert!(
            bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).is_err(),
            "accepted {mutation}"
        );
        assert_eq!(
            fixture.events, events,
            "advanced native fixture after {mutation}"
        );
    }
}

#[test]
fn resumed_authoring_shards_reject_missing_reordered_and_wrongly_bound_parts() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor {
        interrupt: Some("propose".into()),
        ..Default::default()
    };
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    assert!(bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).is_err());
    let step = scratch.plan().join("step-000");
    let path = step.join("authoring-checks.json");
    let index = read(&path);
    assert_eq!(
        index["schema"],
        "ontology-engineering.authoring-checks-index/v1"
    );
    assert!(
        index["parts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["count"].as_u64().unwrap() <= 4)
    );
    let first = step.join(index["parts"][0]["path"].as_str().unwrap());
    let bytes = fs::read(&first).unwrap();
    let events = fixture.events.clone();
    fs::remove_file(&first).unwrap();
    assert!(bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).is_err());
    fs::write(&first, &bytes).unwrap();
    for mutation in ["order", "count", "extra", "path", "hash"] {
        let mut changed = index.clone();
        match mutation {
            "order" => changed["parts"].as_array_mut().unwrap().swap(0, 1),
            "count" => changed["parts"][0]["count"] = json!(3),
            "extra" => {
                let part = changed["parts"][0].clone();
                changed["parts"].as_array_mut().unwrap().push(part);
            }
            "path" => changed["parts"][0]["path"] = json!("../other.json"),
            _ => changed["parts"][0]["sha256"] = json!("0".repeat(64)),
        }
        save(&path, &changed);
        assert!(
            bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).is_err(),
            "accepted {mutation}"
        );
        assert_eq!(fixture.events, events);
    }
}

#[test]
fn adoption_uses_authorized_raw_bytes_and_detects_changes_around_discovery() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options("engineering-evidence-methods"),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).unwrap();
    let proposal = scratch.plan().join("proposed-project-binding.json");
    // Same parsed value, intentionally different raw serialization: authority
    // binds these exact bytes, not a newly normalized representation.
    let mut bytes = serde_json::to_vec(&read(&proposal)).unwrap();
    bytes.extend_from_slice(b"\n\n");
    fs::write(&proposal, &bytes).unwrap();
    let auth = authorize(&scratch.plan(), "adopt-project-binding");
    fixture.mutate_before_discovery = Some(proposal.clone());
    assert!(
        bootstrap::adopt_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("changed_before_discovery")
    );
    assert!(!scratch.plan().join("project-binding.json").exists());
    fs::write(&proposal, &bytes).unwrap();
    fixture.mutate_during_discovery = true;
    assert!(
        bootstrap::adopt_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("changed_during_discovery")
    );
    assert!(!scratch.plan().join("project-binding.json").exists());
    fs::write(&proposal, &bytes).unwrap();
    fixture.mutate_during_discovery = false;
    bootstrap::adopt_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).unwrap();
    assert_eq!(
        fs::read(scratch.plan().join("project-binding.json")).unwrap(),
        bytes
    );
    assert_eq!(
        read(&scratch.plan().join("project-adoption.json"))["binding_sha256"],
        sha256(bytes)
    );
}

#[test]
fn adoption_rejects_other_discovered_snapshot_even_when_proposal_bytes_are_restored() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options("engineering-evidence-methods"),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).unwrap();
    let proposal = scratch.plan().join("proposed-project-binding.json");
    let before = fs::read(&proposal).unwrap();
    let auth = authorize(&scratch.plan(), "adopt-project-binding");
    fixture.substituted_discovery_hash = true;
    assert!(
        bootstrap::adopt_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("discovery_binding_snapshot_differs")
    );
    assert_eq!(fs::read(&proposal).unwrap(), before);
    assert!(!scratch.plan().join("project-binding.json").exists());
    let failure = fs::read_dir(scratch.plan())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("adoption-discovery-identity-failure-")
        })
        .unwrap();
    assert_eq!(read(&failure)["binding"]["source_sha256"], "0".repeat(64));
}

#[test]
fn project_adoption_cannot_expand_plan_subject_even_with_matching_authorization() {
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let original = read(&scratch.plan().join("project-binding-template.json"));
    for key in ["project", "semantic_target", "baseline", "authority"] {
        let mut value = original.clone();
        value[key] = json!({"modified":true});
        save(
            &scratch.plan().join("proposed-project-binding.json"),
            &value,
        );
        let auth = authorize(&scratch.plan(), "adopt-project-binding");
        let calls = fixture.calls.len();
        assert!(
            bootstrap::adopt_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
                .unwrap_err()
                .to_string()
                .contains("exceeds_exact_plan")
        );
        assert_eq!(fixture.calls.len(), calls);
    }
}

#[cfg(unix)]
#[test]
fn lock_contention_symlinks_and_fifo_fail_before_native() {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let mut fixture = FixtureExecutor::default();
    bootstrap::prepare_with_executor(
        &root(),
        &scratch.plan(),
        &options(bootstrap::DEFAULT_BUNDLE),
        &mut fixture,
    )
    .unwrap();
    let auth = authorize(&scratch.plan(), "initialize-frozen-methods");
    let lock = scratch.plan().join(".bootstrap.lock");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let calls = fixture.calls.len();
    assert!(
        bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture)
            .unwrap_err()
            .to_string()
            .contains("already_running")
    );
    drop(file);
    fs::remove_file(&lock).unwrap();
    symlink(&auth, &lock).unwrap();
    assert!(bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).is_err());
    fs::remove_file(&lock).unwrap();
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(lock.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(bootstrap::apply_with_executor(&root(), &scratch.plan(), &auth, &mut fixture).is_err());
    assert_eq!(fixture.calls.len(), calls);
}

#[test]
fn exact_pretty_identity_preserves_python_number_unicode_and_empty_container_conventions() {
    let value=strict_json(br#"{"z":[1e-7,1000000000000000000000000000000000000000],"a":{},"s":"quote\\\"{}[]","n":[]}"#).unwrap();
    let output = String::from_utf8(bootstrap::encoded(&value).unwrap()).unwrap();
    assert!(output.starts_with("{\n  \"a\": {},\n"));
    assert!(output.contains("1e-07"));
    assert!(output.contains("1000000000000000000000000000000000000000"));
    assert_eq!(strict_json(output.as_bytes()).unwrap(), value);
    assert!(output.ends_with("}\n"));
}
