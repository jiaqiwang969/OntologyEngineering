//! Cutover contracts: bounded failures stay machine-readable and never echo data.
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

#[cfg(unix)]
#[test]
fn executable_symlink_discovers_the_skill_root_outside_the_install_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    // Cargo may use a shared target outside the skill. Model an actual install
    // with one program copy under the skill's private directory, not a source
    // checkout copy or a fallback to the compiler's original source path.
    fs::create_dir_all(root.join("var")).unwrap();
    let installation = {
        fs::create_dir_all(root.join("var/tests/unified-system/cli")).unwrap();
        tempfile::tempdir_in(root.join("var/tests/unified-system/cli")).unwrap()
    };
    let installed = installation.path().join("oev2");
    fs::copy(env!("CARGO_BIN_EXE_oev2"), &installed).unwrap();
    fs::set_permissions(
        &installed,
        fs::metadata(env!("CARGO_BIN_EXE_oev2"))
            .unwrap()
            .permissions(),
    )
    .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let alias = temporary.path().join("oe");
    std::os::unix::fs::symlink(&installed, &alias).unwrap();
    let output = Command::new(&alias)
        .current_dir(temporary.path())
        .env_clear()
        .env("PATH", "")
        .args([
            "semantics",
            "bundle",
            "--name",
            "engineering-judgment-intake",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["package_id"], "semantica.engineering.judgment-intake");
}

#[cfg(unix)]
#[test]
fn incomplete_release_bundle_does_not_fall_back_to_valid_cwd_skill() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    assert!(root.join("SKILL.md").is_file());
    assert!(root.join("runtime/semantic-bundles.json").is_file());
    let temporary = tempfile::tempdir().unwrap();
    let bundle = temporary.path().join("bundle");
    fs::create_dir_all(bundle.join("bin")).unwrap();
    fs::create_dir(bundle.join("skill")).unwrap();
    let installed = bundle.join("bin/oev2");
    fs::copy(env!("CARGO_BIN_EXE_oev2"), &installed).unwrap();
    fs::set_permissions(
        &installed,
        fs::metadata(env!("CARGO_BIN_EXE_oev2"))
            .unwrap()
            .permissions(),
    )
    .unwrap();
    fs::write(
        bundle.join("RELEASE-MANIFEST.json"),
        b"{\"schema\":\"oev2.release-manifest/1\",\"skill_root\":\"skill\"}\n",
    )
    .unwrap();
    fs::write(
        bundle.join("skill/SKILL.md"),
        b"---\nname: ontology-engineering-v2\n---\n",
    )
    .unwrap();
    assert!(!bundle.join("skill/runtime/semantic-bundles.json").exists());
    let output = Command::new(&installed)
        .current_dir(root)
        .env_clear()
        .env("PATH", "")
        .args([
            "harness",
            "step",
            "--input",
            "missing.json",
            "--output",
            "var/uncreated",
        ])
        .output()
        .unwrap();
    let value = error(&output, "harness");
    assert!(value.get("payload").is_none());
    assert!(!bundle.join("skill/runtime").exists());
}

fn receiver() -> tempfile::TempDir {
    let receiver = tempfile::tempdir().unwrap();
    fs::write(receiver.path().join("SKILL.md"), "# Isolated receiver\n").unwrap();
    receiver
}

fn run(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root.canonicalize().unwrap())
        .args(arguments)
        .output()
        .unwrap()
}

fn error(output: &Output, family: &str) -> Value {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["status"],
        if family == "harness" {
            "input_or_execution_error"
        } else {
            "input_or_output_error"
        }
    );
    if family == "harness" {
        assert_eq!(value["ontology_promotion"], "not_performed");
    } else {
        assert_eq!(value["model_execution"], "not_confirmed");
    }
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_INPUT_VALUE"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_OPTION_VALUE"));
    value
}

#[test]
fn missing_input_delivers_one_safe_json_on_stdout_without_partial_output() {
    let receiver = receiver();
    for family in ["harness"] {
        let output = run(
            receiver.path(),
            &[
                family,
                "step",
                "--input",
                "PRIVATE_INPUT_VALUE",
                "--output",
                "var/uncreated",
            ],
        );
        error(&output, family);
        assert!(!receiver.path().join("var").exists());
    }
}

