//! Bounded native browser transports. No daemon startup, preference changes,
//! browser storage access or unscoped JavaScript entrypoint is exposed.
mod ax;
mod dom;
mod focus;
mod ipc;
mod peekaboo;
pub use ax::{observe as ax_observe, select_window as select_ax_window};
pub use focus::FocusProbe;
pub use peekaboo::{
    connection_check, parse_existing_cdp_endpoint, start_worker,
    validate_packet as validate_peekaboo_packet,
};

use crate::{
    Action, ActionKind, BackendKind, Browser, BrowserTask, DispatchState, ExecutionReceipt,
    Observation, SessionIdentity, SnapshotSource,
};
use anyhow::{Context, Result, bail, ensure};
use oe_core::{SkillRoot, create_new_private_dir, sha256, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub output: PathBuf,
    pub osascript: PathBuf,
    pub harness_socket: Option<PathBuf>,
    pub peekaboo_socket: Option<PathBuf>,
    pub peekaboo_executable: PathBuf,
    pub node_executable: PathBuf,
    pub cdp_endpoint: Option<String>,
    pub window_id: Option<u64>,
    pub timeout: Duration,
    pub response_limit: usize,
}

impl BackendConfig {
    pub fn new(output: PathBuf) -> Self {
        Self {
            output,
            osascript: PathBuf::from("/usr/bin/osascript"),
            harness_socket: None,
            peekaboo_socket: None,
            peekaboo_executable: PathBuf::from("/opt/homebrew/bin/peekaboo"),
            node_executable: PathBuf::from("/opt/homebrew/bin/node"),
            cdp_endpoint: None,
            window_id: None,
            timeout: Duration::from_secs(30),
            response_limit: 16 * 1024 * 1024,
        }
    }
}

enum Transport {
    Apple { osascript: PathBuf },
    Harness(ipc::IpcTransport),
}

pub fn connect(
    root: &SkillRoot,
    task: &BrowserTask,
    source: &SnapshotSource,
    config: &BackendConfig,
) -> Result<Box<dyn Browser>> {
    task.validate()?;
    ensure_current_policy(task.backend)?;
    peekaboo::connect(root, task, source, config)
}

/// Historical backend values remain readable; they never re-authorize live execution.
pub fn ensure_current_policy(kind: BackendKind) -> Result<()> {
    ensure!(
        kind == BackendKind::PeekabooBackground,
        "current_browser_policy_requires_peekaboo_background_no_legacy_fallback"
    );
    Ok(())
}

// Retained historical transport implementation, unreachable from the current live owner.
#[allow(dead_code)]
fn historical_connect(
    root: &SkillRoot,
    task: &BrowserTask,
    source: &SnapshotSource,
    config: &BackendConfig,
) -> Result<Box<dyn Browser>> {
    ensure!(
        sha256(source.javascript.as_bytes()) == source.sha256,
        "browser_snapshot_source_mismatch"
    );
    ensure!(
        !source.javascript.is_empty() && source.javascript.len() <= 1024 * 1024,
        "browser_snapshot_source_size_invalid"
    );
    ensure!(
        !config.timeout.is_zero() && config.timeout <= Duration::from_secs(60),
        "browser_transport_timeout_out_of_range"
    );
    ensure!(
        (1024..=32 * 1024 * 1024).contains(&config.response_limit),
        "browser_response_limit_out_of_range"
    );
    let output = root.private_path(&config.output)?;
    create_new_private_dir(&output)?;
    let origin = crate::task::http_origin(&task.url)?;
    let transport = match task.backend {
        BackendKind::PeekabooBackground => {
            bail!("historical_transport_cannot_execute_current_backend")
        }
        BackendKind::ChromeAppleEvents => {
            ensure!(
                cfg!(target_os = "macos"),
                "apple_events_unsupported_platform"
            );
            Transport::Apple {
                osascript: config.osascript.clone(),
            }
        }
        BackendKind::BrowserHarness => Transport::Harness(ipc::IpcTransport::new(
            config
                .harness_socket
                .as_ref()
                .context("existing_harness_socket_required")?,
            config.timeout,
            config.response_limit,
        )?),
    };
    let mut browser = NativeBrowser {
        identity: SessionIdentity {
            backend: task.backend,
            target: task.tab_id.map(|v| v.to_string()).unwrap_or_default(),
            session: String::new(),
            origin,
            owns_target: false,
        },
        task: task.clone(),
        source: source.clone(),
        transport,
        output,
        cwd: root.path().to_owned(),
        timeout: config.timeout,
        response_limit: config.response_limit,
        serial: 0,
        closed: false,
        osascript: config.osascript.clone(),
        chrome_instance: None,
        last_observation: None,
        after_input: None,
    };
    let setup = browser.setup();
    if let Err(error) = setup {
        // Only the exact target successfully returned by this invocation may
        // be cleaned up. A timed-out create with no identity is never guessed.
        let cleanup = browser
            .close()
            .unwrap_or_else(|_| json!({"status":"cleanup_unconfirmed"}));
        let _ = write_new(
            &browser.output.join("setup-failure.json"),
            &json!({"code":"browser_setup_failed","identity":browser.identity,"cleanup":cleanup}),
        );
        return Err(error);
    }
    if let Err(error) = write_new(
        &browser.output.join("session.json"),
        &json!({
            "identity":browser.identity,"snapshot_sha256":source.sha256,
            "daemon_started":false,"browser_started":false,"settings_changed":false,
            "storage_accessed":false,"semantic_authority":false
        }),
    ) {
        let cleanup = browser
            .close()
            .unwrap_or_else(|_| json!({"status":"cleanup_unconfirmed"}));
        let _ = write_new(
            &browser.output.join("session-persistence-failure.json"),
            &json!({"code":"browser_session_persistence_failed","identity":browser.identity,"cleanup":cleanup}),
        );
        return Err(error);
    }
    Ok(Box::new(browser))
}

