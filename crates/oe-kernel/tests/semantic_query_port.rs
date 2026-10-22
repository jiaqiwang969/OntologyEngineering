//! Production adapter uses the neutral generated contract with the same engine.
#[test]
fn engineering_query_port_preserves_public_results_and_failures() {
    let result = oe_kernel::query::execute_local_query(
        b"<urn:part> <urn:has> <urn:shaft> .",
        "ASK { <urn:part> <urn:has> <urn:shaft> }",
    )
    .unwrap();
    assert_eq!(result["kind"], "ask");
    assert_eq!(result["value"], true);
    let empty = oe_kernel::query::execute_local_query(b"", "SELECT ?s WHERE { ?s ?p ?o }").unwrap();
    assert_eq!(empty["row_count"], 0);
    assert!(
        oe_kernel::query::execute_local_query(
            b"",
            "ASK { SERVICE <https://example.invalid/> { ?s ?p ?o } }"
        )
        .unwrap_err()
        .to_string()
        .contains("remote_service_unsupported")
    );
    assert!(oe_kernel::query::execute_local_query(b"<broken", "ASK {}").is_err());
}
