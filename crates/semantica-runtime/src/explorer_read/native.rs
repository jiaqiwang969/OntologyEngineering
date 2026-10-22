//! Typed GraphSession node normalization. The existing carrier owns values;
//! this adapter preserves dict update order and copies only reachable output.
use anyhow::{Context, Result, bail, ensure};
use semantica_json::{
    native_operations::{self as ops, TextLimits},
    native_value::{NativeLimits, NativeNode as N, NativeValue},
};
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, Copy)]
enum ValueRef {
    Source(usize),
    None,
    Text(&'static str),
    Character(u32),
}
/// The original library uses different node views for index construction and
/// result lookup. Neither view is the authoritative, seven-field node state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeNodeProjection {
    ToDict,
    FindNode,
}
#[derive(Clone, Copy)]
struct NodeState {
    id: ValueRef,
    kind: ValueRef,
    content: ValueRef,
    metadata: ValueRef,
    properties: ValueRef,
    from: ValueRef,
    until: ValueRef,
}
enum NodeRecord<'a> {
    Raw(&'a [(usize, usize)]),
    Projected(NodeState, NativeNodeProjection),
}
struct Properties {
    entries: Vec<(ValueRef, ValueRef)>,
    positions: BTreeMap<Vec<u8>, usize>,
}
impl Properties {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            positions: BTreeMap::new(),
        }
    }
    fn get(&self, name: &str) -> Option<ValueRef> {
        self.positions
            .get(&text_key(name.chars().map(u32::from)))
            .map(|&i| self.entries[i].1)
    }
}
fn text_key(points: impl Iterator<Item = u32>) -> Vec<u8> {
    let mut key = vec![b'T'];
    for cp in points {
        key.extend_from_slice(&cp.to_be_bytes());
    }
    key
}
fn timely(d: Instant) -> Result<()> {
    ensure!(Instant::now() < d, "explorer_normalize_deadline_exceeded");
    Ok(())
}
/// Only modeled original failures qualify for partial GraphSession effects.
/// Resource, deadline, malformed-reference and identity-pending errors do not.
pub(crate) fn is_original_normalization_error(error: &anyhow::Error) -> bool {
    matches!(
        error.to_string().as_str(),
        "explorer_normalize_update_type_error"
            | "explorer_normalize_update_value_error"
            | "explorer_normalize_key_type_error"
    ) || error
        .downcast_ref::<ops::OperationError>()
        .is_some_and(|error| matches!(error, ops::OperationError::IntegerTextLimit))
}
fn child(node: &N, index: usize) -> Option<usize> {
    match node {
        N::List(ids) | N::Tuple(ids) | N::Set(ids) | N::FrozenSet(ids) => ids.get(index).copied(),
        N::Dict(pairs) => pairs
            .get(index / 2)
            .map(|&(k, v)| if index % 2 == 0 { k } else { v }),
        _ => None,
    }
}
fn size(node: &N) -> Result<(usize, usize)> {
    Ok(match node {
        N::None => (0, 0),
        N::Bool(_) => (0, 1),
        N::Integer(s) => (0, s.len()),
        N::FloatBits(_) => (0, 8),
        N::Text(s) => (
            0,
            s.len()
                .checked_mul(4)
                .context("explorer_normalize_scalar_limit")?,
        ),
        N::List(v) | N::Tuple(v) | N::Set(v) | N::FrozenSet(v) => (v.len(), 0),
        N::Dict(v) => (
            v.len()
                .checked_mul(2)
                .context("explorer_normalize_link_limit")?,
            0,
        ),
    })
}
struct Output {
    nodes: Vec<N>,
    depths: Vec<usize>,
    copied: BTreeMap<usize, usize>,
    links: usize,
    scalars: usize,
    limits: NativeLimits,
}
impl Output {
    fn check(&self, links: usize, scalars: usize) -> Result<()> {
        ensure!(
            self.nodes.len() < self.limits.max_nodes,
            "explorer_normalize_node_limit"
        );
        ensure!(
            links <= self.limits.max_links.saturating_sub(self.links),
            "explorer_normalize_link_limit"
        );
        ensure!(
            scalars <= self.limits.max_scalar_bytes.saturating_sub(self.scalars),
            "explorer_normalize_scalar_limit"
        );
        Ok(())
    }
    fn add(&mut self, node: N) -> Result<usize> {
        let (links, scalars) = size(&node)?;
        self.check(links, scalars)?;
        let mut depth = 0;
        for i in 0..links {
            let id = child(&node, i).context("explorer_normalize_reference")?;
            depth = depth.max(
                self.depths
                    .get(id)
                    .context("explorer_normalize_reference")?
                    .checked_add(1)
                    .context("explorer_normalize_depth_limit")?,
            );
        }
        ensure!(
            depth <= self.limits.max_depth,
            "explorer_normalize_depth_limit"
        );
        self.links += links;
        self.scalars += scalars;
        let id = self.nodes.len();
        self.nodes.push(node);
        self.depths.push(depth);
        Ok(id)
    }
}
struct Normalizer<'a> {
    input: &'a NativeValue,
    output: Output,
    text_limits: TextLimits,
    visits: usize,
    points: usize,
    key_bytes: usize,
    d: Instant,
}
impl<'a> Normalizer<'a> {
    fn new(
        input: &'a NativeValue,
        limits: NativeLimits,
        text_limits: TextLimits,
        d: Instant,
    ) -> Self {
        Self {
            input,
            output: Output {
                nodes: Vec::new(),
                depths: Vec::new(),
                copied: BTreeMap::new(),
                links: 0,
                scalars: 0,
                limits,
            },
            text_limits,
            visits: 0,
            points: 0,
            key_bytes: 0,
            d,
        }
    }
    fn visit(&mut self, depth: usize) -> Result<()> {
        timely(self.d)?;
        ensure!(
            depth <= self.text_limits.max_depth,
            "explorer_normalize_text_depth_limit"
        );
        self.visits = self
            .visits
            .checked_add(1)
            .filter(|n| *n <= self.text_limits.max_visits)
            .context("explorer_normalize_visit_limit")?;
        Ok(())
    }
    fn node(&self, id: usize) -> Result<&'a N> {
        self.input
            .nodes()
            .get(id)
            .context("explorer_normalize_reference")
    }
    fn points(&mut self, count: usize) -> Result<()> {
        self.points = self
            .points
            .checked_add(count)
            .filter(|n| *n <= self.text_limits.max_codepoints)
            .context("explorer_normalize_text_limit")?;
        Ok(())
    }
    fn key_space(&mut self, count: usize) -> Result<()> {
        self.key_bytes = self
            .key_bytes
            .checked_add(count)
            .filter(|n| *n <= self.output.limits.max_scalar_bytes)
            .context("explorer_normalize_key_work_limit")?;
        Ok(())
    }
    fn is_text(&self, id: usize, name: &str) -> bool {
        matches!(self.input.nodes().get(id),Some(N::Text(v)) if v.iter().copied().eq(name.chars().map(u32::from)))
    }
    fn raw(&mut self, pairs: &[(usize, usize)], name: &str) -> Result<Option<ValueRef>> {
        let mut result = None;
        for &(k, v) in pairs {
            self.visit(0)?;
            if self.is_text(k, name) {
                result = Some(ValueRef::Source(v));
            }
        }
        Ok(result)
    }
    fn record_field(&mut self, record: &NodeRecord<'_>, name: &str) -> Result<Option<ValueRef>> {
        match record {
            NodeRecord::Raw(raw) => self.raw(raw, name),
            NodeRecord::Projected(state, mode) => {
                self.visit(0)?;
                // ContextGraph.find_node does not project either independent
                // validity field. normalize_node may only find them in metadata.
                Ok(match name {
                    "id" => Some(state.id),
                    "type" => Some(state.kind),
                    "content" if *mode == NativeNodeProjection::FindNode => Some(state.content),
                    _ => None,
                })
            }
        }
    }
    fn truth(&mut self, value: ValueRef) -> Result<bool> {
        self.visit(0)?;
        Ok(match value {
            ValueRef::None => false,
            ValueRef::Text(s) => !s.is_empty(),
            ValueRef::Character(_) => true,
            ValueRef::Source(id) => ops::truth(self.input, id)?,
        })
    }
    fn none(&self, value: ValueRef) -> Result<bool> {
        Ok(match value {
            ValueRef::None => true,
            ValueRef::Source(id) => matches!(self.node(id)?, N::None),
            _ => false,
        })
    }
    fn string(&mut self, value: ValueRef) -> Result<Vec<u32>> {
        let result = match value {
            ValueRef::Source(id) => {
                let before = self.visits;
                let mut stack = vec![(id, 0usize, 0usize)];
                while let Some((id, next, depth)) = stack.pop() {
                    if next == 0 {
                        self.visit(depth)?;
                    }
                    if let Some(child) = child(self.node(id)?, next) {
                        stack.push((id, next + 1, depth));
                        stack.push((child, 0, depth + 1));
                    }
                }
                ops::python_text(
                    self.input,
                    id,
                    false,
                    TextLimits {
                        max_codepoints: self.text_limits.max_codepoints - self.points,
                        max_visits: self.visits - before,
                        max_depth: self.text_limits.max_depth,
                    },
                )?
            }
            ValueRef::None => {
                self.visit(0)?;
                "None".chars().map(u32::from).collect()
            }
            ValueRef::Text(s) => {
                self.visit(0)?;
                s.chars().map(u32::from).collect()
            }
            ValueRef::Character(cp) => {
                self.visit(0)?;
                vec![cp]
            }
        };
        self.points(result.len())?;
        timely(self.d)?;
        Ok(result)
    }
    // Canonical equality keys are local to dict.update. Integers, booleans and
    // integral floats share Python numeric equality; original key/value objects
    // and insertion positions remain in `entries`, never replaced by these keys.
    fn equality_key(&mut self, value: ValueRef) -> Result<Vec<u8>> {
        enum Task {
            Value(ValueRef, usize),
            Sequence(usize, bool),
        }
        let mut tasks = vec![Task::Value(value, 0)];
        let mut values: Vec<Vec<u8>> = Vec::new();
        let mut pending = 1usize;
        while let Some(task) = tasks.pop() {
            timely(self.d)?;
            match task {
                Task::Sequence(count, frozen) => {
                    let offset = values
                        .len()
                        .checked_sub(count)
                        .context("explorer_normalize_key_shape")?;
                    let mut parts = values.split_off(offset);
                    if frozen {
                        parts.sort();
                        parts.dedup();
                    }
                    let length = parts.iter().try_fold(9usize, |n, p| {
                        n.checked_add(8)
                            .and_then(|n| n.checked_add(p.len()))
                            .context("explorer_normalize_key_work_limit")
                    })?;
                    self.key_space(length)?;
                    let mut result = Vec::with_capacity(length);
                    result.push(if frozen { b'S' } else { b'U' });
                    result.extend_from_slice(&(parts.len() as u64).to_be_bytes());
                    for part in parts {
                        result.extend_from_slice(&(part.len() as u64).to_be_bytes());
                        result.extend_from_slice(&part);
                    }
                    values.push(result);
                }
                Task::Value(value, depth) => {
                    pending -= 1;
                    self.visit(depth)?;
                    let scalar = match value {
                        ValueRef::None => vec![b'N'],
                        ValueRef::Text(s) => {
                            self.key_space(
                                s.chars()
                                    .count()
                                    .checked_mul(4)
                                    .and_then(|n| n.checked_add(1))
                                    .context("explorer_normalize_key_work_limit")?,
                            )?;
                            values.push(text_key(s.chars().map(u32::from)));
                            continue;
                        }
                        ValueRef::Character(cp) => {
                            self.key_space(5)?;
                            values.push(text_key(std::iter::once(cp)));
                            continue;
                        }
                        ValueRef::Source(id) => match self.node(id)? {
                            N::None => vec![b'N'],
                            N::Bool(v) => {
                                if *v {
                                    b"I1".to_vec()
                                } else {
                                    b"I0".to_vec()
                                }
                            }
                            N::Integer(s) => {
                                self.key_space(
                                    s.len()
                                        .checked_add(1)
                                        .context("explorer_normalize_key_work_limit")?,
                                )?;
                                let mut k = Vec::with_capacity(s.len() + 1);
                                k.push(b'I');
                                k.extend_from_slice(s.as_bytes());
                                values.push(k);
                                continue;
                            }
                            N::FloatBits(bits) => {
                                let number = f64::from_bits(*bits);
                                ensure!(
                                    !number.is_nan(),
                                    "explorer_normalize_key_identity_pending"
                                );
                                if number.is_finite() && number.fract() == 0.0 {
                                    let decimal = if number == 0.0 {
                                        "0".to_owned()
                                    } else {
                                        format!("{number:.0}")
                                    };
                                    let mut k = vec![b'I'];
                                    k.extend_from_slice(decimal.as_bytes());
                                    k
                                } else {
                                    let mut k = vec![b'F'];
                                    k.extend_from_slice(&bits.to_be_bytes());
                                    k
                                }
                            }
                            N::Text(points) => {
                                self.key_space(
                                    points
                                        .len()
                                        .checked_mul(4)
                                        .and_then(|n| n.checked_add(1))
                                        .context("explorer_normalize_key_work_limit")?,
                                )?;
                                values.push(text_key(points.iter().copied()));
                                continue;
                            }
                            N::Tuple(items) | N::FrozenSet(items) => {
                                pending = pending
                                    .checked_add(items.len())
                                    .filter(|n| {
                                        *n <= self
                                            .text_limits
                                            .max_visits
                                            .saturating_sub(self.visits)
                                    })
                                    .context("explorer_normalize_visit_limit")?;
                                tasks.push(Task::Sequence(
                                    items.len(),
                                    matches!(self.node(id)?, N::FrozenSet(_)),
                                ));
                                for &id in items.iter().rev() {
                                    tasks.push(Task::Value(ValueRef::Source(id), depth + 1));
                                }
                                continue;
                            }
                            _ => bail!("explorer_normalize_key_type_error"),
                        },
                    };
                    self.key_space(scalar.len())?;
                    values.push(scalar);
                }
            }
        }
        ensure!(values.len() == 1, "explorer_normalize_key_shape");
        Ok(values.pop().unwrap())
    }
    fn put(&mut self, map: &mut Properties, key: ValueRef, value: ValueRef) -> Result<()> {
        let equality = self.equality_key(key)?;
        if let Some(&position) = map.positions.get(&equality) {
            map.entries[position].1 = value;
        } else {
            ensure!(
                map.entries.len()
                    < self
                        .output
                        .limits
                        .max_links
                        .saturating_sub(self.output.links)
                        / 2,
                "explorer_normalize_link_limit"
            );
            let position = map.entries.len();
            map.entries.push((key, value));
            map.positions.insert(equality, position);
        }
        Ok(())
    }
    fn pair(&mut self, entry: ValueRef) -> Result<(ValueRef, ValueRef)> {
        self.visit(0)?;
        let ValueRef::Source(id) = entry else {
            bail!("explorer_normalize_update_value_error")
        };
        match self.node(id)? {
            N::List(items) | N::Tuple(items) | N::Set(items) | N::FrozenSet(items) => {
                ensure!(items.len() == 2, "explorer_normalize_update_value_error");
                Ok((ValueRef::Source(items[0]), ValueRef::Source(items[1])))
            }
            N::Dict(pairs) => {
                ensure!(pairs.len() == 2, "explorer_normalize_update_value_error");
                Ok((ValueRef::Source(pairs[0].0), ValueRef::Source(pairs[1].0)))
            }
            N::Text(points) => {
                ensure!(points.len() == 2, "explorer_normalize_update_value_error");
                Ok((
                    ValueRef::Character(points[0]),
                    ValueRef::Character(points[1]),
                ))
            }
            _ => bail!("explorer_normalize_update_type_error"),
        }
    }
    fn update(&mut self, map: &mut Properties, value: ValueRef) -> Result<()> {
        if !self.truth(value)? {
            return Ok(());
        }
        self.update_required(map, value)
    }
    // ContextNode.to_dict calls props.update(metadata) without `or {}`.
    // None/False/0 are errors there, while empty iterable containers are valid.
    fn update_required(&mut self, map: &mut Properties, value: ValueRef) -> Result<()> {
        self.visit(0)?;
        let ValueRef::Source(id) = value else {
            bail!("explorer_normalize_update_type_error")
        };
        match self.node(id)? {
            N::Dict(pairs) => {
                for &(key, value) in pairs {
                    self.visit(0)?;
                    self.put(map, ValueRef::Source(key), ValueRef::Source(value))?;
                }
            }
            N::List(items) | N::Tuple(items) | N::Set(items) | N::FrozenSet(items) => {
                for &item in items {
                    let (key, value) = self.pair(ValueRef::Source(item))?;
                    self.put(map, key, value)?;
                }
            }
            N::Text(points) if points.is_empty() => {}
            N::Text(_) => bail!("explorer_normalize_update_value_error"),
            _ => bail!("explorer_normalize_update_type_error"),
        }
        Ok(())
    }
    fn copy_source(&mut self, root: usize) -> Result<usize> {
        if let Some(&copied) = self.output.copied.get(&root) {
            return Ok(copied);
        }
        let mut stack = vec![(root, 0usize)];
        while let Some((id, next)) = stack.pop() {
            timely(self.d)?;
            if self.output.copied.contains_key(&id) {
                continue;
            }
            let source = self.node(id)?;
            if next == 0 {
                self.visit(0)?;
                ensure!(
                    stack.len() <= self.output.limits.max_depth,
                    "explorer_normalize_depth_limit"
                );
                let (links, scalars) = size(source)?;
                self.output.check(links, scalars)?;
            }
            if let Some(child) = child(source, next) {
                stack.push((id, next + 1));
                if !self.output.copied.contains_key(&child) {
                    stack.push((child, 0));
                }
                continue;
            }
            let (links, scalars) = size(source)?;
            self.output.check(links, scalars)?;
            let mapped = |id: usize| -> Result<usize> {
                self.output
                    .copied
                    .get(&id)
                    .copied()
                    .context("explorer_normalize_reference")
            };
            let copied = match source {
                N::None => N::None,
                N::Bool(v) => N::Bool(*v),
                N::Integer(v) => N::Integer(v.clone()),
                N::FloatBits(v) => N::FloatBits(*v),
                N::Text(v) => N::Text(v.clone()),
                N::List(v) => N::List(v.iter().map(|id| mapped(*id)).collect::<Result<_>>()?),
                N::Tuple(v) => N::Tuple(v.iter().map(|id| mapped(*id)).collect::<Result<_>>()?),
                N::Set(v) => N::Set(v.iter().map(|id| mapped(*id)).collect::<Result<_>>()?),
                N::FrozenSet(v) => {
                    N::FrozenSet(v.iter().map(|id| mapped(*id)).collect::<Result<_>>()?)
                }
                N::Dict(v) => N::Dict(
                    v.iter()
                        .map(|(k, v)| Ok((mapped(*k)?, mapped(*v)?)))
                        .collect::<Result<_>>()?,
                ),
            };
            let copied = self.output.add(copied)?;
            self.output.copied.insert(id, copied);
        }
        self.output
            .copied
            .get(&root)
            .copied()
            .context("explorer_normalize_reference")
    }
    fn copy(&mut self, value: ValueRef) -> Result<usize> {
        timely(self.d)?;
        match value {
            ValueRef::Source(id) => self.copy_source(id),
            ValueRef::None => self.output.add(N::None),
            ValueRef::Character(cp) => self.output.add(N::Text(vec![cp])),
            ValueRef::Text(s) => {
                self.output.check(0, s.chars().count() * 4)?;
                self.output.add(N::Text(s.chars().map(u32::from).collect()))
            }
        }
    }
    fn node_normalized(&mut self, id: usize) -> Result<usize> {
        self.visit(0)?;
        let N::Dict(raw) = self.node(id)? else {
            bail!("explorer_normalize_node_mapping_required")
        };
        let mut properties = Properties::new();
        for key in ["metadata", "properties"] {
            if let Some(value) = self.raw(raw, key)? {
                self.update(&mut properties, value)?;
            }
        }
        self.normalize_record(NodeRecord::Raw(raw), properties)
    }
    fn normalize_record(
        &mut self,
        raw: NodeRecord<'_>,
        mut properties: Properties,
    ) -> Result<usize> {
        let mut content = self
            .record_field(&raw, "content")?
            .unwrap_or(ValueRef::None);
        if self.none(content)? {
            let mut chosen = None;
            for key in ["content", "text", "label", "name"] {
                if let Some(value) = properties.get(key) {
                    if self.truth(value)? {
                        chosen = Some(value);
                        break;
                    }
                }
            }
            if chosen.is_none() {
                for key in ["label", "name"] {
                    if let Some(value) = self.record_field(&raw, key)? {
                        if self.truth(value)? {
                            chosen = Some(value);
                            break;
                        }
                    }
                }
            }
            content = match chosen {
                Some(value) => value,
                None => self.record_field(&raw, "id")?.unwrap_or(ValueRef::Text("")),
            };
        }
        let from = self
            .record_field(&raw, "valid_from")?
            .or_else(|| properties.get("valid_from"))
            .unwrap_or(ValueRef::None);
        let until = self
            .record_field(&raw, "valid_until")?
            .or_else(|| properties.get("valid_until"))
            .unwrap_or(ValueRef::None);
        if properties.get("content").is_none() {
            self.put(&mut properties, ValueRef::Text("content"), content)?;
        }
        if !self.none(from)? {
            self.put(&mut properties, ValueRef::Text("valid_from"), from)?;
        }
        if !self.none(until)? {
            self.put(&mut properties, ValueRef::Text("valid_until"), until)?;
        }
        let id = self.record_field(&raw, "id")?.unwrap_or(ValueRef::Text(""));
        let kind = self
            .record_field(&raw, "type")?
            .unwrap_or(ValueRef::Text("entity"));
        let id = self.string(id)?;
        let kind = self.string(kind)?;
        let content = if self.truth(content)? {
            self.string(content)?
        } else {
            self.string(ValueRef::Text(""))?
        };
        let id = self.output.add(N::Text(id))?;
        let kind = self.output.add(N::Text(kind))?;
        let content = self.output.add(N::Text(content))?;
        let properties = self.copy_properties(properties)?;
        let from = self.copy(from)?;
        let until = self.copy(until)?;
        let mut fields = Vec::new();
        for (name, value) in [
            ("id", id),
            ("type", kind),
            ("content", content),
            ("properties", properties),
            ("valid_from", from),
            ("valid_until", until),
        ] {
            fields.push((self.copy(ValueRef::Text(name))?, value));
        }
        self.output.add(N::Dict(fields))
    }
    fn copy_properties(&mut self, properties: Properties) -> Result<usize> {
        let mut pairs = Vec::new();
        for (key, value) in properties.entries {
            let key = self.copy(key)?;
            let value = self.copy(value)?;
            pairs.push((key, value));
        }
        self.output.add(N::Dict(pairs))
    }
    fn node_state(&mut self, id: usize) -> Result<NodeState> {
        self.visit(0)?;
        let N::Dict(raw) = self.node(id)? else {
            bail!("explorer_node_state_mapping_required")
        };
        const FIELDS: [&str; 7] = [
            "id",
            "type",
            "content",
            "metadata",
            "properties",
            "valid_from",
            "valid_until",
        ];
        ensure!(raw.len() == FIELDS.len(), "explorer_node_state_shape");
        let mut values = [ValueRef::None; 7];
        let mut seen = [false; 7];
        for &(key, value) in raw {
            self.visit(0)?;
            let position = FIELDS
                .iter()
                .position(|name| self.is_text(key, name))
                .context("explorer_node_state_shape")?;
            ensure!(!seen[position], "explorer_node_state_shape");
            seen[position] = true;
            values[position] = ValueRef::Source(value);
        }
        ensure!(seen.iter().all(|v| *v), "explorer_node_state_shape");
        let [id, kind, content, metadata, properties, from, until] = values;
        Ok(NodeState {
            id,
            kind,
            content,
            metadata,
            properties,
            from,
            until,
        })
    }
    fn state_properties(
        &mut self,
        state: NodeState,
        mode: NativeNodeProjection,
    ) -> Result<Properties> {
        let mut properties = Properties::new();
        match mode {
            NativeNodeProjection::ToDict => {
                let ValueRef::Source(id) = state.properties else {
                    bail!("explorer_node_state_properties_attribute_error")
                };
                match self.node(id)? {
                    N::Dict(_) => self.update_required(&mut properties, state.properties)?,
                    // A builtin set has copy/update, but update can raise TypeError
                    // or the subsequent props["content"] assignment always does.
                    // Other retained builtin types lack copy or update. We model
                    // their original class; no custom-object behavior is inferred.
                    N::Set(_) => bail!("explorer_node_state_properties_type_error"),
                    _ => bail!("explorer_node_state_properties_attribute_error"),
                }
                self.update_required(&mut properties, state.metadata)?;
                self.put(&mut properties, ValueRef::Text("content"), state.content)?;
                if !self.none(state.from)? {
                    self.put(&mut properties, ValueRef::Text("valid_from"), state.from)?;
                }
                if !self.none(state.until)? {
                    self.put(&mut properties, ValueRef::Text("valid_until"), state.until)?;
                }
            }
            NativeNodeProjection::FindNode => {
                self.update(&mut properties, state.metadata)?;
                self.update(&mut properties, state.properties)?;
            }
        }
        Ok(properties)
    }
    fn project_state_node(&mut self, id: usize, mode: NativeNodeProjection) -> Result<usize> {
        let state = self.node_state(id)?;
        let properties = self.state_properties(state, mode)?;
        let id = self.copy(state.id)?;
        let kind = self.copy(state.kind)?;
        let mut fields = vec![
            (self.copy(ValueRef::Text("id"))?, id),
            (self.copy(ValueRef::Text("type"))?, kind),
        ];
        let property_name = match mode {
            NativeNodeProjection::ToDict => "properties",
            NativeNodeProjection::FindNode => {
                let content = self.copy(state.content)?;
                fields.push((self.copy(ValueRef::Text("content"))?, content));
                "metadata"
            }
        };
        let properties = self.copy_properties(properties)?;
        fields.push((self.copy(ValueRef::Text(property_name))?, properties));
        self.output.add(N::Dict(fields))
    }
    fn state_normalized_node(&mut self, id: usize, mode: NativeNodeProjection) -> Result<usize> {
        let state = self.node_state(id)?;
        let properties = self.state_properties(state, mode)?;
        // The temporary projected mapping is represented by a borrowed field
        // view. The same normalizer consumes it without copying the whole graph
        // or resetting the batch's builder, text allowance or deadline.
        self.normalize_record(NodeRecord::Projected(state, mode), properties)
    }
    fn search_result(&mut self, node: usize, score: f64) -> Result<usize> {
        let score = self.output.add(N::FloatBits(score.to_bits()))?;
        let node_key = self.copy(ValueRef::Text("node"))?;
        let score_key = self.copy(ValueRef::Text("score"))?;
        self.output
            .add(N::Dict(vec![(node_key, node), (score_key, score)]))
    }
    fn finish(self, root: usize) -> Result<NativeValue> {
        timely(self.d)?;
        let value = NativeValue::new(self.output.nodes, root, self.output.limits)?;
        // Enforce the caller's typed-wire limit using the single carrier owner.
        value.to_envelope(self.output.limits)?;
        timely(self.d)?;
        Ok(value)
    }
}

