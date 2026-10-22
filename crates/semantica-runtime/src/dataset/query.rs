//! One session owns parameter-aware query results and mutation invalidation.
use super::DatasetSession;
use anyhow::{Context, Result, ensure};
use oxigraph::model::Term;
use semantica_contracts::SemanticSessionQueryResult;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

/// Preserve the legacy text key and its payload accounting. Options are a
/// distinct domain; JSON separators/escaping preserve all caller parameter bytes.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum QueryCacheKey {
    Plain(String),
    Options(String),
    #[cfg(feature = "rdflib760-nullable-internal")]
    Source(String),
}
impl QueryCacheKey {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Plain(s) | Self::Options(s) => s.len(),
            #[cfg(feature = "rdflib760-nullable-internal")]
            Self::Source(s) => s.len(),
        }
    }
    #[cfg(feature = "rdflib760-nullable-internal")]
    pub(super) fn source<T: serde::Serialize>(value: &T) -> Result<Option<Self>> {
        let mut out = KeyWriter {
            bytes: Vec::new(),
            overflow: false,
        };
        let encoded = serde_json::to_writer(&mut out, value);
        if out.overflow {
            return Ok(None);
        }
        encoded?;
        Ok(Some(Self::Source(String::from_utf8(out.bytes)?)))
    }
    fn new(
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Term>,
    ) -> Result<Option<Self>> {
        if namespaces.is_empty() && bindings.is_empty() {
            return Ok(Some(Self::Plain(query.into())));
        }
        // The graph owner's input budget is checked before this bounded copy.
        let terms: Vec<_> = bindings.iter().map(|(k, v)| (k, v.to_string())).collect();
        let mut out = KeyWriter {
            bytes: Vec::new(),
            overflow: false,
        };
        let encoded = serde_json::to_writer(&mut out, &(query, namespaces, terms));
        if out.overflow {
            return Ok(None);
        }
        encoded?;
        Ok(Some(Self::Options(String::from_utf8(out.bytes)?)))
    }
}
struct KeyWriter {
    bytes: Vec<u8>,
    overflow: bool,
}

