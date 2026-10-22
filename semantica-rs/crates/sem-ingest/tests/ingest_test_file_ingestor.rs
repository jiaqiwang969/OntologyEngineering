//! Port of `tests/ingest/test_file_ingestor.py`. The cloud SDK modules the original stubs in
//! `sys.modules` and patches (`boto3.client`, `google.cloud.storage.Client`,
//! `BlobServiceClient.from_connection_string`, `CloudStorageIngestor`) are the crate's
//! replacement hooks with in-memory fakes. Every test installs `conftest.mock_dns` first.
#![allow(non_snake_case)]

mod common;

use std::cell::Cell;
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use common::*;
use sem_ingest::file_ingestor::{
    AZURE_FROM_CONNECTION_STRING, AzureBlob, AzureBlobApi, BOTO3_CLIENT, CLOUD_STORAGE_INGESTOR,
    CloudStorageApi, CloudStorageIngestor, FileContent, FileIngestor, FileObject, FileTypeDetector,
    GCS_CLIENT, GcsApi, GcsBlob, INGEST_FILE, S3Api,
};
use sem_ingest::support::kw::Kwargs;
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

fn exception(message: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "builtins".to_string(),
        type_name: "Exception".to_string(),
        message: message.to_string(),
    }
}

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

/// The `temp_files` fixture.
fn temp_files() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("test.txt"), "Hello World").unwrap(); // 11 bytes
    // Binary file (PDF signature)
    std::fs::write(root.join("test.pdf"), b"%PDF-1.4 content").unwrap();
    // Subdirectory
    std::fs::create_dir(root.join("subdir")).unwrap();
    std::fs::write(root.join("subdir").join("sub.log"), "Log content").unwrap();
    // Latin-1 file to test encoding fallback (4 bytes)
    std::fs::write(root.join("latin.txt"), b"Caf\xe9").unwrap();
    dir
}

