//! BeautifulSoup 4.15 with the `html.parser` tree builder, as `BeautifulSoup(html, "html.parser")`
//! behaves in the original environment.
//!
//! Three layers are ported:
//!
//! - [`Tokenizer`]: CPython 3.11.14's `html.parser.HTMLParser` driven the way bs4 drives it
//!   (`convert_charrefs=False`, one `feed()` followed by `close()`), including its handling of
//!   incomplete constructs at the end of the input, CDATA/RCDATA content of `script`, `style`,
//!   `title` and `textarea`, bogus comments and the HTML5-style declaration rules.
//! - [`Builder`]: bs4's `BeautifulSoupHTMLParser` event handlers and `BeautifulSoup` tree
//!   construction (`endData` whitespace collapsing, string container classes, the
//!   preserve-whitespace stack, `_popToTag`, empty-element tags, duplicate attributes,
//!   multi-valued attributes and the `<meta>` charset substitution).
//! - [`Soup`]: the element API Semantica uses (`find`/`find_all`, `get_text`, attribute
//!   access, `decompose`/`extract`/`replace_with`, and `str(soup)` with the "minimal"
//!   formatter).
//!
//! The tree is an arena of [`Node`]s addressed by [`NodeId`]; node `0` is the
//! `BeautifulSoup` object itself (`[document]`).
//!
//! # Crate-internal DOM API
//!
//! Other modules (for instance a Docling-style HTML backend) can parse a document with
//! [`Soup::parse`] and walk it with [`Soup::root`], [`Soup::children`],
//! [`Soup::descendants`], [`Soup::node`] (tag name, attributes or string text and kind),
//! [`Soup::name`], [`Soup::get`]/[`Soup::get_str`], [`Soup::get_text`] and
//! [`Soup::parent`].

mod entities;

use std::collections::HashMap;

use fancy_regex::Regex as FancyRegex;
use once_cell::sync::Lazy;

use sem_utils::pycompat::fmt::is_space;

pub(crate) use entities::HTML5;

/// Index of a node in a [`Soup`] arena.
pub(crate) type NodeId = usize;

/// The `NavigableString` subclass of a string node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StringKind {
    /// `NavigableString`.
    Navigable,
    /// `CData`.
    CData,
    /// `Comment`.
    Comment,
    /// `Doctype`.
    Doctype,
    /// `Declaration`.
    Declaration,
    /// `ProcessingInstruction`.
    ProcessingInstruction,
    /// `Script` (text inside `<script>`).
    Script,
    /// `Stylesheet` (text inside `<style>`).
    Stylesheet,
    /// `TemplateString` (text inside `<template>`).
    TemplateString,
    /// `RubyTextString` (text inside `<rt>`).
    RubyText,
    /// `RubyParenthesisString` (text inside `<rp>`).
    RubyParenthesis,
}

impl StringKind {
    /// The Python class name.
    #[allow(dead_code)] // DOM API kept for the Docling HTML backend
    pub(crate) fn class_name(self) -> &'static str {
        match self {
            StringKind::Navigable => "NavigableString",
            StringKind::CData => "CData",
            StringKind::Comment => "Comment",
            StringKind::Doctype => "Doctype",
            StringKind::Declaration => "Declaration",
            StringKind::ProcessingInstruction => "ProcessingInstruction",
            StringKind::Script => "Script",
            StringKind::Stylesheet => "Stylesheet",
            StringKind::TemplateString => "TemplateString",
            StringKind::RubyText => "RubyTextString",
            StringKind::RubyParenthesis => "RubyParenthesisString",
        }
    }

    /// `PreformattedString` subclasses are written verbatim with a prefix and suffix.
    fn preformatted(self) -> Option<(&'static str, &'static str)> {
        match self {
            StringKind::CData => Some(("<![CDATA[", "]]>")),
            StringKind::Comment => Some(("<!--", "-->")),
            StringKind::Doctype => Some(("<!DOCTYPE ", ">\n")),
            StringKind::Declaration => Some(("<?", "?>")),
            StringKind::ProcessingInstruction => Some(("<?", ">")),
            _ => None,
        }
    }
}

/// An attribute value as bs4 stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttrValue {
    /// A plain string.
    Str(String),
    /// A multi-valued (cdata-list) attribute such as `class` or `rel`: an `AttributeValueList`.
    List(Vec<String>),
    /// `CharsetMetaAttributeValue` (`<meta charset=...>`): written as the output encoding.
    CharsetMeta(String),
    /// `ContentMetaAttributeValue` (`<meta http-equiv="content-type" content=...>`).
    ContentMeta(String),
}

impl AttrValue {
    /// The string value (`None` for lists).
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            AttrValue::Str(s) | AttrValue::CharsetMeta(s) | AttrValue::ContentMeta(s) => Some(s),
            AttrValue::List(_) => None,
        }
    }

    /// The values matched by find filters (`[value]`, or the list itself).
    fn values(&self) -> Vec<&str> {
        match self {
            AttrValue::List(items) => items.iter().map(String::as_str).collect(),
            other => vec![other.as_str().unwrap_or_default()],
        }
    }
}

/// The payload of a node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NodeData {
    /// The `BeautifulSoup` object (`[document]`), always node 0.
    Document,
    /// A `Tag`: its name and attributes in insertion order.
    Element {
        name: String,
        attrs: Vec<(String, AttrValue)>,
    },
    /// A `NavigableString` (or subclass).
    Text { text: String, kind: StringKind },
}

/// A node of the arena.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub(crate) data: NodeData,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
}

/// The bs4 `HTMLTreeBuilder.DEFAULT_EMPTY_ELEMENT_TAGS`.
const EMPTY_ELEMENT_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "menuitem",
    "meta", "param", "source", "track", "wbr", "basefont", "bgsound", "command", "frame",
    "image", "isindex", "nextid", "spacer",
];

/// `DEFAULT_PRESERVE_WHITESPACE_TAGS`.
const PRESERVE_WHITESPACE_TAGS: &[&str] = &["pre", "textarea"];

/// `DEFAULT_STRING_CONTAINERS`.
fn string_container_kind(name: &str) -> Option<StringKind> {
    match name {
        "rt" => Some(StringKind::RubyText),
        "rp" => Some(StringKind::RubyParenthesis),
        "style" => Some(StringKind::Stylesheet),
        "script" => Some(StringKind::Script),
        "template" => Some(StringKind::TemplateString),
        _ => None,
    }
}

/// `DEFAULT_CDATA_LIST_ATTRIBUTES`.
fn is_cdata_list_attribute(tag: &str, attr: &str) -> bool {
    if matches!(attr, "class" | "accesskey" | "dropzone") {
        return true;
    }
    match tag.to_lowercase().as_str() {
        "a" | "link" => matches!(attr, "rel" | "rev"),
        "td" | "th" => attr == "headers",
        "form" => attr == "accept-charset",
        "object" => attr == "archive",
        "area" => attr == "rel",
        "icon" => attr == "sizes",
        "iframe" => attr == "sandbox",
        "output" => attr == "for",
        _ => false,
    }
}

