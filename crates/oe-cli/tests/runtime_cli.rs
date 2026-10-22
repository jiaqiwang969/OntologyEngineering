//! Real Git source inspection in isolated roots; no real install, npm or pip.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn installed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_owned()
}
fn write(path: &Path, raw: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, raw).unwrap();
}
struct Case {
    path: PathBuf,
    git: PathBuf,
}
impl Case {
    fn new() -> Self {
        let path = installed().join(format!(
            "var/maintenance/rust-tests/runtime-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        write(&path.join("SKILL.md"), "synthetic runtime CLI receiver\n");
        Self {
            path,
            git: oe_core::process::resolve_executable(Path::new("git")).unwrap(),
        }
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_oev2"));
        c.current_dir(&self.path)
            .arg("--root")
            .arg(&self.path)
            .env_clear()
            .env("PATH", "")
            .env("HOME", self.path.join("var/empty-home"));
        c
    }
    fn run(&self, args: &[&str], code: i32) -> Value {
        let o = self.command().args(args).output().unwrap();
        assert_eq!(
            o.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&o.stdout)
        );
        assert!(
            o.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    }
    fn git(&self, args: &[&str]) -> String {
        let o = Command::new(&self.git)
            .current_dir(self.path.join("var/upstream source"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .args(args)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap()
    }
    fn source(&self) -> Value {
        let source = self.path.join("var/upstream source");
        write(
            &source.join("pyproject.toml"),
            "[project]\nname='fixture'\n",
        );
        write(&source.join("explorer/package-lock.json"), "{}\n");
        self.git(&["init", "--quiet"]);
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", "isolated fixture"]);
        let mut lock: Value = serde_json::from_slice(
            &fs::read(installed().join("runtime/semantica-source-lock.json")).unwrap(),
        )
        .unwrap();
        lock["source"]["commit"] = self.git(&["rev-parse", "HEAD"]).trim().into();
        lock["locked_inputs"]["pyproject_sha256"] =
            oe_core::sha256(fs::read(source.join("pyproject.toml")).unwrap()).into();
        lock["locked_inputs"]["explorer_package_lock_sha256"] =
            oe_core::sha256(fs::read(source.join("explorer/package-lock.json")).unwrap()).into();
        write(
            &self.path.join("runtime/semantica-source-lock.json"),
            serde_json::to_vec(&lock).unwrap(),
        );
        lock
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}

#[test]
fn inspect_source_uses_real_explicit_git_and_retains_complete_report_without_runtime_install() {
    let c = Case::new();
    let lock = c.source();
    let original = fs::read(c.path.join("runtime/semantica-source-lock.json")).unwrap();
    let result = c.run(
        &[
            "runtime",
            "inspect-source",
            "--source-root",
            "var/upstream source",
            "--output",
            "var/inspection",
            "--git",
            c.git.to_str().unwrap(),
            "--npm-offline",
        ],
        0,
    );
    assert_eq!(result["action"], "inspect");
    assert_eq!(result["status"], "ready");
    assert_eq!(result["result"]["head"], lock["source"]["commit"]);
    assert_eq!(result["result"]["full_fsck_executed"], true);
    assert!(!result["steps"].as_array().unwrap().is_empty());
    let saved: Value =
        serde_json::from_slice(&fs::read(c.path.join("var/inspection/build-report.json")).unwrap())
            .unwrap();
    assert_eq!(result, saved);
    assert_eq!(result["network_policy"]["npm"], "offline_cache_only");
    assert!(!c.path.join("runtime/.venv").exists());
    assert_eq!(
        fs::read(c.path.join("runtime/semantica-source-lock.json")).unwrap(),
        original
    );
    c.run(
        &[
            "runtime",
            "inspect-source",
            "--source-root",
            "var/upstream source",
            "--output",
            "var/inspection",
            "--git",
            c.git.to_str().unwrap(),
        ],
        2,
    );
}

#[test]
fn dirty_source_and_missing_build_tool_are_blocked_with_actual_steps_and_no_install() {
    let c = Case::new();
    c.source();
    let result = c.run(
        &[
            "runtime",
            "build-source",
            "--source-root",
            "var/upstream source",
            "--output",
            "var/build-attempt",
            "--git",
            c.git.to_str().unwrap(),
            "--node",
            "/missing-native-node",
            "--npm-offline",
        ],
        1,
    );
    assert_eq!(result["status"], "blocked");
    assert_eq!(result["action"], "build");
    assert!(!result["steps"].as_array().unwrap().is_empty());
    assert_eq!(result["installed_runtime_mutated_by_builder"], false);
    assert!(!c.path.join("runtime/.venv").exists());
    write(
        &c.path.join("var/upstream source/pyproject.toml"),
        "changed\n",
    );
    let result = c.run(
        &[
            "runtime",
            "inspect-source",
            "--source-root",
            "var/upstream source",
            "--output",
            "var/dirty",
            "--git",
            c.git.to_str().unwrap(),
        ],
        1,
    );
    assert_eq!(result["status"], "blocked");
    assert!(result["error"].as_str().unwrap().contains("dirty"));
}

#[test]
fn runtime_requires_explicit_evidence_output_and_has_no_arbitrary_install_target() {
    let c = Case::new();
    for args in [
        vec!["runtime", "preflight"],
        vec![
            "runtime",
            "install",
            "--output",
            "var/install",
            "--target",
            "PRIVATE_EXTERNAL_TARGET",
        ],
        vec!["runtime", "build-source", "--source-root", "PRIVATE_SOURCE"],
    ] {
        let v = c.run(&args, 2);
        assert_eq!(v["error"]["code"], "invalid_arguments");
        assert!(!v.to_string().contains("PRIVATE_"));
    }
    assert!(!c.path.join("runtime/.venv").exists());
    assert!(!c.path.join("var/install").exists());
}

#[test]
fn installer_preflight_missing_input_reports_failure_without_creating_a_runtime() {
    let c = Case::new();
    for command in ["preflight", "install"] {
        let out = c
            .command()
            .args([
                "runtime",
                command,
                "--python",
                "/missing-python",
                "--output",
                &format!("var/{command}"),
            ])
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stderr.is_empty());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["status"], "blocked");
    }
    assert!(!c.path.join("runtime/.venv").exists());
}

#[test]
fn runtime_help_keeps_new_commands_and_separate_network_controls() {
    let c = Case::new();
    for command in ["install", "build-source"] {
        let o = c
            .command()
            .args(["runtime", command, "--help"])
            .output()
            .unwrap();
        assert!(o.status.success());
        let text = String::from_utf8(o.stdout).unwrap();
        assert!(text.contains("--output") && text.contains("--wheelhouse"));
        assert!(text.contains(if command == "install" {
            "--offline"
        } else {
            "--npm-offline"
        }));
    }
}
