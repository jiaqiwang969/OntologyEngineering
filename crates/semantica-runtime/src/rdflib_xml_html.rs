//! Private RDFLib 7.6.0 XMLLiteral/HTML candidate.
//! Parser replacement has finite oracle qualification, not whole-domain parity.
//! HTML serializer/optional tags follow html5rdf 1.2.1 (MIT); see NOTICE.md.
// Copyright (c) 2006-2013 James Graham and other contributors
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
// LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
// WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
use anyhow::{Result, anyhow, bail, ensure};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts};
use html5ever::tree_builder::{
    ElementFlags, NodeOrText, QuirksMode, TreeBuilder, TreeBuilderOpts, TreeSink,
};
use html5ever::{Attribute, ExpandedName, QualName, local_name, ns};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};
#[path = "rdflib_xml_name_ranges.rs"]
mod xml_names;

const XML_LITERAL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral";
const HTML_LITERAL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#HTML";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
const WRAPPER: &str = "rdflibtoplevelelement";

/// RDFLib catches lexical parse errors and retains their original lexical form.
/// Unknown datatypes remain a caller error; invalid XML/HTML is not a Rust error.
pub(crate) fn normalize(datatype: &str, text: &str) -> Result<String> {
    match datatype {
        XML_LITERAL => Ok(normalize_xml(text).unwrap_or_else(|_| text.to_owned())),
        HTML_LITERAL => Ok(normalize_html(text).unwrap_or_else(|_| text.to_owned())),
        _ => bail!("XML/HTML normalizer received unsupported datatype: {datatype}"),
    }
}

