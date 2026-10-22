//! Actual native CLI/execution identity probes; generated contracts are never changed in-place.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn original_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}
fn copy_public(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap();
        if ["var", "sources", ".git", "target"]
            .iter()
            .any(|s| name == *s)
        {
            continue;
        }
        let d = dst.join(name);
        if p.is_dir() {
            copy_public(&p, &d);
        } else {
            fs::copy(&p, &d).unwrap();
        }
    }
}
fn invoke(root: &Path, args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn native_service_cli_executes_real_stdio_and_context_summary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("service root");
    copy_public(&original_root(), &root);
    fs::create_dir_all(root.join("var/fixtures")).unwrap();
    fs::write(root.join("var/fixtures/context.json"),serde_json::to_vec(&json!({"nodes":[{"id":"part","type":"Part"},{"id":"decision","type":"decision"}],"edges":[]})).unwrap()).unwrap();
    let out=invoke(&root,&["services","mcp-stdio","--context-graph","var/fixtures/context.json"],b"{\"id\":1,\"method\":\"ping\"}\n{\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"get_graph_summary\"}}\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let values = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0], json!({"jsonrpc":"2.0","id":1,"result":{}}));
    let summary: Value =
        serde_json::from_str(values[1]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        summary,
        json!({"node_count":2,"decision_count":1,"graph_ready":true})
    );
    assert_eq!(
        fs::read_dir(root.join("var/maintenance/executions/semantic_service"))
            .unwrap()
            .count(),
        1
    );
}
#[test]
fn native_service_cli_refuses_catalog_generated_and_added_source_drift() {
    for profile in ["catalog", "generated", "member"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("drift root");
        copy_public(&original_root(), &root);
        match profile {
            "catalog" => {
                let p =
                    root.join("crates/semantica-service/tests/fixtures/original-mcp-catalog.json");
                let mut b = fs::read(&p).unwrap();
                b.push(b' ');
                fs::write(&p, b).unwrap();
            }
            "generated" => {
                let p = root.join("generated/semantic-contracts.rs");
                let mut b = fs::read(&p).unwrap();
                b.extend_from_slice(b"\n// independent semantic contract drift\n");
                fs::write(&p, b).unwrap();
            }
            "member" => {
                fs::write(
                    root.join("crates/semantica-service/src/added_owned.rs"),
                    b"// independent added compile source member\n",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let out = invoke(&root, &["services", "mcp-stdio"], b"");
        assert!(
            !out.status.success(),
            "{profile}: source-bound entry accepted drift"
        );
        assert!(
            out.stdout.is_empty(),
            "{profile}: drift must refuse before stdio payload"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("mismatch"),
            "{profile}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
#[test]
fn native_service_cli_rejects_forged_or_changed_source_before_query() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("source root");
    copy_public(&original_root(), &root);
    fs::create_dir_all(root.join("var/fixtures")).unwrap();
    fs::write(
        root.join("var/fixtures/data.nt"),
        b"<urn:part> <urn:p> <urn:o> .",
    )
    .unwrap();
    fs::write(root.join("var/fixtures/input.json"), b"{\"passed\":true}").unwrap();
    let bad = "0".repeat(64);
    let out = invoke(
        &root,
        &[
            "services",
            "query",
            "--data",
            "var/fixtures/data.nt",
            "--data-sha256",
            &bad,
            "--input",
            "var/fixtures/input.json",
        ],
        b"",
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("service_source_identity_mismatch"));
    assert!(out.stdout.is_empty());
}

#[test]
fn native_service_cli_reads_the_registered_original_books_and_manifests() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("registered books");
    copy_public(&original_root(), &root);
    for id in [oe_semantic::books::rust::ARCHIVE, "sources/continuity.json"] {
        let path = root.join(id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(original_root().join(id), path).unwrap();
    }
    let out=invoke(&root,&["services","mcp-stdio","--book-catalog"],b"{\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"list_chapter_packages\"}}\n{\"id\":2,\"method\":\"resources/list\"}\n{\"id\":3,\"method\":\"resources/read\",\"params\":{\"uri\":\"semantica://packages/manifest/semantica.chapter_packages.vol1.ch03\"}}\n");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let values: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(values.len(), 3);
    let expected: Value = serde_json::from_str(include_str!(
        "../../semantica-service/tests/fixtures/original-book-catalog-oracle.json"
    ))
    .unwrap();
    let all: Value =
        serde_json::from_str(values[0]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(all, expected["cases"][0]["expected"]);
    assert_eq!(
        values[1]["result"]["resources"].as_array().unwrap().len(),
        34
    );
    let manifest: Value =
        serde_json::from_str(values[2]["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    let case = expected["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "get-semantica.chapter_packages.vol1.ch03")
        .unwrap();
    assert_eq!(manifest, case["expected"]);
    let bytes = fs::read(root.join(oe_semantic::books::rust::ARCHIVE)).unwrap();
    let mut changed = bytes;
    changed[20] ^= 1;
    fs::write(root.join(oe_semantic::books::rust::ARCHIVE), changed).unwrap();
    let bad = invoke(
        &root,
        &["services", "mcp-stdio", "--book-catalog"],
        b"{\"id\":1,\"method\":\"resources/list\"}\n",
    );
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty());
}
#[test]
fn native_tool_profile_executes_actual_mutations_extraction_and_reasoning_over_stdio() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp
        .path()
        .canonicalize()
        .unwrap()
        .join("native tools root");
    copy_public(&original_root(), &root);
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"add_entity","arguments":{"id":"shaft","type":"Part"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_graph_summary","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"extract_entities","arguments":{"text":"Alice Wang in 2026"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"run_reasoning","arguments":{"facts":["Shaft(a)"],"rules":["IF Shaft(?x) THEN Part(?x)"]}}}),
    ];
    let input = requests
        .iter()
        .map(|r| format!("{}\n", r))
        .collect::<String>();
    let output = invoke(
        &root,
        &["services", "mcp-stdio", "--tool-profile", "native"],
        input.as_bytes(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let messages = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 4);
    let result = |i: usize| {
        serde_json::from_str::<Value>(
            messages[i]["result"]["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(result(0), json!({"status":"added","id":"shaft"}));
    assert_eq!(result(1)["node_count"], 1);
    assert_eq!(result(2)["_meta"]["model_execution"], "not_run");
    assert_eq!(result(3)["derived_facts"], json!(["Part(a)"]));
    let original=invoke(&root,&["services","mcp-stdio","--tool-profile","original"],b"{\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"extract_relations\",\"arguments\":{\"text\":\"Alice works for Acme\"}}}\n");
    assert!(original.status.success());
    let message: Value = serde_json::from_slice(&original.stdout).unwrap();
    assert_eq!(message["error"]["code"], -32603);
}
#[test]
fn all_five_book_tools_execute_registered_sources_and_keep_release_unknown() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp
        .path()
        .canonicalize()
        .unwrap()
        .join("actual book tools");
    copy_public(&original_root(), &root);
    for id in [oe_semantic::books::rust::ARCHIVE, "sources/continuity.json"] {
        let dst = root.join(id);
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::copy(original_root().join(id), dst).unwrap();
    }
    let continuity: Value =
        serde_json::from_slice(&fs::read(root.join("sources/continuity.json")).unwrap()).unwrap();
    let mut author_count = 0;
    for entry in continuity["source_files"].as_array().unwrap() {
        let id = entry["path"].as_str().unwrap();
        if !id.starts_with("book-authoring/") {
            continue;
        }
        let raw = fs::read(original_root().join("sources").join(id)).unwrap();
        assert_eq!(
            semantica_runtime::sha256(&raw),
            entry["sha256"].as_str().unwrap()
        );
        let dst = root.join("sources").join(id);
        fs::create_dir_all(dst.parent().unwrap()).unwrap();
        fs::write(dst, raw).unwrap();
        author_count += 1;
    }
    assert_eq!(author_count, 294);
    let observed = invoke(&root, &["books", "runtime-identity"], b"");
    assert!(
        observed.status.success(),
        "{}",
        String::from_utf8_lossy(&observed.stderr)
    );
    let observed: Value = serde_json::from_slice(&observed.stdout).unwrap();
    let identity = &observed["result"];
    assert_eq!(identity["compilation_origin"], "unknown");
    let execution_args = json!({"package_id":"semantica.chapter_packages.vol1.ch03","runtime_commit":identity["runtime_commit"],"runtime_artifact_sha256":identity["runtime_artifact_sha256"]});
    let requests = [
        ("list_chapter_packages", json!({})),
        (
            "get_chapter_package",
            json!({"package_id":"semantica.chapter_packages.vol1.ch03"}),
        ),
        ("verify_book_sources", json!({"book_root":root})),
        ("run_chapter_package", execution_args.clone()),
        ("verify_chapter_package", execution_args.clone()),
        ("run_chapter_package", {
            let mut args = execution_args.clone();
            args["runtime_artifact_sha256"] = json!("0".repeat(64));
            args
        }),
        ("run_chapter_package", {
            let mut args = execution_args;
            args["scenario_id"] = json!("");
            args
        }),
    ];
    let input=requests.iter().enumerate().map(|(i,(name,args))|format!("{}\n",json!({"jsonrpc":"2.0","id":i,"method":"tools/call","params":{"name":name,"arguments":args}}))).collect::<String>();
    let output = invoke(
        &root,
        &[
            "services",
            "mcp-stdio",
            "--tool-profile",
            "native",
            "--book-catalog",
        ],
        input.as_bytes(),
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 7);
    let payload = |i: usize| {
        assert!(messages[i].get("error").is_none(), "{}", messages[i]);
        serde_json::from_str::<Value>(
            messages[i]["result"]["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(payload(0)["chapter_package_count"], 29);
    assert_eq!(
        payload(1)["manifest"]["package_id"],
        "semantica.chapter_packages.vol1.ch03"
    );
    let sources = payload(2);
    assert_eq!(sources["ok"], true, "{sources}");
    assert_eq!(sources["result"]["check_count"], 238);
    let run = payload(3);
    assert_eq!(run["ok"], true, "{run}");
    assert_eq!(run["result"]["status"], "passed");
    assert_eq!(run["result"]["receipt"]["runtime_origin"], "unknown");
    assert_eq!(run["result"]["release_verdict"]["status"], "blocked");
    let verify = payload(4);
    assert_eq!(verify["ok"], false);
    assert_eq!(verify["error"]["code"], "release_blocked");
    assert_eq!(verify["result"]["execution"]["status"], "passed");
    assert_eq!(verify["result"]["release_verdict"]["status"], "blocked");
    assert_eq!(payload(5)["error"]["code"], "runtime_identity_mismatch");
    let empty = payload(6);
    assert_eq!(empty["result"]["status"], "passed");
    assert_eq!(empty["result"]["scenario_id"], run["result"]["scenario_id"]);
}
