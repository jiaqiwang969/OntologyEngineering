//! File ingestion (`semantica/ingest/file_ingestor.py`): local files and directories with
//! type detection (extension, MIME type, magic numbers) and cloud storage (S3, GCS, Azure Blob)
//! through per-service client traits with REST implementations.

use std::sync::Arc;

use base64::Engine;
use indexmap::IndexMap;
use sem_utils::constants::{
    FILE_SIZE_LIMITS, SUPPORTED_AUDIO_FORMATS, SUPPORTED_DOCUMENT_FORMATS, SUPPORTED_IMAGE_FORMATS,
    SUPPORTED_VIDEO_FORMATS,
};
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, get_logger, get_progress_tracker};
use serde_json::{Map, Value, json};

use crate::config::{processing_error, validation_error};
use crate::support::aws::{self, AwsCredentials};
use crate::support::etree;
use crate::support::hooks::Hook;
use crate::support::kw::{self, Kwargs};
use crate::support::mimetypes;
use crate::support::pypath;

/// The content of a [`FileObject`] (`bytes`, or a `str` callers put there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    /// Raw bytes (what ingestion reads).
    Bytes(Vec<u8>),
    /// Text.
    Text(String),
}

/// `FileObject`: file object representation.
#[derive(Debug, Clone, PartialEq)]
pub struct FileObject {
    /// `path`.
    pub path: String,
    /// `name`.
    pub name: String,
    /// `size`.
    pub size: u64,
    /// `file_type`.
    pub file_type: String,
    /// `mime_type`.
    pub mime_type: Option<String>,
    /// `content`.
    pub content: Option<FileContent>,
    /// `metadata`.
    pub metadata: Map<String, Value>,
    /// `ingested_at`.
    pub ingested_at: PyDateTime,
}

impl FileObject {
    /// `FileObject(path, name, size, file_type, content=content)` with the other defaults.
    pub fn new(path: &str, name: &str, size: u64, file_type: &str, content: Option<FileContent>) -> FileObject {
        FileObject {
            path: path.to_string(),
            name: name.to_string(),
            size,
            file_type: file_type.to_string(),
            mime_type: None,
            content,
            metadata: Map::new(),
            ingested_at: PyDateTime::now(false),
        }
    }

    /// `file_object.text`: UTF-8, falling back to Latin-1; `""` without content.
    pub fn text(&self) -> String {
        match &self.content {
            None => String::new(),
            Some(FileContent::Text(t)) => t.clone(),
            Some(FileContent::Bytes(b)) => match std::str::from_utf8(b) {
                Ok(s) => s.to_string(),
                Err(_) => b.iter().map(|&c| c as char).collect(),
            },
        }
    }

    /// The raw bytes of the content (text encoded as UTF-8).
    pub fn content_bytes(&self) -> Option<Vec<u8>> {
        match &self.content {
            None => None,
            Some(FileContent::Bytes(b)) => Some(b.clone()),
            Some(FileContent::Text(t)) => Some(t.as_bytes().to_vec()),
        }
    }
}

/// `FileTypeDetector`: extension, MIME type and magic number detection.
#[derive(Debug, Clone)]
pub struct FileTypeDetector {
    /// All supported formats (documents, images, audio, video).
    pub supported_formats: Vec<String>,
    logger: Logger,
}

impl Default for FileTypeDetector {
    fn default() -> Self {
        FileTypeDetector::new()
    }
}

impl FileTypeDetector {
    /// `FileTypeDetector()`.
    pub fn new() -> FileTypeDetector {
        let supported_formats: Vec<String> = SUPPORTED_DOCUMENT_FORMATS
            .iter()
            .chain(SUPPORTED_IMAGE_FORMATS)
            .chain(SUPPORTED_AUDIO_FORMATS)
            .chain(SUPPORTED_VIDEO_FORMATS)
            .map(|s| s.to_string())
            .collect();
        let logger = get_logger("file_type_detector");
        logger.debug(format!(
            "File type detector initialized with {} supported formats",
            supported_formats.len()
        ));
        FileTypeDetector {
            supported_formats,
            logger,
        }
    }

    /// `detect_type(file_path, content=None)`: extension, then MIME type (existing files),
    /// then magic numbers; `"unknown"` otherwise.
    pub fn detect_type(&self, file_path: &str, content: Option<&[u8]>) -> String {
        let extension = pypath::suffix(file_path).trim_start_matches('.').to_lowercase();
        if !extension.is_empty() {
            self.logger.debug(format!("Detected file type by extension: {extension}"));
            return extension;
        }
        if pypath::exists(file_path)
            && let (Some(mime), _) = mimetypes::guess_type(&pypath::normalize(file_path))
            && let Some(ext) = mimetypes::guess_extension(&mime)
        {
            let detected = ext.trim_start_matches('.').to_lowercase();
            self.logger.debug(format!("Detected file type by MIME type: {detected}"));
            return detected;
        }
        if let Some(content) = content.filter(|c| !c.is_empty())
            && let Some(t) = self._detect_by_magic_numbers(content)
        {
            self.logger.debug(format!("Detected file type by magic numbers: {t}"));
            return t;
        }
        self.logger.warning(format!(
            "Could not determine file type for: {}",
            pypath::normalize(file_path)
        ));
        "unknown".to_string()
    }