fn can_be_empty_element(name: &str) -> bool {
    EMPTY_ELEMENT_TAGS.contains(&name)
}

/// The names used by bs4's `EntitySubstitution.HTML_ENTITY_TO_CHARACTER` (names without the
/// trailing `;`, first spelling in sorted order).
static ENTITY_TO_CHARACTER: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for (name, character) in HTML5 {
        let name = name.strip_suffix(';').unwrap_or(name);
        map.entry(name).or_insert(*character);
    }
    map
});

/// `html.entities.html5[name]`.
pub(crate) fn html5_entity(name: &str) -> Option<&'static str> {
    HTML5
        .binary_search_by(|(k, _)| (*k).cmp(name))
        .ok()
        .map(|i| HTML5[i].1)
}

// ----------------------------------------------------------------------- html.unescape

/// `html._invalid_charrefs`.
fn invalid_charref(num: u32) -> Option<&'static str> {
    Some(match num {
        0x00 => "\u{fffd}",
        0x0d => "\r",
        0x80 => "\u{20ac}",
        0x81 => "\u{81}",
        0x82 => "\u{201a}",
        0x83 => "\u{0192}",
        0x84 => "\u{201e}",
        0x85 => "\u{2026}",
        0x86 => "\u{2020}",
        0x87 => "\u{2021}",
        0x88 => "\u{02c6}",
        0x89 => "\u{2030}",
        0x8a => "\u{0160}",
        0x8b => "\u{2039}",
        0x8c => "\u{0152}",
        0x8d => "\u{8d}",
        0x8e => "\u{017d}",
        0x8f => "\u{8f}",
        0x90 => "\u{90}",
        0x91 => "\u{2018}",
        0x92 => "\u{2019}",
        0x93 => "\u{201c}",
        0x94 => "\u{201d}",
        0x95 => "\u{2022}",
        0x96 => "\u{2013}",
        0x97 => "\u{2014}",
        0x98 => "\u{02dc}",
        0x99 => "\u{2122}",
        0x9a => "\u{0161}",
        0x9b => "\u{203a}",
        0x9c => "\u{0153}",
        0x9d => "\u{9d}",
        0x9e => "\u{017e}",
        0x9f => "\u{0178}",
        _ => return None,
    })
}

/// `html._invalid_codepoints`.
fn invalid_codepoint(num: u32) -> bool {
    matches!(num, 0x1..=0x8 | 0xe..=0x1f | 0x7f..=0x9f | 0xfdd0..=0xfdef | 0xb)
        || (num & 0xfffe == 0xfffe && num <= 0x10ffff)
}

/// Parse a run of digits in `radix`, saturating (Python ints are unbounded; anything above
/// U+10FFFF behaves the same).
fn parse_saturating(digits: &str, radix: u32) -> u32 {
    let mut n: u64 = 0;
    for c in digits.chars() {
        let d = c.to_digit(radix).unwrap_or(0) as u64;
        n = n.saturating_mul(radix as u64).saturating_add(d);
        if n > 0x7fff_ffff {
            n = 0x7fff_ffff;
        }
    }
    n as u32
}

/// `html.unescape(s)` (CPython 3.11): HTML5 rules for named and numeric references.
pub(crate) fn html_unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut last = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            i += 1;
            continue;
        }
        // `&(#[0-9]+;?|#[xX][0-9a-fA-F]+;?|[^\t\n\f <&#;]{1,32};?)`
        let rest = &s[i + 1..];
        let mut matched: Option<(usize, String)> = None; // (end byte index in s, replacement)
        if let Some(after) = rest.strip_prefix('#') {
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                let mut end = i + 2 + digits.len();
                if s[end..].starts_with(';') {
                    end += 1;
                }
                matched = Some((end, numeric_charref(parse_saturating(&digits, 10))));
            } else if let Some(hexpart) = after.strip_prefix(['x', 'X']) {
                let hex: String = hexpart.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
                if !hex.is_empty() {
                    let mut end = i + 3 + hex.len();
                    if s[end..].starts_with(';') {
                        end += 1;
                    }
                    matched = Some((end, numeric_charref(parse_saturating(&hex, 16))));
                }
            }
        } else {
            let mut name = String::new();
            let mut end = i + 1;
            for c in rest.chars() {
                if name.chars().count() == 32 || matches!(c, '\t' | '\n' | '\x0c' | ' ' | '<' | '&' | '#' | ';') {
                    break;
                }
                name.push(c);
                end += c.len_utf8();
            }
            if !name.is_empty() {
                if s[end..].starts_with(';') {
                    end += 1;
                    name.push(';');
                }
                matched = Some((end, named_charref(&name)));
            }
        }
        match matched {
            Some((end, replacement)) => {
                out.push_str(&s[last..i]);
                out.push_str(&replacement);
                i = end;
                last = end;
            }
            None => i += 1,
        }
    }
    out.push_str(&s[last..]);
    out
}

fn numeric_charref(num: u32) -> String {
    if let Some(r) = invalid_charref(num) {
        return r.to_string();
    }
    if (0xd800..=0xdfff).contains(&num) || num > 0x10ffff {
        return "\u{fffd}".to_string();
    }
    if invalid_codepoint(num) {
        return String::new();
    }
    char::from_u32(num).map(String::from).unwrap_or_else(|| "\u{fffd}".to_string())
}

fn named_charref(s: &str) -> String {
    if let Some(v) = html5_entity(s) {
        return v.to_string();
    }
    // Longest matching prefix (as defined by the standard).
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let len = chars.len();
    let mut x = len.saturating_sub(1);
    while x > 1 {
        let cut = chars[x].0;
        if let Some(v) = html5_entity(&s[..cut]) {
            return format!("{v}{}", &s[cut..]);
        }
        x -= 1;
    }
    format!("&{s}")
}

// ------------------------------------------------------------------- html.parser port

/// The callbacks `html.parser.HTMLParser` makes (bs4's `BeautifulSoupHTMLParser` overrides).
pub(crate) trait Handler {
    fn handle_starttag(&mut self, tag: &str, attrs: Vec<(String, Option<String>)>);
    fn handle_startendtag(&mut self, tag: &str, attrs: Vec<(String, Option<String>)>);
    fn handle_endtag(&mut self, tag: &str);
    fn handle_data(&mut self, data: &str);
    fn handle_charref(&mut self, name: &str);
    fn handle_entityref(&mut self, name: &str);
    fn handle_comment(&mut self, data: &str);
    fn handle_decl(&mut self, decl: &str);
    fn handle_pi(&mut self, data: &str);
    fn unknown_decl(&mut self, data: &str);
}

/// `tagfind_tolerant`.
static TAGFIND_TOLERANT: Lazy<FancyRegex> = Lazy::new(|| {
    FancyRegex::new(r"\G([a-zA-Z][^\t\n\r\x0c />]*)(?:[\t\n\r\x0c ]|/(?!>))*").expect("valid regex")
});

