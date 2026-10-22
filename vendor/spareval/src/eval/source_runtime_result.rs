//! Original SemanticRuntime query result projection over sealed source tuples.
//! Data/query literal provenance is retained; RDF1.1 terms alone cannot supply
//! the original plain-literal or language-case facts. Private feature only.
use super::{numeric_query::PreparedSourceRows, *};
use crate::numeric::{
    LiteralSourceState, PreparedSourceLiteralRegistry, RawExpressionValue, RawNumericFault,
    SourceDatatype, SourceFact, SourceTermKey, source_identity_key,
};
use oxrdf::{BlankNode, Term};
use serde_json::{Value, json};
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

pub(super) enum TemplateTerm {
    Constant(RawExpressionValue),
    Variable(Variable),
    Blank(BlankNode),
}
pub(super) enum ResultForm {
    Select,
    Ask,
    Construct(Vec<[TemplateTerm; 3]>),
}
pub(super) fn bind_template(
    template: Vec<TriplePattern>,
    registry: &PreparedSourceLiteralRegistry,
) -> Result<ResultForm, RawNumericFault> {
    let term = |value: TermPattern| -> Result<TemplateTerm, RawNumericFault> {
        Ok(match value {
            TermPattern::NamedNode(v) => TemplateTerm::Constant(
                RawExpressionValue::from_import(v.into()).ensure_source_object(),
            ),
            TermPattern::BlankNode(v) => TemplateTerm::Blank(v),
            TermPattern::Variable(v) => TemplateTerm::Variable(v),
            TermPattern::SourceLiteral(v) => TemplateTerm::Constant(registry.resolve(&v)?),
            // An unstamped RDF literal cannot acquire query-source provenance.
            _ => return Err(RawNumericFault::InvalidSourceOrigin),
        })
    };
    let mut bound = Vec::with_capacity(template.len());
    for triple in template {
        let predicate = match triple.predicate {
            NamedNodePattern::NamedNode(v) => TemplateTerm::Constant(
                RawExpressionValue::from_import(v.into()).ensure_source_object(),
            ),
            NamedNodePattern::Variable(v) => TemplateTerm::Variable(v),
        };
        bound.push([term(triple.subject)?, predicate, term(triple.object)?]);
    }
    Ok(ResultForm::Construct(bound))
}
fn error(message: &'static str) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(message.into())
}
fn fault(value: RawNumericFault) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(value))
}

