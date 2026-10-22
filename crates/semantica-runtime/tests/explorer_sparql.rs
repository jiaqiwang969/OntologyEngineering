use semantica_runtime::{context, dataset::DatasetSession, explorer_sparql};
use serde_json::{Value, json};
fn sample() -> DatasetSession {
    let mut s = DatasetSession::new();
    context::load_context_graph(&mut s,&json!({"nodes":[
        {"id":"python","type":"language","content":"Python programming language","properties":{"popularity":"high"}},
        {"id":"javascript","type":"language","content":"JavaScript programming language","properties":{}},
        {"id":"web_dev","type":"concept","content":"Web Development","properties":{}}
    ],"edges":[{"id":"original-used-in","source":"python","target":"web_dev","type":"used_in","properties":{"weight":0.5}}]})).unwrap();
    s
}
fn unordered_rows(v: &Value) -> Vec<String> {
    let mut rows = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect::<Vec<_>>();
    rows.sort();
    rows
}
#[test]
fn original_success_responses_use_same_graph_meaning_without_publishing() {
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/original-explorer-sparql-observations.json"
    ))
    .unwrap();
    let mut s = sample();
    let before = s.snapshot().unwrap();
    let mut compared = 0;
    for case in oracle["cases"].as_array().unwrap() {
        let expected = &case["response"];
        if !expected["error"].is_null() || expected["truncated"] == true {
            continue;
        }
        let actual =
            explorer_sparql::query_projection(&s, case["request"]["query"].as_str().unwrap(), 5000)
                .unwrap();
        for key in [
            "columns",
            "total",
            "truncated",
            "error",
            "error_line",
            "error_column",
        ] {
            assert_eq!(actual[key], expected[key], "{} {key}", case["test_id"]);
        }
        // Original tests make set/membership assertions, no ORDER BY is present.
        assert_eq!(
            unordered_rows(&actual),
            unordered_rows(expected),
            "{}",
            case["test_id"]
        );
        compared += 1;
    }
    assert!(compared >= 10);
    assert_eq!(s.snapshot().unwrap(), before);
}
#[test]
fn bounded_projection_rows_preserve_truncation_and_session_identity() {
    let mut s = sample();
    let before = s.snapshot().unwrap();
    let r = explorer_sparql::query_projection(
        &s,
        "SELECT ?s WHERE {?s a <http://semantica.local/entity/language>} ORDER BY ?s",
        1,
    )
    .unwrap();
    assert_eq!(
        r["rows"],
        json!([{"s":"http://semantica.local/entity/javascript"}])
    );
    assert_eq!(r["truncated"], true);
    assert_eq!(r["total"], 1);
    assert!(explorer_sparql::query_projection(&s, "ASK {}", 5001).is_err());
    assert!(
        explorer_sparql::query_projection(&s, "INSERT DATA {<urn:a> <urn:b> <urn:c>}", 1).is_err()
    );
    assert_eq!(s.snapshot().unwrap(), before);
}
#[test]
fn original_lexical_admission_preserves_unicode_and_literal_refusals() {
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/original-explorer-sparql-admission.json"
    ))
    .unwrap();
    for case in oracle["cases"].as_array().unwrap() {
        let query = case["query"].as_str().unwrap();
        assert_eq!(
            explorer_sparql::read_only_query(query).unwrap(),
            case["accepted"].as_bool().unwrap(),
            "{query:?}"
        );
    }
    let mut s = sample();
    let before = s.snapshot().unwrap();
    let error = explorer_sparql::query_projection(&s, "SELECT ?s WHERE {?s ?p \"DROP\"}", 5000)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Only SELECT, ASK, CONSTRUCT, and DESCRIBE queries are permitted."
    );
    assert_eq!(s.snapshot().unwrap(), before);
}
#[test]
fn original_json_property_terms_keep_lexical_values_and_numeric_datatypes() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/original-explorer-sparql-terms.json")).unwrap();
    let mut s = DatasetSession::new();
    context::load_context_graph(&mut s, &oracle["input"]).unwrap();
    let before = s.snapshot().unwrap();
    let graph = explorer_sparql::project(&s).unwrap();
    assert_eq!(graph.len(), oracle["triples"].as_array().unwrap().len());
    for triple in oracle["triples"].as_array().unwrap() {
        let subject = format!("<{}>", triple["subject"]["value"].as_str().unwrap());
        let predicate = triple["predicate"]["value"].as_str().unwrap();
        let q = graph
            .iter()
            .find(|q| q.subject.to_string() == subject && q.predicate.as_str() == predicate)
            .unwrap();
        let actual = semantica_runtime::graph::term_value(&q.object.into_owned());
        let mut expected = triple["object"].clone();
        // RDF1.1 representation of original plain literals. This does not
        // claim preservation of RDFLib plain-vs-explicit-string identity.
        if expected["kind"] == "literal" && expected["datatype"].is_null() {
            expected["datatype"] = json!("http://www.w3.org/2001/XMLSchema#string");
        }
        assert_eq!(actual, expected, "{predicate}");
    }
    assert_eq!(s.snapshot().unwrap(), before);
}

#[test]
fn native_syntax_diagnostics_keep_positions_and_do_not_mask_other_failures() {
    let mut session = sample();
    let before = session.snapshot().unwrap();
    for (query, line, column) in [("SELECT * WHERE {", 1, 17), ("SELECT * WHERE {\n", 2, 1)] {
        let result = explorer_sparql::query_projection_response(&session, query, 5000).unwrap();
        assert_eq!(result["columns"], json!([]));
        assert_eq!(result["rows"], json!([]));
        assert_eq!(result["total"], 0);
        assert_eq!(result["truncated"], false);
        assert!(result["error"].as_str().unwrap().starts_with("error at "));
        assert_eq!(result["error_line"], line);
        assert_eq!(result["error_column"], column);
    }
    assert!(
        explorer_sparql::query_projection_response(
            &session,
            "ASK {SERVICE <https://example.invalid/query> {?s ?p ?o}}",
            5000
        )
        .is_err()
    );
    assert!(explorer_sparql::query_projection_response(&session, "DROP ALL", 5000).is_err());
    // Empty graph is a valid context by the existing records()/summary contract.
    let mut corrupt = DatasetSession::new();
    corrupt.load(b"<urn:semantica:context:root> <urn:semantica:context:schema> <urn:invalid-nonliteral> <urn:semantica:context:records/1> .",oxigraph::io::RdfFormat::NQuads,None,None).unwrap();
    let error = explorer_sparql::query_projection_response(&corrupt, "ASK {}", 5000).unwrap_err();
    assert_eq!(error.to_string(), "context_projection_nonliteral");
    assert_eq!(session.snapshot().unwrap(), before);
}
