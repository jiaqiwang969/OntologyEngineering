//! Differential observations from the real original GraphSearchIndex, retained
//! before this adapter was implemented. These are development-exposed cases.
use semantica_contracts::SemanticNativeValueEnvelope;
use semantica_json::{
    native_operations::TextLimits,
    native_value::{NativeLimits, NativeNode, NativeValue},
};
use semantica_runtime::{explorer_search_index::GraphSearchIndex, native_values};
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
fn expired() -> Instant {
    Instant::now() - Duration::from_secs(1)
}
fn fixture() -> (Value, Value) {
    (
        serde_json::from_slice(include_bytes!("fixtures/native_index_plan.json")).unwrap(),
        serde_json::from_slice(include_bytes!("fixtures/native_index_observations.json")).unwrap(),
    )
}
fn native(value: &Value) -> NativeValue {
    NativeValue::from_envelope(value, limits()).unwrap()
}
fn plain(value: Value) -> NativeValue {
    NativeValue::from_plain_json(&value, limits()).unwrap()
}

// Compare typed values independently of node allocation and mapping entry order.
// Ordered sequences, tuple/list distinction, all codepoints and IEEE bits remain
// significant. The source projection itself sorts the logical posting sets.
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
        NativeNode::Bool(b) => json!(["bool", b]),
        NativeNode::Integer(s) => json!(["integer", s]),
        NativeNode::FloatBits(bits) => json!(["float64", format!("{bits:016x}")]),
        NativeNode::Text(points) => json!(["text", points]),
        NativeNode::List(ids) => sequence("list", ids),
        NativeNode::Tuple(ids) => sequence("tuple", ids),
        NativeNode::Set(ids) => sequence("set", ids),
        NativeNode::FrozenSet(ids) => sequence("frozenset", ids),
        NativeNode::Dict(entries) => {
            let mut entries: Vec<_> = entries
                .iter()
                .map(|&(k, v)| json!([semantic(value, k), semantic(value, v)]))
                .collect();
            entries.sort_by_cached_key(|pair| serde_json::to_string(&pair[0]).unwrap());
            json!(["dict", entries])
        }
    }
}
fn root_semantic(value: &NativeValue) -> Value {
    semantic(value, value.root())
}
fn envelope_semantic(value: &SemanticNativeValueEnvelope) -> Value {
    root_semantic(&native_values::decode(value, limits()).unwrap())
}
fn state(index: &GraphSearchIndex) -> Value {
    envelope_semantic(&index.observe_native(limits(), deadline()).unwrap())
}
fn expect_state(index: &GraphSearchIndex, expected: &Value, label: &str) {
    assert_eq!(state(index), root_semantic(&native(expected)), "{label}");
}
fn roundtrip(index: &mut GraphSearchIndex, label: &str) {
    let before = state(index);
    let saved = index.store_native(limits(), deadline()).unwrap();
    *index = GraphSearchIndex::restore_native(&saved, limits(), deadline()).unwrap();
    assert_eq!(state(index), before, "persistence: {label}");
}

