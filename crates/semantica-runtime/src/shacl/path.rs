//! W3C SHACL §2.3.1 path grammar and finite graph relations.
use super::{G, RDF, SH};
use anyhow::{Result, bail, ensure};
use oxigraph::model::Term;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub(super) enum Path {
    Predicate(String),
    Sequence(Vec<Self>),
    Alternative(Vec<Self>),
    Inverse(Box<Self>),
    ZeroOrMore(Box<Self>),
    OneOrMore(Box<Self>),
    ZeroOrOne(Box<Self>),
}
impl Path {
    pub(super) fn parse(shapes: &G, node: &Term) -> Result<Self> {
        Self::parse_inner(shapes, node, &mut BTreeSet::new(), 0)
    }
    fn parse_inner(
        shapes: &G,
        node: &Term,
        visiting: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<Self> {
        ensure!(depth < 32, "shacl_path_depth_budget");
        if let Term::NamedNode(iri) = node {
            return Ok(Self::Predicate(iri.as_str().into()));
        }
        ensure!(
            matches!(node, Term::BlankNode(_)),
            "shacl_path_node_invalid"
        );
        let key = node.to_string();
        ensure!(visiting.insert(key.clone()), "shacl_path_recursive");
        let statements = shapes
            .by_subject
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let path = if statements.iter().any(|(p, _)| p == &format!("{RDF}first")) {
            // The list grammar, rather than a heuristic first/rest walk, is
            // validated by G::list; all path members are validated recursively.
            let members = shapes.list(node)?;
            ensure!(
                members.len() >= 2,
                "shacl_sequence_path_requires_two_members"
            );
            Self::Sequence(
                members
                    .iter()
                    .map(|member| Self::parse_inner(shapes, member, visiting, depth + 1))
                    .collect::<Result<_>>()?,
            )
        } else {
            ensure!(statements.len() == 1, "shacl_path_constructor_cardinality");
            let (predicate, value) = &statements[0];
            let Some(local) = predicate.strip_prefix(SH) else {
                bail!("shacl_path_constructor_invalid");
            };
            if local == "alternativePath" {
                let members = shapes.list(value)?;
                ensure!(
                    members.len() >= 2,
                    "shacl_alternative_path_requires_two_members"
                );
                Self::Alternative(
                    members
                        .iter()
                        .map(|member| Self::parse_inner(shapes, member, visiting, depth + 1))
                        .collect::<Result<_>>()?,
                )
            } else {
                let inner = Box::new(Self::parse_inner(shapes, value, visiting, depth + 1)?);
                match local {
                    "inversePath" => Self::Inverse(inner),
                    "zeroOrMorePath" => Self::ZeroOrMore(inner),
                    "oneOrMorePath" => Self::OneOrMore(inner),
                    "zeroOrOnePath" => Self::ZeroOrOne(inner),
                    _ => bail!("shacl_path_constructor_invalid"),
                }
            }
        };
        visiting.remove(&key);
        Ok(path)
    }
    pub(super) fn values(&self, data: &G, focus: &Term, steps: &mut usize) -> Result<Vec<Term>> {
        Ok(self
            .walk(data, focus, false, steps)?
            .into_values()
            .collect())
    }
    fn walk(
        &self,
        data: &G,
        focus: &Term,
        inverse: bool,
        steps: &mut usize,
    ) -> Result<BTreeMap<String, Term>> {
        *steps += 1;
        ensure!(*steps <= 100_000, "shacl_path_step_budget");
        let mut result = BTreeMap::new();
        match self {
            Self::Predicate(predicate) => {
                let next = if inverse {
                    data.subjects(focus, predicate)
                } else {
                    data.objects(focus, predicate)
                };
                *steps = steps
                    .checked_add(next.len())
                    .ok_or_else(|| anyhow::anyhow!("shacl_path_step_budget"))?;
                ensure!(*steps <= 100_000, "shacl_path_step_budget");
                result.extend(next.into_iter().map(|t| (t.to_string(), t)));
            }
            Self::Inverse(inner) => return inner.walk(data, focus, !inverse, steps),
            Self::Sequence(paths) => {
                result.insert(focus.to_string(), focus.clone());
                let ordered: Box<dyn Iterator<Item = &Self>> = if inverse {
                    Box::new(paths.iter().rev())
                } else {
                    Box::new(paths.iter())
                };
                for path in ordered {
                    let mut next = BTreeMap::new();
                    for term in result.values() {
                        next.extend(path.walk(data, term, inverse, steps)?);
                    }
                    result = next;
                }
            }
            Self::Alternative(paths) => {
                for path in paths {
                    result.extend(path.walk(data, focus, inverse, steps)?);
                }
            }
            Self::ZeroOrOne(inner) => {
                result.insert(focus.to_string(), focus.clone());
                result.extend(inner.walk(data, focus, inverse, steps)?);
            }
            Self::ZeroOrMore(inner) | Self::OneOrMore(inner) => {
                if matches!(self, Self::ZeroOrMore(_)) {
                    result.insert(focus.to_string(), focus.clone());
                }
                let mut pending = vec![focus.clone()];
                let mut expanded = BTreeSet::new();
                while let Some(term) = pending.pop() {
                    if !expanded.insert(term.to_string()) {
                        continue;
                    }
                    for (key, value) in inner.walk(data, &term, inverse, steps)? {
                        result.insert(key.clone(), value.clone());
                        if !expanded.contains(&key) {
                            pending.push(value);
                        }
                    }
                }
            }
        }
        ensure!(result.len() <= 100_000, "shacl_path_value_budget");
        Ok(result)
    }
}
