//! Characterisation tests for `semantica/ingest/mcp_client.py` and
//! `semantica/ingest/mcp_ingestor.py` (expected values from the original run in the oracle
//! environment with `requests.Session.request` / the `MCPClient` class patched).

mod common;

use common::*;
use indexmap::IndexMap;
use sem_ingest::mcp_client::{MCPClient, MCPResource, MCPTool, McpClientApi};
use sem_ingest::mcp_ingestor::{MCP_CLIENT, MCPData, MCPIngestor};
use sem_ingest::support::http::Response;
use sem_ingest::support::kw::Kwargs;
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

fn kwargs(v: Value) -> Kwargs {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn mcp_client_round_trip() {
    let _dns = mock_dns();
    let init = json!({"jsonrpc": "2.0", "id": 1, "result": {"serverInfo": {"name": "srv", "version": "2"}, "capabilities": {}}});
    let res_list = json!({"jsonrpc": "2.0", "id": 2, "result": {"resources": [
        {"uri": "file://a", "name": "A", "description": "first", "mimeType": "text/plain", "extra": 1},
        {"uri": "file://b", "name": "B"}]}});
    let tool_list = json!({"jsonrpc": "2.0", "id": 3, "result": {"tools": [
        {"name": "t1", "description": "d", "inputSchema": {"type": "object"}, "x": 2}, {"name": "t2"}]}});
    let read = json!({"jsonrpc": "2.0", "id": 4, "result": {"contents": [{"uri": "file://a", "text": "hello"}]}});
    let call = json!({"jsonrpc": "2.0", "id": 5, "result": {"content": [{"type": "text", "text": "ok"}]}});
    let err = json!({"jsonrpc": "2.0", "id": 6, "error": {"code": -1, "message": "boom"}});
    let fake = FakeTransport::sequence(
        [init, res_list, tool_list, read, call, err]
            .into_iter()
            .map(|p| Response::new(200).with_json(p))
            .collect(),
    );
    let _session = patch_session_request(&fake);
    let mut headers = IndexMap::new();
    headers.insert("Authorization".to_string(), "Bearer t".to_string());
    let mut c = MCPClient::new(
        Some("http://mcp.example.com/mcp"),
        Some(headers.clone()),
        None,
        kwargs(json!({"timeout": 5})),
    )
    .unwrap();
    assert_eq!(c.url.as_deref(), Some("http://mcp.example.com/mcp"));
    assert_eq!(c.transport, "http");
    assert_eq!(c.headers, headers);
    assert_eq!(Value::Object(c.config.clone()), json!({"timeout": 5}));
    assert!(!c.is_connected());
    assert_eq!(c.get_server_info(), None);
    assert!(c.connect().unwrap());
    assert!(c.is_connected());
    let info = json!({"serverInfo": {"name": "srv", "version": "2"}, "capabilities": {}});
    assert_eq!(c.get_server_info(), Some(info.clone()));

    let rs = c.list_resources().unwrap();
    let summary: Vec<Value> = rs
        .iter()
        .map(|r| json!([r.uri, r.name, r.description, r.mime_type, r.metadata]))
        .collect();
    assert_eq!(
        summary,
        vec![json!(["file://a", "A", "first", "text/plain", {}]), json!(["file://b", "B", null, null, {}])]
    );
    let ts = c.list_tools().unwrap();
    let summary: Vec<Value> = ts
        .iter()
        .map(|t| json!([t.name, t.description, t.input_schema, t.metadata]))
        .collect();
    assert_eq!(summary, vec![json!(["t1", "d", {"type": "object"}, {}]), json!(["t2", null, {}, {}])]);
    assert_eq!(
        c.read_resource("file://a").unwrap(),
        json!({"contents": [{"uri": "file://a", "text": "hello"}]})
    );
    let args = kwargs(json!({"a": 1}));
    assert_eq!(
        c.call_tool("t1", Some(&args)).unwrap(),
        json!({"content": [{"type": "text", "text": "ok"}]})
    );
    let e = c.call_tool("t2", None).unwrap_err();
    assert_eq!(e.type_name(), "ProcessingError");
    assert_eq!(e.to_string(), "Failed to call tool t2: boom");
    let bodies: Vec<Value> = fake.calls().iter().map(|x| x.options.json.clone().unwrap()).collect();
    assert_eq!(
        bodies,
        vec![
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {"roots": {"listChanged": true}}, "clientInfo": {"name": "semantica", "version": "1.0.0"}}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "resources/list"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
            json!({"jsonrpc": "2.0", "id": 4, "method": "resources/read", "params": {"uri": "file://a"}}),
            json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "t1", "arguments": {"a": 1}}}),
            json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "t2", "arguments": {}}}),
        ]
    );
    assert!(fake.calls().iter().all(|x| x.method == "POST" && x.url == "http://mcp.example.com/mcp"));
    c.disconnect();
    assert!(!c.is_connected());
    assert_eq!(c.get_server_info(), Some(info));
    let e = c.list_resources().unwrap_err();
    assert_eq!(e.to_string(), "MCP protocol not initialized");
}

