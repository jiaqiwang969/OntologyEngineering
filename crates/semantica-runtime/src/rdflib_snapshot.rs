//! Pinned RDFLib 7.6.0 graph canonicalization and Semantica dataset-v2 encoding.
//! This is a separate identity algorithm; the RDFC snapshot is never rewritten.
//! Port of RDFLib compare.py Color/_TripleCanonicalizer (BSD-3-Clause;
//! copyright RDFLib authors, retained in docs/RDFLIB-SNAPSHOT-LICENSE.md).
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use num_bigint::{BigInt, BigUint};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const ALGORITHM: &str = "semantica-rdf-dataset-c14n-v2";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const C14N: &str = "urn:semantica:dataset-c14n:v2:";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const WORK_BUDGET: u64 = 2_000_000;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Node {
    #[cfg(feature = "rdflib760-nullable-internal")]
    Variable(String),
    Iri(String),
    Blank(String),
    Literal(Literal),
}
#[derive(Clone, Debug)]
pub(crate) struct Literal {
    pub text: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
}
impl Literal {
    fn key(&self) -> (&str, &Option<String>, Option<String>) {
        (
            &self.text,
            &self.datatype,
            self.language.as_ref().map(|s| s.to_ascii_lowercase()),
        )
    }
}
impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}
impl Eq for Literal {}
impl PartialOrd for Literal {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Literal {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}
pub(crate) type Quad = (Node, Node, Node, Option<Node>);
type Triple = (Node, Node, Node);

#[derive(Clone)]
pub(crate) struct Projection {
    #[cfg(feature = "rdflib760-nullable-internal")]
    pub source_state: Option<spareval::SourceDatasetState>,
    pub quads: Option<BTreeSet<Quad>>,
    // Lossless old terms are an observation of actual parser bytes. This map
    // retains the real RDF1.1 terms, rather than inventing them from normalized
    // literals when a compatibility mutation deletes a different old term.
    pub core: BTreeMap<Quad, Vec<oxigraph::model::Quad>>,
    pub source_sha256: Vec<String>,
}
#[derive(Default)]
pub(crate) struct SnapshotCache(
    std::sync::Mutex<Option<semantica_contracts::SemanticRdfLibDatasetSnapshot>>,
);
impl Clone for SnapshotCache {
    fn clone(&self) -> Self {
        Self(std::sync::Mutex::new(self.get()))
    }
}
impl SnapshotCache {
    pub fn get(&self) -> Option<semantica_contracts::SemanticRdfLibDatasetSnapshot> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    pub fn set(&self, value: Option<semantica_contracts::SemanticRdfLibDatasetSnapshot>) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
    }
}
impl Default for Projection {
    fn default() -> Self {
        Self {
            #[cfg(feature = "rdflib760-nullable-internal")]
            source_state: None,
            quads: Some(BTreeSet::new()),
            core: BTreeMap::new(),
            source_sha256: vec![],
        }
    }
}
impl Projection {
    pub fn invalidate(&mut self) {
        #[cfg(feature = "rdflib760-nullable-internal")]
        {
            self.source_state = None;
        }
        self.quads = None;
        self.core.clear();
    }
    /// Require a complete native view. Source observations may contain original
    /// generalized terms that have no RDF1.1 representation in this owner.
    pub fn dataset(&self) -> Result<oxigraph::model::Dataset> {
        self.require_native_complete()?;
        self.native_projection()
    }
    pub fn require_native_complete(&self) -> Result<()> {
        ensure!(
            !self.core.values().any(Vec::is_empty),
            "source_native_projection_incomplete"
        );
        Ok(())
    }
    /// Materialized native projection of this same source owner. An explicit
    /// empty binding is legal only when strict RDF1.1 construction is impossible;
    /// it is not an invented surrogate, missing binding or a second Dataset.
    pub fn native_projection(&self) -> Result<oxigraph::model::Dataset> {
        let quads = self
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?;
        let mut result = oxigraph::model::Dataset::new();
        for q in quads {
            let bound = self
                .core
                .get(q)
                .context("rdflib_source_core_binding_missing")?;
            if bound.is_empty() {
                ensure!(
                    core_quad(q, None).is_err(),
                    "source_native_coverage_false_gap"
                );
            }
            for core in bound {
                result.insert(core);
            }
        }
        Ok(result)
    }
}
pub(crate) fn source_quad(
    raw: &Quad,
    document: u64,
    map: &BTreeMap<String, oxigraph::model::BlankNode>,
) -> Result<Quad> {
    let rename = |n: &Node| -> Result<Node> {
        Ok(match n {
            Node::Blank(id) => Node::Blank(
                map.get(
                    id.strip_prefix(&format!("source_{document}_"))
                        .context("rdflib_source_blank_domain_invalid")?,
                )
                .context("rdflib_source_blank_mapping_missing")?
                .as_str()
                .into(),
            ),
            other => other.clone(),
        })
    };
    Ok((
        rename(&raw.0)?,
        rename(&raw.1)?,
        rename(&raw.2)?,
        raw.3.as_ref().map(rename).transpose()?,
    ))
}
pub(crate) fn remap_quad(
    q: &Quad,
    map: &BTreeMap<String, oxigraph::model::BlankNode>,
) -> Result<Quad> {
    let rename = |n: &Node| -> Result<Node> {
        Ok(match n {
            Node::Blank(id) => Node::Blank(
                map.get(id)
                    .context("rdflib_package_blank_mapping_missing")?
                    .as_str()
                    .into(),
            ),
            other => other.clone(),
        })
    };
    Ok((
        rename(&q.0)?,
        rename(&q.1)?,
        rename(&q.2)?,
        q.3.as_ref().map(rename).transpose()?,
    ))
}
pub(crate) fn remap_core(
    q: &oxigraph::model::Quad,
    map: &BTreeMap<String, oxigraph::model::BlankNode>,
) -> Result<oxigraph::model::Quad> {
    use oxigraph::model::{GraphName, NamedOrBlankNode, Term};
    let mut q = q.clone();
    let blank = |id: &oxigraph::model::BlankNode| {
        map.get(id.as_str())
            .cloned()
            .context("rdflib_package_blank_mapping_missing")
    };
    if let NamedOrBlankNode::BlankNode(n) = &q.subject {
        q.subject = blank(n)?.into();
    }
    if let Term::BlankNode(n) = &q.object {
        q.object = blank(n)?.into();
    }
    if let GraphName::BlankNode(n) = &q.graph_name {
        q.graph_name = blank(n)?.into();
    }
    Ok(q)
}
pub(crate) fn normalized(raw: &BTreeSet<Quad>) -> Result<BTreeSet<Quad>> {
    raw.iter()
        .map(|(s, p, o, g)| {
            let mut o = o.clone();
            if let Node::Literal(l) = &mut o {
                normalize(l)?;
            }
            let g = g
                .clone()
                .filter(|g| *g != Node::Iri("urn:x-rdflib:default".into()));
            Ok((s.clone(), p.clone(), o, g))
        })
        .collect()
}
pub(crate) fn core_quad(
    quad: &Quad,
    document: Option<(u64, &BTreeMap<String, oxigraph::model::BlankNode>)>,
) -> Result<oxigraph::model::Quad> {
    use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
    let term = |n: &Node| -> Result<Term> {
        Ok(match n {
            #[cfg(feature = "rdflib760-nullable-internal")]
            Node::Variable(_) => bail!("source_Variable_has_no_native_RDF_term"),
            Node::Iri(s) => NamedNode::new(s)?.into(),
            Node::Blank(s) => {
                let id = if let Some((revision, map)) = document {
                    map.get(
                        s.strip_prefix(&format!("source_{revision}_"))
                            .context("rdflib_source_blank_domain_invalid")?,
                    )
                    .context("rdflib_source_blank_mapping_missing")?
                    .clone()
                } else {
                    BlankNode::new(s)?
                };
                id.into()
            }
            Node::Literal(l) => {
                if let Some(lang) = &l.language {
                    Literal::new_language_tagged_literal(&l.text, lang)?.into()
                } else if let Some(dtype) = &l.datatype {
                    Literal::new_typed_literal(&l.text, NamedNode::new(dtype)?).into()
                } else {
                    Literal::new_simple_literal(&l.text).into()
                }
            }
        })
    };
    let subject = match term(&quad.0)? {
        Term::NamedNode(n) => NamedOrBlankNode::NamedNode(n),
        Term::BlankNode(n) => NamedOrBlankNode::BlankNode(n),
        _ => bail!("rdflib_subject_invalid"),
    };
    let predicate = match term(&quad.1)? {
        Term::NamedNode(n) => n,
        _ => bail!("rdflib_predicate_invalid"),
    };
    let graph = match quad.3.as_ref().map(term).transpose()? {
        None => GraphName::DefaultGraph,
        Some(Term::NamedNode(n)) => n.into(),
        Some(Term::BlankNode(n)) => n.into(),
        _ => bail!("rdflib_graph_invalid"),
    };
    Ok(oxigraph::model::Quad {
        subject,
        predicate,
        object: term(&quad.2)?,
        graph_name: graph,
    })
}

