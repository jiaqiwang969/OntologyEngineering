// Synthetic fixture derived from the frozen kernel learning regression inputs.
#![allow(dead_code)]
#[cfg(windows)]
pub mod windows_authority;
use oe_core::{SkillRoot, sha256};
use oe_kernel::learning;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub root: SkillRoot,
    pub input: Value,
    pub case: Value,
}
impl Fixture {
    pub fn new(id: &str, mode: &str) -> Self {
        let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let scratch = actual.join("var/builds");
        oe_core::ensure_directory(&scratch).unwrap();
        // SkillRoot uses the canonical Windows prefix. Construct fixtures with
        // that same spelling; keep portable_source_path lexical and strict.
        let temp = tempfile::tempdir_in(scratch.canonicalize().unwrap()).unwrap();
        let path = temp.path();
        fs::write(path.join("SKILL.md"), "private learning test").unwrap();
        fs::create_dir_all(path.join("runtime/vendor")).unwrap();
        let lock: Value = serde_json::from_slice(
            &fs::read(actual.join("runtime/semantic-bundles.json")).unwrap(),
        )
        .unwrap();
        fs::copy(
            actual.join("runtime/semantic-bundles.json"),
            path.join("runtime/semantic-bundles.json"),
        )
        .unwrap();
        fs::copy(
            actual.join("runtime/semantica-source-lock.json"),
            path.join("runtime/semantica-source-lock.json"),
        )
        .unwrap();
        for bundle in lock["bundles"].as_object().unwrap().values() {
            let p = bundle["path"].as_str().unwrap();
            fs::copy(actual.join(p), path.join(p)).unwrap();
        }
        fs::create_dir_all(path.join("crates/oe-kernel/src")).unwrap();
        fs::copy(
            actual.join("crates/oe-kernel/src/learning.rs"),
            path.join("crates/oe-kernel/src/learning.rs"),
        )
        .unwrap();
        fs::create_dir_all(path.join("var/inputs")).unwrap();
        let put = |name: &str, bytes: &[u8]| -> Value {
            let p = format!("var/inputs/{name}");
            fs::write(path.join(&p), bytes).unwrap();
            json!({"path":p,"sha256":sha256(bytes)})
        };
        let jput = |name: &str, v: Value| put(name, &serde_json::to_vec(&v).unwrap());
        let evidence=b"@prefix ee: <urn:semantica:engineering-evidence:> .\n<urn:fixture:review> a ee:ReviewClaim ; ee:subjectRevision \"r1\" ; ee:scope \"declared_fixture\" ; ee:method \"pattern-scope\" ; ee:claim_scope \"declared_fixture\" ; ee:evidence_scope \"declared_fixture\" ; ee:required_conditions \"flow\", \"fluid\" ; ee:required_conditionsComplete true ; ee:established_conditions \"flow\" ; ee:established_conditionsComplete true ; ee:inventory_complete true ; ee:representation \"applicability_reviewed\" .\n";
        let e = put("evidence.ttl", evidence);
        let scope = "declared_fixture";
        let project = jput(
            "project.json",
            json!({"schema":"oev2.project/1","project_id":"fixture-project","revision":"r1","purpose":"diagnose undeclared physical conditions without adopting a product", "objects":{"urn:fixture:review":"r1"},"conditions":{scope:"declared input scope; actual fluid unknown"},"evidence":{"fixture":sha256(evidence)},"decisions":[],"dependencies":{}}),
        );
        let bundle = &lock["bundles"]["engineering-judgment-intake"];
        let binding = jput(
            "binding.json",
            json!({"project_id":"fixture-project","object_id":"urn:fixture:review","object_revision":"r1","package_id":bundle["package_id"],"package_version":bundle["package_version"],"package_sha256":bundle["sha256"],"source_sha256":sha256(evidence),"condition_scope":scope,"required_checks":["findings-pattern-scope","shape-pattern-scope","bound_focus_context"],"authority_id":"private-test-only"}),
        );
        let case = json!({"case_id":"case-a","source_family":"fixture-family-a","exposure":"development_replay","project":project,"binding":binding,"evidence":e,"bundle":"engineering-judgment-intake","query_ids":["findings-pattern-scope"],"shape_ids":["shape-pattern-scope"],"expected_conclusion":"unknown","required_reasons":["required_condition_not_established"]});
        let goal = jput(
            "goal.json",
            json!({"purpose":"retain unknowns and recover required conditions","engineering_adoption":"not_authorized"}),
        );
        let contract = jput(
            "evaluation.json",
            json!({"schema":"oev2.experience-evaluation/1","goal_sha256":goal["sha256"],"model":format!("{}@{}",oe_kernel::ENGINE_ID,oe_kernel::ENGINE_VERSION),"repetitions":1,"budget_max_checks":16,"hard_conditions":["identity","units","conditions","source","unknown_state","necessary_acceptance","evaluator_integrity"],"cases":[case]}),
        );
        let baseline = jput("baseline.json", json!({"mode":"no_semantic_review"}));
        let candidate = jput("candidate.json", json!({"mode":mode}));
        let source = put(
            "source.txt",
            b"synthetic causal evidence; not a physical measurement",
        );
        let experience = json!({"candidate_id":id,"kind":"operation_method","project_id":"fixture-project","object":{"object_id":"urn:fixture:review","revision":"r1","condition_scope":scope,"source_sha256":source["sha256"]},"purpose":"diagnose conditions before adoption","function":"execute existing source-bound applicability checks","mechanism":"unknown physical mechanism; diagnostic procedure only","applicability":["source-bound declared condition inventory"],"sources":[{"source_id":"fixture-source","source_sha256":source["sha256"],"source_uri":source["path"],"extraction_version":"synthetic-test/1","physical_page":null}],"counterexamples":["a completed tool execution does not prove physical acceptance"],"unknowns":["fluid properties and engineering costs"],"verification_requirements":["independent task and physical evidence"],"existing_expression":"pattern-scope","ontology_delta":"no_delta","hypotheses":["the existing Rust review makes condition gaps explicit"]});
        let input = json!({"schema":"oev2.experience-freeze/1","candidate":experience,"baseline":baseline,"candidate_plan":candidate,"goal":goal,"evaluation":contract,"prior_rejection":null,"new_basis":[]});
        let root = SkillRoot::new(path).unwrap();
        Self {
            temp,
            root,
            input,
            case,
        }
    }
    pub fn freeze(&self) -> PathBuf {
        let p = PathBuf::from("var/candidates/candidate");
        learning::freeze(&self.root, &self.input, &p).unwrap();
        p
    }
    pub fn read(&self, p: &str) -> Value {
        serde_json::from_slice(&fs::read(self.temp.path().join(p)).unwrap()).unwrap()
    }
}

