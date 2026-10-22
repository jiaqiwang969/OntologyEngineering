use anyhow::Result;
use oe_core::json::strict_json;
use oe_core::{SkillRoot, sha256, write_new};
use oe_engineering::harness::{
    self, ReviewExecutor, consolidate, load_step, step, step_with_reviewer,
};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static COUNT: AtomicU64 = AtomicU64::new(0);

fn installed() -> SkillRoot {
    SkillRoot::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    )
    .unwrap()
}
struct Fixture {
    root: SkillRoot,
}
impl Fixture {
    fn new() -> Self {
        let path = installed()
            .new_private_dir(Path::new(&format!(
                "var/tests/unified-system/rust-tests/harness-{}-{}",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::Relaxed)
            )))
            .unwrap();
        fs::write(path.join("SKILL.md"), "fixture").unwrap();
        Self {
            root: SkillRoot::new(path).unwrap(),
        }
    }
    fn output(&self, name: &str) -> PathBuf {
        self.root.path().join("var").join(name)
    }
    fn evidence(&self, name: &str, bytes: &[u8]) -> Value {
        let path = self.output(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        reference(&path)
    }
    fn first(&self) -> Value {
        let mut document = local_checkpoint_document();
        document["harness"]["checkpoint_policy"]["schema"] =
            json!("oev2.harness-checkpoint-policy/2");
        step(&self.root, &document, &self.output("first")).unwrap();
        reference(&self.output("first/iteration.json"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.root.path());
    }
}

fn reference(path: &Path) -> Value {
    json!({"path":path, "sha256":sha256(fs::read(path).unwrap())})
}

fn read(path: &Path) -> Value {
    strict_json(&fs::read(path).unwrap()).unwrap()
}

fn document() -> Value {
    json!({"schema":"ontology-engineering.context-input/v1", "task_id":"understand",
        "context":"现场条件未确定，先理解目标和问题。", "request":"分析下一步需要的能力。",
        "harness":{"project_id":"test-project", "decision_id":"decision-1", "object_id":"part-A",
            "revision":"r1", "problem_class":"condition-dependent-function", "goal":"理解功能与现场条件",
            "conditions":[], "constraints":["暂不运行仿真"], "unknowns":["工况未确定"],
            "semantic_modules":[], "review_requests":[], "observations":[]}})
}

fn local_checkpoint_document() -> Value {
    let mut value = document();
    value["schema"] = json!(harness::CHECKPOINT_INPUT_SCHEMA);
    value["harness"]["checkpoint_policy"] = json!({"schema":"oev2.harness-checkpoint-policy/1",
        "mode":"verified_no_model","byte_budget":16777216,"reason":"完整记录当前检查，尚无模型判断"});
    value
}

#[test]
fn checkpoint_roundtrip_uses_explicit_external_project_data_without_new_authority() {
    let software = Fixture::new();
    let data = Fixture::new();
    // Project storage is deliberately not a second Skill installation.
    fs::remove_file(data.root.path().join("SKILL.md")).unwrap();
    let root = software
        .root
        .clone()
        .with_project_data_root(data.root.path())
        .unwrap();
    let observation = b"Current condition remains unknown; no execution is authorized.";
    fs::write(data.root.path().join("observation.txt"), observation).unwrap();
    let mut input = local_checkpoint_document();
    input["harness"]["observations"] = json!([{
        "kind":"reported", "statement":"Preserve the unresolved condition",
        "evidence":[{"path":"observation.txt", "sha256":sha256(observation)}]
    }]);
    let result = step(&root, &input, Path::new("runs/first")).unwrap();
    assert_eq!(root.path(), software.root.path());
    assert_eq!(result["model_execution"], "not_run");
    assert_eq!(result["engineering_acceptance"], "not_determined");
    let saved = data.root.path().join("runs/first/iteration.json");
    let mut saved_ref = reference(&saved);
    saved_ref["path"] = json!("runs/first/iteration.json");
    assert_eq!(load_step(&root, &saved_ref).unwrap(), result);
    assert_eq!(
        fs::read(
            data.root
                .path()
                .join("runs/first/observations")
                .join(sha256(observation))
        )
        .unwrap(),
        observation
    );
    assert!(!software.root.path().join("runs").exists());
    assert!(load_step(&software.root, &reference(&saved)).is_err());
    let next = root
        .clone()
        .with_project_data_root(software.output("other-project"));
    assert!(
        next.is_err(),
        "missing stores are not implicitly initialized"
    );
}

fn checkpoint(reference: Value) -> Value {
    json!({"schema":harness::CONSOLIDATION_INPUT_SCHEMA, "checkpoint_id":"checkpoint-1",
        "problem_class":"condition-dependent-function", "reason":"阶段检查：工况未知造成不同判断",
        "iterations":[reference], "candidates":[{"id":"gap-1", "proposal":"核对条件不明时的功能主张表达是否充分",
            "applicability":["同类功能与条件问题"], "counterexamples":["缺证并非功能失败"], "open_questions":["是否只是项目缺证"]}]})
}

fn archive_checkpoint(reference: Value) -> Value {
    let mut doc = checkpoint(reference);
    doc["schema"] = json!("oev2.consolidation-input/3");
    doc["candidates"] = json!([]);
    doc["archive_policy"] = json!({"schema":"oev2.consolidation-archive-policy/1",
        "mode":"verified_empty_checkpoint","byte_budget":16777216});
    doc
}

fn json_value_nodes(value: &Value) -> usize {
    1 + match value {
        Value::Array(items) => items.iter().map(json_value_nodes).sum::<usize>(),
        Value::Object(items) => items.values().map(json_value_nodes).sum::<usize>(),
        _ => 0,
    }
}

fn review_input(f: &Fixture, facts: Value, sources: Value) -> Value {
    let record = json!({"claim":{"subject_revision":"r1"}, "facts":facts, "sources":sources});
    let reference = f.evidence("record.json", &serde_json::to_vec(&record).unwrap());
    json!({"id":"check", "record":reference["path"], "record_sha256":reference["sha256"], "evidence_root":f.output("evidence")})
}

struct Review {
    calls: Cell<usize>,
    fail: bool,
}
impl Review {
    fn new(fail: bool) -> Self {
        Self {
            calls: Cell::new(0),
            fail,
        }
    }
}
impl ReviewExecutor for Review {
    fn execute(
        &self,
        root: &Path,
        requests: &Value,
        _project_id: &str,
        output: &Path,
    ) -> Result<Value> {
        self.calls.set(self.calls.get() + 1);
        let root = SkillRoot::new(root)?;
        let path = root.new_private_dir(output)?;
        write_new(
            &path.join("original.json"),
            &json!({"requests":requests, "status":"blocked", "execution_kind":"fixture",
            "findings":[{"reason":"identity_mismatch"}], "violations":[]}),
        )?;
        if self.fail {
            anyhow::bail!("fixture_native_unavailable");
        }
        Ok(
            json!({"schema":"ontology-engineering.semantic-feedback/v1", "status":"completed", "execution_kind":"fixture",
            "reviews":[{"review":{"status":"blocked", "findings":[{"reason":"identity_mismatch"}], "violations":[]}}]}),
        )
    }
}

#[test]
fn checkpoint_exact_stored_bytes_bound_terminal_and_reject_credentials() {
    let f = Fixture::new();
    let mut value = local_checkpoint_document();
    fn size(f: &Fixture, name: &str) -> u64 {
        let p = f.output(&format!("{name}/iteration.json"));
        let v = read(&p);
        fs::metadata(&p).unwrap().len()
            + v["artifacts"]
                .as_object()
                .unwrap()
                .keys()
                .map(|r| fs::metadata(f.output(name).join(r)).unwrap().len())
                .sum::<u64>()
    }
    step(&f.root, &value, &f.output("size1")).unwrap();
    // Changing budget digit count changes the complete archive size; calibrate
    // once, then freeze an equal-width budget for exact B and B-1 checks.
    value["harness"]["checkpoint_policy"]["byte_budget"] = json!(size(&f, "size1"));
    step(&f.root, &value, &f.output("size2")).unwrap();
    let exact = size(&f, "size2");
    value["harness"]["checkpoint_policy"]["byte_budget"] = json!(exact);
    step(&f.root, &value, &f.output("exact")).unwrap();
    assert_eq!(size(&f, "exact"), exact);
    load_step(&f.root, &reference(&f.output("exact/iteration.json"))).unwrap();
    value["harness"]["checkpoint_policy"]["byte_budget"] = json!(exact - 1);
    assert!(
        step(&f.root, &value, &f.output("below"))
            .unwrap_err()
            .to_string()
            .contains("checkpoint_byte_budget")
    );
    assert!(!f.output("below/iteration.json").exists());
    let mut secret = local_checkpoint_document();
    secret["harness"]["checkpoint_policy"]["reason"] =
        json!(format!("{}{}", "api", "key_synthetic-not-real"));
    assert!(step(&f.root, &secret, &f.output("credential")).is_err());
    let escaped = br#"{"marker":"api\u006bey_synthetic-not-real"}"#;
    let evidence = f.evidence("escaped.json", escaped);
    let mut source = local_checkpoint_document();
    source["harness"]["observations"] =
        json!([{"kind":"observed","statement":"escape counterexample","evidence":[evidence]}]);
    assert!(step(&f.root, &source, &f.output("escaped")).is_err());
}

#[test]
fn checkpoint_rejects_resource_policy_and_previous_summary_forgery() {
    let f = Fixture::new();
    for budget in [0, 16777217] {
        let mut v = local_checkpoint_document();
        v["harness"]["checkpoint_policy"]["byte_budget"] = json!(budget);
        assert!(step(&f.root, &v, &f.output(&format!("budget-{budget}"))).is_err());
    }
    let mut huge = local_checkpoint_document();
    huge["context"] = json!("x".repeat(1048576));
    assert!(step(&f.root, &huge, &f.output("document")).is_err());
    let mut nodes = local_checkpoint_document();
    nodes["context"] = json!(vec![Value::Null; 200000]);
    assert!(step(&f.root, &nodes, &f.output("nodes")).is_err());
    let mut deep = Value::Null;
    for _ in 0..97 {
        deep = json!([deep]);
    }
    let mut depth = local_checkpoint_document();
    depth["context"] = deep;
    assert!(step(&f.root, &depth, &f.output("depth")).is_err());
    let result = step(&f.root, &local_checkpoint_document(), &f.output("original")).unwrap();
    let mut forged = result;
    forged["current_project_check"] = json!({"conclusion":"pass"});
    fs::write(
        f.output("original/iteration.json"),
        serde_json::to_vec_pretty(&forged).unwrap(),
    )
    .unwrap();
    assert!(load_step(&f.root, &reference(&f.output("original/iteration.json"))).is_err());
}

#[test]
fn checkpoint_rejects_json_looking_opaque_sources_beyond_parser_depth() {
    let f = Fixture::new();
    let raw = format!("{}null{}", "[".repeat(130), "]".repeat(130));
    assert!(strict_json(raw.as_bytes()).is_err());
    let source = f.evidence("parser-depth.json", raw.as_bytes());
    let mut document = local_checkpoint_document();
    document["harness"]["observations"] = json!([{"kind":"observed","statement":"Bounded JSON source counterexample","evidence":[source]}]);
    assert!(step(&f.root, &document, &f.output("parser-depth-checkpoint")).is_err());
    assert!(!f.output("parser-depth-checkpoint/iteration.json").exists());
}

#[test]
fn checkpoint_node_expansion_preserves_original_limit_and_baseline_reason() {
    let f = Fixture::new();
    let raw = serde_json::to_vec(&json!(vec![Value::Null; 220000])).unwrap();
    let source = f.evidence("many-nodes.json", &raw);
    let mut value = local_checkpoint_document();
    value["harness"]["observations"] = json!([{"kind":"observed","statement":"Complete structured source retained","evidence":[source.clone()]}]);
    assert!(
        step(&f.root, &value, &f.output("original-limit"))
            .unwrap_err()
            .to_string()
            .contains("structure_budget")
    );
    value["harness"]["checkpoint_policy"]["schema"] = json!("oev2.harness-checkpoint-policy/2");
    let result = step(&f.root, &value, &f.output("larger-limit")).unwrap();
    assert_eq!(result["model_execution"], "not_run");
    assert_eq!(
        fs::read(
            f.output("larger-limit/observations")
                .join(source["sha256"].as_str().unwrap())
        )
        .unwrap(),
        raw
    );
    load_step(
        &f.root,
        &reference(&f.output("larger-limit/iteration.json")),
    )
    .unwrap();
    step(
        &f.root,
        &local_checkpoint_document(),
        &f.output("baseline-old"),
    )
    .unwrap();
    let mut next = local_checkpoint_document();
    next["harness"]["previous"] = reference(&f.output("baseline-old/iteration.json"));
    next["harness"]["checkpoint_policy"]["schema"] = json!("oev2.harness-checkpoint-policy/2");
    assert!(step(&f.root, &next, &f.output("no-reason")).is_err());
    next["harness"]["change_reason"] =
        json!("Explicit resource protocol2; original1 meaning unchanged");
    step(&f.root, &next, &f.output("new-baseline")).unwrap();
}

#[test]
fn same_named_current_condition_change_blocks_adoption_and_retains_review_diagnostics() {
    let f = Fixture::new();
    let project: oe_kernel::project::Project = serde_json::from_value(json!({
        "schema":"oev2.project/1","project_id":"test-project","revision":"p1","purpose":"diagnostic",
        "objects":{"part-A":"r1"},"conditions":{"fixed":"load=10 N; support=cantilever"},
        "evidence":{"input":"a".repeat(64)},"decisions":[],"dependencies":{}
    })).unwrap();
    let object = oe_contracts::ObjectIdentity {
        object_id: "part-A".into(),
        revision: "r1".into(),
        condition_scope: "fixed".into(),
        source_sha256: "a".repeat(64),
    };
    let context = oe_kernel::project::context(&project, &object).unwrap();
    let mut changed = project.clone();
    changed
        .conditions
        .insert("fixed".into(), "load=20 N; support=cantilever".into());
    for (name, value, expected) in [("before", project, "pass"), ("after", changed, "violated")] {
        let mut doc = local_checkpoint_document();
        doc["harness"]["current_project"] = f.evidence(
            &format!("{name}-project.json"),
            &serde_json::to_vec(&value).unwrap(),
        );
        doc["harness"]["project_context"] = json!(context);
        doc["harness"]["conditions"] = json!([value.conditions["fixed"]]);
        doc["harness"]["review_requests"] = json!([review_input(&f, json!({}), json!([]))]);
        let reviewer = Review::new(false);
        let result = step_with_reviewer(&f.root, &doc, &f.output(name), &reviewer).unwrap();
        assert_eq!(result["current_project_check"]["conclusion"], expected);
        assert_eq!(
            reviewer.calls.get(),
            1,
            "original review must execute independently of current context"
        );
        assert_eq!(
            result["feedback"]["reviews"][0]["review"]["findings"][0]["reason"],
            "identity_mismatch"
        );
        assert_eq!(result["engineering_acceptance"], "not_determined");
        if expected == "violated" {
            assert_eq!(result["feedback"]["dependent_adoption"], "blocked");
        }
    }
}

#[test]
fn fresh_content_binding_keeps_model_mismatch_in_independent_reviews() {
    let f = Fixture::new();
    let quantity = |v, unit| json!({"value":v,"unit":unit,"precision":"synthetic exact input"});
    let facts: Vec<_> = [
        ("force", quantity(20.0, "N")), ("span", quantity(100.0, "mm")),
        ("elastic_modulus", quantity(200000.0, "MPa")), ("second_moment", quantity(1000.0, "mm4")),
        ("support", json!("cantilever_fixed_free")), ("load_position", json!("end")),
        ("regime", json!("linear_elastic_small_deflection")),
    ].into_iter().map(|(property,value)| json!({"property":property,"value":value,"status":"declared","source_refs":["input"]})).collect();
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"part-A","object_revision":"r1","purpose":"diagnostic","acceptance":["deflection limit"],"facts":facts,
        "assertions":[{"assertion_id":"beam","kind":"model_boundary","subject":"beam","requirements":{"method":"beam-cantilever-end-load/1","inputs":{"force":quantity(10.0,"N"),"span":quantity(100.0,"mm"),"elastic_modulus":quantity(200000.0,"MPa"),"second_moment":quantity(1000.0,"mm4")}},"source_refs":["input"],"affected_decisions":[]}]});
    let condition = serde_json::to_string(&basis).unwrap();
    let project: oe_kernel::project::Project = serde_json::from_value(json!({
        "schema":"oev2.project/1","project_id":"test-project","revision":"p1","purpose":"diagnostic",
        "objects":{"part-A":"r1"},"conditions":{"fixed":condition},"evidence":{"input":"a".repeat(64)},"decisions":[],"dependencies":{}
    })).unwrap();
    let object = oe_contracts::ObjectIdentity {
        object_id: "part-A".into(),
        revision: "r1".into(),
        condition_scope: "fixed".into(),
        source_sha256: "a".repeat(64),
    };
    let mut doc = local_checkpoint_document();
    doc["harness"]["current_project"] =
        f.evidence("current.json", &serde_json::to_vec(&project).unwrap());
    doc["harness"]["project_context"] =
        json!(oe_kernel::project::context(&project, &object).unwrap());
    doc["harness"]["conditions"] = json!([condition]);
    doc["harness"]["review_requests"] = json!([review_input(&f, json!({}), json!([]))]);
    let reviewer = Review::new(false);

    let result = step_with_reviewer(&f.root, &doc, &f.output("premise"), &reviewer).unwrap();
    assert_eq!(result["current_project_check"]["conclusion"], "pass");
    assert_eq!(result["engineering_checks"][1]["conclusion"], "violated");
    assert_eq!(
        result["engineering_checks"][1]["result"]["details"]["mismatches"][0]["property"],
        "force"
    );
    assert_eq!(result["feedback"]["dependent_adoption"], "blocked");
    assert_eq!(reviewer.calls.get(), 1, "original review still runs");
    assert_eq!(
        result["feedback"]["reviews"][0]["review"]["findings"][0]["reason"],
        "identity_mismatch"
    );
    assert_eq!(
        load_step(&f.root, &reference(&f.output("premise/iteration.json"))).unwrap(),
        result
    );
}

