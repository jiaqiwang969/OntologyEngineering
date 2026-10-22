//! `BeautifulSoup(markup, "html.parser")` for the operations the ingestors use.
//!
//! The tokenizer is a port of CPython 3.11.14's `html.parser.HTMLParser` (run with
//! `convert_charrefs=False`, as bs4 does) and the tree construction follows bs4 4.15's
//! `HTMLParserTreeBuilder` + `BeautifulSoup` (`handle_starttag`/`_popToTag`, empty-element
//! tags, whitespace-only string collapsing, string containers for `<script>`/`<style>`/
//! `<template>`/`<rt>`/`<rp>`). Queries mirror `find_all`, `get_text`, `decompose` and
//! `Tag.get`.

use std::collections::HashMap;

use fancy_regex::Regex as FancyRegex;
use once_cell::sync::Lazy;
use regex::Regex;

/// Index of a node in a [`Document`].
pub type NodeId = usize;

/// The class of a string node (bs4's `NavigableString` subclasses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringKind {
    /// `NavigableString`.
    Navigable,
    /// `Comment`.
    Comment,
    /// `CData`.
    CData,
    /// `Doctype`.
    Doctype,
    /// `Declaration`.
    Declaration,
    /// `ProcessingInstruction`.
    ProcessingInstruction,
    /// `Script` (strings inside `<script>`).
    Script,
    /// `Stylesheet` (strings inside `<style>`).
    Stylesheet,
    /// `TemplateString` (strings inside `<template>`).
    TemplateString,
    /// `RubyTextString` (strings inside `<rt>`).
    RubyText,
    /// `RubyParenthesisString` (strings inside `<rp>`).
    RubyParenthesis,
}

/// What a node is.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// The `BeautifulSoup` object itself (`[document]`).
    Root,
    /// A tag with its attributes in source order.
    Element {
        /// Lower-cased tag name.
        name: String,
        /// Attributes (lower-cased names, entity-decoded values).
        attrs: Vec<(String, String)>,
    },
    /// A string.
    Text {
        /// The text.
        text: String,
        /// Its class.
        kind: StringKind,
    },
}

#[derive(Debug, Clone)]
struct Node {
    kind: NodeKind,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

/// A parsed HTML document.
#[derive(Debug, Clone)]
pub struct Document {
    nodes: Vec<Node>,
}

const EMPTY_ELEMENT_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "menuitem",
    "meta", "param", "source", "track", "wbr", "basefont", "bgsound", "command", "frame", "image",
    "isindex", "nextid", "spacer",
];

const PRESERVE_WHITESPACE_TAGS: &[&str] = &["pre", "textarea"];

fn string_container(tag: &str) -> Option<StringKind> {
    match tag {
        "rt" => Some(StringKind::RubyText),
        "rp" => Some(StringKind::RubyParenthesis),
        "style" => Some(StringKind::Stylesheet),
        "script" => Some(StringKind::Script),
        "template" => Some(StringKind::TemplateString),
        _ => None,
    }
}

const ASCII_SPACES: &[char] = &['\x20', '\x0a', '\x09', '\x0c', '\x0d'];

/// Tree construction state (bs4's `BeautifulSoup` object during parsing).
struct TreeBuilder {
    doc: Document,
    tag_stack: Vec<NodeId>,
    open_tag_counter: HashMap<String, usize>,
    preserve_whitespace_stack: Vec<NodeId>,
    string_container_stack: Vec<NodeId>,
    current_data: Vec<String>,
    already_closed_empty_element: Vec<String>,
}

impl TreeBuilder {
    fn new() -> Self {
        TreeBuilder {
            doc: Document {
                nodes: vec![Node {
                    kind: NodeKind::Root,
                    parent: None,
                    children: Vec::new(),
                }],
            },
            tag_stack: vec![0],
            open_tag_counter: HashMap::new(),
            preserve_whitespace_stack: Vec::new(),
            string_container_stack: Vec::new(),
            current_data: Vec::new(),
            already_closed_empty_element: Vec::new(),
        }
    }

    fn current(&self) -> NodeId {
        *self.tag_stack.last().unwrap_or(&0)
    }

