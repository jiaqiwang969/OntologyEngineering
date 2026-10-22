//! Relocated CLI acceptance with no transport, interpreter or NX on PATH.
use oe_core::SkillRoot;
use oe_nx_protocol::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("var/maintenance/rust-tests")
            .join(format!(
                "nx-cli-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(directory.parent().unwrap()).unwrap();
        fs::create_dir(&directory).unwrap();
        let f = Self(directory);
        f.bytes("SKILL.md", b"# Isolated source-only CLI receiver\n");
        let executable = FileEntry::from_bytes("worker/native.exe", b"MZ never executed").unwrap();
        let source = FileEntry::from_bytes("sources/main.rs", b"// synthetic source\n").unwrap();
        let build =
            FileEntry::from_bytes("build/recipe.txt", b"fixture, no compilation\n").unwrap();
        for (file, bytes) in [
            (&executable, b"MZ never executed".as_slice()),
            (&source, b"// synthetic source\n".as_slice()),
            (&build, b"fixture, no compilation\n".as_slice()),
        ] {
            f.bytes(&format!("var/worker/{}", file.path.as_str()), bytes);
        }
        f.json(
            "var/worker/worker.json",
            &serde_json::to_value(WorkerManifest {
                schema: WORKER_SCHEMA.into(),
                worker_id: "synthetic-cli".into(),
                version: "fixture".into(),
                target: "x86_64-pc-windows-gnu".into(),
                executable,
                runtime_files: vec![],
                source_files: vec![source],
                build_recipe: build,
                toolchain: "no-compiler".into(),
                required_runtime: RuntimeIdentity {
                    product: "NX".into(),
                    product_label: "fixture".into(),
                    architecture: "x64".into(),
                    libraries: vec![FileEntry::from_bytes("NXBIN/libufun.dll", b"no SDK").unwrap()],
                },
                capabilities: vec![Capability {
                    action: "inspect".into(),
                    params_schema: "fixture.inspect/v1".into(),
                }],
            })
            .unwrap(),
        );
        f.json("var/profile.json", &json!({
            "schema":"cad-agent.nx-remote-profile/v2", "ssh_command":["fleet","ssh","fixture-only"],
            "remote_root":"%USERPROFILE%\\work\\fixture", "nx_root":"C:\\FixtureNX",
            "controller_path":"C:\\fixture\\controller.exe", "controller":BlobIdentity::of(b"no controller"),
            "expected_root":{"host_id":"fixture-host","volume_id":"fixture-volume","directory_id":"fixture-directory"},
            "legacy_review":null
        }));
        f.json(
            "var/parameters.json",
            &json!({"schema":"fixture.inspect/v1","part":"inputs/bracket.prt"}),
        );
        f.bytes("var/bracket.prt", b"source-only part bytes\0\xff");
        f.json(
            "var/request.json",
            &json!({
                "worker_directory":"var/worker", "parameter_file":"var/parameters.json",
                "inputs":[{"source":"var/bracket.prt","member":"inputs/bracket.prt"}],
                "job_id":"cli-inspect", "action":"inspect", "parameter_schema":"fixture.inspect/v1"
            }),
        );
        f
    }
    fn bytes(&self, name: &str, bytes: &[u8]) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn json(&self, name: &str, value: &Value) {
        self.bytes(name, &serde_json::to_vec_pretty(value).unwrap());
    }
    fn read(&self, name: &str) -> Value {
        serde_json::from_slice(&fs::read(self.0.join(name)).unwrap()).unwrap()
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
        command
            .env_clear()
            .env("PATH", "")
            .current_dir(self.0.parent().unwrap())
            .arg("--root")
            .arg(&self.0)
            .args(["cad", "nx"]);
        command
    }
    fn prepare(&self, output: &str) -> Output {
        self.command()
            .args([
                "prepare",
                "--profile",
                "var/profile.json",
                "--request",
                "var/request.json",
                "--output",
                output,
            ])
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn rejected(output: &Output) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    // The v2 CAD boundary retains the safe failure envelope used by the other
    // source-bound execution commands. Rejection and no-publication assertions
    // below remain required for every original malformed-input case.
    let failure: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure["schema"], "oev2.command-failure/1");
    assert_eq!(failure["command"], "cad");
    assert_eq!(failure["execution"], "failed");
    assert_eq!(failure["conclusion"], "unknown");
    assert_eq!(failure["engineering_adoption"], "blocked");
    assert_eq!(failure["error"]["code"], "invalid_contract");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("PRIVATE_TEST_VALUE"));
}

#[test]
fn prepare_in_relocated_root_preserves_original_members_without_execution() {
    let f = Fixture::new();
    let output = f.prepare("var/prepared");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["native_execution"], false);
    assert_eq!(result["job_id"], "cli-inspect");
    let package = oe_cad::nx::from_directory(
        &SkillRoot::new(&f.0).unwrap(),
        &PathBuf::from("var/prepared/job"),
    )
    .unwrap();
    assert_eq!(
        package.job.digest().unwrap().as_str(),
        result["manifest_digest"]
    );
    assert_eq!(
        package.members["inputs/bracket.prt"],
        fs::read(f.0.join("var/bracket.prt")).unwrap()
    );
    let prepared = f.read("var/prepared/prepared.json");
    assert_eq!(prepared["engineering_acceptance"], "not_evaluated");
    rejected(&f.prepare("var/prepared"));
    assert_eq!(prepared, f.read("var/prepared/prepared.json"));
}

#[test]
fn unresolved_remote_identity_cannot_be_prepared() {
    let f = Fixture::new();
    let mut profile = f.read("var/profile.json");
    profile["expected_root"] = Value::Null;
    f.json("var/profile.json", &profile);
    rejected(&f.prepare("var/prepared"));
    assert!(!f.0.join("var/prepared").exists());
}

