//! MCP response parsing (`semantica/parse/mcp_parser.py`).
//!
//! [`MCPParser`] turns resource responses and tool outputs of any MCP server into dicts,
//! decoding JSON text content and CSV-like text tables. Inputs are JSON values (the
//! original's dicts); non-dict inputs fail with the original's `AttributeError` text
//! wrapped in `ProcessingError`.

use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::pycompat::fmt::{strip, truthy, type_name};
use sem_utils::{ProcessingError, SemanticaError};

use crate::common::{Options, PResult, attribute_error, bind_kwarg, merge_config, type_error};
use crate::document_parser::DocumentParser;
use crate::pyjson;
use crate::structured_data_parser::StructuredDataParser;

/// `MCPParser(config=None, **kwargs)`: generic parser for MCP server responses.
#[derive(Debug, Clone)]
pub struct MCPParser {
    /// `self.logger`.
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.structured_parser` (`StructuredDataParser(**self.config)`).
    pub structured_parser: StructuredDataParser,
    /// `self.document_parser` (`DocumentParser(**self.config)`).
    pub document_parser: DocumentParser,
}

impl Default for MCPParser {
    fn default() -> Self {
        MCPParser::new(None, Options::new())
    }
}

/// `Parser(**config)` for `(config=None, **kwargs)` constructors: a `config` key binds the
/// `config` parameter.
fn split(config: &Options) -> (Option<Options>, Options) {
    let (cfg, rest) = bind_kwarg(config, "config");
    match cfg {
        Some(Value::Object(m)) => (Some(m), rest),
        _ => (None, rest),
    }
}

fn dict_get<'a>(value: &'a Value, key: &str) -> PResult<Option<&'a Value>> {
    match value {
        Value::Object(m) => Ok(m.get(key)),
        other => Err(attribute_error(type_name(other), "get")),
    }
}

/// Iterate a value as `for item in value` does.
fn py_iter(value: &Value) -> PResult<Vec<Value>> {
    match value {
        Value::Array(items) => Ok(items.clone()),
        Value::Object(m) => Ok(m.keys().map(|k| Value::String(k.clone())).collect()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string())).collect()),
        other => Err(type_error(format!(
            "'{}' object is not iterable",
            type_name(other)
        ))),
    }
}

fn str_strip(value: &Value) -> PResult<String> {
    match value {
        Value::String(s) => Ok(strip(s).to_string()),
        other => Err(attribute_error(type_name(other), "strip")),
    }
}

impl MCPParser {
    /// `MCPParser(config=None, **kwargs)`.
    pub fn new(config: Option<Options>, kwargs: Options) -> Self {
        let config = merge_config(config, kwargs);
        let (s_cfg, s_rest) = split(&config);
        let (d_cfg, d_rest) = split(&config);
        let parser = MCPParser {
            logger: get_logger("mcp_parser"),
            structured_parser: StructuredDataParser::new(s_cfg, s_rest),
            document_parser: DocumentParser::new(d_cfg, d_rest),
            config,
        };
        parser.logger.debug("MCP parser initialized");
        parser
    }

    /// `parse_resource_response(resource_data, **options)`: `{"type": "resource", "raw",
    /// "content", "metadata", "parsed_content", "uri", "mimeType"}`.
    pub fn parse_resource_response(
        &self,
        resource_data: &Value,
        _options: &Options,
    ) -> PResult<Map<String, Value>> {
        self.parse_response(resource_data, "resource", "contents")
            .map_err(|e| {
                self.logger
                    .error(format!("Failed to parse resource response: {e}"));
                ProcessingError::new(format!("Failed to parse resource response: {e}")).into()
            })
    }

    /// `parse_tool_output(tool_result, **options)`: `{"type": "tool_output", "raw",
    /// "content", "metadata", "parsed_content", "isError"}`.
    pub fn parse_tool_output(
        &self,
        tool_result: &Value,
        _options: &Options,
    ) -> PResult<Map<String, Value>> {
        self.parse_response(tool_result, "tool_output", "content")
            .map_err(|e| {
                self.logger
                    .error(format!("Failed to parse tool output: {e}"));
                ProcessingError::new(format!("Failed to parse tool output: {e}")).into()
            })
    }

    fn parse_response(
        &self,
        data: &Value,
        kind: &str,
        content_key: &str,
    ) -> PResult<Map<String, Value>> {
        let mut result = Map::new();
        result.insert("type".to_string(), Value::String(kind.to_string()));
        result.insert("raw".to_string(), data.clone());
        result.insert("content".to_string(), Value::Null);
        result.insert("metadata".to_string(), Value::Object(Map::new()));
        result.insert("parsed_content".to_string(), Value::Null);

        let content = dict_get(data, content_key)?
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        if truthy(&content) {
            let mut parsed_contents = Vec::new();
            for item in py_iter(&content)? {
                if let Some(parsed) = self.parse_content_item(&item)
                    && !parsed.is_empty()
                {
                    parsed_contents.push(Value::Object(parsed));
                }
            }
            let parsed_content = if parsed_contents.len() == 1 {
                parsed_contents[0].clone()
            } else {
                Value::Array(parsed_contents.clone())
            };
            result.insert("content".to_string(), Value::Array(parsed_contents));
            result.insert("parsed_content".to_string(), parsed_content);
        }
        result.insert(
            "metadata".to_string(),
            dict_get(data, "metadata")?
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new())),
        );
        if kind == "resource" {
            result.insert(
                "uri".to_string(),
                dict_get(data, "uri")?.cloned().unwrap_or(Value::Null),
            );
            result.insert(
                "mimeType".to_string(),
                dict_get(data, "mimeType")?.cloned().unwrap_or(Value::Null),
            );
        } else {
            result.insert(
                "isError".to_string(),
                dict_get(data, "isError")?
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            );
        }
        Ok(result)
    }

    /// `_parse_content_item(item)`: `None` (logged) when the item cannot be parsed.
    fn parse_content_item(&self, item: &Value) -> Option<Map<String, Value>> {
        match self.try_parse_content_item(item) {
            Ok(parsed) => Some(parsed),
            Err(e) => {
                self.logger
                    .warning(format!("Failed to parse content item: {e}"));
                None
            }
        }
    }

    fn try_parse_content_item(&self, item: &Value) -> PResult<Map<String, Value>> {
        let content_type = dict_get(item, "type")?
            .cloned()
            .unwrap_or_else(|| Value::String("text".to_string()));
        let text = dict_get(item, "text")?
            .cloned()
            .unwrap_or_else(|| Value::String(String::new()));
        let data = dict_get(item, "data")?.cloned().unwrap_or(Value::Null);
        let mime_type = dict_get(item, "mimeType")?.cloned().unwrap_or(Value::Null);

        let mut parsed = Map::new();
        parsed.insert("type".to_string(), content_type.clone());
        parsed.insert("mimeType".to_string(), mime_type.clone());
        parsed.insert("raw".to_string(), item.clone());

        let ct = content_type.as_str();
        if ct == Some("text") {
            parsed.insert("text".to_string(), text.clone());
            let stripped = str_strip(&text)?;
            if stripped.starts_with('{') || stripped.starts_with('[') {
                try_json(&text, &mut parsed)?;
            }
        } else if ct == Some("resource") {
            parsed.insert(
                "resource".to_string(),
                dict_get(item, "resource")?.cloned().unwrap_or(Value::Null),
            );
        } else if ct == Some("image") && truthy(&data) {
            parsed.insert("image_data".to_string(), data.clone());
            parsed.insert("image_format".to_string(), mime_type.clone());
        } else {
            parsed.insert("data".to_string(), data.clone());
        }

        if parsed.contains_key("text") && !parsed.contains_key("structured") {
            let text_content = parsed["text"].clone();
            let stripped = str_strip(&text_content)?;
            if stripped.starts_with('{') || stripped.starts_with('[') {
                try_json(&text_content, &mut parsed)?;
            } else if let Value::String(t) = &text_content
                && (t.contains('\t') || t.contains(','))
            {
                let lines: Vec<&str> = stripped.split('\n').collect();
                if lines.len() > 1 {
                    parsed.insert(
                        "structured".to_string(),
                        Value::Array(parse_csv_like(t).into_iter().map(Value::Object).collect()),
                    );
                }
            }
        }
        Ok(parsed)
    }

    /// `parse_mcp_data(mcp_data, data_type=None, **options)`: dispatch on `data_type`, or
    /// detect it (`contents` -> resource, otherwise tool output) for dicts.
    pub fn parse_mcp_data(
        &self,
        mcp_data: &Value,
        data_type: Option<&str>,
        options: &Options,
    ) -> PResult<Map<String, Value>> {
        let result = (|| -> PResult<Map<String, Value>> {
            let data_type = match data_type.filter(|t| !t.is_empty()) {
                Some(t) => t.to_string(),
                None => match mcp_data {
                    Value::Object(m) => {
                        if m.contains_key("contents") {
                            "resource".to_string()
                        } else {
                            "tool_output".to_string()
                        }
                    }
                    _ => return Err(ProcessingError::new("Cannot determine MCP data type").into()),
                },
            };
            match data_type.as_str() {
                "resource" => self.parse_resource_response(mcp_data, options),
                "tool_output" => self.parse_tool_output(mcp_data, options),
                other => {
                    Err(ProcessingError::new(format!("Unknown MCP data type: {other}")).into())
                }
            }
        })();
        result.map_err(|e: SemanticaError| {
            self.logger.error(format!("Failed to parse MCP data: {e}"));
            ProcessingError::new(format!("Failed to parse MCP data: {e}")).into()
        })
    }
}

