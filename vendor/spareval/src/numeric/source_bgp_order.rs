//! Original algebra.reorderTriples over already constructed pattern objects.
//! The parser bridge and direct-constructor receiving share this one algorithm,
//! native list sort and comparison owner. No graph or expression interpreter.
use super::*;
use crate::eval::source_sort::{SourceSortError, source_sort_indices};
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum PatternVariable {
    Variable(oxrdf::Variable),
    BlankNode(oxrdf::BlankNode),
}
#[derive(Clone, Debug)]
pub(crate) enum SourceTripleOrderCell {
    Variable(oxrdf::Variable),
    BlankNode(oxrdf::BlankNode),
    Rdf(RawExpressionValue),
    Path(String),
}

#[derive(Clone, Debug)]
enum PatternOrderKey {
    Node(SourceOrderKey),
    Path(String),
}
impl PatternOrderKey {
    fn repr(&self, session: &QueryNumericSession) -> Result<String, RawNumericFault> {
        match self {
            Self::Path(v) => Ok(v.clone()),
            Self::Node(SourceOrderKey::UnboundVariable(v)) => Ok(format!(
                "rdflib.term.Variable({})",
                semantica_json::python_repr(&serde_json::json!(v.as_str()))
                    .map_err(|_| RawNumericFault::InvalidSourceOrigin)?
            )),
            Self::Node(SourceOrderKey::Rdf(v)) => {
                session.source_repr(&crate::tuple::SolutionValue::Rdf(v.clone()))
            }
            _ => Err(RawNumericFault::InvalidSourceOrigin),
        }
    }
    fn equal(&self, other: &Self) -> Result<bool, RawNumericFault> {
        Ok(match (self, other) {
            (Self::Path(a), Self::Path(b)) => a == b,
            (
                Self::Node(SourceOrderKey::UnboundVariable(a)),
                Self::Node(SourceOrderKey::UnboundVariable(b)),
            ) => a == b,
            (Self::Node(SourceOrderKey::Rdf(a)), Self::Node(SourceOrderKey::Rdf(b))) => {
                source_identity_key(a)? == source_identity_key(b)?
            }
            _ => false,
        })
    }
    fn less(&self, other: &Self, session: &QueryNumericSession) -> Result<bool, RawNumericFault> {
        match (self, other) {
            (Self::Node(a), Self::Node(b)) => session.source_graph_node_less(a, b),
            _ => Ok(self.repr(session)? < other.repr(session)?),
        }
    }
}
impl SourceTripleOrderCell {
    fn variable(&self) -> Option<PatternVariable> {
        match self {
            Self::Variable(v) => Some(PatternVariable::Variable(v.clone())),
            Self::BlankNode(v) => Some(PatternVariable::BlankNode(v.clone())),
            Self::Rdf(_) | Self::Path(_) => None,
        }
    }
    fn order_key(&self) -> PatternOrderKey {
        if let Self::Path(v) = self {
            return PatternOrderKey::Path(v.clone());
        }
        PatternOrderKey::Node(match self {
            Self::Variable(v) => SourceOrderKey::UnboundVariable(v.clone()),
            Self::BlankNode(v) => {
                SourceOrderKey::Rdf(RawExpressionValue::from_import(v.clone().into()))
            }
            Self::Rdf(v) => SourceOrderKey::Rdf(v.clone()),
            Self::Path(_) => unreachable!(),
        })
    }
}
fn resource(operation: &'static str) -> RawNumericFault {
    RawNumericFault::Core(NumericFault::ResourceLimit { operation })
}