    fn push_node(&mut self, kind: NodeKind) -> NodeId {
        let parent = self.current();
        let id = self.doc.nodes.len();
        self.doc.nodes.push(Node {
            kind,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.doc.nodes[parent].children.push(id);
        id
    }

    fn end_data(&mut self, container: Option<StringKind>) {
        if self.current_data.is_empty() {
            return;
        }
        let mut data = self.current_data.concat();
        self.current_data.clear();
        if self.preserve_whitespace_stack.is_empty() && data.chars().all(|c| ASCII_SPACES.contains(&c)) {
            data = if data.contains('\n') {
                "\n".to_string()
            } else {
                " ".to_string()
            };
        }
        let mut kind = container.unwrap_or(StringKind::Navigable);
        if kind == StringKind::Navigable
            && let Some(&top) = self.string_container_stack.last()
            && let Some(name) = self.doc.tag_name(top)
            && let Some(k) = string_container(name)
        {
            kind = k;
        }
        self.push_node(NodeKind::Text { text: data, kind });
    }

    fn handle_starttag(&mut self, name: &str, attrs: Vec<(String, String)>) -> NodeId {
        self.end_data(None);
        let id = self.push_node(NodeKind::Element {
            name: name.to_string(),
            attrs,
        });
        self.tag_stack.push(id);
        *self.open_tag_counter.entry(name.to_string()).or_insert(0) += 1;
        if PRESERVE_WHITESPACE_TAGS.contains(&name) {
            self.preserve_whitespace_stack.push(id);
        }
        if string_container(name).is_some() {
            self.string_container_stack.push(id);
        }
        id
    }

    fn pop_tag(&mut self) {
        if self.tag_stack.len() <= 1 {
            return;
        }
        let tag = self.tag_stack.pop().unwrap_or(0);
        if let Some(name) = self.doc.tag_name(tag).map(str::to_string)
            && let Some(count) = self.open_tag_counter.get_mut(&name)
        {
            *count = count.saturating_sub(1);
        }
        if self.preserve_whitespace_stack.last() == Some(&tag) {
            self.preserve_whitespace_stack.pop();
        }
        if self.string_container_stack.last() == Some(&tag) {
            self.string_container_stack.pop();
        }
    }

    fn pop_to_tag(&mut self, name: &str) {
        let stack_size = self.tag_stack.len();
        for i in (1..stack_size).rev() {
            if self.open_tag_counter.get(name).copied().unwrap_or(0) == 0 {
                break;
            }
            let t = self.tag_stack[i];
            if self.doc.tag_name(t) == Some(name) {
                self.pop_tag();
                break;
            }
            self.pop_tag();
        }
    }

    fn handle_endtag(&mut self, name: &str) {
        self.end_data(None);
        self.pop_to_tag(name);
    }

    // BeautifulSoupHTMLParser callbacks.

    fn on_starttag(&mut self, tag: &str, attrs: Vec<(String, Option<String>)>, handle_empty_element: bool) {
        let mut attr_list: Vec<(String, String)> = Vec::new();
        for (key, value) in attrs {
            let value = value.unwrap_or_default();
            if let Some(existing) = attr_list.iter_mut().find(|(k, _)| *k == key) {
                existing.1 = value;
            } else {
                attr_list.push((key, value));
            }
        }
        let id = self.handle_starttag(tag, attr_list);
        let is_empty = self.doc.nodes[id].children.is_empty() && EMPTY_ELEMENT_TAGS.contains(&tag);
        if is_empty && handle_empty_element {
            self.on_endtag(tag, false);
            self.already_closed_empty_element.push(tag.to_string());
        }
    }

    fn on_endtag(&mut self, tag: &str, check_already_closed: bool) {
        if check_already_closed
            && let Some(pos) = self.already_closed_empty_element.iter().position(|t| t == tag)
        {
            self.already_closed_empty_element.remove(pos);
        } else {
            self.handle_endtag(tag);
        }
    }

    fn on_startendtag(&mut self, tag: &str, attrs: Vec<(String, Option<String>)>) {
        self.on_starttag(tag, attrs, false);
        self.on_endtag(tag, false);
    }

    fn on_data(&mut self, data: &str) {
        self.current_data.push(data.to_string());
    }

    fn on_charref(&mut self, name: &str) {
        let (dereferenced, extra) = dereference_numeric(name);
        self.on_data(&dereferenced);
        self.on_data(&extra);
    }

    fn on_entityref(&mut self, name: &str) {
        match html5_entity(name) {
            Some(c) => self.on_data(&c),
            None => self.on_data(&format!("&{name}")),
        }
    }

    fn on_special(&mut self, data: &str, kind: StringKind) {
        self.end_data(None);
        self.on_data(data);
        self.end_data(Some(kind));
    }

    fn finish(mut self) -> Document {
        self.end_data(None);
        while self.tag_stack.len() > 1 {
            self.pop_tag();
        }
        self.doc
    }
}

/// Looks a named reference up in the HTML5 table (`name` without the trailing `;`).
fn html5_entity(name: &str) -> Option<String> {
    let lookup = |key: &str| {
        html5ever::data::NAMED_ENTITIES.get(key).map(|&(a, b)| {
            let mut s = String::new();
            if let Some(c) = char::from_u32(a) {
                s.push(c);
            }
            if b != 0
                && let Some(c) = char::from_u32(b)
            {
                s.push(c);
            }
            s
        })
    };
    lookup(&format!("{name};")).or_else(|| lookup(name))
}

/// The `_invalid_charrefs` table of `html/__init__.py` (and `UnicodeDammit`'s Windows-1252
/// replacements for 0x80-0x9f).
fn invalid_charref(num: u32) -> Option<char> {
    let c = match num {
        0x00 => '\u{fffd}',
        0x0d => '\r',
        0x80 => '\u{20ac}',
        0x81 => '\u{81}',
        0x82 => '\u{201a}',
        0x83 => '\u{0192}',
        0x84 => '\u{201e}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02c6}',
        0x89 => '\u{2030}',
        0x8a => '\u{0160}',
        0x8b => '\u{2039}',
        0x8c => '\u{0152}',
        0x8d => '\u{8d}',
        0x8e => '\u{017d}',
        0x8f => '\u{8f}',
        0x90 => '\u{90}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201c}',
        0x94 => '\u{201d}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02dc}',
        0x99 => '\u{2122}',
        0x9a => '\u{0161}',
        0x9b => '\u{203a}',
        0x9c => '\u{0153}',
        0x9d => '\u{9d}',
        0x9e => '\u{017e}',
        0x9f => '\u{0178}',
        _ => return None,
    };
    Some(c)
}

fn dereference_numeric(name: &str) -> (String, String) {
    let (digits, base) = if let Some(rest) = name.strip_prefix(['x', 'X']) {
        (rest, 16)
    } else {
        (name, 10)
    };
    let valid: String = digits
        .chars()
        .take_while(|c| if base == 16 { c.is_ascii_hexdigit() } else { c.is_ascii_digit() })
        .collect();
    if valid.is_empty() {
        return (String::new(), name.to_string());
    }
    let extra = digits[valid.len()..].to_string();
    let num = u32::from_str_radix(&valid, base).unwrap_or(0xfffd);
    let c = if (128..160).contains(&num) {
        invalid_charref(num).unwrap_or('\u{fffd}')
    } else {
        char::from_u32(num).unwrap_or('\u{fffd}')
    };
    (c.to_string(), extra)
}

/// `html.unescape(s)` (CPython 3.11).
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    static CHARREF: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"&(#[0-9]+;?|#[xX][0-9a-fA-F]+;?|[^\t\n\x0c <&#;]{1,32};?)").unwrap()
    });
    CHARREF
        .replace_all(s, |caps: &regex::Captures<'_>| {
            let m = &caps[1];
            if let Some(num) = m.strip_prefix('#') {
                let num = num.trim_end_matches(';');
                let value = if let Some(hex) = num.strip_prefix(['x', 'X']) {
                    u64::from_str_radix(hex, 16).unwrap_or(u64::MAX)
                } else {
                    num.parse::<u64>().unwrap_or(u64::MAX)
                };
                if let Some(c) = u32::try_from(value).ok().and_then(invalid_charref) {
                    return c.to_string();
                }
                if (0xD800..=0xDFFF).contains(&value) || value > 0x10FFFF {
                    return '\u{fffd}'.to_string();
                }
                let invalid = matches!(value, 0x1..=0x8 | 0xe..=0x1f | 0x7f..=0x9f | 0xfdd0..=0xfdef | 0xb | 0xfffe | 0xffff)
                    || (value & 0xfffe) == 0xfffe;
                if invalid {
                    return String::new();
                }
                char::from_u32(value as u32)
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            } else {
                let lookup = |key: &str| {
                    html5ever::data::NAMED_ENTITIES.get(key).map(|&(a, b)| {
                        let mut out = String::new();
                        if let Some(c) = char::from_u32(a) {
                            out.push(c);
                        }
                        if b != 0
                            && let Some(c) = char::from_u32(b)
                        {
                            out.push(c);
                        }
                        out
                    })
                };
                if let Some(v) = lookup(m) {
                    return v;
                }
                let chars: Vec<(usize, char)> = m.char_indices().collect();
                for x in (2..chars.len()).rev() {
                    let prefix = &m[..chars[x].0];
                    if let Some(v) = lookup(prefix) {
                        return format!("{v}{}", &m[chars[x].0..]);
                    }
                }
                format!("&{m}")
            }
        })
        .into_owned()
}

