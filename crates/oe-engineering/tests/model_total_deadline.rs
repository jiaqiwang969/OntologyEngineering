//! Independently frozen total-deadline overlay; no production source is changed.
//! No mock transport, public network, real credentials or adoption evidence.
#![cfg(unix)]
use oe_contracts::{ModelServiceProfile, ObjectIdentity};
use oe_core::{SkillRoot, sha256};
use oe_engineering::model_assistance::{self, transport::HttpModelTransport};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
    time::Instant,
};
const ROOT_DUMMY: &str = "synthetic-root-anchor-only-credential";
const CREDENTIAL: &str = "var/fixture-bearer.txt";
fn save(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn reference(root: &Path, relative: &str, role: Option<&str>) -> Value {
    let mut value =
        json!({"path":relative,"sha256":sha256(fs::read(root.join(relative)).unwrap())});
    if let Some(role) = role {
        value["role"] = json!(role);
    }
    value
}
fn credential(path: &Path, dummy: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("{dummy}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn inputs(root: &Path, endpoint: &str) {
    fs::write(
        root.join("var/support.txt"),
        "Source says nominal contact does not establish holding capacity.\n",
    )
    .unwrap();
    let source_sha = sha256(fs::read(root.join("var/support.txt")).unwrap());
    let object = ObjectIdentity {
        object_id: "urn:fixture:support".into(),
        revision: "r1".into(),
        condition_scope: "holding".into(),
        source_sha256: source_sha.clone(),
    };
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":object.object_id,
        "object_revision":"r1","purpose":"Review support before manufacture",
        "acceptance":["retain holding capacity unknown"],"facts":[],"assertions":[]});
    let project: Project = serde_json::from_value(json!({"schema":"oev2.project/1",
        "project_id":"credential-anchor-fixture","revision":"r1","purpose":"Review support before manufacture",
        "objects":{object.object_id.clone():"r1"},"conditions":{"holding":serde_json::to_string(&basis).unwrap()},
        "evidence":{"fixture-source":source_sha},"decisions":[],"dependencies":{}})).unwrap();
    save(
        &root.join("var/project.json"),
        &serde_json::to_value(&project).unwrap(),
    );
    save(
        &root.join("var/original.json"),
        &json!({"schema":"oev2.advisory-questions/1",
        "state":{"purpose":"Review support before manufacture","unknowns":["holding capacity unmeasured"],
            "source_text":"Nominal contact is not physical holding."},
        "questions":{"book":{"type":"choice","instructions":"Is a source review needed?",
            "criteria":{"needed":"Needed source review","not_needed":"No source review needed","unknown":"Cannot decide"}}}}),
    );
    save(
        &root.join("var/input.json"),
        &json!({"schema":"oev2.model-assistance-input/1",
        "project":reference(root,"var/project.json",None),"object":object,
        "project_context":project::context(&project,&object).unwrap(),"purpose":project.purpose,
        "task":"Identify required holding evidence, without inventing a load.",
        "original_requests":[reference(root,"var/original.json",Some("historical_model_request"))],
        "support_files":[reference(root,"var/support.txt",Some("current_support"))]}),
    );
    save(
        &root.join("var/profile.json"),
        &json!({"schema":"oev2.model-service-profile/1",
        "service_id":"synthetic-credential-anchor-service","api_kind":"openai_responses","endpoint":endpoint,
        "requested_model":"fixture-large-context-1","accepted_response_models":["fixture-large-context-1"],
        "context_window_tokens":2000000,"maximum_request_bytes":1000000,"maximum_output_tokens":2048,
        "maximum_response_bytes":1000000,"wrapper_reserve_tokens":4096,"timeout_seconds":5,
        "credential_file":CREDENTIAL,"capacity_evidence":"Synthetic caller declaration; no provider capacity validation"}),
    );
}
fn response() -> Value {
    let advice = json!({"schema":"oev2.model-advisory/1",
        "summary":"Retain unknown holding capacity and obtain bounded evidence.",
        "answers":[{"request_index":0,"question_id":"book","selected_option":"needed",
            "explanation":"Source review is needed before adopting the support.","source_refs":["fixture-source"],
            "unknowns":["unmeasured load"],"verification_required":["verify current original source and measure holding"]}],
        "findings":[{"kind":"missing_condition","claim":"Holding capacity and required load are not established.",
            "source_refs":["fixture-source"],"assumptions":[],"unknowns":["load and holding capacity"],
            "verification_required":["obtain current load and holding test"]}]});
    json!({"id":"fixture-response","model":"fixture-large-context-1","status":"completed",
        "error":null,"incomplete_details":null,
        "output":[{"type":"message","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":serde_json::to_string(&advice).unwrap()}]}],
        "usage":{"input_tokens":100,"output_tokens":50,"total_tokens":150}})
}