pub fn protocol_two(f: &mut Fixture) {
    let put = |name: &str, value: &Value| {
        let path = format!("var/inputs/{name}");
        let bytes = serde_json::to_vec(value).unwrap();
        fs::write(f.temp.path().join(&path), &bytes).unwrap();
        json!({"path":path,"sha256":sha256(bytes)})
    };
    let mut project: oe_kernel::project::Project =
        serde_json::from_value(f.read("var/inputs/project.json")).unwrap();
    let mut binding: oe_kernel::Binding =
        serde_json::from_value(f.read("var/inputs/binding.json")).unwrap();
    let old_evidence = fs::read(
        f.temp
            .path()
            .join(f.case["evidence"]["path"].as_str().unwrap()),
    )
    .unwrap();
    let condition_hash = sha256(project.conditions[&binding.condition_scope].as_bytes());
    let mut evidence = old_evidence;
    evidence.extend_from_slice(
        format!(
            "\n<{}> <urn:oev2:project-context:conditionSha256> \"{condition_hash}\" .\n",
            binding.object_id
        )
        .as_bytes(),
    );
    let evidence_path = "var/inputs/evidence-current.ttl";
    fs::write(f.temp.path().join(evidence_path), &evidence).unwrap();
    binding.source_sha256 = sha256(&evidence);
    project
        .evidence
        .insert("fixture".into(), binding.source_sha256.clone());
    f.case["project"] = put(
        "project-current.json",
        &serde_json::to_value(&project).unwrap(),
    );
    f.case["evidence"] = json!({"path":evidence_path,"sha256":sha256(&evidence)});
    let object = oe_contracts::ObjectIdentity {
        object_id: binding.object_id.clone(),
        revision: binding.object_revision.clone(),
        condition_scope: binding.condition_scope.clone(),
        source_sha256: binding.source_sha256.clone(),
    };
    binding.project_context = Some(oe_kernel::project::context(&project, &object).unwrap());
    f.case["binding"] = put(
        "binding-current.json",
        &serde_json::to_value(binding).unwrap(),
    );
    f.case["current_use"] = json!(true);
    let mut evaluation = f.read("var/inputs/evaluation.json");
    evaluation["schema"] = json!("oev2.experience-evaluation/2");
    evaluation["cases"] = json!([f.case]);
    f.input["evaluation"] = put("evaluation-current.json", &evaluation);
    f.input["baseline"] = put(
        "baseline-current.json",
        &json!({"schema":"oev2.method-plan/2","mode":"required_semantic_review","retrieval_order":"context_first"}),
    );
    f.input["candidate_plan"] = put(
        "candidate-current.json",
        &json!({"schema":"oev2.method-plan/2","mode":"required_semantic_review","retrieval_order":"query_first"}),
    );
    f.input["schema"] = json!("oev2.experience-freeze/2");
    let candidate = &f.input["candidate"];
    let experiment = json!({"schema":"oev2.experience-experiment/2","experiment_id":"stable-experiment","project_id":candidate["project_id"],
        "object_id":candidate["object"]["object_id"],"condition_scope":candidate["object"]["condition_scope"],
        "kind":candidate["kind"],"applicability":candidate["applicability"],
        "hypotheses":{"stable-hypothesis":candidate["hypotheses"][0]},"goal_sha256":f.input["goal"]["sha256"]});
    f.input["retry"] = json!({"experiment":put("experiment.json",&experiment),"hypothesis_ids":["stable-hypothesis"],"prior_rejections":[]});
}