static ENTITYREF: Lazy<FancyRegex> =
    Lazy::new(|| FancyRegex::new(r"&([a-zA-Z][-.a-zA-Z0-9]*)[^a-zA-Z0-9]").unwrap());
static CHARREF_RE: Lazy<FancyRegex> =
    Lazy::new(|| FancyRegex::new(r"&#(?:[0-9]+|[xX][0-9a-fA-F]+)[^0-9a-fA-F]").unwrap());
static INCOMPLETE: Lazy<FancyRegex> = Lazy::new(|| FancyRegex::new(r"&[a-zA-Z#]").unwrap());
static INTERESTING_NORMAL: Lazy<FancyRegex> = Lazy::new(|| FancyRegex::new(r"[&<]").unwrap());
static STARTTAGOPEN: Lazy<FancyRegex> = Lazy::new(|| FancyRegex::new(r"<[a-zA-Z]").unwrap());
static ENDTAGOPEN: Lazy<FancyRegex> = Lazy::new(|| FancyRegex::new(r"</[a-zA-Z]").unwrap());
static COMMENTCLOSE: Lazy<FancyRegex> = Lazy::new(|| FancyRegex::new(r"--!?>").unwrap());
static COMMENTABRUPTCLOSE: Lazy<FancyRegex> = Lazy::new(|| FancyRegex::new(r"-?>").unwrap());
static TAGFIND_TOLERANT: Lazy<FancyRegex> = Lazy::new(|| {
    FancyRegex::new(r"([a-zA-Z][^\t\n\r\x0c />]*)(?:[\t\n\r\x0c ]|/(?!>))*").unwrap()
});
static ATTRFIND_TOLERANT: Lazy<FancyRegex> = Lazy::new(|| {
    FancyRegex::new(concat!(
        r#"((?<=['"\t\n\r\x0c /])[^\t\n\r\x0c />][^\t\n\r\x0c /=>]*)"#,
        r#"([\t\n\r\x0c ]*=[\t\n\r\x0c ]*('[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\x0c ]*))?"#,
        r#"(?:[\t\n\r\x0c ]|/(?!>))*"#
    ))
    .unwrap()
});
static LOCATETAGEND: Lazy<FancyRegex> = Lazy::new(|| {
    FancyRegex::new(concat!(
        r#"[a-zA-Z][^\t\n\r\x0c />]*"#,
        r#"[\t\n\r\x0c /]*"#,
        r#"(?:(?<=['"\t\n\r\x0c /])[^\t\n\r\x0c />][^\t\n\r\x0c /=>]*"#,
        r#"(?:[\t\n\r\x0c ]*=[\t\n\r\x0c ]*(?:'[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\x0c ]*))?"#,
        r#"[\t\n\r\x0c /]*)*"#,
        r#">?"#
    ))
    .unwrap()
});

