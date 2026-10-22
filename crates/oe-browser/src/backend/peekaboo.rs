//! Current Peekaboo owner. Code-owned DOM programs, exact background target and
//! immutable provider/worker identities; no input, browser restart or fallback.
use super::{BackendConfig, dom, focus, ipc::IpcTransport};
use crate::{
    Action, ActionKind, BackendKind, Browser, BrowserTask, DispatchState, ExecutionReceipt,
    Observation, SessionIdentity, SnapshotSource,
};
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

const PROTOCOL: &str = "oev2.peekaboo-bridge/1";
const BRIDGE: &[u8] = include_bytes!("../../../../runtime/peekaboo/bridge.mjs");
const HOOK: &[u8] = include_bytes!("../../../../runtime/peekaboo/focus-policy.mjs");
const LOCK: &[u8] = include_bytes!("../../../../runtime/peekaboo/provider-lock.json");

fn lock() -> Result<Value> {
    Ok(oe_core::json::strict_json(LOCK)?)
}
fn check_assets(root: &SkillRoot) -> Result<()> {
    for (path, bytes) in [
        ("bridge.mjs", BRIDGE),
        ("focus-policy.mjs", HOOK),
        ("provider-lock.json", LOCK),
    ] {
        ensure!(
            read_regular_bytes(
                &root.path().join("runtime/peekaboo").join(path),
                1024 * 1024
            )? == bytes,
            "peekaboo_compiled_transport_assets_changed"
        );
    }
    Ok(())
}

pub(super) fn check_executable(root: &SkillRoot, config: &BackendConfig) -> Result<()> {
    check_assets(root)?;
    let pins = lock()?;
    ensure!(
        config.peekaboo_executable.is_absolute(),
        "peekaboo_executable_absolute_path_required"
    );
    ensure!(
        sha256(&read_regular_bytes(
            &config.peekaboo_executable.canonicalize()?,
            256 * 1024 * 1024
        )?) == pins["peekaboo_binary_sha256"],
        "peekaboo_executable_pin_changed"
    );
    Ok(())
}

