//! Characterisation tests for `namespace_manager.rs` (`semantica/ontology/namespace_manager.py`):
//! base URIs, class/property/individual IRIs (speaking and hash-based), concept schemes,
//! namespaces, alignment predicates and IRI validation for many names and configurations.
//! Expected values come from the oracle (`tests/fixtures/oracle.json`).

mod common;

use common::{oracle, outcome, quiet_progress};
use sem_change::namespace_manager::NamespaceManager;
use sem_utils::pycompat::fmt::truthy;
use serde_json::json;

#[test]
fn test_namespace_manager_iris_match_oracle() {
    quiet_progress();
    let names: Vec<&str> = oracle()["namespace"]["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    for case in oracle()["namespace"]["cases"].as_array().unwrap() {
        let config = case["config"].as_object().cloned();
        let nm = NamespaceManager::new(config);
        let context = case["config"].to_string();
        assert_eq!(json!(nm.base_uri), case["base_uri_attr"], "{context}");
        assert_eq!(nm.version, case["version_attr"], "{context}");
        // The port keeps the flag's truthiness (what every use of it tests).
        assert_eq!(
            nm.use_speaking_iris,
            truthy(&case["use_speaking_iris_attr"]),
            "{context}"
        );
        assert_eq!(
            outcome(nm.get_base_uri()),
            case["get_base_uri"],
            "{context}"
        );
        for (i, name) in names.iter().enumerate() {
            assert_eq!(
                outcome(nm.generate_class_iri(name)),
                case["class"][i],
                "{context} class {name:?}"
            );
            assert_eq!(
                outcome(nm.generate_property_iri(name)),
                case["property"][i],
                "{context} property {name:?}"
            );
            assert_eq!(
                outcome(nm.generate_individual_iri(name)),
                case["individual"][i],
                "{context} individual {name:?}"
            );
        }
        let schemes = [
            "My Vocabulary",
            "  --Weird__Name!!  ",
            "",
            "\u{e9}t\u{e9} 2024",
            "A-B-C",
        ];
        for (i, name) in schemes.iter().enumerate() {
            assert_eq!(
                outcome(nm.build_concept_scheme_uri(name)),
                case["scheme"][i],
                "{context} scheme {name:?}"
            );
        }
    }
}

#[test]
fn test_namespace_manager_namespaces_match_oracle() {
    quiet_progress();
    let expected = &oracle()["namespace_misc"];
    let mut nm = NamespaceManager::new(None);
    assert_eq!(json!(nm.get_all_namespaces()), expected["namespaces"]);
    assert_eq!(
        json!([nm.get_namespace("owl"), nm.get_namespace("nope")]),
        expected["get"]
    );
    assert_eq!(json!(nm.get_skos_uri("Concept").unwrap()), expected["skos"]);
    assert_eq!(json!(nm.get_alignment_predicates()), expected["alignment"]);
    // Key order is part of the result (dicts keep insertion order).
    let keys: Vec<String> = nm.get_alignment_predicates().keys().cloned().collect();
    let expected_keys: Vec<String> = expected["alignment"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, expected_keys);

    nm.register_namespace("ex", "https://example.org/");
    nm.register_namespace("owl", "http://other/owl#");
    assert_eq!(json!(nm.get_all_namespaces()), expected["after_register"]);
    let order: Vec<String> = nm.get_all_namespaces().keys().cloned().collect();
    let expected_order: Vec<String> = expected["after_register"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(order, expected_order);
    assert_eq!(
        json!(nm.get_alignment_predicates()),
        expected["alignment_after"]
    );

    nm.namespaces.shift_remove("skos");
    nm.namespaces.shift_remove("owl");
    assert_eq!(
        outcome(nm.get_skos_uri("Concept")),
        expected["skos_missing"]
    );
    assert_eq!(
        json!(nm.get_alignment_predicates()),
        expected["alignment_missing"]
    );

    let kwargs = NamespaceManager::new(
        json!({"base_uri": "a", "version": "9", "extra": 1})
            .as_object()
            .cloned(),
    );
    assert_eq!(json!(kwargs.config), expected["kwargs_config"]);
    assert!(format!("{kwargs:?}").contains("NamespaceManager"));
    assert_eq!(
        NamespaceManager::default().base_uri,
        "https://semantica.dev/ontology/"
    );
}

#[test]
fn test_namespace_manager_validate_iri_matches_oracle() {
    quiet_progress();
    let nm = NamespaceManager::new(None);
    for case in oracle()["validate_iri"].as_array().unwrap() {
        let iri = case["iri"].as_str().unwrap();
        assert_eq!(json!(nm.validate_iri(iri)), case["valid"], "{iri:?}");
    }
}

/// Like the original, construction switches the global progress tracker back on (unless
/// `SEMANTICA_DISABLE_PROGRESS` forbids it).
#[test]
fn test_namespace_manager_enables_progress_tracker() {
    quiet_progress();
    let tracker = sem_utils::get_progress_tracker();
    tracker.set_enabled(false);
    let nm = NamespaceManager::new(None);
    let forced_off = std::env::var("SEMANTICA_DISABLE_PROGRESS")
        .map(|v| {
            matches!(
                v.trim().to_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    assert_eq!(nm.progress_tracker.enabled(), !forced_off);
}