struct Gateway {
    endpoint: String,
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<Option<Value>>,
}
impl Gateway {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let thread = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(15);
            while Instant::now() < until && !signal.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("loopback accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut raw = Vec::new();
                let header_end = loop {
                    let mut chunk = [0_u8; 4096];
                    let length = stream.read(&mut chunk).unwrap();
                    assert_ne!(length, 0, "incomplete loopback request headers");
                    raw.extend_from_slice(&chunk[..length]);
                    if let Some(start) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break start + 4;
                    }
                    assert!(raw.len() < 16_384, "loopback request header resource bound");
                };
                let headers = std::str::from_utf8(&raw[..header_end]).unwrap();
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .expect("bounded loopback request must declare content length");
                assert!(length <= 1_000_000);
                let accepted = headers.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("authorization")
                            && value.trim() == format!("Bearer {ROOT_DUMMY}")
                    })
                });
                while raw.len() < header_end + length {
                    let mut chunk = [0_u8; 4096];
                    let length = stream.read(&mut chunk).unwrap();
                    assert_ne!(length, 0, "incomplete loopback request body");
                    raw.extend_from_slice(&chunk[..length]);
                }
                let request: Value =
                    serde_json::from_slice(&raw[header_end..header_end + length]).unwrap();
                let (status, body) = if accepted {
                    ("200 OK", serde_json::to_vec(&response()).unwrap())
                } else {
                    (
                        "401 Unauthorized",
                        b"{\"error\":\"synthetic_wrong_path\"}".to_vec(),
                    )
                };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(&body).unwrap();
                stream.flush().unwrap();
                return Some(
                    json!({"requests_received":1,"root_dummy_accepted":accepted,"response_body_bytes":body.len(),
                    "requested_model":request["model"],"store":request["store"],"truncation":request["truncation"]}),
                );
            }
            None
        });
        Self {
            endpoint,
            stop,
            thread,
        }
    }
    fn finish(self) -> Option<Value> {
        self.stop.store(true, Ordering::Release);
        self.thread.join().unwrap()
    }
}

fn protected(root: &Path) -> Vec<(PathBuf, Vec<u8>, u32)> {
    [
        "var/project.json",
        "var/original.json",
        "var/support.txt",
        "var/input.json",
        "var/profile.json",
        CREDENTIAL,
    ]
    .into_iter()
    .filter_map(|relative| {
        let path = root.join(relative);
        path.exists().then(|| {
            (
                path.clone(),
                fs::read(&path).unwrap(),
                fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            )
        })
    })
    .collect()
}
fn unchanged(files: &[(PathBuf, Vec<u8>, u32)]) {
    for (path, original, mode) in files {
        assert_eq!(fs::read(path).unwrap(), *original);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o7777,
            *mode
        );
    }
}