/// `attrfind_tolerant`.
static ATTRFIND_TOLERANT: Lazy<FancyRegex> = Lazy::new(|| {
    FancyRegex::new(concat!(
        r#"\G((?<=['"\t\n\r\x0c /])[^\t\n\r\x0c />][^\t\n\r\x0c /=>]*)"#,
        r#"([\t\n\r\x0c ]*=[\t\n\r\x0c ]*('[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\x0c ]*))?"#,
        r#"(?:[\t\n\r\x0c ]|/(?!>))*"#
    ))
    .expect("valid regex")
});

/// `locatetagend`.
static LOCATETAGEND: Lazy<FancyRegex> = Lazy::new(|| {
    FancyRegex::new(concat!(
        r#"\G[a-zA-Z][^\t\n\r\x0c />]*"#,
        r#"[\t\n\r\x0c /]*"#,
        r#"(?:(?<=['"\t\n\r\x0c /])[^\t\n\r\x0c />][^\t\n\r\x0c /=>]*"#,
        r#"(?:[\t\n\r\x0c ]*=[\t\n\r\x0c ]*"#,
        r#"(?:'[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\x0c ]*))?"#,
        r#"[\t\n\r\x0c /]*)*"#,
        r#">?"#
    ))
    .expect("valid regex")
});

const CDATA_CONTENT_ELEMENTS: &[&str] = &["script", "style"];
const RCDATA_CONTENT_ELEMENTS: &[&str] = &["textarea", "title"];

fn is_ascii_letter_at(s: &str, i: usize) -> bool {
    s.as_bytes().get(i).is_some_and(|b| b.is_ascii_alphabetic())
}

/// `html.parser.HTMLParser` with `convert_charrefs=False`.
pub(crate) struct Tokenizer {
    rawdata: String,
    cdata_elem: Option<String>,
    escapable: bool,
    support_cdata: bool,
}

impl Tokenizer {
    pub(crate) fn new() -> Self {
        Tokenizer {
            rawdata: String::new(),
            cdata_elem: None,
            escapable: true,
            support_cdata: true,
        }
    }

    /// `parser.feed(markup); parser.close()`.
    pub(crate) fn feed_and_close(markup: &str, handler: &mut impl Handler) {
        let mut tok = Tokenizer::new();
        tok.rawdata.push_str(markup);
        tok.goahead(false, handler);
        tok.goahead(true, handler);
    }

    fn set_cdata_mode(&mut self, elem: &str, escapable: bool) {
        self.cdata_elem = Some(elem.to_lowercase());
        self.escapable = escapable;
    }

    fn clear_cdata_mode(&mut self) {
        self.cdata_elem = None;
        self.escapable = true;
    }

    /// `self.interesting.search(rawdata, i)`.
    fn interesting_search(&self, rawdata: &str, i: usize) -> Option<usize> {
        match &self.cdata_elem {
            None => rawdata[i..].find(['&', '<']).map(|p| i + p),
            Some(elem) => {
                // `&|</elem(?=[\t\n\r\f />])` (with `&` only when escapable), ASCII
                // case-insensitive.
                let bytes = rawdata.as_bytes();
                let elem = elem.as_bytes();
                let mut p = i;
                while p < bytes.len() {
                    let b = bytes[p];
                    if b == b'&' && self.escapable {
                        return Some(p);
                    }
                    if b == b'<'
                        && bytes.get(p + 1) == Some(&b'/')
                        && bytes.len() >= p + 2 + elem.len()
                        && bytes[p + 2..p + 2 + elem.len()].eq_ignore_ascii_case(elem)
                        && matches!(
                            bytes.get(p + 2 + elem.len()),
                            Some(b'\t' | b'\n' | b'\r' | b'\x0c' | b' ' | b'/' | b'>')
                        )
                    {
                        return Some(p);
                    }
                    p += 1;
                }
                None
            }
        }
    }

    fn goahead(&mut self, end: bool, h: &mut impl Handler) {
        let rawdata = std::mem::take(&mut self.rawdata);
        let n = rawdata.len();
        let mut i = 0usize;
        while i < n {
            let j = match self.interesting_search(&rawdata, i) {
                Some(p) => p,
                None => {
                    if self.cdata_elem.is_some() {
                        break;
                    }
                    n
                }
            };
            if i < j {
                h.handle_data(&rawdata[i..j]);
            }
            i = j;
            if i == n {
                break;
            }
            let rest = &rawdata[i..];
            if rest.starts_with('<') {
                let k: Option<usize>;
                if is_ascii_letter_at(&rawdata, i + 1) {
                    k = self.parse_starttag(&rawdata, i, h);
                } else if rest.starts_with("</") {
                    k = self.parse_endtag(&rawdata, i, h);
                } else if rest.starts_with("<!--") {
                    k = Self::parse_comment(&rawdata, i, h);
                } else if rest.starts_with("<?") {
                    k = Self::parse_pi(&rawdata, i, h);
                } else if rest.starts_with("<!") {
                    k = self.parse_html_declaration(&rawdata, i, h);
                } else if i + 1 < n || end {
                    h.handle_data("<");
                    k = Some(i + 1);
                } else {
                    break;
                }
                let k = match k {
                    Some(k) => k,
                    None => {
                        if !end {
                            break;
                        }
                        if is_ascii_letter_at(&rawdata, i + 1) {
                            // incomplete start tag: dropped
                        } else if rest.starts_with("</") {
                            if i + 2 == n {
                                h.handle_data("</");
                            } else if is_ascii_letter_at(&rawdata, i + 2) {
                                // incomplete end tag: dropped
                            } else {
                                // bogus comment
                                h.handle_comment(&rawdata[i + 2..]);
                            }
                        } else if rest.starts_with("<!--") {
                            let mut j = n;
                            let body = &rawdata[i + 4..];
                            for suffix in ["--!", "--", "-"] {
                                if body.ends_with(suffix) {
                                    j -= suffix.len();
                                    break;
                                }
                            }
                            h.handle_comment(&rawdata[i + 4..j.max(i + 4)]);
                        } else if rest.starts_with("<![CDATA[") && self.support_cdata {
                            h.unknown_decl(&rawdata[i + 3..]);
                        } else if lower_prefix_is(rest, "<!doctype") {
                            h.handle_decl(&rawdata[i + 2..]);
                        } else if rest.starts_with("<!") {
                            // bogus comment
                            h.handle_comment(&rawdata[i + 2..]);
                        } else if rest.starts_with("<?") {
                            h.handle_pi(&rawdata[i + 2..]);
                        }
                        n
                    }
                };
                i = k;
            } else if rest.starts_with("&#") {
                match charref_match(&rawdata, i) {
                    Some((name, term_start, term_end, term_is_semicolon)) => {
                        h.handle_charref(name);
                        i = if term_is_semicolon { term_end } else { term_start };
                        continue;
                    }
                    None => {
                        if rest.contains(';') {
                            // bail by consuming "&#"
                            h.handle_data(&rawdata[i..i + 2]);
                            i += 2;
                        }
                        break;
                    }
                }
            } else if rest.starts_with('&') {
                if let Some((name, term_start, term_end, term_is_semicolon)) =
                    entityref_match(&rawdata, i)
                {
                    h.handle_entityref(name);
                    i = if term_is_semicolon { term_end } else { term_start };
                    continue;
                }
                // `incomplete = &[a-zA-Z#]`
                let next = rawdata[i + 1..].chars().next();
                if next.is_some_and(|c| c.is_ascii_alphabetic() || c == '#') {
                    let matched_len = 1 + next.map(char::len_utf8).unwrap_or(0);
                    if end && matched_len == n - i {
                        i += 1;
                    }
                    break;
                } else if i + 1 < n {
                    h.handle_data("&");
                    i += 1;
                } else {
                    break;
                }
            } else {
                unreachable!("interesting.search() lied");
            }
        }
        if end && i < n {
            h.handle_data(&rawdata[i..n]);
            i = n;
        }
        self.rawdata = rawdata[i..].to_string();
    }

