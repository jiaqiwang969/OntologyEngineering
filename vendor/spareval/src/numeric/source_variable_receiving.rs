//! Original source values are received independently of the Rust output.
//! N3 for the existing RDF controls is only an observation adapter. Variable
//! notation uses the actual typed value; full runtime UPDATE remains separate.
use super::*;
use crate::dataset::{
    QueryableDataset,
    nullable_numeric::NumericDataset,
    source_memory::{SourceGraphTerm, SourceMemoryGraph},
};
use crate::numeric::source_function::SourceNodeUnary;
use oxrdf::{BlankNode, Dataset};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_original.json"
    )
}
fn private_plan() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_plan.json"
    )
}
fn original() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "04897e1b5d024b7ef47b07caebde42a9445bbca3b99120678ff2f8474cd324c1"
    );
    serde_json::from_str(private_fixture()).unwrap()
}
fn plan() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_plan().as_bytes())),
        "001e10de65e41964d10f9b88731a26dfe111bd83a66543f5c911a1fd8b4fb211"
    );
    serde_json::from_str(private_plan()).unwrap()
}
fn session() -> Rc<QueryNumericSession> {
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
            None,
            true,
            QueryNumericCapabilities::MixedBinary64V1,
            QueryGroupSemantics::SourceAggregateV1,
            QueryLiteralSemantics::SourceLiteralV1,
            QueryContextSemantics::SourceContextV1,
            Some(Rc::new(
                SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap(),
            )),
        )
        .unwrap(),
    )
}
fn terms(s: &QueryNumericSession) -> BTreeMap<String, RawExpressionValue> {
    plan()["terms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|spec| {
            let text = spec["text"].as_str().unwrap();
            let raw = match spec["kind"].as_str().unwrap() {
                "Variable" => s.construct_source_variable(text).unwrap(),
                "URIRef" => RawExpressionValue::from_import(NamedNode::new_unchecked(text).into())
                    .with_new_source_object(),
                "BNode" => RawExpressionValue::from_import(BlankNode::new_unchecked(text).into())
                    .with_new_source_object(),
                "Literal" => s
                    .construct_source_direct_string(
                        text,
                        spec["datatype"].as_str().map(NamedNode::new_unchecked),
                        None,
                        false,
                    )
                    .unwrap(),
                _ => panic!("unknown frozen input"),
            };
            (spec["id"].as_str().unwrap().to_owned(), raw)
        })
        .collect()
}
fn encode(s: &QueryNumericSession, raw: &RawExpressionValue) -> Value {
    let value = SolutionValue::Rdf(raw.clone());
    let repr = s.source_repr(&value).unwrap();
    let hash = s.source_hash().unwrap().term_hash(raw).unwrap();
    if let Some(v) = raw.source_variable() {
        return json!({"kind":"Variable","text":v.as_str(),"n3":v.to_string(),"hash":hash,"repr":repr,"truth":!v.as_str().is_empty(),"python":v.to_string()});
    }
    let (kind, text, n3, truth) = match raw.rdf().unwrap() {
        Term::NamedNode(v) => (
            "URIRef",
            v.as_str(),
            format!("<{}>", v.as_str()),
            !v.as_str().is_empty(),
        ),
        Term::BlankNode(v) => (
            "BNode",
            v.as_str(),
            format!("_:{}", v.as_str()),
            !v.as_str().is_empty(),
        ),
        Term::Literal(v) => {
            let LiteralSourceState::Literal(m) = raw.literal_source() else {
                panic!()
            };
            // Only the independently frozen simple controls use this adapter.
            let mut n3 = serde_json::to_string(v.value()).unwrap();
            if let SourceDatatype::Specified(dt) = &m.datatype {
                n3.push_str(&format!("^^<{}>", dt.as_str()));
            }
            if let SourceFact::Known(Some(lang)) = &m.language_original {
                n3.push('@');
                n3.push_str(lang);
            }
            (
                "Literal",
                v.value(),
                n3,
                s.source_literal_python_truth(raw).unwrap(),
            )
        }
    };
    let mut out = json!({"kind":kind,"text":text,"n3":n3,"hash":hash,"repr":repr,"truth":truth});
    if let LiteralSourceState::Literal(m) = raw.literal_source() {
        out["datatype"] = match &m.datatype {
            SourceDatatype::Absent => Value::Null,
            SourceDatatype::Specified(v) => json!(v.as_str()),
            SourceDatatype::Unknown => panic!(),
        };
        let SourceFact::Known(lang) = &m.language_original else {
            panic!()
        };
        out["language"] = json!(lang);
    }
    out
}
fn error(e: RawNumericFault) -> Value {
    let src = e
        .source_language_exception()
        .expect("actual source-language constructor");
    let [source_exception::LanguageExceptionArgument::Text(message)] = src.arguments.as_slice()
    else {
        panic!()
    };
    json!({"status":"original_error","error":{"module":src.kind.module(),"type":src.kind.class_name(),"message":message}})
}
fn outcome(
    s: &QueryNumericSession,
    result: Result<RawExpressionValue, NumericEvaluationError>,
) -> Value {
    match result {
        Ok(raw) => json!({"status":"completed","value":encode(s,&raw)}),
        Err(e) => match e.into_error_value() {
            Err(fault) => error(fault),
            Ok(error) => {
                let source = error.source_exception().expect("actual original error");
                let SourceExceptionArgument::Text(message) = &source.argument else {
                    panic!()
                };
                let name = match source.class {
                    SourceExceptionClass::Sparql => "SPARQLError",
                    SourceExceptionClass::SparqlType => "SPARQLTypeError",
                    _ => panic!(),
                };
                json!({"status":"original_error","error":{"module":"rdflib.plugins.sparql.sparql","type":name,"message":message}})
            }
        },
    }
}
#[test]
fn source_variable_original_constructors() {
    let s = session();
    let root = original();
    assert_eq!(root["constructors"].as_array().unwrap().len(), 10);
    for row in root["constructors"].as_array().unwrap() {
        let input = row["input"].as_str().unwrap();
        let actual = match s.construct_source_variable(input) {
            Ok(v) => json!({"status":"completed","value":encode(&s,&v)}),
            Err(e) => error(e),
        };
        assert_eq!(actual, row["result"], "{input:?}");
    }
    for (name, v) in terms(&s) {
        assert_eq!(encode(&s, &v), root["terms"][&name], "{name}");
    }
}
#[test]
fn source_variable_original_relations() {
    let s = session();
    let values = terms(&s);
    let root = original();
    assert_eq!(root["comparisons"].as_array().unwrap().len(), 64);
    for row in root["comparisons"].as_array().unwrap() {
        let a = &values[row["left"].as_str().unwrap()];
        let b = &values[row["right"].as_str().unwrap()];
        let eq = source_context_equal(a, b).unwrap();
        let lt = s
            .source_graph_node_less(
                &SourceOrderKey::Rdf(a.clone()),
                &SourceOrderKey::Rdf(b.clone()),
            )
            .unwrap();
        let gt = s
            .source_graph_node_less(
                &SourceOrderKey::Rdf(b.clone()),
                &SourceOrderKey::Rdf(a.clone()),
            )
            .unwrap();
        let mut actual = serde_json::Map::new();
        for (op, value) in [
            ("eq", eq),
            ("ne", !eq),
            ("lt", lt),
            ("le", lt || eq),
            ("gt", gt),
            ("ge", gt || eq),
        ] {
            actual.insert(op.into(), json!({"status":"completed","value":value}));
        }
        assert_eq!(
            Value::Object(actual),
            row["results"],
            "{} / {}",
            row["left"],
            row["right"]
        );
        assert_eq!(
            a.same_source_object(b),
            row["same_object"].as_bool().unwrap()
        );
        assert_eq!(
            s.source_hash().unwrap().term_hash(a).unwrap()
                == s.source_hash().unwrap().term_hash(b).unwrap(),
            row["same_hash"].as_bool().unwrap()
        );
    }
}
#[test]
fn source_variable_original_direct_operators() {
    let s = session();
    let values = terms(&s);
    let root = original();
    assert_eq!(root["operators"].as_array().unwrap().len(), 80);
    for row in root["operators"].as_array().unwrap() {
        let value = SolutionValue::Rdf(values[row["term"].as_str().unwrap()].clone());
        use SourceNodeUnary::*;
        let result = match row["operator"].as_str().unwrap() {
            "Builtin_STR" => s
                .source_text(&value)
                .and_then(|v| s.construct_source_plain_result(v.into_owned()))
                .map_err(Into::into),
            "Builtin_DATATYPE" => s.source_datatype(&value),
            name => s.source_node_unary(
                match name {
                    "Builtin_isIRI" => IsIri,
                    "Builtin_isBLANK" => IsBlank,
                    "Builtin_isLITERAL" => IsLiteral,
                    "Builtin_isNUMERIC" => IsNumeric,
                    "Builtin_LANG" => Lang,
                    "Builtin_STRLEN" => StrLen,
                    "Builtin_UCASE" => Upper,
                    "Builtin_LCASE" => Lower,
                    _ => panic!(),
                },
                &value,
            ),
        };
        assert_eq!(
            outcome(&s, result),
            row["result"],
            "{} / {}",
            row["term"],
            row["operator"]
        );
    }
}
#[test]
fn source_variable_original_memory_actions() {
    let s = session();
    let inputs = terms(&s);
    let root = original();
    let mut graph = SourceMemoryGraph::new_dataset(
        s.clone(),
        None,
        NamedNode::new_unchecked("urn:default").into(),
    )
    .unwrap();
    let names: Vec<_> = plan()["terms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect();
    let terms: BTreeMap<_, SourceGraphTerm> = inputs
        .iter()
        .map(|(k, v)| (k.clone(), graph.admit_source(v.clone()).unwrap()))
        .collect();
    assert_eq!(root["memory"].as_array().unwrap().len(), 7);
    for row in root["memory"].as_array().unwrap() {
        let triple: [SourceGraphTerm; 3] =
            std::array::from_fn(|i| terms[row["input"][i].as_str().unwrap()].clone());
        match row["op"].as_str().unwrap() {
            "add" => graph.insert(triple.clone()).unwrap(),
            "remove" => graph
                .remove_context(None, triple.clone().map(Some))
                .unwrap(),
            _ => panic!(),
        };
        assert_eq!(graph.len(), row["length"].as_u64().unwrap() as usize);
        for scan in row["scans"].as_array().unwrap() {
            let mask = scan["mask"].as_u64().unwrap();
            let pattern =
                std::array::from_fn(|i| (mask & (1 << i) != 0).then(|| triple[i].clone()));
            let actual: Vec<_> = graph
                .scan_context(None, pattern)
                .unwrap()
                .iter()
                .map(|triple| {
                    triple
                        .iter()
                        .map(|term| {
                            let raw = graph.externalize_source(term.clone()).unwrap();
                            let aliases: Vec<_> = names
                                .iter()
                                .filter(|name| raw.same_source_object(&inputs[*name]))
                                .collect();
                            json!({"term":encode(&s,&raw),"aliases":aliases})
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            assert_eq!(json!(actual), scan["rows"], "{} mask{mask}", row["id"]);
        }
    }
    // Declared Rust control after the complete original sequence: transfer
    // actual source slots/objects, never reconstruct from projected RDF.
    let state = graph.into_state();
    let state = std::thread::spawn(move || state).join().unwrap();
    let graph = SourceMemoryGraph::from_state(state, s, None).unwrap();
    assert_eq!(
        graph.len(),
        root["memory"].as_array().unwrap().last().unwrap()["length"]
            .as_u64()
            .unwrap() as usize
    );
}
#[test]
fn source_variable_transport_and_conversion_boundaries() {
    let s = session();
    let raw = s.construct_source_variable("??x").unwrap();
    assert_eq!(raw.source_variable().unwrap().as_str(), "?x");
    assert!(matches!(
        raw.rdf(),
        Err(RawNumericFault::SourceTermNotRdf("Variable"))
    ));
    assert!(raw.clone().into_parts().is_err());
    assert!(raw.clone().into_source_parts().is_err());
    let graph =
        SourceMemoryGraph::new_dataset(s.clone(), None, NamedNode::new_unchecked("urn:d").into())
            .unwrap();
    let term = graph.admit_source(raw.clone()).unwrap();
    assert!(graph.externalize_term(term.clone()).is_err());
    let foreign =
        SourceMemoryGraph::new_dataset(s.clone(), None, NamedNode::new_unchecked("urn:d").into())
            .unwrap();
    assert!(foreign.externalize_source(term).is_err());
    let dataset = NumericDataset::new_with_source_dataset(graph, s.clone()).unwrap();
    let entered = dataset.internalize_raw(raw.clone()).unwrap();
    let read = dataset.externalize_raw(entered).unwrap();
    assert!(read.same_source_object(&raw));
    assert_eq!(read, raw);
    let rdf = Dataset::new();
    assert!(
        NumericDataset::new(&rdf)
            .internalize_raw(raw.clone())
            .is_err()
    );
    let copied = s.construct_source_variable("??x").unwrap();
    assert!(!copied.same_source_object(&raw));
    assert_eq!(copied, raw);
    assert_ne!(
        source_identity_key(&raw).unwrap(),
        source_identity_key(&RawExpressionValue::from_import(
            NamedNode::new_unchecked("?x").into()
        ))
        .unwrap()
    );
    // Pre-existing RDF-only graph contract still refuses non-RDF roles.
    let mut old = SourceMemoryGraph::new(s, None).unwrap();
    let v = old.admit_source(raw).unwrap();
    let p = old
        .internalize_term(NamedNode::new_unchecked("urn:p").into())
        .unwrap();
    assert!(old.insert([v.clone(), p.clone(), v.clone()]).is_err());
    assert!(old.insert([p.clone(), v.clone(), p.clone()]).is_err());
    assert!(old.insert([p.clone(), p, v]).is_err());
}
