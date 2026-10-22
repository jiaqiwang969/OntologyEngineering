use semantica_contracts::SemanticExplorerMutationInput;
use semantica_runtime::{
    context::load_context_graph,
    dataset::DatasetSession,
    explorer_mutation::{execute, observe},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
#[test]
fn original_import_unicode_and_raw_validity_use_explicit_extended_mutation_profile() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/explorer_import_boundaries.json")).unwrap();
    for row in corpus["rows"].as_array().unwrap() {
        let payload = &row["payload"];
        let call = json!({"method":"add_nodes_and_edges","args":[payload.get("nodes").cloned().unwrap_or(json!([])),payload.get("edges").cloned().unwrap_or(json!([]))],"kwargs":{}});
        let mut session = DatasetSession::new();
        load_context_graph(&mut session, &json!({"nodes":[],"edges":[]})).unwrap();
        let before = session.snapshot().unwrap();
        let mut input = SemanticExplorerMutationInput {
            schema: "semantica.explorer-mutation-input/1".into(),
            bridge: true,
            call,
        };
        assert!(
            execute(
                &mut session,
                &input,
                Instant::now() + Duration::from_secs(10)
            )
            .is_err()
        );
        assert_eq!(session.snapshot().unwrap(), before);
        input.schema = "semantica.explorer-mutation-input/2".into();
        assert!(input.validate().is_ok());
        let result = execute(
            &mut session,
            &input,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(result.result["status"], "returned");
        assert_eq!(observe(&session).unwrap(), row["after"], "{}", row["id"]);
    }
}
