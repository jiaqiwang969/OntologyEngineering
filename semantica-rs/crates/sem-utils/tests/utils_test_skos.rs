//! Port of `tests/utils/test_skos.py` (`TestIsSkosHierarchyEdge`, `TestValidateSkosHierarchy`).
#![allow(non_snake_case)]

use sem_utils::skos::{is_skos_hierarchy_edge, validate_skos_hierarchy};
use serde_json::{Value, json};

fn assert_cycle(result: Result<(), sem_utils::SemanticaError>) {
    // assertRaisesRegex(ValueError, "cycle")
    let err = result.expect_err("expected a ValueError");
    assert!(err.is_value_error(), "{err:?}");
    assert!(err.to_string().contains("cycle"), "{err}");
}

#[test]
fn test_is_skos_hierarchy_edge__test_recognizes_broader_and_narrower() {
    assert!(is_skos_hierarchy_edge(
        &json!({"source": "A", "target": "B", "type": "skos:broader"})
    ));
    assert!(is_skos_hierarchy_edge(
        &json!({"source": "A", "target": "B", "type": "skos:narrower"})
    ));
}

#[test]
fn test_is_skos_hierarchy_edge__test_ignores_other_edge_types() {
    assert!(!is_skos_hierarchy_edge(
        &json!({"source": "A", "target": "B", "type": "rdfs:subClassOf"})
    ));
}

#[test]
fn test_is_skos_hierarchy_edge__test_ignores_non_mapping() {
    assert!(!is_skos_hierarchy_edge(&json!("not-a-dict")));
}

#[test]
fn test_validate_skos_hierarchy__test_accepts_acyclic_chain() {
    validate_skos_hierarchy(
        &[
            json!({"source": "A", "target": "B", "type": "skos:broader"}),
            json!({"source": "B", "target": "C", "type": "skos:broader"}),
        ],
        None,
    )
    .unwrap();
}

#[test]
fn test_validate_skos_hierarchy__test_rejects_self_loop() {
    assert_cycle(validate_skos_hierarchy(
        &[json!({"source": "A", "target": "A", "type": "skos:broader"})],
        None,
    ));
}

#[test]
fn test_validate_skos_hierarchy__test_rejects_direct_two_node_cycle() {
    assert_cycle(validate_skos_hierarchy(
        &[
            json!({"source": "A", "target": "B", "type": "skos:broader"}),
            json!({"source": "B", "target": "A", "type": "skos:broader"}),
        ],
        None,
    ));
}

#[test]
fn test_validate_skos_hierarchy__test_rejects_cycle_spanning_existing_and_new_edges() {
    let existing: Vec<Value> = vec![
        json!({"source": "A", "target": "B", "type": "skos:broader"}),
        json!({"source": "B", "target": "C", "type": "skos:broader"}),
    ];
    assert_cycle(validate_skos_hierarchy(
        &[json!({"source": "C", "target": "A", "type": "skos:broader"})],
        Some(&existing),
    ));
}

#[test]
fn test_validate_skos_hierarchy__test_preexisting_unrelated_cycle_does_not_block_new_write() {
    // A cycle already persisted elsewhere in the graph must not poison unrelated writes.
    let existing: Vec<Value> = vec![
        json!({"source": "X", "target": "Y", "type": "skos:broader"}),
        json!({"source": "Y", "target": "X", "type": "skos:broader"}),
    ];
    validate_skos_hierarchy(
        &[json!({"source": "C", "target": "D", "type": "skos:broader"})],
        Some(&existing),
    )
    .unwrap();
}

#[test]
fn test_validate_skos_hierarchy__test_none_and_blank_endpoints_are_ignored() {
    validate_skos_hierarchy(
        &[
            json!({"source": null, "target": "B", "type": "skos:broader"}),
            json!({"source": "  ", "target": "B", "type": "skos:broader"}),
        ],
        None,
    )
    .unwrap();
}
