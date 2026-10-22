//! Original Semantica version-storage checksum and ASCII JSON codec.
//! Shared implementation for snapshot checksums and later persistent state owners.
//! Compose retained native values, exact numeric comparisons and CPython sorting.
use semantica_json::{
    native_operations,
    native_value::{NativeNode as N, NativeValue},
};
use semantica_numeric::{BigInt, DecimalContext, NumericLimits, NumericRelation, NumericValue};
use sha2::{Digest, Sha256};
use spareval::{SourceSortError, source_sort_indices};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Source {
        class: &'static str,
        message: String,
    },
    Refused(&'static str),
}
type Result<T> = std::result::Result<T, Error>;
fn source(class: &'static str, message: impl Into<String>) -> Error {
    Error::Source {
        class,
        message: message.into(),
    }
}
fn name(n: &N) -> &'static str {
    match n {
        N::None => "NoneType",
        N::Bool(_) => "bool",
        N::Integer(_) => "int",
        N::FloatBits(_) => "float",
        N::Text(_) => "str",
        N::List(_) => "list",
        N::Tuple(_) => "tuple",
        N::Set(_) => "set",
        N::FrozenSet(_) => "frozenset",
        N::Dict(_) => "dict",
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub visits: usize,
    pub output_bytes: usize,
    pub depth: usize,
    pub integer_digits: usize,
    pub duration: Duration,
}
pub struct Encoder<'a> {
    value: Option<&'a NativeValue>,
    nodes: &'a [N],
    root: usize,
    active: RefCell<BTreeSet<usize>>,
    limits: Limits,
    visits: Cell<usize>,
    deadline: Instant,
    numeric: NumericLimits,
    storage_json: bool,
}
impl<'a> Encoder<'a> {
    pub fn new(value: &'a NativeValue, limits: Limits) -> Self {
        Self {
            value: Some(value),
            nodes: value.nodes(),
            root: value.root(),
            active: RefCell::new(BTreeSet::new()),
            limits,
            storage_json: false,
            visits: Cell::new(0),
            deadline: Instant::now() + limits.duration,
            numeric: NumericLimits {
                max_input_characters: limits.integer_digits,
                max_coefficient_digits: limits.integer_digits,
                max_output_characters: limits.integer_digits,
                max_materialized_power_of_ten: limits.integer_digits,
            },
        }
    }
    pub fn from_arena(nodes: &'a [N], root: usize, limits: Limits) -> Self {
        Self {
            value: None,
            nodes,
            root,
            active: RefCell::new(BTreeSet::new()),
            limits,
            storage_json: false,
            visits: Cell::new(0),
            deadline: Instant::now() + limits.duration,
            numeric: NumericLimits {
                max_input_characters: limits.integer_digits,
                max_coefficient_digits: limits.integer_digits,
                max_output_characters: limits.integer_digits,
                max_materialized_power_of_ten: limits.integer_digits,
            },
        }
    }
    fn tick(&self, depth: usize) -> Result<()> {
        if depth > self.limits.depth {
            return Err(Error::Refused("depth"));
        }
        let next = self
            .visits
            .get()
            .checked_add(1)
            .ok_or(Error::Refused("visits"))?;
        if next > self.limits.visits {
            return Err(Error::Refused("visits"));
        }
        self.visits.set(next);
        if Instant::now() > self.deadline {
            return Err(Error::Refused("time"));
        }
        Ok(())
    }
    fn node(&self, id: usize) -> Result<&N> {
        self.nodes.get(id).ok_or(Error::Refused("reference"))
    }
    fn numeric(&self, id: usize) -> Result<Option<NumericValue>> {
        Ok(match self.node(id)? {
            N::Bool(b) => Some(NumericValue::Integer(BigInt::from(u8::from(*b)))),
            N::Integer(s) => {
                if s.trim_start_matches('-').len() > self.limits.integer_digits {
                    return Err(Error::Refused("numeric_digits"));
                }
                Some(NumericValue::Integer(
                    s.parse().map_err(|_| Error::Refused("integer"))?,
                ))
            }
            N::FloatBits(bits) => Some(NumericValue::Binary64(f64::from_bits(*bits))),
            _ => None,
        })
    }
    fn numeric_compare(
        &self,
        left: usize,
        right: usize,
        op: NumericRelation,
    ) -> Result<Option<bool>> {
        match (self.numeric(left)?, self.numeric(right)?) {
            (Some(a), Some(b)) => DecimalContext::default()
                .compare_numeric(op, &a, &b, &self.numeric)
                .result
                .map(Some)
                .map_err(|_| Error::Refused("numeric_comparison")),
            _ => Ok(None),
        }
    }
    // Native node identity is explicitly the reconstruction identity for this
    // reconstruction profile; distinct NaN nodes remain distinct, shared nodes match.
    fn equal(&self, a: usize, b: usize, depth: usize) -> Result<bool> {
        self.tick(depth)?;
        if a == b {
            return Ok(true);
        }
        self.rich_equal(a, b, depth)
    }
    /// Direct == keeps NaN != NaN; container members use the source identity
    /// short-circuit above. Dictionary key lookup also uses that short-circuit.
    pub fn rich_equal(&self, a: usize, b: usize, depth: usize) -> Result<bool> {
        self.tick(depth)?;
        if let Some(v) = self.numeric_compare(a, b, NumericRelation::Equal)? {
            return Ok(v);
        }
        match (self.node(a)?, self.node(b)?) {
            (N::None, N::None) => Ok(true),
            (N::Text(x), N::Text(y)) => Ok(x == y),
            (N::Tuple(x), N::Tuple(y)) | (N::List(x), N::List(y)) => {
                if x.len() != y.len() {
                    return Ok(false);
                }
                for (&x, &y) in x.iter().zip(y) {
                    if !self.equal(x, y, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (N::FrozenSet(x) | N::Set(x), N::FrozenSet(y) | N::Set(y)) => {
                Ok(self.subset(x, y, depth + 1)? && self.subset(y, x, depth + 1)?)
            }
            (N::Dict(x), N::Dict(y)) => {
                let x = self.pairs(x, depth + 1)?;
                let y = self.pairs(y, depth + 1)?;
                if x.len() != y.len() {
                    return Ok(false);
                }
                for (k, v) in x {
                    let mut found = None;
                    for &(q, w) in &y {
                        if self.equal(k, q, depth + 1)? {
                            found = Some(w);
                            break;
                        }
                    }
                    let Some(w) = found else {
                        return Ok(false);
                    };
                    if !self.equal(v, w, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub fn key_equal(&self, a: usize, b: usize) -> Result<bool> {
        self.equal(a, b, 0)
    }
    fn subset(&self, x: &[usize], y: &[usize], depth: usize) -> Result<bool> {
        for &a in x {
            self.tick(depth)?;
            let mut found = false;
            for &b in y {
                if self.equal(a, b, depth + 1)? {
                    found = true;
                    break;
                }
            }
            if !found {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn less(&self, a: usize, b: usize, depth: usize) -> Result<bool> {
        self.tick(depth)?;
        if let Some(v) = self.numeric_compare(a, b, NumericRelation::Less)? {
            return Ok(v);
        }
        match (self.node(a)?, self.node(b)?) {
            (N::Text(x), N::Text(y)) => Ok(x < y),
            (N::Tuple(x), N::Tuple(y)) => {
                for (&a, &b) in x.iter().zip(y) {
                    if !self.equal(a, b, depth + 1)? {
                        return self.less(a, b, depth + 1);
                    }
                }
                Ok(x.len() < y.len())
            }
            (N::FrozenSet(x), N::FrozenSet(y)) => {
                Ok(self.subset(x, y, depth + 1)? && !self.subset(y, x, depth + 1)?)
            }
            (x, y) => Err(source(
                "builtins.TypeError",
                format!(
                    "'<' not supported between instances of '{}' and '{}'",
                    name(x),
                    name(y)
                ),
            )),
        }
    }
    fn pairs(&self, entries: &[(usize, usize)], depth: usize) -> Result<Vec<(usize, usize)>> {
        let mut result: Vec<(usize, usize)> = Vec::new();
        for &(key, value) in entries {
            self.tick(depth)?;
            let mut found = None;
            for (i, &(old, _)) in result.iter().enumerate() {
                if self.equal(old, key, depth + 1)? {
                    found = Some(i);
                    break;
                }
            }
            match found {
                Some(i) => result[i].1 = value,
                None => result.push((key, value)),
            }
        }
        Ok(result)
    }
    fn put(&self, output: &mut String, s: &str) -> Result<()> {
        if s.len() > self.limits.output_bytes.saturating_sub(output.len()) {
            return Err(Error::Refused("output_bytes"));
        }
        output.push_str(s);
        Ok(())
    }
    fn quote(&self, output: &mut String, points: &[u32]) -> Result<()> {
        self.put(output, "\"")?;
        for &cp in points {
            self.tick(0)?;
            match cp {
                34 => self.put(output, "\\\"")?,
                92 => self.put(output, "\\\\")?,
                8 => self.put(output, "\\b")?,
                9 => self.put(output, "\\t")?,
                10 => self.put(output, "\\n")?,
                12 => self.put(output, "\\f")?,
                13 => self.put(output, "\\r")?,
                32..=126 => self.put(output, &char::from_u32(cp).unwrap().to_string())?,
                0..=0xffff => self.put(output, &format!("\\u{cp:04x}"))?,
                0x10000..=0x10ffff => {
                    let n = cp - 0x10000;
                    self.put(
                        output,
                        &format!(
                            "\\u{:04x}\\u{:04x}",
                            0xd800 + (n >> 10),
                            0xdc00 + (n & 1023)
                        ),
                    )?;
                }
                _ => return Err(Error::Refused("codepoint")),
            }
        }
        self.put(output, "\"")
    }
    fn float(&self, bits: u64) -> Result<String> {
        let f = f64::from_bits(bits);
        if f.is_nan() {
            Ok("NaN".into())
        } else if f.is_infinite() {
            Ok(if f.is_sign_negative() {
                "-Infinity"
            } else {
                "Infinity"
            }
            .into())
        } else {
            semantica_json::python_float_repr(f).map_err(|_| Error::Refused("float_format"))
        }
    }
    fn integer_text<'b>(&self, s: &'b str) -> Result<&'b str> {
        // Pinned original CPython's decimal text limit, distinct from the local
        // admission limit for retained integer data and numeric comparison.
        if s.trim_start_matches('-').len() > 4300 {
            Err(source(
                "builtins.ValueError",
                "Exceeds the limit (4300 digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit",
            ))
        } else {
            Ok(s)
        }
    }
    fn key(&self, output: &mut String, id: usize) -> Result<()> {
        self.tick(0)?;
        let text = match self.node(id)? {
            N::Text(s) => return self.quote(output, s),
            N::None => "null".into(),
            N::Bool(b) => if *b { "true" } else { "false" }.into(),
            N::Integer(s) => self.integer_text(s)?.into(),
            N::FloatBits(bits) => self.float(*bits)?,
            n => {
                return Err(source(
                    "builtins.TypeError",
                    format!(
                        "keys must be str, int, float, bool or None, not {}",
                        name(n)
                    ),
                ));
            }
        };
        self.quote(output, &text.chars().map(u32::from).collect::<Vec<_>>())
    }
    fn dict(
        &self,
        output: &mut String,
        entries: &[(usize, usize)],
        depth: usize,
        remove_checksum: bool,
    ) -> Result<()> {
        let mut pairs = self.pairs(entries, depth)?;
        if remove_checksum {
            pairs.retain(|&(k,_)| !matches!(self.node(k),Ok(N::Text(s)) if s.iter().copied().eq("checksum".chars().map(u32::from))));
        }
        let mut indices: Vec<usize> = (0..pairs.len()).collect();
        if !self.storage_json {
            source_sort_indices(
                &mut indices,
                false,
                |a, b| self.less(pairs[a].0, pairs[b].0, depth + 1),
                || self.tick(depth),
            )
            .map_err(|e| match e {
                SourceSortError::Callback(e) => e,
                _ => Error::Refused("sort_resource"),
            })?;
        }
        self.put(output, "{")?;
        for (i, index) in indices.into_iter().enumerate() {
            if i > 0 {
                self.put(output, if self.storage_json { ", " } else { "," })?;
            }
            let (key, value) = pairs[index];
            self.key(output, key)?;
            self.put(output, if self.storage_json { ": " } else { ":" })?;
            self.write(output, value, depth + 1)?;
        }
        self.put(output, "}")
    }
    fn write(&self, output: &mut String, id: usize, depth: usize) -> Result<()> {
        let marked = matches!(self.node(id)?, N::List(_) | N::Tuple(_) | N::Dict(_));
        if marked && !self.active.borrow_mut().insert(id) {
            return Err(source("builtins.ValueError", "Circular reference detected"));
        }
        let result = self.write_inner(output, id, depth);
        if marked {
            self.active.borrow_mut().remove(&id);
        }
        result
    }
    fn write_inner(&self, output: &mut String, id: usize, depth: usize) -> Result<()> {
        self.tick(depth)?;
        match self.node(id)? {
            N::None => self.put(output, "null"),
            N::Bool(b) => self.put(output, if *b { "true" } else { "false" }),
            N::Integer(s) => self.put(output, self.integer_text(s)?),
            N::FloatBits(bits) => self.put(output, &self.float(*bits)?),
            N::Text(s) => self.quote(output, s),
            N::List(items) | N::Tuple(items) => {
                self.put(output, "[")?;
                for (i, &id) in items.iter().enumerate() {
                    if i > 0 {
                        self.put(output, if self.storage_json { ", " } else { "," })?;
                    }
                    self.write(output, id, depth + 1)?;
                }
                self.put(output, "]")
            }
            N::Dict(entries) => self.dict(output, entries, depth, false),
            n => Err(source(
                "builtins.TypeError",
                format!("Object of type {} is not JSON serializable", name(n)),
            )),
        }
    }
    /// Default storage json.dumps followed by the existing original json.loads
    /// value parser. The result is a typed value tree, not a mutable object heap;
    /// shared decoder NaN identity across calls remains a separate state owner.
    pub fn storage_text(mut self) -> Result<String> {
        self.storage_json = true;
        let mut output = String::new();
        self.write(&mut output, self.root, 0)?;
        self.tick(0)?;
        Ok(output)
    }
    pub fn storage_copy(
        mut self,
        parse_limits: semantica_json::original_json::ParseLimits,
    ) -> Result<(String, NativeValue)> {
        self.storage_json = true;
        let mut output = String::new();
        self.write(&mut output, self.root, 0)?;
        self.tick(0)?;
        let value =
            semantica_json::original_json::parse_original_json(output.as_bytes(), parse_limits)
                .map_err(|error| match error {
                    semantica_json::original_json::ParseError::IntegerDigits(_) => {
                        source("builtins.ValueError", error.to_string())
                    }
                    semantica_json::original_json::ParseError::Resource(_)
                    | semantica_json::original_json::ParseError::Decode(_) => {
                        Error::Refused("storage_parse_resource")
                    }
                    semantica_json::original_json::ParseError::Syntax(_) => {
                        Error::Refused("internal_storage_roundtrip_syntax")
                    }
                })?;
        self.tick(0)?;
        Ok((output, value))
    }
    pub fn compute(&self) -> Result<(String, String)> {
        let mut output = String::new();
        self.write(&mut output, self.root, 0)?;
        self.tick(0)?;
        let digest = format!("{:x}", Sha256::digest(output.as_bytes()));
        Ok((digest, output))
    }
    pub fn verify(&self) -> Result<bool> {
        let entries = match self.node(self.root)? {
            N::Dict(entries) => entries,
            n => {
                return Err(source(
                    "builtins.AttributeError",
                    format!("'{}' object has no attribute 'get'", name(n)),
                ));
            }
        };
        let pairs = self.pairs(entries, 0)?;
        let stored=pairs.iter().find_map(|&(k,v)|matches!(self.node(k),Ok(N::Text(s)) if s.iter().copied().eq("checksum".chars().map(u32::from))).then_some(v));
        let Some(stored) = stored else {
            return Ok(false);
        };
        if !native_operations::truth(
            self.value
                .ok_or(Error::Refused("native_truth_owner_required"))?,
            stored,
        )
        .map_err(|_| Error::Refused("truth"))?
        {
            return Ok(false);
        }
        let mut output = String::new();
        self.dict(&mut output, entries, 0, true)?;
        self.tick(0)?;
        let digest = format!("{:x}", Sha256::digest(output.as_bytes()));
        Ok(
            matches!(self.node(stored)?,N::Text(s) if s.iter().copied().eq(digest.chars().map(u32::from))),
        )
    }
}
