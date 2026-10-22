//! MCP (Model Context Protocol) client (`semantica/ingest/mcp_client.py`): JSON-RPC over
//! HTTP(S) through the SSRF guard (operator-configured hosts may be private; redirects to other
//! hosts may not), plus the internal stdio transport.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Value, json};

use crate::config::{processing_error, validation_error};
use crate::ssrf::{GuardOptions, request_with_ssrf_guard};
use crate::support::http::RequestOptions;
use crate::support::kw::{self, Kwargs};

/// `MCPResource`.
#[derive(Debug, Clone, PartialEq)]
pub struct MCPResource {
    /// `uri`.
    pub uri: String,
    /// `name`.
    pub name: String,
    /// `description`.
    pub description: Option<String>,
    /// `mime_type`.
    pub mime_type: Option<String>,
    /// `metadata`.
    pub metadata: Map<String, Value>,
}

impl MCPResource {
    /// `MCPResource(uri, name)`.
    pub fn new(uri: &str, name: &str) -> MCPResource {
        MCPResource {
            uri: uri.to_string(),
            name: name.to_string(),
            description: None,
            mime_type: None,
            metadata: Map::new(),
        }
    }
}

/// `MCPTool`.
#[derive(Debug, Clone, PartialEq)]
pub struct MCPTool {
    /// `name`.
    pub name: String,
    /// `description`.
    pub description: Option<String>,
    /// `input_schema`.
    pub input_schema: Map<String, Value>,
    /// `metadata`.
    pub metadata: Map<String, Value>,
}

impl MCPTool {
    /// `MCPTool(name)`.
    pub fn new(name: &str) -> MCPTool {
        MCPTool {
            name: name.to_string(),
            description: None,
            input_schema: Map::new(),
            metadata: Map::new(),
        }
    }
}

/// What the MCP ingestor needs from a client (implemented by [`MCPClient`]; fakes in tests).
pub trait McpClientApi: Send {
    /// `connect()`.
    fn connect(&mut self) -> Result<bool, SemanticaError>;
    /// `disconnect()`.
    fn disconnect(&mut self);
    /// `list_resources()`.
    fn list_resources(&mut self) -> Result<Vec<MCPResource>, SemanticaError>;
    /// `list_tools()`.
    fn list_tools(&mut self) -> Result<Vec<MCPTool>, SemanticaError>;
    /// `read_resource(uri)`.
    fn read_resource(&mut self, uri: &str) -> Result<Value, SemanticaError>;
    /// `call_tool(name, arguments)`.
    fn call_tool(&mut self, name: &str, arguments: Option<&Map<String, Value>>) -> Result<Value, SemanticaError>;
    /// `is_connected()`.
    fn is_connected(&self) -> bool;
}

struct StdioProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// `MCPClient`: generic MCP client for Python / FastMCP servers.
pub struct MCPClient {
    /// `url`.
    pub url: Option<String>,
    /// `headers` (authentication etc.).
    pub headers: IndexMap<String, String>,
    /// `config` (`timeout`, ...).
    pub config: Kwargs,
    /// `transport` (`"http"` or `"sse"`; `"stdio"` internally).
    pub transport: String,
    command: Option<String>,
    args: Vec<String>,
    process: Option<StdioProcess>,
    connected: bool,
    initialized: bool,
    server_info: Option<Value>,
    request_id: i64,
    logger: Logger,
}

impl std::fmt::Debug for MCPClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MCPClient")
            .field("url", &self.url)
            .field("transport", &self.transport)
            .field("connected", &self.connected)
            .field("initialized", &self.initialized)
            .finish()
    }
}

