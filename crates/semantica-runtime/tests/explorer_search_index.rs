use semantica_runtime::explorer_search_index::{GraphSearchIndex, SearchMatches};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
fn load() -> Value {
    serde_json::from_str(include_str!("fixtures/explorer_search_tcp.json")).unwrap()
}
fn index_oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/explorer_search_index.json")).unwrap()
}
fn pairs(v: &SearchMatches) -> Value {
    json!(v.matches)
}
fn compare(v: &SearchMatches, r: &Value) {
    assert_eq!(pairs(v), r["result"]);
    assert_eq!(
        json!({"cache_hit":v.cache_hit,"path":v.path,"candidates":v.candidates}),
        r["diagnostics"]
    );
}
#[test]
fn original_tcp_relevance_scores_filters_and_mutation_invalidation() {
    let corpus = load();
    let nodes: Vec<_> = corpus["raw_graph_before"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let mut n = n.clone();
            n["content"] = n["properties"]["content"].clone();
            n
        })
        .collect();
    let mut index = GraphSearchIndex::default();
    index.rebuild(&nodes, deadline()).unwrap();
    let mut checked = 0;
    for row in corpus["cases"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        if name == "after_add" {
            index
                .upsert(&corpus["mutations"][0]["node"], deadline())
                .unwrap();
        }
        if name == "after_update_old_query" {
            index
                .upsert(&corpus["mutations"][1]["node"], deadline())
                .unwrap();
        }
        if name == "after_delete" {
            index.remove("new", deadline()).unwrap();
        }
        let body = &row["body"];
        if row["status"] != 200 || body["anchor_node"].as_str().is_some_and(|s| !s.is_empty()) {
            continue;
        }
        let query = body["query"].as_str().unwrap();
        let limit = match body.get("limit") {
            None => 20,
            Some(Value::Bool(b)) => usize::from(*b),
            Some(Value::String(s)) => s.parse::<f64>().unwrap() as usize,
            Some(v) => v.as_f64().unwrap() as usize,
        };
        let actual = index
            .search(
                query,
                limit,
                body.get("filters").unwrap_or(&json!({})),
                deadline(),
            )
            .unwrap();
        let expected: Vec<_> = row["response"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| json!([x["node"]["id"], x["score"]]))
            .collect();
        assert_eq!(pairs(&actual), json!(expected), "{name}");
        checked += 1;
    }
    assert!(
        checked >= 60,
        "corpus coverage accidentally narrowed: {checked}"
    );
}
#[test]
fn original_secondary_scan_and_hit_caps() {
    let oracle = index_oracle();
    let mut index = GraphSearchIndex::default();
    let nodes:Vec<_>=(0..12005).map(|i|json!({"id":format!("n{i:05}"),"type":"Part","content":"plain","properties":{"note":"needle"}})).collect();
    index.rebuild(&nodes, deadline()).unwrap();
    for row in &oracle["rows"].as_array().unwrap()[..5] {
        let actual = index
            .search(
                "needle",
                row["limit"].as_u64().unwrap() as usize,
                &json!({}),
                deadline(),
            )
            .unwrap();
        compare(&actual, row);
    }
    let nodes:Vec<_>=(0..12005).map(|i|json!({"id":format!("n{i:05}"),"type":"Part","content":"plain","properties":{"note":if i>=12000{"needle"}else{"other"}}})).collect();
    index.rebuild(&nodes, deadline()).unwrap();
    compare(
        &index.search("needle", 20, &json!({}), deadline()).unwrap(),
        &oracle["rows"][5],
    );
}
#[test]
fn original_fragment_limits_and_cache_lifecycle() {
    let oracle = index_oracle();
    let mut index = GraphSearchIndex::default();
    index
        .rebuild(&[oracle["rows"][6]["input"].clone()], deadline())
        .unwrap();
    for row in &oracle["rows"].as_array().unwrap()[7..13] {
        compare(
            &index
                .search(row["query"].as_str().unwrap(), 20, &json!({}), deadline())
                .unwrap(),
            row,
        );
    }
    index
        .rebuild(
            &[json!({"id":"a","content":"first","properties":{}})],
            deadline(),
        )
        .unwrap();
    for op in oracle["rows"][13]["operations"].as_array().unwrap() {
        match op["action"].as_str().unwrap() {
            "query" => compare(
                &index
                    .search(op["query"].as_str().unwrap(), 20, &json!({}), deadline())
                    .unwrap(),
                op,
            ),
            "upsert" => index.upsert(&op["node"], deadline()).unwrap(),
            "remove" => index
                .remove(op["id"].as_str().unwrap(), deadline())
                .unwrap(),
            _ => panic!(),
        }
    }
    assert_eq!(index.cache_len(), 128);
}
#[test]
fn invalid_filters_and_expired_work_are_not_success_or_cached_results() {
    let mut index = GraphSearchIndex::default();
    index
        .rebuild(
            &[json!({"id":"a","content":"bearing","properties":{}})],
            deadline(),
        )
        .unwrap();
    assert!(
        index
            .search("bearing", 20, &json!({"tags":1}), deadline())
            .is_err()
    );
    assert_eq!(index.cache_len(), 0);
    assert!(
        index
            .search(
                "bearing",
                20,
                &json!({}),
                Instant::now() - Duration::from_secs(1)
            )
            .is_err()
    );
    assert_eq!(index.cache_len(), 0);
    let expired = Instant::now() - Duration::from_secs(1);
    assert!(
        index
            .upsert(
                &json!({"id":"b","content":"changed","properties":{}}),
                expired
            )
            .is_err()
    );
    assert!(index.remove("a", expired).is_err());
    assert!(index.rebuild(&[], expired).is_err());
    assert_eq!(
        index
            .search("bearing", 20, &json!({}), deadline())
            .unwrap()
            .matches[0]
            .0,
        "a"
    );
}