    fn parse_html_declaration(&mut self, rawdata: &str, i: usize, h: &mut impl Handler) -> Option<usize> {
        let rest = &rawdata[i..];
        if rest.starts_with("<!--") {
            return Self::parse_comment(rawdata, i, h);
        }
        if rest.starts_with("<![CDATA[") && self.support_cdata {
            let j = rawdata[i + 9..].find("]]>")? + i + 9;
            h.unknown_decl(&rawdata[i + 3..j]);
            return Some(j + 3);
        }
        if lower_prefix_is(rest, "<!doctype") {
            let start = i + nth_char_boundary(rest, 9);
            let gtpos = rawdata[start..].find('>')? + start;
            h.handle_decl(&rawdata[i + 2..gtpos]);
            return Some(gtpos + 1);
        }
        if rest.starts_with("<![") {
            let j = rawdata[i + 3..].find('>')? + i + 3;
            if rawdata[..j].ends_with(']') {
                h.unknown_decl(&rawdata[i + 3..j - 1]);
            } else {
                h.handle_comment(&rawdata[i + 2..j]);
            }
            return Some(j + 1);
        }
        Self::parse_bogus_comment(rawdata, i, h)
    }

    fn parse_comment(rawdata: &str, i: usize, h: &mut impl Handler) -> Option<usize> {
        // commentclose = `--!?>` searched from i+4, else commentabruptclose = `-?>` at i+4.
        let start = i + 4;
        let body = &rawdata[start..];
        let mut found: Option<(usize, usize)> = None;
        let mut search = 0;
        while let Some(p) = body[search..].find("--") {
            let at = search + p;
            let after = &body[at + 2..];
            if after.starts_with('>') {
                found = Some((at, at + 3));
                break;
            }
            if after.starts_with("!>") {
                found = Some((at, at + 4));
                break;
            }
            search = at + 1;
        }
        if found.is_none() {
            if body.starts_with('>') {
                found = Some((0, 1));
            } else if body.starts_with("->") {
                found = Some((0, 2));
            }
        }
        let (mstart, mend) = found?;
        h.handle_comment(&body[..mstart]);
        Some(start + mend)
    }

    fn parse_bogus_comment(rawdata: &str, i: usize, h: &mut impl Handler) -> Option<usize> {
        let pos = rawdata[i + 2..].find('>')? + i + 2;
        h.handle_comment(&rawdata[i + 2..pos]);
        Some(pos + 1)
    }

    fn parse_pi(rawdata: &str, i: usize, h: &mut impl Handler) -> Option<usize> {
        let j = rawdata[i + 2..].find('>')? + i + 2;
        h.handle_pi(&rawdata[i + 2..j]);
        Some(j + 1)
    }

    fn check_for_whole_start_tag(rawdata: &str, i: usize) -> Option<usize> {
        let m = LOCATETAGEND
            .find_from_pos(rawdata, i + 1)
            .ok()
            .flatten()
            .filter(|m| m.start() == i + 1)?;
        let j = m.end();
        if rawdata[..j].ends_with('>') { Some(j) } else { None }
    }

    fn parse_starttag(&mut self, rawdata: &str, i: usize, h: &mut impl Handler) -> Option<usize> {
        let endpos = Self::check_for_whole_start_tag(rawdata, i)?;
        let caps = TAGFIND_TOLERANT
            .captures_from_pos(rawdata, i + 1)
            .ok()
            .flatten()
            .expect("unexpected call to parse_starttag()");
        let mut k = caps.get(0).map(|m| m.end()).unwrap_or(i + 1);
        let tag = caps.get(1).map(|m| m.as_str()).unwrap_or_default().to_lowercase();
        let mut attrs: Vec<(String, Option<String>)> = Vec::new();
        // Matches are bounded by the end of the tag (the attributes of a complete tag end
        // before `endpos`), which keeps failed matches from scanning the rest of the input.
        let bounded = &rawdata[..endpos];
        while k < endpos {
            let Some(m) = ATTRFIND_TOLERANT.captures_from_pos(bounded, k).ok().flatten() else {
                break;
            };
            let Some(whole) = m.get(0).filter(|w| w.start() == k) else {
                break;
            };
            let attrname = m.get(1).map(|g| g.as_str()).unwrap_or_default();
            let rest = m.get(2).map(|g| g.as_str());
            let mut attrvalue: Option<String> = m.get(3).map(|g| g.as_str().to_string());
            match rest {
                None | Some("") => attrvalue = None,
                Some(_) => {
                    if let Some(v) = &attrvalue {
                        let first = v.chars().next();
                        let last = v.chars().last();
                        if v.chars().count() >= 1
                            && first == last
                            && (first == Some('\'') || first == Some('"'))
                            && v.len() >= 2
                        {
                            attrvalue = Some(v[1..v.len() - 1].to_string());
                        } else if v.len() == 1 && (first == Some('\'') || first == Some('"')) {
                            // `attrvalue[:1] == "'" == attrvalue[-1:]` for a lone quote
                            attrvalue = Some(String::new());
                        }
                    }
                }
            }
            if let Some(v) = &attrvalue
                && !v.is_empty()
            {
                attrvalue = Some(html_unescape(v));
            }
            attrs.push((attrname.to_lowercase(), attrvalue));
            k = whole.end();
        }
        let end = sem_utils::pycompat::fmt::strip(if k <= endpos { &rawdata[k..endpos] } else { "" });
        if end != ">" && end != "/>" {
            h.handle_data(&rawdata[i..endpos]);
            return Some(endpos);
        }
        if end.ends_with("/>") {
            // XHTML-style empty tag: <span attr="value" />
            h.handle_startendtag(&tag, attrs);
        } else {
            h.handle_starttag(&tag, attrs);
            if CDATA_CONTENT_ELEMENTS.contains(&tag.as_str()) {
                self.set_cdata_mode(&tag, false);
            } else if RCDATA_CONTENT_ELEMENTS.contains(&tag.as_str()) {
                self.set_cdata_mode(&tag, true);
            }
        }
        Some(endpos)
    }

