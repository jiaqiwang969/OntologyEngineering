use semantica_runtime::{context::load_context_graph, dataset::DatasetSession};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[test]
fn original_constructor_admission_and_error_order_match_42_cases() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_initialization.json")).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 42);
    for row in rows {
        let mut session = DatasetSession::new();
        load_context_graph(&mut session, &row["graph"]).unwrap();
        let before = session.snapshot().unwrap();
        let result = session.prepare_explorer_index(Instant::now() + Duration::from_secs(5));
        assert_eq!(
            result.is_ok(),
            row["result"]["admitted"].as_bool().unwrap(),
            "{}: {result:?}",
            row["id"]
        );
        if let Err(error) = result {
            let expected = match row["result"]["error_type"].as_str().unwrap() {
                "OverflowError" => "explorer_search_integer_float_overflow",
                "TypeError" => "explorer_search_tags_not_iterable",
                other => panic!("unexpected original exception {other}"),
            };
            assert!(
                error.to_string().contains(expected),
                "{}: {error}",
                row["id"]
            );
        }
        assert_eq!(session.snapshot().unwrap(), before, "{}", row["id"]);
    }
}

#[test]
fn initialization_preserves_source_and_rechecks_changed_graph_and_deadline() {
    let mut session = DatasetSession::new();
    load_context_graph(
        &mut session,
        &json!({"nodes":[{"id":"n","type":"Part","properties":{"tags":["guide"]}}],"edges":[]}),
    )
    .unwrap();
    let before = session.snapshot().unwrap();
    session
        .prepare_explorer_index(Instant::now() + Duration::from_secs(5))
        .unwrap();
    assert_eq!(session.snapshot().unwrap(), before);
    assert!(
        session
            .prepare_explorer_index(Instant::now() - Duration::from_secs(1))
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    assert_eq!(session.snapshot().unwrap(), before);
    let mut replacement = DatasetSession::new();
    load_context_graph(
        &mut replacement,
        &json!({"nodes":[{"id":"bad","type":"Part","properties":{"tags":true}}],"edges":[]}),
    )
    .unwrap();
    session
        .replace_named_graph("urn:semantica:context:records/1", replacement.dataset())
        .unwrap();
    let changed = session.snapshot().unwrap();
    assert_ne!(before, changed);
    assert!(
        session
            .prepare_explorer_index(Instant::now() + Duration::from_secs(5))
            .unwrap_err()
            .to_string()
            .contains("tags_not_iterable")
    );
    assert_eq!(session.snapshot().unwrap(), changed);
}
