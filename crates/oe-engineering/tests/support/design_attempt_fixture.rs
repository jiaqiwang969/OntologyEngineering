//! Synthetic declarations for orchestration tests; no physical/native success.
use crate::incad_fixture;
use oe_contracts::{ArtifactReference, IncadWorkPreparationRequest, ObjectIdentity};
use oe_core::SkillRoot;
use oe_kernel::project::{Project, context};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn quantity(value: f64, unit: &str) -> Value {
    json!({"value":value,"unit":unit,"precision":"independent synthetic input"})
}

pub fn save(path: &Path, value: &Value) -> String {
    incad_fixture::save(path, value)
}

pub struct Fixture {
    pub directory: tempfile::TempDir,
    pub root: SkillRoot,
    pub request: IncadWorkPreparationRequest,
    pub project: Project,
    pub object: ObjectIdentity,
    pub work: Value,
    pub basis: Value,
}

impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("SKILL.md"), "synthetic test fixture").unwrap();
        let root = SkillRoot::new(directory.path().canonicalize().unwrap()).unwrap();
        let library = incad_fixture::Fixture::new(&root.path().join("var/library"));
        let intake = oe_knowledge::incad::verify(&library.request).unwrap();
        let evidence: serde_json::Map<String, Value> = intake
            .roles()
            .iter()
            .map(|(role, pin)| (role.clone(), json!(pin.sha256)))
            .collect();
        let purpose = "Study guide motion under the declared load while retaining manufacturing and physical acceptance obligations";
        let acceptance = json!([
            "Guide elastic displacement at 10 N must be at most 0.02 mm",
            "Save and independently reopen the same native CAD revision",
            "Provide manufacturing dimensions, material, tolerances and inspection evidence",
            "Verify the physical guide under the declared load and measurement conditions"
        ]);
        let facts: Vec<_> = [
            ("force", quantity(10.0, "N")),
            ("span", quantity(100.0, "mm")),
            ("elastic_modulus", quantity(200000.0, "MPa")),
            ("second_moment", quantity(1000.0, "mm4")),
            ("support", json!("cantilever_fixed_free")),
            ("load_position", json!("end")),
            ("regime", json!("linear_elastic_small_deflection")),
            ("medium", json!("air")),
        ]
        .into_iter()
        .map(|(property, value)| {
            json!({
                "property":property,"value":value,"status":"declared","source_refs":["task"]
            })
        })
        .collect();
        let inputs = json!({"force":quantity(10.0,"N"),"span":quantity(100.0,"mm"),"elastic_modulus":quantity(200000.0,"MPa"),"second_moment":quantity(1000.0,"mm4")});
        let basis = json!({
            "schema":"oev2.engineering-basis/1","object_id":"guide","object_revision":"r1","purpose":purpose,
            "acceptance":acceptance,"facts":facts,"assertions":[
                {"assertion_id":"beam-boundary","kind":"model_boundary","subject":"stiffness","requirements":{"method":"beam-cantilever-end-load/1","inputs":inputs},"source_refs":["task"],"affected_decisions":[]},
                {"assertion_id":"source-boundary","kind":"source_applicability","subject":"supplier-page","requirements":{"limits":{"medium":["air"]}},"source_refs":["supplier_html"],"affected_decisions":[]},
                {"assertion_id":"guide-function","kind":"function_continuity","subject":"guide-workpiece","requirements":{"function_id":"guide-workpiece","inventory_scope":"declared_complete","current_realization":"linear guide support candidate","mechanism":"constrained translation","verification_plan":"independent native geometry and load measurements"},"source_refs":["task"],"affected_decisions":[]}
            ]
        });
        let project: Project = serde_json::from_value(json!({
            "schema":"oev2.project/1","project_id":"design-attempt-synthetic","revision":"r1","purpose":purpose,
            "objects":{"guide":"r1"},"conditions":{"study":basis.to_string()},"evidence":evidence,"decisions":[],"dependencies":{}
        })).unwrap();
        let object = ObjectIdentity {
            object_id: "guide".into(),
            revision: "r1".into(),
            condition_scope: "study".into(),
            source_sha256: intake.roles()["supplier_html"].sha256.clone(),
        };
        let project_sha = save(&root.path().join("var/project.json"), &json!(project));
        let work = json!({
            "schema":"oev2.engineering-work/3","project":{"path":"var/project.json","sha256":project_sha},
            "object":object,"project_context":context(&project,&object).unwrap(),"acceptance":acceptance,
            "necessary_functions":["guide-workpiece"],"physical_mechanisms":["constrained translation"],"gaps":[],"source_requests":[],
            "calculations":[{"calculation_id":"stiffness","object":object,"method":"beam-cantilever-end-load/1","inputs":inputs,
                "assumptions":["synthetic declared elastic support"],"validity_conditions":["linear elastic small displacement"],
                "invalid_when":["support or load differs; geometry is not independently verified"],"source_refs":["task"]}],
            "handoff":null
        });
        let work_sha = save(&root.path().join("var/work.json"), &work);
        let request = IncadWorkPreparationRequest {
            schema: "oev2.incad-work-preparation/1".into(),
            work: ArtifactReference {
                path: "var/work.json".into(),
                sha256: work_sha,
            },
            intake: library.request,
            source_ids: json!({"supplier_html":"supplier_html","projection_manifest":"projection_manifest","specifications":"specifications","task":"task"}),
        };
        Self {
            directory,
            root,
            request,
            project,
            object,
            work,
            basis,
        }
    }

    /// Rebind bytes and current context without changing any model assertion.
    /// A correct coordinator must still detect incompatible engineering facts.
    pub fn rebind(&mut self) {
        self.project
            .conditions
            .insert("study".into(), self.basis.to_string());
        let digest = save(
            &self.root.path().join("var/project.json"),
            &json!(self.project),
        );
        self.work["project"]["sha256"] = json!(digest);
        self.work["project_context"] = json!(context(&self.project, &self.object).unwrap());
        self.request.work.sha256 = save(&self.root.path().join("var/work.json"), &self.work);
    }

    pub fn reference(&self, name: &str, value: &Value) -> ArtifactReference {
        let path = format!("var/{name}.json");
        let sha256 = save(&self.root.path().join(&path), value);
        ArtifactReference { path, sha256 }
    }

    pub fn install_method_sources(&self) {
        let installed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        for relative in [
            "references/supplier-knowledge-policy.json",
            "references/context-routing-instructions.json",
            "runtime/semantic-bundles.json",
            "runtime/semantica-source-lock.json",
            "runtime/vendor/engineering-judgment-intake-0.2.2.sem-package.zip",
        ] {
            let destination = self.root.path().join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(installed.join(relative), &destination).unwrap();
        }
    }

    pub fn design_request(&self) -> oe_contracts::EngineeringDesignAttemptRequest {
        let original = self.reference("incad-request", &json!(self.request));
        let prepared = oe_engineering::incad::prepare(&self.root, &self.request).unwrap();
        let directory = self.root.path().join("var/incad-prepared");
        fs::create_dir_all(&directory).unwrap();
        for (name, bytes) in [
            (
                "request.json",
                fs::read(self.root.path().join(&original.path)).unwrap(),
            ),
            ("work.json", prepared.work_bytes.clone()),
            (
                "specifications.json",
                prepared.intake.specifications_bytes().to_vec(),
            ),
            (
                "zero-design-task.json",
                prepared.intake.task_bytes().to_vec(),
            ),
        ] {
            fs::write(directory.join(name), bytes).unwrap();
        }
        save(&directory.join("preparation.json"), &prepared.result);
        save(
            &directory.join("completed.json"),
            &json!({
                "status":"prepared","work_sha256":oe_core::sha256(&prepared.work_bytes),
                "output":directory,"execution_binding":{"scope":"synthetic direct library fixture; no CLI/native qualification"},
                "engineering_applicability":"not_reviewed","task_work_goal_relation":"not_reviewed",
                "native_execution":"not_run","engineering_adoption":"not_assessed"
            }),
        );
        prepared.ensure_current(&self.root).unwrap();
        let intent = self.reference("independent-intent", &json!({
            "schema":"oev2.design-intent/1","object":self.object,
            "statement":"Independently compare the declared cantilever support with a two-ended support; preserve the 10 N requirement and all manufacturing/physical obligations.",
            "source_refs":["task"],"alternatives":["cantilever support","two-ended support"],
            "unknowns":["material identity and actual support stiffness unmeasured","native CAD not yet created","manufacturing and physical acceptance not established"]
        }));
        let steps: Vec<_> = ["work", "harness", "cad_evidence", "handoff", "kernel_review", "feedback"]
            .into_iter().map(|activity| json!({
                "step_id":format!("step-{activity}"),"activity":activity,"acceptance":self.work["acceptance"],
                "required_evidence":[format!("actual {activity} evidence bound to the same intent, source and model revision")]
            })).collect();
        serde_json::from_value(json!({
            "schema":"oev2.design-attempt/1","attempt_id":"attempt-guide-01",
            "incad_request":original,"incad_prepared":"var/incad-prepared","independent_intent":intent,
            "tool_plan":steps,"reference_exposure":[{"stage":"before_freeze","basis":"This synthetic case and source family are already exposed development material; reference absence from a design window does not establish unseen validation.","source_refs":["supplier_html","task"]}]
        })).unwrap()
    }
}
