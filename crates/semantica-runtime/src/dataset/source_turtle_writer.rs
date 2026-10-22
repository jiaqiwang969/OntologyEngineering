//! Complete internal Turtle document traversal, RDFLib7.6.0 (BSD-3-Clause).
//! Source/native validity and public protocol admission remain separate.
use super::source_turtle_read::{TurtleRead, TurtleTerm};
use super::{
    source_namespace::{NamespaceState, can_relativize},
    source_turtle::PreparedTurtle,
};
use crate::rdflib_snapshot::Node;
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SUBJECT: u8 = 0;
const VERB: u8 = 1;
const OBJECT: u8 = 2;
fn node<T: TurtleTerm>(t: &T) -> Result<Node> {
    Ok(super::source_update::node(t.term()?))
}
pub(super) struct Writer<'a, G: TurtleRead> {
    prepared: PreparedTurtle,
    graph: G,
    names: &'a mut NamespaceState,
    named: bool,
    refs: BTreeMap<Node, usize>,
    done: BTreeSet<Node>,
    depth: usize,
    output: String,
    bytes: usize,
    entries: usize,
}
impl<'a, G: TurtleRead> Writer<'a, G> {
    pub(super) fn new(
        prepared: PreparedTurtle,
        graph: G,
        names: &'a mut NamespaceState,
        named: bool,
        bytes: usize,
        entries: usize,
    ) -> Self {
        Self {
            prepared,
            graph,
            names,
            named,
            refs: BTreeMap::new(),
            done: BTreeSet::new(),
            depth: 0,
            output: String::new(),
            bytes,
            entries,
        }
    }
    fn write(&mut self, text: &str) -> Result<()> {
        self.graph.charge(1)?;
        ensure!(
            self.output
                .len()
                .checked_add(text.len())
                .is_some_and(|n| n <= self.bytes),
            "source_turtle_output_budget"
        );
        self.output.try_reserve(text.len())?;
        self.output.push_str(text);
        Ok(())
    }
    fn indent(&self, extra: usize) -> Result<String> {
        ensure!(self.depth + extra <= 128, "source_turtle_depth_budget");
        Ok("    ".repeat(self.depth + extra))
    }
    pub(super) fn render(mut self) -> Result<String> {
        let rows = self.graph.scan([None, None, None])?;
        let mut subjects = Vec::new();
        let mut seen = BTreeSet::new();
        for [s, p, o] in rows {
            *self.refs.entry(node(&o)?).or_default() += 1;
            if matches!(node(&p)?, Node::Blank(_)) {
                *self.refs.entry(node(&p)?).or_default() += 1;
            }
            if seen.insert(node(&s)?) {
                subjects.push(s);
            }
        }
        let kind = self.graph.iri(&format!("{RDF}type"))?;
        let class = self.graph.iri(&format!("{RDFS}Class"))?;
        let mut tops = self
            .graph
            .scan([None, Some(&kind), Some(&class)])?
            .into_iter()
            .map(|[s, _, _]| s)
            .collect();
        self.graph.sort(&mut tops, None)?;
        let topkeys: BTreeSet<_> = tops.iter().map(node).collect::<Result<_>>()?;
        let mut rest = subjects
            .into_iter()
            .filter(|s| !topkeys.contains(&node(s).expect("observed source term")))
            .collect::<Vec<_>>();
        let prefixes = rest
            .iter()
            .map(|s| {
                let n = node(s)?;
                Ok((
                    matches!(n, Node::Blank(_)),
                    *self.refs.get(&n).unwrap_or(&0),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        self.graph.sort(&mut rest, Some(&prefixes))?;
        tops.extend(rest);
        if let Some(base) = self.prepared.base.clone().filter(|v| !v.is_empty()) {
            self.write(&format!("@base <{base}> .\n"))?;
        }
        for (p, n) in self.prepared.names.namespaces.clone() {
            self.write(&format!("@prefix {p}: <{n}> .\n"))?;
        }
        for s in tops {
            let n = node(&s)?;
            if !self.done.insert(n.clone()) {
                continue;
            }
            self.write(&format!("\n{}", self.indent(0)?))?;
            if matches!(n, Node::Blank(_)) && *self.refs.get(&n).unwrap_or(&0) == 0 {
                self.write("[]")?;
            } else {
                self.path(&s, SUBJECT, false)?;
            }
            self.predicates(&s)?;
            self.write(" .\n")?;
        }
        self.write("\n")?;
        Ok(self.output)
    }
    fn pname(&mut self, uri: &str, generate: bool) -> Result<Option<String>> {
        self.prepared.names.pname(
            self.names,
            self.named,
            uri,
            generate,
            self.bytes,
            self.entries,
        )
    }
    fn label(&mut self, term: &G::Term, position: u8) -> Result<String> {
        let n = node(term)?;
        if n == Node::Iri(format!("{RDF}nil")) {
            return Ok("()".into());
        }
        if position == VERB && n == Node::Iri(format!("{RDF}type")) {
            return Ok("a".into());
        }
        if matches!(n, Node::Literal(_)) {
            let graph = &self.graph;
            let prepared = &mut self.prepared;
            let names = &mut *self.names;
            let (named, bytes, entries) = (self.named, self.bytes, self.entries);
            return super::source_turtle_literal::literal_n3(
                term,
                || graph.cached(term),
                |dt| {
                    prepared
                        .names
                        .pname(names, named, dt, false, bytes, entries)
                },
            );
        }
        let mut n = n;
        let text = term.term()?.cache_parts().1;
        if can_relativize(text, self.prepared.base.as_deref()) {
            let base = self.prepared.base.as_deref().unwrap();
            n = Node::Iri(text[base.len()..].into());
        }
        if let Node::Iri(uri) = &n {
            if let Some(name) = self.pname(uri, position == VERB)? {
                return Ok(name);
            }
            if uri.chars().any(|c| "<>\" {}|\\^`".contains(c)) {
                return Err(super::source_failure::SourceFailure::turtle_uri(uri).into());
            }
        }
        Ok(n.n3())
    }
    fn path(&mut self, term: &G::Term, position: u8, newline: bool) -> Result<()> {
        self.graph.charge(1)?;
        ensure!(self.depth <= 128, "source_turtle_depth_budget");
        let n = node(term)?;
        if matches!(n, Node::Blank(_))
            && position != SUBJECT
            && !self.done.contains(&n)
            && *self.refs.get(&n).unwrap_or(&0) <= 1
        {
            if !newline {
                self.write(" ")?;
            }
            if self.valid_list(term)? {
                self.write("(")?;
                self.depth += 1;
                let mut current = Some(term.clone());
                let mut seen = BTreeSet::new();
                while let Some(t) = current {
                    if !self.graph.truth(&t)? {
                        break;
                    }
                    ensure!(seen.insert(node(&t)?), "source_turtle_cyclic_list");
                    if let Some(item) = self.value(&t, "first")? {
                        self.path(&item, OBJECT, false)?;
                        self.done.insert(node(&t)?);
                    }
                    current = self.value(&t, "rest")?;
                }
                self.depth -= 1;
                self.write(" )")?;
            } else {
                self.done.insert(n);
                self.depth += 1;
                self.write("[")?;
                self.predicates(term)?;
                self.write(" ]")?;
                self.depth -= 1;
            }
        } else {
            if position != SUBJECT && !newline {
                self.write(" ")?;
            }
            let label = self.label(term, position)?;
            self.write(&label)?;
        }
        Ok(())
    }
    fn value(&self, subject: &G::Term, predicate: &str) -> Result<Option<G::Term>> {
        let p = self.graph.iri(&format!("{RDF}{predicate}"))?;
        Ok(self
            .graph
            .scan([Some(subject), Some(&p), None])?
            .into_iter()
            .next()
            .map(|[_, _, o]| o))
    }
    fn valid_list(&self, term: &G::Term) -> Result<bool> {
        // Only source exceptions are eligible for the original initial-value
        // catch. Local work/unknown guards must propagate, never become false.
        if self.value(term, "first")?.is_none() {
            return Ok(false);
        }
        let mut current = Some(term.clone());
        let mut seen = BTreeSet::new();
        while let Some(t) = current {
            if !self.graph.truth(&t)? {
                break;
            }
            let n = node(&t)?;
            ensure!(seen.insert(n.clone()), "source_turtle_cyclic_list");
            if n != Node::Iri(format!("{RDF}nil"))
                && self.graph.scan([Some(&t), None, None])?.len() != 2
            {
                return Ok(false);
            }
            current = self.value(&t, "rest")?;
        }
        Ok(true)
    }
    fn predicates(&mut self, subject: &G::Term) -> Result<()> {
        let mut properties: Vec<(G::Term, Vec<G::Term>)> = Vec::new();
        let mut positions = BTreeMap::new();
        for [_, p, o] in self.graph.scan([Some(subject), None, None])? {
            let at = if let Some(at) = positions.get(&node(&p)?) {
                *at
            } else {
                let at = properties.len();
                positions.insert(node(&p)?, at);
                properties.push((p, Vec::new()));
                at
            };
            properties[at].1.push(o);
        }
        for (_, objects) in &mut properties {
            self.graph.sort(objects, None)?;
        }
        let mut sorted = properties.iter().map(|(p, _)| p.clone()).collect();
        self.graph.sort(&mut sorted, None)?;
        let mut order = Vec::new();
        for uri in [format!("{RDF}type"), format!("{RDFS}label")] {
            if let Some(at) = positions.get(&Node::Iri(uri)) {
                order.push(*at);
            }
        }
        for p in sorted {
            let at = positions[&node(&p)?];
            if !order.contains(&at) {
                order.push(at);
            }
        }
        for (i, at) in order.into_iter().enumerate() {
            if i > 0 {
                self.write(&format!(" ;\n{}", self.indent(1)?))?;
            }
            let (p, objects) = &properties[at];
            self.path(p, VERB, i > 0)?;
            // Original `(count == 1) and 0 or 1` yields 1 even for one
            // object. Preserve that actual indentation, not its apparent intent.
            let extra = 1;
            self.depth += extra;
            for (j, o) in objects.iter().enumerate() {
                if j > 0 {
                    self.write(&format!(",\n{}", self.indent(1)?))?;
                }
                self.path(o, OBJECT, j > 0)?;
            }
            self.depth -= extra;
        }
        Ok(())
    }
}
