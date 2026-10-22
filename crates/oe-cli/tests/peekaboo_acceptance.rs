//! Independent acceptance counterexamples for the current browser policy.
//! All pages, account receipts, protocol packets and executable output here are
//! synthetic. These tests neither connect to Chrome nor establish live focus,
//! supplier-account, download, CAD or engineering acceptance.
use oe_browser::{BackendKind, BrowserTask, SessionIdentity, backend, source};
use oe_cad::supplier::{artifacts, session};
use oe_core::SkillRoot;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let path = workspace.join(format!(
            "var/maintenance/independent-peekaboo-tests/{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        oe_core::write_new_bytes(
            &path.join("SKILL.md"),
            b"Synthetic independent policy fixture; no live browser or account.\n",
        )
        .unwrap();
        for (name, bytes) in source::SOURCE_FILES
            .iter()
            .chain(oe_cad::supplier::source::SOURCE_FILES.iter())
        {
            let file = path.join(name);
            oe_core::ensure_directory(file.parent().unwrap()).unwrap();
            if file.exists() {
                assert_eq!(fs::read(&file).unwrap(), *bytes, "shared source differs");
            } else {
                oe_core::write_new_bytes(&file, bytes).unwrap();
            }
        }
        Self(path)
    }
    fn root(&self) -> SkillRoot {
        SkillRoot::new(&self.0).unwrap()
    }
    fn json(&self, name: &str, value: &Value) {
        let file = self.0.join(name);
        oe_core::ensure_directory(file.parent().unwrap()).unwrap();
        oe_core::write_new(&file, value).unwrap();
    }
    #[cfg(unix)]
    fn executable(&self, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let file = self.0.join(name);
        oe_core::ensure_directory(file.parent().unwrap()).unwrap();
        oe_core::write_new_bytes(&file, body.as_bytes()).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
        file
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn task(kind: &str) -> BrowserTask {
    let mut value = json!({
        "schema":"oev2.browser-task/3",
        "goal":"Read this explicitly scoped supplier page without changing user focus",
        "url":"https://www.misumi.com.cn/observed/P1",
        "backend":kind,
        "max_seconds":2,
    });
    if kind == "chrome_apple_events" {
        value["tab_id"] = json!(71);
        value["keep_open"] = json!(true);
        value["allowed_actions"] = json!([{"kind":"click","label":"Observed CAD control"}]);
    }
    BrowserTask::parse(value).unwrap()
}

#[cfg(unix)]
#[test]
fn old_live_backends_reject_before_any_owned_transport_or_script_launch() {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};
    let case = Fixture::new();
    let root = case.root();
    let observed = source::snapshot(&root).unwrap();
    let marker = case.0.join("forbidden-process-ran");
    let forbidden = case.executable(
        "var/forbidden-executable.sh",
        &format!(
            "#!/bin/sh\nprintf invoked > {}\nexit 91\n",
            shell_quote(marker.to_str().unwrap())
        ),
    );
    let socket_directory = tempfile::Builder::new()
        .prefix("pb-policy-")
        .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    let socket = socket_directory.path().join("transport.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    listener.set_nonblocking(true).unwrap();

    let current_default = BrowserTask::parse(json!({
        "schema":"oev2.browser-task/3", "goal":"Read only",
        "url":"https://www.misumi.com.cn/observed/P1"
    }))
    .unwrap();
    assert_eq!(current_default.backend, BackendKind::PeekabooBackground);
    for kind in ["chrome_apple_events", "browser_harness"] {
        // The old input remains parseable as a historical record. Its presence
        // in current task JSON does not supply new execution authorization.
        let old = task(kind);
        let output = PathBuf::from(format!("var/legacy-{kind}"));
        let mut config = backend::BackendConfig::new(output.clone());
        config.osascript = forbidden.clone();
        config.peekaboo_executable = forbidden.clone();
        config.node_executable = forbidden.clone();
        config.harness_socket = Some(socket.clone());
        config.peekaboo_socket = Some(socket.clone());
        let error = backend::connect(&root, &old, &observed, &config)
            .err()
            .expect("a historical backend must not connect");
        assert!(
            error
                .to_string()
                .contains("current_browser_policy_requires_peekaboo_background")
        );
        assert!(
            !case.0.join(output).exists(),
            "rejected connection created owned output"
        );
        assert!(
            !marker.exists(),
            "rejected connection launched an executable"
        );
        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "rejected connection dispatched to a configured transport"
        );
    }
}

