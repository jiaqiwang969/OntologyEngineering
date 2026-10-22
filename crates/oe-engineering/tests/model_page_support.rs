//! Fixed source/page/bytes expectations, not a live model or applicability test.
use oe_contracts::{
    ModelArtifactReference, ModelPageSupportPort, ModelPageSupportRequest2, ObjectIdentity,
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

#[test]
fn actual_original_pages_are_bound_without_dispatch() {
    let f = Fixture::new();
    let before = serde_json::to_value(&f.project).unwrap();
    let executor = pages::PageSupportExecutor {
        root: &f.root,
        project: &f.project,
        tools: &tools(),
    };
    let packet = executor.prepare_pages(&f.request).unwrap();
    assert_eq!(packet.pages.len(), 2);
    assert_eq!(
        packet.pages[0].text,
        include_str!("fixtures/model-pages/vol2-page24.txt")
    );
    assert_eq!(packet.pages[0].content_status, "native_text");
    assert_eq!(packet.pages[0].document_bytes, 17_154_281);
    assert_eq!(packet.pages[0].page_count, 353);
    assert_eq!(packet.pages[1].text, "");
    assert_eq!(packet.pages[1].content_status, "navigation_only");
    assert!(packet.pages[1].native_text_sparse);
    assert_eq!(packet.pages[1].document_bytes, 35_986_251);
    assert_eq!(packet.pages[1].headings, vec!["尺寸链"]);
    for page in &packet.pages {
        assert_eq!(page.ocr, "not_run");
        assert_eq!(page.applicability, "unknown");
        assert_eq!(page.printed_page_status, "unverified");
        assert_ne!(page.text_sha256, page.document.sha256);
    }
    assert_eq!(serde_json::to_value(&f.project).unwrap(), before);
}
#[test]
fn exact_source_refs_and_complete_pdf_identity_are_required() {
    let mut f = Fixture::new();
    for (id, sha) in [
        (
            "invented-source-ref",
            f.request.pages[0].document_sha256.clone(),
        ),
        ("trust-scope-page24", f.request.pages[0].text_sha256.clone()),
        (
            "tolerance-dimension-chain-page188",
            f.request.pages[0].document_sha256.clone(),
        ),
    ] {
        f.request.pages[0].source_ref = id.into();
        f.request.pages[0].document_sha256 = sha;
        assert!(
            f.prepare()
                .unwrap_err()
                .to_string()
                .contains("source_ref_unbound")
        );
    }
}
#[test]
fn selected_physical_page_and_both_text_digests_are_required() {
    let mut f = Fixture::new();
    let original = f.request.clone();
    f.request.pages[0].physical_page = 25;
    assert!(
        f.prepare()
            .unwrap_err()
            .to_string()
            .contains("selection_mismatch")
    );
    f.request = original.clone();
    f.request.pages[0].text_sha256 = sha256("fabricated body");
    assert!(f.prepare().is_err());
    f.request = original;
    f.request.pages[0].raw_text_sha256 = sha256("fabricated raw");
    assert!(f.prepare().is_err());
}
#[test]
fn unknown_missing_and_caller_body_are_not_page_authority() {
    let f = Fixture::new();
    let original = serde_json::to_value(&f.request).unwrap();
    let mut v = original.clone();
    v["pages"][0]["body"] = json!("forged");
    assert!(pages::parse_request(&v).is_err());
    let mut v = original.clone();
    v.as_object_mut().unwrap().remove("index");
    assert!(pages::parse_request(&v).is_err());
    let mut v = original.clone();
    v["registration"]["passed"] = json!(true);
    assert!(pages::parse_request(&v).is_err());
    let mut v = original;
    v["schema"] = json!(model_assistance::INPUT_SCHEMA);
    assert!(pages::parse_request(&v).is_err());
}
#[test]
fn same_size_pdf_and_symlink_replacements_are_rejected() {
    let f = Fixture::new();
    let path = f.root.path().join("sources/books/vol2.pdf");
    let original = fs::read(&path).unwrap();
    let mut changed = original.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    fs::write(&path, &changed).unwrap();
    assert!(
        f.prepare()
            .unwrap_err()
            .to_string()
            .contains("document_changed")
    );
    fs::write(&path, &original).unwrap();
    #[cfg(unix)]
    {
        let backup = path.with_extension("original");
        fs::rename(&path, &backup).unwrap();
        std::os::unix::fs::symlink(&backup, &path).unwrap();
        assert!(f.prepare().is_err());
        fs::remove_file(&path).unwrap();
        fs::rename(&backup, &path).unwrap();
    }
    assert_eq!(fs::read(path).unwrap(), original);
}
#[test]
fn registration_and_index_raw_replacements_are_rejected() {
    let f = Fixture::new();
    for r in [&f.request.registration, &f.request.index] {
        let path = Path::new(&r.path);
        let original = fs::read(path).unwrap();
        let mut raw = original.clone();
        raw[0] ^= 1;
        fs::write(path, raw).unwrap();
        assert!(f.prepare().is_err());
        fs::write(path, &original).unwrap();
    }
}
#[test]
fn rehashed_index_forgery_still_requires_real_original_page_text() {
    let mut f = Fixture::new();
    let path = Path::new(&f.request.index.path);
    let db = rusqlite::Connection::open(path).unwrap();
    let text = "malicious replacement preserving complete PDF identity";
    db.execute("UPDATE pages SET body=?,text_sha256=?,raw_text_sha256=?,text_chars=?,raw_text_chars=?,native_text_sparse=1 WHERE id=?",
        rusqlite::params![text,sha256(text),sha256(text),text.chars().count() as u64,text.chars().count() as u64,"local_pdf:vol2:page:24"]).unwrap();
    drop(db);
    f.request.index.sha256 = sha256(fs::read(path).unwrap());
    let mut registration: Value =
        serde_json::from_slice(&fs::read(&f.request.registration.path).unwrap()).unwrap();
    registration["index_sha256"] = json!(f.request.index.sha256);
    save(Path::new(&f.request.registration.path), &registration);
    f.request.registration.sha256 = sha256(fs::read(&f.request.registration.path).unwrap());
    f.request.pages[0].text_sha256 = sha256(text);
    f.request.pages[0].raw_text_sha256 = sha256(text);
    assert!(
        f.prepare()
            .unwrap_err()
            .to_string()
            .contains("native_text_mismatch")
    );
}
#[test]
fn offline_wrapper_preserves_original_allowed_refs_and_reuses_budget() {
    let f = Fixture::new();
    let before = fs::read(&f.request.project.path).unwrap();
    let old = model_assistance::prepare(&f.root, &f.assistance, &f.profile).unwrap();
    assert!(old.admitted());
    let new = pages::prepare_with_pages(&f.root, &f.wrapper, &f.profile, &tools()).unwrap();
    let payload: Value = serde_json::from_str(
        new.request()["input"][0]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        payload["selected_page_support"]["pages"][0]["text"],
        include_str!("fixtures/model-pages/vol2-page24.txt")
    );
    assert_eq!(
        payload["selected_page_support"]["pages"][1]["content_status"],
        "navigation_only"
    );
    assert_eq!(
        payload["allowed_source_refs"],
        old.report["allowed_source_refs"]
    );
    assert_eq!(
        new.request()["text"]["format"],
        old.request["text"]["format"]
    );
    let delta = new.report()["budget"]["input_text_bytes"].as_u64().unwrap()
        - old.report["budget"]["input_text_bytes"].as_u64().unwrap();
    assert_eq!(
        new.report()["budget"]["conservative_total_tokens"]
            .as_u64()
            .unwrap(),
        old.report["budget"]["conservative_total_tokens"]
            .as_u64()
            .unwrap()
            + delta
    );
    assert_eq!(new.report()["execution_port"], "not_connected");
    assert_eq!(new.report()["model_execution"], "not_run");
    assert_eq!(new.request()["store"], false);
    assert_eq!(new.request()["truncation"], "disabled");
    assert_eq!(new.request()["stream"], false);
    assert_eq!(fs::read(&f.request.project.path).unwrap(), before);
    assert!(model_assistance::prepare(&f.root, &f.wrapper, &f.profile).is_err());
}
#[test]
fn added_page_text_cannot_bypass_original_service_byte_budget() {
    let f = Fixture::new();
    let old = model_assistance::prepare(&f.root, &f.assistance, &f.profile).unwrap();
    let mut profile: Value = serde_json::from_slice(&fs::read(&f.profile).unwrap()).unwrap();
    profile["maximum_request_bytes"] = json!(serde_json::to_vec(&old.request).unwrap().len() + 1);
    save(&f.profile, &profile);
    assert!(
        model_assistance::prepare(&f.root, &f.assistance, &f.profile)
            .unwrap()
            .admitted()
    );
    let new = pages::prepare_with_pages(&f.root, &f.wrapper, &f.profile, &tools()).unwrap();
    assert_eq!(new.report()["status"], "blocked");
    assert!(
        new.report()["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["code"] == "model_assistance_request_byte_budget_exceeded")
    );
}
#[test]
fn retained_offline_request_rechecks_sources_instead_of_trusting_json() {
    let f = Fixture::new();
    let new = pages::prepare_with_pages(&f.root, &f.wrapper, &f.profile, &tools()).unwrap();
    let path = f.root.path().join("sources/books/vol2.pdf");
    let mut raw = fs::read(&path).unwrap();
    raw[0] ^= 1;
    fs::write(&path, raw).unwrap();
    assert!(new.verify_sources(&f.root, &tools()).is_err());
}
