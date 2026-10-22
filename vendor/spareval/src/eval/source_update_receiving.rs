//! Independent original preparation checks. These do not qualify execution.
use super::{source_bridge, source_update};
use crate::numeric::{
    LiteralSourceState, PreparedSourceLiteralRegistry, SourceDatatype, SourceFact,
};
use serde_json::{Value, json};
use spargebra::algebra::{GraphPattern, GraphTarget, SourceContextAnnotation};
use spargebra::term::{GraphName, GraphNamePattern, NamedNodePattern, TermPattern};
use spargebra::{SourceUpdateData, SourceUpdateOperationKind as Op};
use std::collections::BTreeMap;

fn fixture() -> Value {
    serde_json::from_str(crate::private_test_fixtures::text_ref!(
        "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-update-original.json"
    ))
    .unwrap()
}
fn atom(kind: &str, value: &str) -> Value {
    json!({"type":kind,"value":value})
}
fn named(v: &NamedNodePattern) -> Value {
    match v {
        NamedNodePattern::NamedNode(v) => atom("URIRef", v.as_str()),
        NamedNodePattern::Variable(v) => atom("Variable", v.as_str()),
    }
}
fn term(v: &TermPattern, registry: &PreparedSourceLiteralRegistry) -> Value {
    match v {
        TermPattern::NamedNode(v) => atom("URIRef", v.as_str()),
        TermPattern::BlankNode(v) => atom("BNode", v.as_str()),
        TermPattern::Variable(v) => atom("Variable", v.as_str()),
        TermPattern::SourceLiteral(v) => {
            let raw = registry.resolve(v).unwrap();
            let oxrdf::Term::Literal(rdf) = raw.rdf().unwrap() else {
                panic!("literal changed RDF kind")
            };
            let LiteralSourceState::Literal(meta) = raw.literal_source() else {
                panic!("missing source identity")
            };
            let dt = match &meta.datatype {
                SourceDatatype::Absent => None,
                SourceDatatype::Specified(v) => Some(v.as_str()),
                SourceDatatype::Unknown => panic!("unknown source datatype"),
            };
            let SourceFact::Known(lang) = &meta.language_original else {
                panic!("unknown source language")
            };
            json!({"type":"Literal","value":rdf.value(),"datatype":dt,"language":lang})
        }
        _ => panic!("native literal bypassed source preparation"),
    }
}
fn graph(v: &GraphNamePattern) -> Value {
    match v {
        GraphNamePattern::DefaultGraph => json!("DEFAULT"),
        GraphNamePattern::NamedNode(v) => atom("URIRef", v.as_str()),
        GraphNamePattern::Variable(v) => atom("Variable", v.as_str()),
    }
}
fn data(v: &SourceUpdateData, registry: &PreparedSourceLiteralRegistry) -> Value {
    let triples = |v: &[spargebra::term::TriplePattern]| {
        v.iter()
            .map(|t| {
                json!([
                    term(&t.subject, registry),
                    named(&t.predicate),
                    term(&t.object, registry)
                ])
            })
            .collect::<Vec<_>>()
    };
    json!({"triples":triples(&v.triples),"quads":{"entries":v.quads.iter().map(|(g,t)|json!([graph(g),triples(t)])).collect::<Vec<_>>()}})
}
fn target(v: &GraphTarget) -> Value {
    match v {
        GraphTarget::NamedNode(v) => atom("URIRef", v.as_str()),
        GraphTarget::DefaultGraph => json!("DEFAULT"),
        GraphTarget::NamedGraphs => json!("NAMED"),
        GraphTarget::AllGraphs => json!("ALL"),
    }
}
fn graph_or_default(v: &GraphName) -> Value {
    match v {
        GraphName::DefaultGraph => json!("DEFAULT"),
        GraphName::NamedNode(v) => atom("URIRef", v.as_str()),
    }
}
fn operation(v: &Op, registry: &PreparedSourceLiteralRegistry) -> Value {
    let silent = |name: &str, enabled: bool, mut fields: Value| {
        if enabled {
            fields["silent"] = json!("SILENT");
        }
        json!({"name":name,"fields":fields})
    };
    match v {
        Op::InsertData(v) => json!({"name":"InsertData","fields":data(v,registry)}),
        Op::DeleteData(v) => json!({"name":"DeleteData","fields":data(v,registry)}),
        Op::DeleteWhere(v) => json!({"name":"DeleteWhere","fields":data(v,registry)}),
        Op::Clear { silent: s, graph } => silent("Clear", *s, json!({"graphiri":target(graph)})),
        Op::Drop { silent: s, graph } => silent("Drop", *s, json!({"graphiri":target(graph)})),
        Op::Create { silent: s, graph } => silent(
            "Create",
            *s,
            json!({"graphiri":atom("URIRef",graph.as_str())}),
        ),
        Op::Add {
            silent: s,
            from,
            to,
        } => silent(
            "Add",
            *s,
            json!({"graph":[graph_or_default(from),graph_or_default(to)]}),
        ),
        Op::Move {
            silent: s,
            from,
            to,
        } => silent(
            "Move",
            *s,
            json!({"graph":[graph_or_default(from),graph_or_default(to)]}),
        ),
        Op::Copy {
            silent: s,
            from,
            to,
        } => silent(
            "Copy",
            *s,
            json!({"graph":[graph_or_default(from),graph_or_default(to)]}),
        ),
        Op::Load {
            silent: s,
            source,
            destination,
        } => {
            let mut fields = json!({"iri":atom("URIRef",source.as_str())});
            if let Some(g) = destination {
                fields["graphiri"] = atom("URIRef", g.as_str());
            }
            silent("Load", *s, fields)
        }
        Op::Modify {
            delete,
            insert,
            with,
            using,
            ..
        } => {
            let mut fields = json!({});
            if let Some(v) = delete {
                fields["delete"] = json!({"name":"DeleteClause","fields":data(v,registry)});
            }
            if let Some(v) = insert {
                fields["insert"] = json!({"name":"InsertClause","fields":data(v,registry)});
            }
            if let Some(v) = with {
                fields["withClause"] = atom("URIRef", v.as_str());
            }
            if !using.is_empty() {
                fields["using"] = json!(
                    using
                        .iter()
                        .map(|(d, n)| {
                            let f = if let Some(d) = d {
                                json!({"default":atom("URIRef",d.as_str())})
                            } else {
                                json!({"named":atom("URIRef",n.as_ref().unwrap().as_str())})
                            };
                            json!({"name":"UsingClause","fields":f})
                        })
                        .collect::<Vec<_>>()
                );
            }
            json!({"name":"Modify","fields":fields})
        }
    }
}

