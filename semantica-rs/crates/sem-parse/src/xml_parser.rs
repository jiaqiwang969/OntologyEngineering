//! XML document parsing (`semantica/parse/xml_parser.py`).
//!
//! [`XMLParser`] parses XML (a string, or a file path) into an [`XMLData`] tree of
//! [`XMLElement`]s, finds elements with XPath, extracts elements by tag and converts the
//! tree to a dictionary.
//!
//! The original uses lxml (`engine="lxml"`, the default) or `xml.etree.ElementTree`
//! (`engine="etree"`); [`reader`] reproduces both trees and their error messages:
//!
//! - with lxml, comments and processing instructions inside the root element are children,
//!   and converting them fails (`TypeError` text in a `ProcessingError`), as in the
//!   original; namespaces are the root's `nsmap` (the default namespace has the key `None`);
//! - with ElementTree they are dropped, and because the original unpacks `iterparse`'s
//!   `(event, (prefix, uri))` pairs as `(prefix, uri)`, the namespaces are
//!   `{"start-ns": [prefix, uri]}` for the last declaration in the document.

pub(crate) mod reader;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::pycompat::fmt::strip as py_strip;
use sem_utils::{ProcessingError, SemanticaError, ValidationError};

use crate::common::{
    DecodeErrors, Options, PResult, Source, opt_string, opt_truthy, py_exists, py_name, py_path_str, py_read_text,
};
use crate::xpath::{XNode, XValue};

use reader::{Document, Engine, NodeId, NodeKind};

/// The Python type name lxml uses for comment/PI tags (`etree.Comment`/`etree.PI` are Cython
/// functions), which appears in the original's `TypeError` messages.
const CYTHON_FUNCTION: &str = "_cython_3_2_9.cython_function_or_method";

/// `XMLElement` dataclass.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct XMLElement {
    pub tag: String,
    pub text: String,
    #[serde(default)]
    pub attributes: IndexMap<String, String>,
    #[serde(default)]
    pub children: Vec<XMLElement>,
    #[serde(default)]
    pub namespace: Option<String>,
}

impl XMLElement {
    /// `XMLElement(tag, text)` with no attributes, children or namespace.
    pub fn new(tag: impl Into<String>, text: impl Into<String>) -> Self {
        XMLElement {
            tag: tag.into(),
            text: text.into(),
            attributes: IndexMap::new(),
            children: Vec::new(),
            namespace: None,
        }
    }
}

/// The `namespaces` dictionary of [`XMLData`]: prefix (`None` for lxml's default namespace)
/// to URI (or, for the ElementTree engine, the `[prefix, uri]` pair the original stores).
pub type Namespaces = IndexMap<Option<String>, Value>;

/// `XMLData` dataclass. It serialises as its `__dict__`
/// (`{"root": ..., "namespaces": ..., "metadata": ...}`); a `None` namespace prefix is
/// written as the key `"null"`, as `json.dumps` writes it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct XMLData {
    pub root: XMLElement,
    #[serde(default, serialize_with = "serialize_namespaces", deserialize_with = "deserialize_namespaces")]
    pub namespaces: Namespaces,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

fn serialize_namespaces<S: Serializer>(ns: &Namespaces, s: S) -> Result<S::Ok, S::Error> {
    let mut map = s.serialize_map(Some(ns.len()))?;
    for (k, v) in ns {
        map.serialize_entry(k.as_deref().unwrap_or("null"), v)?;
    }
    map.end()
}

fn deserialize_namespaces<'de, D: Deserializer<'de>>(d: D) -> Result<Namespaces, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Namespaces;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a namespace map")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Namespaces, A::Error> {
            let mut out = Namespaces::new();
            while let Some((k, v)) = access.next_entry::<String, Value>()? {
                out.insert(if k == "null" { None } else { Some(k) }, v);
            }
            Ok(out)
        }
    }
    d.deserialize_map(V)
}

impl XMLData {
    /// `XMLData(root)` with empty namespaces and metadata.
    pub fn new(root: XMLElement) -> Self {
        XMLData {
            root,
            namespaces: Namespaces::new(),
            metadata: Map::new(),
        }
    }
}

