//! Actual full promotion followed by independently authored project evidence.
//! The original full12 source plan and all original tests/oracles are untouched.
use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use anyhow::{Context, Result, ensure};
use base64::Engine;
use semantica_contracts::SemanticPromotedReviewRequest1;
use semantica_runtime::{
    books::SourceProvider,
    promoted_review::{self, interpret_findings_v1},
    refinery::{RefineryEngine, RefineryStore},
    refinery_contracts as wire,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const WHEN: &str = "2026-08-19T00:00:00Z";
const REGISTRY: &str = "registry-original-full12";

const POSITIVE: &str = "<urn:review:focus> a <urn:review:Record>; <urn:review:name> \"Ada\" .";
const SHAPES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:review:Shape> a sh:NodeShape; sh:targetClass <urn:review:Record>; sh:property [ sh:path <urn:review:name>; sh:minCount 1 ] .";

struct Store(tempfile::TempDir);
impl Store {
    fn new() -> Result<Self> {
        let result = Self(tempfile::tempdir()?);
        RefineryEngine::initialize(&result, REGISTRY, WHEN)?;
        Ok(result)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }
}
impl RefineryStore for Store {
    fn read(&self, p: &str) -> Result<Option<Vec<u8>>> {
        let p = self.file(p);
        Ok(if p.try_exists()? {
            Some(fs::read(p)?)
        } else {
            None
        })
    }
    fn write_new(&self, p: &str, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.file(p))?
            .write_all(bytes)?;
        Ok(())
    }
    fn replace(&self, p: &str, expected: &[u8], bytes: &[u8]) -> Result<()> {
        ensure!(fs::read(self.file(p))? == expected, "test CAS mismatch");
        fs::write(self.file(p), bytes)?;
        Ok(())
    }
    fn remove(&self, p: &str, expected: &[u8]) -> Result<()> {
        ensure!(fs::read(self.file(p))? == expected, "test CAS mismatch");
        fs::remove_file(self.file(p))?;
        Ok(())
    }
    fn list(&self, p: &str) -> Result<Vec<String>> {
        let mut result = fs::read_dir(self.file(p))?
            .map(|e| Ok(e?.file_name().to_string_lossy().into_owned()))
            .collect::<Result<Vec<_>>>()?;
        result.sort();
        Ok(result)
    }
    fn create_dir(&self, p: &str) -> Result<()> {
        fs::create_dir(self.file(p))?;
        Ok(())
    }
    fn publish_dir(&self, stage: &str, dst: &str) -> Result<()> {
        ensure!(!self.file(dst).exists(), "test publish destination exists");
        fs::rename(self.file(stage), self.file(dst))?;
        Ok(())
    }
    fn discard_stage(&self, stage: &str) -> Result<()> {
        fs::remove_dir_all(self.file(stage))?;
        Ok(())
    }
}
struct NoExternalInput;
impl SourceProvider for NoExternalInput {
    fn read(&self, _: &str) -> Result<Vec<u8>> {
        anyhow::bail!("unexpected external fixture read")
    }
}

