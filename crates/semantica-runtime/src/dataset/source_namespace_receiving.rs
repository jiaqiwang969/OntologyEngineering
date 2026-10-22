//! Independent original-library observations, never inputs to production logic.
use super::super::{DatasetSession, RdfFormat};
use super::*;
use serde_json::{Value, json};
const BYTES: usize = 16 * 1024 * 1024;
const ENTRIES: usize = 100_000;
fn fixture() -> Value {
    let text = crate::private_test_fixtures::text_ref!("source-turtle-43/original.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "36e8735cd217a4b5804907a166cf393d5e073a918580ffd25c2a9566ffd5ff02"
    );
    serde_json::from_str(text).unwrap()
}
#[test]
fn source_turtle_namespace_original_106_actions() {
    let fixture = fixture();
    let mut total = 0;
    for case in fixture["namespace_cases"].as_array().unwrap() {
        let mut state = NamespaceState::default();
        for row in case["rows"].as_array().unwrap() {
            let a = &row["action"];
            let named = a["scope"] == "named";
            let uri = a["uri"].as_str().unwrap();
            let generate = a["generate"].as_bool().unwrap();
            let result = if a["op"] == "compute" {
                state
                    .compute(named, uri, generate, BYTES, ENTRIES)
                    .map(|v| json!(v))
            } else {
                TurtleNames::default()
                    .pname(&mut state, named, uri, generate, BYTES, ENTRIES)
                    .map(|v| json!(v))
            };
            let result = match result {
                Ok(value) => json!({"status":"completed","value":value}),
                Err(e) => {
                    let original = e
                        .downcast_ref::<OriginalNamespaceError>()
                        .expect("original exception identity");
                    json!({"status":"original_error","error":{"module":"builtins","type":original.class,"message":e.to_string()}})
                }
            };
            assert_eq!(result, row["result"], "{}: {a}", case["id"]);
            assert_eq!(
                json!(state.bindings()),
                row["bindings"],
                "{}: {a}",
                case["id"]
            );
            total += 1;
        }
    }
    assert_eq!(total, 106);
}
fn req(row: &Value) -> semantica_contracts::SemanticSourceSerializeRequest {
    semantica_contracts::SemanticSourceSerializeRequest {
        format: row["format"].as_str().unwrap().into(),
        graph_name: row["graph_name"].as_str().map(str::to_owned),
        base_uri: row["base_uri"].as_str().map(str::to_owned),
    }
}
#[test]
fn source_turtle_preparation_original_three_wrapper_sequences() {
    let fixture = fixture();
    let mut total = 0;
    let mut prepared = 0;
    let mut refused = 0;
    for case in fixture["wrapper_cases"].as_array().unwrap() {
        let mut session = DatasetSession::new_source_update_internal(true);
        for row in case["rows"].as_array().unwrap() {
            let a = &row["action"];
            match a["op"].as_str().unwrap() {
                "load" => {
                    let format = if a["format"] == "turtle" {
                        RdfFormat::Turtle
                    } else {
                        RdfFormat::NTriples
                    };
                    let v = session
                        .load_source_internal(
                            a["data"].as_str().unwrap().as_bytes(),
                            format,
                            None,
                            a["graph_name"].as_str(),
                        )
                        .unwrap();
                    assert_eq!(serde_json::to_value(v).unwrap(), row["result"]["value"]);
                }
                "clear" => {
                    assert_eq!(
                        serde_json::to_value(session.clear_rdflib(None).unwrap()).unwrap(),
                        row["result"]["value"]
                    );
                }
                "serialize" => {
                    let quads = session.rdflib_projection.quads.clone();
                    let revision = session.revision;
                    let cache = session.query_cache.len();
                    let result = session.prepare_source_turtle_internal(&req(a));
                    assert_eq!(session.rdflib_projection.quads, quads);
                    assert_eq!(session.revision, revision);
                    assert_eq!(session.query_cache.len(), cache);
                    if row["result"]["status"] == "original_error" {
                        let e = result.err().expect("whole named dataset must refuse");
                        let e = e
                            .downcast_ref::<super::super::source_failure::SourceFailure>()
                            .unwrap();
                        assert_eq!(e.class, row["result"]["error"]["type"]);
                        assert_eq!(e.to_string(), row["result"]["error"]["message"]);
                        refused += 1;
                    } else {
                        let actual = result.unwrap();
                        let mut expected = BTreeMap::new();
                        for line in row["result"]["value"]
                            .as_str()
                            .unwrap()
                            .lines()
                            .filter(|s| s.starts_with("@prefix "))
                        {
                            let (prefix, rest) = line
                                .strip_prefix("@prefix ")
                                .unwrap()
                                .split_once(": <")
                                .unwrap();
                            expected.insert(
                                prefix.to_owned(),
                                rest.strip_suffix("> .").unwrap().to_owned(),
                            );
                        }
                        assert_eq!(actual.names.namespaces, expected, "{}: {a}", case["id"]);
                        assert_eq!(json!(session.source_namespaces.bindings()), row["bindings"]);
                        assert_eq!(actual.base, a["base_uri"].as_str().map(str::to_owned));
                        assert_eq!(
                            actual.rows.len(),
                            if a["graph_name"] == "urn:missing" {
                                0
                            } else {
                                1
                            }
                        );
                        prepared += 1;
                    }
                }
                _ => panic!("frozen operation"),
            }
            assert_eq!(session.revision, row["revision"].as_u64().unwrap());
            total += 1;
        }
    }
    assert_eq!((total, prepared, refused), (13, 7, 1));
}
#[test]
fn source_turtle_namespace_budget_errors_are_not_missing_prefixes() {
    let mut state = NamespaceState::default();
    let e = TurtleNames::default()
        .pname(&mut state, false, "urn:test:p", true, 32, ENTRIES)
        .unwrap_err();
    assert_eq!(e.to_string(), "source_namespace_retained_budget");
    assert!(state.bindings().is_empty());
    state
        .compute(false, "urn:test:p", true, BYTES, ENTRIES)
        .unwrap();
    let bindings = state.bindings().to_vec();
    let used = state.charged_bytes;
    let e = TurtleNames::default()
        .pname(&mut state, false, "urn:other:p", false, used, ENTRIES)
        .unwrap_err();
    assert_eq!(e.to_string(), "source_namespace_retained_budget");
    assert_eq!(state.bindings(), bindings);
    assert_eq!(
        state
            .compute(false, "urn:test:p", false, used, ENTRIES)
            .unwrap()[0],
        "ns1"
    );
}
#[test]
fn source_turtle_preparation_preserves_old_admission_and_graph_guards() {
    let mut ordinary = DatasetSession::new();
    let mut request = req(&json!({"format":"turtle"}));
    assert!(ordinary.prepare_source_turtle_internal(&request).is_err());
    let mut source = DatasetSession::new_source_update_internal(false);
    assert_eq!(
        source
            .serialize_source_nt_nq_internal(&request)
            .unwrap_err()
            .to_string(),
        "source_serialization_format_not_received"
    );
    for invalid in ["", "relative", "urn:bad graph"] {
        request.graph_name = Some(invalid.into());
        let e = source
            .prepare_source_turtle_internal(&request)
            .err()
            .unwrap();
        assert_eq!(
            e.downcast_ref::<super::super::source_failure::SourceFailure>()
                .unwrap()
                .class,
            "SemanticInputError"
        );
    }
    request.graph_name = None;
    request.format = "nquads".into();
    assert!(source.prepare_source_turtle_internal(&request).is_err());
    assert_eq!(
        source
            .serialize_source_nt_nq_internal(&request)
            .unwrap()
            .text,
        "\n"
    );
}
#[test]
fn source_turtle_unicode151_and_original_base_replacement() {
    for c in ['中', 'Ⅻ', '١', '\u{2ebf0}'] {
        assert_eq!(semantica_json::python_xml_name_class(c), (true, true));
    }
    assert_eq!(
        semantica_json::python_xml_name_class('\u{0301}'),
        (false, true)
    );
    assert_eq!(
        semantica_json::python_xml_name_class('\u{1c89}'),
        (false, false)
    ); // Unicode16 letter, unassigned in15.1.
    assert!(can_relativize("urn:x:urn:x:a", Some("urn:x:")));
    assert!(!can_relativize("urn:x:a/b", Some("urn:x:")));
    assert!(!can_relativize("urn:x:a#b", Some("urn:x:")));
    assert!(!can_relativize("urn:x:a", None));
}