/// `TypeError: argument of type '...' is not iterable` (the `"}" in tag` test on a
/// comment/PI tag function).
fn not_iterable(type_name: &str) -> SemanticaError {
    SemanticaError::Type(format!("argument of type '{type_name}' is not iterable"))
}

/// An exception of the `lxml.etree` module.
fn lxml_error(type_name: &str, message: impl Into<String>) -> SemanticaError {
    SemanticaError::Other {
        module: "lxml.etree".to_string(),
        type_name: type_name.to_string(),
        message: message.into(),
    }
}

/// `_element_to_xml_element` / `_etree_element_to_xml_element`: lxml iterates comments and
/// processing instructions as children and fails on them.
fn element_to_xml_element(doc: &Document, id: NodeId) -> PResult<XMLElement> {
    let NodeKind::Element {
        local,
        namespace,
        attrs,
        ..
    } = &doc.nodes[id].kind
    else {
        return Err(not_iterable(CYTHON_FUNCTION));
    };
    let mut element = XMLElement {
        tag: local.clone(),
        text: py_strip(&doc.element_text(id).unwrap_or_default()).to_string(),
        attributes: attrs.iter().map(|a| (a.key(), a.value.clone())).collect(),
        children: Vec::new(),
        namespace: namespace.clone(),
    };
    for child in doc.child_items(id) {
        element.children.push(element_to_xml_element(doc, child)?);
    }
    Ok(element)
}

/// `XMLParser(**config)`: XML document parser.
#[derive(Debug, Clone)]
pub struct XMLParser {
    /// `self.logger` (`semantica.xml_parser`).
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
}

impl Default for XMLParser {
    fn default() -> Self {
        XMLParser::new(Options::new())
    }
}

impl XMLParser {
    /// `XMLParser(**config)`.
    pub fn new(config: Options) -> Self {
        XMLParser {
            logger: get_logger("xml_parser"),
            config,
            progress_tracker: get_progress_tracker(),
        }
    }

    /// `parse(file_path, **options)`: parse an XML file or string.
    ///
    /// Options: `engine` (`"lxml"` default, anything else uses ElementTree) and
    /// `extract_namespaces` (default true).
    pub fn parse(&self, file_path: impl Into<Source>, options: &Options) -> PResult<XMLData> {
        let source: Source = file_path.into();
        let file = source.file_path()?;
        let file_str = file.as_deref().map(py_path_str);
        let label = file.as_deref().map(py_name).unwrap_or_else(|| "content".to_string());
        let tracking_id = self.progress_tracker.start_tracking(
            file_str.as_deref(),
            Some("parse"),
            Some("XMLParser"),
            &format!("XML: {label}"),
            None,
        );
        let outer = (|| -> PResult<XMLData> {
            let engine = opt_string(options, "engine", "lxml");
            let (xml_string, source_label) = match &file {
                Some(path) => {
                    if !py_exists(path)? {
                        return Err(ValidationError::new(format!("XML file not found: {}", py_path_str(path))).into());
                    }
                    (py_read_text(path, "utf-8", DecodeErrors::Ignore)?, py_path_str(path))
                }
                None => (source.text().unwrap_or_default().to_string(), "string".to_string()),
            };
            let result = if engine == "lxml" {
                parse_with_lxml(&xml_string, &source_label, options)
            } else {
                parse_with_etree(&xml_string, &source_label, options)
            };
            match result {
                Ok(data) => {
                    self.progress_tracker
                        .stop_tracking(&tracking_id, Some("completed"), "Parsed XML successfully", None);
                    Ok(data)
                }
                Err(e) => {
                    self.progress_tracker
                        .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                    self.logger.error(format!("Failed to parse XML: {e}"));
                    Err(ProcessingError::new(format!("Failed to parse XML: {e}")).into())
                }
            }
        })();
        outer.inspect_err(|e| {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        })
    }