fn text(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

// --- FileObject Tests ---

#[test]
fn test_file_object_text_decoding() {
    let _dns = mock_dns();
    let f1 = FileObject::new("p", "n", 1, "txt", Some(FileContent::Bytes(b"Hello".to_vec())));
    let f2 = FileObject::new("p", "n", 1, "txt", Some(FileContent::Bytes(b"Caf\xe9".to_vec())));
    let f3 = FileObject::new("p", "n", 1, "txt", None);
    let f4 = FileObject::new("p", "n", 1, "txt", Some(FileContent::Text("Already String".to_string())));
    assert_eq!(f1.text(), "Hello");
    assert_eq!(f2.text(), "Café");
    assert_eq!(f3.text(), "");
    assert_eq!(f4.text(), "Already String");
}

// --- FileTypeDetector Tests ---

#[test]
fn test_type_detector_extended() {
    let _dns = mock_dns();
    let detector = FileTypeDetector::new();
    let png_sig = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a";
    // The original patches `Path.exists` and `mimetypes.guess_type`; the extension wins first.
    let is_mp4 = detector.detect_type("movie.mp4", None);
    let detected_gz = detector.detect_type("file.tar.gz", None);
    let detected_png = detector.detect_type("test", Some(png_sig));
    let detected_unknown = detector.detect_type("unknown", None);
    assert_eq!(detected_gz, "gz");
    assert_eq!(is_mp4, "mp4");
    assert_eq!(detected_png, "png");
    assert_eq!(detected_unknown, "unknown");
}

// --- CloudStorageIngestor Tests ---

/// `mock_boto.return_value`.
#[derive(Default)]
struct FakeS3 {
    pages: Vec<Value>,
    object: Vec<u8>,
    paginator_error: Option<String>,
    get_object_error: Option<String>,
}

impl S3Api for FakeS3 {
    fn list_objects_v2_pages(&self, _bucket: &str, _prefix: &str) -> Result<Vec<Value>, SemanticaError> {
        match &self.paginator_error {
            Some(m) => Err(exception(m)),
            None => Ok(self.pages.clone()),
        }
    }

    fn get_object(&self, _bucket: &str, _key: &str) -> Result<Vec<u8>, SemanticaError> {
        match &self.get_object_error {
            Some(m) => Err(exception(m)),
            None => Ok(self.object.clone()),
        }
    }
}

#[test]
fn test_cloud_storage_s3() {
    let _dns = mock_dns();
    let _boto = BOTO3_CLIENT.set(Box::new(|_config: &Kwargs| {
        Box::new(FakeS3 {
            pages: vec![json!({"Contents": [{"Key": "doc.txt", "Size": 100, "LastModified": "2025", "ETag": "tag"}]})],
            object: b"s3_data".to_vec(),
            ..FakeS3::default()
        }) as Box<dyn S3Api>
    }));
    let ingestor = CloudStorageIngestor::new(
        "s3",
        kwargs(json!({"access_key_id": "x", "secret_access_key": "y"})),
    )
    .unwrap();
    let objects = ingestor.list_objects("bucket", "", &Kwargs::new()).unwrap();
    let content = ingestor.download_object("bucket", "doc.txt").unwrap();
    assert_eq!(objects[0]["key"], "doc.txt");
    assert_eq!(content, b"s3_data");
}

struct FakeGcs;

impl GcsApi for FakeGcs {
    fn list_blobs(&self, _bucket: &str, _prefix: &str) -> Result<Vec<GcsBlob>, SemanticaError> {
        Ok(vec![GcsBlob {
            name: "gcs.txt".to_string(),
            size: json!(200),
            updated: json!("2025"),
            etag: json!("tag"),
        }])
    }

    fn download_as_bytes(&self, _bucket: &str, _name: &str) -> Result<Vec<u8>, SemanticaError> {
        Ok(b"gcs_data".to_vec())
    }
}

#[test]
fn test_cloud_storage_gcs() {
    let _dns = mock_dns();
    let _gcs = GCS_CLIENT.set(Box::new(|| Box::new(FakeGcs) as Box<dyn GcsApi>));
    let ingestor = CloudStorageIngestor::new("gcs", Kwargs::new()).unwrap();
    let objects = ingestor.list_objects("bucket", "", &Kwargs::new()).unwrap();
    let content = ingestor.download_object("bucket", "gcs.txt").unwrap();
    assert_eq!(objects[0]["key"], "gcs.txt");
    assert_eq!(content, b"gcs_data");
}

struct FakeAzure;

impl AzureBlobApi for FakeAzure {
    fn list_blobs(&self, _container: &str, _prefix: &str) -> Result<Vec<AzureBlob>, SemanticaError> {
        Ok(vec![AzureBlob {
            name: "azure.txt".to_string(),
            size: json!(300),
            last_modified: json!("2025"),
            etag: json!("tag"),
        }])
    }

    fn download_blob(&self, _container: &str, _blob: &str) -> Result<Vec<u8>, SemanticaError> {
        Ok(b"azure_data".to_vec())
    }
}

#[test]
fn test_cloud_storage_azure() {
    let _dns = mock_dns();
    let _azure = AZURE_FROM_CONNECTION_STRING.set(Box::new(|_conn: Option<&str>| {
        Box::new(FakeAzure) as Box<dyn AzureBlobApi>
    }));
    let ingestor = CloudStorageIngestor::new("azure", kwargs(json!({"connection_string": "conn"}))).unwrap();
    let objects = ingestor.list_objects("bucket", "", &Kwargs::new()).unwrap();
    let content = ingestor.download_object("bucket", "azure.txt").unwrap();
    assert_eq!(objects[0]["key"], "azure.txt");
    assert_eq!(content, b"azure_data");
}

#[test]
fn test_cloud_storage_invalid() {
    let _dns = mock_dns();
    let err = CloudStorageIngestor::new("dropbox", Kwargs::new()).unwrap_err();
    assert!(err.is_value_error(), "{err:?}");
}

#[test]
fn test_cloud_storage_list_error() {
    let _dns = mock_dns();
    // Raise error on the METHOD call, not the constructor
    let _boto = BOTO3_CLIENT.set(Box::new(|_config: &Kwargs| {
        Box::new(FakeS3 {
            paginator_error: Some("Auth Fail".to_string()),
            ..FakeS3::default()
        }) as Box<dyn S3Api>
    }));
    let ingestor = CloudStorageIngestor::new("s3", Kwargs::new()).unwrap();
    let err = ingestor.list_objects("bucket", "", &Kwargs::new()).unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
}

#[test]
fn test_cloud_storage_download_error() {
    let _dns = mock_dns();
    let _boto = BOTO3_CLIENT.set(Box::new(|_config: &Kwargs| {
        Box::new(FakeS3 {
            get_object_error: Some("Fail".to_string()),
            ..FakeS3::default()
        }) as Box<dyn S3Api>
    }));
    let ingestor = CloudStorageIngestor::new("s3", Kwargs::new()).unwrap();
    let err = ingestor.download_object("bucket", "key").unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
}

// --- FileIngestor Tests ---

#[test]
fn test_ingest_directory_recursive() {
    let _dns = mock_dns();
    let temp = temp_files();
    let mut ingestor = FileIngestor::default();
    let results = ingestor
        .ingest_directory(&text(temp.path()), true, &Kwargs::new())
        .unwrap();
    assert!(results.len() >= 3);
}

#[test]
fn test_ingest_directory_non_recursive() {
    let _dns = mock_dns();
    let temp = temp_files();
    let mut ingestor = FileIngestor::default();
    let results = ingestor
        .ingest_directory(&text(temp.path()), false, &Kwargs::new())
        .unwrap();
    let has_sub_log = results.iter().any(|f| f.name.contains("sub.log"));
    assert_eq!(results.len(), 3);
    assert!(!has_sub_log);
}

#[test]
fn test_ingest_file_callback() {
    let _dns = mock_dns();
    let temp = temp_files();
    let mut ingestor = FileIngestor::default();
    let called = Arc::new(Mutex::new(false));
    let flag = called.clone();
    ingestor.set_progress_callback(move |_, _, _| *flag.lock().unwrap() = true);
    ingestor
        .ingest_directory(&text(temp.path()), false, &Kwargs::new())
        .unwrap();
    assert!(*called.lock().unwrap());
}

#[test]
fn test_ingest_file_fail_fast() {
    let _dns = mock_dns();
    let temp = temp_files();
    let mut ingestor = FileIngestor::new(kwargs(json!({"fail_fast": true})));
    let _boom = INGEST_FILE.set(Box::new(|_path: &str, _options: &Kwargs| Err(exception("Boom"))));
    let err = ingestor
        .ingest_directory(&text(temp.path()), true, &Kwargs::new())
        .unwrap_err();
    assert_eq!(err.type_name(), "ProcessingError");
}

#[test]
fn test_ingest_alias() {
    let _dns = mock_dns();
    let temp = temp_files();
    let mut ingestor = FileIngestor::default();
    let res_dir = ingestor.ingest(&text(temp.path()), &Kwargs::new()).unwrap();
    let res_file = ingestor
        .ingest(&text(&temp.path().join("test.txt")), &Kwargs::new())
        .unwrap();
    assert!(!res_dir.is_empty());
    assert_eq!(res_file.len(), 1);
    let err = ingestor.ingest("ghost_path", &Kwargs::new()).unwrap_err();
    assert!(err.is_validation_error());
}

/// `mock_cloud_cls.return_value` with `download_object.side_effect = [b"good", Exception]`.
struct FakeCloud {
    downloads: Mutex<VecDeque<Result<Vec<u8>, SemanticaError>>>,
}

impl CloudStorageApi for FakeCloud {
    fn list_objects(&self, _bucket: &str, _prefix: &str, _filters: &Kwargs) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        Ok(vec![
            kwargs(json!({"key": "good.txt", "size": 10, "last_modified": "2025", "etag": "1"})),
            kwargs(json!({"key": "bad.txt", "size": 10, "last_modified": "2025", "etag": "2"})),
        ])
    }

    fn download_object(&self, _bucket: &str, _key: &str) -> Result<Vec<u8>, SemanticaError> {
        self.downloads.lock().unwrap().pop_front().unwrap_or_else(|| Err(exception("StopIteration")))
    }
}