/// A port of `html.parser.HTMLParser.goahead` driving a [`TreeBuilder`].
struct Tokenizer<'a> {
    raw: &'a str,
    builder: &'a mut TreeBuilder,
    cdata_elem: Option<String>,
    interesting: FancyRegex,
    escapable: bool,
}

fn match_at(re: &FancyRegex, text: &str, pos: usize) -> Option<(usize, usize)> {
    // `re.match(text, pos)`: anchored at pos, with look-behind seeing text before pos.
    // The leftmost match from `pos` starts at `pos` exactly when an anchored match exists.
    match re.find_from_pos(text, pos) {
        Ok(Some(m)) if m.start() == pos => Some((m.start(), m.end())),
        _ => None,
    }
}


impl<'a> Tokenizer<'a> {
    fn search(&self, re: &FancyRegex, pos: usize) -> Option<(usize, usize)> {
        re.find_from_pos(self.raw, pos)
            .ok()
            .flatten()
            .map(|m| (m.start(), m.end()))
    }

    fn set_cdata_mode(&mut self, elem: &str, escapable: bool) {
        self.cdata_elem = Some(elem.to_lowercase());
        self.escapable = escapable;
        let pattern = if escapable {
            format!(r"(?i)&|</{}(?=[\t\n\r\x0c />])", regex::escape(elem))
        } else {
            format!(r"(?i)</{}(?=[\t\n\r\x0c />])", regex::escape(elem))
        };
        self.interesting = FancyRegex::new(&pattern).unwrap();
    }

    fn clear_cdata_mode(&mut self) {
        self.interesting = INTERESTING_NORMAL.clone();
        self.cdata_elem = None;
        self.escapable = true;
    }

    fn starts(&self, i: usize, prefix: &str) -> bool {
        self.raw[i..].starts_with(prefix)
    }

    /// Runs the tokenizer over all input (`feed(markup)` followed by `close()`).
    fn run(&mut self) {
        let mut i = self.goahead(0, false);
        i = self.goahead(i, true);
        let _ = i;
    }

