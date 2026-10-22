//! Typed adapters for the existing derived index. Text and original floating
//! values remain typed; postings, ranking and LRU still have one implementation.
use super::*;
use semantica_json::{
    native_operations::{self as ops, OperationError, TextLimits},
    native_value::{NativeNode as N, NativeValue},
};
use std::cell::Cell;

#[derive(Clone, Debug, PartialEq)]
pub struct NativeSearchMatches {
    pub matches: Vec<(Vec<u32>, f64)>,
    pub cache_hit: bool,
    pub path: &'static str,
    pub candidates: usize,
}

fn normalize_points(text: &[u32], limit: usize, d: Instant) -> Result<Text> {
    timely(d)?;
    let lower = super::super::explorer_search_unicode::lower_points(ops::strip(text), limit)?;
    let mut out = Vec::new();
    let mut space = false;
    for cp in lower {
        if char::from_u32(cp).is_some_and(whitespace) {
            space = !out.is_empty();
        } else {
            if space {
                out.push(32);
                space = false;
            }
            out.push(cp);
        }
    }
    timely(d)?;
    Ok(Text(out))
}

/// Native restore checks relations between retained derived fields, using the
/// same normalization and token owners as document construction.
pub(super) fn validate_document(doc: &Document, d: Instant) -> Result<()> {
    timely(d)?;
    ensure!(
        ops::strip(&doc.id.0) == doc.id.0.as_slice(),
        "explorer_native_document_identity_invalid"
    );
    // The pinned lowercase data expands one codepoint by at most two. A factor
    // of three leaves a bounded working buffer even before whitespace collapse.
    let limit = doc
        .id
        .0
        .len()
        .checked_mul(3)
        .context("explorer_native_text_budget")?;
    ensure!(
        normalize_points(&doc.id.0, limit, d)? == doc.normalized_id,
        "explorer_native_document_normalization_invalid"
    );
    ensure!(
        tokens_points(&doc.primary.0) == doc.tokens,
        "explorer_native_document_tokens_invalid"
    );
    timely(d)
}