fn xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}
fn xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xa | 0xd | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
}
fn xml_name_start(c: char) -> bool {
    xml_names::start(c)
}
fn xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(xml_name_start) && chars.all(xml_names::name)
}
fn xml_qname(name: &str) -> Result<(&str, &str)> {
    let mut parts = name.split(':');
    let first = parts.next().unwrap();
    ensure!(xml_name(first), "invalid XML name");
    match parts.next() {
        Some(second) => {
            ensure!(
                xml_name(second) && parts.next().is_none(),
                "invalid XML qualified name"
            );
            Ok((first, second))
        }
        None => Ok(("", first)),
    }
}
fn xml_decode(text: &str, attribute: bool) -> Result<String> {
    ensure!(
        !attribute || !text.contains('<'),
        "literal < in XML attribute"
    );
    let prepared = if attribute {
        text.chars()
            .map(|c| if xml_space(c) { ' ' } else { c })
            .collect::<String>()
    } else {
        text.to_owned()
    };
    let decoded = quick_xml::escape::unescape(&prepared)?.into_owned();
    ensure!(
        decoded.chars().all(xml_char),
        "illegal XML character reference"
    );
    Ok(decoded)
}
fn xml_escape(text: &str, attribute: bool) -> String {
    let mut out = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if attribute {
        out = out
            .replace('"', "&quot;")
            .replace('\r', "&#13;")
            .replace('\n', "&#10;")
            .replace('\t', "&#9;");
    }
    out
}
fn check_xml_attribute_spacing(tail: &str) -> Result<()> {
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let before = i;
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }
        ensure!(i > before, "missing space before XML attribute");
        while i < bytes.len() && !matches!(bytes[i], b'=' | b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        ensure!(bytes.get(i) == Some(&b'='), "missing XML attribute =");
        i += 1;
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        let quote = *bytes
            .get(i)
            .ok_or_else(|| anyhow!("missing attribute value"))?;
        ensure!(matches!(quote, b'\'' | b'"'), "unquoted XML attribute");
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        ensure!(i < bytes.len(), "unclosed XML attribute");
        i += 1;
    }
    Ok(())
}
struct XmlFrame {
    name: String,
    attrs: Vec<(String, String)>,
    namespace: BTreeMap<String, String>,
    children: String,
}
impl XmlFrame {
    fn render(self) -> String {
        let mut out = format!("<{}", self.name);
        for (name, value) in self.attrs {
            out.push_str(&format!(" {name}=\"{}\"", xml_escape(&value, true)));
        }
        if self.children.is_empty() {
            out.push_str("/>");
        } else {
            out.push('>');
            out.push_str(&self.children);
            out.push_str(&format!("</{}>", self.name));
        }
        out
    }
}
fn xml_start(start: &BytesStart<'_>, inherited: &BTreeMap<String, String>) -> Result<XmlFrame> {
    let name = std::str::from_utf8(start.name().as_ref())?.to_owned();
    xml_qname(&name)?;
    let raw = std::str::from_utf8(start.as_ref())?;
    check_xml_attribute_spacing(&raw[name.len()..])?;
    let mut namespace = inherited.clone();
    let mut declarations = Vec::new();
    let mut attrs = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute?;
        let key = std::str::from_utf8(attribute.key.as_ref())?.to_owned();
        let (prefix, local) = xml_qname(&key)?;
        let value = xml_decode(std::str::from_utf8(attribute.value.as_ref())?, true)?;
        if key == "xmlns" || prefix == "xmlns" {
            let bound_prefix = if key == "xmlns" { "" } else { local };
            ensure!(
                bound_prefix != "xmlns" && value != XMLNS_NS,
                "reserved xmlns namespace"
            );
            ensure!(
                (bound_prefix == "xml") == (value == XML_NS),
                "reserved XML namespace"
            );
            ensure!(
                bound_prefix.is_empty() || !value.is_empty(),
                "empty prefixed namespace binding"
            );
            // The pinned Expat namespace separator is an ASCII space.
            ensure!(!value.contains(' '), "space in namespace URI");
            namespace.insert(bound_prefix.to_owned(), value.clone());
            declarations.push((key, value));
        } else {
            attrs.push((key, value));
        }
    }
    let (prefix, _) = xml_qname(&name)?;
    ensure!(
        prefix.is_empty() || namespace.contains_key(prefix),
        "unbound element prefix"
    );
    let mut expanded = BTreeSet::new();
    for (key, _) in &attrs {
        let (prefix, local) = xml_qname(key)?;
        let uri = if prefix.is_empty() {
            ""
        } else {
            namespace
                .get(prefix)
                .ok_or_else(|| anyhow!("unbound attribute prefix"))?
                .as_str()
        };
        ensure!(
            expanded.insert((uri.to_owned(), local.to_owned())),
            "duplicate expanded XML attribute"
        );
    }
    // Namespace callbacks run before Expat's ordinary attribute callbacks.
    declarations.extend(attrs);
    Ok(XmlFrame {
        name,
        attrs: declarations,
        namespace,
        children: String::new(),
    })
}
fn normalize_xml(text: &str) -> Result<String> {
    ensure!(text.chars().all(xml_char), "illegal XML character");
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let wrapped = format!("<{WRAPPER}>{text}</{WRAPPER}>");
    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().check_comments = true;
    let mut initial = BTreeMap::new();
    initial.insert("xml".to_owned(), XML_NS.to_owned());
    let mut stack: Vec<XmlFrame> = Vec::new();
    let mut result = None;
    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                ensure!(result.is_none(), "multiple XML document roots");
                let inherited = stack.last().map(|x| &x.namespace).unwrap_or(&initial);
                stack.push(xml_start(&start, inherited)?);
            }
            Event::Empty(start) => {
                let inherited = &stack
                    .last()
                    .ok_or_else(|| anyhow!("XML node outside wrapper"))?
                    .namespace;
                let rendered = xml_start(&start, inherited)?.render();
                stack.last_mut().unwrap().children.push_str(&rendered);
            }
            Event::End(end) => {
                let frame = stack
                    .pop()
                    .ok_or_else(|| anyhow!("unmatched XML end tag"))?;
                ensure!(
                    frame.name.as_bytes() == end.name().as_ref(),
                    "mismatched XML end tag"
                );
                if let Some(parent) = stack.last_mut() {
                    parent.children.push_str(&frame.render());
                } else {
                    ensure!(
                        frame.name == WRAPPER && result.is_none(),
                        "invalid XML root"
                    );
                    result = Some(frame.children);
                }
            }
            Event::Text(data) => {
                let raw = std::str::from_utf8(data.as_ref())?;
                ensure!(!raw.contains("]]>"), "CDATA terminator in XML text");
                let decoded = xml_decode(raw, false)?;
                stack
                    .last_mut()
                    .ok_or_else(|| anyhow!("XML text outside wrapper"))?
                    .children
                    .push_str(&xml_escape(&decoded, false));
            }
            Event::CData(data) => {
                let data = std::str::from_utf8(data.as_ref())?;
                if !data.is_empty() {
                    stack
                        .last_mut()
                        .ok_or_else(|| anyhow!("CDATA outside wrapper"))?
                        .children
                        .push_str(&format!("<![CDATA[{data}]]>"));
                }
            }
            Event::Comment(data) => {
                stack
                    .last_mut()
                    .ok_or_else(|| anyhow!("comment outside wrapper"))?
                    .children
                    .push_str(&format!("<!--{}-->", std::str::from_utf8(data.as_ref())?));
            }
            Event::PI(data) => {
                let target = std::str::from_utf8(data.target())?;
                ensure!(
                    xml_name(target) && !target.eq_ignore_ascii_case("xml"),
                    "invalid XML PI target"
                );
                let body = std::str::from_utf8(data.content())?.trim_start_matches(xml_space);
                stack
                    .last_mut()
                    .ok_or_else(|| anyhow!("PI outside wrapper"))?
                    .children
                    .push_str(&format!("<?{target} {body}?>"));
            }
            Event::Decl(_) | Event::DocType(_) => bail!("declaration inside XML fragment wrapper"),
            Event::Eof => break,
        }
    }
    ensure!(stack.is_empty(), "unclosed XML element");
    let result = result.ok_or_else(|| anyhow!("missing XML wrapper"))?;
    // RDFLib _writeXML performs this second test after removing the outer wrapper.
    Ok(if result == format!("<{WRAPPER}/>") {
        String::new()
    } else {
        result
    })
}

