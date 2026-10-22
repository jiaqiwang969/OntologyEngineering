//! Finite JSON keys with CPython 3.13.11's 64-bit SipHash13/set-table behavior.
//!
//! The explicit seed replaces interpreter startup entropy, not the algorithm.
//! This port follows CPython Objects/setobject.c and Python/pyhash.c (PSF-2.0).
//! It preserves iteration order and equal-key representatives; a Rust HashSet
//! or sorted projection cannot supply that source behavior.
use crate::{JsonError, python_equal};
use serde_json::Value;
use std::{cell::Cell, rc::Rc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetError {
    Unhashable(&'static str),
    Json(JsonError),
    ResourceLimit,
    DifferentContext,
}
impl From<JsonError> for SetError {
    fn from(e: JsonError) -> Self {
        Self::Json(e)
    }
}
impl std::fmt::Display for SetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unhashable(kind) => write!(f, "unhashable type: '{kind}'"),
            Self::Json(e) => write!(f, "{e}"),
            Self::ResourceLimit => f.write_str("Python set work or table limit exceeded"),
            Self::DifferentContext => f.write_str("Python sets require the same hash/work context"),
        }
    }
}
impl std::error::Error for SetError {}
type Result<T> = std::result::Result<T, SetError>;
const MODULUS: u64 = (1u64 << 61) - 1;
const MAX_SLOTS: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct SetContext {
    seed: u32,
    remaining: Rc<Cell<usize>>,
}
impl SetContext {
    pub fn new(seed: u32, work_limit: usize) -> Self {
        Self {
            seed,
            remaining: Rc::new(Cell::new(work_limit)),
        }
    }
    fn step(&self) -> Result<()> {
        let n = self
            .remaining
            .get()
            .checked_sub(1)
            .ok_or(SetError::ResourceLimit)?;
        self.remaining.set(n);
        Ok(())
    }
    pub fn hash(&self, value: &Value) -> Result<i64> {
        self.step()?;
        python_hash(value, self.seed)
    }
    pub fn equal(&self, left: &Value, right: &Value) -> Result<bool> {
        self.step()?;
        Ok(python_equal(left, right)?)
    }
}

fn round(v: &mut [u64; 4]) {
    v[0] = v[0].wrapping_add(v[1]);
    v[1] = v[1].rotate_left(13);
    v[1] ^= v[0];
    v[0] = v[0].rotate_left(32);
    v[2] = v[2].wrapping_add(v[3]);
    v[3] = v[3].rotate_left(16);
    v[3] ^= v[2];
    v[0] = v[0].wrapping_add(v[3]);
    v[3] = v[3].rotate_left(21);
    v[3] ^= v[0];
    v[2] = v[2].wrapping_add(v[1]);
    v[1] = v[1].rotate_left(17);
    v[1] ^= v[2];
    v[2] = v[2].rotate_left(32);
}
fn string_hash(s: &str, seed: u32) -> i64 {
    if s.is_empty() {
        return 0;
    }
    let mut secret = [0u8; 16];
    if seed != 0 {
        let mut x = seed;
        for b in &mut secret {
            x = x.wrapping_mul(214013).wrapping_add(2531011);
            *b = (x >> 16) as u8;
        }
    }
    let k0 = u64::from_le_bytes(secret[..8].try_into().unwrap());
    let k1 = u64::from_le_bytes(secret[8..].try_into().unwrap());
    // PEP 393 stores code points in the smallest 1/2/4-byte representation.
    let maximum = s.chars().map(u32::from).max().unwrap_or(0);
    let width = if maximum <= 255 {
        1
    } else if maximum <= 65535 {
        2
    } else {
        4
    };
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        bytes.extend_from_slice(&(c as u32).to_le_bytes()[..width]);
    }
    let mut v = [
        k0 ^ 0x736f6d6570736575,
        k1 ^ 0x646f72616e646f6d,
        k0 ^ 0x6c7967656e657261,
        k1 ^ 0x7465646279746573,
    ];
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let m = u64::from_le_bytes(chunk.try_into().unwrap());
        v[3] ^= m;
        round(&mut v);
        v[0] ^= m;
    }
    let mut b = (bytes.len() as u64) << 56;
    for (i, x) in chunks.remainder().iter().enumerate() {
        b |= (*x as u64) << (i * 8);
    }
    v[3] ^= b;
    round(&mut v);
    v[0] ^= b;
    v[2] ^= 0xff;
    for _ in 0..3 {
        round(&mut v);
    }
    (v[0] ^ v[1] ^ v[2] ^ v[3]) as i64
}
/// Hashable scalar finite JSON only. Lists and dictionaries raise source errors.
pub fn python_hash(value: &Value, seed: u32) -> Result<i64> {
    let hash = match value {
        Value::Null => 0xfca86420,
        Value::Bool(b) => i64::from(*b),
        Value::String(s) => string_hash(s, seed),
        Value::Array(_) => return Err(SetError::Unhashable("list")),
        Value::Object(_) => return Err(SetError::Unhashable("dict")),
        Value::Number(n) => {
            let token = n.to_string();
            if token.contains(['.', 'e', 'E']) {
                let x = n
                    .as_f64()
                    .filter(|x| x.is_finite())
                    .ok_or(JsonError::NonfiniteJsonNumber)?;
                let bits = x.to_bits();
                let exp = ((bits >> 52) & 0x7ff) as i32;
                let mantissa = (bits & ((1u64 << 52) - 1)) | if exp == 0 { 0 } else { 1u64 << 52 };
                let shift = (if exp == 0 { -1074 } else { exp - 1075 }).rem_euclid(61) as u32;
                let h = ((mantissa << shift) & MODULUS) | (mantissa >> (61 - shift));
                if bits >> 63 != 0 {
                    -(h as i64)
                } else {
                    h as i64
                }
            } else {
                let digits = token.trim_start_matches('-');
                if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(JsonError::MalformedJson.into());
                }
                let h = digits.bytes().fold(0u64, |v, b| {
                    ((v as u128 * 10 + (b - b'0') as u128) % MODULUS as u128) as u64
                });
                if token.starts_with('-') {
                    -(h as i64)
                } else {
                    h as i64
                }
            }
        }
    };
    Ok(if hash == -1 { -2 } else { hash })
}

