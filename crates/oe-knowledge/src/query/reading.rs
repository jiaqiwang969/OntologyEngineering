//! Source-bound lexical candidates. Source identity is checked; semantic applicability is unassessed.
use super::Engine;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const PAYLOAD_BYTES: usize = 28_000;
const COORDINATES: &str = "normalized_native_text_unicode_characters";
fn named<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .context("knowledge_reading_string_missing")
}
pub(super) fn screen(_engine: &Engine<'_>, pages: &[Value], _route: &Value) -> Result<Vec<Value>> {
    pages.iter().map(|p| {
        let body = named(p,"body")?;
        ensure!(p["text_sha256"] == oe_core::sha256(body), "screen_page_text_identity_mismatch");
        Ok(json!({"id":p["id"],"relation":if body.trim().is_empty(){"unknown"}else{"lexical_only"},
            "evaluation":"not_run","document_type":"unknown","knowledge_roles":[],"chunks":[],
            "errors":[],"unprocessed_ranges":[],"missing_question_ranges":[]}))
    }).collect()
}
pub(super) fn units(
    engine: &Engine<'_>,
    page: &Value,
    _evaluation: &Value,
    _route: &Value,
) -> Result<Vec<Value>> {
    let body = named(page, "body")?;
    ensure!(
        page["text_sha256"] == oe_core::sha256(body),
        "unit_page_text_identity_mismatch"
    );
    let mut result = Vec::new();
    for (start, text) in crate::windows::bounded_chunks(body, 9000, 350)? {
        if text.trim().is_empty() {
            continue;
        }
        let end = start + text.chars().count();
        let hash = oe_core::sha256(&text);
        result.push(json!({"id":format!("{}:span:{start}:{end}:{}",named(page,"id")?,&hash[..12]),
            "page_id":page["id"],"source_text_sha256":page["text_sha256"],
            "source_span":{"start":start,"end":end,"text":text,"sha256":hash,"coordinate_system":COORDINATES},
            "roles":[],"serves_topics":[],"conditions_present":"not_assessed","cautions_present":"not_assessed",
            "context_sha256":engine.context_sha,"status":"source_passage_candidate","applicability":"not_assessed",
            "visual_and_numeric_verification":"not_run","claim_atomization":"not_run",
            "required_reading":"Read the complete source page and referenced continuation; retain conditions and exceptions."}));
    }
    Ok(result)
}
fn review_evidence(
    engine: &Engine<'_>,
    results: &[Value],
    source_bytes: usize,
    page_limit: usize,
) -> Result<Value> {
    ensure!(
        source_bytes <= 6500 && page_limit <= 8,
        "review_evidence_limit_invalid"
    );
    let chosen = &results[..results.len().min(page_limit)];
    let per_page = source_bytes / chosen.len().max(1);
    let mut chars = 0;
    let mut bytes = 0;
    let mut pages = Vec::new();
    for result in chosen {
        let units = result["knowledge_units"]
            .as_array()
            .context("review_units_missing")?;
        let mut seen = BTreeSet::new();
        for unit in units {
            ensure!(seen.insert(named(unit, "id")?), "review_duplicate_unit");
            let parent = &unit["source_span"];
            let text = named(parent, "text")?;
            let start = parent["start"].as_u64().context("review_parent_start")?;
            let end = parent["end"].as_u64().context("review_parent_end")?;
            ensure!(
                end >= start
                    && end - start == text.chars().count() as u64
                    && parent["sha256"] == oe_core::sha256(text)
                    && parent["coordinate_system"] == COORDINATES
                    && unit["source_text_sha256"] == result["text_sha256"]
                    && unit["page_id"] == result["id"]
                    && unit["context_sha256"] == engine.context_sha,
                "review_parent_identity_mismatch"
            );
        }
        let windows =
            crate::windows::review_windows(units, engine.query, engine.context, per_page)?;
        let mut selected: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
        for (id, start, end) in windows {
            ensure!(seen.contains(id.as_str()), "review_unknown_unit_window");
            selected.entry(id).or_default().push((start, end));
        }
        let (mut spans, mut omitted, mut ranges) = (Vec::new(), Vec::new(), Vec::new());
        for unit in units {
            let id = named(unit, "id")?;
            let parent = &unit["source_span"];
            let first = parent["start"].as_u64().unwrap() as usize;
            let last = parent["end"].as_u64().unwrap() as usize;
            let windows = selected.entry(id.into()).or_default();
            windows.sort_unstable();
            if windows.is_empty() {
                omitted.push(json!(id));
            }
            let mut cursor = first;
            for &(start, end) in windows.iter() {
                ensure!(
                    start >= cursor && end > start && end <= last,
                    "review_window_outside_parent_or_overlap"
                );
                if start > cursor {
                    ranges.push(json!({"unit_id":id,"start":cursor,"end":start}));
                }
                let text =
                    crate::text::char_slice(named(parent, "text")?, start - first, end - first)?;
                chars += text.chars().count();
                bytes += text.len();
                spans.push(json!({"unit_id":id,"parent_span_sha256":parent["sha256"],"start":start,"end":end,"text":text,"sha256":oe_core::sha256(&text),"roles":unit["roles"],"conditions_present":unit["conditions_present"],"cautions_present":unit["cautions_present"]}));
                cursor = end;
            }
            if cursor < last {
                ranges.push(json!({"unit_id":id,"start":cursor,"end":last}));
            }
        }
        pages.push(json!({"page_id":result["id"],"source_sha256":result["source_sha256"],"text_sha256":result["text_sha256"],"catalog":result["catalog"],"printed_page":result["printed_page"],"pdf_page":result["pdf_page"],"native_text_sparse":result["native_text_sparse"],"spans":spans,"omitted_unit_ids":omitted,"omitted_ranges":ranges}));
    }
    ensure!(
        bytes <= source_bytes && chars <= 12000,
        "review_source_budget_overflow"
    );
    Ok(
        json!({"pages":pages,"returned_page_count":results.len(),"omitted_page_ids":results[chosen.len()..].iter().map(|r|r["id"].clone()).collect::<Vec<_>>(),"coordinate_system":COORDINATES,"limits":{"max_pages":8,"max_source_characters":12000,"max_source_utf8_bytes":6500,"max_payload_utf8_bytes":PAYLOAD_BYTES},"coverage":"selected_returned_source_passage_candidates_only","window_selection":"opening_plus_context_and_condition_windows","condition_coverage":"not_established_read_omitted_ranges_before_adoption","full_corpus_semantic_scan":false,"full_page_reading":"not_run","visual_and_numeric_verification":"not_run","omission_meaning":"Omitted, truncated, sparse or unretrieved text is unknown, not absent.","included_page_count":chosen.len(),"source_characters":chars,"source_utf8_bytes":bytes}),
    )
}

pub(super) fn review(engine: &Engine<'_>, results: &[Value], _route: &Value) -> Result<Value> {
    let mut plan = json!({"schema":"ontology-engineering.supplier-review-plan/v1","status":"not_run",
        "kind":"next_inquiry_candidates","adoption":"pending_agent_review","execution":"not_run",
        "context_sha256":engine.context_sha,"policy_sha256":engine.policy.sha256,
        "method_bundle_sha256":engine.policy.sha256,"actions":[],"answers":{},"selected_actions":[],
        "uncertain_actions":[],"errors":[],"evidence_input":review_evidence(engine,results,6500,8)?,
        "reason":"host_agent_review_required"});
    super::merge(
        &mut plan,
        crate::policy::pattern_candidates(engine.policy, None)?,
    )?;
    Ok(plan)
}
