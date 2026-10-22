//! Source graph-term/variable transport, never graph execution qualification.
//! Root-authored receivers compare candidate objects to before-candidate
//! original observations; no new independent review is claimed.
use super::source_exists_runtime_receiving::{Backend, case, session};
use super::*;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, PreparedSourceLiteralRegistry, SourceFact,
    SourceLiteralCacheKind,
};
use oxrdf::NamedOrBlankNode;
use semantica_numeric::{NumericLimits, NumericValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spargebra::algebra::GraphPattern as P;
use spargebra::term::{TermPattern, TriplePattern};
use spargebra::{ParsedSourceLiteral, SourceDatatype, SparqlParser};
use std::collections::BTreeSet;

const FIXTURE: &str = include_str!("fixtures/source_graph_terms_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "b2098e49be38f400d814163d4e47e702ff53e267fd6e2f6a2c54e1c8dcd0c2db"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "0554a023b7d7ee4765b42e33692bf51fd63bfe280636af98da1a2bf4eebf0382"
    );
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(value["cases"].as_array().unwrap().len(), 19);
    value
}
fn input(id: &str) -> Value {
    fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["id"] == id)
        .unwrap()
        .clone()
}
fn parse(
    query: &str,
) -> (
    P,
    Rc<QueryNumericSession>,
    Rc<PreparedSourceLiteralRegistry>,
) {
    let session = session(&case("EX01"), NumericLimits::default());
    let registry = PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(&session));
    let (query, _) = SparqlParser::new()
        .parse_query_rdflib760_source_exists_frontend(query, registry.hooks(), true)
        .unwrap();
    let spargebra::Query::Select { pattern, .. } = query else {
        panic!("SELECT fixture");
    };
    (pattern, session, registry)
}
fn bgps(pattern: &P, out: &mut Vec<Vec<TriplePattern>>) {
    match pattern {
        P::Bgp { patterns } => out.push(patterns.clone()),
        P::Project { inner, .. }
        | P::Filter { inner, .. }
        | P::Extend { inner, .. }
        | P::SourceContext { inner, .. }
        | P::Graph { inner, .. }
        | P::Distinct { inner }
        | P::Reduced { inner }
        | P::Slice { inner, .. } => bgps(inner, out),
        P::Join { left, right }
        | P::Union { left, right }
        | P::Minus { left, right }
        | P::LeftJoin { left, right, .. } => {
            bgps(left, out);
            bgps(right, out);
        }
        _ => panic!("unreceived graph shape {pattern:?}"),
    }
}
fn graph_literals(pattern: &P) -> Vec<ParsedSourceLiteral> {
    let mut blocks = Vec::new();
    bgps(pattern, &mut blocks);
    blocks
        .into_iter()
        .flatten()
        .flat_map(|t| [t.subject, t.object])
        .filter_map(|term| match term {
            TermPattern::SourceLiteral(literal) => Some(literal),
            TermPattern::Literal(_) => panic!("source graph occurrence was erased"),
            _ => None,
        })
        .collect()
}
fn literal_json(
    literal: &ParsedSourceLiteral,
    session: &QueryNumericSession,
    registry: &PreparedSourceLiteralRegistry,
) -> Value {
    assert!(literal.occurrence.is_some());
    let raw = registry.resolve(literal).unwrap();
    assert!(raw.same_source_object(&registry.resolve(&literal.clone()).unwrap()));
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!("source metadata");
    };
    let datatype = match &metadata.datatype {
        SourceDatatype::Absent => Value::Null,
        SourceDatatype::Specified(node) => json!(node.as_str()),
        SourceDatatype::Unknown => panic!("unknown source datatype"),
    };
    let SourceFact::Known(language) = &metadata.language_original else {
        panic!("unknown language");
    };
    let SourceFact::Known(ill_typed) = metadata.ill_typed else {
        panic!("unknown ill_typed");
    };
    let (kind, cache) = match (&metadata.cached_value_kind, raw.numeric()) {
        (SourceFact::Known(SourceLiteralCacheKind::String), NumericCacheState::NotNumeric) => {
            ("builtins.str", json!(literal.rdf.value()))
        }
        (SourceFact::Known(SourceLiteralCacheKind::None), _) => {
            assert!(!matches!(raw.numeric(), NumericCacheState::Known(_)));
            ("builtins.NoneType", Value::Null)
        }
        (
            SourceFact::Known(SourceLiteralCacheKind::Boolean(value)),
            NumericCacheState::NotNumeric,
        ) => ("builtins.bool", json!(value)),
        (SourceFact::Known(SourceLiteralCacheKind::Numeric), NumericCacheState::Known(value)) => {
            match value.value() {
                NumericValue::Integer(value) => (
                    "builtins.int",
                    serde_json::from_str(&value.to_string()).unwrap(),
                ),
                _ => panic!("unreceived graph literal cache family"),
            }
        }
        _ => panic!("unreceived graph source cache"),
    };
    json!({"lexical": literal.rdf.value(), "datatype": datatype, "language": language,
        "ill_typed": ill_typed, "cache_type": kind, "cache": cache,
        "repr": session.source_repr(&SolutionValue::Rdf(raw.clone())).unwrap(),
        "hash": session.source_hash().unwrap().term_hash(&raw).unwrap()})
}
fn receive(id: &str) {
    let expected = input(id);
    let (pattern, session, registry) = parse(expected["query"].as_str().unwrap());
    let mut actual = Vec::new();
    bgps(&pattern, &mut actual);
    assert_eq!(actual.len(), expected["bgps"].as_array().unwrap().len());
    for (triples, wanted) in actual.iter().zip(expected["bgps"].as_array().unwrap()) {
        let mut variables = BTreeSet::new();
        GraphPattern::from(&P::Bgp {
            patterns: triples.clone(),
        })
        .lookup_used_variables(&mut |variable| {
            variables.insert(variable.as_str().to_owned());
        });
        assert_eq!(
            json!(variables.into_iter().collect::<Vec<_>>()),
            wanted["variables"]
        );
        let literals: Vec<_> = triples
            .iter()
            .flat_map(|t| [&t.subject, &t.object])
            .filter_map(|term| match term {
                TermPattern::SourceLiteral(literal) => {
                    Some(literal_json(literal, &session, &registry))
                }
                TermPattern::Literal(_) => panic!("ordinary RDF erased source occurrence"),
                _ => None,
            })
            .collect();
        assert_eq!(json!(literals), wanted["literals"]);
    }
    let mut root = &pattern;
    while let P::SourceContext { inner, .. } = root {
        root = inner;
    }
    let P::Project { variables, .. } = root else {
        panic!("actual projection");
    };
    let members: BTreeSet<_> = variables.iter().map(Variable::as_str).collect();
    assert_eq!(
        json!(members.into_iter().collect::<Vec<_>>()),
        expected["projection_members"]
    );
    // The existing physical planner must preserve actual occurrence handles.
    let lowered = GraphPattern::from(&pattern);
    let restored = P::from(&lowered);
    let original = graph_literals(&pattern);
    assert_eq!(graph_literals(&restored), original);
    for literal in original {
        assert!(registry.resolve(&literal).is_ok());
    }
}

