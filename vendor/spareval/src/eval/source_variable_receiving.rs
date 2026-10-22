//! Actual prepared DATA mutation plus parser/evaluator/result-owner receiving.
//! Wrapper revision/cache/snapshot/error-class duties remain explicitly pending.
use super::{numeric_query, source_bridge, source_update};
use crate::dataset::source_memory::SourceMemoryGraph;
use oxrdf::NamedNode;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[test]
fn source_variable_original_data_and_query_projection() {
    let plan = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_plan.json"
    );
    let original = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_original.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(original.as_bytes())),
        "04897e1b5d024b7ef47b07caebde42a9445bbca3b99120678ff2f8474cd324c1"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(plan.as_bytes())),
        "001e10de65e41964d10f9b88731a26dfe111bd83a66543f5c911a1fd8b4fb211"
    );
    let plan: Value = serde_json::from_str(plan).unwrap();
    let original: Value = serde_json::from_str(original).unwrap();
    let mut queries = 0;
    let mut data = 0;
    for (ci, case) in plan["runtime_cases"].as_array().unwrap().iter().enumerate() {
        let session = source_bridge::source_session().unwrap();
        let mut graph = SourceMemoryGraph::new_dataset(
            session.clone(),
            None,
            NamedNode::new_unchecked("urn:x-rdflib:default").into(),
        )
        .unwrap();
        for (ai, action) in case["actions"].as_array().unwrap().iter().enumerate() {
            let expected = &original["runtime"][ci]["actions"][ai];
            match action["op"].as_str().unwrap() {
                "update" => {
                    source_update::prepare(
                        action["query"].as_str().unwrap(),
                        &BTreeMap::new(),
                        session.clone(),
                    )
                    .unwrap()
                    .execute_data(&mut graph)
                    .unwrap();
                    assert_eq!(
                        graph.len(),
                        expected["quad_count"].as_u64().unwrap() as usize,
                        "{}",
                        expected["id"]
                    );
                    data += 1;
                }
                "query" => {
                    let result = numeric_query::prepare_source_runtime_query(
                        action["query"].as_str().unwrap(),
                        graph.clone(),
                        session.clone(),
                        vec![],
                    )
                    .unwrap()
                    .execute_source_runtime_result();
                    if expected["result"]["status"] == "completed" {
                        let mut value = expected["result"]["value"].clone();
                        let object = value.as_object_mut().unwrap();
                        object.remove("revision");
                        object.remove("cache_hit");
                        assert_eq!(
                            result.unwrap_or_else(|e| panic!(
                                "{} {}: {e}",
                                expected["id"], action["query"]
                            )),
                            value,
                            "{}",
                            expected["id"]
                        );
                    } else {
                        assert_eq!(
                            result.unwrap_err().to_string(),
                            expected["result"]["error"]["message"].as_str().unwrap(),
                            "{}",
                            expected["id"]
                        );
                    }
                    queries += 1;
                }
                "snapshot" => (), // Required full wrapper receiving, not executed here.
                _ => panic!(),
            }
        }
    }
    assert_eq!(data, 10);
    assert_eq!(queries, 80);
    // Separately declared Rust authority/partial-operation controls. These
    // are not additional original observations or full wrapper UPDATE proof.
    let session = source_bridge::source_session().unwrap();
    let mut graph = SourceMemoryGraph::new_dataset(
        session.clone(),
        None,
        NamedNode::new_unchecked("urn:x-rdflib:default").into(),
    )
    .unwrap();
    let foreign = source_update::prepare(
        "INSERT DATA {?x <urn:p> <urn:o>}",
        &BTreeMap::new(),
        source_bridge::source_session().unwrap(),
    )
    .unwrap();
    assert!(foreign.execute_data(&mut graph).is_err());
    assert_eq!(graph.len(), 0);
    let partial = source_update::prepare(
        "INSERT DATA {?x <urn:p> <urn:o>}; CREATE GRAPH <urn:g>",
        &BTreeMap::new(),
        session,
    )
    .unwrap();
    assert!(partial.execute_data(&mut graph).is_err());
    assert_eq!(graph.len(), 1);
}
