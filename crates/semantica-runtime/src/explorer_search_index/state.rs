//! Explicit native index persistence, using the existing generated value envelope.
//! Stored data is derived index state, never another graph or Project authority.
use super::*;
use semantica_contracts::SemanticNativeValueEnvelope;
use semantica_json::native_value::{NativeLimits, NativeNode as N, NativeValue};
const PROFILE: &str = "semantica.explorer-index/2";
struct Builder {
    nodes: Vec<N>,
    limits: NativeLimits,
    links: usize,
    scalars: usize,
    d: Instant,
}
impl Builder {
    fn add(&mut self, n: N) -> Result<usize> {
        timely(self.d)?;
        ensure!(
            self.nodes.len() < self.limits.max_nodes,
            "native_index_node_limit"
        );
        let (links, scalars) = match &n {
            N::Text(v) => (
                0,
                v.len()
                    .checked_mul(4)
                    .context("native_index_scalar_limit")?,
            ),
            N::FloatBits(_) => (0, 8),
            N::List(v) | N::Tuple(v) => (v.len(), 0),
            N::Dict(v) => (
                v.len().checked_mul(2).context("native_index_link_limit")?,
                0,
            ),
            N::None => (0, 0),
            _ => unreachable!(),
        };
        self.links = self
            .links
            .checked_add(links)
            .filter(|n| *n <= self.limits.max_links)
            .context("native_index_link_limit")?;
        self.scalars = self
            .scalars
            .checked_add(scalars)
            .filter(|n| *n <= self.limits.max_scalar_bytes)
            .context("native_index_scalar_limit")?;
        let id = self.nodes.len();
        self.nodes.push(n);
        Ok(id)
    }
    fn text(&mut self, t: &Text) -> Result<usize> {
        ensure!(
            t.0.len() <= self.limits.max_scalar_bytes.saturating_sub(self.scalars) / 4,
            "native_index_scalar_limit"
        );
        self.add(N::Text(t.0.clone()))
    }
    fn name(&mut self, s: &str) -> Result<usize> {
        ensure!(
            s.chars().count() <= self.limits.max_scalar_bytes.saturating_sub(self.scalars) / 4,
            "native_index_scalar_limit"
        );
        self.add(N::Text(s.chars().map(u32::from).collect()))
    }
    fn list<'a>(&mut self, items: impl IntoIterator<Item = &'a Text>) -> Result<usize> {
        let mut ids = Vec::new();
        for t in items {
            ensure!(
                ids.len() < self.limits.max_links.saturating_sub(self.links),
                "native_index_link_limit"
            );
            ids.push(self.text(t)?);
        }
        self.add(N::List(ids))
    }
    fn field(&mut self, out: &mut Vec<(usize, usize)>, key: &str, val: usize) -> Result<()> {
        ensure!(
            out.len() < self.limits.max_links.saturating_sub(self.links) / 2,
            "native_index_link_limit"
        );
        out.push((self.name(key)?, val));
        Ok(())
    }
    fn posting(&mut self, p: &Posting) -> Result<usize> {
        let mut entries = Vec::new();
        for (k, ids) in p {
            ensure!(
                entries.len() < self.limits.max_links.saturating_sub(self.links) / 2,
                "native_index_link_limit"
            );
            let key = self.text(k)?;
            let val = self.list(ids)?;
            entries.push((key, val));
        }
        self.add(N::Dict(entries))
    }
    fn matches(&mut self, hits: &[(Text, f64)]) -> Result<usize> {
        let mut values = Vec::new();
        for (id, score) in hits {
            ensure!(
                values.len() < self.limits.max_links.saturating_sub(self.links),
                "native_index_link_limit"
            );
            let id = self.text(id)?;
            let score = self.add(N::FloatBits(score.to_bits()))?;
            values.push(self.add(N::Tuple(vec![id, score]))?);
        }
        self.add(N::List(values))
    }
    fn index(&mut self, index: &GraphSearchIndex, stored: bool) -> Result<usize> {
        let mut docs = Vec::new();
        for (id, doc) in &index.documents {
            ensure!(
                docs.len() < self.limits.max_links.saturating_sub(self.links) / 2,
                "native_index_link_limit"
            );
            let key = self.text(id)?;
            let mut fields = Vec::new();
            for (name, text) in [
                ("node_id", &doc.id),
                ("normalized_id", &doc.normalized_id),
                ("node_type", &doc.node_type),
                ("primary_text", &doc.primary),
                ("secondary_text", &doc.secondary),
            ] {
                let val = self.text(text)?;
                self.field(&mut fields, name, val)?;
            }
            let val = self.list(&doc.exact)?;
            self.field(&mut fields, "exact_terms", val)?;
            let mut tokens = Vec::new();
            for t in &doc.tokens {
                ensure!(
                    tokens.len() < self.limits.max_links.saturating_sub(self.links),
                    "native_index_link_limit"
                );
                tokens.push(self.name(t)?);
            }
            let val = self.add(N::List(tokens))?;
            self.field(&mut fields, "tokens", val)?;
            let val = self.add(
                doc.confidence
                    .map(|v| N::FloatBits(v.to_bits()))
                    .unwrap_or(N::None),
            )?;
            self.field(&mut fields, "confidence", val)?;
            let val = self.list(&doc.tags)?;
            self.field(&mut fields, "tags", val)?;
            docs.push((key, self.add(N::Dict(fields))?));
        }
        let mut fields = Vec::new();
        let val = self.add(N::Dict(docs))?;
        self.field(&mut fields, "documents", val)?;
        for (name, p) in [
            ("exact", &index.exact),
            ("tokens", &index.tokens),
            ("prefixes", &index.prefixes),
        ] {
            let val = self.posting(p)?;
            self.field(&mut fields, name, val)?;
        }
        let val = self.list(index.documents.keys())?;
        self.field(&mut fields, "ordered_ids", val)?;
        let mut cache = Vec::new();
        for (_, hits) in &index.cache {
            ensure!(
                cache.len() < self.limits.max_links.saturating_sub(self.links),
                "native_index_link_limit"
            );
            cache.push(self.matches(hits)?);
        }
        let val = self.add(N::List(cache))?;
        self.field(&mut fields, "cache_results", val)?;
        if stored {
            let mut keys = Vec::new();
            for (key, _) in &index.cache {
                ensure!(
                    keys.len() < self.limits.max_links.saturating_sub(self.links),
                    "native_index_link_limit"
                );
                keys.push(self.name(key)?);
            }
            let val = self.add(N::List(keys))?;
            self.field(&mut fields, "cache_keys", val)?;
        }
        self.add(N::Dict(fields))
    }
    fn finish(self, root: usize) -> Result<SemanticNativeValueEnvelope> {
        let v = NativeValue::new(self.nodes, root, self.limits)?;
        let out = crate::native_values::encode(&v, self.limits)?;
        timely(self.d)?;
        Ok(out)
    }
}
struct Read<'a> {
    value: &'a NativeValue,
    d: Instant,
    limits: NativeLimits,
    nodes: std::cell::Cell<usize>,
    links: std::cell::Cell<usize>,
    scalars: std::cell::Cell<usize>,
}
impl<'a> Read<'a> {
    fn charge(
        counter: &std::cell::Cell<usize>,
        amount: usize,
        limit: usize,
        error: &'static str,
    ) -> Result<()> {
        counter.set(
            counter
                .get()
                .checked_add(amount)
                .filter(|n| *n <= limit)
                .context(error)?,
        );
        Ok(())
    }
    fn links(&self, n: usize) -> Result<()> {
        Self::charge(
            &self.links,
            n,
            self.limits.max_links,
            "native_index_materialized_link_limit",
        )
    }
    fn node(&self, id: usize) -> Result<&'a N> {
        timely(self.d)?;
        Self::charge(
            &self.nodes,
            1,
            self.limits.max_nodes,
            "native_index_materialized_node_limit",
        )?;
        self.value.nodes().get(id).context("native_index_reference")
    }
    fn text(&self, id: usize) -> Result<Text> {
        match self.node(id)? {
            N::Text(v) => {
                Self::charge(
                    &self.scalars,
                    v.len()
                        .checked_mul(4)
                        .context("native_index_materialized_scalar_limit")?,
                    self.limits.max_scalar_bytes,
                    "native_index_materialized_scalar_limit",
                )?;
                Ok(Text(v.clone()))
            }
            _ => anyhow::bail!("native_index_text"),
        }
    }
    fn string(&self, id: usize) -> Result<String> {
        self.text(id)?.utf8()
    }
    fn list(&self, id: usize) -> Result<&'a [usize]> {
        match self.node(id)? {
            N::List(v) => {
                self.links(v.len())?;
                Ok(v)
            }
            _ => anyhow::bail!("native_index_list"),
        }
    }
    fn dict(&self, id: usize) -> Result<&'a [(usize, usize)]> {
        match self.node(id)? {
            N::Dict(v) => {
                self.links(
                    v.len()
                        .checked_mul(2)
                        .context("native_index_materialized_link_limit")?,
                )?;
                Ok(v)
            }
            _ => anyhow::bail!("native_index_dict"),
        }
    }
    fn fields(&self, id: usize, names: &[&str]) -> Result<BTreeMap<String, usize>> {
        let mut fields = BTreeMap::new();
        for (k, v) in self.dict(id)? {
            let key = self.string(*k)?;
            ensure!(
                names.contains(&key.as_str()) && fields.insert(key, *v).is_none(),
                "native_index_fields"
            );
        }
        ensure!(fields.len() == names.len(), "native_index_fields");
        Ok(fields)
    }
    fn texts(&self, id: usize) -> Result<Vec<Text>> {
        self.list(id)?.iter().map(|v| self.text(*v)).collect()
    }
    fn set(&self, id: usize) -> Result<BTreeSet<Text>> {
        let values = self.texts(id)?;
        ensure!(
            values.windows(2).all(|w| w[0] < w[1]),
            "native_index_set_order"
        );
        Ok(values.into_iter().collect())
    }
    fn posting(&self, id: usize) -> Result<Posting> {
        let mut p = Posting::new();
        for (k, v) in self.dict(id)? {
            let key = self.text(*k)?;
            let val = self.set(*v)?;
            ensure!(
                !val.is_empty() && p.insert(key, val).is_none(),
                "native_index_posting"
            );
        }
        Ok(p)
    }
    fn hits(&self, id: usize) -> Result<Vec<(Text, f64)>> {
        self.list(id)?
            .iter()
            .map(|id| {
                let N::Tuple(pair) = self.node(*id)? else {
                    anyhow::bail!("native_index_hit")
                };
                ensure!(pair.len() == 2, "native_index_hit");
                self.links(2)?;
                let id = self.text(pair[0])?;
                let N::FloatBits(bits) = self.node(pair[1])? else {
                    anyhow::bail!("native_index_score")
                };
                Self::charge(
                    &self.scalars,
                    8,
                    self.limits.max_scalar_bytes,
                    "native_index_materialized_scalar_limit",
                )?;
                let score = f64::from_bits(*bits);
                ensure!(
                    !id.is_empty() && score.is_finite() && score > 0.0,
                    "native_index_score"
                );
                Ok((id, score))
            })
            .collect()
    }
    fn index(&self, id: usize) -> Result<GraphSearchIndex> {
        let f = self.fields(
            id,
            &[
                "documents",
                "exact",
                "tokens",
                "prefixes",
                "ordered_ids",
                "cache_results",
                "cache_keys",
            ],
        )?;
        let mut documents = BTreeMap::new();
        for (k, v) in self.dict(f["documents"])? {
            let key = self.text(*k)?;
            let doc = self.fields(
                *v,
                &[
                    "node_id",
                    "normalized_id",
                    "node_type",
                    "exact_terms",
                    "tokens",
                    "primary_text",
                    "secondary_text",
                    "confidence",
                    "tags",
                ],
            )?;
            let confidence = match self.node(doc["confidence"])? {
                N::None => None,
                N::FloatBits(bits) => {
                    Self::charge(
                        &self.scalars,
                        8,
                        self.limits.max_scalar_bytes,
                        "native_index_materialized_scalar_limit",
                    )?;
                    Some(f64::from_bits(*bits))
                }
                _ => anyhow::bail!("native_index_confidence"),
            };
            let tags = self.texts(doc["tags"])?;
            ensure!(
                tags.iter().all(|v| !v.is_empty())
                    && tags.iter().collect::<BTreeSet<_>>().len() == tags.len(),
                "native_index_tags"
            );
            let tokens = self
                .set(doc["tokens"])?
                .into_iter()
                .map(|t| t.utf8())
                .collect::<Result<_>>()?;
            let doc = Document {
                id: self.text(doc["node_id"])?,
                normalized_id: self.text(doc["normalized_id"])?,
                node_type: self.text(doc["node_type"])?,
                exact: self.set(doc["exact_terms"])?,
                tokens,
                primary: self.text(doc["primary_text"])?,
                secondary: self.text(doc["secondary_text"])?,
                confidence,
                tags,
            };
            super::native::validate_document(&doc, self.d)?;
            ensure!(
                documents.insert(key, doc).is_none(),
                "native_index_duplicate_document"
            );
        }
        ensure!(
            self.texts(f["ordered_ids"])?.iter().eq(documents.keys()),
            "native_index_ordered_ids"
        );
        let keys = self.list(f["cache_keys"])?;
        let results = self.list(f["cache_results"])?;
        ensure!(
            keys.len() == results.len() && keys.len() <= 128,
            "native_index_cache"
        );
        let mut cache = VecDeque::new();
        let mut seen = BTreeSet::new();
        for (k, v) in keys.iter().zip(results) {
            let key = self.string(*k)?;
            let limit = super::native::validate_cache_key(&key)?;
            ensure!(seen.insert(key.clone()), "native_index_duplicate_cache_key");
            let hits = self.hits(*v)?;
            ensure!(
                hits.len() <= limit
                    && hits.iter().map(|(id, _)| id).collect::<BTreeSet<_>>().len() == hits.len(),
                "native_index_cache_hits"
            );
            cache.push_back((key, hits));
        }
        let index = GraphSearchIndex {
            native_mode: true,
            documents,
            exact: self.posting(f["exact"])?,
            tokens: self.posting(f["tokens"])?,
            prefixes: self.posting(f["prefixes"])?,
            cache,
        };
        index.validate_structure_before(Some(self.d))?;
        timely(self.d)?;
        Ok(index)
    }
}
impl GraphSearchIndex {
    /// Logical state projection for original-value comparisons. Set fields are
    /// sorted lists; it does not claim Python set order or object identity.
    pub fn observe_native(
        &self,
        limits: NativeLimits,
        d: Instant,
    ) -> Result<SemanticNativeValueEnvelope> {
        timely(d)?;
        ensure!(
            self.native_mode,
            "explorer_native_index_requires_native_profile"
        );
        let mut b = Builder {
            nodes: Vec::new(),
            limits,
            links: 0,
            scalars: 0,
            d,
        };
        let root = b.index(self, false)?;
        b.finish(root)
    }
    pub fn store_native(
        &self,
        limits: NativeLimits,
        d: Instant,
    ) -> Result<SemanticNativeValueEnvelope> {
        timely(d)?;
        ensure!(
            self.native_mode,
            "explorer_native_index_requires_native_profile"
        );
        self.validate_structure_before(Some(d))?;
        let mut b = Builder {
            nodes: Vec::new(),
            limits,
            links: 0,
            scalars: 0,
            d,
        };
        let schema = b.name(PROFILE)?;
        let index = b.index(self, true)?;
        let mut fields = Vec::new();
        b.field(&mut fields, "schema", schema)?;
        b.field(&mut fields, "index", index)?;
        let root = b.add(N::Dict(fields))?;
        b.finish(root)
    }
    pub fn restore_native(
        input: &SemanticNativeValueEnvelope,
        limits: NativeLimits,
        d: Instant,
    ) -> Result<Self> {
        timely(d)?;
        let value = crate::native_values::decode(input, limits)?;
        let r = Read {
            value: &value,
            d,
            limits,
            nodes: std::cell::Cell::new(0),
            links: std::cell::Cell::new(0),
            scalars: std::cell::Cell::new(0),
        };
        let fields = r.fields(value.root(), &["schema", "index"])?;
        ensure!(
            r.string(fields["schema"])? == PROFILE,
            "native_index_profile"
        );
        r.index(fields["index"])
    }
}
