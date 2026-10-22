//! Frozen original operations and separately observed state, no expected-state injection.
use super::super::{
    DatasetSession, RdfFormat,
    source_failure::SourceFailure,
    source_observation,
    source_storage::{SourceSnapshotInput, source_snapshot_argument},
    source_update_loader::SourceUpdateBytesLoader,
};
use super::*;
use semantica_contracts::{SemanticSourceSerializeRequest, SemanticSourceSnapshotArgument};
use serde_json::{Value, json};
const BUDGET: usize = 4 * 1024 * 1024;

fn captured(result: Result<Value>) -> Value {
    match result {
        Ok(value) => json!({"status":"completed","value":value}),
        Err(e) => {
            if let Some(e) = e.downcast_ref::<SourceFailure>() {
                json!({"status":"original_error","error":{"module":e.module,"type":e.class,"message":e.to_string()}})
            } else {
                json!({"status":"rust_error","message":format!("{e:#}")})
            }
        }
    }
}
fn operation(
    s: &mut DatasetSession,
    a: &Value,
    snapshots: &mut BTreeMap<String, SemanticSourceSnapshotArgument>,
    legacy: bool,
) -> Result<Value> {
    let ns = serde_json::from_value(a.get("namespaces").cloned().unwrap_or(json!({})))?;
    let bindings = serde_json::from_value(a.get("bindings").cloned().unwrap_or(json!({})))?;
    Ok(match a["op"].as_str().unwrap() {
        "observe" => Value::Null,
        "load" => serde_json::to_value(s.load_source_internal(
            a["data"].as_str().unwrap().as_bytes(),
            RdfFormat::NTriples,
            None,
            a["graph_name"].as_str(),
        )?)?,
        "clear" => serde_json::to_value(s.clear_rdflib(a["graph_name"].as_str())?)?,
        "query" | "select" | "ask" | "construct_query" => {
            let text = a["sparql"].as_str().unwrap();
            let cache = a["use_cache"].as_bool().unwrap_or(true);
            let r = if a["op"] == "query" {
                s.query_source_internal(text, &ns, &bindings, cache, None)?
            } else {
                s.helper_source_internal(
                    if a["op"] == "construct_query" {
                        "construct"
                    } else {
                        a["op"].as_str().unwrap()
                    },
                    text,
                    &ns,
                    &bindings,
                    cache,
                )?
            };
            let mut v = r.result;
            v["revision"] = json!(r.revision);
            v["cache_hit"] = json!(r.cache_hit);
            v
        }
        "update" => {
            if legacy {
                let r = s.update_source_internal(a["sparql"].as_str().unwrap(), &ns)?;
                json!({"added":r.added,"removed":r.removed,"quad_count":r.quad_count,"revision":r.revision})
            } else {
                let mut loader =
                    SourceUpdateBytesLoader::new(BTreeMap::new(), 1024 * 1024, 100_000)?;
                let r = s.update_source_full_internal(&a["sparql"], &ns, &bindings, &mut loader)?;
                json!({"added":r.added,"removed":r.removed,"quad_count":r.quad_count,"revision":r.revision})
            }
        }
        "snapshot" => {
            let r = s.snapshot_rdflib(a["created_at"].as_str().unwrap())?;
            snapshots.insert(
                a["id"].as_str().unwrap().into(),
                source_snapshot_argument(&r),
            );
            serde_json::to_value(r)?
        }
        "diff" => {
            let before = snapshots
                .get(a["before"].as_str().unwrap())
                .map(SourceSnapshotInput::Snapshot)
                .unwrap_or(SourceSnapshotInput::NotSnapshot);
            let after = a["after"].as_str().map(|id| {
                snapshots
                    .get(id)
                    .map(SourceSnapshotInput::Snapshot)
                    .unwrap_or(SourceSnapshotInput::NotSnapshot)
            });
            serde_json::to_value(s.diff_source_internal(
                before,
                after,
                a["created_at"].as_str().unwrap(),
            )?)?
        }
        "serialize" => json!(s.serialize_source_turtle_internal(
            &SemanticSourceSerializeRequest {
                format: a["format"].as_str().unwrap().into(),
                base_uri: a["base_uri"].as_str().map(str::to_owned),
                graph_name: a["graph_name"].as_str().map(str::to_owned)
            }
        )?),
        _ => panic!("unfrozen operation"),
    })
}
#[test]
fn source_lifecycle46_original_remaining_actions() {
    let text =
        crate::private_test_fixtures::text_ref!("source-lifecycle-46/original-complete.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "2abb516f7dabf70ec24e3756749e8f832c484c6d4af26902c9c32e65857aa5b3"
    );
    receive(text, 136);
}
#[test]
fn source_lifecycle46_original_cold_actions() {
    let text = crate::private_test_fixtures::text_ref!("source-lifecycle-46/cold.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "47f700b5b1c5f8b843ae57d38560f51e810b28b9e79b2d217643b22bcacbd9f2"
    );
    receive(text, 38);
}
fn receive(text: &str, expected: usize) {
    let corpus: Value = serde_json::from_str(text).unwrap();
    let mut mismatches = Vec::new();
    let mut count = 0;
    for case in corpus["cases"].as_array().unwrap() {
        let legacy = case["contract"] == "data_v2";
        let mut s = if legacy {
            DatasetSession::new_source_data_internal(true)
        } else {
            DatasetSession::new_source_update_internal(true)
        };
        let mut snapshots = BTreeMap::new();
        for (index, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let before = s.source_namespaces.capture(BUDGET).unwrap();
            let result = captured(operation(&mut s, &row["action"], &mut snapshots, legacy));
            let after = s.source_namespaces.capture(BUDGET).unwrap();
            let source_state = if case["observe_source_state"] == false {
                Value::Null
            } else {
                captured(source_observation::state(&mut s))
            };
            let after_observation = s.source_namespaces.capture(BUDGET).unwrap();
            let actual = json!({"action":row["action"],"result":result,"before":before,"after":after,"after_observation":after_observation,"source_state":source_state,"revision":s.revision,"query_cache_entries":s.query_cache.len()});
            if actual != *row {
                let keys: Vec<_> = actual
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|k| actual[*k] != row[*k])
                    .cloned()
                    .collect();
                mismatches.push(format!("{} action{index}: {keys:?}", case["id"]));
            }
            println!(
                "OE_LIFECYCLE46 {}",
                json!({"case":case["id"],"action":index,"matched":actual==*row,"actual":actual})
            );
            count += 1;
        }
    }
    assert_eq!(count, expected);
    assert!(
        mismatches.is_empty(),
        "{} mismatches: {mismatches:?}",
        mismatches.len()
    );
}

