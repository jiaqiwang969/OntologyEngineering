use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

#[test]
fn datalog_numeric_cli_preserves_symbols_and_rejects_nonfinite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/datalog-numeric-cli-tests-{}",
            std::process::id()
        )))
        .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/datalog-numeric-original60.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let input = dir.join(format!("{id}-input.json"));
        let result = dir.join(format!("{id}-result.json"));
        fs::write(&input, case["request_raw"].as_str().unwrap()).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", case["command"].as_str().unwrap(), "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&result)
            .output()
            .unwrap();
        fs::write(dir.join(format!("{id}.stdout")), &out.stdout).unwrap();
        fs::write(dir.join(format!("{id}.stderr")), &out.stderr).unwrap();
        assert!(
            out.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["query_rows"], case["expected"]["rows"], "{id}");
        assert_eq!(
            value["seed_fact_count"], case["expected"]["seed_fact_count"],
            "{id}"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&result).unwrap()).unwrap(),
            value
        );
    }
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["id"].as_str().unwrap().starts_with("small-seven-"))
    {
        let id = case["id"].as_str().unwrap();
        let input = dir.join(format!("{id}-overflow-input.json"));
        let result = dir.join(format!("{id}-overflow-result.json"));
        fs::write(
            &input,
            case["request_raw"]
                .as_str()
                .unwrap()
                .replace("\"object\":1e-7", "\"object\":1e400"),
        )
        .unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", case["command"].as_str().unwrap(), "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&result)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!result.exists());
    }
}

#[test]
fn datalog_cli_executes_without_python_and_refuses_invalid_input() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let skill = oe_core::SkillRoot::new(root).unwrap();
    let dir = skill
        .new_private_dir(Path::new(&format!(
            "var/maintenance/datalog-cli-tests-{}",
            std::process::id()
        )))
        .unwrap();
    let request = json!({"schema":"semantica.datalog-request/1","facts":["depends(drawing, assembly)","depends(assembly, part)"],"rules":["affected(X, Y) :- depends(X, Y).","affected(X, Y) :- depends(X, Z), affected(Z, Y)."],"query":{"pattern":"affected(?object, part)","bindings":{}},"budget":{"max_iterations":1024,"max_match_attempts":10000000,"max_facts":100000,"max_query_rows":10000}});
    for (name, mut input, success) in [
        ("valid", request.clone(), true),
        ("schema", request.clone(), false),
        ("unsafe", request, false),
    ] {
        if name == "schema" {
            input["schema"] = json!("semantica.datalog-request/99");
        }
        if name == "unsafe" {
            input["rules"] = json!(["unsafe(X) :- depends(Y, Z)."]);
        }
        let input_path = dir.join(format!("{name}-input.json"));
        let output_path = dir.join(format!("{name}-result.json"));
        fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "datalog", "--input"])
            .arg(&input_path)
            .arg("--output")
            .arg(&output_path)
            .output()
            .unwrap();
        fs::write(dir.join(format!("{name}.stdout")), &output.stdout).unwrap();
        fs::write(dir.join(format!("{name}.stderr")), &output.stderr).unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        if success {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["schema"], "semantica.datalog-result/1");
            assert_eq!(value["complete"], true);
            assert_eq!(
                value["query_rows"],
                json!([{"object":"assembly"},{"object":"drawing"}])
            );
            assert_eq!(value["derived_fact_count"], 3);
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(output_path).unwrap()).unwrap(),
                value
            );
        } else {
            assert_eq!(output.status.code(), Some(2));
            assert!(!output_path.exists());
        }
    }
}

#[test]
fn datalog_graph_cli_executes_and_rejects_invalid_snapshot() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let skill = oe_core::SkillRoot::new(root).unwrap();
    let dir = skill
        .new_private_dir(Path::new(&format!(
            "var/maintenance/datalog-graph-cli-tests-{}",
            std::process::id()
        )))
        .unwrap();
    let request = json!({"schema":"semantica.datalog-graph-request/1","facts":[],"graph":{"nodes":[],"edges":[{"source":"drawing","target":"assembly","type":"depends"},{"source":"assembly","target":"part","type":"depends"}]},"rules":["affected(X, Y) :- depends(X, Y).","affected(X, Y) :- depends(X, Z), affected(Z, Y)."],"query":{"pattern":"affected(?object, part)","bindings":{}},"budget":{"max_iterations":1024,"max_match_attempts":10000000,"max_facts":100000,"max_query_rows":10000}});
    for (name, mut input, success) in [
        ("valid", request.clone(), true),
        ("schema", request.clone(), false),
        ("unsafe", request.clone(), false),
        ("bad-graph", request, false),
    ] {
        if name == "schema" {
            input["schema"] = json!("semantica.datalog-graph-request/99");
        }
        if name == "unsafe" {
            input["rules"] = json!(["unsafe(X) :- depends(Y, Z)."]);
        }
        if name == "bad-graph" {
            input["graph"]["edges"][1] = json!(null);
        }
        let input_path = dir.join(format!("{name}-input.json"));
        let output_path = dir.join(format!("{name}-result.json"));
        fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "datalog-graph", "--input"])
            .arg(&input_path)
            .arg("--output")
            .arg(&output_path)
            .output()
            .unwrap();
        fs::write(dir.join(format!("{name}.stdout")), &output.stdout).unwrap();
        fs::write(dir.join(format!("{name}.stderr")), &output.stderr).unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        if success {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["schema"], "semantica.datalog-graph-result/1");
            assert_eq!(value["complete"], true);
            assert_eq!(
                value["query_rows"],
                json!([{"object":"assembly"},{"object":"drawing"}])
            );
            assert_eq!(value["derived_fact_count"], 3);
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(output_path).unwrap()).unwrap(),
                value
            );
        } else {
            assert_eq!(output.status.code(), Some(2));
            assert!(!output_path.exists());
        }
    }
}
