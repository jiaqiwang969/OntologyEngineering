//! Obligations: who executes a constraint, at which entry, by which run evidence. Synthetic contract types and
//! run documents on top of the composition foundation fixture; no engineering content.
use oe_design::obligations::{Evidence, Executor, Obligation, evaluate, untyped};
use oe_design::{Design, Field, Type};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_owned()
}

fn foundation() -> Design {
    oe_design::load(&manifest().join("tests/fixtures/composition-r2-foundation.toml")).unwrap()
}

fn field(name: &str, rust_type: &str) -> Field {
    Field {
        name: name.into(),
        meaning: format!("Synthetic run report field {name}"),
        rust_type: rust_type.into(),
        required: true,
        format: "typed".into(),
        unit: "none".into(),
        precision: "exact".into(),
        write: "synthetic test".into(),
        applicability: "synthetic obligation tests only".into(),
        acceptance: "synthetic test".into(),
        allowed_values: vec![],
    }
}

fn contract(name: &str, fields: Vec<Field>) -> Type {
    Type {
        name: name.into(),
        contract_group: "engineering".into(),
        meaning: format!("Synthetic {name} for obligation tests"),
        identity: "Synthetic".into(),
        fields,
    }
}

fn evidence(pointer: &str, op: &str, value: Option<Value>) -> Evidence {
    Evidence {
        contract: "ObligationTestReport".into(),
        schema: "obligation-test.report/1".into(),
        pointer: pointer.into(),
        op: op.into(),
        value,
        min_documents: 1,
    }
}

fn executor(kind: &str, evidence: Vec<Evidence>) -> Executor {
    Executor {
        kind: kind.into(),
        module: "design".into(),
        rust_path: Some("crates/oe-design/src/obligations.rs".into()),
        symbol: Some("evaluate".into()),
        entry: "synthetic entry".into(),
        evidence,
    }
}

/// The foundation plus two synthetic contracts and two obligations citing different values.
fn design() -> Design {
    let mut d = foundation();
    let constraint = d
        .constraints
        .iter()
        .find(|c| c.module == "design")
        .unwrap()
        .id
        .clone();
    d.types.push(contract(
        "ObligationTestStep",
        vec![field("id", "String"), field("status", "String")],
    ));
    d.types.push(contract(
        "ObligationTestReport",
        vec![
            field("schema", "String"),
            field("status", "String"),
            field("counts", "std::collections::BTreeMap<String, u64>"),
            field("steps", "Vec<ObligationTestStep>"),
            field("facts", "serde_json::Value"),
        ],
    ));
    d.obligations.push(Obligation {
        id: "ob_test_status".into(),
        constraint: constraint.clone(),
        statement: "The synthetic report passed".into(),
        executors: vec![executor(
            "independent_check",
            vec![evidence("/status", "equals", Some(json!("passed")))],
        )],
        gaps: vec![],
    });
    d.obligations.push(Obligation {
        id: "ob_test_steps".into(),
        constraint,
        statement: "The synthetic second step passed and was counted".into(),
        executors: vec![
            executor(
                "runtime_receipt",
                vec![
                    evidence("/steps/1/status", "equals", Some(json!("passed"))),
                    evidence("/counts/second step", "at_least", Some(json!(1))),
                ],
            ),
            Executor {
                kind: "qualification".into(),
                module: "design".into(),
                rust_path: None,
                symbol: None,
                entry: "repeat on a development case".into(),
                evidence: vec![],
            },
        ],
        gaps: vec!["Repetition across runs is not checked within one run".into()],
    });
    d
}

fn report(status: &str, second: &str, count: u64) -> Value {
    json!({"schema":"obligation-test.report/1","status":status,"counts":{"second step":count},
        "steps":[{"id":"first","status":"passed"},{"id":"second","status":second}],"facts":{"any":{"thing":true}}})
}

