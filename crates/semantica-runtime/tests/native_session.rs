//! Typed observations from the actual original GraphSession and ContextGraph.
//! The oracle source and source freeze are recorded in the private review;
//! these development-exposed cases are not held-out or HTTP qualification.
use semantica_contracts::{SemanticExplorerNativeState, SemanticNativeValueEnvelope};
use semantica_json::{
    native_operations::{OperationError, TextLimits},
    native_value::{NativeLimits, NativeNode, NativeValue},
};
use semantica_runtime::explorer_read::{
    NativeNodeProjection, normalize_node_native, project_native_node_records,
};
use semantica_runtime::{context, dataset::DatasetSession, native_values};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn limits() -> NativeLimits {
    NativeLimits {
        max_nodes: 100_000,
        max_links: 200_000,
        max_scalar_bytes: 16_000_000,
        max_depth: 128,
        max_wire_bytes: 32_000_000,
    }
}
fn text_limits() -> TextLimits {
    TextLimits {
        max_codepoints: 1_000_000,
        max_visits: 100_000,
        max_depth: 128,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
fn fixture() -> (Value, Value) {
    (
        serde_json::from_slice(include_bytes!("fixtures/native_session_plan.json")).unwrap(),
        serde_json::from_slice(include_bytes!("fixtures/native_session_observations.json"))
            .unwrap(),
    )
}
fn native(value: &Value) -> NativeValue {
    NativeValue::from_envelope(value, limits()).unwrap()
}
fn plain(value: Value) -> NativeValue {
    NativeValue::from_plain_json(&value, limits()).unwrap()
}

// Node allocation is incidental, but ordered mappings and container distinctions
// are significant: update retains the first equal key and its original position.
fn semantic(value: &NativeValue, id: usize) -> Value {
    let sequence = |kind: &str, ids: &[usize]| {
        json!([
            kind,
            ids.iter()
                .map(|&id| semantic(value, id))
                .collect::<Vec<_>>()
        ])
    };
    match &value.nodes()[id] {
        NativeNode::None => json!(["none"]),
        NativeNode::Bool(v) => json!(["bool", v]),
        NativeNode::Integer(v) => json!(["integer", v]),
        NativeNode::FloatBits(v) => json!(["float64", format!("{v:016x}")]),
        NativeNode::Text(v) => json!(["text", v]),
        NativeNode::List(ids) => sequence("list", ids),
        NativeNode::Tuple(ids) => sequence("tuple", ids),
        NativeNode::Set(ids) => sequence("set", ids),
        NativeNode::FrozenSet(ids) => sequence("frozenset", ids),
        NativeNode::Dict(pairs) => json!([
            "dict",
            pairs
                .iter()
                .map(|&(k, v)| json!([semantic(value, k), semantic(value, v)]))
                .collect::<Vec<_>>()
        ]),
    }
}
fn root_semantic(value: &NativeValue) -> Value {
    semantic(value, value.root())
}
fn error_class(error: &anyhow::Error) -> &str {
    if let Some(original) = error
        .downcast_ref::<OperationError>()
        .and_then(|error| error.original_class())
    {
        return original;
    }
    match error.to_string().as_str() {
        "explorer_normalize_update_type_error" | "explorer_normalize_key_type_error" => "TypeError",
        "explorer_normalize_update_value_error" => "ValueError",
        "explorer_search_tags_not_iterable" => "TypeError",
        "explorer_search_integer_float_overflow" => "OverflowError",
        _ => "unmapped_native_error",
    }
}

#[test]
fn original_43_normalizations_preserve_ordered_properties_types_and_error_classes() {
    let (plan, observations) = fixture();
    let cases = plan["normalizations"].as_array().unwrap();
    let rows = observations["normalizations"].as_array().unwrap();
    assert_eq!(cases.len(), 43);
    assert_eq!(rows.len(), cases.len());
    let (mut returned, mut raised) = (0, 0);
    for (case, row) in cases.iter().zip(rows) {
        assert_eq!(case["id"], row["id"]);
        let id = case["id"].as_str().unwrap();
        assert_eq!(row["input_unchanged"], true, "original input: {id}");
        let value = native(&case["input"]);
        let before = root_semantic(&value);
        let actual =
            normalize_node_native(&value, value.root(), limits(), text_limits(), deadline());
        match row["result"]["status"].as_str().unwrap() {
            "returned" => {
                let actual = actual.unwrap_or_else(|e| panic!("{id}: {e}"));
                assert_eq!(
                    root_semantic(&actual),
                    root_semantic(&native(&row["result"]["value"])),
                    "normalized {id}"
                );
                returned += 1;
            }
            "raised" => {
                let error = actual.unwrap_err();
                assert_eq!(
                    error_class(&error),
                    row["result"]["class"].as_str().unwrap(),
                    "{id}: {error}"
                );
                raised += 1;
            }
            status => panic!("unexpected original status {status}"),
        }
        assert_eq!(
            root_semantic(&value),
            before,
            "native input unchanged: {id}"
        );
        assert_eq!(
            before,
            root_semantic(&native(&row["input_after"])),
            "original input unchanged: {id}"
        );
    }
    assert_eq!((returned, raised), (36, 7));
}

#[test]
fn normalization_limits_deadline_and_bad_references_do_not_change_raw_input() {
    let value = plain(
        json!({"id":"a","content":"bearing","metadata":{"old":1},"properties":{"label":"support","extra":["x","y"]}}),
    );
    let before = root_semantic(&value);
    let expired = Instant::now() - Duration::from_secs(1);
    assert!(normalize_node_native(&value, value.root(), limits(), text_limits(), expired).is_err());
    assert!(
        normalize_node_native(&value, usize::MAX, limits(), text_limits(), deadline()).is_err()
    );
    for small in [
        NativeLimits {
            max_nodes: 1,
            ..limits()
        },
        NativeLimits {
            max_links: 1,
            ..limits()
        },
        NativeLimits {
            max_scalar_bytes: 1,
            ..limits()
        },
        NativeLimits {
            max_depth: 0,
            ..limits()
        },
    ] {
        assert!(
            normalize_node_native(&value, value.root(), small, text_limits(), deadline()).is_err()
        );
        assert_eq!(root_semantic(&value), before);
    }
    for small in [
        TextLimits {
            max_codepoints: 1,
            ..text_limits()
        },
        TextLimits {
            max_visits: 1,
            ..text_limits()
        },
    ] {
        assert!(normalize_node_native(&value, value.root(), limits(), small, deadline()).is_err());
        assert_eq!(root_semantic(&value), before);
    }
    assert!(
        normalize_node_native(&value, value.root(), limits(), text_limits(), deadline()).is_ok()
    );
    assert_eq!(root_semantic(&value), before);
}

fn field(value: &NativeValue, id: usize, name: &str) -> usize {
    let NativeNode::Dict(pairs) = &value.nodes()[id] else {
        panic!("field {name}: expected dict");
    };
    pairs
        .iter()
        .find_map(|&(k, v)| match &value.nodes()[k] {
            NativeNode::Text(points) if points.iter().copied().eq(name.chars().map(u32::from)) => {
                Some(v)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing field {name}"))
}
fn unordered(value: &NativeValue, id: usize) -> Value {
    let mut result = semantic(value, id);
    fn sort(value: &mut Value) {
        let parts = value.as_array_mut().unwrap();
        match parts[0].as_str().unwrap() {
            "dict" => {
                let entries = parts[1].as_array_mut().unwrap();
                for pair in entries.iter_mut() {
                    sort(&mut pair[0]);
                    sort(&mut pair[1]);
                }
                entries.sort_by_cached_key(|pair| serde_json::to_string(&pair[0]).unwrap());
            }
            "list" | "tuple" | "set" | "frozenset" => {
                for item in parts[1].as_array_mut().unwrap() {
                    sort(item);
                }
            }
            _ => {}
        }
    }
    sort(&mut result);
    result
}
fn query_value(query: &NativeValue, filters: &NativeValue) -> (NativeValue, usize, usize) {
    let mut nodes = query.nodes().to_vec();
    let shift = nodes.len();
    for node in filters.nodes() {
        nodes.push(match node {
            NativeNode::List(v) => NativeNode::List(v.iter().map(|id| id + shift).collect()),
            NativeNode::Tuple(v) => NativeNode::Tuple(v.iter().map(|id| id + shift).collect()),
            NativeNode::Set(v) => NativeNode::Set(v.iter().map(|id| id + shift).collect()),
            NativeNode::FrozenSet(v) => {
                NativeNode::FrozenSet(v.iter().map(|id| id + shift).collect())
            }
            NativeNode::Dict(v) => {
                NativeNode::Dict(v.iter().map(|(k, v)| (k + shift, v + shift)).collect())
            }
            v => v.clone(),
        });
    }
    let q = query.root();
    let f = filters.root() + shift;
    let root = nodes.len();
    nodes.push(NativeNode::Tuple(vec![q, f]));
    (NativeValue::new(nodes, root, limits()).unwrap(), q, f)
}
fn envelope(value: &Value) -> SemanticNativeValueEnvelope {
    native_values::encode(&native(value), limits()).unwrap()
}
fn state_observations() -> Value {
    serde_json::from_slice(include_bytes!(
        "fixtures/native_session_context_state_observations.json"
    ))
    .unwrap()
}
fn append_value(nodes: &mut Vec<NativeNode>, value: &NativeValue) -> usize {
    let shift = nodes.len();
    for node in value.nodes() {
        nodes.push(match node {
            NativeNode::List(v) => NativeNode::List(v.iter().map(|id| id + shift).collect()),
            NativeNode::Tuple(v) => NativeNode::Tuple(v.iter().map(|id| id + shift).collect()),
            NativeNode::Set(v) => NativeNode::Set(v.iter().map(|id| id + shift).collect()),
            NativeNode::FrozenSet(v) => {
                NativeNode::FrozenSet(v.iter().map(|id| id + shift).collect())
            }
            NativeNode::Dict(v) => {
                NativeNode::Dict(v.iter().map(|(k, v)| (k + shift, v + shift)).collect())
            }
            v => v.clone(),
        });
    }
    value.root() + shift
}
fn append_text(nodes: &mut Vec<NativeNode>, value: &str) -> usize {
    let id = nodes.len();
    nodes.push(NativeNode::Text(value.chars().map(u32::from).collect()));
    id
}
fn optional_field(value: &NativeValue, id: usize, name: &str) -> Option<usize> {
    let NativeNode::Dict(pairs) = &value.nodes()[id] else {
        panic!("constructor dict")
    };
    pairs.iter().find_map(|&(k, v)| match &value.nodes()[k] {
        NativeNode::Text(points) if points.iter().copied().eq(name.chars().map(u32::from)) => {
            Some(v)
        }
        _ => None,
    })
}
fn list_items(value: &NativeValue, id: usize) -> &[usize] {
    let NativeNode::List(ids) = &value.nodes()[id] else {
        panic!("expected list")
    };
    ids
}

// Inputs come from the original constructor arguments frozen in PLAN-01,
// including that process's actual set iteration. Capture 03 independently
// checks raw fields, defaults, retained IDs and the two original projections.
// No expected search result or lossy to_dict record supplies a hidden field.
fn constructor_state(plan: &Value, id: &str) -> SemanticNativeValueEnvelope {
    let source = plan["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    let observed = state_observations();
    let row = observed["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    let actual_state = native(&row["native_context_state"]);
    let actual_ids: Vec<_> = list_items(
        &actual_state,
        field(&actual_state, actual_state.root(), "nodes"),
    )
    .iter()
    .map(|&n| semantic(&actual_state, field(&actual_state, n, "id")))
    .collect();
    let constructors = native(&source["constructor_nodes"]);
    let defaults = native(&observed["context_node_defaults"]);
    let mut nodes = constructors.nodes().to_vec();
    let defaults_shift = nodes.len();
    append_value(&mut nodes, &defaults);
    let mut node_ids = vec![];
    for &constructor in list_items(&constructors, constructors.root()) {
        let original_id = field(&constructors, constructor, "node_id");
        if !actual_ids.contains(&semantic(&constructors, original_id)) {
            continue;
        }
        let mut pairs = vec![];
        for (target, input) in [
            ("id", "node_id"),
            ("type", "node_type"),
            ("content", "content"),
            ("metadata", "metadata"),
            ("properties", "properties"),
            ("valid_from", "valid_from"),
            ("valid_until", "valid_until"),
        ] {
            let value = optional_field(&constructors, constructor, input)
                .unwrap_or_else(|| defaults_shift + field(&defaults, defaults.root(), input));
            let key = append_text(&mut nodes, target);
            pairs.push((key, value));
        }
        node_ids.push(nodes.len());
        nodes.push(NativeNode::Dict(pairs));
    }
    assert_eq!(
        node_ids.len(),
        actual_ids.len(),
        "actual retained nodes: {id}"
    );
    let node_list = nodes.len();
    nodes.push(NativeNode::List(node_ids));
    // Original edges have no cross-process set-order variation in this fixture;
    // retain the actual ContextEdge fields, including its resolved family ID.
    assert_eq!(row["constructor_edge_iteration_matches_plan01"], true);
    let edges = append_value(&mut nodes, &selected(&actual_state, "edges"));
    let nodes_key = append_text(&mut nodes, "nodes");
    let edges_key = append_text(&mut nodes, "edges");
    let root = nodes.len();
    nodes.push(NativeNode::Dict(vec![
        (nodes_key, node_list),
        (edges_key, edges),
    ]));
    native_values::encode(&subtree(&nodes, root), limits()).unwrap()
}
fn observe(session: &mut DatasetSession) -> NativeValue {
    native_values::decode(
        &session
            .observe_native_explorer_index(limits(), text_limits(), deadline())
            .unwrap(),
        limits(),
    )
    .unwrap()
}
fn index_matches(session: &mut DatasetSession, state: &Value, label: &str) {
    let expected = native(state);
    let actual = observe(session);
    assert_eq!(
        unordered(&actual, actual.root()),
        unordered(&expected, field(&expected, expected.root(), "index")),
        "index: {label}"
    );
}
fn reload(session: &mut DatasetSession) -> DatasetSession {
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
        restored.snapshot().unwrap().dataset_sha256,
        snapshot.dataset_sha256
    );
    restored
}

#[test]
fn eleven_original_session_constructors_and_96_searches_survive_checkpoint_reload() {
    let (plan, observations) = fixture();
    let rows = observations["scenarios"].as_array().unwrap();
    let queries = plan["queries"].as_array().unwrap();
    assert_eq!(rows.len(), 11);
    assert_eq!(queries.len(), 12);
    let (mut initialized, mut failed, mut searches) = (0, 0, 0);
    for row in rows {
        let id = row["id"].as_str().unwrap();
        let input = constructor_state(&plan, id);
        let mut session = DatasetSession::new();
        context::store_native_context_state(&mut session, &input, limits(), deadline()).unwrap();
        session
            .update("INSERT DATA { GRAPH <urn:other> { <urn:s> <urn:p> <urn:o> } }")
            .unwrap();
        let before = session.snapshot().unwrap();
        let result = session.prepare_native_explorer_index(limits(), text_limits(), deadline());
        if row["initialize"]["status"] == "raised" {
            let error = result.unwrap_err();
            assert_eq!(
                error_class(&error),
                row["initialize"]["class"].as_str().unwrap(),
                "constructor {id}: {error}"
            );
            assert_eq!(
                session.snapshot().unwrap(),
                before,
                "constructor refusal {id}"
            );
            assert!(
                session
                    .observe_native_explorer_index(limits(), text_limits(), deadline())
                    .is_err(),
                "failed original private half-object must not become a ready cache: {id}"
            );
            assert_eq!(session.snapshot().unwrap(), before);
            failed += 1;
            continue;
        }
        result.unwrap_or_else(|e| panic!("constructor {id}: {e}"));
        initialized += 1;
        assert_eq!(session.snapshot().unwrap(), before);
        index_matches(&mut session, &row["state_after_initialize"], id);
        for (j, (query, expected)) in queries
            .iter()
            .zip(row["searches"].as_array().unwrap())
            .enumerate()
        {
            let (v, q, f) = query_value(&native(&query["query"]), &native(&query["filters"]));
            let actual = session
                .search_native_explorer(&v, q, 20, f, limits(), text_limits(), deadline())
                .unwrap_or_else(|e| panic!("{id} query {j}: {e}"));
            assert_eq!(actual.schema, "semantica.explorer-native-search-result/1");
            assert_eq!(expected["result"]["status"], "returned");
            assert_eq!(
                root_semantic(&native_values::decode(&actual.results, limits()).unwrap()),
                root_semantic(&native(&expected["result"]["value"])),
                "session results {id}/{j}"
            );
            assert_eq!(
                json!({"cache_hit":actual.cache_hit,"path":actual.path,"candidates":actual.candidates}),
                expected["index_call"]["diagnostics"],
                "diagnostics {id}/{j}"
            );
            index_matches(&mut session, &expected["state_after"], &format!("{id}/{j}"));
            let revision = session.revision();
            let saved = session
                .checkpoint_native_explorer(false, limits(), text_limits(), deadline())
                .unwrap();
            assert_eq!(saved.schema, "semantica.explorer-mutation-state/2");
            assert_eq!(saved.graph_revision, 0);
            assert!(!saved.bridge);
            assert_eq!(session.revision(), revision + 1);
            let original_state = native(&expected["state_after"]);
            let original_types = field(&original_state, original_state.root(), "node_type_index");
            let actual_types = native_values::decode(&saved.node_type_index, limits()).unwrap();
            assert_eq!(
                unordered(&actual_types, actual_types.root()),
                unordered(&original_state, original_types)
            );
            session = reload(&mut session);
            index_matches(
                &mut session,
                &expected["state_after"],
                &format!("restored {id}/{j}"),
            );
            assert_eq!(
                root_semantic(
                    &native_values::decode(
                        &context::read_native_context_state(&session, limits(), deadline())
                            .unwrap(),
                        limits()
                    )
                    .unwrap()
                ),
                root_semantic(&native_values::decode(&input, limits()).unwrap())
            );
            searches += 1;
        }
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
    }
    assert_eq!((initialized, failed, searches), (8, 3, 96));
}

fn ordinary_session() -> DatasetSession {
    let (plan, _) = fixture();
    let mut session = DatasetSession::new();
    context::store_native_context_state(
        &mut session,
        &constructor_state(&plan, "ordinary"),
        limits(),
        deadline(),
    )
    .unwrap();
    session
        .prepare_native_explorer_index(limits(), text_limits(), deadline())
        .unwrap();
    session
}

#[test]
fn original_projection_collisions_require_complete_context_state() {
    let observations = state_observations();
    let collisions = observations["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 2);
    for collision in collisions {
        let label = collision["id"].as_str().unwrap();
        assert_eq!(collision["to_dict_equal"], true);
        assert_eq!(collision["find_node_different"], true);
        assert_eq!(collision["search_different"], true);
        let mut projections = vec![];
        let mut searches = vec![];
        for side in collision["sides"].as_array().unwrap() {
            let input = native(&side["native_context_state"]);
            let ids = list_items(&input, field(&input, input.root(), "nodes"));
            assert_eq!(ids.len(), 1);
            let mut views = vec![];
            for (mode, name) in [
                (NativeNodeProjection::ToDict, "to_dict"),
                (NativeNodeProjection::FindNode, "find_node"),
            ] {
                let actual = project_native_node_records(
                    &input,
                    ids,
                    mode,
                    limits(),
                    text_limits(),
                    deadline(),
                )
                .unwrap();
                let actual_ids = list_items(&actual, actual.root());
                assert_eq!(actual_ids.len(), 1);
                let view = semantic(&actual, actual_ids[0]);
                assert_eq!(view, root_semantic(&native(&side[name])), "{label}: {name}");
                views.push(view);
            }
            projections.push(views);
            let mut session = DatasetSession::new();
            let wire = native_values::encode(&input, limits()).unwrap();
            context::store_native_context_state(&mut session, &wire, limits(), deadline()).unwrap();
            assert!(context::read_native_records(&session, limits(), deadline()).is_err());
            let (value, query, filters) = query_value(&plain(json!("bearing")), &plain(json!({})));
            let actual = session
                .search_native_explorer(
                    &value,
                    query,
                    20,
                    filters,
                    limits(),
                    text_limits(),
                    deadline(),
                )
                .unwrap();
            let actual = root_semantic(&native_values::decode(&actual.results, limits()).unwrap());
            assert_eq!(
                actual,
                root_semantic(&native(&side["search"])),
                "{label}: actual GraphSession search"
            );
            searches.push(actual);
            let stored =
                context::read_native_context_state(&session, limits(), deadline()).unwrap();
            assert_eq!(
                root_semantic(&native_values::decode(&stored, limits()).unwrap()),
                root_semantic(&input)
            );
        }
        assert_eq!(projections[0][0], projections[1][0]);
        assert_ne!(projections[0][1], projections[1][1]);
        assert_ne!(searches[0], searches[1]);
    }
}

#[test]
fn native_context_state_storage_guards_keep_other_graphs_and_revision() {
    let (plan, observations) = fixture();
    let input = constructor_state(&plan, "special-values-and-edge");
    let mut session = DatasetSession::new();
    session
        .update("INSERT DATA { GRAPH <urn:other> { <urn:s> <urn:p> <urn:o> } }")
        .unwrap();
    let revision = session.revision();
    context::store_native_context_state(&mut session, &input, limits(), deadline()).unwrap();
    assert_eq!(session.revision(), revision + 1);
    let expected = root_semantic(&native_values::decode(&input, limits()).unwrap());
    let stored = context::read_native_context_state(&session, limits(), deadline()).unwrap();
    assert_eq!(
        root_semantic(&native_values::decode(&stored, limits()).unwrap()),
        expected
    );
    assert!(context::read_native_records(&session, limits(), deadline()).is_err());
    session = reload(&mut session);
    let stored = context::read_native_context_state(&session, limits(), deadline()).unwrap();
    assert_eq!(
        root_semantic(&native_values::decode(&stored, limits()).unwrap()),
        expected
    );
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
    let before = session.snapshot().unwrap();
    let valid = json!({"nodes":[{"id":"a","type":"entity","content":"bearing","metadata":{},"properties":{},"valid_from":null,"valid_until":null}],"edges":[]});
    let mut missing = valid.clone();
    missing["nodes"][0]
        .as_object_mut()
        .unwrap()
        .remove("metadata");
    let mut extra = valid.clone();
    extra["nodes"][0]["hidden"] = json!("unexpected");
    let mut wrong_id = valid.clone();
    wrong_id["nodes"][0]["id"] = json!(7);
    let mut duplicate_id = valid.clone();
    duplicate_id["nodes"]
        .as_array_mut()
        .unwrap()
        .push(valid["nodes"][0].clone());
    let mut endpoint = valid.clone();
    endpoint["edges"] = json!([{"id":"e","family_id":"f","source_id":"a","target_id":"absent","type":"link","weight":1.0,"metadata":{},"valid_from":null,"valid_until":null}]);
    let mut edge_shape = endpoint.clone();
    edge_shape["edges"][0]["target_id"] = json!("a");
    edge_shape["edges"][0]
        .as_object_mut()
        .unwrap()
        .remove("family_id");
    for malformed in [missing, extra, wrong_id, duplicate_id, endpoint, edge_shape] {
        let wire = native_values::encode(&plain(malformed), limits()).unwrap();
        assert!(
            context::store_native_context_state(&mut session, &wire, limits(), deadline()).is_err()
        );
        assert_eq!(session.snapshot().unwrap(), before);
    }
    let expired = Instant::now() - Duration::from_secs(1);
    assert!(context::store_native_context_state(&mut session, &input, limits(), expired).is_err());
    assert!(context::read_native_context_state(&session, limits(), expired).is_err());
    let small = NativeLimits {
        max_wire_bytes: 1,
        ..limits()
    };
    assert!(context::store_native_context_state(&mut session, &input, small, deadline()).is_err());
    assert!(context::read_native_context_state(&session, small, deadline()).is_err());
    assert_eq!(session.snapshot().unwrap(), before);

    // /3 remains readable through its explicit existing API, but contains too
    // little state to initialize the original same-session search semantics.
    let row = observations["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "ordinary")
        .unwrap();
    let old = envelope(&row["raw_records"]);
    let mut legacy = DatasetSession::new();
    context::store_native_records(&mut legacy, &old, limits(), deadline()).unwrap();
    assert!(context::read_native_records(&legacy, limits(), deadline()).is_ok());
    let old_snapshot = legacy.snapshot().unwrap();
    assert!(context::read_native_context_state(&legacy, limits(), deadline()).is_err());
    assert!(
        legacy
            .prepare_native_explorer_index(limits(), text_limits(), deadline())
            .is_err()
    );
    assert_eq!(legacy.snapshot().unwrap(), old_snapshot);

    // Storing raw fields does not claim that Python's projection can consume
    // them. The native state retains falsy metadata; ToDict still raises.
    let mut raw = valid;
    raw["nodes"][0]["metadata"] = json!(false);
    let raw = native_values::encode(&plain(raw), limits()).unwrap();
    let mut invalid_projection = DatasetSession::new();
    context::store_native_context_state(&mut invalid_projection, &raw, limits(), deadline())
        .unwrap();
    let before = invalid_projection.snapshot().unwrap();
    assert!(
        invalid_projection
            .prepare_native_explorer_index(limits(), text_limits(), deadline())
            .is_err()
    );
    assert_eq!(invalid_projection.snapshot().unwrap(), before);
    let retained =
        context::read_native_context_state(&invalid_projection, limits(), deadline()).unwrap();
    assert_eq!(
        root_semantic(&native_values::decode(&retained, limits()).unwrap()),
        root_semantic(&native_values::decode(&raw, limits()).unwrap())
    );
}

#[test]
fn checkpoint_source_bridge_and_profile_boundaries_reject_without_replacing_state() {
    let mut session = ordinary_session();
    let (q, qi, fi) = query_value(&plain(json!("bearing")), &plain(json!({})));
    session
        .search_native_explorer(&q, qi, 20, fi, limits(), text_limits(), deadline())
        .unwrap();
    session
        .checkpoint_native_explorer(false, limits(), text_limits(), deadline())
        .unwrap();
    let before = session.snapshot().unwrap();
    assert!(
        session
            .checkpoint_native_explorer(true, limits(), text_limits(), deadline())
            .unwrap_err()
            .to_string()
            .contains("bridge_mode_changed")
    );
    assert_eq!(session.snapshot().unwrap(), before);
    assert!(session.prepare_explorer_index(deadline()).is_err());
    assert!(semantica_runtime::explorer_mutation::observe(&session).is_err());
    assert!(context::context_summary(&session).is_err());
    assert!(context::read_native_records(&session, limits(), deadline()).is_err());
    assert_eq!(session.snapshot().unwrap(), before);
    let changed = plain(
        json!({"nodes":[{"id":"different","type":"entity","content":"bearing","metadata":{},"properties":{},"valid_from":null,"valid_until":null}],"edges":[]}),
    );
    context::store_native_context_state(
        &mut session,
        &native_values::encode(&changed, limits()).unwrap(),
        limits(),
        deadline(),
    )
    .unwrap();
    let stale = session.snapshot().unwrap();
    for attempt in [
        session.prepare_native_explorer_index(limits(), text_limits(), deadline()),
        session
            .search_native_explorer(&q, qi, 20, fi, limits(), text_limits(), deadline())
            .map(|_| ()),
        session
            .checkpoint_native_explorer(false, limits(), text_limits(), deadline())
            .map(|_| ()),
    ] {
        assert!(attempt.unwrap_err().to_string().contains("state_stale"));
    }
    assert_eq!(session.snapshot().unwrap(), stale);
}

#[test]
fn native_session_deadline_and_resource_refusals_preserve_live_cache_and_checkpoint() {
    let mut session = ordinary_session();
    let (q, qi, fi) = query_value(&plain(json!("bearing")), &plain(json!({})));
    session
        .search_native_explorer(&q, qi, 20, fi, limits(), text_limits(), deadline())
        .unwrap();
    session
        .checkpoint_native_explorer(false, limits(), text_limits(), deadline())
        .unwrap();
    let before = session.snapshot().unwrap();
    let before_index = root_semantic(&observe(&mut session));
    let expired = Instant::now() - Duration::from_secs(1);
    assert!(
        session
            .prepare_native_explorer_index(limits(), text_limits(), expired)
            .is_err()
    );
    assert!(
        session
            .search_native_explorer(&q, qi, 20, fi, limits(), text_limits(), expired)
            .is_err()
    );
    assert!(
        session
            .checkpoint_native_explorer(false, limits(), text_limits(), expired)
            .is_err()
    );
    let small = NativeLimits {
        max_wire_bytes: 1,
        ..limits()
    };
    assert!(
        session
            .prepare_native_explorer_index(small, text_limits(), deadline())
            .is_err()
    );
    assert!(
        session
            .search_native_explorer(&q, qi, 20, fi, small, text_limits(), deadline())
            .is_err()
    );
    assert!(
        session
            .checkpoint_native_explorer(false, small, text_limits(), deadline())
            .is_err()
    );
    assert!(
        session
            .search_native_explorer(
                &q,
                qi,
                20,
                fi,
                limits(),
                TextLimits {
                    max_codepoints: 1,
                    ..text_limits()
                },
                deadline()
            )
            .is_err()
    );
    assert_eq!(session.snapshot().unwrap(), before);
    assert_eq!(root_semantic(&observe(&mut session)), before_index);
    assert!(
        session
            .search_native_explorer(&q, qi, 20, fi, limits(), text_limits(), deadline())
            .unwrap()
            .cache_hit
    );
}

#[test]
fn native_session_shares_the_existing_query_cache_allowance() {
    let mut session = DatasetSession::new();
    let records = plain(
        json!({"nodes":[{"id":"a","type":"entity","content":"bearing","metadata":{},"properties":{"content":"bearing","extra":"x".repeat(4096)},"valid_from":null,"valid_until":null}],"edges":[]}),
    );
    let input = native_values::encode(&records, limits()).unwrap();
    context::store_native_context_state(&mut session, &input, limits(), deadline()).unwrap();
    let literal = "x".repeat(session.dataset_byte_limit() / 16 - 512);
    let rdf = format!("<urn:cache> <urn:value> \"{literal}\" <urn:cache-budget> .\n");
    session
        .load(rdf.as_bytes(), oxigraph::io::RdfFormat::NQuads, None, None)
        .unwrap();
    session
        .prepare_native_explorer_index(limits(), text_limits(), deadline())
        .unwrap();
    let mut used = 0;
    let mut queries = vec![];
    for i in 0..16 {
        let query = format!(
            "SELECT ?value WHERE {{ GRAPH <urn:cache-budget> {{ <urn:cache> <urn:value> ?value }} }} # {i}"
        );
        let result = session.query(&query, true).unwrap();
        assert!(!result.cache_hit);
        used += query.len() + serde_json::to_vec(&result.result).unwrap().len();
        queries.push(query);
    }
    assert!(used < session.dataset_byte_limit());
    assert!(session.dataset_byte_limit() - used < records.encode(limits()).unwrap().len());
    let before = session.snapshot().unwrap();
    assert!(
        session
            .prepare_native_explorer_index(limits(), text_limits(), deadline())
            .is_err()
    );
    assert_eq!(session.snapshot().unwrap(), before);
    assert!(
        session.query(&queries[0], true).unwrap().cache_hit,
        "a native refusal cannot evict the existing SPARQL cache"
    );
    let actual = context::read_native_context_state(&session, limits(), deadline()).unwrap();
    assert_eq!(
        root_semantic(&native_values::decode(&actual, limits()).unwrap()),
        root_semantic(&records)
    );
}

fn subtree(nodes: &[NativeNode], root: usize) -> NativeValue {
    fn children(node: &NativeNode) -> Vec<usize> {
        match node {
            NativeNode::List(v)
            | NativeNode::Tuple(v)
            | NativeNode::Set(v)
            | NativeNode::FrozenSet(v) => v.clone(),
            NativeNode::Dict(v) => v.iter().flat_map(|(k, v)| [*k, *v]).collect(),
            _ => vec![],
        }
    }
    let mut reachable = vec![false; nodes.len()];
    reachable[root] = true;
    for i in (0..nodes.len()).rev() {
        if reachable[i] {
            for child in children(&nodes[i]) {
                assert!(child < i);
                reachable[child] = true;
            }
        }
    }
    let mut mapped = vec![usize::MAX; nodes.len()];
    let mut out = vec![];
    for (i, node) in nodes.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        let remap = |v: &[usize]| v.iter().map(|&id| mapped[id]).collect();
        let node = match node {
            NativeNode::List(v) => NativeNode::List(remap(v)),
            NativeNode::Tuple(v) => NativeNode::Tuple(remap(v)),
            NativeNode::Set(v) => NativeNode::Set(remap(v)),
            NativeNode::FrozenSet(v) => NativeNode::FrozenSet(remap(v)),
            NativeNode::Dict(v) => {
                NativeNode::Dict(v.iter().map(|&(k, v)| (mapped[k], mapped[v])).collect())
            }
            v => v.clone(),
        };
        mapped[i] = out.len();
        out.push(node);
    }
    NativeValue::new(out, mapped[root], limits()).unwrap()
}
fn selected(value: &NativeValue, name: &str) -> NativeValue {
    subtree(value.nodes(), field(value, value.root(), name))
}

// Convert the frozen original cache-key representation to the explicit native
// internal key profile. This case's keys are only (query,20,empty filters);
// no query is rerun or result recomputed to fabricate a persisted original state.
fn frozen_index(original: &NativeValue) -> SemanticNativeValueEnvelope {
    let index = selected(original, "index");
    let mut nodes = index.nodes().to_vec();
    let NativeNode::Dict(mut entries) = nodes[index.root()].clone() else {
        panic!("original index");
    };
    let NativeNode::List(keys) =
        &original.nodes()[field(original, original.root(), "original_cache_keys")]
    else {
        panic!("original keys");
    };
    let mut ids = vec![];
    for &key in keys {
        let NativeNode::Tuple(key) = &original.nodes()[key] else {
            panic!("original key tuple");
        };
        assert_eq!(key.len(), 3);
        let NativeNode::Text(query) = &original.nodes()[key[0]] else {
            panic!("original query");
        };
        let NativeNode::Integer(limit) = &original.nodes()[key[1]] else {
            panic!("original limit");
        };
        let NativeNode::Tuple(filters) = &original.nodes()[key[2]] else {
            panic!("original filters");
        };
        assert!(filters.is_empty());
        assert_eq!(limit, "20");
        let text = serde_json::to_string(&json!([query, 20, []])).unwrap();
        ids.push(nodes.len());
        nodes.push(NativeNode::Text(text.chars().map(u32::from).collect()));
    }
    let key_list = nodes.len();
    nodes.push(NativeNode::List(ids));
    fn text(nodes: &mut Vec<NativeNode>, value: &str) -> usize {
        let id = nodes.len();
        nodes.push(NativeNode::Text(value.chars().map(u32::from).collect()));
        id
    }
    let key = text(&mut nodes, "cache_keys");
    entries.push((key, key_list));
    let index_id = nodes.len();
    nodes.push(NativeNode::Dict(entries));
    let schema = text(&mut nodes, "semantica.explorer-index/2");
    let schema_key = text(&mut nodes, "schema");
    let index_key = text(&mut nodes, "index");
    let root = nodes.len();
    nodes.push(NativeNode::Dict(vec![
        (schema_key, schema),
        (index_key, index_id),
    ]));
    let result = native_values::encode(&subtree(&nodes, root), limits()).unwrap();
    let restored = semantica_runtime::explorer_search_index::GraphSearchIndex::restore_native(
        &result,
        limits(),
        deadline(),
    )
    .unwrap();
    let actual = native_values::decode(
        &restored.observe_native(limits(), deadline()).unwrap(),
        limits(),
    )
    .unwrap();
    assert_eq!(
        unordered(&actual, actual.root()),
        unordered(&index, index.root()),
        "native fixture transport retains the exact frozen original index"
    );
    result
}

#[test]
fn original_failed_result_normalization_preserves_completed_index_search_cache_changes() {
    let plan: Value = serde_json::from_slice(include_bytes!(
        "fixtures/native_session_failed_search_plan.json"
    ))
    .unwrap();
    let observations: Value = serde_json::from_slice(include_bytes!(
        "fixtures/native_session_failed_search_observations.json"
    ))
    .unwrap();
    let rows = observations["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0]["result"]["status"], "returned");
    assert_eq!(rows[1]["method"], "add_node");
    assert_eq!(rows[1]["result"]["class"], "ValueError");
    assert!(rows[1]["index_calls"].as_array().unwrap().is_empty());
    let failed_mutation = native(&rows[1]["state_after"]);
    let observed_state = state_observations();
    let records = native(&observed_state["original02_partial_state"]["native_context_state"]);
    let mut session = DatasetSession::new();
    context::store_native_context_state(
        &mut session,
        &native_values::encode(&records, limits()).unwrap(),
        limits(),
        deadline(),
    )
    .unwrap();
    let revision = field(&failed_mutation, failed_mutation.root(), "graph_revision");
    let NativeNode::Integer(revision) = &failed_mutation.nodes()[revision] else {
        panic!("original graph revision");
    };
    assert_eq!(revision, "1");
    // Restore an observed original partial state. Current typed add_* support is
    // deliberately not inferred from this read/checkpoint continuation test.
    let saved = SemanticExplorerNativeState {
        schema: "semantica.explorer-mutation-state/2".into(),
        context_sha256: semantica_runtime::sha256(records.encode(limits()).unwrap()),
        bridge: false,
        graph_revision: 1,
        node_type_index: native_values::encode(
            &selected(&failed_mutation, "node_type_index"),
            limits(),
        )
        .unwrap(),
        index: frozen_index(&failed_mutation),
    };
    let literal = serde_json::to_string(&serde_json::to_string(&saved).unwrap()).unwrap();
    session.update(&format!("INSERT DATA {{ GRAPH <urn:semantica:explorer:mutation-state/1> {{ <urn:semantica:explorer:mutation-state> <urn:semantica:explorer:mutation-value> {literal} }} }}")).unwrap();
    index_matches(
        &mut session,
        &rows[1]["state_after"],
        "restored failed original mutation",
    );
    for i in 2..4 {
        let args = native(&plan["calls"][i]["args"]);
        let NativeNode::List(ids) = &args.nodes()[args.root()] else {
            panic!("original query args");
        };
        let query = subtree(args.nodes(), ids[0]);
        let (v, q, f) = query_value(&query, &plain(json!({})));
        let before = session.snapshot().unwrap();
        let error = session
            .search_native_explorer(&v, q, 20, f, limits(), text_limits(), deadline())
            .unwrap_err();
        assert_eq!(error_class(&error), "ValueError");
        assert_eq!(rows[i]["result"]["class"], "ValueError");
        assert_eq!(
            rows[i]["index_calls"][0]["diagnostics"]["cache_hit"],
            i == 3
        );
        assert_eq!(
            session.snapshot().unwrap(),
            before,
            "failed read changes cache only"
        );
        index_matches(
            &mut session,
            &rows[i]["state_after"],
            &format!("failed original search {i}"),
        );
        let checkpoint = session
            .checkpoint_native_explorer(false, limits(), text_limits(), deadline())
            .unwrap();
        assert_eq!(checkpoint.graph_revision, 1);
        session = reload(&mut session);
        index_matches(
            &mut session,
            &rows[i]["state_after"],
            &format!("reloaded failed search {i}"),
        );
    }
}
