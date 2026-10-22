//! The RDF graph behind `ProvenanceManager::export_prov` and serialisers that write it the way
//! rdflib 7.6 does.
//!
//! The original builds an `rdflib.Graph` and calls `g.serialize(format=...)`. Here the graph is
//! a list of `oxigraph::model` (oxrdf) triples in insertion order, and the serialisers port
//! rdflib's `TurtleSerializer` (prefixed names, subject ordering, inline blank nodes),
//! `NTSerializer`, `JsonLDSerializer`, `XMLSerializer` and `HextuplesSerializer` for the terms
//! a PROV export contains (IRIs, blank nodes, `xsd:dateTime` literals).
//!
//! Where rdflib's output order depends on Python set iteration or on random blank-node ids
//! (N-Triples/JSON-LD/RDF-XML/HexTuples statement order, sibling blank nodes in Turtle), the
//! port uses insertion order instead; the set of statements is identical.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use oxigraph::model::{BlankNode, Graph, Literal, NamedNode, NamedOrBlankNode, Term, Triple};
use regex::Regex;
use serde_json::{Map, Value};

use sem_utils::SemanticaError;
use sem_utils::pycompat::json::{DumpsOptions, dumps};

use crate::isoformat::normalize_datetime_lexical;
use crate::storage::py_error;

/// The W3C PROV-O namespace.
pub const PROV_NS: &str = "http://www.w3.org/ns/prov#";
/// The RDF namespace.
pub const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `xsd:dateTime`.
pub const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const XMLNS: &str = "http://www.w3.org/XML/1998/namespace";

/// The namespaces a fresh `rdflib.Graph()` binds (rdflib 7.6 defaults).
const DEFAULT_BINDINGS: [(&str, &str); 29] = [
    ("brick", "https://brickschema.org/schema/Brick#"),
    ("csvw", "http://www.w3.org/ns/csvw#"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("dcat", "http://www.w3.org/ns/dcat#"),
    ("dcmitype", "http://purl.org/dc/dcmitype/"),
    ("dcterms", "http://purl.org/dc/terms/"),
    ("dcam", "http://purl.org/dc/dcam/"),
    ("doap", "http://usefulinc.com/ns/doap#"),
    ("foaf", "http://xmlns.com/foaf/0.1/"),
    ("geo", "http://www.opengis.net/ont/geosparql#"),
    ("odrl", "http://www.w3.org/ns/odrl/2/"),
    ("org", "http://www.w3.org/ns/org#"),
    ("prof", "http://www.w3.org/ns/dx/prof/"),
    ("prov", "http://www.w3.org/ns/prov#"),
    ("qb", "http://purl.org/linked-data/cube#"),
    ("schema", "https://schema.org/"),
    ("sh", "http://www.w3.org/ns/shacl#"),
    ("skos", "http://www.w3.org/2004/02/skos/core#"),
    ("sosa", "http://www.w3.org/ns/sosa/"),
    ("ssn", "http://www.w3.org/ns/ssn/"),
    ("time", "http://www.w3.org/2006/time#"),
    ("vann", "http://purl.org/vocab/vann/"),
    ("void", "http://rdfs.org/ns/void#"),
    ("wgs", "https://www.w3.org/2003/01/geo/wgs84_pos#"),
    ("owl", "http://www.w3.org/2002/07/owl#"),
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("xml", "http://www.w3.org/XML/1998/namespace"),
];

// ---------------------------------------------------------------------------------------------
// graph
// ---------------------------------------------------------------------------------------------

/// The PROV-O graph `export_prov` serialises: oxrdf triples in insertion order (duplicates
/// dropped, as in an rdflib `Graph`), plus the `ex:` namespace the entity IRIs are minted in.
#[derive(Debug, Clone, Default)]
pub struct ProvGraph {
    base: String,
    triples: Vec<Triple>,
    seen: HashSet<Triple>,
    blank_order: HashMap<String, usize>,
}

impl ProvGraph {
    /// An empty graph whose `ex:` namespace is `base`.
    pub fn new(base: impl Into<String>) -> Self {
        ProvGraph {
            base: base.into(),
            ..ProvGraph::default()
        }
    }

    /// The namespace bound to `ex`.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// `EX[id]`: the IRI minted for an identifier (plain concatenation, as rdflib's
    /// `Namespace.__getitem__`).
    pub fn uri(&self, id: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}{id}", self.base))
    }

    /// `PROV[name]`.
    pub fn prov(name: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{PROV_NS}{name}"))
    }

    /// A fresh blank node (`BNode()`: `N` + a random uuid4 hex id).
    pub fn bnode(&mut self) -> BlankNode {
        let id = format!("N{}", uuid::Uuid::new_v4().simple());
        let order = self.blank_order.len();
        self.blank_order.insert(id.clone(), order);
        BlankNode::new_unchecked(id)
    }

    /// `g.add((s, p, o))`; a statement already present is not added again.
    pub fn add(
        &mut self,
        subject: impl Into<NamedOrBlankNode>,
        predicate: NamedNode,
        object: impl Into<Term>,
    ) {
        let triple = Triple::new(subject, predicate, object);
        if self.seen.insert(triple.clone()) {
            self.triples.push(triple);
        }
    }

    /// The statements in insertion order.
    pub fn triples(&self) -> &[Triple] {
        &self.triples
    }

    /// Number of statements.
    pub fn len(&self) -> usize {
        self.triples.len()
    }

    /// Whether the graph has no statements.
    pub fn is_empty(&self) -> bool {
        self.triples.is_empty()
    }

    /// The statements as an `oxigraph::model::Graph` (e.g. to load into an oxigraph store).
    pub fn to_graph(&self) -> Graph {
        let mut graph = Graph::new();
        for triple in &self.triples {
            graph.insert(triple);
        }
        graph
    }

    fn blank_rank(&self, id: &str) -> usize {
        self.blank_order.get(id).copied().unwrap_or(usize::MAX)
    }
}

