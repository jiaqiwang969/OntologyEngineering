//! Independent expectations frozen before production implementation.
use oxigraph::{io::RdfFormat, model::Dataset};
use semantica_contracts::*;
use semantica_runtime::{dataset::DatasetSession, provenance::*, sha256};
use serde_json::json;

const RAW: &str = "用户：孔距改为32 mm。\n助手：需重做强度校核。\n";
const SOURCE_SHA: &str = "c5dfa8fe77a805836a7b19754809508867dc8414792d625b60eec8bec9b8251c";
const SPAN_SHA: &str = "8fcbcc6dd2f10861bf0e432f3004a60db746f8bf072aa66a21ab53251db6f881";
const GRAPH: &str = "urn:example:prov:session:t1";
const EXPECTATIONS_SHA: &str = "dddd9041e8b6dec75252cd2e7348d4b98d26f7a451adb11316d8819fe3a37394";
fn binding() -> SemanticSourceBinding {
    serde_json::from_value(json!({"schema":"semantica.source-binding/1","source_id":"urn:example:message:1","source_sha256":SOURCE_SHA,"byte_count":64})).unwrap()
}
fn span() -> SemanticSourceSpan {
    serde_json::from_value(json!({"schema":"semantica.source-span/1","span_id":"urn:example:span:1","source_id":"urn:example:message:1","source_sha256":SOURCE_SHA,"start_byte":9,"end_byte":29,"span_sha256":SPAN_SHA,"quote":"孔距改为32 mm。"})).unwrap()
}
fn batch() -> SemanticProvenanceBatchRequest {
    serde_json::from_value(json!({
      "schema":"semantica.provenance-batch/1","graph_id":GRAPH,
      "sources":[{"binding":binding(),"data":RAW,"spans":[span()]}],
      "agents":[{"agent_id":"urn:example:agent:codex","kind":"software_agent"}],
      "activities":[{"activity_id":"urn:example:activity:extract:1","used":["urn:example:message:1"],
         "associations":[{"association_id":"urn:example:association:1","agent_id":"urn:example:agent:codex","role_id":"urn:example:role:extractor"}]}],
      "records":[{"record_id":"urn:example:candidate:1","subject_id":"urn:example:candidate:1","entity_kind":"entity",
         "generated_by":"urn:example:activity:extract:1","attributed_to":["urn:example:agent:codex"],
         "source_spans":["urn:example:span:1"],"derived_from":["urn:example:message:1"],"revision_of":[]}],
      "max_records":64,"max_quads":1000
    })).unwrap()
}
fn lookup(subject: &str) -> SemanticProvenanceLookup {
    serde_json::from_value(json!({"schema":"semantica.provenance-lookup/1","graph_id":GRAPH,"subject_id":subject,"max_quads":1000})).unwrap()
}
fn lineage() -> SemanticLineageRequest {
    serde_json::from_value(json!({"schema":"semantica.lineage-request/1","graph_id":GRAPH,"root":"urn:example:candidate:2","direction":"ancestors","edge_kinds":["derived_from","revision_of"],"max_levels":10,"max_nodes":100,"max_edges":100})).unwrap()
}
fn lineage_session() -> DatasetSession {
    let mut session = DatasetSession::new();
    session.load(
        b"<urn:example:candidate:2> <http://www.w3.org/ns/prov#wasDerivedFrom> <urn:example:message:2> <urn:example:prov:session:t1> .\n<urn:example:candidate:2> <http://www.w3.org/ns/prov#wasRevisionOf> <urn:example:candidate:1> <urn:example:prov:session:t1> .\n<urn:example:candidate:1> <http://www.w3.org/ns/prov#wasDerivedFrom> <urn:example:message:1> <urn:example:prov:session:t1> .\n<urn:example:message:1> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/prov#Entity> <urn:example:prov:session:t1> .\n<urn:example:message:2> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/ns/prov#Entity> <urn:example:prov:session:t1> .\n",
        RdfFormat::NQuads, None, None
    ).unwrap();
    session
}
fn error(error: anyhow::Error, code: &str) {
    assert!(
        error.to_string().contains(code),
        "{error:?} expected {code}"
    );
}
#[test]
fn frozen_expectations_and_oracle_bytes_are_unchanged() {
    assert_eq!(
        sha256(include_bytes!(
            "fixtures/provenance/frozen-expectations.json"
        )),
        EXPECTATIONS_SHA
    );
    let fixture = include_str!("fixtures/provenance/expected-ground-provenance.nq");
    assert_eq!(fixture.len(), 3294);
    assert_eq!(fixture.lines().count(), 24);
    assert_eq!(
        sha256(fixture),
        "3db1708c07b505d35b0b628974b01a3ad4ba8b320d1a3b8c78f9e592787b703b"
    );
}
#[test]
fn frozen_original_source_iri_is_verified_without_oracle_fixture_aliasing() {
    let frozen: serde_json::Value = serde_json::from_slice(include_bytes!(
        "fixtures/provenance/frozen-expectations.json"
    ))
    .unwrap();
    let source = &frozen["source"];
    let b:SemanticSourceBinding=serde_json::from_value(json!({"schema":"semantica.source-binding/1","source_id":source["source_id"],"source_sha256":source["source_sha256"],"byte_count":source["byte_count"]})).unwrap();
    let mut value = frozen["valid_span"].clone();
    value["schema"] = json!("semantica.source-span/1");
    value["span_id"] = json!("urn:example:span:original");
    let s: SemanticSourceSpan = serde_json::from_value(value).unwrap();
    let verified =
        verify_source_span(source["content"].as_str().unwrap().as_bytes(), &b, &s).unwrap();
    assert_eq!(
        verified.receipt().binding.source_id,
        "urn:example:session:t1:message:1"
    );
    assert_eq!(
        verified.receipt().span.source_id,
        verified.receipt().binding.source_id
    );
    let mut mismatched = s.clone();
    mismatched.source_id = "urn:example:message:1".into();
    error(
        verify_source_span(RAW.as_bytes(), &b, &mismatched)
            .err()
            .unwrap(),
        "span_source_binding_mismatch",
    );
}
#[test]
fn span_exact_utf8() {
    assert_eq!(RAW.len(), 64);
    let verified = verify_source_span(RAW.as_bytes(), &binding(), &span()).unwrap();
    assert_eq!(
        verified.receipt().profile,
        "semantica.source-utf8-span-sha256/1"
    );
    assert_eq!(verified.receipt().engineering_status, "unknown");
    assert_eq!(verified.receipt().span.span_sha256, SPAN_SHA);
}
#[test]
fn source_byte_change() {
    error(
        verify_source_span(RAW.replace("32", "35").as_bytes(), &binding(), &span())
            .err()
            .unwrap(),
        "source_sha_mismatch",
    );
}
#[test]
fn inside_utf8_byte() {
    let mut s = span();
    s.start_byte = 10;
    error(
        verify_source_span(RAW.as_bytes(), &binding(), &s)
            .err()
            .unwrap(),
        "utf8_boundary",
    );
}
#[test]
fn quote_not_at_range() {
    let mut s = span();
    s.quote = Some("孔距改为35 mm。".into());
    error(
        verify_source_span(RAW.as_bytes(), &binding(), &s)
            .err()
            .unwrap(),
        "quote_mismatch",
    );
}
#[test]
fn oversized_span() {
    let mut s = span();
    s.end_byte = 65;
    error(
        verify_source_span(RAW.as_bytes(), &binding(), &s)
            .err()
            .unwrap(),
        "span_range",
    );
}
#[test]
fn duplicate_record_id_different_content() {
    let mut session = DatasetSession::new();
    load_records(&mut session, &batch()).unwrap();
    let before = session.snapshot().unwrap();
    let q = "ASK {GRAPH <urn:example:prov:session:t1> {?s ?p ?o}}";
    session.query(q, true).unwrap();
    assert!(session.query(q, true).unwrap().cache_hit);
    let mut b = batch();
    b.records[0].record_id = "urn:new:record".into();
    b.records[0].subject_id = "urn:new:entity".into();
    b.records.push(b.records[0].clone());
    b.records[1].metadata = Some(json!({"different":true}));
    error(
        load_records(&mut session, &b).unwrap_err(),
        "duplicate_record",
    );
    assert_eq!(session.snapshot().unwrap(), before);
    assert!(session.query(q, true).unwrap().cache_hit);
    error(
        load_records(&mut session, &batch()).unwrap_err(),
        "existing_record",
    );
    assert_eq!(session.snapshot().unwrap(), before);
}
#[test]
fn qualified_role_attaches_activity() {
    let mut session = DatasetSession::new();
    load_records(&mut session, &batch()).unwrap();
    assert_eq!(session.query("ASK {GRAPH <urn:example:prov:session:t1> {<urn:example:activity:extract:1> <http://www.w3.org/ns/prov#qualifiedAssociation> <urn:example:association:1>}}",false).unwrap().result["value"],true);
    assert_eq!(session.query("ASK {GRAPH <urn:example:prov:session:t1> {<urn:example:candidate:1> <http://www.w3.org/ns/prov#qualifiedAssociation> ?a}}",false).unwrap().result["value"],false);
}
#[test]
fn derivation_and_revision_are_distinct() {
    let session = lineage_session();
    let all = lineage_view(&session, &lineage()).unwrap();
    assert_eq!(
        all.nodes,
        vec![
            "urn:example:candidate:1",
            "urn:example:candidate:2",
            "urn:example:message:1",
            "urn:example:message:2"
        ]
    );
    assert_eq!(all.coverage, "complete");
    assert!(all.edges.iter().any(|edge| edge.kind == "revision_of"));
    let mut request = lineage();
    request.edge_kinds = vec!["derived_from".into()];
    let derived = lineage_view(&session, &request).unwrap();
    assert_eq!(
        derived.nodes,
        vec!["urn:example:candidate:2", "urn:example:message:2"]
    );
    let mut reverse = lineage();
    reverse.root = "urn:example:message:1".into();
    reverse.direction = "descendants".into();
    let descendants = descendants_view(&session, &reverse).unwrap();
    assert_eq!(
        descendants.nodes,
        vec![
            "urn:example:candidate:1",
            "urn:example:candidate:2",
            "urn:example:message:1"
        ]
    );
    assert!(
        !descendants.cycle,
        "converging acyclic edges are not cycles"
    );
}
#[test]
fn invalidation_retains_original() {
    let mut session = DatasetSession::new();
    load_records(&mut session, &batch()).unwrap();
    let original = session.snapshot().unwrap();
    let request:SemanticProvenanceInvalidationRequest=serde_json::from_value(json!({"schema":"semantica.provenance-invalidation/1","graph_id":GRAPH,"subject_id":"urn:example:candidate:1","record_id":"urn:example:candidate:1","activity_id":"urn:example:activity:withdraw:1","agent_id":"urn:example:agent:codex","reason":"新工况不适用","max_quads":1000})).unwrap();
    load_invalidation(&mut session, &request).unwrap();
    let current = session.snapshot().unwrap();
    for line in original.canonical_nquads.lines() {
        assert!(current.canonical_nquads.lines().any(|new| new == line));
    }
    let view = record_view(&session, &lookup("urn:example:candidate:1")).unwrap();
    assert_eq!(
        view.invalidation_activities,
        vec!["urn:example:activity:withdraw:1"]
    );
    assert_eq!(
        record_view(&session, &lookup("urn:example:message:1"))
            .unwrap()
            .execution,
        "completed"
    );
    assert!(
        source_view(&session, &lookup("urn:example:candidate:1"))
            .unwrap()
            .related_entities
            .contains(&"urn:example:message:1".into())
    );
    error(
        load_invalidation(&mut session, &request).unwrap_err(),
        "existing_activity",
    );
}
#[test]
fn missing_origin() {
    let mut session = lineage_session();
    session
        .update(
            "DELETE WHERE {GRAPH <urn:example:prov:session:t1> {<urn:example:message:2> ?p ?o}}",
        )
        .unwrap();
    let result = lineage_view(&session, &lineage()).unwrap();
    assert_eq!(result.missing_references, vec!["urn:example:message:2"]);
    assert_eq!(result.coverage, "partial");
    assert_eq!(result.execution, "partial");
    assert_eq!(
        record_view(&session, &lookup("urn:missing"))
            .unwrap()
            .execution,
        "not_found"
    );
}
#[test]
fn lineage_cycle() {
    let mut session = lineage_session();
    session.load(b"<urn:example:message:1> <http://www.w3.org/ns/prov#wasDerivedFrom> <urn:example:candidate:2> <urn:example:prov:session:t1> .",RdfFormat::NQuads,None,None).unwrap();
    let result = lineage_view(&session, &lineage()).unwrap();
    assert!(result.cycle);
    assert_eq!(result.nodes.len(), 4);
    assert_eq!(
        result.coverage, "complete",
        "a finite cycle can be completely enumerated"
    );
}
#[test]
fn max_levels1() {
    let mut request = lineage();
    request.max_levels = 1;
    let result = lineage_view(&lineage_session(), &request).unwrap();
    assert_eq!(result.nodes, vec!["urn:example:candidate:2"]);
    assert_eq!(result.coverage, "partial");
    assert!(!result.frontier.is_empty());
}
#[test]
fn maximum_nodes_exceeded() {
    let mut request = lineage();
    request.max_nodes = 1;
    let result = lineage_view(&lineage_session(), &request).unwrap();
    assert_eq!(result.nodes, vec!["urn:example:candidate:2"]);
    assert_eq!(result.coverage, "partial");
    assert!(!result.frontier.is_empty());
}
#[test]
fn history_not_physical_cause() {
    let result = lineage_view(&lineage_session(), &lineage()).unwrap();
    assert_eq!(result.causal_validation, "not_validated");
}
#[test]
fn asserted_success_or_confidence1() {
    let mut b = batch();
    b.records[0].confidence = Some(1.0);
    b.records[0].metadata = Some(json!({"status":"passed","adopted":true}));
    let immutable_project = json!({"decision":"unknown","adopted":false});
    let before = immutable_project.clone();
    let projection = prepare_records(&Dataset::new(), &b).unwrap();
    assert_eq!(projection.engineering_status, "unknown");
    assert_eq!(immutable_project, before);
    assert!(
        projection.nquads.contains("adopted"),
        "keep original metadata without granting it authority"
    );
}
#[test]
fn prov_projection_is_order_invariant() {
    let mut session = DatasetSession::new();
    load_records(&mut session, &batch()).unwrap();
    let snapshot = session.snapshot().unwrap();
    assert_eq!(snapshot.quad_count, 24);
    assert_eq!(
        snapshot.canonical_nquads,
        include_str!("fixtures/provenance/expected-ground-provenance.nq")
    );
    assert_eq!(
        snapshot.dataset_sha256,
        "3db1708c07b505d35b0b628974b01a3ad4ba8b320d1a3b8c78f9e592787b703b"
    );
    let mut request = batch();
    request.activities.reverse();
    request.agents.reverse();
    request.sources.reverse();
    request.records.reverse();
    let mut reordered = DatasetSession::new();
    load_records(&mut reordered, &request).unwrap();
    assert_eq!(
        reordered.snapshot().unwrap().dataset_sha256,
        snapshot.dataset_sha256
    );
    let port = RustProvenanceRuntime;
    assert_eq!(
        port.project(&request).unwrap().nquads,
        snapshot.canonical_nquads
    );
}
#[test]
fn quote_or_entity_id_tamper() {
    let original = prepare_records(&Dataset::new(), &batch()).unwrap();
    let mut b = batch();
    b.records[0].subject_id = "urn:other:entity".into();
    assert_ne!(
        prepare_records(&Dataset::new(), &b)
            .unwrap()
            .record_identities[0]
            .content_sha256,
        original.record_identities[0].content_sha256
    );
    let mut b = batch();
    b.sources[0].spans[0].quote = None;
    assert_ne!(
        prepare_records(&Dataset::new(), &b)
            .unwrap()
            .record_identities[0]
            .content_sha256,
        original.record_identities[0].content_sha256
    );
    let mut b = batch();
    b.sources[0].spans[0].quote = Some("forged".into());
    assert!(prepare_records(&Dataset::new(), &b).is_err());
}
#[test]
fn public_proof_json_cannot_authorize_unverified_sources() {
    let mut value = serde_json::to_value(batch()).unwrap();
    value["sources"][0]["data"] = json!(RAW.replace("32", "35"));
    value["sources"][0]["proof"] = json!({"passed":true,"source_sha256":SOURCE_SHA});
    assert!(serde_json::from_value::<SemanticProvenanceBatchRequest>(value).is_err());
    let mut b = batch();
    b.sources[0].data = RAW.replace("32", "35");
    assert!(prepare_records(&Dataset::new(), &b).is_err());
    let mut forged = serde_json::to_value(
        verify_source_span(RAW.as_bytes(), &binding(), &span())
            .unwrap()
            .receipt(),
    )
    .unwrap();
    forged["engineering_status"] = json!("passed");
    let receipt: SemanticSourceSpanProof = serde_json::from_value(forged).unwrap();
    assert!(receipt.validate().is_err());
    // VerifiedSourceSpan has private fields and no Deserialize implementation.
}
#[test]
fn malformed_identity_budgets_unknown_fields_and_confidence_fail_closed() {
    for mutate in 0..5 {
        let mut b = batch();
        match mutate {
            0 => b.graph_id = "relative".into(),
            1 => b.max_quads = 0,
            2 => b.records[0].confidence = Some(f64::NAN),
            3 => b.records[0].source_spans = vec!["urn:unverified".into()],
            _ => b.sources[0].binding.byte_count = 65,
        };
        assert!(prepare_records(&Dataset::new(), &b).is_err(), "{mutate}");
    }
    let mut value = serde_json::to_value(batch()).unwrap();
    value["adopted"] = json!(true);
    assert!(serde_json::from_value::<SemanticProvenanceBatchRequest>(value).is_err());
}
#[test]
fn convergent_dag_is_not_cycle_and_cross_graph_edges_are_not_followed() {
    let mut session = lineage_session();
    session.load(b"<urn:example:candidate:2> <http://www.w3.org/ns/prov#wasDerivedFrom> <urn:example:message:1> <urn:example:prov:session:t1> .\n<urn:example:message:1> <http://www.w3.org/ns/prov#wasDerivedFrom> <urn:other> <urn:other:graph> .",RdfFormat::NQuads,None,None).unwrap();
    let result = lineage_view(&session, &lineage()).unwrap();
    assert!(!result.cycle);
    assert!(!result.nodes.contains(&"urn:other".into()));
}
#[test]
fn multi_source_record_and_metadata_order_is_invariant() {
    let mut request = batch();
    request.agents.push(SemanticProvenanceAgent {
        agent_id: "urn:other:agent".into(),
        kind: "person".into(),
    });
    request.activities[0]
        .associations
        .push(SemanticProvenanceAssociation {
            association_id: "urn:other:association".into(),
            agent_id: "urn:other:agent".into(),
            role_id: None,
        });
    request.sources.push(SemanticProvenanceSource {
        binding: SemanticSourceBinding {
            schema: "semantica.source-binding/1".into(),
            source_id: "urn:other:source".into(),
            source_sha256: sha256("other\n"),
            byte_count: 6,
        },
        data: "other\n".into(),
        spans: Vec::new(),
    });
    request.activities[0].used.push("urn:other:source".into());
    let mut next = request.records[0].clone();
    next.record_id = "urn:other:record".into();
    next.subject_id = "urn:other:subject".into();
    next.source_spans.clear();
    next.derived_from = vec!["urn:other:source".into()];
    next.metadata = Some(json!({"b":"second","a":{"z":1,"x":2}}));
    request.records.push(next);
    let mut first = DatasetSession::new();
    load_records(&mut first, &request).unwrap();
    request.sources.reverse();
    request.records.reverse();
    request.agents.reverse();
    request.activities[0].associations.reverse();
    request.activities[0].used.reverse();
    request.records[0].metadata = Some(json!({"a":{"x":2,"z":1},"b":"second"}));
    let mut second = DatasetSession::new();
    load_records(&mut second, &request).unwrap();
    assert_eq!(
        first.snapshot().unwrap().dataset_sha256,
        second.snapshot().unwrap().dataset_sha256
    );
    assert_eq!(
        record_view(&second, &lookup("urn:other:record"))
            .unwrap()
            .execution,
        "completed"
    );
}
#[test]
fn invalidation_cannot_relabel_plain_source_as_a_record_and_failed_load_preserves_state() {
    let mut session = DatasetSession::new();
    load_records(&mut session, &batch()).unwrap();
    let before = session.snapshot().unwrap();
    let request:SemanticProvenanceInvalidationRequest=serde_json::from_value(json!({"schema":"semantica.provenance-invalidation/1","graph_id":GRAPH,"subject_id":"urn:example:message:1","record_id":"urn:example:message:1","activity_id":"urn:bad:withdraw","agent_id":"urn:example:agent:codex","reason":"source is not event record","max_quads":1000})).unwrap();
    error(
        load_invalidation(&mut session, &request).unwrap_err(),
        "record_mismatch",
    );
    assert_eq!(session.snapshot().unwrap(), before);
    let mut changed = batch();
    changed.records[0].record_id = "urn:next:record".into();
    changed.records[0].subject_id = "urn:next:subject".into();
    changed.sources[0].spans[0].quote = Some("changed".into());
    assert!(load_records(&mut session, &changed).is_err());
    assert_eq!(session.snapshot().unwrap(), before);
}
#[test]
fn time_confidence_metadata_and_edge_budget_are_explicit() {
    let mut request = batch();
    request.activities[0].started_at = Some("2026-10-02T14:00:00Z".into());
    request.activities[0].ended_at = Some("2026-10-02T13:00:00Z".into());
    error(
        prepare_records(&Dataset::new(), &request).unwrap_err(),
        "time_order",
    );
    request.activities[0].started_at = Some("2026-10-02T12:00:00".into());
    error(
        prepare_records(&Dataset::new(), &request).unwrap_err(),
        "timezone_missing",
    );
    let mut request = batch();
    let mut value = json!(null);
    for _ in 0..97 {
        value = json!([value]);
    }
    request.records[0].metadata = Some(value);
    error(
        prepare_records(&Dataset::new(), &request).unwrap_err(),
        "metadata_structure_budget",
    );
    let mut request = lineage();
    request.max_edges = 1;
    let result = lineage_view(&lineage_session(), &request).unwrap();
    assert_eq!(result.coverage, "partial");
    assert_eq!(result.edges.len(), 1);
    assert!(!result.frontier.is_empty());
}
