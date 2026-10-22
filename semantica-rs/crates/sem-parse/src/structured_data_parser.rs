//! Structured data parsing (`semantica/parse/structured_data_parser.py`).
//!
//! [`StructuredDataParser`] routes JSON, CSV and XML to their parsers and parses YAML
//! itself; it also validates data against a minimal JSON-schema-like dict, converts between
//! formats (JSON, CSV, XML, YAML text) and infers schemas.
//!
//! Results are the parsers' dataclass fields as dicts (`JSONData.__dict__`, ...). The XML
//! result holds an `XMLElement` object in Python; here it is that element's field map.

use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};
use sem_utils::progress_tracker::{ProgressTracker, get_progress_tracker};
use sem_utils::pycompat::fmt::{py_str, type_name};
use sem_utils::{SemanticaError, ValidationError};

use crate::common::{
    DecodeErrors, Options, PResult, Source, attribute_error, merge_config, py_name, py_path_str,
    py_read_text, py_suffix, sub_config, type_error,
};
use crate::csv_parser::CSVParser;
use crate::json_parser::JSONParser;
use crate::pycsv::{Dialect, write_row, write_value_row};
use crate::xml_parser::XMLParser;

/// An argument of [`StructuredDataParser::convert_data`]: a path or data string to parse,
/// or already-parsed data.
#[derive(Debug, Clone, PartialEq)]
pub enum StructuredInput {
    /// A `str` or `Path`: parsed with `parse_data` first.
    Source(Source),
    /// Any other value: converted as is.
    Data(Value),
}

impl From<Source> for StructuredInput {
    fn from(value: Source) -> Self {
        StructuredInput::Source(value)
    }
}

impl From<&str> for StructuredInput {
    fn from(value: &str) -> Self {
        StructuredInput::Source(Source::from(value))
    }
}

impl From<String> for StructuredInput {
    fn from(value: String) -> Self {
        StructuredInput::Source(Source::from(value))
    }
}

impl From<&Path> for StructuredInput {
    fn from(value: &Path) -> Self {
        StructuredInput::Source(Source::from(value))
    }
}

impl From<std::path::PathBuf> for StructuredInput {
    fn from(value: std::path::PathBuf) -> Self {
        StructuredInput::Source(Source::from(value))
    }
}

impl From<Value> for StructuredInput {
    /// A JSON string is a Python `str` (and is parsed); other values are data.
    fn from(value: Value) -> Self {
        match value {
            Value::String(s) => StructuredInput::Source(Source::Text(s)),
            other => StructuredInput::Data(other),
        }
    }
}

/// `StructuredDataParser(config=None, **kwargs)`: structured data format parsing handler.
#[derive(Debug, Clone)]
pub struct StructuredDataParser {
    /// `self.logger`.
    pub logger: Logger,
    /// `self.config`.
    pub config: Options,
    /// `self.progress_tracker`.
    pub progress_tracker: Arc<ProgressTracker>,
    /// `self.json_parser` (`JSONParser(**config.get("json", {}))`).
    pub json_parser: JSONParser,
    /// `self.csv_parser`.
    pub csv_parser: CSVParser,
    /// `self.xml_parser`.
    pub xml_parser: XMLParser,
}

impl Default for StructuredDataParser {
    fn default() -> Self {
        StructuredDataParser::new(None, Options::new())
    }
}

impl StructuredDataParser {
    /// `StructuredDataParser(config=None, **kwargs)`.
    pub fn new(config: Option<Options>, kwargs: Options) -> Self {
        let config = merge_config(config, kwargs);
        StructuredDataParser {
            logger: get_logger("structured_data_parser"),
            progress_tracker: get_progress_tracker(),
            json_parser: JSONParser::new(sub_config(&config, "json")),
            csv_parser: CSVParser::new(sub_config(&config, "csv")),
            xml_parser: XMLParser::new(sub_config(&config, "xml")),
            config,
        }
    }

