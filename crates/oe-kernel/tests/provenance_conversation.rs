//! Two archive-specific frozen cases plus real Project authority isolation.
use oe_kernel::{
    project::{Project, validate},
    provenance::prepare_entry_provenance,
};
use oe_knowledge::conversation::{self, Entry};
use semantica_runtime::{dataset::DatasetSession, provenance::load_records, sha256};
use serde_json::json;

fn archive() -> (tempfile::TempDir, std::path::PathBuf, Entry) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let base = root.join("var/maintenance/provenance-next-01/archive-tests");
    std::fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("synthetic.jsonl");
    // The decoded Chinese quote and escaped newline cannot have the same byte
    // interval or SHA as the surrounding raw JSON record.
    let raw = "{\"type\":\"response_item\",\"timestamp\":\"2026-10-02T12:00:00Z\",\"payload\":{\"type\":\"message\",\"id\":\"m1\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"\\u5b54\\u8ddd\\u6539\\u4e3a32 mm\\u3002\\n\"}]}}\n";
    std::fs::write(&path, raw).unwrap();
    let result = conversation::read_archive(&path, 100_000).unwrap();
    let entry = serde_json::from_value(result["entries"][0].clone()).unwrap();
    (temp, path, entry)
}
#[test]
fn conversation_archive_binding() {
    let (_temp, path, entry) = archive();
    let request = prepare_entry_provenance(
        &path,
        &entry,
        "urn:test:conversation",
        "urn:test:agent",
        "urn:test:candidate",
    )
    .unwrap();
    let mut session = DatasetSession::new();
    load_records(&mut session, &request).unwrap();
    let before = session.snapshot().unwrap();
    let mut changed = entry.clone();
    changed.text = "孔距改为35 mm。\n".into();
    assert!(
        prepare_entry_provenance(
            &path,
            &changed,
            "urn:test:conversation",
            "urn:test:agent",
            "urn:test:candidate2"
        )
        .is_err()
    );
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, original.replace("32 mm", "35 mm")).unwrap();
    assert!(
        prepare_entry_provenance(
            &path,
            &entry,
            "urn:test:conversation",
            "urn:test:agent",
            "urn:test:candidate2"
        )
        .is_err()
    );
    assert_eq!(session.snapshot().unwrap(), before);
}
#[test]
fn escaped_archive_projection_has_separate_identity() {
    let (_temp, path, entry) = archive();
    let request = prepare_entry_provenance(
        &path,
        &entry,
        "urn:test:conversation",
        "urn:test:agent",
        "urn:test:candidate",
    )
    .unwrap();
    assert_eq!(request.sources[0].binding.source_sha256, entry.raw_sha256);
    assert_eq!(
        request.sources[1].binding.source_sha256,
        sha256(&entry.text)
    );
    assert_ne!(
        request.sources[0].binding.source_sha256,
        request.sources[1].binding.source_sha256
    );
    assert!(
        request.sources[0].spans.is_empty(),
        "do not quote raw record containing unselected blocks"
    );
    let span = &request.sources[1].spans[0];
    assert_eq!(
        (span.start_byte, span.end_byte),
        (0, entry.text.len() as u64)
    );
    assert_eq!(span.source_id, request.sources[1].binding.source_id);
    assert_eq!(
        request.records[0].derived_from,
        vec![request.sources[0].binding.source_id.clone()]
    );
    let mut session = DatasetSession::new();
    load_records(&mut session, &request).unwrap();
    let snapshot = session.snapshot().unwrap();
    assert!(snapshot.canonical_nquads.contains("wasDerivedFrom"));
    assert!(!snapshot.canonical_nquads.contains("\\\\u5b54"));
    assert!(snapshot.canonical_nquads.contains("孔距改为32 mm。"));
}
#[test]
fn asserted_success_or_confidence1_keeps_actual_project_adoption_unchanged() {
    let (temp, path, entry) = archive();
    let project:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"project","revision":"1","purpose":"检验原需求",
        "objects":{"part":"1"},"conditions":{"current":"本轮工况"},"evidence":{},"dependencies":{},
        "decisions":[{"id":"d1","object_id":"part","object_revision":"1","condition_scope":"current","source_sha256":sha256("basis"),
            "conclusion":"unknown","adopted":false,"basis":[]}]})).unwrap();
    validate(&project).unwrap();
    let project_path = temp.path().join("project.json");
    let original = serde_json::to_vec(&project).unwrap();
    std::fs::write(&project_path, &original).unwrap();
    let mut request = prepare_entry_provenance(
        &path,
        &entry,
        "urn:test:conversation",
        "urn:test:agent",
        "urn:test:candidate",
    )
    .unwrap();
    request.records[1].confidence = Some(1.0);
    request.records[1].metadata = Some(json!({"passed":true,"adopted":true}));
    let mut session = DatasetSession::new();
    load_records(&mut session, &request).unwrap();
    assert_eq!(std::fs::read(&project_path).unwrap(), original);
    let current: Project = serde_json::from_slice(&original).unwrap();
    validate(&current).unwrap();
    assert_eq!(current.decisions[0].conclusion, "unknown");
    assert!(!current.decisions[0].adopted);
}