    fn parse_endtag(&mut self, rawdata: &str, i: usize, h: &mut impl Handler) -> Option<usize> {
        rawdata[i + 2..].find('>')?;
        if !is_ascii_letter_at(rawdata, i + 2) {
            if rawdata[i + 2..].starts_with('>') {
                // "missing-end-tag-name" parser error: `</>` is ignored
                return Some(i + 3);
            }
            return Self::parse_bogus_comment(rawdata, i, h);
        }
        let m = LOCATETAGEND
            .find_from_pos(rawdata, i + 2)
            .ok()
            .flatten()
            .filter(|m| m.start() == i + 2)?;
        let j = m.end();
        if !rawdata[..j].ends_with('>') {
            return None;
        }
        let caps = TAGFIND_TOLERANT
            .captures_from_pos(rawdata, i + 2)
            .ok()
            .flatten()
            .expect("tagfind matches after a letter");
        let tag = caps.get(1).map(|m| m.as_str()).unwrap_or_default().to_lowercase();
        h.handle_endtag(&tag);
        self.clear_cdata_mode();
        Some(j)
    }
}

/// Byte offset of the `n`th character of `s` (or `s.len()`).
fn nth_char_boundary(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(b, _)| b).unwrap_or(s.len())
}

/// `rawdata[i:i+len(prefix)].lower() == prefix` (character-wise Unicode lowercase).
fn lower_prefix_is(rest: &str, prefix: &str) -> bool {
    let count = prefix.chars().count();
    let head: String = rest.chars().take(count).flat_map(char::to_lowercase).collect();
    head == prefix
}

/// `charref = &#(?:[0-9]+|[xX][0-9a-fA-F]+)[^0-9a-fA-F]` matched at `i`: returns the name
/// (`match.group()[2:-1]`), the terminator's start and end, and whether it is `;`.
fn charref_match(rawdata: &str, i: usize) -> Option<(&str, usize, usize, bool)> {
    let body = &rawdata[i + 2..];
    let first = body.chars().next()?;
    let name_len = if first.is_ascii_digit() {
        body.bytes().take_while(|b| b.is_ascii_digit()).count()
    } else if first == 'x' || first == 'X' {
        let hex = body[1..].bytes().take_while(|b| b.is_ascii_hexdigit()).count();
        if hex == 0 {
            return None;
        }
        1 + hex
    } else {
        return None;
    };
    let term = body[name_len..].chars().next()?;
    if term.is_ascii_hexdigit() {
        return None;
    }
    let term_start = i + 2 + name_len;
    Some((&body[..name_len], term_start, term_start + term.len_utf8(), term == ';'))
}

/// `entityref = &([a-zA-Z][-.a-zA-Z0-9]*)[^a-zA-Z0-9]` matched at `i` (with the regex's
/// backtracking: a name running to the end of the input may end at its last `-`/`.`, which
/// then serves as the terminator).
fn entityref_match(rawdata: &str, i: usize) -> Option<(&str, usize, usize, bool)> {
    let body = &rawdata[i + 1..];
    let bytes = body.as_bytes();
    if !bytes.first()?.is_ascii_alphabetic() {
        return None;
    }
    let run = 1 + bytes[1..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-' || **b == b'.')
        .count();
    if let Some(term) = body[run..].chars().next() {
        let term_start = i + 1 + run;
        return Some((&body[..run], term_start, term_start + term.len_utf8(), term == ';'));
    }
    // The run reaches the end: backtrack to the last '-' or '.' after the first letter.
    let pos = bytes[1..run].iter().rposition(|b| *b == b'-' || *b == b'.')? + 1;
    let term_start = i + 1 + pos;
    Some((&body[..pos], term_start, term_start + 1, false))
}

// --------------------------------------------------------------------------- tree build

/// `UnicodeDammit.numeric_character_reference`.
fn numeric_character_reference(numeric: u32) -> String {
    if numeric == 0 || numeric > 0x10ffff || (0xd800..=0xdfff).contains(&numeric) {
        return "\u{fffd}".to_string();
    }
    if (0x80..=0x9f).contains(&numeric)
        && let Some(s) = windows_1252(numeric)
    {
        return s.to_string();
    }
    char::from_u32(numeric)
        .map(String::from)
        .unwrap_or_else(|| "\u{fffd}".to_string())
}

/// `UnicodeDammit.WINDOWS_1252_TO_UTF8` for 0x80..=0x9F.
fn windows_1252(b: u32) -> Option<&'static str> {
    Some(match b {
        0x80 => "\u{20ac}",
        0x82 => "\u{201a}",
        0x83 => "\u{192}",
        0x84 => "\u{201e}",
        0x85 => "\u{2026}",
        0x86 => "\u{2020}",
        0x87 => "\u{2021}",
        0x88 => "\u{2c6}",
        0x89 => "\u{2030}",
        0x8a => "\u{160}",
        0x8b => "\u{2039}",
        0x8c => "\u{152}",
        0x8e => "\u{17d}",
        0x91 => "\u{2018}",
        0x92 => "\u{2019}",
        0x93 => "\u{201c}",
        0x94 => "\u{201d}",
        0x95 => "\u{2022}",
        0x96 => "\u{2013}",
        0x97 => "\u{2014}",
        0x98 => "\u{2dc}",
        0x99 => "\u{2122}",
        0x9a => "\u{161}",
        0x9b => "\u{203a}",
        0x9c => "\u{153}",
        0x9e => "\u{17e}",
        0x9f => "\u{178}",
        _ => return None,
    })
}

/// bs4's `BeautifulSoupHTMLParser` handlers plus `BeautifulSoup`'s tree construction.
struct Builder {
    soup: Soup,
    current_data: Vec<String>,
    tag_stack: Vec<NodeId>,
    open_tag_counter: HashMap<String, usize>,
    preserve_whitespace_tag_stack: Vec<NodeId>,
    string_container_stack: Vec<NodeId>,
    already_closed_empty_element: Vec<String>,
}

const ASCII_SPACES: &[char] = &['\x20', '\x0a', '\x09', '\x0c', '\x0d'];

impl Builder {
    fn new() -> Self {
        let soup = Soup {
            nodes: vec![Node {
                data: NodeData::Document,
                parent: None,
                children: Vec::new(),
            }],
        };
        Builder {
            soup,
            current_data: Vec::new(),
            tag_stack: vec![0],
            open_tag_counter: HashMap::new(),
            preserve_whitespace_tag_stack: Vec::new(),
            string_container_stack: Vec::new(),
            already_closed_empty_element: Vec::new(),
        }
    }