#[test]
fn source_graph_terms_original_gt01() {
    receive("GT01");
}
#[test]
fn source_graph_terms_original_gt02() {
    receive("GT02");
}
#[test]
fn source_graph_terms_original_gt03() {
    receive("GT03");
}
#[test]
fn source_graph_terms_original_gt04() {
    receive("GT04");
}
#[test]
fn source_graph_terms_original_gt05() {
    receive("GT05");
}
#[test]
fn source_graph_terms_original_gt06() {
    receive("GT06");
}
#[test]
fn source_graph_terms_original_gt07() {
    receive("GT07");
}
#[test]
fn source_graph_terms_original_gt08() {
    receive("GT08");
}
#[test]
fn source_graph_terms_original_gt12() {
    receive("GT12");
}
#[test]
fn source_graph_terms_original_gt15() {
    receive("GT15");
}
#[test]
fn source_graph_terms_original_gt16() {
    receive("GT16");
}
#[test]
fn source_graph_terms_original_gt17() {
    receive("GT17");
}
#[test]
fn source_graph_terms_original_gt18() {
    receive("GT18");
}
#[test]
fn source_graph_terms_original_gt19() {
    receive("GT19");
}
#[test]
fn source_graph_terms_original_gt20() {
    receive("GT20");
}
#[test]
fn source_graph_terms_original_gt21() {
    receive("GT21");
}
#[test]
fn source_graph_terms_original_gt22() {
    receive("GT22");
}
#[test]
fn source_graph_terms_original_gt23() {
    receive("GT23");
}
#[test]
fn source_graph_terms_original_gt24() {
    receive("GT24");
}

