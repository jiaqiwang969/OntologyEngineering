//! A non-validating XML 1.0 parser producing a namespace-resolved DOM that keeps what the
//! two Python front-ends need: comments, processing instructions, the DOCTYPE (with its
//! internal subset parsed into entity/element/attribute-list declarations), in-scope namespace
//! maps and source lines. [`super::etree`] builds `xml.etree.ElementTree` elements from it and
//! the XML ingestor reads it the way it reads an lxml tree.
//!
//! Well-formedness errors carry a kind, line and column so each front-end can word them like
//! expat (`"mismatched tag: line 1, column 15"`) or libxml2.

use indexmap::IndexMap;

/// The XML namespace (`xml:` prefix).
pub const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";

/// What went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlErrorKind {
    /// Text or markup where none is allowed (expat "syntax error").
    Syntax,
    /// An invalid character or token (expat "not well-formed (invalid token)").
    InvalidToken,
    /// `</b>` closes `<a>` (expat "mismatched tag").
    MismatchedTag {
        /// The open element.
        expected: String,
        /// The end tag found.
        found: String,
        /// Line of the start tag.
        start_line: usize,
    },
    /// End of input inside the document (expat "no element found" / "unclosed token").
    NoElementFound,
    /// A construct was not closed (expat "unclosed token").
    UnclosedToken,
    /// Content after the root element (expat "junk after document element").
    JunkAfterDocument,
    /// `&name;` with no declaration.
    UndefinedEntity(String),
    /// An attribute given twice.
    DuplicateAttribute(String),
    /// A prefix with no namespace declaration.
    UnboundPrefix(String),
    /// Entity declarations were rejected (`forbid_entities`, defusedxml's `EntitiesForbidden`).
    EntitiesForbidden(String),
    /// A DTD was rejected (`forbid_dtd`).
    DtdForbidden,
    /// A recursive entity reference.
    RecursiveEntity(String),
}

/// A well-formedness error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError {
    /// The kind of error.
    pub kind: XmlErrorKind,
    /// 1-based line.
    pub line: usize,
    /// 0-based column (characters).
    pub column: usize,
}

impl XmlError {
    /// The expat wording ElementTree's `ParseError` uses.
    pub fn expat_message(&self) -> String {
        let what = match &self.kind {
            XmlErrorKind::Syntax => "syntax error".to_string(),
            XmlErrorKind::InvalidToken => "not well-formed (invalid token)".to_string(),
            XmlErrorKind::MismatchedTag { .. } => "mismatched tag".to_string(),
            XmlErrorKind::NoElementFound => "no element found".to_string(),
            XmlErrorKind::UnclosedToken => "unclosed token".to_string(),
            XmlErrorKind::JunkAfterDocument => "junk after document element".to_string(),
            XmlErrorKind::UndefinedEntity(_) => "undefined entity".to_string(),
            XmlErrorKind::DuplicateAttribute(_) => "duplicate attribute".to_string(),
            XmlErrorKind::UnboundPrefix(_) => "unbound prefix".to_string(),
            XmlErrorKind::EntitiesForbidden(name) => {
                return format!("EntitiesForbidden(name='{name}', system_id=None, public_id=None)");
            }
            XmlErrorKind::DtdForbidden => {
                return "DTDForbidden(name='', system_id=None, public_id=None)".to_string();
            }
            XmlErrorKind::RecursiveEntity(_) => "recursive entity reference".to_string(),
        };
        format!("{what}: line {}, column {}", self.line, self.column)
    }

    /// The libxml2 wording lxml's `XMLSyntaxError` uses.
    pub fn libxml2_message(&self) -> String {
        let col = self.column + 1;
        match &self.kind {
            XmlErrorKind::MismatchedTag {
                expected,
                found,
                start_line,
            } => format!(
                "Opening and ending tag mismatch: {expected} line {start_line} and {found}, line {}, column {col}",
                self.line
            ),
            XmlErrorKind::Syntax => format!("Start tag expected, '<' not found, line {}, column {col}", self.line),
            XmlErrorKind::InvalidToken => format!("StartTag: invalid element name, line {}, column {col}", self.line),
            XmlErrorKind::NoElementFound => format!(
                "Premature end of data in tag, line {}, column {col}",
                self.line
            ),
            XmlErrorKind::UnclosedToken => format!("Couldn't find end of Start Tag, line {}, column {col}", self.line),
            XmlErrorKind::JunkAfterDocument => format!(
                "Extra content at the end of the document, line {}, column {col}",
                self.line
            ),
            XmlErrorKind::UndefinedEntity(name) => {
                format!("Entity '{name}' not defined, line {}, column {col}", self.line)
            }
            XmlErrorKind::DuplicateAttribute(name) => {
                format!("Attribute {name} redefined, line {}, column {col}", self.line)
            }
            XmlErrorKind::UnboundPrefix(prefix) => format!(
                "Namespace prefix {prefix} is not defined, line {}, column {col}",
                self.line
            ),
            XmlErrorKind::EntitiesForbidden(name) => format!("Entity '{name}' forbidden"),
            XmlErrorKind::DtdForbidden => "DTD forbidden".to_string(),
            XmlErrorKind::RecursiveEntity(name) => format!(
                "Detected an entity reference loop ({name}), line {}, column {col}",
                self.line
            ),
        }
    }
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.expat_message())
    }
}