/// Normalize one selected dictionary without cloning its enclosing input graph.
/// NaN keys (including nested keys) are explicitly pending: the carrier does not
/// identify Python objects, which dict lookup uses even when NaN != NaN.
pub fn normalize_node_native(
    value: &NativeValue,
    node: usize,
    limits: NativeLimits,
    text_limits: TextLimits,
    d: Instant,
) -> Result<NativeValue> {
    timely(d)?;
    let mut normalizer = Normalizer::new(value, limits, text_limits, d);
    let root = normalizer.node_normalized(node)?;
    normalizer.finish(root)
}

/// Bulk counterpart, with one output builder, shared reachable values and one
/// cumulative operation allowance. Output order follows the supplied node list.
pub fn normalize_nodes_native(
    value: &NativeValue,
    nodes: &[usize],
    limits: NativeLimits,
    text_limits: TextLimits,
    d: Instant,
) -> Result<NativeValue> {
    timely(d)?;
    let mut normalizer = Normalizer::new(value, limits, text_limits, d);
    let mut output = Vec::new();
    for &node in nodes {
        ensure!(
            output.len() < limits.max_links,
            "explorer_normalize_link_limit"
        );
        output.push(normalizer.node_normalized(node)?);
    }
    let root = normalizer.output.add(N::List(output))?;
    normalizer.finish(root)
}

