use oe_kernel::{Binding, CheckResult};
use serde_json::json;

fn binding(checks: &[&str]) -> Binding {
    Binding {
        project_context: None,
        rdf_focus: None,
        project_id: "test-project".into(),
        object_id: "bracket".into(),
        object_revision: "A".into(),
        package_id: "test-package".into(),
        package_version: "1".into(),
        package_sha256: "a".repeat(64),
        source_sha256: "b".repeat(64),
        condition_scope: "room-temperature".into(),
        required_checks: checks.iter().map(|s| (*s).into()).collect(),
        authority_id: "test-reviewer".into(),
    }
}
fn check(id: &str, execution: &str, conclusion: &str) -> CheckResult {
    CheckResult {
        id: id.into(),
        execution: execution.into(),
        conclusion: conclusion.into(),
        coverage: vec![id.into()],
        failures: Vec::new(),
        result: json!({"fixture":"state aggregation only"}),
    }
}

#[test]
fn unknown_partial_and_missing_coverage_never_pass() {
    for checks in [
        vec![],
        vec![check("one", "completed", "pass")],
        vec![
            check("one", "completed", "unknown"),
            check("two", "completed", "pass"),
        ],
        vec![
            check("one", "completed", "pass"),
            check("two", "failed", "unknown"),
        ],
        vec![
            check("one", "unsupported", "unknown"),
            check("two", "not_run", "unknown"),
        ],
    ] {
        let receipt = oe_kernel::evaluate(binding(&["one", "two"]), b"evidence", checks).unwrap();
        assert_eq!(receipt.conclusion, "unknown");
    }
    let mut failed = check("one", "completed", "pass");
    failed.failures.push("tool_failed".into());
    assert!(oe_kernel::evaluate(binding(&["one"]), b"evidence", vec![failed]).is_err());
    assert!(
        oe_kernel::evaluate(
            binding(&["one"]),
            b"evidence",
            vec![check("one", "failed", "pass")]
        )
        .is_err()
    );
}

#[test]
fn shape_messages_bounds_and_unsupported_paths_are_preserved() {
    let shapes=br#"@prefix sh:<http://www.w3.org/ns/shacl#>. @prefix xsd:<http://www.w3.org/2001/XMLSchema#>. <urn:Shape> a sh:NodeShape;sh:targetNode <urn:part>;sh:property [sh:path <urn:count>;sh:datatype xsd:nonNegativeInteger;sh:minCount 1;sh:message "count.must-be-nonnegative"]."#;
    let positive = oe_kernel::shacl::validate(
        br#"<urn:part> <urn:count> "3"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger>."#,
        shapes,
    )
    .unwrap();
    assert_eq!(positive["conforms"], true);
    let negative = oe_kernel::shacl::validate(
        br#"<urn:part> <urn:count> "-1"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger>."#,
        shapes,
    )
    .unwrap();
    assert_eq!(negative["conforms"], false);
    assert_eq!(
        negative["findings"][0]["message"],
        "count.must-be-nonnegative"
    );
    // Historical profile oev2.shacl-v1-corpus/2 refused this exact inverse
    // input. Profile /3 expands capability without treating absent predecessors
    // as present: successful execution must still find minCount violated.
    let formerly_unsupported=br#"@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:Shape> sh:targetNode <urn:part>;sh:property [sh:path [sh:inversePath <urn:count>];sh:minCount 1]."#;
    let expanded = oe_kernel::shacl::validate(b"", formerly_unsupported).unwrap();
    assert_eq!(expanded["conforms"], false);
    assert_eq!(expanded["violation_count"], 1);
    assert_eq!(expanded["findings"][0]["component"], "minCount");
    let unsupported=br#"@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:Shape> sh:targetNode <urn:part>;sh:rule [a sh:SPARQLRule;sh:construct \"CONSTRUCT { $this <urn:count> 1 } WHERE {}\"]."#;
    assert!(oe_kernel::shacl::validate(b"", unsupported).is_err());
    let malformed_path=br#"@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:Shape> sh:targetNode <urn:part>;sh:property [sh:path [sh:inversePath \"not-an-IRI-or-path\"];sh:minCount 1]."#;
    assert!(oe_kernel::shacl::validate(b"", malformed_path).is_err());
}

#[test]
fn positive_rules_preserve_derivation_and_reject_negation() {
    let result = oe_kernel::rules::forward_chain(
        "Measured(x)\nInScope(x)\n",
        "IF Measured(?x) AND InScope(?x) THEN Supported(?x)\nIF Supported(?x) THEN Reviewed(?x)\n",
    )
    .unwrap();
    assert_eq!(
        result["conclusions"],
        json!(["Reviewed(x)", "Supported(x)"])
    );
    assert!(
        result["provenance"]["Supported(x)"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "Measured(x)")
    );
    assert!(
        oe_kernel::rules::forward_chain("Measured(x)", "IF NOT Measured(?x) THEN Unsupported(?x)")
            .is_err()
    );
    assert!(
        oe_kernel::rules::forward_chain("Measured(x)", "IF Measured(?x) THEN Reviewed(?y)")
            .is_err()
    );
}

#[test]
fn changed_binding_requires_fresh_review_without_relabelling_history() {
    let old = oe_kernel::evaluate(
        binding(&["one"]),
        b"original",
        vec![check("one", "completed", "pass")],
    )
    .unwrap();
    let original = serde_json::to_vec(&old).unwrap();
    let mut changed = old.binding.clone();
    changed.object_revision = "B".into();
    changed.source_sha256 = "c".repeat(64);
    changed.condition_scope = "60C".into();
    let new = oe_kernel::review(
        changed,
        b"new evidence",
        vec![check("one", "completed", "unknown")],
        Some(&old),
    )
    .unwrap();
    assert_eq!(new.conclusion, "unknown");
    assert_ne!(new.binding_sha256, old.binding_sha256);
    assert!(
        new.adoption_conditions
            .iter()
            .any(|s| s.contains("fresh_review"))
    );
    assert_eq!(serde_json::to_vec(&old).unwrap(), original);
    let mut forged = old.clone();
    forged.binding.object_revision = "forged".into();
    assert!(oe_kernel::review(forged.binding.clone(), b"x", Vec::new(), Some(&forged)).is_err());
}

#[test]
fn empty_project_evidence_does_not_establish_a_pass() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let checks = oe_kernel::packages::project_checks(
        root,
        "manufacturing-process-cost",
        b"",
        &[],
        &["closure-identity".into()],
    )
    .unwrap();
    assert_eq!(checks[0].conclusion, "unknown");
}