type Handle = Rc<Node>;
enum NodeData {
    Document,
    Element {
        name: QualName,
        display_name: String,
        attrs: RefCell<Vec<Attribute>>,
        integration: bool,
        template: Option<Handle>,
    },
    Text(RefCell<String>),
    Comment(String),
    Pi,
}
struct Node {
    data: NodeData,
    parent: RefCell<Weak<Node>>,
    children: RefCell<Vec<Handle>>,
}
impl Node {
    fn new(data: NodeData) -> Handle {
        Rc::new(Self {
            data,
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(Vec::new()),
        })
    }
    fn detach(node: &Handle) {
        let old = node.parent.borrow_mut().upgrade();
        *node.parent.borrow_mut() = Weak::new();
        if let Some(parent) = old {
            parent
                .children
                .borrow_mut()
                .retain(|x| !Rc::ptr_eq(x, node));
        }
    }
    fn insert(parent: &Handle, at: usize, child: NodeOrText<Handle>) {
        match child {
            NodeOrText::AppendText(text) => {
                if text.is_empty() {
                    return;
                }
                if at > 0 {
                    if let NodeData::Text(previous) = &parent.children.borrow()[at - 1].data {
                        previous.borrow_mut().push_str(&text);
                        return;
                    }
                }
                let node = Node::new(NodeData::Text(RefCell::new(text.to_string())));
                *node.parent.borrow_mut() = Rc::downgrade(parent);
                parent.children.borrow_mut().insert(at, node);
            }
            NodeOrText::AppendNode(node) => {
                let old_parent = node.parent.borrow().upgrade();
                let old_index = old_parent
                    .as_ref()
                    .filter(|old| Rc::ptr_eq(old, parent))
                    .and_then(|old| {
                        old.children
                            .borrow()
                            .iter()
                            .position(|x| Rc::ptr_eq(x, &node))
                    });
                let at = if old_index.is_some_and(|index| index < at) {
                    at - 1
                } else {
                    at
                };
                Node::detach(&node);
                *node.parent.borrow_mut() = Rc::downgrade(parent);
                parent.children.borrow_mut().insert(at, node);
            }
        }
    }
}
struct Dom {
    doc: Handle,
    errors: RefCell<Vec<String>>,
}
impl Dom {
    fn new() -> Self {
        Self {
            doc: Node::new(NodeData::Document),
            errors: RefCell::new(Vec::new()),
        }
    }
}
const MARKER_NS: &str = "urn:rdflib-private-compat-marker";
impl TreeSink for Dom {
    type Handle = Handle;
    type Output = Self;
    type ElemName<'a> = ExpandedName<'a>;
    fn finish(self) -> Self {
        self
    }
    fn parse_error(&self, msg: Cow<'static, str>) {
        // html5rdf InTableTextPhase.flushCharacters foster-parents this text
        // without reporting an error. Preserve that older strict policy only.
        if msg.as_ref() != "Non-space table text" {
            self.errors.borrow_mut().push(msg.into_owned());
        }
    }
    fn get_document(&self) -> Handle {
        self.doc.clone()
    }
    fn elem_name<'a>(&'a self, node: &'a Handle) -> ExpandedName<'a> {
        match &node.data {
            NodeData::Element { name, .. } => name.expanded(),
            _ => panic!("tree-builder requested non-element name"),
        }
    }
    fn create_element(
        &self,
        mut name: QualName,
        mut attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Handle {
        let display_name = attrs
            .iter()
            .find(|x| x.name.ns.as_ref() == MARKER_NS)
            .map(|x| x.value.to_string())
            .unwrap_or_else(|| name.local.to_string());
        attrs.retain(|x| x.name.ns.as_ref() != MARKER_NS);
        // command is void in HTML and an ordinary, identity-distinct foreign
        // element. Keep its foreign stack name distinct from a lexical param.
        if display_name == "command" && name.ns != ns!(html) {
            name.local = "command".into();
        }
        Node::new(NodeData::Element {
            name,
            display_name,
            attrs: RefCell::new(attrs),
            integration: flags.mathml_annotation_xml_integration_point,
            template: if flags.template {
                Some(Node::new(NodeData::Document))
            } else {
                None
            },
        })
    }
    fn create_comment(&self, text: StrTendril) -> Handle {
        Node::new(NodeData::Comment(text.to_string()))
    }
    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> Handle {
        Node::new(NodeData::Pi)
    }
    fn append(&self, parent: &Handle, child: NodeOrText<Handle>) {
        let at = parent.children.borrow().len();
        Node::insert(parent, at, child);
    }
    fn append_based_on_parent_node(
        &self,
        element: &Handle,
        previous: &Handle,
        child: NodeOrText<Handle>,
    ) {
        if element.parent.borrow().upgrade().is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(previous, child);
        }
    }
    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public: StrTendril,
        _system: StrTendril,
    ) {
        self.parse_error(Cow::Borrowed("doctype in fragment"));
    }
    fn get_template_contents(&self, target: &Handle) -> Handle {
        match &target.data {
            NodeData::Element {
                template: Some(fragment),
                ..
            } => fragment.clone(),
            _ => panic!("missing template contents"),
        }
    }
    fn same_node(&self, a: &Handle, b: &Handle) -> bool {
        Rc::ptr_eq(a, b)
    }
    fn set_quirks_mode(&self, _mode: QuirksMode) {}
    fn append_before_sibling(&self, sibling: &Handle, child: NodeOrText<Handle>) {
        let parent = sibling.parent.borrow().upgrade().expect("sibling parent");
        let at = parent
            .children
            .borrow()
            .iter()
            .position(|x| Rc::ptr_eq(x, sibling))
            .unwrap();
        Node::insert(&parent, at, child);
    }
    fn add_attrs_if_missing(&self, target: &Handle, added: Vec<Attribute>) {
        if let NodeData::Element { attrs, .. } = &target.data {
            let mut attrs = attrs.borrow_mut();
            for attr in added {
                if !attrs.iter().any(|x| x.name == attr.name) {
                    attrs.push(attr);
                }
            }
        }
    }
    fn remove_from_parent(&self, target: &Handle) {
        Node::detach(target);
    }
    fn reparent_children(&self, node: &Handle, parent: &Handle) {
        let children = std::mem::take(&mut *node.children.borrow_mut());
        for child in children {
            *child.parent.borrow_mut() = Weak::new();
            self.append(parent, NodeOrText::AppendNode(child));
        }
    }
    fn is_mathml_annotation_xml_integration_point(&self, node: &Handle) -> bool {
        matches!(
            &node.data,
            NodeData::Element {
                integration: true,
                ..
            }
        )
    }
}
/// html5rdf 1.2.1 treats command as a head/body void element and template as an
/// ordinary element. The selected modern parser has different branches for both.
struct LegacyTokens {
    builder: TreeBuilder<Handle, Dom>,
    pending: RefCell<String>,
    raw_element: RefCell<Option<String>>,
    cdata: Cell<bool>,
}
impl TokenSink for LegacyTokens {
    type Handle = Handle;
    fn process_token(&self, mut token: Token, line: u64) -> TokenSinkResult<Handle> {
        let mut raw_candidate = None;
        if let Token::CharacterTokens(_) = &token {
            if self.pending.borrow().starts_with("<![CDATA[")
                && self
                    .builder
                    .adjusted_current_node_present_but_not_in_html_namespace()
            {
                self.cdata.set(true);
            }
            if self.raw_element.borrow().is_none()
                && !self.cdata.get()
                && !strict_named_entities(&self.pending.borrow(), None)
            {
                self.builder
                    .sink
                    .parse_error(Cow::Borrowed("html5rdf strict named entity error"));
            }
        }
        if let Token::TagToken(tag) = &mut token {
            let original = tag.name.to_string();
            if tag.kind == html5ever::tokenizer::TagKind::StartTag {
                if !strict_attribute_entities(&self.pending.borrow()) {
                    self.builder
                        .sink
                        .parse_error(Cow::Borrowed("html5rdf strict attribute entity error"));
                }
                raw_candidate = Some(original.clone());
            } else if self.raw_element.borrow().as_deref() == Some(original.as_str()) {
                *self.raw_element.borrow_mut() = None;
            }
            let replacement = match original.as_str() {
                "command"
                    if tag.kind == html5ever::tokenizer::TagKind::EndTag
                        && self
                            .builder
                            .adjusted_current_node_present_but_not_in_html_namespace() =>
                {
                    None
                }
                "command" => Some("param"),
                // NUL cannot occur in a valid lexical HTML tag name. Ordinary
                // user tags therefore cannot alias these internal stack names.
                "template" => Some("\0rdflib-template"),
                "rb" => Some("\0rdflib-rb"),
                "rtc" => Some("\0rdflib-rtc"),
                "search" => Some("\0rdflib-search"),
                "dialog" => Some("\0rdflib-dialog"),
                _ => None,
            };
            if let Some(replacement) = replacement {
                tag.name = replacement.into();
                if tag.kind == html5ever::tokenizer::TagKind::StartTag {
                    tag.attrs.push(Attribute {
                        name: QualName::new(None, MARKER_NS.into(), "original".into()),
                        value: original.into(),
                    });
                }
            }
        }
        if !matches!(token, Token::ParseError(_)) {
            self.pending.borrow_mut().clear();
        }
        let result = self.builder.process_token(token, line);
        if matches!(
            result,
            TokenSinkResult::RawData(
                html5ever::tokenizer::states::RawKind::Rawtext
                    | html5ever::tokenizer::states::RawKind::ScriptData
                    | html5ever::tokenizer::states::RawKind::ScriptDataEscaped(_)
            ) | TokenSinkResult::Plaintext
        ) {
            *self.raw_element.borrow_mut() = raw_candidate;
        }
        result
    }
    fn end(&self) {
        self.builder.end();
    }
    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.builder
            .adjusted_current_node_present_but_not_in_html_namespace()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HtmlKind {
    Start,
    Empty,
    End,
    Text,
    Space,
    Comment,
}
struct HtmlEvent {
    kind: HtmlKind,
    name: String,
    attrs: Vec<(String, String)>,
}
impl HtmlEvent {
    fn new(kind: HtmlKind, name: String) -> Self {
        Self {
            kind,
            name,
            attrs: Vec::new(),
        }
    }
}
fn html_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0c' | ' ' | '\r')
}
fn html_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "command"
            | "embed"
            | "event-source"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}
