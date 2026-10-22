use oe_core::{SkillRoot, sha256};
use oe_kernel::learning;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    temp: tempfile::TempDir,
    root: SkillRoot,
    input: Value,
    case: Value,
}

fn ledger(f: &Fixture, id: &str) -> PathBuf {
    f.temp
        .path()
        .join(format!("var/state/knowledge/experience-events/{id}"))
}

fn ledger_bytes(f: &Fixture, id: &str) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(ledger(f, id))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|path| {
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect()
}

// Explicitly synthetic repetitions of one real Feedback event exercise the
// history boundary without hundreds of redundant semantic executions.
fn seed_feedback_history(f: &Fixture, id: &str, count: usize) {
    let dir = ledger(f, id);
    let history = ledger_bytes(f, id);
    let mut previous_bytes = history.last_key_value().unwrap().1.clone();
    let template: Value = serde_json::from_slice(&previous_bytes).unwrap();
    assert_eq!(template["event"], "Feedback");
    for sequence in history.len()..count {
        let mut event = template.clone();
        event["sequence"] = json!(sequence);
        event["previous_sha256"] = json!(sha256(&previous_bytes));
        previous_bytes = serde_json::to_vec(&event).unwrap();
        fs::write(dir.join(format!("{sequence:04}.json")), &previous_bytes).unwrap();
    }
}

fn unrelated_input(f: &Fixture, id: &str) -> Value {
    let put = |name: &str, value: &Value| {
        let path = format!("var/inputs/{name}");
        let bytes = serde_json::to_vec(value).unwrap();
        fs::write(f.temp.path().join(&path), &bytes).unwrap();
        json!({"path":path,"sha256":sha256(bytes)})
    };
    let mut next = f.input.clone();
    next["candidate"]["candidate_id"] = json!(id);
    next["candidate"]["project_id"] = json!("unrelated-project");
    next["candidate"]["hypotheses"] = json!(["a distinct unrelated project hypothesis"]);
    let mut project: oe_kernel::project::Project =
        serde_json::from_value(f.read("var/inputs/project-current.json")).unwrap();
    let mut binding: oe_kernel::Binding =
        serde_json::from_value(f.read("var/inputs/binding-current.json")).unwrap();
    project.project_id = "unrelated-project".into();
    binding.project_id = project.project_id.clone();
    let object = oe_contracts::ObjectIdentity {
        object_id: binding.object_id.clone(),
        revision: binding.object_revision.clone(),
        condition_scope: binding.condition_scope.clone(),
        source_sha256: binding.source_sha256.clone(),
    };
    binding.project_context = Some(oe_kernel::project::context(&project, &object).unwrap());
    let mut case = f.case.clone();
    case["project"] = put(
        "unrelated-project.json",
        &serde_json::to_value(project).unwrap(),
    );
    case["binding"] = put(
        "unrelated-binding.json",
        &serde_json::to_value(binding).unwrap(),
    );
    let mut evaluation = f.read("var/inputs/evaluation-current.json");
    evaluation["cases"] = json!([case]);
    next["evaluation"] = put("unrelated-evaluation.json", &evaluation);
    let candidate = &next["candidate"];
    let experiment = json!({"schema":"oev2.experience-experiment/2","experiment_id":"unrelated-capacity-experiment",
        "project_id":candidate["project_id"],"object_id":candidate["object"]["object_id"],
        "condition_scope":candidate["object"]["condition_scope"],"kind":candidate["kind"],
        "applicability":candidate["applicability"],"hypotheses":{"unrelated-hypothesis":candidate["hypotheses"][0]},
        "goal_sha256":next["goal"]["sha256"]});
    next["retry"] = json!({"experiment":put("unrelated-experiment.json", &experiment),
        "hypothesis_ids":["unrelated-hypothesis"],"prior_rejections":[]});
    next
}

