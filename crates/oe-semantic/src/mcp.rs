//! Semantica-only MCP tool catalog, command building, and result interpretation.
//!
//! Ported from skills/cad-agent/scripts/mcp_stdio.py.
//! Owns the tool definitions, argv construction, and response classification.
//! The MCP server wiring (anyio + Python mcp SDK) remains in the Python entry
//! point, which shells out to the `oe` CLI — exactly as it already does.
//! CAD execution uses direct NX journals and is not exposed by this transport.

use anyhow::{Result, ensure};
use serde_json::{Map, Value, json};

pub const TRANSPORT_VERSION: &str = "1.1.0";
pub const TRANSPORT_NAME: &str = "ontology-engineering";
pub const RESPONSE_SCHEMA: &str = "ontology-engineering.semantic-engagement-response/v1";

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ToolField {
    pub name: &'static str,
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct SemanticTool {
    pub name: &'static str,
    pub description: &'static str,
    pub fields: &'static [ToolField],
}

impl SemanticTool {
    /// Build the JSON Schema `inputSchema` for this tool.
    pub fn input_schema(&self) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for field in self.fields {
            properties.insert(
                field.name.to_owned(),
                json!({"type": "string", "minLength": 1}),
            );
            if field.required {
                required.push(Value::String(field.name.to_owned()));
            }
        }
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        })
    }

    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema(),
            "annotations": {
                "readOnlyHint": true,
                "destructiveHint": false,
                "idempotentHint": true,
                "openWorldHint": false,
            }
        })
    }
}

// Macro shorthand: field!("name", required)
macro_rules! field {
    ($name:literal, required) => {
        ToolField {
            name: $name,
            required: true,
        }
    };
    ($name:literal, optional) => {
        ToolField {
            name: $name,
            required: false,
        }
    };
}

pub const SEMANTIC_TOOLS: &[SemanticTool] = &[
    SemanticTool {
        name: "semantic_doctor",
        description: "Verify the canonical Semantica source lock and runtime; no ontology execution.",
        fields: &[field!("binding", optional)],
    },
    SemanticTool {
        name: "semantic_discover",
        description: "Discover source-locked packages and capabilities; no promotion or release.",
        fields: &[field!("binding", optional), field!("workspace", optional)],
    },
    SemanticTool {
        name: "semantic_run",
        description: "Run an exact bound package/scenario through Semantica. Requires a current task; inspect native verdicts and receipts.",
        fields: &[
            field!("binding", required),
            field!("task", required),
            field!("workspace", optional),
            field!("scenario", optional),
            field!("recorded_at", optional),
        ],
    },
    SemanticTool {
        name: "semantic_review",
        description: "Review project evidence against an adopted promoted package through Semantica; never physical or manufacturing acceptance.",
        fields: &[
            field!("binding", required),
            field!("task", required),
            field!("workspace", required),
            field!("evidence_file", required),
            field!("source_id", required),
            field!("format", required),
            field!("scope", required),
            field!("focus", required),
            field!("focus_type", required),
            field!("query_asset", required),
            field!("shape_asset", required),
            field!("recorded_at", optional),
        ],
    },
];

// ---------------------------------------------------------------------------
// Tool catalog
// ---------------------------------------------------------------------------

pub fn tool_catalog(mode: &str) -> Result<Vec<Value>> {
    ensure!(
        mode == "semantica",
        "Only Semantica is available in this MCP transport; CAD uses NXDirect"
    );
    Ok(SEMANTIC_TOOLS.iter().map(SemanticTool::to_json).collect())
}

// ---------------------------------------------------------------------------
// Command builder
// ---------------------------------------------------------------------------

