//! Characterisation of `entity_normalizer` (`EntityNormalizer`, `AliasResolver`,
//! `EntityDisambiguator`, `NameVariantHandler`) against the oracle.

mod common;

use sem_normalize::entity_normalizer::DEFAULT_TITLES;
use sem_normalize::{AliasResolver, EntityDisambiguator, EntityNormalizer, NameVariantHandler};
use serde_json::{Map, Value, json};

use common::{check, kw, obj, oracle, text_opt};

fn cases(key: &str) -> &'static Vec<Value> {
    oracle(key).as_array().expect("a list of cases")
}

fn aliases() -> Map<String, Value> {
    kw(json!({"alias_map": {
        "j. doe": "John Doe",
        "Bill Gates": "William Henry Gates III",
        "ibm": "",
        "acme": "ACME Corp."
    }}))
}

#[test]
fn test_entity_normalizer_matches_oracle() {
    let normalizer = EntityNormalizer::new(aliases());
    for case in cases("normalize_entity") {
        let result = normalizer.normalize_entity(
            text_opt(&case["name"]),
            case["entity_type"].as_str(),
            case["resolve_aliases"].as_bool().unwrap(),
        );
        check(json!({"ok": result}), case);
    }
    for case in cases("entity_resolve_aliases") {
        let result = normalizer.resolve_aliases(case["name"].as_str().unwrap(), &Map::new());
        check(json!({"ok": result}), case);
    }
    for case in cases("entity_disambiguate") {
        let result =
            normalizer.disambiguate_entity(case["name"].as_str().unwrap(), &obj(&case["context"]));
        check(json!({"ok": result}), case);
    }
    for case in cases("link_entities") {
        let entities: Vec<&str> = case["entities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
            .collect();
        let kwargs = obj(&case["kwargs"]);
        let entity_type = kwargs.get("entity_type").and_then(Value::as_str);
        let resolve = kwargs
            .get("resolve_aliases")
            .map(|v| v.as_bool().unwrap())
            .unwrap_or(true);
        let result = normalizer.link_entities(&entities, entity_type, resolve);
        check(json!({"ok": result}), case);
    }
}

#[test]
fn test_alias_resolver_matches_oracle() {
    let resolver = AliasResolver::new(aliases());
    for case in cases("alias_resolve") {
        check(
            json!({"ok": resolver.resolve_aliases(case["name"].as_str().unwrap())}),
            case,
        );
    }
    assert_eq!(
        json!({"ok": AliasResolver::default().resolve_aliases("anything")}),
        *oracle("alias_resolve_empty")
    );
    assert_eq!(
        json!({"ok": resolver.map_variants("Jon", "Person")}),
        *oracle("map_variants")
    );
    let cultural = oracle("handle_cultural_variations").as_array().unwrap();
    assert_eq!(
        json!({"ok": resolver.handle_cultural_variations("Müller", None)}),
        cultural[0]
    );
    assert_eq!(
        json!({"ok": resolver.handle_cultural_variations("Müller", Some("de-DE"))}),
        cultural[1]
    );
}

#[test]
fn test_entity_disambiguator_matches_oracle() {
    let disambiguator = EntityDisambiguator::default();
    for case in cases("disambiguate") {
        let result =
            disambiguator.disambiguate(case["name"].as_str().unwrap(), &obj(&case["context"]));
        check(json!({"ok": result}), case);
    }
    for case in cases("classify_entity_type") {
        let result = disambiguator.classify_entity_type(case["name"].as_str().unwrap());
        check(json!({"ok": result}), case);
    }
    assert_eq!(
        json!({"ok": disambiguator.calculate_confidence(&["a", "b", "a"])}),
        *oracle("calculate_confidence")
    );
}

#[test]
fn test_name_variant_handler_matches_oracle() {
    let handler = NameVariantHandler::default();
    assert_eq!(handler.titles, DEFAULT_TITLES.to_vec());
    for case in cases("normalize_name_format") {
        let result = handler
            .normalize_name_format(case["name"].as_str().unwrap(), case["format_type"].as_str());
        check(json!({"ok": result}), case);
    }
    for case in cases("process_variants") {
        check(
            json!({"ok": handler.process_variants(case["name"].as_str().unwrap())}),
            case,
        );
    }
    for case in cases("handle_titles_and_honorifics") {
        let result = handler.handle_titles_and_honorifics(case["name"].as_str().unwrap());
        check(json!({"ok": result}), case);
    }
    let custom = NameVariantHandler::new(kw(json!({"titles": ["Captain", "Dr."]})));
    for case in cases("custom_titles") {
        let result = custom.normalize_name_format(case["name"].as_str().unwrap(), Some("standard"));
        check(json!({"ok": result}), case);
    }
    // Replacing the titles keeps the compiled patterns in step.
    let mut replaced = NameVariantHandler::default();
    replaced.set_titles(&["Captain", "Dr."]);
    assert_eq!(
        replaced.normalize_name_format("captain jack sparrow", None),
        "Jack Sparrow"
    );
    replaced.titles = vec!["Mr.".to_string()];
    assert_eq!(replaced.normalize_name_format("Mr. Bean", None), "Bean");
}
