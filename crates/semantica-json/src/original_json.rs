//! Explicit historical JSON import parser. Does not replace strict_json.
//! Original bytes decode to codepoints and then directly to NativeValue.
//! No Python delegate, lossy String bridge, or recursive container tree.
use crate::native_value::{NativeError, NativeLimits, NativeNode, NativeValue};
use crate::original_json_bytes::{DecodeError, DecodeLimits, decode_original_bytes};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub msg: &'static str,
    pub pos: usize,
    pub lineno: usize,
    pub colno: usize,
}
impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: line {} column {} (char {})",
            self.msg, self.lineno, self.colno, self.pos
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Decode(DecodeError),
    Syntax(SyntaxError),
    IntegerDigits(usize),
    Resource(NativeError),
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => e.fmt(f),
            Self::Syntax(e) => e.fmt(f),
            Self::Resource(e) => e.fmt(f),
            Self::IntegerDigits(n) => write!(
                f,
                "Exceeds the limit (4300 digits) for integer string conversion: value has {n} digits; use sys.set_int_max_str_digits() to increase the limit"
            ),
        }
    }
}
impl std::error::Error for ParseError {}
impl From<NativeError> for ParseError {
    fn from(e: NativeError) -> Self {
        Self::Resource(e)
    }
}
type Result<T> = std::result::Result<T, ParseError>;

/// The pinned CPython JSON ingress limit, also usable after a typed JSON
/// transport has retained the integer token. `digits` excludes a leading sign.
pub fn check_original_integer_digits(digits: usize) -> Result<()> {
    if digits > 4300 {
        Err(ParseError::IntegerDigits(digits))
    } else {
        Ok(())
    }
}