#[derive(Clone, Debug)]
struct Entry {
    key: Value,
    hash: i64,
}
#[derive(Clone, Debug)]
enum Slot {
    Empty,
    Dummy,
    Entry(Entry),
}
#[derive(Clone, Debug)]
pub struct PythonSet {
    context: SetContext,
    table: Vec<Slot>,
    used: usize,
    fill: usize,
}
impl PythonSet {
    pub fn empty(context: &SetContext) -> Self {
        Self {
            context: context.clone(),
            table: vec![Slot::Empty; 8],
            used: 0,
            fill: 0,
        }
    }
    pub fn from_values(
        context: &SetContext,
        values: impl IntoIterator<Item = Value>,
    ) -> Result<Self> {
        let mut out = Self::empty(context);
        for value in values {
            out.insert(value)?;
        }
        Ok(out)
    }
    pub fn values(&self) -> Vec<Value> {
        self.entries().map(|e| e.key.clone()).collect()
    }
    fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.table.iter().filter_map(|s| match s {
            Slot::Entry(e) => Some(e),
            _ => None,
        })
    }
    fn find(&self, key: &Value, hash: i64) -> Result<(usize, bool)> {
        let mask = self.table.len() - 1;
        let mut i = hash as usize & mask;
        let mut perturb = hash as u64;
        let mut free = None;
        loop {
            let probes = if i + 9 <= mask { 9 } else { 0 };
            for at in i..=i + probes {
                self.context.step()?;
                match &self.table[at] {
                    Slot::Empty => return Ok((free.unwrap_or(at), false)),
                    Slot::Dummy => free = Some(at),
                    Slot::Entry(e) if e.hash == hash && python_equal(&e.key, key)? => {
                        return Ok((at, true));
                    }
                    _ => (),
                }
            }
            perturb >>= 5;
            i = i
                .wrapping_mul(5)
                .wrapping_add(1)
                .wrapping_add(perturb as usize)
                & mask;
        }
    }
    fn clean_insert(&mut self, entry: Entry) -> Result<()> {
        let mask = self.table.len() - 1;
        let mut i = entry.hash as usize & mask;
        let mut perturb = entry.hash as u64;
        loop {
            let probes = if i + 9 <= mask { 9 } else { 0 };
            for at in i..=i + probes {
                self.context.step()?;
                if matches!(self.table[at], Slot::Empty) {
                    self.table[at] = Slot::Entry(entry);
                    return Ok(());
                }
            }
            perturb >>= 5;
            i = i
                .wrapping_mul(5)
                .wrapping_add(1)
                .wrapping_add(perturb as usize)
                & mask;
        }
    }
    fn resize(&mut self, minimum: usize) -> Result<()> {
        let mut size = 8;
        while size <= minimum {
            size *= 2;
            if size > MAX_SLOTS {
                return Err(SetError::ResourceLimit);
            }
        }
        if size == 8 && self.table.len() == 8 && self.fill == self.used {
            return Ok(());
        }
        let old = std::mem::replace(&mut self.table, vec![Slot::Empty; size]);
        self.fill = self.used;
        for slot in old {
            if let Slot::Entry(e) = slot {
                self.clean_insert(e)?;
            }
        }
        Ok(())
    }
    pub fn insert(&mut self, key: Value) -> Result<()> {
        let hash = self.context.hash(&key)?;
        self.add(Entry { key, hash })
    }
    fn add(&mut self, e: Entry) -> Result<()> {
        let (at, found) = self.find(&e.key, e.hash)?;
        if found {
            return Ok(());
        }
        let virgin = matches!(self.table[at], Slot::Empty);
        self.table[at] = Slot::Entry(e);
        self.used += 1;
        if virgin {
            self.fill += 1;
            if self.fill * 5 >= (self.table.len() - 1) * 3 {
                self.resize(self.used * if self.used > 50000 { 2 } else { 4 })?;
            }
        }
        Ok(())
    }
    fn compatible(&self, other: &Self) -> Result<()> {
        if Rc::ptr_eq(&self.context.remaining, &other.context.remaining) {
            Ok(())
        } else {
            Err(SetError::DifferentContext)
        }
    }
    fn merge(&mut self, other: &Self) -> Result<()> {
        self.compatible(other)?;
        if other.used == 0 {
            return Ok(());
        }
        if (self.fill + other.used) * 5 >= (self.table.len() - 1) * 3 {
            self.resize((self.used + other.used) * 2)?;
        }
        if self.fill == 0 && self.table.len() == other.table.len() && other.fill == other.used {
            self.table = other.table.clone();
            self.fill = other.fill;
            self.used = other.used;
        } else if self.fill == 0 {
            self.fill = other.used;
            self.used = other.used;
            for e in other.entries() {
                self.clean_insert(e.clone())?;
            }
        } else {
            for e in other.entries() {
                self.add(e.clone())?;
            }
        }
        Ok(())
    }
    pub fn copy(&self) -> Result<Self> {
        let mut out = Self::empty(&self.context);
        out.merge(self)?;
        Ok(out)
    }
    pub fn union(&self, other: &Self) -> Result<Self> {
        self.compatible(other)?;
        let mut out = self.copy()?;
        if std::ptr::eq(self, other) {
            return Ok(out);
        }
        out.merge(other)?;
        Ok(out)
    }
    pub fn intersection(&self, other: &Self) -> Result<Self> {
        self.compatible(other)?;
        if std::ptr::eq(self, other) {
            return self.copy();
        }
        let (large, small) = if other.used > self.used {
            (other, self)
        } else {
            (self, other)
        };
        let mut out = Self::empty(&self.context);
        for e in small.entries() {
            if large.find(&e.key, e.hash)?.1 {
                out.add(e.clone())?;
            }
        }
        Ok(out)
    }
    pub fn difference(&self, other: &Self) -> Result<Self> {
        self.compatible(other)?;
        if (self.used >> 2) > other.used {
            let mut out = self.copy()?;
            for e in other.entries() {
                let (at, found) = out.find(&e.key, e.hash)?;
                if found {
                    out.table[at] = Slot::Dummy;
                    out.used -= 1;
                }
            }
            if out.fill - out.used > (out.table.len() - 1) / 4 {
                out.resize(out.used * if out.used > 50000 { 2 } else { 4 })?;
            }
            Ok(out)
        } else {
            let mut out = Self::empty(&self.context);
            for e in self.entries() {
                if !other.find(&e.key, e.hash)?.1 {
                    out.add(e.clone())?;
                }
            }
            Ok(out)
        }
    }
}