fn inputs(f: &Value) -> BTreeMap<&str, &Value> {
    f["source_sets"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["input"].as_array().unwrap())
        .map(|a| (a["id"].as_str().unwrap(), a))
        .collect()
}

#[test]
fn source_update_original_operation_and_template_algebra() {
    let f = fixture();
    let inputs = inputs(&f);
    let mut failures = Vec::new();
    let mut count = 0;
    for row in f["algebra_with_constructor_observations"]["actions"]
        .as_array()
        .unwrap()
    {
        if row["status"] != "completed" {
            continue;
        }
        let id = row["id"].as_str().unwrap();
        let input = inputs[id];
        count += 1;
        let ns = serde_json::from_value(input["namespaces"].clone()).unwrap();
        let construction = std::rc::Rc::new(std::cell::RefCell::new(
            row["blank_constructions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["phase"] == "parse_update")
                .map(|r| r["value"].as_str().unwrap().to_owned())
                .collect::<std::collections::VecDeque<_>>(),
        ));
        let calls = construction.clone();
        let constructor = std::rc::Rc::new(move || {
            Ok(oxrdf::BlankNode::new_unchecked(
                calls
                    .borrow_mut()
                    .pop_front()
                    .expect("source parse requested an unobserved random constructor"),
            ))
        });
        let prepared = match source_update::prepare_with_constructor(
            input["query"].as_str().unwrap(),
            &ns,
            source_bridge::source_session().unwrap(),
            Some(constructor),
        ) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{id}: prepare: {e}"));
                continue;
            }
        };
        assert!(
            construction.borrow().is_empty(),
            "{id}: source constructor events unconsumed"
        );
        let actual = json!(
            prepared
                .request
                .operations
                .iter()
                .map(|op| operation(&op.kind, &prepared.registry))
                .collect::<Vec<_>>()
        );
        let mut expected = row["operations"].clone();
        for op in expected.as_array_mut().unwrap() {
            op["fields"].as_object_mut().unwrap().remove("where");
        }
        // Constructor outputs are observed random inputs, never answers.
        // Comparing their exact names also checks list topology and ordering.
        if actual != expected {
            failures.push(format!("{id}: actual {actual} expected {expected}"));
        }
    }
    assert_eq!(count, 95);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn original_shape(v: &Value) -> Value {
    let mut f = serde_json::Map::new();
    for key in ["p", "p1", "p2"] {
        if v["fields"].get(key).is_some() {
            f.insert(key.into(), original_shape(&v["fields"][key]));
        }
    }
    json!({"name":v["name"],"children":f})
}
fn source_shape(v: &GraphPattern) -> Value {
    match v {
        GraphPattern::SourceContext { inner, annotation } => {
            assert!(
                !matches!(
                    annotation,
                    SourceContextAnnotation::Join { .. }
                        | SourceContextAnnotation::Filter { .. }
                        | SourceContextAnnotation::Extend { .. }
                        | SourceContextAnnotation::LeftJoin { .. }
                ),
                "update received query-only analysis"
            );
            source_shape(inner)
        }
        GraphPattern::Bgp { .. } => json!({"name":"BGP","children":{}}),
        GraphPattern::Join { left, right } => {
            json!({"name":"Join","children":{"p1":source_shape(left),"p2":source_shape(right)}})
        }
        GraphPattern::LeftJoin { left, right, .. } => {
            json!({"name":"LeftJoin","children":{"p1":source_shape(left),"p2":source_shape(right)}})
        }
        GraphPattern::Union { left, right } => {
            json!({"name":"Union","children":{"p1":source_shape(left),"p2":source_shape(right)}})
        }
        GraphPattern::Graph { inner, .. } => {
            json!({"name":"Graph","children":{"p":source_shape(inner)}})
        }
        _ => panic!("unexpected plan in frozen structural check: {v:?}"),
    }
}
#[test]
fn source_update_original_where_translation_retains_unanalyzed_groups() {
    let f = fixture();
    let inputs = inputs(&f);
    for id in ["UP0608", "UP0617", "UP0626", "UP0698", "UP0707"] {
        let row = f["algebra_with_constructor_observations"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap();
        let p = source_update::prepare(
            inputs[id]["query"].as_str().unwrap(),
            &BTreeMap::new(),
            source_bridge::source_session().unwrap(),
        )
        .unwrap();
        let Op::Modify { pattern, .. } = &p.request.operations[0].kind else {
            panic!("not Modify")
        };
        assert_eq!(
            source_shape(pattern),
            original_shape(&row["operations"][0]["fields"]["where"]),
            "{id}"
        );
    }
}

#[test]
fn source_update_preparation_keeps_original_errors_and_occurrence_provenance() {
    let f = fixture();
    let inputs = inputs(&f);
    for id in ["UP0005", "UP0122", "UP0131", "UP0221", "UX0018"] {
        let row = f["algebra_with_constructor_observations"]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap();
        let error = source_update::prepare(
            inputs[id]["query"].as_str().unwrap(),
            &BTreeMap::new(),
            source_bridge::source_session().unwrap(),
        )
        .err()
        .expect(id);
        let actual = match error {
            super::numeric_query::NumericPrepareError::SourceSyntax(
                spargebra::SourceParserError::ParseException {
                    input,
                    location,
                    message,
                },
            ) => {
                json!({"module":"pyparsing.exceptions","type":"ParseException","args":[input,location,message]})
            }
            super::numeric_query::NumericPrepareError::LiteralConstruction(
                crate::numeric::RawNumericFault::SourceParserLiteralType { argument },
            ) => json!({"module":"builtins","type":"TypeError","args":[argument]}),
            super::numeric_query::NumericPrepareError::Query(e) => {
                json!({"module":"builtins","type":"Exception","args":[e.to_string()]})
            }
            e => panic!("{id}: unreceived preparation error {e:?}"),
        };
        let mut expected = row["error"].clone();
        expected.as_object_mut().unwrap().remove("message");
        assert_eq!(
            actual, expected,
            "{id}: original structured exception constructor"
        );
    }
    for id in ["UP0248", "UP0257"] {
        // Original translateUpdate returns a list when no request exists;
        // evalUpdate's later .algebra access is a backend execution failure.
        let p = source_update::prepare(
            inputs[id]["query"].as_str().unwrap(),
            &BTreeMap::new(),
            source_bridge::source_session().unwrap(),
        )
        .unwrap();
        assert!(p.request.operations.is_empty());
    }
    let q = "INSERT DATA { <urn:s> <urn:p> \"same\", \"same\"^^<http://www.w3.org/2001/XMLSchema#string> }";
    let a = source_update::prepare(
        q,
        &BTreeMap::new(),
        source_bridge::source_session().unwrap(),
    )
    .unwrap();
    let b = source_update::prepare(
        q,
        &BTreeMap::new(),
        source_bridge::source_session().unwrap(),
    )
    .unwrap();
    let Op::InsertData(data) = &a.request.operations[0].kind else {
        panic!("not data")
    };
    for t in &data.triples {
        let TermPattern::SourceLiteral(v) = &t.object else {
            panic!("literal lost origin")
        };
        assert!(a.registry.resolve(v).is_ok());
        assert!(b.registry.resolve(v).is_err());
    }
}


// Exact typed fault companion to the runtime partial-update control. This is
// an actual parser/compiler execution, never an action-ID error injection.
#[test]
fn source_update_execution_unanalyzed_empty_values_actual_fault() {
    use super::source_persistent::{SourceDatasetState, SourceUpdateLoader};
    use super::source_bridge::SourceTermInput;
    use crate::{QueryEvaluationError, numeric::{RawNumericFault, SourceContextFault}};
    struct NoResources;
    impl SourceUpdateLoader for NoResources {
        fn load(&mut self, _: &str,
            _: &mut dyn FnMut([SourceTermInput<'_>; 3]) -> Result<(), QueryEvaluationError>
        ) -> Result<bool, QueryEvaluationError> {
            Err(QueryEvaluationError::Unexpected("independent control grants no resource authority".into()))
        }
    }
    let mut state=SourceDatasetState::default();
    state.update_data("INSERT DATA {<urn:s> <urn:p> <urn:initial>}",&BTreeMap::new(),100,4096).unwrap();
    let error=state.update_full(
        "INSERT DATA {<urn:prefix> <urn:p> <urn:kept>}; INSERT {?s <urn:seen> ?o} WHERE {{?s <urn:p> ?o} UNION {VALUES () {}}}",
        &BTreeMap::new(),&BTreeMap::new(),&mut NoResources,100,4096,
    ).unwrap_err();
    assert!(!error.before_backend);
    assert!(matches!(crate::numeric::raw_numeric_fault(&error.error),
        Some(RawNumericFault::SourceContext(SourceContextFault::EmptyValuesAttribute))));
    assert_eq!(error.error.to_string(),"AttributeError: 'list' object has no attribute 'name'");
    assert!(state.matches_projection([
        ([SourceTermInput::Iri("urn:s"),SourceTermInput::Iri("urn:p"),SourceTermInput::Iri("urn:initial")],None),
        ([SourceTermInput::Iri("urn:prefix"),SourceTermInput::Iri("urn:p"),SourceTermInput::Iri("urn:kept")],None),
    ]).unwrap());
}

// Source-derived structural control, separate from original-query execution.
#[test]
fn source_update_optional_nested_preparation_retains_two_unanalyzed_scopes() {
    fn walk(value: &GraphPattern, unanalyzed_parent: bool, found: &mut (usize, usize, usize)) {
        match value {
            GraphPattern::SourceContext { inner, annotation } => {
                assert!(
                    !matches!(
                        annotation,
                        SourceContextAnnotation::Join { .. }
                            | SourceContextAnnotation::Filter { .. }
                            | SourceContextAnnotation::Extend { .. }
                            | SourceContextAnnotation::LeftJoin { .. }
                    ),
                    "UPDATE acquired QUERY analysis"
                );
                walk(
                    inner,
                    matches!(annotation, SourceContextAnnotation::Unanalyzed { .. }),
                    found,
                );
            }
            GraphPattern::LeftJoin {
                left,
                right,
                expression,
            } => {
                assert!(
                    unanalyzed_parent,
                    "OPTIONAL must retain actual unanalyzed owner"
                );
                assert!(
                    expression.is_none(),
                    "original authored query has no FILTER"
                );
                found.0 += 1;
                walk(left, false, found);
                walk(right, false, found);
            }
            GraphPattern::Join { left, right } => {
                walk(left, false, found);
                walk(right, false, found);
            }
            GraphPattern::Extend {
                inner,
                variable,
                expression,
            } => {
                assert_eq!(variable.as_str(), "marker");
                assert!(
                    matches!(expression, spargebra::algebra::Expression::NamedNode(node)
                    if node.as_str() == "urn:kept")
                );
                found.1 += 1;
                walk(inner, false, found);
            }
            GraphPattern::SourceValues {
                variables,
                bindings,
            } => {
                assert_eq!(
                    variables.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
                    vec!["x"]
                );
                assert_eq!(bindings.len(), 1);
                assert_eq!(bindings[0].len(), 1);
                let expected = if found.2 == 0 {
                    "urn:outside"
                } else {
                    "urn:other"
                };
                assert!(
                    matches!(&bindings[0][0], Some(spargebra::algebra::SourceValueTerm::NamedNode(node))
                    if node.as_str() == expected)
                );
                found.2 += 1;
            }
            GraphPattern::Bgp { patterns } => assert!(patterns.is_empty()),
            other => panic!("unexpected prepared node in fixed OPTIONAL control: {other:?}"),
        }
    }
    let prepared = source_update::prepare(
        "INSERT {<urn:s> <urn:seen> ?marker} WHERE { VALUES ?x {<urn:outside>} OPTIONAL { BIND(<urn:kept> AS ?marker) OPTIONAL { VALUES ?x {<urn:other>} } } }",
        &BTreeMap::new(), source_bridge::source_session().unwrap(),
    ).unwrap();
    assert_eq!(prepared.request.operations.len(), 1);
    let Op::Modify { pattern, .. } = &prepared.request.operations[0].kind else {
        panic!("fixed request must prepare one Modify operation")
    };
    let mut found = (0, 0, 0);
    walk(pattern, false, &mut found);
    assert_eq!(found, (2, 1, 2));
}

#[test]
fn source_update_constructor_legacy_parser_only_does_not_install_execution_hook() {
    use std::{cell::Cell,rc::Rc};
    let calls=Rc::new(Cell::new(0usize));let seen=calls.clone();
    let constructor:spargebra::SourceBlankNodeConstructor=Rc::new(move || {
        seen.set(seen.get()+1);Ok(oxrdf::BlankNode::new("legacy_parser_input")?)
    });
    let prepared=source_update::prepare_with_constructor(
        "INSERT DATA {<urn:s> <urn:p> [<urn:q> <urn:o>]}",
        &BTreeMap::new(),source_bridge::source_session().unwrap(),Some(constructor)
    ).unwrap();
    assert_eq!(calls.get(),1);
    assert!(prepared.update_blank_constructor.is_none(),"legacy parse input must not become execution input");
    let operation=&prepared.request.operations[0];
    let Op::InsertData(data)=&operation.kind else {panic!("source operation changed")};
    assert_eq!(data.triples.len(),2);
    let rows=data.triples.iter().flat_map(|t|[&t.subject,&t.object]).filter_map(|t|{
        if let TermPattern::BlankNode(b)=t {Some(b.as_str())}else{None}
    }).collect::<Vec<_>>();
    assert_eq!(rows,vec!["legacy_parser_input","legacy_parser_input"]);
}
