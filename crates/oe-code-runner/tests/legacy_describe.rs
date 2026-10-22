//! Actual existing runner1 consumer, independent original expected output.
use oe_core::SkillRoot;
use oe_wire::json::canonical_json;
use serde_json::{Value, json};
use std::path::Path;
use std::process::Command;

#[test]
fn current_legacy_runner1_hidden_describe_matches_original_and_keeps_strict_guards() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let root = SkillRoot::new(workspace).unwrap();
    let directory = root
        .new_private_dir(Path::new(&format!(
            "var/maintenance/legacy-describe-runner-tests/{}",
            std::process::id()
        )))
        .unwrap();
    let original: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/query-bindings-original15.json"
    ))
    .unwrap();
    let hidden = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "describe_hidden_parameter")
        .unwrap();
    let binary = Path::new(env!("CARGO_BIN_EXE_oe-code-runner"));
    let before = oe_core::fingerprint_file(binary, 512 * 1024 * 1024).unwrap();
    for (id, query, bindings, expected_error) in [
        (
            "hidden",
            hidden["query"].as_str().unwrap(),
            json!({"s":{"kind":"iri","value":"urn:other"}}),
            None,
        ),
        (
            "reassignment",
            "SELECT ?name WHERE { BIND(\"new\" AS ?name) }",
            json!({"name":{"kind":"iri","value":"urn:prior"}}),
            Some("query_initial_binding_reassignment_unsupported"),
        ),
        (
            "aggregate-alias-reassignment",
            "SELECT (COUNT(?s) AS ?count) WHERE { ?s ex:name ?name }",
            json!({"count":{"kind":"iri","value":"urn:prior"}}),
            // The genuine parser lowers COUNT AS ?count through an Extend
            // alias; this prior guard rejects before the inner Group guard.
            Some("query_initial_binding_reassignment_unsupported"),
        ),
    ] {
        let input = json!({"turtle":original["input_data"],"query":query,"namespaces":{"ex":"urn:"},"bindings":bindings});
        let invocation = json!({"schema":"oev2.code-replay-invocation/1","profile":"local-semantic-replay/1","capability":"rdf.query-with-bindings/1","case_id":id,"input_sha256":oe_core::sha256(canonical_json(&input).unwrap()),"conditions_sha256":oe_core::sha256(b"original target/context repair through same runner1 with original strict guards"),"input":input,"run_id":1,"seed":1,"budget":{"timeout_ms":60000,"output_bytes":1048576}});
        let path = directory.join(format!("{id}.json"));
        std::fs::write(&path, canonical_json(&invocation).unwrap()).unwrap();
        let actual = Command::new(binary).arg("run").arg(&path).output().unwrap();
        assert!(actual.status.success());
        assert!(actual.stderr.is_empty());
        let observation: Value = serde_json::from_slice(&actual.stdout).unwrap();
        if let Some(error) = expected_error {
            assert_eq!(observation["execution"], "failed");
            assert_eq!(observation["error"], error);
        } else {
            assert_eq!(observation["execution"], "completed");
            let triples: Vec<_> = observation["result"]["triples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|triple| format!("{} .", triple.as_str().unwrap()))
                .collect();
            assert_eq!(serde_json::to_value(triples).unwrap(), hidden["triples"]);
        }
        assert_eq!(
            oe_core::fingerprint_file(binary, 512 * 1024 * 1024)
                .unwrap()
                .sha256,
            before.sha256
        );
    }
}
