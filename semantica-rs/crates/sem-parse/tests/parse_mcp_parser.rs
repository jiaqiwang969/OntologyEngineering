//! Characterisation tests for `mcp_parser.rs` against the original (fixtures/core).

mod common;

use common::{assert_case, load_oracle, oracle_form, quiet_progress};
use once_cell::sync::Lazy;
use sem_parse::{MCPParser, Options};
use serde_json::{Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| load_oracle("core"));

fn check(key: &str, actual: Value) {
    assert_case(&ORACLE, "mcp", key, actual);
}

#[test]
fn test_resource_responses() {
    quiet_progress();
    let p = MCPParser::default();
    let none = Options::new();
    check(
        "resource_json",
        oracle_form(
            "core",
            p.parse_resource_response(
                &json!({"uri": "file:///x", "mimeType": "application/json",
            "contents": [{"type": "text", "text": " {\"a\": [1, 2]}"}], "metadata": {"k": 1}}),
                &none,
            ),
        ),
    );
    let multi = json!({"contents": [
        {"type": "text", "text": "name,age\nAnn,3\n\nBob,4,extra\nCy,5"},
        {"type": "text", "text": "a\tb\n1\t2"},
        {"type": "text", "text": "single line, with comma"},
        {"type": "text", "text": "[1, 2"},
        {"type": "image", "data": "aGVsbG8=", "mimeType": "image/png"},
        {"type": "image", "data": ""},
        {"type": "resource", "resource": {"uri": "r"}},
        {"type": "blob", "data": [1, 2]},
        {"text": "default type"},
        "not a dict",
        {"type": "text", "text": 5}
    ]});
    check(
        "resource_multi",
        oracle_form("core", p.parse_resource_response(&multi, &none)),
    );
    check(
        "resource_empty",
        oracle_form(
            "core",
            p.parse_resource_response(&json!({"contents": []}), &none),
        ),
    );
    check(
        "resource_none",
        oracle_form(
            "core",
            p.parse_resource_response(&json!({"contents": null, "uri": "u"}), &none),
        ),
    );
    check(
        "resource_not_dict",
        oracle_form("core", p.parse_resource_response(&json!([1, 2]), &none)),
    );
    check(
        "resource_int_contents",
        oracle_form(
            "core",
            p.parse_resource_response(&json!({"contents": 5}), &none),
        ),
    );
    check(
        "resource_str_contents",
        oracle_form(
            "core",
            p.parse_resource_response(&json!({"contents": "ab"}), &none),
        ),
    );
}

#[test]
fn test_tool_outputs_and_dispatch() {
    quiet_progress();
    let p = MCPParser::new(None, Options::new());
    let none = Options::new();
    check(
        "tool_output",
        oracle_form(
            "core",
            p.parse_tool_output(
                &json!({"content": [{"type": "text", "text": "{\"ok\": true}"}], "isError": true}),
                &none,
            ),
        ),
    );
    check(
        "tool_output_default",
        oracle_form("core", p.parse_tool_output(&json!({}), &none)),
    );
    check(
        "mcp_resource",
        oracle_form(
            "core",
            p.parse_mcp_data(
                &json!({"contents": [{"type": "text", "text": "hi"}]}),
                None,
                &none,
            ),
        ),
    );
    check(
        "mcp_tool",
        oracle_form(
            "core",
            p.parse_mcp_data(&json!({"content": []}), None, &none),
        ),
    );
    check(
        "mcp_other_dict",
        oracle_form("core", p.parse_mcp_data(&json!({"x": 1}), None, &none)),
    );
    check(
        "mcp_list",
        oracle_form("core", p.parse_mcp_data(&json!([1]), None, &none)),
    );
    check(
        "mcp_unknown_type",
        oracle_form(
            "core",
            p.parse_mcp_data(&json!({"x": 1}), Some("weird"), &none),
        ),
    );
    check(
        "mcp_explicit_resource",
        oracle_form(
            "core",
            p.parse_mcp_data(
                &json!({"contents": [{"text": "x"}]}),
                Some("resource"),
                &none,
            ),
        ),
    );
    check(
        "mcp_resource_error",
        oracle_form(
            "core",
            p.parse_mcp_data(&json!("str"), Some("resource"), &none),
        ),
    );
    assert_eq!(p.logger.name(), "semantica.mcp_parser");
    assert!(p.config.is_empty());
    assert!(p.document_parser.supported_formats.contains_key(".pdf"));
    assert!(p.structured_parser.config.is_empty());
}
