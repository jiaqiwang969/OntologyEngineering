//! Native neutral service capability, with one borrowed DatasetSession and host budget.
//! No Project, credential pool, adoption or version registry is constructed here.
pub mod books;
use anyhow::{Result, bail, ensure};
use oxigraph::io::RdfFormat;
use semantica_contracts::{
    SemanticBookCatalogPort, SemanticBookCatalogRequest, SemanticExtractionPort,
    SemanticExtractionRequest, SemanticMcpPort, SemanticMcpRequest, SemanticMcpResult,
    SemanticMcpTool, SemanticRuntimeFailure, SemanticServiceBudgetCharge,
    SemanticServiceBudgetPort, SemanticServiceBudgetReceipt, SemanticServiceQueryPort,
    SemanticServiceQueryRequest, SemanticServiceQueryResult,
};
use semantica_runtime::dataset::DatasetSession;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    sync::Mutex,
};

pub const PROFILE: &str = "semantica.native-read-service/1";
pub const MAX_FRAME: usize = 256 * 1024;
const CATALOG: &str = include_str!("../tests/fixtures/original-mcp-catalog.json");

fn failure(code: &str, phase: &str) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: code.into(),
        phase: phase.into(),
        message: format!("{code}: the declared service operation did not complete"),
        retryable: false,
    }
}
fn require_contract<T>(
    result: std::result::Result<T, Vec<String>>,
) -> std::result::Result<T, SemanticRuntimeFailure> {
    result.map_err(|_| failure("service_contract_invalid", "contract"))
}

/// Ephemeral native byte/call accounting, supplied by the application host.
/// A future model adapter must inject existing upper model-budget/credential owners instead.
pub struct NativeServiceBudget {
    max_calls: u64,
    max_input: u64,
    max_output: u64,
    counts: Mutex<SemanticServiceBudgetReceipt>,
}
impl NativeServiceBudget {
    pub fn new(max_calls: u64, max_input: u64, max_output: u64) -> Self {
        Self {
            max_calls,
            max_input,
            max_output,
            counts: Mutex::new(SemanticServiceBudgetReceipt {
                calls: 0,
                input_bytes: 0,
                output_bytes: 0,
            }),
        }
    }
    pub fn bounded() -> Self {
        Self::new(128, 8 * 1024 * 1024, 16 * 1024 * 1024)
    }
    pub fn charge_claim(
        &self,
        input: &SemanticServiceBudgetCharge,
    ) -> std::result::Result<SemanticServiceBudgetReceipt, SemanticRuntimeFailure> {
        require_contract(input.validate())?;
        let mut old = self
            .counts
            .lock()
            .map_err(|_| failure("service_budget_lock_failed", "budget"))?;
        let mut next = old.clone();
        match input.kind.as_str() {
            "request" | "query" => {
                if input.bytes > MAX_FRAME as u64 {
                    return Err(failure("service_frame_budget_exceeded", "budget"));
                }
                next.calls = next
                    .calls
                    .checked_add(1)
                    .ok_or_else(|| failure("service_budget_overflow", "budget"))?;
                next.input_bytes = next
                    .input_bytes
                    .checked_add(input.bytes)
                    .ok_or_else(|| failure("service_budget_overflow", "budget"))?;
            }
            "framing" => {
                if input.bytes > MAX_FRAME as u64 {
                    return Err(failure("service_frame_budget_exceeded", "budget"));
                }
                next.input_bytes = next
                    .input_bytes
                    .checked_add(input.bytes)
                    .ok_or_else(|| failure("service_budget_overflow", "budget"))?;
            }
            "response" => {
                next.output_bytes = next
                    .output_bytes
                    .checked_add(input.bytes)
                    .ok_or_else(|| failure("service_budget_overflow", "budget"))?
            }
            _ => return Err(failure("service_budget_kind_unsupported", "budget")),
        }
        if next.calls > self.max_calls
            || next.input_bytes > self.max_input
            || next.output_bytes > self.max_output
        {
            return Err(failure("service_budget_exceeded", "budget"));
        }
        *old = next.clone();
        Ok(next)
    }
}
impl SemanticServiceBudgetPort for NativeServiceBudget {
    fn charge(
        &self,
        input: &SemanticServiceBudgetCharge,
    ) -> std::result::Result<SemanticServiceBudgetReceipt, SemanticRuntimeFailure> {
        self.charge_claim(input)
    }
}

