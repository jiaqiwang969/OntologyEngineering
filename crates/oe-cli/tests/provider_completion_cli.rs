//! Actual CLI and literal loopback HTTP only; no live credentials/model calls.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
fn read_body(stream: &mut TcpStream) -> Value {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut header = vec![];
    let mut byte = [0];
    while !header.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        header.push(byte[0]);
        assert!(header.len() < 16384);
    }
    let length = std::str::from_utf8(&header)
        .unwrap()
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}
fn server(replies: Vec<Value>) -> (String, Arc<Mutex<Vec<Value>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/synthetic", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(vec![]));
    let target = requests.clone();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut replies = replies.into_iter();
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    target.lock().unwrap().push(read_body(&mut stream));
                    let Some(reply) = replies.next() else {
                        panic!("unexpected extra synthetic dispatch")
                    };
                    let raw = serde_json::to_vec(&reply).unwrap();
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",raw.len()).unwrap();
                    stream.write_all(&raw).unwrap();
                    if replies.len() == 0 {
                        return;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("synthetic listener {e}"),
            }
        }
    });
    (url, requests, handle)
}
fn invoke(
    command: &str,
    input: Value,
    max_calls: u64,
    replies: Vec<Value>,
) -> (std::process::Output, Value, Vec<Value>) {
    let root = root();
    fs::create_dir_all(root.join("var")).unwrap();
    let case = tempfile::tempdir_in(root.join("var")).unwrap();
    let file = case.path().join("input.json");
    fs::write(&file, serde_json::to_vec(&input).unwrap()).unwrap();
    let secret = case.path().join("synthetic-key");
    fs::write(
        &secret,
        &["sk-", "SYNTHETIC_PROVIDER_COMPLETION_ONLY"].concat(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let (url, requests, server) = server(replies);
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(&root)
        .args(["providers", command, "--input"])
        .arg(&file)
        .arg("--output")
        .arg(case.path().join("result"))
        .arg("--credential-file")
        .arg(secret)
        .arg("--local-synthetic-endpoint")
        .arg(url)
        .arg("--max-calls")
        .arg(max_calls.to_string())
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(
        !String::from_utf8_lossy(&output.stdout)
            .contains(&["sk-", "SYNTHETIC_PROVIDER_COMPLETION_ONLY"].concat())
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr)
            .contains(&["sk-", "SYNTHETIC_PROVIDER_COMPLETION_ONLY"].concat())
    );
    let value=serde_json::from_slice(&output.stdout).unwrap_or_else(|_|json!({"safe_stdout":String::from_utf8_lossy(&output.stdout),"safe_stderr":String::from_utf8_lossy(&output.stderr)}));
    let requests = requests.lock().unwrap().clone();
    (output, value, requests)
}
fn embedding(text: &str) -> Value {
    json!({"schema":"semantica.embedding-request/1","provider":"openai",
    "interface_profile":"provider_store","model":null,"custom_endpoint":false,"input":text,"options":{}})
}
#[test]
fn original_openai_embedding_cli_uses_first_data_and_exact_float32_bits() {
    let mut input = embedding("literal input");
    input["options"] = json!({"model":"synthetic-embed","ignored":42});
    let reply = json!({"model":"synthetic-resolved","data":[{"embedding":[0.1,-0.2,0.25,-0.0,1e100,"NaN","-Infinity",null,true]},{"embedding":[42]}]});
    let (output, value, requests) = invoke("embed", input, 1, vec![reply]);
    assert_eq!(output.status.code(), Some(0), "{value}");
    assert_eq!(
        requests,
        vec![json!({"model":"synthetic-embed","input":"literal input"})]
    );
    let bits: Vec<u64> = vec![
        1036831949, 3192704205, 1048576000, 2147483648, 2139095040, 2143289344, 4286578688,
        2143289344, 1065353216,
    ];
    assert_eq!(
        value["result"]["output"],
        json!({"dtype":"float32","shape":[9],"bits":bits})
    );
    assert_eq!(value["calls"]["attempts"], 1);
    assert_eq!(value["result"]["usage_status"], "unknown");
    assert!(value["result"]["usage"]["input_tokens"].is_null());
    assert_eq!(value["cost"]["status"], "unknown");
    assert_eq!(value["engineering_acceptance"], "not_determined");
    assert!(
        value["provenance_snapshot"]["canonical_nquads"]
            .as_str()
            .unwrap()
            .contains("embedding_generation")
    );
}
#[test]
fn actual_embedding_batch_stops_at_shared_budget_and_keeps_failed_input_identity() {
    let input = json!({"schema":"semantica.embedding-batch-request/1","requests":[embedding("first"),embedding("second"),embedding("third")]});
    let (output, value, requests) = invoke(
        "embed",
        input,
        1,
        vec![json!({"data":[{"embedding":[0.1,0.2]}]})],
    );
    assert_eq!(output.status.code(), Some(2), "{value}");
    assert_eq!(
        requests,
        vec![json!({"model":"text-embedding-3-small","input":"first"})]
    );
    assert_eq!(value["batch"]["failed_index"], 1);
    assert!(value["batch"]["output"].is_null());
    assert_eq!(value["batch"]["results"].as_array().unwrap().len(), 1);
    assert_eq!(value["batch"]["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(value["batch"]["attempts"][1]["request"]["input"], "second");
    assert_eq!(value["batch"]["attempts"][1]["reserved"], false);
    assert_eq!(
        value["batch"]["failure"]["code"],
        "shared_model_call_budget_exhausted"
    );
    assert_eq!(value["calls"]["attempts"], 1);
}
#[test]
fn actual_base_structured_cli_retries_false_and_empty_without_changing_original_prompt() {
    let input = json!({"schema":"oev2.base-structured-request/1","max_retries":3,"generation":{
        "schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract","operation":"generate",
        "model":null,"custom_endpoint":false,"prompt":"please json","options":{}}});
    let replies = ["false", "{}", "{\"x\":1}"]
        .into_iter()
        .map(|s| json!({"choices":[{"message":{"content":s}}]}))
        .collect();
    let (output, value, requests) = invoke("base-structured", input, 3, replies);
    assert_eq!(output.status.code(), Some(0), "{value}");
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert_eq!(
            request,
            json!({"model":"gpt-3.5-turbo","messages":[{"role":"user","content":"please json\n\nReturn the response as valid JSON only."}]})
        );
    }
    assert_eq!(value["result"]["output"], json!({"x":1}));
    assert_eq!(value["result"]["attempts"].as_array().unwrap().len(), 3);
    assert_eq!(value["calls"]["attempts"], 3);
    assert_eq!(value["calls"]["unreported_attempts"], 3);
}
#[test]
fn actual_pool_cli_keeps_hierarchical_short_chunk_source_semantics() {
    let root = root();
    let case = tempfile::tempdir_in(root.join("var")).unwrap();
    let input = case.path().join("pool.json");
    fs::write(
        &input,
        serde_json::to_vec(
            &json!({"schema":"semantica.embedding-pool-request/1","strategy":"hierarchical",
        "embeddings":[[1,2],[3,4],[9,10]],"chunk_size":2}),
        )
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(root)
        .args(["providers", "pool", "--input"])
        .arg(input)
        .arg("--output")
        .arg(case.path().join("result"))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["output"], json!([5.5, 6.5]));
    assert_eq!(
        value["result"]["profile"],
        "semantica.provided-vectors-finite-f64-2d/1"
    );
    assert_eq!(value["execution_kind"], "native_arithmetic");
    assert_eq!(value["fact_admission"], false);
    assert_eq!(value["engineering_acceptance"], "not_determined");
}

#[test]
fn original_negative_base_retries_return_empty_without_dispatch() {
    let root = root();
    let case = tempfile::tempdir_in(root.join("var")).unwrap();
    let input = case.path().join("negative.json");
    fs::write(&input,serde_json::to_vec(&json!({
        "schema":"oev2.base-structured-request/1","max_retries":-1,"generation":{
            "schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract",
            "operation":"generate","model":null,"custom_endpoint":false,"prompt":"JSON","options":{}}})).unwrap()).unwrap();
    let key = case.path().join("synthetic-key");
    fs::write(&key, &["sk-", "SYNTHETIC_NEGATIVE_ONLY"].concat()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/synthetic", listener.local_addr().unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(root)
        .args(["providers", "base-structured", "--input"])
        .arg(input)
        .arg("--output")
        .arg(case.path().join("result"))
        .arg("--credential-file")
        .arg(key)
        .arg("--local-synthetic-endpoint")
        .arg(endpoint)
        .args(["--max-calls", "0"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["output"], json!([]));
    assert_eq!(value["result"]["attempts"], json!([]));
    assert_eq!(value["calls"]["attempts"], 0);
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SYNTHETIC_NEGATIVE_ONLY"));
}