    /// `parse_data(data, data_format=None, **options)`: parse JSON/CSV/XML/YAML from a file
    /// or a content string; the format is detected from the extension or content when not
    /// given.
    pub fn parse_data(
        &self,
        data: impl Into<Source>,
        data_format: Option<&str>,
        options: &Options,
    ) -> PResult<Map<String, Value>> {
        let data: Source = data.into();
        let file_path = data.file_path()?;
        let message = format!(
            "Structured data: {}",
            file_path
                .as_deref()
                .map(py_name)
                .unwrap_or_else(|| "content".to_string())
        );
        let file_text = file_path.as_deref().map(py_path_str);
        let tracking_id = self.progress_tracker.start_tracking(
            file_text.as_deref(),
            Some("parse"),
            Some("StructuredDataParser"),
            &message,
            None,
        );
        let mut format_used = String::new();
        let result = (|| -> PResult<Map<String, Value>> {
            let data_format = match data_format {
                Some(f) => f.to_string(),
                None => self.detect_format(&data)?,
            };
            format_used = data_format.clone();
            self.progress_tracker.update_tracking(
                &tracking_id,
                None,
                &format!("Parsing {data_format}..."),
            );
            match data_format.as_str() {
                "json" => as_dict(&self.json_parser.parse(data.clone(), options)?),
                "csv" => {
                    let (delimiter, rest) = crate::common::bind_kwarg(options, "delimiter");
                    let path = match &data {
                        Source::Text(s) => std::path::PathBuf::from(s),
                        Source::Path(p) => p.clone(),
                    };
                    let csv = match delimiter {
                        None => self.csv_parser.parse(&path, Some(","), &rest)?,
                        Some(Value::Null) => self.csv_parser.parse(&path, None, &rest)?,
                        Some(Value::String(d)) => self.csv_parser.parse(&path, Some(&d), &rest)?,
                        Some(_) => {
                            return Err(type_error("\"delimiter\" must be a 1-character string"));
                        }
                    };
                    as_dict(&csv)
                }
                "xml" => as_dict(&self.xml_parser.parse(data.clone(), options)?),
                "yaml" => self.parse_yaml(&data),
                other => {
                    Err(ValidationError::new(format!("Unsupported data format: {other}")).into())
                }
            }
        })();
        match &result {
            Ok(_) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Parsed {format_used} successfully"),
                None,
            ),
            Err(e) => self.progress_tracker.stop_tracking(
                &tracking_id,
                Some("failed"),
                &e.to_string(),
                None,
            ),
        }
        result
    }

    /// `validate_data(data, schema=None)`: check `required` fields and `properties` types
    /// (`string`, `number`, `integer`, `boolean`, `array`, `object`) of a dict.
    pub fn validate_data(
        &self,
        data: &Value,
        schema: Option<&Value>,
    ) -> PResult<Map<String, Value>> {
        let mut result = Map::new();
        result.insert("valid".to_string(), Value::Bool(true));
        result.insert("errors".to_string(), Value::Array(Vec::new()));
        let Some(schema) = schema else {
            result.insert("valid".to_string(), Value::Bool(true));
            result.insert(
                "message".to_string(),
                Value::String("No schema provided, skipping validation".to_string()),
            );
            return Ok(result);
        };
        let Value::Object(data) = data else {
            return Ok(result);
        };
        let Value::Object(schema) = schema else {
            return Err(attribute_error(type_name(schema), "get"));
        };
        let mut errors: Vec<Value> = Vec::new();
        let mut valid = true;
        let required = schema
            .get("required")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        for field in py_iter(&required)? {
            let present = match &field {
                Value::String(s) => data.contains_key(s),
                _ => false,
            };
            if !present {
                valid = false;
                errors.push(Value::String(format!(
                    "Missing required field: {}",
                    py_str(&field)
                )));
            }
        }
        let properties = schema
            .get("properties")
            .cloned()
            .unwrap_or(Value::Object(Map::new()));
        let Value::Object(properties) = properties else {
            return Err(attribute_error(type_name(&properties), "get"));
        };
        for (field, value) in data {
            let Some(prop) = properties.get(field) else {
                continue;
            };
            let Value::Object(prop) = prop else {
                return Err(attribute_error(type_name(prop), "get"));
            };
            let expected = prop.get("type").cloned().unwrap_or(Value::Null);
            let actual = type_name(value);
            let expected_text = py_str(&expected);
            let ok = match expected.as_str() {
                Some("string") => actual == "str",
                Some("number") => actual == "int" || actual == "float",
                Some("integer") => actual == "int",
                Some("boolean") => actual == "bool",
                Some("array") => actual == "list",
                Some("object") => actual == "dict",
                // Unknown types map to the list `[expected_type]`, which never equals a
                // type name.
                _ => false,
            };
            if !ok {
                valid = false;
                errors.push(Value::String(format!(
                    "Field {field}: expected {expected_text}, got {actual}"
                )));
            }
        }
        result.insert("valid".to_string(), Value::Bool(valid));
        result.insert("errors".to_string(), Value::Array(errors));
        Ok(result)
    }

    /// `convert_data(data, from_format, to_format)`: parse `data` (when it is a path or a
    /// string) and render it as `json` (indented), `csv`, `xml` (pretty-printed) or `yaml`.
    pub fn convert_data(
        &self,
        data: impl Into<StructuredInput>,
        from_format: &str,
        to_format: &str,
    ) -> PResult<String> {
        let mut from_xml = false;
        let parsed = match data.into() {
            StructuredInput::Source(source) => {
                let parsed = self.parse_data(source, Some(from_format), &Options::new())?;
                from_xml = parsed.contains_key("root");
                match parsed.get("data") {
                    Some(d) => d.clone(),
                    None => Value::Object(parsed),
                }
            }
            StructuredInput::Data(d) => d,
        };
        match to_format {
            "json" => {
                if from_xml {
                    return Err(type_error(
                        "Object of type XMLElement is not JSON serializable",
                    ));
                }
                Ok(sem_utils::pycompat::json::dumps(
                    &parsed,
                    &sem_utils::pycompat::json::DumpsOptions {
                        indent: Some(2),
                        ..Default::default()
                    },
                ))
            }
            "csv" => convert_to_csv(&parsed),
            "xml" => convert_to_xml(&parsed, "root"),
            "yaml" => Ok(sem_utils::pycompat::yaml::dump_default(&parsed)),
            other => {
                Err(ValidationError::new(format!("Unsupported target format: {other}")).into())
            }
        }
    }

    /// `extract_schema(data)`: `{"type": ..., "properties": {...}}` (plus `items` for a
    /// non-empty list).
    pub fn extract_schema(&self, data: &Value) -> Map<String, Value> {
        let mut schema = Map::new();
        let kind = match data {
            Value::Object(_) => "object".to_string(),
            Value::Array(_) => "array".to_string(),
            other => type_name(other).to_string(),
        };
        schema.insert("type".to_string(), Value::String(kind));
        let mut properties = Map::new();
        if let Value::Object(map) = data {
            for (k, v) in map {
                properties.insert(k.clone(), Value::Object(infer_type(v)));
            }
        }
        schema.insert("properties".to_string(), Value::Object(properties));
        if let Value::Array(items) = data
            && let Some(first) = items.first()
        {
            schema.insert("items".to_string(), Value::Object(infer_type(first)));
        }
        schema
    }

    /// `_detect_format(data)`.
    fn detect_format(&self, data: &Source) -> PResult<String> {
        if let Some(path) = data.file_path()? {
            let suffix = py_suffix(&path).to_lowercase();
            let format = match suffix.as_str() {
                ".json" => Some("json"),
                ".csv" => Some("csv"),
                ".xml" => Some("xml"),
                ".yaml" | ".yml" => Some("yaml"),
                _ => None,
            };
            if let Some(f) = format {
                return Ok(f.to_string());
            }
        }
        if let Source::Text(text) = data {
            let s = text.trim();
            if s.starts_with('{') || s.starts_with('[') {
                return Ok("json".to_string());
            } else if s.starts_with('<') {
                return Ok("xml".to_string());
            } else if s.contains(',') && s.contains('\n') {
                return Ok("csv".to_string());
            }
        }
        Ok("unknown".to_string())
    }

    /// `_parse_yaml(data)`.
    fn parse_yaml(&self, data: &Source) -> PResult<Map<String, Value>> {
        let yaml_data = match data.file_path()? {
            Some(path) => {
                let text = py_read_text(&path, "utf-8", DecodeErrors::Strict)?;
                yaml_safe_load(&text)?
            }
            None => yaml_safe_load(data.text().unwrap_or_default())?,
        };
        let kind = type_name(&yaml_data).to_string();
        let mut ordered = Map::new();
        ordered.insert("data".to_string(), yaml_data);
        ordered.insert("type".to_string(), Value::String(kind));
        ordered.insert(
            "metadata".to_string(),
            Value::Object(Map::from_iter([(
                "format".to_string(),
                Value::String("yaml".to_string()),
            )])),
        );
        Ok(ordered)
    }
}

