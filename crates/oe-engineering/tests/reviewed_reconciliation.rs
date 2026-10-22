mod common;
// Capacity /2 cases below retain the original two /1 regression tests.
use common::Fixture;
use oe_contracts::{
    ArtifactReference, DecisionReviewReplay, ObjectIdentity, ReviewRequest,
    ReviewedReconciliationRequest,
};
use oe_engineering::{artifacts, reviewed_reconciliation::reconcile_reviewed};
use oe_kernel::{
    Binding,
    port::RustSemanticPort,
    project::{self, Project},
};
use serde_json::{Value, json};
use std::fs;

fn put(f: &Fixture, name: &str, v: &Value) -> ArtifactReference {
    fs::write(f.temp.path().join(name), serde_json::to_vec(v).unwrap()).unwrap();
    artifacts::reference(&f.root, &f.temp.path().join(name)).unwrap()
}
fn fixture() -> (Fixture, ReviewedReconciliationRequest) {
    let f = Fixture::new(
        "reviewed-reconciliation-synthetic",
        "required_semantic_review",
    );
    let lock = f.read("runtime/semantic-bundles.json");
    let bundle = &lock["bundles"]["engineering-evidence-methods"];
    let source = put(
        &f,
        "var/inputs/observation.json",
        &json!({"fixture":"synthetic source; not actual business evidence"}),
    );
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"urn:fixture:review","object_revision":"r1","purpose":"Synthetic review coverage only","acceptance":["explicit synthetic scope"],
      "facts":[{"property":"scope","value":"synthetic","status":"declared","source_refs":["raw"]}],
      "assertions":[{"assertion_id":"scope","kind":"source_applicability","subject":"source","requirements":{"limits":{"scope":["synthetic"]}},"source_refs":["raw"],"affected_decisions":["up","down"]}]});
    let mut p:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"synthetic-review","revision":"r2","purpose":"Synthetic review coverage only","objects":{"urn:fixture:review":"r1"},"conditions":{"scope":serde_json::to_string(&basis).unwrap()},"evidence":{"raw":source.sha256},"dependencies":{"down":["up"]},"decisions":[]})).unwrap();
    for id in ["up", "down"] {
        p.decisions.push(project::Decision {
            id: id.into(),
            object_id: "urn:fixture:review".into(),
            object_revision: "r1".into(),
            condition_scope: "scope".into(),
            source_sha256: source.sha256.clone(),
            conclusion: "supported".into(),
            adopted: true,
            basis: vec!["raw".into()],
        });
    }
    let object = ObjectIdentity {
        object_id: "urn:fixture:review".into(),
        revision: "r1".into(),
        condition_scope: "scope".into(),
        source_sha256: source.sha256.clone(),
    };
    let mut old = p.clone();
    old.revision = "r1".into();
    for d in &mut old.decisions {
        d.adopted = false;
    }
    let before = put(
        &f,
        "var/inputs/before.json",
        &serde_json::to_value(old).unwrap(),
    );
    let current = put(
        &f,
        "var/inputs/current.json",
        &serde_json::to_value(&p).unwrap(),
    );
    let binding:Binding=serde_json::from_value(json!({"project_id":p.project_id,"object_id":object.object_id,"object_revision":"r1","condition_scope":"scope","source_sha256":source.sha256,"package_id":bundle["package_id"],"package_version":bundle["package_version"],"package_sha256":bundle["sha256"],"authority_id":"synthetic-no-approval","project_context":project::context(&p,&object).unwrap(),"required_checks":["shape-review-coverage","bound_focus_context","current_project_context","current_condition_model","engineering_purpose","engineering:scope"]})).unwrap();
    let condition_hash = oe_core::sha256(p.conditions["scope"].as_bytes());
    let ttl = format!(
        "@prefix ee: <urn:semantica:engineering-evidence:> .\n<urn:fixture:review> a ee:ReviewClaim ; <urn:oev2:project-context:conditionSha256> \"{condition_hash}\" ; ee:subjectRevision \"r1\" ; ee:scope \"scope\" ; ee:method \"review-coverage\" ; ee:fieldEvidence <urn:source> ; ee:required_methods \"fixture-check\" ; ee:reviewed_methods \"fixture-check\" ; ee:required_methodsComplete true ; ee:reviewed_methodsComplete true ; ee:violated_methodsComplete true ; ee:unknown_methodsComplete true ; ee:claim_revision \"r1\" ; ee:review_revision \"r1\" .\n<urn:source> ee:sourceSha256 \"{}\" .",
        source.sha256
    );
    fs::write(f.temp.path().join("var/inputs/current.ttl"), ttl.as_bytes()).unwrap();
    let port = RustSemanticPort {
        root: f.root.path().into(),
        binding: binding.clone(),
        bundle: "engineering-evidence-methods".into(),
        queries: vec![],
        shapes: vec!["shape-review-coverage".into()],
        evidence: ttl.as_bytes().to_vec(),
    };
    let request = ReviewRequest {
        object,
        evidence_sha256: oe_core::sha256(ttl.as_bytes()),
        required_checks: binding.required_checks.iter().cloned().collect(),
    };
    let saved = project::review_current(
        &f.root,
        &p,
        ttl.as_bytes(),
        port.execute(&request, None).unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved.conclusion,
        "pass",
        "{}",
        serde_json::to_string_pretty(&saved).unwrap()
    );
    let entry = DecisionReviewReplay {
        decision_id: "up".into(),
        binding: put(
            &f,
            "var/inputs/current-binding.json",
            &serde_json::to_value(binding).unwrap(),
        ),
        evidence: artifacts::reference(&f.root, &f.temp.path().join("var/inputs/current.ttl"))
            .unwrap(),
        receipt: put(
            &f,
            "var/inputs/receipt.json",
            &serde_json::to_value(saved).unwrap(),
        ),
        bundle: "engineering-evidence-methods".into(),
        query_ids: vec![],
        shape_ids: vec!["shape-review-coverage".into()],
    };
    let mut second = entry.clone();
    second.decision_id = "down".into();
    (
        f,
        ReviewedReconciliationRequest {
            schema: "oev2.reviewed-reconciliation-request/1".into(),
            before,
            current,
            sources: vec![source],
            reviews: vec![entry, second],
        },
    )
}
#[test]
fn fresh_review_replays_and_preserves_legacy_impact() {
    let (f, r) = fixture();
    let before = artifacts::read(&f.root, &r.current).unwrap();
    let x = reconcile_reviewed(&f.root, &r).unwrap();
    assert_eq!(x.status, "review_complete");
    assert_eq!(x.review_satisfied, vec!["down", "up"]);
    assert_eq!(x.engineering_adoption, "not_granted");
    assert_eq!(x.legacy_impact["stale_adoptions"], json!(["down", "up"]));
    assert_eq!(before, artifacts::read(&f.root, &r.current).unwrap());
    assert!(
        !f.temp
            .path()
            .join("var/state/engineering-adoption-authority.json")
            .exists()
    );
}
#[test]
fn stale_forged_missing_and_partial_reviews_remain_pending() {
    let (f, r) = fixture();
    let mut partial = r.clone();
    partial.reviews.remove(0);
    let x = reconcile_reviewed(&f.root, &partial).unwrap();
    assert!(x.review_satisfied.is_empty());
    assert_eq!(x.still_requires_review, vec!["down", "up"]);
    assert_eq!(x.reviews[0]["reason"], "affected_prerequisite_pending");
    for variant in [
        "executor",
        "forged_check",
        "binding",
        "required",
        "shape",
        "receipt_status",
        "old_context",
    ] {
        let mut q = r.clone();
        q.reviews.truncate(1);
        let e = &mut q.reviews[0];
        match variant {
            "executor" | "forged_check" | "receipt_status" => {
                let mut v = artifacts::json(&f.root, &e.receipt).unwrap();
                if variant == "executor" {
                    v["executable_sha256"] = json!("0".repeat(64));
                } else if variant == "forged_check" {
                    v["checks"][0]["result"] = json!({"forged":true});
                } else {
                    v["conclusion"] = json!("unknown");
                }
                e.receipt = put(&f, &format!("var/inputs/{variant}.json"), &v);
            }
            "binding" | "required" | "old_context" => {
                let mut v = artifacts::json(&f.root, &e.binding).unwrap();
                if variant == "binding" {
                    v["object_revision"] = json!("other");
                } else if variant == "required" {
                    v["required_checks"] = json!(["shape-review-coverage"]);
                } else {
                    v["project_context"]["project_revision"] = json!("old");
                }
                e.binding = put(&f, &format!("var/inputs/{variant}.json"), &v);
            }
            "shape" => e.shape_ids.clear(),
            _ => unreachable!(),
        }
        let x = reconcile_reviewed(&f.root, &q).unwrap();
        assert_eq!(x.status, "review_partial", "{variant}");
        assert!(x.review_satisfied.is_empty(), "{variant}");
    }
    for variant in [
        "missing",
        "duplicate_source",
        "duplicate_review",
        "unknown",
        "oversized_reviews",
    ] {
        let mut q = r.clone();
        match variant {
            "missing" => q.sources.clear(),
            "duplicate_source" => q.sources.push(q.sources[0].clone()),
            "duplicate_review" => q.reviews.push(q.reviews[0].clone()),
            "unknown" => q.reviews[0].decision_id = "absent".into(),
            "oversized_reviews" => q.reviews = vec![q.reviews[0].clone(); 33],
            _ => unreachable!(),
        };
        assert!(reconcile_reviewed(&f.root, &q).is_err(), "{variant}");
    }
    let path = f.temp.path().join(&r.sources[0].path);
    let original = fs::read(&path).unwrap();
    fs::write(&path, b"changed raw source").unwrap();
    assert!(reconcile_reviewed(&f.root, &r).is_err());
    fs::write(path, original).unwrap();
    assert_eq!(
        reconcile_reviewed(&f.root, &r).unwrap().status,
        "review_complete"
    );
}

