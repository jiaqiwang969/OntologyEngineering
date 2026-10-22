//! Independent unmodified original 16-case oracle and complete seven stage
//! bytes. Legacy content replay is not current Rust execution authority.
use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use anyhow::{Context, Result, ensure};
use semantica_runtime::{
    refinery::{RefineryEngine, RefineryStore},
    refinery_subject,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Mutex,
};

const SHA: &str = "002b312aaa020ea2782113c4a614f5dfc579b8b2f1d85426b9f72026e3fba99d";
const DELTA: &str = "e2ebd4fd3fa591620cdcfa3942172d9108a3488670eb604bbff0c75aecb6896d";
const WHEN: &str = "2026-08-19T00:00:00Z";
#[derive(Default)]
struct Store {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    dirs: Mutex<BTreeSet<String>>,
}
fn parent(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |v| v.0)
}
impl Store {
    fn stage(name: &str) -> Result<Self> {
        let result = Self::default();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../original/checkpoints")
            .join(name);
        result.dirs.lock().unwrap().insert("".into());
        result.copy(&root, &root)?;
        for initial in [
            "registry-events",
            "packages",
            "engagements",
            "objects",
            "objects/sha256",
            "execution-suites",
            "refinements",
        ] {
            result.dirs.lock().unwrap().insert(initial.into());
        }
        Ok(result)
    }
    fn copy(&self, root: &Path, path: &Path) -> Result<()> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            let relative = p
                .strip_prefix(root)?
                .to_str()
                .context("utf8 path")?
                .replace('\\', "/");
            if p.is_dir() {
                self.dirs.lock().unwrap().insert(relative);
                self.copy(root, &p)?;
            } else {
                ensure!(
                    p.symlink_metadata()?.is_file(),
                    "original fixture regular file"
                );
                self.files
                    .lock()
                    .unwrap()
                    .insert(relative, std::fs::read(p)?);
            }
        }
        Ok(())
    }
    fn saved(&self) -> BTreeMap<String, Vec<u8>> {
        self.files.lock().unwrap().clone()
    }
    fn object(&self, sha: &str) -> Value {
        serde_json::from_slice(
            self.files
                .lock()
                .unwrap()
                .get(&format!("objects/sha256/{}/{sha}", &sha[..2]))
                .unwrap(),
        )
        .unwrap()
    }
    fn fault(&self, path: &str, bytes: Vec<u8>) {
        assert!(
            self.files
                .lock()
                .unwrap()
                .insert(path.into(), bytes)
                .is_some()
        );
    }
}
impl RefineryStore for Store {
    fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.files.lock().unwrap().get(p).cloned())
    }
    fn write_new(&self, p: &str, b: &[u8]) -> Result<()> {
        ensure!(
            self.dirs.lock().unwrap().contains(parent(p)),
            "parent missing"
        );
        let mut files = self.files.lock().unwrap();
        ensure!(!files.contains_key(p), "file exists");
        files.insert(p.into(), b.into());
        Ok(())
    }
    fn replace(&self, p: &str, e: &[u8], b: &[u8]) -> Result<()> {
        let mut f = self.files.lock().unwrap();
        ensure!(f.get(p).is_some_and(|v| v == e), "CAS differs");
        f.insert(p.into(), b.into());
        Ok(())
    }
    fn remove(&self, p: &str, e: &[u8]) -> Result<()> {
        let mut f = self.files.lock().unwrap();
        ensure!(f.get(p).is_some_and(|v| v == e), "CAS differs");
        f.remove(p);
        Ok(())
    }
    fn list(&self, p: &str) -> Result<Vec<String>> {
        ensure!(self.dirs.lock().unwrap().contains(p), "directory missing");
        let prefix = format!("{p}/");
        let files = self.files.lock().unwrap();
        let dirs = self.dirs.lock().unwrap();
        Ok(files
            .keys()
            .chain(dirs.iter())
            .filter_map(|s| s.strip_prefix(&prefix))
            .filter(|s| !s.is_empty() && !s.contains('/'))
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
    fn create_dir(&self, p: &str) -> Result<()> {
        let mut dirs = self.dirs.lock().unwrap();
        ensure!(
            dirs.contains(parent(p)) && dirs.insert(p.into()),
            "directory conflict"
        );
        Ok(())
    }
    fn publish_dir(&self, stage: &str, dst: &str) -> Result<()> {
        let mut dirs = self.dirs.lock().unwrap();
        ensure!(
            !dirs.contains(dst) && dirs.contains(stage),
            "publish conflict"
        );
        let prefix = format!("{stage}/");
        let moved = dirs
            .iter()
            .filter(|p| p.as_str() == stage || p.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        for p in moved {
            dirs.remove(&p);
            dirs.insert(format!("{dst}{}", &p[stage.len()..]));
        }
        let mut files = self.files.lock().unwrap();
        let moved = files
            .keys()
            .filter(|p| p.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        for p in moved {
            let b = files.remove(&p).unwrap();
            files.insert(format!("{dst}{}", &p[stage.len()..]), b);
        }
        Ok(())
    }
    fn discard_stage(&self, stage: &str) -> Result<()> {
        self.dirs
            .lock()
            .unwrap()
            .retain(|p| p != stage && !p.starts_with(&format!("{stage}/")));
        self.files
            .lock()
            .unwrap()
            .retain(|p, _| !p.starts_with(&format!("{stage}/")));
        Ok(())
    }
}
fn original() -> Value {
    let raw_fixture: &str =
        &private_test_fixtures::text("original/full12/ORIGINAL-FULL12-ORACLE.json");
    assert_eq!(semantica_runtime::sha256(raw_fixture), SHA);
    let v = semantica_json::strict_json(raw_fixture.as_bytes()).unwrap();
    assert_eq!(v["case_count"], 16);
    assert_eq!(v["business_stubs"], false);
    v
}
fn positive(f: &Value) -> &Value {
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case_id"] == "full-positive-six-states-twelve-gates")
        .unwrap()
}
fn engine(store: &Store) -> Result<RefineryEngine<'_>> {
    RefineryEngine::new(store, "registry-original-full12")
}
fn ctx(store: &Store, evidence: &Value) -> Value {
    store.object(
        evidence["transition_context_object_sha256"]
            .as_str()
            .unwrap(),
    )
}

#[test]
fn original_seven_complete_stage_bytes_replay_all_six_states() -> Result<()> {
    let _ = original();
    for (name, count, state) in [
        ("candidate", 23, "candidate"),
        ("proposed", 25, "proposed"),
        ("committed", 29, "committed"),
        (
            "committed-with-suite-and-derived-regression",
            49,
            "committed",
        ),
        ("regression_passed", 52, "regression_passed"),
        ("release_complete", 63, "release_complete"),
        ("promoted", 69, "promoted"),
    ] {
        let store = Store::stage(name)?;
        let before = store.saved();
        assert_eq!(before.len(), count);
        let e = engine(&store)?;
        let status = e.status(DELTA)?;
        let mut expected: Value = serde_json::from_slice(
            before
                .get(&format!("refinements/{DELTA}/current.json"))
                .unwrap(),
        )?;
        expected.as_object_mut().unwrap().remove("schema_version");
        assert_eq!(status, expected, "stage {name}");
        assert_eq!(status["state"], state);
        let history = e.history(DELTA)?;
        assert_eq!(history.len(), status["sequence"].as_u64().unwrap() as usize);
        assert_eq!(
            store.saved(),
            before,
            "read replay preserves every original raw byte: {name}"
        );
    }
    Ok(())
}
#[test]
fn original_regression_and_release_derivation_match_full_bytes() -> Result<()> {
    let f = original();
    let p = positive(&f);
    for (stage, kind) in [
        ("committed-with-suite-and-derived-regression", "regression"),
        ("regression_passed", "release"),
    ] {
        let store = Store::stage(stage)?;
        let e = engine(&store)?;
        let expected = &p[kind];
        let context = if kind == "regression" {
            ctx(&store, expected)
        } else {
            let release = Store::stage("release_complete")?;
            ctx(&release, expected)
        };
        let actual = e.derive_gate(
            DELTA,
            kind,
            p["suite"]["suite_sha256"].as_str().unwrap(),
            &context,
            WHEN,
        )?;
        assert_eq!(actual, *expected, "full original {kind} output");
        for check in actual["checks"].as_array().unwrap() {
            let digest = check["output_sha256"].as_str().unwrap();
            let output = store.object(digest);
            assert_eq!(output["passed"], true);
        }
        assert_eq!(actual["checks"].as_array().unwrap().len(), 6);
    }
    Ok(())
}
#[test]
fn original_release_restart_retry_loads_recorded_gates_without_writes() -> Result<()> {
    let f = original();
    let p = positive(&f);
    let store = Store::stage("release_complete")?;
    let before = store.saved();
    let e = engine(&store)?;
    let derive = ctx(&store, &p["release"]);
    let history = e.history(DELTA)?;
    let event = history.last().unwrap();
    let record = store.object(
        event["payload"]["transition_context_object_sha256"]
            .as_str()
            .unwrap(),
    );
    let actual = e.verify_candidate_data(
        DELTA,
        p["suite"]["suite_sha256"].as_str().unwrap(),
        None,
        None,
        &derive,
        &record,
        WHEN,
    )?;
    let expected = &f["cases"][1]["verification"];
    assert_eq!(actual, *expected);
    assert_eq!(
        e.verify_candidate_data(
            DELTA,
            p["suite"]["suite_sha256"].as_str().unwrap(),
            None,
            None,
            &derive,
            &record,
            WHEN
        )?,
        actual
    );
    assert_eq!(store.saved(), before);
    Ok(())
}
#[test]
fn original_same_size_CAS_tamper_and_forged_outer_gate_reject() -> Result<()> {
    let f = original();
    let p = positive(&f);
    let store = Store::stage("committed-with-suite-and-derived-regression")?;
    let e = engine(&store)?;
    e.verify_suite_content(&p["suite"])?;
    let sha = p["suite"]["runs"][0]["run_result_object_sha256"]
        .as_str()
        .unwrap();
    let path = format!("objects/sha256/{}/{sha}", &sha[..2]);
    let mut bytes = store.saved()[&path].clone();
    bytes[0] = b'[';
    store.fault(&path, bytes);
    assert!(e.verify_suite_content(&p["suite"]).is_err());
    let store = Store::stage("committed-with-suite-and-derived-regression")?;
    let e = engine(&store)?;
    let mut forged = p["regression"].clone();
    forged["checks"][0]["output_sha256"] = json!("0".repeat(64));
    let mut content = forged.clone();
    content.as_object_mut().unwrap().remove("evidence_sha256");
    content
        .as_object_mut()
        .unwrap()
        .remove("missing_capabilities");
    forged["evidence_sha256"] = json!(semantica_runtime::refinery::hash(&content)?);
    refinery_subject::gate(&forged)?;
    let context_store = Store::stage("regression_passed")?;
    let history = engine(&context_store)?.history(DELTA)?;
    let context = context_store.object(
        history.last().unwrap()["payload"]["transition_context_object_sha256"]
            .as_str()
            .unwrap(),
    );
    assert!(e.record_gate(DELTA, &forged, &context, WHEN).is_err());
    assert_eq!(e.status(DELTA)?["state"], "committed");
    Ok(())
}

struct NoExternalInput;
impl semantica_runtime::books::SourceProvider for NoExternalInput {
    fn read(&self, _: &str) -> Result<Vec<u8>> {
        anyhow::bail!(
            "test reference is fully embedded; external input would change its frozen scope"
        )
    }
}
fn current_expected() -> Value {
    let expected_raw_fixture: &str =
        &private_test_fixtures::text("FROZEN-CURRENT-EXPECTATIONS-01.json");
    assert_eq!(
        semantica_runtime::sha256(expected_raw_fixture),
        "519b0055644405e37e02daf60b2e12df6085456bd56ba7534d4638db931a95e6"
    );
    serde_json::from_str(expected_raw_fixture).unwrap()
}
fn plan() -> Value {
    let plan_raw_fixture: &str =
        &private_test_fixtures::text("original/full12/ORIGINAL-FULL12-FIXED-PLAN.json");
    assert_eq!(
        semantica_runtime::sha256(plan_raw_fixture),
        "e364a710b4921a17c27360e325e21c9a26a931c45c6c49ccaf3cfd2d0e624f41"
    );
    serde_json::from_str(plan_raw_fixture).unwrap()
}
fn new_store() -> Result<Store> {
    let store = Store::default();
    store.dirs.lock().unwrap().insert("".into());
    RefineryEngine::initialize(&store, "registry-original-full12", WHEN)?;
    Ok(store)
}
fn current_source() -> Result<Value> {
    // This catalog is an independently supplied test input, not evidence of
    // how the executable was compiled. Standalone four-crate receivers have
    // their own Cargo manifest/lock. An explicit read-only source bundle keeps
    // every original source-byte check and the actual current executable hash.
    // The private receiver must freeze-check the bundle before and after tests.
    let root = std::env::var_os("SEMANTICA_TEST_SOURCE_BUNDLE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    ensure!(
        root.is_absolute(),
        "test source bundle root must be absolute"
    );
    let root = root.canonicalize()?;
    let generation: Value =
        serde_json::from_slice(&std::fs::read(root.join("generated/generation.json"))?)?;
    let sources = &generation["source_inputs"];
    for (path, digest) in sources.as_object().context("generated source input map")? {
        assert_eq!(
            semantica_runtime::sha256(std::fs::read(root.join(path))?),
            digest.as_str().unwrap(),
            "actual declared source byte: {path}"
        );
    }
    let exe = std::fs::read(std::env::current_exe()?)?;
    Ok(
        json!({"schema":"semantica.refinery-rust-source/1","source_sha256":semantica_runtime::refinery::hash(sources)?,"executable_sha256":semantica_runtime::sha256(&exe),"executable_size_bytes":exe.len(),"status":"source_bound","compilation_origin":"unknown","engineering_adoption":"unknown"}),
    )
}
fn full_reference<'a>(
    e: &'a RefineryEngine<'_>,
    p: &Value,
    delta: &Value,
    source: &Value,
) -> Result<semantica_runtime::refinery_engagement::CurrentEngagement<'a>> {
    let scenarios = p["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    e.engage_current(&NoExternalInput,&p["reference_execution_manifest"],&scenarios,&p["task"],&p["binding"],&json!({"status":"candidate","rationale":delta["rationale"],"delta_sha256":delta["delta_sha256"]}),source,WHEN)
}
fn ids(value: &Value) -> Value {
    json!(
        value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["check_id"].clone())
            .collect::<Vec<_>>()
    )
}
fn failures(value: &Value) -> Value {
    json!(
        value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["passed"] == false)
            .map(|c| c["check_id"].clone())
            .collect::<Vec<_>>()
    )
}
#[test]
fn actual_current_reference_target_twelve_gates_and_immutable_promotion() -> Result<()> {
    let expected = current_expected();
    let p = plan();
    let source = current_source()?;
    let store = new_store()?;
    let e = engine(&store)?;
    let reference = full_reference(&e, &p, &p["factory_delta"], &source)?;
    assert_eq!(
        reference.observation()["status"],
        expected["positive"]["reference_status"]
    );
    let before = store.saved();
    assert!(
        e.register_data(
            &p["factory_delta"],
            &p["task"],
            &p["binding"],
            reference.observation(),
            WHEN
        )
        .is_err()
    );
    assert!(
        e.propose_data(
            &p["factory_delta"],
            &p["task"],
            &p["contexts"]["proposed"]["envelope"],
            &p["binding"],
            reference.observation(),
            WHEN
        )
        .is_err()
    );
    assert_eq!(store.saved(), before);
    e.propose_current(
        &reference,
        &p["factory_delta"],
        &p["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    let before = store.saved();
    assert!(
        e.propose_context_data(DELTA, &p["contexts"]["proposed"], WHEN)
            .is_err()
    );
    assert!(
        e.commit_data(
            DELTA,
            &p["contexts"]["committed"]["envelope"],
            &p["binding"],
            &p["commit_authorization"],
            WHEN
        )
        .is_err()
    );
    assert_eq!(store.saved(), before);
    e.commit_current(
        &reference,
        DELTA,
        &p["contexts"]["committed"]["envelope"],
        &p["commit_authorization"],
        WHEN,
    )?;
    let executed = e.execute_current(DELTA, &p["contexts"]["execute_candidate"], &source, WHEN)?;
    let suite = executed.observation();
    assert_eq!(suite["status"], expected["positive"]["subject_status"]);
    assert_eq!(suite["runs"].as_array().unwrap().len(), 4);
    let digest = suite["suite_sha256"].as_str().unwrap();
    let regression = e.derive_gate(
        DELTA,
        "regression",
        digest,
        &p["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    assert_eq!(
        regression["status"],
        expected["positive"]["regression_status"]
    );
    assert_eq!(ids(&regression), expected["positive"]["regression_order"]);
    let before = store.saved();
    assert!(
        e.record_gate(
            DELTA,
            &regression,
            &p["contexts"]["regression_passed"],
            WHEN
        )
        .is_err()
    );
    assert!(
        e.verify_candidate_data(
            DELTA,
            digest,
            Some(&regression),
            Some(&p["contexts"]["regression_passed"]),
            &p["contexts"]["derive_release_gate"],
            &p["contexts"]["release_complete"],
            WHEN
        )
        .is_err()
    );
    assert_eq!(store.saved(), before);
    let verified = e.verify_candidate_current(
        &executed,
        Some(&regression),
        Some(&p["contexts"]["regression_passed"]),
        &p["contexts"]["derive_release_gate"],
        &p["contexts"]["release_complete"],
        WHEN,
    )?;
    assert_eq!(
        verified["release_evidence"]["status"],
        expected["positive"]["release_status"]
    );
    assert_eq!(
        ids(&verified["release_evidence"]),
        expected["positive"]["release_order"]
    );
    let before = store.saved();
    assert!(
        e.promote_data(
            DELTA,
            &p["promote_authorization"],
            &p["contexts"]["promoted"],
            WHEN
        )
        .is_err()
    );
    assert_eq!(store.saved(), before);
    let descriptor = e.promote_current(
        &executed,
        &p["promote_authorization"],
        &p["contexts"]["promoted"],
        WHEN,
    )?;
    assert_eq!(descriptor["version"], "1.0.0");
    e.verify_published_refinement(DELTA)?;
    assert_eq!(
        json!(
            e.history(DELTA)?
                .iter()
                .map(|event| event["state"].clone())
                .collect::<Vec<_>>()
        ),
        expected["positive"]["states"]
    );
    let before = store.saved();
    for scenario in p["scenarios"].as_array().unwrap() {
        let run = e.run_registry_current(
            p["factory_delta"]["package_id"].as_str().unwrap(),
            Some("1.0.0"),
            scenario.as_str().unwrap(),
            &source,
            WHEN,
        )?;
        assert_eq!(run["actual_run"]["status"], "passed");
        assert_eq!(
            run["release_content"]["content_verdict"]["status"],
            "complete"
        );
        assert_eq!(
            run["release_content"]["engineering_adoption"],
            expected["positive"]["engineering_adoption"]
        );
    }
    assert_eq!(
        store.saved(),
        before,
        "registry execution does not append history or asset objects"
    );
    Ok(())
}
#[test]
fn actual_target_positive_fault_survives_reference_then_fails_only_original_two_gates() -> Result<()>
{
    let expected = current_expected();
    let p = plan();
    let source = current_source()?;
    let input: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/full12/INPUT-actual-positive-case-single-fault.json",
    ))?;
    let delta = &input["original_delta_variant"];
    let digest = delta["delta_sha256"].as_str().unwrap();
    let store = new_store()?;
    let e = engine(&store)?;
    let reference = full_reference(&e, &p, delta, &source)?;
    assert_eq!(
        reference.observation()["status"],
        expected["positive_single_fault"]["reference_status"]
    );
    e.propose_current(
        &reference,
        delta,
        &input["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    e.commit_current(
        &reference,
        digest,
        &input["contexts"]["committed"]["envelope"],
        &input["authorization"],
        WHEN,
    )?;
    assert_eq!(
        e.status(digest)?["state"],
        expected["positive_single_fault"]["before_subject"]
    );
    let executed = e.execute_current(
        digest,
        &input["contexts"]["execute_candidate"],
        &source,
        WHEN,
    )?;
    assert_eq!(
        executed.observation()["status"],
        expected["positive_single_fault"]["subject_status"]
    );
    let regression = e.derive_gate(
        digest,
        "regression",
        executed.observation()["suite_sha256"].as_str().unwrap(),
        &input["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    assert_eq!(
        regression["status"],
        expected["positive_single_fault"]["regression_status"]
    );
    assert_eq!(
        failures(&regression),
        expected["positive_single_fault"]["failed_regression_ids"]
    );
    assert!(
        e.record_current_gate(
            &executed,
            &regression,
            &input["contexts"]["regression_passed"],
            WHEN
        )
        .is_err()
    );
    assert_eq!(
        e.status(digest)?["state"],
        expected["positive_single_fault"]["after_failed_record"]
    );
    Ok(())
}

fn publish_baseline(e: &RefineryEngine<'_>, p: &Value, source: &Value) -> Result<Value> {
    let reference = full_reference(e, p, &p["factory_delta"], source)?;
    e.propose_current(
        &reference,
        &p["factory_delta"],
        &p["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    e.commit_current(
        &reference,
        DELTA,
        &p["contexts"]["committed"]["envelope"],
        &p["commit_authorization"],
        WHEN,
    )?;
    let executed = e.execute_current(DELTA, &p["contexts"]["execute_candidate"], source, WHEN)?;
    let regression = e.derive_gate(
        DELTA,
        "regression",
        executed.observation()["suite_sha256"].as_str().unwrap(),
        &p["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    e.verify_candidate_current(
        &executed,
        Some(&regression),
        Some(&p["contexts"]["regression_passed"]),
        &p["contexts"]["derive_release_gate"],
        &p["contexts"]["release_complete"],
        WHEN,
    )?;
    e.promote_current(
        &executed,
        &p["promote_authorization"],
        &p["contexts"]["promoted"],
        WHEN,
    )
}
fn nonzero_reference<'a>(
    e: &'a RefineryEngine<'_>,
    p: &Value,
    input: &Value,
    source: &Value,
) -> Result<semantica_runtime::refinery_engagement::CurrentEngagement<'a>> {
    let scenarios = p["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    e.engage_current(&NoExternalInput,&p["reference_execution_manifest"],&scenarios,&input["task"],&input["binding"],&json!({"status":"candidate","rationale":input["delta"]["rationale"],"delta_sha256":input["delta"]["delta_sha256"]}),source,WHEN)
}
#[test]
fn actual_nonzero_replacement_preserves_full_immutable_baseline_and_prior_cq() -> Result<()> {
    let expected = current_expected();
    let p = plan();
    let source = current_source()?;
    let store = new_store()?;
    let e = engine(&store)?;
    let baseline = publish_baseline(&e, &p, &source)?;
    let input: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/full12/INPUT-nonzero-immutable-baseline-valid-ontology-replacement.json",
    ))?;
    assert_eq!(
        baseline["package_sha256"],
        input["immutable_base_descriptor"]["package_sha256"]
    );
    let before = store.saved();
    let base_files = before
        .iter()
        .filter(|(k, _)| k.starts_with("packages/"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    let reference = nonzero_reference(&e, &p, &input, &source)?;
    assert_eq!(reference.observation()["status"], "complete");
    let delta = input["delta"]["delta_sha256"].as_str().unwrap();
    e.propose_current(
        &reference,
        &input["delta"],
        &input["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    e.commit_current(
        &reference,
        delta,
        &input["contexts"]["committed"]["envelope"],
        &input["commit_authorization"],
        WHEN,
    )?;
    let actual = e.execute_current(
        delta,
        &input["contexts"]["execute_candidate"],
        &source,
        WHEN,
    )?;
    assert_eq!(
        actual.observation()["status"],
        expected["valid_nonzero_replacement"]["subject_status"]
    );
    let regression = e.derive_gate(
        delta,
        "regression",
        actual.observation()["suite_sha256"].as_str().unwrap(),
        &input["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    assert_eq!(
        regression["status"],
        expected["valid_nonzero_replacement"]["regression_status"]
    );
    let verified = e.verify_candidate_current(
        &actual,
        Some(&regression),
        Some(&input["contexts"]["regression_passed"]),
        &input["contexts"]["derive_release_gate"],
        &input["contexts"]["release_complete"],
        WHEN,
    )?;
    assert_eq!(
        verified["release_evidence"]["status"],
        expected["valid_nonzero_replacement"]["release_status"]
    );
    let mut auth = p["promote_authorization"].clone();
    auth["delta_sha256"] = json!(delta);
    auth.as_object_mut().unwrap().remove("authorization_sha256");
    let auth = semantica_runtime::refinery_contracts::authorization(&auth)?;
    let descriptor = e.promote_current(&actual, &auth, &input["contexts"]["promoted"], WHEN)?;
    assert_eq!(descriptor["version"], "2.0.0");
    assert_eq!(
        e.status(delta)?["state"],
        expected["valid_nonzero_replacement"]["final_state"]
    );
    let after = store.saved();
    for (k, v) in base_files {
        assert_eq!(after[&k], v, "immutable baseline bytes {k}");
    }
    assert_eq!(
        e.resolve_package(
            p["factory_delta"]["package_id"].as_str().unwrap(),
            Some("1.0.0")
        )?,
        baseline
    );
    Ok(())
}
#[test]
fn actual_whole_cq_replacement_cannot_waive_previous_question() -> Result<()> {
    let expected = current_expected();
    let p = plan();
    let source = current_source()?;
    let store = new_store()?;
    let e = engine(&store)?;
    publish_baseline(&e, &p, &source)?;
    let input: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/full12/INPUT-nonzero-whole-CQ-registry-replacement-does-not-waive-prior-CQ.json",
    ))?;
    let reference = nonzero_reference(&e, &p, &input, &source)?;
    let delta = input["delta"]["delta_sha256"].as_str().unwrap();
    e.propose_current(
        &reference,
        &input["delta"],
        &input["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    e.commit_current(
        &reference,
        delta,
        &input["contexts"]["committed"]["envelope"],
        &input["commit_authorization"],
        WHEN,
    )?;
    let actual = e.execute_current(
        delta,
        &input["contexts"]["execute_candidate"],
        &source,
        WHEN,
    )?;
    assert_eq!(
        actual.observation()["status"],
        expected["whole_CQ_registry_replacement"]["subject_status"]
    );
    let regression = e.derive_gate(
        delta,
        "regression",
        actual.observation()["suite_sha256"].as_str().unwrap(),
        &input["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    assert_eq!(
        regression["status"],
        expected["whole_CQ_registry_replacement"]["regression_status"]
    );
    assert_eq!(
        failures(&regression),
        expected["whole_CQ_registry_replacement"]["failed_regression_ids"]
    );
    assert!(
        e.record_current_gate(
            &actual,
            &regression,
            &input["contexts"]["regression_passed"],
            WHEN
        )
        .is_err()
    );
    assert_eq!(
        e.status(delta)?["state"],
        expected["whole_CQ_registry_replacement"]["state"]
    );
    Ok(())
}
#[test]
fn foreign_private_owner_and_original_extra_registry_pointer_field() -> Result<()> {
    let p = plan();
    let source = current_source()?;
    let store = new_store()?;
    let e = engine(&store)?;
    let reference = full_reference(&e, &p, &p["factory_delta"], &source)?;
    let other = new_store()?;
    let other_engine = engine(&other)?;
    let before = other.saved();
    assert!(
        other_engine
            .propose_current(
                &reference,
                &p["factory_delta"],
                &p["contexts"]["proposed"]["envelope"],
                WHEN
            )
            .is_err()
    );
    assert_eq!(other.saved(), before);
    let historical = Store::stage("promoted")?;
    let before = historical.saved();
    let mut pointer: Value = serde_json::from_slice(&before["registry.json"])?;
    pointer["original_allows_additional_field"] = json!({"observed":true});
    historical.fault("registry.json", semantica_json::canonical_json(&pointer)?);
    let historical_engine = engine(&historical)?;
    assert_eq!(historical_engine.status(DELTA)?["state"], "promoted");
    assert_eq!(
        historical_engine.resolve_package(
            p["factory_delta"]["package_id"].as_str().unwrap(),
            Some("1.0.0")
        )?["version"],
        "1.0.0"
    );
    Ok(())
}

fn committed_current<'a>(
    e: &'a RefineryEngine<'_>,
    p: &Value,
    source: &Value,
) -> Result<semantica_runtime::refinery_subject::CurrentSubjectExecution<'a>> {
    let reference = full_reference(e, p, &p["factory_delta"], source)?;
    e.propose_current(
        &reference,
        &p["factory_delta"],
        &p["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    e.commit_current(
        &reference,
        DELTA,
        &p["contexts"]["committed"]["envelope"],
        &p["commit_authorization"],
        WHEN,
    )?;
    e.execute_current(DELTA, &p["contexts"]["execute_candidate"], source, WHEN)
}
fn rehash_gate(mut value: Value) -> Result<Value> {
    let mut contents = value.clone();
    contents.as_object_mut().unwrap().remove("evidence_sha256");
    contents
        .as_object_mut()
        .unwrap()
        .remove("missing_capabilities");
    value["evidence_sha256"] = json!(semantica_runtime::refinery::hash(&contents)?);
    Ok(value)
}
#[test]
fn actual_current_order_forged_gate_capability_and_governed_retry_countercases() -> Result<()> {
    let p = plan();
    let source = current_source()?;
    let store = new_store()?;
    let e = engine(&store)?;
    let proof = committed_current(&e, &p, &source)?;
    let suite = proof.observation()["suite_sha256"].as_str().unwrap();
    let before = store.saved();
    assert!(
        e.derive_gate(
            DELTA,
            "release",
            suite,
            &p["contexts"]["derive_release_gate"],
            WHEN
        )
        .is_err(),
        "release-before-recorded-regression"
    );
    assert_eq!(e.status(DELTA)?["state"], "committed");
    let regression = e.derive_gate(
        DELTA,
        "regression",
        suite,
        &p["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    let mut forged = regression.clone();
    forged["checks"][0]["output_sha256"] = json!("0".repeat(64));
    let forged = rehash_gate(forged)?;
    refinery_subject::gate(&forged)?;
    assert!(
        e.record_current_gate(&proof, &forged, &p["contexts"]["regression_passed"], WHEN)
            .is_err(),
        "forged-valid-outer-hash-current-proof"
    );
    assert_eq!(e.status(DELTA)?["state"], "committed");
    let mut missing = regression.clone();
    missing["observed_capabilities"] = json!(
        regression["observed_capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| !p["factory_delta"]["required_capabilities"]
                .as_array()
                .unwrap()
                .contains(v))
            .cloned()
            .collect::<Vec<_>>()
    );
    missing["missing_capabilities"] = p["factory_delta"]["required_capabilities"].clone();
    missing["status"] = json!("blocked");
    let missing = rehash_gate(missing)?;
    refinery_subject::gate(&missing)?;
    assert!(
        e.record_current_gate(&proof, &missing, &p["contexts"]["regression_passed"], WHEN)
            .is_err(),
        "missing-actual-required-capability-current-proof"
    );
    assert_eq!(e.status(DELTA)?["state"], "committed");
    e.verify_candidate_current(
        &proof,
        Some(&regression),
        Some(&p["contexts"]["regression_passed"]),
        &p["contexts"]["derive_release_gate"],
        &p["contexts"]["release_complete"],
        WHEN,
    )?;
    let before_retry = store.saved();
    let mut changed = p["contexts"]["derive_release_gate"]["envelope"].clone();
    changed["task_id"] = json!("task-changed-retry");
    changed.as_object_mut().unwrap().remove("envelope_sha256");
    let changed = semantica_runtime::refinery_contracts::task(&changed)?;
    let changed = semantica_runtime::refinery_contracts::transition_context(
        "derive_release_gate",
        DELTA,
        &changed,
        &p["binding"],
    )?;
    assert!(
        e.verify_candidate_current(
            &proof,
            None,
            None,
            &changed,
            &p["contexts"]["release_complete"],
            WHEN
        )
        .is_err(),
        "current release retry changed-context"
    );
    assert_eq!(store.saved(), before_retry);
    let mut auth = p["promote_authorization"].clone();
    auth["actor_id"] = json!("outsider.authority");
    auth["authority"] = json!("outsider.authority");
    auth.as_object_mut().unwrap().remove("authorization_sha256");
    let auth = semantica_runtime::refinery_contracts::authorization(&auth)?;
    assert!(
        e.promote_current(&proof, &auth, &p["contexts"]["promoted"], WHEN)
            .is_err(),
        "promotion outside decision authorities with actual proof"
    );
    assert_eq!(store.saved(), before_retry);
    assert_eq!(e.status(DELTA)?["state"], "release_complete");
    assert!(before.len() < store.saved().len());
    Ok(())
}
#[test]
fn actual_current_full_receipt_and_context_tamper_refuse_after_new_engine() -> Result<()> {
    let p = plan();
    let source = current_source()?;
    for target in [
        "full_receipt",
        "transition_context",
        "same_size_full_result",
    ] {
        let store = new_store()?;
        let e = engine(&store)?;
        let proof = committed_current(&e, &p, &source)?;
        let suite = proof.observation();
        e.verify_suite_content(suite)?;
        let digest = match target {
            "full_receipt" => suite["runs"][0]["receipt_object_sha256"]
                .as_str()
                .unwrap()
                .to_owned(),
            "same_size_full_result" => suite["runs"][0]["run_result_object_sha256"]
                .as_str()
                .unwrap()
                .to_owned(),
            _ => e.history(DELTA)?[1]["payload"]["transition_context_object_sha256"]
                .as_str()
                .unwrap()
                .to_owned(),
        };
        let path = format!("objects/sha256/{}/{digest}", &digest[..2]);
        let saved = store.saved();
        let bytes = &saved[&path];
        let mut value: Value = serde_json::from_slice(bytes)?;
        match target {
            "full_receipt" => {
                value["provenance_bundle"]["records"][1]["previous_checksum"] =
                    json!("0".repeat(64))
            }
            "same_size_full_result" => value["actual_run"]["status"] = json!("failed"),
            _ => value["action"] = json!("committed"),
        };
        let mut changed = semantica_json::canonical_json(&value)?;
        if bytes.last() == Some(&b'\n') {
            changed.push(b'\n');
        }
        if target == "same_size_full_result" {
            assert_eq!(changed.len(), bytes.len());
        }
        store.fault(&path, changed);
        if target == "transition_context" {
            assert!(engine(&store)?.status(DELTA).is_err());
        } else {
            assert!(e.verify_suite_content(suite).is_err());
            assert!(
                e.record_current_gate(
                    &proof,
                    &json!({}),
                    &p["contexts"]["regression_passed"],
                    WHEN
                )
                .is_err()
            );
            assert_eq!(e.status(DELTA)?["state"], "committed");
        }
    }
    Ok(())
}
#[test]
fn actual_source_binding_and_rights_preflight_retain_only_original_single_context_cas() -> Result<()>
{
    let original = original();
    let p = plan();
    let source = current_source()?;
    for name in [
        "source-binding-mismatch-before-native-run-writes",
        "source-rights-asset-wrong-category",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../original/full12")
            .join(format!("INPUT-{name}.json"));
        let input: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let delta = &input["original_delta_variant"];
        let digest = delta["delta_sha256"].as_str().unwrap();
        let store = new_store()?;
        let e = engine(&store)?;
        let reference = full_reference(&e, &p, delta, &source)?;
        assert_eq!(reference.observation()["status"], "complete");
        e.propose_current(
            &reference,
            delta,
            &input["contexts"]["proposed"]["envelope"],
            WHEN,
        )?;
        e.commit_current(
            &reference,
            digest,
            &input["contexts"]["committed"]["envelope"],
            &input["authorization"],
            WHEN,
        )?;
        let before = store.saved();
        assert!(
            e.execute_current(
                digest,
                &input["contexts"]["execute_candidate"],
                &source,
                WHEN
            )
            .is_err(),
            "{name}"
        );
        let after = store.saved();
        let added = after
            .keys()
            .filter(|k| !before.contains_key(*k))
            .cloned()
            .collect::<Vec<_>>();
        let old = original["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["case_id"] == name)
            .unwrap();
        assert_eq!(
            added,
            json!(old["new_CAS_files"])
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "only original actual transition-context CAS, zero native result files"
        );
        assert_eq!(e.status(digest)?["state"], "committed");
        for (k, v) in before {
            assert_eq!(after[&k], v, "prior files preserved {name} {k}");
        }
    }
    Ok(())
}
