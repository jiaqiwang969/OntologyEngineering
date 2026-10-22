//! Read-only engineering source adapter. The existing conversation parser and
//! verify_entry remain authoritative; this module never owns Project state.
use anyhow::{Context, Result, ensure};
use oe_knowledge::conversation::{self, Entry};
use semantica_contracts::*;
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

/// Build a private source-byte request only after the original raw record and
/// selected visible block have been reverified. Raw archive text is not emitted
/// as a source-span quote; only the selected visible text enters the RDF view.
pub fn prepare_entry_provenance(
    path: &Path,
    entry: &Entry,
    graph_id: &str,
    agent_id: &str,
    subject_id: &str,
) -> Result<SemanticProvenanceBatchRequest> {
    conversation::verify_entry(path, entry)?;
    let count = entry
        .byte_end
        .checked_sub(entry.byte_start)
        .context("conversation_record_range_invalid")?;
    ensure!(
        count <= semantica_runtime::MAX_BYTES as u64,
        "conversation_provenance_record_budget"
    );
    let mut file = oe_core::open_regular_file(path, u64::MAX)?;
    file.seek(SeekFrom::Start(entry.byte_start))?;
    let mut raw = vec![0; usize::try_from(count)?];
    file.read_exact(&mut raw)?;
    ensure!(
        semantica_runtime::sha256(&raw) == entry.raw_sha256,
        "historical_record_changed"
    );
    conversation::verify_entry(path, entry)?;
    let raw_data = String::from_utf8(raw).context("conversation_raw_utf8_invalid")?;
    let text_sha = semantica_runtime::sha256(&entry.text);
    let raw_id = format!("urn:semantica:conversation:raw:{}", entry.raw_sha256);
    let text_id = format!(
        "urn:semantica:conversation:text:{}:{}",
        entry.raw_sha256, entry.id
    );
    let span_id = format!(
        "urn:semantica:conversation:span:{}:{}",
        entry.raw_sha256, entry.id
    );
    let parser_id = format!(
        "urn:semantica:conversation:parse:{}:{}",
        entry.raw_sha256, entry.id
    );
    let parser_association = format!("{parser_id}:association");
    let extract_id = format!("{subject_id}:source-extraction");
    let source_binding =
        |source_id: String, source_sha256: String, byte_count: u64| SemanticSourceBinding {
            schema: "semantica.source-binding/1".into(),
            source_id,
            source_sha256,
            byte_count,
        };
    let span = SemanticSourceSpan {
        schema: "semantica.source-span/1".into(),
        span_id: span_id.clone(),
        source_id: text_id.clone(),
        source_sha256: text_sha.clone(),
        start_byte: 0,
        end_byte: entry.text.len() as u64,
        span_sha256: text_sha.clone(),
        quote: Some(entry.text.clone()),
    };
    let record = |subject: String,
                  generated_by: String,
                  source_spans: Vec<String>,
                  derived_from: Vec<String>,
                  metadata: Option<serde_json::Value>| SemanticProvenanceRecord {
        record_id: subject.clone(),
        subject_id: subject,
        entity_kind: "entity".into(),
        generated_by: Some(generated_by),
        attributed_to: vec![agent_id.into()],
        source_spans,
        derived_from,
        revision_of: Vec::new(),
        metadata,
        confidence: None,
    };
    let request = SemanticProvenanceBatchRequest {
        schema: "semantica.provenance-batch/1".into(),
        graph_id: graph_id.into(),
        sources: vec![
            SemanticProvenanceSource {
                binding: source_binding(raw_id.clone(), entry.raw_sha256.clone(), count),
                data: raw_data,
                spans: Vec::new(),
            },
            SemanticProvenanceSource {
                binding: source_binding(text_id.clone(), text_sha, entry.text.len() as u64),
                data: entry.text.clone(),
                spans: vec![span],
            },
        ],
        agents: vec![SemanticProvenanceAgent {
            agent_id: agent_id.into(),
            kind: "software_agent".into(),
        }],
        activities: vec![
            SemanticProvenanceActivity {
                activity_id: parser_id.clone(),
                used: vec![raw_id.clone()],
                associations: vec![SemanticProvenanceAssociation {
                    association_id: parser_association,
                    agent_id: agent_id.into(),
                    role_id: Some("urn:semantica:conversation:role:visible-parser".into()),
                }],
                started_at: None,
                ended_at: None,
            },
            SemanticProvenanceActivity {
                activity_id: extract_id.clone(),
                used: vec![text_id.clone()],
                associations: vec![SemanticProvenanceAssociation {
                    association_id: format!("{extract_id}:association"),
                    agent_id: agent_id.into(),
                    role_id: Some("urn:semantica:conversation:role:source-extractor".into()),
                }],
                started_at: None,
                ended_at: None,
            },
        ],
        records: vec![
            record(
                text_id.clone(),
                parser_id,
                vec![span_id.clone()],
                vec![raw_id],
                Some(
                    serde_json::json!({"parser_profile":"oe-knowledge.conversation.visible-record/1","entry_id":entry.id,"kind":entry.kind,"role":entry.role,"record_id":entry.record_id,"raw_record_start":entry.byte_start,"raw_record_end":entry.byte_end,"raw_record_sha256":entry.raw_sha256,"decoded_text_span_origin":"separate_utf8_source","timestamp_declared":entry.timestamp}),
                ),
            ),
            record(
                subject_id.into(),
                extract_id,
                vec![span_id],
                vec![text_id],
                None,
            ),
        ],
        max_records: 64,
        max_quads: 4096,
    };
    semantica_runtime::provenance::prepare_records(&oxigraph::model::Dataset::new(), &request)?;
    Ok(request)
}
