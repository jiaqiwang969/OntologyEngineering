use crate::{Action, ActionKind, BackendKind};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TASK_SCHEMA: &str = "oev2.browser-task/3";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AllowedAction {
    pub kind: ActionKind,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserTask {
    pub schema: String,
    pub goal: String,
    pub url: String,
    #[serde(default = "default_backend")]
    pub backend: BackendKind,
    #[serde(default = "default_actions")]
    pub max_actions: u32,
    #[serde(default = "default_seconds")]
    pub max_seconds: u64,
    #[serde(default)]
    pub keep_open: bool,
    #[serde(default)]
    pub text_values: BTreeMap<String, String>,
    #[serde(default)]
    pub required_initial_text: Vec<String>,
    #[serde(default)]
    pub tab_id: Option<u64>,
    #[serde(default)]
    pub allowed_actions: Vec<AllowedAction>,
    #[serde(default)]
    pub custom_click_selectors: Vec<String>,
}
fn default_backend() -> BackendKind {
    BackendKind::PeekabooBackground
}
fn default_actions() -> u32 {
    30
}

fn default_seconds() -> u64 {
    120
}

pub fn http_origin(value: &str) -> Result<String> {
    ensure!(
        value.len() <= 16384 && !value.chars().any(char::is_control),
        "invalid_browser_url"
    );
    let url = url::Url::parse(value).context("invalid_browser_url")?;
    ensure!(
        ["http", "https"].contains(&url.scheme())
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "http_url_without_credentials_required"
    );
    Ok(url.origin().ascii_serialization())
}
impl BrowserTask {
    pub fn parse(value: serde_json::Value) -> Result<Self> {
        if value
            .get("backend")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("peekaboo_background")
            == "peekaboo_background"
            && !value.get("tab_id").is_some_and(|v| v.is_u64())
        {
            ensure!(
                !["allowed_actions", "custom_click_selectors"]
                    .iter()
                    .any(|k| value.get(k).is_some()),
                "explicit_background_target_required_for_existing_tab_options"
            );
        }
        if value
            .get("backend")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("peekaboo_background")
            == "browser_harness"
        {
            ensure!(
                !["tab_id", "allowed_actions", "custom_click_selectors"]
                    .iter()
                    .any(|k| value.get(k).is_some()),
                "existing_tab_options_require_apple_events_backend"
            );
        }
        let task: Self = serde_json::from_value(value).context("invalid_browser_task")?;
        task.validate()?;
        Ok(task)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == TASK_SCHEMA && !self.goal.trim().is_empty(),
            "browser_goal_and_schema_required"
        );
        http_origin(&self.url)?;

        ensure!(
            (1..=60).contains(&self.max_actions) && (1..=3600).contains(&self.max_seconds),
            "invalid_browser_budget"
        );
        ensure!(
            self.text_values.iter().all(|(k, v)| !k.trim().is_empty()
                && !v.trim().is_empty()
                && v.chars().count() <= 2000),
            "invalid_exact_field_values"
        );
        ensure!(
            self.required_initial_text
                .iter()
                .all(|v| !v.trim().is_empty()),
            "invalid_initial_observation_check"
        );
        if self.backend == BackendKind::ChromeAppleEvents
            || (self.backend == BackendKind::PeekabooBackground && self.tab_id.is_some())
        {
            ensure!(
                self.tab_id.is_some_and(|v| v > 0) && self.keep_open,
                "existing_tab_id_and_keep_open_required"
            );
            ensure!(
                (!self.allowed_actions.is_empty()
                    || self.backend == BackendKind::PeekabooBackground)
                    && self.allowed_actions.iter().all(|a| matches!(
                        a.kind,
                        ActionKind::Click | ActionKind::Fill | ActionKind::Select
                    ) && !a.label.trim().is_empty()),
                "observed_action_scope_required"
            );
            ensure!(
                self.custom_click_selectors
                    .iter()
                    .all(|v| !v.trim().is_empty()),
                "invalid_observed_control_selectors"
            );
        } else {
            ensure!(
                self.tab_id.is_none()
                    && self.allowed_actions.is_empty()
                    && self.custom_click_selectors.is_empty(),
                "existing_tab_options_require_explicit_background_target"
            );
        }
        Ok(())
    }
    pub fn permits(&self, action: &Action) -> bool {
        self.backend == BackendKind::BrowserHarness
            || (self.backend == BackendKind::PeekabooBackground && self.tab_id.is_none())
            || matches!(action.kind, ActionKind::Wait | ActionKind::Scroll)
            || self
                .allowed_actions
                .iter()
                .any(|a| a.kind == action.kind && a.label == action.label)
    }
}
