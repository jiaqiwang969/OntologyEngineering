use semantica_contracts::SemanticExplorerGraphReadQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_read::execute,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn query(operation: &str) -> SemanticExplorerGraphReadQuery {
    SemanticExplorerGraphReadQuery {
        schema: "semantica.explorer-graph-read-query/1".into(),
        operation: operation.into(),
        node_id: None,
        node_type: None,
        edge_type: None,
        source: None,
        target: None,
        search: None,
        bbox: None,
        skip: "0".into(),
        limit: 100,
        cursor: None,
        depth: 1,
    }
}
fn session(raw: &Value) -> DatasetSession {
    let mut raw = raw.clone();
    for edge in raw["edges"].as_array_mut().unwrap() {
        let map = edge.as_object_mut().unwrap();
        if let Some(v) = map.remove("source_id") {
            map.insert("source".into(), v);
        }
        if let Some(v) = map.remove("target_id") {
            map.insert("target".into(), v);
        }
    }
    let mut s = DatasetSession::new();
    load_context_graph(&mut s, &raw).unwrap();
    s
}
#[test]
fn original_tcp_read_corpus_after_transport_admission() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_read_tcp.json")).unwrap();
    let mut s = session(&corpus["raw_graph_before"]);
    let before = s.snapshot().unwrap();
    let revision = s.revision();
    let mut checked = 0;
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        // These exact records remain in the corpus for HTTP admission/mutation
        // acceptance. They are not passed off as runtime cases.
        if [
            "nodes_limit_zero",
            "nodes_limit_large",
            "nodes_skip_negative",
            "neighbors_bad_depth",
            "node_slash",
            "after_mutation_cursor",
            "after_mutation_all",
        ]
        .contains(&name)
        {
            continue;
        }
        let path = case["path"].as_str().unwrap();
        let operation = if path.ends_with("/nodes") {
            "nodes"
        } else if path.ends_with("/edges") {
            "edges"
        } else if path.ends_with("/neighbors") {
            "neighbors"
        } else {
            "node"
        };
        let mut q = query(operation);
        if operation == "node" || operation == "neighbors" {
            // Literal expected IDs are independent of production route decoding.
            q.node_id = Some(
                match name {
                    "node_unicode" => "轴",
                    "node_percent" => "a%2Fb",
                    "node_missing" | "neighbors_missing" => "missing",
                    _ => "a",
                }
                .into(),
            );
        }
        let pairs: Vec<(String, Value)> = if let Some(a) = case["params"].as_array() {
            a.iter()
                .map(|v| (v[0].as_str().unwrap().into(), v[1].clone()))
                .collect()
        } else {
            case["params"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        for (key, value) in pairs {
            let text = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            match key.as_str() {
                "type" => {
                    if operation == "nodes" {
                        q.node_type = Some(text)
                    } else {
                        q.edge_type = Some(text)
                    }
                }
                "search" => q.search = Some(text),
                "bbox" => q.bbox = Some(text),
                "cursor" => q.cursor = Some(text),
                "source" => q.source = Some(text),
                "target" => q.target = Some(text),
                "skip" => q.skip = text,
                "limit" => {
                    q.limit = if text == "2.0" {
                        2
                    } else {
                        text.parse().unwrap()
                    }
                }
                "depth" => q.depth = text.parse().unwrap(),
                _ => panic!("unexpected fixture key"),
            }
        }
        let result = execute(&s, &q, deadline()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(result.status, case["status"].as_u64().unwrap(), "{name}");
        assert_eq!(result.body, case["body"], "{name}");
        checked += 1;
    }
    assert_eq!(checked, 50);
    assert_eq!(s.snapshot().unwrap(), before);
    assert_eq!(s.revision(), revision);
}
#[test]
fn huge_skip_and_cursor_recovery_preserve_original_request_value() {
    let s = session(&json!({"nodes":[{"id":"b"},{"id":"a"}],"edges":[]}));
    let mut q = query("nodes");
    q.skip = "9999999999999999999999999999999999999999".into();
    q.limit = 1;
    let result = execute(&s, &q, deadline()).unwrap();
    assert_eq!(result.body["nodes"], json!([]));
    assert_eq!(
        result.body["skip"],
        serde_json::from_str::<Value>(&q.skip).unwrap()
    );
    q.cursor = Some("bWlzc2luZw==".into());
    let result = execute(&s, &q, deadline()).unwrap();
    assert_eq!(result.body["nodes"][0]["id"], "a");
    assert_eq!(
        result.body["skip"],
        serde_json::from_str::<Value>(&q.skip).unwrap()
    );
    assert_eq!(result.body["next_cursor"], "YQ==");
}
#[test]
fn directed_neighbor_order_cycles_negative_edges_and_depth() {
    let nodes = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|id| json!({"id":id,"type":"Part","content":id}))
        .collect::<Vec<_>>();
    let edges=[("z","a","c",1.0),("a","a","b",0.5),("back","c","a",1.0),("negative","a","e",-1.0),("cd","c","d",0.25),("bd","b","d",0.75)]
        .iter().map(|(id,source,target,weight)|json!({"id":id,"source":source,"target":target,"type":"L","weight":weight})).collect::<Vec<_>>();
    let s = session(&json!({"nodes":nodes,"edges":edges}));
    let mut q = query("neighbors");
    q.node_id = Some("a".into());
    q.depth = 2;
    let result = execute(&s, &q, deadline()).unwrap();
    let rows = result.body.as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["c", "b", "d"]
    );
    assert_eq!(rows[2]["weight"], 0.25);
    assert_eq!(rows[2]["hop"], 2);
    q.node_id = Some("d".into());
    assert_eq!(execute(&s, &q, deadline()).unwrap().body, json!([]));
}
#[test]
fn deadline_invalid_projection_and_bounds_are_not_success() {
    let mut s = session(&json!({"nodes":[],"edges":[]}));
    let before = s.snapshot().unwrap();
    let mut q = query("nodes");
    assert!(
        execute(&s, &q, Instant::now() - Duration::from_secs(1))
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    q.limit = 0;
    assert!(
        execute(&s, &q, deadline())
            .unwrap_err()
            .to_string()
            .contains("bounds")
    );
    q.limit = 100;
    assert!(execute(&DatasetSession::new(), &q, deadline()).is_err());
    assert_eq!(s.snapshot().unwrap(), before);
}
