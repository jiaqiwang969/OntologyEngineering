//! Port of `tests/ingest/test_ingestors.py`. Every test installs `conftest.mock_dns` first.
//!
//! Three originals failed in the baseline run and are ported to what the code actually does:
//! * `TestWebIngestor.test_ingest_url` / `TestUnifiedIngest.test_ingest_web_dispatch` configure
//!   `MockSession.return_value.get`, but the SSRF-guarded path calls `Session.request`, whose
//!   unconfigured `MagicMock` result reached BeautifulSoup (`TypeError`). A Rust transport always
//!   yields a real response, so the ports assert that the configured page is never used.
//! * `TestFeedIngestor.test_ingest_feed` patches `requests.get`, which the guarded path never
//!   calls; the request failed closed on the reference machine's proxy.
#![allow(non_snake_case)]

mod common;

use common::*;
use sem_ingest::feed_ingestor::FeedIngestor;
use sem_ingest::file_ingestor::{FileContent, FileIngestor, FileTypeDetector};
use sem_ingest::methods::{IngestOutput, Source, ingest};
use sem_ingest::support::kw::Kwargs;
use sem_ingest::web_ingestor::{ROBOTS_CAN_FETCH, WebIngestor, WebIngestorOptions};
use serde_json::json;

fn kwargs(v: serde_json::Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

// --- TestFileIngestor ---

#[test]
fn test_file_ingestor__test_file_type_detector() {
    let _dns = mock_dns();
    let detector = FileTypeDetector::new();
    // Test known extension
    assert_eq!(detector.detect_type("test.txt", None), "txt");
    assert_eq!(detector.detect_type("test.pdf", None), "pdf");
    assert_eq!(detector.detect_type("test.jpg", None), "jpg");
}

#[test]
fn test_file_ingestor__test_ingest_file() {
    let _dns = mock_dns();
    let mut ingestor = FileIngestor::default();
    let tmp = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
    std::fs::write(tmp.path(), "Hello World").unwrap();
    let tmp_path = tmp.path().to_string_lossy().into_owned();
    let result = ingestor
        .ingest_file(&tmp_path, &kwargs(json!({"read_content": true})))
        .unwrap();
    assert_eq!(result.path, tmp_path);
    assert_eq!(result.file_type, "txt");
    assert_eq!(result.mime_type.as_deref(), Some("text/plain"));
    assert_eq!(result.content, Some(FileContent::Bytes(b"Hello World".to_vec())));
}

#[test]
fn test_file_ingestor__test_ingest_directory() {
    let _dns = mock_dns();
    let mut ingestor = FileIngestor::default();
    let tmp_dir = tempfile::tempdir().unwrap();
    let root = tmp_dir.path();
    std::fs::write(root.join("f1.txt"), "content1").unwrap();
    std::fs::write(root.join("f2.md"), "content2").unwrap();
    std::fs::create_dir(root.join("subdir")).unwrap();
    std::fs::write(root.join("subdir").join("f3.log"), "content3").unwrap();
    let dir = root.to_string_lossy().into_owned();
    // Non-recursive
    let results = ingestor.ingest_directory(&dir, false, &Kwargs::new()).unwrap();
    assert_eq!(results.len(), 2);
    // Recursive
    let results = ingestor.ingest_directory(&dir, true, &Kwargs::new()).unwrap();
    assert_eq!(results.len(), 3);
}

// --- TestWebIngestor ---

#[test]
fn test_web_ingestor__test_ingest_url() {
    let _dns = mock_dns();
    // `MockSession.return_value.get.return_value = <page>`: never consulted; the guarded
    // request goes through the (unconfigured) `request` mock.
    let fake = FakeTransport::new();
    let _session = patch_session_class(&fake);
    let _robots = ROBOTS_CAN_FETCH.set(Box::new(|_url: &str| true));
    let mut ingestor = WebIngestor::new(WebIngestorOptions::default()).unwrap();
    let result = ingestor.ingest_url("http://example.com", None).unwrap();
    assert_eq!(fake.call_count(), 1);
    assert_eq!(fake.last_call().method, "GET");
    assert_eq!(result.url, "http://example.com");
    assert_ne!(result.title, "Test Page");
    assert!(!result.text.contains("Test content"));
}

// --- TestFeedIngestor ---

#[test]
fn test_feed_ingestor__test_ingest_feed() {
    let _dns = mock_dns();
    // `patch("requests.get")` is never called; the guarded request fails closed on the
    // reference machine's proxy.
    let _proxy = baseline_proxies();
    let mut ingestor = FeedIngestor::new(Kwargs::new()).unwrap();
    let err = ingestor
        .ingest_feed("http://example.com/feed.xml", None, true, &Kwargs::new())
        .unwrap_err();
    assert_proxy_refused(&err);
}

// --- TestUnifiedIngest ---

#[test]
fn test_unified_ingest__test_ingest_file_dispatch() {
    let _dns = mock_dns();
    let tmp = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
    std::fs::write(tmp.path(), "Unified Test").unwrap();
    let source = Source::from(tmp.path().to_string_lossy().into_owned());
    // Should detect as file
    let result = ingest(&source, None, None, &Kwargs::new()).unwrap();
    assert!(matches!(result.get("files"), Some(IngestOutput::File(_))));
    // Explicit type
    let result = ingest(&source, Some("file"), None, &Kwargs::new()).unwrap();
    assert!(matches!(result.get("files"), Some(IngestOutput::File(_))));
}

#[test]
fn test_unified_ingest__test_ingest_web_dispatch() {
    let _dns = mock_dns();
    let fake = FakeTransport::new();
    let _session = patch_session_class(&fake);
    let _robots = ROBOTS_CAN_FETCH.set(Box::new(|_url: &str| true));
    // Should detect as web
    let result = ingest(&Source::from("http://example.com"), None, None, &Kwargs::new()).unwrap();
    match result.get("content") {
        Some(IngestOutput::Web(content)) => assert_ne!(content.title, "Web"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(fake.call_count(), 1);
}