#[test]
fn invalid_attempt_value_and_unknown_options_do_not_echo_caller_data() {
    let receiver = receiver();
    for family in ["harness"] {
        for option in ["--max-attempts", "--unsupported-option"] {
            let output = run(
                receiver.path(),
                &[
                    family,
                    "step",
                    "--input",
                    "PRIVATE_INPUT_VALUE",
                    "--output",
                    "var/uncreated",
                    option,
                    "PRIVATE_OPTION_VALUE",
                ],
            );
            error(&output, family);
        }
        assert!(!receiver.path().join("var").exists());
    }
}

#[test]
fn duplicate_json_keys_are_rejected_before_output_without_parser_fragments() {
    let receiver = receiver();
    let input = receiver.path().join("bad.json");
    fs::write(
        &input,
        b"{\"schema\":\"PRIVATE_INPUT_VALUE\",\"schema\":null}",
    )
    .unwrap();
    for arguments in [
        vec![
            "harness",
            "step",
            "--input",
            input.to_str().unwrap(),
            "--output",
            "var/uncreated",
        ],
        vec![
            "harness",
            "consolidate",
            "--input",
            input.to_str().unwrap(),
            "--output",
            "var/uncreated",
        ],
    ] {
        error(&run(receiver.path(), &arguments), arguments[0]);
        assert!(!receiver.path().join("var").exists());
    }
}

#[test]
fn help_stays_successful_and_is_not_rewritten_as_an_error() {
    let receiver = receiver();
    for family in ["harness"] {
        let output = run(receiver.path(), &[family, "--help"]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: oe"));
    }
}

#[test]
fn invalid_root_retains_family_error_without_creating_the_requested_directory() {
    let receiver = receiver();
    for family in ["harness"] {
        let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(receiver.path().join("PRIVATE_INPUT_VALUE"))
            .args([
                family,
                "step",
                "--input",
                "missing.json",
                "--output",
                "var/uncreated",
            ])
            .output()
            .unwrap();
        error(&output, family);
    }
    assert!(!receiver.path().join("PRIVATE_INPUT_VALUE").exists());
}

#[test]
fn output_collision_preserves_the_original_trace_and_never_dispatches() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let receiver = {
        fs::create_dir_all(root.join("var/tests/unified-system/cli")).unwrap();
        tempfile::tempdir_in(root.join("var/tests/unified-system/cli")).unwrap()
    };
    let input = receiver.path().join("input.json");
    fs::write(&input, serde_json::to_vec(&json!({
        "schema":"oev2.harness-checkpoint-input/1", "task_id":"collision",
        "context":"maintain existing trace", "request":"Record complete conditions",
        "harness":{"project_id":"project", "decision_id":"decision", "object_id":"object",
            "revision":"r1", "problem_class":"maintenance", "goal":"preserve trace",
            "conditions":[],"constraints":[],"unknowns":["No engineering verification"],
            "semantic_modules":[],"review_requests":[],
            "checkpoint_policy":{"schema":"oev2.harness-checkpoint-policy/1","mode":"verified_no_model",
                "byte_budget":16777216,"reason":"Preserve existing output exactly"}}
    })).unwrap()).unwrap();
    let output = receiver.path().join("existing");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("original"), b"preserve these exact bytes").unwrap();
    let result = run(
        &root,
        &[
            "harness",
            "step",
            "--input",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
    );
    error(&result, "harness");
    assert_eq!(
        fs::read(output.join("original")).unwrap(),
        b"preserve these exact bytes"
    );
    assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
}

#[test]
fn retired_routes_cannot_dispatch_or_record_advice() {
    let receiver = receiver();
    for command in ["prepare", "run", "advise", "batch"] {
        let output = run(
            receiver.path(),
            &[
                "route",
                command,
                "--input",
                "PRIVATE_INPUT_VALUE",
                "--output",
                "var/uncreated",
            ],
        );
        error(&output, "route");
        assert!(!receiver.path().join("var").exists());
    }
}
