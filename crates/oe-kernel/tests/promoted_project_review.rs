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

struct Fixture {
    root: SkillRoot,
    run: tempfile::TempDir,
    project: oe_kernel::project::Project,
    binding: Value,
    task: Value,
    evidence: String,
    input: oe_contracts::TypedPromotedProjectReviewRequest1,
    descriptor: Value,
}
impl Fixture {
    fn new(focus: &str) -> Result<Self> {
        use std::collections::BTreeMap;
        let root = SkillRoot::new(root())?;
        fs::create_dir_all(root.path().join("var"))?;
        let run = tempfile::Builder::new()
            .prefix("typed-promoted-review-")
            .tempdir_in(root.path().join("var"))?;
        let workspace = run.path().join("registry");
        let runtime = RustRuntime::new(root.path())?;
        let store = FileStore::open(&root, &workspace, REGISTRY, WHEN, true)?;
        let descriptor = promote(&store, &runtime.source()?)?;
        drop(store);
        let object = oe_contracts::ObjectIdentity {
            object_id: focus.into(),
            revision: "r1".into(),
            condition_scope: "scope:fluid".into(),
            source_sha256: oe_core::sha256(b"independent declared fluid source"),
        };
        let basis = json!({"schema":"oev2.engineering-basis/1","object_id":focus,"object_revision":"r1",
            "purpose":"declared fluid applicability","acceptance":["declared fluid in source range"],
            "facts":[{"property":"fluid","value":"glycol","status":"declared","source_refs":["source:fluid"]}],
            "assertions":[{"assertion_id":"fluid-scope","kind":"source_applicability","subject":"source:fluid",
                "requirements":{"limits":{"fluid":["glycol"]}},"source_refs":["source:fluid"],"affected_decisions":[]}]});
        let condition = serde_json::to_string(&basis)?;
        let evidence = format!(
            "<{focus}> a <urn:review:Record>; <urn:review:name> \"Ada\"; \
            <urn:semantica:engineering-evidence:subjectRevision> \"r1\"; \
            <urn:semantica:engineering-evidence:scope> \"scope:fluid\"; \
            <urn:semantica:engineering-evidence:fieldEvidence> [<urn:semantica:engineering-evidence:sourceSha256> \"{}\"]; \
            <urn:oev2:project-context:conditionSha256> \"{}\" .",
            object.source_sha256,
            oe_core::sha256(condition.as_bytes())
        );
        let project = oe_kernel::project::Project {
            schema: "oev2.project/1".into(),
            project_id: "project-original-full12".into(),
            revision: "1".into(),
            purpose: "declared fluid applicability".into(),
            objects: BTreeMap::from([(focus.into(), "r1".into())]),
            conditions: BTreeMap::from([("scope:fluid".into(), condition)]),
            evidence: BTreeMap::from([
                ("source:fluid".into(), object.source_sha256.clone()),
                ("source:review".into(), oe_core::sha256(evidence.as_bytes())),
            ]),
            decisions: vec![],
            dependencies: BTreeMap::new(),
        };
        let binding = json!({"$schema":contracts::BINDING_SCHEMA,"binding_id":"binding:typed-review",
            "project":{"project_id":project.project_id,"domain":"example-industry"},
            "semantic_target":{"kind":"workspace","package_id":descriptor["package_id"],"workspace_id":REGISTRY},
            "baseline":{"version":descriptor["version"],"digest":descriptor["package_sha256"]},
            "evidence":{"logical_root":"urn:review:evidence"},
            "authority":{"fact":{"authority_id":"fact.authority","scope":["review"]},
                "decision":{"authority_id":"decision.authority","scope":["review"]}},
            "allowed_actions":["propose","review"],"lifecycle_actions":["candidate"],
            "promotion":{"target":"industry-registry","requires_decision_authority":true},
            "created_at":WHEN,"semantic_api":contracts::NATIVE_REFINERY_CONTRACT});
        let task = json!({"$schema":contracts::TASK_SCHEMA,"task_id":"task:typed-review",
            "task_kind":"source-scoped-review","intent":"Review current declared fluid Project",
            "project":project.project_id,"domain":"example-industry","requested_decision":"Scoped consistency",
            "evidence":[{"source_id":"source:review","uri":"urn:review:evidence/record","sha256":oe_core::sha256(evidence.as_bytes()),"media_type":"text/turtle","captured_at":WHEN}],
            "requested_actions":["review"],"actor_id":"fact.authority",
            "required_capabilities":["semantica.ontology.decision-review/v1"],"created_at":WHEN});
        let placeholder = oe_contracts::ArtifactReference {
            path: "var/placeholder".into(),
            sha256: "0".repeat(64),
        };
        let input = oe_contracts::TypedPromotedProjectReviewRequest1 {
            schema: "oev2.typed-promoted-project-review-request/1".into(),
            project: placeholder.clone(),
            semantic_binding: placeholder.clone(),
            task: placeholder.clone(),
            evidence: placeholder,
            workspace: workspace.to_string_lossy().into_owned(),
            expected_context: oe_kernel::project::context(&project, &object)?,
            object,
            source_id: "source:review".into(),
            focus_type_iri: "urn:review:Record".into(),
            query_asset_id: "review-query".into(),
            shape_asset_id: "review-shape".into(),
            required_checks: [
                "review-query",
                "review-shape",
                "promoted_input",
                "promoted_review",
                "promoted_focus",
                "bound_focus_context",
                "current_project_context",
                "current_condition_model",
                "engineering_purpose",
                "engineering:fluid-scope",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        };
        let mut f = Self {
            root,
            run,
            project,
            binding,
            task,
            evidence,
            input,
            descriptor,
        };
        f.write()?;
        Ok(f)
    }
    fn write(&mut self) -> Result<()> {
        let put = |name: &str, bytes: Vec<u8>| -> Result<oe_contracts::ArtifactReference> {
            let path = self.run.path().join(name);
            fs::write(&path, &bytes)?;
            Ok(oe_contracts::ArtifactReference {
                path: path
                    .strip_prefix(self.root.path())?
                    .to_string_lossy()
                    .into_owned(),
                sha256: oe_core::sha256(bytes),
            })
        };
        self.input.project = put("project.json", serde_json::to_vec(&self.project)?)?;
        self.input.semantic_binding = put("binding.json", serde_json::to_vec(&self.binding)?)?;
        self.input.task = put("task.json", serde_json::to_vec(&self.task)?)?;
        self.input.evidence = put("evidence.ttl", self.evidence.as_bytes().to_vec())?;
        Ok(())
    }
    fn bind_evidence(&mut self) -> Result<()> {
        let hash = oe_core::sha256(self.evidence.as_bytes());
        self.task["evidence"][0]["sha256"] = json!(hash);
        self.project.evidence.insert("source:review".into(), hash);
        self.input.expected_context =
            oe_kernel::project::context(&self.project, &self.input.object)?;
        self.write()
    }
    fn review(&self) -> Result<oe_contracts::TypedPromotedProjectReviewReceipt1> {
        oe_kernel::project::review_promoted_current(&self.root, &self.input)
    }
}
fn check<'a>(
    r: &'a oe_contracts::TypedPromotedProjectReviewReceipt1,
    id: &str,
) -> &'a oe_contracts::ProjectCheckObservation1 {
    let values = r.checks.iter().filter(|c| c.id == id).collect::<Vec<_>>();
    assert_eq!(values.len(), 1);
    values[0]
}