#[test]
fn test_ingest_cloud_loop_errors() {
    let _dns = mock_dns();
    let created = Rc::new(Cell::new(0));
    let counter = created.clone();
    let _cloud = CLOUD_STORAGE_INGESTOR.set(Box::new(move |_provider: &str, _config: &Kwargs| {
        counter.set(counter.get() + 1);
        Ok(Box::new(FakeCloud {
            downloads: Mutex::new(VecDeque::from(vec![Ok(b"good".to_vec()), Err(exception("Bad dl"))])),
        }) as Box<dyn CloudStorageApi>)
    }));
    let mut ingestor = FileIngestor::new(kwargs(json!({"fail_fast": false})));
    let results = ingestor.ingest_cloud("s3", "bucket", "", &Kwargs::new()).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "good.txt");
    assert_eq!(created.get(), 1);
}

#[test]
fn test_scan_directory_filters() {
    let _dns = mock_dns();
    let temp = temp_files();
    let ingestor = FileIngestor::default();
    // latin.txt is 4 bytes. 4 <= 5 is True. So we expect latin.txt to survive.
    let res_max = ingestor
        .scan_directory(&text(temp.path()), &kwargs(json!({"max_size": 5})))
        .unwrap();
    // All files are small.
    let res_min = ingestor
        .scan_directory(&text(temp.path()), &kwargs(json!({"min_size": 1})))
        .unwrap();
    assert_eq!(res_max.len(), 1); // Expect latin.txt
    assert!(res_min.len() >= 3);
}
