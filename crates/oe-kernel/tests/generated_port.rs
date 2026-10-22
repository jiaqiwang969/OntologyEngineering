use oe_contracts::{ObjectIdentity, ReviewRequest, SemanticReviewPort};
use oe_kernel::{Binding, port::RustSemanticPort};
use std::path::Path;

#[test]
fn unrelated_valid_evidence_cannot_pass_a_different_bound_object() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let private = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/generated-port-tests/{}",
            std::process::id()
        )))
        .unwrap();
    for file in [
        "review-current-support-identity_current.json",
        "review-sources.json",
    ] {
        std::fs::copy(
            root.join("examples/judgment_intake").join(file),
            private.join(file),
        )
        .unwrap();
    }
    let record = private.join("review-current-support-identity_current.json");
    let projection =
        oe_semantic::review::project_record(root, &record, &private, "engineering-judgment-intake")
            .unwrap();
    let package = oe_semantic::load_bundle(root, "engineering-judgment-intake").unwrap();
    let sha = oe_core::sha256(&projection.rdf);
    let mut binding = Binding {
        project_context: None,
        rdf_focus: None,
        project_id: "synthetic-project".into(),
        object_id: projection.audit["focus"].as_str().unwrap().into(),
        object_revision: "R2".into(),
        package_id: package.spec["package_id"].as_str().unwrap().into(),
        package_version: package.spec["package_version"].as_str().unwrap().into(),
        package_sha256: package.spec["sha256"].as_str().unwrap().into(),
        source_sha256: sha.clone(),
        condition_scope: projection.audit["scope"].as_str().unwrap().into(),
        required_checks: [
            "findings-pattern-identity".into(),
            "shape-pattern-identity".into(),
        ]
        .into_iter()
        .collect(),
        authority_id: "synthetic-reviewer".into(),
    };
    let request = |binding: &Binding| ReviewRequest {
        object: ObjectIdentity {
            object_id: binding.object_id.clone(),
            revision: binding.object_revision.clone(),
            condition_scope: binding.condition_scope.clone(),
            source_sha256: binding.source_sha256.clone(),
        },
        evidence_sha256: sha.clone(),
        required_checks: binding.required_checks.iter().cloned().collect(),
    };
    let mut port = RustSemanticPort {
        root: root.to_owned(),
        binding: binding.clone(),
        bundle: "engineering-judgment-intake".into(),
        queries: vec!["findings-pattern-identity".into()],
        shapes: vec!["shape-pattern-identity".into()],
        evidence: projection.rdf,
    };
    assert_eq!(port.review(&request(&binding)).unwrap().conclusion, "pass");
    for field in ["revision", "condition", "source"] {
        let mut changed = binding.clone();
        match field {
            "revision" => changed.object_revision = "R3".into(),
            "condition" => changed.condition_scope = "assembly-A-R2-at-60-degC".into(),
            _ => changed.source_sha256 = "f".repeat(64),
        };
        port.binding = changed.clone();
        assert_eq!(
            port.review(&request(&changed)).unwrap().conclusion,
            "unknown",
            "changed {field} cannot reuse the original record's pass"
        );
    }
    binding.object_id = "urn:synthetic:absent-object".into();
    port.binding = binding.clone();
    assert_eq!(
        port.review(&request(&binding)).unwrap().conclusion,
        "unknown",
        "a valid record about another object is not evidence for this object"
    );
}
