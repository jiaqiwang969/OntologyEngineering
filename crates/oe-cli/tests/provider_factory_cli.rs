//! Local synthetic credentials and loopback HTTP only; no production SDK calls.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    thread,
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
fn invoke(
    command: &str,
    input: Value,
    endpoint: Option<&str>,
    max_calls: u64,
) -> (std::process::Output, Value) {
    let root = root();
    fs::create_dir_all(root.join("var")).unwrap();
    let case = tempfile::tempdir_in(root.join("var")).unwrap();
    let input_file = case.path().join("input.json");
    fs::write(&input_file, serde_json::to_vec(&input).unwrap()).unwrap();
    let secret = case.path().join("synthetic-bearer");
    fs::write(&secret, &["sk-", "SYNTHETIC_FACTORY_LITELLM_ONLY"].concat()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_oev2"));
    cmd.arg("--root")
        .arg(root)
        .args(["providers", command, "--input"])
        .arg(&input_file)
        .arg("--output")
        .arg(case.path().join("call"))
        .arg("--credential-file")
        .arg(secret)
        .arg("--max-calls")
        .arg(max_calls.to_string());
    if let Some(endpoint) = endpoint {
        cmd.arg("--local-synthetic-endpoint").arg(endpoint);
    }
    let output = cmd.output().unwrap();
    assert!(
        !String::from_utf8_lossy(&output.stdout)
            .contains(&["sk-", "SYNTHETIC_FACTORY_LITELLM_ONLY"].concat())
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr)
            .contains(&["sk-", "SYNTHETIC_FACTORY_LITELLM_ONLY"].concat())
    );
    let result =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|_| json!({"parse_failed":true}));
    (output, result)
}
fn lite(provider: &str, structured: bool) -> Value {
    json!({"generation":{"schema":"semantica.provider-request/1","provider":"litellm","interface_profile":"llms",
        "operation":if structured{"generate_structured"}else{"generate"},"model":format!("{provider}/original"),
        "custom_endpoint":false,"prompt":"JSON original","options":{"temperature":0.2}},
        "config":{"temperature":0.8,"max_tokens":12}})
}
#[test]
fn actual_litellm_cli_sends_original_prompt_merge_without_json_response_format() {
    for provider in ["openai", "groq"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/synthetic", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut header = vec![];
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
                assert!(header.len() < 16384);
            }
            let len = std::str::from_utf8(&header)
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .map(|(_, n)| n.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            let mut body = vec![0; len];
            stream.read_exact(&mut body).unwrap();
            let received: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                received,
                json!({"model":"original","messages":[{"role":"user","content":"JSON original\n\nReturn the response as valid JSON only."}],
                "temperature":0.2,"max_tokens":12})
            );
            let reply = json!({"model":"resolved","choices":[{"message":{"content":"{\"a\":1}"}}]})
                .to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",reply.len()).unwrap();
            stream.write_all(reply.as_bytes()).unwrap();
            body
        });
        let (out, value) = invoke("litellm", lite(provider, true), Some(&endpoint), 1);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{} {}",
            value,
            String::from_utf8_lossy(&out.stderr)
        );
        let actual = server.join().unwrap();
        assert_eq!(value["response"]["result"]["output"], json!({"a":1}));
        assert_eq!(value["response"]["result"]["provider"], provider);
        assert_eq!(
            value["response"]["result"]["requested_model"],
            format!("{provider}/original")
        );
        assert_eq!(
            value["response"]["result"]["wire_request_sha256"],
            oe_core::sha256(&actual)
        );
        assert_ne!(
            value["completion_request"]["body_sha256"],
            value["actual_wire_request"]["body_sha256"]
        );
        assert_eq!(value["calls"]["attempts"], 1);
        assert_eq!(value["calls"]["unreported_attempts"], 1);
        assert_eq!(value["response"]["result"]["usage_status"], "unknown");
        assert_eq!(value["cost"]["status"], "unknown");
        assert_eq!(value["third_party_sdk_dispatch"], "unknown");
        assert_eq!(value["fact_admission"], false);
        assert_eq!(value["engineering_acceptance"], "not_determined");
        assert!(
            value["provenance_snapshot"]["canonical_nquads"]
                .as_str()
                .unwrap()
                .contains(value["completion_request"]["body_sha256"].as_str().unwrap())
        );
    }
}
#[test]
fn actual_litellm_budget_duplicate_keyword_and_unknown_sdk_dispatch_zero_network() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/synthetic", listener.local_addr().unwrap());
    let (out, value) = invoke("litellm", lite("openai", true), Some(&endpoint), 0);
    assert_eq!(out.status.code(), Some(2), "{value}");
    assert_eq!(
        value["response"]["failure"]["code"],
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(value["calls"]["attempts"], 0);
    let mut duplicate = lite("openai", true);
    duplicate["generation"]["options"]["model"] = json!("collision");
    let (out, _) = invoke("litellm", duplicate, Some(&endpoint), 1);
    assert_ne!(out.status.code(), Some(0));
    let (out, _) = invoke("litellm", lite("anthropic", true), Some(&endpoint), 1);
    assert_ne!(out.status.code(), Some(0));
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn actual_litellm_structured_decode_uses_same_pool_guard_after_unicode_unescape() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/synthetic", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut header = vec![];
        let mut byte = [0];
        while !header.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
            assert!(header.len() < 16384);
        }
        let len = std::str::from_utf8(&header)
            .unwrap()
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .map(|(_, n)| n.trim().parse::<usize>().unwrap())
            })
            .unwrap();
        let mut body = vec![0; len];
        stream.read_exact(&mut body).unwrap();
        let encoded = ["sk-", "SYNTHETIC_FACTORY_LITELLM_ONLY"]
            .concat()
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect::<String>();
        let content = format!("{{\"unexpected\":\"{encoded}\"}}");
        let reply = json!({"choices":[{"message":{"content":content}}]}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",reply.len()).unwrap();
        stream.write_all(reply.as_bytes()).unwrap();
    });
    let (out, value) = invoke("litellm", lite("openai", true), Some(&endpoint), 1);
    server.join().unwrap();
    assert_eq!(out.status.code(), Some(2), "{value}");
    assert_eq!(
        value["response"]["failure"]["code"],
        "credential_material_in_response"
    );
    assert_eq!(value["calls"]["attempts"], 1);
    assert!(value["response"].get("result").is_none());
}
fn command(operation: &str, name: Value, kwargs: Value, id: Option<&str>) -> Value {
    json!({"schema":"semantica.provider-factory-request/1","operation":operation,"name":name,
        "constructor_id":id,"kwargs":kwargs,"kwargs_profile":"json/1","use_pool":true})
}
#[test]
fn actual_factory_cli_reuses_then_clears_without_borrowing_sdk_availability_or_budget() {
    let input = json!({"schema":"semantica.provider-factory-plan/1","commands":[
        command("create_provider",json!("OPENAI"),json!({"model":"selected"}),None),
        command("create_provider",json!("OPENAI"),json!({"model":"selected"}),None),
        command("register",json!("custom"),json!({}),Some("caller_custom")),
        command("get",json!("custom"),json!({}),None),
        command("list_all",Value::Null,json!({}),None),
        command("clear",Value::Null,json!({}),None),
        command("create_provider",json!("OPENAI"),json!({"model":"selected"}),None),
        command("unregister",json!("custom"),json!({}),None),
        command("list_all",Value::Null,json!({}),None)]});
    let (out, value) = invoke("factory", input, None, 0);
    assert_eq!(out.status.code(), Some(0), "{value}");
    let observations = value["observations"].as_array().unwrap();
    assert_eq!(observations[0]["result"]["instance"]["backend"], "openai");
    assert_eq!(
        observations[0]["result"]["instance"]["default_model"],
        "selected"
    );
    assert!(observations[0]["result"]["instance"]["available"].is_null());
    assert_eq!(
        observations[0]["result"]["instance"]["instance_id"],
        observations[1]["result"]["instance"]["instance_id"]
    );
    assert_eq!(observations[1]["result"]["cache_hit"], true);
    assert_eq!(
        observations[3]["result"]["binding"]["constructor_id"],
        "caller_custom"
    );
    assert_eq!(observations[4]["result"]["names"], json!(["custom"]));
    assert_ne!(
        observations[0]["result"]["instance"]["instance_id"],
        observations[6]["result"]["instance"]["instance_id"]
    );
    assert_eq!(observations[8]["result"]["names"], json!([]));
    assert_eq!(value["calls"]["attempts"], 0);
    assert_eq!(value["network_dispatches"], 0);
    assert_eq!(value["sdk_initialization"], "not_observed");
    assert_eq!(value["fact_admission"], false);
}
