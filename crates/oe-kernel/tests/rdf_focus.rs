use oe_contracts::{ObjectIdentity, ReviewRequest};
use std::{fs, path::PathBuf};
#[test]
#[ignore = "explicit original Project/native jobs and retained semantic bundle required"]
fn original_plain_object_identity_reports_exact_review_failure() {
    let root = PathBuf::from(std::env::var("OE_RDF_FOCUS_ROOT").unwrap());
    let d = root.join("var/full-review-01");
    let binding: oe_kernel::Binding =
        serde_json::from_slice(&fs::read(d.join("binding-original.json")).unwrap()).unwrap();
    let evidence = fs::read(d.join("evidence.ttl")).unwrap();
    let input = ReviewRequest {
        object: ObjectIdentity {
            object_id: binding.object_id.clone(),
            revision: binding.object_revision.clone(),
            condition_scope: binding.condition_scope.clone(),
            source_sha256: binding.source_sha256.clone(),
        },
        evidence_sha256: oe_core::sha256(&evidence),
        required_checks: binding.required_checks.iter().cloned().collect(),
    };
    let port = oe_kernel::port::RustSemanticPort {
        root,
        binding,
        bundle: "engineering-evidence-methods".into(),
        queries: vec![],
        shapes: vec!["shape-review-coverage".into()],
        evidence,
    };
    let error = port.execute(&input, None).unwrap_err().to_string();
    println!("actual_error={error}");
    assert!(error.contains("IRI"), "{error}");
}

#[test]
#[ignore = "explicit original Project/native jobs and retained semantic bundle required"]
fn mapped_full_current_review_recomputes_project_and_rejects_false_identity() {
    let root = PathBuf::from(std::env::var("OE_RDF_FOCUS_ROOT").unwrap());
    let d = root.join("var/full-review-01");
    let original = fs::read(d.join("binding-original.json")).unwrap();
    let legacy: oe_kernel::Binding = serde_json::from_slice(&original).unwrap();
    assert!(
        serde_json::to_value(&legacy)
            .unwrap()
            .get("rdf_focus")
            .is_none()
    );
    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["rdf_focus"] = serde_json::json!({"schema":"oev2.rdf-object-focus/1","object_id":legacy.object_id,"focus_iri":"urn:oev2:development:stepped-shaft-A-drawing"});
    let binding: oe_kernel::Binding = serde_json::from_value(value.clone()).unwrap();
    binding.validate().unwrap();
    assert_ne!(legacy.digest().unwrap(), binding.digest().unwrap());
    let evidence = fs::read(d.join("evidence.ttl")).unwrap();
    let project: oe_kernel::project::Project =
        serde_json::from_slice(&fs::read(d.join("project.json")).unwrap()).unwrap();
    let input = ReviewRequest {
        object: ObjectIdentity {
            object_id: binding.object_id.clone(),
            revision: binding.object_revision.clone(),
            condition_scope: binding.condition_scope.clone(),
            source_sha256: binding.source_sha256.clone(),
        },
        evidence_sha256: oe_core::sha256(&evidence),
        required_checks: binding.required_checks.iter().cloned().collect(),
    };
    let execute =
        |binding: oe_kernel::Binding, data: Vec<u8>| -> anyhow::Result<oe_kernel::Receipt> {
            let mut input = input.clone();
            input.evidence_sha256 = oe_core::sha256(&data);
            let port = oe_kernel::port::RustSemanticPort {
                root: root.clone(),
                binding,
                bundle: "engineering-evidence-methods".into(),
                queries: vec![],
                shapes: vec!["shape-review-coverage".into()],
                evidence: data.clone(),
            };
            let semantic = port.execute(&input, None)?;
            oe_kernel::project::review_current(
                &oe_core::SkillRoot::new(root.clone())?,
                &project,
                &data,
                semantic,
            )
        };
    let receipt = execute(binding.clone(), evidence.clone()).unwrap();
    assert_eq!(receipt.conclusion, "pass", "{receipt:?}");
    assert!(
        receipt
            .coverage
            .contains("engineering_native_source:sheet-persistence")
    );
    assert!(receipt.coverage.contains("current_condition_model"));
    let mut wrong = binding.clone();
    wrong.rdf_focus.as_mut().unwrap().object_id = "other".into();
    assert!(execute(wrong, evidence.clone()).is_err());
    let mut wrong = binding.clone();
    wrong.rdf_focus.as_mut().unwrap().focus_iri = "urn:wrong:focus".into();
    assert!(execute(wrong, evidence.clone()).is_err());
    let mut duplicate = evidence.clone();
    duplicate.extend_from_slice(b"\n<urn:another:focus> <urn:oev2:project-context:objectId> \"stepped-shaft-A-drawing\" .\n");
    assert!(execute(binding.clone(), duplicate).is_err());
    let old = String::from_utf8(evidence.clone()).unwrap();
    let missing = old.replace("pc:objectId \"stepped-shaft-A-drawing\" ;", "");
    // Remove only the object identity assertion; preserve the condition and all other triples.
    assert!(execute(binding.clone(), missing.into_bytes()).is_err());
    let context = project.conditions[&input.object.condition_scope].as_bytes();
    let hash = oe_core::sha256(context);
    let changed = old.replace(&hash, &"0".repeat(64)).into_bytes();
    let result = execute(binding, changed).unwrap();
    assert_ne!(result.conclusion, "pass");
    println!(
        "full_current_review=pass; false object, wrong focus, duplicate/missing mapping and changed condition refused; original object unchanged"
    );
}
