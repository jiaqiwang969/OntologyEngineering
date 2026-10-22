use oe_core::{SkillRoot, sha256};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn actual_rust_forward_review_runs_without_python_and_keeps_negative_results_blocked() {
    let original = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let real = SkillRoot::new(original).unwrap();
    let mut mismatches = Vec::new();
    for (index, record, clear) in [
        (0, "review-current-support-identity_current.json", true),
        (1, "review-old-report-identity_stale.json", false),
    ] {
        let path = real
            .new_private_dir(Path::new(&format!(
                "var/maintenance/rust-forward-tests/{}-{index}",
                std::process::id()
            )))
            .unwrap();
        fs::write(path.join("SKILL.md"), "isolated actual Rust forward review").unwrap();
        fs::create_dir_all(path.join("var/project/evidence")).unwrap();
        fs::create_dir(path.join("var/project/registry")).unwrap();
        let lock: Value = serde_json::from_slice(
            &fs::read(original.join("runtime/semantic-bundles.json")).unwrap(),
        )
        .unwrap();
        let spec = &lock["bundles"]["engineering-judgment-intake"];
        for relative in [
            "runtime/semantic-bundles.json",
            "runtime/semantica-source-lock.json",
            spec["path"].as_str().unwrap(),
        ] {
            let target = path.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(original.join(relative), target).unwrap();
        }
        for file in [record, "review-sources.json"] {
            fs::copy(
                original.join("examples/judgment_intake").join(file),
                path.join("var/project/evidence").join(file),
            )
            .unwrap();
        }
        let binding = json!({"$schema":"ontology-engineering.semantic-project-binding/v1","binding_id":"synthetic-binding","project":{"project_id":"synthetic-project","domain":"discrete-manufacturing"},"semantic_target":{"kind":"workspace","package_id":spec["package_id"],"workspace_id":"synthetic-registry"},"baseline":{"version":spec["package_version"],"digest":spec["package_sha256"]},"evidence":{"logical_root":"evidence:synthetic-review"},"authority":{"fact":{"authority_id":"synthetic-engineer","scope":["test-evidence"]},"decision":{"authority_id":"synthetic-reviewer","scope":["test-review"]}},"allowed_actions":["review","propose"],"lifecycle_actions":["candidate"],"promotion":{"target":"industry-registry","requires_decision_authority":true},"created_at":"2026-09-27T00:00:00Z","semantic_api":"semantica.ontology.refinery/v1"});
        let binding_bytes = oe_core::json::canonical_json(&binding).unwrap();
        fs::write(path.join("var/project/binding.json"), &binding_bytes).unwrap();
        let input = json!({"project_id":"synthetic-project","review_requests":[{"id":"identity-check","record":format!("var/project/evidence/{record}"),"record_sha256":sha256(fs::read(path.join("var/project/evidence").join(record)).unwrap()),"binding":"var/project/binding.json","binding_sha256":sha256(&binding_bytes),"evidence_root":"var/project/evidence","workspace":"var/project/registry","actor":"synthetic-operator"}]});
        fs::write(
            path.join("var/input.json"),
            oe_core::json::canonical_json(&input).unwrap(),
        )
        .unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(&path)
            .args(["semantics", "review", "--input"])
            .arg(path.join("var/input.json"))
            .args(["--output", "var/project/reviews"])
            .output()
            .unwrap();
        fs::write(path.join("var/execution.stdout"), &result.stdout).unwrap();
        fs::write(path.join("var/execution.stderr"), &result.stderr).unwrap();
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(
            report["status"], "completed",
            "formal execution must run despite empty PATH: {report}"
        );
        let is_clear = report["reviews"][0]["review"]["status"] == "clear";
        let expected_code = if clear { 0 } else { 1 };
        if is_clear != clear || result.status.code() != Some(expected_code) {
            mismatches.push(format!(
                "{record}: clear={is_clear}; exit={:?}; report={report}",
                result.status.code()
            ));
        }
        let native_path = report["reviews"][0]["artifacts"]["native-execution.json"]["path"]
            .as_str()
            .unwrap();
        let native: Value =
            serde_json::from_slice(&fs::read(path.join(native_path)).unwrap()).unwrap();
        assert_eq!(native["engine_id"], oe_kernel::ENGINE_ID);
        assert_eq!(
            native["conclusion"],
            if clear { "pass" } else { "violated" }
        );
        assert!(!path.join("var/runtime").exists());
        assert_eq!(
            fs::read(path.join("var/project/binding.json")).unwrap(),
            binding_bytes
        );
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