impl Node {
    fn blank(&self) -> bool {
        matches!(self, Self::Blank(_))
    }
    pub(crate) fn n3(&self) -> String {
        match self {
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Variable(s) => format!("?{s}"),
            Self::Iri(s) => format!("<{s}>"),
            Self::Blank(s) => format!("_:{s}"),
            Self::Literal(l) => {
                let mut quoted = if l.text.contains('\n') {
                    let mut s = l
                        .text
                        .replace('\\', "\\\\")
                        .replace("\"\"\"", "\\\"\\\"\\\"");
                    if s.ends_with('"') && !s.ends_with("\\\"") {
                        s.insert(s.len() - 1, '\\');
                    }
                    format!("\"\"\"{}\"\"\"", s.replace('\r', "\\r"))
                } else {
                    format!(
                        "\"{}\"",
                        l.text
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('\r', "\\r")
                    )
                };
                if l.datatype.as_deref().is_some_and(|d| {
                    matches!(d.strip_prefix(XSD), Some("double" | "float" | "decimal"))
                }) {
                    let number = semantica_contracts::json::python_builtin_float(&json!(l.text));
                    if number.as_ref().is_ok_and(|n| n.is_infinite()) {
                        quoted = quoted.replace("inf", "INF").replace("Infinity", "INF");
                    }
                    if number.as_ref().is_ok_and(|n| n.is_nan()) {
                        quoted = quoted.replace("nan", "NaN");
                    }
                }
                if let Some(lang) = &l.language {
                    format!("{quoted}@{lang}")
                } else if let Some(datatype) = &l.datatype {
                    format!("{quoted}^^<{datatype}>")
                } else {
                    quoted
                }
            }
        }
    }
}

