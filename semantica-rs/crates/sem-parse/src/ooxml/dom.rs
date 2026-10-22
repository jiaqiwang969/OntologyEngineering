//! A small namespace-aware XML tree over quick-xml, giving the views python-docx,
//! python-pptx and openpyxl read through lxml: element tags as namespace URI + local name,
//! attributes (namespace declarations excluded), lxml's `.text` (the text before the first
//! child node) and children in document order.
//!
//! Character data follows the XML rules lxml applies: line endings are normalised
//! (`\r\n`/`\r` become `\n`) before entity references are expanded, and literal tabs and
//! newlines in attribute values become spaces. Adjacent text and CDATA sections merge into
//! one text node, as in libxml2.

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;

/// An attribute: namespace URI (`None` for unprefixed attributes), local name, value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attr {
    pub ns: Option<String>,
    pub name: String,
    pub value: String,
}

/// A child node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Element(Element),
    Text(String),
    /// A comment or processing instruction: a child in lxml, but neither text nor element.
    Other,
}

/// An element.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Element {
    /// Namespace URI of the tag (`None` when unqualified).
    pub ns: Option<String>,
    /// Local name.
    pub name: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
}

impl Element {
    /// `True` when the tag is `{ns}name`.
    pub fn is(&self, ns: &str, name: &str) -> bool {
        self.name == name && self.ns.as_deref() == Some(ns)
    }

    /// Child elements in document order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }

    /// Child elements tagged `{ns}name`.
    pub fn children_named<'a>(
        &'a self,
        ns: &'a str,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements().filter(move |e| e.is(ns, name))
    }

    /// The first child element tagged `{ns}name`.
    pub fn child(&self, ns: &str, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.is(ns, name))
    }

    /// Follow a path of `(ns, name)` steps through first matching children.
    #[allow(dead_code)] // kept for the Docling office backends
    pub fn path(&self, steps: &[(&str, &str)]) -> Option<&Element> {
        let mut cur = self;
        for (ns, name) in steps {
            cur = cur.child(ns, name)?;
        }
        Some(cur)
    }

    /// Attribute value by namespace URI (`None` = unprefixed) and local name.
    pub fn attr(&self, ns: Option<&str>, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name == name && a.ns.as_deref() == ns)
            .map(|a| a.value.as_str())
    }

    /// lxml's `element.text`: the text before the first child node, `None` when absent.
    pub fn text(&self) -> Option<&str> {
        match self.children.first() {
            Some(Node::Text(t)) => Some(t.as_str()),
            _ => None,
        }
    }

    /// All descendant text concatenated (lxml's `itertext()` joined).
    #[allow(dead_code)] // kept for the Docling office backends
    pub fn all_text(&self) -> String {
        let mut out = String::new();
        fn walk(e: &Element, out: &mut String) {
            for c in &e.children {
                match c {
                    Node::Text(t) => out.push_str(t),
                    Node::Element(el) => walk(el, out),
                    Node::Other => {}
                }
            }
        }
        walk(self, &mut out);
        out
    }

    /// Depth-first iteration over this element and its descendants.
    #[allow(dead_code)] // kept for the Docling office backends
    pub fn descendants(&self) -> Vec<&Element> {
        let mut out = Vec::new();
        fn walk<'a>(e: &'a Element, out: &mut Vec<&'a Element>) {
            out.push(e);
            for c in e.elements() {
                walk(c, out);
            }
        }
        walk(self, &mut out);
        out
    }
}

/// Normalise XML line endings (`\r\n` and lone `\r` become `\n`).
fn normalize_newlines(raw: &str) -> String {
    if !raw.contains('\r') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

fn unescape(raw: &str) -> Result<String, String> {
    quick_xml::escape::unescape(raw)
        .map(|c| c.into_owned())
        .map_err(|e| e.to_string())
}

fn decode_document(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let (text, _, had_errors) = if bytes.starts_with(&[0xff, 0xfe]) {
            encoding_rs::UTF_16LE.decode(&bytes[2..])
        } else {
            encoding_rs::UTF_16BE.decode(&bytes[2..])
        };
        if had_errors {
            return Err("Document labelled UTF-16 but has UTF-8 content".to_string());
        }
        // The declaration still names UTF-16; quick-xml reads the decoded text as-is.
        return Ok(text.into_owned());
    }
    let body = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    match std::str::from_utf8(body) {
        Ok(s) => Ok(s.to_string()),
        Err(e) => Err(format!(
            "Input is not proper UTF-8, indicate encoding !\nBytes: 0x{:02X}, line 1, column {}",
            body[e.valid_up_to()],
            e.valid_up_to() + 1
        )),
    }
}

