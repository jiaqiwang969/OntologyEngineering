//! Port of `tests/ingest/test_submodules.py`. Library-level patches (`requests.Session`,
//! `MCPClient`, `get_progress_tracker`, `KafkaProcessor`, the DuckDB / Elasticsearch / Google
//! / Hugging Face / MongoDB / git client entry points) are the crate's replacement hooks with
//! fakes. Every test installs `conftest.mock_dns` first.
#![allow(non_snake_case)]

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use indexmap::IndexMap;
use sem_ingest::api_ingestor::{EndpointRequest, RESTIngestor};
use sem_ingest::mcp_client::{MCPClient, MCPResource, MCPTool, McpClientApi};
use sem_ingest::mcp_ingestor::{GET_PROGRESS_TRACKER, MCP_CLIENT, MCPIngestor};
use sem_ingest::repo_ingestor::{self, GitApi, GitCommit, GitRepo, RepoIngestor};
use sem_ingest::stream_ingestor::{KAFKA_PROCESSOR, StreamIngestor, StreamKind, StreamProcessor};
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

// --- TestRESTIngestor ---

#[test]
fn test_rest_ingestor__test_ingest_endpoint() {
    let _dns = mock_dns();
    let fake = FakeTransport::returning(
        Response::new(200)
            .with_header("Content-Type", "application/json")
            .with_json(json!({"key": "value"})),
    );
    let _session = patch_session_class(&fake);
    let mut ingestor = RESTIngestor::new(Kwargs::new()).unwrap();
    let data = ingestor
        .ingest_endpoint("https://api.example.com/data", &EndpointRequest::default())
        .unwrap();
    // If response.json() is mocked to return {"key": "value"}, data.data should be that dict
    assert_eq!(data.data, json!({"key": "value"}));
    assert_eq!(data.endpoint, "https://api.example.com/data");
    assert_eq!(data.response_status, 200);
}

#[test]
fn test_rest_ingestor__test_paginated_fetch() {
    let _dns = mock_dns();
    // The loop continues while `has_more` or `next` is present, so the mock data uses "next".
    let fake = FakeTransport::sequence(vec![
        Response::new(200).with_json(json!({"items": [1, 2], "next": "https://api.example.com/data?page=2"})),
        Response::new(200).with_json(json!({"items": [3, 4], "next": null})),
    ]);
    let _session = patch_session_class(&fake);
    let mut ingestor = RESTIngestor::new(Kwargs::new()).unwrap();
    let results = ingestor
        .paginated_fetch(
            "https://api.example.com/data",
            100,
            "page",
            "size",
            None,
            &EndpointRequest::default(),
        )
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].data["items"], json!([1, 2]));
    assert_eq!(results[1].data["items"], json!([3, 4]));
}

// --- TestMCPIngestor ---

/// `MockClient.return_value`.
struct FakeMcpClient {
    connected: bool,
}

impl McpClientApi for FakeMcpClient {
    fn connect(&mut self) -> Result<bool, SemanticaError> {
        // An unconfigured MagicMock return value is truthy.
        self.connected = true;
        Ok(true)
    }

    fn disconnect(&mut self) {
        self.connected = false;
    }

    fn list_resources(&mut self) -> Result<Vec<MCPResource>, SemanticaError> {
        Ok(vec![MCPResource::new("mcp://res1", "Res1")])
    }

    fn list_tools(&mut self) -> Result<Vec<MCPTool>, SemanticaError> {
        Ok(Vec::new())
    }

    fn read_resource(&mut self, _uri: &str) -> Result<Value, SemanticaError> {
        Ok(json!("Resource Content"))
    }

    fn call_tool(&mut self, _name: &str, _arguments: Option<&Map<String, Value>>) -> Result<Value, SemanticaError> {
        Ok(Value::Null)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[test]
fn test_mcp_ingestor__test_connect_and_ingest() {
    let _dns = mock_dns();
    let _client = MCP_CLIENT.set(Box::new(
        |_url: Option<&str>, _headers: Option<IndexMap<String, String>>, _config: &Kwargs| {
            Ok(Box::new(FakeMcpClient { connected: false }) as Box<dyn McpClientApi>)
        },
    ));
    let (tracker, display) = recording_tracker();
    let tracker_for_hook = tracker.clone();
    let _tracker = GET_PROGRESS_TRACKER.set(Box::new(move || tracker_for_hook.clone()));

    let mut ingestor = MCPIngestor::new(Kwargs::new());
    ingestor
        .connect("server1", Some("http://localhost:8000"), None, &Kwargs::new())
        .unwrap();
    // List resources
    let resources = ingestor.list_available_resources("server1").unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].name, "Res1");
    // Ingest resource
    let data = ingestor
        .ingest_resources("server1", Some(&["mcp://res1".to_string()]), None)
        .unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0].content, json!("Resource Content"));
    assert_eq!(data[0].server_name, "server1");
    // Verify tracker usage: a started item, then status updates.
    let items = recorded(&display);
    assert!(items.iter().any(|i| i.message == "Ingesting resources from server1"));
    assert!(items.iter().any(|i| i.message == "Reading resource: mcp://res1 (1/1)"));
}

// --- TestMCPClient ---

fn init_response() -> Value {
    json!({"jsonrpc": "2.0", "result": {"serverInfo": {"name": "test", "version": "1.0"}}, "id": 1})
}

fn tool_response() -> Value {
    json!({"jsonrpc": "2.0", "result": {"content": [{"type": "text", "text": "Tool Result"}]}, "id": 2})
}