impl MCPClient {
    /// `MCPClient(url=None, headers=None, transport=None, **config)`.
    pub fn new(
        url: Option<&str>,
        headers: Option<IndexMap<String, String>>,
        transport: Option<&str>,
        config: Kwargs,
    ) -> Result<MCPClient, SemanticaError> {
        let logger = get_logger("mcp_client");
        let transport = match url {
            Some(u) if !u.is_empty() => {
                let lower = u.to_lowercase();
                if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mcp://") {
                    "http".to_string()
                } else if lower.starts_with("sse://") {
                    "sse".to_string()
                } else {
                    logger.warning(format!("Unknown URL scheme, defaulting to HTTP: {u}"));
                    "http".to_string()
                }
            }
            _ => match transport {
                Some(t) if !t.is_empty() => t.to_lowercase(),
                _ => {
                    return Err(validation_error(
                        "Either 'url' or 'transport' parameter is required. URL-based connection is the primary method.",
                    ));
                }
            },
        };
        if transport != "http" && transport != "sse" {
            return Err(validation_error(format!(
                "Unsupported transport: {transport}. Supported: http, sse. Note: stdio transport is not supported in public API."
            )));
        }
        if url.is_none_or(str::is_empty) {
            return Err(validation_error(format!(
                "{transport} transport requires 'url' parameter"
            )));
        }
        let client = MCPClient {
            url: url.map(str::to_string),
            headers: headers.unwrap_or_default(),
            config,
            transport,
            command: None,
            args: Vec::new(),
            process: None,
            connected: false,
            initialized: false,
            server_info: None,
            request_id: 0,
            logger,
        };
        client.logger.debug(format!(
            "MCP client initialized: url={}, transport={}",
            client.url.as_deref().unwrap_or("None"),
            client.transport
        ));
        Ok(client)
    }

    /// The internal stdio transport (`_command` / `_args`; not reachable through the public
    /// constructor, as in the original).
    pub fn _with_stdio_command(command: &str, args: &[&str], config: Kwargs) -> MCPClient {
        MCPClient {
            url: None,
            headers: IndexMap::new(),
            config,
            transport: "stdio".to_string(),
            command: Some(command.to_string()),
            args: args.iter().map(|s| s.to_string()).collect(),
            process: None,
            connected: false,
            initialized: false,
            server_info: None,
            request_id: 0,
            logger: get_logger("mcp_client"),
        }
    }

    /// `connect()`.
    pub fn connect(&mut self) -> Result<bool, SemanticaError> {
        let result = match self.transport.as_str() {
            "http" => self._connect_http(),
            "sse" => self._connect_sse(),
            "stdio" => self._connect_stdio(),
            other => Err(validation_error(format!("Unsupported transport: {other}"))),
        };
        result.map_err(|e| {
            self.logger.error(format!("Failed to connect to MCP server: {e}"));
            processing_error(format!("Failed to connect to MCP server: {e}"))
        })
    }

    /// `_connect_stdio()`.
    pub fn _connect_stdio(&mut self) -> Result<bool, SemanticaError> {
        let Some(command) = self.command.clone() else {
            return Err(SemanticaError::Attribute(
                "'MCPClient' object has no attribute 'command'".to_string(),
            ));
        };
        let mut child = Command::new(&command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SemanticaError::from_io(&e, None))?;
        let stdin = child.stdin.take().ok_or_else(|| processing_error("Process not running"))?;
        let stdout = child.stdout.take().ok_or_else(|| processing_error("Process not running"))?;
        self.process = Some(StdioProcess {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        });
        self.connected = true;
        self.logger.info("Connected to MCP server via stdio");
        self._initialize()
    }

    /// `_connect_http()`.
    pub fn _connect_http(&mut self) -> Result<bool, SemanticaError> {
        self.connected = true;
        self.logger.info(format!(
            "Connected to MCP server via HTTP: {}",
            self.url.as_deref().unwrap_or("")
        ));
        self._initialize()
    }

    /// `_connect_sse()`.
    pub fn _connect_sse(&mut self) -> Result<bool, SemanticaError> {
        self.connected = true;
        self.logger.info(format!(
            "Connected to MCP server via SSE: {}",
            self.url.as_deref().unwrap_or("")
        ));
        self._initialize()
    }

    fn error_message(response: &Option<Value>) -> String {
        response
            .as_ref()
            .and_then(|r| r.get("error"))
            .and_then(|e| e.get("message"))
            .map(sem_utils::pycompat::fmt::py_str)
            .unwrap_or_else(|| "Unknown error".to_string())
    }

    fn result_of(response: &Option<Value>) -> Option<Value> {
        response
            .as_ref()
            .and_then(Value::as_object)
            .and_then(|m| m.get("result"))
            .cloned()
    }

    /// `_initialize()`: the `initialize` handshake.
    pub fn _initialize(&mut self) -> Result<bool, SemanticaError> {
        let id = self._get_request_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"roots": {"listChanged": true}},
                "clientInfo": {"name": "semantica", "version": "1.0.0"},
            },
        });
        let response = self._send_request(&request).inspect_err(|e| {
            self.logger.error(format!("Failed to initialize MCP protocol: {e}"));
        })?;
        match Self::result_of(&response) {
            Some(result) => {
                self.server_info = Some(result);
                self.initialized = true;
                self.logger.info("MCP protocol initialized");
                Ok(true)
            }
            None => {
                let e = processing_error(format!(
                    "Failed to initialize MCP: {}",
                    Self::error_message(&response)
                ));
                self.logger.error(format!("Failed to initialize MCP protocol: {e}"));
                Err(e)
            }
        }
    }

    /// `disconnect()`.
    pub fn disconnect(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
        self.connected = false;
        self.initialized = false;
        self.logger.info("Disconnected from MCP server");
    }

    /// `_get_request_id()`.
    pub fn _get_request_id(&mut self) -> i64 {
        self.request_id += 1;
        self.request_id
    }

    /// `_send_request(request)`.
    pub fn _send_request(&mut self, request: &Value) -> Result<Option<Value>, SemanticaError> {
        if !self.connected {
            return Err(processing_error("Not connected to MCP server"));
        }
        let result = match self.transport.as_str() {
            "stdio" => self._send_request_stdio(request),
            "http" => self._send_request_http(request).map(Some),
            "sse" => self._send_request_sse(request),
            other => Err(validation_error(format!("Unsupported transport: {other}"))),
        };
        result.map_err(|e| {
            self.logger.error(format!("Failed to send request: {e}"));
            processing_error(format!("Failed to send request: {e}"))
        })
    }

    /// `_send_request_stdio(request)`: one JSON line out, one JSON line back.
    pub fn _send_request_stdio(&mut self, request: &Value) -> Result<Option<Value>, SemanticaError> {
        let Some(process) = self.process.as_mut() else {
            return Err(processing_error("Process not running"));
        };
        let line = format!(
            "{}\n",
            sem_utils::pycompat::json::dumps_default(request)
        );
        process
            .stdin
            .write_all(line.as_bytes())
            .and_then(|_| process.stdin.flush())
            .map_err(SemanticaError::from)?;
        let mut response = String::new();
        process
            .stdout
            .read_line(&mut response)
            .map_err(SemanticaError::from)?;
        if response.is_empty() {
            return Ok(None);
        }
        crate::support::pyjson::loads(response.trim())
            .map(Some)
            .map_err(SemanticaError::from)
    }

    /// `_send_request_http(request)`: POST through the SSRF guard (`allow_private_ips=True`
    /// for the configured URL, `allow_private_ips_on_redirect=False`).
    pub fn _send_request_http(&mut self, request: &Value) -> Result<Value, SemanticaError> {
        let url = self.url.clone().unwrap_or_default();
        let mut options = RequestOptions {
            headers: Some(self.headers.clone()),
            json: Some(request.clone()),
            ..RequestOptions::default()
        };
        options.timeout = Some(kw::get_f64(&self.config, "timeout", 30.0));
        let guard = GuardOptions {
            allow_private_ips: true,
            allow_private_ips_on_redirect: Some(false),
            ..GuardOptions::default()
        };
        let result = request_with_ssrf_guard("POST", &url, None, &guard, options).and_then(|response| {
            response.raise_for_status()?;
            response.json()
        });
        if let Err(e) = &result {
            self.logger.error(format!("Failed to send HTTP request: {e}"));
        }
        result
    }

    /// `_send_request_sse(request)`: not implemented in the original.
    pub fn _send_request_sse(&mut self, _request: &Value) -> Result<Option<Value>, SemanticaError> {
        Err(processing_error("SSE transport not yet implemented"))
    }

    fn rpc(&mut self, method: &str, params: Option<Value>, what: &str) -> Result<Value, SemanticaError> {
        if !self.initialized {
            return Err(processing_error("MCP protocol not initialized"));
        }
        let id = self._get_request_id();
        let mut request = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if let Some(p) = params {
            request["params"] = p;
        }
        let response = self._send_request(&request)?;
        match Self::result_of(&response) {
            Some(result) => Ok(result),
            None => Err(processing_error(format!(
                "Failed to {what}: {}",
                Self::error_message(&response)
            ))),
        }
    }

    /// `list_resources()`.
    pub fn list_resources(&mut self) -> Result<Vec<MCPResource>, SemanticaError> {
        let result = self.rpc("resources/list", None, "list resources").inspect_err(|e| {
            if self.initialized {
                self.logger.error(format!("Failed to list resources: {e}"));
            }
        })?;
        let items = result
            .get("resources")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .map(|r| MCPResource {
                uri: r.get("uri").map(sem_utils::pycompat::fmt::py_str).unwrap_or_default(),
                name: r.get("name").map(sem_utils::pycompat::fmt::py_str).unwrap_or_default(),
                description: r.get("description").and_then(Value::as_str).map(str::to_string),
                mime_type: r.get("mimeType").and_then(Value::as_str).map(str::to_string),
                metadata: r
                    .get("metadata")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect())
    }

    /// `list_tools()`.
    pub fn list_tools(&mut self) -> Result<Vec<MCPTool>, SemanticaError> {
        let result = self.rpc("tools/list", None, "list tools").inspect_err(|e| {
            if self.initialized {
                self.logger.error(format!("Failed to list tools: {e}"));
            }
        })?;
        let items = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .map(|t| MCPTool {
                name: t.get("name").map(sem_utils::pycompat::fmt::py_str).unwrap_or_default(),
                description: t.get("description").and_then(Value::as_str).map(str::to_string),
                input_schema: t
                    .get("inputSchema")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
                metadata: t
                    .get("metadata")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect())
    }

    /// `read_resource(uri)`.
    pub fn read_resource(&mut self, uri: &str) -> Result<Value, SemanticaError> {
        self.rpc("resources/read", Some(json!({"uri": uri})), "read resource")
            .inspect_err(|e| {
                if self.initialized {
                    self.logger.error(format!("Failed to read resource {uri}: {e}"));
                }
            })
    }

    /// `call_tool(name, arguments=None)`.
    pub fn call_tool(&mut self, name: &str, arguments: Option<&Map<String, Value>>) -> Result<Value, SemanticaError> {
        let args = Value::Object(arguments.cloned().unwrap_or_default());
        self.rpc(
            "tools/call",
            Some(json!({"name": name, "arguments": args})),
            &format!("call tool {name}"),
        )
        .inspect_err(|e| {
            if self.initialized {
                self.logger.error(format!("Failed to call tool {name}: {e}"));
            }
        })
    }

    /// `is_connected()`.
    pub fn is_connected(&self) -> bool {
        self.connected && self.initialized
    }

    /// `get_server_info()`.
    pub fn get_server_info(&self) -> Option<Value> {
        self.server_info.clone()
    }
}

impl McpClientApi for MCPClient {
    fn connect(&mut self) -> Result<bool, SemanticaError> {
        MCPClient::connect(self)
    }

    fn disconnect(&mut self) {
        MCPClient::disconnect(self)
    }

    fn list_resources(&mut self) -> Result<Vec<MCPResource>, SemanticaError> {
        MCPClient::list_resources(self)
    }

    fn list_tools(&mut self) -> Result<Vec<MCPTool>, SemanticaError> {
        MCPClient::list_tools(self)
    }

    fn read_resource(&mut self, uri: &str) -> Result<Value, SemanticaError> {
        MCPClient::read_resource(self, uri)
    }

    fn call_tool(&mut self, name: &str, arguments: Option<&Map<String, Value>>) -> Result<Value, SemanticaError> {
        MCPClient::call_tool(self, name, arguments)
    }

    fn is_connected(&self) -> bool {
        MCPClient::is_connected(self)
    }
}

impl Drop for MCPClient {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }
}
