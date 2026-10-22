//! `yaml.dump` (PyYAML 6.0) for JSON-like values, reproducing PyYAML's representer, implicit
//! tag resolution and emitter: key sorting, scalar style choice (plain / single / double
//! quoted), line folding at `width`, `allow_unicode` escaping and the `...` document end that
//! follows a plain root scalar.
//!
//! `serde_yaml` emits a different (equally valid) document; Semantica's YAML output is what
//! PyYAML writes, so the emitter is ported here state for state.

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

use super::fmt::float_repr;

/// Keyword arguments of `yaml.dump` that Semantica uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumpOptions {
    /// `default_flow_style`: `Some(false)` (PyYAML's default) writes block collections,
    /// `Some(true)` flow collections, `None` flow style only for collections of scalars.
    pub default_flow_style: Option<bool>,
    /// `sort_keys` (default `True`).
    pub sort_keys: bool,
    /// `allow_unicode` (default `False`: non-ASCII characters are escaped in double quotes).
    pub allow_unicode: bool,
    /// `indent` (default 2; values outside 2..=9 fall back to 2, as in PyYAML).
    pub indent: usize,
    /// `width` (default 80; values not above `2 * indent` fall back to 80).
    pub width: usize,
    /// `explicit_start`: write `---` before the document.
    pub explicit_start: bool,
}

impl Default for DumpOptions {
    fn default() -> Self {
        DumpOptions {
            default_flow_style: Some(false),
            sort_keys: true,
            allow_unicode: false,
            indent: 2,
            width: 80,
            explicit_start: false,
        }
    }
}

/// `yaml.dump(value, **options)`.
pub fn dump(value: &Value, options: &DumpOptions) -> String {
    let node = represent(value, options);
    let mut emitter = Emitter::new(options);
    emitter.emit_document(&node, options.explicit_start);
    emitter.out
}

/// `yaml.dump(value, default_flow_style=False)` (the call `format_data` makes).
pub fn dump_default(value: &Value) -> String {
    dump(value, &DumpOptions::default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tag {
    Str,
    Int,
    Float,
    Bool,
    Null,
    Other,
}

impl Tag {
    /// Length of the short form (`!!str`) PyYAML counts towards the simple-key limit.
    fn short_len(self) -> usize {
        match self {
            Tag::Str | Tag::Int => 5,
            Tag::Float => 7,
            Tag::Bool | Tag::Null => 6,
            Tag::Other => 0,
        }
    }

    fn short(self) -> &'static str {
        match self {
            Tag::Str => "!!str",
            Tag::Int => "!!int",
            Tag::Float => "!!float",
            Tag::Bool => "!!bool",
            Tag::Null => "!!null",
            Tag::Other => "",
        }
    }
}

enum Node {
    Scalar {
        tag: Tag,
        value: String,
    },
    Seq {
        items: Vec<Node>,
        flow: bool,
    },
    Map {
        pairs: Vec<(Node, Node)>,
        flow: bool,
    },
}

fn represent(value: &Value, options: &DumpOptions) -> Node {
    match value {
        Value::Null => scalar(Tag::Null, "null".to_string()),
        Value::Bool(b) => scalar(Tag::Bool, if *b { "true" } else { "false" }.to_string()),
        Value::Number(n) if n.is_f64() => {
            scalar(Tag::Float, represent_float(n.as_f64().unwrap_or(f64::NAN)))
        }
        Value::Number(n) => scalar(Tag::Int, n.to_string()),
        Value::String(s) => scalar(Tag::Str, s.clone()),
        Value::Array(items) => {
            let items: Vec<Node> = items.iter().map(|v| represent(v, options)).collect();
            let best = items.iter().all(|n| matches!(n, Node::Scalar { .. }));
            Node::Seq {
                flow: options.default_flow_style.unwrap_or(best),
                items,
            }
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            if options.sort_keys {
                entries.sort_by(|a, b| a.0.cmp(b.0));
            }
            let pairs: Vec<(Node, Node)> = entries
                .into_iter()
                .map(|(k, v)| (scalar(Tag::Str, k.clone()), represent(v, options)))
                .collect();
            let best = pairs
                .iter()
                .all(|(k, v)| matches!(k, Node::Scalar { .. }) && matches!(v, Node::Scalar { .. }));
            Node::Map {
                flow: options.default_flow_style.unwrap_or(best),
                pairs,
            }
        }
    }
}

fn scalar(tag: Tag, value: String) -> Node {
    Node::Scalar { tag, value }
}

fn represent_float(data: f64) -> String {
    if data.is_nan() {
        return ".nan".to_string();
    }
    if data.is_infinite() {
        return if data > 0.0 { ".inf" } else { "-.inf" }.to_string();
    }
    let value = float_repr(data).to_lowercase();
    if !value.contains('.') && value.contains('e') {
        value.replacen('e', ".0e", 1)
    } else {
        value
    }
}

// PyYAML's implicit resolvers (`yaml/resolver.py`), keyed by first character. Python's `$`
// also matches before a final newline, hence `\n?\z`.
static RE_BOOL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?:yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF)\n?\z",
    )
    .expect("bool resolver")
});
static RE_FLOAT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"^(?:[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?",
        r"|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?",
        r"|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*",
        r"|[-+]?\.(?:inf|Inf|INF)",
        r"|\.(?:nan|NaN|NAN))\n?\z"
    ))
    .expect("float resolver")
});
static RE_INT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"^(?:[-+]?0b[0-1_]+",
        r"|[-+]?0[0-7_]+",
        r"|[-+]?(?:0|[1-9][0-9_]*)",
        r"|[-+]?0x[0-9a-fA-F_]+",
        r"|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)\n?\z"
    ))
    .expect("int resolver")
});
static RE_MERGE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:<<)\n?\z").expect("merge resolver"));
static RE_NULL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:~|null|Null|NULL|)\n?\z").expect("null resolver"));
static RE_TIMESTAMP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"^(?:[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]",
        r"|[0-9][0-9][0-9][0-9]-[0-9][0-9]?-[0-9][0-9]?",
        r"(?:[Tt]|[ \t]+)[0-9][0-9]?",
        r":[0-9][0-9]:[0-9][0-9](?:\.[0-9]*)?",
        r"(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9][0-9])?))?)\n?\z"
    ))
    .expect("timestamp resolver")
});
static RE_VALUE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:=)\n?\z").expect("value resolver"));
static RE_YAML: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:!|&|\*)\n?\z").expect("yaml resolver"));