#[test]
fn source_graph_terms_equal_rdf_distinct_source_construction() {
    let (pattern, session, registry) = parse(input("GT06")["query"].as_str().unwrap());
    let values = graph_literals(&pattern);
    assert_eq!(values.len(), 2);
    let a = registry.resolve(&values[0]).unwrap();
    let b = registry.resolve(&values[1]).unwrap();
    assert_eq!(a.rdf().unwrap(), b.rdf().unwrap());
    assert_ne!(values[0], values[1]);
    assert!(!a.same_source_object(&b));
    assert_ne!(
        session.source_hash().unwrap().term_hash(&a).unwrap(),
        session.source_hash().unwrap().term_hash(&b).unwrap()
    );
}
#[test]
fn source_graph_terms_duplicate_occurrences_keep_actual_owner() {
    let (pattern, _, registry) = parse("SELECT * WHERE { ?s <urn:p> \"a\", \"a\" }");
    let values = graph_literals(&pattern);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].rdf, values[1].rdf);
    assert_ne!(values[0].occurrence, values[1].occurrence);
    let a = registry.resolve(&values[0]).unwrap();
    let b = registry.resolve(&values[1]).unwrap();
    assert!(!a.same_source_object(&b));
    assert!(a.same_source_object(&registry.resolve(&values[0].clone()).unwrap()));
}
#[test]
fn source_graph_terms_foreign_and_tampered_occurrences_refused() {
    let query = input("GT03")["query"].as_str().unwrap().to_owned();
    let (a, _, owner) = parse(&query);
    let (_, _, foreign) = parse(&query);
    let literal = graph_literals(&a).remove(0);
    assert!(matches!(
        foreign.resolve(&literal),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    let mut changed = literal.clone();
    changed.rdf = Literal::new_simple_literal("forged");
    assert!(matches!(
        owner.resolve(&changed),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    changed = literal;
    changed.occurrence = None;
    assert!(matches!(
        owner.resolve(&changed),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
}
#[test]
fn source_graph_terms_rdf_conversion_refused_ground_transport_preserved() {
    let (p, _, registry) = parse(input("GT03")["query"].as_str().unwrap());
    let literal = graph_literals(&p).remove(0);
    let term = TermPattern::SourceLiteral(literal.clone());
    assert!(Term::try_from(term.clone()).is_err());
    assert!(NamedOrBlankNode::try_from(term.clone()).is_err());
    let ground = GroundTermPattern::try_from(term.clone()).unwrap();
    assert_eq!(TermPattern::from(ground.clone()), term);
    let Expression::SourceLiteral(back) = Expression::from(ground) else {
        panic!("expression lost occurrence");
    };
    assert!(
        registry
            .resolve(&back)
            .unwrap()
            .same_source_object(&registry.resolve(&literal).unwrap())
    );
}
#[test]
fn source_graph_terms_legacy_parser_keeps_physical_literals() {
    let query = input("GT03")["query"].as_str().unwrap().to_owned();
    let spargebra::Query::Select { pattern, .. } = SparqlParser::new().parse_query(&query).unwrap()
    else {
        panic!("SELECT");
    };
    let mut blocks = Vec::new();
    bgps(&pattern, &mut blocks);
    assert!(matches!(blocks[0][0].object, TermPattern::Literal(_)));
    assert!(Term::try_from(blocks[0][0].object.clone()).is_ok());
}
#[test]
fn source_graph_terms_syntax_variable_lookup_expands_existing_container() {
    let query = "SELECT ?e WHERE { BIND(EXISTS { ?s ?p ?o } AS ?e) }";
    let (pattern, _, _) = parse(query);
    let lowered = GraphPattern::from(&pattern);
    let Expression::SourceExists(expression) =
        super::source_exists_runtime_receiving::first_source_expression(&lowered)
    else {
        panic!("EXISTS");
    };
    let mut vars = BTreeSet::new();
    expression.syntax.lookup_used_variables(&mut |v| {
        vars.insert(v.as_str().to_owned());
    });
    assert_eq!(vars, BTreeSet::from(["s".into(), "p".into(), "o".into()]));
}
#[test]
fn source_graph_terms_execution_requires_actual_dataset_admission() {
    let backend = Backend::default();
    let session = session(&case("EX01"), NumericLimits::default());
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        input("GT03")["query"].as_str().unwrap(),
        backend.clone(),
        session,
        Vec::new(),
    )
    .unwrap();
    let error = match prepared.execute_source_presence() {
        Err(e) => e,
        Ok(_) => panic!("unqualified graph execution"),
    };
    let QueryEvaluationError::Unexpected(error) = error else {
        panic!("fatal gate");
    };
    assert!(matches!(
        error.downcast_ref::<RawNumericFault>(),
        Some(RawNumericFault::UnsupportedSourceLiteralOperator(
            "graph literal identity"
        ))
    ));
    assert_eq!(backend.scans.get(), 0);
}

#[test]
fn source_graph_terms_scope_original_gt11() {
    use spargebra::SourceSyntaxValue as V;
    let expected = fixture();
    let cases = expected["syntax_mapping_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    let (pattern, _, _) = parse(cases[0]["query"].as_str().unwrap());
    let lowered = GraphPattern::from(&pattern);
    let Expression::SourceExists(expression) =
        super::source_exists_runtime_receiving::first_source_expression(&lowered)
    else {
        panic!("actual source EXISTS");
    };
    let syntax = &expression.syntax;
    let root = syntax.root_node().unwrap();
    let V::Node(graph) = &root
        .fields
        .iter()
        .find(|(key, _)| key == "graph")
        .unwrap()
        .1
    else {
        panic!("actual mapping graph");
    };
    let group = &syntax.nodes()[*graph];
    let V::List(parts) = &group
        .fields
        .iter()
        .find(|(key, _)| key == "part")
        .unwrap()
        .1
    else {
        panic!("actual graph parts");
    };
    let V::Node(block) = &parts[0] else {
        panic!("actual triple block");
    };
    for node in [group, &syntax.nodes()[*block]] {
        let want = cases
            .iter()
            .find(|value| value["name"] == node.name)
            .unwrap();
        let members = node.attributes.variables.as_ref().unwrap();
        assert_eq!(
            json!(members.iter().map(Variable::as_str).collect::<Vec<_>>()),
            want["variables"]
        );
        let V::VariableSet(iteration) = &node
            .fields
            .iter()
            .find(|(key, _)| key == "_vars")
            .unwrap()
            .1
        else {
            panic!("actual source set owner");
        };
        assert_eq!(
            json!(iteration.iter().map(Variable::as_str).collect::<Vec<_>>()),
            want["variable_set_iteration"]
        );
    }
}