#[test]
fn strict_request_and_changed_source_fail_before_publication() {
    let f = Fixture::new();
    let original = f.read("var/request.json");
    let mut extra = original.clone();
    extra["automatic_retry"] = json!(true);
    f.json("var/request.json", &extra);
    rejected(&f.prepare("var/prepared"));
    f.bytes(
        "var/request.json",
        br#"{"private":"PRIVATE_TEST_VALUE","private":"duplicate"}"#,
    );
    rejected(&f.prepare("var/prepared"));
    f.json("var/request.json", &original);
    f.bytes("var/worker/sources/main.rs", b"changed source");
    rejected(&f.prepare("var/prepared"));
    assert!(!f.0.join("var/prepared").exists());
}

#[test]
fn output_and_profile_paths_cannot_escape_selected_root() {
    let f = Fixture::new();
    rejected(&f.prepare("prepared-outside-var"));
    assert!(!f.0.join("prepared-outside-var").exists());
    let outside = f.0.parent().unwrap().join("PRIVATE_TEST_VALUE.json");
    let output = f
        .command()
        .args(["prepare", "--profile"])
        .arg(outside)
        .args(["--request", "var/request.json", "--output", "var/prepared"])
        .output()
        .unwrap();
    rejected(&output);
    assert!(!f.0.join("var/prepared").exists());
}

#[test]
fn run_rejects_invalid_wait_budget_before_remote_work() {
    let f = Fixture::new();
    for wait in ["0", "301"] {
        let output = f
            .command()
            .args([
                "run",
                "--profile",
                "var/profile.json",
                "--job-directory",
                "var/absent",
                "--output",
                "var/run",
                "--wait-seconds",
                wait,
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!f.0.join("var/run").exists());
    }
}

#[cfg(unix)]
#[test]
fn symlinked_profile_is_not_followed() {
    let f = Fixture::new();
    fs::rename(
        f.0.join("var/profile.json"),
        f.0.join("var/real-profile.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink("real-profile.json", f.0.join("var/profile.json")).unwrap();
    rejected(&f.prepare("var/prepared"));
    assert!(!f.0.join("var/prepared").exists());
}

#[test]
fn dispatch_writer_review_requires_explicit_pin_and_root_before_preparation() {
    for mutation in ["no_digest", "no_root", "retry_claim", "control_character"] {
        let f = Fixture::new();
        let mut profile = f.read("var/profile.json");
        profile["dispatch_writer_review"] = json!({
            "remote_path":"C:/review/control.json", "sha256":oe_nx_protocol::Digest::of(b"review")
        });
        match mutation {
            "no_digest" => {
                profile["dispatch_writer_review"]
                    .as_object_mut()
                    .unwrap()
                    .remove("sha256");
            }
            "no_root" => {
                profile["expected_root"] = Value::Null;
            }
            "retry_claim" => {
                profile["dispatch_writer_review"]["allow_retry"] = json!(true);
            }
            "control_character" => {
                profile["dispatch_writer_review"]["remote_path"] = json!("C:/review\ncontrol.json");
            }
            _ => unreachable!(),
        }
        f.json("var/profile.json", &profile);
        rejected(&f.prepare("var/prepared"));
        assert!(!f.0.join("var/prepared").exists());
    }
}

#[test]
fn worker_verify_needs_no_profile_parameters_output_or_program_on_path() {
    let f = Fixture::new();
    for name in [
        "var/profile.json",
        "var/parameters.json",
        "var/bracket.prt",
        "var/request.json",
    ] {
        fs::remove_file(f.0.join(name)).unwrap();
    }
    let original = fs::read(f.0.join("var/worker/worker.json")).unwrap();
    let output = f
        .command()
        .args(["worker", "verify", "--worker-directory", "var/worker"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "cad-agent.nx-worker-verification/v2");
    assert_eq!(report["status"], "package_complete");
    assert_eq!(report["execution_performed"], false);
    for field in [
        "compilation_provenance",
        "native_acceptance",
        "engineering_acceptance",
        "job_preparation",
    ] {
        assert_eq!(report[field], "not_evaluated");
    }
    assert_eq!(report["required_runtime_availability"], "not_observed");
    let manifest: WorkerManifest = decode(&original).unwrap();
    assert_eq!(
        report["worker_manifest_digest"],
        manifest.digest().unwrap().as_str()
    );
    assert_eq!(
        report["worker_manifest_file"],
        serde_json::to_value(BlobIdentity::of(&original)).unwrap()
    );
    assert_eq!(
        fs::read(f.0.join("var/worker/worker.json")).unwrap(),
        original
    );
    assert_eq!(fs::read_dir(f.0.join("var")).unwrap().count(), 1);
}

#[test]
fn worker_verify_rejects_missing_changed_extra_and_outside_packages_without_outputs() {
    let f = Fixture::new();
    let verify = |directory: &str| {
        f.command()
            .args(["worker", "verify", "--worker-directory", directory])
            .output()
            .unwrap()
    };
    rejected(&verify("var/absent"));
    rejected(&verify("/PRIVATE_TEST_VALUE/outside"));
    let path = f.0.join("var/worker/sources/main.rs");
    let original = fs::read(&path).unwrap();
    fs::write(&path, b"changed").unwrap();
    rejected(&verify("var/worker"));
    rejected(&f.prepare("var/prepared"));
    fs::write(&path, original).unwrap();
    f.bytes("var/worker/extra", b"unlisted");
    rejected(&verify("var/worker"));
    assert!(!f.0.join("var/prepared").exists());
}