/// The tag a plain scalar with this text would resolve to.
fn resolve_plain(value: &str) -> Tag {
    let first = value.chars().next();
    let candidates: Vec<(&Regex, Tag)> = match first {
        None => vec![(&RE_NULL, Tag::Null)],
        Some(c) => {
            let mut v: Vec<(&Regex, Tag)> = Vec::new();
            if "yYnNtTfFoO".contains(c) {
                v.push((&RE_BOOL, Tag::Bool));
            }
            if "-+0123456789.".contains(c) {
                v.push((&RE_FLOAT, Tag::Float));
            }
            if "-+0123456789".contains(c) {
                v.push((&RE_INT, Tag::Int));
            }
            if c == '<' {
                v.push((&RE_MERGE, Tag::Other));
            }
            if "~nN".contains(c) {
                v.push((&RE_NULL, Tag::Null));
            }
            if c.is_ascii_digit() {
                v.push((&RE_TIMESTAMP, Tag::Other));
            }
            if c == '=' {
                v.push((&RE_VALUE, Tag::Other));
            }
            if "!&*".contains(c) {
                v.push((&RE_YAML, Tag::Other));
            }
            v
        }
    };
    for (re, tag) in candidates {
        if re.is_match(value) {
            return tag;
        }
    }
    Tag::Str
}

#[derive(Debug, Clone, Copy)]
struct Analysis {
    empty: bool,
    multiline: bool,
    allow_flow_plain: bool,
    allow_block_plain: bool,
    allow_single_quoted: bool,
}

const BREAKS: &[char] = &['\n', '\u{85}', '\u{2028}', '\u{2029}'];
const WHITESPACE_OR_BREAK: &[char] = &[
    '\0', ' ', '\t', '\r', '\n', '\u{85}', '\u{2028}', '\u{2029}',
];

fn is_break(c: char) -> bool {
    BREAKS.contains(&c)
}