fn element_from_start(
    reader: &NsReader<&[u8]>,
    start: &BytesStart<'_>,
) -> Result<Element, String> {
    let (ns, local) = reader.resolve_element(start.name());
    let ns = match ns {
        ResolveResult::Bound(n) => Some(String::from_utf8_lossy(n.as_ref()).into_owned()),
        ResolveResult::Unbound => None,
        ResolveResult::Unknown(p) => {
            return Err(format!(
                "Namespace prefix {} on {} is not defined",
                String::from_utf8_lossy(&p),
                String::from_utf8_lossy(local.as_ref())
            ));
        }
    };
    let mut attrs = Vec::new();
    for attr in start.attributes().with_checks(false) {
        let attr = attr.map_err(|e| e.to_string())?;
        let key = attr.key;
        if key.as_ref() == b"xmlns" || key.as_ref().starts_with(b"xmlns:") {
            continue;
        }
        let (ans, alocal) = reader.resolve_attribute(key);
        let ans = match ans {
            ResolveResult::Bound(n) => Some(String::from_utf8_lossy(n.as_ref()).into_owned()),
            ResolveResult::Unbound => None,
            ResolveResult::Unknown(_) => None,
        };
        let raw = std::str::from_utf8(&attr.value).map_err(|e| e.to_string())?;
        // Attribute-value normalisation: literal whitespace characters become spaces.
        let normalized: String = normalize_newlines(raw)
            .chars()
            .map(|c| if c == '\t' || c == '\n' { ' ' } else { c })
            .collect();
        attrs.push(Attr {
            ns: ans,
            name: String::from_utf8_lossy(alocal.as_ref()).into_owned(),
            value: unescape(&normalized)?,
        });
    }
    Ok(Element {
        ns,
        name: String::from_utf8_lossy(local.as_ref()).into_owned(),
        attrs,
        children: Vec::new(),
    })
}

fn push_text(parent: &mut Element, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Node::Text(prev)) = parent.children.last_mut() {
        prev.push_str(text);
    } else {
        parent.children.push(Node::Text(text.to_string()));
    }
}

/// Parse an XML document and return its root element. Errors carry lxml's message
/// (python-docx/python-pptx/openpyxl parse parts with lxml): when the document is not
/// well-formed, the lxml-compatible reader of `xml_parser` supplies libxml2's wording.
pub(crate) fn parse(bytes: &[u8]) -> Result<Element, String> {
    parse_quick(bytes).map_err(|message| {
        let Ok(text) = decode_document(bytes) else {
            return message;
        };
        match crate::xml_parser::reader::parse(&text, crate::xml_parser::reader::Engine::Lxml, true) {
            Err(lxml_message) => lxml_message,
            Ok(_) => message,
        }
    })
}

fn parse_quick(bytes: &[u8]) -> Result<Element, String> {
    let text = decode_document(bytes)?;
    if text.trim().is_empty() {
        return Err("Document is empty, line 1, column 1".to_string());
    }
    let mut reader = NsReader::from_str(&text);
    reader.config_mut().check_end_names = true;
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            Event::Start(start) => {
                let el = element_from_start(&reader, &start)?;
                if root.is_some() && stack.is_empty() {
                    return Err("Extra content at the end of the document".to_string());
                }
                stack.push(el);
            }
            Event::Empty(start) => {
                let el = element_from_start(&reader, &start)?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Node::Element(el)),
                    None => {
                        if root.is_some() {
                            return Err("Extra content at the end of the document".to_string());
                        }
                        root = Some(el);
                    }
                }
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("unexpected end tag")?;
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Node::Element(el)),
                    None => root = Some(el),
                }
            }
            Event::Text(t) => {
                let raw = std::str::from_utf8(t.as_ref()).map_err(|e| e.to_string())?;
                match stack.last_mut() {
                    Some(parent) => {
                        let value = unescape(&normalize_newlines(raw))?;
                        push_text(parent, &value);
                    }
                    None => {
                        if !raw.trim().is_empty() {
                            return Err(if root.is_some() {
                                "Extra content at the end of the document".to_string()
                            } else {
                                "Start tag expected, '<' not found".to_string()
                            });
                        }
                    }
                }
            }
            Event::CData(c) => {
                let raw = std::str::from_utf8(c.as_ref()).map_err(|e| e.to_string())?;
                if let Some(parent) = stack.last_mut() {
                    push_text(parent, &normalize_newlines(raw));
                }
            }
            Event::Comment(_) | Event::PI(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Other);
                }
            }
            Event::Decl(_) | Event::DocType(_) => {}
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err("Premature end of data".to_string());
    }
    root.ok_or_else(|| "Start tag expected, '<' not found".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_namespaces_text_and_attributes() {
        let xml = br#"<?xml version="1.0"?>
<w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:body><w:p w:rsid="1" r:id="x&amp;y"><w:t xml:space="preserve"> a &lt;b&gt;&#13;</w:t></w:p></w:body></w:document>"#;
        let root = parse(xml).unwrap();
        assert!(root.is("urn:w", "document"));
        let p = root.path(&[("urn:w", "body"), ("urn:w", "p")]).unwrap();
        assert_eq!(p.attr(Some("urn:w"), "rsid"), Some("1"));
        assert_eq!(p.attr(Some("urn:r"), "id"), Some("x&y"));
        let t = p.child("urn:w", "t").unwrap();
        assert_eq!(t.text(), Some(" a <b>\r"));
        assert_eq!(
            t.attr(Some("http://www.w3.org/XML/1998/namespace"), "space"),
            Some("preserve")
        );
    }

    #[test]
    fn normalises_newlines_and_reports_errors() {
        let root = parse(b"<a b=\"x\ty\r\nz\">1\r\n2\r3<![CDATA[4\r\n]]></a>").unwrap();
        assert_eq!(root.attr(None, "b"), Some("x y z"));
        assert_eq!(root.text(), Some("1\n2\n34\n"));
        assert!(parse(b"").is_err());
        assert!(parse(b"<a><b></a>").is_err());
        assert!(parse(b"not xml").is_err());
    }
}
