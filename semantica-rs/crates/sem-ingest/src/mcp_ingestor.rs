//! MCP server ingestion (`semantica/ingest/mcp_ingestor.py`): connects to Python / FastMCP
//! servers by URL and ingests resources and tool outputs.

use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Value};

use crate::config::{processing_error, validation_error};
use crate::mcp_client::{MCPClient, MCPResource, MCPTool, McpClientApi};
use crate::support::hooks::Hook;
use crate::support::kw::Kwargs;

type ClientFactory =
    dyn Fn(Option<&str>, Option<IndexMap<String, String>>, &Kwargs) -> Result<Box<dyn McpClientApi>, SemanticaError>;

/// Replacement for the `MCPClient` class the ingestor instantiates
/// (`patch("semantica.ingest.mcp_ingestor.MCPClient")`).
pub static MCP_CLIENT: Hook<Box<ClientFactory>> = Hook::new("semantica.ingest.mcp_ingestor.MCPClient");

/// Replacement for `get_progress_tracker` as seen by this module.
pub static GET_PROGRESS_TRACKER: Hook<Box<dyn Fn() -> Arc<ProgressTracker>>> =
    Hook::new("semantica.ingest.mcp_ingestor.get_progress_tracker");

fn progress_tracker() -> Arc<ProgressTracker> {
    match GET_PROGRESS_TRACKER.get() {
        Some(f) => f(),
        None => sem_utils::get_progress_tracker(),
    }
}

/// `MCPData`.
#[derive(Debug, Clone, PartialEq)]
pub struct MCPData {
    /// `source`.
    pub source: String,
    /// `server_name`.
    pub server_name: String,
    /// `data_type` (`"resource"` or `"tool_output"`).
    pub data_type: String,
    /// `content`.
    pub content: Value,
    /// `metadata`.
    pub metadata: Map<String, Value>,
    /// `ingested_at`.
    pub ingested_at: PyDateTime,
    /// `resource_uri`.
    pub resource_uri: Option<String>,
    /// `tool_name`.
    pub tool_name: Option<String>,
}

/// `MCPIngestor`.
pub struct MCPIngestor {
    /// Merged configuration.
    pub config: Kwargs,
    /// Progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    clients: IndexMap<String, Box<dyn McpClientApi>>,
    logger: Logger,
}