/// An attribute after namespace resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlAttribute {
    /// Prefix as written.
    pub prefix: Option<String>,
    /// Local name.
    pub local: String,
    /// Namespace URI (unprefixed attributes have none).
    pub namespace: Option<String>,
    /// Value (entities expanded, whitespace normalised as XML requires).
    pub value: String,
}

impl XmlAttribute {
    /// Clark notation (`{uri}local` or `local`).
    pub fn clark_name(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{{{ns}}}{}", self.local),
            None => self.local.clone(),
        }
    }
}

/// An element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlElement {
    /// Prefix as written.
    pub prefix: Option<String>,
    /// Local name.
    pub local: String,
    /// Namespace URI.
    pub namespace: Option<String>,
    /// Attributes in source order (namespace declarations excluded).
    pub attributes: Vec<XmlAttribute>,
    /// Namespace declarations made on this element (`None` key = default namespace).
    pub ns_decls: Vec<(Option<String>, String)>,
    /// In-scope namespaces (lxml's `nsmap`, without the implicit `xml` prefix).
    pub nsmap: IndexMap<Option<String>, String>,
    /// Child nodes.
    pub children: Vec<XmlNode>,
    /// 1-based source line of the start tag.
    pub line: usize,
}

impl XmlElement {
    /// Clark notation of the tag (`{uri}local` or `local`).
    pub fn clark_name(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{{{ns}}}{}", self.local),
            None => self.local.clone(),
        }
    }

    /// Qualified name as written (`prefix:local`).
    pub fn qname(&self) -> String {
        match &self.prefix {
            Some(p) => format!("{p}:{}", self.local),
            None => self.local.clone(),
        }
    }

    /// Child elements.
    pub fn element_children(&self) -> impl Iterator<Item = &XmlElement> {
        self.children.iter().filter_map(|c| match c {
            XmlNode::Element(e) => Some(e),
            _ => None,
        })
    }
}

/// A node of the content tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlNode {
    /// An element.
    Element(XmlElement),
    /// Character data (CDATA sections are merged into text).
    Text(String),
    /// A comment.
    Comment(String),
    /// A processing instruction (target, data).
    Pi(String, String),
}

/// A content model particle of an `<!ELEMENT>` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentParticle {
    /// An element name.
    Name(String, Occurrence),
    /// `( a , b )`.
    Seq(Vec<ContentParticle>, Occurrence),
    /// `( a | b )`.
    Choice(Vec<ContentParticle>, Occurrence),
}

/// `?`, `*`, `+` or exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occurrence {
    /// Exactly once.
    One,
    /// `?`.
    Optional,
    /// `*`.
    ZeroOrMore,
    /// `+`.
    OneOrMore,
}

/// An `<!ELEMENT>` content specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentSpec {
    /// `EMPTY`.
    Empty,
    /// `ANY`.
    Any,
    /// `(#PCDATA | a | b)*` (names allowed in mixed content).
    Mixed(Vec<String>),
    /// Element content.
    Children(ContentParticle),
}

/// An attribute definition of an `<!ATTLIST>` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeDecl {
    /// Attribute name.
    pub name: String,
    /// Type (`CDATA`, `ID`, an enumeration as `(a|b)`, ...).
    pub kind: String,
    /// `#REQUIRED`, `#IMPLIED`, `#FIXED` or empty.
    pub default_kind: String,
    /// Default value.
    pub default_value: Option<String>,
}

/// A parsed DOCTYPE.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocType {
    /// Root element name.
    pub name: String,
    /// `PUBLIC` identifier.
    pub public_id: Option<String>,
    /// `SYSTEM` identifier.
    pub system_url: Option<String>,
    /// Whether an internal subset was given.
    pub has_internal_subset: bool,
    /// `<!ELEMENT>` declarations.
    pub elements: IndexMap<String, ContentSpec>,
    /// `<!ATTLIST>` declarations.
    pub attlists: IndexMap<String, Vec<AttributeDecl>>,
    /// Internal general entities (`<!ENTITY name "value">`).
    pub entities: IndexMap<String, String>,
    /// External general entities (`<!ENTITY name SYSTEM "uri">`).
    pub external_entities: IndexMap<String, String>,
}

/// A parsed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlDocument {
    /// `version` from the XML declaration.
    pub xml_version: Option<String>,
    /// `encoding` from the XML declaration.
    pub encoding: Option<String>,
    /// `standalone` from the XML declaration.
    pub standalone: Option<bool>,
    /// The DOCTYPE, if any.
    pub doctype: Option<DocType>,
    /// The root element.
    pub root: XmlElement,
}

/// Parser options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOptions {
    /// Keep comment nodes.
    pub keep_comments: bool,
    /// Keep processing instructions.
    pub keep_pis: bool,
    /// Drop whitespace-only text between elements (lxml `remove_blank_text`).
    pub remove_blank_text: bool,
    /// Expand internal entities declared in the DTD (expat and libxml2 with `resolve_entities`).
    pub expand_internal_entities: bool,
    /// Reject documents declaring entities (defusedxml `forbid_entities`).
    pub forbid_entities: bool,
    /// Reject documents with a DTD (defusedxml `forbid_dtd`).
    pub forbid_dtd: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            keep_comments: false,
            keep_pis: false,
            remove_blank_text: false,
            expand_internal_entities: true,
            forbid_entities: false,
            forbid_dtd: false,
        }
    }
}

