//! Frozen original public synthetic behavior; not historical pass inheritance.
//! The memory backend adapts storage only, with the production refinery engine.
//! Whole original22/19, native filesystem/rights,13494 and adoption stay separate.
use anyhow::{Result, ensure};
use semantica_contracts::*;
use semantica_runtime::{
    refinery::{RefineryEngine, RefineryStore},
    refinery_contracts as wire,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
};

fn original() -> Value {
    assert_eq!(
        semantica_runtime::sha256(ORIGINAL_JSON),
        ORIGINAL_PROJECTION_SHA256
    );
    let value = semantica_json::strict_json(ORIGINAL_JSON.as_bytes()).unwrap();
    assert_eq!(
        value["original_oracle_sha256"],
        "ad31214e067b1e86c0e82e849b2ad10b6ec8b733cdb266dcefc055e044290119"
    );
    assert_eq!(
        value["original_input_sha256"],
        "33de30c7b70b85ad83b7ce9950962dccc4ade2fff9da1ac918e74628be09e206"
    );
    value
}
fn case<'a>(fixture: &'a Value, id: &str) -> &'a Value {
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case_id"] == id)
        .unwrap()
}
fn without(mut value: Value, field: &str) -> Value {
    value.as_object_mut().unwrap().remove(field);
    value
}
fn compact(value: &Value) -> Vec<u8> {
    let mut bytes = semantica_json::canonical_json(value).unwrap();
    bytes.push(b'\n');
    bytes
}
fn context_commit(fixture: &Value) -> Value {
    // This exact committed context was constructed by the original module for
    // its wrong-action counterexample, independently of the Rust algorithm.
    case(fixture, "original-propose-context-rejections")["outcome"]["rejected_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "wrong-action")
        .unwrap()["input"]
        .clone()
}

#[derive(Clone, Default)]
struct Contents {
    files: BTreeMap<String, Vec<u8>>,
    directories: BTreeSet<String>,
}
#[derive(Default)]
struct MemoryStore {
    contents: Mutex<Contents>,
    fail_one_replace: Mutex<bool>,
}
impl MemoryStore {
    fn new() -> Self {
        let result = Self::default();
        result
            .contents
            .lock()
            .unwrap()
            .directories
            .insert(String::new());
        result
    }
    fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        self.contents.lock().unwrap().files.clone()
    }
    fn overwrite_for_fault(&self, path: &str, bytes: Vec<u8>) {
        let mut contents = self.contents.lock().unwrap();
        assert!(
            contents.files.contains_key(path),
            "fault must target an actually retained file"
        );
        contents.files.insert(path.into(), bytes);
    }
    fn fail_next_replace(&self) {
        *self.fail_one_replace.lock().unwrap() = true;
    }
}
fn safe_path(relative: &str) -> Result<()> {
    ensure!(
        !relative.is_empty()
            && !relative.starts_with('/')
            && !relative.contains(['\\', '\0'])
            && relative
                .split('/')
                .all(|v| !v.is_empty() && v != "." && v != ".."),
        "test_storage_path_invalid"
    );
    Ok(())
}
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |v| v.0)
}
impl RefineryStore for MemoryStore {
    fn read(&self, relative: &str) -> Result<Option<Vec<u8>>> {
        safe_path(relative)?;
        Ok(self.contents.lock().unwrap().files.get(relative).cloned())
    }
    fn write_new(&self, relative: &str, bytes: &[u8]) -> Result<()> {
        safe_path(relative)?;
        let mut contents = self.contents.lock().unwrap();
        ensure!(
            contents.directories.contains(parent(relative))
                && !contents.files.contains_key(relative)
                && !contents.directories.contains(relative),
            "test_storage_new_conflict"
        );
        contents.files.insert(relative.into(), bytes.into());
        Ok(())
    }
    fn replace(&self, relative: &str, expected: &[u8], bytes: &[u8]) -> Result<()> {
        safe_path(relative)?;
        let mut fail = self.fail_one_replace.lock().unwrap();
        if *fail {
            *fail = false;
            anyhow::bail!("test_storage_actual_replace_failure");
        }
        let mut contents = self.contents.lock().unwrap();
        ensure!(
            contents.files.get(relative).is_some_and(|v| v == expected),
            "test_storage_cas_mismatch"
        );
        contents.files.insert(relative.into(), bytes.into());
        Ok(())
    }
    fn remove(&self, relative: &str, expected: &[u8]) -> Result<()> {
        safe_path(relative)?;
        let mut contents = self.contents.lock().unwrap();
        ensure!(
            contents.files.get(relative).is_some_and(|v| v == expected),
            "test_storage_remove_cas_mismatch"
        );
        contents.files.remove(relative);
        Ok(())
    }
    fn list(&self, relative: &str) -> Result<Vec<String>> {
        safe_path(relative)?;
        let contents = self.contents.lock().unwrap();
        ensure!(
            contents.directories.contains(relative),
            "test_storage_directory_missing"
        );
        let prefix = format!("{relative}/");
        Ok(contents
            .files
            .keys()
            .chain(contents.directories.iter())
            .filter_map(|path| path.strip_prefix(&prefix))
            .filter(|name| !name.is_empty() && !name.contains('/'))
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
    fn create_dir(&self, relative: &str) -> Result<()> {
        safe_path(relative)?;
        let mut contents = self.contents.lock().unwrap();
        ensure!(
            contents.directories.contains(parent(relative))
                && !contents.files.contains_key(relative)
                && !contents.directories.contains(relative),
            "test_storage_directory_exists_or_parent_missing"
        );
        contents.directories.insert(relative.into());
        Ok(())
    }
    fn ensure_dir(&self, relative: &str) -> Result<()> {
        if self.list(relative).is_ok() {
            Ok(())
        } else {
            self.create_dir(relative)
        }
    }
    fn publish_dir(&self, stage: &str, destination: &str) -> Result<()> {
        safe_path(stage)?;
        safe_path(destination)?;
        let mut contents = self.contents.lock().unwrap();
        ensure!(
            contents.directories.contains(stage)
                && contents.directories.contains(parent(destination))
                && !contents.files.contains_key(destination)
                && !contents.directories.contains(destination),
            "test_storage_publish_conflict"
        );
        let prefix = format!("{stage}/");
        let files = contents
            .files
            .keys()
            .filter(|v| v.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        let directories = contents
            .directories
            .iter()
            .filter(|v| *v == stage || v.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        for path in files {
            let bytes = contents.files.remove(&path).unwrap();
            contents
                .files
                .insert(format!("{destination}{}", &path[stage.len()..]), bytes);
        }
        for path in directories {
            contents.directories.remove(&path);
            contents
                .directories
                .insert(format!("{destination}{}", &path[stage.len()..]));
        }
        Ok(())
    }
}
fn initialize(store: &MemoryStore, fixture: &Value) {
    RefineryEngine::initialize(
        store,
        "registry-1",
        fixture["inputs"]["fixed_time"].as_str().unwrap(),
    )
    .unwrap();
}
fn expect_files(store: &MemoryStore, expected: &Value) {
    let actual = store.snapshot();
    let expected = expected.as_object().unwrap();
    assert_eq!(
        actual.keys().collect::<BTreeSet<_>>(),
        expected.keys().collect::<BTreeSet<_>>(),
        "complete original public file set; no filtered business assets/events"
    );
    for (path, expected) in expected {
        assert_eq!(
            actual[path].len() as u64,
            expected["bytes"].as_u64().unwrap(),
            "original bytes:{path}"
        );
        assert_eq!(
            semantica_runtime::sha256(&actual[path]),
            expected["sha256"].as_str().unwrap(),
            "original SHA:{path}"
        );
    }
}
fn register(engine: &RefineryEngine<'_>, fixture: &Value) -> Value {
    let input = &fixture["inputs"];
    engine
        .register_data(
            &input["delta"],
            &input["candidate_envelope"],
            &input["binding"],
            &input["complete_synthetic_original_engagement"],
            input["fixed_time"].as_str().unwrap(),
        )
        .unwrap()
}
fn propose(engine: &RefineryEngine<'_>, fixture: &Value) -> Value {
    let input = &fixture["inputs"];
    engine
        .propose_context_data(
            input["delta"]["delta_sha256"].as_str().unwrap(),
            &input["proposed_context"],
            input["fixed_time"].as_str().unwrap(),
        )
        .unwrap()
}
fn commit(engine: &RefineryEngine<'_>, fixture: &Value) -> Value {
    let input = &fixture["inputs"];
    engine
        .commit_context_data(
            input["delta"]["delta_sha256"].as_str().unwrap(),
            &input["commit_authorization"],
            &context_commit(fixture),
            input["fixed_time"].as_str().unwrap(),
        )
        .unwrap()
}

#[test]
fn original_refinery_public_dtos_keep_fixed_control_inputs_and_hashes() {
    let fixture = original();
    let input = &fixture["inputs"];
    assert_eq!(wire::binding(&input["binding"]).unwrap(), input["binding"]);
    assert_eq!(
        wire::task(&input["candidate_envelope"]).unwrap(),
        input["candidate_envelope"]
    );
    assert_eq!(wire::delta(&input["delta"]).unwrap(), input["delta"]);
    assert_eq!(
        wire::authorization(&input["commit_authorization"]).unwrap(),
        input["commit_authorization"]
    );
    assert_eq!(
        wire::engagement(&input["complete_synthetic_original_engagement"]).unwrap(),
        input["complete_synthetic_original_engagement"]
    );
    assert_eq!(
        wire::transition_context(
            "proposed",
            input["delta"]["delta_sha256"].as_str().unwrap(),
            &input["proposed_context"]["envelope"],
            &input["binding"]
        )
        .unwrap(),
        input["proposed_context"]
    );
    // Defaults are original from_dict behavior, compared to original output.
    for (name, digest) in [
        ("binding", "binding_sha256"),
        ("candidate_envelope", "envelope_sha256"),
        ("delta", "delta_sha256"),
        ("complete_synthetic_original_engagement", "receipt_sha256"),
    ] {
        let default = without(input[name].clone(), "schema_version");
        let default = if name == "complete_synthetic_original_engagement" {
            default
        } else {
            without(default, digest)
        };
        let normalized = match name {
            "binding" => wire::binding(&default),
            "candidate_envelope" => wire::task(&default),
            "delta" => wire::delta(&default),
            _ => wire::engagement(&default),
        }
        .unwrap();
        assert_eq!(normalized, input[name], "original default:{name}");
    }
    let unknown = json!({"runtime_commit":"","runtime_artifact_sha256":"","runtime_version":""});
    assert_eq!(wire::runtime_source(&unknown).unwrap(), unknown);
    let blocked = &case(&fixture, "original-open-blocked-runtime-input")["input_engagement"];
    assert_eq!(wire::engagement(blocked).unwrap(), *blocked);
    assert_eq!(
        blocked["blocked_reasons"],
        json!(["runtime_source_identity"])
    );
    for name in ["execution", "regression", "receipt", "release"] {
        let phase = &input["complete_synthetic_original_engagement"][name];
        assert_eq!(wire::phase(phase).unwrap(), *phase);
        assert_eq!(
            wire::phase(&without(phase.clone(), "missing_capabilities")).unwrap(),
            *phase
        );
        let mut wrong_derived = phase.clone();
        wrong_derived["missing_capabilities"] = json!(["ignored-original-derived-field"]);
        assert_eq!(
            wire::phase(&wrong_derived).unwrap(),
            *phase,
            "original from_dict recomputes derived missing_capabilities"
        );
    }
    for row in case(&fixture, "original-delta-four-book-impacts")["outcome"]
        .as_array()
        .unwrap()
    {
        let mut delta = without(input["delta"].clone(), "delta_sha256");
        delta["book_impact"] = row["book_impact"].clone();
        assert_eq!(
            wire::delta(&delta).unwrap()["delta_sha256"],
            row["delta_sha256"]
        );
    }
    for id in [
        "original-delta-missing-array",
        "original-delta-declared-hash-tamper",
    ] {
        assert!(
            wire::delta(&case(&fixture, id)["input"]).is_err(),
            "original rejected:{id}"
        );
    }
    let mut invalid_impact = without(input["delta"].clone(), "delta_sha256");
    invalid_impact["book_impact"] = json!("hidden-in-provenance");
    assert!(wire::delta(&invalid_impact).is_err());
    for name in [
        "binding",
        "candidate_envelope",
        "delta",
        "commit_authorization",
        "complete_synthetic_original_engagement",
    ] {
        let mut value = input[name].clone();
        value["shadow"] = json!(true);
        let rejected = match name {
            "binding" => wire::binding(&value),
            "candidate_envelope" => wire::task(&value),
            "delta" => wire::delta(&value),
            "commit_authorization" => wire::authorization(&value),
            _ => wire::engagement(&value),
        };
        assert!(rejected.is_err(), "unknown field:{name}");
    }
    for category in wire::CATEGORIES {
        for asset in input["delta"][*category].as_array().unwrap() {
            let bytes = wire::asset_content(asset).unwrap().unwrap();
            assert_eq!(semantica_runtime::sha256(bytes), asset["sha256"]);
        }
    }
    // Shared generated structures are actual deserialization boundaries. These
    // finite original families do not assert all22 behaviors are implemented.
    let delta: SemanticRefineryPackageDelta =
        serde_json::from_slice(&semantica_json::canonical_json(&input["delta"]).unwrap()).unwrap();
    assert!(delta.validate().is_ok());
    let mut missing_option = input["delta"]["ontology"][0].clone();
    for key in ["role", "case_kind", "replaces_sha256"] {
        missing_option.as_object_mut().unwrap().remove(key);
    }
    let asset: SemanticRefineryPackageAssetDelta =
        serde_json::from_slice(&semantica_json::canonical_json(&missing_option).unwrap()).unwrap();
    assert!(asset.role.is_none() && asset.case_kind.is_none() && asset.replaces_sha256.is_none());
    let context: SemanticRefineryTransitionContext = serde_json::from_slice(
        &semantica_json::canonical_json(&input["proposed_context"]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        context.context_sha256,
        input["proposed_context"]["context_sha256"]
            .as_str()
            .unwrap()
    );
}

#[test]
fn original_lifecycle_nonzero_delta_keeps_candidate_authorization_history_and_unknown_gates() {
    let fixture = original();
    let input = &fixture["inputs"];
    let hash = input["delta"]["delta_sha256"].as_str().unwrap();
    let time = input["fixed_time"].as_str().unwrap();
    let store = MemoryStore::new();
    initialize(&store, &fixture);
    let engine = RefineryEngine::new(&store, "registry-1").unwrap();
    expect_files(
        &store,
        &case(&fixture, "original-open-complete-synthetic-test-input")["files_before"],
    );
    assert_eq!(
        engine
            .open_data(
                &input["candidate_envelope"],
                &input["binding"],
                &input["complete_synthetic_original_engagement"]
            )
            .unwrap(),
        input["complete_synthetic_original_engagement"]
    );
    expect_files(
        &store,
        &case(&fixture, "original-open-complete-synthetic-test-input")["files_after"],
    );
    let initial = register(&engine, &fixture);
    assert_eq!(initial["state"], "candidate");
    assert_eq!(
        initial["event_sha256"],
        case(&fixture, "original-propose-two-event-chain")["outcome"]["history"][0]["event_sha256"]
    );
    expect_files(
        &store,
        &case(&fixture, "original-propose-two-event-chain")["files_before"],
    );
    for row in case(&fixture, "original-propose-context-rejections")["outcome"]["rejected_cases"]
        .as_array()
        .unwrap()
    {
        let before = store.snapshot();
        let result = if row.get("input").is_some() {
            engine.propose_context_data(hash, &row["input"], time)
        } else {
            let mut task = without(
                input["proposed_context"]["envelope"].clone(),
                "envelope_sha256",
            );
            if row["id"] == "wrong-project" {
                task["project_id"] = json!("wrong-project");
            } else {
                assert_eq!(row["id"], "outside-evidence-root");
                task["evidence"][0]["uri"] = json!("urn:outside:evidence");
            }
            wire::transition_context("proposed", hash, &task, &input["binding"])
        };
        assert!(result.is_err(), "original rejection:{}", row["id"]);
        assert_eq!(
            store.snapshot(),
            before,
            "original rejection must not write:{}",
            row["id"]
        );
        assert_eq!(engine.status(hash).unwrap(), initial);
    }
    assert_eq!(
        propose(&engine, &fixture),
        case(&fixture, "original-propose-two-event-chain")["outcome"]["state"]
    );
    assert_eq!(
        json!(engine.history(hash).unwrap()),
        case(&fixture, "original-propose-two-event-chain")["outcome"]["history"]
    );
    expect_files(
        &store,
        &case(&fixture, "original-propose-two-event-chain")["files_after"],
    );
    let before = store.snapshot();
    let retried = engine
        .propose_data(
            &input["delta"],
            &input["candidate_envelope"],
            &input["proposed_context"]["envelope"],
            &input["binding"],
            &input["complete_synthetic_original_engagement"],
            time,
        )
        .unwrap();
    assert_eq!(
        retried["state"],
        case(&fixture, "original-propose-retry-exact")["outcome"]["state"]
    );
    assert_eq!(store.snapshot(), before);
    let retry = &case(&fixture, "original-propose-retry-different-context")["input_context"];
    assert!(
        engine
            .propose_data(
                &input["delta"],
                &input["candidate_envelope"],
                &retry["envelope"],
                &input["binding"],
                &input["complete_synthetic_original_engagement"],
                time
            )
            .is_err()
    );
    assert_eq!(store.snapshot(), before);
    for row in case(&fixture, "original-commit-authority-rejections")["outcome"]["rejected_cases"]
        .as_array()
        .unwrap()
    {
        assert!(
            engine
                .commit_context_data(hash, &row["input"], &context_commit(&fixture), time)
                .is_err(),
            "original authorization rejected:{}",
            row["id"]
        );
        assert_eq!(
            store.snapshot(),
            before,
            "original authorization rejection writes nothing"
        );
        assert_eq!(engine.status(hash).unwrap()["state"], "proposed");
    }
    assert_eq!(
        commit(&engine, &fixture),
        case(&fixture, "original-commit-materialization")["outcome"]["state"]
    );
    assert_eq!(
        json!(engine.history(hash).unwrap()),
        case(&fixture, "original-commit-materialization")["outcome"]["history"]
    );
    expect_files(
        &store,
        &case(&fixture, "original-commit-materialization")["files_after"],
    );
    let manifest = engine.committed_manifest(hash).unwrap();
    assert_eq!(manifest["assets"].as_array().unwrap().len(), 13);
    assert_eq!(
        manifest["package_sha256"],
        "b71ac2a7f5eec5f33ecf20981718714320c2c2ff268b34f0b81f1dc8d0483656"
    );
    assert_eq!(
        engine.document("registry.json").unwrap()["packages"],
        json!({})
    );
    assert_eq!(store.list("packages").unwrap(), Vec::<String>::new());
    assert_eq!(
        engine.status(hash).unwrap()["state"],
        "committed",
        "materialization is not validation, promotion or engineering adoption"
    );
    assert_eq!(
        case(&fixture, "original-execute-dependency-boundary")["semantic_validation"],
        "unknown_dependency_unavailable"
    );
    assert_eq!(
        case(&fixture, "original-execute-dependency-boundary")["state_after"]["state"],
        "committed"
    );

    let blocked_store = MemoryStore::new();
    initialize(&blocked_store, &fixture);
    let blocked_engine = RefineryEngine::new(&blocked_store, "registry-1").unwrap();
    let blocked = &case(&fixture, "original-open-blocked-runtime-input")["input_engagement"];
    blocked_engine
        .open_data(&input["candidate_envelope"], &input["binding"], blocked)
        .unwrap();
    let before = blocked_store.snapshot();
    assert!(
        blocked_engine
            .register_data(
                &input["delta"],
                &input["candidate_envelope"],
                &input["binding"],
                blocked,
                time
            )
            .is_err()
    );
    assert_eq!(blocked_store.snapshot(), before);
    expect_files(
        &blocked_store,
        &case(&fixture, "original-open-blocked-runtime-input")["files_after"],
    );
}

#[test]
fn original_timestamp53_matches_actual_observations_without_rewriting_forecast() {
    assert_eq!(
        semantica_runtime::sha256(TIMESTAMP_JSON),
        TIMESTAMP_PROJECTION_SHA256
    );
    let fixture = semantica_json::strict_json(TIMESTAMP_JSON.as_bytes()).unwrap();
    assert_eq!(
        fixture["fixed_forecast_sha256"],
        "59c149d8a0119e7adf998f7eef8cb1ca9a53576bb9e4f0b775b9bc54b820d465"
    );
    let rows = fixture["observations"].as_array().unwrap();
    assert_eq!(rows.len(), 53);
    assert_eq!(
        rows.iter()
            .filter(|v| v["accepted"] == v["actual_accepted"])
            .count(),
        50,
        "original forecast's3surprises remain recorded"
    );
    for row in rows {
        let result = wire::timestamp(&row["input"]);
        assert_eq!(
            result.is_ok(),
            row["actual_accepted"].as_bool().unwrap(),
            "actual original datetime observation:{}",
            row["id"]
        );
        if let Ok(value) = result {
            assert_eq!(
                value,
                row["input"].as_str().unwrap(),
                "timestamp bytes retained"
            );
        }
    }
}

#[test]
fn storage_compare_replace_and_publication_are_atomic() {
    let store = MemoryStore::new();
    store.create_dir("stage").unwrap();
    store.create_dir("stage/nested").unwrap();
    store.write_new("stage/nested/input", b"original").unwrap();
    let before = store.snapshot();
    assert!(
        store
            .replace("stage/nested/input", b"stale", b"replacement")
            .is_err()
    );
    assert_eq!(store.snapshot(), before);
    assert!(store.remove("stage/nested/input", b"stale").is_err());
    assert_eq!(store.snapshot(), before);
    store.create_dir("existing").unwrap();
    assert!(store.publish_dir("stage", "existing").is_err());
    assert_eq!(store.snapshot(), before);
    store.publish_dir("stage", "published").unwrap();
    assert_eq!(
        store.read("published/nested/input").unwrap().unwrap(),
        b"original"
    );
    assert!(store.list("stage").is_err());
    assert!(store.read("../published/nested/input").is_err());
    store
        .replace("published/nested/input", b"original", b"replacement")
        .unwrap();
    assert_eq!(
        store.read("published/nested/input").unwrap().unwrap(),
        b"replacement"
    );
}

#[test]
fn original_candidate_replay_rejects_bytes_chain_and_tip_tampering() {
    let fixture = original();
    let hash = fixture["inputs"]["delta"]["delta_sha256"].as_str().unwrap();
    for fault in ["asset", "event", "pointer", "binding", "authorization"] {
        let store = MemoryStore::new();
        initialize(&store, &fixture);
        let engine = RefineryEngine::new(&store, "registry-1").unwrap();
        register(&engine, &fixture);
        propose(&engine, &fixture);
        commit(&engine, &fixture);
        let history = engine.history(hash).unwrap();
        let path = match fault {
            "asset" => {
                let sha = fixture["inputs"]["delta"]["ontology"][0]["sha256"]
                    .as_str()
                    .unwrap();
                format!("objects/sha256/{}/{sha}", &sha[..2])
            }
            "authorization" => {
                let sha = history[2]["payload"]["authorization_object_sha256"]
                    .as_str()
                    .unwrap();
                format!("objects/sha256/{}/{sha}", &sha[..2])
            }
            "event" => format!(
                "refinements/{hash}/events/000002-{}.json",
                history[1]["event_sha256"].as_str().unwrap()
            ),
            "pointer" => format!("refinements/{hash}/current.json"),
            _ => format!("refinements/{hash}/binding.json"),
        };
        let mut bytes = store.read(&path).unwrap().unwrap();
        bytes.push(b'x');
        store.overwrite_for_fault(&path, bytes);
        let before = store.snapshot();
        assert!(
            engine.history(hash).is_err(),
            "corrupt actual retained:{fault}"
        );
        assert_eq!(
            store.snapshot(),
            before,
            "invalid chain is not rewritten to pass"
        );
    }
}

#[test]
fn original_pending_event_recovery_preserves_chain_and_cas_tip() {
    let fixture = original();
    let input = &fixture["inputs"];
    let hash = input["delta"]["delta_sha256"].as_str().unwrap();
    let time = input["fixed_time"].as_str().unwrap();
    let store = MemoryStore::new();
    initialize(&store, &fixture);
    let engine = RefineryEngine::new(&store, "registry-1").unwrap();
    register(&engine, &fixture);
    store.fail_next_replace();
    assert!(
        engine
            .propose_context_data(hash, &input["proposed_context"], time)
            .is_err()
    );
    assert!(
        store
            .read(&format!("refinements/{hash}/pending-event.json"))
            .unwrap()
            .is_some()
    );
    let history = engine.history(hash).unwrap();
    assert_eq!(
        json!(history),
        case(&fixture, "original-propose-two-event-chain")["outcome"]["history"]
    );
    assert!(
        store
            .read(&format!("refinements/{hash}/pending-event.json"))
            .unwrap()
            .is_none()
    );
    expect_files(
        &store,
        &case(&fixture, "original-propose-two-event-chain")["files_after"],
    );
    let stable = store.snapshot();
    engine.history(hash).unwrap();
    assert_eq!(store.snapshot(), stable);

    let store = MemoryStore::new();
    initialize(&store, &fixture);
    let engine = RefineryEngine::new(&store, "registry-1").unwrap();
    register(&engine, &fixture);
    store.fail_next_replace();
    assert!(
        engine
            .propose_context_data(hash, &input["proposed_context"], time)
            .is_err()
    );
    let path = format!("refinements/{hash}/current.json");
    let mut current = semantica_json::strict_json(&store.read(&path).unwrap().unwrap()).unwrap();
    current["event_sha256"] =
        json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    store.overwrite_for_fault(&path, compact(&current));
    let before = store.snapshot();
    assert!(engine.history(hash).is_err());
    assert_eq!(
        store.snapshot(),
        before,
        "recovery rejects conflicting actual tip without deleting evidence"
    );
}

#[test]
fn data_port_keeps_unverified_subject_gates_and_truth_registry_empty() {
    let fixture = original();
    let input = &fixture["inputs"];
    let store = MemoryStore::new();
    initialize(&store, &fixture);
    let engine = RefineryEngine::new(&store, "registry-1").unwrap();
    register(&engine, &fixture);
    propose(&engine, &fixture);
    commit(&engine, &fixture);
    let before = store.snapshot();
    for operation in ["verify", "promote"] {
        let request = SemanticRefineryRequest {
            schema: "semantica.refinery-request/1".into(),
            operation: operation.into(),
            registry_id: "registry-1".into(),
            binding: input["binding"].clone(),
            task: Some(context_commit(&fixture)["envelope"].clone()),
            delta: Some(input["delta"].clone()),
            engagement: Some(input["complete_synthetic_original_engagement"].clone()),
            candidate: Some(input["delta"]["delta_sha256"].as_str().unwrap().into()),
            authorization: Some(input["commit_authorization"].clone()),
            recorded_at: input["fixed_time"].as_str().unwrap().into(),
        };
        let rejected = engine.execute(&request).unwrap_err();
        assert_eq!(rejected.phase, operation);
        assert_eq!(store.snapshot(), before);
        assert_eq!(
            engine.document("registry.json").unwrap()["packages"],
            json!({})
        );
    }
    let request = SemanticRefineryRequest {
        schema: "semantica.refinery-request/1".into(),
        operation: "status".into(),
        registry_id: "registry-1".into(),
        binding: input["binding"].clone(),
        task: None,
        delta: None,
        engagement: None,
        candidate: Some(input["delta"]["delta_sha256"].as_str().unwrap().into()),
        authorization: None,
        recorded_at: input["fixed_time"].as_str().unwrap().into(),
    };
    let observed = engine.execute(&request).unwrap();
    assert_eq!(observed.result["state"]["state"], "committed");
    assert!(!observed.uncovered.is_empty());
    assert_eq!(store.snapshot(), before);
}

const ORIGINAL_PROJECTION_SHA256: &str =
    "969603c4d1ca446bec3a1bd0c072b2fda084d803421db97954981841420b5fa1";
const ORIGINAL_JSON: &str = r####"{"schema":"semantica.refinery.original-rust-test-projection/1","original_input_sha256":"33de30c7b70b85ad83b7ce9950962dccc4ade2fff9da1ac918e74628be09e206","original_oracle_sha256":"ad31214e067b1e86c0e82e849b2ad10b6ec8b733cdb266dcefc055e044290119","source_commit":"8b2ed123c380fc8abc78507e6e6648e6f5c0052c","original_refinery_sha256":"6a7d70ae69f2720a85414db2b7ee05787699a1d93271699276aae5908cd94594","inputs":{"delta":{"schema_version":"1.0","package_id":"industry.example","base_version":"0","base_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","target_version":"1.0.0","rationale":"Repeated engineering practice merits a governed candidate.","created_by":"fact.authority","created_at":"2026-08-19T00:00:00Z","required_capabilities":["semantic.package.load"],"source_evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"book_impact":"none","ontology":[{"category":"ontology","asset_id":"ontology-core","operation":"add","media_type":"text/turtle","sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","content_base64":"QHByZWZpeCBvd2w6IDxodHRwOi8vd3d3LnczLm9yZy8yMDAyLzA3L293bCM+IC4KPHVybjppbmR1c3RyeTpleGFtcGxlPiBhIG93bDpPbnRvbG9neSAuCg==","replaces_sha256":null,"role":null,"case_kind":null}],"competency_questions":[{"category":"competency_questions","asset_id":"cq-registry","operation":"add","media_type":"application/json","sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","content_base64":"eyJjb21wZXRlbmN5X3F1ZXN0aW9ucyI6W3siaWQiOiJjcS1jdXJyZW50IiwicXVlc3Rpb24iOiJDYW4gdGhlIGN1cnJlbnQgcmV1c2FibGUgY2FzZSBiZSByZXRyaWV2ZWQ/Iiwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLWN1cnJlbnQiXX0seyJpZCI6ImNxLXByaW9yIiwicXVlc3Rpb24iOiJEb2VzIHRoZSBwcmlvci1yZWxlYXNlIGNhc2Ugc3RpbGwgZXhlY3V0ZT8iLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX1dLCJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"shapes":[{"category":"shapes","asset_id":"quality-shapes","operation":"add","media_type":"text/turtle","sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","content_base64":"QHByZWZpeCBzaDogPGh0dHA6Ly93d3cudzMub3JnL25zL3NoYWNsIz4gLgpAcHJlZml4IGV4OiA8dXJuOmV4YW1wbGU6PiAuCmV4Ok5hbWVkU2hhcGUgYSBzaDpOb2RlU2hhcGUgOyBzaDp0YXJnZXRTdWJqZWN0c09mIGV4Om5hbWUgLgo=","replaces_sha256":null,"role":null,"case_kind":null}],"queries":[{"category":"queries","asset_id":"cq-primary","operation":"add","media_type":"application/sparql-query","sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","content_base64":"U0VMRUNUID9uYW1lIFdIRVJFIHsgP3MgPHVybjpleGFtcGxlOm5hbWU+ID9uYW1lIC4gfSBPUkRFUiBCWSA/bmFtZQo=","replaces_sha256":null,"role":null,"case_kind":null}],"rules":[{"category":"rules","asset_id":"engineering-rules","operation":"add","media_type":"text/plain","sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","content_base64":"SUYgb2JzZXJ2ZWQoP3gpIFRIRU4gcmV1c2FibGUoP3gpCg==","replaces_sha256":null,"role":null,"case_kind":null}],"cases":[{"category":"cases","asset_id":"case-positive","operation":"add","media_type":"text/turtle","sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","content_base64":"PHVybjpjYXNlOnBvc2l0aXZlPiA8dXJuOmV4YW1wbGU6bmFtZT4gInBvc2l0aXZlIiAuCg==","replaces_sha256":null,"role":null,"case_kind":"positive"},{"category":"cases","asset_id":"case-negative","operation":"add","media_type":"text/turtle","sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","content_base64":"IyBkZWxpYmVyYXRlbHkgZW1wdHkgbmVnYXRpdmUgZ3JhcGgK","replaces_sha256":null,"role":null,"case_kind":"negative"},{"category":"cases","asset_id":"case-ambiguity","operation":"add","media_type":"text/turtle","sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","content_base64":"PHVybjpjYXNlOmFtYmlndWl0eT4gPHVybjpleGFtcGxlOm5hbWU+ICJhbWJpZ3VpdHkiIC4K","replaces_sha256":null,"role":null,"case_kind":"ambiguity"},{"category":"cases","asset_id":"case-prior_release","operation":"add","media_type":"text/turtle","sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","content_base64":"PHVybjpjYXNlOnByaW9yX3JlbGVhc2U+IDx1cm46ZXhhbXBsZTpuYW1lPiAicHJpb3IiIC4K","replaces_sha256":null,"role":null,"case_kind":"prior_release"}],"contract":[{"category":"contract","asset_id":"package-contract","operation":"add","media_type":"application/json","sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","content_base64":"eyJhc3NldHMiOnsiY2FzZS1hbWJpZ3VpdHkiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNhc2UifSwiY2FzZS1uZWdhdGl2ZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjYXNlLXBvc2l0aXZlIjp7ImZvcm1hdCI6InR1cnRsZSIsImtpbmQiOiJyZGYiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjYXNlIn0sImNhc2UtcHJpb3JfcmVsZWFzZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjcS1wcmltYXJ5Ijp7ImZvcm1hdCI6InNwYXJxbCIsImtpbmQiOiJ0ZXh0IiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic3BhcnFsIn0sImNxLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNvbXBldGVuY3lfcXVlc3Rpb25zIn0sImVuZ2luZWVyaW5nLXJ1bGVzIjp7ImZvcm1hdCI6InRleHQiLCJraW5kIjoidGV4dCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImVuZ2luZWVyaW5nX3J1bGVzIn0sIm9udG9sb2d5LWNvcmUiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0Ijp0cnVlLCJyb2xlIjoib250b2xvZ3kifSwicGFja2FnZS1jb250cmFjdCI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjb250cmFjdCJ9LCJxdWFsaXR5LXNoYXBlcyI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic2hhcGVzIn0sInJpZ2h0cy1ldmlkZW5jZSI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJwcm92ZW5hbmNlIn0sInNjZW5hcmlvLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InNjZW5hcmlvX3JlZ2lzdHJ5In0sInNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIjp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InByb3ZlbmFuY2UifX0sImV4ZWN1dGlvbiI6eyJjcV9yZWdpc3RyeV9hc3NldF9pZCI6ImNxLXJlZ2lzdHJ5Iiwic2NlbmFyaW9fcmVnaXN0cnlfYXNzZXRfaWQiOiJzY2VuYXJpby1yZWdpc3RyeSJ9LCJnYXRlX3N1aXRlIjp7ImNhc2UuYW1iaWd1aXR5Ijp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtYW1iaWd1aXR5Il0sImNxX2lkcyI6W10sInNjZW5hcmlvX2lkcyI6WyJzY2VuYXJpby1hbWJpZ3VpdHkiXX0sImNhc2UubmVnYXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1uZWdhdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tbmVnYXRpdmUiXX0sImNhc2UucG9zaXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1wb3NpdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY2FzZS5wcmlvcl9yZWxlYXNlIjp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtcHJpb3JfcmVsZWFzZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX0sImNxLmN1cnJlbnQiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtY3VycmVudCJdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY3EucHJpb3IiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtcHJpb3IiXSwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLXByaW9yIl19fSwibmFtZXNwYWNlIjoidXJuOmluZHVzdHJ5OmV4YW1wbGU6IiwicHJvdmVuYW5jZV9ldmlkZW5jZV9hc3NldF9pZHMiOlsic291cmNlLWV2aWRlbmNlLWJpbmRpbmciXSwicmVsZWFzZV9zdGF0dXMiOiJjb21wbGV0ZSIsInJpZ2h0c19ldmlkZW5jZV9hc3NldF9pZHMiOlsicmlnaHRzLWV2aWRlbmNlIl0sInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"provenance":[{"category":"provenance","asset_id":"scenario-registry","operation":"add","media_type":"application/json","sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","content_base64":"eyJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjZW5hcmlvcyI6W3siZXhlY3V0aW9uIjp7Im9wZXJhdGlvbiI6InNlbGVjdF9leGFjdF9tdWx0aXNldCIsInByb3ZpZGVyIjoiU2VtYW50aWNSdW50aW1lIiwicXVlcnlfYXNzZXRfaWQiOiJjcS1wcmltYXJ5In0sImlkIjoic2NlbmFyaW8tY3VycmVudCIsImlucHV0cyI6WyJjYXNlLXBvc2l0aXZlIiwiY2FzZS1uZWdhdGl2ZSJdLCJvcmFjbGUiOnsiaWQiOiJvcmFjbGUtc2NlbmFyaW8tY3VycmVudCIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicG9zaXRpdmUiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLW5lZ2F0aXZlIiwiaW5wdXRzIjpbImNhc2UtbmVnYXRpdmUiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1uZWdhdGl2ZSIsInBvc2l0aXZlIjp7InJvd19jb3VudCI6MH0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLWFtYmlndWl0eSIsImlucHV0cyI6WyJjYXNlLWFtYmlndWl0eSIsImNhc2UtbmVnYXRpdmUiXSwib3JhY2xlIjp7ImlkIjoib3JhY2xlLXNjZW5hcmlvLWFtYmlndWl0eSIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsiYW1iaWd1aXR5Il19LCJyb3dfY291bnQiOjF9LCJzaW5nbGVfZmF1bHRfbmVnYXRpdmUiOnsicm93X2NvdW50IjowfSwic3RhdHVzIjoicmVhZHkifSwic3RhdHVzIjoibmF0aXZlIn0seyJleGVjdXRpb24iOnsib3BlcmF0aW9uIjoic2VsZWN0X2V4YWN0X211bHRpc2V0IiwicHJvdmlkZXIiOiJTZW1hbnRpY1J1bnRpbWUiLCJxdWVyeV9hc3NldF9pZCI6ImNxLXByaW1hcnkifSwiaWQiOiJzY2VuYXJpby1wcmlvciIsImlucHV0cyI6WyJjYXNlLXByaW9yX3JlbGVhc2UiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1wcmlvciIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicHJpb3IiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifV0sInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"rights-evidence","operation":"add","media_type":"application/json","sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","content_base64":"eyJzY2hlbWFfdmVyc2lvbiI6IjEuMCIsInNvdXJjZV9pZCI6ImVuZ2luZWVyaW5nLXJlY29yZCIsInNvdXJjZV9zaGEyNTYiOiJhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhIiwic3RhdGVtZW50IjoiUmlnaHRzIGV2aWRlbmNlIHJldGFpbmVkOyBubyBsZWdhbCBjb25jbHVzaW9uLiJ9","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"source-evidence-binding","operation":"add","media_type":"application/json","sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","content_base64":"eyJraW5kIjoic2VtYW50aWNhLnNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIiwic2NoZW1hX3ZlcnNpb24iOiIxLjAiLCJzb3VyY2VfZXZpZGVuY2UiOlt7ImNhcHR1cmVkX2F0IjoiMjAyNi0wOC0xOVQwMDowMDowMFoiLCJtZWRpYV90eXBlIjoiYXBwbGljYXRpb24vanNvbiIsInNoYTI1NiI6ImFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWEiLCJzb3VyY2VfaWQiOiJlbmdpbmVlcmluZy1yZWNvcmQiLCJ1cmkiOiJ1cm46ZXhhbXBsZTpldmlkZW5jZS1yb290OmVuZ2luZWVyaW5nLXJlY29yZCJ9XX0=","replaces_sha256":null,"role":null,"case_kind":null}],"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"candidate_envelope":{"schema_version":"1.0","task_id":"task-candidate","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["candidate"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436"},"complete_synthetic_original_engagement":{"schema_version":"1.0","contract":"semantica.ontology.refinery/v1","engagement_id":"engagement-1","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee","required_capabilities":["semantic.package.load"],"runtime_source":{"runtime_commit":"1111111111111111111111111111111111111111","runtime_artifact_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","runtime_version":"0.6.5+oe.2"},"execution":{"name":"execution","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"regression":{"name":"regression","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"receipt":{"name":"receipt","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"release":{"name":"release","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"learning":{"status":"candidate","rationale":"The lesson is reusable outside this project.","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960"},"execution_receipts":[{"receipt_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","package_id":"semantica.chapter.reference","package_version":"1","package_digest":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}],"status":"complete","blocked_reasons":[],"created_at":"2026-08-19T00:00:00Z","receipt_sha256":"2e345ecbbdd7143d356bfb3fe321bc78c4354e3b9d80c053bfe5f650e1ceaeac"},"proposed_context":{"schema_version":"1.0","action":"proposed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"task-proposed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"a3004835a439319ab1e63ec0f3a33cff7a169e83ce335b25f55033eb6bebe617"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"context_sha256":"4dd2b54f1990466e84bd53ab765a7b54db9e1fb43c9c70f8b3ccf525b6700452"},"commit_authorization":{"authorization_id":"authorization-commit","action":"commit","actor_id":"decision.authority","authority":"decision.authority","package_id":"industry.example","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","promotion_target":"industry-registry","reason":"Reviewed against the bound evidence and project authority.","source":{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"},"issued_at":"2026-08-19T00:00:00Z","decisions":[],"authorization_sha256":"ab604f3e094adaa464a12600fb6ee7e8c7b5b1393cb7fad452f66d5499295067"},"fixed_time":"2026-08-19T00:00:00Z"},"cases":[{"case_id":"original-pure-five-assertion-functions","outcome":{"status":"original_assertions_passed","count":5,"functions":["test_contract_exposes_exact_six_state_and_complete_package_surface","test_engagement_always_has_five_outputs_and_missing_capability_blocks","test_engagement_phase_fields_have_lossless_round_trip","test_package_delta_serializes_every_asset_family_and_case_kind","test_public_acceptance_factory_owns_complete_executable_payload"]}},{"case_id":"original-open-complete-synthetic-test-input","outcome":{"returned_receipt":{"schema_version":"1.0","contract":"semantica.ontology.refinery/v1","engagement_id":"engagement-1","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee","required_capabilities":["semantic.package.load"],"runtime_source":{"runtime_commit":"1111111111111111111111111111111111111111","runtime_artifact_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","runtime_version":"0.6.5+oe.2"},"execution":{"name":"execution","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"regression":{"name":"regression","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"receipt":{"name":"receipt","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"release":{"name":"release","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"learning":{"status":"candidate","rationale":"The lesson is reusable outside this project.","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960"},"execution_receipts":[{"receipt_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","package_id":"semantica.chapter.reference","package_version":"1","package_digest":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}],"status":"complete","blocked_reasons":[],"created_at":"2026-08-19T00:00:00Z","receipt_sha256":"2e345ecbbdd7143d356bfb3fe321bc78c4354e3b9d80c053bfe5f650e1ceaeac"},"receipt_equal_input":true,"truth_packages":{},"refinement_count":0},"files_before":{"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}},"files_after":{"engagements/4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2.json":{"sha256":"4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2","bytes":3801},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}}},{"case_id":"original-open-blocked-runtime-input","outcome":{"status":"raised","exception":"RefineryGateError","message":"candidate requires a complete semantic engagement receipt","violations":["candidate requires a complete semantic engagement receipt"]},"input_engagement":{"schema_version":"1.0","contract":"semantica.ontology.refinery/v1","engagement_id":"engagement-1","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee","required_capabilities":["semantic.package.load"],"runtime_source":{"runtime_commit":"","runtime_artifact_sha256":"","runtime_version":""},"execution":{"name":"execution","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"regression":{"name":"regression","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"receipt":{"name":"receipt","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"release":{"name":"release","status":"passed","required_capabilities":["semantic.package.load"],"observed_capabilities":["semantic.package.load"],"missing_capabilities":[],"evidence_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","details":{"executed":true}},"learning":{"status":"candidate","rationale":"The lesson is reusable outside this project.","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960"},"execution_receipts":[{"receipt_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","package_id":"semantica.chapter.reference","package_version":"1","package_digest":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}],"status":"blocked","blocked_reasons":["runtime_source_identity"],"created_at":"2026-08-19T00:00:00Z","receipt_sha256":"d20ba9f467520908512c89a2ee85bff41f8b69154655ffc604880cb6fbc86a5e"},"recordable":true,"files_before":{"engagements/2e49097691727cb1091fc15efe9369a1e6da146ba05945ca156540b2b50b7409.json":{"sha256":"2e49097691727cb1091fc15efe9369a1e6da146ba05945ca156540b2b50b7409","bytes":3711},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}},"files_after":{"engagements/2e49097691727cb1091fc15efe9369a1e6da146ba05945ca156540b2b50b7409.json":{"sha256":"2e49097691727cb1091fc15efe9369a1e6da146ba05945ca156540b2b50b7409","bytes":3711},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}}},{"case_id":"original-propose-context-rejections","outcome":{"rejected_cases":[{"id":"wrong-action","input":{"schema_version":"1.0","action":"committed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"task-committed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["committed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"77a9628935ccf7089fb521b2fcd68ca8fc463b8d928bdb66126121d14771fcc8"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"context_sha256":"e46b738ad5f2e45aa63ddc7b9253c1be3566197f02deebe4233e2f560615b179"},"outcome":{"status":"raised","exception":"RefineryGateError","message":"transition context action must be exactly proposed; transition task must request exactly the invoked action","violations":["transition context action must be exactly proposed","transition task must request exactly the invoked action"]},"files_unchanged":true},{"id":"wrong-delta","input":{"schema_version":"1.0","action":"proposed","delta_sha256":"00d0b4f0a74339a05c38c2a047c8effb159362c95366bf3b65473a6a6ce07c63","envelope":{"schema_version":"1.0","task_id":"task-proposed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"a3004835a439319ab1e63ec0f3a33cff7a169e83ce335b25f55033eb6bebe617"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"context_sha256":"b6ae8b13bceda0d0d01335ec61c40eeaad53e730501fac014a684ff3c3d1d2c7"},"outcome":{"status":"raised","exception":"RefineryGateError","message":"transition context does not bind exact delta","violations":["transition context does not bind exact delta"]},"files_unchanged":true},{"id":"wrong-workspace","input":{"schema_version":"1.0","action":"proposed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"task-proposed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"a3004835a439319ab1e63ec0f3a33cff7a169e83ce335b25f55033eb6bebe617"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-2","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"5ac83799788627ce3995db189537d791c6d243e51a0b9e76c45839ffc3bae4bd"},"context_sha256":"d05cb903ad19c8c640ed8a5903e0c27e04c52675382e4709c397116c5c40e59c"},"outcome":{"status":"raised","exception":"RefineryGateError","message":"transition context binding differs from retained current binding; transition context workspace differs from registry","violations":["transition context binding differs from retained current binding","transition context workspace differs from registry"]},"files_unchanged":true},{"id":"wrong-package","input":{"schema_version":"1.0","action":"proposed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"task-proposed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"a3004835a439319ab1e63ec0f3a33cff7a169e83ce335b25f55033eb6bebe617"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.other","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"ef31ec34a07a5e5eb5a3d0e8678801836542c13ffba67cab297da8553e2188fe"},"context_sha256":"c06da5af0316e91d7ebc6f6aa2d2e378a3c1a23c3e2e8736edf439640dce189a"},"outcome":{"status":"raised","exception":"RefineryGateError","message":"transition context binding differs from retained current binding; transition context package differs from delta","violations":["transition context binding differs from retained current binding","transition context package differs from delta"]},"files_unchanged":true},{"id":"wrong-binding","input":{"schema_version":"1.0","action":"proposed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"task-proposed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"a3004835a439319ab1e63ec0f3a33cff7a169e83ce335b25f55033eb6bebe617"},"binding":{"schema_version":"1.0","binding_id":"binding-2","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"1cf80cf62873197b6f81c57ebda98ce99382d4232bf3d91f83c58640a47dc95f"},"context_sha256":"01510cd169470c231bd37095498503319f4b75bcb52c5a3fbbe2911bc644910f"},"outcome":{"status":"raised","exception":"RefineryGateError","message":"transition context binding differs from retained current binding","violations":["transition context binding differs from retained current binding"]},"files_unchanged":true},{"id":"wrong-project","outcome":{"status":"raised","exception":"RefineryInputError","message":"transition task project/domain differs from current binding","violations":null},"constructor_checks_performed":true},{"id":"outside-evidence-root","outcome":{"status":"raised","exception":"RefineryInputError","message":"transition task evidence is outside current binding evidence_root","violations":null},"constructor_checks_performed":true},{"id":"unknown-context-field","input":{"schema_version":"1.0","action":"proposed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"task-proposed","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"a3004835a439319ab1e63ec0f3a33cff7a169e83ce335b25f55033eb6bebe617"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"context_sha256":"4dd2b54f1990466e84bd53ab765a7b54db9e1fb43c9c70f8b3ccf525b6700452","shadow":true},"outcome":{"status":"raised","exception":"RefineryInputError","message":"transition context contains unknown fields: shadow","violations":null}}],"state_still":"candidate"}},{"case_id":"original-propose-two-event-chain","outcome":{"state":{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","package_id":"industry.example","state":"proposed","sequence":2,"event_sha256":"7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53","previous_event_sha256":"33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210","package_sha256":null,"recorded_at":"2026-08-19T00:00:00Z"},"history":[{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","event_sha256":"33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210","package_id":"industry.example","package_sha256":null,"payload":{"binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee","engagement_receipt_sha256":"2e345ecbbdd7143d356bfb3fe321bc78c4354e3b9d80c053bfe5f650e1ceaeac","engagement_status":"complete","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436","transition_context_object_sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","transition_context_sha256":"fcf8ef5f718bdffbbdc0adf00d2979a77784ae4bd4342ae498a7db4e3a2b7732"},"previous_event_sha256":null,"recorded_at":"2026-08-19T00:00:00Z","schema_version":"1.0","sequence":1,"state":"candidate"},{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","event_sha256":"7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53","package_id":"industry.example","package_sha256":null,"payload":{"proposal":"candidate submitted for governed decision","transition_context_object_sha256":"df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018","transition_context_sha256":"4dd2b54f1990466e84bd53ab765a7b54db9e1fb43c9c70f8b3ccf525b6700452"},"previous_event_sha256":"33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210","recorded_at":"2026-08-19T00:00:00Z","schema_version":"1.0","sequence":2,"state":"proposed"}],"industry_packages_unchanged":true},"files_before":{"engagements/4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2.json":{"sha256":"4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2","bytes":3801},"objects/sha256/1e/1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90":{"sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","bytes":320},"objects/sha256/25/25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac":{"sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","bytes":54},"objects/sha256/27/2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752":{"sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","bytes":302},"objects/sha256/41/41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a":{"sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","bytes":201},"objects/sha256/4f/4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b":{"sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","bytes":1535},"objects/sha256/64/64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324":{"sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","bytes":88},"objects/sha256/7a/7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a":{"sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","bytes":1697},"objects/sha256/7b/7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0":{"sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","bytes":2267},"objects/sha256/88/88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13":{"sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","bytes":54},"objects/sha256/94/94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed":{"sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","bytes":52},"objects/sha256/bf/bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968":{"sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","bytes":68},"objects/sha256/c4/c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9":{"sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","bytes":34},"objects/sha256/d4/d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466":{"sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","bytes":36},"objects/sha256/eb/ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8":{"sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","bytes":134},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/binding.json":{"sha256":"8edfd4554ca68935c04c7726ed6a848d8a0f5638099a7202c2f4387ac749240a","bytes":728},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/current.json":{"sha256":"d69dbc6d16156e1bf784224b2d8dc09f556010ac4ff49f747ffad84950059b93","bytes":342},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/delta.json":{"sha256":"29e0d6bdfa0e6650690ead2867df828f225fc5b2cb629e03966a097b391ae7b6","bytes":10984},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/engagement.json":{"sha256":"499d5d15176cdb4da7b70c881b17abab180753c1dd9fb63f13fab7732a623dd1","bytes":2270},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/envelope.json":{"sha256":"fef7e9bbfbe2be98484df0aeccbae44f216496b5c6ef40ac25258c187886e9d0","bytes":736},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000001-33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210.json":{"sha256":"689413982be5c6e2f2be5807538c6b704c7e99a45f290c64f073586cee5b4dd9","bytes":846},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}},"files_after":{"engagements/4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2.json":{"sha256":"4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2","bytes":3801},"objects/sha256/1e/1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90":{"sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","bytes":320},"objects/sha256/25/25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac":{"sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","bytes":54},"objects/sha256/27/2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752":{"sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","bytes":302},"objects/sha256/41/41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a":{"sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","bytes":201},"objects/sha256/4f/4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b":{"sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","bytes":1535},"objects/sha256/64/64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324":{"sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","bytes":88},"objects/sha256/7a/7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a":{"sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","bytes":1697},"objects/sha256/7b/7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0":{"sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","bytes":2267},"objects/sha256/88/88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13":{"sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","bytes":54},"objects/sha256/94/94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed":{"sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","bytes":52},"objects/sha256/bf/bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968":{"sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","bytes":68},"objects/sha256/c4/c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9":{"sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","bytes":34},"objects/sha256/d4/d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466":{"sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","bytes":36},"objects/sha256/df/df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018":{"sha256":"df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018","bytes":1694},"objects/sha256/eb/ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8":{"sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","bytes":134},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/binding.json":{"sha256":"8edfd4554ca68935c04c7726ed6a848d8a0f5638099a7202c2f4387ac749240a","bytes":728},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/current.json":{"sha256":"3f95623b341a675a6262ba3ebcbe781cde434a92c91cde505c8d7c0f35e251df","bytes":403},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/delta.json":{"sha256":"29e0d6bdfa0e6650690ead2867df828f225fc5b2cb629e03966a097b391ae7b6","bytes":10984},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/engagement.json":{"sha256":"499d5d15176cdb4da7b70c881b17abab180753c1dd9fb63f13fab7732a623dd1","bytes":2270},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/envelope.json":{"sha256":"fef7e9bbfbe2be98484df0aeccbae44f216496b5c6ef40ac25258c187886e9d0","bytes":736},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000001-33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210.json":{"sha256":"689413982be5c6e2f2be5807538c6b704c7e99a45f290c64f073586cee5b4dd9","bytes":846},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000002-7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53.json":{"sha256":"22ae451b4b8e9a3beb8239cb1d6d17f64e646fa2f9fe56dd9b5318d4c529e102","bytes":667},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}}},{"case_id":"original-propose-retry-exact","outcome":{"state":{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","package_id":"industry.example","state":"proposed","sequence":2,"event_sha256":"7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53","previous_event_sha256":"33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210","package_sha256":null,"recorded_at":"2026-08-19T00:00:00Z"},"files_unchanged":true,"history_length":2}},{"case_id":"original-propose-retry-different-context","outcome":{"status":"raised","exception":"RefineryGateError","message":"retry transition context differs from proposed event","violations":["retry transition context differs from proposed event"]},"input_context":{"schema_version":"1.0","action":"proposed","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","envelope":{"schema_version":"1.0","task_id":"different-retry-task","task_kind":"engineering-refinement","project_id":"project-1","domain":"example-industry","intent":"Refine repeated engineering knowledge.","requested_decision":"Determine whether the lesson is reusable.","actor_id":"fact.authority","requested_actions":["proposed"],"required_capabilities":["semantic.package.load"],"evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"created_at":"2026-08-19T00:00:00Z","envelope_sha256":"9d7c6029bb9ce206b02a6d885ef1f2a7e648831e246585bc3867d56ceede0d72"},"binding":{"schema_version":"1.0","binding_id":"binding-1","project_id":"project-1","domain":"example-industry","package_id":"industry.example","workspace_id":"registry-1","baseline_version":"0","baseline_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","evidence_root":"urn:example:evidence-root","fact_authorities":["fact.authority"],"decision_authorities":["decision.authority"],"allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],"semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":"2026-08-19T00:00:00Z","binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee"},"context_sha256":"da272a547aa087b2411b2fbe9511d92facdb9006015eb8f4a85e7651898b3e88"}},{"case_id":"original-commit-authority-rejections","outcome":{"rejected_cases":[{"id":"wrong-actor","input":{"authorization_id":"authorization-commit","action":"commit","actor_id":"fact.authority","authority":"decision.authority","package_id":"industry.example","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","promotion_target":"industry-registry","reason":"Reviewed against the bound evidence and project authority.","source":{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"},"issued_at":"2026-08-19T00:00:00Z","decisions":[],"authorization_sha256":"0b508c9a11126dd986113918c1da3f54ccc978b430b2653dc50394e5e6967f61"},"outcome":{"status":"raised","exception":"RefineryGateError","message":"authorization actor is not a decision authority","violations":["authorization actor is not a decision authority"]},"files_unchanged":true},{"id":"outside-source","input":{"authorization_id":"authorization-commit","action":"commit","actor_id":"decision.authority","authority":"decision.authority","package_id":"industry.example","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","promotion_target":"industry-registry","reason":"Reviewed against the bound evidence and project authority.","source":{"source_id":"engineering-record","uri":"urn:outside:evidence","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"},"issued_at":"2026-08-19T00:00:00Z","decisions":[],"authorization_sha256":"c730e5e25bd6a285dcabfe00dde293e6b0cdac1895bdea24bdd1b6b12294516a"},"outcome":{"status":"raised","exception":"RefineryGateError","message":"authorization evidence URI is outside binding evidence_root","violations":["authorization evidence URI is outside binding evidence_root"]},"files_unchanged":true}],"state_still":"proposed"}},{"case_id":"original-commit-materialization","outcome":{"state":{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","package_id":"industry.example","state":"committed","sequence":3,"event_sha256":"57379225432eba0ab5fff9c1c0820df13567223fe533d6934b07a37099d4642b","previous_event_sha256":"7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53","package_sha256":"b71ac2a7f5eec5f33ecf20981718714320c2c2ff268b34f0b81f1dc8d0483656","recorded_at":"2026-08-19T00:00:00Z"},"history":[{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","event_sha256":"33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210","package_id":"industry.example","package_sha256":null,"payload":{"binding_sha256":"d5d6f20c237c8080a3ca1936f269139d786de00596a155479784cb37fba7baee","engagement_receipt_sha256":"2e345ecbbdd7143d356bfb3fe321bc78c4354e3b9d80c053bfe5f650e1ceaeac","engagement_status":"complete","envelope_sha256":"900e86870090eb3d73a2569c9261c906bd79626faad2a4e3e593fdd57dc35436","transition_context_object_sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","transition_context_sha256":"fcf8ef5f718bdffbbdc0adf00d2979a77784ae4bd4342ae498a7db4e3a2b7732"},"previous_event_sha256":null,"recorded_at":"2026-08-19T00:00:00Z","schema_version":"1.0","sequence":1,"state":"candidate"},{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","event_sha256":"7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53","package_id":"industry.example","package_sha256":null,"payload":{"proposal":"candidate submitted for governed decision","transition_context_object_sha256":"df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018","transition_context_sha256":"4dd2b54f1990466e84bd53ab765a7b54db9e1fb43c9c70f8b3ccf525b6700452"},"previous_event_sha256":"33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210","recorded_at":"2026-08-19T00:00:00Z","schema_version":"1.0","sequence":2,"state":"proposed"},{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","event_sha256":"57379225432eba0ab5fff9c1c0820df13567223fe533d6934b07a37099d4642b","package_id":"industry.example","package_sha256":"b71ac2a7f5eec5f33ecf20981718714320c2c2ff268b34f0b81f1dc8d0483656","payload":{"authorization_object_sha256":"ce7d810680bbc3a13f463e7b391ef24571591ab3c902ff31cfba717aa786a797","authorization_sha256":"ab604f3e094adaa464a12600fb6ee7e8c7b5b1393cb7fad452f66d5499295067","engagement_receipt_sha256":"2e345ecbbdd7143d356bfb3fe321bc78c4354e3b9d80c053bfe5f650e1ceaeac","manifest_sha256":"9097f2bf84707922eede0f483f2599e12d436d56656b00f9c1bde5fbd8b8f2aa","transition_context_object_sha256":"244f2be0a99c2b0cad905bf5017a545df26f7da0dbb930bf73bd2eb1e7ba78a7","transition_context_sha256":"e46b738ad5f2e45aa63ddc7b9253c1be3566197f02deebe4233e2f560615b179"},"previous_event_sha256":"7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53","recorded_at":"2026-08-19T00:00:00Z","schema_version":"1.0","sequence":3,"state":"committed"}],"industry_packages_unchanged":true,"full_assets_materialized":13},"files_before":{"engagements/4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2.json":{"sha256":"4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2","bytes":3801},"objects/sha256/1e/1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90":{"sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","bytes":320},"objects/sha256/25/25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac":{"sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","bytes":54},"objects/sha256/27/2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752":{"sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","bytes":302},"objects/sha256/41/41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a":{"sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","bytes":201},"objects/sha256/4f/4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b":{"sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","bytes":1535},"objects/sha256/64/64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324":{"sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","bytes":88},"objects/sha256/7a/7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a":{"sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","bytes":1697},"objects/sha256/7b/7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0":{"sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","bytes":2267},"objects/sha256/88/88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13":{"sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","bytes":54},"objects/sha256/94/94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed":{"sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","bytes":52},"objects/sha256/bf/bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968":{"sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","bytes":68},"objects/sha256/c4/c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9":{"sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","bytes":34},"objects/sha256/d4/d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466":{"sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","bytes":36},"objects/sha256/df/df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018":{"sha256":"df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018","bytes":1694},"objects/sha256/eb/ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8":{"sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","bytes":134},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/binding.json":{"sha256":"8edfd4554ca68935c04c7726ed6a848d8a0f5638099a7202c2f4387ac749240a","bytes":728},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/current.json":{"sha256":"3f95623b341a675a6262ba3ebcbe781cde434a92c91cde505c8d7c0f35e251df","bytes":403},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/delta.json":{"sha256":"29e0d6bdfa0e6650690ead2867df828f225fc5b2cb629e03966a097b391ae7b6","bytes":10984},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/engagement.json":{"sha256":"499d5d15176cdb4da7b70c881b17abab180753c1dd9fb63f13fab7732a623dd1","bytes":2270},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/envelope.json":{"sha256":"fef7e9bbfbe2be98484df0aeccbae44f216496b5c6ef40ac25258c187886e9d0","bytes":736},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000001-33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210.json":{"sha256":"689413982be5c6e2f2be5807538c6b704c7e99a45f290c64f073586cee5b4dd9","bytes":846},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000002-7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53.json":{"sha256":"22ae451b4b8e9a3beb8239cb1d6d17f64e646fa2f9fe56dd9b5318d4c529e102","bytes":667},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}},"files_after":{"engagements/4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2.json":{"sha256":"4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2","bytes":3801},"objects/sha256/1e/1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90":{"sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","bytes":320},"objects/sha256/24/244f2be0a99c2b0cad905bf5017a545df26f7da0dbb930bf73bd2eb1e7ba78a7":{"sha256":"244f2be0a99c2b0cad905bf5017a545df26f7da0dbb930bf73bd2eb1e7ba78a7","bytes":1697},"objects/sha256/25/25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac":{"sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","bytes":54},"objects/sha256/27/2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752":{"sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","bytes":302},"objects/sha256/41/41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a":{"sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","bytes":201},"objects/sha256/4f/4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b":{"sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","bytes":1535},"objects/sha256/64/64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324":{"sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","bytes":88},"objects/sha256/7a/7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a":{"sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","bytes":1697},"objects/sha256/7b/7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0":{"sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","bytes":2267},"objects/sha256/88/88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13":{"sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","bytes":54},"objects/sha256/90/9097f2bf84707922eede0f483f2599e12d436d56656b00f9c1bde5fbd8b8f2aa":{"sha256":"9097f2bf84707922eede0f483f2599e12d436d56656b00f9c1bde5fbd8b8f2aa","bytes":3199},"objects/sha256/94/94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed":{"sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","bytes":52},"objects/sha256/bf/bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968":{"sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","bytes":68},"objects/sha256/c4/c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9":{"sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","bytes":34},"objects/sha256/ce/ce7d810680bbc3a13f463e7b391ef24571591ab3c902ff31cfba717aa786a797":{"sha256":"ce7d810680bbc3a13f463e7b391ef24571591ab3c902ff31cfba717aa786a797","bytes":732},"objects/sha256/d4/d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466":{"sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","bytes":36},"objects/sha256/df/df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018":{"sha256":"df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018","bytes":1694},"objects/sha256/eb/ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8":{"sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","bytes":134},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/binding.json":{"sha256":"8edfd4554ca68935c04c7726ed6a848d8a0f5638099a7202c2f4387ac749240a","bytes":728},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/current.json":{"sha256":"c1e9846892121b486d406cc5d8ba0054d5666b4cde0100eab50417ac0a61ee40","bytes":466},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/delta.json":{"sha256":"29e0d6bdfa0e6650690ead2867df828f225fc5b2cb629e03966a097b391ae7b6","bytes":10984},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/engagement.json":{"sha256":"499d5d15176cdb4da7b70c881b17abab180753c1dd9fb63f13fab7732a623dd1","bytes":2270},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/envelope.json":{"sha256":"fef7e9bbfbe2be98484df0aeccbae44f216496b5c6ef40ac25258c187886e9d0","bytes":736},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000001-33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210.json":{"sha256":"689413982be5c6e2f2be5807538c6b704c7e99a45f290c64f073586cee5b4dd9","bytes":846},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000002-7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53.json":{"sha256":"22ae451b4b8e9a3beb8239cb1d6d17f64e646fa2f9fe56dd9b5318d4c529e102","bytes":667},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000003-57379225432eba0ab5fff9c1c0820df13567223fe533d6934b07a37099d4642b.json":{"sha256":"c4a51359d5a77e63f4f1515f1b1559bffc528ed7893a89674eeaf06840cc81b2","bytes":1042},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}}},{"case_id":"original-execute-dependency-boundary","outcome":{"status":"raised","exception":"ModuleNotFoundError","message":"No module named 'rdflib'","violations":null},"semantic_validation":"unknown_dependency_unavailable","state_after":{"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","package_id":"industry.example","state":"committed","sequence":3,"event_sha256":"57379225432eba0ab5fff9c1c0820df13567223fe533d6934b07a37099d4642b","previous_event_sha256":"7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53","package_sha256":"b71ac2a7f5eec5f33ecf20981718714320c2c2ff268b34f0b81f1dc8d0483656","recorded_at":"2026-08-19T00:00:00Z"},"business_runtime_stub_used":false,"files_before":{"engagements/4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2.json":{"sha256":"4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2","bytes":3801},"objects/sha256/1e/1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90":{"sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","bytes":320},"objects/sha256/24/244f2be0a99c2b0cad905bf5017a545df26f7da0dbb930bf73bd2eb1e7ba78a7":{"sha256":"244f2be0a99c2b0cad905bf5017a545df26f7da0dbb930bf73bd2eb1e7ba78a7","bytes":1697},"objects/sha256/25/25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac":{"sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","bytes":54},"objects/sha256/27/2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752":{"sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","bytes":302},"objects/sha256/41/41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a":{"sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","bytes":201},"objects/sha256/4f/4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b":{"sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","bytes":1535},"objects/sha256/64/64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324":{"sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","bytes":88},"objects/sha256/7a/7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a":{"sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","bytes":1697},"objects/sha256/7b/7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0":{"sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","bytes":2267},"objects/sha256/88/88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13":{"sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","bytes":54},"objects/sha256/90/9097f2bf84707922eede0f483f2599e12d436d56656b00f9c1bde5fbd8b8f2aa":{"sha256":"9097f2bf84707922eede0f483f2599e12d436d56656b00f9c1bde5fbd8b8f2aa","bytes":3199},"objects/sha256/94/94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed":{"sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","bytes":52},"objects/sha256/bf/bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968":{"sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","bytes":68},"objects/sha256/c4/c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9":{"sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","bytes":34},"objects/sha256/ce/ce7d810680bbc3a13f463e7b391ef24571591ab3c902ff31cfba717aa786a797":{"sha256":"ce7d810680bbc3a13f463e7b391ef24571591ab3c902ff31cfba717aa786a797","bytes":732},"objects/sha256/d4/d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466":{"sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","bytes":36},"objects/sha256/df/df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018":{"sha256":"df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018","bytes":1694},"objects/sha256/eb/ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8":{"sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","bytes":134},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/binding.json":{"sha256":"8edfd4554ca68935c04c7726ed6a848d8a0f5638099a7202c2f4387ac749240a","bytes":728},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/current.json":{"sha256":"c1e9846892121b486d406cc5d8ba0054d5666b4cde0100eab50417ac0a61ee40","bytes":466},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/delta.json":{"sha256":"29e0d6bdfa0e6650690ead2867df828f225fc5b2cb629e03966a097b391ae7b6","bytes":10984},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/engagement.json":{"sha256":"499d5d15176cdb4da7b70c881b17abab180753c1dd9fb63f13fab7732a623dd1","bytes":2270},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/envelope.json":{"sha256":"fef7e9bbfbe2be98484df0aeccbae44f216496b5c6ef40ac25258c187886e9d0","bytes":736},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000001-33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210.json":{"sha256":"689413982be5c6e2f2be5807538c6b704c7e99a45f290c64f073586cee5b4dd9","bytes":846},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000002-7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53.json":{"sha256":"22ae451b4b8e9a3beb8239cb1d6d17f64e646fa2f9fe56dd9b5318d4c529e102","bytes":667},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000003-57379225432eba0ab5fff9c1c0820df13567223fe533d6934b07a37099d4642b.json":{"sha256":"c4a51359d5a77e63f4f1515f1b1559bffc528ed7893a89674eeaf06840cc81b2","bytes":1042},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}},"files_after":{"engagements/4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2.json":{"sha256":"4677118769321cf7cfbe14f50abf6d1b7a33ae7dd231172d1f448088d58e8ab2","bytes":3801},"objects/sha256/1e/1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90":{"sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","bytes":320},"objects/sha256/24/244f2be0a99c2b0cad905bf5017a545df26f7da0dbb930bf73bd2eb1e7ba78a7":{"sha256":"244f2be0a99c2b0cad905bf5017a545df26f7da0dbb930bf73bd2eb1e7ba78a7","bytes":1697},"objects/sha256/25/25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac":{"sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","bytes":54},"objects/sha256/27/2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752":{"sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","bytes":302},"objects/sha256/41/41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a":{"sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","bytes":201},"objects/sha256/4f/4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b":{"sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","bytes":1535},"objects/sha256/64/64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324":{"sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","bytes":88},"objects/sha256/7a/7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a":{"sha256":"7afebd988661b2fd2ddb7fcbb6dc8017c852c4718c728f43b604b9704d3d115a","bytes":1697},"objects/sha256/7b/7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0":{"sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","bytes":2267},"objects/sha256/88/88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13":{"sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","bytes":54},"objects/sha256/90/9097f2bf84707922eede0f483f2599e12d436d56656b00f9c1bde5fbd8b8f2aa":{"sha256":"9097f2bf84707922eede0f483f2599e12d436d56656b00f9c1bde5fbd8b8f2aa","bytes":3199},"objects/sha256/94/94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed":{"sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","bytes":52},"objects/sha256/bf/bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968":{"sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","bytes":68},"objects/sha256/c4/c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9":{"sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","bytes":34},"objects/sha256/ce/ce7d810680bbc3a13f463e7b391ef24571591ab3c902ff31cfba717aa786a797":{"sha256":"ce7d810680bbc3a13f463e7b391ef24571591ab3c902ff31cfba717aa786a797","bytes":732},"objects/sha256/d2/d230d75d3ab8e79b0d77c6ad715ea814f7c7013ed5cf0658fa79358690bb135e":{"sha256":"d230d75d3ab8e79b0d77c6ad715ea814f7c7013ed5cf0658fa79358690bb135e","bytes":1721},"objects/sha256/d4/d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466":{"sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","bytes":36},"objects/sha256/df/df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018":{"sha256":"df8858b0dacc5ad018b9e4476a5d0b02cc174ed67a1748f9f972e1e2a7b5f018","bytes":1694},"objects/sha256/eb/ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8":{"sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","bytes":134},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/binding.json":{"sha256":"8edfd4554ca68935c04c7726ed6a848d8a0f5638099a7202c2f4387ac749240a","bytes":728},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/current.json":{"sha256":"c1e9846892121b486d406cc5d8ba0054d5666b4cde0100eab50417ac0a61ee40","bytes":466},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/delta.json":{"sha256":"29e0d6bdfa0e6650690ead2867df828f225fc5b2cb629e03966a097b391ae7b6","bytes":10984},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/engagement.json":{"sha256":"499d5d15176cdb4da7b70c881b17abab180753c1dd9fb63f13fab7732a623dd1","bytes":2270},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/envelope.json":{"sha256":"fef7e9bbfbe2be98484df0aeccbae44f216496b5c6ef40ac25258c187886e9d0","bytes":736},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000001-33219824b6717810b415588c2fb86d498d6c5e475ce21065d4d738e16a4d6210.json":{"sha256":"689413982be5c6e2f2be5807538c6b704c7e99a45f290c64f073586cee5b4dd9","bytes":846},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000002-7fe54f681318599b90a95cfc467eb1ba56ddaa15c463aaad4e6f88f9616b0f53.json":{"sha256":"22ae451b4b8e9a3beb8239cb1d6d17f64e646fa2f9fe56dd9b5318d4c529e102","bytes":667},"refinements/065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960/events/000003-57379225432eba0ab5fff9c1c0820df13567223fe533d6934b07a37099d4642b.json":{"sha256":"c4a51359d5a77e63f4f1515f1b1559bffc528ed7893a89674eeaf06840cc81b2","bytes":1042},"registry.json":{"sha256":"8ea684fc0321329bbebd4e9ced536571849ee0adc2a49f6bf7dc71e48b9e5a32","bytes":189},"workspace.json":{"sha256":"0960a38d46b61a72ee7478de126a2358fab8b9392297efcb9b7972ed497c0f46","bytes":222}}},{"case_id":"original-delta-four-book-impacts","outcome":[{"book_impact":"none","delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960","roundtrip_exact":true},{"book_impact":"vol1-method","delta_sha256":"a419ae3a3ac76828bc2bb951690c810c2ba08185720ac6062bad729874cc1c7c","roundtrip_exact":true},{"book_impact":"vol2-iso-exemplar","delta_sha256":"0185040688c5ae7a3e4b1e1eb374b3ef352ba3f5e767422c7df286ab12003f6d","roundtrip_exact":true},{"book_impact":"both","delta_sha256":"eac1803ab30ae2604b30ea1e676e00d670f2aa82df52f17ca41552f80fcb9640","roundtrip_exact":true}]},{"case_id":"original-delta-invalid-book-impact","outcome":{"status":"raised","exception":"RefineryInputError","message":"book_impact must be none, vol1-method, vol2-iso-exemplar, or both","violations":null}},{"case_id":"original-delta-missing-array","outcome":{"status":"raised","exception":"RefineryInputError","message":"package delta must explicitly declare all asset arrays; missing: shapes","violations":null},"input":{"schema_version":"1.0","package_id":"industry.example","base_version":"0","base_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","target_version":"1.0.0","rationale":"Repeated engineering practice merits a governed candidate.","created_by":"fact.authority","created_at":"2026-08-19T00:00:00Z","required_capabilities":["semantic.package.load"],"source_evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"book_impact":"none","ontology":[{"category":"ontology","asset_id":"ontology-core","operation":"add","media_type":"text/turtle","sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","content_base64":"QHByZWZpeCBvd2w6IDxodHRwOi8vd3d3LnczLm9yZy8yMDAyLzA3L293bCM+IC4KPHVybjppbmR1c3RyeTpleGFtcGxlPiBhIG93bDpPbnRvbG9neSAuCg==","replaces_sha256":null,"role":null,"case_kind":null}],"competency_questions":[{"category":"competency_questions","asset_id":"cq-registry","operation":"add","media_type":"application/json","sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","content_base64":"eyJjb21wZXRlbmN5X3F1ZXN0aW9ucyI6W3siaWQiOiJjcS1jdXJyZW50IiwicXVlc3Rpb24iOiJDYW4gdGhlIGN1cnJlbnQgcmV1c2FibGUgY2FzZSBiZSByZXRyaWV2ZWQ/Iiwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLWN1cnJlbnQiXX0seyJpZCI6ImNxLXByaW9yIiwicXVlc3Rpb24iOiJEb2VzIHRoZSBwcmlvci1yZWxlYXNlIGNhc2Ugc3RpbGwgZXhlY3V0ZT8iLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX1dLCJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"queries":[{"category":"queries","asset_id":"cq-primary","operation":"add","media_type":"application/sparql-query","sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","content_base64":"U0VMRUNUID9uYW1lIFdIRVJFIHsgP3MgPHVybjpleGFtcGxlOm5hbWU+ID9uYW1lIC4gfSBPUkRFUiBCWSA/bmFtZQo=","replaces_sha256":null,"role":null,"case_kind":null}],"rules":[{"category":"rules","asset_id":"engineering-rules","operation":"add","media_type":"text/plain","sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","content_base64":"SUYgb2JzZXJ2ZWQoP3gpIFRIRU4gcmV1c2FibGUoP3gpCg==","replaces_sha256":null,"role":null,"case_kind":null}],"cases":[{"category":"cases","asset_id":"case-positive","operation":"add","media_type":"text/turtle","sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","content_base64":"PHVybjpjYXNlOnBvc2l0aXZlPiA8dXJuOmV4YW1wbGU6bmFtZT4gInBvc2l0aXZlIiAuCg==","replaces_sha256":null,"role":null,"case_kind":"positive"},{"category":"cases","asset_id":"case-negative","operation":"add","media_type":"text/turtle","sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","content_base64":"IyBkZWxpYmVyYXRlbHkgZW1wdHkgbmVnYXRpdmUgZ3JhcGgK","replaces_sha256":null,"role":null,"case_kind":"negative"},{"category":"cases","asset_id":"case-ambiguity","operation":"add","media_type":"text/turtle","sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","content_base64":"PHVybjpjYXNlOmFtYmlndWl0eT4gPHVybjpleGFtcGxlOm5hbWU+ICJhbWJpZ3VpdHkiIC4K","replaces_sha256":null,"role":null,"case_kind":"ambiguity"},{"category":"cases","asset_id":"case-prior_release","operation":"add","media_type":"text/turtle","sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","content_base64":"PHVybjpjYXNlOnByaW9yX3JlbGVhc2U+IDx1cm46ZXhhbXBsZTpuYW1lPiAicHJpb3IiIC4K","replaces_sha256":null,"role":null,"case_kind":"prior_release"}],"contract":[{"category":"contract","asset_id":"package-contract","operation":"add","media_type":"application/json","sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","content_base64":"eyJhc3NldHMiOnsiY2FzZS1hbWJpZ3VpdHkiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNhc2UifSwiY2FzZS1uZWdhdGl2ZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjYXNlLXBvc2l0aXZlIjp7ImZvcm1hdCI6InR1cnRsZSIsImtpbmQiOiJyZGYiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjYXNlIn0sImNhc2UtcHJpb3JfcmVsZWFzZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjcS1wcmltYXJ5Ijp7ImZvcm1hdCI6InNwYXJxbCIsImtpbmQiOiJ0ZXh0IiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic3BhcnFsIn0sImNxLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNvbXBldGVuY3lfcXVlc3Rpb25zIn0sImVuZ2luZWVyaW5nLXJ1bGVzIjp7ImZvcm1hdCI6InRleHQiLCJraW5kIjoidGV4dCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImVuZ2luZWVyaW5nX3J1bGVzIn0sIm9udG9sb2d5LWNvcmUiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0Ijp0cnVlLCJyb2xlIjoib250b2xvZ3kifSwicGFja2FnZS1jb250cmFjdCI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjb250cmFjdCJ9LCJxdWFsaXR5LXNoYXBlcyI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic2hhcGVzIn0sInJpZ2h0cy1ldmlkZW5jZSI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJwcm92ZW5hbmNlIn0sInNjZW5hcmlvLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InNjZW5hcmlvX3JlZ2lzdHJ5In0sInNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIjp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InByb3ZlbmFuY2UifX0sImV4ZWN1dGlvbiI6eyJjcV9yZWdpc3RyeV9hc3NldF9pZCI6ImNxLXJlZ2lzdHJ5Iiwic2NlbmFyaW9fcmVnaXN0cnlfYXNzZXRfaWQiOiJzY2VuYXJpby1yZWdpc3RyeSJ9LCJnYXRlX3N1aXRlIjp7ImNhc2UuYW1iaWd1aXR5Ijp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtYW1iaWd1aXR5Il0sImNxX2lkcyI6W10sInNjZW5hcmlvX2lkcyI6WyJzY2VuYXJpby1hbWJpZ3VpdHkiXX0sImNhc2UubmVnYXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1uZWdhdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tbmVnYXRpdmUiXX0sImNhc2UucG9zaXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1wb3NpdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY2FzZS5wcmlvcl9yZWxlYXNlIjp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtcHJpb3JfcmVsZWFzZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX0sImNxLmN1cnJlbnQiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtY3VycmVudCJdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY3EucHJpb3IiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtcHJpb3IiXSwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLXByaW9yIl19fSwibmFtZXNwYWNlIjoidXJuOmluZHVzdHJ5OmV4YW1wbGU6IiwicHJvdmVuYW5jZV9ldmlkZW5jZV9hc3NldF9pZHMiOlsic291cmNlLWV2aWRlbmNlLWJpbmRpbmciXSwicmVsZWFzZV9zdGF0dXMiOiJjb21wbGV0ZSIsInJpZ2h0c19ldmlkZW5jZV9hc3NldF9pZHMiOlsicmlnaHRzLWV2aWRlbmNlIl0sInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"provenance":[{"category":"provenance","asset_id":"scenario-registry","operation":"add","media_type":"application/json","sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","content_base64":"eyJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjZW5hcmlvcyI6W3siZXhlY3V0aW9uIjp7Im9wZXJhdGlvbiI6InNlbGVjdF9leGFjdF9tdWx0aXNldCIsInByb3ZpZGVyIjoiU2VtYW50aWNSdW50aW1lIiwicXVlcnlfYXNzZXRfaWQiOiJjcS1wcmltYXJ5In0sImlkIjoic2NlbmFyaW8tY3VycmVudCIsImlucHV0cyI6WyJjYXNlLXBvc2l0aXZlIiwiY2FzZS1uZWdhdGl2ZSJdLCJvcmFjbGUiOnsiaWQiOiJvcmFjbGUtc2NlbmFyaW8tY3VycmVudCIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicG9zaXRpdmUiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLW5lZ2F0aXZlIiwiaW5wdXRzIjpbImNhc2UtbmVnYXRpdmUiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1uZWdhdGl2ZSIsInBvc2l0aXZlIjp7InJvd19jb3VudCI6MH0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLWFtYmlndWl0eSIsImlucHV0cyI6WyJjYXNlLWFtYmlndWl0eSIsImNhc2UtbmVnYXRpdmUiXSwib3JhY2xlIjp7ImlkIjoib3JhY2xlLXNjZW5hcmlvLWFtYmlndWl0eSIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsiYW1iaWd1aXR5Il19LCJyb3dfY291bnQiOjF9LCJzaW5nbGVfZmF1bHRfbmVnYXRpdmUiOnsicm93X2NvdW50IjowfSwic3RhdHVzIjoicmVhZHkifSwic3RhdHVzIjoibmF0aXZlIn0seyJleGVjdXRpb24iOnsib3BlcmF0aW9uIjoic2VsZWN0X2V4YWN0X211bHRpc2V0IiwicHJvdmlkZXIiOiJTZW1hbnRpY1J1bnRpbWUiLCJxdWVyeV9hc3NldF9pZCI6ImNxLXByaW1hcnkifSwiaWQiOiJzY2VuYXJpby1wcmlvciIsImlucHV0cyI6WyJjYXNlLXByaW9yX3JlbGVhc2UiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1wcmlvciIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicHJpb3IiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifV0sInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"rights-evidence","operation":"add","media_type":"application/json","sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","content_base64":"eyJzY2hlbWFfdmVyc2lvbiI6IjEuMCIsInNvdXJjZV9pZCI6ImVuZ2luZWVyaW5nLXJlY29yZCIsInNvdXJjZV9zaGEyNTYiOiJhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhIiwic3RhdGVtZW50IjoiUmlnaHRzIGV2aWRlbmNlIHJldGFpbmVkOyBubyBsZWdhbCBjb25jbHVzaW9uLiJ9","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"source-evidence-binding","operation":"add","media_type":"application/json","sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","content_base64":"eyJraW5kIjoic2VtYW50aWNhLnNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIiwic2NoZW1hX3ZlcnNpb24iOiIxLjAiLCJzb3VyY2VfZXZpZGVuY2UiOlt7ImNhcHR1cmVkX2F0IjoiMjAyNi0wOC0xOVQwMDowMDowMFoiLCJtZWRpYV90eXBlIjoiYXBwbGljYXRpb24vanNvbiIsInNoYTI1NiI6ImFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWEiLCJzb3VyY2VfaWQiOiJlbmdpbmVlcmluZy1yZWNvcmQiLCJ1cmkiOiJ1cm46ZXhhbXBsZTpldmlkZW5jZS1yb290OmVuZ2luZWVyaW5nLXJlY29yZCJ9XX0=","replaces_sha256":null,"role":null,"case_kind":null}],"delta_sha256":"065191b22bf26c7a6d3557e903ed9460028cdc8c1695713ecaac2004552e3960"}},{"case_id":"original-delta-declared-hash-tamper","outcome":{"status":"raised","exception":"RefineryInputError","message":"delta_sha256 does not match content","violations":null},"input":{"schema_version":"1.0","package_id":"industry.example","base_version":"0","base_package_sha256":"fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947","target_version":"1.0.0","rationale":"Repeated engineering practice merits a governed candidate.","created_by":"fact.authority","created_at":"2026-08-19T00:00:00Z","required_capabilities":["semantic.package.load"],"source_evidence":[{"source_id":"engineering-record","uri":"urn:example:evidence-root:engineering-record","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","media_type":"application/json","captured_at":"2026-08-19T00:00:00Z"}],"book_impact":"none","ontology":[{"category":"ontology","asset_id":"ontology-core","operation":"add","media_type":"text/turtle","sha256":"64f5571d124b2966362c6ddea84537c37cc2c7768fa75be41aa325f01324e324","content_base64":"QHByZWZpeCBvd2w6IDxodHRwOi8vd3d3LnczLm9yZy8yMDAyLzA3L293bCM+IC4KPHVybjppbmR1c3RyeTpleGFtcGxlPiBhIG93bDpPbnRvbG9neSAuCg==","replaces_sha256":null,"role":null,"case_kind":null}],"competency_questions":[{"category":"competency_questions","asset_id":"cq-registry","operation":"add","media_type":"application/json","sha256":"2784bd43cc5c61668501b7b3a2c528b17aa0dfb6b7f4e3df91e01175091cf752","content_base64":"eyJjb21wZXRlbmN5X3F1ZXN0aW9ucyI6W3siaWQiOiJjcS1jdXJyZW50IiwicXVlc3Rpb24iOiJDYW4gdGhlIGN1cnJlbnQgcmV1c2FibGUgY2FzZSBiZSByZXRyaWV2ZWQ/Iiwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLWN1cnJlbnQiXX0seyJpZCI6ImNxLXByaW9yIiwicXVlc3Rpb24iOiJEb2VzIHRoZSBwcmlvci1yZWxlYXNlIGNhc2Ugc3RpbGwgZXhlY3V0ZT8iLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX1dLCJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"shapes":[{"category":"shapes","asset_id":"quality-shapes","operation":"add","media_type":"text/turtle","sha256":"ebe79ac03457974d6323028ee1a0db19de3ace63fe48314e30b924d6f5ae1cf8","content_base64":"QHByZWZpeCBzaDogPGh0dHA6Ly93d3cudzMub3JnL25zL3NoYWNsIz4gLgpAcHJlZml4IGV4OiA8dXJuOmV4YW1wbGU6PiAuCmV4Ok5hbWVkU2hhcGUgYSBzaDpOb2RlU2hhcGUgOyBzaDp0YXJnZXRTdWJqZWN0c09mIGV4Om5hbWUgLgo=","replaces_sha256":null,"role":null,"case_kind":null}],"queries":[{"category":"queries","asset_id":"cq-primary","operation":"add","media_type":"application/sparql-query","sha256":"bf826448f1e44c3cfa964fa185d95318ae087d1f82ff29683474663aa9376968","content_base64":"U0VMRUNUID9uYW1lIFdIRVJFIHsgP3MgPHVybjpleGFtcGxlOm5hbWU+ID9uYW1lIC4gfSBPUkRFUiBCWSA/bmFtZQo=","replaces_sha256":null,"role":null,"case_kind":null}],"rules":[{"category":"rules","asset_id":"engineering-rules","operation":"add","media_type":"text/plain","sha256":"c4e56cedfe6a7ecbc9e6f7f4cafc10d67caa90ffa7b4bcb7a3a664756d6cbfe9","content_base64":"SUYgb2JzZXJ2ZWQoP3gpIFRIRU4gcmV1c2FibGUoP3gpCg==","replaces_sha256":null,"role":null,"case_kind":null}],"cases":[{"category":"cases","asset_id":"case-positive","operation":"add","media_type":"text/turtle","sha256":"94524f7e21d2232ee10d966bd6640a860a8f36b6f934edeffbb0845d682786ed","content_base64":"PHVybjpjYXNlOnBvc2l0aXZlPiA8dXJuOmV4YW1wbGU6bmFtZT4gInBvc2l0aXZlIiAuCg==","replaces_sha256":null,"role":null,"case_kind":"positive"},{"category":"cases","asset_id":"case-negative","operation":"add","media_type":"text/turtle","sha256":"d4b504a3a6363c8f915ec07f80f529d725e03889d13deac0dffb2f2149f9d466","content_base64":"IyBkZWxpYmVyYXRlbHkgZW1wdHkgbmVnYXRpdmUgZ3JhcGgK","replaces_sha256":null,"role":null,"case_kind":"negative"},{"category":"cases","asset_id":"case-ambiguity","operation":"add","media_type":"text/turtle","sha256":"25ea7fe3cf53cc198f4b3771ef2a05f103a3ea682e33dd94151e2de3a0c8b2ac","content_base64":"PHVybjpjYXNlOmFtYmlndWl0eT4gPHVybjpleGFtcGxlOm5hbWU+ICJhbWJpZ3VpdHkiIC4K","replaces_sha256":null,"role":null,"case_kind":"ambiguity"},{"category":"cases","asset_id":"case-prior_release","operation":"add","media_type":"text/turtle","sha256":"88f250c703bee25ad3dfe57d9483d34e06a35f6e6fb2c3667ddfe1e33cc88c13","content_base64":"PHVybjpjYXNlOnByaW9yX3JlbGVhc2U+IDx1cm46ZXhhbXBsZTpuYW1lPiAicHJpb3IiIC4K","replaces_sha256":null,"role":null,"case_kind":"prior_release"}],"contract":[{"category":"contract","asset_id":"package-contract","operation":"add","media_type":"application/json","sha256":"7b37ee915cad0c136637eddcd363757d56510b6a660239761eec76ba95828da0","content_base64":"eyJhc3NldHMiOnsiY2FzZS1hbWJpZ3VpdHkiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNhc2UifSwiY2FzZS1uZWdhdGl2ZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjYXNlLXBvc2l0aXZlIjp7ImZvcm1hdCI6InR1cnRsZSIsImtpbmQiOiJyZGYiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjYXNlIn0sImNhc2UtcHJpb3JfcmVsZWFzZSI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoiY2FzZSJ9LCJjcS1wcmltYXJ5Ijp7ImZvcm1hdCI6InNwYXJxbCIsImtpbmQiOiJ0ZXh0IiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic3BhcnFsIn0sImNxLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImNvbXBldGVuY3lfcXVlc3Rpb25zIn0sImVuZ2luZWVyaW5nLXJ1bGVzIjp7ImZvcm1hdCI6InRleHQiLCJraW5kIjoidGV4dCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6ImVuZ2luZWVyaW5nX3J1bGVzIn0sIm9udG9sb2d5LWNvcmUiOnsiZm9ybWF0IjoidHVydGxlIiwia2luZCI6InJkZiIsImxvYWRfaW50b19kYXRhc2V0Ijp0cnVlLCJyb2xlIjoib250b2xvZ3kifSwicGFja2FnZS1jb250cmFjdCI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJjb250cmFjdCJ9LCJxdWFsaXR5LXNoYXBlcyI6eyJmb3JtYXQiOiJ0dXJ0bGUiLCJraW5kIjoicmRmIiwibG9hZF9pbnRvX2RhdGFzZXQiOmZhbHNlLCJyb2xlIjoic2hhcGVzIn0sInJpZ2h0cy1ldmlkZW5jZSI6eyJmb3JtYXQiOiJqc29uIiwia2luZCI6InN0cnVjdHVyZWQiLCJsb2FkX2ludG9fZGF0YXNldCI6ZmFsc2UsInJvbGUiOiJwcm92ZW5hbmNlIn0sInNjZW5hcmlvLXJlZ2lzdHJ5Ijp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InNjZW5hcmlvX3JlZ2lzdHJ5In0sInNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIjp7ImZvcm1hdCI6Impzb24iLCJraW5kIjoic3RydWN0dXJlZCIsImxvYWRfaW50b19kYXRhc2V0IjpmYWxzZSwicm9sZSI6InByb3ZlbmFuY2UifX0sImV4ZWN1dGlvbiI6eyJjcV9yZWdpc3RyeV9hc3NldF9pZCI6ImNxLXJlZ2lzdHJ5Iiwic2NlbmFyaW9fcmVnaXN0cnlfYXNzZXRfaWQiOiJzY2VuYXJpby1yZWdpc3RyeSJ9LCJnYXRlX3N1aXRlIjp7ImNhc2UuYW1iaWd1aXR5Ijp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtYW1iaWd1aXR5Il0sImNxX2lkcyI6W10sInNjZW5hcmlvX2lkcyI6WyJzY2VuYXJpby1hbWJpZ3VpdHkiXX0sImNhc2UubmVnYXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1uZWdhdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tbmVnYXRpdmUiXX0sImNhc2UucG9zaXRpdmUiOnsiY2FzZV9hc3NldF9pZHMiOlsiY2FzZS1wb3NpdGl2ZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY2FzZS5wcmlvcl9yZWxlYXNlIjp7ImNhc2VfYXNzZXRfaWRzIjpbImNhc2UtcHJpb3JfcmVsZWFzZSJdLCJjcV9pZHMiOltdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tcHJpb3IiXX0sImNxLmN1cnJlbnQiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtY3VycmVudCJdLCJzY2VuYXJpb19pZHMiOlsic2NlbmFyaW8tY3VycmVudCJdfSwiY3EucHJpb3IiOnsiY2FzZV9hc3NldF9pZHMiOltdLCJjcV9pZHMiOlsiY3EtcHJpb3IiXSwic2NlbmFyaW9faWRzIjpbInNjZW5hcmlvLXByaW9yIl19fSwibmFtZXNwYWNlIjoidXJuOmluZHVzdHJ5OmV4YW1wbGU6IiwicHJvdmVuYW5jZV9ldmlkZW5jZV9hc3NldF9pZHMiOlsic291cmNlLWV2aWRlbmNlLWJpbmRpbmciXSwicmVsZWFzZV9zdGF0dXMiOiJjb21wbGV0ZSIsInJpZ2h0c19ldmlkZW5jZV9hc3NldF9pZHMiOlsicmlnaHRzLWV2aWRlbmNlIl0sInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null}],"provenance":[{"category":"provenance","asset_id":"scenario-registry","operation":"add","media_type":"application/json","sha256":"4f81d5749e7a5d60cc36047987a8709c691591a978de9713b74f482ad985bc6b","content_base64":"eyJwYWNrYWdlX2lkIjoiaW5kdXN0cnkuZXhhbXBsZSIsInJ1bm5lcl9jb250cmFjdCI6InNlbWFudGljYS5jaGFwdGVyX3BhY2thZ2VzLlNlbWFudGljUGFja2FnZVJ1bm5lci92MSIsInNjZW5hcmlvcyI6W3siZXhlY3V0aW9uIjp7Im9wZXJhdGlvbiI6InNlbGVjdF9leGFjdF9tdWx0aXNldCIsInByb3ZpZGVyIjoiU2VtYW50aWNSdW50aW1lIiwicXVlcnlfYXNzZXRfaWQiOiJjcS1wcmltYXJ5In0sImlkIjoic2NlbmFyaW8tY3VycmVudCIsImlucHV0cyI6WyJjYXNlLXBvc2l0aXZlIiwiY2FzZS1uZWdhdGl2ZSJdLCJvcmFjbGUiOnsiaWQiOiJvcmFjbGUtc2NlbmFyaW8tY3VycmVudCIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicG9zaXRpdmUiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLW5lZ2F0aXZlIiwiaW5wdXRzIjpbImNhc2UtbmVnYXRpdmUiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1uZWdhdGl2ZSIsInBvc2l0aXZlIjp7InJvd19jb3VudCI6MH0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifSx7ImV4ZWN1dGlvbiI6eyJvcGVyYXRpb24iOiJzZWxlY3RfZXhhY3RfbXVsdGlzZXQiLCJwcm92aWRlciI6IlNlbWFudGljUnVudGltZSIsInF1ZXJ5X2Fzc2V0X2lkIjoiY3EtcHJpbWFyeSJ9LCJpZCI6InNjZW5hcmlvLWFtYmlndWl0eSIsImlucHV0cyI6WyJjYXNlLWFtYmlndWl0eSIsImNhc2UtbmVnYXRpdmUiXSwib3JhY2xlIjp7ImlkIjoib3JhY2xlLXNjZW5hcmlvLWFtYmlndWl0eSIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsiYW1iaWd1aXR5Il19LCJyb3dfY291bnQiOjF9LCJzaW5nbGVfZmF1bHRfbmVnYXRpdmUiOnsicm93X2NvdW50IjowfSwic3RhdHVzIjoicmVhZHkifSwic3RhdHVzIjoibmF0aXZlIn0seyJleGVjdXRpb24iOnsib3BlcmF0aW9uIjoic2VsZWN0X2V4YWN0X211bHRpc2V0IiwicHJvdmlkZXIiOiJTZW1hbnRpY1J1bnRpbWUiLCJxdWVyeV9hc3NldF9pZCI6ImNxLXByaW1hcnkifSwiaWQiOiJzY2VuYXJpby1wcmlvciIsImlucHV0cyI6WyJjYXNlLXByaW9yX3JlbGVhc2UiLCJjYXNlLW5lZ2F0aXZlIl0sIm9yYWNsZSI6eyJpZCI6Im9yYWNsZS1zY2VuYXJpby1wcmlvciIsInBvc2l0aXZlIjp7ImJpbmRpbmdzIjp7Im5hbWUiOlsicHJpb3IiXX0sInJvd19jb3VudCI6MX0sInNpbmdsZV9mYXVsdF9uZWdhdGl2ZSI6eyJyb3dfY291bnQiOjB9LCJzdGF0dXMiOiJyZWFkeSJ9LCJzdGF0dXMiOiJuYXRpdmUifV0sInNjaGVtYV92ZXJzaW9uIjoiMS4wIn0=","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"rights-evidence","operation":"add","media_type":"application/json","sha256":"41c4fe7c9cf3f6f17a1663364fec3e857e7c06a6099aa59642a5677cb6b42c1a","content_base64":"eyJzY2hlbWFfdmVyc2lvbiI6IjEuMCIsInNvdXJjZV9pZCI6ImVuZ2luZWVyaW5nLXJlY29yZCIsInNvdXJjZV9zaGEyNTYiOiJhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhIiwic3RhdGVtZW50IjoiUmlnaHRzIGV2aWRlbmNlIHJldGFpbmVkOyBubyBsZWdhbCBjb25jbHVzaW9uLiJ9","replaces_sha256":null,"role":null,"case_kind":null},{"category":"provenance","asset_id":"source-evidence-binding","operation":"add","media_type":"application/json","sha256":"1e632558d705453e1f0ca6d4203022fe00c2479b4419709cbe1678a410839e90","content_base64":"eyJraW5kIjoic2VtYW50aWNhLnNvdXJjZS1ldmlkZW5jZS1iaW5kaW5nIiwic2NoZW1hX3ZlcnNpb24iOiIxLjAiLCJzb3VyY2VfZXZpZGVuY2UiOlt7ImNhcHR1cmVkX2F0IjoiMjAyNi0wOC0xOVQwMDowMDowMFoiLCJtZWRpYV90eXBlIjoiYXBwbGljYXRpb24vanNvbiIsInNoYTI1NiI6ImFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWEiLCJzb3VyY2VfaWQiOiJlbmdpbmVlcmluZy1yZWNvcmQiLCJ1cmkiOiJ1cm46ZXhhbXBsZTpldmlkZW5jZS1yb290OmVuZ2luZWVyaW5nLXJlY29yZCJ9XX0=","replaces_sha256":null,"role":null,"case_kind":null}],"delta_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}}],"scope":"Original public synthetic fixtures and original outputs; memory store cannot assert filesystem mode/platform, observed candidate is not engineering adoption; full22/19/13494 remain separate"}"####;

const TIMESTAMP_PROJECTION_SHA256: &str =
    "32f88dbc78b665598ea65a3d941283b4bd322ca340c8d326f5ddf34da8b6d81b";
const TIMESTAMP_JSON: &str = r####"{"schema":"semantica.refinery.original-rust-timestamp-projection/1","original_receipt_sha256":"03c19fe90a96011e61290b4b25d44f37c7467b1450c5762926d6b97e0e6ec0b1","fixed_forecast_sha256":"59c149d8a0119e7adf998f7eef8cb1ca9a53576bb9e4f0b775b9bc54b820d465","python_version":"3.13.11 (main, Dec  5 2025, 16:06:33) [Clang 21.1.7 ]","observations":[{"id":"timestamp-00","input":"2026-08-19T00:00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-01","input":"2026-08-19T00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-02","input":"2026-08-19T00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-03","input":"2026-08-19 00:00:00+0000","accepted":true,"actual_accepted":true},{"id":"timestamp-04","input":"20260819T000000+00:00","accepted":true,"actual_accepted":true},{"id":"timestamp-05","input":"2026-W34-3T00:00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-06","input":"2026W343T000000Z","accepted":true,"actual_accepted":true},{"id":"timestamp-07","input":"2026-W34T00:00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-08","input":"2026W34T00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-09","input":"2026-08-19X00:00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-10","input":"2026-08-19😀00:00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-11","input":"2026-08-19T00:00:00+00:00:30","accepted":true,"actual_accepted":true},{"id":"timestamp-12","input":"2026-08-19T00:00:00+00:00:00.1","accepted":true,"actual_accepted":true},{"id":"timestamp-13","input":"2026-08-19T00:00:00+00.5","accepted":true,"actual_accepted":true},{"id":"timestamp-14","input":"2026-08-19T23:59:59.999999999999+23:59:59.9","accepted":true,"actual_accepted":true},{"id":"timestamp-15","input":"2026-08-19T00:00:00+00:99:99","accepted":true,"actual_accepted":true},{"id":"timestamp-16","input":"2026-08-19T00,5Z","accepted":true,"actual_accepted":true},{"id":"timestamp-17","input":"2026-08-19T0000.5-0000","accepted":true,"actual_accepted":true},{"id":"timestamp-18","input":"0001-01-01T00:00:00Z","accepted":true,"actual_accepted":true},{"id":"timestamp-19","input":"9999-12-31T23:59:59-23:59","accepted":true,"actual_accepted":true},{"id":"timestamp-20","input":"2020-W53-7T00:00+00","accepted":true,"actual_accepted":true},{"id":"timestamp-21","input":"2026-08-19T12:30:00-00:00:61","accepted":true,"actual_accepted":true},{"id":"timestamp-22","input":"2026-08-19T00:00:60Z","accepted":false,"actual_accepted":false},{"id":"timestamp-23","input":"2026-08-19T24:00:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-24","input":"2026-08-19T00:60:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-25","input":"2026-08-19T00:00:00","accepted":false,"actual_accepted":false},{"id":"timestamp-26","input":"2026-08-19","accepted":false,"actual_accepted":false},{"id":"timestamp-27","input":"2026-08-19T00:00:00+24","accepted":false,"actual_accepted":false},{"id":"timestamp-28","input":"2026-08-19T00:00:00+23:99","accepted":false,"actual_accepted":false},{"id":"timestamp-29","input":"2026-08-19T00:00:00Z00","accepted":false,"actual_accepted":false},{"id":"timestamp-30","input":"2026-08-19T00:00:00+0","accepted":false,"actual_accepted":false},{"id":"timestamp-31","input":"2026-08-19T00:00:00+000","accepted":false,"actual_accepted":false},{"id":"timestamp-32","input":"2026-08-19T00:00:00+00:","accepted":false,"actual_accepted":false},{"id":"timestamp-33","input":"2026-08-19T00:00:00+00:00:","accepted":false,"actual_accepted":false},{"id":"timestamp-34","input":"2026-08-19T00:00:00+00:0000","accepted":false,"actual_accepted":false},{"id":"timestamp-35","input":"2026-08-19T00:00:00+0000:00","accepted":false,"actual_accepted":false},{"id":"timestamp-36","input":"2026-08-19T00:00:00. Z","accepted":false,"actual_accepted":false},{"id":"timestamp-37","input":"2026-08-19T00:00:00Z ","accepted":false,"actual_accepted":false},{"id":"timestamp-38","input":" 2026-08-19T00:00:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-39","input":"2026-08-19T00:00:00z","accepted":false,"actual_accepted":false},{"id":"timestamp-40","input":"0000-01-01T00:00:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-41","input":"2026-02-29T00:00:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-42","input":"2026-W53-1T00:00:00Z","accepted":false,"actual_accepted":true},{"id":"timestamp-43","input":"9999-W52-7T00:00:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-44","input":"2026-W34-0T00:00:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-45","input":"2026-08-19T0Z","accepted":false,"actual_accepted":false},{"id":"timestamp-46","input":"2026-08-19T000Z","accepted":false,"actual_accepted":true},{"id":"timestamp-47","input":"2026-08-19T00:0000Z","accepted":false,"actual_accepted":false},{"id":"timestamp-48","input":"2026-08-19T0000:00Z","accepted":false,"actual_accepted":false},{"id":"timestamp-49","input":"2026-08-19T00.Z","accepted":false,"actual_accepted":true},{"id":"timestamp-50","input":null,"accepted":false,"actual_accepted":false},{"id":"timestamp-51","input":false,"accepted":false,"actual_accepted":false},{"id":"timestamp-52","input":5,"accepted":false,"actual_accepted":false}],"fixed_forecast_matched":50,"fixed_forecast_cases":53,"scope":"Actual pinned original pure helpers on Python3.13.11; fixed50/3 forecast distinction unchanged; no fullmodule completion"}"####;
