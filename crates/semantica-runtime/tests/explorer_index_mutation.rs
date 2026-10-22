use semantica_runtime::explorer_search_index::GraphSearchIndex;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn original_documents(index: &GraphSearchIndex) -> Value {
    let mut data = serde_json::to_value(index).unwrap()["documents"].clone();
    for doc in data.as_object_mut().unwrap().values_mut() {
        let doc = doc.as_object_mut().unwrap();
        for (ours, original) in [
            ("id", "node_id"),
            ("exact", "exact_terms"),
            ("primary", "primary_text"),
            ("secondary", "secondary_text"),
        ] {
            let value = doc.remove(ours).unwrap();
            doc.insert(original.into(), value);
        }
    }
    data
}

#[test]
fn original_index_mutation_30_transitions_preserve_partial_documents_and_stale_cache() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/explorer_index_mutation.json")).unwrap();
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 30);
    let mut index = GraphSearchIndex::default();
    let mut scenario = Value::Null;
    for (step, row) in rows.iter().enumerate() {
        if scenario != row["scenario"] {
            scenario = row["scenario"].clone();
            index = GraphSearchIndex::default();
        }
        let result = match row["method"].as_str().unwrap() {
            "upsert" => index
                .upsert_original_partial(&row["input"], deadline())
                .map(|_| Value::Null),
            "rebuild" => index
                .rebuild_original_partial(row["input"].as_array().unwrap(), deadline())
                .map(|_| Value::Null),
            "search" => index
                .search(row["input"].as_str().unwrap(), 20, &json!({}), deadline())
                .map(|r| json!(r.matches)),
            other => panic!("unknown oracle operation {other}"),
        };
        match result {
            Ok(value) => {
                assert_eq!(row["result"]["status"], "returned", "{step}");
                assert_eq!(value, row["result"]["value"], "{step}");
            }
            Err(e) => {
                assert_eq!(row["result"]["status"], "raised", "{step}: {e}");
                let error = match row["result"]["error_type"].as_str().unwrap() {
                    "TypeError" => "explorer_search_tags_not_iterable",
                    "OverflowError" => "explorer_search_integer_float_overflow",
                    other => panic!("{other}"),
                };
                assert_eq!(e.to_string(), error, "{step}");
            }
        }
        assert_eq!(
            original_documents(&index),
            row["documents"],
            "documents step {step}"
        );
        assert_eq!(
            json!(index.cache_len()),
            row["cache_len"],
            "cache step {step}"
        );
    }
}

#[test]
fn native_atomic_operations_and_deadline_refusal_keep_prior_index() {
    let mut index = GraphSearchIndex::default();
    index
        .upsert(&json!({"id":"a","content":"bearing"}), deadline())
        .unwrap();
    index.search("bearing", 20, &json!({}), deadline()).unwrap();
    let before = serde_json::to_value(&index).unwrap();
    let bad = json!({"id":"a","properties":{"tags":true}});
    assert!(index.upsert(&bad, deadline()).is_err());
    assert_eq!(serde_json::to_value(&index).unwrap(), before);
    assert!(
        index
            .rebuild(&[json!({"id":"z"}), bad.clone()], deadline())
            .is_err()
    );
    assert_eq!(serde_json::to_value(&index).unwrap(), before);
    let expired = Instant::now() - Duration::from_secs(1);
    assert!(
        index
            .upsert_original_partial(&bad, expired)
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    assert_eq!(serde_json::to_value(&index).unwrap(), before);
    assert!(
        index
            .rebuild_original_partial(&[bad], expired)
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    assert_eq!(serde_json::to_value(&index).unwrap(), before);
}
