//! Offline CLI contracts for observed supplier choices and exact file snapshots.
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
            "var/tests/unified-system/cli/supplier-cli-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        fs::write(path.join("SKILL.md"), "Synthetic supplier CLI root.\n").unwrap();
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
        for (path, bytes) in oe_cad::supplier::source::SOURCE_FILES {
            self.put(path, bytes);
        }
    }
    fn inputs(&self) {
        self.json("var/input/context.json", &context());
        self.json("var/input/observation.json", &observation());
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
        let out = self.command().args(args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn context() -> Value {
    json!({"schema":"cad-agent.supplier-acquisition-context/v1","goal":"Check the interface before choosing a native model",
    "background":"Synthetic project with unknown clearance","object_identity":{"part":"P1"},
    "constraints":["Do not order"], "needs":[{"id":"drawing","purpose":"Read interface dimensions","acceptance":"Source-bound drawing and later engineering review"}]})
}
fn observation() -> Value {
    json!({"part":"P1","source_url":"https://www.misumi.com.cn/observed","observation_id":"synthetic-observation",
    "candidates":[{"id":"drawing","kind":"document","label":"Observed drawing","url":"https://www.misumi.com.cn/observed/drawing.pdf"},
        {"id":"native","kind":"cad_format","label":"PARASOLID","value":"4"}]})
}
fn step(part: &str) -> String {
    format!(
        "ISO-10303-21;\nHEADER;FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;#1=PRODUCT('{part}','','',());#2=SI_UNIT(.MILLI.,.METRE.);ENDSEC;END-ISO-10303-21;\n"
    )
}

#[test]
fn offline_file_snapshot_keeps_actual_bytes_and_never_claims_native_units_or_geometry() {
    let c = Case::new();
    let original = step("P1");
    c.put("var/input/P1.stp", &original);
    let args = [
        "supplier",
        "inspect",
        "--file",
        "var/input/P1.stp",
        "--part",
        "P1",
        "--format",
        "STEP AP214",
        "--output",
        "var/inspected",
    ];
    let report = c.run(&args, 0);
    assert_eq!(report["status"], "inspected");
    assert_eq!(report["result"]["fresh_download_observed"], false);
    assert_eq!(report["result"]["artifact"]["files"][0]["units"], "unknown");
    assert_eq!(
        report["result"]["artifact"]["files"][0]["millimetre_unit_definition_present"],
        true
    );
    for field in ["native_import", "native_save", "independent_reopen"] {
        assert_eq!(report["result"]["native_handoff"][field], "not_run");
    }
    assert_eq!(
        fs::read(c.0.join("var/inspected/source/P1.stp")).unwrap(),
        original.as_bytes()
    );
    assert_eq!(
        fs::read(c.0.join("var/inspected/members/P1.stp")).unwrap(),
        original.as_bytes()
    );
    assert_eq!(
        fs::read(c.0.join("var/input/P1.stp")).unwrap(),
        original.as_bytes()
    );
    let before = fs::read(c.0.join("var/inspected/receipt.json")).unwrap();
    assert_eq!(c.run(&args, 2)["status"], "blocked");
    assert_eq!(
        fs::read(c.0.join("var/inspected/receipt.json")).unwrap(),
        before
    );
    c.put(
        "var/input/P1.prt",
        "synthetic native file requiring its own reader",
    );
    let nx = c.run(
        &[
            "supplier",
            "inspect",
            "--file",
            "var/input/P1.prt",
            "--part",
            "P1",
            "--format",
            "NX",
            "--output",
            "var/native",
        ],
        0,
    );
    assert_eq!(
        nx["result"]["artifact"]["files"][0]["format_check"],
        "native_reader_required"
    );
    assert!(!c.0.join("runtime").exists());
}

#[test]
fn bad_artifact_identity_html_and_external_output_leave_inputs_unchanged() {
    let c = Case::new();
    for bytes in [
        step("OTHER"),
        "<html>not a STEP file</html>".into(),
        "ISO-10303-21;truncated".into(),
    ] {
        c.put("var/input/P1.stp", &bytes);
        let report = c.run(
            &[
                "supplier",
                "inspect",
                "--file",
                "var/input/P1.stp",
                "--part",
                "P1",
                "--format",
                "STEP",
                "--output",
                "var/invalid",
            ],
            2,
        );
        assert_eq!(report["status"], "blocked");
        assert!(!c.0.join("var/invalid").exists());
        assert_eq!(
            fs::read(c.0.join("var/input/P1.stp")).unwrap(),
            bytes.as_bytes()
        );
    }
    c.put("var/input/P1.stp", step("P1"));
    assert_eq!(
        c.run(
            &[
                "supplier",
                "inspect",
                "--file",
                "var/input/P1.stp",
                "--part",
                "P1",
                "--format",
                "STEP",
                "--output",
                "references/not-private"
            ],
            2
        )["status"],
        "blocked"
    );
    assert!(!c.0.join("references/not-private").exists());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            c.0.join("var/input/P1.stp"),
            c.0.join("var/input/P1_alias.stp"),
        )
        .unwrap();
        assert_eq!(
            c.run(
                &[
                    "supplier",
                    "inspect",
                    "--file",
                    "var/input/P1_alias.stp",
                    "--part",
                    "P1",
                    "--format",
                    "STEP",
                    "--output",
                    "var/link"
                ],
                2
            )["status"],
            "blocked"
        );
        assert!(!c.0.join("var/link").exists());
    }
}

