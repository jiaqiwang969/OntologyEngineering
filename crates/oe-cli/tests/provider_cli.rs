//! Original provider protocol expectations, exercised only against synthetic
//! loopback HTTP. These slice tests do not close the full original API scope.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

fn read_body(stream: &mut TcpStream) -> Vec<u8> {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    while !raw.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        raw.push(byte[0]);
        assert!(raw.len() < 16_384);
    }
    let headers = std::str::from_utf8(&raw).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map(|(_, value)| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    assert!(length < 2 * 1024 * 1024);
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    body
}

#[test]
fn original_openai_custom_structured_protocol_runs_in_local_fixture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    fs::create_dir_all(root.join("var")).unwrap();
    let case = tempfile::tempdir_in(root.join("var")).unwrap();
    let input = case.path().join("input.json");
    fs::write(&input,serde_json::to_vec(&json!({
        "schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract",
        "operation":"generate_structured","model":"test-model","custom_endpoint":true,
        "prompt":"Find entities.","options":{"temperature":null,"max_completion_tokens":12345,"top_p":0.9}
    })).unwrap()).unwrap();
    let secret = case.path().join("synthetic-bearer");
    fs::write(
        &secret,
        &["apikey_", "SYNTHETIC_LOCAL_FIXTURE_NOT_REAL"].concat(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let server_count = count.clone();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    server_count.fetch_add(1, Ordering::SeqCst);
                    let body: Value = serde_json::from_slice(&read_body(&mut stream)).unwrap();
                    assert_eq!(
                        body,
                        json!({"model":"test-model","messages":[{"role":"user","content":"Find entities."}],"max_completion_tokens":12345,"top_p":0.9})
                    );
                    let result=serde_json::to_vec(&json!({"id":"synthetic-provider-response","model":"test-model","choices":[{"message":{"content":"{\"entities\":[{\"text\":\"Bob\",\"label\":\"PERSON\",\"confidence\":0.8}]}"}}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}})).unwrap();
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        result.len()
                    );
                    stream.write_all(header.as_bytes()).unwrap();
                    stream.write_all(&result).unwrap();
                    return;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("synthetic_listener:{e}"),
            }
        }
    });
    let output_path = case.path().join("call");
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(root)
        .args(["providers", "generate", "--input"])
        .arg(input)
        .arg("--output")
        .arg(output_path)
        .arg("--credential-file")
        .arg(secret)
        .arg("--local-synthetic-endpoint")
        .arg(url)
        .args(["--max-calls", "1", "--max-attempts", "1"])
        .output()
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "provider_execution_failed: frozen expectation exit0 unchanged; calls={}; safe stdout={}; stderr={}",
        count.load(Ordering::SeqCst),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "one caller-budgeted HTTP dispatch"
    );
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        answer["result"]["output"],
        json!({"entities":[{"text":"Bob","label":"PERSON","confidence":0.8}]})
    );
    assert_eq!(answer["execution_kind"], "synthetic_loopback");
    assert_eq!(answer["engineering_acceptance"], "not_determined");
}

fn provider_case(
    provider: &str,
    custom: bool,
    content: &str,
    key: &str,
    expected_body: Value,
    max_calls: u64,
) -> (std::process::Output, Value, usize) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let case = tempfile::tempdir_in(root.join("var")).unwrap();
    let input = case.path().join("input.json");
    fs::write(&input,serde_json::to_vec(&json!({"schema":"semantica.provider-request/1","provider":provider,
        "interface_profile":"llms","operation":"generate_structured","model":null,"custom_endpoint":custom,
        "prompt":"Extract entities.","options":{}})).unwrap()).unwrap();
    let secret = case.path().join("synthetic-bearer");
    fs::write(&secret, key).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let content = content.to_owned();
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let server = thread::spawn(move || {
        if max_calls == 0 {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let body: Value = serde_json::from_slice(&read_body(&mut stream)).unwrap();
                    assert_eq!(body, expected_body);
                    let raw=serde_json::to_vec(&json!({"model":"fixture-resolved","choices":[{"message":{"content":content}}]})).unwrap();
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",raw.len()).unwrap();
                    stream.write_all(&raw).unwrap();
                    return;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("local fixture accept failed: {e}"),
            }
        }
    });
    let output_dir = case.path().join("call");
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .arg("--root")
        .arg(root)
        .args(["providers", "generate", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output_dir)
        .arg("--credential-file")
        .arg(&secret)
        .arg("--local-synthetic-endpoint")
        .arg(endpoint)
        .arg("--max-calls")
        .arg(max_calls.to_string())
        .output()
        .unwrap();
    server.join().unwrap();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains(key));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(key));
    let persisted = fs::read(output_dir.join("provider-call.json")).unwrap();
    assert!(!String::from_utf8_lossy(&persisted).contains(key));
    (output, value, count.load(Ordering::SeqCst))
}
#[test]
fn groq_structured_false_is_completed_with_unknown_usage_and_cost() {
    let (output, result, count) = provider_case(
        "groq",
        false,
        "false",
        "gsk_SYNTHETIC_LOCAL_NOT_REAL",
        json!({"model":"llama-3.1-8b-instant","messages":[{"role":"user","content":"Extract entities.\n\nReturn the response as valid JSON only."}],"response_format":{"type":"json_object"}}),
        1,
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(count, 1);
    assert_eq!(result["result"]["output"], false);
    assert_eq!(result["result"]["usage_status"], "unknown");
    assert_eq!(result["result"]["usage"]["input_tokens"], Value::Null);
    assert_eq!(result["calls"]["unreported_attempts"], 1);
    assert_eq!(result["cost"]["status"], "unknown");
    assert_eq!(result["cost"]["amount"], Value::Null);
    assert_eq!(result["fact_admission"], false);
}
#[test]
fn second_json_decode_secret_is_refused_before_any_receipt_or_output_leak() {
    let content = r#"{"private":"\u0073k-SYNTHETIC_LOCAL_NOT_REAL"}"#;
    let (output, result, count) = provider_case(
        "openai",
        true,
        content,
        &["sk-", "SYNTHETIC_LOCAL_NOT_REAL"].concat(),
        json!({"model":"gpt-3.5-turbo","messages":[{"role":"user","content":"Extract entities."}]}),
        1,
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(count, 1);
    assert_eq!(result["error"]["code"], "credential_material_in_response");
    assert_eq!(result["calls"]["attempts"], 1);
    assert_eq!(result["calls"]["unreported_attempts"], 1);
    assert_eq!(result["cost"]["status"], "unknown");
}
#[test]
fn provider_invalid_argument_never_echoes_synthetic_bearer() {
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .args([
            "providers",
            "generate",
            "--api-key",
            &["sk-", "SYNTHETIC_DO_NOT_ECHO"].concat(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !String::from_utf8_lossy(&output.stdout)
            .contains(&["sk-", "SYNTHETIC_DO_NOT_ECHO"].concat())
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr)
            .contains(&["sk-", "SYNTHETIC_DO_NOT_ECHO"].concat())
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], "invalid_arguments");
}