/// Decodes raw XML bytes using a BOM or the encoding of the XML declaration.
pub fn decode_xml_bytes(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(&bytes[2..]).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(&bytes[2..]).0.into_owned();
    }
    if bytes.starts_with(b"<?xml") {
        let end = bytes.iter().position(|&b| b == b'>').unwrap_or(bytes.len());
        let decl = String::from_utf8_lossy(&bytes[..end]);
        if let Some(enc) = decl_attr(&decl, "encoding") {
            let lower = enc.to_lowercase();
            if matches!(lower.as_str(), "iso-8859-1" | "latin-1" | "latin1" | "iso8859-1") {
                return bytes.iter().map(|&b| b as char).collect();
            }
            if lower != "utf-8"
                && let Some(e) = encoding_rs::Encoding::for_label(lower.as_bytes())
            {
                return e.decode_without_bom_handling(bytes).0.into_owned();
            }
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn decl_attr(decl: &str, name: &str) -> Option<String> {
    let idx = decl.find(name)?;
    let rest = decl[idx + name.len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let inner = &rest[1..];
    let end = inner.find(quote)?;
    Some(inner[..end].to_string())
}

struct Parser<'o> {
    chars: Vec<char>,
    pos: usize,
    options: &'o ParseOptions,
    doctype: Option<DocType>,
}

fn is_name_start(c: char) -> bool {
    c == ':'
        || c == '_'
        || c.is_ascii_alphabetic()
        || matches!(c as u32, 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0x2FF | 0x370..=0x37D | 0x37F..=0x1FFF | 0x200C..=0x200D | 0x2070..=0x218F | 0x2C00..=0x2FEF | 0x3001..=0xD7FF | 0xF900..=0xFDCF | 0xFDF0..=0xFFFD | 0x10000..=0xEFFFF)
}

fn is_name_char(c: char) -> bool {
    is_name_start(c)
        || c == '-'
        || c == '.'
        || c.is_ascii_digit()
        || c as u32 == 0xB7
        || matches!(c as u32, 0x0300..=0x036F | 0x203F..=0x2040)
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

struct RawElement {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<RawNode>,
    line: usize,
}

enum RawNode {
    Element(RawElement),
    Text(String),
    Comment(String),
    Pi(String, String),
}

impl<'o> Parser<'o> {
    fn error_at(&self, kind: XmlErrorKind, pos: usize) -> XmlError {
        let mut line = 1;
        let mut col = 0;
        for &c in &self.chars[..pos.min(self.chars.len())] {
            if c == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        XmlError {
            kind,
            line,
            column: col,
        }
    }

    fn line_at(&self, pos: usize) -> usize {
        1 + self.chars[..pos.min(self.chars.len())]
            .iter()
            .filter(|&&c| c == '\n')
            .count()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
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

    fn skip_space(&mut self) {
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
    }

    fn find(&self, s: &str) -> Option<usize> {
        let pat: Vec<char> = s.chars().collect();
        let mut i = self.pos;
        while i + pat.len() <= self.chars.len() {
            if self.chars[i..i + pat.len()] == pat[..] {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    fn slice(&self, a: usize, b: usize) -> String {
        self.chars[a..b].iter().collect()
    }

    fn read_name(&mut self) -> Result<String, XmlError> {
        let start = self.pos;
        match self.peek() {
            Some(c) if is_name_start(c) => self.pos += 1,
            Some(_) => return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos)),
            None => return Err(self.error_at(XmlErrorKind::NoElementFound, self.pos)),
        }
        while self.peek().is_some_and(is_name_char) {
            self.pos += 1;
        }
        Ok(self.slice(start, self.pos))
    }

    fn read_quoted(&mut self) -> Result<String, XmlError> {
        let quote = match self.peek() {
            Some(q @ ('"' | '\'')) => q,
            Some(_) => return Err(self.error_at(XmlErrorKind::Syntax, self.pos)),
            None => return Err(self.error_at(XmlErrorKind::UnclosedToken, self.pos)),
        };
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == quote {
                let s = self.slice(start, self.pos);
                self.pos += 1;
                return Ok(s);
            }
            self.pos += 1;
        }
        Err(self.error_at(XmlErrorKind::UnclosedToken, start))
    }

    fn parse_decl(&mut self) -> Result<(Option<String>, Option<String>, Option<bool>), XmlError> {
        // at "<?xml"
        let start = self.pos;
        let Some(end) = self.find("?>") else {
            return Err(self.error_at(XmlErrorKind::UnclosedToken, start));
        };
        let text = self.slice(start + 5, end);
        self.pos = end + 2;
        let version = decl_attr(&text, "version");
        let encoding = decl_attr(&text, "encoding");
        let standalone = decl_attr(&text, "standalone").map(|s| s == "yes");
        if version.is_none() {
            return Err(self.error_at(XmlErrorKind::Syntax, start));
        }
        Ok((version, encoding, standalone))
    }

    fn parse_comment(&mut self) -> Result<String, XmlError> {
        let start = self.pos;
        self.pos += 4;
        let Some(end) = self.find("--") else {
            return Err(self.error_at(XmlErrorKind::UnclosedToken, start));
        };
        if self.chars.get(end + 2) != Some(&'>') {
            return Err(self.error_at(XmlErrorKind::InvalidToken, end));
        }
        let text = self.slice(self.pos, end);
        self.pos = end + 3;
        Ok(text)
    }

    fn parse_pi(&mut self) -> Result<(String, String), XmlError> {
        let start = self.pos;
        self.pos += 2;
        let target = self.read_name()?;
        if target.eq_ignore_ascii_case("xml") {
            return Err(self.error_at(XmlErrorKind::Syntax, start));
        }
        let Some(end) = self.find("?>") else {
            return Err(self.error_at(XmlErrorKind::UnclosedToken, start));
        };
        let data = self.slice(self.pos, end);
        self.pos = end + 2;
        Ok((target, data.trim_start().to_string()))
    }

    fn parse_doctype(&mut self) -> Result<DocType, XmlError> {
        let start = self.pos;
        self.pos += 9; // <!DOCTYPE
        if !self.peek().is_some_and(is_space) {
            return Err(self.error_at(XmlErrorKind::Syntax, self.pos));
        }
        self.skip_space();
        let mut doctype = DocType {
            name: self.read_name()?,
            ..DocType::default()
        };
        self.skip_space();
        if self.starts_with("PUBLIC") {
            self.pos += 6;
            self.skip_space();
            doctype.public_id = Some(self.read_quoted()?);
            self.skip_space();
            if matches!(self.peek(), Some('"' | '\'')) {
                doctype.system_url = Some(self.read_quoted()?);
            }
        } else if self.starts_with("SYSTEM") {
            self.pos += 6;
            self.skip_space();
            doctype.system_url = Some(self.read_quoted()?);
        }
        self.skip_space();
        if self.peek() == Some('[') {
            doctype.has_internal_subset = true;
            self.pos += 1;
            self.parse_internal_subset(&mut doctype)?;
            self.skip_space();
        }
        if self.peek() != Some('>') {
            return Err(self.error_at(
                if self.peek().is_none() {
                    XmlErrorKind::UnclosedToken
                } else {
                    XmlErrorKind::Syntax
                },
                if self.peek().is_none() { start } else { self.pos },
            ));
        }
        self.pos += 1;
        Ok(doctype)
    }

    fn parse_internal_subset(&mut self, doctype: &mut DocType) -> Result<(), XmlError> {
        loop {
            self.skip_space();
            match self.peek() {
                None => return Err(self.error_at(XmlErrorKind::UnclosedToken, self.pos)),
                Some(']') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some('%') => {
                    // Parameter entity reference: skip.
                    while self.peek().is_some_and(|c| c != ';') {
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                Some('<') => {
                    if self.starts_with("<!--") {
                        self.parse_comment()?;
                    } else if self.starts_with("<?") {
                        self.parse_pi()?;
                    } else if self.starts_with("<!ENTITY") {
                        self.pos += 8;
                        self.skip_space();
                        let parameter = if self.peek() == Some('%') {
                            self.pos += 1;
                            self.skip_space();
                            true
                        } else {
                            false
                        };
                        let name = self.read_name()?;
                        if self.options.forbid_entities && !parameter {
                            return Err(self.error_at(XmlErrorKind::EntitiesForbidden(name), self.pos));
                        }
                        self.skip_space();
                        if matches!(self.peek(), Some('"' | '\'')) {
                            let value = self.read_quoted()?;
                            if !parameter {
                                doctype.entities.entry(name).or_insert(value);
                            }
                        } else {
                            let mut system = None;
                            if self.starts_with("PUBLIC") {
                                self.pos += 6;
                                self.skip_space();
                                self.read_quoted()?;
                                self.skip_space();
                                system = Some(self.read_quoted()?);
                            } else if self.starts_with("SYSTEM") {
                                self.pos += 6;
                                self.skip_space();
                                system = Some(self.read_quoted()?);
                            }
                            if !parameter && let Some(s) = system {
                                doctype.external_entities.entry(name).or_insert(s);
                            }
                        }
                        self.skip_to_decl_end()?;
                    } else if self.starts_with("<!ELEMENT") {
                        self.pos += 9;
                        self.skip_space();
                        let name = self.read_name()?;
                        self.skip_space();
                        let decl_start = self.pos;
                        self.skip_to_decl_end()?;
                        let spec_text = self.slice(decl_start, self.pos - 1);
                        let spec = parse_content_spec(spec_text.trim())
                            .ok_or_else(|| self.error_at(XmlErrorKind::Syntax, decl_start))?;
                        doctype.elements.entry(name).or_insert(spec);
                    } else if self.starts_with("<!ATTLIST") {
                        self.pos += 9;
                        self.skip_space();
                        let element = self.read_name()?;
                        let decl_start = self.pos;
                        self.skip_to_decl_end()?;
                        let text = self.slice(decl_start, self.pos - 1);
                        let decls = parse_attlist(&text);
                        doctype.attlists.entry(element).or_default().extend(decls);
                    } else if self.starts_with("<!NOTATION") {
                        self.skip_to_decl_end()?;
                    } else {
                        return Err(self.error_at(XmlErrorKind::Syntax, self.pos));
                    }
                }
                Some(_) => return Err(self.error_at(XmlErrorKind::Syntax, self.pos)),
            }
        }
    }

    fn skip_to_decl_end(&mut self) -> Result<(), XmlError> {
        let start = self.pos;
        let mut quote: Option<char> = None;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None => {
                    if c == '"' || c == '\'' {
                        quote = Some(c);
                    } else if c == '>' {
                        return Ok(());
                    }
                }
            }
        }
        Err(self.error_at(XmlErrorKind::UnclosedToken, start))
    }

    fn expand_entity(&self, name: &str, depth: usize, ref_pos: usize) -> Result<String, XmlError> {
        match name {
            "lt" => return Ok("<".to_string()),
            "gt" => return Ok(">".to_string()),
            "amp" => return Ok("&".to_string()),
            "apos" => return Ok("'".to_string()),
            "quot" => return Ok("\"".to_string()),
            _ => {}
        }
        if let Some(dt) = &self.doctype {
            if let Some(value) = dt.entities.get(name) {
                if !self.options.expand_internal_entities {
                    return Ok(String::new());
                }
                if depth > 16 {
                    return Err(self.error_at(XmlErrorKind::RecursiveEntity(name.to_string()), ref_pos));
                }
                return self.expand_text(value, depth + 1, ref_pos);
            }
            if dt.external_entities.contains_key(name) {
                // External entities are never loaded.
                return Ok(String::new());
            }
        }
        Err(self.error_at(XmlErrorKind::UndefinedEntity(name.to_string()), ref_pos))
    }

    fn expand_text(&self, text: &str, depth: usize, ref_pos: usize) -> Result<String, XmlError> {
        if !text.contains('&') {
            return Ok(text.to_string());
        }
        let mut out = String::new();
        let mut rest = text;
        while let Some(i) = rest.find('&') {
            out.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let Some(semi) = after.find(';') else {
                return Err(self.error_at(XmlErrorKind::InvalidToken, ref_pos));
            };
            let name = &after[..semi];
            out.push_str(&self.resolve_reference(name, depth, ref_pos)?);
            rest = &after[semi + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }

    fn resolve_reference(&self, name: &str, depth: usize, pos: usize) -> Result<String, XmlError> {
        if let Some(num) = name.strip_prefix('#') {
            let value = if let Some(hex) = num.strip_prefix('x') {
                u32::from_str_radix(hex, 16).ok()
            } else {
                num.parse::<u32>().ok()
            };
            return match value.and_then(char::from_u32) {
                Some(c) if c != '\0' => Ok(c.to_string()),
                _ => Err(self.error_at(XmlErrorKind::InvalidToken, pos)),
            };
        }
        if name.is_empty() || !name.chars().next().is_some_and(is_name_start) {
            return Err(self.error_at(XmlErrorKind::InvalidToken, pos));
        }
        self.expand_entity(name, depth, pos)
    }

    fn read_reference(&mut self) -> Result<String, XmlError> {
        let start = self.pos;
        self.pos += 1;
        let name_start = self.pos;
        while let Some(c) = self.peek() {
            if c == ';' {
                let name = self.slice(name_start, self.pos);
                self.pos += 1;
                return self.resolve_reference(&name, 0, start);
            }
            if !(is_name_char(c) || c == '#') {
                break;
            }
            self.pos += 1;
        }
        Err(self.error_at(XmlErrorKind::InvalidToken, start))
    }

    fn parse_attr_value(&mut self) -> Result<String, XmlError> {
        let quote = match self.peek() {
            Some(q @ ('"' | '\'')) => q,
            _ => return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos)),
        };
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error_at(XmlErrorKind::UnclosedToken, self.pos)),
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some('<') => return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos)),
                Some('&') => {
                    let text = self.read_reference()?;
                    out.push_str(&text);
                }
                Some(c) => {
                    out.push(if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c });
                    self.pos += 1;
                }
            }
        }
    }

    /// Parses `<name attr="..." ...>` or `<name .../>` at `self.pos`.
    fn parse_start_tag(&mut self) -> Result<(RawElement, bool), XmlError> {
        let start = self.pos;
        self.pos += 1;
        let name = self.read_name()?;
        let mut attrs: Vec<(String, String)> = Vec::new();
        loop {
            let had_space = self.peek().is_some_and(is_space);
            self.skip_space();
            match self.peek() {
                None => return Err(self.error_at(XmlErrorKind::UnclosedToken, start)),
                Some('>') => {
                    self.pos += 1;
                    return Ok((
                        RawElement {
                            name,
                            attrs,
                            children: Vec::new(),
                            line: self.line_at(start),
                        },
                        false,
                    ));
                }
                Some('/') => {
                    if self.chars.get(self.pos + 1) == Some(&'>') {
                        self.pos += 2;
                        return Ok((
                            RawElement {
                                name,
                                attrs,
                                children: Vec::new(),
                                line: self.line_at(start),
                            },
                            true,
                        ));
                    }
                    return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos));
                }
                Some(_) => {
                    if !had_space {
                        return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos));
                    }
                    let attr_pos = self.pos;
                    let attr = self.read_name()?;
                    self.skip_space();
                    if self.peek() != Some('=') {
                        return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos));
                    }
                    self.pos += 1;
                    self.skip_space();
                    let value = self.parse_attr_value()?;
                    if attrs.iter().any(|(a, _)| *a == attr) {
                        return Err(self.error_at(XmlErrorKind::DuplicateAttribute(attr), attr_pos));
                    }
                    attrs.push((attr, value));
                }
            }
        }
    }

    fn parse_element(&mut self) -> Result<RawElement, XmlError> {
        let start_pos = self.pos;
        let (mut element, empty) = self.parse_start_tag()?;
        if empty {
            return Ok(element);
        }
        let mut text = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error_at(XmlErrorKind::NoElementFound, self.pos)),
                Some('<') => {
                    if self.starts_with("</") {
                        flush_text(&mut text, &mut element.children);
                        let end_start = self.pos;
                        self.pos += 2;
                        let name = self.read_name()?;
                        self.skip_space();
                        if self.peek() != Some('>') {
                            return Err(self.error_at(
                                if self.peek().is_none() {
                                    XmlErrorKind::UnclosedToken
                                } else {
                                    XmlErrorKind::InvalidToken
                                },
                                self.pos,
                            ));
                        }
                        if name != element.name {
                            return Err(self.error_at(
                                XmlErrorKind::MismatchedTag {
                                    expected: element.name.clone(),
                                    found: name,
                                    start_line: self.line_at(start_pos),
                                },
                                end_start + 2,
                            ));
                        }
                        self.pos += 1;
                        return Ok(element);
                    } else if self.starts_with("<!--") {
                        let comment = self.parse_comment()?;
                        if self.options.keep_comments {
                            flush_text(&mut text, &mut element.children);
                            element.children.push(RawNode::Comment(comment));
                        }
                    } else if self.starts_with("<![CDATA[") {
                        let cstart = self.pos;
                        self.pos += 9;
                        let Some(end) = self.find("]]>") else {
                            return Err(self.error_at(XmlErrorKind::UnclosedToken, cstart));
                        };
                        text.push_str(&self.slice(self.pos, end));
                        self.pos = end + 3;
                    } else if self.starts_with("<?") {
                        let (target, data) = self.parse_pi()?;
                        if self.options.keep_pis {
                            flush_text(&mut text, &mut element.children);
                            element.children.push(RawNode::Pi(target, data));
                        }
                    } else if self.starts_with("<!") {
                        return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos));
                    } else {
                        flush_text(&mut text, &mut element.children);
                        let child = self.parse_element()?;
                        element.children.push(RawNode::Element(child));
                    }
                }
                Some('&') => {
                    let expanded = self.read_reference()?;
                    text.push_str(&expanded);
                }
                Some(c) => {
                    if c == ']' && self.starts_with("]]>") {
                        return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos));
                    }
                    if (c as u32) < 0x20 && !is_space(c) {
                        return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos));
                    }
                    // Normalise CRLF/CR to LF as XML parsers do.
                    if c == '\r' {
                        text.push('\n');
                        self.pos += 1;
                        if self.peek() == Some('\n') {
                            self.pos += 1;
                        }
                        continue;
                    }
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn parse_document(&mut self) -> Result<XmlDocument, XmlError> {
        let mut xml_version = None;
        let mut encoding = None;
        let mut standalone = None;
        if self.peek() == Some('\u{feff}') {
            self.pos += 1;
        }
        if self.starts_with("<?xml")
            && self.chars.get(self.pos + 5).is_some_and(|&c| is_space(c) || c == '?')
        {
            let (v, e, s) = self.parse_decl()?;
            xml_version = v;
            encoding = e;
            standalone = s;
        }
        let mut root: Option<RawElement> = None;
        loop {
            self.skip_space();
            match self.peek() {
                None => {
                    if root.is_none() {
                        return Err(self.error_at(XmlErrorKind::NoElementFound, self.pos));
                    }
                    break;
                }
                Some('<') => {
                    if self.starts_with("<!--") {
                        self.parse_comment()?;
                    } else if self.starts_with("<?") {
                        self.parse_pi()?;
                    } else if self.starts_with("<!DOCTYPE") {
                        if root.is_some() || self.doctype.is_some() {
                            return Err(self.error_at(XmlErrorKind::Syntax, self.pos));
                        }
                        if self.options.forbid_dtd {
                            return Err(self.error_at(XmlErrorKind::DtdForbidden, self.pos));
                        }
                        let dt = self.parse_doctype()?;
                        self.doctype = Some(dt);
                    } else if root.is_some() {
                        return Err(self.error_at(XmlErrorKind::JunkAfterDocument, self.pos));
                    } else if self.chars.get(self.pos + 1).is_some_and(|&c| is_name_start(c)) {
                        root = Some(self.parse_element()?);
                    } else {
                        return Err(self.error_at(XmlErrorKind::InvalidToken, self.pos + 1));
                    }
                }
                Some(_) => {
                    if root.is_some() {
                        return Err(self.error_at(XmlErrorKind::JunkAfterDocument, self.pos));
                    }
                    return Err(self.error_at(XmlErrorKind::Syntax, self.pos));
                }
            }
        }
        let raw_root = root.expect("root parsed");
        let mut scope: Vec<IndexMap<Option<String>, String>> = vec![IndexMap::new()];
        let root = self.resolve(raw_root, &mut scope)?;
        Ok(XmlDocument {
            xml_version,
            encoding,
            standalone,
            doctype: self.doctype.take(),
            root,
        })
    }

    fn resolve(
        &self,
        raw: RawElement,
        scope: &mut Vec<IndexMap<Option<String>, String>>,
    ) -> Result<XmlElement, XmlError> {
        let mut nsmap = scope.last().cloned().unwrap_or_default();
        let mut ns_decls = Vec::new();
        let mut plain_attrs = Vec::new();
        for (name, value) in raw.attrs {
            if name == "xmlns" {
                if value.is_empty() {
                    nsmap.shift_remove(&None);
                } else {
                    nsmap.insert(None, value.clone());
                }
                ns_decls.push((None, value));
            } else if let Some(prefix) = name.strip_prefix("xmlns:") {
                nsmap.insert(Some(prefix.to_string()), value.clone());
                ns_decls.push((Some(prefix.to_string()), value));
            } else {
                plain_attrs.push((name, value));
            }
        }
        let lookup = |prefix: &str, nsmap: &IndexMap<Option<String>, String>| -> Option<String> {
            if prefix == "xml" {
                return Some(XML_NAMESPACE.to_string());
            }
            if prefix == "xmlns" {
                return Some(XMLNS_NAMESPACE.to_string());
            }
            nsmap.get(&Some(prefix.to_string())).cloned()
        };
        let (prefix, local) = split_qname(&raw.name);
        let namespace = match &prefix {
            Some(p) => Some(lookup(p, &nsmap).ok_or_else(|| XmlError {
                kind: XmlErrorKind::UnboundPrefix(p.clone()),
                line: raw.line,
                column: 0,
            })?),
            None => nsmap.get(&None).cloned(),
        };
        let mut attributes = Vec::new();
        for (name, value) in plain_attrs {
            let (aprefix, alocal) = split_qname(&name);
            let anamespace = match &aprefix {
                Some(p) => Some(lookup(p, &nsmap).ok_or_else(|| XmlError {
                    kind: XmlErrorKind::UnboundPrefix(p.clone()),
                    line: raw.line,
                    column: 0,
                })?),
                None => None,
            };
            attributes.push(XmlAttribute {
                prefix: aprefix,
                local: alocal,
                namespace: anamespace,
                value,
            });
        }
        // Default attribute values from the DTD.
        if let Some(dt) = &self.doctype
            && let Some(decls) = dt.attlists.get(&raw.name)
        {
            for decl in decls {
                if let Some(default) = &decl.default_value
                    && !attributes.iter().any(|a| a.local == decl.name && a.prefix.is_none())
                    && !decl.name.contains(':')
                {
                    attributes.push(XmlAttribute {
                        prefix: None,
                        local: decl.name.clone(),
                        namespace: None,
                        value: default.clone(),
                    });
                }
            }
        }
        scope.push(nsmap.clone());
        let has_element_children = raw
            .children
            .iter()
            .any(|c| matches!(c, RawNode::Element(_)));
        let mut children = Vec::new();
        for child in raw.children {
            match child {
                RawNode::Element(e) => children.push(XmlNode::Element(self.resolve(e, scope)?)),
                RawNode::Text(t) => {
                    if self.options.remove_blank_text
                        && has_element_children
                        && t.chars().all(is_space)
                    {
                        continue;
                    }
                    children.push(XmlNode::Text(t));
                }
                RawNode::Comment(c) => children.push(XmlNode::Comment(c)),
                RawNode::Pi(t, d) => children.push(XmlNode::Pi(t, d)),
            }
        }
        scope.pop();
        Ok(XmlElement {
            prefix,
            local,
            namespace,
            attributes,
            ns_decls,
            nsmap,
            children,
            line: raw.line,
        })
    }
}