fn decimal(text: &str) -> Option<String> {
    let text = python_digits(text).replace('_', "");
    let s = text.trim();
    let (negative, s) = if let Some(s) = s.strip_prefix('-') {
        (true, s)
    } else {
        (false, s.strip_prefix('+').unwrap_or(s))
    };
    let special = s.to_ascii_lowercase();
    let sign = if negative { "-" } else { "" };
    if matches!(special.as_str(), "inf" | "infinity") {
        return Some(format!("{sign}Infinity"));
    }
    for (prefix, out) in [("snan", "sNaN"), ("nan", "NaN")] {
        if let Some(payload) = special.strip_prefix(prefix) {
            if payload.bytes().all(|c| c.is_ascii_digit()) {
                return Some(format!("{sign}{out}{}", payload.trim_start_matches('0')));
            }
        }
    }
    let (mantissa, exp) = s
        .split_once(['e', 'E'])
        .map_or(Some((s, 0_i64)), |(m, e)| e.parse().ok().map(|e| (m, e)))?;
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole
        .bytes()
        .chain(fraction.bytes())
        .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let raw = format!("{whole}{fraction}");
    let digits = raw.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let exponent = exp.checked_sub(fraction.len() as i64)?;
    let adjusted = exponent.checked_add(digits.len() as i64 - 1)?;
    if exponent.abs() > 10_000 || adjusted.abs() > 10_000 {
        return None;
    }
    let sign = if negative { "-" } else { "" };
    {
        let point = digits.len() as i64 + exponent;
        let number = if point <= 0 {
            format!("0.{}{digits}", "0".repeat((-point) as usize))
        } else if point >= digits.len() as i64 {
            format!(
                "{digits}{}",
                "0".repeat((point - digits.len() as i64) as usize)
            )
        } else {
            format!(
                "{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        };
        Some(format!("{sign}{number}"))
    }
}
fn python_digits(text: &str) -> String {
    text.chars()
        .map(|c| {
            semantica_contracts::json::python_decimal_digit(c).map_or(c, |n| (b'0' + n) as char)
        })
        .collect()
}
fn integer(text: &str) -> Option<String> {
    let text = python_digits(text);
    let s = text.trim_matches(|c| semantica_contracts::json::python_character_class(c).1);
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() {
        return None;
    }
    // Pinned Python 3.13 defaults to 4300 decimal digits. A conversion failure
    // preserves the original literal's text; it is not a partial integer.
    if digits.bytes().filter(u8::is_ascii_digit).count() > 4300 {
        return None;
    }
    let bytes = digits.as_bytes();
    for (i, c) in bytes.iter().enumerate() {
        if *c == b'_' {
            if i == 0
                || i + 1 == bytes.len()
                || !bytes[i - 1].is_ascii_digit()
                || !bytes[i + 1].is_ascii_digit()
            {
                return None;
            }
        } else if !c.is_ascii_digit() {
            return None;
        }
    }
    s.replace('_', "")
        .parse::<BigInt>()
        .ok()
        .map(|n| n.to_string())
}
pub(crate) fn normalize(l: &mut Literal) -> Result<()> {
    if matches!(
        l.datatype.as_deref(),
        Some(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral"
                | "http://www.w3.org/1999/02/22-rdf-syntax-ns#HTML"
        )
    ) {
        // Resource refusals belong to this owner boundary. They must not be
        // mistaken for converter parse errors that preserve their lexical form.
        ensure!(l.text.len() <= 10_000, "rdflib_xml_html_input_budget");
        ensure!(
            l.text.bytes().filter(|c| *c == b'<').count() <= 256,
            "rdflib_xml_html_structure_input_budget"
        );
        let text = crate::rdflib_xml_html::normalize(
            l.datatype.as_deref().expect("XML/HTML datatype matched"),
            &l.text,
        )?;
        ensure!(text.len() <= 100_000, "rdflib_xml_html_output_budget");
        l.text = text;
        return Ok(());
    }
    let Some(kind) = l.datatype.as_deref().and_then(|d| d.strip_prefix(XSD)) else {
        return Ok(());
    };
    match kind {
        "integer" | "long" | "int" | "short" | "byte" | "nonNegativeInteger"
        | "positiveInteger" | "unsignedLong" | "unsignedInt" | "unsignedShort" | "unsignedByte"
        | "negativeInteger" | "nonPositiveInteger" => {
            if let Some(n) = integer(&l.text) {
                l.text = n;
            }
        }
        "boolean" => {
            l.text = if matches!(l.text.to_ascii_lowercase().as_str(), "true" | "1") {
                "true"
            } else {
                "false"
            }
            .into()
        }
        "decimal" => {
            ensure!(l.text.len() <= 10_000, "rdflib_decimal_expansion_budget");
            if let Some((_, exp)) = l.text.split_once(['e', 'E']) {
                if let Ok(exp) = exp.parse::<i64>() {
                    ensure!(
                        exp.unsigned_abs() <= 10_000,
                        "rdflib_decimal_expansion_budget"
                    );
                }
            }
            if let Some(s) = decimal(&l.text) {
                l.text = s;
            }
        }
        "float" | "double" => {
            if let Ok(n) = semantica_contracts::json::python_builtin_float(&json!(l.text)) {
                l.text = if n.is_nan() {
                    "nan".into()
                } else if n == f64::INFINITY {
                    "inf".into()
                } else if n == f64::NEG_INFINITY {
                    "-inf".into()
                } else {
                    semantica_contracts::json::python_str(&json!(n))?
                };
            }
        }
        "normalizedString" => l.text = l.text.replace(['\t', '\n', '\r'], " "),
        "token" => {
            let text = l.text.replace(['\t', '\n', '\r'], " ");
            l.text = text
                .trim_matches(|c| semantica_contracts::json::python_character_class(c).1)
                .split(' ')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
        }
        // Remaining lexically stable datatypes preserve their exact term bytes.
        "string" => {}
        "time" | "date" | "dateTime" | "duration" | "dayTimeDuration" | "yearMonthDuration"
        | "hexBinary" | "base64Binary" => {
            l.text = crate::rdflib_literal_normalizers::normalize(
                l.datatype.as_deref().expect("XSD datatype matched"),
                &l.text,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Lossless lexical term projection, verified separately by the existing RDF
/// parser on the exact same bytes before it can enter DatasetSession state.
pub(crate) fn parse(
    bytes: &[u8],
    default_graph: Option<&str>,
    document: u64,
) -> Result<BTreeSet<Quad>> {
    let mut quads = BTreeSet::new();
    parse_emissions(bytes, default_graph, document, |quad| {
        quads.insert(quad);
        ensure!(quads.len() <= 100_000, "rdflib_quad_budget");
        Ok(())
    })?;
    Ok(quads)
}
/// Ordered parser observations, before source Graph set/index construction.
/// Repeated triples are kept; the old unique-quad and input-byte limits remain.
pub(crate) fn parse_ordered(
    bytes: &[u8],
    default_graph: Option<&str>,
    document: u64,
) -> Result<Vec<Quad>> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    parse_emissions(bytes, default_graph, document, |quad| {
        seen.insert(quad.clone());
        ensure!(seen.len() <= 100_000, "rdflib_quad_budget");
        rows.push(quad);
        Ok(())
    })?;
    Ok(rows)
}
fn parse_emissions(
    bytes: &[u8],
    default_graph: Option<&str>,
    document: u64,
    mut emit: impl FnMut(Quad) -> Result<()>,
) -> Result<()> {
    ensure!(bytes.len() <= crate::MAX_BYTES, "rdflib_source_byte_budget");
    let text = std::str::from_utf8(bytes)?;
    let mut parser = Parser { text, pos: 0 };
    while parser.space() {
        let subject = parser.node(document)?;
        let predicate = parser.node(document)?;
        let object = parser.node(document)?;
        ensure!(
            !matches!(subject, Node::Literal(_)) && matches!(predicate, Node::Iri(_)),
            "rdflib_rdf_term_position_invalid"
        );
        parser.space();
        let graph = if parser.peek() == Some('.') {
            default_graph.map(|s| Node::Iri(s.into()))
        } else {
            Some(parser.node(document)?)
        };
        ensure!(
            graph
                .as_ref()
                .is_none_or(|g| !matches!(g, Node::Literal(_))),
            "rdflib_graph_name_invalid"
        );
        parser.space();
        ensure!(parser.take() == Some('.'), "rdflib_quad_dot_missing");
        emit((subject, predicate, object, graph))?;
    }
    Ok(())
}
struct Parser<'a> {
    text: &'a str,
    pos: usize,
}
impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }
    fn take(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn space(&mut self) -> bool {
        loop {
            while self
                .peek()
                .is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
            {
                self.take();
            }
            if self.peek() == Some('#') {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.take();
                }
            } else {
                return self.peek().is_some();
            }
        }
    }
    fn escaped(&mut self) -> Result<char> {
        Ok(match self.take().context("rdflib_escape_truncated")? {
            't' => '\t',
            'b' => '\u{8}',
            'n' => '\n',
            'r' => '\r',
            'f' => '\u{c}',
            '"' => '"',
            '\'' => '\'',
            '\\' => '\\',
            kind @ ('u' | 'U') => {
                let mut hex = String::new();
                for _ in 0..if kind == 'u' { 4 } else { 8 } {
                    hex.push(self.take().context("rdflib_unicode_escape_truncated")?);
                }
                char::from_u32(u32::from_str_radix(&hex, 16)?)
                    .context("rdflib_unicode_escape_invalid")?
            }
            _ => bail!("rdflib_escape_invalid"),
        })
    }
    fn iri(&mut self) -> Result<String> {
        ensure!(self.take() == Some('<'), "rdflib_iri_required");
        let mut s = String::new();
        loop {
            match self.take().context("rdflib_iri_unclosed")? {
                '>' => break,
                '\\' => s.push(self.escaped()?),
                c => s.push(c),
            }
        }
        Ok(s)
    }
    fn node(&mut self, document: u64) -> Result<Node> {
        self.space();
        match self.peek().context("rdflib_term_missing")? {
            '<' => Ok(Node::Iri(self.iri()?)),
            '_' => {
                ensure!(
                    self.take() == Some('_') && self.take() == Some(':'),
                    "rdflib_blank_node_invalid"
                );
                let mut id = String::new();
                while self.peek().is_some_and(|c| !c.is_whitespace()) {
                    id.push(self.take().unwrap());
                }
                while id.ends_with('.') {
                    id.pop();
                    self.pos -= 1;
                }
                ensure!(!id.is_empty(), "rdflib_blank_node_empty");
                Ok(Node::Blank(format!("source_{document}_{id}")))
            }
            '"' => {
                self.take();
                let mut text = String::new();
                loop {
                    match self.take().context("rdflib_literal_unclosed")? {
                        '"' => break,
                        '\\' => text.push(self.escaped()?),
                        c => text.push(c),
                    }
                }
                let mut literal = Literal {
                    text,
                    datatype: None,
                    language: None,
                };
                if self.peek() == Some('@') {
                    self.take();
                    let mut lang = String::new();
                    while self
                        .peek()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
                    {
                        lang.push(self.take().unwrap());
                    }
                    literal.language = Some(lang);
                } else if self.peek() == Some('^') {
                    self.take();
                    ensure!(self.take() == Some('^'), "rdflib_datatype_marker_invalid");
                    literal.datatype = Some(self.iri()?);
                }
                Ok(Node::Literal(literal))
            }
            _ => bail!("rdflib_lexical_term_unsupported"),
        }
    }
}

#[derive(Clone)]
enum ColorData {
    Node(Node),
    Features(Vec<Vec<String>>),
}
#[derive(Clone)]
struct Color {
    id: u64,
    nodes: Vec<Node>,
    data: ColorData,
}
impl Color {
    fn hash(&self) -> String {
        match &self.data {
            ColorData::Node(n) => n.n3(),
            ColorData::Features(features) => {
                let mut sum = BigUint::default();
                for feature in features {
                    sum += BigUint::from_bytes_be(&Sha256::digest(feature.join(" ").as_bytes()));
                }
                sum.to_str_radix(16)
            }
        }
    }
    fn key(&self) -> (usize, String) {
        (self.nodes.len(), self.hash())
    }
    fn discrete(&self) -> bool {
        self.nodes.len() == 1
    }
}
struct Canonicalizer {
    next: u64,
    work: u64,
    edges: BTreeMap<(Node, Node), Vec<Node>>,
}
impl Canonicalizer {
    fn new(graph: &BTreeSet<Triple>) -> Self {
        let mut edges = BTreeMap::<(Node, Node), Vec<Node>>::new();
        for (s, p, o) in graph {
            edges
                .entry((s.clone(), o.clone()))
                .or_default()
                .push(p.clone());
        }
        Self {
            next: 0,
            work: 0,
            edges,
        }
    }
    fn charge(&mut self, n: u64) -> Result<()> {
        self.work = self
            .work
            .checked_add(n)
            .context("rdflib_canonical_work_overflow")?;
        ensure!(self.work <= WORK_BUDGET, "rdflib_canonical_work_budget");
        Ok(())
    }
    fn color(&mut self, nodes: Vec<Node>, data: ColorData) -> Color {
        let id = self.next;
        self.next += 1;
        Color { id, nodes, data }
    }
    fn copied(&mut self, c: &Color) -> Color {
        self.color(c.nodes.clone(), c.data.clone())
    }
    fn distinguish(&mut self, color: &Color, w: &Color) -> Result<Vec<Color>> {
        let mut grouped = BTreeMap::<String, Color>::new();
        for node in &color.nodes {
            let mut features = match &color.data {
                ColorData::Features(f) => f.clone(),
                _ => bail!("rdflib_blank_color_invalid"),
            };
            let whash = w.hash();
            for target in &w.nodes {
                self.charge(1)?;
                if let Some(predicates) = self.edges.get(&(node.clone(), target.clone())) {
                    for p in predicates {
                        features.push(vec!["1".into(), p.n3(), whash.clone()]);
                    }
                }
                if let Some(predicates) = self.edges.get(&(target.clone(), node.clone())) {
                    for p in predicates {
                        features.push(vec![whash.clone(), p.n3(), "3".into()]);
                    }
                }
            }
            let candidate = self.color(vec![], ColorData::Features(features));
            let hash = candidate.hash();
            grouped
                .entry(hash)
                .or_insert(candidate)
                .nodes
                .push(node.clone());
        }
        let mut colors = grouped.into_values().collect::<Vec<_>>();
        colors.sort_by_key(|c| std::cmp::Reverse(c.key()));
        Ok(colors)
    }
    fn refine(&mut self, mut coloring: Vec<Color>, mut sequence: Vec<Color>) -> Result<Vec<Color>> {
        sequence.sort_by_key(|c| std::cmp::Reverse(c.key()));
        while !sequence.is_empty() && !coloring.iter().all(Color::discrete) {
            let w = sequence.pop().unwrap();
            let original = coloring.clone();
            for c in original {
                if c.nodes.len() > 1 || c.nodes[0].blank() {
                    let colors = self.distinguish(&c, &w)?;
                    coloring.retain(|x| x.id != c.id);
                    coloring.extend(colors.clone());
                    if let Some(index) = sequence.iter().position(|x| x.id == c.id) {
                        sequence.splice(index..index + 1, colors);
                    } else {
                        sequence.splice(0..0, colors.into_iter().skip(1));
                    }
                }
            }
        }
        let mut combined = Vec::<Color>::new();
        let mut hashes = BTreeMap::<String, usize>::new();
        for color in coloring {
            let h = color.hash();
            if let Some(i) = hashes.get(&h) {
                combined[*i].nodes.extend(color.nodes);
            } else {
                hashes.insert(h, combined.len());
                combined.push(color);
            }
        }
        Ok(combined)
    }
    fn individuate(&mut self, color: &mut Color, node: &Node) -> Result<Color> {
        let mut data = match &color.data {
            ColorData::Features(f) => f.clone(),
            _ => bail!("rdflib_individual_color_invalid"),
        };
        data.push(vec![color.nodes.len().to_string()]);
        color.nodes.retain(|n| n != node);
        Ok(self.color(vec![node.clone()], ColorData::Features(data)))
    }
    fn experimental(&mut self, coloring: &[Color]) -> Result<Vec<Color>> {
        let mut current = coloring.iter().map(|c| self.copied(c)).collect::<Vec<_>>();
        while let Some(i) = current.iter().position(|c| !c.discrete()) {
            let n = current[i].nodes[0].clone();
            let new = self.individuate(&mut current[i], &n)?;
            current.push(new.clone());
            current = self.refine(current, vec![new])?;
        }
        Ok(current)
    }
    fn traces(&mut self, coloring: Vec<Color>, depth: usize) -> Result<Vec<Color>> {
        ensure!(depth <= 128, "rdflib_canonical_depth_budget");
        self.charge(1)?;
        let candidates = coloring
            .iter()
            .filter(|c| !c.discrete())
            .flat_map(|c| c.nodes.iter().map(|n| (n.clone(), c.id)))
            .collect::<Vec<_>>();
        let mut best = Vec::<Vec<Color>>::new();
        let mut score: Option<Vec<(usize, String)>> = None;
        let mut experimental_score = None;
        let mut last: Option<Vec<Color>> = None;
        let mut generator = BTreeMap::<Node, BTreeSet<Node>>::new();
        let mut visited = BTreeSet::<Node>::new();
        for (candidate, id) in candidates {
            if generator
                .get(&candidate)
                .is_some_and(|g| !g.is_disjoint(&visited))
            {
                visited.insert(candidate);
                continue;
            }
            visited.insert(candidate.clone());
            let mut copy = coloring.iter().map(|c| self.copied(c)).collect::<Vec<_>>();
            let index = coloring.iter().position(|c| c.id == id).unwrap();
            let new = self.individuate(&mut copy[index], &candidate)?;
            copy.push(new.clone());
            let refined = self.refine(copy.clone(), vec![new])?;
            let new_score = refined.iter().map(Color::key).collect::<Vec<_>>();
            let experimental = self.experimental(&copy)?;
            let new_experimental = experimental.iter().map(Color::key).collect::<BTreeSet<_>>();
            if let Some(previous) = last {
                for (a, b) in previous.iter().zip(&experimental) {
                    let group = BTreeSet::from([a.nodes[0].clone(), b.nodes[0].clone()]);
                    for n in &group {
                        generator.insert(n.clone(), group.clone());
                    }
                }
            }
            last = Some(experimental);
            match score.as_ref().map(|s| s.cmp(&new_score)) {
                None | Some(std::cmp::Ordering::Less) => {
                    best = vec![refined];
                    score = Some(new_score);
                    experimental_score = Some(new_experimental);
                }
                Some(std::cmp::Ordering::Equal)
                    if experimental_score.as_ref() != Some(&new_experimental) =>
                {
                    best.push(refined)
                }
                _ => {}
            }
        }
        if let Some(discrete) = best.iter().find(|c| c.iter().all(Color::discrete)) {
            return Ok(discrete.clone());
        }
        // Pinned RDFLib recursively selects the first equally-scored best
        // branch (its recursive comparison reads the last refined score).
        let selected = best
            .into_iter()
            .next()
            .context("rdflib_canonical_branch_missing")?;
        self.traces(selected, depth + 1)
    }
    fn labels(&mut self, graph: &BTreeSet<Triple>) -> Result<BTreeMap<Node, String>> {
        let mut blanks = BTreeSet::new();
        let mut others = BTreeSet::new();
        for (s, p, o) in graph {
            let terms = [s, p, o];
            if terms.iter().any(|n| n.blank()) {
                for n in terms {
                    if n.blank() {
                        blanks.insert(n.clone());
                    } else {
                        others.insert(n.clone());
                    }
                }
            }
        }
        ensure!(blanks.len() <= 1024, "rdflib_blank_node_budget");
        if blanks.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut colors =
            vec![self.color(blanks.into_iter().collect(), ColorData::Features(vec![]))];
        for n in others {
            colors.push(self.color(vec![n.clone()], ColorData::Node(n)));
        }
        let mut colors = self.refine(colors.clone(), colors)?;
        if !colors.iter().all(Color::discrete) {
            colors = self.traces(colors, 0)?;
        }
        Ok(colors
            .into_iter()
            .map(|c| (c.nodes[0].clone(), c.hash()))
            .collect())
    }
}

#[derive(Debug)]
pub(crate) struct PredicateNotIri;
impl std::fmt::Display for PredicateNotIri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RDF Dataset predicate must be an IRI")
    }
}
impl std::error::Error for PredicateNotIri {}

