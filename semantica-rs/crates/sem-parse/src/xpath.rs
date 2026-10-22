//! An XPath 1.0 evaluator over the XML tree of [`crate::xml_parser`], for
//! `XMLParser.find_elements` (lxml's `root.xpath(expr, namespaces=...)`).
//!
//! Supported: the full expression grammar (`or`, `and`, comparisons, arithmetic, unary minus,
//! unions, filter expressions with predicates, location paths with `/` and `//`), all axes
//! except `namespace`, name tests (`name`, `prefix:name`, `prefix:*`, `*`), node-type tests
//! (`node()`, `text()`, `comment()`, `processing-instruction()`), and the XPath 1.0 core
//! function library. Errors carry lxml's `XPathEvalError` messages (`Invalid expression`,
//! `Undefined namespace prefix`, `Unregistered function`, `Undefined variable`,
//! `Invalid number of arguments`, `Invalid type`).

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::xml_parser::reader::{Document, NodeId, NodeKind};

/// A node in the XPath data model: a tree node or an attribute of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum XNode {
    /// A document, element, text, comment or processing-instruction node.
    Node(NodeId),
    /// The `index`th attribute of element `element`.
    Attr(NodeId, usize),
}

impl XNode {
    fn order_key(self) -> (NodeId, usize) {
        match self {
            XNode::Node(n) => (n, 0),
            XNode::Attr(e, i) => (e, i + 1),
        }
    }
}

/// An XPath value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum XValue {
    NodeSet(Vec<XNode>),
    Bool(bool),
    Number(f64),
    Str(String),
}

/// An evaluation error (lxml's `XPathEvalError` message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XPathError(pub(crate) String);

type XResult<T> = Result<T, XPathError>;

fn invalid() -> XPathError {
    XPathError("Invalid expression".to_string())
}

// ------------------------------------------------------------------------------- lexer

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    DotDot,
    At,
    Comma,
    ColonColon,
    Slash,
    SlashSlash,
    Pipe,
    Plus,
    Minus,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Star,
    Literal(String),
    Number(f64),
    Dollar(String),
    /// A name test or function/axis/operator name: (prefix, local) where local may be `*`.
    Name(Option<String>, String),
}

fn is_ncname_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ncname_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '\u{B7}')
}

fn lex(expr: &str) -> XResult<Vec<Tok>> {
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    let mut toks: Vec<Tok> = Vec::new();
    let ncname = |chars: &[char], i: &mut usize| -> String {
        let start = *i;
        while *i < chars.len() && is_ncname_char(chars[*i]) {
            *i += 1;
        }
        chars[start..*i].iter().collect()
    };
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let next = chars.get(i + 1).copied();
        let tok = match c {
            '(' => {
                i += 1;
                Tok::LParen
            }
            ')' => {
                i += 1;
                Tok::RParen
            }
            '[' => {
                i += 1;
                Tok::LBracket
            }
            ']' => {
                i += 1;
                Tok::RBracket
            }
            '@' => {
                i += 1;
                Tok::At
            }
            ',' => {
                i += 1;
                Tok::Comma
            }
            '|' => {
                i += 1;
                Tok::Pipe
            }
            '+' => {
                i += 1;
                Tok::Plus
            }
            '-' => {
                i += 1;
                Tok::Minus
            }
            '=' => {
                i += 1;
                Tok::Eq
            }
            '!' if next == Some('=') => {
                i += 2;
                Tok::Ne
            }
            '<' if next == Some('=') => {
                i += 2;
                Tok::Le
            }
            '<' => {
                i += 1;
                Tok::Lt
            }
            '>' if next == Some('=') => {
                i += 2;
                Tok::Ge
            }
            '>' => {
                i += 1;
                Tok::Gt
            }
            '/' if next == Some('/') => {
                i += 2;
                Tok::SlashSlash
            }
            '/' => {
                i += 1;
                Tok::Slash
            }
            ':' if next == Some(':') => {
                i += 2;
                Tok::ColonColon
            }
            '*' => {
                i += 1;
                Tok::Star
            }
            '"' | '\'' => {
                let end = chars[i + 1..].iter().position(|&x| x == c).ok_or_else(invalid)?;
                let s: String = chars[i + 1..i + 1 + end].iter().collect();
                i += end + 2;
                Tok::Literal(s)
            }
            '.' if next == Some('.') => {
                i += 2;
                Tok::DotDot
            }
            '.' if next.is_some_and(|d| d.is_ascii_digit()) => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                Tok::Number(s.parse().map_err(|_| invalid())?)
            }
            '.' => {
                i += 1;
                Tok::Dot
            }
            d if d.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i < chars.len() && chars[i] == '.' {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let s: String = chars[start..i].iter().collect();
                Tok::Number(s.parse().map_err(|_| invalid())?)
            }
            '$' => {
                i += 1;
                if i >= chars.len() || !is_ncname_start(chars[i]) {
                    return Err(invalid());
                }
                let mut name = ncname(&chars, &mut i);
                if i + 1 < chars.len() && chars[i] == ':' && is_ncname_start(chars[i + 1]) {
                    i += 1;
                    name = format!("{name}:{}", ncname(&chars, &mut i));
                }
                Tok::Dollar(name)
            }
            c if is_ncname_start(c) => {
                let first = ncname(&chars, &mut i);
                if i < chars.len() && chars[i] == ':' && chars.get(i + 1) != Some(&':') {
                    if chars.get(i + 1) == Some(&'*') {
                        i += 2;
                        Tok::Name(Some(first), "*".to_string())
                    } else if chars.get(i + 1).is_some_and(|&c| is_ncname_start(c)) {
                        i += 1;
                        let local = ncname(&chars, &mut i);
                        Tok::Name(Some(first), local)
                    } else {
                        return Err(invalid());
                    }
                } else {
                    Tok::Name(None, first)
                }
            }
            _ => return Err(invalid()),
        };
        toks.push(tok);
    }
    // Disambiguation (XPath 1.0 §3.7): when the preceding token is not `@`, `::`, `(`, `[`,
    // `,` or an operator, `*` is the multiplication operator and an NCName is an operator
    // name. Operators are marked with a leading NUL.
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    for tok in toks {
        let as_operator = !prev_is_operator(&out);
        let tok = match tok {
            Tok::Star if as_operator => Tok::Name(None, "\u{0}mul".to_string()),
            Tok::Name(None, n) if as_operator && is_operator_name(&n) => Tok::Name(None, format!("\u{0}{n}")),
            other => other,
        };
        out.push(tok);
    }
    Ok(out)
}