fn protocol_packet() -> Value {
    let lock: Value = serde_json::from_slice(include_bytes!(
        "../../../runtime/peekaboo/provider-lock.json"
    ))
    .unwrap();
    json!({
        "schema":"oev2.peekaboo-bridge/1", "success":true, "status":"ready",
        "bridge_sha256":oe_core::sha256(include_bytes!("../../../runtime/peekaboo/bridge.mjs")),
        "focus_hook_sha256":oe_core::sha256(include_bytes!("../../../runtime/peekaboo/focus-policy.mjs")),
        "peekaboo_binary_sha256":lock["peekaboo_binary_sha256"],
        "chrome_devtools_source_sha256":lock["chrome_devtools_source_sha256"],
        "provider_pin_applied":true,
        "chrome_launches":0, "settings_changes":0, "focus_actions":0,
        "storage_reads":0, "request_retries":0,
        "worker_session":"synthetic-worker-A", "worker_pid":101, "chrome_pid":102,
        "chrome_pid_before":102, "chrome_pid_after":102,
        "focus_before":{"bundle":"com.example.editor","pid":103},
        "focus_after":{"bundle":"com.example.editor","pid":103},
        "active_application_unchanged":true,
        "result":{"connection_verified":true,"real_operation":"list_pages"},
    })
}

