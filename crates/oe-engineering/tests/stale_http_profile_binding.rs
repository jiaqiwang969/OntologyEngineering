//! Independently frozen stale actual-HTTP configuration regression.
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
#[test]
fn consistent_large_cap_accepts_actual_valid_response() {
    let gateway = Gateway::start();
    let root = setup(&gateway, 1_000_000);
    prepared(&root, 1_000_000);
    let adapter = actual_adapter(&root);
    let (result, gateway) = actual_run(&root, &adapter, gateway, "consistent_large_cap_control");
    assert_eq!(result["status"], "completed");
    assert_eq!(result["model_execution"], "completed_advisory");
    let gateway = gateway.unwrap();
    assert_eq!(gateway["root_dummy_accepted"], true);
    assert!(gateway["response_body_bytes"].as_u64().unwrap() > 1);
    assert_eq!(result["client_dispatch_attempts"], 1);
}
#[test]
fn consistent_one_byte_cap_rejects_actual_response_body() {
    let gateway = Gateway::start();
    let root = setup(&gateway, 1);
    prepared(&root, 1);
    let adapter = actual_adapter(&root);
    let (result, gateway) = actual_run(&root, &adapter, gateway, "consistent_one_byte_cap_control");
    assert_eq!(result["status"], "failed");
    assert_eq!(result["transport_error"]["code"], "response_too_large");
    let gateway = gateway.unwrap();
    assert_eq!(gateway["root_dummy_accepted"], true);
    assert!(gateway["response_body_bytes"].as_u64().unwrap() > 1);
    assert_eq!(result["client_dispatch_attempts"], 1);
}
#[test]
fn stale_large_adapter_cannot_dispatch_under_current_one_byte_profile() {
    let gateway = Gateway::start();
    let root = setup(&gateway, 1_000_000);
    prepared(&root, 1_000_000);
    let adapter = actual_adapter(&root);
    fs::copy(
        root.path().join("var/profile.json"),
        root.path().join("var/profile-before-setup-mutation.json"),
    )
    .unwrap();
    update_maximum(&root, 1);
    prepared(&root, 1);
    let (result, gateway) = actual_run(
        &root,
        &adapter,
        gateway,
        "stale_large_adapter_against_current_one_byte_cap",
    );
    assert_eq!(
        result["status"], "blocked",
        "stale actual HTTP profile configuration must reject before dispatch"
    );
    assert_eq!(result["client_dispatch_attempts"], 0);
    assert!(
        gateway.is_none(),
        "a stale actual HTTP adapter must not dispatch"
    );
}
