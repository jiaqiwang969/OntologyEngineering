//! Characterisation tests for `semantica/ingest/file_ingestor.py` (expected values from the
//! original run in the oracle environment).

use sem_ingest::file_ingestor::{FileIngestor, FileTypeDetector};
use sem_ingest::support::kw::Kwargs;
use serde_json::{Value, json};

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn type_detector_support_and_magic_numbers() {
    let d = FileTypeDetector::new();
    let supported: Vec<bool> = ["pdf", "PDF", "xyz", "txt", "mp4", "md", "json", "csv", "log", "gz"]
        .iter()
        .map(|x| d.is_supported(x))
        .collect();
    assert_eq!(supported, vec![true, true, false, true, true, false, true, true, false, false]);
    assert_eq!(d._detect_by_magic_numbers(b"PAR1xxxx").as_deref(), Some("parquet"));
    assert_eq!(d._detect_by_magic_numbers(b"\xff\xd8\xffxx").as_deref(), Some("jpg"));
    assert_eq!(d._detect_by_magic_numbers(b"abc"), None);
}

#[test]
fn ingest_file_metadata_and_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_string_lossy().into_owned();
    let p = tmp.path().join("notes.md");
    std::fs::write(&p, "# hi").unwrap();
    let p = p.to_string_lossy().into_owned();
    let mut ing = FileIngestor::default();
    let o = ing
        .ingest_file(&p, &kwargs(json!({"read_content": false, "extra": 1})))
        .unwrap();
    assert_eq!((o.name.as_str(), o.size, o.file_type.as_str()), ("notes.md", 4, "md"));
    assert_eq!(o.mime_type, None);
    assert_eq!(o.content, None);
    let mut metadata = o.metadata.clone();
    assert_eq!(metadata.remove("parent"), Some(Value::String(dir.clone())));
    assert_eq!(
        Value::Object(metadata),
        json!({"extension": ".md", "is_supported": false, "read_content": false, "extra": 1})
    );

    let err = ing
        .ingest_file(&format!("{dir}/missing.txt"), &Kwargs::new())
        .unwrap_err();
    assert!(err.is_validation_error());
    assert_eq!(err.to_string(), format!("File not found: {dir}/missing.txt"));
    let err = ing.ingest_file(&dir, &Kwargs::new()).unwrap_err();
    assert_eq!(err.to_string(), format!("Path is not a file: {dir}"));
    let err = ing.ingest_directory(&p, true, &Kwargs::new()).unwrap_err();
    assert!(err.is_validation_error());
    assert_eq!(err.to_string(), format!("Path is not a directory: {p}"));

    let scan = ing
        .scan_directory(&dir, &kwargs(json!({"extensions": ["md"]})))
        .unwrap();
    let summary: Vec<(Value, Value, Value)> = scan
        .iter()
        .map(|x| (x["name"].clone(), x["size"].clone(), x["extension"].clone()))
        .collect();
    assert_eq!(summary, vec![(json!("notes.md"), json!(4), json!("md"))]);
    let names: Vec<Value> = ing
        .scan_directory(&dir, &kwargs(json!({"pattern": "*.md"})))
        .unwrap()
        .iter()
        .map(|x| x["name"].clone())
        .collect();
    assert_eq!(names, vec![json!("notes.md")]);
}
