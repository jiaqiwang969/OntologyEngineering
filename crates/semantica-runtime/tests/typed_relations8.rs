use semantica_contracts::*;
use semantica_runtime::typed_relations8::{
    RELATIONS_MODEL8, TEMPORAL_MODEL8, convert_relations8, prepare_relations8,
};
use serde_json::{Value, json};
fn gold() -> Value {
    serde_json::from_str(include_str!("fixtures/typed_relations8_original.json")).unwrap()
}
fn input(temporal: bool) -> SemanticTypedRelationRequest8 {
    let x = gold();
    SemanticTypedRelationRequest8 {
        schema: "semantica.typed-relation-request/8".into(),
        text: x["text"].as_str().unwrap().into(),
        entities: x["entities"].clone(),
        relation_types: vec![],
        temporal,
        max_retries: 3,
        generation: SemanticProviderRequest {
            schema: "semantica.provider-request/1".into(),
            provider: "openai".into(),
            interface_profile: "semantic_extract".into(),
            operation: "generate".into(),
            model: Some("fixture-only".into()),
            custom_endpoint: false,
            prompt: String::new(),
            options: json!({"max_tokens":1024}),
        },
    }
}
#[test]
fn original_ordinary_prompt_and_model_are_exact() {
    let (request, id) = prepare_relations8(&input(false)).unwrap();
    assert_eq!(request.prompt, gold()["ordinary_prompt"]);
    assert_eq!(id, RELATIONS_MODEL8);
    assert_eq!(
        semantica_runtime::sha256(request.prompt),
        gold()["ordinary_prompt_sha256"]
    );
}
#[test]
fn original_temporal_prompt_and_model_are_exact() {
    let (request, id) = prepare_relations8(&input(true)).unwrap();
    assert_eq!(request.prompt, gold()["temporal_prompt"]);
    assert_eq!(id, TEMPORAL_MODEL8);
    assert_eq!(
        semantica_runtime::sha256(request.prompt),
        gold()["temporal_prompt_sha256"]
    );
}
#[test]
fn original_finite_relation_conversion_and_temporal_fields() {
    let x = gold();
    let value = convert_relations8(&input(false), &x["typed_original_output"]).unwrap();
    assert_eq!(value[0], x["expected_relation"]);
    let value = convert_relations8(&input(true), &x["temporal_original_output"]).unwrap();
    assert_eq!(value[0]["metadata"], x["temporal_expected_metadata"]);
}
#[test]
fn missing_or_fuzzy_entities_does_not_invent_source_offsets() {
    let mut x = gold()["typed_original_output"].clone();
    x["relations"][0]["subject"] = json!("Apple Inc.");
    let error = convert_relations8(&input(false), &x).unwrap_err();
    assert_eq!(error.code, "typed_relations8_hybrid_match_unmigrated");
}
#[test]
fn original_content_and_prompt_cannot_be_silently_replaced() {
    let mut x = input(false);
    x.generation.prompt = "caller fabricated prompt".into();
    assert!(prepare_relations8(&x).is_err());
    x = input(false);
    x.entities = Value::Array(vec![]);
    assert!(prepare_relations8(&x).is_err());
    x = input(false);
    x.text = "x".repeat(64001);
    assert!(prepare_relations8(&x).is_err());
}

#[test]
fn original_preferred_instruction_is_exact() {
    let x: Value =
        serde_json::from_str(include_str!("fixtures/typed_relations8_preferred.json")).unwrap();
    let mut i = input(false);
    i.relation_types = vec!["acquired".into(), "owns".into()];
    let (r, _) = prepare_relations8(&i).unwrap();
    assert_eq!(r.prompt, x["prompt"]);
    assert_eq!(semantica_runtime::sha256(r.prompt), x["prompt_sha256"]);
}