#[test]
fn mcp_client_constructor_validation() {
    let e = MCPClient::new(None, None, None, Kwargs::new()).unwrap_err();
    assert!(e.is_validation_error());
    assert_eq!(
        e.to_string(),
        "Either 'url' or 'transport' parameter is required. URL-based connection is the primary method."
    );
    let e = MCPClient::new(None, None, Some("stdio"), Kwargs::new()).unwrap_err();
    assert_eq!(
        e.to_string(),
        "Unsupported transport: stdio. Supported: http, sse. Note: stdio transport is not supported in public API."
    );
    let e = MCPClient::new(None, None, Some("sse"), Kwargs::new()).unwrap_err();
    assert_eq!(e.to_string(), "sse transport requires 'url' parameter");
    assert_eq!(MCPClient::new(Some("ftp://x"), None, None, Kwargs::new()).unwrap().transport, "http");
    let r = MCPResource::new("u", "n");
    assert_eq!((r.description, r.mime_type, r.metadata.is_empty()), (None, None, true));
    let t = MCPTool::new("t");
    assert_eq!((t.description, t.input_schema.is_empty(), t.metadata.is_empty()), (None, true, true));
}

/// The `FakeClient` class the oracle run patched in.
struct FakeClient {
    connected: bool,
}

impl McpClientApi for FakeClient {
    fn connect(&mut self) -> Result<bool, SemanticaError> {
        self.connected = true;
        Ok(true)
    }

    fn disconnect(&mut self) {
        self.connected = false;
    }

    fn list_resources(&mut self) -> Result<Vec<MCPResource>, SemanticaError> {
        let mut r1 = MCPResource::new("r1", "R1");
        r1.description = Some("d".to_string());
        r1.mime_type = Some("text/plain".to_string());
        r1.metadata = kwargs(json!({"k": 1}));
        Ok(vec![r1, MCPResource::new("r2", "R2")])
    }

    fn list_tools(&mut self) -> Result<Vec<MCPTool>, SemanticaError> {
        Ok(vec![MCPTool::new("tool")])
    }

    fn read_resource(&mut self, uri: &str) -> Result<Value, SemanticaError> {
        if uri == "r2" {
            return Err(SemanticaError::Other {
                module: "builtins".to_string(),
                type_name: "Exception".to_string(),
                message: "unreadable".to_string(),
            });
        }
        Ok(json!({"uri": uri}))
    }