#[test]
fn typed_promoted_project_actual_clear_and_current_condition_controls() -> Result<()> {
    let mut f = Fixture::new("urn:review:focus")?;
    let original = fs::read(f.root.path().join(&f.input.project.path))?;
    let good = f.review()?;
    assert_eq!(
        (
            &*good.execution,
            &*good.conclusion,
            &*good.project_conclusion
        ),
        ("completed", "pass", "pass")
    );
    assert_eq!(good.semantic_review.review_v1["status"], "clear");
    assert_eq!(
        good.registry.logical_package_sha256,
        f.descriptor["package_sha256"]
    );
    assert_eq!(good.registry.version, f.descriptor["version"]);
    assert_eq!(
        good.source_observation["source_recheck"]["status"],
        "matched"
    );
    assert_eq!(
        good.source_observation["typed_project_recheck"]["status"],
        "matched"
    );
    assert_eq!(good.engineering_adoption, "unknown");
    assert!(!good.fact_admitted);
    assert_eq!(good.checks.len(), 11);
    for id in &f.input.required_checks {
        assert_eq!(check(&good, id).conclusion, "pass");
    }
    assert_eq!(
        fs::read(f.root.path().join(&f.input.project.path))?,
        original
    );
    f.input.focus_type_iri = "urn:wrong:type".into();
    let wrong_type = f.review()?;
    assert_eq!(check(&wrong_type, "promoted_focus").conclusion, "violated");
    assert_eq!(wrong_type.project_conclusion, "pass");
    assert_eq!(wrong_type.conclusion, "violated");
    f.input.focus_type_iri = "urn:review:Record".into();
    f.input.query_asset_id = "ordinary-query".into();
    f.input.required_checks[0] = "ordinary-query".into();
    let uninterpreted = f.review()?;
    assert_eq!(
        uninterpreted.semantic_review.query_observation["execution"],
        "completed"
    );
    assert_eq!(
        uninterpreted.semantic_review.interpretation["status"],
        "unknown"
    );
    assert_eq!(uninterpreted.conclusion, "unknown");
    assert_eq!(uninterpreted.project_conclusion, "pass");
    f.input.query_asset_id = "review-query".into();
    f.input.required_checks[0] = "review-query".into();
    let old_context = f.input.expected_context.clone();
    let mut condition: Value = serde_json::from_str(&f.project.conditions["scope:fluid"])?;
    condition["facts"][0]["value"] = json!("acid");
    f.project
        .conditions
        .insert("scope:fluid".into(), serde_json::to_string(&condition)?);
    f.write()?;
    let stale = f.review()?;
    assert_eq!(stale.semantic_review.review_v1["status"], "clear");
    assert_eq!(
        check(&stale, "current_project_context").conclusion,
        "violated"
    );
    assert_eq!(
        check(&stale, "current_condition_model").conclusion,
        "violated"
    );
    assert_eq!(
        check(&stale, "engineering:fluid-scope").conclusion,
        "violated"
    );
    f.input.expected_context = oe_kernel::project::context(&f.project, &f.input.object)?;
    assert_ne!(f.input.expected_context, old_context);
    let rebound = f.review()?;
    assert_eq!(
        check(&rebound, "current_project_context").conclusion,
        "pass"
    );
    assert_eq!(
        check(&rebound, "current_condition_model").conclusion,
        "violated"
    );
    assert_eq!(rebound.conclusion, "violated");
    Ok(())
}