/// `dataclass.__dict__` of a parser result.
fn as_dict<T: serde::Serialize>(value: &T) -> PResult<Map<String, Value>> {
    match serde_json::to_value(value) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(other) => Ok(Map::from_iter([("value".to_string(), other)])),
        Err(e) => Err(SemanticaError::Runtime(e.to_string())),
    }
}

/// Iterate a value the way a `for` loop over it would (lists, dict keys, string chars).
fn py_iter(value: &Value) -> PResult<Vec<Value>> {
    match value {
        Value::Array(items) => Ok(items.clone()),
        Value::Object(map) => Ok(map.keys().map(|k| Value::String(k.clone())).collect()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string())).collect()),
        other => Err(type_error(format!(
            "'{}' object is not iterable",
            type_name(other)
        ))),
    }
}

/// `_infer_type(value)`.
fn infer_type(value: &Value) -> Map<String, Value> {
    let json_type = match value {
        Value::String(_) => "string",
        Value::Number(n) if n.is_f64() => "number",
        Value::Number(_) => "integer",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "string",
    };
    let mut result = Map::new();
    result.insert("type".to_string(), Value::String(json_type.to_string()));
    match value {
        Value::Array(items) if !items.is_empty() => {
            result.insert("items".to_string(), Value::Object(infer_type(&items[0])));
        }
        Value::Object(map) => {
            result.insert(
                "properties".to_string(),
                Value::Object(
                    map.iter()
                        .map(|(k, v)| (k.clone(), Value::Object(infer_type(v))))
                        .collect(),
                ),
            );
        }
        _ => {}
    }
    result
}