    fn goahead(&mut self, start: usize, end: bool) -> usize {
        let raw = self.raw;
        let n = raw.len();
        let mut i = start;
        while i < n {
            let j = match self.search(&self.interesting.clone(), i) {
                Some((s, _)) => s,
                None => {
                    if self.cdata_elem.is_some() {
                        break;
                    }
                    n
                }
            };
            if i < j {
                self.builder.on_data(&raw[i..j]);
            }
            i = j;
            if i == n {
                break;
            }
            if self.starts(i, "<") {
                let k: isize;
                if match_at(&STARTTAGOPEN, raw, i).is_some() {
                    k = self.parse_starttag(i);
                } else if self.starts(i, "</") {
                    k = self.parse_endtag(i);
                } else if self.starts(i, "<!--") {
                    k = self.parse_comment(i);
                } else if self.starts(i, "<?") {
                    k = self.parse_pi(i);
                } else if self.starts(i, "<!") {
                    k = self.parse_html_declaration(i);
                } else if i + 1 < n || end {
                    self.builder.on_data("<");
                    k = (i + 1) as isize;
                } else {
                    break;
                }
                let k = if k < 0 {
                    if !end {
                        break;
                    }
                    if match_at(&STARTTAGOPEN, raw, i).is_some() {
                        // incomplete start tag: dropped
                    } else if self.starts(i, "</") {
                        if i + 2 == n {
                            self.builder.on_data("</");
                        } else if match_at(&ENDTAGOPEN, raw, i).is_some() {
                        } else {
                            self.builder.on_special(&raw[i + 2..], StringKind::Comment);
                        }
                    } else if self.starts(i, "<!--") {
                        let mut j = n;
                        for suffix in ["--!", "--", "-"] {
                            if raw[i + 4..].ends_with(suffix) {
                                j -= suffix.len();
                                break;
                            }
                        }
                        let content = if j >= i + 4 { &raw[i + 4..j] } else { "" };
                        self.builder.on_special(content, StringKind::Comment);
                    } else if self.starts(i, "<![CDATA[") {
                        self.unknown_decl(&raw[i + 3..]);
                    } else if raw[i..].len() >= 9 && raw[i..i + 9].eq_ignore_ascii_case("<!doctype") {
                        self.handle_decl(&raw[i + 2..]);
                    } else if self.starts(i, "<!") {
                        self.builder.on_special(&raw[i + 2..], StringKind::Comment);
                    } else if self.starts(i, "<?") {
                        self.builder.on_special(&raw[i + 2..], StringKind::ProcessingInstruction);
                    }
                    n
                } else {
                    k as usize
                };
                i = k;
            } else if self.starts(i, "&#") {
                if let Some((_, mend)) = match_at(&CHARREF_RE, raw, i) {
                    let name = &raw[i + 2..mend - 1];
                    self.builder.on_charref(name);
                    let mut k = mend;
                    if !raw[..k].ends_with(';') {
                        k -= 1;
                    }
                    i = k;
                    continue;
                } else {
                    if raw[i..].contains(';') {
                        self.builder.on_data(&raw[i..i + 2]);
                        i += 2;
                    }
                    break;
                }
            } else if self.starts(i, "&") {
                if let Some((_, mend)) = match_at(&ENTITYREF, raw, i) {
                    let caps = ENTITYREF.captures_from_pos(raw, i).ok().flatten();
                    let name = caps
                        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                        .unwrap_or_default();
                    self.builder.on_entityref(&name);
                    let mut k = mend;
                    if !raw[..k].ends_with(';') {
                        k -= 1;
                    }
                    i = k;
                    continue;
                }
                if let Some((_, mend)) = match_at(&INCOMPLETE, raw, i) {
                    if end && mend == n {
                        // `match.group() == rawdata[i:]`
                        i += 1;
                        // The remainder is emitted as data below.
                        let _ = mend;
                    }
                    break;
                } else if i + 1 < n {
                    self.builder.on_data("&");
                    i += 1;
                } else {
                    break;
                }
            } else {
                // interesting matched something else (cannot happen)
                break;
            }
        }
        if end && i < n {
            self.builder.on_data(&raw[i..n]);
            i = n;
        }
        i
    }

    fn handle_decl(&mut self, decl: &str) {
        let decl = decl.get(8..).unwrap_or("");
        self.builder.on_special(decl, StringKind::Doctype);
    }

    fn unknown_decl(&mut self, data: &str) {
        if data.len() >= 6 && data[..6].eq_ignore_ascii_case("CDATA[") {
            self.builder.on_special(&data[6..], StringKind::CData);
        } else {
            self.builder.on_special(data, StringKind::Declaration);
        }
    }