/// Limits come from the host. Native node/link/scalar limits also bound ALL
/// parsed workspace, including values subsequently overwritten by duplicate keys.
#[derive(Clone, Copy, Debug)]
pub struct ParseLimits {
    pub decode: DecodeLimits,
    pub native: NativeLimits,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    ArrayStart,
    ArrayValue,
    ArrayAfter,
    ObjectStart,
    ObjectKey,
    ObjectColon,
    ObjectValue,
    ObjectAfter,
}
struct Frame {
    stage: Stage,
    items: Vec<usize>,
    pairs: Vec<(usize, usize)>,
    keys: BTreeMap<Vec<u32>, usize>,
    key: Option<usize>,
    comma: usize,
}
impl Frame {
    fn new(object: bool) -> Self {
        Self {
            stage: if object {
                Stage::ObjectStart
            } else {
                Stage::ArrayStart
            },
            items: Vec::new(),
            pairs: Vec::new(),
            keys: BTreeMap::new(),
            key: None,
            comma: 0,
        }
    }
}
struct Parser {
    text: Vec<u32>,
    pos: usize,
    nodes: Vec<NativeNode>,
    frames: Vec<Frame>,
    limits: NativeLimits,
    scalars: usize,
    links: usize,
}
impl Parser {
    fn syntax(&self, msg: &'static str, pos: usize) -> ParseError {
        let prefix = &self.text[..pos.min(self.text.len())];
        ParseError::Syntax(SyntaxError {
            msg,
            pos,
            lineno: 1 + prefix.iter().filter(|&&c| c == 10).count(),
            colno: pos - prefix.iter().rposition(|&c| c == 10).map_or(0, |i| i + 1) + 1,
        })
    }
    fn peek(&self) -> Option<u32> {
        self.text.get(self.pos).copied()
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(9 | 10 | 13 | 32)) {
            self.pos += 1
        }
    }
    fn charge_scalar(&mut self, n: usize) -> Result<()> {
        self.scalars = self
            .scalars
            .checked_add(n)
            .filter(|n| *n <= self.limits.max_scalar_bytes)
            .ok_or(NativeError::ScalarLimit)?;
        Ok(())
    }
    fn charge_links(&mut self, n: usize) -> Result<()> {
        self.links = self
            .links
            .checked_add(n)
            .filter(|n| *n <= self.limits.max_links)
            .ok_or(NativeError::LinkLimit)?;
        Ok(())
    }
    fn node(&mut self, node: NativeNode) -> Result<usize> {
        if self.nodes.len() >= self.limits.max_nodes {
            return Err(NativeError::NodeLimit.into());
        }
        let id = self.nodes.len();
        self.nodes.push(node);
        Ok(id)
    }
    fn hex4(&self, at: usize) -> Option<u32> {
        if at.checked_add(4)? >= self.text.len() {
            return None;
        }
        let mut result = 0;
        for &cp in self.text.get(at..at.checked_add(4)?)? {
            let d = match cp {
                48..=57 => cp - 48,
                65..=70 => cp - 55,
                97..=102 => cp - 87,
                _ => return None,
            };
            result = (result << 4) | d;
        }
        Some(result)
    }
    fn string(&mut self) -> Result<usize> {
        let start = self.pos;
        self.pos += 1;
        let mut out = Vec::new();
        loop {
            let Some(mut cp) = self.peek() else {
                return Err(self.syntax("Unterminated string starting at", start));
            };
            if cp == 34 {
                self.pos += 1;
                return self.node(NativeNode::Text(out));
            }
            if cp < 32 {
                return Err(self.syntax("Invalid control character at", self.pos));
            }
            self.pos += 1;
            if cp == 92 {
                let slash = self.pos - 1;
                let Some(escape) = self.peek() else {
                    return Err(self.syntax("Unterminated string starting at", start));
                };
                self.pos += 1;
                cp = match escape {
                    34 | 47 | 92 => escape,
                    98 => 8,
                    102 => 12,
                    110 => 10,
                    114 => 13,
                    116 => 9,
                    117 => {
                        let Some(mut first) = self.hex4(self.pos) else {
                            return Err(self.syntax("Invalid \\uXXXX escape", self.pos - 1));
                        };
                        self.pos += 4;
                        if (0xd800..=0xdbff).contains(&first)
                            && self.text.get(self.pos..self.pos + 2) == Some(&[92, 117])
                        {
                            if let Some(second) = self.hex4(self.pos + 2) {
                                if (0xdc00..=0xdfff).contains(&second) {
                                    first = 0x10000 + ((first - 0xd800) << 10) + second - 0xdc00;
                                    self.pos += 6;
                                }
                            }
                        }
                        first
                    }
                    _ => return Err(self.syntax("Invalid \\escape", slash)),
                };
            }
            self.charge_scalar(4)?;
            out.push(cp);
        }
    }
    fn literal(&mut self, s: &str, node: NativeNode, scalar: usize) -> Result<Option<usize>> {
        if self
            .text
            .get(self.pos..self.pos + s.len())
            .is_some_and(|v| v.iter().copied().eq(s.bytes().map(u32::from)))
        {
            self.pos += s.len();
            self.charge_scalar(scalar)?;
            return Ok(Some(self.node(node)?));
        }
        Err(self.syntax("Expecting value", self.pos))
    }
    fn number(&mut self) -> Result<Option<usize>> {
        let start = self.pos;
        let mut end = start;
        if self.text.get(end) == Some(&45) {
            end += 1;
        }
        let digit_start = end;
        match self.text.get(end) {
            Some(48) => end += 1,
            Some(49..=57) => {
                end += 1;
                while matches!(self.text.get(end), Some(48..=57)) {
                    end += 1;
                }
            }
            _ => return Err(self.syntax("Expecting value", start)),
        }
        let digits = end - digit_start;
        let mut float = false;
        if self.text.get(end) == Some(&46) && matches!(self.text.get(end + 1), Some(48..=57)) {
            float = true;
            end += 2;
            while matches!(self.text.get(end), Some(48..=57)) {
                end += 1;
            }
        }
        if matches!(self.text.get(end), Some(69 | 101)) {
            let mut at = end + 1;
            if matches!(self.text.get(at), Some(43 | 45)) {
                at += 1;
            }
            if matches!(self.text.get(at), Some(48..=57)) {
                float = true;
                at += 1;
                while matches!(self.text.get(at), Some(48..=57)) {
                    at += 1;
                }
                end = at;
            }
        }
        if !float {
            check_original_integer_digits(digits)?;
        }
        // ASCII number token only. String parsing never passes through here.
        // Charge token storage before allocating the temporary number text.
        if end - start > self.limits.max_scalar_bytes.saturating_sub(self.scalars) {
            return Err(NativeError::ScalarLimit.into());
        }
        let token: String = self.text[start..end]
            .iter()
            .map(|&c| c as u8 as char)
            .collect();
        self.pos = end;
        let node = if float {
            self.charge_scalar(8)?;
            let value = token
                .parse::<f64>()
                .map_err(|_| self.syntax("Expecting value", start))?;
            NativeNode::FloatBits(value.to_bits())
        } else {
            let value = if token == "-0" { "0".to_owned() } else { token };
            self.charge_scalar(value.len())?;
            NativeNode::Integer(value)
        };
        Ok(Some(self.node(node)?))
    }
    fn value(&mut self) -> Result<Option<usize>> {
        self.ws();
        if !matches!(self.peek(), Some(91 | 123)) && self.frames.len() > self.limits.max_depth {
            return Err(NativeError::DepthLimit.into());
        }
        match self.peek() {
            Some(34) => Ok(Some(self.string()?)),
            Some(91 | 123) => {
                if self.frames.len() > self.limits.max_depth {
                    return Err(NativeError::DepthLimit.into());
                }
                if self.frames.len() >= self.limits.max_nodes.saturating_sub(self.nodes.len()) {
                    return Err(NativeError::NodeLimit.into());
                }
                let object = self.peek() == Some(123);
                self.pos += 1;
                self.frames.push(Frame::new(object));
                Ok(None)
            }
            Some(110) => self.literal("null", NativeNode::None, 0),
            Some(116) => self.literal("true", NativeNode::Bool(true), 1),
            Some(102) => self.literal("false", NativeNode::Bool(false), 1),
            Some(78) => self.literal("NaN", NativeNode::FloatBits(0x7ff8000000000000), 8),
            Some(73) => self.literal(
                "Infinity",
                NativeNode::FloatBits(f64::INFINITY.to_bits()),
                8,
            ),
            Some(45) if self.text.get(self.pos + 1) == Some(&73) => self.literal(
                "-Infinity",
                NativeNode::FloatBits(f64::NEG_INFINITY.to_bits()),
                8,
            ),
            Some(45 | 48..=57) => self.number(),
            _ => Err(self.syntax("Expecting value", self.pos)),
        }
    }
    fn run(mut self) -> Result<NativeValue> {
        let mut completed = self.value()?;
        loop {
            if let Some(id) = completed.take() {
                let Some(mut frame) = self.frames.pop() else {
                    self.ws();
                    if self.pos != self.text.len() {
                        return Err(self.syntax("Extra data", self.pos));
                    }
                    return self.finish(id);
                };
                match frame.stage {
                    Stage::ArrayValue => {
                        self.charge_links(1)?;
                        frame.items.push(id);
                        frame.stage = Stage::ArrayAfter;
                    }
                    Stage::ObjectValue => {
                        self.charge_links(2)?;
                        let key = frame.key.take().expect("key parsed before object value");
                        let NativeNode::Text(text) = &self.nodes[key] else {
                            unreachable!("JSON key is text")
                        };
                        if let Some(&index) = frame.keys.get(text) {
                            frame.pairs[index].1 = id;
                        } else {
                            frame.keys.insert(text.clone(), frame.pairs.len());
                            frame.pairs.push((key, id));
                        }
                        frame.stage = Stage::ObjectAfter;
                    }
                    _ => unreachable!("completed value has awaiting frame"),
                }
                self.frames.push(frame);
            }
            self.ws();
            let mut frame = self
                .frames
                .pop()
                .expect("root value completed or container open");
            match frame.stage {
                Stage::ArrayStart | Stage::ArrayValue => {
                    if self.peek() == Some(93) {
                        if frame.stage == Stage::ArrayValue {
                            return Err(self.syntax(
                                "Illegal trailing comma before end of array",
                                frame.comma,
                            ));
                        }
                        self.pos += 1;
                        completed = Some(self.node(NativeNode::List(frame.items))?);
                    } else {
                        frame.stage = Stage::ArrayValue;
                        self.frames.push(frame);
                        completed = self.value()?;
                    }
                }
                Stage::ArrayAfter => match self.peek() {
                    Some(93) => {
                        self.pos += 1;
                        completed = Some(self.node(NativeNode::List(frame.items))?);
                    }
                    Some(44) => {
                        frame.comma = self.pos;
                        self.pos += 1;
                        frame.stage = Stage::ArrayValue;
                        self.frames.push(frame);
                    }
                    _ => return Err(self.syntax("Expecting ',' delimiter", self.pos)),
                },
                Stage::ObjectStart | Stage::ObjectKey => {
                    if self.peek() == Some(125) {
                        if frame.stage == Stage::ObjectKey {
                            return Err(self.syntax(
                                "Illegal trailing comma before end of object",
                                frame.comma,
                            ));
                        }
                        self.pos += 1;
                        completed = Some(self.node(NativeNode::Dict(frame.pairs))?);
                    } else {
                        if self.peek() != Some(34) {
                            return Err(self.syntax(
                                "Expecting property name enclosed in double quotes",
                                self.pos,
                            ));
                        }
                        frame.key = Some(self.string()?);
                        frame.stage = Stage::ObjectColon;
                        self.frames.push(frame);
                    }
                }
                Stage::ObjectColon => {
                    if self.peek() != Some(58) {
                        return Err(self.syntax("Expecting ':' delimiter", self.pos));
                    }
                    self.pos += 1;
                    frame.stage = Stage::ObjectValue;
                    self.frames.push(frame);
                    completed = self.value()?;
                }
                Stage::ObjectAfter => match self.peek() {
                    Some(125) => {
                        self.pos += 1;
                        completed = Some(self.node(NativeNode::Dict(frame.pairs))?);
                    }
                    Some(44) => {
                        frame.comma = self.pos;
                        self.pos += 1;
                        frame.stage = Stage::ObjectKey;
                        self.frames.push(frame);
                    }
                    _ => return Err(self.syntax("Expecting ',' delimiter", self.pos)),
                },
                Stage::ObjectValue => unreachable!("object value is consumed immediately"),
            }
        }
    }
    fn finish(self, root: usize) -> Result<NativeValue> {
        // Rebuild final postorder from retained tree, removing shadowed values
        // and preserving first-key position. All structures remain flat on drop.
        let mut source: Vec<Option<NativeNode>> = self.nodes.into_iter().map(Some).collect();
        let mut mapping = vec![0; source.len()];
        let mut todo = vec![(root, false)];
        let mut nodes = Vec::new();
        while let Some((id, close)) = todo.pop() {
            if !close {
                todo.push((id, true));
                match source[id].as_ref().expect("JSON tree node") {
                    NativeNode::List(items) => {
                        todo.extend(items.iter().rev().map(|&id| (id, false)))
                    }
                    NativeNode::Dict(pairs) => {
                        for &(key, value) in pairs.iter().rev() {
                            todo.push((value, false));
                            todo.push((key, false));
                        }
                    }
                    _ => {}
                }
            } else {
                let mut node = source[id].take().expect("JSON tree has no aliases");
                match &mut node {
                    NativeNode::List(items) => {
                        for id in items {
                            *id = mapping[*id]
                        }
                    }
                    NativeNode::Dict(pairs) => {
                        for (key, value) in pairs {
                            *key = mapping[*key];
                            *value = mapping[*value];
                        }
                    }
                    _ => {}
                }
                mapping[id] = nodes.len();
                nodes.push(node);
            }
        }
        NativeValue::new(nodes, mapping[root], self.limits).map_err(Into::into)
    }
}

/// CPython 3.13.11 import profile (default decimal integer limit 4300).
/// Success is parsed data only, not graph admission, source authority or adoption.
pub fn parse_original_json(bytes: &[u8], limits: ParseLimits) -> Result<NativeValue> {
    let decoded = decode_original_bytes(bytes, limits.decode).map_err(ParseError::Decode)?;
    Parser {
        text: decoded.codepoints,
        pos: 0,
        nodes: Vec::new(),
        frames: Vec::new(),
        limits: limits.native,
        scalars: 0,
        links: 0,
    }
    .run()
}