fn is_operator_name(n: &str) -> bool {
    matches!(n, "and" | "or" | "mod" | "div")
}

/// Whether the previous token is itself an operator (so the current token starts an operand).
fn prev_is_operator(out: &[Tok]) -> bool {
    match out.last() {
        Some(Tok::Name(None, n)) => n.starts_with('\u{0}'),
        Some(
            Tok::At
            | Tok::ColonColon
            | Tok::LParen
            | Tok::LBracket
            | Tok::Comma
            | Tok::Slash
            | Tok::SlashSlash
            | Tok::Pipe
            | Tok::Plus
            | Tok::Minus
            | Tok::Eq
            | Tok::Ne
            | Tok::Lt
            | Tok::Le
            | Tok::Gt
            | Tok::Ge,
        ) => true,
        None => true,
        _ => false,
    }
}

// ------------------------------------------------------------------------------ parser

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Ancestor,
    AncestorOrSelf,
    Attribute,
    Child,
    Descendant,
    DescendantOrSelf,
    Following,
    FollowingSibling,
    Namespace,
    Parent,
    Preceding,
    PrecedingSibling,
    SelfAxis,
}

impl Axis {
    fn from_name(n: &str) -> Option<Axis> {
        Some(match n {
            "ancestor" => Axis::Ancestor,
            "ancestor-or-self" => Axis::AncestorOrSelf,
            "attribute" => Axis::Attribute,
            "child" => Axis::Child,
            "descendant" => Axis::Descendant,
            "descendant-or-self" => Axis::DescendantOrSelf,
            "following" => Axis::Following,
            "following-sibling" => Axis::FollowingSibling,
            "namespace" => Axis::Namespace,
            "parent" => Axis::Parent,
            "preceding" => Axis::Preceding,
            "preceding-sibling" => Axis::PrecedingSibling,
            "self" => Axis::SelfAxis,
            _ => return None,
        })
    }

}

