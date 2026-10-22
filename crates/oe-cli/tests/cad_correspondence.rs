//! Actual CLI source-bound acceptance; synthetic jobs are never launched.
//! These development replays preserve native/engineering/heldout limits.
#[path = "../../oe-cad/tests/support/correspondence_fixture.rs"]
mod fixture;

use fixture::{Fixture, observation, selector};
use oe_contracts::{
    ArtifactReference, CadCorrespondenceRequest1, CadCorrespondenceState1, DrawingReferenceIntent,
};
use oe_core::SkillRoot;
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Cli {
    fixture: Fixture,
    directory: PathBuf,
    sequence: Cell<usize>,
}
impl Cli {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Use the real source root; a minimal synthetic SKILL.md is insufficient
        // for require_execution and cannot substitute a compiler identity.
        let root = SkillRoot::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap(),
        )
        .unwrap();
        let directory = root.path().join("var/maintenance/rust-tests").join(format!(
            "cad-correspondence-cli-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let fixture = Fixture::in_root(root, &directory);
        fs::create_dir(directory.join("home")).unwrap();
        Self {
            fixture,
            directory,
            sequence: Cell::new(0),
        }
    }
    fn call(&self, args: &[String]) -> Output {
        let sequence = self.sequence.get();
        self.sequence.set(sequence + 1);
        let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
        command
            .env_clear()
            .env("PATH", "")
            .env("HOME", self.directory.join("home"));
        #[cfg(windows)]
        {
            for name in ["SystemRoot", "WINDIR"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
            command
                .env("TEMP", self.directory.join("home"))
                .env("TMP", self.directory.join("home"));
        }
        let output = command
            .current_dir(self.fixture.root.path())
            .arg("--root")
            .arg(self.fixture.root.path())
            .args(args)
            .output()
            .unwrap();
        let prefix = self.directory.join(format!("call-{sequence:03}"));
        fs::write(prefix.with_extension("stdout"), &output.stdout).unwrap();
        fs::write(prefix.with_extension("stderr"), &output.stderr).unwrap();
        fs::write(
            prefix.with_extension("json"),
            serde_json::to_vec_pretty(&json!({
                "program":env!("CARGO_BIN_EXE_oev2"), "root":self.fixture.root.path(),
                "argv":args, "exit_code":output.status.code(), "native_dispatch":"not_requested",
                "api_credentials":"not_inherited"
            }))
            .unwrap(),
        )
        .unwrap();
        output
    }
    fn request(&self) -> CadCorrespondenceRequest1 {
        let f = &self.fixture;
        let previous = f.endpoint("before", "r1", observation("before"), &["left_before"]);
        let current = f.endpoint("current", "r2", observation("diameter"), &["left_after"]);
        f.retained(previous, current)
    }
    fn correspond(
        &self,
        request: &CadCorrespondenceRequest1,
        output_name: &str,
    ) -> (Output, PathBuf) {
        let input = self.fixture.write_json(
            "correspondence-input",
            &serde_json::to_value(request).unwrap(),
        );
        let output = self.directory.join(output_name);
        let response = self.call(&[
            "cad".into(),
            "evidence".into(),
            "correspond".into(),
            "--input".into(),
            input.path,
            "--output".into(),
            output.to_str().unwrap().into(),
        ]);
        (response, output)
    }
    fn drawing(
        &self,
        input: &Drawing,
        correspondence: Option<&ArtifactReference>,
        output_name: &str,
    ) -> Output {
        let mut args = vec![
            "cad".into(),
            "drawing".into(),
            "check-references".into(),
            "--input".into(),
            input.input.path.clone(),
            "--project".into(),
            input.request.current.project.path.clone(),
            "--project-context".into(),
            input.context.path.clone(),
            "--output".into(),
            self.directory.join(output_name).to_str().unwrap().into(),
        ];
        if let Some(correspondence) = correspondence {
            args.extend(["--correspondence".into(), correspondence.path.clone()]);
        }
        self.call(&args)
    }
}
fn response(output: Output, expected_code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn rejected(output: Output, destination: &std::path::Path) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    let failure: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure["schema"], "oev2.command-failure/1");
    assert_eq!(failure["execution"], "failed");
    assert_eq!(failure["engineering_adoption"], "blocked");
    assert!(
        !destination.exists(),
        "failed source binding must not publish a checked report"
    );
}
fn edit_project(f: &Fixture, state: &mut CadCorrespondenceState1, edit: impl FnOnce(&mut Project)) {
    let mut project: Project = serde_json::from_value(f.read_json(&state.project)).unwrap();
    edit(&mut project);
    state.project_context = project::context(&project, &state.object).unwrap();
    state.project = f.write_json("project-variant", &serde_json::to_value(project).unwrap());
}
fn assert_library_geometry_still_qualified(f: &Fixture, request: &CadCorrespondenceRequest1) {
    let checked = oe_cad::evidence::correspondence::verify(&f.root, request).unwrap();
    assert_eq!(
        checked.report()["changes"][0]["correspondence_status"],
        "bounded_declared_correspondence"
    );
}

