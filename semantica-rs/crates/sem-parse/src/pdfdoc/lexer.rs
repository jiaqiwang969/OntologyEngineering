//! A PostScript-style tokenizer and stack parser for content streams and CMaps, following
//! pdfminer's `PSBaseParser`/`PSStackParser`/`PDFContentParser` (numbers, literal and hex
//! strings, names with `#xx` escapes, arrays, dictionaries, keywords, inline images).

use super::num::Num;

/// A parsed operand or operator.
#[derive(Debug, Clone, PartialEq)]
pub enum Obj {
    /// An integer or real number.
    Num(Num),
    /// `true`/`false`.
    Bool(bool),
    /// A `/Name` (escapes decoded).
    Name(Vec<u8>),
    /// A literal or hex string.
    Str(Vec<u8>),
    /// `[ ... ]`.
    Array(Vec<Obj>),
    /// `<< ... >>` (key order kept).
    Dict(Vec<(Vec<u8>, Obj)>),
    /// An inline image (`BI ... ID data EI`): its dictionary and raw data.
    InlineImage(Vec<(Vec<u8>, Obj)>, Vec<u8>),
    /// An operator / keyword.
    Kw(Vec<u8>),
}

impl Obj {
    /// The number, if this is one.
    pub fn num(&self) -> Option<Num> {
        match self {
            Obj::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// The name, if this is one.
    pub fn name(&self) -> Option<&[u8]> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }

    /// Dictionary lookup.
    pub fn dict_get(&self, key: &[u8]) -> Option<&Obj> {
        match self {
            Obj::Dict(items) | Obj::InlineImage(items, _) => {
                items.iter().find(|(k, _)| k == key).map(|(_, v)| v)
            }
            _ => None,
        }
    }
}

fn is_white(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\0')
}

fn is_delim(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// One lexical token.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(Num),
    Bool(bool),
    Name(Vec<u8>),
    Str(Vec<u8>),
    Kw(Vec<u8>),
    ArrayStart,
    ArrayEnd,
    DictStart,
    DictEnd,
}

/// Tokenizer over a byte buffer.
pub struct Lexer<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Lexer { buf, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.buf.get(self.pos).copied()
    }