/// `Literal(value, datatype=XSD.dateTime)`: rdflib normalises a lexical form that
/// `datetime.fromisoformat` accepts to its `isoformat()`, and keeps any other text as is.
pub fn datetime_literal(value: &str) -> Literal {
    let lexical = normalize_datetime_lexical(value).unwrap_or_else(|| value.to_string());
    Literal::new_typed_literal(lexical, NamedNode::new_unchecked(XSD_DATE_TIME))
}

// ---------------------------------------------------------------------------------------------
// node model shared by the serialisers
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Node {
    Iri(String),
    Blank(String),
    Lit {
        lexical: String,
        datatype: Option<String>,
    },
}

impl Node {
    fn from_subject(subject: &NamedOrBlankNode) -> Node {
        match subject {
            NamedOrBlankNode::NamedNode(n) => Node::Iri(n.as_str().to_string()),
            NamedOrBlankNode::BlankNode(b) => Node::Blank(b.as_str().to_string()),
        }
    }

    #[allow(unreachable_patterns)]
    fn from_term(term: &Term) -> Node {
        match term {
            Term::NamedNode(n) => Node::Iri(n.as_str().to_string()),
            Term::BlankNode(b) => Node::Blank(b.as_str().to_string()),
            Term::Literal(l) => Node::Lit {
                lexical: l.value().to_string(),
                datatype: if l.language().is_some() {
                    None
                } else {
                    Some(l.datatype().as_str().to_string())
                },
            },
            other => Node::Iri(other.to_string()),
        }
    }

    fn is_blank(&self) -> bool {
        matches!(self, Node::Blank(_))
    }
}

struct Statements<'g> {
    graph: &'g ProvGraph,
    triples: Vec<(Node, String, Node)>,
}

impl<'g> Statements<'g> {
    fn new(graph: &'g ProvGraph) -> Self {
        let triples = graph
            .triples
            .iter()
            .map(|t| {
                (
                    Node::from_subject(&t.subject),
                    t.predicate.as_str().to_string(),
                    Node::from_term(&t.object),
                )
            })
            .collect();
        Statements { graph, triples }
    }

    /// Subjects in order of first appearance.
    fn subjects(&self) -> Vec<&Node> {
        let mut seen = HashSet::new();
        self.triples
            .iter()
            .filter_map(|(s, _, _)| seen.insert(s).then_some(s))
            .collect()
    }

    /// `predicate_objects(s)`: grouped by predicate in order of first appearance (rdflib's
    /// memory store keeps per-subject dicts).
    fn predicate_objects(&self, subject: &Node) -> IndexMap<&str, Vec<&Node>> {
        let mut out: IndexMap<&str, Vec<&Node>> = IndexMap::new();
        for (s, p, o) in &self.triples {
            if s == subject {
                out.entry(p.as_str()).or_default().push(o);
            }
        }
        out
    }

    fn compare_nodes(&self, a: &Node, b: &Node) -> std::cmp::Ordering {
        fn rank(node: &Node) -> u8 {
            match node {
                Node::Blank(_) => 0,
                Node::Iri(_) => 1,
                Node::Lit { .. } => 2,
            }
        }
        match (a, b) {
            (Node::Iri(x), Node::Iri(y)) => x.cmp(y),
            (Node::Blank(x), Node::Blank(y)) => self
                .graph
                .blank_rank(x)
                .cmp(&self.graph.blank_rank(y))
                .then_with(|| x.cmp(y)),
            (
                Node::Lit {
                    lexical: x,
                    datatype: dx,
                },
                Node::Lit {
                    lexical: y,
                    datatype: dy,
                },
            ) => x.cmp(y).then_with(|| dx.cmp(dy)),
            _ => rank(a).cmp(&rank(b)),
        }
    }
}

