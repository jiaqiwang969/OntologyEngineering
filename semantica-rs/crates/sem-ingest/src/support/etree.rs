//! `xml.etree.ElementTree` for the operations the ingestors use: `fromstring`, `Element.tag`
//! (Clark notation), `.attrib`, `.text`, `.tail`, `.get()`, `.iter()`, and `find`/`findall`
//! with the ElementPath subset (`tag`, `{ns}tag`, `prefix:tag` + namespace map, `.//tag`,
//! `*`, `[@attr]`, `[@attr='value']`).

use indexmap::IndexMap;
use sem_utils::SemanticaError;

use super::xmldom::{self, ParseOptions, XmlElement, XmlError, XmlNode};

/// An `Element`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    /// `element.tag`.
    pub tag: String,
    /// `element.attrib` (Clark-notation keys).
    pub attrib: IndexMap<String, String>,
    /// `element.text`.
    pub text: Option<String>,
    /// `element.tail`.
    pub tail: Option<String>,
    /// Child elements.
    pub children: Vec<Element>,
}

/// `xml.etree.ElementTree.ParseError`.
pub fn parse_error(error: &XmlError) -> SemanticaError {
    SemanticaError::Other {
        module: "xml.etree.ElementTree".to_string(),
        type_name: "ParseError".to_string(),
        message: error.expat_message(),
    }
}

/// `isinstance(e, xml.etree.ElementTree.ParseError)`.
pub fn is_parse_error(error: &SemanticaError) -> bool {
    matches!(error, SemanticaError::Other { module, type_name, .. }
        if module == "xml.etree.ElementTree" && type_name == "ParseError")
}

fn convert(element: &XmlElement) -> Element {
    let mut out = Element {
        tag: element.clark_name(),
        attrib: element
            .attributes
            .iter()
            .map(|a| (a.clark_name(), a.value.clone()))
            .collect(),
        ..Element::default()
    };
    for child in &element.children {
        match child {
            XmlNode::Text(t) => match out.children.last_mut() {
                Some(last) => last.tail.get_or_insert_with(String::new).push_str(t),
                None => out.text.get_or_insert_with(String::new).push_str(t),
            },
            XmlNode::Element(e) => out.children.push(convert(e)),
            XmlNode::Comment(_) | XmlNode::Pi(..) => {}
        }
    }
    out
}

/// `ET.fromstring(text)` from bytes (encoding taken from a BOM or the XML declaration).
pub fn fromstring_bytes(bytes: &[u8]) -> Result<Element, SemanticaError> {
    xmldom::parse_bytes(bytes, &ParseOptions::default())
        .map(|d| convert(&d.root))
        .map_err(|e| parse_error(&e))
}

/// `ET.fromstring(text)` from a string.
pub fn fromstring(text: &str) -> Result<Element, SemanticaError> {
    xmldom::parse_str(text, &ParseOptions::default())
        .map(|d| convert(&d.root))
        .map_err(|e| parse_error(&e))
}

/// `defusedxml.ElementTree.fromstring(text)`: entity declarations are rejected with
/// `EntitiesForbidden` (returned as the error).
pub fn defused_fromstring(text: &str) -> Result<Element, XmlError> {
    let options = ParseOptions {
        forbid_entities: true,
        ..ParseOptions::default()
    };
    xmldom::parse_str(text, &options).map(|d| convert(&d.root))
}

/// The lxml fallback of the public API ingestor (`resolve_entities=False`, `load_dtd=False`,
/// no network): internal entities are left unexpanded (their references vanish), unknown
/// ones are errors.
pub fn lxml_safe_fromstring(text: &str) -> Result<Element, XmlError> {
    let options = ParseOptions {
        expand_internal_entities: false,
        ..ParseOptions::default()
    };
    xmldom::parse_str(text, &options).map(|d| convert(&d.root))
}

/// One step of an ElementPath expression.
#[derive(Debug, Clone)]
struct Step {
    descendant: bool,
    tag: String,
    predicates: Vec<(String, Option<String>)>,
}

fn expand_tag(tag: &str, namespaces: &IndexMap<String, String>) -> Result<String, SemanticaError> {
    if tag == "*" || tag.starts_with('{') {
        return Ok(tag.to_string());
    }
    if let Some((prefix, local)) = tag.split_once(':') {
        return match namespaces.get(prefix) {
            Some(uri) => Ok(format!("{{{uri}}}{local}")),
            None => Err(SemanticaError::Other {
                module: "builtins".to_string(),
                type_name: "SyntaxError".to_string(),
                message: format!("prefix {} not found in prefix map", sem_utils::pycompat::fmt::str_repr(prefix)),
            }),
        };
    }
    if let Some(default) = namespaces.get("") {
        return Ok(format!("{{{default}}}{tag}"));
    }
    Ok(tag.to_string())
}

