//! Lexical candidates only. Unicode normalization never changes stored evidence.
use crate::contracts::ExperienceRecord;
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

fn normalize(text: &str) -> String {
    text.nfkc().flat_map(char::to_lowercase).collect()
}

fn terms(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let mut adjacent = Vec::new();
    let flush = |adjacent: &mut Vec<char>, result: &mut BTreeSet<String>| {
        if adjacent.len() == 1 {
            result.insert(adjacent.iter().collect());
        } else {
            // UAX word boundaries separate individual Han characters. Adjacent
            // pairs/triples retain local phrases without a language dictionary.
            for size in [2, 3] {
                for window in adjacent.windows(size) {
                    result.insert(window.iter().collect());
                }
            }
        }
        adjacent.clear();
    };
    for word in text.split_word_bounds() {
        let mut chars = word.chars();
        let first = chars.next();
        if let Some(c) = first.filter(|c| !c.is_ascii() && c.is_alphabetic())
            && chars.next().is_none()
        {
            adjacent.push(c);
        } else {
            flush(&mut adjacent, &mut result);
            if word.chars().any(char::is_alphanumeric) {
                result.insert(word.to_owned());
            }
        }
    }
    flush(&mut adjacent, &mut result);
    result
}

fn record_fields(record: &ExperienceRecord) -> Vec<(f64, String)> {
    let mut fields = vec![
        (6.0, record.purpose.clone()),
        (2.0, record.conditions.join("\n")),
        (0.5, record.unknowns.join("\n")),
        (
            1.0,
            record
                .timeline
                .iter()
                .map(|e| e.description.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    ];
    let mut causes = Vec::new();
    for claim in &record.causal_claims {
        causes.push(claim.statement.as_str());
        causes.extend(claim.confounders.iter().map(String::as_str));
        causes.extend(claim.unknowns.iter().map(String::as_str));
    }
    fields.push((1.0, causes.join("\n")));
    if let Some(method) = &record.method {
        fields.extend([
            (4.0, method.purpose.clone()),
            (3.0, method.mechanism.clone()),
            (2.0, method.validation.clone()),
        ]);
    }
    fields
}

pub(crate) fn lexical_candidates(
    query: &str,
    records: Vec<ExperienceRecord>,
) -> Vec<(usize, ExperienceRecord)> {
    let query = normalize(query.trim());
    let query_terms = terms(&query);
    let mut ranked = Vec::new();
    for record in records {
        // Subject and method fields carry more signal than incidental chronology.
        // Normalize by distinct field vocabulary: repeats cannot pump rank, and a
        // long history does not win merely by mentioning more unrelated subjects.
        // Every field remains searchable; scores are relevance hints, not evidence.
        let score = if query.is_empty() {
            1
        } else {
            let mut weighted = 0.0;
            for (weight, text) in record_fields(&record) {
                let text = normalize(&text);
                let document_terms = terms(&text);
                let overlap = query_terms.intersection(&document_terms).count();
                if overlap > 0 {
                    weighted += weight
                        * (overlap as f64 + 0.5 * usize::from(text.contains(&query)) as f64)
                        / (document_terms.len() as f64).sqrt();
                }
            }
            if weighted > 0.0 {
                (weighted * 10000.0).round().max(1.0) as usize
            } else {
                0
            }
        };
        if score > 0 {
            ranked.push((score, record));
        }
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
    ranked
}

// --- track D: history anchor catalog scoring ---
/// A query prepared once with recall's normalization and terms.
pub(crate) struct QueryTerms {
    normalized: String,
    terms: BTreeSet<String>,
}

impl QueryTerms {
    pub(crate) fn new(query: &str) -> Self {
        let normalized = normalize(query.trim());
        let terms = terms(&normalized);
        Self { normalized, terms }
    }

    /// Distinct terms in the query.
    pub(crate) fn len(&self) -> usize {
        self.terms.len()
    }
}

/// For one anchor text: how many distinct query terms it contains, and recall's
/// field formula with weight 1.0. The substring test only skips texts that
/// cannot share a term, since every document term is a substring of the
/// normalized text; it never changes a nonzero result.
pub(crate) fn text_match(query: &QueryTerms, text: &str) -> Option<(u64, u64)> {
    // NFKC leaves ASCII unchanged, so lowercasing alone is the same result.
    let text = if text.is_ascii() {
        text.to_ascii_lowercase()
    } else {
        normalize(text)
    };
    if !query.terms.iter().any(|term| text.contains(term.as_str())) {
        return None;
    }
    let document_terms = terms(&text);
    let overlap = query.terms.intersection(&document_terms).count();
    if overlap == 0 {
        return None;
    }
    let weighted = (overlap as f64 + 0.5 * usize::from(text.contains(&query.normalized)) as f64)
        / (document_terms.len() as f64).sqrt();
    Some((overlap as u64, (weighted * 10000.0).round().max(1.0) as u64))
}