// Put the separately captured query and filters in one carrier without any
// conversion through plain JSON (which would lose surrogate and NaN values).
fn query_value(query: &NativeValue, filters: &NativeValue) -> (NativeValue, usize, usize) {
    let mut nodes = query.nodes().to_vec();
    let shift = nodes.len();
    for node in filters.nodes() {
        nodes.push(match node {
            NativeNode::List(ids) => NativeNode::List(ids.iter().map(|id| id + shift).collect()),
            NativeNode::Tuple(ids) => NativeNode::Tuple(ids.iter().map(|id| id + shift).collect()),
            NativeNode::Set(ids) => NativeNode::Set(ids.iter().map(|id| id + shift).collect()),
            NativeNode::FrozenSet(ids) => {
                NativeNode::FrozenSet(ids.iter().map(|id| id + shift).collect())
            }
            NativeNode::Dict(entries) => NativeNode::Dict(
                entries
                    .iter()
                    .map(|(k, v)| (k + shift, v + shift))
                    .collect(),
            ),
            other => other.clone(),
        });
    }
    let query_id = query.root();
    let filters_id = filters.root() + shift;
    let root = nodes.len();
    nodes.push(NativeNode::Tuple(vec![query_id, filters_id]));
    (
        NativeValue::new(nodes, root, limits()).unwrap(),
        query_id,
        filters_id,
    )
}
fn expected_matches(envelope: &Value) -> Vec<(Vec<u32>, u64)> {
    let value = native(envelope);
    let NativeNode::List(ids) = &value.nodes()[value.root()] else {
        panic!("original matches must be list");
    };
    ids.iter()
        .map(|&id| {
            let NativeNode::Tuple(pair) = &value.nodes()[id] else {
                panic!("original match must be tuple");
            };
            assert_eq!(pair.len(), 2);
            let NativeNode::Text(id) = &value.nodes()[pair[0]] else {
                panic!("original match ID must be text");
            };
            let NativeNode::FloatBits(score) = value.nodes()[pair[1]] else {
                panic!("original score must be float");
            };
            (id.clone(), score)
        })
        .collect()
}
fn check_search(
    index: &mut GraphSearchIndex,
    query: &Value,
    filters: &Value,
    limit: usize,
    expected: &Value,
    label: &str,
) {
    let (value, query_id, filters_id) = query_value(&native(query), &native(filters));
    let actual = index
        .search_native(
            &value,
            query_id,
            limit,
            filters_id,
            text_limits(),
            deadline(),
        )
        .unwrap_or_else(|e| panic!("{label}: {e}"));
    assert_eq!(
        actual
            .matches
            .iter()
            .map(|(id, score)| (id.clone(), score.to_bits()))
            .collect::<Vec<_>>(),
        expected_matches(&expected["matches"]),
        "matches: {label}"
    );
    assert_eq!(
        json!({"cache_hit":actual.cache_hit, "path":actual.path, "candidates":actual.candidates}),
        expected["diagnostics"],
        "diagnostics: {label}"
    );
}

#[test]
fn original_13_mutations_303_searches_preserve_typed_state_and_restored_lru() {
    let (plan, observations) = fixture();
    let steps = plan["steps"].as_array().unwrap();
    let rows = observations["rows"].as_array().unwrap();
    let queries = plan["queries"].as_array().unwrap();
    assert_eq!(steps.len(), 13);
    assert_eq!(rows.len(), steps.len());
    assert_eq!(queries.len(), 13);
    let mut index = GraphSearchIndex::new_native();
    let mut search_count = 0;
    for (i, (step, row)) in steps.iter().zip(rows).enumerate() {
        assert_eq!(row["step"], i);
        let value = native(&step["input"]);
        let result = match step["op"].as_str().unwrap() {
            "upsert" => index.upsert_native_original_partial(
                &value,
                value.root(),
                text_limits(),
                deadline(),
            ),
            "rebuild" => {
                let NativeNode::List(ids) = &value.nodes()[value.root()] else {
                    panic!("rebuild fixture");
                };
                index.rebuild_native_original_partial(&value, ids, text_limits(), deadline())
            }
            "remove" => {
                let NativeNode::Text(id) = &value.nodes()[value.root()] else {
                    panic!("remove fixture");
                };
                index.remove_native(id, deadline())
            }
            op => panic!("unknown original operation {op}"),
        };
        match row["result"]["status"].as_str().unwrap() {
            "returned" => result.unwrap_or_else(|e| panic!("step {i}: {e}")),
            "raised" => {
                let expected = match row["result"]["class"].as_str().unwrap() {
                    "TypeError" => "explorer_search_tags_not_iterable",
                    "OverflowError" => "explorer_search_integer_float_overflow",
                    name => panic!("unmapped original error {name}"),
                };
                assert_eq!(result.unwrap_err().to_string(), expected, "step {i}");
            }
            status => panic!("unknown original status {status}"),
        }
        let label = format!("step {i} after mutation");
        expect_state(&index, &row["state_after_mutation"], &label);
        // In particular, steps 5 and 9 restore caches that still contain an ID
        // removed before an original field error. Subsequent queries test them.
        roundtrip(&mut index, &label);
        let searches = row["searches"].as_array().unwrap();
        assert_eq!(searches.len(), queries.len());
        for (j, (query, expected)) in queries.iter().zip(searches).enumerate() {
            assert_eq!(expected["status"], "returned");
            check_search(
                &mut index,
                &query["query"],
                &query["filters"],
                query["limit"].as_u64().unwrap() as usize,
                expected,
                &format!("step {i} query {j}"),
            );
            search_count += 1;
        }
        let label = format!("step {i} after searches");
        expect_state(&index, &row["state_after_searches"], &label);
        roundtrip(&mut index, &label);
    }
    let empty = plain(json!({})).to_envelope(limits()).unwrap();
    let eviction = observations["eviction"].as_array().unwrap();
    assert_eq!(eviction.len(), 134);
    assert_eq!(plan["eviction_queries"], 130);
    for (i, row) in eviction.iter().enumerate() {
        let query = plain(row["query"].clone()).to_envelope(limits()).unwrap();
        check_search(
            &mut index,
            &query,
            &empty,
            20,
            row,
            &format!("eviction query {i}"),
        );
        roundtrip(&mut index, &format!("eviction query {i}"));
        assert!(index.cache_len() <= 128);
        search_count += 1;
    }
    assert_eq!(search_count, 303);
    expect_state(&index, &observations["state_final"], "final");
}