#[test]
fn public_archive_url_rejection_precedes_network_and_output() {
    let c = Case::new();
    c.sources();
    for url in [
        "http://www.misumi.com.cn/linked/material/a.zip",
        "https://example.invalid/a.zip",
        "https://www.misumi.com.cn/unobserved/a.zip",
        "https://www.misumi.com.cn/linked/material/a.zip?secret=PRIVATE_VALUE",
    ] {
        let report = c.run(
            &[
                "supplier",
                "archive",
                "--url",
                url,
                "--observed-from",
                "synthetic visible link",
                "--output",
                "var/archive",
            ],
            2,
        );
        assert_eq!(report["status"], "blocked");
        assert!(!report.to_string().contains("PRIVATE_VALUE"));
        assert!(!c.0.join("var/archive").exists());
    }
}

#[test]
fn supplier_argument_errors_are_structured_and_never_echo_supplied_values() {
    let c = Case::new();
    for args in [
        vec!["supplier", "inspect", "--file", "PRIVATE_PATH"],
        vec!["supplier", "choice", "--unknown", "PRIVATE_VALUE"],
        vec!["supplier", "archive", "--url", "PRIVATE_URL"],
    ] {
        let report = c.run(&args, 2);
        assert_eq!(report["error"]["code"], "invalid_arguments");
        assert!(!report.to_string().contains("PRIVATE_"));
    }
}

fn existing_tab_inputs(c: &Case) {
    c.json("var/input/task.json", &json!({"schema":"oev2.browser-task/3",
        "goal":"Read a synthetic existing supplier tab", "url":"https://www.misumi.com.cn/my/user/manage/",
        "backend":"peekaboo_background","tab_id":11,"keep_open":true,
        "allowed_actions":[{"kind":"click","label":"Explicit synthetic scope"}]}));
    c.json(
        "var/input/backend.json",
        &json!({"schema":"ontology-engineering.native-browser-backend-config/v1",
        "backend":"peekaboo_background","peekaboo_socket":c.0.join("var/absent-relay.sock")}),
    );
}

#[cfg(unix)]
#[test]
fn session_check_keeps_private_identity_out_of_the_failed_connection_record() {
    use std::os::unix::fs::PermissionsExt;
    let c = Case::new();
    c.sources();
    existing_tab_inputs(&c);
    c.json(
        "var/input/account.json",
        &json!({"schema":"cad-agent.misumi-account-identity/v1",
        "match_fields":{"username":"synthetic_private_account_value"}}),
    );
    fs::set_permissions(
        c.0.join("var/input/account.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let report = c.run(
        &[
            "supplier",
            "session-check",
            "--task",
            "var/input/task.json",
            "--backend-config",
            "var/input/backend.json",
            "--identity",
            "var/input/account.json",
            "--output",
            "var/session-check",
            "--max-seconds",
            "1",
        ],
        1,
    );
    assert_eq!(report["status"], "browser_connection_required");
    assert_eq!(report["result"]["account_verified"], false);
    assert_eq!(report["result"]["model_calls"], 0);
    assert!(
        !report
            .to_string()
            .contains("synthetic_private_account_value")
    );
    let saved = fs::read_to_string(c.0.join("var/session-check/command-result.json")).unwrap();
    assert!(!saved.contains("synthetic_private_account_value"));
    assert!(!c.0.join("var/session-check/session").exists());
    assert!(!c.0.join("var/absent-relay.sock").exists());
}

#[test]
fn historical_ap203_transfer_is_explicit_and_rejects_unobserved_or_changed_url_paths() {
    let c = Case::new();
    c.sources();
    for url in [
        "https://example.invalid/P1_STEP.zip",
        "https://rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/STEP_AP203/OTHER_STEP.zip",
        "https://rd-cn-caddata.misumi.com.cn/prod/CadSupplyGC/STEP_AP203/P1_STEP.zip?PRIVATE_VALUE",
    ] {
        let report = c.run(
            &[
                "supplier",
                "download-observed-ap203",
                "--url",
                url,
                "--part",
                "P1",
                "--output",
                "var/historical",
            ],
            2,
        );
        assert_eq!(report["status"], "blocked");
        assert!(!report.to_string().contains("PRIVATE_VALUE"));
        assert!(!c.0.join("var/historical").exists());
    }
}
