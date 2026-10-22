//! Fixed source/page/bytes expectations, not a live model or applicability test.
use oe_contracts::{
    ModelArtifactReference, ModelPageRunInput2, ModelPageRunPort, ModelPageSupportRequest2,
    ObjectIdentity,
};
use oe_core::{SkillRoot, sha256};
use oe_engineering::{model_assistance, model_page_support as pages};
use oe_kernel::project::{self, Project};
use oe_knowledge::poppler::PopplerTools;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    _dir: tempfile::TempDir,
    root: SkillRoot,
    project: Project,
    request: ModelPageSupportRequest2,
    assistance: PathBuf,
    wrapper: PathBuf,
    profile: PathBuf,
}
fn save(path: &Path, v: &Value) {
    fs::write(path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}
fn reference(path: &Path) -> Value {
    json!({"path":path,"sha256":sha256(fs::read(path).unwrap())})
}
fn tools() -> PopplerTools {
    PopplerTools::default()
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root_path = dir.path().canonicalize().unwrap();
        fs::write(
            root_path.join("SKILL.md"),
            "Explicit isolated original-page fixture",
        )
        .unwrap();
        fs::create_dir(root_path.join("var")).unwrap();
        let root = SkillRoot::new(&root_path).unwrap();
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let original = std::env::var_os("OE_PAGE_ORIGINAL_INPUT_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| parent.join("original-page-inputs"));
        for relative in [
            "sources/.indexes/local-pdf/registration.json",
            "sources/.indexes/local-pdf/catalog.sqlite3",
            "sources/books/interchangeability.pdf",
            "sources/books/vol2.pdf",
        ] {
            let dest = root_path.join(relative);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::copy(original.join(relative), dest).unwrap();
        }
        let vol2 = "f46ca36c98064bb5dc6f4d196fb9f3d7c4e68c1efb5c58e2b41a7e29dc25623e";
        let interchangeability = "baf6363d11afb072bdb960c9a735821393771f213e7495f981ee97c3d9732259";
        let object = ObjectIdentity {
            object_id: "urn:page-support:test".into(),
            revision: "r1".into(),
            condition_scope: "source-review".into(),
            source_sha256: vol2.into(),
        };
        let basis = json!({"schema":"oev2.engineering-basis/1","object_id":object.object_id,"object_revision":"r1",
            "purpose":"Review original pages without engineering adoption","acceptance":["preserve source applicability unknown"],"facts":[],"assertions":[]});
        let project: Project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"page-fixture","revision":"r1",
            "purpose":"Review original pages without engineering adoption","objects":{object.object_id.clone():"r1"},
            "conditions":{"source-review":serde_json::to_string(&basis).unwrap()},
            "evidence":{"trust-scope-page24":vol2,"tolerance-dimension-chain-page188":interchangeability},"decisions":[],"dependencies":{}})).unwrap();
        let project_path = root_path.join("var/project.json");
        save(&project_path, &serde_json::to_value(&project).unwrap());
        let request = pages::parse_request(&json!({"schema":pages::REQUEST_SCHEMA,"project":reference(&project_path),
            "registration":reference(&root_path.join("sources/.indexes/local-pdf/registration.json")),
            "index":reference(&root_path.join("sources/.indexes/local-pdf/catalog.sqlite3")),
            "pages":[{"source_ref":"trust-scope-page24","page_id":"local_pdf:vol2:page:24","physical_page":24,
                "document_sha256":vol2,"text_sha256":"247603640b14bf4e4adb6c193d3dbbde628c65b1df55530652bdf39e41e342cd",
                "raw_text_sha256":"2c5ac3750ae214e3b6d885704a6675b4776588df1c9bb030e8b53bc09ebf42eb"},
                {"source_ref":"tolerance-dimension-chain-page188","page_id":"local_pdf:interchangeability:page:188","physical_page":188,
                "document_sha256":interchangeability,"text_sha256":sha256(""),"raw_text_sha256":sha256("")}]})).unwrap();
        let original_request = root_path.join("var/original.json");
        save(
            &original_request,
            &json!({"schema":"oev2.advisory-questions/1","state":{"purpose":project.purpose,"unknowns":["load and source applicability unknown"],"source_text":"Original question remains complete."},
            "questions":{"review":{"type":"choice","instructions":"Review original source?","criteria":{"needed":"Needed","unknown":"Unknown"}}}}),
        );
        let mut old = reference(&original_request);
        old["role"] = json!("historical_model_request");
        let assistance = root_path.join("var/assistance.json");
        save(
            &assistance,
            &json!({"schema":model_assistance::INPUT_SCHEMA,"project":reference(&project_path),"object":object,
            "project_context":project::context(&project,&object).unwrap(),"purpose":project.purpose,
            "task":"Read the selected original pages, preserving all unknowns.","original_requests":[old],"support_files":[]}),
        );
        let wrapper = root_path.join("var/page-assistance.json");
        save(
            &wrapper,
            &json!({"schema":pages::ASSISTANCE_SCHEMA,"assistance":reference(&assistance),"page_support":request}),
        );
        let profile = root_path.join("var/profile.json");
        save(
            &profile,
            &json!({"schema":model_assistance::PROFILE_SCHEMA,"service_id":"offline-page-fixture","api_kind":"openai_responses",
            "endpoint":"http://127.0.0.1:1/v1/responses","requested_model":"offline-model","accepted_response_models":["offline-model"],
            "context_window_tokens":2000000,"maximum_request_bytes":1000000,"maximum_output_tokens":2048,"maximum_response_bytes":1000000,
            "wrapper_reserve_tokens":4096,"timeout_seconds":30,"credential_file":"var/unused-credential.txt",
            "capacity_evidence":"Synthetic offline declaration; not provider capacity proof"}),
        );
        Self {
            _dir: dir,
            root,
            project,
            request,
            assistance,
            wrapper,
            profile,
        }
    }
    fn prepare(&self) -> anyhow::Result<oe_contracts::ModelPageSupportPacket2> {
        pages::prepare(&self.root, &self.project, &self.request, &tools())
    }
}

