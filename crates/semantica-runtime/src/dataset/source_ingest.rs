//! Original TriX and ConstructDTO ingestion under the same DatasetSession.
//! TriX state transitions port RDFLib7.6.0 trix.py (BSD-3-Clause; see
//! docs/RDFLIB-SNAPSHOT-LICENSE.md). XML events use the existing quick-xml lock.
use super::*;
use crate::rdflib_snapshot::{Literal as SourceLiteral, Node, Quad as SourceQuad};
use quick_xml::{NsReader, events::Event, name::ResolveResult};

fn literal(text: String, datatype: Option<String>, language: Option<String>) -> Result<Node> {
    let language = language.filter(|v| !v.is_empty());
    ensure!(
        datatype.is_none() || language.is_none(),
        "source_literal_has_type_and_language"
    );
    if let Some(language) = &language {
        let mut parts = language.split('-');
        let head = parts.next().unwrap_or("");
        ensure!(
            !head.is_empty()
                && head.bytes().all(|c| c.is_ascii_alphabetic())
                && parts.all(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_alphanumeric())),
            "source_literal_invalid_language"
        );
    }
    let mut value = SourceLiteral {
        text,
        datatype,
        language,
    };
    crate::rdflib_snapshot::normalize(&mut value)?;
    Ok(Node::Literal(value))
}