#[test]
fn empty_archive_preserves_complete_bytes_and_old_budget_rejections() {
    let f = Fixture::new();
    let source = f.evidence(
        "large-observation.txt",
        "failed partial; no saved drawing. "
            .repeat(20_000)
            .as_bytes(),
    );
    let mut input = local_checkpoint_document();
    input["harness"]["observations"] = json!([{"kind":"reported","statement":"All original failed observation bytes must survive.","evidence":[source]}]);
    step(&f.root, &input, &f.output("large")).unwrap();
    let prior = reference(&f.output("large/iteration.json"));

    let mut legacy = checkpoint(prior.clone());
    legacy["candidates"] = json!([]);
    for protocol in [1, 2] {
        let mut old = legacy.clone();
        if protocol == 2 {
            old["schema"] = json!("oev2.consolidation-input/2");
            old["context_policy"] = json!({"schema":"oev2.consolidation-context-policy/1","mode":"lossless_shared_text","byte_budget":512000});
        }
        let output = f.output(&format!("old-{protocol}"));
        assert!(
            consolidate(&f.root, &old, &output)
                .unwrap_err()
                .to_string()
                .contains("verified_archive_input_required")
        );
        assert!(!output.exists());
    }
    let archive = archive_checkpoint(prior.clone());
    let result = consolidate(&f.root, &archive, &f.output("archive")).unwrap();
    assert_eq!(result["status"], "no_delta");
    assert_eq!(result["authority"], "author_declaration_only");
    assert_eq!(
        result["historical_checks"],
        "historical_checks_not_reexecuted"
    );
    assert_eq!(result["execution_kind"], "not_run");
    assert_eq!(result["request_bytes"], 0);
    assert_eq!(result["ontology_promotion"], "not_performed");
    assert_eq!(result["engineering_acceptance"], "not_determined");
    assert!(!f.output("archive/request.json").exists());
    let manifest = read(&f.output("archive/archive-manifest.json"));
    let mut original_bytes = 0_u64;
    for entry in manifest["files"].as_array().unwrap() {
        let original = fs::read(entry["source"].as_str().unwrap()).unwrap();
        let copied = fs::read(
            f.output("archive")
                .join(entry["archived_path"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(copied, original);
        assert_eq!(entry["sha256"], sha256(&original));
        original_bytes += original.len() as u64;
    }
    assert!(original_bytes > 512000);
    assert_eq!(manifest["total_original_bytes"], original_bytes);
    assert_eq!(
        fs::read(f.output("large/iteration.json")).unwrap(),
        fs::read(f.output("archive/trajectories/00/iteration.json")).unwrap()
    );
    let mut exact = archive.clone();
    exact["archive_policy"]["byte_budget"] = json!(original_bytes);
    consolidate(&f.root, &exact, &f.output("exact-source-budget")).unwrap();
    exact["archive_policy"]["byte_budget"] = json!(original_bytes - 1);
    assert!(
        consolidate(&f.root, &exact, &f.output("one-byte-short"))
            .unwrap_err()
            .to_string()
            .contains("consolidation_archive_byte_budget_exceeded")
    );
    assert!(!f.output("one-byte-short").exists());

    assert_eq!(reference(&f.output("large/iteration.json")), prior);
}

#[test]
fn empty_archive_rejects_candidates_tampering_and_resource_escapes() {
    for variant in [
        "candidate",
        "mixed",
        "zero",
        "above-limit",
        "document",
        "document-credential",
        "duplicate",
        "trajectory-count",
        "problem-class",
        "tamper",
        "summary",
        "credential",
        "escaped-credential",
        "artifact-count",
        "nodes",
        "depth",
    ] {
        let f = Fixture::new();
        let prior = f.first();
        let mut input = archive_checkpoint(prior);
        let expected = match variant {
            "candidate" => {
                input["candidates"] = json!([{"id":"cannot_bypass_model_budget"}]);
                "consolidation_archive_requires_empty_candidates"
            }
            "mixed" => {
                input["context_policy"] = json!({});
                "invalid_consolidation_archive_input"
            }
            "zero" => {
                input["archive_policy"]["byte_budget"] = json!(0);
                "invalid_consolidation_archive_policy"
            }
            "above-limit" => {
                input["archive_policy"]["byte_budget"] = json!(16777217);
                "invalid_consolidation_archive_policy"
            }
            "document" => {
                input["reason"] = json!("r".repeat(1048576));
                "consolidation_archive_document_budget_exceeded"
            }
            "document-credential" => {
                input["reason"] = json!(["api", "key_synthetic"].concat());
                "credential_material_in_trajectory"
            }
            "duplicate" => {
                input["iterations"] = json!([input["iterations"][0], input["iterations"][0]]);
                "duplicate_consolidation_episode"
            }
            "trajectory-count" => {
                input["iterations"] = json!(vec![input["iterations"][0].clone(); 17]);
                "consolidation_archive_trajectory_budget_exceeded"
            }
            "problem-class" => {
                input["problem_class"] = json!("other-class");
                "consolidation_problem_class_mismatch"
            }
            "tamper" => {
                fs::write(f.output("first/feedback.json"), b"{}").unwrap();
                "harness_artifact_changed"
            }
            "summary" => {
                let path = f.output("first/iteration.json");
                let mut value = read(&path);
                value["status"] = json!("complete");
                fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
                input["iterations"] = json!([reference(&path)]);
                "harness_checkpoint_authority_mismatch"
            }
            "artifact-count" => {
                let path = f.output("first/iteration.json");
                let mut value = read(&path);
                for i in 0..257 {
                    value["artifacts"][format!("extra-{i}")] = json!("0".repeat(64));
                }
                fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
                input["iterations"] = json!([reference(&path)]);
                "consolidation_archive_artifact_budget_exceeded"
            }
            mode => {
                let data = match mode {
                    "credential" => {
                        [["api", "key_"].concat().as_bytes(), b"synthetic-marker"].concat()
                    }
                    "escaped-credential" => br#"{"fixture":"api\u006bey_synthetic"}"#.to_vec(),
                    "nodes" => serde_json::to_vec(&vec![0; 200001]).unwrap(),
                    "depth" => {
                        let mut v = json!(0);
                        for _ in 0..97 {
                            v = json!([v]);
                        }
                        serde_json::to_vec(&v).unwrap()
                    }
                    _ => unreachable!(),
                };
                let path = f.output("first/extra.json");
                fs::write(&path, &data).unwrap();
                let step_path = f.output("first/iteration.json");
                let mut value = read(&step_path);
                value["artifacts"]["extra.json"] = json!(sha256(&data));
                fs::write(&step_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
                input["iterations"] = json!([reference(&step_path)]);
                if mode == "credential" || mode == "escaped-credential" {
                    "credential_material_in_trajectory"
                } else {
                    "consolidation_archive_structure_budget_exceeded"
                }
            }
        };

        let output = f.output("refused");
        let error = consolidate(&f.root, &input, &output).unwrap_err();
        assert!(error.to_string().contains(expected), "{variant}: {error:#}");
        assert!(!output.exists(), "{variant}");
    }
}

#[test]
fn empty_archive_counts_json_independently_of_artifact_name() {
    for name in ["large.json", "8a35bc4c2adac7c5197414967748e3ab"] {
        let f = Fixture::new();
        f.first();
        let data = serde_json::to_vec(&vec![0; 200001]).unwrap();
        fs::write(f.output("first").join(name), &data).unwrap();
        let step_path = f.output("first/iteration.json");
        let mut iteration = read(&step_path);
        iteration["artifacts"][name] = json!(sha256(&data));
        fs::write(&step_path, serde_json::to_vec_pretty(&iteration).unwrap()).unwrap();
        let prior = reference(&step_path);
        load_step(&f.root, &prior).unwrap();
        let input = archive_checkpoint(prior);
        let error = consolidate(&f.root, &input, &f.output("refused")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("consolidation_archive_structure_budget_exceeded"),
            "{name}: {error:#}"
        );
        assert!(!f.output("refused").exists());
    }
}

#[test]
fn empty_archive_explicit_node_capacity_preserves_old_limit() {
    for (version, cap) in [(1, 200_000_usize), (2, 524_288)] {
        for extra in [0, 1] {
            let f = Fixture::new();
            f.first();
            let path = f.output("first/iteration.json");
            let mut iteration = read(&path);
            let name = "a024f159e056c7254d4360a1e8e3c4ff";
            iteration["artifacts"][name] = json!(sha256(b"[]"));
            fs::write(f.output("first").join(name), b"[]").unwrap();
            fs::write(&path, serde_json::to_vec_pretty(&iteration).unwrap()).unwrap();
            let mut input = archive_checkpoint(reference(&path));
            input["archive_policy"]["schema"] =
                json!(format!("oev2.consolidation-archive-policy/{version}"));
            let base = json_value_nodes(&input)
                + json_value_nodes(&iteration)
                + iteration["artifacts"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(|name| {
                        strict_json(&fs::read(f.output("first").join(name)).unwrap())
                            .ok()
                            .as_ref()
                            .map(json_value_nodes)
                            .unwrap_or(0)
                    })
                    .sum::<usize>();
            let data = serde_json::to_vec(&vec![0; cap + extra - base]).unwrap();
            fs::write(f.output("first").join(name), &data).unwrap();
            iteration["artifacts"][name] = json!(sha256(&data));
            fs::write(&path, serde_json::to_vec_pretty(&iteration).unwrap()).unwrap();
            input["iterations"] = json!([reference(&path)]);

            let result = consolidate(&f.root, &input, &f.output("archive"));
            if extra == 1 {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("consolidation_archive_structure_budget_exceeded")
                );
                assert!(!f.output("archive").exists());
            } else {
                let receipt = result.unwrap();
                assert_eq!(receipt["status"], "no_delta");
                assert_eq!(receipt["model_calls"], 0);
                assert_eq!(
                    read(&f.output("archive/archive-manifest.json"))["json_nodes"],
                    cap
                );
                assert_eq!(
                    fs::read(f.output("archive/trajectories/00").join(name)).unwrap(),
                    data
                );
                if version == 2 {
                    input["archive_policy"]["schema"] =
                        json!("oev2.consolidation-archive-policy/1");
                    assert!(
                        consolidate(&f.root, &input, &f.output("old-refused"))
                            .unwrap_err()
                            .to_string()
                            .contains("consolidation_archive_structure_budget_exceeded")
                    );
                }
            }
        }
    }
}

#[test]
fn empty_archive_extensionless_json_cannot_escape_parsing_or_credentials() {
    let deep = ["[".repeat(140), "0".to_owned(), "]".repeat(140)].concat();
    for (bytes, expected) in [
        (deep.as_bytes(), "harness_checkpoint_json_source_invalid"),
        (
            br#"{"synthetic":"api\u006bey_synthetic"}"#.as_slice(),
            "credential_material_in_trajectory",
        ),
    ] {
        let f = Fixture::new();
        f.first();
        let name = sha256(bytes);
        fs::write(f.output("first").join(&name), bytes).unwrap();
        let path = f.output("first/iteration.json");
        let mut value = read(&path);
        value["artifacts"][&name] = json!(&name);
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let input = archive_checkpoint(reference(&path));
        assert!(
            consolidate(&f.root, &input, &f.output("refused"))
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
        assert!(!f.output("refused").exists());
    }
}
