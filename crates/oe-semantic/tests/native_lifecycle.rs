//! Source-locked native lifecycle acceptance, never an actual project registry.
use base64::{Engine, engine::general_purpose::STANDARD};
use oe_semantic::native::{
    NativeExecution, NativeRuntime, OpenEngagementRequest, ProposeRequest, RegistryRequest,
    TransitionRequest, VerifyRequest,
};
use oe_semantic::{canonical_json, digest, strict_json};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const STAMP: &str = "2026-09-27T00:00:00Z";
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn directory(label: &str) -> PathBuf {
    let path = root().join(format!(
        "var/maintenance/rust-rewrite-20260927/lifecycle-native/{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    oe_core::create_new_private_dir(&path).unwrap();
    path
}
fn source() -> Value {
    json!({"source_id":"synthetic-source","uri":"evidence:rust-lifecycle/source","sha256":digest(b"isolated native acceptance"),"media_type":"text/plain","captured_at":STAMP})
}
fn binding() -> Value {
    json!({"schema_version":"1.0","binding_id":"rust-lifecycle-binding","project_id":"rust-lifecycle",
    "domain":"native-boundary-acceptance","package_id":"rust.native.lifecycle.acceptance","workspace_id":"rust-lifecycle-registry",
    "baseline_version":"0","baseline_package_sha256":digest(canonical_json(&json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]})).unwrap()),
    "evidence_root":"evidence:rust-lifecycle","fact_authorities":["fixture-author"],"decision_authorities":["fixture-controller"],
    "allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],
    "semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":STAMP})
}
fn envelope(action: &str) -> Value {
    json!({"schema_version":"1.0","task_id":format!("rust-lifecycle-{action}"),"task_kind":"synthetic-native-acceptance",
    "project_id":"rust-lifecycle","domain":"native-boundary-acceptance","intent":"Exercise explicitly authorized isolated development lifecycle",
    "requested_decision":"No project fact, engineering or publication acceptance","actor_id":"fixture-controller","requested_actions":[action],
    "required_capabilities":["semantic.package.load"],"evidence":[source()],"created_at":STAMP})
}
fn retain(path: &Path, name: &str, value: &impl serde::Serialize) {
    oe_core::write_new_bytes(
        &path.join(format!("{name}.json")),
        &serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}
fn accept(path: &Path, name: &str, result: NativeExecution) -> Value {
    retain(path, name, &result);
    assert_eq!(result.semantic_execution, "completed", "{name}: {result:?}");
    result.into_result().unwrap()
}
fn transition(registry: &RegistryRequest, delta: &str, action: &str) -> TransitionRequest {
    TransitionRequest {
        registry: registry.clone(),
        delta_sha256: delta.into(),
        envelope: envelope(action),
        recorded_at: Some(STAMP.into()),
    }
}
fn authorization(action: &str, delta: &str) -> Value {
    json!({"authorization_id":format!("fixture-{action}"),"action":action,"actor_id":"fixture-controller","authority":"fixture-controller",
    "package_id":"rust.native.lifecycle.acceptance","delta_sha256":delta,"promotion_target":"industry-registry","reason":"Explicit isolated native boundary integration test",
    "source":source(),"issued_at":STAMP,"decisions":[]})
}

#[test]
fn exact_action_required_before_native_execution() {
    let path = directory("negative");
    let runtime = NativeRuntime::new(&root(), Path::new("/unavailable/interpreter")).unwrap();
    let request = transition(
        &RegistryRequest {
            workspace: path.join("registry"),
            binding: binding(),
        },
        &"a".repeat(64),
        "committed",
    );
    for result in [
        runtime.transition_context(&request, "promoted"),
        runtime.commit_candidate(
            &TransitionRequest {
                delta_sha256: "wrong".into(),
                ..request.clone()
            },
            &json!({}),
        ),
    ] {
        assert_eq!(result.semantic_execution, "not_run");
        assert!(result.runtime_identity.is_none());
    }
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn authoring_manifest_paths_and_hashes_are_checked_before_interpreter() {
    let path = directory("manifest-negative");
    let runtime = NativeRuntime::new(&root(), Path::new("/unavailable/interpreter")).unwrap();
    let asset = path.join("asset.json");
    fs::write(&asset, b"{}").unwrap();
    for (index, entry) in [
        json!({"path":"../escape","sha256":digest(b"{}"),"format":"json"}),
        json!({"path":"asset.json","sha256":"0".repeat(64),"format":"json"}),
        json!({"path":"asset.json","data":"{}","sha256":digest(b"{}"),"format":"json"}),
    ]
    .into_iter()
    .enumerate()
    {
        let manifest = path.join(format!("manifest-{index}.json"));
        fs::write(
            &manifest,
            serde_json::to_vec(&json!({"assets":[entry]})).unwrap(),
        )
        .unwrap();
        let result = runtime.run_manifest(&manifest, "scenario", None);
        assert_eq!(result.semantic_execution, "not_run");
        assert!(result.runtime_identity.is_none());
    }
    let result = runtime.run_manifest_batch(
        &path.join("absent.json"),
        &vec!["scenario".into(); 65],
        None,
    );
    assert!(result.error.unwrap().contains("batch_limit"));
    fs::remove_dir_all(path).unwrap();
}

#[cfg(unix)]
#[test]
fn authoring_manifest_rejects_symlink_asset_before_execution() {
    let path = directory("manifest-symlink");
    fs::write(path.join("asset"), b"{}").unwrap();
    std::os::unix::fs::symlink(path.join("asset"), path.join("link")).unwrap();
    let manifest = path.join("manifest.json");
    fs::write(
        &manifest,
        serde_json::to_vec(
            &json!({"assets":[{"path":"link","sha256":digest(b"{}"),"format":"json"}]}),
        )
        .unwrap(),
    )
    .unwrap();
    let runtime = NativeRuntime::new(&root(), Path::new("/unavailable/interpreter")).unwrap();
    let result = runtime.run_manifest(&manifest, "scenario", None);
    assert_eq!(result.semantic_execution, "not_run");
    assert!(result.runtime_identity.is_none());
    fs::remove_dir_all(path).unwrap();
}

#[test]
#[ignore = "requires installed source-locked external Semantica; private development registry only"]
fn actual_native_fresh_lifecycle_and_verified_recovery() {
    let path = directory("actual");
    let runtime = NativeRuntime::new(&root(), &root().join("runtime/.venv/bin/python")).unwrap();
    let capabilities = accept(&path, "capabilities", runtime.capabilities());
    assert_eq!(
        capabilities["native"]["contract"],
        "semantica.ontology.refinery/v1"
    );
    let delta = accept(
        &path,
        "delta",
        runtime.acceptance_delta(
            "rust.native.lifecycle.acceptance",
            &source(),
            "fixture-author",
            STAMP,
            "1.0.0",
        ),
    );
    let hash = delta["delta_sha256"].as_str().unwrap();
    assert_eq!(
        accept(&path, "normalized-delta", runtime.normalize_delta(&delta)),
        delta
    );
    let registry = RegistryRequest {
        workspace: path.join("registry"),
        binding: binding(),
    };
    let open = accept(
        &path,
        "open",
        runtime.open_engagement(&OpenEngagementRequest {
            workspace: registry.workspace.clone(),
            binding: registry.binding.clone(),
            envelope: envelope("engagement"),
        }),
    );
    assert_eq!(open["status"], "blocked");
    assert_eq!(open["execution_registry_id"], binding()["workspace_id"]);
    // A real missing candidate is a retained error, never events=[] or a reset.
    let missing = runtime.history(&registry, hash);
    retain(&path, "missing-history", &missing);
    assert_eq!(missing.semantic_execution, "failed");
    assert!(missing.result.is_some());
    let contract_bytes = STANDARD
        .decode(delta["contract"][0]["content_base64"].as_str().unwrap())
        .unwrap();
    let projection = strict_json(&contract_bytes).unwrap();
    let mut assets = Vec::new();
    for category in [
        "ontology",
        "competency_questions",
        "shapes",
        "queries",
        "rules",
        "cases",
        "contract",
        "provenance",
    ] {
        for asset in delta[category].as_array().unwrap() {
            let id = asset["asset_id"].as_str().unwrap();
            let bytes = STANDARD
                .decode(asset["content_base64"].as_str().unwrap())
                .unwrap();
            let mut projected = projection["assets"][id].clone();
            projected["asset_id"] = id.into();
            projected["sha256"] = asset["sha256"].clone();
            projected["data"] = String::from_utf8(bytes).unwrap().into();
            assets.push(projected);
        }
    }
    let manifest = json!({"schema_version":"1.0","package_id":delta["package_id"],"version":delta["target_version"],"namespace":projection["namespace"],"execution":projection["execution"],"release_status":projection["release_status"],"assets":assets});
    let manifest_path = path.join("authoring-manifest.json");
    retain(&path, "authoring-manifest", &manifest);
    let scenarios = vec![
        "scenario-current".into(),
        "scenario-negative".into(),
        "scenario-ambiguity".into(),
        "scenario-prior".into(),
    ];
    let checks = accept(
        &path,
        "authoring",
        runtime.run_manifest_batch(&manifest_path, &scenarios, Some(STAMP)),
    );
    for check in checks["checks"].as_array().unwrap() {
        assert_eq!(check["execution"]["status"], "passed");
        assert_eq!(check["verification"]["status"], "complete");
    }
    let evidence_hash = digest(canonical_json(&checks).unwrap());
    let mut engagement = json!({"engagement_id":"rust-lifecycle-authoring","learning":{"status":"candidate","rationale":"Native acceptance fixture only"},"execution_receipts":[],"created_at":STAMP});
    for phase in ["execution", "regression", "receipt", "release"] {
        engagement[phase] = json!({"name":phase,"status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"evidence_sha256":evidence_hash,"details":{"scope":"Actual authoring execution only; native candidate gates follow"}});
    }
    engagement["execution_receipts"]=checks["checks"].as_array().unwrap().iter().map(|c| json!({"receipt_sha256":c["execution"]["receipt"]["receipt_sha256"],"package_id":c["execution"]["package_id"],"package_version":c["execution"]["package_version"],"package_digest":c["execution"]["package_digest"]})).collect();
    let mut open_evidence = engagement.clone();
    open_evidence["engagement_id"] = "rust-lifecycle-evidence-open".into();
    open_evidence["learning"] = json!({"status":"no_delta","rationale":"Authoring evidence alone does not adopt a candidate"});
    let evidence_open = accept(
        &path,
        "open-with-evidence",
        runtime.open_engagement_with_evidence(
            &OpenEngagementRequest {
                workspace: registry.workspace.clone(),
                binding: registry.binding.clone(),
                envelope: envelope("engagement"),
            },
            &open_evidence,
        ),
    );
    assert_eq!(
        evidence_open["registry_id"],
        registry.binding["workspace_id"]
    );
    assert_eq!(evidence_open["engagement"]["status"], "complete");
    let propose = ProposeRequest {
        registry: registry.clone(),
        delta: delta.clone(),
        candidate_envelope: envelope("candidate"),
        proposed_envelope: envelope("proposed"),
        engagement,
        recorded_at: Some(STAMP.into()),
    };
    let first = accept(&path, "propose", runtime.propose_candidate(&propose));
    let retry = accept(&path, "propose-retry", runtime.propose_candidate(&propose));
    assert_eq!(first, retry);
    let mut wrong = propose.clone();
    wrong.registry.binding["workspace_id"] = "other-registry".into();
    let rejected = runtime.propose_candidate(&wrong);
    retain(&path, "wrong-registry", &rejected);
    assert_eq!(rejected.semantic_execution, "failed");
    let commit = authorization("commit", hash);
    let normalized = accept(
        &path,
        "authorization",
        runtime.normalize_authorization(&commit),
    );
    let committed = accept(
        &path,
        "commit",
        runtime.commit_candidate(&transition(&registry, hash, "committed"), &commit),
    );
    assert_eq!(committed["state"]["state"], "committed");
    let mut envelopes = BTreeMap::new();
    for action in [
        "execute_candidate",
        "derive_regression_gate",
        "regression_passed",
        "derive_release_gate",
        "release_complete",
    ] {
        envelopes.insert(action.into(), envelope(action));
    }
    let verify = VerifyRequest {
        registry: registry.clone(),
        delta_sha256: hash.into(),
        envelopes,
        recorded_at: Some(STAMP.into()),
    };
    let verified = accept(&path, "verify", runtime.verify_candidate(&verify));
    assert_eq!(verified["state"]["state"], "release_complete");
    let recovered = accept(&path, "verify-recovery", runtime.verify_candidate(&verify));
    assert!(recovered["execution_suite"].is_null());
    assert_eq!(
        recovered["native_verification"],
        verified["native_verification"]
    );
    let promoted = accept(
        &path,
        "promote",
        runtime.promote_candidate(
            &transition(&registry, hash, "promoted"),
            &authorization("promote", hash),
        ),
    );
    let promoted_retry = accept(
        &path,
        "promote-recovery",
        runtime.promote_candidate(
            &transition(&registry, hash, "promoted"),
            &authorization("promote", hash),
        ),
    );
    assert_eq!(promoted, promoted_retry);
    let history = accept(&path, "history", runtime.history(&registry, hash));
    assert_eq!(history["events"].as_array().unwrap().len(), 6);
    let commit_event = &history["events"][2];
    assert_eq!(
        commit_event["payload"]["authorization_sha256"],
        normalized["authorization_sha256"]
    );
    let context = accept(
        &path,
        "context",
        runtime.transition_context(&transition(&registry, hash, "committed"), "committed"),
    );
    assert_eq!(
        commit_event["payload"]["transition_context_sha256"],
        context["context_sha256"]
    );
    let mut adopted = registry.clone();
    adopted.binding["baseline_version"] = promoted["descriptor"]["version"].clone();
    adopted.binding["baseline_package_sha256"] = promoted["descriptor"]["package_sha256"].clone();
    let current = accept(&path, "current", runtime.discover(&adopted));
    assert_eq!(current["descriptor"], promoted["descriptor"]);
    let stale = runtime.discover(&registry);
    retain(&path, "stale-binding", &stale);
    assert_eq!(stale.semantic_execution, "failed");
    // Corrupt only this test-owned mutable pointer; the actual immutable event
    // chain remains present. A corrupt read must fail rather than look absent.
    let pointer = registry
        .workspace
        .join("refinements")
        .join(hash)
        .join("current.json");
    let before = fs::read(&pointer).unwrap();
    fs::write(&pointer, b"{}").unwrap();
    let corrupted = runtime.history(&registry, hash);
    retain(&path, "corrupted-history", &corrupted);
    fs::write(&pointer, &before).unwrap();
    assert_eq!(corrupted.semantic_execution, "failed");
    assert!(corrupted.result.is_some());
    println!("retained native lifecycle evidence: {}", path.display());
}

#[test]
#[ignore = "requires installed source-locked external Semantica built-in packages"]
fn actual_native_builtin_discovery_preserves_descriptors_and_digests() {
    let path = directory("discovery");
    let runtime = NativeRuntime::new(&root(), &root().join("runtime/.venv/bin/python")).unwrap();
    let value = accept(&path, "builtins", runtime.discover_packages());
    for family in ["chapters", "domains"] {
        let packages = value[family].as_array().unwrap();
        assert!(!packages.is_empty());
        for package in packages {
            assert_eq!(package["package_digest"].as_str().unwrap().len(), 64);
            assert!(package["descriptor"]["package_id"].is_string());
            assert!(package["descriptor"].get("manifest_path").is_none());
            assert!(
                package["available_capabilities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == "semantic.package.load")
            );
        }
    }
    println!("retained native discovery evidence: {}", path.display());
}
