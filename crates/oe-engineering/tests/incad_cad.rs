#[path = "../../oe-knowledge/tests/support/incad_cad_fixture.rs"]
mod fixture;
use fixture::*;
use oe_contracts::{
    ArtifactReference, IncadCadSelectionRequest, IncadWorkPreparationRequest, ObjectIdentity,
};
use oe_core::SkillRoot;
use oe_engineering::incad::cad::{prepare, stage, verify_staged};
use oe_kernel::project::{Project, context};
use serde_json::{Value, json};
fn setup() -> (tempfile::TempDir, SkillRoot, IncadCadSelectionRequest) {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("SKILL.md"), "synthetic source test").unwrap();
    let root = SkillRoot::new(t.path().canonicalize().unwrap()).unwrap();
    let f = CadFixture::new(&root.path().join("var/library"));
    let intake = oe_knowledge::incad::verify(&f.source.request).unwrap();
    let selected = oe_knowledge::incad::cad::verify(&intake, &["ap203".into()]).unwrap();
    let mut evidence: serde_json::Map<String, Value> = intake
        .roles()
        .iter()
        .map(|(r, p)| (r.clone(), json!(p.sha256)))
        .collect();
    let mut sources = serde_json::Map::new();
    for (i, f) in selected.files().iter().enumerate() {
        let id = format!("cad-source-{i}");
        evidence.insert(id.clone(), json!(f.identity.sha256));
        sources.insert(f.relative_path.clone(), json!(id));
    }
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"reference","object_revision":"r1","purpose":"reference source study","acceptance":["full source identity","native inspection later"],"facts":[],"assertions":[]});
    let project:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"cad-source-study","revision":"r1","purpose":"reference source study","objects":{"reference":"r1"},"conditions":{"study":basis.to_string()},"evidence":evidence,"decisions":[],"dependencies":{}})).unwrap();
    let project_sha = save(
        &root.path().join("var/project.json"),
        &serde_json::to_value(&project).unwrap(),
    );
    let object = ObjectIdentity {
        object_id: "reference".into(),
        revision: "r1".into(),
        condition_scope: "study".into(),
        source_sha256: intake.roles()["supplier_html"].sha256.clone(),
    };
    let work = json!({"schema":"oev2.engineering-work/3","project":{"path":"var/project.json","sha256":project_sha},"object":object,"project_context":context(&project,&object).unwrap(),"acceptance":["full source identity","native inspection later"],"necessary_functions":["reference observation"],"physical_mechanisms":["source selection"],"gaps":[],"source_requests":[],"calculations":[],"handoff":null});
    let work_sha = save(&root.path().join("var/work.json"), &work);
    let prep = IncadWorkPreparationRequest {
        schema: "oev2.incad-work-preparation/1".into(),
        work: ArtifactReference {
            path: "var/work.json".into(),
            sha256: work_sha,
        },
        intake: f.source.request,
        source_ids: json!({"supplier_html":"supplier_html","projection_manifest":"projection_manifest","specifications":"specifications","task":"task"}),
    };
    (
        t,
        root,
        IncadCadSelectionRequest {
            schema: "oev2.incad-cad-selection/1".into(),
            preparation: prep,
            formats: vec!["ap203".into()],
            source_ids: json!(sources),
        },
    )
}
#[test]
fn current_project_and_complete_staged_files_cannot_be_rebound() {
    let (_t, root, request) = setup();
    let project = std::fs::read(root.path().join("var/project.json")).unwrap();
    let result = stage(&root, &request, std::path::Path::new("var/staged")).unwrap();
    assert_eq!(result["native_execution"], "not_run");
    assert_eq!(result["adopted"], false);
    verify_staged(&root, &request, std::path::Path::new("var/staged")).unwrap();
    assert_eq!(
        std::fs::read(root.path().join("var/project.json")).unwrap(),
        project
    );
    let path = root.path().join("var/staged/handoff.json");
    let mut handoff: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    handoff["native_input_candidates"][0]["source"] = json!("/different/model.step");
    let new_sha = save(&path, &handoff);
    let terminal = root.path().join("var/staged/completed.json");
    let mut c: Value = serde_json::from_slice(&std::fs::read(&terminal).unwrap()).unwrap();
    c["handoff_sha256"] = json!(new_sha);
    save(&terminal, &c);
    assert!(verify_staged(&root, &request, std::path::Path::new("var/staged")).is_err());
}
#[test]
fn missing_project_sources_changed_current_project_and_added_payloads_are_refused() {
    let (_t, root, request) = setup();
    let mut missing = request.clone();
    missing
        .source_ids
        .as_object_mut()
        .unwrap()
        .remove("cad/ap203/original.zip");
    assert!(prepare(&root, &missing).is_err());
    stage(&root, &request, std::path::Path::new("var/staged")).unwrap();
    std::fs::write(
        root.path().join("var/staged/payload/extra.step"),
        b"unbound",
    )
    .unwrap();
    assert!(verify_staged(&root, &request, std::path::Path::new("var/staged")).is_err());
    std::fs::remove_file(root.path().join("var/staged/payload/extra.step")).unwrap();
    let selected = prepare(&root, &request).unwrap();
    let p = root.path().join("var/project.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    v["revision"] = json!("r2");
    save(&p, &v);
    assert!(selected.ensure_current(&root).is_err());
    assert!(verify_staged(&root, &request, std::path::Path::new("var/staged")).is_err());
}