fn run(documents: &[(&str, Value)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, value) in documents {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
    std::fs::write(dir.path().join("not-json.json"), b"{ not json").unwrap();
    std::fs::write(
        dir.path().join("no-schema.json"),
        b"{\"status\":\"failed\"}",
    )
    .unwrap();
    dir
}

fn statuses(report: &Value) -> Vec<(String, String)> {
    report["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["id"].as_str().unwrap().to_owned(),
                o["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn obligations_exist_only_in_fragment_four_under_the_extended_root() {
    let base = manifest().join("../../var");
    std::fs::create_dir_all(&base).unwrap();
    let foundation_bytes =
        std::fs::read(manifest().join("tests/fixtures/composition-r2-foundation.toml")).unwrap();
    let original = foundation();
    let constraint = original
        .constraints
        .iter()
        .find(|c| c.module == "design")
        .unwrap()
        .id
        .clone();
    let obligation = format!(
        "[[obligations]]\nid = \"ob_fragment\"\nconstraint = \"{constraint}\"\nstatement = \"Synthetic\"\n\n[[obligations.executors]]\nkind = \"engineering_review\"\nmodule = \"design\"\nentry = \"review\"\n"
    );
    for (root_schema, fragment_schema, ok, reason) in [
        ("oev2.design/5", "oev2.design-fragment/4", true, ""),
        (
            "oev2.design/5",
            "oev2.design-fragment/2",
            false,
            "obligations_require_fragment_schema_4",
        ),
        (
            "oev2.design/4",
            "oev2.design-fragment/4",
            false,
            "design_composition_nested_manifest_unsupported",
        ),
    ] {
        let tmp = tempfile::tempdir_in(base.canonicalize().unwrap()).unwrap();
        std::fs::write(tmp.path().join("foundation.toml"), &foundation_bytes).unwrap();
        let fragment = format!("schema = \"{fragment_schema}\"\n\n{obligation}");
        std::fs::write(tmp.path().join("extra.toml"), &fragment).unwrap();
        let root = format!(
            "schema={root_schema:?}\nversion={}\nproblem={}\npurpose={}\nvalidation_encoding=\"oev2.validation-groups/1\"\n[[fragments]]\npath=\"foundation.toml\"\nsha256=\"{}\"\nformat=\"oev2.design/1\"\n\n[[fragments]]\npath=\"extra.toml\"\nsha256=\"{}\"\nformat=\"{fragment_schema}\"\n",
            serde_json::to_string(&original.version).unwrap(),
            serde_json::to_string(&original.problem).unwrap(),
            serde_json::to_string(&original.purpose).unwrap(),
            oe_core::sha256(&foundation_bytes),
            oe_core::sha256(fragment.as_bytes())
        );
        std::fs::write(tmp.path().join("root.toml"), root).unwrap();
        let loaded = oe_design::composition::load(tmp.path(), &tmp.path().join("root.toml"));
        match loaded {
            Ok(l) => {
                assert!(ok, "{root_schema} {fragment_schema} loaded");
                assert_eq!(l.design.obligations.len(), 1);
                assert_eq!(l.design.obligations[0].id, "ob_fragment");
            }
            Err(e) => assert!(
                !ok && format!("{e:#}").contains(reason),
                "{root_schema} {fragment_schema}: {e:#}"
            ),
        }
    }
    // A single oev2.design/1 file cannot carry obligations either.
    let tmp = tempfile::tempdir_in(base.canonicalize().unwrap()).unwrap();
    let mut text = String::from_utf8(foundation_bytes).unwrap();
    text.push_str(&format!("\n{obligation}"));
    std::fs::write(tmp.path().join("single.toml"), text).unwrap();
    let e = oe_design::load(&tmp.path().join("single.toml")).unwrap_err();
    assert!(
        format!("{e:#}").contains("obligations_require_fragment_schema_4"),
        "{e:#}"
    );
}

#[test]
fn a_design_without_obligations_keeps_its_normalized_bytes() {
    let d = foundation();
    assert!(d.obligations.is_empty());
    let value = serde_json::to_value(d.clone().normalize().unwrap()).unwrap();
    assert!(value.get("obligations").is_none());
    let mut with = design();
    with.obligations.clear();
    with.types.retain(|t| !t.name.starts_with("ObligationTest"));
    assert_eq!(with.canonical().unwrap(), d.canonical().unwrap());
    assert!(design().canonical().unwrap() != d.canonical().unwrap());
}

#[test]
fn each_malformed_obligation_is_refused_with_its_own_reason() {
    design().validate().unwrap();
    type Change = Box<dyn Fn(&mut Design)>;
    let cases: Vec<(&str, Change)> = vec![
        (
            "obligation_constraint_missing",
            Box::new(|d| d.obligations[0].constraint = "no_such_constraint".into()),
        ),
        (
            "obligation_id_invalid",
            Box::new(|d| d.obligations[1].id = "ob_test_status".into()),
        ),
        (
            "obligation_incomplete",
            Box::new(|d| d.obligations[0].executors.clear()),
        ),
        (
            "obligation_gap_empty",
            Box::new(|d| d.obligations[0].gaps = vec![" ".into()]),
        ),
        (
            "obligation_executor_kind",
            Box::new(|d| d.obligations[0].executors[0].kind = "hope".into()),
        ),
        (
            "obligation_executor_module_missing",
            Box::new(|d| d.obligations[0].executors[0].module = "nowhere".into()),
        ),
        (
            "obligation_executor_entry_missing",
            Box::new(|d| d.obligations[0].executors[0].entry = "".into()),
        ),
        (
            "obligation_executor_path_missing",
            Box::new(|d| d.obligations[0].executors[0].rust_path = None),
        ),
        (
            "obligation_executor_path_outside_module",
            Box::new(|d| {
                d.obligations[0].executors[0].rust_path = Some("crates/oe-core/src/lib.rs".into())
            }),
        ),
        (
            "obligation_executor_symbol_invalid",
            Box::new(|d| d.obligations[0].executors[0].symbol = Some("not an identifier".into())),
        ),
        (
            "obligation_generated_validation_type",
            Box::new(|d| {
                let e = &mut d.obligations[0].executors[0];
                e.kind = "generated_validation".into();
                e.rust_path = None;
                e.symbol = Some("NoSuchContract".into());
            }),
        ),
        (
            "obligation_outside_run_executor_carries_code_or_evidence",
            Box::new(|d| d.obligations[0].executors[0].kind = "engineering_review".into()),
        ),
        (
            "obligation_evidence_contract_missing",
            Box::new(|d| {
                d.obligations[0].executors[0].evidence[0].contract = "NoSuchContract".into()
            }),
        ),
        (
            "obligation_evidence_schema",
            Box::new(|d| d.obligations[0].executors[0].evidence[0].min_documents = 0),
        ),
        (
            "obligation_evidence_pointer_syntax",
            Box::new(|d| d.obligations[0].executors[0].evidence[0].pointer = "status".into()),
        ),
        (
            "obligation_evidence_op",
            Box::new(|d| d.obligations[0].executors[0].evidence[0].op = "matches".into()),
        ),
        (
            "obligation_evidence_value_missing",
            Box::new(|d| d.obligations[0].executors[0].evidence[0].value = None),
        ),
        (
            "obligation_evidence_value_not_a_number",
            Box::new(|d| {
                let v = &mut d.obligations[0].executors[0].evidence[0];
                v.op = "at_least".into();
                v.value = Some(json!("many"));
            }),
        ),
        (
            "obligation_evidence_value_unused",
            Box::new(|d| d.obligations[0].executors[0].evidence[0].op = "present".into()),
        ),
    ];
    for (reason, change) in cases {
        let mut d = design();
        change(&mut d);
        let e = d.validate().unwrap_err();
        assert!(format!("{e:#}").starts_with(reason), "{reason}: {e:#}");
    }
}

#[test]
fn evidence_pointers_resolve_through_declared_contract_fields() {
    let d = design();
    for typed in [
        "",
        "/status",
        "/steps/0/status",
        "/counts/any key",
        "/facts/a/b/c",
    ] {
        assert_eq!(untyped(&d, "ObligationTestReport", typed), None, "{typed}");
    }
    for wrong in [
        "/missing",
        "/steps/first/status",
        "/steps/0/missing",
        "/status/extra",
    ] {
        assert!(
            untyped(&d, "ObligationTestReport", wrong).is_some(),
            "{wrong}"
        );
    }
    assert!(untyped(&d, "NoSuchContract", "/status").is_some());
    assert_eq!(
        oe_design::obligations::pointer_tokens("/a~1b/c~0d"),
        Some(vec!["a/b".into(), "c~d".into()])
    );
    assert_eq!(oe_design::obligations::pointer_tokens("/bad~2escape"), None);
}

#[test]
fn a_run_is_judged_per_obligation_and_one_changed_value_fails_only_its_obligations() {
    let d = design();
    let held = run(&[("report.json", report("passed", "passed", 1))]);
    let r = evaluate(&d, held.path()).unwrap();
    assert_eq!(r["schema"], oe_design::obligations::REPORT_SCHEMA);
    assert_eq!(
        statuses(&r),
        [
            ("ob_test_status".into(), "held".into()),
            ("ob_test_steps".into(), "held".into())
        ]
    );
    assert_eq!(r["obligations"][1]["executors"][1]["status"], "outside_run");
    assert_eq!(r["documents"]["obligation-test.report/1"], 1);
    // One changed value fails only the obligation that cites it.
    let changed = run(&[("nested/report.json", report("passed", "failed", 1))]);
    assert_eq!(
        statuses(&evaluate(&d, changed.path()).unwrap()),
        [
            ("ob_test_status".into(), "held".into()),
            ("ob_test_steps".into(), "failed".into())
        ]
    );
    let changed = run(&[("report.json", report("failed", "passed", 1))]);
    assert_eq!(
        statuses(&evaluate(&d, changed.path()).unwrap()),
        [
            ("ob_test_status".into(), "failed".into()),
            ("ob_test_steps".into(), "held".into())
        ]
    );
    let changed = run(&[("report.json", report("passed", "passed", 0))]);
    let r = evaluate(&d, changed.path()).unwrap();
    assert_eq!(r["obligations"][1]["status"], "failed");
    assert_eq!(
        r["obligations"][1]["executors"][0]["evidence"][1]["misses"][0]["observed"],
        0
    );
    // Every document of a schema must satisfy the predicate.
    let mixed = run(&[
        ("a.json", report("passed", "passed", 1)),
        ("b.json", report("failed", "passed", 1)),
    ]);
    assert_eq!(
        evaluate(&d, mixed.path()).unwrap()["obligations"][0]["status"],
        "failed"
    );
    // Too few documents are absent, never held.
    let empty = run(&[]);
    assert_eq!(
        statuses(&evaluate(&d, empty.path()).unwrap()),
        [
            ("ob_test_status".into(), "absent".into()),
            ("ob_test_steps".into(), "absent".into())
        ]
    );
    let mut two = design();
    two.obligations[0].executors[0].evidence[0].min_documents = 2;
    assert_eq!(
        evaluate(&two, held.path()).unwrap()["obligations"][0]["status"],
        "absent"
    );
    // Only a review executor: nothing in a run can hold it.
    let mut review = design();
    review.obligations[0].executors = vec![Executor {
        kind: "engineering_review".into(),
        module: "design".into(),
        rust_path: None,
        symbol: None,
        entry: "review".into(),
        evidence: vec![],
    }];
    assert_eq!(
        evaluate(&review, held.path()).unwrap()["obligations"][0]["status"],
        "outside_run"
    );
}

#[test]
fn inspection_reports_missing_executors_and_untyped_evidence() {
    let root = manifest().join("../..").canonicalize().unwrap();
    let mut d = design();
    d.obligations[0].executors[0].symbol = Some("no_such_function_anywhere".into());
    d.obligations[1].executors[0].evidence[0].pointer = "/steps/1/missing".into();
    let v = oe_design::inspection::inspect(&root, &d).unwrap();
    let findings: Vec<(&str, &str)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["code"].as_str().unwrap(), f["subject"].as_str().unwrap()))
        .collect();
    assert!(
        findings.contains(&(
            "obligation_executor_missing",
            "obligation_ob_test_status_executor_00"
        )),
        "{findings:?}"
    );
    assert!(
        findings.contains(&(
            "obligation_evidence_untyped",
            "obligation_ob_test_steps_executor_00"
        )),
        "{findings:?}"
    );
    let constraint = d.obligations[0].constraint.clone();
    let traced = v["traceability"]["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == constraint.as_str())
        .unwrap()
        .clone();
    let obligations = traced["obligations"].as_array().unwrap();
    assert_eq!(obligations.len(), 2);
    assert_eq!(obligations[0]["executors"][0]["symbol_present"], false);
    assert_eq!(
        obligations[1]["executors"][0]["evidence"][0]["typed"],
        false
    );
    assert_eq!(obligations[1]["executors"][0]["evidence"][1]["typed"], true);
    assert_eq!(v["traceability"]["counts"]["obligations"], 2);
    // Correct declarations produce neither finding.
    let v = oe_design::inspection::inspect(&root, &design()).unwrap();
    assert!(
        !v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"].as_str().unwrap().starts_with("obligation_"))
    );
}
