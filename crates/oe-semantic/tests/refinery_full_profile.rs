//! Current host composition uses actual observed source and explicit independent
//! reference/target files. It does not accept saved caller-passed receipts.
use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use anyhow::{Context, Result, ensure};
use oe_semantic::{
    native::{
        OpenEngagementRequest, ProposeRequest, RegistryRequest, RegistryRunRequest,
        TransitionRequest, VerifyRequest,
    },
    refinery_runtime::RustRuntime,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};
const WHEN: &str = "2026-08-19T00:00:00Z";
const DELTA: &str = "e2ebd4fd3fa591620cdcfa3942172d9108a3488670eb604bbff0c75aecb6896d";
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
fn complete(result: oe_semantic::native::NativeExecution) -> Value {
    assert_eq!(result.semantic_execution, "completed", "{:?}", result);
    assert!(result.runtime_identity.is_none());
    let value = result.result.unwrap();
    assert_eq!(value["source_recheck"]["status"], "matched");
    value
}
fn target(delta: &Value) -> Result<Value> {
    let changes = semantica_runtime::refinery::CATEGORIES
        .iter()
        .flat_map(|c| delta[*c].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    let contract = changes
        .iter()
        .find(|a| a["category"] == "contract")
        .context("test fixture contract")?;
    let projection: Value = serde_json::from_slice(
        &semantica_runtime::refinery_contracts::asset_content(contract)?.unwrap(),
    )?;
    let mut assets = Vec::new();
    for a in changes {
        ensure!(a["operation"] == "add", "test fixture base zero");
        let id = a["asset_id"].as_str().unwrap();
        let setting = &projection["assets"][id];
        let mut asset = json!({"asset_id":id,"sha256":a["sha256"],"data":String::from_utf8(semantica_runtime::refinery_contracts::asset_content(a)?.unwrap())?,"role":setting["role"],"format":setting["format"],"kind":setting["kind"],"load_into_dataset":setting["load_into_dataset"]});
        if !setting["graph_name"].is_null() {
            asset["graph_name"] = setting["graph_name"].clone();
        }
        assets.push(asset);
    }
    Ok(
        json!({"schema_version":"1.0","package_id":delta["package_id"],"version":delta["target_version"],"namespace":projection["namespace"],"release_status":projection["release_status"],"execution":projection["execution"],"assets":assets}),
    )
}
fn files(runtime: &RustRuntime, p: &Value, delta: &Value, label: &str) -> Result<(PathBuf, Value)> {
    let run = root()
        .join("var/full12-host-profile")
        .join(format!("{}-{label}", std::process::id()));
    fs::create_dir_all(&run)?;
    let reference = run.join("reference.json");
    let targetpath = run.join("target.json");
    let refbytes = serde_json::to_vec(&p["reference_execution_manifest"])?;
    let targetbytes = serde_json::to_vec(&target(delta)?)?;
    fs::write(&reference, &refbytes)?;
    fs::write(&targetpath, &targetbytes)?;
    let input = json!({"schema":"semantica.refinery-full-observation-request/1","reference_manifest_path":reference,"reference_manifest_sha256":semantica_runtime::sha256(&refbytes),"target_manifest_path":targetpath,"target_manifest_sha256":semantica_runtime::sha256(&targetbytes),"reference_scenario_ids":p["scenarios"],"required_regression_check_ids":p["fixed_regression_order"],"required_release_check_ids":p["fixed_release_order"],"runtime_source":runtime.source()?});
    fs::write(
        run.join("actual-input-plan.json"),
        serde_json::to_vec_pretty(&input)?,
    )?;
    Ok((run, input))
}
fn snapshot(path: &std::path::Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn visit(
        b: &std::path::Path,
        p: &std::path::Path,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<()> {
        for en in fs::read_dir(p)? {
            let p = en?.path();
            if p.is_dir() {
                visit(b, &p, out)?
            } else {
                out.insert(p.strip_prefix(b)?.to_str().unwrap().into(), fs::read(p)?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(path, path, &mut out)?;
    Ok(out)
}
#[test]
fn full_host_profile_proposes_commits_verifies_promotes_retries_and_runs_real_registry()
-> Result<()> {
    let p = plan();
    let runtime = RustRuntime::new(&root())?;
    let (run, input) = files(&runtime, &p, &p["factory_delta"], "positive")?;
    let registry = RegistryRequest {
        workspace: run.join("registry"),
        binding: p["binding"].clone(),
    };
    let open = OpenEngagementRequest {
        workspace: registry.workspace.clone(),
        binding: registry.binding.clone(),
        envelope: p["task"].clone(),
    };
    let opened = complete(runtime.open_engagement_with_evidence(&open, &input));
    assert_eq!(opened["engagement"]["status"], "complete");
    assert_eq!(opened["engagement"]["learning"]["status"], "no_delta");
    let mut wrong = input.clone();
    wrong["runtime_source"]["source_sha256"] = json!("f".repeat(64));
    let before = snapshot(&registry.workspace)?;
    let request = ProposeRequest {
        registry: registry.clone(),
        delta: p["factory_delta"].clone(),
        candidate_envelope: p["task"].clone(),
        proposed_envelope: p["contexts"]["proposed"]["envelope"].clone(),
        engagement: wrong,
        recorded_at: Some(WHEN.into()),
    };
    assert_eq!(
        runtime.propose_candidate(&request).semantic_execution,
        "failed"
    );
    assert_eq!(snapshot(&registry.workspace)?, before);
    let request = ProposeRequest {
        engagement: input.clone(),
        ..request
    };
    let proposed = complete(runtime.propose_candidate(&request));
    assert_eq!(proposed["state"]["state"], "proposed");
    let transition = TransitionRequest {
        registry: registry.clone(),
        delta_sha256: DELTA.into(),
        envelope: p["contexts"]["committed"]["envelope"].clone(),
        recorded_at: Some(WHEN.into()),
    };
    let committed = complete(runtime.commit_candidate(&transition, &p["commit_authorization"]));
    assert_eq!(committed["state"]["state"], "committed");
    let envelopes = [
        "execute_candidate",
        "derive_regression_gate",
        "regression_passed",
        "derive_release_gate",
        "release_complete",
    ]
    .into_iter()
    .map(|a| (a.into(), p["contexts"][a]["envelope"].clone()))
    .collect();
    let verify = VerifyRequest {
        registry: registry.clone(),
        delta_sha256: DELTA.into(),
        envelopes,
        recorded_at: Some(WHEN.into()),
    };
    let verified = complete(runtime.verify_candidate(&verify));
    assert_eq!(verified["state"]["state"], "release_complete");
    assert_eq!(
        verified["regression_evidence"]["checks"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        verified["release_evidence"]["checks"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let before = snapshot(&registry.workspace)?;
    let retried = complete(runtime.verify_candidate(&verify));
    assert_eq!(
        retried["native_verification"],
        verified["native_verification"]
    );
    assert!(retried["execution_suite"].is_null());
    assert!(verified["execution_suite"].is_object());
    assert_eq!(snapshot(&registry.workspace)?, before);
    let promote = TransitionRequest {
        envelope: p["contexts"]["promoted"]["envelope"].clone(),
        ..transition
    };
    let promotion = complete(runtime.promote_candidate(&promote, &p["promote_authorization"]));
    let descriptor = promotion["descriptor"].clone();
    assert_eq!(descriptor["version"], "1.0.0");
    let before = snapshot(&registry.workspace)?;
    let retried = complete(runtime.promote_candidate(&promote, &p["promote_authorization"]));
    assert_eq!(retried, promotion);
    assert_eq!(snapshot(&registry.workspace)?, before);
    let mut published = p["binding"].clone();
    published["baseline_version"] = json!("1.0.0");
    published["baseline_package_sha256"] = descriptor["package_sha256"].clone();
    published.as_object_mut().unwrap().remove("binding_sha256");
    let published = semantica_runtime::refinery_contracts::binding(&published)?;
    for scenario in p["scenarios"].as_array().unwrap() {
        let result = complete(runtime.run_registry(&RegistryRunRequest {
            registry: RegistryRequest {
                workspace: registry.workspace.clone(),
                binding: published.clone(),
            },
            scenario_id: Some(scenario.as_str().unwrap().into()),
            created_at: Some(WHEN.into()),
        }));
        assert_eq!(result["actual_run"]["status"], "passed");
        assert_eq!(
            result["release_content"]["content_verdict"]["status"],
            "complete"
        );
        assert_eq!(result["release_content"]["engineering_adoption"], "unknown");
    }
    assert_eq!(snapshot(&registry.workspace)?, before);
    fs::write(
        run.join("ACTUAL-HOST-RESULTS.json"),
        serde_json::to_vec_pretty(
            &json!({"opened":opened,"proposed":proposed,"committed":committed,"verified":verified,"descriptor":descriptor,"original_expected_unchanged":true,"business_python":false}),
        )?,
    )?;
    Ok(())
}
fn project_binding(p: &Value) -> Value {
    json!({"$schema":"ontology-engineering.semantic-project-binding/v1","binding_id":"full12-current-project-binding","project":{"project_id":p["binding"]["project_id"],"domain":p["binding"]["domain"]},"semantic_api":"semantica.ontology.refinery/v1","semantic_target":{"kind":"workspace","package_id":p["binding"]["package_id"],"workspace_id":p["binding"]["workspace_id"]},"baseline":{"version":"0","digest":p["binding"]["baseline_package_sha256"]},"evidence":{"logical_root":p["source_evidence"]["uri"]},"authority":{"fact":{"authority_id":"fact.authority","scope":["record_candidate"]},"decision":{"authority_id":"decision.authority","scope":["commit","promote"]}},"allowed_actions":["doctor","discover","open","propose","commit","verify","promote","history","run"],"lifecycle_actions":p["binding"]["allowed_actions"],"promotion":{"requires_decision_authority":true,"target":"industry-registry"},"created_at":WHEN})
}
fn project_task(p: &Value, action: &str) -> Value {
    json!({"$schema":"ontology-engineering.semantic-task-envelope/v1","task_id":format!("full12-current-{action}"),"task_kind":p["task"]["task_kind"],"project":p["task"]["project_id"],"domain":p["task"]["domain"],"actor_id":if ["commit","promote"].contains(&action){"decision.authority"}else{"fact.authority"},"intent":p["task"]["intent"],"requested_decision":p["task"]["requested_decision"],"requested_actions":[action],"required_capabilities":p["task"]["required_capabilities"],"evidence":p["task"]["evidence"],"created_at":WHEN})
}
fn good_command(value: Value) -> Value {
    assert_eq!(value["command_verdict"], "passed", "{value}");
    assert_eq!(value["diagnostics"], json!([]));
    value
}
#[test]
fn current_lifecycle_facade_full_profile_reaches_promotion_and_registry_execution() -> Result<()> {
    let p = plan();
    let runtime = RustRuntime::new(&root())?;
    let (run, input) = files(&runtime, &p, &p["factory_delta"], "facade")?;
    let workspace = run.join("registry");
    let binding = run.join("binding.json");
    let task = run.join("task.json");
    let inputpath = run.join("full-plan.json");
    let deltapath = run.join("delta.json");
    fs::write(&binding, serde_json::to_vec(&project_binding(&p))?)?;
    fs::write(&inputpath, serde_json::to_vec(&input)?)?;
    fs::write(&task, serde_json::to_vec(&project_task(&p, "open"))?)?;
    let opened = good_command(oe_semantic::lifecycle::open(
        &root(),
        &binding,
        &task,
        &workspace,
        Some(&inputpath),
    )?);
    assert_eq!(opened["learning"]["verdict"], "no_delta");
    assert_eq!(opened["execution"]["ontology_truth_mutated"], false);
    let mut delta = p["factory_delta"].clone();
    delta.as_object_mut().unwrap().remove("delta_sha256");
    fs::write(&deltapath, serde_json::to_vec(&delta)?)?;
    fs::write(&task, serde_json::to_vec(&project_task(&p, "propose"))?)?;
    let proposed = good_command(oe_semantic::lifecycle::propose_at(
        &root(),
        &binding,
        &task,
        &workspace,
        &deltapath,
        &inputpath,
        Some(WHEN),
    )?);
    let candidate = proposed["learning"]["candidate"]["candidate_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let authorization = run.join("authorization.json");
    for action in ["commit", "verify", "promote"] {
        fs::write(&task, serde_json::to_vec(&project_task(&p, action))?)?;
        let authpath = if action == "verify" {
            None
        } else {
            let auth = json!({"$schema":"ontology-engineering.refinery-authorization/v1","authorization_id":format!("full12-control-{action}"),"action":action,"actor_id":"decision.authority","authority_id":"decision.authority","authority_scope":["commit","promote"],"package_id":p["binding"]["package_id"],"delta_sha256":candidate,"promotion_target":"industry-registry","reason":"Finite private independent current acceptance fixture; no physical engineering adoption.","source":p["source_evidence"],"issued_at":WHEN,"decisions":[]});
            fs::write(&authorization, serde_json::to_vec(&auth)?)?;
            Some(authorization.as_path())
        };
        let result = good_command(oe_semantic::lifecycle::transition_at(
            &root(),
            action,
            &binding,
            &task,
            &workspace,
            &candidate,
            authpath,
            Some(WHEN),
        )?);
        assert_eq!(
            result["learning"]["current_state"],
            match action {
                "commit" => "committed",
                "verify" => "release_complete",
                _ => "promoted",
            }
        );
        fs::write(
            run.join(format!("ACTUAL-FACADE-{action}.json")),
            serde_json::to_vec_pretty(&result)?,
        )?;
        if action == "verify" {
            let before = snapshot(&workspace)?;
            let again = good_command(oe_semantic::lifecycle::transition_at(
                &root(),
                action,
                &binding,
                &task,
                &workspace,
                &candidate,
                None,
                Some(WHEN),
            )?);
            assert_eq!(
                again["execution"]["recovered_from_immutable_evidence"],
                true
            );
            assert_eq!(snapshot(&workspace)?, before);
        }
        if action == "promote" {
            assert_eq!(
                result["learning"]["next_binding"]["requires_control_plane_approval"],
                true
            );
            assert_eq!(result["learning"]["next_binding"]["auto_applied"], false);
            fs::write(
                &binding,
                serde_json::to_vec(&result["learning"]["next_binding"]["document"])?,
            )?;
        }
    }
    fs::write(&task, serde_json::to_vec(&project_task(&p, "run"))?)?;
    let before = snapshot(&workspace)?;
    for scenario in p["scenarios"].as_array().unwrap() {
        let result = good_command(oe_semantic::lifecycle::run(
            &root(),
            &oe_semantic::lifecycle::ExecutionRequest {
                binding: &binding,
                task: Some(&task),
                workspace: Some(&workspace),
                scenario_id: scenario.as_str(),
                recorded_at: Some(WHEN),
            },
        )?);
        assert_eq!(result["release"]["status"], "complete");
        fs::write(
            run.join(format!("ACTUAL-FACADE-{}.json", scenario.as_str().unwrap())),
            serde_json::to_vec_pretty(&result)?,
        )?;
    }
    assert_eq!(snapshot(&workspace)?, before);
    Ok(())
}