fn populated() -> GraphSearchIndex {
    let (plan, _) = fixture();
    let mut index = GraphSearchIndex::new_native();
    let node = native(&plan["steps"][0]["input"]);
    index
        .upsert_native_original_partial(&node, node.root(), text_limits(), deadline())
        .unwrap();
    let query = &plan["queries"][0];
    let (v, q, f) = query_value(&native(&query["query"]), &native(&query["filters"]));
    index
        .search_native(&v, q, 20, f, text_limits(), deadline())
        .unwrap();
    assert_eq!(index.cache_len(), 1);
    index
}

#[test]
fn native_deadline_and_text_limit_refusals_leave_documents_postings_and_cache_unchanged() {
    let (plan, _) = fixture();
    let mut index = populated();
    let before = state(&index);
    let bad_tags = native(&plan["steps"][5]["input"]);
    let replacement = plain(json!({"id":"a","content":"replacement with additional text"}));
    let (query, q, f) = query_value(&plain(json!("bearing")), &plain(json!({})));
    assert!(
        index
            .upsert_native_original_partial(&bad_tags, bad_tags.root(), text_limits(), expired())
            .is_err()
    );
    assert_eq!(state(&index), before);
    assert!(
        index
            .rebuild_native_original_partial(
                &bad_tags,
                &[bad_tags.root()],
                text_limits(),
                expired()
            )
            .is_err()
    );
    assert_eq!(state(&index), before);
    assert!(index.remove_native(&[97], expired()).is_err());
    assert_eq!(state(&index), before);
    assert!(
        index
            .search_native(&query, q, 20, f, text_limits(), expired())
            .is_err()
    );
    assert_eq!(state(&index), before);
    let tiny = TextLimits {
        max_codepoints: 1,
        ..text_limits()
    };
    assert!(
        index
            .upsert_native_original_partial(&replacement, replacement.root(), tiny, deadline())
            .is_err()
    );
    assert_eq!(state(&index), before);
    assert!(
        index
            .rebuild_native_original_partial(&replacement, &[replacement.root()], tiny, deadline())
            .is_err()
    );
    assert_eq!(state(&index), before);
    assert!(
        index
            .search_native(&query, q, 20, f, tiny, deadline())
            .is_err()
    );
    assert_eq!(state(&index), before);
    assert!(
        index
            .upsert_native_original_partial(&replacement, usize::MAX, text_limits(), deadline())
            .is_err()
    );
    assert_eq!(state(&index), before);
}