/// `json.loads(text)` into `parsed["structured"]`, ignoring `JSONDecodeError` (other errors,
/// such as a `RecursionError`, propagate).
fn try_json(text: &Value, parsed: &mut Map<String, Value>) -> PResult<()> {
    let Value::String(t) = text else {
        return Err(type_error(format!(
            "the JSON object must be str, bytes or bytearray, not {}",
            type_name(text)
        )));
    };
    match pyjson::loads(t) {
        Ok(v) => {
            parsed.insert("structured".to_string(), v);
            Ok(())
        }
        Err(e) if e.recursion => Err(e.into()),
        Err(_) => Ok(()),
    }
}

/// `_parse_csv_like(text)`: header line plus rows with the same number of fields
/// (tab-delimited when the header has a tab, else comma-delimited).
fn parse_csv_like(text: &str) -> Vec<Map<String, Value>> {
    let stripped = strip(text);
    let lines: Vec<&str> = stripped.split('\n').collect();
    let delimiter = if lines[0].contains('\t') { "\t" } else { "," };
    let header: Vec<String> = lines[0]
        .split(delimiter)
        .map(|h| strip(h).to_string())
        .collect();
    let mut rows = Vec::new();
    for line in &lines[1..] {
        if strip(line).is_empty() {
            continue;
        }
        let values: Vec<String> = line
            .split(delimiter)
            .map(|v| strip(v).to_string())
            .collect();
        if values.len() == header.len() {
            let mut row = Map::new();
            for (h, v) in header.iter().zip(values) {
                row.insert(h.clone(), Value::String(v));
            }
            rows.push(row);
        }
    }
    rows
}