    /// `find_elements(file_path, xpath, **options)`: the elements an XPath expression selects
    /// (evaluated with lxml semantics on the root element, with the parsed namespaces).
    ///
    /// As in the original, non-element results fail when converted: text and attribute
    /// results raise `AttributeError`, numbers and booleans `TypeError`, and elements
    /// containing comments or processing instructions `TypeError`.
    pub fn find_elements(&self, file_path: impl Into<Source>, xpath: &str, options: &Options) -> PResult<Vec<XMLElement>> {
        let source: Source = file_path.into();
        let xml_data = self.parse(source.clone(), options)?;
        let xml_string = match &source {
            Source::Text(s) if !py_exists(Path::new(s))? => s.clone(),
            other => {
                let path = match other {
                    Source::Text(s) => std::path::PathBuf::from(s),
                    Source::Path(p) => p.clone(),
                };
                py_read_text(&path, "utf-8", DecodeErrors::Strict)?
            }
        };
        let doc = reader::parse(&xml_string, Engine::Lxml, false).map_err(|m| lxml_error("XMLSyntaxError", m))?;
        // `root.xpath(xpath, namespaces=namespaces)`: lxml validates the namespace map first.
        let mut namespaces = HashMap::new();
        for (prefix, uri) in &xml_data.namespaces {
            let Some(prefix) = prefix else {
                return Err(SemanticaError::Type("empty namespace prefix is not supported in XPath".to_string()));
            };
            let uri = match uri {
                Value::String(s) => s.clone(),
                other => {
                    return Err(SemanticaError::Type(format!(
                        "Argument must be bytes or unicode, got '{}'",
                        python_type_of(other)
                    )));
                }
            };
            namespaces.insert(prefix.clone(), uri);
        }
        let value = crate::xpath::evaluate(&doc, doc.root, xpath, &namespaces)
            .map_err(|e| lxml_error("XPathEvalError", e.0))?;
        match value {
            XValue::NodeSet(nodes) => {
                let mut out = Vec::new();
                for node in nodes {
                    match node {
                        XNode::Node(id) => match &doc.nodes[id].kind {
                            NodeKind::Document => {}
                            NodeKind::Element { .. } => out.push(element_to_xml_element(&doc, id)?),
                            NodeKind::Text(_) => {
                                return Err(SemanticaError::Attribute(
                                    "'lxml.etree._ElementUnicodeResult' object has no attribute 'tag'".to_string(),
                                ));
                            }
                            NodeKind::Comment(_) | NodeKind::Pi { .. } => return Err(not_iterable(CYTHON_FUNCTION)),
                        },
                        XNode::Attr(..) => {
                            return Err(SemanticaError::Attribute(
                                "'lxml.etree._ElementUnicodeResult' object has no attribute 'tag'".to_string(),
                            ));
                        }
                    }
                }
                Ok(out)
            }
            XValue::Number(_) => Err(SemanticaError::Type("'float' object is not iterable".to_string())),
            XValue::Bool(_) => Err(SemanticaError::Type("'bool' object is not iterable".to_string())),
            XValue::Str(s) if s.is_empty() => Ok(Vec::new()),
            XValue::Str(_) => Err(SemanticaError::Attribute("'str' object has no attribute 'tag'".to_string())),
        }
    }

    /// `extract_by_tag(file_path, tag_name, **options)`: every element (depth first, root
    /// included) whose local tag name is `tag_name`.
    pub fn extract_by_tag(&self, file_path: impl Into<Source>, tag_name: &str, options: &Options) -> PResult<Vec<XMLElement>> {
        let xml_data = self.parse(file_path, options)?;
        let mut results = Vec::new();
        fn find_by_tag(element: &XMLElement, tag: &str, results: &mut Vec<XMLElement>) {
            if element.tag == tag {
                results.push(element.clone());
            }
            for child in &element.children {
                find_by_tag(child, tag, results);
            }
        }
        find_by_tag(&xml_data.root, tag_name, &mut results);
        Ok(results)
    }

    /// `to_dict(file_path, **options)`: the tree as `{"tag", "text", "attributes",
    /// "children": {tag: [...]}}` dictionaries.
    pub fn to_dict(&self, file_path: impl Into<Source>, options: &Options) -> PResult<Map<String, Value>> {
        let xml_data = self.parse(file_path, options)?;
        Ok(element_to_dict(&xml_data.root))
    }
}

fn python_type_of(v: &Value) -> &'static str {
    match v {
        Value::Array(_) => "tuple",
        other => sem_utils::pycompat::fmt::type_name(other),
    }
}

