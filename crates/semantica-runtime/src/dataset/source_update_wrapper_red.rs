//! First original whole-case UPDATE diagnostics, before compatibility fixes.
//! This staged file is not part of the frozen SOURCE04 build.
use super::*;
use super::source_failure::SourceFailure;
use serde_json::{Value, json};
use spareval::{SourceNodeObservation, SourceTermInput, SourceUpdateLoader};

struct NoResources;
impl SourceUpdateLoader for NoResources {
    fn load(
        &mut self,
        _: &str,
        _: &mut dyn FnMut([SourceTermInput<'_>; 3]) -> Result<(), spareval::QueryEvaluationError>,
    ) -> Result<bool, spareval::QueryEvaluationError> {
        Err(spareval::QueryEvaluationError::Unexpected(
            std::io::Error::other("diagnostic_has_no_external_resource_authority").into(),
        ))
    }
}

fn observed_node(value: SourceNodeObservation<'_>) -> Value {
    let (kind, text, datatype, language) = match value.term {
        SourceTermInput::Iri(v) => ("iri", v, None, None),
        SourceTermInput::Blank(v) => ("blank_node", v, None, None),
        SourceTermInput::Literal { value, datatype, language } =>
            ("literal", value, datatype, language),
        SourceTermInput::Variable(_) => panic!("Variable outside the declared first diagnostic cases"),
    };
    assert_ne!(kind, "blank_node", "blank mapping belongs to the complete receiver");
    assert!(text.is_ascii());
    assert!(datatype.is_none_or(str::is_ascii));
    assert!(language.is_none_or(str::is_ascii));
    json!({"value":text,"term_type":kind,"datatype":datatype,"language":language})
}

fn state(session: &mut DatasetSession) -> Value {
    let mut quads = Vec::new();
    let mut contexts = Vec::new();
    session.rdflib_projection.source_state.as_mut().unwrap().visit_observations(
        |row| {
            let mut fields = Vec::new();
            for (position, value) in row.into_iter().enumerate() {
                if position == 3 && matches!(value.term, SourceTermInput::Iri("urn:x-rdflib:default")) {
                    fields.push(Value::Null);
                } else {
                    fields.push(observed_node(value));
                }
            }
            quads.push(Value::Array(fields));
            Ok(())
        },
        |value| { contexts.push(observed_node(value)); Ok(()) },
    ).unwrap();
    // Only the observation copies are sorted. The live store is unchanged.
    // These two first cases have no blanks or non-ASCII terms.
    quads.sort_by_cached_key(|value| semantica_json::canonical_json(value).unwrap());
    contexts.sort_by_cached_key(|value| semantica_json::canonical_json(value).unwrap());
    json!({"contexts":contexts,"revision":session.revision(),
        "query_cache_entries":session.query_cache.len(),
        "snapshot_cached":session.rdflib_snapshot_cache.get().is_some(),
        "quad_count":session.quad_count(),"quads":quads})
}

fn run_first_case(first: &str, last: &str) {
    let original = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-update-original.json"
    );
    assert_eq!(crate::sha256(original),
        "7bb3c73aebd8725aaaafa1bebfcf54fb1d1c0f1ed3ed27d0bcf0f9673d258895");
    let original: Value = serde_json::from_str(original).unwrap();
    let source = &original["source_sets"][0];
    let inputs = source["input"].as_array().unwrap();
    let expected = source["actual"].as_array().unwrap();
    assert_eq!(inputs.len(), expected.len());
    let mut session = DatasetSession::new_source_update_internal(true);
    let mut failures = Vec::new();
    let mut records = Vec::new();
    for (input, expected) in inputs.iter().zip(expected) {
        let id = input["id"].as_str().unwrap();
        if id < first || id > last { continue; }
        assert_eq!(input["id"], expected["id"]);
        assert_eq!(input["operation"], expected["operation"]);
        let operation = input["operation"].as_str().unwrap();
        let result = (|| -> anyhow::Result<Value> {
            Ok(match operation {
                "new" => {
                    assert_eq!(input["profile"], "rdf");
                    session = DatasetSession::new_source_update_internal(input["cache_queries"].as_bool().unwrap());
                    json!({"profile":"rdf","cache_queries":input["cache_queries"]})
                },
                "load" => {
                    assert_eq!(input["format"], "nquads");
                    serde_json::to_value(session.load_rdflib_with_base(
                        input["data"].as_str().unwrap().as_bytes(), RdfFormat::NQuads,
                        input["base_uri"].as_str(), input["graph_name"].as_str(),
                    )?)?
                },
                "select" => {
                    let result = session.helper_source_internal("select", input["query"].as_str().unwrap(),
                        &serde_json::from_value(input["namespaces"].clone())?,
                        &serde_json::from_value(input["bindings"].clone())?,
                        input["use_cache"].as_bool().unwrap())?;
                    let mut value = result.result;
                    value["revision"] = json!(result.revision);
                    value["cache_hit"] = json!(result.cache_hit);
                    value
                },
                "update" => {
                    let result = session.update_source_full_internal(&input["query"],
                        &serde_json::from_value(input["namespaces"].clone())?,
                        &serde_json::from_value(input["bindings"].clone())?, &mut NoResources)?;
                    json!({"added":result.added,"removed":result.removed,
                        "quad_count":result.quad_count,"revision":result.revision})
                },
                "snapshot" => serde_json::to_value(session.snapshot_rdflib(input["created_at"].as_str().unwrap())?)?,
                "clear" => serde_json::to_value(session.clear_rdflib(input["graph_name"].as_str())?)?,
                _ => panic!("operation outside the declared first diagnostic cases"),
            })
        })();
        let mut actual = json!({"id":id,"operation":operation,"state":state(&mut session)});
        match result {
            Ok(value) => { actual["status"] = json!("completed"); actual["raw"] = value; },
            Err(error) => match error.downcast_ref::<SourceFailure>() {
                Some(error) => {
                    actual["status"] = json!("original_error");
                    actual["error"] = json!({"module":error.module,"type":error.class,"message":error.to_string()});
                },
                None => { actual["status"] = json!("unreceived_Rust_error"); actual["diagnostic"] = json!(format!("{error:#}")); },
            },
        }
        let matched = actual == *expected;
        println!("OE_SOURCE_UPDATE_RED {}", json!({"id":id,"matched":matched,"actual":actual,"expected":expected}));
        if !matched { failures.push(id.to_owned()); }
        records.push(id.to_owned());
    }
    assert_eq!(records.len(), 9);
    assert_eq!(records.first().unwrap(), first);
    assert_eq!(records.last().unwrap(), last);
    assert!(failures.is_empty(), "original whole-case mismatches: {failures:?}");
}

#[test]
fn source_update_wrapper_red_comment_only_original_lifecycle() {
    run_first_case("UP0244", "UP0252");
}
#[test]
fn source_update_wrapper_red_prologue_only_original_lifecycle() {
    run_first_case("UP0253", "UP0261");
}

// Original whole cases, including lifecycle effects, not only the update call.
#[test]
fn source_update_wrapper_red_variable_graph_original_lifecycle() {
    run_first_case("UP0622", "UP0630");
}
#[test]
fn source_update_wrapper_red_optional_original_lifecycle() {
    run_first_case("UP0694", "UP0702");
}
#[test]
fn source_update_wrapper_red_union_original_lifecycle() {
    run_first_case("UP0703", "UP0711");
}