fn analyze_scalar(scalar: &[char], allow_unicode: bool) -> Analysis {
    if scalar.is_empty() {
        return Analysis {
            empty: true,
            multiline: false,
            allow_flow_plain: false,
            allow_block_plain: true,
            allow_single_quoted: true,
        };
    }
    let mut block_indicators = false;
    let mut flow_indicators = false;
    let mut line_breaks = false;
    let mut special_characters = false;
    let mut leading_space = false;
    let mut leading_break = false;
    let mut trailing_space = false;
    let mut trailing_break = false;
    let mut break_space = false;
    let mut space_break = false;

    let text: String = scalar.iter().collect();
    if text.starts_with("---") || text.starts_with("...") {
        block_indicators = true;
        flow_indicators = true;
    }
    let mut preceded_by_whitespace = true;
    let mut followed_by_whitespace = scalar.len() == 1 || WHITESPACE_OR_BREAK.contains(&scalar[1]);
    let mut previous_space = false;
    let mut previous_break = false;

    let mut index = 0;
    while index < scalar.len() {
        let ch = scalar[index];
        if index == 0 {
            if "#,[]{}&*!|>'\"%@`".contains(ch) {
                flow_indicators = true;
                block_indicators = true;
            }
            if ch == '?' || ch == ':' {
                flow_indicators = true;
                if followed_by_whitespace {
                    block_indicators = true;
                }
            }
            if ch == '-' && followed_by_whitespace {
                flow_indicators = true;
                block_indicators = true;
            }
        } else {
            if ",?[]{}".contains(ch) {
                flow_indicators = true;
            }
            if ch == ':' {
                flow_indicators = true;
                if followed_by_whitespace {
                    block_indicators = true;
                }
            }
            if ch == '#' && preceded_by_whitespace {
                flow_indicators = true;
                block_indicators = true;
            }
        }
        if is_break(ch) {
            line_breaks = true;
        }
        if !(ch == '\n' || ('\x20'..='\x7e').contains(&ch)) {
            let unicode = (ch == '\u{85}'
                || ('\u{a0}'..='\u{d7ff}').contains(&ch)
                || ('\u{e000}'..='\u{fffd}').contains(&ch)
                || ('\u{10000}'..'\u{10ffff}').contains(&ch))
                && ch != '\u{feff}';
            if !unicode || !allow_unicode {
                special_characters = true;
            }
        }
        if ch == ' ' {
            if index == 0 {
                leading_space = true;
            }
            if index == scalar.len() - 1 {
                trailing_space = true;
            }
            if previous_break {
                break_space = true;
            }
            previous_space = true;
            previous_break = false;
        } else if is_break(ch) {
            if index == 0 {
                leading_break = true;
            }
            if index == scalar.len() - 1 {
                trailing_break = true;
            }
            if previous_space {
                space_break = true;
            }
            previous_space = false;
            previous_break = true;
        } else {
            previous_space = false;
            previous_break = false;
        }
        index += 1;
        preceded_by_whitespace = WHITESPACE_OR_BREAK.contains(&ch);
        followed_by_whitespace =
            index + 1 >= scalar.len() || WHITESPACE_OR_BREAK.contains(&scalar[index + 1]);
    }

    let mut allow_flow_plain = true;
    let mut allow_block_plain = true;
    let mut allow_single_quoted = true;
    if leading_space || leading_break || trailing_space || trailing_break {
        allow_flow_plain = false;
        allow_block_plain = false;
    }
    if break_space {
        allow_flow_plain = false;
        allow_block_plain = false;
        allow_single_quoted = false;
    }
    if space_break || special_characters {
        allow_flow_plain = false;
        allow_block_plain = false;
        allow_single_quoted = false;
    }
    if line_breaks {
        allow_flow_plain = false;
        allow_block_plain = false;
    }
    if flow_indicators {
        allow_flow_plain = false;
    }
    if block_indicators {
        allow_block_plain = false;
    }
    Analysis {
        empty: false,
        multiline: line_breaks,
        allow_flow_plain,
        allow_block_plain,
        allow_single_quoted,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Plain,
    Single,
    Double,
}

struct Emitter {
    out: String,
    indents: Vec<Option<usize>>,
    indent: Option<usize>,
    flow_level: usize,
    root_context: bool,
    simple_key_context: bool,
    mapping_context: bool,
    column: usize,
    whitespace: bool,
    indention: bool,
    open_ended: bool,
    best_indent: usize,
    best_width: usize,
    allow_unicode: bool,
}

impl Emitter {
    fn new(options: &DumpOptions) -> Self {
        let best_indent = if options.indent > 1 && options.indent < 10 {
            options.indent
        } else {
            2
        };
        let best_width = if options.width > best_indent * 2 {
            options.width
        } else {
            80
        };
        Emitter {
            out: String::new(),
            indents: Vec::new(),
            indent: None,
            flow_level: 0,
            root_context: false,
            simple_key_context: false,
            mapping_context: false,
            column: 0,
            whitespace: true,
            indention: true,
            open_ended: false,
            best_indent,
            best_width,
            allow_unicode: options.allow_unicode,
        }
    }

    fn emit_document(&mut self, node: &Node, explicit_start: bool) {
        if explicit_start {
            self.write_indent();
            self.write_indicator("---", true, false, false);
        }
        self.expect_node(node, true, false, false);
        // DocumentEndEvent
        self.write_indent();
        // StreamEndEvent
        if self.open_ended {
            self.write_indicator("...", true, false, false);
            self.write_indent();
        }
    }

    fn increase_indent(&mut self, flow: bool, indentless: bool) {
        self.indents.push(self.indent);
        match self.indent {
            None => self.indent = Some(if flow { self.best_indent } else { 0 }),
            Some(i) if !indentless => self.indent = Some(i + self.best_indent),
            Some(_) => {}
        }
    }

    fn pop_indent(&mut self) {
        self.indent = self.indents.pop().unwrap_or(None);
    }

    fn expect_node(&mut self, node: &Node, root: bool, mapping: bool, simple_key: bool) {
        self.root_context = root;
        self.mapping_context = mapping;
        self.simple_key_context = simple_key;
        match node {
            Node::Scalar { tag, value } => {
                let chars: Vec<char> = value.chars().collect();
                let analysis = analyze_scalar(&chars, self.allow_unicode);
                let implicit_plain = resolve_plain(value) == *tag;
                let implicit_quoted = *tag == Tag::Str;
                let style = self.choose_scalar_style(&analysis, implicit_plain);
                if !((style == Style::Plain && implicit_plain)
                    || (style != Style::Plain && implicit_quoted))
                {
                    self.write_indicator(tag.short(), true, false, false);
                }
                self.increase_indent(true, false);
                let split = !self.simple_key_context;
                match style {
                    Style::Double => self.write_double_quoted(&chars, split),
                    Style::Single => self.write_single_quoted(&chars, split),
                    Style::Plain => self.write_plain(&chars, split),
                }
                self.pop_indent();
            }
            Node::Seq { items, flow } => {
                if self.flow_level > 0 || *flow || items.is_empty() {
                    self.expect_flow_sequence(items);
                } else {
                    self.expect_block_sequence(items);
                }
            }
            Node::Map { pairs, flow } => {
                if self.flow_level > 0 || *flow || pairs.is_empty() {
                    self.expect_flow_mapping(pairs);
                } else {
                    self.expect_block_mapping(pairs);
                }
            }
        }
    }

    fn choose_scalar_style(&self, analysis: &Analysis, implicit_plain: bool) -> Style {
        if implicit_plain
            && !(self.simple_key_context && (analysis.empty || analysis.multiline))
            && ((self.flow_level > 0 && analysis.allow_flow_plain)
                || (self.flow_level == 0 && analysis.allow_block_plain))
        {
            return Style::Plain;
        }
        if analysis.allow_single_quoted && !(self.simple_key_context && analysis.multiline) {
            return Style::Single;
        }
        Style::Double
    }

    fn check_simple_key(&self, key: &Node) -> bool {
        match key {
            Node::Scalar { tag, value } => {
                let chars: Vec<char> = value.chars().collect();
                let analysis = analyze_scalar(&chars, self.allow_unicode);
                let length = tag.short_len() + chars.len();
                length < 128 && !analysis.empty && !analysis.multiline
            }
            Node::Seq { items, .. } => items.is_empty(),
            Node::Map { pairs, .. } => pairs.is_empty(),
        }
    }

    fn expect_flow_sequence(&mut self, items: &[Node]) {
        self.write_indicator("[", true, true, false);
        self.flow_level += 1;
        self.increase_indent(true, false);
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.write_indicator(",", false, false, false);
            }
            if self.column > self.best_width {
                self.write_indent();
            }
            self.expect_node(item, false, false, false);
        }
        self.pop_indent();
        self.flow_level -= 1;
        self.write_indicator("]", false, false, false);
    }

    fn expect_flow_mapping(&mut self, pairs: &[(Node, Node)]) {
        self.write_indicator("{", true, true, false);
        self.flow_level += 1;
        self.increase_indent(true, false);
        for (i, (key, value)) in pairs.iter().enumerate() {
            if i > 0 {
                self.write_indicator(",", false, false, false);
            }
            if self.column > self.best_width {
                self.write_indent();
            }
            if self.check_simple_key(key) {
                self.expect_node(key, false, true, true);
                self.write_indicator(":", false, false, false);
                self.expect_node(value, false, true, false);
            } else {
                self.write_indicator("?", true, false, false);
                self.expect_node(key, false, true, false);
                if self.column > self.best_width {
                    self.write_indent();
                }
                self.write_indicator(":", true, false, false);
                self.expect_node(value, false, true, false);
            }
        }
        self.pop_indent();
        self.flow_level -= 1;
        self.write_indicator("}", false, false, false);
    }

    fn expect_block_sequence(&mut self, items: &[Node]) {
        let indentless = self.mapping_context && !self.indention;
        self.increase_indent(false, indentless);
        for item in items {
            self.write_indent();
            self.write_indicator("-", true, false, true);
            self.expect_node(item, false, false, false);
        }
        self.pop_indent();
    }

    fn expect_block_mapping(&mut self, pairs: &[(Node, Node)]) {
        self.increase_indent(false, false);
        for (key, value) in pairs {
            self.write_indent();
            if self.check_simple_key(key) {
                self.expect_node(key, false, true, true);
                self.write_indicator(":", false, false, false);
                self.expect_node(value, false, true, false);
            } else {
                self.write_indicator("?", true, false, true);
                self.expect_node(key, false, true, false);
                self.write_indent();
                self.write_indicator(":", true, false, true);
                self.expect_node(value, false, true, false);
            }
        }
        self.pop_indent();
    }

    fn write_raw(&mut self, chars: &[char]) {
        self.column += chars.len();
        self.out.extend(chars.iter());
    }

    fn write_str(&mut self, data: &str) {
        self.column += data.chars().count();
        self.out.push_str(data);
    }

    fn write_indicator(
        &mut self,
        indicator: &str,
        need_whitespace: bool,
        whitespace: bool,
        indention: bool,
    ) {
        let data = if self.whitespace || !need_whitespace {
            indicator.to_string()
        } else {
            format!(" {indicator}")
        };
        self.whitespace = whitespace;
        self.indention = self.indention && indention;
        self.open_ended = false;
        self.write_str(&data);
    }

    fn write_indent(&mut self) {
        let indent = self.indent.unwrap_or(0);
        if !self.indention || self.column > indent || (self.column == indent && !self.whitespace) {
            self.write_line_break(None);
        }
        if self.column < indent {
            self.whitespace = true;
            let pad = " ".repeat(indent - self.column);
            self.out.push_str(&pad);
            self.column = indent;
        }
    }

    fn write_line_break(&mut self, data: Option<char>) {
        self.whitespace = true;
        self.indention = true;
        self.column = 0;
        self.out.push(data.unwrap_or('\n'));
    }

    fn write_breaks(&mut self, breaks: &[char]) {
        for &br in breaks {
            if br == '\n' {
                self.write_line_break(None);
            } else {
                self.write_line_break(Some(br));
            }
        }
    }

    fn write_plain(&mut self, text: &[char], split: bool) {
        if self.root_context {
            self.open_ended = true;
        }
        if text.is_empty() {
            return;
        }
        if !self.whitespace {
            self.write_str(" ");
        }
        self.whitespace = false;
        self.indention = false;
        let mut spaces = false;
        let mut breaks = false;
        let (mut start, mut end) = (0usize, 0usize);
        while end <= text.len() {
            let ch = text.get(end).copied();
            if spaces {
                if ch != Some(' ') {
                    if start + 1 == end && self.column > self.best_width && split {
                        self.write_indent();
                        self.whitespace = false;
                        self.indention = false;
                    } else {
                        self.write_raw(&text[start..end]);
                    }
                    start = end;
                }
            } else if breaks {
                if !ch.map(is_break).unwrap_or(false) {
                    if text[start] == '\n' {
                        self.write_line_break(None);
                    }
                    self.write_breaks(&text[start..end]);
                    self.write_indent();
                    self.whitespace = false;
                    self.indention = false;
                    start = end;
                }
            } else if ch.is_none() || ch == Some(' ') || ch.map(is_break).unwrap_or(false) {
                self.write_raw(&text[start..end]);
                start = end;
            }
            if let Some(c) = ch {
                spaces = c == ' ';
                breaks = is_break(c);
            }
            end += 1;
        }
    }

    fn write_single_quoted(&mut self, text: &[char], split: bool) {
        self.write_indicator("'", true, false, false);
        let mut spaces = false;
        let mut breaks = false;
        let (mut start, mut end) = (0usize, 0usize);
        while end <= text.len() {
            let ch = text.get(end).copied();
            if spaces {
                if ch != Some(' ') {
                    if start + 1 == end
                        && self.column > self.best_width
                        && split
                        && start != 0
                        && end != text.len()
                    {
                        self.write_indent();
                    } else {
                        self.write_raw(&text[start..end]);
                    }
                    start = end;
                }
            } else if breaks {
                if !ch.map(is_break).unwrap_or(false) {
                    if text[start] == '\n' {
                        self.write_line_break(None);
                    }
                    self.write_breaks(&text[start..end]);
                    self.write_indent();
                    start = end;
                }
            } else if (ch.is_none()
                || ch == Some(' ')
                || ch.map(is_break).unwrap_or(false)
                || ch == Some('\''))
                && start < end
            {
                self.write_raw(&text[start..end]);
                start = end;
            }
            if ch == Some('\'') {
                self.write_str("''");
                start = end + 1;
            }
            if let Some(c) = ch {
                spaces = c == ' ';
                breaks = is_break(c);
            }
            end += 1;
        }
        self.write_indicator("'", false, false, false);
    }

    fn write_double_quoted(&mut self, text: &[char], split: bool) {
        self.write_indicator("\"", true, false, false);
        let len = text.len() as isize;
        let (mut start, mut end) = (0isize, 0isize);
        while end <= len {
            let ch = if end < len {
                Some(text[end as usize])
            } else {
                None
            };
            let needs_escape = match ch {
                None => true,
                Some(c) => {
                    matches!(
                        c,
                        '"' | '\\' | '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{feff}'
                    ) || !(('\x20'..='\x7e').contains(&c)
                        || (self.allow_unicode
                            && (('\u{a0}'..='\u{d7ff}').contains(&c)
                                || ('\u{e000}'..='\u{fffd}').contains(&c))))
                }
            };
            if needs_escape {
                if start < end {
                    self.write_raw(&text[start as usize..end as usize]);
                    start = end;
                }
                if let Some(c) = ch {
                    let data = match escape_replacement(c) {
                        Some(r) => format!("\\{r}"),
                        None => {
                            let code = c as u32;
                            if code <= 0xff {
                                format!("\\x{code:02X}")
                            } else if code <= 0xffff {
                                format!("\\u{code:04X}")
                            } else {
                                format!("\\U{code:08X}")
                            }
                        }
                    };
                    self.write_str(&data);
                    start = end + 1;
                }
            }
            if 0 < end
                && end < len - 1
                && (ch == Some(' ') || start >= end)
                && self.column as isize + (end - start) > self.best_width as isize
                && split
            {
                let mut data: String = if start < end {
                    text[start as usize..end as usize].iter().collect()
                } else {
                    String::new()
                };
                data.push('\\');
                if start < end {
                    start = end;
                }
                self.write_str(&data);
                self.write_indent();
                self.whitespace = false;
                self.indention = false;
                if text[start as usize] == ' ' {
                    self.write_str("\\");
                }
            }
            end += 1;
        }
        self.write_indicator("\"", false, false, false);
    }
}