#[test]
fn typed_promoted_project_binding_authority_and_coverage_controls() -> Result<()> {
    let mut f = Fixture::new("urn:review:focus")?;
    f.input
        .required_checks
        .retain(|id| id != "current_condition_model");
    let omitted = f.review()?;
    assert_eq!(omitted.conclusion, "violated");
    assert!(
        omitted
            .failures
            .iter()
            .any(|s| s == "typed_project_required_check_omitted:current_condition_model")
    );
    assert_eq!(
        check(&omitted, "current_condition_model").conclusion,
        "pass"
    );
    f.input
        .required_checks
        .push("current_condition_model".into());
    let original_binding = f.binding.clone();
    f.binding["baseline"]["version"] = json!("old");
    f.write()?;
    let stale_error = f.review().unwrap_err();
    let stale = lifecycle::error_response("review", &stale_error);
    assert_eq!(stale["native_execution"]["semantic_execution"], "failed");
    assert!(
        stale["native_execution"]["error"]
            .as_str()
            .unwrap()
            .contains("review_current_baseline_mismatch")
    );
    f.binding = original_binding;
    let original_task = f.task.clone();
    // actor_id is recorded metadata under the original review contract. The
    // original authority guard requires this task to request the actual action.
    f.task["requested_actions"] = json!(["propose"]);
    f.write()?;
    let authority_error = f.review().unwrap_err();
    let authority = lifecycle::error_response("review", &authority_error);
    assert_eq!(
        authority["execution"]["error_type"],
        "ActionNotAllowedError"
    );
    assert_eq!(
        authority["execution"]["message"],
        "ActionNotAllowedError: current action not requested by task"
    );
    f.task = original_task;
    f.write()?;
    let evidence = f.input.evidence.clone();
    f.input.evidence.sha256 = "a".repeat(64);
    let source_error = f.review().unwrap_err();
    let source = lifecycle::error_response("review", &source_error);
    assert_eq!(
        source["execution"]["message"],
        "typed_review_source_changed"
    );
    f.input.evidence = evidence;
    f.project.project_id = "other-project".into();
    f.write()?;
    let error = f.review().unwrap_err();
    let partial = lifecycle::error_response("review", &error);
    assert_eq!(
        partial["native_execution"]["result"]["review_observation"]["review_v1"]["status"],
        "clear"
    );
    assert_eq!(partial["native_execution"]["semantic_execution"], "failed");
    assert!(
        partial
            .to_string()
            .contains("typed_project_owner_identity_mismatch")
    );
    Ok(())
}

