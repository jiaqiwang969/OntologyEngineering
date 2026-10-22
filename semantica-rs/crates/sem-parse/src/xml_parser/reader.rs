//! An XML 1.0 + Namespaces reader producing the tree lxml (libxml2) or
//! `xml.etree.ElementTree` (expat) builds for `XMLParser`, and their error messages.
//!
//! The reader is a recursive-descent parser over the decoded document. Well-formedness and
//! namespace errors are reported with the message text and line/column conventions of the
//! engine being imitated:
//!
//! - lxml: `<message>, line L, column C (<string>, line L)` with 1-based columns, as
//!   `XMLSyntaxError` prints it;
//! - expat: `<message>: line L, column C` with 0-based columns, as `ParseError` prints it.
//!
//! Both count characters (not bytes) and normalise line ends first.

use std::collections::HashMap;

/// The engine whose tree shape and error messages are reproduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engine {
    /// lxml: comments and processing instructions are tree nodes, the encoding declaration
    /// is honoured (the input is UTF-8 bytes).
    Lxml,
    /// `xml.etree.ElementTree` over expat: comments and processing instructions are dropped,
    /// the input is a `str` (the encoding declaration is ignored).
    Expat,
}

/// Index of a node in a [`Document`].
pub(crate) type NodeId = usize;

/// An attribute (namespace declarations are kept separately).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attr {
    pub(crate) prefix: Option<String>,
    pub(crate) local: String,
    pub(crate) namespace: Option<String>,
    pub(crate) value: String,
}

impl Attr {
    /// The lxml/ElementTree key: `{namespace}local` or `local`.
    pub(crate) fn key(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{{{ns}}}{}", self.local),
            None => self.local.clone(),
        }
    }

    /// The qualified name as written.
    pub(crate) fn qname(&self) -> String {
        match &self.prefix {
            Some(p) => format!("{p}:{}", self.local),
            None => self.local.clone(),
        }
    }
}

/// The payload of a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeKind {
    /// The document node (always node 0).
    Document,
    /// An element.
    Element {
        prefix: Option<String>,
        local: String,
        namespace: Option<String>,
        attrs: Vec<Attr>,
        /// Namespace declarations on this element, in document order (`None` = default).
        nsdecls: Vec<(Option<String>, String)>,
    },
    /// Character data (adjacent text, CDATA and entity text merged).
    Text(String),
    /// A comment (lxml only).
    Comment(String),
    /// A processing instruction (lxml only).
    Pi { target: String, data: String },
}

/// A node of the document tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) kind: NodeKind,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
}

/// A parsed document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Document {
    pub(crate) nodes: Vec<Node>,
    /// The document element.
    pub(crate) root: NodeId,
}

impl Document {
    /// The element's tag as lxml/ElementTree report it (`{ns}local` or `local`).
    #[allow(dead_code)] // DOM API kept for the Docling HTML backend
    pub(crate) fn tag(&self, id: NodeId) -> Option<String> {
        match &self.nodes[id].kind {
            NodeKind::Element { local, namespace, .. } => Some(match namespace {
                Some(ns) => format!("{{{ns}}}{local}"),
                None => local.clone(),
            }),
            _ => None,
        }
    }

    /// `element.text`: the text before the first non-text child (`None` if there is none).
    pub(crate) fn element_text(&self, id: NodeId) -> Option<String> {
        let mut out: Option<String> = None;
        for &c in &self.nodes[id].children {
            match &self.nodes[c].kind {
                NodeKind::Text(t) => out.get_or_insert_with(String::new).push_str(t),
                _ => break,
            }
        }
        out
    }

    /// The children that `for child in element` yields: elements, comments and processing
    /// instructions (text is not a child in lxml/ElementTree).
    pub(crate) fn child_items(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes[id]
            .children
            .iter()
            .copied()
            .filter(|&c| !matches!(self.nodes[c].kind, NodeKind::Text(_)))
            .collect()
    }

    /// Whether the node is an element.
    pub(crate) fn is_element(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Element { .. })
    }
}

// ------------------------------------------------------------------------- characters