impl std::fmt::Debug for MCPIngestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MCPIngestor")
            .field("config", &self.config)
            .field("servers", &self.clients.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl MCPIngestor {
    /// `MCPIngestor(config=None, **kwargs)`.
    pub fn new(config: Kwargs) -> MCPIngestor {
        let ingestor = MCPIngestor {
            config,
            progress_tracker: progress_tracker(),
            clients: IndexMap::new(),
            logger: get_logger("mcp_ingestor"),
        };
        ingestor.logger.info("MCP ingestor initialized");
        ingestor
    }

    /// `connect(server_name, url=None, headers=None, **config)`.
    pub fn connect(
        &mut self,
        server_name: &str,
        url: Option<&str>,
        headers: Option<IndexMap<String, String>>,
        config: &Kwargs,
    ) -> Result<bool, SemanticaError> {
        let result = (|| -> Result<bool, SemanticaError> {
            if self.clients.contains_key(server_name) {
                self.logger.warning(format!(
                    "Server {server_name} already connected, reconnecting..."
                ));
                self.disconnect(Some(server_name));
            }
            let url = match url {
                Some(u) if !u.is_empty() => u,
                _ => {
                    return Err(validation_error(
                        "URL is required for MCP server connection. Provide MCP server URL: http://, https://, or mcp://",
                    ));
                }
            };
            let mut client: Box<dyn McpClientApi> = match MCP_CLIENT.get() {
                Some(factory) => factory(Some(url), headers.clone(), config)?,
                None => Box::new(MCPClient::new(Some(url), headers.clone(), None, config.clone())?),
            };
            if client.connect()? {
                self.clients.insert(server_name.to_string(), client);
                self.logger
                    .info(format!("Connected to MCP server: {server_name} at {url}"));
                Ok(true)
            } else {
                Err(processing_error(format!(
                    "Failed to connect to MCP server: {server_name}"
                )))
            }
        })();
        result.map_err(|e| {
            self.logger
                .error(format!("Failed to connect to MCP server {server_name}: {e}"));
            processing_error(format!("Failed to connect to MCP server {server_name}: {e}"))
        })
    }

    /// `disconnect(server_name=None)`.
    pub fn disconnect(&mut self, server_name: Option<&str>) {
        match server_name {
            Some(name) => {
                if let Some(mut client) = self.clients.shift_remove(name) {
                    client.disconnect();
                    self.logger.info(format!("Disconnected from MCP server: {name}"));
                } else {
                    self.logger.warning(format!("Server {name} not connected"));
                }
            }
            None => {
                for (_, mut client) in self.clients.drain(..) {
                    client.disconnect();
                }
                self.logger.info("Disconnected from all MCP servers");
            }
        }
    }

    fn get_client(&mut self, server_name: &str) -> Result<&mut Box<dyn McpClientApi>, SemanticaError> {
        self.clients.get_mut(server_name).ok_or_else(|| {
            validation_error(format!(
                "MCP server '{server_name}' not connected. Call connect() first."
            ))
        })
    }

    /// `list_available_resources(server_name)`.
    pub fn list_available_resources(&mut self, server_name: &str) -> Result<Vec<MCPResource>, SemanticaError> {
        self.get_client(server_name)?.list_resources()
    }

    /// `list_available_tools(server_name)`.
    pub fn list_available_tools(&mut self, server_name: &str) -> Result<Vec<MCPTool>, SemanticaError> {
        self.get_client(server_name)?.list_tools()
    }

    /// `read_resource(server_name, uri)`.
    pub fn read_resource(&mut self, server_name: &str, uri: &str) -> Result<Value, SemanticaError> {
        self.get_client(server_name)?.read_resource(uri)
    }

    /// `call_tool(server_name, tool_name, arguments=None)`.
    pub fn call_tool(
        &mut self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<&Map<String, Value>>,
    ) -> Result<Value, SemanticaError> {
        let args = arguments.cloned().unwrap_or_default();
        self.get_client(server_name)?.call_tool(tool_name, Some(&args))
    }

    /// `ingest_resources(server_name, resource_uris=None, filter_func=None, **options)`.
    pub fn ingest_resources(
        &mut self,
        server_name: &str,
        resource_uris: Option<&[String]>,
        filter_func: Option<&dyn Fn(&MCPResource) -> bool>,
    ) -> Result<Vec<MCPData>, SemanticaError> {
        self.get_client(server_name)?;
        let tracker = self.progress_tracker.clone();
        let result = (|| -> Result<Vec<MCPData>, SemanticaError> {
            let tracking_id = tracker.start_tracking(
                None,
                Some("ingest"),
                Some("MCPIngestor"),
                &format!("Ingesting resources from {server_name}"),
                None,
            );
            let client = self.get_client(server_name)?;
            let all = client.list_resources()?;
            let resources: Vec<MCPResource> = match (resource_uris, filter_func) {
                (Some(uris), _) if !uris.is_empty() => {
                    all.into_iter().filter(|r| uris.contains(&r.uri)).collect()
                }
                (_, Some(f)) => all.into_iter().filter(|r| f(r)).collect(),
                _ => all,
            };
            if resources.is_empty() {
                self.logger
                    .warning(format!("No resources found for server {server_name}"));
                tracker.update_tracking(&tracking_id, Some("completed"), "No resources found");
                return Ok(Vec::new());
            }
            let total = resources.len();
            let mut ingested = Vec::new();
            for (idx, resource) in resources.iter().enumerate() {
                tracker.update_tracking(
                    &tracking_id,
                    Some("running"),
                    &format!("Reading resource: {} ({}/{total})", resource.uri, idx + 1),
                );
                let client = self.get_client(server_name)?;
                match client.read_resource(&resource.uri) {
                    Ok(content) => {
                        let mut metadata = Map::new();
                        metadata.insert("resource_name".to_string(), Value::String(resource.name.clone()));
                        metadata.insert(
                            "resource_description".to_string(),
                            crate::support::kw::opt_str(resource.description.as_deref()),
                        );
                        metadata.insert(
                            "mime_type".to_string(),
                            crate::support::kw::opt_str(resource.mime_type.as_deref()),
                        );
                        for (k, v) in &resource.metadata {
                            metadata.insert(k.clone(), v.clone());
                        }
                        ingested.push(MCPData {
                            source: resource.uri.clone(),
                            server_name: server_name.to_string(),
                            data_type: "resource".to_string(),
                            content,
                            metadata,
                            ingested_at: PyDateTime::now(false),
                            resource_uri: Some(resource.uri.clone()),
                            tool_name: None,
                        });
                    }
                    Err(e) => {
                        self.logger
                            .error(format!("Failed to ingest resource {}: {e}", resource.uri));
                        tracker.update_tracking(
                            &tracking_id,
                            Some("running"),
                            &format!("Failed to ingest resource {}: {e}", resource.uri),
                        );
                    }
                }
            }
            tracker.update_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Successfully ingested {} resources", ingested.len()),
            );
            self.logger.info(format!(
                "Ingested {} resources from {server_name}",
                ingested.len()
            ));
            Ok(ingested)
        })();
        result.map_err(|e| {
            self.logger
                .error(format!("Failed to ingest resources from {server_name}: {e}"));
            processing_error(format!("Failed to ingest resources: {e}"))
        })
    }

    /// `ingest_tool_output(server_name, tool_name, arguments=None, **options)`.
    pub fn ingest_tool_output(
        &mut self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<&Map<String, Value>>,
    ) -> Result<MCPData, SemanticaError> {
        self.get_client(server_name)?;
        let tracker = self.progress_tracker.clone();
        let args = arguments.cloned().unwrap_or_default();
        let result = (|| -> Result<MCPData, SemanticaError> {
            let tracking_id = tracker.start_tracking(
                None,
                Some("ingest"),
                Some("MCPIngestor"),
                &format!("Calling tool {tool_name} on {server_name}"),
                None,
            );
            tracker.update_tracking(&tracking_id, Some("running"), &format!("Calling tool: {tool_name}"));
            let content = self.get_client(server_name)?.call_tool(tool_name, Some(&args))?;
            let mut metadata = Map::new();
            metadata.insert("tool_name".to_string(), Value::String(tool_name.to_string()));
            metadata.insert("arguments".to_string(), Value::Object(args.clone()));
            let data = MCPData {
                source: tool_name.to_string(),
                server_name: server_name.to_string(),
                data_type: "tool_output".to_string(),
                content,
                metadata,
                ingested_at: PyDateTime::now(false),
                resource_uri: None,
                tool_name: Some(tool_name.to_string()),
            };
            tracker.update_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Successfully called tool {tool_name}"),
            );
            self.logger
                .info(format!("Ingested data from tool {tool_name} on {server_name}"));
            Ok(data)
        })();
        result.map_err(|e| {
            self.logger
                .error(format!("Failed to ingest tool output from {server_name}: {e}"));
            processing_error(format!("Failed to ingest tool output: {e}"))
        })
    }

    /// `ingest_all_resources(server_name, **options)`.
    pub fn ingest_all_resources(&mut self, server_name: &str) -> Result<Vec<MCPData>, SemanticaError> {
        self.ingest_resources(server_name, None, None)
    }

    /// `get_connected_servers()`.
    pub fn get_connected_servers(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    /// `is_connected(server_name)`.
    pub fn is_connected(&self, server_name: &str) -> bool {
        self.clients
            .get(server_name)
            .is_some_and(|c| c.is_connected())
    }
}