#[test]
fn typed_promoted_project_partial_and_source_changes_retained() -> Result<()> {
    // The actual existing query budget rejects only evidence focus querying.
    // SHACL targets the short class IRI, so its independent observation completes.
    let mut f = Fixture::new(&format!("urn:focus:{}", "x".repeat(1024 * 1024)))?;
    let partial = f.review()?;
    assert_eq!(
        partial.semantic_review.focus_observation["execution"],
        "failed"
    );
    assert_eq!(
        partial.semantic_review.query_observation["execution"],
        "completed"
    );
    assert_eq!(
        partial.semantic_review.shape_observation["execution"],
        "completed"
    );
    assert!(
        partial.semantic_review.review_v1["findings"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(partial.semantic_review.conclusion, "unknown");
    assert_eq!(partial.conclusion, "unknown");
    assert_eq!(partial.engineering_adoption, "unknown");
    // A real query counterexample stays violated even with another partial failure.
    f.evidence
        .push_str("\n<urn:other> <urn:review:bad> \"counterexample\" .");
    f.bind_evidence()?;
    let violated = f.review()?;
    assert_eq!(violated.conclusion, "violated");
    assert_eq!(check(&violated, "review-query").conclusion, "violated");
    assert_eq!(
        violated.semantic_review.focus_observation["execution"],
        "failed"
    );
    // Nonempty source bytes can contain no RDF quads. This is an observed
    // counterexample even when the independent focus query exceeds its budget.
    f.evidence = "# actual nonempty Turtle bytes with no triples\n".into();
    f.bind_evidence()?;
    let empty = f.review()?;
    assert_eq!(empty.semantic_review.evidence["input_quad_count"], 0);
    assert_eq!(
        empty.semantic_review.focus_observation["execution"],
        "failed"
    );
    assert!(
        empty.semantic_review.review_v1["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["finding"] == "input:empty-abox")
    );
    assert_eq!(empty.semantic_review.conclusion, "violated");
    assert_eq!(empty.execution, "partial");
    assert_eq!(check(&empty, "promoted_input").conclusion, "violated");
    assert_eq!(
        empty.conclusion, "violated",
        "completed empty-dataset observation must survive independent focus failure"
    );
    // Exercise the original source recheck after a genuine M review. The observer
    // returns a diagnostic, never a fabricated engineering passed receipt.
    let f = Fixture::new("urn:review:focus")?;
    let binding = f.root.path().join(&f.input.semantic_binding.path);
    let task = f.root.path().join(&f.input.task.path);
    let evidence = f.root.path().join(&f.input.evidence.path);
    let workspace = PathBuf::from(&f.input.workspace);
    let request = ProjectReviewRequest {
        binding: &binding,
        workspace: &workspace,
        task: &task,
        evidence_file: &evidence,
        source_id: "source:review",
        evidence_format: "turtle",
        scope_id: "scope:fluid",
        focus_iri: "urn:review:focus",
        focus_type_iri: "urn:review:Record",
        query_asset_id: "review-query",
        shape_asset_id: "review-shape",
    };
    let project_path = f.root.path().join(&f.input.project.path);
    let mut observer = |_: &oe_semantic::native::DecisionReviewRequest,
                        result: &semantica_contracts::SemanticPromotedReviewResult1,
                        _: &[Vec<u8>]|
     -> Result<Value> {
        assert_eq!(result.review_v1["status"], "clear");
        fs::write(&project_path, b"changed during observer")?;
        Ok(
            json!({"execution":"completed","conclusion":"unknown","diagnostic":"source change control","engineering_adoption":"unknown"}),
        )
    };
    let error = lifecycle::project_review_observed_at(
        f.root.path(),
        &request,
        Some(WHEN),
        &[f.input.project.clone()],
        Some(&mut observer),
    )
    .unwrap_err();
    let retained = lifecycle::error_response("review", &error);
    assert_eq!(
        retained["native_execution"]["result"]["review_observation"]["query_observation"]["execution"],
        "completed"
    );
    assert_eq!(retained["command_verdict"], "blocked");
    assert!(retained.to_string().contains("changed"));
    Ok(())
}
