//! Observed browser identities and bounded actions; no semantic authority.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    PeekabooBackground,
    BrowserHarness,
    ChromeAppleEvents,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Click,
    Fill,
    Select,
    Scroll,
    Wait,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionIdentity {
    pub backend: BackendKind,
    pub target: String,
    pub session: String,
    pub origin: String,
    pub owns_target: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub session: SessionIdentity,
    pub page: Value,
}

/// Bind semantic marker, current controls and guards; geometry is re-resolved
/// immediately before input. A restricted action subset gets its own identity.
pub fn fingerprint_page(page: &Value) -> Result<String> {
    let actions = page["actions"]
        .as_array()
        .context("browser_actions_required")?;
    let actions: Vec<_> = actions
        .iter()
        .map(|a| {
            let mut a = a.clone();
            if let Some(a) = a.as_object_mut() {
                a.remove("rect");
            }
            a
        })
        .collect();
    Ok(oe_core::json::canonical_digest(&json!({
        "marker":page["marker"], "page_key":page["page_key"],
        "guards":page["guards"], "actions":actions
    }))?)
}

impl Observation {
    pub fn new(session: SessionIdentity, mut page: Value) -> Result<Self> {
        let hash = fingerprint_page(&page)?;
        page.as_object_mut()
            .context("browser_page_required")?
            .insert("fingerprint".into(), Value::String(hash));
        let observation = Self { session, page };
        observation.validate()?;
        Ok(observation)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.session.target.is_empty() && !self.session.session.is_empty(),
            "browser_session_identity_required"
        );
        let url = self.page["url"]
            .as_str()
            .context("browser_page_url_required")?;
        ensure!(
            crate::task::http_origin(url)? == self.session.origin,
            "browser_origin_changed"
        );
        ensure!(
            self.page["title"].is_string() && self.page["text"].is_string(),
            "browser_visible_text_required"
        );
        ensure!(
            self.page["marker"].is_array()
                && self.page["page_key"].is_array()
                && self.page["guards"].is_object(),
            "browser_observation_guards_required"
        );
        ensure!(
            self.page["fingerprint"].as_str() == Some(fingerprint_page(&self.page)?.as_str()),
            "browser_observation_fingerprint_mismatch"
        );
        let actions = self.page["actions"]
            .as_array()
            .context("browser_actions_required")?;
        ensure!(actions.len() <= 253, "browser_action_limit_exceeded");
        let mut ids = BTreeSet::new();
        for raw in actions {
            let a = Action::parse(raw.clone())?;
            ensure!(ids.insert(a.id), "duplicate_browser_action_id");
            if let Some(node) = a.node {
                ensure!(
                    self.page["guards"]
                        .get(node.to_string())
                        .is_some_and(Value::is_array),
                    "browser_action_guard_required"
                );
            }
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> &str {
        self.page["fingerprint"].as_str().unwrap_or("")
    }
    pub fn actions(&self) -> Result<Vec<Action>> {
        self.validate()?;
        self.page["actions"]
            .as_array()
            .context("browser_actions_required")?
            .iter()
            .cloned()
            .map(Action::parse)
            .collect()
    }
    pub fn restrict_actions(&self, ids: &[String]) -> Result<Self> {
        self.validate()?;
        let desired: BTreeSet<_> = ids.iter().collect();
        ensure!(desired.len() == ids.len(), "duplicate_selected_action_id");
        let mut page = self.page.clone();
        let selected: Vec<_> = self
            .actions()?
            .into_iter()
            .filter(|a| desired.contains(&a.id))
            .map(|a| a.raw)
            .collect();
        ensure!(selected.len() == ids.len(), "unobserved_selected_action");
        page["actions"] = Value::Array(selected);
        Self::new(self.session.clone(), page)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Action {
    pub id: String,
    pub kind: ActionKind,
    pub label: String,
    pub node: Option<u64>,
    pub value: Option<String>,
    pub delta: Option<i64>,
    pub raw: Value,
}
impl Action {
    fn parse(raw: Value) -> Result<Self> {
        let id = raw["id"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 128)
            .context("browser_action_id_required")?
            .to_owned();
        ensure!(
            !["DONE", "BLOCKED"].contains(&id.as_str()),
            "reserved_browser_action_id"
        );
        let kind: ActionKind =
            serde_json::from_value(raw["kind"].clone()).context("unsupported_browser_action")?;
        let label = raw["label"]
            .as_str()
            .filter(|v| !v.trim().is_empty())
            .context("browser_action_label_required")?
            .to_owned();
        let node = raw
            .get("node")
            .map(|v| {
                v.as_u64()
                    .filter(|v| *v > 0)
                    .context("browser_node_identity_required")
            })
            .transpose()?;
        let value = raw
            .get("value")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .context("invalid_browser_action_value")
            })
            .transpose()?;
        let delta = raw
            .get("delta")
            .map(|v| v.as_i64().context("invalid_browser_scroll_delta"))
            .transpose()?;
        match kind {
            ActionKind::Click | ActionKind::Fill | ActionKind::Select => {
                ensure!(node.is_some(), "browser_node_identity_required")
            }
            ActionKind::Scroll => ensure!(
                delta.is_some_and(|v| v != 0 && v.unsigned_abs() <= 2000),
                "invalid_browser_scroll_delta"
            ),
            ActionKind::Wait => {}
        }
        ensure!(
            kind != ActionKind::Select || value.is_some(),
            "observed_select_value_required"
        );
        Ok(Self {
            id,
            kind,
            label,
            node,
            value,
            delta,
            raw,
        })
    }
    pub fn from_observation(observation: &Observation, id: &str) -> Result<Self> {
        observation
            .actions()?
            .into_iter()
            .find(|a| a.id == id)
            .context("unobserved_browser_action")
    }
    pub fn validate_against(&self, observation: &Observation) -> Result<()> {
        ensure!(
            *self == Self::from_observation(observation, &self.id)?,
            "browser_action_changed"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispatchState {
    NotDispatched,
    Confirmed,
    OutcomeUnknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionReceipt {
    pub action_id: String,
    pub state: DispatchState,
    pub execution_kind: String,
    pub readback: Option<Value>,
    pub diagnostic_code: Option<String>,
}

pub trait Browser {
    fn identity(&self) -> &SessionIdentity;
    fn observe(&mut self, screenshot: bool) -> Result<Observation>;
    fn fresh(&mut self, observation: &Observation, action: Option<&Action>) -> Result<bool>;
    /// Err is reserved for definite pre-dispatch rejection. Once input may have
    /// been sent, return OutcomeUnknown on failure; callers must never replay it.
    fn act(
        &mut self,
        observation: &Observation,
        action: &Action,
        exact_text: Option<&str>,
    ) -> Result<ExecutionReceipt>;
    fn supplier_observation(&mut self) -> Result<Value>;
    fn account_observation(&mut self) -> Result<Value>;
    /// Explicit account-preflight navigation, never part of model-selected
    /// generic actions. Implementations restrict this to the fixed supplier URL.
    fn navigate_account(&mut self) -> Result<ExecutionReceipt> {
        anyhow::bail!("account_navigation_unsupported")
    }
    fn close(&mut self) -> Result<Value>;
}

#[derive(Debug, Clone)]
pub struct SnapshotSource {
    pub javascript: String,
    pub sha256: String,
}
