//! Source UPDATE reductions of the existing parser; no string reparsing.
use super::*;

impl ParserState {
    pub(super) fn source_update_unresolved_prefix(
        &mut self,
        prefix: &str,
        local: &str,
    ) -> Iri<String> {
        if self.unresolved_update_prefix.is_none() {
            self.unresolved_update_prefix = Some(prefix.to_owned());
        }
        // This is only the unresolved QName's original syntax spelling. A
        // missing prefix must not mask a later parse failure. The entry point
        // rejects after a successful parse, before any AST can be returned;
        // it never admits this unresolved token as an executable RDF value.
        Iri::parse_unchecked(format!("{prefix}:{local}"))
    }

    pub(super) fn source_update_plain_triples(
        values: Vec<TripleOrPathPattern>,
    ) -> Result<Vec<TriplePattern>, &'static str> {
        let mut triples = Vec::new();
        for value in values {
            match value {
                TripleOrPathPattern::Triple(t) => triples.push(t),
                TripleOrPathPattern::SourceRow { physical, .. } => {
                    triples.extend(Self::source_update_plain_triples(physical)?)
                }
                TripleOrPathPattern::Path { .. } => {
                    return Err("Paths are not allowed in source update templates");
                }
            }
        }
        Ok(triples)
    }
    pub(super) fn source_update_data(
        &mut self,
        mut triples: Vec<TriplePattern>,
        named: Vec<(GraphNamePattern, Vec<TriplePattern>, Vec<TriplePattern>)>,
    ) -> Result<SourceUpdateData, &'static str> {
        let Some(order) = self
            .source_literal_hooks
            .as_ref()
            .and_then(|h| h.triples_order.clone())
        else {
            return Err("Source update requires its actual literal and triples owner");
        };
        for (_, _, tail) in &named {
            triples.extend(tail.iter().cloned());
        }
        let triples = order(&triples).map_err(|e| {
            self.numeric_construction_error = Some(e);
            "Source update triple ordering failed"
        })?;
        let mut quads: Vec<(GraphNamePattern, Vec<TriplePattern>)> = Vec::new();
        for (name, values, _) in named {
            if values.is_empty() {
                continue;
            }
            let mut values = order(&values).map_err(|e| {
                self.numeric_construction_error = Some(e);
                "Source update triple ordering failed"
            })?;
            if let Some((_, previous)) = quads.iter_mut().find(|(key, _)| key == &name) {
                previous.append(&mut values);
            } else {
                quads.push((name, values));
            }
        }
        Ok(SourceUpdateData { triples, quads })
    }

    pub(super) fn source_update_where(
        &mut self,
        parsed: ParsedPattern,
    ) -> Result<GraphPattern, &'static str> {
        let ParsedPattern::Source(group) = parsed else {
            return Err("Source update WHERE requires its actual syntax owner");
        };
        let hooks = self
            .source_literal_hooks
            .clone()
            .ok_or("Source update literal owner missing")?;
        self.source_arena
            .finish_update_where(group, &hooks)
            .map_err(|e| {
                self.numeric_construction_error = Some(e);
                "Source update WHERE translation failed"
            })
    }
}