use oe_contracts::ModelServiceProfile;
use oe_engineering::model_assistance::transport::{
    HttpModelTransport, ModelTransport, ModelTransportError, endpoint_identity,
};
use std::{
    cell::RefCell,
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn load(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn advice() -> Value {
    json!({"schema":"oev2.model-advisory/1","summary":"Read the genuine selected source and retain applicability unknown.",
      "answers":[{"request_index":0,"question_id":"review","selected_option":"needed","explanation":"Review the available original source body before bounded checks.",
      "source_refs":["trust-scope-page24"],"unknowns":["engineering applicability"],"verification_required":["check applicability and independent engineering evidence"]}],
      "findings":[{"kind":"missing_condition","claim":"A source citation does not establish engineering applicability.","source_refs":["trust-scope-page24"],
      "assumptions":[],"unknowns":["applicable conditions"],"verification_required":["check applicable conditions"]}]})
}
fn response() -> Value {
    json!({"id":"loopback-page-response","model":"offline-model","status":"completed","error":null,"incomplete_details":null,
    "output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":serde_json::to_string(&advice()).unwrap()}]}],
    "usage":{"input_tokens":100,"output_tokens":50,"total_tokens":150}})
}
impl Fixture {
    fn run(&self, name: &str, transport: Option<&dyn ModelTransport>) -> Value {
        model_assistance::run_pages(
            &self.root,
            &self.wrapper,
            &self.profile,
            Path::new(&format!("var/{name}")),
            &tools(),
            transport,
        )
        .unwrap()
    }
    fn typed(&self, name: &str) -> ModelPageRunInput2 {
        ModelPageRunInput2 {
            schema: "oev2.model-page-run-input/2".into(),
            input: serde_json::from_value(reference(&self.wrapper)).unwrap(),
            profile: serde_json::from_value(reference(&self.profile)).unwrap(),
            output_dir: format!("var/{name}"),
        }
    }
    fn injected(&self) -> Injected {
        Injected {
            identity: endpoint_identity(load(&self.profile)["endpoint"].as_str().unwrap()).unwrap(),
            requests: RefCell::new(vec![]),
            response: response(),
            before: None,
            after: None,
        }
    }
    fn project_bytes(&self) -> Vec<u8> {
        fs::read(self.root.path().join("var/project.json")).unwrap()
    }
}
struct Injected {
    identity: String,
    requests: RefCell<Vec<Value>>,
    response: Value,
    before: Option<(PathBuf, Vec<u8>)>,
    after: Option<(PathBuf, Vec<u8>)>,
}
impl ModelTransport for Injected {
    fn identity(&self) -> String {
        self.identity.clone()
    }
    fn check_service_configuration(
        &self,
        _root: &SkillRoot,
        _profile: &ModelServiceProfile,
    ) -> Option<Result<(), ModelTransportError>> {
        if let Some((path, raw)) = &self.before {
            fs::write(path, raw).unwrap();
        }
        // An injected transport never claims the builtin factory was verified.
        None
    }
    fn send(&self, request: &Value) -> Result<Value, ModelTransportError> {
        self.requests.borrow_mut().push(request.clone());
        if let Some((path, raw)) = &self.after {
            fs::write(path, raw).unwrap();
        }
        Ok(self.response.clone())
    }
}
fn changed(path: &Path) -> Vec<u8> {
    let mut raw = fs::read(path).unwrap();
    let at = raw.len() - 1;
    raw[at] ^= 1;
    raw
}
fn assert_not_adopted(result: &Value) {
    assert_eq!(result["adopted"], false);
    assert_eq!(result["engineering_adoption"], "not_established");
    assert_eq!(result["project_update"], "none");
    assert_eq!(result["automatic_retries"], 0);
    assert_eq!(result["automatic_fallback"], false);
    assert_eq!(result["upstream_attempts"], "unknown");
}
#[test]
fn loopback_send_uses_real_selected_page_request() {
    let f = Fixture::new();
    let project = f.project_bytes();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let mut profile = load(&f.profile);
    profile["endpoint"] = json!(endpoint);
    save(&f.profile, &profile);
    let credential = f.root.path().join("var/unused-credential.txt");
    fs::write(&credential, "isolated-loopback-fixture-bearer\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let server = thread::spawn(move || {
        let started = std::time::Instant::now();
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(v) => break v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        started.elapsed() < Duration::from_secs(30),
                        "no loopback request"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("loopback accept {e}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut received = Vec::new();
        let mut chunk = [0u8; 4096];
        let end = loop {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            received.extend_from_slice(&chunk[..n]);
            assert!(received.len() < 1_100_000);
            if let Some(i) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let headers = std::str::from_utf8(&received[..end]).unwrap();
        assert!(headers.starts_with("POST /v1/responses HTTP/1.1\r\n"));
        let length: usize = headers
            .lines()
            .find_map(|s| {
                s.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse().unwrap())
            })
            .unwrap();
        while received.len() - end < length {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            received.extend_from_slice(&chunk[..n]);
        }
        assert_eq!(received.len() - end, length);
        let body = received[end..].to_vec();
        let raw = serde_json::to_vec(&response()).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",raw.len()).unwrap();
        stream.write_all(&raw).unwrap();
        stream.flush().unwrap();
        drop(stream);
        // One application request only; retries would leave an additional accepted socket.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        body
    });
    let transport =
        HttpModelTransport::new(&endpoint, &credential, Duration::from_secs(30), 1_000_000)
            .unwrap();
    let t = tools();
    let executor = pages::PageModelExecutor {
        root: &f.root,
        tools: &t,
        transport: Some(&transport),
    };
    let receipt = executor.run_pages(&f.typed("loopback")).unwrap();
    let wire = server.join().unwrap();
    let result_path = f.root.expand(Path::new(&receipt.result.path)).unwrap();
    let result = load(&result_path);
    assert_eq!(receipt.status, "completed");
    assert_eq!(result["client_dispatch_attempts"], 1);
    assert_eq!(
        receipt.result.sha256,
        sha256(fs::read(&result_path).unwrap())
    );
    assert_eq!(result["transport_configuration_status"], "matched");
    assert_eq!(
        result["plan"]["execution_port"],
        "shared_model_run_prepared/2"
    );
    let request: Value = serde_json::from_slice(&wire).unwrap();
    let payload: Value =
        serde_json::from_str(request["input"][0]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["schema"], "oev2.model-assistance-payload/3");
    assert_eq!(
        payload["selected_page_support_scope"],
        pages::NATIVE_PAGE_SCOPE
    );
    assert!(pages::NATIVE_PAGE_SCOPE.contains("not a blank physical page"));
    assert_eq!(
        payload["selected_page_support"]["pages"][0]["text"],
        include_str!("fixtures/model-pages/vol2-page24.txt")
    );
    assert_eq!(payload["selected_page_support"]["pages"][1]["text"], "");
    assert_eq!(
        payload["selected_page_support"]["pages"][1]["content_status"],
        "navigation_only"
    );
    assert_eq!(
        payload["selected_page_support"]["pages"][1]["ocr"],
        "not_run"
    );
    assert_eq!(
        payload["selected_page_support"]["pages"][0]["document_bytes"],
        17_154_281
    );
    assert_eq!(
        payload["current_project"],
        serde_json::to_value(&f.project).unwrap()
    );
    assert_eq!(
        payload["allowed_source_refs"],
        json!(["tolerance-dimension-chain-page188", "trust-scope-page24"])
    );
    assert_eq!(request["store"], false);
    assert_eq!(request["stream"], false);
    assert_eq!(request["truncation"], "disabled");
    assert_eq!(
        fs::read(f.root.path().join("var/loopback/request.json")).unwrap(),
        wire
    );
    assert_eq!(transport.observation()["send_call_attempts"], 1);
    assert_eq!(
        transport.observation()["attempts"][0]["request_sha256"],
        sha256(&wire)
    );
    assert_eq!(transport.observation()["attempts"][0]["http_status"], 200);
    assert_eq!(result["plan"]["request_sha256"], sha256(&wire));
    assert_not_adopted(&result);
    assert_eq!(f.project_bytes(), project);
    let archived = load(
        &f.root
            .path()
            .join("var/loopback/selected-pages.original.json"),
    );
    assert_eq!(archived, payload["selected_page_support"]);
    assert_eq!(
        fs::read(
            f.root
                .path()
                .join("var/loopback/page-assistance-input.original")
        )
        .unwrap(),
        fs::read(&f.wrapper).unwrap()
    );
    assert_eq!(
        fs::read(f.root.path().join("var/loopback/profile.original")).unwrap(),
        fs::read(&f.profile).unwrap()
    );
}
#[test]
fn no_transport_remains_not_run_and_sources_unchanged() {
    let f = Fixture::new();
    let project = f.project_bytes();
    let r = f.run("no-transport", None);
    assert_eq!(r["status"], "not_run");
    assert_eq!(r["client_dispatch_attempts"], 0);
    assert_eq!(r["transport_configuration_status"], "unverified");
    assert_eq!(r["advisory"], Value::Null);
    assert_not_adopted(&r);
    assert_eq!(f.project_bytes(), project);
}
#[test]
fn before_dispatch_pdf_index_registration_and_wrapper_mutation_send_zero() {
    for relative in [
        "sources/books/vol2.pdf",
        "sources/.indexes/local-pdf/catalog.sqlite3",
        "sources/.indexes/local-pdf/registration.json",
        "var/page-assistance.json",
    ] {
        let f = Fixture::new();
        let project = f.project_bytes();
        let mut transport = f.injected();
        let path = f.root.path().join(relative);
        transport.before = Some((path.clone(), changed(&path)));
        let r = f.run("before-mutated", Some(&transport));
        assert_eq!(r["status"], "stale", "{relative}");
        assert_eq!(r["client_dispatch_attempts"], 0, "{relative}");
        assert_eq!(r["source_binding_current"], false);
        assert!(transport.requests.borrow().is_empty());
        assert_eq!(r["transport_configuration_status"], "unverified");
        assert_not_adopted(&r);
        assert_eq!(f.project_bytes(), project);
    }
}
#[test]
fn after_dispatch_pdf_and_index_mutation_preserve_response_but_not_support() {
    for relative in [
        "sources/books/vol2.pdf",
        "sources/.indexes/local-pdf/catalog.sqlite3",
    ] {
        let f = Fixture::new();
        let project = f.project_bytes();
        let mut transport = f.injected();
        let path = f.root.path().join(relative);
        transport.after = Some((path.clone(), changed(&path)));
        let r = f.run("after-mutated", Some(&transport));
        assert_eq!(r["status"], "stale");
        assert_eq!(r["client_dispatch_attempts"], 1);
        assert_eq!(transport.requests.borrow().len(), 1);
        assert_eq!(r["source_binding_current"], false);
        assert_eq!(r["current_support"], "not_established");
        assert!(r["advisory"].is_object());
        assert!(
            f.root
                .path()
                .join("var/after-mutated/response.parsed.json")
                .is_file()
        );
        assert_not_adopted(&r);
        assert_eq!(f.project_bytes(), project);
    }
}
#[test]
fn invented_and_context_refs_still_fail_shared_response_validation() {
    for id in ["fabricated-source", "source-review"] {
        let f = Fixture::new();
        let mut transport = f.injected();
        let mut a = advice();
        a["answers"][0]["source_refs"] = json!([id]);
        transport.response["output"][0]["content"][0]["text"] =
            json!(serde_json::to_string(&a).unwrap());
        let r = f.run("invalid-source", Some(&transport));
        assert_eq!(r["status"], "failed");
        assert_eq!(r["client_dispatch_attempts"], 1);
        assert_eq!(r["advisory"], Value::Null);
        assert_eq!(transport.requests.borrow().len(), 1);
        assert_not_adopted(&r);
    }
}
#[test]
fn typed_reference_sha_cannot_authorize_different_current_inputs() {
    let f = Fixture::new();
    let transport = f.injected();
    let t = tools();
    let executor = pages::PageModelExecutor {
        root: &f.root,
        tools: &t,
        transport: Some(&transport),
    };
    for field in ["input", "profile"] {
        let mut input = f.typed(&format!("wrong-{field}"));
        if field == "input" {
            input.input.sha256 = sha256("not wrapper");
        } else {
            input.profile.sha256 = sha256("not profile");
        }
        assert!(executor.run_pages(&input).is_err());
        assert!(transport.requests.borrow().is_empty());
        assert!(!f.root.path().join(&input.output_dir).exists());
    }
}
#[test]
fn page_increment_reuses_original_byte_and_context_caps() {
    for cap in ["maximum_request_bytes", "context_window_tokens"] {
        let f = Fixture::new();
        let base = model_assistance::prepare(&f.root, &f.assistance, &f.profile).unwrap();
        assert!(base.admitted());
        let mut profile = load(&f.profile);
        profile[cap] = base.value()[if cap == "maximum_request_bytes" {
            "request_bytes"
        } else {
            "budget"
        }]
        .clone();
        if cap == "context_window_tokens" {
            profile[cap] = base.value()["budget"]["conservative_total_tokens"].clone();
        }
        save(&f.profile, &profile);
        assert!(
            model_assistance::prepare(&f.root, &f.assistance, &f.profile)
                .unwrap()
                .admitted()
        );
        let transport = f.injected();
        let r = f.run("over-page-budget", Some(&transport));
        assert_eq!(r["status"], "blocked");
        assert_eq!(r["client_dispatch_attempts"], 0);
        assert!(transport.requests.borrow().is_empty());
        assert_not_adopted(&r);
    }
}
#[test]
fn fresh_output_collision_refuses_without_second_dispatch() {
    let f = Fixture::new();
    let transport = f.injected();
    let r = f.run("once", Some(&transport));
    assert_eq!(r["status"], "completed");
    let before = fs::read(f.root.path().join("var/once/result.json")).unwrap();
    assert!(
        model_assistance::run_pages(
            &f.root,
            &f.wrapper,
            &f.profile,
            Path::new("var/once"),
            &tools(),
            Some(&transport)
        )
        .is_err()
    );
    assert_eq!(transport.requests.borrow().len(), 1);
    assert_eq!(
        fs::read(f.root.path().join("var/once/result.json")).unwrap(),
        before
    );
}
#[test]
fn builtin_factory_mismatch_rejects_before_http_attempt() {
    let f = Fixture::new();
    let credential = f.root.path().join("var/unused-credential.txt");
    fs::write(&credential, "isolated-loopback-fixture-bearer\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let transport = HttpModelTransport::new(
        load(&f.profile)["endpoint"].as_str().unwrap(),
        &credential,
        Duration::from_secs(30),
        999_999,
    )
    .unwrap();
    let r = f.run("wrong-factory", Some(&transport));
    assert_eq!(r["status"], "blocked");
    assert_eq!(r["transport_configuration_status"], "mismatch");
    assert_eq!(r["client_dispatch_attempts"], 0);
    assert_eq!(transport.observation()["send_call_attempts"], 0);
    assert_not_adopted(&r);
}
