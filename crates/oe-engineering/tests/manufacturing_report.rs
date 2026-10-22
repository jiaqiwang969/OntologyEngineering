use oe_engineering::manufacturing_report as report;
use serde_json::{Value, json};
fn oracle() -> Value {
    oe_core::json::strict_json(include_bytes!(
        "fixtures/manufacturing-report-legacy-v1.json"
    ))
    .unwrap()
}
fn sample() -> Value {
    oracle()["examples"][2]["input"].clone()
}
fn preamble() -> String {
    oracle()["preamble"].as_str().unwrap().to_owned()
}
#[test]
fn all_four_original_examples_match_complete_tex_and_render_map() {
    let data = oracle();
    for example in data["examples"].as_array().unwrap() {
        let actual = report::render(&example["input"], data["preamble"].as_str().unwrap()).unwrap();
        assert_eq!(
            actual.tex,
            example["tex"].as_str().unwrap(),
            "{}",
            example["name"]
        );
        assert_eq!(actual.map, example["map"], "{}", example["name"]);
    }
}
#[test]
fn no_future_sources_and_original_cost_changes() {
    let data = oracle();
    for (i, e) in data["examples"].as_array().unwrap().iter().enumerate() {
        report::validate(&e["input"]).unwrap();
        let ids = e["input"]["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            (1..=i + 1).map(|n| format!("S{n}")).collect::<Vec<_>>()
        );
    }
    let early = report::cost_result(Some(&data["examples"][2]["input"]["cost"]))
        .unwrap()
        .unwrap();
    let later = report::cost_result(Some(&data["examples"][3]["input"]["cost"]))
        .unwrap()
        .unwrap();
    assert_eq!(early["known_subtotal"], "2088.00");
    assert_eq!(early["omitted_ids"], json!(["C-QC", "C-PACK"]));
    assert_eq!(later["known_subtotal"], "2556.00");
    assert_eq!(later["omitted_ids"], json!([]));
}
#[test]
fn unknown_quantity_retains_setup_and_omits_unit_fees() {
    let mut s = sample();
    s["cost"]["quantity"] = Value::Null;
    report::validate(&s).unwrap();
    let r = report::cost_result(s.get("cost")).unwrap().unwrap();
    assert_eq!(r["known_subtotal"], "720.00");
    assert!(
        r["omitted_ids"]
            .as_array()
            .unwrap()
            .contains(&json!("C-MAT"))
    );
    assert!(
        report::render(&s, &preamble())
            .unwrap()
            .tex
            .contains("未记录")
    );
    s["cost"] = Value::Null;
    assert_eq!(report::cost_result(s.get("cost")).unwrap(), None);
    assert!(
        report::render(&s, &preamble())
            .unwrap()
            .tex
            .contains("尚未提供费用明细")
    );
}
#[test]
fn exact_decimal_matches_82_python_context_and_half_even_oracles() {
    for (i, v) in oracle()["decimal"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            report::cost_result(Some(&v["cost"])).unwrap().unwrap(),
            v["expected"],
            "oracle {i}"
        );
    }
}
#[test]
fn tex_escaping_and_whole_source_locators_match_original() {
    for v in oracle()["escaping"].as_array().unwrap() {
        let raw = v["input"].as_str().unwrap();
        assert_eq!(report::tex(raw), v["tex"]);
        assert_eq!(report::source_tex(raw), v["source_tex"]);
    }
    let s = r"A&B_2 5% {draft} \input{secret}";
    assert!(report::tex(s).contains(r"\textbackslash{}input\{secret\}"));
    let mut s = sample();
    let locator = format!(
        "https://example.org/{}?page=2979",
        "Abc_0123456789/".repeat(20)
    );
    s["sources"][0]["locator"] = json!(locator);
    let result = report::render(&s, &preamble()).unwrap();
    assert!(
        result.map["expected_text"]
            .as_array()
            .unwrap()
            .contains(&json!(locator))
    );
    assert!(result.tex.contains(r"\allowbreak{}"));
}
#[test]
fn dangling_source_and_cross_collection_ids_fail() {
    let mut s = sample();
    s["records"][0]["source_ids"] = json!(["MISSING"]);
    assert!(report::validate(&s).is_err());
    let mut s = sample();
    s["cost"]["lines"][0]["id"] = s["records"][0]["id"].clone();
    assert!(report::validate(&s).is_err());
    let mut s = sample();
    s["relations"][0]["to"] = json!("UNKNOWN");
    assert!(report::validate(&s).is_err());
}
#[test]
fn unknown_view_missing_trigger_or_unresolved_template_fail() {
    let mut s = sample();
    s["views"][0]["kind"] = json!("automatic_approval");
    assert!(report::validate(&s).is_err());
    let mut s = sample();
    s["views"][0]
        .as_object_mut()
        .unwrap()
        .remove("update_triggers");
    assert!(report::validate(&s).is_err());
    assert!(report::render(&sample(), "@@MISSING@@").is_err());
    assert!(report::render(&sample(), "@@MISSING").is_ok());
}
#[test]
fn renderer_cannot_claim_semantic_execution_or_engineering_approval() {
    let mut s = sample();
    s["semantic_status"] = json!("passed");
    assert!(report::validate(&s).is_err());
    let r = report::render(&sample(), &preamble()).unwrap();
    assert_eq!(r.map["semantic_status"], "not_executed_for_this_snapshot");
    assert!(r.tex.contains("所列小计不自动成为对外售价或已实现收益"));
}
#[test]
fn no_double_quantity_bool_float_or_nonfinite_amounts() {
    for (value, count) in [
        (json!("Infinity"), json!(1)),
        (json!(true), json!(1)),
        (json!(1.1), json!(1)),
        (json!("10"), json!(2)),
        (json!("NaN"), json!(1)),
    ] {
        let mut s = sample();
        s["cost"]["lines"][0]["value"] = value;
        s["cost"]["lines"][0]["count"] = count;
        assert!(report::validate(&s).is_err());
        assert!(report::cost_result(s.get("cost")).is_err());
    }
    for quantity in [json!(0), json!(-1), json!(true), json!(18.0)] {
        let mut s = sample();
        s["cost"]["quantity"] = quantity;
        assert!(report::validate(&s).is_err());
    }
}
#[test]
fn view_selection_is_explicit_and_sources_remain_bound() {
    let mut s = sample();
    s["views"] = json!([s["views"][0].clone()]);
    s["views"][0]["items"] = json!(["R-EQUIP"]);
    let r = report::render(&s, &preamble()).unwrap();
    assert_eq!(r.map["views"][0]["selected_ids"], json!(["R-EQUIP"]));
    assert_eq!(r.map["bindings"]["R-EQUIP"]["source_ids"], json!(["S3"]));
    assert!(r.tex.contains("资源可用，能力待验"));
    assert!(!r.tex.contains("按上述条件计算的小计："));
    s["views"][0]["items"] = json!([]);
    let r = report::render(&s, &preamble()).unwrap();
    assert!(r.tex.contains("不能据此判断没有问题"));
    assert_eq!(r.map["views"][0]["selected_ids"], json!([]));
}
#[test]
fn independent_render_recomputation_exposes_tex_and_map_tampering() {
    let s = sample();
    let original = report::render(&s, &preamble()).unwrap();
    let mut altered = original.clone();
    altered.tex = altered.tex.replace("2088.00", "9999.00");
    assert_ne!(report::render(&s, &preamble()).unwrap(), altered);
    let mut altered = original.clone();
    altered.map["bindings"]["R-EQUIP"]["source_ids"] = json!(["S1"]);
    assert_ne!(report::render(&s, &preamble()).unwrap(), altered);
    assert_eq!(report::render(&s, &preamble()).unwrap(), original);
}
#[test]
fn decimal_and_input_resource_bounds_fail_without_saturation() {
    for value in [
        "1e4097".to_owned(),
        "1e-4097".to_owned(),
        "7".repeat(4097),
        "١.٥".into(),
    ] {
        let mut s = sample();
        s["cost"]["lines"][0]["value"] = json!(value);
        assert!(report::validate(&s).is_err());
    }
    let mut s = sample();
    s["cost"]["lines"][0]["value"] = json!("1e4096");
    report::validate(&s).unwrap();
    assert!(
        report::cost_result(s.get("cost")).unwrap().unwrap()["known_subtotal"]
            .as_str()
            .unwrap()
            .len()
            > 4096
    );
    let mut s = sample();
    s["sources"][0]["excerpt"] = json!("x".repeat(report::MAX_INPUT_BYTES));
    assert!(report::validate(&s).is_err());
}