fn setup(gateway: &Gateway, maximum: u64) -> SkillRoot {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let parent = source.join("var/tests/unified-system/stale-actual-http-profile-tests");
    fs::create_dir_all(&parent).unwrap();
    // Retain private actual input bytes, archived current profile and process
    // observations after the test, including the RED case.
    let path = tempfile::tempdir_in(parent).unwrap().keep();
    fs::write(
        path.join("SKILL.md"),
        "Synthetic bounded actual HTTP profile regression",
    )
    .unwrap();
    fs::create_dir_all(path.join("var")).unwrap();
    inputs(&path, &gateway.endpoint);
    credential(&path.join(CREDENTIAL), ROOT_DUMMY);
    let root = SkillRoot::new(path).unwrap();
    update_maximum(&root, maximum);
    root
}
fn read_profile(root: &SkillRoot) -> ModelServiceProfile {
    let profile: ModelServiceProfile =
        serde_json::from_slice(&fs::read(root.path().join("var/profile.json")).unwrap()).unwrap();
    profile.validate().unwrap();
    profile
}
fn update_maximum(root: &SkillRoot, maximum: u64) {
    let path = root.path().join("var/profile.json");
    let mut profile: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    profile["maximum_response_bytes"] = json!(maximum);
    save(&path, &profile);
}
fn actual_adapter(root: &SkillRoot) -> HttpModelTransport {
    let profile = read_profile(root);
    HttpModelTransport::new(
        &profile.endpoint,
        &root.expand(Path::new(&profile.credential_file)).unwrap(),
        Duration::from_secs(profile.timeout_seconds),
        profile.maximum_response_bytes,
    )
    .unwrap()
}
fn prepared(root: &SkillRoot, maximum: u64) {
    let profile = read_profile(root);
    assert_eq!(profile.maximum_response_bytes, maximum);
    let observed = model_assistance::prepare(
        root,
        Path::new("var/input.json"),
        Path::new("var/profile.json"),
    )
    .unwrap();
    assert!(
        observed.admitted(),
        "valid fixture must be admitted before the transport guard"
    );
    assert_eq!(observed.profile, profile);
    assert_eq!(
        observed.report["profile_sha256"],
        sha256(fs::read(root.path().join("var/profile.json")).unwrap())
    );
}
fn actual_run(
    root: &SkillRoot,
    adapter: &HttpModelTransport,
    gateway: Gateway,
    case: &str,
) -> (Value, Option<Value>) {
    let before = protected(root.path());
    let result = model_assistance::run(
        root,
        Path::new("var/input.json"),
        Path::new("var/profile.json"),
        Path::new("var/execution"),
        Some(adapter),
    )
    .unwrap();
    let gateway = gateway.finish();
    unchanged(&before);
    assert_eq!(result["adopted"], false);
    assert_eq!(result["project_update"], "none");
    assert_eq!(result["source_binding_current"], true);
    assert_eq!(
        result["plan"]["profile_sha256"],
        sha256(fs::read(root.path().join("var/profile.json")).unwrap())
    );
    let observation = json!({"schema":"independent-stale-actual-http-profile-observation/1","case":case,
        "current_maximum_response_bytes":read_profile(root).maximum_response_bytes,
        "result":result,"actual_transport":adapter.observation(),"gateway":gateway,
        "private_fixture":root.path(),"protected_bytes_and_modes_unchanged":true,
        "real_http":true,"mock_transport":false,"engineering_adoption":"not_established"});
    save(
        &root.path().join("var/actual-observation.json"),
        &observation,
    );
    println!("{}", observation);
    (result, gateway)
}