fn flush_text(text: &mut String, children: &mut Vec<RawNode>) {
    if text.is_empty() {
        return;
    }
    if let Some(RawNode::Text(prev)) = children.last_mut() {
        prev.push_str(text);
    } else {
        children.push(RawNode::Text(std::mem::take(text)));
    }
    text.clear();
}

fn split_qname(name: &str) -> (Option<String>, String) {
    match name.split_once(':') {
        Some((p, l)) if !p.is_empty() && !l.is_empty() => (Some(p.to_string()), l.to_string()),
        _ => (None, name.to_string()),
    }
}

fn parse_occurrence(s: &str, pos: &mut usize) -> Occurrence {
    let occ = match s[*pos..].chars().next() {
        Some('?') => Occurrence::Optional,
        Some('*') => Occurrence::ZeroOrMore,
        Some('+') => Occurrence::OneOrMore,
        _ => return Occurrence::One,
    };
    *pos += 1;
    occ
}

fn parse_content_spec(text: &str) -> Option<ContentSpec> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact == "EMPTY" {
        return Some(ContentSpec::Empty);
    }
    if compact == "ANY" {
        return Some(ContentSpec::Any);
    }
    if compact.starts_with("(#PCDATA") {
        let inner = compact
            .trim_end_matches('*')
            .trim_start_matches('(')
            .trim_end_matches(')');
        let names: Vec<String> = inner
            .split('|')
            .skip(1)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
            .collect();
        return Some(ContentSpec::Mixed(names));
    }
    let mut pos = 0;
    let particle = parse_particle(&compact, &mut pos)?;
    if pos != compact.len() {
        return None;
    }
    Some(ContentSpec::Children(particle))
}