    fn current_tag(&self) -> NodeId {
        *self.tag_stack.last().unwrap_or(&0)
    }

    fn push_tag(&mut self, id: NodeId, name: &str) {
        let parent = self.current_tag();
        self.soup.nodes[parent].children.push(id);
        self.soup.nodes[id].parent = Some(parent);
        self.tag_stack.push(id);
        *self.open_tag_counter.entry(name.to_string()).or_insert(0) += 1;
        if PRESERVE_WHITESPACE_TAGS.contains(&name) {
            self.preserve_whitespace_tag_stack.push(id);
        }
        if string_container_kind(name).is_some() {
            self.string_container_stack.push(id);
        }
    }

    fn pop_tag(&mut self) {
        if self.tag_stack.len() <= 1 {
            return;
        }
        let id = self.tag_stack.pop().expect("non-empty");
        if let Some(name) = self.soup.name(id).map(str::to_string)
            && let Some(c) = self.open_tag_counter.get_mut(&name)
        {
            *c = c.saturating_sub(1);
        }
        if self.preserve_whitespace_tag_stack.last() == Some(&id) {
            self.preserve_whitespace_tag_stack.pop();
        }
        if self.string_container_stack.last() == Some(&id) {
            self.string_container_stack.pop();
        }
    }

    fn pop_to_tag(&mut self, name: &str) {
        if name == "[document]" {
            return;
        }
        let mut i = self.tag_stack.len();
        while i > 1 {
            i -= 1;
            if self.open_tag_counter.get(name).copied().unwrap_or(0) == 0 {
                break;
            }
            let t = self.tag_stack[i];
            let is_match = self.soup.name(t) == Some(name);
            self.pop_tag();
            if is_match {
                break;
            }
        }
    }

    fn end_data(&mut self, container: Option<StringKind>) {
        if self.current_data.is_empty() {
            return;
        }
        let mut data: String = self.current_data.concat();
        self.current_data.clear();
        if self.preserve_whitespace_tag_stack.is_empty() && data.chars().all(|c| ASCII_SPACES.contains(&c)) {
            data = if data.contains('\n') { "\n".to_string() } else { " ".to_string() };
        }
        let mut kind = container.unwrap_or(StringKind::Navigable);
        if kind == StringKind::Navigable
            && let Some(&top) = self.string_container_stack.last()
            && let Some(name) = self.soup.name(top)
            && let Some(k) = string_container_kind(name)
        {
            kind = k;
        }
        let id = self.soup.nodes.len();
        let parent = self.current_tag();
        self.soup.nodes.push(Node {
            data: NodeData::Text { text: data, kind },
            parent: Some(parent),
            children: Vec::new(),
        });
        self.soup.nodes[parent].children.push(id);
    }

    /// `BeautifulSoup.handle_starttag`: returns the new tag.
    fn soup_handle_starttag(&mut self, name: &str, attrs: Vec<(String, String)>) -> NodeId {
        self.end_data(None);
        let mut values: Vec<(String, AttrValue)> = attrs
            .into_iter()
            .map(|(k, v)| {
                if is_cdata_list_attribute(name, &k) {
                    let items = v.split(is_space).filter(|s| !s.is_empty()).map(str::to_string).collect();
                    (k, AttrValue::List(items))
                } else {
                    (k, AttrValue::Str(v))
                }
            })
            .collect();
        if name == "meta" {
            set_up_substitutions(&mut values);
        }
        let id = self.soup.nodes.len();
        self.soup.nodes.push(Node {
            data: NodeData::Element {
                name: name.to_string(),
                attrs: values,
            },
            parent: None,
            children: Vec::new(),
        });
        self.push_tag(id, name);
        id
    }

    fn soup_handle_endtag(&mut self, name: &str) {
        self.end_data(None);
        self.pop_to_tag(name);
    }

    fn bs_handle_starttag(&mut self, tag: &str, attrs: Vec<(String, Option<String>)>, handle_empty_element: bool) {
        // `attr_dict[key] = value` (REPLACE on duplicates keeps the first position).
        let mut attr_dict: Vec<(String, String)> = Vec::new();
        for (key, value) in attrs {
            let value = value.unwrap_or_default();
            if let Some(slot) = attr_dict.iter_mut().find(|(k, _)| *k == key) {
                slot.1 = value;
            } else {
                attr_dict.push((key, value));
            }
        }
        let id = self.soup_handle_starttag(tag, attr_dict);
        if handle_empty_element && self.soup.is_empty_element(id) {
            self.bs_handle_endtag(tag, false);
            self.already_closed_empty_element.push(tag.to_string());
        }
    }

    fn bs_handle_endtag(&mut self, tag: &str, check_already_closed: bool) {
        if check_already_closed
            && let Some(pos) = self.already_closed_empty_element.iter().position(|t| t == tag)
        {
            self.already_closed_empty_element.remove(pos);
        } else {
            self.soup_handle_endtag(tag);
        }
    }

    fn finish(mut self) -> Soup {
        self.end_data(None);
        while self.tag_stack.len() > 1 {
            self.pop_tag();
        }
        self.soup
    }
}

/// `HTMLTreeBuilder.set_up_substitutions` for a `<meta>` tag.
fn set_up_substitutions(attrs: &mut [(String, AttrValue)]) {
    let get = |attrs: &[(String, AttrValue)], key: &str| attrs.iter().position(|(k, _)| k == key);
    let charset = get(attrs, "charset");
    let content = get(attrs, "content");
    let http_equiv: Vec<String> = match get(attrs, "http-equiv").map(|i| &attrs[i].1) {
        None => Vec::new(),
        Some(AttrValue::List(items)) => items.clone(),
        Some(other) => vec![other.as_str().unwrap_or_default().to_string()],
    };
    if let Some(i) = charset {
        if let AttrValue::Str(s) = &attrs[i].1 {
            attrs[i].1 = AttrValue::CharsetMeta(s.clone());
        }
    } else if let Some(i) = content
        && http_equiv.iter().any(|x| x.to_lowercase() == "content-type")
        && let AttrValue::Str(s) = &attrs[i].1
    {
        attrs[i].1 = AttrValue::ContentMeta(s.clone());
    }
}

impl Handler for Builder {
    fn handle_starttag(&mut self, tag: &str, attrs: Vec<(String, Option<String>)>) {
        self.bs_handle_starttag(tag, attrs, true);
    }

    fn handle_startendtag(&mut self, tag: &str, attrs: Vec<(String, Option<String>)>) {
        self.bs_handle_starttag(tag, attrs, false);
        self.bs_handle_endtag(tag, false);
    }

    fn handle_endtag(&mut self, tag: &str) {
        self.bs_handle_endtag(tag, true);
    }

    fn handle_data(&mut self, data: &str) {
        self.current_data.push(data.to_string());
    }