#[test]
fn provider_identity_foreground_and_unconfirmed_operation_refuse_without_fallback() {
    let original = protocol_packet();
    // Accepted metadata is still only a pure protocol fixture, not a live check.
    backend::validate_peekaboo_packet(&original).unwrap();
    let mut counterexamples = Vec::new();
    for field in [
        "bridge_sha256",
        "focus_hook_sha256",
        "peekaboo_binary_sha256",
        "chrome_devtools_source_sha256",
    ] {
        let mut changed = original.clone();
        changed[field] = json!("0".repeat(64));
        counterexamples.push((field, changed));
    }
    for field in [
        "chrome_launches",
        "settings_changes",
        "focus_actions",
        "storage_reads",
        "request_retries",
    ] {
        let mut changed = original.clone();
        changed[field] = json!(1);
        counterexamples.push((field, changed));
    }
    for field in [
        "success",
        "provider_pin_applied",
        "active_application_unchanged",
    ] {
        let mut changed = original.clone();
        changed[field] = json!(false);
        counterexamples.push((field, changed));
    }
    for field in ["worker_pid", "chrome_pid"] {
        let mut changed = original.clone();
        changed[field] = json!(0);
        counterexamples.push((field, changed));
    }
    for field in ["chrome_pid_before", "chrome_pid_after"] {
        let mut changed = original.clone();
        changed[field] = json!(105);
        counterexamples.push((field, changed));
    }
    let mut started_only = original.clone();
    started_only["status"] = json!("worker_started");
    counterexamples.push(("MCP-startup-is-not-ready", started_only));
    let mut different_application = original.clone();
    different_application["focus_after"]["bundle"] = json!("com.example.other");
    counterexamples.push((
        "foreground-application-changed-despite-true-flag",
        different_application,
    ));
    let mut different_process = original.clone();
    different_process["focus_after"]["pid"] = json!(104);
    counterexamples.push((
        "foreground-process-changed-despite-true-flag",
        different_process,
    ));
    for (scenario, packet) in counterexamples {
        assert!(
            backend::validate_peekaboo_packet(&packet).is_err(),
            "{scenario}"
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::{fs::PermissionsExt, net::UnixListener};
        let case = Fixture::new();
        let root = case.root();
        let observed = source::snapshot(&root).unwrap();
        let marker = case.0.join("endpoint-discovery-or-fallback-ran");
        let forbidden = case.executable(
            "var/forbidden-profile-discovery.sh",
            &format!(
                "#!/bin/sh\nprintf invoked > {}\nexit 91\n",
                shell_quote(marker.to_str().unwrap())
            ),
        );
        let socket_directory = tempfile::Builder::new()
            .prefix("pb-no-profile-")
            .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
            .unwrap();
        let socket = socket_directory.path().join("transport.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut config = backend::BackendConfig::new(PathBuf::from("var/endpoint-required"));
        config.peekaboo_socket = Some(socket);
        config.osascript = forbidden.clone();
        config.peekaboo_executable = forbidden.clone();
        config.node_executable = forbidden;
        config.timeout = Duration::from_secs(2);
        assert!(config.cdp_endpoint.is_none());
        let error = backend::connect(&root, &task("peekaboo_background"), &observed, &config)
            .err()
            .expect("DOM transport must require an explicit existing endpoint");
        assert!(
            error
                .to_string()
                .contains("explicit_existing_cdp_endpoint_required_without_profile_discovery")
        );
        assert!(
            !marker.exists(),
            "missing endpoint launched a discovery or fallback process"
        );
        assert!(
            matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
            "missing endpoint dispatched to a worker that could discover a profile"
        );

        // A browser profile file or a remote host cannot be substituted for an
        // explicit existing local CDP endpoint. The fixture never reads a real
        // profile and never invokes the default AX observation/check owner.
        for (index, endpoint) in [
            "file:///synthetic-profile/DevToolsActivePort",
            "http://example.invalid:9222/",
            "http://user@127.0.0.1:9222/",
            "http://127.0.0.1:9222/?profile=synthetic",
        ]
        .iter()
        .enumerate()
        {
            config.output = PathBuf::from(format!("var/invalid-endpoint-{index}"));
            config.cdp_endpoint = Some((*endpoint).into());
            assert!(
                backend::connect(&root, &task("peekaboo_background"), &observed, &config).is_err(),
                "unacceptable endpoint was admitted: {endpoint}"
            );
            assert!(
                !marker.exists(),
                "unacceptable endpoint launched discovery or fallback"
            );
            assert!(
                matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "unacceptable endpoint dispatched to a worker"
            );
        }
    }
}

#[test]
fn old_backend_session_receipts_and_different_current_sessions_are_not_current_account_authority() {
    let case = Fixture::new();
    let root = case.root();
    let current = SessionIdentity {
        backend: BackendKind::PeekabooBackground,
        target: "71".into(),
        session: "peekaboo-background:synthetic-worker-A".into(),
        origin: session::ORIGIN.into(),
        owns_target: false,
    };
    let receipt = json!({
        "schema":"cad-agent.misumi-session-check/v2", "status":"session_ready",
        "account_verified":true, "session":current,
        "checked_at_unix_ms":session::unix_ms().unwrap(),
        "source_identity":oe_cad::supplier::source::identity(&root).unwrap(),
    });
    case.json("var/account/exact.json", &receipt);
    let exact = session::validate_receipt(&root, Path::new("var/account/exact.json"), &current, 60)
        .unwrap();
    assert_eq!(exact["status"], "prior_normal_page_check_bound");
    assert_eq!(exact["password_reauthentication_verified"], false);
    assert!(
        exact["scope"]
            .as_str()
            .unwrap()
            .contains("not fresh account-page reobservation")
    );
    for (index, field, replacement) in [
        (0, "backend", json!("chrome_apple_events")),
        (1, "backend", json!("browser_harness")),
        (
            2,
            "session",
            json!("peekaboo-background:synthetic-worker-B"),
        ),
        (3, "target", json!("72")),
        (4, "origin", json!("https://elsewhere.example")),
        (5, "owns_target", json!(true)),
    ] {
        let mut stale = receipt.clone();
        stale["session"][field] = replacement;
        let file = format!("var/account/stale-{index}.json");
        case.json(&file, &stale);
        let error = session::validate_receipt(&root, Path::new(&file), &current, 60).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("account_receipt_session_mismatch"),
            "{field}: {error}"
        );
    }
    // A source edit in this private fixture cannot preserve account authority
    // simply by keeping the task/session strings equal.
    let bridge = case.0.join("runtime/peekaboo/bridge.mjs");
    let mut changed = fs::read(&bridge).unwrap();
    changed.extend_from_slice(b"\n// synthetic changed source\n");
    fs::write(bridge, changed).unwrap();
    assert!(
        session::validate_receipt(&root, Path::new("var/account/exact.json"), &current, 60)
            .is_err()
    );
}

#[cfg(target_os = "macos")]
#[test]
fn focus_probe_uses_only_peekaboo_app_listing_and_keeps_missing_window_identity_unknown() {
    let case = Fixture::new();
    let root = case.root();
    let forbidden_marker = case.0.join("osascript-ran");
    let forbidden = case.executable(
        "var/forbidden-apple.sh",
        &format!(
            "#!/bin/sh\nprintf invoked > {}\nexit 91\n",
            shell_quote(forbidden_marker.to_str().unwrap())
        ),
    );
    let raw = json!({"success":true,"data":{"applications":[
        {"isActive":false,"bundleIdentifier":"com.google.Chrome","processIdentifier":11},
        {"isActive":true,"bundleIdentifier":"com.example.editor","processIdentifier":12},
    ]}});
    let arguments = case.0.join("peekaboo-arguments");
    let peekaboo_fixture = case.executable(
        "var/controlled-peekaboo.sh",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nprintf '%s\\n' {}\n",
            shell_quote(arguments.to_str().unwrap()),
            shell_quote(&serde_json::to_string(&raw).unwrap()),
        ),
    );
    let mut config = backend::BackendConfig::new(PathBuf::from("var/focus-sample"));
    config.osascript = forbidden;
    config.peekaboo_executable = peekaboo_fixture;
    config.timeout = Duration::from_secs(2);
    let observed = backend::FocusProbe::new(&root, &config)
        .unwrap()
        .sample()
        .unwrap();
    assert_eq!(
        fs::read_to_string(&arguments).unwrap(),
        "list\napps\n--json\n--no-remote\n"
    );
    assert!(!forbidden_marker.exists());
    assert_eq!(observed["app"], "com.example.editor");
    assert_eq!(observed["pid"], 12);
    assert_eq!(observed["observer"], "peekaboo_list_apps");
    assert_eq!(observed["foreground_window_tab_identity"], "not_observed");
    assert!(observed.get("chrome_window_id").is_none() && observed.get("chrome_tab_id").is_none());

    let mut ambiguous = raw.clone();
    ambiguous["data"]["applications"][0]["isActive"] = json!(true);
    let invalid_fixture = case.executable(
        "var/ambiguous-peekaboo.sh",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' {}\n",
            shell_quote(&serde_json::to_string(&ambiguous).unwrap())
        ),
    );
    config.output = PathBuf::from("var/ambiguous-focus");
    config.peekaboo_executable = invalid_fixture;
    assert!(
        backend::FocusProbe::new(&root, &config)
            .unwrap()
            .sample()
            .is_err()
    );
    assert!(!forbidden_marker.exists());
}