fn parse_particle(s: &str, pos: &mut usize) -> Option<ContentParticle> {
    if s[*pos..].starts_with('(') {
        *pos += 1;
        let mut items = Vec::new();
        let mut sep: Option<char> = None;
        loop {
            items.push(parse_particle(s, pos)?);
            match s[*pos..].chars().next()? {
                ')' => {
                    *pos += 1;
                    break;
                }
                c @ (',' | '|') => {
                    if sep.is_some_and(|x| x != c) {
                        return None;
                    }
                    sep = Some(c);
                    *pos += 1;
                }
                _ => return None,
            }
        }
        let occ = parse_occurrence(s, pos);
        Some(match sep {
            Some('|') => ContentParticle::Choice(items, occ),
            _ => ContentParticle::Seq(items, occ),
        })
    } else {
        let start = *pos;
        while let Some(c) = s[*pos..].chars().next() {
            if is_name_char(c) {
                *pos += c.len_utf8();
            } else {
                break;
            }
        }
        if *pos == start {
            return None;
        }
        let name = s[start..*pos].to_string();
        let occ = parse_occurrence(s, pos);
        Some(ContentParticle::Name(name, occ))
    }
}

fn parse_attlist(text: &str) -> Vec<AttributeDecl> {
    let mut tokens: Vec<String> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' || c == '\'' {
            let start = i + 1;
            i += 1;
            while i < chars.len() && chars[i] != c {
                i += 1;
            }
            tokens.push(format!("\"{}", chars[start..i.min(chars.len())].iter().collect::<String>()));
            i += 1;
        } else if c == '(' {
            let start = i;
            while i < chars.len() && chars[i] != ')' {
                i += 1;
            }
            i += 1;
            tokens.push(chars[start..i.min(chars.len())].iter().filter(|c| !c.is_whitespace()).collect());
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        }
    }
    let mut decls = Vec::new();
    let mut t = 0;
    while t + 1 < tokens.len() {
        let name = tokens[t].clone();
        let kind = tokens[t + 1].clone();
        t += 2;
        if kind == "NOTATION" && t < tokens.len() {
            t += 1;
        }
        let mut default_kind = String::new();
        let mut default_value = None;
        if t < tokens.len() {
            let tok = tokens[t].clone();
            if tok == "#REQUIRED" || tok == "#IMPLIED" {
                default_kind = tok;
                t += 1;
            } else if tok == "#FIXED" {
                default_kind = tok;
                t += 1;
                if t < tokens.len() {
                    default_value = Some(tokens[t].trim_start_matches('"').to_string());
                    t += 1;
                }
            } else if let Some(v) = tok.strip_prefix('"') {
                default_value = Some(v.to_string());
                t += 1;
            }
        }
        decls.push(AttributeDecl {
            name,
            kind,
            default_kind,
            default_value,
        });
    }
    decls
}