    fn handle_charref(&mut self, name: &str) {
        let (digits, radix) = match name.strip_prefix(['x', 'X']) {
            Some(rest) => (rest, 16),
            None => (name, 10),
        };
        // html.parser only passes all-digit (or `x`+hex) names, so `int(name, base)` succeeds.
        let dereferenced = numeric_character_reference(parse_saturating(digits, radix));
        self.handle_data(&dereferenced);
        self.handle_data("");
    }

    fn handle_entityref(&mut self, name: &str) {
        match ENTITY_TO_CHARACTER.get(name) {
            Some(c) => self.handle_data(c),
            None => self.handle_data(&format!("&{name}")),
        }
    }

    fn handle_comment(&mut self, data: &str) {
        self.end_data(None);
        self.handle_data(data);
        self.end_data(Some(StringKind::Comment));
    }

    fn handle_decl(&mut self, decl: &str) {
        self.end_data(None);
        let decl: String = decl.chars().skip("DOCTYPE ".len()).collect();
        self.handle_data(&decl);
        self.end_data(Some(StringKind::Doctype));
    }

    fn unknown_decl(&mut self, data: &str) {
        let (kind, data) = if data.to_uppercase().starts_with("CDATA[") {
            (StringKind::CData, data.chars().skip("CDATA[".len()).collect::<String>())
        } else {
            (StringKind::Declaration, data.to_string())
        };
        self.end_data(None);
        self.handle_data(&data);
        self.end_data(Some(kind));
    }

    fn handle_pi(&mut self, data: &str) {
        self.end_data(None);
        self.handle_data(data);
        self.end_data(Some(StringKind::ProcessingInstruction));
    }
}

// ------------------------------------------------------------------------- element API

/// A parsed document: `BeautifulSoup(markup, "html.parser")`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Soup {
    nodes: Vec<Node>,
}

/// `Formatter.substitute` with the "minimal" formatter: `EntitySubstitution.substitute_xml`.
fn substitute_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

/// `EntitySubstitution.quoted_attribute_value`.
fn quoted_attribute_value(value: &str) -> String {
    if value.contains('"') {
        if value.contains('\'') {
            return format!("\"{}\"", value.replace('"', "&quot;"));
        }
        return format!("'{value}'");
    }
    format!("\"{value}\"")
}

/// `ContentMetaAttributeValue.substitute_encoding("utf-8")`: rewrite each
/// `((^|;)\s*charset=)([^;]*)` (multi-line `^`) with the encoding.
fn substitute_content_charset(original: &str) -> String {
    static CHARSET_RE: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"(?m)((^|;)\s*charset=)([^;]*)").expect("valid regex"));
    CHARSET_RE.replace_all(original, "${1}utf-8").into_owned()
}

impl Soup {
    /// `BeautifulSoup(markup, "html.parser")`.
    pub(crate) fn parse(markup: &str) -> Soup {
        let mut builder = Builder::new();
        Tokenizer::feed_and_close(markup, &mut builder);
        builder.finish()
    }

    /// The `BeautifulSoup` object (node 0).
    pub(crate) fn root(&self) -> NodeId {
        0
    }

