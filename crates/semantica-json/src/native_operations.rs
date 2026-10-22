//! Original-value operations over the existing flat carrier. No String coercion
//! of surrogate text, no Python delegate, no graph/index/authority ownership.
use crate::{
    PythonFloatError,
    native_value::{NativeNode as N, NativeValue},
};
use std::fmt;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationError {
    Reference,
    TextLimit,
    VisitLimit,
    DepthLimit,
    IntegerTextLimit,
    FloatInvalid,
    FloatType,
    FloatOverflow,
    Representation,
}
impl OperationError {
    /// Categories needed by the original callers' catch branches. Full Python
    /// error messages (which can embed arbitrary source text) are not synthesized.
    pub fn original_class(self) -> Option<&'static str> {
        match self {
            Self::IntegerTextLimit | Self::FloatInvalid => Some("ValueError"),
            Self::FloatType => Some("TypeError"),
            Self::FloatOverflow => Some("OverflowError"),
            _ => None,
        }
    }
}
impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native_operation_{self:?}")
    }
}
impl std::error::Error for OperationError {}
type Result<T> = std::result::Result<T, OperationError>;
#[derive(Clone, Copy, Debug)]
pub struct TextLimits {
    pub max_codepoints: usize,
    pub max_visits: usize,
    pub max_depth: usize,
}
fn node(value: &NativeValue, id: usize) -> Result<&N> {
    value.nodes().get(id).ok_or(OperationError::Reference)
}
pub fn truth(value: &NativeValue, id: usize) -> Result<bool> {
    Ok(match node(value, id)? {
        N::None => false,
        N::Bool(b) => *b,
        N::Integer(s) => s != "0",
        N::FloatBits(bits) => f64::from_bits(*bits) != 0.0,
        N::Text(s) => !s.is_empty(),
        N::List(v) | N::Tuple(v) | N::Set(v) | N::FrozenSet(v) => !v.is_empty(),
        N::Dict(v) => !v.is_empty(),
    })
}
/// Unicode scalar segments use the existing pinned Python numeric owner.
/// A surrogate in numeric text is an original conversion error, not replacement.
pub fn builtin_float(value: &NativeValue, id: usize) -> Result<f64> {
    match node(value, id)? {
        N::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        N::FloatBits(bits) => Ok(f64::from_bits(*bits)),
        N::Integer(s) => {
            let value = s
                .parse::<f64>()
                .map_err(|_| OperationError::FloatOverflow)?;
            if value.is_finite() {
                Ok(value)
            } else {
                Err(OperationError::FloatOverflow)
            }
        }
        N::Text(s) => {
            let text = s
                .iter()
                .map(|&cp| char::from_u32(cp).ok_or(OperationError::FloatInvalid))
                .collect::<Result<String>>()?;
            crate::python_builtin_float(&serde_json::Value::String(text)).map_err(|e| match e {
                PythonFloatError::IntegerOverflow => OperationError::FloatOverflow,
                PythonFloatError::InvalidInput => OperationError::FloatInvalid,
            })
        }
        _ => Err(OperationError::FloatType),
    }
}
/// Python whitespace owner treats surrogate codepoints as non-whitespace.
pub fn strip(text: &[u32]) -> &[u32] {
    let whitespace = |cp: u32| char::from_u32(cp).is_some_and(crate::python_unicode::is_whitespace);
    let start = text
        .iter()
        .position(|&cp| !whitespace(cp))
        .unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|&cp| !whitespace(cp))
        .map_or(start, |i| i + 1);
    &text[start..end]
}
enum Task {
    Node(usize, bool, usize),
    Token(&'static str),
}
struct Output {
    text: Vec<u32>,
    limit: usize,
}
impl Output {
    fn cp(&mut self, cp: u32) -> Result<()> {
        if self.text.len() >= self.limit {
            return Err(OperationError::TextLimit);
        }
        self.text.push(cp);
        Ok(())
    }
    fn token(&mut self, s: &str) -> Result<()> {
        for c in s.chars() {
            self.cp(c as u32)?;
        }
        Ok(())
    }
}
/// str/repr of observed carrier values, preserving supplied container order.
/// Does not construct Python sets, rehash keys, or assert object alias identity.
/// Decimal integer representation uses the observed default 4300-digit profile.
pub fn python_text(
    value: &NativeValue,
    id: usize,
    quoted: bool,
    limits: TextLimits,
) -> Result<Vec<u32>> {
    node(value, id)?;
    let mut output = Output {
        text: Vec::new(),
        limit: limits.max_codepoints,
    };
    let mut tasks = vec![Task::Node(id, quoted, 0)];
    let mut visits = 0usize;
    let mut pending = 1usize;
    while let Some(task) = tasks.pop() {
        let Task::Node(id, quoted, depth) = task else {
            if let Task::Token(s) = task {
                output.token(s)?;
            }
            continue;
        };
        pending -= 1;
        visits = visits
            .checked_add(1)
            .filter(|n| *n <= limits.max_visits)
            .ok_or(OperationError::VisitLimit)?;
        if depth > limits.max_depth {
            return Err(OperationError::DepthLimit);
        }
        match node(value, id)? {
            N::None => output.token("None")?,
            N::Bool(v) => output.token(if *v { "True" } else { "False" })?,
            N::Integer(s) => {
                if s.trim_start_matches('-').len() > 4300 {
                    return Err(OperationError::IntegerTextLimit);
                }
                output.token(s)?;
            }
            N::FloatBits(bits) => {
                let v = f64::from_bits(*bits);
                output.token(
                    &crate::python_float_repr(v).map_err(|_| OperationError::Representation)?,
                )?;
            }
            N::Text(s) => {
                if quoted {
                    crate::python_quote_points(s.iter().copied(), |cp| output.cp(cp))?;
                } else {
                    for &cp in s {
                        output.cp(cp)?;
                    }
                }
            }
            N::List(items) | N::Tuple(items) | N::Set(items) | N::FrozenSet(items) => {
                pending = pending
                    .checked_add(items.len())
                    .filter(|n| *n <= limits.max_visits - visits)
                    .ok_or(OperationError::VisitLimit)?;
                let (open, close) = match node(value, id)? {
                    N::List(_) => ("[", "]"),
                    N::Tuple(_) if items.len() == 1 => ("(", ",)"),
                    N::Tuple(_) => ("(", ")"),
                    N::Set(_) if items.is_empty() => ("set(", ")"),
                    N::Set(_) => ("{", "}"),
                    N::FrozenSet(_) if items.is_empty() => ("frozenset(", ")"),
                    N::FrozenSet(_) => ("frozenset({", "})"),
                    _ => unreachable!(),
                };
                output.token(open)?;
                tasks.push(Task::Token(close));
                for (i, &child) in items.iter().enumerate().rev() {
                    tasks.push(Task::Node(child, true, depth + 1));
                    if i > 0 {
                        tasks.push(Task::Token(", "));
                    }
                }
            }
            N::Dict(pairs) => {
                pending = pending
                    .checked_add(
                        pairs
                            .len()
                            .checked_mul(2)
                            .ok_or(OperationError::VisitLimit)?,
                    )
                    .filter(|n| *n <= limits.max_visits - visits)
                    .ok_or(OperationError::VisitLimit)?;
                output.token("{")?;
                tasks.push(Task::Token("}"));
                for (i, &(key, val)) in pairs.iter().enumerate().rev() {
                    tasks.push(Task::Node(val, true, depth + 1));
                    tasks.push(Task::Token(": "));
                    tasks.push(Task::Node(key, true, depth + 1));
                    if i > 0 {
                        tasks.push(Task::Token(", "));
                    }
                }
            }
        }
    }
    Ok(output.text)
}