    fn call_tool(&mut self, name: &str, arguments: Option<&Map<String, Value>>) -> Result<Value, SemanticaError> {
        Ok(json!({"called": name, "args": arguments}))
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[test]
fn mcp_ingestor_lifecycle() {
    let _client = MCP_CLIENT.set(Box::new(
        |_url: Option<&str>, _headers: Option<IndexMap<String, String>>, _config: &Kwargs| {
            Ok(Box::new(FakeClient { connected: false }) as Box<dyn McpClientApi>)
        },
    ));
    let mut ing = MCPIngestor::new(Kwargs::new());
    assert!(ing.connect("s1", Some("http://x"), None, &Kwargs::new()).unwrap());
    assert_eq!(ing.get_connected_servers(), vec!["s1"]);
    assert!(ing.is_connected("s1"));
    assert!(!ing.is_connected("zz"));
    let uris: Vec<String> = ing.list_available_resources("s1").unwrap().into_iter().map(|r| r.uri).collect();
    assert_eq!(uris, vec!["r1", "r2"]);
    let tools: Vec<String> = ing.list_available_tools("s1").unwrap().into_iter().map(|t| t.name).collect();
    assert_eq!(tools, vec!["tool"]);
    assert_eq!(ing.read_resource("s1", "r1").unwrap(), json!({"uri": "r1"}));
    let q1 = kwargs(json!({"q": 1}));
    assert_eq!(ing.call_tool("s1", "tool", Some(&q1)).unwrap(), json!({"called": "tool", "args": {"q": 1}}));

    let data = ing.ingest_resources("s1", None, None).unwrap();
    assert_eq!(data.len(), 1);
    let d = &data[0];
    assert_eq!(
        (d.source.as_str(), d.server_name.as_str(), d.data_type.as_str()),
        ("r1", "s1", "resource")
    );
    assert_eq!(d.content, json!({"uri": "r1"}));
    assert_eq!(
        Value::Object(d.metadata.clone()),
        json!({"resource_name": "R1", "resource_description": "d", "mime_type": "text/plain", "k": 1})
    );
    assert_eq!((d.resource_uri.as_deref(), d.tool_name.as_deref()), (Some("r1"), None));
    let only_r1 = |r: &MCPResource| r.uri == "r1";
    let filtered = ing.ingest_resources("s1", None, Some(&only_r1)).unwrap();
    assert_eq!(filtered.iter().map(|d| d.source.as_str()).collect::<Vec<_>>(), vec!["r1"]);
    let all = ing.ingest_all_resources("s1").unwrap();
    assert_eq!(all.iter().map(|d| d.source.as_str()).collect::<Vec<_>>(), vec!["r1"]);
    let q2 = kwargs(json!({"q": 2}));
    let out = ing.ingest_tool_output("s1", "tool", Some(&q2)).unwrap();
    assert_eq!((out.source.as_str(), out.data_type.as_str()), ("tool", "tool_output"));
    assert_eq!(out.content, json!({"called": "tool", "args": {"q": 2}}));
    assert_eq!(Value::Object(out.metadata.clone()), json!({"tool_name": "tool", "arguments": {"q": 2}}));
    assert_eq!((out.tool_name.as_deref(), out.resource_uri.as_deref()), (Some("tool"), None));

    let e = ing.list_available_resources("nope").unwrap_err();
    assert!(e.is_validation_error());
    assert_eq!(e.to_string(), "MCP server 'nope' not connected. Call connect() first.");
    let e = ing.ingest_resources("nope", None, None).unwrap_err();
    assert_eq!(e.to_string(), "MCP server 'nope' not connected. Call connect() first.");
    let e = ing.connect("s2", None, None, &Kwargs::new()).unwrap_err();
    assert_eq!(e.type_name(), "ProcessingError");
    assert_eq!(
        e.to_string(),
        "Failed to connect to MCP server s2: URL is required for MCP server connection. Provide MCP server URL: http://, https://, or mcp://"
    );
    ing.connect("s2", Some("http://y"), None, &Kwargs::new()).unwrap();
    ing.disconnect(Some("s1"));
    assert_eq!(ing.get_connected_servers(), vec!["s2"]);
    ing.disconnect(None);
    assert!(ing.get_connected_servers().is_empty());

    let d = MCPData {
        source: "s".to_string(),
        server_name: "n".to_string(),
        data_type: "resource".to_string(),
        content: json!(1),
        metadata: Map::new(),
        ingested_at: sem_utils::pycompat::time::PyDateTime::now(false),
        resource_uri: None,
        tool_name: None,
    };
    assert!(d.metadata.is_empty() && d.resource_uri.is_none() && d.tool_name.is_none());
}