/// GraphSession.search projects matches back through current raw graph records.
/// The caller resolves those record IDs; this helper shares the same bounded
/// normalizer and does not make an index match into a retained graph snapshot.
pub(crate) fn normalize_search_results_native(
    value: &NativeValue,
    hits: &[(usize, f64)],
    limits: NativeLimits,
    text_limits: TextLimits,
    d: Instant,
) -> Result<NativeValue> {
    timely(d)?;
    let mut normalizer = Normalizer::new(value, limits, text_limits, d);
    let mut output = Vec::new();
    for &(record, score) in hits {
        ensure!(
            output.len() < limits.max_links,
            "explorer_normalize_link_limit"
        );
        let node = normalizer.node_normalized(record)?;
        output.push(normalizer.search_result(node, score)?);
    }
    let root = normalizer.output.add(N::List(output))?;
    normalizer.finish(root)
}

/// Derive one original node view from complete, separate node fields. The
/// result is a temporary List, not another graph or persisted authority.
pub fn project_native_node_records(
    value: &NativeValue,
    nodes: &[usize],
    mode: NativeNodeProjection,
    limits: NativeLimits,
    text_limits: TextLimits,
    d: Instant,
) -> Result<NativeValue> {
    timely(d)?;
    let mut normalizer = Normalizer::new(value, limits, text_limits, d);
    let mut output = Vec::new();
    for &node in nodes {
        ensure!(
            output.len() < limits.max_links,
            "explorer_normalize_link_limit"
        );
        output.push(normalizer.project_state_node(node, mode)?);
    }
    let root = normalizer.output.add(N::List(output))?;
    normalizer.finish(root)
}