#[derive(Clone, Copy)]
struct Schedule {
    header_delay: Duration,
    body_interval: Duration,
    chunk_count: usize,
}
impl Gateway {
    fn scheduled(schedule: Schedule) -> Self {
        assert!(schedule.chunk_count > 0);
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let thread = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(15);
            while Instant::now() < until && !signal.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("scheduled loopback accept failed: {error}"),
                };
                stream.set_nodelay(true).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut raw = Vec::new();
                let header_end = loop {
                    let mut chunk = [0_u8; 4096];
                    let length = stream.read(&mut chunk).unwrap();
                    assert_ne!(length, 0, "incomplete scheduled request headers");
                    raw.extend_from_slice(&chunk[..length]);
                    if let Some(start) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break start + 4;
                    }
                    assert!(raw.len() < 16_384);
                };
                let headers = std::str::from_utf8(&raw[..header_end]).unwrap();
                assert!(headers.starts_with("POST /v1/responses HTTP/1.1\r\n"));
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                assert!(length <= 1_000_000);
                let accepted = headers.lines().any(|line| {
                    line.split_once(':').is_some_and(|(name, value)| {
                        name.eq_ignore_ascii_case("authorization")
                            && value.trim() == format!("Bearer {ROOT_DUMMY}")
                    })
                });
                while raw.len() < header_end + length {
                    let mut chunk = [0_u8; 4096];
                    let length = stream.read(&mut chunk).unwrap();
                    assert_ne!(length, 0, "incomplete scheduled request body");
                    raw.extend_from_slice(&chunk[..length]);
                }
                let request_raw = &raw[header_end..header_end + length];
                let request: Value = serde_json::from_slice(request_raw).unwrap();
                let (status, body) = if accepted {
                    ("200 OK", serde_json::to_vec(&response()).unwrap())
                } else {
                    (
                        "401 Unauthorized",
                        b"{\"error\":\"synthetic_wrong_path\"}".to_vec(),
                    )
                };
                let width = body.len().div_ceil(schedule.chunk_count);
                assert_eq!(body.chunks(width).count(), schedule.chunk_count);
                thread::sleep(schedule.header_delay);
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let header_written = stream
                    .write_all(header.as_bytes())
                    .and_then(|_| stream.flush())
                    .is_ok();
                let mut chunks_written = 0_u64;
                let mut body_bytes_written = 0_u64;
                let mut write_error = None;
                if header_written {
                    for (index, chunk) in body.chunks(width).enumerate() {
                        if index > 0 {
                            thread::sleep(schedule.body_interval);
                        }
                        match stream.write_all(chunk).and_then(|_| stream.flush()) {
                            Ok(()) => {
                                chunks_written += 1;
                                body_bytes_written += chunk.len() as u64;
                            }
                            Err(error) => {
                                // A deadline client may close before the remaining body.
                                // Preserve the server-side write outcome; never panic or
                                // manufacture that remaining bytes reached the client.
                                write_error = Some(format!("{:?}", error.kind()));
                                break;
                            }
                        }
                    }
                }
                assert!(
                    matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
                    "unexpected second actual POST"
                );
                return Some(json!({
                    "requests_received": 1,
                    "request_sha256": sha256(request_raw),
                    "root_dummy_accepted": accepted,
                    "response_body_bytes": body.len(),
                    "planned_chunk_count": schedule.chunk_count,
                    "chunk_width": width,
                    "header_delay_ms": schedule.header_delay.as_millis() as u64,
                    "body_interval_ms": schedule.body_interval.as_millis() as u64,
                    "server_header_write_completed": header_written,
                    "server_body_chunks_written": chunks_written,
                    "server_body_bytes_written": body_bytes_written,
                    "server_write_error_kind": write_error,
                    "requested_model": request["model"],
                    "store": request["store"],
                    "stream": request["stream"],
                    "truncation": request["truncation"]
                }));
            }
            None
        });
        Self {
            endpoint,
            stop,
            thread,
        }
    }
}
fn one_second_profile(root: &SkillRoot) {
    let path = root.path().join("var/profile.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["timeout_seconds"] = json!(1);
    save(&path, &value);
}
fn run_deadline_case(case: &str, schedule: Schedule) -> (Value, Value, Value, Value) {
    let gateway = Gateway::scheduled(schedule);
    let root = setup(&gateway, 1_000_000);
    one_second_profile(&root);
    prepared(&root, 1_000_000);
    let profile = read_profile(&root);
    assert_eq!(profile.timeout_seconds, 1);
    assert_eq!(profile.maximum_response_bytes, 1_000_000);
    let adapter = actual_adapter(&root);
    // The original factory compares endpoint, expanded native credential
    // pathname, exact Duration and response-byte cap; raw180ms is not used.
    adapter.verify_current_profile(&root, &profile).unwrap();
    let before = protected(root.path());
    let result = model_assistance::run(
        &root,
        Path::new("var/input.json"),
        Path::new("var/profile.json"),
        Path::new("var/execution"),
        Some(&adapter),
    )
    .unwrap();
    let gateway = gateway.finish().expect("one actual scheduled POST");
    unchanged(&before);
    assert_eq!(result["transport_configuration_status"], "matched");
    assert_eq!(result["client_dispatch_attempts"], 1);
    assert_eq!(result["automatic_retries"], 0);
    assert_eq!(result["automatic_fallback"], false);
    assert_eq!(result["adopted"], false);
    assert_eq!(result["project_update"], "none");
    assert_eq!(result["source_binding_current"], true);
    assert_eq!(result["kernel_source_diagnostics_stable"], true);
    assert_eq!(
        result["plan"]["profile_sha256"],
        sha256(fs::read(root.path().join("var/profile.json")).unwrap())
    );
    assert_eq!(gateway["requests_received"], 1);
    assert_eq!(gateway["root_dummy_accepted"], true);
    assert_eq!(gateway["requested_model"], "fixture-large-context-1");
    assert_eq!(gateway["store"], false);
    assert_eq!(gateway["stream"], false);
    assert_eq!(gateway["truncation"], "disabled");
    assert_eq!(
        gateway["request_sha256"],
        sha256(fs::read(root.path().join("var/execution/request.json")).unwrap())
    );
    let legacy = adapter.observation();
    assert_eq!(legacy["client_dispatch_attempts"], 1);
    assert_eq!(legacy["send_call_attempts"], 1);
    assert_eq!(legacy["upstream_attempts"], "unknown");
    let typed_wait = adapter.wait_observation();
    typed_wait.validate().unwrap();
    for attempt in typed_wait.attempts.as_ref().unwrap() {
        attempt.validate().unwrap();
    }
    let wait = serde_json::to_value(typed_wait).unwrap();
    assert_eq!(wait["schema"], "oev2.responses-http-wait-observation/1");
    assert_eq!(wait["timing_scope"], "client_api_boundaries");
    assert_eq!(wait["attempts"].as_array().unwrap().len(), 1);
    for field in [
        "dns_tls_timing",
        "server_processing_timing",
        "upstream_arrival",
        "usage_cost",
    ] {
        assert_eq!(wait[field], "unknown");
    }
    let mut previous = 0;
    for field in [
        "serialization_finished_us",
        "dispatch_api_started_us",
        "dispatch_api_finished_us",
        "headers_observed_us",
        "body_read_started_us",
        "body_read_finished_us",
        "terminal_us",
    ] {
        if let Some(current) = wait["attempts"][0][field].as_u64() {
            assert!(current >= previous, "{field} out of order");
            previous = current;
        }
    }
    assert!(!wait.to_string().contains(ROOT_DUMMY));
    assert!(!wait.to_string().contains(&profile.endpoint));
    assert!(!wait.to_string().contains("fixture-source"));
    save(
        &root.path().join("var/deadline-wait-observation.json"),
        &wait,
    );
    save(
        &root.path().join("var/deadline-case-observation.json"),
        &json!({
            "schema": "independent-total-deadline-case/1",
            "case": case,
            "result": result,
            "legacy_observation": legacy,
            "wait_observation": wait,
            "gateway": gateway,
            "protected_bytes_and_modes_unchanged": true,
            "factory_configuration_status": "matched",
            "engineering_adoption": "not_established"
        }),
    );
    // The nocapture log contains only safe generated client timing, never
    // payload, source identifiers, endpoint, dummy credential or server time.
    println!("{}", json!({"case":case,"wait_observation":wait}));
    (result, legacy, wait, gateway)
}
fn partial_body_deadline_failure(result: &Value, legacy: &Value, wait: &Value, gateway: &Value) {
    assert_eq!(
        result["status"], "failed",
        "new total deadline must fail; old per-operation completion is the retained RED"
    );
    assert_eq!(result["model_execution"], "failed");
    assert_eq!(result["transport_error"]["code"], "response_read_failed");
    assert_eq!(result["transport_error"]["http_status"], 200);
    assert_eq!(result["advisory"], Value::Null);
    assert_eq!(result["usage"]["status"], "unknown");
    assert_eq!(result["usage"]["cost"], "unknown");
    let attempt = &legacy["attempts"][0];
    assert_eq!(attempt["http_status"], 200);
    assert_eq!(attempt["response_read_status"], "failed");
    assert_eq!(attempt["response_json_status"], "not_parsed");
    assert_eq!(attempt["safe_error"], "response_read_failed");
    let partial = attempt["response_bytes_read"].as_u64().unwrap();
    let full = gateway["response_body_bytes"].as_u64().unwrap();
    assert!(
        partial > 0 && partial < full,
        "actual body must remain partial"
    );
    assert_eq!(wait["attempts"][0]["terminal_stage"], "body_read");
    assert_eq!(wait["attempts"][0]["http_status"], 200);
    assert_eq!(wait["attempts"][0]["safe_error"], "response_read_failed");
    for field in [
        "serialization_finished_us",
        "dispatch_api_started_us",
        "dispatch_api_finished_us",
        "headers_observed_us",
        "body_read_started_us",
        "body_read_finished_us",
        "terminal_us",
    ] {
        assert!(
            wait["attempts"][0][field].is_u64(),
            "{field} must be observed"
        );
    }
}
#[test]
fn deadline_rejects_progressing_valid_advisory_after_one_second() {
    let (result, legacy, wait, gateway) = run_deadline_case(
        "body_four_writes_400ms_exact_profile_1s",
        Schedule {
            header_delay: Duration::ZERO,
            body_interval: Duration::from_millis(400),
            chunk_count: 4,
        },
    );
    partial_body_deadline_failure(&result, &legacy, &wait, &gateway);
}
#[test]
fn deadline_accepts_valid_response_within_one_second() {
    let (result, legacy, wait, gateway) = run_deadline_case(
        "body_four_writes_50ms_exact_profile_1s",
        Schedule {
            header_delay: Duration::ZERO,
            body_interval: Duration::from_millis(50),
            chunk_count: 4,
        },
    );
    assert_eq!(result["status"], "completed");
    assert_eq!(result["model_execution"], "completed_advisory");
    assert!(result["advisory"].is_object());
    assert_eq!(result["usage"]["status"], "reported");
    assert_eq!(result["usage"]["cost"], "unknown");
    assert_eq!(legacy["attempts"][0]["http_status"], 200);
    assert_eq!(legacy["attempts"][0]["response_read_status"], "read");
    assert_eq!(legacy["attempts"][0]["response_json_status"], "object");
    assert_eq!(
        legacy["attempts"][0]["response_bytes_read"],
        gateway["response_body_bytes"]
    );
    assert_eq!(gateway["server_body_chunks_written"], 4);
    assert_eq!(wait["attempts"][0]["terminal_stage"], "completed");
    for field in [
        "serialization_finished_us",
        "dispatch_api_started_us",
        "dispatch_api_finished_us",
        "headers_observed_us",
        "body_read_started_us",
        "body_read_finished_us",
        "terminal_us",
    ] {
        assert!(wait["attempts"][0][field].is_u64(), "{field}");
    }
}
#[test]
fn deadline_rejects_delayed_headers_with_exact_profile_timeout() {
    let (result, legacy, wait, _) = run_deadline_case(
        "header_1200ms_exact_profile_1s",
        Schedule {
            header_delay: Duration::from_millis(1200),
            body_interval: Duration::ZERO,
            chunk_count: 1,
        },
    );
    assert_eq!(result["status"], "failed");
    assert_eq!(result["transport_error"]["code"], "timeout");
    assert_eq!(result["transport_error"]["http_status"], Value::Null);
    assert_eq!(result["advisory"], Value::Null);
    assert_eq!(legacy["observed_http_attempts"], 0);
    assert_eq!(legacy["attempts"][0]["http_status"], Value::Null);
    assert_eq!(legacy["attempts"][0]["response_read_status"], "not_started");
    assert_eq!(legacy["attempts"][0]["response_bytes_read"], Value::Null);
    assert_eq!(legacy["attempts"][0]["response_json_status"], "not_parsed");
    assert_eq!(wait["attempts"][0]["terminal_stage"], "dispatch");
    assert!(wait["attempts"][0]["dispatch_api_finished_us"].is_u64());
    assert!(wait["attempts"][0]["terminal_us"].is_u64());
    for field in [
        "http_status",
        "headers_observed_us",
        "body_read_started_us",
        "body_read_finished_us",
    ] {
        assert!(wait["attempts"][0][field].is_null(), "{field}");
    }
}
#[test]
fn deadline_counts_headers_and_body_against_same_budget() {
    let (result, legacy, wait, gateway) = run_deadline_case(
        "header_600ms_body_four_writes_200ms_exact_profile_1s",
        Schedule {
            header_delay: Duration::from_millis(600),
            body_interval: Duration::from_millis(200),
            chunk_count: 4,
        },
    );
    partial_body_deadline_failure(&result, &legacy, &wait, &gateway);
}
