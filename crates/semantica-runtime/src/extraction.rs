//! Original finite pattern extraction, selected explicitly when model backends are absent.
//! A learned NER/LLM backend is a host port; this fallback does not certify model inference.
use anyhow::{Context, Result, ensure};
use regex::Regex;
use semantica_contracts::{
    SemanticExtractionPort, SemanticExtractionRequest, SemanticExtractionResult,
    SemanticRuntimeFailure,
};
use serde_json::{Value, json};

pub const PROFILE: &str = "semantica.original-pattern-fallback/1";
const PATTERNS: &str = include_str!("relation-patterns.json");
struct Entity {
    text: String,
    label: &'static str,
    start: usize,
    end: usize,
}
fn fallback(text: &str) -> Result<Vec<Entity>> {
    ensure!(text.len() <= 256 * 1024, "extraction_text_byte_budget");
    let mut entities = Vec::<Entity>::new();
    for (label, pattern) in [
        ("PERSON", r"\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+)+)\b"),
        (
            "ORG",
            r"\b([A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*\s+(?:Inc|Corp|LLC|Ltd|Company))\b",
        ),
        ("GPE", r"\b([A-Z][a-z]+\s*(?:City|State|Country|Nation))\b"),
        ("DATE", r"\b(\d{1,2}[/-]\d{1,2}[/-]\d{2,4}|\d{4})\b"),
    ] {
        for matched in Regex::new(pattern)?.captures_iter(text) {
            let whole = matched.get(0).context("extraction_match_missing")?;
            if entities
                .iter()
                .any(|e| e.start < whole.end() && e.end > whole.start())
            {
                continue;
            }
            let captured = matched.get(1).unwrap_or(whole);
            entities.push(Entity {
                text: captured.as_str().to_owned(),
                label,
                start: whole.start(),
                end: whole.end(),
            });
            ensure!(entities.len() <= 2048, "extraction_entity_budget");
        }
    }
    if entities.is_empty() {
        for matched in Regex::new(r"\b[A-Z][a-z]{2,}\b")?.find_iter(text) {
            entities.push(Entity {
                text: matched.as_str().to_owned(),
                label: "UNKNOWN",
                start: matched.start(),
                end: matched.end(),
            });
            ensure!(entities.len() <= 2048, "extraction_entity_budget");
        }
    }
    Ok(entities)
}
pub fn extract_pattern(text: &str, operation: &str) -> Result<Value> {
    let entities = fallback(text)?;
    if operation == "entities" {
        // Preserve the original canonical projection: Entity has start_char/end_char,
        // so its historical getattr(start/end/type) fields are null.
        return Ok(
            json!({"entities":entities.iter().map(|e|json!({"label":e.label,"type":null,"start":null,"end":null})).collect::<Vec<_>>()}),
        );
    }
    ensure!(operation == "relations", "extraction_operation_unsupported");
    let mut sorted = entities.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|e| std::cmp::Reverse(e.text.chars().count()));
    let mut texts = Vec::new();
    for e in sorted {
        let text = regex::escape(&e.text);
        if !texts.contains(&text) {
            texts.push(text);
        }
    }
    if texts.is_empty() {
        return Ok(json!({"relations":[],"triplets":[]}));
    }
    let alternatives = format!("(?:{})", texts.join("|"));
    let patterns: Value = crate::providers::parse_protocol_json(PATTERNS)
        .map_err(|_| anyhow::anyhow!("extraction_patterns_invalid"))?;
    let mut relations = Vec::new();
    let mut triplets = Vec::new();
    for (kind, patterns) in patterns
        .as_object()
        .context("extraction_patterns_invalid")?
    {
        for pattern in patterns.as_array().context("extraction_patterns_invalid")? {
            let expression = pattern
                .as_str()
                .context("extraction_pattern_invalid")?
                .replace("{ENTITY}", &alternatives);
            let regex = regex::RegexBuilder::new(&expression)
                .case_insensitive(true)
                .size_limit(2 * 1024 * 1024)
                .build()?;
            for matched in regex.captures_iter(text) {
                let subject = matched
                    .name("subject")
                    .context("extraction_subject_missing")?
                    .as_str()
                    .trim();
                let object = matched
                    .name("object")
                    .context("extraction_object_missing")?
                    .as_str()
                    .trim();
                let source = entities
                    .iter()
                    .rev()
                    .find(|e| e.text.to_lowercase() == subject.to_lowercase());
                let target = entities
                    .iter()
                    .rev()
                    .find(|e| e.text.to_lowercase() == object.to_lowercase());
                if let (Some(source), Some(target)) = (source, target) {
                    relations.push(json!({"source":source.text,"type":kind,"target":target.text}));
                    triplets
                        .push(json!({"subject":source.text,"predicate":kind,"object":target.text}));
                }
                ensure!(relations.len() <= 8192, "extraction_relation_budget");
            }
        }
    }
    Ok(json!({"relations":relations,"triplets":triplets}))
}
pub struct RustPatternExtraction;
impl SemanticExtractionPort for RustPatternExtraction {
    fn extract(
        &self,
        input: &SemanticExtractionRequest,
    ) -> std::result::Result<SemanticExtractionResult, SemanticRuntimeFailure> {
        let result = (|| -> Result<SemanticExtractionResult> {
            ensure!(
                input.validate().is_ok(),
                "extraction_request_contract_invalid"
            );
            let data = extract_pattern(&input.text, &input.operation)?;
            Ok(SemanticExtractionResult{schema:"semantica.extraction-result/1".into(),profile:PROFILE.into(),model_execution:"not_run".into(),data,uncovered:vec!["spaCy/HuggingFace/LLM extraction backend not selected by this finite fallback".into(),"original canonical relation missing-entities exception is retained; corrected relation projection is separately versioned".into()]})
        })();
        result.map_err(|_| SemanticRuntimeFailure {
            code: "extraction_failed".into(),
            phase: "extraction".into(),
            message: "The selected bounded extraction operation did not complete.".into(),
            retryable: false,
        })
    }
}