// Explicit /2 fixtures extend source capacity while retaining /1 and the
// same current kernel, original Project impact and independent adoption.
fn rebind_current(f: &Fixture, r: &mut ReviewedReconciliationRequest, p: &Project) {
    r.current = put(
        f,
        "var/inputs/current.json",
        &serde_json::to_value(p).unwrap(),
    );
    let mut binding: Binding =
        serde_json::from_value(artifacts::json(&f.root, &r.reviews[0].binding).unwrap()).unwrap();
    let object = ObjectIdentity {
        object_id: binding.object_id.clone(),
        revision: binding.object_revision.clone(),
        condition_scope: binding.condition_scope.clone(),
        source_sha256: binding.source_sha256.clone(),
    };
    binding.project_context = Some(project::context(p, &object).unwrap());
    let evidence = artifacts::read(&f.root, &r.reviews[0].evidence).unwrap();
    let request = ReviewRequest {
        object,
        evidence_sha256: oe_core::sha256(&evidence),
        required_checks: binding.required_checks.iter().cloned().collect(),
    };
    let port = RustSemanticPort {
        root: f.root.path().to_owned(),
        binding: binding.clone(),
        bundle: r.reviews[0].bundle.clone(),
        queries: r.reviews[0].query_ids.clone(),
        shapes: r.reviews[0].shape_ids.clone(),
        evidence: evidence.clone(),
    };
    let receipt =
        project::review_current(&f.root, p, &evidence, port.execute(&request, None).unwrap())
            .unwrap();
    assert_eq!(receipt.conclusion, "pass");
    let binding_ref = put(
        f,
        "var/inputs/current-binding.json",
        &serde_json::to_value(binding).unwrap(),
    );
    let receipt_ref = put(
        f,
        "var/inputs/current-receipt.json",
        &serde_json::to_value(receipt).unwrap(),
    );
    for review in &mut r.reviews {
        review.binding = binding_ref.clone();
        review.receipt = receipt_ref.clone();
    }
}