fn escape_replacement(c: char) -> Option<char> {
    Some(match c {
        '\0' => '0',
        '\x07' => 'a',
        '\x08' => 'b',
        '\x09' => 't',
        '\x0a' => 'n',
        '\x0b' => 'v',
        '\x0c' => 'f',
        '\x0d' => 'r',
        '\x1b' => 'e',
        '"' => '"',
        '\\' => '\\',
        '\u{85}' => 'N',
        '\u{a0}' => '_',
        '\u{2028}' => 'L',
        '\u{2029}' => 'P',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_mapping_with_sorted_keys() {
        let v = json!({"b": [1, 2], "a": {"x": "y"}, "c": []});
        assert_eq!(dump_default(&v), "a:\n  x: y\nb:\n- 1\n- 2\nc: []\n");
    }

    #[test]
    fn plain_root_scalar_is_open_ended() {
        assert_eq!(dump_default(&json!("hello")), "hello\n...\n");
        assert_eq!(dump_default(&json!("")), "''\n");
        assert_eq!(dump_default(&json!(1e16)), "1.0e+16\n...\n");
    }

    #[test]
    fn ambiguous_strings_are_quoted() {
        let v = json!({"k": ["yes", "123", "1e10", "\u{e9}", "a\tb", "it's"]});
        assert_eq!(
            dump_default(&v),
            "k:\n- 'yes'\n- '123'\n- 1e10\n- \"\\xE9\"\n- \"a\\tb\"\n- it's\n"
        );
    }
}
