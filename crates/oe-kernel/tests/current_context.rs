use oe_contracts::ObjectIdentity;
use oe_kernel::{Binding, project};
use serde_json::json;
use std::collections::BTreeSet;

fn fixture() -> (project::Project, ObjectIdentity) {
    let p=serde_json::from_value(json!({
        "schema":"oev2.project/1","project_id":"same-scope","revision":"r1","purpose":"diagnostic, not adoption",
        "objects":{"urn:load":"r1"},"conditions":{"loadcase":"load=10 N; support=cantilever"},
        "evidence":{"input":oe_core::sha256(b"old RDF")},"decisions":[],"dependencies":{}
    })).unwrap();
    let object = ObjectIdentity {
        object_id: "urn:load".into(),
        revision: "r1".into(),
        condition_scope: "loadcase".into(),
        source_sha256: oe_core::sha256(b"old RDF"),
    };
    (p, object)
}

#[test]
fn same_named_condition_change_invalidates_current_use_but_not_history() {
    let (p, object) = fixture();
    let expected = project::context(&p, &object).unwrap();
    assert_eq!(
        project::check_current_context(&p, &object, Some(&expected)).conclusion,
        "pass"
    );
    for value in [
        "load=20 N; support=cantilever",
        "load=10 N; support=simple",
        "load=10 N; support=cantilever; temperature unknown",
    ] {
        let mut changed = p.clone();
        changed.conditions.insert("loadcase".into(), value.into());
        let check = project::check_current_context(&changed, &object, Some(&expected));
        assert_eq!(check.conclusion, "violated");
        assert!(
            check
                .failures
                .contains(&"current_project_context_changed".into())
        );
        assert_eq!(
            project::check_current_context(&p, &object, Some(&expected)).conclusion,
            "pass",
            "unchanged historical snapshot retains its original meaning"
        );
    }
    assert_eq!(
        project::check_current_context(&p, &object, None).conclusion,
        "unknown"
    );
}

#[test]
fn project_object_source_and_revision_are_checked_in_addition_to_conditions() {
    let (p, object) = fixture();
    let context = project::context(&p, &object).unwrap();
    let mut versions = vec![p.clone(); 4];
    versions[0].revision = "r2".into();
    versions[1]
        .objects
        .insert(object.object_id.clone(), "r2".into());
    versions[2]
        .evidence
        .insert("input".into(), oe_core::sha256(b"new source"));
    versions[3].project_id = "other".into();
    for changed in versions {
        assert_eq!(
            project::check_current_context(&changed, &object, Some(&context)).conclusion,
            "violated"
        );
    }
}

#[test]
fn condition_change_invalidates_adoption_without_a_manual_dependency_edge() {
    let (mut before, object) = fixture();
    before.decisions.push(project::Decision {
        id: "select".into(),
        object_id: object.object_id.clone(),
        object_revision: object.revision.clone(),
        condition_scope: object.condition_scope.clone(),
        source_sha256: object.source_sha256.clone(),
        conclusion: "supported".into(),
        adopted: true,
        basis: vec!["input".into()],
    });
    let mut after = before.clone();
    after
        .conditions
        .insert("loadcase".into(), "load=20 N; support=cantilever".into());
    let report = project::reconcile(&before, &after).unwrap();
    assert_eq!(report["stale_adoptions"], json!(["select"]));
    assert_eq!(
        before.conditions["loadcase"],
        "load=10 N; support=cantilever"
    );
}

#[test]
fn legacy_binding_keeps_its_exact_digest_and_receipt_diagnostics() {
    let (p, object) = fixture();
    let raw = json!({"project_id":p.project_id,"object_id":object.object_id,"object_revision":"r1",
        "package_id":"fixture","package_version":"1","package_sha256":oe_core::sha256(b"package"),
        "source_sha256":object.source_sha256,"condition_scope":"loadcase","authority_id":"diagnostic",
        "required_checks":["old-check"]});
    let binding: Binding = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(
        binding.digest().unwrap(),
        oe_core::sha256(oe_wire::json::canonical_json(&raw).unwrap())
    );
    let receipt = oe_kernel::evaluate(
        binding,
        b"old RDF",
        vec![oe_kernel::CheckResult {
            id: "old-check".into(),
            execution: "completed".into(),
            conclusion: "pass".into(),
            coverage: vec![],
            failures: vec![],
            result: json!({"independent_expected":"pass"}),
        }],
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("SKILL.md"), "fixture").unwrap();
    let root = oe_core::SkillRoot::new(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let current = project::review_current(&root, &p, b"old RDF", receipt.clone()).unwrap();
    assert_eq!(receipt.conclusion, "pass");
    assert_eq!(current.conclusion, "unknown");
    assert_eq!(current.checks[0].result, receipt.checks[0].result);
    assert_eq!(
        current.coverage,
        BTreeSet::from(["old-check".into(), "current_project_context".into(),])
    );
    assert!(
        current
            .checks
            .iter()
            .any(|check| check.id == "current_condition_model"
                && check.execution == "failed"
                && check.conclusion == "unknown")
    );
}

#[test]
fn fresh_context_cannot_resign_rdf_bound_to_old_condition_contents() {
    let (mut p, mut object) = fixture();
    let old_hash = oe_core::sha256(p.conditions["loadcase"].as_bytes());
    let evidence =
        format!("<urn:load> <urn:oev2:project-context:conditionSha256> \"{old_hash}\" .\n");
    object.source_sha256 = oe_core::sha256(evidence.as_bytes());
    p.evidence
        .insert("input".into(), object.source_sha256.clone());
    assert_eq!(
        project::check_condition_model(&p, &object, evidence.as_bytes()).conclusion,
        "pass"
    );
    p.conditions
        .insert("loadcase".into(), "load=20 N; support=cantilever".into());
    let fresh = project::context(&p, &object).unwrap();
    assert_eq!(
        project::check_current_context(&p, &object, Some(&fresh)).conclusion,
        "pass"
    );
    assert_eq!(
        project::check_condition_model(&p, &object, evidence.as_bytes()).conclusion,
        "violated"
    );
    let absent = b"<urn:load> <urn:label> \"old unbound model\" .";
    object.source_sha256 = oe_core::sha256(absent);
    p.evidence
        .insert("input".into(), object.source_sha256.clone());
    assert_eq!(
        project::check_condition_model(&p, &object, absent).conclusion,
        "unknown"
    );
}