fn raw_element(name: &str) -> bool {
    matches!(
        name,
        "style" | "script" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript"
    )
}
fn walk_html(node: &Handle, events: &mut Vec<HtmlEvent>) {
    match &node.data {
        NodeData::Document => {
            for child in node.children.borrow().iter() {
                walk_html(child, events);
            }
        }
        NodeData::Element {
            name,
            display_name,
            attrs,
            template,
            ..
        } => {
            let empty = name.ns == ns!(html) && html_void(display_name);
            let mut event = HtmlEvent::new(
                if empty {
                    HtmlKind::Empty
                } else {
                    HtmlKind::Start
                },
                display_name.clone(),
            );
            event.attrs = attrs
                .borrow()
                .iter()
                .map(|x| (x.name.local.to_string(), x.value.to_string()))
                .collect();
            events.push(event);
            if !empty {
                let children = template.as_ref().unwrap_or(node);
                for child in children.children.borrow().iter() {
                    walk_html(child, events);
                }
                events.push(HtmlEvent::new(HtmlKind::End, display_name.clone()));
            }
        }
        NodeData::Text(text) => {
            let text = text.borrow();
            let left = text.len() - text.trim_start_matches(html_space).len();
            let middle = &text[left..];
            let middle_end = middle.trim_end_matches(html_space).len();
            if left > 0 {
                events.push(HtmlEvent::new(HtmlKind::Space, text[..left].to_owned()));
            }
            if middle_end > 0 {
                events.push(HtmlEvent::new(
                    HtmlKind::Text,
                    middle[..middle_end].to_owned(),
                ));
            }
            if middle_end < middle.len() {
                events.push(HtmlEvent::new(
                    HtmlKind::Space,
                    middle[middle_end..].to_owned(),
                ));
            }
        }
        NodeData::Comment(text) => events.push(HtmlEvent::new(HtmlKind::Comment, text.clone())),
        NodeData::Pi => {}
    }
}
fn next_is(next: Option<&HtmlEvent>, kinds: &[HtmlKind]) -> bool {
    next.is_some_and(|x| kinds.contains(&x.kind))
}
fn optional_start(name: &str, previous: Option<&HtmlEvent>, next: Option<&HtmlEvent>) -> bool {
    // Preserve Python's `tagname in 'html'` substring predicate, including h/t/m/l.
    if "html".contains(name) {
        return !next_is(next, &[HtmlKind::Comment, HtmlKind::Space]);
    }
    match name {
        "head" => {
            next_is(next, &[HtmlKind::Start, HtmlKind::Empty])
                || next.is_some_and(|x| x.kind == HtmlKind::End && x.name == "head")
        }
        "body" => {
            !next_is(next, &[HtmlKind::Comment, HtmlKind::Space])
                && !next.is_some_and(|x| {
                    x.kind == HtmlKind::Start && matches!(x.name.as_str(), "script" | "style")
                })
        }
        "colgroup" => next.is_some_and(|x| {
            matches!(x.kind, HtmlKind::Start | HtmlKind::Empty) && x.name == "col"
        }),
        "tbody" => {
            next.is_some_and(|x| x.kind == HtmlKind::Start && x.name == "tr")
                && !previous.is_some_and(|x| {
                    x.kind == HtmlKind::End
                        && matches!(x.name.as_str(), "tbody" | "thead" | "tfoot")
                })
        }
        _ => false,
    }
}
fn optional_end(name: &str, next: Option<&HtmlEvent>) -> bool {
    let starts = next_is(next, &[HtmlKind::Start]);
    let ends = next.is_none() || next_is(next, &[HtmlKind::End]);
    let n = next.map(|x| x.name.as_str()).unwrap_or("");
    match name {
        "html" | "head" | "body" => !next_is(next, &[HtmlKind::Comment, HtmlKind::Space]),
        "li" | "optgroup" | "tr" => {
            if starts {
                n == name
            } else {
                ends
            }
        }
        "dt" => starts && matches!(n, "dt" | "dd"),
        "dd" => {
            if starts {
                matches!(n, "dt" | "dd")
            } else {
                ends
            }
        }
        "p" => {
            if next_is(next, &[HtmlKind::Start, HtmlKind::Empty]) {
                matches!(
                    n,
                    "address"
                        | "article"
                        | "aside"
                        | "blockquote"
                        | "datagrid"
                        | "dialog"
                        | "dir"
                        | "div"
                        | "dl"
                        | "fieldset"
                        | "footer"
                        | "form"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "header"
                        | "hr"
                        | "menu"
                        | "nav"
                        | "ol"
                        | "p"
                        | "pre"
                        | "section"
                        | "table"
                        | "ul"
                )
            } else {
                ends
            }
        }
        "option" => {
            if starts {
                matches!(n, "option" | "optgroup")
            } else {
                ends
            }
        }
        "rt" | "rp" => {
            if starts {
                matches!(n, "rt" | "rp")
            } else {
                ends
            }
        }
        "colgroup" => {
            if next_is(next, &[HtmlKind::Comment, HtmlKind::Space]) {
                false
            } else {
                !starts || n != "colgroup"
            }
        }
        "thead" => starts && matches!(n, "tbody" | "tfoot"),
        "tbody" => {
            if starts {
                matches!(n, "tbody" | "tfoot")
            } else {
                ends
            }
        }
        "tfoot" => {
            if starts {
                n == "tbody"
            } else {
                ends
            }
        }
        "td" | "th" => {
            if starts {
                matches!(n, "td" | "th")
            } else {
                ends
            }
        }
        _ => false,
    }
}
fn boolean_attribute(tag: &str, attr: &str) -> bool {
    if matches!(attr, "irrelevant" | "itemscope") {
        return true;
    }
    match tag {
        "style" => attr == "scoped",
        "img" => attr == "ismap",
        "audio" | "video" => matches!(attr, "autoplay" | "controls"),
        "script" => matches!(attr, "defer" | "async"),
        "details" => attr == "open",
        "datagrid" => matches!(attr, "multiple" | "disabled"),
        "command" => matches!(attr, "hidden" | "disabled" | "checked" | "default"),
        "hr" => attr == "noshade",
        "menu" => attr == "autosubmit",
        "fieldset" | "optgroup" | "output" => matches!(attr, "disabled" | "readonly"),
        "option" => matches!(attr, "disabled" | "readonly" | "selected"),
        "button" => matches!(attr, "disabled" | "autofocus"),
        "input" => matches!(
            attr,
            "disabled" | "readonly" | "required" | "autofocus" | "checked" | "ismap"
        ),
        "select" => matches!(attr, "disabled" | "readonly" | "autofocus" | "multiple"),
        "ol" => attr == "reversed",
        "iframe" => attr == "seamless",
        _ => false,
    }
}
fn quote_legacy(c: char) -> bool {
    matches!(c as u32,0..=0x20|0x22|0x27|0x2f|0x3c..=0x3e|0x60|0xa0|0x1680|0x180e|0x180f|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000)
}
/// html5rdf reports missing/unknown named references before the ambiguous
/// attribute-reference exception. Modern HTML5 parsers suppress some of these.
/// Only original raw lexemes are inspected; decoded &amp; never becomes an error.
fn strict_named_entities(raw: &str, allowed: Option<u8>) -> bool {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            i += 1;
            continue;
        }
        i += 1;
        let Some(&next) = bytes.get(i) else {
            return true;
        };
        if matches!(next, b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b'<' | b'&')
            || Some(next) == allowed
        {
            continue;
        }
        if next == b'#' {
            continue;
        } // Numeric strict errors are emitted by html5ever.
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if bytes.get(i) != Some(&b';') {
            return false;
        }
        let candidate = &raw[start..=i];
        if !html5ever::data::NAMED_ENTITIES
            .get(candidate)
            .is_some_and(|&(first, _)| first != 0)
        {
            return false;
        }
        i += 1;
    }
    true
}
fn strict_attribute_entities(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    let mut i = 0;
    // Each input scalar is fed separately; at TagToken emission this buffer is
    // the raw markup since the preceding emitted token, without decoded data.
    while i < bytes.len() && bytes[i] != b'<' {
        i += 1;
    }
    if i == bytes.len() {
        return true;
    }
    i += 1;
    if bytes.get(i) == Some(&b'/') {
        return true;
    }
    while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | 0x0c | b'>' | b'/')
    {
        i += 1;
    }
    while i < bytes.len() {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | 0x0c | b'/') {
            i += 1;
        }
        if i == bytes.len() || bytes[i] == b'>' {
            break;
        }
        while i < bytes.len()
            && !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | 0x0c | b'=' | b'>')
        {
            i += 1;
        }
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | 0x0c) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | 0x0c) {
            i += 1;
        }
        let Some(&first) = bytes.get(i) else {
            break;
        };
        let quote = if matches!(first, b'\'' | b'"') {
            i += 1;
            Some(first)
        } else {
            None
        };
        let start = i;
        while i < bytes.len()
            && match quote {
                Some(q) => bytes[i] != q,
                None => !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | 0x0c | b'>'),
            }
        {
            i += 1;
        }
        if !strict_named_entities(&tag[start..i], quote.or(Some(b'>'))) {
            return false;
        }
        if quote.is_some() && i < bytes.len() {
            i += 1;
        }
    }
    true
}
fn serialize_html(events: &[HtmlEvent]) -> String {
    let mut out = String::new();
    let mut raw = false;
    for (i, event) in events.iter().enumerate() {
        let previous = i.checked_sub(1).map(|x| &events[x]);
        let next = events.get(i + 1);
        if event.kind == HtmlKind::Start
            && event.attrs.is_empty()
            && optional_start(&event.name, previous, next)
        {
            continue;
        }
        if event.kind == HtmlKind::End && optional_end(&event.name, next) {
            continue;
        }
        match event.kind {
            HtmlKind::Start | HtmlKind::Empty => {
                out.push('<');
                out.push_str(&event.name);
                if raw_element(&event.name) {
                    raw = true;
                }
                for (name, value) in &event.attrs {
                    out.push(' ');
                    out.push_str(name);
                    if boolean_attribute(&event.name, name) {
                        continue;
                    }
                    out.push('=');
                    let quote = value.is_empty() || value.chars().any(quote_legacy);
                    let value = value.replace('&', "&amp;");
                    if quote {
                        let character = if value.contains('"') && !value.contains('\'') {
                            '\''
                        } else {
                            '"'
                        };
                        out.push(character);
                        out.push_str(&if character == '\'' {
                            value.replace('\'', "&#39;")
                        } else {
                            value.replace('"', "&quot;")
                        });
                        out.push(character);
                    } else {
                        out.push_str(&value);
                    }
                }
                out.push('>');
            }
            HtmlKind::End => {
                if raw_element(&event.name) {
                    raw = false;
                }
                out.push_str("</");
                out.push_str(&event.name);
                out.push('>');
            }
            HtmlKind::Text => {
                if raw {
                    out.push_str(&event.name);
                } else {
                    out.push_str(&xml_escape(&event.name, false));
                }
            }
            HtmlKind::Space => out.push_str(&event.name),
            HtmlKind::Comment => {
                out.push_str("<!--");
                out.push_str(&event.name);
                out.push_str("-->");
            }
        }
    }
    out
}
fn normalize_html(text: &str) -> Result<String> {
    let dom = Dom::new();
    let context = dom.create_element(
        QualName::new(None, ns!(html), local_name!("div")),
        Vec::new(),
        ElementFlags::default(),
    );
    let builder = TreeBuilder::new_for_fragment(
        dom,
        context,
        None,
        TreeBuilderOpts {
            exact_errors: true,
            scripting_enabled: false,
            ..Default::default()
        },
    );
    let state = builder.tokenizer_state_for_context_elem(false);
    let tokenizer = Tokenizer::new(
        LegacyTokens {
            builder,
            pending: RefCell::new(String::new()),
            raw_element: RefCell::new(None),
            cdata: Cell::new(false),
        },
        TokenizerOpts {
            exact_errors: true,
            discard_bom: false,
            initial_state: Some(state),
            ..Default::default()
        },
    );
    let queue = html5ever::buffer_queue::BufferQueue::default();
    let mut previous = ['\0'; 2];
    for character in text.chars() {
        tokenizer.sink.pending.borrow_mut().push(character);
        queue.push_back(character.to_string().into());
        while let html5ever::TokenizerResult::Script(_) = tokenizer.feed(&queue) {}
        if previous == [']', ']'] && character == '>' {
            tokenizer.sink.cdata.set(false);
        }
        previous = [previous[1], character];
    }
    tokenizer.end();
    let dom = &tokenizer.sink.builder.sink;
    ensure!(
        dom.errors.borrow().is_empty(),
        "strict HTML parse error: {}",
        dom.errors.borrow().join("; ")
    );
    let root = dom
        .doc
        .children
        .borrow()
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("missing HTML fragment root"))?;
    let mut events = Vec::new();
    for node in root.children.borrow().iter() {
        walk_html(node, &mut events);
    }
    Ok(serialize_html(&events))
}
