//! Characterisation tests for `methods.rs` against the original (fixtures/core). The
//! registry and `parse_config` are process-wide, so the sequence runs in one test.

mod common;

use std::sync::{Arc, Mutex};

use common::{files_dir, load_oracle, oracle_form, quiet_progress, replace_in_strings};
use sem_parse::common::py_path_str;
use sem_parse::methods::{self as m, ParseOutput};
use sem_parse::registry::{MethodArg, parse_method};
use sem_parse::{Options, method_registry, parse_config};
use serde_json::{Map, Value, json};

fn f(name: &str) -> String {
    files_dir("core").join(name).display().to_string()
}

fn arg_str(a: &MethodArg) -> String {
    match a {
        MethodArg::None => "None".to_string(),
        MethodArg::Str(s) => s.clone(),
        MethodArg::Path(p) => py_path_str(p),
        MethodArg::Bytes(b) => format!("{b:?}"),
    }
}

#[test]
fn test_convenience_functions_match_original() {
    quiet_progress();
    let oracle = load_oracle("core");
    let exp = &oracle["methods"];
    let none = Options::new();
    let check = |key: &str, actual: Value| assert_eq!(&actual, &exp[key], "case methods.{key}");

    check(
        "list_available",
        serde_json::to_value(m::list_available_methods(None)).unwrap(),
    );
    check(
        "parse_document_txt",
        oracle_form("core", m::parse_document(f("notes.txt"), None, None, &none)),
    );
    check(
        "parse_document_missing",
        oracle_form(
            "core",
            m::parse_document(f("missing.txt"), None, None, &none),
        ),
    );
    check(
        "parse_structured_json",
        oracle_form(
            "core",
            m::parse_structured_data(f("data.json"), None, None, &none),
        ),
    );
    check(
        "parse_structured_bad",
        oracle_form(
            "core",
            m::parse_structured_data(f("unknown.xyz"), None, None, &none),
        ),
    );
    check(
        "parse_json_via_structured",
        oracle_form("core", m::parse_json(f("data.json"), None, &none)),
    );
    check(
        "parse_json_string",
        oracle_form("core", m::parse_json("{\"a\": 1}", None, &none)),
    );
    check(
        "parse_csv_fallback",
        oracle_form("core", m::parse_csv(f("companies.csv"), None, None, &none)),
    );
    check(
        "parse_csv_semicolon",
        oracle_form(
            "core",
            m::parse_csv(f("semicolon.csv"), Some(";"), None, &none),
        ),
    );
    check(
        "parse_pdf_txt",
        oracle_form("core", m::parse_pdf(f("notes.txt"), None, &none)),
    );
    check(
        "parse_docx_txt",
        oracle_form("core", m::parse_docx(f("notes.txt"), None, &none)),
    );
    check(
        "get_parse_method_default",
        json!(m::get_parse_method("document", "default").is_some()),
    );
    check(
        "get_parse_method_missing",
        json!(m::get_parse_method("document", "zzz").is_none()),
    );

    // Custom methods: recorded calls, success and fallback.
    let calls: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let rec = calls.clone();
    let custom_ok = parse_method(move |args: &[MethodArg], kwargs: &Options| {
        rec.lock().unwrap().push(json!({
            "args": args.iter().map(arg_str).collect::<Vec<_>>(),
            "kwargs": Value::Object(kwargs.clone()),
        }));
        Ok(ParseOutput::Value(
            json!({"custom": true, "nargs": args.len()}),
        ))
    });
    let custom_fail = parse_method(|_a: &[MethodArg], _k: &Options| {
        Err(sem_utils::SemanticaError::Value("boom".to_string()))
    });
    method_registry.register("document", "custom_ok", custom_ok.clone());
    method_registry.register("document", "custom_fail", custom_fail);
    method_registry.register("structured", "custom_ok", custom_ok);
    let mut extra = Map::new();
    extra.insert("extra".into(), json!(1));
    check(
        "custom_ok_document",
        oracle_form(
            "core",
            m::parse_document(f("notes.txt"), None, Some("custom_ok"), &extra),
        ),
    );
    check(
        "custom_fail_document",
        oracle_form(
            "core",
            m::parse_document(f("notes.txt"), None, Some("custom_fail"), &none),
        ),
    );
    check(
        "custom_ok_pdf",
        oracle_form(
            "core",
            m::parse_pdf(f("notes.txt"), Some("custom_ok"), &none),
        ),
    );
    check(
        "custom_ok_csv",
        oracle_form(
            "core",
            m::parse_csv(f("companies.csv"), None, Some("custom_ok"), &none),
        ),
    );
    let files = files_dir("core").display().to_string();
    let recorded = replace_in_strings(
        Value::Array(calls.lock().unwrap().clone()),
        &files,
        "$FILES",
    );
    check("custom_calls", recorded);
    check(
        "unknown_method",
        oracle_form(
            "core",
            m::parse_document(f("notes.txt"), None, Some("nope"), &none),
        ),
    );
    method_registry.unregister("document", "custom_ok");
    method_registry.unregister("document", "custom_fail");
    method_registry.unregister("structured", "custom_ok");

    // `get_method_config` hands out the stored dict, which the convenience functions update.
    let mut seen = Options::new();
    seen.insert("seen".into(), json!(1));
    parse_config.set_method_config("structured", seen);
    let mut kw = Options::new();
    kw.insert("extra_kw".into(), json!(2));
    let _ = m::parse_structured_data(f("data.json"), None, None, &kw);
    check(
        "method_config_after",
        Value::Object(parse_config.get_method_config("structured")),
    );
}