#[test]
fn source_lifecycle46_capture_uses_existing_deadline() {
    use super::super::source_namespace_witness::validate_namespace_witness_until;
    use std::time::{Duration, Instant};
    let mut s = DatasetSession::new_source_update_internal(true);
    for populated in [false, true] {
        if populated {
            s.load_source_internal(
                b"<urn:s> <http://test/p> <urn:o> .",
                RdfFormat::NTriples,
                None,
                None,
            )
            .unwrap();
            s.serialize_source_turtle_internal(&SemanticSourceSerializeRequest {
                format: "turtle".into(),
                graph_name: None,
                base_uri: None,
            })
            .unwrap();
        }
        let before = json!(s.source_namespaces.capture(BUDGET).unwrap());
        let identity = (
            s.revision,
            s.query_cache.len(),
            s.rdflib_projection.quads.clone(),
        );
        let expired = Instant::now() - Duration::from_secs(1);
        let error = s
            .source_namespaces
            .capture_until(BUDGET, expired)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("source_namespace_capture_deadline")
        );
        let live = Instant::now() + Duration::from_secs(10);
        let witness = s.source_namespaces.capture_until(BUDGET, live).unwrap();
        validate_namespace_witness_until(&witness, BUDGET, live).unwrap();
        assert!(
            validate_namespace_witness_until(&witness, BUDGET, expired)
                .unwrap_err()
                .to_string()
                .contains("source_namespace_witness_deadline")
        );
        assert!(s.source_namespaces.capture_until(0, live).is_err());
        assert_eq!(before, json!(s.source_namespaces.capture(BUDGET).unwrap()));
        assert_eq!(
            identity,
            (
                s.revision,
                s.query_cache.len(),
                s.rdflib_projection.quads.clone()
            )
        );
    }
}
