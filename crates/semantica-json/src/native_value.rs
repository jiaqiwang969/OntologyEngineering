//! Lossless native data representation, independent of Python execution and Project authority.
//! Flat postorder nodes preserve type, container/key order and IEEE bits. Node IDs
//! identify retained data only; they do not assert Python object identity/equality.
use serde_json::{Value, json};
use std::{collections::BTreeSet, fmt};
pub const SCHEMA: &str = "semantica.native-value/1";
#[derive(Clone, Debug)]
pub enum NativeNode {
    None,
    Bool(bool),
    Integer(String),
    FloatBits(u64),
    Text(Vec<u32>),
    List(Vec<usize>),
    Tuple(Vec<usize>),
    Set(Vec<usize>),
    FrozenSet(Vec<usize>),
    Dict(Vec<(usize, usize)>),
}
#[derive(Clone, Copy, Debug)]
pub struct NativeLimits {
    pub max_nodes: usize,
    pub max_links: usize,
    pub max_scalar_bytes: usize,
    pub max_depth: usize,
    pub max_wire_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeError {
    Shape,
    Profile,
    NodeLimit,
    LinkLimit,
    ScalarLimit,
    DepthLimit,
    WireLimit,
    Integer,
    Codepoint,
    FloatBits,
    Reference,
    Unreachable,
    UnhashableKey,
    LossyJson,
    MalformedJson,
}
impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native_value_{self:?}")
    }
}
impl std::error::Error for NativeError {}
type Result<T> = std::result::Result<T, NativeError>;
#[derive(Clone, Debug)]
pub struct NativeValue {
    nodes: Vec<NativeNode>,
    root: usize,
}
fn add(total: &mut usize, n: usize, limit: usize, error: NativeError) -> Result<()> {
    *total = total.checked_add(n).filter(|n| *n <= limit).ok_or(error)?;
    Ok(())
}
fn integer(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && s != "-0"
}
fn children(n: &NativeNode) -> Vec<usize> {
    match n {
        NativeNode::List(v)
        | NativeNode::Tuple(v)
        | NativeNode::Set(v)
        | NativeNode::FrozenSet(v) => v.clone(),
        NativeNode::Dict(v) => v.iter().flat_map(|(k, v)| [*k, *v]).collect(),
        _ => vec![],
    }
}
fn link_count(n: &NativeNode) -> Result<usize> {
    match n {
        NativeNode::List(v)
        | NativeNode::Tuple(v)
        | NativeNode::Set(v)
        | NativeNode::FrozenSet(v) => Ok(v.len()),
        NativeNode::Dict(v) => v.len().checked_mul(2).ok_or(NativeError::LinkLimit),
        _ => Ok(0),
    }
}
fn scalar_bytes(n: &NativeNode) -> Result<usize> {
    Ok(match n {
        NativeNode::None => 0,
        NativeNode::Bool(_) => 1,
        NativeNode::Integer(v) => v.len(),
        NativeNode::FloatBits(_) => 8,
        NativeNode::Text(v) => v.len().checked_mul(4).ok_or(NativeError::ScalarLimit)?,
        _ => 0,
    })
}
fn wire_size(v: &Value, max: usize) -> Result<usize> {
    struct Count {
        n: usize,
        max: usize,
    }
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.n = self
                .n
                .checked_add(b.len())
                .filter(|n| *n <= self.max)
                .ok_or_else(|| std::io::Error::other("native wire limit"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut c = Count { n: 0, max };
    serde_json::to_writer(&mut c, v).map_err(|_| NativeError::WireLimit)?;
    Ok(c.n)
}
impl NativeValue {
    pub fn new(nodes: Vec<NativeNode>, root: usize, limits: NativeLimits) -> Result<Self> {
        let value = Self { nodes, root };
        value.validate(limits)?;
        Ok(value)
    }
    pub fn nodes(&self) -> &[NativeNode] {
        &self.nodes
    }
    pub fn root(&self) -> usize {
        self.root
    }
    pub fn validate(&self, limits: NativeLimits) -> Result<()> {
        if self.nodes.is_empty() || self.root >= self.nodes.len() {
            return Err(NativeError::Reference);
        }
        if self.nodes.len() > limits.max_nodes {
            return Err(NativeError::NodeLimit);
        }
        let (mut links, mut scalars) = (0, 0);
        let mut depths = Vec::with_capacity(self.nodes.len());
        let mut hashable = Vec::with_capacity(self.nodes.len());
        for (i, node) in self.nodes.iter().enumerate() {
            match node {
                NativeNode::Integer(s) if !integer(s) => return Err(NativeError::Integer),
                NativeNode::Text(s) if s.iter().any(|cp| *cp > 0x10ffff) => {
                    return Err(NativeError::Codepoint);
                }
                _ => {}
            }
            add(
                &mut scalars,
                scalar_bytes(node)?,
                limits.max_scalar_bytes,
                NativeError::ScalarLimit,
            )?;
            add(
                &mut links,
                link_count(node)?,
                limits.max_links,
                NativeError::LinkLimit,
            )?;
            let ids = children(node);
            if ids.iter().any(|id| *id >= i) {
                return Err(NativeError::Reference);
            }
            let depth = ids.iter().map(|id| depths[*id] + 1usize).max().unwrap_or(0);
            if depth > limits.max_depth {
                return Err(NativeError::DepthLimit);
            }
            depths.push(depth);
            let h = match node {
                NativeNode::List(_) | NativeNode::Set(_) | NativeNode::Dict(_) => false,
                NativeNode::Tuple(_) | NativeNode::FrozenSet(_) => {
                    ids.iter().all(|id| hashable[*id])
                }
                _ => true,
            };
            if let NativeNode::Dict(pairs) = node {
                if pairs.iter().any(|(key, _)| !hashable[*key]) {
                    return Err(NativeError::UnhashableKey);
                }
            }
            if matches!(node, NativeNode::Set(_) | NativeNode::FrozenSet(_))
                && ids.iter().any(|id| !hashable[*id])
            {
                return Err(NativeError::UnhashableKey);
            }
            hashable.push(h);
        }
        let mut reachable = vec![false; self.nodes.len()];
        reachable[self.root] = true;
        for i in (0..self.nodes.len()).rev() {
            if reachable[i] {
                for id in children(&self.nodes[i]) {
                    reachable[id] = true;
                }
            }
        }
        if reachable.iter().any(|r| !*r) {
            return Err(NativeError::Unreachable);
        }
        Ok(())
    }
    pub fn to_envelope(&self, limits: NativeLimits) -> Result<Value> {
        self.validate(limits)?;
        let nodes = self
            .nodes
            .iter()
            .map(|node| match node {
                NativeNode::None => json!({"kind":"none"}),
                NativeNode::Bool(v) => json!({"kind":"bool","value":v}),
                NativeNode::Integer(v) => json!({"kind":"int","decimal":v}),
                NativeNode::FloatBits(v) => json!({"kind":"float64","bits":format!("{v:016x}")}),
                NativeNode::Text(v) => json!({"kind":"text","codepoints":v}),
                NativeNode::List(v) => json!({"kind":"list","items":v}),
                NativeNode::Tuple(v) => json!({"kind":"tuple","items":v}),
                NativeNode::Set(v) => json!({"kind":"set","items":v}),
                NativeNode::FrozenSet(v) => json!({"kind":"frozenset","items":v}),
                NativeNode::Dict(v) => json!({"kind":"dict","entries":v}),
            })
            .collect::<Vec<_>>();
        let envelope = json!({"schema":SCHEMA,"root":self.root,"nodes":nodes});
        wire_size(&envelope, limits.max_wire_bytes)?;
        Ok(envelope)
    }
    pub fn encode(&self, limits: NativeLimits) -> Result<Vec<u8>> {
        serde_json::to_vec(&self.to_envelope(limits)?).map_err(|_| NativeError::Shape)
    }
    pub fn decode(bytes: &[u8], limits: NativeLimits) -> Result<Self> {
        if bytes.len() > limits.max_wire_bytes {
            return Err(NativeError::WireLimit);
        }
        let envelope = crate::strict_json(bytes).map_err(|_| NativeError::MalformedJson)?;
        Self::from_envelope(&envelope, limits)
    }
    pub fn from_envelope(value: &Value, limits: NativeLimits) -> Result<Self> {
        fn fields(v: &Value, names: &[&str]) -> Result<()> {
            let m = v.as_object().ok_or(NativeError::Shape)?;
            if m.len() != names.len() || names.iter().any(|n| !m.contains_key(*n)) {
                return Err(NativeError::Shape);
            }
            Ok(())
        }
        fn index(v: &Value) -> Result<usize> {
            usize::try_from(v.as_u64().ok_or(NativeError::Reference)?)
                .map_err(|_| NativeError::Reference)
        }
        fields(value, &["schema", "root", "nodes"])?;
        if value["schema"] != SCHEMA {
            return Err(NativeError::Profile);
        }
        let root = index(&value["root"])?;
        let input = value["nodes"].as_array().ok_or(NativeError::Shape)?;
        let result = Self::from_flat_nodes(root, input, limits)?;
        wire_size(value, limits.max_wire_bytes)?;
        Ok(result)
    }
    /// Already typed hosts pass flat fields by reference; no unvalidated deep Value clone.
    pub fn from_flat_nodes(root: usize, input: &[Value], limits: NativeLimits) -> Result<Self> {
        fn fields(v: &Value, names: &[&str]) -> Result<()> {
            let m = v.as_object().ok_or(NativeError::Shape)?;
            if m.len() != names.len() || names.iter().any(|n| !m.contains_key(*n)) {
                return Err(NativeError::Shape);
            }
            Ok(())
        }
        fn index(v: &Value) -> Result<usize> {
            usize::try_from(v.as_u64().ok_or(NativeError::Reference)?)
                .map_err(|_| NativeError::Reference)
        }
        if input.len() > limits.max_nodes {
            return Err(NativeError::NodeLimit);
        }
        let mut nodes = Vec::with_capacity(input.len());
        let (mut scalars, mut links) = (0, 0);
        for v in input {
            let kind = v["kind"].as_str().ok_or(NativeError::Shape)?;
            let node = match kind {
                "none" => {
                    fields(v, &["kind"])?;
                    NativeNode::None
                }
                "bool" => {
                    fields(v, &["kind", "value"])?;
                    NativeNode::Bool(v["value"].as_bool().ok_or(NativeError::Shape)?)
                }
                "int" => {
                    fields(v, &["kind", "decimal"])?;
                    let s = v["decimal"].as_str().ok_or(NativeError::Shape)?;
                    if s.len() > limits.max_scalar_bytes.saturating_sub(scalars) {
                        return Err(NativeError::ScalarLimit);
                    }
                    NativeNode::Integer(s.into())
                }
                "float64" => {
                    fields(v, &["kind", "bits"])?;
                    let s = v["bits"].as_str().ok_or(NativeError::FloatBits)?;
                    if s.len() != 16
                        || !s
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        return Err(NativeError::FloatBits);
                    }
                    NativeNode::FloatBits(
                        u64::from_str_radix(s, 16).map_err(|_| NativeError::FloatBits)?,
                    )
                }
                "text" => {
                    fields(v, &["kind", "codepoints"])?;
                    let a = v["codepoints"].as_array().ok_or(NativeError::Shape)?;
                    if a.len() > limits.max_scalar_bytes.saturating_sub(scalars) / 4 {
                        return Err(NativeError::ScalarLimit);
                    }
                    let a = a
                        .iter()
                        .map(|v| {
                            u32::try_from(v.as_u64().ok_or(NativeError::Codepoint)?)
                                .map_err(|_| NativeError::Codepoint)
                        })
                        .collect::<Result<Vec<_>>>()?;
                    NativeNode::Text(a)
                }
                "list" | "tuple" | "set" | "frozenset" => {
                    fields(v, &["kind", "items"])?;
                    let a = v["items"].as_array().ok_or(NativeError::Shape)?;
                    if a.len() > limits.max_links.saturating_sub(links) {
                        return Err(NativeError::LinkLimit);
                    }
                    let a = a.iter().map(index).collect::<Result<Vec<_>>>()?;
                    match kind {
                        "list" => NativeNode::List(a),
                        "tuple" => NativeNode::Tuple(a),
                        "set" => NativeNode::Set(a),
                        _ => NativeNode::FrozenSet(a),
                    }
                }
                "dict" => {
                    fields(v, &["kind", "entries"])?;
                    let a = v["entries"].as_array().ok_or(NativeError::Shape)?;
                    if a.len() > limits.max_links.saturating_sub(links) / 2 {
                        return Err(NativeError::LinkLimit);
                    }
                    let a = a
                        .iter()
                        .map(|v| {
                            let p = v.as_array().ok_or(NativeError::Shape)?;
                            if p.len() != 2 {
                                return Err(NativeError::Shape);
                            }
                            Ok((index(&p[0])?, index(&p[1])?))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    NativeNode::Dict(a)
                }
                _ => return Err(NativeError::Shape),
            };
            add(
                &mut scalars,
                scalar_bytes(&node)?,
                limits.max_scalar_bytes,
                NativeError::ScalarLimit,
            )?;
            add(
                &mut links,
                link_count(&node)?,
                limits.max_links,
                NativeError::LinkLimit,
            )?;
            nodes.push(node);
        }
        let result = Self::new(nodes, root, limits)?;
        result.to_envelope(limits)?;
        Ok(result)
    }
    /// A checked bridge. It rejects loss of container/key type, nonfinite IEEE
    /// values, surrogate codepoints, duplicate JSON keys and excessive expansion.
    pub fn to_plain_json(&self, limits: NativeLimits) -> Result<Value> {
        self.validate(NativeLimits {
            max_depth: limits.max_depth.min(128),
            ..limits
        })?;
        let mut values: Vec<Value> = Vec::with_capacity(self.nodes.len());
        let mut sizes = Vec::with_capacity(self.nodes.len());
        let mut wire_sizes = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let mut size = 1usize;
            for child in children(node) {
                add(
                    &mut size,
                    sizes[child],
                    limits.max_nodes,
                    NativeError::NodeLimit,
                )?;
            }
            // Check expanded wire size before cloning shared child values.
            match node {
                NativeNode::List(ids) => {
                    let mut bytes = 2usize;
                    add(
                        &mut bytes,
                        ids.len().saturating_sub(1),
                        limits.max_wire_bytes,
                        NativeError::WireLimit,
                    )?;
                    for id in ids {
                        add(
                            &mut bytes,
                            wire_sizes[*id],
                            limits.max_wire_bytes,
                            NativeError::WireLimit,
                        )?;
                    }
                }
                NativeNode::Dict(entries) => {
                    let mut bytes = 2usize;
                    add(
                        &mut bytes,
                        entries.len().saturating_sub(1),
                        limits.max_wire_bytes,
                        NativeError::WireLimit,
                    )?;
                    for (k, v) in entries {
                        add(
                            &mut bytes,
                            wire_sizes[*k],
                            limits.max_wire_bytes,
                            NativeError::WireLimit,
                        )?;
                        add(&mut bytes, 1, limits.max_wire_bytes, NativeError::WireLimit)?;
                        add(
                            &mut bytes,
                            wire_sizes[*v],
                            limits.max_wire_bytes,
                            NativeError::WireLimit,
                        )?;
                    }
                }
                _ => {}
            }
            let v = match node {
                NativeNode::None => Value::Null,
                NativeNode::Bool(v) => json!(v),
                NativeNode::Integer(s) => {
                    serde_json::from_str(s).map_err(|_| NativeError::Integer)?
                }
                NativeNode::FloatBits(bits) => Value::Number(
                    serde_json::Number::from_f64(f64::from_bits(*bits))
                        .ok_or(NativeError::LossyJson)?,
                ),
                NativeNode::Text(points) => Value::String(
                    points
                        .iter()
                        .map(|cp| char::from_u32(*cp).ok_or(NativeError::LossyJson))
                        .collect::<Result<String>>()?,
                ),
                NativeNode::List(ids) => {
                    Value::Array(ids.iter().map(|id| values[*id].clone()).collect())
                }
                NativeNode::Dict(entries) => {
                    let mut result = serde_json::Map::new();
                    let mut keys = BTreeSet::new();
                    for (k, v) in entries {
                        let key = values[*k].as_str().ok_or(NativeError::LossyJson)?;
                        if !keys.insert(key) {
                            return Err(NativeError::LossyJson);
                        }
                        result.insert(key.into(), values[*v].clone());
                    }
                    Value::Object(result)
                }
                _ => return Err(NativeError::LossyJson),
            };
            wire_sizes.push(wire_size(&v, limits.max_wire_bytes)?);
            sizes.push(size);
            values.push(v);
        }
        let value = values.swap_remove(self.root);
        Ok(value)
    }
    /// Literal JSON objects stay dictionaries even when their keys resemble tags.
    pub fn from_plain_json(value: &Value, limits: NativeLimits) -> Result<Self> {
        fn push(
            value: &Value,
            nodes: &mut Vec<NativeNode>,
            depth: usize,
            limits: NativeLimits,
            scalars: &mut usize,
            links: &mut usize,
        ) -> Result<usize> {
            if depth > limits.max_depth.min(128) {
                return Err(NativeError::DepthLimit);
            }
            let node = match value {
                Value::Null => NativeNode::None,
                Value::Bool(v) => NativeNode::Bool(*v),
                Value::String(v) => {
                    if v.chars().count() > limits.max_scalar_bytes.saturating_sub(*scalars) / 4 {
                        return Err(NativeError::ScalarLimit);
                    }
                    NativeNode::Text(v.chars().map(u32::from).collect())
                }
                Value::Number(v) => {
                    let s = v.to_string();
                    if s.contains(['.', 'e', 'E']) {
                        let f = s.parse::<f64>().map_err(|_| NativeError::LossyJson)?;
                        if !f.is_finite() {
                            return Err(NativeError::LossyJson);
                        }
                        NativeNode::FloatBits(f.to_bits())
                    } else {
                        NativeNode::Integer(s)
                    }
                }
                Value::Array(a) => {
                    add(links, a.len(), limits.max_links, NativeError::LinkLimit)?;
                    NativeNode::List(
                        a.iter()
                            .map(|v| push(v, nodes, depth + 1, limits, scalars, links))
                            .collect::<Result<Vec<_>>>()?,
                    )
                }
                Value::Object(o) => {
                    add(
                        links,
                        o.len().checked_mul(2).ok_or(NativeError::LinkLimit)?,
                        limits.max_links,
                        NativeError::LinkLimit,
                    )?;
                    let mut entries = vec![];
                    for (k, v) in o {
                        let key = push(&json!(k), nodes, depth + 1, limits, scalars, links)?;
                        let value = push(v, nodes, depth + 1, limits, scalars, links)?;
                        entries.push((key, value));
                    }
                    NativeNode::Dict(entries)
                }
            };
            add(
                scalars,
                scalar_bytes(&node)?,
                limits.max_scalar_bytes,
                NativeError::ScalarLimit,
            )?;
            if nodes.len() >= limits.max_nodes {
                return Err(NativeError::NodeLimit);
            }
            let id = nodes.len();
            nodes.push(node);
            Ok(id)
        }
        let mut nodes = vec![];
        let mut scalars = 0;
        let mut links = 0;
        let root = push(value, &mut nodes, 0, limits, &mut scalars, &mut links)?;
        let result = Self::new(nodes, root, limits)?;
        wire_size(value, limits.max_wire_bytes)?;
        Ok(result)
    }
}