    /// `_detect_by_magic_numbers(content)`.
    pub fn _detect_by_magic_numbers(&self, content: &[u8]) -> Option<String> {
        if content.len() < 4 {
            return None;
        }
        const MAGIC: [(&[u8], &str); 9] = [
            (b"\x25\x50\x44\x46", "pdf"),
            (b"%PDF", "pdf"),
            (b"\x50\x4b\x03\x04", "zip"),
            (b"\x89\x50\x4e\x47", "png"),
            (b"\xff\xd8\xff", "jpg"),
            (b"\x47\x49\x46\x38", "gif"),
            (b"PK\x03\x04", "zip"),
            (b"PAR1", "parquet"),
            (b"ARROW1\x00\x00", "arrow"),
        ];
        MAGIC
            .iter()
            .find(|(magic, _)| content.starts_with(magic))
            .map(|(_, t)| t.to_string())
    }

    /// `is_supported(file_type)`.
    pub fn is_supported(&self, file_type: &str) -> bool {
        self.supported_formats.contains(&file_type.to_lowercase())
    }
}

fn cloud_get(url: &str, headers: &[(String, String)]) -> Result<Vec<u8>, SemanticaError> {
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| SemanticaError::Runtime(e.to_string()))?;
    let mut request = client.get(url);
    for (k, v) in headers {
        request = request.header(k.as_str(), v.as_str());
    }
    let response = request
        .send()
        .map_err(|e| SemanticaError::Runtime(e.to_string()))?;
    let status = response.status();
    let body = response
        .bytes()
        .map_err(|e| SemanticaError::Runtime(e.to_string()))?
        .to_vec();
    if !status.is_success() {
        return Err(SemanticaError::Runtime(format!(
            "HTTP {}: {}",
            status.as_u16(),
            String::from_utf8_lossy(&body)
        )));
    }
    Ok(body)
}

/// The S3 operations the ingestor uses (`boto3.client("s3")`).
pub trait S3Api: Send {
    /// `get_paginator("list_objects_v2").paginate(Bucket=..., Prefix=...)`: response pages
    /// (`{"Contents": [{"Key", "Size", "LastModified", "ETag"}, ...]}`).
    fn list_objects_v2_pages(&self, bucket: &str, prefix: &str) -> Result<Vec<Value>, SemanticaError>;
    /// `get_object(Bucket=..., Key=...)["Body"].read()`.
    fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, SemanticaError>;
}

/// S3 over its REST API with SigV4 (virtual-hosted style, or path style with an
/// `endpoint_url`).
#[derive(Debug, Clone)]
pub struct S3RestClient {
    /// Credentials (anonymous requests when absent).
    pub credentials: Option<AwsCredentials>,
    /// Region.
    pub region: String,
    /// Custom endpoint (S3-compatible stores); path-style addressing is used with it.
    pub endpoint_url: Option<String>,
}

impl S3RestClient {
    fn request(&self, bucket: &str, path: &str, query: &[(String, String)]) -> Result<Vec<u8>, SemanticaError> {
        let (scheme, host, canonical_uri) = match &self.endpoint_url {
            Some(endpoint) => {
                let parsed = url::Url::parse(endpoint).map_err(|e| SemanticaError::Value(e.to_string()))?;
                let host = match parsed.port() {
                    Some(p) => format!("{}:{p}", parsed.host_str().unwrap_or("")),
                    None => parsed.host_str().unwrap_or("").to_string(),
                };
                (
                    parsed.scheme().to_string(),
                    host,
                    format!("/{}{}", aws::uri_encode(bucket, false), aws::uri_encode(path, true)),
                )
            }
            None => (
                "https".to_string(),
                format!("{bucket}.s3.{}.amazonaws.com", self.region),
                aws::uri_encode(path, true),
            ),
        };
        let canonical_uri = if canonical_uri.is_empty() { "/".to_string() } else { canonical_uri };
        let mut q: Vec<(String, String)> = query.to_vec();
        q.sort();
        let query_string = q
            .iter()
            .map(|(k, v)| format!("{}={}", aws::uri_encode(k, false), aws::uri_encode(v, false)))
            .collect::<Vec<_>>()
            .join("&");
        let url = if query_string.is_empty() {
            format!("{scheme}://{host}{canonical_uri}")
        } else {
            format!("{scheme}://{host}{canonical_uri}?{query_string}")
        };
        let headers = match &self.credentials {
            Some(c) => aws::sign_v4(c, &self.region, "s3", "GET", &host, &canonical_uri, query, &[], b"", &aws::amz_now()),
            None => Vec::new(),
        };
        cloud_get(&url, &headers)
    }
}