/// `_convert_to_csv(data)`.
fn convert_to_csv(data: &Value) -> PResult<String> {
    let dialect = Dialect::default();
    match data {
        Value::Object(map) => {
            let fieldnames: Vec<String> = map.keys().cloned().collect();
            let mut out = dict_writer_header(&fieldnames, &dialect);
            out.push_str(&dict_writer_row(data, &fieldnames, &dialect)?);
            Ok(out)
        }
        Value::Array(items) => {
            if items.is_empty() {
                return Ok(String::new());
            }
            let mut out = String::new();
            if let Value::Object(first) = &items[0] {
                let fieldnames: Vec<String> = first.keys().cloned().collect();
                out.push_str(&dict_writer_header(&fieldnames, &dialect));
                for row in items {
                    out.push_str(&dict_writer_row(row, &fieldnames, &dialect)?);
                }
            } else {
                for row in items {
                    out.push_str(&write_value_row(row, &dialect)?);
                }
            }
            Ok(out)
        }
        other => Ok(py_str(other)),
    }
}

fn dict_writer_header(fieldnames: &[String], dialect: &Dialect) -> String {
    let header: Vec<Value> = fieldnames
        .iter()
        .map(|f| Value::String(f.clone()))
        .collect();
    write_row(&header, dialect)
}