/// Validate only an explicit existing loopback listener owned by the current
/// Chrome process. Never discover endpoints in Chrome's profile directory.
fn verify_existing_cdp(
    root: &SkillRoot,
    config: &BackendConfig,
    output: &std::path::Path,
) -> Result<Value> {
    let endpoint = config
        .cdp_endpoint
        .as_deref()
        .context("explicit_existing_cdp_endpoint_required_without_profile_discovery")?;
    let url = parse_existing_cdp_endpoint(endpoint)?;
    check_executable(root, config)?;
    let apps = oe_core::process::run_command_memory(
        &oe_core::process::CommandSpec {
            executable: config.peekaboo_executable.clone(),
            arguments: ["list", "apps", "--json", "--no-remote"]
                .map(Into::into)
                .to_vec(),
            cwd: root.path().to_owned(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout: Duration::from_secs(5),
            output_limit: 8 * 1024 * 1024,
        },
        &output.join("apps"),
    )?;
    ensure!(
        apps.status == "completed" && apps.exit_code == Some(0),
        "existing_chrome_process_unconfirmed"
    );
    let raw = oe_core::json::strict_json(&apps.stdout)?;
    let chrome: Vec<_> = raw["data"]["applications"]
        .as_array()
        .context("peekaboo_applications_required")?
        .iter()
        .filter(|v| v["bundleIdentifier"] == "com.google.Chrome")
        .collect();
    ensure!(
        raw["success"] == true && chrome.len() == 1,
        "existing_chrome_process_unconfirmed"
    );
    let pid = chrome[0]["processIdentifier"]
        .as_u64()
        .filter(|p| *p > 0 && *p < i32::MAX as u64)
        .context("peekaboo_chrome_pid_required")? as u32;
    let instance = focus::process_start(pid)?;
    let port = url.port_or_known_default().unwrap();
    let listener = oe_core::process::run_command_memory(
        &oe_core::process::CommandSpec {
            executable: PathBuf::from("/usr/sbin/lsof"),
            arguments: vec![
                "-nP".into(),
                "-a".into(),
                format!("-p{pid}").into(),
                format!("-iTCP:{port}").into(),
                "-sTCP:LISTEN".into(),
                "-Fpn".into(),
            ],
            cwd: root.path().to_owned(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout: Duration::from_secs(5),
            output_limit: 65536,
        },
        &output.join("listener"),
    )?;
    let text = std::str::from_utf8(&listener.stdout)?;
    let expected = if url.host_str() == Some("127.0.0.1") {
        format!("n127.0.0.1:{port}")
    } else {
        format!("n[::1]:{port}")
    };
    ensure!(
        listener.status == "completed"
            && listener.exit_code == Some(0)
            && text.lines().any(|v| v == format!("p{pid}"))
            && text.lines().any(|v| v == expected),
        "existing_cdp_listener_not_owned_by_current_chrome"
    );
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()?;
    let response = client
        .get(url.join("json/version")?)
        .send()
        .context("existing_cdp_version_unconfirmed")?;
    ensure!(
        response.status().is_success(),
        "existing_cdp_version_unconfirmed"
    );
    use std::io::Read;
    let mut bytes = Vec::new();
    response.take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "existing_cdp_version_too_large");
    let version = oe_core::json::strict_json(&bytes)?;
    let ws = url::Url::parse(
        version["webSocketDebuggerUrl"]
            .as_str()
            .context("existing_cdp_websocket_required")?,
    )?;
    ensure!(
        ws.scheme() == "ws"
            && ws.host_str() == url.host_str()
            && ws.port_or_known_default() == url.port_or_known_default()
            && ws.username().is_empty()
            && ws.password().is_none()
            && ws.query().is_none()
            && ws.fragment().is_none()
            && ws.path().starts_with("/devtools/browser/"),
        "existing_cdp_websocket_scope_mismatch"
    );
    ensure!(
        focus::process_start(pid)? == instance,
        "chrome_process_instance_changed"
    );
    let result = json!({"status":"existing_cdp_endpoint_verified","chrome_pid":pid,"endpoint_sha256":sha256(endpoint.as_bytes()),
        "version_response_sha256":sha256(&bytes),"websocket_url_sha256":sha256(ws.as_str().as_bytes()),"profile_directory_read":false,"browser_started":false});
    write_new(&output.join("result.json"), &result)?;
    Ok(result)
}

/// Exposed as a pure protocol check for independent counterexamples. A matching
/// packet alone is not a real-browser receipt; actual transport and source are separate.
pub fn validate_packet(packet: &Value) -> Result<()> {
    // The shared identity is generated from specs by actual locked ggen;
    // hand-written policy below remains the sole execution/adoption owner.
    let identity: oe_contracts::PeekabooTransportIdentity = serde_json::from_value(json!({
        "schema":packet["schema"],"worker_session":packet["worker_session"],"worker_pid":packet["worker_pid"],
        "chrome_pid":packet["chrome_pid"],"bridge_sha256":packet["bridge_sha256"],"focus_hook_sha256":packet["focus_hook_sha256"],
        "peekaboo_binary_sha256":packet["peekaboo_binary_sha256"],"chrome_devtools_source_sha256":packet["chrome_devtools_source_sha256"],
        "provider_pin_applied":packet["provider_pin_applied"]
    })).context("peekaboo_generated_identity_shape_invalid")?;
    ensure!(
        identity.validate().is_ok(),
        "peekaboo_generated_identity_constraint_invalid"
    );
    let pins = lock()?;
    ensure!(
        packet["schema"] == PROTOCOL && packet["success"] == true && packet["status"] == "ready",
        "peekaboo_connection_or_operation_unconfirmed"
    );
    ensure!(
        packet["bridge_sha256"] == sha256(BRIDGE)
            && packet["focus_hook_sha256"] == sha256(HOOK)
            && packet["peekaboo_binary_sha256"] == pins["peekaboo_binary_sha256"]
            && packet["chrome_devtools_source_sha256"] == pins["chrome_devtools_source_sha256"]
            && packet["provider_pin_applied"] == true,
        "peekaboo_provider_identity_changed"
    );
    ensure!(
        packet["chrome_launches"] == 0
            && packet["settings_changes"] == 0
            && packet["focus_actions"] == 0
            && packet["storage_reads"] == 0
            && packet["request_retries"] == 0,
        "peekaboo_transport_policy_violation"
    );
    ensure!(
        packet["worker_session"]
            .as_str()
            .is_some_and(|v| !v.is_empty() && v.len() <= 256)
            && packet["worker_pid"]
                .as_u64()
                .is_some_and(|v| v > 0 && v < i32::MAX as u64)
            && packet["chrome_pid"]
                .as_u64()
                .is_some_and(|v| v > 0 && v < i32::MAX as u64),
        "peekaboo_process_identity_required"
    );
    ensure!(
        packet["chrome_pid_before"] == packet["chrome_pid"]
            && packet["chrome_pid_after"] == packet["chrome_pid"],
        "peekaboo_chrome_instance_changed_during_operation"
    );
    let before = &packet["focus_before"];
    ensure!(
        before.is_object()
            && before["bundle"].as_str().is_some_and(|v| !v.is_empty())
            && before["pid"].as_u64().is_some_and(|v| v > 0)
            && *before == packet["focus_after"]
            && packet["active_application_unchanged"] == true,
        "peekaboo_active_application_changed"
    );
    Ok(())
}

fn transport(config: &BackendConfig) -> Result<IpcTransport> {
    ensure!(
        !config.timeout.is_zero() && config.timeout <= Duration::from_secs(60),
        "browser_transport_timeout_out_of_range"
    );
    ensure!(
        (1024..=32 * 1024 * 1024).contains(&config.response_limit),
        "browser_response_limit_out_of_range"
    );
    IpcTransport::new_peekaboo(
        config
            .peekaboo_socket
            .as_deref()
            .context("existing_peekaboo_socket_required")?,
        config.timeout,
        config.response_limit,
    )
}

/// Start only our transport; startup never claims a connection. Chrome and its
/// profile are untouched. Existing sockets are inspected rather than replaced.
pub fn start_worker(root: &SkillRoot, config: &BackendConfig) -> Result<Value> {
    if config.cdp_endpoint.is_none() {
        return super::ax::observe(root, config, config.window_id);
    }
    ensure!(
        cfg!(target_os = "macos"),
        "peekaboo_background_unsupported_platform"
    );
    check_assets(root)?;
    let path = config
        .peekaboo_socket
        .as_deref()
        .context("explicit_peekaboo_socket_required")?;
    ensure!(
        path.is_absolute() && path.as_os_str().len() < 100,
        "peekaboo_socket_path_invalid"
    );
    oe_core::no_symlinks(path.parent().context("peekaboo_socket_parent_required")?)?;
    let output = root.new_private_dir(&config.output)?;
    verify_existing_cdp(root, config, &output.join("cdp-preflight"))?;
    if path.exists() {
        let mut current = transport(config)?;
        let value = current.request(
            &json!({"schema":PROTOCOL,"action":"status"}),
            &output.join("existing-worker"),
        )?;
        ensure!(
            value["schema"] == PROTOCOL
                && value["bridge_sha256"] == sha256(BRIDGE)
                && value["focus_hook_sha256"] == sha256(HOOK)
                && value["cdp_endpoint_sha256"]
                    == sha256(config.cdp_endpoint.as_deref().unwrap().as_bytes()),
            "existing_peekaboo_worker_identity_mismatch"
        );
        return Ok(
            json!({"status":"existing_worker_retained","worker_status":value["status"],
            "connection_verified":false,"transport_process_started":false,"browser_started":false}),
        );
    }
    let pins = lock()?;
    for (executable, pin) in [
        (&config.peekaboo_executable, "peekaboo_binary_sha256"),
        (&config.node_executable, "node_binary_sha256"),
    ] {
        ensure!(
            executable.is_absolute(),
            "peekaboo_executable_absolute_path_required"
        );
        let canonical = executable.canonicalize()?;
        ensure!(
            sha256(&read_regular_bytes(&canonical, 256 * 1024 * 1024)?) == pins[pin],
            "peekaboo_executable_pin_changed"
        );
    }
    let permissions = oe_core::process::run_command_memory(
        &oe_core::process::CommandSpec {
            executable: config.peekaboo_executable.clone(),
            arguments: ["permissions", "status", "--json", "--no-remote"]
                .map(Into::into)
                .to_vec(),
            cwd: root.path().to_owned(),
            environment: BTreeMap::new(),
            remove_environment: vec![],
            timeout: Duration::from_secs(5),
            output_limit: 1024 * 1024,
        },
        &output.join("permissions"),
    )?;
    ensure!(
        permissions.exit_code == Some(0) && permissions.status == "completed",
        "peekaboo_permissions_unknown"
    );
    let granted = oe_core::json::strict_json(&permissions.stdout)?;
    ensure!(
        granted["success"] == true
            && granted["data"]["permissions"].as_array().is_some_and(|p| [
                "Screen Recording",
                "Accessibility"
            ]
            .iter()
            .all(|name| p
                .iter()
                .any(|v| v["name"] == *name && v["isGranted"] == true))),
        "peekaboo_required_permissions_not_granted"
    );
    let _before = focus::active_application(
        &config.peekaboo_executable,
        root.path(),
        Duration::from_secs(5),
        &output.join("before-start"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(output.join("worker.log"))?;
        let child = std::process::Command::new(&config.node_executable)
            .arg(root.path().join("runtime/peekaboo/bridge.mjs"))
            .arg(path)
            .arg(&config.peekaboo_executable)
            .arg(&output)
            .arg(
                config
                    .cdp_endpoint
                    .as_deref()
                    .context("explicit_existing_cdp_endpoint_required_without_profile_discovery")?,
            )
            .current_dir(root.path())
            .stdin(std::process::Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file)
            .process_group(0)
            .spawn()?;
        let result = json!({"status":"worker_started","worker_pid":child.id(),"socket":path,
            "connection_verified":false,"transport_process_started":true,"browser_started":false,"settings_changed":false,"request_retries":0});
        write_new(&output.join("result.json"), &result)?;
        return Ok(result);
    }
    #[cfg(not(unix))]
    anyhow::bail!("peekaboo_unix_transport_unsupported_platform")
}

pub fn connection_check(root: &SkillRoot, config: &BackendConfig) -> Result<Value> {
    if config.cdp_endpoint.is_none() {
        return super::ax::observe(root, config, config.window_id);
    }
    check_assets(root)?;
    let output = root.new_private_dir(&config.output)?;
    verify_existing_cdp(root, config, &output.join("cdp-preflight"))?;
    let mut channel = transport(config)?;
    let packet = channel.request(
        &json!({"schema":PROTOCOL,"action":"check"}),
        &output.join("check"),
    )?;
    validate_packet(&packet)?;
    ensure!(
        packet["cdp_endpoint_sha256"] == sha256(config.cdp_endpoint.as_deref().unwrap().as_bytes()),
        "peekaboo_existing_cdp_endpoint_changed"
    );
    ensure!(
        packet["result"]["connection_verified"] == true
            && packet["result"]["real_operation"] == "list_pages",
        "peekaboo_real_connection_operation_required"
    );
    write_new(&output.join("result.json"), &packet)?;
    Ok(packet)
}

pub(super) fn connect(
    root: &SkillRoot,
    task: &BrowserTask,
    source: &SnapshotSource,
    config: &BackendConfig,
) -> Result<Box<dyn Browser>> {
    task.validate()?;
    ensure!(
        sha256(source.javascript.as_bytes()) == source.sha256
            && !source.javascript.is_empty()
            && source.javascript.len() <= 1024 * 1024,
        "browser_snapshot_source_mismatch"
    );
    let endpoint = config
        .cdp_endpoint
        .as_deref()
        .context("explicit_existing_cdp_endpoint_required_without_profile_discovery")?;
    parse_existing_cdp_endpoint(endpoint)?;
    let preflight = root
        .private_path(&config.output)?
        .with_extension("cdp-preflight");
    oe_core::create_new_private_dir(&preflight)?;
    verify_existing_cdp(root, config, &preflight)?;
    connect_protocol(root, task, source, config)
}

// Internal protocol owner: caller supplies the independently validated endpoint.
// Unit tests exercise only this layer with synthetic Unix packets, never live admission.
fn connect_protocol(
    root: &SkillRoot,
    task: &BrowserTask,
    source: &SnapshotSource,
    config: &BackendConfig,
) -> Result<Box<dyn Browser>> {
    ensure!(
        task.backend == BackendKind::PeekabooBackground,
        "peekaboo_backend_required"
    );
    ensure!(
        sha256(source.javascript.as_bytes()) == source.sha256
            && !source.javascript.is_empty()
            && source.javascript.len() <= 1024 * 1024,
        "browser_snapshot_source_mismatch"
    );
    check_assets(root)?;
    let output = root.new_private_dir(&config.output)?;
    let mut channel = transport(config)?;
    let packet = channel.request(
        &json!({"schema":PROTOCOL,"action":"check"}),
        &output.join("connection"),
    )?;
    validate_packet(&packet)?;
    ensure!(
        packet["result"]["connection_verified"] == true
            && packet["result"]["real_operation"] == "list_pages",
        "peekaboo_real_connection_operation_required"
    );
    let worker_session = packet["worker_session"].as_str().unwrap().to_owned();
    let endpoint_sha256 = sha256(
        config
            .cdp_endpoint
            .as_deref()
            .context("explicit_existing_cdp_endpoint_required_without_profile_discovery")?
            .as_bytes(),
    );
    ensure!(
        packet["cdp_endpoint_sha256"] == endpoint_sha256,
        "peekaboo_existing_cdp_endpoint_changed"
    );
    let chrome_pid = packet["chrome_pid"].as_u64().unwrap() as u32;
    let chrome_instance = focus::process_start(chrome_pid)?;
    let session_identity = chrome_instance.peekaboo_session(&worker_session);
    let owner = sha256(output.to_string_lossy().as_bytes());
    let mut browser = PeekabooBrowser {
        channel,
        output,
        serial: 0,
        worker_session: worker_session.clone(),
        endpoint_sha256,
        chrome_pid,
        chrome_instance,
        baseline_focus: packet["focus_before"].clone(),
        owner,
        closed: false,
        task: task.clone(),
        source: source.clone(),
        issued: None,
        identity: SessionIdentity {
            backend: task.backend,
            target: String::new(),
            session: session_identity,
            origin: crate::task::http_origin(&task.url)?,
            owns_target: false,
        },
    };
    let action = if task.tab_id.is_some() {
        "bind"
    } else {
        "open"
    };
    let established = browser.request(
        action,
        json!({"url":task.url,"page_id":task.tab_id,"owner":browser.owner}),
        true,
    )?;
    let id = established["page_id"]
        .as_u64()
        .context("peekaboo_exact_target_required")?;
    if task.tab_id.is_none() {
        let prior = established["prior_page_ids"]
            .as_array()
            .context("peekaboo_owned_creation_prior_pages_required")?;
        ensure!(
            id > 0 && established["selected_created_page_id"] == id && !prior.contains(&json!(id)),
            "peekaboo_target_not_created_by_current_operation"
        );
    }
    browser.identity.target = id.to_string();
    browser.identity.owns_target = task.tab_id.is_none();
    ensure!(
        established["owns_target"] == browser.identity.owns_target,
        "peekaboo_target_ownership_mismatch"
    );
    if let Err(error) = write_new(
        &browser.output.join("session.json"),
        &json!({"identity":browser.identity,"connection":packet,
        "snapshot_sha256":source.sha256,"browser_started":false,"settings_changed":false,"storage_accessed":false,"semantic_authority":false}),
    ) {
        let cleanup = browser.close().unwrap_or_else(
            |_| json!({"status":"cleanup_unconfirmed","target_retained_for_diagnosis":true}),
        );
        let _ = write_new(
            &browser.output.join("session-persistence-failure.json"),
            &json!({"code":"browser_session_persistence_failed","identity":browser.identity,"cleanup":cleanup}),
        );
        return Err(error);
    }
    Ok(Box::new(browser))
}

struct PeekabooBrowser {
    channel: IpcTransport,
    output: PathBuf,
    serial: u64,
    worker_session: String,
    endpoint_sha256: String,
    chrome_pid: u32,
    chrome_instance: focus::ChromeInstance,
    baseline_focus: Value,
    owner: String,
    closed: bool,
    task: BrowserTask,
    source: SnapshotSource,
    identity: SessionIdentity,
    issued: Option<Observation>,
}
impl PeekabooBrowser {
    fn validate_target_state(&self, state: &Value) -> Result<()> {
        ensure!(
            state["page_has_focus"] == false,
            "peekaboo_target_focus_changed"
        );
        let url = state["url"]
            .as_str()
            .context("peekaboo_target_url_required")?;
        ensure!(
            state["origin"] == self.identity.origin
                && crate::task::http_origin(url)? == self.identity.origin,
            "peekaboo_target_source_changed"
        );
        Ok(())
    }
    fn request(&mut self, action: &str, body: Value, retain: bool) -> Result<Value> {
        if matches!(action, "evaluate" | "screenshot" | "navigate" | "close") {
            let state =
                self.request_raw("target_state", json!({"page_id":body["page_id"]}), true)?;
            self.validate_target_state(&state)?;
        }
        self.request_raw(action, body, retain)
    }
    fn request_raw(&mut self, action: &str, mut body: Value, retain: bool) -> Result<Value> {
        ensure!(!self.closed, "browser_session_closed");
        ensure!(
            focus::process_start(self.chrome_pid)? == self.chrome_instance,
            "chrome_process_instance_changed"
        );
        body["schema"] = json!(PROTOCOL);
        body["action"] = json!(action);
        body["worker_session"] = json!(self.worker_session);
        self.serial += 1;
        let output = self.output.join(format!("{:05}-{action}", self.serial));
        let packet = if retain {
            self.channel.request(&body, &output)?
        } else {
            self.channel.request_memory(&body, &output)?
        };
        validate_packet(&packet)?;
        ensure!(
            packet["provider_page_ids_changed"] != true,
            "peekaboo_provider_page_identity_changed"
        );
        ensure!(
            packet["worker_session"] == self.worker_session
                && packet["chrome_pid"] == self.chrome_pid
                && packet["cdp_endpoint_sha256"] == self.endpoint_sha256,
            "peekaboo_session_identity_changed"
        );
        ensure!(
            packet["focus_before"] == self.baseline_focus,
            "peekaboo_active_application_changed_since_connection"
        );
        ensure!(
            focus::process_start(self.chrome_pid)? == self.chrome_instance,
            "chrome_process_instance_changed"
        );
        if action != "close" {
            self.validate_target_state(&packet["target_after"])?;
            if matches!(action, "open" | "bind") {
                ensure!(
                    packet["target_after"]["url"] == self.task.url,
                    "peekaboo_exact_target_url_mismatch"
                );
            }
        }
        Ok(packet["result"].clone())
    }
    fn evaluate(&mut self, expression: &str, retain: bool) -> Result<Value> {
        let focus_guard = format!(
            "(()=>{{if(document.hasFocus())throw new Error('Target page focus changed');return ({expression});}})()"
        );
        let code = dom::origin_guard(&focus_guard, &self.identity.origin)?;
        let result = self.request(
            "evaluate",
            json!({"page_id":self.identity.target.parse::<u64>()?,"expression":code}),
            retain,
        )?;
        let encoded = result["value"]
            .as_str()
            .context("peekaboo_javascript_result_missing")?;
        oe_core::json::strict_json(encoded.as_bytes()).context("peekaboo_javascript_invalid_result")
    }
    fn validate_observation(&self, observed: &Observation) -> Result<()> {
        observed.validate()?;
        ensure!(
            observed.session == self.identity,
            "browser_observation_session_mismatch"
        );
        let issued = self
            .issued
            .as_ref()
            .context("browser_current_observation_required")?;
        let (mut left, mut right) = (observed.page.clone(), issued.page.clone());
        for key in ["fingerprint", "actions"] {
            left.as_object_mut().unwrap().remove(key);
            right.as_object_mut().unwrap().remove(key);
        }
        ensure!(left == right, "browser_observation_not_issued_by_backend");
        for action in observed.actions()? {
            ensure!(
                issued.page["actions"]
                    .as_array()
                    .is_some_and(|items| items.contains(&action.raw)),
                "browser_unobserved_action_content"
            );
        }
        Ok(())
    }
    fn supplier_origin(&self) -> Result<()> {
        ensure!(
            self.identity.origin == "https://www.misumi.com.cn",
            "misumi_origin_required"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "peekaboo_protocol_tests.rs"]
mod protocol_tests;
impl Browser for PeekabooBrowser {
    fn identity(&self) -> &SessionIdentity {
        &self.identity
    }
    fn observe(&mut self, screenshot: bool) -> Result<Observation> {
        self.issued = None;
        let expression = dom::snapshot(&self.source.javascript, &self.task.custom_click_selectors)?;
        let mut page = self.evaluate(&expression, true)?;
        ensure!(page.is_object(), "browser_observation_stale");
        if self.task.tab_id.is_some() {
            let allowed: Vec<_> = page["actions"]
                .as_array()
                .context("browser_actions_required")?
                .iter()
                .filter(|a| {
                    matches!(a["kind"].as_str(), Some("scroll" | "wait"))
                        || self.task.allowed_actions.iter().any(|r| {
                            serde_json::to_value(r.kind).ok().as_ref() == a.get("kind")
                                && a["label"].as_str() == Some(&r.label)
                        })
                })
                .cloned()
                .collect();
            page["actions"] = json!(allowed);
        }
        if screenshot {
            let before = page["marker"].clone();
            let shot = self.request(
                "screenshot",
                json!({"page_id":self.identity.target.parse::<u64>()?}),
                true,
            )?;
            let after = self.evaluate(
                &format!("(()=>{{const p={expression};return p?.marker??null;}})()"),
                true,
            )?;
            ensure!(before == after, "browser_screenshot_observation_stale");
            page["screenshot"] = shot["data"].clone();
        }
        let observation = Observation::new(self.identity.clone(), page)?;
        self.issued = Some(observation.clone());
        Ok(observation)
    }
    fn fresh(&mut self, observed: &Observation, action: Option<&Action>) -> Result<bool> {
        self.validate_observation(observed)?;
        if let Some(action) = action {
            action.validate_against(observed)?;
            if let Some(node) = action.node {
                let value=self.evaluate(&format!("(()=>{{const c=window.__oeBrowserSnapshot;return c?[c.pageKey(),c.guard(c.nodes.get({node}))]:null;}})()"),true)?;
                return Ok(value
                    == json!([
                        observed.page["page_key"],
                        observed.page["guards"][node.to_string()]
                    ]));
            }
        }
        let expression = dom::snapshot(&self.source.javascript, &self.task.custom_click_selectors)?;
        Ok(self.evaluate(
            &format!("(()=>{{const p={expression};return p?.marker??null;}})()"),
            true,
        )? == observed.page["marker"])
    }
    fn act(
        &mut self,
        observed: &Observation,
        action: &Action,
        text: Option<&str>,
    ) -> Result<ExecutionReceipt> {
        self.validate_observation(observed)?;
        action.validate_against(observed)?;
        ensure!(self.task.permits(action), "action_outside_declared_scope");
        if action.kind == ActionKind::Fill {
            ensure!(
                text.is_some()
                    && self.task.text_values.get(&action.label).map(String::as_str) == text,
                "exact_field_value_mismatch"
            );
        } else {
            ensure!(text.is_none(), "text_only_for_observed_fill");
        }
        ensure!(
            self.fresh(observed, Some(action))?,
            "browser_observation_stale"
        );
        let result = if action.kind == ActionKind::Wait {
            std::thread::sleep(Duration::from_millis(250));
            Ok(json!({"executed":action.id}))
        } else {
            let data = json!({"action":action.raw,"text":text,"page_key":observed.page["page_key"],"guard":action.node.and_then(|n|observed.page["guards"].get(n.to_string())).cloned()});
            self.evaluate(&dom::apply(&data)?, true)
        };
        self.issued = None;
        let (state, diagnostic) = match &result {
            Ok(v) if v["stale"] == true => (
                DispatchState::NotDispatched,
                Some("browser_observation_stale"),
            ),
            Ok(v) if v["unsupported"] == true => (
                DispatchState::NotDispatched,
                Some("browser_action_unsupported"),
            ),
            Ok(v) if v["executed"].as_str() == Some(&action.id) => (DispatchState::Confirmed, None),
            _ => (
                DispatchState::OutcomeUnknown,
                Some("browser_action_outcome_unconfirmed"),
            ),
        };
        Ok(ExecutionReceipt {
            action_id: action.id.clone(),
            state,
            execution_kind: "peekaboo_background_dom".into(),
            readback: result.ok(),
            diagnostic_code: diagnostic.map(Into::into),
        })
    }
    fn supplier_observation(&mut self) -> Result<Value> {
        self.supplier_origin()?;
        self.evaluate(dom::SUPPLIER, true)
    }
    fn account_observation(&mut self) -> Result<Value> {
        self.supplier_origin()?;
        self.evaluate(dom::ACCOUNT, false)
    }
    fn navigate_account(&mut self) -> Result<ExecutionReceipt> {
        self.supplier_origin()?;
        self.evaluate("location.origin", true)?;
        let result = self.request(
            "navigate",
            json!({"page_id":self.identity.target.parse::<u64>()?,"url":"https://www.misumi.com.cn/my/user/manage/"}),
            true,
        );
        self.issued = None;
        let confirmed = result.as_ref().is_ok_and(|v| v["navigated"] == true);
        Ok(ExecutionReceipt {
            action_id: "navigate_account".into(),
            state: if confirmed {
                DispatchState::Confirmed
            } else {
                DispatchState::OutcomeUnknown
            },
            execution_kind: "peekaboo_background_navigation".into(),
            readback: result.ok(),
            diagnostic_code: if confirmed {
                None
            } else {
                Some("account_navigation_outcome_unconfirmed".into())
            },
        })
    }
    fn close(&mut self) -> Result<Value> {
        if self.closed {
            return Ok(json!({"status":"already_closed"}));
        }
        if !self.identity.owns_target || self.task.keep_open {
            self.closed = true;
            return Ok(
                json!({"status":"retained","owns_target":self.identity.owns_target,"target":self.identity.target}),
            );
        }
        let result = self.request(
            "close",
            json!({"page_id":self.identity.target.parse::<u64>()?,"owner":self.owner}),
            true,
        )?;
        ensure!(result["closed"] == true, "owned_target_cleanup_unconfirmed");
        self.closed = true;
        Ok(json!({"status":"closed_owned_target","target":self.identity.target}))
    }
}

pub fn parse_existing_cdp_endpoint(endpoint: &str) -> Result<url::Url> {
    ensure!(
        endpoint.len() <= 16384
            && !endpoint.chars().any(char::is_control)
            && endpoint.trim() == endpoint,
        "existing_cdp_endpoint_invalid"
    );
    let url = url::Url::parse(endpoint).context("existing_cdp_endpoint_invalid")?;
    ensure!(
        url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.port_or_known_default().is_some_and(|p| p > 0),
        "existing_cdp_loopback_endpoint_required"
    );
    Ok(url)
}