pub(crate) fn source_reorder_triples(
    session: &QueryNumericSession,
    rows: &[[SourceTripleOrderCell; 3]],
) -> Result<Vec<usize>, RawNumericFault> {
    if session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
        return Err(RawNumericFault::InvalidSourceOrigin);
    }
    let mut counts = HashMap::<PatternVariable, usize>::new();
    counts
        .try_reserve(
            rows.len()
                .checked_mul(3)
                .ok_or_else(|| resource("source_bgp_order_cells"))?,
        )
        .map_err(|_| resource("source_bgp_order_counts"))?;
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(rows.len())
        .map_err(|_| resource("source_bgp_order_cells"))?;
    for row in rows {
        let row = row
            .each_ref()
            .map(|term| (term.variable(), term.order_key()));
        for (key, _) in &row {
            if let Some(key) = key {
                let count = counts.entry(key.clone()).or_default();
                *count = count
                    .checked_add(1)
                    .ok_or_else(|| resource("source_bgp_variable_count"))?;
            }
        }
        cells.push(row);
    }
    let mut order = Vec::new();
    order
        .try_reserve_exact(rows.len())
        .map_err(|_| resource("source_bgp_order_indices"))?;
    order.extend(0..rows.len());
    let mut known = HashSet::<PatternVariable>::new();
    known
        .try_reserve(counts.len())
        .map_err(|_| resource("source_bgp_order_known"))?;
    // Sort each tail, learn the whole leading block, then advance by ONE.
    for i in 0..order.len() {
        let mut decorated = Vec::new();
        decorated
            .try_reserve_exact(order.len() - i)
            .map_err(|_| resource("source_bgp_order_decorated"))?;
        for &row in &order[i..] {
            let unknown = cells[row]
                .iter()
                .filter(|(v, _)| v.as_ref().is_some_and(|v| !known.contains(v)))
                .count();
            let frequency = cells[row]
                .iter()
                .filter_map(|(v, _)| v.as_ref().and_then(|v| counts.get(v)))
                .try_fold(0_usize, |sum, count| sum.checked_add(*count))
                .ok_or_else(|| resource("source_bgp_order_frequency"))?;
            let literal = matches!(&cells[row][2].1, PatternOrderKey::Node(SourceOrderKey::Rdf(v)) if v.literal().is_some());
            decorated.push((row, (unknown, Reverse(frequency), !literal)));
        }
        let mut permutation = Vec::new();
        permutation
            .try_reserve_exact(decorated.len())
            .map_err(|_| resource("source_bgp_order_permutation"))?;
        permutation.extend(0..decorated.len());
        source_sort_indices(
            &mut permutation,
            false,
            |left, right| {
                let (a, ka) = &decorated[left];
                let (b, kb) = &decorated[right];
                if ka != kb {
                    return Ok(ka < kb);
                }
                for ((_, a), (_, b)) in cells[*a].iter().zip(&cells[*b]) {
                    let equal = a.equal(b)?;
                    // Tuple comparison stops at the first identity-unequal node,
                    // even when two numeric caches compare equal in value space.
                    if !equal {
                        return a.less(b, session);
                    }
                }
                Ok(false)
            },
            || Ok(()),
        )
        .map_err(|error| match error {
            SourceSortError::Callback(error) => error,
            SourceSortError::Allocation => resource("source_bgp_order_sort_allocation"),
            SourceSortError::IndexOverflow => resource("source_bgp_order_sort_index"),
        })?;
        for (at, &selected) in permutation.iter().enumerate() {
            order[i + at] = decorated[selected].0;
        }
        let unknown = decorated[permutation[0]].1.0;
        for &selected in &permutation {
            if decorated[selected].1.0 != unknown {
                break;
            }
            for (variable, _) in &cells[decorated[selected].0] {
                if let Some(variable) = variable {
                    known.insert(variable.clone());
                }
            }
        }
    }
    Ok(order)
}