fn source() -> Result<Value> {
    let root = std::env::var_os("SEMANTICA_TEST_SOURCE_BUNDLE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let root = root.canonicalize()?;
    let generation: Value =
        serde_json::from_slice(&fs::read(root.join("generated/generation.json"))?)?;
    let sources = &generation["source_inputs"];
    for (path, digest) in sources.as_object().context("source inputs map")? {
        ensure!(
            semantica_runtime::sha256(fs::read(root.join(path))?) == digest.as_str().unwrap(),
            "actual source differs: {path}"
        );
    }
    let executable = fs::read(std::env::current_exe()?)?;
    Ok(json!({"schema":"semantica.refinery-rust-source/1",
        "source_sha256":semantica_runtime::refinery::hash(sources)?,
        "executable_sha256":semantica_runtime::sha256(&executable),
        "executable_size_bytes":executable.len(),"status":"source_bound",
        "compilation_origin":"unknown","engineering_adoption":"unknown"}))
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

fn promote(store: &Store) -> Result<Value> {
    let p = plan()?;
    let e = RefineryEngine::new(store, REGISTRY)?;
    let source = source()?;
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
        &source,
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
    let executed = e.execute_current(digest, &p["contexts"]["execute_candidate"], &source, WHEN)?;
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

fn request(descriptor: &Value, evidence: &str) -> SemanticPromotedReviewRequest1 {
    SemanticPromotedReviewRequest1 {
        schema: promoted_review::REQUEST.into(),
        review_contract: promoted_review::REVIEW_CONTRACT.into(),
        registry_id: REGISTRY.into(),
        package_id: descriptor["package_id"].as_str().unwrap().into(),
        expected_current_version: descriptor["version"].as_str().unwrap().into(),
        expected_registry_package_sha256: descriptor["package_sha256"].as_str().unwrap().into(),
        evidence_bytes: evidence.as_bytes().to_vec(),
        evidence_sha256: semantica_runtime::sha256(evidence),
        evidence_format: "turtle".into(),
        evidence_id: "source:actual-review".into(),
        evidence_uri: "urn:example:evidence-root:actual-review".into(),
        evidence_captured_at: WHEN.into(),
        scope_id: "task:review".into(),
        focus_iri: "urn:review:focus".into(),
        focus_type_iri: "urn:review:Record".into(),
        query_asset_id: "review-query".into(),
        shape_asset_id: "review-shape".into(),
        created_at: WHEN.into(),
    }
}

#[test]
fn real_promoted_review_clear_violation_unknown() -> Result<()> {
    let store = Store::new()?;
    let descriptor = promote(&store)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    let clear = e.review_promoted_current(&request(&descriptor, POSITIVE))?;
    assert_eq!(clear.conclusion, "pass");
    assert_eq!(clear.review_v1["status"], "clear");
    assert_eq!(clear.query_observation["result"]["rows"], json!([]));
    assert_eq!(clear.shape_observation["result"]["focus_count"], 1);
    assert_eq!(clear.engineering_adoption, "unknown");
    assert_eq!(clear.typed_project_checks, "not_run");
    assert!(!clear.fact_admitted);
    let mut content = serde_json::to_value(&clear)?;
    content.as_object_mut().unwrap().remove("report_sha256");
    assert_eq!(
        clear.report_sha256,
        semantica_runtime::refinery::hash(&content)?
    );
    let bad = format!("{POSITIVE} <urn:review:focus> <urn:review:bad> \"independent-negative\" .");
    let violation = e.review_promoted_current(&request(&descriptor, &bad))?;
    assert_eq!(violation.conclusion, "violated");
    assert_eq!(
        violation.review_v1["findings"][0]["finding"],
        "independent-negative"
    );
    let shape = e.review_promoted_current(&request(
        &descriptor,
        "<urn:review:focus> a <urn:review:Record> .",
    ))?;
    assert_eq!(shape.conclusion, "violated");
    assert_eq!(shape.shape_observation["result"]["conforms"], false);
    let mut unknown = request(&descriptor, POSITIVE);
    unknown.query_asset_id = "ordinary-query".into();
    let unknown = e.review_promoted_current(&unknown)?;
    assert_eq!(unknown.conclusion, "unknown");
    assert_eq!(unknown.review_v1["status"], "blocked");
    assert_eq!(
        unknown.query_observation["result"]["rows"][0]["name"]["value"],
        "Ada"
    );
    Ok(())
}

#[test]
fn stale_baseline_and_asset_tamper_refuse_review() -> Result<()> {
    let store = Store::new()?;
    let descriptor = promote(&store)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    let original = request(&descriptor, POSITIVE);
    let mut wrong = original.clone();
    wrong.expected_current_version = "old".into();
    assert!(
        e.review_promoted_current(&wrong)
            .unwrap_err()
            .to_string()
            .contains("current_baseline")
    );
    let mut wrong = original.clone();
    wrong.expected_registry_package_sha256 = "f".repeat(64);
    assert!(e.review_promoted_current(&wrong).is_err());
    let mut wrong = original.clone();
    wrong.registry_id = "different".into();
    assert!(e.review_promoted_current(&wrong).is_err());
    let mut wrong = original.clone();
    wrong.evidence_sha256 = "f".repeat(64);
    assert!(e.review_promoted_current(&wrong).is_err());
    let mut wrong = original.clone();
    wrong.evidence_bytes = vec![0xff];
    wrong.evidence_sha256 = semantica_runtime::sha256(&wrong.evidence_bytes);
    assert!(
        e.review_promoted_current(&wrong)
            .unwrap_err()
            .to_string()
            .contains("not_utf8")
    );
    let mut wrong = original.clone();
    wrong.query_asset_id = "review-shape".into();
    assert!(e.review_promoted_current(&wrong).is_err());
    // Unselected original asset must still be validated, not only query/shape.
    let p = plan()?;
    let sha = p["factory_delta"]["cases"][0]["sha256"].as_str().unwrap();
    fs::write(
        store.file(&format!("objects/sha256/{}/{sha}", &sha[..2])),
        b"tampered",
    )?;
    assert!(e.review_promoted_current(&original).is_err());
    let empty = Store::new()?;
    assert!(
        RefineryEngine::new(&empty, REGISTRY)?
            .review_promoted_current(&original)
            .is_err()
    );
    Ok(())
}

#[test]
fn evidence_focus_and_named_graph_partial_are_preserved() -> Result<()> {
    let store = Store::new()?;
    let descriptor = promote(&store)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    for evidence in [
        "<urn:other> a <urn:review:Record>; <urn:review:name> \"Ada\" .",
        "<urn:review:focus> a <urn:wrong>; <urn:review:name> \"Ada\" .",
        "<urn:review:focus> a <urn:review:Record>; <urn:review:name> \"Ada\" . <urn:second> a <urn:review:Record>; <urn:review:name> \"Bea\" .",
    ] {
        let observed = e.review_promoted_current(&request(&descriptor, evidence))?;
        assert_eq!(observed.focus_observation["execution"], "completed");
        assert_eq!(observed.conclusion, "violated");
        assert_eq!(observed.review_v1["status"], "blocked");
    }
    assert!(
        e.review_promoted_current(&request(&descriptor, ""))
            .is_err()
    );
    let data = format!("{POSITIVE} <urn:graph> {{ <urn:x> <urn:p> <urn:y> . }}");
    let mut named = request(&descriptor, &data);
    named.evidence_format = "trig".into();
    let result = e.review_promoted_current(&named)?;
    assert_eq!(result.execution, "partial");
    assert_eq!(result.conclusion, "unknown");
    assert_eq!(result.query_observation["execution"], "completed");
    assert_eq!(result.query_observation["result"]["rows"], json!([]));
    assert_eq!(result.shape_observation["execution"], "failed");
    assert!(
        result
            .failures
            .iter()
            .any(|v| v["message"].as_str().unwrap().contains("named_graph"))
    );
    // A legal evidence IRI can exceed the focus ASK query budget while the
    // short package query and direct SHACL evaluation still complete.
    let long_focus = format!("urn:review:{}", "x".repeat(1024 * 1024));
    let evidence = format!("<{long_focus}> a <urn:review:Record>; <urn:review:name> \"Ada\" .");
    let mut long_request = request(&descriptor, &evidence);
    long_request.focus_iri = long_focus;
    let limited = e.review_promoted_current(&long_request)?;
    assert_eq!(limited.execution, "partial");
    assert_eq!(limited.conclusion, "unknown");
    assert_eq!(limited.review_v1["status"], "blocked");
    assert_eq!(limited.focus_observation["execution"], "failed");
    assert!(limited.focus_observation["result"].is_null());
    assert_eq!(limited.query_observation["execution"], "completed");
    assert_eq!(limited.query_observation["result"]["rows"], json!([]));
    assert_eq!(limited.shape_observation["execution"], "completed");
    assert_eq!(limited.shape_observation["result"]["conforms"], true);
    assert_eq!(limited.shape_observation["result"]["focus_count"], 1);
    assert_eq!(limited.dataset_observation["input"]["quad_count"], 2);
    assert_eq!(limited.review_v1["findings"], json!([]));
    assert!(limited.failures.iter().any(|failure| {
        failure["phase"] == "focus"
            && failure["message"]
                .as_str()
                .is_some_and(|message| message.contains("query_byte_budget"))
    }));
    assert_eq!(limited.engineering_adoption, "unknown");
    assert!(!limited.fact_admitted);
    // The old byte entry and the shared graph entry compute the same real
    // finding on this independently authored minCount counterexample.
    let bytes = b"<urn:review:focus> a <urn:review:Record> .";
    let data = semantica_runtime::graph::load(bytes)?;
    let old = semantica_runtime::shacl::validate(bytes, SHAPES.as_bytes())?;
    let shared = semantica_runtime::shacl::validate_dataset(&data, SHAPES.as_bytes())?;
    assert_eq!(old["violation_count"], 1);
    assert_eq!(shared["violation_count"], 1);
    // Anonymous shape labels belong to each parse, so compare the existing
    // v1 transport meaning without claiming cross-parse blank-node identity.
    assert_eq!(
        promoted_review::violations_v1(&old["findings"])?,
        promoted_review::violations_v1(&shared["findings"])?
    );
    // Named shape identities also permit a strict comparison of every raw
    // result field, including source_shape, across the two real entry points.
    let named_shapes = b"@prefix sh: <http://www.w3.org/ns/shacl#> . <urn:review:Shape> a sh:NodeShape; sh:targetClass <urn:review:Record>; sh:property <urn:review:NameShape> . <urn:review:NameShape> sh:path <urn:review:name>; sh:minCount 1 .";
    let old_named = semantica_runtime::shacl::validate(bytes, named_shapes)?;
    let shared_named = semantica_runtime::shacl::validate_dataset(&data, named_shapes)?;
    assert_eq!(old_named["violation_count"], 1);
    assert_eq!(
        old_named["findings"][0]["source_shape"],
        "<urn:review:NameShape>"
    );
    assert_eq!(old_named, shared_named);
    Ok(())
}

#[test]
fn existing_findings_contract_rejects_unknown_or_unbound_queries() -> Result<()> {
    let store = Store::new()?;
    let descriptor = promote(&store)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    for id in [
        "ordinary-query",
        "ask-query",
        "unbound-query",
        "empty-finding-query",
    ] {
        let mut input = request(&descriptor, POSITIVE);
        input.query_asset_id = id.into();
        let result = e.review_promoted_current(&input)?;
        assert_eq!(result.query_observation["execution"], "completed");
        assert_eq!(result.interpretation["status"], "unknown");
        assert_eq!(result.conclusion, "unknown");
        assert_eq!(result.review_v1["status"], "blocked");
    }
    assert!(interpret_findings_v1(&json!({"kind":"construct","triples":[]})).is_err());
    Ok(())
}

#[test]
fn v1_violation_projection_uses_frozen_field_meanings_and_order() -> Result<()> {
    // Frozen wheel runtime.py:2132-2174 and decision_review.py:413-427 specify
    // these values independently of the Rust violation's display strings.
    let shapes = r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
        <urn:review:Shape> a sh:NodeShape; sh:targetClass <urn:review:Record>;
            sh:property [ sh:path <urn:review:name>; sh:minCount 1;
                sh:severity sh:Warning; sh:message "zeta", "alpha\nline" ] ."#;
    let actual = semantica_runtime::shacl::validate(
        b"<urn:review:focus> a <urn:review:Record> .",
        shapes.as_bytes(),
    )?;
    let projected = promoted_review::violations_v1(&actual["findings"])?;
    assert_eq!(
        projected,
        vec![json!({"focus":"urn:review:focus","path":"urn:review:name",
        "source_constraint":"MinCountConstraintComponent","severity":"Warning","message":"alpha\nline\nzeta"})]
    );
    assert_eq!(actual["findings"][0]["focus"], "<urn:review:focus>");
    assert_eq!(
        actual["findings"][0]["severity"],
        "http://www.w3.org/ns/shacl#Warning"
    );
    let data = semantica_runtime::graph::load(b"")?;
    let query = semantica_runtime::graph::query(
        &data,
        "SELECT ?finding WHERE { VALUES ?finding { \"zeta\"@en \"alpha\"@en } }",
    )?;
    let finding = interpret_findings_v1(&query)?;
    assert_eq!(finding[0]["finding"], "alpha");
    assert_eq!(finding[1]["finding"], "zeta");
    assert_eq!(
        finding[0]["bindings"]["finding"],
        json!({"value":"alpha","term_type":"literal","datatype":null,"language":"en"})
    );
    Ok(())
}

#[test]
fn deactivated_shape_retains_v1_clear_without_claiming_active_coverage() -> Result<()> {
    let store = Store::new()?;
    let descriptor = promote(&store)?;
    let e = RefineryEngine::new(&store, REGISTRY)?;
    let mut input = request(&descriptor, POSITIVE);
    input.shape_asset_id = "deactivated-shape".into();
    let result = e.review_promoted_current(&input)?;
    assert_eq!(result.review_v1["status"], "clear");
    assert_eq!(result.shape_observation["result"]["focus_count"], 1);
    assert_eq!(
        result.dataset_observation["active_constraint_coverage"],
        "unknown"
    );
    assert_eq!(
        result.dataset_observation["engineering_required_checks"],
        "not_evaluated"
    );
    assert_eq!(result.engineering_adoption, "unknown");
    assert!(!result.fact_admitted);
    Ok(())
}