/// Parses a document from text.
pub fn parse_str(text: &str, options: &ParseOptions) -> Result<XmlDocument, XmlError> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        pos: 0,
        options,
        doctype: None,
    };
    parser.parse_document()
}

/// Parses a document from bytes (BOM / declared encoding honoured).
pub fn parse_bytes(bytes: &[u8], options: &ParseOptions) -> Result<XmlDocument, XmlError> {
    parse_str(&decode_xml_bytes(bytes), options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_and_text() {
        let doc = parse_str(
            r#"<?xml version="1.0"?><catalog xmlns="urn:c" xmlns:bk="urn:b"><bk:book id="b1" xml:lang="en">x &amp; y</bk:book></catalog>"#,
            &ParseOptions::default(),
        )
        .unwrap();
        assert_eq!(doc.root.clark_name(), "{urn:c}catalog");
        let book = doc.root.element_children().next().unwrap();
        assert_eq!(book.clark_name(), "{urn:b}book");
        assert_eq!(book.attributes[1].clark_name(), format!("{{{XML_NAMESPACE}}}lang"));
        assert_eq!(book.children[0], XmlNode::Text("x & y".to_string()));
    }

    #[test]
    fn errors() {
        let e = parse_str("NOT XML", &ParseOptions::default()).unwrap_err();
        assert_eq!(e.expat_message(), "syntax error: line 1, column 0");
        let e = parse_str("<root><item></root>", &ParseOptions::default()).unwrap_err();
        assert!(matches!(e.kind, XmlErrorKind::MismatchedTag { .. }));
        let e = parse_str("", &ParseOptions::default()).unwrap_err();
        assert_eq!(e.expat_message(), "no element found: line 1, column 0");
    }

    #[test]
    fn doctype_entities() {
        let text = "<!DOCTYPE note [\n<!ELEMENT note (to,from)>\n<!ELEMENT to (#PCDATA)>\n<!ENTITY who \"Ada\">\n]>\n<note><to>&who;</to><from>Grace</from></note>";
        let doc = parse_str(text, &ParseOptions::default()).unwrap();
        let dt = doc.doctype.unwrap();
        assert_eq!(dt.name, "note");
        assert!(matches!(dt.elements.get("note"), Some(ContentSpec::Children(_))));
        let to = doc.root.element_children().next().unwrap();
        assert_eq!(to.children[0], XmlNode::Text("Ada".to_string()));
        let forbid = ParseOptions {
            forbid_entities: true,
            ..ParseOptions::default()
        };
        assert!(matches!(
            parse_str(text, &forbid).unwrap_err().kind,
            XmlErrorKind::EntitiesForbidden(_)
        ));
    }
}