impl S3Api for S3RestClient {
    fn list_objects_v2_pages(&self, bucket: &str, prefix: &str) -> Result<Vec<Value>, SemanticaError> {
        let mut pages = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![
                ("list-type".to_string(), "2".to_string()),
                ("prefix".to_string(), prefix.to_string()),
            ];
            if let Some(t) = &token {
                query.push(("continuation-token".to_string(), t.clone()));
            }
            let body = self.request(bucket, "/", &query)?;
            let root = etree::fromstring_bytes(&body)?;
            let ns = |tag: &str| format!("{{http://s3.amazonaws.com/doc/2006-03-01/}}{tag}");
            let text = |e: &etree::Element, tag: &str| {
                e.find(&ns(tag))
                    .or_else(|| e.find(tag))
                    .and_then(|c| c.text.clone())
            };
            let mut contents = Vec::new();
            for item in root.findall(&ns("Contents")).into_iter().chain(root.findall("Contents")) {
                contents.push(json!({
                    "Key": text(item, "Key").unwrap_or_default(),
                    "Size": text(item, "Size").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
                    "LastModified": text(item, "LastModified"),
                    "ETag": text(item, "ETag"),
                }));
            }
            let mut page = Map::new();
            if !contents.is_empty() {
                page.insert("Contents".to_string(), Value::Array(contents));
            }
            pages.push(Value::Object(page));
            let truncated = text(&root, "IsTruncated").is_some_and(|t| t == "true");
            token = text(&root, "NextContinuationToken");
            if !truncated || token.is_none() {
                break;
            }
        }
        Ok(pages)
    }

    fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, SemanticaError> {
        self.request(bucket, &format!("/{key}"), &[])
    }
}

/// A GCS blob listing entry.
#[derive(Debug, Clone, PartialEq)]
pub struct GcsBlob {
    /// `blob.name`.
    pub name: String,
    /// `blob.size`.
    pub size: Value,
    /// `blob.updated`.
    pub updated: Value,
    /// `blob.etag`.
    pub etag: Value,
}

/// The GCS operations the ingestor uses (`storage.Client()`).
pub trait GcsApi: Send {
    /// `client.bucket(bucket).list_blobs(prefix=prefix)`.
    fn list_blobs(&self, bucket: &str, prefix: &str) -> Result<Vec<GcsBlob>, SemanticaError>;
    /// `client.bucket(bucket).blob(name).download_as_bytes()`.
    fn download_as_bytes(&self, bucket: &str, name: &str) -> Result<Vec<u8>, SemanticaError>;
}

/// GCS over the JSON API (bearer token from `token`/`access_token` or
/// `GOOGLE_OAUTH_ACCESS_TOKEN`; anonymous otherwise).
#[derive(Debug, Clone, Default)]
pub struct GcsRestClient {
    /// OAuth access token.
    pub access_token: Option<String>,
}

impl GcsRestClient {
    fn headers(&self) -> Vec<(String, String)> {
        match &self.access_token {
            Some(t) => vec![("Authorization".to_string(), format!("Bearer {t}"))],
            None => Vec::new(),
        }
    }
}

impl GcsApi for GcsRestClient {
    fn list_blobs(&self, bucket: &str, prefix: &str) -> Result<Vec<GcsBlob>, SemanticaError> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut url = format!(
                "https://storage.googleapis.com/storage/v1/b/{}/o?prefix={}",
                aws::uri_encode(bucket, false),
                aws::uri_encode(prefix, false)
            );
            if let Some(t) = &token {
                url.push_str(&format!("&pageToken={}", aws::uri_encode(t, false)));
            }
            let body = cloud_get(&url, &self.headers())?;
            let page: Value = serde_json::from_slice(&body).map_err(|e| SemanticaError::JsonDecode(e.to_string()))?;
            for item in page.get("items").and_then(Value::as_array).cloned().unwrap_or_default() {
                out.push(GcsBlob {
                    name: item.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    size: item
                        .get("size")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    updated: item.get("updated").cloned().unwrap_or(Value::Null),
                    etag: item.get("etag").cloned().unwrap_or(Value::Null),
                });
            }
            token = page.get("nextPageToken").and_then(Value::as_str).map(str::to_string);
            if token.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn download_as_bytes(&self, bucket: &str, name: &str) -> Result<Vec<u8>, SemanticaError> {
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
            aws::uri_encode(bucket, false),
            aws::uri_encode(name, false)
        );
        cloud_get(&url, &self.headers())
    }
}

/// An Azure blob listing entry.
#[derive(Debug, Clone, PartialEq)]
pub struct AzureBlob {
    /// `blob.name`.
    pub name: String,
    /// `blob.size`.
    pub size: Value,
    /// `blob.last_modified`.
    pub last_modified: Value,
    /// `blob.etag`.
    pub etag: Value,
}

/// The Azure Blob operations the ingestor uses (`BlobServiceClient.from_connection_string`).
pub trait AzureBlobApi: Send {
    /// `get_container_client(container).list_blobs(name_starts_with=prefix)`.
    fn list_blobs(&self, container: &str, prefix: &str) -> Result<Vec<AzureBlob>, SemanticaError>;
    /// `get_container_client(container).get_blob_client(blob).download_blob().readall()`.
    fn download_blob(&self, container: &str, blob: &str) -> Result<Vec<u8>, SemanticaError>;
}

/// Azure Blob storage over REST (Shared Key or SAS from the connection string).
#[derive(Clone)]
pub struct AzureBlobRestClient {
    /// Blob endpoint (`https://<account>.blob.core.windows.net`).
    pub endpoint: String,
    /// Account name.
    pub account_name: Option<String>,
    account_key: Option<Vec<u8>>,
    sas: Option<String>,
}