pub(crate) fn snapshot(
    quads: &BTreeSet<Quad>,
    revision: u64,
    created_at: &str,
) -> Result<semantica_contracts::SemanticRdfLibDatasetSnapshot> {
    ensure!(quads.len() <= 100_000, "rdflib_snapshot_quad_budget");
    let mut encoded = BTreeSet::new();
    let mut bindings = BTreeMap::<Node, (Node, Option<Node>)>::new();
    for (index, (s, p, o, g)) in quads.iter().enumerate() {
        if !matches!(p, Node::Iri(_)) {
            return Err(PredicateNotIri.into());
        }
        if matches!(g, Some(Node::Blank(_))) {
            let statement = Node::Blank(format!("encoded_statement_{index}"));
            for (pred, obj) in [
                (RDF_TYPE, Node::Iri(format!("{C14N}Quad"))),
                (&format!("{C14N}subject"), s.clone()),
                (&format!("{C14N}predicate"), p.clone()),
                (&format!("{C14N}object"), o.clone()),
                (&format!("{C14N}graph"), g.clone().unwrap()),
            ] {
                encoded.insert((statement.clone(), Node::Iri(pred.into()), obj));
            }
        } else {
            let payload = semantica_contracts::json::canonical_json(
                &json!({"graph":g.as_ref().map(Node::n3),"predicate":p.n3()}),
            )?;
            let encoded_pred = Node::Iri(format!(
                "{C14N}predicate:{}",
                URL_SAFE_NO_PAD.encode(payload)
            ));
            let binding = (p.clone(), g.clone());
            if let Some(old) = bindings.insert(encoded_pred.clone(), binding.clone()) {
                ensure!(old == binding, "rdflib_dataset_predicate_collision");
            }
            encoded.insert((s.clone(), encoded_pred, o.clone()));
        }
    }
    let mut canonicalizer = Canonicalizer::new(&encoded);
    let labels = canonicalizer.labels(&encoded)?;
    let mapped = |n: &Node| -> Result<Node> {
        if n.blank() {
            Ok(Node::Blank(format!(
                "cb{}",
                labels.get(n).context("rdflib_canonical_label_missing")?
            )))
        } else {
            Ok(n.clone())
        }
    };
    // The reification statement label is internal and never enters output.
    let mut lines = Vec::with_capacity(quads.len());
    for (s, p, o, g) in quads {
        let mut terms = vec![mapped(s)?.n3(), p.n3(), mapped(o)?.n3()];
        if let Some(g) = g {
            terms.push(mapped(g)?.n3());
        }
        lines.push(format!("{} .", terms.join(" ")));
    }
    lines.sort();
    let canonical_nquads = lines.join("\n");
    ensure!(
        canonical_nquads.len() <= crate::MAX_BYTES,
        "rdflib_canonical_byte_budget"
    );
    Ok(semantica_contracts::SemanticRdfLibDatasetSnapshot {
        schema_version: "1.0".into(),
        algorithm: ALGORITHM.into(),
        dataset_sha256: crate::sha256(&canonical_nquads),
        quad_count: quads.len() as u64,
        canonical_nquads,
        revision,
        created_at: created_at.into(),
    })
}
