//! Actual parser source facts, carried into the existing DatasetSession owner.
use super::{DatasetSession, ParsedLoad, RdfFormat};
use crate::rdflib_snapshot::{Literal, Node, Quad as SourceQuad};
use anyhow::{Result, ensure};
use oxigraph::model::{GraphName, NamedOrBlankNode, Term};

pub(super) fn parse<'a>(
    bytes: &'a [u8],
    format: RdfFormat,
    base: Option<&str>,
) -> Result<Box<dyn Iterator<Item = Result<ParsedLoad>> + 'a>> {
    if matches!(format, RdfFormat::JsonLd { .. }) {
        let mut parser = oxjsonld::JsonLdParser::new();
        if let Some(base) = base {
            parser = parser.with_base_iri(base)?;
        }
        let emitted = parser.for_source_slice(bytes)?;
        return Ok(Box::new(emitted.filter_map(|item| {
            let emitted = match item {
                Ok(v) => v,
                Err(e) => return Some(Err(e.into())),
            };
            // The pinned source wrapper loads JSON-LD through Graph, then only
            // iterates that graph. Named contexts are not flattened by override.
            if emitted.quad.graph_name != GraphName::DefaultGraph {
                return None;
            }
            Some(jsonld_source(emitted))
        })));
    }
    ensure!(
        matches!(
            format,
            RdfFormat::Turtle | RdfFormat::TriG | RdfFormat::RdfXml
        ),
        "source_rdf_syntax_unreceived"
    );
    let emitted: Box<dyn Iterator<Item = Result<oxttl::SourceQuad>> + 'a> = if format
        == RdfFormat::RdfXml
    {
        let mut parser = oxrdfxml::RdfXmlParser::new();
        if let Some(base) = base {
            parser = parser.with_base_iri(base)?;
        }
        Box::new(parser.for_source_slice(bytes)?.map(|item| {
            let source = item?;
            Ok(oxttl::SourceQuad {
                quad: source.triple.in_graph(GraphName::DefaultGraph),
                literal: source.literal.map(|s| oxttl::SourceLiteral {
                    value: s.value,
                    datatype: s.datatype,
                    language: s.language,
                }),
            })
        }))
    } else {
        Box::new(
            oxttl::SourceRdfParser::new(bytes, format == RdfFormat::TriG, base)?.map(|v| Ok(v?)),
        )
    };
    Ok(Box::new(emitted.map(|item| {
        let emitted = item?;
        let q = emitted.quad;
        let subject = match &q.subject {
            NamedOrBlankNode::NamedNode(n) => Node::Iri(n.as_str().into()),
            NamedOrBlankNode::BlankNode(n) => Node::Blank(n.as_str().into()),
        };
        let object = match (&q.object, emitted.literal) {
            (Term::NamedNode(n), None) => Node::Iri(n.as_str().into()),
            (Term::BlankNode(n), None) => Node::Blank(n.as_str().into()),
            (Term::Literal(_), Some(s)) => Node::Literal(Literal {
                text: s.value,
                datatype: s.datatype,
                language: s.language,
            }),
            _ => anyhow::bail!("source_rdf_literal_fact_binding"),
        };
        let graph = match &q.graph_name {
            GraphName::NamedNode(n) => Some(Node::Iri(n.as_str().into())),
            GraphName::BlankNode(n) => Some(Node::Blank(n.as_str().into())),
            GraphName::DefaultGraph => None,
        };
        let raw = (
            subject,
            Node::Iri(q.predicate.as_str().into()),
            object,
            graph,
        );
        Ok(ParsedLoad {
            quad: q,
            source: Some(raw),
            normalize_source_literal: true,
        })
    })))
}

impl DatasetSession {
    /// Existing lossless mutation accounting with an explicit parser base.
    pub(crate) fn load_source_internal(
        &mut self,
        bytes: &[u8],
        format: RdfFormat,
        base: Option<&str>,
        graph: Option<&str>,
    ) -> Result<semantica_contracts::SemanticMutationResult> {
        ensure!(
            self.source_cache_enabled.is_some(),
            "source_session_not_admitted"
        );
        self.load_rdflib_with_base(bytes, format, base, graph)
    }
}

fn jsonld_source(emitted: oxjsonld::SourceQuad) -> Result<ParsedLoad> {
    use oxjsonld::SourceValue;
    let q = emitted.quad;
    let mut normalize_source_literal = true;
    let object = match (&q.object, emitted.literal) {
        (Term::NamedNode(n), None) => Node::Iri(n.as_str().into()),
        (Term::BlankNode(n), None) => Node::Blank(n.as_str().into()),
        (Term::Literal(native), Some(source)) => {
            ensure!(
                native == &source.native,
                "source_jsonld_literal_fact_binding"
            );
            let mut datatype = source.datatype;
            let mut language = source.language;
            let text = match source.value {
                SourceValue::String(value) => value,
                SourceValue::Number(raw) => {
                    normalize_source_literal = false;
                    language = None;
                    if raw.contains(['.', 'e', 'E']) {
                        datatype.get_or_insert_with(|| {
                            "http://www.w3.org/2001/XMLSchema#double".into()
                        });
                        semantica_contracts::json::python_float_repr(raw.parse::<f64>()?)?
                    } else {
                        // Match the pinned Python integer input domain; no f64
                        // rounding or synthesized confidence/precision.
                        ensure!(
                            raw.trim_start_matches('-').len() <= 4300,
                            "source_jsonld_integer_digit_budget"
                        );
                        datatype.get_or_insert_with(|| {
                            "http://www.w3.org/2001/XMLSchema#integer".into()
                        });
                        raw.parse::<num_bigint::BigInt>()?.to_string()
                    }
                }
                SourceValue::Boolean(value) => {
                    normalize_source_literal = false;
                    language = None;
                    datatype
                        .get_or_insert_with(|| "http://www.w3.org/2001/XMLSchema#boolean".into());
                    if value { "true" } else { "false" }.into()
                }
                SourceValue::Json(raw) => {
                    // Existing JSON owner specifies Python sorted compact JSON.
                    // Nonfinite embedded JSON remains an explicit unreceived domain.
                    let value: serde_json::Value = serde_json::from_str(&raw)?;
                    String::from_utf8(semantica_contracts::json::canonical_json(&value)?)?
                }
            };
            Node::Literal(Literal {
                text,
                datatype,
                language,
            })
        }
        _ => anyhow::bail!("source_jsonld_literal_fact_binding"),
    };
    let subject = match &q.subject {
        NamedOrBlankNode::NamedNode(n) => Node::Iri(n.as_str().into()),
        NamedOrBlankNode::BlankNode(n) => Node::Blank(n.as_str().into()),
    };
    let source: SourceQuad = (
        subject,
        Node::Iri(q.predicate.as_str().into()),
        object,
        None,
    );
    Ok(ParsedLoad {
        quad: q,
        source: Some(source),
        normalize_source_literal,
    })
}
