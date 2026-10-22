//! Characterisation tests for `csv_parser.rs` against the original (fixtures/core).

mod common;

use common::{assert_case, files_dir, load_oracle, oracle_form, quiet_progress};
use once_cell::sync::Lazy;
use sem_parse::{CSVParser, Options};
use serde_json::{Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| load_oracle("core"));

fn f(name: &str) -> std::path::PathBuf {
    files_dir("core").join(name)
}

fn opts(v: Value) -> Options {
    v.as_object().cloned().unwrap_or_default()
}

fn check(key: &str, actual: Value) {
    assert_case(&ORACLE, "csv", key, actual);
}

#[test]
fn test_parse_variants() {
    quiet_progress();
    let p = CSVParser::default();
    let comma = Some(",");
    check(
        "basic",
        oracle_form("core", p.parse(f("companies.csv"), comma, &Options::new())),
    );
    check(
        "no_header",
        oracle_form(
            "core",
            p.parse(
                f("companies.csv"),
                comma,
                &opts(json!({"has_header": false})),
            ),
        ),
    );
    check(
        "skip_max",
        oracle_form(
            "core",
            p.parse(
                f("companies.csv"),
                comma,
                &opts(json!({"skip_rows": 1, "max_rows": 1})),
            ),
        ),
    );
    check(
        "no_header_skip",
        oracle_form(
            "core",
            p.parse(
                f("companies.csv"),
                comma,
                &opts(json!({"has_header": false, "skip_rows": 1})),
            ),
        ),
    );
    check(
        "max_zero",
        oracle_form(
            "core",
            p.parse(f("companies.csv"), comma, &opts(json!({"max_rows": 0}))),
        ),
    );
    check(
        "max_negative",
        oracle_form(
            "core",
            p.parse(f("companies.csv"), comma, &opts(json!({"max_rows": -1}))),
        ),
    );
    check(
        "semicolon",
        oracle_form(
            "core",
            p.parse(f("semicolon.csv"), Some(";"), &Options::new()),
        ),
    );
    check(
        "ragged",
        oracle_form("core", p.parse(f("ragged.csv"), comma, &Options::new())),
    );
    check(
        "ragged_no_header",
        oracle_form(
            "core",
            p.parse(f("ragged.csv"), comma, &opts(json!({"has_header": false}))),
        ),
    );
    check(
        "quoted",
        oracle_form("core", p.parse(f("quoted.csv"), comma, &Options::new())),
    );
    check(
        "empty",
        oracle_form("core", p.parse(f("empty.csv"), comma, &Options::new())),
    );
    check(
        "empty_no_header",
        oracle_form(
            "core",
            p.parse(f("empty.csv"), comma, &opts(json!({"has_header": false}))),
        ),
    );
    check(
        "crlf",
        oracle_form("core", p.parse(f("crlf.csv"), comma, &Options::new())),
    );
    check(
        "latin1",
        oracle_form(
            "core",
            p.parse(
                f("latin1.csv"),
                comma,
                &opts(json!({"encoding": "latin-1"})),
            ),
        ),
    );
    check(
        "mixedbytes",
        oracle_form("core", p.parse(f("mixedbytes.csv"), comma, &Options::new())),
    );
    check(
        "has_header_zero",
        oracle_form(
            "core",
            p.parse(f("companies.csv"), comma, &opts(json!({"has_header": 0}))),
        ),
    );
}

#[test]
fn test_sniffing() {
    quiet_progress();
    let p = CSVParser::default();
    check(
        "sniff",
        oracle_form("core", p.parse(f("semicolon.csv"), None, &Options::new())),
    );
    check(
        "sniff_fail",
        oracle_form("core", p.parse(f("ragged.csv"), None, &Options::new())),
    );
    check(
        "sniff_single",
        oracle_form("core", p.parse(f("single.csv"), None, &Options::new())),
    );
}

#[test]
fn test_errors() {
    quiet_progress();
    let p = CSVParser::default();
    check(
        "missing",
        oracle_form(
            "core",
            p.parse(f("missing.csv"), Some(","), &Options::new()),
        ),
    );
    check(
        "bad_delimiter",
        oracle_form(
            "core",
            p.parse(f("companies.csv"), Some(";;"), &Options::new()),
        ),
    );
    check(
        "unknown_encoding",
        oracle_form(
            "core",
            p.parse(
                f("companies.csv"),
                Some(","),
                &opts(json!({"encoding": "nope"})),
            ),
        ),
    );
}

#[test]
fn test_parse_to_dict_and_list() {
    quiet_progress();
    let p = CSVParser::default();
    check(
        "to_dict",
        oracle_form("core", p.parse_to_dict(f("ragged.csv"), &Options::new())),
    );
    check(
        "to_dict_delim",
        oracle_form(
            "core",
            p.parse_to_dict(f("semicolon.csv"), &opts(json!({"delimiter": ";"}))),
        ),
    );
    check(
        "to_list",
        oracle_form("core", p.parse_to_list(f("ragged.csv"), &Options::new())),
    );
    check(
        "to_list_no_header",
        oracle_form(
            "core",
            p.parse_to_list(f("ragged.csv"), &opts(json!({"has_header": false}))),
        ),
    );
}
