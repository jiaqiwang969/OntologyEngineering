use serde_json::{Value, json};
use std::{path::Path, process::Command};

#[test]
fn current_drafting_entry_requires_project_and_current_context() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    for required in ["--project", "--project-context"] {
        let mut args = vec![
            "cad",
            "drawing",
            "check-drafting",
            "--input",
            "var/absent-drafting-request.json",
            "--output",
            "var/absent-drafting-receipt.json",
        ];
        if required != "--project" {
            args.extend(["--project", "var/absent-project.json"]);
        }
        if required != "--project-context" {
            args.extend(["--project-context", "var/absent-context.json"]);
        }
        let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains(required),
            "current adoption must not omit {required}: {error}"
        );
        assert!(output.stdout.is_empty());
        assert!(!root.join("var/absent-drafting-receipt.json").exists());
    }
}

#[test]
fn missing_native_job_preserves_changed_current_plan_diagnosis() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temporary = tempfile::tempdir_in(root.join("var")).unwrap();
    let directory = temporary.path();
    let plan = json!({"schema":"oev2.native-drafting-plan/1","sheet_name":"prototype-reference",
        "object":{"object_id":"coupon","revision":"b","condition_scope":"design","source_sha256":"a".repeat(64)},
        "unit":"mm","width":297.0,"height":210.0,"scale_numerator":1.0,"scale_denominator":2.0,
        "projection_angle":"third","model_view_name":"Top","view_center":[148.5,105.0],
        "purpose":"prototype_geometry_only","source_refs":["independent-design"]});
    let mut revised = plan.clone();
    revised["width"] = json!(310.0);
    let observation = json!({"schema":"oev2.native-drawing-observation/2","object":plan["object"],
        "native_result_ref":"native-receipt","native_result_sha256":"b".repeat(64),"part":{},
        "job_directory":directory.join("absent-job"),"part_input":"inputs/coupon.prt"});
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"coupon","object_revision":"b",
        "purpose":"current prototype drawing obligation","acceptance":["current geometry"],"facts":[
            {"property":"drafting_plan:prototype-reference","value":revised,"status":"declared","source_refs":["independent-design"]},
            {"property":"native_drafting:prototype-reference","value":observation,"status":"declared","source_refs":["native-receipt"]}],
        "assertions":[{"assertion_id":"prototype-page","kind":"cad_drafting_sheet","subject":"prototype-reference",
            "requirements":plan,"source_refs":["independent-design"],"affected_decisions":["drawing"]}]});
    let project: oe_kernel::project::Project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"current-drafting",
        "revision":"b","purpose":basis["purpose"],"objects":{"coupon":"b"},"conditions":{"design":serde_json::to_string(&basis).unwrap()},
        "evidence":{"cad":"a".repeat(64),"native-receipt":"b".repeat(64),"independent-design":"c".repeat(64)},
        "decisions":[{"id":"drawing","object_id":"coupon","object_revision":"b","condition_scope":"design",
            "source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["independent-design"]}],"dependencies":{}})).unwrap();
    let object = serde_json::from_value(plan["object"].clone()).unwrap();
    let context = oe_kernel::project::context(&project, &object).unwrap();
    for (name, value) in [
        (
            "request.json",
            json!({"plan":plan,"job_directory":directory.join("absent-job"),
        "part_input":"inputs/coupon.prt","native_result_ref":"native-receipt"}),
        ),
        ("project.json", json!(project)),
        ("context.json", json!(context)),
    ] {
        std::fs::write(directory.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(&root)
        .args(["cad", "drawing", "check-drafting", "--input"])
        .arg(directory.join("request.json"))
        .arg("--project")
        .arg(directory.join("project.json"))
        .arg("--project-context")
        .arg(directory.join("context.json"))
        .arg("--output")
        .arg(directory.join("receipt.json"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["current_project_check"]["conclusion"], "pass");
    assert_eq!(value["declared_plan_check"]["conclusion"], "violated");
    assert_eq!(
        value["declared_plan_check"]["failures"],
        json!(["drafting_design_content_changed"])
    );
    assert_eq!(value["native_source_check"]["conclusion"], "unknown");
    assert_eq!(
        value["native_drafting_check"]["reason"],
        "drafting_native_job_observation_failed"
    );
    assert_eq!(value["observation"], Value::Null);
    assert_eq!(value["current_use"], "blocked_or_historical_only");
    assert_eq!(value["engineering_adoption"], "not_established");
    assert_eq!(
        value["execution_binding"]["normalizer"]["status"],
        "matched"
    );
    assert!(directory.join("receipt.json").is_file());
}