fn expanded_fixture(
    count: usize,
    large: Option<usize>,
) -> (Fixture, ReviewedReconciliationRequest) {
    let (f, mut r) = fixture();
    let mut p: Project =
        serde_json::from_value(artifacts::json(&f.root, &r.current).unwrap()).unwrap();
    for i in 1..count {
        let reference = put(
            &f,
            &format!("var/inputs/source-{i}.json"),
            &json!({"source":i}),
        );
        p.evidence
            .insert(format!("source-{i}"), reference.sha256.clone());
        r.sources.push(reference);
    }
    if let Some(bytes) = large {
        let data = vec![0x5a; bytes];
        let path = "var/inputs/large-source.bin";
        fs::write(f.temp.path().join(path), &data).unwrap();
        let reference = ArtifactReference {
            path: path.into(),
            sha256: oe_core::sha256(&data),
        };
        p.evidence
            .insert("large-source".into(), reference.sha256.clone());
        r.sources.push(reference);
    }
    rebind_current(&f, &mut r, &p);
    (f, r)
}

fn version2(r: &ReviewedReconciliationRequest) -> oe_contracts::ReviewedReconciliationRequest2 {
    oe_contracts::ReviewedReconciliationRequest2 {
        schema: "oev2.reviewed-reconciliation-request/2".into(),
        before: r.before.clone(),
        current: r.current.clone(),
        sources: r.sources.clone(),
        reviews: r.reviews.clone(),
        source_budget: oe_contracts::ReviewedSourceBudget2 {
            max_files: 1024,
            max_file_bytes: 32 * 1024 * 1024,
            max_total_bytes_per_pass: 64 * 1024 * 1024,
        },
    }
}
fn run2(
    f: &Fixture,
    r: &oe_contracts::ReviewedReconciliationRequest2,
) -> anyhow::Result<oe_contracts::ReviewedReconciliationResult2> {
    oe_engineering::reviewed_reconciliation::reconcile_reviewed2(&f.root, r)
}