impl std::fmt::Debug for AzureBlobRestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzureBlobRestClient")
            .field("endpoint", &self.endpoint)
            .field("account_name", &self.account_name)
            .finish()
    }
}

impl AzureBlobRestClient {
    /// `BlobServiceClient.from_connection_string(conn)`.
    pub fn from_connection_string(conn: &str) -> Result<AzureBlobRestClient, SemanticaError> {
        let mut parts: IndexMap<String, String> = IndexMap::new();
        for piece in conn.split(';').filter(|p| !p.trim().is_empty()) {
            if let Some((k, v)) = piece.split_once('=') {
                parts.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        let account_name = parts.get("AccountName").cloned();
        let account_key = match parts.get("AccountKey") {
            Some(k) => Some(
                base64::engine::general_purpose::STANDARD
                    .decode(k)
                    .map_err(|e| SemanticaError::Value(format!("Invalid AccountKey: {e}")))?,
            ),
            None => None,
        };
        let endpoint = match parts.get("BlobEndpoint") {
            Some(e) => e.trim_end_matches('/').to_string(),
            None => {
                let protocol = parts
                    .get("DefaultEndpointsProtocol")
                    .cloned()
                    .unwrap_or_else(|| "https".to_string());
                let suffix = parts
                    .get("EndpointSuffix")
                    .cloned()
                    .unwrap_or_else(|| "core.windows.net".to_string());
                let account = account_name.clone().ok_or_else(|| {
                    SemanticaError::Value("Connection string missing required connection details.".to_string())
                })?;
                format!("{protocol}://{account}.blob.{suffix}")
            }
        };
        Ok(AzureBlobRestClient {
            endpoint,
            account_name,
            account_key,
            sas: parts.get("SharedAccessSignature").cloned(),
        })
    }

    fn get(&self, path: &str, query: &[(String, String)]) -> Result<Vec<u8>, SemanticaError> {
        let mut query_string = query
            .iter()
            .map(|(k, v)| format!("{}={}", aws::uri_encode(k, false), aws::uri_encode(v, false)))
            .collect::<Vec<_>>()
            .join("&");
        if let Some(sas) = &self.sas {
            if !query_string.is_empty() {
                query_string.push('&');
            }
            query_string.push_str(sas.trim_start_matches('?'));
        }
        let url = if query_string.is_empty() {
            format!("{}{}", self.endpoint, aws::uri_encode(path, true))
        } else {
            format!("{}{}?{query_string}", self.endpoint, aws::uri_encode(path, true))
        };
        let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let mut headers = vec![
            ("x-ms-date".to_string(), date.clone()),
            ("x-ms-version".to_string(), "2021-08-06".to_string()),
        ];
        if let (Some(account), Some(key)) = (&self.account_name, &self.account_key)
            && self.sas.is_none()
        {
            let canonical_headers = format!("x-ms-date:{date}\nx-ms-version:2021-08-06\n");
            let mut canonical_resource = format!("/{account}{}", aws::uri_encode(path, true));
            let mut q: Vec<(String, String)> = query.iter().map(|(k, v)| (k.to_lowercase(), v.clone())).collect();
            q.sort();
            for (k, v) in q {
                canonical_resource.push_str(&format!("\n{k}:{v}"));
            }
            let string_to_sign = format!("GET\n\n\n\n\n\n\n\n\n\n\n\n{canonical_headers}{canonical_resource}");
            let signature = base64::engine::general_purpose::STANDARD
                .encode(aws::hmac_sha256(key, string_to_sign.as_bytes()));
            headers.push(("Authorization".to_string(), format!("SharedKey {account}:{signature}")));
        }
        cloud_get(&url, &headers)
    }
}

impl AzureBlobApi for AzureBlobRestClient {
    fn list_blobs(&self, container: &str, prefix: &str) -> Result<Vec<AzureBlob>, SemanticaError> {
        let mut out = Vec::new();
        let mut marker: Option<String> = None;
        loop {
            let mut query = vec![
                ("restype".to_string(), "container".to_string()),
                ("comp".to_string(), "list".to_string()),
            ];
            if !prefix.is_empty() {
                query.push(("prefix".to_string(), prefix.to_string()));
            }
            if let Some(m) = &marker {
                query.push(("marker".to_string(), m.clone()));
            }
            let body = self.get(&format!("/{container}"), &query)?;
            let root = etree::fromstring_bytes(&body)?;
            for blob in root.findall(".//Blob") {
                let prop = |name: &str| {
                    blob.find(&format!("Properties/{name}"))
                        .and_then(|e| e.text.clone())
                };
                out.push(AzureBlob {
                    name: blob.find("Name").and_then(|e| e.text.clone()).unwrap_or_default(),
                    size: prop("Content-Length")
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    last_modified: prop("Last-Modified").map(Value::String).unwrap_or(Value::Null),
                    etag: prop("Etag").map(Value::String).unwrap_or(Value::Null),
                });
            }
            marker = root
                .find("NextMarker")
                .and_then(|e| e.text.clone())
                .filter(|m| !m.is_empty());
            if marker.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn download_blob(&self, container: &str, blob: &str) -> Result<Vec<u8>, SemanticaError> {
        self.get(&format!("/{container}/{blob}"), &[])
    }
}

/// Replacement for `boto3.client("s3", ...)` (`patch("boto3.client")`); receives the
/// ingestor's config.
pub static BOTO3_CLIENT: Hook<Box<dyn Fn(&Kwargs) -> Box<dyn S3Api>>> = Hook::new("boto3.client");

/// Replacement for `google.cloud.storage.Client()`.
pub static GCS_CLIENT: Hook<Box<dyn Fn() -> Box<dyn GcsApi>>> = Hook::new("google.cloud.storage.Client");

/// Replacement for `azure.storage.blob.BlobServiceClient.from_connection_string`.
pub static AZURE_FROM_CONNECTION_STRING: Hook<Box<dyn Fn(Option<&str>) -> Box<dyn AzureBlobApi>>> =
    Hook::new("azure.storage.blob.BlobServiceClient.from_connection_string");

/// What `FileIngestor.ingest_cloud` needs from a cloud ingestor.
pub trait CloudStorageApi: Send {
    /// `list_objects(bucket, prefix="", **filters)`.
    fn list_objects(&self, bucket: &str, prefix: &str, filters: &Kwargs) -> Result<Vec<Map<String, Value>>, SemanticaError>;
    /// `download_object(bucket, key, **options)`.
    fn download_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, SemanticaError>;
}

/// Replacement for the `CloudStorageIngestor` class `FileIngestor.ingest_cloud` instantiates.
pub static CLOUD_STORAGE_INGESTOR: Hook<Box<dyn Fn(&str, &Kwargs) -> Result<Box<dyn CloudStorageApi>, SemanticaError>>> =
    Hook::new("semantica.ingest.file_ingestor.CloudStorageIngestor");

enum CloudClient {
    S3(Box<dyn S3Api>),
    Gcs(Box<dyn GcsApi>),
    Azure(Box<dyn AzureBlobApi>),
}

/// `CloudStorageIngestor`: S3, GCS and Azure Blob listing and download.
pub struct CloudStorageIngestor {
    /// `provider` (`"s3"`, `"gcs"`, `"azure"`).
    pub provider: String,
    /// Provider configuration.
    pub config: Kwargs,
    client: CloudClient,
    logger: Logger,
}

impl std::fmt::Debug for CloudStorageIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudStorageIngestor")
            .field("provider", &self.provider)
            .finish()
    }
}

impl CloudStorageIngestor {
    /// `CloudStorageIngestor(provider, **config)` (`ValueError` for unknown providers).
    pub fn new(provider: &str, config: Kwargs) -> Result<CloudStorageIngestor, SemanticaError> {
        let provider = provider.to_lowercase();
        let client = match provider.as_str() {
            "s3" => CloudClient::S3(match BOTO3_CLIENT.get() {
                Some(f) => f(&config),
                None => Box::new(S3RestClient {
                    credentials: AwsCredentials::resolve(
                        kw::get_str(&config, "access_key_id").as_deref(),
                        kw::get_str(&config, "secret_access_key").as_deref(),
                    ),
                    region: kw::get_str(&config, "region").unwrap_or_else(|| "us-east-1".to_string()),
                    endpoint_url: kw::get_str(&config, "endpoint_url"),
                }),
            }),
            "gcs" => CloudClient::Gcs(match GCS_CLIENT.get() {
                Some(f) => f(),
                None => Box::new(GcsRestClient {
                    access_token: kw::get_str(&config, "access_token")
                        .or_else(|| kw::get_str(&config, "token"))
                        .or_else(|| std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN").ok()),
                }),
            }),
            "azure" => {
                let conn = kw::get_str(&config, "connection_string");
                CloudClient::Azure(match AZURE_FROM_CONNECTION_STRING.get() {
                    Some(f) => f(conn.as_deref()),
                    None => Box::new(AzureBlobRestClient::from_connection_string(conn.as_deref().unwrap_or(""))?),
                })
            }
            other => return Err(SemanticaError::Value(format!("Unsupported cloud provider: {other}"))),
        };
        Ok(CloudStorageIngestor {
            provider,
            config,
            client,
            logger: get_logger("cloud_storage_ingestor"),
        })
    }

    /// `list_objects(bucket, prefix="", **filters)`.
    pub fn list_objects(&self, bucket: &str, prefix: &str, filters: &Kwargs) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        let result = (|| -> Result<Vec<Map<String, Value>>, SemanticaError> {
            let mut objects = Vec::new();
            match &self.client {
                CloudClient::S3(client) => {
                    for page in client.list_objects_v2_pages(bucket, prefix)? {
                        if let Some(contents) = page.get("Contents").and_then(Value::as_array) {
                            for obj in contents {
                                let field = |k: &str| {
                                    obj.get(k)
                                        .cloned()
                                        .ok_or_else(|| SemanticaError::Key(k.to_string()))
                                };
                                objects.push(kw::obj([
                                    ("key", field("Key")?),
                                    ("size", field("Size")?),
                                    ("last_modified", field("LastModified")?),
                                    ("etag", field("ETag")?),
                                ]));
                            }
                        }
                    }
                }
                CloudClient::Gcs(client) => {
                    for blob in client.list_blobs(bucket, prefix)? {
                        objects.push(kw::obj([
                            ("key", Value::String(blob.name)),
                            ("size", blob.size),
                            ("last_modified", blob.updated),
                            ("etag", blob.etag),
                        ]));
                    }
                }
                CloudClient::Azure(client) => {
                    for blob in client.list_blobs(bucket, prefix)? {
                        objects.push(kw::obj([
                            ("key", Value::String(blob.name)),
                            ("size", blob.size),
                            ("last_modified", blob.last_modified),
                            ("etag", blob.etag),
                        ]));
                    }
                }
            }
            if !filters.is_empty() {
                objects = self._apply_filters(objects, filters);
            }
            Ok(objects)
        })();
        result.map_err(|e| {
            self.logger
                .error(format!("Failed to list objects from {}: {e}", self.provider));
            processing_error(format!("Failed to list objects: {e}"))
        })
    }

    /// `_apply_filters(objects, filters)`: `min_size`, `max_size`, `extensions`.
    pub fn _apply_filters(&self, objects: Vec<Map<String, Value>>, filters: &Kwargs) -> Vec<Map<String, Value>> {
        let size = |o: &Map<String, Value>| o.get("size").and_then(Value::as_f64).unwrap_or(0.0);
        let mut filtered = objects;
        if let Some(min) = filters.get("min_size").and_then(Value::as_f64) {
            filtered.retain(|o| size(o) >= min);
        }
        if let Some(max) = filters.get("max_size").and_then(Value::as_f64) {
            filtered.retain(|o| size(o) <= max);
        }
        if let Some(exts) = kw::get_str_list(filters, "extensions") {
            filtered.retain(|o| {
                let key = o.get("key").and_then(Value::as_str).unwrap_or("");
                exts.contains(&pypath::suffix(key).trim_start_matches('.').to_string())
            });
        }
        filtered
    }

    /// `download_object(bucket, key, **options)`.
    pub fn download_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, SemanticaError> {
        let result = match &self.client {
            CloudClient::S3(c) => c.get_object(bucket, key),
            CloudClient::Gcs(c) => c.download_as_bytes(bucket, key),
            CloudClient::Azure(c) => c.download_blob(bucket, key),
        };
        result.map_err(|e| {
            self.logger.error(format!(
                "Failed to download object {key} from {}: {e}",
                self.provider
            ));
            processing_error(format!("Failed to download object: {e}"))
        })
    }
}

impl CloudStorageApi for CloudStorageIngestor {
    fn list_objects(&self, bucket: &str, prefix: &str, filters: &Kwargs) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        CloudStorageIngestor::list_objects(self, bucket, prefix, filters)
    }

    fn download_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>, SemanticaError> {
        CloudStorageIngestor::download_object(self, bucket, key)
    }
}

/// Replacement for `FileIngestor.ingest_file` (`patch.object(ingestor, "ingest_file")`).
pub static INGEST_FILE: Hook<Box<dyn Fn(&str, &Kwargs) -> Result<FileObject, SemanticaError>>> =
    Hook::new("semantica.ingest.file_ingestor.FileIngestor.ingest_file");

type ProgressCallback = Box<dyn FnMut(usize, usize, &FileObject) + Send>;

/// `FileIngestor`: local filesystem and cloud storage ingestion.
pub struct FileIngestor {
    /// Merged configuration.
    pub config: Kwargs,
    /// The type detector.
    pub type_detector: FileTypeDetector,
    /// `supported_formats`.
    pub supported_formats: Vec<String>,
    /// Progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    cloud_providers: IndexMap<String, Box<dyn CloudStorageApi>>,
    progress_callback: Option<ProgressCallback>,
    logger: Logger,
}

impl std::fmt::Debug for FileIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileIngestor").field("config", &self.config).finish()
    }
}