// Limits account for work in this call, including repeated representations and
// joined text. Traversal is lazy so a fragment cap does not inspect unused tails.
struct Input<'a> {
    value: &'a NativeValue,
    limits: TextLimits,
    visits: Cell<usize>,
    points: Cell<usize>,
    deadline: Instant,
}
impl<'a> Input<'a> {
    fn new(value: &'a NativeValue, limits: TextLimits, deadline: Instant) -> Self {
        Self {
            value,
            limits,
            visits: Cell::new(0),
            points: Cell::new(0),
            deadline,
        }
    }
    fn visit(&self, depth: usize) -> Result<()> {
        timely(self.deadline)?;
        ensure!(
            depth <= self.limits.max_depth,
            "explorer_native_depth_budget"
        );
        let visits = self
            .visits
            .get()
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_visits)
            .context("explorer_native_visit_budget")?;
        self.visits.set(visits);
        Ok(())
    }
    fn points(&self, count: usize) -> Result<()> {
        let used = self
            .points
            .get()
            .checked_add(count)
            .filter(|n| *n <= self.limits.max_codepoints)
            .context("explorer_native_text_budget")?;
        self.points.set(used);
        Ok(())
    }
    fn node(&self, id: usize) -> Result<&'a N> {
        self.value
            .nodes()
            .get(id)
            .context("explorer_native_reference")
    }
    fn is_key(&self, id: usize, key: &str) -> bool {
        matches!(self.value.nodes().get(id),Some(N::Text(text)) if text.iter().copied().eq(key.chars().map(u32::from)))
    }
    fn mapping(&self, id: usize, error: &'static str) -> Result<&'a [(usize, usize)]> {
        self.visit(0)?;
        match self.node(id)? {
            N::Dict(pairs) => Ok(pairs),
            _ => anyhow::bail!(error),
        }
    }
    fn get(&self, pairs: &[(usize, usize)], key: &str) -> Result<Option<usize>> {
        let mut found = None;
        for &(k, v) in pairs {
            self.visit(0)?;
            if self.is_key(k, key) {
                ensure!(found.is_none(), "explorer_native_duplicate_mapping_key");
                found = Some(v);
            }
        }
        Ok(found)
    }
    fn truth(&self, id: usize) -> Result<bool> {
        self.visit(0)?;
        Ok(ops::truth(self.value, id)?)
    }
    fn string(&self, id: usize) -> Result<Text> {
        self.string_at(id, 0)
    }
    fn string_at(&self, id: usize, base_depth: usize) -> Result<Text> {
        // Reserve visits with a lazy depth-first walk before the formatter can
        // expand shared references. No pending vector proportional to fan-out.
        let before = self.visits.get();
        let mut stack = vec![(id, 0usize, base_depth)];
        while let Some((current, next, depth)) = stack.pop() {
            if next == 0 {
                self.visit(depth)?;
            }
            let child = match self.node(current)? {
                N::List(items) | N::Tuple(items) | N::Set(items) | N::FrozenSet(items) => {
                    items.get(next).copied()
                }
                N::Dict(pairs) => pairs
                    .get(next / 2)
                    .map(|&(k, v)| if next % 2 == 0 { k } else { v }),
                _ => None,
            };
            if let Some(child) = child {
                stack.push((current, next + 1, depth));
                stack.push((child, 0, depth + 1));
            }
        }
        let max_depth = self
            .limits
            .max_depth
            .checked_sub(base_depth)
            .context("explorer_native_depth_budget")?;
        let limits = TextLimits {
            max_codepoints: self.limits.max_codepoints - self.points.get(),
            max_visits: self.visits.get() - before,
            max_depth,
        };
        let text = ops::python_text(self.value, id, false, limits)?;
        self.points(text.len())?;
        timely(self.deadline)?;
        Ok(Text(text))
    }
    fn normalize_text(&self, text: &[u32]) -> Result<Text> {
        let out = normalize_points(
            text,
            self.limits.max_codepoints - self.points.get(),
            self.deadline,
        )?;
        self.points(out.0.len())?;
        Ok(out)
    }
    fn normalize(&self, id: usize) -> Result<Text> {
        self.normalize_at(id, 0)
    }
    fn normalize_at(&self, id: usize, depth: usize) -> Result<Text> {
        if matches!(self.node(id)?, N::None) {
            self.visit(depth)?;
            return Ok(Text::default());
        }
        self.normalize_text(&self.string_at(id, depth)?.0)
    }
    fn fragments(&self, id: usize, out: &mut Vec<Text>, cap: usize) -> Result<()> {
        let mut stack = vec![(id, 0usize, 0usize)];
        while out.len() < cap {
            let Some((current, next, depth)) = stack.pop() else {
                break;
            };
            if next == 0 {
                self.visit(depth)?;
            }
            let child = match self.node(current)? {
                N::Dict(pairs) => pairs.get(next).map(|&(_, v)| v),
                N::List(items) | N::Tuple(items) | N::Set(items) => items.get(next).copied(),
                N::None => continue,
                _ => {
                    let text = self.normalize_at(current, depth)?;
                    if !text.is_empty() {
                        out.push(text);
                    }
                    continue;
                }
            };
            if let Some(child) = child {
                stack.push((current, next + 1, depth));
                stack.push((child, 0, depth + 1));
            }
        }
        Ok(())
    }
    fn number(&self, id: usize) -> Result<Option<f64>> {
        self.visit(0)?;
        if matches!(self.node(id)?, N::None) || matches!(self.node(id)?,N::Text(s) if s.is_empty())
        {
            return Ok(None);
        }
        // Bound copied numeric text before the shared Python float conversion.
        match self.node(id)? {
            N::Text(s) => self.points(s.len())?,
            N::Integer(s) => self.points(s.len())?,
            _ => {}
        }
        match ops::builtin_float(self.value, id) {
            Ok(number) => Ok(Some(number)),
            Err(OperationError::FloatInvalid | OperationError::FloatType) => Ok(None),
            Err(OperationError::FloatOverflow) => {
                anyhow::bail!("explorer_search_integer_float_overflow")
            }
            Err(error) => Err(error.into()),
        }
    }
    fn tags(&self, id: usize, keep_empty_string: bool) -> Result<Vec<Text>> {
        let mut tags = Vec::new();
        if matches!(self.node(id)?, N::Text(_)) {
            let text = self.normalize(id)?;
            if keep_empty_string || !text.is_empty() {
                tags.push(text);
            }
        } else {
            let mut push = |item| -> Result<()> {
                self.visit(0)?;
                let text = self.normalize(item)?;
                if !text.is_empty() {
                    tags.push(text);
                }
                Ok(())
            };
            match self.node(id)? {
                N::List(items) | N::Tuple(items) | N::Set(items) | N::FrozenSet(items) => {
                    for &item in items {
                        push(item)?;
                    }
                }
                N::Dict(pairs) => {
                    for &(key, _) in pairs {
                        push(key)?;
                    }
                }
                _ => anyhow::bail!("explorer_search_tags_not_iterable"),
            }
        }
        Ok(dedup_text(tags))
    }
    fn join(&self, texts: &[Text]) -> Result<Text> {
        let count = texts
            .iter()
            .try_fold(0usize, |n, t| {
                n.checked_add(t.0.len())
                    .context("explorer_native_text_budget")
            })?
            .checked_add(texts.len().saturating_sub(1))
            .context("explorer_native_text_budget")?;
        self.points(count)?;
        let mut result = Vec::with_capacity(count);
        for (i, text) in texts.iter().enumerate() {
            if i > 0 {
                result.push(32);
            }
            result.extend_from_slice(&text.0);
        }
        Ok(Text(result))
    }
}
fn dedup_text(items: Vec<Text>) -> Vec<Text> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}
fn original_native_field_error(error: &anyhow::Error) -> bool {
    original_field_error(error)
        || error
            .downcast_ref::<OperationError>()
            .is_some_and(|error| matches!(error, OperationError::IntegerTextLimit))
}
fn node_identity(input: &Input<'_>, node: usize) -> Result<Text> {
    let pairs = input.mapping(node, "explorer_native_node_mapping_required")?;
    match input.get(pairs, "id")? {
        Some(id) => Ok(Text(ops::strip(&input.string(id)?.0).to_vec())),
        None => Ok(Text::default()),
    }
}
fn build_document(input: &Input<'_>, node: usize, id: Text) -> Result<Document> {
    let pairs = input.mapping(node, "explorer_native_node_mapping_required")?;
    let node_type = match input.get(pairs, "type")? {
        Some(value) => input.string(value)?,
        None => Text::from("entity"),
    };
    let empty = &[][..];
    let properties = match input.get(pairs, "properties")? {
        Some(value) if input.truth(value)? => {
            input.mapping(value, "explorer_search_properties_invalid")?
        }
        _ => empty,
    };
    let normalized_id = input.normalize_text(&id.0)?;
    let mut primary = Vec::new();
    if !normalized_id.is_empty() {
        primary.push(normalized_id.clone());
    }
    if let Some(content) = input.get(pairs, "content")? {
        let normalized = input.normalize(content)?;
        if !normalized.is_empty() {
            primary.push(normalized);
        }
    }
    for key in ALIASES {
        if let Some(value) = input.get(properties, key)? {
            input.fragments(value, &mut primary, 32)?;
        }
    }
    let primary = dedup_text(primary);
    let primary_text = input.join(&primary)?;
    let mut secondary = Vec::new();
    for &(key, value) in properties {
        input.visit(0)?;
        if ALIASES.iter().any(|name| input.is_key(key, name))
            || input.is_key(key, "valid_from")
            || input.is_key(key, "valid_until")
        {
            continue;
        }
        input.fragments(value, &mut secondary, 48)?;
        if secondary.len() >= 48 {
            break;
        }
    }
    let secondary_text = input.join(&dedup_text(secondary))?;
    let confidence = input
        .get(properties, "confidence")?
        .map(|id| input.number(id))
        .transpose()?
        .flatten();
    let tags = match input.get(properties, "tags")? {
        Some(value) if input.truth(value)? => input.tags(value, false)?,
        _ => vec![],
    };
    timely(input.deadline)?;
    Ok(Document {
        id,
        normalized_id,
        node_type,
        exact: primary.into_iter().collect(),
        tokens: tokens_points(&primary_text.0),
        primary: primary_text,
        secondary: secondary_text,
        confidence,
        tags,
    })
}
fn passes(input: &Input<'_>, doc: &Document, filters: &[(usize, usize)]) -> Result<bool> {
    let first = input.get(filters, "type")?;
    let filter_type = match first {
        Some(id) if input.truth(id)? => Some(id),
        _ => input.get(filters, "node_type")?,
    };
    if let Some(id) = filter_type {
        if input.truth(id)? && doc.node_type != input.string(id)? {
            return Ok(false);
        }
    }
    if let Some(min) = input
        .get(filters, "min_confidence")?
        .map(|id| input.number(id))
        .transpose()?
        .flatten()
    {
        if doc.confidence.is_none_or(|confidence| confidence < min) {
            return Ok(false);
        }
    }
    if let Some(tags) = input.get(filters, "tags")? {
        if input.truth(tags)?
            && input
                .tags(tags, true)?
                .iter()
                .any(|tag| !doc.tags.contains(tag))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

// An explicit native internal cache key. Original Python tuple keys are not
// claimed byte-identical. The value distinctions and comparisons are retained.
fn cache_key(
    input: &Input<'_>,
    query: &Text,
    limit: usize,
    filters: &[(usize, usize)],
) -> Result<String> {
    let mut keys = Vec::new();
    let mut seen = BTreeSet::new();
    for &(key, value) in filters {
        input.visit(0)?;
        let N::Text(points) = input.node(key)? else {
            anyhow::bail!("explorer_native_filter_key_type_pending")
        };
        input.points(points.len())?;
        ensure!(
            seen.insert(points.clone()),
            "explorer_native_duplicate_mapping_key"
        );
        keys.push((points, value));
    }
    keys.sort_by(|a, b| a.0.cmp(b.0));
    let mut serialized = Vec::new();
    for (key, id) in keys {
        let value = match input.node(id)? {
            N::List(items) | N::Tuple(items) | N::Set(items) => {
                let mut values = Vec::new();
                for &item in items {
                    values.push(input.string(item)?.0);
                }
                values.sort();
                json!({"tuple":values})
            }
            _ => json!({"string":input.string(id)?.0}),
        };
        serialized.push(json!([key, value]));
    }
    timely(input.deadline)?;
    Ok(serde_json::to_string(&json!([query.0, limit, serialized]))?)
}

/// Structural restore guard for this internal native key profile. Source and
/// graph binding remain the caller's responsibility; cached stale IDs are valid.
pub(super) fn validate_cache_key(key: &str) -> Result<usize> {
    fn points(value: &Value) -> Result<Vec<u32>> {
        value
            .as_array()
            .context("explorer_native_cache_key_invalid")?
            .iter()
            .map(|cp| {
                cp.as_u64()
                    .filter(|n| *n <= 0x10ffff)
                    .map(|n| n as u32)
                    .context("explorer_native_cache_key_invalid")
            })
            .collect()
    }
    let parsed = semantica_json::strict_json(key.as_bytes())?;
    let outer = parsed
        .as_array()
        .filter(|v| v.len() == 3)
        .context("explorer_native_cache_key_invalid")?;
    ensure!(
        !points(&outer[0])?.is_empty(),
        "explorer_native_cache_key_invalid"
    );
    let limit = outer[1]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .context("explorer_native_cache_key_invalid")?;
    let entries = outer[2]
        .as_array()
        .context("explorer_native_cache_key_invalid")?;
    let mut previous: Option<Vec<u32>> = None;
    for entry in entries {
        let pair = entry
            .as_array()
            .filter(|v| v.len() == 2)
            .context("explorer_native_cache_key_invalid")?;
        let name = points(&pair[0])?;
        ensure!(
            previous.as_ref().is_none_or(|p| p < &name),
            "explorer_native_cache_key_invalid"
        );
        previous = Some(name);
        let object = pair[1]
            .as_object()
            .filter(|v| v.len() == 1)
            .context("explorer_native_cache_key_invalid")?;
        if let Some(value) = object.get("string") {
            points(value)?;
        } else if let Some(values) = object.get("tuple") {
            let mut previous: Option<Vec<u32>> = None;
            for value in values
                .as_array()
                .context("explorer_native_cache_key_invalid")?
            {
                let text = points(value)?;
                ensure!(
                    previous.as_ref().is_none_or(|p| p <= &text),
                    "explorer_native_cache_key_invalid"
                );
                previous = Some(text);
            }
        } else {
            anyhow::bail!("explorer_native_cache_key_invalid");
        }
    }
    ensure!(
        serde_json::to_string(&parsed)? == key,
        "explorer_native_cache_key_invalid"
    );
    Ok(limit)
}

impl GraphSearchIndex {
    pub fn new_native() -> Self {
        Self {
            native_mode: true,
            ..Self::default()
        }
    }

    /// Original document-field failures retain a removed previous document and
    /// its old cached hits. Resource refusals discard this entire staged change.
    pub fn upsert_native_original_partial(
        &mut self,
        value: &NativeValue,
        node: usize,
        limits: TextLimits,
        d: Instant,
    ) -> Result<()> {
        ensure!(
            self.native_mode,
            "explorer_native_index_requires_native_profile"
        );
        timely(d)?;
        let input = Input::new(value, limits, d);
        let id = node_identity(&input, node)?;
        if id.is_empty() {
            return timely(d);
        }
        let mut next = self.clone();
        let cache = next.cache.clone();
        next.remove_text(&id, d)?;
        next.cache = cache;
        let outcome = build_document(&input, node, id).and_then(|doc| next.install(doc, d));
        if outcome.is_ok() || outcome.as_ref().is_err_and(original_native_field_error) {
            timely(d)?;
            *self = next;
        }
        outcome
    }
    pub fn rebuild_native_original_partial(
        &mut self,
        value: &NativeValue,
        nodes: &[usize],
        limits: TextLimits,
        d: Instant,
    ) -> Result<()> {
        ensure!(
            self.native_mode,
            "explorer_native_index_requires_native_profile"
        );
        timely(d)?;
        let input = Input::new(value, limits, d);
        let mut next = Self::new_native();
        for &node in nodes {
            let outcome = (|| {
                let id = node_identity(&input, node)?;
                if id.is_empty() {
                    return timely(d);
                }
                next.remove_text(&id, d)?;
                let doc = build_document(&input, node, id)?;
                next.install(doc, d)
            })();
            if let Err(error) = outcome {
                if original_native_field_error(&error) {
                    timely(d)?;
                    *self = next;
                }
                return Err(error);
            }
        }
        timely(d)?;
        *self = next;
        Ok(())
    }
    pub fn remove_native(&mut self, id: &[u32], d: Instant) -> Result<()> {
        ensure!(
            self.native_mode,
            "explorer_native_index_requires_native_profile"
        );
        timely(d)?;
        ensure!(
            id.iter().all(|cp| *cp <= 0x10ffff),
            "explorer_native_codepoint_invalid"
        );
        let mut next = self.clone();
        next.remove_text(&Text(id.to_vec()), d)?;
        timely(d)?;
        *self = next;
        Ok(())
    }
    pub fn search_native(
        &mut self,
        value: &NativeValue,
        query: usize,
        limit: usize,
        filters: usize,
        limits: TextLimits,
        d: Instant,
    ) -> Result<NativeSearchMatches> {
        ensure!(
            self.native_mode,
            "explorer_native_index_requires_native_profile"
        );
        timely(d)?;
        let input = Input::new(value, limits, d);
        let query = input.normalize(query)?;
        if query.is_empty() {
            return Ok(NativeSearchMatches {
                matches: vec![],
                cache_hit: false,
                path: "empty",
                candidates: 0,
            });
        }
        let filters = if input.truth(filters)? {
            input.mapping(filters, "explorer_search_filters_invalid")?
        } else {
            &[]
        };
        let key = cache_key(&input, &query, limit, filters)?;
        let mut next = self.clone();
        let result = next.search_core(query, limit, key, |doc| passes(&input, doc, filters), d)?;
        timely(d)?;
        *self = next;
        Ok(NativeSearchMatches {
            matches: result
                .matches
                .into_iter()
                .map(|(id, score)| (id.0, score))
                .collect(),
            cache_hit: result.cache_hit,
            path: result.path,
            candidates: result.candidates,
        })
    }
}