#[test]
fn explicit_971_sources_match_independent_counts_and_legacy_rejects() {
    let (f, r) = expanded_fixture(971, None);
    assert_eq!(r.sources.len(), 971);
    assert_eq!(
        reconcile_reviewed(&f.root, &r).unwrap_err().to_string(),
        "reviewed_source_limit"
    );
    let q = version2(&r);
    let original = fs::read(f.temp.path().join(&r.current.path)).unwrap();
    let expected_bytes: u64 = q
        .sources
        .iter()
        .map(|s| fs::metadata(f.temp.path().join(&s.path)).unwrap().len())
        .sum();
    let result = run2(&f, &q).unwrap();
    assert_eq!(result.status, "review_complete");
    assert_eq!(result.source_inventory.files, 971);
    assert_eq!(result.source_inventory.unique_digests, 971);
    assert_eq!(result.source_inventory.bytes_per_pass, expected_bytes);
    assert_eq!(result.source_inventory.verification_passes, 2);
    assert_eq!(result.engineering_adoption, "not_granted");
    assert_eq!(result.review_satisfied, vec!["down", "up"]);
    let before: Project =
        serde_json::from_value(artifacts::json(&f.root, &q.before).unwrap()).unwrap();
    let current: Project = serde_json::from_slice(&original).unwrap();
    assert_eq!(
        result.legacy_impact,
        project::reconcile(&before, &current).unwrap()
    );
    assert_eq!(
        fs::read(f.temp.path().join(&r.current.path)).unwrap(),
        original
    );
}

#[test]
fn explicit_large_source_is_fully_hashed_while_legacy_artifact_bound_remains() {
    let (f, r) = expanded_fixture(1, Some(16 * 1024 * 1024 + 1));
    assert!(
        format!("{:#}", reconcile_reviewed(&f.root, &r).unwrap_err())
            .contains("input_exceeds_limit")
    );
    let q = version2(&r);
    let x = run2(&f, &q).unwrap();
    assert_eq!(x.status, "review_complete");
    assert_eq!(x.source_inventory.files, 2);
    assert_eq!(x.source_inventory.unique_digests, 2);
    let expected = fs::metadata(f.temp.path().join(&q.sources[0].path))
        .unwrap()
        .len()
        + 16 * 1024 * 1024
        + 1;
    assert_eq!(x.source_inventory.bytes_per_pass, expected);
    assert_eq!(x.source_inventory.verification_passes, 2);
    let mut small = q.clone();
    small.source_budget.max_file_bytes = 16 * 1024 * 1024;
    assert!(format!("{:#}", run2(&f, &small).unwrap_err()).contains("input_exceeds_limit"));
}

