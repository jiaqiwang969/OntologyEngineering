use oe_knowledge::retrieval::{Pool, Replay, select_retrieval_pools};
use serde_json::json;

#[test]
fn pool_order_changes_budget_use_without_changing_source_bytes() {
    let replay = Replay {
        schema: "oev2.retrieval-pools/1".into(),
        query: "specific current question".into(),
        context_sha256: "a".repeat(64),
        provider: "fixture".into(),
        index_identity: json!({"sha256":"b".repeat(64)}),
        limit: 2,
        pools: vec![
            Pool {
                label: "broad_context".into(),
                pages: vec![
                    json!({"id":"unrelated","body":"unrelated source"}),
                    json!({"id":"secondary","body":"context"}),
                ],
            },
            Pool {
                label: "other_context".into(),
                pages: vec![json!({"id":"generic","body":"generic source"})],
            },
            Pool {
                label: "query_and_object".into(),
                pages: vec![
                    json!({"id":"specific","body":"specific source"}),
                    json!({"id":"secondary","body":"context"}),
                ],
            },
        ],
    };
    let original = serde_json::to_vec(&replay).unwrap();
    let baseline = select_retrieval_pools(&replay, "context_first").unwrap();
    let candidate = select_retrieval_pools(&replay, "query_first").unwrap();
    assert_eq!(
        baseline.iter().map(|p| p["id"].clone()).collect::<Vec<_>>(),
        vec![json!("unrelated"), json!("generic")]
    );
    assert_eq!(
        candidate
            .iter()
            .map(|p| p["id"].clone())
            .collect::<Vec<_>>(),
        vec![json!("specific"), json!("unrelated")]
    );
    assert_eq!(serde_json::to_vec(&replay).unwrap(), original);
    let mut expanded = replay;
    expanded.limit = 8;
    let all = select_retrieval_pools(&expanded, "query_first").unwrap();
    assert_eq!(
        all.len(),
        4,
        "repeated source identity consumes only one selection"
    );
    assert!(select_retrieval_pools(&expanded, "hardcode_answer").is_err());
}
