//! Actual literal localhost HTTP on independent synthetic registered identity
//! fixtures. It proves source/typed/transport guards, never PDF transcription,
//! external inference quality or engineering adoption.
use oe_contracts::{KnowledgeTypedExtractionInput8 as Input, SharedModelCallPolicy};
use oe_core::{SkillRoot, sha256};
use oe_engineering::{
    harness::SharedModelCallContext,
    typed_extraction8::{check_post8, run_source8},
};
use oe_knowledge::typed_source8::{parse_input8, prepare_native8};
use oe_model_credentials::{CredentialPool, provider_transport::ProviderHttpTransport};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
fn gold() -> Value {
    serde_json::from_str(include_str!(
        "../../semantica-runtime/tests/fixtures/typed_relations8_original.json"
    ))
    .unwrap()
}
fn fixture(temporal: bool) -> (tempfile::TempDir, SkillRoot, Input, PathBuf) {
    fixture_text(temporal, gold()["text"].as_str().unwrap())
}
fn fixture_text(temporal: bool, body: &str) -> (tempfile::TempDir, SkillRoot, Input, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    let index = base.join("sources/.indexes/local-pdf");
    fs::create_dir_all(&index).unwrap();
    fs::create_dir_all(base.join("sources/books")).unwrap();
    fs::write(
        base.join("SKILL.md"),
        "synthetic source/HTTP guard fixture; not engineering or PDF transcription proof",
    )
    .unwrap();
    let doc = base.join("sources/books/fixture.pdf");
    let bytes = b"explicit synthetic source identity fixture, not parsed as PDF";
    fs::write(&doc, bytes).unwrap();
    let text = body.to_owned();
    let mut registration = json!({"schema":"ontology-engineering.local-pdf-registration/v1","sources":[{"id":"fixture","title":"synthetic source identity fixture","edition":"independent","path":"../../books/fixture.pdf","sha256":sha256(bytes),"bytes":bytes.len(),"page_count":1,"chapters":[]}]});
    let db = Connection::open(index.join("catalog.sqlite3")).unwrap();
    db.execute_batch(oe_knowledge::registration::TABLES)
        .unwrap();
    db.execute(
        "INSERT INTO pages VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            "local_pdf:fixture:page:1",
            "local_pdf:fixture",
            1,
            1,
            Option::<String>::None,
            Option::<String>::None,
            "",
            "",
            sha256(bytes),
            bytes.len() as i64,
            &text,
            sha256(&text),
            sha256(&text),
            text.chars().count() as i64,
            text.chars().count() as i64,
            0,
            "[]",
            "[]",
            0,
            "{\"document_id\":\"fixture\"}"
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO search(rowid,models,titles,body,chapter) VALUES(1,'','',?,'')",
        [&text],
    )
    .unwrap();
    let mut meta = json!({"schema":"ontology-engineering.local-pdf-index/v1","registration_sha256":oe_core::json::canonical_digest(&registration).unwrap()});
    meta["fingerprint"] = json!(oe_core::json::canonical_digest(&meta).unwrap());
    for (k, v) in meta.as_object().unwrap() {
        db.execute("INSERT INTO meta VALUES(?,?)", params![k, v.to_string()])
            .unwrap();
    }
    drop(db);
    registration["index_sha256"] = json!(
        oe_core::fingerprint_file(&index.join("catalog.sqlite3"), u64::MAX)
            .unwrap()
            .sha256
    );
    fs::write(
        index.join("registration.json"),
        serde_json::to_vec(&registration).unwrap(),
    )
    .unwrap();
    let root = SkillRoot::new(base).unwrap();
    let input=parse_input8(&json!({"schema":"oev2.source-bound-typed-extraction-input/8","source_ref":"synthetic-only","page_id":"local_pdf:fixture:page:1","document_sha256":sha256(bytes),"text_sha256":sha256(&text),"raw_text_sha256":sha256(&text),"physical_page":1,"page_count":1,"channel":"native","selected_text_sha256":sha256(&text),"start_byte":0,"end_byte":text.len().max(1),"generation":{"schema":"semantica.provider-request/1","provider":"openai","interface_profile":"semantic_extract","operation":"generate","model":"fixture-only","custom_endpoint":false,"prompt":"","options":{"max_tokens":1024}},"entities":gold()["entities"],"relation_types":[],"temporal":temporal,"max_retries":2,"maximum_request_bytes":65536,"context_window_tokens":131072,"reserved_output_tokens":1024})).unwrap();
    (dir, root, input, doc)
}
fn pool(dir: &std::path::Path) -> CredentialPool {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join("explicit-synthetic-credential.txt");
    fs::write(&p, "synthetic-fixture-only-local-secret\n").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    CredentialPool::read_single_http_bearer(&p).unwrap()
}
fn server(
    replies: Vec<Value>,
    mutate: Option<PathBuf>,
) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut requests = Vec::new();
        for reply in replies {
            let mut socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "bounded synthetic server never received request"
                        );
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            // Darwin accepted sockets inherit the listener nonblocking flag.
            // The fixture parses a complete bounded request with blocking reads.
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut raw = Vec::new();
            let header_end = loop {
                let mut b = [0; 1024];
                let n = socket.read(&mut b).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&b[..n]);
                if let Some(i) = raw.windows(4).position(|x| x == b"\r\n\r\n") {
                    break i + 4;
                }
                assert!(raw.len() < 16 * 1024);
            };
            let header = std::str::from_utf8(&raw[..header_end]).unwrap();
            let length = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            while raw.len() < header_end + length {
                let mut b = [0; 4096];
                let n = socket.read(&mut b).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&b[..n]);
            }
            let body = &raw[header_end..header_end + length];
            requests.push(json!({"request_body":serde_json::from_slice::<Value>(body).unwrap(),"request_body_bytes":length,"request_body_sha256":sha256(body),"authorization":"not_recorded"}));
            if let Some(p) = mutate.as_ref() {
                fs::write(p, "changed after real HTTP request, before response").unwrap();
            }
            let content = serde_json::to_string(&reply).unwrap();
            let data =
                serde_json::to_vec(&json!({"choices":[{"message":{"content":content}}]})).unwrap();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",data.len()).as_bytes()).unwrap();
            socket.write_all(&data).unwrap();
        }
        requests
    });
    (endpoint, handle)
}
fn save(tag: &str, value: &Value) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("var/typed-extraction8-http-evidence");
    fs::create_dir_all(&base).unwrap();
    oe_core::write_new(
        &base.join(format!("{tag}-{}.json", std::process::id())),
        value,
    )
    .unwrap();
}
fn actual(temporal: bool, replies: Vec<Value>, mutate: bool, tag: &str) -> Value {
    let (dir, root, input, doc) = fixture(temporal);
    let source = prepare_native8(&root, &input, Instant::now() + Duration::from_secs(120)).unwrap();
    let credentials = pool(dir.path());
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: 4 }).unwrap();
    let (endpoint, thread) = server(replies, mutate.then_some(doc));
    let transport = ProviderHttpTransport::local_synthetic(
        &credentials,
        "openai",
        &endpoint,
        false,
        Duration::from_secs(10),
    )
    .unwrap();
    let result = run_source8(&source, &calls, &transport, &transport).unwrap();
    let requests = thread.join().unwrap();
    let value = json!({"scope":"actual synthetic registered source identity and literal localhost HTTP, not PDF transcription or external inference","receipt":result,"actual_server_requests":requests});
    save(tag, &value);
    value
}
#[test]
fn original_relations_through_real_HTTP_same_dispatch() {
    let v = actual(
        false,
        vec![gold()["typed_original_output"].clone()],
        false,
        "ordinary",
    );
    assert_eq!(v["receipt"]["execution"], "completed");
    assert_eq!(v["actual_server_requests"].as_array().unwrap().len(), 1);
    assert_eq!(
        v["receipt"]["outcome"]["relations"][0],
        gold()["expected_relation"]
    );
    assert_eq!(
        v["receipt"]["wire_observations"][0]["response"]["transport_kind"],
        "synthetic_loopback"
    );
}
#[test]
fn original_temporal_fields_through_real_HTTP() {
    let v = actual(
        true,
        vec![gold()["temporal_original_output"].clone()],
        false,
        "temporal",
    );
    assert_eq!(v["receipt"]["execution"], "completed");
    assert_eq!(
        v["receipt"]["outcome"]["relations"][0]["metadata"],
        gold()["temporal_expected_metadata"]
    );
}
#[test]
fn actual_schema_feedback_repair_preserves_first_response() {
    let v = actual(
        false,
        vec![
            json!({"relations":"invalid array"}),
            gold()["typed_original_output"].clone(),
        ],
        false,
        "schema-repair",
    );
    assert_eq!(v["receipt"]["execution"], "completed");
    assert_eq!(v["actual_server_requests"].as_array().unwrap().len(), 2);
    assert_eq!(
        v["receipt"]["outcome"]["typed"]["attempts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(v["receipt"]["outcome"]["typed"]["attempts"][0]["validation"]["valid"] == false);
}
#[test]
fn actual_empty_typed_fallback_uses_same_calls() {
    let v = actual(
        false,
        vec![
            json!({"relations":[]}),
            gold()["typed_original_output"].clone(),
        ],
        false,
        "empty-fallback",
    );
    assert_eq!(v["receipt"]["execution"], "completed");
    assert_eq!(v["actual_server_requests"].as_array().unwrap().len(), 2);
    assert_eq!(v["receipt"]["calls"]["attempts"], 2);
    assert!(v["receipt"]["outcome"]["fallback"]["reserved"] == true);
}
#[test]
fn source_drift_after_real_response_keeps_raw_body_and_refuses() {
    let v = actual(
        false,
        vec![gold()["typed_original_output"].clone()],
        true,
        "post-drift",
    );
    assert_eq!(v["receipt"]["execution"], "failed");
    assert_eq!(v["receipt"]["source_current"], false);
    assert!(
        v["receipt"]["wire_observations"][0]["response"]["body"]
            .as_str()
            .unwrap()
            .contains("acquired")
    );
    assert_eq!(v["actual_server_requests"].as_array().unwrap().len(), 1);
}
#[test]
fn actual_shared_zero_call_and_dual_budget_never_HTTP() {
    for (tag, cap, bytes, tokens) in [
        ("zero-calls", 0, 65536, 131072),
        ("byte-cap", 1, 10, 131072),
        ("token-cap", 1, 65536, 10),
    ] {
        let (dir, root, mut input, _) = fixture(false);
        input.maximum_request_bytes = bytes;
        input.context_window_tokens = tokens;
        let source =
            prepare_native8(&root, &input, Instant::now() + Duration::from_secs(120)).unwrap();
        let credentials = pool(dir.path());
        let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls: cap }).unwrap();
        let transport = ProviderHttpTransport::local_synthetic(
            &credentials,
            "openai",
            "http://127.0.0.1:9/v1/chat/completions",
            false,
            Duration::from_secs(1),
        )
        .unwrap();
        let result = run_source8(&source, &calls, &transport, &transport).unwrap();
        assert_eq!(result.execution, "failed");
        assert!(
            result
                .wire_observations
                .as_array()
                .unwrap()
                .iter()
                .all(|x| x["http_started"] == false)
        );
        save(tag, &serde_json::to_value(result).unwrap());
    }
}
#[test]
fn source_before_post_and_expired_deadline_refuse_without_HTTP() {
    let (_dir, root, input, doc) = fixture(false);
    assert!(prepare_native8(&root, &input, Instant::now()).is_err());
    let source = prepare_native8(&root, &input, Instant::now() + Duration::from_secs(120)).unwrap();
    fs::write(doc, "changed before send").unwrap();
    let wire = oe_semantic::providers::prepare_request(&input.generation).unwrap();
    assert!(check_post8(&source, &wire).is_err());
}

#[test]
fn current_missing_native_body_and_fake_ocr_refuse() {
    let (_dir, root, input, _) = fixture_text(false, "");
    let error = prepare_native8(&root, &input, Instant::now() + Duration::from_secs(120))
        .err()
        .unwrap();
    assert!(error.to_string().contains("native_body_missing"));
    let (_dir, root, mut input, _) = fixture(false);
    input.channel = "ocr".into();
    let error = prepare_native8(&root, &input, Instant::now() + Duration::from_secs(120))
        .err()
        .unwrap();
    assert!(error.to_string().contains("actual_opaque6_required"));
    input.channel = "native".into();
    input.source_ref = "".into();
    assert!(prepare_native8(&root, &input, Instant::now() + Duration::from_secs(120)).is_err());
}

#[test]
fn accepted_http_connection_waits_for_delayed_request_bytes() {
    let (endpoint, worker) = server(vec![json!({"relations": []})], None);
    let address = endpoint
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    let mut socket = std::net::TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    // Ensure the server can accept before the first request bytes arrive.
    thread::sleep(Duration::from_millis(100));
    socket.write_all(b"POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    let requests = worker.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["request_body"], json!({}));
    assert_eq!(requests[0]["request_body_bytes"], 2);
}