#[test]
fn native_wire_checks_limits_deadlines_and_invalid_carrier_before_restoration() {
    let index = populated();
    let before = state(&index);
    let saved = index.store_native(limits(), deadline()).unwrap();
    assert!(index.store_native(limits(), expired()).is_err());
    assert!(index.observe_native(limits(), expired()).is_err());
    assert!(GraphSearchIndex::restore_native(&saved, limits(), expired()).is_err());
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
            max_depth: 1,
            ..limits()
        },
        NativeLimits {
            max_wire_bytes: 1,
            ..limits()
        },
    ] {
        assert!(index.store_native(small, deadline()).is_err());
        assert!(index.observe_native(small, deadline()).is_err());
        assert!(GraphSearchIndex::restore_native(&saved, small, deadline()).is_err());
    }
    let mut invalid = saved.clone();
    invalid.schema = "semantica.native-value/unknown".into();
    assert!(GraphSearchIndex::restore_native(&invalid, limits(), deadline()).is_err());
    let mut invalid = saved.clone();
    invalid.root = u64::MAX;
    assert!(GraphSearchIndex::restore_native(&invalid, limits(), deadline()).is_err());
    let mut invalid = saved.clone();
    let text = invalid
        .nodes
        .iter_mut()
        .find(|n| n["kind"] == "text")
        .unwrap();
    text["codepoints"] = json!([0x110000]);
    assert!(GraphSearchIndex::restore_native(&invalid, limits(), deadline()).is_err());
    assert_eq!(state(&index), before);
}

#[test]
fn native_and_legacy_entrypoints_do_not_mix_profiles_or_drop_typed_data() {
    let mut index = populated();
    let before = state(&index);
    assert!(serde_json::to_value(&index).is_err());
    assert!(index.upsert(&json!({"id":"b"}), deadline()).is_err());
    assert!(
        index
            .upsert_original_partial(&json!({"id":"b"}), deadline())
            .is_err()
    );
    assert!(index.rebuild(&[], deadline()).is_err());
    assert!(index.rebuild_original_partial(&[], deadline()).is_err());
    assert!(index.remove("a", deadline()).is_err());
    assert!(index.search("bearing", 20, &json!({}), deadline()).is_err());
    assert_eq!(state(&index), before);
    let mut legacy = GraphSearchIndex::default();
    let node = plain(json!({"id":"b"}));
    assert!(
        legacy
            .upsert_native_original_partial(&node, node.root(), text_limits(), deadline())
            .is_err()
    );
    assert!(
        legacy
            .rebuild_native_original_partial(&node, &[node.root()], text_limits(), deadline())
            .is_err()
    );
    assert!(legacy.remove_native(&[98], deadline()).is_err());
    let (query, q, f) = query_value(&plain(json!("bearing")), &plain(json!({})));
    assert!(
        legacy
            .search_native(&query, q, 20, f, text_limits(), deadline())
            .is_err()
    );
    assert!(legacy.store_native(limits(), deadline()).is_err());
    assert!(legacy.observe_native(limits(), deadline()).is_err());
    index.clear();
    assert!(index.is_empty());
    assert_eq!(index.cache_len(), 0);
    assert!(index.store_native(limits(), deadline()).is_ok());
    assert!(serde_json::to_value(&index).is_err());
}