/// `DictWriter.writerow(rowdict)` with `extrasaction="raise"` and `restval=""`.
fn dict_writer_row(row: &Value, fieldnames: &[String], dialect: &Dialect) -> PResult<String> {
    let Value::Object(map) = row else {
        return Err(attribute_error(type_name(row), "keys"));
    };
    let wrong: Vec<String> = map
        .keys()
        .filter(|k| !fieldnames.contains(k))
        .map(|k| sem_utils::pycompat::fmt::str_repr(k))
        .collect();
    if !wrong.is_empty() {
        return Err(SemanticaError::Value(format!(
            "dict contains fields not in fieldnames: {}",
            wrong.join(", ")
        )));
    }
    let values: Vec<Value> = fieldnames
        .iter()
        .map(|k| {
            map.get(k)
                .cloned()
                .unwrap_or_else(|| Value::String(String::new()))
        })
        .collect();
    Ok(write_row(&values, dialect))
}

// ----------------------------------------------------------------- XML conversion

/// A tiny ElementTree element for `_convert_to_xml`.
#[derive(Debug, Clone)]
struct XElem {
    tag: String,
    text: Option<String>,
    children: Vec<XElem>,
}

impl XElem {
    fn new(tag: &str) -> Self {
        XElem {
            tag: tag.to_string(),
            text: None,
            children: Vec::new(),
        }
    }
}

fn dict_to_xml(map: &Map<String, Value>, parent: &mut XElem) {
    for (key, value) in map {
        let mut elem = XElem::new(key);
        match value {
            Value::Object(m) => dict_to_xml(m, &mut elem),
            Value::Array(items) => {
                for item in items {
                    if let Value::Object(m) = item {
                        dict_to_xml(m, &mut elem);
                    } else {
                        let mut child = XElem::new("item");
                        child.text = Some(py_str(item));
                        elem.children.push(child);
                    }
                }
            }
            other => elem.text = Some(py_str(other)),
        }
        parent.children.push(elem);
    }
}

/// `_convert_to_xml(data, root_tag="root")`: build an ElementTree, serialise it
/// (`tostring`), re-parse it with minidom (expat, namespace-aware) and pretty-print with
/// two-space indentation. Keys that are not XML names fail the re-parse with expat's
/// `ExpatError` (message and position as expat reports them).
fn convert_to_xml(data: &Value, root_tag: &str) -> PResult<String> {
    let mut root = XElem::new(root_tag);
    match data {
        Value::Object(m) => dict_to_xml(m, &mut root),
        Value::Array(items) => {
            for item in items {
                if let Value::Object(m) = item {
                    dict_to_xml(m, &mut root);
                } else {
                    let mut elem = XElem::new("item");
                    elem.text = Some(py_str(item));
                    root.children.push(elem);
                }
            }
        }
        other => root.text = Some(py_str(other)),
    }
    let mut rough = String::new();
    serialize_et(&root, &mut rough);
    let document = expat_dom::parse(&rough).map_err(|e| expat_error(&e))?;
    let mut out = String::from("<?xml version=\"1.0\" ?>\n");
    expat_dom::write_pretty(&document, "", &mut out);
    Ok(out)
}

fn expat_error(message: &str) -> SemanticaError {
    SemanticaError::Other {
        module: "xml.parsers.expat".to_string(),
        type_name: "ExpatError".to_string(),
        message: message.to_string(),
    }
}

