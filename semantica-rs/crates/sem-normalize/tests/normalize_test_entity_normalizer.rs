//! Port of `tests/normalize/test_entity_normalizer.py` (`TestEntityNormalizer`,
//! `TestNameVariantHandler`).
#![allow(non_snake_case)]

use sem_normalize::{EntityNormalizer, NameVariantHandler};
use serde_json::{Map, Value, json};

fn normalizer() -> EntityNormalizer {
    let config = json!({
        "alias_map": {
            "j. doe": "John Doe",
            "bill gates": "William Henry Gates III"
        }
    });
    EntityNormalizer::new(config.as_object().unwrap().clone())
}

#[test]
fn test_entity_normalizer__test_normalize_entity_basic() {
    assert_eq!(
        normalizer().normalize_entity("  john doe  ", Some("Person"), true),
        "John Doe"
    );
}

#[test]
fn test_entity_normalizer__test_resolve_aliases() {
    let normalizer = normalizer();
    assert_eq!(
        normalizer.resolve_aliases("J. Doe", &Map::new()).as_deref(),
        Some("John Doe")
    );
    assert_eq!(
        normalizer
            .resolve_aliases("Bill Gates", &Map::new())
            .as_deref(),
        Some("William Henry Gates III")
    );
    // Unmapped should return None
    assert_eq!(
        normalizer.resolve_aliases("Unknown Person", &Map::new()),
        None
    );
}

#[test]
fn test_entity_normalizer__test_disambiguate_entity() {
    let context = json!({"context": "tech"});
    let result = normalizer().disambiguate_entity("Apple", context.as_object().unwrap());
    assert_eq!(result["entity_name"], json!("Apple"));
    assert_eq!(result["confidence"], json!(0.8));
}

#[test]
fn test_entity_normalizer__test_link_entities() {
    let linked = normalizer().link_entities(&["J. Doe", "Bill Gates"], Some("Person"), true);
    assert_eq!(linked["J. Doe"], "John Doe");
    // Standard normalization title-cases the string, so III becomes Iii
    assert_eq!(linked["Bill Gates"], "William Henry Gates Iii");
}

#[test]
fn test_name_variant_handler__test_normalize_name_format() {
    let handler = NameVariantHandler::default();
    assert_eq!(
        handler.normalize_name_format("Dr. John Doe", Some("standard")),
        "John Doe"
    );
    assert_eq!(
        handler.normalize_name_format("MR. JOHN DOE", Some("lower")),
        "john doe"
    );
}

#[test]
fn test_name_variant_handler__test_handle_titles() {
    let handler = NameVariantHandler::default();
    let result = handler.handle_titles_and_honorifics("Dr. House");
    assert_eq!(result["name"], json!("House"));
    assert_eq!(result["title"], Value::from("Dr."));
}