impl DatasetSession {
    /// An explicitly source-shaped row has either actual native construction or
    /// a checked empty native binding. No surrogate or second live store.
    fn commit_source_rows(
        &mut self,
        rows: Vec<SourceQuad>,
        source_sha: Option<String>,
        staged_graph: bool,
    ) -> Result<semantica_contracts::SemanticMutationResult> {
        ensure!(
            self.source_cache_enabled.is_some(),
            "source_session_not_admitted"
        );
        let mut projection = self.rdflib_projection.clone();
        if rows.iter().any(|q|q.3.is_some()) {
            self.source_namespaces.initialize(true,self.dataset_byte_limit(),self.resource_profile.quads())?;
        }
        projection
            .source_state
            .as_mut()
            .context("source_persistent_state_unavailable")?
            .load_parsed(
                rows.iter().map(source_query::quad_input),
                staged_graph,
                self.resource_profile.quads(),
                self.dataset_byte_limit(),
            )?;
        let retained = projection
            .quads
            .as_mut()
            .context("rdflib_lossless_source_projection_unavailable")?;
        let before = retained.len();
        for mut source in rows {
            source.3 = source
                .3
                .filter(|g| *g != Node::Iri("urn:x-rdflib:default".into()));
            let native = crate::rdflib_snapshot::core_quad(&source, None).ok();
            retained.insert(source.clone());
            let core = projection.core.entry(source).or_default();
            if let Some(native) = native {
                if !core.contains(&native) {
                    core.push(native);
                }
            }
            ensure!(
                retained.len() <= self.resource_profile.quads(),
                "dataset_load_quad_budget"
            );
        }
        let after = retained.len();
        // Exact retained source N3 bytes, without canonicalization or a raw
        // emission-count limit. Duplicate occurrences consume no extra state.
        let mut bytes = 0usize;
        for (s, p, o, g) in retained.iter() {
            for term in [Some(s), Some(p), Some(o), g.as_ref()]
                .into_iter()
                .flatten()
            {
                bytes = bytes
                    .checked_add(term.n3().len() + 1)
                    .context("source_dataset_byte_budget")?;
            }
            bytes = bytes.checked_add(2).context("source_dataset_byte_budget")?;
            ensure!(
                bytes <= self.dataset_byte_limit(),
                "source_dataset_byte_budget"
            );
        }
        if let Some(hash) = source_sha {
            if !projection.source_sha256.contains(&hash) {
                ensure!(
                    projection.source_sha256.len() < 1024,
                    "source_document_history_budget"
                );
                projection.source_sha256.push(hash);
            }
        }
        let native = projection.native_projection()?;
        let mutation = self.publish_source_state("load", native, projection)?;
        Ok(semantica_contracts::SemanticMutationResult {
            operation: "load".into(),
            added: (after - before) as u64,
            removed: 0,
            quad_count: after as u64,
            revision: mutation.revision,
        })
    }
    pub(crate) fn load_trix_source_internal(
        &mut self,
        bytes: &[u8],
        _base: Option<&str>,
        graph: Option<&str>,
    ) -> Result<semantica_contracts::SemanticMutationResult> {
        ensure!(
            self.source_cache_enabled.is_some(),
            "source_session_not_admitted"
        );
        ensure!(
            bytes.len() <= self.dataset_byte_limit(),
            "dataset_load_byte_budget"
        );
        ensure!(graph.is_none(), "dataset_graph_override_for_dataset_syntax");
        let rows = trix(bytes)?;
        self.commit_source_rows(rows, Some(crate::sha256(bytes)), true)
    }
    pub(crate) fn load_construct_source_internal(
        &mut self,
        value: &Value,
        format: Option<&str>,
        graph: Option<&str>,
    ) -> Result<semantica_contracts::SemanticMutationResult> {
        ensure!(
            self.source_cache_enabled.is_some(),
            "source_session_not_admitted"
        );
        let selected = graph
            .map(NamedNode::new)
            .transpose()?
            .map(|v| Node::Iri(v.as_str().into()));
        ensure!(
            format.is_none(),
            "format is not accepted when loading a ConstructResultDTO"
        );
        let triples = value
            .get("triples")
            .and_then(Value::as_array)
            .context("source_construct_triples_required")?;
        // Convert every DTO before committing, as the original wrapper does.
        let mut rows = Vec::with_capacity(triples.len());
        for row in triples {
            let subject = dto(&row["subject"])?;
            let predicate = dto(&row["predicate"])?;
            let object = dto(&row["object"])?;
            ensure!(
                !matches!(subject, Node::Literal(_)),
                "an RDF triple subject cannot be a literal"
            );
            ensure!(
                matches!(predicate, Node::Iri(_)),
                "an RDF triple predicate must be an IRI"
            );
            rows.push((subject, predicate, object, selected.clone()));
        }
        if graph.is_some() {
            self.source_namespaces.initialize(true,self.dataset_byte_limit(),self.resource_profile.quads())?;
        }
        self.commit_source_rows(rows, None, false)
    }
}
fn dto(value: &Value) -> Result<Node> {
    let object = value
        .as_object()
        .context("source_rdf_term_object_required")?;
    ensure!(
        object
            .keys()
            .all(|k| ["value", "term_type", "datatype", "language"].contains(&k.as_str())),
        "source_rdf_term_unknown_field"
    );
    let text = value["value"]
        .as_str()
        .context("source_rdf_term_text_required")?;
    let optional = |key: &str| -> Result<Option<String>> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(v)) => Ok(Some(v.clone())),
            _ => bail!("source_rdf_term_metadata_invalid"),
        }
    };
    let dtype = optional("datatype")?;
    let language = optional("language")?;
    let kind = value["term_type"]
        .as_str()
        .context("source_rdf_term_kind_required")?;
    if kind != "literal" {
        ensure!(
            dtype.as_ref().is_none_or(String::is_empty)
                && language.as_ref().is_none_or(String::is_empty),
            "only literal terms may have datatype or language"
        );
    }
    Ok(match kind {
        "iri" => Node::Iri(text.into()),
        "blank_node" => Node::Blank(text.into()),
        "literal" => literal(text.into(), dtype, language)?,
        _ => bail!("source_rdf_term_kind_invalid"),
    })
}

