//! Characterisation of the Python-compatibility layer against the oracle: float `repr`,
//! fixed formatting, `str()`/`repr()`, `json.dumps`, `yaml.dump`, `strftime`, `strptime`.

mod common;

use chrono::{FixedOffset, NaiveDate, TimeZone};
use common::oracle;
use sem_utils::pycompat::fmt::{float_repr, format_fixed, py_repr, py_str};
use sem_utils::pycompat::json::{DumpsOptions, dumps};
use sem_utils::pycompat::time::{PyDateTime, strptime};
use sem_utils::pycompat::warnings::{USER_WARNING, catch_warnings, warn};
use sem_utils::pycompat::yaml::{DumpOptions, dump};
use serde_json::{Value, json};

#[test]
fn test_float_repr_matches_python() {
    for case in oracle("float_repr").as_array().unwrap() {
        let x = case[0].as_f64().unwrap();
        assert_eq!(float_repr(x), case[1].as_str().unwrap(), "repr({x})");
    }
    assert_eq!(float_repr(f64::NAN), "nan");
    assert_eq!(float_repr(f64::NEG_INFINITY), "-inf");
}

#[test]
fn test_format_fixed_matches_python() {
    for case in oracle("format_fixed").as_array().unwrap() {
        let x = case[0].as_f64().unwrap();
        let precision = case[1].as_u64().unwrap() as usize;
        assert_eq!(
            format_fixed(x, precision),
            case[2].as_str().unwrap(),
            "format({x}, '.{precision}f')"
        );
    }
}

#[test]
fn test_str_and_repr_match_python() {
    for case in oracle("py_repr").as_array().unwrap() {
        assert_eq!(
            py_str(&case[0]),
            case[1].as_str().unwrap(),
            "str({})",
            case[0]
        );
        assert_eq!(
            py_repr(&case[0]),
            case[2].as_str().unwrap(),
            "repr({})",
            case[0]
        );
    }
}

#[test]
fn test_json_dumps_matches_python() {
    for case in oracle("json_dumps").as_array().unwrap() {
        let options = &case["options"];
        let separators = options["separators"].as_array().map(|s| {
            (
                s[0].as_str().unwrap().to_string(),
                s[1].as_str().unwrap().to_string(),
            )
        });
        let opts = DumpsOptions {
            indent: options["indent"].as_u64().map(|i| i as usize),
            ensure_ascii: options["ensure_ascii"].as_bool().unwrap_or(true),
            sort_keys: options["sort_keys"].as_bool().unwrap_or(false),
            separators: separators.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
        };
        assert_eq!(
            dumps(&case["value"], &opts),
            case["expected"].as_str().unwrap(),
            "json.dumps({}, {options})",
            case["value"]
        );
    }
}

fn yaml_options(options: &Value) -> DumpOptions {
    let mut opts = DumpOptions::default();
    if let Some(style) = options.get("default_flow_style") {
        opts.default_flow_style = style.as_bool();
    }
    if let Some(sort) = options["sort_keys"].as_bool() {
        opts.sort_keys = sort;
    }
    if let Some(unicode) = options["allow_unicode"].as_bool() {
        opts.allow_unicode = unicode;
    }
    if let Some(width) = options["width"].as_u64() {
        opts.width = width as usize;
    }
    if let Some(indent) = options["indent"].as_u64() {
        opts.indent = indent as usize;
    }
    if let Some(start) = options["explicit_start"].as_bool() {
        opts.explicit_start = start;
    }
    opts
}

#[test]
fn test_yaml_dump_default_matches_pyyaml() {
    for case in oracle("yaml_dump_default").as_array().unwrap() {
        assert_eq!(
            dump(&case["value"], &DumpOptions::default()),
            case["expected"].as_str().unwrap(),
            "yaml.dump({})",
            case["value"]
        );
    }
}

#[test]
fn test_yaml_dump_options_match_pyyaml() {
    for case in oracle("yaml_dump_options").as_array().unwrap() {
        assert_eq!(
            dump(&case["value"], &yaml_options(&case["options"])),
            case["expected"].as_str().unwrap(),
            "yaml.dump({}, {})",
            case["value"],
            case["options"]
        );
    }
}

/// The datetime a fixture spec describes.
pub fn make_dt(spec: &Value) -> PyDateTime {
    let fields = |v: &Value| -> Vec<i64> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect()
    };
    if let Some(naive) = spec.get("naive") {
        let f = fields(naive);
        let dt = NaiveDate::from_ymd_opt(f[0] as i32, f[1] as u32, f[2] as u32)
            .unwrap()
            .and_hms_micro_opt(f[3] as u32, f[4] as u32, f[5] as u32, f[6] as u32)
            .unwrap();
        PyDateTime::Naive(dt)
    } else {
        let f = fields(&spec["aware"]);
        let naive = NaiveDate::from_ymd_opt(f[0] as i32, f[1] as u32, f[2] as u32)
            .unwrap()
            .and_hms_micro_opt(f[3] as u32, f[4] as u32, f[5] as u32, f[6] as u32)
            .unwrap();
        let offset = FixedOffset::east_opt(f[7] as i32).unwrap();
        PyDateTime::Aware(offset.from_local_datetime(&naive).unwrap())
    }
}

#[test]
fn test_strftime_matches_python() {
    for case in oracle("strftime").as_array().unwrap() {
        let dt = make_dt(&case["dt"]);
        let format = case["format"].as_str().unwrap();
        assert_eq!(
            dt.strftime(format),
            case["expected"].as_str().unwrap(),
            "{} strftime {format:?}",
            case["dt"]
        );
    }
}

#[test]
fn test_isoformat_and_str_match_python() {
    for case in oracle("isoformat").as_array().unwrap() {
        let dt = make_dt(&case["dt"]);
        assert_eq!(dt.to_string(), case["str"].as_str().unwrap());
        assert_eq!(dt.isoformat('T'), case["iso"].as_str().unwrap());
    }
}

/// `dt_out` of the fixture generator.
pub fn dt_json(dt: &PyDateTime) -> Value {
    use chrono::{Datelike, Timelike};
    let wall = dt.wall();
    json!({
        "wall": [wall.year(), wall.month(), wall.day(), wall.hour(), wall.minute(), wall.second(), wall.nanosecond() / 1000],
        "offset": dt.offset().map(|o| o.local_minus_utc()),
        "str": dt.to_string(),
    })
}

#[test]
fn test_strptime_matches_python() {
    for case in oracle("strptime").as_array().unwrap() {
        let data = case["data"].as_str().unwrap();
        let format = case["format"].as_str().unwrap();
        let got = match strptime(data, format) {
            Ok(dt) => json!({"ok": dt_json(&dt)}),
            Err(e) => common::err_json(&e),
        };
        let expected = common::expected_outcome(case);
        assert_eq!(got, expected, "strptime({data:?}, {format:?})");
    }
}

#[test]
fn test_warnings_record_category_and_location() {
    let ((), caught) = catch_warnings(|| warn("careful", USER_WARNING));
    assert_eq!(caught.len(), 1);
    assert_eq!(caught[0].message, "careful");
    assert_eq!(caught[0].category.name(), "UserWarning");
    assert!(caught[0].filename.ends_with("utils_pycompat.rs"));
}