struct NativeBrowser {
    identity: SessionIdentity,
    task: BrowserTask,
    source: SnapshotSource,
    transport: Transport,
    output: PathBuf,
    cwd: PathBuf,
    timeout: Duration,
    response_limit: usize,
    serial: u64,
    closed: bool,
    osascript: PathBuf,
    chrome_instance: Option<focus::ChromeInstance>,
    last_observation: Option<Observation>,
    after_input: Option<Action>,
}

impl NativeBrowser {
    fn next_dir(&mut self, label: &str) -> PathBuf {
        self.serial += 1;
        self.output.join(format!("{:05}-{label}", self.serial))
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.call_with_retention(method, params, true)
    }
    fn call_with_retention(
        &mut self,
        method: &str,
        params: Value,
        persist_response: bool,
    ) -> Result<Value> {
        ensure!(!self.closed, "browser_session_closed");
        let request = json!({"method":method,"params":params,
            "session_id":if self.identity.session.is_empty(){Value::Null}else{json!(self.identity.session)}});
        let out = self.next_dir("ipc");
        let Transport::Harness(transport) = &mut self.transport else {
            bail!("cdp_transport_required")
        };
        let value = if persist_response {
            transport.request(&request, &out)?
        } else {
            transport.request_memory(&request, &out)?
        };
        ensure!(
            value.get("error").is_none(),
            "browser_cdp_execution_unconfirmed"
        );
        value
            .get("result")
            .cloned()
            .context("browser_cdp_result_missing")
    }
    fn setup(&mut self) -> Result<()> {
        match self.identity.backend {
            BackendKind::PeekabooBackground => {
                bail!("historical_transport_cannot_execute_current_backend")
            }
            BackendKind::ChromeAppleEvents => {
                let out = self.next_dir("chrome-instance");
                let instance =
                    focus::chrome_instance(&self.osascript, &self.cwd, self.timeout, &out)?;
                self.identity.session = instance.session();
                self.chrome_instance = Some(instance);
                let current = self.evaluate("location.href")?;
                ensure!(
                    current.as_str() == Some(self.task.url.as_str()),
                    "existing_tab_url_mismatch"
                );
            }
            BackendKind::BrowserHarness => {
                let out = self.next_dir("handshake");
                if let Transport::Harness(transport) = &mut self.transport {
                    transport.handshake(&out)?;
                }
                // Existing daemon/CDP health only. Never discover or attach to
                // the daemon's default user target.
                let targets = self.call("Target.getTargets", json!({}))?;
                ensure!(
                    targets["targetInfos"].is_array(),
                    "existing_browser_connection_required"
                );
                let created = self.call(
                    "Target.createTarget",
                    json!({"url":"about:blank","background":true}),
                )?;
                self.identity.target = token(&created["targetId"], "created_target_id_required")?;
                self.identity.owns_target = true;
                let attached = self.call(
                    "Target.attachToTarget",
                    json!({"targetId":self.identity.target,"flatten":true}),
                )?;
                self.identity.session =
                    token(&attached["sessionId"], "attached_session_id_required")?;
                self.call(
                    "Emulation.setDeviceMetricsOverride",
                    json!({"width":1120,"height":780,"deviceScaleFactor":1,"mobile":false}),
                )?;
                self.call(
                    "Emulation.setFocusEmulationEnabled",
                    json!({"enabled":true}),
                )?;
                let navigated = self.call("Page.navigate", json!({"url":self.task.url}))?;
                ensure!(
                    navigated.get("errorText").is_none(),
                    "browser_navigation_unconfirmed"
                );
                let deadline = Instant::now() + Duration::from_secs(15).min(self.timeout);
                loop {
                    if self
                        .evaluate("document.readyState")
                        .is_ok_and(|v| v == "complete")
                    {
                        break;
                    }
                    ensure!(Instant::now() < deadline, "browser_navigation_not_ready");
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        Ok(())
    }
    fn evaluate(&mut self, expression: &str) -> Result<Value> {
        self.evaluate_with_retention(expression, true)
    }
    fn evaluate_with_retention(
        &mut self,
        expression: &str,
        persist_response: bool,
    ) -> Result<Value> {
        ensure!(!self.closed, "browser_session_closed");
        if self.identity.backend == BackendKind::ChromeAppleEvents {
            self.check_chrome_instance()?;
        }
        let script = dom::origin_guard(expression, &self.identity.origin)?;
        match &self.transport {
            Transport::Apple { osascript } => {
                let osascript = osascript.clone();
                let script = dom::apple_script(&self.identity.target, &script)?;
                let out = self.next_dir("apple-events");
                create_new_private_dir(&out)?;
                let input = out.join("input.applescript");
                write_new_bytes(&input, script.as_bytes())?;
                let spec = oe_core::process::CommandSpec {
                    executable: osascript,
                    arguments: vec![input.into_os_string()],
                    cwd: self.cwd.clone(),
                    environment: BTreeMap::new(),
                    remove_environment: vec![],
                    timeout: self.timeout,
                    output_limit: self.response_limit,
                };
                let result = if persist_response {
                    oe_core::process::run_command(&spec, &out.join("process"))?
                } else {
                    oe_core::process::run_command_memory(&spec, &out.join("process"))?
                };
                ensure!(
                    result.status == "completed" && result.exit_code == Some(0),
                    "apple_events_execution_unconfirmed"
                );
                self.check_chrome_instance()?;
                oe_core::json::strict_json(&result.stdout).context("apple_events_invalid_result")
            }
            Transport::Harness(_) => {
                let value = self.call_with_retention(
                    "Runtime.evaluate",
                    json!({"expression":script,"returnByValue":true}),
                    persist_response,
                )?;
                ensure!(
                    value.get("exceptionDetails").is_none(),
                    "browser_javascript_execution_unconfirmed"
                );
                let encoded = value["result"]["value"]
                    .as_str()
                    .context("browser_javascript_result_missing")?;
                oe_core::json::strict_json(encoded.as_bytes())
                    .context("browser_javascript_invalid_result")
            }
        }
    }
    fn validate_action(
        &self,
        observation: &Observation,
        action: &Action,
        text: Option<&str>,
    ) -> Result<()> {
        ensure!(!self.closed, "browser_session_closed");
        self.validate_observation(observation)?;
        action.validate_against(observation)?;
        ensure!(
            observation.session == self.identity,
            "browser_observation_session_mismatch"
        );
        ensure!(self.task.permits(action), "action_outside_declared_scope");
        if action.kind == ActionKind::Fill {
            let text = text.context("exact_field_value_required")?;
            ensure!(
                self.task.text_values.get(&action.label).map(String::as_str) == Some(text),
                "exact_field_value_mismatch"
            );
        } else {
            ensure!(text.is_none(), "text_only_for_observed_fill");
        }
        Ok(())
    }
    fn check_chrome_instance(&mut self) -> Result<()> {
        let out = self.next_dir("chrome-instance");
        let current = focus::chrome_instance(&self.osascript, &self.cwd, self.timeout, &out)?;
        ensure!(
            self.chrome_instance.as_ref() == Some(&current),
            "chrome_process_instance_changed"
        );
        Ok(())
    }
    fn validate_observation(&self, observation: &Observation) -> Result<()> {
        observation.validate()?;
        ensure!(
            observation.session == self.identity,
            "browser_observation_session_mismatch"
        );
        let original = self
            .last_observation
            .as_ref()
            .context("browser_current_observation_required")?;
        let mut observed = observation.page.clone();
        let mut baseline = original.page.clone();
        for key in ["fingerprint", "actions"] {
            observed.as_object_mut().unwrap().remove(key);
            baseline.as_object_mut().unwrap().remove(key);
        }
        ensure!(
            observed == baseline,
            "browser_observation_not_issued_by_backend"
        );
        for action in observation.actions()? {
            ensure!(
                original.page["actions"]
                    .as_array()
                    .is_some_and(|items| items.contains(&action.raw)),
                "browser_unobserved_action_content"
            );
        }
        Ok(())
    }
    fn receipt(
        &self,
        action: &Action,
        state: DispatchState,
        readback: Option<Value>,
        code: Option<&str>,
    ) -> ExecutionReceipt {
        ExecutionReceipt {
            action_id: action.id.clone(),
            state,
            execution_kind: match self.identity.backend {
                BackendKind::PeekabooBackground => "peekaboo_background_dom",
                BackendKind::ChromeAppleEvents => "chrome_apple_events_dom",
                BackendKind::BrowserHarness => "browser_harness_cdp",
            }
            .into(),
            readback,
            diagnostic_code: code.map(str::to_owned),
        }
    }
    fn ensure_supplier_origin(&self) -> Result<()> {
        ensure!(
            self.identity.origin == "https://www.misumi.com.cn",
            "misumi_origin_required"
        );
        Ok(())
    }
    fn background_guard(&mut self) -> Result<()> {
        bail!("historical_account_navigation_disabled_by_current_policy")
    }
    fn execute(
        &mut self,
        observation: &Observation,
        action: &Action,
        text: Option<&str>,
    ) -> Result<Value> {
        let data = json!({"action":action.raw,"text":text,"page_key":observation.page["page_key"],
            "guard":action.node.and_then(|n|observation.page["guards"].get(n.to_string())).cloned()});
        if self.identity.backend == BackendKind::ChromeAppleEvents
            || action.kind == ActionKind::Select
        {
            return self.evaluate(&dom::apply(&data)?);
        }
        if action.kind == ActionKind::Scroll {
            // Verify document immediately before dispatch. CDP input remains a
            // separate native operation and is not represented as atomic DOM.
            ensure!(
                self.fresh(observation, Some(action))?,
                "browser_observation_stale"
            );
            self.call(
                "Input.dispatchMouseEvent",
                json!({"type":"mouseWheel","x":550,"y":650,
                "deltaX":0,"deltaY":action.delta}),
            )?;
            return Ok(json!({"executed":action.id,"native_input_atomic_with_guard":false}));
        }
        let target = self.evaluate(&dom::resolve_target(&data)?)?;
        if target["stale"] == true {
            return Ok(target);
        }
        let x = target["x"]
            .as_f64()
            .filter(|v| v.is_finite())
            .context("browser_target_geometry_missing")?;
        let y = target["y"]
            .as_f64()
            .filter(|v| v.is_finite())
            .context("browser_target_geometry_missing")?;
        for event in ["mousePressed", "mouseReleased"] {
            self.call(
                "Input.dispatchMouseEvent",
                json!({"type":event,"x":x,"y":y,"button":"left","clickCount":1}),
            )?;
        }
        if action.kind == ActionKind::Fill {
            // Do not type into a different focus owner after click handlers.
            let focused = self.evaluate(&dom::focused(&data)?)?;
            ensure!(focused == true, "browser_fill_focus_unconfirmed");
            let info = self.call("Browser.getVersion", json!({}))?;
            let ua = info["userAgent"]
                .as_str()
                .context("browser_platform_unconfirmed")?;
            let modifier = if ua.contains("Mac OS X") || ua.contains("Macintosh") {
                4
            } else {
                2
            };
            self.call(
                "Input.dispatchKeyEvent",
                json!({"type":"keyDown","key":"a","code":"KeyA",
                "modifiers":modifier,"commands":["selectAll"]}),
            )?;
            self.call(
                "Input.dispatchKeyEvent",
                json!({"type":"keyUp","key":"a","code":"KeyA","modifiers":modifier}),
            )?;
            ensure!(
                self.evaluate(&dom::focused(&data)?)? == true,
                "browser_fill_focus_unconfirmed"
            );
            self.call("Input.insertText", json!({"text":text}))?;
            let readback = self.evaluate(&dom::readback(&data)?)?;
            ensure!(
                readback["value"].as_str() == text,
                "browser_fill_readback_unconfirmed"
            );
            return Ok(
                json!({"executed":action.id,"readback":readback,"native_input_atomic_with_guard":false}),
            );
        }
        Ok(json!({"executed":action.id,"native_input_atomic_with_guard":false}))
    }
}

impl Browser for NativeBrowser {
    fn identity(&self) -> &SessionIdentity {
        &self.identity
    }
    fn observe(&mut self, screenshot: bool) -> Result<Observation> {
        ensure!(
            !screenshot || self.identity.backend != BackendKind::ChromeAppleEvents,
            "apple_events_has_no_screenshot_capture"
        );
        self.last_observation = None;
        if let Some(action) = self.after_input.take()
            && self.identity.backend == BackendKind::BrowserHarness
        {
            let expression = dom::settle(&action.raw, &self.identity.origin)?;
            // Input has already been journaled by the caller. Navigation
            // may interrupt this read-only render wait; never replay input.
            let _ = self.call(
                "Runtime.evaluate",
                json!({"expression":expression,
                    "awaitPromise":true,"returnByValue":true,"timeout":1000}),
            );
        }
        let source = dom::snapshot(&self.source.javascript, &self.task.custom_click_selectors)?;
        let mut page = self.evaluate(&source)?;
        let deadline = Instant::now() + self.timeout.min(Duration::from_secs(1));
        for _ in 0..9 {
            if !page.is_null() || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
            page = self.evaluate(&source)?;
        }
        ensure!(page.is_object(), "browser_observation_stale");
        let actions = page["actions"]
            .as_array()
            .context("browser_actions_required")?
            .clone();
        if self.identity.backend == BackendKind::ChromeAppleEvents {
            page["actions"] = json!(
                actions
                    .into_iter()
                    .filter(|a| {
                        matches!(a["kind"].as_str(), Some("scroll" | "wait"))
                            || self.task.allowed_actions.iter().any(|r| {
                                serde_json::to_value(r.kind).ok().as_ref() == a.get("kind")
                                    && a["label"].as_str() == Some(&r.label)
                            })
                    })
                    .collect::<Vec<_>>()
            );
        }
        // Keep selected native SELECT values in the same captured snapshot,
        // avoiding a second-page read mixed with the earlier observation.
        if screenshot {
            let before = page["marker"].clone();
            let shot = self.call(
                "Page.captureScreenshot",
                json!({"format":"jpeg","quality":72}),
            )?;
            let encoded = shot["data"]
                .as_str()
                .context("browser_screenshot_missing")?;
            ensure!(
                encoded.len() <= self.response_limit,
                "browser_screenshot_too_large"
            );
            let after = self.evaluate(&format!(
                "(()=>{{const p={source};return p?.marker??null;}})()"
            ))?;
            ensure!(before == after, "browser_screenshot_observation_stale");
            page["screenshot"] = json!(encoded);
        }
        let observation = Observation::new(self.identity.clone(), page)?;
        self.last_observation = Some(observation.clone());
        Ok(observation)
    }
    fn fresh(&mut self, observation: &Observation, action: Option<&Action>) -> Result<bool> {
        self.validate_observation(observation)?;
        ensure!(
            observation.session == self.identity,
            "browser_observation_session_mismatch"
        );
        if let Some(action) = action {
            action.validate_against(observation)?;
            if matches!(
                action.kind,
                ActionKind::Click | ActionKind::Select | ActionKind::Fill
            ) {
                let node = action.node.context("browser_node_identity_required")?;
                let value=self.evaluate(&format!("(()=>{{const c=window.__oeBrowserSnapshot;return c?[c.pageKey(),c.guard(c.nodes.get({node}))]:null;}})()"))?;
                return Ok(value
                    == json!([
                        observation.page["page_key"],
                        observation.page["guards"][node.to_string()]
                    ]));
            }
        }
        let source = dom::snapshot(&self.source.javascript, &self.task.custom_click_selectors)?;
        Ok(self.evaluate(&format!(
            "(()=>{{const p={source};return p?.marker??null;}})()"
        ))? == observation.page["marker"])
    }
    fn act(
        &mut self,
        observation: &Observation,
        action: &Action,
        text: Option<&str>,
    ) -> Result<ExecutionReceipt> {
        self.validate_action(observation, action, text)?;
        ensure!(
            self.fresh(observation, Some(action))?,
            "browser_observation_stale"
        );
        if action.kind == ActionKind::Wait {
            std::thread::sleep(Duration::from_millis(250));
            self.last_observation = None;
            return Ok(self.receipt(
                action,
                DispatchState::Confirmed,
                Some(json!({"executed":action.id})),
                None,
            ));
        }
        // Any failure after this point is conservatively outcome-unknown,
        // including a timeout whose request may already have reached Chrome.
        let outcome = self.execute(observation, action, text);
        self.last_observation = None;
        let receipt = match outcome {
            Ok(result) if result["stale"] == true => self.receipt(
                action,
                DispatchState::NotDispatched,
                Some(result),
                Some("browser_observation_stale"),
            ),
            Ok(result) if result["unsupported"] == true => self.receipt(
                action,
                DispatchState::NotDispatched,
                Some(result),
                Some("browser_action_unsupported"),
            ),
            Ok(result) if result["executed"].as_str() == Some(&action.id) => {
                self.receipt(action, DispatchState::Confirmed, Some(result), None)
            }
            _ => self.receipt(
                action,
                DispatchState::OutcomeUnknown,
                None,
                Some("browser_action_outcome_unconfirmed"),
            ),
        };
        if receipt.state != DispatchState::NotDispatched {
            self.after_input = Some(action.clone());
        }
        Ok(receipt)
    }
    fn supplier_observation(&mut self) -> Result<Value> {
        self.ensure_supplier_origin()?;
        self.evaluate(dom::SUPPLIER)
    }
    fn account_observation(&mut self) -> Result<Value> {
        self.ensure_supplier_origin()?;
        self.evaluate_with_retention(dom::ACCOUNT, false)
    }
    fn navigate_account(&mut self) -> Result<ExecutionReceipt> {
        self.ensure_supplier_origin()?;
        self.background_guard()?;
        // Bind the current exact tab and origin before the first navigation
        // attempt. The caller owns the explicit navigation intent and journal.
        self.evaluate("location.origin")?;
        let result=self.evaluate("(()=>{location.assign('https://www.misumi.com.cn/my/user/manage/');return {navigation:'account_preflight'};})()");
        self.last_observation = None;
        let confirmed = result
            .as_ref()
            .is_ok_and(|v| v["navigation"] == "account_preflight");
        Ok(ExecutionReceipt {
            action_id: "navigate_account".into(),
            state: if confirmed {
                DispatchState::Confirmed
            } else {
                DispatchState::OutcomeUnknown
            },
            execution_kind: match self.identity.backend {
                BackendKind::PeekabooBackground => "peekaboo_background_dom",
                BackendKind::ChromeAppleEvents => "chrome_apple_events_dom",
                BackendKind::BrowserHarness => "browser_harness_cdp",
            }
            .into(),
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
        let result = self.call(
            "Target.closeTarget",
            json!({"targetId":self.identity.target}),
        )?;
        ensure!(
            result["success"] == true,
            "owned_target_cleanup_unconfirmed"
        );
        self.closed = true;
        Ok(json!({"status":"closed_owned_target","target":self.identity.target}))
    }
}

fn token(value: &Value, code: &str) -> Result<String> {
    let s = value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .context(code.to_owned())?;
    Ok(s.to_owned())
}
