use std::path::Path;

fn baseline() -> oe_design::Design {
    oe_design::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("specs/engineering.toml"),
    )
    .unwrap()
}

#[test]
fn contract_groups_preserve_defaults_and_reject_reverse_dependencies() {
    let d = baseline();
    assert_eq!(
        d.types
            .iter()
            .find(|t| t.name == "ReviewRequest")
            .unwrap()
            .contract_group,
        "engineering"
    );
    assert_eq!(
        d.types
            .iter()
            .find(|t| t.name == "SemanticQueryRequest")
            .unwrap()
            .contract_group,
        "semantic"
    );
    let port = d
        .interfaces
        .iter()
        .find(|p| p.name == "SemanticQueryPort")
        .unwrap();
    assert_eq!(d.interface_group(port).unwrap(), "semantic");
    assert!(
        d.rdf()
            .unwrap()
            .contains("<urn:oev2:design:contractGroup> \"semantic\"")
    );

    let mut changed = d.clone();
    changed
        .types
        .iter_mut()
        .find(|t| t.name == "SemanticQueryResult")
        .unwrap()
        .fields[0]
        .rust_type = "Vec<ReviewRequest>".into();
    assert!(
        changed
            .validate_contract_groups()
            .unwrap_err()
            .to_string()
            .contains("semantic_contract_depends_on_engineering")
    );
    let mut changed = d.clone();
    changed
        .types
        .iter_mut()
        .find(|t| t.name == "SemanticQueryRequest")
        .unwrap()
        .contract_group = "unregistered".into();
    assert!(changed.validate_contract_groups().is_err());
    let mut changed = d.clone();
    changed
        .interfaces
        .iter_mut()
        .find(|p| p.name == "SemanticQueryPort")
        .unwrap()
        .output = "ReviewResult".into();
    assert!(
        changed
            .validate_contract_groups()
            .unwrap_err()
            .to_string()
            .contains("interface_contract_group_mixed")
    );
    let mut changed = d.clone();
    changed
        .modules
        .iter_mut()
        .find(|m| m.id == "semantic_runtime")
        .unwrap()
        .depends_on
        .push("kernel".into());
    assert!(changed.validate_contract_groups().is_err());

    // Semantic contract names have no required prefix. The declared group owns them.
    let mut changed = d.clone();
    let mut neutral = changed
        .types
        .iter()
        .find(|t| t.name == "Quantity")
        .unwrap()
        .clone();
    neutral.name = "NeutralAmount".into();
    neutral.contract_group = "semantic".into();
    changed.types.push(neutral);
    changed
        .types
        .iter_mut()
        .find(|t| t.name == "ReviewRequest")
        .unwrap()
        .fields
        .iter_mut()
        .find(|field| field.name == "object")
        .unwrap()
        .rust_type = "NeutralAmount".into();
    assert!(
        changed.validate_contract_groups().is_ok(),
        "engineering may consume one canonical semantic type"
    );
    assert!(
        changed
            .rdf()
            .unwrap()
            .contains("<urn:oev2:design:referencedType> <urn:oev2:design:type_NeutralAmount>")
    );
}

#[test]
fn dependency_cycles_and_raw_payload_type_confusion_are_rejected() {
    let d = baseline();
    let mut changed = d.clone();
    changed
        .modules
        .iter_mut()
        .find(|m| m.id == "semantic_contracts")
        .unwrap()
        .depends_on
        .push("semantic_runtime".into());
    assert!(
        changed
            .validate_contract_groups()
            .unwrap_err()
            .to_string()
            .contains("module_dependency_cycle")
    );
    let mut changed = d;
    changed
        .types
        .iter_mut()
        .find(|t| t.name == "SemanticQueryRequest")
        .unwrap()
        .fields
        .iter_mut()
        .find(|f| f.name == "data")
        .unwrap()
        .rust_type = "serde_json::Value".into();
    assert!(
        changed
            .validate_contract_groups()
            .unwrap_err()
            .to_string()
            .contains("raw_utf8_contract_requires_string")
    );
}

#[test]
fn interface_owner_requires_real_production_trait_implementation() {
    let d = baseline();
    let target = d
        .interfaces
        .iter()
        .find(|i| i.name == "SemanticQueryPort")
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let path = root.join(target.rust_path.as_ref().unwrap());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for (source, accepted) in [
        (
            "// impl SemanticQueryPort for RustSemanticRuntime {}",
            false,
        ),
        (
            "#[cfg(test)] mod fixture { impl SemanticQueryPort for RustSemanticRuntime {} }",
            false,
        ),
        (
            "impl semantica_contracts::SemanticQueryPort for RustSemanticRuntime {}",
            true,
        ),
    ] {
        std::fs::write(&path, source).unwrap();
        let report = oe_design::inspection::inspect(&root, &d).unwrap();
        let missing = report["findings"].as_array().unwrap().iter().any(|f| {
            f["code"] == "interface_implementation_missing"
                && f["subject"] == "interface_SemanticQueryPort"
        });
        assert_ne!(
            missing, accepted,
            "only real production impl may satisfy mapping"
        );
    }
    let mut changed = d;
    changed
        .interfaces
        .iter_mut()
        .find(|i| i.name == "SemanticQueryPort")
        .unwrap()
        .rust_path = Some("crates/oe-kernel/src/port.rs".into());
    assert!(
        changed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("interface_implementation_owner_invalid")
    );
}