/// `_element_to_dict(element)`.
pub(crate) fn element_to_dict(element: &XMLElement) -> Map<String, Value> {
    let mut result = Map::new();
    result.insert("tag".to_string(), Value::String(element.tag.clone()));
    result.insert("text".to_string(), Value::String(element.text.clone()));
    result.insert("attributes".to_string(), json!(element.attributes));
    if !element.children.is_empty() {
        let mut children: Map<String, Value> = Map::new();
        for child in &element.children {
            let entry = children.entry(child.tag.clone()).or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(items) = entry {
                items.push(Value::Object(element_to_dict(child)));
            }
        }
        result.insert("children".to_string(), Value::Object(children));
    }
    result
}

/// `_parse_with_lxml(xml_string, source, options)`.
fn parse_with_lxml(xml_string: &str, source: &str, options: &Options) -> PResult<XMLData> {
    let doc = reader::parse(xml_string, Engine::Lxml, true).map_err(|m| lxml_error("XMLSyntaxError", m))?;
    let mut namespaces = Namespaces::new();
    if opt_truthy(options, "extract_namespaces", true)
        && let NodeKind::Element { nsdecls, .. } = &doc.nodes[doc.root].kind
    {
        for (prefix, uri) in nsdecls {
            namespaces.entry(prefix.clone()).or_insert_with(|| Value::String(uri.clone()));
        }
    }
    let root = element_to_xml_element(&doc, doc.root)?;
    let mut metadata = Map::new();
    metadata.insert("source".to_string(), Value::String(source.to_string()));
    metadata.insert("engine".to_string(), Value::String("lxml".to_string()));
    Ok(XMLData {
        root,
        namespaces,
        metadata,
    })
}

/// `_parse_with_etree(xml_string, source, options)`.
fn parse_with_etree(xml_string: &str, source: &str, options: &Options) -> PResult<XMLData> {
    let doc = reader::parse(xml_string, Engine::Expat, false).map_err(|m| SemanticaError::Other {
        module: "xml.etree.ElementTree".to_string(),
        type_name: "ParseError".to_string(),
        message: m,
    })?;
    let mut namespaces = Namespaces::new();
    if opt_truthy(options, "extract_namespaces", true) {
        // `for prefix, uri in ET.iterparse(..., events=["start-ns"])` unpacks
        // `("start-ns", (prefix, uri))`: the key is always "start-ns" and the value the last
        // declaration's `(prefix or "", uri or "")` pair.
        let mut last: Option<(String, String)> = None;
        for node in &doc.nodes {
            if let NodeKind::Element { nsdecls, .. } = &node.kind {
                for (prefix, uri) in nsdecls {
                    last = Some((prefix.clone().unwrap_or_default(), uri.clone()));
                }
            }
        }
        if let Some((prefix, uri)) = last {
            namespaces.insert(Some("start-ns".to_string()), json!([prefix, uri]));
        }
    }
    let root = element_to_xml_element(&doc, doc.root)?;
    let mut metadata = Map::new();
    metadata.insert("source".to_string(), Value::String(source.to_string()));
    metadata.insert("engine".to_string(), Value::String("etree".to_string()));
    Ok(XMLData {
        root,
        namespaces,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_follow_each_engine() {
        let lxml = |s: &str| reader::parse(s, Engine::Lxml, true).err().unwrap_or_default();
        let expat = |s: &str| reader::parse(s, Engine::Expat, false).err().unwrap_or_default();
        assert_eq!(
            lxml("<a><b></a>"),
            "Opening and ending tag mismatch: b line 1 and a, line 1, column 11 (<string>, line 1)"
        );
        assert_eq!(expat("<a><b></a>"), "mismatched tag: line 1, column 8");
        assert_eq!(lxml("not xml"), "Start tag expected, '<' not found, line 1, column 1 (<string>, line 1)");
        assert_eq!(expat("not xml"), "syntax error: line 1, column 0");
        assert_eq!(lxml("<a>é</b>"), "Opening and ending tag mismatch: a line 1 and b, line 1, column 9 (<string>, line 1)");
        assert_eq!(expat("<a>é</b>"), "mismatched tag: line 1, column 6");
    }
}