    /// A node.
    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// The node's parent (`None` for the root and detached nodes).
    pub(crate) fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes[id].parent
    }

    /// The node's children (`.contents`).
    #[allow(dead_code)] // DOM API kept for the Docling HTML backend
    pub(crate) fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    /// `tag.name` (`[document]` for the root, `None` for strings).
    pub(crate) fn name(&self, id: NodeId) -> Option<&str> {
        match &self.nodes[id].data {
            NodeData::Document => Some("[document]"),
            NodeData::Element { name, .. } => Some(name),
            NodeData::Text { .. } => None,
        }
    }

    /// Whether the node is a `Tag` (the root counts as one).
    pub(crate) fn is_tag(&self, id: NodeId) -> bool {
        !matches!(self.nodes[id].data, NodeData::Text { .. })
    }

    /// A string node's text and class.
    pub(crate) fn string(&self, id: NodeId) -> Option<(&str, StringKind)> {
        match &self.nodes[id].data {
            NodeData::Text { text, kind } => Some((text, *kind)),
            _ => None,
        }
    }

    /// `.descendants`: every node below `id`, in document order.
    pub(crate) fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.nodes[id].children.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.nodes[n].children.iter().rev().copied());
        }
        out
    }

    /// The attributes of a tag (empty for other nodes).
    pub(crate) fn attrs(&self, id: NodeId) -> &[(String, AttrValue)] {
        match &self.nodes[id].data {
            NodeData::Element { attrs, .. } => attrs,
            _ => &[],
        }
    }

    /// `tag.get(key)`.
    pub(crate) fn get(&self, id: NodeId, key: &str) -> Option<&AttrValue> {
        self.attrs(id).iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// `tag.get(key)` for a string-valued attribute (lists are joined with spaces).
    pub(crate) fn get_str(&self, id: NodeId, key: &str) -> Option<String> {
        self.get(id, key).map(|v| match v {
            AttrValue::List(items) => items.join(" "),
            other => other.as_str().unwrap_or_default().to_string(),
        })
    }

    /// `tag.has_attr(key)`.
    pub(crate) fn has_attr(&self, id: NodeId, key: &str) -> bool {
        self.get(id, key).is_some()
    }

    /// `tag.is_empty_element`: no contents and a void element name.
    pub(crate) fn is_empty_element(&self, id: NodeId) -> bool {
        match &self.nodes[id].data {
            NodeData::Element { name, .. } => self.nodes[id].children.is_empty() && can_be_empty_element(name),
            _ => false,
        }
    }

    /// `tag.find_all(...)` with a predicate over descendants (document order).
    pub(crate) fn find_all(&self, id: NodeId, pred: impl Fn(&Soup, NodeId) -> bool) -> Vec<NodeId> {
        self.descendants(id).into_iter().filter(|&n| pred(self, n)).collect()
    }

    /// `tag.find(...)`: the first matching descendant.
    pub(crate) fn find(&self, id: NodeId, pred: impl Fn(&Soup, NodeId) -> bool) -> Option<NodeId> {
        self.descendants(id).into_iter().find(|&n| pred(self, n))
    }

    /// `tag.find_all(name)` / `tag.find_all([names])`: descendant tags with one of the names.
    pub(crate) fn find_all_named(&self, id: NodeId, names: &[&str]) -> Vec<NodeId> {
        self.find_all(id, |s, n| {
            matches!(&s.nodes[n].data, NodeData::Element { name, .. } if names.contains(&name.as_str()))
        })
    }

    /// `tag.find(name)`.
    pub(crate) fn find_named(&self, id: NodeId, name: &str) -> Option<NodeId> {
        self.find(id, |s, n| matches!(&s.nodes[n].data, NodeData::Element { name: m, .. } if m == name))
    }

    /// The `SoupStrainer` attribute-value rule for a string: some value of the attribute (or
    /// the space-joined list) equals `expected`.
    pub(crate) fn attribute_matches(&self, id: NodeId, attr: &str, expected: &str) -> bool {
        match self.get(id, attr) {
            None => false,
            Some(v) => {
                let values = v.values();
                values.contains(&expected) || (values.len() != 1 && values.join(" ") == expected)
            }
        }
    }

    /// The string classes `tag.get_text()` collects (`interesting_string_types`).
    fn interesting_kinds(&self, id: NodeId) -> Vec<StringKind> {
        match self.name(id).and_then(string_container_kind) {
            Some(kind) if self.is_tag(id) => vec![kind],
            _ => vec![StringKind::Navigable, StringKind::CData],
        }
    }

    /// `tag.get_text()`: the concatenated interesting strings below `id`.
    pub(crate) fn get_text(&self, id: NodeId) -> String {
        if let NodeData::Text { text, kind } = &self.nodes[id].data {
            return if matches!(kind, StringKind::Navigable | StringKind::CData) {
                text.clone()
            } else {
                String::new()
            };
        }
        let kinds = self.interesting_kinds(id);
        let mut out = String::new();
        for n in self.descendants(id) {
            if let NodeData::Text { text, kind } = &self.nodes[n].data
                && kinds.contains(kind)
            {
                out.push_str(text);
            }
        }
        out
    }

    /// `element.extract()`: detach the node from its parent.
    pub(crate) fn extract(&mut self, id: NodeId) {
        if let Some(parent) = self.nodes[id].parent.take()
            && let Some(pos) = self.nodes[parent].children.iter().position(|&c| c == id)
        {
            self.nodes[parent].children.remove(pos);
        }
    }

    /// `element.decompose()`: detach the node and discard its subtree.
    pub(crate) fn decompose(&mut self, id: NodeId) {
        self.extract(id);
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let children = std::mem::take(&mut self.nodes[n].children);
            for c in &children {
                self.nodes[*c].parent = None;
            }
            stack.extend(children);
            if let NodeData::Element { name, .. } = &mut self.nodes[n].data {
                name.clear();
            }
        }
    }

    /// `element.replace_with(text)`: replace a node with a plain `NavigableString`.
    pub(crate) fn replace_with_text(&mut self, id: NodeId, text: &str) {
        let Some(parent) = self.nodes[id].parent else {
            return;
        };
        let Some(pos) = self.nodes[parent].children.iter().position(|&c| c == id) else {
            return;
        };
        let new_id = self.nodes.len();
        self.nodes.push(Node {
            data: NodeData::Text {
                text: text.to_string(),
                kind: StringKind::Navigable,
            },
            parent: Some(parent),
            children: Vec::new(),
        });
        self.nodes[parent].children[pos] = new_id;
        self.nodes[id].parent = None;
    }

    /// `str(element)`: `decode()` with the "minimal" formatter and `eventual_encoding="utf-8"`.
    pub(crate) fn decode(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.decode_into(id, &mut out);
        out
    }

    fn decode_into(&self, id: NodeId, out: &mut String) {
        match &self.nodes[id].data {
            NodeData::Document => {
                for &c in &self.nodes[id].children {
                    self.decode_into(c, out);
                }
            }
            NodeData::Text { text, kind } => match kind.preformatted() {
                Some((prefix, suffix)) => {
                    out.push_str(prefix);
                    out.push_str(text);
                    out.push_str(suffix);
                }
                None => {
                    let in_cdata_tag = self.nodes[id]
                        .parent
                        .and_then(|p| self.name(p))
                        .is_some_and(|n| n == "script" || n == "style");
                    if in_cdata_tag {
                        out.push_str(text);
                    } else {
                        out.push_str(&substitute_xml(text));
                    }
                }
            },
            NodeData::Element { name, attrs } => {
                out.push('<');
                out.push_str(name);
                let mut sorted: Vec<&(String, AttrValue)> = attrs.iter().collect();
                sorted.sort_by(|a, b| a.0.cmp(&b.0));
                for (key, value) in sorted {
                    let val = match value {
                        AttrValue::Str(s) => s.clone(),
                        AttrValue::List(items) => items.join(" "),
                        AttrValue::CharsetMeta(_) => "utf-8".to_string(),
                        AttrValue::ContentMeta(original) => substitute_content_charset(original),
                    };
                    out.push(' ');
                    out.push_str(key);
                    out.push('=');
                    out.push_str(&quoted_attribute_value(&substitute_xml(&val)));
                }
                if self.is_empty_element(id) {
                    out.push_str("/>");
                    return;
                }
                out.push('>');
                for &c in &self.nodes[id].children {
                    self.decode_into(c, out);
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_follows_html5_rules() {
        assert_eq!(html_unescape("a &amp; b"), "a & b");
        assert_eq!(html_unescape("&ampx &notit; &#150; &#x41;"), "&x ¬it; \u{2013} A");
        assert_eq!(html_unescape("&#0;&#xD800;&#1;"), "\u{fffd}\u{fffd}");
        assert_eq!(html_unescape("&zzz;"), "&zzz;");
    }

    #[test]
    fn tokenizer_quirks_match_html_parser() {
        struct Rec(Vec<String>);
        impl Handler for Rec {
            fn handle_starttag(&mut self, t: &str, _: Vec<(String, Option<String>)>) {
                self.0.push(format!("START {t}"));
            }
            fn handle_startendtag(&mut self, t: &str, _: Vec<(String, Option<String>)>) {
                self.0.push(format!("STARTEND {t}"));
            }
            fn handle_endtag(&mut self, t: &str) {
                self.0.push(format!("END {t}"));
            }
            fn handle_data(&mut self, d: &str) {
                self.0.push(format!("DATA {d}"));
            }
            fn handle_charref(&mut self, n: &str) {
                self.0.push(format!("CHAR {n}"));
            }
            fn handle_entityref(&mut self, n: &str) {
                self.0.push(format!("ENT {n}"));
            }
            fn handle_comment(&mut self, d: &str) {
                self.0.push(format!("COMMENT {d}"));
            }
            fn handle_decl(&mut self, d: &str) {
                self.0.push(format!("DECL {d}"));
            }
            fn handle_pi(&mut self, d: &str) {
                self.0.push(format!("PI {d}"));
            }
            fn unknown_decl(&mut self, d: &str) {
                self.0.push(format!("UNKNOWN {d}"));
            }
        }
        let run = |s: &str| {
            let mut r = Rec(Vec::new());
            Tokenizer::feed_and_close(s, &mut r);
            r.0
        };
        assert_eq!(run("x &a"), vec!["DATA x ", "DATA a"]);
        assert_eq!(run("t &amp"), vec!["DATA t ", "DATA &amp"]);
        assert_eq!(
            run("a &#12ab; b <b>bold</b> c &#zz; d <i>x</i> e"),
            vec![
                "DATA a ",
                "DATA &#",
                "DATA 12ab; b ",
                "START b",
                "DATA bold",
                "END b",
                "DATA  c ",
                "DATA &#",
                "DATA zz; d <i>x</i> e"
            ]
        );
    }
}