fn field(nodes: &[NativeNode], id: usize, name: &str) -> usize {
    let NativeNode::Dict(entries) = &nodes[id] else {
        panic!("field {name} requires a dictionary");
    };
    let key: Vec<_> = name.chars().map(u32::from).collect();
    entries
        .iter()
        .find_map(|&(k, v)| match &nodes[k] {
            NativeNode::Text(points) if points == &key => Some(v),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing field {name}"))
}
fn children(node: &NativeNode) -> Vec<usize> {
    match node {
        NativeNode::List(ids)
        | NativeNode::Tuple(ids)
        | NativeNode::Set(ids)
        | NativeNode::FrozenSet(ids) => ids.clone(),
        NativeNode::Dict(entries) => entries.iter().flat_map(|(k, v)| [*k, *v]).collect(),
        _ => vec![],
    }
}

// Deleting/replacing a reference may leave old carrier nodes unreachable.
// Prune and remap those nodes so each counterexample remains a valid native
// carrier. Rejection must then come from the stored-index contract itself.
fn repack(nodes: Vec<NativeNode>, root: usize) -> SemanticNativeValueEnvelope {
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
    for (i, node) in nodes.into_iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        let remap = |ids: Vec<usize>| ids.into_iter().map(|id| mapped[id]).collect();
        let node = match node {
            NativeNode::List(ids) => NativeNode::List(remap(ids)),
            NativeNode::Tuple(ids) => NativeNode::Tuple(remap(ids)),
            NativeNode::Set(ids) => NativeNode::Set(remap(ids)),
            NativeNode::FrozenSet(ids) => NativeNode::FrozenSet(remap(ids)),
            NativeNode::Dict(entries) => NativeNode::Dict(
                entries
                    .into_iter()
                    .map(|(k, v)| (mapped[k], mapped[v]))
                    .collect(),
            ),
            other => other,
        };
        mapped[i] = out.len();
        out.push(node);
    }
    native_values::encode(
        &NativeValue::new(out, mapped[root], limits()).unwrap(),
        limits(),
    )
    .unwrap()
}

fn corrupt_index(saved: &SemanticNativeValueEnvelope, change: &str) -> SemanticNativeValueEnvelope {
    let value = native_values::decode(saved, limits()).unwrap();
    let mut nodes = value.nodes().to_vec();
    let index = field(&nodes, value.root(), "index");
    match change {
        "internal_schema" => {
            let id = field(&nodes, value.root(), "schema");
            nodes[id] = NativeNode::Text(
                "semantica.explorer-index/unknown"
                    .chars()
                    .map(u32::from)
                    .collect(),
            );
        }
        "posting_unknown_id" => {
            let exact = field(&nodes, index, "exact");
            let NativeNode::Dict(entries) = &nodes[exact] else {
                panic!("exact postings");
            };
            let NativeNode::List(ids) = &nodes[entries[0].1] else {
                panic!("posting bucket");
            };
            let id = ids[0];
            nodes[id] = NativeNode::Text("missing-document".chars().map(u32::from).collect());
            let documents = field(&nodes, index, "documents");
            let NativeNode::Dict(documents) = &nodes[documents] else {
                panic!("documents");
            };
            assert!(
                documents.iter().all(|(key, _)| *key != id),
                "counterexample must not modify a document identity"
            );
        }
        "cache_nan_score"
        | "cache_infinite_score"
        | "cache_zero_score"
        | "cache_empty_id"
        | "cache_duplicate_ids" => {
            let cache = field(&nodes, index, "cache_results");
            let NativeNode::List(rows) = &nodes[cache] else {
                panic!("cache rows");
            };
            let NativeNode::List(hits) = &nodes[rows[0]] else {
                panic!("cache hits");
            };
            let first = hits[0];
            let second = *hits.get(1).unwrap();
            let NativeNode::Tuple(pair) = &nodes[first] else {
                panic!("cache match");
            };
            let id = pair[0];
            let score = pair[1];
            match change {
                "cache_empty_id" => nodes[id] = NativeNode::Text(vec![]),
                "cache_duplicate_ids" => {
                    let NativeNode::Tuple(pair) = &mut nodes[second] else {
                        panic!("second cache match");
                    };
                    pair[0] = id;
                }
                _ => {
                    nodes[score] = NativeNode::FloatBits(match change {
                        "cache_nan_score" => f64::NAN.to_bits(),
                        "cache_infinite_score" => f64::INFINITY.to_bits(),
                        "cache_zero_score" => 0.0f64.to_bits(),
                        _ => unreachable!(),
                    })
                }
            }
        }
        "cache_missing_key" | "cache_duplicate_keys" => {
            let keys = field(&nodes, index, "cache_keys");
            let NativeNode::List(ids) = &mut nodes[keys] else {
                panic!("cache keys");
            };
            assert!(ids.len() >= 2);
            if change == "cache_missing_key" {
                ids.pop();
            } else {
                ids[1] = ids[0];
            }
        }
        "ordered_ids_reversed" => {
            let ordered = field(&nodes, index, "ordered_ids");
            let NativeNode::List(ids) = &mut nodes[ordered] else {
                panic!("ordered IDs");
            };
            assert!(ids.len() >= 2);
            ids.reverse();
        }
        "cache_hits_exceed_key_limit" => {
            let keys = field(&nodes, index, "cache_keys");
            let NativeNode::List(ids) = &nodes[keys] else {
                panic!("cache keys");
            };
            let key = ids[0];
            let NativeNode::Text(points) = &nodes[key] else {
                panic!("cache key");
            };
            let text: String = points
                .iter()
                .map(|&cp| char::from_u32(cp).unwrap())
                .collect();
            let mut key_value: Value = serde_json::from_str(&text).unwrap();
            key_value[1] = json!(0);
            nodes[key] = NativeNode::Text(
                serde_json::to_string(&key_value)
                    .unwrap()
                    .chars()
                    .map(u32::from)
                    .collect(),
            );
        }
        "primary_tokens_disagree" | "normalized_id_disagrees" => {
            let documents = field(&nodes, index, "documents");
            let NativeNode::Dict(documents) = &nodes[documents] else {
                panic!("documents");
            };
            let doc = documents[0].1;
            let name = if change == "primary_tokens_disagree" {
                "primary_text"
            } else {
                "normalized_id"
            };
            let id = field(&nodes, doc, name);
            nodes[id] = NativeNode::Text("inconsistent-text".chars().map(u32::from).collect());
        }
        other => panic!("unknown corruption {other}"),
    }
    repack(nodes, value.root())
}

#[test]
fn valid_native_carriers_cannot_restore_inconsistent_postings_order_or_cache() {
    let (plan, _) = fixture();
    let mut index = GraphSearchIndex::new_native();
    for step in &plan["steps"].as_array().unwrap()[..3] {
        let value = native(&step["input"]);
        index
            .upsert_native_original_partial(&value, value.root(), text_limits(), deadline())
            .unwrap();
    }
    for query in &plan["queries"].as_array().unwrap()[..2] {
        let (v, q, f) = query_value(&native(&query["query"]), &native(&query["filters"]));
        index
            .search_native(&v, q, 20, f, text_limits(), deadline())
            .unwrap();
    }
    assert_eq!(index.len(), 3);
    assert_eq!(index.cache_len(), 2);
    let before = state(&index);
    let saved = index.store_native(limits(), deadline()).unwrap();
    // The unmodified input includes NaN/+infinity document confidence and a
    // surrogate ID. They are legal index values; only the defects below reject.
    let restored = GraphSearchIndex::restore_native(&saved, limits(), deadline()).unwrap();
    assert_eq!(state(&restored), before);
    for change in [
        "internal_schema",
        "posting_unknown_id",
        "cache_nan_score",
        "cache_infinite_score",
        "cache_zero_score",
        "cache_empty_id",
        "cache_duplicate_ids",
        "cache_missing_key",
        "cache_duplicate_keys",
        "ordered_ids_reversed",
        "cache_hits_exceed_key_limit",
        "primary_tokens_disagree",
        "normalized_id_disagrees",
    ] {
        let invalid = corrupt_index(&saved, change);
        assert!(
            native_values::decode(&invalid, limits()).is_ok(),
            "carrier validity: {change}"
        );
        assert!(
            GraphSearchIndex::restore_native(&invalid, limits(), deadline()).is_err(),
            "index validation: {change}"
        );
    }
    assert_eq!(state(&index), before);
}

// Sharing equal Text nodes changes representation only. All document fields,
// posting contents and ordering retain exactly the stored values.
fn share_text_nodes(saved: &SemanticNativeValueEnvelope) -> SemanticNativeValueEnvelope {
    let value = native_values::decode(saved, limits()).unwrap();
    let mut mapped = vec![usize::MAX; value.nodes().len()];
    let mut text_ids = std::collections::BTreeMap::<Vec<u32>, usize>::new();
    let mut out = vec![];
    for (id, node) in value.nodes().iter().enumerate() {
        if let NativeNode::Text(points) = node {
            if let Some(&shared) = text_ids.get(points) {
                mapped[id] = shared;
                continue;
            }
        }
        let remap = |ids: &[usize]| ids.iter().map(|&id| mapped[id]).collect();
        let node = match node {
            NativeNode::List(ids) => NativeNode::List(remap(ids)),
            NativeNode::Tuple(ids) => NativeNode::Tuple(remap(ids)),
            NativeNode::Set(ids) => NativeNode::Set(remap(ids)),
            NativeNode::FrozenSet(ids) => NativeNode::FrozenSet(remap(ids)),
            NativeNode::Dict(entries) => NativeNode::Dict(
                entries
                    .iter()
                    .map(|&(k, v)| (mapped[k], mapped[v]))
                    .collect(),
            ),
            other => other.clone(),
        };
        mapped[id] = out.len();
        if let NativeNode::Text(points) = &node {
            text_ids.insert(points.clone(), out.len());
        }
        out.push(node);
    }
    native_values::encode(
        &NativeValue::new(out, mapped[value.root()], limits()).unwrap(),
        limits(),
    )
    .unwrap()
}

fn scalar_bytes(value: &NativeValue) -> usize {
    value
        .nodes()
        .iter()
        .map(|node| match node {
            NativeNode::None => 0,
            NativeNode::Bool(_) => 1,
            NativeNode::Integer(s) => s.len(),
            NativeNode::FloatBits(_) => 8,
            NativeNode::Text(points) => points.len() * 4,
            _ => 0,
        })
        .sum()
}

#[test]
fn shared_long_text_is_charged_before_index_materialization() {
    let mut index = GraphSearchIndex::new_native();
    for i in 0..16 {
        let node = plain(
            json!({"id":format!("n{i:02}"),"content":"bearing","properties":{"description":"x".repeat(4096)}}),
        );
        index
            .upsert_native_original_partial(&node, node.root(), text_limits(), deadline())
            .unwrap();
    }
    let before = state(&index);
    let saved = index.store_native(limits(), deadline()).unwrap();
    let shared = share_text_nodes(&saved);
    assert_eq!(envelope_semantic(&shared), envelope_semantic(&saved));
    let carrier = native_values::decode(&shared, limits()).unwrap();
    let tight = NativeLimits {
        max_scalar_bytes: scalar_bytes(&carrier) + 32,
        ..limits()
    };
    assert!(tight.max_scalar_bytes < 4096 * 4 * index.len());
    assert!(
        native_values::decode(&shared, tight).is_ok(),
        "shared input carrier fits the host scalar budget"
    );
    let restored = GraphSearchIndex::restore_native(&shared, limits(), deadline()).unwrap();
    assert_eq!(
        state(&restored),
        before,
        "ample budget accepts unchanged shared values"
    );
    let error = match GraphSearchIndex::restore_native(&shared, tight, deadline()) {
        Ok(_) => panic!("restoring shared strings must charge their materialized copies"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("scalar"),
        "materialization budget: {error}"
    );
    assert_eq!(state(&index), before);
}

#[test]
fn fragment_and_frozenset_representation_depth_share_the_same_host_limit() {
    // aliases = [[frozenset({(('pin',),)})]]: the fragment traversal reaches
    // frozenset at depth 2 and its text leaf at total depth 5. Repr conversion
    // must carry that depth forward instead of granting a fresh depth budget.
    let t = |s: &str| NativeNode::Text(s.chars().map(u32::from).collect());
    let value = NativeValue::new(
        vec![
            t("a"),
            t("id"),
            t("aliases"),
            t("pin"),
            NativeNode::Tuple(vec![3]),
            NativeNode::Tuple(vec![4]),
            NativeNode::FrozenSet(vec![5]),
            NativeNode::List(vec![6]),
            NativeNode::List(vec![7]),
            NativeNode::Dict(vec![(2, 8)]),
            t("properties"),
            NativeNode::Dict(vec![(1, 0), (10, 9)]),
        ],
        11,
        limits(),
    )
    .unwrap();
    let mut index = populated();
    let before = state(&index);
    let error = index
        .upsert_native_original_partial(
            &value,
            value.root(),
            TextLimits {
                max_depth: 4,
                ..text_limits()
            },
            deadline(),
        )
        .unwrap_err();
    assert!(
        error.to_string().contains("depth"),
        "cumulative depth refusal: {error}"
    );
    assert_eq!(
        state(&index),
        before,
        "host refusal must preserve the previous same-ID document and cache"
    );
    index
        .upsert_native_original_partial(
            &value,
            value.root(),
            TextLimits {
                max_depth: 5,
                ..text_limits()
            },
            deadline(),
        )
        .unwrap();
    assert_eq!(index.len(), 1);
    assert_eq!(index.cache_len(), 0);
}
