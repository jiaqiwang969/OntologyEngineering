//! Actual parser/registry/compiler literal bridge receiving. The oracle and
//! adapter share an author; source expectations were independently reviewed.
//! These no-EXISTS queries do not receive the whole source EXISTS frontend.
use super::numeric_query::{NumericPrepareError, prepare_source_frontend_numeric_query};
use super::*;
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    LiteralSourceState, NumericCacheState, PreparedSourceLiteralRegistry, QueryContextSemantics,
    QueryGroupSemantics, QueryLiteralSemantics, QueryNumericCapabilities, SourceFact,
    SourceHashPolicy, SourceHashRuntime, resolve_source_literal,
};
use semantica_numeric::{
    DecimalContext, DecimalSignals, DecimalValue, NumericLimits, NumericValue,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spargebra::algebra::SourceValueTerm;
use spargebra::{
    ParsedSourceLiteral, SourceDatatype, SourceLiteralHooks, SourceLiteralOrigin, SparqlParser,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;

const FIXTURE: &str = include_str!("fixtures/source_literal_parser_bridge_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "80a8addd53317774c504291a7115e4775c1727f7de86e12247ec847acf0b7a07"
    );
    let f: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        f["source_record_sha256"],
        "04a072ca2b02cbe8f24c48ae96f24b3649f9e67318c3f7fd5f5f92025a23145e"
    );
    f
}
fn case(id: &str) -> Value {
    let f = fixture();
    ["source_query_cases", "bridge_controls"]
        .into_iter()
        .flat_map(|k| f["payload"][k].as_array().unwrap())
        .find(|c| c["id"] == id)
        .unwrap()
        .clone()
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn session(untrapped: bool) -> Rc<QueryNumericSession> {
    let decimal = if untrapped {
        DecimalContext {
            traps: DecimalSignals::EMPTY,
            ..DecimalContext::default()
        }
    } else {
        DecimalContext::default()
    };
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            decimal,
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
fn flags(s: &QueryNumericSession) -> Value {
    Value::Array(
        s.flags()
            .iter()
            .map(|signal| json!(format!("{signal:?}")))
            .collect(),
    )
}
#[derive(Clone, Default)]
struct Backend {
    calls: Rc<Cell<usize>>,
}
impl<'a> QueryableDataset<'a> for Backend {
    type InternalTerm = Term;
    type Error = Infallible;
    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.calls.set(self.calls.get() + 1);
        empty()
    }
    fn internalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(value)
    }
    fn externalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        self.calls.set(self.calls.get() + 1);
        Ok(value)
    }
}
fn raw<'a>(row: &'a SourcePresenceSolution, name: &str) -> &'a RawExpressionValue {
    let slot = row
        .variables
        .iter()
        .position(|v| v.as_str() == name)
        .unwrap();
    let Some(SolutionValue::Rdf(value)) = &row.values[slot] else {
        panic!("missing actual RDF output")
    };
    value
}
fn text(value: &RawExpressionValue) -> &str {
    let Term::Literal(value) = value.rdf().unwrap() else {
        panic!("expected actual literal")
    };
    value.value()
}
fn rows_json(rows: &[SourcePresenceSolution]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| {
                Value::Object(
                    row.variables
                        .iter()
                        .zip(&row.values)
                        .filter_map(|(v, cell)| match cell {
                            Some(SolutionValue::Rdf(raw)) => {
                                Some((v.as_str().to_owned(), json!(text(raw))))
                            }
                            None => None,
                            _ => panic!("unexpected cell"),
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}
fn collect_literals(plan: &GraphPattern, out: &mut Vec<ParsedSourceLiteral>) {
    match plan {
        GraphPattern::Project { inner, .. } | GraphPattern::SourceContext { inner, .. } => {
            collect_literals(inner, out)
        }
        GraphPattern::Extend {
            inner, expression, ..
        } => {
            collect_literals(inner, out);
            match expression {
                Expression::SourceLiteral(literal) => out.push(literal.clone()),
                Expression::Variable(_) => (),
                Expression::UnaryMinus(operand) => match operand.as_ref() {
                    Expression::SourceLiteral(literal) => out.push(literal.clone()),
                    other => panic!("unfrozen unary bridge operand {other:?}"),
                },
                other => panic!("unfrozen bridge expression {other:?}"),
            }
        }
        GraphPattern::SourceValues { bindings, .. } => {
            for row in bindings {
                for value in row.iter().flatten() {
                    match value {
                        SourceValueTerm::Literal(literal) => out.push(literal.clone()),
                        other => panic!("unfrozen source table cell {other:?}"),
                    }
                }
            }
        }
        GraphPattern::Join { left, right, .. } => {
            collect_literals(left, out);
            collect_literals(right, out)
        }
        GraphPattern::Values {
            variables,
            bindings,
        } => assert!(variables.is_empty() && bindings.iter().all(Vec::is_empty)),
        other => panic!("literal-only bridge test reached unreceived plan {other:?}"),
    }
}
fn parsed_plan(query: &str, hooks: SourceLiteralHooks) -> GraphPattern {
    let (query, _) = SparqlParser::new()
        .parse_query_rdflib760_source_exists_frontend(query, hooks, true)
        .unwrap();
    let spargebra::Query::Select { pattern, .. } = query else {
        panic!("SELECT required")
    };
    GraphPattern::from(&pattern)
}
fn only_leaf(query: &str, registry: &Rc<PreparedSourceLiteralRegistry>) -> ParsedSourceLiteral {
    let plan = parsed_plan(query, registry.hooks());
    let mut leaves = Vec::new();
    collect_literals(&plan, &mut leaves);
    assert_eq!(leaves.len(), 1);
    leaves.pop().unwrap()
}
fn metadata(raw: &RawExpressionValue) -> &crate::numeric::SourceLiteralMetadata {
    let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
        panic!("missing source facts")
    };
    metadata
}

#[test]
fn source_literal_bridge_fixture_identity() {
    let f = fixture();
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical(&f["payload"])).unwrap())
        ),
        f["payload_sha256"]
    );
    assert_eq!(
        f["payload"]["source_query_cases"].as_array().unwrap().len(),
        1
    );
    assert_eq!(f["payload"]["bridge_controls"].as_array().unwrap().len(), 6);
}
#[test]
fn source_literal_bridge_bq01() {
    let c = case("BQ01");
    let s = session(false);
    let backend = Backend::default();
    let prepared = prepare_source_frontend_numeric_query(
        c["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&s),
        vec![],
    )
    .unwrap();
    let registry = Rc::clone(
        prepared
            .evaluator()
            .dataset
            .numeric
            .as_ref()
            .unwrap()
            .prepared_literals
            .as_ref()
            .unwrap(),
    );
    let mut leaves = Vec::new();
    collect_literals(prepared.plan(), &mut leaves);
    let factory: Vec<_> = leaves
        .iter()
        .map(|leaf| registry.resolve(leaf).unwrap())
        .collect();
    let mut iter = prepared.execute_source_presence().unwrap();
    let mut rows = Vec::new();
    while let Some(row) = iter.next() {
        rows.push(row.unwrap());
    }
    let calls = backend.calls.get();
    assert!(iter.next().is_none());
    assert_eq!(backend.calls.get(), calls);
    assert_eq!(rows_json(&rows), c["expected"]["rows"]);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        raw(&rows[0], "x").same_source_object(raw(&rows[1], "x")),
        c["expected"]["x_source_objects_same_between_rows"]
    );
    assert_eq!(
        raw(&rows[0], "y").same_source_object(raw(&rows[1], "y")),
        c["expected"]["y_source_objects_same_between_rows"]
    );
    for row in &rows {
        for name in ["x", "y"] {
            let value = raw(row, name);
            let Term::Literal(rdf) = value.rdf().unwrap() else {
                unreachable!()
            };
            assert_eq!(rdf.datatype().as_str(), c["expected"]["datatype"]);
            assert_eq!(
                metadata(value).ill_typed,
                SourceFact::Known(c["expected"]["ill_typed"].as_bool())
            );
            assert_eq!(
                factory
                    .iter()
                    .filter(|f| f.same_source_object(value))
                    .count(),
                1
            );
        }
    }
    assert_eq!(flags(&s), c["expected"]["source_flags"]);
}
#[test]
fn source_literal_bridge_bp01() {
    let c = case("BP01");
    let s = session(false);
    let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
    let mut hooks = registry.hooks();
    let real = Rc::clone(&hooks.construct);
    let capture: Rc<RefCell<Vec<(usize, RawExpressionValue)>>> = Rc::default();
    let sink = Rc::clone(&capture);
    let r = Rc::clone(&registry);
    hooks.construct = Rc::new(move |id, input| {
        let description = real(id.clone(), input)?;
        // The ID was allocated by the real parser. This test-only lookup
        // observes the registered object; the returned description stays
        // unstamped and no replacement AST is supplied to an evaluator.
        let mut lookup = description.clone();
        lookup.occurrence = Some(id.clone());
        let value = r
            .resolve(&lookup)
            .map_err(|e| -> spargebra::SourceLiteralBridgeError { Box::new(e) })?;
        sink.borrow_mut().push((id.index(), value));
        Ok(description)
    });
    let truth = Rc::clone(&hooks.python_truth);
    let plan = parsed_plan(c["input"]["query"].as_str().unwrap(), hooks);
    let mut leaves = Vec::new();
    collect_literals(&plan, &mut leaves);
    assert_eq!(
        capture.borrow().len() as u64,
        c["expected"]["constructor_callback_count"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(
        json!(capture.borrow().iter().map(|(i, _)| *i).collect::<Vec<_>>()),
        c["expected"]["occurrence_indices"]
    );
    assert_eq!(
        leaves.len() as u64,
        c["expected"]["unique_occurrence_count"].as_u64().unwrap()
    );
    let actual_ids: BTreeSet<_> = leaves
        .iter()
        .map(|leaf| leaf.occurrence.as_ref().unwrap().index())
        .collect();
    assert_eq!(
        json!(actual_ids.iter().copied().collect::<Vec<_>>()),
        c["expected"]["occurrence_indices"]
    );
    assert_eq!(actual_ids.len(), leaves.len());
    for leaf in &leaves {
        let id = leaf.occurrence.as_ref().unwrap();
        let raw = registry.resolve(leaf).unwrap();
        let capture = capture.borrow();
        let original = &capture.iter().find(|(i, _)| *i == id.index()).unwrap().1;
        assert!(raw.same_source_object(original));
        for _ in 0..2 {
            assert!(truth(id).unwrap());
        }
    }
    let equal: Vec<_> = capture
        .borrow()
        .iter()
        .filter(|(_, r)| text(r) == "1")
        .map(|(_, r)| r.clone())
        .collect();
    assert_eq!(equal.len(), 2);
    assert_eq!(equal[0].rdf().unwrap(), equal[1].rdf().unwrap());
    assert!(!equal[0].same_source_object(&equal[1]));
    let reverse: spargebra::algebra::GraphPattern = (&plan).into();
    let lowered = GraphPattern::from(&reverse);
    let mut again = Vec::new();
    collect_literals(&lowered, &mut again);
    assert_eq!(leaves, again);
    assert_eq!(capture.borrow().len(), 3);
    assert_eq!(
        flags(&s),
        c["expected"]["flags_after_truth_and_lower_reverse"]
    );
}
#[test]
fn source_literal_bridge_bp02() {
    let c = case("BP02");
    let s = session(false);
    let a = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
    let b = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
    let query = c["input"]["query"].as_str().unwrap();
    let la = only_leaf(query, &a);
    let lb = only_leaf(query, &b);
    let va = a.resolve(&la).unwrap();
    let vb = b.resolve(&lb).unwrap();
    assert_eq!(va.rdf().unwrap(), vb.rdf().unwrap());
    assert!(!va.same_source_object(&vb));
    assert_eq!(la.occurrence.as_ref().unwrap().index(), 0);
    assert_eq!(lb.occurrence.as_ref().unwrap().index(), 0);
    assert!(matches!(
        b.resolve(&la),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    assert!(matches!(
        a.resolve(&lb),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    let error = (b.hooks().python_truth)(la.occurrence.as_ref().unwrap()).unwrap_err();
    assert_eq!(
        error.downcast_ref::<RawNumericFault>(),
        Some(&RawNumericFault::InvalidSourceOrigin)
    );
    assert_eq!(flags(&s), c["expected"]["flags"]);
}
#[test]
fn source_literal_bridge_bp03() {
    let c = case("BP03");
    let s = session(false);
    let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&s));
    let leaf = only_leaf(c["input"]["query"].as_str().unwrap(), &registry);
    let original = registry.resolve(&leaf).unwrap();
    assert_eq!(c["input"]["tampering"].as_array().unwrap().len(), 5);
    let mut lexical = leaf.clone();
    lexical.rdf = Literal::new_typed_literal("2", leaf.rdf.datatype());
    let mut datatype = leaf.clone();
    datatype.datatype = SourceDatatype::Absent;
    let mut origin = leaf.clone();
    origin.origin = SourceLiteralOrigin::QueryPlain;
    let mut missing = leaf.clone();
    missing.occurrence = None;
    for edited in [lexical, datatype, origin, missing] {
        assert!(matches!(
            registry.resolve(&edited),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
    }
    assert!(matches!(
        resolve_source_literal(&s, None, &leaf),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    assert!(original.same_source_object(&registry.resolve(&leaf).unwrap()));
    assert_eq!(flags(&s), c["expected"]["flags"]);
}
#[test]
fn source_literal_bridge_bp04() {
    // Retained original SELECT, corrected from independently observed source
    // grammar after the first receiving failure. It is UnaryMinus over an
    // unsigned Decimal, unlike the new BP06 VALUES negative-token case.
    let c = case("BP04");
    let s = session(false);
    let backend = Backend::default();
    let prepared = prepare_source_frontend_numeric_query(
        c["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&s),
        vec![],
    )
    .unwrap();
    let registry = Rc::clone(
        prepared
            .evaluator()
            .dataset
            .numeric
            .as_ref()
            .unwrap()
            .prepared_literals
            .as_ref()
            .unwrap(),
    );
    let mut leaves = Vec::new();
    collect_literals(prepared.plan(), &mut leaves);
    assert_eq!(leaves.len(), 1);
    let operand = registry.resolve(&leaves[0]).unwrap();
    assert_eq!(text(&operand), c["expected"]["unsigned_operand"]["lexical"]);
    assert_eq!(
        metadata(&operand).ill_typed,
        SourceFact::Known(c["expected"]["unsigned_operand"]["ill_typed"].as_bool())
    );
    assert_eq!(flags(&s), c["expected"]["numeric_flags"]);
    let mut iter = prepared.execute_source_presence().unwrap();
    let rows = iter.by_ref().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(rows_json(&rows), c["expected"]["rows"]);
    assert_eq!(rows.len(), 1);
    let result = raw(&rows[0], "x");
    let Term::Literal(rdf) = result.rdf().unwrap() else {
        unreachable!()
    };
    assert_eq!(rdf.datatype().as_str(), c["expected"]["datatype"]);
    assert_eq!(metadata(result).ill_typed, SourceFact::Known(None));
    assert_eq!(
        result.same_source_object(&operand),
        c["expected"]["result_source_object_same_as_unsigned_operand"]
    );
    let NumericCacheState::Known(cache) = result.numeric() else {
        panic!("expected numeric cache")
    };
    let NumericValue::Decimal(DecimalValue::Finite {
        negative,
        coefficient,
        exponent,
    }) = cache.value()
    else {
        panic!("expected actual finite Decimal cache")
    };
    assert_eq!(*negative, c["expected"]["numeric_cache"]["negative"]);
    assert_eq!(
        coefficient.to_string(),
        c["expected"]["numeric_cache"]["coefficient"]
    );
    assert_eq!(
        exponent.to_string(),
        c["expected"]["numeric_cache"]["exponent"].to_string()
    );
    assert_eq!(flags(&s), c["expected"]["numeric_flags"]);
    let calls = backend.calls.get();
    assert!(iter.next().is_none());
    assert_eq!(backend.calls.get(), calls);
}
#[test]
fn source_literal_bridge_bp05() {
    let c = case("BP05");
    assert_eq!(
        c["input"]["environment_overrides"]["decimal_traps"],
        json!([])
    );
    let s = session(true);
    let backend = Backend::default();
    let prepared = prepare_source_frontend_numeric_query(
        c["input"]["query"].as_str().unwrap(),
        backend,
        Rc::clone(&s),
        vec![],
    )
    .unwrap();
    assert_eq!(flags(&s), c["expected"]["flags_after_prepare"]);
    let registry = Rc::clone(
        prepared
            .evaluator()
            .dataset
            .numeric
            .as_ref()
            .unwrap()
            .prepared_literals
            .as_ref()
            .unwrap(),
    );
    let mut leaves = Vec::new();
    collect_literals(prepared.plan(), &mut leaves);
    let original_bad: Vec<_> = leaves
        .iter()
        .map(|leaf| registry.resolve(leaf).unwrap())
        .filter(|value| text(value) == "bad")
        .collect();
    assert_eq!(original_bad.len(), 1);
    let original_bad = &original_bad[0];
    let rows = prepared
        .execute_source_presence()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows_json(&rows), c["expected"]["rows"]);
    assert_eq!(rows.len(), 2);
    let a = raw(&rows[0], "y");
    let b = raw(&rows[1], "y");
    assert!(a.same_source_object(b));
    assert!(a.same_source_object(original_bad));
    assert!(b.same_source_object(original_bad));
    for raw in [a, b] {
        let NumericCacheState::Known(cache) = raw.numeric() else {
            panic!("known cache")
        };
        assert!(matches!(
            cache.value(),
            NumericValue::Decimal(DecimalValue::NaN {
                signaling: false,
                ..
            })
        ));
    }
    assert_eq!(flags(&s), c["expected"]["flags_after_execution"]);
    assert_eq!(
        s.source_literal_python_truth(a).unwrap(),
        c["expected"]["truth_of_output_y"]
    );
    assert_eq!(flags(&s), c["expected"]["flags_after_truth"]);
}

#[test]
fn source_literal_bridge_bp06() {
    let c = case("BP06");
    let s = session(false);
    let backend = Backend::default();
    let result = prepare_source_frontend_numeric_query(
        c["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&s),
        vec![],
    );
    assert!(matches!(
        result,
        Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::ParserLiteralType
        ))
    ));
    assert_eq!(backend.calls.get(), 0);
    assert_eq!(flags(&s), c["expected"]["numeric_flags"]);
}