/// Map a tool call into argv for the `oe` CLI subprocess.
/// `oe_executable`: absolute path to the compiled `oe` binary.
/// `root`: skill root passed as `--root`.
/// Returns the full argv slice (first element is the executable).
pub fn build_command(
    oe_executable: &str,
    root: &str,
    mode: &str,
    tool_name: &str,
    arguments: &Map<String, Value>,
) -> Result<Vec<String>> {
    ensure!(mode == "semantica", "unsupported_transport_mode");

    let tool = SEMANTIC_TOOLS
        .iter()
        .find(|t| t.name == tool_name)
        .ok_or_else(|| anyhow::anyhow!("unknown tool for this transport: {tool_name}"))?;

    // Validate required fields are present
    for field in tool.fields {
        if field.required {
            ensure!(
                arguments
                    .get(field.name)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty()),
                "required field missing or empty: {}",
                field.name
            );
        }
    }

    // Fixed tool names select fixed CLI subcommands. Values stay values,
    // never new options — no shell expansion, no arbitrary argv surface.
    let subcommand = tool_name.trim_start_matches("semantic_");

    let mut argv = vec![
        oe_executable.to_owned(),
        "--root".to_owned(),
        root.to_owned(),
        "semantics".to_owned(),
        subcommand.to_owned(),
    ];

    for field in tool.fields {
        if let Some(value) = arguments.get(field.name).and_then(Value::as_str)
            && !value.is_empty()
        {
            argv.push(format!("--{}={}", field.name.replace('_', "-"), value));
        }
    }

    Ok(argv)
}

// ---------------------------------------------------------------------------
// Result interpreter
// ---------------------------------------------------------------------------

pub struct InvokeResult {
    pub ok: bool,
    pub payload: Value,
    pub is_error: bool,
}

impl InvokeResult {
    pub fn to_json(&self) -> Value {
        json!({
            "ok": self.ok,
            "is_error": self.is_error,
            "payload": self.payload,
        })
    }
}

/// Classify a subprocess result into an MCP tool result.
/// `stdout_json`: the parsed JSON from subprocess stdout (or null on parse failure).
/// `exit_code`: subprocess exit code.
pub fn result_for(mode: &str, stdout_json: Value, exit_code: i32) -> Result<InvokeResult> {
    ensure!(mode == "semantica", "unsupported transport mode: {mode}");

    let ok = exit_code == 0
        && stdout_json.get("$schema").and_then(Value::as_str) == Some(RESPONSE_SCHEMA)
        && stdout_json.get("command_verdict").and_then(Value::as_str) == Some("passed");

    // Do not replace the canonical taxonomy, native receipt, or blocked verdict.
    Ok(InvokeResult {
        ok,
        is_error: !ok,
        payload: stdout_json,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_catalog_only_allows_semantica_mode() {
        assert!(tool_catalog("semantica").is_ok());
        assert!(tool_catalog("cad").is_err());
        assert!(tool_catalog("").is_err());
    }

    #[test]
    fn tool_catalog_returns_four_tools() {
        let tools = tool_catalog("semantica").unwrap();
        assert_eq!(tools.len(), 4);
    }

    #[test]
    fn build_command_doctor_produces_correct_argv() {
        let mut args = Map::new();
        args.insert("binding".into(), json!("/path/to/binding.json"));
        let argv = build_command(
            "/usr/local/bin/oe",
            "/skill",
            "semantica",
            "semantic_doctor",
            &args,
        )
        .unwrap();
        assert_eq!(argv[0], "/usr/local/bin/oe");
        assert_eq!(argv[1], "--root");
        assert_eq!(argv[2], "/skill");
        assert_eq!(argv[3], "semantics");
        assert_eq!(argv[4], "doctor");
        assert!(argv.iter().any(|a| a.contains("binding=")));
    }

    #[test]
    fn build_command_run_requires_binding_and_task() {
        let mut args = Map::new();
        args.insert("binding".into(), json!("/binding.json"));
        // Missing task — should fail
        let result = build_command("/oe", "/root", "semantica", "semantic_run", &args);
        assert!(result.is_err());

        args.insert("task".into(), json!("/task.json"));
        let result = build_command("/oe", "/root", "semantica", "semantic_run", &args);
        assert!(result.is_ok());
    }

    #[test]
    fn result_for_ok_when_schema_and_verdict_match() {
        let payload = json!({
            "$schema": RESPONSE_SCHEMA,
            "command_verdict": "passed",
        });
        let r = result_for("semantica", payload, 0).unwrap();
        assert!(r.ok);
        assert!(!r.is_error);
    }

    #[test]
    fn result_for_error_on_nonzero_exit() {
        let payload = json!({"$schema": RESPONSE_SCHEMA, "command_verdict": "passed"});
        let r = result_for("semantica", payload, 1).unwrap();
        assert!(!r.ok);
        assert!(r.is_error);
    }

    #[test]
    fn result_for_error_when_verdict_blocked() {
        let payload = json!({"$schema": RESPONSE_SCHEMA, "command_verdict": "blocked"});
        let r = result_for("semantica", payload, 0).unwrap();
        assert!(!r.ok);
        assert!(r.is_error);
    }
}
