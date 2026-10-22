//! RDFLib 7.6.0 derivation; copyright and BSD-3-Clause terms are retained
//! in docs/RDFLIB-SNAPSHOT-LICENSE.md at the workspace root.
//! Ordered source grammar reductions for collections and blank properties.
//! Syntax is captured during construction. The original literal handles and
//! parsed predicate operands also produce the physical row in that order.
use super::*;
use crate::SourceSyntaxValue as V;

#[doc(hidden)]
pub type SourceBlankNodeConstructor =
    Rc<dyn Fn() -> Result<BlankNode, crate::SourceLiteralBridgeError>>;

/// Ordinary source BNode(None): one owner shared by parsing and UPDATE.
/// This does not replace native blank-node defaults or supplied constructors.
#[doc(hidden)]
pub fn source_default_blank_node() -> BlankNode {
    // RDFLib's default BNode(None) concatenates N and uuid4().hex.
    // Fresh entropy is not a replay of a historical process's RNG state.
    let value = (random::<u128>() & 0xffffffffffff0fff3fffffffffffffff)
        | 0x00000000000040008000000000000000;
    BlankNode::new_unchecked(format!("N{value:032x}"))
}

impl ParserState {
    pub(super) fn source_blank_node(&mut self) -> Result<BlankNode, &'static str> {
        if !self.capture_source_graph_syntax() {
            return Ok(BlankNode::default());
        }
        if self.numeric_construction_error.is_some() {
            return Err("Source constructor already failed");
        }
        if let Some(construct) = &self.source_blank_node_constructor {
            return construct().map_err(|error| {
                self.numeric_construction_error = Some(error);
                "Source blank-node constructor failed"
            });
        }
        Ok(source_default_blank_node())
    }

    pub(super) fn capture_source_predicate(&mut self, verb: &ParsedVerb) {
        if let Some(V::Node(id)) = &verb.syntax {
            self.source_predicates.insert(*id, verb.expression.clone());
        }
    }

    fn source_term(value: &V) -> Result<TermPattern, &'static str> {
        Ok(match value {
            V::Variable(v) => v.clone().into(),
            V::NamedNode(v) => v.clone().into(),
            V::BlankNode(v) => v.clone().into(),
            V::Literal(v) => TermPattern::SourceLiteral(v.clone()),
            _ => return Err("Source graph term has no same-parse RDF operand"),
        })
    }

    pub(super) fn source_physical_row(
        &self,
        terms: &[V],
    ) -> Result<Vec<TripleOrPathPattern>, &'static str> {
        if terms.len() % 3 != 0 {
            return Err("Source row does not contain complete triples");
        }
        let mut out = Vec::new();
        for triple in terms.chunks_exact(3) {
            let predicate = match &triple[1] {
                V::NamedNode(v) => VariableOrPropertyPath::from(NamedNodePattern::from(v.clone())),
                V::Variable(v) => VariableOrPropertyPath::from(v.clone()),
                V::Node(id) => self
                    .source_predicates
                    .get(id)
                    .ok_or("Source predicate lacks its same-parse operand")?
                    .clone(),
                _ => return Err("Source predicate has no original operand"),
            };
            add_source_native_pattern(
                Self::source_term(&triple[0])?,
                predicate,
                ReifiedTerm {
                    term: Self::source_term(&triple[2])?,
                    reifiers: Vec::new(),
                    source: None,
                },
                &mut out,
                self.source_native_paths(),
            )?;
        }
        Ok(out)
    }

    pub(super) fn source_expanded_row(
        &self,
        subject: &TermPattern,
        source_subject: Option<&V>,
        properties: &[(ParsedVerb, Vec<ReifiedTerm>)],
    ) -> Option<Vec<V>> {
        if !self.capture_source_graph_syntax() {
            return None;
        }
        let subject = source_subject
            .cloned()
            .or_else(|| Self::source_graph_term(subject))?;
        let mut tokens = vec![subject];
        for (property_at, (predicate, objects)) in properties.iter().enumerate() {
            if property_at != 0 {
                tokens.push(V::Text(";".into()));
            }
            tokens.push(predicate.syntax.clone()?);
            for (object_at, object) in objects.iter().enumerate() {
                if !object.reifiers.is_empty() {
                    return None;
                }
                if object_at != 0 {
                    tokens.push(V::Text(",".into()));
                }
                tokens.push(
                    object
                        .source
                        .clone()
                        .or_else(|| Self::source_graph_term(&object.term))?,
                );
            }
        }
        expand_triples(&tokens)
    }

    pub(super) fn source_collection(
        &mut self,
        children: Vec<FocusedTripleOrPathPattern<TermPattern>>,
    ) -> Result<FocusedTripleOrPathPattern<TermPattern>, &'static str> {
        let mut terms = Vec::<V>::new();
        let mut other = Vec::new();
        for child in children {
            let value = child
                .source
                .or_else(|| Self::source_graph_term(&child.focus))
                .ok_or("Collection child lacks source syntax")?;
            let value = if let V::List(values) = value {
                let first = values
                    .first()
                    .ok_or("Empty source collection expansion")?
                    .clone();
                other.extend(values);
                first
            } else {
                value
            };
            let node = V::BlankNode(self.source_blank_node()?);
            if !terms.is_empty() {
                let previous = terms[terms.len() - 3].clone();
                terms.extend([previous, V::NamedNode(rdf::REST.into_owned()), node.clone()]);
            }
            terms.extend([node, V::NamedNode(rdf::FIRST.into_owned()), value]);
        }
        let last = terms
            .get(
                terms
                    .len()
                    .checked_sub(3)
                    .ok_or("Empty source collection")?,
            )
            .ok_or("Missing source list node")?
            .clone();
        terms.extend([
            last,
            V::NamedNode(rdf::REST.into_owned()),
            V::NamedNode(rdf::NIL.into_owned()),
        ]);
        terms.extend(other);
        let focus = Self::source_term(&terms[0])?;
        let patterns = self.source_physical_row(&terms)?;
        Ok(FocusedTripleOrPathPattern {
            focus,
            patterns,
            source: Some(V::List(terms)),
        })
    }

    pub(super) fn source_blank_properties(
        &mut self,
        properties: Vec<(ParsedVerb, Vec<ReifiedTerm>)>,
    ) -> Result<FocusedTripleOrPathPattern<TermPattern>, &'static str> {
        let focus = TermPattern::BlankNode(self.source_blank_node()?);
        let terms = self
            .source_expanded_row(&focus, None, &properties)
            .ok_or("Blank property lacks source syntax")?;
        let patterns = self.source_physical_row(&terms)?;
        Ok(FocusedTripleOrPathPattern {
            focus,
            patterns,
            source: Some(V::List(terms)),
        })
    }
}

