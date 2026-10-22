//! Actual fresh OS processes restore authority only by repeating retained native
//! execution. Historical JSON is verified content and never an execution token.
use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use anyhow::{Context, Result, ensure};
use oe_core::SkillRoot;
use oe_semantic::{refinery_runtime::RustRuntime, refinery_storage::FileStore};
use semantica_runtime::{books::SourceProvider, refinery::RefineryEngine};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
const WHEN: &str = "2026-08-19T00:00:00Z";
const DELTA: &str = "e2ebd4fd3fa591620cdcfa3942172d9108a3488670eb604bbff0c75aecb6896d";
const REGISTRY: &str = "registry-original-full12";
static SERIAL: AtomicU64 = AtomicU64::new(0);
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn plan() -> Value {
    serde_json::from_str(&private_test_fixtures::text(
        "original/full12/ORIGINAL-FULL12-FIXED-PLAN.json",
    ))
    .unwrap()
}
struct Embedded;
impl SourceProvider for Embedded {
    fn read(&self, _: &str) -> Result<Vec<u8>> {
        anyhow::bail!("frozen reference assets are fully embedded")
    }
}
fn snapshot(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn visit(base: &Path, p: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        for e in fs::read_dir(p)? {
            let p = e?.path();
            let m = p.symlink_metadata()?;
            ensure!(!m.file_type().is_symlink(), "test fixture symlink");
            if m.is_dir() {
                visit(base, &p, out)?;
            } else {
                ensure!(m.is_file(), "regular test file");
                let r = p
                    .strip_prefix(base)?
                    .to_str()
                    .context("test path utf8")?
                    .to_owned();
                if r != ".refinery.lock" {
                    out.insert(r, fs::read(p)?);
                }
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(path, path, &mut out)?;
    Ok(out)
}
fn duplicate(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir(to)?;
    fn visit(a: &Path, b: &Path) -> Result<()> {
        for e in fs::read_dir(a)? {
            let e = e?;
            let p = e.path();
            let q = b.join(e.file_name());
            let m = p.symlink_metadata()?;
            ensure!(!m.file_type().is_symlink(), "checkpoint no symlink");
            if m.is_dir() {
                fs::create_dir(&q)?;
                visit(&p, &q)?;
            } else if e.file_name() != ".refinery.lock" {
                fs::copy(&p, &q)?;
            }
        }
        Ok(())
    }
    visit(from, to)
}
fn job(
    run: &Path,
    name: &str,
    state: &Path,
    source: &Value,
    suite: Option<&str>,
    expected: &Value,
) -> Result<PathBuf> {
    let path = run.join(format!("{name}.job.json"));
    fs::write(
        &path,
        serde_json::to_vec(
            &json!({"operation":name,"workspace":state,"source":source,"suite_sha256":suite,"expected":expected,"parent_pid":std::process::id()}),
        )?,
    )?;
    Ok(path)
}
fn child(path: &Path) -> Result<Value> {
    let output = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "fresh_process_worker",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("OE_FULL12_PRIVATE_JOB", path)
        .output()?;
    fs::write(path.with_extension("stdout"), &output.stdout)?;
    fs::write(path.with_extension("stderr"), &output.stderr)?;
    ensure!(
        output.status.success(),
        "fresh child failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(serde_json::from_slice(&fs::read(
        path.with_extension("result.json"),
    )?)?)
}
#[test]
fn fresh_process_worker() -> Result<()> {
    let Some(job) = std::env::var_os("OE_FULL12_PRIVATE_JOB") else {
        return Ok(());
    };
    let job = PathBuf::from(job);
    let input: Value = serde_json::from_slice(&fs::read(&job)?)?;
    ensure!(
        input["parent_pid"].as_u64() != Some(std::process::id() as u64),
        "must be different OS process"
    );
    let p = plan();
    let skill = SkillRoot::new(root())?;
    let path = PathBuf::from(input["workspace"].as_str().context("workspace")?);
    let source = RustRuntime::new(&root())?.source()?;
    assert_eq!(
        source, input["source"],
        "same exact executable/source bytes in distinct process"
    );
    let store = FileStore::open(&skill, &path, REGISTRY, WHEN, false)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    let before = snapshot(&path)?;
    let operation = input["operation"].as_str().unwrap();
    let value = match operation {
        "retry_partial_proposal" => {
            let scenarios = p["scenarios"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let proof = e.engage_current(&Embedded, &p["reference_execution_manifest"], &scenarios, &p["task"], &p["binding"], &json!({"status":"candidate","rationale":p["factory_delta"]["rationale"],"delta_sha256":DELTA}), &source, WHEN)?;
            e.propose_current(
                &proof,
                &p["factory_delta"],
                &p["contexts"]["proposed"]["envelope"],
                WHEN,
            )?;
            assert_eq!(
                e.history(DELTA)?.len(),
                2,
                "retry retains the original proposed event"
            );
            let target = e.preview_delta_manifest(&p["factory_delta"], &p["binding"])?;
            e.retain_current_observation_plan(
                &proof,
                DELTA,
                &private_input_plan(&source, &p),
                &target,
            )?;
            e.commit_current(
                &proof,
                DELTA,
                &p["contexts"]["committed"]["envelope"],
                &p["commit_authorization"],
                WHEN,
            )?
        }
        "proposed_to_commit" => {
            let proof = e.replay_current_engagement(DELTA, &source)?;
            e.commit_current(
                &proof,
                DELTA,
                &p["contexts"]["committed"]["envelope"],
                &p["commit_authorization"],
                WHEN,
            )?
        }
        "regression_to_release" | "release_exact_retry" => {
            let proof = e.replay_current_execution(
                DELTA,
                input["suite_sha256"].as_str().unwrap(),
                &source,
            )?;
            e.verify_candidate_current(
                &proof,
                None,
                None,
                &p["contexts"]["derive_release_gate"],
                &p["contexts"]["release_complete"],
                WHEN,
            )?
        }
        "promoted_registry_four" => {
            let mut runs = Vec::new();
            for s in p["scenarios"].as_array().unwrap() {
                runs.push(e.run_registry_current(
                    p["factory_delta"]["package_id"].as_str().unwrap(),
                    Some("1.0.0"),
                    s.as_str().unwrap(),
                    &source,
                    WHEN,
                )?);
            }
            for r in &runs {
                assert_eq!(r["actual_run"]["status"], "passed");
                assert_eq!(
                    r["release_content"]["content_verdict"]["status"],
                    "complete"
                );
                assert_eq!(r["release_content"]["engineering_adoption"], "unknown");
            }
            json!(runs)
        }
        _ => anyhow::bail!("unknown private test operation"),
    };
    let status = e.status(DELTA)?;
    let after = snapshot(&path)?;
    if matches!(operation, "release_exact_retry" | "promoted_registry_four") {
        assert_eq!(
            after, before,
            "exact retry and registry rerun leave all engine bytes unchanged"
        );
    }
    if !input["expected"].is_null() {
        assert_eq!(
            value, input["expected"],
            "retained original algorithm result matches prepared current run"
        );
    }
    let result = json!({"operation":operation,"child_pid":std::process::id(),"parent_pid":input["parent_pid"],"value":value,"status":status,"history_count":e.history(DELTA)?.len(),"files_before":before.len(),"files_after":after.len(),"source":source,"private_authority_restored_by":"actual native execution","python_production":false});
    fs::write(
        job.with_extension("result.json"),
        serde_json::to_vec(&result)?,
    )?;
    Ok(())
}
#[test]
fn four_fresh_os_processes_reexecute_reference_subject_and_read_published_registry() -> Result<()> {
    let p = plan();
    let source = RustRuntime::new(&root())?.source()?;
    let skill = SkillRoot::new(root())?;
    let run = root().join("var/full12-fresh-process").join(format!(
        "{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&run)?;
    let state = run.join("preparation");
    let store = FileStore::open(&skill, &state, REGISTRY, WHEN, true)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    let scenarios = p["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let reference=e.engage_current(&Embedded,&p["reference_execution_manifest"],&scenarios,&p["task"],&p["binding"],&json!({"status":"candidate","rationale":p["factory_delta"]["rationale"],"delta_sha256":DELTA}),&source,WHEN)?;
    assert_eq!(reference.observation()["status"], "complete");
    e.propose_current(
        &reference,
        &p["factory_delta"],
        &p["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    let proposed = run.join("proposed");
    duplicate(&state, &proposed)?;
    let committed = e.commit_current(
        &reference,
        DELTA,
        &p["contexts"]["committed"]["envelope"],
        &p["commit_authorization"],
        WHEN,
    )?;
    let execute = e.execute_current(DELTA, &p["contexts"]["execute_candidate"], &source, WHEN)?;
    assert_eq!(execute.observation()["status"], "complete");
    let suite = execute.observation()["suite_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let regression = e.derive_gate(
        DELTA,
        "regression",
        &suite,
        &p["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    e.record_current_gate(
        &execute,
        &regression,
        &p["contexts"]["regression_passed"],
        WHEN,
    )?;
    let reg_state = run.join("regression");
    duplicate(&state, &reg_state)?;
    let release = e.verify_candidate_current(
        &execute,
        None,
        None,
        &p["contexts"]["derive_release_gate"],
        &p["contexts"]["release_complete"],
        WHEN,
    )?;
    let release_state = run.join("release");
    duplicate(&state, &release_state)?;
    e.promote_current(
        &execute,
        &p["promote_authorization"],
        &p["contexts"]["promoted"],
        WHEN,
    )?;
    let promoted = run.join("promoted");
    duplicate(&state, &promoted)?;
    drop(execute);
    drop(reference);
    drop(e);
    drop(store);
    let jobs = [
        job(
            &run,
            "proposed_to_commit",
            &proposed,
            &source,
            None,
            &committed,
        )?,
        job(
            &run,
            "regression_to_release",
            &reg_state,
            &source,
            Some(&suite),
            &release,
        )?,
        job(
            &run,
            "release_exact_retry",
            &release_state,
            &source,
            Some(&suite),
            &release,
        )?,
        job(
            &run,
            "promoted_registry_four",
            &promoted,
            &source,
            None,
            &Value::Null,
        )?,
    ];
    let mut results = Vec::new();
    for j in jobs {
        results.push(child(&j)?);
    }
    assert_eq!(
        results
            .iter()
            .map(|r| r["status"]["state"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "committed",
            "release_complete",
            "release_complete",
            "promoted"
        ]
    );
    assert_eq!(
        results
            .iter()
            .map(|r| r["history_count"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![3, 5, 5, 6]
    );
    fs::write(
        run.join("ACTUAL-FOUR-PROCESS-RESULTS.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    Ok(())
}

#[cfg(unix)]
struct ActualWriteDenied<'a> {
    store: &'a FileStore,
    suffix: &'static str,
    fired: std::cell::Cell<bool>,
}
#[cfg(unix)]
impl semantica_runtime::refinery::RefineryStore for ActualWriteDenied<'_> {
    fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
        self.store.read(p)
    }
    fn write_new(&self, p: &str, b: &[u8]) -> Result<()> {
        if p.ends_with(self.suffix) && !self.fired.replace(true) {
            use std::os::unix::fs::PermissionsExt;
            let parent = self.store.root().join(p).parent().unwrap().to_owned();
            let before = fs::metadata(&parent)?.permissions();
            fs::set_permissions(&parent, fs::Permissions::from_mode(0o500))?;
            let result = self.store.write_new(p, b);
            fs::set_permissions(&parent, before)?;
            ensure!(
                result.is_err(),
                "real operating system must deny creation in unwritable retained directory"
            );
            return result;
        }
        self.store.write_new(p, b)
    }
    fn replace(&self, p: &str, e: &[u8], b: &[u8]) -> Result<()> {
        self.store.replace(p, e, b)
    }
    fn remove(&self, p: &str, e: &[u8]) -> Result<()> {
        self.store.remove(p, e)
    }
    fn list(&self, p: &str) -> Result<Vec<String>> {
        self.store.list(p)
    }
    fn create_dir(&self, p: &str) -> Result<()> {
        self.store.create_dir(p)
    }
    fn ensure_dir(&self, p: &str) -> Result<()> {
        self.store.ensure_dir(p)
    }
    fn publish_dir(&self, a: &str, b: &str) -> Result<()> {
        self.store.publish_dir(a, b)
    }
    fn discard_stage(&self, p: &str) -> Result<()> {
        self.store.discard_stage(p)
    }
}
fn private_input_plan(source: &Value, p: &Value) -> Value {
    json!({"schema":"semantica.refinery-full-observation-request/1","reference_manifest_path":"var/finite-fixed-reference.json","reference_manifest_sha256":semantica_runtime::refinery::hash(&p["reference_execution_manifest"]).unwrap(),"target_manifest_path":"var/finite-fixed-target.json","target_manifest_sha256":semantica_runtime::refinery::hash(&p["reference_execution_manifest"]).unwrap(),"reference_scenario_ids":p["scenarios"],"required_regression_check_ids":p["fixed_regression_order"],"required_release_check_ids":p["fixed_release_order"],"runtime_source":source})
}
#[cfg(unix)]
#[test]
fn actual_retained_reference_and_input_plan_write_denial_recover_in_new_os_process() -> Result<()> {
    let p = plan();
    let source = RustRuntime::new(&root())?.source()?;
    let skill = SkillRoot::new(root())?;
    let run = root().join("var/full12-propose-write-fault").join(format!(
        "{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&run)?;
    for suffix in ["reference-execution.json", "input-plan.json"] {
        let path = run.join(suffix.trim_end_matches(".json"));
        let store = FileStore::open(&skill, &path, REGISTRY, WHEN, true)?;
        let faulty = ActualWriteDenied {
            store: &store,
            suffix,
            fired: std::cell::Cell::new(false),
        };
        let e = RefineryEngine::new(&faulty, REGISTRY)?;
        let scenarios = p["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let reference=e.engage_current(&Embedded,&p["reference_execution_manifest"],&scenarios,&p["task"],&p["binding"],&json!({"status":"candidate","rationale":p["factory_delta"]["rationale"],"delta_sha256":DELTA}),&source,WHEN)?;
        let attempted = if suffix == "reference-execution.json" {
            e.propose_current(
                &reference,
                &p["factory_delta"],
                &p["contexts"]["proposed"]["envelope"],
                WHEN,
            )
            .map(|_| ())
        } else {
            e.propose_current(
                &reference,
                &p["factory_delta"],
                &p["contexts"]["proposed"]["envelope"],
                WHEN,
            )?;
            let target = e.preview_delta_manifest(&p["factory_delta"], &p["binding"])?;
            e.retain_current_observation_plan(
                &reference,
                DELTA,
                &private_input_plan(&source, &p),
                &target,
            )
        };
        assert!(attempted.is_err(), "real denied {suffix} must fail");
        assert!(faulty.fired.get());
        assert_eq!(e.status(DELTA)?["state"], "proposed");
        assert_eq!(e.history(DELTA)?.len(), 2);
        assert!(!path.join(format!("refinements/{DELTA}/{suffix}")).exists());
        fs::write(
            run.join(format!("{suffix}.actual-failure.json")),
            serde_json::to_vec_pretty(
                &json!({"suffix":suffix,"error":format!("{:#}",attempted.unwrap_err()),"state":e.status(DELTA)?,"history_count":e.history(DELTA)?.len(),"real_os_permissions":true,"input_plan_does_not_grant_authority":true}),
            )?,
        )?;
        drop(reference);
        drop(e);
        drop(faulty);
        drop(store);
        let j = job(
            &run,
            &format!("retry-{}", suffix.trim_end_matches(".json")),
            &path,
            &source,
            None,
            &Value::Null,
        )?;
        let mut request: Value = serde_json::from_slice(&fs::read(&j)?)?;
        request["operation"] = json!("retry_partial_proposal");
        fs::write(&j, serde_json::to_vec(&request)?)?;
        let recovered = child(&j)?;
        assert_eq!(recovered["status"]["state"], "committed");
        assert_eq!(recovered["history_count"], 3);
        assert!(
            path.join(format!("refinements/{DELTA}/reference-execution.json"))
                .is_file()
        );
        assert!(
            path.join(format!("refinements/{DELTA}/input-plan.json"))
                .is_file()
        );
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn actual_filesystem_same_size_and_mtime_after_verification_cannot_reuse_current_execution()
-> Result<()> {
    use semantica_runtime::refinery::RefineryStore;
    let p = plan();
    let source = RustRuntime::new(&root())?.source()?;
    let skill = SkillRoot::new(root())?;
    let path = root().join("var/full12-same-stat-fault").join(format!(
        "{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(path.parent().unwrap())?;
    let store = FileStore::open(&skill, &path, REGISTRY, WHEN, true)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    let scenarios = p["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let reference=e.engage_current(&Embedded,&p["reference_execution_manifest"],&scenarios,&p["task"],&p["binding"],&json!({"status":"candidate","rationale":p["factory_delta"]["rationale"],"delta_sha256":DELTA}),&source,WHEN)?;
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
    let proof = e.execute_current(DELTA, &p["contexts"]["execute_candidate"], &source, WHEN)?;
    let suite = proof.observation();
    e.verify_suite_content(suite)?;
    let regression = e.derive_gate(
        DELTA,
        "regression",
        suite["suite_sha256"].as_str().unwrap(),
        &p["contexts"]["derive_regression_gate"],
        WHEN,
    )?;
    let digest = suite["runs"][0]["run_result_object_sha256"]
        .as_str()
        .unwrap();
    let relative = format!("objects/sha256/{}/{digest}", &digest[..2]);
    let full = path.join(&relative);
    let before = fs::metadata(&full)?;
    let original = store.read(&relative)?.unwrap();
    let mut value: Value = serde_json::from_slice(&original)?;
    value["actual_run"]["status"] = json!("failed");
    let mut changed = oe_core::json::canonical_json(&value)?;
    if original.last() == Some(&b'\n') {
        changed.push(b'\n');
    }
    assert_eq!(
        changed.len(),
        original.len(),
        "failed/passed changes preserve exact file length"
    );
    let all_before = snapshot(&path)?;
    fs::write(&full, &changed)?;
    fs::File::options().write(true).open(&full)?.set_times(
        std::fs::FileTimes::new()
            .set_accessed(before.accessed()?)
            .set_modified(before.modified()?),
    )?;
    let after = fs::metadata(&full)?;
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified()?, before.modified()?);
    let integrity = e.verify_suite_content(suite);
    assert!(
        integrity.is_err(),
        "full bytes are checked after prior verification, even with identical size and mtime"
    );
    let gate = e.record_current_gate(
        &proof,
        &regression,
        &p["contexts"]["regression_passed"],
        WHEN,
    );
    assert!(
        gate.is_err(),
        "a genuine private proof and correct derived gate cannot authorize damaged CAS"
    );
    assert_eq!(e.status(DELTA)?["state"], "committed");
    assert_eq!(e.history(DELTA)?.len(), 3);
    let all_after = snapshot(&path)?;
    let mut expected = all_before;
    expected.insert(relative.clone(), changed);
    assert_eq!(
        all_after, expected,
        "rejection makes no other engine file changes"
    );
    fs::write(
        path.parent().unwrap().join(format!(
            "{}.actual-failure.json",
            path.file_name().unwrap().to_string_lossy()
        )),
        serde_json::to_vec_pretty(
            &json!({"full_result_cas":relative,"size_before":before.len(),"size_after":after.len(),"mtime_exact":before.modified()?==after.modified()?,"prior_full_verification":true,"stat_cache_claim":false,"private_proof":true,"correct_derived_gate":true,"integrity_error":format!("{:#}",integrity.unwrap_err()),"gate_error":format!("{:#}",gate.unwrap_err()),"state":"committed","history_count":3,"other_engine_files_exact":true}),
        )?,
    )?;
    Ok(())
}
