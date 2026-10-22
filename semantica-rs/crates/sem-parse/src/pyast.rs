//! A port of the parts of CPython 3.11's `ast` module that `code_parser.py` relies on:
//! `ast.parse` (the tokenizer, the PEG grammar and the string/f-string literal parser, so
//! that exactly the sources CPython accepts are accepted), `ast.walk` order over the
//! statement nodes the code parser inspects, and `ast.unparse` for expressions.
//!
//! The parser builds full expression trees (needed to unparse decorators and base classes)
//! and a statement skeleton: function and class definitions, imports, and the nesting of
//! compound statements (bodies, `else` blocks, exception handlers, `match` cases), which is
//! all `ast.walk` needs to order the nodes the code parser reads.
//!
//! Differences from CPython that can be observed are listed in `PORT-NOTES.md` (Unicode
//! identifier tables, `\N{...}` escapes, lone surrogates, the depth at which very deeply
//! nested code raises `RecursionError`, and the wording of some syntax error messages).

use std::collections::HashMap;
use std::fmt;

use unicode_normalization::UnicodeNormalization;

// ================================================================================ errors

/// Why `ast.parse` failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParseFailure {
    /// `SyntaxError` (or its subclasses `IndentationError` / `TabError`).
    Syntax(SyntaxError),
    /// `RecursionError: maximum recursion depth exceeded during ast construction`.
    Recursion,
    /// `MemoryError` (the parser's stack limit).
    Memory,
}

impl fmt::Display for ParseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseFailure::Syntax(e) => fmt::Display::fmt(e, f),
            ParseFailure::Recursion => {
                f.write_str("maximum recursion depth exceeded during ast construction")
            }
            ParseFailure::Memory => Ok(()),
        }
    }
}

/// A `SyntaxError` raised by `ast.parse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntaxError {
    /// `SyntaxError`, `IndentationError` or `TabError`.
    pub(crate) class: &'static str,
    /// `e.msg`.
    pub(crate) msg: String,
    /// `e.lineno` (`None` when Python attaches no location).
    pub(crate) lineno: Option<usize>,
}

impl SyntaxError {
    fn new(msg: impl Into<String>, lineno: usize) -> Self {
        SyntaxError {
            class: "SyntaxError",
            msg: msg.into(),
            lineno: Some(lineno),
        }
    }

    fn indentation(msg: impl Into<String>, lineno: usize) -> Self {
        SyntaxError {
            class: "IndentationError",
            msg: msg.into(),
            lineno: Some(lineno),
        }
    }
}

impl fmt::Display for SyntaxError {
    /// `str(e)`: `msg (<unknown>, line N)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.lineno {
            Some(n) => write!(f, "{} (<unknown>, line {n})", self.msg),
            None => f.write_str(&self.msg),
        }
    }
}

type R<T> = Result<T, ParseFailure>;

fn syntax<T>(msg: impl Into<String>, lineno: usize) -> R<T> {
    Err(ParseFailure::Syntax(SyntaxError::new(msg, lineno)))
}

// ============================================================================= tokenizer

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokKind {
    Name,
    Number,
    Str,
    Op,
    Newline,
    Indent,
    Dedent,
    EndMarker,
    /// Where the tokenizer failed (matches nothing).
    Error,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokKind,
    text: String,
    line: usize,
}

const MAXLEVEL: usize = 200;
const MAXINDENT: usize = 100;
const TABSIZE: usize = 8;

const KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
    "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "while", "with", "yield",
];

fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// Zero code points of the Unicode `Nd` (decimal digit) blocks.
const ND_ZEROS: &[u32] = &[
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66,
    0x0CE6, 0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946,
    0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0,
    0xA9F0, 0xAA50, 0xABF0, 0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0,
    0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50,
    0x11DA0, 0x16A60, 0x16AC0, 0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140,
    0x1E2F0, 0x1E950, 0x1FBF0,
];

fn is_decimal_digit(c: char) -> bool {
    let cp = c as u32;
    ND_ZEROS.iter().any(|&z| cp >= z && cp < z + 10)
}

/// `XID_Start` (approximated with Rust's Unicode tables).
fn is_xid_start(c: char) -> bool {
    c == '_'
        || (c.is_alphabetic() && !unicode_normalization::char::is_combining_mark(c))
        || matches!(c, '\u{2118}' | '\u{212E}' | '\u{309B}' | '\u{309C}')
}

/// `XID_Continue` (approximated with Rust's Unicode tables).
fn is_xid_continue(c: char) -> bool {
    is_xid_start(c)
        || c.is_alphabetic()
        || is_decimal_digit(c)
        || unicode_normalization::char::is_combining_mark(c)
        || matches!(
            c,
            '\u{00B7}'
                | '\u{0387}'
                | '\u{1369}'..='\u{1371}'
                | '\u{19DA}'
                | '\u{203F}'
                | '\u{2040}'
                | '\u{2054}'
                | '\u{FE33}'
                | '\u{FE34}'
                | '\u{FE4D}'..='\u{FE4F}'
                | '\u{FF3F}'
                | '\u{200C}'
                | '\u{200D}'
        )
}

fn is_potential_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || (c as u32) >= 128
}

fn is_potential_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || (c as u32) >= 128
}

const TWO_CHAR_OPS: &[&str] = &[
    "!=", "%=", "&=", "**", "*=", "+=", "-=", "->", "//", "/=", ":=", "<<", "<=", "<>", "==",
    ">=", ">>", "@=", "^=", "|=",
];
const THREE_CHAR_OPS: &[&str] = &["**=", "...", "//=", "<<=", ">>="];

struct Tokenizer {
    src: Vec<char>,
    pos: usize,
    line_starts: Vec<usize>,
    atbol: bool,
    indstack: Vec<usize>,
    altindstack: Vec<usize>,
    pendin: isize,
    parens: Vec<(char, usize)>,
}

impl Tokenizer {
    fn new(source: &str) -> Self {
        // translate_newlines + the trailing newline exec input gets.
        let mut src: Vec<char> = Vec::with_capacity(source.len() + 1);
        let mut chars = source.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\r' {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                src.push('\n');
            } else {
                src.push(c);
            }
        }
        if src.last() != Some(&'\n') {
            src.push('\n');
        }
        let mut line_starts = vec![0];
        for (i, c) in src.iter().enumerate() {
            if *c == '\n' {
                line_starts.push(i + 1);
            }
        }
        Tokenizer {
            src,
            pos: 0,
            line_starts,
            atbol: true,
            indstack: vec![0],
            altindstack: vec![0],
            pendin: 0,
            parens: Vec::new(),
        }
    }

    fn line_of(&self, pos: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= pos)
    }

    fn nextc(&mut self) -> Option<char> {
        let c = self.src.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn backup(&mut self, c: Option<char>) {
        if c.is_some() {
            self.pos -= 1;
        }
    }

    fn line(&self) -> usize {
        self.line_of(self.pos.saturating_sub(1))
    }

    /// `tok_continuation_line`: a backslash must be followed by a newline, and the input
    /// must continue after it.
    fn continuation_line(&mut self) -> R<()> {
        let c = self.nextc();
        if c != Some('\n') {
            return syntax(
                "unexpected character after line continuation character",
                self.line(),
            );
        }
        let line = self.line();
        let c = self.nextc();
        if c.is_none() {
            return syntax("unexpected EOF while parsing", line);
        }
        self.backup(c);
        Ok(())
    }

    fn token(&self, kind: TokKind, start: usize) -> Token {
        Token {
            kind,
            text: self.src[start..self.pos].iter().collect(),
            line: self.line_of(start),
        }
    }

    fn next_token(&mut self) -> R<Token> {
        'nextline: loop {
            let mut blankline = false;
            if self.atbol {
                self.atbol = false;
                let mut col = 0usize;
                let mut altcol = 0usize;
                let mut cont_line_col = 0usize;
                let mut c;
                loop {
                    c = self.nextc();
                    match c {
                        Some(' ') => {
                            col += 1;
                            altcol += 1;
                        }
                        Some('\t') => {
                            col = (col / TABSIZE + 1) * TABSIZE;
                            altcol += 1;
                        }
                        Some('\x0c') => {
                            col = 0;
                            altcol = 0;
                        }
                        Some('\\') => {
                            if cont_line_col == 0 {
                                cont_line_col = col;
                            }
                            self.continuation_line()?;
                        }
                        _ => break,
                    }
                }
                self.backup(c);
                if c == Some('#') || c == Some('\n') {
                    blankline = true;
                }
                if !blankline && self.parens.is_empty() {
                    if cont_line_col != 0 {
                        col = cont_line_col;
                        altcol = cont_line_col;
                    }
                    let top = *self.indstack.last().expect("indent stack");
                    let alttop = *self.altindstack.last().expect("indent stack");
                    let line = self.line_of(self.pos);
                    if col == top {
                        if altcol != alttop {
                            return Err(ParseFailure::Syntax(SyntaxError {
                                class: "TabError",
                                msg: "inconsistent use of tabs and spaces in indentation".into(),
                                lineno: Some(line),
                            }));
                        }
                    } else if col > top {
                        if self.indstack.len() >= MAXINDENT {
                            return Err(ParseFailure::Syntax(SyntaxError::indentation(
                                "too many levels of indentation",
                                line,
                            )));
                        }
                        if altcol <= alttop {
                            return Err(ParseFailure::Syntax(SyntaxError {
                                class: "TabError",
                                msg: "inconsistent use of tabs and spaces in indentation".into(),
                                lineno: Some(line),
                            }));
                        }
                        self.pendin += 1;
                        self.indstack.push(col);
                        self.altindstack.push(altcol);
                    } else {
                        while self.indstack.len() > 1 && col < *self.indstack.last().expect("stack") {
                            self.pendin -= 1;
                            self.indstack.pop();
                            self.altindstack.pop();
                        }
                        if col != *self.indstack.last().expect("stack") {
                            return Err(ParseFailure::Syntax(SyntaxError::indentation(
                                "unindent does not match any outer indentation level",
                                line,
                            )));
                        }
                        if altcol != *self.altindstack.last().expect("stack") {
                            return Err(ParseFailure::Syntax(SyntaxError {
                                class: "TabError",
                                msg: "inconsistent use of tabs and spaces in indentation".into(),
                                lineno: Some(line),
                            }));
                        }
                    }
                }
            }

            let line_now = self.line_of(self.pos);
            if self.pendin != 0 {
                if self.pendin < 0 {
                    self.pendin += 1;
                    return Ok(Token {
                        kind: TokKind::Dedent,
                        text: String::new(),
                        line: line_now,
                    });
                }
                self.pendin -= 1;
                return Ok(Token {
                    kind: TokKind::Indent,
                    text: String::new(),
                    line: line_now,
                });
            }

            'again: loop {
                let mut c;
                loop {
                    c = self.nextc();
                    if !matches!(c, Some(' ') | Some('\t') | Some('\x0c')) {
                        break;
                    }
                }
                let start = if c.is_some() { self.pos - 1 } else { self.pos };
                if c == Some('#') {
                    while !matches!(c, None | Some('\n')) {
                        c = self.nextc();
                    }
                }
                let Some(ch) = c else {
                    if let Some(&(open, line)) = self.parens.last() {
                        return syntax(format!("'{open}' was never closed"), line);
                    }
                    return Ok(Token {
                        kind: TokKind::EndMarker,
                        text: String::new(),
                        line: self.line_of(self.pos.saturating_sub(1)),
                    });
                };

                if is_potential_identifier_start(ch) {
                    let (mut saw_b, mut saw_r, mut saw_u, mut saw_f) = (false, false, false, false);
                    let mut c = Some(ch);
                    loop {
                        let cc = c.unwrap_or('\0');
                        if !(saw_b || saw_u || saw_f) && (cc == 'b' || cc == 'B') {
                            saw_b = true;
                        } else if !(saw_b || saw_u || saw_r || saw_f) && (cc == 'u' || cc == 'U') {
                            saw_u = true;
                        } else if !(saw_r || saw_u) && (cc == 'r' || cc == 'R') {
                            saw_r = true;
                        } else if !(saw_f || saw_b || saw_u) && (cc == 'f' || cc == 'F') {
                            saw_f = true;
                        } else {
                            break;
                        }
                        c = self.nextc();
                        if c == Some('"') || c == Some('\'') {
                            return self.string(start, c.expect("quote"));
                        }
                    }
                    let mut nonascii = false;
                    while let Some(cc) = c {
                        if !is_potential_identifier_char(cc) {
                            break;
                        }
                        if (cc as u32) >= 128 {
                            nonascii = true;
                        }
                        c = self.nextc();
                    }
                    self.backup(c);
                    if nonascii {
                        self.verify_identifier(start)?;
                    }
                    return Ok(self.token(TokKind::Name, start));
                }

                if ch == '\n' {
                    self.atbol = true;
                    if blankline || !self.parens.is_empty() {
                        continue 'nextline;
                    }
                    let mut tok = self.token(TokKind::Newline, start);
                    tok.text = String::new();
                    return Ok(tok);
                }

                if ch == '.' {
                    let c2 = self.nextc();
                    if c2.is_some_and(|d| d.is_ascii_digit()) {
                        return self.number_fraction(start, c2);
                    } else if c2 == Some('.') {
                        let c3 = self.nextc();
                        if c3 == Some('.') {
                            return Ok(self.token(TokKind::Op, start));
                        }
                        self.backup(c3);
                        self.backup(c2);
                    } else {
                        self.backup(c2);
                    }
                    return Ok(self.token(TokKind::Op, start));
                }

                if ch.is_ascii_digit() {
                    return self.number(start, ch);
                }

                if ch == '\'' || ch == '"' {
                    return self.string(start, ch);
                }

                if ch == '\\' {
                    self.continuation_line()?;
                    continue 'again;
                }

                // Two- and three-character operators.
                let c2 = self.nextc();
                if let Some(c2) = c2 {
                    let two: String = [ch, c2].iter().collect();
                    if TWO_CHAR_OPS.contains(&two.as_str()) {
                        let c3 = self.nextc();
                        if let Some(c3) = c3 {
                            let three: String = [ch, c2, c3].iter().collect();
                            if THREE_CHAR_OPS.contains(&three.as_str()) {
                                return Ok(self.token(TokKind::Op, start));
                            }
                        }
                        self.backup(c3);
                        return Ok(self.token(TokKind::Op, start));
                    }
                }
                self.backup(c2);

                match ch {
                    '(' | '[' | '{' => {
                        if self.parens.len() >= MAXLEVEL {
                            return syntax("too many nested parentheses", self.line_of(start));
                        }
                        self.parens.push((ch, self.line_of(start)));
                    }
                    ')' | ']' | '}' => {
                        let Some((opening, open_line)) = self.parens.pop() else {
                            return syntax(format!("unmatched '{ch}'"), self.line_of(start));
                        };
                        let ok = matches!((opening, ch), ('(', ')') | ('[', ']') | ('{', '}'));
                        if !ok {
                            let line = self.line_of(start);
                            if open_line != line {
                                return syntax(
                                    format!(
                                        "closing parenthesis '{ch}' does not match opening parenthesis '{opening}' on line {open_line}"
                                    ),
                                    line,
                                );
                            }
                            return syntax(
                                format!(
                                    "closing parenthesis '{ch}' does not match opening parenthesis '{opening}'"
                                ),
                                line,
                            );
                        }
                    }
                    _ => {}
                }
                if !sem_utils::pycompat::fmt::is_printable(ch) {
                    return syntax(
                        format!("invalid non-printable character U+{:04X}", ch as u32),
                        self.line_of(start),
                    );
                }
                return Ok(self.token(TokKind::Op, start));
            }
        }
    }

    fn verify_identifier(&self, start: usize) -> R<()> {
        let chars = &self.src[start..self.pos];
        for (i, &c) in chars.iter().enumerate() {
            let ok = if i == 0 { is_xid_start(c) } else { is_xid_continue(c) };
            if !ok {
                let line = self.line_of(start);
                if sem_utils::pycompat::fmt::is_printable(c) {
                    return syntax(
                        format!("invalid character '{c}' (U+{:04X})", c as u32),
                        line,
                    );
                }
                return syntax(
                    format!("invalid non-printable character U+{:04X}", c as u32),
                    line,
                );
            }
        }
        Ok(())
    }

    /// `verify_end_of_number`.
    fn verify_end_of_number(&mut self, c: Option<char>, kind: &str) -> R<()> {
        let Some(ch) = c else { return Ok(()) };
        let rest: String = self.src[self.pos..self.src.len().min(self.pos + 3)].iter().collect();
        let keyword_follows = match ch {
            'a' => rest.starts_with("nd"),
            'e' => rest.starts_with("lse"),
            'f' => rest.starts_with("or"),
            'i' => matches!(rest.chars().next(), Some('f' | 'n' | 's')),
            'o' => rest.starts_with('r'),
            'n' => rest.starts_with("ot"),
            _ => false,
        };
        if keyword_follows {
            // DeprecationWarning only.
            return Ok(());
        }
        if is_potential_identifier_char(ch) {
            return syntax(format!("invalid {kind} literal"), self.line());
        }
        Ok(())
    }

    /// `tok_decimal_tail`: digits with single underscores between them.
    fn decimal_tail(&mut self) -> R<Option<char>> {
        loop {
            let mut c;
            loop {
                c = self.nextc();
                if !c.is_some_and(|d| d.is_ascii_digit()) {
                    break;
                }
            }
            if c != Some('_') {
                return Ok(c);
            }
            c = self.nextc();
            if !c.is_some_and(|d| d.is_ascii_digit()) {
                self.backup(c);
                return syntax("invalid decimal literal", self.line());
            }
        }
    }

    fn number(&mut self, start: usize, first: char) -> R<Token> {
        let mut c;
        if first == '0' {
            c = self.nextc();
            match c {
                Some('x') | Some('X') => {
                    c = self.nextc();
                    loop {
                        if c == Some('_') {
                            c = self.nextc();
                        }
                        if !c.is_some_and(|d| d.is_ascii_hexdigit()) {
                            self.backup(c);
                            return syntax("invalid hexadecimal literal", self.line());
                        }
                        loop {
                            c = self.nextc();
                            if !c.is_some_and(|d| d.is_ascii_hexdigit()) {
                                break;
                            }
                        }
                        if c != Some('_') {
                            break;
                        }
                    }
                    self.verify_end_of_number(c, "hexadecimal")?;
                }
                Some('o') | Some('O') => {
                    c = self.nextc();
                    loop {
                        if c == Some('_') {
                            c = self.nextc();
                        }
                        if !c.is_some_and(|d| ('0'..'8').contains(&d)) {
                            if let Some(d) = c.filter(|d| d.is_ascii_digit()) {
                                return syntax(format!("invalid digit '{d}' in octal literal"), self.line());
                            }
                            self.backup(c);
                            return syntax("invalid octal literal", self.line());
                        }
                        loop {
                            c = self.nextc();
                            if !c.is_some_and(|d| ('0'..'8').contains(&d)) {
                                break;
                            }
                        }
                        if c != Some('_') {
                            break;
                        }
                    }
                    if let Some(d) = c.filter(|d| d.is_ascii_digit()) {
                        return syntax(format!("invalid digit '{d}' in octal literal"), self.line());
                    }
                    self.verify_end_of_number(c, "octal")?;
                }
                Some('b') | Some('B') => {
                    c = self.nextc();
                    loop {
                        if c == Some('_') {
                            c = self.nextc();
                        }
                        if !matches!(c, Some('0') | Some('1')) {
                            if let Some(d) = c.filter(|d| d.is_ascii_digit()) {
                                return syntax(format!("invalid digit '{d}' in binary literal"), self.line());
                            }
                            self.backup(c);
                            return syntax("invalid binary literal", self.line());
                        }
                        loop {
                            c = self.nextc();
                            if !matches!(c, Some('0') | Some('1')) {
                                break;
                            }
                        }
                        if c != Some('_') {
                            break;
                        }
                    }
                    if let Some(d) = c.filter(|d| d.is_ascii_digit()) {
                        return syntax(format!("invalid digit '{d}' in binary literal"), self.line());
                    }
                    self.verify_end_of_number(c, "binary")?;
                }
                _ => {
                    let mut nonzero = false;
                    loop {
                        if c == Some('_') {
                            c = self.nextc();
                            if !c.is_some_and(|d| d.is_ascii_digit()) {
                                self.backup(c);
                                return syntax("invalid decimal literal", self.line());
                            }
                        }
                        if c != Some('0') {
                            break;
                        }
                        c = self.nextc();
                    }
                    if c.is_some_and(|d| d.is_ascii_digit()) {
                        nonzero = true;
                        c = self.decimal_tail()?;
                    }
                    if c == Some('.') {
                        let c2 = self.nextc();
                        return self.number_fraction(start, c2);
                    } else if matches!(c, Some('e') | Some('E')) {
                        return self.number_exponent(start, c.expect("e"));
                    } else if matches!(c, Some('j') | Some('J')) {
                        return self.number_imaginary(start);
                    } else if nonzero {
                        self.backup(c);
                        return syntax(
                            "leading zeros in decimal integer literals are not permitted; use an 0o prefix for octal integers",
                            self.line(),
                        );
                    }
                    self.verify_end_of_number(c, "decimal")?;
                }
            }
        } else {
            c = self.decimal_tail()?;
            if c == Some('.') {
                let c2 = self.nextc();
                return self.number_fraction(start, c2);
            }
            if matches!(c, Some('e') | Some('E')) {
                return self.number_exponent(start, c.expect("e"));
            }
            if matches!(c, Some('j') | Some('J')) {
                return self.number_imaginary(start);
            }
            self.verify_end_of_number(c, "decimal")?;
        }
        self.backup(c);
        Ok(self.token(TokKind::Number, start))
    }

    /// The fraction part; `c` is the character after the `.`.
    fn number_fraction(&mut self, start: usize, c: Option<char>) -> R<Token> {
        let mut c = c;
        if c.is_some_and(|d| d.is_ascii_digit()) {
            c = self.decimal_tail()?;
        }
        if matches!(c, Some('e') | Some('E')) {
            return self.number_exponent(start, c.expect("e"));
        }
        if matches!(c, Some('j') | Some('J')) {
            return self.number_imaginary(start);
        }
        self.verify_end_of_number(c, "decimal")?;
        self.backup(c);
        Ok(self.token(TokKind::Number, start))
    }

    fn number_exponent(&mut self, start: usize, e: char) -> R<Token> {
        let mut c = self.nextc();
        if c == Some('+') || c == Some('-') {
            c = self.nextc();
            if !c.is_some_and(|d| d.is_ascii_digit()) {
                self.backup(c);
                return syntax("invalid decimal literal", self.line());
            }
        } else if !c.is_some_and(|d| d.is_ascii_digit()) {
            self.backup(c);
            self.verify_end_of_number(Some(e), "decimal")?;
            // `tok_backup(tok, e)`: the number ends before the `e`.
            self.pos -= 1;
            return Ok(self.token(TokKind::Number, start));
        }
        c = self.decimal_tail()?;
        if matches!(c, Some('j') | Some('J')) {
            return self.number_imaginary(start);
        }
        self.verify_end_of_number(c, "decimal")?;
        self.backup(c);
        Ok(self.token(TokKind::Number, start))
    }

    fn number_imaginary(&mut self, start: usize) -> R<Token> {
        let c = self.nextc();
        self.verify_end_of_number(c, "imaginary")?;
        self.backup(c);
        Ok(self.token(TokKind::Number, start))
    }

    fn string(&mut self, start: usize, quote: char) -> R<Token> {
        let first_line = self.line_of(start);
        let mut quote_size = 1;
        let mut end_quote_size = 0;
        let mut c = self.nextc();
        if c == Some(quote) {
            c = self.nextc();
            if c == Some(quote) {
                quote_size = 3;
            } else {
                end_quote_size = 1;
            }
        }
        if c != Some(quote) {
            self.backup(c);
        }
        while end_quote_size != quote_size {
            let c = self.nextc();
            match c {
                None => {
                    let detected = self.line_of(self.src.len().saturating_sub(1));
                    return self.unterminated(quote_size, first_line, detected);
                }
                Some('\n') if quote_size == 1 => {
                    let detected = self.line_of(self.pos - 1);
                    return self.unterminated(quote_size, first_line, detected);
                }
                Some(ch) if ch == quote => end_quote_size += 1,
                Some(ch) => {
                    end_quote_size = 0;
                    if ch == '\\' {
                        self.nextc();
                    }
                }
            }
        }
        Ok(self.token(TokKind::Str, start))
    }

    fn unterminated(&self, quote_size: usize, first_line: usize, detected: usize) -> R<Token> {
        if quote_size == 3 {
            syntax(
                format!("unterminated triple-quoted string literal (detected at line {detected})"),
                first_line,
            )
        } else {
            syntax(
                format!("unterminated string literal (detected at line {detected})"),
                first_line,
            )
        }
    }
}