    fn parse_html_declaration(&mut self, i: usize) -> isize {
        let raw = self.raw;
        if self.starts(i, "<!--") {
            return self.parse_comment(i);
        }
        if self.starts(i, "<![CDATA[") {
            return match raw[i + 9..].find("]]>") {
                Some(j) => {
                    let j = i + 9 + j;
                    self.unknown_decl(&raw[i + 3..j]);
                    (j + 3) as isize
                }
                None => -1,
            };
        }
        if raw.len() >= i + 9 && raw[i..i + 9].eq_ignore_ascii_case("<!doctype") {
            return match raw[i + 9..].find('>') {
                Some(g) => {
                    let gtpos = i + 9 + g;
                    self.handle_decl(&raw[i + 2..gtpos]);
                    (gtpos + 1) as isize
                }
                None => -1,
            };
        }
        if self.starts(i, "<![") {
            return match raw[i + 3..].find('>') {
                Some(j) => {
                    let j = i + 3 + j;
                    if raw[..j].ends_with(']') {
                        self.unknown_decl(&raw[i + 3..j - 1]);
                    } else {
                        self.builder.on_special(&raw[i + 2..j], StringKind::Comment);
                    }
                    (j + 1) as isize
                }
                None => -1,
            };
        }
        self.parse_bogus_comment(i)
    }

    fn parse_comment(&mut self, i: usize) -> isize {
        let raw = self.raw;
        let found = self
            .search(&COMMENTCLOSE, i + 4)
            .or_else(|| match_at(&COMMENTABRUPTCLOSE, raw, i + 4));
        match found {
            Some((s, e)) => {
                self.builder.on_special(&raw[i + 4..s], StringKind::Comment);
                e as isize
            }
            None => -1,
        }
    }

    fn parse_bogus_comment(&mut self, i: usize) -> isize {
        let raw = self.raw;
        match raw[i + 2..].find('>') {
            Some(p) => {
                let pos = i + 2 + p;
                self.builder.on_special(&raw[i + 2..pos], StringKind::Comment);
                (pos + 1) as isize
            }
            None => -1,
        }
    }

    fn parse_pi(&mut self, i: usize) -> isize {
        let raw = self.raw;
        match raw[i + 2..].find('>') {
            Some(p) => {
                let j = i + 2 + p;
                self.builder
                    .on_special(&raw[i + 2..j], StringKind::ProcessingInstruction);
                (j + 1) as isize
            }
            None => -1,
        }
    }

    fn check_for_whole_start_tag(&self, i: usize) -> isize {
        match match_at(&LOCATETAGEND, self.raw, i + 1) {
            Some((_, j)) => {
                if self.raw[..j].ends_with('>') {
                    j as isize
                } else {
                    -1
                }
            }
            None => -1,
        }
    }

    fn parse_starttag(&mut self, i: usize) -> isize {
        let endpos = self.check_for_whole_start_tag(i);
        if endpos < 0 {
            return endpos;
        }
        let endpos = endpos as usize;
        let raw = self.raw;
        let Some(caps) = TAGFIND_TOLERANT
            .captures_from_pos(raw, i + 1)
            .ok()
            .flatten()
            .filter(|c| c.get(0).is_some_and(|m| m.start() == i + 1))
        else {
            return endpos as isize;
        };
        let whole = caps.get(0).map(|m| m.end()).unwrap_or(i + 1);
        let tag = caps
            .get(1)
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_default();
        let mut k = whole;
        let mut attrs: Vec<(String, Option<String>)> = Vec::new();
        while k < endpos {
            let Some(m) = ATTRFIND_TOLERANT.captures_from_pos(raw, k).ok().flatten() else {
                break;
            };
            let Some(whole_m) = m.get(0) else { break };
            if whole_m.start() != k || whole_m.end() == k {
                break;
            }
            let name = m.get(1).map(|x| x.as_str()).unwrap_or("");
            let rest = m.get(2).map(|x| x.as_str()).unwrap_or("");
            let mut value: Option<String> = if rest.is_empty() {
                None
            } else {
                m.get(3).map(|x| x.as_str().to_string())
            };
            if let Some(v) = &value
                && v.len() >= 2
                && ((v.starts_with('\'') && v.ends_with('\'')) || (v.starts_with('"') && v.ends_with('"')))
            {
                value = Some(v[1..v.len() - 1].to_string());
            } else if let Some(v) = &value
                && v.len() == 1
                && (v == "'" || v == "\"")
            {
                // A lone quote both starts and ends the value in Python's slicing.
                value = Some(String::new());
            }
            if let Some(v) = &value
                && !v.is_empty()
            {
                value = Some(unescape(v));
            }
            attrs.push((name.to_lowercase(), value));
            k = whole_m.end();
        }
        let end = raw[k..endpos].trim_matches(|c: char| c.is_whitespace());
        if end != ">" && end != "/>" {
            self.builder.on_data(&raw[i..endpos]);
            return endpos as isize;
        }
        if end.ends_with("/>") {
            self.builder.on_startendtag(&tag, attrs);
        } else {
            self.builder.on_starttag(&tag, attrs, true);
            if tag == "script" || tag == "style" {
                self.set_cdata_mode(&tag, false);
            } else if tag == "textarea" || tag == "title" {
                self.set_cdata_mode(&tag, true);
            }
        }
        endpos as isize
    }