impl PreparedSourceLiteralRegistry {
    pub(crate) fn order_native_graph_triples(
        &self,
        patterns: &[spargebra::SourceTriplePattern],
    ) -> Result<Vec<spargebra::SourceTriplePattern>, crate::QueryEvaluationError> {
        use spargebra::SourceTriplePredicate as P;
        let raw = |e| crate::QueryEvaluationError::Unexpected(Box::new(e));
        if !self.admits_path_execution(self.session()) {
            return Err(raw(RawNumericFault::InvalidSourceOrigin));
        }
        let convert = |v: &TermPattern| -> Result<SourceTripleOrderCell, RawNumericFault> {
            Ok(match v {
                TermPattern::Variable(v) => SourceTripleOrderCell::Variable(v.clone()),
                TermPattern::BlankNode(v) => {
                    self.record_graph_blank_node(v)?;
                    SourceTripleOrderCell::BlankNode(v.clone())
                }
                TermPattern::NamedNode(v) => {
                    SourceTripleOrderCell::Rdf(RawExpressionValue::from_import(v.clone().into()))
                }
                TermPattern::SourceLiteral(v) => SourceTripleOrderCell::Rdf(self.resolve(v)?),
                _ => return Err(RawNumericFault::InvalidSourceOrigin),
            })
        };
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(patterns.len())
            .map_err(|_| raw(resource("source_path_bgp_order_cells")))?;
        for p in patterns {
            let predicate = match &p.predicate {
                P::Rdf(NamedNodePattern::Variable(v)) => SourceTripleOrderCell::Variable(v.clone()),
                P::Rdf(NamedNodePattern::NamedNode(v)) => {
                    SourceTripleOrderCell::Rdf(RawExpressionValue::from_import(v.clone().into()))
                }
                P::Path { definition, .. } => SourceTripleOrderCell::Path(
                    crate::eval::source_path::source_path_text(
                        definition,
                        self.session(),
                        self,
                        &mut || Ok(()),
                    )?
                    .1,
                ),
            };
            cells.push([
                convert(&p.subject).map_err(raw)?,
                predicate,
                convert(&p.object).map_err(raw)?,
            ]);
        }
        let order = source_reorder_triples(self.session(), &cells).map_err(raw)?;
        let mut result = Vec::new();
        result
            .try_reserve_exact(order.len())
            .map_err(|_| raw(resource("source_path_bgp_order_output")))?;
        result.extend(order.into_iter().map(|i| patterns[i].clone()));
        Ok(result)
    }

    pub(crate) fn order_graph_triples(
        &self,
        patterns: &[TriplePattern],
    ) -> Result<Vec<TriplePattern>, RawNumericFault> {
        if !self.admits_graph_execution(self.session()) {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let convert = |term: &TermPattern| -> Result<SourceTripleOrderCell, RawNumericFault> {
            Ok(match term {
                TermPattern::Variable(v) => SourceTripleOrderCell::Variable(v.clone()),
                TermPattern::BlankNode(v) => {
                    self.record_graph_blank_node(v)?;
                    SourceTripleOrderCell::BlankNode(v.clone())
                }
                TermPattern::NamedNode(v) => {
                    SourceTripleOrderCell::Rdf(RawExpressionValue::from_import(v.clone().into()))
                }
                TermPattern::SourceLiteral(v) => SourceTripleOrderCell::Rdf(self.resolve(v)?),
                TermPattern::Literal(_) => return Err(RawNumericFault::InvalidSourceOrigin),
                #[cfg(feature = "sparql-12")]
                TermPattern::Triple(_) => {
                    return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                        "source BGP quoted triple ordering",
                    ));
                }
            })
        };
        let mut cells = Vec::new();
        cells
            .try_reserve_exact(patterns.len())
            .map_err(|_| resource("source_bgp_order_cells"))?;
        for pattern in patterns {
            let predicate = match &pattern.predicate {
                NamedNodePattern::NamedNode(v) => TermPattern::NamedNode(v.clone()),
                NamedNodePattern::Variable(v) => TermPattern::Variable(v.clone()),
            };
            cells.push([
                convert(&pattern.subject)?,
                convert(&predicate)?,
                convert(&pattern.object)?,
            ]);
        }
        let order = source_reorder_triples(self.session(), &cells)?;
        let mut result = Vec::new();
        result
            .try_reserve_exact(order.len())
            .map_err(|_| resource("source_bgp_order_output"))?;
        result.extend(order.into_iter().map(|i| patterns[i].clone()));
        Ok(result)
    }
}
