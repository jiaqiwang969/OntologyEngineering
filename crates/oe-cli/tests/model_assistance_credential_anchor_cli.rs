//! Real CLI credential selection from --root A while running from cwd B.
//! Only synthetic owner-mode-0600 bearer files and literal loopback are used.
#![cfg(unix)]

use oe_contracts::ObjectIdentity;
use oe_core::sha256;
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ROOT_DUMMY: &str = "synthetic-root-anchor-only-credential";
const CWD_DUMMY: &str = "synthetic-cwd-decoy-only-credential";
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

/// Copy exactly the actual compiled normalizer's source inputs. No synthetic
/// source-binding manifest or bypass of require_execution is used.
fn snapshot() -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let parent = source.join("var/maintenance/model-assistance-credential-anchor-cli-tests");
    fs::create_dir_all(&parent).unwrap();
    let target = tempfile::tempdir_in(parent).unwrap();
    let manifest = oe_design::provenance::compiled().unwrap();
    let mut files: Vec<String> = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    files.extend(
        [
            "SKILL.md",
            "generated/contracts.rs",
            "generated/semantic-contracts.rs",
            "references/context-capabilities.json",
            "references/context-routing-instructions.json",
        ]
        .map(str::to_owned),
    );
    files.sort();
    files.dedup();
    for relative in files {
        let path = target.path().join(&relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(source.join(relative), path).unwrap();
    }
    fs::create_dir_all(target.path().join("var")).unwrap();
    target
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
                return Some(json!({"requests_received":1,"root_dummy_accepted":accepted,
                    "requested_model":request["model"],"store":request["store"],"truncation":request["truncation"]}));
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

fn execute(root: &Path, cwd: &Path) -> (i32, Value, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .current_dir(cwd)
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args([
            "project",
            "model-assistance",
            "run",
            "--input",
            "var/input.json",
            "--profile",
            "var/profile.json",
            "--output-dir",
            "var/run",
            "--live",
        ])
        .output()
        .unwrap();
    assert!(
        output.stdout.is_empty() || output.stderr.is_empty(),
        "one CLI JSON channel"
    );
    let data = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let summary: Value = serde_json::from_slice(data).unwrap();
    let entry: Value =
        serde_json::from_slice(&fs::read(root.join("var/run/entry.json")).unwrap()).unwrap();
    (output.status.code().unwrap(), summary, entry)
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

#[test]
fn live_cli_anchors_relative_credential_to_root_even_when_cwd_has_decoy() {
    let root = snapshot();
    let cwd = tempfile::tempdir_in(root.path().join("var")).unwrap();
    let gateway = Gateway::start();
    inputs(root.path(), &gateway.endpoint);
    credential(&root.path().join(CREDENTIAL), ROOT_DUMMY);
    credential(&cwd.path().join(CREDENTIAL), CWD_DUMMY);
    let mut before = protected(root.path());
    before.extend(protected(cwd.path()));
    let (code, summary, entry) = execute(root.path(), cwd.path());
    let observed = gateway.finish();
    unchanged(&before);
    println!(
        "{}",
        json!({"schema":"synthetic-credential-anchor-cli-observation/1",
        "case":"root_has_credential_and_cwd_has_different_decoy","cli_exit":code,"summary":summary,
        "gateway":observed,"normalizer_status":entry["execution_binding"]["normalizer"]["status"],
        "project_update":entry["result"]["project_update"],"protected_bytes_and_modes_unchanged":true})
    );
    assert_eq!(
        entry["execution_binding"]["normalizer"]["status"],
        "matched"
    );
    assert_eq!(
        entry["execution_binding"]["execution_role"],
        "model_assistance"
    );
    assert_eq!(entry["result"]["project_update"], "none");
    assert_eq!(entry["result"]["adopted"], false);
    assert_eq!(entry["engineering_adoption"], "not_established");
    assert_eq!(code, 0, "must read --root credential, not cwd decoy");
    assert_eq!(summary["status"], "completed");
    assert_eq!(summary["model_execution"], "completed_advisory");
    assert_eq!(entry["result"]["client_dispatch_attempts"], 1);
    assert_eq!(entry["result"]["usage"]["cost"], "unknown");
    let observed = observed.expect("one actual loopback HTTP request");
    assert_eq!(observed["requests_received"], 1);
    assert_eq!(observed["root_dummy_accepted"], true);
    assert_eq!(observed["requested_model"], "fixture-large-context-1");
    assert_eq!(observed["store"], false);
    assert_eq!(observed["truncation"], "disabled");
}

#[test]
fn live_cli_does_not_fallback_to_cwd_when_root_credential_is_absent() {
    let root = snapshot();
    let cwd = tempfile::tempdir_in(root.path().join("var")).unwrap();
    let gateway = Gateway::start();
    inputs(root.path(), &gateway.endpoint);
    credential(&cwd.path().join(CREDENTIAL), CWD_DUMMY);
    assert!(!root.path().join(CREDENTIAL).exists());
    let mut before = protected(root.path());
    before.extend(protected(cwd.path()));
    let (code, summary, entry) = execute(root.path(), cwd.path());
    let observed = gateway.finish();
    unchanged(&before);
    println!(
        "{}",
        json!({"schema":"synthetic-credential-anchor-cli-observation/1",
        "case":"root_credential_absent_cwd_has_decoy","cli_exit":code,"summary":summary,
        "gateway":observed,"normalizer_status":entry["execution_binding"]["normalizer"]["status"],
        "adapter_constructed":entry["adapter_constructed"],"protected_bytes_and_modes_unchanged":true})
    );
    assert_eq!(
        entry["execution_binding"]["normalizer"]["status"],
        "matched"
    );
    assert_eq!(code, 1);
    assert_eq!(summary["status"], "not_run");
    assert_eq!(summary["model_execution"], "not_run");
    assert_eq!(entry["adapter_constructed"], false);
    assert_eq!(
        entry["adapter_error"]["code"],
        "credential_file_unavailable"
    );
    assert_eq!(entry["result"]["client_dispatch_attempts"], 0);
    assert_eq!(entry["result"]["project_update"], "none");
    assert_eq!(entry["result"]["adopted"], false);
    assert!(
        observed.is_none(),
        "absent root credential must prevent HTTP dispatch"
    );
    assert!(!root.path().join(CREDENTIAL).exists());
}