fn invalid_uri_error(uri: &str) -> SemanticaError {
    py_error(
        "builtins",
        "Exception",
        format!(
            "\"{uri}\" does not look like a valid URI, I cannot serialize this as N3/Turtle. Perhaps you wanted to urlencode it?"
        ),
    )
}

/// rdflib's `_is_valid_uri`.
fn is_valid_uri(uri: &str) -> bool {
    !uri.chars().any(|c| "<>\" {}|\\^`".contains(c))
}

/// `URIRef.n3()`.
fn uri_n3(uri: &str) -> Result<String, SemanticaError> {
    if is_valid_uri(uri) {
        Ok(format!("<{uri}>"))
    } else {
        Err(invalid_uri_error(uri))
    }
}

// ---------------------------------------------------------------------------------------------
// rdflib NamespaceManager.compute_qname
// ---------------------------------------------------------------------------------------------

static NAME_START: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[\p{Ll}\p{Lu}\p{Lo}\p{Lt}\p{Nl}]$").expect("valid class"));
static SPLIT_START: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[\p{Ll}\p{Lu}\p{Lo}\p{Lt}\p{Nl}\p{Nd}]$").expect("valid class"));
static NAME_CHAR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[\p{Ll}\p{Lu}\p{Lo}\p{Lt}\p{Nl}\p{Mc}\p{Me}\p{Mn}\p{Lm}\p{Nd}]$")
        .expect("valid class")
});
const ALLOWED_NAME_CHARS: [char; 8] = ['\u{00B7}', '\u{0387}', '-', '.', '_', '%', '(', ')'];

fn char_in(re: &Regex, c: char) -> bool {
    let mut buf = [0u8; 4];
    re.is_match(c.encode_utf8(&mut buf))
}

/// `split_uri(uri)` with the default `SPLIT_START_CATEGORIES`.
fn split_uri(uri: &str) -> Option<(String, String)> {
    if let Some(rest) = uri.strip_prefix(XMLNS) {
        let tail = rest.split(XMLNS).next().unwrap_or("");
        return Some((XMLNS.to_string(), tail.to_string()));
    }
    let chars: Vec<char> = uri.chars().collect();
    let length = chars.len() as isize;
    let at = |j: isize| -> char {
        let index = if j < 0 { length + j } else { j };
        chars[index as usize]
    };
    for i in 0..length {
        let c = at(-i - 1);
        if char_in(&NAME_CHAR, c) {
            continue;
        }
        if ALLOWED_NAME_CHARS.contains(&c) {
            continue;
        }
        let mut j = -1 - i;
        while j < length {
            let cj = at(j);
            if char_in(&SPLIT_START, cj) || cj == '_' {
                let split = if j < 0 { length + j } else { j } as usize;
                if split == 0 {
                    break;
                }
                let ns: String = chars[..split].iter().collect();
                let ln: String = chars[split..].iter().collect();
                return Some((ns, ln));
            }
            j += 1;
        }
        break;
    }
    None
}

/// `is_ncname(name)`.
fn is_ncname(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !char_in(&NAME_START, first) {
        return false;
    }
    chars.all(|c| char_in(&NAME_CHAR, c) || ALLOWED_NAME_CHARS.contains(&c))
}

#[derive(Default)]
struct TrieNode {
    children: IndexMap<String, usize>,
}

/// The parts of rdflib's `NamespaceManager` that `compute_qname` uses: bindings and the
/// namespace trie.
struct NamespaceManager {
    prefixes: IndexMap<String, String>,
    namespaces: HashMap<String, String>,
    nodes: Vec<TrieNode>,
    strie: HashMap<String, usize>,
    cache: HashMap<String, (String, String, String)>,
}

impl NamespaceManager {
    fn new(base: &str) -> Self {
        let mut nm = NamespaceManager {
            prefixes: IndexMap::new(),
            namespaces: HashMap::new(),
            nodes: vec![TrieNode::default()],
            strie: HashMap::new(),
            cache: HashMap::new(),
        };
        for (prefix, ns) in DEFAULT_BINDINGS {
            nm.bind(prefix, ns);
        }
        nm.bind("prov", PROV_NS);
        nm.bind("ex", base);
        nm
    }

    fn bind(&mut self, prefix: &str, namespace: &str) {
        if let Some(old) = self.prefixes.get(prefix).cloned() {
            self.namespaces.remove(&old);
        }
        if let Some(old_prefix) = self.namespaces.get(namespace).cloned() {
            self.prefixes.shift_remove(&old_prefix);
        }
        self.prefixes
            .insert(prefix.to_string(), namespace.to_string());
        self.namespaces
            .insert(namespace.to_string(), prefix.to_string());
        self.insert_trie(0, namespace);
    }

    fn prefix_of(&self, namespace: &str) -> Option<String> {
        self.namespaces.get(namespace).cloned()
    }

    fn new_node(&mut self) -> usize {
        self.nodes.push(TrieNode::default());
        self.nodes.len() - 1
    }