/// Initialization uses ContextNode.to_dict followed by GraphSession.normalize_node.
/// Both steps share the single cumulative operation and output allowance.
pub(crate) fn normalize_native_state_nodes(
    value: &NativeValue,
    nodes: &[usize],
    limits: NativeLimits,
    text_limits: TextLimits,
    d: Instant,
) -> Result<NativeValue> {
    timely(d)?;
    let mut normalizer = Normalizer::new(value, limits, text_limits, d);
    let mut output = Vec::new();
    for &node in nodes {
        ensure!(
            output.len() < limits.max_links,
            "explorer_normalize_link_limit"
        );
        output.push(normalizer.state_normalized_node(node, NativeNodeProjection::ToDict)?);
    }
    let root = normalizer.output.add(N::List(output))?;
    normalizer.finish(root)
}

/// Search uses ContextGraph.find_node followed by the SAME normalize_node core.
/// In particular, it excludes independent validity attributes from this view.
pub(crate) fn normalize_native_state_search_results(
    value: &NativeValue,
    hits: &[(usize, f64)],
    limits: NativeLimits,
    text_limits: TextLimits,
    d: Instant,
) -> Result<NativeValue> {
    timely(d)?;
    let mut normalizer = Normalizer::new(value, limits, text_limits, d);
    let mut output = Vec::new();
    for &(record, score) in hits {
        ensure!(
            output.len() < limits.max_links,
            "explorer_normalize_link_limit"
        );
        let node = normalizer.state_normalized_node(record, NativeNodeProjection::FindNode)?;
        output.push(normalizer.search_result(node, score)?);
    }
    let root = normalizer.output.add(N::List(output))?;
    normalizer.finish(root)
}
