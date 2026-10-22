//! Actual current CLI /2 with synthetic source records and real Project owner.
#[path = "../../oe-cad/tests/support/occurrence_fixture.rs"]
mod fixture;
use fixture::definition::change;
use fixture::*;
use oe_contracts::{CadInstanceCorrespondenceRequest2, CadOccurrenceBindingRequest1};
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
    f: Fixture,
    directory: PathBuf,
    sequence: Cell<usize>,
}
impl Cli {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = SkillRoot::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap(),
        )
        .unwrap();
        let directory = root.path().join("var/maintenance/rust-tests").join(format!(
            "instance-correspondence-cli-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let f = Fixture::in_root(root, &directory);
        fs::create_dir(directory.join("home")).unwrap();
        Self {
            f,
            directory,
            sequence: Cell::new(0),
        }
    }
    fn context(&self, input: &mut CadOccurrenceBindingRequest1) {
        let p: Project =
            serde_json::from_value(self.f.read_json(&input.definition.project)).unwrap();
        input.definition.project_context = project::context(&p, &input.definition.object).unwrap();
        input.assembly.project_context = project::context(&p, &input.assembly.object).unwrap();
    }
    fn request(&self) -> CadInstanceCorrespondenceRequest2 {
        let previous = endpoint(
            &self.f,
            "before",
            "a1",
            observation("before"),
            direct_graph(0., 0.04),
            false,
        );
        let mut current = endpoint(
            &self.f,
            "after",
            "a2",
            observation("before"),
            direct_graph(0.01, 0.04),
            false,
        );
        let changes = vec![
            change(
                "left",
                "retained",
                &previous.definition,
                &current.definition,
                &[0],
                &[0],
            ),
            change(
                "right",
                "retained",
                &previous.definition,
                &current.definition,
                &[1],
                &[1],
            ),
        ];
        let original = self.f.request(
            previous.definition.clone(),
            current.definition.clone(),
            changes,
        );
        current.definition = original.current;
        refresh_project(&self.f, &mut current, "whole-current-project");
        let mut r = CadInstanceCorrespondenceRequest2 {
            schema: "oev2.cad-correspondence/2".into(),
            previous,
            current,
            changes: original.changes,
        };
        self.context(&mut r.previous);
        self.context(&mut r.current);
        r
    }
    fn call(&self, r: &CadInstanceCorrespondenceRequest2) -> (Output, PathBuf) {
        let id = self.sequence.get();
        self.sequence.set(id + 1);
        let input = self
            .f
            .write_json("instance-input", &serde_json::to_value(r).unwrap());
        let directory = self.directory.join(format!("output-{id}"));
        let args = vec![
            "--root".to_string(),
            self.f.root.path().to_string_lossy().into_owned(),
            "cad".into(),
            "evidence".into(),
            "correspond-instances".into(),
            "--input".into(),
            input.path,
            "--output".into(),
            directory.to_string_lossy().into_owned(),
        ];
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_oev2"));
        cmd.env_clear()
            .env("PATH", "")
            .env("HOME", self.directory.join("home"));
        #[cfg(windows)]
        for name in ["SystemRoot", "WINDIR"] {
            if let Some(v) = std::env::var_os(name) {
                cmd.env(name, v);
            }
        }
        let output = cmd
            .current_dir(self.f.root.path())
            .args(&args)
            .output()
            .unwrap();
        fs::write(
            self.directory.join(format!("call-{id}.stdout")),
            &output.stdout,
        )
        .unwrap();
        fs::write(
            self.directory.join(format!("call-{id}.stderr")),
            &output.stderr,
        )
        .unwrap();
        fs::write(self.directory.join(format!("call-{id}.json")),serde_json::to_vec_pretty(&json!({"program":env!("CARGO_BIN_EXE_oev2"),"args":args,"exit_code":output.status.code(),"native_dispatch":0,"api_credentials":"not_inherited"})).unwrap()).unwrap();
        (output, directory)
    }
    fn edit_project(&self, r: &mut CadOccurrenceBindingRequest1, edit: impl FnOnce(&mut Value)) {
        let mut p = self.f.read_json(&r.definition.project);
        edit(&mut p);
        r.definition.project = self.f.write_json("edited-project", &p);
        self.context(r);
    }
}
#[test]
fn production_instance_correspondence_checks_the_actual_project_and_keeps_unknown_adoption() {
    let cli = Cli::new();
    let r = cli.request();
    let before = fs::read(cli.f.root.path().join(&r.previous.definition.project.path)).unwrap();
    let current = fs::read(cli.f.root.path().join(&r.current.definition.project.path)).unwrap();
    let (out, dir) = cli.call(&r);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["schema"], "oev2.current-cad-correspondence/2");
    assert_eq!(report["engineering_adoption"], "not_established");
    assert_eq!(report["project_write"], "not_run");
    assert_eq!(
        report["correspondence"]["changes"][0]["observed_instance_placement_changed"],
        true
    );
    assert_eq!(
        report["correspondence"]["changes"][1]["observed_instance_placement_changed"],
        false
    );
    assert_eq!(
        report["execution_binding"]["normalizer"]["status"],
        "matched"
    );
    assert_eq!(
        fs::read(cli.f.root.path().join(&r.previous.definition.project.path)).unwrap(),
        before
    );
    assert_eq!(
        fs::read(cli.f.root.path().join(&r.current.definition.project.path)).unwrap(),
        current
    );
    let mut forged = r.clone();
    forged.schema = "oev2.cad-correspondence/1".into();
    assert!(!cli.call(&forged).0.status.success());
}
#[test]
fn current_change_native_condition_and_project_identity_each_reject_at_their_guard() {
    for kind in ["change", "native", "condition", "project"] {
        let cli = Cli::new();
        let mut r = cli.request();
        let expected = match kind {
            "change" => {
                let digest = r.changes[0].source.sha256.clone();
                cli.edit_project(&mut r.current, |p| {
                    p["evidence"]
                        .as_object_mut()
                        .unwrap()
                        .retain(|_, v| v.as_str() != Some(digest.as_str()))
                });
                "cad_instance_correspondence_current_project_missing_change_source"
            }
            "native" => {
                r.current.assembly.native_result_ref = "unregistered-native".into();
                "cad_occurrence_native_result_not_in_project"
            }
            "condition" => {
                r.current.assembly.project_context.condition_sha256 = "0".repeat(64);
                "cad_focus_current_context_mismatch"
            }
            _ => {
                cli.edit_project(&mut r.current, |p| p["project_id"] = json!("other-project"));
                "cad_instance_correspondence_project_identity_mismatch"
            }
        };
        let (out, dir) = cli.call(&r);
        assert_eq!(out.status.code(), Some(2), "{kind}");
        let failure: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(failure["error"]["reason"], expected, "{kind}");
        assert_eq!(failure["engineering_adoption"], "blocked");
        assert!(!dir.exists());
    }
}
#[test]
fn deleted_old_dependency_still_reaches_its_original_downstream_decision() {
    let cli = Cli::new();
    let mut r = cli.request();
    cli.edit_project(&mut r.previous,|p|{
        p["objects"]["unrelated"]=json!("u1");p["evidence"]["unrelated-source"]=json!("e".repeat(64));
        p["dependencies"]["obsolete-edge"]=json!(["fit:left"]);
        p["decisions"]=json!([{"id":"downstream-original","object_id":"unrelated","object_revision":"u1","condition_scope":"geometry-study","source_sha256":"e".repeat(64),"conclusion":"unknown","adopted":false,"basis":["obsolete-edge"]}]);
    });
    cli.edit_project(&mut r.current,|p|{
        p["objects"]["unrelated"]=json!("u1");p["evidence"]["unrelated-source"]=json!("e".repeat(64));
        p["dependencies"].as_object_mut().unwrap().remove("obsolete-edge");
        p["decisions"]=json!([{"id":"downstream-original","object_id":"unrelated","object_revision":"u1","condition_scope":"geometry-study","source_sha256":"e".repeat(64),"conclusion":"unknown","adopted":false,"basis":["unrelated-source"]}]);
    });
    let (out, dir) = cli.call(&r);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(dir.join("report.json")).unwrap()).unwrap();
    assert!(
        report["previous_dependency_impact"]["affected"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "obsolete-edge")
    );
    assert!(
        !report["current_dependency_impact"]["affected"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "obsolete-edge")
    );
    assert!(
        report["requires_review"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "downstream-original")
    );
}
