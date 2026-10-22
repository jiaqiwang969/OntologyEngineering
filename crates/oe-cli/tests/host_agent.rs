//! Local calculation and host Agent exploration preserve evidence and applicability.
use serde_json::Value;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn invoke(root: &Path, arguments: &[&str]) -> std::process::Output {
    // The receipt can exceed a pipe's capacity. Capture to private temporary
    // files so the child can finish before we read, retaining the same deadline.
    let mut stdout = tempfile::tempfile().unwrap();
    let mut stderr = tempfile::tempfile().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .current_dir(root)
        .arg("--root")
        .arg(root)
        .args(arguments)
        .stdout(Stdio::from(stdout.try_clone().unwrap()))
        .stderr(Stdio::from(stderr.try_clone().unwrap()))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            let status = child.wait().unwrap();
            stdout.seek(SeekFrom::Start(0)).unwrap();
            stderr.seek(SeekFrom::Start(0)).unwrap();
            let mut out = Vec::new();
            let mut err = Vec::new();
            stdout.read_to_end(&mut out).unwrap();
            stderr.read_to_end(&mut err).unwrap();
            return std::process::Output {
                status,
                stdout: out,
                stderr: err,
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("retired entry waited for external input: {arguments:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn default_local_work_keeps_calculation_conditions_and_source_drift_checks_without_a_key() {
    use serde_json::json;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = {
        fs::create_dir_all(root.join("var/tests/unified-system/cli")).unwrap();
        tempfile::tempdir_in(root.join("var/tests/unified-system/cli")).unwrap()
    };
    let dir = temp.path();
    let save = |name: &str, value: &Value| {
        let bytes = serde_json::to_vec(value).unwrap();
        fs::write(dir.join(name), &bytes).unwrap();
        oe_core::sha256(bytes)
    };
    let source = b"Synthetic beam model, not field evidence";
    fs::write(dir.join("source.txt"), source).unwrap();
    let source_sha = oe_core::sha256(source);
    let object = json!({"object_id":"guide","revision":"r1","condition_scope":"study","source_sha256":source_sha});
    let q = |value: f64, unit: &str| json!({"value":value,"unit":unit,"precision":"declared synthetic input"});
    let inputs = json!({"force":q(10.0,"N"),"span":q(100.0,"mm"),"elastic_modulus":q(200000.0,"MPa"),"second_moment":q(1000.0,"mm4")});
    let mut facts: Vec<Value> = inputs.as_object().unwrap().iter().map(|(property,value)| json!({"property":property,"value":value,"status":"declared","source_refs":["mechanics-source"]})).collect();
    for (property, value) in [
        ("support", "cantilever_fixed_free"),
        ("load_position", "end"),
        ("regime", "linear_elastic_small_deflection"),
    ] {
        facts.push(json!({"property":property,"value":value,"status":"declared","source_refs":["mechanics-source"]}));
    }
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"guide","object_revision":"r1","purpose":"diagnose a declared beam model","acceptance":["Preserve load, units and boundary conditions; field acceptance remains unknown"],"facts":facts,"assertions":[{"assertion_id":"beam-boundary","kind":"model_boundary","subject":"beam","requirements":{"method":"beam-cantilever-end-load/1","inputs":inputs},"source_refs":["mechanics-source"],"affected_decisions":[]}]});
    let project = json!({"schema":"oev2.project/1","project_id":"host-local-fixture","revision":"p1","purpose":"diagnose a declared beam model","objects":{"guide":"r1"},"conditions":{"study":serde_json::to_string(&basis).unwrap()},"evidence":{"mechanics-source":source_sha},"decisions":[{"id":"beam-prediction","object_id":"guide","object_revision":"r1","condition_scope":"study","source_sha256":source_sha,"conclusion":"unknown","adopted":false,"basis":["mechanics-source"]}],"dependencies":{}});
    let project_sha = save("project.json", &project);
    save("object.json", &object);
    let project_path = dir.join("project.json");
    let object_path = dir.join("object.json");
    let context_path = dir.join("context.json");
    let out = invoke(
        &root,
        &[
            "project",
            "context",
            "--input",
            project_path.to_str().unwrap(),
            "--object",
            object_path.to_str().unwrap(),
            "--output",
            context_path.to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "{:?}", out);
    let context: Value = serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    let calculation = json!({"calculation_id":"beam","method":"beam-cantilever-end-load/1","object":object,"inputs":inputs,"assumptions":["fixed-free beam, end load"],"validity_conditions":["linear elastic small deflection"],"invalid_when":["load or support changes"],"source_refs":["mechanics-source"]});
    let mut work = json!({"schema":"oev2.engineering-work/3","project":{"path":project_path,"sha256":project_sha},"object":object,"project_context":context,"acceptance":basis["acceptance"],"necessary_functions":["support the declared end load"],"physical_mechanisms":["elastic bending"],"gaps":[{"gap_id":"beam-deflection","question":"Does the declared model predict less than 0.03 mm?","necessary_function":"support the declared end load","physical_mechanism":"elastic bending","required_conditions":["declared end load and support"],"needed_evidence":["actual Rust calculation and original model applicability checks"]}],"source_requests":[],"calculations":[calculation],"handoff":null});
    let input = dir.join("work.json");
    let run_work = |out_name: &str| {
        let target = dir.join(out_name);
        invoke(
            &root,
            &[
                "project",
                "work",
                "--input",
                input.to_str().unwrap(),
                "--output-dir",
                target.to_str().unwrap(),
            ],
        )
    };
    save("work.json", &work);
    let out = run_work("good");
    assert!(out.status.success(), "{:?}", out);
    let report: Value =
        serde_json::from_slice(&fs::read(dir.join("good/work.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "analysis_completed");
    assert_eq!(report["semantic_review"]["execution"], "not_run");
    assert_eq!(report["semantic_review"]["conclusion"], "unknown");
    // Independent arithmetic: 10 * 100^3 / (3 * 200000 * 1000) mm.
    assert!(
        (report["calculations"][0]["result"]["value"]
            .as_f64()
            .unwrap()
            - 1.0 / 60.0)
            .abs()
            < 1e-12
    );
    assert_eq!(report["calculations"][0]["result"]["unit"], "mm");
    assert_eq!(report["adopted"], false);
    assert_eq!(report["engineering_conclusion"], "unknown");
    // A real host loop: freeze criteria, ask for a tool result, execute the
    // original work CLI, submit its bytes, then inspect Project and Harness.
    let reference =
        |path: &Path| json!({"path":path,"sha256":oe_core::sha256(fs::read(path).unwrap())});
    let work_ref = reference(&input);
    let source_ref = reference(&dir.join("source.txt"));
    save(
        "expected.json",
        &json!({"schema":"oev2.exploration-expectations/1","exposure":"development_replay",
        "quality":{"preserve_complete_acceptance":true,"action_order":["calculate"],"decision_conclusions":{"beam-prediction":"supported"}},
        "cost":{"maximum_actions":1,"maximum_model_calls":0}}),
    );
    let candidate = "beam-model";
    let hypothesis = "small-deflection";
    let plan = json!({"schema":"oev2.engineering-exploration/1","exploration_id":"host-beam-probe",
        "parent_work":work_ref,"comparison_mode":"mechanism","parameter_rules":[],
        "comparison_extension_reason":"Explicit declared-model inquiry with host-dispatched calculation; physical conditions remain unverified.",
        "prior_contract":null,"observations":[source_ref],"unknowns":["physical performance and method benefit"],
        "candidates":[{"candidate_id":candidate,"work":work_ref,"principle":"elastic bending","representation":"declared beam",
            "applicability":["declared 10 N fixed-free beam only"],"source_refs":[source_ref],"remaining_obligations":work["acceptance"]}],
        "hypotheses":[{"hypothesis_id":hypothesis,"decision_id":"beam-prediction","candidate_id":candidate,"gap_id":"beam-deflection",
            "statement":"The declared model predicts displacement less than 0.03 mm.","kind":"model_prediction",
            "grounding":"hypothesis","source_refs":[source_ref],"scope":["declared beam model only"]}],
        "actions":[{"action_id":"calculate","candidate_id":candidate,"gap_id":"beam-deflection","requirement":work["acceptance"][0],
            "kind":"external","calculation_id":null,"source":null,"pointer":"/observation/calculations/0/result","hypotheses":[hypothesis],
            "purpose":"Evaluate the declared deflection and retain its applicability checks.","expected_impact":"Distinguish prediction below the criterion from at-or-above it.",
            "required_facts":["force","support"],"when_outcomes":[],"selection_reason":"Required model question; existing calculation is available; total cost is unknown.",
            "cost_basis":"unknown","cost_sources":[]}],
        "outcomes":[{"outcome_id":"below","action_id":"calculate","operator":"lt","value":q(0.03,"mm"),"supported":[hypothesis],"rejected":[],"review":[],
            "retained_candidates":[candidate],"rejected_candidates":[],"review_candidates":[]},
            {"outcome_id":"at-or-above","action_id":"calculate","operator":"ge","value":q(0.03,"mm"),"supported":[],"rejected":[hypothesis],"review":[],
            "retained_candidates":[],"rejected_candidates":[candidate],"review_candidates":[]}],
        "independent_expected":reference(&dir.join("expected.json")),"exposure":"development_replay"});
    save("plan.json", &plan);
    let frozen_dir = dir.join("frozen");
    let out = invoke(
        &root,
        &[
            "project",
            "explore",
            "freeze",
            "--input",
            dir.join("plan.json").to_str().unwrap(),
            "--output",
            frozen_dir.to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let frozen_ref = reference(&frozen_dir.join("completed.json"));
    let mut request = json!({"schema":"oev2.exploration-host-input/1","frozen":frozen_ref,"previous":null,"receipt":null,"output":dir.join("host-pending")});
    save("host-start.json", &request);
    let out = invoke(
        &root,
        &[
            "project",
            "explore",
            "advance",
            "--input",
            dir.join("host-start.json").to_str().unwrap(),
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let pending: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(pending["status"], "requires_host_agent");
    assert_eq!(pending["pending"][0]["action"]["action_id"], "calculate");
    assert_eq!(pending["adopted"], false);
    assert!(run_work("host-tool").status.success());
    request["previous"] = reference(&dir.join("host-pending/completed.json"));
    request["receipt"] = json!({"action_id":"calculate","owner":"project_work","artifact":reference(&dir.join("host-tool/work.json"))});
    request["output"] = json!(dir.join("host-finished"));
    save("host-finish.json", &request);
    let out = invoke(
        &root,
        &[
            "project",
            "explore",
            "advance",
            "--input",
            dir.join("host-finish.json").to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let completed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(completed["status"], "results_recorded");
    let result: Value =
        serde_json::from_slice(&fs::read(dir.join("host-finished/run/results.json")).unwrap())
            .unwrap();
    assert_eq!(
        result["decision_conclusions"]["beam-prediction"],
        "supported"
    );
    assert_eq!(result["physical_truth"], "not_established");
    assert_eq!(result["complete_acceptance_preserved"], true);
    assert_eq!(result["cost"]["model_calls"], 0);
    let harness: Value =
        serde_json::from_slice(&fs::read(dir.join("host-finished/run/harness.json")).unwrap())
            .unwrap();
    assert_eq!(harness["schema"], "oev2.harness-checkpoint/1");
    assert_eq!(harness["model_execution"], "not_run");
    // A fabricated result cannot get past re-execution of the original model.
    let mut forged = report.clone();
    forged["calculations"][0]["result"]["value"] = json!(1.0);
    save("forged.json", &forged);
    request["receipt"]["artifact"] = reference(&dir.join("forged.json"));
    request["output"] = json!(dir.join("forged-output"));
    save("host-forged.json", &request);
    let out = invoke(
        &root,
        &[
            "project",
            "explore",
            "advance",
            "--input",
            dir.join("host-forged.json").to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(!dir.join("forged-output").exists());
    // A valid arithmetic result for a changed load must lose current support.
    work["calculations"][0]["inputs"]["force"] = q(20.0, "N");
    save("work.json", &work);
    let out = run_work("mismatched-load");
    assert!(!out.status.success());
    let report: Value =
        serde_json::from_slice(&fs::read(dir.join("mismatched-load/work.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "partial");
    assert_eq!(
        report["calculations"][0]["current_support"],
        "not_established"
    );
    assert!(
        (report["calculations"][0]["result"]["value"]
            .as_f64()
            .unwrap()
            - 1.0 / 30.0)
            .abs()
            < 1e-12
    );
    // An edited Project cannot reuse the old digest-bound work or context.
    let mut revised = project;
    revised["objects"]["guide"] = json!("r2");
    save("project.json", &revised);
    assert_eq!(run_work("stale").status.code(), Some(2));
    assert!(!dir.join("stale/work.json").exists());
}
