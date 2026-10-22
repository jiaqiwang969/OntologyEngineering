use semantica_contracts::SemanticNativeValueEnvelope;
use semantica_json::native_value::{NativeLimits, NativeNode as N, NativeValue};
use semantica_json::original_json::ParseLimits;
use semantica_json::original_json_bytes::DecodeLimits;
use semantica_runtime::{context, dataset::DatasetSession, native_values};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
fn limits() -> NativeLimits {
    NativeLimits {
        max_nodes: 100_000,
        max_links: 200_000,
        max_scalar_bytes: 1_000_000,
        max_depth: 4096,
        max_wire_bytes: 8_000_000,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn flatten(v: &Value, nodes: &mut Vec<N>) -> usize {
    let node = match v["type"].as_str().unwrap() {
        "none" => N::None,
        "bool" => N::Bool(v["value"].as_bool().unwrap()),
        "int" => N::Integer(v["decimal"].as_str().unwrap().into()),
        "float" => {
            N::FloatBits(u64::from_str_radix(v["ieee754_be_hex"].as_str().unwrap(), 16).unwrap())
        }
        "str" => N::Text(
            v["codepoints"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u32)
                .collect(),
        ),
        "list" => N::List(
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| flatten(v, nodes))
                .collect(),
        ),
        "dict" => N::Dict(
            v["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (flatten(&p[0], nodes), flatten(&p[1], nodes)))
                .collect(),
        ),
        other => panic!("unexpected original graph type {other}"),
    };
    let id = nodes.len();
    nodes.push(node);
    id
}
fn graph(row: &Value) -> SemanticNativeValueEnvelope {
    let pairs = row["typed_state_after_import"]["items"].as_array().unwrap();
    let selected = pairs
        .iter()
        .filter(|p| {
            p[0]["codepoints"] == json!([110, 111, 100, 101, 115])
                || p[0]["codepoints"] == json!([101, 100, 103, 101, 115])
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 2);
    let mut nodes = Vec::new();
    let root = flatten(&json!({"type":"dict","items":selected}), &mut nodes);
    native_values::encode(&NativeValue::new(nodes, root, limits()).unwrap(), limits()).unwrap()
}
fn parse(input: &[u8]) -> SemanticNativeValueEnvelope {
    native_values::parse_import_json(
        input,
        ParseLimits {
            decode: DecodeLimits {
                max_input_bytes: 1_000_000,
                max_codepoints: 1_000_000,
            },
            native: limits(),
        },
    )
    .unwrap()
}
#[test]
fn twelve_real_original_graph_states_survive_same_dataset_storage_and_reload() {
    let original: Value = serde_json::from_slice(include_bytes!(
        "fixtures/original_native_context_states.json"
    ))
    .unwrap();
    assert_eq!(original["rows"].as_array().unwrap().len(), 12);
    for row in original["rows"].as_array().unwrap() {
        let input = graph(row);
        let expected = serde_json::to_value(&input).unwrap();
        let mut session = DatasetSession::new();
        context::load_context_graph(&mut session, &json!({"nodes":[{"id":"old"}],"edges":[]}))
            .unwrap();
        session
            .update("INSERT DATA { GRAPH <urn:other> { <urn:s> <urn:p> <urn:o> } }")
            .unwrap();
        let revision = session.revision();
        context::store_native_records(&mut session, &input, limits(), deadline()).unwrap();
        assert_eq!(session.revision(), revision + 1);
        assert_eq!(
            session
                .query(
                    "ASK { GRAPH <urn:other> { <urn:s> <urn:p> <urn:o> } }",
                    false
                )
                .unwrap()
                .result["value"],
            true
        );
        // Old readers must not silently return an empty or lossy projection.
        assert!(context::context_summary(&session).is_err());
        assert_eq!(session.quad_count(), 3);
        assert_eq!(
            serde_json::to_value(
                context::read_native_records(&session, limits(), deadline()).unwrap()
            )
            .unwrap(),
            expected,
            "{}",
            row["id"]
        );
        assert_eq!(session.revision(), revision + 1);
        let snapshot = session.snapshot().unwrap();
        let mut restored = DatasetSession::new();
        restored
            .load(
                snapshot.canonical_nquads.as_bytes(),
                oxigraph::io::RdfFormat::NQuads,
                None,
                None,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(
                context::read_native_records(&restored, limits(), deadline()).unwrap()
            )
            .unwrap(),
            expected,
            "reload {}",
            row["id"]
        );
    }
}
#[test]
fn native_graph_refusals_and_tamper_keep_original_session_boundaries() {
    let mut session = DatasetSession::new();
    context::load_context_graph(&mut session, &json!({"nodes":[{"id":"old"}],"edges":[]})).unwrap();
    assert!(context::read_native_records(&session, limits(), deadline()).is_err());
    let before = session.snapshot().unwrap();
    let empty = parse(br#"{"nodes":[],"edges":[]}"#);
    assert!(
        context::store_native_records(
            &mut session,
            &empty,
            limits(),
            Instant::now() - Duration::from_secs(1)
        )
        .is_err()
    );
    let mut small = limits();
    small.max_wire_bytes = 1;
    assert!(context::store_native_records(&mut session, &empty, small, deadline()).is_err());
    for raw in [
        br#"{"nodes":[],"edges":[],"proof":true}"#.as_slice(),
        br#"{"nodes":[{"id":"a","type":"entity","properties":{}},{"id":"a","type":"entity","properties":{}}],"edges":[]}"#,
        br#"{"nodes":[],"edges":[{"id":"e","source_id":"a","target_id":"b","type":"related","weight":NaN,"properties":{}}]}"#,
        br#"{"nodes":[{"id":"a","type":"entity","properties":null}],"edges":[]}"#,
    ] {let bad=parse(raw);assert!(context::store_native_records(&mut session,&bad,limits(),deadline()).is_err());assert_eq!(session.snapshot().unwrap(),before);}
    context::store_native_records(&mut session, &empty, limits(), deadline()).unwrap();
    session.update("INSERT DATA { GRAPH <urn:semantica:context:records/1> { <urn:semantica:context:root> <urn:foreign> \"extra\" } }").unwrap();
    let corrupted = session.snapshot().unwrap();
    assert!(context::read_native_records(&session, limits(), deadline()).is_err());
    assert_eq!(session.snapshot().unwrap(), corrupted);
}

#[test]
fn native_record_publish_obeys_combined_dataset_budget_without_partial_replacement() {
    let mut session = DatasetSession::new();
    let other = format!(
        "<urn:s> <urn:p> \"{}\" <urn:other> .\n",
        "x".repeat(session.dataset_byte_limit() - 2048)
    );
    session
        .load(
            other.as_bytes(),
            oxigraph::io::RdfFormat::NQuads,
            None,
            None,
        )
        .unwrap();
    context::load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
    let before = session.snapshot().unwrap();
    let input=parse(serde_json::to_vec(&json!({"nodes":[{"id":"a","type":"entity","properties":{"content":"x".repeat(2000)}}],"edges":[]})).unwrap().as_slice());
    let error =
        context::store_native_records(&mut session, &input, limits(), deadline()).unwrap_err();
    assert_eq!(error.to_string(), "dataset_byte_budget");
    assert_eq!(session.snapshot().unwrap(), before);
    assert_eq!(context::context_summary(&session).unwrap()["node_count"], 0);
}

#[test]
fn typed_profile_never_reuses_an_old_graph_search_index() {
    use semantica_contracts::{SemanticExplorerMutationInput, SemanticExplorerSearchQuery};
    use semantica_runtime::{explorer_mutation, explorer_search};
    let mut session = DatasetSession::new();
    context::load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
    let input = SemanticExplorerMutationInput {
        schema: "semantica.explorer-mutation-input/1".into(),
        bridge: true,
        call: json!({"method":"add_node","args":["old","Part","bearing"],"kwargs":{}}),
    };
    explorer_mutation::execute(&mut session, &input, deadline()).unwrap();
    let query = SemanticExplorerSearchQuery {
        schema: "semantica.explorer-search-query/1".into(),
        body: json!({"query":"bearing","limit":20}),
    };
    assert_eq!(
        explorer_search::execute(&mut session, &query, deadline())
            .unwrap()
            .body["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    context::store_native_records(
        &mut session,
        &parse(br#"{"nodes":[],"edges":[]}"#),
        limits(),
        deadline(),
    )
    .unwrap();
    let before = session.snapshot().unwrap();
    assert!(explorer_search::execute(&mut session, &query, deadline()).is_err());
    assert!(explorer_mutation::observe(&session).is_err());
    assert_eq!(session.snapshot().unwrap(), before);
}
