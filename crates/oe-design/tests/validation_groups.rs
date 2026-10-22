//! Independent full values, source origins and refusal cases frozen before /4.
use oe_design::{Design, composition};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn foundation() -> Design {
    serde_json::from_value(json!({
        "schema":"oev2.design/1","version":"groups-test/1","problem":"Retain complete verification meaning",
        "purpose":"Share representation without removing any requirement or test",
        "requirements":[{"id":"req_probe","function":"Verify source","condition":"Bound original bytes","acceptance":"All declared cases and original criteria"}],
        "modules":[{"id":"checker","kind":"calculation","rust_path":"crates/checker","purpose":"Check exact inputs","realizes":["req_probe"],"platform":"Rust","effects":"Read only","failures":"Reject invalid source","status":"source_only"}],
        "types":[],"constraints":[{"id":"probe","requirement":"req_probe","module":"checker","check":"verify","rust_path":"crates/checker/src/lib.rs","statement":"Preserve original checks","severity":"error"}]
    })).unwrap()
}
fn group() -> Value {
    json!({"defaults":{"module":"checker","constraint":"probe","rust_path":"crates/checker/tests/source.rs","oracle":"原件\nfull oracle","scope":"all necessary checks"},
        "cases":[{"id":"v_second","behavior":"reads_second","scope":"explicit override"},{"id":"v_first","behavior":"reads_first"}]})
}
fn expected() -> Design {
    let mut d = foundation();
    d.validations=serde_json::from_value(json!([
        {"id":"v_first","module":"checker","constraint":"probe","rust_path":"crates/checker/tests/source.rs","oracle":"原件\nfull oracle","scope":"all necessary checks","behavior":"reads_first"},
        {"id":"v_second","module":"checker","constraint":"probe","rust_path":"crates/checker/tests/source.rs","oracle":"原件\nfull oracle","scope":"explicit override","behavior":"reads_second"}
    ])).unwrap();
    d.normalize().unwrap()
}
struct Fixture {
    tmp: tempfile::TempDir,
    root: PathBuf,
    fragment: PathBuf,
    body: Value,
}
impl Fixture {
    fn new() -> Self {
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let parent = crate_dir.parent().unwrap().parent().unwrap().join("var");
        let tmp = tempfile::tempdir_in(parent).unwrap();
        let root = tmp.path().join("root.toml");
        let fragment = tmp.path().join("checks.toml");
        fs::write(
            tmp.path().join("foundation.toml"),
            toml::to_string(&foundation()).unwrap(),
        )
        .unwrap();
        let mut f = Self {
            tmp,
            root,
            fragment,
            body: json!({"schema":"oev2.design-fragment/3","validation_groups":[group()]}),
        };
        f.write("oev2.design/4", Some("oev2.validation-groups/1"));
        f
    }
    fn write(&mut self, schema: &str, encoding: Option<&str>) {
        let raw = toml::to_string(&self.body).unwrap();
        fs::write(&self.fragment, &raw).unwrap();
        let f = foundation();
        let mut root = json!({"schema":schema,"version":f.version,"problem":f.problem,"purpose":f.purpose,
            "fragments":[{"path":"foundation.toml","sha256":oe_core::sha256(fs::read(self.tmp.path().join("foundation.toml")).unwrap()),"format":"oev2.design/1"},
                {"path":"checks.toml","sha256":oe_core::sha256(raw),"format":self.body["schema"]}]});
        if let Some(value) = encoding {
            root["validation_encoding"] = json!(value);
        }
        fs::write(&self.root, toml::to_string(&root).unwrap()).unwrap();
    }
    fn load(&self) -> anyhow::Result<composition::Loaded> {
        composition::load(self.tmp.path(), &self.root)
    }
    fn error(&self) -> String {
        self.load().err().expect("must refuse").to_string()
    }
}
#[test]
fn grouped4_preserves_full_design_and_rdf() {
    let f = Fixture::new();
    let loaded = f.load().unwrap();
    let expected = expected();
    assert_eq!(
        loaded.design.canonical().unwrap(),
        expected.canonical().unwrap()
    );
    assert_eq!(loaded.design.rdf().unwrap(), expected.rdf().unwrap());
    assert_eq!(
        oe_design::load(&f.root).unwrap().canonical().unwrap(),
        expected.canonical().unwrap()
    );
    let normalized: Value = serde_json::from_slice(&loaded.normalized().unwrap()).unwrap();
    assert_eq!(normalized["schema"], "oev2.normalized-design/4");
    assert_eq!(normalized["input_schema"], "oev2.design/4");
    assert_eq!(normalized["semantic_ast_schema"], "oev2.design/1");
    assert_eq!(
        normalized["design"],
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(loaded.generation_schema(), "oev2.generation/4");
    loaded
        .verify_reference_source(&normalized, &loaded.source_bytes().unwrap())
        .unwrap();
}
#[test]
fn grouped4_origins_resolve_exact_defaults_and_overrides() {
    let f = Fixture::new();
    let loaded = f.load().unwrap();
    let source = &loaded.evidence;
    assert_eq!(source["schema"], "oev2.design-composition-source/3");
    let origins = source["grouped_validation_origins"].as_array().unwrap();
    assert_eq!(origins.len(), 2);
    let raw = fs::read(&f.fragment).unwrap();
    let body: toml::Value = toml::from_str(std::str::from_utf8(&raw).unwrap()).unwrap();
    let locators = source["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["path"] == "checks.toml")
        .unwrap()["field_locators"]
        .as_array()
        .unwrap();
    let expected = serde_json::to_value(expected()).unwrap();
    for origin in origins {
        assert_eq!(origin["source"], "checks.toml");
        let row = expected["validations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == origin["validation_id"])
            .unwrap();
        let fields = origin["fields"].as_object().unwrap();
        assert_eq!(fields.len(), 7);
        for (name, path) in fields {
            let mut value = &body;
            for item in path.as_array().unwrap() {
                let key = item.as_str().unwrap();
                value = if let Some(a) = value.as_array() {
                    &a[key.parse::<usize>().unwrap()]
                } else {
                    &value[key]
                };
            }
            assert_eq!(value.as_str().unwrap(), row[name].as_str().unwrap());
            let locator = locators
                .iter()
                .find(|v| v["key_path"] == *path)
                .expect("actual parser locator");
            let start = locator["byte_start"].as_u64().unwrap() as usize;
            let end = locator["byte_end"].as_u64().unwrap() as usize;
            let one: toml::Value = toml::from_str(&format!(
                "value={}",
                std::str::from_utf8(&raw[start..end]).unwrap()
            ))
            .unwrap();
            assert_eq!(one["value"], *value);
            assert_eq!(
                locator["line_start"],
                1 + raw[..start].iter().filter(|b| **b == b'\n').count()
            );
        }
        assert_eq!(fields["id"][2], "cases");
        assert_eq!(
            fields["scope"][2],
            if row["id"] == "v_second" {
                "cases"
            } else {
                "defaults"
            }
        );
    }
}
#[test]
fn grouped4_rejects_implicit_old_admission_and_unknown_fields() {
    let mut f = Fixture::new();
    for (schema, encoding) in [
        ("oev2.design/2", None),
        ("oev2.design/3", None),
        ("oev2.design/2", Some("oev2.validation-groups/1")),
        ("oev2.design/3", Some("oev2.validation-groups/1")),
        ("oev2.design/4", None),
        ("oev2.design/4", Some("other")),
    ] {
        f.write(schema, encoding);
        assert!(f.load().is_err());
    }
    let original = f.body.clone();
    for pointer in [
        "",
        "/validation_groups/0",
        "/validation_groups/0/defaults",
        "/validation_groups/0/cases/0",
    ] {
        f.body = original.clone();
        f.body
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        f.write("oev2.design/4", Some("oev2.validation-groups/1"));
        assert!(f.load().is_err());
    }
    f.body = original.clone();
    f.body["schema"] = json!("oev2.design-fragment/2");
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.load().is_err());
    f.body = original;
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.load().is_ok());
}
#[test]
fn grouped4_rejects_missing_duplicate_invalid_and_empty_cases() {
    let mut f = Fixture::new();
    let original = f.body.clone();
    for (pointer, key) in [
        ("/validation_groups/0", "defaults"),
        ("/validation_groups/0/defaults", "oracle"),
        ("/validation_groups/0/cases/0", "id"),
        ("/validation_groups/0/cases/0", "behavior"),
    ] {
        f.body = original.clone();
        f.body
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        f.write("oev2.design/4", Some("oev2.validation-groups/1"));
        assert!(f.load().is_err());
    }
    for pointer in ["/validation_groups", "/validation_groups/0/cases"] {
        f.body = original.clone();
        *f.body.pointer_mut(pointer).unwrap() = json!([]);
        f.write("oev2.design/4", Some("oev2.validation-groups/1"));
        assert!(f.load().is_err());
    }
    for (field, value) in [
        ("id", "v_first"),
        ("module", "missing"),
        ("constraint", "missing"),
        ("scope", ""),
        ("rust_path", "../outside"),
        ("behavior", "bad-name"),
    ] {
        f.body = original.clone();
        f.body["validation_groups"][0]["cases"][0][field] = json!(value);
        f.write("oev2.design/4", Some("oev2.validation-groups/1"));
        assert!(f.load().is_err(), "{field}");
    }
    f.body = original.clone();
    f.body["validations"] = serde_json::to_value(expected().validations).unwrap();
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.load().is_err());
    f.body = original;
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.load().is_ok());
}
#[test]
fn grouped4_expansion_is_bounded_before_cloning() {
    let mut f = Fixture::new();
    let original = f.body.clone();
    f.body["validation_groups"][0]["cases"] = json!(
        (0..10_001)
            .map(|i| json!({"id":format!("v_{i}"),"behavior":"read"}))
            .collect::<Vec<_>>()
    );
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.error().contains("validation_group_row_budget"));
    f.body = original.clone();
    f.body["validation_groups"][0]["defaults"]["oracle"] = json!("\"".repeat(1024 * 1024));
    f.body["validation_groups"][0]["cases"] = json!(
        (0..8)
            .map(|i| json!({"id":format!("v_{i}"),"behavior":"read"}))
            .collect::<Vec<_>>()
    );
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.error().contains("validation_group_expanded_byte_budget"));
    f.body = original;
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.load().is_ok());
}
#[test]
fn grouped4_source_drift_forged_origins_and_recovery() {
    let mut f = Fixture::new();
    let loaded = f.load().unwrap();
    let normalized: Value = serde_json::from_slice(&loaded.normalized().unwrap()).unwrap();
    let mut source = loaded.evidence.clone();
    source["grouped_validation_origins"][0]["fields"]["scope"] =
        json!(["validation_groups", "0", "defaults", "oracle"]);
    assert!(
        loaded
            .verify_reference_source(
                &normalized,
                &oe_wire::json::canonical_json(&source).unwrap()
            )
            .is_err()
    );
    let mut downgraded = normalized.clone();
    downgraded["schema"] = json!("oev2.normalized-design/3");
    assert!(
        loaded
            .verify_reference_source(&downgraded, &loaded.source_bytes().unwrap())
            .is_err()
    );
    let original = fs::read(&f.fragment).unwrap();
    fs::write(&f.fragment, [original.as_slice(), b"\n"].concat()).unwrap();
    assert!(loaded.verify(f.tmp.path()).is_err());
    assert!(f.load().is_err());
    fs::write(&f.fragment, &original).unwrap();
    loaded.verify(f.tmp.path()).unwrap();
    f.body["validation_groups"][0]["defaults"]["scope"] = json!("changed scope");
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(loaded.verify(f.tmp.path()).is_err());
    assert_ne!(
        f.load().unwrap().design.canonical().unwrap(),
        loaded.design.canonical().unwrap()
    );
}

