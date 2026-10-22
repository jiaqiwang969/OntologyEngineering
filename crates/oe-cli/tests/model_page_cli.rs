//! Fixed source/page/bytes expectations, not a live model or applicability test.
use oe_contracts::{ModelPageSupportRequest2, ObjectIdentity};
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

fn bind_existing_cli_source(f: &Fixture) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
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
    for relative in files {
        let dest = f.root.path().join(&relative);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(source.join(relative), dest).unwrap();
    }
}
fn load(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
#[test]
fn explicit_page_cli_prepare_and_offline_run_use_existing_owner_without_credentials() {
    let f = Fixture::new();
    bind_existing_cli_source(&f);
    let project_before = fs::read(f.root.path().join("var/project.json")).unwrap();
    for (command, out, expected_code) in [("prepare-pages", "prepare", 0), ("run-pages", "run", 1)]
    {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_oev2"))
            .arg("--root")
            .arg(f.root.path())
            .args(["project", "model-assistance", command])
            .arg("--input")
            .arg(&f.wrapper)
            .arg("--profile")
            .arg(&f.profile)
            .arg("--output-dir")
            .arg(format!("var/{out}"))
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(expected_code),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let entry = load(&f.root.path().join(format!("var/{out}/entry.json")));
        assert_eq!(entry["schema"], "oev2.model-page-assistance-cli/2");
        assert_eq!(entry["engineering_adoption"], "not_established");
        if command == "prepare-pages" {
            assert_eq!(entry["result"]["execution_port"], "not_connected");
            assert_eq!(
                entry["result"]["selected_page_support"]["pages"][0]["content_status"],
                "native_text"
            );
            assert_eq!(entry["model_execution"], "not_run");
        } else {
            assert_eq!(entry["live_requested"], false);
            assert_eq!(entry["adapter_constructed"], false);
            assert_eq!(entry["result"]["status"], "not_run");
            assert_eq!(entry["result"]["client_dispatch_attempts"], 0);
            assert_eq!(
                entry["result"]["plan"]["execution_port"],
                "shared_model_run_prepared/2"
            );
            assert_eq!(
                entry["result"]["plan"]["selected_page_support"]["pages"][1]["text"],
                ""
            );
            assert_eq!(
                entry["result"]["plan"]["selected_page_support"]["pages"][1]["ocr"],
                "not_run"
            );
        }
    }
    assert_eq!(
        fs::read(f.root.path().join("var/project.json")).unwrap(),
        project_before
    );
    assert!(!f.root.path().join("var/unused-credential.txt").exists());
}