#[derive(Debug, Clone, PartialEq)]
enum NodeTest {
    /// `*` or `prefix:*` (namespace URI resolved at evaluation).
    AnyName(Option<String>),
    /// A (possibly prefixed) name.
    Name(Option<String>, String),
    Node,
    Text,
    Comment,
    Pi(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
struct Step {
    axis: Axis,
    test: NodeTest,
    predicates: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Cmp(String, Box<Expr>, Box<Expr>),
    Arith(char, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Union(Box<Expr>, Box<Expr>),
    /// A location path: absolute or relative steps.
    Path { absolute: bool, steps: Vec<Step> },
    /// A filter expression followed by optional steps.
    Filter {
        primary: Box<Expr>,
        predicates: Vec<Expr>,
        steps: Vec<Step>,
    },
    Literal(String),
    Number(f64),
    Variable(String),
    Call(String, Vec<Expr>),
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_op(&mut self, name: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Name(None, n)) if n.strip_prefix('\u{0}') == Some(name)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> XResult<Expr> {
        let mut left = self.and_expr()?;
        while self.eat_op("or") {
            let right = self.and_expr()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> XResult<Expr> {
        let mut left = self.equality()?;
        while self.eat_op("and") {
            let right = self.equality()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn equality(&mut self) -> XResult<Expr> {
        let mut left = self.relational()?;
        loop {
            let op = if self.eat(&Tok::Eq) {
                "="
            } else if self.eat(&Tok::Ne) {
                "!="
            } else {
                break;
            };
            let right = self.relational()?;
            left = Expr::Cmp(op.to_string(), Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn relational(&mut self) -> XResult<Expr> {
        let mut left = self.additive()?;
        loop {
            let op = if self.eat(&Tok::Lt) {
                "<"
            } else if self.eat(&Tok::Le) {
                "<="
            } else if self.eat(&Tok::Gt) {
                ">"
            } else if self.eat(&Tok::Ge) {
                ">="
            } else {
                break;
            };
            let right = self.additive()?;
            left = Expr::Cmp(op.to_string(), Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn additive(&mut self) -> XResult<Expr> {
        let mut left = self.multiplicative()?;
        loop {
            let op = if self.eat(&Tok::Plus) {
                '+'
            } else if self.eat(&Tok::Minus) {
                '-'
            } else {
                break;
            };
            let right = self.multiplicative()?;
            left = Expr::Arith(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn multiplicative(&mut self) -> XResult<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat_op("mul") {
                '*'
            } else if self.eat_op("div") {
                '/'
            } else if self.eat_op("mod") {
                '%'
            } else {
                break;
            };
            let right = self.unary()?;
            left = Expr::Arith(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> XResult<Expr> {
        if self.eat(&Tok::Minus) {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.union()
    }

    fn union(&mut self) -> XResult<Expr> {
        let mut left = self.path_expr()?;
        while self.eat(&Tok::Pipe) {
            let right = self.path_expr()?;
            left = Expr::Union(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn starts_primary(&self) -> bool {
        match self.peek() {
            Some(Tok::LParen | Tok::Literal(_) | Tok::Number(_) | Tok::Dollar(_)) => true,
            Some(Tok::Name(prefix, n)) => {
                let is_call = self.toks.get(self.pos + 1) == Some(&Tok::LParen);
                let node_type = prefix.is_none()
                    && matches!(n.as_str(), "node" | "text" | "comment" | "processing-instruction");
                is_call && !node_type && !n.starts_with('\u{0}')
            }
            _ => false,
        }
    }

    fn path_expr(&mut self) -> XResult<Expr> {
        if self.starts_primary() {
            let primary = self.primary()?;
            let mut predicates = Vec::new();
            while self.eat(&Tok::LBracket) {
                predicates.push(self.expr()?);
                if !self.eat(&Tok::RBracket) {
                    return Err(invalid());
                }
            }
            let mut steps = Vec::new();
            loop {
                if self.eat(&Tok::Slash) {
                    steps.push(self.step()?);
                } else if self.eat(&Tok::SlashSlash) {
                    steps.push(descendant_or_self());
                    steps.push(self.step()?);
                } else {
                    break;
                }
            }
            if predicates.is_empty() && steps.is_empty() {
                return Ok(primary);
            }
            return Ok(Expr::Filter {
                primary: Box::new(primary),
                predicates,
                steps,
            });
        }
        self.location_path()
    }

    fn primary(&mut self) -> XResult<Expr> {
        match self.next() {
            Some(Tok::LParen) => {
                let e = self.expr()?;
                if !self.eat(&Tok::RParen) {
                    return Err(invalid());
                }
                Ok(e)
            }
            Some(Tok::Literal(s)) => Ok(Expr::Literal(s)),
            Some(Tok::Number(n)) => Ok(Expr::Number(n)),
            Some(Tok::Dollar(v)) => Ok(Expr::Variable(v)),
            Some(Tok::Name(prefix, n)) => {
                let name = match prefix {
                    Some(p) => format!("{p}:{n}"),
                    None => n,
                };
                self.eat(&Tok::LParen);
                let mut args = Vec::new();
                if !self.eat(&Tok::RParen) {
                    loop {
                        args.push(self.expr()?);
                        if self.eat(&Tok::Comma) {
                            continue;
                        }
                        if self.eat(&Tok::RParen) {
                            break;
                        }
                        return Err(invalid());
                    }
                }
                Ok(Expr::Call(name, args))
            }
            _ => Err(invalid()),
        }
    }

    fn location_path(&mut self) -> XResult<Expr> {
        let mut steps = Vec::new();
        let absolute;
        if self.eat(&Tok::Slash) {
            absolute = true;
            if self.starts_step() {
                steps.push(self.step()?);
            } else {
                return Ok(Expr::Path { absolute, steps });
            }
        } else if self.eat(&Tok::SlashSlash) {
            absolute = true;
            steps.push(descendant_or_self());
            steps.push(self.step()?);
        } else {
            absolute = false;
            steps.push(self.step()?);
        }
        loop {
            if self.eat(&Tok::Slash) {
                steps.push(self.step()?);
            } else if self.eat(&Tok::SlashSlash) {
                steps.push(descendant_or_self());
                steps.push(self.step()?);
            } else {
                break;
            }
        }
        Ok(Expr::Path { absolute, steps })
    }

    fn starts_step(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Dot | Tok::DotDot | Tok::At | Tok::Star | Tok::Name(..))
        ) && !matches!(self.peek(), Some(Tok::Name(None, n)) if n.starts_with('\u{0}'))
    }

    fn step(&mut self) -> XResult<Step> {
        if self.eat(&Tok::Dot) {
            return Ok(Step {
                axis: Axis::SelfAxis,
                test: NodeTest::Node,
                predicates: Vec::new(),
            });
        }
        if self.eat(&Tok::DotDot) {
            return Ok(Step {
                axis: Axis::Parent,
                test: NodeTest::Node,
                predicates: Vec::new(),
            });
        }
        let mut axis = Axis::Child;
        if self.eat(&Tok::At) {
            axis = Axis::Attribute;
        } else if let Some(Tok::Name(None, n)) = self.peek().cloned()
            && self.toks.get(self.pos + 1) == Some(&Tok::ColonColon)
        {
            axis = Axis::from_name(&n).ok_or_else(invalid)?;
            self.pos += 2;
        }
        let test = match self.next() {
            Some(Tok::Star) => NodeTest::AnyName(None),
            Some(Tok::Name(prefix, n)) => {
                if prefix.is_none() && self.peek() == Some(&Tok::LParen) {
                    self.pos += 1;
                    let test = match n.as_str() {
                        "node" => NodeTest::Node,
                        "text" => NodeTest::Text,
                        "comment" => NodeTest::Comment,
                        "processing-instruction" => {
                            if let Some(Tok::Literal(s)) = self.peek().cloned() {
                                self.pos += 1;
                                NodeTest::Pi(Some(s))
                            } else {
                                NodeTest::Pi(None)
                            }
                        }
                        _ => return Err(invalid()),
                    };
                    if !self.eat(&Tok::RParen) {
                        return Err(invalid());
                    }
                    test
                } else if n == "*" {
                    NodeTest::AnyName(prefix)
                } else {
                    NodeTest::Name(prefix, n)
                }
            }
            _ => return Err(invalid()),
        };
        let mut predicates = Vec::new();
        while self.eat(&Tok::LBracket) {
            predicates.push(self.expr()?);
            if !self.eat(&Tok::RBracket) {
                return Err(invalid());
            }
        }
        Ok(Step { axis, test, predicates })
    }
}

fn descendant_or_self() -> Step {
    Step {
        axis: Axis::DescendantOrSelf,
        test: NodeTest::Node,
        predicates: Vec::new(),
    }
}

/// Compile an XPath expression.
fn compile(expr: &str) -> XResult<Expr> {
    let toks = lex(expr)?;
    if toks.is_empty() {
        return Err(invalid());
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    if p.pos != p.toks.len() {
        return Err(invalid());
    }
    Ok(e)
}

// ---------------------------------------------------------------------------- evaluation

struct Ctx<'a> {
    doc: &'a Document,
    namespaces: &'a HashMap<String, String>,
}

#[derive(Clone, Copy)]
struct Focus {
    node: XNode,
    position: usize,
    size: usize,
}

impl Ctx<'_> {
    fn parent(&self, n: XNode) -> Option<XNode> {
        match n {
            XNode::Node(id) => self.doc.nodes[id].parent.map(XNode::Node),
            XNode::Attr(e, _) => Some(XNode::Node(e)),
        }
    }

    fn children(&self, n: XNode) -> Vec<XNode> {
        match n {
            XNode::Node(id) => self.doc.nodes[id].children.iter().map(|&c| XNode::Node(c)).collect(),
            XNode::Attr(..) => Vec::new(),
        }
    }

    fn descendants(&self, n: XNode, out: &mut Vec<XNode>) {
        for c in self.children(n) {
            out.push(c);
            self.descendants(c, out);
        }
    }

    fn attributes(&self, n: XNode) -> Vec<XNode> {
        match n {
            XNode::Node(id) => match &self.doc.nodes[id].kind {
                NodeKind::Element { attrs, .. } => (0..attrs.len()).map(|i| XNode::Attr(id, i)).collect(),
                _ => Vec::new(),
            },
            XNode::Attr(..) => Vec::new(),
        }
    }

    fn axis_nodes(&self, n: XNode, axis: Axis) -> Vec<XNode> {
        match axis {
            Axis::Child => self.children(n),
            Axis::Descendant => {
                let mut out = Vec::new();
                self.descendants(n, &mut out);
                out
            }
            Axis::DescendantOrSelf => {
                let mut out = vec![n];
                self.descendants(n, &mut out);
                out
            }
            Axis::Parent => self.parent(n).into_iter().collect(),
            Axis::Ancestor | Axis::AncestorOrSelf => {
                let mut out = Vec::new();
                if axis == Axis::AncestorOrSelf {
                    out.push(n);
                }
                let mut cur = self.parent(n);
                while let Some(p) = cur {
                    out.push(p);
                    cur = self.parent(p);
                }
                out
            }
            Axis::FollowingSibling | Axis::PrecedingSibling => {
                let XNode::Node(id) = n else {
                    return Vec::new();
                };
                let Some(parent) = self.doc.nodes[id].parent else {
                    return Vec::new();
                };
                let siblings = &self.doc.nodes[parent].children;
                let pos = siblings.iter().position(|&c| c == id).unwrap_or(0);
                if axis == Axis::FollowingSibling {
                    siblings[pos + 1..].iter().map(|&c| XNode::Node(c)).collect()
                } else {
                    siblings[..pos].iter().rev().map(|&c| XNode::Node(c)).collect()
                }
            }
            Axis::Following => {
                let anchor = match n {
                    XNode::Attr(e, _) => XNode::Node(e),
                    other => other,
                };
                let mut descendants_of_anchor = Vec::new();
                self.descendants(anchor, &mut descendants_of_anchor);
                let mut all = Vec::new();
                self.descendants(XNode::Node(0), &mut all);
                all.into_iter()
                    .filter(|m| m.order_key() > anchor.order_key() && !descendants_of_anchor.contains(m))
                    .collect()
            }
            Axis::Preceding => {
                let anchor = match n {
                    XNode::Attr(e, _) => XNode::Node(e),
                    other => other,
                };
                let ancestors = self.axis_nodes(anchor, Axis::Ancestor);
                let mut all = Vec::new();
                self.descendants(XNode::Node(0), &mut all);
                let mut out: Vec<XNode> = all
                    .into_iter()
                    .filter(|m| m.order_key() < anchor.order_key() && !ancestors.contains(m))
                    .collect();
                out.reverse();
                out
            }
            Axis::Attribute => self.attributes(n),
            Axis::Namespace => Vec::new(),
            Axis::SelfAxis => vec![n],
        }
    }

    fn resolve_prefix(&self, prefix: &str) -> XResult<String> {
        if prefix == "xml" {
            return Ok("http://www.w3.org/XML/1998/namespace".to_string());
        }
        self.namespaces
            .get(prefix)
            .cloned()
            .ok_or_else(|| XPathError("Undefined namespace prefix".to_string()))
    }

    fn test_matches(&self, n: XNode, test: &NodeTest, axis: Axis) -> XResult<bool> {
        let principal_attr = axis == Axis::Attribute;
        Ok(match test {
            NodeTest::Node => true,
            NodeTest::Text => matches!(n, XNode::Node(id) if matches!(self.doc.nodes[id].kind, NodeKind::Text(_))),
            NodeTest::Comment => {
                matches!(n, XNode::Node(id) if matches!(self.doc.nodes[id].kind, NodeKind::Comment(_)))
            }
            NodeTest::Pi(target) => match n {
                XNode::Node(id) => match &self.doc.nodes[id].kind {
                    NodeKind::Pi { target: t, .. } => target.as_ref().is_none_or(|x| x == t),
                    _ => false,
                },
                _ => false,
            },
            NodeTest::AnyName(prefix) => {
                let uri = match prefix {
                    Some(p) => Some(self.resolve_prefix(p)?),
                    None => None,
                };
                match self.name_parts(n) {
                    Some((ns, _)) if principal_attr == matches!(n, XNode::Attr(..)) => match &uri {
                        Some(u) => ns.as_deref() == Some(u.as_str()),
                        None => true,
                    },
                    _ => false,
                }
            }
            NodeTest::Name(prefix, local) => {
                let uri = match prefix {
                    Some(p) => Some(self.resolve_prefix(p)?),
                    None => None,
                };
                match self.name_parts(n) {
                    Some((ns, l)) if principal_attr == matches!(n, XNode::Attr(..)) => {
                        &l == local && ns.as_deref() == uri.as_deref()
                    }
                    _ => false,
                }
            }
        })
    }

    /// `(namespace, local-name)` of an element or attribute.
    fn name_parts(&self, n: XNode) -> Option<(Option<String>, String)> {
        match n {
            XNode::Node(id) => match &self.doc.nodes[id].kind {
                NodeKind::Element { local, namespace, .. } => Some((namespace.clone(), local.clone())),
                _ => None,
            },
            XNode::Attr(e, i) => match &self.doc.nodes[e].kind {
                NodeKind::Element { attrs, .. } => attrs.get(i).map(|a| (a.namespace.clone(), a.local.clone())),
                _ => None,
            },
        }
    }

    fn qualified_name(&self, n: XNode) -> String {
        match n {
            XNode::Node(id) => match &self.doc.nodes[id].kind {
                NodeKind::Element { prefix, local, .. } => match prefix {
                    Some(p) => format!("{p}:{local}"),
                    None => local.clone(),
                },
                NodeKind::Pi { target, .. } => target.clone(),
                _ => String::new(),
            },
            XNode::Attr(e, i) => match &self.doc.nodes[e].kind {
                NodeKind::Element { attrs, .. } => attrs.get(i).map(|a| a.qname()).unwrap_or_default(),
                _ => String::new(),
            },
        }
    }

    /// The string-value of a node.
    fn string_value(&self, n: XNode) -> String {
        match n {
            XNode::Attr(e, i) => match &self.doc.nodes[e].kind {
                NodeKind::Element { attrs, .. } => attrs.get(i).map(|a| a.value.clone()).unwrap_or_default(),
                _ => String::new(),
            },
            XNode::Node(id) => match &self.doc.nodes[id].kind {
                NodeKind::Text(t) | NodeKind::Comment(t) => t.clone(),
                NodeKind::Pi { data, .. } => data.clone(),
                NodeKind::Document | NodeKind::Element { .. } => {
                    let mut out = String::new();
                    let mut all = Vec::new();
                    self.descendants(n, &mut all);
                    for d in all {
                        if let XNode::Node(i) = d
                            && let NodeKind::Text(t) = &self.doc.nodes[i].kind
                        {
                            out.push_str(t);
                        }
                    }
                    out
                }
            },
        }
    }

    fn eval_steps(&self, start: Vec<XNode>, steps: &[Step]) -> XResult<Vec<XNode>> {
        let mut current = start;
        for step in steps {
            let mut next: Vec<XNode> = Vec::new();
            for &n in &current {
                let mut candidates = Vec::new();
                for m in self.axis_nodes(n, step.axis) {
                    if self.test_matches(m, &step.test, step.axis)? {
                        candidates.push(m);
                    }
                }
                // Predicates filter in axis order (proximity positions).
                for pred in &step.predicates {
                    candidates = self.filter(candidates, pred)?;
                }
                next.extend(candidates);
            }
            current = sort_unique(next);
        }
        Ok(current)
    }

    fn filter(&self, nodes: Vec<XNode>, pred: &Expr) -> XResult<Vec<XNode>> {
        let size = nodes.len();
        let mut out = Vec::new();
        for (i, n) in nodes.into_iter().enumerate() {
            let focus = Focus {
                node: n,
                position: i + 1,
                size,
            };
            let v = self.eval(pred, focus)?;
            let keep = match v {
                XValue::Number(num) => num == (i + 1) as f64,
                other => self.boolean(&other),
            };
            if keep {
                out.push(n);
            }
        }
        Ok(out)
    }

    fn eval(&self, e: &Expr, f: Focus) -> XResult<XValue> {
        Ok(match e {
            Expr::Or(a, b) => XValue::Bool(self.boolean(&self.eval(a, f)?) || self.boolean(&self.eval(b, f)?)),
            Expr::And(a, b) => XValue::Bool(self.boolean(&self.eval(a, f)?) && self.boolean(&self.eval(b, f)?)),
            Expr::Cmp(op, a, b) => {
                let l = self.eval(a, f)?;
                let r = self.eval(b, f)?;
                XValue::Bool(self.compare(op, &l, &r))
            }
            Expr::Arith(op, a, b) => {
                let l = self.number(&self.eval(a, f)?);
                let r = self.number(&self.eval(b, f)?);
                XValue::Number(match op {
                    '+' => l + r,
                    '-' => l - r,
                    '*' => l * r,
                    '/' => l / r,
                    _ => l % r,
                })
            }
            Expr::Neg(a) => XValue::Number(-self.number(&self.eval(a, f)?)),
            Expr::Union(a, b) => {
                let (XValue::NodeSet(mut l), XValue::NodeSet(r)) = (self.eval(a, f)?, self.eval(b, f)?) else {
                    return Err(XPathError("Invalid type".to_string()));
                };
                l.extend(r);
                XValue::NodeSet(sort_unique(l))
            }
            Expr::Path { absolute, steps } => {
                let start = if *absolute { XNode::Node(0) } else { f.node };
                XValue::NodeSet(self.eval_steps(vec![start], steps)?)
            }
            Expr::Filter {
                primary,
                predicates,
                steps,
            } => {
                let v = self.eval(primary, f)?;
                let XValue::NodeSet(mut nodes) = v else {
                    if predicates.is_empty() && steps.is_empty() {
                        return Ok(v);
                    }
                    return Err(XPathError("Invalid type".to_string()));
                };
                for p in predicates {
                    nodes = self.filter(nodes, p)?;
                }
                XValue::NodeSet(self.eval_steps(nodes, steps)?)
            }
            Expr::Literal(s) => XValue::Str(s.clone()),
            Expr::Number(n) => XValue::Number(*n),
            Expr::Variable(_) => return Err(XPathError("Undefined variable".to_string())),
            Expr::Call(name, args) => self.call(name, args, f)?,
        })
    }

    fn boolean(&self, v: &XValue) -> bool {
        match v {
            XValue::NodeSet(n) => !n.is_empty(),
            XValue::Bool(b) => *b,
            XValue::Number(n) => *n != 0.0 && !n.is_nan(),
            XValue::Str(s) => !s.is_empty(),
        }
    }

    fn string(&self, v: &XValue) -> String {
        match v {
            XValue::NodeSet(n) => n.first().map(|&x| self.string_value(x)).unwrap_or_default(),
            XValue::Bool(b) => b.to_string(),
            XValue::Number(n) => number_to_string(*n),
            XValue::Str(s) => s.clone(),
        }
    }

    fn number(&self, v: &XValue) -> f64 {
        match v {
            XValue::Number(n) => *n,
            XValue::Bool(b) => f64::from(u8::from(*b)),
            other => string_to_number(&self.string(other)),
        }
    }

    fn compare(&self, op: &str, l: &XValue, r: &XValue) -> bool {
        match (l, r) {
            (XValue::NodeSet(a), XValue::NodeSet(b)) => a.iter().any(|&x| {
                let sx = self.string_value(x);
                b.iter().any(|&y| compare_atoms(op, &XValue::Str(sx.clone()), &XValue::Str(self.string_value(y)), self))
            }),
            (XValue::NodeSet(a), other) => a.iter().any(|&x| {
                let sv = XValue::Str(self.string_value(x));
                match other {
                    XValue::Number(_) => compare_atoms(op, &XValue::Number(self.number(&sv)), other, self),
                    XValue::Bool(_) => compare_atoms(op, &XValue::Bool(self.boolean(&XValue::NodeSet(a.clone()))), other, self),
                    _ => compare_atoms(op, &sv, other, self),
                }
            }),
            (other, XValue::NodeSet(_)) => {
                let flipped = match op {
                    "<" => ">",
                    "<=" => ">=",
                    ">" => "<",
                    ">=" => "<=",
                    o => o,
                };
                self.compare(flipped, r, other)
            }
            _ => compare_atoms(op, l, r, self),
        }
    }

    fn call(&self, name: &str, args: &[Expr], f: Focus) -> XResult<XValue> {
        let argc = args.len();
        let arity = |min: usize, max: usize| -> XResult<()> {
            if argc < min || argc > max {
                Err(XPathError("Invalid number of arguments".to_string()))
            } else {
                Ok(())
            }
        };
        let eval_arg = |i: usize| self.eval(&args[i], f);
        let node_arg = |i: usize| -> XResult<Option<XNode>> {
            match eval_arg(i)? {
                XValue::NodeSet(n) => Ok(n.first().copied()),
                _ => Err(XPathError("Invalid type".to_string())),
            }
        };
        let str_arg = |i: usize| -> XResult<String> { Ok(self.string(&eval_arg(i)?)) };
        Ok(match name {
            "last" => {
                arity(0, 0)?;
                XValue::Number(f.size as f64)
            }
            "position" => {
                arity(0, 0)?;
                XValue::Number(f.position as f64)
            }
            "count" => {
                arity(1, 1)?;
                match eval_arg(0)? {
                    XValue::NodeSet(n) => XValue::Number(n.len() as f64),
                    _ => return Err(XPathError("Invalid type".to_string())),
                }
            }
            "id" => {
                arity(1, 1)?;
                XValue::NodeSet(Vec::new())
            }
            "local-name" | "name" | "namespace-uri" => {
                arity(0, 1)?;
                let n = if argc == 0 { Some(f.node) } else { node_arg(0)? };
                XValue::Str(match n {
                    None => String::new(),
                    Some(n) => match name {
                        "local-name" => match n {
                            XNode::Node(id) if matches!(self.doc.nodes[id].kind, NodeKind::Pi { .. }) => {
                                self.qualified_name(n)
                            }
                            _ => self.name_parts(n).map(|(_, l)| l).unwrap_or_default(),
                        },
                        "name" => self.qualified_name(n),
                        _ => self.name_parts(n).and_then(|(ns, _)| ns).unwrap_or_default(),
                    },
                })
            }
            "string" => {
                arity(0, 1)?;
                XValue::Str(if argc == 0 {
                    self.string_value(f.node)
                } else {
                    str_arg(0)?
                })
            }
            "concat" => {
                if argc < 2 {
                    return Err(XPathError("Invalid number of arguments".to_string()));
                }
                let mut s = String::new();
                for i in 0..argc {
                    s.push_str(&str_arg(i)?);
                }
                XValue::Str(s)
            }
            "starts-with" => {
                arity(2, 2)?;
                XValue::Bool(str_arg(0)?.starts_with(&str_arg(1)?))
            }
            "contains" => {
                arity(2, 2)?;
                XValue::Bool(str_arg(0)?.contains(&str_arg(1)?))
            }
            "substring-before" => {
                arity(2, 2)?;
                let s = str_arg(0)?;
                let t = str_arg(1)?;
                XValue::Str(s.find(&t).map(|i| s[..i].to_string()).unwrap_or_default())
            }
            "substring-after" => {
                arity(2, 2)?;
                let s = str_arg(0)?;
                let t = str_arg(1)?;
                XValue::Str(s.find(&t).map(|i| s[i + t.len()..].to_string()).unwrap_or_default())
            }
            "substring" => {
                arity(2, 3)?;
                let s: Vec<char> = str_arg(0)?.chars().collect();
                let start = xpath_round(self.number(&eval_arg(1)?));
                let end = if argc == 3 {
                    start + xpath_round(self.number(&eval_arg(2)?))
                } else {
                    f64::INFINITY
                };
                let out: String = s
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        let p = (*i + 1) as f64;
                        p >= start && p < end
                    })
                    .map(|(_, c)| *c)
                    .collect();
                XValue::Str(out)
            }
            "string-length" => {
                arity(0, 1)?;
                let s = if argc == 0 { self.string_value(f.node) } else { str_arg(0)? };
                XValue::Number(s.chars().count() as f64)
            }
            "normalize-space" => {
                arity(0, 1)?;
                let s = if argc == 0 { self.string_value(f.node) } else { str_arg(0)? };
                XValue::Str(
                    s.split(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r'))
                        .filter(|w| !w.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            }
            "translate" => {
                arity(3, 3)?;
                let s = str_arg(0)?;
                let from: Vec<char> = str_arg(1)?.chars().collect();
                let to: Vec<char> = str_arg(2)?.chars().collect();
                XValue::Str(
                    s.chars()
                        .filter_map(|c| match from.iter().position(|&x| x == c) {
                            Some(i) => to.get(i).copied(),
                            None => Some(c),
                        })
                        .collect(),
                )
            }
            "boolean" => {
                arity(1, 1)?;
                XValue::Bool(self.boolean(&eval_arg(0)?))
            }
            "not" => {
                arity(1, 1)?;
                XValue::Bool(!self.boolean(&eval_arg(0)?))
            }
            "true" => {
                arity(0, 0)?;
                XValue::Bool(true)
            }
            "false" => {
                arity(0, 0)?;
                XValue::Bool(false)
            }
            "lang" => {
                arity(1, 1)?;
                let want = str_arg(0)?.to_lowercase();
                let mut cur = Some(f.node);
                let mut lang = None;
                while let Some(n) = cur {
                    if let XNode::Node(id) = n
                        && let NodeKind::Element { attrs, .. } = &self.doc.nodes[id].kind
                        && let Some(a) = attrs
                            .iter()
                            .find(|a| a.local == "lang" && a.namespace.as_deref() == Some("http://www.w3.org/XML/1998/namespace"))
                    {
                        lang = Some(a.value.to_lowercase());
                        break;
                    }
                    cur = self.parent(n);
                }
                XValue::Bool(lang.is_some_and(|l| l == want || l.starts_with(&format!("{want}-"))))
            }
            "number" => {
                arity(0, 1)?;
                XValue::Number(if argc == 0 {
                    string_to_number(&self.string_value(f.node))
                } else {
                    self.number(&eval_arg(0)?)
                })
            }
            "sum" => {
                arity(1, 1)?;
                match eval_arg(0)? {
                    XValue::NodeSet(n) => XValue::Number(n.iter().map(|&x| string_to_number(&self.string_value(x))).sum()),
                    _ => return Err(XPathError("Invalid type".to_string())),
                }
            }
            "floor" => {
                arity(1, 1)?;
                XValue::Number(self.number(&eval_arg(0)?).floor())
            }
            "ceiling" => {
                arity(1, 1)?;
                XValue::Number(self.number(&eval_arg(0)?).ceil())
            }
            "round" => {
                arity(1, 1)?;
                XValue::Number(xpath_round(self.number(&eval_arg(0)?)))
            }
            _ => return Err(XPathError("Unregistered function".to_string())),
        })
    }
}

fn compare_atoms(op: &str, l: &XValue, r: &XValue, ctx: &Ctx<'_>) -> bool {
    match op {
        "=" | "!=" => {
            let eq = if matches!(l, XValue::Bool(_)) || matches!(r, XValue::Bool(_)) {
                ctx.boolean(l) == ctx.boolean(r)
            } else if matches!(l, XValue::Number(_)) || matches!(r, XValue::Number(_)) {
                ctx.number(l) == ctx.number(r)
            } else {
                ctx.string(l) == ctx.string(r)
            };
            if op == "=" { eq } else { !eq }
        }
        _ => {
            let a = ctx.number(l);
            let b = ctx.number(r);
            match op {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            }
        }
    }
}

fn sort_unique(mut nodes: Vec<XNode>) -> Vec<XNode> {
    nodes.sort_by(|a, b| a.order_key().cmp(&b.order_key()));
    nodes.dedup();
    nodes
}

/// XPath `round()`: halves round towards positive infinity.
fn xpath_round(n: f64) -> f64 {
    if n.is_nan() || n.is_infinite() {
        return n;
    }
    (n + 0.5).floor()
}

/// The XPath number of a string (`NaN` unless it is `-? digits (. digits)?` with optional
/// surrounding whitespace).
fn string_to_number(s: &str) -> f64 {
    let t = s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r'));
    let body = t.strip_prefix('-').unwrap_or(t);
    let valid = !body.is_empty()
        && body.chars().all(|c| c.is_ascii_digit() || c == '.')
        && body.chars().filter(|&c| c == '.').count() <= 1
        && body != ".";
    if !valid {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

/// The XPath string of a number.
fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if n == n.trunc() && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let s = format!("{n}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Evaluate `expr` with the document's root element as context node (lxml's
/// `root.xpath(expr, namespaces=...)`).
pub(crate) fn evaluate(
    doc: &Document,
    context: NodeId,
    expr: &str,
    namespaces: &HashMap<String, String>,
) -> Result<XValue, XPathError> {
    let compiled = compile(expr)?;
    let ctx = Ctx { doc, namespaces };
    ctx.eval(
        &compiled,
        Focus {
            node: XNode::Node(context),
            position: 1,
            size: 1,
        },
    )
}

/// Document order comparison of two nodes.
#[allow(dead_code)]
pub(crate) fn document_order(a: XNode, b: XNode) -> Ordering {
    a.order_key().cmp(&b.order_key())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_parser::reader::{Engine, parse};

    fn run(xml: &str, expr: &str) -> XValue {
        let doc = parse(xml, Engine::Lxml, false).unwrap();
        evaluate(&doc, doc.root, expr, &HashMap::new()).unwrap()
    }

    #[test]
    fn paths_predicates_and_functions() {
        let xml = "<root a='1'><item id='1'>x</item><item id='2'><sub>y</sub></item><other/></root>";
        assert_eq!(run(xml, "count(//item)"), XValue::Number(2.0));
        assert_eq!(run(xml, "string(//item[2])"), XValue::Str("y".to_string()));
        assert_eq!(run(xml, "count(item[@id='2']/sub | other)"), XValue::Number(2.0));
        assert_eq!(run(xml, "count(//item[last()]/preceding-sibling::*)"), XValue::Number(1.0));
        assert_eq!(run(xml, "concat('a', 1 + 2 * 3, 'b')"), XValue::Str("a7b".to_string()));
        assert_eq!(run(xml, "10 div 4 mod 2"), XValue::Number(0.5));
        assert_eq!(run(xml, "count(//*[local-name()='sub']/ancestor::*)"), XValue::Number(2.0));
        assert_eq!(run(xml, "//div"), XValue::NodeSet(Vec::new()));
        assert!(compile("//item[").is_err());
    }
}