    /// `insert_trie(trie, value)`: returns the node holding `value`'s subtree.
    fn insert_trie(&mut self, node: usize, value: &str) -> usize {
        if let Some(&child) = self.nodes[node].children.get(value) {
            return child;
        }
        let mut multi_check: Option<usize> = None;
        let keys: Vec<String> = self.nodes[node].children.keys().cloned().collect();
        for key in keys {
            if value.len() > key.len() && value.starts_with(&key) {
                let child = self.nodes[node].children[&key];
                return self.insert_trie(child, value);
            } else if key.starts_with(value) {
                let holder = match multi_check {
                    Some(holder) => holder,
                    None => {
                        let fresh = self.new_node();
                        self.nodes[node].children.insert(value.to_string(), fresh);
                        multi_check = Some(fresh);
                        fresh
                    }
                };
                let moved = self.nodes[node]
                    .children
                    .shift_remove(&key)
                    .expect("key listed above");
                self.nodes[holder].children.insert(key, moved);
            }
        }
        if let Some(&child) = self.nodes[node].children.get(value) {
            return child;
        }
        let fresh = self.new_node();
        self.nodes[node].children.insert(value.to_string(), fresh);
        fresh
    }

    fn get_longest_namespace(&self, node: usize, value: &str) -> Option<String> {
        for (key, &child) in &self.nodes[node].children {
            if value.starts_with(key.as_str()) {
                return Some(
                    self.get_longest_namespace(child, value)
                        .unwrap_or_else(|| key.clone()),
                );
            }
        }
        None
    }

    /// `compute_qname(uri, generate)`.
    fn compute_qname(&mut self, uri: &str, generate: bool) -> Option<(String, String, String)> {
        if let Some(hit) = self.cache.get(uri) {
            return Some(hit.clone());
        }
        if !is_valid_uri(uri) {
            return None;
        }
        let (mut namespace, mut name) = match split_uri(uri) {
            Some(parts) => parts,
            None => {
                self.prefix_of(uri)?;
                (uri.to_string(), String::new())
            }
        };
        if !self.strie.contains_key(&namespace) {
            let node = self.insert_trie(0, &namespace);
            self.strie.insert(namespace.clone(), node);
        }
        let node = self.strie[&namespace];
        if !self.nodes[node].children.is_empty()
            && let Some(longest) = self.get_longest_namespace(node, uri)
        {
            name = uri[longest.len()..].to_string();
            namespace = longest;
        }
        let prefix = match self.prefix_of(&namespace) {
            Some(prefix) => prefix,
            None => {
                if !generate {
                    return None;
                }
                let mut num = 1;
                let prefix = loop {
                    let candidate = format!("ns{num}");
                    if !self.prefixes.contains_key(&candidate) {
                        break candidate;
                    }
                    num += 1;
                };
                self.bind(&prefix, &namespace);
                prefix
            }
        };
        let parts = (prefix, namespace, name);
        self.cache.insert(uri.to_string(), parts.clone());
        Some(parts)
    }