const TRIX: &[u8] = b"http://www.w3.org/2004/03/trix/trix-1/";
const XML: &[u8] = b"http://www.w3.org/XML/1998/namespace";
struct Trix {
    state: u8,
    graph: Option<Node>,
    triple: Vec<Node>,
    chars: String,
    language: Option<String>,
    datatype: Option<String>,
    rows: Vec<SourceQuad>,
}
impl Trix {
    fn start(
        &mut self,
        name: &[u8],
        language: Option<String>,
        datatype: Option<String>,
    ) -> Result<()> {
        match name {
            v if v.eq_ignore_ascii_case(b"trix") => {
                ensure!(self.state == 0, "source_trix_unexpected_root");
                self.state = 1;
            }
            b"graph" => {
                ensure!(self.state == 1, "source_trix_unexpected_graph");
                self.state = 2;
            }
            b"uri" | b"id" => {
                ensure!(matches!(self.state, 2 | 4), "source_trix_unexpected_node");
                if self.state == 2 {
                    self.state = 3;
                }
            }
            b"triple" => {
                ensure!(self.state == 2, "source_trix_unexpected_triple");
                if self.graph.is_none() {
                    self.graph = Some(Node::Blank(BlankNode::default().as_str().into()));
                }
                self.triple.clear();
                self.state = 4;
            }
            b"plainLiteral" | b"typedLiteral" => {
                ensure!(self.state == 4, "source_trix_unexpected_literal");
                self.language = language;
                self.datatype = if name == b"typedLiteral" {
                    Some(datatype.context("source_trix_missing_datatype")?)
                } else {
                    None
                };
            }
            _ => bail!("source_trix_unknown_element"),
        }
        self.chars.clear();
        Ok(())
    }
    fn end(&mut self, name: &[u8]) -> Result<()> {
        match name {
            b"uri" | b"id" => {
                let text = semantica_contracts::json::python_strip(&self.chars).to_owned();
                let node = if name == b"uri" {
                    Node::Iri(text)
                } else {
                    Node::Blank(text)
                };
                if self.state == 3 {
                    // RDFLib Graph's constructor gives false-valued identifiers
                    // an anonymous identifier, including the empty URI.
                    self.graph = Some(
                        if matches!(&node,Node::Iri(v)|Node::Blank(v) if v.is_empty()) {
                            Node::Blank(BlankNode::default().as_str().into())
                        } else {
                            node
                        },
                    );
                    self.state = 2;
                } else {
                    ensure!(self.state == 4, "source_trix_node_state");
                    self.triple.push(node);
                }
            }
            b"plainLiteral" | b"typedLiteral" => {
                ensure!(self.state == 4, "source_trix_literal_state");
                self.triple.push(literal(
                    self.chars.clone(),
                    self.datatype.clone(),
                    self.language.clone(),
                )?);
            }
            b"triple" => {
                ensure!(
                    self.state == 4 && self.triple.len() == 3,
                    "source_trix_triple_length"
                );
                self.rows.push((
                    self.triple[0].clone(),
                    self.triple[1].clone(),
                    self.triple[2].clone(),
                    self.graph.clone(),
                ));
                self.state = 2;
            }
            b"graph" => {
                self.graph = None;
                self.state = 1;
            }
            v if v.eq_ignore_ascii_case(b"trix") => self.state = 0,
            _ => bail!("source_trix_unknown_close"),
        }
        Ok(())
    }
}
fn entity<'a>(name: &str, entities: &'a BTreeMap<String, String>) -> Option<&'a str> {
    quick_xml::escape::resolve_xml_entity(name).or_else(|| entities.get(name).map(String::as_str))
}
fn doctype(text: &str, entities: &mut BTreeMap<String, String>) -> Result<()> {
    // Only literal entity declarations are resolved. External entities have the
    // original SAX default empty expansion; never open files or network URLs.
    let mut rest = text;
    while let Some((_, tail)) = rest.split_once("<!ENTITY") {
        let tail = tail.trim_start();
        ensure!(
            !tail.starts_with('%'),
            "source_trix_parameter_entity_unreceived"
        );
        let (name, value) = tail
            .split_once(char::is_whitespace)
            .context("source_trix_entity_declaration")?;
        let value = value.trim_start();
        if value.starts_with("SYSTEM") || value.starts_with("PUBLIC") {
            let (_, after) = value
                .split_once('>')
                .context("source_trix_entity_declaration")?;
            entities.entry(name.into()).or_default();
            rest = after;
        } else {
            let quote = value
                .chars()
                .next()
                .filter(|c| matches!(c, '\'' | '"'))
                .context("source_trix_entity_quote")?;
            let (value, after) = value[1..]
                .split_once(quote)
                .context("source_trix_entity_quote")?;
            let expanded =
                quick_xml::escape::unescape_with(value, |e| entity(e, entities))?.into_owned();
            ensure!(
                expanded.len() <= crate::MAX_BYTES && entities.len() < 1024,
                "source_trix_entity_budget"
            );
            entities.entry(name.into()).or_insert(expanded);
            rest = after
                .trim_start()
                .strip_prefix('>')
                .context("source_trix_entity_declaration")?;
        }
    }
    Ok(())
}
fn trix(bytes: &[u8]) -> Result<Vec<SourceQuad>> {
    let text = std::str::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut reader = NsReader::from_reader(text.as_bytes());
    reader.config_mut().expand_empty_elements = true;
    let mut state = Trix {
        state: 0,
        graph: None,
        triple: vec![],
        chars: String::new(),
        language: None,
        datatype: None,
        rows: vec![],
    };
    let mut entities = BTreeMap::new();
    let mut depth = 0usize;
    let mut seen_root = false;
    let mut finished = false;
    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                let (namespace, name) = reader.resolve_element(start.name());
                ensure!(
                    matches!(namespace,ResolveResult::Bound(ns) if ns.as_ref()==TRIX),
                    "source_trix_namespace"
                );
                let name = name.as_ref().to_vec();
                ensure!(!finished, "source_trix_after_root");
                if depth == 0 {
                    ensure!(!seen_root, "source_trix_multiple_roots");
                    seen_root = true;
                }
                depth += 1;
                ensure!(depth <= 256, "source_trix_depth_budget");
                let mut language = None;
                let mut datatype = None;
                for attribute in start.attributes() {
                    let a = attribute?;
                    let value = a
                        .decode_and_unescape_value_with(reader.decoder(), |e| entity(e, &entities))?
                        .into_owned();
                    let (ns, local) = reader.resolve_attribute(a.key);
                    if matches!(ns,ResolveResult::Bound(ns) if ns.as_ref()==XML)
                        && local.as_ref() == b"lang"
                    {
                        language = Some(value.clone());
                    }
                    if a.key.as_ref() == b"datatype" {
                        datatype = Some(value);
                    }
                }
                state.start(&name, language, datatype)?;
            }
            Event::Empty(_) => bail!("source_trix_unexpanded_empty_event"),
            Event::End(end) => {
                let (namespace, name) = reader.resolve_element(end.name());
                ensure!(
                    matches!(namespace,ResolveResult::Bound(ns) if ns.as_ref()==TRIX),
                    "source_trix_namespace"
                );
                ensure!(depth > 0, "source_trix_unexpected_close");
                state.end(name.as_ref())?;
                depth -= 1;
                if depth == 0 {
                    finished = true;
                }
            }
            Event::Text(text) => {
                let value = text.unescape_with(|e| entity(e, &entities))?;
                if depth == 0 {
                    ensure!(
                        value.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')),
                        "source_trix_text_outside_root"
                    );
                }
                state.chars.push_str(&value);
                ensure!(
                    state.chars.len() <= crate::MAX_BYTES,
                    "source_trix_text_budget"
                );
            }
            Event::CData(data) => {
                state.chars.push_str(&data.decode()?);
                ensure!(
                    state.chars.len() <= crate::MAX_BYTES,
                    "source_trix_text_budget"
                );
            }
            Event::DocType(d) => {
                ensure!(!seen_root, "source_trix_late_doctype");
                doctype(&reader.decoder().decode(d.as_ref())?, &mut entities)?;
            }
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) => (),
            Event::Eof => {
                ensure!(
                    seen_root && finished && depth == 0 && state.state == 0,
                    "source_trix_incomplete_document"
                );
                break;
            }
        }
    }
    Ok(state.rows)
}