/// `ElementTree._escape_cdata`.
fn escape_cdata(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `ElementTree.tostring(elem, encoding="unicode")` (short empty elements).
fn serialize_et(elem: &XElem, out: &mut String) {
    out.push('<');
    out.push_str(&elem.tag);
    let text = elem.text.as_deref().filter(|t| !t.is_empty());
    if text.is_some() || !elem.children.is_empty() {
        out.push('>');
        if let Some(text) = text {
            out.push_str(&escape_cdata(text));
        }
        for child in &elem.children {
            serialize_et(child, out);
        }
        out.push_str("</");
        out.push_str(&elem.tag);
        out.push('>');
    } else {
        out.push_str(" />");
    }
}

/// An expat-like (namespace-aware) parser for the documents `_convert_to_xml` produces, and
/// minidom's `toprettyxml` writer.
mod expat_dom {
    /// A DOM node.
    #[derive(Debug, Clone)]
    pub(super) enum Node {
        Element {
            tag: String,
            attrs: Vec<(String, String)>,
            children: Vec<Node>,
        },
        Text(String),
    }

    fn is_name_start(c: char) -> bool {
        matches!(c, 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}'
            | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
            | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
            | '\u{10000}'..='\u{EFFFF}')
    }

    fn is_name_char(c: char) -> bool {
        is_name_start(c)
            || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
    }

    fn is_xml_char(c: char) -> bool {
        matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
    }

    fn is_ws(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\n' | '\r')
    }

    struct Parser {
        chars: Vec<char>,
        pos: usize,
    }

    impl Parser {
        fn peek(&self) -> Option<char> {
            self.chars.get(self.pos).copied()
        }

        /// `line L, column C` of a character index (columns count characters from 0).
        fn location(&self, index: usize) -> String {
            let mut line = 1;
            let mut col = 0;
            let mut i = 0;
            while i < index.min(self.chars.len()) {
                let c = self.chars[i];
                if c == '\n' || (c == '\r' && self.chars.get(i + 1) != Some(&'\n')) {
                    line += 1;
                    col = 0;
                } else if c != '\r' {
                    col += 1;
                }
                i += 1;
            }
            format!("line {line}, column {col}")
        }

        fn error(&self, what: &str, index: usize) -> String {
            format!("{what}: {}", self.location(index))
        }

        fn invalid(&self, index: usize) -> String {
            self.error("not well-formed (invalid token)", index)
        }

        /// A QName: NCName (':' NCName)?; returns the name and the index after it.
        fn qname(&mut self) -> Result<String, String> {
            let start = self.pos;
            match self.peek() {
                Some(c) if is_name_start(c) => self.pos += 1,
                _ => return Err(self.invalid(self.pos)),
            }
            let mut colon = false;
            loop {
                match self.peek() {
                    Some(':') if !colon => {
                        colon = true;
                        self.pos += 1;
                        match self.peek() {
                            Some(c) if is_name_start(c) => self.pos += 1,
                            _ => return Err(self.invalid(self.pos)),
                        }
                    }
                    Some(':') => return Err(self.invalid(self.pos)),
                    Some(c) if is_name_char(c) => self.pos += 1,
                    _ => break,
                }
            }
            Ok(self.chars[start..self.pos].iter().collect())
        }

        fn skip_ws(&mut self) {
            while self.peek().is_some_and(is_ws) {
                self.pos += 1;
            }
        }

        /// Text up to the next `<` with entity references resolved.
        fn text(&mut self) -> Result<String, String> {
            let mut out = String::new();
            while let Some(c) = self.peek() {
                if c == '<' {
                    break;
                }
                if !is_xml_char(c) {
                    return Err(self.invalid(self.pos));
                }
                if c == '&' {
                    let start = self.pos;
                    let end = self.chars[start..]
                        .iter()
                        .position(|&ch| ch == ';')
                        .map(|p| start + p);
                    let Some(end) = end else {
                        return Err(self.invalid(start));
                    };
                    let name: String = self.chars[start + 1..end].iter().collect();
                    let value = match name.as_str() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" => '\'',
                        n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16)
                            .ok()
                            .and_then(char::from_u32)
                            .ok_or_else(|| {
                                self.error("reference to invalid character number", start)
                            })?,
                        n if n.starts_with('#') => n[1..]
                            .parse::<u32>()
                            .ok()
                            .and_then(char::from_u32)
                            .ok_or_else(|| {
                                self.error("reference to invalid character number", start)
                            })?,
                        _ => return Err(self.error("undefined entity", start)),
                    };
                    out.push(value);
                    self.pos = end + 1;
                    continue;
                }
                if c == '\r' {
                    // Line ends are normalised to "\n".
                    out.push('\n');
                    self.pos += 1;
                    if self.peek() == Some('\n') {
                        self.pos += 1;
                    }
                    continue;
                }
                out.push(c);
                self.pos += 1;
            }
            Ok(out)
        }

        fn attr_value(&mut self) -> Result<String, String> {
            let quote = match self.peek() {
                Some(q @ ('"' | '\'')) => q,
                _ => return Err(self.invalid(self.pos)),
            };
            self.pos += 1;
            let mut out = String::new();
            loop {
                match self.peek() {
                    None => return Err(self.error("unclosed token", self.pos)),
                    Some(c) if c == quote => {
                        self.pos += 1;
                        break;
                    }
                    Some('<') => return Err(self.invalid(self.pos)),
                    Some(c) if !is_xml_char(c) => return Err(self.invalid(self.pos)),
                    Some(c) => {
                        out.push(if is_ws(c) { ' ' } else { c });
                        self.pos += 1;
                    }
                }
            }
            Ok(out)
        }

        /// One element starting at `<` (already checked), with its content.
        fn element(&mut self, scopes: &mut Vec<Vec<(String, String)>>) -> Result<Node, String> {
            let tag_start = self.pos;
            self.pos += 1;
            let tag = self.qname()?;
            let mut attrs: Vec<(String, String)> = Vec::new();
            let empty;
            loop {
                let had_ws = self.peek().is_some_and(is_ws);
                self.skip_ws();
                match self.peek() {
                    Some('>') => {
                        self.pos += 1;
                        empty = false;
                        break;
                    }
                    Some('/') => {
                        self.pos += 1;
                        if self.peek() == Some('>') {
                            self.pos += 1;
                            empty = true;
                            break;
                        }
                        return Err(self.invalid(self.pos));
                    }
                    Some(_) if had_ws => {
                        let name = self.qname()?;
                        self.skip_ws();
                        if self.peek() != Some('=') {
                            return Err(self.invalid(self.pos));
                        }
                        self.pos += 1;
                        self.skip_ws();
                        let value = self.attr_value()?;
                        if attrs.iter().any(|(n, _)| *n == name) {
                            return Err(self.error("duplicate attribute", tag_start));
                        }
                        attrs.push((name, value));
                    }
                    None => return Err(self.error("unclosed token", tag_start)),
                    Some(_) => return Err(self.invalid(self.pos)),
                }
            }
            // Namespace declarations and prefix binding.
            let mut scope = Vec::new();
            for (name, value) in &attrs {
                if name == "xmlns" {
                    scope.push((String::new(), value.clone()));
                } else if let Some(p) = name.strip_prefix("xmlns:") {
                    scope.push((p.to_string(), value.clone()));
                }
            }
            scopes.push(scope);
            let bound = |prefix: &str, scopes: &Vec<Vec<(String, String)>>| {
                prefix == "xml" || scopes.iter().any(|s| s.iter().any(|(p, _)| p == prefix))
            };
            if let Some((prefix, _)) = tag.split_once(':')
                && !bound(prefix, scopes)
            {
                return Err(self.error("unbound prefix", tag_start));
            }
            for (name, _) in &attrs {
                if let Some((prefix, _)) = name.split_once(':')
                    && prefix != "xmlns"
                    && !bound(prefix, scopes)
                {
                    return Err(self.error("unbound prefix", tag_start));
                }
            }
            let mut children = Vec::new();
            if !empty {
                loop {
                    let text = self.text()?;
                    if !text.is_empty() {
                        match children.last_mut() {
                            Some(Node::Text(prev)) => prev.push_str(&text),
                            _ => children.push(Node::Text(text)),
                        }
                    }
                    match self.peek() {
                        None => return Err(self.error("no element found", self.chars.len())),
                        Some('<') => {
                            if self.chars.get(self.pos + 1) == Some(&'/') {
                                let end_start = self.pos;
                                self.pos += 2;
                                let name = self.qname()?;
                                self.skip_ws();
                                if self.peek() != Some('>') {
                                    return Err(self.invalid(self.pos));
                                }
                                self.pos += 1;
                                if name != tag {
                                    return Err(self.error("mismatched tag", end_start));
                                }
                                break;
                            }
                            children.push(self.element(scopes)?);
                        }
                        Some(_) => unreachable!("text() stops at '<' or the end"),
                    }
                }
            }
            scopes.pop();
            Ok(Node::Element {
                tag,
                attrs,
                children,
            })
        }
    }

    /// Parse a serialised document into its root element.
    pub(super) fn parse(text: &str) -> Result<Node, String> {
        let mut p = Parser {
            chars: text.chars().collect(),
            pos: 0,
        };
        p.skip_ws();
        if p.peek() != Some('<') {
            return Err(p.error(
                if p.peek().is_none() {
                    "no element found"
                } else {
                    "syntax error"
                },
                p.pos,
            ));
        }
        let mut scopes = Vec::new();
        let root = p.element(&mut scopes)?;
        p.skip_ws();
        if p.pos < p.chars.len() {
            return Err(p.error("junk after document element", p.pos));
        }
        Ok(root)
    }

    /// minidom's `_write_data` escaping.
    fn write_data(out: &mut String, data: &str) {
        out.push_str(
            &data
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('"', "&quot;")
                .replace('>', "&gt;"),
        );
    }

    /// minidom `Node.writexml(writer, indent, addindent="  ", newl="\n")`.
    pub(super) fn write_pretty(node: &Node, indent: &str, out: &mut String) {
        match node {
            Node::Text(data) => {
                write_data(out, &format!("{indent}{data}\n"));
            }
            Node::Element {
                tag,
                attrs,
                children,
            } => {
                out.push_str(indent);
                out.push('<');
                out.push_str(tag);
                for (name, value) in attrs {
                    out.push_str(&format!(" {name}=\""));
                    write_data(out, value);
                    out.push('"');
                }
                if children.is_empty() {
                    out.push_str("/>\n");
                    return;
                }
                out.push('>');
                if let [Node::Text(data)] = children.as_slice() {
                    write_data(out, data);
                } else {
                    out.push('\n');
                    let inner = format!("{indent}  ");
                    for child in children {
                        write_pretty(child, &inner, out);
                    }
                    out.push_str(indent);
                }
                out.push_str(&format!("</{tag}>\n"));
            }
        }
    }
}