#[test]
fn grouped4_counts_flat_and_foundation_rows_with_exact_json_bytes() {
    let mut f = Fixture::new();
    let mut flat = expected();
    flat.validations.truncate(1);
    flat.validations[0].id = "from_foundation".into();
    flat.validations[0].oracle = "Unicode 原件, quotes \" and newline\n".into();
    fs::write(
        f.tmp.path().join("foundation.toml"),
        toml::to_string(&flat).unwrap(),
    )
    .unwrap();
    let mut row = flat.validations[0].clone();
    row.id = "from_flat_fragment".into();
    f.body["validations"] = serde_json::to_value([row]).unwrap();
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    let loaded = f.load().unwrap();
    assert_eq!(loaded.design.validations.len(), 4);
    assert_eq!(loaded.evidence["validation_expansion"]["actual_rows"], 4);
    assert_eq!(
        loaded.evidence["validation_expansion"]["actual_json_bytes"],
        serde_json::to_vec(&loaded.design.validations)
            .unwrap()
            .len()
    );
    f.body["validation_groups"][0]["cases"] = json!(
        (0..9_999)
            .map(|i| json!({"id":format!("v_{i}"),"behavior":"read"}))
            .collect::<Vec<_>>()
    );
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    assert!(f.error().contains("validation_group_row_budget"));
}

#[test]
fn grouped4_all_overrides_resolve_to_actual_case_fields() {
    let mut f = Fixture::new();
    // Invalid defaults are unused here; all effective values are declared
    // explicitly on the case and must point to those actual source positions.
    let row = serde_json::to_value(&expected().validations[0]).unwrap();
    f.body["validation_groups"][0]["cases"] = json!([row]);
    for key in ["module", "constraint", "rust_path", "oracle", "scope"] {
        f.body["validation_groups"][0]["defaults"][key] = json!("unused default");
    }
    f.write("oev2.design/4", Some("oev2.validation-groups/1"));
    let loaded = f.load().unwrap();
    assert_eq!(
        serde_json::to_value(&loaded.design.validations[0]).unwrap(),
        row
    );
    let origin = &loaded.evidence["grouped_validation_origins"][0];
    let locators = loaded.evidence["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["path"] == "checks.toml")
        .unwrap()["field_locators"]
        .as_array()
        .unwrap();
    for (field, path) in origin["fields"].as_object().unwrap() {
        assert_eq!(
            *path,
            json!(["validation_groups", "0", "cases", "0", field])
        );
        assert!(locators.iter().any(|v| v["key_path"] == *path));
    }
}