    fn next_token(&mut self) -> Option<Tok> {
        loop {
            let c = self.peek()?;
            if is_white(c) {
                self.pos += 1;
                continue;
            }
            if c == b'%' {
                while let Some(c) = self.peek() {
                    if c == b'\r' || c == b'\n' {
                        break;
                    }
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
        let c = self.peek()?;
        match c {
            b'/' => {
                self.pos += 1;
                let mut name = Vec::new();
                while let Some(c) = self.peek() {
                    if is_white(c) || is_delim(c) {
                        break;
                    }
                    if c == b'#' {
                        let h = self.buf.get(self.pos + 1..self.pos + 3);
                        if let Some(h) = h
                            && h.iter().all(|b| b.is_ascii_hexdigit())
                        {
                            let v = u8::from_str_radix(std::str::from_utf8(h).unwrap_or("0"), 16)
                                .unwrap_or(0);
                            name.push(v);
                            self.pos += 3;
                            continue;
                        }
                    }
                    name.push(c);
                    self.pos += 1;
                }
                Some(Tok::Name(name))
            }
            b'(' => {
                self.pos += 1;
                Some(Tok::Str(self.literal_string()))
            }
            b'<' => {
                if self.buf.get(self.pos + 1) == Some(&b'<') {
                    self.pos += 2;
                    return Some(Tok::DictStart);
                }
                self.pos += 1;
                Some(Tok::Str(self.hex_string()))
            }
            b'>' => {
                if self.buf.get(self.pos + 1) == Some(&b'>') {
                    self.pos += 2;
                    return Some(Tok::DictEnd);
                }
                self.pos += 1;
                Some(Tok::Kw(b">".to_vec()))
            }
            b'[' => {
                self.pos += 1;
                Some(Tok::ArrayStart)
            }
            b']' => {
                self.pos += 1;
                Some(Tok::ArrayEnd)
            }
            b'{' | b'}' => {
                self.pos += 1;
                Some(Tok::Kw(vec![c]))
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => Some(self.number()),
            _ => {
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if is_white(c) || is_delim(c) {
                        break;
                    }
                    self.pos += 1;
                }
                if self.pos == start {
                    // A lone delimiter we do not understand: skip it.
                    self.pos += 1;
                    return self.next_token();
                }
                let word = self.buf[start..self.pos].to_vec();
                match word.as_slice() {
                    b"true" => Some(Tok::Bool(true)),
                    b"false" => Some(Tok::Bool(false)),
                    _ => Some(Tok::Kw(word)),
                }
            }
        }
    }

    /// pdfminer's `_parse_number`/`_parse_float`: digits, with one `.` making a float; a
    /// token that does not convert is dropped.
    fn number(&mut self) -> Tok {
        let start = self.pos;
        let first = self.buf[self.pos];
        self.pos += 1;
        let mut is_float = first == b'.';
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_digit() => self.pos += 1,
                Some(b'.') if !is_float => {
                    is_float = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        let text = std::str::from_utf8(&self.buf[start..self.pos]).unwrap_or("");
        if is_float {
            match text.parse::<f64>() {
                Ok(f) => Tok::Num(Num::Float(f)),
                Err(_) => {
                    // "-." / "." / "+." do not convert: pdfminer drops them.
                    self.next_token().unwrap_or(Tok::Kw(Vec::new()))
                }
            }
        } else {
            match text.parse::<i64>() {
                Ok(i) => Tok::Num(Num::Int(i)),
                Err(_) => match text.parse::<f64>() {
                    Ok(f) if text.len() > 1 => Tok::Num(Num::Float(f)),
                    _ => self.next_token().unwrap_or(Tok::Kw(Vec::new())),
                },
            }
        }
    }

    fn literal_string(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut depth = 1;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return out;
                    }
                    out.push(c);
                }
                b'\\' => {
                    let Some(e) = self.peek() else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'(' | b')' | b'\\' => out.push(e),
                        b'0'..=b'7' => {
                            let mut v: u32 = (e - b'0') as u32;
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + (d - b'0') as u32;
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push((v & 0xff) as u8);
                        }
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        other => out.push(other),
                    }
                }
                _ => out.push(c),
            }
        }
        out
    }

    /// pdfminer's `_parse_hexstring`: whitespace ignored, pairs of hex digits become bytes
    /// and a trailing single digit becomes its own value.
    fn hex_string(&mut self) -> Vec<u8> {
        let mut digits = Vec::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_hexdigit() {
                digits.push(c);
                self.pos += 1;
            } else if is_white(c) {
                self.pos += 1;
            } else {
                if c == b'>' {
                    self.pos += 1;
                }
                break;
            }
        }
        digits
            .chunks(2)
            .map(|pair| {
                let text = std::str::from_utf8(pair).unwrap_or("0");
                u8::from_str_radix(text, 16).unwrap_or(0)
            })
            .collect()
    }

    /// Raw inline-image data after `ID`: up to `EI` followed by whitespace (pdfminer's
    /// `get_inline_data`), minus the separator before `EI`.
    fn inline_data(&mut self, target: &[u8]) -> Vec<u8> {
        // Skip the single whitespace after "ID".
        if let Some(c) = self.peek()
            && is_white(c)
        {
            self.pos += 1;
        }
        let start = self.pos;
        let mut i = start;
        while i < self.buf.len() {
            if self.buf[i..].starts_with(target) {
                let after = i + target.len();
                let ok = if target == b"EI" {
                    after >= self.buf.len() || is_white(self.buf[after])
                } else {
                    true
                };
                if ok {
                    let mut data = self.buf[start..i].to_vec();
                    // pdfminer strips one trailing end-of-line before the terminator.
                    if data.ends_with(b"\r\n") {
                        data.truncate(data.len() - 2);
                    } else if data.ends_with(b"\r") || data.ends_with(b"\n") {
                        data.truncate(data.len() - 1);
                    }
                    self.pos = after;
                    if target != b"EI" {
                        data.extend_from_slice(target);
                    }
                    return data;
                }
            }
            i += 1;
        }
        self.pos = self.buf.len();
        self.buf[start..].to_vec()
    }
}

/// Stack-building parser: yields operands and operators in order.
pub struct ContentParser<'a> {
    lexer: Lexer<'a>,
    /// An inline image's `EI` keyword pending after its image object.
    pending_ei: bool,
}

enum Frame {
    Array(Vec<Obj>),
    Dict(Vec<Obj>),
    Inline(Vec<Obj>),
}

