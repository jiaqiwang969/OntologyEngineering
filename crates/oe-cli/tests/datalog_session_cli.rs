use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn state(value: &Value) -> Value {
    let mut facts: Vec<_> = value["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{}({})",
                f["predicate"].as_str().unwrap(),
                f["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    facts.sort();
    json!({"facts":facts,"rule_count":value["rule_count"],"derived":value["derived"]})
}

#[test]
fn datalog_session_cli_records_failures_without_python() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let dir = oe_core::SkillRoot::new(root)
        .unwrap()
        .new_private_dir(Path::new(&format!(
            "var/maintenance/datalog-session-cli-tests-{}",
            std::process::id()
        )))
        .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/datalog-session-original22.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let input = dir.join(format!("{id}-input.json"));
        let output = dir.join(format!("{id}-output.json"));
        fs::write(&input, serde_json::to_vec(&case["request"]).unwrap()).unwrap();
        let actual = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(root)
            .args(["kernel", "datalog-session", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        fs::write(dir.join(format!("{id}.stdout")), &actual.stdout).unwrap();
        fs::write(dir.join(format!("{id}.stderr")), &actual.stderr).unwrap();
        assert_eq!(
            actual.status.code(),
            Some(if case["expected"]["all_actions_succeeded"] == true {
                0
            } else {
                1
            }),
            "{id}: {}",
            String::from_utf8_lossy(&actual.stdout)
        );
        let value: Value = serde_json::from_slice(&actual.stdout).unwrap();
        assert_eq!(
            value,
            serde_json::from_slice::<Value>(&fs::read(&output).unwrap()).unwrap()
        );
        let steps: Vec<_> = value["steps"].as_array().unwrap().iter().map(|s| {
            assert_eq!(s["failure"].is_null(), s["status"]!="failed");
            json!({"action_id":s["action_id"],"operation":s["operation"],"status":s["status"],
                "state_before":state(&s["state_before"]),"state_after":state(&s["state_after"]),
                "inserted":s["inserted"],"graph_facts_added":s["graph_facts_added"],"query_rows":s["query_rows"]})
        }).collect();
        assert_eq!(
            json!({"steps":steps,"final_state":state(&value["final_state"]),
            "all_actions_succeeded":value["all_actions_succeeded"]}),
            case["expected"],
            "{id}"
        );
    }
    for (i, bytes) in [
        b"{\"schema\":\"semantica.datalog-session-request/99\",\"actions\":[],\"on_error\":\"continue\",\"budget\":{\"max_iterations\":1024,\"max_match_attempts\":10000000,\"max_facts\":100000,\"max_query_rows\":10000}}".as_slice(),
        b"{\"schema\":\"semantica.datalog-session-request/1\",\"schema\":\"semantica.datalog-session-request/1\"}".as_slice(),
    ].into_iter().enumerate() {
        let input = dir.join(format!("bad{i}.json")); let output = dir.join(format!("bad{i}-out.json"));
        fs::write(&input, bytes).unwrap();
        let actual = Command::new(env!("CARGO_BIN_EXE_oev2")).env_clear().env("PATH", "")
            .arg("--root").arg(root).args(["kernel","datalog-session","--input"]).arg(input)
            .arg("--output").arg(&output).output().unwrap();
        assert_eq!(actual.status.code(), Some(2)); assert!(!output.exists());
    }
}