impl Default for FileIngestor {
    fn default() -> Self {
        FileIngestor::new(Kwargs::new())
    }
}

impl FileIngestor {
    /// `FileIngestor(config=None, **kwargs)`.
    pub fn new(config: Kwargs) -> FileIngestor {
        let type_detector = FileTypeDetector::new();
        let ingestor = FileIngestor {
            supported_formats: type_detector.supported_formats.clone(),
            type_detector,
            config,
            progress_tracker: get_progress_tracker(),
            cloud_providers: IndexMap::new(),
            progress_callback: None,
            logger: get_logger("file_ingestor"),
        };
        ingestor.logger.info("File ingestor initialized");
        ingestor
    }

    /// `ingest(source, **options)`: a directory or a single file.
    pub fn ingest(&mut self, source: &str, options: &Kwargs) -> Result<Vec<FileObject>, SemanticaError> {
        let path = pypath::normalize(source);
        if pypath::is_dir(&path) {
            let recursive = options.get("recursive").map(kw::truthy).unwrap_or(true);
            let mut filters = options.clone();
            filters.remove("recursive");
            self.ingest_directory(&path, recursive, &filters)
        } else if pypath::is_file(&path) {
            Ok(vec![self.ingest_file(&path, options)?])
        } else {
            Err(validation_error(format!("Path not found: {path}")))
        }
    }

