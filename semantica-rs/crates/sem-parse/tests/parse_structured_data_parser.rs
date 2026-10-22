//! Characterisation tests for `structured_data_parser.rs` against the original
//! (fixtures/core).

mod common;

use common::{assert_case, files_dir, load_oracle, oracle_form, quiet_progress};
use once_cell::sync::Lazy;
use sem_parse::structured_data_parser::StructuredInput;
use sem_parse::{Options, StructuredDataParser};
use serde_json::{Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| load_oracle("core"));

fn f(name: &str) -> String {
    files_dir("core").join(name).display().to_string()
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

fn check(key: &str, actual: Value) {
    assert_case(&ORACLE, "structured", key, actual);
}

#[test]
fn test_parse_data_formats() {
    quiet_progress();
    let p = StructuredDataParser::default();
    let none = Options::new();
    check(
        "json_auto",
        oracle_form("core", p.parse_data(f("data.json"), None, &none)),
    );
    check(
        "json_explicit",
        oracle_form("core", p.parse_data(f("data.json"), Some("json"), &none)),
    );
    check(
        "csv_auto",
        oracle_form("core", p.parse_data(f("companies.csv"), None, &none)),
    );
    check(
        "csv_delimiter",
        oracle_form(
            "core",
            p.parse_data(
                f("semicolon.csv"),
                Some("csv"),
                &opts(json!({"delimiter": ";"})),
            ),
        ),
    );
    check(
        "yaml_file",
        oracle_form("core", p.parse_data(f("data.yaml"), None, &none)),
    );
    check(
        "yaml_scalar",
        oracle_form("core", p.parse_data(f("scalar.yaml"), None, &none)),
    );
    check(
        "yaml_list",
        oracle_form("core", p.parse_data(f("list.yaml"), None, &none)),
    );
    check(
        "yaml_string",
        oracle_form(
            "core",
            p.parse_data("a: 1\nb: [x, 2]\n", Some("yaml"), &none),
        ),
    );
    check(
        "json_content",
        oracle_form("core", p.parse_data(" {\"k\": [1, 2]}", None, &none)),
    );
    check(
        "csv_content",
        oracle_form("core", p.parse_data("a,b\n1,2", None, &none)),
    );
    check(
        "unknown_content",
        oracle_form("core", p.parse_data("plain words", None, &none)),
    );
    check(
        "unknown_file",
        oracle_form("core", p.parse_data(f("unknown.xyz"), None, &none)),
    );
    check(
        "bad_format",
        oracle_form("core", p.parse_data(f("data.json"), Some(","), &none)),
    );
}

#[test]
fn test_validate_data() {
    let p = StructuredDataParser::default();
    check(
        "validate_none",
        oracle_form("core", p.validate_data(&json!({"a": 1}), None)),
    );
    check(
        "validate_required",
        oracle_form(
            "core",
            p.validate_data(&json!({"a": 1}), Some(&json!({"required": ["a", "b"]}))),
        ),
    );
    let data =
        json!({"s": "x", "n": 1.5, "i": true, "b": true, "l": [1], "o": {}, "z": null, "q": 1});
    let schema = json!({"properties": {"s": {"type": "string"}, "n": {"type": "number"}, "i": {"type": "integer"},
        "b": {"type": "boolean"}, "l": {"type": "array"}, "o": {"type": "object"}, "z": {"type": "null"}, "q": {}}});
    check(
        "validate_types",
        oracle_form("core", p.validate_data(&data, Some(&schema))),
    );
    check(
        "validate_number_int",
        oracle_form(
            "core",
            p.validate_data(
                &json!({"n": 3}),
                Some(&json!({"properties": {"n": {"type": "number"}}})),
            ),
        ),
    );
    check(
        "validate_non_dict",
        oracle_form(
            "core",
            p.validate_data(&json!([1, 2]), Some(&json!({"required": ["a"]}))),
        ),
    );
}

#[test]
fn test_convert_data() {
    quiet_progress();
    let p = StructuredDataParser::default();
    let conv = |data: StructuredInput, from: &str, to: &str| {
        oracle_form("core", p.convert_data(data, from, to))
    };
    let v = |j: Value| StructuredInput::Data(j);
    check(
        "convert_dict_json",
        conv(
            v(json!({"name": "Ann", "tags": ["a", "é"], "n": 1.0})),
            "json",
            "json",
        ),
    );
    check(
        "convert_dict_csv",
        conv(v(json!({"a": 1, "b": "x,y", "c": null})), "json", "csv"),
    );
    check(
        "convert_list_dicts_csv",
        conv(v(json!([{"a": 1, "b": 2}, {"a": 3}])), "json", "csv"),
    );
    check(
        "convert_list_extra_csv",
        conv(v(json!([{"a": 1}, {"a": 2, "z": 3}])), "json", "csv"),
    );
    check(
        "convert_list_lists_csv",
        conv(v(json!([[1, "two"], ["x\"y", null]])), "json", "csv"),
    );
    check(
        "convert_list_mixed_csv",
        conv(v(json!([[1], 5])), "json", "csv"),
    );
    check("convert_empty_csv", conv(v(json!([])), "json", "csv"));
    check("convert_scalar_csv", conv(v(json!(42)), "json", "csv"));
    check(
        "convert_dict_xml",
        conv(
            v(
                json!({"person": {"name": "A & B", "age": 3, "tags": ["x", {"k": "v"}], "empty": [], "none": null}}),
            ),
            "json",
            "xml",
        ),
    );
    check(
        "convert_list_xml",
        conv(v(json!([1, {"a": "<b>"}])), "json", "xml"),
    );
    check("convert_scalar_xml", conv(v(json!(3.5)), "json", "xml"));
    check(
        "convert_bad_tag_xml",
        conv(v(json!({"first name": 1})), "json", "xml"),
    );
    check(
        "convert_empty_dict_xml",
        conv(v(json!({"a": {}, "b": ""})), "json", "xml"),
    );
    check(
        "convert_dict_yaml",
        conv(v(json!({"b": [1, 2], "a": {"x": "y"}})), "json", "yaml"),
    );
    check(
        "convert_file_csv",
        conv(StructuredInput::from(f("array2.json")), "json", "csv"),
    );
    check(
        "convert_string_yaml",
        conv(
            StructuredInput::from("{\"z\": 1, \"a\": [true, null]}"),
            "json",
            "yaml",
        ),
    );
    check(
        "convert_csv_file_json",
        conv(StructuredInput::from(f("companies.csv")), "csv", "json"),
    );
    check(
        "convert_unsupported",
        conv(v(json!({"a": 1})), "json", "toml"),
    );
}

#[test]
fn test_extract_schema_and_detection() {
    let p = StructuredDataParser::default();
    let schema = |d: Value| {
        oracle_form::<_>(
            "core",
            Ok::<_, sem_utils::SemanticaError>(p.extract_schema(&d)),
        )
    };
    check(
        "schema_dict",
        schema(
            json!({"s": "x", "i": 1, "f": 1.5, "b": false, "n": null, "l": [{"a": 1}], "o": {"p": []}}),
        ),
    );
    check("schema_list", schema(json!([1, 2])));
    check("schema_empty_list", schema(json!([])));
    check("schema_scalar", schema(json!("x")));
    check("schema_none", schema(json!(null)));
    assert!(p.json_parser.progress_tracker.enabled());
    assert_eq!(p.logger.name(), "semantica.structured_data_parser");
}

#[test]
fn test_convert_to_xml_matches_expat_and_minidom() {
    let p = StructuredDataParser::default();
    let cases = ORACLE["xml_conversions"].as_array().expect("cases");
    assert!(cases.len() > 20);
    for case in cases {
        let input = case["input"].clone();
        let actual = oracle_form("core", p.convert_data(StructuredInput::Data(input.clone()), "json", "xml"));
        assert_eq!(actual, case["result"], "xml conversion of {input}");
    }
}
