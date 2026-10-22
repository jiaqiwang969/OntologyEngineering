#[path = "../../oe-knowledge/tests/support/incad_fixture.rs"]
mod fixture;
use fixture::*;
use oe_contracts::{ArtifactReference, IncadWorkPreparationRequest, ObjectIdentity};
use oe_core::SkillRoot;
use oe_engineering::incad::{prepare, verify_preparation};
use oe_kernel::project::{Project, context};
use serde_json::{Value, json};
fn setup() -> (tempfile::TempDir, SkillRoot, IncadWorkPreparationRequest) {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("SKILL.md"), "fixture").unwrap();
    let root = SkillRoot::new(t.path().canonicalize().unwrap()).unwrap();
    let f = Fixture::new(&root.path().join("var/library"));
    let intake = oe_knowledge::incad::verify(&f.request).unwrap();
    let evidence: serde_json::Map<String, Value> = intake
        .roles()
        .iter()
        .map(|(role, pin)| (role.clone(), json!(pin.sha256)))
        .collect();
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"guide","object_revision":"r1","purpose":"source study","acceptance":["source-bound design questions"],"facts":[],"assertions":[]});
    let project:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"incad-study","revision":"r1","purpose":"source study","objects":{"guide":"r1"},"conditions":{"study":basis.to_string()},"evidence":evidence,"decisions":[],"dependencies":{}})).unwrap();
    let sha = save(
        &root.path().join("var/project.json"),
        &serde_json::to_value(&project).unwrap(),
    );
    let object = ObjectIdentity {
        object_id: "guide".into(),
        revision: "r1".into(),
        condition_scope: "study".into(),
        source_sha256: intake.roles()["supplier_html"].sha256.clone(),
    };
    let work = json!({"schema":"oev2.engineering-work/3","project":{"path":"var/project.json","sha256":sha},"object":object,"project_context":context(&project,&object).unwrap(),"acceptance":["source-bound design questions"],"necessary_functions":["guide workpiece"],"physical_mechanisms":["constrained motion"],"gaps":[],"source_requests":[],"calculations":[],"handoff":null});
    let sha = save(&root.path().join("var/work.json"), &work);
    let request = IncadWorkPreparationRequest {
        schema: "oev2.incad-work-preparation/1".into(),
        work: ArtifactReference {
            path: "var/work.json".into(),
            sha256: sha,
        },
        intake: f.request,
        source_ids: json!({"supplier_html":"supplier_html","projection_manifest":"projection_manifest","specifications":"specifications","task":"task"}),
    };
    (t, root, request)
}
#[test]
fn incad_preparation_requires_current_project_and_preserves_original_work() {
    let (_t, root, r) = setup();
    let project = std::fs::read(root.path().join("var/project.json")).unwrap();
    let p = prepare(&root, &r).unwrap();
    assert_eq!(
        p.work_bytes,
        std::fs::read(root.path().join("var/work.json")).unwrap()
    );
    assert_eq!(p.result["project_id"], "incad-study");
    assert_eq!(p.result["model_execution"], "not_run");
    verify_preparation(&root, &r, &p.result).unwrap();
    assert_eq!(
        std::fs::read(root.path().join("var/project.json")).unwrap(),
        project
    );
    let mut changed = p.result.clone();
    changed["design_window"]["current_request"] = json!("lost old requirement");
    assert!(verify_preparation(&root, &r, &changed).is_err());
    let mut bad = r.clone();
    bad.source_ids["supplier_html"] = json!("absent-source");
    assert!(prepare(&root, &bad).is_err());
    std::fs::write(root.path().join("var/project.json"), b"changed").unwrap();
    assert!(p.ensure_current(&root).is_err());
    assert!(prepare(&root, &r).is_err());
}
#[test]
fn same_revision_condition_change_cannot_reuse_context_or_preparation() {
    let (_t, root, mut r) = setup();
    let p = prepare(&root, &r).unwrap();
    let path = root.path().join("var/project.json");
    let mut project: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    project["conditions"]["study"] = json!("different same-revision conditions");
    let new_sha = save(&path, &project);
    let path = root.path().join("var/work.json");
    let mut work: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    work["project"]["sha256"] = json!(new_sha);
    r.work.sha256 = save(&path, &work);
    assert!(prepare(&root, &r).is_err());
    assert!(p.ensure_current(&root).is_err());
}