#[test]
fn test_mcp_client__test_call_tool() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![
        Response::new(200).with_json(init_response()),
        Response::new(200).with_json(tool_response()),
    ]);
    let _session = patch_session_request(&fake);
    let mut client = MCPClient::new(Some("http://localhost:8000"), None, None, Kwargs::new()).unwrap();
    client.connect().unwrap();
    let args = json!({"arg": "val"}).as_object().cloned().unwrap();
    client.call_tool("my_tool", Some(&args)).unwrap();
}

#[test]
fn test_mcp_client__test_call_tool_mock_check() {
    let _dns = mock_dns();
    let fake = FakeTransport::sequence(vec![
        Response::new(200).with_json(init_response()),
        Response::new(200).with_json(tool_response()),
    ]);
    let _session = patch_session_request(&fake);
    let mut client = MCPClient::new(Some("http://localhost:8000"), None, None, Kwargs::new()).unwrap();
    client.connect().unwrap();
    let args = json!({"arg": "val"}).as_object().cloned().unwrap();
    let result = client.call_tool("my_tool", Some(&args)).unwrap();
    assert_eq!(result["content"], json!([{"type": "text", "text": "Tool Result"}]));
}

// --- TestStreamIngestor ---

#[test]
fn test_stream_ingestor__test_ingest_kafka() {
    let _dns = mock_dns();
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let _processor = KAFKA_PROCESSOR.set(Box::new(move |_topic: &str, _servers: &[String], options| {
        counter.set(counter.get() + 1);
        Ok(StreamProcessor::with_source(StreamKind::Kafka, Map::new(), options.clone(), None))
    }));
    let mut ingestor = StreamIngestor::new(Kwargs::new());
    let processor = ingestor
        .ingest_kafka("topic", &["localhost:9092".to_string()], Default::default())
        .unwrap();
    assert!(!processor.running());
    assert_eq!(calls.get(), 1);
}

// --- TestRepoIngestor ---

/// `MockRepo.return_value` with `iter_commits.return_value = [mock_commit]`.
struct MockRepoWithCommit;

impl GitRepo for MockRepoWithCommit {
    fn checkout(&mut self, _branch: &str) -> Result<(), SemanticaError> {
        Ok(())
    }
    fn branches(&self) -> Result<Vec<String>, SemanticaError> {
        Ok(Vec::new())
    }
    fn tags(&self) -> Result<Vec<String>, SemanticaError> {
        Ok(Vec::new())
    }
    fn remotes(&self) -> Result<Vec<(String, String)>, SemanticaError> {
        Ok(vec![("origin".to_string(), "<MagicMock>".to_string())])
    }
    fn has_heads(&self) -> Result<bool, SemanticaError> {
        Ok(true)
    }
    fn head_commit(&self) -> Result<GitCommit, SemanticaError> {
        Ok(mock_commit())
    }
    fn iter_commits(&self, _args: &[String]) -> Result<Vec<GitCommit>, SemanticaError> {
        Ok(vec![mock_commit()])
    }
}

/// `mock_commit`: hexsha, message and author name set; MagicMock's `__float__` (1.0) is
/// what `datetime.fromtimestamp` sees, and its stats iterate as empty.
fn mock_commit() -> GitCommit {
    GitCommit {
        hexsha: "abc1234".to_string(),
        message: "Initial commit".to_string(),
        author: "Test Author".to_string(),
        committed_date: 1.0,
        files: Vec::new(),
        insertions: 0,
        deletions: 0,
    }
}

struct MockGitClass;

impl GitApi for MockGitClass {
    fn clone_from(&self, _url: &str, _to: &str, _options: &Kwargs) -> Result<Box<dyn GitRepo>, SemanticaError> {
        Ok(Box::new(MockRepoWithCommit))
    }
    fn open(&self, _path: &str) -> Result<Box<dyn GitRepo>, SemanticaError> {
        Ok(Box::new(MockRepoWithCommit))
    }
}

#[test]
fn test_repo_ingestor__test_ingest_repository() {
    let _dns = mock_dns();
    // Create a real temp dir and populate it
    let real_temp_dir = tempfile::tempdir().unwrap();
    std::fs::write(real_temp_dir.path().join("main.py"), "print('hello')").unwrap();
    std::fs::write(real_temp_dir.path().join("README.md"), "# Repo").unwrap();
    let dir = real_temp_dir.path().to_string_lossy().into_owned();
    let _git = repo_ingestor::GIT_REPO.set(std::sync::Arc::new(MockGitClass) as std::sync::Arc<dyn GitApi>);
    let made = dir.clone();
    let _mkdtemp = repo_ingestor::MKDTEMP.set(Box::new(move |_prefix: &str| made.clone()));
    let _rmtree = repo_ingestor::RMTREE.set(Box::new(|_path: &str| {}));
    let (tracker, display) = recording_tracker();
    let _tracker = repo_ingestor::GET_PROGRESS_TRACKER.set(Box::new(move || tracker.clone()));

    let mut ingestor = RepoIngestor::new(Kwargs::new());
    let result = ingestor
        .ingest_repository("https://github.com/user/repo.git", &Kwargs::new())
        .unwrap();
    // Note: RepoIngestor returns 'code_files' instead of 'files'
    assert!(result.contains_key("code_files"));
    assert!(result["code_files"].as_array().unwrap().len() >= 2);
    assert!(result.contains_key("commits"));
    assert_eq!(result["commits"].as_array().unwrap().len(), 1);
    // Check progress tracker calls
    let items = recorded(&display);
    assert!(items.iter().any(|i| i.message == "Repository: https://github.com/user/repo.git"));
    assert!(items.iter().any(|i| i.message == "Cloning repository..."));
}
