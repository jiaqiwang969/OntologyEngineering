//! Source-only browser commands. No real browser, account, credential or model is used.
use oe_browser::{BackendKind, Observation, SessionIdentity};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = root.join(format!(
            "var/tests/unified-system/cli/browser-cli-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "Synthetic CLI root.\n").unwrap();
        Self(path)
    }
    fn put(&self, name: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn json(&self, name: &str, value: &Value) {
        self.put(name, serde_json::to_vec(value).unwrap());
    }
    fn sources(&self) {
        for (path, bytes) in oe_browser::source::SOURCE_FILES {
            self.put(path, bytes);
        }
    }
    fn task(&self) {
        self.json("var/input/task.json", &json!({"schema":"oev2.browser-task/3",
            "goal":"Inspect the supplied synthetic control.","url":"https://example.invalid/fixture",
            "text_values":{"Engineering note":"PRIVATE_EXACT_FIELD_VALUE"}}));
    }
    fn page(&self) {
        let page = Observation::new(SessionIdentity {
            backend: BackendKind::PeekabooBackground, target:"synthetic-target".into(),
            session:"synthetic-session".into(), origin:"https://example.invalid".into(), owns_target:true,
        }, json!({"url":"https://example.invalid/fixture", "title":"Synthetic", "text":"PRIVATE_OBSERVATION_TEXT",
            "marker":["fixture"],"page_key":["fixture-document"],
            "guards":{"1":["synthetic-button"]},"actions":[{"id":"select-synthetic", "kind":"click",
                "label":"Observed synthetic control", "node":1}]})).unwrap();
        self.json("var/input/page.json", &serde_json::to_value(page).unwrap());
    }
    fn backend(&self) {
        self.json(
            "var/input/backend.json",
            &json!({
            "schema":"ontology-engineering.native-browser-backend-config/v1",
            "backend":"peekaboo_background", "peekaboo_socket":self.0.join("var/absent-relay.sock")}),
        );
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
        command
            .arg("--root")
            .arg(&self.0)
            .current_dir(self.0.parent().unwrap())
            .env_clear()
            .env("PATH", "")
            .env("HOME", self.0.join("var/absent-home"));
        command
    }
    fn run(&self, args: &[&str], code: i32) -> Value {
        let output = self.command().args(args).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

#[test]
fn doctor_is_source_only_and_never_creates_state_or_claims_a_connection() {
    let c = Case::new();
    assert_eq!(c.run(&["browser", "doctor"], 2)["status"], "blocked");
    assert!(!c.0.join("var").exists());
    c.sources();
    let report = c.run(&["browser", "doctor"], 0);
    assert_eq!(report["status"], "native_adapter_ready");
    assert_eq!(report["result"]["browser_connection"], "not_checked");
    assert_eq!(report["result"]["live_task"], "not_run");
    assert_eq!(report["fact_admitted"], false);
    assert!(!c.0.join("var").exists());
    c.put(
        "runtime/browser-observer/upstream/snapshot.js",
        "changed observer source\n",
    );
    assert_eq!(c.run(&["browser", "doctor"], 2)["status"], "blocked");
    assert!(!c.0.join("var").exists());
}

#[test]
fn browser_argument_errors_are_json_and_do_not_echo_supplied_values() {
    let c = Case::new();
    for args in [
        vec!["browser", "--unknown", "PRIVATE_VALUE"],
        vec!["browser", "run", "--task", "PRIVATE_TASK"],
        vec!["browser", "prepare", "--task", "PRIVATE_TASK"],
    ] {
        let report = c.run(&args, 2);
        assert_eq!(report["error"]["code"], "invalid_arguments");
        assert!(!report.to_string().contains("PRIVATE_"));
    }
    let help = c
        .command()
        .args(["browser", "connection-check", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success() && help.stderr.is_empty());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("--backend-config")
    );
}