fn assert_event_count(f: &Fixture, id: &str, expected: usize) {
    let listed = learning::list(&f.root, "").unwrap();
    let candidate = listed["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["candidate"]["candidate_id"] == id)
        .unwrap();
    assert_eq!(candidate["event_count"], expected);
}

#[test]
fn history_capacity_preserves_a_recovery_slot_and_keeps_other_candidates_available() {
    let id = "capacity-rejected";
    let mut f = Fixture::new(id, "required_semantic_review");
    protocol_two(&mut f);
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    learning::decide(&f.root, &p, "reject", "no demonstrated improvement").unwrap();
    learning::apply(&f.root, &p, &f.case, Path::new("var/first-feedback")).unwrap();
    seed_feedback_history(&f, id, 254);
    learning::apply(&f.root, &p, &f.case, Path::new("var/last-feedback")).unwrap();
    assert_event_count(&f, id, 255);
    let before = ledger_bytes(&f, id);
    let error =
        learning::apply(&f.root, &p, &f.case, Path::new("var/refused-feedback")).unwrap_err();
    assert!(error.to_string().contains("experience_history_budget"));
    assert!(!f.temp.path().join("var/refused-feedback").exists());
    assert_eq!(ledger_bytes(&f, id), before);
    assert_event_count(&f, id, 255);
    learning::recover(
        &f.root,
        &p,
        "retain exact baseline",
        Path::new("var/recovered"),
    )
    .unwrap();
    assert_event_count(&f, id, 256);
    assert_eq!(
        fs::read(f.temp.path().join("var/recovered/baseline-plan.json")).unwrap(),
        fs::read(f.temp.path().join(&p).join("baseline-plan.json")).unwrap()
    );
    let before = ledger_bytes(&f, id);
    let error = learning::apply(&f.root, &p, &f.case, Path::new("var/refused-257")).unwrap_err();
    assert!(error.to_string().contains("experience_history_budget"));
    assert!(!f.temp.path().join("var/refused-257").exists());
    assert_eq!(ledger_bytes(&f, id), before);
    assert_event_count(&f, id, 256);
    learning::freeze(
        &f.root,
        &unrelated_input(&f, "capacity-unrelated"),
        Path::new("var/candidates/unrelated"),
    )
    .unwrap();
    assert_event_count(&f, "capacity-unrelated", 1);
}

#[test]
fn full_legacy_history_refuses_feedback_and_recovery_before_creating_outputs() {
    let id = "capacity-legacy";
    let mut f = Fixture::new(id, "required_semantic_review");
    protocol_two(&mut f);
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    learning::decide(&f.root, &p, "reject", "no demonstrated improvement").unwrap();
    learning::apply(&f.root, &p, &f.case, Path::new("var/first-feedback")).unwrap();
    seed_feedback_history(&f, id, 256);
    assert_event_count(&f, id, 256);
    let before = ledger_bytes(&f, id);
    let error =
        learning::apply(&f.root, &p, &f.case, Path::new("var/refused-feedback")).unwrap_err();
    assert!(error.to_string().contains("experience_history_budget"));
    let error = learning::recover(
        &f.root,
        &p,
        "cannot append beyond frozen budget",
        Path::new("var/refused-recovery"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("experience_history_budget"));
    assert!(!f.temp.path().join("var/refused-feedback").exists());
    assert!(!f.temp.path().join("var/refused-recovery").exists());
    assert_eq!(ledger_bytes(&f, id), before);
    assert_event_count(&f, id, 256);
    learning::freeze(
        &f.root,
        &unrelated_input(&f, "legacy-unrelated"),
        Path::new("var/candidates/unrelated"),
    )
    .unwrap();
    assert_event_count(&f, "legacy-unrelated", 1);
}

#[test]
fn active_trial_keeps_capacity_for_restore_and_full_legacy_trial_stays_unchanged() {
    let id = "capacity-active";
    let f = Fixture::new(id, "required_semantic_review");
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    learning::decide(&f.root, &p, "accept-trial", "bounded diagnostic gain").unwrap();
    learning::apply(&f.root, &p, &f.case, Path::new("var/first-feedback")).unwrap();
    seed_feedback_history(&f, id, 254);
    learning::apply(&f.root, &p, &f.case, Path::new("var/last-feedback")).unwrap();
    let before = ledger_bytes(&f, id);
    let error =
        learning::apply(&f.root, &p, &f.case, Path::new("var/refused-feedback")).unwrap_err();
    assert!(error.to_string().contains("experience_history_budget"));
    assert!(!f.temp.path().join("var/refused-feedback").exists());
    assert_eq!(ledger_bytes(&f, id), before);
    assert_event_count(&f, id, 255);
    // A pre-existing full active ledger is readable; capacity refusal must not
    // pretend it restored the trial or leave a new baseline output behind.
    seed_feedback_history(&f, id, 256);
    let full = ledger_bytes(&f, id);
    let error = learning::recover(
        &f.root,
        &p,
        "legacy capacity cannot be exceeded",
        Path::new("var/refused-recovery"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("experience_history_budget"));
    assert!(!f.temp.path().join("var/refused-recovery").exists());
    assert_eq!(ledger_bytes(&f, id), full);
    assert_event_count(&f, id, 256);
    assert_eq!(
        f.read("var/state/knowledge/experience-events/capacity-active/0255.json")["state"],
        "trial_active"
    );
}

#[test]
fn competing_history_writer_is_refused_before_trial_or_recovery_outputs() {
    let id = "capacity-busy";
    let mut f = Fixture::new(id, "required_semantic_review");
    protocol_two(&mut f);
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    learning::decide(&f.root, &p, "reject", "no demonstrated improvement").unwrap();
    let before = ledger_bytes(&f, id);
    let _guard = oe_core::AdvisoryFileLock::acquire(&ledger(&f, id).join(".lock"), true).unwrap();
    let error =
        learning::apply(&f.root, &p, &f.case, Path::new("var/refused-feedback")).unwrap_err();
    assert!(error.to_string().contains("resource_busy"));
    let error = learning::recover(
        &f.root,
        &p,
        "writer is already busy",
        Path::new("var/refused-recovery"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("resource_busy"));
    assert!(!f.temp.path().join("var/refused-feedback").exists());
    assert!(!f.temp.path().join("var/refused-recovery").exists());
    assert_eq!(ledger_bytes(&f, id), before);
    assert_event_count(&f, id, 3);
}

#[test]
fn active_trial_uses_its_last_reserved_slot_to_restore_exact_baseline() {
    let id = "capacity-active-restore";
    let f = Fixture::new(id, "required_semantic_review");
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    learning::decide(&f.root, &p, "accept-trial", "bounded diagnostic gain").unwrap();
    learning::apply(&f.root, &p, &f.case, Path::new("var/first-feedback")).unwrap();
    seed_feedback_history(&f, id, 255);
    assert_event_count(&f, id, 255);
    let before = ledger_bytes(&f, id);
    learning::recover(
        &f.root,
        &p,
        "restore without consuming an extra feedback slot",
        Path::new("var/restored"),
    )
    .unwrap();
    assert_event_count(&f, id, 256);
    let after = ledger_bytes(&f, id);
    assert!(
        before
            .iter()
            .all(|(path, bytes)| after.get(path) == Some(bytes))
    );
    assert_eq!(
        f.read("var/state/knowledge/experience-events/capacity-active-restore/0255.json")["state"],
        "restored"
    );
    assert_eq!(
        fs::read(f.temp.path().join("var/restored/baseline-plan.json")).unwrap(),
        fs::read(f.temp.path().join(&p).join("baseline-plan.json")).unwrap()
    );
}
impl Fixture {
    fn new(id: &str, mode: &str) -> Self {
        let actual = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let temp = tempfile::tempdir_in(actual.join("var/builds")).unwrap();
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
    fn freeze(&self) -> PathBuf {
        let p = PathBuf::from("var/candidates/candidate");
        learning::freeze(&self.root, &self.input, &p).unwrap();
        p
    }
    fn read(&self, p: &str) -> Value {
        serde_json::from_slice(&fs::read(self.temp.path().join(p)).unwrap()).unwrap()
    }
}

#[test]
fn external_project_store_freezes_equivalent_bytes_and_keeps_software_evaluator() {
    let f = Fixture::new("external-store", "required_semantic_review");
    let candidate = f.freeze();
    let before = learning::assess(&f.root, &candidate, Path::new("var/assessment.json")).unwrap();
    let original_manifest = fs::read(f.root.path().join(&candidate).join("freeze.json")).unwrap();
    let home = tempfile::tempdir().unwrap();
    let home_path = fs::canonicalize(home.path()).unwrap();
    fs::create_dir_all(home_path.join("var")).unwrap();
    fs::rename(
        f.root.path().join("var/inputs"),
        home_path.join("var/inputs"),
    )
    .unwrap();
    // Similarly named data cannot replace the installation's fixed evaluator.
    fs::create_dir_all(home_path.join("crates/oe-kernel/src")).unwrap();
    fs::write(
        home_path.join("crates/oe-kernel/src/learning.rs"),
        b"untrusted project shadow",
    )
    .unwrap();
    let bound = f.root.clone().with_project_data_root(&home_path).unwrap();
    learning::freeze(&bound, &f.input, &candidate).unwrap();
    assert_eq!(
        fs::read(home_path.join(&candidate).join("freeze.json")).unwrap(),
        original_manifest
    );
    let after = learning::assess(&bound, &candidate, Path::new("var/assessment.json")).unwrap();
    assert_eq!(
        before["assessment"]["verdict"],
        after["assessment"]["verdict"]
    );
    let before_record: Value =
        serde_json::from_slice(&fs::read(f.root.path().join("var/assessment.json")).unwrap())
            .unwrap();
    let after_record: Value =
        serde_json::from_slice(&fs::read(home_path.join("var/assessment.json")).unwrap()).unwrap();
    assert_eq!(before_record["goal_sha256"], after_record["goal_sha256"]);
    assert_eq!(
        before_record["evaluator_sha256"],
        after_record["evaluator_sha256"]
    );
    assert_eq!(after_record["engineering_acceptance"], "not_established");
    assert!(
        home_path
            .join("var/state/knowledge/experience-events/external-store/0001.json")
            .is_file()
    );
    // Removing a data input does not cause a retry to read an installation copy.
    let source = f.input["candidate"]["sources"][0]["source_uri"]
        .as_str()
        .unwrap();
    fs::create_dir_all(f.root.path().join("var/inputs")).unwrap();
    fs::copy(home_path.join(source), f.root.path().join(source)).unwrap();
    fs::remove_file(home_path.join(source)).unwrap();
    let mut another = f.input.clone();
    another["candidate"]["candidate_id"] = json!("external-missing-source");
    assert!(
        learning::freeze(&bound, &another, Path::new("var/candidates/missing-source")).is_err()
    );
    assert!(
        !home_path
            .join("var/candidates/missing-source/freeze.json")
            .exists()
    );
}

fn protocol_two(f: &mut Fixture) {
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

#[test]
fn multiple_rejections_require_complete_refs_and_distinct_new_basis() {
    let mut f = Fixture::new("retry-v2-first", "required_semantic_review");
    protocol_two(&mut f);
    let first = f.freeze();
    let assessed =
        learning::assess(&f.root, &first, Path::new("var/v2-assessment-1.json")).unwrap();
    assert_eq!(
        assessed["assessment"]["verdict"], "no_delta",
        "both sides retain necessary reviews"
    );
    learning::decide(&f.root, &first, "reject", "no demonstrated improvement").unwrap();
    let reference = |path: &str| json!({"path":path,"sha256":sha256(fs::read(f.temp.path().join(path)).unwrap())});
    let rejection1 = reference("var/state/knowledge/experience-events/retry-v2-first/0002.json");
    let basis = |name: &str, bytes: &[u8]| {
        let path = format!("var/inputs/{name}");
        fs::write(f.temp.path().join(&path), bytes).unwrap();
        json!({"path":path,"sha256":sha256(bytes)})
    };
    let new2 = basis(
        "distinct-2.txt",
        b"new verifiable diagnostic bytes 2, physical value unknown",
    );
    let mut second = f.input.clone();
    second["candidate"]["candidate_id"] = json!("retry-v2-second");
    second["retry"]["prior_rejections"] = json!([rejection1]);
    assert!(
        learning::freeze(&f.root, &second, Path::new("var/candidates/missing-basis"))
            .unwrap_err()
            .to_string()
            .contains("needs_new_basis")
    );
    second["new_basis"] = json!([new2]);
    second["candidate"]["sources"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "source_id":"new-source","source_uri":new2["path"],"source_sha256":new2["sha256"],
        "extraction_version":"test/2","physical_page":null}));
    let second_path = Path::new("var/candidates/second");
    learning::freeze(&f.root, &second, second_path).unwrap();
    learning::assess(&f.root, second_path, Path::new("var/v2-assessment-2.json")).unwrap();
    learning::decide(
        &f.root,
        second_path,
        "reject",
        "still no demonstrated improvement",
    )
    .unwrap();
    let rejection2 = reference("var/state/knowledge/experience-events/retry-v2-second/0002.json");
    let mut third = second.clone();
    third["candidate"]["candidate_id"] = json!("retry-v2-third");
    third["retry"]["prior_rejections"] = json!([rejection2]);
    third["new_basis"] = json!([basis("distinct-3.txt", b"third actual new source bytes")]);
    assert!(
        learning::freeze(&f.root, &third, Path::new("var/candidates/incomplete"))
            .unwrap_err()
            .to_string()
            .contains("related_rejections_incomplete")
    );
    third["retry"]["prior_rejections"] = json!([rejection1, rejection2]);
    let mut forged = third.clone();
    forged["retry"]["prior_rejections"][0]["sha256"] = json!("0".repeat(64));
    assert!(
        learning::freeze(&f.root, &forged, Path::new("var/candidates/forged"))
            .unwrap_err()
            .to_string()
            .contains("prior_rejection_invalid")
    );
    let mut copied = third.clone();
    copied["retry"]["prior_rejections"][0] = basis(
        "copied-rejection.json",
        &fs::read(
            f.temp
                .path()
                .join("var/state/knowledge/experience-events/retry-v2-first/0002.json"),
        )
        .unwrap(),
    );
    assert!(
        learning::freeze(&f.root, &copied, Path::new("var/candidates/copied"))
            .unwrap_err()
            .to_string()
            .contains("not_in_registered_history")
    );
    let mut repeated = third.clone();
    repeated["new_basis"] = json!([new2]);
    assert!(
        learning::freeze(&f.root, &repeated, Path::new("var/candidates/repeated"))
            .unwrap_err()
            .to_string()
            .contains("repeats_old_source")
    );
    let mut renamed = third.clone();
    renamed["candidate"]["hypotheses"] = json!(["same intended mechanism with renamed wording"]);
    assert!(
        learning::freeze(&f.root, &renamed, Path::new("var/candidates/renamed"))
            .unwrap_err()
            .to_string()
            .contains("registered_hypothesis_changed")
    );
    let result = learning::freeze(&f.root, &third, Path::new("var/candidates/third")).unwrap();
    assert_eq!(
        result["event"]["body"]["manifest"]["retry_history"]["related_rejection_count"],
        2
    );
    assert_eq!(
        f.read("var/state/knowledge/experience-events/retry-v2-first/0002.json")["state"],
        "rejected"
    );
    assert_eq!(
        f.read("var/state/knowledge/experience-events/retry-v2-second/0002.json")["state"],
        "rejected"
    );
}

#[test]
fn current_trial_rejects_same_scope_old_rdf_after_project_condition_change() {
    let mut f = Fixture::new("current-conditions", "required_semantic_review");
    protocol_two(&mut f);
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/current-assessment.json")).unwrap();
    learning::decide(&f.root, &p, "reject", "no gain").unwrap();
    let mut changed = f.read("var/inputs/project-current.json");
    changed["conditions"]["declared_fixture"] =
        json!("load and support changed; old required fluid still unknown");
    let bytes = serde_json::to_vec(&changed).unwrap();
    fs::write(f.temp.path().join("var/inputs/changed.json"), &bytes).unwrap();
    let mut task = f.case.clone();
    task["project"] = json!({"path":"var/inputs/changed.json","sha256":sha256(bytes)});
    let trial = learning::apply(&f.root, &p, &task, Path::new("var/current-trial")).unwrap();
    assert_eq!(
        trial["trial"]["result"]["receipt"]["conclusion"],
        "violated"
    );
    let checks = trial["trial"]["result"]["receipt"]["checks"]
        .as_array()
        .unwrap();
    assert!(
        checks
            .iter()
            .any(|c| c["id"] == "bound_focus_context" && c["conclusion"] == "pass")
    );
    assert!(
        checks
            .iter()
            .any(|c| c["id"] == "current_project_context" && c["conclusion"] == "violated")
    );
}

#[test]
fn rejected_protocol_two_restores_real_baseline_and_keeps_rejection_history() {
    let mut f = Fixture::new("rejected-v2-recovery", "required_semantic_review");
    protocol_two(&mut f);
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    learning::decide(
        &f.root,
        &p,
        "reject",
        "no measured gain; keep valid reviewed baseline",
    )
    .unwrap();
    let rejection = fs::read(
        f.temp
            .path()
            .join("var/state/knowledge/experience-events/rejected-v2-recovery/0002.json"),
    )
    .unwrap();
    assert!(
        learning::decide(
            &f.root,
            &p,
            "accept-trial",
            "cannot change a completed rejection"
        )
        .is_err()
    );
    learning::recover(
        &f.root,
        &p,
        "explicit no-gain recovery",
        Path::new("var/restored"),
    )
    .unwrap();
    assert_eq!(
        fs::read(f.temp.path().join("var/restored/baseline-plan.json")).unwrap(),
        fs::read(
            f.temp
                .path()
                .join("var/candidates/candidate/baseline-plan.json")
        )
        .unwrap()
    );
    assert_eq!(
        fs::read(
            f.temp
                .path()
                .join("var/state/knowledge/experience-events/rejected-v2-recovery/0002.json")
        )
        .unwrap(),
        rejection
    );
    let result = learning::apply(&f.root, &p, &f.case, Path::new("var/after-restore")).unwrap();
    assert_eq!(result["trial"]["selected_plan"], "baseline-plan.json");
    assert_eq!(
        result["trial"]["result"]["receipt"]["conclusion"],
        "unknown"
    );
    assert_eq!(result["event"]["event"]["state"], "restored");
}

#[test]
fn frozen_candidate_refuses_drift_and_keeps_evaluator_history() {
    let f = Fixture::new("freeze-a", "required_semantic_review");
    let p = f.freeze();
    let before = fs::read(f.temp.path().join("var/candidates/candidate/freeze.json")).unwrap();
    fs::write(
        f.temp
            .path()
            .join("var/candidates/candidate/evaluation-original.json"),
        "{}",
    )
    .unwrap();
    assert!(
        learning::assess(&f.root, &p, Path::new("var/assessment.json"))
            .unwrap_err()
            .to_string()
            .contains("experience_frozen_bytes_drift")
    );
    assert_eq!(
        fs::read(f.temp.path().join("var/candidates/candidate/freeze.json")).unwrap(),
        before
    );
    assert_eq!(
        f.read("var/state/knowledge/experience-events/freeze-a/0000.json")["state"],
        "frozen"
    );
    let g = Fixture::new("freeze-b", "required_semantic_review");
    let p = g.freeze();
    fs::write(
        g.temp.path().join("crates/oe-kernel/src/learning.rs"),
        "changed evaluator",
    )
    .unwrap();
    assert!(
        learning::assess(&g.root, &p, Path::new("var/assessment.json"))
            .unwrap_err()
            .to_string()
            .contains("new_baseline_required")
    );
    let h = Fixture::new("freeze-executable", "required_semantic_review");
    let p = h.freeze();
    fs::write(
        h.temp
            .path()
            .join("var/candidates/candidate/evaluator-executable.bin"),
        b"different compiled evaluator",
    )
    .unwrap();
    assert!(
        learning::assess(&h.root, &p, Path::new("var/assessment.json"))
            .unwrap_err()
            .to_string()
            .contains("experience_frozen_evaluator_executable_drift")
    );
}

#[test]
fn no_gain_and_unsafe_answers_cannot_be_selected() {
    let f = Fixture::new("no-gain", "no_semantic_review");
    let p = f.freeze();
    let record = learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    assert_eq!(record["assessment"]["verdict"], "no_delta");
    assert!(
        learning::decide(
            &f.root,
            &p,
            "accept-trial",
            "novelty alone is not improvement"
        )
        .is_err()
    );
    let rejection =
        learning::decide(&f.root, &p, "reject", "retain the actual baseline; no gain").unwrap();
    assert_eq!(
        rejection["event"]["body"]["selected_plan"],
        "baseline-plan.json"
    );
    let g = Fixture::new("leak-a", "required_semantic_review");
    let path = g.input["candidate_plan"]["path"].as_str().unwrap();
    let raw=serde_json::to_vec(&json!({"mode":"required_semantic_review","answers":{"case-a":"pass"},"unit_override":"mm","claimed_score":1000000})).unwrap();
    fs::write(g.temp.path().join(path), &raw).unwrap();
    let mut input = g.input.clone();
    input["candidate_plan"]["sha256"] = json!(sha256(raw));
    learning::freeze(&g.root, &input, Path::new("var/candidates/unsafe")).unwrap();
    let record = learning::assess(
        &g.root,
        Path::new("var/candidates/unsafe"),
        Path::new("var/assessment.json"),
    )
    .unwrap();
    assert_eq!(record["assessment"]["verdict"], "rejected");
    assert!(
        learning::decide(
            &g.root,
            Path::new("var/candidates/unsafe"),
            "accept-trial",
            "a high score cannot override units or answers"
        )
        .is_err()
    );
    let private = g.read("var/assessment.json");
    assert_eq!(private["engineering_cost"]["status"], "unknown");
    assert_eq!(
        private["candidate"].as_array().unwrap().len(),
        1,
        "critic failure keeps the denominator and unknown slot"
    );
}

#[test]
fn trial_rejection_and_recovery_preserve_baseline_bytes() {
    let f = Fixture::new("trial-a", "required_semantic_review");
    let p = f.freeze();
    let before = fs::read(
        f.temp
            .path()
            .join("var/candidates/candidate/baseline-plan.json"),
    )
    .unwrap();
    let assessed = learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    assert_eq!(assessed["assessment"]["verdict"], "admissible_trial");
    let accepted = learning::decide(
        &f.root,
        &p,
        "accept-trial",
        "bounded diagnostic gain; project remains unknown",
    )
    .unwrap();
    assert_eq!(accepted["event"]["state"], "trial_active");
    let trial = learning::apply(
        &f.root,
        &p,
        &f.case,
        Path::new("var/projects/second-task/trial"),
    )
    .unwrap();
    assert_eq!(trial["trial"]["result"]["receipt"]["conclusion"], "unknown");
    assert_eq!(
        trial["trial"]["result"]["required_condition_diagnosis"],
        "matched"
    );
    learning::recover(
        &f.root,
        &p,
        "independent transfer remains unverified",
        Path::new("var/projects/second-task/recovered"),
    )
    .unwrap();
    assert_eq!(
        fs::read(
            f.temp
                .path()
                .join("var/projects/second-task/recovered/baseline-plan.json")
        )
        .unwrap(),
        before
    );
    let replay = learning::apply(
        &f.root,
        &p,
        &f.case,
        Path::new("var/projects/second-task/baseline-replay"),
    )
    .unwrap();
    assert_eq!(replay["trial"]["selected_plan"], "baseline-plan.json");
    assert_eq!(replay["trial"]["result"]["receipt"]["execution"], "not_run");
    assert_eq!(
        f.read("var/state/knowledge/experience-events/trial-a/0000.json")["state"],
        "frozen"
    );
    assert_eq!(
        f.read("var/state/knowledge/experience-events/trial-a/0005.json")["state"],
        "restored"
    );
}

#[test]
fn one_failed_case_cannot_be_compensated_by_other_diagnoses() {
    let f = Fixture::new("hard-a", "required_semantic_review");
    let mut contract = f.read("var/inputs/evaluation.json");
    let mut wrong = f.case.clone();
    wrong["case_id"] = json!("case-b");
    let mut binding = f.read("var/inputs/binding.json");
    binding["object_revision"] = json!("different revision");
    let raw = serde_json::to_vec(&binding).unwrap();
    fs::write(f.temp.path().join("var/inputs/wrong-binding.json"), &raw).unwrap();
    wrong["binding"] = json!({"path":"var/inputs/wrong-binding.json","sha256":sha256(raw)});
    contract["cases"].as_array_mut().unwrap().push(wrong);
    let raw = serde_json::to_vec(&contract).unwrap();
    fs::write(f.temp.path().join("var/inputs/evaluation.json"), &raw).unwrap();
    let mut input = f.input.clone();
    input["evaluation"]["sha256"] = json!(sha256(raw));
    learning::freeze(&f.root, &input, Path::new("var/candidates/unsafe")).unwrap();
    let assessed = learning::assess(
        &f.root,
        Path::new("var/candidates/unsafe"),
        Path::new("var/assessment.json"),
    )
    .unwrap();
    assert_eq!(assessed["assessment"]["verdict"], "inconclusive");
    assert_eq!(assessed["assessment"]["cost_status"], "unknown");
    assert!(
        learning::decide(
            &f.root,
            Path::new("var/candidates/unsafe"),
            "accept-trial",
            "aggregate gain cannot cancel identity mismatch"
        )
        .is_err()
    );
}

#[test]
fn graph_scope_mismatch_and_candidate_query_drift_are_refused() {
    let f = Fixture::new("scope-a", "required_semantic_review");
    let mut binding = f.read("var/inputs/binding.json");
    binding["condition_scope"] = json!("different_scope");
    let raw = serde_json::to_vec(&binding).unwrap();
    fs::write(f.temp.path().join("var/inputs/binding.json"), &raw).unwrap();
    let binding_sha = sha256(raw);
    let mut project = f.read("var/inputs/project.json");
    project["conditions"] = json!({"different_scope":"declared project scope differs from RDF"});
    let raw = serde_json::to_vec(&project).unwrap();
    fs::write(f.temp.path().join("var/inputs/project.json"), &raw).unwrap();
    let mut contract = f.read("var/inputs/evaluation.json");
    contract["cases"][0]["binding"]["sha256"] = json!(binding_sha);
    contract["cases"][0]["project"]["sha256"] = json!(sha256(raw));
    let raw = serde_json::to_vec(&contract).unwrap();
    fs::write(f.temp.path().join("var/inputs/evaluation.json"), &raw).unwrap();
    let mut input = f.input.clone();
    input["evaluation"]["sha256"] = json!(sha256(raw));
    learning::freeze(&f.root, &input, Path::new("var/candidates/scope")).unwrap();
    let assessed = learning::assess(
        &f.root,
        Path::new("var/candidates/scope"),
        Path::new("var/assessment.json"),
    )
    .unwrap();
    assert_eq!(assessed["assessment"]["verdict"], "inconclusive");
    assert!(
        f.read("var/assessment.json")["candidate"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("experience_focus_scope_mismatch")
    );
    fs::write(
        f.temp.path().join("var/candidates/scope/experience.json"),
        b"{}",
    )
    .unwrap();
    assert!(
        learning::list(&f.root, "")
            .unwrap_err()
            .to_string()
            .contains("experience_frozen_bytes_drift")
    );
}

#[test]
fn rejected_hypothesis_accepts_new_basis_from_raw_ledger_event() {
    let f = Fixture::new("retry-root", "no_semantic_review");
    let p = f.freeze();
    learning::assess(&f.root, &p, Path::new("var/assessment.json")).unwrap();
    learning::decide(&f.root, &p, "reject", "no gain; evidence absent").unwrap();
    let mut input = f.input.clone();
    input["candidate"]["candidate_id"] = json!("retry-next");
    assert!(
        learning::freeze(&f.root, &input, Path::new("var/candidates/denied"))
            .unwrap_err()
            .to_string()
            .contains("needs_new_basis")
    );
    let prior = "var/state/knowledge/experience-events/retry-root/0002.json";
    input["prior_rejection"] =
        json!({"path":prior,"sha256":sha256(fs::read(f.temp.path().join(prior)).unwrap())});
    let basis = "var/inputs/new-basis.txt";
    let bytes = b"distinct source evidence, not a novelty or score assertion";
    fs::write(f.temp.path().join(basis), bytes).unwrap();
    input["new_basis"] = json!([{"path":basis,"sha256":sha256(bytes)}]);
    let result = learning::freeze(&f.root, &input, Path::new("var/candidates/retry")).unwrap();
    assert_eq!(result["event"]["state"], "frozen");
    assert_eq!(
        f.read(prior)["state"],
        "rejected",
        "the old rejection is retained"
    );
}

#[test]
fn evaluation_three_binds_original_source_separately_and_retains_two_oracle() {
    let mut f = Fixture::new("origin-linked-eval2", "required_semantic_review");
    protocol_two(&mut f);
    let put = |name: &str, value: &Value| {
        let path = format!("var/inputs/{name}");
        let bytes = serde_json::to_vec(value).unwrap();
        fs::write(f.temp.path().join(&path), &bytes).unwrap();
        json!({"path":path,"sha256":sha256(bytes)})
    };
    let origin = f.input["candidate"]["object"]["source_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut p: oe_kernel::project::Project =
        serde_json::from_value(f.read("var/inputs/project-current.json")).unwrap();
    p.evidence.insert("original-source".into(), origin.clone());
    let mut binding: oe_kernel::Binding =
        serde_json::from_value(f.read("var/inputs/binding-current.json")).unwrap();
    binding.source_sha256 = origin.clone();
    let object = oe_contracts::ObjectIdentity {
        object_id: binding.object_id.clone(),
        revision: binding.object_revision.clone(),
        condition_scope: binding.condition_scope.clone(),
        source_sha256: origin.clone(),
    };
    binding.project_context = Some(oe_kernel::project::context(&p, &object).unwrap());
    let mut evidence = fs::read(
        f.temp
            .path()
            .join(f.case["evidence"]["path"].as_str().unwrap()),
    )
    .unwrap();
    evidence.extend_from_slice(format!("\n<{}> <urn:semantica:engineering-evidence:fieldEvidence> [ <urn:semantica:engineering-evidence:sourceSha256> \"{}\" ] .\n",object.object_id,origin).as_bytes());
    assert_ne!(
        sha256(&evidence),
        origin,
        "derived RDF has its own identity"
    );
    fs::write(
        f.temp.path().join("var/inputs/evidence-origin.ttl"),
        &evidence,
    )
    .unwrap();
    f.case["evidence"] =
        json!({"path":"var/inputs/evidence-origin.ttl","sha256":sha256(&evidence)});
    f.case["project"] = put("project-origin.json", &serde_json::to_value(&p).unwrap());
    f.case["binding"] = put(
        "binding-origin-two.json",
        &serde_json::to_value(&binding).unwrap(),
    );
    let mut evaluation = f.read("var/inputs/evaluation-current.json");
    evaluation["cases"] = json!([f.case]);
    f.input["evaluation"] = put("evaluation-origin-two.json", &evaluation);
    let old = f.freeze();
    let old_result =
        learning::assess(&f.root, &old, Path::new("var/origin-two-assessment.json")).unwrap();
    assert_eq!(old_result["assessment"]["verdict"], "inconclusive");
    let before = f.read("var/origin-two-assessment.json");
    assert_eq!(
        before["baseline"][0]["reason"],
        "experience_project_identity_mismatch"
    );
    // /3 is a new frozen evaluation baseline. /2 input and receipt stay intact.
    binding.required_checks.extend([
        "current_project_context".into(),
        "current_condition_model".into(),
    ]);
    f.case["binding"] = put(
        "binding-origin-three.json",
        &serde_json::to_value(&binding).unwrap(),
    );
    evaluation["schema"] = json!("oev2.experience-evaluation/3");
    evaluation["cases"] = json!([f.case]);
    f.input["evaluation"] = put("evaluation-origin-three.json", &evaluation);
    f.input["candidate"]["candidate_id"] = json!("origin-linked-eval3");
    let dir = Path::new("var/candidates/origin-three");
    learning::freeze(&f.root, &f.input, dir).unwrap();
    let current =
        learning::assess(&f.root, dir, Path::new("var/origin-three-assessment.json")).unwrap();
    assert_eq!(current["assessment"]["verdict"], "no_delta");
    let after = f.read("var/origin-three-assessment.json");
    for side in ["baseline", "candidate"] {
        assert_eq!(after[side][0]["receipt"]["execution"], "completed");
        assert_eq!(after[side][0]["required_condition_diagnosis"], "matched");
        assert_eq!(after[side][0]["receipt"]["conclusion"], "unknown");
        for id in [
            "bound_focus_context",
            "current_project_context",
            "current_condition_model",
        ] {
            assert!(
                after[side][0]["receipt"]["checks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["id"] == id && c["conclusion"] == "pass")
            );
        }
    }
    assert_eq!(f.read("var/origin-two-assessment.json"), before);
    learning::decide(
        &f.root,
        dir,
        "reject",
        "no method gain or physical evidence",
    )
    .unwrap();
    learning::recover(
        &f.root,
        dir,
        "restore exact required-review baseline",
        Path::new("var/origin-three-restored.json"),
    )
    .unwrap();
    learning::apply(&f.root, dir, &f.case, Path::new("var/origin-three-apply")).unwrap();
}