fn parse_path(path: &str, namespaces: &IndexMap<String, String>) -> Result<Vec<Step>, SemanticaError> {
    let mut path = path;
    let mut steps = Vec::new();
    let mut descendant = false;
    if let Some(rest) = path.strip_prefix(".//") {
        descendant = true;
        path = rest;
    } else if let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    // Split on '/' outside braces and brackets.
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for c in path.chars() {
        match c {
            '{' | '[' => {
                depth += 1;
                current.push(c);
            }
            '}' | ']' => {
                depth -= 1;
                current.push(c);
            }
            '/' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    parts.push(current);
    for part in parts {
        if part.is_empty() {
            // "a//b": next step searches descendants.
            descendant = true;
            continue;
        }
        let (tag_part, preds) = match part.find('[') {
            Some(i) if !part.starts_with('{') || part.find('}').is_some_and(|j| j < i) => {
                (part[..i].to_string(), part[i..].to_string())
            }
            _ => (part.clone(), String::new()),
        };
        let mut predicates = Vec::new();
        let mut rest = preds.as_str();
        while let Some(start) = rest.find('[') {
            let Some(end) = rest[start..].find(']') else { break };
            let body = &rest[start + 1..start + end];
            if let Some(attr) = body.strip_prefix('@') {
                match attr.split_once('=') {
                    Some((name, value)) => {
                        let value = value.trim_matches(|c| c == '\'' || c == '"').to_string();
                        predicates.push((expand_attr(name, namespaces), Some(value)));
                    }
                    None => predicates.push((expand_attr(attr, namespaces), None)),
                }
            }
            rest = &rest[start + end + 1..];
        }
        steps.push(Step {
            descendant,
            tag: if tag_part == "." {
                ".".to_string()
            } else {
                expand_tag(&tag_part, namespaces)?
            },
            predicates,
        });
        descendant = false;
    }
    Ok(steps)
}

fn expand_attr(name: &str, namespaces: &IndexMap<String, String>) -> String {
    if let Some((prefix, local)) = name.split_once(':')
        && let Some(uri) = namespaces.get(prefix)
    {
        return format!("{{{uri}}}{local}");
    }
    name.to_string()
}

impl Element {
    /// `element.get(key)`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrib.get(key).map(String::as_str)
    }

    /// `element.iter()` (self first, document order).
    pub fn iter(&self) -> Vec<&Element> {
        let mut out = vec![self];
        for child in &self.children {
            out.extend(child.iter());
        }
        out
    }

    fn step_matches(element: &Element, step: &Step) -> bool {
        (step.tag == "*" || element.tag == step.tag)
            && step.predicates.iter().all(|(name, value)| match value {
                None => element.attrib.contains_key(name),
                Some(v) => element.attrib.get(name) == Some(v),
            })
    }

    /// `element.findall(path, namespaces)`.
    pub fn findall_ns(
        &self,
        path: &str,
        namespaces: &IndexMap<String, String>,
    ) -> Result<Vec<&Element>, SemanticaError> {
        let steps = parse_path(path, namespaces)?;
        let mut current: Vec<&Element> = vec![self];
        for step in &steps {
            if step.tag == "." {
                continue;
            }
            let mut next: Vec<&Element> = Vec::new();
            for element in &current {
                let candidates: Vec<&Element> = if step.descendant {
                    element.iter().into_iter().skip(1).collect()
                } else {
                    element.children.iter().collect()
                };
                for candidate in candidates {
                    if Self::step_matches(candidate, step)
                        && !next.iter().any(|e| std::ptr::eq(*e, candidate))
                    {
                        next.push(candidate);
                    }
                }
            }
            current = next;
        }
        Ok(current)
    }

    /// `element.findall(path)`.
    pub fn findall(&self, path: &str) -> Vec<&Element> {
        self.findall_ns(path, &IndexMap::new()).unwrap_or_default()
    }

    /// `element.find(path, namespaces)`.
    pub fn find_ns(
        &self,
        path: &str,
        namespaces: &IndexMap<String, String>,
    ) -> Result<Option<&Element>, SemanticaError> {
        Ok(self.findall_ns(path, namespaces)?.into_iter().next())
    }

    /// `element.find(path)`.
    pub fn find(&self, path: &str) -> Option<&Element> {
        self.findall(path).into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitemap_paths() {
        let xml = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>http://ex.com/page1</loc></url></urlset>"#;
        let root = fromstring(xml).unwrap();
        let mut ns = IndexMap::new();
        ns.insert(
            "sitemap".to_string(),
            "http://www.sitemaps.org/schemas/sitemap/0.9".to_string(),
        );
        let urls = root.findall_ns(".//sitemap:url", &ns).unwrap();
        assert_eq!(urls.len(), 1);
        let loc = urls[0].find_ns("sitemap:loc", &ns).unwrap().unwrap();
        assert_eq!(loc.text.as_deref(), Some("http://ex.com/page1"));
        assert!(root.findall(".//url").is_empty());
    }

    #[test]
    fn predicates_and_text() {
        let root = fromstring(
            "<feed xmlns='http://www.w3.org/2005/Atom'><link href='a' rel='self'/><link href='b'/></feed>",
        )
        .unwrap();
        let l = root
            .find("{http://www.w3.org/2005/Atom}link[@rel='self']")
            .unwrap();
        assert_eq!(l.get("href"), Some("a"));
        assert_eq!(
            fromstring("NOT XML").unwrap_err().to_string(),
            "syntax error: line 1, column 0"
        );
    }
}
