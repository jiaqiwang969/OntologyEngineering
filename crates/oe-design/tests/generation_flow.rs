use oe_core::{SkillRoot, read_regular_bytes};
use oe_design::{Design, generation};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: SkillRoot,
    tool: PathBuf,
    design: Design,
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn whole_semantica_scope_is_bound_before_generation_and_invalid_scope_preserves_version() {
    let f = Fixture::new();
    let first = f.generate(&f.design);
    let model = std::fs::read_to_string(first.join("model.ttl")).unwrap();
    let scope_path = f.root.path().join("runtime/semantica-scope.json");
    let raw = std::fs::read(&scope_path).unwrap();
    let scope_sha = oe_core::sha256(&raw);
    assert!(model.contains(&scope_sha));
    assert!(model.contains("<urn:oev2:design:migrationStatus> \"incomplete\""));
    let source_map: serde_json::Value =
        serde_json::from_slice(&std::fs::read(first.join("source-map.json")).unwrap()).unwrap();
    assert_eq!(source_map["migration_scope"][0]["scope_sha256"], scope_sha);
    assert_eq!(
        source_map["migration_scope"][0]["migration_status"],
        "incomplete"
    );
    assert_eq!(
        source_map["migration_scope"][0]["behavior_status"],
        "unknown"
    );
    assert!(
        source_map["migration_assets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| { v["path"] == "runtime/semantica-scope.json" && v["sha256"] == scope_sha })
    );
    generation::publish(&f.root, &first).unwrap();
    let previous = generation::snapshot(&f.root.path().join("generated")).unwrap();

    // A syntactically legal reduced scope and matching self-reported lock cannot
    // substitute the approved original. This is a scope change, not a migration.
    let mut scope: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    scope["console_entrypoints"].as_array_mut().unwrap().pop();
    let reduced = serde_json::to_vec_pretty(&scope).unwrap();
    std::fs::write(&scope_path, &reduced).unwrap();
    let lock_path = f.root.path().join("runtime/semantica-scope-lock.json");
    let mut lock: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&lock_path).unwrap()).unwrap();
    lock["scope_sha256"] = json!(oe_core::sha256(&reduced));
    lock["scope_bytes"] = json!(reduced.len());
    std::fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();
    let error = generation::run(&f.root, &f.design, &f.tool)
        .unwrap_err()
        .to_string();
    assert!(error.contains("migration_scope"), "{error}");
    assert_eq!(
        previous,
        generation::snapshot(&f.root.path().join("generated")).unwrap(),
        "invalid scope must leave the previous generated version intact"
    );
}
impl Fixture {
    fn new() -> Self {
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let actual = SkillRoot::new(original).unwrap();
        let path = actual
            .new_private_dir(Path::new(&format!(
                "var/maintenance/generation-tests/{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            )))
            .unwrap();
        std::fs::write(path.join("SKILL.md"), "isolated actual ggen verification\n").unwrap();
        for directory in ["templates", "runtime"] {
            std::fs::create_dir(path.join(directory)).unwrap();
        }
        for input in oe_design::inspection::files(original, &original.join("templates")).unwrap() {
            let output = path.join(input.strip_prefix(original).unwrap());
            std::fs::create_dir_all(output.parent().unwrap()).unwrap();
            std::fs::copy(&input, output).unwrap();
        }
        for input in oe_design::inspection::files(original, &original.join("crates")).unwrap() {
            if input.extension().is_some_and(|s| s == "rs") {
                let output = path.join(input.strip_prefix(original).unwrap());
                std::fs::create_dir_all(output.parent().unwrap()).unwrap();
                std::fs::copy(&input, output).unwrap();
            }
        }
        std::fs::copy(
            original.join("runtime/ggen-lock.json"),
            path.join("runtime/ggen-lock.json"),
        )
        .unwrap();
        // Fixture acquires the now-required build inventory; all original
        // behavior/oracle assertions below stay unchanged.
        for p in oe_design::provenance::compiled().unwrap()["files"]
            .as_object()
            .unwrap()
            .keys()
        {
            let target = path.join(p);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(original.join(p), target).unwrap();
        }
        Self {
            root: SkillRoot::new(path).unwrap(),
            tool: original.join("var/builds/ggen-target/debug/ggen"),
            design: oe_design::load(&original.join("specs/engineering.toml")).unwrap(),
        }
    }
    fn generate(&self, d: &Design) -> PathBuf {
        generation::run(&self.root, d, &self.tool).unwrap().0
    }
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn real_ggen_updates_contract_and_docs_and_detects_drift() {
    let f = Fixture::new();
    let first = f.generate(&f.design);
    let identity = generation::snapshot(&first).unwrap();
    assert_eq!(
        identity,
        generation::snapshot(&f.generate(&f.design)).unwrap(),
        "byte determinism"
    );
    generation::publish(&f.root, &first).unwrap();
    let mut changed = f.design.clone();
    changed
        .types
        .iter_mut()
        .find(|t| t.name == "Quantity")
        .unwrap()
        .fields
        .iter_mut()
        .find(|v| v.name == "unit")
        .unwrap()
        .meaning = "independent revision: explicit physical unit with no inferred unit".into();
    let revised = f.generate(&changed);
    for file in ["contracts.rs", "interfaces.md"] {
        assert_ne!(
            read_regular_bytes(&f.root.path().join("generated").join(file), 4 * 1024 * 1024)
                .unwrap(),
            read_regular_bytes(&revised.join(file), 4 * 1024 * 1024).unwrap()
        );
        assert!(
            std::fs::read_to_string(revised.join(file))
                .unwrap()
                .contains("independent revision: explicit physical unit")
        );
    }
    generation::publish(&f.root, &revised).unwrap();
    assert_eq!(
        generation::check(&f.root, &changed, &f.tool).unwrap()["status"],
        "consistent"
    );
    let working = f.root.path().join("generated");
    std::fs::write(
        working.join("contracts.rs"),
        "drift introduced by independent test",
    )
    .unwrap();
    let before = generation::snapshot(&working).unwrap();
    let report = generation::check(&f.root, &changed, &f.tool).unwrap();
    assert_eq!(report["status"], "drift");
    assert!(
        report["drift"]
            .as_array()
            .unwrap()
            .contains(&json!("contracts.rs"))
    );
    assert_eq!(
        before,
        generation::snapshot(&working).unwrap(),
        "check must not repair or publish"
    );
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn failed_real_ggen_preserves_previous_working_version() {
    let f = Fixture::new();
    generation::publish(&f.root, &f.generate(&f.design)).unwrap();
    let before = generation::snapshot(&f.root.path().join("generated")).unwrap();
    let mut invalid = f.design.clone();
    invalid.types[0].fields[0].name = "not a Rust identifier".into();
    assert!(generation::run(&f.root, &invalid, &f.tool).is_err());
    std::fs::write(
        f.root.path().join("templates/pack/contracts.rs.tmpl"),
        "---\nto: out/contracts.rs\n---\n{% impossible_tag %}",
    )
    .unwrap();
    let error = generation::run(&f.root, &f.design, &f.tool)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("ggen_execution_failed"),
        "must actually reach ggen: {error}"
    );
    assert_eq!(
        before,
        generation::snapshot(&f.root.path().join("generated")).unwrap()
    );
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn new_contract_is_generated_compiled_and_behaves_against_independent_inputs() {
    let f = Fixture::new();
    let mut d = f.design.clone();
    let mut added = d
        .types
        .iter()
        .find(|t| t.name == "Quantity")
        .unwrap()
        .clone();
    added.name = "ThermalLimit".into();
    added.meaning = "new engineering temperature limit with explicit physical unit".into();
    d.types.push(added);
    let out = f.generate(&d);
    let receiver = f.root.path().join("var/receiver-contract");
    std::fs::create_dir_all(receiver.join("src")).unwrap();
    std::fs::copy(out.join("contracts.rs"), receiver.join("src/lib.rs")).unwrap();
    std::fs::write(receiver.join("Cargo.toml"),"[package]\nname='independent-new-contract'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['semantic-contracts','semantic-json']\n[dependencies]\nsemantica-contracts={path='semantic-contracts'}\nserde={version='=1.0.229',features=['derive']}\nserde_json={version='=1.0.151',features=['preserve_order','arbitrary_precision','raw_value']}\n").unwrap();
    std::fs::create_dir(receiver.join("tests")).unwrap();
    std::fs::write(receiver.join("tests/temperature.rs"),r##"use independent_new_contract::ThermalLimit;
#[test] fn declared_limit_accepts_60_degc(){let v:ThermalLimit=serde_json::from_str(r#"{"unit":"degC","value":60.0,"precision":"tolerance unknown"}"#).unwrap();assert!(v.validate().is_ok());}
#[test] fn missing_unit_is_rejected(){let v:ThermalLimit=serde_json::from_str(r#"{"unit":"","value":60.0,"precision":"tolerance unknown"}"#).unwrap();assert!(v.validate().is_err());}
#[test] fn infinite_limit_is_rejected(){assert!(ThermalLimit{unit:"degC".into(),value:f64::INFINITY,precision:"tolerance unknown".into()}.validate().is_err());}
"##).unwrap();
    receiver_json_support(&f, &out, &receiver);
    lock_receiver_coordinates(&f, &receiver);
    let result = std::process::Command::new("rustup")
        .args(["run", "1.95.0", "cargo", "test", "--offline", "--locked"])
        .env("CARGO_TARGET_DIR", receiver.join("var/receiver-target"))
        .current_dir(&receiver)
        .output()
        .unwrap();
    std::fs::write(receiver.join("execution.stdout"), &result.stdout).unwrap();
    std::fs::write(receiver.join("execution.stderr"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn grouped_contracts_compile_and_preserve_semantic_drift() {
    let f = Fixture::new();
    let mut d = f.design.clone();
    let mut carrier = d
        .types
        .iter()
        .find(|t| t.name == "Quantity")
        .unwrap()
        .clone();
    carrier.name = "QueryEnvelope".into();
    carrier.meaning = "independent engineering caller consumes canonical generic query".into();
    carrier.fields.truncate(1);
    carrier.fields[0].name = "request".into();
    carrier.fields[0].rust_type = "SemanticQueryRequest".into();
    carrier.fields[0].format = "object".into();
    d.types.push(carrier);
    let out = f.generate(&d);
    let engineering = std::fs::read_to_string(out.join("contracts.rs")).unwrap();
    let semantic = std::fs::read_to_string(out.join("semantic-contracts.rs")).unwrap();
    assert!(!engineering.contains("pub struct SemanticQueryRequest"));
    assert!(engineering.contains("pub use semantica_contracts::SemanticQueryRequest"));
    assert!(!engineering.contains("pub trait SemanticQueryPort"));
    assert!(semantic.contains("pub trait SemanticQueryPort"));
    assert!(!semantic.contains("pub struct ReviewRequest"));
    assert!(!semantic.contains("oe_contracts::"));
    assert!(!semantic.contains("self.data.trim().is_empty()"));

    let receiver = f.root.path().join("var/grouped-contracts-receiver");
    for folder in ["src", "semantic-contracts/src", "semantic-contracts/tests"] {
        std::fs::create_dir_all(receiver.join(folder)).unwrap();
    }
    std::fs::copy(out.join("contracts.rs"), receiver.join("src/lib.rs")).unwrap();
    std::fs::copy(
        out.join("semantic-contracts.rs"),
        receiver.join("semantic-contracts/src/lib.rs"),
    )
    .unwrap();
    std::fs::copy(
        out.join("semantic-contract-tests.rs"),
        receiver.join("semantic-contracts/tests/generated.rs"),
    )
    .unwrap();
    std::fs::write(receiver.join("Cargo.toml"), "[package]\nname='grouped-contracts-receiver'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['semantic-contracts','semantic-json']\n[dependencies]\nserde={version='=1.0.229',features=['derive']}\nserde_json={version='=1.0.151',features=['preserve_order','arbitrary_precision','raw_value']}\nsemantica-contracts={path='semantic-contracts'}\n").unwrap();
    std::fs::write(receiver.join("semantic-contracts/Cargo.toml"), "[package]\nname='semantica-contracts'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nsemantica-json={path='../semantic-json'}\nserde={version='=1.0.229',features=['derive']}\nserde_json={version='=1.0.151',features=['preserve_order','arbitrary_precision','raw_value']}\n").unwrap();
    receiver_json_support(&f, &out, &receiver);
    lock_receiver_coordinates(&f, &receiver);
    let result = std::process::Command::new("rustup")
        .args([
            "run",
            "1.95.0",
            "cargo",
            "test",
            "--offline",
            "--locked",
            "--workspace",
        ])
        .env("CARGO_TARGET_DIR", receiver.join("var/receiver-target"))
        .current_dir(&receiver)
        .output()
        .unwrap();
    std::fs::write(receiver.join("execution.stdout"), &result.stdout).unwrap();
    std::fs::write(receiver.join("execution.stderr"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    generation::publish(&f.root, &out).unwrap();
    let products = generation::snapshot(&f.root.path().join("generated")).unwrap();
    std::fs::write(
        f.root.path().join("generated/semantic-contracts.rs"),
        "independent semantic drift",
    )
    .unwrap();
    let changed = generation::snapshot(&f.root.path().join("generated")).unwrap();
    let check = generation::check(&f.root, &d, &f.tool).unwrap();
    assert_eq!(check["status"], "drift");
    assert!(
        check["drift"]
            .as_array()
            .unwrap()
            .contains(&json!("semantic-contracts.rs"))
    );
    assert_eq!(
        generation::snapshot(&f.root.path().join("generated")).unwrap(),
        changed
    );
    assert_ne!(products, changed);
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn template_or_product_changed_after_generation_is_rejected_before_publication() {
    let f = Fixture::new();
    let output = f.generate(&f.design);
    let p = f.root.path().join("templates/pack/interfaces.md.tmpl");
    std::fs::write(p, "changed since real ggen executed").unwrap();
    assert!(
        generation::publish(&f.root, &output)
            .unwrap_err()
            .to_string()
            .contains("generation_source_changed")
    );
    assert!(!f.root.path().join("generated").exists());
    let f = Fixture::new();
    let output = f.generate(&f.design);
    std::fs::write(output.join("contracts.rs"), "staged product tamper").unwrap();
    assert!(
        generation::publish(&f.root, &output)
            .unwrap_err()
            .to_string()
            .contains("generation_staged_product_drift")
    );
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn source_drift_and_a_missing_checker_are_detected_by_actual_model_queries() {
    let f = Fixture::new();
    generation::publish(&f.root, &f.generate(&f.design)).unwrap();
    let file = f.root.path().join("crates/oe-design/src/lib.rs");
    let original = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        format!("{original}\n// independently introduced source drift\n"),
    )
    .unwrap();
    assert_eq!(
        generation::check(&f.root, &f.design, &f.tool).unwrap()["status"],
        "drift"
    );
    let working = generation::snapshot(&f.root.path().join("generated")).unwrap();
    std::fs::write(
        &file,
        original.replace("pub fn validate(", "pub fn renamed_checker("),
    )
    .unwrap();
    assert!(
        generation::run(&f.root, &f.design, &f.tool)
            .unwrap_err()
            .to_string()
            .contains("generation_constraint_checker_missing")
    );
    assert_eq!(
        working,
        generation::snapshot(&f.root.path().join("generated")).unwrap()
    );
}

fn declared_domain_type(
    design: &Design,
    name: &str,
    group: &str,
    rust_type: &str,
    required: bool,
    format: &str,
    allowed_values: Vec<String>,
) -> oe_design::Type {
    let prototype = design.types.iter().find(|t| t.name == "Quantity").unwrap();
    let mut field = prototype
        .fields
        .iter()
        .find(|f| f.name == "unit")
        .unwrap()
        .clone();
    field.name = "value".into();
    field.meaning = "Exact membership applies to a present value".into();
    field.rust_type = rust_type.into();
    field.required = required;
    field.format = format.into();
    field.unit = "UTF-8 bytes".into();
    field.precision = "Exact declared value membership".into();
    field.write = "Independent generated contract receiver".into();
    field.applicability = "Every present value with a declared domain".into();
    field.acceptance = "Preserve absence; reject any present value outside the exact domain".into();
    field.allowed_values = allowed_values;
    oe_design::Type {
        name: name.into(),
        contract_group: group.into(),
        meaning: "Independent declared-domain contract".into(),
        identity: "Value and declared domain".into(),
        fields: vec![field],
    }
}

fn run_declared_domain_receiver(f: &Fixture, output: &Path, label: &str, oracle: &str) {
    let receiver = f.root.path().join("var").join(label);
    for folder in ["src", "tests", "semantic-contracts/src"] {
        std::fs::create_dir_all(receiver.join(folder)).unwrap();
    }
    std::fs::copy(output.join("contracts.rs"), receiver.join("src/lib.rs")).unwrap();
    std::fs::copy(
        output.join("semantic-contracts.rs"),
        receiver.join("semantic-contracts/src/lib.rs"),
    )
    .unwrap();
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let parent: toml::Value =
        toml::from_str(&std::fs::read_to_string(original.join("Cargo.lock")).unwrap()).unwrap();
    let packages = parent["package"].as_array().unwrap();
    let version = |name: &str| {
        packages
            .iter()
            .find(|p| p["name"].as_str() == Some(name))
            .unwrap()["version"]
            .as_str()
            .unwrap()
    };
    let dependencies = format!(
        "[dependencies]\nserde={{version='={}',features=['derive']}}\nserde_json={{version='={}',features=['arbitrary_precision','raw_value','preserve_order']}}\n",
        version("serde"),
        version("serde_json"),
    );
    std::fs::write(receiver.join("Cargo.toml"), format!(
        "[package]\nname='{label}'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['semantic-contracts']\n{dependencies}semantica-contracts={{path='semantic-contracts'}}\n",
    )).unwrap();
    std::fs::write(receiver.join("semantic-contracts/Cargo.toml"), format!(
        "[package]\nname='semantica-contracts'\nversion='0.0.0'\nedition='2024'\n{dependencies}",
    )).unwrap();
    receiver_json_support(f, output, &receiver);
    std::fs::write(receiver.join("tests/independent.rs"), oracle).unwrap();
    std::fs::copy(original.join("Cargo.lock"), receiver.join("Cargo.lock")).unwrap();
    let lock = std::process::Command::new("rustup")
        .args([
            "run",
            "1.95.0",
            "cargo",
            "metadata",
            "--offline",
            "--format-version",
            "1",
        ])
        .env("CARGO_TARGET_DIR", receiver.join("var/receiver-target"))
        .current_dir(&receiver)
        .output()
        .unwrap();
    std::fs::write(receiver.join("lock.stdout"), &lock.stdout).unwrap();
    std::fs::write(receiver.join("lock.stderr"), &lock.stderr).unwrap();
    assert!(
        lock.status.success(),
        "{}",
        String::from_utf8_lossy(&lock.stderr)
    );
    let selected: toml::Value =
        toml::from_str(&std::fs::read_to_string(receiver.join("Cargo.lock")).unwrap()).unwrap();
    for package in selected["package"].as_array().unwrap() {
        if package.get("source").is_some() {
            assert!(
                packages
                    .iter()
                    .any(|p| ["name", "version", "source", "checksum"]
                        .iter()
                        .all(|key| p.get(key) == package.get(key))),
                "independent receiver must keep the exact parent dependency coordinates"
            );
        }
    }
    let result = std::process::Command::new("rustup")
        .args([
            "run",
            "1.95.0",
            "cargo",
            "test",
            "--locked",
            "--offline",
            "--workspace",
            "-j",
            "4",
            "--target-dir",
        ])
        .arg(receiver.join("var/absolute-target"))
        .env("CARGO_TARGET_DIR", receiver.join("var/receiver-target"))
        .current_dir(&receiver)
        .output()
        .unwrap();
    std::fs::write(receiver.join("execution.stdout"), &result.stdout).unwrap();
    std::fs::write(receiver.join("execution.stderr"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "independent generated contract oracle failed; stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn all_declared_string_domains_are_generated_and_checked_in_both_groups() {
    let f = Fixture::new();
    let mut d = f.design.clone();
    let values = vec!["qualification".to_string(), r#"quoted"\β"#.to_string()];
    for (name, group, rust_type, required) in [
        ("EngineeringRequiredDomain", "engineering", "String", true),
        (
            "EngineeringOptionalDomain",
            "engineering",
            "Option<String>",
            false,
        ),
        ("SemanticRequiredDomain", "semantic", "String", true),
        (
            "SemanticOptionalDomain",
            "semantic",
            "Option<String>",
            false,
        ),
    ] {
        d.types.push(declared_domain_type(
            &f.design,
            name,
            group,
            rust_type,
            required,
            "id",
            values.clone(),
        ));
    }
    for (name, group) in [
        ("EngineeringOpenText", "engineering"),
        ("SemanticOpenText", "semantic"),
    ] {
        d.types.push(declared_domain_type(
            &f.design,
            name,
            group,
            "String",
            true,
            "raw_utf8",
            vec![],
        ));
    }
    let out = f.generate(&d);
    run_declared_domain_receiver(
        &f,
        &out,
        "declared-domains-receiver",
        r##"
use declared_domains_receiver::{EngineeringRequiredDomain, EngineeringOptionalDomain, EngineeringOpenText};
use semantica_contracts::{SemanticRequiredDomain, SemanticOptionalDomain, SemanticOpenText};
#[test] fn required_exact_domains() {
    for value in ["qualification", r#"quoted"\β"#] {
        assert!(EngineeringRequiredDomain { value: value.into() }.validate().is_ok());
        assert!(SemanticRequiredDomain { value: value.into() }.validate().is_ok());
    }
    for value in ["development", "QUALIFICATION", " qualification ", ""] {
        assert!(EngineeringRequiredDomain { value: value.into() }.validate().is_err(), "engineering must enforce declared membership: {value:?}");
        assert!(SemanticRequiredDomain { value: value.into() }.validate().is_err(), "semantic must enforce declared membership: {value:?}");
    }
}
#[test] fn optional_exact_domains_and_absence() {
    assert!(EngineeringOptionalDomain { value: None }.validate().is_ok());
    assert!(SemanticOptionalDomain { value: None }.validate().is_ok());
    for value in ["qualification", r#"quoted"\β"#] {
        assert!(EngineeringOptionalDomain { value: Some(value.into()) }.validate().is_ok());
        assert!(SemanticOptionalDomain { value: Some(value.into()) }.validate().is_ok());
    }
    for value in ["development", "QUALIFICATION", " qualification ", ""] {
        assert!(EngineeringOptionalDomain { value: Some(value.into()) }.validate().is_err(), "engineering optional domain: {value:?}");
        assert!(SemanticOptionalDomain { value: Some(value.into()) }.validate().is_err(), "semantic optional domain: {value:?}");
    }
}
#[test] fn undeclared_raw_text_remains_open() {
    for value in ["", " ", "development", r#"quoted"\β"#] {
        assert!(EngineeringOpenText { value: value.into() }.validate().is_ok());
        assert!(SemanticOpenText { value: value.into() }.validate().is_ok());
    }
}
"##,
    );
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn optional_string_formats_compile_and_behave_in_the_semantic_group() {
    let f = Fixture::new();
    let mut d = f.design.clone();
    for (name, format, values) in [
        ("SemanticOptionalDigestProbe", "sha256", vec![]),
        (
            "SemanticOptionalStatusProbe",
            "status",
            vec!["unknown".to_string(), "completed".to_string()],
        ),
    ] {
        d.types.push(declared_domain_type(
            &f.design,
            name,
            "semantic",
            "Option<String>",
            false,
            format,
            values,
        ));
    }
    let out = f.generate(&d);
    run_declared_domain_receiver(
        &f,
        &out,
        "optional-formats-receiver",
        r##"
use semantica_contracts::{SemanticOptionalDigestProbe, SemanticOptionalStatusProbe};
#[test] fn digest_absence_and_strict_present_format() {
    assert!(SemanticOptionalDigestProbe { value: None }.validate().is_ok());
    assert!(SemanticOptionalDigestProbe { value: Some("a".repeat(64)) }.validate().is_ok());
    for value in ["A".repeat(64), "a".repeat(63), "g".repeat(64), String::new()] {
        assert!(SemanticOptionalDigestProbe { value: Some(value) }.validate().is_err());
    }
}
#[test] fn status_absence_and_exact_membership() {
    assert!(SemanticOptionalStatusProbe { value: None }.validate().is_ok());
    for value in ["unknown", "completed"] {
        assert!(SemanticOptionalStatusProbe { value: Some(value.into()) }.validate().is_ok());
    }
    for value in ["unknown ", "development", ""] {
        assert!(SemanticOptionalStatusProbe { value: Some(value.into()) }.validate().is_err());
    }
}
"##,
    );
}

#[test]
#[ignore = "requires the locked official ggen binary; explicitly run for acceptance"]
fn generated_json_containers_preserve_identity_and_original_absence_semantics() {
    let f = Fixture::new();
    let mut d = f.design.clone();
    let source = d
        .types
        .iter()
        .find(|t| t.name == "SemanticMcpRequest")
        .unwrap()
        .clone();
    let leaf = source
        .fields
        .iter()
        .find(|v| v.name == "request")
        .unwrap()
        .clone();
    for (name, group) in [
        ("NeutralJsonContainers", "semantic"),
        ("EngineeringJsonContainers", "engineering"),
    ] {
        let mut contract = source.clone();
        contract.name = name.into();
        contract.contract_group = group.into();
        contract.meaning =
            "Independent JSON container identity receiver; no engineering authority".into();
        contract.fields.clear();
        for (name, rust_type, required) in [
            ("values", "Vec<Option<serde_json::Value>>", true),
            ("optional", "Option<Vec<serde_json::Value>>", false),
        ] {
            let mut field = leaf.clone();
            field.name = name.into();
            field.rust_type = rust_type.into();
            field.required = required;
            field.meaning =
                "Original JSON object/list/scalar identity, preserving Option/Vec shape".into();
            contract.fields.push(field);
        }
        d.types.push(contract);
    }
    let out = f.generate(&d);
    let receiver = f.root.path().join("var/json-container-receiver");
    std::fs::create_dir_all(receiver.join("src")).unwrap();
    std::fs::create_dir_all(receiver.join("tests")).unwrap();
    std::fs::copy(out.join("contracts.rs"), receiver.join("src/lib.rs")).unwrap();
    std::fs::write(receiver.join("Cargo.toml"), "[package]\nname='json-container-receiver'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['semantic-contracts','semantic-json']\n[dependencies]\nsemantica-contracts={path='semantic-contracts'}\nserde={version='=1.0.229',features=['derive']}\nserde_json={version='=1.0.151',features=['preserve_order','arbitrary_precision','raw_value']}\n").unwrap();
    std::fs::write(receiver.join("tests/containers.rs"), r##"use json_container_receiver::EngineeringJsonContainers;
use semantica_contracts::{NeutralJsonContainers,json::protocol_json};
const RAW:&str=r#"{"values":[{"$serde_json::private::Number":"42"},null,[{"$serde_json::private::RawValue":"true"}]],"optional":[12345678901234567890123456789012345678901234567890]}"#;
fn preserved(values:&[Option<serde_json::Value>],optional:&Option<Vec<serde_json::Value>>){
assert!(values[0].as_ref().unwrap().is_object());assert_eq!(values[0].as_ref().unwrap()["$serde_json::private::Number"],"42");
assert!(values[1].is_none());assert_eq!(values[2].as_ref().unwrap()[0]["$serde_json::private::RawValue"],"true");
assert!(optional.as_ref().unwrap()[0].is_number());assert_eq!(optional.as_ref().unwrap()[0].to_string(),"12345678901234567890123456789012345678901234567890");}
#[test]fn neutral_raw_and_value_containers_preserve_identity(){let original=protocol_json(RAW,4096).unwrap();for v in [serde_json::from_str::<NeutralJsonContainers>(RAW).unwrap(),serde_json::from_value::<NeutralJsonContainers>(original).unwrap()]{preserved(&v.values,&v.optional);}}
#[test]fn engineering_raw_and_value_containers_preserve_identity(){let original=protocol_json(RAW,4096).unwrap();for v in [serde_json::from_str::<EngineeringJsonContainers>(RAW).unwrap(),serde_json::from_value::<EngineeringJsonContainers>(original).unwrap()]{preserved(&v.values,&v.optional);}}
#[test]fn original_option_absence_and_null_survive(){for raw in [r#"{"values":[]}"#,r#"{"values":[],"optional":null}"#]{assert!(serde_json::from_str::<NeutralJsonContainers>(raw).unwrap().optional.is_none());assert!(serde_json::from_str::<EngineeringJsonContainers>(raw).unwrap().optional.is_none());}}
#[test]fn required_vec_missing_null_wrong_shape_and_extra_field_stay_errors(){for raw in [r#"{}"#,r#"{"values":null}"#,r#"{"values":{}}"#,r#"{"values":[],"optional":{}}"#,r#"{"values":[],"eligible":true}"#]{assert!(serde_json::from_str::<NeutralJsonContainers>(raw).is_err());assert!(serde_json::from_str::<EngineeringJsonContainers>(raw).is_err());}}
"##).unwrap();
    receiver_json_support(&f, &out, &receiver);
    lock_receiver_coordinates(&f, &receiver);
    let result = std::process::Command::new("rustup")
        .args([
            "run",
            "1.95.0",
            "cargo",
            "test",
            "--offline",
            "--locked",
            "--workspace",
        ])
        .env("CARGO_TARGET_DIR", receiver.join("var/receiver-target"))
        .current_dir(&receiver)
        .output()
        .unwrap();
    std::fs::write(receiver.join("execution.stdout"), &result.stdout).unwrap();
    std::fs::write(receiver.join("execution.stderr"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

// The generated JSON field hooks require the same neutral JSON owner in a
// fresh receiver. Copy exact generated/helper bytes, with only normal crate
// build context; the original receiver behavior assertions remain unchanged.
fn receiver_json_support(f: &Fixture, out: &Path, receiver: &Path) {
    let neutral = receiver.join("semantic-contracts");
    let json_owner = receiver.join("semantic-json");
    std::fs::create_dir_all(json_owner.join("src")).unwrap();
    // The current owner has additional Rust child modules. Receive its whole
    // source tree; a partial lib.rs copy cannot execute the original oracles.
    let source = f.root.path().join("crates/semantica-json/src");
    for input in oe_design::inspection::files(f.root.path(), &source).unwrap() {
        let output = json_owner
            .join("src")
            .join(input.strip_prefix(&source).unwrap());
        std::fs::create_dir_all(output.parent().unwrap()).unwrap();
        std::fs::copy(&input, &output).unwrap();
    }
    std::fs::write(json_owner.join("Cargo.toml"), "[package]\nname='semantica-json'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nserde={version='=1.0.229',features=['derive']}\nserde_json={version='=1.0.151',features=['preserve_order','arbitrary_precision','raw_value']}\n").unwrap();
    std::fs::create_dir_all(neutral.join("src")).unwrap();
    std::fs::create_dir_all(neutral.join("tests")).unwrap();
    std::fs::copy(
        out.join("semantic-contracts.rs"),
        neutral.join("src/generated.rs"),
    )
    .unwrap();
    std::fs::copy(
        f.root.path().join("crates/semantica-contracts/src/json.rs"),
        neutral.join("src/json.rs"),
    )
    .unwrap();
    std::fs::copy(
        f.root
            .path()
            .join("crates/semantica-contracts/src/models.rs"),
        neutral.join("src/models.rs"),
    )
    .unwrap();
    std::fs::write(
        neutral.join("src/lib.rs"),
        "pub mod json;\npub mod models;\npub use models::SemanticModelSnapshot;\ninclude!(\"generated.rs\");\n",
    )
    .unwrap();
    std::fs::copy(
        out.join("semantic-contract-tests.rs"),
        neutral.join("tests/generated.rs"),
    )
    .unwrap();
    std::fs::copy(
        f.root
            .path()
            .join("crates/semantica-contracts/tests/json_identity.rs"),
        neutral.join("tests/json_identity.rs"),
    )
    .unwrap();
    std::fs::write(neutral.join("Cargo.toml"),"[package]\nname='semantica-contracts'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nsemantica-json={path='../semantic-json'}\nserde={version='=1.0.229',features=['derive']}\nserde_json={version='=1.0.151',features=['preserve_order','arbitrary_precision','raw_value']}\n").unwrap();
}
fn lock_receiver_coordinates(f: &Fixture, receiver: &Path) {
    std::fs::copy(
        f.root.path().join("Cargo.lock"),
        receiver.join("Cargo.lock"),
    )
    .unwrap();
    let lock = std::process::Command::new("rustup")
        .args([
            "run",
            "1.95.0",
            "cargo",
            "metadata",
            "--offline",
            "--format-version",
            "1",
        ])
        .env("CARGO_TARGET_DIR", receiver.join("var/receiver-target"))
        .current_dir(receiver)
        .output()
        .unwrap();
    std::fs::write(receiver.join("lock.stdout"), &lock.stdout).unwrap();
    std::fs::write(receiver.join("lock.stderr"), &lock.stderr).unwrap();
    assert!(
        lock.status.success(),
        "{}",
        String::from_utf8_lossy(&lock.stderr)
    );
    let coordinates = |path: &Path| {
        let lock: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        lock["package"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p.get("source").is_some())
            .map(|p| {
                ["name", "version", "source", "checksum"]
                    .map(|key| p[key].as_str().unwrap().to_owned())
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert!(
        coordinates(&receiver.join("Cargo.lock"))
            .is_subset(&coordinates(&f.root.path().join("Cargo.lock"))),
        "fresh receiver external coordinates must be the pinned workspace subset"
    );
}

#[test]
#[ignore = "requires locked official ggen and actual independent receiver compilation"]
fn optional_named_objects_validate_none_some_and_required_in_both_groups() {
    let f = Fixture::new();
    // A private historical-template override exercises the exact original defect;
    // only the isolated fixture is changed, never the production generator.
    if let Some(path) = std::env::var_os("OPTIONAL_NAMED_OBJECT_LEGACY_TEMPLATES") {
        for name in ["contracts.rs.tmpl", "semantic-contracts.rs.tmpl"] {
            std::fs::copy(
                Path::new(&path).join(name),
                f.root.path().join("templates/pack").join(name),
            )
            .unwrap();
        }
    }
    let mut d = f.design.clone();
    let base = d
        .types
        .iter()
        .find(|t| t.name == "Quantity")
        .unwrap()
        .clone();
    for group in ["engineering", "semantic"] {
        for optional in [true, false] {
            let mut added = base.clone();
            added.name = format!(
                "{}{}Object",
                if group == "semantic" {
                    "Neutral"
                } else {
                    "Engineering"
                },
                if optional { "Optional" } else { "Required" }
            );
            added.contract_group = group.into();
            added.meaning =
                "independently tested optional and mandatory named-object validation".into();
            added.fields.truncate(1);
            let field = &mut added.fields[0];
            field.name = "inner".into();
            field.rust_type = if optional {
                "Option<SemanticBookCatalogResult>"
            } else {
                "SemanticBookCatalogResult"
            }
            .into();
            field.required = !optional;
            field.format = "object".into();
            field.allowed_values.clear();
            d.types.push(added);
        }
    }
    let out = f.generate(&d);
    let receiver = f.root.path().join("var/optional-named-object-receiver");
    std::fs::create_dir_all(receiver.join("src")).unwrap();
    std::fs::copy(out.join("contracts.rs"), receiver.join("src/lib.rs")).unwrap();
    std::fs::write(receiver.join("Cargo.toml"), "[package]\nname='optional-named-object-receiver'\nversion='0.0.0'\nedition='2024'\n[workspace]\nmembers=['semantic-contracts','semantic-json']\n[dependencies]\nsemantica-contracts={path='semantic-contracts'}\nserde={version='=1.0.229',features=['derive']}\nserde_json={version='=1.0.151',features=['preserve_order','arbitrary_precision','raw_value']}\n").unwrap();
    receiver_json_support(&f, &out, &receiver);
    lock_receiver_coordinates(&f, &receiver);
    std::fs::create_dir(receiver.join("tests")).unwrap();
    std::fs::write(receiver.join("tests/oracle.rs"), r##"
use optional_named_object_receiver::{EngineeringOptionalObject,EngineeringRequiredObject};
use semantica_contracts::{NeutralOptionalObject,NeutralRequiredObject,SemanticBookCatalogResult};
fn valid()->SemanticBookCatalogResult { SemanticBookCatalogResult {schema:"semantica.book-catalog-result/1".into(),source_sha256:"a".repeat(64),data:serde_json::json!({})} }
#[test] fn none_valid_both(){assert!(EngineeringOptionalObject{inner:None}.validate().is_ok());assert!(NeutralOptionalObject{inner:None}.validate().is_ok());}
#[test] fn some_valid_both(){assert!(EngineeringOptionalObject{inner:Some(valid())}.validate().is_ok());assert!(NeutralOptionalObject{inner:Some(valid())}.validate().is_ok());}
#[test] fn some_invalid_both(){let mut v=valid();v.schema.clear();assert!(EngineeringOptionalObject{inner:Some(v.clone())}.validate().is_err());assert!(NeutralOptionalObject{inner:Some(v)}.validate().is_err());}
#[test] fn required_valid_and_invalid_both(){assert!(EngineeringRequiredObject{inner:valid()}.validate().is_ok());assert!(NeutralRequiredObject{inner:valid()}.validate().is_ok());let mut v=valid();v.source_sha256="not hash".into();assert!(EngineeringRequiredObject{inner:v.clone()}.validate().is_err());assert!(NeutralRequiredObject{inner:v}.validate().is_err());}
#[test] fn required_missing_denied_both(){assert!(serde_json::from_str::<EngineeringRequiredObject>("{}").is_err());assert!(serde_json::from_str::<NeutralRequiredObject>("{}").is_err());}
#[test] fn optional_null_missing_equivalent_both(){for raw in ["{}","{\"inner\":null}"]{assert!(serde_json::from_str::<EngineeringOptionalObject>(raw).unwrap().inner.is_none());assert!(serde_json::from_str::<NeutralOptionalObject>(raw).unwrap().inner.is_none());}}
"##).unwrap();
    let result = std::process::Command::new("rustup")
        .args([
            "run",
            "1.95.0",
            "cargo",
            "test",
            "--locked",
            "--offline",
            "-j4",
            "--target-dir",
        ])
        .arg(receiver.join("target"))
        .env("CARGO_TARGET_DIR", receiver.join("var/receiver-target"))
        .current_dir(&receiver)
        .output()
        .unwrap();
    std::fs::write(receiver.join("execution.stdout"), &result.stdout).unwrap();
    std::fs::write(receiver.join("execution.stderr"), &result.stderr).unwrap();
    std::fs::write(
        receiver.join("execution.exit"),
        result.status.code().unwrap_or(-1).to_string(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
