//! Actual Q40 owner acceptance. Explicitly ignored until fresh, independently
//! frozen Q40 binaries/oracle/assurance exist. No old six-case fixture, synthetic
//! process or caller-authored assurance is generated here. Missing env is fatal
//! when the ignored tests are explicitly selected.
use oe_contracts::{
    ArtifactReference, CodeArtifactEligibilityRequest, CodeComparison, CodeComparisonRequest,
    CodeExperiment, CodeFullScopeQualificationContext, CodeReplayCase, CodeScopeAssociationIndex2,
    CodeScopeAssociationPort, CodeScopeWitnessReport2, CodeScopeWitnessRequest, CodeSourceArtifact,
};
use oe_core::json::{canonical_json, strict_json};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use oe_engineering::{artifacts, registry};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const FILES: [&str; 6] = [
    "source.json",
    "comparison.json",
    "experiment.json",
    "context.json",
    "full-scope-request.json",
    "protocol19.json",
];
const ORACLE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/q40-replay-cases.json"
));
const CATALOG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/q40-catalog.json"
));
const ORACLE_SHA: &str = "fb556d112c0302389240f83b536c82aa7e5079a36be7c2cec2645a9d8a5b51ce";
const CATALOG_SHA: &str = "31a5ac5f7b95b976c75d809293f613fbd0fb946d0a330fd48d4064550981bbc4";
const CONDITIONS_SHA: &str = "5d96bb44c4da300d2e677de54331535b3082d0e08c89edc923f714325f290f59";
const SCOPE_SHA: &str = "b1470d9980755811c815be24cdc041740de5f55e28232c327f0845d160455226";
const LOCK_SHA: &str = "d874aecfa20e3753ea85725c0a9b24e2f4795e4b54df819b52a31ac56b93e1dc";
const PROTOCOL_SHA: &str = "12cf2e3bcd171ac1000373257553dc0202d9bc42a7d8ff46f910338158ea2809";
fn raw(path: &Path) -> Vec<u8> {
    read_regular_bytes(path, 16 * 1024 * 1024).unwrap()
}
fn json_file(path: &Path) -> Value {
    strict_json(&raw(path)).unwrap()
}
fn digest(value: &Value) -> String {
    sha256(canonical_json(value).unwrap())
}
fn private_mode(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn replace_private(path: &Path, value: &Value) {
    private_mode(path);
    fs::write(path, canonical_json(value).unwrap()).unwrap();
}
fn catalog() -> Value {
    assert_eq!(sha256(CATALOG), CATALOG_SHA);
    assert_eq!(sha256(ORACLE), ORACLE_SHA);
    strict_json(CATALOG).unwrap()
}
struct Fixture {
    root: SkillRoot,
    original: PathBuf,
    original_pins: BTreeMap<String, String>,
    request: CodeArtifactEligibilityRequest,
    context: CodeFullScopeQualificationContext,
    experiment: CodeExperiment,
    source: CodeSourceArtifact,
    assurance: CodeComparison,
}
fn bind_policy(root: &SkillRoot, request: &CodeArtifactEligibilityRequest) {
    let reference =
        artifacts::reference(root, &root.path().join("var/input/context.json")).unwrap();
    let mut bindings = serde_json::Map::new();
    bindings.insert(request.binding_id.clone(), json!({"kind":"code_artifact", "scope_id":request.scope_id,
        "actor_scopes":{"maintainer":["register","activate","restore"]}, "code_qualification":reference}));
    let path = root.path().join(registry::POLICY);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        canonical_json(
            &json!({"schema":"oev2.controlled-version-authority/2","bindings":bindings}),
        )
        .unwrap(),
    )
    .unwrap();
    private_mode(&path);
}
fn fixture() -> Fixture {
    let original = PathBuf::from(std::env::var_os("OE_Q40_QUALIFICATION_FIXTURE")
        .expect("OE_Q40_QUALIFICATION_FIXTURE must name a newly frozen real Q40 fixture; old six-case fixtures and silent skips are forbidden"));
    let original = original
        .canonicalize()
        .expect("Q40 fixture root must exist");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    fs::create_dir_all(workspace.join("var/builds")).unwrap();
    let temp = tempfile::Builder::new()
        .prefix("q40-owner-actual-")
        .tempdir_in(workspace.join("var/builds"))
        .unwrap();
    fs::write(
        temp.path().join("SKILL.md"),
        "Private Q40 owner acceptance; no whole-library or engineering qualification\n",
    )
    .unwrap();
    let persisted = temp.keep(); // Keep raw evidence even when an assertion fails.
    eprintln!("retained Q40 owner fixture: {}", persisted.display());
    let root = SkillRoot::new(&persisted).unwrap();
    assert_ne!(root.path(), original.as_path());
    let input = root.path().join("var/input");
    fs::create_dir_all(&input).unwrap();
    let mut original_pins = BTreeMap::new();
    for name in FILES {
        let bytes = raw(&original.join("var/input").join(name));
        let pin = sha256(&bytes);
        original_pins.insert(name.to_owned(), pin.clone());
        fs::write(input.join(name), &bytes).unwrap();
        private_mode(&input.join(name));
        assert_eq!(
            sha256(raw(&input.join(name))),
            pin,
            "copy must preserve original {name} bytes"
        );
    }
    write_new(&root.path().join("var/fixture-source-pins.json"), &json!({"original_root":original,"files":original_pins,"source_kind":"actual_prepared_q40_fixture"})).unwrap();
    let context: CodeFullScopeQualificationContext =
        serde_json::from_value(json_file(&input.join("context.json"))).unwrap();
    context.validate().unwrap();
    assert_eq!(context.schema, "oev2.code-qualification-context/2");
    let q = &context.qualification;
    assert_eq!(q.schema, "oev2.code-qualification-context/1");
    assert_eq!(q.purpose, "full_code_release");
    assert_eq!(q.source.path, "var/input/source.json");
    assert_eq!(q.assurance.path, "var/input/comparison.json");
    assert_eq!(q.experiment.path, "var/input/experiment.json");
    assert_eq!(q.required_protocol.path, "var/input/protocol19.json");
    assert_eq!(
        context.full_scope_request.path,
        "var/input/full-scope-request.json"
    );
    let request = CodeArtifactEligibilityRequest {
        schema: "oev2.code-artifact-eligibility-request/1".into(),
        binding_id: q.binding_id.clone(),
        revision: q.revision.clone(),
        scope_id: q.scope_id.clone(),
        source: q.source.clone(),
        assurance: q.assurance.clone(),
    };
    let source: CodeSourceArtifact =
        serde_json::from_value(artifacts::json(&root, &q.source).unwrap()).unwrap();
    let assurance: CodeComparison =
        serde_json::from_value(artifacts::json(&root, &q.assurance).unwrap()).unwrap();
    let experiment: CodeExperiment =
        serde_json::from_value(artifacts::json(&root, &q.experiment).unwrap()).unwrap();
    let scope: CodeScopeWitnessRequest =
        serde_json::from_value(artifacts::json(&root, &context.full_scope_request).unwrap())
            .unwrap();
    // Both selected source artifacts use actual composed generation. The fixed
    // driver's own composition capture is deliberately a different snapshot.
    for (side, artifact) in [
        ("baseline", &experiment.baseline),
        ("candidate", &experiment.candidate),
    ] {
        assert_eq!(
            artifact.generation["receipt"]["schema"], "oev2.generation/2",
            "{side} must retain its original generation schema"
        );
        assert_eq!(
            artifact.generation["check"]["generation"], artifact.generation["receipt"],
            "{side} actual drift check must name this receipt"
        );
        assert_eq!(
            artifact.generation["receipt"],
            json_file(&Path::new(&artifact.source_root).join("generated/generation.json")),
            "{side} source receipt must be re-read from its own root"
        );
        assert_eq!(
            artifact.generation["receipt"]["composition_binding"]["status"],
            "matched"
        );
    }

    assert_eq!(
        scope.schema, "oev2.code-scope-witness-request/2",
        "old /1 fixture is not Q40"
    );
    assert_eq!(scope.experiment_sha256, q.experiment_sha256);
    assert_eq!(
        q.dispatch_budget.timeout_ms, 600000,
        "fixture freezes the unchanged owner deadline"
    );
    assert_eq!(
        scope.dispatch_budget.timeout_ms, 570000,
        "nested evaluator dispatch reserves the existing30 seconds"
    );
    assert_eq!(q.dispatch_budget.output_bytes, 4194304);
    assert_eq!(scope.dispatch_budget.output_bytes, 4194304);
    assert!(
        scope
            .dispatch_budget
            .timeout_ms
            .checked_add(30000)
            .is_some_and(|v| v <= q.dispatch_budget.timeout_ms)
    );
    assert_eq!(
        digest(&serde_json::to_value(&experiment).unwrap()),
        q.experiment_sha256
    );
    assert_eq!(source, experiment.candidate);
    assert_eq!(q.driver, experiment.driver);
    assert_eq!(q.evaluator, experiment.evaluator);
    assert_eq!(experiment.cases.len(), 40);
    assert_eq!(experiment.oracle_sha256, ORACLE_SHA);
    assert_eq!(experiment.conditions_sha256, CONDITIONS_SHA);
    assert_eq!(experiment.scope_sha256, SCOPE_SHA);
    assert_eq!(experiment.protocol_expectations_sha256, PROTOCOL_SHA);
    assert_eq!(q.required_protocol.sha256, PROTOCOL_SHA);
    assert_eq!(
        canonical_json(&serde_json::to_value(&experiment.cases).unwrap()).unwrap(),
        canonical_json(&strict_json(ORACLE).unwrap()).unwrap()
    );
    assert_eq!(
        experiment.case_set_sha256,
        digest(&serde_json::to_value(&experiment.cases).unwrap())
    );
    let protocol = artifacts::json(&root, &q.required_protocol).unwrap();
    let protocol_ids: Vec<_> = protocol["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["case_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(protocol_ids.len(), 19);
    assert_eq!(protocol_ids, experiment.protocol_case_ids);
    assert_eq!(scope.items.len(), 13494);
    assert_eq!(assurance.fixed_denominator, 40);
    assert_eq!(assurance.slots.len(), 40);
    assert_eq!(assurance.oracle_sha256, ORACLE_SHA);
    assert_eq!(assurance.engineering_adoption, "unknown");
    assert_eq!(assurance.version_selection, "not_connected");
    let driver_root = Path::new(&experiment.driver.source_root)
        .canonicalize()
        .unwrap();
    let candidate_root = Path::new(&source.source_root).canonicalize().unwrap();
    let baseline_root = Path::new(&experiment.baseline.source_root)
        .canonicalize()
        .unwrap();
    assert_ne!(driver_root, candidate_root);
    assert_ne!(driver_root, baseline_root);
    assert_ne!(
        source.manifest_sha256, experiment.driver.source_manifest_sha256,
        "positive must discriminate candidate mappings from independent driver source, not just different path names"
    );
    assert_eq!(
        experiment.driver.source_root,
        experiment.evaluator.source_root
    );
    let external_oracle = Path::new(&experiment.oracle_path).canonicalize().unwrap();
    assert!(
        !external_oracle.starts_with(candidate_root) && !external_oracle.starts_with(baseline_root)
    );
    assert_eq!(sha256(raw(&external_oracle)), ORACLE_SHA);
    bind_policy(&root, &request);
    let f = Fixture {
        root,
        original,
        original_pins,
        request,
        context,
        experiment,
        source,
        assurance,
    };
    unchanged_originals(&f);
    no_dispatch(&f.root);
    no_adoption(&f);
    f
}
fn unchanged_originals(f: &Fixture) {
    for (name, pin) in &f.original_pins {
        assert_eq!(
            sha256(raw(&f.original.join("var/input").join(name))),
            *pin,
            "original fixture changed: {name}"
        );
    }
}
fn no_dispatch(root: &SkillRoot) {
    let p = root.path().join("var/code-qualification");
    assert!(
        !p.exists() || fs::read_dir(p).unwrap().next().is_none(),
        "negative must fail before owner dispatch"
    );
}
fn no_adoption(f: &Fixture) {
    assert!(!f.root.path().join("var/state/controlled-versions").exists());
    assert!(
        !f.root
            .path()
            .join("var/state/knowledge/experience-events")
            .exists()
    );
    assert!(registry::active(&f.root, &f.request.binding_id).is_err());
}
fn original_ids(candidate: &Path) -> BTreeSet<String> {
    let bytes = raw(&candidate.join("runtime/semantica-scope.json"));
    assert_eq!(sha256(&bytes), SCOPE_SHA);
    let scope = strict_json(&bytes).unwrap();
    let mut expected = BTreeSet::new();
    for (section, kind) in [
        ("tracked_files", "tracked_file"),
        ("empty_files", "original_empty_file"),
        ("package_python", "package_source"),
        ("outside_python", "outside_source"),
        ("frontend_code", "frontend_source"),
        ("test_files", "original_test_file"),
        ("python_test_sources", "original_python_test_source"),
    ] {
        for row in scope[section].as_array().unwrap() {
            assert!(expected.insert(format!("{}|{kind}||0", row["path"].as_str().unwrap())));
        }
    }
    for section in [
        "public_symbols",
        "outside_symbols",
        "python_exports",
        "frontend_exports",
        "routes",
        "original_test_symbols",
        "placeholder_candidates",
        "empty_modules",
        "console_entrypoints",
    ] {
        for row in scope[section].as_array().unwrap() {
            assert!(expected.insert(row["id"].as_str().unwrap().into()));
        }
    }
    assert_eq!(expected.len(), 13494);
    expected
}
fn mapping_expectations(source: &CodeSourceArtifact, ids: &BTreeSet<String>) -> (String, Value) {
    let candidate = Path::new(&source.source_root);
    let bytes = raw(&candidate.join("runtime/semantica-migration-mappings.json"));
    let mapping_sha = sha256(&bytes);
    assert_eq!(
        source.manifest["files"]["runtime/semantica-migration-mappings.json"]["sha256"],
        mapping_sha
    );
    let mapping = strict_json(&bytes).unwrap();
    let mut expected = Vec::new();
    for claim in mapping["claims"].as_array().unwrap() {
        assert_eq!(claim["status"], "declared");
        let path = claim["rust_path"].as_str().unwrap();
        let digest = source.manifest["files"]
            .get(path)
            .map(|v| v["sha256"].as_str().unwrap().to_owned());
        if let Some(pin) = &digest {
            assert_eq!(sha256(raw(&candidate.join(path))), *pin);
        }
        for id in claim["scope_ids"].as_array().unwrap() {
            assert!(ids.contains(id.as_str().unwrap()));
            expected.push(json!({"scope_id":id,"module":claim["module"],"rust_path":path,"source_sha256":digest,
                "resolution":if digest.is_some(){"declared_source_matched"}else{"declared_source_missing"}}));
        }
    }
    assert!(
        !expected.is_empty(),
        "candidate mapping comparison cannot be vacuous"
    );
    expected.sort_by(|a, b| {
        (
            a["scope_id"].as_str(),
            a["module"].as_str(),
            a["rust_path"].as_str(),
        )
            .cmp(&(
                b["scope_id"].as_str(),
                b["module"].as_str(),
                b["rust_path"].as_str(),
            ))
    });
    (mapping_sha, Value::Array(expected))
}
fn process(folder: &Path, executable_sha: &str, cwd: &str, arguments: &Value) -> (Value, Vec<u8>) {
    let record = json_file(&folder.join("process.json"));
    assert_eq!(record["status"], "completed");
    assert_eq!(record["exit_code"], 0);
    assert_eq!(record["capture_complete"], true);
    assert_eq!(record["executable"]["identity"]["sha256"], executable_sha);
    assert_eq!(record["executable"]["unchanged"], true);
    assert_eq!(record["cwd"], cwd);
    assert_eq!(&record["arguments"], arguments);
    let mut stdout = Vec::new();
    for stream in ["stdout", "stderr"] {
        let bytes = raw(&folder.join(stream));
        assert_eq!(record[stream]["sha256"], sha256(&bytes));
        assert_eq!(record[stream]["bytes"], bytes.len());
        if stream == "stdout" {
            stdout = bytes;
        }
    }
    (record, stdout)
}
fn expected_invocation(case: &CodeReplayCase) -> Value {
    json!({"schema":"oev2.code-replay-invocation/1","case_id":case.case_id,"capability":case.capability,
        "profile":case.profile,"input":case.input,"input_sha256":case.input_sha256,"conditions_sha256":case.conditions_sha256,
        "budget":case.budget,"seed":case.seed,"run_id":0})
}
fn check_all80(f: &Fixture, witness: &CodeScopeWitnessReport2) {
    let driver = SkillRoot::new(&f.experiment.driver.source_root).unwrap();
    let reference = &witness.observation.witness.process;
    let driver_process = artifacts::json(&driver, reference).unwrap();
    let arguments = &driver_process["arguments"];
    assert_eq!(arguments.as_array().unwrap().len(), 5);
    assert_eq!(arguments[0], "run");
    assert_eq!(arguments[1], f.experiment.driver.source_root);
    assert_eq!(arguments[3], f.context.qualification.experiment_sha256);
    assert_eq!(
        json_file(Path::new(arguments[2].as_str().unwrap())),
        serde_json::to_value(&f.experiment).unwrap()
    );
    let process_path = driver.private_path(Path::new(&reference.path)).unwrap();
    let (actual_process, output) = process(
        process_path.parent().unwrap(),
        &f.experiment.driver.sha256,
        &f.experiment.driver.source_root,
        arguments,
    );
    assert_eq!(driver_process, actual_process);
    let comparison = witness.observation.witness.comparison.as_ref().unwrap();
    assert_eq!(
        strict_json(&output).unwrap(),
        serde_json::to_value(comparison).unwrap()
    );
    let replay = driver
        .private_path(Path::new(arguments[4].as_str().unwrap()))
        .unwrap();
    assert_eq!(
        json_file(&replay.join("comparison.json")),
        serde_json::to_value(comparison).unwrap()
    );
    let runs: CodeComparisonRequest =
        serde_json::from_value(json_file(&replay.join("comparison-input.json"))).unwrap();
    assert_eq!(runs.experiment, f.experiment);
    assert_eq!(runs.observations.len(), 80);
    let name = replay.file_name().unwrap().to_str().unwrap();
    let mut seen = BTreeSet::new();
    let mut candidate_success = 0;
    let mut candidate_rejected = 0;
    for run in &runs.observations {
        assert_eq!(run.run_id, 0);
        assert!(["baseline", "candidate"].contains(&run.side.as_str()));
        assert!(seen.insert((run.side.clone(), run.case_id.clone(), run.run_id)));
        let case = f
            .experiment
            .cases
            .iter()
            .find(|c| c.case_id == run.case_id)
            .unwrap();
        let artifact = if run.side == "candidate" {
            &f.experiment.candidate
        } else {
            &f.experiment.baseline
        };
        let side_root = SkillRoot::new(&artifact.source_root).unwrap();
        let folder = side_root
            .private_path(Path::new(&format!(
                "var/code-replays/{name}/{}/{}/0",
                run.side, run.case_id
            )))
            .unwrap();
        let invocation_path = folder.join("invocation.json");
        let invocation = raw(&invocation_path);
        assert_eq!(sha256(&invocation), run.invocation_sha256);
        assert_eq!(strict_json(&invocation).unwrap(), expected_invocation(case));
        assert_eq!(run.runner_sha256, artifact.runner.sha256);
        assert_eq!(run.artifact_manifest_sha256, artifact.manifest_sha256);
        let (record, stdout) = process(
            &folder.join("process"),
            &artifact.runner.sha256,
            &artifact.source_root,
            &json!(["run", invocation_path]),
        );
        assert_eq!(record, run.process);
        assert_eq!(run.execution, "completed");
        assert_eq!(run.stdout_bytes, Some(stdout.len() as u64));
        assert_eq!(
            run.stderr_bytes,
            Some(raw(&folder.join("process/stderr")).len() as u64)
        );
        assert!(run.elapsed_micros.is_some());
        let inner = run
            .observed
            .as_ref()
            .expect("all80 actual runner observations are required");
        assert_eq!(
            strict_json(&stdout).unwrap(),
            serde_json::to_value(inner).unwrap()
        );
        assert_eq!(inner.case_id, case.case_id);
        assert_eq!(inner.input_sha256, case.input_sha256);
        assert_eq!(inner.conditions_sha256, case.conditions_sha256);
        assert_eq!(inner.compiled_manifest_sha256, artifact.manifest_sha256);
        if run.side == "candidate" {
            assert_eq!(inner.execution, "completed");
            assert!(inner.error.is_none());
            assert!(case.checks.iter().all(|c| inner.coverage.contains(c)));
            let mut actual = inner.result.clone();
            if case.case_id == "max_iterations_bool_rejected" {
                assert!(
                    actual["failure"]["message"]
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty())
                );
                actual["failure"]["message"] = case.expected["failure"]["message"].clone();
            }
            assert_eq!(
                canonical_json(&actual).unwrap(),
                canonical_json(&case.expected).unwrap()
            );
            if actual["outcome"] == "success" {
                candidate_success += 1;
            } else {
                assert_eq!(actual["outcome"], "rejected");
                candidate_rejected += 1;
            }
        }
    }
    let expected: BTreeSet<_> = f
        .experiment
        .cases
        .iter()
        .flat_map(|c| {
            ["baseline", "candidate"].map(|side| (side.to_owned(), c.case_id.clone(), 0u64))
        })
        .collect();
    assert_eq!(seen, expected);
    assert_eq!((candidate_success, candidate_rejected), (16, 24));
    let (eval_process, eval_stdout) = process(
        &replay.join("fixed-evaluator"),
        &f.experiment.evaluator.sha256,
        &f.experiment.evaluator.source_root,
        &json!(["compare", replay.join("comparison-input.json")]),
    );
    assert_eq!(eval_process["status"], "completed");
    assert_eq!(
        strict_json(&eval_stdout).unwrap(),
        serde_json::to_value(comparison).unwrap()
    );
}