#[test]
fn explicit_source_inventory_rejects_missing_duplicate_undeclared_and_changed() {
    let (f, r) = expanded_fixture(3, None);
    let base = version2(&r);
    for variant in [
        "missing_digest",
        "duplicate_path",
        "undeclared",
        "escape",
        "missing_file",
    ] {
        let mut q = base.clone();
        match variant {
            "missing_digest" => {
                q.sources.pop();
            }
            "duplicate_path" => q.sources.push(q.sources[1].clone()),
            "undeclared" => q.sources[1].sha256 = oe_core::sha256(b"not registered"),
            "escape" => q.sources[1].path = "../outside".into(),
            "missing_file" => q.sources[1].path = "var/inputs/missing".into(),
            _ => unreachable!(),
        }
        assert!(run2(&f, &q).is_err(), "{variant}");
    }
    let path = f.temp.path().join(&base.sources[1].path);
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, b"changed").unwrap();
    assert!(format!("{:#}", run2(&f, &base).unwrap_err()).contains("artifact_source_changed"));
    fs::write(&path, bytes).unwrap();
    assert_eq!(run2(&f, &base).unwrap().status, "review_complete");
}

#[test]
fn explicit_budgets_bound_count_file_and_total_before_success() {
    let (f, r) = expanded_fixture(3, None);
    let base = version2(&r);
    for variant in [
        "count",
        "file",
        "total",
        "zero_count",
        "zero_file",
        "zero_total",
        "hard_count",
        "hard_file",
        "hard_total",
    ] {
        let mut q = base.clone();
        match variant {
            "count" => q.source_budget.max_files = 2,
            "file" => q.source_budget.max_file_bytes = 1,
            "total" => q.source_budget.max_total_bytes_per_pass = 1,
            "zero_count" => q.source_budget.max_files = 0,
            "zero_file" => q.source_budget.max_file_bytes = 0,
            "zero_total" => q.source_budget.max_total_bytes_per_pass = 0,
            "hard_count" => q.source_budget.max_files = 4097,
            "hard_file" => q.source_budget.max_file_bytes = 256 * 1024 * 1024 + 1,
            "hard_total" => q.source_budget.max_total_bytes_per_pass = 2 * 1024 * 1024 * 1024 + 1,
            _ => unreachable!(),
        };
        assert!(run2(&f, &q).is_err(), "{variant}");
    }
    let total: u64 = base
        .sources
        .iter()
        .map(|s| fs::metadata(f.temp.path().join(&s.path)).unwrap().len())
        .sum();
    let mut q = base.clone();
    q.source_budget.max_total_bytes_per_pass = total;
    assert_eq!(run2(&f, &q).unwrap().source_inventory.bytes_per_pass, total);
    q.source_budget.max_total_bytes_per_pass = total - 1;
    assert!(run2(&f, &q).is_err());
}

#[test]
fn explicit_duplicate_content_counts_paths_but_alias_ids_need_no_extra_file() {
    let (f, mut r) = expanded_fixture(2, None);
    let mut p: Project =
        serde_json::from_value(artifacts::json(&f.root, &r.current).unwrap()).unwrap();
    p.evidence
        .insert("alias".into(), r.sources[1].sha256.clone());
    rebind_current(&f, &mut r, &p);
    let q = version2(&r);
    assert_eq!(run2(&f, &q).unwrap().source_inventory.files, 2);
    let copy = "var/inputs/duplicate-content.json";
    fs::copy(
        f.temp.path().join(&q.sources[1].path),
        f.temp.path().join(copy),
    )
    .unwrap();
    let mut q2 = q.clone();
    q2.sources.push(ArtifactReference {
        path: copy.into(),
        sha256: q.sources[1].sha256.clone(),
    });
    let x = run2(&f, &q2).unwrap();
    assert_eq!(x.source_inventory.files, 3);
    assert_eq!(x.source_inventory.unique_digests, 2);
    assert_eq!(
        x.source_inventory.bytes_per_pass,
        run2(&f, &q).unwrap().source_inventory.bytes_per_pass
            + fs::metadata(f.temp.path().join(copy)).unwrap().len()
    );
    q2.source_budget.max_total_bytes_per_pass = x.source_inventory.bytes_per_pass - 1;
    assert!(run2(&f, &q2).is_err());
}

