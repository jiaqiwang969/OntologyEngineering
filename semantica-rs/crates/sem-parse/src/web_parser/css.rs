//! A CSS selector subset for the static WebDriver's `presence_of_element_located((By.CSS_SELECTOR,
//! ...))`: selector lists of compound selectors (type or `*`, `#id`, `.class`, attribute
//! selectors `[a]`, `[a=v]`, `[a~=v]`, `[a|=v]`, `[a^=v]`, `[a$=v]`, `[a*=v]`) joined by the
//! descendant and child combinators. Other syntax is reported as an invalid selector.

use super::html5dom::{Handle, parent_element};

#[derive(Debug, Clone, PartialEq)]
enum AttrOp {
    Exists,
    Equals(String),
    Includes(String),
    DashMatch(String),
    Prefix(String),
    Suffix(String),
    Substring(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Compound {
    tag: Option<String>,
    ids: Vec<String>,
    classes: Vec<String>,
    attrs: Vec<(String, AttrOp)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Combinator {
    Descendant,
    Child,
}

/// A parsed selector list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SelectorList(Vec<Vec<(Combinator, Compound)>>);

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

struct Cursor<'a> {
    chars: Vec<char>,
    pos: usize,
    _src: &'a str,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        self.pos += 1;
        c
    }
    fn skip_ws(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
        self.pos > start
    }
    fn ident(&mut self) -> Option<String> {
        let start = self.pos;
        while self.peek().is_some_and(is_ident_char) {
            self.pos += 1;
        }
        (self.pos > start).then(|| self.chars[start..self.pos].iter().collect())
    }
    fn value(&mut self) -> Option<String> {
        match self.peek() {
            Some(q @ ('"' | '\'')) => {
                self.pos += 1;
                let start = self.pos;
                while self.peek().is_some_and(|c| c != q) {
                    self.pos += 1;
                }
                let v: String = self.chars[start..self.pos].iter().collect();
                self.bump()?;
                Some(v)
            }
            _ => self.ident(),
        }
    }
}

impl SelectorList {
    /// Parse a selector list; `None` for invalid (or unsupported) syntax.
    pub(crate) fn parse(selector: &str) -> Option<SelectorList> {
        let mut cur = Cursor {
            chars: selector.chars().collect(),
            pos: 0,
            _src: selector,
        };
        let mut list = Vec::new();
        loop {
            cur.skip_ws();
            let mut complex: Vec<(Combinator, Compound)> = Vec::new();
            let mut combinator = Combinator::Descendant;
            loop {
                let compound = parse_compound(&mut cur)?;
                complex.push((combinator, compound));
                let had_ws = cur.skip_ws();
                match cur.peek() {
                    None | Some(',') => break,
                    Some('>') => {
                        cur.bump();
                        cur.skip_ws();
                        combinator = Combinator::Child;
                    }
                    Some(_) if had_ws => combinator = Combinator::Descendant,
                    Some(_) => return None,
                }
            }
            list.push(complex);
            match cur.bump() {
                None => break,
                Some(',') => continue,
                Some(_) => return None,
            }
        }
        Some(SelectorList(list))
    }

    /// Whether any element of `elements` matches.
    pub(crate) fn matches_any(&self, elements: &[Handle]) -> bool {
        elements.iter().any(|e| self.0.iter().any(|complex| matches_complex(e, complex)))
    }
}

fn parse_compound(cur: &mut Cursor<'_>) -> Option<Compound> {
    let mut compound = Compound::default();
    let mut any = false;
    if cur.peek() == Some('*') {
        cur.bump();
        any = true;
    } else if let Some(tag) = cur.ident() {
        compound.tag = Some(tag.to_lowercase());
        any = true;
    }
    loop {
        match cur.peek() {
            Some('#') => {
                cur.bump();
                compound.ids.push(cur.ident()?);
            }
            Some('.') => {
                cur.bump();
                compound.classes.push(cur.ident()?);
            }
            Some('[') => {
                cur.bump();
                cur.skip_ws();
                let name = cur.ident()?.to_lowercase();
                cur.skip_ws();
                let op = match cur.peek()? {
                    ']' => AttrOp::Exists,
                    c => {
                        let kind = if c == '=' {
                            cur.bump();
                            '='
                        } else {
                            cur.bump();
                            if cur.bump()? != '=' {
                                return None;
                            }
                            c
                        };
                        cur.skip_ws();
                        let v = cur.value()?;
                        cur.skip_ws();
                        match kind {
                            '=' => AttrOp::Equals(v),
                            '~' => AttrOp::Includes(v),
                            '|' => AttrOp::DashMatch(v),
                            '^' => AttrOp::Prefix(v),
                            '$' => AttrOp::Suffix(v),
                            '*' => AttrOp::Substring(v),
                            _ => return None,
                        }
                    }
                };
                if cur.bump()? != ']' {
                    return None;
                }
                compound.attrs.push((name, op));
            }
            _ => break,
        }
        any = true;
    }
    any.then_some(compound)
}

fn matches_compound(e: &Handle, c: &Compound) -> bool {
    let Some(local) = e.local_name() else {
        return false;
    };
    if let Some(tag) = &c.tag
        && tag != local
    {
        return false;
    }
    if !c.ids.iter().all(|id| e.attr("id").as_deref() == Some(id.as_str())) {
        return false;
    }
    let classes = e.attr("class").unwrap_or_default();
    if !c
        .classes
        .iter()
        .all(|cl| classes.split_ascii_whitespace().any(|x| x == cl))
    {
        return false;
    }
    c.attrs.iter().all(|(name, op)| {
        let Some(v) = e.attr(name) else {
            return false;
        };
        match op {
            AttrOp::Exists => true,
            AttrOp::Equals(x) => &v == x,
            AttrOp::Includes(x) => v.split_ascii_whitespace().any(|w| w == x),
            AttrOp::DashMatch(x) => &v == x || v.starts_with(&format!("{x}-")),
            AttrOp::Prefix(x) => !x.is_empty() && v.starts_with(x.as_str()),
            AttrOp::Suffix(x) => !x.is_empty() && v.ends_with(x.as_str()),
            AttrOp::Substring(x) => !x.is_empty() && v.contains(x.as_str()),
        }
    })
}

/// Right-to-left matching of a complex selector.
fn matches_complex(e: &Handle, complex: &[(Combinator, Compound)]) -> bool {
    let Some(((_, last), rest)) = complex.split_last() else {
        return false;
    };
    if !matches_compound(e, last) {
        return false;
    }
    let combinator = complex.last().map(|(c, _)| *c).unwrap_or(Combinator::Descendant);
    matches_ancestors(e, rest, combinator)
}

fn matches_ancestors(e: &Handle, rest: &[(Combinator, Compound)], combinator: Combinator) -> bool {
    let Some(((next_comb, compound), remaining)) = rest.split_last() else {
        return true;
    };
    match combinator {
        Combinator::Child => match parent_element(e) {
            Some(p) => matches_compound(&p, compound) && matches_ancestors(&p, remaining, *next_comb),
            None => false,
        },
        Combinator::Descendant => {
            let mut cur = parent_element(e);
            while let Some(p) = cur {
                if matches_compound(&p, compound) && matches_ancestors(&p, remaining, *next_comb) {
                    return true;
                }
                cur = parent_element(&p);
            }
            false
        }
    }
}
