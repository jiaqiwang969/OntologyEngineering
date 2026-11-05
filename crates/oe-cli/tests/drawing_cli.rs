//! The drawing route, discovery, the independent check and the sandboxed worker through the oev2 executable, on
//! the original public I1 drawing and on synthetic bytes. macOS only: the worker runs under sandbox-exec.
#![cfg(target_os = "macos")]

use oe_contracts::{ArtifactReference, WorkspaceCommitRequest};
use oe_core::{SkillRoot, sha256};
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PROBE_DXF: &[u8] = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n1\n8\n0\n330\nM\n10\n0\n20\n0\n11\n10\n21\n0\n0\nLINE\n5\n2\n8\n0\n330\nM\n10\n10\n20\n0\n11\n10\n21\n5\n0\nENDSEC\n0\nEOF\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn example(name: &str) -> PathBuf {
    root()
        .join("examples/drawing-understanding/i1-stepped-shaft")
        .join(name)
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn write(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

/// A project home with an anchored workspace whose current Project binds the public I1 drawing.
struct Home {
    _directory: tempfile::TempDir,
    path: PathBuf,
    project: Value,
    object: Value,
}

impl Home {
    fn new() -> Self {
        let root = root();
        let directory = tempfile::tempdir_in(root.join("var")).unwrap();
        let path = directory.path().canonicalize().unwrap();
        let skill = SkillRoot::new(&root)
            .unwrap()
            .with_project_data_root(&path)
            .unwrap();
        std::fs::create_dir(path.join("sources")).unwrap();
        for name in ["drawing.dwg", "source.dxf", "source.svg"] {
            std::fs::copy(example(name), path.join("sources").join(name)).unwrap();
        }
        let source = sha256(std::fs::read(path.join("sources/source.dxf")).unwrap());
        let purpose = "Read the original public I1 drawing with every source entity kept and part targets unknown.";
        let project = json!({"schema":"oev2.project/1","project_id":"i1-drawing","revision":"r1","purpose":purpose,
            "objects":{"i1-shaft":"r1"},"conditions":{"i1-public":"Original synthetic I1 stepped shaft drawing in mm; no manufacturing adoption."},
            "evidence":{"i1-drawing":source},"decisions":[],"dependencies":{}});
        let object = json!({"object_id":"i1-shaft","revision":"r1","condition_scope":"i1-public","source_sha256":source});
        let anchor = json!({"schema":"oev2.route-anchor/1","project_id":"i1-drawing","root_object":"i1-shaft",
            "deliverable":"Source-bound reading of the public drawing with unknowns retained",
            "requests":[{"id":"U1","kind":"root","text":purpose,"source":"root-user.txt","affects":[]}],
            "functions":[{"id":"F1","statement":"Keep every source entity and its uses","structures":["i1-shaft"]}],
            "acceptance":[{"id":"A1","statement":"Every source entity is kept and part targets stay unknown","functions":["F1"],"status":"open"}],
            "stages":[{"id":"S1","object_id":"i1-shaft","relation":"root","relation_target":"i1-shaft","advances":["A1"],"outcome":"advanced"}],
            "reviews":[{"id":"R1","after_stage":"S1","source":"synthetic route review"}],
            "policy":{"off_root_stage_limit":2,"no_progress_stage_limit":2,"review_interval":5}});
        write(&path.join("project.json"), &project);
        write(&path.join("object.json"), &object);
        write(&path.join("anchor.json"), &anchor);
        std::fs::write(path.join("root-user.txt"), purpose).unwrap();
        let pin = |name: &str| ArtifactReference {
            path: name.into(),
            sha256: sha256(std::fs::read(path.join(name)).unwrap()),
        };
        oe_engineering::project_workspace::initialize(&skill, "i1-drawing", "i1-drawing").unwrap();
        let proposal = json!({"schema":"oev2.workspace-commit-request/1","workspace_id":"i1-drawing","commit_id":"c1",
            "expected_parent":null,"project":pin("project.json"),"anchor":pin("anchor.json"),
            "requests":[{"request_id":"U1","source":pin("root-user.txt")}],"corrections":[],"causal_claims":[],"reviews":[],
            "events":[],"sources":[pin("sources/source.dxf")],"work":null,"checkpoint":null,"stage":null});
        let request: WorkspaceCommitRequest = serde_json::from_value(proposal).unwrap();
        let committed = oe_engineering::project_workspace::commit(&skill, &request).unwrap();
        assert_eq!(committed["status"], "committed", "{committed:#}");
        let resumed = oe_engineering::project_workspace::resume(&skill).unwrap();
        assert_eq!(resumed["status"], "ready", "{resumed:#}");
        assert_eq!(resumed["route"]["status"], "anchored");
        Self {
            _directory: directory,
            path,
            project,
            object,
        }
    }

    fn oev2<S: AsRef<OsStr>>(&self, args: &[S]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .arg("--root")
            .arg(root())
            .arg("--project-home")
            .arg(&self.path)
            .args(args)
            .output()
            .unwrap()
    }

    /// A discovery request for the I1 DXF under the given Project, written to `name`.
    fn request(&self, project: &Value, name: &str, output_directory: &str) -> Value {
        let project: oe_kernel::project::Project = serde_json::from_value(project.clone()).unwrap();
        let object = serde_json::from_value(self.object.clone()).unwrap();
        let context = oe_kernel::project::context(&project, &object).unwrap();
        let request = json!({"schema":"oev2.drawing-discovery-request/1","subject":self.object,"project_context":context,
            "source":{"path":"sources/source.dxf","sha256":self.object["source_sha256"]},
            "profile":oe_drawing::engineering_expression::PROFILE,"output_directory":output_directory});
        write(&self.path.join(name), &request);
        request
    }

    fn discover(&self) -> Value {
        self.request(&self.project, "request.json", "classified");
        let output = self.oev2(&[
            "drawing",
            "discover",
            "--project",
            "project.json",
            "--request",
            "request.json",
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        read(&self.path.join("classified/graph.json"))
    }
}

fn failure_reason(output: &Output) -> String {
    let line = String::from_utf8_lossy(&output.stderr);
    let value: Value = serde_json::from_str(line.trim()).unwrap_or_else(|_| panic!("{line}"));
    value["error"]["reason"].as_str().unwrap().to_owned()
}

fn sandboxed(program: &Path, args: &[&OsStr], stdin: &[u8]) -> Output {
    let profile = oe_drawing_check::graph::expected_profile(program.to_str().unwrap()).unwrap();
    let mut child = Command::new("/usr/bin/sandbox-exec")
        .env_clear()
        .current_dir("/")
        .arg("-p")
        .arg(profile)
        .arg(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn the_worker_runs_only_in_its_sandbox_and_refuses_extra_arguments() {
    let exe = Path::new(env!("CARGO_BIN_EXE_oev2"));
    let flag = OsStr::new(oe_drawing::review::WORKER_FLAG);
    let output = sandboxed(exe, &[flag], PROBE_DXF);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let graph: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(graph["schema"], "oev2.drawing-expression-graph/1");
    assert_eq!(graph["source"]["entities"].as_array().unwrap().len(), 2);
    let directory = tempfile::tempdir_in(root().join("var")).unwrap();
    let canary = directory.path().join("canary.txt");
    std::fs::write(&canary, "Boundary probe only, not engineering data.\n").unwrap();
    for (program, args) in [
        ("/bin/cat", vec![canary.as_os_str()]),
        (
            "/usr/bin/nc",
            ["-vz", "-w", "1", "127.0.0.1", "9"]
                .map(OsStr::new)
                .to_vec(),
        ),
    ] {
        let output = sandboxed(Path::new(program), &args, b"");
        assert!(
            !output.status.success(),
            "{program} ran outside the deny-default rules"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Operation not permitted"),
            "{program}"
        );
    }
    let output = sandboxed(exe, &[flag, OsStr::new("unexpected-path")], PROBE_DXF);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("expression_worker_no_extra_arguments")
    );
}

#[test]
fn discovery_is_bound_to_the_current_project() {
    let home = Home::new();
    let graph = home.discover();
    assert_eq!(graph["schema"], "oev2.drawing-expression-graph/1");
    assert_eq!(graph["source_sha256"], home.object["source_sha256"]);
    let review = read(&home.path.join("classified/review.json"));
    assert_eq!(review["status"], "expression_candidates_for_review");
    assert_eq!(
        review["source"],
        json!({"path":"sources/source.dxf","sha256":home.object["source_sha256"]})
    );
    // A context computed from a Project revision that is not the workspace's current one is refused.
    let mut other = home.project.clone();
    other["revision"] = json!("r2");
    write(&home.path.join("project-r2.json"), &other);
    home.request(&other, "request-r2.json", "classified-r2");
    let output = home.oev2(&[
        "drawing",
        "discover",
        "--project",
        "project-r2.json",
        "--request",
        "request-r2.json",
    ]);
    assert!(!output.status.success());
    assert_eq!(failure_reason(&output), "native_current_project_changed");
    assert!(!home.path.join("classified-r2").exists());
}

#[test]
fn the_route_on_the_public_drawing_writes_graph_check_and_page() {
    let home = Home::new();
    let output = home.oev2(&[
        "drawing",
        "route",
        "--project",
        "project.json",
        "--object",
        "object.json",
        "--dxf",
        "sources/source.dxf",
        "--svg",
        "sources/source.svg",
        "--output-directory",
        "route",
        "--title",
        "I1 stepped shaft",
    ]);
    let receipt = read(&home.path.join("route/route-receipt.json"));
    assert_eq!(output.status.code(), Some(1), "{receipt:#}");
    assert_eq!(receipt["status"], "page_not_checked", "{receipt:#}");
    let status: Vec<(&str, &str)> = receipt["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["id"].as_str().unwrap(), s["status"].as_str().unwrap()))
        .collect();
    assert_eq!(
        status,
        [
            ("verify-runtime", "passed"),
            ("read-source", "passed"),
            ("probe-sandbox", "passed"),
            ("convert", "not_applicable"),
            ("render", "not_applicable"),
            ("context", "passed"),
            ("request", "passed"),
            ("classify", "passed"),
            ("check-graph", "passed"),
            ("page", "passed"),
            ("check-page", "skipped"),
            ("verify-source", "passed"),
        ]
    );
    let graph = read(&home.path.join("route/classified/graph.json"));
    assert_eq!(graph["schema"], "oev2.drawing-expression-graph/1");
    let check = read(&home.path.join("route/graph-check.json"));
    assert_eq!(check["status"], "passed");
    assert!(check["source_derived_count"].as_u64().unwrap() > 0);
    assert_eq!(
        check["by_kind"]["Every original model-space graphic root preserved exactly"],
        1
    );
    let page = std::fs::read_to_string(home.path.join("route/review-page.html")).unwrap();
    assert!(
        page.contains("\"schema\":\"oev2.drawing-expression-graph/1\"")
            && !page.contains("@@DATA@@")
            && !page.contains("@@SVG@@")
    );
    assert!(page.contains(&std::fs::read_to_string(example("source.svg")).unwrap()));
}

#[test]
fn a_converter_that_does_not_match_its_record_or_the_lock_is_refused() {
    let home = Home::new();
    let tools = home.path.join("tools");
    std::fs::create_dir(&tools).unwrap();
    let program = tools.join("cadcraft-cli");
    std::fs::write(&program, "#!/bin/sh\necho 'cadcraft-cli 0.0.0 (other)'\n").unwrap();
    std::fs::set_permissions(
        &program,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let lock = oe_drawing::route::lock().unwrap();
    let digest = sha256(std::fs::read(&program).unwrap());
    let base = json!({"schema":"oev2.cadcraft-runtime/1","program_path":program,"program_sha256":digest,
        "repository":lock["repository"],"source_commit":lock["commit"],"fonts_directory":null,"fonts_manifest_path":null,"fonts_manifest_sha256":null});
    let mut wrong_digest = base.clone();
    wrong_digest["program_sha256"] = json!("0".repeat(64));
    let mut wrong_commit = base.clone();
    wrong_commit["source_commit"] = json!("1".repeat(40));
    for (name, record, reason, ran) in [
        (
            "digest",
            wrong_digest,
            "cadcraft_program_digest_mismatch",
            false,
        ),
        ("commit", wrong_commit, "cadcraft_commit_not_locked", false),
        ("version", base, "cadcraft_version_mismatch", true),
    ] {
        write(&home.path.join(format!("runtime-{name}.json")), &record);
        let output = home.oev2(&[
            "drawing",
            "route",
            "--project",
            "project.json",
            "--object",
            "object.json",
            "--dwg",
            "sources/drawing.dwg",
            "--cadcraft-runtime",
            &format!("runtime-{name}.json"),
            "--output-directory",
            &format!("refused-{name}"),
            "--title",
            "I1 stepped shaft",
        ]);
        assert_eq!(output.status.code(), Some(1), "{name}");
        let receipt = read(&home.path.join(format!("refused-{name}/route-receipt.json")));
        assert_eq!(receipt["status"], "failed");
        assert_eq!(receipt["failure"]["step"], "verify-runtime", "{name}");
        assert!(
            receipt["failure"]["message"]
                .as_str()
                .unwrap()
                .starts_with(reason),
            "{name}: {receipt:#}"
        );
        let steps = receipt["steps"].as_array().unwrap();
        assert_eq!(
            steps[0]["external_processes"].as_array().unwrap().len(),
            usize::from(ran),
            "{name}"
        );
        assert!(
            steps[1..].iter().all(|s| s["status"] == "not_run"),
            "{name}"
        );
    }
}

#[test]
fn the_check_passes_on_a_graph_derived_from_its_source() {
    let home = Home::new();
    home.discover();
    let output = home.oev2(&[
        "drawing",
        "check",
        "--request",
        "request.json",
        "--svg",
        "sources/source.svg",
        "--output",
        "check.json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = read(&home.path.join("check.json"));
    assert_eq!(report["status"], "passed");
    assert_eq!(report["schema"], "oev2.drawing-expression-graph-check/1");
    // Identity assertions: the worker and the review binding; no pinned files without the route.
    assert_eq!(
        report["count"].as_u64().unwrap(),
        report["source_derived_count"].as_u64().unwrap() + 2
    );
    assert_eq!(
        report["graph_sha256"],
        sha256(std::fs::read(home.path.join("classified/graph.json")).unwrap())
    );
}

#[test]
fn a_single_changed_fact_fails_its_own_check() {
    let home = Home::new();
    let graph = home.discover();
    let entities = graph["source"]["entities"].as_array().unwrap();
    let line = entities
        .iter()
        .position(|e| e["geometry_kind"] == "line")
        .expect("a LINE source");
    let members = graph["members"].as_array().unwrap();
    let single = entities
        .iter()
        .filter(|e| e["geometry_kind"] == "line")
        .map(|e| e["id"].clone())
        .find(|id| members.iter().filter(|m| m["source_handle"] == *id).count() == 1)
        .expect("a LINE source with one member");
    let member = members
        .iter()
        .position(|m| m["source_handle"] == single)
        .unwrap();
    let expression = graph["expressions"]
        .as_array()
        .unwrap()
        .iter()
        .position(|e| e["family"] != "unresolved")
        .unwrap();
    type Change = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, Change, &str)> = vec![
        (
            "endpoint",
            Box::new(move |g: &mut Value| {
                let x = g["source"]["entities"][line]["coordinates"][0]
                    .as_f64()
                    .unwrap();
                g["source"]["entities"][line]["coordinates"][0] = json!(x + 1.0);
            }),
            "Qualified LINE keeps exact source endpoints",
        ),
        (
            "interval",
            Box::new(move |g: &mut Value| g["members"][member]["interval"] = json!([0.0, 0.5])),
            "Source domain covered to its end",
        ),
        (
            "target",
            Box::new(move |g: &mut Value| {
                g["expressions"][expression]["engineering_target"] = json!("shaft.left")
            }),
            "No invented part target",
        ),
        (
            "family",
            Box::new(move |g: &mut Value| {
                g["expressions"][expression]["family"] = json!("unresolved")
            }),
            "Family follows kind",
        ),
    ];
    for (name, change, label) in cases {
        let directory = format!("fault-{name}");
        std::fs::create_dir(home.path.join(&directory)).unwrap();
        let mut changed = graph.clone();
        change(&mut changed);
        write(&home.path.join(&directory).join("graph.json"), &changed);
        std::fs::copy(
            home.path.join("classified/review.json"),
            home.path.join(&directory).join("review.json"),
        )
        .unwrap();
        let mut request = read(&home.path.join("request.json"));
        request["output_directory"] = json!(directory);
        write(&home.path.join(format!("request-{name}.json")), &request);
        let output = home.oev2(&[
            "drawing",
            "check",
            "--request",
            &format!("request-{name}.json"),
            "--svg",
            "sources/source.svg",
            "--output",
            &format!("check-{name}.json"),
        ]);
        assert!(!output.status.success(), "{name}");
        assert_eq!(
            failure_reason(&output),
            format!("drawing_check_failed:{label}"),
            "{name}"
        );
        assert!(!home.path.join(format!("check-{name}.json")).exists());
    }
}

#[test]
fn the_route_receipts_discharge_the_drawing_obligations() {
    let home = Home::new();
    let output = home.oev2(&[
        "drawing",
        "route",
        "--project",
        "project.json",
        "--object",
        "object.json",
        "--dxf",
        "sources/source.dxf",
        "--svg",
        "sources/source.svg",
        "--output-directory",
        "route",
        "--title",
        "I1 stepped shaft",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let judged = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .arg("--root")
        .arg(root())
        .args(["design", "evidence", "--run"])
        .arg(home.path.join("route"))
        .output()
        .unwrap();
    // Without a browser the page obligations lack their documents, so the command does not pass.
    assert_eq!(
        judged.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&judged.stderr)
    );
    let report: Value = serde_json::from_slice(&judged.stdout).unwrap();
    assert_eq!(report["schema"], "oev2.design-obligation-evidence/1");
    let status: std::collections::BTreeMap<String, String> = report["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["id"].as_str().unwrap().starts_with("ob_drawing_"))
        .map(|o| {
            (
                o["id"].as_str().unwrap().to_owned(),
                o["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let expected: std::collections::BTreeMap<String, String> = [
        ("ob_drawing_converter_identity", "held"),
        ("ob_drawing_faithful_page", "absent"),
        ("ob_drawing_no_invented_target", "absent"),
        ("ob_drawing_partial_intervals_on_lines", "held"),
        ("ob_drawing_recorded_run", "held"),
        ("ob_drawing_source_conservation", "held"),
        ("ob_drawing_source_only_worker", "held"),
        ("ob_drawing_structural_lines", "absent"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();
    assert_eq!(status, expected, "{report:#}");
    // Every absent obligation lacks exactly the browser reports, never another document.
    for o in report["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["status"] == "absent")
    {
        for e in o["executors"].as_array().unwrap() {
            for v in e["evidence"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|v| v["status"] == "absent")
            {
                assert_eq!(v["schema"], "oev2.drawing-review-page-check/1", "{o:#}");
            }
        }
    }
}