    /// `qname_strict(uri)` (used by RDF/XML for predicates).
    fn qname_strict(&mut self, uri: &str) -> Option<String> {
        let (prefix, _, name) = self.compute_qname(uri, true)?;
        if !is_ncname(&name) {
            return None;
        }
        Some(if prefix.is_empty() {
            name
        } else {
            format!("{prefix}:{name}")
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Turtle (rdflib TurtleSerializer)
// ---------------------------------------------------------------------------------------------

const SUBJECT: u8 = 0;
const VERB: u8 = 1;
const OBJECT: u8 = 2;

struct TurtleWriter<'g> {
    stmts: Statements<'g>,
    nm: NamespaceManager,
    namespaces: IndexMap<String, String>,
    ns_rewrite: HashMap<String, String>,
    references: HashMap<Node, usize>,
    serialized: HashSet<Node>,
    depth: usize,
    out: String,
}

/// `Literal._quote_encode()` (Turtle/N3).
fn turtle_quote(lexical: &str) -> String {
    if lexical.contains('\n') {
        let mut encoded = lexical.replace('\\', "\\\\");
        if lexical.contains("\"\"\"") {
            encoded = encoded.replace("\"\"\"", "\\\"\\\"\\\"");
        }
        let chars: Vec<char> = encoded.chars().collect();
        if chars.last() == Some(&'"') && chars.len() >= 2 && chars[chars.len() - 2] != '\\' {
            encoded.pop();
            encoded.push_str("\\\"");
        }
        format!("\"\"\"{}\"\"\"", encoded.replace('\r', "\\r"))
    } else {
        format!(
            "\"{}\"",
            lexical
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\r', "\\r")
        )
    }
}

/// `local.replace("(", r"\(").replace(")", r"\)")` and `\%` for a `%` not followed by two hex
/// digits.
fn escape_local(local: &str) -> String {
    let replaced = local.replace('(', "\\(").replace(')', "\\)");
    let chars: Vec<char> = replaced.chars().collect();
    let mut out = String::with_capacity(replaced.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '%' {
            let hex = |k: usize| chars.get(k).is_some_and(|ch| ch.is_ascii_hexdigit());
            if !(hex(i + 1) && hex(i + 2)) {
                out.push('\\');
            }
        }
        out.push(c);
    }
    out
}

impl<'g> TurtleWriter<'g> {
    fn new(graph: &'g ProvGraph) -> Self {
        TurtleWriter {
            stmts: Statements::new(graph),
            nm: NamespaceManager::new(graph.base()),
            namespaces: IndexMap::new(),
            ns_rewrite: HashMap::new(),
            references: HashMap::new(),
            serialized: HashSet::new(),
            depth: 0,
            out: String::new(),
        }
    }

    fn add_namespace(&mut self, prefix: &str, namespace: &str) -> String {
        let mut prefix = prefix.to_string();
        let conflicting = self
            .namespaces
            .get(&prefix)
            .is_some_and(|existing| existing != namespace);
        if prefix.starts_with('_') || conflicting {
            if !self.ns_rewrite.contains_key(&prefix) {
                let mut p = format!("p{prefix}");
                while self.namespaces.contains_key(&p) {
                    p = format!("p{p}");
                }
                self.ns_rewrite.insert(prefix.clone(), p);
            }
            prefix = self.ns_rewrite[&prefix].clone();
        }
        self.namespaces
            .insert(prefix.clone(), namespace.to_string());
        prefix
    }

    /// `get_pname(uri, gen_prefix)`.
    fn get_pname(&mut self, uri: &str, gen_prefix: bool) -> Option<String> {
        let (prefix, namespace, local) = match self.nm.compute_qname(uri, gen_prefix) {
            Some(parts) => parts,
            None => {
                let prefix = self.nm.prefix_of(uri)?;
                (prefix, uri.to_string(), String::new())
            }
        };
        let local = escape_local(&local);
        if local.ends_with('.') {
            return None;
        }
        let prefix = self.add_namespace(&prefix, &namespace);
        Some(format!("{prefix}:{local}"))
    }

    fn preprocess(&mut self) {
        let triples = self.stmts.triples.clone();
        for (s, p, o) in &triples {
            *self.references.entry(o.clone()).or_insert(0) += 1;
            for (i, node) in [(SUBJECT, s), (OBJECT, o)] {
                self.pname_node(node, i == VERB);
            }
            if p != RDF_TYPE {
                self.get_pname(p, true);
            }
        }
    }

    fn pname_node(&mut self, node: &Node, gen_prefix: bool) {
        match node {
            Node::Iri(uri) => {
                self.get_pname(uri, gen_prefix);
            }
            Node::Lit {
                datatype: Some(dt), ..
            } => {
                self.get_pname(dt, false);
            }
            _ => {}
        }
    }

    fn order_subjects(&self) -> Vec<Node> {
        let mut subjects: Vec<Node> = self.stmts.subjects().into_iter().cloned().collect();
        subjects.sort_by(|a, b| {
            a.is_blank()
                .cmp(&b.is_blank())
                .then_with(|| {
                    let ra = self.references.get(a).copied().unwrap_or(0);
                    let rb = self.references.get(b).copied().unwrap_or(0);
                    ra.cmp(&rb)
                })
                .then_with(|| self.stmts.compare_nodes(a, b))
        });
        subjects
    }

    fn indent(&self, modifier: usize) -> String {
        "    ".repeat(self.depth + modifier)
    }

    fn label(&mut self, node: &Node, position: u8) -> Result<String, SemanticaError> {
        match node {
            Node::Iri(uri) if uri == RDF_NIL => Ok("()".to_string()),
            Node::Iri(uri) if position == VERB && uri == RDF_TYPE => Ok("a".to_string()),
            Node::Lit { lexical, datatype } => {
                let encoded = turtle_quote(lexical);
                Ok(match datatype {
                    Some(dt) => {
                        let quoted = self
                            .get_pname(dt, false)
                            .unwrap_or_else(|| format!("<{dt}>"));
                        format!("{encoded}^^{quoted}")
                    }
                    None => encoded,
                })
            }
            Node::Iri(uri) => match self.get_pname(uri, position == VERB) {
                Some(pname) => Ok(pname),
                None => uri_n3(uri),
            },
            Node::Blank(id) => Ok(format!("_:{id}")),
        }
    }

    fn path(&mut self, node: &Node, position: u8, newline: bool) -> Result<(), SemanticaError> {
        if self.p_squared(node, position, newline)? {
            return Ok(());
        }
        if position != SUBJECT && !newline {
            self.out.push(' ');
        }
        let label = self.label(node, position)?;
        self.out.push_str(&label);
        Ok(())
    }

    fn p_squared(
        &mut self,
        node: &Node,
        position: u8,
        newline: bool,
    ) -> Result<bool, SemanticaError> {
        if !node.is_blank()
            || self.serialized.contains(node)
            || self.references.get(node).copied().unwrap_or(0) > 1
            || position == SUBJECT
        {
            return Ok(false);
        }
        if !newline {
            self.out.push(' ');
        }
        self.serialized.insert(node.clone());
        self.depth += 2;
        self.out.push('[');
        self.depth -= 1;
        self.predicate_list(node, false)?;
        self.out.push_str(" ]");
        self.depth -= 1;
        Ok(true)
    }

    fn predicate_list(&mut self, subject: &Node, newline: bool) -> Result<(), SemanticaError> {
        let mut properties: Vec<(String, Vec<Node>)> = self
            .stmts
            .predicate_objects(subject)
            .into_iter()
            .map(|(p, objects)| (p.to_string(), objects.into_iter().cloned().collect()))
            .collect();
        for (_, objects) in properties.iter_mut() {
            objects.sort_by(|a, b| self.stmts.compare_nodes(a, b));
        }
        properties
            .sort_by(|(a, _), (b, _)| (a != RDF_TYPE).cmp(&(b != RDF_TYPE)).then_with(|| a.cmp(b)));
        let Some(((first_p, first_objects), rest)) = properties.split_first() else {
            return Ok(());
        };
        self.path(&Node::Iri(first_p.clone()), VERB, newline)?;
        self.object_list(first_objects)?;
        for (predicate, objects) in rest {
            let indent = self.indent(1);
            self.out.push_str(&format!(" ;\n{indent}"));
            self.path(&Node::Iri(predicate.clone()), VERB, true)?;
            self.object_list(objects)?;
        }
        Ok(())
    }

    fn object_list(&mut self, objects: &[Node]) -> Result<(), SemanticaError> {
        let Some((first, rest)) = objects.split_first() else {
            return Ok(());
        };
        // rdflib computes `(count == 1) and 0 or 1`, which is always 1.
        let depthmod = 1;
        self.depth += depthmod;
        self.path(first, OBJECT, false)?;
        for obj in rest {
            let indent = self.indent(1);
            self.out.push_str(&format!(",\n{indent}"));
            self.path(obj, OBJECT, true)?;
        }
        self.depth -= depthmod;
        Ok(())
    }

    fn statement(&mut self, subject: &Node) -> Result<(), SemanticaError> {
        self.serialized.insert(subject.clone());
        let unreferenced_blank =
            subject.is_blank() && self.references.get(subject).copied().unwrap_or(0) == 0;
        let indent = self.indent(0);
        if unreferenced_blank {
            self.out.push_str(&format!("\n{indent}[]"));
        } else {
            self.out.push_str(&format!("\n{indent}"));
            self.path(subject, SUBJECT, false)?;
        }
        self.predicate_list(subject, false)?;
        self.out.push_str(" .");
        Ok(())
    }

    fn serialize(mut self) -> Result<String, SemanticaError> {
        self.preprocess();
        let subjects = self.order_subjects();
        // startDocument writes the prefixes registered during preprocessing.
        let mut prefixes: Vec<(String, String)> = self
            .namespaces
            .iter()
            .map(|(p, n)| (p.clone(), n.clone()))
            .collect();
        prefixes.sort();
        let mut body = std::mem::take(&mut self.out);
        for (prefix, uri) in &prefixes {
            body.push_str(&format!("@prefix {prefix}: <{uri}> .\n"));
        }
        self.out = body;
        for subject in subjects {
            if self.serialized.contains(&subject) {
                continue;
            }
            self.statement(&subject)?;
            self.out.push('\n');
        }
        self.out.push('\n');
        Ok(self.out)
    }
}

// ---------------------------------------------------------------------------------------------
// N-Triples, JSON-LD, RDF/XML, HexTuples
// ---------------------------------------------------------------------------------------------

/// `nt._quote_encode`.
fn nt_quote(lexical: &str) -> String {
    format!(
        "\"{}\"",
        lexical
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
    )
}

fn nt_node(node: &Node) -> Result<String, SemanticaError> {
    match node {
        Node::Iri(uri) => uri_n3(uri),
        Node::Blank(id) => Ok(format!("_:{id}")),
        Node::Lit { lexical, datatype } => Ok(match datatype {
            Some(dt) => format!("{}^^<{dt}>", nt_quote(lexical)),
            None => nt_quote(lexical),
        }),
    }
}

fn serialize_ntriples(graph: &ProvGraph) -> Result<String, SemanticaError> {
    let stmts = Statements::new(graph);
    let mut out = String::new();
    for (s, p, o) in &stmts.triples {
        out.push_str(&format!(
            "{} {} {} .\n",
            nt_node(s)?,
            uri_n3(p)?,
            nt_node(o)?
        ));
    }
    Ok(out)
}

fn serialize_jsonld(graph: &ProvGraph) -> String {
    let stmts = Statements::new(graph);
    let objects: HashSet<&Node> = stmts.triples.iter().map(|(_, _, o)| o).collect();
    let mut nodemap: IndexMap<String, Map<String, Value>> = IndexMap::new();

    fn node_id(node: &Node) -> String {
        match node {
            Node::Iri(uri) => uri.clone(),
            Node::Blank(id) => format!("_:{id}"),
            Node::Lit { lexical, .. } => lexical.clone(),
        }
    }

    fn process_subject(
        stmts: &Statements<'_>,
        subject: &Node,
        nodemap: &mut IndexMap<String, Map<String, Value>>,
    ) {
        let id = node_id(subject);
        if nodemap.contains_key(&id) {
            return;
        }
        nodemap.insert(
            id.clone(),
            Map::from_iter([("@id".to_string(), Value::String(id.clone()))]),
        );
        for (predicate, objects) in stmts.predicate_objects(subject) {
            for object in objects {
                let (key, value) = if predicate == RDF_TYPE && matches!(object, Node::Iri(_)) {
                    ("@type".to_string(), Value::String(node_id(object)))
                } else {
                    let key = if predicate == RDF_TYPE {
                        "@type".to_string()
                    } else {
                        predicate.to_string()
                    };
                    let raw = match object {
                        Node::Blank(_) => {
                            process_subject(stmts, object, nodemap);
                            Value::Object(Map::from_iter([(
                                "@id".to_string(),
                                Value::String(node_id(object)),
                            )]))
                        }
                        Node::Iri(uri) => Value::Object(Map::from_iter([(
                            "@id".to_string(),
                            Value::String(uri.clone()),
                        )])),
                        Node::Lit { lexical, datatype } => {
                            let mut lit = Map::new();
                            if let Some(dt) = datatype {
                                lit.insert("@type".to_string(), Value::String(dt.clone()));
                            }
                            lit.insert("@value".to_string(), Value::String(lexical.clone()));
                            Value::Object(lit)
                        }
                    };
                    (key, raw)
                };
                let node = nodemap.get_mut(&id).expect("inserted above");
                match node.get_mut(&key) {
                    Some(Value::Array(values)) => values.push(value),
                    _ => {
                        node.insert(key, Value::Array(vec![value]));
                    }
                }
            }
        }
    }

    for subject in stmts.subjects() {
        if matches!(subject, Node::Iri(_)) || !objects.contains(subject) {
            process_subject(&stmts, subject, &mut nodemap);
        }
    }
    let doc = Value::Array(nodemap.into_values().map(Value::Object).collect());
    dumps(
        &doc,
        &DumpsOptions {
            indent: Some(2),
            ensure_ascii: false,
            sort_keys: true,
            separators: Some((",", ": ")),
        },
    )
}

/// `xml.sax.saxutils.escape(text, {"\r": "&#13;"})`.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;")
        .replace('\r', "&#13;")
}

/// `xml.sax.saxutils.quoteattr(text)`.
fn xml_quoteattr(text: &str) -> String {
    let data = text
        .replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;");
    if data.contains('"') {
        if data.contains('\'') {
            format!("\"{}\"", data.replace('"', "&quot;"))
        } else {
            format!("'{data}'")
        }
    } else {
        format!("\"{data}\"")
    }
}

fn serialize_rdfxml(graph: &ProvGraph) -> Result<String, SemanticaError> {
    let stmts = Statements::new(graph);
    let mut nm = NamespaceManager::new(graph.base());
    let mut bindings: IndexMap<String, String> = IndexMap::new();
    let mut seen_predicates: HashSet<&str> = HashSet::new();
    for (_, p, _) in &stmts.triples {
        if !seen_predicates.insert(p.as_str()) {
            continue;
        }
        let (prefix, namespace, name) = nm.compute_qname(p, true).ok_or_else(|| {
            SemanticaError::Value(format!(
                "This graph cannot be serialized to a strict format because there is no valid way to shorten {p}"
            ))
        })?;
        if !is_ncname(&name) {
            return Err(SemanticaError::Value(format!(
                "This graph cannot be serialized to a strict format because there is no valid way to shorten {p}"
            )));
        }
        bindings.insert(prefix, namespace);
    }
    bindings
        .entry("rdf".to_string())
        .or_insert_with(|| RDF_NS.to_string());
    let mut sorted: Vec<(String, String)> = bindings.into_iter().collect();
    sorted.sort();

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<rdf:RDF\n");
    for (prefix, namespace) in &sorted {
        if prefix.is_empty() {
            out.push_str(&format!("   xmlns=\"{namespace}\"\n"));
        } else {
            out.push_str(&format!("   xmlns:{prefix}=\"{namespace}\"\n"));
        }
    }
    out.push_str(">\n");
    for subject in stmts.subjects() {
        match subject {
            Node::Blank(id) => out.push_str(&format!("  <rdf:Description rdf:nodeID=\"{id}\"")),
            Node::Iri(uri) => out.push_str(&format!(
                "  <rdf:Description rdf:about={}",
                xml_quoteattr(uri)
            )),
            Node::Lit { .. } => continue,
        }
        out.push_str(">\n");
        for (predicate, objects) in stmts.predicate_objects(subject) {
            let qname = nm.qname_strict(predicate).ok_or_else(|| {
                SemanticaError::Value(format!(
                    "This graph cannot be serialized to a strict format because there is no valid way to shorten {predicate}"
                ))
            })?;
            for object in objects {
                match object {
                    Node::Lit { lexical, datatype } => {
                        let attrs = datatype
                            .as_ref()
                            .map(|dt| format!(" rdf:datatype=\"{dt}\""))
                            .unwrap_or_default();
                        out.push_str(&format!(
                            "    <{qname}{attrs}>{}</{qname}>\n",
                            xml_escape(lexical)
                        ));
                    }
                    Node::Blank(id) => {
                        out.push_str(&format!("    <{qname} rdf:nodeID=\"{id}\"/>\n"));
                    }
                    Node::Iri(uri) => {
                        out.push_str(&format!(
                            "    <{qname} rdf:resource={}/>\n",
                            xml_quoteattr(uri)
                        ));
                    }
                }
            }
        }
        out.push_str("  </rdf:Description>\n");
    }
    out.push_str("</rdf:RDF>\n");
    Ok(out)
}

fn serialize_hext(graph: &ProvGraph) -> String {
    let stmts = Statements::new(graph);
    let options = DumpsOptions {
        indent: None,
        ensure_ascii: false,
        sort_keys: false,
        separators: Some((",", ":")),
    };
    let mut out = String::new();
    for (s, p, o) in &stmts.triples {
        let subject = match s {
            Node::Blank(id) => format!("_:{id}"),
            Node::Iri(uri) => uri.clone(),
            Node::Lit { lexical, .. } => lexical.clone(),
        };
        let (value, datatype) = match o {
            Node::Iri(uri) => (uri.clone(), "globalId".to_string()),
            Node::Blank(id) => (format!("_:{id}"), "localId".to_string()),
            Node::Lit { lexical, datatype } => (
                lexical.clone(),
                datatype
                    .clone()
                    .unwrap_or_else(|| "http://www.w3.org/2001/XMLSchema#string".to_string()),
            ),
        };
        let line = Value::Array(vec![
            Value::String(subject),
            Value::String(p.clone()),
            Value::String(value),
            Value::String(datatype),
            Value::String(String::new()),
            Value::String(String::new()),
        ]);
        out.push_str(&dumps(&line, &options));
        out.push('\n');
    }
    out
}

/// `g.serialize(format=fmt)` for the formats rdflib registers.
pub fn serialize(graph: &ProvGraph, format: &str) -> Result<String, SemanticaError> {
    match format {
        "turtle" | "ttl" | "text/turtle" | "n3" | "text/n3" => TurtleWriter::new(graph).serialize(),
        "nt" | "ntriples" | "nt11" | "application/n-triples" => serialize_ntriples(graph),
        "json-ld" | "application/ld+json" => Ok(serialize_jsonld(graph)),
        "xml" | "application/rdf+xml" => serialize_rdfxml(graph),
        "hext" => Ok(serialize_hext(graph)),
        "nquads" | "application/n-quads" => Err(py_error(
            "builtins",
            "Exception",
            "NQuads serialization only makes sense for context-aware stores!",
        )),
        "trix" | "application/trix" => Err(py_error(
            "builtins",
            "Exception",
            "TriX serialization only makes sense for context-aware stores",
        )),
        "patch" => Err(SemanticaError::Attribute(
            "'Graph' object has no attribute 'contexts'".to_string(),
        )),
        "pretty-xml" | "longturtle" | "trig" | "application/trig" => {
            Err(SemanticaError::NotImplemented(format!(
                "rdflib serializer {format:?} is not available in the Rust port; use turtle, nt, json-ld or xml"
            )))
        }
        other => Err(py_error(
            "rdflib.plugin",
            "PluginException",
            format!("No plugin registered for ({other}, <class 'rdflib.serializer.Serializer'>)"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_uri_matches_rdflib() {
        let split = |u: &str| split_uri(u);
        assert_eq!(
            split("https://semantica.dev/ns#e1"),
            Some(("https://semantica.dev/ns#".into(), "e1".into()))
        );
        assert_eq!(split("https://semantica.dev/ns#"), None);
        assert_eq!(
            split("https://semantica.dev/ns#-x"),
            Some(("https://semantica.dev/ns#-".into(), "x".into()))
        );
        assert_eq!(
            split("https://semantica.dev/ns#_x"),
            Some(("https://semantica.dev/ns#".into(), "_x".into()))
        );
    }

    #[test]
    fn escapes_local_names() {
        assert_eq!(escape_local("a(b)"), "a\\(b\\)");
        assert_eq!(escape_local("a%zz"), "a\\%zz");
        assert_eq!(escape_local("a%41"), "a%41");
    }
}