#[test]
fn current_change_provenance_accepts_immutable_old_project_and_refuses_output_reuse() {
    let cli = Cli::new();
    let request = cli.request();
    let f = &cli.fixture;
    let old = fs::read(f.root.path().join(&request.previous.project.path)).unwrap();
    let old_project: Project = serde_json::from_slice(&old).unwrap();
    assert!(
        !old_project
            .evidence
            .values()
            .any(|v| v == &request.changes[0].source.sha256)
    );
    let current: Project = serde_json::from_value(f.read_json(&request.current.project)).unwrap();
    assert!(
        current
            .evidence
            .values()
            .any(|v| v == &request.changes[0].source.sha256)
    );
    let (output, directory) = cli.correspond(&request, "accepted");
    let report = response(output, 0);
    assert_eq!(report["schema"], "oev2.current-cad-correspondence/1");
    assert_eq!(
        report["correspondence"]["changes"][0]["declared_design_changes"],
        json!(["selector.radius"])
    );
    assert_eq!(
        report["execution_binding"]["execution_role"],
        "cad_correspondence"
    );
    assert_eq!(
        report["execution_binding"]["executable_sha256"],
        oe_core::sha256(fs::read(env!("CARGO_BIN_EXE_oev2")).unwrap())
    );
    assert_eq!(report["project_write"], "not_run");
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(
        fs::read(f.root.path().join(&request.previous.project.path)).unwrap(),
        old
    );
    let before_report = fs::read(directory.join("report.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&before_report).unwrap(),
        report
    );
    let (again, same_directory) = cli.correspond(&request, "accepted");
    assert_eq!(again.status.code(), Some(2));
    assert_eq!(same_directory, directory);
    assert_eq!(
        fs::read(directory.join("report.json")).unwrap(),
        before_report
    );
}

#[test]
fn undeclared_current_change_or_endpoint_sources_block_publication() {
    for target in [
        "current-change",
        "current-job",
        "current-intent",
        "previous-job",
        "previous-intent",
    ] {
        let cli = Cli::new();
        let mut request = cli.request();
        let digest = match target {
            "current-change" => request.changes[0].source.sha256.clone(),
            "current-job" => request.current.job.sha256.clone(),
            "current-intent" => request.current.bindings[0].intent.source.sha256.clone(),
            "previous-job" => request.previous.job.sha256.clone(),
            "previous-intent" => request.previous.bindings[0].intent.source.sha256.clone(),
            _ => unreachable!(),
        };
        let state = if target.starts_with("previous") {
            &mut request.previous
        } else {
            &mut request.current
        };
        edit_project(&cli.fixture, state, |project| {
            project.evidence.retain(|_, value| value != &digest);
        });
        assert_library_geometry_still_qualified(&cli.fixture, &request);
        let (output, destination) = cli.correspond(&request, "must-not-publish");
        rejected(output, &destination);
    }
}

#[test]
fn wrong_native_digest_or_stale_project_context_cannot_use_qualified_geometry() {
    for target in ["previous-native", "current-native", "project-context"] {
        let cli = Cli::new();
        let mut request = cli.request();
        if target == "project-context" {
            let old_context = request.current.project_context.clone();
            edit_project(&cli.fixture, &mut request.current, |project| {
                project.conditions.insert(
                    "geometry-study".into(),
                    "Changed engineering conditions at same revision".into(),
                );
            });
            request.current.project_context = old_context;
        } else {
            let state = if target == "previous-native" {
                &mut request.previous
            } else {
                &mut request.current
            };
            let source_id = state.native_result_ref.clone();
            edit_project(&cli.fixture, state, |project| {
                project.evidence.insert(
                    source_id,
                    oe_core::sha256(b"independently different original receipt"),
                );
            });
        }
        assert_library_geometry_still_qualified(&cli.fixture, &request);
        let (output, destination) = cli.correspond(&request, "must-not-publish");
        rejected(output, &destination);
    }
}

#[test]
fn dependency_declared_only_in_other_endpoint_does_not_satisfy_original_scope() {
    for endpoint in ["previous", "current"] {
        let cli = Cli::new();
        let mut request = cli.request();
        let state = if endpoint == "previous" {
            &mut request.previous
        } else {
            &mut request.current
        };
        edit_project(&cli.fixture, state, |project| {
            assert_eq!(
                project.dependencies.remove("fit:left"),
                Some(["model".into()].into())
            );
        });
        let other = if endpoint == "previous" {
            &request.current
        } else {
            &request.previous
        };
        assert!(
            cli.fixture.read_json(&other.project)["dependencies"]
                .get("fit:left")
                .is_some()
        );
        assert_library_geometry_still_qualified(&cli.fixture, &request);
        let (output, destination) = cli.correspond(&request, "must-not-publish");
        rejected(output, &destination);
    }
}

#[test]
fn deleting_old_dependency_reopens_downstream_decision_even_when_geometry_has_no_delta() {
    let cli = Cli::new();
    let f = &cli.fixture;
    let mut previous = f.endpoint("unchanged", "r1", observation("before"), &["left_before"]);
    let independent = f.write_json(
        "unrelated-catalogue",
        &json!({"scope":"unrelated synthetic catalogue"}),
    );
    edit_project(f, &mut previous, |project| {
        project.objects.insert("deliverable".into(), "r1".into());
        project
            .conditions
            .insert("independent".into(), "Unchanged separate condition".into());
        project
            .evidence
            .insert("independent-document".into(), independent.sha256.clone());
        project
            .dependencies
            .insert("downstream".into(), ["inspection:left".into()].into());
        for (id, basis) in [
            ("affected-decision", "downstream"),
            ("unrelated-decision", "independent-document"),
        ] {
            project.decisions.push(project::Decision {
                id: id.into(),
                object_id: "deliverable".into(),
                object_revision: "r1".into(),
                condition_scope: "independent".into(),
                source_sha256: independent.sha256.clone(),
                conclusion: "unknown".into(),
                adopted: false,
                basis: vec![basis.into()],
            });
        }
    });
    let mut current = previous.clone();
    edit_project(f, &mut current, |project| {
        project.dependencies.remove("downstream");
    });
    let request = f.retained(previous, current);
    let original_before = f.read_json(&request.previous.project);
    let original_after = f.read_json(&request.current.project);
    let (output, _) = cli.correspond(&request, "old-dependency-review");
    let report = response(output, 0);
    assert_eq!(
        report["correspondence"]["changes"][0]["review"],
        "no_correspondence_delta"
    );
    assert_eq!(report["requires_review"], json!(["affected-decision"]));
    assert_eq!(
        report["project_reconciliation"]["decisions"][0]["decision_id"],
        "affected-decision"
    );
    assert_eq!(
        report["unknown_dependencies"],
        "unlisted links remain unverified"
    );
    assert_eq!(f.read_json(&request.previous.project), original_before);
    assert_eq!(f.read_json(&request.current.project), original_after);
    assert_eq!(report["project_write"], "not_run");
}

#[test]
fn partial_observation_is_reported_as_unresolved_and_never_as_engineering_adoption() {
    let cli = Cli::new();
    let f = &cli.fixture;
    let previous = f.endpoint("before", "r1", observation("before"), &["left_before"]);
    let current = f.endpoint("current", "r2", observation("partial"), &["left_before"]);
    let request = f.retained(previous, current);
    let before = f.read_json(&request.current.project);
    let (output, _) = cli.correspond(&request, "partial");
    let report = response(output, 0); // A recorded bounded check is not successful engineering.
    assert_eq!(
        report["correspondence"]["changes"][0]["correspondence_status"],
        "unresolved"
    );
    assert_eq!(
        report["correspondence"]["changes"][0]["after_checks"][0]["conclusion"],
        "unknown"
    );
    assert_eq!(
        report["correspondence"]["changes"][0]["after_checks"][0]["candidate_edge_tags"],
        json!([101])
    );
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(report["project_write"], "not_run");
    assert_eq!(f.read_json(&request.current.project), before);
}

struct Drawing {
    request: CadCorrespondenceRequest1,
    input: ArtifactReference,
    context: ArtifactReference,
}
fn drawing_fixture(cli: &Cli, defect: &str, different_selector: bool) -> Drawing {
    let f = &cli.fixture;
    let mut part = observation("diameter");
    let entry = json!({
        "association_position_valid":true,"association_position":[-20,0,10],"unit":"mm",
        "coordinate_basis":"UF_DRF assoc_point absolute coordinates","point_type_raw":2,"line_type_raw":0,
        "object_1":{"present":true,"tag":201,"metadata":{
            "type_query":{"return_code":0,"type_raw":70,"subtype_raw":3},
            "owning_part_query":{"value_valid":true,"matches_expected_owner":true}
        }}
    });
    let dimension = json!({"dimension_tag":900,"retained":false,"retained_query_code":0,
        "associativity_parents":{"capture_complete":true,"wrapper_code":0,"uf_return_code":0,"entries":[entry]}});
    part["annotations"] = json!({"schema":"oev2.nx-annotation-observation/2",
        "units_query":{"return_code":0,"unit":"mm"},"dimensions":[dimension.clone()]});
    match defect {
        "none" => {}
        "wrong-parent" => {
            part["annotations"]["dimensions"][0]["associativity_parents"]["entries"][0]["object_1"]
                ["tag"] = json!(999)
        }
        "duplicate" => part["annotations"]["dimensions"]
            .as_array_mut()
            .unwrap()
            .push(dimension),
        "unit-unknown" => part["annotations"]["units_query"]["unit"] = Value::Null,
        _ => panic!("unknown independent drawing defect"),
    }
    let previous = f.endpoint("before", "r1", observation("before"), &["left_before"]);
    let mut current = f.endpoint("current", "r2", part, &["left_after"]);
    let mut circle = selector("left_after");
    if different_selector {
        circle["center_tolerance"] = json!(0.0002);
    }
    let intent: DrawingReferenceIntent = serde_json::from_value(json!({
        "schema":"oev2.drawing-reference-intent/3","intent_id":"left-hole-location",
        "object":current.object,"local_frame":"part_absolute","unit":"mm","position_tolerance":0.0001,
        "proof_scope":"unique_model_parent","dimension_value":null,
        "points":[{"point_id":"left","position":[-20,0,10],"point_type_raw":2,"line_type_raw":0,
            "parent_type_raw":70,"parent_subtype_raw":3,"parent_edge":null,"parent_edge_v2":circle}],
        "source_refs":["independent-drawing-design"]
    })).unwrap();
    let independent = f.write_json(
        "independent-drawing-design",
        &serde_json::to_value(&intent).unwrap(),
    );
    // This reads complete synthetic protocol originals through the normal reader.
    // It does not launch NX and is never counted as an actual native receipt.
    let native = oe_cad::drawing::intent::observe_bound(
        &f.job_directory(&current),
        &current.part_input,
        &intent,
        &current.native_result_ref,
    )
    .unwrap();
    let basis = json!({
        "schema":"oev2.engineering-basis/1","object_id":current.object.object_id,
        "object_revision":current.object.revision,"purpose":"Synthetic independent drawing obligation",
        "acceptance":["source-bound reference only; material, fit and physical acceptance remain unknown"],
        "facts":[
            {"property":"drawing_reference:left-hole-location","value":oe_cad::drawing::intent::intent_value(&intent).unwrap(),
                "status":"declared","source_refs":["independent-drawing-design"]},
            {"property":"native_drawing:left-hole-location","value":oe_cad::drawing::intent::observation_value(&native).unwrap(),
                "status":"declared","source_refs":[current.native_result_ref,"model"]}
        ],
        "assertions":[]
    });
    edit_project(f, &mut current, |project| {
        project
            .evidence
            .insert("independent-drawing-design".into(), independent.sha256);
        project.conditions.insert(
            "geometry-study".into(),
            serde_json::to_string(&basis).unwrap(),
        );
    });
    let request = f.retained(previous, current);
    let context = f.write_json(
        "drawing-project-context",
        &serde_json::to_value(&request.current.project_context).unwrap(),
    );
    let input = f.write_json("drawing-input", &json!({
        "intent":oe_cad::drawing::intent::intent_value(&intent).unwrap(),
        "job_directory":f.job_directory(&request.current),
        "part_input":request.current.part_input,"native_result_ref":request.current.native_result_ref
    }));
    Drawing {
        request,
        input,
        context,
    }
}
fn mapping_input(cli: &Cli, drawing: &Drawing) -> ArtifactReference {
    cli.fixture.write_json(
        "drawing-correspondence",
        &serde_json::to_value(&drawing.request).unwrap(),
    )
}
fn legacy_projection(mut report: Value) -> Value {
    report["schema"] = json!("oev2.current-drawing-reference/1");
    report
        .as_object_mut()
        .unwrap()
        .remove("correspondence_check");
    report.as_object_mut().unwrap().remove("execution_binding");
    report
}

#[test]
fn drawing_optional_correspondence_preserves_every_legacy_check_and_requires_fresh_sources() {
    let cli = Cli::new();
    let drawing = drawing_fixture(&cli, "none", false);
    let mapping = mapping_input(&cli, &drawing);
    let legacy = response(cli.drawing(&drawing, None, "drawing-legacy.json"), 0);
    let current = response(
        cli.drawing(&drawing, Some(&mapping), "drawing-current.json"),
        0,
    );
    assert_eq!(legacy["schema"], "oev2.current-drawing-reference/1");
    assert!(legacy.get("correspondence_check").is_none());
    assert_eq!(current["schema"], "oev2.current-drawing-reference/2");
    assert_eq!(current["correspondence_check"]["conclusion"], "pass");
    assert_eq!(
        current["native_reference_check"]["intended_parent_correctness"],
        "pass"
    );
    assert_eq!(current["native_source_check"]["conclusion"], "pass");
    assert_eq!(current["engineering_adoption"], "not_established");
    assert_eq!(legacy_projection(legacy), legacy_projection(current));
}

#[test]
fn drawing_mapping_cannot_override_wrong_parent_duplicate_or_unknown_unit() {
    for (defect, reason) in [
        ("wrong-parent", "drawing_reference_model_parent_mismatch"),
        ("duplicate", "drawing_reference_ambiguous"),
        ("unit-unknown", "drawing_reference_unit_unknown"),
    ] {
        let cli = Cli::new();
        let drawing = drawing_fixture(&cli, defect, false);
        let mapping = mapping_input(&cli, &drawing);
        let legacy = response(cli.drawing(&drawing, None, "drawing-legacy.json"), 2);
        let current = response(
            cli.drawing(&drawing, Some(&mapping), "drawing-current.json"),
            2,
        );
        assert_eq!(
            current["correspondence_check"]["conclusion"], "pass",
            "the independent circle mapping is available; the original drawing check must still decide this defect"
        );
        assert_eq!(current["native_reference_check"]["reason"], reason);
        assert_eq!(current["current_use"], "blocked_or_historical_only");
        assert_eq!(legacy_projection(legacy), legacy_projection(current));
    }
}

#[test]
fn drawing_requires_exact_frozen_selector_even_when_both_geometries_match() {
    let cli = Cli::new();
    let drawing = drawing_fixture(&cli, "none", true);
    let mapping = mapping_input(&cli, &drawing);
    let legacy = response(cli.drawing(&drawing, None, "drawing-legacy.json"), 0);
    let current = response(
        cli.drawing(&drawing, Some(&mapping), "drawing-current.json"),
        2,
    );
    assert_eq!(legacy["native_reference_check"]["conclusion"], "pass");
    assert_eq!(
        current["native_reference_check"],
        legacy["native_reference_check"]
    );
    assert_eq!(current["correspondence_check"]["conclusion"], "unknown");
    assert_eq!(
        current["correspondence_check"]["error"],
        "cad_correspondence_drawing_selector_differs"
    );
    assert_eq!(current["current_use"], "blocked_or_historical_only");
}

#[test]
fn drawing_rejects_mapping_for_different_project_bytes_and_a_saved_report_as_input() {
    for variant in ["different-project", "saved-report"] {
        let cli = Cli::new();
        let mut drawing = drawing_fixture(&cli, "none", false);
        let mapping = if variant == "different-project" {
            let prior = mapping_input(&cli, &drawing);
            edit_project(&cli.fixture, &mut drawing.request.current, |project| {
                project.evidence.insert(
                    "extra-original".into(),
                    oe_core::sha256(b"new unchanged-design context source"),
                );
            });
            drawing.context = cli.fixture.write_json(
                "updated-drawing-context",
                &serde_json::to_value(&drawing.request.current.project_context).unwrap(),
            );
            prior
        } else {
            let checked =
                oe_cad::evidence::correspondence::verify(&cli.fixture.root, &drawing.request)
                    .unwrap();
            cli.fixture
                .write_json("saved-report-not-authority", checked.report())
        };
        let legacy = response(cli.drawing(&drawing, None, "drawing-legacy.json"), 0);
        let current = response(
            cli.drawing(&drawing, Some(&mapping), "drawing-current.json"),
            2,
        );
        assert_eq!(
            current["native_reference_check"],
            legacy["native_reference_check"]
        );
        assert_eq!(current["correspondence_check"]["conclusion"], "unknown");
        assert_eq!(
            current["correspondence_check"]["reason"],
            "cad_correspondence_current_drawing_unbound"
        );
        if variant == "different-project" {
            assert_eq!(
                current["correspondence_check"]["error"],
                "cad_correspondence_drawing_project_changed"
            );
        }
        assert_eq!(current["current_use"], "blocked_or_historical_only");
    }
}
