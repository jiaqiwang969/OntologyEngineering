//! Frozen lifecycle expectations from locked SemanticRuntime and W3C RDFC1.0.
use oxigraph::{
    io::{RdfFormat, RdfParser},
    model::Dataset,
};
use semantica_runtime::dataset::{DatasetSession, diff_snapshots, snapshot};

fn dataset(input: &[u8]) -> Dataset {
    let mut graph = Dataset::new();
    for quad in RdfParser::from_format(RdfFormat::NQuads).for_reader(input) {
        graph.insert(&quad.unwrap());
    }
    graph
}
#[test]
fn w3c_canonical_nquads_match_official_bytes_including_blank_graphs() {
    for (input, expected) in [
        (
            include_bytes!("fixtures/rdf-canon/test003-in.nq").as_slice(),
            include_str!("fixtures/rdf-canon/test003-rdfc10.nq"),
        ),
        (
            include_bytes!("fixtures/rdf-canon/test020-in.nq").as_slice(),
            include_str!("fixtures/rdf-canon/test020-rdfc10.nq"),
        ),
        (
            include_bytes!("fixtures/rdf-canon/test021-in.nq").as_slice(),
            include_str!("fixtures/rdf-canon/test021-rdfc10.nq"),
        ),
        (
            include_bytes!("fixtures/rdf-canon/test070-in.nq").as_slice(),
            include_str!("fixtures/rdf-canon/test070-rdfc10.nq"),
        ),
        (
            include_bytes!("fixtures/rdf-canon/test071-in.nq").as_slice(),
            include_str!("fixtures/rdf-canon/test071-rdfc10.nq"),
        ),
        (
            include_bytes!("fixtures/rdf-canon/test072-in.nq").as_slice(),
            include_str!("fixtures/rdf-canon/test072-rdfc10.nq"),
        ),
        (
            include_bytes!("fixtures/rdf-canon/test073-in.nq").as_slice(),
            include_str!("fixtures/rdf-canon/test073-rdfc10.nq"),
        ),
    ] {
        let s = snapshot(&dataset(input), 9).unwrap();
        assert_eq!(s.canonical_nquads, expected);
        assert_eq!(s.dataset_sha256, semantica_runtime::sha256(expected));
        assert_eq!(s.revision, 9);
        assert_eq!(s.canonicalization, "semantica.rdfc-1.0-sha256/1");
    }
}
#[test]
fn canonical_content_excludes_revision_and_blank_labels_but_preserves_graph_identity() {
    let before = snapshot(&dataset(b"_:a <urn:p> _:b <urn:g> .\n"), 1).unwrap();
    let after = snapshot(&dataset(b"_:right <urn:p> _:left <urn:g> .\n"), 7).unwrap();
    assert_eq!(before.dataset_sha256, after.dataset_sha256);
    let moved = snapshot(&dataset(b"_:a <urn:p> _:b <urn:other> .\n"), 1).unwrap();
    assert_ne!(before.dataset_sha256, moved.dataset_sha256);
    assert!(diff_snapshots(&before, &after).unwrap().added.is_empty());
    let empty = snapshot(&Dataset::new(), 0).unwrap();
    assert_eq!(empty.canonical_nquads, "");
    assert_eq!(empty.dataset_sha256, semantica_runtime::sha256([]));
}
#[test]
fn canonicalization_poison_is_rejected_but_unique_hash_nodes_are_not_limited_to_eight() {
    assert!(
        snapshot(
            &dataset(include_bytes!("fixtures/rdf-canon/test074-in.nq")),
            0
        )
        .is_err()
    );
    let distinct = (0..12)
        .map(|i| format!("_:b{i} <urn:p> \"{i}\" .\n"))
        .collect::<String>();
    assert_eq!(
        snapshot(&dataset(distinct.as_bytes()), 0)
            .unwrap()
            .quad_count,
        12
    );
    let ambiguous = (0..9)
        .map(|i| format!("_:b{i} <urn:p> <urn:o> .\n"))
        .collect::<String>();
    assert!(snapshot(&dataset(ambiguous.as_bytes()), 0).is_err());
}
#[test]
fn load_is_atomic_and_every_success_invalidates_query_and_snapshot_caches() {
    let mut s = DatasetSession::new();
    let m = s
        .load(b"<urn:a> <urn:p> 1.", RdfFormat::Turtle, None, None)
        .unwrap();
    assert_eq!((m.added, m.removed, m.revision), (1, 0, 1));
    let query = "SELECT ?o WHERE {<urn:a> <urn:p> ?o}";
    assert!(!s.query(query, true).unwrap().cache_hit);
    assert!(s.query(query, true).unwrap().cache_hit);
    let before = s.snapshot().unwrap();
    assert!(
        s.load(b"<urn:b> <urn:p> 2. <broken", RdfFormat::Turtle, None, None)
            .is_err()
    );
    assert_eq!(s.revision(), 1);
    assert_eq!(s.snapshot().unwrap().dataset_sha256, before.dataset_sha256);
    assert!(s.query(query, true).unwrap().cache_hit);
    let duplicate = s
        .load(b"<urn:a> <urn:p> 1.", RdfFormat::Turtle, None, None)
        .unwrap();
    assert_eq!((duplicate.added, duplicate.revision), (0, 2));
    assert!(!s.query(query, true).unwrap().cache_hit);
}
#[test]
fn document_blank_nodes_are_independent_and_named_dataset_graphs_are_preserved() {
    let mut s = DatasetSession::new();
    for _ in 0..2 {
        s.load(b"_:shared <urn:p> \"a\".", RdfFormat::Turtle, None, None)
            .unwrap();
    }
    assert_eq!(s.quad_count(), 2);
    s.load(
        b"GRAPH <urn:g> { _:shared <urn:p> \"b\" } GRAPH _:bg { _:shared <urn:q> \"c\" }",
        RdfFormat::TriG,
        None,
        None,
    )
    .unwrap();
    assert_eq!(s.quad_count(), 4);
    let q = s
        .query("SELECT ?g WHERE {GRAPH ?g {?s <urn:p> \"b\"}}", true)
        .unwrap();
    assert_eq!(q.result["rows"][0]["g"]["value"], "urn:g");
    assert!(s.serialize(RdfFormat::Turtle, None).is_err());
    assert!(s.serialize(RdfFormat::NTriples, None).is_err());
    assert!(
        s.load(
            b"<urn:a> <urn:p> <urn:b> <urn:g> .",
            RdfFormat::NQuads,
            None,
            Some("urn:override")
        )
        .is_err()
    );
    let whole = s.serialize(RdfFormat::NQuads, None).unwrap();
    assert_eq!(dataset(whole.as_bytes()).len(), 4);
    assert_eq!(
        dataset(
            s.serialize(RdfFormat::NTriples, Some("urn:g"))
                .unwrap()
                .as_bytes()
        )
        .len(),
        1
    );
}
#[test]
fn load_supports_base_iri_xml_nt_and_explicit_named_graph() {
    let mut s = DatasetSession::new();
    s.load(
        b"<part> <predicate> <value>.",
        RdfFormat::Turtle,
        Some("https://example.com/base/"),
        Some("urn:g"),
    )
    .unwrap();
    s.load(b"<urn:a> <urn:p> \"nt\" .", RdfFormat::NTriples, None, None)
        .unwrap();
    s.load(br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:ex="urn:test:"><rdf:Description rdf:about="urn:a"><ex:p>xml</ex:p></rdf:Description></rdf:RDF>"#, RdfFormat::RdfXml, None, None).unwrap();
    assert_eq!(s.quad_count(), 3);
    assert_eq!(
        s.query(
            "ASK {GRAPH <urn:g> {<https://example.com/base/part> ?p ?o}}",
            false
        )
        .unwrap()
        .result["value"],
        true
    );
}
#[test]
fn local_update_publishes_atomically_with_actual_added_removed_and_cache_revision() {
    let mut s = DatasetSession::new();
    s.load(b"<urn:a> <urn:p> 1.", RdfFormat::Turtle, None, None)
        .unwrap();
    let q = "ASK {<urn:a> <urn:p> 1}";
    s.query(q, true).unwrap();
    let result = s
        .update("DELETE DATA {<urn:a> <urn:p> 1}; INSERT DATA {<urn:a> <urn:p> 2}")
        .unwrap();
    assert_eq!(
        (
            result.added,
            result.removed,
            result.quad_count,
            result.revision
        ),
        (1, 1, 1, 2)
    );
    let current = s.query(q, true).unwrap();
    assert!(!current.cache_hit);
    assert_eq!(current.result["value"], false);
    assert_eq!(current.revision, 2);
    let frozen = s.snapshot().unwrap();
    for update in [
        "INSERT DATA {<urn:b> <urn:p> 3}; MALFORMED",
        "LOAD <https://example.com/data>",
        "INSERT {?s <urn:q> ?o} WHERE {SERVICE <https://example.com/s> {?s ?p ?o}}",
        "CREATE GRAPH <urn:empty>",
    ] {
        assert!(s.update(update).is_err(), "{update}");
        assert_eq!(s.revision(), 2);
        assert_eq!(s.snapshot().unwrap().dataset_sha256, frozen.dataset_sha256);
    }
}
#[test]
fn query_construct_cache_and_selected_clear_keep_other_graphs() {
    let mut s = DatasetSession::new();
    s.load(
        b"<urn:a> <urn:p> 1.",
        RdfFormat::Turtle,
        None,
        Some("urn:g"),
    )
    .unwrap();
    s.load(b"<urn:b> <urn:p> 2.", RdfFormat::Turtle, None, None)
        .unwrap();
    let q = "CONSTRUCT {?s <urn:q> ?o} WHERE {?s <urn:p> ?o}";
    assert!(!s.query(q, true).unwrap().cache_hit);
    assert!(s.query(q, true).unwrap().cache_hit);
    assert!(!s.query(q, false).unwrap().cache_hit);
    let cleared = s.clear(Some("urn:g")).unwrap();
    assert_eq!(
        (cleared.removed, cleared.quad_count, cleared.revision),
        (1, 1, 3)
    );
    assert!(!s.query(q, true).unwrap().cache_hit);
    assert_eq!(s.clear(None).unwrap().quad_count, 0);
}
#[test]
fn snapshots_and_diffs_reject_tampered_hashes_and_cross_algorithm_claims() {
    let before = snapshot(&dataset(b"<urn:a> <urn:p> <urn:b> .\n"), 0).unwrap();
    let after = snapshot(&dataset(b"<urn:a> <urn:p> <urn:c> .\n"), 1).unwrap();
    let diff = diff_snapshots(&before, &after).unwrap();
    assert_eq!((diff.added.len(), diff.removed.len()), (1, 1));
    let mut tampered = before.clone();
    tampered.dataset_sha256 = "0".repeat(64);
    assert!(diff_snapshots(&tampered, &after).is_err());
    let mut historical = before.clone();
    historical.canonicalization = "rdflib-old".into();
    assert!(diff_snapshots(&historical, &after).is_err());
}

#[test]
fn batch_load_has_one_public_mutation_and_document_scopes_with_atomic_failure() {
    let mut s = DatasetSession::new();
    let sources = [
        (
            b"_:shared <urn:p> \"a\".".as_slice(),
            RdfFormat::Turtle,
            None,
            None,
        ),
        (
            b"_:shared <urn:p> \"b\".".as_slice(),
            RdfFormat::Turtle,
            None,
            Some("urn:g"),
        ),
    ];
    let success = s.load_batch(&sources).unwrap();
    assert_eq!(
        (success.added, success.quad_count, success.revision),
        (2, 2, 1)
    );
    let before = s.snapshot().unwrap();
    let sources = [
        (
            b"<urn:c> <urn:p> \"c\".".as_slice(),
            RdfFormat::Turtle,
            None,
            None,
        ),
        (b"<broken".as_slice(), RdfFormat::Turtle, None, None),
    ];
    assert!(s.load_batch(&sources).is_err());
    assert_eq!(s.revision(), 1);
    assert_eq!(s.snapshot().unwrap().dataset_sha256, before.dataset_sha256);
    assert!(
        s.load_batch(&[]).is_err(),
        "an absent manifest load is not a successful batch"
    );
}

#[test]
fn store_term_normalization_is_never_published_as_lossless_update() {
    let mut s = DatasetSession::new();
    s.load(
        br#"<urn:a> <urn:p> "003"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger>."#,
        RdfFormat::Turtle,
        None,
        None,
    )
    .unwrap();
    let before = s.snapshot().unwrap();
    assert!(before.canonical_nquads.contains("\"003\""));
    assert!(s.update("INSERT DATA {<urn:b> <urn:p> 1}").is_err());
    assert_eq!(s.revision(), 1);
    assert_eq!(s.snapshot().unwrap().dataset_sha256, before.dataset_sha256);
}

#[test]
fn verified_package_registration_uses_one_identity_state_and_original_revision_rules() {
    let mut s = DatasetSession::new();
    let sources = [(
        b"<urn:a> <urn:p> 1.".as_slice(),
        RdfFormat::Turtle,
        None,
        None,
    )];
    let digest = "a".repeat(64);
    let first = s
        .load_verified_package("package", "1", &digest, &sources)
        .unwrap();
    assert_eq!(first["rdf_added"], 1);
    assert_eq!(first["revision"], 1);
    let duplicate = s
        .load_verified_package("package", "1", &digest, &sources)
        .unwrap();
    assert_eq!(duplicate["rdf_added"], 0);
    assert_eq!(duplicate["revision"], 1);
    assert!(
        s.load_verified_package("package", "1", &"b".repeat(64), &sources)
            .is_err()
    );
    assert_eq!(s.revision(), 1);
    assert_eq!(
        s.load_verified_package("metadata", "1", &digest, &[])
            .unwrap()["revision"],
        1
    );
    let empty = [(b"".as_slice(), RdfFormat::Turtle, None, None)];
    assert_eq!(
        s.load_verified_package("empty", "1", &digest, &empty)
            .unwrap()["revision"],
        1
    );
    s.clear(None).unwrap();
    let after_clear = s
        .load_verified_package("package", "1", &digest, &sources)
        .unwrap();
    assert_eq!(after_clear["revision"], 2);
    assert_eq!(after_clear["rdf_added"], 0);
    assert_eq!(s.quad_count(), 0);
    assert_eq!(
        s.load(b"", RdfFormat::Turtle, None, None).unwrap().revision,
        3,
        "ordinary empty load is still a successful mutation"
    );
}

#[test]
fn failed_package_staging_does_not_reserve_the_identity() {
    let mut s = DatasetSession::new();
    let bad = [
        (
            b"<urn:a> <urn:p> 1.".as_slice(),
            RdfFormat::Turtle,
            None,
            None,
        ),
        (b"<bad".as_slice(), RdfFormat::Turtle, None, None),
    ];
    assert!(
        s.load_verified_package("package", "1", &"a".repeat(64), &bad)
            .is_err()
    );
    assert_eq!((s.quad_count(), s.revision()), (0, 0));
    let good = [(
        b"<urn:a> <urn:p> 2.".as_slice(),
        RdfFormat::Turtle,
        None,
        None,
    )];
    assert_eq!(
        s.load_verified_package("package", "1", &"b".repeat(64), &good)
            .unwrap()["rdf_added"],
        1
    );
}

#[test]
fn blank_free_snapshot_matches_unoptimized_rdfc_for_heterogeneous_terms() {
    use oxrdf::dataset::{CanonicalizationAlgorithm, CanonicalizationHashAlgorithm};
    let graph = dataset(
        concat!(
            "<urn:机械> <urn:p> \"line\\nquote\\\"\" .\n",
            "<urn:b> <urn:p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> <urn:g> .\n",
            "<urn:a> <urn:p> \"français\"@fr .\n",
            "<urn:b> <urn:q> <urn:目标> <urn:g> .\n"
        )
        .as_bytes(),
    );
    let mut original_algorithm = graph.clone();
    original_algorithm.canonicalize(CanonicalizationAlgorithm::Rdfc10 {
        hash_algorithm: CanonicalizationHashAlgorithm::Sha256,
    });
    let mut rows = original_algorithm
        .iter()
        .map(|q| format!("{q} .\n"))
        .collect::<Vec<_>>();
    rows.sort();
    let expected = rows.concat();
    let result = snapshot(&graph, 7).unwrap();
    assert_eq!(result.canonical_nquads, expected);
    assert_eq!(result.dataset_sha256, semantica_runtime::sha256(&expected));
    assert_eq!(result.revision, 7);
}