    fn parse_endtag(&mut self, i: usize) -> isize {
        let raw = self.raw;
        if !raw[i + 2..].contains('>') {
            return -1;
        }
        if match_at(&ENDTAGOPEN, raw, i).is_none() {
            if raw[i + 2..].starts_with('>') {
                return (i + 3) as isize;
            }
            return self.parse_bogus_comment(i);
        }
        let Some((_, j)) = match_at(&LOCATETAGEND, raw, i + 2) else {
            return -1;
        };
        if !raw[..j].ends_with('>') {
            return -1;
        }
        let tag = TAGFIND_TOLERANT
            .captures_from_pos(raw, i + 2)
            .ok()
            .flatten()
            .and_then(|c| c.get(1).map(|m| m.as_str().to_lowercase()))
            .unwrap_or_default();
        self.builder.on_endtag(&tag, true);
        self.clear_cdata_mode();
        j as isize
    }
}


/// A value an attribute filter compares against (bs4's `find_all(attr=...)` forms).
pub enum AttrFilter {
    /// `attr=True`: present.
    Present,
    /// `attr="value"`: equal (or one of the space-separated values for multi-valued
    /// attributes).
    Equals(String),
    /// `attr=re.compile(...)`: `search` matches.
    Regex(Regex),
    /// `attr=lambda value: ...` (called with `None` when absent).
    Func(Box<dyn Fn(Option<&str>) -> bool>),
}

/// The arguments of `find_all(name, **attrs)`.
#[derive(Default)]
pub struct TagFilter {
    names: Option<Vec<String>>,
    attrs: Vec<(String, AttrFilter)>,
}

impl TagFilter {
    /// `find_all(name)`.
    pub fn name(name: &str) -> TagFilter {
        TagFilter {
            names: Some(vec![name.to_string()]),
            attrs: Vec::new(),
        }
    }

    /// `find_all([name1, name2, ...])`.
    pub fn names(names: &[&str]) -> TagFilter {
        TagFilter {
            names: Some(names.iter().map(|s| s.to_string()).collect()),
            attrs: Vec::new(),
        }
    }

    /// Adds an attribute filter.
    pub fn attr(mut self, name: &str, filter: AttrFilter) -> TagFilter {
        self.attrs.push((name.to_string(), filter));
        self
    }

    fn matches(&self, doc: &Document, id: NodeId) -> bool {
        let NodeKind::Element { name, .. } = &doc.nodes[id].kind else {
            return false;
        };
        if let Some(names) = &self.names
            && !names.iter().any(|n| n == name)
        {
            return false;
        }
        for (attr, filter) in &self.attrs {
            let value = doc.attr(id, attr);
            let ok = match filter {
                AttrFilter::Present => value.is_some(),
                AttrFilter::Equals(expected) => match value {
                    Some(v) => {
                        v == expected
                            || (is_multi_valued(name, attr)
                                && v.split_ascii_whitespace().any(|p| p == expected))
                    }
                    None => false,
                },
                AttrFilter::Regex(re) => match value {
                    Some(v) => {
                        re.is_match(v)
                            || (is_multi_valued(name, attr)
                                && v.split_ascii_whitespace().any(|p| re.is_match(p)))
                    }
                    None => false,
                },
                AttrFilter::Func(f) => f(value),
            };
            if !ok {
                return false;
            }
        }
        true
    }
}

fn is_multi_valued(tag: &str, attr: &str) -> bool {
    matches!(attr, "class" | "accesskey" | "dropzone")
        || (matches!(tag, "a" | "link") && matches!(attr, "rel" | "rev"))
        || (matches!(tag, "td" | "th") && attr == "headers")
        || (tag == "form" && attr == "accept-charset")
        || (tag == "object" && attr == "archive")
        || (matches!(tag, "area") && matches!(attr, "rel"))
        || (matches!(tag, "icon") && attr == "sizes")
        || (matches!(tag, "iframe") && attr == "sandbox")
        || (matches!(tag, "output") && attr == "for")
}