pub fn rdf_format(name: &str) -> Result<RdfFormat> {
    Ok(match name {
        "turtle" => RdfFormat::Turtle,
        "ntriples" => RdfFormat::NTriples,
        "rdfxml" => RdfFormat::RdfXml,
        "trig" => RdfFormat::TriG,
        "nquads" => RdfFormat::NQuads,
        _ => bail!("service_rdf_format_unsupported"),
    })
}

use semantica_runtime::context::context_summary;
pub use semantica_runtime::context::load_context_graph;

/// Borrows the application's single authoritative dataset; no copy/alternate engine.
pub struct McpService<'a> {
    session: Mutex<&'a mut DatasetSession>,
    budget: &'a dyn SemanticServiceBudgetPort,
    books: Option<&'a dyn SemanticBookCatalogPort>,
    native_tools: bool,
    original_tool_behavior: bool,
    extraction: Option<&'a dyn SemanticExtractionPort>,
}
impl<'a> McpService<'a> {
    pub fn new(session: &'a mut DatasetSession, budget: &'a dyn SemanticServiceBudgetPort) -> Self {
        Self {
            session: Mutex::new(session),
            budget,
            books: None,
            native_tools: false,
            original_tool_behavior: false,
            extraction: None,
        }
    }
    pub fn with_book_catalog(mut self, books: &'a dyn SemanticBookCatalogPort) -> Self {
        self.books = Some(books);
        self
    }
    /// Explicit host selection: mutations remain in the existing dataset and
    /// are observations, not Project decisions, permissions, or adoption.
    fn profile(&self) -> &'static str {
        if self.original_tool_behavior {
            "semantica.original-tool-compatibility/1"
        } else if self.native_tools {
            "semantica.native-tools/1"
        } else {
            PROFILE
        }
    }
    pub fn with_native_tools(mut self) -> Self {
        self.native_tools = true;
        self.extraction = Some(&semantica_runtime::extraction::RustPatternExtraction);
        self
    }
    /// Preserve actual canonical errors while migrating the original interface.
    pub fn with_original_tools(self) -> Self {
        let mut service = self.with_native_tools();
        service.original_tool_behavior = true;
        service
    }
    /// Learned extraction must reuse host Harness/pool/budget through this port.
    pub fn with_extraction_provider(mut self, port: &'a dyn SemanticExtractionPort) -> Self {
        self.extraction = Some(port);
        self
    }
    pub fn revision(&self) -> std::result::Result<u64, SemanticRuntimeFailure> {
        Ok(self
            .session
            .lock()
            .map_err(|_| failure("service_session_lock_failed", "session"))?
            .revision())
    }
    pub fn handle(
        &self,
        request: &Value,
    ) -> std::result::Result<Option<Value>, SemanticRuntimeFailure> {
        let result = self.dispatch_request(&SemanticMcpRequest {
            schema: "semantica.mcp-service-request/1".into(),
            expected_revision: self.revision()?,
            request: request.clone(),
        })?;
        Ok(if result.response.is_null() {
            None
        } else {
            Some(result.response)
        })
    }
    pub fn tools(&self) -> std::result::Result<Vec<SemanticMcpTool>, SemanticRuntimeFailure> {
        let dataset = self
            .session
            .lock()
            .map_err(|_| failure("service_session_lock_failed", "session"))?;
        self.tool_catalog(&dataset)
    }
    fn tool_catalog(
        &self,
        dataset: &DatasetSession,
    ) -> std::result::Result<Vec<SemanticMcpTool>, SemanticRuntimeFailure> {
        let catalog: Value = semantica_runtime::providers::parse_protocol_json(CATALOG)
            .map_err(|_| failure("service_catalog_invalid", "catalog"))?;
        catalog["tools"]
            .as_array()
            .ok_or_else(|| failure("service_catalog_invalid", "catalog"))?
            .iter()
            .map(|tool| {
                let available = (tool["name"] == "get_graph_summary"
                    && context_summary(dataset).is_ok())
                    || (self.native_tools&&!books::ALL_BOOK_TOOLS.contains(&tool["name"].as_str().unwrap_or("")))
                    || (self.books.is_some()
                        && (books::BOOK_TOOLS.iter().any(|n| tool["name"] == *n)||(self.native_tools&&books::ALL_BOOK_TOOLS.iter().any(|n|tool["name"]==*n))));
                Ok(SemanticMcpTool {
                    name: tool["name"].as_str().unwrap_or("").into(),
                    description: tool["description"].as_str().unwrap_or("").into(),
                    input_schema: tool["inputSchema"].clone(),
                    availability: if available {
                        "available"
                    } else {
                        "unavailable"
                    }
                    .into(),
                    reason: if available {
                        if tool["name"] == "get_graph_summary" {
                            "same DatasetSession context-record read"
                        } else {
                            "explicit native operations or existing registered book source; original behavior and remaining backend scope are separate"
                        }
                    } else {
                        "executor/provider/context profile not connected in this slice"
                    }
                    .into(),
                })
            })
            .collect()
    }
    fn catalog_wire(
        &self,
        dataset: &DatasetSession,
    ) -> std::result::Result<Value, SemanticRuntimeFailure> {
        Ok(Value::Array(self.tool_catalog(dataset)?.into_iter().map(|tool|json!({"name":tool.name,"description":tool.description,"inputSchema":tool.input_schema,"_meta":{"semantica":{"availability":tool.availability,"reason":tool.reason,"profile":self.profile()}}})).collect()))
    }
    fn resources(&self) -> Value {
        let catalog: Value = semantica_runtime::providers::parse_protocol_json(CATALOG)
            .expect("frozen public MCP catalog");
        let mut resources: Vec<Value> = catalog["fixed_resources"].as_array().expect("fixed catalog resources").iter().map(|r|{
            let mut r=r.clone();r["_meta"]=json!({"semantica":{"availability":if r["uri"]=="semantica://graph/summary"||r["uri"]=="semantica://schema/info"{"profile_dependent"}else{"unavailable"},"dynamic_manifests":"not_connected","profile":self.profile()}});r
        }).collect();
        if self.books.is_some() {
            let data = self.read_books("list", json!({}));
            if data["ok"] == true {
                for resource in &mut resources {
                    resource["_meta"]["semantica"]["dynamic_manifests"] =
                        json!("registered_book_source");
                }
                if let Some(registry) = resources
                    .iter_mut()
                    .find(|r| r["uri"] == "semantica://packages/registry")
                {
                    registry["_meta"]["semantica"]["availability"] = json!("available");
                }
                for package in data["packages"].as_array().into_iter().flatten() {
                    if let Some(id) = package["package_id"].as_str() {
                        resources.push(json!({"uri":format!("{}{id}",books::MANIFEST_PREFIX),"name":format!("Package manifest: {id}"),"description":"Source-grounded built-in semantic package manifest.","mimeType":"application/json","_meta":{"semantica":{"availability":"available","profile":self.profile()}}}));
                    }
                }
            }
        }
        Value::Array(resources)
    }
    fn read_books(&self, operation: &str, arguments: Value) -> Value {
        let Some(port) = self.books else {
            return unavailable("package_manifest_provider_unavailable");
        };
        let input = SemanticBookCatalogRequest {
            schema: "semantica.book-catalog-request/1".into(),
            operation: operation.into(),
            arguments,
        };
        match port.read(&input) {
            Ok(result) if result.validate().is_ok() => result.data,
            _ => {
                json!({"schema_version":"1.0","ok":false,"error":{"code":"package_operation_failed","message":"Package operation failed closed."}})
            }
        }
    }
    pub fn dispatch_request(
        &self,
        input: &SemanticMcpRequest,
    ) -> std::result::Result<SemanticMcpResult, SemanticRuntimeFailure> {
        require_contract(input.validate())?;
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "request".into(),
            bytes: serde_json::to_vec(&input.request)
                .map_err(|_| failure("service_wire_invalid", "wire"))?
                .len() as u64,
        })?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| failure("service_session_lock_failed", "session"))?;
        if session.revision() != input.expected_revision {
            return Err(failure("service_revision_mismatch", "session"));
        }
        // Stage with the existing DatasetSession algorithm. Publishing only after
        // output accounting and result-contract checks prevents a failed reply
        // from silently committing a context mutation to the authoritative borrow.
        let mut staged = session.clone();
        let response = self.dispatch_wire(&mut staged, &input.request)?;
        let dataset_sha256 = staged
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?
            .dataset_sha256;
        let wire = response.unwrap_or(Value::Null);
        if !wire.is_null() {
            self.budget.charge(&SemanticServiceBudgetCharge {
                kind: "response".into(),
                bytes: serde_json::to_vec(&wire)
                    .map_err(|_| failure("service_result_invalid", "wire"))?
                    .len() as u64,
            })?;
        }
        let result = SemanticMcpResult {
            schema: "semantica.mcp-service-result/1".into(),
            execution: "completed".into(),
            profile: self.profile().into(),
            revision: staged.revision(),
            dataset_sha256,
            response: wire,
        };
        require_contract(result.validate())?;
        **session = staged;
        Ok(result)
    }
    fn dispatch_wire(
        &self,
        session: &mut DatasetSession,
        request: &Value,
    ) -> std::result::Result<Option<Value>, SemanticRuntimeFailure> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let error = |code: i64, message: String| {
            Some(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
        };
        let ok = |value: Value| Some(json!({"jsonrpc":"2.0","id":id,"result":value}));
        let Some(request) = request.as_object() else {
            return Ok(error(-32600, "Invalid Request".into()));
        };
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            return Ok(error(-32600, "Invalid Request".into()));
        };
        if id.is_null() && method.starts_with("notifications/") {
            return Ok(None);
        }
        let params = request
            .get("params")
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(json!({}));
        if !params.is_object() {
            return Ok(error(-32602, "Request params must be an object".into()));
        }
        Ok(match method {
            "initialize" => ok(
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false},"resources":{"listChanged":false,"subscribe":false}},"serverInfo":{"name":"semantica","version":env!("CARGO_PKG_VERSION")},"_meta":{"semantica":{"profile":self.profile(),"lifecycle":"legacy-fixed-version; strict negotiation not_connected","engineering_adoption":"not_established"}}}),
            ),
            "notifications/initialized" => None,
            "ping" => ok(json!({})),
            "prompts/list" => ok(json!({"prompts":[]})),
            "tools/list" => ok(json!({"tools":self.catalog_wire(session)?})),
            "resources/list" => ok(json!({"resources":self.resources()})),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                let args = params
                    .get("arguments")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or(json!({}));
                let catalog = self.tool_catalog(session)?;
                if !catalog.iter().any(|t| t.name == name) {
                    error(-32601, format!("Unknown tool: {name}"))
                } else if !args.is_object() {
                    error(-32602, "Tool arguments must be an object".into())
                } else {
                    if self.native_tools
                        && self.original_tool_behavior
                        && name == "extract_relations"
                        && args["text"].as_str().is_some_and(|s| !s.is_empty())
                    {
                        return Ok(error(-32603,"Tool 'extract_relations' failed (TypeError). See server logs for details.".into()));
                    }
                    let data = if name == "get_graph_summary" {
                        context_summary(session)
                            .unwrap_or_else(|_| unavailable("context_projection_unavailable"))
                    } else if self.books.is_some() && books::BOOK_TOOLS.contains(&name) {
                        self.read_books(
                            if name == "list_chapter_packages" {
                                "list"
                            } else {
                                "get"
                            },
                            args,
                        )
                    } else if self.native_tools && books::ALL_BOOK_TOOLS.contains(&name) {
                        self.read_books(
                            match name {
                                "verify_book_sources" => "verify_sources",
                                "run_chapter_package" => "run",
                                "verify_chapter_package" => "verify",
                                _ => "get",
                            },
                            args,
                        )
                    } else if self.native_tools {
                        self.native_tool(session, name, &args)
                    } else {
                        unavailable("tool_unavailable")
                    };
                    if self.original_tool_behavior
                        && name == "get_causal_chain"
                        && data["chain"].as_array().is_some_and(|v| !v.is_empty())
                    {
                        return Ok(error(-32603,"Tool 'get_causal_chain' failed (TypeError). See server logs for details.".into()));
                    }
                    if self.original_tool_behavior
                        && name == "add_relationship"
                        && data["error"].as_str().is_some_and(|e| {
                            e.starts_with("SKOS hierarchy contains a cycle involving '")
                        })
                    {
                        return Ok(error(-32603,"Tool 'add_relationship' failed (ValueError). See server logs for details.".into()));
                    }
                    let is_error =
                        data.get("ok") == Some(&Value::Bool(false)) || data.get("error").is_some();
                    let mut payload = json!({"content":[{"type":"text","text":pretty(&data)?}]});
                    if is_error {
                        payload["isError"] = json!(true);
                    }
                    ok(payload)
                }
            }
            "resources/read" => {
                let uri = params["uri"].as_str().unwrap_or("");
                let data = match uri {
                    "semantica://graph/summary" => context_summary(session)
                        .unwrap_or_else(|_| unavailable("context_projection_unavailable")),
                    "semantica://schema/info" => {
                        json!({"name":"Semantica","version":env!("CARGO_PKG_VERSION"),"tools":self.tool_catalog(session)?.iter().map(|t|t.name.clone()).collect::<Vec<_>>(),"resources":self.resources().as_array().unwrap().iter().map(|r|r["uri"].clone()).collect::<Vec<_>>(),"profile":self.profile(),"dynamic_manifests":if self.books.is_some(){"registered_book_source"}else{"not_connected"},"engineering_adoption":"not_established"})
                    }
                    "semantica://packages/registry" if self.books.is_some() => {
                        self.read_books("list", json!({}))
                    }
                    "semantica://decisions/list" if self.native_tools => {
                        self.native_tool(session, "query_decisions", &json!({"limit":50}))
                    }
                    "semantica://decisions/list" | "semantica://packages/registry" => {
                        unavailable("resource_executor_unavailable")
                    }
                    _ if uri.starts_with("semantica://packages/manifest/") => {
                        if self.books.is_some() {
                            self.read_books("get",json!({"package_id":uri.strip_prefix(books::MANIFEST_PREFIX).unwrap_or("")}))
                        } else {
                            unavailable("package_manifest_provider_unavailable")
                        }
                    }
                    _ => {
                        json!({"ok":false,"error":{"code":"resource_not_found","message":"Unknown resource URI."}})
                    }
                };
                ok(
                    json!({"contents":[{"uri":uri,"mimeType":"application/json","text":pretty(&data)?}]}),
                )
            }
            _ => error(-32601, format!("Method not found: {method}")),
        })
    }
    fn native_tool(&self, session: &mut DatasetSession, name: &str, args: &Value) -> Value {
        if ["extract_entities", "extract_relations"].contains(&name) {
            let Some(text) = args["text"].as_str().filter(|s| !s.is_empty()) else {
                return json!({"error":"text is required"});
            };
            let Some(port) = self.extraction else {
                return unavailable("extraction_provider_unavailable");
            };
            let input = SemanticExtractionRequest {
                schema: "semantica.extraction-request/1".into(),
                operation: if name == "extract_entities" {
                    "entities"
                } else {
                    "relations"
                }
                .into(),
                text: text.into(),
            };
            return match port.extract(&input) {
                Ok(result) if result.validate().is_ok() => {
                    let mut data = result.data;
                    data["_meta"] = json!({"profile":result.profile,"model_execution":result.model_execution,"uncovered":result.uncovered,"source_utf8_sha256":semantica_runtime::sha256(text),"engineering_adoption":"not_established"});
                    data
                }
                _ => unavailable("extraction_failed"),
            };
        }
        if name == "run_reasoning" {
            let facts = args.get("facts").cloned().unwrap_or(json!([]));
            let rules = args.get("rules").cloned().unwrap_or(json!([]));
            if facts.as_array().is_some_and(Vec::is_empty)
                || rules.as_array().is_some_and(Vec::is_empty)
            {
                return json!({"error":"facts and rules are required"});
            }
            return match semantica_runtime::reasoner::infer_facts(&facts, &rules) {
                Ok(derived) => json!({"derived_facts":derived}),
                Err(_) => unavailable("reasoning_failed"),
            };
        }
        if self.original_tool_behavior && name == "get_graph_analytics" {
            return json!({"error":"PageRank calculation failed: 'dict' object is not callable"});
        }
        if self.original_tool_behavior && name == "export_graph" {
            return json!({"error":if args.get("format").and_then(Value::as_str).is_some_and(|s|!["turtle","ttl","nt","xml","json-ld"].contains(&s)){"JSONExporter.export() missing 1 required positional argument: 'file_path'"}else{"'ContextGraph' object has no attribute 'get'"}});
        }
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let id = if name == "record_decision" {
            match semantica_runtime::context::new_decision_id() {
                Ok(id) => id,
                Err(_) => {
                    return json!({"error":"context_event_entropy_unavailable","execution":"failed"});
                }
            }
        } else {
            "unused-observation".into()
        };
        let recorded_at = semantica_runtime::context::observation_time(timestamp)
            .unwrap_or_else(|_| "1970-01-01T00:00:00".into());
        let event = semantica_runtime::context::DecisionEvent {
            id: &id,
            timestamp,
            recorded_at: &recorded_at,
        };
        semantica_runtime::context::execute_tool(session,name,args,&event).unwrap_or_else(|e|json!({"error":e.to_string(),"execution":"failed","engineering_adoption":"not_established"}))
    }
    pub fn execute_query(
        &self,
        input: &SemanticServiceQueryRequest,
    ) -> std::result::Result<SemanticServiceQueryResult, SemanticRuntimeFailure> {
        require_contract(input.validate())?;
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "query".into(),
            bytes: input.query.len() as u64,
        })?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| failure("service_session_lock_failed", "session"))?;
        if session.revision() != input.expected_revision {
            return Err(failure("service_revision_mismatch", "session"));
        }
        let dataset_sha256 = session
            .snapshot()
            .map_err(|_| failure("service_snapshot_failed", "session"))?
            .dataset_sha256;
        if dataset_sha256 != input.expected_dataset_sha256 {
            return Err(failure("service_dataset_identity_mismatch", "source"));
        }
        let result = session
            .query(&input.query, input.use_cache)
            .map_err(|_| failure("service_query_failed", "query"))?;
        self.budget.charge(&SemanticServiceBudgetCharge {
            kind: "response".into(),
            bytes: serde_json::to_vec(&result.result)
                .map_err(|_| failure("service_result_invalid", "query"))?
                .len() as u64,
        })?;
        let result = SemanticServiceQueryResult {
            schema: "semantica.service-query-result/1".into(),
            execution: "completed".into(),
            profile: self.profile().into(),
            dataset_sha256,
            result,
        };
        require_contract(result.validate())?;
        Ok(result)
    }
}
impl SemanticMcpPort for McpService<'_> {
    fn dispatch(
        &self,
        input: &SemanticMcpRequest,
    ) -> std::result::Result<SemanticMcpResult, SemanticRuntimeFailure> {
        self.dispatch_request(input)
    }
}
impl SemanticServiceQueryPort for McpService<'_> {
    fn query(
        &self,
        input: &SemanticServiceQueryRequest,
    ) -> std::result::Result<SemanticServiceQueryResult, SemanticRuntimeFailure> {
        self.execute_query(input)
    }
}
fn unavailable(code: &str) -> Value {
    json!({"ok":false,"execution":"not_run","error":{"code":code,"message":"Executor/provider is not connected in this declared service slice."}})
}
fn pretty(value: &Value) -> std::result::Result<String, SemanticRuntimeFailure> {
    fn sorted(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys = m.keys().collect::<Vec<_>>();
                keys.sort();
                Value::Object(
                    keys.into_iter()
                        .map(|k| (k.clone(), sorted(&m[k])))
                        .collect(),
                )
            }
            Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
            _ => v.clone(),
        }
    }
    serde_json::to_string_pretty(&sorted(value))
        .map_err(|_| failure("service_wire_serialization_failed", "wire"))
}