pub(crate) fn dto(raw: &RawExpressionValue) -> Result<Value, QueryEvaluationError> {
    if raw.source_variable().is_some() {
        return Err(QueryEvaluationError::UnsupportedSourceRuntimeTerm(
            "Variable",
        ));
    }
    let (value, kind, datatype, language) = match raw.rdf().map_err(fault)? {
        Term::NamedNode(v) => (v.as_str(), "iri", None, None),
        Term::BlankNode(v) => (v.as_str(), "blank_node", None, None),
        Term::Literal(v) => {
            let LiteralSourceState::Literal(meta) = raw.literal_source() else {
                return Err(fault(RawNumericFault::InvalidSourceOrigin));
            };
            let dt = match &meta.datatype {
                SourceDatatype::Absent => None,
                SourceDatatype::Specified(dt) => Some(dt.as_str()),
                SourceDatatype::Unknown => {
                    return Err(fault(RawNumericFault::UnknownSourceLiteralIdentity));
                }
            };
            let SourceFact::Known(language) = &meta.language_original else {
                return Err(fault(RawNumericFault::UnknownSourceLiteralIdentity));
            };
            (v.value(), "literal", dt, language.as_deref())
        }
        #[cfg(feature = "sparql-12")]
        Term::Triple(_) => {
            return Err(QueryEvaluationError::UnsupportedSourceRuntimeTerm("Triple"));
        }
    };
    let term = match kind {
        "iri" => source_bridge::SourceTermInput::Iri(value),
        "blank_node" => source_bridge::SourceTermInput::Blank(value),
        _ => source_bridge::SourceTermInput::Literal {
            value,
            datatype,
            language,
        },
    };
    dto_from_source_term(term)
}
/// Shared presentation of an already source-identified term. Raw DTO callers
/// still perform their original provenance checks above; this is no constructor.
pub(super) fn dto_from_source_term(
    term: source_bridge::SourceTermInput<'_>,
) -> Result<Value, QueryEvaluationError> {
    use source_bridge::SourceTermInput;
    let (value, kind, datatype, language) = match term {
        SourceTermInput::Variable(_) => {
            return Err(QueryEvaluationError::UnsupportedSourceRuntimeTerm(
                "Variable",
            ));
        }
        SourceTermInput::Iri(v) => (v, "iri", None, None),
        SourceTermInput::Blank(v) => (v, "blank_node", None, None),
        SourceTermInput::Literal {
            value,
            datatype,
            language,
        } => (value, "literal", datatype, language),
    };
    Ok(json!({"value":value,"term_type":kind,"datatype":datatype,"language":language}))
}
struct ByteBudget(usize);
impl Write for ByteBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|v| *v <= 16 * 1024 * 1024)
            .ok_or_else(|| io::Error::other("source_runtime_result_byte_budget"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl ByteBudget {
    fn add(&mut self, value: &Value) -> Result<(), QueryEvaluationError> {
        serde_json::to_writer(self, value)
            .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))
    }
}
fn triple_sort_key(triple: &Value) -> [String; 7] {
    [
        triple["subject"]["term_type"].as_str().unwrap_or(""),
        triple["subject"]["value"].as_str().unwrap_or(""),
        triple["predicate"]["value"].as_str().unwrap_or(""),
        triple["object"]["term_type"].as_str().unwrap_or(""),
        triple["object"]["value"].as_str().unwrap_or(""),
        triple["object"]["datatype"].as_str().unwrap_or(""),
        triple["object"]["language"].as_str().unwrap_or(""),
    ]
    .map(str::to_owned)
}
pub(super) fn consume<'a, T: Clone + 'a>(
    form: ResultForm,
    mut stream: PreparedSourceRows<'a, T>,
) -> Result<Value, QueryEvaluationError> {
    stream.cancellation.ensure_alive()?;
    // ASK consumes the first actual solution, including an empty mapping. It
    // neither materializes a SELECT row nor evaluates a later generator item.
    if matches!(form, ResultForm::Ask) {
        return Ok(json!({"query_type":"ASK","boolean":stream.rows.next().transpose()?.is_some()}));
    }
    let mut payload = ByteBudget(0);
    let result = match form {
        ResultForm::Select => {
            let variables: Vec<_> = stream.variables.iter().map(|v| v.as_str()).collect();
            let mut rows = Vec::new();
            for (ordinal, row) in stream.rows.enumerate() {
                stream.cancellation.ensure_alive()?;
                if ordinal >= 10_000 {
                    return Err(error("source_runtime_result_row_budget"));
                }
                let row = row?;
                // RDFLib Result.__iter__ drops an empty binding dictionary.
                // A present None makes that dictionary nonempty, so the public
                // wrapper retains its row while omitting that individual cell.
                if !row.clone().into_cells().any(|v| v.is_some()) {
                    continue;
                }
                let mut pairs = Vec::new();
                for (slot, variable) in stream.variables.iter().enumerate() {
                    match row.cell(slot) {
                        Some(SolutionValue::Rdf(raw)) => pairs.push(json!([
                            variable.as_str(),
                            dto(&(stream.codec.externalize)(raw.clone())?)?
                        ])),
                        Some(SolutionValue::ReturnedError(_)) => {
                            return Err(QueryEvaluationError::UnsupportedSourceRuntimeTerm(
                                "SPARQLError",
                            ));
                        }
                        None | Some(SolutionValue::ExplicitNone) => {}
                    }
                }
                let row = json!({"values":pairs});
                payload.add(&row)?;
                rows.push(row);
            }
            json!({"query_type":"SELECT","variables":variables,"rows":rows})
        }
        ResultForm::Construct(template) => {
            let mut seen: HashSet<[SourceTermKey; 3]> = HashSet::new();
            let mut triples = Vec::new();
            for (ordinal, row) in stream.rows.enumerate() {
                stream.cancellation.ensure_alive()?;
                if ordinal >= 10_000 {
                    return Err(error("source_runtime_result_row_budget"));
                }
                let row = row?;
                let mut blanks = HashMap::new();
                for triple in &template {
                    let mut values = Vec::with_capacity(3);
                    for term in triple {
                        let raw = match term {
                            TemplateTerm::Constant(raw) => Some(raw.clone()),
                            TemplateTerm::Blank(id) => Some(
                                blanks
                                    .entry(id.clone())
                                    .or_insert_with(|| {
                                        RawExpressionValue::from_import(BlankNode::default().into())
                                            .ensure_source_object()
                                    })
                                    .clone(),
                            ),
                            TemplateTerm::Variable(var) => {
                                let local = stream
                                    .variables
                                    .iter()
                                    .position(|v| v == var)
                                    .and_then(|slot| row.cell(slot));
                                match local.or_else(|| stream.initial.get(var)) {
                                    Some(SolutionValue::Rdf(raw)) => {
                                        Some((stream.codec.externalize)(raw.clone())?)
                                    }
                                    None | Some(SolutionValue::ExplicitNone) => None,
                                    Some(SolutionValue::ReturnedError(_)) => {
                                        return Err(error(
                                            "Unsupported RDF term returned by backend: SPARQLError",
                                        ));
                                    }
                                }
                            }
                        };
                        if let Some(raw) = raw {
                            values.push(raw);
                        } else {
                            break;
                        }
                    }
                    if values.len() != 3 {
                        continue;
                    }
                    let keys = [
                        source_identity_key(&values[0]).map_err(fault)?,
                        source_identity_key(&values[1]).map_err(fault)?,
                        source_identity_key(&values[2]).map_err(fault)?,
                    ];
                    if seen.insert(keys) {
                        if triples.len() >= 100_000 {
                            return Err(error("source_runtime_result_triple_budget"));
                        }
                        let value = json!({"subject":dto(&values[0])?,"predicate":dto(&values[1])?,"object":dto(&values[2])?});
                        payload.add(&value)?;
                        triples.push(value);
                    }
                }
            }
            // Original DTO role checks occur after graph materialization.
            for triple in &triples {
                if triple["subject"]["term_type"] == "literal" {
                    return Err(error("an RDF triple subject cannot be a literal"));
                }
                if triple["predicate"]["term_type"] != "iri" {
                    return Err(error("an RDF triple predicate must be an IRI"));
                }
            }
            triples.sort_by_cached_key(triple_sort_key);
            json!({"query_type":"CONSTRUCT","triples":triples})
        }
        ResultForm::Ask => unreachable!("ASK consumed above"),
    };
    ByteBudget(0).add(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_runtime_results_preserve_unknown_metadata_and_exact_byte_budget() {
        let raw = RawExpressionValue::from_import(
            oxrdf::Literal::new_simple_literal("unknown source").into(),
        );
        assert!(dto(&raw).is_err());
        let mut budget = ByteBudget(0);
        budget.write_all(&vec![b'x'; 16 * 1024 * 1024]).unwrap();
        assert_eq!(budget.0, 16 * 1024 * 1024);
        assert!(budget.write_all(b"x").is_err());
        assert_eq!(budget.0, 16 * 1024 * 1024);
    }
}