impl Document {
    /// `BeautifulSoup(markup, "html.parser")`.
    pub fn parse(markup: &str) -> Document {
        let mut builder = TreeBuilder::new();
        {
            let mut tokenizer = Tokenizer {
                raw: markup,
                builder: &mut builder,
                cdata_elem: None,
                interesting: INTERESTING_NORMAL.clone(),
                escapable: true,
            };
            tokenizer.run();
        }
        builder.finish()
    }

    /// The `BeautifulSoup` object (`[document]`).
    pub fn root(&self) -> NodeId {
        0
    }

    /// The node's kind.
    pub fn kind(&self, id: NodeId) -> &NodeKind {
        &self.nodes[id].kind
    }

    /// `tag.name` (`None` for strings and the root).
    pub fn tag_name(&self, id: NodeId) -> Option<&str> {
        match &self.nodes[id].kind {
            NodeKind::Element { name, .. } => Some(name),
            _ => None,
        }
    }

    /// `tag.get(attr)` (multi-valued attributes are returned as their raw string).
    pub fn attr(&self, id: NodeId, attr: &str) -> Option<&str> {
        match &self.nodes[id].kind {
            NodeKind::Element { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == attr)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    /// The node's attributes in source order.
    pub fn attrs(&self, id: NodeId) -> &[(String, String)] {
        match &self.nodes[id].kind {
            NodeKind::Element { attrs, .. } => attrs,
            _ => &[],
        }
    }

    /// `tag.contents`.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    /// `tag.descendants` in document order.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.nodes[id].children.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            for &c in self.nodes[n].children.iter().rev() {
                stack.push(c);
            }
        }
        out
    }

    /// `tag.find_all(...)`.
    pub fn find_all(&self, from: NodeId, filter: &TagFilter) -> Vec<NodeId> {
        self.descendants(from)
            .into_iter()
            .filter(|&n| filter.matches(self, n))
            .collect()
    }

    /// `tag.find(...)`.
    pub fn find(&self, from: NodeId, filter: &TagFilter) -> Option<NodeId> {
        self.descendants(from)
            .into_iter()
            .find(|&n| filter.matches(self, n))
    }

    fn interesting_kinds(&self, id: NodeId) -> Vec<StringKind> {
        match self.tag_name(id).and_then(string_container) {
            Some(k) => vec![k],
            None => vec![StringKind::Navigable, StringKind::CData],
        }
    }

    /// `tag.get_text()` (separator `""`, no stripping, the tag's interesting string types).
    pub fn get_text(&self, id: NodeId) -> String {
        if let NodeKind::Text { text, kind } = &self.nodes[id].kind {
            return if matches!(kind, StringKind::Navigable | StringKind::CData) {
                text.clone()
            } else {
                String::new()
            };
        }
        let kinds = self.interesting_kinds(id);
        let mut out = String::new();
        for n in self.descendants(id) {
            if let NodeKind::Text { text, kind } = &self.nodes[n].kind
                && kinds.contains(kind)
            {
                out.push_str(text);
            }
        }
        out
    }

    /// `tag.decompose()`: removes the tag (and its subtree) from the tree.
    pub fn decompose(&mut self, id: NodeId) {
        if let Some(parent) = self.nodes[id].parent.take() {
            self.nodes[parent].children.retain(|&c| c != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_text() {
        let doc = Document::parse(
            "<html><head><title>Test Page</title></head><body><p>Test content</p></body></html>",
        );
        let title = doc.find(doc.root(), &TagFilter::name("title")).unwrap();
        assert_eq!(doc.get_text(title), "Test Page");
        assert_eq!(doc.get_text(doc.root()), "Test PageTest content");
    }

    #[test]
    fn script_and_style_strings_are_containers() {
        let mut doc = Document::parse(
            "<html><style>body { color: red; }</style><script>alert('x');</script><body><h1>Real Text</h1></body></html>",
        );
        for n in doc.find_all(doc.root(), &TagFilter::names(&["script", "style"])) {
            doc.decompose(n);
        }
        assert_eq!(doc.get_text(doc.root()), "Real Text");
    }

    #[test]
    fn attributes_and_entities() {
        let doc = Document::parse("<a href='/1?a=1&amp;b=2'>x &amp; y &copy</a><meta name=x content=\"y\">");
        let a = doc.find(doc.root(), &TagFilter::name("a")).unwrap();
        assert_eq!(doc.attr(a, "href"), Some("/1?a=1&b=2"));
        assert_eq!(doc.get_text(a), "x & y ©");
        let m = doc.find(doc.root(), &TagFilter::name("meta")).unwrap();
        assert_eq!(doc.attr(m, "content"), Some("y"));
    }
}