#[test]
#[ignore = "requires OE_Q40_QUALIFICATION_FIXTURE with fresh independent Q40 artifacts; actually executes all80 slots once"]
fn actual_q40_owner_association_preserves80_runs_full_scope_and_catalog_roles() {
    let f = fixture();
    let candidate = Path::new(&f.source.source_root);
    let expected_ids = original_ids(candidate);
    let (mapping_sha, mappings) = mapping_expectations(&f.source, &expected_ids);
    let expected_catalog = catalog();
    let result = registry::CodeArtifactEligibilityExecutor { root: &f.root }
        .associate(&f.request)
        .unwrap();
    assert_eq!(result.execution, "completed");
    assert_eq!(result.qualification.execution, "completed");
    assert_eq!(result.qualification.eligibility, "unknown");
    let folders: Vec<_> = fs::read_dir(f.root.path().join("var/code-qualification"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(
        folders.len(),
        1,
        "one association must dispatch exactly once"
    );
    let folder = &folders[0];
    assert_eq!(
        json_file(&folder.join("scope-association-receipt.json")),
        serde_json::to_value(&result).unwrap()
    );
    let index: CodeScopeAssociationIndex2 =
        serde_json::from_value(artifacts::json(&f.root, result.index.as_ref().unwrap()).unwrap())
            .unwrap();
    index.validate().unwrap();
    assert_eq!(index.schema, "oev2.code-scope-association-index/2");
    assert_eq!(index.catalog_sha256, CATALOG_SHA);
    let inventory = &index.inventory;
    assert_eq!(inventory.request, f.request);
    assert_eq!(inventory.fixed_scope_count, 13494);
    assert_eq!(inventory.items.len(), 13494);
    assert_eq!(inventory.scope_sha256, SCOPE_SHA);
    assert_eq!(inventory.scope_lock_sha256, LOCK_SHA);
    assert_eq!(inventory.behavior_status, "unknown");
    assert_eq!(inventory.eligibility, "unknown");
    assert_eq!(
        inventory
            .items
            .iter()
            .map(|i| i.scope_id.clone())
            .collect::<BTreeSet<_>>(),
        expected_ids
    );
    assert!(
        inventory
            .items
            .iter()
            .all(|i| i.behavior_status == "unknown")
    );
    assert_eq!(
        inventory
            .items
            .iter()
            .map(|i| i.observed_case_ids.len())
            .sum::<usize>(),
        40
    );
    assert_eq!(
        inventory.candidate_manifest_sha256,
        f.source.manifest_sha256
    );
    assert_eq!(
        inventory.candidate_mappings_sha256.as_deref(),
        Some(mapping_sha.as_str())
    );
    assert_eq!(
        serde_json::to_value(&inventory.implementations).unwrap(),
        mappings
    );
    assert_eq!(
        inventory.policy_sha256,
        sha256(raw(&f.root.path().join(registry::POLICY)))
    );
    assert_eq!(
        inventory.context,
        artifacts::reference(&f.root, &f.root.path().join("var/input/context.json")).unwrap()
    );
    let mut expected_relations = expected_catalog["relations"].clone();
    for relation in expected_relations.as_array_mut().unwrap() {
        relation["observation"] = json!("matched");
        relation["behavior_status"] = json!("unknown");
    }
    assert_eq!(
        serde_json::to_value(&index.relations).unwrap(),
        expected_relations
    );
    let witness: CodeScopeWitnessReport2 =
        serde_json::from_value(artifacts::json(&f.root, &inventory.witness).unwrap()).unwrap();
    assert_eq!(witness.schema, "oev2.code-scope-witness-report/2");
    assert_eq!(witness.catalog_sha256, CATALOG_SHA);
    assert_eq!(witness.relations, index.relations);
    assert_eq!(witness.observation.items, inventory.items);
    assert_eq!(witness.observation.witness.execution, "completed");
    assert_eq!(
        witness.observation.witness.experiment_sha256,
        f.context.qualification.experiment_sha256
    );
    assert_eq!(
        witness.observation.witness.driver_sha256,
        f.experiment.driver.sha256
    );
    assert_eq!(
        witness.observation.witness.evaluator_sha256,
        f.experiment.evaluator.sha256
    );
    assert_eq!(
        witness.observation.witness.baseline_sha256,
        f.experiment.baseline.manifest_sha256
    );
    assert_eq!(
        witness.observation.witness.candidate_sha256,
        f.source.manifest_sha256
    );
    let comparison = witness.observation.witness.comparison.as_ref().unwrap();
    assert_eq!(*comparison, f.assurance);
    assert_eq!(comparison.fixed_denominator, 40);
    assert_eq!(comparison.slots.len(), 40);
    assert_eq!(comparison.engineering_adoption, "unknown");
    assert_eq!(comparison.version_selection, "not_connected");
    for (case, slot) in f.experiment.cases.iter().zip(&comparison.slots) {
        assert_eq!(slot["case_id"], case.case_id);
        assert_eq!(slot["run_id"], 0);
        assert_eq!(slot["candidate"]["status"], "matched");
        assert_eq!(slot["case_in_fixed_denominator"], true);
    }
    let replay = json_file(&folder.join("replay-observation.json"));
    assert_eq!(
        replay["experiment_sha256"],
        f.context.qualification.experiment_sha256
    );
    assert_eq!(
        replay["source"],
        serde_json::to_value(&f.request.source).unwrap()
    );
    assert_eq!(
        replay["assurance"],
        serde_json::to_value(&f.request.assurance).unwrap()
    );
    assert_eq!(
        replay["full_scope_request"],
        serde_json::to_value(&f.context.full_scope_request).unwrap()
    );
    let witness_ref: ArtifactReference =
        serde_json::from_value(replay["scope_witness_report"].clone()).unwrap();
    assert_eq!(witness_ref, inventory.witness);
    let output = replay["fixed_output"].as_str().unwrap();
    let (outer, outer_stdout) = process(
        &folder.join("fixed-driver"),
        &f.experiment.driver.sha256,
        &f.experiment.driver.source_root,
        &json!([
            "scope-witness",
            f.experiment.driver.source_root,
            f.root.path().join("var/input/experiment.json"),
            f.context.qualification.experiment_sha256,
            f.root.path().join("var/input/full-scope-request.json"),
            output
        ]),
    );
    assert_eq!(outer, replay["process"]);
    assert_eq!(
        strict_json(&outer_stdout).unwrap(),
        serde_json::to_value(&witness).unwrap()
    );
    let driver = SkillRoot::new(&f.experiment.driver.source_root).unwrap();
    let scope_folder = driver.private_path(Path::new(output)).unwrap();
    let dispatch = json_file(&scope_folder.join("dispatch.json"));
    assert_eq!(dispatch["schema"], "oev2.code-scope-witness-dispatch/2");
    assert_eq!(
        dispatch["experiment"],
        serde_json::to_value(&f.experiment).unwrap()
    );
    assert_eq!(
        dispatch["experiment_sha256"],
        f.context.qualification.experiment_sha256
    );
    let (_, eval_stdout) = process(
        &scope_folder.join("evaluator-dispatch"),
        &f.experiment.evaluator.sha256,
        &f.experiment.evaluator.source_root,
        &json!(["witness", scope_folder.join("dispatch.json")]),
    );
    assert_eq!(
        strict_json(&eval_stdout).unwrap(),
        serde_json::to_value(&witness.observation.witness).unwrap()
    );
    check_all80(&f, &witness);
    unchanged_originals(&f);
    no_adoption(&f);
}

#[test]
#[ignore = "requires OE_Q40_QUALIFICATION_FIXTURE with current real source/binary descriptors; rejects own altered private copies before dispatch"]
fn q40_owner_rejects_rebound_scope_or_descriptor_before_dispatch() {
    for fault in [
        "source_bytes_without_resign",
        "remove_one_case_and_rebind",
        "remove_one_scope_id_and_rebind",
    ] {
        let f = fixture();
        let expected = match fault {
            "source_bytes_without_resign" => {
                let path = f.root.path().join("var/input/source.json");
                let mut bytes = raw(&path);
                bytes.push(b'\n');
                private_mode(&path);
                fs::write(path, bytes).unwrap();
                "artifact_source_changed"
            }
            "remove_one_case_and_rebind" | "remove_one_scope_id_and_rebind" => {
                let path = f.root.path().join("var/input/full-scope-request.json");
                let mut scope = json_file(&path);
                if fault == "remove_one_case_and_rebind" {
                    scope["items"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|i| !i["case_ids"].as_array().unwrap().is_empty())
                        .unwrap()["case_ids"]
                        .as_array_mut()
                        .unwrap()
                        .pop();
                } else {
                    scope["items"].as_array_mut().unwrap().pop();
                }
                replace_private(&path, &scope);
                let context_path = f.root.path().join("var/input/context.json");
                let mut context = json_file(&context_path);
                context["full_scope_request"] =
                    serde_json::to_value(artifacts::reference(&f.root, &path).unwrap()).unwrap();
                replace_private(&context_path, &context);
                bind_policy(&f.root, &f.request);
                if fault == "remove_one_case_and_rebind" {
                    "q40_catalog_bindings_changed"
                } else {
                    "scope_witness_missing_scope_id"
                }
            }
            _ => unreachable!(),
        };
        let error = registry::CodeArtifactEligibilityExecutor { root: &f.root }
            .associate(&f.request)
            .unwrap_err();
        assert_eq!(error.phase, "code_scope_association");
        assert!(!error.retryable);
        let partial = error.partial_result.as_ref().unwrap();
        assert_eq!(partial["reason"], expected);
        assert_eq!(partial["execution"], "failed");
        assert_eq!(partial["conclusion"], "unknown");
        assert_eq!(partial["engineering_adoption"], "not_established");
        write_new(
            &f.root.path().join("var/owner-negative-result.json"),
            &json!({"fault":fault,"expected_reason":expected,"actual":error}),
        )
        .unwrap();
        no_dispatch(&f.root);
        unchanged_originals(&f);
        no_adoption(&f);
    }
}
