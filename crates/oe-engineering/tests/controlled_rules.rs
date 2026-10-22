mod common;
use common::Fixture;
use oe_contracts::{
    ControlledVersionDefinition, ObjectIdentity, ReviewRequest, VersionSelectionRequest,
};
use oe_engineering::{artifacts, registry};
use oe_kernel::governance::{self, Event};
use serde_json::{Value, json};
use std::{fs, path::Path};
fn put(f: &Fixture, p: &str, v: &Value) {
    let path = f.temp.path().join(p);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    oe_core::write_new_bytes(&path, &oe_core::json::canonical_json(v).unwrap()).unwrap();
}
#[test]
fn rule_activation_requires_actual_governance_and_the_selected_package_is_executed() {
    let f = Fixture::new("rule-version", "required_semantic_review");
    let bundle = oe_semantic::load_bundle(f.root.path(), "engineering-judgment-intake").unwrap();
    put(&f, "var/inputs/rule-spec.json", &bundle.spec);
    let name = "private:var/inputs/rule-spec.json";
    let loaded = oe_semantic::load_bundle(f.root.path(), name).unwrap();
    assert_eq!(loaded.spec, bundle.spec);
    let receipt = oe_kernel::packages::regress(f.root.path(), name).unwrap();
    assert_eq!(receipt["status"], "passed");
    let evidence = oe_core::sha256(oe_core::json::canonical_json(&receipt).unwrap());
    put(
        &f,
        &format!("var/rule-ledger/evidence/{evidence}.json"),
        &receipt,
    );
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    fs::copy(
        original.join("LICENSE"),
        f.temp.path().join("var/inputs/LICENSE"),
    )
    .unwrap();
    let inventory = vec![
        (bundle.spec["path"].as_str().unwrap(), "package_archive"),
        ("var/inputs/LICENSE", "license"),
        ("runtime/semantic-bundles.json", "interface"),
        ("runtime/semantica-source-lock.json", "origin"),
    ];
    let inventory: Vec<_> = inventory
        .iter()
        .map(|(path, role)| {
            json!({"path":path,"role":role,
        "sha256":oe_core::sha256(fs::read(f.temp.path().join(path)).unwrap())})
        })
        .collect();
    let candidate = bundle.spec["sha256"].as_str().unwrap();
    let package_binding = oe_core::sha256(
        oe_core::json::canonical_json(&json!({"package_id":bundle.spec["package_id"],
        "package_version":bundle.spec["package_version"],"archive_sha256":candidate}))
        .unwrap(),
    );
    let states = [
        "proposed",
        "committed",
        "regression_passed",
        "release_complete",
        "promoted",
    ];
    let required: serde_json::Map<String, Value> = states
        .iter()
        .map(|s| (s.to_string(), json!([evidence])))
        .collect();
    let policy = json!({"schema":"oev2.governance-policy/1","ledger":"var/rule-ledger","bundle":name,
        "candidate_sha256":candidate,"package_binding_sha256":package_binding,
        "actor_scopes":{"author":["proposed","committed"],"reviewer":["regression_passed","release_complete"],"owner":["promoted"]},
        "required_evidence":required,"release_artifacts":inventory});
    put(&f, "var/state/governance-authority.json", &policy);
    put(
        &f,
        registry::POLICY,
        &json!({"schema":"oev2.controlled-version-authority/1",
        "bindings":{"scope-rules":{"kind":"semantic_bundle","scope_id":"scope-checks",
            "actor_scopes":{"maintainer":["register","activate","restore"]}}}}),
    );
    let d = ControlledVersionDefinition {
        schema: "oev2.controlled-version-definition/1".into(),
        binding_id: "scope-rules".into(),
        revision: "rules-1".into(),
        kind: "semantic_bundle".into(),
        scope_id: "scope-checks".into(),
        source: artifacts::reference(&f.root, &f.temp.path().join("var/inputs/rule-spec.json"))
            .unwrap(),
        assurance: artifacts::reference(
            &f.root,
            &f.temp.path().join("var/state/governance-authority.json"),
        )
        .unwrap(),
    };
    registry::register(&f.root, &d, "maintainer").unwrap();
    let r = VersionSelectionRequest {
        schema: "oev2.version-selection-request/1".into(),
        actor: "maintainer".into(),
        binding_id: "scope-rules".into(),
        revision: "rules-1".into(),
        reason: "promoted source-bound original package, no new engineering claim".into(),
        expected_head_sha256: None,
    };
    assert!(registry::select(&f.root, &r, false).is_err());
    let mut previous = None;
    let mut from = "candidate";
    for (i, to) in states.iter().enumerate() {
        let actor = if i < 2 {
            "author"
        } else if i < 4 {
            "reviewer"
        } else {
            "owner"
        };
        let event: Event = serde_json::from_value(
            json!({"candidate_sha256":candidate,"previous_sha256":previous,
            "from":from,"to":to,"actor":actor,"authority_scope":[to],"evidence_sha256":[evidence],
            "package_binding_sha256":package_binding,"event_sha256":""}),
        )
        .unwrap();
        let result = governance::transition(&f.root, Path::new("var/rule-ledger"), event).unwrap();
        previous = Some(
            result["event"]["event_sha256"]
                .as_str()
                .unwrap()
                .to_string(),
        );
        from = to;
    }
    registry::select(&f.root, &r, false).unwrap();
    let (selected, _) = registry::rule_bundle(&f.root, "scope-rules").unwrap();
    assert_eq!(selected, name);
    let b: oe_kernel::Binding = serde_json::from_value(f.read("var/inputs/binding.json")).unwrap();
    let bytes = fs::read(f.temp.path().join("var/inputs/evidence.ttl")).unwrap();
    let request = ReviewRequest {
        object: ObjectIdentity {
            object_id: b.object_id.clone(),
            revision: b.object_revision.clone(),
            condition_scope: b.condition_scope.clone(),
            source_sha256: b.source_sha256.clone(),
        },
        evidence_sha256: oe_core::sha256(&bytes),
        required_checks: b.required_checks.iter().cloned().collect(),
    };
    let actual = oe_kernel::port::RustSemanticPort {
        root: f.root.path().into(),
        binding: b,
        bundle: selected,
        queries: vec!["findings-pattern-scope".into()],
        shapes: vec!["shape-pattern-scope".into()],
        evidence: bytes,
    }
    .execute(&request, None)
    .unwrap();
    assert_eq!(actual.execution, "completed");
    assert_eq!(actual.conclusion, "unknown");
    fs::write(
        f.temp.path().join("var/inputs/LICENSE"),
        b"changed licensed inventory",
    )
    .unwrap();
    assert!(registry::rule_bundle(&f.root, "scope-rules").is_err());
}