    /// `ingest_directory(directory_path, recursive=True, **filters)`.
    pub fn ingest_directory(&mut self, directory_path: &str, recursive: bool, filters: &Kwargs) -> Result<Vec<FileObject>, SemanticaError> {
        let dir = pypath::normalize(directory_path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&dir),
            Some("ingest"),
            Some("FileIngestor"),
            &format!("Directory: {}", pypath::name(&dir)),
            None,
        );
        let result = (|| -> Result<Vec<FileObject>, SemanticaError> {
            if !pypath::exists(&dir) {
                return Err(validation_error(format!("Directory not found: {dir}")));
            }
            if !pypath::is_dir(&dir) {
                return Err(validation_error(format!("Path is not a directory: {dir}")));
            }
            let mut scan_filters = filters.clone();
            scan_filters.insert("recursive".to_string(), Value::Bool(recursive));
            let files = self.scan_directory(&dir, &scan_filters)?;
            let total = files.len();
            self.progress_tracker
                .update_tracking(&tracking_id, None, &format!("Processing {total} files"));
            let mut objects = Vec::new();
            for (idx, info) in files.iter().enumerate() {
                let idx = idx + 1;
                let path = info.get("path").and_then(Value::as_str).unwrap_or("").to_string();
                match self.ingest_file(&path, info) {
                    Ok(obj) => {
                        self.progress_tracker.update_progress(
                            &tracking_id,
                            idx as i64,
                            total as i64,
                            &format!("Processing file {idx}/{total}: {}", pypath::name(&path)),
                        );
                        if let Some(cb) = self.progress_callback.as_mut() {
                            cb(idx, total, &obj);
                        }
                        self.logger
                            .debug(format!("Ingested file {idx}/{total}: {path}"));
                        objects.push(obj);
                    }
                    Err(e) => {
                        self.logger.error(format!("Failed to ingest file {path}: {e}"));
                        if kw::get_truthy(&self.config, "fail_fast", false) {
                            return Err(processing_error(format!("Failed to ingest file: {e}")));
                        }
                    }
                }
            }
            self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Ingested {} files", objects.len()),
                None,
            );
            Ok(objects)
        })();
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `ingest_file(file_path, **options)`: `read_content` (default True) plus metadata.
    pub fn ingest_file(&mut self, file_path: &str, options: &Kwargs) -> Result<FileObject, SemanticaError> {
        if let Some(f) = INGEST_FILE.get() {
            return f(file_path, options);
        }
        let path = pypath::normalize(file_path);
        let file_name = pypath::name(&path);
        let tracking_id = self.progress_tracker.start_tracking(
            Some(&path),
            Some("ingest"),
            Some("FileIngestor"),
            &format!("File: {file_name}"),
            None,
        );
        let result = (|| -> Result<FileObject, SemanticaError> {
            if !pypath::exists(&path) {
                return Err(validation_error(format!("File not found: {path}")));
            }
            if !pypath::is_file(&path) {
                return Err(validation_error(format!("Path is not a file: {path}")));
            }
            let size = std::fs::metadata(&path)
                .map_err(|e| SemanticaError::from_io(&e, Some(std::path::Path::new(&path))))?
                .len();
            let max_size = FILE_SIZE_LIMITS.get("MAX_DOCUMENT_SIZE").copied().unwrap_or(104_857_600);
            if size > max_size {
                return Err(validation_error(format!(
                    "File size {} bytes exceeds maximum {} bytes ({file_name})",
                    thousands(size),
                    thousands(max_size)
                )));
            }
            let file_type = self.type_detector.detect_type(&path, None);
            if !self.type_detector.is_supported(&file_type) {
                self.logger.warning(format!(
                    "Unsupported file type '{file_type}' for file: {path}. Processing may be limited."
                ));
            }
            let read_content = options.get("read_content").cloned().unwrap_or(Value::Bool(true));
            let mut content = None;
            if kw::truthy(&read_content) {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        self.logger.debug(format!(
                            "Read {} bytes from {file_name}",
                            thousands(bytes.len() as u64)
                        ));
                        content = Some(FileContent::Bytes(bytes));
                    }
                    Err(e) => {
                        let err = SemanticaError::from_io(&e, Some(std::path::Path::new(&path)));
                        self.logger
                            .error(format!("Failed to read file content from {path}: {err}"));
                        return Err(processing_error(format!("Failed to read file: {err}")));
                    }
                }
            }
            let (mime_type, _) = mimetypes::guess_type(&path);
            let mut metadata = Map::new();
            metadata.insert("extension".to_string(), Value::String(pypath::suffix(&path)));
            metadata.insert("parent".to_string(), Value::String(pypath::parent(&path)));
            metadata.insert(
                "is_supported".to_string(),
                Value::Bool(self.type_detector.is_supported(&file_type)),
            );
            metadata.insert("read_content".to_string(), read_content);
            for (k, v) in options {
                metadata.insert(k.clone(), v.clone());
            }
            let obj = FileObject {
                path: pypath::absolute(&path),
                name: file_name.clone(),
                size,
                file_type: file_type.clone(),
                mime_type,
                content,
                metadata,
                ingested_at: PyDateTime::now(false),
            };
            self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Ingested {file_name} ({file_type})"),
                None,
            );
            self.logger
                .debug(format!("Successfully ingested file: {file_name} ({file_type})"));
            Ok(obj)
        })();
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `ingest_cloud(provider, bucket, prefix="", **config)`.
    pub fn ingest_cloud(&mut self, provider: &str, bucket: &str, prefix: &str, config: &Kwargs) -> Result<Vec<FileObject>, SemanticaError> {
        if !self.cloud_providers.contains_key(provider) {
            let ingestor: Box<dyn CloudStorageApi> = match CLOUD_STORAGE_INGESTOR.get() {
                Some(f) => f(provider, config)?,
                None => Box::new(CloudStorageIngestor::new(provider, config.clone())?),
            };
            self.cloud_providers.insert(provider.to_string(), ingestor);
        }
        let objects = self.cloud_providers[provider].list_objects(bucket, prefix, &Kwargs::new())?;
        let mut files = Vec::new();
        for info in objects {
            let key = info.get("key").and_then(Value::as_str).unwrap_or("").to_string();
            let result = (|| -> Result<FileObject, SemanticaError> {
                let content = self.cloud_providers[provider].download_object(bucket, &key)?;
                let file_type = self
                    .type_detector
                    ._detect_by_magic_numbers(&content)
                    .unwrap_or_else(|| self.type_detector.detect_type(&key, None));
                let size = info.get("size").and_then(Value::as_u64).unwrap_or(0);
                let mut obj = FileObject::new(&key, &pypath::name(&key), size, &file_type, Some(FileContent::Bytes(content)));
                obj.metadata = kw::obj([
                    ("provider", Value::String(provider.to_string())),
                    ("bucket", Value::String(bucket.to_string())),
                    ("etag", info.get("etag").cloned().unwrap_or(Value::Null)),
                    (
                        "last_modified",
                        Value::String(sem_utils::pycompat::fmt::py_str(
                            info.get("last_modified").unwrap_or(&Value::Null),
                        )),
                    ),
                ]);
                Ok(obj)
            })();
            match result {
                Ok(obj) => files.push(obj),
                Err(e) => {
                    self.logger.error(format!("Failed to ingest object {key}: {e}"));
                    if kw::get_truthy(&self.config, "fail_fast", false) {
                        return Err(processing_error(format!("Failed to ingest object: {e}")));
                    }
                }
            }
        }
        Ok(files)
    }

    /// `scan_directory(directory_path, **filters)`: file information without processing
    /// (`recursive`, `extensions`, `min_size`, `max_size`, `pattern`).
    pub fn scan_directory(&self, directory_path: &str, filters: &Kwargs) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        let dir = pypath::normalize(directory_path);
        let mut filters = filters.clone();
        let recursive = filters.remove("recursive").map(|v| kw::truthy(&v)).unwrap_or(true);
        let entries = if recursive {
            pypath::rglob_all(&dir)
        } else {
            pypath::iterdir(&dir)
        };
        let mut files: Vec<Map<String, Value>> = entries
            .into_iter()
            .filter(|p| pypath::is_file(p))
            .map(|p| self._get_file_info(&p))
            .collect();
        if !filters.is_empty() {
            files = self._apply_file_filters(files, &filters);
        }
        Ok(files)
    }

    /// `_get_file_info(file_path)`.
    pub fn _get_file_info(&self, path: &str) -> Map<String, Value> {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        kw::obj([
            ("path", Value::String(path.to_string())),
            ("name", Value::String(pypath::name(path))),
            ("size", json!(size)),
            (
                "extension",
                Value::String(pypath::suffix(path).trim_start_matches('.').to_string()),
            ),
            ("modified", json!(pypath::mtime(path).unwrap_or(0.0))),
        ])
    }

    /// `_apply_file_filters(files, filters)`.
    pub fn _apply_file_filters(&self, files: Vec<Map<String, Value>>, filters: &Kwargs) -> Vec<Map<String, Value>> {
        let size = |f: &Map<String, Value>| f.get("size").and_then(Value::as_f64).unwrap_or(0.0);
        let mut filtered = files;
        if let Some(exts) = kw::get_str_list(filters, "extensions") {
            let exts: Vec<String> = exts.iter().map(|e| e.trim_start_matches('.').to_string()).collect();
            filtered.retain(|f| exts.iter().any(|e| Some(e.as_str()) == f.get("extension").and_then(Value::as_str)));
        }
        if let Some(min) = filters.get("min_size").and_then(Value::as_f64) {
            filtered.retain(|f| size(f) >= min);
        }
        if let Some(max) = filters.get("max_size").and_then(Value::as_f64) {
            filtered.retain(|f| size(f) <= max);
        }
        if let Some(pattern) = kw::get_str(filters, "pattern") {
            filtered.retain(|f| pypath::fnmatch(f.get("name").and_then(Value::as_str).unwrap_or(""), &pattern));
        }
        filtered
    }

    /// `set_progress_callback(callback)`: called as `callback(index, total, file_object)`.
    pub fn set_progress_callback(&mut self, callback: impl FnMut(usize, usize, &FileObject) + Send + 'static) {
        self.progress_callback = Some(Box::new(callback));
    }
}

/// Python's `f"{n:,}"`.
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