// ------------------------------------------------------------------------------ YAML

fn yaml_error(message: String) -> SemanticaError {
    SemanticaError::Other {
        module: "yaml".to_string(),
        type_name: "YAMLError".to_string(),
        message,
    }
}

/// `yaml.safe_load(text)` (serde_yaml with merge keys applied; see `PORT-NOTES.md` for the
/// YAML 1.1 scalar forms PyYAML resolves differently).
pub(crate) fn yaml_safe_load(text: &str) -> PResult<Value> {
    let mut value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|e| yaml_error(e.to_string()))?;
    value.apply_merge().map_err(|e| yaml_error(e.to_string()))?;
    yaml_to_json(value)
}

fn yaml_key(key: serde_yaml::Value) -> PResult<String> {
    Ok(match key {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Null => "null".to_string(),
        serde_yaml::Value::Bool(b) => if b { "true" } else { "false" }.to_string(),
        serde_yaml::Value::Number(n) => py_str(&yaml_to_json(serde_yaml::Value::Number(n))?),
        other => {
            return Err(yaml_error(format!(
                "while constructing a mapping\nfound unhashable key: {other:?}"
            )));
        }
    })
}

fn yaml_to_json(value: serde_yaml::Value) -> PResult<Value> {
    Ok(match value {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
        }
        serde_yaml::Value::String(s) => Value::String(s),
        serde_yaml::Value::Sequence(items) => Value::Array(
            items
                .into_iter()
                .map(yaml_to_json)
                .collect::<PResult<Vec<_>>>()?,
        ),
        serde_yaml::Value::Mapping(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(yaml_key(k)?, yaml_to_json(v)?);
            }
            Value::Object(out)
        }
        serde_yaml::Value::Tagged(tagged) => {
            return Err(yaml_error(format!(
                "could not determine a constructor for the tag '{}'",
                tagged.tag
            )));
        }
    })
}
