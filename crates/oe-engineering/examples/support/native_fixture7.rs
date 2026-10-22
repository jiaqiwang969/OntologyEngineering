// Original finite native2 fixture reused as synthetic same-Project source input.
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
    _dir: PathBuf,
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
        let dir = tempfile::tempdir_in(
            std::env::var_os("OE_MODEL7_FIXTURE_PARENT").expect("persistent fixture parent"),
        )
        .unwrap();
        let dir_path = dir.keep();
        let root_path = dir_path.canonicalize().unwrap();
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
            &json!({"model":"gpt-6-luna","state":{"purpose":project.purpose,"unknowns":["load and source applicability unknown"],"source_text":"Original question remains complete."},
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
            _dir: dir_path,
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
