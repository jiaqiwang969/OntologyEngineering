use oe_design::{changes, load};
#[test]
fn complete_model_records_feed_ggen_and_actual_change_impact() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let before = load(&root.join("specs/engineering.toml")).unwrap();
    assert_eq!(before.semantic_models.len(), 106);
    assert_eq!(
        before
            .semantic_models
            .iter()
            .map(|m| m.fields.len())
            .sum::<usize>(),
        529
    );
    let rdf = before.rdf().unwrap();
    assert!(rdf.contains("SemanticSnapshot"));
    assert!(rdf.contains("SemanticModelCatalog"));
    let mut after = before.clone();
    let model = &mut after.semantic_models[0];
    let changed = format!("semantic_model_{}", model.rust_name);
    model
        .fields
        .iter_mut()
        .find(|f| !f.required)
        .unwrap()
        .default_json = r#"{"kind":"value","value":"independent candidate change"}"#.into();
    let delta = changes(&before, &after).unwrap();
    assert!(delta.contains(&changed));
    let impact = after.impact(&delta);
    for required in ["semantic_schema", "semantic_typed", "semantic_contracts"] {
        assert!(impact.contains(required), "{required}");
    }
    assert!(impact.contains("semantic_schema_original_parity"));
}
#[test]
fn model_literals_and_strong_types_cannot_inject_code_or_unknown_owner_paths() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let before = load(&root.join("specs/engineering.toml")).unwrap();
    for mutation in 0..4 {
        let mut candidate = before.clone();
        let m = &mut candidate.semantic_models[0];
        match mutation {
            0 => m.id = "forged\"; arbitrary_code()".into(),
            1 => m.meaning = "description\npub fn forged() {}".into(),
            2 => m.fields[0].rust_type = "oe_kernel::Project".into(),
            _ => m.fields[0].rust_type = "*const String".into(),
        }
        assert!(candidate.validate().is_err());
    }
}

#[test]
fn literal_model_defaults_keep_json_object_identity_before_ggen() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut design = load(&root.join("specs/engineering.toml")).unwrap();
    let model = design
        .semantic_models
        .iter_mut()
        .find(|m| m.name == "NodeResponse")
        .unwrap();
    let field = model
        .fields
        .iter_mut()
        .find(|f| f.name == "properties")
        .unwrap();
    field.default_json = r#"{"kind":"value","value":{"$serde_json::private::Number":"42"}}"#.into();
    design.validate().unwrap();
    let catalog = oe_design::schema_design::catalog(&design.semantic_models).unwrap();
    let model = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == "NodeResponse")
        .unwrap();
    let field = model["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "properties")
        .unwrap();
    assert_eq!(
        field["default"]["value"],
        serde_json::json!({"$serde_json::private::Number":"42"})
    );
}
