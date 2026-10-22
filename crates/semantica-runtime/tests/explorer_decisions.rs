use semantica_contracts::SemanticExplorerDecisionQuery;
use semantica_runtime::{
    context::load_context_graph, dataset::DatasetSession, explorer_decisions::execute,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}
fn query(op: &str, id: Option<&str>) -> SemanticExplorerDecisionQuery {
    SemanticExplorerDecisionQuery {
        schema: "semantica.explorer-decision-query/1".into(),
        operation: op.into(),
        decision_id: id.map(str::to_owned),
        category: None,
        skip: "0".into(),
        limit: 50,
    }
}
#[test]
fn original_decision_bodies_and_literal_route_identity() {
    let observed: Value = serde_json::from_str(include_str!(
        "fixtures/explorer_decisions_observations-04.json"
    ))
    .unwrap();
    let mut raw = observed["raw_graph_before"].clone();
    for e in raw["edges"].as_array_mut().unwrap() {
        let m = e.as_object_mut().unwrap();
        for (a, b) in [("source_id", "source"), ("target_id", "target")] {
            if let Some(v) = m.remove(a) {
                m.insert(b.into(), v);
            }
        }
    }
    let mut session = DatasetSession::new();
    load_context_graph(&mut session, &raw).unwrap();
    let mut compared = 0;
    let mut admission_pending = 0;
    for row in observed["rows"].as_array().unwrap() {
        if row["phase"] == "literal_id_added" {
            raw["nodes"].as_array_mut().unwrap().push(json!({"id":"causal-distance","type":"decision","content":"literal route identity","properties":{"category":"literal","scenario":"route priority","confidence":0.5}}));
            let mut staged = DatasetSession::new();
            load_context_graph(&mut staged, &raw).unwrap();
            session
                .replace_named_graph("urn:semantica:context:records/1", staged.dataset())
                .unwrap();
        }
        // These four original query-parameter errors remain assigned to HTTP admission.
        // This runtime comparison does not pretend its typed query ran the HTTP parser.
        if row["status"] == 422 {
            admission_pending += 1;
            continue;
        }
        let path = row["path"].as_str().unwrap();
        let rest = path.strip_prefix("/api/decisions").unwrap();
        let mut q = if rest.is_empty() {
            query("list", None)
        } else {
            let pieces = rest.trim_start_matches('/').split('/').collect::<Vec<_>>();
            query(pieces.get(1).copied().unwrap_or("get"), Some(pieces[0]))
        };
        q.limit = if q.operation == "precedents" { 10 } else { 50 };
        if let Some(c) = row["params"].get("category") {
            q.category = c.as_str().map(str::to_owned);
        }
        if let Some(v) = row["params"].get("skip") {
            q.skip = v.to_string();
        }
        if let Some(v) = row["params"].get("limit") {
            q.limit = v.as_u64().unwrap();
        }
        let before = session.snapshot().unwrap();
        let revision = session.revision();
        let got = execute(&session, &q, deadline()).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(json!(got.status), row["status"], "{path}");
        assert_eq!(got.body, row["response"], "{path}");
        assert_eq!(session.snapshot().unwrap(), before);
        assert_eq!(session.revision(), revision);
        compared += 1;
    }
    assert_eq!(compared, 17);
    assert_eq!(admission_pending, 4);
}
#[test]
fn expired_queries_invalid_bounds_and_same_session_changes() {
    let mut s = DatasetSession::new();
    load_context_graph(
        &mut s,
        &json!({"nodes":[{"id":"d","type":"decision"}],"edges":[]}),
    )
    .unwrap();
    let mut q = query("list", None);
    let before = s.snapshot().unwrap();
    assert!(execute(&s, &q, Instant::now() - Duration::from_secs(1)).is_err());
    q.limit = 501;
    assert!(execute(&s, &q, deadline()).is_err());
    q.limit = 50;
    q.skip = "-1".into();
    assert!(execute(&s, &q, deadline()).is_err());
    assert_eq!(s.snapshot().unwrap(), before);
    q.skip = "999999999999999999999999999999999999999".into();
    assert_eq!(execute(&s, &q, deadline()).unwrap().body, json!([]));
    let c = query("compliance", Some("d"));
    assert_eq!(execute(&s, &c, deadline()).unwrap().body["compliant"], true);
    let mut staged = DatasetSession::new();
    load_context_graph(&mut staged,&json!({"nodes":[{"id":"d","type":"decision"},{"id":"p","type":"Policy"}],"edges":[{"id":"violation","source":"d","target":"p","type":"breaches"}]})).unwrap();
    s.replace_named_graph("urn:semantica:context:records/1", staged.dataset())
        .unwrap();
    assert_eq!(
        execute(&s, &c, deadline()).unwrap().body["compliant"],
        false
    );
}
#[test]
fn original_scalar_bodies_and_rejections_keep_timestamp_precision() {
    for (plan, observed) in [
        (
            include_str!("fixtures/explorer_decisions_scalar-plan-02.json"),
            include_str!("fixtures/explorer_decisions_scalar-observations-02.json"),
        ),
        (
            include_str!("fixtures/explorer_decisions_timestamp-plan-03.json"),
            include_str!("fixtures/explorer_decisions_timestamp-observations-03.json"),
        ),
    ] {
        let plan: Value = serde_json::from_str(plan).unwrap();
        let observed: Value = serde_json::from_str(observed).unwrap();
        let mut s = DatasetSession::new();
        load_context_graph(&mut s, &json!({"nodes":plan["nodes"],"edges":[]})).unwrap();
        for row in observed["rows"].as_array().unwrap() {
            let id = row["node_id"].as_str().unwrap();
            let got = execute(&s, &query("get", Some(id)), deadline());
            if row["status"] == 200 {
                assert_eq!(
                    got.unwrap_or_else(|e| panic!("{id}: {e}")).body,
                    row["response"],
                    "{id}"
                );
            } else {
                assert_eq!(row["status"], 422);
                assert!(got.is_err(), "{id}");
            }
        }
    }
}