impl<'a> ContentParser<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        ContentParser {
            lexer: Lexer::new(buf),
            pending_ei: false,
        }
    }

    fn build_dict(items: Vec<Obj>) -> Vec<(Vec<u8>, Obj)> {
        let mut out = Vec::new();
        let mut it = items.into_iter();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            let key = match k {
                Obj::Name(n) => n,
                Obj::Str(s) => s,
                Obj::Kw(k) => k,
                _ => continue,
            };
            out.push((key, v));
        }
        out
    }

    /// The next top-level object (operand) or keyword.
    pub fn next_object(&mut self) -> Option<Obj> {
        if self.pending_ei {
            self.pending_ei = false;
            return Some(Obj::Kw(b"EI".to_vec()));
        }
        let mut stack: Vec<Frame> = Vec::new();
        loop {
            let tok = match self.lexer.next_token() {
                Some(t) => t,
                None => {
                    // Unterminated containers are dropped at the end of input.
                    return None;
                }
            };
            let obj = match tok {
                Tok::Num(n) => Obj::Num(n),
                Tok::Bool(b) => Obj::Bool(b),
                Tok::Name(n) => Obj::Name(n),
                Tok::Str(s) => Obj::Str(s),
                Tok::ArrayStart => {
                    stack.push(Frame::Array(Vec::new()));
                    continue;
                }
                Tok::DictStart => {
                    stack.push(Frame::Dict(Vec::new()));
                    continue;
                }
                Tok::ArrayEnd => match stack.pop() {
                    Some(Frame::Array(items)) => Obj::Array(items),
                    Some(other) => {
                        // Mismatched close: pdfminer reports a type error and drops it.
                        stack.push(other);
                        continue;
                    }
                    None => continue,
                },
                Tok::DictEnd => match stack.pop() {
                    Some(Frame::Dict(items)) => Obj::Dict(Self::build_dict(items)),
                    Some(other) => {
                        stack.push(other);
                        continue;
                    }
                    None => continue,
                },
                Tok::Kw(k) => {
                    if k == b"BI" {
                        stack.push(Frame::Inline(Vec::new()));
                        continue;
                    }
                    if k == b"ID" {
                        if let Some(Frame::Inline(items)) = stack.pop() {
                            let dict = Self::build_dict(items);
                            let filter = dict
                                .iter()
                                .find(|(k, _)| k == b"F" || k == b"Filter")
                                .map(|(_, v)| v.clone());
                            let first_filter = match filter {
                                Some(Obj::Name(n)) => Some(n),
                                Some(Obj::Array(items)) => items.first().and_then(|o| o.name().map(<[u8]>::to_vec)),
                                _ => None,
                            };
                            let target: &[u8] = match first_filter.as_deref() {
                                Some(b"A85") | Some(b"ASCII85Decode") => b"~>",
                                _ => b"EI",
                            };
                            let data = self.lexer.inline_data(target);
                            if target == b"EI" {
                                self.pending_ei = true;
                            }
                            let image = Obj::InlineImage(dict, data);
                            if stack.is_empty() {
                                return Some(image);
                            }
                            // An inline image nested in a container: keep it there.
                            match stack.last_mut() {
                                Some(Frame::Array(v)) | Some(Frame::Dict(v)) | Some(Frame::Inline(v)) => v.push(image),
                                None => {}
                            }
                            continue;
                        }
                        Obj::Kw(k)
                    } else if stack.is_empty() {
                        return Some(Obj::Kw(k));
                    } else {
                        Obj::Kw(k)
                    }
                }
            };
            match stack.last_mut() {
                Some(Frame::Array(v)) | Some(Frame::Dict(v)) | Some(Frame::Inline(v)) => v.push(obj),
                None => return Some(obj),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(src: &[u8]) -> Vec<Obj> {
        let mut p = ContentParser::new(src);
        let mut out = Vec::new();
        while let Some(o) = p.next_object() {
            out.push(o);
        }
        out
    }

    #[test]
    fn tokens_follow_pdfminer() {
        let objs = all(b"BT /F1 12 Tf 72 712.5 Td (Hi \\(x\\)\\101) Tj <48 65 6> Tj [(a) -120 (b)] TJ ET");
        assert_eq!(objs[0], Obj::Kw(b"BT".to_vec()));
        assert_eq!(objs[1], Obj::Name(b"F1".to_vec()));
        assert_eq!(objs[2], Obj::Num(Num::Int(12)));
        assert_eq!(objs[5], Obj::Num(Num::Float(712.5)));
        assert_eq!(objs[7], Obj::Str(b"Hi (x)A".to_vec()));
        assert_eq!(objs[9], Obj::Str(vec![0x48, 0x65, 0x06]));
        assert_eq!(
            objs[11],
            Obj::Array(vec![
                Obj::Str(b"a".to_vec()),
                Obj::Num(Num::Int(-120)),
                Obj::Str(b"b".to_vec())
            ])
        );
    }

    #[test]
    fn inline_images_are_read_raw() {
        let objs = all(b"q BI /W 2 /H 1 /BPC 8 /CS /G ID \x01\x02 EI Q");
        assert_eq!(objs[0], Obj::Kw(b"q".to_vec()));
        match &objs[1] {
            Obj::InlineImage(d, data) => {
                assert_eq!(d.len(), 4);
                assert_eq!(data, &vec![1, 2, b' ']);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(objs[2], Obj::Kw(b"EI".to_vec()));
        assert_eq!(objs[3], Obj::Kw(b"Q".to_vec()));
    }
}
