//! Frozen original state and independent structural controls, not a replay oracle.
use super::super::{
    DatasetSession, RdfFormat, source_failure::SourceFailure,
    source_namespace_witness::validate_namespace_witness,
    source_update_loader::SourceUpdateBytesLoader,
};
use super::*;
use semantica_contracts::{SemanticSourceNamespaceState4, SemanticSourceSerializeRequest};
use serde_json::{Value, json};
const BUDGET: usize = 4 * 1024 * 1024;
fn original() -> Value {
    let text = crate::private_test_fixtures::text_ref!("source-namespace-45/original.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "e9e05863822aaa1bf97e14ee8e8f7e4f33231992ce8911d875cba646af7c0698"
    );
    serde_json::from_str(text).unwrap()
}
fn operation(s: &mut DatasetSession, a: &Value) -> Result<Value> {
    let ns: BTreeMap<String, String> =
        serde_json::from_value(a.get("namespaces").cloned().unwrap_or(json!({})))?;
    Ok(match a["op"].as_str().unwrap() {
        "observe" => Value::Null,
        "construct" => serde_json::to_value(s.load_construct_source_internal(
            &json!({"triples":a["triples"]}),
            None,
            a["graph_name"].as_str(),
        )?)?,
        "load" => serde_json::to_value(s.load_source_internal(
            a["data"].as_str().unwrap().as_bytes(),
            RdfFormat::NTriples,
            None,
            a["graph_name"].as_str(),
        )?)?,
        "clear" => serde_json::to_value(s.clear_rdflib(a["graph_name"].as_str())?)?,
        "query" => {
            let r = s.query_source_internal(
                a["sparql"].as_str().unwrap(),
                &ns,
                &BTreeMap::new(),
                a["use_cache"].as_bool().unwrap_or(true),
                None,
            )?;
            let mut value = r.result;
            value["revision"] = json!(r.revision);
            value["cache_hit"] = json!(r.cache_hit);
            value
        }
        "update" => {
            let mut loader = SourceUpdateBytesLoader::new(BTreeMap::new(), 1024 * 1024, 100_000)?;
            let r =
                s.update_source_full_internal(&a["sparql"], &ns, &BTreeMap::new(), &mut loader)?;
            json!({"added":r.added,"removed":r.removed,"quad_count":r.quad_count,"revision":r.revision})
        }
        "serialize" => {
            let req = SemanticSourceSerializeRequest {
                format: a["format"].as_str().unwrap().into(),
                graph_name: a["graph_name"].as_str().map(str::to_owned),
                base_uri: a["base_uri"].as_str().map(str::to_owned),
            };
            if matches!(req.format.as_str(), "turtle" | "ttl") {
                json!(s.serialize_source_turtle_internal(&req)?)
            } else {
                json!(s.serialize_source_nt_nq_internal(&req)?.text)
            }
        }
        _ => panic!("frozen action"),
    })
}
#[test]
fn source_namespace45_original_mixed_history() {
    receive(original(), 108);
}
#[test]
fn source_namespace45_original_prefix_use_and_failure() {
    let text = crate::private_test_fixtures::text_ref!("source-namespace-45/supplement.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "b5a466466d2886e72b8aedb11dc34b2989c495b8d17d7bba2afaebfbd5e00aa9"
    );
    receive(serde_json::from_str(text).unwrap(), 33);
}
#[test]
fn source_namespace45_original_query_error_locations() {
    let text = crate::private_test_fixtures::text_ref!("source-namespace-45/query_errors.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "46e3688d086543349d503e130958b14a4c8b1d6cacb969ecffbd51cccdffeba1"
    );
    receive(serde_json::from_str(text).unwrap(), 15);
}
fn receive(corpus: Value, expected_actions: usize) {
    let mut mismatches = Vec::new();
    let mut actions = 0;
    for case in corpus["cases"].as_array().unwrap() {
        let mut s = DatasetSession::new_source_update_internal(true);
        for (index, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let before = s.source_namespaces.capture(BUDGET).unwrap();
            let result = match operation(&mut s, &row["action"]) {
                Ok(value) => json!({"status":"completed","value":value}),
                Err(e) => {
                    if let Some(e) = e.downcast_ref::<SourceFailure>() {
                        json!({"status":"original_error","error":{"module":e.module,"type":e.class,"message":e.to_string()}})
                    } else {
                        json!({"status":"rust_error","message":format!("{e:#}")})
                    }
                }
            };
            let after = s.source_namespaces.capture(BUDGET).unwrap();
            validate_namespace_witness(&before, BUDGET).unwrap();
            validate_namespace_witness(&after, BUDGET).unwrap();
            let actual = json!({"action":row["action"],"result":result,"before":before,"after":after,"revision":s.revision,"query_cache_entries":s.query_cache.len()});
            if actual != *row {
                let keys: Vec<_> = [
                    "result",
                    "before",
                    "after",
                    "revision",
                    "query_cache_entries",
                ]
                .into_iter()
                .filter(|key| actual[*key] != row[*key])
                .collect();
                mismatches.push(format!("{} action{index}: {keys:?}", case["id"]));
            }
            println!(
                "OE_NAMESPACE45 {}",
                json!({"case":case["id"],"action":index,"matched":actual==*row,"actual":actual})
            );
            actions += 1;
        }
    }
    assert_eq!(actions, expected_actions);
    assert!(
        mismatches.is_empty(),
        "{} mismatches: {mismatches:?}",
        mismatches.len()
    );
}
#[test]
fn source_namespace45_capture_is_bounded_and_nonwarming() {
    let mut s = DatasetSession::new_source_update_internal(true);
    for populate in [false, true] {
        if populate {
            s.load_source_internal(
                b"<urn:s> <http://test/p> <urn:o> .",
                RdfFormat::NTriples,
                None,
                None,
            )
            .unwrap();
            s.serialize_source_turtle_internal(&SemanticSourceSerializeRequest {
                format: "turtle".into(),
                base_uri: None,
                graph_name: None,
            })
            .unwrap();
        }
        let before = json!(s.source_namespaces.capture(BUDGET).unwrap());
        let identity = (
            s.revision,
            s.query_cache.len(),
            s.rdflib_projection.quads.clone(),
        );
        for limit in [0, 1, 32] {
            assert!(
                s.source_namespaces
                    .capture(limit)
                    .unwrap_err()
                    .to_string()
                    .contains("source_namespace_capture_budget")
            );
        }
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
#[test]
fn source_namespace45_witness_rejects_structural_mutations() {
    let text = crate::private_test_fixtures::text_ref!("source-namespace-45/controls.json");
    let controls: Value = serde_json::from_str(text).unwrap();
    for case in controls["cases"].as_array().unwrap() {
        let w: SemanticSourceNamespaceState4 =
            serde_json::from_value(case["witness"].clone()).unwrap();
        let result = validate_namespace_witness(&w, BUDGET);
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}: {result:?}",
            case["id"]
        );
        if let Err(e) = result {
            assert!(
                e.to_string().starts_with("source_namespace_witness_"),
                "unexpected owner: {e:#}"
            );
        }
    }
}
