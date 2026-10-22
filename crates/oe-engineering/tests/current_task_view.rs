use oe_contracts::{ArtifactReference, CurrentTaskViewInput};
use oe_core::SkillRoot;
use oe_engineering::current_task_view::{derive, verify_view};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

fn fixture_root() -> PathBuf {
    std::env::var_os("OE_CURRENT_TASK_VIEW_FIXTURE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/current-task-view")
        })
}
fn value(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn reference(root: &SkillRoot, path: &Path) -> ArtifactReference {
    ArtifactReference {
        path: path
            .strip_prefix(root.path())
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        sha256: oe_core::sha256(std::fs::read(path).unwrap()),
    }
}
fn ids(value: &Value, field: Option<&str>) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| field.map_or(v, |name| &v[name]).as_str().unwrap().into())
        .collect()
}
fn strings(values: &Value) -> BTreeSet<String> {
    ids(values, None)
}
fn setup(name: &str) -> (tempfile::TempDir, SkillRoot, CurrentTaskViewInput, Value) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("SKILL.md"),
        "independent static Project view fixture",
    )
    .unwrap();
    let root = SkillRoot::new(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let dst = root.path().join("var/case");
    std::fs::create_dir_all(&dst).unwrap();
    let source = fixture_root().join(name);
    for n in [
        "before.project.json",
        "current.project.json",
        "primary.txt",
        "downstream.txt",
        "independent.txt",
        "unrelated.txt",
    ] {
        std::fs::copy(source.join(n), dst.join(n)).unwrap();
    }
    let sources = [
        "primary.txt",
        "downstream.txt",
        "independent.txt",
        "unrelated.txt",
    ]
    .iter()
    .map(|n| reference(&root, &dst.join(n)))
    .collect();
    let input = CurrentTaskViewInput {
        schema: "oev2.current-task-view-input/1".into(),
        current: reference(&root, &dst.join("current.project.json")),
        previous: Some(reference(&root, &dst.join("before.project.json"))),
        work: None,
        sources,
    };
    (dir, root, input, value(&source.join("EXPECTED.json")))
}
#[test]
fn frozen_same_project_cases_preserve_original_controls() {
    for name in [
        "unchanged",
        "condition-change",
        "refuted-predecessor",
        "deleted-predecessor",
        "deleted-dependency",
        "same-revision-adoption-change",
        "unknown-unscheduled",
        "registry-source-change",
        "unrelated-registry-change",
        "no-model-deterministic",
        "source-byte-drift-linked",
        "source-byte-drift-unrelated",
        "same-typed-different-raw",
    ] {
        let (_dir, root, input, expected) = setup(name);
        let before: Project =
            serde_json::from_value(value(&root.path().join("var/case/before.project.json")))
                .unwrap();
        let current: Project =
            serde_json::from_value(value(&root.path().join("var/case/current.project.json")))
                .unwrap();
        let original = project::reconcile(&before, &current).unwrap();
        assert_eq!(
            ids(&original["decisions"], Some("decision_id")),
            strings(&expected["reconcile_review_ids"]),
            "{name}: original owner control"
        );
        assert_eq!(
            strings(&original["stale_adoptions"]),
            strings(&expected["reconcile_stale_adoptions"]),
            "{name}: original adoption control"
        );
        let raw = std::fs::read(root.path().join("var/case/current.project.json")).unwrap();
        let view = derive(&root, &input).unwrap();
        assert_eq!(view.objects, current.objects);
        for (scope, raw) in &current.conditions {
            assert!(
                view.conditions
                    .iter()
                    .any(|c| c.condition_scope == *scope && c.current_value.as_ref() == Some(raw)),
                "{name}: exact current conditions without source reinterpretation"
            );
        }
        assert_eq!(view.project_raw_sha256, input.current.sha256);
        assert_eq!(
            view.project_value_sha256,
            oe_core::sha256(
                oe_core::json::canonical_json(&serde_json::to_value(&current).unwrap()).unwrap()
            )
        );
        assert_eq!(
            view,
            derive(&root, &input).unwrap(),
            "{name}: deterministic, no model needed"
        );
        assert_eq!(view.model_execution, "not_run");
        assert_eq!(view.project_update, "none");
        assert_eq!(view.engineering_adoption, "not_established");
        assert_eq!(view.arrangement, "unscheduled");
        assert!(
            view.gaps
                .iter()
                .all(|g| g.arrangement == "unscheduled" && g.next_action.starts_with("candidate:"))
        );
        let review: BTreeSet<_> = view
            .decisions
            .iter()
            .filter(|d| d.status == "requires_review")
            .map(|d| d.decision_id.clone())
            .collect();
        assert_eq!(
            review,
            strings(&expected["reconcile_review_ids"]),
            "{name}: original unchanged expectations"
        );
        if name == "deleted-predecessor" {
            assert!(view.decisions.iter().any(|d| d.decision_id == "select"
                && d.status == "deleted"
                && d.current_value.is_none()));
        }
        if name == "deleted-dependency" {
            assert!(view.changes.iter().any(|c| c.kind == "dependency"
                && c.identity == "support-premise"
                && c.status == "deleted"));
        }
        if name == "condition-change" {
            assert!(view.changes.iter().any(|c| c.kind == "condition"
                && c.previous_value.as_deref() == Some("force=10 N; support=cantilever")
                && c.current_value.as_deref() == Some("force=20 N; support=cantilever")));
        }
        if name == "same-revision-adoption-change" {
            assert_eq!(view.project_revision, "same-r1");
            let d = view
                .decisions
                .iter()
                .find(|d| d.decision_id == "select")
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(d.prior_value.as_ref().unwrap()).unwrap()["adopted"],
                true
            );
            assert_eq!(
                serde_json::from_str::<Value>(d.current_value.as_ref().unwrap()).unwrap()["adopted"],
                false
            );
        }
        assert_eq!(
            std::fs::read(root.path().join("var/case/current.project.json")).unwrap(),
            raw,
            "{name}: Project read-only"
        );
        eprintln!(
            "{}",
            json!({"schema":"independent-current-task-case-observation/1","case":name,"original_control":original,"view":view,"criteria":"frozen before candidate edit","project_bytes_unchanged":true})
        );
    }
}
#[test]
fn freshness_and_precise_refresh_are_separate() {
    for (name, file, bytes, review) in [
        (
            "source-byte-drift-linked",
            "primary.txt",
            b"changed raw source without Project registry rewrite\n".as_slice(),
            vec!["select", "cost"],
        ),
        (
            "source-byte-drift-unrelated",
            "unrelated.txt",
            b"changed unrelated raw source without registry rewrite\n".as_slice(),
            vec![],
        ),
    ] {
        let (_dir, root, input, _) = setup(name);
        let prior = derive(&root, &input).unwrap();
        verify_view(&root, &prior).unwrap();
        std::fs::write(root.path().join("var/case").join(file), bytes).unwrap();
        assert!(
            verify_view(&root, &prior).is_err(),
            "{name}: entire old view rejects reuse"
        );
        let refreshed = derive(&root, &input).unwrap();
        let actual: BTreeSet<_> = refreshed
            .decisions
            .iter()
            .filter(|d| d.status == "requires_review")
            .map(|d| d.decision_id.as_str())
            .collect();
        assert_eq!(actual, review.into_iter().collect());
        assert!(
            refreshed
                .source_observations
                .iter()
                .any(|s| s.status == "drift")
        );
        assert!(
            verify_view(&root, &refreshed).is_err(),
            "drifted source is unknown freshness, not certified"
        );
        eprintln!(
            "{}",
            json!({"schema":"independent-current-task-freshness-observation/1","case":name,"prior_reuse":"rejected","view":refreshed})
        );
    }
    for (name, mutation) in [
        ("same-revision-adoption-change", 0),
        ("unrelated-registry-change", 1),
        ("same-typed-different-raw", 2),
    ] {
        let (_dir, root, mut input, _) = setup("unchanged");
        let prior = derive(&root, &input).unwrap();
        verify_view(&root, &prior).unwrap();
        let path = root.path().join("var/case/current.project.json");
        let mut project = value(&path);
        match mutation {
            0 => project["decisions"][0]["adopted"] = json!(false),
            1 => {
                project["evidence"]["source-unrelated"] =
                    json!(oe_core::sha256(b"new unrelated source\n"))
            }
            _ => {}
        }
        let raw = if mutation == 2 {
            serde_json::to_vec(&project).unwrap()
        } else {
            serde_json::to_vec_pretty(&project).unwrap()
        };
        std::fs::write(&path, raw).unwrap();
        assert!(
            verify_view(&root, &prior).is_err(),
            "{name}: full raw/value freshness"
        );
        input.current = reference(&root, &path);
        let refreshed = derive(&root, &input).unwrap();
        if mutation != 0 {
            assert!(
                refreshed
                    .decisions
                    .iter()
                    .all(|d| d.status == "unchanged_declared")
            );
        }
        if mutation == 2 {
            assert_eq!(prior.project_value_sha256, refreshed.project_value_sha256);
            assert_ne!(prior.project_raw_sha256, refreshed.project_raw_sha256);
        }
        eprintln!(
            "{}",
            json!({"schema":"independent-current-task-freshness-observation/1","case":name,"prior_reuse":"rejected","view":refreshed})
        );
    }
}
#[test]
#[ignore = "requires explicitly bound current169 originals; never historical V262 as current"]
fn current169_originals_remain_scoped_and_unscheduled() {
    let fixtures = fixture_root();
    let originals = fixtures.parent().unwrap().join("originals");
    let expected = value(&fixtures.join("CURRENT169-EXPECTED.json"));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("SKILL.md"),
        "independent current169 readonly source fixture",
    )
    .unwrap();
    let root = SkillRoot::new(std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let current = "var/maintenance/engineering-goal-token-diagnostics-20261003-01/current169/inputs-01/project-r260.json";
    let previous =
        "var/maintenance/historical-project-scope/current/project-manufacturing-r259.json";
    let work = "var/maintenance/engineering-goal-token-diagnostics-20261003-01/current169/inputs-01/work-input.json";
    for name in [current, previous, work] {
        let path = root.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(originals.join(name), path).unwrap();
    }
    let input = CurrentTaskViewInput {
        schema: "oev2.current-task-view-input/1".into(),
        current: reference(&root, &root.path().join(current)),
        previous: Some(reference(&root, &root.path().join(previous))),
        work: Some(reference(&root, &root.path().join(work))),
        sources: vec![],
    };
    assert_eq!(input.current.sha256, expected["current_raw_sha256"]);
    assert_eq!(
        input.previous.as_ref().unwrap().sha256,
        expected["prior_raw_sha256"]
    );
    let view = derive(&root, &input).unwrap();
    assert_eq!(view.project_id, expected["project_id"]);
    assert_eq!(view.project_revision, expected["current_revision"]);
    assert_eq!(view.decisions.len(), 10);
    assert!(view.decisions.iter().all(|d| {
        let value: Value = serde_json::from_str(d.current_value.as_ref().unwrap()).unwrap();
        value["conclusion"] == "unknown" && value["adopted"] == false
    }));
    let work_value = value(&root.path().join(work));
    assert_eq!(
        json!(view.necessary_functions),
        work_value["necessary_functions"]
    );
    assert_eq!(
        json!(view.physical_mechanisms),
        work_value["physical_mechanisms"]
    );
    for a in work_value["acceptance"].as_array().unwrap() {
        assert!(view.requirements.contains(&a.as_str().unwrap().to_string()));
    }
    for g in work_value["gaps"].as_array().unwrap() {
        let actual = view
            .gaps
            .iter()
            .find(|row| row.origin == "work" && row.gap_id == g["gap_id"])
            .unwrap();
        assert_eq!(json!(actual.needed_evidence), g["needed_evidence"]);
        assert_eq!(json!(actual.required_conditions), g["required_conditions"]);
        assert_eq!(actual.arrangement, "unscheduled");
    }
    assert_eq!(
        view.gaps.iter().filter(|g| g.origin == "source").count(),
        54
    );
    assert_eq!(view.model_execution, "not_run");
    assert_eq!(view.engineering_adoption, "not_established");
    verify_view(&root, &view).unwrap();
    eprintln!(
        "{}",
        json!({"schema":"independent-current169-task-view-observation/1","source_exposure":"already seen developer replay","original_project_update":"none","view":view})
    );
}