fn is_xml_char(c: char) -> bool {
    matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn is_name_start_char(c: char) -> bool {
    matches!(c,
        ':' | 'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
}

fn is_name_char(c: char) -> bool {
    is_name_start_char(c)
        || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

fn is_blank(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

// ---------------------------------------------------------------------------- reader

/// What a reference expanded to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RefKind {
    /// A character reference (`&#...;`).
    Char,
    /// One of the five predefined entities.
    Predefined,
    /// An internal general entity declared in the DTD.
    Internal,
}

/// A positioned error: the message and character index for each engine.
struct Fail {
    lxml: (String, usize),
    expat: (String, usize),
}

type R<T> = Result<T, Fail>;

fn fail(lxml_msg: impl Into<String>, lxml_pos: usize, expat_msg: impl Into<String>, expat_pos: usize) -> Fail {
    Fail {
        lxml: (lxml_msg.into(), lxml_pos),
        expat: (expat_msg.into(), expat_pos),
    }
}

const INVALID_TOKEN: &str = "not well-formed (invalid token)";

struct Reader {
    chars: Vec<char>,
    pos: usize,
    engine: Engine,
    remove_blank_text: bool,
    doc: Document,
    /// Internal general entities: name -> replacement text.
    entities: HashMap<String, String>,
    has_external_subset: bool,
    /// Namespace bindings in scope (innermost last).
    ns_scopes: Vec<Vec<(Option<String>, String)>>,
    /// Open elements: (qname, start line) for "Premature end of data" messages.
    open: Vec<(String, usize)>,
    /// Characters before the decoded input started (the skipped BOM for lxml).
    entity_depth: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, off: usize) -> Option<char> {
        self.chars.get(self.pos + off).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        let mut i = self.pos;
        for c in s.chars() {
            if self.chars.get(i) != Some(&c) {
                return false;
            }
            i += 1;
        }
        true
    }

    fn eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn skip_blanks(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(is_blank) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn text_between(&self, a: usize, b: usize) -> String {
        self.chars[a..b].iter().collect()
    }

    /// `Name`; `None` (position unchanged) if no name starts here.
    fn name(&mut self) -> Option<String> {
        let start = self.pos;
        if !self.peek().is_some_and(is_name_start_char) {
            return None;
        }
        self.pos += 1;
        while self.peek().is_some_and(is_name_char) {
            self.pos += 1;
        }
        Some(self.text_between(start, self.pos))
    }

    fn add_node(&mut self, parent: NodeId, kind: NodeKind) -> NodeId {
        let id = self.doc.nodes.len();
        self.doc.nodes.push(Node {
            kind,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.doc.nodes[parent].children.push(id);
        id
    }

    fn add_text(&mut self, parent: NodeId, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(&last) = self.doc.nodes[parent].children.last()
            && let NodeKind::Text(t) = &mut self.doc.nodes[last].kind
        {
            t.push_str(text);
            return;
        }
        self.add_node(parent, NodeKind::Text(text.to_string()));
    }

    fn line_of(&self, pos: usize) -> usize {
        1 + self.chars[..pos.min(self.chars.len())].iter().filter(|&&c| c == '\n').count()
    }

    // ------------------------------------------------------------------- prolog

    fn parse_document(&mut self) -> R<()> {
        if self.peek() == Some('\u{feff}') && self.engine == Engine::Expat {
            // expat counts the BOM as a character.
            self.pos += 1;
        }
        if self.starts_with("<?xml") && self.peek_at(5).is_some_and(is_blank) {
            self.parse_xml_decl()?;
        }
        self.parse_misc(true)?;
        if self.starts_with("<!DOCTYPE") {
            self.parse_doctype()?;
            self.parse_misc(true)?;
        }
        if self.eof() {
            return Err(fail(
                "Start tag expected, '<' not found",
                self.pos,
                "no element found",
                self.pos,
            ));
        }
        if self.peek() != Some('<') {
            return Err(fail("Start tag expected, '<' not found", self.pos, "syntax error", self.pos));
        }
        if !self.peek_at(1).is_some_and(is_name_start_char) {
            let expat_msg = if self.peek_at(1) == Some('!') { "syntax error" } else { INVALID_TOKEN };
            let expat_pos = if self.peek_at(1) == Some('!') { self.pos } else { self.pos + 1 };
            return Err(fail("StartTag: invalid element name", self.pos + 1, expat_msg, expat_pos));
        }
        let root = self.parse_element(0)?;
        self.doc.root = root;
        self.parse_misc(false)?;
        if !self.eof() {
            if self.starts_with("<?xml") && self.peek_at(5).is_some_and(|c| is_blank(c) || c == '?') {
                return Err(fail(
                    "XML declaration allowed only at the start of the document",
                    self.pos + 5,
                    "junk after document element",
                    self.pos,
                ));
            }
            return Err(fail(
                "Extra content at the end of the document",
                self.pos,
                "junk after document element",
                self.pos,
            ));
        }
        Ok(())
    }

    /// Comments, processing instructions and whitespace (`Misc*`).
    fn parse_misc(&mut self, before_root: bool) -> R<()> {
        loop {
            self.skip_blanks();
            if self.starts_with("<!--") {
                self.parse_comment(0)?;
            } else if self.starts_with("<?") {
                if before_root
                    && self.starts_with("<?xml")
                    && self.peek_at(5).is_some_and(|c| is_blank(c) || c == '?')
                {
                    return Err(fail(
                        "XML declaration allowed only at the start of the document",
                        self.pos + 5,
                        "XML or text declaration not at start of entity",
                        self.pos,
                    ));
                }
                if !before_root
                    && self.starts_with("<?xml")
                    && self.peek_at(5).is_some_and(|c| is_blank(c) || c == '?')
                {
                    return Ok(());
                }
                self.parse_pi(0)?;
            } else {
                return Ok(());
            }
        }
    }

    fn parse_xml_decl(&mut self) -> R<()> {
        let decl_start = self.pos;
        self.pos += 5;
        self.skip_blanks();
        if !self.starts_with("version") {
            return Err(fail(
                "Malformed declaration expecting version",
                self.pos,
                "XML declaration not well-formed",
                decl_start,
            ));
        }
        self.pos += "version".len();
        let version = self.decl_value()?;
        if version != "1.0" && !version.starts_with("1.") {
            if self.engine == Engine::Lxml {
                return Err(fail(format!("Unsupported version '{version}'"), self.pos, "", 0));
            }
        }
        let had_blank = self.skip_blanks();
        if self.starts_with("encoding") {
            if !had_blank {
                return Err(fail("Blank needed here", self.pos, "XML declaration not well-formed", self.pos));
            }
            self.pos += "encoding".len();
            let encoding = self.decl_value()?;
            if self.engine == Engine::Lxml {
                self.switch_encoding(&encoding)?;
            }
            self.skip_blanks();
        }
        if self.starts_with("standalone") {
            self.pos += "standalone".len();
            self.skip_blanks();
            if self.peek() != Some('=') {
                return Err(fail("expected '='", self.pos, "XML declaration not well-formed", self.pos));
            }
            self.pos += 1;
            self.skip_blanks();
            let quote = self.peek();
            if !matches!(quote, Some('"' | '\'')) {
                return Err(fail("String not started expecting ' or \"", self.pos, "XML declaration not well-formed", self.pos));
            }
            self.pos += 1;
            let value_start = self.pos;
            while self.peek().is_some_and(|c| Some(c) != quote) {
                self.pos += 1;
            }
            let value = self.text_between(value_start, self.pos);
            if value != "yes" && value != "no" {
                return Err(fail(
                    "standalone accepts only 'yes' or 'no'",
                    value_start,
                    "XML declaration not well-formed",
                    value_start,
                ));
            }
            self.pos += 1;
            self.skip_blanks();
        }
        if !self.starts_with("?>") {
            return Err(fail(
                "parsing XML declaration: '?>' expected",
                self.pos,
                "XML declaration not well-formed",
                self.pos,
            ));
        }
        self.pos += 2;
        Ok(())
    }

    /// `S? = S? ("..." | '...')` of the XML declaration.
    fn decl_value(&mut self) -> R<String> {
        self.skip_blanks();
        if self.peek() != Some('=') {
            return Err(fail("Malformed declaration expecting version", self.pos, "XML declaration not well-formed", self.pos));
        }
        self.pos += 1;
        self.skip_blanks();
        let quote = self.peek();
        if !matches!(quote, Some('"' | '\'')) {
            return Err(fail("String not started expecting ' or \"", self.pos, "XML declaration not well-formed", self.pos));
        }
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| Some(c) != quote) {
            self.pos += 1;
        }
        if self.eof() {
            return Err(fail("String not closed expecting \" or '", self.pos, "unclosed token", start - 1));
        }
        let value = self.text_between(start, self.pos);
        self.pos += 1;
        Ok(value)
    }

    /// lxml decodes the (UTF-8) input bytes with the declared encoding.
    fn switch_encoding(&mut self, encoding: &str) -> R<()> {
        let lower = encoding.to_ascii_lowercase();
        if matches!(lower.as_str(), "utf-8" | "utf8") {
            return Ok(());
        }
        let codec = match crate::common::py_codec_name(&lower) {
            Some(c) if !c.starts_with("utf-16") && !c.starts_with("utf-32") => c,
            _ => {
                return Err(fail(format!("Unsupported encoding: {encoding}"), self.pos, "", 0));
            }
        };
        let rest: String = self.chars[self.pos..].iter().collect();
        let bytes = rest.into_bytes();
        let decoded = crate::common::py_decode(&bytes, codec, crate::common::DecodeErrors::Replace)
            .map_err(|_| fail(format!("Unsupported encoding: {encoding}"), self.pos, "", 0))?;
        self.chars.truncate(self.pos);
        self.chars.extend(decoded.chars());
        Ok(())
    }

    fn parse_doctype(&mut self) -> R<()> {
        let start = self.pos;
        self.pos += "<!DOCTYPE".len();
        if !self.skip_blanks() {
            return Err(fail("Space required after '<!DOCTYPE'", self.pos, INVALID_TOKEN, self.pos));
        }
        if self.name().is_none() {
            return Err(fail("xmlParseDocTypeDecl : no DOCTYPE name !", self.pos, INVALID_TOKEN, self.pos));
        }
        self.skip_blanks();
        if self.starts_with("SYSTEM") || self.starts_with("PUBLIC") {
            let public = self.starts_with("PUBLIC");
            self.pos += 6;
            self.skip_blanks();
            self.quoted_literal(start)?;
            if public {
                self.skip_blanks();
                if matches!(self.peek(), Some('"' | '\'')) {
                    self.quoted_literal(start)?;
                }
            }
            self.has_external_subset = true;
            self.skip_blanks();
        }
        if self.peek() == Some('[') {
            self.pos += 1;
            self.parse_internal_subset(start)?;
            self.skip_blanks();
        }
        if self.peek() != Some('>') {
            return Err(fail("DOCTYPE improperly terminated", self.pos, INVALID_TOKEN, self.pos));
        }
        self.pos += 1;
        Ok(())
    }

    fn quoted_literal(&mut self, token_start: usize) -> R<String> {
        let quote = self.peek();
        if !matches!(quote, Some('"' | '\'')) {
            return Err(fail("String not started expecting ' or \"", self.pos, INVALID_TOKEN, self.pos));
        }
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| Some(c) != quote) {
            self.pos += 1;
        }
        if self.eof() {
            return Err(fail("Unfinished System or Public ID \" or ' expected", self.pos, "unclosed token", token_start));
        }
        let value = self.text_between(start, self.pos);
        self.pos += 1;
        Ok(value)
    }

    fn parse_internal_subset(&mut self, doctype_start: usize) -> R<()> {
        loop {
            self.skip_blanks();
            match self.peek() {
                None => {
                    return Err(fail(
                        "Premature end of data in tag DOCTYPE",
                        self.pos,
                        "unclosed token",
                        doctype_start,
                    ));
                }
                Some(']') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some('%') => {
                    // Parameter-entity reference: skipped.
                    while self.peek().is_some_and(|c| c != ';') {
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                Some('<') => {
                    if self.starts_with("<!--") {
                        // Comments in the DTD are not tree nodes.
                        let save = self.doc.nodes.len();
                        self.parse_comment(0)?;
                        if self.doc.nodes.len() > save {
                            self.doc.nodes.truncate(save);
                            self.doc.nodes[0].children.pop();
                        }
                    } else if self.starts_with("<?") {
                        let save = self.doc.nodes.len();
                        self.parse_pi(0)?;
                        if self.doc.nodes.len() > save {
                            self.doc.nodes.truncate(save);
                            self.doc.nodes[0].children.pop();
                        }
                    } else if self.starts_with("<!ENTITY") {
                        self.parse_entity_decl(doctype_start)?;
                    } else if self.starts_with("<!") {
                        self.skip_markup_decl(doctype_start)?;
                    } else {
                        return Err(fail("internal error: PEReference in subset", self.pos, "syntax error", self.pos));
                    }
                }
                Some(_) => {
                    return Err(fail("PEReference: forbidden within markup decl in internal subset", self.pos, "syntax error", self.pos));
                }
            }
        }
    }

    fn skip_markup_decl(&mut self, doctype_start: usize) -> R<()> {
        let mut quote: Option<char> = None;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '"' || c == '\'' => quote = Some(c),
                None if c == '>' => return Ok(()),
                None => {}
            }
        }
        Err(fail("Premature end of data in tag DOCTYPE", self.pos, "unclosed token", doctype_start))
    }

    fn parse_entity_decl(&mut self, doctype_start: usize) -> R<()> {
        self.pos += "<!ENTITY".len();
        self.skip_blanks();
        let parameter = if self.peek() == Some('%') {
            self.pos += 1;
            self.skip_blanks();
            true
        } else {
            false
        };
        let Some(name) = self.name() else {
            return Err(fail("xmlParseEntityDecl: no name", self.pos, INVALID_TOKEN, self.pos));
        };
        self.skip_blanks();
        let value = if matches!(self.peek(), Some('"' | '\'')) {
            let raw = self.quoted_literal(doctype_start)?;
            Some(expand_char_refs(&raw))
        } else {
            None // external entity (SYSTEM/PUBLIC): not loaded
        };
        self.skip_markup_decl(doctype_start)?;
        if !parameter
            && let Some(v) = value
        {
            self.entities.entry(name).or_insert(v);
        }
        Ok(())
    }

    // ----------------------------------------------------------- markup in content

    fn parse_comment(&mut self, parent: NodeId) -> R<()> {
        let start = self.pos;
        self.pos += 4;
        let body_start = self.pos;
        loop {
            if self.eof() {
                return Err(fail("Comment not terminated", self.pos, "unclosed token", start));
            }
            if self.starts_with("--") {
                if self.peek_at(2) == Some('>') {
                    let body = self.text_between(body_start, self.pos);
                    self.pos += 3;
                    if self.engine == Engine::Lxml {
                        self.add_node(parent, NodeKind::Comment(body));
                    }
                    return Ok(());
                }
                let so_far = self.text_between(body_start, self.pos);
                return Err(fail(
                    format!("Double hyphen within comment: <!--{so_far}"),
                    self.pos,
                    INVALID_TOKEN,
                    self.pos + 2,
                ));
            }
            let c = self.peek().unwrap_or(' ');
            if !is_xml_char(c) {
                return Err(fail(format!("xmlParseComment: invalid xmlChar value {}", c as u32), self.pos, INVALID_TOKEN, self.pos));
            }
            self.pos += 1;
        }
    }

    fn parse_pi(&mut self, parent: NodeId) -> R<()> {
        let start = self.pos;
        self.pos += 2;
        let name_start = self.pos;
        let Some(target) = self.name() else {
            return Err(fail("xmlParsePI : no target name", self.pos, INVALID_TOKEN, self.pos));
        };
        if target.eq_ignore_ascii_case("xml") {
            if target == "xml" {
                return Err(fail(
                    "XML declaration allowed only at the start of the document",
                    self.pos,
                    "XML or text declaration not at start of entity",
                    start,
                ));
            }
            return Err(fail("Invalid PI name", self.pos, INVALID_TOKEN, self.pos));
        }
        let _ = name_start;
        if self.starts_with("?>") {
            self.pos += 2;
            if self.engine == Engine::Lxml {
                self.add_node(
                    parent,
                    NodeKind::Pi {
                        target,
                        data: String::new(),
                    },
                );
            }
            return Ok(());
        }
        if !self.skip_blanks() {
            return Err(fail(format!("ParsePI: PI {target} space expected"), self.pos, INVALID_TOKEN, self.pos));
        }
        let data_start = self.pos;
        loop {
            if self.eof() {
                return Err(fail(format!("ParsePI: PI {target} never end ..."), self.pos, "unclosed token", start));
            }
            if self.starts_with("?>") {
                let data = self.text_between(data_start, self.pos);
                self.pos += 2;
                if self.engine == Engine::Lxml {
                    self.add_node(parent, NodeKind::Pi { target, data });
                }
                return Ok(());
            }
            self.pos += 1;
        }
    }

    fn parse_cdata(&mut self, parent: NodeId) -> R<()> {
        let start = self.pos;
        self.pos += "<![CDATA[".len();
        let body_start = self.pos;
        loop {
            if self.eof() {
                // libxml2 reports the buffered content, which lags two characters behind.
                let end = self.pos.saturating_sub(2).max(body_start);
                let body: String = self.text_between(body_start, end).chars().take(50).collect();
                return Err(fail(format!("CData section not finished\n{body}"), self.pos, "unclosed CDATA section", self.pos));
            }
            if self.starts_with("]]>") {
                let body = self.text_between(body_start, self.pos);
                self.pos += 3;
                self.add_text(parent, &body);
                return Ok(());
            }
            let _ = start;
            self.pos += 1;
        }
    }

    // ------------------------------------------------------------------- elements

    fn lookup_ns(&self, prefix: Option<&str>) -> Option<String> {
        if prefix == Some("xml") {
            return Some(XML_NS.to_string());
        }
        for scope in self.ns_scopes.iter().rev() {
            for (p, uri) in scope.iter().rev() {
                if p.as_deref() == prefix {
                    return if uri.is_empty() { None } else { Some(uri.clone()) };
                }
            }
        }
        None
    }

    fn is_bound(&self, prefix: &str) -> bool {
        if prefix == "xml" {
            return true;
        }
        self.ns_scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|(p, _)| p.as_deref() == Some(prefix))
            .is_some_and(|(_, uri)| !uri.is_empty())
    }

    fn split_qname(name: &str) -> (Option<String>, String) {
        match name.split_once(':') {
            Some((p, l)) if !p.is_empty() && !l.is_empty() && !l.contains(':') => (Some(p.to_string()), l.to_string()),
            _ => (None, name.to_string()),
        }
    }

    /// Parse an element starting at `<` (a name start character follows).
    fn parse_element(&mut self, parent: NodeId) -> R<NodeId> {
        let tag_start = self.pos;
        let start_line = self.line_of(tag_start);
        self.pos += 1;
        let qname = self.name().expect("caller checked the name start");
        // Attributes.
        let mut raw_attrs: Vec<(String, String, usize)> = Vec::new(); // (qname, value, name pos)
        let mut dup: Option<(String, usize)> = None;
        loop {
            let had_blank = self.skip_blanks();
            match self.peek() {
                None => {
                    return Err(fail(
                        format!("Couldn't find end of Start Tag {qname} line {start_line}"),
                        self.pos,
                        "unclosed token",
                        tag_start,
                    ));
                }
                Some('>') | Some('/') if self.peek() == Some('>') || self.peek_at(1) == Some('>') => break,
                Some(c) if is_name_start_char(c) => {
                    if !had_blank {
                        return Err(fail("attributes construct error", self.pos, INVALID_TOKEN, self.pos));
                    }
                    let name_pos = self.pos;
                    let name = self.name().expect("name start");
                    self.skip_blanks();
                    if self.peek() != Some('=') {
                        return Err(fail(
                            format!("Specification mandates value for attribute {name}"),
                            self.pos,
                            INVALID_TOKEN,
                            self.pos,
                        ));
                    }
                    self.pos += 1;
                    self.skip_blanks();
                    let value = self.attribute_value(tag_start)?;
                    if dup.is_none() && raw_attrs.iter().any(|(n, _, _)| *n == name) {
                        if self.engine == Engine::Expat {
                            return Err(fail("", 0, "duplicate attribute", name_pos));
                        }
                        dup = Some((name.clone(), name_pos));
                    }
                    raw_attrs.push((name, value, name_pos));
                }
                Some(_) => {
                    let msg = if had_blank || self.peek() == Some('/') {
                        "error parsing attribute name"
                    } else {
                        "attributes construct error"
                    };
                    let expat_pos = if self.peek() == Some('/') { self.pos + 1 } else { self.pos };
                    return Err(fail(msg, self.pos, INVALID_TOKEN, expat_pos));
                }
            }
        }
        let end_of_attrs = self.pos;
        if let Some((name, _)) = dup {
            return Err(fail(format!("Attribute {name} redefined"), end_of_attrs, "duplicate attribute", 0));
        }
        // Namespace declarations.
        let mut nsdecls: Vec<(Option<String>, String)> = Vec::new();
        let mut attrs_plain: Vec<(String, String)> = Vec::new();
        for (name, value, _) in &raw_attrs {
            if name == "xmlns" {
                nsdecls.push((None, value.clone()));
            } else if let Some(prefix) = name.strip_prefix("xmlns:") {
                if prefix == "xml" {
                    if value != XML_NS {
                        return Err(fail(
                            "xml namespace prefix mapped to wrong URI",
                            end_of_attrs,
                            "reserved prefix (xml) must not be undeclared or bound to another namespace name",
                            tag_start,
                        ));
                    }
                    continue;
                }
                if value.is_empty() {
                    return Err(fail(
                        format!("xmlns:{prefix}: Empty XML namespace is not allowed"),
                        end_of_attrs,
                        "must not undeclare prefix",
                        tag_start,
                    ));
                }
                nsdecls.push((Some(prefix.to_string()), value.clone()));
            } else {
                attrs_plain.push((name.clone(), value.clone()));
            }
        }
        self.ns_scopes.push(nsdecls.clone());
        let (prefix, local) = Self::split_qname(&qname);
        // libxml2 reports a mismatched end tag with the open element's local name, and the end
        // tag's local part when the open element is prefixed (its full name otherwise).
        let mismatch_open_name = local.clone();
        let open_is_prefixed = prefix.is_some();
        let namespace = match &prefix {
            Some(p) => {
                if !self.is_bound(p) {
                    self.ns_scopes.pop();
                    return Err(fail(
                        format!("Namespace prefix {p} on {local} is not defined"),
                        end_of_attrs,
                        "unbound prefix",
                        tag_start,
                    ));
                }
                self.lookup_ns(Some(p))
            }
            None => self.lookup_ns(None),
        };
        let mut attrs = Vec::new();
        for (name, value) in attrs_plain {
            let (aprefix, alocal) = Self::split_qname(&name);
            let anamespace = match &aprefix {
                Some(p) => {
                    if !self.is_bound(p) {
                        self.ns_scopes.pop();
                        return Err(fail(
                            format!("Namespace prefix {p} for {alocal} on {local} is not defined"),
                            end_of_attrs,
                            "unbound prefix",
                            tag_start,
                        ));
                    }
                    self.lookup_ns(Some(p))
                }
                None => None,
            };
            attrs.push(Attr {
                prefix: aprefix,
                local: alocal,
                namespace: anamespace,
                value,
            });
        }
        // Namespaced duplicates (`a:x` and `b:x` bound to one URI).
        for (i, a) in attrs.iter().enumerate() {
            if a.namespace.is_some() && attrs[..i].iter().any(|b| b.namespace == a.namespace && b.local == a.local) {
                self.ns_scopes.pop();
                return Err(fail(
                    format!("Namespaced Attribute {} in '{}' redefined", a.local, a.namespace.clone().unwrap_or_default()),
                    end_of_attrs,
                    "duplicate attribute",
                    tag_start,
                ));
            }
        }
        let id = self.add_node(
            parent,
            NodeKind::Element {
                prefix,
                local,
                namespace,
                attrs,
                nsdecls,
            },
        );
        if self.starts_with("/>") {
            self.pos += 2;
            self.ns_scopes.pop();
            return Ok(id);
        }
        self.pos += 1; // '>'
        self.open.push((qname.clone(), start_line));
        self.parse_content(id)?;
        // End tag: `</` Name S? `>`
        let end_tag_start = self.pos;
        self.pos += 2;
        let name_pos = self.pos;
        let close = self.name();
        self.skip_blanks();
        if self.peek() != Some('>') {
            let expat_pos = if close.is_none() { name_pos } else { self.pos };
            return Err(fail("expected '>'", self.pos, INVALID_TOKEN, expat_pos));
        }
        self.pos += 1;
        let close = close.unwrap_or_default();
        if close != qname {
            let close_name = if open_is_prefixed {
                Self::split_qname(&close).1
            } else {
                close.clone()
            };
            return Err(fail(
                format!(
                    "Opening and ending tag mismatch: {mismatch_open_name} line {start_line} and {close_name}"
                ),
                self.pos,
                "mismatched tag",
                name_pos,
            ));
        }
        let _ = end_tag_start;
        self.open.pop();
        self.ns_scopes.pop();
        Ok(id)
    }

    /// `AttValue` with entity expansion and whitespace normalisation.
    fn attribute_value(&mut self, tag_start: usize) -> R<String> {
        let quote = match self.peek() {
            Some(q @ ('"' | '\'')) => q,
            _ => {
                return Err(fail("AttValue: \" or ' expected", self.pos, INVALID_TOKEN, self.pos));
            }
        };
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(fail("AttValue: ' expected", self.pos, "unclosed token", tag_start));
                }
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some('<') => {
                    return Err(fail(
                        "Unescaped '<' not allowed in attributes values",
                        self.pos,
                        INVALID_TOKEN,
                        self.pos,
                    ));
                }
                Some('&') => {
                    let (text, kind) = self.reference(tag_start, true)?;
                    if kind == RefKind::Internal {
                        let expanded = self.expand_in_attribute(&text, tag_start, 0)?;
                        out.push_str(&expanded);
                    } else {
                        out.push_str(&text);
                    }
                }
                Some(c) if matches!(c, '\t' | '\n' | '\r') => {
                    out.push(' ');
                    self.pos += 1;
                }
                Some(c) if !is_xml_char(c) => {
                    return Err(fail(
                        format!("invalid character in attribute value"),
                        self.pos,
                        INVALID_TOKEN,
                        self.pos,
                    ));
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    /// A reference at `&`: returns the replacement text (char refs, predefined and internal
    /// entities) and its kind. `in_attribute` selects the expat error position for undefined
    /// entities.
    fn reference(&mut self, tag_start: usize, in_attribute: bool) -> R<(String, RefKind)> {
        let amp = self.pos;
        self.pos += 1;
        if self.peek() == Some('#') {
            self.pos += 1;
            let hex = self.peek() == Some('x');
            if hex {
                self.pos += 1;
            }
            let digits_start = self.pos;
            let mut value: u64 = 0;
            loop {
                match self.peek() {
                    Some(';') if self.pos > digits_start => break,
                    Some(c) if (hex && c.is_ascii_hexdigit()) || (!hex && c.is_ascii_digit()) => {
                        value = value.saturating_mul(if hex { 16 } else { 10 }).saturating_add(c.to_digit(16).unwrap_or(0) as u64);
                        if value > 0x7fff_ffff {
                            value = 0x7fff_ffff;
                        }
                        self.pos += 1;
                    }
                    _ => {
                        let msg = if hex { "CharRef: invalid hexadecimal value" } else { "CharRef: invalid decimal value" };
                        return Err(fail(msg, self.pos, INVALID_TOKEN, self.pos));
                    }
                }
            }
            self.pos += 1; // ';'
            let ch = char::from_u32(value as u32).filter(|c| is_xml_char(*c));
            return match ch {
                Some(c) => Ok((c.to_string(), RefKind::Char)),
                None => {
                    let msg = if value > 0x10ffff {
                        "xmlParseCharRef: character reference out of bounds".to_string()
                    } else {
                        format!("xmlParseCharRef: invalid xmlChar value {value}")
                    };
                    Err(fail(msg, self.pos, "reference to invalid character number", amp))
                }
            };
        }
        let Some(name) = self.name() else {
            return Err(fail("xmlParseEntityRef: no name", self.pos, INVALID_TOKEN, self.pos));
        };
        if self.peek() != Some(';') {
            return Err(fail("EntityRef: expecting ';'", self.pos, INVALID_TOKEN, self.pos));
        }
        self.pos += 1;
        let predefined = match name.as_str() {
            "lt" => Some("<"),
            "gt" => Some(">"),
            "amp" => Some("&"),
            "apos" => Some("'"),
            "quot" => Some("\""),
            _ => None,
        };
        if let Some(p) = predefined {
            return Ok((p.to_string(), RefKind::Predefined));
        }
        match self.entities.get(&name).cloned() {
            Some(v) => Ok((v, RefKind::Internal)),
            None => {
                let expat_msg = if self.has_external_subset && !in_attribute {
                    format!("undefined entity &{name};")
                } else {
                    "undefined entity".to_string()
                };
                let expat_pos = if in_attribute { tag_start } else { amp };
                Err(fail(format!("Entity '{name}' not defined"), self.pos, expat_msg, expat_pos))
            }
        }
    }

    /// An internal entity's replacement text inside an attribute value: nested references
    /// expanded, white space normalised, `<` rejected.
    fn expand_in_attribute(&self, text: &str, tag_start: usize, depth: usize) -> R<String> {
        if depth > 16 {
            return Err(fail("Detected an entity reference loop", self.pos, "recursive entity reference", tag_start));
        }
        let mut out = String::new();
        let mut rest = text;
        while let Some(c) = rest.chars().next() {
            match c {
                '<' => {
                    return Err(fail(
                        "'<' in entity is not allowed in attributes values",
                        self.pos,
                        INVALID_TOKEN,
                        tag_start,
                    ));
                }
                '&' => {
                    let Some(end) = rest.find(';') else {
                        return Err(fail("EntityRef: expecting ';'", self.pos, INVALID_TOKEN, tag_start));
                    };
                    let name = &rest[1..end];
                    let replacement = match name {
                        "lt" => "<".to_string(),
                        "gt" => ">".to_string(),
                        "amp" => "&".to_string(),
                        "apos" => "'".to_string(),
                        "quot" => "\"".to_string(),
                        _ => match self.entities.get(name) {
                            Some(v) => self.expand_in_attribute(v, tag_start, depth + 1)?,
                            None => {
                                return Err(fail(
                                    format!("Entity '{name}' not defined"),
                                    self.pos,
                                    "undefined entity",
                                    tag_start,
                                ));
                            }
                        },
                    };
                    out.push_str(&replacement);
                    rest = &rest[end + 1..];
                }
                c => {
                    out.push(if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c });
                    rest = &rest[c.len_utf8()..];
                }
            }
        }
        Ok(out)
    }

    /// Element content up to the matching `</`.
    fn parse_content(&mut self, parent: NodeId) -> R<()> {
        loop {
            let text_start = self.pos;
            let mut text = String::new();
            loop {
                match self.peek() {
                    None | Some('<') | Some('&') => break,
                    Some(']') if self.starts_with("]]>") => {
                        return Err(fail("Sequence ']]>' not allowed in content", self.pos, INVALID_TOKEN, self.pos + 2));
                    }
                    Some(c) if !is_xml_char(c) => {
                        return Err(fail(format!("PCDATA invalid Char value {}", c as u32), self.pos, INVALID_TOKEN, self.pos));
                    }
                    Some(c) => {
                        text.push(c);
                        self.pos += 1;
                    }
                }
            }
            let _ = text_start;
            if !text.is_empty() {
                self.push_char_data(parent, &text);
            }
            match self.peek() {
                None if self.entity_depth > 0 => return Ok(()),
                None => {
                    let (name, line) = self.open.last().cloned().unwrap_or_default();
                    return Err(fail(
                        format!("Premature end of data in tag {name} line {line}"),
                        self.pos,
                        "no element found",
                        self.pos,
                    ));
                }
                Some('&') => {
                    let amp = self.pos;
                    let (replacement, kind) = self.reference(amp, false)?;
                    if kind == RefKind::Internal && (replacement.contains('<') || replacement.contains('&')) {
                        self.expand_entity_markup(parent, &replacement)?;
                    } else {
                        self.push_char_data(parent, &replacement);
                    }
                }
                Some('<') => {
                    if self.starts_with("</") {
                        return Ok(());
                    } else if self.starts_with("<!--") {
                        self.parse_comment(parent)?;
                    } else if self.starts_with("<![CDATA[") {
                        self.parse_cdata(parent)?;
                    } else if self.starts_with("<?") {
                        self.parse_pi(parent)?;
                    } else if self.peek_at(1).is_some_and(is_name_start_char) {
                        self.parse_element(parent)?;
                    } else {
                        return Err(fail("StartTag: invalid element name", self.pos + 1, INVALID_TOKEN, self.pos + 1));
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    fn push_char_data(&mut self, parent: NodeId, text: &str) {
        self.add_text(parent, text);
    }

    /// Parse an internal entity's replacement text that contains markup, as content of
    /// `parent`.
    fn expand_entity_markup(&mut self, parent: NodeId, replacement: &str) -> R<()> {
        if self.entity_depth > 16 {
            return Err(fail("Detected an entity reference loop", self.pos, "recursive entity reference", self.pos));
        }
        let mut sub = Reader {
            chars: replacement.chars().collect(),
            pos: 0,
            engine: self.engine,
            remove_blank_text: self.remove_blank_text,
            doc: std::mem::take(&mut self.doc),
            entities: self.entities.clone(),
            has_external_subset: self.has_external_subset,
            ns_scopes: self.ns_scopes.clone(),
            open: vec![("entity".to_string(), 0)],
            entity_depth: self.entity_depth + 1,
        };
        let here = self.pos;
        let result = sub.parse_content(parent);
        self.doc = std::mem::take(&mut sub.doc);
        match result {
            Ok(()) if sub.eof() => Ok(()),
            Ok(()) => Err(fail("Opening and ending tag mismatch in entity", here, "mismatched tag", here)),
            Err(_) => Err(fail("Entity value is not well-formed", here, "not well-formed (invalid token)", here)),
        }
    }
}

/// Character references in an entity literal are expanded at declaration time.
fn expand_char_refs(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(i) = rest.find("&#") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let (hex, digits) = match after.strip_prefix('x') {
            Some(h) => (true, h),
            None => (false, after),
        };
        let n = digits.chars().take_while(|c| if hex { c.is_ascii_hexdigit() } else { c.is_ascii_digit() }).count();
        if n > 0 && digits[n..].starts_with(';') {
            let value = u32::from_str_radix(&digits[..n], if hex { 16 } else { 10 }).ok();
            if let Some(c) = value.and_then(char::from_u32) {
                out.push(c);
                rest = &digits[n + 1..];
                continue;
            }
        }
        out.push_str("&#");
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Remove whitespace-only text nodes that sit between elements (`remove_blank_text`).
fn remove_blank_text(doc: &mut Document) {
    for i in 0..doc.nodes.len() {
        let children = doc.nodes[i].children.clone();
        if !children.iter().any(|&c| doc.is_element(c)) {
            continue;
        }
        let keep: Vec<NodeId> = children
            .into_iter()
            .filter(|&c| match &doc.nodes[c].kind {
                NodeKind::Text(t) => !t.chars().all(is_blank),
                _ => true,
            })
            .collect();
        doc.nodes[i].children = keep;
    }
}

/// Normalise line ends (`\r\n` and lone `\r` become `\n`).
fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
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

/// Format an error position for the engine.
fn format_error(chars: &[char], engine: Engine, message: &str, pos: usize) -> String {
    let pos = pos.min(chars.len());
    let line = 1 + chars[..pos].iter().filter(|&&c| c == '\n').count();
    let line_start = chars[..pos].iter().rposition(|&c| c == '\n').map(|p| p + 1).unwrap_or(0);
    let col = pos - line_start;
    match engine {
        Engine::Lxml => format!("{message}, line {line}, column {} (<string>, line {line})", col + 1),
        Engine::Expat => format!("{message}: line {line}, column {col}"),
    }
}

/// Parse `text` as lxml (`etree.fromstring(text.encode("utf-8"), parser)`) or ElementTree
/// (`ET.fromstring(text)`) would; `Err` holds the exception text.
pub(crate) fn parse(text: &str, engine: Engine, remove_blank: bool) -> Result<Document, String> {
    let normalized = normalize_newlines(text);
    let mut chars: Vec<char> = normalized.chars().collect();
    if engine == Engine::Lxml && chars.first() == Some(&'\u{feff}') {
        // libxml2 consumes the byte-order mark before it starts counting.
        chars.remove(0);
    }
    let mut reader = Reader {
        chars,
        pos: 0,
        engine,
        remove_blank_text: remove_blank,
        doc: Document {
            nodes: vec![Node {
                kind: NodeKind::Document,
                parent: None,
                children: Vec::new(),
            }],
            root: 0,
        },
        entities: HashMap::new(),
        has_external_subset: false,
        ns_scopes: Vec::new(),
        open: Vec::new(),
        entity_depth: 0,
    };
    match reader.parse_document() {
        Ok(()) => {
            let mut doc = reader.doc;
            if reader.remove_blank_text {
                remove_blank_text(&mut doc);
            }
            Ok(doc)
        }
        Err(f) => {
            let (msg, pos) = match engine {
                Engine::Lxml => f.lxml,
                Engine::Expat => f.expat,
            };
            Err(format_error(&reader.chars, engine, &msg, pos))
        }
    }
}

/// The namespace bound to the `xmlns` attributes namespace (kept for completeness of the
/// namespace rules).
#[allow(dead_code)]
pub(crate) const XMLNS_NAMESPACE: &str = XMLNS_NS;