/// Explicit old entry profiles; native I/O never creates another dataset or engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdioProfile {
    Canonical,
    CompatibilityBatch,
}
/// EOF-terminated bounded binary UTF8 newline processing. No socket or external process.
pub fn serve_stdio<R: BufRead, W: Write, D: Write>(
    service: &McpService<'_>,
    reader: &mut R,
    writer: &mut W,
    diagnostics: &mut D,
    profile: StdioProfile,
) -> Result<()> {
    loop {
        let mut frame = Vec::new();
        let mut eof = false;
        loop {
            let chunk = reader.fill_buf()?;
            if chunk.is_empty() {
                eof = true;
                break;
            }
            let take = chunk
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| i + 1)
                .unwrap_or(chunk.len());
            ensure!(
                frame.len() + take <= MAX_FRAME,
                "service_frame_budget_exceeded"
            );
            let ended = chunk[take - 1] == b'\n';
            frame.extend_from_slice(&chunk[..take]);
            reader.consume(take);
            if ended {
                break;
            }
        }
        if frame.is_empty() && eof {
            return Ok(());
        }
        let trimmed = frame.iter().all(u8::is_ascii_whitespace);
        if trimmed {
            service
                .budget
                .charge(&SemanticServiceBudgetCharge {
                    kind: "framing".into(),
                    bytes: frame.len() as u64,
                })
                .map_err(|e| anyhow::anyhow!(e.code))?;
            if eof {
                return Ok(());
            }
            continue;
        }
        let parsed = std::str::from_utf8(&frame)
            .ok()
            .and_then(|s| semantica_runtime::providers::parse_protocol_json(s).ok());
        let response = match parsed {
            None => {
                service
                    .budget
                    .charge(&SemanticServiceBudgetCharge {
                        kind: "request".into(),
                        bytes: frame.len() as u64,
                    })
                    .map_err(|e| anyhow::anyhow!(e.code))?;
                Some(
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error: invalid UTF-8 or JSON"}}),
                )
            }
            Some(Value::Array(batch)) if profile == StdioProfile::CompatibilityBatch => {
                let normalized = batch
                    .iter()
                    .map(|v| serde_json::to_vec(v).map(|b| b.len()))
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .sum::<usize>();
                service
                    .budget
                    .charge(&SemanticServiceBudgetCharge {
                        kind: "framing".into(),
                        bytes: frame.len().saturating_sub(normalized) as u64,
                    })
                    .map_err(|e| anyhow::anyhow!(e.code))?;
                let mut results = Vec::new();
                for request in batch {
                    if let Some(value) = service
                        .handle(&request)
                        .map_err(|e| anyhow::anyhow!(e.code))?
                    {
                        results.push(value);
                    }
                }
                if results.is_empty() {
                    None
                } else {
                    Some(Value::Array(results))
                }
            }
            Some(Value::Array(_)) => {
                service
                    .budget
                    .charge(&SemanticServiceBudgetCharge {
                        kind: "request".into(),
                        bytes: frame.len() as u64,
                    })
                    .map_err(|e| anyhow::anyhow!(e.code))?;
                writeln!(
                    diagnostics,
                    "canonical legacy batch unsupported; no response"
                )?;
                None
            }
            Some(request) => {
                let normalized = serde_json::to_vec(&request)?.len();
                service
                    .budget
                    .charge(&SemanticServiceBudgetCharge {
                        kind: "framing".into(),
                        bytes: frame.len().saturating_sub(normalized) as u64,
                    })
                    .map_err(|e| anyhow::anyhow!(e.code))?;
                service
                    .handle(&request)
                    .map_err(|e| anyhow::anyhow!(e.code))?
            }
        };
        if let Some(response) = response {
            let bytes = serde_json::to_vec(&response)?;
            // Valid dispatches are already charged; framing parse/batch aggregation adds its exact byte delta.
            if response
                .get("error")
                .and_then(|e| e.get("code"))
                .and_then(Value::as_i64)
                == Some(-32700)
            {
                service
                    .budget
                    .charge(&SemanticServiceBudgetCharge {
                        kind: "response".into(),
                        bytes: (bytes.len() + 1) as u64,
                    })
                    .map_err(|e| anyhow::anyhow!(e.code))?;
            } else {
                service
                    .budget
                    .charge(&SemanticServiceBudgetCharge {
                        kind: "response".into(),
                        bytes: if response.is_array() {
                            bytes.len().saturating_sub(
                                response
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|v| serde_json::to_vec(v).map(|b| b.len()).unwrap_or(0))
                                    .sum::<usize>(),
                            ) as u64
                                + 1
                        } else {
                            1
                        },
                    })
                    .map_err(|e| anyhow::anyhow!(e.code))?;
            }
            writer.write_all(&bytes)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        if eof {
            return Ok(());
        }
    }
}

/// Explicit finite Explorer HTTP route; application keeps the sole session/auth/budget owners.
pub mod explorer_http;
pub mod explorer_sparql;
pub mod explorer_sparql_http;

pub mod explorer_path;

pub mod explorer_path_details;

pub mod explorer_path_route;

pub mod explorer_path_http;

pub mod explorer_read;
pub mod explorer_read_http;
pub mod explorer_read_route;

pub mod explorer_search;
pub mod explorer_search_http;
pub mod explorer_search_route;

mod explorer_json;

pub mod explorer_decisions;

pub mod explorer_decisions_http;
pub mod explorer_decisions_route;

pub mod explorer_causal_distance;