/// parser.expandTriples, over actual grammar tokens, including the source
/// list branch's deliberately separate last-subject/predicate updates.
fn expand_triples(tokens: &[V]) -> Option<Vec<V>> {
    let mut out = Vec::new();
    let (mut subject, mut predicate) = (V::None, V::None);
    for (at, token) in tokens.iter().enumerate() {
        match token {
            V::Text(v) if v == "," => out.extend([subject.clone(), predicate.clone()]),
            V::Text(v) if v == ";" => {
                if tokens
                    .get(at + 1)
                    .is_none_or(|next| matches!(next,V::Text(v) if v == ";" || v == "."))
                {
                    continue;
                }
                out.push(subject.clone());
            }
            V::List(values) => {
                let first = values.first()?;
                if out.len() % 3 == 2 {
                    out.push(first.clone());
                }
                if values.len() > 1 {
                    out.extend(values.iter().cloned());
                }
                if tokens.get(at + 1).is_some_and(
                    |next| !matches!(next,V::Text(v) if matches!(v.as_str(),"." | "," | ";")),
                ) {
                    subject = first.clone();
                    predicate = V::None;
                    out.push(first.clone());
                }
            }
            V::Text(v) if v == "." => (),
            value => {
                out.push(value.clone());
                match out.len() % 3 {
                    1 => subject = value.clone(),
                    2 => predicate = value.clone(),
                    _ => (),
                }
            }
        }
    }
    Some(out)
}