/// Tokenize like the parser consumes tokens: everything up to the first tokenizer error,
/// then an error sentinel (the error is reported only if the parser gets that far).
fn tokenize_lazy(source: &str) -> (Vec<Token>, Option<(usize, ParseFailure)>) {
    let mut tz = Tokenizer::new(source);
    let mut out = Vec::new();
    loop {
        match tz.next_token() {
            Ok(tok) => {
                let end = tok.kind == TokKind::EndMarker;
                out.push(tok);
                if end {
                    return (out, None);
                }
            }
            Err(e) => {
                let line = out.last().map(|t: &Token| t.line).unwrap_or(1);
                let idx = out.len();
                out.push(Token {
                    kind: TokKind::Error,
                    text: String::new(),
                    line,
                });
                return (out, Some((idx, e)));
            }
        }
    }
}

// ============================================================================ the AST

/// A constant value (`ast.Constant.value`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Const {
    None,
    True,
    False,
    Ellipsis,
    /// An `int` literal: its digits (underscores removed) and radix.
    Int { digits: String, radix: u32 },
    Float(f64),
    /// A `complex` literal (only the imaginary part can be written).
    Imag(f64),
    Str(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoolOpKind {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOpKind {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    FloorDiv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnaryOpKind {
    Invert,
    Not,
    UAdd,
    USub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CmpOpKind {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Comprehension {
    target: Expr,
    iter: Expr,
    ifs: Vec<Expr>,
    is_async: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Keyword {
    arg: Option<String>,
    value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Arg {
    name: String,
    annotation: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Arguments {
    posonlyargs: Vec<Arg>,
    args: Vec<Arg>,
    vararg: Option<Arg>,
    kwonlyargs: Vec<Arg>,
    kw_defaults: Vec<Option<Expr>>,
    kwarg: Option<Arg>,
    defaults: Vec<Expr>,
}

/// An expression node (`ast.expr`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    BoolOp(BoolOpKind, Vec<Expr>),
    NamedExpr(Box<Expr>, Box<Expr>),
    BinOp(Box<Expr>, BinOpKind, Box<Expr>),
    UnaryOp(UnaryOpKind, Box<Expr>),
    Lambda(Box<Arguments>, Box<Expr>),
    IfExp(Box<Expr>, Box<Expr>, Box<Expr>),
    Dict(Vec<Option<Expr>>, Vec<Expr>),
    Set(Vec<Expr>),
    ListComp(Box<Expr>, Vec<Comprehension>),
    SetComp(Box<Expr>, Vec<Comprehension>),
    DictComp(Box<Expr>, Box<Expr>, Vec<Comprehension>),
    GeneratorExp(Box<Expr>, Vec<Comprehension>),
    Await(Box<Expr>),
    Yield(Option<Box<Expr>>),
    YieldFrom(Box<Expr>),
    Compare(Box<Expr>, Vec<CmpOpKind>, Vec<Expr>),
    Call(Box<Expr>, Vec<Expr>, Vec<Keyword>),
    FormattedValue(Box<Expr>, i32, Option<Box<Expr>>),
    JoinedStr(Vec<Expr>),
    Constant(Const, bool),
    Attribute(Box<Expr>, String),
    Subscript(Box<Expr>, Box<Expr>),
    Starred(Box<Expr>),
    Name(String),
    List(Vec<Expr>),
    Tuple(Vec<Expr>),
    Slice(Option<Box<Expr>>, Option<Box<Expr>>, Option<Box<Expr>>),
}

fn bx(e: Expr) -> Box<Expr> {
    Box::new(e)
}

fn opt_height(e: &Option<Box<Expr>>) -> usize {
    e.as_ref().map(|e| e.height()).unwrap_or(0)
}

fn max_height<'a>(items: impl IntoIterator<Item = &'a Expr>) -> usize {
    items.into_iter().map(Expr::height).max().unwrap_or(0)
}

impl Comprehension {
    fn height(&self) -> usize {
        1 + self
            .target
            .height()
            .max(self.iter.height())
            .max(max_height(&self.ifs))
    }
}

impl Arguments {
    fn height(&self) -> usize {
        let arg_h = |a: &Arg| 1 + a.annotation.as_ref().map(Expr::height).unwrap_or(0);
        let mut h = 0;
        for a in self.posonlyargs.iter().chain(&self.args).chain(&self.kwonlyargs) {
            h = h.max(arg_h(a));
        }
        for a in self.vararg.iter().chain(self.kwarg.iter()) {
            h = h.max(arg_h(a));
        }
        h = h.max(max_height(&self.defaults));
        h = h.max(max_height(self.kw_defaults.iter().flatten()));
        1 + h
    }
}

impl Expr {
    /// The node's height in the AST (CPython counts it when converting the AST to Python
    /// objects).
    fn height(&self) -> usize {
        1 + match self {
            Expr::BoolOp(_, v) | Expr::Set(v) | Expr::JoinedStr(v) | Expr::List(v) | Expr::Tuple(v) => {
                max_height(v)
            }
            Expr::NamedExpr(a, b) | Expr::BinOp(a, _, b) | Expr::Subscript(a, b) => {
                a.height().max(b.height())
            }
            Expr::UnaryOp(_, a) | Expr::Await(a) | Expr::YieldFrom(a) | Expr::Starred(a) | Expr::Attribute(a, _) => {
                a.height()
            }
            Expr::Lambda(args, body) => args.height().max(body.height()),
            Expr::IfExp(a, b, c) => a.height().max(b.height()).max(c.height()),
            Expr::Dict(keys, values) => max_height(keys.iter().flatten()).max(max_height(values)),
            Expr::ListComp(e, g) | Expr::SetComp(e, g) | Expr::GeneratorExp(e, g) => {
                e.height().max(g.iter().map(Comprehension::height).max().unwrap_or(0))
            }
            Expr::DictComp(k, v, g) => k
                .height()
                .max(v.height())
                .max(g.iter().map(Comprehension::height).max().unwrap_or(0)),
            Expr::Yield(v) => opt_height(v),
            Expr::Compare(l, _, c) => l.height().max(max_height(c)),
            Expr::Call(f, a, k) => f
                .height()
                .max(max_height(a))
                .max(k.iter().map(|k| 1 + k.value.height()).max().unwrap_or(0)),
            Expr::FormattedValue(v, _, spec) => v.height().max(opt_height(spec)),
            Expr::Constant(..) | Expr::Name(_) => 0,
            Expr::Slice(a, b, c) => opt_height(a).max(opt_height(b)).max(opt_height(c)),
        }
    }
}

// ============================================================================ statements

/// A statement, reduced to what `code_parser` reads through `ast.walk`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stmt {
    /// `node.lineno`.
    pub(crate) lineno: usize,
    pub(crate) kind: StmtKind,
    height: usize,
}

/// The statement kinds `code_parser` distinguishes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StmtKind {
    /// `def` / `async def` (`is_async` distinguishes `ast.AsyncFunctionDef`).
    FunctionDef {
        name: String,
        /// `[arg.arg for arg in node.args.args]`.
        args: Vec<String>,
        /// `ast.unparse(d)` of each decorator (`Err` holds the `ValueError` message).
        decorators: Vec<Result<String, String>>,
        body: Vec<Stmt>,
        is_async: bool,
    },
    /// `class`.
    ClassDef {
        name: String,
        /// `ast.unparse(b)` of each base.
        bases: Vec<Result<String, String>>,
        body: Vec<Stmt>,
    },
    /// `import a.b as c, d` (`alias.name` of each name).
    Import { names: Vec<String> },
    /// `from module import names` (`node.module`, `alias.name` of each name).
    ImportFrom {
        module: Option<String>,
        names: Vec<String>,
    },
    /// `if`/`for`/`while`/`with`/`try`/`match`: the child statements in field order, with
    /// exception handlers and match cases as groups (one more level in `ast.walk`).
    Compound { children: Vec<Child> },
    /// Any other statement.
    Simple,
}

/// A child of a compound statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Child {
    Stmt(Stmt),
    /// An `ExceptHandler` or `match_case` node and its body.
    Group(Vec<Stmt>),
}

/// A parsed module (`ast.Module`).
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Module {
    pub(crate) body: Vec<Stmt>,
}

impl Module {
    /// The statements in `ast.walk` (breadth-first) order.
    pub(crate) fn walk(&self) -> Vec<&Stmt> {
        enum Node<'a> {
            Stmt(&'a Stmt),
            Group(&'a [Stmt]),
        }
        let mut out = Vec::new();
        let mut queue: std::collections::VecDeque<Node<'_>> =
            self.body.iter().map(Node::Stmt).collect();
        while let Some(node) = queue.pop_front() {
            match node {
                Node::Stmt(s) => {
                    out.push(s);
                    match &s.kind {
                        StmtKind::FunctionDef { body, .. } | StmtKind::ClassDef { body, .. } => {
                            queue.extend(body.iter().map(Node::Stmt));
                        }
                        StmtKind::Compound { children } => {
                            for c in children {
                                match c {
                                    Child::Stmt(s) => queue.push_back(Node::Stmt(s)),
                                    Child::Group(g) => queue.push_back(Node::Group(g)),
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Node::Group(g) => queue.extend(g.iter().map(Node::Stmt)),
            }
        }
        out
    }
}

fn stmt(lineno: usize, kind: StmtKind, height: usize) -> Stmt {
    Stmt {
        lineno,
        kind,
        height,
    }
}

fn stmts_height(stmts: &[Stmt]) -> usize {
    stmts.iter().map(|s| s.height).max().unwrap_or(0)
}

// ====================================================================== string literals

/// Common `\N{...}` names (the port has no Unicode name database; other names decode to
/// U+FFFD).
const UNICODE_NAMES: &[(&str, char)] = &[
    ("BULLET", '\u{2022}'),
    ("EM DASH", '\u{2014}'),
    ("EN DASH", '\u{2013}'),
    ("NO-BREAK SPACE", '\u{00A0}'),
    ("SPACE", ' '),
    ("DEGREE SIGN", '\u{00B0}'),
    ("COPYRIGHT SIGN", '\u{00A9}'),
    ("REGISTERED SIGN", '\u{00AE}'),
    ("TRADE MARK SIGN", '\u{2122}'),
    ("HORIZONTAL ELLIPSIS", '\u{2026}'),
    ("LEFT DOUBLE QUOTATION MARK", '\u{201C}'),
    ("RIGHT DOUBLE QUOTATION MARK", '\u{201D}'),
    ("LEFT SINGLE QUOTATION MARK", '\u{2018}'),
    ("RIGHT SINGLE QUOTATION MARK", '\u{2019}'),
    ("MULTIPLICATION SIGN", '\u{00D7}'),
    ("DIVISION SIGN", '\u{00F7}'),
    ("CHECK MARK", '\u{2713}'),
    ("HEAVY CHECK MARK", '\u{2714}'),
    ("BALLOT X", '\u{2717}'),
    ("BLACK STAR", '\u{2605}'),
    ("WHITE STAR", '\u{2606}'),
    ("RIGHTWARDS ARROW", '\u{2192}'),
    ("LEFTWARDS ARROW", '\u{2190}'),
    ("UPWARDS ARROW", '\u{2191}'),
    ("DOWNWARDS ARROW", '\u{2193}'),
    ("GREEK SMALL LETTER ALPHA", '\u{03B1}'),
    ("GREEK SMALL LETTER BETA", '\u{03B2}'),
    ("GREEK SMALL LETTER GAMMA", '\u{03B3}'),
    ("GREEK SMALL LETTER DELTA", '\u{03B4}'),
    ("GREEK SMALL LETTER LAMDA", '\u{03BB}'),
    ("GREEK SMALL LETTER MU", '\u{03BC}'),
    ("GREEK SMALL LETTER PI", '\u{03C0}'),
    ("GREEK SMALL LETTER SIGMA", '\u{03C3}'),
    ("GREEK CAPITAL LETTER DELTA", '\u{0394}'),
    ("GREEK CAPITAL LETTER SIGMA", '\u{03A3}'),
    ("GREEK CAPITAL LETTER OMEGA", '\u{03A9}'),
    ("ZERO WIDTH SPACE", '\u{200B}'),
    ("ZERO WIDTH JOINER", '\u{200D}'),
    ("ZERO WIDTH NON-JOINER", '\u{200C}'),
    ("BYTE ORDER MARK", '\u{FEFF}'),
    ("ZERO WIDTH NO-BREAK SPACE", '\u{FEFF}'),
    ("REPLACEMENT CHARACTER", '\u{FFFD}'),
    ("PILCROW SIGN", '\u{00B6}'),
    ("SECTION SIGN", '\u{00A7}'),
    ("MIDDLE DOT", '\u{00B7}'),
    ("EURO SIGN", '\u{20AC}'),
    ("POUND SIGN", '\u{00A3}'),
    ("YEN SIGN", '\u{00A5}'),
    ("CENT SIGN", '\u{00A2}'),
    ("PLUS-MINUS SIGN", '\u{00B1}'),
    ("INFINITY", '\u{221E}'),
    ("NOT EQUAL TO", '\u{2260}'),
    ("LESS-THAN OR EQUAL TO", '\u{2264}'),
    ("GREATER-THAN OR EQUAL TO", '\u{2265}'),
    ("ALMOST EQUAL TO", '\u{2248}'),
    ("SNOWMAN", '\u{2603}'),
    ("BLACK HEART SUIT", '\u{2665}'),
    ("LINE FEED", '\n'),
    ("CARRIAGE RETURN", '\r'),
    ("CHARACTER TABULATION", '\t'),
    ("NULL", '\0'),
    ("ESCAPE", '\u{1B}'),
    ("DELETE", '\u{7F}'),
    ("BOX DRAWINGS LIGHT HORIZONTAL", '\u{2500}'),
    ("BOX DRAWINGS LIGHT VERTICAL", '\u{2502}'),
    ("FULL BLOCK", '\u{2588}'),
    ("LIGHT SHADE", '\u{2591}'),
];

fn unicode_error(reason: &str, line: usize) -> ParseFailure {
    ParseFailure::Syntax(SyntaxError::new(
        format!("(unicode error) 'unicodeescape' codec can't decode bytes: {reason}"),
        line,
    ))
}

fn hex_value(chars: &[char], start: usize, count: usize) -> Option<u32> {
    if start + count > chars.len() {
        return None;
    }
    let mut v: u32 = 0;
    for &c in &chars[start..start + count] {
        v = v.checked_mul(16)? + c.to_digit(16)?;
    }
    Some(v)
}

/// `decode_unicode_with_escapes`: the `unicode_escape` codec over the literal's text
/// (non-ASCII characters pass through; a backslash before one is kept).
fn decode_unicode_escapes(s: &[char], line: usize) -> R<String> {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        i += 1;
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(&n) = s.get(i) else {
            return Err(unicode_error("\\ at end of string", line));
        };
        i += 1;
        match n {
            '\n' => {}
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            't' => out.push('\t'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            'v' => out.push('\u{b}'),
            'a' => out.push('\u{7}'),
            '0'..='7' => {
                let mut v = n.to_digit(8).expect("octal");
                for _ in 0..2 {
                    match s.get(i).and_then(|c| c.to_digit(8)) {
                        Some(d) => {
                            v = v * 8 + d;
                            i += 1;
                        }
                        None => break,
                    }
                }
                out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            'x' | 'u' | 'U' => {
                let (count, what) = match n {
                    'x' => (2, "truncated \\xXX escape"),
                    'u' => (4, "truncated \\uXXXX escape"),
                    _ => (8, "truncated \\UXXXXXXXX escape"),
                };
                let Some(v) = hex_value(s, i, count) else {
                    return Err(unicode_error(what, line));
                };
                i += count;
                if v > 0x10FFFF {
                    return Err(unicode_error("illegal Unicode character", line));
                }
                // Lone surrogates cannot be held in a Rust string.
                out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
            }
            'N' => {
                if s.get(i) != Some(&'{') {
                    return Err(unicode_error("malformed \\N character escape", line));
                }
                let Some(close) = s[i + 1..].iter().position(|&c| c == '}') else {
                    return Err(unicode_error("malformed \\N character escape", line));
                };
                if close == 0 {
                    return Err(unicode_error("malformed \\N character escape", line));
                }
                let name: String = s[i + 1..i + 1 + close].iter().collect::<String>().to_uppercase();
                i += close + 2;
                let ch = UNICODE_NAMES
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, c)| *c)
                    .unwrap_or('\u{fffd}');
                out.push(ch);
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    Ok(out)
}

/// `_PyBytes_DecodeEscape`.
fn decode_bytes_escapes(s: &[char], line: usize) -> R<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        i += 1;
        if c != '\\' {
            out.push(c as u8);
            continue;
        }
        let Some(&n) = s.get(i) else {
            return syntax("(value error) Trailing \\ in string", line);
        };
        i += 1;
        match n {
            '\n' => {}
            '\\' => out.push(b'\\'),
            '\'' => out.push(b'\''),
            '"' => out.push(b'"'),
            'b' => out.push(8),
            'f' => out.push(12),
            't' => out.push(b'\t'),
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            'v' => out.push(11),
            'a' => out.push(7),
            '0'..='7' => {
                let mut v = n.to_digit(8).expect("octal");
                for _ in 0..2 {
                    match s.get(i).and_then(|c| c.to_digit(8)) {
                        Some(d) => {
                            v = v * 8 + d;
                            i += 1;
                        }
                        None => break,
                    }
                }
                out.push((v & 0xff) as u8);
            }
            'x' => {
                let Some(v) = hex_value(s, i, 2) else {
                    return syntax(
                        format!("(value error) invalid \\x escape at position {}", i - 2),
                        line,
                    );
                };
                i += 2;
                out.push(v as u8);
            }
            other => {
                out.push(b'\\');
                out.push(other as u8);
            }
        }
    }
    Ok(out)
}

enum StrPart {
    Bytes(Vec<u8>),
    Str(String),
    FStr { body: Vec<char>, raw: bool },
}

/// `_PyPegen_parsestr`.
fn parsestr(t: &Token) -> R<StrPart> {
    let chars: Vec<char> = t.text.chars().collect();
    let mut i = 0;
    let (mut bytes, mut raw, mut fmode) = (false, false, false);
    while i < chars.len() && chars[i].is_ascii_alphabetic() {
        match chars[i].to_ascii_lowercase() {
            'b' => bytes = true,
            'r' => raw = true,
            'f' => fmode = true,
            _ => {}
        }
        i += 1;
    }
    let quote = chars[i];
    let mut body = &chars[i + 1..chars.len() - 1];
    if body.len() >= 4 && body[0] == quote && body[1] == quote {
        body = &body[2..body.len() - 2];
    }
    if fmode {
        return Ok(StrPart::FStr {
            body: body.to_vec(),
            raw,
        });
    }
    let raw = raw || !body.contains(&'\\');
    if bytes {
        if body.iter().any(|c| (*c as u32) >= 0x80) {
            return syntax("bytes can only contain ASCII literal characters", t.line);
        }
        if raw {
            return Ok(StrPart::Bytes(body.iter().map(|c| *c as u8).collect()));
        }
        return Ok(StrPart::Bytes(decode_bytes_escapes(body, t.line)?));
    }
    if raw {
        return Ok(StrPart::Str(body.iter().collect()));
    }
    Ok(StrPart::Str(decode_unicode_escapes(body, t.line)?))
}

#[derive(Default)]
struct FState {
    last_str: Option<String>,
    exprs: Vec<Expr>,
    fmode: bool,
}

impl FState {
    fn concat(&mut self, s: String) {
        if s.is_empty() {
            return;
        }
        match &mut self.last_str {
            Some(acc) => acc.push_str(&s),
            None => self.last_str = Some(s),
        }
    }

    fn finish(mut self, kind_u: bool) -> Expr {
        if !self.fmode {
            return Expr::Constant(Const::Str(self.last_str.unwrap_or_default()), kind_u);
        }
        if let Some(s) = self.last_str.take() {
            self.exprs.push(Expr::Constant(Const::Str(s), kind_u));
        }
        Expr::JoinedStr(self.exprs)
    }
}

/// `_PyPegen_concatenate_strings`.
fn concatenate_strings(toks: &[Token]) -> R<Expr> {
    let kind_u = toks[0].text.starts_with('u');
    let mut bytesmode = false;
    let mut bytes_acc = Vec::new();
    let mut state = FState::default();
    for (i, t) in toks.iter().enumerate() {
        let part = parsestr(t)?;
        let this_bytes = matches!(part, StrPart::Bytes(_));
        if i != 0 && bytesmode != this_bytes {
            return syntax("cannot mix bytes and nonbytes literals", t.line);
        }
        bytesmode = this_bytes;
        match part {
            StrPart::Bytes(b) => bytes_acc.extend(b),
            StrPart::Str(s) => state.concat(s),
            StrPart::FStr { body, raw } => {
                let mut pos = 0;
                let end = body.len();
                concat_fstring(&mut state, &body, &mut pos, end, raw, 0, kind_u, t.line)?;
            }
        }
    }
    if bytesmode {
        return Ok(Expr::Constant(Const::Bytes(bytes_acc), false));
    }
    Ok(state.finish(kind_u))
}

/// `_PyPegen_FstringParser_ConcatFstring`.
#[allow(clippy::too_many_arguments)]
fn concat_fstring(
    state: &mut FState,
    s: &[char],
    pos: &mut usize,
    end: usize,
    raw: bool,
    recurse_lvl: u32,
    kind_u: bool,
    line: usize,
) -> R<()> {
    state.fmode = true;
    loop {
        let (literal, result) = fstring_find_literal(s, pos, end, raw, recurse_lvl, line)?;
        if let Some(l) = literal {
            state.concat(l);
        }
        if result == 1 {
            continue;
        }
        if *pos >= end || s[*pos] == '}' {
            break;
        }
        let (expr_text, expression) = fstring_find_expr(s, pos, end, raw, recurse_lvl, line)?;
        if let Some(t) = expr_text {
            state.concat(t);
        }
        if let Some(ls) = state.last_str.take() {
            state.exprs.push(Expr::Constant(Const::Str(ls), kind_u));
        }
        state.exprs.push(expression);
    }
    if recurse_lvl == 0 && end > 0 && *pos < end - 1 {
        return syntax("f-string: unexpected end of string", line);
    }
    if recurse_lvl != 0 && (*pos >= end || s[*pos] != '}') {
        return syntax("f-string: expecting '}'", line);
    }
    Ok(())
}

/// `fstring_find_literal`: returns the literal (decoded) and 1 when a doubled brace was
/// consumed (keep scanning).
fn fstring_find_literal(
    s: &[char],
    pos: &mut usize,
    end: usize,
    raw: bool,
    recurse_lvl: u32,
    line: usize,
) -> R<(Option<String>, i32)> {
    let literal_start = *pos;
    let mut i = *pos;
    let mut result = 0;
    let mut lit_end = None;
    while i < end {
        let mut ch = s[i];
        i += 1;
        if !raw && ch == '\\' && i < end {
            ch = s[i];
            i += 1;
            if ch == 'N' {
                if i < end {
                    let c = s[i];
                    i += 1;
                    if c == '{' {
                        while i < end {
                            let c = s[i];
                            i += 1;
                            if c == '}' {
                                break;
                            }
                        }
                        continue;
                    }
                }
                continue;
            }
        }
        if ch == '{' || ch == '}' {
            if recurse_lvl == 0 {
                if i < end && s[i] == ch {
                    *pos = i + 1;
                    result = 1;
                    lit_end = Some(i);
                    break;
                }
                if ch == '}' {
                    *pos = i - 1;
                    return syntax("f-string: single '}' is not allowed", line);
                }
            }
            i -= 1;
            break;
        }
    }
    let lit_end = match lit_end {
        Some(e) => e,
        None => {
            *pos = i;
            i
        }
    };
    let literal = if literal_start != lit_end {
        let text = &s[literal_start..lit_end];
        Some(if raw {
            text.iter().collect()
        } else {
            decode_unicode_escapes(text, line)?
        })
    } else {
        None
    };
    Ok((literal, result))
}

fn fstring_error<T>(msg: impl Into<String>, line: usize) -> R<T> {
    syntax(msg, line)
}

/// `fstring_find_expr`: parse one replacement field starting at `{`.
fn fstring_find_expr(
    s: &[char],
    pos: &mut usize,
    end: usize,
    raw: bool,
    recurse_lvl: u32,
    line: usize,
) -> R<(Option<String>, Expr)> {
    if recurse_lvl >= 2 {
        return fstring_error("f-string: expressions nested too deeply", line);
    }
    *pos += 1;
    let expr_start = *pos;
    let mut quote_char: Option<char> = None;
    let mut string_type = 0;
    let mut parens: Vec<char> = Vec::new();
    while *pos < end {
        let ch = s[*pos];
        if ch == '\\' {
            return fstring_error("f-string expression part cannot include a backslash", line);
        }
        if let Some(q) = quote_char {
            if ch == q {
                if string_type == 3 {
                    if *pos + 2 < end && s[*pos + 1] == ch && s[*pos + 2] == ch {
                        *pos += 3;
                        string_type = 0;
                        quote_char = None;
                        continue;
                    }
                } else {
                    quote_char = None;
                    string_type = 0;
                    *pos += 1;
                    continue;
                }
            }
        } else if ch == '\'' || ch == '"' {
            if *pos + 2 < end && s[*pos + 1] == ch && s[*pos + 2] == ch {
                string_type = 3;
                *pos += 2;
            } else {
                string_type = 1;
            }
            quote_char = Some(ch);
        } else if ch == '[' || ch == '{' || ch == '(' {
            if parens.len() >= MAXLEVEL {
                return fstring_error("f-string: too many nested parenthesis", line);
            }
            parens.push(ch);
        } else if ch == '#' {
            return fstring_error("f-string expression part cannot include '#'", line);
        } else if parens.is_empty() && matches!(ch, '!' | ':' | '}' | '=' | '>' | '<') {
            if *pos + 1 < end {
                let next = s[*pos + 1];
                if (ch == '!' && next == '=')
                    || (ch == '=' && next == '=')
                    || (ch == '<' && next == '=')
                    || (ch == '>' && next == '=')
                {
                    *pos += 2;
                    continue;
                }
            }
            if ch == '>' || ch == '<' {
                *pos += 1;
                continue;
            }
            break;
        } else if ch == ']' || ch == '}' || ch == ')' {
            let Some(opening) = parens.pop() else {
                return fstring_error(format!("f-string: unmatched '{ch}'"), line);
            };
            if !matches!((opening, ch), ('(', ')') | ('[', ']') | ('{', '}')) {
                return fstring_error(
                    format!(
                        "f-string: closing parenthesis '{ch}' does not match opening parenthesis '{opening}'"
                    ),
                    line,
                );
            }
        }
        *pos += 1;
    }
    let expr_end = *pos;
    if quote_char.is_some() {
        return fstring_error("f-string: unterminated string", line);
    }
    if let Some(opening) = parens.last() {
        return fstring_error(format!("f-string: unmatched '{opening}'"), line);
    }
    if *pos >= end {
        return fstring_error("f-string: expecting '}'", line);
    }
    let simple_expression = fstring_compile_expr(&s[expr_start..expr_end], s[expr_end], line)?;
    let mut expr_text = None;
    if s[*pos] == '=' {
        *pos += 1;
        while *pos < end && matches!(s[*pos], ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c') {
            *pos += 1;
        }
        if *pos >= end {
            return fstring_error("f-string: expecting '}'", line);
        }
        expr_text = Some(s[expr_start..*pos].iter().collect::<String>());
    }
    let mut conversion: i32 = -1;
    if *pos < end && s[*pos] == '!' {
        *pos += 1;
        if *pos >= end {
            return fstring_error("f-string: expecting '}'", line);
        }
        let c = s[*pos];
        *pos += 1;
        if !matches!(c, 's' | 'r' | 'a') {
            return fstring_error(
                "f-string: invalid conversion character: expected 's', 'r', or 'a'",
                line,
            );
        }
        conversion = c as i32;
    }
    let mut format_spec = None;
    if *pos < end && s[*pos] == ':' {
        *pos += 1;
        if *pos >= end {
            return fstring_error("f-string: expecting '}'", line);
        }
        let mut state = FState::default();
        concat_fstring(&mut state, s, pos, end, raw, recurse_lvl + 1, false, line)?;
        format_spec = Some(bx(state.finish(false)));
    }
    if *pos >= end || s[*pos] != '}' {
        return fstring_error("f-string: expecting '}'", line);
    }
    *pos += 1;
    if expr_text.is_some() && format_spec.is_none() && conversion == -1 {
        conversion = 'r' as i32;
    }
    Ok((
        expr_text,
        Expr::FormattedValue(bx(simple_expression), conversion, format_spec),
    ))
}

/// `fstring_compile_expr`: parse the expression text as `(<text>)`.
fn fstring_compile_expr(text: &[char], terminator: char, line: usize) -> R<Expr> {
    if text.iter().all(|c| matches!(c, ' ' | '\t' | '\n' | '\x0c')) {
        if matches!(terminator, '!' | ':' | '=') {
            return fstring_error(
                format!("f-string: expression required before '{terminator}'"),
                line,
            );
        }
        return fstring_error("f-string: empty expression not allowed", line);
    }
    let mut src = String::with_capacity(text.len() + 2);
    src.push('(');
    src.extend(text.iter());
    src.push(')');
    let (toks, tok_err) = tokenize_lazy(&src);
    let mut p = Parser::new(&toks);
    match p.star_expressions() {
        Ok(Some(e)) => Ok(e),
        Ok(None) => {
            if let Some((idx, e)) = tok_err
                && p.furthest >= idx
            {
                return Err(match e {
                    ParseFailure::Syntax(mut se) => {
                        se.lineno = Some(line);
                        ParseFailure::Syntax(se)
                    }
                    other => other,
                });
            }
            fstring_error("f-string: invalid syntax", line)
        }
        Err(e) => Err(e),
    }
}

/// `int` digits for an integer literal; decimal literals longer than CPython's
/// int/str conversion limit are a `SyntaxError`.
fn parse_number(text: &str, line: usize) -> R<Const> {
    let clean: String = text.chars().filter(|&c| c != '_').collect();
    let lower = clean.to_ascii_lowercase();
    if let Some(imag) = lower.strip_suffix('j') {
        return Ok(Const::Imag(imag.parse::<f64>().unwrap_or(f64::INFINITY)));
    }
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = lower.strip_prefix(prefix) {
            return Ok(Const::Int {
                digits: digits.to_string(),
                radix,
            });
        }
    }
    if lower.contains('.') || lower.contains('e') {
        return Ok(Const::Float(lower.parse::<f64>().unwrap_or(f64::INFINITY)));
    }
    if lower.len() > INT_MAX_STR_DIGITS {
        return syntax(
            format!(
                "Exceeds the limit ({INT_MAX_STR_DIGITS} digits) for integer string conversion: value has {} digits; use sys.set_int_max_str_digits() to increase the limit - Consider hexadecimal for huge integer literals to avoid decimal conversion limits.",
                lower.len()
            ),
            line,
        );
    }
    Ok(Const::Int {
        digits: lower,
        radix: 10,
    })
}

const INT_MAX_STR_DIGITS: usize = 4300;

/// The decimal representation of an integer literal.
fn int_to_decimal(digits: &str, radix: u32) -> String {
    if radix == 10 {
        let trimmed = digits.trim_start_matches('0');
        return if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() };
    }
    // Little-endian base-1e9 limbs.
    const BASE: u64 = 1_000_000_000;
    let mut limbs: Vec<u64> = vec![0];
    for c in digits.chars() {
        let d = c.to_digit(radix).unwrap_or(0) as u64;
        let mut carry = d;
        for limb in limbs.iter_mut() {
            let v = *limb * radix as u64 + carry;
            *limb = v % BASE;
            carry = v / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    let mut out = limbs.last().expect("limb").to_string();
    for limb in limbs.iter().rev().skip(1) {
        out.push_str(&format!("{limb:09}"));
    }
    out
}

fn normalize_identifier(raw: &str) -> String {
    if raw.is_ascii() {
        raw.to_string()
    } else {
        raw.nfkc().collect()
    }
}

// =============================================================================== parser

/// CPython's `MAXSTACK`: deeper rule nesting raises `MemoryError`.
const MAXSTACK: usize = 6000;

const M_EXPRESSION: u8 = 1;
const M_NAMED: u8 = 2;
const M_ATOM: u8 = 3;
const M_STRINGS: u8 = 4;
const M_STAR_TARGET: u8 = 5;
const M_DISJUNCTION: u8 = 6;
const M_BITWISE_OR: u8 = 7;
const M_PRIMARY: u8 = 8;

/// What a block belongs to (for the "expected an indented block" message).
#[derive(Clone, Copy)]
struct BlockCtx {
    what: &'static str,
    line: usize,
}

impl BlockCtx {
    fn new(what: &'static str, line: usize) -> Self {
        BlockCtx { what, line }
    }
}

struct Parser<'t> {
    toks: &'t [Token],
    pos: usize,
    furthest: usize,
    memo: HashMap<(u8, usize), (Option<Expr>, usize)>,
    hints: Vec<(usize, SyntaxError)>,
    level: usize,
}

type PO<T> = R<Option<T>>;

impl<'t> Parser<'t> {
    fn new(toks: &'t [Token]) -> Self {
        Parser {
            toks,
            pos: 0,
            furthest: 0,
            memo: HashMap::new(),
            hints: Vec::new(),
            level: 0,
        }
    }

    // ---------------------------------------------------------------- token helpers

    fn peek(&mut self) -> &'t Token {
        if self.pos > self.furthest {
            self.furthest = self.pos;
        }
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn line(&mut self) -> usize {
        self.peek().line
    }

    fn at_op(&mut self, op: &str) -> bool {
        let t = self.peek();
        t.kind == TokKind::Op && t.text == op
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if self.at_op(op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn at_kw(&mut self, kw: &str) -> bool {
        let t = self.peek();
        t.kind == TokKind::Name && t.text == kw
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.at_kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn at_kind(&mut self, kind: TokKind) -> bool {
        self.peek().kind == kind
    }

    fn eat_kind(&mut self, kind: TokKind) -> bool {
        if self.at_kind(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// `NAME` (not a keyword), NFKC-normalised.
    fn name(&mut self) -> Option<String> {
        let t = self.peek();
        if t.kind == TokKind::Name && !is_keyword(&t.text) {
            self.pos += 1;
            Some(normalize_identifier(&t.text))
        } else {
            None
        }
    }

    fn hint(&mut self, err: SyntaxError) {
        let pos = self.pos;
        if pos > self.furthest {
            self.furthest = pos;
        }
        self.hints.push((pos, err));
    }

    fn enter(&mut self) -> R<()> {
        self.level += 1;
        if self.level > MAXSTACK {
            return Err(ParseFailure::Memory);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.level -= 1;
    }

    fn memo<F>(&mut self, key: u8, f: F) -> PO<Expr>
    where
        F: FnOnce(&mut Self) -> PO<Expr>,
    {
        let start = self.pos;
        if let Some((res, end)) = self.memo.get(&(key, start)) {
            self.pos = *end;
            return Ok(res.clone());
        }
        let res = f(self)?;
        if res.is_none() {
            self.pos = start;
        }
        self.memo.insert((key, start), (res.clone(), self.pos));
        Ok(res)
    }

    /// `','.elem+`.
    fn gather<T, F>(&mut self, mut elem: F) -> PO<Vec<T>>
    where
        F: FnMut(&mut Self) -> PO<T>,
    {
        let Some(first) = elem(self)? else {
            return Ok(None);
        };
        let mut items = vec![first];
        loop {
            let p = self.pos;
            if self.eat_op(",") {
                if let Some(e) = elem(self)? {
                    items.push(e);
                    continue;
                }
            }
            self.pos = p;
            break;
        }
        Ok(Some(items))
    }

    /// `elem*`.
    fn repeat<T, F>(&mut self, mut elem: F) -> R<Vec<T>>
    where
        F: FnMut(&mut Self) -> PO<T>,
    {
        let mut items = Vec::new();
        while let Some(e) = elem(self)? {
            items.push(e);
        }
        Ok(items)
    }

    // ------------------------------------------------------------------ statements

    fn file(&mut self) -> PO<Vec<Stmt>> {
        let stmts = self.statements()?.unwrap_or_default();
        if self.at_kind(TokKind::EndMarker) {
            Ok(Some(stmts))
        } else {
            Ok(None)
        }
    }

    fn statements(&mut self) -> PO<Vec<Stmt>> {
        let mut out = Vec::new();
        while let Some(mut s) = self.statement()? {
            out.append(&mut s);
        }
        Ok(if out.is_empty() { None } else { Some(out) })
    }

    fn statement(&mut self) -> PO<Vec<Stmt>> {
        if let Some(s) = self.compound_stmt()? {
            return Ok(Some(vec![s]));
        }
        self.simple_stmts()
    }

    fn simple_stmts(&mut self) -> PO<Vec<Stmt>> {
        let start = self.pos;
        let Some(first) = self.simple_stmt()? else {
            return Ok(None);
        };
        let mut out = vec![first];
        loop {
            if self.eat_kind(TokKind::Newline) {
                return Ok(Some(out));
            }
            if self.eat_op(";") {
                if self.eat_kind(TokKind::Newline) {
                    return Ok(Some(out));
                }
                match self.simple_stmt()? {
                    Some(s) => out.push(s),
                    None => {
                        self.pos = start;
                        return Ok(None);
                    }
                }
            } else {
                self.pos = start;
                return Ok(None);
            }
        }
    }

    fn simple_stmt(&mut self) -> PO<Stmt> {
        let line = self.line();
        if let Some(h) = self.assignment()? {
            return Ok(Some(stmt(line, StmtKind::Simple, 1 + h)));
        }
        if let Some(e) = self.star_expressions()? {
            return Ok(Some(stmt(line, StmtKind::Simple, 1 + e.height())));
        }
        if self.at_kw("return") {
            self.pos += 1;
            let h = self.star_expressions()?.map(|e| e.height()).unwrap_or(0);
            return Ok(Some(stmt(line, StmtKind::Simple, 1 + h)));
        }
        if self.at_kw("import") || self.at_kw("from") {
            if let Some(s) = self.import_stmt()? {
                return Ok(Some(s));
            }
        }
        if self.at_kw("raise") {
            let start = self.pos;
            self.pos += 1;
            if let Some(e) = self.expression()? {
                let mut h = e.height();
                let p = self.pos;
                if self.eat_kw("from") {
                    match self.expression()? {
                        Some(c) => h = h.max(c.height()),
                        None => self.pos = p,
                    }
                }
                return Ok(Some(stmt(line, StmtKind::Simple, 1 + h)));
            }
            self.pos = start + 1;
            return Ok(Some(stmt(line, StmtKind::Simple, 1)));
        }
        if self.eat_kw("pass") {
            return Ok(Some(stmt(line, StmtKind::Simple, 1)));
        }
        if self.at_kw("del") {
            let start = self.pos;
            self.pos += 1;
            if let Some(targets) = self.del_targets()?
                && (self.at_op(";") || self.at_kind(TokKind::Newline))
            {
                return Ok(Some(stmt(line, StmtKind::Simple, 1 + max_height(&targets))));
            }
            self.pos = start;
        }
        if self.at_kw("yield") {
            if let Some(e) = self.yield_expr()? {
                return Ok(Some(stmt(line, StmtKind::Simple, 2 + e.height())));
            }
        }
        if self.at_kw("assert") {
            let start = self.pos;
            self.pos += 1;
            if let Some(test) = self.expression()? {
                let mut h = test.height();
                let p = self.pos;
                if self.eat_op(",") {
                    match self.expression()? {
                        Some(m) => h = h.max(m.height()),
                        None => self.pos = p,
                    }
                }
                return Ok(Some(stmt(line, StmtKind::Simple, 1 + h)));
            }
            self.pos = start;
        }
        if self.eat_kw("break") || self.eat_kw("continue") {
            return Ok(Some(stmt(line, StmtKind::Simple, 1)));
        }
        if self.at_kw("global") || self.at_kw("nonlocal") {
            let start = self.pos;
            self.pos += 1;
            if self.gather(|p| Ok(p.name()))?.is_some() {
                return Ok(Some(stmt(line, StmtKind::Simple, 1)));
            }
            self.pos = start;
        }
        Ok(None)
    }

    /// `assignment`; returns the statement's height.
    fn assignment(&mut self) -> PO<usize> {
        let start = self.pos;
        // NAME ':' expression ['=' annotated_rhs]
        if self.name().is_some() && self.eat_op(":") {
            if let Some(ann) = self.expression()? {
                let mut h = 1 + ann.height().max(1);
                let p = self.pos;
                if self.eat_op("=") {
                    match self.annotated_rhs()? {
                        Some(v) => h = h.max(1 + v.height()),
                        None => self.pos = p,
                    }
                }
                return Ok(Some(h));
            }
        }
        self.pos = start;
        // ('(' single_target ')' | single_subscript_attribute_target) ':' expression ['=' annotated_rhs]
        let target = {
            let mut t = None;
            if self.eat_op("(") {
                if let Some(e) = self.single_target()?
                    && self.eat_op(")")
                {
                    t = Some(e);
                }
                if t.is_none() {
                    self.pos = start;
                }
            }
            if t.is_none() {
                t = self.single_subscript_attribute_target()?;
            }
            t
        };
        if let Some(target) = target {
            if self.eat_op(":") {
                if let Some(ann) = self.expression()? {
                    let mut h = 1 + target.height().max(ann.height());
                    let p = self.pos;
                    if self.eat_op("=") {
                        match self.annotated_rhs()? {
                            Some(v) => h = h.max(1 + v.height()),
                            None => self.pos = p,
                        }
                    }
                    return Ok(Some(h));
                }
            }
        }
        self.pos = start;
        // (star_targets '=')+ (yield_expr | star_expressions) !'='
        let mut targets_h = 0;
        let mut count = 0;
        loop {
            let p = self.pos;
            if let Some(t) = self.star_targets()?
                && self.eat_op("=")
            {
                targets_h = targets_h.max(t.height());
                count += 1;
                continue;
            }
            self.pos = p;
            break;
        }
        if count > 0 {
            let value = match self.yield_expr()? {
                Some(v) => Some(v),
                None => self.star_expressions()?,
            };
            if let Some(v) = value
                && !self.at_op("=")
            {
                return Ok(Some(1 + targets_h.max(v.height())));
            }
        }
        self.pos = start;
        // single_target augassign ~ (yield_expr | star_expressions)
        if let Some(t) = self.single_target()? {
            const AUG: &[&str] = &[
                "+=", "-=", "*=", "@=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "**=", "//=",
            ];
            let tok = self.peek();
            if tok.kind == TokKind::Op && AUG.contains(&tok.text.as_str()) {
                self.pos += 1;
                let value = match self.yield_expr()? {
                    Some(v) => Some(v),
                    None => self.star_expressions()?,
                };
                if let Some(v) = value {
                    return Ok(Some(1 + t.height().max(v.height())));
                }
            }
        }
        self.pos = start;
        Ok(None)
    }

    fn annotated_rhs(&mut self) -> PO<Expr> {
        if let Some(y) = self.yield_expr()? {
            return Ok(Some(y));
        }
        self.star_expressions()
    }

    fn import_stmt(&mut self) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        if self.eat_kw("import") {
            let names = self.gather(|p| {
                let Some(name) = p.dotted_name() else {
                    return Ok(None);
                };
                let q = p.pos;
                if p.eat_kw("as") && p.name().is_none() {
                    p.pos = q;
                }
                Ok(Some(name))
            })?;
            if let Some(names) = names {
                return Ok(Some(stmt(line, StmtKind::Import { names }, 2)));
            }
            self.pos = start;
            return Ok(None);
        }
        if !self.eat_kw("from") {
            return Ok(None);
        }
        let after_from = self.pos;
        // ('.' | '...')* dotted_name 'import' import_from_targets
        let mut level = 0;
        loop {
            if self.eat_op(".") {
                level += 1;
            } else if self.eat_op("...") {
                level += 3;
            } else {
                break;
            }
        }
        let dots_end = self.pos;
        if let Some(module) = self.dotted_name()
            && self.eat_kw("import")
            && let Some(names) = self.import_from_targets()?
        {
            return Ok(Some(stmt(
                line,
                StmtKind::ImportFrom {
                    module: Some(module),
                    names,
                },
                2,
            )));
        }
        // ('.' | '...')+ 'import' import_from_targets
        self.pos = dots_end;
        if level > 0
            && self.eat_kw("import")
            && let Some(names) = self.import_from_targets()?
        {
            return Ok(Some(stmt(
                line,
                StmtKind::ImportFrom {
                    module: None,
                    names,
                },
                2,
            )));
        }
        let _ = after_from;
        self.pos = start;
        Ok(None)
    }

    fn dotted_name(&mut self) -> Option<String> {
        let mut name = self.name()?;
        loop {
            let p = self.pos;
            if self.eat_op(".") {
                if let Some(n) = self.name() {
                    name.push('.');
                    name.push_str(&n);
                    continue;
                }
            }
            self.pos = p;
            break;
        }
        Some(name)
    }

    fn import_from_targets(&mut self) -> PO<Vec<String>> {
        let start = self.pos;
        let as_name = |p: &mut Self| -> PO<String> {
            let Some(name) = p.name() else {
                return Ok(None);
            };
            let q = p.pos;
            if p.eat_kw("as") && p.name().is_none() {
                p.pos = q;
            }
            Ok(Some(name))
        };
        if self.eat_op("(") {
            if let Some(names) = self.gather(as_name)? {
                self.eat_op(",");
                if self.eat_op(")") {
                    return Ok(Some(names));
                }
            }
            self.pos = start;
        }
        if let Some(names) = self.gather(as_name)? {
            if !self.at_op(",") {
                return Ok(Some(names));
            }
        }
        self.pos = start;
        if self.eat_op("*") {
            return Ok(Some(vec!["*".to_string()]));
        }
        Ok(None)
    }

    fn compound_stmt(&mut self) -> PO<Stmt> {
        if self.at_kw("def") || self.at_op("@") || self.at_kw("async") {
            if let Some(s) = self.function_def()? {
                return Ok(Some(s));
            }
        }
        if self.at_kw("if") {
            if let Some(s) = self.if_stmt()? {
                return Ok(Some(s));
            }
        }
        if self.at_kw("class") || self.at_op("@") {
            if let Some(s) = self.class_def()? {
                return Ok(Some(s));
            }
        }
        if self.at_kw("with") || self.at_kw("async") {
            if let Some(s) = self.with_stmt()? {
                return Ok(Some(s));
            }
        }
        if self.at_kw("for") || self.at_kw("async") {
            if let Some(s) = self.for_stmt()? {
                return Ok(Some(s));
            }
        }
        if self.at_kw("try") {
            if let Some(s) = self.try_stmt()? {
                return Ok(Some(s));
            }
        }
        if self.at_kw("while") {
            if let Some(s) = self.while_stmt()? {
                return Ok(Some(s));
            }
        }
        self.match_stmt()
    }

    fn block(&mut self, ctx: BlockCtx) -> PO<Vec<Stmt>> {
        let start = self.pos;
        if self.eat_kind(TokKind::Newline) {
            if self.at_kind(TokKind::Indent) {
                self.pos += 1;
                if let Some(s) = self.statements()?
                    && self.eat_kind(TokKind::Dedent)
                {
                    return Ok(Some(s));
                }
            } else {
                let line = self.line();
                self.hint(SyntaxError::indentation(
                    format!("expected an indented block after {} on line {}", ctx.what, ctx.line),
                    line,
                ));
            }
            self.pos = start;
        }
        self.simple_stmts()
    }

    fn expect_colon(&mut self) -> bool {
        if self.eat_op(":") {
            return true;
        }
        let line = self.line();
        self.hint(SyntaxError::new("expected ':'", line));
        false
    }

    fn decorators(&mut self) -> PO<Vec<Expr>> {
        let mut out = Vec::new();
        loop {
            let p = self.pos;
            if !self.eat_op("@") {
                break;
            }
            match self.named_expression()? {
                Some(e) if self.eat_kind(TokKind::Newline) => out.push(e),
                _ => {
                    self.pos = p;
                    break;
                }
            }
        }
        Ok(if out.is_empty() { None } else { Some(out) })
    }

    fn function_def(&mut self) -> PO<Stmt> {
        let start = self.pos;
        if let Some(decorators) = self.decorators()? {
            if let Some(s) = self.function_def_raw(decorators)? {
                return Ok(Some(s));
            }
            self.pos = start;
        }
        self.function_def_raw(Vec::new())
    }

    fn function_def_raw(&mut self, decorators: Vec<Expr>) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        let is_async = self.eat_kw("async");
        if !self.eat_kw("def") {
            self.pos = start;
            return Ok(None);
        }
        let Some(name) = self.name() else {
            self.pos = start;
            return Ok(None);
        };
        if !self.eat_op("(") {
            self.pos = start;
            return Ok(None);
        }
        let params = self.params(false)?.unwrap_or_default();
        if !self.eat_op(")") {
            self.pos = start;
            return Ok(None);
        }
        let mut returns_h = 0;
        let p = self.pos;
        if self.eat_op("->") {
            match self.expression()? {
                Some(r) => returns_h = r.height(),
                None => self.pos = p,
            }
        }
        if !self.expect_colon() {
            self.pos = start;
            return Ok(None);
        }
        let Some(body) = self.block(BlockCtx::new("function definition", line))? else {
            self.pos = start;
            return Ok(None);
        };
        let h = 1 + params
            .height()
            .max(stmts_height(&body))
            .max(max_height(&decorators))
            .max(returns_h);
        let args = params.args.iter().map(|a| a.name.clone()).collect();
        let decorators = decorators.iter().map(unparse).collect();
        Ok(Some(stmt(
            line,
            StmtKind::FunctionDef {
                name,
                args,
                decorators,
                body,
                is_async,
            },
            h,
        )))
    }

    fn class_def(&mut self) -> PO<Stmt> {
        let start = self.pos;
        if let Some(decorators) = self.decorators()? {
            if let Some(s) = self.class_def_raw(max_height(&decorators))? {
                return Ok(Some(s));
            }
            self.pos = start;
        }
        self.class_def_raw(0)
    }

    fn class_def_raw(&mut self, decorators_h: usize) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        if !self.eat_kw("class") {
            return Ok(None);
        }
        let Some(name) = self.name() else {
            self.pos = start;
            return Ok(None);
        };
        let mut bases = Vec::new();
        let mut keywords = Vec::new();
        let p = self.pos;
        if self.eat_op("(") {
            if let Some((a, k)) = self.arguments()? {
                bases = a;
                keywords = k;
            }
            if !self.eat_op(")") {
                bases.clear();
                keywords.clear();
                self.pos = p;
            }
        }
        if !self.expect_colon() {
            self.pos = start;
            return Ok(None);
        }
        let Some(body) = self.block(BlockCtx::new("class definition", line))? else {
            self.pos = start;
            return Ok(None);
        };
        let h = 1 + stmts_height(&body)
            .max(max_height(&bases))
            .max(keywords.iter().map(|k| 1 + k.value.height()).max().unwrap_or(0))
            .max(decorators_h);
        let bases = bases.iter().map(unparse).collect();
        Ok(Some(stmt(line, StmtKind::ClassDef { name, bases, body }, h)))
    }

    fn if_stmt(&mut self) -> PO<Stmt> {
        self.if_or_elif("if", "'if' statement")
    }

    fn if_or_elif(&mut self, kw: &str, what: &'static str) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        if !self.eat_kw(kw) {
            return Ok(None);
        }
        let Some(test) = self.named_expression()? else {
            self.pos = start;
            return Ok(None);
        };
        if !self.expect_colon() {
            self.pos = start;
            return Ok(None);
        }
        let Some(body) = self.block(BlockCtx::new(what, line))? else {
            self.pos = start;
            return Ok(None);
        };
        let mut h = test.height().max(stmts_height(&body));
        let mut children: Vec<Child> = body.into_iter().map(Child::Stmt).collect();
        let p = self.pos;
        if self.at_kw("elif") {
            if let Some(e) = self.if_or_elif("elif", "'elif' statement")? {
                h = h.max(e.height);
                children.push(Child::Stmt(e));
                return Ok(Some(stmt(line, StmtKind::Compound { children }, 1 + h)));
            }
            self.pos = p;
        }
        if let Some(orelse) = self.else_block()? {
            h = h.max(stmts_height(&orelse));
            children.extend(orelse.into_iter().map(Child::Stmt));
        }
        Ok(Some(stmt(line, StmtKind::Compound { children }, 1 + h)))
    }

    fn else_block(&mut self) -> PO<Vec<Stmt>> {
        let start = self.pos;
        let line = self.line();
        if !self.eat_kw("else") {
            return Ok(None);
        }
        if !self.eat_op(":") {
            let l = self.line();
            return syntax("expected ':'", l);
        }
        match self.block(BlockCtx::new("'else' statement", line))? {
            Some(b) => Ok(Some(b)),
            None => {
                self.pos = start;
                Ok(None)
            }
        }
    }

    fn while_stmt(&mut self) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        if !self.eat_kw("while") {
            return Ok(None);
        }
        let Some(test) = self.named_expression()? else {
            self.pos = start;
            return Ok(None);
        };
        if !self.expect_colon() {
            self.pos = start;
            return Ok(None);
        }
        let Some(body) = self.block(BlockCtx::new("'while' statement", line))? else {
            self.pos = start;
            return Ok(None);
        };
        let mut h = test.height().max(stmts_height(&body));
        let mut children: Vec<Child> = body.into_iter().map(Child::Stmt).collect();
        if let Some(orelse) = self.else_block()? {
            h = h.max(stmts_height(&orelse));
            children.extend(orelse.into_iter().map(Child::Stmt));
        }
        Ok(Some(stmt(line, StmtKind::Compound { children }, 1 + h)))
    }

    fn for_stmt(&mut self) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        self.eat_kw("async");
        if !self.eat_kw("for") {
            self.pos = start;
            return Ok(None);
        }
        let Some(target) = self.star_targets()? else {
            self.pos = start;
            return Ok(None);
        };
        if !self.eat_kw("in") {
            self.pos = start;
            return Ok(None);
        }
        let Some(iter) = self.star_expressions()? else {
            self.pos = start;
            return Ok(None);
        };
        if !self.expect_colon() {
            self.pos = start;
            return Ok(None);
        }
        let Some(body) = self.block(BlockCtx::new("'for' statement", line))? else {
            self.pos = start;
            return Ok(None);
        };
        let mut h = target.height().max(iter.height()).max(stmts_height(&body));
        let mut children: Vec<Child> = body.into_iter().map(Child::Stmt).collect();
        if let Some(orelse) = self.else_block()? {
            h = h.max(stmts_height(&orelse));
            children.extend(orelse.into_iter().map(Child::Stmt));
        }
        Ok(Some(stmt(line, StmtKind::Compound { children }, 1 + h)))
    }

    fn with_item(&mut self) -> PO<usize> {
        let Some(e) = self.expression()? else {
            return Ok(None);
        };
        let p = self.pos;
        if self.eat_kw("as") {
            if let Some(t) = self.star_target()?
                && (self.at_op(",") || self.at_op(")") || self.at_op(":"))
            {
                return Ok(Some(1 + e.height().max(t.height())));
            }
            self.pos = p;
        }
        Ok(Some(1 + e.height()))
    }

    fn with_stmt(&mut self) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        self.eat_kw("async");
        if !self.eat_kw("with") {
            self.pos = start;
            return Ok(None);
        }
        let after_with = self.pos;
        if self.eat_op("(") {
            if let Some(items) = self.gather(|p| p.with_item())? {
                self.eat_op(",");
                if self.eat_op(")") && self.eat_op(":") {
                    if let Some(body) = self.block(BlockCtx::new("'with' statement", line))? {
                        let h = 1 + items.iter().copied().max().unwrap_or(0).max(stmts_height(&body));
                        let children = body.into_iter().map(Child::Stmt).collect();
                        return Ok(Some(stmt(line, StmtKind::Compound { children }, h)));
                    }
                }
            }
            self.pos = after_with;
        }
        if let Some(items) = self.gather(|p| p.with_item())? {
            if self.expect_colon()
                && let Some(body) = self.block(BlockCtx::new("'with' statement", line))?
            {
                let h = 1 + items.iter().copied().max().unwrap_or(0).max(stmts_height(&body));
                let children = body.into_iter().map(Child::Stmt).collect();
                return Ok(Some(stmt(line, StmtKind::Compound { children }, h)));
            }
        }
        self.pos = start;
        Ok(None)
    }

    fn try_stmt(&mut self) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        if !self.eat_kw("try") {
            return Ok(None);
        }
        if !self.eat_op(":") {
            let l = self.line();
            return syntax("expected ':'", l);
        }
        let Some(body) = self.block(BlockCtx::new("'try' statement", line))? else {
            self.pos = start;
            return Ok(None);
        };
        let mut h = stmts_height(&body);
        let mut children: Vec<Child> = body.into_iter().map(Child::Stmt).collect();
        if let Some(fin) = self.finally_block()? {
            h = h.max(stmts_height(&fin));
            children.extend(fin.into_iter().map(Child::Stmt));
            return Ok(Some(stmt(line, StmtKind::Compound { children }, 1 + h)));
        }
        let mut handlers = Vec::new();
        while let Some(handler) = self.except_block(false)? {
            handlers.push(handler);
        }
        if handlers.is_empty() {
            while let Some(handler) = self.except_block(true)? {
                handlers.push(handler);
            }
        }
        if handlers.is_empty() {
            let l = self.line();
            self.hint(SyntaxError::new("expected 'except' or 'finally' block", l));
            self.pos = start;
            return Ok(None);
        }
        for (hh, group) in handlers {
            h = h.max(hh);
            children.push(Child::Group(group));
        }
        if let Some(orelse) = self.else_block()? {
            h = h.max(stmts_height(&orelse));
            children.extend(orelse.into_iter().map(Child::Stmt));
        }
        if let Some(fin) = self.finally_block()? {
            h = h.max(stmts_height(&fin));
            children.extend(fin.into_iter().map(Child::Stmt));
        }
        Ok(Some(stmt(line, StmtKind::Compound { children }, 1 + h)))
    }

    /// `except_block` (`star = false`) / `except_star_block` (`star = true`): the
    /// handler's height and body.
    fn except_block(&mut self, star: bool) -> PO<(usize, Vec<Stmt>)> {
        let start = self.pos;
        let line = self.line();
        if !self.eat_kw("except") {
            return Ok(None);
        }
        if star && !self.eat_op("*") {
            self.pos = start;
            return Ok(None);
        }
        let what = if star { "'except*' statement" } else { "'except' statement" };
        let after_kw = self.pos;
        // 'except' ['*'] expression ['as' NAME] ':' block
        if let Some(e) = self.expression()? {
            let p = self.pos;
            if self.eat_kw("as") && self.name().is_none() {
                self.pos = p;
            }
            if self.expect_colon()
                && let Some(body) = self.block(BlockCtx::new(what, line))?
            {
                return Ok(Some((1 + e.height().max(stmts_height(&body)), body)));
            }
        }
        // 'except' ':' block
        self.pos = after_kw;
        if !star && self.eat_op(":") {
            if let Some(body) = self.block(BlockCtx::new(what, line))? {
                return Ok(Some((1 + stmts_height(&body), body)));
            }
        }
        self.pos = start;
        Ok(None)
    }

    fn finally_block(&mut self) -> PO<Vec<Stmt>> {
        let start = self.pos;
        let line = self.line();
        if !self.eat_kw("finally") {
            return Ok(None);
        }
        if !self.eat_op(":") {
            let l = self.line();
            return syntax("expected ':'", l);
        }
        match self.block(BlockCtx::new("'finally' statement", line))? {
            Some(b) => Ok(Some(b)),
            None => {
                self.pos = start;
                Ok(None)
            }
        }
    }

    // ------------------------------------------------------------------------ match

    fn match_stmt(&mut self) -> PO<Stmt> {
        let start = self.pos;
        let line = self.line();
        if !self.at_kw("match") {
            return Ok(None);
        }
        self.pos += 1;
        // subject_expr: star_named_expression ',' star_named_expressions? | named_expression
        let subject = {
            let p = self.pos;
            let mut subj = None;
            if let Some(first) = self.star_named_expression()?
                && self.eat_op(",")
            {
                let mut elts = vec![first];
                if let Some(mut rest) = self.star_named_expressions()? {
                    elts.append(&mut rest);
                }
                subj = Some(Expr::Tuple(elts));
            }
            if subj.is_none() {
                self.pos = p;
                subj = self.named_expression()?;
            }
            subj
        };
        let Some(subject) = subject else {
            self.pos = start;
            return Ok(None);
        };
        if !(self.eat_op(":") && self.eat_kind(TokKind::Newline) && self.eat_kind(TokKind::Indent)) {
            self.pos = start;
            return Ok(None);
        }
        let mut cases = Vec::new();
        while let Some(case) = self.case_block()? {
            cases.push(case);
        }
        if cases.is_empty() || !self.eat_kind(TokKind::Dedent) {
            self.pos = start;
            return Ok(None);
        }
        let mut h = subject.height();
        let mut children = Vec::new();
        for (ch, body) in cases {
            h = h.max(ch);
            children.push(Child::Group(body));
        }
        Ok(Some(stmt(line, StmtKind::Compound { children }, 1 + h)))
    }

    fn case_block(&mut self) -> PO<(usize, Vec<Stmt>)> {
        let start = self.pos;
        let line = self.line();
        if !self.at_kw("case") {
            return Ok(None);
        }
        self.pos += 1;
        let Some(ph) = self.patterns()? else {
            self.pos = start;
            return Ok(None);
        };
        let mut h = ph;
        let p = self.pos;
        if self.eat_kw("if") {
            match self.named_expression()? {
                Some(g) => h = h.max(g.height()),
                None => self.pos = p,
            }
        }
        if !self.expect_colon() {
            self.pos = start;
            return Ok(None);
        }
        let Some(body) = self.block(BlockCtx::new("'case' statement", line))? else {
            self.pos = start;
            return Ok(None);
        };
        Ok(Some((1 + h.max(stmts_height(&body)), body)))
    }

    fn patterns(&mut self) -> PO<usize> {
        if let Some(h) = self.open_sequence_pattern()? {
            return Ok(Some(h + 1));
        }
        self.pattern()
    }

    fn pattern(&mut self) -> PO<usize> {
        // as_pattern: or_pattern 'as' pattern_capture_target
        let start = self.pos;
        if let Some(h) = self.or_pattern()? {
            if self.eat_kw("as") && self.pattern_capture_target() {
                return Ok(Some(h + 1));
            }
            self.pos = start;
        }
        self.or_pattern()
    }

    fn or_pattern(&mut self) -> PO<usize> {
        let Some(first) = self.closed_pattern()? else {
            return Ok(None);
        };
        let mut h = first;
        let mut n = 1;
        loop {
            let p = self.pos;
            if self.eat_op("|") {
                if let Some(c) = self.closed_pattern()? {
                    h = h.max(c);
                    n += 1;
                    continue;
                }
            }
            self.pos = p;
            break;
        }
        Ok(Some(if n > 1 { h + 1 } else { h }))
    }

    fn closed_pattern(&mut self) -> PO<usize> {
        self.enter()?;
        let r = self.closed_pattern_inner();
        self.leave();
        r
    }

    fn closed_pattern_inner(&mut self) -> PO<usize> {
        if let Some(h) = self.literal_pattern()? {
            return Ok(Some(h));
        }
        let start = self.pos;
        // capture_pattern
        if self.pattern_capture_target() {
            return Ok(Some(1));
        }
        // wildcard_pattern
        if self.at_kw("_") {
            self.pos += 1;
            return Ok(Some(1));
        }
        // value_pattern: attr !('.' | '(' | '=')
        if let Some(depth) = self.attr()
            && !(self.at_op(".") || self.at_op("(") || self.at_op("="))
        {
            return Ok(Some(2 + depth));
        }
        self.pos = start;
        // group_pattern: '(' pattern ')'
        if self.eat_op("(") {
            if let Some(h) = self.pattern()?
                && self.eat_op(")")
            {
                return Ok(Some(h));
            }
            self.pos = start;
        }
        // sequence_pattern
        if self.eat_op("[") {
            let h = self.maybe_sequence_pattern()?.unwrap_or(0);
            if self.eat_op("]") {
                return Ok(Some(h + 1));
            }
            self.pos = start;
        }
        if self.eat_op("(") {
            let h = self.open_sequence_pattern()?.unwrap_or(0);
            if self.eat_op(")") {
                return Ok(Some(h + 1));
            }
            self.pos = start;
        }
        // mapping_pattern
        if let Some(h) = self.mapping_pattern()? {
            return Ok(Some(h));
        }
        // class_pattern
        self.class_pattern()
    }

    fn pattern_capture_target(&mut self) -> bool {
        let start = self.pos;
        if self.at_kw("_") {
            return false;
        }
        if self.name().is_none() {
            return false;
        }
        if self.at_op(".") || self.at_op("(") || self.at_op("=") {
            self.pos = start;
            return false;
        }
        true
    }

    /// `attr: name_or_attr '.' NAME` (at least one dot); returns the number of dots.
    fn attr(&mut self) -> Option<usize> {
        let start = self.pos;
        self.name()?;
        let mut dots = 0;
        loop {
            let p = self.pos;
            if self.eat_op(".") && self.name().is_some() {
                dots += 1;
                continue;
            }
            self.pos = p;
            break;
        }
        if dots == 0 {
            self.pos = start;
            return None;
        }
        Some(dots)
    }

    fn name_or_attr(&mut self) -> Option<usize> {
        let start = self.pos;
        if let Some(d) = self.attr() {
            return Some(d);
        }
        self.pos = start;
        self.name().map(|_| 0)
    }

    /// `signed_number !('+' | '-') | complex_number | strings | 'None' | 'True' | 'False'`.
    fn literal_pattern(&mut self) -> PO<usize> {
        let start = self.pos;
        if self.signed_number() && !(self.at_op("+") || self.at_op("-")) {
            return Ok(Some(3));
        }
        self.pos = start;
        if self.complex_number()? {
            return Ok(Some(4));
        }
        self.pos = start;
        if self.at_kind(TokKind::Str) {
            if let Some(s) = self.strings()? {
                return Ok(Some(1 + s.height()));
            }
        }
        if self.eat_kw("None") || self.eat_kw("True") || self.eat_kw("False") {
            return Ok(Some(1));
        }
        Ok(None)
    }

    fn signed_number(&mut self) -> bool {
        let start = self.pos;
        self.eat_op("-");
        if self.eat_kind(TokKind::Number) {
            return true;
        }
        self.pos = start;
        false
    }

    fn number_is_imaginary(text: &str) -> bool {
        text.ends_with('j') || text.ends_with('J')
    }

    /// `signed_real_number ('+' | '-') imaginary_number`; the real/imaginary checks raise.
    fn complex_number(&mut self) -> R<bool> {
        let start = self.pos;
        self.eat_op("-");
        let t = self.peek();
        if t.kind != TokKind::Number {
            self.pos = start;
            return Ok(false);
        }
        if Self::number_is_imaginary(&t.text) {
            return syntax("real number required in complex literal", t.line);
        }
        self.pos += 1;
        if !(self.eat_op("+") || self.eat_op("-")) {
            self.pos = start;
            return Ok(false);
        }
        let t = self.peek();
        if t.kind != TokKind::Number {
            self.pos = start;
            return Ok(false);
        }
        if !Self::number_is_imaginary(&t.text) {
            return syntax("imaginary number required in complex literal", t.line);
        }
        self.pos += 1;
        Ok(true)
    }

    fn maybe_star_pattern(&mut self) -> PO<usize> {
        let start = self.pos;
        if self.eat_op("*") {
            if self.pattern_capture_target() || self.eat_kw("_") {
                return Ok(Some(1));
            }
            self.pos = start;
        }
        self.pattern()
    }

    fn maybe_sequence_pattern(&mut self) -> PO<usize> {
        let Some(items) = self.gather(|p| p.maybe_star_pattern())? else {
            return Ok(None);
        };
        self.eat_op(",");
        Ok(Some(items.into_iter().max().unwrap_or(0)))
    }

    fn open_sequence_pattern(&mut self) -> PO<usize> {
        let start = self.pos;
        let Some(first) = self.maybe_star_pattern()? else {
            return Ok(None);
        };
        if !self.eat_op(",") {
            self.pos = start;
            return Ok(None);
        }
        let rest = self.maybe_sequence_pattern()?.unwrap_or(0);
        Ok(Some(first.max(rest)))
    }

    fn mapping_pattern(&mut self) -> PO<usize> {
        let start = self.pos;
        if !self.eat_op("{") {
            return Ok(None);
        }
        if self.eat_op("}") {
            return Ok(Some(1));
        }
        let double_star = |p: &mut Self| -> bool {
            let q = p.pos;
            if p.eat_op("**") && p.pattern_capture_target() {
                return true;
            }
            p.pos = q;
            false
        };
        // '{' double_star_pattern ','? '}'
        if double_star(self) {
            self.eat_op(",");
            if self.eat_op("}") {
                return Ok(Some(1));
            }
            self.pos = start + 1;
        }
        // items_pattern
        let items = self.gather(|p| {
            let q = p.pos;
            let key_h = {
                let lit = p.literal_pattern()?;
                match lit {
                    Some(h) => Some(h),
                    None => {
                        p.pos = q;
                        p.attr().map(|d| d + 1)
                    }
                }
            };
            let Some(kh) = key_h else {
                p.pos = q;
                return Ok(None);
            };
            if !p.eat_op(":") {
                p.pos = q;
                return Ok(None);
            }
            match p.pattern()? {
                Some(ph) => Ok(Some(kh.max(ph))),
                None => {
                    p.pos = q;
                    Ok(None)
                }
            }
        })?;
        if let Some(items) = items {
            let h = items.into_iter().max().unwrap_or(0) + 1;
            let after_items = self.pos;
            if self.eat_op(",") && double_star(self) {
                self.eat_op(",");
                if self.eat_op("}") {
                    return Ok(Some(h));
                }
            }
            self.pos = after_items;
            self.eat_op(",");
            if self.eat_op("}") {
                return Ok(Some(h));
            }
        }
        self.pos = start;
        Ok(None)
    }

    fn class_pattern(&mut self) -> PO<usize> {
        let start = self.pos;
        let Some(cls) = self.name_or_attr() else {
            return Ok(None);
        };
        if !self.eat_op("(") {
            self.pos = start;
            return Ok(None);
        }
        let after_paren = self.pos;
        if self.eat_op(")") {
            return Ok(Some(2 + cls));
        }
        let keyword_pattern = |p: &mut Self| -> PO<usize> {
            let q = p.pos;
            if p.name().is_some() && p.eat_op("=") {
                if let Some(h) = p.pattern()? {
                    return Ok(Some(h));
                }
            }
            p.pos = q;
            Ok(None)
        };
        // positional_patterns ','? ')'
        if let Some(pos) = self.gather(|p| p.pattern())? {
            let h = pos.into_iter().max().unwrap_or(0);
            let after_pos = self.pos;
            self.eat_op(",");
            if self.eat_op(")") {
                return Ok(Some(2 + h.max(cls)));
            }
            // positional_patterns ',' keyword_patterns ','? ')'
            self.pos = after_pos;
            if self.eat_op(",") {
                if let Some(kw) = self.gather(keyword_pattern)? {
                    let h2 = kw.into_iter().max().unwrap_or(0);
                    self.eat_op(",");
                    if self.eat_op(")") {
                        return Ok(Some(2 + h.max(h2).max(cls)));
                    }
                }
            }
        }
        self.pos = after_paren;
        // keyword_patterns ','? ')'
        if let Some(kw) = self.gather(keyword_pattern)? {
            let h = kw.into_iter().max().unwrap_or(0);
            self.eat_op(",");
            if self.eat_op(")") {
                return Ok(Some(2 + h.max(cls)));
            }
        }
        self.pos = start;
        Ok(None)
    }

    // ---------------------------------------------------------------------- params

    fn param(&mut self, lambda: bool) -> PO<Arg> {
        let Some(name) = self.name() else {
            return Ok(None);
        };
        let mut annotation = None;
        if !lambda {
            let p = self.pos;
            if self.eat_op(":") {
                match self.expression()? {
                    Some(e) => annotation = Some(e),
                    None => self.pos = p,
                }
            }
        }
        Ok(Some(Arg { name, annotation }))
    }

    fn param_end(&mut self, lambda: bool) -> bool {
        if self.eat_op(",") {
            return true;
        }
        self.at_op(if lambda { ":" } else { ")" })
    }

    fn default(&mut self) -> PO<Expr> {
        let p = self.pos;
        if self.eat_op("=") {
            if let Some(e) = self.expression()? {
                return Ok(Some(e));
            }
        }
        self.pos = p;
        Ok(None)
    }

    fn param_no_default(&mut self, lambda: bool) -> PO<Arg> {
        let start = self.pos;
        if let Some(a) = self.param(lambda)?
            && self.param_end(lambda)
        {
            return Ok(Some(a));
        }
        self.pos = start;
        Ok(None)
    }

    fn param_with_default(&mut self, lambda: bool) -> PO<(Arg, Expr)> {
        let start = self.pos;
        if let Some(a) = self.param(lambda)?
            && let Some(d) = self.default()?
            && self.param_end(lambda)
        {
            return Ok(Some((a, d)));
        }
        self.pos = start;
        Ok(None)
    }

    fn param_maybe_default(&mut self, lambda: bool) -> PO<(Arg, Option<Expr>)> {
        let start = self.pos;
        if let Some(a) = self.param(lambda)? {
            let d = self.default()?;
            if self.param_end(lambda) {
                return Ok(Some((a, d)));
            }
        }
        self.pos = start;
        Ok(None)
    }

    /// `'/' ','` or `'/' &')'`.
    fn slash(&mut self, lambda: bool) -> bool {
        let p = self.pos;
        if self.eat_op("/") && self.param_end(lambda) {
            return true;
        }
        self.pos = p;
        false
    }

    fn params(&mut self, lambda: bool) -> PO<Arguments> {
        let start = self.pos;
        // slash_no_default param_no_default* param_with_default* [star_etc]
        {
            let nd = self.repeat(|p| p.param_no_default(lambda))?;
            if !nd.is_empty() && self.slash(lambda) {
                let args = self.repeat(|p| p.param_no_default(lambda))?;
                let wd = self.repeat(|p| p.param_with_default(lambda))?;
                let mut a = Arguments {
                    posonlyargs: nd,
                    args,
                    ..Arguments::default()
                };
                for (arg, d) in wd {
                    a.args.push(arg);
                    a.defaults.push(d);
                }
                self.star_etc(lambda, &mut a)?;
                return Ok(Some(a));
            }
            self.pos = start;
        }
        // slash_with_default param_with_default* [star_etc]
        {
            let nd = self.repeat(|p| p.param_no_default(lambda))?;
            let wd = self.repeat(|p| p.param_with_default(lambda))?;
            if !wd.is_empty() && self.slash(lambda) {
                let more = self.repeat(|p| p.param_with_default(lambda))?;
                let mut a = Arguments {
                    posonlyargs: nd,
                    ..Arguments::default()
                };
                for (arg, d) in wd {
                    a.posonlyargs.push(arg);
                    a.defaults.push(d);
                }
                for (arg, d) in more {
                    a.args.push(arg);
                    a.defaults.push(d);
                }
                self.star_etc(lambda, &mut a)?;
                return Ok(Some(a));
            }
            self.pos = start;
        }
        // param_no_default+ param_with_default* [star_etc]
        {
            let nd = self.repeat(|p| p.param_no_default(lambda))?;
            if !nd.is_empty() {
                let wd = self.repeat(|p| p.param_with_default(lambda))?;
                let mut a = Arguments {
                    args: nd,
                    ..Arguments::default()
                };
                for (arg, d) in wd {
                    a.args.push(arg);
                    a.defaults.push(d);
                }
                self.star_etc(lambda, &mut a)?;
                return Ok(Some(a));
            }
            self.pos = start;
        }
        // param_with_default+ [star_etc]
        {
            let wd = self.repeat(|p| p.param_with_default(lambda))?;
            if !wd.is_empty() {
                let mut a = Arguments::default();
                for (arg, d) in wd {
                    a.args.push(arg);
                    a.defaults.push(d);
                }
                self.star_etc(lambda, &mut a)?;
                return Ok(Some(a));
            }
            self.pos = start;
        }
        // star_etc
        let mut a = Arguments::default();
        if self.star_etc(lambda, &mut a)? {
            return Ok(Some(a));
        }
        self.pos = start;
        Ok(None)
    }

    /// `[star_etc]` into `a`; returns whether it matched.
    fn star_etc(&mut self, lambda: bool, a: &mut Arguments) -> R<bool> {
        let start = self.pos;
        if self.eat_op("*") {
            let after_star = self.pos;
            // '*' param_no_default param_maybe_default* [kwds]
            if let Some(v) = self.param_no_default(lambda)? {
                let kw = self.repeat(|p| p.param_maybe_default(lambda))?;
                a.vararg = Some(v);
                for (arg, d) in kw {
                    a.kwonlyargs.push(arg);
                    a.kw_defaults.push(d);
                }
                self.kwds(lambda, a)?;
                return Ok(true);
            }
            self.pos = after_star;
            // '*' param_no_default_star_annotation param_maybe_default* [kwds]
            if !lambda {
                if let Some(name) = self.name() {
                    let q = self.pos;
                    if self.eat_op(":") {
                        if let Some(ann) = self.star_expression()?
                            && self.param_end(false)
                        {
                            let kw = self.repeat(|p| p.param_maybe_default(false))?;
                            a.vararg = Some(Arg {
                                name,
                                annotation: Some(ann),
                            });
                            for (arg, d) in kw {
                                a.kwonlyargs.push(arg);
                                a.kw_defaults.push(d);
                            }
                            self.kwds(false, a)?;
                            return Ok(true);
                        }
                    }
                    let _ = q;
                }
                self.pos = after_star;
            }
            // '*' ',' param_maybe_default+ [kwds]
            if self.eat_op(",") {
                let kw = self.repeat(|p| p.param_maybe_default(lambda))?;
                if !kw.is_empty() {
                    for (arg, d) in kw {
                        a.kwonlyargs.push(arg);
                        a.kw_defaults.push(d);
                    }
                    self.kwds(lambda, a)?;
                    return Ok(true);
                }
            }
            self.pos = start;
        }
        // kwds
        if self.kwds(lambda, a)? {
            return Ok(true);
        }
        self.pos = start;
        Ok(false)
    }

    fn kwds(&mut self, lambda: bool, a: &mut Arguments) -> R<bool> {
        let start = self.pos;
        if self.eat_op("**") {
            if let Some(k) = self.param_no_default(lambda)? {
                a.kwarg = Some(k);
                return Ok(true);
            }
        }
        self.pos = start;
        Ok(false)
    }

    // ----------------------------------------------------------------- expressions

    fn star_expressions(&mut self) -> PO<Expr> {
        let Some(first) = self.star_expression()? else {
            return Ok(None);
        };
        if !self.at_op(",") {
            return Ok(Some(first));
        }
        let mut elts = vec![first];
        while self.eat_op(",") {
            match self.star_expression()? {
                Some(e) => elts.push(e),
                None => break,
            }
        }
        Ok(Some(Expr::Tuple(elts)))
    }

    fn star_expression(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_op("*") {
            if let Some(e) = self.bitwise_or()? {
                return Ok(Some(Expr::Starred(bx(e))));
            }
            self.pos = start;
        }
        self.expression()
    }

    fn star_named_expressions(&mut self) -> PO<Vec<Expr>> {
        let Some(items) = self.gather(|p| p.star_named_expression())? else {
            return Ok(None);
        };
        self.eat_op(",");
        Ok(Some(items))
    }

    fn star_named_expression(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_op("*") {
            if let Some(e) = self.bitwise_or()? {
                return Ok(Some(Expr::Starred(bx(e))));
            }
            self.pos = start;
        }
        self.named_expression()
    }

    fn assignment_expression(&mut self) -> PO<Expr> {
        let start = self.pos;
        if let Some(n) = self.name()
            && self.eat_op(":=")
            && let Some(v) = self.expression()?
        {
            return Ok(Some(Expr::NamedExpr(bx(Expr::Name(n)), bx(v))));
        }
        self.pos = start;
        Ok(None)
    }

    fn named_expression(&mut self) -> PO<Expr> {
        self.memo(M_NAMED, |p| {
            if let Some(e) = p.assignment_expression()? {
                return Ok(Some(e));
            }
            let start = p.pos;
            let e = p.expression()?;
            if e.is_some() && p.at_op(":=") {
                p.pos = start;
                return Ok(None);
            }
            Ok(e)
        })
    }

    fn expression(&mut self) -> PO<Expr> {
        self.memo(M_EXPRESSION, |p| {
            p.enter()?;
            let r = p.expression_inner();
            p.leave();
            r
        })
    }

    fn expression_inner(&mut self) -> PO<Expr> {
        if self.at_kw("lambda") {
            return self.lambdef();
        }
        let Some(d) = self.disjunction()? else {
            return Ok(None);
        };
        let after = self.pos;
        if self.eat_kw("if") {
            if let Some(test) = self.disjunction()?
                && self.eat_kw("else")
                && let Some(orelse) = self.expression()?
            {
                return Ok(Some(Expr::IfExp(bx(test), bx(d), bx(orelse))));
            }
            self.pos = after;
        }
        Ok(Some(d))
    }

    fn lambdef(&mut self) -> PO<Expr> {
        let start = self.pos;
        if !self.eat_kw("lambda") {
            return Ok(None);
        }
        let args = self.params(true)?.unwrap_or_default();
        if self.eat_op(":")
            && let Some(body) = self.expression()?
        {
            return Ok(Some(Expr::Lambda(Box::new(args), bx(body))));
        }
        self.pos = start;
        Ok(None)
    }

    fn yield_expr(&mut self) -> PO<Expr> {
        let start = self.pos;
        if !self.eat_kw("yield") {
            return Ok(None);
        }
        let p = self.pos;
        if self.eat_kw("from") {
            if let Some(e) = self.expression()? {
                return Ok(Some(Expr::YieldFrom(bx(e))));
            }
            self.pos = p;
        }
        let _ = start;
        let value = self.star_expressions()?;
        Ok(Some(Expr::Yield(value.map(bx))))
    }

    fn disjunction(&mut self) -> PO<Expr> {
        self.memo(M_DISJUNCTION, |p| p.bool_chain("or", BoolOpKind::Or))
    }

    fn bool_chain(&mut self, kw: &str, kind: BoolOpKind) -> PO<Expr> {
        let first = if kind == BoolOpKind::Or {
            self.bool_chain("and", BoolOpKind::And)?
        } else {
            self.inversion()?
        };
        let Some(first) = first else {
            return Ok(None);
        };
        let mut values = vec![first];
        loop {
            let p = self.pos;
            if self.eat_kw(kw) {
                let next = if kind == BoolOpKind::Or {
                    self.bool_chain("and", BoolOpKind::And)?
                } else {
                    self.inversion()?
                };
                if let Some(n) = next {
                    values.push(n);
                    continue;
                }
            }
            self.pos = p;
            break;
        }
        if values.len() == 1 {
            Ok(values.pop())
        } else {
            Ok(Some(Expr::BoolOp(kind, values)))
        }
    }

    fn inversion(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_kw("not") {
            self.enter()?;
            let r = self.inversion();
            self.leave();
            if let Some(e) = r? {
                return Ok(Some(Expr::UnaryOp(UnaryOpKind::Not, bx(e))));
            }
            self.pos = start;
            return Ok(None);
        }
        self.comparison()
    }

    fn comparison(&mut self) -> PO<Expr> {
        let Some(left) = self.bitwise_or()? else {
            return Ok(None);
        };
        let mut ops = Vec::new();
        let mut comparators = Vec::new();
        loop {
            let p = self.pos;
            let t = self.peek();
            let op = match (t.kind, t.text.as_str()) {
                (TokKind::Op, "==") => Some(CmpOpKind::Eq),
                (TokKind::Op, "!=") => Some(CmpOpKind::NotEq),
                (TokKind::Op, "<=") => Some(CmpOpKind::LtE),
                (TokKind::Op, "<") => Some(CmpOpKind::Lt),
                (TokKind::Op, ">=") => Some(CmpOpKind::GtE),
                (TokKind::Op, ">") => Some(CmpOpKind::Gt),
                (TokKind::Name, "not") => {
                    self.pos += 1;
                    if self.at_kw("in") {
                        Some(CmpOpKind::NotIn)
                    } else {
                        self.pos = p;
                        None
                    }
                }
                (TokKind::Name, "in") => Some(CmpOpKind::In),
                (TokKind::Name, "is") => {
                    self.pos += 1;
                    if self.at_kw("not") {
                        // 'is' 'not' bitwise_or, else 'is' bitwise_or
                        let q = self.pos;
                        self.pos += 1;
                        if let Some(r) = self.bitwise_or()? {
                            ops.push(CmpOpKind::IsNot);
                            comparators.push(r);
                            continue;
                        }
                        self.pos = q;
                    }
                    self.pos = p;
                    Some(CmpOpKind::Is)
                }
                _ => None,
            };
            let Some(op) = op else {
                self.pos = p;
                break;
            };
            self.pos += 1;
            match self.bitwise_or()? {
                Some(r) => {
                    ops.push(op);
                    comparators.push(r);
                }
                None => {
                    self.pos = p;
                    break;
                }
            }
        }
        if ops.is_empty() {
            Ok(Some(left))
        } else {
            Ok(Some(Expr::Compare(bx(left), ops, comparators)))
        }
    }

    fn binop_chain(&mut self, level: usize) -> PO<Expr> {
        // level 0: '|', 1: '^', 2: '&', 3: shifts, 4: sums, 5: terms
        const LEVELS: &[&[(&str, BinOpKind)]] = &[
            &[("|", BinOpKind::BitOr)],
            &[("^", BinOpKind::BitXor)],
            &[("&", BinOpKind::BitAnd)],
            &[("<<", BinOpKind::LShift), (">>", BinOpKind::RShift)],
            &[("+", BinOpKind::Add), ("-", BinOpKind::Sub)],
            &[
                ("*", BinOpKind::Mult),
                ("/", BinOpKind::Div),
                ("//", BinOpKind::FloorDiv),
                ("%", BinOpKind::Mod),
                ("@", BinOpKind::MatMult),
            ],
        ];
        let next = |p: &mut Self| -> PO<Expr> {
            if level + 1 < LEVELS.len() {
                p.binop_chain(level + 1)
            } else {
                p.factor()
            }
        };
        let Some(mut left) = next(self)? else {
            return Ok(None);
        };
        loop {
            let p = self.pos;
            let t = self.peek();
            let op = if t.kind == TokKind::Op {
                LEVELS[level].iter().find(|(s, _)| *s == t.text).map(|(_, k)| *k)
            } else {
                None
            };
            let Some(op) = op else {
                break;
            };
            self.pos += 1;
            match next(self)? {
                Some(r) => left = Expr::BinOp(bx(left), op, bx(r)),
                None => {
                    self.pos = p;
                    break;
                }
            }
        }
        Ok(Some(left))
    }

    fn bitwise_or(&mut self) -> PO<Expr> {
        self.memo(M_BITWISE_OR, |p| p.binop_chain(0))
    }

    fn factor(&mut self) -> PO<Expr> {
        let start = self.pos;
        let t = self.peek();
        let op = match (t.kind, t.text.as_str()) {
            (TokKind::Op, "+") => Some(UnaryOpKind::UAdd),
            (TokKind::Op, "-") => Some(UnaryOpKind::USub),
            (TokKind::Op, "~") => Some(UnaryOpKind::Invert),
            _ => None,
        };
        if let Some(op) = op {
            self.pos += 1;
            self.enter()?;
            let r = self.factor();
            self.leave();
            if let Some(e) = r? {
                return Ok(Some(Expr::UnaryOp(op, bx(e))));
            }
            self.pos = start;
            return Ok(None);
        }
        self.power()
    }

    fn power(&mut self) -> PO<Expr> {
        let Some(base) = self.await_primary()? else {
            return Ok(None);
        };
        let p = self.pos;
        if self.eat_op("**") {
            self.enter()?;
            let r = self.factor();
            self.leave();
            if let Some(exp) = r? {
                return Ok(Some(Expr::BinOp(bx(base), BinOpKind::Pow, bx(exp))));
            }
            self.pos = p;
        }
        Ok(Some(base))
    }

    fn await_primary(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_kw("await") {
            if let Some(e) = self.primary()? {
                return Ok(Some(Expr::Await(bx(e))));
            }
            self.pos = start;
            return Ok(None);
        }
        self.primary()
    }

    fn primary(&mut self) -> PO<Expr> {
        self.memo(M_PRIMARY, |p| {
            let Some(mut e) = p.atom()? else {
                return Ok(None);
            };
            loop {
                let q = p.pos;
                if p.eat_op(".") {
                    if let Some(n) = p.name() {
                        e = Expr::Attribute(bx(e), n);
                        continue;
                    }
                    p.pos = q;
                    break;
                }
                if p.at_op("(") {
                    if let Some(g) = p.genexp()? {
                        e = Expr::Call(bx(e), vec![g], Vec::new());
                        continue;
                    }
                    p.pos = q;
                    p.pos += 1;
                    let (args, keywords) = p.arguments()?.unwrap_or_default();
                    if p.eat_op(")") {
                        e = Expr::Call(bx(e), args, keywords);
                        continue;
                    }
                    p.pos = q;
                    break;
                }
                if p.eat_op("[") {
                    if let Some(s) = p.slices()?
                        && p.eat_op("]")
                    {
                        e = Expr::Subscript(bx(e), bx(s));
                        continue;
                    }
                    p.pos = q;
                    break;
                }
                break;
            }
            Ok(Some(e))
        })
    }

    fn slices(&mut self) -> PO<Expr> {
        let start = self.pos;
        if let Some(s) = self.slice()?
            && !self.at_op(",")
        {
            return Ok(Some(s));
        }
        self.pos = start;
        let items = self.gather(|p| match p.slice()? {
            Some(s) => Ok(Some(s)),
            None => p.starred_expression(),
        })?;
        let Some(items) = items else {
            self.pos = start;
            return Ok(None);
        };
        self.eat_op(",");
        Ok(Some(Expr::Tuple(items)))
    }

    fn slice(&mut self) -> PO<Expr> {
        let start = self.pos;
        let lower = self.expression()?;
        if self.eat_op(":") {
            let upper = self.expression()?;
            let mut step = None;
            if self.eat_op(":") {
                // `[':' [expression]]`: a lone second ':' is part of the slice.
                step = self.expression()?;
            }
            return Ok(Some(Expr::Slice(lower.map(bx), upper.map(bx), step.map(bx))));
        }
        self.pos = start;
        self.named_expression()
    }

    fn starred_expression(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_op("*")
            && let Some(e) = self.expression()?
        {
            return Ok(Some(Expr::Starred(bx(e))));
        }
        self.pos = start;
        Ok(None)
    }

    fn atom(&mut self) -> PO<Expr> {
        self.memo(M_ATOM, |p| {
            p.enter()?;
            let r = p.atom_inner();
            p.leave();
            r
        })
    }

    fn atom_inner(&mut self) -> PO<Expr> {
        let t = self.peek();
        match t.kind {
            TokKind::Name => {
                let e = match t.text.as_str() {
                    "True" => Expr::Constant(Const::True, false),
                    "False" => Expr::Constant(Const::False, false),
                    "None" => Expr::Constant(Const::None, false),
                    kw if is_keyword(kw) => return Ok(None),
                    raw => Expr::Name(normalize_identifier(raw)),
                };
                self.pos += 1;
                Ok(Some(e))
            }
            TokKind::Str => self.strings(),
            TokKind::Number => {
                let c = parse_number(&t.text, t.line)?;
                self.pos += 1;
                Ok(Some(Expr::Constant(c, false)))
            }
            TokKind::Op => match t.text.as_str() {
                "(" => {
                    if let Some(e) = self.tuple()? {
                        return Ok(Some(e));
                    }
                    if let Some(e) = self.group()? {
                        return Ok(Some(e));
                    }
                    self.genexp()
                }
                "[" => {
                    if let Some(e) = self.list()? {
                        return Ok(Some(e));
                    }
                    self.listcomp()
                }
                "{" => {
                    if let Some(e) = self.dict()? {
                        return Ok(Some(e));
                    }
                    if let Some(e) = self.set()? {
                        return Ok(Some(e));
                    }
                    if let Some(e) = self.dictcomp()? {
                        return Ok(Some(e));
                    }
                    self.setcomp()
                }
                "..." => {
                    self.pos += 1;
                    Ok(Some(Expr::Constant(Const::Ellipsis, false)))
                }
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    fn strings(&mut self) -> PO<Expr> {
        self.memo(M_STRINGS, |p| {
            let start = p.pos;
            while p.at_kind(TokKind::Str) {
                p.pos += 1;
            }
            if p.pos == start {
                return Ok(None);
            }
            let toks = &p.toks[start..p.pos];
            Ok(Some(concatenate_strings(toks)?))
        })
    }

    fn tuple(&mut self) -> PO<Expr> {
        let start = self.pos;
        if !self.eat_op("(") {
            return Ok(None);
        }
        let mut elts = Vec::new();
        let p = self.pos;
        if let Some(first) = self.star_named_expression()? {
            if self.eat_op(",") {
                elts.push(first);
                if let Some(mut rest) = self.star_named_expressions()? {
                    elts.append(&mut rest);
                }
            } else {
                self.pos = p;
            }
        }
        if self.eat_op(")") {
            return Ok(Some(Expr::Tuple(elts)));
        }
        self.pos = start;
        Ok(None)
    }

    fn group(&mut self) -> PO<Expr> {
        let start = self.pos;
        if !self.eat_op("(") {
            return Ok(None);
        }
        let inner = match self.yield_expr()? {
            Some(y) => Some(y),
            None => self.named_expression()?,
        };
        if let Some(e) = inner
            && self.eat_op(")")
        {
            return Ok(Some(e));
        }
        self.pos = start;
        Ok(None)
    }

    fn genexp(&mut self) -> PO<Expr> {
        let start = self.pos;
        if !self.eat_op("(") {
            return Ok(None);
        }
        let elt = match self.assignment_expression()? {
            Some(e) => Some(e),
            None => {
                let q = self.pos;
                let e = self.expression()?;
                if e.is_some() && self.at_op(":=") {
                    self.pos = q;
                    None
                } else {
                    e
                }
            }
        };
        if let Some(elt) = elt
            && let Some(gens) = self.for_if_clauses()?
            && self.eat_op(")")
        {
            return Ok(Some(Expr::GeneratorExp(bx(elt), gens)));
        }
        self.pos = start;
        Ok(None)
    }

    fn list(&mut self) -> PO<Expr> {
        let start = self.pos;
        if !self.eat_op("[") {
            return Ok(None);
        }
        let elts = self.star_named_expressions()?.unwrap_or_default();
        if self.eat_op("]") {
            return Ok(Some(Expr::List(elts)));
        }
        self.pos = start;
        Ok(None)
    }

    fn listcomp(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_op("[")
            && let Some(elt) = self.named_expression()?
            && let Some(gens) = self.for_if_clauses()?
            && self.eat_op("]")
        {
            return Ok(Some(Expr::ListComp(bx(elt), gens)));
        }
        self.pos = start;
        Ok(None)
    }

    fn kvpair(&mut self) -> PO<(Expr, Expr)> {
        let start = self.pos;
        if let Some(k) = self.expression()?
            && self.eat_op(":")
            && let Some(v) = self.expression()?
        {
            return Ok(Some((k, v)));
        }
        self.pos = start;
        Ok(None)
    }

    fn dict(&mut self) -> PO<Expr> {
        let start = self.pos;
        if !self.eat_op("{") {
            return Ok(None);
        }
        let items = self.gather(|p| {
            let q = p.pos;
            if p.eat_op("**") {
                if let Some(e) = p.bitwise_or()? {
                    return Ok(Some((None, e)));
                }
                p.pos = q;
                return Ok(None);
            }
            Ok(p.kvpair()?.map(|(k, v)| (Some(k), v)))
        })?;
        if items.is_some() {
            self.eat_op(",");
        }
        if self.eat_op("}") {
            let (keys, values) = items.unwrap_or_default().into_iter().unzip();
            return Ok(Some(Expr::Dict(keys, values)));
        }
        self.pos = start;
        Ok(None)
    }

    fn set(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_op("{")
            && let Some(elts) = self.star_named_expressions()?
            && self.eat_op("}")
        {
            return Ok(Some(Expr::Set(elts)));
        }
        self.pos = start;
        Ok(None)
    }

    fn dictcomp(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_op("{")
            && let Some((k, v)) = self.kvpair()?
            && let Some(gens) = self.for_if_clauses()?
            && self.eat_op("}")
        {
            return Ok(Some(Expr::DictComp(bx(k), bx(v), gens)));
        }
        self.pos = start;
        Ok(None)
    }

    fn setcomp(&mut self) -> PO<Expr> {
        let start = self.pos;
        if self.eat_op("{")
            && let Some(elt) = self.named_expression()?
            && let Some(gens) = self.for_if_clauses()?
            && self.eat_op("}")
        {
            return Ok(Some(Expr::SetComp(bx(elt), gens)));
        }
        self.pos = start;
        Ok(None)
    }

    fn for_if_clauses(&mut self) -> PO<Vec<Comprehension>> {
        let mut out = Vec::new();
        loop {
            let start = self.pos;
            let is_async = self.eat_kw("async");
            if !self.eat_kw("for") {
                self.pos = start;
                break;
            }
            let Some(target) = self.star_targets()? else {
                self.pos = start;
                break;
            };
            if !self.eat_kw("in") {
                self.pos = start;
                break;
            }
            let Some(iter) = self.disjunction()? else {
                self.pos = start;
                break;
            };
            let mut ifs = Vec::new();
            loop {
                let p = self.pos;
                if self.eat_kw("if") {
                    if let Some(c) = self.disjunction()? {
                        ifs.push(c);
                        continue;
                    }
                }
                self.pos = p;
                break;
            }
            out.push(Comprehension {
                target,
                iter,
                ifs,
                is_async,
            });
        }
        Ok(if out.is_empty() { None } else { Some(out) })
    }

    // ------------------------------------------------------------------- arguments

    fn arguments(&mut self) -> PO<(Vec<Expr>, Vec<Keyword>)> {
        let start = self.pos;
        if let Some(r) = self.args()? {
            self.eat_op(",");
            if self.at_op(")") {
                return Ok(Some(r));
            }
        }
        self.pos = start;
        Ok(None)
    }

    fn positional_arg(&mut self) -> PO<Expr> {
        if let Some(e) = self.starred_expression()? {
            return Ok(Some(e));
        }
        let start = self.pos;
        let e = match self.assignment_expression()? {
            Some(e) => Some(e),
            None => {
                let e = self.expression()?;
                if e.is_some() && self.at_op(":=") {
                    self.pos = start;
                    None
                } else {
                    e
                }
            }
        };
        match e {
            Some(e) if !self.at_op("=") => Ok(Some(e)),
            _ => {
                self.pos = start;
                Ok(None)
            }
        }
    }

    /// `NAME '=' expression | starred_expression` (`double = false`) or
    /// `NAME '=' expression | '**' expression` (`double = true`).
    fn kwarg_item(&mut self, double: bool) -> PO<(Option<Expr>, Option<Keyword>)> {
        let start = self.pos;
        if let Some(n) = self.name()
            && self.eat_op("=")
            && let Some(v) = self.expression()?
        {
            return Ok(Some((
                None,
                Some(Keyword {
                    arg: Some(n),
                    value: v,
                }),
            )));
        }
        self.pos = start;
        if double {
            if self.eat_op("**")
                && let Some(v) = self.expression()?
            {
                return Ok(Some((None, Some(Keyword { arg: None, value: v }))));
            }
            self.pos = start;
            return Ok(None);
        }
        Ok(self.starred_expression()?.map(|e| (Some(e), None)))
    }

    fn kwargs(&mut self) -> PO<(Vec<Expr>, Vec<Keyword>)> {
        let start = self.pos;
        let collect = |items: Vec<(Option<Expr>, Option<Keyword>)>, a: &mut Vec<Expr>, k: &mut Vec<Keyword>| {
            for (e, kw) in items {
                if let Some(e) = e {
                    a.push(e);
                }
                if let Some(kw) = kw {
                    k.push(kw);
                }
            }
        };
        // ','.kwarg_or_starred+ ',' ','.kwarg_or_double_starred+
        if let Some(first) = self.gather(|p| p.kwarg_item(false))? {
            let p = self.pos;
            if self.eat_op(",")
                && let Some(second) = self.gather(|p| p.kwarg_item(true))?
            {
                let (mut a, mut k) = (Vec::new(), Vec::new());
                collect(first, &mut a, &mut k);
                collect(second, &mut a, &mut k);
                return Ok(Some((a, k)));
            }
            self.pos = p;
            let (mut a, mut k) = (Vec::new(), Vec::new());
            collect(first, &mut a, &mut k);
            return Ok(Some((a, k)));
        }
        self.pos = start;
        if let Some(items) = self.gather(|p| p.kwarg_item(true))? {
            let (mut a, mut k) = (Vec::new(), Vec::new());
            collect(items, &mut a, &mut k);
            return Ok(Some((a, k)));
        }
        self.pos = start;
        Ok(None)
    }

    fn args(&mut self) -> PO<(Vec<Expr>, Vec<Keyword>)> {
        let start = self.pos;
        if let Some(mut positional) = self.gather(|p| p.positional_arg())? {
            let mut keywords = Vec::new();
            let p = self.pos;
            if self.eat_op(",") {
                match self.kwargs()? {
                    Some((mut a, k)) => {
                        positional.append(&mut a);
                        keywords = k;
                    }
                    None => self.pos = p,
                }
            }
            return Ok(Some((positional, keywords)));
        }
        self.pos = start;
        self.kwargs()
    }

    // --------------------------------------------------------------------- targets

    fn star_targets(&mut self) -> PO<Expr> {
        let Some(first) = self.star_target()? else {
            return Ok(None);
        };
        if !self.at_op(",") {
            return Ok(Some(first));
        }
        let mut elts = vec![first];
        while self.eat_op(",") {
            match self.star_target()? {
                Some(t) => elts.push(t),
                None => break,
            }
        }
        Ok(Some(Expr::Tuple(elts)))
    }

    fn star_target(&mut self) -> PO<Expr> {
        self.memo(M_STAR_TARGET, |p| {
            let start = p.pos;
            if p.eat_op("*") {
                if !p.at_op("*") {
                    p.enter()?;
                    let r = p.star_target();
                    p.leave();
                    if let Some(t) = r? {
                        return Ok(Some(Expr::Starred(bx(t))));
                    }
                }
                p.pos = start;
                return Ok(None);
            }
            p.target_with_star_atom()
        })
    }

    /// `t_primary '.' NAME !t_lookahead | t_primary '[' slices ']' !t_lookahead`.
    fn subscript_attribute_target(&mut self) -> PO<Expr> {
        let start = self.pos;
        if let Some(e) = self.primary()?
            && matches!(e, Expr::Attribute(..) | Expr::Subscript(..))
        {
            return Ok(Some(e));
        }
        self.pos = start;
        Ok(None)
    }

    fn target_with_star_atom(&mut self) -> PO<Expr> {
        if let Some(e) = self.subscript_attribute_target()? {
            return Ok(Some(e));
        }
        self.star_atom()
    }

    fn star_atom(&mut self) -> PO<Expr> {
        let start = self.pos;
        if let Some(n) = self.name() {
            return Ok(Some(Expr::Name(n)));
        }
        if self.eat_op("(") {
            if let Some(t) = self.target_with_star_atom()?
                && self.eat_op(")")
            {
                return Ok(Some(t));
            }
            self.pos = start + 1;
            // '(' [star_targets_tuple_seq] ')'
            let mut elts = Vec::new();
            if let Some(first) = self.star_target()? {
                if self.eat_op(",") {
                    elts.push(first);
                    loop {
                        match self.star_target()? {
                            Some(t) => elts.push(t),
                            None => break,
                        }
                        if !self.eat_op(",") {
                            break;
                        }
                    }
                } else {
                    self.pos = start + 1;
                }
            }
            if self.eat_op(")") {
                return Ok(Some(Expr::Tuple(elts)));
            }
            self.pos = start;
            return Ok(None);
        }
        if self.eat_op("[") {
            let elts = match self.gather(|p| p.star_target())? {
                Some(e) => {
                    self.eat_op(",");
                    e
                }
                None => Vec::new(),
            };
            if self.eat_op("]") {
                return Ok(Some(Expr::List(elts)));
            }
            self.pos = start;
        }
        Ok(None)
    }

    fn single_target(&mut self) -> PO<Expr> {
        if let Some(e) = self.single_subscript_attribute_target()? {
            return Ok(Some(e));
        }
        if let Some(n) = self.name() {
            return Ok(Some(Expr::Name(n)));
        }
        let start = self.pos;
        if self.eat_op("(")
            && let Some(t) = self.single_target()?
            && self.eat_op(")")
        {
            return Ok(Some(t));
        }
        self.pos = start;
        Ok(None)
    }

    fn single_subscript_attribute_target(&mut self) -> PO<Expr> {
        self.subscript_attribute_target()
    }

    fn del_targets(&mut self) -> PO<Vec<Expr>> {
        let Some(items) = self.gather(|p| p.del_target())? else {
            return Ok(None);
        };
        self.eat_op(",");
        Ok(Some(items))
    }

    fn del_target(&mut self) -> PO<Expr> {
        if let Some(e) = self.subscript_attribute_target()? {
            return Ok(Some(e));
        }
        let start = self.pos;
        if let Some(n) = self.name() {
            return Ok(Some(Expr::Name(n)));
        }
        if self.eat_op("(") {
            if let Some(t) = self.del_target()?
                && self.eat_op(")")
            {
                return Ok(Some(t));
            }
            self.pos = start + 1;
            let elts = self.del_targets()?.unwrap_or_default();
            if self.eat_op(")") {
                return Ok(Some(Expr::Tuple(elts)));
            }
            self.pos = start;
            return Ok(None);
        }
        if self.eat_op("[") {
            let elts = self.del_targets()?.unwrap_or_default();
            if self.eat_op("]") {
                return Ok(Some(Expr::List(elts)));
            }
            self.pos = start;
        }
        Ok(None)
    }

    /// The error for a failed parse.
    fn failure(&self, tok_err: Option<(usize, ParseFailure)>) -> ParseFailure {
        if let Some((idx, e)) = tok_err
            && self.furthest >= idx
        {
            return e;
        }
        if let Some((_, h)) = self.hints.iter().rev().find(|(pos, _)| *pos == self.furthest) {
            return ParseFailure::Syntax(h.clone());
        }
        let t = &self.toks[self.furthest.min(self.toks.len() - 1)];
        match t.kind {
            TokKind::Indent => {
                ParseFailure::Syntax(SyntaxError::indentation("unexpected indent", t.line))
            }
            TokKind::Dedent => {
                ParseFailure::Syntax(SyntaxError::indentation("unexpected unindent", t.line))
            }
            _ => ParseFailure::Syntax(SyntaxError::new("invalid syntax", t.line)),
        }
    }
}

/// CPython converts the AST to Python objects with a recursion limit of three times the
/// interpreter's (3000), less what the calling Python frames already use; this is the
/// limit for a call from a top-level script through `CodeParser`.
const AST_HEIGHT_LIMIT: usize = 2985;

fn parse_inner(source: &str) -> Result<Module, ParseFailure> {
    let (toks, tok_err) = tokenize_lazy(source);
    let mut p = Parser::new(&toks);
    match p.file()? {
        Some(body) => {
            let height = 1 + stmts_height(&body);
            if height > AST_HEIGHT_LIMIT {
                return Err(ParseFailure::Recursion);
            }
            Ok(Module { body })
        }
        None => Err(p.failure(tok_err)),
    }
}

/// `ast.parse(source)`.
pub(crate) fn parse_module(source: &str) -> Result<Module, ParseFailure> {
    if source.contains('\0') {
        return Err(ParseFailure::Syntax(SyntaxError {
            class: "SyntaxError",
            msg: "source code string cannot contain null bytes".to_string(),
            lineno: None,
        }));
    }
    // Deeply nested sources recurse deeply; parse on a thread with a large stack.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("pyast-parser".to_string())
            .stack_size(512 << 20)
            .spawn_scoped(scope, || parse_inner(source))
            .expect("spawn parser thread")
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e))
    })
}

// ============================================================================== unparse

/// `ast._Precedence`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    NamedExpr = 1,
    Tuple,
    Yield,
    Test,
    Or,
    And,
    Not,
    Cmp,
    /// `EXPR` (also `BOR`).
    Expr,
    BXor,
    BAnd,
    Shift,
    Arith,
    Term,
    Factor,
    Power,
    Await,
    Atom,
}

impl Prec {
    fn next(self) -> Prec {
        use Prec::*;
        match self {
            NamedExpr => Tuple,
            Tuple => Yield,
            Yield => Test,
            Test => Or,
            Or => And,
            And => Not,
            Not => Cmp,
            Cmp => Expr,
            Expr => BXor,
            BXor => BAnd,
            BAnd => Shift,
            Shift => Arith,
            Arith => Term,
            Term => Factor,
            Factor => Power,
            Power => Await,
            Await | Atom => Atom,
        }
    }
}

const INFSTR: &str = "1e309";
const MULTI_QUOTES: &[&str] = &["\"\"\"", "'''"];
const ALL_QUOTES: &[&str] = &["'", "\"", "\"\"\"", "'''"];

/// `ast.unparse(expr)`; `Err` holds the `ValueError` message.
pub(crate) fn unparse(e: &Expr) -> Result<String, String> {
    let mut u = Unparser {
        out: String::new(),
        avoid_backslashes: false,
    };
    u.traverse(e, Prec::Test)?;
    Ok(u.out)
}

struct Unparser {
    out: String,
    avoid_backslashes: bool,
}

/// `c.encode("unicode_escape").decode("ascii")` for one character.
fn unicode_escape_char(c: char) -> String {
    let cp = c as u32;
    match c {
        '\\' => "\\\\".to_string(),
        '\t' => "\\t".to_string(),
        '\n' => "\\n".to_string(),
        '\r' => "\\r".to_string(),
        _ if cp < 0x20 || (0x7f..0x100).contains(&cp) => format!("\\x{cp:02x}"),
        _ if cp < 0x80 => c.to_string(),
        _ if cp < 0x10000 => format!("\\u{cp:04x}"),
        _ => format!("\\U{cp:08x}"),
    }
}

fn py_str_repr(s: &str) -> String {
    sem_utils::pycompat::fmt::str_repr(s)
}

fn bytes_repr(b: &[u8]) -> String {
    let quote = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from("b");
    out.push(quote);
    for &c in b {
        match c {
            _ if c == quote as u8 || c == b'\\' => {
                out.push('\\');
                out.push(c as char);
            }
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            _ if !(b' '..0x7f).contains(&c) => out.push_str(&format!("\\x{c:02x}")),
            _ => out.push(c as char),
        }
    }
    out.push(quote);
    out
}

fn float_text(f: f64) -> String {
    let r = if f.is_infinite() {
        if f > 0.0 { "inf".to_string() } else { "-inf".to_string() }
    } else if f.is_nan() {
        "nan".to_string()
    } else {
        sem_utils::pycompat::fmt::float_repr(f)
    };
    r.replace("inf", INFSTR)
        .replace("nan", &format!("({INFSTR}-{INFSTR})"))
}

fn imag_text(f: f64) -> String {
    let r = if f.is_infinite() {
        "inf".to_string()
    } else if f.is_nan() {
        "nan".to_string()
    } else {
        let r = sem_utils::pycompat::fmt::float_repr(f);
        match r.strip_suffix(".0") {
            Some(s) => s.to_string(),
            None => r,
        }
    };
    format!("{r}j")
        .replace("inf", INFSTR)
        .replace("nan", &format!("({INFSTR}-{INFSTR})"))
}

/// `_str_literal_helper`.
fn str_literal_helper(
    string: &str,
    quote_types: &[&'static str],
    escape_special_whitespace: bool,
) -> (String, Vec<&'static str>) {
    let mut escaped = String::with_capacity(string.len());
    for c in string.chars() {
        if !escape_special_whitespace && (c == '\n' || c == '\t') {
            escaped.push(c);
        } else if c == '\\' || !sem_utils::pycompat::fmt::is_printable(c) {
            escaped.push_str(&unicode_escape_char(c));
        } else {
            escaped.push(c);
        }
    }
    let mut possible: Vec<&'static str> = quote_types.to_vec();
    if escaped.contains('\n') {
        possible.retain(|q| MULTI_QUOTES.contains(q));
    }
    possible.retain(|q| !escaped.contains(q));
    if possible.is_empty() {
        let r = py_str_repr(string);
        let first = r.chars().next().unwrap_or('\'');
        let quote = quote_types
            .iter()
            .copied()
            .find(|q| q.contains(first))
            .unwrap_or(if first == '"' { "\"" } else { "'" });
        let inner = r[1..r.len() - 1].to_string();
        return (inner, vec![quote]);
    }
    if let Some(last) = escaped.chars().last() {
        // Stable sort: quotes not starting with the last character first.
        possible.sort_by_key(|q| q.starts_with(last));
        if possible[0].starts_with(last) {
            escaped.pop();
            escaped.push('\\');
            escaped.push(last);
        }
    }
    (escaped, possible)
}

impl Unparser {
    fn write(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn buffered<F>(&mut self, f: F) -> Result<String, String>
    where
        F: FnOnce(&mut Self) -> Result<(), String>,
    {
        let saved = std::mem::take(&mut self.out);
        let r = f(self);
        let buf = std::mem::replace(&mut self.out, saved);
        r.map(|_| buf)
    }

    fn items_view(&mut self, items: &[Expr]) -> Result<(), String> {
        if items.len() == 1 {
            self.traverse(&items[0], Prec::Test)?;
            self.write(",");
        } else {
            self.interleave(items, Prec::Test)?;
        }
        Ok(())
    }

    fn interleave(&mut self, items: &[Expr], prec: Prec) -> Result<(), String> {
        for (i, e) in items.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.traverse(e, prec)?;
        }
        Ok(())
    }

    fn write_str_avoiding_backslashes(&mut self, string: &str, quote_types: &[&'static str]) {
        let (s, q) = str_literal_helper(string, quote_types, false);
        let quote = q[0];
        self.write(quote);
        self.write(&s);
        self.write(quote);
    }

    fn write_constant(&mut self, c: &Const) -> Result<(), String> {
        match c {
            Const::Float(f) => self.write(&float_text(*f)),
            Const::Imag(f) => self.write(&imag_text(*f)),
            Const::Str(s) if self.avoid_backslashes => {
                self.write_str_avoiding_backslashes(s, ALL_QUOTES)
            }
            Const::Str(s) => self.write(&py_str_repr(s)),
            Const::Bytes(b) => self.write(&bytes_repr(b)),
            Const::Int { digits, radix } => {
                let d = int_to_decimal(digits, *radix);
                if d.len() > INT_MAX_STR_DIGITS {
                    return Err(format!(
                        "Exceeds the limit ({INT_MAX_STR_DIGITS} digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit"
                    ));
                }
                self.write(&d);
            }
            Const::None => self.write("None"),
            Const::True => self.write("True"),
            Const::False => self.write("False"),
            Const::Ellipsis => self.write("Ellipsis"),
        }
        Ok(())
    }

    fn comprehensions(&mut self, gens: &[Comprehension]) -> Result<(), String> {
        for g in gens {
            self.write(if g.is_async { " async for " } else { " for " });
            self.traverse(&g.target, Prec::Tuple)?;
            self.write(" in ");
            self.traverse(&g.iter, Prec::Test.next())?;
            for cond in &g.ifs {
                self.write(" if ");
                self.traverse(cond, Prec::Test.next())?;
            }
        }
        Ok(())
    }

    fn arguments(&mut self, a: &Arguments) -> Result<(), String> {
        let mut first = true;
        let all: Vec<&Arg> = a.posonlyargs.iter().chain(a.args.iter()).collect();
        let n_plain = all.len() - a.defaults.len().min(all.len());
        for (i, arg) in all.iter().enumerate() {
            if first {
                first = false;
            } else {
                self.write(", ");
            }
            self.arg(arg)?;
            if i >= n_plain {
                self.write("=");
                self.traverse(&a.defaults[i - n_plain], Prec::Test)?;
            }
            if i + 1 == a.posonlyargs.len() {
                self.write(", /");
            }
        }
        if a.vararg.is_some() || !a.kwonlyargs.is_empty() {
            if first {
                first = false;
            } else {
                self.write(", ");
            }
            self.write("*");
            if let Some(v) = &a.vararg {
                self.write(&v.name);
                if let Some(ann) = &v.annotation {
                    self.write(": ");
                    self.traverse(ann, Prec::Test)?;
                }
            }
        }
        for (arg, d) in a.kwonlyargs.iter().zip(&a.kw_defaults) {
            self.write(", ");
            self.arg(arg)?;
            if let Some(d) = d {
                self.write("=");
                self.traverse(d, Prec::Test)?;
            }
        }
        if let Some(k) = &a.kwarg {
            if !first {
                self.write(", ");
            }
            self.write("**");
            self.write(&k.name);
            if let Some(ann) = &k.annotation {
                self.write(": ");
                self.traverse(ann, Prec::Test)?;
            }
        }
        Ok(())
    }

    fn arg(&mut self, a: &Arg) -> Result<(), String> {
        self.write(&a.name);
        if let Some(ann) = &a.annotation {
            self.write(": ");
            self.traverse(ann, Prec::Test)?;
        }
        Ok(())
    }

    fn fstring_inner(&mut self, e: &Expr) -> Result<(), String> {
        match e {
            Expr::JoinedStr(values) => {
                for v in values {
                    self.fstring_inner(v)?;
                }
                Ok(())
            }
            Expr::Constant(Const::Str(s), _) => {
                self.write(&s.replace('{', "{{").replace('}', "}}"));
                Ok(())
            }
            Expr::FormattedValue(v, conversion, spec) => {
                self.formatted_value(v, *conversion, spec.as_deref())
            }
            other => Err(format!("Unexpected node inside JoinedStr, {other:?}")),
        }
    }

    fn formatted_value(&mut self, value: &Expr, conversion: i32, spec: Option<&Expr>) -> Result<(), String> {
        self.write("{");
        let mut inner = Unparser {
            out: String::new(),
            avoid_backslashes: true,
        };
        inner.traverse(value, Prec::Test.next())?;
        let expr = inner.out;
        if expr.contains('\\') {
            return Err("Unable to avoid backslash in f-string expression part".to_string());
        }
        if expr.starts_with('{') {
            self.write(" ");
        }
        self.write(&expr);
        if conversion != -1 {
            self.write("!");
            self.out.push(char::from_u32(conversion as u32).unwrap_or('r'));
        }
        if let Some(spec) = spec {
            self.write(":");
            self.fstring_inner(spec)?;
        }
        self.write("}");
        Ok(())
    }

    fn joined_str(&mut self, values: &[Expr]) -> Result<(), String> {
        self.write("f");
        if self.avoid_backslashes {
            let buffer = self.buffered(|u| {
                for v in values {
                    u.fstring_inner(v)?;
                }
                Ok(())
            })?;
            self.write_str_avoiding_backslashes(&buffer, ALL_QUOTES);
            return Ok(());
        }
        let mut parts = Vec::new();
        for v in values {
            let text = self.buffered(|u| u.fstring_inner(v))?;
            parts.push((text, matches!(v, Expr::Constant(..))));
        }
        let mut new_parts = Vec::new();
        let mut quote_types: Vec<&'static str> = ALL_QUOTES.to_vec();
        let mut fallback = false;
        for (value, is_constant) in &parts {
            let (v, new_q) = str_literal_helper(value, &quote_types, *is_constant);
            new_parts.push(v);
            if new_q.iter().all(|q| !quote_types.contains(q)) {
                fallback = true;
                break;
            }
            quote_types = new_q;
        }
        if fallback {
            quote_types = vec!["'''"];
            new_parts.clear();
            for (value, _) in &parts {
                let r = py_str_repr(&format!("\"{value}"));
                new_parts.push(r[2..r.len() - 1].to_string());
            }
        }
        let quote = quote_types[0];
        self.write(quote);
        self.write(&new_parts.concat());
        self.write(quote);
        Ok(())
    }

    fn traverse(&mut self, e: &Expr, prec: Prec) -> Result<(), String> {
        match e {
            Expr::BoolOp(kind, values) => {
                let (word, base) = match kind {
                    BoolOpKind::And => (" and ", Prec::And),
                    BoolOpKind::Or => (" or ", Prec::Or),
                };
                let parens = prec > base;
                if parens {
                    self.write("(");
                }
                let mut level = base;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        self.write(word);
                    }
                    level = level.next();
                    self.traverse(v, level)?;
                }
                if parens {
                    self.write(")");
                }
            }
            Expr::NamedExpr(target, value) => {
                let parens = prec > Prec::NamedExpr;
                if parens {
                    self.write("(");
                }
                self.traverse(target, Prec::Atom)?;
                self.write(" := ");
                self.traverse(value, Prec::Atom)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::BinOp(left, op, right) => {
                let (sym, op_prec) = match op {
                    BinOpKind::Add => ("+", Prec::Arith),
                    BinOpKind::Sub => ("-", Prec::Arith),
                    BinOpKind::Mult => ("*", Prec::Term),
                    BinOpKind::MatMult => ("@", Prec::Term),
                    BinOpKind::Div => ("/", Prec::Term),
                    BinOpKind::Mod => ("%", Prec::Term),
                    BinOpKind::FloorDiv => ("//", Prec::Term),
                    BinOpKind::LShift => ("<<", Prec::Shift),
                    BinOpKind::RShift => (">>", Prec::Shift),
                    BinOpKind::BitOr => ("|", Prec::Expr),
                    BinOpKind::BitXor => ("^", Prec::BXor),
                    BinOpKind::BitAnd => ("&", Prec::BAnd),
                    BinOpKind::Pow => ("**", Prec::Power),
                };
                let parens = prec > op_prec;
                if parens {
                    self.write("(");
                }
                let (lp, rp) = if *op == BinOpKind::Pow {
                    (op_prec.next(), op_prec)
                } else {
                    (op_prec, op_prec.next())
                };
                self.traverse(left, lp)?;
                self.write(&format!(" {sym} "));
                self.traverse(right, rp)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::UnaryOp(op, operand) => {
                let (sym, op_prec) = match op {
                    UnaryOpKind::Invert => ("~", Prec::Factor),
                    UnaryOpKind::Not => ("not", Prec::Not),
                    UnaryOpKind::UAdd => ("+", Prec::Factor),
                    UnaryOpKind::USub => ("-", Prec::Factor),
                };
                let parens = prec > op_prec;
                if parens {
                    self.write("(");
                }
                self.write(sym);
                if op_prec != Prec::Factor {
                    self.write(" ");
                }
                self.traverse(operand, op_prec)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::Lambda(args, body) => {
                let parens = prec > Prec::Test;
                if parens {
                    self.write("(");
                }
                self.write("lambda");
                let buffer = self.buffered(|u| u.arguments(args))?;
                if !buffer.is_empty() {
                    self.write(" ");
                    self.write(&buffer);
                }
                self.write(": ");
                self.traverse(body, Prec::Test)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::IfExp(test, body, orelse) => {
                let parens = prec > Prec::Test;
                if parens {
                    self.write("(");
                }
                self.traverse(body, Prec::Test.next())?;
                self.write(" if ");
                self.traverse(test, Prec::Test.next())?;
                self.write(" else ");
                self.traverse(orelse, Prec::Test)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::Dict(keys, values) => {
                self.write("{");
                for (i, (k, v)) in keys.iter().zip(values).enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    match k {
                        None => {
                            self.write("**");
                            self.traverse(v, Prec::Expr)?;
                        }
                        Some(k) => {
                            self.traverse(k, Prec::Test)?;
                            self.write(": ");
                            self.traverse(v, Prec::Test)?;
                        }
                    }
                }
                self.write("}");
            }
            Expr::Set(elts) => {
                if elts.is_empty() {
                    self.write("{*()}");
                } else {
                    self.write("{");
                    self.interleave(elts, Prec::Test)?;
                    self.write("}");
                }
            }
            Expr::ListComp(elt, gens) => {
                self.write("[");
                self.traverse(elt, Prec::Test)?;
                self.comprehensions(gens)?;
                self.write("]");
            }
            Expr::GeneratorExp(elt, gens) => {
                self.write("(");
                self.traverse(elt, Prec::Test)?;
                self.comprehensions(gens)?;
                self.write(")");
            }
            Expr::SetComp(elt, gens) => {
                self.write("{");
                self.traverse(elt, Prec::Test)?;
                self.comprehensions(gens)?;
                self.write("}");
            }
            Expr::DictComp(k, v, gens) => {
                self.write("{");
                self.traverse(k, Prec::Test)?;
                self.write(": ");
                self.traverse(v, Prec::Test)?;
                self.comprehensions(gens)?;
                self.write("}");
            }
            Expr::Await(v) => {
                let parens = prec > Prec::Await;
                if parens {
                    self.write("(");
                }
                self.write("await ");
                self.traverse(v, Prec::Atom)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::Yield(v) => {
                let parens = prec > Prec::Yield;
                if parens {
                    self.write("(");
                }
                self.write("yield");
                if let Some(v) = v {
                    self.write(" ");
                    self.traverse(v, Prec::Atom)?;
                }
                if parens {
                    self.write(")");
                }
            }
            Expr::YieldFrom(v) => {
                let parens = prec > Prec::Yield;
                if parens {
                    self.write("(");
                }
                self.write("yield from ");
                self.traverse(v, Prec::Atom)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::Compare(left, ops, comparators) => {
                let parens = prec > Prec::Cmp;
                if parens {
                    self.write("(");
                }
                self.traverse(left, Prec::Cmp.next())?;
                for (op, c) in ops.iter().zip(comparators) {
                    let sym = match op {
                        CmpOpKind::Eq => "==",
                        CmpOpKind::NotEq => "!=",
                        CmpOpKind::Lt => "<",
                        CmpOpKind::LtE => "<=",
                        CmpOpKind::Gt => ">",
                        CmpOpKind::GtE => ">=",
                        CmpOpKind::Is => "is",
                        CmpOpKind::IsNot => "is not",
                        CmpOpKind::In => "in",
                        CmpOpKind::NotIn => "not in",
                    };
                    self.write(&format!(" {sym} "));
                    self.traverse(c, Prec::Cmp.next())?;
                }
                if parens {
                    self.write(")");
                }
            }
            Expr::Call(func, args, keywords) => {
                self.traverse(func, Prec::Atom)?;
                self.write("(");
                let mut comma = false;
                for a in args {
                    if comma {
                        self.write(", ");
                    }
                    comma = true;
                    self.traverse(a, Prec::Test)?;
                }
                for k in keywords {
                    if comma {
                        self.write(", ");
                    }
                    comma = true;
                    match &k.arg {
                        None => self.write("**"),
                        Some(name) => {
                            self.write(name);
                            self.write("=");
                        }
                    }
                    self.traverse(&k.value, Prec::Test)?;
                }
                self.write(")");
            }
            Expr::FormattedValue(v, conversion, spec) => {
                self.formatted_value(v, *conversion, spec.as_deref())?;
            }
            Expr::JoinedStr(values) => self.joined_str(values)?,
            Expr::Constant(c, kind_u) => {
                if *c == Const::Ellipsis {
                    self.write("...");
                } else {
                    if *kind_u {
                        self.write("u");
                    }
                    self.write_constant(c)?;
                }
            }
            Expr::Attribute(value, attr) => {
                self.traverse(value, Prec::Atom)?;
                if matches!(
                    **value,
                    Expr::Constant(Const::Int { .. } | Const::True | Const::False, _)
                ) {
                    self.write(" ");
                }
                self.write(".");
                self.write(attr);
            }
            Expr::Subscript(value, slice) => {
                self.traverse(value, Prec::Atom)?;
                self.write("[");
                match &**slice {
                    Expr::Tuple(elts) if !elts.is_empty() => self.items_view(elts)?,
                    other => self.traverse(other, Prec::Test)?,
                }
                self.write("]");
            }
            Expr::Starred(v) => {
                self.write("*");
                self.traverse(v, Prec::Expr)?;
            }
            Expr::Name(id) => self.write(id),
            Expr::List(elts) => {
                self.write("[");
                self.interleave(elts, Prec::Test)?;
                self.write("]");
            }
            Expr::Tuple(elts) => {
                let parens = elts.is_empty() || prec > Prec::Tuple;
                if parens {
                    self.write("(");
                }
                self.items_view(elts)?;
                if parens {
                    self.write(")");
                }
            }
            Expr::Slice(lower, upper, step) => {
                if let Some(l) = lower {
                    self.traverse(l, Prec::Test)?;
                }
                self.write(":");
                if let Some(u) = upper {
                    self.traverse(u, Prec::Test)?;
                }
                if let Some(s) = step {
                    self.write(":");
                    self.traverse(s, Prec::Test)?;
                }
            }
        }
        Ok(())
    }
}
