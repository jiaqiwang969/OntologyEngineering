//! Existing public lifecycle task/evidence entry on a real promoted registry.
use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use anyhow::{Result, ensure};
use base64::Engine;
use oe_core::SkillRoot;
use oe_semantic::{
    contracts,
    lifecycle::{self, ProjectReviewRequest},
    refinery_runtime::RustRuntime,
    refinery_storage::FileStore,
};
use semantica_runtime::{
    books::SourceProvider, refinery::RefineryEngine, refinery_contracts as wire,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
const WHEN: &str = "2026-08-19T00:00:00Z";
const REGISTRY: &str = "registry-original-full12";

const POSITIVE: &str = "<urn:review:focus> a <urn:review:Record>; <urn:review:name> \"Ada\" .";
const SHAPES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:review:Shape> a sh:NodeShape; sh:targetClass <urn:review:Record>; sh:property [ sh:path <urn:review:name>; sh:minCount 1 ] .";

struct NoExternalInput;
impl SourceProvider for NoExternalInput {
    fn read(&self, _: &str) -> Result<Vec<u8>> {
        anyhow::bail!("unexpected external fixture read")
    }
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn add_asset(
    delta: &mut Value,
    projection: &mut Value,
    category: &str,
    id: &str,
    content: &str,
    role: &str,
    format: &str,
    kind: &str,
) {
    delta[category].as_array_mut().unwrap().push(json!({"category":category,"asset_id":id,
        "operation":"add","media_type":if category=="queries" {"application/sparql-query"} else {"text/turtle"},
        "sha256":semantica_runtime::sha256(content),
        "content_base64":base64::engine::general_purpose::STANDARD.encode(content),
        "replaces_sha256":null,"role":null,"case_kind":null}));
    projection["assets"][id] =
        json!({"role":role,"format":format,"kind":kind,"load_into_dataset":false});
}

fn plan() -> Result<Value> {
    let plan_fixture: &str =
        &private_test_fixtures::text("original/full12/ORIGINAL-FULL12-FIXED-PLAN.json");
    ensure!(
        semantica_runtime::sha256(plan_fixture)
            == "e364a710b4921a17c27360e325e21c9a26a931c45c6c49ccaf3cfd2d0e624f41",
        "original plan changed"
    );
    let mut p: Value = serde_json::from_str(plan_fixture)?;
    let delta = &mut p["factory_delta"];
    let mut projection: Value =
        serde_json::from_slice(&wire::asset_content(&delta["contract"][0])?.unwrap())?;
    for (id, query) in [
        (
            "review-query",
            "SELECT ?finding WHERE { ?s <urn:review:bad> ?finding }",
        ),
        (
            "ordinary-query",
            "SELECT ?name WHERE { ?s <urn:review:name> ?name }",
        ),
        ("ask-query", "ASK { ?s <urn:review:name> ?name }"),
        (
            "unbound-query",
            "SELECT ?finding WHERE { VALUES ?other { 1 } }",
        ),
        (
            "empty-finding-query",
            "SELECT ?finding WHERE { VALUES ?finding { \" \" } }",
        ),
    ] {
        add_asset(
            delta,
            &mut projection,
            "queries",
            id,
            query,
            "sparql",
            "sparql",
            "text",
        );
    }
    add_asset(
        delta,
        &mut projection,
        "shapes",
        "review-shape",
        SHAPES,
        "shapes",
        "turtle",
        "rdf",
    );
    add_asset(
        delta,
        &mut projection,
        "shapes",
        "deactivated-shape",
        "@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:review:Disabled> a sh:NodeShape; sh:targetClass <urn:review:Record>; sh:deactivated true; sh:property [ sh:path <urn:review:missing>; sh:minCount 1 ] .",
        "shapes",
        "turtle",
        "rdf",
    );
    let bytes = serde_json::to_vec(&projection)?;
    delta["contract"][0]["sha256"] = json!(semantica_runtime::sha256(&bytes));
    delta["contract"][0]["content_base64"] =
        json!(base64::engine::general_purpose::STANDARD.encode(&bytes));
    delta.as_object_mut().unwrap().remove("delta_sha256");
    p["factory_delta"] = wire::delta(delta)?;
    let delta_sha = p["factory_delta"]["delta_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    for (action, context) in p["contexts"].as_object_mut().unwrap() {
        *context = wire::transition_context(
            action,
            &delta_sha,
            &context["envelope"],
            &context["binding"],
        )?;
    }
    for key in ["commit_authorization", "promote_authorization"] {
        p[key]["delta_sha256"] = p["factory_delta"]["delta_sha256"].clone();
        p[key]
            .as_object_mut()
            .unwrap()
            .remove("authorization_sha256");
        p[key] = wire::authorization(&p[key])?;
    }
    Ok(p)
}

fn promote(store: &FileStore, source: &Value) -> Result<Value> {
    let p = plan()?;
    let e = RefineryEngine::new(store, REGISTRY)?;
    let delta = &p["factory_delta"];
    let digest = delta["delta_sha256"].as_str().unwrap();
    let scenarios = p["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let reference = e.engage_current(
        &NoExternalInput,
        &p["reference_execution_manifest"],
        &scenarios,
        &p["task"],
        &p["binding"],
        &json!({"status":"candidate","rationale":delta["rationale"],"delta_sha256":digest}),
        source,
        WHEN,
    )?;
    e.propose_current(
        &reference,
        delta,
        &p["contexts"]["proposed"]["envelope"],
        WHEN,
    )?;
    e.commit_current(
        &reference,
        digest,
        &p["contexts"]["committed"]["envelope"],
        &p["commit_authorization"],
        WHEN,
    )?;
    let executed = e.execute_current(digest, &p["contexts"]["execute_candidate"], source, WHEN)?;
    let regression = e.derive_gate(
        digest,
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

#[test]
fn existing_lifecycle_review_uses_current_promoted_task_evidence() -> Result<()> {
    let root = root();
    fs::create_dir_all(root.join("var"))?;
    let run = tempfile::Builder::new()
        .prefix("promoted-task-review-")
        .tempdir_in(root.join("var"))?;
    let workspace = run.path().join("registry");
    let runtime = RustRuntime::new(&root)?;
    let store = FileStore::open(&SkillRoot::new(&root)?, &workspace, REGISTRY, WHEN, true)?;
    let descriptor = promote(&store, &runtime.source()?)?;
    drop(store); // lifecycle must acquire the original production lock itself.
    let binding_path = run.path().join("binding.json");
    let task_path = run.path().join("task.json");
    let evidence_path = run.path().join("evidence.ttl");
    let binding = json!({"$schema":contracts::BINDING_SCHEMA,"binding_id":"binding:review",
        "project":{"project_id":"project-original-full12","domain":"example-industry"},
        "semantic_target":{"kind":"workspace","package_id":descriptor["package_id"],"workspace_id":REGISTRY},
        "baseline":{"version":descriptor["version"],"digest":descriptor["package_sha256"]},
        "evidence":{"logical_root":"urn:review:evidence"},
        "authority":{"fact":{"authority_id":"fact.authority","scope":["review"]},
            "decision":{"authority_id":"decision.authority","scope":["review"]}},
        "allowed_actions":["propose","review"],"lifecycle_actions":["candidate"],
        "promotion":{"target":"industry-registry","requires_decision_authority":true},
        "created_at":WHEN,"semantic_api":contracts::NATIVE_REFINERY_CONTRACT});
    let task = json!({"$schema":contracts::TASK_SCHEMA,"task_id":"task:actual-review",
        "task_kind":"source-scoped-review","intent":"Review exactly this evidence",
        "project":"project-original-full12","domain":"example-industry",
        "requested_decision":"Determine scoped semantic consistency",
        "evidence":[{"source_id":"source:review","uri":"urn:review:evidence/record",
            "sha256":semantica_runtime::sha256(POSITIVE),"media_type":"text/turtle","captured_at":WHEN}],
        "requested_actions":["review"],"actor_id":"fact.authority",
        "required_capabilities":["semantica.ontology.decision-review/v1"],"created_at":WHEN});
    fs::write(&binding_path, serde_json::to_vec(&binding)?)?;
    fs::write(&task_path, serde_json::to_vec(&task)?)?;
    fs::write(&evidence_path, POSITIVE)?;
    let input = ProjectReviewRequest {
        binding: &binding_path,
        workspace: &workspace,
        task: &task_path,
        evidence_file: &evidence_path,
        source_id: "source:review",
        evidence_format: "turtle",
        scope_id: "task:actual-review",
        focus_iri: "urn:review:focus",
        focus_type_iri: "urn:review:Record",
        query_asset_id: "review-query",
        shape_asset_id: "review-shape",
    };
    // The original fixture advertised a full lifecycle without its command
    // authority. Keep that exact mismatch as a refusal at the public entry.
    let mut mismatched = binding.clone();
    mismatched["allowed_actions"] = json!(["review"]);
    mismatched["lifecycle_actions"] = json!(contracts::STATE_MODEL);
    fs::write(&binding_path, serde_json::to_vec(&mismatched)?)?;
    let refused = lifecycle::project_review_at(&root, &input, Some(WHEN)).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("lifecycle action lacks command authority")
    );
    fs::write(&binding_path, serde_json::to_vec(&binding)?)?;
    let clear = lifecycle::project_review_at(&root, &input, Some(WHEN))?;
    assert_eq!(clear["command_verdict"], "passed");
    assert_eq!(clear["capabilities"]["decision_review"]["available"], true);
    assert_eq!(clear["execution"]["review"]["status"], "clear");
    assert_eq!(
        clear["execution"]["task_sha256"],
        semantica_runtime::sha256(fs::read(&task_path)?)
    );
    assert_eq!(clear["execution"]["typed_project_checks"], "not_run");
    assert_eq!(clear["execution"]["engineering_adoption"], "unknown");
    assert_eq!(clear["native_execution"]["result"]["fact_admitted"], false);
    assert_eq!(
        clear["native_execution"]["result"]["source_recheck"]["status"],
        "matched"
    );
    for changed in [
        {
            let mut t = task.clone();
            t["required_capabilities"] = json!(["semantic.package.load"]);
            t
        },
        {
            let mut t = task.clone();
            t["evidence"][0]["sha256"] = json!("f".repeat(64));
            t
        },
        {
            let mut t = task.clone();
            t["evidence"][0]["media_type"] = json!("application/json");
            t
        },
        {
            let mut t = task.clone();
            let item = t["evidence"][0].clone();
            t["evidence"].as_array_mut().unwrap().push(item);
            t
        },
    ] {
        fs::write(&task_path, serde_json::to_vec(&changed)?)?;
        assert!(lifecycle::project_review_at(&root, &input, Some(WHEN)).is_err());
    }
    fs::write(&task_path, serde_json::to_vec(&task)?)?;
    let mut stale = binding.clone();
    stale["baseline"]["version"] = json!("old");
    fs::write(&binding_path, serde_json::to_vec(&stale)?)?;
    assert!(lifecycle::project_review_at(&root, &input, Some(WHEN)).is_err());
    fs::write(&binding_path, serde_json::to_vec(&binding)?)?;
    // Retain actual query and source identity even when the same-session SHACL
    // cannot accept a named graph. No caller-authored passed receipt is used.
    let named = format!("{POSITIVE} <urn:graph> {{ <urn:x> <urn:p> <urn:y> . }}");
    let mut task = task;
    task["evidence"][0]["media_type"] = json!("application/trig");
    task["evidence"][0]["sha256"] = json!(semantica_runtime::sha256(&named));
    fs::write(&task_path, serde_json::to_vec(&task)?)?;
    fs::write(&evidence_path, named)?;
    let partial = ProjectReviewRequest {
        evidence_format: "trig",
        ..input
    };
    let response = lifecycle::project_review_at(&root, &partial, Some(WHEN))?;
    assert_eq!(response["command_verdict"], "blocked");
    assert_eq!(
        response["execution"]["review_observation"]["query_observation"]["execution"],
        "completed"
    );
    assert_eq!(
        response["execution"]["review_observation"]["shape_observation"]["execution"],
        "failed"
    );
    assert_eq!(
        response["execution"]["review_observation"]["execution"],
        "partial"
    );
    Ok(())
}