// Count only cache-admissible JSON bytes. Oversized graph results do not need a
// second full serialization allocation merely to decide they cannot be cached.
#[derive(Default)]
struct ResultSizer {
    bytes: usize,
    overflow: bool,
}
impl Write for ResultSizer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > crate::MAX_BYTES - self.bytes {
            self.overflow = true;
            return Err(io::Error::other("dataset_query_result_byte_budget"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "rdflib760-nullable-internal")]
pub(super) fn bounded_json_size<T: serde::Serialize>(value: &T) -> Result<Option<usize>> {
    let mut size = ResultSizer::default();
    let encoded = serde_json::to_writer(&mut size, value);
    if size.overflow {
        return Ok(None);
    }
    encoded?;
    Ok(Some(size.bytes))
}

#[derive(Clone, Copy)]
enum ResultDomain {
    Session,
    ExistingChapterGraph,
}
impl Write for KeyWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > crate::MAX_BYTES - self.bytes.len() {
            self.overflow = true;
            return Err(io::Error::other("dataset_query_cache_key_budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl DatasetSession {
    pub fn query(&mut self, query: &str, use_cache: bool) -> Result<SemanticSessionQueryResult> {
        self.query_with_options(query, &BTreeMap::new(), &BTreeMap::new(), use_cache)
    }

    /// Native local-query semantics with caller namespaces and exact RDF1.1
    /// bindings. The same session, result DTO, source budgets and mutation
    /// invalidation apply. This does not expose the private nullable evaluator or
    /// promise Python wrapper/term/cache-capacity equivalence outside its scope.
    pub fn query_with_options(
        &mut self,
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Term>,
        use_cache: bool,
    ) -> Result<SemanticSessionQueryResult> {
        self.query_options_inner(
            query,
            namespaces,
            bindings,
            use_cache,
            ResultDomain::Session,
        )
    }

    /// Preserve the chapter owner's existing graph-result range. All cached
    /// entries still fit the ordinary session's limits, so no cache hit can
    /// carry an oversized chapter response into a bounded session query.
    pub(crate) fn query_chapter_with_options(
        &mut self,
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Term>,
    ) -> Result<SemanticSessionQueryResult> {
        self.query_options_inner(
            query,
            namespaces,
            bindings,
            true,
            ResultDomain::ExistingChapterGraph,
        )
    }

    fn query_options_inner(
        &mut self,
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Term>,
        use_cache: bool,
        domain: ResultDomain,
    ) -> Result<SemanticSessionQueryResult> {
        self.rdflib_projection.require_native_complete()?;
        crate::graph::check_query_parameter_budget(query, namespaces, bindings)?;
        let key = if use_cache {
            QueryCacheKey::new(query, namespaces, bindings)?
        } else {
            None
        };
        if let Some(key) = &key
            && let Some(result) = self.query_cache.get(key)
        {
            return Ok(SemanticSessionQueryResult {
                result: result.clone(),
                revision: self.revision,
                cache_hit: true,
            });
        }
        let result = crate::graph::query_with_options(&self.dataset, query, namespaces, bindings)?;
        let mut size = ResultSizer::default();
        let encoded = serde_json::to_writer(&mut size, &result);
        if !size.overflow {
            encoded?;
        }
        ensure!(
            !size.overflow || matches!(domain, ResultDomain::ExistingChapterGraph),
            "dataset_query_result_byte_budget"
        );
        if !size.overflow
            && let Some(key) = key
        {
            self.cache_query_result(key, &result, size.bytes)?;
        }

        Ok(SemanticSessionQueryResult {
            result,
            revision: self.revision,
            cache_hit: false,
        })
    }
    pub(super) fn cache_query_result(
        &mut self,
        key: QueryCacheKey,
        result: &Value,
        result_bytes: usize,
    ) -> Result<()> {
        let bytes = result_bytes
            .checked_add(key.len())
            .context("dataset_query_cache_budget")?;
        if bytes > crate::MAX_BYTES {
            return Ok(());
        }
        if self.explorer_search_cache.as_ref().is_some_and(|c| {
            c.payload_bytes
                .saturating_add(self.cache_bytes)
                .saturating_add(bytes)
                > self.dataset_byte_limit()
        }) {
            self.explorer_search_cache = None;
        }
        while self.query_cache.len() >= 256 || self.cache_bytes + bytes > crate::MAX_BYTES {
            let (key, value) = self
                .query_cache
                .pop_first()
                .context("dataset_query_cache_accounting_invalid")?;
            self.cache_bytes -= key.len() + serde_json::to_vec(&value)?.len();
        }
        self.query_cache.insert(key, result.clone());
        self.cache_bytes += bytes;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{Literal, NamedNode};

    #[test]
    fn option_key_domain_and_escaped_payload_budget_are_exact() {
        for value in [
            serde_json::json!({"汉字":"\"\\\u{1}"}),
            serde_json::json!([null, true, 17, "x"]),
        ] {
            let mut size = ResultSizer::default();
            serde_json::to_writer(&mut size, &value).unwrap();
            assert_eq!(size.bytes, serde_json::to_vec(&value).unwrap().len());
            assert!(!size.overflow);
        }
        let mut size = ResultSizer::default();
        size.write_all(&vec![b'x'; crate::MAX_BYTES]).unwrap();
        assert!(size.write_all(b"x").is_err());
        assert!(size.overflow);
        assert_eq!(size.bytes, crate::MAX_BYTES);
        let ns = BTreeMap::from([("x".into(), "urn:".into())]);
        let key = QueryCacheKey::new("ASK {}", &ns, &BTreeMap::new())
            .unwrap()
            .unwrap();
        let QueryCacheKey::Options(encoded) = key else {
            panic!("options key")
        };
        assert!(QueryCacheKey::Plain(encoded.clone()) != QueryCacheKey::Options(encoded));
        // Valid RDF input can exceed the cache allowance after JSON escaping.
        let bindings = BTreeMap::from([(
            "v".into(),
            Term::Literal(Literal::new_simple_literal(
                "\u{1}".repeat((crate::MAX_BYTES - 4096) / 6),
            )),
        )]);
        crate::graph::check_query_parameter_budget("ASK {}", &ns, &bindings).unwrap();
        assert!(
            QueryCacheKey::new("ASK {}", &ns, &bindings)
                .unwrap()
                .is_none()
        );
        let mut session = DatasetSession::new();
        for _ in 0..2 {
            let result = session
                .query_with_options("ASK {}", &ns, &bindings, true)
                .unwrap();
            assert!(!result.cache_hit);
            assert_eq!(result.result["value"], true);
        }
        assert!(session.query_cache.is_empty());
        assert_eq!(session.revision, 0);
        let mut writer = KeyWriter {
            bytes: Vec::new(),
            overflow: false,
        };
        writer.write_all(&vec![b'x'; crate::MAX_BYTES]).unwrap();
        assert!(writer.write_all(b"x").is_err());
        assert_eq!(writer.bytes.len(), crate::MAX_BYTES);
    }
    #[test]
    fn mixed_cache_eviction_accounts_for_complete_option_keys() {
        let mut s = DatasetSession::new();
        let ns = BTreeMap::new();
        s.query("ASK {}", true).unwrap();
        for n in 0..270 {
            let b = BTreeMap::from([(
                "v".into(),
                NamedNode::new(format!("urn:item:{n}")).unwrap().into(),
            )]);
            assert!(
                !s.query_with_options("ASK {}", &ns, &b, true)
                    .unwrap()
                    .cache_hit
            );
            let exact: usize = s
                .query_cache
                .iter()
                .map(|(k, v)| k.len() + serde_json::to_vec(v).unwrap().len())
                .sum();
            assert_eq!(exact, s.cache_bytes);
            assert!(s.query_cache.len() <= 256);
            assert!(exact <= crate::MAX_BYTES);
        }
        assert_eq!(s.revision, 0);
        assert!(!s.query("ASK {}", true).unwrap().cache_hit);
        s.clear(None).unwrap();
        assert!(s.query_cache.is_empty());
        assert_eq!(s.cache_bytes, 0);
    }
}
