//! Derived Explorer index. It owns no graph, source authority, or service budget.
use super::explorer_index_text::Text;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};
const ALIASES: &[&str] = &[
    "label",
    "name",
    "title",
    "pref_label",
    "preferred_label",
    "prefLabel",
    "aliases",
    "alias",
    "synonyms",
    "synonym",
    "symbol",
    "display_name",
    "displayName",
    "text",
    "content",
];
fn timely(d: Instant) -> Result<()> {
    ensure!(Instant::now() < d, "explorer_search_deadline_exceeded");
    Ok(())
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(_) => !semantica_json::python_equal(v, &json!(0)).unwrap_or(false),
    }
}
fn text(v: &Value) -> Result<String> {
    Ok(semantica_json::python_str(v)?)
}
fn whitespace(c: char) -> bool {
    let mut b = [0; 4];
    semantica_json::python_strip(c.encode_utf8(&mut b)).is_empty()
}
fn normalize(v: &Value) -> Result<String> {
    if v.is_null() {
        return Ok(String::new());
    }
    let s = super::explorer_search_unicode::lower(&text(v)?);
    Ok(s.split(whitespace)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" "))
}
fn tokens(s: &str) -> BTreeSet<String> {
    s.split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
fn fragments(v: &Value, out: &mut Vec<String>, limit: usize, d: Instant) -> Result<()> {
    timely(d)?;
    if v.is_null() || out.len() >= limit {
        return Ok(());
    }
    match v {
        Value::Object(o) => {
            for v in o.values() {
                fragments(v, out, limit, d)?;
                if out.len() >= limit {
                    break;
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                fragments(v, out, limit, d)?;
                if out.len() >= limit {
                    break;
                }
            }
        }
        _ => {
            let s = normalize(v)?;
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    Ok(())
}
fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}
fn number(v: &Value) -> Result<Option<f64>> {
    if v.is_null() || v == "" {
        return Ok(None);
    }
    match semantica_json::python_builtin_float(v) {
        Ok(n) => Ok(Some(n)),
        Err(semantica_json::PythonFloatError::IntegerOverflow) => {
            anyhow::bail!("explorer_search_integer_float_overflow")
        }
        Err(_) => Ok(None),
    }
}
fn iter_values(v: &Value) -> Result<Vec<Value>> {
    match v {
        Value::Array(a) => Ok(a.clone()),
        Value::Object(o) => Ok(o.keys().map(|s| json!(s)).collect()),
        Value::String(s) => Ok(s.chars().map(|c| json!(c.to_string())).collect()),
        _ => anyhow::bail!("explorer_search_tags_not_iterable"),
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Document {
    id: Text,
    normalized_id: Text,
    node_type: Text,
    exact: BTreeSet<Text>,
    tokens: BTreeSet<String>,
    primary: Text,
    secondary: Text,
    confidence: Option<f64>,
    tags: Vec<Text>,
}
impl Document {
    fn build(n: &Value, d: Instant) -> Result<Self> {
        let id = text(n.get("id").unwrap_or(&json!("")))?
            .trim_matches(whitespace)
            .to_owned();
        let node_type = text(n.get("type").unwrap_or(&json!("entity")))?;
        let empty = json!({});
        let p = n
            .get("properties")
            .filter(|v| truth(v))
            .unwrap_or(&empty)
            .as_object()
            .context("explorer_search_properties_invalid")?;
        let mut primary = vec![];
        for v in [json!(id), n.get("content").cloned().unwrap_or(json!(""))] {
            let s = normalize(&v)?;
            if !s.is_empty() {
                primary.push(s);
            }
        }
        for k in ALIASES {
            if let Some(v) = p.get(*k) {
                fragments(v, &mut primary, 32, d)?;
            }
        }
        let primary = dedup(primary);
        let primary_text = primary.join(" ");
        let mut secondary = vec![];
        for (k, v) in p {
            if ALIASES.contains(&k.as_str()) || ["valid_from", "valid_until"].contains(&k.as_str())
            {
                continue;
            }
            fragments(v, &mut secondary, 48, d)?;
            if secondary.len() >= 48 {
                break;
            }
        }
        let confidence = p.get("confidence").map(number).transpose()?.flatten();
        let tags = match p.get("tags").filter(|v| truth(v)) {
            None => vec![],
            Some(Value::String(s)) => vec![normalize(&json!(s))?],
            Some(v) => iter_values(v)?
                .iter()
                .map(normalize)
                .collect::<Result<Vec<_>>>()?,
        };
        timely(d)?;
        Ok(Self {
            id: id.clone().into(),
            normalized_id: normalize(&json!(id))?.into(),
            node_type: node_type.into(),
            exact: primary.into_iter().map(Text::from).collect(),
            tokens: tokens(&primary_text),
            primary: primary_text.into(),
            secondary: dedup(secondary).join(" ").into(),
            confidence,
            tags: dedup(tags.into_iter().filter(|s| !s.is_empty()).collect())
                .into_iter()
                .map(Text::from)
                .collect(),
        })
    }
    fn passes(&self, f: &serde_json::Map<String, Value>) -> Result<bool> {
        if let Some(v) = f
            .get("type")
            .filter(|v| truth(v))
            .or_else(|| f.get("node_type").filter(|v| truth(v)))
        {
            if self.node_type != Text::from(text(v)?) {
                return Ok(false);
            }
        }
        if let Some(min) = f.get("min_confidence").map(number).transpose()?.flatten() {
            if self.confidence.is_none_or(|c| c < min) {
                return Ok(false);
            }
        }
        if let Some(v) = f.get("tags").filter(|v| truth(v)) {
            let tags = if let Value::String(_) = v {
                vec![normalize(v)?]
            } else {
                iter_values(v)?
                    .iter()
                    .map(normalize)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect()
            };
            if tags
                .iter()
                .any(|t| !self.tags.contains(&Text::from(t.as_str())))
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn score(&self, q: &Text, ts: &BTreeSet<String>) -> (f64, bool, usize) {
        let mut score: f64 = if q == &self.normalized_id {
            140.0
        } else if self.exact.contains(q) {
            120.0
        } else {
            0.0
        };
        let len = q.0.len().min(24) as f64;
        if !q.is_empty() && self.primary.contains(q) {
            score = score.max(78.0 + len / 10.0);
        } else if !q.is_empty() && self.secondary.contains(q) {
            score = score.max(26.0 + len / 20.0);
        }
        let hits = ts.iter().filter(|t| self.tokens.contains(*t)).count();
        let prefixes = ts
            .iter()
            .filter(|t| {
                !self.tokens.contains(*t)
                    && t.len() >= 2
                    && self.tokens.iter().any(|s| s.starts_with(t.as_str()))
            })
            .count();
        score += hits as f64 * 18.0;
        score += prefixes as f64 * 10.0;
        if ts.len() > 1 && hits > 0 {
            score += hits as f64 * 4.0;
        }
        (
            score,
            q == &self.normalized_id || self.exact.contains(q),
            hits,
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SearchMatches {
    pub matches: Vec<(String, f64)>,
    pub cache_hit: bool,
    pub path: &'static str,
    pub candidates: usize,
}
type Posting = BTreeMap<Text, BTreeSet<Text>>;
#[derive(Default, Clone)]
pub struct GraphSearchIndex {
    native_mode: bool,
    documents: BTreeMap<Text, Document>,
    exact: Posting,
    tokens: Posting,
    prefixes: Posting,
    cache: VecDeque<(String, Vec<(Text, f64)>)>,
}
#[derive(serde::Deserialize)]
struct StoredIndex {
    documents: BTreeMap<Text, Document>,
    exact: Posting,
    tokens: Posting,
    prefixes: Posting,
    cache: VecDeque<(String, Vec<(Text, f64)>)>,
}
fn insert(index: &mut Posting, key: &Text, id: &Text) {
    index.entry(key.clone()).or_default().insert(id.clone());
}
fn remove(index: &mut Posting, key: &Text, id: &Text) {
    if let Some(ids) = index.get_mut(key) {
        ids.remove(id);
        if ids.is_empty() {
            index.remove(key);
        }
    }
}
fn original_field_error(error: &anyhow::Error) -> bool {
    matches!(
        error.to_string().as_str(),
        "explorer_search_integer_float_overflow" | "explorer_search_tags_not_iterable"
    )
}
impl GraphSearchIndex {
    pub(crate) fn deserialize_stored<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let wire = <StoredIndex as serde::Deserialize>::deserialize(deserializer)?;
        let index = Self {
            native_mode: false,
            documents: wire.documents,
            exact: wire.exact,
            tokens: wire.tokens,
            prefixes: wire.prefixes,
            cache: wire.cache,
        };
        index
            .validate_persistable()
            .map_err(serde::de::Error::custom)?;
        Ok(index)
    }

    /// Logical JSON payload accounting; this is not an allocator/RSS measurement.
    pub(crate) fn payload_bytes(&self, limit: usize) -> Result<usize> {
        struct Count {
            used: usize,
            limit: usize,
        }
        impl std::io::Write for Count {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.used = self
                    .used
                    .checked_add(b.len())
                    .filter(|n| *n <= self.limit)
                    .ok_or_else(|| std::io::Error::other("explorer_search_cache_budget"))?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count { used: 0, limit };
        serde_json::to_writer(&mut count, self)?;
        Ok(count.used)
    }
    pub(crate) fn validate_persistable(&self) -> Result<()> {
        ensure!(
            self.documents
                .values()
                .all(|doc| doc.confidence.is_none_or(f64::is_finite))
                && self
                    .cache
                    .iter()
                    .all(|(_, hits)| hits.iter().all(|(_, score)| score.is_finite())),
            "explorer_mutation_nonfinite_index_pending"
        );
        self.validate_structure()
    }
    fn validate_structure(&self) -> Result<()> {
        self.validate_structure_before(None)
    }
    fn validate_structure_before(&self, deadline: Option<Instant>) -> Result<()> {
        let check_time = || -> Result<()> {
            if let Some(d) = deadline {
                timely(d)?;
            }
            Ok(())
        };
        check_time()?;
        ensure!(
            self.cache.len() <= 128,
            "explorer_mutation_index_cache_invalid"
        );
        // Preserve document/token validation before posting comparison. No expected
        // posting map is materialized: long IDs stay borrowed throughout validation.
        for (id, doc) in &self.documents {
            check_time()?;
            ensure!(
                id == &doc.id && !id.is_empty(),
                "explorer_mutation_index_document_invalid"
            );
            for term in &doc.tokens {
                check_time()?;
                ensure!(
                    !term.is_empty()
                        && term
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
                    "explorer_mutation_index_token_invalid"
                );
            }
        }
        // Every required document-to-posting edge must be present.
        for (id, doc) in &self.documents {
            check_time()?;
            for term in &doc.exact {
                check_time()?;
                ensure!(
                    self.exact.get(term).is_some_and(|ids| ids.contains(id)),
                    "explorer_mutation_index_postings_invalid"
                );
            }
            for term in &doc.tokens {
                check_time()?;
                ensure!(
                    self.tokens
                        .get(&Text::from(term.as_str()))
                        .is_some_and(|ids| ids.contains(id)),
                    "explorer_mutation_index_postings_invalid"
                );
                for len in 2..=term.len().min(12) {
                    check_time()?;
                    ensure!(
                        self.prefixes
                            .get(&Text::from(&term[..len]))
                            .is_some_and(|ids| ids.contains(id)),
                        "explorer_mutation_index_postings_invalid"
                    );
                }
            }
        }
        // Every stored posting edge must be justified by the corresponding document;
        // explicit empty buckets are invalid even though they contain no edges.
        for (term, ids) in &self.exact {
            check_time()?;
            ensure!(!ids.is_empty(), "explorer_mutation_index_postings_invalid");
            for id in ids {
                check_time()?;
                ensure!(
                    self.documents
                        .get(id)
                        .is_some_and(|doc| doc.exact.contains(term)),
                    "explorer_mutation_index_postings_invalid"
                );
            }
        }
        for (term, ids) in &self.tokens {
            check_time()?;
            ensure!(!ids.is_empty(), "explorer_mutation_index_postings_invalid");
            let token = term
                .utf8()
                .context("explorer_mutation_index_postings_invalid")?;
            for id in ids {
                check_time()?;
                ensure!(
                    self.documents
                        .get(id)
                        .is_some_and(|doc| doc.tokens.contains(&token)),
                    "explorer_mutation_index_postings_invalid"
                );
            }
        }
        for (term, ids) in &self.prefixes {
            check_time()?;
            ensure!(
                !ids.is_empty()
                    && (2..=12).contains(&term.0.len())
                    && term
                        .0
                        .iter()
                        .all(|cp| (97..=122).contains(cp) || (48..=57).contains(cp)),
                "explorer_mutation_index_postings_invalid"
            );
            let prefix = term
                .utf8()
                .context("explorer_mutation_index_postings_invalid")?;
            for id in ids {
                check_time()?;
                let doc = self
                    .documents
                    .get(id)
                    .context("explorer_mutation_index_postings_invalid")?;
                // The first token at or beyond this prefix is enough in a sorted set.
                // The temporary bound is at most twelve ASCII bytes; no ID is cloned.
                ensure!(
                    doc.tokens
                        .range(prefix.clone()..)
                        .next()
                        .is_some_and(|token| token.starts_with(&prefix)),
                    "explorer_mutation_index_postings_invalid"
                );
            }
        }
        check_time()
    }
    pub fn clear(&mut self) {
        let mode = self.native_mode;
        *self = Self::default();
        self.native_mode = mode;
    }
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
    pub fn len(&self) -> usize {
        self.documents.len()
    }
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
    pub fn rebuild(&mut self, nodes: &[Value], d: Instant) -> Result<()> {
        ensure!(
            !self.native_mode,
            "explorer_native_index_requires_native_api"
        );
        let mut next = Self::default();
        for n in nodes {
            next.upsert(n, d)?;
        }
        timely(d)?;
        *self = next;
        Ok(())
    }
    /// Explicit original Explorer behavior: a field conversion failure retains
    /// completed index writes. Native deadline failures leave this index unchanged.
    /// This is a derived-index primitive, not graph publication or authorization.
    pub fn rebuild_original_partial(&mut self, nodes: &[Value], d: Instant) -> Result<()> {
        ensure!(
            !self.native_mode,
            "explorer_native_index_requires_native_api"
        );
        timely(d)?;
        let mut next = Self::default();
        for node in nodes {
            if let Err(error) = next.upsert_original_partial(node, d) {
                if original_field_error(&error) {
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
    /// Original upsert removes the previous document before converting fields.
    /// On a field error its query cache survives, including stale cached hits.
    /// Keep the existing atomic upsert API unchanged for native callers.
    pub fn upsert_original_partial(&mut self, node: &Value, d: Instant) -> Result<()> {
        ensure!(
            !self.native_mode,
            "explorer_native_index_requires_native_api"
        );
        timely(d)?;
        let id = text(node.get("id").unwrap_or(&json!("")))?
            .trim_matches(whitespace)
            .to_owned();
        if id.is_empty() {
            return timely(d);
        }
        let mut next = self.clone();
        let cache = next.cache.clone();
        next.remove(&id, d)?;
        next.cache = cache;
        let outcome = next.upsert(node, d);
        if outcome.is_ok() || outcome.as_ref().is_err_and(original_field_error) {
            timely(d)?;
            *self = next;
        }
        outcome
    }
    pub fn upsert(&mut self, node: &Value, d: Instant) -> Result<()> {
        ensure!(
            !self.native_mode,
            "explorer_native_index_requires_native_api"
        );
        timely(d)?;
        let id = text(node.get("id").unwrap_or(&json!("")))?
            .trim_matches(whitespace)
            .to_owned();
        if id.is_empty() {
            return timely(d);
        }
        let doc = Document::build(node, d)?;
        self.remove(&id, d)?;
        self.install(doc, d)
    }
    fn install(&mut self, doc: Document, d: Instant) -> Result<()> {
        let id = doc.id.clone();
        for t in &doc.exact {
            insert(&mut self.exact, t, &id);
        }
        for t in &doc.tokens {
            insert(&mut self.tokens, &Text::from(t.as_str()), &id);
            for len in 2..=t.len().min(12) {
                insert(&mut self.prefixes, &Text::from(&t[..len]), &id);
            }
        }
        self.documents.insert(id, doc);
        self.cache.clear();
        timely(d)
    }
    pub fn remove(&mut self, id: &str, d: Instant) -> Result<()> {
        ensure!(
            !self.native_mode,
            "explorer_native_index_requires_native_api"
        );
        self.remove_text(&Text::from(id), d)
    }
    fn remove_text(&mut self, id: &Text, d: Instant) -> Result<()> {
        timely(d)?;
        if let Some(doc) = self.documents.remove(id) {
            for t in &doc.exact {
                remove(&mut self.exact, t, id);
            }
            for t in &doc.tokens {
                remove(&mut self.tokens, &Text::from(t.as_str()), id);
                for len in 2..=t.len().min(12) {
                    remove(&mut self.prefixes, &Text::from(&t[..len]), id);
                }
            }
            self.cache.clear();
        }
        timely(d)
    }
    pub fn search(
        &mut self,
        query: &str,
        limit: usize,
        filters: &Value,
        d: Instant,
    ) -> Result<SearchMatches> {
        ensure!(
            !self.native_mode,
            "explorer_native_index_requires_native_api"
        );
        timely(d)?;
        let q = normalize(&json!(query))?;
        if q.is_empty() {
            return Ok(SearchMatches {
                matches: vec![],
                cache_hit: false,
                path: "empty",
                candidates: 0,
            });
        }
        let f = filters
            .as_object()
            .context("explorer_search_filters_invalid")?;
        let mut keys: Vec<_> = f.keys().collect();
        keys.sort();
        let mut keyed = vec![];
        for k in keys {
            let v = &f[k];
            let value = if let Value::Array(a) = v {
                let mut a = a.iter().map(text).collect::<Result<Vec<_>>>()?;
                a.sort();
                json!({"tuple":a})
            } else {
                json!({"string":text(v)?})
            };
            keyed.push(json!([k, value]));
        }
        let key = serde_json::to_string(&json!([q, limit, keyed]))?;
        let native = self.search_core(Text::from(q), limit, key, |doc| doc.passes(f), d)?;
        Ok(SearchMatches {
            matches: native
                .matches
                .into_iter()
                .map(|(id, score)| Ok((id.utf8()?, score)))
                .collect::<Result<_>>()?,
            cache_hit: native.cache_hit,
            path: native.path,
            candidates: native.candidates,
        })
    }
    fn search_core<F: Fn(&Document) -> Result<bool>>(
        &mut self,
        q: Text,
        limit: usize,
        key: String,
        passes: F,
        d: Instant,
    ) -> Result<CoreMatches> {
        if let Some(i) = self.cache.iter().position(|(k, _)| k == &key) {
            let pair = self.cache.remove(i).unwrap();
            let matches = pair.1.clone();
            self.cache.push_back(pair);
            timely(d)?;
            return Ok(CoreMatches {
                candidates: matches.len(),
                matches,
                cache_hit: true,
                path: "cache",
            });
        }
        let ts = tokens_points(&q.0);
        let mut ids = self.exact.get(&q).cloned().unwrap_or_default();
        let mut token_sets = vec![];
        for t in &ts {
            if let Some(v) = self.tokens.get(&Text::from(t.as_str())) {
                if !v.is_empty() {
                    token_sets.push(v);
                }
            }
        }
        if let Some(first) = token_sets.first() {
            let mut intersection = (*first).clone();
            for set in &token_sets[1..] {
                intersection.retain(|id| set.contains(id));
            }
            if intersection.is_empty() {
                for set in token_sets {
                    ids.extend(set.iter().cloned());
                }
            } else {
                ids.extend(intersection);
            }
        }
        for t in &ts {
            if t.len() >= 2 {
                if let Some(v) = self.prefixes.get(&Text::from(t.as_str())) {
                    ids.extend(v.iter().cloned());
                }
            }
        }
        let mut path = "index";
        if ids.is_empty() {
            path = "secondary_scan";
            let max_hits = limit.saturating_mul(20).max(200);
            for (i, (id, doc)) in self.documents.iter().enumerate() {
                timely(d)?;
                if i >= 12000 || ids.len() >= max_hits {
                    break;
                }
                if doc.primary.contains(&q) || doc.secondary.contains(&q) {
                    ids.insert(id.clone());
                }
            }
        }
        let candidates = ids.len();
        let mut scored = vec![];
        for id in ids {
            timely(d)?;
            let doc = &self.documents[&id];
            if !passes(doc)? {
                continue;
            }
            let (score, exact, hits) = doc.score(&q, &ts);
            if score > 0.0 {
                scored.push((score, exact, hits, doc.id.clone()));
            }
        }
        scored.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| b.3.cmp(&a.3))
        });
        let matches: Vec<_> = scored
            .into_iter()
            .take(limit)
            .map(|(score, _, _, id)| (id, (score * 10000.0).round_ties_even() / 10000.0))
            .collect();
        timely(d)?;
        self.cache.push_back((key, matches.clone()));
        while self.cache.len() > 128 {
            self.cache.pop_front();
        }
        Ok(CoreMatches {
            matches,
            cache_hit: false,
            path,
            candidates,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_complete_scalar_normalization_and_sigma_contexts() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/explorer_search_scalar.json"
        ))
        .unwrap();
        let changes = oracle["changed_normalization"].as_object().unwrap();
        let mut checked = 0;
        for cp in 0..=0x10ffff {
            if let Some(c) = char::from_u32(cp) {
                let s = c.to_string();
                let expected = changes
                    .get(&cp.to_string())
                    .and_then(Value::as_str)
                    .unwrap_or(&s);
                assert_eq!(normalize(&json!(s)).unwrap(), expected, "U+{cp:04X}");
                checked += 1;
            }
        }
        assert_eq!(checked, 1112064);
        for row in oracle["context_cases"].as_array().unwrap() {
            assert_eq!(
                normalize(&row["input"]).unwrap(),
                row["normalized"].as_str().unwrap()
            );
        }
    }
    #[test]
    fn original_float_invalid_and_overflow_are_distinct() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/explorer_search_scalar.json"
        ))
        .unwrap();
        for row in oracle["float_cases"].as_array().unwrap() {
            let actual = number(&row["input"]);
            if row["error"] == "OverflowError" {
                assert!(actual.unwrap_err().to_string().contains("overflow"));
            } else {
                let got = actual.unwrap();
                match row["result"].as_str().unwrap() {
                    "None" => assert!(got.is_none()),
                    "nan" => assert!(got.unwrap().is_nan()),
                    "inf" => assert_eq!(got.unwrap(), f64::INFINITY),
                    text => assert_eq!(got.unwrap(), text.parse::<f64>().unwrap()),
                }
            }
        }
    }
}

/// Text preparation for the typed index path. Shares the existing scalar and
/// pinned lowercase owners; no graph, postings, cache or authority is created.
pub fn normalize_native(
    value: &semantica_json::native_value::NativeValue,
    id: usize,
    limits: semantica_json::native_operations::TextLimits,
    d: Instant,
) -> Result<Vec<u32>> {
    timely(d)?;
    let node = value.nodes().get(id).context("explorer_native_reference")?;
    if matches!(node, semantica_json::native_value::NativeNode::None) {
        return Ok(Vec::new());
    }
    let text = semantica_json::native_operations::python_text(value, id, false, limits)?;
    let lower = super::explorer_search_unicode::lower_points(
        semantica_json::native_operations::strip(&text),
        limits.max_codepoints,
    )?;
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
    Ok(out)
}

struct CoreMatches {
    matches: Vec<(Text, f64)>,
    cache_hit: bool,
    path: &'static str,
    candidates: usize,
}
fn tokens_points(points: &[u32]) -> BTreeSet<String> {
    points
        .split(|&cp| !((97..=122).contains(&cp) || (48..=57).contains(&cp)))
        .filter(|v| !v.is_empty())
        .map(|v| v.iter().map(|&cp| cp as u8 as char).collect())
        .collect()
}
impl serde::Serialize for GraphSearchIndex {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        if self.native_mode {
            return Err(serde::ser::Error::custom(
                "explorer_native_index_requires_native_wire",
            ));
        }
        use serde::ser::SerializeStruct;
        let mut out = s.serialize_struct("GraphSearchIndex", 5)?;
        out.serialize_field("documents", &self.documents)?;
        out.serialize_field("exact", &self.exact)?;
        out.serialize_field("tokens", &self.tokens)?;
        out.serialize_field("prefixes", &self.prefixes)?;
        out.serialize_field("cache", &self.cache)?;
        out.end()
    }
}
mod native;
pub use native::NativeSearchMatches;

mod state;