#[test]
fn explicit_no_reviews_and_pending_old_dependency_preserve_impact() {
    let (f, mut r) = expanded_fixture(2, None);
    let mut p: Project =
        serde_json::from_value(artifacts::json(&f.root, &r.current).unwrap()).unwrap();
    p.dependencies.clear();
    rebind_current(&f, &mut r, &p);
    let mut q = version2(&r);
    q.reviews.retain(|r| r.decision_id == "down");
    let x = run2(&f, &q).unwrap();
    assert_eq!(x.status, "review_partial");
    assert!(x.review_satisfied.is_empty());
    assert_eq!(x.still_requires_review, vec!["down", "up"]);
    q.reviews.clear();
    let y = run2(&f, &q).unwrap();
    assert_eq!(y.status, "review_partial");
    assert_eq!(y.legacy_impact, x.legacy_impact);
    assert_eq!(y.engineering_adoption, "not_granted");
}

#[test]
fn explicit_receipt_context_checks_and_unknown_conclusions_are_not_promoted() {
    let (f, r) = expanded_fixture(2, None);
    let base = version2(&r);
    for variant in ["executor", "checks", "context", "shape"] {
        let mut q = base.clone();
        match variant {
            "executor" => {
                let mut v = artifacts::json(&f.root, &q.reviews[0].receipt).unwrap();
                v["executable_sha256"] = json!("0".repeat(64));
                q.reviews[0].receipt = put(&f, "var/inputs/forged-receipt.json", &v);
            }
            "checks" => {
                let mut v = artifacts::json(&f.root, &q.reviews[0].binding).unwrap();
                v["required_checks"] = json!(["bound_focus_context"]);
                q.reviews[0].binding = put(&f, "var/inputs/forged-binding.json", &v);
            }
            "context" => {
                let mut v = artifacts::json(&f.root, &q.reviews[0].binding).unwrap();
                v["project_context"] = Value::Null;
                q.reviews[0].binding = put(&f, "var/inputs/forged-binding.json", &v);
            }
            "shape" => q.reviews[0].shape_ids.clear(),
            _ => unreachable!(),
        };
        let x = run2(&f, &q).unwrap();
        assert_eq!(x.status, "review_partial", "{variant}");
        assert!(x.review_satisfied.is_empty(), "{variant}");
    }
    for conclusion in ["unknown", "refuted"] {
        let mut p: Project =
            serde_json::from_value(artifacts::json(&f.root, &base.current).unwrap()).unwrap();
        p.decisions[0].conclusion = conclusion.into();
        p.decisions[0].adopted = false;
        let mut q = base.clone();
        q.current = put(
            &f,
            "var/inputs/unknown-current.json",
            &serde_json::to_value(p).unwrap(),
        );
        let bytes = fs::read(f.temp.path().join(&q.current.path)).unwrap();
        let x = run2(&f, &q).unwrap();
        assert_eq!(x.status, "review_partial");
        assert!(x.review_satisfied.is_empty());
        assert_eq!(
            fs::read(f.temp.path().join(&q.current.path)).unwrap(),
            bytes
        );
    }
}

#[test]
fn explicit_request_identity_includes_budget_and_schema() {
    let (f, r) = fixture();
    let q = version2(&r);
    let legacy = reconcile_reviewed(&f.root, &r).unwrap();
    let a = run2(&f, &q).unwrap();
    assert_ne!(a.request_sha256, legacy.request_sha256);
    assert_eq!(a.legacy_impact, legacy.legacy_impact);
    assert_eq!(a.review_satisfied, legacy.review_satisfied);
    assert_eq!(a.reviews, legacy.reviews);
    let mut changed = q.clone();
    changed.source_budget.max_files -= 1;
    let b = run2(&f, &changed).unwrap();
    assert_ne!(a.request_sha256, b.request_sha256);
    assert_eq!(a.source_inventory, b.source_inventory);
    changed.schema = "oev2.reviewed-reconciliation-request/1".into();
    assert!(run2(&f, &changed).is_err());
}

#[cfg(unix)]
#[test]
fn explicit_source_symlink_is_rejected_on_unix() {
    let (f, r) = expanded_fixture(2, None);
    let mut q = version2(&r);
    let link = f.temp.path().join("var/inputs/source-link.json");
    std::os::unix::fs::symlink(f.temp.path().join(&q.sources[1].path), &link).unwrap();
    q.sources[1].path = "var/inputs/source-link.json".into();
    assert!(run2(&f, &q).is_err());
}