#[test]
fn menu_format_label_and_actual_step_schema_remain_distinct_source_evidence() {
    let bytes = b"ISO-10303-21;\nHEADER;FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));ENDSEC;DATA;#1=PRODUCT('P1','','',());#2=SI_UNIT(.MILLI.,.METRE.);ENDSEC;END-ISO-10303-21;\n";
    let inspected = artifacts::inspect_file("P1.stp", bytes, "P1", "STEP AP214").unwrap();
    assert_eq!(inspected["requested_format"], "STEP AP214");
    assert_eq!(inspected["schema"], "CONFIG_CONTROL_DESIGN");
    assert_eq!(inspected["sha256"], oe_core::sha256(bytes));
    assert_eq!(
        inspected["format_check"],
        "STEP_envelope_and_schema_present"
    );
    assert_eq!(inspected["units"], "unknown");
    assert_eq!(inspected["engineering_acceptance"], "not_evaluated");
    let native =
        artifacts::inspect_file("P1.prt", b"synthetic native payload", "P1", "NX 2306").unwrap();
    assert_eq!(native["requested_format"], "NX 2306");
    assert_eq!(native["format_check"], "native_reader_required");
    assert_eq!(native["units"], "unknown");
    assert_eq!(native["engineering_acceptance"], "not_evaluated");
    assert!(native.get("native_version").is_none());
    assert!(artifacts::inspect_file("P1.prt", bytes, "P1", "STEP AP214").is_err());
}
