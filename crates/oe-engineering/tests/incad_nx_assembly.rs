//! Independent synthetic sources/package only. No executable, NX or model runs.
#[path = "../../oe-knowledge/tests/support/incad_cad_fixture.rs"]
mod fixture;
use fixture::{CadFixture, save};
use oe_contracts::*;
use oe_core::SkillRoot;
use oe_engineering::incad::cad::{
    self,
    nx_assembly::{prepare, verify},
};
use oe_kernel::project::{Project, context};
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn setup() -> (tempfile::TempDir, SkillRoot, IncadNxAssemblyBridgeRequest) {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("SKILL.md"),
        "synthetic bridge source boundary\n",
    )
    .unwrap();
    let root = SkillRoot::new(tmp.path().canonicalize().unwrap()).unwrap();
    let mut source = CadFixture::new(&root.path().join("var/library"));
    source.files.insert("model/assembly.step".into(),b"ISO-10303-21;HEADER;FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;#1=PRODUCT('one','','',());#2=PRODUCT('two','','',());ENDSEC;END-ISO-10303-21;".to_vec());
    source.files.insert("model/part.step".into(),b"ISO-10303-21;HEADER;FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));ENDSEC;DATA;#1=PRODUCT('other','','',());ENDSEC;END-ISO-10303-21;".to_vec());
    source.publish();
    let intake = oe_knowledge::incad::verify(&source.source.request).unwrap();
    let selected = oe_knowledge::incad::cad::verify(&intake, &["ap203".into()]).unwrap();
    let mut evidence: serde_json::Map<String, Value> = intake
        .roles()
        .iter()
        .map(|(k, v)| (k.clone(), json!(v.sha256)))
        .collect();
    let mut ids = serde_json::Map::new();
    for (index, file) in selected.files().iter().enumerate() {
        let id = format!("cad-{index}");
        evidence.insert(id.clone(), json!(file.identity.sha256));
        ids.insert(file.relative_path.clone(), json!(id));
    }
    let acceptance = vec![
        "complete original sources",
        "current task preserved",
        "reference exposure retained",
        "unit not inferred",
        "budget not geometry expectation",
        "no independent intent backfill",
        "native save/reopen later",
        "no physical adoption",
    ];
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"reference","object_revision":"r1","purpose":"reference source study","acceptance":acceptance,"facts":[],"assertions":[]});
    let project:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"bridge-source-test","revision":"r1","purpose":"reference source study","objects":{"reference":"r1"},"conditions":{"study":basis.to_string()},"evidence":evidence,"decisions":[],"dependencies":{}})).unwrap();
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
    let work = json!({"schema":"oev2.engineering-work/3","project":{"path":"var/project.json","sha256":project_sha},"object":object,"project_context":context(&project,&object).unwrap(),"acceptance":acceptance,"necessary_functions":["reference observation"],"physical_mechanisms":["source selection"],"gaps":[],"source_requests":[],"calculations":[],"handoff":null});
    let work_sha = save(&root.path().join("var/work.json"), &work);
    let selection = IncadCadSelectionRequest {
        schema: "oev2.incad-cad-selection/1".into(),
        preparation: IncadWorkPreparationRequest {
            schema: "oev2.incad-work-preparation/1".into(),
            work: ArtifactReference {
                path: "var/work.json".into(),
                sha256: work_sha,
            },
            intake: source.source.request,
            source_ids: json!({"supplier_html":"supplier_html","projection_manifest":"projection_manifest","specifications":"specifications","task":"task"}),
        },
        formats: vec!["ap203".into()],
        source_ids: json!(ids),
    };
    cad::stage(&root, &selection, Path::new("var/staged")).unwrap();
    let worker = root.path().join("var/worker");
    let payload = [
        (
            "worker/native.exe",
            b"MZsynthetic-never-executed".as_slice(),
        ),
        ("sources/main.rs", b"// synthetic, not compiled evidence"),
        (
            "build/recipe.txt",
            b"synthetic recipe, no toolchain execution",
        ),
    ];
    for (name, bytes) in payload {
        let path = worker.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let manifest = WorkerManifest {
        schema: WORKER_SCHEMA.into(),
        worker_id: "synthetic-reference-worker".into(),
        version: "test".into(),
        target: "x86_64-pc-windows-msvc".into(),
        executable: FileEntry::from_bytes(payload[0].0, payload[0].1).unwrap(),
        runtime_files: vec![],
        source_files: vec![FileEntry::from_bytes(payload[1].0, payload[1].1).unwrap()],
        build_recipe: FileEntry::from_bytes(payload[2].0, payload[2].1).unwrap(),
        toolchain: "synthetic-no-compiler".into(),
        required_runtime: RuntimeIdentity {
            product: "NX".into(),
            product_label: "synthetic-identity-only".into(),
            architecture: "x64".into(),
            libraries: vec![
                FileEntry::from_bytes("NXBIN/libufun.dll", b"synthetic-identity").unwrap(),
            ],
        },
        capabilities: vec![Capability {
            action: "import_step_assembly".into(),
            params_schema: "cad-agent.nx-native-assembly-params/v1".into(),
        }],
    };
    save(
        &worker.join("worker.json"),
        &serde_json::to_value(manifest).unwrap(),
    );
    let step = "cad/ap203/files/model/assembly.step";
    let request = IncadNxAssemblyBridgeRequest {
        schema: "oev2.incad-nx-assembly-bridge/1".into(),
        selection,
        staged_handoff: ArtifactReference {
            path: "var/staged/handoff.json".into(),
            sha256: oe_core::sha256(fs::read(root.path().join("var/staged/handoff.json")).unwrap()),
        },
        step_source: ArtifactReference {
            path: step.into(),
            sha256: oe_core::sha256(
                fs::read(root.path().join("var/staged/payload").join(step)).unwrap(),
            ),
        },
        source_role: "supplier_reference_assembly".into(),
        budget: NxAssemblyBudget {
            max_occurrences: 100,
            max_prototypes: 20,
            max_depth: 10,
        },
        worker_directory: "var/worker".into(),
        job_id: "reference-assembly-bridge".into(),
        root_identity: json!({"host_id":"synthetic-host","volume_id":"synthetic-volume","directory_id":"synthetic-directory"}),
    };
    (tmp, root, request)
}
#[test]
fn selected_sources_bind_current_work_and_actual_nx_job_without_adoption() {
    let (_tmp, root, request) = setup();
    let source_before = fs::read(root.path().join("var/staged/handoff.json")).unwrap();
    let work_before = fs::read(root.path().join("var/work.json")).unwrap();
    let result = prepare(&root, &request, Path::new("var/bridge")).unwrap();
    assert_eq!(result.native_execution, "not_run");
    assert_eq!(result.engineering_adoption, "not_assessed");
    assert_eq!(
        result.source_binding["selected_step"]["supplier_option"],
        "ap203"
    );
    assert_eq!(
        result.source_binding["selected_step"]["actual_header"]["actual_file_schema"],
        "AUTOMOTIVE_DESIGN"
    );
    let package =
        oe_cad::nx::from_directory(&root, &root.path().join("var/bridge/prepared/job")).unwrap();
    let parameters: Value = serde_json::from_slice(&package.members["params.json"]).unwrap();
    assert_eq!(
        parameters,
        json!({"schema":"cad-agent.nx-native-assembly-params/v1","action":"import_step_assembly","step":"reference/payload/cad/ap203/files/model/assembly.step","budget":{"max_occurrences":100,"max_prototypes":20,"max_depth":10}})
    );
    assert_eq!(package.job.inputs.len(), 10); // ZIP + two members + six original records + binding.
    assert_eq!(
        package.members["inputs/reference/records/handoff.json"],
        source_before
    );
    assert_eq!(
        package.members["inputs/reference/records/work.json"],
        work_before
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&work_before).unwrap()["acceptance"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        result.job_binding["job_digest"],
        serde_json::to_value(package.job.digest().unwrap()).unwrap()
    );
    assert_eq!(
        verify(&root, &request, Path::new("var/bridge"))
            .unwrap()
            .status,
        "verified"
    );
    assert_eq!(
        fs::read(root.path().join("var/staged/handoff.json")).unwrap(),
        source_before
    );
    assert!(prepare(&root, &request, Path::new("var/bridge")).is_err());
}
#[test]
fn wrong_selected_member_role_format_budget_and_root_identity_are_refused() {
    let (_tmp, root, request) = setup();
    for index in 0..6 {
        let mut bad = request.clone();
        match index {
            0 => bad.step_source.path = "cad/ap203/files/missing.step".into(),
            1 => bad.step_source.sha256 = "0".repeat(64),
            2 => bad.source_role = "independent_design".into(),
            3 => bad.selection.formats = vec!["dxf".into()],
            4 => bad.budget.max_occurrences = 0,
            _ => bad.root_identity["unexpected"] = json!(true),
        };
        assert!(prepare(&root, &bad, Path::new(&format!("var/bad-{index}"))).is_err());
    }
    assert!(!root.path().join("var/bad-0").exists());
}
#[test]
fn same_revision_project_work_and_staged_closure_changes_block_reuse() {
    for index in 0..4 {
        let (_tmp, root, request) = setup();
        prepare(&root, &request, Path::new("var/bridge")).unwrap();
        match index {
            0 => {
                let p = root.path().join("var/project.json");
                let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
                v["purpose"] = json!("changed without revision");
                save(&p, &v);
            }
            1 => {
                let p = root.path().join("var/work.json");
                let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
                v["acceptance"] = json!(["shortened"]);
                save(&p, &v);
            }
            2 => {
                fs::remove_file(
                    root.path()
                        .join("var/staged/payload/cad/ap203/files/model/part.step"),
                )
                .unwrap();
            }
            _ => {
                fs::write(
                    root.path().join("var/staged/payload/extra.step"),
                    b"unbound",
                )
                .unwrap();
            }
        }
        assert!(verify(&root, &request, Path::new("var/bridge")).is_err());
    }
}
#[test]
fn rebound_prepared_authority_and_changed_worker_cannot_qualify_original_job() {
    for index in 0..3 {
        let (_tmp, root, request) = setup();
        prepare(&root, &request, Path::new("var/bridge")).unwrap();
        match index {
            0 => {
                let p = root.path().join("var/bridge/completed.json");
                let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
                v["native_execution"] = json!("completed");
                save(&p, &v);
            }
            1 => {
                let p = root.path().join("var/bridge/prepared/prepared.json");
                let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
                v["engineering_acceptance"] = json!("accepted");
                save(&p, &v);
            }
            _ => {
                fs::write(root.path().join("var/worker/sources/main.rs"), b"changed").unwrap();
            }
        }
        assert!(verify(&root, &request, Path::new("var/bridge")).is_err());
    }
}
#[test]
fn assembly_header_is_separate_from_original_single_part_identity_guard() {
    let bytes=b"ISO-10303-21;HEADER;FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;#1=PRODUCT('one','','',());#2=PRODUCT('two','','',());ENDSEC;END-ISO-10303-21;";
    assert_eq!(
        oe_cad::supplier::artifacts::inspect_step_header(bytes).unwrap()["actual_file_schema"],
        "AUTOMOTIVE_DESIGN"
    );
    assert!(
        oe_cad::supplier::artifacts::inspect_file("assembly.step", bytes, "one", "STEP(AP203)")
            .is_err()
    );
    assert!(
        oe_cad::supplier::artifacts::inspect_step_header(
            b"ISO-10303-21;/* FILE_SCHEMA(('AUTOMOTIVE_DESIGN')); */END-ISO-10303-21;"
        )
        .is_err()
    );
    assert!(oe_cad::supplier::artifacts::inspect_step_header(b"ISO-10303-21;FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));END-ISO-10303-21;").is_err());
}
